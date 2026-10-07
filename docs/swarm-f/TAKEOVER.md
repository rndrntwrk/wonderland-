# Swarm F takeover and local Codex client handoff

Date: 7 October 2026.
Repository: `rndrntwrk/wonderland-`.
User instruction: move the current avatar/UI continuation to local Codex and have this ChatGPT workstream pick up Swarm F.

This is an ownership and intake record, **not a new runtime implementation, executed local Codex session, approval to merge, or release claim**. The repository has issues disabled, so the handoff is retained as ordinary source on `feat/swarm-f-integration-baseline` rather than as an issue.

## 1. Division of work

The original `FreeSO-Parallel-Work-Packages.md` and `FreeSO-Rust-Browser-Rewrite-Plan.md` assign **W00 and W17 to Swarm F**: baseline/capability coverage, shared contracts, original-runtime reference evidence, differential replay, reproducible builds, cross-lane integration and release qualification.

**Local Codex takes the application continuation:** avatar presentation, native-lot UI wiring, terrain/visibility and gameplay-audio integration, rendering-resource lifetime, and the associated actual-browser acceptance work. Preserve the action-first Sim grid, interactive city map, property controls and all existing player capabilities. Do not redesign the game or replace missing original artwork with new fabricated characters.

**This ChatGPT workstream takes Swarm F:** shared contract mapping and review, capability/evidence ledger, original-runtime reference harness, native/WASM/reference comparison and cross-swarm acceptance gates. F does not duplicate the local client implementation.

**Swarm E remains the online-services implementation owner.** Its lot actors, account/city providers, SQL claims, persistence and durable ownership/transaction adapters are not reassigned to F. F supplies/reviews the shared contracts and integration tests those providers must satisfy.

For initial work, local Codex owns changes to `apps/web-shell/src/native_avatar*`, `apps/web-shell/src/native_lot.rs`, presentation/audio bridges and their focused UI tests. F starts in assigned documentation/reference/replay/verification directories. Shared game-runtime interfaces, source fixtures, workspace manifests and existing CI workflows require an explicit cross-lane change record before simultaneous edits.

## 2. Client handoff: exact source, not an old ZIP

Verified handoff anchor:

- PR **#39**: https://github.com/rndrntwrk/wonderland-/pull/39
- Branch: `feat/native-pose-replay-retention`
- Head: `015f00e0955a791286f0013868fa0ab399821686`
- Tree: `10f800f16b9997fbe41300212d46233661830b4f`
- Parent: PR #34, `13580dd2d0e2fefbefb73e4d0011032f80c06fb0`

PR #39 was read from GitHub during this intake: open, non-draft, conflict-free, unmerged. Its five previously completed workflows and exact evidence are recorded in the PR. No fresh build/test execution is claimed by this documentation-only intake.

Read these committed documents first:

- `docs/design/action-first/native-pose-replay-publication.md`
- `docs/design/action-first/native-avatar-pose-retention.md`
- `docs/design/action-first/native-avatar-reliable-session.md`
- `docs/design/action-first/native-receipt-deadlines.md`

The older pose-retention document intentionally preserves historical unpublished/local-only statements. Its later publication record and #39 supersede those delivery-status statements. Use the tracked source, not a stale patch layered over it a second time.

Preserve all of the following: every validated intermediate avatar frame before render coalescing; failure-atomic accepted batches; original PosePlayer channel retention; no blend accumulation on repeated draws; exact entity/lot/content/resource-bank ownership; unknown-result handling; independent receipt deadlines; and no automatic retry.

Native checkpoints do not serialize historical bone poses. Recovery deliberately resets unavailable pose history; an ended clip can reset at that boundary. That behavior is not a newly authorized request to invent a different checkpoint format. Propose any historical-bone contract through F before implementing it.

## 3. Reconcile the existing client branches

Refresh current PR heads and read the actual diffs before changing any branch. These are known integration inputs, not claims that their combined tree has been verified:

| Input | Role | Integration rule |
| --- | --- | --- |
| #39, `015f00e0955a791286f0013868fa0ab399821686` | Per-accepted-tick pose retention and batch/browser evidence | Start from this accepted continuation. |
| #37, recorded head `194e998d021c5ce894753222a57c8518162ffc32` | Overlapping observed-presentation-sample history | Compare and reconcile; do not blindly merge both history implementations. Its slow one-shot witness is already reused by #39. |
| #35, recorded head `8d0f78c60a885c6968abe349afd8c89ef890c8f8` | Terrain contact, nonzero Hidden visibility and current-scene picking | Preserve its numerical and elevated-lot browser checks. |
| #38, recorded head `8b2d6a7197b8b57fa59060f89e3c740b95a540ab` | Native gameplay audio, stacked on #35 | Reuse its accepted-cue/lifetime implementation and existing audio controls; do not rebuild it as if no work exists. |
| #32 / #36 | Separate C lighting/facade stack | Coordinate with the C owner; do not assume these files are already in the native-player branch. |

Start with a new local worktree/feature branch off the pinned #39 revision. Preserve dirty files and user work in any existing checkout. Combine chosen client changes there, leaving parent branches untouched. Do not merge to the default branch or close sibling PRs as an incidental part of the handoff.

First local deliverable: a reproducible application integration branch containing the intended pose/terrain/audio changes, an explicit reconciliation of #37, the combined test results and browser evidence, and a precise remaining-work list. Production services and full original artwork are not supplied by these synthetic fixtures.

## 4. Swarm F initial inspection

The inspected #39 tree already has substantial lane-specific native/WASM/browser verification. It does not establish completion of the entire W00/W17 program.

`crates/contracts/src/lib.rs` explicitly calls itself a **bounded presentation/preview boundary, not a multiplayer wire protocol**. Keep its opaque display IDs and eligibility separate from authenticated runtime commands, accepted ticks, native snapshots/effects and server authority. Map the actual `crates/game-runtime/src/live_wire*` boundary instead of inventing a second parallel native protocol by name alone.

The inspected `docs/compat/` directory contains `tools.json`, not the planned central `baseline.json`/`dispositions.md`. The inspected top-level `tools/` contains asset-cooker, creator, native-browser and swarm-b, not central reference-csharp/replay directories. These are observations about this exact tree, not proof that no other branch or local workspace contains useful work.

PR #6 documents A's extracted C# reference probes and native/WASM replay tooling. B also has published format/corpus/reference work. Locate and reuse these artifacts before writing replacements. Native=WASM agreement alone does not prove equivalence with execution of the original C# engine.

Read `docs/swarm-e/C_HANDOFF.md` from #36 at `2298b7e4bef62c922e1f94ac47db7c26534da303`. It records E's request for explicit F-owned native-wire/claim contract review, distinguishes the original-service compatibility gateway from native online services, and describes the shared snapshot/effect boundaries. Its source baseline is `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

## 5. F execution order and acceptance

### First: W00.1/W00.2 inventory and mapping

Reconcile active source capabilities with current A–E owners, commits, fixtures and evidence. Preserve the original work-package denominators. Record implementation, integration and qualification separately; unknown is not absent and one tested object is not complete family coverage.

Map current IDs, source/content versions, intents, accepted ticks, checkpoint descriptors, effect requests/results, presentation data and legacy adapters. Preserve signed legacy IDs, entity generations, source/authority epochs and the state-after-N/tail-from-N+1 boundary. A logical durable operation ID survives lease change while a fencing credential does not. Private EOD state must not leak into public snapshots, common hashes, logs or generic browser projections.

Output: a source-linked baseline ledger and an explicit native-versus-presentation-versus-legacy contract map with test vectors and unresolved decisions. Do not silently replace public types or advertise an ABI freeze before consumers are reviewed.

### Next: W00.3/W00.4 original reference and differential replay

Reuse hash-pinned A/B probes, then establish a controlled original C# runtime harness with source/content/seed/commands/external-results provenance. Repeat inputs to test trace reproducibility. Extracted-method probes remain labelled as such; unavailable original-runtime or resource cohorts remain explicit blockers to the corresponding equivalence claims.

The replay report must identify the first divergent tick and field, not merely compare totals. A deliberately injected branch/RNG error must fail and localize. Compare original/native/WASM only for cases whose real reference and compatible canonical fields are available; separate private state and presentation from replicated simulation hashes.

### In parallel after mapping: W17.1 reproducible build/evidence gates

Reuse the existing locked toolchain and workflows. Define one evidence contract binding tested commit/tree, toolchain, input/content identity, command result, failures/skips and actual artifact hashes. Detect mismatched/absent required evidence with deliberate negative cases. Keep verification read-only and avoid CI-generated product call sites.

### Later: complete shared-lot and release qualification

W17.2 requires real independent authenticated users and the complete join/select/walk/sit/chat/buy/place/view-switch/disconnect/reconnect path, including rejected actions, durable cost/ownership and real providers. Two browser contexts controlling one fixture actor do not satisfy that requirement.

The later W17 qualification must retain the approved browser/device, security/chaos, load and release matrix. Collect actual physical-device and provider evidence where required; do not infer it from headless software-rendering results or rising unit-test totals.

## 6. Initial F path boundary and status

Initial F work is limited to its own leaves under `docs/swarm-f/`, `docs/compat/`, `docs/contracts/`, `docs/verification/`, `tools/reference-csharp/`, `tools/replay/`, `tests/parity/` and assigned fixture directories. Reserve a new F workflow only when its executable gate is ready. Existing app/runtime/CI consumers are changed through a documented coordinated contract task, not by parallel ownership of the same files.

No source assets, saves, dependency versions, root manifests, lockfiles, credentials, production databases, billing, deployment configuration or application code change in this intake. High-risk protocol/authority/persistence changes require independent review. F taking over coordination is not independent approval of work it authored.

Current checkpoint: **original assignment and live client handoff inspected; scope recorded; F implementation not yet started; local Codex task not launched from this chat**.

## Local Codex task prompt

Take over the Wonderland application/client integration in `rndrntwrk/wonderland-`. Read this handoff, fetch PR #39 and verify head `015f00e0955a791286f0013868fa0ab399821686`, then create an isolated local worktree. Preserve existing user changes. Review the current #35/#38 terrain/audio changes and reconcile the overlapping #37 implementation without losing #39's per-accepted-tick history. Preserve the action-first UI and all receipt/recovery guarantees. Run the combined native, WASM, actual-browser, avatar/terrain/audio, action/cancellation, batching, deadline/reconnect and responsive gates on the final source. Return the exact commit, evidence and remaining gaps. Do not merge or deploy. Swarm F is owned by the ChatGPT integration workstream; coordinate shared contracts and fixture/CI changes rather than creating a second F implementation.
