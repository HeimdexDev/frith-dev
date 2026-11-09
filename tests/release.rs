use frith_release::*;
use std::time::Duration;

fn checker() -> LeanChecker {
    LeanChecker::new(demo::checker_path(), Duration::from_secs(2))
        .expect("Build the real Lean checker with scripts/verify.sh first")
}
fn pending(m: Manifest) -> State {
    State {
        manifest: m,
        stage: Stage::Pending,
        revision: 0,
        receipt: None,
    }
}
fn cmd(action: Action, revision: u64) -> Command {
    Command {
        action,
        expected_revision: revision,
        receipt: None,
    }
}
fn copied(checker: &LeanChecker, context: &Context) -> State {
    let (m, _, receipt) = demo::fixture();
    let state = checker
        .transition(&pending(m), &cmd(Action::Authorize, 0), context)
        .unwrap();
    checker
        .transition(
            &state,
            &Command {
                receipt: Some(receipt),
                ..cmd(Action::Complete, 1)
            },
            context,
        )
        .unwrap()
}

#[test]
fn happy_path_and_terminal_revocation_use_the_real_checker() {
    let checker = checker();
    let (_, context, _) = demo::fixture();
    let state = copied(&checker, &context);
    let published = checker
        .transition(&state, &cmd(Action::Publish, 2), &context)
        .unwrap();
    assert_eq!(published.stage, Stage::Published);
    assert_eq!(published.manifest, state.manifest);
    let revoked = checker
        .transition(&published, &cmd(Action::Revoke, 3), &context)
        .unwrap();
    for action in [Action::Authorize, Action::Publish, Action::Revoke] {
        assert_eq!(
            checker.transition(&revoked, &cmd(action, 4), &context),
            Err(Error::Denied)
        );
    }
}

#[test]
fn each_policy_requirement_is_enforced() {
    type Mutation = (&'static str, fn(&mut Manifest, &mut Context));
    let cases: &[Mutation] = &[
        ("policy schema", |_, c| c.policy.schema_version = 2),
        ("tenant", |_, c| c.policy.tenant = "other".into()),
        ("policy version", |_, c| c.policy.version += 1),
        ("destination account", |_, c| {
            c.policy.destination.account = "other".into()
        }),
        ("destination region", |_, c| {
            c.policy.destination.region = "other".into()
        }),
        ("destination bucket", |_, c| {
            c.policy.destination.bucket = "other".into()
        }),
        ("destination key", |_, c| {
            c.policy.destination.key = "other".into()
        }),
        ("recipient", |_, c| c.policy.recipient = "other".into()),
        ("purpose", |_, c| c.policy.purpose = "other".into()),
        ("model", |_, c| c.policy.model_digest = "c".repeat(64)),
        ("basis", |_, c| c.policy.basis_id = "other".into()),
        ("manifest expiry", |m, c| {
            m.expires_at = c.now;
            c.first.manifest = m.clone();
            c.second.manifest = m.clone();
        }),
        ("policy expiry", |_, c| c.policy.expires_at = c.now),
        ("first expiry", |_, c| c.first.expires_at = c.now),
        ("second expiry", |_, c| c.second.expires_at = c.now),
        ("duplicate signer", |_, c| {
            c.second.principal = c.first.principal.clone()
        }),
        ("empty first signer even if allowlisted", |_, c| {
            c.first.principal.clear();
            c.policy.approvers.push(String::new());
        }),
        ("empty second signer even if allowlisted", |_, c| {
            c.second.principal.clear();
            c.policy.approvers.push(String::new());
        }),
        ("first unauthorized", |_, c| {
            c.first.principal = "mallory".into()
        }),
        ("second unauthorized", |_, c| {
            c.second.principal = "mallory".into()
        }),
        ("first swapped signature payload", |_, c| {
            c.first.manifest.destination.key = "other".into()
        }),
        ("second swapped signature payload", |_, c| {
            c.second.manifest.artifact.sha256 = "c".repeat(64)
        }),
        ("review failed", |_, c| c.review.passed = false),
        ("review wrong version", |_, c| {
            c.review.artifact.version_id = "other".into()
        }),
        ("review wrong model", |_, c| {
            c.review.model_digest = "c".repeat(64)
        }),
        ("release replay", |m, _| {
            m.release_id = "different-release".into()
        }),
        ("generation replay", |m, _| m.generation += 1),
        ("wrong approved digest", |m, _| {
            m.artifact.sha256 = "c".repeat(64)
        }),
    ];
    let checker = checker();
    for (name, mutate) in cases {
        let (mut m, mut c, _) = demo::fixture();
        mutate(&mut m, &mut c);
        assert_eq!(
            checker.transition(&pending(m), &cmd(Action::Authorize, 0), &c),
            Err(Error::Denied),
            "{name}"
        );
    }
}

#[test]
fn publication_rechecks_policy_and_expiry() {
    let checker = checker();
    let (_, mut c, _) = demo::fixture();
    let state = copied(&checker, &c);
    c.policy.version += 1;
    assert_eq!(
        checker.transition(&state, &cmd(Action::Publish, 2), &c),
        Err(Error::Denied)
    );
    c.policy.version -= 1;
    c.now = 2000;
    assert_eq!(
        checker.transition(&state, &cmd(Action::Publish, 2), &c),
        Err(Error::Denied)
    );
    // Emergency revocation still works with expired approvals and policy.
    assert_eq!(
        checker
            .transition(&state, &cmd(Action::Revoke, 2), &c)
            .unwrap()
            .stage,
        Stage::Revoked
    );
}

#[test]
fn late_completion_cannot_revive_revoked_generation() {
    let checker = checker();
    let (m, c, receipt) = demo::fixture();
    let authorized = checker
        .transition(&pending(m), &cmd(Action::Authorize, 0), &c)
        .unwrap();
    let revoked = checker
        .transition(&authorized, &cmd(Action::Revoke, 1), &c)
        .unwrap();
    let late = Command {
        receipt: Some(receipt),
        ..cmd(Action::Complete, 1)
    };
    assert_eq!(
        checker.transition(&revoked, &late, &c),
        Err(Error::Conflict)
    );
    // Even supplying the current revision cannot bypass terminal revocation.
    assert_eq!(
        checker.transition(
            &revoked,
            &Command {
                expected_revision: 2,
                ..late
            },
            &c
        ),
        Err(Error::Denied)
    );
}

#[test]
fn no_skip_or_duplicate_transitions_and_receipts_bind_exact_destination() {
    let checker = checker();
    let (m, c, receipt) = demo::fixture();
    let state = pending(m);
    assert_eq!(
        checker.transition(&state, &cmd(Action::Publish, 0), &c),
        Err(Error::Denied)
    );
    let completion = Command {
        receipt: Some(receipt.clone()),
        ..cmd(Action::Complete, 0)
    };
    assert_eq!(
        checker.transition(&state, &completion, &c),
        Err(Error::Denied)
    );
    let authorized = checker
        .transition(&state, &cmd(Action::Authorize, 0), &c)
        .unwrap();
    assert_eq!(
        checker.transition(&authorized, &cmd(Action::Authorize, 1), &c),
        Err(Error::Denied)
    );
    for field in 0..6 {
        let mut r = receipt.clone();
        match field {
            0 => r.destination.account = "other".into(),
            1 => r.destination.region = "other".into(),
            2 => r.destination.bucket = "other".into(),
            3 => r.destination.key = "other".into(),
            4 => r.sha256 = "c".repeat(64),
            _ => r.version_id.clear(),
        }
        assert_eq!(
            checker.transition(
                &authorized,
                &Command {
                    receipt: Some(r),
                    ..cmd(Action::Complete, 1)
                },
                &c
            ),
            Err(Error::Denied)
        );
    }
    let copied = checker
        .transition(
            &authorized,
            &Command {
                receipt: Some(receipt.clone()),
                ..cmd(Action::Complete, 1)
            },
            &c,
        )
        .unwrap();
    assert_eq!(
        checker.transition(
            &copied,
            &Command {
                receipt: Some(receipt),
                ..cmd(Action::Complete, 2)
            },
            &c
        ),
        Err(Error::Denied)
    );
}

#[test]
fn tenant_roles_immutable_registration_and_failures_preserve_state() {
    let checker = checker();
    let (m, mut c, _) = demo::fixture();
    let operator = Actor {
        tenant: m.tenant.clone(),
        role: Role::Operator,
    };
    let foreign = Actor {
        tenant: "customer-b".into(),
        role: Role::Operator,
    };
    let security = Actor {
        tenant: m.tenant.clone(),
        role: Role::SecurityOfficer,
    };
    let mut store = Coordinator::default();
    assert_eq!(store.register(&foreign, m.clone()), Err(Error::Denied));
    let key = store.register(&operator, m.clone()).unwrap();
    assert_eq!(store.register(&operator, m), Err(Error::AlreadyExists));
    assert_eq!(store.get(&foreign, &key), Err(Error::Denied));
    assert_eq!(
        store.apply(&checker, &foreign, &key, cmd(Action::Authorize, 0), &c),
        Err(Error::Denied)
    );
    assert_eq!(
        store.apply(&checker, &operator, &key, cmd(Action::Revoke, 0), &c),
        Err(Error::Denied)
    );
    c.review.passed = false;
    assert_eq!(
        store.apply(&checker, &operator, &key, cmd(Action::Authorize, 0), &c),
        Err(Error::Denied)
    );
    assert_eq!(store.get(&operator, &key).unwrap().revision, 0);
    store
        .apply(&checker, &security, &key, cmd(Action::Revoke, 0), &c)
        .unwrap();
    assert_eq!(store.get(&operator, &key).unwrap().stage, Stage::Revoked);
}

#[test]
fn strict_manifest_decoding_rejects_ambiguous_and_unsupported_inputs() {
    let (m, _, _) = demo::fixture();
    let bytes = serde_json::to_vec(&m).unwrap();
    assert_eq!(decode_manifest(&bytes).unwrap(), m);
    let json = String::from_utf8(bytes).unwrap();
    let duplicate = json.replacen('{', "{\"tenant\":\"attacker\",", 1);
    let unknown = json.replacen('{', "{\"unknown\":true,", 1);
    let cases = [
        duplicate,
        unknown,
        format!("{json}{{}}"),
        json.replace("\"schemaVersion\":1", "\"schemaVersion\":2"),
        json.replace("\"generation\":1", "\"generation\":-1"),
        json.replace("\"generation\":1", "\"generation\":18446744073709551616"),
        json.replace(&"a".repeat(64), "invalid-digest"),
    ];
    for input in cases {
        assert_eq!(decode_manifest(input.as_bytes()), Err(Error::InvalidInput));
    }
    assert_eq!(
        decode_manifest(&vec![b' '; 65_537]),
        Err(Error::InvalidInput)
    );
}

#[test]
fn overflow_and_unexpected_receipts_fail_closed() {
    let checker = checker();
    let (m, c, r) = demo::fixture();
    let mut state = pending(m);
    assert_eq!(
        checker.transition(
            &state,
            &Command {
                receipt: Some(r),
                ..cmd(Action::Authorize, 0)
            },
            &c
        ),
        Err(Error::InvalidInput)
    );
    state.revision = u64::MAX;
    assert_eq!(
        checker.transition(&state, &cmd(Action::Revoke, u64::MAX), &c),
        Err(Error::InvalidInput)
    );
}

#[cfg(unix)]
#[test]
fn checker_failure_timeout_and_malformed_output_never_commit() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("checker");
    let cases = [
        ("#!/bin/sh\nexit 1\n", Error::CheckerUnavailable),
        ("#!/bin/sh\nexec /bin/sleep 5\n", Error::CheckerUnavailable),
        ("#!/bin/sh\nprintf 'true'\n", Error::InvalidCheckerOutput),
        (
            "#!/bin/sh\nprintf 'null trailing'\n",
            Error::InvalidCheckerOutput,
        ),
    ];
    for (script, expected) in cases {
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let checker = LeanChecker::new(&path, Duration::from_millis(100)).unwrap();
        let (m, c, _) = demo::fixture();
        let actor = Actor {
            tenant: m.tenant.clone(),
            role: Role::Operator,
        };
        let mut store = Coordinator::default();
        let key = store.register(&actor, m).unwrap();
        assert_eq!(
            store.apply(&checker, &actor, &key, cmd(Action::Authorize, 0), &c),
            Err(expected)
        );
        assert_eq!(store.get(&actor, &key).unwrap().revision, 0);
    }
}
