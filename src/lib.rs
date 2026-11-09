//! Local reference coordinator. All policy decisions execute in the proved Lean core.
//! Actor identities, context, and copy receipts MUST come from trusted adapters.
//! No network authentication, cryptography, persistence, or cloud enforcement is provided.

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command as Process, Stdio},
    thread,
    time::{Duration, Instant},
};

const MAX_WIRE_BYTES: usize = 65_536;

macro_rules! record {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        pub struct $name { $(pub $field: $ty),* }
    };
}

record!(Artifact {
    bucket: String,
    key: String,
    version_id: String,
    sha256: String
});
record!(Destination {
    account: String,
    region: String,
    bucket: String,
    key: String
});
record!(Manifest {
    schema_version: u64,
    tenant: String,
    release_id: String,
    generation: u64,
    artifact: Artifact,
    destination: Destination,
    recipient: String,
    purpose: String,
    policy_version: u64,
    model_digest: String,
    basis_id: String,
    expires_at: u64,
});
record!(Policy {
    schema_version: u64, tenant: String, version: u64, destination: Destination,
    recipient: String, purpose: String, model_digest: String, basis_id: String,
    expires_at: u64, approvers: Vec<String>,
});
record!(Approval {
    principal: String,
    manifest: Manifest,
    expires_at: u64
});
record!(Review {
    artifact: Artifact,
    model_digest: String,
    passed: bool
});

/// Authenticated evidence plus authoritative policy and time, not client assertions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Context {
    pub policy: Policy,
    pub first: Approval,
    pub second: Approval,
    pub review: Review,
    pub now: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Pending,
    Authorized,
    Copied,
    Published,
    Revoked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Authorize,
    Complete,
    Publish,
    Revoke,
}

record!(Receipt {
    destination: Destination,
    sha256: String,
    version_id: String
});
record!(State { manifest: Manifest, stage: Stage, revision: u64, receipt: Option<Receipt> });
record!(Command { action: Action, expected_revision: u64, receipt: Option<Receipt> });

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Input<'a> {
    protocol_version: u64,
    state: &'a State,
    command: &'a Command,
    context: &'a Context,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    Denied,
    Conflict,
    NotFound,
    AlreadyExists,
    CheckerUnavailable,
    InvalidCheckerOutput,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn nonempty(s: &str) -> bool {
    !s.trim().is_empty() && !s.chars().any(char::is_control)
}

impl Manifest {
    pub fn validate(&self) -> Result<(), Error> {
        let strings = [
            &self.tenant,
            &self.release_id,
            &self.artifact.bucket,
            &self.artifact.key,
            &self.artifact.version_id,
            &self.destination.account,
            &self.destination.region,
            &self.destination.bucket,
            &self.destination.key,
            &self.recipient,
            &self.purpose,
            &self.basis_id,
        ];
        if self.schema_version != 1
            || self.generation == 0
            || self.policy_version == 0
            || !digest(&self.artifact.sha256)
            || !digest(&self.model_digest)
            || strings.iter().any(|s| !nonempty(s))
        {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }
}

/// Strict external manifest decoder. Canonical signing is deliberately not implemented.
pub fn decode_manifest(bytes: &[u8]) -> Result<Manifest, Error> {
    if bytes.len() > MAX_WIRE_BYTES {
        return Err(Error::InvalidInput);
    }
    let m: Manifest = serde_json::from_slice(bytes).map_err(|_| Error::InvalidInput)?;
    m.validate()?;
    Ok(m)
}

/// The configured executable is trusted deployment material, never a user-supplied path.
pub struct LeanChecker {
    executable: PathBuf,
    timeout: Duration,
}
impl LeanChecker {
    pub fn new(path: impl AsRef<Path>, timeout: Duration) -> Result<Self, Error> {
        if timeout.is_zero() {
            return Err(Error::InvalidInput);
        }
        let executable = path
            .as_ref()
            .canonicalize()
            .map_err(|_| Error::CheckerUnavailable)?;
        if !executable.is_file() {
            return Err(Error::CheckerUnavailable);
        }
        Ok(Self {
            executable,
            timeout,
        })
    }

    pub fn transition(
        &self,
        state: &State,
        command: &Command,
        context: &Context,
    ) -> Result<State, Error> {
        state.manifest.validate()?;
        if command.expected_revision != state.revision {
            return Err(Error::Conflict);
        }
        let next_revision = state.revision.checked_add(1).ok_or(Error::InvalidInput)?;
        // A receipt is meaningful only for completion; do not silently ignore extra input.
        if (command.action == Action::Complete) != command.receipt.is_some() {
            return Err(Error::InvalidInput);
        }
        let mut bytes = serde_json::to_vec(&Input {
            protocol_version: 1,
            state,
            command,
            context,
        })
        .map_err(|_| Error::InvalidInput)?;
        bytes.push(b'\n');
        if bytes.len() > MAX_WIRE_BYTES {
            return Err(Error::InvalidInput);
        }

        // Files avoid pipe-buffer deadlocks. Anonymous temporary files are private and removed on close.
        let unavailable = |_| Error::CheckerUnavailable;
        let mut input = tempfile::tempfile().map_err(unavailable)?;
        input.write_all(&bytes).map_err(unavailable)?;
        input.seek(SeekFrom::Start(0)).map_err(unavailable)?;
        let mut output = tempfile::tempfile().map_err(unavailable)?;
        let mut child = Process::new(&self.executable)
            .env_clear()
            .stdin(Stdio::from(input))
            .stdout(Stdio::from(output.try_clone().map_err(unavailable)?))
            .stderr(Stdio::null())
            .spawn()
            .map_err(unavailable)?;
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        return Err(Error::CheckerUnavailable);
                    }
                    break;
                }
                Ok(None) if started.elapsed() < self.timeout => {
                    thread::sleep(Duration::from_millis(5))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error::CheckerUnavailable);
                }
            }
        }
        if output.metadata().map_err(unavailable)?.len() > MAX_WIRE_BYTES as u64 {
            return Err(Error::InvalidCheckerOutput);
        }
        output.seek(SeekFrom::Start(0)).map_err(unavailable)?;
        let mut response = Vec::new();
        output
            .take((MAX_WIRE_BYTES + 1) as u64)
            .read_to_end(&mut response)
            .map_err(unavailable)?;
        if response.len() > MAX_WIRE_BYTES {
            return Err(Error::InvalidCheckerOutput);
        }
        let next: Option<State> =
            serde_json::from_slice(&response).map_err(|_| Error::InvalidCheckerOutput)?;
        let next = next.ok_or(Error::Denied)?;
        let expected_stage = match command.action {
            Action::Authorize => Stage::Authorized,
            Action::Complete => Stage::Copied,
            Action::Publish => Stage::Published,
            Action::Revoke => Stage::Revoked,
        };
        // Protocol checks, not a second implementation of export policy.
        if next.manifest != state.manifest
            || next.revision != next_revision
            || next.stage != expected_stage
        {
            return Err(Error::InvalidCheckerOutput);
        }
        Ok(next)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReleaseKey {
    pub tenant: String,
    pub release_id: String,
    pub generation: u64,
}
impl From<&Manifest> for ReleaseKey {
    fn from(m: &Manifest) -> Self {
        Self {
            tenant: m.tenant.clone(),
            release_id: m.release_id.clone(),
            generation: m.generation,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Operator,
    SecurityOfficer,
}

/// Construct only after authentication; this is not an HTTP request DTO.
pub struct Actor {
    pub tenant: String,
    pub role: Role,
}

/// Single-process, in-memory reference store. Exclusive &mut access serializes commits.
/// It must not be used to claim durable revocation or distributed linearizability.
#[derive(Default)]
pub struct Coordinator {
    releases: HashMap<ReleaseKey, State>,
}
impl Coordinator {
    pub fn register(&mut self, actor: &Actor, manifest: Manifest) -> Result<ReleaseKey, Error> {
        manifest.validate()?;
        if actor.tenant != manifest.tenant || actor.role != Role::Operator {
            return Err(Error::Denied);
        }
        let key = ReleaseKey::from(&manifest);
        if self.releases.contains_key(&key) {
            return Err(Error::AlreadyExists);
        }
        self.releases.insert(
            key.clone(),
            State {
                manifest,
                stage: Stage::Pending,
                revision: 0,
                receipt: None,
            },
        );
        Ok(key)
    }

    pub fn get(&self, actor: &Actor, key: &ReleaseKey) -> Result<&State, Error> {
        if actor.tenant != key.tenant {
            return Err(Error::Denied);
        }
        self.releases.get(key).ok_or(Error::NotFound)
    }

    pub fn apply(
        &mut self,
        checker: &LeanChecker,
        actor: &Actor,
        key: &ReleaseKey,
        command: Command,
        context: &Context,
    ) -> Result<State, Error> {
        let required = if command.action == Action::Revoke {
            Role::SecurityOfficer
        } else {
            Role::Operator
        };
        if actor.role != required {
            return Err(Error::Denied);
        }
        let current = self.get(actor, key)?;
        let next = checker.transition(current, &command, context)?;
        self.releases.insert(key.clone(), next.clone());
        Ok(next)
    }
}

/// Synthetic fixture for examples and integration tests. Never use as real approval evidence.
pub mod demo;
