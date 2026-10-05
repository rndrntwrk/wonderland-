//! Exact active-client city channel semantics, separate legacy bounds policies.
use crate::Error;
use wonderland_render_core::{Mat4, Mesh, Vec2, Vec3};
pub mod facade;
mod geometry;
pub mod transition;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TerrainClass {
    Grass = 0,
    Sand = 1,
    Rock = 2,
    Snow = 3,
    Water = 4,
    Void = 255,
}
pub fn terrain_class(rgba: [u8; 4]) -> TerrainClass {
    match rgba {
        [0, 255, 0, 255] => TerrainClass::Grass,
        [255, 255, 0, 255] => TerrainClass::Sand,
        [255, 0, 0, 255] => TerrainClass::Rock,
        [255, 255, 255, 255] => TerrainClass::Snow,
        [12, 0, 255, 255] => TerrainClass::Water,
        _ => TerrainClass::Void,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForestClass {
    None,
    Fir,
    Birch,
    Cactus,
    Palm,
    Unknown,
}
pub fn forest_class(rgba: [u8; 4]) -> ForestClass {
    match rgba {
        [0, 106, 40, 255] => ForestClass::Fir,
        [0, 235, 66, 255] => ForestClass::Birch,
        [255, 0, 0, 255] => ForestClass::Cactus,
        [255, 252, 0, 255] => ForestClass::Palm,
        [0, 0, 0, 255] => ForestClass::None,
        _ => ForestClass::Unknown,
    }
}
impl ForestClass {
    pub fn atlas_2d(self) -> Option<u8> {
        match self {
            Self::Fir => Some(0),
            Self::Birch => Some(1),
            Self::Cactus => Some(2),
            Self::Palm => Some(3),
            _ => None,
        }
    }
    pub fn atlas_3d(self) -> Option<u8> {
        match self {
            Self::Palm => Some(2),
            Self::Cactus => Some(3),
            _ => self.atlas_2d(),
        }
    }
}
pub fn tree_count(density: u8) -> usize {
    match u16::from(density) * 4 / 255 {
        0 => 0,
        1 => 1,
        2 => 4,
        3 => 7,
        _ => 15,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CityBoundary {
    Rectangle,
    RendererDiamond,
    /// The active renderer's ten-tile side fade, clipped to the image rectangle.
    RendererWithFade,
    ServerDiamond { padding: u16 },
    LegacyMapData,
}
pub fn in_bounds(x: i32, y: i32, policy: CityBoundary) -> bool {
    match policy {
        CityBoundary::Rectangle => (0..512).contains(&x) && (0..512).contains(&y),
        CityBoundary::RendererWithFade => {
            if !(0..512).contains(&x) || !(0..512).contains(&y) {
                return false;
            }
            let start = (y - 306).abs();
            let end = if y < 205 { 307 + y } else { 717 - y };
            x >= start - 10 && x < end + 10
        }
        CityBoundary::LegacyMapData => x > 0 && x < 512 && (0..512).contains(&y),
        CityBoundary::RendererDiamond | CityBoundary::ServerDiamond { .. } => {
            let pad = if let CityBoundary::ServerDiamond { padding } = policy {
                i32::from(padding)
            } else {
                0
            };
            if y < pad || y > 511 - pad {
                return false;
            }
            let start = (y - 306).abs();
            let end = if y < 205 { 307 + y } else { 717 - y };
            x >= start + pad
                && if matches!(policy, CityBoundary::RendererDiamond) {
                    x < end
                } else {
                    x <= end - pad
                }
        }
    }
}
pub fn pack_location(x: u16, y: u16) -> u32 {
    (u32::from(x) << 16) | u32::from(y)
}
pub fn unpack_location(packed: u32) -> (u16, u16) {
    ((packed >> 16) as u16, packed as u16)
}
pub const FLAG_LAYOUT: [u8; 16] = [11, 7, 15, 2, 9, 6, 0, 4, 1, 16, 20, 12, 14, 18, 10, 8];
pub const ROAD_LAYOUT: [i8; 16] = [-1, 5, 12, 13, 7, 6, 15, 14, 28, 29, 20, 21, 31, 30, 23, 22];
pub const ROAD_CORNER_LAYOUT: [i8; 16] =
    [-1, 8, 2, 26, 3, 17, 16, 10, 25, 24, 9, 18, 1, 27, 11, 19];
pub fn road_atlas(road: u8) -> (Option<u8>, Option<u8>) {
    let convert = |x: i8| if x < 0 { None } else { Some(x as u8) };
    (
        convert(ROAD_LAYOUT[(road & 15) as usize]),
        convert(ROAD_CORNER_LAYOUT[(road >> 4) as usize]),
    )
}
pub fn blend_atlas(absence_mask: u8) -> u8 {
    FLAG_LAYOUT[(absence_mask & 15) as usize]
}
pub fn cubic(v: [f32; 4], mu: f32, continuity: f32) -> f32 {
    let tension = 0.5 + continuity / 2.;
    let mu2 = mu * mu;
    let mu3 = mu2 * mu;
    let m0 = ((v[1] - v[0]) * (1. + continuity) + (v[2] - v[1]) * (1. - continuity))
        * (1. - tension)
        / 2.;
    let m1 = ((v[2] - v[1]) * (1. - continuity) + (v[3] - v[2]) * (1. + continuity))
        * (1. - tension)
        / 2.;
    (2. * mu3 - 3. * mu2 + 1.) * v[1]
        + (mu3 - 2. * mu2 + mu) * m0
        + (mu3 - mu2) * m1
        + (-2. * mu3 + 3. * mu2) * v[2]
}
pub fn nearest_neighborhood(x: f32, y: f32, origins: &[(u64, f32, f32)]) -> Option<u64> {
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    let mut result = None;
    let mut best = f32::INFINITY;
    for &(id, ox, oy) in origins {
        let dx = x + 0.5 - ox;
        let dy = y + 0.5 - oy;
        let d = dx * dx + dy * dy;
        if d < best {
            best = d;
            result = Some(id);
        }
    }
    result
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CityPixel {
    pub terrain: [u8; 4],
    pub elevation: u8,
    pub forest: [u8; 4],
    pub density: u8,
    pub road: u8,
}
#[derive(Clone, Debug)]
pub struct CityMap {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<CityPixel>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CityPartKind {
    Terrain(TerrainClass),
    Blend { class: TerrainClass, mask: u8 },
    RoadEdge(u8),
    RoadCorner(u8),
}
#[derive(Clone, Debug)]
pub struct CityPart {
    pub tile: (u16, u16),
    pub chunk: (u16, u16),
    pub kind: CityPartKind,
    pub mesh: Mesh,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoliageInstance {
    pub model: u8,
    pub position: Vec3,
    pub yaw: f32,
    pub scale: f32,
}
impl CityPart {
    /// Secondary blend-mask coordinates in vertex order. Terrain UVs stay in
    /// Mesh::vertices.uv; this channel works for both coarse and detailed meshes.
    pub fn mask_uv(&self) -> Option<Vec<Vec2>> {
        let CityPartKind::Blend { mask, .. } = self.kind else {
            return None;
        };
        let index = blend_atlas(mask);
        let offset = Vec2::new(f32::from(index % 7) / 7., f32::from(index / 7) / 3.);
        Some(self.mesh.vertices.iter().map(|v| {
            offset + Vec2::new(
                (v.position.x - f32::from(self.tile.0)) / 7.,
                (v.position.z - f32::from(self.tile.1)) / 3.,
            )
        }).collect())
    }
}
pub use geometry::{
    build_city_mesh, build_city_parts, build_near_patch, build_near_patch_parts,
    foliage_instances,
};
pub fn lot_center_to_city(city: (u16, u16), lot: Vec2) -> Result<Vec2, Error> {
    if !lot.is_finite() {
        return Err(Error::InvalidInput("lot camera center"));
    }
    let result = Vec2::new(
        f32::from(city.0) + 1. - (lot.y - 2.) / 72.,
        f32::from(city.1) + (lot.x - 2.) / 72.,
    );
    if !result.is_finite() {
        return Err(Error::InvalidInput("lot-to-city coordinate overflow"));
    }
    Ok(result)
}
pub fn city_center_to_lot(city: (u16, u16), position: Vec2) -> Result<Vec2, Error> {
    if !position.is_finite() {
        return Err(Error::InvalidInput("city camera center"));
    }
    let result = Vec2::new(
        (position.y - f32::from(city.1)) * 72. + 2.,
        (f32::from(city.0) + 1. - position.x) * 72. + 2.,
    );
    if !result.is_finite() {
        return Err(Error::InvalidInput("city-to-lot coordinate overflow"));
    }
    Ok(result)
}
/// Source tile-unit transform. Graphics-unit geometry must first be divided by 3.
pub fn facade_transform(
    city: (u16, u16),
    corner_elevation: [u8; 4],
    y_squish: f32,
) -> Result<Mat4, Error> {
    if !y_squish.is_finite() || y_squish <= 0. {
        return Err(Error::InvalidInput("facade Y squish"));
    }
    let h = corner_elevation.iter().map(|v| f32::from(*v)).sum::<f32>() / 48.;
    let rotation =
        wonderland_render_core::Quat::from_axis_angle(Vec3::Y, -std::f32::consts::FRAC_PI_2)
            .ok_or(Error::InvalidInput("facade rotation"))?;
    Ok(
        Mat4::from_translation(Vec3::new(f32::from(city.0) + 1., h, f32::from(city.1)))
            * Mat4::from_quat(rotation)
            * Mat4::from_scale(Vec3::new(1. / 77., y_squish / 77., 1. / 77.)),
    )
}
/// Original lossless class-map fixture, not a content-provider capture.
pub fn synthetic_city() -> CityMap {
    let colors = [
        [0, 255, 0, 255],
        [255, 255, 0, 255],
        [255, 0, 0, 255],
        [255, 255, 255, 255],
        [12, 0, 255, 255],
    ];
    let mut pixels = Vec::new();
    for y in 0..6 {
        for x in 0..6 {
            pixels.push(CityPixel {
                terrain: colors[(x + y) % 5],
                elevation: (x * 6 + y * 3) as u8,
                forest: if x == 1 {
                    [0, 106, 40, 255]
                } else {
                    [0, 0, 0, 255]
                },
                density: if x == 1 { 128 } else { 0 },
                road: if y == 3 { 5 } else { 0 },
            });
        }
    }
    CityMap {
        width: 6,
        height: 6,
        pixels,
    }
}
