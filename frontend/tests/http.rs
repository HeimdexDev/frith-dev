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
    assert!(html.contains("Data requests"));
    assert!(html.contains("Incoming requests"));
    assert!(!html.contains("New data request"));
    let (_, _, html) = request(app.clone(), "GET", "/?tab=files", "").await;
    assert!(html.contains("Safe sharing"));
    assert!(html.contains("Seoul"));
    let (_, _, html) = request(app.clone(), "GET", "/?tab=files&q=zzzz", "").await;
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

#[tokio::test]
async fn request_forms_preserve_input_and_handoff_is_separate_from_access() {
    let app = frith_web::app();
    let fields = "title=Box+task&task=Pick+an+object&criteria=Hands+visible&volume=20+clips&purpose=Evaluation&destination=Germany&seller=heimdex&action_data=any&perspective=&deadline=";
    let (status, _, html) = request(
        app.clone(),
        "POST",
        "/requests?org=atlas",
        &fields.replace("title=Box+task", "title="),
    )
    .await;
    assert_eq!(status, 422);
    assert!(html.contains("Pick an object"));
    assert!(html.contains("role=\"alert\""));
    let (status, headers, _) = request(app.clone(), "POST", "/requests?org=atlas", fields).await;
    assert_eq!(status, 303);
    assert_eq!(headers["location"], "/requests/2?org=atlas");
    assert_eq!(
        request(app.clone(), "GET", "/requests/2?org=northstar", "")
            .await
            .0,
        404
    );
    assert_eq!(
        request(app.clone(), "GET", "/requests/2/edit?org=heimdex", "")
            .await
            .0,
        403
    );
    let edit = format!("revision=1&{fields}")
        .replace("title=Box+task", "title=%3Cscript%3Ex%3C%2Fscript%3E");
    assert_eq!(
        request(app.clone(), "POST", "/requests/2/edit?org=atlas", &edit)
            .await
            .0,
        303
    );
    let (_, _, html) = request(app.clone(), "GET", "/requests/2?org=atlas", "").await;
    assert!(html.contains("&lt;script&gt;"));
    assert!(!html.contains("<script>x</script>"));
    assert!(!html.contains("name=\"source_id\""));
    assert_eq!(
        request(
            app.clone(),
            "POST",
            "/requests/2/change?org=heimdex",
            "revision=2&action=offer&source_id=2&terms=Two+sample+clips+for+evaluation"
        )
        .await
        .0,
        303
    );
    let (_, _, html) = request(app.clone(), "GET", "/requests/2?org=atlas", "").await;
    assert!(html.contains("Agree to proposed scope"));
    assert!(!html.contains("/original/"));
    assert!(!html.contains("/media/"));
    assert_eq!(
        request(
            app.clone(),
            "POST",
            "/requests/2/change?org=atlas",
            "revision=3&action=agree"
        )
        .await
        .0,
        303
    );
    let (status, headers, _) = request(
        app.clone(),
        "POST",
        "/requests/2/change?org=heimdex",
        "revision=4&action=prepare",
    )
    .await;
    assert_eq!(status, 303);
    assert_eq!(headers["location"], "/datasets/6?org=heimdex");
    assert_eq!(
        request(app.clone(), "GET", "/datasets/6?org=atlas", "")
            .await
            .0,
        404
    );
    assert_eq!(
        request(app.clone(), "GET", "/datasets/6/media/0?org=atlas", "")
            .await
            .0,
        404
    );
    let (_, _, html) = request(app.clone(), "GET", "/requests/2?org=atlas", "").await;
    assert!(html.contains("Waiting for the seller to finish review"));
    assert!(!html.contains("href=\"/datasets/6"));
    let (_, _, html) = request(app.clone(), "GET", "/datasets/6?org=heimdex", "").await;
    assert!(html.contains("Request #2"));
    assert!(html.contains("Finish processing"));
    assert!(!html.contains("Share dataset"));
    for body in [
        "revision=1&action=finish",
        "revision=2&action=approve&clip=0",
        "revision=3&action=approve&clip=1",
    ] {
        assert_eq!(
            request(app.clone(), "POST", "/datasets/6/change?org=heimdex", body)
                .await
                .0,
            303
        );
    }
    let (_, _, html) = request(app.clone(), "GET", "/datasets/6?org=heimdex", "").await;
    assert!(html.contains("Agreed request scope"));
    assert!(html.contains("Declared destination"));
    assert!(html.contains("Germany"));
    assert!(html.contains("Evaluation"));
    assert!(html.contains("not verified identity, location or export approval"));
    assert_eq!(
        request(
            app,
            "POST",
            "/requests/2/change?org=heimdex",
            "revision=4&action=prepare"
        )
        .await
        .0,
        409
    );
}

#[tokio::test]
async fn buying_and_supplying_show_distinct_requests_and_actions() {
    let app = frith_web::app();
    let body = "title=Heimdex+buyer+request&task=Pick&criteria=Visible&volume=2&purpose=Evaluation&destination=Korea&seller=atlas&action_data=any&perspective=&deadline=";
    assert_eq!(
        request(app.clone(), "POST", "/requests?org=heimdex", body)
            .await
            .0,
        303
    );
    let (_, _, supply) = request(
        app.clone(),
        "GET",
        "/?org=heimdex&tab=requests&view=supplying",
        "",
    )
    .await;
    assert!(supply.contains("Incoming requests"));
    assert!(supply.contains("Pick and place into a box"));
    assert!(supply.contains("Review request"));
    assert!(!supply.contains("Heimdex buyer request"));
    assert!(!supply.contains("New data request"));
    assert!(!supply.contains("Buying and supplying"));
    let (_, _, buy) = request(app.clone(), "GET", "/requests?org=heimdex&view=buying", "").await;
    assert!(buy.contains("My requests"));
    assert!(buy.contains("Heimdex buyer request"));
    assert!(!buy.contains("Pick and place into a box"));
    assert!(buy.contains("New data request"));
    assert!(buy.contains("tab=files&amp;view=buying") || buy.contains("tab=files&view=buying"));
    assert!(buy.contains("name=\"view\" value=\"buying\""));
    let (_, _, empty) = request(
        app.clone(),
        "GET",
        "/requests?org=northstar&view=supplying",
        "",
    )
    .await;
    assert!(empty.contains("No incoming requests found"));
    assert!(!empty.contains("Create your first request"));
    assert_eq!(request(app, "GET", "/?view=administrator", "").await.0, 400);
}

#[tokio::test]
async fn sharing_views_and_review_queue_remain_scoped() {
    let app = frith_web::app();
    let (_, _, buy) = request(
        app.clone(),
        "GET",
        "/?org=heimdex&tab=files&view=buying",
        "",
    )
    .await;
    assert!(buy.contains("Received datasets"));
    assert!(buy.contains("Mobile manipulation"));
    assert!(!buy.contains("Seoul · warehouse floor"));
    assert!(!buy.contains("New sharing package"));
    assert!(!buy.contains("Package workflow"));
    let (_, _, supply) = request(
        app.clone(),
        "GET",
        "/?org=heimdex&tab=files&view=supplying",
        "",
    )
    .await;
    assert!(supply.contains("Outgoing packages"));
    assert!(supply.contains("New sharing package"));
    assert!(!supply.contains("Mobile manipulation"));
    let (_, _, review) = request(
        app.clone(),
        "GET",
        "/?org=heimdex&tab=files&view=supplying&stage=review",
        "",
    )
    .await;
    assert!(review.contains("Picking &amp; placement"));
    assert!(!review.contains("Seoul · warehouse floor"));
    assert!(!review.contains("Evening shift"));
    assert!(!review.contains("Loading bay"));
    assert!(review.contains("name=\"stage\" value=\"review\""));
    let (_, _, failed) = request(
        app.clone(),
        "GET",
        "/?org=heimdex&tab=files&view=supplying&stage=failed",
        "",
    )
    .await;
    assert!(failed.contains("Loading bay"));
    assert!(!failed.contains("Evening shift"));
    let (_, _, legacy) = request(app, "GET", "/?org=heimdex&tab=received", "").await;
    assert!(legacy.contains("Received datasets"));
    assert!(legacy.contains("Mobile manipulation"));
}

#[tokio::test]
async fn task_view_never_changes_authority_and_details_use_actual_role() {
    let app = frith_web::app();
    let (_, _, buyer) = request(
        app.clone(),
        "GET",
        "/requests/1?org=atlas&view=supplying",
        "",
    )
    .await;
    assert!(buyer.contains("Atlas Robotics / Buying"));
    assert!(!buyer.contains("name=\"source_id\""));
    assert_eq!(
        request(
            app.clone(),
            "POST",
            "/requests/1/change?org=atlas&view=supplying",
            "revision=1&action=offer&source_id=2&terms=Forged"
        )
        .await
        .0,
        403
    );
    assert_eq!(
        request(
            app.clone(),
            "GET",
            "/datasets/2/original/0?org=atlas&view=supplying",
            ""
        )
        .await
        .0,
        404
    );
    let (_, _, owner) = request(
        app.clone(),
        "GET",
        "/datasets/2?org=heimdex&view=buying",
        "",
    )
    .await;
    assert!(owner.contains("Heimdex / Supplying"));
    assert!(owner.contains("Preparation workflow"));
    assert!(owner.contains("2. Review faces"));
    let (_, _, receiver) = request(app, "GET", "/datasets/1?org=atlas&view=supplying", "").await;
    assert!(receiver.contains("Atlas Robotics / Buying"));
    assert!(!receiver.contains("Preparation workflow"));
    assert!(!receiver.contains("Compare original"));
    assert!(!receiver.contains("Share dataset"));
}
