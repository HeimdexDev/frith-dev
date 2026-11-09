# Monthly cost estimate (ap-northeast-2)

**No price in this file was verified against an AWS source in the authoring session.**
Every unit price is an `(unverified list price)` from memory; re-derive with the
[AWS Pricing Calculator](https://calculator.aws/) or `aws pricing get-products` before
relying on it. Formulas are given so each line can be recomputed. 1 month = 730 h.
Free tiers are ignored unless stated. Taxes (KR VAT) excluded.

## Unit prices used (all unverified list prices, USD)

| Item | Unit price |
|---|---|
| RDS PostgreSQL db.t4g.micro single-AZ | $0.025 /h |
| RDS gp3 storage | $0.131 /GB-mo |
| Fargate ARM64 vCPU / GB memory | $0.03725 /vCPU-h, $0.00409 /GB-h |
| ALB hour / LCU | $0.0225 /h, $0.008 /LCU-h |
| EC2 t4g.nano (NAT instance) | $0.0052 /h |
| EBS gp3 | $0.0912 /GB-mo |
| Public IPv4 address | $0.005 /h each |
| KMS customer key | $1.00 /key-mo; $0.03 /10k requests |
| Secrets Manager secret | $0.40 /secret-mo |
| CloudWatch standard alarm | $0.10 /alarm-metric-mo |
| CloudWatch Logs ingest / storage | $0.76 /GB, $0.0314 /GB-mo |
| CloudTrail S3 data events | $0.10 /100k events |
| S3 Standard storage | $0.025 /GB-mo |
| S3 PUT / GET requests | $0.0045 /1k, $0.00035 /1k |
| Data transfer out to internet (S3/ALB) | $0.126 /GB (first 10 TB) |
| CloudFront data out (KR/JP edges) | $0.120 /GB |
| ECR storage | $0.10 /GB-mo |
| NAT Gateway (only if `nat_mode=gateway`) | $0.059 /h + $0.059 /GB |

## Scenario A — idle / light (web + API + DB up, no anonymization jobs, ~1 GB data)

### Fixed (runs whether used or not)

| Line | Formula | $/mo |
|---|---|---|
| RDS instance | 0.025 x 730 | 18.25 |
| RDS storage 20 GB | 20 x 0.131 | 2.62 |
| Fargate 1 task 0.25 vCPU / 0.5 GB | (0.25 x 0.03725 + 0.5 x 0.00409) x 730 | 8.29 |
| ALB hours | 0.0225 x 730 | 16.43 |
| ALB LCU (assume 0.2 avg) | 0.2 x 0.008 x 730 | 1.17 |
| NAT instance + 8 GB EBS | 0.0052 x 730 + 8 x 0.0912 | 4.53 |
| Public IPv4: 2 ALB + 1 NAT | 3 x 0.005 x 730 | 10.95 |
| KMS 2 keys | 2 x 1.00 | 2.00 |
| Secrets Manager (RDS master) | 1 x 0.40 | 0.40 |
| CloudWatch alarms (2x DLQ, 2x ALB 5xx, RDS CPU) | 5 x 0.10 | 0.50 |
| **Fixed subtotal** | | **~65.1** |

### Usage-dependent (light)

| Line | Assumption | $/mo |
|---|---|---|
| S3 raw+safe | 1 GB | 0.03 |
| ECR storage | api 0.1 GB + worker 5 GB x up to 10 versions (layers dedupe; assume 10 GB) | 1.00 |
| Logs | 0.5 GB ingest | 0.40 |
| CloudFront / egress | < 1 GB | ~0.1 |
| KMS / CloudTrail / SQS requests | negligible | ~0.1 |
| **Usage subtotal** | | **~1.6** |

**Scenario A total: ~$67/mo** (unverified).

The anonymization GPU runs on **Aircloud** (external, Korean region): its GPU hours are
**outside the AWS bill** and not included anywhere in this file.

## Scenario B — active processing

Assumptions: 50 GB raw uploaded and retained; Aircloud pulls all 50 GB raw once (S3 ->
internet egress); Aircloud writes 25 GB derivatives back to safe (internet -> S3 ingress is
free); 50 GB downloaded by receivers via presigned S3 GET (egress); Aircloud pulls the
~5 GB worker image 4 times (ECR -> internet egress, 20 GB); 40k S3 PUT parts, 20k GETs;
300k CloudTrail data events; 2 GB logs. Same fixed baseline as A.
Aircloud GPU hours: billed by Aircloud, outside AWS.

| Line | Formula | $/mo |
|---|---|---|
| Fixed baseline (from A) | | 65.1 |
| S3 storage raw 50 + safe 25 GB (+ noncurrent, ignored) | 75 x 0.025 | 1.88 |
| S3 PUT requests | 40 x 0.0045 | 0.18 |
| S3 GET requests | 20 x 0.00035 | 0.01 |
| Egress: receiver downloads 50 GB (S3 -> internet) | 50 x 0.126 | 6.30 |
| Egress: raw pulled by Aircloud 50 GB (S3 -> internet) | 50 x 0.126 | 6.30 |
| Ingress: derivatives pushed by Aircloud 25 GB | free | 0.00 |
| Egress: worker image pulls by Aircloud 20 GB (ECR -> internet) | 20 x 0.126 | 2.52 |
| CloudTrail data events | 3 x 0.10 | 0.30 |
| Logs | 2 x 0.76 | 1.52 |
| ECR storage (as A) | | 1.00 |
| KMS requests (Bucket Keys: assume < 20k) | 2 x 0.03 | 0.06 |
| SQS jobs + results requests | < 1M | ~0 |
| **Scenario B total (AWS only)** | | **~$85/mo** (unverified) + Aircloud GPU hours |

Usage-dependent share in B: ~$20.1; fixed: ~$65.1. Formula for the Aircloud-driven part:
`(raw_GB_pulled + worker_image_GB_pulled) x egress_price`; derivatives back are free.

## Sensitivities and levers

- **Biggest fixed lines:** RDS (~$21), ALB (~$18), public IPv4 (~$11), Fargate (~$8).
  Stopping RDS (max 7 days at a time) and setting `api_desired_count = 0` cut idle cost
  when nobody is demoing; ALB and IPv4 remain while the ALB exists.
- `nat_mode = "gateway"` adds ~ $43/mo + $0.059/GB processed (the NAT instance line ~$4.5
  is removed; EIP IPv4 cost is the same).
- Each interface endpoint: 2 AZ x 730 x ~$0.013 ≈ $19/mo (unverified).
- Aircloud: each re-pull of raw (retries) and each worker image pull is billed as AWS egress;
  cache the worker image on Aircloud and avoid re-downloading raw on retries where possible.
- RDS final snapshot after destroy: billed per GB-month until deleted.
- AWS free tier (100 GB/mo internet egress aggregate, CloudFront 1 TB/mo, Cognito MAU
  allowance) can reduce B noticeably; not applied above because eligibility is account-specific.
- Cost by tag: activate `Project` / `Environment` as cost allocation tags in Billing to
  break down by tag (default tags are on every resource).
