use frith_web::api::*;
fn action(api: &mut MockApi, org: &str, id: u64, name: &str) -> ApiResult<Option<u64>> {
    api.act_on_request(
        org,
        id,
        RequestAction {
            revision: api.request(org, id)?.revision,
            action: name.into(),
            source_id: Some(2),
            terms: Some(
                "Two sample clips for robotics evaluation; not the full 20-clip request.".into(),
            ),
        },
    )
}
#[test]
fn agreement_handoff_does_not_grant_access_or_copy_reviews() {
    let mut api = MockApi::default();
    action(&mut api, "heimdex", 1, "offer").unwrap();
    assert!(api.get("atlas", 2).is_err());
    action(&mut api, "atlas", 1, "agree").unwrap();
    let package = action(&mut api, "heimdex", 1, "prepare").unwrap().unwrap();
    let d = api.get("heimdex", package).unwrap();
    assert_eq!(d.status, Status::Processing);
    assert!(d.grants.is_empty());
    assert!(d.clips.iter().all(|c| !c.approved));
    let origin = d.request_origin.unwrap();
    assert_eq!(origin.buyer, "atlas");
    assert_eq!(origin.request_revision, 3);
    assert_eq!(origin.offer.source_id, 2);
    assert_eq!(origin.version, 1);
    assert!(api.get("atlas", package).is_err());
    assert!(api.media("atlas", package, 0, false).is_err());
    assert!(api.original("atlas", 2, 0).is_err());
    let before = api.list("heimdex").len();
    assert!(action(&mut api, "heimdex", 1, "prepare").is_err());
    assert_eq!(api.list("heimdex").len(), before);
    let mut req = api.request("atlas", 1).unwrap();
    req.requirements.task = "Changed scope".into();
    assert!(
        api.save_request("atlas", Some((1, req.revision)), req.requirements)
            .is_err()
    );
    for (name, clip) in [("finish", None), ("approve", Some(0)), ("approve", Some(1))] {
        api.change(
            "heimdex",
            package,
            Change {
                revision: api.get("heimdex", package).unwrap().revision,
                action: name.into(),
                clip,
                recipient: None,
                permission: None,
            },
        )
        .unwrap();
    }
    let share = |revision, recipient: &str| Change {
        revision,
        action: "share".into(),
        clip: None,
        recipient: Some(recipient.into()),
        permission: Some("preview".into()),
    };
    let revision = api.get("heimdex", package).unwrap().revision;
    assert!(
        api.change("heimdex", package, share(revision, "northstar"))
            .is_err()
    );
    api.change("heimdex", package, share(revision, "atlas"))
        .unwrap();
    assert!(api.media("atlas", package, 0, false).is_ok());
    assert!(api.media("atlas", package, 0, true).is_err());
    assert!(api.original("atlas", package, 0).is_err());
    api.change(
        "heimdex",
        package,
        Change {
            revision: revision + 1,
            action: "revoke".into(),
            clip: None,
            recipient: Some("atlas".into()),
            permission: None,
        },
    )
    .unwrap();
    assert!(api.media("atlas", package, 0, false).is_err());
}
#[test]
fn role_checks_stale_updates_and_invalid_sources_leave_state_unchanged() {
    let mut api = MockApi::default();
    assert!(api.requests("northstar").is_empty());
    assert_eq!(api.request("northstar", 1).unwrap_err().0, 404);
    assert!(action(&mut api, "atlas", 1, "offer").is_err());
    assert!(action(&mut api, "heimdex", 1, "agree").is_err());
    assert!(action(&mut api, "heimdex", 1, "prepare").is_err());
    for source in [4, 999] {
        assert!(
            api.act_on_request(
                "heimdex",
                1,
                RequestAction {
                    revision: 1,
                    action: "offer".into(),
                    source_id: Some(source),
                    terms: Some("Terms".into())
                }
            )
            .is_err()
        );
    }
    assert_eq!(api.request("atlas", 1).unwrap().revision, 1);
    action(&mut api, "heimdex", 1, "offer").unwrap();
    assert_eq!(
        api.act_on_request(
            "atlas",
            1,
            RequestAction {
                revision: 1,
                action: "agree".into(),
                source_id: None,
                terms: None
            }
        )
        .unwrap_err()
        .0,
        409
    );
    assert!(!api.request("atlas", 1).unwrap().agreed);
    assert!(action(&mut api, "atlas", 1, "prepare").is_err());
}
#[test]
fn editing_requirements_and_replacing_offers_require_fresh_agreement() {
    let mut api = MockApi::default();
    action(&mut api, "heimdex", 1, "offer").unwrap();
    action(&mut api, "atlas", 1, "agree").unwrap();
    action(&mut api, "heimdex", 1, "offer").unwrap();
    assert!(!api.request("atlas", 1).unwrap().agreed);
    action(&mut api, "atlas", 1, "agree").unwrap();
    let r = api.request("atlas", 1).unwrap();
    assert!(
        api.save_request("heimdex", Some((1, r.revision)), r.requirements.clone())
            .is_err()
    );
    api.save_request("atlas", Some((1, r.revision)), r.requirements)
        .unwrap();
    let r = api.request("atlas", 1).unwrap();
    assert!(r.offer.is_none());
    assert!(!r.agreed);
    assert!(action(&mut api, "heimdex", 1, "prepare").is_err());
}
#[test]
fn source_changes_block_handoff_and_validation_is_atomic() {
    let mut api = MockApi::default();
    action(&mut api, "heimdex", 1, "offer").unwrap();
    action(&mut api, "atlas", 1, "agree").unwrap();
    api.change(
        "heimdex",
        2,
        Change {
            revision: 1,
            action: "approve".into(),
            clip: Some(0),
            recipient: None,
            permission: None,
        },
    )
    .unwrap();
    assert_eq!(
        action(&mut api, "heimdex", 1, "prepare").unwrap_err().0,
        409
    );
    assert!(api.request("atlas", 1).unwrap().package_id.is_none());
    let r = api.request("atlas", 1).unwrap();
    for input in [
        Requirements {
            title: " ".into(),
            ..r.requirements.clone()
        },
        Requirements {
            seller: "atlas".into(),
            ..r.requirements.clone()
        },
        Requirements {
            action_data: "invented".into(),
            ..r.requirements.clone()
        },
    ] {
        assert_eq!(api.save_request("atlas", None, input).unwrap_err().0, 422);
    }
    assert_eq!(api.requests("atlas").len(), 1);
    let id = api.save_request("atlas", None, r.requirements).unwrap();
    assert_eq!(id, 2);
    assert_eq!(api.request("atlas", id).unwrap().revision, 1);
}
