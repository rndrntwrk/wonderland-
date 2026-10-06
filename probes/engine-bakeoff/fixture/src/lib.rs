//! Shared, synthetic presentation inputs for both real engine adapters.
//! No original game assets and no simulation advancement occur in this crate.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use wonderland_render_core::reference::{FragmentOptions, ReferenceSurface};
use wonderland_render_core::{
    AssetKey, EntityProjection, EntityRef, FrameStamp, Mat4, Mesh, Quat, RenderFrame, RenderLimits,
    RgbaImage, Transform, Vec2, Vec3, ViewMode,
};

mod audio;
pub use audio::audio_reference;

pub const FIXTURE_VERSION: u32 = 2;
pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 480;
const CROWD_CAPACITY: usize = 64;
const ANIMATION_PHASES: u64 = 60;
/// Two live reference surfaces plus the returned color/depth/ID buffers. Mesh
/// scratch is separately constrained by RenderLimits; dimensions are rejected
/// before allocating any framebuffer storage.
pub const MAX_REFERENCE_BUFFER_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FixtureScene {
    pub frame: RenderFrame,
    pub camera: CameraSpec,
    pub draws: Vec<DrawMesh>,
    pub sprites: Vec<SpriteDraw>,
    /// SHA-256 of the canonical fixture inputs and uploaded presentation data.
    pub hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct CameraSpec {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub vertical_size: f32,
    pub fov_y_radians: f32,
    pub near: f32,
    pub far: f32,
    pub orthographic: bool,
}

impl CameraSpec {
    pub fn view_projection(self, aspect: f32) -> Result<Mat4, FixtureError> {
        let view = Mat4::look_at_rh(self.eye, self.target, self.up)
            .ok_or_else(|| FixtureError("invalid fixture camera".into()))?;
        let projection = if self.orthographic {
            let half = self.vertical_size * 0.5;
            Mat4::orthographic_rh(
                -half * aspect,
                half * aspect,
                -half,
                half,
                self.near,
                self.far,
            )
        } else {
            Mat4::perspective_rh(self.fov_y_radians, aspect, self.near, self.far)
        }
        .ok_or_else(|| FixtureError("invalid fixture projection".into()))?;
        Ok(projection * view)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrawMesh {
    pub name: String,
    pub owner: Option<EntityRef>,
    pub mesh: Mesh,
    pub model: Mat4,
    pub material: MaterialSpec,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaterialSpec {
    pub color: [f32; 4],
    pub image: Option<RgbaImage>,
    pub unlit: bool,
    pub alpha_cutoff: Option<f32>,
    pub double_sided: bool,
}

/// Logical sprite pixels; both backends implement the same source sampling law.
/// Rect is top-left screen x/y/width/height at the fixed fixture resolution.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpriteDraw {
    pub name: String,
    pub owner: EntityRef,
    pub rect: [f32; 4],
    pub image: RgbaImage,
    pub depth: Vec<u8>,
    pub mask: Option<Vec<u8>>,
    pub back_depth: f32,
    pub front_depth: f32,
    pub lighting: [f32; 3],
    pub room: u16,
    pub mirror: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixtureError(pub String);
impl std::fmt::Display for FixtureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for FixtureError {}

pub fn representative_scene(
    mode: ViewMode,
    avatar_count: u16,
    tick: u64,
) -> Result<FixtureScene, FixtureError> {
    use wonderland_render_3d::lot::{self, BuildOptions, SurfaceKind};
    if usize::from(avatar_count) > CROWD_CAPACITY || tick == u64::MAX {
        return Err(FixtureError("fixture actor/tick limit".into()));
    }
    let camera = CameraSpec {
        eye: Vec3::new(34., 22.412415, 34.),
        target: Vec3::new(9., 2., 9.),
        up: Vec3::Y,
        vertical_size: 32.,
        fov_y_radians: std::f32::consts::FRAC_PI_3,
        near: 0.1,
        far: 200.,
        orthographic: mode != ViewMode::Full3D,
    };
    let visual_lot = lot::synthetic_lot();
    let options = BuildOptions {
        show_roofs: mode != ViewMode::Full2D,
        ..BuildOptions::default()
    };
    let architecture = lot::build_lot(&visual_lot, &options).map_err(problem)?;
    let mut draws = Vec::new();
    for (ordinal, part) in architecture.parts.into_iter().enumerate() {
        let color = match part.kind {
            SurfaceKind::Terrain => [0.75, 0.92, 0.70, 1.],
            SurfaceKind::Floor => [0.83, 0.71, 0.53, 1.],
            SurfaceKind::Wall | SurfaceKind::WallTop => [0.80, 0.77, 0.66, 1.],
            SurfaceKind::Water | SurfaceKind::Pool => [0.15, 0.62, 0.82, 1.],
            _ => [0.51, 0.22, 0.18, 1.],
        };
        draws.push(DrawMesh {
            name: format!("lot-{:?}-{ordinal}", part.kind),
            owner: None,
            mesh: part.mesh,
            model: Mat4::IDENTITY,
            material: material(color),
        });
    }
    let city = wonderland_render_3d::city::build_city_mesh(
        &wonderland_render_3d::city::synthetic_city(),
        wonderland_render_3d::city::CityBoundary::Rectangle,
    )
    .map_err(problem)?;
    draws.push(DrawMesh {
        name: "city-map".into(),
        owner: None,
        mesh: city,
        model: Mat4::from_translation(Vec3::new(-8., 0., 0.)),
        material: material([1.; 4]),
    });
    let mut entities = Vec::new();
    let (posed_mesh, placements) = posed_crowd(
        tick,
        Vec2::new(
            f32::from(visual_lot.width) * lot::TILE_UNITS,
            f32::from(visual_lot.height) * lot::TILE_UNITS,
        ),
    )?;
    for index in 0..avatar_count {
        let reference = EntityRef {
            object_id: 1000 + u32::from(index),
            generation: 1,
        };
        let position = placements[usize::from(index)];
        let x = position.x / lot::TILE_UNITS;
        let y = position.y / lot::TILE_UNITS;
        let height = visual_lot.contact_height(x, y, 1).map_err(problem)?;
        let transform = Transform {
            translation: Vec3::new(position.x, height, position.y),
            ..Transform::IDENTITY
        };
        draws.push(DrawMesh {
            name: format!("avatar-{index:02}"),
            owner: Some(reference),
            mesh: posed_mesh.clone(),
            model: transform.matrix(),
            material: material([0.32 + f32::from(index % 4) * 0.13, 0.43, 0.85, 1.]),
        });
        entities.push(entity(reference, tick, transform, AssetKey([12; 32])));
    }
    let sprites = fixture_sprites(&camera, tick, &mut entities)?;
    let frame = RenderFrame {
        stamp: FrameStamp {
            lot_id: 0x574c_4346,
            epoch: 1,
            tick,
            architecture_revision: 1,
            content: AssetKey(Sha256::digest(b"wonderland-c-synthetic-fixture-v2").into()),
        },
        entities,
        selected: Some(EntityRef {
            object_id: 100,
            generation: 1,
        }),
    };
    let mut store = wonderland_render_core::frame::FrameStore::new(RenderLimits::default());
    store.reset(frame.stamp.lot_id, frame.stamp.epoch);
    store.admit(frame.clone()).map_err(problem)?;
    let mut scene = FixtureScene {
        frame,
        camera,
        draws,
        sprites,
        hash: [0; 32],
    };
    let mut hasher = Sha256::new();
    hasher.update(FIXTURE_VERSION.to_le_bytes());
    hasher.update([match mode {
        ViewMode::Full2D => 0,
        ViewMode::Hybrid2D => 1,
        ViewMode::Full3D => 2,
    }]);
    hasher.update(bincode::serialize(&scene).map_err(problem)?);
    scene.hash = hasher.finalize().into();
    Ok(scene)
}

/// Keep the synthetic crowd's actual horizontal geometry disjoint throughout
/// its complete animation cycle. The old 0.7-tile grid interpenetrated adjacent
/// arms on coincident planes; their separately tessellated GPU depths could
/// disagree despite an unchanged source LessEqual policy. This input correction
/// makes no claim of general CPU/GPU coplanar rasterization equivalence.
fn posed_crowd(
    tick: u64,
    world_size: Vec2,
) -> Result<(Mesh, [Vec2; CROWD_CAPACITY]), FixtureError> {
    const MARGIN: f32 = 0.125;
    const CLEARANCE: f32 = 0.125;
    let rig = wonderland_avatar_view::fixtures::representative_rig();
    let prepared = wonderland_avatar_view::fixtures::representative_mesh(&rig);
    let mut low = Vec2::new(f32::INFINITY, f32::INFINITY);
    let mut high = Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut active_mesh = None;
    for phase in 0..ANIMATION_PHASES {
        let mut pose = rig.bind_pose();
        let angle = (phase as f32 / ANIMATION_PHASES as f32 * std::f32::consts::TAU).sin() * 0.3;
        pose.locals[1].rotation = Quat::from_axis_angle(Vec3::Z, angle)
            .ok_or_else(|| FixtureError("pose angle".into()))?;
        pose.rebuild(&rig).map_err(problem)?;
        let mesh = prepared.skin(&pose, Mat4::IDENTITY).map_err(problem)?;
        for vertex in &mesh.vertices {
            low.x = low.x.min(vertex.position.x);
            low.y = low.y.min(vertex.position.z);
            high.x = high.x.max(vertex.position.x);
            high.y = high.y.max(vertex.position.z);
        }
        if phase == tick % ANIMATION_PHASES {
            active_mesh = Some(mesh);
        }
    }
    let footprint = high - low;
    let available = world_size - Vec2::new(2. * MARGIN, 2. * MARGIN);
    if !footprint.is_finite() || footprint.x <= 0. || footprint.y <= 0. {
        return Err(FixtureError("crowd footprint".into()));
    }
    let columns = (((available.x + CLEARANCE) / (footprint.x + CLEARANCE)).floor() as usize)
        .min(CROWD_CAPACITY);
    if columns == 0 {
        return Err(FixtureError("crowd does not fit lot width".into()));
    }
    let rows = (CROWD_CAPACITY + columns - 1) / columns;
    let axis = |span: f32, size: f32, count: usize| -> Result<f32, FixtureError> {
        let required = count as f32 * size + (count - 1) as f32 * CLEARANCE;
        if span < required {
            return Err(FixtureError("crowd does not fit lot bounds".into()));
        }
        Ok(if count == 1 {
            0.
        } else {
            (span - size) / (count - 1) as f32
        })
    };
    let stride = Vec2::new(
        axis(available.x, footprint.x, columns)?,
        axis(available.y, footprint.y, rows)?,
    );
    let origin = Vec2::new(
        MARGIN - low.x
            + if columns == 1 {
                (available.x - footprint.x) * 0.5
            } else {
                0.
            },
        MARGIN - low.y
            + if rows == 1 {
                (available.y - footprint.y) * 0.5
            } else {
                0.
            },
    );
    let placements = std::array::from_fn(|index| {
        Vec2::new(
            origin.x + (index % columns) as f32 * stride.x,
            origin.y + (index / columns) as f32 * stride.y,
        )
    });
    Ok((
        active_mesh.ok_or_else(|| FixtureError("active crowd pose".into()))?,
        placements,
    ))
}

pub fn hash_hex(hash: [u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn problem(error: impl std::fmt::Debug) -> FixtureError {
    FixtureError(format!("{error:?}"))
}
fn material(color: [f32; 4]) -> MaterialSpec {
    MaterialSpec {
        color,
        image: None,
        unlit: true,
        alpha_cutoff: None,
        double_sided: true,
    }
}
fn entity(
    reference: EntityRef,
    tick: u64,
    transform: Transform,
    asset: AssetKey,
) -> EntityProjection {
    EntityProjection {
        reference,
        visual_revision: tick + 1,
        transform,
        previous_transform: None,
        asset,
        level: 1,
        visible: true,
        selectable: true,
    }
}

fn fixture_sprites(
    camera: &CameraSpec,
    tick: u64,
    entities: &mut Vec<EntityProjection>,
) -> Result<Vec<SpriteDraw>, FixtureError> {
    use wonderland_render_iso::*;
    let projection = Projection::new(
        Zoom::Medium,
        Rotation::TopLeft,
        1.,
        Vec3::new(3., 3., 0.),
        Vec2::new(WIDTH as f32, HEIGHT as f32),
    )
    .map_err(problem)?;
    let policy = PreparePolicy {
        wvp: Some(camera.view_projection(WIDTH as f32 / HEIGHT as f32)?),
        gamma: GammaMode::Basic,
        ..PreparePolicy::default()
    };
    let mut sprites = Vec::new();
    for ordinal in 0..3u32 {
        let owner = EntityRef {
            object_id: 100 + ordinal,
            generation: 1,
        };
        let tile = Vec3::new(
            2.6 + ordinal as f32 * 0.45,
            3.0 - ordinal as f32 * 0.08,
            0.2,
        );
        let colors = [
            [230, 70, 55, 255],
            [65, 190, 245, 128],
            [180, 160, 220, 255],
        ];
        let mut pixels = vec![colors[ordinal as usize]; 64 * 96];
        let mut mask = vec![255; pixels.len()];
        let mut depth = vec![170; pixels.len()];
        for y in 0..96 {
            for x in 0..64 {
                let i = y * 64 + x;
                depth[i] = (255 - x * 3).max(1) as u8;
                if x < 4 || x >= 60 || y < 4 || y >= 92 {
                    pixels[i][3] = 0;
                }
                if ordinal == 1 {
                    mask[i] = if (20..44).contains(&x) && (28..68).contains(&y) {
                        0
                    } else {
                        150
                    };
                }
                if ordinal == 2 && x < 16 {
                    pixels[i][3] = [2, 3, 25, 26][x / 4];
                }
            }
        }
        let image = RgbaImage {
            width: 64,
            height: 96,
            pixels,
        };
        let mask_input = (ordinal == 1).then(|| MaskInput {
            key: AssetKey([25; 32]),
            rgba: RgbaImage {
                width: 64,
                height: 96,
                pixels: mask.iter().map(|a| [255, 255, 255, *a]).collect(),
            },
            physical_size: [64, 96],
        });
        let asset = Arc::new(SpriteAsset {
            key: AssetKey([20 + ordinal as u8; 32]),
            rgba: image.clone(),
            physical_size: [64, 96],
            depth: DepthInput::Bytes {
                key: AssetKey([30 + ordinal as u8; 32]),
                width: 64,
                height: 96,
                values: depth.clone(),
            },
            mask: mask_input,
        });
        let group = DgrpImage {
            direction: 1,
            zoom: Zoom::Medium,
            layers: vec![Some(DgrpLayer {
                sprite_id: ordinal,
                frame_index: 0,
                sprite_offset: Vec2::new(-32., 0.),
                object_offset: Vec3::ZERO,
                flags: u32::from(ordinal == 2),
                asset: Some(asset),
            })],
        };
        let instance = DgrpInstance {
            reference: owner,
            visual_revision: tick + 1,
            direction: 1,
            tile_position: tile,
            room: if ordinal == 2 { 65534 } else { 1 },
            base_room: None,
            level: 1,
            visible: true,
            selectable: true,
            cutaway_hidden: false,
            dynamic_base: 0,
            dynamic_count: 0,
            dynamic_masks: [0; 2],
        };
        let output = prepare_sprites(&projection, &instance, &[group], &policy).map_err(problem)?;
        let prepared = output
            .sprites
            .first()
            .ok_or_else(|| FixtureError("expected fixture DGRP sprite".into()))?;
        let anchors = prepared
            .anchors
            .ok_or_else(|| FixtureError("expected fixture sprite depth".into()))?;
        sprites.push(SpriteDraw {
            name: format!("sprite-{ordinal}"),
            owner,
            rect: [
                prepared.rect.x,
                prepared.rect.y,
                prepared.rect.width,
                prepared.rect.height,
            ],
            image,
            depth,
            mask: (ordinal == 1).then_some(mask),
            back_depth: anchors.back.depth,
            front_depth: anchors.front.depth,
            lighting: [0.75, 0.85, 1.],
            room: instance.room,
            mirror: ordinal == 2,
        });
        entities.push(entity(
            owner,
            tick,
            Transform {
                translation: wonderland_render_core::units::tile_to_graphics(tile),
                ..Transform::IDENTITY
            },
            AssetKey([20 + ordinal as u8; 32]),
        ));
    }
    Ok(sprites)
}

#[derive(Clone, Debug)]
pub struct ReferenceFrame {
    pub image: RgbaImage,
    pub depths: Vec<f32>,
    pub ids: Vec<Option<EntityRef>>,
    pub digest: [u8; 32],
}

/// Separate color and selection passes preserve the source's distinct alpha
/// thresholds. These results are CPU reference evidence, never GPU readbacks.
pub fn reference_frame(scene: &FixtureScene) -> Result<ReferenceFrame, FixtureError> {
    reference_frame_at_size(scene, WIDTH, HEIGHT)
}

/// Rerasterize the fixture at physical framebuffer dimensions. Mesh projection
/// uses the requested aspect ratio; sprites retain their logical 640x480 layout
/// and sample at the new physical pixel centers. This never resizes encoded IDs.
/// Errors return no partial frame, and the borrowed scene is never modified.
pub fn reference_frame_at_size(
    scene: &FixtureScene,
    width: u32,
    height: u32,
) -> Result<ReferenceFrame, FixtureError> {
    let limits = RenderLimits::default();
    let pixels = RgbaImage::checked_pixel_count(width, height, &limits).map_err(problem)?;
    let bytes_per_pixel = 3
        * (std::mem::size_of::<[u8; 4]>()
            + std::mem::size_of::<f32>()
            + std::mem::size_of::<Option<EntityRef>>());
    if (pixels as u64)
        .checked_mul(bytes_per_pixel as u64)
        .map_or(true, |bytes| bytes > MAX_REFERENCE_BUFFER_BYTES)
    {
        return Err(FixtureError("reference framebuffer byte budget".into()));
    }
    let vp = scene.camera.view_projection(width as f32 / height as f32)?;
    let mut color = ReferenceSurface::new(width, height, &limits).map_err(problem)?;
    let mut picks = ReferenceSurface::new(width, height, &limits).map_err(problem)?;
    // Both source engine adapters admit later equal-depth fragments. Keep the
    // CPU oracle's coplanar floor/terrain and city layers on that same policy.
    color.set_depth_comparison(wonderland_render_core::reference::DepthComparison::LessEqual);
    picks.set_depth_comparison(wonderland_render_core::reference::DepthComparison::LessEqual);
    color.clear([22, 29, 40, 255]);
    for draw in &scene.draws {
        if !draw.material.unlit {
            return Err(FixtureError(
                "CPU fixture reference requires explicit unlit materials".into(),
            ));
        }
        let mut mesh = draw.mesh.clone();
        for vertex in &mut mesh.vertices {
            for channel in 0..4 {
                vertex.color[channel] *= draw.material.color[channel];
            }
        }
        let color_options = FragmentOptions {
            alpha_cutoff: draw
                .material
                .alpha_cutoff
                .map_or(0, |f| (f * 255.).clamp(0., 255.) as u8),
            write_id: false,
            ..FragmentOptions::default()
        };
        let pick_options = FragmentOptions {
            alpha_cutoff: color_options.alpha_cutoff.max(25),
            ..FragmentOptions::default()
        };
        if let Some(image) = &draw.material.image {
            color
                .draw_textured_mesh(
                    &mesh,
                    vp * draw.model,
                    image,
                    draw.owner,
                    color_options,
                    &limits,
                )
                .map_err(problem)?;
            picks
                .draw_textured_mesh(
                    &mesh,
                    vp * draw.model,
                    image,
                    draw.owner,
                    pick_options,
                    &limits,
                )
                .map_err(problem)?;
        } else {
            color
                .draw_mesh(&mesh, vp * draw.model, draw.owner, color_options, &limits)
                .map_err(problem)?;
            picks
                .draw_mesh(&mesh, vp * draw.model, draw.owner, pick_options, &limits)
                .map_err(problem)?;
        }
    }
    for sprite in &scene.sprites {
        render_sprite(sprite, &mut color, &mut picks)?;
    }
    let mut digest = Sha256::new();
    for pixel in &color.image().pixels {
        digest.update(pixel);
    }
    for id in picks.ids() {
        digest.update(id.map_or([0; 8], |id| {
            let mut b = [0; 8];
            b[..4].copy_from_slice(&id.object_id.to_le_bytes());
            b[4..].copy_from_slice(&id.generation.to_le_bytes());
            b
        }));
    }
    Ok(ReferenceFrame {
        image: color.image().clone(),
        depths: color.depths().to_vec(),
        ids: picks.ids().to_vec(),
        digest: digest.finalize().into(),
    })
}

fn render_sprite(
    sprite: &SpriteDraw,
    color: &mut ReferenceSurface,
    picks: &mut ReferenceSurface,
) -> Result<(), FixtureError> {
    use wonderland_render_iso::{shade_fragment, sprite_depth_fraction, AlphaPass, GammaMode};
    let output_width = color.image().width;
    let output_height = color.image().height;
    let scale_x = output_width as f32 / WIDTH as f32;
    let scale_y = output_height as f32 / HEIGHT as f32;
    let [left, top, width, height] = [
        sprite.rect[0] * scale_x,
        sprite.rect[1] * scale_y,
        sprite.rect[2] * scale_x,
        sprite.rect[3] * scale_y,
    ];
    if ![left, top, width, height].iter().all(|v| v.is_finite()) || width <= 0. || height <= 0. {
        return Err(FixtureError("sprite rectangle".into()));
    }
    let right = left + width;
    let bottom = top + height;
    if !right.is_finite() || !bottom.is_finite() {
        return Err(FixtureError("sprite rectangle extent".into()));
    }
    sprite
        .image
        .validate(&RenderLimits::default())
        .map_err(problem)?;
    if sprite.depth.len() != sprite.image.pixels.len()
        || sprite
            .mask
            .as_ref()
            .is_some_and(|m| m.len() != sprite.depth.len())
    {
        return Err(FixtureError("sprite channels".into()));
    }
    for y in top.floor().max(0.) as u32..bottom.ceil().min(output_height as f32).max(0.) as u32 {
        for x in left.floor().max(0.) as u32..right.ceil().min(output_width as f32).max(0.) as u32 {
            let u = (x as f32 + 0.5 - left) / width;
            let v = (y as f32 + 0.5 - top) / height;
            if !(0. ..1.).contains(&u) || !(0. ..1.).contains(&v) {
                continue;
            }
            let sx = ((u * sprite.image.width as f32) as u32).min(sprite.image.width - 1);
            let sx = if sprite.mirror {
                sprite.image.width - 1 - sx
            } else {
                sx
            };
            let sy = ((v * sprite.image.height as f32) as u32).min(sprite.image.height - 1);
            let i = (sy * sprite.image.width + sx) as usize;
            let mut rgba = sprite.image.pixels[i];
            // SpriteDraw stores effective mask semantics independently from the
            // source pass enum: a supplied mask REPLACES color alpha in both passes.
            rgba[3] = sprite.mask.as_ref().map_or(rgba[3], |m| m[i]);
            let depth = sprite.back_depth
                + sprite_depth_fraction(sprite.depth[i]) * (sprite.front_depth - sprite.back_depth);
            for (surface, pass, cutoff) in [
                (&mut *color, AlphaPass::ColorDepth, 2),
                (&mut *picks, AlphaPass::ObjectIdDepth, 25),
            ] {
                if let Some(p) = shade_fragment(
                    rgba,
                    None,
                    sprite.lighting,
                    sprite.room,
                    GammaMode::Basic,
                    pass,
                ) {
                    if p[3] <= 0. {
                        continue;
                    }
                    let straight = [
                        channel(p[0] / p[3]),
                        channel(p[1] / p[3]),
                        channel(p[2] / p[3]),
                        channel(p[3]),
                    ];
                    surface
                        .write_fragment(
                            x,
                            y,
                            depth,
                            straight,
                            Some(sprite.owner),
                            FragmentOptions {
                                alpha_cutoff: cutoff,
                                ..FragmentOptions::default()
                            },
                        )
                        .map_err(problem)?;
                }
            }
        }
    }
    Ok(())
}
fn channel(v: f32) -> u8 {
    (v.clamp(0., 1.) * 255.).round() as u8
}

pub fn pick_at(scene: &FixtureScene, x: u32, y: u32) -> Option<EntityRef> {
    if x >= WIDTH || y >= HEIGHT {
        return None;
    }
    reference_frame(scene).ok()?.ids[(y * WIDTH + x) as usize]
}
