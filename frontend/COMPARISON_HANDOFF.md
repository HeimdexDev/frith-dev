# Face comparison player — integration handoff

## Implemented frontend

Owner review/details → Compare original. Anonymized-only by default. Original is loaded only on opt-in and released on opt-out. Original is left, anonymized is right. Pointer/touch dragging and Left/Right (1%), Shift+Left/Right (10%), Home/End move the divider without seeking. One play/pause, seek, speed, volume and fullscreen control set drives both streams; original is always muted. Leaving the browser tab pauses review.

The processed stream is the playback clock. Buffering/seeking pauses both; the original is aligned to that clock before resuming. Comparison is covered while alignment is in progress. Playback drift over 80 ms triggers realignment; completion requires clock difference at most 40 ms. Incompatible dimensions/duration, media errors, rejected play promises and a 10-second alignment timeout return to paused anonymized-only mode with a message. Retry by toggling Compare original again.

This is a two-decoder visual review aid, **not guaranteed frame-exact synchronization or proof of anonymization quality**. Timing is checked on browser animation frames. CPU load, decoder scheduling and background throttling affect precision. A future frame-exact review workflow may require common decoded frame timestamps and a canvas/WebCodecs player; do not use this player's clock tolerance as an acceptance standard for the model.

The fixtures are eight seconds, 960×540, 24 fps, no audio, with a synthetic illustrated face and moving timing marker. `scripts/generate-comparison.sh` regenerates both with FFmpeg. `static/sample.mp4` has only the illustrated face blurred; `static/original.mp4` is the matching unblurred source. No inference service is invoked.

## Parallel responsibilities

| Frontend (implemented) | Infrastructure engineer | GPU / blur engineer |
| --- | --- | --- |
| Split rendering and unified controls | Authenticated review-media capabilities | Matched original-review proxy and blurred proxy |
| Lazy original loading, receiver UI exclusion | Owner/reviewer authorization on every media/range request | Immutable pair manifest with hashes and model identity |
| Readiness, error and drift handling | Jurisdiction-bound storage, worker routing and original delivery policy | Matching geometry, frame timestamps, duration and ordering |
| Mock media routes and byte-range support | Range-capable streaming and capability refresh | Face blur output and actionable failure states |

No changes to the AWS or GPU implementations are made by this frontend work. The contracts below are proposed integration requirements, not existing APIs. Coordinate any schema changes before wiring the adapter.

## Proposed pair manifest (GPU → backend → authorized frontend)

```json
{
  "pair_id": "pair-immutable-id",
  "dataset_id": "dataset-id",
  "dataset_version": 1,
  "video_id": "video-id",
  "revision": 7,
  "status": "ready",
  "processing_jurisdiction": "KR",
  "width": 1920,
  "height": 1080,
  "fps_num": 24,
  "fps_den": 1,
  "duration_ms": 8000,
  "frame_count": 192,
  "timeline_origin_ms": 0,
  "timeline_id": "shared-timeline-digest",
  "original_proxy": {"artifact_id": "original-proxy-id", "sha256": "..."},
  "anonymized_proxy": {"artifact_id": "derivative-id", "sha256": "..."},
  "model_digest": "...",
  "pipeline_version": "..."
}
```

GPU engineer:

- Produce both review proxies from the same normalized timeline. If source is variable-frame-rate, normalize once before branching or provide exact timestamp mapping; do not independently resample the two outputs.
- Match presentation timestamps, frame count/order, duration, dimensions, rotation, pixel aspect ratio, crop, letterboxing and color conversion. No independent trimming or dropped frames. Match the visible geometry, not just encoded width/height.
- Return failure explicitly; never label a missing/partial/failed blur output ready. Supply immutable artifact hashes and model/pipeline identity. Reprocessing creates a new pair and invalidates the old review for the new derivative.
- Use a browser-playable review proxy (initial target H.264 MP4, yuv420p, fast-start metadata). Agree supported browser/codec combinations with infra/frontend. Keep original audio muted in comparison; only the derivative audio may play. Audio anonymization remains out of scope.
- Deliver a fixture set: moving face across divider, multiple faces, no faces, fast motion, long clip, portrait/rotated input, and variable-frame-rate input. Include known frame landmarks for alignment checks. None should contain real sensitive material during initial integration.

Infrastructure engineer:

- Proposed `GET /api/v1/datasets/{id}/videos/{video_id}/comparison` returns the manifest and `can_compare_original` only to an authenticated owner-side reviewer. Receivers must not receive original artifact IDs or URLs.
- Proposed `POST /api/v1/datasets/{id}/videos/{video_id}/review-media-grants` accepts `{pair_id, variant: "original" | "anonymized"}`. Issue short-lived, pair/version-bound capabilities. Original issuance happens only after the user opts in. Bind the grant to the authenticated reviewer and permitted jurisdiction; the UI toggle is not authority.
- Response: `{url, expires_at, pair_id, artifact_id, sha256, content_type}`. Streaming must preserve that immutable identity across range requests. Define expiry/re-authentication and refresh behavior before integration; the mock currently asks the reviewer to retry rather than refreshing credentials.
- Support `Range: bytes=start-end`, open-ended and suffix ranges; return 206 + correct Content-Range/Content-Length/Accept-Ranges, or 416 for unsatisfiable ranges. Authenticate before responding to GET, HEAD or ranges. Never allow range requests to bypass access checks.
- Originals stay in the sharer's permitted jurisdiction. **Streaming an original to a reviewer's browser is itself a transfer.** Storage region alone does not enforce this; define and enforce allowed reviewer access/location/network policy. No public original-media bucket/CDN or global cache. Apply no-store/private cache rules and audit original grant issuance/access.
- If assets use a separate origin, configure narrowly scoped CORS/credentials and the app's media CSP. Do not persist signed original URLs in client storage, analytics or logs.
- Review submission must bind reviewer identity, pair/version, exact derivative hash, model digest and expected revision. Revocation/reprocessing must invalidate stale grants/reviews. Neither a successful comparison nor the UI's ready status bypasses the existing Rust/Lean export gate.

## Integration acceptance checks

1. Receiver lacks comparison UI and original metadata; direct original GET/HEAD/Range requests are denied. Owner without review permission is denied too (real authentication is not yet implemented in this mock).
2. Opt-in fetches original; opt-out stops it. Expired/revoked credentials never fall back to public assets. Original access is audited and jurisdiction policy is exercised.
3. At several known frame landmarks, paused original/derivative represent the same source frame. Seek near start/middle/end, replay, change speed and drag while playing. If real proxies exceed the timing tolerance, resolve timestamps/streaming first.
4. Buffer either stream or fail one request: both stop, comparison stays covered while aligning, and errors are actionable. Exercise cancellation during load, rapid toggling, codec mismatch and long-duration seeks.
5. Test keyboard, touch, fullscreen and narrow screens on supported desktop/mobile browsers. Browser synchronization is best-effort; acceptance must include real media under CPU/network stress.

## Local verification

```sh
cargo test --manifest-path frontend/Cargo.toml --locked
node --test frontend/tests/comparison.test.cjs
cargo run --manifest-path frontend/Cargo.toml --locked
```

Open `/datasets/2?org=heimdex`. The mock `org` switcher is impersonation, **not authentication**. The original route enforces ownership only within this demo identity model. No real media, authentication, GPU connection or AWS integration is present.

Rust tests cover owner-only original routes, original URL absence on receiver pages, lazy original markup, processing/failed/invalid-clip denials, and range handling. Deterministic JS tests cover opt-in/out, divider independence, buffer holds, shared seeking/drift correction and mismatch/error/timeout fallback. These controller tests use simulated media events; they do not prove decoder/frame precision.
