//! Synthetic records only. No signatures or real review evidence.
use crate::*;

pub fn fixture() -> (Manifest, Context, Receipt) {
    let artifact = Artifact {
        bucket: "source-approved".into(),
        key: "demo/video.mp4".into(),
        version_id: "source-version-1".into(),
        sha256: "a".repeat(64),
    };
    let destination = Destination {
        account: "222222222222".into(),
        region: "ap-northeast-2".into(),
        bucket: "destination-staging".into(),
        key: "demo/release-1.mp4".into(),
    };
    let manifest = Manifest {
        schema_version: 1,
        tenant: "customer-a".into(),
        release_id: "release-1".into(),
        generation: 1,
        artifact: artifact.clone(),
        destination: destination.clone(),
        recipient: "partner-a".into(),
        purpose: "robot-evaluation".into(),
        policy_version: 1,
        model_digest: "b".repeat(64),
        basis_id: "customer-approved-route-1".into(),
        expires_at: 2000,
    };
    let context = Context {
        policy: Policy {
            schema_version: 1,
            tenant: manifest.tenant.clone(),
            version: 1,
            destination: destination.clone(),
            recipient: manifest.recipient.clone(),
            purpose: manifest.purpose.clone(),
            model_digest: manifest.model_digest.clone(),
            basis_id: manifest.basis_id.clone(),
            expires_at: 2000,
            approvers: vec!["alice".into(), "bob".into()],
        },
        first: Approval {
            principal: "alice".into(),
            manifest: manifest.clone(),
            expires_at: 2000,
        },
        second: Approval {
            principal: "bob".into(),
            manifest: manifest.clone(),
            expires_at: 2000,
        },
        review: Review {
            artifact,
            model_digest: manifest.model_digest.clone(),
            passed: true,
        },
        now: 1000,
    };
    let receipt = Receipt {
        destination,
        sha256: manifest.artifact.sha256.clone(),
        version_id: "dest-version-1".into(),
    };
    (manifest, context, receipt)
}

pub fn checker_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("formal/.lake/build/bin/frith-checker")
}
