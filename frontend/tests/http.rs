use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;
async fn request(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: &str,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}
#[tokio::test]
async fn renders_navigation_search_and_receiver_controls() {
    let app = frith_web::app();
    let (status, _, html) = request(app.clone(), "GET", "/", "").await;
    assert_eq!(status, 200);
    assert!(html.contains("My files"));
    assert!(html.contains("Seoul"));
    let (_, _, html) = request(app.clone(), "GET", "/?q=zzzz", "").await;
    assert!(html.contains("No datasets here yet"));
    let (_, _, html) = request(app.clone(), "GET", "/datasets/1?org=atlas", "").await;
    assert!(html.contains("Preview only"));
    assert!(!html.contains("Share dataset"));
    assert!(!html.contains("Download video"));
    assert_eq!(
        request(app.clone(), "GET", "/datasets/1/download/0?org=atlas", "")
            .await
            .0,
        403
    );
    assert_eq!(
        request(app.clone(), "GET", "/datasets/1/media/0?org=northstar", "")
            .await
            .0,
        404
    );
    assert_eq!(
        request(app.clone(), "GET", "/?org=unknown", "").await.0,
        403
    );
    assert_eq!(request(app, "GET", "/datasets/2?clip=99", "").await.0, 404);
}
#[tokio::test]
async fn form_workflow_and_escaping() {
    let app = frith_web::app();
    let (status, headers, _) = request(
        app.clone(),
        "POST",
        "/datasets?org=heimdex",
        "name=%3Cscript%3Ealert%281%29%3C%2Fscript%3E&files=a.mp4",
    )
    .await;
    assert_eq!(status, 303);
    assert_eq!(headers["location"], "/datasets/6?org=heimdex");
    let (_, _, html) = request(app.clone(), "GET", "/datasets/6", "").await;
    assert!(html.contains("&lt;script&gt;"));
    assert!(!html.contains("<script>alert"));
    assert_eq!(
        request(
            app.clone(),
            "POST",
            "/datasets/6/change",
            "revision=1&action=finish"
        )
        .await
        .0,
        303
    );
    assert_eq!(
        request(
            app.clone(),
            "POST",
            "/datasets/6/change",
            "revision=1&action=approve&clip=0"
        )
        .await
        .0,
        409
    );
    assert_eq!(
        request(
            app.clone(),
            "POST",
            "/datasets/6/change",
            "revision=2&action=approve&clip=0"
        )
        .await
        .0,
        303
    );
    assert_eq!(
        request(
            app.clone(),
            "POST",
            "/datasets/6/change",
            "revision=3&action=share&recipient=atlas&permission=download"
        )
        .await
        .0,
        303
    );
    let (status, headers, _) =
        request(app.clone(), "GET", "/datasets/6/download/0?org=atlas", "").await;
    assert_eq!(status, 200);
    assert!(
        headers["content-disposition"]
            .to_str()
            .unwrap()
            .contains("attachment")
    );
    assert_eq!(headers["cache-control"], "no-store");
    assert_eq!(
        request(
            app.clone(),
            "POST",
            "/datasets/6/change?org=atlas",
            "revision=4&action=revoke&recipient=atlas"
        )
        .await
        .0,
        403
    );
    assert_eq!(
        request(
            app.clone(),
            "POST",
            "/datasets/6/change",
            "revision=4&action=revoke&recipient=atlas"
        )
        .await
        .0,
        303
    );
    assert_eq!(
        request(app, "GET", "/datasets/6/media/0?org=atlas", "")
            .await
            .0,
        404
    );
}

#[tokio::test]
async fn original_review_media_is_owner_only_and_not_preloaded() {
    let app = frith_web::app();
    let (status, _, html) = request(app.clone(), "GET", "/datasets/2?org=heimdex", "").await;
    assert_eq!(status, 200);
    assert!(html.contains("Compare original"));
    assert!(html.contains(r#"data-source="/datasets/2/original/0?org=heimdex""#));
    assert!(!html.contains(r#"src="/datasets/2/original/"#));
    let (status, headers, _) =
        request(app.clone(), "GET", "/datasets/2/original/0?org=heimdex", "").await;
    assert_eq!(status, 200);
    assert_eq!(headers["cache-control"], "private, no-store");
    for org in ["atlas", "northstar"] {
        let (_, _, html) = request(app.clone(), "GET", &format!("/datasets/1?org={org}"), "").await;
        assert!(!html.contains("Compare original"));
        assert!(!html.contains("/original/"));
        assert!(!html.contains("comparison.js"));
        let status = request(
            app.clone(),
            "GET",
            &format!("/datasets/1/original/0?org={org}"),
            "",
        )
        .await
        .0;
        assert!(status == 403 || status == 404);
    }
    assert_eq!(
        request(app.clone(), "GET", "/datasets/3/original/0", "")
            .await
            .0,
        409
    );
    assert_eq!(
        request(app.clone(), "GET", "/datasets/5/original/0", "")
            .await
            .0,
        409
    );
    assert_eq!(
        request(app, "GET", "/datasets/2/original/99", "").await.0,
        404
    );
}

#[tokio::test]
async fn media_ranges_support_seeking_without_bypassing_owner_checks() {
    let app = frith_web::app();
    for path in ["media", "original"] {
        for (range, status, size) in [
            ("bytes=0-1", 206, Some(2)),
            ("bytes=-16", 206, Some(16)),
            ("bytes=999999999-", 416, None),
            ("bytes=3-1", 416, None),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/datasets/2/{path}/0?org=heimdex"))
                        .header("Range", range)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), status);
            assert_eq!(response.headers()["accept-ranges"], "bytes");
            assert_eq!(response.headers()["cache-control"], "private, no-store");
            assert!(response.headers().contains_key("content-range"));
            if let Some(size) = size {
                assert_eq!(
                    response
                        .into_body()
                        .collect()
                        .await
                        .unwrap()
                        .to_bytes()
                        .len(),
                    size
                );
            }
        }
    }
    let response = app
        .oneshot(
            Request::builder()
                .uri("/datasets/1/original/0?org=atlas")
                .header("Range", "bytes=0-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}
