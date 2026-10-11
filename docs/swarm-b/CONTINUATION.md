# Swarm B runtime and authoring continuation

> **For agentic workers:** Use `superpowers:subagent-driven-development` for the independently owned tasks below. The coordinator alone stages, commits, and publishes the resulting changes.

**Goal:** Extend the published Swarm B implementation with safe indexed-resource authoring, additional native EOD handlers, and executable integration with Swarm A's actual simulation runtime.

**Architecture:** Preserve the existing bounded formats, resolved content, scoped native EOD host, and isolated creator workspace. Add original-aware IFF resource-map rebuilding, guarded resource transactions, typed EOD handler state, and a separate bridge to an exactly pinned simulation revision. Each subsystem remains independently testable.

**Tech stack:** Rust 1.90, existing standalone Cargo packages, Python verification tools, and the unchanged FreeSO C# sources as reference evidence.

**Spec:** The supplied `FreeSO-Parallel-Work-Packages.md`, W01, W06 and W16; the supplied `FreeSO-Rust-Browser-Rewrite-Plan.md`; and [the existing implementation plan](IMPLEMENTATION.md).

## Global constraints

- Source baseline: `4c6b3e8f5835b228723caea3c9f683c62f244f73` in `rndrntwrk/wonderland-`.
- Continuation base: `feaa91549acae3da2931f0b95ba768348f7d56f8`; branch: `swarm-b/runtime-and-authoring`.
- Simulation integration revision: `8a0e251d19e222a0a6833d7408ca629f674e1729` from `feat/swarm-a-simulation`.
- Do not edit original C# or assets, Swarm A's simulation root, shared contracts, or the root Cargo workspace.
- Preserve native/WASM portability for formats and content; keep private EOD execution native-only.
- Treat counts, encoded lengths, allocation budgets, stale guards, and authenticated identities as validation boundaries.
- Source translation, original-runtime comparison, integrated execution, and production provider qualification are separate evidence claims.
- The user has authorized implementation and publication of reviewable PRs. Merging is a separate action.

## Review focus

1. A changed resource size must not leave a stale resource-map offset or silently rewrite unsupported metadata.
2. A failed transaction, stale hash, conflicting destination ID, or malicious payload must leave the document and output unchanged.
3. Private EOD text/state must remain outside shared VM projections and public diagnostic output, including checkpoint and reconnect paths.
4. A persistence retry or recovery must preserve its identity and reconcile an ambiguous result before publishing a conflicting state.
5. A source resource that the simulation cannot represent must fail explicitly; an isolated query or debugger step must not mutate the live runtime.

## Task 1 — Rebuild indexed IFFs from their original document

**Owner/files:** `crates/legacy-formats/src/iff/`, focused format tests, `docs/swarm-b/indexed-iff.md`.

**Interface:** `iff::encode_rebuilding_index(original_bytes: &[u8], edited: &IffFile, limits: &Limits) -> Result<Vec<u8>>`; `IffDocument::encode` uses the same original-aware behavior. Ordinary `iff::encode` keeps its conservative contract for a detached indexed file.

- [x] Establish source/corpus map layouts and tests that reject stale or malformed indexes.
- [x] Demonstrate the current failure for supported indexed edits.
- [x] Implement checked map rebuilding and exact unchanged passthrough, with unknown resources preserved.
- [x] Test growth, insertion, deletion, reordering, map version/offset/count boundaries, unsupported metadata, and resource budgets.
- [x] Review and commit the independently verified format change.

## Task 2 — Guarded resource authoring transactions

**Owner/files:** `tools/creator/` excluding the debug provider seam, `tests/tools/`, `docs/swarm-b/creator.md`.

**Interfaces:** `ResourceTransaction { source_hash, operations }`, `ResourceGuard { resource_hash, format_version }`, and typed add/remove/edit/metadata operations. All guards bind to the same pretransaction document; final resource keys must be unique. Direct `rsmp` edits are prohibited.

- [x] Add stale-guard, conflict, atomic-failure, and valid multi-operation acceptance tests.
- [x] Apply edits to a candidate, validate and encode through Task 1, then publish the complete validated candidate.
- [x] Add strict, bounded `schema_version = 1` transaction interchange and usable CLI commands inside the existing workspace boundary.
- [x] Verify real CLI import/edit/export/reopen flows and failure without partial output.
- [x] Review and commit the authoring change.

## Task 3 — Four additional source-translated EOD handlers

**Owner/files:** `crates/eod-runtime/`, EOD census generator and generated registry/coverage, `docs/swarm-b/eod-coverage.md`.

**Interfaces:** Preserve existing timer callers. Add typed authoritative connect inputs and private handler state for DanceFloor (`0x4A5BE8AB`), Signs (`0x2A6356A0`), Scoreboard (`0x0949E698`), and PermissionDoor (`0x0A69F29F`). Persistence uses a typed native provider with stable requests and explicit recovery policy.

- [x] Derive event/state/authority cases from each original handler and client.
- [x] Demonstrate missing dispatch and implement each handler through `NativeHost`.
- [x] Preserve scoped tickets, authenticated actor bindings, private delivery, rate/sequence budgets, and checkpoint barriers.
- [x] Exercise malformed events, byte/string limits, disconnect/rebind, output pressure, checkpoint restore, and persistence ambiguity.
- [x] Regenerate coverage with original-runtime and production-service qualification still distinguished, then independently review and commit.

## Task 4 — Real content and isolated runtime bridge

**Owner/files:** `crates/content-runtime-bridge/`, `tools/swarm-b/runtime-bridge*`, `tests/integration/swarm_b_runtime/`, `docs/swarm-b/runtime-bridge.md`.

**Interfaces:** Convert validated Swarm B resources into Swarm A `ContentSet`; run actual bounded `SimRuntime::query_behavior`; inspect and advance an isolated restored snapshot through accepted ticks. Pin the exact Swarm A dependency rather than copying or modifying its implementation.

- [x] Record the exact representable BHAV/OBJD/entrypoint/string/tuning/slot semantics and reject unavailable or ambiguous mappings.
- [x] Add real interpreter cases with source-decoded resources and immutable content hashes.
- [x] Test query isolation, expected branch/operand behavior, snapshot restore/resume, unsupported content, and content mismatches.
- [x] Provide a reproducible integration runner and document public API gaps, including unavailable full interaction queue/check-tree callbacks.
- [x] Independently review and commit the bridge.

## Task 5 — Source-backed sprite authoring

**Owner/files:** Additive sprite encoders in `crates/legacy-formats/`, focused source-oracle fixtures/tests, `docs/swarm-b/sprite-authoring.md`.

**Interfaces:** Bounded palette and SPR2 encoding from explicit indexed pixels and source frame metadata; no implicit color quantizer. Canonical output and source alpha quantization are documented separately from raw unchanged passthrough.

- [x] Read source writers and establish exact frame/command vectors, including transparent rows and alpha/depth combinations.
- [x] Add missing-encoder tests, then implement checked deterministic encoding for source-supported layouts.
- [x] Verify decoded visual values, run/count/row overflow behavior, palette consistency and budgets; compare against unchanged C# encoder where feasible.
- [x] Review and commit the independently usable authoring API.

## Integration and publication

- [x] Run affected package tests, formatting and strict Clippy; check portable libraries on WASM and retain the native-only EOD guard.
- [x] Run the actual content/runtime integration against the pinned dependency and existing deterministic cooker/parity checks affected by edits.
- [x] Resolve concrete independent-review findings, record commands/results and limits, and update the public status.
- [x] Publish reviewable follow-up PRs with trees verified against local tested content. Leave merge decisions to the user/integrator.

## Execution ruling

Independent implementers may run concurrently only on the disjoint file sets above, as required by the active multi-agent developer instruction. Dependent APIs, shared manifests, root staging, publication, and adaptive follow-up work remain sequential. The existing user authorization and request to proceed govern continuation; this plan does not add a redundant approval checkpoint.

## Published continuation increments

- Indexed IFF and sprite authoring: [PR #7](https://github.com/rndrntwrk/wonderland-/pull/7), tree `fe2719f5ec644360e56891ec2c88cf7a456c50a6`.
- Four native EOD handlers and persistence recovery: [PR #8](https://github.com/rndrntwrk/wonderland-/pull/8), tree `b9a7fbfd3cf769118a83711ccad6559805b2a623`.
- Guarded creator transactions and palette editing: [PR #9](https://github.com/rndrntwrk/wonderland-/pull/9), tree `6f0ecf8e5886376bfce0b5c625ff23213506d9c5`.

These are sequential draft PRs above the initial #1–#4 stack. Each remote tree
was fetched and checked against the locally reviewed commit before continuing.
The final `swarm-b/runtime-and-authoring` increment contains the actual runtime
bridge and current aggregate record. Its draft PR continues this sequence.
All 44 final gates passed: 347 Rust tests and 19 Python tests; both native/WASI
comparisons and all source oracles passed on unchanged verification inputs.
