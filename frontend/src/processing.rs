//! Optional real processing for a fixed, server-selected demonstration clip.
use crate::{Service, api::*};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::io::AsyncWriteExt;

const MAX_VIDEO: usize = 32 * 1024 * 1024;
#[derive(Clone)]
pub struct GpuConfig {
    pub python: PathBuf,
    pub helper: PathBuf,
    pub sample: Arc<Vec<u8>>,
}
impl GpuConfig {
    pub fn from_env() -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let Some(sample) = std::env::var_os("FRITH_GPU_SAMPLE") else {
            return Ok(None);
        };
        let bytes = std::fs::read(sample)?;
        if bytes.is_empty() || bytes.len() > MAX_VIDEO {
            return Err("GPU demo sample must be 1–32 MiB".into());
        }
        let helper = PathBuf::from(std::env::var("FRITH_GPU_HELPER")?);
        if !helper.is_file() {
            return Err("FRITH_GPU_HELPER must name the installed bridge script".into());
        }
        for key in ["AWS_REGION", "RAW_BUCKET", "SAFE_BUCKET", "JOBS_QUEUE_URL"] {
            if std::env::var(key).unwrap_or_default().is_empty() {
                return Err(format!("Missing {key}").into());
            }
        }
        Ok(Some(Self {
            python: std::env::var_os("FRITH_GPU_PYTHON")
                .unwrap_or_else(|| "python3".into())
                .into(),
            helper,
            sample: Arc::new(bytes),
        }))
    }
}
#[derive(Clone)]
enum Job {
    Running,
    Complete(Arc<Vec<u8>>),
    Failed,
}
#[derive(Clone, Default)]
pub struct Processing {
    pub config: Option<GpuConfig>,
    jobs: Arc<Mutex<HashMap<u64, Job>>>,
}
impl Processing {
    pub fn new(config: Option<GpuConfig>) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }
    pub fn running(&self, id: u64) -> bool {
        matches!(self.jobs.lock().unwrap().get(&id), Some(Job::Running))
    }
    pub fn has_job(&self, id: u64) -> bool {
        self.jobs.lock().unwrap().contains_key(&id)
    }
    pub fn video(&self, id: u64, original: bool) -> ApiResult<Option<Arc<Vec<u8>>>> {
        let jobs = self.jobs.lock().unwrap();
        match jobs.get(&id) {
            Some(Job::Complete(bytes)) => Ok(Some(if original {
                self.config.as_ref().unwrap().sample.clone()
            } else {
                bytes.clone()
            })),
            Some(_) => Err(ApiError(409, "The GPU result is not ready.")),
            None => Ok(None),
        }
    }
    pub fn start(&self, api: Service, org: &str, id: u64, revision: u64) -> ApiResult<()> {
        let config = self
            .config
            .clone()
            .ok_or(ApiError(503, "GPU processing is not configured."))?;
        let guard = api.lock().unwrap();
        let d = guard.get(org, id)?;
        if d.owner != org {
            return Err(ApiError(403, "Only the owner can process this package."));
        }
        if d.owner != "heimdex" {
            return Err(ApiError(
                422,
                "The configured GPU demo processes the Korean sample only.",
            ));
        }
        if d.revision != revision || d.status != Status::Processing {
            return Err(ApiError(409, "Refresh before starting processing."));
        }
        let mut jobs = self.jobs.lock().unwrap();
        if jobs.values().any(|j| matches!(j, Job::Running)) {
            return Err(ApiError(409, "A demo job is already running. Please wait."));
        }
        jobs.insert(id, Job::Running);
        drop(jobs);
        drop(guard);
        let this = self.clone();
        let org = org.to_string();
        tokio::spawn(async move {
            let result = run(config).await;
            let mut api = api.lock().unwrap();
            let mut jobs = this.jobs.lock().unwrap();
            // A completion cannot revive a stale/replaced package.
            let current = api
                .get(&org, id)
                .ok()
                .is_some_and(|d| d.revision == revision && d.status == Status::Processing);
            if !current {
                jobs.insert(id, Job::Failed);
                return;
            }
            let success = result.is_ok();
            jobs.insert(
                id,
                match result {
                    Ok(bytes) => Job::Complete(Arc::new(bytes)),
                    Err(()) => Job::Failed,
                },
            );
            let change = Change {
                revision,
                action: if success { "gpu_finish" } else { "gpu_fail" }.into(),
                clip: None,
                recipient: None,
                permission: None,
            };
            if api.change(&org, id, change).is_err() {
                jobs.insert(id, Job::Failed);
            }
        });
        Ok(())
    }
}
async fn run(config: GpuConfig) -> Result<Vec<u8>, ()> {
    let task = async {
        let mut child = tokio::process::Command::new(&config.python)
            .arg(&config.helper)
            .kill_on_drop(true)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|_| ())?;
        let mut stdin = child.stdin.take().ok_or(())?;
        stdin.write_all(&config.sample).await.map_err(|_| ())?;
        drop(stdin);
        let output = child.wait_with_output().await.map_err(|_| ())?;
        let bytes = output.stdout;
        if !output.status.success()
            || bytes.len() < 12
            || bytes.len() > MAX_VIDEO
            || &bytes[4..8] != b"ftyp"
            || bytes == *config.sample
        {
            return Err(());
        }
        Ok(bytes)
    };
    tokio::time::timeout(Duration::from_secs(330), task)
        .await
        .map_err(|_| ())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    #[tokio::test]
    async fn real_process_boundary_rejects_forgery_duplicates_and_exposes_only_completed_bytes() {
        let path = std::env::temp_dir().join(format!(
            "frith-bridge-test-{}-{}.sh",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(
            &path,
            "cat >/dev/null\nsleep 0.1\nprintf '\\000\\000\\000\\030ftypisom-worker-output'\n",
        )
        .unwrap();
        let config = GpuConfig {
            python: "/bin/sh".into(),
            helper: path.clone(),
            sample: Arc::new(b"original sample".to_vec()),
        };
        let gpu = Processing::new(Some(config));
        let api: Service = Arc::new(Mutex::new(Box::new(MockApi::default())));
        let id = api
            .lock()
            .unwrap()
            .create(
                "heimdex",
                CreateDataset {
                    name: "GPU demo".into(),
                    files: "sample.mp4".into(),
                },
            )
            .unwrap();
        assert!(gpu.start(api.clone(), "atlas", id, 1).is_err());
        assert!(gpu.start(api.clone(), "heimdex", id, 2).is_err());
        gpu.start(api.clone(), "heimdex", id, 1).unwrap();
        assert!(gpu.start(api.clone(), "heimdex", id, 1).is_err());
        assert!(gpu.video(id, false).is_err());
        tokio::time::timeout(Duration::from_secs(5), async {
            while gpu.running(id) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            api.lock().unwrap().get("heimdex", id).unwrap().status,
            Status::Review
        );
        assert_eq!(&gpu.video(id, false).unwrap().unwrap()[4..8], b"ftyp");
        assert_eq!(*gpu.video(id, true).unwrap().unwrap(), b"original sample");
        assert!(api.lock().unwrap().get("atlas", id).is_err());
        std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn failed_command_does_not_return_fixture_video() {
        let gpu = Processing::new(Some(GpuConfig {
            python: "/missing/frith-python".into(),
            helper: "/missing/helper".into(),
            sample: Arc::new(vec![1]),
        }));
        let api: Service = Arc::new(Mutex::new(Box::new(MockApi::default())));
        let id = api
            .lock()
            .unwrap()
            .create(
                "heimdex",
                CreateDataset {
                    name: "Failure".into(),
                    files: "sample.mp4".into(),
                },
            )
            .unwrap();
        gpu.start(api.clone(), "heimdex", id, 1).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while gpu.running(id) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            api.lock().unwrap().get("heimdex", id).unwrap().status,
            Status::Failed
        );
        assert!(gpu.video(id, false).is_err());
    }
}
