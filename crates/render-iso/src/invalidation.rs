use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct IsoChanges {
    pub rotation: bool,
    pub discrete_zoom: bool,
    pub level: bool,
    pub scroll_cell: bool,
    pub precise_zoom: bool,
    pub walls: bool,
    pub wall_cut: bool,
    pub floors: bool,
    pub rooms: bool,
    pub lighting: bool,
    pub lighting_unimportant: bool,
    pub roof_style: bool,
    pub lot_or_content: bool,
    pub device: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WallRecache {
    #[default]
    None,
    CutOnly,
    Full,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct InvalidationPlan {
    pub sprites: bool,
    pub static_surface: bool,
    pub walls: WallRecache,
    pub floors: bool,
    pub room_maps: bool,
    pub lighting: bool,
    pub roofs: bool,
    pub immediate: bool,
    pub gpu_handles: bool,
}
/// Domain dependency plan consumed by render-core cache/device ownership. Every
/// typed flag is processed independently; source Dirty.ALL's omitted room/light
/// bits cannot discard standalone changes here.
pub fn plan_invalidation(c: IsoChanges) -> InvalidationPlan {
    let all = c.lot_or_content || c.device;
    let view = c.rotation || c.discrete_zoom || c.level;
    let walls = if all || view || c.walls {
        WallRecache::Full
    } else if c.wall_cut {
        WallRecache::CutOnly
    } else {
        WallRecache::None
    };
    InvalidationPlan {
        sprites: all || c.rotation || c.discrete_zoom,
        static_surface: all
            || view
            || c.scroll_cell
            || c.walls
            || c.wall_cut
            || c.floors
            || c.rooms
            || c.lighting
            || c.lighting_unimportant,
        walls,
        floors: all || view || c.floors,
        room_maps: all || c.rooms,
        lighting: all || c.rooms || c.lighting || c.lighting_unimportant,
        roofs: all || c.rooms || c.roof_style,
        immediate: c.precise_zoom,
        gpu_handles: all,
    }
}
pub fn time_light_changed(old: f64, next: f64) -> Result<bool> {
    if !old.is_finite() || !next.is_finite() {
        return Err(IsoError::Invalid("time of day"));
    }
    Ok((next - old).abs() > f64::from(0.001f32))
}
pub fn outside_light_changed(old: [u8; 4], next: [u8; 4]) -> bool {
    let delta = (0..3)
        .map(|i| u32::from(old[i].abs_diff(next[i])))
        .sum::<u32>();
    delta > 20 || (next == [255; 4] && old != [255; 4])
}
