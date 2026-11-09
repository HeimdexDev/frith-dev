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

## Try the workflow

1. My files → New dataset. Name it and choose videos, or use the sample videos.
2. Create dataset → Finish processing. This explicitly simulates local face processing; the location comes from the owner organization.
3. Watch each video and approve it. Owners can enable **Compare original**, then drag the vertical divider (or use arrow keys) to compare the unblurred and face-blurred synthetic clips. All videos must be approved before sharing is available. “Needs reprocessing” resets all approvals. A failed job can be retried.
4. Share dataset → Atlas Robotics → Preview only. Change the grant with the same dialog to enable downloads.
5. Switch workspace to Atlas Robotics in the sidebar. Open Shared with me. The approved dataset is visible; download appears only with permission.
6. Switch back to Heimdex and revoke access. The receiver's next page or media request is denied.
7. Check Activity for creation, review, sharing, and revocation events. Search and all four navigation views work independently for each demo organization.

## Structure

```text
frontend/
  Cargo.toml, Cargo.lock    Independent, locked Rust package
  src/main.rs              Local server entry point
  src/lib.rs               HTTP routes and service injection
  src/api.rs               Typed models, FrithApi boundary, in-memory mock
  src/ui.rs                Leptos views and reusable shell/forms
  static/style.css         Responsive design tokens and components
  static/app.js            Native dialog and local filename selection
  static/sample.mp4        Bundled synthetic geometric scene, no real people
  tests/workflows.rs       Review gates, tenant isolation, grants, revocation
  tests/http.rs            Routes, forms, stale revisions, escaping, media denial
  BACKEND_CONTRACTS.md      Proposed integration contract and core boundary
```

## Scope and limitations

- Face anonymization only. Voice, badges, screens and location-metadata redaction are not implemented.
- Filename selection registers metadata only; no real file upload, face detection, blurring, residency enforcement or export authorization occurs. All previews and individual-video downloads use the same bundled eight-second synthetic MP4. It has no audio.
- The organization switcher is **demo impersonation, not authentication**. Do not expose this mock server publicly or use customer data. Tenant and permission guards demonstrate expected application behavior but are not a production security boundary.
- Native `<dialog>` supplies focus management, Escape dismissal and focus return. The layout adapts to narrow screens; wide data tables scroll horizontally.
- Preview-only denies the download endpoint; it cannot prevent a recipient from recording or saving already delivered media bytes. Revocation cannot erase bytes already received.
- No persistent storage, background worker, async job polling, real timestamps, pagination, dataset editing, bulk download or actual backend client is included. Processing advances using labeled mock controls.
- Receivers use native video controls. Owners use the comparison player with unified controls; local preview routes support byte ranges for seeking. Production streaming still needs fresh authorization and capability expiry handling.
- `FrithApi` is a synchronous service seam suitable for this in-memory mock. For remote calls, convert it to an async boundary and add loading, retry, timeout and cancellation behavior. Never hold the mock mutex over network work.

Framework reference: [Leptos rendering documentation](https://docs.rs/leptos/latest/leptos/prelude/trait.RenderHtml.html).

## Comparison player and engineering handoff

See [COMPARISON_HANDOFF.md](COMPARISON_HANDOFF.md) for the implemented player, GPU pair-manifest requirements, infrastructure streaming/access contracts, and acceptance checks. Comparison is owner-only, lazy-loads the original, and defaults to anonymized-only. Paired HTML video playback is best-effort, not guaranteed frame-exact review.

## Heimdex product direction

My files now introduces Discover → Match → Exchange. Discovery and marketplace matching are explicitly labeled as planned integrations; the dataset workflow remains a synthetic exchange demo. Owner detail and sharing copy distinguish face review from production export authorization.

See [the wireframe comparison and redesign proposal](../docs/frith-redesign.md) for the six-screen mapping, buyer/seller workflow, demo sequence, and decisions required to integrate the newly merged infrastructure.
