# Normalized FSOm object meshes and source draw contracts

This W08.2 adapter turns a bounded, decoded object model into immutable
`render-core::Mesh` buffers, resolved texture bindings, ordered draw commands,
and generation-fenced replacement state. The implementation is
[`objects.rs`](../../crates/render-3d/src/objects.rs); its regression and source
probes are in [`tests/objects.rs`](../../crates/render-3d/tests/objects.rs).

The adapter consumes B's normalized content. It does not parse IFF, gzip, FSOm,
SPR, MTEX, PNG, or replacement-directory files. It also does not assert that a
real provider, an authorized asset cohort, or an engine's GPU implementation
has been exercised. Those boundaries are described below so the normalized
adapter's completed work can be integrated without overstating end-to-end
readiness.

## Source anchors

The source files used here are the original files in the Wonderland fork:

| Source | Behavior carried into C |
| --- | --- |
| [`FSOM.cs`](../../TSOClient/tso.files/Formats/IFF/Chunks/FSOM.cs) | The IFF wrapper owns compressed data until it constructs and caches `DGRP3DMesh`. B retains that byte-decoding responsibility. |
| [`DGRP3DMesh.cs`](../../TSOClient/tso.files/RC/DGRP3DMesh.cs) | Format versions 1–3; reconstruction version 0 for authored meshes and rejection of old nonzero versions below 2; ordered geometry groups; v3 normal/portal mask; serialized source bounds. Group 0 is static and generated dynamic group index is `1 + spriteID - DynamicSpriteBaseId`. |
| [`DGRP3DGeometry.cs`](../../TSOClient/tso.files/RC/DGRP3DGeometry.cs) | Per-part geometry, texture binding, triangle order, v1 normal generation, `PixelSPR` ordinal, `PixelDir` rotation, and custom texture sentinel 65535. |
| [`DGRP3DVert.cs`](../../TSOClient/tso.files/RC/DGRP3DVert.cs) | Source area-weighted normal accumulation and f32 normalization. |
| [`RCMeshProvider.cs`](../../TSOClient/tso.content/RCMeshProvider.cs) | Selected-source and texture-provider boundaries. C preserves the selected effective identities; this module performs no filesystem lookup. |
| [`DGRPRenderer.cs`](../../TSOClient/tso.world/Utils/DGRPRenderer.cs) | Two dynamic 64-bit banks, static and portal visibility, ordered mask passes, room-dependent shader choice, texture UV scale, lightmap exclusion, and depth/stencil initializers. |
| [`ObjectComponent.cs`](../../TSOClient/tso.world/Components/ObjectComponent.cs) | Exact `World3D` scale, yaw, tile-to-world mapping, center offset, source bounds transformation, and render elevation offset. |
| [`RCObject.fx`](../../TSOClient/tso.content/ContentSrc/Effects/RCObject.fx) | Vertex UV multiplication; linear clamp texture sampler; basic, directional, disabled, depth-clear and lightmap shader policies. |
| [`WorldEntities.cs`](../../TSOClient/tso.world/WorldEntities.cs) | Object `NonPremultiplied` blending and unculled object drawing. |
| [`LMapBatch.cs`](../../TSOClient/tso.world/LMap/LMapBatch.cs) | Object/outside shadow targets have `DepthFormat.None`; `MaxBlendGreen` writes green and alpha. |

The first-mask-pass comment in `DGRPRenderer` says “no depth write,” but its
actual `DepthClear1` initializer sets `DepthBufferWriteEnable = true`. This
adapter follows the executable initializer. Its first pass retains rasterized
depth; only the second depth-clear shader writes the far depth value 1.

## B input and immutable prepared output

`NormalizedFsom` carries the selected FSOm's `ObjectMeshIdentity`, its DGRP or
standalone context, format/reconstruction versions, original group order, a
resolved texture table, source body bounds, and an optional normal/portal mask.
`effective_source` identifies the selected patched FSOm bytes;
`effective_content` identifies the content/patch set in which the model was
resolved. A DGRP context additionally preserves its effective IFF identity and
chunk ID.

Each group contains `NormalizedPart` values that reference the texture table
by index. Empty groups remain in place, so removing an empty group cannot shift
the meaning of either dynamic sprite bank. Each source texture binding retains
its selector, effective asset key, decoded straight-alpha pixels, and
`TextureInfo.UVScale`. The selector distinguishes a DGRP sprite's rotation and
sprite **ordinal** from a custom texture ID. An ordinal is not a sprite ID.
Custom replacement textures and embedded MTEX textures can therefore use the
same rendering path after B resolves the source's lookup precedence.

`PreparedFsom::prepare` validates the complete input before publishing output.
Its geometry conversion preserves source vertex positions, UVs, triangle
winding/order, group order, part order, and texture binding. FSOm has no vertex
color field; the resulting `Mesh` uses white vertex colors. Version 2 and 3
normals remain unchanged. Version 1 uses the original accumulated triangle cross
products and f32 normalization; degenerate or overflowing inputs that would
produce nonfinite legacy normals are rejected.

Prepared fields are private and exposed through immutable references. Neither
the content key nor validated meshes can be retagged or changed through a public
mutable field. The derived SHA-256 key includes the selected effective source
and content identities, context, versions, source bounds, texture selectors and
effective identities, actual image pixels, UV scales, group/part delimiters,
actual vertex/index buffers, and mask type/geometry. A caller changing normalized
data while reusing a provider's declared source key still changes the prepared
content key. This does not independently authenticate B's claim about original
encoded bytes; it makes C's own disposable derivation sensitive to its actual
inputs.

The adapter rejects unsupported format versions, obsolete reconstructed data,
wrapped negative or otherwise out-of-range signed-Int32 reconstruction versions,
more than 129 groups, inconsistent v1/v2 masks, invalid mask kinds, duplicate
texture selectors or same-texture entries within one source group, invalid
texture references, out-of-range indices, incomplete triangles, malformed
images, nonfinite attributes, invalid UV scales, and invalid body bounds.
Declared body bounds must contain body vertices. A portal mask may extend beyond
them, matching the original source's deliberate exclusion of portal-mask
vertices from object bounds.

`ObjectLimits` bounds total parts and textures as well as aggregate vertex,
index, pixel and retained buffer counts. Geometry limits include the depth mask;
pixel limits include all texture-table entries. Accounting occurs before output
mesh allocation. The buffer budget describes retained vertex/index/pixel
storage; separately bounded metadata and B's already-decoded input allocation
are not mislabeled as GPU memory. An atomic replacement may temporarily retain
the old complete model while validating the new bounded model.

## Scene commands, materials and coordinates

`PreparedFsom::scene(ObjectInstance, ObjectTarget)` returns an `ObjectScene`
with immutable borrowed meshes and materials, explicit object/entity identity,
visual revision, room, shader level, world matrix, transformed object bounds,
transformed render bounds, and ordered `ObjectDraw` commands. Commands include
their original group/part ordinal, shader kind, blend mode, depth comparison,
depth-write flag, optional forced fragment depth, both stencil faces, and
culling policy. No geometry or texture is silently flattened into a single
untextured object.

For an ordinary object, the original row-vector world expression is:

```text
Scale(3) * RotationY(-direction) *
Translation((1.5, 0.1, 1.5) + (tileX*3, tileZ*3, tileY*3))
```

C emits the equivalent matrix in `render-core`'s column-vector convention.
Local mesh coordinates are already Y-up. The scale and axis conversion must not
be applied again by an engine adapter. Source object bounds remain distinct from
render bounds, which also include an outlying mask. Scene admission validates
the transformed body, mask and normal envelopes before returning any commands,
so finite but extreme inputs cannot produce infinite uploaded coordinates or
normals. Normal envelopes are computed during preparation rather than rescanned
for every instance.

`shader_level` is `Level - 0.999`. Room 65533 selects the disabled shader;
directional lighting applies only when both source lighting switches are enabled
and room is below 65533. Rooms 65534 and 65535 use the basic shader. The disabled
shader's eventual engine implementation must preserve the source post-lighting
grayscale weights `(0.2989, 0.5870, 0.1140)` and its lack of the ordinary 0.01
alpha discard.

`PreparedTexture::shader_uv` executes the source multiplication of unscaled
vertex UV by `UVScale`. It does not clamp vertices before interpolation.
`sampling()` explicitly requires linear minification, magnification and mip
filtering with U/V clamp. Actual image filtering, source lighting, mip generation
and GPU shader execution belong to the consuming renderer.

Object blending uses the exact MonoGame `NonPremultiplied` state: source factor
`SourceAlpha`, destination factor `InverseSourceAlpha`, and addition, for both
RGB **and alpha**. Thus source alpha 0.5 over destination alpha 0.75 produces
alpha 0.625 in this render target. `ObjectBlend::blend_rgba` is an executable
oracle for these normalized, already-shaded samples. It also implements mask
color suppression and the source green/alpha maximum blend for shadow targets.

## Dynamic groups and depth masks

Static group 0 is always considered. Groups 1–64 use bits 0–63 of the first
dynamic word; groups 65–128 use bits 0–63 of the second word. Only a portal's
literal final group is forced visible and stencil-restricted, including when
that is group 0. An empty final group does not transfer portal treatment to a
previous nonempty group. Parts with no triangles produce no draw call.

For a nonempty normal mask the draw order is:

1. Draw both mask faces with ordinary less-equal depth and depth writes enabled.
   A depth-passing clockwise face writes stencil zero; a counterclockwise face
   replaces stencil with one. Failed depth tests preserve stencil.
2. Draw the mask with forced far depth 1, unconditional depth comparison,
   stencil-equals-one, depth writes enabled, and no color writes. This clears
   stencil on passing/depth-failing fragments.
3. Draw the visible object groups normally.

For portals, the second pass keeps stencil one. The final group then draws only
where stencil is one, with the ordinary depth test/write behavior. A final mask
pass clears stencil without writing color or depth. Mask prepasses and portal
cleanup still run if the object contains zero body groups. An empty mask mesh
emits no mask draw, matching the original absent vertex-buffer behavior.

`ObjectDraw::apply_depth_stencil` executes this state machine for an already
rasterized fragment. It applies the ordinary shader's strict alpha-below-0.01
discard before stencil/depth effects, preserves the disabled shader exception,
and validates fragment inputs before mutation. This is a numeric state oracle;
it does not claim full triangle rasterization or GPU image parity.

Lightmap targets skip all portal objects. For ordinary objects they emit visible
body parts with the lightmap shader, no mask passes, and no texture sampling.
`DrawLMap` overwrites the world matrix's Y translation with
`((objectLevel - lightmapLevel) - 1) * 2.95 + yOffset`; the source does not multiply
that overwrite by three. Signed level subtraction is widened before arithmetic.
The actual object/outside shadow targets have no depth attachment, so these
commands use unconditional depth comparison and disable depth writes. Every
overlapping fragment therefore reaches the green/alpha maximum blend.

## Atomic dynamic replacement

`ObjectMeshSlot` owns one current model and an optional pending request. It uses
opaque `ObjectMeshTicket` values containing session generation, internal reset
generation, entity reference/generation, content generation, request serial and
effective content identity. Each new request increments its serial; even an
A→B→A content transition cannot revive an old A completion. Content generations
cannot move backward within a slot boundary. Reusing an installed ticket is
rejected, and a completion from another entity, reset, session or effective
content set is rejected before preparation.

`install` validates and prepares a candidate before replacing the current
model. Malformed data retains the prior mesh, derived key and installed
generation, and leaves the pending ticket available for a corrected completion.
`reset` removes the current and pending models while advancing a private reset
generation. Reconstructing a whole slot/controller requires a fresh nonzero
session generation supplied by its owner. The slot never advances simulation
time, changes A's events or authoritative state, or grants service admission.

## Evidence and remaining integration

The adapter adds 26 Rust tests. They cover source normal vectors, material/group
preservation, texture UV policy, source blend equations, normal and portal
depth/stencil transcripts, portal/group edge cases, both dynamic flag banks,
world/lightmap equations, room shader selection, malformed and aggregate-budget
rejection, finite-output checks, and session/reset/content replacement fences.
The complete render-3d suite passes 105 tests in both debug and release after
this addition.

The source numeric check compiled the unchanged `DGRP3DVert.cs` with `mcs` and
executed it under Mono against the repository's MonoGame assembly. Four source
normal vectors from triangles `(0,1,2)` and `(0,2,3)` over positions
`(0,0,0), (2,0,0), (2,0,1), (0,3,1)` match all twelve float bit patterns in the
Rust test. The same executable confirmed source world-transform output,
`NonPremultiplied` factors, and default less-equal/depth-write state.

| Executed input | SHA-256 |
| --- | --- |
| `DGRP3DVert.cs` | `0ed99a523ec4f8aa14b2c35458ade6afc09918dcbd553bc0e7907a5d7113c7f7` |
| Repository `MonoGame.Framework.dll` | `dbb1c9203614fcea82e5a4e66df288f8144288b6adbea2b2ee525e4afa4a433d` |

The independently reviewed lightmap depth-attachment discrepancy was reproduced
as a failing regression and corrected: its far overlapping fragments now reach
the maximum blend without changing depth. Engine GPU parity is a separate gate.

B must supply bounded decoded FSOm geometry, source metadata, resolved custom
PNG/MTEX or sprite textures, correct effective identities and authorized content.
This adapter preserves those resolved bindings; it does not itself execute
custom-PNG-before-MTEX fallback, file replacement precedence, archive decoding,
texture fetching or provenance authentication. C must connect the prepared
scene commands to the selected engine's actual shader, stencil and texture
resources and exercise a representative patched-content cohort. The existing
single-mesh reconstruction resolver does not automatically become a full
multi-material FSOm provider merely because this normalized adapter now exists.

These are explicit remaining integration responsibilities. No synthetic model,
hash declaration, standalone shader enum or fragment-state oracle is recorded
as proof of real provider use or completed GPU rendering.
