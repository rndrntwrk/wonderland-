use wonderland_render_3d::environment::*;
use wonderland_render_core::{AssetKey, Mat4, Vec2, Vec3, ViewMode};
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.0001, "{a} != {b}");
}

// Catches hemisphere-only geometry, wrong source winding/counts, and UVs outside the gradient sampling policy.
#[test]
fn sky_dome_uses_source_65_subdivisions_and_real_gradient_uvs() {
    let sky = build_sky_dome(0.5, 256, EnvironmentBudget::default()).unwrap();
    assert_eq!(sky.mesh().vertices.len(), 4225);
    assert_eq!(sky.mesh().indices.len(), 24765);
    assert_eq!(sky.mesh().vertices[0].position, Vec3::Y);
    close(sky.mesh().vertices[0].uv.x, 1.0625);
    close(sky.mesh().vertices[0].uv.y, 1. / 256.);
    assert_eq!(&sky.mesh().indices[..3], &[0, 2, 1]);
    assert!(sky
        .mesh()
        .vertices
        .iter()
        .all(|v| v.position.is_finite() && v.uv.is_finite()));
    let wrapped = build_sky_dome(1.5, 256, EnvironmentBudget::default()).unwrap();
    assert_eq!(sky.key(), wrapped.key());
    let dusk = build_sky_dome(0.8, 256, EnvironmentBudget::default()).unwrap();
    assert_ne!(sky.key(), dusk.key());
    let mut budget = EnvironmentBudget::default();
    budget.max_vertices = 4224;
    assert!(build_sky_dome(0.5, 256, budget).is_err());
}

// Catches wrong axis order or sun phase and unsafe horizon infinities.
#[test]
fn solar_transform_preserves_source_sun_moon_and_shadow_fade() {
    let sun = source_sun(0.55).unwrap();
    close(sun.sun_vector.x, 0.41562694);
    close(sun.sun_vector.y, 0.70710677);
    close(sun.sun_vector.z, -0.5720614);
    close(sun.falloff_multiplier, 1.);
    close(sun.shadow_multiplier, 1.);
    assert!(!sun.night);
    close(sun.shadow_direction.x, 0.80901694);
    close(sun.shadow_direction.y, 0.58778524);
    assert!(source_sun(0.).unwrap().night);
    close(source_sun(0.).unwrap().shadow_multiplier, 1.33);
    let dawn = source_sun(0.25).unwrap();
    close(dawn.shadow_multiplier, 0.);
    assert!(dawn.falloff_multiplier.is_finite());
    assert_eq!(source_sun(-1.).unwrap(), source_sun(0.).unwrap());
    assert!(source_sun(f64::NAN).is_err());
}

fn frame(t: f64) -> WeatherFrameInput {
    WeatherFrameInput {
        identity: EnvironmentIdentity {
            lot_id: 7,
            epoch: 3,
            content: AssetKey([8; 32]),
            device_generation: 1,
        },
        presentation_seconds: t,
        sim_speed: 1.,
        weather: decode_weather(2 | (1 << 8)),
        enabled: true,
        mode: ViewMode::Full3D,
        zoom: WeatherZoom::Near,
        camera_id: 1,
        camera_position: Vec3::ZERO,
        inverse_view_rotation: Mat4::IDENTITY,
        center_tile: Vec2::new(1., 1.),
        base_altitude_tiles: 0.,
        level: 1,
        stories: 1,
        width: 3,
        height: 3,
        indoors: vec![0; 9],
        outside_color: [255; 4],
    }
}

// Catches reseeding each frame, VM-clock coupling, bad source rain displacement, and omitted GPU mesh output.
#[test]
fn weather_executes_source_particles_with_injected_time_and_independent_seed() {
    let mut weather = WeatherPresentation::new(42);
    let first = weather
        .prepare(&frame(0.), EnvironmentBudget::default())
        .unwrap();
    assert_eq!(first.batches().len(), 1);
    assert_eq!(first.batches()[0].seeds().len(), 250);
    assert_eq!(first.batches()[0].mesh().vertices.len(), 1000);
    assert_eq!(first.batches()[0].mesh().indices.len(), 1500);
    let next = weather
        .prepare(&frame(1. / 30.), EnvironmentBudget::default())
        .unwrap();
    close(next.batches()[0].uniforms().time, 0.001 / 30.);
    assert_eq!(first.batches()[0].seeds(), next.batches()[0].seeds());
    assert_ne!(
        first.batches()[0].mesh().vertices,
        next.batches()[0].mesh().vertices
    );
    let again = weather
        .prepare(&frame(1. / 30.), EnvironmentBudget::default())
        .unwrap();
    assert_eq!(next.key(), again.key());
    let mut other = WeatherPresentation::new(42);
    let reference = other
        .prepare(&frame(0.), EnvironmentBudget::default())
        .unwrap();
    assert_eq!(first.key(), reference.key());
}

// Catches presentation cadence changing seed positions, source time, or fade amount.
#[test]
fn weather_time_and_fades_are_stable_at_30_60_and_120_hz() {
    let mut endpoints = vec![];
    for hz in [30, 60, 120] {
        let mut weather = WeatherPresentation::new(123);
        weather
            .prepare(&frame(0.), EnvironmentBudget::default())
            .unwrap();
        for i in 1..=hz {
            weather
                .prepare(&frame(i as f64 / hz as f64), EnvironmentBudget::default())
                .unwrap();
        }
        endpoints.push(
            weather
                .prepare(&frame(1.), EnvironmentBudget::default())
                .unwrap(),
        );
    }
    for end in &endpoints[1..] {
        assert_eq!(end.batches()[0].seeds(), endpoints[0].batches()[0].seeds());
        close(
            end.batches()[0].uniforms().time,
            endpoints[0].batches()[0].uniforms().time,
        );
        assert_eq!(
            end.batches()[0].mesh().vertices,
            endpoints[0].batches()[0].mesh().vertices
        );
    }
}

// Catches stale camera velocity, mutation on budget errors, and reusing a previous lot's weather emitters.
#[test]
fn weather_reset_camera_modes_and_budget_failures_are_atomic() {
    let mut weather = WeatherPresentation::new(7);
    let original = weather
        .prepare(&frame(0.), EnvironmentBudget::default())
        .unwrap();
    let mut moved = frame(0.1);
    moved.camera_position = Vec3::new(100., 0., 0.);
    moved.camera_id = 2;
    let cut = weather
        .prepare(&moved, EnvironmentBudget::default())
        .unwrap();
    assert_eq!(cut.batches()[0].uniforms().camera_velocity, Vec3::ZERO);
    let mut budget = EnvironmentBudget::default();
    budget.max_particles = 1;
    assert!(weather.prepare(&frame(0.2), budget).is_err());
    assert_eq!(
        weather
            .prepare(&moved, EnvironmentBudget::default())
            .unwrap()
            .key(),
        cut.key()
    );
    let mut next_lot = frame(0.);
    next_lot.identity.lot_id = 8;
    let reset = weather
        .prepare(&next_lot, EnvironmentBudget::default())
        .unwrap();
    close(reset.batches()[0].uniforms().time, 0.);
    assert_ne!(original.key(), reset.key());
    for mode in [ViewMode::Full2D, ViewMode::Hybrid2D, ViewMode::Full3D] {
        let mut f = frame(1.);
        f.identity = next_lot.identity;
        f.mode = mode;
        f.zoom = WeatherZoom::Far;
        let result = weather.prepare(&f, EnvironmentBudget::default()).unwrap();
        assert!(result.batches().iter().all(|b| b
            .mesh()
            .vertices
            .iter()
            .all(|v| v.position.is_finite())));
        assert_eq!(
            result.batches().len(),
            if mode == ViewMode::Full3D { 1 } else { 4 }
        );
    }
    weather.reset();
    let fresh = weather
        .prepare(&frame(0.), EnvironmentBudget::default())
        .unwrap();
    assert_eq!(original.key(), fresh.key());
}

// Catches unbounded old emitters, lost fade-outs, or replacing rain/snow without changing the active mode.
#[test]
fn weather_changes_crossfade_two_bounded_emitters_and_eventually_release_them() {
    let mut runtime = WeatherPresentation::new(9);
    runtime
        .prepare(&frame(0.), EnvironmentBudget::default())
        .unwrap();
    let mut snow = frame(0.1);
    snow.weather = decode_weather(2 | (1 << 8) | (1 << 9));
    let both = runtime
        .prepare(&snow, EnvironmentBudget::default())
        .unwrap();
    assert_eq!(both.batches().len(), 2);
    assert_eq!(both.batches()[0].kind(), WeatherType::Rain);
    assert_eq!(both.batches()[1].kind(), WeatherType::Snow);
    close(both.batches()[1].uniforms().color[3], 0.);
    snow.presentation_seconds = 1.1;
    let faded = runtime
        .prepare(&snow, EnvironmentBudget::default())
        .unwrap();
    close(faded.batches()[0].uniforms().color[3], 0.85);
    close(faded.batches()[1].uniforms().color[3], 0.2775);
    snow.presentation_seconds = 7.;
    let only = runtime
        .prepare(&snow, EnvironmentBudget::default())
        .unwrap();
    assert_eq!(only.batches().len(), 1);
    snow.presentation_seconds = 7.1;
    snow.enabled = false;
    assert_eq!(
        runtime
            .prepare(&snow, EnvironmentBudget::default())
            .unwrap()
            .batches()
            .len(),
        1
    );
    snow.presentation_seconds = 14.;
    assert!(runtime
        .prepare(&snow, EnvironmentBudget::default())
        .unwrap()
        .batches()
        .is_empty());
}

// Catches admitting snow/rain under a roof, wrong /2 model position, and NaN fog on dark nights.
#[test]
fn weather_fragment_roof_clipping_and_fog_follow_source_spaces() {
    let mut runtime = WeatherPresentation::new(2);
    let prepared = runtime
        .prepare(&frame(0.), EnvironmentBudget::default())
        .unwrap();
    let u = prepared.batches()[0].uniforms();
    assert!(!weather_fragment_visible(Vec3::new(3., 8.85, 3.), u, &[127; 9], 3, 3).unwrap());
    assert!(weather_fragment_visible(Vec3::new(3., 17.7, 3.), u, &[127; 9], 3, 3).unwrap());
    assert!(!weather_fragment_visible(Vec3::new(3., -0.1, 3.), u, &[0; 9], 3, 3).unwrap());
    let fog = atmosphere([0, 0, 0, 255], decode_weather(100 | (1 << 8))).unwrap();
    assert_eq!(fog.fog_color, [0.; 3]);
    close(fog.fog_distance, 1125.);
    close(fog.sky_opacity, 0.25);
    assert_eq!(fog.outside_tint, [159, 164, 181, 255]);
    let dry = atmosphere([255; 4], decode_weather(0)).unwrap();
    close(dry.fog_distance, 22500.);
    close(dry.sky_opacity, 1.);
}
