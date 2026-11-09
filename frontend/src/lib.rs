pub mod api;
pub mod processing;
mod request_ui;
mod ui;
use api::*;
use axum::{
    Extension, Form, Router,
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
    pub view: Option<WorkspaceView>,
    pub stage: Option<PackageStage>,
}
/// Presentation context only; ownership and grants remain the authorization boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceView {
    Buying,
    Supplying,
}
impl WorkspaceView {
    pub fn key(self) -> &'static str {
        match self {
            Self::Buying => "buying",
            Self::Supplying => "supplying",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Buying => "Buying",
            Self::Supplying => "Supplying",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageStage {
    #[default]
    All,
    Processing,
    Review,
    Ready,
    Failed,
    Shared,
}
impl PackageStage {
    pub fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Processing => "processing",
            Self::Review => "review",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Shared => "shared",
        }
    }
}
impl PageQuery {
    pub fn workspace_view(&self, org: &str) -> WorkspaceView {
        self.view.unwrap_or_else(|| match self.tab.as_deref() {
            Some("received") => WorkspaceView::Buying,
            Some("shared") => WorkspaceView::Supplying,
            _ if org == "heimdex" => WorkspaceView::Supplying,
            _ => WorkspaceView::Buying,
        })
    }
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
    app_with_gpu(api, None)
}
pub fn app_with_gpu(api: Box<dyn FrithApi>, config: Option<processing::GpuConfig>) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/", get(index))
        .route("/requests", get(request_list).post(create_request))
        .route("/requests/new", get(new_request))
        .route("/requests/{id}", get(request_detail))
        .route(
            "/requests/{id}/edit",
            get(edit_request).post(update_request),
        )
        .route("/requests/{id}/change", post(change_request))
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
        .layer(Extension(processing::Processing::new(config)))
        .with_state(Arc::new(Mutex::new(api)))
}
async fn index(
    State(api): State<Service>,
    Query(q): Query<PageQuery>,
) -> Result<Html<String>, ApiError> {
    let org = q.org()?;
    let api = api.lock().unwrap();
    if q.tab.as_deref().is_none_or(|tab| tab == "requests") {
        Ok(Html(request_ui::list(org, &q, api.requests(org))))
    } else {
        Ok(Html(ui::index(org, &q, api.list(org), api.activity(org))))
    }
}
async fn new_dataset(Query(q): Query<PageQuery>) -> Result<Html<String>, ApiError> {
    Ok(Html(ui::new_dataset(q.org()?)))
}
async fn detail(
    Extension(gpu): Extension<processing::Processing>,
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
    Ok(Html(ui::detail(
        org,
        d,
        clip,
        gpu.config.is_some(),
        gpu.running(id),
        gpu.has_job(id),
    )))
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
    Extension(gpu): Extension<processing::Processing>,
    State(api): State<Service>,
    Path(id): Path<u64>,
    Query(q): Query<PageQuery>,
    Form(input): Form<Change>,
) -> Result<Redirect, ApiError> {
    let org = q.org()?;
    let clip = input.clip.unwrap_or(0);
    if input.action == "process" {
        gpu.start(api.clone(), org, id, input.revision)?;
    } else {
        if input.action.starts_with("gpu_")
            || (gpu.config.is_some() && ["finish", "fail"].contains(&input.action.as_str()))
        {
            return Err(ApiError(403, "Only the GPU job can complete processing."));
        }
        api.lock().unwrap().change(org, id, input)?;
    }
    Ok(Redirect::to(&format!(
        "/datasets/{id}?org={org}&clip={clip}"
    )))
}
async fn media(
    Extension(gpu): Extension<processing::Processing>,
    State(api): State<Service>,
    Path((id, clip)): Path<(u64, usize)>,
    Query(q): Query<PageQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    api.lock().unwrap().media(q.org()?, id, clip, false)?;
    if let Some(bytes) = gpu.video(id, false)? {
        return Ok(video_response(&bytes, &headers));
    }
    Ok(video_response(
        include_bytes!("../static/sample.mp4"),
        &headers,
    ))
}

async fn download(
    Extension(gpu): Extension<processing::Processing>,
    State(api): State<Service>,
    Path((id, clip)): Path<(u64, usize)>,
    Query(q): Query<PageQuery>,
) -> Result<Response, ApiError> {
    api.lock().unwrap().media(q.org()?, id, clip, true)?;
    let bytes = gpu
        .video(id, false)?
        .map(|b| (*b).clone())
        .unwrap_or_else(|| include_bytes!("../static/sample.mp4").to_vec());
    Ok((
        [
            (header::CONTENT_TYPE, "video/mp4"),
            (header::CACHE_CONTROL, "no-store"),
            (
                header::CONTENT_DISPOSITION,
                if gpu.has_job(id) {
                    "attachment; filename=frith-gpu-demo.mp4"
                } else {
                    "attachment; filename=frith-synthetic-preview.mp4"
                },
            ),
        ],
        bytes,
    )
        .into_response())
}

async fn original(
    Extension(gpu): Extension<processing::Processing>,
    State(api): State<Service>,
    Path((id, clip)): Path<(u64, usize)>,
    Query(q): Query<PageQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    api.lock().unwrap().original(q.org()?, id, clip)?;
    if let Some(bytes) = gpu.video(id, true)? {
        return Ok(video_response(&bytes, &headers));
    }
    Ok(video_response(
        include_bytes!("../static/original.mp4"),
        &headers,
    ))
}

// The small fixture is embedded. Production should delegate authorized range delivery
// to the media service and never make an original publicly cacheable.
fn video_response(bytes: &[u8], headers: &HeaderMap) -> Response {
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
        None => bytes.to_vec().into_response(),
        Some(Some((start, end))) => {
            let mut response =
                (StatusCode::PARTIAL_CONTENT, bytes[start..=end].to_vec()).into_response();
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

async fn request_list(
    State(api): State<Service>,
    Query(q): Query<PageQuery>,
) -> Result<Html<String>, ApiError> {
    let org = q.org()?;
    Ok(Html(request_ui::list(
        org,
        &q,
        api.lock().unwrap().requests(org),
    )))
}
async fn new_request(Query(q): Query<PageQuery>) -> Result<Html<String>, ApiError> {
    Ok(Html(request_ui::form(
        q.org()?,
        None,
        Requirements {
            action_data: "any".into(),
            ..Default::default()
        },
        None,
    )))
}
async fn request_detail(
    State(api): State<Service>,
    Path(id): Path<u64>,
    Query(q): Query<PageQuery>,
) -> Result<Html<String>, ApiError> {
    let org = q.org()?;
    let api = api.lock().unwrap();
    let r = api.request(org, id)?;
    let sources = if org == r.requirements.seller {
        api.list(org)
            .into_iter()
            .filter(|d| d.owner == org && d.request_origin.is_none())
            .collect()
    } else {
        vec![]
    };
    let package_visible = r.package_id.is_some_and(|id| api.get(org, id).is_ok());
    Ok(Html(request_ui::detail(org, r, sources, package_visible)))
}
async fn edit_request(
    State(api): State<Service>,
    Path(id): Path<u64>,
    Query(q): Query<PageQuery>,
) -> Result<Html<String>, ApiError> {
    let org = q.org()?;
    let r = api.lock().unwrap().request(org, id)?;
    if r.buyer != org {
        return Err(ApiError(403, "Only the buyer can edit requirements."));
    }
    if r.package_id.is_some() {
        return Err(ApiError(
            409,
            "Prepared package scope cannot be edited. Create a new request.",
        ));
    }
    Ok(Html(request_ui::form(
        org,
        Some((id, r.revision)),
        r.requirements,
        None,
    )))
}
async fn create_request(
    State(api): State<Service>,
    Query(q): Query<PageQuery>,
    Form(input): Form<Requirements>,
) -> Result<Response, ApiError> {
    let org = q.org()?;
    let result = api.lock().unwrap().save_request(org, None, input.clone());
    Ok(match result {
        Ok(id) => Redirect::to(&format!("/requests/{id}?org={org}")).into_response(),
        Err(e) => (
            StatusCode::from_u16(e.0).unwrap(),
            Html(request_ui::form(org, None, input, Some(e.1))),
        )
            .into_response(),
    })
}
#[derive(Deserialize)]
struct EditRequest {
    revision: u64,
    #[serde(flatten)]
    requirements: Requirements,
}
async fn update_request(
    State(api): State<Service>,
    Path(id): Path<u64>,
    Query(q): Query<PageQuery>,
    Form(input): Form<EditRequest>,
) -> Result<Response, ApiError> {
    let org = q.org()?;
    let result = api.lock().unwrap().save_request(
        org,
        Some((id, input.revision)),
        input.requirements.clone(),
    );
    Ok(match result {
        Ok(id) => Redirect::to(&format!("/requests/{id}?org={org}")).into_response(),
        Err(e) => (
            StatusCode::from_u16(e.0).unwrap(),
            Html(request_ui::form(
                org,
                Some((id, input.revision)),
                input.requirements,
                Some(e.1),
            )),
        )
            .into_response(),
    })
}
async fn change_request(
    State(api): State<Service>,
    Path(id): Path<u64>,
    Query(q): Query<PageQuery>,
    Form(input): Form<RequestAction>,
) -> Result<Redirect, ApiError> {
    let org = q.org()?;
    let package = api.lock().unwrap().act_on_request(org, id, input)?;
    Ok(Redirect::to(&match package {
        Some(package) => format!("/datasets/{package}?org={org}"),
        None => format!("/requests/{id}?org={org}"),
    }))
}
