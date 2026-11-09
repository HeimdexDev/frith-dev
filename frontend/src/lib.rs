pub mod api;
mod ui;
use api::*;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use std::sync::{Arc, Mutex};
type Service = Arc<Mutex<Box<dyn FrithApi>>>;
#[derive(Default, Deserialize)]
pub struct PageQuery {
    pub org: Option<String>,
    pub tab: Option<String>,
    pub q: Option<String>,
    pub clip: Option<usize>,
}
impl PageQuery {
    fn org(&self) -> ApiResult<&str> {
        let org = self.org.as_deref().unwrap_or("heimdex");
        if ORGS.iter().any(|o| o.0 == org) {
            Ok(org)
        } else {
            Err(ApiError(403, "Unknown demo organization."))
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::from_u16(self.0).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Html(ui::error(self.1)),
        )
            .into_response()
    }
}
pub fn app() -> Router {
    app_with(Box::new(MockApi::default()))
}
pub fn app_with(api: Box<dyn FrithApi>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/datasets/new", get(new_dataset))
        .route("/datasets", post(create))
        .route("/datasets/{id}", get(detail))
        .route("/datasets/{id}/change", post(change))
        .route("/datasets/{id}/media/{clip}", get(media))
        .route("/datasets/{id}/original/{clip}", get(original))
        .route(
            "/assets/comparison.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../static/comparison.js"),
                )
            }),
        )
        .route("/datasets/{id}/download/{clip}", get(download))
        .route(
            "/assets/style.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css")],
                    include_str!("../static/style.css"),
                )
            }),
        )
        .route(
            "/assets/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../static/app.js"),
                )
            }),
        )
        .fallback(|| async { ApiError(404, "Page not found.") })
        .with_state(Arc::new(Mutex::new(api)))
}
async fn index(
    State(api): State<Service>,
    Query(q): Query<PageQuery>,
) -> Result<Html<String>, ApiError> {
    let org = q.org()?;
    let api = api.lock().unwrap();
    Ok(Html(ui::index(org, &q, api.list(org), api.activity(org))))
}
async fn new_dataset(Query(q): Query<PageQuery>) -> Result<Html<String>, ApiError> {
    Ok(Html(ui::new_dataset(q.org()?)))
}
async fn detail(
    State(api): State<Service>,
    Path(id): Path<u64>,
    Query(q): Query<PageQuery>,
) -> Result<Html<String>, ApiError> {
    let org = q.org()?;
    let d = api.lock().unwrap().get(org, id)?;
    let clip = q.clip.unwrap_or(0);
    if clip >= d.clips.len() {
        return Err(ApiError(404, "Video not found."));
    }
    Ok(Html(ui::detail(org, d, clip)))
}
async fn create(
    State(api): State<Service>,
    Query(q): Query<PageQuery>,
    Form(input): Form<CreateDataset>,
) -> Result<Redirect, ApiError> {
    let org = q.org()?;
    let id = api.lock().unwrap().create(org, input)?;
    Ok(Redirect::to(&format!("/datasets/{id}?org={org}")))
}
async fn change(
    State(api): State<Service>,
    Path(id): Path<u64>,
    Query(q): Query<PageQuery>,
    Form(input): Form<Change>,
) -> Result<Redirect, ApiError> {
    let org = q.org()?;
    let clip = input.clip.unwrap_or(0);
    api.lock().unwrap().change(org, id, input)?;
    Ok(Redirect::to(&format!(
        "/datasets/{id}?org={org}&clip={clip}"
    )))
}
async fn media(
    State(api): State<Service>,
    Path((id, clip)): Path<(u64, usize)>,
    Query(q): Query<PageQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    api.lock().unwrap().media(q.org()?, id, clip, false)?;
    Ok(video_response(
        include_bytes!("../static/sample.mp4"),
        &headers,
    ))
}

async fn download(
    State(api): State<Service>,
    Path((id, clip)): Path<(u64, usize)>,
    Query(q): Query<PageQuery>,
) -> Result<Response, ApiError> {
    api.lock().unwrap().media(q.org()?, id, clip, true)?;
    Ok((
        [
            (header::CONTENT_TYPE, "video/mp4"),
            (header::CACHE_CONTROL, "no-store"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=frith-synthetic-preview.mp4",
            ),
        ],
        include_bytes!("../static/sample.mp4").as_slice(),
    )
        .into_response())
}

async fn original(
    State(api): State<Service>,
    Path((id, clip)): Path<(u64, usize)>,
    Query(q): Query<PageQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    api.lock().unwrap().original(q.org()?, id, clip)?;
    Ok(video_response(
        include_bytes!("../static/original.mp4"),
        &headers,
    ))
}

// The small fixture is embedded. Production should delegate authorized range delivery
// to the media service and never make an original publicly cacheable.
fn video_response(bytes: &'static [u8], headers: &HeaderMap) -> Response {
    let len = bytes.len();
    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());
    let bounds = range.map(|value| -> Option<(usize, usize)> {
        let (start, end) = value.strip_prefix("bytes=")?.split_once('-')?;
        if start.is_empty() {
            let suffix: usize = end.parse().ok()?;
            return (suffix > 0).then_some((len.saturating_sub(suffix), len - 1));
        }
        let start: usize = start.parse().ok()?;
        let end = if end.is_empty() {
            len - 1
        } else {
            end.parse::<usize>().ok()?.min(len - 1)
        };
        (start <= end && start < len).then_some((start, end))
    });
    let mut response = match bounds {
        None => bytes.into_response(),
        Some(Some((start, end))) => {
            let mut response = (StatusCode::PARTIAL_CONTENT, &bytes[start..=end]).into_response();
            response.headers_mut().insert(
                header::CONTENT_RANGE,
                format!("bytes {start}-{end}/{len}").parse().unwrap(),
            );
            response
        }
        Some(None) => {
            let mut response = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
            response.headers_mut().insert(
                header::CONTENT_RANGE,
                format!("bytes */{len}").parse().unwrap(),
            );
            response
        }
    };
    for (key, value) in [
        (header::CONTENT_TYPE, "video/mp4"),
        (header::CACHE_CONTROL, "private, no-store"),
        (header::ACCEPT_RANGES, "bytes"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response.headers_mut().insert(key, value.parse().unwrap());
    }
    response
}
