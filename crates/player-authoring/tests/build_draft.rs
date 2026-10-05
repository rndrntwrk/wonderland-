use wonderland_player_authoring::*;
fn context() -> BuildDraftContext {
    BuildDraftContext {
        actor: SourceActorLot {
            avatar_id: 42,
            lot_id: None,
            location: 55,
            epoch: 2,
            incarnation: 7,
        },
        generation: 8,
        bounds: LotBounds {
            width: 10,
            height: 10,
            levels: 2,
        },
    }
}
fn resource(tool: u8) -> BuildResource {
    BuildResource {
        tool,
        pattern: 1234,
        style: 9,
        name: "Source resource".into(),
        price: None,
        thumbnail: None,
    }
}
fn tile(x: u16, y: u16) -> BuildTile {
    BuildTile { x, y, level: 1 }
}
#[test]
fn source_rectangle_uses_minimum_and_deltas_not_cell_counts() {
    for tool in [2, 5] {
        let mut d = BuildTileDraft::default();
        assert_eq!(
            d.pick(&context(), &resource(tool), tile(6, 7), Some(false))
                .unwrap(),
            BuildPick::Started
        );
        assert_eq!(
            d.pick(&context(), &resource(tool), tile(2, 3), Some(false))
                .unwrap(),
            BuildPick::Ready
        );
        let c = d.command.unwrap();
        assert_eq!((c.x, c.y, c.x2, c.y2), (2, 3, 4, 4));
        assert_eq!(c.pattern, 1234);
        assert_eq!(c.style, if tool == 5 { 0 } else { 9 });
    }
}
#[test]
fn wall_line_matches_original_length_direction_and_rejects_snapped_overflow() {
    let mut d = BuildTileDraft::default();
    d.pick(&context(), &resource(0), tile(2, 2), Some(false))
        .unwrap();
    d.pick(&context(), &resource(0), tile(4, 4), Some(false))
        .unwrap();
    let c = d.command.unwrap();
    assert_eq!((c.x2, c.y2), (3, 1));
    let mut d = BuildTileDraft::default();
    d.pick(&context(), &resource(1), tile(6, 6), Some(false))
        .unwrap();
    assert!(
        d.pick(&context(), &resource(1), tile(9, 9), Some(false))
            .is_err()
    );
}
#[test]
fn source_fill_is_one_pick_and_floor_single_diagonal_requires_half() {
    for tool in [4, 6] {
        let mut d = BuildTileDraft::default();
        assert_eq!(
            d.pick(&context(), &resource(tool), tile(3, 4), Some(false))
                .unwrap(),
            BuildPick::Ready
        );
        let c = d.command.unwrap();
        assert_eq!((c.x2, c.y2, c.style), (0, 0, 0));
    }
    let mut d = BuildTileDraft::default();
    d.pick(&context(), &resource(5), tile(3, 4), Some(true))
        .unwrap();
    assert_eq!(
        d.pick(&context(), &resource(5), tile(3, 4), Some(true)),
        Err(AuthoringError::Missing("diagonal floor half selection"))
    );
}
#[test]
fn cancel_and_context_change_cannot_reuse_old_start() {
    let mut d = BuildTileDraft::default();
    d.pick(&context(), &resource(5), tile(2, 2), Some(false))
        .unwrap();
    d.cancel();
    assert!(d.start.is_none());
    d.pick(&context(), &resource(5), tile(3, 3), Some(false))
        .unwrap();
    let mut changed = context();
    changed.generation += 1;
    assert_eq!(
        d.pick(&changed, &resource(5), tile(4, 4), Some(false)),
        Err(AuthoringError::Stale)
    );
    assert!(d.command.is_none());
    assert!(d.start.is_none());
}
#[test]
fn unsupported_inputs_and_outside_level_are_explicit() {
    for tool in [3, 7, 8, 9] {
        assert_eq!(
            BuildTileDraft::default().pick(&context(), &resource(tool), tile(2, 2), Some(false)),
            Err(AuthoringError::Unsupported(tool))
        );
    }
    let mut t = tile(2, 2);
    t.level = 3;
    assert!(
        BuildTileDraft::default()
            .pick(&context(), &resource(6), t, Some(false))
            .is_err()
    );
}
