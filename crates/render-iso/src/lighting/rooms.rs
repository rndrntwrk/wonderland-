//! World room-map uploads: GPURoomMaps, Blueprint.GenerateRoomLights/GetIndoors,
//! and LightMap2D.fx OutsideRoomCheck. Room topology/IDs come from the immutable
//! semantic provider; this module never flood-fills or changes VM rooms.
use crate::{IsoError, Result};
use std::collections::BTreeMap;
use wonderland_render_core::{cache::DerivedKey, AssetKey, RgbaImage, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightingBudget {
    pub max_cells: usize,
    pub max_rooms: usize,
    pub max_lights: usize,
    pub max_walls: usize,
    pub max_occluders: usize,
    pub max_vertices: usize,
    pub max_indices: usize,
    pub max_texture_dimension: u32,
    pub max_texture_pixels: usize,
    /// Peak owned CPU data, including staging and temporary render targets.
    pub max_total_bytes: usize,
    pub max_raster_samples: u64,
}
impl Default for LightingBudget {
    fn default() -> Self {
        Self {
            max_cells: 512 * 512 * 5,
            max_rooms: 32767,
            max_lights: 1024,
            max_walls: 65536,
            max_occluders: 65536,
            max_vertices: 1_000_000,
            max_indices: 3_000_000,
            max_texture_dimension: 8192,
            max_texture_pixels: 16_777_216,
            max_total_bytes: 256 * 1024 * 1024,
            max_raster_samples: 250_000_000,
        }
    }
}
impl LightingBudget {
    pub(super) fn bytes(self, count: usize) -> Result<()> {
        if count > self.max_total_bytes {
            Err(IsoError::Limit("lighting total bytes"))
        } else {
            Ok(())
        }
    }
    pub(super) fn texture(self, width: u32, height: u32) -> Result<usize> {
        if width == 0 || height == 0 {
            return Err(IsoError::Invalid("lighting texture size"));
        }
        let count = (width as usize)
            .checked_mul(height as usize)
            .ok_or(IsoError::Limit("lighting texture pixels"))?;
        if width > self.max_texture_dimension
            || height > self.max_texture_dimension
            || count > self.max_texture_pixels
        {
            return Err(IsoError::Limit("lighting texture size"));
        }
        Ok(count)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomDiagonal {
    None,
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoomMapCell {
    pub first: u16,
    pub second: u16,
    pub diagonal: RoomDiagonal,
    /// Source floor pattern: nonzero surfaces affect the weather indoors map.
    pub floor_pattern: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightingRoom {
    pub id: u16,
    pub floor: u8,
    pub outside: bool,
    pub outside_light: u16,
    pub ambient_light: u16,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomMapInput {
    pub source: AssetKey,
    pub lot_id: u64,
    pub epoch: u64,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    /// The source GPURoomMaps exposes five floor textures, not six.
    pub stories: u8,
    /// Row-major cells, floor-major. IDs must already resolve room/base aliases.
    pub cells: Vec<RoomMapCell>,
    pub rooms: Vec<LightingRoom>,
    pub minimum: [u8; 4],
    pub outside: [u8; 4],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedRoomMaps {
    key: DerivedKey,
    pub(super) source: AssetKey,
    pub(super) lot_id: u64,
    pub(super) epoch: u64,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) maps: Vec<RgbaImage>,
    pub(super) rooms: Vec<LightingRoom>,
    colors: RgbaImage,
    indoors: Vec<u8>,
    pub(super) minimum: [u8; 4],
    pub(super) outside: [u8; 4],
    pub(super) resident_bytes: usize,
}
impl PreparedRoomMaps {
    pub fn key(&self) -> DerivedKey {
        self.key
    }
    pub fn source(&self) -> AssetKey {
        self.source
    }
    pub fn lot_id(&self) -> u64 {
        self.lot_id
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn maps(&self) -> &[RgbaImage] {
        &self.maps
    }
    pub fn rooms(&self) -> &[LightingRoom] {
        &self.rooms
    }
    pub fn basic_colors(&self) -> &RgbaImage {
        &self.colors
    }
    pub fn indoors(&self) -> &[u8] {
        &self.indoors
    }
    pub fn resident_bytes(&self) -> usize {
        self.resident_bytes
    }
    pub fn room_at(&self, floor: u8, tile: Vec2) -> Result<u16> {
        let map = self
            .maps
            .get(floor as usize)
            .ok_or(IsoError::Invalid("room map floor"))?;
        if !tile.is_finite()
            || tile.x < 0.
            || tile.y < 0.
            || tile.x >= self.width as f32
            || tile.y >= self.height as f32
        {
            return Err(IsoError::Invalid("room map sample"));
        }
        let rgba = map.pixels[tile.x as usize + tile.y as usize * self.width as usize];
        let first = u16::from_le_bytes([rgba[0], rgba[1]]);
        let packed_second = u16::from_le_bytes([rgba[2], rgba[3]]);
        let second = packed_second & 0x7fff;
        let x = tile.x.fract();
        let y = tile.y.fract();
        Ok(if first == second {
            first
        } else if packed_second & 0x8000 != 0 {
            if x + y >= 1. {
                second
            } else {
                first
            }
        } else if x - y > 0. {
            first
        } else {
            second
        })
    }
}

pub fn prepare_room_maps(input: &RoomMapInput, budget: LightingBudget) -> Result<PreparedRoomMaps> {
    if input.lot_id == 0
        || input.epoch == 0
        || input.width < 3
        || input.height < 3
        || !(1..=5).contains(&input.stories)
    {
        return Err(IsoError::Invalid("room map identity/dimensions"));
    }
    let pixels = budget.texture(input.width, input.height)?;
    let cells = pixels
        .checked_mul(input.stories as usize)
        .ok_or(IsoError::Limit("room map cells"))?;
    if cells > budget.max_cells || input.rooms.len() > budget.max_rooms {
        return Err(IsoError::Limit("room map entries"));
    }
    if input.cells.len() != cells {
        return Err(IsoError::Invalid("room map cell count"));
    }
    budget.texture(256, 256)?;
    // Includes canonical-key staging, room lookup allocation and all output.
    let peak = cells
        .checked_mul(12)
        .and_then(|v| v.checked_add(262_144 + 128 + pixels))
        .and_then(|v| v.checked_add(input.stories as usize * std::mem::size_of::<RgbaImage>()))
        .and_then(|v| v.checked_add(input.rooms.len().checked_mul(128)?))
        .ok_or(IsoError::Limit("room map bytes"))?;
    budget.bytes(peak)?;
    let mut lookup = BTreeMap::new();
    for room in &input.rooms {
        if room.id == 0
            || room.id > 32767
            || room.floor >= input.stories
            || lookup.insert(room.id, *room).is_some()
        {
            return Err(IsoError::Invalid("lighting room identity"));
        }
    }
    for (i, cell) in input.cells.iter().enumerate() {
        if cell.first > 32767
            || cell.second > 32767
            || (cell.diagonal == RoomDiagonal::None && cell.first != cell.second)
        {
            return Err(IsoError::Invalid("packed room IDs"));
        }
        for id in [cell.first, cell.second] {
            if id != 0
                && lookup
                    .get(&id)
                    .map_or(true, |room| room.floor as usize != i / pixels)
            {
                return Err(IsoError::Invalid("unknown or cross-floor room"));
            }
        }
    }
    // Exact capacity prevents Vec's geometric growth from exceeding the peak
    // bound while serializing a large multi-floor room map.
    let parameter_capacity = cells
        .checked_mul(7)
        .and_then(|n| n.checked_add(input.rooms.len().checked_mul(8)?))
        .and_then(|n| n.checked_add(b"source-room-map-v1\0".len() + 3 * 8 + 2 * 4 + 1 + 4 + 4 + 8))
        .ok_or(IsoError::Limit("room map key bytes"))?;
    let mut parameters = Vec::with_capacity(parameter_capacity);
    parameters.extend_from_slice(b"source-room-map-v1\0");
    for value in [input.lot_id, input.epoch, input.revision] {
        parameters.extend_from_slice(&value.to_le_bytes());
    }
    parameters.extend_from_slice(&input.width.to_le_bytes());
    parameters.extend_from_slice(&input.height.to_le_bytes());
    parameters.push(input.stories);
    parameters.extend_from_slice(&input.minimum);
    parameters.extend_from_slice(&input.outside);
    parameters.extend_from_slice(&(input.rooms.len() as u64).to_le_bytes());
    let mut colors = vec![[0; 4]; 65536];
    for room in lookup.values() {
        parameters.extend_from_slice(&room.id.to_le_bytes());
        parameters.extend_from_slice(&[room.floor, u8::from(room.outside)]);
        parameters.extend_from_slice(&room.outside_light.to_le_bytes());
        parameters.extend_from_slice(&room.ambient_light.to_le_bytes());
        let mut color = [255; 4];
        let ambient = (255. * (f32::from(room.ambient_light) / 100.)).clamp(0., 255.) as u8;
        for (c, out) in color[..3].iter_mut().enumerate() {
            let outside = (f32::from(input.outside[c]) * (f32::from(room.outside_light) / 100.))
                .clamp(0., 255.) as u8;
            *out = outside.max(input.minimum[c]).saturating_add(ambient);
        }
        colors[room.id as usize] = color;
    }
    colors[65535] = [255; 4];
    let mut maps = Vec::with_capacity(input.stories as usize);
    let mut indoors = vec![0; pixels];
    let partition = 255 / (u16::from(input.stories) + 1);
    for (floor, data) in input.cells.chunks(pixels).enumerate() {
        let mut map = Vec::with_capacity(pixels);
        for (i, cell) in data.iter().enumerate() {
            let second = cell.second
                | if cell.diagonal == RoomDiagonal::Horizontal {
                    0x8000
                } else {
                    0
                };
            let packed = u32::from(cell.first) | (u32::from(second) << 16);
            let rgba = packed.to_le_bytes();
            map.push(rgba);
            parameters.extend_from_slice(&rgba);
            parameters.extend_from_slice(&cell.floor_pattern.to_le_bytes());
            parameters.push(match cell.diagonal {
                RoomDiagonal::None => 0,
                RoomDiagonal::Horizontal => 1,
                RoomDiagonal::Vertical => 2,
            });
            if cell.floor_pattern > 0 {
                indoors[i] = (partition * floor as u16) as u8;
            }
            if cell.first != 0
                && [cell.first, cell.second]
                    .iter()
                    .any(|id| lookup.get(id).is_some_and(|room| !room.outside))
            {
                // Unlike the old GetIndoors indexing bug, the diagonal bit is not a room ID.
                indoors[i] = (partition * (floor as u16 + 1)) as u8;
            }
        }
        maps.push(RgbaImage {
            width: input.width,
            height: input.height,
            pixels: map,
        });
    }
    debug_assert_eq!(parameters.len(), parameter_capacity);
    debug_assert_eq!(parameters.capacity(), parameter_capacity);
    let key = DerivedKey::new(input.source, input.source, 1, &parameters);
    let resident_bytes = cells * 4
        + 262_144
        + pixels
        + input.rooms.len() * std::mem::size_of::<LightingRoom>()
        + maps.capacity() * std::mem::size_of::<RgbaImage>();
    Ok(PreparedRoomMaps {
        key,
        source: input.source,
        lot_id: input.lot_id,
        epoch: input.epoch,
        width: input.width,
        height: input.height,
        maps,
        rooms: lookup.into_values().collect(),
        colors: RgbaImage {
            width: 256,
            height: 256,
            pixels: colors,
        },
        indoors,
        minimum: input.minimum,
        outside: input.outside,
        resident_bytes,
    })
}
