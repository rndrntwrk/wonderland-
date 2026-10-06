# Swarm C integration handoff

C owns disposable presentation state. A owns simulation, B owns content decoding
and provider precedence, D owns the client/DOM/UI host, E owns live directory
and admission services, and F owns global contracts/workspace integration.
The [CONTRACT.md](CONTRACT.md) types now have concrete consumers in the existing
client adapters listed below. This does not change the authoritative schemas or
make engine handles, device generations, or legacy transport counters game identities.

This branch is independently reviewable in
[PR #12](https://github.com/rndrntwrk/wonderland-/pull/12). Its original source
baseline is `4c6b3e8f5835b228723caea3c9f683c62f244f73`. The read-only A/C probe
pins A [PR #6](https://github.com/rndrntwrk/wonderland-/pull/6) at
`8a0e251d19e222a0a6833d7408ca629f674e1729`. B metadata was inspected at
`2c189402160f8c80ef1fc8f0f58ac004350c1863`; that inspection is not adoption of
every later B interface.

The continuation starts from C `a4c8bbaacde50cc5b1b271639b5fe454ac317e02`,
client PR18 `a318581f33770540808aefcf132018255a3a1d94`, and available B providers
at `60c121b85486eb38764f988a69581cbe79dba5ac`. Client composition is on
[feat/swarm-c-client-integration][client]. Its [integration guide][client-guide]
documents the actual module architecture, source inputs, entry actions and test
inventory. No stacked PR number is assumed here.

The production composition paths and source review are complete. The final
C reference, native/WASM and recorded browser GPU checks passed within the
scope below; the integrator owns their exact source revision and artifacts in
[VERIFICATION.md](VERIFICATION.md). Client release/UI verification and external
content, service and device acceptance remain separately identified.
The explicit `Blueprint.WCRC` sunlight capture/filtering extension and client
attachment are implemented, and the [independent source review](evidence/source-completion-review.md)
is closed. WCRC presence is supplied source state, independent of rendering
mode or Ultra quality.

## Implemented continuation and entry points

| Path | Concrete consumer and action |
| --- | --- |
| Source lot geometry and FSOm | [WorldDocument/PreparedWorld][world-view], existing B decoders, and the [main WebGL2 owner][client-gpu] prepare and execute real source geometry, textures, masks, stencil and depth. **Original lot → Open lot** reads validated XML/JSON. **Object appearance → Open model and textures** imports supplied resources atomically. |
| Source sprite/hybrid/2D | [World iso preparation][client-iso] and the browser sprite owner preserve source color/depth/alpha, projection, room and ID rules. **Open object sprites** resolves the selected OBJD and actual IFF/SPF resources. Hybrid requires source sprites; Full2D also requires explicit normalized architecture. |
| GPU picking | [Bevy/Fyrox offscreen ID and transfer owners](../../probes/engine-bakeoff/web/GPU-PICKING.md) and the [client GPU owner][client-gpu] perform actual asynchronous readback. Rust validates request-time identity before selection. Interactive selection is no longer the CPU reference path. |
| Lighting and environment | [Room/shadow/light-atlas preparation](../../crates/render-iso/src/lighting/), [environment execution](../../crates/render-3d/src/environment.rs), and [client presentation][client-presentation] supply real bounded textures/geometry to browser owners. Exact room/light/weather data is optional source input; the WCRC extension has a [closed source review](evidence/source-completion-review.md). |
| Source thumbnails and facades | [Source-world/camera preparation](../../crates/render-core/src/derivatives/source.rs), original [FSOf read/write](../../crates/render-core/src/derivatives/fsof.rs), the [source worker](../../tools/swarm-c/facade-worker/README.md), and [client controls][client-derivatives] implement **Generate views**, cancellation, PNG/FSOf downloads and an owned GPU facade preview. Supplied night material passes must match the complete source digest. |
| Accepted avatars and audio | [Cooked-content host][client-accepted], [runtime presentation][client-runtime-presentation], [avatar content][client-avatar-content], and [audio content][client-audio-content] compose actual local/replica `GameRuntime` accepted ticks with supplied visual/audio resources. **Open playable content → Open content folder** admits `wonderland-runtime.json` and its declared files. |
| Connected city and admission | [Native-source city adapter][client-city] binds the existing directory/admission host to scoped source identity, request/receipt tokens, retained camera/selection and cleanup. [Connected-city tests][client-city-tests] cover denied/cancelled/stale flows and remounts. Authorized live endpoint/account acceptance remains open. |

The source view and cooked accepted host are separate entry paths. Original-server
snapshots/`VmDelivery` retain their legacy lot-incarnation and transport contracts;
the accepted host admits actual A runtime data. Neither path manufactures the
other's identity or authority.

## Inputs to agree with providers

| Provider | Required input | C consumer and integration condition |
|---|---|---|
| A: identity and timeline | Full immutable lot/epoch/tick frame, architecture/content identity, entity generation and visual revision, accepted ordered event batches | `FrameStore`, `PosePlayer`, `CueLedger`, and the concrete client `RuntimePresentationSession`/accepted avatar sessions. Rendering may skip presentation frames; replayed accepted ticks remain idempotent. Do not substitute an engine entity handle or render-frame counter for game identity/time. |
| A: continuous avatar visuals | Exact visual position start/velocity, radian direction/turn velocity, animation layer frame/weight/loop/reverse/hurry state, carry and head-look inputs, generation-aware container participants | Avatar pose/contact/look sampling. Fixed subtile positions and eight facing notches cannot establish source continuous foot motion. Reset/restoration policy must be explicit because A does not persist the legacy visual skeleton. |
| A plus B: architecture | Semantic walls/floors/pools/rooms and explicit visual wall styles, roof style/pitch, base altitude, altitude centers, grass/environment fields, cutaway/room maps | `VisualLot`, production room/light preparation, lot meshes, and client `WorldDocument`/actual runtime world projection. Do not guess missing cosmetics or let visual meshes redefine collision/placement legality. |
| B: sprites and materials | Effective IFF/PIFF/resource digest and revision, source IDs, DGRP images/layers/offsets, straight RGBA, logical and padded extents, raw depth bytes, masks, dynamic OBJD fields and room mapping | `render-iso`, client `WorldIsoInputs`, object source import and the real sprite GPU owner. Preserve missing-depth versus fallback-depth distinctions; bind each resource explicitly and upload transparent padding correctly. Full2D architecture recipes remain explicit source data. |
| B: avatar assets | FreeSO-normalized skeleton/mesh/animation DTOs with original resource IDs/order and f32 bits; effective digests; appearance/outfit/HandGroup graph; textures; raw visual SLOT records; TS1/BCF/CFP providers where requested | `avatar-view::normalized`, client avatar-content source imports, accepted sessions, texture tickets and world-avatar/GPU preparation. Apply the documented FreeSO coordinate conversion exactly once. Complete dependency resolution precedes atomic appearance install. |
| B: audio assets | Complete HIT code and event/track metadata, scoped/global FWAV lookup, authorized sample ranges/format capabilities, FSC/playlist/ambience mappings and effective content identities | `audio-runtime::projection`, `AudioRuntime`, `AudioSystem`, client source banks, accepted audio session and actual Web Audio resource registration. The complete code range and byte/count limits must be available before bounded execution/decode. |
| B: 3D/city assets | Authorized maps, terrain/road/blend materials, roof/pool/tree payloads, normalized FSOm groups/materials/depth masks, authored overrides and facade textures | Reconstruction resolver, `objects::PreparedFsom`, source client importer, main GPU owner, city mesh and FSOf consumers. Source archives and sprite/custom-PNG/MTEX precedence pass through the actual B decoder/provider boundary. Prepared ordered commands retain shader/depth/stencil contracts. |
| World-view adapters plus B: derivatives | Actual document/prepared draws and complete source digest; optional exact room base aliases/lines/FineArea; explicit source-prepared night materials and outside color | `SourceWorld`, `WorldDerivatives`, bounded CPU/GPU queues, source worker and original FSOf consumer. Source camera/crop/exterior/topology preparation is implemented. Missing exact room metadata stays absent or uses the documented bounded topology fallback; missing night materials remain unavailable. |
| Source lighting/environment producer | Matching lot/epoch/architecture identity, packed room map, room/light/caster records, supplied time/weather and textures; optional exact WCRC geometry/masks/noise and target quality | `WorldPresentationInputs` and the color/direction atlas plus sky/weather GPU payload. WCRC is an explicit optional source component, not inferred from Full3D/Hybrid/Ultra. The implemented attachment and source filtering have a [closed independent review](evidence/source-completion-review.md). |
| E: city and admission | Revisioned live directory of persistent destinations, availability, coordinates, expected lot identity when known; admission/rejection and cancellation transport; lot/epoch receipt | `CityLotTransition`. Receipts and the first accepted frame must match the active request. A packed map coordinate is not a persistent destination ID. `LiveProvider` is a declaration to validate, not proof of a service call. |
| D: client/DOM/UI host | Immutable ingestion, agreed engine/backend selection, event/resource ownership, DOM/input routing, device reset and release boundaries | The existing client mounts source world, accepted runtime, source resources, GPU selection, environment, derivatives and audio owners. Qualify the assembled focus/input/suspension/release paths using the client guide. |
| F: global integration | Adopted contract versions, workspace/dependency membership and cross-swarm acceptance | Adopt the individual workspaces deliberately. Do not add renderer/audio engine dependencies to the authoritative crates or silently change global schemas. |

## Integration sequence

1. **Admit inputs before drawing.** Bound serialized envelopes and collection
   counts before deserialization, validate decoded content, then admit the entire
   frame atomically. The serde data types are owned representations, not bounded
   network decoders. Configure `RenderLimits` and residency budgets for the target.

2. **Translate into visual data once.** Resolved tile coordinates map to graphics
   as `(3*x, 3*z, 3*y)`. DGRP/SLOT horizontal offsets use sixteenths of a tile;
   their vertical offsets use fifths. Do not add terrain twice or reapply B's
   FreeSO avatar conversion. Follow the iso final-framebuffer convention when
   mixing direct sprites and cached surfaces.

3. **Commit once; sample at presentation cadence.** Retain a pose baseline once
   per accepted timeline tick, sample copies for draws, and project accepted A
   sound/animation events in their original causal order. C draws and audio
   callbacks produce no A command, gameplay completion or event acknowledgement.

4. **Execute audio on its own fixed cadence.** `AudioSystem::tick` advances the
   cosmetic HIT/FSC/station machinery at 60 Hz and returns ordered `MixerIntent`
   values. Browser playback starts after `unlockFromGesture`; resume may also
   require a gesture. The [native transport](../../crates/audio-runtime/native/README.md)
   accepts bounded sample/intent commands tagged with its private session token.
   Drain command results: accepted queue submission is not yet successful mixer
   admission. Preserve a load's original token so reset rejects stale completion.
   Completed voices return to C through `complete_voice`, never A. Native natural
   completion means the application callback consumed the final block, not that
   physical speakers played it.

5. **Stage and install assets atomically.** Preserve effective patched content,
   derivation parameters and algorithm version in keys. Keep existing visuals
   until the replacement is validated. Generation/token checks reject an old
   appearance, pick or decode completion after a newer request or reset.
   `ObjectMeshSlot` prepares the whole FSOm replacement before installation.
   Derivative queue cancellation invalidates running jobs while retaining their
   count/byte reservations until completion or drop; held replaced/reset artifacts
   remain charged until their final generation-specific lease is released.

6. **Retain engine ownership and UI boundaries.** Bind all source material inputs explicitly,
   retain the game-ID mapping, and validate the implemented async GPU readback
   ticket against the current frame. Execute each FSOm command's ordered material,
   mask, stencil and depth policy; flattening the normalized object into one
   ordinary mesh loses source behavior. Canvas game controls must yield keyboard
   input to login/chat/search/interaction fields and honor pointer cancellation.

7. **Exercise actual city entry.** Load E's directory, select a live destination,
   request admission, validate the receipt, and present the matching first lot
   state. The native-source client uses its actual legacy receipt/incarnation;
   the separate accepted host admits genuine A runtime frames. Cancellation,
   rejection and stale completion release admission/resource ownership and
   preserve valid city intent. Use an authorized endpoint/account for live acceptance.

8. **Close the acceptance conditions.** Run the exact committed packages and
   native/WASM/source probes, then real engine/browser rendering and lifecycle
   cases. Finally test authorized content and physical target devices using the
   criteria in [COVERAGE.md](COVERAGE.md) and [engine decision](../decisions/engine.md).

## Reset and lifetime rules

A lot/epoch reset clears the displayed frame and identity history, invalidates
pick tickets, resets retained pose baselines and causal audio state, and retires
owned view/audio resources. An entity ID reused in one epoch must have a greater
generation. Content A→B→A and explicit same-lot resets cannot revive old tickets.

Device loss invalidates GPU/staging handles and picks while immutable decoded
assets may survive within budget. The client WebGL2 owners invalidate their
generation, release pending transfers and rebuild from current source on a new
submission. The derivative preview's actual context-loss/restoration fixture
retains its [pixel/handle evidence][client-derivative-evidence]. Engine bakeoff
device recovery remains engine-specific and may reload the application. A
restore event alone is not evidence that all resources were reconstructed;
qualify each observed owner and backend separately.

Native device faults latch until the stream/controller is reopened. Reopen creates
a new instance/session and requires rebinding current presentation assets/cues;
old commands cannot revive the old stream. CPAL rejects unknown buffer bounds
rather than assuming the transport can contain an arbitrary callback. Its real
ALSA-null execution passed again with locked dependencies at `d247ebb`; physical output, unplug/reopen and
platform/load qualification remain open in the [verification ledger](VERIFICATION.md).

Cache byte counts distinguish encoded, decoded CPU, staging and GPU ownership.
Callers must supply honest payload costs; generic Rust values cannot discover
driver allocations. Drop/eviction tests and engine handle counts do not establish
measured GPU memory reclamation by themselves.

The derivative GPU cache pins the actual CPU lease during upload and for the
resource lifetime. Its separate facade canvas owns real textures, geometry,
targets and a GPU fence, with one pending upload, four retained entries and a
128 MiB retained/pending/staging ceiling. Source/reset invalidation hides stale
leases without erasing their charged bytes. The main world owner accounts its
world/avatar/iso/environment bundles and targets against a separate combined
256 MiB ceiling. These are allocation policies, not measurements of opaque
driver overhead or process memory.

## Reviewed WCRC behavior and cost

The [independent source review](evidence/source-completion-review.md) is closed.
The source 512×512 noise uses ten `TextureUtils.Decimate` levels: integer RGB
averaging over nontransparent texels, maximum alpha, and source point mip
sampling. The actual 32×32 GPU fixture uploads all ten levels and independently
confirms implicit mip 7 before its four PCF stages.

The exact constant-run shortcut retains the original filter equations and
channel order. A bounded run table proves that all possible source/noise-offset
taps have identical RGBA before a one-entry-per-stage cache reuses a result;
edge pixels execute the literal source taps. RGBA8 conversion after each of the
four stages and the second stage's BA-only write mask are preserved. Scratch,
retained targets and old/new noise mip allocations remain charged to their
budgets.

The standard sparse 64×64×5 test supplies source-size noise, a wall, upper-floor
quad and roof, with an outdoor object added for Ultra. Both cases fit the
unchanged 250,000,000-work-unit default:

| Quality | Whole-scene work units | Retained sunlight bytes | Retained atlas bytes |
| --- | ---: | ---: | ---: |
| Normal | 41,690,764 | 20,321,680 | 12,197,504 |
| Ultra | 164,997,309 | 81,285,520 | 30,486,656 |

The client [4×4-noise PCF report][client-pcf-report] and
[512×512-noise/mip report][client-pcf-noise-report] each match all **4,096 final
RGBA bytes exactly** on actual WebGL2. Stage two preserves raw red/green, and
the separate implicit mip-7 lookup is exact. The [GPU review guide][client-gpu-review]
provides the runners and source translation details. These are controlled
Chrome/ANGLE SwiftShader source-policy fixtures; the counters are bounded work
units and retained payload bytes, not frame-time or physical-device guarantees.
Complex lots remain subject to the explicit limits.

## Current verification and remaining acceptance

The normalized FSOm adapter and actual client GPU command execution, source
thumbnail/facade jobs and original FSOf worker/consumer, async GPU picking,
source room/shadow/light-atlas and environment execution, accepted avatar/audio
host, connected-city adapters and continuous native callback transport are
implemented and source reviewed. Their source mappings and
verification are recorded in [FSOm notes](fsom-source-notes.md),
[derivative notes](derivatives-source-notes.md) and the
[native guide](../../crates/audio-runtime/native/README.md), with client composition
and test commands in the [client guide][client-guide].

The [retained continuation bundle](evidence/continuation-2026-10-06/index.json)
contains the full reference log, native/WASM records, source input hashes and
authority/codec rerun results.

| Final C gate | Recorded result |
| --- | --- |
| [Reference suites](../../tools/swarm-c/verify.sh) | **Passed:** 441 Rust tests; 19 audio, eight browser-host and nine GPU-readback Node tests. Both [normalized PNG](../../tools/swarm-c/facade-worker/verify.py) and [source PNG/FSOf](../../tools/swarm-c/facade-worker/verify_source.py) validators passed. |
| [Pinned-A probe](../../tools/swarm-c/avatar-integration/README.md) | **Passed on final rerun:** state hashes and ordered events remain identical over 60 actual ticks with C absent and sampled at 30/60/120 Hz. |
| [Audio/source codec checks](../../tools/swarm-c/verify-audio.sh) | **Passed on final rerun:** 11 Python tests and 32 unchanged-C# XA/UTK comparisons of complete WAV bytes. |
| [Rust 1.75 native/WASM reference](../../tools/swarm-c/verify-native-wasm.sh) | **Passed:** all 18 complete records exactly equal; zero host imports; non-shared linear memory of 21,299,200 bytes. The official WASM release run took 2.620 seconds. |
| [Bevy WebGPU](../../probes/engine-bakeoff/web/evidence/bevy-webgpu-final-picker.json) | **Passed:** six scenes, 60 exact GPU picks, 18 interruptions with actual map completion held, and all 12 raw canvas copies. All 20,319 ID samples match, including 6,560 ownerless samples; all six color comparisons pass unchanged limits. |
| [Bevy WebGL2](../../probes/engine-bakeoff/web/evidence/bevy-webgl2-picker.json) | **Passed:** six scenes, 60 exact GPU picks and 18 interruptions with actual PBO/fence completion held. |
| [Fyrox WebGL2](../../probes/engine-bakeoff/web/evidence/fyrox-webgl2-picker.json) | **Passed:** six scenes, 60 exact GPU picks and 18 interruptions with actual PBO/fence completion held. |

The native and official WASM compilers both report Rust 1.75 commit `82e1608`
and LLVM 17.0.6. The final release WASM SHA-256 is
`d9b9752b6b3fb555c9ecfb07e8234a1d92a865d98e5bf722b0321b25d2c5c8ad`.
Its timing measures reference algorithms and linear-memory use, not graphics
performance. The three engine browser runs use Chromium 141.0.7390.37 with
SwiftShader, Rust 1.95, ordinary WASM and dev optimization level 1 with debug
information disabled. The [GPU ownership/reproduction guide](../../probes/engine-bakeoff/web/GPU-PICKING.md)
and [engine decision](../decisions/engine.md) preserve exact artifacts, diagnostics
and acceptance limits. These are browser correctness/lifecycle runs; current
native engine execution and release performance are not inferred from them.

The source WCRC sunlight extension and its optional client attachment have
completed independent source review, including the exact noise mip and PCF
comparisons above. The separate client guide records assembled release/UI gates.
Source captures for authorized
lot/material/avatar/audio cohorts, real endpoint/account city admission, ordinary
WebGPU page presentation, physical output and target-device load/performance
remain distinct acceptance requirements. Missing provider input is represented
explicitly, including unavailable night materials and unresolved 2D architecture.

The C lock inventory covers fourteen manifests: twelve dependency-light/core/tool
manifests and two separately owned engines. Locked offline metadata passes for
the twelve; the engine owner confirms its locked metadata/builds. CPAL uses
Rust 1.95, the other non-engine manifests use 1.75. Its lock adds only
`adler2 2.0.1`, `crc32fast 1.5.2`, `flate2 1.0.35` and `miniz_oxide 0.8.9` plus the core
dependency; existing package versions/checksums are preserved. There is no
separate audio-cooker Cargo manifest: the `audio-decode` binary belongs to
`crates/audio-runtime/Cargo.toml`. Metadata confirms dependency resolution,
not a new native device execution.

## Preserved historical evidence

The expanded reference gate and both native engine jobs passed at `d247ebb`.
The reference job passed 377 Rust tests, including the 16-test fixture suite,
19 Node audio tests, six host tests, six readback-protocol tests, 11 Python audio
tests and 32 unchanged-source codec comparisons.
All 18 complete native/WASM records matched; the real pinned-A probe preserved
state and event order across 60 ticks with C absent and at 30/60/120 Hz. The
separate locked CPAL job passed four configuration tests and the actual ALSA-null
stream. The [reference evidence](evidence/reference-d247ebb.json) and
[native-audio evidence](evidence/native-audio-d247ebb.json) retain exact results.

The published follow-up corrects the synthetic crowd's unintended overlap using
its full-cycle posed footprint, removes fractional host transforms, and adds
separate engine-texture, canvas and page observations for WebGPU. The
[fixture review](evidence/fixture-v2-review.json) preserves real ownerless
occlusion and dedicated coplanar tests. Both Bevy and Fyrox WebGL2 passed all six
color/physical-ID scenes and lifecycle checks on fixture version 2. These passes
preserve the exact gate and do not establish
arbitrary coplanar production-scene parity.
Historical `f6f78be` failures are retained.

At the historical fixture-v2 checkpoint, Bevy WebGPU remained failed. Its six direct engine-canvas ID snapshots passed,
but only three of twelve raw GPU copies completed. An independent minimal clear
showed correct GPU/canvas pixels and incorrect page presentation without Bevy.
That established a hosted browser presentation failure at that revision while
leaving the ordinary scene gate and incomplete diagnostic unresolved. Keep both
successful and failed evidence commit-specific; later direct GPU results do not
rewrite those original page observations.

The historical nine generic deadlines did not locate the stalled stage. The
continuation now implements bounded stage observations at acquisition,
submission, copy, validation and mapping without increasing the deadline; see
[GPU selection/readback](../../probes/engine-bakeoff/web/GPU-PICKING.md).

The final continuation results above close actual private GPU picking for all
three browser variants and all twelve raw WebGPU copies. The earlier
[WebGPU in-flight report](../../probes/engine-bakeoff/web/evidence/bevy-webgpu-inflight-picker.json)
remains historical; the final report includes source-view readiness and exclusive
buffer-ownership corrections. Ordinary Bevy WebGPU page presentation remains
unqualified and its historical failure unresolved. A headful experiment was
blocked before graphics initialization by the container's AF_UNIX socket
permission. None of the successful private-picking/raw-copy results constitutes
a page-presentation pass or a production engine selection.

The [coverage ledger](COVERAGE.md) retains all seventeen scope rows and separates
implemented paths, final C checks, focused/historical evidence, separate client gates, and
authorized-content/live-service/physical-device acceptance. There is no aggregate
acceptance claim in this handoff.

[client]: https://github.com/rndrntwrk/wonderland-/tree/feat/swarm-c-client-integration
[client-guide]: https://github.com/rndrntwrk/wonderland-/blob/feat/swarm-c-client-integration/docs/swarm-c/CLIENT_INTEGRATION.md
[world-view]: https://github.com/rndrntwrk/wonderland-/tree/feat/swarm-c-client-integration/crates/world-view/src
[client-gpu]: https://github.com/rndrntwrk/wonderland-/tree/feat/swarm-c-client-integration/apps/web-shell/public/world
[client-iso]: https://github.com/rndrntwrk/wonderland-/blob/feat/swarm-c-client-integration/crates/world-view/src/iso.rs
[client-presentation]: https://github.com/rndrntwrk/wonderland-/blob/feat/swarm-c-client-integration/crates/world-view/src/presentation.rs
[client-derivatives]: https://github.com/rndrntwrk/wonderland-/blob/feat/swarm-c-client-integration/apps/web-shell/src/derivative_controls.rs
[client-derivative-evidence]: https://github.com/rndrntwrk/wonderland-/blob/feat/swarm-c-client-integration/apps/web-shell/tests/fixtures/derivative-gpu-verification.json
[client-accepted]: https://github.com/rndrntwrk/wonderland-/blob/feat/swarm-c-client-integration/apps/web-shell/src/accepted_runtime.rs
[client-runtime-presentation]: https://github.com/rndrntwrk/wonderland-/blob/feat/swarm-c-client-integration/apps/web-shell/src/runtime_presentation.rs
[client-avatar-content]: https://github.com/rndrntwrk/wonderland-/tree/feat/swarm-c-client-integration/crates/avatar-content
[client-audio-content]: https://github.com/rndrntwrk/wonderland-/tree/feat/swarm-c-client-integration/crates/audio-content
[client-city]: https://github.com/rndrntwrk/wonderland-/blob/feat/swarm-c-client-integration/apps/web-shell/src/connected_adapter/city.rs
[client-city-tests]: https://github.com/rndrntwrk/wonderland-/blob/feat/swarm-c-client-integration/apps/web-shell/tests/connected_city.rs

[client-gpu-review]: https://github.com/rndrntwrk/wonderland-/blob/feat/swarm-c-client-integration/apps/web-shell/tests/evidence/gpu-review/README.md
[client-pcf-report]: https://github.com/rndrntwrk/wonderland-/blob/feat/swarm-c-client-integration/apps/web-shell/tests/evidence/gpu-review/reports/pcf-result.json
[client-pcf-noise-report]: https://github.com/rndrntwrk/wonderland-/blob/feat/swarm-c-client-integration/apps/web-shell/tests/evidence/gpu-review/reports/pcf-noise-result.json
