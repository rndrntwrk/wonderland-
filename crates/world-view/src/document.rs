use crate::WorldError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use wonderland_render_core::{Aabb, AssetKey, EntityRef, Mesh, RgbaImage, Vec2, Vec3};

pub const WORLD_SCHEMA_VERSION: u16 = 1;
pub const ORIGINAL_SOURCE_REVISION: &str = "4c6b3e8f5835b228723caea3c9f683c62f244f73";
pub const ORIGINAL_EMPTY_LOT_PATH: &str =
    "TSOClient/FSO.Content.TSO/Content/Blueprints/empty_lot_fso.xml";
pub const ORIGINAL_EMPTY_LOT_XML: &str =
    include_str!("../../../apps/web-shell/public/assets/world/empty_lot_fso.xml");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldSourceKind {
    OriginalXml,
    SavedWorld,
    LiveSession,
    /// FSOv refresh projection; revision fields fence presentation, not commands.
    LegacySnapshot,
    TestFixture,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldProvenance {
    pub kind: WorldSourceKind,
    pub origin: String,
    pub source_revision: String,
    pub effective_source: AssetKey,
}

/// Decimal strings on the wire preserve identities above JavaScript's 2^53.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldRevision {
    #[serde(with = "optional_decimal")]
    pub lot_id: Option<u64>,
    #[serde(with = "decimal")]
    pub epoch: u64,
    #[serde(with = "decimal")]
    pub tick: u64,
    #[serde(with = "decimal")]
    pub architecture_revision: u64,
    pub content: AssetKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerrainBoundary {
    /// TerrainComponent.GetElevationPoint returns raw zero at right/bottom edges.
    TerrainComponentZero,
    /// A producer supplies every corner explicitly; no adapter reshaping occurs.
    ExplicitCorners,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldTerrain {
    pub boundary: TerrainBoundary,
    pub corners: Vec<i16>,
    pub base_alt: i16,
    pub altitude_centers: Vec<i16>,
    pub grass: Vec<u8>,
    pub light: [f32; 4],
    pub dark: [f32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldDiagonal {
    Vertical,
    Horizontal,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldWall {
    /// West, north, south, east, matching the source RC wall mapping.
    pub patterns: [u16; 4],
    pub styles: [u16; 2],
    pub object_styles: [u16; 2],
    pub west: bool,
    pub north: bool,
    pub south: bool,
    pub east: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldTile {
    pub floor: u16,
    pub half_floors: Option<[u16; 2]>,
    pub diagonal: Option<WorldDiagonal>,
    /// None is unresolved source room/support state, never inferred from floors.
    pub indoors: Option<bool>,
    pub supported: Option<bool>,
    pub wall: WorldWall,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldRoof {
    pub material: u32,
    pub pitch: f32,
    pub advanced: bool,
    pub average_color: [f32; 4],
    pub texture_scale: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldLot {
    pub width: u16,
    pub height: u16,
    pub levels: u8,
    pub terrain: WorldTerrain,
    /// Row-major tiles, followed by the next one-based level.
    pub tiles: Vec<WorldTile>,
    pub roof: Option<WorldRoof>,
    /// Source/provider cut map, one width*height entry per displayed level.
    pub cutaway: Option<Vec<bool>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldSurface {
    Terrain,
    Floor,
    Wall,
    WallTop,
    Pool,
    Water,
    Roof,
    RoofRim,
    RoofUnderside,
    RoofEdge,
    BuildSupport,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldMaterial {
    pub surface: WorldSurface,
    pub material: u32,
    pub style: u16,
    pub source: String,
    pub effective_asset: AssetKey,
    pub image: RgbaImage,
    pub uv_scale: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintObject {
    pub record: u32,
    pub x: i32,
    pub y: i32,
    pub level: i32,
    pub direction: i32,
    pub group: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotObject {
    pub record: u32,
    pub object_id: i16,
    pub persistent_id: u32,
    pub x: i16,
    pub y: i16,
    pub level: i8,
    pub avatar: bool,
    #[serde(with = "decimal")]
    pub presentation_generation: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldObject {
    pub source_guid: u32,
    /// A source XML record is distinct from an admitted live instance identity.
    pub blueprint: Option<BlueprintObject>,
    #[serde(default)]
    pub snapshot: Option<SnapshotObject>,
    pub entity: Option<EntityRef>,
    #[serde(with = "decimal")]
    pub visual_revision: u64,
    pub position_tiles: Vec3,
    pub yaw_radians: f32,
    #[serde(with = "decimal_pair")]
    pub dynamic_flags: [u64; 2],
    pub room: u16,
    pub level: u8,
    pub visible: bool,
    pub selectable: bool,
    pub model: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelContext {
    Dgrp {
        effective_iff: AssetKey,
        chunk_id: u16,
    },
    Standalone,
    /// CPU-skinned original avatar geometry in source Y-up graphics units.
    /// Its one static group already includes source scale, pose and display tint.
    /// Source format 1 here is this normalized posed-mesh contract, not FSOm.
    Vitaboy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelTextureSelector {
    Sprite { rotation: u16, ordinal: u16 },
    Custom { id: u16 },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelTexture {
    pub selector: ModelTextureSelector,
    pub effective_asset: AssetKey,
    pub uv_scale: Vec2,
    pub image: RgbaImage,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelPart {
    pub texture: usize,
    pub mesh: Mesh,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMaskKind {
    Normal,
    Portal,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelDepthMask {
    pub kind: ModelMaskKind,
    pub mesh: Mesh,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldModel {
    pub effective_source: AssetKey,
    pub effective_content: AssetKey,
    pub context: ModelContext,
    pub format_version: u32,
    pub reconstruction_version: u32,
    /// Complete source file ordering: group 0 static, groups 1..128 dynamic.
    pub groups: Vec<Vec<ModelPart>>,
    pub textures: Vec<ModelTexture>,
    pub bounds: Aabb,
    pub depth_mask: Option<ModelDepthMask>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldDiagnostic {
    pub code: String,
    pub resource: String,
    pub message: String,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCounts {
    pub floors: usize,
    pub walls: usize,
    pub pools: usize,
    pub objects: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlueprintSound {
    pub id: u32,
    pub on: i32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldDocument {
    pub schema_version: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lighting: Option<crate::WorldLighting>,
    pub provenance: WorldProvenance,
    pub revision: WorldRevision,
    pub lot: WorldLot,
    pub objects: Vec<WorldObject>,
    pub models: Vec<WorldModel>,
    pub materials: Vec<WorldMaterial>,
    pub source_counts: Option<SourceCounts>,
    pub category: Option<i32>,
    pub sounds: Vec<BlueprintSound>,
    pub diagnostics: Vec<WorldDiagnostic>,
}

impl WorldDocument {
    pub fn original_empty_lot() -> Result<Self, WorldError> {
        Self::from_blueprint_xml(
            ORIGINAL_EMPTY_LOT_XML,
            ORIGINAL_EMPTY_LOT_PATH,
            ORIGINAL_SOURCE_REVISION,
        )
    }
    pub fn from_blueprint_xml(xml: &str, origin: &str, revision: &str) -> Result<Self, WorldError> {
        crate::blueprint::parse(xml, origin, revision)
    }
    pub fn validate(&self) -> Result<(), WorldError> {
        let invalid = |message: &str| WorldError(message.into());
        if self.schema_version != WORLD_SCHEMA_VERSION
            && !(self.schema_version == crate::LIGHTING_WORLD_SCHEMA_VERSION
                && self.lighting.is_some())
        {
            return Err(invalid("unsupported world document schema version"));
        }
        let area = checked_area(self.lot.width, self.lot.height)?;
        if let Some(lighting) = &self.lighting {
            lighting.validate(self)?;
        }
        let cells = area * usize::from(self.lot.levels);
        if self.lot.levels == 0 || self.lot.levels > 16 || cells > 262_144 {
            return Err(invalid(
                "world exceeds the renderer's bounded level/cell capacity",
            ));
        }
        if self.lot.tiles.len() != cells
            || self.lot.terrain.corners.len()
                != (usize::from(self.lot.width) + 1) * (usize::from(self.lot.height) + 1)
            || self.lot.terrain.altitude_centers.len() != area
            || self.lot.terrain.grass.len() != area
            || self
                .lot
                .cutaway
                .as_ref()
                .is_some_and(|map| map.len() != cells)
        {
            return Err(invalid("world array length does not match dimensions"));
        }
        if self.provenance.origin.is_empty() || self.provenance.source_revision.is_empty() {
            return Err(invalid("world source provenance is required"));
        }
        if self.provenance.kind == WorldSourceKind::LiveSession && self.revision.lot_id.is_none() {
            return Err(invalid("a live world requires its admitted lot identity"));
        }
        if self
            .lot
            .terrain
            .light
            .iter()
            .chain(self.lot.terrain.dark.iter())
            .any(|value| !value.is_finite() || !(0. ..=1.).contains(value))
        {
            return Err(invalid("invalid terrain color"));
        }
        if let Some(roof) = self.lot.roof
            && (!roof.pitch.is_finite()
                || !(0. ..=4.).contains(&roof.pitch)
                || !roof.texture_scale.is_finite()
                || !(0. ..=1_000_000.).contains(&roof.texture_scale)
                || roof.texture_scale == 0.
                || roof
                    .average_color
                    .iter()
                    .any(|value| !value.is_finite() || !(0. ..=1.).contains(value)))
        {
            return Err(invalid("invalid roof appearance"));
        }
        if self.objects.len() > 65_535
            || self.models.len() > 65_535
            || self.materials.len() > 65_535
        {
            return Err(invalid(
                "world exceeds the renderer's bounded resource capacity",
            ));
        }
        let mut entities = BTreeSet::new();
        let mut source_records = BTreeSet::new();
        let mut snapshot_ids = BTreeSet::new();
        for object in &self.objects {
            // Source VMGameObject centers may lie at x/y=0; its WorldUI
            // subtracts half a tile before the FSOm renderer adds the center.
            let origin = if object.snapshot.is_some_and(|source| !source.avatar) {
                -0.5
            } else {
                0.
            };
            if !object.position_tiles.is_finite()
                || !object.yaw_radians.is_finite()
                || object.level > self.lot.levels
                || (object.level == 0 && (object.visible || object.selectable))
                || (object.level != 0
                    && (object.position_tiles.x < origin
                        || object.position_tiles.y < origin
                        || object.position_tiles.x >= f32::from(self.lot.width) + origin
                        || object.position_tiles.y >= f32::from(self.lot.height) + origin))
                || object.model.is_some_and(|index| index >= self.models.len())
            {
                return Err(invalid("invalid world object position or model reference"));
            }
            if let Some(entity) = object.entity
                && (entity.generation == 0 || !entities.insert(entity.object_id))
            {
                return Err(invalid(
                    "duplicate or unversioned live world object identity",
                ));
            }
            if let Some(source) = object.blueprint
                && (!source_records.insert(source.record)
                    || source.level < 0
                    || source.level > i32::from(self.lot.levels))
            {
                return Err(invalid("invalid source blueprint object record"));
            }
            if let Some(source) = object.snapshot
                && (object.entity.is_some()
                    || object.blueprint.is_some()
                    || source.object_id <= 0
                    || !snapshot_ids.insert(source.object_id)
                    || !source_records.insert(source.record))
            {
                return Err(invalid("ambiguous or duplicate snapshot object identity"));
            }
            if object
                .model
                .is_some_and(|index| matches!(self.models[index].context, ModelContext::Vitaboy))
                && (object.blueprint.is_some()
                    || object.snapshot.is_some_and(|source| !source.avatar))
            {
                return Err(invalid(
                    "avatar geometry requires an avatar source identity",
                ));
            }
        }
        if let Some(counts) = self.source_counts
            && (counts.objects != self.objects.len()
                || counts.floors > cells
                || counts.walls > cells
                || counts.pools > cells)
        {
            return Err(invalid("source record count does not match world document"));
        }
        let limits = wonderland_render_core::RenderLimits::default();
        let mut bindings = BTreeSet::new();
        let mut texture_pixels = 0usize;
        let mut model_vertices = 0usize;
        let mut model_indices = 0usize;
        for material in &self.materials {
            if material.source.is_empty()
                || !bindings.insert((material.surface, material.material, material.style))
                || !material.uv_scale.is_finite()
                || material.uv_scale.x <= 0.
                || material.uv_scale.y <= 0.
            {
                return Err(invalid("invalid or duplicate world material binding"));
            }
            material
                .image
                .validate(&limits)
                .map_err(|error| invalid(&error.to_string()))?;
            texture_pixels = texture_pixels
                .checked_add(material.image.pixels.len())
                .ok_or_else(|| invalid("world texture pixel overflow"))?;
        }
        for model in &self.models {
            if !(1..=3).contains(&model.format_version)
                || model.groups.len() > 129
                || (model.reconstruction_version != 0
                    && !(2..=i32::MAX as u32).contains(&model.reconstruction_version))
                || Aabb::new(model.bounds.min, model.bounds.max).is_none()
                || (model.depth_mask.is_some() && model.format_version != 3)
            {
                return Err(invalid("invalid source object model format or bounds"));
            }
            if matches!(model.context, ModelContext::Vitaboy)
                && (model.format_version != 1
                    || model.reconstruction_version != 0
                    || model.groups.len() != 1
                    || model.depth_mask.is_some())
            {
                return Err(invalid("invalid posed avatar model contract"));
            }
            let mut selectors = BTreeSet::new();
            for texture in &model.textures {
                let key = match texture.selector {
                    ModelTextureSelector::Sprite { rotation, ordinal } => {
                        if rotation > 3 || !matches!(model.context, ModelContext::Dgrp { .. }) {
                            return Err(invalid("invalid DGRP texture selector"));
                        }
                        (0, rotation, ordinal)
                    }
                    ModelTextureSelector::Custom { id } => (1, 0, id),
                };
                if !selectors.insert(key)
                    || !texture.uv_scale.is_finite()
                    || texture.uv_scale.x <= 0.
                    || texture.uv_scale.y <= 0.
                    || texture.uv_scale.x > 1.
                    || texture.uv_scale.y > 1.
                {
                    return Err(invalid("invalid source object texture binding"));
                }
                texture
                    .image
                    .validate(&limits)
                    .map_err(|error| invalid(&error.to_string()))?;
                texture_pixels = texture_pixels
                    .checked_add(texture.image.pixels.len())
                    .ok_or_else(|| invalid("world texture pixel overflow"))?;
            }
            for group in &model.groups {
                let mut textures = BTreeSet::new();
                for part in group {
                    if part.texture >= model.textures.len()
                        || (!matches!(model.context, ModelContext::Vitaboy)
                            && !textures.insert(part.texture))
                    {
                        return Err(invalid("invalid model material group"));
                    }
                    part.mesh
                        .validate(&limits)
                        .map_err(|error| invalid(&error.to_string()))?;
                    model_vertices = model_vertices
                        .checked_add(part.mesh.vertices.len())
                        .ok_or_else(|| invalid("world model vertex overflow"))?;
                    model_indices = model_indices
                        .checked_add(part.mesh.indices.len())
                        .ok_or_else(|| invalid("world model index overflow"))?;
                }
            }
            if let Some(mask) = &model.depth_mask {
                mask.mesh
                    .validate(&limits)
                    .map_err(|error| invalid(&error.to_string()))?;
                model_vertices = model_vertices
                    .checked_add(mask.mesh.vertices.len())
                    .ok_or_else(|| invalid("world model vertex overflow"))?;
                model_indices = model_indices
                    .checked_add(mask.mesh.indices.len())
                    .ok_or_else(|| invalid("world model index overflow"))?;
            }
        }
        if texture_pixels > 16_777_216
            || model_vertices > limits.max_vertices
            || model_indices > limits.max_indices
        {
            return Err(invalid(
                "world assets exceed the renderer's bounded memory capacity",
            ));
        }
        Ok(())
    }
}

pub(crate) fn checked_area(width: u16, height: u16) -> Result<usize, WorldError> {
    if width == 0 || height == 0 || width > 256 || height > 256 {
        return Err(WorldError(
            "world exceeds the renderer's bounded dimension capacity".into(),
        ));
    }
    Ok(usize::from(width) * usize::from(height))
}

pub fn source_terrain(
    width: u16,
    height: u16,
    heights: &[i16],
    base_alt: i16,
) -> Result<WorldTerrain, WorldError> {
    let area = checked_area(width, height)?;
    if heights.len() != area {
        return Err(WorldError("source terrain height count".into()));
    }
    let (width, height) = (usize::from(width), usize::from(height));
    let mut corners = vec![0; (width + 1) * (height + 1)];
    let mut altitude_centers = Vec::with_capacity(area);
    for y in 0..height {
        corners[y * (width + 1)..y * (width + 1) + width]
            .copy_from_slice(&heights[y * width..(y + 1) * width]);
        for x in 0..width {
            let right = (x + 1) % width;
            let bottom = (y + 1) % height;
            // VMArchitectureTerrain.RegenerateCenters uses wrapped neighbors
            // and signed integer division, separately from the rendering edge.
            let sum = i32::from(heights[y * width + x])
                + i32::from(heights[y * width + right])
                + i32::from(heights[bottom * width + x])
                + i32::from(heights[bottom * width + right]);
            altitude_centers.push((sum / 4) as i16);
        }
    }
    Ok(WorldTerrain {
        boundary: TerrainBoundary::TerrainComponentZero,
        corners,
        base_alt,
        altitude_centers,
        grass: vec![0; area],
        // LotTypeGrassInfo, TerrainType.GRASS. These are source vertex colors,
        // not a substitute floor/wall texture or a claimed original grass shader.
        light: [80. / 255., 116. / 255., 59. / 255., 1.],
        dark: [157. / 255., 117. / 255., 65. / 255., 1.],
    })
}

pub(crate) mod decimal {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}
mod optional_decimal {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(value) => serializer.serialize_some(&value.to_string()),
            None => serializer.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<u64>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|value| value.parse().map_err(serde::de::Error::custom))
            .transpose()
    }
}
mod decimal_pair {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(value: &[u64; 2], serializer: S) -> Result<S::Ok, S::Error> {
        value.map(|part| part.to_string()).serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u64; 2], D::Error> {
        let values = <[String; 2]>::deserialize(deserializer)?;
        Ok([
            values[0].parse().map_err(serde::de::Error::custom)?,
            values[1].parse().map_err(serde::de::Error::custom)?,
        ])
    }
}
