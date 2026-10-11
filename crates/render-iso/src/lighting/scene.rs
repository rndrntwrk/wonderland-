//! Executable room-light preparation: LMapBatch.DrawRoom/MultiplyOutdoors and
//! LightMap2D.fx. Produces real uploadable atlas pixels as well as immutable GPU
//! pass geometry. RGBA8 attachments saturate and quantize after every draw.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use wonderland_render_core::{cache::DerivedKey, RgbaImage, Vec2, Vec3};

#[derive(Clone, Debug, PartialEq)]
pub struct RoomShadowGeometry {
    pub room: u16,
    pub floor: u8,
    pub walls: Vec<[Vec2; 2]>,
    pub objects: Vec<crate::Rect>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneLight {
    pub id: u64,
    pub room: u16,
    pub floor: u8,
    pub light: ShadowLight,
    pub color: [u8; 3],
    pub intensity: f32,
    pub outdoors_color: bool,
    pub window_room: Option<u16>,
    /// Source LightData.Height, in tile units, for projected object shadows.
    pub height: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionalLighting {
    pub light: ShadowLight,
    pub shadow_multiplier: f32,
    /// Shader direction: (-SunVector.Z, -SunVector.Y, SunVector.X).
    pub sun_direction: Vec3,
}
#[derive(Clone, Copy, Debug)]
pub struct LightingSceneInput<'a> {
    pub room_maps: &'a PreparedRoomMaps,
    pub geometry: &'a [RoomShadowGeometry],
    pub lights: &'a [SceneLight],
    pub outdoors: Option<DirectionalLighting>,
    pub ultra: bool,
    pub software_depth: bool,
    pub directional: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightPassKind {
    Clear,
    Outdoors,
    OutdoorsSsaa,
    LightBleed,
    Point(u64),
    MultiplyOutside,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightUniforms {
    pub color: [f32; 4],
    pub position_sixteenths: Vec2,
    pub radius_sixteenths: f32,
    pub height_sixteenths: f32,
    pub shadow_powers: [f32; 2],
    pub direction: Vec3,
    pub outdoors_color: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedLightPass {
    pub room: u16,
    pub floor: u8,
    pub kind: LightPassKind,
    /// Local floor rectangle, x/y/width/height; add layout.scissor_origin(floor).
    pub scissor: [u32; 4],
    pub uniforms: LightUniforms,
    pub wall_shadow: Option<ShadowMesh>,
    pub object_shadow: Option<ShadowMesh>,
    pub projected_object_shadows: Vec<ProjectedShadowMesh>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DirectionAtlas {
    pub width: u32,
    pub height: u32,
    /// Each component is quantized to a source HalfVector4 attachment after each draw.
    pub pixels: Vec<[f32; 4]>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedLightingScene {
    key: DerivedKey,
    room_map_key: DerivedKey,
    wcrc_key: Option<DerivedKey>,
    layout: LightAtlasLayout,
    color: RgbaImage,
    direction: Option<DirectionAtlas>,
    passes: Vec<PreparedLightPass>,
    resident_bytes: usize,
    raster_samples: u64,
}
impl PreparedLightingScene {
    pub fn key(&self) -> DerivedKey {
        self.key
    }
    pub fn wcrc_key(&self) -> Option<DerivedKey> {
        self.wcrc_key
    }
    pub fn room_map_key(&self) -> DerivedKey {
        self.room_map_key
    }
    pub fn layout(&self) -> LightAtlasLayout {
        self.layout
    }
    pub fn color_atlas(&self) -> &RgbaImage {
        &self.color
    }
    pub fn direction_atlas(&self) -> Option<&DirectionAtlas> {
        self.direction.as_ref()
    }
    pub fn passes(&self) -> &[PreparedLightPass] {
        &self.passes
    }
    pub fn resident_bytes(&self) -> usize {
        self.resident_bytes
    }
    pub fn raster_samples(&self) -> u64 {
        self.raster_samples
    }
}

pub fn prepare_lighting_scene(
    input: &LightingSceneInput<'_>,
    budget: LightingBudget,
) -> Result<PreparedLightingScene> {
    prepare_lighting_scene_with_meshes(input, &[], budget)
}
pub fn prepare_lighting_scene_with_meshes(
    input: &LightingSceneInput<'_>,
    mesh_rooms: &[RoomMeshShadows],
    budget: LightingBudget,
) -> Result<PreparedLightingScene> {
    prepare_scene(input, mesh_rooms, None, budget)
}
/// Execute the source WCRC SSAA sunlight/indoor-bleed passes using real filtered
/// wall/floor/roof/object targets. The input component is independent of mode.
pub fn prepare_lighting_scene_with_wcrc(
    input: &LightingSceneInput<'_>,
    mesh_rooms: &[RoomMeshShadows],
    sunlight: &PreparedWcrcSunlight,
    budget: LightingBudget,
) -> Result<PreparedLightingScene> {
    if input.room_maps.key() != sunlight.room_map_key
        || input.outdoors != Some(sunlight.sunlight)
        || input.ultra != sunlight.ultra
        || input.software_depth != sunlight.software_depth
    {
        return Err(IsoError::Invalid("stale WCRC sunlight capture"));
    }
    prepare_scene(input, mesh_rooms, Some(sunlight), budget)
}
fn prepare_scene(
    input: &LightingSceneInput<'_>,
    mesh_rooms: &[RoomMeshShadows],
    sunlight: Option<&PreparedWcrcSunlight>,
    budget: LightingBudget,
) -> Result<PreparedLightingScene> {
    let maps = input.room_maps;
    let layout = LightAtlasLayout::new(
        maps.width,
        maps.height,
        input.ultra,
        input.software_depth,
        budget.max_texture_pixels,
    )?;
    let color_count = budget.texture(layout.color_atlas[0], layout.color_atlas[1])?;
    let wall_count = budget.texture(layout.wall_shadow[0], layout.wall_shadow[1])?;
    let object_count = budget.texture(layout.object_shadow[0], layout.object_shadow[1])?;
    let direction_count = if input.directional {
        budget.texture(layout.direction_atlas[0], layout.direction_atlas[1])?
    } else {
        0
    };
    let texture_bytes = color_count
        .checked_mul(4)
        .and_then(|n| n.checked_add(direction_count.checked_mul(16)?))
        .ok_or(IsoError::Limit("light atlas bytes"))?;
    let scratch_bytes = object_count
        .checked_mul(if input.ultra { 2 } else { 1 })
        .and_then(|n| n.checked_add(wall_count))
        .ok_or(IsoError::Limit("shadow target pixels"))?
        .checked_mul(4)
        .ok_or(IsoError::Limit("shadow target bytes"))?;
    // Bounds/room/geometry lookup nodes and worst-case amortized pass capacity.
    let metadata_bytes = maps
        .rooms
        .len()
        .checked_mul(512 + 6 * std::mem::size_of::<PreparedLightPass>())
        .and_then(|n| {
            n.checked_add(
                input
                    .lights
                    .len()
                    .checked_mul(128 + 2 * std::mem::size_of::<PreparedLightPass>())?,
            )
        })
        .ok_or(IsoError::Limit("lighting metadata bytes"))?;
    let peak_base = texture_bytes
        .checked_add(scratch_bytes)
        .and_then(|n| n.checked_add(maps.resident_bytes))
        .and_then(|n| n.checked_add(metadata_bytes))
        .and_then(|n| n.checked_add(sunlight.map_or(0, |s| s.resident_bytes())))
        .ok_or(IsoError::Limit("lighting scene bytes"))?;
    budget.bytes(peak_base)?;
    let initial_work = u64::from(maps.width)
        .checked_mul(u64::from(maps.height))
        .and_then(|n| n.checked_mul(maps.maps.len() as u64))
        .and_then(|n| n.checked_mul(2))
        .and_then(|n| n.checked_add(color_count as u64))
        .and_then(|n| n.checked_add(direction_count as u64))
        .and_then(|n| n.checked_add(sunlight.map_or(0, |s| s.raster_samples())))
        .ok_or(IsoError::Limit("lighting map scan work"))?;
    if initial_work > budget.max_raster_samples {
        return Err(IsoError::Limit("lighting map scan work"));
    }
    if input.lights.len() > budget.max_lights
        || maps.rooms.len() > budget.max_rooms
        || input.geometry.len() > budget.max_rooms
    {
        return Err(IsoError::Limit("lighting scene inputs"));
    }
    let rooms: BTreeMap<_, _> = maps.rooms.iter().map(|r| (r.id, *r)).collect();
    let mut geometry = BTreeMap::new();
    let mut wall_total = 0usize;
    let mut object_total = 0usize;
    for g in input.geometry {
        if rooms.get(&g.room).map_or(true, |r| r.floor != g.floor)
            || geometry.insert(g.room, g).is_some()
        {
            return Err(IsoError::Invalid("room shadow geometry identity"));
        }
        wall_total = wall_total
            .checked_add(g.walls.len())
            .ok_or(IsoError::Limit("lighting wall count"))?;
        object_total = object_total
            .checked_add(g.objects.len())
            .ok_or(IsoError::Limit("lighting object count"))?;
        if wall_total > budget.max_walls || object_total > budget.max_occluders {
            return Err(IsoError::Limit("lighting geometry count"));
        }
        if g.walls.iter().any(|[a, b]| {
            !a.is_finite()
                || !b.is_finite()
                || a == b
                || [a.x, a.y, b.x, b.y].iter().any(|x| x.abs() > 1_000_000.)
        }) || g.objects.iter().any(|&r| {
            !super::valid_rect(r)
                || [r.x, r.y, r.width, r.height]
                    .iter()
                    .any(|x| x.fract() != 0. || x.abs() > 1_000_000.)
        }) {
            return Err(IsoError::Invalid("lighting geometry"));
        }
    }
    let mut mesh_geometry = BTreeMap::new();
    let (mut mesh_vertices, mut mesh_indices, mut mesh_count) = (0usize, 0usize, 0usize);
    if mesh_rooms.len() > budget.max_rooms {
        return Err(IsoError::Limit("mesh shadow rooms"));
    }
    for group in mesh_rooms {
        if rooms
            .get(&group.room)
            .map_or(true, |r| r.floor != group.floor)
            || mesh_geometry
                .insert(group.room, &group.meshes[..])
                .is_some()
        {
            return Err(IsoError::Invalid("mesh shadow room identity"));
        }
        mesh_count = mesh_count
            .checked_add(group.meshes.len())
            .ok_or(IsoError::Limit("mesh shadow count"))?;
        if mesh_count > budget.max_occluders {
            return Err(IsoError::Limit("mesh shadow count"));
        }
        for mesh in &group.meshes {
            super::projected::validate_mesh(mesh, budget)?;
            mesh_vertices = mesh_vertices
                .checked_add(mesh.vertices.len())
                .ok_or(IsoError::Limit("mesh shadow vertices"))?;
            mesh_indices = mesh_indices
                .checked_add(mesh.indices.len())
                .ok_or(IsoError::Limit("mesh shadow indices"))?;
            if mesh_vertices > budget.max_vertices || mesh_indices > budget.max_indices {
                return Err(IsoError::Limit("mesh shadow geometry"));
            }
        }
    }
    // Ultra source shadows require actual object geometry, never substitute ellipses.
    if input.ultra
        && input.geometry.iter().any(|g| {
            !g.objects.is_empty()
                && mesh_geometry
                    .get(&g.room)
                    .map_or(true, |m| m.iter().all(|m| m.indices.is_empty()))
        })
    {
        return Err(IsoError::Invalid(
            "ultra lighting requires projected object meshes",
        ));
    }
    let mut ids = BTreeSet::new();
    for l in input.lights {
        l.light.validate()?;
        if l.light.kind != ShadowLightKind::Point
            || l.id == 0
            || !ids.insert(l.id)
            || rooms.get(&l.room).map_or(true, |r| r.floor != l.floor)
            || !l.intensity.is_finite()
            || !(0. ..=1_000_000.).contains(&l.intensity)
            || !l.height.is_finite()
            || !(0.01..=1000.).contains(&l.height)
            || l.window_room.is_some_and(|id| !rooms.contains_key(&id))
        {
            return Err(IsoError::Invalid("scene light identity/parameters"));
        }
    }
    if let Some(sun) = input.outdoors {
        sun.light.validate()?;
        if sun.light.kind != ShadowLightKind::Directional
            || !sun.shadow_multiplier.is_finite()
            || !(0. ..=1.33).contains(&sun.shadow_multiplier)
            || !sun.sun_direction.is_finite()
            || (sun.sun_direction.length() - 1.).abs() > 0.0001
        {
            return Err(IsoError::Invalid("directional lighting"));
        }
    }
    let empty = RoomShadowGeometry {
        room: 0,
        floor: 0,
        walls: vec![],
        objects: vec![],
    };
    let bounds = room_bounds(maps, layout.pixels_per_tile);
    let mut scene = PreparedLightingScene {
        key: maps.key(),
        room_map_key: maps.key(),
        wcrc_key: sunlight.map(|s| s.key()),
        layout,
        color: RgbaImage {
            width: layout.color_atlas[0],
            height: layout.color_atlas[1],
            pixels: vec![[maps.minimum[3]; 4]; color_count],
        },
        direction: input.directional.then(|| DirectionAtlas {
            width: layout.direction_atlas[0],
            height: layout.direction_atlas[1],
            pixels: vec![[0.; 4]; direction_count],
        }),
        passes: vec![],
        resident_bytes: texture_bytes,
        raster_samples: initial_work,
    };
    let base = LightUniforms {
        color: [0.; 4],
        position_sixteenths: Vec2::ZERO,
        radius_sixteenths: 1.,
        height_sixteenths: 16. * (maps.width - 1) as f32 / maps.width as f32,
        shadow_powers: [1.; 2],
        direction: Vec3::ZERO,
        outdoors_color: false,
    };
    for room in maps.rooms() {
        let scissor = bounds.get(&room.id).copied().unwrap_or([0; 4]);
        if scissor[2] == 0 || scissor[3] == 0 {
            continue;
        }
        let g = geometry.get(&room.id).copied().unwrap_or(&empty);
        let mesh_inputs = mesh_geometry.get(&room.id).copied().unwrap_or(&[]);
        scene.passes.push(PreparedLightPass {
            room: room.id,
            floor: room.floor,
            kind: LightPassKind::Clear,
            scissor,
            uniforms: LightUniforms {
                color: [f32::from(maps.minimum[3]) / 255.; 4],
                ..base
            },
            wall_shadow: None,
            object_shadow: None,
            projected_object_shadows: vec![],
        });
        if room.outside || sunlight.is_some() {
            if let Some(sun) = input.outdoors {
                let uniforms = LightUniforms {
                    color: [1. - f32::from(maps.minimum[3]) / 255.; 4],
                    position_sixteenths: sun.light.position_sixteenths,
                    shadow_powers: [0.75 * sun.shadow_multiplier, 0.6 * sun.shadow_multiplier],
                    direction: sun.sun_direction,
                    outdoors_color: true,
                    ..base
                };
                let pass = PreparedLightPass {
                    room: room.id,
                    floor: room.floor,
                    kind: if sunlight.is_some() {
                        if room.outside {
                            LightPassKind::OutdoorsSsaa
                        } else {
                            LightPassKind::LightBleed
                        }
                    } else {
                        LightPassKind::Outdoors
                    },
                    scissor: if sunlight.is_some() && !room.outside {
                        [0, 0, layout.wall_shadow[0], layout.wall_shadow[1]]
                    } else {
                        scissor
                    },
                    uniforms,
                    wall_shadow: None,
                    object_shadow: None,
                    projected_object_shadows: vec![],
                };
                if let Some(sunlight) = sunlight {
                    let capture = sunlight
                        .floor(room.floor)
                        .ok_or(IsoError::Invalid("missing WCRC floor target"))?;
                    execute_pass(
                        &mut scene,
                        maps,
                        &pass,
                        None,
                        None,
                        Some(capture),
                        input.ultra,
                        budget,
                    )?;
                    scene.passes.push(pass);
                } else {
                    draw_pass(
                        &mut scene,
                        maps,
                        g,
                        mesh_inputs,
                        2.2125,
                        sun.light,
                        pass,
                        budget,
                        peak_base,
                        input.ultra,
                    )?;
                }
            }
        }
        for outside in [true, false] {
            if !outside {
                let c = maps.outside.map(|v| f32::from(v) / 255.);
                let uniforms = LightUniforms {
                    color: [c[0], c[1], c[2], (c[0] + c[1] + c[2]) / 3.],
                    direction: input.outdoors.map_or(Vec3::ZERO, |s| s.sun_direction),
                    ..base
                };
                let pass = PreparedLightPass {
                    room: room.id,
                    floor: room.floor,
                    kind: LightPassKind::MultiplyOutside,
                    scissor,
                    uniforms,
                    wall_shadow: None,
                    object_shadow: None,
                    projected_object_shadows: vec![],
                };
                execute_pass(
                    &mut scene,
                    maps,
                    &pass,
                    None,
                    None,
                    None,
                    input.ultra,
                    budget,
                )?;
                scene.passes.push(pass);
            }
            for l in input
                .lights
                .iter()
                .filter(|l| l.room == room.id && l.outdoors_color == outside)
            {
                let intensity = l
                    .window_room
                    .map_or(l.intensity, |id| f32::from(rooms[&id].ambient_light) / 150.);
                if l.window_room.is_some() && intensity < 0.2 {
                    continue;
                }
                let raw = l.color.map(|v| f32::from(v) / 255.);
                let mut color = [raw[0], raw[1], raw[2], (raw[0] + raw[1] + raw[2]) / 3.];
                for (i, value) in color.iter_mut().enumerate() {
                    *value *= intensity
                        * if outside {
                            1. - f32::from(maps.minimum[i]) / 255.
                        } else {
                            0.70
                        };
                }
                let bounds = l.light.bounds();
                let factor = layout.pixels_per_tile as f32 / 16.;
                let s = intersect_scissor(
                    scissor,
                    [
                        (bounds.x * factor).trunc(),
                        (bounds.y * factor).trunc(),
                        (bounds.width * factor).trunc(),
                        (bounds.height * factor).trunc(),
                    ],
                );
                if s[2] == 0 || s[3] == 0 {
                    continue;
                }
                let uniforms = LightUniforms {
                    color,
                    position_sixteenths: l.light.position_sixteenths,
                    radius_sixteenths: l.light.radius_sixteenths,
                    outdoors_color: outside,
                    ..base
                };
                draw_pass(
                    &mut scene,
                    maps,
                    g,
                    mesh_inputs,
                    l.height,
                    l.light,
                    PreparedLightPass {
                        room: room.id,
                        floor: room.floor,
                        kind: LightPassKind::Point(l.id),
                        scissor: s,
                        uniforms,
                        wall_shadow: None,
                        object_shadow: None,
                        projected_object_shadows: vec![],
                    },
                    budget,
                    peak_base,
                    input.ultra,
                )?;
            }
        }
    }
    let parameter_capacity = scene
        .passes
        .iter()
        .try_fold(97usize, |n, pass| {
            n.checked_add(155)?
                .checked_add(pass.projected_object_shadows.len().checked_mul(32)?)
        })
        .ok_or(IsoError::Limit("light atlas key bytes"))?;
    budget.bytes(
        peak_base
            .checked_add(scene.resident_bytes - texture_bytes)
            .and_then(|n| n.checked_add(parameter_capacity))
            .ok_or(IsoError::Limit("lighting scene peak bytes"))?,
    )?;
    let mut parameters = Vec::with_capacity(parameter_capacity);
    parameters.extend_from_slice(b"source-light-atlas-v1\0");
    parameters.extend_from_slice(&maps.key().0);
    parameters.extend_from_slice(&[
        input.ultra as u8,
        input.software_depth as u8,
        input.directional as u8,
    ]);
    parameters.push(sunlight.is_some() as u8);
    if let Some(sunlight) = sunlight {
        parameters.extend_from_slice(&sunlight.key().0);
    }
    for pass in &scene.passes {
        parameters.extend_from_slice(&pass.room.to_le_bytes());
        parameters.push(pass.floor);
        let id = match pass.kind {
            LightPassKind::Clear => {
                parameters.push(0);
                0
            }
            LightPassKind::Outdoors => {
                parameters.push(1);
                0
            }
            LightPassKind::MultiplyOutside => {
                parameters.push(2);
                0
            }
            LightPassKind::OutdoorsSsaa => {
                parameters.push(4);
                0
            }
            LightPassKind::LightBleed => {
                parameters.push(5);
                0
            }
            LightPassKind::Point(id) => {
                parameters.push(3);
                id
            }
        };
        parameters.extend_from_slice(&id.to_le_bytes());
        for value in pass.scissor {
            parameters.extend_from_slice(&value.to_le_bytes());
        }
        let u = pass.uniforms;
        for value in u.color.into_iter().chain([
            u.position_sixteenths.x,
            u.position_sixteenths.y,
            u.radius_sixteenths,
            u.height_sixteenths,
            u.shadow_powers[0],
            u.shadow_powers[1],
            u.direction.x,
            u.direction.y,
            u.direction.z,
        ]) {
            parameters.extend_from_slice(&value.to_le_bytes());
        }
        parameters.push(u.outdoors_color as u8);
        for mesh in [&pass.wall_shadow, &pass.object_shadow] {
            parameters.push(mesh.is_some() as u8);
            if let Some(mesh) = mesh {
                parameters.extend_from_slice(&mesh.key().0);
            }
        }
        parameters.extend_from_slice(&(pass.projected_object_shadows.len() as u64).to_le_bytes());
        for mesh in &pass.projected_object_shadows {
            parameters.extend_from_slice(&mesh.key().0);
        }
    }
    scene.resident_bytes = scene
        .resident_bytes
        .checked_add(
            scene
                .passes
                .capacity()
                .checked_mul(std::mem::size_of::<PreparedLightPass>())
                .ok_or(IsoError::Limit("light pass descriptors"))?,
        )
        .ok_or(IsoError::Limit("lighting resident bytes"))?;
    scene.key = DerivedKey::new(maps.source, maps.source, 1, &parameters);
    Ok(scene)
}
fn room_bounds(maps: &PreparedRoomMaps, res: u32) -> BTreeMap<u16, [u32; 4]> {
    let mut bounds = BTreeMap::new();
    // Source room bounds in one map traversal, rather than O(rooms*cells).
    for map in &maps.maps {
        for (i, p) in map.pixels.iter().enumerate() {
            let x = i as u32 % maps.width;
            let y = i as u32 / maps.width;
            for id in [
                u16::from_le_bytes([p[0], p[1]]),
                u16::from_le_bytes([p[2], p[3]]) & 0x7fff,
            ] {
                if id == 0 {
                    continue;
                }
                let value = bounds.entry(id).or_insert([maps.width, maps.height, 0, 0]);
                value[0] = value[0].min(x);
                value[1] = value[1].min(y);
                value[2] = value[2].max(x + 1);
                value[3] = value[3].max(y + 1);
            }
        }
    }
    for value in bounds.values_mut() {
        let [left, top, right, bottom] = *value;
        let x = left
            .saturating_mul(res)
            .saturating_sub(8)
            .min((maps.width - 1) * res);
        let y = top
            .saturating_mul(res)
            .saturating_sub(8)
            .min((maps.height - 1) * res);
        *value = [
            x,
            y,
            (right * res + 8).min((maps.width - 1) * res) - x,
            (bottom * res + 8).min((maps.height - 1) * res) - y,
        ];
    }
    bounds
}
fn intersect_scissor(a: [u32; 4], b: [f32; 4]) -> [u32; 4] {
    let x = (a[0] as f32).max(b[0]);
    let y = (a[1] as f32).max(b[1]);
    let right = ((a[0] + a[2]) as f32).min(b[0] + b[2]).max(x);
    let bottom = ((a[1] + a[3]) as f32).min(b[1] + b[3]).max(y);
    [x as u32, y as u32, (right - x) as u32, (bottom - y) as u32]
}
// These parameters keep the independently bounded source light, geometry,
// retained scene and raster admission explicit at the draw boundary.
#[allow(clippy::too_many_arguments)]
fn draw_pass(
    scene: &mut PreparedLightingScene,
    maps: &PreparedRoomMaps,
    geometry: &RoomShadowGeometry,
    mesh_inputs: &[MeshShadowInput],
    height: f32,
    light: ShadowLight,
    mut pass: PreparedLightPass,
    budget: LightingBudget,
    peak_base: usize,
    ultra: bool,
) -> Result<()> {
    // Admit each builder against the bytes and element counts still available.
    // A late aggregate check cannot prevent individually valid meshes from
    // transiently accumulating past the scene budget before returning Err.
    let mut retained = 0usize;
    let mut counts = [0usize; 2];
    for p in &scene.passes {
        retained = retained
            .checked_add(
                p.projected_object_shadows.capacity() * std::mem::size_of::<ProjectedShadowMesh>(),
            )
            .ok_or(IsoError::Limit("retained shadow descriptors"))?;
        for m in [&p.wall_shadow, &p.object_shadow].into_iter().flatten() {
            retained = retained
                .checked_add(m.resident_bytes())
                .ok_or(IsoError::Limit("retained shadows"))?;
            counts[0] += m.vertices().len();
            counts[1] += m.indices().len();
        }
        for m in &p.projected_object_shadows {
            retained = retained
                .checked_add(m.resident_bytes())
                .ok_or(IsoError::Limit("retained shadows"))?;
            counts[0] += m.vertices().len();
            counts[1] += m.indices().len();
        }
    }
    let descriptor_count = if ultra {
        mesh_inputs.iter().filter(|m| !m.indices.is_empty()).count()
    } else {
        0
    };
    let mut geometry_bytes = descriptor_count
        .checked_mul(std::mem::size_of::<ProjectedShadowMesh>())
        .ok_or(IsoError::Limit("projected shadow descriptors"))?;
    let remaining_budget = |geometry_bytes: usize, counts: [usize; 2]| -> Result<LightingBudget> {
        let occupied = peak_base
            .checked_add(retained)
            .and_then(|n| n.checked_add(geometry_bytes))
            .ok_or(IsoError::Limit("light pass bytes"))?;
        Ok(LightingBudget {
            max_total_bytes: budget
                .max_total_bytes
                .checked_sub(occupied)
                .ok_or(IsoError::Limit("light pass bytes"))?,
            max_vertices: budget
                .max_vertices
                .checked_sub(counts[0])
                .ok_or(IsoError::Limit("combined lighting vertices"))?,
            max_indices: budget
                .max_indices
                .checked_sub(counts[1])
                .ok_or(IsoError::Limit("combined lighting indices"))?,
            ..budget
        })
    };
    let wall = generate_wall_shadows(
        &geometry.walls,
        light,
        remaining_budget(geometry_bytes, counts)?,
    )?;
    geometry_bytes += wall.resident_bytes();
    counts[0] += wall.vertices().len();
    counts[1] += wall.indices().len();
    let object = generate_object_shadows(
        if ultra { &[] } else { &geometry.objects },
        light,
        remaining_budget(geometry_bytes, counts)?,
    )?;
    geometry_bytes += object.resident_bytes();
    counts[0] += object.vertices().len();
    counts[1] += object.indices().len();
    remaining_budget(geometry_bytes, counts)?;
    let mut projected = Vec::with_capacity(descriptor_count);
    if ultra {
        for mesh in mesh_inputs.iter().filter(|m| !m.indices.is_empty()) {
            let prepared = project_object_shadow(
                mesh,
                light,
                height,
                remaining_budget(geometry_bytes, counts)?,
            )?;
            if !prepared.indices().is_empty() {
                geometry_bytes += prepared.resident_bytes();
                counts[0] += prepared.vertices().len();
                counts[1] += prepared.indices().len();
                projected.push(prepared);
            }
        }
    }
    remaining_budget(geometry_bytes, counts)?;
    // Range tables are allocated while every prepared mesh remains retained.
    let range_triangles = projected
        .iter()
        .map(|m| m.indices().len() / 3)
        .chain([wall.indices().len() / 3, object.indices().len() / 3])
        .max()
        .unwrap_or(0);
    let range_bytes = range_triangles
        .checked_mul(16)
        .ok_or(IsoError::Limit("shadow raster ranges"))?;
    budget.bytes(
        peak_base
            .checked_add(retained)
            .and_then(|n| n.checked_add(geometry_bytes))
            .and_then(|n| n.checked_add(range_bytes))
            .ok_or(IsoError::Limit("combined raster bytes"))?,
    )?;
    let extent = Vec2::new(
        (maps.width - 1) as f32 * 16.,
        (maps.height - 1) as f32 * 16.,
    );
    let mut remaining = budget;
    remaining.max_raster_samples = budget
        .max_raster_samples
        .saturating_sub(scene.raster_samples);
    let (wall_target, work) = super::shadows::rasterize_shadow_counted(
        &wall,
        scene.layout.wall_shadow,
        extent,
        remaining,
    )?;
    scene.raster_samples += work;
    remaining.max_raster_samples = budget
        .max_raster_samples
        .saturating_sub(scene.raster_samples);
    let (mut object_target, work) = super::shadows::rasterize_shadow_counted(
        &object,
        scene.layout.object_shadow,
        extent,
        remaining,
    )?;
    scene.raster_samples += work;
    for mesh in &projected {
        remaining.max_raster_samples = budget
            .max_raster_samples
            .saturating_sub(scene.raster_samples);
        let (target, work) = super::projected::rasterize_projected_counted(
            mesh,
            scene.layout.object_shadow,
            extent,
            remaining,
        )?;
        scene.raster_samples += work;
        scene.raster_samples = scene
            .raster_samples
            .checked_add(object_target.pixels.len() as u64)
            .ok_or(IsoError::Limit("projected shadow combine work"))?;
        if scene.raster_samples > budget.max_raster_samples {
            return Err(IsoError::Limit("projected shadow combine work"));
        }
        for (a, b) in object_target.pixels.iter_mut().zip(target.pixels) {
            a[1] = a[1].max(b[1]);
        }
    }
    execute_pass(
        scene,
        maps,
        &pass,
        Some(&wall_target),
        Some(&object_target),
        None,
        ultra,
        budget,
    )?;
    scene.resident_bytes += geometry_bytes;
    pass.wall_shadow = Some(wall);
    pass.object_shadow = Some(object);
    pass.projected_object_shadows = projected;
    scene.passes.push(pass);
    Ok(())
}
// The three optional attachments correspond to distinct source samplers.
#[allow(clippy::too_many_arguments)]
fn execute_pass(
    scene: &mut PreparedLightingScene,
    maps: &PreparedRoomMaps,
    pass: &PreparedLightPass,
    wall: Option<&RgbaImage>,
    object: Option<&RgbaImage>,
    outside: Option<&RgbaImage>,
    ultra: bool,
    budget: LightingBudget,
) -> Result<()> {
    let r = scene.layout.pixels_per_tile;
    let origin = scene.layout.scissor_origin(pass.floor)?;
    let samples = u64::from(pass.scissor[2]) * u64::from(pass.scissor[3]);
    let work = samples
        .checked_mul(match pass.kind {
            LightPassKind::MultiplyOutside => 1,
            LightPassKind::Point(_) if ultra => 26,
            LightPassKind::OutdoorsSsaa | LightPassKind::LightBleed => 5,
            _ => 2,
        })
        .and_then(|n| {
            n.checked_add(if scene.direction.is_some() {
                samples
            } else {
                0
            })
        })
        .ok_or(IsoError::Limit("atlas raster work"))?;
    scene.raster_samples = scene
        .raster_samples
        .checked_add(work)
        .ok_or(IsoError::Limit("atlas raster work"))?;
    if scene.raster_samples > budget.max_raster_samples {
        return Err(IsoError::Limit("atlas raster work"));
    }
    for y in pass.scissor[1]..pass.scissor[1] + pass.scissor[3] {
        for x in pass.scissor[0]..pass.scissor[0] + pass.scissor[2] {
            let tile = Vec2::new((x as f32 + 0.5) / r as f32, (y as f32 + 0.5) / r as f32);
            if maps.room_at(pass.floor, tile)? != pass.room {
                continue;
            }
            let p = tile * 16.;
            let target = &mut scene.color.pixels
                [((y + origin[1]) * scene.color.width + x + origin[0]) as usize];
            if pass.kind == LightPassKind::MultiplyOutside {
                for (value, factor) in target.iter_mut().zip(pass.uniforms.color) {
                    *value = (f32::from(*value) * factor).round().clamp(0., 255.) as u8;
                }
            } else {
                let value = light_value(pass, p, maps, wall, object, outside, ultra);
                for (target, value) in target.iter_mut().zip(value) {
                    *target = (f32::from(*target) + value * 255.).round().clamp(0., 255.) as u8;
                }
            }
        }
    }
    if let Some(direction) = &mut scene.direction {
        let ratio = r / 4;
        let s = pass.scissor.map(|v| v / ratio);
        let offset = [
            u32::from(pass.floor % 3) * (maps.width - 1) * 4,
            u32::from(pass.floor / 3) * (maps.height - 1) * 4,
        ];
        for y in s[1]..s[1] + s[3] {
            for x in s[0]..s[0] + s[2] {
                let tile = Vec2::new((x as f32 + 0.5) / 4., (y as f32 + 0.5) / 4.);
                if maps.room_at(pass.floor, tile)? != pass.room {
                    continue;
                }
                let target = &mut direction.pixels
                    [((y + offset[1]) * direction.width + x + offset[0]) as usize];
                if pass.kind == LightPassKind::MultiplyOutside {
                    for v in target.iter_mut() {
                        *v = half_round(*v * pass.uniforms.color[3]);
                    }
                    for (v, limit) in target[..3].iter_mut().zip([
                        pass.uniforms.direction.x,
                        pass.uniforms.direction.y,
                        pass.uniforms.direction.z,
                    ]) {
                        *v = half_round(v.clamp(
                            -limit.abs() * pass.uniforms.color[3],
                            limit.abs() * pass.uniforms.color[3],
                        ));
                    }
                } else {
                    let p = tile * 16.;
                    let u = pass.uniforms;
                    let distance =
                        (p.x - u.position_sixteenths.x).hypot(p.y - u.position_sixteenths.y);
                    let outdoor_pass = matches!(
                        pass.kind,
                        LightPassKind::Outdoors
                            | LightPassKind::OutdoorsSsaa
                            | LightPassKind::LightBleed
                    );
                    let shadow = outside.map_or_else(
                        || sample(wall, p, maps, 0),
                        |image| {
                            super::wcrc::ssaa(
                                image,
                                Vec2::new(
                                    p.x / ((maps.width - 1) as f32 * 16.),
                                    p.y / ((maps.height - 1) as f32 * 16.),
                                ),
                            )[0]
                        },
                    );
                    let power = if pass.kind == LightPassKind::LightBleed {
                        1.25
                    } else if outdoor_pass {
                        u.shadow_powers[0]
                    } else {
                        1.
                    };
                    let strength = if outdoor_pass {
                        1.
                    } else {
                        (1. - distance / u.radius_sixteenths)
                            .clamp(0., 1.)
                            .powf(2.2)
                    } * (1. - shadow * power)
                        * u.color[3];
                    let d = if outdoor_pass {
                        u.direction
                    } else {
                        Vec3::new(
                            p.x - u.position_sixteenths.x,
                            -u.height_sixteenths,
                            p.y - u.position_sixteenths.y,
                        )
                        .normalize_or_zero()
                    };
                    for (out, value) in target.iter_mut().zip([
                        d.x * strength,
                        d.y * strength,
                        d.z * strength,
                        strength,
                    ]) {
                        *out = half_round(*out + value);
                        if !out.is_finite() {
                            return Err(IsoError::Invalid("direction atlas overflow"));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
fn sample(image: Option<&RgbaImage>, p: Vec2, maps: &PreparedRoomMaps, channel: usize) -> f32 {
    image.map_or(0., |image| {
        let x = (p.x / ((maps.width - 1) as f32 * 16.) * image.width as f32)
            .floor()
            .clamp(0., (image.width - 1) as f32) as u32;
        let y = (p.y / ((maps.height - 1) as f32 * 16.) * image.height as f32)
            .floor()
            .clamp(0., (image.height - 1) as f32) as u32;
        f32::from(image.pixels[(y * image.width + x) as usize][channel]) / 255.
    })
}
fn light_value(
    pass: &PreparedLightPass,
    p: Vec2,
    maps: &PreparedRoomMaps,
    wall: Option<&RgbaImage>,
    object: Option<&RgbaImage>,
    outside: Option<&RgbaImage>,
    ultra: bool,
) -> [f32; 4] {
    let u = pass.uniforms;
    if let Some(image) = outside {
        let shadow = super::wcrc::ssaa(
            image,
            Vec2::new(
                p.x / ((maps.width - 1) as f32 * 16.),
                p.y / ((maps.height - 1) as f32 * 16.),
            ),
        );
        let (light, floor) = if pass.kind == LightPassKind::LightBleed {
            let light = 1. - shadow[0] * 1.25;
            (light, light)
        } else {
            let r = shadow[0] * u.shadow_powers[0];
            let g = shadow[1] * u.shadow_powers[1];
            (1. - r, 1. - r.max(g))
        };
        return [
            u.color[0] * light,
            u.color[1] * light,
            u.color[2] * light,
            u.color[3] * floor,
        ];
    }
    let distance =
        (p.x - u.position_sixteenths.x).hypot(p.y - u.position_sixteenths.y) / u.radius_sixteenths;
    let wall = sample(wall, p, maps, 0) * u.shadow_powers[0];
    let floor = if ultra && matches!(pass.kind, LightPassKind::Point(_)) {
        let min = if u.outdoors_color {
            1. / (maps.width as f32 * 9.)
        } else {
            0.
        };
        let max = 1. / (maps.width as f32 * 5.);
        let spacing = min + (max - min) * distance;
        let mut sum = 0.;
        for y in -2..=2 {
            for x in -2..=2 {
                sum += sample_linear(
                    object,
                    p + Vec2::new(
                        x as f32 * (maps.width - 1) as f32 * 16. * spacing,
                        y as f32 * (maps.height - 1) as f32 * 16. * spacing,
                    ),
                    maps,
                    1,
                );
            }
        }
        sum / 25.
    } else {
        sample(object, p, maps, 1)
    } * u.shadow_powers[1];
    let strength = if pass.kind == LightPassKind::Outdoors {
        1.
    } else {
        (1. - distance).clamp(0., 1.).powf(2.2)
    } * (1. - wall);
    [
        u.color[0] * strength,
        u.color[1] * strength,
        u.color[2] * strength,
        u.color[3] * (strength * (1. - floor)).min(1.),
    ]
}
fn sample_linear(
    image: Option<&RgbaImage>,
    p: Vec2,
    maps: &PreparedRoomMaps,
    channel: usize,
) -> f32 {
    image.map_or(0., |image| {
        let x = p.x / ((maps.width - 1) as f32 * 16.) * image.width as f32 - 0.5;
        let y = p.y / ((maps.height - 1) as f32 * 16.) * image.height as f32 - 0.5;
        let at = |x: f32, y: f32| {
            f32::from(
                image.pixels[(y.clamp(0., (image.height - 1) as f32) as u32 * image.width
                    + x.clamp(0., (image.width - 1) as f32) as u32)
                    as usize][channel],
            ) / 255.
        };
        let a = at(x.floor(), y.floor());
        let b = at(x.floor() + 1., y.floor());
        let c = at(x.floor(), y.floor() + 1.);
        let d = at(x.floor() + 1., y.floor() + 1.);
        let fx = x - x.floor();
        let fy = y - y.floor();
        (a + (b - a) * fx) * (1. - fy) + (c + (d - c) * fx) * fy
    })
}
// This literal is the exact IEEE binary16 2^-14 boundary, not a rounded fit.
#[allow(clippy::excessive_precision)]
fn half_round(value: f32) -> f32 {
    if !value.is_finite() || value == 0. {
        return value;
    }
    let a = value.abs();
    if a > 65504. {
        return value.signum() * f32::INFINITY;
    }
    let step = if a < 0.00006103515625 {
        2f32.powi(-24)
    } else {
        2f32.powi((a.log2().floor() as i32) - 10)
    };
    let scaled = a / step;
    let base = scaled.floor();
    let rem = scaled - base;
    let n = if rem > 0.5 || (rem == 0.5 && (base as u32) & 1 != 0) {
        base + 1.
    } else {
        base
    };
    value.signum() * n * step
}
