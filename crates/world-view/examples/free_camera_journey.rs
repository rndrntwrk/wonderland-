// Local integration witness through serialized controls and the real renderer
// camera path. Browser focus, live authority and visual evidence are separate.
use wonderland_world_view::{
    ViewportControls, WorldDocument, active_camera, advance_free_camera, camera_projection,
    stop_free_camera, toggle_free_camera,
};
fn main() {
    let world = WorldDocument::from_blueprint_xml(
        "<house><size>8</size><world><floors/><walls/></world><objects/></house>",
        "test:camera-journey",
        "declared camera integration fixture",
    )
    .unwrap();
    let orbit = ViewportControls::default();
    let mut saved = serde_json::to_value(orbit).unwrap();
    saved["free_camera"] = serde_json::json!({
        "position":[12.0,6.0,12.0], "velocity":[0.0,0.0,0.0],
        "fov_y":std::f32::consts::FRAC_PI_4, "saved_pitch":1.1
    });
    let free: ViewportControls = serde_json::from_value(saved).unwrap();
    assert_ne!(
        camera_projection(&world, orbit, 1.6).unwrap(),
        camera_projection(&world, free, 1.6).unwrap(),
        "Free camera must drive the actual world projection"
    );

    let before_world = serde_json::to_string(&world).unwrap();
    let before = active_camera(&world, orbit, 1.6).unwrap();
    let mut controls = orbit;
    toggle_free_camera(&world, &mut controls, 1.6).unwrap();
    let entered = active_camera(&world, controls, 1.6).unwrap();
    assert_eq!(before.position, entered.position);
    let direction_before = before.target - before.position;
    let direction_after = entered.target - entered.position;
    let cosine = direction_before.dot(direction_after)
        / (direction_before.length() * direction_after.length());
    assert!(
        cosine > 0.99999,
        "Entering free camera must preserve facing"
    );
    let mut strafe = controls;
    strafe.yaw_radians = 0.;
    let start_z = strafe.free_camera.unwrap().position[2];
    advance_free_camera(&world, &mut strafe, [1., 0., 0.], 0.05, false).unwrap();
    assert!(
        strafe.free_camera.unwrap().position[2] < start_z,
        "D moves screen-right when facing -X"
    );
    let mut at_30 = controls;
    let mut at_60 = controls;
    for _ in 0..30 {
        advance_free_camera(&world, &mut at_30, [0., 1., 0.], 1. / 30., false).unwrap();
    }
    for _ in 0..60 {
        advance_free_camera(&world, &mut at_60, [0., 1., 0.], 1. / 60., false).unwrap();
    }
    let a = at_30.free_camera.unwrap().position;
    let b = at_60.free_camera.unwrap().position;
    assert!(
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.001),
        "Flight is presentation-rate independent"
    );
    stop_free_camera(&mut at_60);
    let stopped = at_60;
    advance_free_camera(&world, &mut at_60, [0.; 3], 60., false).unwrap();
    assert_eq!(
        at_60, stopped,
        "Stopped input must not drift after suspension"
    );
    let mut bad = controls;
    bad.free_camera.as_mut().unwrap().position[0] = f32::NAN;
    assert!(camera_projection(&world, bad, 1.6).is_err());
    let mut slope = world.clone();
    for row in slope.lot.terrain.corners.chunks_mut(9) {
        for (x, value) in row.iter_mut().enumerate() {
            *value = if x == 0 { 0 } else { 160 };
        }
    }
    slope.lot.terrain.altitude_centers.fill(80);
    let mut slope_camera = controls;
    slope_camera.free_camera.as_mut().unwrap().position = [2.97, 0., 1.5];
    advance_free_camera(&slope, &mut slope_camera, [0.; 3], 0., false).unwrap();
    assert!(
        slope_camera.free_camera.unwrap().position[1] >= 9.9,
        "Flight stays above actual sloping terrain"
    );
    let mut grounded = controls;
    grounded.free_camera.as_mut().unwrap().position[1] = -100.;
    advance_free_camera(&world, &mut grounded, [0., 0., -1.], 0.05, false).unwrap();
    assert!(grounded.free_camera.unwrap().position[1] >= 1.);
    toggle_free_camera(&world, &mut controls, 1.6).unwrap();
    assert!(controls.free_camera.is_none());
    assert_eq!(
        serde_json::to_string(&world).unwrap(),
        before_world,
        "Camera must not alter the world"
    );
    println!(
        "Free camera entry, rendering, frame rates, stop/resume, invalid input, floor and authority isolation passed."
    );
}
