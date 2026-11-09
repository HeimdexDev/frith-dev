# Real processing in the demo

This connects the existing sample-organization UI to the existing S3/SQS face-blur worker. It is not a production API or a replacement GPU service. The configured sample is processed once per package; all listed clips deliberately display that same sample. Only Heimdex's Korean demo packages can submit to this Korean pipeline.

## Run

From the repository root, using credentials already authorized for the API role:

```sh
python3 -m venv /tmp/frith-demo-gpu
/tmp/frith-demo-gpu/bin/pip install -r frontend/gpu/requirements.txt
export FRITH_GPU_PYTHON=/tmp/frith-demo-gpu/bin/python
export FRITH_GPU_HELPER="$PWD/frontend/gpu/process.py"
export FRITH_GPU_SAMPLE=/absolute/path/to/short-demo-with-faces.mp4
export AWS_REGION=ap-northeast-2
export RAW_BUCKET=frith-poc-raw-752198711321
export SAFE_BUCKET=frith-poc-safe-752198711321
export JOBS_QUEUE_URL=https://sqs.ap-northeast-2.amazonaws.com/752198711321/frith-poc-jobs
FRITH_DEMO_PORT=3115 cargo run --manifest-path frontend/Cargo.toml --locked
```

Use a cleared short MP4 with visible faces, at most 32 MiB. The existing bundled geometric video is suitable for transport testing but cannot demonstrate face detection. The app snapshots the source bytes at startup. Credentials stay server-side; never put AWS or Aircloud keys in browser code or committed files.

Create a sharing package as Heimdex, open it, and click **Process on GPU**. The page refreshes every three seconds while processing. Completion opens review with the actual output and the configured source for comparison. Approvals and recipient grants remain separate. On failure or timeout, use Retry processing, then Process on GPU. Each retry creates a new job ID. Only one job runs per demo process at a time. Run one demo instance and do not expose the job trigger to unrestricted traffic.

The Docker image includes the helper and dependencies. Mount the selected sample read-only, set `FRITH_GPU_SAMPLE` to its container path, and supply the four AWS settings above. Use the host's existing task role. Without `FRITH_GPU_SAMPLE`, the original mock behavior remains enabled. If GPU mode is configured incorrectly, startup fails rather than silently falling back.

## What the bridge does

- Uploads the selected sample to a unique `orgs/heimdex/videos/demo-<uuid>/source.mp4` key and records the returned version and SHA-256.
- Sends the existing schema-1 job, with `min_face_px=0`, to the configured jobs queue.
- Polls only its unique output key for up to five minutes, then retrieves that exact S3 version into demo memory. There is no synthetic-video fallback for a started job.
- Does not consume or delete shared result messages, clear queues, change IAM, start/stop GPU replicas, or modify the engineers' service. Consequently detailed worker failures are currently reported as timeout unless the bridge itself fails.
- Does not delete remote inputs/outputs; their unique keys remain for the engineers' normal lifecycle/cleanup policy. A timed-out or abandoned queued job can still finish later. Demo restart discards UI state, not cloud jobs.
- A completed upload is evidence of an output artifact, not face-review approval or proof of GPU hardware use. The engineer must verify the running worker uses `USE_GPU=true`; model quality still requires visual review.

Set `FRITH_GPU_OUTPUT_PREFIX` only if the worker's configured `SAFE_PREFIX` requires it (including its trailing slash). The worker still requires every output prefix to end in `jobs/<job_id>/`.

S3 returns 403 for missing keys when ListBucket is absent, as it is on this API role. Polling treats 403/404 as pending within the deadline, never as success; persistent access failures therefore also time out. See [AWS HeadObject permissions](https://docs.aws.amazon.com/AmazonS3/latest/API/API_HeadObject.html).

## Deployment evidence, 9 November 2026

- AWS `frith-poc`: raw/safe buckets and jobs/results queues exist.
- API service: desired/running count 0; task definition `frith-poc-api:3`.
- Worker ECR repository: no images at inspection time.
- Configured Aircloud endpoint: running, one 4090 replica; serving URL is a Jupyter workspace behind Aircloud login. Status alone does not prove `worker/main.py` is consuming the Frith queue.
- The local AWS principal is excluded from raw data access; the API role trusts ECS tasks only. Use an API-role task or an engineer-approved access path; do not widen policies just for this local test.
- Live bridge attempt with the local AWS identity returned `AccessDenied` at raw upload, before enqueue. The running demo correctly entered Failed/Retry and denied media; no GPU success was claimed.
- Final container image built and includes boto3. GPU-mode container startup smoke checks stalled in Docker before the containers started; the native Rust process was used for the live failure-path test instead.
- A live GPU completion has not been verified. The engineer must have `worker/main.py` running with the existing worker credentials, bucket names and queue URLs. This connector makes no changes to that process.

## Tests

```sh
python3 -m unittest discover -s frontend/gpu -p 'test_*.py'
cargo test --manifest-path frontend/Cargo.toml --locked
```

Bridge tests use fake AWS clients; Rust/HTTP tests use a local subprocess returning fixture media. These test orchestration and access behavior, not GPU inference. A successful live acceptance run must record processing duration, observe the new blurred output, compare it with the source, and verify review/sharing separately.
