# C-local presentation contract v1

This is C's internal contract pending adoption by the global contract owner.
No renderer data or handle enters authoritative simulation state. All crates
use package names `wonderland-render-core`, `wonderland-render-iso`,
`wonderland-render-3d`, `wonderland-avatar-view`, and `wonderland-audio-runtime`.
Each library is independently buildable with its own `[workspace]` manifest.

## Common Rust API

`wonderland_render_core::math` provides the following owned, `Copy` f32 types,
all with `Debug`, `PartialEq`, and serde support:

```rust
pub struct Vec2 { pub x: f32, pub y: f32 }
pub struct Vec3 { pub x: f32, pub y: f32, pub z: f32 }
pub struct Quat { pub x: f32, pub y: f32, pub z: f32, pub w: f32 }
pub struct Mat4 { pub cols: [[f32; 4]; 4] }
pub struct Aabb { pub min: Vec3, pub max: Vec3 }
pub struct Ray { pub origin: Vec3, pub direction: Vec3 }
```

Vec2/Vec3 expose `new`, `ZERO`, `is_finite`, arithmetic Add/Sub/Mul<f32>/Div<f32>
and Neg. Vec3 also exposes `ONE`, `X`, `Y`, `Z`, `dot`, `cross`, `length`,
`length_squared`, `normalize_or_zero`, and `lerp`. Quaternion identity is
`Quat::IDENTITY`; operations are `new`, `from_axis_angle`, `normalize`,
`conjugate`, `slerp`, `rotate_vec3`, and Mul<Quat>. Invalid normalization or
axis-angle input returns `Option<Quat>`; slerp expects validated unit inputs.

Matrices are column-major and multiply column vectors. `Mat4::IDENTITY`,
`from_translation`, `from_scale`, `from_quat`, `from_trs(translation, rotation,
scale)`, `transpose`, `inverse() -> Option<Mat4>`, `transform_point3`,
`transform_vector3`, `transform_vec4([f32;4]) -> [f32;4]`, and Mul<Mat4> are
available. `perspective_rh(fov_y, aspect, near, far)` and
`orthographic_rh(left,right,bottom,top,near,far)` return `Option<Mat4>` and use
right-handed view space, looking down -Z, with clip depth 0..1.
`look_at_rh(eye,target,up)` returns `Option<Mat4>`.

The equivalent of source row-vector `localRotation * localTranslation * parent`
is column-vector `parent * translation * rotation`. This conversion happens
once. Aabb has `new(min,max) -> Option<Aabb>`, `from_points(&[Vec3])`,
`contains`, `intersects`, `transformed(Mat4) -> Option<Aabb>`, and
`ray_interval(Ray) -> Option<(f32,f32)>`.

`units` contains named conversions and constants. A visual tile position already
including terrain/container elevation maps `(x,y,z)` to `(3*x,3*z,3*y)`.
Story height is 2.95 tile units. Terrain height factor is 3/160 tile units.
DGRP/SLOT offsets use x/y divided by 16 and z divided by 5 before the appropriate
coordinate mapping. Render adapters must not add terrain twice.

## Identity, frames, meshes, and images

```rust
pub struct EntityRef { pub object_id: u32, pub generation: u32 }
pub struct AssetKey(pub [u8; 32]);
pub struct FrameStamp {
    pub lot_id: u64, pub epoch: u64, pub tick: u64,
    pub architecture_revision: u64, pub content: AssetKey,
}
pub enum ViewMode { Full2D, Hybrid2D, Full3D }
pub struct Transform {
    pub translation: Vec3, pub rotation: Quat, pub scale: Vec3,
}
pub struct EntityProjection {
    pub reference: EntityRef, pub visual_revision: u64,
    pub transform: Transform, pub previous_transform: Option<Transform>,
    pub asset: AssetKey, pub level: i16, pub visible: bool, pub selectable: bool,
}
pub struct RenderFrame {
    pub stamp: FrameStamp, pub entities: Vec<EntityProjection>,
    pub selected: Option<EntityRef>,
}
pub struct Vertex {
    pub position: Vec3, pub normal: Vec3, pub uv: Vec2,
    pub color: [f32; 4],
}
pub struct Mesh { pub vertices: Vec<Vertex>, pub indices: Vec<u32> }
pub struct RgbaImage { pub width: u32, pub height: u32, pub pixels: Vec<[u8;4]> }
```

These public types are re-exported at the render-core crate root. Mesh carries
CPU data; material association lives on the draw instance. `Mesh::validate` and
`RgbaImage::validate` enforce explicit `RenderLimits`. Default limits allow
65,535 entities, 2,000,000 vertices, 6,000,000 indices, 4,096 image dimension,
and 16,777,216 texture pixels; callers may set smaller budgets. Validate finite
positions/normals/UV/colors and index ranges before allocations/uploads.

`Transform::IDENTITY`, `Transform::matrix()`, and
`Transform::interpolate(self, next, fraction) -> Option<Transform>` support
presentation sampling. Invalid fractions or transforms fail explicitly.

Frame admission is atomic. Duplicate object IDs, zero generations, invalid
transforms, stale epochs/revisions, and inconsistent selection cannot partially
replace a displayed frame. A new lot/epoch is admitted by an explicit reset
boundary. Full frames may skip presentation ticks; they do not admit simulation
commands. Ordering/replay identity is separate from view cadence.

## Picking and resource lifetime

Picking returns the game EntityRef, never a GPU entity ID. Tickets bind lot,
epoch, content, entity generation and visual revision. An old ticket may survive
an unrelated entity update but must fail after target movement/deletion/reuse,
lot reset, or content/device generation changes.

Derived cache keys include effective source hash, content/patch identity,
algorithm version and parameter bytes. Bounded residency accounts for encoded,
decoded CPU, staging, and GPU bytes separately. Eviction releases owned handles;
device reset invalidates GPU handles while immutable decoded data may survive.

## Ownership across libraries

- render-iso owns source sprite/DGRP, depth, material/light, batch, cutaway and
  mode-transition types. It uses core math/images/meshes/identities.
- render-3d owns explicit visual lot input, architecture/reconstruction/city,
  camera/environment/quality types. Missing A visual cosmetics are inputs;
  their absence is not filled with invented legacy data.
- avatar-view owns normalized rig/mesh/appearance/animation/contact types.
  Its provider adapters retain B's resource IDs and conversion policy.
- audio-runtime owns HIT/FSC/program, cue and mixer types. It may use core
  FrameStamp/EntityRef/AssetKey/Vec3 but does not depend on rendering engines.
- Engine adapters consume these pure outputs. They never become providers of
  animation events, world legality, authoritative movement, or durable effects.

Public changes to this contract are coordinated by root before consumers change.
