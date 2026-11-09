# Buying, supplying and review: task views

Implemented 2026-11-09 in the isolated `codex/frith-exchange-redesign` worktree. These are demo task views, not authenticated user roles.

## Journeys and screen ownership

| User task | Entry | Next steps | Excluded controls |
| --- | --- | --- | --- |
| Buyer | Buying → Data requests → My requests | Create/edit requirements → inspect supplier proposal → agree scope → await access → Buying → Safe sharing → Received datasets | Source inventory selection, processing, original comparison, review approval, recipient grants |
| Supplier | Supplying → Data requests → Incoming requests | Assess request → propose owned footage/terms → wait for buyer agreement → prepare package → Supplying → Safe sharing → Outgoing packages | Buyer request creation in the incoming queue; agreeing on behalf of the buyer |
| Owner-side reviewer/operator | Supplying → Safe sharing → Needs review or Needs attention | Open owned package → compare original/processed → approve or send back for processing → manage recipient grants separately | No separate reviewer account permissions are claimed; assigning authenticated reviewers is a production decision |

The Buying/Supplying switch is separate from the demo organization switch. Selecting a task view changes lists, guidance, empty states, primary actions and navigation. It cannot change request membership, package ownership or grant authority. A deep link to a record selects the view implied by that record and current organization. Activity deliberately shows organization-wide events and says so.

For the supplied Heimdex URL with no view parameter, the demo selects Supplying. Its heading is Incoming requests, its primary row action is Review request, and it does not offer New data request. Atlas/Northstar default to Buying when no explicit view or legacy received/shared tab is supplied. Explicit selection is preserved in navigation and search URLs and when switching demo organizations. This is URL navigation state, not a saved user preference or production entitlement.

## What the references contribute

| Reference | Observed pattern | Frith decision |
| --- | --- | --- |
| [Hugging Face gated datasets](https://huggingface.co/docs/hub/datasets-gated) | Custom company/country/purpose fields; manual request review; pending/accepted/rejected/reset decisions; API administration. Grants are individual-user based, and default gating can approve automatically. | Use explicit recipient/purpose/destination context before access. Keep dataset procurement requests distinct from access grants. Do not adopt automatic approval or replace Frith's organization/package boundary with a Hub user grant. No API integration in this iteration. |
| [Encord LiDAR / Physical AI](https://encord.com/lidar/) | Metadata-based curation, human review with multi-frame context, consensus/disagreement analysis. | Keep the reviewer’s queue and comparison work in the supplier workspace. Consensus metrics and multi-sensor annotation need actual evidence and are not simulated as completed features. |
| [Scale Physical AI](https://scale.com/blog/physical-ai) | Task-specific collection and semantic annotation of intent, task structure and failures, with multiple validation steps. | Retain task and acceptance criteria in buyer requirements. Treat robotics usefulness and privacy review as different evaluations; the current face-review status does not assert robotics quality. |
| [Voxel51 Robotics](https://voxel51.com/industries/robotics) | Metadata/temporal/label filtering and embeddings-based similarity search. | Keep discovery criteria separate from delivery permissions. This iteration adds filters for real mock package states, not invented similarity scores; Heimdex search and temporal selection remain integration work. |
| [V7 workflow documentation](https://docs.v7labs.com/docs/use-workflows-to-manage-your-projects) | Explicit Dataset → Annotate → Review → Complete stages, with rejected work returning to earlier stages; model/consensus stages are configurable. | Show Process locally → Review faces → Manage access using current package state. Failed or rejected work remains blocked from sharing. No generic workflow builder or model stage is implied. |

Hugging Face is useful as an interaction/API reference, not a drop-in export-policy engine. A country form field records a declaration; it is not proof of where someone is or where bytes will go. The referenced EU restriction is IP-based and specific to that feature, not a general-purpose Frith jurisdiction policy. The request-linked share dialog therefore labels the destination as declared and distinguishes all context from verified identity/location/export approval.

## Implemented increment

- `frontend/src/lib.rs`: typed Buying/Supplying and package-stage query parameters, explicit demo defaults and legacy tab compatibility.
- `frontend/src/request_ui.rs`: role-specific lists, filtering, empty states, actions, guidance and next steps; record-derived detail context.
- `frontend/src/ui.rs`: persistent task-view navigation; received versus outgoing lists; supplier stage queues; actual package preparation state; request-bound access-review context; neutral error pages.
- `frontend/static/style.css`: task switcher, package stages and access context styles.
- `frontend/src/api/requests.rs`: removed obsolete role-neutral next-action text; request status and authority remain unchanged.
- `frontend/tests/http.rs`: regression coverage for separated lists, filters, navigation state, forged view parameters, detail ownership and access-review context.

Not implemented: separate reviewer authentication/assignment, per-user account roles, custom access-request forms, real geolocation, legal decisions, semantic search, annotation consensus, worker/API wiring, persistent task-view preferences, durable storage or production export authorization.

## Integration and verification

Fetched `origin/main` at `8d5e797` and inspected its sole new commit: the face-blur/SQS worker skeleton. It adds only `worker/` files and merged cleanly into the isolated review branch before frontend edits. No worker code was modified or connected to the mock. Original main and existing demo processes were preserved; the current iteration is previewed separately on port 3111.

Frontend verification: 16 Rust integration tests (8 HTTP, 4 request, 4 sharing), frontend formatting, Clippy with warnings denied, and diff whitespace validation. Desktop browser verified supplier incoming requests without buyer creation, switching to Buying, retaining Buying across organization changes and Safe sharing navigation, received-only packages, supplier review filtering, and the owner preparation workflow.

Mobile layout, the new worker/inference pipeline, AWS infrastructure, real identity, production authorization and external integrations were not exercised. Root Rust/Lean source is unchanged and its tests were not rerun for this UI increment.
