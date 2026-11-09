# frith-infra

Terraform for the Frith POC (private cross-border video exchange) in a single
jurisdiction region (`ap-northeast-2` by default). Phase 2 infrastructure only;
the API and anonymization worker application code are built separately.
Face anonymization runs on **Aircloud** (external GPU cloud); this repo provides only the
AWS-side adapter for it ([docs/gpu-adapter.md](docs/gpu-adapter.md)).

- Architecture: [docs/architecture.md](docs/architecture.md)
- Decisions (handoff section 10): [docs/decisions.md](docs/decisions.md)
- Cost estimate: [docs/cost-estimate.md](docs/cost-estimate.md)
- External GPU adapter + message contract: [docs/gpu-adapter.md](docs/gpu-adapter.md)
- Teardown: [docs/cleanup.md](docs/cleanup.md)

## Layout

All paths below and every `cd` in these docs are relative to `infra/`; workflows stay in the
repository root's `.github/`.

```
bootstrap/               state bucket (local state; run once per account)
environments/poc/        root module for the POC environment (one dir per jurisdiction)
modules/
  network/               VPC, 2 AZ public/private subnets, NAT instance|gateway, S3 gateway endpoint
  storage/               raw + safe buckets, one KMS key each, bucket policies, CORS, lifecycle
  database/              RDS PostgreSQL (single-AZ, Secrets Manager-managed password)
  queue/                 SQS jobs queue + DLQ, results queue + DLQ
  api/                   ECR (api), ECS Fargate ARM64 service, public ALB, task/execution roles
  gpu-external/          Aircloud adapter: ECR (worker), assume-role-only IAM user, scoped worker role
  auth/                  Cognito user pool + public PKCE app client + hosted UI domain
  frontend/              web bucket + CloudFront (OAC), optional /api/* route to the ALB
  monitoring/            SNS alerts, alarms, AWS Budgets, CloudTrail data events for raw/safe
  ci-oidc/               GitHub OIDC provider + CI role
.github/workflows/       terraform-plan (PR comment), deploy-api, deploy-worker
```

## Prerequisites

- Terraform >= 1.10 (S3 native state locking). Tested with 1.12.0, AWS provider 6.x.
- AWS credentials for an admin-capable principal in the target account.
- An ExternalId for the Aircloud worker: `export TF_VAR_external_gpu_external_id="$(openssl rand -hex 24)"`
  (keep it; Aircloud needs the same value; never commit it).
- Jurisdiction: Aircloud worker runs in a Korean region (user confirmation 2026-11-09), so
  in-jurisdiction anonymization is met.

## Deploy

```sh
# 1. State bucket (once per account)
cd bootstrap
cp terraform.tfvars.example terraform.tfvars   # fill placeholders
terraform init && terraform apply

# 2. POC environment
cd ../environments/poc
cp backend.hcl.example backend.hcl             # bucket = bootstrap output
cp terraform.tfvars.example terraform.tfvars   # fill placeholders
terraform init -backend-config=backend.hcl
terraform plan -out=tfplan
terraform apply tfplan
```

3. Confirm the SNS subscription emails.
4. Push images: until an image tagged `api_image_tag` (default `poc`) exists in the api ECR
   repo, ECS tasks fail to start (expected on first apply). Use the workflows or push manually:
   `docker build --platform linux/arm64 ... && docker push <ecr_api_repository_url>:poc`,
   then `aws ecs update-service --cluster <ecs_cluster_name> --service api --force-new-deployment`.
5. Worker (Aircloud): push `<ecr_worker_repository_url>:<git sha>` (deploy-worker workflow).
   Create the bootstrap access key **out of band** (Terraform never creates it, so it is
   never in state) and store it only in Aircloud's secret store with the ExternalId and role ARN:
   `aws iam create-access-key --user-name <external_gpu_user_name>`.
   See [docs/gpu-adapter.md](docs/gpu-adapter.md) for the profile config and message contract.
6. Frontend: `aws s3 sync dist/ s3://<web_bucket>/ && aws cloudfront create-invalidation --distribution-id <id> --paths '/*'`.
7. Create users (invite-only): `aws cognito-idp admin-create-user --user-pool-id <id> --username <email>`.

## App contract (what the API/worker containers receive)

| Container | Env | Notes |
|---|---|---|
| api | `PORT`(8080) `AWS_REGION` `RAW_BUCKET` `SAFE_BUCKET` `JOBS_QUEUE_URL` `RESULTS_QUEUE_URL` `DB_HOST` `DB_PORT` `DB_NAME` `DB_USERNAME` `DB_PASSWORD` `DB_SECRET_ARN` `COGNITO_USER_POOL_ID` `COGNITO_CLIENT_ID` `COGNITO_ISSUER` | must answer `GET /healthz` with 200; when `route_api_through_cloudfront=true` routes are under `/api/*` |
| worker (Aircloud) | configured on Aircloud: role ARN, ExternalId, queue URLs, bucket names | no DB access; talks to AWS only via SQS/S3/ECR with the external worker role |

- The API may presign `PutObject`/multipart on raw and `GetObject` on safe. It **cannot read
  raw bytes** (no `s3:GetObject` on raw); verify uploads with `GetObjectAttributes`.
- RDS rotates the managed master password (Secrets Manager). Injected `DB_PASSWORD` goes
  stale after rotation: re-read `DB_SECRET_ARN` on auth failure.
- Write temporary artifacts under `tmp/` (expires after 7 days).

## HTTPS warning

Without `acm_certificate_arn` the ALB listens on **HTTP only**. The default
`route_api_through_cloudfront = true` hides this from browsers (they call
`https://<cloudfront>/api/*`), but CloudFront -> ALB is plain HTTP over the internet and
the ALB itself is publicly reachable on :80. Acceptable for a POC with test data only.
For anything beyond that: custom domain + ACM certificate, CloudFront -> ALB over HTTPS,
and restrict the ALB SG to the CloudFront origin-facing prefix list.

## CI

Workflows use GitHub OIDC (no stored AWS keys). Set repository variables listed at the top
of each workflow from `terraform output`. `aquasecurity/trivy-action@<PIN_VERSION>` is a
TODO: pin a release you have verified. `terraform-plan.yml` comments the plan on PRs;
apply is manual for the POC.

## Validation status

`terraform fmt -check -recursive` and `terraform validate` pass for `bootstrap/` and
`environments/poc/`. No `plan`/`apply` has been run (no AWS credentials in the authoring
environment).

## Env templates

`infra/env/api.env.example` (API) and `infra/env/aircloud-worker.env.example` (Aircloud
endpoint) follow the playground `.env.example` format and the livecommerce/playground
`AIRCLOUD_*` names. After apply, `python3 -I infra/scripts/render_env.py api|worker` fills
every non-secret value from `terraform output`; secrets stay empty and are set by hand:
`AIRCLOUD_API_KEY` goes into the `aircloud_secret_arn` secret as
`{"AIRCLOUD_API_KEY": "..."}` (`aws secretsmanager put-secret-value`).
