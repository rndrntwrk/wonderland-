//! Renderer-to-source terrain selection. This never predicts or applies movement.
use wonderland_game_runtime::LotPosition;
use wonderland_world_view::{
    WorldDocument, WorldPick, WorldPickTarget, WorldSourceKind, WorldSurface,
};

/// Revalidate a renderer-admitted tile against the latest native scene. The
/// renderer owns frame-generation checks; source checks own route availability.
pub fn native_pick_terrain(world: &WorldDocument, pick: &WorldPick) -> Option<LotPosition> {
    if world.provenance.kind != WorldSourceKind::LiveSession
        || world.revision.lot_id.is_none()
        || world.revision.lot_id != pick.revision.lot_id
        || world.revision.epoch != pick.revision.epoch
        || world.revision.content != pick.revision.content
        || world.revision.architecture_revision != pick.revision.architecture_revision
        || pick.revision.tick > world.revision.tick
    {
        return None;
    }
    let WorldPickTarget::Tile {
        x,
        y,
        level,
        surface,
    } = pick.target
    else {
        return None;
    };
    if x >= world.lot.width
        || y >= world.lot.height
        || level == 0
        || level > world.lot.levels
        || !matches!(
            surface,
            WorldSurface::Terrain | WorldSurface::Floor | WorldSurface::Pool | WorldSurface::Water
        )
    {
        return None;
    }
    Some(LotPosition {
        x: i32::from(i16::try_from(u32::from(x).checked_mul(16)?.checked_add(8)?).ok()?),
        y: i32::from(i16::try_from(u32::from(y).checked_mul(16)?.checked_add(8)?).ok()?),
        level,
    })
}
