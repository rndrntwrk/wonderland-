use wonderland_contracts::Anchor;
use wonderland_web_shell::geometry::{Camera, Point, Size, clamp_overlay};

fn close(got: f64, want: f64) {
    assert!((got - want).abs() < 0.001, "got {got}, want {want}");
}

#[test]
fn cover_camera_projects_the_same_normalized_point_as_the_image() {
    let camera = Camera::new(Size {
        width: 836.0,
        height: 470.5,
    });
    close(camera.scale(), 0.5);
    let point = camera.project(Anchor { x: 0.5, y: 0.5 });
    close(point.x, 418.0);
    close(point.y, 235.25);
}

#[test]
fn zoom_keeps_the_point_under_the_pointer_still() {
    let mut camera = Camera::new(Size {
        width: 836.0,
        height: 470.5,
    });
    camera.zoom_at(
        2.0,
        Point {
            x: 209.0,
            y: 235.25,
        },
    );
    let point = camera.project(Anchor { x: 0.25, y: 0.5 });
    close(point.x, 209.0);
    close(point.y, 235.25);
    close(camera.scale(), 1.0);
}

#[test]
fn panning_cannot_reveal_empty_space_outside_the_illustration() {
    let mut camera = Camera::new(Size {
        width: 836.0,
        height: 470.5,
    });
    camera.zoom_at(
        2.0,
        Point {
            x: 418.0,
            y: 235.25,
        },
    );
    camera.pan_by(Point {
        x: 9999.0,
        y: -9999.0,
    });
    close(camera.origin().x, 0.0);
    close(camera.origin().y, -470.5);
}

#[test]
fn focusing_a_mobile_destination_brings_its_pick_into_view() {
    let mut camera = Camera::new(Size {
        width: 390.0,
        height: 844.0,
    });
    camera.focus(Anchor { x: 0.72, y: 0.8 });
    close(camera.project(Anchor { x: 0.72, y: 0.8 }).x, 195.0);
    camera.resize(Size {
        width: 1440.0,
        height: 900.0,
    });
    assert!(camera.origin().x <= 0.0);
    assert!(camera.origin().y <= 0.0);
}

#[test]
fn focused_cafe_below_the_viewport_is_revealed_after_vertical_pan() {
    let mut camera = Camera::new(Size {
        width: 1363.0,
        height: 936.0,
    });
    let cafe = Anchor { x: 0.515, y: 0.74 };
    camera.zoom_at(2.8, Point { x: 681.5, y: 468.0 });
    camera.pan_by(Point { x: 0.0, y: 120.0 });
    let clipped = camera.project(cafe);
    assert!((60.0..1303.0).contains(&clipped.x));
    assert!(clipped.y > 936.0);

    camera.reveal(cafe, 76.0, 137.0);

    close(camera.project(cafe).x, 681.5);
    close(camera.project(cafe).y, 468.0);
    close(camera.zoom, 2.8);
}

#[test]
fn focus_reveals_targets_under_chrome_but_keeps_visible_targets_still() {
    let center = Anchor { x: 0.5, y: 0.5 };
    for (y, expected_y) in [(40.0, 468.0), (870.0, 468.0), (300.0, 300.0)] {
        let mut camera = Camera::new(Size {
            width: 1363.0,
            height: 936.0,
        });
        camera.zoom_at(2.8, Point { x: 681.5, y: 468.0 });
        camera.pan_by(Point {
            x: 0.0,
            y: y - 468.0,
        });
        camera.reveal(center, 76.0, 137.0);
        close(camera.project(center).x, 681.5);
        close(camera.project(center).y, expected_y);
    }
}

#[test]
fn overlays_stay_between_top_chrome_and_bottom_hud_near_each_edge() {
    let viewport = Size {
        width: 390.0,
        height: 844.0,
    };
    let size = Size {
        width: 290.0,
        height: 240.0,
    };
    assert_eq!(
        clamp_overlay(Point { x: -40.0, y: 800.0 }, size, viewport, 90.0, 210.0),
        Point { x: 12.0, y: 394.0 }
    );
    assert_eq!(
        clamp_overlay(
            Point {
                x: 700.0,
                y: -100.0
            },
            size,
            viewport,
            90.0,
            210.0
        ),
        Point { x: 88.0, y: 90.0 }
    );
}
