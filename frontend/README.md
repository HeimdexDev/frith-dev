# Frith frontend

A runnable, local-only Rust web app for the private exchange UX. Leptos 0.8 renders the UI on the server; Axum handles navigation and forms. A small JavaScript file enhances native dialogs and the file picker. No Node runtime, WASM build, Trunk, AWS, credentials, or Lean checker is required. Form submissions navigate to fresh server-rendered pages (this is not a hydrated SPA).

## Run

From the repository root, with the repository's Rust 1.95 toolchain:

```sh
cargo run --manifest-path frontend/Cargo.toml --locked
```

Open <http://127.0.0.1:3000>. To preview a separate checkout while another demo is running, set `FRITH_DEMO_PORT=3107` before the run command and open <http://127.0.0.1:3107>. Binding remains loopback-only. The server binds only to loopback. The first build downloads dependencies; subsequent builds can use `--offline`. Stop with Ctrl-C. All mock state is shared by tabs and resets when the server restarts.

```sh
cargo test --manifest-path frontend/Cargo.toml --locked
cargo fmt --manifest-path frontend/Cargo.toml --check
# Optional controller tests (Node 18+; no npm dependencies)
node --test frontend/tests/comparison.test.cjs
```

The existing release core remains an independent Cargo package. Its verification command is still `bash scripts/verify.sh` from the root.

## Data requests → Safe sharing

The default landing screen is **Data requests**. Main navigation separates **Data requests**, **Safe sharing**, and **Activity**. Choose **Buying** or **Supplying** independently of organization identity:

| Task view | Data requests | Safe sharing |
| --- | --- | --- |
| Buying | My requests; New data request | Received datasets; preview/download only as granted |
| Supplying | Incoming requests; Review request | Outgoing packages; processing, review and access management |

The view remains in navigation/search links and the organization switch form. Without an explicit view, the demo defaults Heimdex to Supplying and other sample organizations to Buying; this is a fixture convenience, not a production role assignment. Detail pages infer the actual participant role from the record, so changing `view` never enables owner or seller actions. Activity remains an explicitly labeled organization-wide log.

Supplier package filters provide All packages, Processing, Needs review, Needs attention, Ready to share and Active access. The review queue is inside Supplying; a separately authenticated reviewer role is not implemented. The package detail shows actual processing/review/access state. Before sharing a request-linked package, the dialog shows the agreed recipient, declared destination and intended use, explicitly distinguished from verified identity or export approval.

See [reference decisions and role journeys](../docs/frith-role-views.md) for the designer references and production boundaries.

1. Switch to Atlas Robotics and choose **Buying**. Open **New data request**, describe the task, acceptance criteria, volume, intended use and destination, and choose Heimdex as the invited seller. Camera perspective, action-data requirements and deadline are optional. Alternatively, use the seeded “Pick and place into a box” request.
2. Switch to Heimdex, choose **Supplying**, and open the request. Select an owned source dataset and describe its fit, shortfalls and proposed usage terms. **Propose footage** shares only the dataset name, count and terms with the buyer. Matching is explicitly simulated, not a Heimdex search integration.
3. Switch to Atlas Robotics, choose **Buying**, and **Agree to proposed scope**. This grants no video access. Editing requirements clears the proposal and agreement; replacing a proposal also requires fresh agreement.
4. Switch to Heimdex, choose **Supplying**, and **Prepare for sharing**. A new v1 package opens in Safe sharing, with the agreed requirements/source revision attached, processing pending, all review decisions reset and no grants. Duplicate or stale handoffs are rejected. A source revision change requires a new proposal and agreement.
5. Finish mock processing, approve every video, and grant access separately. A request-linked package can only be shared with its agreed buyer. Buyer access to the source dataset and originals is never implied by a request or agreement.
6. Use **New sharing package** in Safe sharing when there is no buyer request. Standalone packages retain the existing invited-recipient controls.

Request scope is frozen after handoff. This increment supports one v1 package per request; create a new request for new scope. It does not implement later package versions, real legal agreements, billing, clip selection, semantic matching, persistence or production export authority. Mutation revisions are concurrency checks, not immutable media versions. All mock state resets on restart.

## Try standalone safe sharing

1. Supplying → Safe sharing → New sharing package. Name it and choose videos, or use the sample videos.
2. Create sharing package → Finish processing. This explicitly simulates local face processing; the location comes from the owner organization.
3. Watch each video and approve it. Owners can enable **Compare original**, then drag the vertical divider (or use arrow keys) to compare the unblurred and face-blurred synthetic clips. All videos must be approved before sharing is available. “Needs reprocessing” resets all approvals. A failed job can be retried.
4. Share dataset → Atlas Robotics → Preview only. Change the grant with the same dialog to enable downloads.
5. Switch workspace to Atlas Robotics in the sidebar. Choose Buying → Safe sharing → Received datasets. The approved dataset is visible; download appears only with permission.
6. Switch back to Heimdex and revoke access. The receiver's next page or media request is denied.
7. Check Activity for creation, review, sharing, and revocation events. Search is scoped to the selected Buying or Supplying view. Activity includes both sides of the organization.

## Structure

```text
frontend/
  Cargo.toml, Cargo.lock    Independent, locked Rust package
  src/main.rs              Local server entry point
  src/lib.rs               HTTP routes and service injection
  src/api.rs               Typed sharing models, FrithApi boundary, in-memory mock
  src/api/requests.rs      Request/offer/scope model and guarded handoff
  src/request_ui.rs        Request list, editable requirements and agreement UI
  src/ui.rs                Leptos views and reusable shell/forms
  static/style.css         Responsive design tokens and components
  static/app.js            Native dialog and local filename selection
  static/sample.mp4        Bundled synthetic geometric scene, no real people
  tests/workflows.rs       Review gates, tenant isolation, grants, revocation
  tests/requests.rs        Request roles, stale scopes, handoff and access separation
  tests/http.rs            Routes, forms, stale revisions, escaping, media denial
  BACKEND_CONTRACTS.md      Proposed integration contract and core boundary
```

## Scope and limitations

- Face anonymization only. Voice, badges, screens and location-metadata redaction are not implemented.
- Filename selection registers metadata only; no real file upload, face detection, blurring, residency enforcement or export authorization occurs. All previews and individual-video downloads use the same bundled eight-second synthetic MP4. It has no audio.
- The organization switcher is **demo impersonation, not authentication**. Do not expose this mock server publicly or use customer data. Tenant and permission guards demonstrate expected application behavior but are not a production security boundary.
- Native `<dialog>` supplies focus management, Escape dismissal and focus return. The layout adapts to narrow screens; wide data tables scroll horizontally.
- Preview-only denies the download endpoint; it cannot prevent a recipient from recording or saving already delivered media bytes. Revocation cannot erase bytes already received.
- The frontend has no persistent storage, connected background worker, async job polling, real timestamps, pagination, dataset editing, bulk download or actual backend client. The separate `worker/` implementation is not wired to this demo. Processing advances using labeled mock controls.
- Receivers use native video controls. Owners use the comparison player with unified controls; local preview routes support byte ranges for seeking. Production streaming still needs fresh authorization and capability expiry handling.
- `FrithApi` is a synchronous service seam suitable for this in-memory mock. For remote calls, convert it to an async boundary and add loading, retry, timeout and cancellation behavior. Never hold the mock mutex over network work.

Framework reference: [Leptos rendering documentation](https://docs.rs/leptos/latest/leptos/prelude/trait.RenderHtml.html).

## Comparison player and engineering handoff

See [COMPARISON_HANDOFF.md](COMPARISON_HANDOFF.md) for the implemented player, GPU pair-manifest requirements, infrastructure streaming/access contracts, and acceptance checks. Comparison is owner-only, lazy-loads the original, and defaults to anonymized-only. Paired HTML video playback is best-effort, not guaranteed frame-exact review.

## Heimdex product direction

Data requests implements the request/proposal/agreement workflow and hands off to Safe sharing. Matching remains simulated with seller-selected whole datasets. Owner detail and sharing copy distinguish commercial scope, face review and production export authorization.

See [the wireframe comparison and redesign proposal](../docs/frith-redesign.md) for the six-screen mapping, buyer/seller workflow, demo sequence, and decisions required to integrate the newly merged infrastructure.
