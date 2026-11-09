# Assurance boundary and milestone acceptance

## Proved properties

All declarations below are in `formal/Frith/Proofs.lean`. `scripts/audit-proofs.py` re-elaborates the file and checks each named theorem's transitive axioms against `{propext, Classical.choice, Quot.sound}`. No additional axiom or unfinished proof is allowed by that check.

| Theorem | Meaning | Important limit |
|---|---|---|
| `checkExport_sound` | Boolean acceptance implies `ExportAllowed` | Evidence is supplied by trusted adapters |
| `checkExport_complete` | Every input satisfying the supported requirements is accepted | Rust rejects malformed/oversized wire data beyond the logical domain |
| `reachable_invariant` | Every reachable copied/published state has a matching receipt | Receipt truth and byte-level hashing are not proved |
| `revoked_terminal` | A revoked state has no successful next transition | Applies to that stored release generation, not lost state or recreated registries |
| `stale_revision_denied` | A mismatching expected revision cannot transition | Distributed storage must implement the atomic comparison |
| `manifest_preserved` | Successful transitions cannot substitute the manifest | Registration and restore boundaries remain external |
| `revision_increases` | A transition increases revision by exactly one | Lean uses unbounded naturals; Rust rejects `u64` overflow |
| `authorization_requires_policy` | Authorize transitions require the export policy | Does not prevent a cloud executor bypassing the coordinator |
| `publication_requires_policy` | Publish transitions require a fresh supplied context passing the policy | Authoritativeness and freshness of that supplied context remain assumptions |

Supporting lemmas prove the equivalence of the Boolean checker and the requirements, the initial invariant, and the invariant after any successful modeled step. The safety invariant is about receipt binding; it does not assert that an old published state remains authorized after its policy expires.

## Executable boundary

The Lean executable is the same decision/transition code imported by the proofs. The Rust coordinator never mirrors the export predicate. It adds strict schema decoding, caller role/tenant checks, revision overflow checks, process deadlines and response-shape checks. Those additions are tested, not proved in Lean.

The JSON subprocess protocol is internal. Rust emits typed records with fixed field names. The Lean derived decoder is not claimed to be a strict public JSON API and can ignore unknown fields; never expose it directly to untrusted callers. The public manifest decoding helper rejects duplicate/unknown fields, invalid integers, extra documents and malformed digests. Real signing requires an independently reviewed canonical encoding; JSON serialization here is not a signing format.

`LeanChecker::transition` is a low-level trusted function taking state and context. Application-facing paths should use the coordinator and future authenticated adapters. Arbitrarily constructing a state does not make it reachable. The configured executable path and installed binary must be controlled by deployment administrators. A forged checker binary invalidates the assurance claim.

Compilation, serialization, temporary-file IO, timeout handling, the OS and Rust orchestration are in the runtime trust boundary. The process has a deadline and size-limited protocol but is not an adversarial executable sandbox. The pinned Lean distribution does not bundle the separately distributed `lean4checker`; no independent external rechecking is claimed. Proofs are re-elaborated by Lean's kernel and their axiom dependencies audited. Build scripts and imported proof code are trusted and must not receive production secrets.

## Assumptions with future owners

| Assumption | Required integration evidence | Owner role |
|---|---|---|
| Principal IDs represent authenticated, independent approvers | Identity mapping and two-person approval tests | Identity/security |
| Approval records authenticate the exact manifest | Signature verification, key policy, canonical encoding and replay tests | Security |
| Context is current, not a stale replica or client claim | Transactional policy/clock/revocation authority and fail-closed retrieval | Backend |
| Review evidence describes the exact final video and model | Trusted evaluator isolation and version-bound review receipts | AI/review operations |
| Receipt describes actual copied bytes | S3 version selection, full-object digest verification and restricted staging | Platform |
| No cloud effect bypasses this authorization | Least-privilege roles, bounded grants, negative permission tests | Platform/security |
| Revocation survives a crash or failover | Durable tombstones, fencing, reconciliation and restore tests | Backend/SRE |

These roles are ownership requirements, not assignments to existing staff. Current fixtures satisfy none of these real-world evidence obligations.

## Tested failure cases

- Each policy field and both approvals; duplicate and unauthorized signers; failed or mismatched reviews.
- Expiry exactly at the boundary (`now < expiresAt`, not `<=`).
- Policy update and expiry between authorization and publication.
- Delayed completion with stale and current revisions after revocation.
- Wrong destination account/region/bucket/key/digest and missing destination version.
- Duplicate registration, skipped transitions, repeated completion and tenant/role misuse.
- Malformed/ambiguous manifests, numeric overflow and extraneous receipts.
- Checker nonzero exit, timeout and malformed output, with no state commit.

## Deliberate omissions

There is no AWS client, database, HTTP API, signature implementation, face detector, read grant, deletion service or real-data test. The in-memory coordinator serializes operations through exclusive mutable access; there is no distributed concurrency or crash durability claim. We have not proved identity privacy, geography, legal permission, video quality, cryptographic security or downstream deletion.

The next acceptance gate is durable storage with authoritative policy state and a crash-safe operation log. Do not add a broad-credential copy worker first: it would bypass the very boundary this milestone establishes.
