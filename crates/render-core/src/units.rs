//! FreeSO visual units. Tile positions include terrain/container elevation already.
use crate::math::Vec3;
pub const GRAPHICS_UNITS_PER_TILE: f32 = 3.;
pub const STORY_HEIGHT_TILES: f32 = 2.95;
pub const TERRAIN_HEIGHT_FACTOR_TILES: f32 = 3. / 160.;
pub const CONTENT_HORIZONTAL_DIVISOR: f32 = 16.;
pub const CONTENT_VERTICAL_DIVISOR: f32 = 5.;
pub fn tile_to_graphics(tile: Vec3) -> Vec3 {
    Vec3::new(tile.x * 3., tile.z * 3., tile.y * 3.)
}
pub fn graphics_to_tile(world: Vec3) -> Vec3 {
    Vec3::new(world.x / 3., world.z / 3., world.y / 3.)
}
pub fn terrain_height_to_tiles(height: f32) -> f32 {
    height * TERRAIN_HEIGHT_FACTOR_TILES
}
pub fn story_height_to_tiles(stories: f32) -> f32 {
    stories * STORY_HEIGHT_TILES
}
pub fn dgrp_offset_to_tiles(offset: Vec3) -> Vec3 {
    Vec3::new(offset.x / 16., offset.y / 16., offset.z / 5.)
}
pub fn slot_offset_to_tiles(offset: Vec3) -> Vec3 {
    dgrp_offset_to_tiles(offset)
}
pub fn dgrp_offset_to_graphics(offset: Vec3) -> Vec3 {
    tile_to_graphics(dgrp_offset_to_tiles(offset))
}
pub fn slot_offset_to_graphics(offset: Vec3) -> Vec3 {
    tile_to_graphics(slot_offset_to_tiles(offset))
}
