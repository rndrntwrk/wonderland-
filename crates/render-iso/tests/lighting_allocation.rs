use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};
use wonderland_render_core::{AssetKey, Vec2, Vec3};
use wonderland_render_iso::*;

struct CountingAllocator;
static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let result = System.alloc(layout);
        if !result.is_null() {
            let live =
                LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed) + layout.size() as isize;
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        result
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        System.dealloc(ptr, layout);
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

// Many individually valid meshes must not allocate beyond the scene's remaining
// bytes before its final combined-budget rejection. Only this test is in this
// executable, so the allocator's peak cannot include another concurrent test.
#[test]
fn rejected_geometry_respects_peak_allocation_budget() {
    let source = RoomMapInput {
        source: AssetKey([1; 32]),
        lot_id: 1,
        epoch: 1,
        revision: 1,
        width: 3,
        height: 3,
        stories: 1,
        cells: vec![
            RoomMapCell {
                first: 1,
                second: 1,
                diagonal: RoomDiagonal::None,
                floor_pattern: 1
            };
            9
        ],
        rooms: vec![LightingRoom {
            id: 1,
            floor: 0,
            outside: false,
            outside_light: 0,
            ambient_light: 0,
        }],
        minimum: [0; 4],
        outside: [255; 4],
    };
    let maps = prepare_room_maps(&source, LightingBudget::default()).unwrap();
    let light = SceneLight {
        id: 1,
        room: 1,
        floor: 0,
        light: ShadowLight {
            kind: ShadowLightKind::Point,
            position_sixteenths: Vec2::ZERO,
            direction: Vec2::new(1., 0.),
            radius_sixteenths: 256.,
            falloff_multiplier: 1.,
        },
        color: [255; 3],
        intensity: 1.,
        outdoors_color: false,
        window_room: None,
        height: 2.,
    };
    let lights = [light];
    let one = MeshShadowInput {
        source: AssetKey([3; 32]),
        vertices: vec![
            Vec3::new(0., 1., 0.),
            Vec3::new(3., 1., 0.),
            Vec3::new(0., 1., 3.),
        ],
        indices: [0, 1, 2].repeat(300),
    };
    let mesh_rooms = [RoomMeshShadows {
        room: 1,
        floor: 0,
        meshes: vec![one; 40],
    }];
    let input = LightingSceneInput {
        room_maps: &maps,
        geometry: &[],
        lights: &lights,
        outdoors: None,
        ultra: true,
        software_depth: false,
        directional: false,
    };
    let mut budget = LightingBudget::default();
    budget.max_total_bytes = 500_000;
    let before = LIVE.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let result = prepare_lighting_scene_with_meshes(&input, &mesh_rooms, budget);
    let peak = (PEAK.load(Ordering::Relaxed) - before) as usize;
    assert!(result.is_err());
    assert!(
        peak + maps.resident_bytes() <= budget.max_total_bytes,
        "allocated peak {peak} plus maps {} exceeds {}",
        maps.resident_bytes(),
        budget.max_total_bytes
    );
    wcrc_capture_and_atlas_peak_stays_within_the_same_budget();
}

fn wcrc_capture_and_atlas_peak_stays_within_the_same_budget() {
    let mut budget = LightingBudget::default();
    budget.max_total_bytes = 30_000_000;
    let maps = prepare_room_maps(
        &RoomMapInput {
            source: AssetKey([51; 32]),
            lot_id: 1,
            epoch: 1,
            revision: 1,
            width: 64,
            height: 64,
            stories: 5,
            cells: (0..64 * 64 * 5)
                .map(|i| RoomMapCell {
                    first: (i / (64 * 64) + 1) as u16,
                    second: (i / (64 * 64) + 1) as u16,
                    diagonal: RoomDiagonal::None,
                    floor_pattern: 1,
                })
                .collect(),
            rooms: (0..5)
                .map(|floor| LightingRoom {
                    id: u16::from(floor) + 1,
                    floor,
                    outside: true,
                    outside_light: 100,
                    ambient_light: 0,
                })
                .collect(),
            minimum: [0; 4],
            outside: [255; 4],
        },
        budget,
    )
    .unwrap();
    let input = WcrcGeometryInput {
        source: AssetKey([52; 32]),
        quality_divider: 1,
        walls: vec![WcrcWallGroup {
            source: AssetKey([53; 32]),
            floor: 0,
            use_offset: false,
            mask: WcrcShadowTexture {
                source: AssetKey([54; 32]),
                image: wonderland_render_core::RgbaImage {
                    width: 1,
                    height: 1,
                    pixels: vec![[255; 4]],
                },
            },
            vertices: vec![
                WcrcWallVertex {
                    position: Vec3::new(1., 1., 0.),
                    texture: Vec2::new(0., 0.),
                },
                WcrcWallVertex {
                    position: Vec3::new(1., 2., 0.),
                    texture: Vec2::new(1., 0.),
                },
                WcrcWallVertex {
                    position: Vec3::new(1., 2., 1.),
                    texture: Vec2::new(1., 1.),
                },
                WcrcWallVertex {
                    position: Vec3::new(1., 1., 1.),
                    texture: Vec2::new(0., 1.),
                },
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        }],
        floors: vec![],
        roofs: vec![],
        objects: vec![],
        noise: WcrcShadowTexture {
            source: AssetKey([55; 32]),
            image: wonderland_render_core::RgbaImage {
                width: 512,
                height: 512,
                pixels: (0..512 * 512)
                    .map(|i| [(i * 53 % 255) as u8, (i * 97 % 255) as u8, 0, 255])
                    .collect(),
            },
        },
    };
    let sun = DirectionalLighting {
        light: ShadowLight {
            kind: ShadowLightKind::Directional,
            position_sixteenths: Vec2::ZERO,
            direction: Vec2::new(1., 0.),
            radius_sixteenths: 1.,
            falloff_multiplier: 1.,
        },
        shadow_multiplier: 1.,
        sun_direction: Vec3::new(0., -1., 0.),
    };
    let before = LIVE.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let sunlight = prepare_wcrc_sunlight(&input, &maps, sun, false, false, budget).unwrap();
    let scene = prepare_lighting_scene_with_wcrc(
        &LightingSceneInput {
            room_maps: &maps,
            geometry: &[],
            lights: &[],
            outdoors: Some(sun),
            ultra: false,
            software_depth: false,
            directional: false,
        },
        &[],
        &sunlight,
        budget,
    )
    .unwrap();
    let peak = (PEAK.load(Ordering::Relaxed) - before) as usize + maps.resident_bytes();
    assert!(
        peak <= budget.max_total_bytes,
        "WCRC peak {peak} exceeds {}",
        budget.max_total_bytes
    );
    let actual_retained = (LIVE.load(Ordering::Relaxed) - before) as usize;
    assert_eq!(
        actual_retained,
        sunlight.resident_bytes() + scene.resident_bytes(),
        "published WCRC/atlas residency must include every owned allocation"
    );
    eprintln!(
        "WCRC64x64x5 measured peak={peak}, allocated retained={actual_retained}, budget={}",
        budget.max_total_bytes
    );
}
