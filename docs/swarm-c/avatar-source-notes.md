# W09 avatar visual pipeline source notes

This is C-local contract evidence, not an adopted global avatar schema. The
shipping package is `wonderland-avatar-view`; binary decoders, resource provider
precedence and gameplay animation authority remain with B and A respectively.
No original game payloads are included. All drawable assets in the crate and
cooker fixture are synthetic.

## Pinned inputs and executable evidence

- Original C# baseline: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
- Read-only A integration: `8a0e251d19e222a0a6833d7408ca629f674e1729`.
- B DTO reference: the supplied `swarm-c-inputs/vitaboy.rs`, captured from B
  `2c189402160f8c80ef1fc8f0f58ac004350c1863`.
- Detailed prior investigation: `swarm-c-inputs/avatar-source-discovery.md`.
- Extracted C# executable: `tools/swarm-c/avatar-cooker/source_probe.py` verifies
  its `Skeleton.cs` and `Animator.cs` text against the baseline, then compiles
  the original `ComputeBonePositions`, `RenderFrame`, and `CalculateHeadSeek`
  methods into a small reference harness using the checkout's Mac MonoGame DLL.
  Generated source and executable are kept outside the shipping crate.
- Assembly SHA256:
  `dbb1c9203614fcea82e5a4e66df288f8144288b6adbea2b2ee525e4afa4a433d`.
  Its assembly identity reports **Version=0.0.0.0**. The project references
  MonoGame 3.6.0.1625, but that pin is not verified for this DLL. Consequently
  this probe establishes parity with the hashed local assembly for the measured
  vectors only. It does not establish historical engine or NuGet numeric parity.
  Measured vectors are in `tools/swarm-c/avatar-cooker/source-probe-results.txt`.

## Provider boundary and coordinate policy

`normalized.rs` mirrors the concrete B decoded Skeleton/Bone, Mesh/BoneBinding,
MeshVertex/BlendVertex, Animation/Motion, properties, appearance, binding, outfit,
and integer resource key fields. C calls the mesh DTO `SourceMesh` and the real
vertex DTO `SourceVertex` to distinguish them from core output geometry.
`F32Bits` serializes as a u32, preserving every binary32 bit, including negative
zero. Channel flags are active only when their source byte is 1.

B has already applied FreeSO's import conversion: vectors negate X;
quaternions preserve X and negate Y/Z/W. C requires `CoordinatePolicy::FreeSo`
and never applies this involution again. `CoordinatePolicy::Source` is rejected.
Source-normalized signed zero remains in the immutable DTO and local pose.
Raw blend integer weights become `raw_weight as f32 / 32768.0` without clamping.
UVs, face order, original bone indices and property record order are preserved.

B remains the standalone TSO binary decoder. These DTOs do not claim that B's
HandGroup, texture pixels, TS1/BCF/CFP bindings, content digests, or visual SLOT
providers already exist. A provider can map the inspected decoded fields
without a second byte parser. The format and DTO field order are C-local;
providers must explicitly agree on a version before serializing between crates.

`Clip::new_resolved` accepts A's resolved `metadata.resource` separately from
B's internal animation name. Association checks resource names with ASCII case
folding, preserves whitespace, checks num_frames, and checks the effective
source digest. A's AnimationKey is ingestion identity rather than resource
lookup. `Clip::new` uses the internal name as a convenience for synthetic
fixtures; real providers should use `new_resolved`.

## Source decisions and coverage

Source paths below are relative to the pinned original checkout. `VB` means
`TSOClient/tso.vitaboy.model`, `VE` means `TSOClient/tso.vitaboy.engine`.

| Source anchor | Implemented behavior | Evidence / limits |
|---|---|---|
| `VB/Skeleton.cs:27–60,76–98,212–224` | Exact skeleton/parent names; encounter-order children; original file palette order. Absolute column matrices are `parent * translation * rotation`, the transpose of source `rotation * translation * parent`. | `rig_mesh` and extracted source probe. Duplicate names, case-fold collisions, cycles, missing/inconsistent parents/children, invalid indices and unreachable bones fail admission. |
| `VB/Mesh.cs:67–110` | Mesh bone names use ASCII case-insensitive matching; this covers B's admitted ASCII names. Overlapping ranges apply in hierarchy traversal then binding order. Blend destinations overwrite in record order. | Different rig layouts hash to different prepared cache keys; duplicate blend record test asserts last record wins. A missing binding bone or unbound vertex is an explicit fault rather than default joint zero. |
| `Vitaboy.fx:73–85`; `VB/Mesh.cs:251–256` | Two distinct bone-local positions; no inverse bind multiplication. `NormalPolicy::SourcePrimary` transforms only the primary normal. Zero real normal becomes `(0,1,0)`; secondary zero stays zero. World normal is not silently normalized. | Analytic skin point `(1.5,.75,0)`, weights `-.5` and `1.220703125`, primary normal, original winding and UV fixtures. |
| `Vitaboy.fx:9` | Default maximum palette is 50; oversized rigs fail rather than truncate. CPU callers may explicitly choose another bounded limit; engine shader agreement is then required. | Palette limit fixture. |
| `VE/Animator.cs:86–140` | Integer frame clamps to animation bounds, short tracks hold last sample, negative remainder does not interpolate. Exact animation bone names; missing tracks/bones retain existing local channels. Channel metadata and order remain intact. | Reverse, short-track, exact-name and retained-channel fixtures; extracted RenderFrame vectors. Active zero-frame tracks are rejected before sampling. |
| `VMAvatar.FractionalAnim:726–748` | Ordered cumulative-prefix mixing. Ended layers contribute prefix weight without channel writes; carry follows ordinary layers at its frozen integer frame. | Weights 1/3 produce 6, ended prior layer produces 7 from retained 4 toward 8, carry overrides to 2. C never wraps or advances A's loop/frame state. |
| `VE/Animator.cs:143–171`, `VE/SimAvatar.cs:18–27` | Neck-relative target transform, +/-65 degree horizontal and +/-45 degree vertical clamps; 0.2 radians per admitted 30 Hz frame smoothing. HEAD override can follow carry sampling. | Clamp and antipodal/no-step fixtures, hashed assembly vectors at 5e-7 component tolerance. A owns fade state, target lifetime, timeout and seek weight; C samples only fractional fade. |
| `AvatarComponent.cs:31–47,91–100,215–235` | Avatar slot 0/1/2 maps R_FINGER0/HEAD/PELVIS. Positional slot space uses swap-YZ, /3, explicit scale, facing+pi and half-tile subtraction. Draw transform uses facing pi-direction and explicit slope/scale. | Bone `(3,6,9)` at scale2/facing0 and tile `(10,20,0)` gives `(7.5,13.5,4)`. Bone rotation is not an invented held-object orientation. |
| `ObjectComponent.cs:177–187`, `SLOT.cs:15–26,94–97` | Raw visual offsets /16,/16,/5, source height table and read-time 0=>5 category; avatar occupants suppress SLOT z then add center/scale offset. | Seat/chair fixtures ensure seated height is not added twice. VisualSlot is a provider input, not A's support/capacity slot definition. |
| `EntityComponent.cs:88–174` | Pure relative-container interpolation snaps when displacement exceeds 1.5 tiles; admitted interruption resets retained interpolation. | ContactInterpolation plus generation-aware, atomic AttachmentRegistry; absent participants and attachment cycles cannot install. No gameplay owner changes. |
| `VB/Outfit.cs:37–83`, `VE/SimAvatar.cs:143–275,385–443,494–499` | Three skin appearances; hand file zero falls back to file37/type18. File order is idle/fist/pointing while gesture enum is Idle0/Pointing1/Fist2/None3. None selects no hand. Both hands are recomposed from admitted state. | All skin IDs and all gesture/file mappings tested. Body/head/decorations/accessories retain group/file/type provenance; binding Bone is not applied a second time. |
| `VE/Avatar.cs:61–89`; `VE/SimAvatar.cs:199–212,421–431` | Repeated accessory role+appearance pairs deduplicate. Literal TS1 right Fist/Pointing retains Idle texture; literal hand inputs correspond to resolved Light slot. | Explicit TS1 quirk fixture. B still needs the actual TS1 rig/appearance provider. C atomically recomposes all skin parts; it does not reproduce the source's head-only Appearance setter order quirk. |
| `UISim.cs:46–65,94–119,122–155`; `UIIconCache.cs:56–134` | Bounded CPU reference preview scenes and image/depth/ID targets. Preview key includes rig, mesh, texture, role, skin, pose, size and renderer generation. Closing one scene releases its targets while shared immutable meshes stay valid. | 20 two-scene open/close/reset cycles and real rasterized nontransparent pixels. Diagnostic colors are used; texture pixels, GPU handles and framework callback disposal belong to the engine adapter and remain acceptance gates. |
| `WorldEntities.cs:66–95`; `AvatarComponent.Draw` | Visibility/story/bounds culling and explicit distance-based pose cadence. Required visible-dependent endpoints update even when their holder is culled. Budget overflow for required endpoints is an explicit error. | Stable, input-order-independent 32/64 plans and endpoint checks. LOD never suppresses simulation or cues. No claim that source shipped mesh LOD. |
| `GLTFExporter.cs:20–70,114,207,212–225`; `GLTFImporter.cs:214–260,400–423` | C sidecar preserves dual positions and ordered motion/property metadata instead of assuming generic one-position skinning is lossless. | Cook round trip preserves resource IDs, negative zero, duplicate/unsorted and signed marker records. No glTF round-trip fidelity claim. |

## Declared numerical and admission policy

Nonfinite source vectors/normals/quaternions, invalid ranges, invalid palette
indices, singular neck matrices, and finite source hierarchies whose composed
matrices overflow fail explicitly. Bone and channel quaternions must have
squared norm within .001 of 1; strongly nonunit source content is rejected
rather than silently normalized during ingestion. Core Slerp uses validated
unit inputs, shortest path interpolation, a near-angle branch and output
normalization. This remains a deliberate numeric policy until the pinned
historical math assembly is verified. The local assembly source vectors pass
component tolerance 5e-7; bit identity is not claimed.

A cumulative layer prefix exactly zero causes **no channel writes**. The source
can compute `0/0` and poison retained channels; C preserves prior channels in
that case. No generic weight clamp is applied. Negative finite layer weights
are accepted by sampling where they produce a valid pose; conservative channel
envelopes require nonnegative/convex mixing. The exact current skinned bounds
remain available for externally authored/extrapolated poses.

A non-ROOT root is rejected because the source's mesh finalization is triggered
specifically by ROOT. Case-fold-ambiguous bone names are rejected so mesh binding
is unambiguous. These are explicit narrowed content policies beyond B's decoder.

`PreparedMesh::conservative_animation_bounds` sums maximum supplied local
translation radii through parent chains, allows arbitrary unit bone rotations,
retains absolute coefficients for unclamped skin weights, and allows a .005
per-joint stretch plus binary32 margin. It is a conservative sphere/AABB,
not tight animated bounds or an invented collision volume. `bounds` returns the
exact current skinning bounds. Callers must include every relevant clip or use
an explicit larger envelope; an omitted clip cannot be culled safely by an
unrelated envelope.

## Timeline and state ownership

`PosePlayer` retains local channels once per admitted presentation tick. Draws
sample copies of the pre-commit baseline; repeating a timestamp cannot repeatedly
blend onto a prior draw. Duplicate tick commits return false. Stale ticks fail
without mutation. Explicit entity/generation/lot/restore boundaries call reset;
initialization/resync returns to bind pose because A does not persist the legacy
visual skeleton. That baseline strategy is documented, not exact restored
legacy visual parity.

The A-linked probe generates an isolated temporary manifest, verifies the A
commit, extracts only read-only test helpers, then advances genuine A runtimes.
For 60 ticks, state hashes and ordered RuntimeEvent vectors match with C absent
and with C drawn at 30/60/120 Hz. Two actual A animation cues execute in every
run, and replayed accepted ticks add zero events/cues. The C pose asset is
synthetic and its resolved name/frame count/digest association is explicit.
Neither the shipping crate nor this adapter exposes a gameplay event producer,
completion result, simulation command, RNG, or timeline tick function.

## Appearance, preview and resource boundaries

`AppearanceCatalog::compose` resolves the full graph into a staged bundle before
installation. Missing mesh/texture selectors, incomplete dependencies and
budgets fail. The same prepared resource shares an Arc across roles within one
bundle. `AppearanceState` uses monotonic request tokens plus EntityRef generation:
late async completions cannot overwrite newer requests or reused entities. A
successful install replaces every part together; failure leaves the old bundle.
Clear/reset invalidates pending tokens and releases owned references.

PreviewPool holds bounded CPU target buffers and shared bundles. Device reset
invalidates its surfaces and keys while retaining immutable appearance/pose
inputs; rerender recreates them. No framework callback is registered by this
pure package, so there is no hidden callback lifetime to count. Engine GPU and
callback lifecycle evidence must be supplied by the engine adapter.

`WCAV0001` contains an eight-byte magic, LE payload length, SHA256 payload digest,
and bounded fixed-int bincode payload. The cooked payload retains normalized
rig, mesh, animation and their resource IDs/source digest. It is not an outfit
resolver or a complete licensed asset archive; decoded outfit/appearance/
texture dependency graphs are B-resolved inputs. Signed negative time-property
IDs survive round trip. `validate_a_time_ids` rejects them for A's u32 bridge
instead of wrapping to a large timestamp. Decode checks bytes, digest, record
limits and renderer admission before use.

## Synthetic workload evidence

Recorded on 2026-10-05, x86_64 Linux container exposing
INTEL(R) XEON(R) PLATINUM 8573C, Rust 1.75 release profile with overflow checks.
This is one CPU fixture run, without a graphics backend, browser, textures,
physical device or GPU pass. Timings are observational, not a performance gate.
The mannequin has 14 bones, 312 vertices, 156 triangles, 312 dual-position blend
records and 26,832 bytes of shared prepared mesh data. Every actor has separate
retained presentation pose state; immutable geometry is shared.

| Actors | Ticks | Palette samples | Draw instances | Endpoint checks | Pose commit/sample CPU us | CPU skin us | GPU pass |
|---:|---:|---:|---:|---:|---:|---:|---|
| 32 | 60 | 1,020 | 1,440 | 240 | 8,743 | 39,465 | Unmeasured |
| 64 | 60 | 1,560 | 2,160 | 480 | 13,586 | 51,576 | Unmeasured |

`tools/swarm-c/avatar-cooker/crowd-results.txt` records the exact workload.
Palette samples count draw-pose updates; committed pose retention still runs
once per admitted tick. This fixture deliberately preserves required endpoints
within a maximum of 32 draw-pose updates per tick. It is not a production avatar
crowd benchmark. Root's engine/browser and native/WASM probes own their distinct
backend evidence.

## Remaining integration and acceptance gates

- Actual licensed content cohorts, B texture pixels/material semantics, HandGroup
  and TS1/BCF/CFP rig/appearance providers, and effective per-resource digests.
- Exact historical MonoGame 3.6.0.1625 Slerp rounding/branch verification and
  nonunit content compatibility decisions.
- A's exact continuous route visual start/velocity/radian direction/turn velocity
  and B's raw visual container SLOT records. Fixed subtiles/eight facing notches
  cannot establish exact source continuous foot motion. C does not invent an IK,
  route, reach action or slot owner to fill those gaps.
- Real chair/bed/stair/portal and multitile content pose/contact fixtures; the
  implemented visual slot algebra covers supplied records, not missing content.
- Legacy visual skeleton restoration and setter-order parity beyond the declared
  retained-pose and atomic appearance policy.
- Engine texture/GPU/callback/device-loss lifecycle, 32/64 production-resource
  GPU timing, real browser and physical-device acceptance. CPU reference preview
  and synthetic crowd success cannot close these gates.
