use wonderland_contracts::authoring::{Direction, Footprint, GridCell, GridPose, LotBounds};
use wonderland_web_shell::authoring_geometry::*;
use wonderland_web_shell::geometry::{Point, Size};
fn pick(camera: HomeCamera, screen: Point) -> Option<GridCell> {
    wonderland_web_shell::authoring_geometry::pick(
        camera,
        screen,
        LotBounds {
            origin: GridCell { x: 0, y: 0 },
            width: 8,
            depth: 6,
        },
    )
}
#[test]
fn affine_round_trips_with_pan_zoom_and_edges() {
    let mut camera = HomeCamera::new(Size {
        width: 390.,
        height: 490.,
    });
    camera.zoom_at(1.7, Point { x: 195., y: 245. });
    camera.pan_by(Point { x: 31., y: -62. });
    for y in 0..6 {
        for x in 0..8 {
            let cell = GridCell { x, y };
            assert_eq!(pick(camera, camera.project(cell_center(cell))), Some(cell));
        }
    }
    assert_eq!(
        pick(camera, camera.project(Point { x: -200., y: -100. })),
        None
    );
    assert_eq!(pick(camera, Point { x: f64::NAN, y: 0. }), None);
}
#[test]
fn reveal_keeps_first_and_last_rows_above_drawer() {
    let mut camera = HomeCamera::new(Size {
        width: 390.,
        height: 490.,
    });
    for cell in [GridCell { x: 1, y: 0 }, GridCell { x: 7, y: 5 }] {
        camera.reveal(cell_center(cell), 75., 50.);
        let p = camera.project(cell_center(cell));
        assert!((75. ..=440.).contains(&p.y));
        assert!((40. ..=350.).contains(&p.x));
    }
}
#[test]
fn rotated_extent_and_sprite_width_are_stable() {
    let f = Footprint { width: 2, depth: 1 };
    let pose = GridPose {
        cell: GridCell { x: 3, y: 2 },
        level: 0,
        direction: Direction::East,
    };
    assert_eq!(f.rotated(pose.direction), Footprint { width: 1, depth: 2 });
    assert_eq!(footprint_points(f, pose).len(), 4);
    for id in ["armchair", "coffee-table", "bookcase"] {
        let a = sprite_layout(id, Direction::North);
        let b = sprite_layout(id, Direction::South);
        let (front_span, back_span, image_width, expected) = match id {
            "armchair" => (1137., 1196., 1254., 145.),
            "coffee-table" => (1345., 1385., 1536., 230.),
            _ => (974., 1146., 1536., 220.),
        };
        let front_effective = a.width * front_span / image_width;
        let back_effective = b.width * back_span / image_width;
        assert!((front_effective - back_effective).abs() < 0.001);
        assert!((front_effective - expected).abs() < 0.001);
    }
}

#[test]
fn nearby_actions_are_clamped_below_chrome_above_drawer() {
    for size in [
        Size {
            width: 390.,
            height: 584.,
        },
        Size {
            width: 1440.,
            height: 775.,
        },
    ] {
        let camera = HomeCamera::new(size);
        for cell in [GridCell { x: 0, y: 0 }, GridCell { x: 7, y: 5 }] {
            let p = owned_action_position(camera, cell_center(cell));
            assert!(p.x >= 12.);
            assert!(p.x + (size.width - 24.).min(330.) <= size.width - 12.);
            assert!(p.y >= if size.width <= 700. { 200. } else { 160. });
            assert!(p.y + 64. <= size.height - 20.);
        }
    }
}

#[test]
fn mobile_edge_placements_and_rotations_reveal_actual_sprite_and_footprint() {
    let projection = wonderland_client_app::authoring::preview_authoring_projection();
    for item in &projection.catalog {
        let directions = if item.can_rotate() {
            vec![
                Direction::North,
                Direction::East,
                Direction::South,
                Direction::West,
            ]
        } else {
            vec![Direction::North]
        };
        for direction in directions {
            let f = item.footprint.rotated(direction);
            for cell in [
                GridCell {
                    x: 0,
                    y: 6 - i16::try_from(f.depth).unwrap(),
                },
                GridCell {
                    x: 8 - i16::try_from(f.width).unwrap(),
                    y: 0,
                },
                GridCell { x: 1, y: 0 },
                GridCell {
                    x: 8 - i16::try_from(f.width).unwrap(),
                    y: 6 - i16::try_from(f.depth).unwrap(),
                },
            ] {
                let pose = GridPose {
                    cell,
                    level: 0,
                    direction,
                };
                assert!(
                    item.footprint
                        .cells(pose, &projection.profiles[0].home.lot)
                        .is_ok()
                );
                let mut camera = HomeCamera::new(Size {
                    width: 390.,
                    height: 654.,
                });
                camera.zoom = 734. / (941. * (390. / 1672.));
                camera.pan_by(Point { x: -200., y: 150. });
                let bounds = furniture_bounds(item.id.as_ref(), item.footprint, pose);
                camera.reveal_bounds(bounds, 240., 70.);
                let min = camera.project(bounds.min);
                let max = camera.project(bounds.max);
                assert!(
                    min.x >= 40. - 1e-7 && max.x <= 350. + 1e-7,
                    "{} {:?} {:?}: {:?}..{:?}",
                    item.id,
                    direction,
                    cell,
                    min,
                    max
                );
                assert!(
                    min.y >= 240. - 1e-7 && max.y <= 584. + 1e-7,
                    "{} {:?} {:?}: {:?}..{:?}",
                    item.id,
                    direction,
                    cell,
                    min,
                    max
                );
                // Independently project the rectangle used by Furniture's actual CSS layout.
                let sprite = sprite_layout(item.id.as_ref(), pose.direction);
                let center = footprint_center(item.footprint, pose);
                let sprite_min = camera.project(Point {
                    x: center.x - sprite.anchor.x,
                    y: center.y - sprite.anchor.y,
                });
                let sprite_max = camera.project(Point {
                    x: center.x - sprite.anchor.x + sprite.width,
                    y: center.y - sprite.anchor.y + sprite.height,
                });
                assert!(sprite_min.x >= 40. - 1e-7 && sprite_max.x <= 350. + 1e-7);
                assert!(sprite_min.y >= 240. - 1e-7 && sprite_max.y <= 584. + 1e-7);
                let revealed_pan = camera.pan;
                let revealed_zoom = camera.zoom;
                camera.reveal_bounds(bounds, 240., 70.);
                assert!(
                    (camera.pan.x - revealed_pan.x).abs() < 1e-7
                        && (camera.pan.y - revealed_pan.y).abs() < 1e-7
                );
                assert!((camera.zoom - revealed_zoom).abs() < 1e-7);
                for point in footprint_points(item.footprint, pose) {
                    let p = camera.project(point);
                    assert!(
                        p.x >= min.x - 1e-7
                            && p.x <= max.x + 1e-7
                            && p.y >= min.y - 1e-7
                            && p.y <= max.y + 1e-7
                    );
                }
            }
        }
    }
}

#[test]
fn oversized_furniture_fits_by_zooming_out_or_centers_at_minimum_zoom() {
    let f = Footprint { width: 2, depth: 1 };
    let pose = GridPose {
        cell: GridCell { x: 0, y: 4 },
        level: 0,
        direction: Direction::North,
    };
    let bounds = furniture_bounds("bookcase", f, pose);
    let mut camera = HomeCamera::new(Size {
        width: 390.,
        height: 654.,
    });
    camera.zoom = 4.;
    camera.reveal_bounds(bounds, 240., 70.);
    assert!(camera.zoom < 4. && camera.zoom >= 1.);
    let min = camera.project(bounds.min);
    let max = camera.project(bounds.max);
    assert!(min.x >= 40. - 1e-7 && max.x <= 350. + 1e-7);
    let mut tiny = HomeCamera::new(Size {
        width: 100.,
        height: 400.,
    });
    tiny.reveal_bounds(bounds, 240., 70.);
    assert_eq!(tiny.zoom, 1.);
    let min = tiny.project(bounds.min);
    let max = tiny.project(bounds.max);
    assert!(max.x - min.x > 20.);
    assert!(((min.x + max.x) / 2. - 50.).abs() < 1e-7);
    tiny.reset();
    assert_eq!(tiny.pan, Point::default());
    assert_eq!(tiny.zoom, 1.);
}

#[test]
fn source_lot_art_transform_round_trips_nonzero_origins_and_large_lots() {
    let mut camera = HomeCamera::new(Size {
        width: 1440.,
        height: 760.,
    });
    camera.zoom_at(1.4, Point { x: 400., y: 300. });
    camera.pan_by(Point { x: 40., y: -25. });
    for bounds in [
        LotBounds {
            origin: GridCell { x: -17, y: 23 },
            width: 24,
            depth: 18,
        },
        LotBounds {
            origin: GridCell { x: 100, y: -200 },
            width: 512,
            depth: 512,
        },
    ] {
        let transform = LotArtTransform::new(bounds);
        for cell in [
            bounds.origin,
            GridCell {
                x: bounds.origin.x + 1,
                y: bounds.origin.y + 1,
            },
            GridCell {
                x: bounds.origin.x + bounds.width as i16 - 1,
                y: bounds.origin.y + bounds.depth as i16 - 1,
            },
        ] {
            assert_eq!(
                transform.pick(camera, camera.project(transform.cell_center(cell))),
                Some(cell)
            );
        }
        assert_eq!(transform.pick(camera, Point { x: f64::NAN, y: 1. }), None);
        assert_eq!(
            transform.pick(
                camera,
                camera.project(
                    transform.point(f64::from(bounds.origin.x) - 0.1, f64::from(bounds.origin.y))
                )
            ),
            None
        );
    }
}

#[test]
fn dense_lot_dom_budget_does_not_limit_pointer_picking() {
    let camera = HomeCamera::new(Size {
        width: 1440.,
        height: 760.,
    });
    let t = LotArtTransform::new(LotBounds {
        origin: GridCell { x: 0, y: 0 },
        width: 512,
        depth: 512,
    });
    assert!(t.visible_cells(camera, 4096).is_empty());
    let cell = GridCell { x: 500, y: 500 };
    assert_eq!(
        t.pick(camera, camera.project(t.cell_center(cell))),
        Some(cell)
    );
}
