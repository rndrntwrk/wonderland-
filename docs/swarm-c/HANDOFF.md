# Swarm C integration handoff

C owns disposable presentation state. A owns simulation, B owns content decoding
and provider precedence, D owns the client/DOM/UI host, E owns live directory
and admission services, and F owns global contracts/workspace integration. The classes in [CONTRACT.md](CONTRACT.md)
are C-local until those owners adopt a version together.

This branch is independently reviewable in
[PR #12](https://github.com/rndrntwrk/wonderland-/pull/12). Its original source
baseline is `4c6b3e8f5835b228723caea3c9f683c62f244f73`. The read-only A/C probe
pins A [PR #6](https://github.com/rndrntwrk/wonderland-/pull/6) at
`8a0e251d19e222a0a6833d7408ca629f674e1729`. B metadata was inspected at
`2c189402160f8c80ef1fc8f0f58ac004350c1863`; that inspection is not adoption of
every later B interface.

## Inputs to agree with providers

| Provider | Required input | C consumer and integration condition |
|---|---|---|
| A: identity and timeline | Full immutable lot/epoch/tick frame, architecture/content identity, entity generation and visual revision, accepted ordered event batches | `FrameStore`, `PosePlayer`, `CueLedger`. Rendering may skip presentation frames; replayed accepted ticks remain idempotent. Do not substitute an engine entity handle or render-frame counter for game identity/time. |
| A: continuous avatar visuals | Exact visual position start/velocity, radian direction/turn velocity, animation layer frame/weight/loop/reverse/hurry state, carry and head-look inputs, generation-aware container participants | Avatar pose/contact/look sampling. Fixed subtile positions and eight facing notches cannot establish source continuous foot motion. Reset/restoration policy must be explicit because A does not persist the legacy visual skeleton. |
| A plus B: architecture | Semantic walls/floors/pools/rooms and explicit visual wall styles, roof style/pitch, base altitude, altitude centers, grass/environment fields, cutaway/room maps | `VisualLot`, iso cutaway/light planning and lot meshes. Do not guess missing cosmetics or let visual meshes redefine collision/placement legality. |
| B: sprites and materials | Effective IFF/PIFF/resource digest and revision, source IDs, DGRP images/layers/offsets, straight RGBA, logical and padded extents, raw depth bytes, masks, dynamic OBJD fields and room mapping | `render-iso` preparation/material identities. Preserve missing-depth versus fallback-depth distinctions; bind each resource explicitly and upload transparent padding correctly. |
| B: avatar assets | FreeSO-normalized skeleton/mesh/animation DTOs with original resource IDs/order and f32 bits; effective digests; appearance/outfit/HandGroup graph; textures; raw visual SLOT records; TS1/BCF/CFP providers where requested | `avatar-view::normalized`, rig/mesh/appearance admission and cooking. Apply the documented FreeSO coordinate conversion exactly once. Complete dependency resolution precedes atomic appearance install. |
| B: audio assets | Complete HIT code and event/track metadata, scoped/global FWAV lookup, authorized sample ranges/format capabilities, FSC/playlist/ambience mappings and effective content identities | `audio-runtime::projection`, `AudioRuntime`, `AudioSystem`, decoder/cooker and browser sample callback. The complete code range and byte/count limits must be available before bounded execution/decode. |
| B: 3D/city assets | Authorized maps, terrain/road/blend materials, roof/pool/tree payloads, normalized FSOm groups/materials/depth masks, authored overrides and facade textures | Reconstruction resolver, `objects::PreparedFsom`, city mesh and facade outputs. B must decode the source archive and resolve sprite/custom-PNG/MTEX precedence. C preserves full normalized groups/textures and emits ordered scene commands; the consuming engine must execute their shader/depth/stencil contracts. |
| World-view adapters plus B: derivatives | Accepted immutable frame; ordered normalized meshes, explicit day/night materials and lighting, exterior wall classification, midpoint altitude and complete camera/layout inputs | `derivatives::PreparedDerivative`, `DerivativeQueue` and the standalone PNG worker. C provides bounded scheduling, source atlas equations and CPU pixels. World-view adapters still need source room/light/shadow preparation, thumbnail centering/cropping and complete material conversion; a provenance label cannot replace those algorithms. |
| E: city and admission | Revisioned live directory of persistent destinations, availability, coordinates, expected lot identity when known; admission/rejection and cancellation transport; lot/epoch receipt | `CityLotTransition`. Receipts and the first accepted frame must match the active request. A packed map coordinate is not a persistent destination ID. `LiveProvider` is a declaration to validate, not proof of a service call. |
| D: client/DOM/UI host | Immutable ingestion, agreed engine/backend selection, event/resource ownership, DOM/input routing, device reset and release boundaries | Compose C outputs into the real city/lot client and exercise focus, input cancellation, presentation suspension and release. |
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

6. **Connect engine ownership and UI.** Bind all source material inputs explicitly,
   retain the game-ID mapping, implement GPU readback if selected, and check its
   ticket against the current frame. Execute each FSOm command's ordered material,
   mask, stencil and depth policy; flattening the normalized object into one
   ordinary mesh loses source behavior. Canvas game controls must yield keyboard
   input to login/chat/search/interaction fields and honor pointer cancellation.

7. **Exercise actual city entry.** Load E's directory, select a live destination,
   request admission, validate the receipt, admit A's first frame, and only then
   enter the lot view. Cancellation/rejection/stale completion must release the
   transport's admission/resource ownership and preserve valid city intent.

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
assets may survive within budget. The probe currently recovers an actual lost
device/context by reloading the application. A restore event alone is not
resource reconstruction. A production in-process recovery strategy still needs
engine-specific implementation and testing.

Native device faults latch until the stream/controller is reopened. Reopen creates
a new instance/session and requires rebinding current presentation assets/cues;
old commands cannot revive the old stream. CPAL rejects unknown buffer bounds
rather than assuming the transport can contain an arbitrary callback. Its real
ALSA-null execution passed at `bc529fd4`; physical output, unplug/reopen and
platform/load qualification remain open in the [verification ledger](VERIFICATION.md).

Cache byte counts distinguish encoded, decoded CPU, staging and GPU ownership.
Callers must supply honest payload costs; generic Rust values cannot discover
driver allocations. Drop/eviction tests and engine handle counts do not establish
measured GPU memory reclamation by themselves.

## Concrete follow-on implementation

The normalized FSOm adapter, bounded CPU derivative jobs/PNG worker and continuous
native callback transport are implemented. Their source mappings and local
verification are recorded in [FSOm notes](fsom-source-notes.md),
[derivative notes](derivatives-source-notes.md) and the
[native guide](../../crates/audio-runtime/native/README.md).

The largest remaining C work is complete client composition: advanced
lighting/shadow and environment render passes; asynchronous GPU picking; execution
of the FSOm material/stencil commands; derivative texture upload and production
city scheduling; source thumbnail/facade preparation and legacy FSOF output where
required; native/browser audio host integration; and the actual
city/neighborhood/lot UI with live admission. The CPU worker's explicit day/night
inputs do not implement room-light/shadow generation. A fragment-state oracle does
not implement a GPU pass, and an OS null-device stream does not qualify speakers.

The expanded reference gate passed at `bc529fd4`, covering all 368 Rust package
tests, the new adapters, 18 exact native/WASM records, 32 original-source codec
comparisons, reproducible derivative PNGs and the real pinned-A boundary probe.
Both native engine jobs and the separate CPAL job passed there, including actual
device compilation, four configuration tests and the ALSA-null stream. Browser
corrections and the client/provider/physical gates remain separate. Keep both
successful and failed evidence commit-specific.

These items remain implementation responsibilities even when they also require
provider cooperation. Missing authorized content, real service responses, and
physical hardware are separate evidence gaps. The [coverage ledger](COVERAGE.md)
keeps both visible.
