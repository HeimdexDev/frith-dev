# Frith verified release core

A working first milestone for controlled robotics-video sharing: Lean 4 proves the export decision and release-state rules; Rust runs that exact Lean implementation and owns a small local coordinator.

**Scope:** synthetic evidence, one fixed policy schema, an in-memory store, and no cloud side effects. This is a foundation for the product, not a deployed privacy/compliance system. No video, credentials or customer data are needed to run it.

## Frontend prototype

The independent Leptos/Axum frontend is in [`frontend/`](frontend/README.md). Run `cargo run --manifest-path frontend/Cargo.toml --locked` and open <http://127.0.0.1:3000>. It uses mock data, synthetic media and no AWS or Lean runtime dependencies. See [backend contracts](frontend/BACKEND_CONTRACTS.md) for the intended integration boundary.

## Run

Prerequisites: Rust/rustup, Python 3, curl, tar and zstd. Rust 1.95.0 and Lean 4.24.0 are pinned; Lean has no third-party package dependencies. The Lean release is intentionally pinned, not claimed to be the latest release. Bootstrap verifies the official archive SHA-256 and installs only under `.tools/`.

```sh
bash scripts/bootstrap-lean.sh
bash scripts/verify.sh
```

With dependencies cached, `CARGO_NET_OFFLINE=true bash scripts/verify.sh` runs without network access. Set `FRITH_LEAN_BIN` to an existing Lean 4.24.0 `bin` directory if desired. The first Rust build fetches locked crates; rustup may install the pinned toolchain/components if missing.

The demo prints:

```text
Authorized -> revision 1
Copied -> revision 2
Published -> revision 3
Revoked -> revision 4
Late publication after revocation: denied
```

## Layout

- `formal/Frith/Model.lean`: typed manifest, requirements, executable checker and release transitions.
- `formal/Frith/Proofs.lean`: checked theorems and transitive axiom reports.
- `formal/Main.lean`: internal JSON transport for the executable checker.
- `src/lib.rs`: Rust wire types, strict manifest decoding, bounded checker invocation and single-writer coordinator.
- `tests/release.rs`: tests against the real Lean executable, including policy mutations, expiry, wrong receipts, replay, revocation and process failure.
- `docs/assurance.md`: precise theorem scope, assumptions and remaining work.
- `scripts/verify.sh`: one verification entry point for local use or future CI.

## Why Rust and Lean

Rust is the default for services, cloud adapters, persistence, APIs and media orchestration. The security-sensitive decision and transition functions stay in Lean and are compiled into `frith-checker`. Rust invokes that executable instead of reproducing its policy logic. The subprocess is a simple milestone boundary; it is not a throughput-optimized service. Benchmark before choosing a long-lived worker or FFI.

Existing CV inference/codec tooling can be invoked from Rust; there is no need to rewrite an AI model or GPU stack to satisfy the language choice.

## Contract

The immutable manifest binds tenant, release ID/generation, source bucket/key/version/digest, destination account/region/bucket/key, recipient, purpose, policy version, model digest, transfer-basis ID and expiry. Both approvals bind the entire typed manifest. A review binds the artifact and model. Policy authorizes one exact destination and purpose in this milestone.

Two distinct principal IDs must be in the policy's approver list. The core cannot establish that those IDs belong to independent humans or that signatures were actually checked. `Context`, `Actor` and `Receipt` are trusted-adapter records, **not public request bodies**. The demo fabricates them explicitly.

Workflow: `pending -> authorized -> copied -> published`; a security officer can revoke any non-revoked stage. Revocation is terminal for that registered generation. Every successful transition increments the revision. Stale revisions, skipped stages and duplicate transitions fail without changing state. Retries return a conflict/denial rather than silently repeating an effect.

Authorization and publication both run the full policy check. Completion checks the exact destination/digest and requires a non-empty destination version. Revocation does not depend on valid approvals or policy freshness, but still requires the local checker and authorized security-officer caller.

No recipient read API or export capability is issued. `Published` is historical workflow state, **not perpetual permission to read**. The future access adapter must check current authorization for each grant. Destination staging is already an international transfer, so export authorization must precede copying any bytes.

## What is verified

The nine audited headline theorems establish checker soundness/completeness, receipt invariants for reachable states, terminal revocation, stale-revision denial, manifest preservation, increasing revisions, and policy checks at authorization/publication. The audit rejects unfinished proofs and axioms beyond standard Lean logic. These are proofs about `Model.lean`, not about AWS, cryptography, the Rust compiler or a distributed database.

The Rust test suite executes the actual compiled checker. It covers 28 negative policy mutations plus receipt mismatches and failure paths. This integration evidence does not constitute a formal proof of Rust/Lean serialization or Rust coordinator equivalence.

## Next milestones

1. **Durable authority:** transactional storage, immutable registration, revocation tombstones, idempotent operation records and authoritative policy revisions. Prove/model the transaction boundary and test crash recovery before running multiple writers.
2. **Authenticated evidence:** real actor authentication, independently verified signatures over a versioned canonical format, trusted clock and review/evaluation evidence adapters. Never accept the demo's context as evidence.
3. **Cloud enforcement:** narrowly scoped transfer authority, staging inaccessible to recipients, verified object receipts, fresh publication/read grants and cancellation/reconciliation after revocation.
4. **Privacy performance:** representative final-video evaluation, human review standards, robotics utility measurements and approved legal routes.

Each milestone should retain this proof/test boundary and explicitly add assumptions when expanding the model. A restarted empty in-memory store does not remember revoked releases: do not use this milestone for real exports.
