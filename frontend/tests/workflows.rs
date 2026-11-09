use frith_web::api::*;
fn change(api: &mut MockApi, id: u64, action: &str, clip: Option<usize>) -> ApiResult<()> {
    api.change(
        "heimdex",
        id,
        Change {
            revision: api.get("heimdex", id)?.revision,
            action: action.into(),
            clip,
            recipient: None,
            permission: None,
        },
    )
}
fn share(api: &mut MockApi, id: u64, permission: &str) -> ApiResult<()> {
    api.change(
        "heimdex",
        id,
        Change {
            revision: api.get("heimdex", id)?.revision,
            action: "share".into(),
            clip: None,
            recipient: Some("atlas".into()),
            permission: Some(permission.into()),
        },
    )
}
#[test]
fn create_process_review_share_and_revoke() {
    let mut api = MockApi::default();
    let id = api
        .create(
            "heimdex",
            CreateDataset {
                name: "New collection".into(),
                files: "a.mp4\nb.mov".into(),
            },
        )
        .unwrap();
    assert_eq!(api.get("heimdex", id).unwrap().status, Status::Processing);
    assert!(api.media("heimdex", id, 0, false).is_err());
    assert!(share(&mut api, id, "preview").is_err());
    change(&mut api, id, "finish", None).unwrap();
    assert!(api.media("heimdex", id, 0, false).is_ok());
    assert!(api.get("atlas", id).is_err());
    change(&mut api, id, "approve", Some(0)).unwrap();
    assert_eq!(api.get("heimdex", id).unwrap().status, Status::Review);
    assert!(share(&mut api, id, "download").is_err());
    change(&mut api, id, "approve", Some(1)).unwrap();
    share(&mut api, id, "preview").unwrap();
    assert!(api.media("atlas", id, 0, false).is_ok());
    assert_eq!(
        api.media("atlas", id, 0, true),
        Err(ApiError(403, "Download permission is required."))
    );
    share(&mut api, id, "download").unwrap();
    assert!(api.media("atlas", id, 0, true).is_ok());
    api.change(
        "heimdex",
        id,
        Change {
            revision: api.get("heimdex", id).unwrap().revision,
            action: "revoke".into(),
            clip: None,
            recipient: Some("atlas".into()),
            permission: None,
        },
    )
    .unwrap();
    assert!(api.get("atlas", id).is_err());
    assert!(api.media("atlas", id, 0, true).is_err());
    assert!(!api.list("atlas").iter().any(|d| d.id == id));
    assert!(
        api.activity("atlas")
            .iter()
            .any(|e| e.text.contains("revoked"))
    );
}
#[test]
fn rejected_review_resets_all_approvals() {
    let mut api = MockApi::default();
    change(&mut api, 2, "approve", Some(0)).unwrap();
    change(&mut api, 2, "reject", None).unwrap();
    let d = api.get("heimdex", 2).unwrap();
    assert_eq!(d.status, Status::Failed);
    assert!(d.clips.iter().all(|c| !c.approved));
    change(&mut api, 2, "retry", None).unwrap();
    change(&mut api, 2, "finish", None).unwrap();
    assert_eq!(api.get("heimdex", 2).unwrap().status, Status::Review);
}
#[test]
fn invalid_and_stale_changes_are_atomic() {
    let mut api = MockApi::default();
    let mut input = Change {
        revision: 0,
        action: "share".into(),
        clip: None,
        recipient: Some("atlas".into()),
        permission: Some("preview".into()),
    };
    assert_eq!(api.change("heimdex", 1, input.clone()).unwrap_err().0, 409);
    input.revision = 1;
    assert_eq!(api.change("atlas", 1, input.clone()).unwrap_err().0, 403);
    input.recipient = Some("unknown".into());
    assert_eq!(api.change("heimdex", 1, input.clone()).unwrap_err().0, 422);
    input.recipient = Some("heimdex".into());
    assert_eq!(api.change("heimdex", 1, input.clone()).unwrap_err().0, 422);
    input.recipient = Some("atlas".into());
    input.permission = Some("admin".into());
    assert_eq!(api.change("heimdex", 1, input).unwrap_err().0, 422);
    assert_eq!(api.get("heimdex", 1).unwrap().revision, 1);
    assert!(!api.get("heimdex", 1).unwrap().grants[0].download);
    assert!(api.media("northstar", 1, 0, false).is_err());
    assert!(api.media("atlas", 1, 99, false).is_err());
}
#[test]
fn validates_new_datasets_and_scopes_lists() {
    let mut api = MockApi::default();
    for (name, files) in [("", "a.mp4"), ("Dataset", ""), ("Dataset", "secret.txt")] {
        assert!(
            api.create(
                "heimdex",
                CreateDataset {
                    name: name.into(),
                    files: files.into()
                }
            )
            .is_err()
        );
    }
    assert!(api.list("northstar").is_empty());
    assert_eq!(api.list("atlas").len(), 2);
    assert!(!api.list("atlas").iter().any(|d| d.status != Status::Ready));
}
