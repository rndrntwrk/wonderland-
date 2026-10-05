//! Disposable scene outlines. A draft never changes the displayed source world.
use wonderland_player_authoring::BuildTileDraft;
use wonderland_render_core::Vec3;
use wonderland_world_view::WorldDocument;

#[derive(Clone, Debug, Default)]
pub struct WorldDraftOutline {
    pub points: Vec<Vec3>,
    pub closed: bool,
}

pub fn draft_outline(world: &WorldDocument, draft: &BuildTileDraft) -> WorldDraftOutline {
    let (Some(start), Some(resource)) = (draft.start, draft.resource.as_ref()) else {
        return WorldDraftOutline::default();
    };
    let end = draft.end.unwrap_or(start);
    if start.level == 0 || start.level > world.lot.levels || end.level != start.level {
        return WorldDraftOutline::default();
    }
    let rectangle = draft.command.is_some() && matches!(resource.tool, 2 | 5);
    let mut coordinates = if rectangle {
        // Source floor rectangles include both selected cells. Wall rectangles
        // instead connect the two selected top-left vertices.
        let extra = u16::from(resource.tool == 5);
        let (left, top) = (start.x.min(end.x), start.y.min(end.y));
        let (right, bottom) = (
            start.x.max(end.x).saturating_add(extra),
            start.y.max(end.y).saturating_add(extra),
        );
        vec![(left, top), (right, top), (right, bottom), (left, bottom)]
    } else if draft.end.is_some() && start != end {
        vec![(start.x, start.y), (end.x, end.y)]
    } else {
        vec![(start.x, start.y)]
    };
    if coordinates
        .iter()
        .any(|&(x, y)| x > world.lot.width || y > world.lot.height)
    {
        return WorldDraftOutline::default();
    }
    let stride = usize::from(world.lot.width) + 1;
    let points: Option<Vec<_>> = coordinates
        .drain(..)
        .map(|(x, y)| {
            let raw = *world
                .lot
                .terrain
                .corners
                .get(usize::from(y) * stride + usize::from(x))?;
            // The original lot geometry uses three world units per tile, 3/160
            // tile units per height step and 2.95 tile units per story.
            let height = (f32::from(raw) - f32::from(world.lot.terrain.base_alt)) * 9. / 160.
                + f32::from(start.level - 1) * 8.85;
            Some(Vec3::new(
                f32::from(x) * 3.,
                height + 0.03,
                f32::from(y) * 3.,
            ))
        })
        .collect();
    WorldDraftOutline {
        points: points.unwrap_or_default(),
        closed: rectangle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wonderland_player_authoring::{ArchitectureCommand, BuildResource, BuildTile};

    fn draft(tool: u8) -> BuildTileDraft {
        BuildTileDraft {
            resource: Some(BuildResource {
                tool,
                pattern: 1,
                style: 1,
                name: "Test resource".into(),
                price: None,
                thumbnail: None,
            }),
            start: Some(BuildTile {
                x: 2,
                y: 3,
                level: 1,
            }),
            end: Some(BuildTile {
                x: 4,
                y: 6,
                level: 1,
            }),
            command: Some(ArchitectureCommand {
                kind: tool,
                x: 2,
                y: 3,
                level: 1,
                x2: 2,
                y2: 3,
                pattern: 1,
                style: 1,
            }),
            ..Default::default()
        }
    }
    #[test]
    fn floor_cell_extents_and_wall_vertex_extents_remain_distinct() {
        let world = WorldDocument::original_empty_lot().unwrap();
        let floor = draft_outline(&world, &draft(5));
        let wall = draft_outline(&world, &draft(2));
        assert!(floor.closed && wall.closed);
        assert_eq!((floor.points[2].x, floor.points[2].z), (15., 21.));
        assert_eq!((wall.points[2].x, wall.points[2].z), (12., 18.));
        assert_eq!((floor.points[0].x, floor.points[0].z), (6., 9.));
    }
    #[test]
    fn source_height_and_story_raise_the_same_draft_geometry() {
        let mut world = WorldDocument::original_empty_lot().unwrap();
        let stride = usize::from(world.lot.width) + 1;
        world.lot.terrain.corners[3 * stride + 2] = 160;
        world.lot.terrain.base_alt = 0;
        let low = draft_outline(&world, &draft(0));
        let mut upper = draft(0);
        upper.start.as_mut().unwrap().level = 2;
        upper.end.as_mut().unwrap().level = 2;
        let high = draft_outline(&world, &upper);
        assert!((low.points[0].y - 9.03).abs() < 0.001);
        assert!((high.points[0].y - low.points[0].y - 8.85).abs() < 0.001);
    }
}
