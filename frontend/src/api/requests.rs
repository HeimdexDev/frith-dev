use super::*;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Requirements {
    pub title: String,
    pub task: String,
    pub criteria: String,
    pub volume: String,
    pub perspective: String,
    pub action_data: String,
    pub purpose: String,
    pub destination: String,
    pub deadline: String,
    pub seller: String,
}
impl Requirements {
    fn validate(&mut self, buyer: &str) -> ApiResult<()> {
        for value in [
            &mut self.title,
            &mut self.task,
            &mut self.criteria,
            &mut self.volume,
            &mut self.perspective,
            &mut self.purpose,
            &mut self.destination,
            &mut self.deadline,
        ] {
            *value = value.trim().to_string();
            if value.chars().count() > 2000 {
                return Err(ApiError(422, "Keep each field under 2,000 characters."));
            }
        }
        if self.title.is_empty()
            || self.title.chars().count() > 100
            || self.task.is_empty()
            || self.criteria.is_empty()
            || self.volume.is_empty()
            || self.purpose.is_empty()
            || self.destination.is_empty()
        {
            return Err(ApiError(
                422,
                "Add a title (up to 100 characters), task, acceptance criteria, volume, purpose and destination.",
            ));
        }
        if !["measured", "estimated", "none", "any"].contains(&self.action_data.as_str()) {
            return Err(ApiError(422, "Choose an action-data requirement."));
        }
        if self.seller == buyer || !ORGS.iter().any(|o| o.0 == self.seller) {
            return Err(ApiError(
                422,
                "Choose another invited organization as the seller.",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Offer {
    pub source_id: u64,
    pub source_revision: u64,
    pub name: String,
    pub clip_count: usize,
    pub terms: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataRequest {
    pub id: u64,
    pub revision: u64,
    pub buyer: String,
    pub requirements: Requirements,
    pub offer: Option<Offer>,
    pub agreed: bool,
    pub package_id: Option<u64>,
}
impl DataRequest {
    pub fn status(&self) -> &'static str {
        if self.package_id.is_some() {
            "In safe sharing"
        } else if self.agreed {
            "Scope agreed"
        } else if self.offer.is_some() {
            "Awaiting buyer agreement"
        } else {
            "Finding matches"
        }
    }
    pub fn next_action(&self) -> &'static str {
        if self.package_id.is_some() {
            "Seller reviews and grants access"
        } else if self.agreed {
            "Seller prepares sharing package"
        } else if self.offer.is_some() {
            "Buyer reviews proposed scope"
        } else {
            "Seller proposes footage"
        }
    }
}
/// Frozen request scope for a package; dataset revision still tracks review/access mutations.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestOrigin {
    pub request_id: u64,
    pub request_revision: u64,
    pub buyer: String,
    pub requirements: Requirements,
    pub offer: Offer,
    pub version: u64,
}
#[derive(Clone, Deserialize)]
pub struct RequestAction {
    pub revision: u64,
    pub action: String,
    pub source_id: Option<u64>,
    pub terms: Option<String>,
}
pub trait RequestApi {
    fn requests(&self, org: &str) -> Vec<DataRequest>;
    fn request(&self, org: &str, id: u64) -> ApiResult<DataRequest>;
    fn save_request(
        &mut self,
        org: &str,
        edit: Option<(u64, u64)>,
        input: Requirements,
    ) -> ApiResult<u64>;
    fn act_on_request(
        &mut self,
        org: &str,
        id: u64,
        input: RequestAction,
    ) -> ApiResult<Option<u64>>;
}
impl RequestApi for MockApi {
    fn requests(&self, org: &str) -> Vec<DataRequest> {
        self.requests
            .iter()
            .filter(|r| r.buyer == org || r.requirements.seller == org)
            .cloned()
            .collect()
    }
    fn request(&self, org: &str, id: u64) -> ApiResult<DataRequest> {
        self.requests
            .iter()
            .find(|r| r.id == id && (r.buyer == org || r.requirements.seller == org))
            .cloned()
            .ok_or(ApiError(404, "Request not found."))
    }
    fn save_request(
        &mut self,
        org: &str,
        edit: Option<(u64, u64)>,
        mut input: Requirements,
    ) -> ApiResult<u64> {
        if !ORGS.iter().any(|o| o.0 == org) {
            return Err(ApiError(403, "Unknown organization."));
        }
        input.validate(org)?;
        let id = if let Some((id, revision)) = edit {
            let r = self
                .requests
                .iter_mut()
                .find(|r| r.id == id && r.buyer == org)
                .ok_or(ApiError(403, "Only the buyer can edit requirements."))?;
            if r.revision != revision || r.package_id.is_some() {
                return Err(ApiError(
                    409,
                    "Refresh this request. Prepared package scope cannot be edited.",
                ));
            }
            r.requirements = input;
            r.offer = None;
            r.agreed = false;
            r.revision += 1;
            id
        } else {
            let id = self.next_request_id;
            self.next_request_id += 1;
            self.requests.push(DataRequest {
                id,
                revision: 1,
                buyer: org.into(),
                requirements: input,
                offer: None,
                agreed: false,
                package_id: None,
            });
            id
        };
        self.events.push(Event {
            org: org.into(),
            text: format!("Request #{id} · Requirements saved; matching and agreement required"),
        });
        Ok(id)
    }
    fn act_on_request(
        &mut self,
        org: &str,
        id: u64,
        input: RequestAction,
    ) -> ApiResult<Option<u64>> {
        let r = self.request(org, id)?;
        if r.revision != input.revision || r.package_id.is_some() {
            return Err(ApiError(
                409,
                "This request changed or already has a package. Refresh before continuing.",
            ));
        }
        let mut updated = r.clone();
        let mut package = None;
        let event = match input.action.as_str() {
            "offer" if r.requirements.seller == org => {
                let source = self
                    .datasets
                    .iter()
                    .find(|d| {
                        Some(d.id) == input.source_id
                            && d.owner == org
                            && d.request_origin.is_none()
                    })
                    .ok_or(ApiError(422, "Choose footage owned by your organization."))?;
                let terms = input.terms.unwrap_or_default().trim().to_string();
                if terms.is_empty() || terms.chars().count() > 2000 {
                    return Err(ApiError(
                        422,
                        "Describe the proposed scope and usage terms (up to 2,000 characters).",
                    ));
                }
                updated.offer = Some(Offer {
                    source_id: source.id,
                    source_revision: source.revision,
                    name: source.name.clone(),
                    clip_count: source.clips.len(),
                    terms,
                });
                updated.agreed = false;
                "Seller proposed footage · Buyer agreement required"
            }
            "agree" if r.buyer == org && r.offer.is_some() && !r.agreed => {
                updated.agreed = true;
                "Buyer agreed to scope · No media access granted"
            }
            "prepare" if r.requirements.seller == org && r.agreed => {
                let offer = r
                    .offer
                    .clone()
                    .ok_or(ApiError(409, "Propose footage before preparation."))?;
                let source = self
                    .datasets
                    .iter()
                    .find(|d| {
                        d.id == offer.source_id
                            && d.owner == org
                            && d.revision == offer.source_revision
                    })
                    .ok_or(ApiError(
                        409,
                        "Source footage changed. Propose it again and obtain buyer agreement.",
                    ))?;
                let id = self.next_id;
                package = Some(Dataset {
                    id,
                    revision: 1,
                    name: r.requirements.title.clone(),
                    owner: org.into(),
                    status: Status::Processing,
                    clips: source
                        .clips
                        .iter()
                        .map(|c| Clip {
                            name: c.name.clone(),
                            approved: false,
                        })
                        .collect(),
                    grants: vec![],
                    request_origin: Some(RequestOrigin {
                        request_id: r.id,
                        request_revision: r.revision,
                        buyer: r.buyer.clone(),
                        requirements: r.requirements.clone(),
                        offer,
                        version: 1,
                    }),
                });
                updated.package_id = Some(id);
                "Sharing package prepared · Processing and review required; no access granted"
            }
            _ => {
                return Err(ApiError(
                    403,
                    "This action is not available to your organization in the current state.",
                ));
            }
        };
        if let Some(d) = package {
            self.next_id += 1;
            self.datasets.push(d);
        }
        updated.revision += 1;
        let package_id = updated.package_id;
        *self.requests.iter_mut().find(|r| r.id == id).unwrap() = updated;
        for participant in [&r.buyer, &r.requirements.seller] {
            self.events.push(Event {
                org: participant.clone(),
                text: format!("Request #{} · {event}", r.id),
            });
        }
        Ok(package_id)
    }
}
pub(super) fn sample_requests() -> Vec<DataRequest> {
    vec![DataRequest {
        id: 1,
        revision: 1,
        buyer: "atlas".into(),
        requirements: Requirements {
            title: "Pick and place into a box".into(),
            task: "People picking an object from a table and placing it into a box.".into(),
            criteria: "Hands and object visible; full action captured; outcome observable.".into(),
            volume: "20 clips, 5–15 seconds each".into(),
            perspective: "Fixed side view".into(),
            action_data: "any".into(),
            purpose: "Robotics training and evaluation".into(),
            destination: "Germany".into(),
            deadline: "Flexible".into(),
            seller: "heimdex".into(),
        },
        offer: None,
        agreed: false,
        package_id: None,
    }]
}
