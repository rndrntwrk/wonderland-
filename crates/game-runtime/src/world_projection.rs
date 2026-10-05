//! Simulation geometry and live identities, with source appearance supplied by
//! the rendering/content owner. A missing model remains a missing model.
use crate::{GameRuntime, GameRuntimeError};
use sim_core::world::{Cardinal, Diagonal, TilePos};
use wonderland_render_core::{AssetKey, EntityRef, Vec3};
use wonderland_world_view::{
    TerrainBoundary, WorldDiagnostic, WorldDiagonal, WorldDocument, WorldObject, WorldRevision,
    WorldSourceKind, WorldTile, source_terrain,
};

impl GameRuntime {
    pub fn world_document(
        &self,
        appearance: &WorldDocument,
    ) -> Result<WorldDocument, GameRuntimeError> {
        appearance
            .validate()
            .map_err(|e| GameRuntimeError::Content(e.to_string()))?;
        let state = self.sim.state();
        let lot = &state.world.lot;
        if appearance.lot.width != lot.width() || appearance.lot.height != lot.height() {
            return Err(GameRuntimeError::Content(
                "source appearance and simulation lot dimensions differ".into(),
            ));
        }
        let mut document = appearance.clone();
        document.provenance.kind = WorldSourceKind::LiveSession;
        document.revision = WorldRevision {
            lot_id: Some(state.lot_id),
            epoch: state.authority_epoch,
            tick: state.completed_tick,
            architecture_revision: lot.revision().architecture,
            content: AssetKey(state.content.content_hash),
        };
        document.source_counts = None;
        document.lot.levels = lot.levels();
        let mut tiles = Vec::new();
        let mut missing_wall_appearance = 0;
        for level in 1..=lot.levels() {
            for y in 0..lot.height() {
                for x in 0..lot.width() {
                    let position = TilePos::new(x as i16, y as i16, level);
                    let source = lot.tile(position).expect("bounded source tile");
                    let mut target = appearance
                        .lot
                        .tiles
                        .get(tiles.len())
                        .cloned()
                        .unwrap_or_else(WorldTile::default);
                    target.floor = source.floor;
                    target.half_floors = source.wall.half_floors;
                    target.diagonal = match source.wall.diagonal {
                        Diagonal::None => None,
                        Diagonal::Vertical => Some(WorldDiagonal::Vertical),
                        Diagonal::Horizontal => Some(WorldDiagonal::Horizontal),
                    };
                    target.supported = Some(source.supported);
                    target.indoors = lot
                        .room_at(position.center())
                        .and_then(|id| lot.rooms().rooms.get(&id))
                        .map(|room| !room.outside);
                    target.wall.west = source.wall.sides & Cardinal::West.wall_bit() != 0;
                    target.wall.north = source.wall.sides & Cardinal::North.wall_bit() != 0;
                    target.wall.south = source.wall.sides & Cardinal::South.wall_bit() != 0;
                    target.wall.east = source.wall.sides & Cardinal::East.wall_bit() != 0;
                    if (source.wall.sides != 0 || source.wall.diagonal != Diagonal::None)
                        && target.wall.styles == [0; 2]
                    {
                        missing_wall_appearance += 1;
                    }
                    tiles.push(target);
                }
            }
        }
        document.lot.tiles = tiles;
        if document
            .lot
            .cutaway
            .as_ref()
            .is_some_and(|map| map.len() != document.lot.tiles.len())
        {
            document.lot.cutaway = None;
        }
        let mut heights = Vec::new();
        let mut grass = Vec::new();
        for y in 0..lot.height() {
            for x in 0..lot.width() {
                heights.push(lot.terrain_vertex(x, y).expect("bounded terrain vertex"));
                grass.push(lot.grass(x as i16, y as i16).expect("bounded grass cell"));
            }
        }
        // Source center altitude uses wrapped width*height neighbors. A also
        // supplies the explicit right/bottom corners consumed by its geometry.
        let mut terrain = source_terrain(
            lot.width(),
            lot.height(),
            &heights,
            appearance.lot.terrain.base_alt,
        )
        .map_err(|e| GameRuntimeError::Content(e.to_string()))?;
        terrain.boundary = TerrainBoundary::ExplicitCorners;
        terrain.corners.clear();
        for y in 0..=lot.height() {
            for x in 0..=lot.width() {
                terrain
                    .corners
                    .push(lot.terrain_vertex(x, y).expect("bounded terrain corner"));
            }
        }
        terrain.grass = grass;
        terrain.light = appearance.lot.terrain.light;
        terrain.dark = appearance.lot.terrain.dark;
        document.lot.terrain = terrain;
        document.objects.clear();
        document
            .diagnostics
            .retain(|d| !d.code.starts_with("runtime_"));
        if missing_wall_appearance != 0 {
            document.diagnostics.push(WorldDiagnostic {
            code: "runtime_wall_appearance_unavailable".into(), resource: document.provenance.origin.clone(),
            message: format!("{missing_wall_appearance} simulation wall tiles require source wall style/material bindings"),
        });
        }
        for entity in state.entities.values() {
            let info = &entity.info;
            let reference = EntityRef {
                object_id: info.reference.object_id.0 as u32,
                generation: info.reference.generation,
            };
            let placed = state.world.object(info.reference).ok_or_else(|| {
                GameRuntimeError::Content("live entity has no world projection".into())
            })?;
            let in_world = !placed.position.is_out_of_world();
            let visible = in_world && !info.dead && entity.object_data[34] != 1;
            let model = if info.is_avatar {
                None
            } else {
                appearance
                    .objects
                    .iter()
                    .find(|o| o.entity == Some(reference) && o.source_guid == info.guid)
                    .and_then(|o| o.model)
                    .or_else(|| {
                        let mut values = appearance
                            .objects
                            .iter()
                            .filter(|o| o.source_guid == info.guid)
                            .map(|o| o.model);
                        let first = values.next().flatten()?;
                        values.all(|value| value == Some(first)).then_some(first)
                    })
            };
            if model.is_none() {
                document.diagnostics.push(WorldDiagnostic {
                    code: "runtime_model_unavailable".into(),
                    resource: format!("guid:{:08x}", info.guid),
                    message: if info.is_avatar {
                        "Live avatar requires its source Vitaboy appearance provider".into()
                    } else {
                        "Live object has no unambiguous normalized source model binding".into()
                    },
                });
            }
            let mut dynamic_flags = [0u64; 2];
            for (index, enabled) in entity.dynamic_sprite_flags.iter().take(128).enumerate() {
                if *enabled {
                    dynamic_flags[index / 64] |= 1u64 << (index % 64);
                }
            }
            let offset = if info.is_avatar { 0. } else { 0.5 };
            let position_tiles = if in_world {
                Vec3::new(
                    placed.position.x as f32 / 16. - offset,
                    placed.position.y as f32 / 16. - offset,
                    f32::from(placed.position.level - 1) * 2.95,
                )
            } else {
                Vec3::ZERO
            };
            let room = lot
                .room_at(placed.position)
                .map(|id| u16::try_from(id.0))
                .transpose()
                .map_err(|_| {
                    GameRuntimeError::Content(
                        "room identity exceeds source presentation width".into(),
                    )
                })?
                .unwrap_or(0);
            document.objects.push(WorldObject {
                source_guid: info.guid,
                blueprint: None,
                snapshot: None,
                entity: Some(reference),
                visual_revision: entity.revision,
                position_tiles,
                yaw_radians: f32::from(info.direction) * std::f32::consts::FRAC_PI_4,
                dynamic_flags,
                room,
                level: if in_world { placed.position.level } else { 0 },
                visible,
                selectable: visible,
                model,
            });
        }
        document
            .validate()
            .map_err(|e| GameRuntimeError::Content(e.to_string()))?;
        Ok(document)
    }
}
