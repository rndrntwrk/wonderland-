//! Versioned source room-light recipes. Validate *every* input, including unused
//! shadow geometry, before the renderer admits a replacement world.
use crate::{WorldDocument, WorldError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use wonderland_render_core::{AssetKey, Mat4, RgbaImage, Vec2};
use wonderland_render_iso as iso;

pub const LIGHTING_WORLD_SCHEMA_VERSION: u16 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightRoomDiagonal {
    None,
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldLightCell {
    pub first: u16,
    pub second: u16,
    pub diagonal: LightRoomDiagonal,
    pub floor_pattern: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldLightRoom {
    pub id: u16,
    pub floor: u8,
    pub outside: bool,
    pub outside_light: u16,
    pub ambient_light: u16,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldRoomShadows {
    pub room: u16,
    pub floor: u8,
    /// Original shadow coordinates, in sixteenths of a tile.
    pub walls: Vec<[Vec2; 2]>,
    /// Original integer x/y/width/height object occluder rectangles.
    pub objects: Vec<[f32; 4]>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldPointLight {
    #[serde(with = "crate::document::decimal")]
    pub id: u64,
    pub room: u16,
    pub floor: u8,
    pub position_sixteenths: Vec2,
    pub radius_sixteenths: f32,
    pub falloff_multiplier: f32,
    pub color: [u8; 3],
    pub intensity: f32,
    pub outdoors_color: bool,
    pub window_room: Option<u16>,
    pub height: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldLighting {
    pub source: AssetKey,
    #[serde(with = "crate::document::decimal")]
    pub lot_id: u64,
    #[serde(with = "crate::document::decimal")]
    pub epoch: u64,
    #[serde(with = "crate::document::decimal")]
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub stories: u8,
    pub cells: Vec<WorldLightCell>,
    pub rooms: Vec<WorldLightRoom>,
    pub geometry: Vec<WorldRoomShadows>,
    pub lights: Vec<WorldPointLight>,
    pub minimum: [u8; 4],
    pub outside: [u8; 4],
}

fn invalid(message: &str) -> WorldError {
    WorldError(format!("world lighting: {message}"))
}
fn bounded_point(v: Vec2) -> bool {
    v.is_finite() && v.x.abs() <= 1_000_000. && v.y.abs() <= 1_000_000.
}
fn budget() -> iso::LightingBudget {
    iso::LightingBudget {
        max_cells: 262_144,
        max_rooms: 4096,
        max_lights: 256,
        max_walls: 16_384,
        max_occluders: 16_384,
        max_texture_dimension: 4096,
        max_texture_pixels: 4_194_304,
        max_total_bytes: 96 * 1024 * 1024,
        max_raster_samples: 50_000_000,
        ..Default::default()
    }
}
impl WorldLighting {
    /// Structural checks only: no shadow atlas or geometry is allocated here.
    /// Missing/cross-floor identifiers are errors even when no lamp would use them.
    pub fn validate(&self, world: &WorldDocument) -> Result<(), WorldError> {
        let b = budget();
        if world.schema_version != LIGHTING_WORLD_SCHEMA_VERSION
            || world.revision.lot_id != Some(self.lot_id)
            || self.lot_id == 0
            || self.epoch == 0
            || self.revision == 0
            || self.epoch != world.revision.epoch
            || self.source != world.revision.content
            || self.width != u32::from(world.lot.width)
            || self.height != u32::from(world.lot.height)
            || self.stories != world.lot.levels
            || self.width < 3
            || self.height < 3
            || !(1..=5).contains(&self.stories)
        {
            return Err(invalid("schema, source, boundary or dimensions"));
        }
        let area = (self.width as usize)
            .checked_mul(self.height as usize)
            .ok_or_else(|| invalid("cell overflow"))?;
        let count = area
            .checked_mul(self.stories as usize)
            .ok_or_else(|| invalid("cell overflow"))?;
        if count > b.max_cells
            || self.cells.len() != count
            || self.rooms.len() > b.max_rooms
            || self.geometry.len() > b.max_rooms
            || self.lights.len() > b.max_lights
        {
            return Err(invalid("input budget or shape"));
        }
        let layout =
            iso::LightAtlasLayout::new(self.width, self.height, false, false, b.max_texture_pixels)
                .map_err(|e| invalid(&e.to_string()))?;
        if layout
            .color_atlas
            .into_iter()
            .any(|n| n > b.max_texture_dimension)
        {
            return Err(invalid("atlas dimension budget"));
        }
        let mut rooms = BTreeMap::new();
        for room in &self.rooms {
            if room.id == 0
                || room.id > 32767
                || room.floor >= self.stories
                || rooms.insert(room.id, room.floor).is_some()
            {
                return Err(invalid("duplicate, missing or out-of-range room"));
            }
        }
        let has_room = |id: u16, floor: u8| rooms.get(&id) == Some(&floor);
        for (i, cell) in self.cells.iter().enumerate() {
            if cell.first > 32767
                || cell.second > 32767
                || (cell.diagonal == LightRoomDiagonal::None && cell.first != cell.second)
                || [cell.first, cell.second]
                    .into_iter()
                    .any(|id| id != 0 && !has_room(id, (i / area) as u8))
            {
                return Err(invalid("room-map reference or diagonal"));
            }
        }
        let mut groups = BTreeSet::new();
        let (mut walls, mut objects) = (0usize, 0usize);
        for geometry in &self.geometry {
            if !has_room(geometry.room, geometry.floor) || !groups.insert(geometry.room) {
                return Err(invalid("shadow room reference"));
            }
            walls = walls
                .checked_add(geometry.walls.len())
                .ok_or_else(|| invalid("shadow count"))?;
            objects = objects
                .checked_add(geometry.objects.len())
                .ok_or_else(|| invalid("shadow count"))?;
            if walls > b.max_walls || objects > b.max_occluders {
                return Err(invalid("shadow budget"));
            }
            for [a, b] in &geometry.walls {
                if !bounded_point(*a) || !bounded_point(*b) || a == b {
                    return Err(invalid("wall shadow geometry"));
                }
            }
            for &[x, y, w, h] in &geometry.objects {
                if [x, y, w, h]
                    .into_iter()
                    .any(|v| !v.is_finite() || v.fract() != 0. || v.abs() > 1_000_000.)
                    || w <= 0.
                    || h <= 0.
                    || !(x + w).is_finite()
                    || !(y + h).is_finite()
                {
                    return Err(invalid("object shadow geometry"));
                }
            }
        }
        let mut ids = BTreeSet::new();
        for l in &self.lights {
            if l.id == 0
                || !ids.insert(l.id)
                || !has_room(l.room, l.floor)
                || l.window_room.is_some_and(|id| !rooms.contains_key(&id))
                || !bounded_point(l.position_sixteenths)
                || !l.radius_sixteenths.is_finite()
                || !(1. ..=1_000_000.).contains(&l.radius_sixteenths)
                || !l.falloff_multiplier.is_finite()
                || !(0. ..=10_000.).contains(&l.falloff_multiplier)
                || !l.intensity.is_finite()
                || !(0. ..=1_000_000.).contains(&l.intensity)
                || !l.height.is_finite()
                || !(0.01..=1000.).contains(&l.height)
            {
                return Err(invalid("light reference or parameters"));
            }
        }
        Ok(())
    }
    pub fn prepare(&self, world: &WorldDocument) -> Result<PreparedWorldLighting, WorldError> {
        self.validate(world)?;
        let maps = iso::prepare_room_maps(
            &iso::RoomMapInput {
                source: self.source,
                lot_id: self.lot_id,
                epoch: self.epoch,
                revision: self.revision,
                width: self.width,
                height: self.height,
                stories: self.stories,
                cells: self
                    .cells
                    .iter()
                    .map(|c| iso::RoomMapCell {
                        first: c.first,
                        second: c.second,
                        diagonal: match c.diagonal {
                            LightRoomDiagonal::None => iso::RoomDiagonal::None,
                            LightRoomDiagonal::Horizontal => iso::RoomDiagonal::Horizontal,
                            LightRoomDiagonal::Vertical => iso::RoomDiagonal::Vertical,
                        },
                        floor_pattern: c.floor_pattern,
                    })
                    .collect(),
                rooms: self
                    .rooms
                    .iter()
                    .map(|r| iso::LightingRoom {
                        id: r.id,
                        floor: r.floor,
                        outside: r.outside,
                        outside_light: r.outside_light,
                        ambient_light: r.ambient_light,
                    })
                    .collect(),
                minimum: self.minimum,
                outside: self.outside,
            },
            budget(),
        )
        .map_err(|e| invalid(&e.to_string()))?;
        let geometry: Vec<_> = self
            .geometry
            .iter()
            .map(|g| iso::RoomShadowGeometry {
                room: g.room,
                floor: g.floor,
                walls: g.walls.clone(),
                objects: g
                    .objects
                    .iter()
                    .map(|&[x, y, width, height]| iso::Rect {
                        x,
                        y,
                        width,
                        height,
                    })
                    .collect(),
            })
            .collect();
        let lights: Vec<_> = self
            .lights
            .iter()
            .map(|l| iso::SceneLight {
                id: l.id,
                room: l.room,
                floor: l.floor,
                light: iso::ShadowLight {
                    kind: iso::ShadowLightKind::Point,
                    position_sixteenths: l.position_sixteenths,
                    direction: Vec2::new(1., 0.),
                    radius_sixteenths: l.radius_sixteenths,
                    falloff_multiplier: l.falloff_multiplier,
                },
                color: l.color,
                intensity: l.intensity,
                outdoors_color: l.outdoors_color,
                window_room: l.window_room,
                height: l.height,
            })
            .collect();
        let scene = iso::prepare_lighting_scene(
            &iso::LightingSceneInput {
                room_maps: &maps,
                geometry: &geometry,
                lights: &lights,
                outdoors: None,
                ultra: false,
                software_depth: false,
                directional: false,
            },
            budget(),
        )
        .map_err(|e| invalid(&e.to_string()))?;
        Ok(PreparedWorldLighting { scene })
    }
}

pub struct PreparedWorldLighting {
    scene: iso::PreparedLightingScene,
}
impl PreparedWorldLighting {
    pub fn image(&self) -> &RgbaImage {
        self.scene.color_atlas()
    }
    pub fn key(&self) -> wonderland_render_core::cache::DerivedKey {
        self.scene.key()
    }
    /// Same world-to-atlas coordinates as LightMap2D. The model transform is
    /// applied exactly once. Floor is zero-based; VM levels remain one-based.
    pub fn model_to_uv(&self, model: Mat4, floor: u8) -> Result<Mat4, WorldError> {
        let layout = self.scene.layout();
        let slot = layout
            .floor_slot(floor)
            .map_err(|e| invalid(&e.to_string()))?;
        let scale = layout.world_to_atlas();
        let mapping = Mat4 {
            cols: [
                [scale.x, 0., 0., 0.],
                [0., 0., 0., 0.],
                [0., scale.z, 0., 0.],
                [slot[0] as f32 / 3., slot[1] as f32 / 2., 0., 1.],
            ],
        } * model;
        if !mapping.is_finite() {
            return Err(invalid("light transform overflow"));
        }
        Ok(mapping)
    }
}
