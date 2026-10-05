use wonderland_render_3d::{camera::*, environment::*};
use wonderland_render_core::{EntityRef, Mat4, Quat, Transform, Vec2, Vec3, ViewMode};
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.0001, "{a} != {b}");
}

// Catches source orbit matrix order, zoom/pitch clamping, and tile-axis swaps.
#[test]
fn orbit_camera_preserves_units_defaults_and_limits() {
    let mut c = OrbitCamera {
        yaw: 0.,
        pitch_control: 0.,
        zoom: 2.,
        center: Vec2::new(2., 3.),
        cam_height: 5.,
    };
    let p = c.pose().unwrap();
    assert_eq!(p.target, Vec3::new(6., 8., 9.));
    close(p.position.x, 21.2);
    close(p.position.y, 12.);
    close(p.position.z, 9.);
    c.pitch_control = -5.;
    c.zoom = -4.;
    let p = c.pose().unwrap();
    assert_eq!(p.position, Vec3::new(16., 8., 9.));
    c.inherit_2d(2, IsoZoom::Far).unwrap();
    close(c.zoom, 11.);
    close(c.yaw, 3. * std::f32::consts::FRAC_PI_4);
    close(c.pitch_control, 0.);
    assert!(c.pose().unwrap().view_projection(1.5).is_ok());
    c.zoom = f32::NAN;
    assert!(c.pose().is_err());
}

// Catches replacing C# banker's rounding with away-from-zero rounding.
#[test]
fn cut_rotation_uses_ties_to_even_and_positive_modulo() {
    for (yaw, expected) in [
        (0., 0),
        (std::f32::consts::FRAC_PI_2, 2),
        (std::f32::consts::PI, 2),
        (3. * std::f32::consts::FRAC_PI_2, 0),
        (-std::f32::consts::FRAC_PI_2, 0),
        (-std::f32::consts::PI, 2),
    ] {
        assert_eq!(cut_rotation(yaw).unwrap(), expected);
    }
    assert!(cut_rotation(f32::NAN).is_err());
}

// Catches integer RefreshRate/60 damping and any render-cadence acceleration drift.
#[test]
fn elapsed_time_damping_and_flight_match_at_30_60_120_hz() {
    let simulate = |hz| {
        let mut h = 0.;
        let mut c = FirstPersonCamera {
            position: Vec3::new(0., 2., 0.),
            velocity: Vec3::ZERO,
            yaw: 0.,
            pitch_control: 1.,
            fov_y: 0.9,
            captured: true,
            focused: true,
        };
        for _ in 0..hz {
            h = damp_height(h, 10., 1. / hz as f32).unwrap();
            c.advance(Vec3::new(3., 0., -1.), 1. / hz as f32, None)
                .unwrap();
        }
        (h, c)
    };
    let a = simulate(30);
    for hz in [60, 120] {
        let b = simulate(hz);
        close(a.0, b.0);
        assert!((a.1.position - b.1.position).length() < 0.0001);
        assert!((a.1.velocity - b.1.velocity).length() < 0.0001);
    }
    let mut c = a.1;
    c.position.y = -20.;
    c.advance(Vec3::ZERO, 0., Some(4.)).unwrap();
    close(c.position.y, 5.);
    assert!(c.advance(Vec3::ZERO, -1., None).is_err());
    c.captured = false;
    let before = c.position;
    c.advance(Vec3::new(100., 0., 0.), 1., None).unwrap();
    assert_eq!(c.position, before);
}

// Catches direct-camera head anchor scale, near-plane and identity leakage.
#[test]
fn direct_camera_mount_is_generation_aware_and_uses_source_head_clearance() {
    let anchor = HeadAnchor {
        entity: EntityRef {
            object_id: 7,
            generation: 2,
        },
        position_tile: Vec3::new(2., 3., 1.5),
        scale: 2.,
    };
    let p = direct_pose(anchor, 0., 1.).unwrap();
    assert_eq!(p.position, Vec3::new(6., 5., 9.));
    close(p.fov_y, 0.9);
    close(p.near, 0.5);
    assert_eq!(p.hide_head, Some(anchor.entity));
    let fp = FirstPersonCamera {
        position: Vec3::ZERO,
        velocity: Vec3::ZERO,
        yaw: 0.,
        pitch_control: 1.,
        fov_y: 0.9,
        captured: false,
        focused: false,
    }
    .pose()
    .unwrap();
    close(fp.near, 1.);
    assert_eq!(fp.hide_head, None);
}

// Catches matrix lerp of camera rotation instead of quaternion interpolation.
#[test]
fn transition_slerps_pose_lerps_projection_and_handles_zero_duration() {
    let to = Transform {
        translation: Vec3::new(10., 2., 0.),
        rotation: Quat::from_axis_angle(Vec3::Y, std::f32::consts::PI / 2.).unwrap(),
        scale: Vec3::new(2., 2., 2.),
    };
    let mut projection = Mat4::IDENTITY;
    projection.cols[0][0] = 3.;
    let sample = sample_transition(
        Transform::IDENTITY,
        to,
        Mat4::IDENTITY,
        projection,
        0.33,
        0.66,
    )
    .unwrap();
    assert_eq!(sample.transform.translation, Vec3::new(5., 1., 0.));
    close(sample.transform.rotation.rotate_vec3(Vec3::X).x, 0.70710677);
    close(sample.projection.cols[0][0], 2.);
    assert!(!sample.complete);
    let instant =
        sample_transition(Transform::IDENTITY, to, Mat4::IDENTITY, projection, 0., 0.).unwrap();
    assert_eq!(instant.transform, to);
    assert!(instant.complete);
    assert!(
        sample_transition(Transform::IDENTITY, to, Mat4::IDENTITY, projection, 0., -1.).is_err()
    );
}

// Catches interpreting the weather word as normalized 0..1 or losing packed fields.
#[test]
fn manual_weather_word_preserves_intensity_type_and_thunder() {
    let w = decode_weather(150 | (1 << 8) | (2 << 9) | (1 << 11));
    assert!(w.manual && w.thunder);
    assert_eq!(w.kind, WeatherType::Hail);
    close(w.intensity, 1.5);
    close(w.darken, 0.);
    assert_eq!(decode_weather(3 << 9).kind, WeatherType::Unknown);
    close(decode_weather(100 | (1 << 8)).darken, 1.);
}

// Catches adding simulation time/RNG dependencies or a hard automatic-weather hour jump.
#[test]
fn automatic_weather_crossfades_injected_utc_and_honors_tuning() {
    let before = automatic_weather(3599, AutoWeatherTuning::Rain, false).unwrap();
    let start = automatic_weather(3600, AutoWeatherTuning::Rain, false).unwrap();
    close(start.intensity, before.intensity);
    let mid = automatic_weather(3675, AutoWeatherTuning::Rain, false).unwrap();
    let end = automatic_weather(3750, AutoWeatherTuning::Rain, false).unwrap();
    close(mid.intensity, (start.intensity + end.intensity) / 2.);
    assert_eq!(end.kind, WeatherType::Rain);
    close(
        automatic_weather(3750, AutoWeatherTuning::Rain, true)
            .unwrap()
            .intensity,
        0.,
    );
    assert_eq!(
        automatic_weather(3750, AutoWeatherTuning::Snow, false)
            .unwrap()
            .kind,
        WeatherType::Snow
    );
}

// Catches linear-light interpolation or blending across the deliberate 1->0 sky jump.
#[test]
fn time_of_day_interpolates_bytes_then_power_and_wraps_safely() {
    let midnight = time_of_day(0., 1.).unwrap();
    assert_eq!(midnight.outside_rgba, [11, 23, 81, 255]);
    assert!(midnight.night);
    assert_eq!(time_of_day(1., 1.).unwrap(), midnight);
    assert_eq!(time_of_day(-1., 1.).unwrap(), midnight);
    let noon = time_of_day(0.5, 1.).unwrap();
    close(noon.sky_gradient, 1.);
    assert_eq!(noon.outside_rgba, [255; 4]);
    close(time_of_day(7. / 13., 1.).unwrap().sky_gradient, 0.);
    assert!(!time_of_day(0.25, 1.).unwrap().night);
    assert_eq!(time_of_day(0., 0.).unwrap().outside_rgba, [255; 4]);
    assert!(time_of_day(f64::NAN, 1.).is_err());
}

// Catches silent capability over-commit or conflating backend choice with view mode.
#[test]
fn quality_tiers_and_aa_follow_source_semantics_with_explicit_capability_limits() {
    let req = QualityRequest {
        lighting: 3,
        aa: AntiAlias::Ssaa2,
        weather: true,
        surrounding_lots: 2,
        directional: true,
        complex_shaders: true,
        transitions: true,
    };
    let cap = Capabilities {
        max_msaa: 8,
        ssaa: true,
        depth_stencil: true,
        complex_shaders: true,
        memory_budget_bytes: 64 * 1024 * 1024,
    };
    let hi = choose_quality(req, cap, QualityTier::Ultra, ViewMode::Full3D).unwrap();
    assert!(hi.advanced_lighting && hi.shadows_3d && hi.ultra_lighting);
    assert_eq!(hi.aa, AppliedAa::Ssaa(2));
    assert_eq!(
        choose_quality(req, cap, QualityTier::Ultra, ViewMode::Hybrid2D)
            .unwrap()
            .aa,
        AppliedAa::Msaa(8)
    );
    let low = choose_quality(req, cap, QualityTier::Low, ViewMode::Full3D).unwrap();
    assert!(!low.shadows_3d && !low.ultra_lighting && !low.complex_shaders);
    assert_eq!(low.object_lod(1.).unwrap(), ObjectLod::Hidden);
    assert_eq!(low.object_lod(200.).unwrap(), ObjectLod::Full);
    assert!(low.object_lod(f32::NAN).is_err());
    let limited = Capabilities {
        max_msaa: 1,
        ssaa: false,
        depth_stencil: false,
        complex_shaders: false,
        memory_budget_bytes: 1024,
    };
    let p = choose_quality(req, limited, QualityTier::Ultra, ViewMode::Full3D).unwrap();
    assert_eq!(p.aa, AppliedAa::Off);
    assert!(!p.shadows_3d && !p.complex_shaders);
}

// Catches finite inputs overflowing into an admitted camera pose or damping result.
#[test]
fn extreme_camera_inputs_fail_atomically_without_nonfinite_outputs(){
    let orbit=OrbitCamera{center:Vec2::new(f32::MAX,0.),..OrbitCamera::default()};
    assert!(orbit.pose().is_err());
    assert!(damp_height(f32::MAX,-f32::MAX,0.001).unwrap().is_finite());
    let mut fp=FirstPersonCamera{position:Vec3::new(f32::MAX,0.,0.),velocity:Vec3::ZERO,yaw:0.,pitch_control:1.,fov_y:0.9,captured:true,focused:true};
    let before=fp;
    assert!(fp.advance(Vec3::new(f32::MAX,0.,0.),100.,None).is_err());
    assert_eq!(fp,before);
}
