//! Source FSOv refresh snapshots become disposable world presentation only.
//!
//! `VMTSOLotState.LotID` is the packed city location (LotContainer.cs), not a
//! directory database ID. Reusable short ObjectIDs have no incarnation in the
//! snapshot. Neither field becomes an admitted renderer/game identity here.
use sha2::{Digest, Sha256};
use wonderland_render_core::{AssetKey, Vec3};
use wonderland_vm_protocol::{
    Snapshot,
    snapshot::{Appearance, Architecture, Entity},
};
use wonderland_world_view::*;

fn invalid(message: &str) -> WorldError {
    WorldError(format!("source snapshot world: {message}"))
}
fn diagnostic(
    code: &str,
    resource: impl Into<String>,
    message: impl Into<String>,
) -> WorldDiagnostic {
    WorldDiagnostic {
        code: code.into(),
        resource: resource.into(),
        message: message.into(),
    }
}

/// Convert an already decoded FSOv38 refresh. Epoch and generation are the
/// receiver's presentation fences; they do not certify source VM continuity.
pub fn snapshot_world(
    snapshot: &Snapshot,
    epoch: u64,
    tick: u64,
    presentation_generation: u64,
) -> Result<WorldDocument, WorldError> {
    if snapshot.version != 38 {
        return Err(invalid("unsupported FSOv version"));
    }
    if presentation_generation == 0 {
        return Err(invalid("unversioned presentation"));
    }
    let architecture = &snapshot.context.architecture;
    let (width, height, levels) = (
        architecture.width,
        architecture.height,
        architecture.stories,
    );
    // source_terrain checks dimensions before allocating and implements the
    // TerrainComponent raw-zero edge separately from VM terrain-center wrap.
    // BaseAlt is not serialized; VM's ordinary snapshot load starts it at zero.
    let mut terrain = source_terrain(width, height, &architecture.heights, 0)?;
    let area = usize::from(width) * usize::from(height);
    if levels == 0
        || levels > 16
        || area * usize::from(levels) > 262_144
        || architecture.grass.len() != area
        || architecture.walls.len() != usize::from(levels)
        || architecture.floors.len() != usize::from(levels)
        || architecture.walls.iter().any(|level| level.len() != area)
        || architecture.floors.iter().any(|level| level.len() != area)
        || architecture
            .fine_buildable
            .as_ref()
            .is_some_and(|map| map.len() != area)
        || snapshot.entities.len() > 65_535
    {
        return Err(invalid(
            "array lengths or resource counts do not match source dimensions",
        ));
    }
    let (light, dark) = terrain_colors(architecture.terrain_light, architecture.terrain_dark)?;
    terrain.light = light;
    terrain.dark = dark;
    terrain.grass.clone_from(&architecture.grass);
    let mut tiles = Vec::with_capacity(area * usize::from(levels));
    let mut counts = SourceCounts {
        objects: snapshot.entities.len(),
        ..Default::default()
    };
    for (walls, floors) in architecture.walls.iter().zip(&architecture.floors) {
        for (wall, &floor) in walls.iter().zip(floors) {
            if wall.segments & !63 != 0 || wall.segments & 48 == 48 {
                return Err(invalid("invalid wall segment flags"));
            }
            let diagonal = if wall.segments & 32 != 0 {
                Some(WorldDiagonal::Vertical)
            } else if wall.segments & 16 != 0 {
                Some(WorldDiagonal::Horizontal)
            } else {
                None
            };
            tiles.push(WorldTile {
                floor,
                half_floors: diagonal.map(|diagonal| match diagonal {
                    WorldDiagonal::Vertical => [wall.patterns[0], wall.styles[0]],
                    WorldDiagonal::Horizontal => [wall.styles[0], wall.patterns[0]],
                }),
                diagonal,
                // FSOv has no room/support graph. A floor is not evidence of a
                // room, nor are fine-buildable permission bits support data.
                indoors: None,
                supported: None,
                wall: WorldWall {
                    patterns: wall.patterns,
                    styles: wall.styles,
                    object_styles: [0; 2],
                    west: wall.segments & 1 != 0,
                    north: wall.segments & 2 != 0,
                    east: wall.segments & 4 != 0,
                    south: wall.segments & 8 != 0,
                },
            });
            counts.floors += usize::from(floor != 0);
            counts.walls += usize::from(wall.segments != 0);
            counts.pools += usize::from(floor == 65_535);
        }
    }
    let mut diagnostics = vec![
        diagnostic(
            "snapshot_presentation_revision",
            "snapshot:revision",
            "This is an original server snapshot. Its display generation expires picks; the snapshot does not supply authoritative object generations or architecture revisions.",
        ),
        diagnostic(
            "unresolved_room_support",
            "architecture:rooms",
            "Source walls, floors and roof settings are retained. Room, support and cutaway maps are absent from this snapshot; roof and cutaway decisions await those source maps.",
        ),
    ];
    if architecture.id_map.is_some() {
        diagnostics.push(diagnostic("unresolved_architecture_id_map", "architecture:content-map",
            "The snapshot includes an original content name map. Matching wall, floor and roof resources have not been resolved; their source numeric IDs are retained."));
    }
    if snapshot
        .multitile_groups
        .iter()
        .any(|group| group.objects.len() > 1)
    {
        diagnostics.push(diagnostic("snapshot_multitile_contact", "objects:multitile",
            "Multi-part object positions are preserved. Group ground contact adjustments need the original object definitions and placement projection."));
    }
    let mut objects = Vec::with_capacity(snapshot.entities.len());
    for (record, entity) in snapshot.entities.iter().enumerate() {
        let source = &entity.position;
        let off_world = (source.x == i16::MIN && source.y == i16::MIN) || source.level == 0;
        if !off_world
            && (source.level < 1
                || source.level > levels as i8
                || source.x < 0
                || source.y < 0
                || i32::from(source.x) >= i32::from(width) * 16
                || i32::from(source.y) >= i32::from(height) * 16)
        {
            return Err(invalid(
                "object position is outside the source architecture",
            ));
        }
        let (avatar, yaw) = match &entity.appearance {
            Appearance::Object { direction, .. } => {
                if !direction.is_power_of_two() {
                    diagnostics.push(diagnostic("source_noncanonical_direction", format!("object:{}:direction:{direction}", entity.object_id),
                        "The source object has a noncanonical direction byte. Its orientation follows the original DirectionUtils.Log2Int rule."));
                }
                // Source Log2Int(0)=0 and non-powers use their highest set bit.
                let notch = if *direction == 0 {
                    0
                } else {
                    7 - direction.leading_zeros()
                };
                let mut yaw = notch as f32 * std::f32::consts::FRAC_PI_4;
                if yaw > std::f32::consts::PI {
                    yaw -= std::f32::consts::TAU;
                }
                (false, yaw)
            }
            Appearance::Avatar(avatar) => {
                if !avatar.yaw.is_finite() {
                    return Err(invalid("nonfinite avatar direction"));
                }
                (true, avatar.yaw)
            }
        };
        let level = if off_world { 0 } else { source.level as u8 };
        let x = f32::from(source.x) / 16.;
        let y = f32::from(source.y) / 16.;
        // VMGameObject.VisualPosition subtracts .5 before EntityComponent.
        // AvatarComponent receives the raw center position without that step.
        let center_offset = if avatar { 0. } else { 0.5 };
        let altitude = if off_world {
            0.
        } else {
            source_contact_altitude(
                architecture,
                x + 0.5 - center_offset,
                y + 0.5 - center_offset,
            )
        };
        let hidden = entity
            .object_data
            .get(34)
            .is_some_and(|&hidden| hidden != 0);
        let visible = !off_world && !hidden;
        objects.push(WorldObject {
            source_guid: entity.guid,
            blueprint: None,
            snapshot: Some(SnapshotObject {
                record: record as u32,
                object_id: entity.object_id,
                persistent_id: entity.persist_id,
                x: source.x,
                y: source.y,
                level: source.level,
                avatar,
                presentation_generation,
            }),
            entity: None,
            visual_revision: presentation_generation,
            position_tiles: Vec3::new(
                x - center_offset,
                y - center_offset,
                f32::from(level.saturating_sub(1)) * 2.95 + altitude,
            ),
            yaw_radians: yaw,
            dynamic_flags: [entity.dynamic_flags, entity.dynamic_flags2],
            room: source_object_room(entity),
            level,
            visible,
            selectable: visible,
            model: None,
        });
        if visible {
            diagnostics.push(diagnostic(if avatar { "missing_avatar_visual" } else { "missing_object_model" },
                format!("object:{:08X}:source-id:{}:record:{record}", entity.guid, entity.object_id),
                if avatar { format!("Avatar {} retains its original outfit and animation records in the snapshot. Its source Vitaboy visual resources have not been resolved.", entity.object_id) }
                else { format!("Object {:08X} (source ID {}) retains its original position and dynamic sprite flags. Its source model and textures have not been resolved.", entity.guid, entity.object_id) }));
        }
        if entity.container != 0 {
            diagnostics.push(diagnostic("unresolved_container_slot", format!("object:{}:container:{}", entity.object_id, entity.container),
                "A contained object's source tile pose is retained. Its displayed slot position requires the original SLOT/bone resources."));
        }
    }
    let effective_source = AssetKey(Sha256::digest(&snapshot.source_body).into());
    let document = WorldDocument {
        schema_version: WORLD_SCHEMA_VERSION,
        provenance: WorldProvenance {
            kind: WorldSourceKind::LegacySnapshot,
            origin: format!("FSOv38:packed-location/{:08X}", snapshot.platform.lot_id),
            source_revision: ORIGINAL_SOURCE_REVISION.into(),
            effective_source,
        },
        revision: WorldRevision {
            lot_id: None,
            epoch,
            tick,
            architecture_revision: presentation_generation,
            content: effective_source,
        },
        lot: WorldLot {
            width,
            height,
            levels,
            terrain,
            tiles,
            roof: Some(WorldRoof {
                material: architecture.roof_style,
                pitch: architecture.roof_pitch,
                advanced: true,
                average_color: [1.; 4],
                texture_scale: 1.,
            }),
            cutaway: None,
        },
        objects,
        models: vec![],
        materials: vec![],
        source_counts: Some(counts),
        category: Some(i32::from(snapshot.platform.category)),
        sounds: vec![],
        diagnostics,
    };
    document.validate()?;
    Ok(document)
}

// Blueprint.InterpAltitude has a clamped interior sample, distinct from the
// TerrainComponent raw-zero outer edge. Remainder and truncation match C#.
fn source_contact_altitude(architecture: &Architecture, x: f32, y: f32) -> f32 {
    let width = usize::from(architecture.width);
    let height = usize::from(architecture.height);
    let sample = |value: f32, bound: usize| value.min(bound as f32 - 1.).max(1.) as usize % bound;
    let (bx, nx) = (sample(x, width), sample(x.ceil(), width));
    let (by, ny) = (sample(y, height), sample(y.ceil(), height));
    let (u, v) = (x % 1., y % 1.);
    let h = |x, y| f32::from(architecture.heights[y * width + x]);
    let top = u * h(nx, by) + (1. - u) * h(bx, by);
    let bottom = u * h(nx, ny) + (1. - u) * h(bx, ny);
    (v * bottom + (1. - v) * top) * 3. / 160.
}

fn source_object_room(entity: &Entity) -> u16 {
    if let Appearance::Object { disabled, .. } = entity.appearance {
        // VMGameObject.RefreshLight: disabled rooms are the source grayscale
        // shader sentinel; light-generating non-window/door objects use 65535.
        if disabled >= 4 {
            return 65_533;
        }
        let flags = entity.object_data.get(40).copied().unwrap_or(0) as u16;
        if flags & (1 << 10) != 0
            && flags & ((1 << 14) | (1 << 15)) == 0
            && entity
                .object_data
                .get(51)
                .is_some_and(|&contribution| contribution > 0)
        {
            return 65_535;
        }
    }
    entity
        .object_data
        .get(29)
        .map_or(0, |room| room.wrapping_add(1) as u16)
}

fn terrain_colors(light: u8, dark: u8) -> Result<([f32; 4], [f32; 4]), WorldError> {
    // LotTypes.cs and TerrainComponent.UpdateLotType, including TS1 terrain.
    const GREEN: [[u8; 3]; 8] = [
        [80, 116, 59],
        [181, 171, 149],
        [126, 96, 70],
        [240, 245, 250],
        [0, 0, 255],
        [74, 89, 66],
        [140, 113, 49],
        [240, 245, 250],
    ];
    const BROWN: [[u8; 3]; 8] = [
        [157, 117, 65],
        [196, 185, 162],
        [126, 96, 70],
        [240, 245, 250],
        [0, 0, 255],
        [90, 69, 41],
        [115, 73, 33],
        [15, 20, 140],
    ];
    let color = |rgb: [u8; 3]| {
        [
            f32::from(rgb[0]) / 255.,
            f32::from(rgb[1]) / 255.,
            f32::from(rgb[2]) / 255.,
            1.,
        ]
    };
    let light_color = GREEN
        .get(usize::from(light))
        .ok_or_else(|| invalid("unknown terrain light type"))?;
    let dark_color = (if light == dark { &BROWN } else { &GREEN })
        .get(usize::from(dark))
        .ok_or_else(|| invalid("unknown terrain dark type"))?;
    Ok((color(*light_color), color(*dark_color)))
}
