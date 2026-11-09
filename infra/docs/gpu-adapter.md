# External GPU adapter (Aircloud)

Face anonymization runs on **Aircloud**, an external GPU cloud. AWS keeps the queues, the
buckets, the keys and the worker image; the worker reaches in with a narrowly scoped role.

```
API --jobs msg--> SQS jobs  <--Receive/Delete/ChangeVisibility-- Aircloud worker
                                                                   |  GetObject raw (TLS, internet)
                                                                   |  PutObject safe
API <--Receive--- SQS results <--------------SendMessage-----------+
```

## Jurisdiction: MET

The non-negotiable "face anonymization executes within the sharer's jurisdiction" is
**met**: the Aircloud GPU runs in a Korean region (source: user confirmation 2026-11-09),
and the AWS side is `ap-northeast-2` (Seoul). Raw bytes never leave Korea.

Remaining (non-jurisdiction) points: raw video bytes leave AWS over the public internet
(TLS) to Aircloud and anonymized output comes back the same way; Aircloud's retention/
deletion terms for raw bytes on its hosts and contract/DPA coverage are separate items for
the user to settle before real footage.

## Credentials

1. `terraform apply` creates IAM user `<prefix>-external-gpu` (only permission:
   `sts:AssumeRole` on `<prefix>-external-gpu-worker`) and the role. No access key is
   created by Terraform (keys would be stored in state).
2. Out of band, by an admin:
   `aws iam create-access-key --user-name <external_gpu_user_name>`
   and store the key pair **only** in Aircloud's secret store, together with the
   ExternalId (`TF_VAR_external_gpu_external_id`) and the role ARN
   (`external_gpu_worker_role_arn`).
3. Worker AWS config (AWS SDK/CLI profile):
   ```ini
   [profile frith-worker]
   role_arn = <external_gpu_worker_role_arn>
   external_id = <external id>
   source_profile = frith-bootstrap
   duration_seconds = 3600
   region = ap-northeast-2
   ```
   SDKs refresh the 1 h role session automatically.
4. Rotate: create a second key, update Aircloud, delete the old key. Revoke instantly by
   deleting the key or detaching the user policy.
5. Optional: set `external_gpu_allowed_source_cidrs` to Aircloud egress IPs — every role
   call (and AssumeRole) from elsewhere is denied.

Image pull from Aircloud: `aws ecr get-login-password --profile frith-worker | docker login --username AWS --password-stdin <registry>`;
then `docker pull <ecr_worker_repository_url>:<git sha>`.

## Job message (jobs queue)

Sent by the API. `schema_version` is mandatory.

```json
{
  "schema_version": 1,
  "job_id": "0b9f7c1e-2c55-4d6a-9a52-0d6f1d3f5e11",
  "attempt": 1,
  "video_id": "5d0c…",
  "dataset_version_id": "a1b2…",
  "raw": { "bucket": "frith-poc-raw-<acct>", "key": "orgs/<org>/videos/<video>/source.mp4", "version_id": "…", "sha256": "…" },
  "output": { "bucket": "frith-poc-safe-<acct>", "key_prefix": "orgs/<org>/videos/<video>/jobs/<job_id>/" },
  "params": { "method": "blur", "min_face_px": 24 },
  "created_at": "2026-11-09T10:00:00Z"
}
```

## Result message (results queue)

Sent by the worker, consumed by the API.

```json
{
  "schema_version": 1,
  "job_id": "0b9f7c1e-2c55-4d6a-9a52-0d6f1d3f5e11",
  "status": "succeeded",
  "retryable": false,
  "output": { "bucket": "frith-poc-safe-<acct>", "key": "orgs/<org>/videos/<video>/jobs/<job_id>/anonymized.mp4", "sha256": "…", "bytes": 123456789 },
  "error": null,
  "worker": { "provider": "aircloud", "image": "<ecr uri>@sha256:…", "region": "<aircloud region>" },
  "started_at": "2026-11-09T10:01:00Z",
  "finished_at": "2026-11-09T10:09:30Z"
}
```

| `status` | API lifecycle state | Notes |
|---|---|---|
| `started` | Processing | optional, sent once on pickup |
| `succeeded` | Needs Review | never auto-approved / published |
| `failed` + `retryable: true` | Failed (retryable) | API may re-enqueue with `attempt + 1` |
| `failed` + `retryable: false` | Failed | bad input, unsupported codec, etc. |

`error` (when failed): `{ "code": "DECODE_ERROR", "message": "…" }` — no raw frames or PII.

## Rules for both sides

- **Versioning:** consumers reject (and do not delete-as-success) any message whose
  `schema_version` they do not know: worker sends `failed`/`retryable:false` with
  `code: UNSUPPORTED_SCHEMA`; API logs and moves on. Unparseable messages go to the DLQ via
  `maxReceiveCount` (3).
- **Idempotency key = `job_id`.** The worker writes outputs under `.../jobs/<job_id>/`, so a
  redelivered job overwrites its own output only. The API applies a result once per
  (`job_id`, `status`) and ignores stale ones (e.g. `started` after `succeeded`).
- **Heartbeat:** jobs queue visibility timeout is 3600 s. A worker processing longer than
  ~50 min calls `ChangeMessageVisibility` (e.g. every 10 min, extend to 30 min). Delete the
  job message only after the result message is sent and the output object is written.
- **Order:** write output -> send result -> delete job. A crash between steps causes a
  redelivery, which idempotency absorbs.
- **Never** copy raw bytes anywhere except the worker's local scratch; delete scratch after
  each job.

The endpoint env template is `infra/env/aircloud-worker.env.example`; render it with
`python3 -I infra/scripts/render_env.py worker`.
