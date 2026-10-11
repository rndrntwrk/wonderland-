# Swarm C implementation completion

The continuation completes the production presentation and host composition
work identified after the PR12 checkpoint against the published [SPEC](SPEC.md).
The implementation and independent source review are complete, and the recorded
C reference, native/WASM and browser GPU gates below pass. The tested C continuation
is published for review in [PR12](https://github.com/rndrntwrk/wonderland-/pull/12).
The client composition has its own stacked branch and validation record.
Authorized production content, live-service visits, ordinary Bevy
WebGPU page presentation and physical-device acceptance retain their separate
requirements in [COVERAGE.md](COVERAGE.md).

## Pinned inputs and integration boundary

- C library base: `a4c8bbaacde50cc5b1b271639b5fe454ac317e02` (runtime source `d247ebb94d1d4de79247983b4f62fffb7e06427d`).
- Existing client and gateway: PR18, `a318581f33770540808aefcf132018255a3a1d94`.
- Available B providers: PR19, `60c121b85486eb38764f988a69581cbe79dba5ac`.
- Original source baseline: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

The C branch keeps its independent Rust 1.75 libraries. Client integration is isolated on `feat/swarm-c-client-integration`, based on PR18, so it does not overwrite another swarm's branch or alter the master branch. Current C fixes are incorporated selectively without removing client adapters.

Original-server StateSync snapshots and genuine A accepted frames remain distinct. Gateway session epochs, lot incarnations, packed lot locations and local presentation revisions are never recast as A authority identities. An unconfigured service or synthetic replay is not evidence of live admission.

## Implementation work and ownership

| Work | Implementation owner | Completed paths and evidence |
| --- | --- | --- |
| Offscreen GPU ID picking and browser readback lifecycle | GPU worker | [Actual Bevy/Fyrox ID passes](../../probes/engine-bakeoff/web/GPU-PICKING.md), request-time generation tickets, source-view readiness, exclusive transfer ownership, stale-result rejection and bounded diagnostics. All three browser variants pass the selection and interruption gates below. |
| Room maps, shadow/light atlas preparation, sky and weather | Lighting worker | Source room/caster inputs, bounded color/direction targets, environment geometry and client GPU consumption. The explicit WCRC component, source noise mips, four PCF stages and exact constant-run optimization have a [closed independent review](evidence/source-completion-review.md). |
| Source thumbnails/facades and original FSOf container | Derivative worker | [Source worker and validators](../../tools/swarm-c/facade-worker/README.md), original camera/topology/container rules, actual PNG/FSOf output, retained leases, cancellation and replacement; client generation/download controls and an owned GPU facade preview. |
| Source resource banks and accepted runtime audio composition | Audio worker | Real content identity and complete causal ordinals reach source HIT/FSC/sample execution and independent 60 Hz playback. Browser/native owners preserve reset, retirement and completion boundaries; source codec checks pass below. |
| Avatar and FSOm provider composition | Content worker | Existing B decoders, source texture precedence, rig/animation/contact inputs, atomic admission and malformed-resource/lifecycle regressions feed the real client GPU owner. |
| City intent and original-service admission composition | City worker | Native-source directory/admission host events, retained camera, token/receipt identity, denied/cancelled/stale request handling and one-shot cleanup are implemented and tested. Authorized live endpoint/account acceptance remains external. |
| Client GPU renderer and common integration | Root | Real source geometry/material color, depth, stencil and asynchronous ID readback; accepted runtime/avatar/audio composition; resource/device lifetimes. The separate [client integration guide][client-guide] records modules, entry actions and client gates. |

Implementation used disjoint owners. The integrator owns common exports,
workspace manifests, final evidence, review and publication. The
[handoff](HANDOFF.md) records provider contracts, ownership and reset rules.

## Final C verification

| Gate | Final scoped result |
| --- | --- |
| [C reference suites](../../tools/swarm-c/verify.sh) | **Passed:** 441 Rust tests, 19 audio Node tests, eight browser-host Node tests and nine GPU-readback Node tests. Both normalized and source-world derivative validators pass. |
| [Pinned-A integration](../../tools/swarm-c/avatar-integration/README.md) | **Passed on final rerun:** 60 real ticks preserve every state hash and ordered event with C absent and at 30/60/120 Hz. |
| [Original audio codecs](../../tools/swarm-c/verify-audio.sh) | **Passed on final rerun:** 11 Python tests and 32 unchanged-source C# XA/UTK complete-WAV comparisons. |
| [Rust 1.75 native/WASM equivalence](../../tools/swarm-c/verify-native-wasm.sh) | **Passed:** all 18 complete records exactly equal, zero host imports and non-shared memory. Official WASM release execution uses 21,299,200 linear-memory bytes and takes 2.620 seconds; this is a reference-algorithm observation. |
| [Bevy WebGPU](../../probes/engine-bakeoff/web/evidence/bevy-webgpu-final-picker.json) | **Passed:** six scenes, 60 exact private GPU picks, 18 interruptions after actual transfer and all 12 raw canvas copies. All 20,319 ID samples match, including 6,560 ownerless samples; all six color comparisons pass unchanged thresholds. |
| [Bevy WebGL2](../../probes/engine-bakeoff/web/evidence/bevy-webgl2-picker.json) | **Passed:** six scenes, 60 exact private GPU picks and 18 interruptions held after actual PBO/fence transfer. |
| [Fyrox WebGL2](../../probes/engine-bakeoff/web/evidence/fyrox-webgl2-picker.json) | **Passed:** six scenes, 60 exact private GPU picks and 18 interruptions held after actual PBO/fence transfer. |

The [retained continuation bundle](evidence/continuation-2026-10-06/index.json)
contains the reference logs, complete observations, compiler identities, artifact
hashes and source input hashes. The integrator records the final source revision
and ledger in [VERIFICATION.md](VERIFICATION.md). Historical
failures remain tied to their original revisions. The three current engine
browser runs use Chromium 141.0.7390.37/SwiftShader and ordinary WASM with dev
optimization level 1 and debug information disabled. Their exact
[GPU contract and repro](../../probes/engine-bakeoff/web/GPU-PICKING.md) preserve
the unchanged thresholds and actual transfer/cancellation observations.

## Acceptance and publication

Ordinary Bevy WebGPU browser-page presentation remains unqualified: the
historical failure is unresolved, and the headful retry was blocked before
graphics initialization by the container's AF_UNIX socket permission. The
successful private-picking and raw-copy gates establish their recorded backend
behavior. Native engine execution remains historical, and release performance,
authorized production cohorts, live city admission, physical graphics/audio
output and mobile devices require their own evidence. The
[engine decision](../decisions/engine.md) remains pending acceptance; no
production engine has been selected.

Client release, workspace and assembled UI results are recorded separately in
the [client guide][client-guide]. The C changes are reviewed in PR12, while client
integration remains isolated in its own reviewable branch/PR. The final report
retains exact revisions and the external acceptance conditions above.

## Progress

- [x] Restore exact C/client inputs and toolchains; render-core baseline passes.
- [x] Inspect current B/D interfaces and replace stale provider assumptions.
- [x] Assign independent implementation owners.
- [x] Complete and integrate production source and host paths.
- [x] Complete independent source review and verify the scoped C native/WASM/browser behavior recorded above.
- [x] Publish the tested C implementation and exact evidence in PR12.

[client-guide]: https://github.com/rndrntwrk/wonderland-/blob/feat/swarm-c-client-integration/docs/swarm-c/CLIENT_INTEGRATION.md
