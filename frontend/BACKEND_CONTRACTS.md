# Backend contracts — proposed, not live endpoints

The UI currently calls `FrithApi` through local Axum handlers. The form URLs in `src/lib.rs` are the implemented demo transport. The following `/api/v1` endpoints describe the intended backend integration; they are **not implemented HTTP JSON APIs**. Backend ownership, identity and authorization must be established before replacing the mock.

## Domain and service mapping

| FrithApi method | Proposed backend operation | Required behavior |
| --- | --- | --- |
| list | GET /api/v1/datasets?view=mine\|received\|shared&q=…&cursor=… | Tenant-filtered summaries, page cursor; never expose unshared datasets |
| get | GET /api/v1/datasets/{id} | Owner detail or receiver-safe DTO; hide raw assets, review internals and other recipients from receivers |
| create | POST /api/v1/datasets | Name and file descriptors; return dataset ID, revision, immutable version and local upload intents |
| change: finish/fail/retry | POST /api/v1/datasets/{id}/jobs | Real processing is asynchronous; finish/fail controls exist only in the mock |
| change: approve/reject | POST /api/v1/datasets/{id}/reviews | Reviewer identity, derivative digest, model/version, decision, expected revision |
| change: share | PUT /api/v1/datasets/{id}/grants/{recipient_org_id} | Invited organization only; preview or preview+download, approved immutable version, expected revision |
| change: revoke | DELETE /api/v1/datasets/{id}/grants/{recipient_org_id} | Durable revocation, atomic revision check and capability invalidation |
| media | POST /api/v1/datasets/{id}/videos/{video_id}/preview-grants | Short-lived, scoped capability for approved derivative only |
| media(download=true) | POST /api/v1/datasets/{id}/videos/{video_id}/download-grants | Separate fresh download authorization and auditable capability issuance |
| activity | GET /api/v1/activity?cursor=… | Tenant-filtered audit events with server timestamp, actor, object/version and outcome |

## Payload sketch

```json
{
  "id": "dataset-id",
  "revision": 7,
  "version": 1,
  "name": "Warehouse morning shift",
  "owner_org_id": "organization-id",
  "processing_jurisdiction": "KR",
  "face_status": "processing | failed | review | ready",
  "videos": [{
    "id": "video-id",
    "name": "sequence_01.mp4",
    "size_bytes": 1048576,
    "review_status": "pending | approved | rejected",
    "derivative_id": "immutable-derivative-id"
  }],
  "permissions": { "can_review": true, "can_share": false, "can_preview": true, "can_download": false }
}
```

Create input: `{name, files: [{name, size_bytes, content_type, checksum}]}`. Do not reuse the demo's newline-delimited filenames as a production upload protocol. Upload intent output binds file ID, owner, jurisdiction, size, checksum, expiry and destination. Upload completion validates bytes before starting a job. Raw media never leaves the permitted jurisdiction.

Grant input: `{expected_revision, dataset_version, preview: true, download: false}`. The two supported permission modes both include preview. A grant applies to one immutable approved version; adding or reprocessing files creates a new derivative/version requiring review and authorization, never silently reuses old grants.

Errors: `{code, message, request_id, current_revision?}`. Use 401 for missing authentication, 403 for forbidden actions, 404 for absent/invisible resources, 409 for stale revision or invalid transition, 422 for input errors, 429 for throttling, and retryable 5xx for service failures. Preserve entered form values and show accessible errors when wiring a real client. Mutations require an idempotency key and atomic compare-and-swap of `expected_revision`; the mock only implements revision checks.

## Authority and jurisdiction

- Resolve organization and actor from an authenticated server session. Never accept the demo `org` query as authority. Apply CSRF protections for cookie-authenticated mutations. Invite enumeration must be tenant-scoped.
- The organization determines the permitted processing jurisdiction; the user does not choose an arbitrary region. Bind each upload, job and output to this jurisdiction and enforce placement in the backend.
- Workers authenticate status events and preserve artifact/model identity. Browser clients must not set a job to complete or forge review evidence. Failed/rejected derivatives cannot be shared.
- Check all selected videos, the exact approved version, current grant, expiry and revocation on each media capability issuance. Deny raw media to receivers. Prevent caching that outlives authorization; define bounded behavior for in-flight streams and previously issued URLs.
- Preview-only is an application entitlement, not DRM. Viewing necessarily delivers media bytes.
- Durable audit records include failed access, preview and download events, not only the mutation events simulated in this UI.

## Existing verified release core

The root `frith-release` crate and `formal/` remain unchanged and are not called by this mock. A `ready` dataset means the UI's face-review workflow is complete; it is **not a legal/export approval**. Production sharing must additionally satisfy the core's exact manifest, two distinct approvals, trusted evidence, policy and transition requirements. A UI grant alone must not copy media across borders, publish a release, or issue recipient access.

A future backend adapter must orchestrate that core before transfer and publication, then enforce current authorization on every preview/download grant. See `../README.md` and `../docs/assurance.md` for the existing proof boundary. Do not turn mock identities, synthetic media or browser review assertions directly into trusted core `Context`, `Actor` or `Receipt` records.

## Owner comparison review

The mock now serves an owner-only original-review route and byte-range-capable preview media. See [COMPARISON_HANDOFF.md](COMPARISON_HANDOFF.md) for the proposed pair manifest and separate responsibilities for infrastructure and GPU/blur engineering. This adds no authority to the demo organization switcher. Receivers remain limited to approved derivatives.

## Request workflow: implemented mock transport

`FrithApi` now extends `RequestApi`. Implementors must supply `requests`, `request`, `save_request` and `act_on_request`; the remote async adapter remains outstanding.

| Local form/page route | Mock behavior |
| --- | --- |
| GET `/` or `/requests?org=…&q=…` | Participant-scoped request list; default landing page |
| GET `/requests/new?org=…`, POST `/requests?org=…` | Create as current buyer; invited seller gains request visibility |
| GET `/requests/{id}?org=…` | Requirements, seller's proposal summary and agreement/handoff status |
| GET/POST `/requests/{id}/edit?org=…` | Buyer only, expected revision; reset proposal/agreement; reject after preparation |
| POST `/requests/{id}/change?org=…` | Expected revision plus `offer`, `agree`, or `prepare` |

Requirements include title, task, acceptance criteria, requested volume, perspective, action-data requirement, purpose, destination, deadline and seller. Optional free-text deadline/volume are descriptive, not parsed scheduling or fulfillment rules. Destination is buyer intent, never verified residency evidence.

Only the seller can propose an owned standalone source dataset, binding its ID/revision, name, count and terms. The buyer sees this intentionally disclosed summary, not raw filename lists, media URLs or other seller inventory. Agreement belongs to the buyer. A replacement proposal or requirement edit invalidates agreement. Whole-dataset proposals can fall short of the requested volume; the buyer explicitly agrees to the displayed proposal, not an automated fulfillment claim.

Only the seller can prepare an agreed request. The mock serializes mutations under its existing mutex, validates expected request/source revisions, creates one new package, then links it to the request. The package stores `request_origin` (request/revision, buyer, requirements, offer and version 1); processing/review starts fresh and grants are empty. Scope freezes after handoff. Sharing a linked package is restricted to its agreed buyer; ordinary review, media and revocation checks still apply. Request participants can view request progress without being entitled to its package. Package links for buyers appear only after existing access checks succeed.

Production must replace the mutex/in-memory records with an atomic transaction and durable uniqueness/idempotency, authenticate each role, bind source/output artifact hashes and versions, validate rights and commercial evidence, and orchestrate the export core. The v1 snapshot is a demo scope record, not an immutable cloud artifact or legal authorization. Real semantic matching, selected time ranges, billing and subsequent package versions are not implemented.
