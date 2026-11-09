# Decisions (handoff section 10)

Costs are monthly, ap-northeast-2, and **unverified list prices** unless stated; see
[cost-estimate.md](cost-estimate.md) for formulas.

## 1-2. GPU compute and lifecycle: external GPU on Aircloud, AWS-side adapter only

*(Supersedes the earlier EC2 Auto Scaling Group 0..1 decision, removed by user override
2026-11-09: the anonymization GPU runs on Aircloud.)*

- **Chosen:** `modules/gpu-external`. IAM user `<prefix>-external-gpu` whose only permission is
  `sts:AssumeRole` on `<prefix>-external-gpu-worker` with an `sts:ExternalId` condition; the
  role (1 h max session) can consume the jobs queue, send to a new results queue, get raw,
  put safe (prefix variable), use the raw/safe KMS keys accordingly, and pull the worker ECR
  repo. Optional `allowed_source_cidrs` adds a deny-unless `aws:SourceIp`. Terraform creates
  no access key; it is created out of band and lives only in Aircloud's secret store.
- **Complexity:** low (1 user, 1 role, 1 results queue + DLQ). **Ops:** manual key rotation;
  worker start/stop and GPU-hour control live on Aircloud.
- **Cost (AWS side):** S3 internet egress for every raw GB Aircloud pulls (~$0.126/GB
  unverified); uploads back to safe are free ingress; SQS negligible. Aircloud GPU hours are
  outside the AWS bill.
- **Jurisdiction:** met — Aircloud runs in a Korean region (source: user confirmation
  2026-11-09).
- **Tradeoffs / risks:** raw bytes leave AWS over the internet (TLS), staying in Korea.
  A static access key exists (scoped to AssumeRole only, revocable, ExternalId-bound).
- **Deviation from the override text:** the role also gets `kms:Decrypt` on the safe key,
  because S3 requires it for multipart uploads to SSE-KMS buckets (large derivatives).
- **Rejected:**
  - *IAM Roles Anywhere* (X.509 from a private CA, no static key): needs a CA (ACM PCA is
    costly; self-managed CA is extra ops). Right choice past the POC.
  - *Long-lived access key on a user with the worker permissions directly:* a leaked key
    would grant data access until manually revoked; no ExternalId/session bound.
  - *Worker asks the API for presigned URLs (no AWS credentials on Aircloud):* best
    isolation, but needs API endpoints and auth for machine clients now (Phase 3 app code).

## 3. Upload: browser -> S3 multipart via presigned URLs

- **Chosen:** API creates the multipart upload and presigns each `UploadPart`; browser PUTs
  parts directly to the raw bucket; API completes and verifies with `GetObjectAttributes`.
  Raw CORS: PUT from the CloudFront origin (+ `extra_cors_origins`), exposes `ETag`. Lifecycle
  aborts incomplete multipart uploads after 1 day.
- **Complexity:** medium (app side). **Ops:** none.
- **Cost:** S3 PUT requests (~$0.0045/1k unverified) + storage; no API bandwidth.
- **Tradeoff:** retries/resume are client logic (re-presign failed parts; `ListParts` to
  resume). Uploads never traverse the API, so multi-GB files are fine.

## 4. Storage: raw / safe / web buckets, one CMK per data class

- **Chosen:** BPA on, BucketOwnerEnforced, versioning, SSE-KMS with S3 Bucket Keys.
  Raw key: api (GenerateDataKey/Decrypt for multipart presign), gpu (Decrypt). Safe key:
  gpu (GenerateDataKey/Decrypt), api (Decrypt for presigned GET). Account root gets key
  administration only. Bucket policies deny non-TLS and deny object Get/Put to any principal
  except api + gpu roles (+ `break_glass_principal_arns`). `tmp/` expires in 7 days;
  noncurrent versions in 30 days. The API has no `s3:GetObject` on raw.
- **Complexity:** medium. **Ops:** break-glass access must be added explicitly.
- **Cost:** 2 KMS keys (~$1 each/mo unverified) + requests (Bucket Keys keep this small).
- **Tradeoff:** stronger isolation than SSE-S3, but admins cannot browse raw videos in the
  console; that is intended. Web and audit buckets use SSE-S3 (not sensitive / simpler).

## 5. Networking: NAT instance + S3 gateway endpoint, no interface endpoints

- **Chosen:** 2-AZ VPC, one t4g.nano NAT instance (switch: `nat_mode = "gateway"`), free S3
  gateway endpoint, interface endpoints opt-in.
- **Complexity:** low-medium (NAT instance is self-managed). **Ops:** patch/replace the NAT
  instance; single-AZ egress dependency.
- **Cost:** NAT instance ~$4/mo + public IPv4 ~$3.65 vs NAT Gateway ~$43/mo + $0.059/GB
  processed (unverified). Each interface endpoint ~$19/mo for 2 AZs (unverified); the 6 that
  would replace NAT for ECS/ECR/logs/SQS/secrets/STS cost more than a NAT Gateway.
- **Tradeoff:** cheapest egress; lower throughput and no managed HA. S3 (videos, ECR layers)
  bypasses the NAT, which keeps its load small.

## 6. Authentication: Cognito (not an external OIDC provider yet)

- **Chosen:** user pool in the jurisdiction region, invite-only (admin create), email
  usernames, optional TOTP MFA, public app client (code + PKCE, no secret), hosted UI domain.
  No groups / custom attributes: authorization is the API's job over Postgres.
- **Complexity:** low. **Ops:** user invites via console/CLI.
- **Cost:** within the free MAU allowance for a POC (tier pricing unverified; the pool tier
  is left at the provider/AWS default).
- **Tradeoff:** customers' own IdPs (SAML/OIDC federation) can be added to the same pool
  later; an existing corporate OIDC provider would avoid Cognito entirely but none was named.

## 7. Deployment: ECS Fargate ARM64 behind an ALB

- **Chosen:** 1 task (0.25 vCPU / 0.5 GB), private subnets, ALB SG -> task SG only, circuit
  breaker rollback, `/healthz`. HTTPS only if `acm_certificate_arn` is set. By default the API
  is also routed through CloudFront at `/api/*` so browsers get HTTPS without a custom domain.
- **Complexity:** medium. **Ops:** low (no hosts).
- **Cost:** Fargate ~$8 + ALB ~$17-20 + 2 public IPv4 ~$7 (unverified).
- **Tradeoff vs simpler (App Runner / single EC2):** App Runner removes the ALB but needs a
  VPC connector to reach RDS and its regional availability/pricing was not checked here; a
  single EC2 is cheaper but becomes a host to patch. Fargate keeps the API stateless and matches the "no Kubernetes" constraint.

## 8. Cross-border: region-parameterised modules, one environment dir per jurisdiction

- **Chosen:** no multi-region code; see [architecture.md](architecture.md).
- **Complexity:** low now. **Ops:** one apply per jurisdiction.
- **Cost:** each jurisdiction repeats the fixed baseline (~$64/mo for this shape).
- **Tradeoff:** duplication of baseline per region, but raw data residency is guaranteed by
  construction rather than by policy.

## Additional decisions made in this repo

- **Database:** RDS PostgreSQL `db.t4g.micro`, single-AZ, encrypted, Secrets Manager-managed
  master password, 7-day backups, deletion protection off for POC. The app uses the master
  user (POC shortcut; create a least-privilege app role before real data).
- **Worker has no DB access.** Job status flows back through the results queue to the API
  (contract in [gpu-adapter.md](gpu-adapter.md)).
- **ECR tags are mutable** so the API can deploy with `update-service --force-new-deployment`
  on a moving tag; CI also pushes immutable-by-convention `<git sha>` tags.
- **CloudTrail:** separate trail, object-level events for raw + safe only, log file validation,
  audit bucket retention 365 days. Management events included (free only for the first copy
  in the account; turn off if an org trail exists).
- **SNS alerts topic unencrypted:** CloudWatch cannot publish to a topic using `aws/sns`.
- **CI role:** one OIDC role per repo with `ReadOnlyAccess` (for plan) + scoped push/deploy.
  Only `main` pushes and `pull_request` jobs can assume it (`sub` = `ref:refs/heads/main` or
  `pull_request`); other branches and tags are refused. A PR job still holds the deploy rights:
  split a read-only plan role before production.
