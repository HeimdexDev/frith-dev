# Frith: request-led dataset exchange

Review date: 2026-11-09 (Asia/Seoul). Proposal, not a claim that the full workflow is implemented.

## Product decision

Heimdex discovers relevant footage in indexed archives and matches rights-holding sellers with buyers. Frith prepares and controls the exchange of the agreed dataset. Organize the product around one buyer request and its immutable dataset versions, with the existing file library as supporting inventory.

Keep the cofounder's six steps as a request's workflow, not six disconnected global tools. Use a persistent request header showing task, seller, buyer, intended use, source jurisdiction, destination, version and next blocking decision. Separate the seller workspace, authorized preparation/review workspace and buyer workspace. An organization switcher remains demo-only.

Use a request such as “pick an object from a table and place it in a box, with hands and object visible through the outcome.” Video of an action is not automatically paired robot action data; preserve the wireframe's distinction between measured, estimated and absent action data.

## Evidence and wireframe comparison

Source: uploaded `Frith 와이어프레임 (1).html` from the referenced Frith Demo Strategy conversation. The outer HTML contains six gzip/base64 page bundles, each with a nested HTML template. All six templates were decoded and read; counts below are illustrative wireframe data, not repository or customer facts.

| Wireframe screen | Retain | Existing Rust product | Redesign / missing integration |
| --- | --- | --- | --- |
| 1. 데이터셋 요청 — request/task definition | Task, success criteria, quality constraints, recipient, purpose, KR→US route, policy reference | Dataset creation records a name and filenames only | Add a buyer request linked to seller offers. Capture intended use, rights evidence, commercial agreement reference, quantity and acceptance criteria. Do not treat a request as an access grant. |
| 2. 영상 소스·구간 탐색 — source/segment discovery | Natural-language intent, source/country filters, action-data type, time ranges; 48 candidates from 19 videos | Name search over datasets; no index query or segment selection | Connect Heimdex search, preserve source version/hash and exact time ranges. Buyers see only seller-approved catalog metadata/previews; originals and unrestricted matches stay with authorized seller-side operators. |
| 3. 품질 검수 — quality review | Hands/object visibility, complete action, observable result, success/failure, holds and reviewer corrections | Per-video boolean face review only | Add robotics-utility review separately from privacy review. An automatic suggestion is not approval. Trimming changes the artifact/version and invalidates affected review evidence. |
| 4. 개인정보 처리 — privacy processing | Original/result comparison, detection timeline, blur/exclude/hold, evidence and review history | Synthetic face-blur pair and comparison controls; no actual processing | Keep the comparison player. Integrate local processing and derivative review first. Text/badges/audio, manual regions and consent-based “keep” are future decisions, not face-blur capabilities. No silent “keep” default. |
| 5. 데이터셋 구성 — package/version assembly | Clips plus JSON metadata, source/time provenance, action-data type, history and held/excluded counts | UI says v1; revision tracks mutations, not immutable published versions | Freeze reviewed clip identities and evidence into a versioned package. Exclude held clips. Exporting a preview is also access and must pass authorization. |
| 6. 고객사 승인·전달 — customer approval/delivery | Review package, purpose/recipient, access period, change requests, blocked delivery and activity | Mock preview/download grants and revocation; separate Rust/Lean export coordinator | Separate buyer acceptance, seller licensing/commercial approval, privacy/transfer approval and publication. A changed package needs new approvals. Wireframe buyer approval does not replace the core's manifest-bound independent approvals. |

The wireframe use restriction “no redistribution/commercial sale” is an example license for a particular recipient, not a blanket prohibition on the seller's marketplace transaction. Agree licensing terms per request. Decide whether price/payment stays in a contract reference for the first release; do not introduce a checkout system merely to demonstrate matching.

## Proposed information architecture and authority

Global navigation: Requests, Inventory, Exchanges, Activity. Within each request: Define → Find clips → Quality review → Privacy review → Package → Approve & deliver. Buyer views show request progress, authorized samples, agreed terms, acceptance and received packages; seller/internal screens carry raw sources, processing controls and review evidence.

Recommended state boundaries:

1. Request and offer: task, requirements, authorized inventory match, seller rights evidence, license/contract reference and recipient acceptance. No raw-video access follows from matching.
2. Preparation: source segments → utility review → local privacy processing → final derivative review → immutable package. Track quality and privacy decisions independently. Reprocessing creates a new artifact/version.
3. Release: the existing core's pending → authorized → copied → published states. Authorization precedes cross-border bytes, including staging and recipient previews. Publishing rechecks policy; authorized security officers can revoke. Buyer acceptance is additional commercial evidence, not a substitute for this gate.
4. Access: check current recipient, version, purpose, expiry and revocation before granting access. “Published” is history, not permanent access permission. Never imply that revocation recalls downloaded copies.

Proposed records: DatasetRequest, SellerOffer (rights/license reference), SourceSegment (source version and times), QualityReview, PrivacyReview (derivative/model identity), DatasetVersion, Release and RecipientGrant. Agree identifiers and schemas with API/GPU owners before implementing migrations. Do not reuse a dataset mutation revision as an immutable version ID.

## Safe update delivered in this branch

This is a presentation and integration-planning increment, not implementation of all six screens:

- `frontend/src/ui.rs`: adds Discover → Match → Exchange context above My files, explicitly labels discovery/matching as planned and the exchange as synthetic; explains the distinction between face review and export authorization on owner detail/share views; qualifies the library's jurisdiction statement as a production requirement.
- `frontend/static/style.css`: adds scoped responsive styles for the context panel and owner note.
- `frontend/src/main.rs`: accepts `FRITH_DEMO_PORT`, default 3000, while retaining a loopback-only listener. This permits independent preview without stopping an engineer's server.
- `frontend/README.md`: documents the separate preview port and links this proposal.
- `docs/frith-redesign.md`: this comparison, redesign, demo and integration report.

No changes to routes, grants, state transitions, media gates, GPU schemas, Terraform or Lean models. Existing API and backend contracts remain proposals. No automated copy, deploy, PR or push is performed.

## Integration decisions exposed by the merged infrastructure

These are source-level observations, not a cloud deployment audit.

| Decision / mismatch | Evidence | Required resolution |
| --- | --- | --- |
| Server-rendered UI versus static hosting | `frontend/src/ui.rs` renders Leptos on Axum; `infra/modules/frontend/main.tf` serves S3 web assets and forwards `/api/*` | Choose SSR hosting/routing or a separate static client. Current `/datasets/...` forms are not the proposed JSON API. Do not upload the Rust server as a static website. |
| Production service is missing from this merged tree | Deployment workflows point to `api/` and `worker/`, neither present; both contain `trivy-action@<PIN_VERSION>` | Engineers must supply service/worker sources, Dockerfiles, routes, health check and an approved scan-action pin before these workflows can run. |
| Original comparison has no production delivery adapter | Mock supports owner original requests; API task IAM deliberately lacks raw `GetObject`; comparison handoff expects jurisdiction-bound review media | Agree a separate local review service or constrained reviewer capability. Do not broaden the shared API role merely to make the player work. Reviewer browser location is part of the residency boundary. |
| GPU result is not a complete immutable review artifact | `infra/docs/gpu-adapter.md` returns output key/hash/worker region; comparison contract needs pair/timeline/model identity; core receipt binds object version | Agree source/output object versions, model digest, normalized timeline/pair fields, attempt identity and stale-result handling. Queue success must never auto-approve or publish. |
| Revocation semantics differ | Mock checks each media request; infrastructure docs explicitly say issued S3 URLs live until expiry | Decide acceptable residual-access duration and show it honestly, or use an authorization-enforcing gateway. Include range requests, cached bytes and in-flight playback in acceptance tests. |
| Residency is asserted by engineering documentation | Aircloud KR region is recorded as user-confirmed in infra docs | Preserve this as reported context, not runtime verification. Bind worker placement and original-review access to the seller's allowed jurisdiction; settle scratch deletion/retention before customer footage. |
| Durable identity, tenancy and release authority | Infrastructure provides Cognito/RDS; frontend uses `org` impersonation and in-memory grants; root core is independent | Implement authenticated roles, database transactions, immutable registration, idempotency and durable revocation; call the core through trusted evidence adapters. No public deployment of the demo switcher. |
| Rights and acceptance ownership | Wireframe combines internal review, customer data owner and legal/privacy review | Name seller rights approver, buyer acceptor and export approvers. Bind each decision to the exact package/version, terms, purpose and recipient. Define change-request and re-approval behavior. |

## Two-minute demo

- 0:00–0:20: Existing archives contain valuable physical-task footage; robotics buyers need relevant, usable data.
- 0:20–0:40: Show Discover → Match → Exchange. Explain that Heimdex's index provides discovery, the marketplace connects rights-holding sellers and buyers, and Frith controls the exchange. Discovery/matching integrations are planned in this local demo.
- 0:40–1:10: Open Picking & placement in the owner workspace. Review the synthetic original/processed pair and approve both clips. Explain that actual processing is a separate integration.
- 1:10–1:30: Grant Atlas Robotics preview access, switch to its workspace and open the received dataset. Explain that the production gate will additionally check rights, purpose and export authorization.
- 1:30–1:50: Revoke as the owner, return as Atlas and show denied future retrieval. This demonstrates the mock access behavior; cloud signed-URL revocation has an expiry window until the delivery design is settled.
- 1:50–2:00: “Heimdex finds the right footage and connects the buyer and seller. Frith turns the agreement into a reviewed, controlled dataset exchange.”

## Git safety and validation

Initial checkout: clean `main` at `cf34a0baf9db08b58c9be921ad728fa73f7bfd06`, one worktree, no unmerged index entries, no merge/rebase operation found. No repository `AGENTS.md` exists in the original or fetched tree; the supplied global instructions were used.

Read-only remote inspection found `main` at `ae647e9a655f3254eaa3bbf36c78c245e0f6576d`, 14 commits ahead of the initial checkout. Fetch updated remote-tracking refs. The merge adds 66 infrastructure/workflow files or modifications (4,475 added lines), with no Rust/frontend changes relative to the initial checkout.

Created `codex/frith-exchange-redesign` from that exact merged commit in `tmp/frith-redesign`, a separate Git worktree under the original workspace's ignored `tmp/` directory. The original `main` files and running port-3000 demo were preserved. Remote freshness is a point-in-time observation, not a promise about subsequent engineer pushes.

Checks completed successfully in the isolated worktree:

- `cargo test --manifest-path frontend/Cargo.toml --locked --offline`: 8 integration tests (4 HTTP, 4 workflow), including review gates, owner-only original access, stale revisions, grants, revocation and media ranges. Re-run after the preview-port change; all passed.
- `cargo fmt --manifest-path frontend/Cargo.toml --check`: passed.
- `node --test frontend/tests/comparison.test.cjs`: 3 tests passed.
- `FRITH_LEAN_BIN=<original workspace>/.tools/lean-4.24.0-darwin_aarch64/bin CARGO_NET_OFFLINE=true bash scripts/verify.sh`: Lean build and audit of 9 theorems passed; root formatting and Clippy passed; 9 Rust release integration tests passed; executable demo rejected late publication after revocation.
- `git diff --check`: passed.
- Browser: desktop homepage shows all three stages with planned/synthetic labels; owner detail retains the comparison controls and displays the new export-authorization distinction. Separate preview served on 127.0.0.1:3107; existing port-3000 process was not stopped.
- Final read-only remote check still reported `ae647e9a655f3254eaa3bbf36c78c245e0f6576d`. Original main remains clean at `cf34a0b`; no unmerged entries or active merge/rebase/lock markers.

Limits: the uploaded board's iframe screens rendered blank in the available browser, so the six-screen comparison relies on the decoded HTML templates, not a successful visual rendering of that board. A requested narrow viewport did not change the captured browser dimensions; mobile rendering remains unverified. No AWS/Terraform plan/apply, live identity, GPU inference, real export, signed-URL expiry or production privacy evaluation was tested. The new UI is presentation-only; it does not implement the proposed request/offer/package records.

Original main intentionally remains 14 commits behind the inspected remote. The separate review worktree contains the merged infrastructure plus this update; the human can integrate its local commits when engineers are ready. No push, PR or deployment was performed.
