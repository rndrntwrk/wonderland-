use wonderland_contracts::authoring::*;

fn projection() -> AuthoringProjection {
    serde_json::from_str(include_str!("../../../fixtures/ui/authoring-v2.json")).unwrap()
}

#[test]
fn aggregate_occupancy_work_is_bounded_before_allocating_each_footprint() {
    let mut projection = projection();
    projection.catalog[0].footprint = Footprint {
        width: 256,
        depth: 256,
    };
    let home = &mut projection.profiles[0].home;
    home.lot.bounds.width = 256;
    home.lot.bounds.depth = 256;
    home.lot.levels = (0..17).collect();
    home.lot.reserved.clear();
    home.instances = (0..17)
        .map(|level| OwnedInstance {
            id: format!("large-{level}").into(),
            catalog_id: "armchair".into(),
            owner_id: Some(home.owner_id.clone()),
            placement: Some(GridPose {
                cell: GridCell { x: 0, y: 0 },
                level,
                direction: Direction::North,
            }),
        })
        .collect();
    assert_eq!(projection.validate(), Err(AuthoringError::SafetyLimit));
}

#[test]
fn a_maximum_reserved_set_handles_large_disjoint_footprints_and_detects_intersection() {
    let mut projection = projection();
    projection.catalog[0].footprint = Footprint {
        width: 32,
        depth: 256,
    };
    let home = &mut projection.profiles[0].home;
    home.lot.bounds.width = 512;
    home.lot.bounds.depth = 256;
    home.lot.reserved = (0..256)
        .flat_map(|y| {
            (0..256).map(move |x| LotCell {
                cell: GridCell { x, y },
                level: 0,
            })
        })
        .collect();
    home.instances = (0..8)
        .map(|index| OwnedInstance {
            id: format!("strip-{index}").into(),
            catalog_id: "armchair".into(),
            owner_id: Some(home.owner_id.clone()),
            placement: Some(GridPose {
                cell: GridCell {
                    x: 256 + index * 32,
                    y: 0,
                },
                level: 0,
                direction: Direction::North,
            }),
        })
        .collect();
    projection.validate().unwrap();
    let home = &projection.profiles[0].home;
    assert_eq!(
        Footprint { width: 2, depth: 1 }.cells(
            GridPose {
                cell: GridCell { x: 255, y: 255 },
                level: 0,
                direction: Direction::North,
            },
            &home.lot
        ),
        Err(AuthoringError::EntranceReserved)
    );
}
