//! Replace this adapter with authenticated backend calls; never trust browser organization IDs.
use serde::{Deserialize, Serialize};

pub const ORGS: [(&str, &str, &str); 3] = [
    ("heimdex", "Heimdex", "South Korea"),
    ("atlas", "Atlas Robotics", "Germany"),
    ("northstar", "Northstar Labs", "United States"),
];
pub fn org_name(id: &str) -> &str {
    ORGS.iter()
        .find(|o| o.0 == id)
        .map(|o| o.1)
        .unwrap_or("Unknown organization")
}
pub fn jurisdiction(id: &str) -> &str {
    ORGS.iter()
        .find(|o| o.0 == id)
        .map(|o| o.2)
        .unwrap_or("Unknown")
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Processing,
    Failed,
    Review,
    Ready,
}
impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Processing => "Processing faces",
            Self::Failed => "Processing failed",
            Self::Review => "Needs review",
            Self::Ready => "Ready to share",
        }
    }
    pub fn class(self) -> &'static str {
        match self {
            Self::Processing => "processing",
            Self::Failed => "failed",
            Self::Review => "review",
            Self::Ready => "ready",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clip {
    pub name: String,
    pub approved: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grant {
    pub org: String,
    pub download: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dataset {
    pub id: u64,
    pub revision: u64,
    pub name: String,
    pub owner: String,
    pub status: Status,
    pub clips: Vec<Clip>,
    pub grants: Vec<Grant>,
}
impl Dataset {
    pub fn visible(&self, org: &str) -> bool {
        self.owner == org
            || (self.status == Status::Ready && self.grants.iter().any(|g| g.org == org))
    }
}
#[derive(Clone, Debug)]
pub struct Event {
    pub org: String,
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError(pub u16, pub &'static str);
pub type ApiResult<T> = Result<T, ApiError>;
#[derive(Clone, Deserialize)]
pub struct CreateDataset {
    pub name: String,
    pub files: String,
}
#[derive(Clone, Deserialize)]
pub struct Change {
    pub revision: u64,
    pub action: String,
    pub clip: Option<usize>,
    pub recipient: Option<String>,
    pub permission: Option<String>,
}
/// UI-facing service boundary. The mock is synchronous; a network adapter should make these async.
pub trait FrithApi: Send + Sync {
    fn list(&self, org: &str) -> Vec<Dataset>;
    fn get(&self, org: &str, id: u64) -> ApiResult<Dataset>;
    fn create(&mut self, org: &str, input: CreateDataset) -> ApiResult<u64>;
    fn change(&mut self, org: &str, id: u64, input: Change) -> ApiResult<()>;
    fn media(&self, org: &str, id: u64, clip: usize, download: bool) -> ApiResult<()>;
    /// Original review media requires ownership in addition to normal preview readiness.
    fn original(&self, org: &str, id: u64, clip: usize) -> ApiResult<()> {
        let dataset = self.get(org, id)?;
        if dataset.owner != org {
            return Err(ApiError(
                403,
                "Original videos are only available to the owner.",
            ));
        }
        self.media(org, id, clip, false)
    }
    fn activity(&self, org: &str) -> Vec<Event>;
}
pub struct MockApi {
    datasets: Vec<Dataset>,
    events: Vec<Event>,
    next_id: u64,
}
impl Default for MockApi {
    fn default() -> Self {
        let dataset = |id, name: &str, owner: &str, status, count, grants| Dataset {
            id,
            revision: 1,
            name: name.into(),
            owner: owner.into(),
            status,
            clips: (1..=count)
                .map(|i| Clip {
                    name: format!("sequence_{i:02}.mp4"),
                    approved: status == Status::Ready,
                })
                .collect(),
            grants,
        };
        Self {
            datasets: vec![
                dataset(
                    1,
                    "Seoul · warehouse floor",
                    "heimdex",
                    Status::Ready,
                    3,
                    vec![Grant {
                        org: "atlas".into(),
                        download: false,
                    }],
                ),
                dataset(
                    2,
                    "Picking & placement",
                    "heimdex",
                    Status::Review,
                    2,
                    vec![],
                ),
                dataset(3, "Evening shift", "heimdex", Status::Processing, 4, vec![]),
                dataset(
                    4,
                    "Mobile manipulation",
                    "atlas",
                    Status::Ready,
                    2,
                    vec![Grant {
                        org: "heimdex".into(),
                        download: true,
                    }],
                ),
                dataset(5, "Loading bay", "heimdex", Status::Failed, 1, vec![]),
            ],
            events: vec![Event {
                org: "heimdex".into(),
                text: "Seoul · warehouse floor shared with Atlas Robotics · Preview only".into(),
            }],
            next_id: 6,
        }
    }
}
impl FrithApi for MockApi {
    fn list(&self, org: &str) -> Vec<Dataset> {
        self.datasets
            .iter()
            .filter(|d| d.visible(org))
            .cloned()
            .collect()
    }
    fn get(&self, org: &str, id: u64) -> ApiResult<Dataset> {
        self.datasets
            .iter()
            .find(|d| d.id == id && d.visible(org))
            .cloned()
            .ok_or(ApiError(404, "Dataset not found or no longer shared."))
    }
    fn create(&mut self, org: &str, input: CreateDataset) -> ApiResult<u64> {
        let name = input.name.trim();
        let files: Vec<_> = input
            .files
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if name.is_empty() || name.chars().count() > 100 {
            return Err(ApiError(422, "Use a dataset name of 1–100 characters."));
        }
        if files.is_empty()
            || files.len() > 50
            || files.iter().any(|f| {
                f.len() > 255
                    || ![".mp4", ".mov", ".webm"]
                        .iter()
                        .any(|ext| f.to_lowercase().ends_with(ext))
            })
        {
            return Err(ApiError(422, "Choose 1–50 videos (MP4, MOV or WebM)."));
        }
        if !ORGS.iter().any(|o| o.0 == org) {
            return Err(ApiError(403, "Unknown organization."));
        }
        let id = self.next_id;
        self.next_id += 1;
        self.datasets.push(Dataset {
            id,
            revision: 1,
            name: name.into(),
            owner: org.into(),
            status: Status::Processing,
            clips: files
                .iter()
                .map(|f| Clip {
                    name: f.to_string(),
                    approved: false,
                })
                .collect(),
            grants: vec![],
        });
        self.events.push(Event {
            org: org.into(),
            text: format!("Created {name} · Face processing in {}", jurisdiction(org)),
        });
        Ok(id)
    }
    fn change(&mut self, org: &str, id: u64, input: Change) -> ApiResult<()> {
        let d = self
            .datasets
            .iter_mut()
            .find(|d| d.id == id && d.owner == org)
            .ok_or(ApiError(
                403,
                "Only the dataset owner can make this change.",
            ))?;
        if d.revision != input.revision {
            return Err(ApiError(
                409,
                "This dataset changed. Go back and refresh before trying again.",
            ));
        }
        let event = match input.action.as_str() {
            "finish" if d.status == Status::Processing => {
                d.status = Status::Review;
                "Mock processing completed".to_string()
            }
            "fail" if d.status == Status::Processing => {
                d.status = Status::Failed;
                "Mock processing failed".to_string()
            }
            "retry" if d.status == Status::Failed => {
                d.status = Status::Processing;
                "Face processing restarted".to_string()
            }
            "approve" if d.status == Status::Review => {
                let c = input
                    .clip
                    .and_then(|i| d.clips.get_mut(i))
                    .ok_or(ApiError(422, "Choose a valid video."))?;
                c.approved = true;
                if d.clips.iter().all(|c| c.approved) {
                    d.status = Status::Ready;
                }
                "Face review approved".to_string()
            }
            "reject" if d.status == Status::Review => {
                d.status = Status::Failed;
                for c in &mut d.clips {
                    c.approved = false;
                }
                "Face review rejected · Reprocessing required".to_string()
            }
            "share" if d.status == Status::Ready => {
                let recipient = input
                    .recipient
                    .ok_or(ApiError(422, "Choose an organization."))?;
                if recipient == org || !ORGS.iter().any(|o| o.0 == recipient) {
                    return Err(ApiError(422, "Choose another invited organization."));
                }
                let download = match input.permission.as_deref() {
                    Some("preview") => false,
                    Some("download") => true,
                    _ => return Err(ApiError(422, "Choose a valid permission.")),
                };
                d.grants.retain(|g| g.org != recipient);
                d.grants.push(Grant {
                    org: recipient.clone(),
                    download,
                });
                self.events.push(Event {
                    org: recipient.clone(),
                    text: format!("{} shared {} with your organization", org_name(org), d.name),
                });
                format!(
                    "Shared with {} · {}",
                    org_name(&recipient),
                    if download {
                        "Preview + download"
                    } else {
                        "Preview only"
                    }
                )
            }
            "revoke" => {
                let recipient = input
                    .recipient
                    .ok_or(ApiError(422, "Choose an organization."))?;
                if !d.grants.iter().any(|g| g.org == recipient) {
                    return Err(ApiError(404, "Share not found."));
                }
                d.grants.retain(|g| g.org != recipient);
                self.events.push(Event {
                    org: recipient.clone(),
                    text: format!("Access revoked · {}", d.name),
                });
                format!("Revoked access for {}", org_name(&recipient))
            }
            _ => {
                return Err(ApiError(
                    409,
                    "This action is not available in the current state.",
                ));
            }
        };
        d.revision += 1;
        self.events.push(Event {
            org: org.into(),
            text: format!("{} · {event}", d.name),
        });
        Ok(())
    }
    fn media(&self, org: &str, id: u64, clip: usize, download: bool) -> ApiResult<()> {
        let d = self.get(org, id)?;
        if clip >= d.clips.len() {
            return Err(ApiError(404, "Video not found."));
        }
        if !matches!(d.status, Status::Review | Status::Ready) {
            return Err(ApiError(409, "The anonymized preview is not ready."));
        }
        if download
            && (d.status != Status::Ready
                || (d.owner != org && !d.grants.iter().any(|g| g.org == org && g.download)))
        {
            return Err(ApiError(403, "Download permission is required."));
        }
        Ok(())
    }
    fn activity(&self, org: &str) -> Vec<Event> {
        self.events
            .iter()
            .rev()
            .filter(|e| e.org == org)
            .cloned()
            .collect()
    }
}
