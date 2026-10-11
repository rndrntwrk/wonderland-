use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use wonderland_render_3d::{
    camera::{OrbitCamera, cut_rotation},
    lot, objects as source_objects,
    reconstruction::MaskType,
};
use wonderland_render_core::{EntityRef, Mat4, Mesh, Quat, RgbaImage, Vec2, Vec3, Vertex};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallMode {
    Up,
    Down,
    Cutaway,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewportControls {
    pub yaw_radians: f32,
    pub pitch_radians: f32,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub visible_level: u8,
    pub walls: WallMode,
    pub show_roofs: bool,
}
impl Default for ViewportControls {
    fn default() -> Self {
        Self {
            yaw_radians: -std::f32::consts::FRAC_PI_4,
            pitch_radians: 1.1,
            zoom: 1.,
            pan_x: 0.,
            pan_y: 0.,
            visible_level: 1,
            walls: WallMode::Up,
            show_roofs: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScenePart {
    /// One-based source floor, including tile-less roof geometry.
    pub level: u8,
    /// None for architecture; Some preserves each source object material pass.
    pub pipeline: Option<wonderland_render_core::reference::FragmentPipeline>,
    pub mesh: Arc<Mesh>,
    pub transform: Mat4,
    pub surface: Option<WorldSurface>,
    pub tile: Option<(u16, u16, u8)>,
    pub object: Option<usize>,
    pub material: u32,
    pub texture: Option<Arc<RgbaImage>>,
}
#[derive(Clone)]
pub struct PreparedWorld {
    pub parts: Vec<ScenePart>,
    pub diagnostics: Vec<WorldDiagnostic>,
}

/// Allocation/work guards for the complete expanded scene, not per resource.
/// Model groups are counted conservatively even when a dynamic bit hides them.
#[derive(Clone, Copy, Debug)]
pub struct SceneBudget {
    pub max_vertices: usize,
    pub max_indices: usize,
    pub max_parts: usize,
    pub max_buffer_bytes: usize,
}
impl Default for SceneBudget {
    fn default() -> Self {
        Self {
            max_vertices: 2_000_000,
            max_indices: 6_000_000,
            max_parts: 262_144,
            max_buffer_bytes: 256 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct SceneUsage {
    vertices: usize,
    indices: usize,
    parts: usize,
    bytes: usize,
}
fn capacity_error(kind: &str) -> WorldError {
    WorldError(format!("expanded world scene budget: {kind}"))
}
fn buffer_bytes(vertices: usize, indices: usize, parts: usize) -> Result<usize, WorldError> {
    vertices
        .checked_mul(size_of::<Vertex>())
        .and_then(|bytes| {
            indices
                .checked_mul(size_of::<u32>())
                .and_then(|indices| bytes.checked_add(indices))
        })
        .and_then(|bytes| {
            parts
                .checked_mul(size_of::<ScenePart>() + size_of::<Mesh>() + 2 * size_of::<usize>())
                .and_then(|parts| bytes.checked_add(parts))
        })
        .ok_or_else(|| capacity_error("bytes"))
}
impl SceneUsage {
    fn buffers(&mut self, bytes: usize, budget: SceneBudget) -> Result<(), WorldError> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .filter(|&bytes| bytes <= budget.max_buffer_bytes)
            .ok_or_else(|| capacity_error("bytes"))?;
        Ok(())
    }
    fn geometry(
        &mut self,
        vertices: usize,
        indices: usize,
        parts: usize,
        budget: SceneBudget,
    ) -> Result<(), WorldError> {
        self.vertices = self
            .vertices
            .checked_add(vertices)
            .filter(|&count| count <= budget.max_vertices)
            .ok_or_else(|| capacity_error("vertices"))?;
        self.indices = self
            .indices
            .checked_add(indices)
            .filter(|&count| count <= budget.max_indices)
            .ok_or_else(|| capacity_error("indices"))?;
        self.parts = self
            .parts
            .checked_add(parts)
            .filter(|&count| count <= budget.max_parts)
            .ok_or_else(|| capacity_error("parts"))?;
        self.buffers(buffer_bytes(vertices, indices, parts)?, budget)
    }
}

fn visible_model(object: &WorldObject, level: u8) -> Option<usize> {
    (object.visible && object.level != 0 && object.level <= level)
        .then_some(object.model)
        .flatten()
}

// Run before source geometry or prepared resource allocation. The normalized
// input is already validated; unique-input limits alone cannot bound instances.
fn reserve_objects(
    document: &WorldDocument,
    controls: ViewportControls,
    budget: SceneBudget,
) -> Result<SceneUsage, WorldError> {
    let mut usage = SceneUsage::default();
    for material in &document.materials {
        usage.buffers(
            material
                .image
                .pixels
                .len()
                .checked_mul(4)
                .ok_or_else(|| capacity_error("bytes"))?,
            budget,
        )?;
    }
    let mut unique = BTreeSet::new();
    for object in &document.objects {
        let Some(index) = visible_model(object, controls.visible_level) else {
            continue;
        };
        let model = &document.models[index];
        let first = unique.insert(index);
        for part in model.groups.iter().flatten() {
            usage.geometry(part.mesh.vertices.len(), part.mesh.indices.len(), 1, budget)?;
            if first {
                // Peak preparation has one source-prepared mesh beside the
                // shared UV-adjusted/shaded mesh. Input document is caller owned.
                usage.buffers(
                    buffer_bytes(part.mesh.vertices.len(), part.mesh.indices.len(), 1)?,
                    budget,
                )?;
            }
        }
        if let Some(mask) = &model.depth_mask {
            let passes = if mask.kind == ModelMaskKind::Portal {
                3
            } else {
                2
            };
            // Repeated draw work and one retained shared prepared mask are
            // separate bounds. Even hidden body groups cannot hide mask cost.
            usage.geometry(
                mask.mesh.vertices.len() * passes,
                mask.mesh.indices.len() * passes,
                passes,
                budget,
            )?;
            if first {
                usage.buffers(
                    buffer_bytes(mask.mesh.vertices.len(), mask.mesh.indices.len(), 1)?
                        .checked_mul(2)
                        .ok_or_else(|| capacity_error("bytes"))?,
                    budget,
                )?;
            }
        }
        if first {
            for texture in &model.textures {
                // PreparedFsom owns a copy, then the scene's one shared Arc copy.
                usage.buffers(
                    texture
                        .image
                        .pixels
                        .len()
                        .checked_mul(8)
                        .ok_or_else(|| capacity_error("bytes"))?,
                    budget,
                )?;
            }
        }
    }
    Ok(usage)
}

fn reserve_architecture(
    lot: &lot::VisualLot,
    pool: Option<&lot::PoolAssets>,
    controls: ViewportControls,
    usage: &mut SceneUsage,
    budget: SceneBudget,
) -> Result<(), WorldError> {
    let area = usize::from(lot.width) * usize::from(lot.height);
    // The source builder creates the ground tile even where a pool replaces it.
    usage.geometry(area * 4, area * 6, area, budget)?;
    for (index, tile) in lot
        .tiles
        .iter()
        .take(area * usize::from(controls.visible_level))
        .enumerate()
    {
        if tile.floor == lot::POOL {
            let assets = pool.ok_or_else(|| WorldError("source pool assets unavailable".into()))?;
            let coordinate = lot::TileCoord {
                x: (index % area % usize::from(lot.width)) as u16,
                y: (index % area / usize::from(lot.width)) as u16,
                level: (index / area + 1) as u8,
            };
            let (selector, corners) = lot::pool_selector(lot::pool_neighbors(lot, coordinate));
            let selected = std::iter::once(&assets.tiles[15])
                .chain((selector != 15).then_some(&assets.tiles[usize::from(selector)]))
                .chain(
                    corners
                        .iter()
                        .map(|&corner| &assets.corners[usize::from(corner)]),
                );
            usage.geometry(0, 0, 1, budget)?;
            for mesh in selected {
                usage.geometry(mesh.vertices.len(), mesh.indices.len(), 0, budget)?;
            }
        } else if tile.diagonal.is_some() {
            for floor in tile.half_floors.unwrap_or([tile.floor; 2]) {
                if floor != 0 {
                    usage.geometry(4, 3, 1, budget)?;
                }
            }
        } else if tile.floor != 0 {
            usage.geometry(4, 6, 1, budget)?;
        }
        // Every source wall line is a quad. A thick west/north/diagonal wall
        // has at most four face/cap lines plus its top; neighbor joins reduce it.
        let thick = |style| style == 1 || style == 255;
        let wall = &tile.wall;
        let quads = usize::from(wall.west) * if thick(wall.styles[0]) { 5 } else { 1 }
            + usize::from(wall.north) * if thick(wall.styles[1]) { 5 } else { 1 }
            + usize::from(wall.south)
            + usize::from(wall.east)
            + usize::from(tile.diagonal.is_some()) * if thick(wall.styles[1]) { 5 } else { 2 };
        usage.geometry(quads * 4, quads * 6, quads, budget)?;
    }
    if controls.show_roofs
        && let Some(roof) = lot.roof
    {
        for level in 2..=lot.levels.min(controls.visible_level) + 1 {
            // Bounded source topology metadata only; no mesh is allocated.
            let count = lot::roof_rectangles(
                lot,
                level,
                lot::GeometryBudget::default().max_roof_evaluations,
            )?
            .len();
            let (vertices, indices, parts) = if roof.advanced {
                (84, 126, 4)
            } else {
                (16, 24, 1)
            };
            usage.geometry(count * vertices, count * indices, count * parts, budget)?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldPickTarget {
    Tile {
        x: u16,
        y: u16,
        level: u8,
        surface: WorldSurface,
    },
    Object {
        entity: Option<EntityRef>,
        source_guid: u32,
        source_record: Option<u32>,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct WorldPick {
    pub revision: WorldRevision,
    pub frame_generation: u64,
    pub target: WorldPickTarget,
    /// Coordinates in the canvas backing store, not CSS/client pixels.
    pub screen: [u32; 2],
}

pub fn visual_lot(document: &WorldDocument) -> Result<lot::VisualLot, WorldError> {
    document.validate()?;
    let source = &document.lot;
    Ok(lot::VisualLot {
        width: source.width,
        height: source.height,
        levels: source.levels,
        terrain: source.terrain.corners.clone(),
        base_alt: source.terrain.base_alt,
        altitude_centers: source.terrain.altitude_centers.clone(),
        grass: source.terrain.grass.clone(),
        terrain_light: source.terrain.light,
        terrain_dark: source.terrain.dark,
        roof_average_color: source.roof.map_or([1.; 4], |roof| roof.average_color),
        roof_texture_scale: source.roof.map_or(1., |roof| roof.texture_scale),
        roof: source.roof.map(|roof| lot::RoofStyle {
            material: roof.material,
            pitch: roof.pitch,
            advanced: roof.advanced,
        }),
        tiles: source
            .tiles
            .iter()
            .map(|tile| lot::VisualTile {
                floor: tile.floor,
                half_floors: tile.half_floors,
                diagonal: tile.diagonal.map(|diagonal| match diagonal {
                    WorldDiagonal::Vertical => lot::Diagonal::Vertical,
                    WorldDiagonal::Horizontal => lot::Diagonal::Horizontal,
                }),
                indoors: tile.indoors.unwrap_or(false),
                supported: tile.supported.unwrap_or(false),
                wall: lot::WallAppearance {
                    patterns: tile.wall.patterns,
                    styles: tile.wall.styles,
                    object_styles: tile.wall.object_styles,
                    west: tile.wall.west,
                    north: tile.wall.north,
                    south: tile.wall.south,
                    east: tile.wall.east,
                },
            })
            .collect(),
    })
}

pub(crate) fn validate_controls(
    document: &WorldDocument,
    controls: ViewportControls,
) -> Result<(), WorldError> {
    if [
        controls.yaw_radians,
        controls.pitch_radians,
        controls.zoom,
        controls.pan_x,
        controls.pan_y,
    ]
    .iter()
    .any(|value| !value.is_finite())
        || controls.zoom <= 0.
        || controls.visible_level == 0
        || controls.visible_level > document.lot.levels
    {
        return Err(WorldError("invalid world camera or visible floor".into()));
    }
    Ok(())
}

impl From<lot::SurfaceKind> for WorldSurface {
    fn from(value: lot::SurfaceKind) -> Self {
        match value {
            lot::SurfaceKind::Terrain => Self::Terrain,
            lot::SurfaceKind::Floor => Self::Floor,
            lot::SurfaceKind::Wall => Self::Wall,
            lot::SurfaceKind::WallTop => Self::WallTop,
            lot::SurfaceKind::Pool => Self::Pool,
            lot::SurfaceKind::Water => Self::Water,
            lot::SurfaceKind::Roof => Self::Roof,
            lot::SurfaceKind::RoofRim => Self::RoofRim,
            lot::SurfaceKind::RoofUnderside => Self::RoofUnderside,
            lot::SurfaceKind::RoofEdge => Self::RoofEdge,
            lot::SurfaceKind::BuildSupport => Self::BuildSupport,
        }
    }
}

fn shade(mesh: &mut Mesh, material: [f32; 4]) {
    let light = Vec3::new(-0.35, 0.85, 0.4).normalize_or_zero();
    for vertex in &mut mesh.vertices {
        let gain = 0.74 + 0.26 * vertex.normal.normalize_or_zero().dot(light).max(0.);
        for (index, component) in vertex.color.iter_mut().enumerate() {
            *component *= material[index] * if index == 3 { 1. } else { gain };
        }
    }
}

pub fn build_scene(
    document: &WorldDocument,
    controls: ViewportControls,
) -> Result<PreparedWorld, WorldError> {
    build_scene_with_budget(document, controls, SceneBudget::default())
}

pub fn build_scene_with_budget(
    document: &WorldDocument,
    controls: ViewportControls,
    budget: SceneBudget,
) -> Result<PreparedWorld, WorldError> {
    validate_controls(document, controls)?;
    document.validate()?;
    let objects = reserve_objects(document, controls, budget)?;
    let lot = visual_lot(document)?;
    let area = usize::from(lot.width) * usize::from(lot.height);
    let mut diagnostics = document.diagnostics.clone();
    let cutaway = match controls.walls {
        WallMode::Up => None,
        WallMode::Down => Some(lot::Cutaway {
            level: controls.visible_level,
            rotation: cut_rotation(controls.yaw_radians)?,
            tiles: vec![true; area],
        }),
        WallMode::Cutaway => {
            if let Some(map) = &document.lot.cutaway {
                let start = usize::from(controls.visible_level - 1) * area;
                Some(lot::Cutaway {
                    level: controls.visible_level,
                    rotation: cut_rotation(controls.yaw_radians)?,
                    tiles: map[start..start + area].to_vec(),
                })
            } else {
                diagnostics.push(WorldDiagnostic {
                    code: "missing_cutaway_map".into(),
                    resource: "architecture:cutaway".into(),
                    message: "A cutaway map is unavailable for this source. Walls remain up."
                        .into(),
                });
                None
            }
        }
    };
    let pool = if lot.tiles.iter().any(|tile| tile.floor == lot::POOL) {
        Some(source_pool_assets()?)
    } else {
        None
    };
    let mut usage = objects;
    if let Some((assets, texture)) = &pool {
        usage.buffers(
            texture
                .pixels
                .len()
                .checked_mul(4)
                .ok_or_else(|| capacity_error("bytes"))?,
            budget,
        )?;
        for mesh in assets.tiles.iter().chain(&assets.corners) {
            // BuildOptions holds a temporary copy of these fixed source assets.
            usage.buffers(
                buffer_bytes(mesh.vertices.len(), mesh.indices.len(), 1)?
                    .checked_mul(2)
                    .ok_or_else(|| capacity_error("bytes"))?,
                budget,
            )?;
        }
    }
    reserve_architecture(
        &lot,
        pool.as_ref().map(|(assets, _)| assets),
        controls,
        &mut usage,
        budget,
    )?;
    let output = lot::build_lot(
        &lot,
        &lot::BuildOptions {
            visible_level: controls.visible_level,
            show_roofs: controls.show_roofs,
            cutaway,
            pool_assets: pool.as_ref().map(|(assets, _)| assets.clone()),
            budget: lot::GeometryBudget {
                max_vertices: budget.max_vertices - objects.vertices,
                max_indices: budget.max_indices - objects.indices,
                ..Default::default()
            },
            ..Default::default()
        },
    )?;
    for missing in output.missing_assets {
        diagnostics.push(WorldDiagnostic {
            code: "missing_architecture_resource".into(),
            resource: missing.into(),
            message: format!("Source architecture resource is unavailable: {missing}."),
        });
    }
    let mut bindings: BTreeMap<_, _> = document
        .materials
        .iter()
        .map(|material| {
            (
                (material.surface, material.material, material.style),
                (Arc::new(material.image.clone()), material.uv_scale),
            )
        })
        .collect();
    if let Some((_, texture)) = pool {
        bindings
            .entry((WorldSurface::Pool, u32::from(lot::POOL), 0))
            .or_insert((Arc::new(texture), Vec2::new(1., 1.)));
    }
    let mut missing_materials = BTreeSet::new();
    let mut parts = Vec::with_capacity(output.parts.len());
    for part in output.parts {
        let surface = WorldSurface::from(part.kind);
        // In the original world the modelled floor replaces this terrain tile.
        // Keeping a full grass quad here would occlude the authored pool bowl.
        if surface == WorldSurface::Terrain
            && part.tile.is_some_and(|tile| {
                lot.tile_index(tile)
                    .is_some_and(|index| lot.tiles[index].floor == lot::POOL)
            })
        {
            continue;
        }
        let texture = bindings.get(&(surface, part.material, part.style));
        let mut mesh = part.mesh;
        let tint = if surface == WorldSurface::Terrain || texture.is_some() {
            [1.; 4]
        } else {
            [0.67, 0.66, 0.62, 1.]
        };
        if let Some((_, scale)) = texture {
            for vertex in &mut mesh.vertices {
                vertex.uv = Vec2::new(vertex.uv.x * scale.x, vertex.uv.y * scale.y);
            }
        } else if surface != WorldSurface::Terrain
            && missing_materials.insert((surface, part.material, part.style))
        {
            diagnostics.push(WorldDiagnostic { code: "missing_surface_texture".into(), resource: format!("{surface:?}:{}:{}", part.material, part.style), message: format!("Original {surface:?} material {} is unavailable. Its source geometry uses a neutral material.", part.material) });
        }
        shade(&mut mesh, tint);
        parts.push(ScenePart {
            level: part.level,
            pipeline: None,
            mesh: Arc::new(mesh),
            transform: Mat4::IDENTITY,
            surface: Some(surface),
            tile: part.tile.map(|tile| (tile.x, tile.y, tile.level)),
            object: None,
            material: part.material,
            texture: texture.map(|(image, _)| Arc::clone(image)),
        });
    }
    // Equal-depth authored flooring is drawn first. Strict default depth then
    // prevents a coplanar terrain triangle from overwriting that source floor.
    parts.sort_by_key(|part| part.surface == Some(WorldSurface::Terrain));
    let mut prepared_models = BTreeMap::new();
    let mut prepared_avatars = BTreeMap::<usize, (Vec<Arc<Mesh>>, Vec<Arc<RgbaImage>>)>::new();
    for (index, object) in document.objects.iter().enumerate() {
        if !object.visible || object.level == 0 || object.level > controls.visible_level {
            continue;
        }
        let Some(model_index) = object.model else {
            continue;
        };
        let model = &document.models[model_index];
        if matches!(model.context, ModelContext::Vitaboy) {
            let (meshes, images) = prepared_avatars.entry(model_index).or_insert_with(|| {
                (
                    model.groups[0]
                        .iter()
                        .map(|part| Arc::new(part.mesh.clone()))
                        .collect(),
                    model
                        .textures
                        .iter()
                        .map(|texture| Arc::new(texture.image.clone()))
                        .collect(),
                )
            });
            // AvatarComponent.Draw: Scale * RotationY(pi-direction) * World.
            // The normalized posed mesh already includes the source scale;
            // avatars receive raw tile centers, with no FSOm +1.5 translation.
            let rotation =
                Quat::from_axis_angle(Vec3::Y, std::f32::consts::PI - object.yaw_radians)
                    .ok_or_else(|| WorldError("invalid source avatar yaw".into()))?;
            let position = object.position_tiles;
            let transform = Mat4::from_translation(Vec3::new(
                position.x * 3.,
                position.z * 3.,
                position.y * 3.,
            )) * Mat4::from_quat(rotation);
            if images.iter().any(|image| {
                image
                    .pixels
                    .iter()
                    .any(|pixel| pixel[3] > 0 && pixel[3] < 255)
            }) || meshes.iter().any(|mesh| {
                mesh.vertices
                    .iter()
                    .any(|vertex| vertex.color[3] > 0. && vertex.color[3] < 1.)
            }) {
                diagnostics.push(WorldDiagnostic { code: "software_alpha_approximation".into(), resource: format!("avatar:record:{index}"), message: "The source avatar has translucent texels or display tint; software uses straight alpha compositing, without the original avatar material passes.".into() });
            }
            diagnostics.push(WorldDiagnostic { code: "unresolved_avatar_lighting".into(), resource: format!("avatar:record:{index}"), message: "Original avatar geometry and textures are displayed. Source room lighting, lightmaps and avatar shadows require the original lighting providers.".into() });
            diagnostics.push(WorldDiagnostic { code: "software_avatar_sampling".into(), resource: format!("avatar:record:{index}"), message: "Original avatar UVs use the source wrap addressing. The software renderer uses nearest texture filtering rather than the original linear/mipmap filtering.".into() });
            for (part_index, part) in model.groups[0].iter().enumerate() {
                parts.push(ScenePart {
                    level: object.level,
                    pipeline: None,
                    mesh: Arc::clone(&meshes[part_index]),
                    transform,
                    surface: None,
                    tile: None,
                    object: Some(index),
                    material: part.texture as u32,
                    texture: Some(Arc::clone(&images[part.texture])),
                });
            }
            continue;
        }

        if let std::collections::btree_map::Entry::Vacant(e) = prepared_models.entry(model_index) {
            e.insert(PreparedModel::new(model)?);
        }
        let cached = &prepared_models[&model_index];
        let prepared = &cached.source;
        // The source renderer requires an ID for its disposable scene record.
        // Local XML record IDs stay private and never enter the live FrameStore.
        let private_reference = EntityRef {
            object_id: (index + 1) as u32,
            generation: 1,
        };
        let scene = prepared
            .scene(
                source_objects::ObjectInstance {
                    entity: object.entity.unwrap_or(private_reference),
                    visual_revision: object.visual_revision,
                    position_tiles: object.position_tiles,
                    yaw_radians: object.yaw_radians,
                    dynamic_flags: object.dynamic_flags,
                    room: object.room,
                    level: object.level as i8,
                    directional_lighting: false,
                },
                source_objects::ObjectTarget::Color,
            )
            .map_err(|error| WorldError(error.to_string()))?;
        for draw in scene.draws {
            let (mesh, texture, material) = if let Some((group, part_index)) = draw.group_part {
                let part = &prepared.groups()[usize::from(group)][part_index];
                (
                    Arc::clone(&cached.meshes[usize::from(group)][part_index]),
                    Some(Arc::clone(&cached.images[part.texture_index()])),
                    part.texture_index() as u32,
                )
            } else {
                (
                    Arc::clone(
                        cached
                            .mask
                            .as_ref()
                            .expect("source mask command requires prepared geometry"),
                    ),
                    None,
                    0,
                )
            };
            parts.push(ScenePart {
                level: object.level,
                pipeline: Some(crate::materials::pipeline(draw.pipeline)),
                mesh,
                transform: scene.world,
                surface: None,
                tile: None,
                object: Some(index),
                material,
                texture,
            });
        }
    }
    Ok(PreparedWorld { parts, diagnostics })
}

struct PreparedModel {
    source: source_objects::PreparedFsom,
    meshes: Vec<Vec<Arc<Mesh>>>,
    images: Vec<Arc<RgbaImage>>,
    mask: Option<Arc<Mesh>>,
}
impl PreparedModel {
    fn new(model: &WorldModel) -> Result<Self, WorldError> {
        let source = prepare_model(model)?;
        let images: Vec<_> = source
            .textures()
            .iter()
            .map(|texture| Arc::new(texture.image().clone()))
            .collect();
        let mask = source.depth_mask().map(|(_, mesh)| Arc::new(mesh.clone()));
        let meshes = source
            .groups()
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|part| {
                        let mut mesh = part.mesh().clone();
                        let texture = &source.textures()[part.texture_index()];
                        for vertex in &mut mesh.vertices {
                            vertex.uv = texture
                                .shader_uv(vertex.uv)
                                .map_err(|error| WorldError(error.to_string()))?;
                        }
                        shade(&mut mesh, [1.; 4]);
                        Ok(Arc::new(mesh))
                    })
                    .collect::<Result<Vec<_>, WorldError>>()
            })
            .collect::<Result<Vec<_>, WorldError>>()?;
        Ok(Self {
            source,
            meshes,
            images,
            mask,
        })
    }
}

fn prepare_model(model: &WorldModel) -> Result<source_objects::PreparedFsom, WorldError> {
    let geometry = |mesh: &Mesh| source_objects::NormalizedGeometry {
        vertices: mesh
            .vertices
            .iter()
            .map(|vertex| source_objects::FsomVertex {
                position: vertex.position,
                uv: vertex.uv,
                normal: vertex.normal,
            })
            .collect(),
        indices: mesh.indices.clone(),
    };
    source_objects::PreparedFsom::prepare(
        source_objects::NormalizedFsom {
            identity: source_objects::ObjectMeshIdentity {
                effective_source: model.effective_source,
                effective_content: model.effective_content,
            },
            context: match model.context {
                ModelContext::Vitaboy => {
                    return Err(WorldError(
                        "avatar model cannot use object-model preparation".into(),
                    ));
                }
                ModelContext::Standalone => source_objects::FsomContext::Standalone,
                ModelContext::Dgrp {
                    effective_iff,
                    chunk_id,
                } => source_objects::FsomContext::Dgrp {
                    effective_iff,
                    chunk_id,
                },
            },
            format_version: model.format_version,
            reconstruction_version: model.reconstruction_version,
            groups: model
                .groups
                .iter()
                .map(|group| {
                    group
                        .iter()
                        .map(|part| source_objects::NormalizedPart {
                            texture: part.texture,
                            geometry: geometry(&part.mesh),
                        })
                        .collect()
                })
                .collect(),
            textures: model
                .textures
                .iter()
                .map(|texture| source_objects::NormalizedTexture {
                    selector: match texture.selector {
                        ModelTextureSelector::Sprite { rotation, ordinal } => {
                            source_objects::TextureSelector::Sprite { rotation, ordinal }
                        }
                        ModelTextureSelector::Custom { id } => {
                            source_objects::TextureSelector::Custom { id }
                        }
                    },
                    effective_asset: texture.effective_asset,
                    uv_scale: texture.uv_scale,
                    image: texture.image.clone(),
                })
                .collect(),
            bounds: model.bounds,
            depth_mask: model
                .depth_mask
                .as_ref()
                .map(|mask| source_objects::NormalizedDepthMask {
                    kind: match mask.kind {
                        ModelMaskKind::Normal => MaskType::Normal,
                        ModelMaskKind::Portal => MaskType::Portal,
                    },
                    geometry: geometry(&mask.mesh),
                }),
        },
        source_objects::ObjectLimits::default(),
    )
    .map_err(|error| WorldError(error.to_string()))
}

pub fn orbit_camera(
    document: &WorldDocument,
    controls: ViewportControls,
    aspect: f32,
) -> Result<OrbitCamera, WorldError> {
    if !aspect.is_finite() || aspect <= 0. {
        return Err(WorldError("invalid viewport aspect".into()));
    }
    validate_controls(document, controls)?;
    crate::document::checked_area(document.lot.width, document.lot.height)?;
    let extent = f32::from(document.lot.width.max(document.lot.height));
    let center = Vec2::new(
        f32::from(document.lot.width) / 2. + controls.pan_x.clamp(-extent, extent),
        f32::from(document.lot.height) / 2. + controls.pan_y.clamp(-extent, extent),
    );
    let center_x = (center.x.max(0.) as usize).min(usize::from(document.lot.width) - 1);
    let center_y = (center.y.max(0.) as usize).min(usize::from(document.lot.height) - 1);
    let ground = document
        .lot
        .terrain
        .altitude_centers
        .get(center_y * usize::from(document.lot.width) + center_x)
        .copied()
        .unwrap_or(0);
    let camera = OrbitCamera {
        yaw: controls.yaw_radians,
        pitch_control: controls.pitch_radians.clamp(0., std::f32::consts::PI),
        // The source orbit distance grows quadratically with its zoom control.
        // Fit the full source boundary on initial entry, including portrait views.
        zoom: (extent * 3.7 * (0.85 / aspect).max(1.)).sqrt()
            / controls.zoom.clamp(0.25, 8.).sqrt(),
        center,
        cam_height: (f32::from(ground) - f32::from(document.lot.terrain.base_alt)) * 9. / 160.
            + f32::from(controls.visible_level - 1) * 8.85,
    };
    Ok(camera)
}

/// Projection and positional sound share one validated camera calculation.
pub fn camera_projection(
    document: &WorldDocument,
    controls: ViewportControls,
    aspect: f32,
) -> Result<Mat4, WorldError> {
    let camera = orbit_camera(document, controls, aspect)?;
    let mut pose = camera.pose()?;
    pose.far = (f32::from(document.lot.width.max(document.lot.height)) * 48.).max(800.);
    pose.view_projection(aspect).map_err(Into::into)
}

#[cfg(test)]
#[path = "scene/material_tests.rs"]
mod material_tests;
