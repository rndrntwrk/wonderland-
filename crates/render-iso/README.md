# wonderland-render-iso

Rust 1.75 CPU/reference isometric presentation for Swarm C. This independent
package depends on `wonderland-render-core` and contains no renderer engine,
simulation, original assets, or content decoder. All test images are synthetic.

## Pipeline

1. Adapt normalized B `DrawingGroup`, sprite-frame bytes, effective content keys,
   and OBJD dynamic-sprite fields into `DgrpImage`, `DgrpLayer`, `SpriteAsset`,
   and `DgrpInstance`. `sprite_offset` is the source-compatible f32 value;
   `object_offset` remains raw content coordinates. SPR2 `frame.position` is not
   DGRP placement. Preserve straight RGBA and raw, uninverted depth bytes.
2. Build `Projection` for a discrete zoom, quarter rotation, precise zoom,
   resolved visual center, and framebuffer viewport. Resolved tile positions
   already contain terrain/container height.
3. Call `prepare_sprites` with `PreparePolicy`. It validates dimensions and
   limits, selects the first rotated DGRP match, applies the two dynamic masks,
   places logical rectangles, pairs flipped color/depth UVs, computes depth
   anchors, and emits immutable shared assets, quads, room/floor metadata,
   material keys, and optional bounds.
4. Use `make_batches` or `make_batches_with_limits` within each scene draw phase.
   These preserve stable depth ties and group adjacent compatible materials.
   `make_software_batches` additionally splits overlapping rectangles into
   contiguous runs. Keep architecture, restored static surface, avatar, and
   dynamic-object phases under the integration adapter's control.
5. An engine consumes `shader_vertices`, `MaterialKey`, `DepthInput`,
   `DepthAnchors`, and `lighting_bindings`. `PreparedSprite::fragment` is the
   CPU color/depth oracle at logical texel coordinates. `shader_vertices` takes
   a positive source-compatible pick code allocated by the generation-aware
   core pick table, independently of the game object ID.

`cutaway` computes wall cuts, rotated neighbor cuts, door/style decisions,
principal wall depth images, and object hiding. `lighting` computes atlas
allocation/UVs, explicit point and room-light arithmetic, ultra blur recipes,
provider footprint selection, source-compatible clustering, and dirty-room
budgeting. `invalidation` emits dependency actions for core-owned caches.
`view` preserves camera/selection intent and samples transitions without VM
events or RNG use.

## Engine-facing contract

Framebuffer positions use a top-left origin and increasing downward Y. Sprite
UVs point into physical padded textures; visible width/height remain logical.
Sampling color, raw depth, masks, and ambient rooms is point/clamp. Advanced
lighting and direction textures are linear/clamp. Depth is one explicit unorm
channel: a browser adapter should use R8/red and avoid implicit source Alpha8
swizzle assumptions. `DepthInput::None` selects a no-depth technique and clears
the prior binding; `Constant(128)` is an actual depth resource.

The shader data ABI is an explicit array of 12 f32 values per vertex:
screen position 3, UV 2, graphics-world anchor 3, pick code/floor 2, room UV 2.
`SpriteShaderVertex::floats` avoids relying on Rust struct layout or unsafe
casts. Each sprite's front/back clip depth and W remain available separately.

Color enters as straight RGBA. The material oracle applies source gamma and
premultiplies once. The target default uses premultiplied blending consistently;
`LegacyDynamicNonPremultiplied` is an explicit source discrepancy comparison.
Automatic engine sRGB conversions must not be applied on top of the source
transfer functions. Cached restore input is already premultiplied.

Effective color/depth/mask keys must include provider and cook identity.
`LightingResources.revision` must change when effective global light uniforms
or fallback values change; its resource keys include all light textures. Core
owns resident GPU handles, content/device generations, and cache eviction.

## View policy

Settled Full2D uses sprites for objects and architecture. Settled Hybrid2D uses
sprites for objects and geometry architecture, a deliberate target policy.
Full3D, active transitions, and nonzero rotation offsets require geometry and
immediate rendering. The geometry provider is supplied by `render-3d` through
the integration adapter; this crate neither invents geometry nor changes
simulation placement rules. Desired 2D zoom and selected live identity survive
view switches; Full3D's sprite resource zoom is Near.

## Verification and limits

```sh
cargo test --offline --locked --manifest-path crates/render-iso/Cargo.toml
cargo fmt --check --manifest-path crates/render-iso/Cargo.toml
```

The package has analytic and adversarial CPU tests. It contains an engine-facing
data ABI rather than unverified WGSL/GLSL shader text. No GPU shader compilation,
engine qualification, source-runtime screenshot parity, browser performance,
or physical-device acceptance is claimed. See
`docs/swarm-c/iso-source-notes.md` for source anchors, decisions, and open gates.
