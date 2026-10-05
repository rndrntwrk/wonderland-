use wonderland_render_core::derivatives::*;
use wonderland_render_core::*;

fn input(tick: u64, content: u8) -> DerivativeInput {
    DerivativeInput {
        frame: RenderFrame {
            stamp: FrameStamp {
                lot_id: 3,
                epoch: 7,
                tick,
                architecture_revision: 1,
                content: AssetKey([content; 32]),
            },
            entities: vec![],
            selected: None,
        },
        source_provenance: AssetKey([9; 32]),
        lighting: [LightingPass::DAY, LightingPass::NIGHT],
        materials: vec![DerivativeMaterial::solid(
            [1., 0.4, 0.2, 1.],
            [0.1, 0.2, 0.4, 1.],
        )],
        draws: vec![DerivativeDraw {
            owner: None,
            source_asset: AssetKey([4; 32]),
            model: Mat4::IDENTITY,
            material: 0,
            layer: DrawLayer::Wall,
            mesh: Mesh {
                vertices: vec![vertex(-0.9, -0.9), vertex(0.9, -0.9), vertex(0., 0.9)],
                indices: vec![0, 1, 2],
            },
        }],
        output: DerivativeOutput::Thumbnail(ThumbnailRequest {
            width: 16,
            height: 16,
            clip_from_world: Mat4::IDENTITY,
            clear: [0; 4],
        }),
    }
}
fn vertex(x: f32, y: f32) -> Vertex {
    Vertex {
        position: Vec3::new(x, y, 0.5),
        normal: Vec3::new(0., 0., 1.),
        uv: Vec2::ZERO,
        color: [1.; 4],
    }
}
fn prepared(tick: u64, content: u8) -> PreparedDerivative {
    PreparedDerivative::new(input(tick, content), DerivativeRenderLimits::default()).unwrap()
}
fn queue(limits: QueueLimits) -> DerivativeQueue {
    let q = DerivativeQueue::new(limits, RenderLimits::default()).unwrap();
    q.reset(3, 7).unwrap();
    q.admit_frame(input(1, 1).frame).unwrap();
    q
}

#[test]
fn repeated_render_is_identical_and_day_night_are_distinct() {
    let job = prepared(1, 1);
    let a = job.render().unwrap();
    let b = job.render().unwrap();
    assert_eq!(a.digest(), b.digest());
    assert_eq!(a.images(), b.images());
    assert_ne!(a.images()[0].image, a.images()[1].image);
    assert!(a.images()[0].image.pixels.iter().any(|p| p[3] != 0));
}

#[test]
fn source_and_material_bytes_are_hashed_without_trusting_declared_provenance() {
    let a = prepared(1, 1);
    let mut changed = input(1, 1);
    changed.materials[0].night.tint[0] = 0.9;
    let b = PreparedDerivative::new(changed, DerivativeRenderLimits::default()).unwrap();
    assert_ne!(a.key(), b.key());
    let mut changed = input(1, 1);
    changed.draws[0].mesh.vertices[0].position.x = -0.8;
    assert_ne!(
        a.key(),
        PreparedDerivative::new(changed, DerivativeRenderLimits::default())
            .unwrap()
            .key()
    );
}

#[test]
fn content_a_b_a_never_revives_old_work() {
    let q = queue(QueueLimits::default());
    q.submit(prepared(1, 1)).unwrap();
    let job = q.start_next().unwrap().unwrap();
    let artifact = job.request().render().unwrap();
    q.admit_frame(input(2, 2).frame).unwrap();
    q.admit_frame(input(3, 1).frame).unwrap();
    assert_eq!(job.complete(artifact).unwrap(), Completion::Stale);
    assert_eq!(q.stats().unwrap().resident_entries, 0);
}

#[test]
fn same_key_supersession_does_not_cancel_or_install_over_new_work() {
    let q = queue(QueueLimits::default());
    let old = q.submit(prepared(1, 1)).unwrap();
    let first = q.start_next().unwrap().unwrap();
    let artifact = first.request().render().unwrap();
    let new = q.submit(prepared(1, 1)).unwrap();
    assert!(!q.cancel(&old).unwrap());
    assert_eq!(first.complete(artifact).unwrap(), Completion::Stale);
    let second = q.start_next().unwrap().unwrap();
    assert_eq!(second.ticket(), &new);
    assert_eq!(second.execute().unwrap(), Completion::Installed);
    assert!(q.acquire(&new.key()).unwrap().is_some());
}

#[test]
fn queued_supersession_reuses_its_reservation_atomically() {
    let cost = prepared(1, 1).reservation_bytes();
    let q = queue(QueueLimits {
        max_queued: 1,
        max_pending_bytes: cost,
        ..QueueLimits::default()
    });
    let a = q.submit(prepared(1, 1)).unwrap();
    let b = q.submit(prepared(1, 1)).unwrap();
    assert_ne!(a, b);
    assert_eq!(q.stats().unwrap().queued, 1);
    assert_eq!(q.stats().unwrap().pending_bytes, cost);
}

#[test]
fn cancelled_running_work_retains_capacity_until_owned_handle_drops() {
    let cost = prepared(1, 1).reservation_bytes();
    let q = queue(QueueLimits {
        max_in_flight: 1,
        max_pending_bytes: cost,
        ..QueueLimits::default()
    });
    let ticket = q.submit(prepared(1, 1)).unwrap();
    let job = q.start_next().unwrap().unwrap();
    assert!(q.cancel(&ticket).unwrap());
    assert!(job.is_cancelled());
    assert_eq!(q.stats().unwrap().in_flight, 1);
    assert!(matches!(
        q.submit(prepared(1, 1)),
        Err(DerivativeError::Limit("pending bytes"))
    ));
    drop(job);
    assert_eq!(q.stats().unwrap().pending_bytes, 0);
    q.submit(prepared(1, 1)).unwrap();
}

#[test]
fn reset_to_same_boundary_invalidates_jobs_and_retains_running_reservations() {
    let q = queue(QueueLimits::default());
    q.submit(prepared(1, 1)).unwrap();
    let job = q.start_next().unwrap().unwrap();
    let artifact = job.request().render().unwrap();
    q.reset(3, 7).unwrap();
    q.admit_frame(input(1, 1).frame).unwrap();
    assert_eq!(q.stats().unwrap().in_flight, 1);
    assert_eq!(job.complete(artifact).unwrap(), Completion::Stale);
    assert_eq!(q.stats().unwrap().pending_bytes, 0);
}

#[test]
fn unrelated_queue_cannot_cancel_ticket_and_drop_releases_reservations() {
    let q = queue(QueueLimits::default());
    let other = queue(QueueLimits::default());
    let ticket = q.submit(prepared(1, 1)).unwrap();
    assert!(!other.cancel(&ticket).unwrap());
    let job = q.start_next().unwrap().unwrap();
    drop(job);
    assert_eq!(q.stats().unwrap().pending_bytes, 0);
}

#[test]
fn stale_frames_and_malformed_geometry_do_not_replace_valid_requests() {
    let q = queue(QueueLimits::default());
    q.submit(prepared(1, 1)).unwrap();
    assert!(q.admit_frame(input(0, 2).frame).is_err());
    let mut malformed = input(1, 1);
    malformed.draws[0].mesh.indices[0] = u32::MAX;
    assert!(PreparedDerivative::new(malformed, DerivativeRenderLimits::default()).is_err());
    assert_eq!(q.stats().unwrap().queued, 1);
    q.start_next().unwrap().unwrap().execute().unwrap();
}

#[test]
fn late_failure_does_not_remove_a_replacement_request() {
    let q = queue(QueueLimits::default());
    q.submit(prepared(1, 1)).unwrap();
    let old = q.start_next().unwrap().unwrap();
    let new = q.submit(prepared(1, 1)).unwrap();
    drop(old);
    let active = q.start_next().unwrap().unwrap();
    assert_eq!(active.ticket(), &new);
    active.execute().unwrap();
}

#[test]
fn wrong_artifact_cannot_be_installed() {
    let q = queue(QueueLimits::default());
    q.submit(prepared(1, 1)).unwrap();
    let job = q.start_next().unwrap().unwrap();
    let wrong = prepared(2, 2).render().unwrap();
    assert!(matches!(
        job.complete(wrong),
        Err(DerivativeError::Invalid("completion key"))
    ));
    assert_eq!(q.stats().unwrap().resident_entries, 0);
    assert_eq!(q.stats().unwrap().pending_bytes, 0);
}

#[test]
fn pinned_artifacts_remain_accounted_across_reset_until_last_lease_drops() {
    let q = queue(QueueLimits::default());
    let ticket = q.submit(prepared(1, 1)).unwrap();
    q.start_next().unwrap().unwrap().execute().unwrap();
    let lease = q.acquire(&ticket.key()).unwrap().unwrap();
    let before = q.stats().unwrap().resident_bytes;
    assert!(before > 0);
    assert!(!q.evict(&ticket.key()).unwrap());
    q.reset(3, 7).unwrap();
    assert!(q.acquire(&ticket.key()).unwrap().is_none());
    assert_eq!(q.stats().unwrap().resident_bytes, before);
    assert_eq!(q.stats().unwrap().retired_entries, 1);
    assert_eq!(lease.artifact().key(), ticket.key());
    drop(lease);
    assert_eq!(q.stats().unwrap().resident_bytes, 0);
}

#[test]
fn resident_budget_is_enforced_and_unpinned_artifacts_are_evicted() {
    let one = prepared(1, 1).render().unwrap().resident_bytes();
    let q = queue(QueueLimits {
        max_resident_bytes: one,
        max_resident_entries: 1,
        ..QueueLimits::default()
    });
    let first = q.submit(prepared(1, 1)).unwrap();
    q.start_next().unwrap().unwrap().execute().unwrap();
    let mut other = input(1, 1);
    other.materials[0].day.tint[0] = 0.8;
    let other = PreparedDerivative::new(other, DerivativeRenderLimits::default()).unwrap();
    let second = q.submit(other).unwrap();
    q.start_next().unwrap().unwrap().execute().unwrap();
    assert!(q.acquire(&first.key()).unwrap().is_none());
    assert!(q.acquire(&second.key()).unwrap().is_some());
    assert_eq!(q.stats().unwrap().resident_bytes, one);
}

#[test]
fn invalid_cameras_materials_frame_owners_and_work_limits_are_rejected_before_render() {
    let mut bad = input(1, 1);
    bad.materials[0].night.tint[0] = f32::NAN;
    assert!(PreparedDerivative::new(bad, DerivativeRenderLimits::default()).is_err());
    let mut bad = input(1, 1);
    bad.draws[0].owner = Some(EntityRef {
        object_id: 8,
        generation: 1,
    });
    assert!(PreparedDerivative::new(bad, DerivativeRenderLimits::default()).is_err());
    let mut bad = input(1, 1);
    bad.draws[0].model.cols[0][0] = f32::INFINITY;
    assert!(PreparedDerivative::new(bad, DerivativeRenderLimits::default()).is_err());
    assert!(PreparedDerivative::new(
        input(1, 1),
        DerivativeRenderLimits {
            max_work_units: 1,
            ..DerivativeRenderLimits::default()
        }
    )
    .is_err());
}

#[test]
fn explicit_eviction_cancels_pending_work_and_late_completion_cannot_resurrect_it() {
    let q = queue(QueueLimits::default());
    let ticket = q.submit(prepared(1, 1)).unwrap();
    let job = q.start_next().unwrap().unwrap();
    let artifact = job.request().render().unwrap();
    assert!(q.evict(&ticket.key()).unwrap());
    assert!(job.is_cancelled());
    assert_eq!(job.complete(artifact).unwrap(), Completion::Stale);
    assert!(q.acquire(&ticket.key()).unwrap().is_none());
    assert_eq!(q.stats().unwrap().pending_bytes, 0);
}

#[test]
fn queue_and_running_count_limits_are_independent() {
    let q = queue(QueueLimits {
        max_queued: 1,
        max_in_flight: 1,
        ..QueueLimits::default()
    });
    q.submit(prepared(1, 1)).unwrap();
    let mut different = input(1, 1);
    different.materials[0].day.tint[1] = 0.9;
    assert!(matches!(
        q.submit(
            PreparedDerivative::new(different.clone(), DerivativeRenderLimits::default()).unwrap()
        ),
        Err(DerivativeError::Limit("queued requests"))
    ));
    let first = q.start_next().unwrap().unwrap();
    q.submit(PreparedDerivative::new(different, DerivativeRenderLimits::default()).unwrap())
        .unwrap();
    assert!(q.start_next().unwrap().is_none());
    drop(first);
    assert!(q.start_next().unwrap().is_some());
}

#[test]
fn failed_pinned_replacement_keeps_the_existing_artifact() {
    let one = prepared(1, 1).render().unwrap().resident_bytes();
    let q = queue(QueueLimits {
        max_resident_bytes: one,
        max_resident_entries: 1,
        ..QueueLimits::default()
    });
    let first = q.submit(prepared(1, 1)).unwrap();
    q.start_next().unwrap().unwrap().execute().unwrap();
    let lease = q.acquire(&first.key()).unwrap().unwrap();
    q.submit(prepared(1, 1)).unwrap();
    assert!(matches!(
        q.start_next().unwrap().unwrap().execute(),
        Err(DerivativeError::Limit("resident capacity"))
    ));
    assert!(q.acquire(&first.key()).unwrap().is_some());
    assert_eq!(q.stats().unwrap().resident_bytes, one);
    assert_eq!(q.stats().unwrap().pending_bytes, 0);
    drop(lease);
}

#[test]
fn dropping_the_queue_detaches_running_jobs_and_owned_leases_safely() {
    let q = queue(QueueLimits::default());
    let ticket = q.submit(prepared(1, 1)).unwrap();
    q.start_next().unwrap().unwrap().execute().unwrap();
    let lease = q.acquire(&ticket.key()).unwrap().unwrap();
    q.submit(prepared(1, 1)).unwrap();
    let job = q.start_next().unwrap().unwrap();
    drop(q);
    assert!(job.is_cancelled());
    assert_eq!(job.execute().unwrap(), Completion::Stale);
    assert_eq!(lease.artifact().key(), ticket.key());
}

fn facade_input() -> DerivativeInput {
    let mut value = input(1, 1);
    value.draws.clear();
    value.output = DerivativeOutput::Facade(FacadeRequest {
        lot_width: 77,
        lot_height: 77,
        floor_tiles: 64,
        floor_resolution_per_tile: 2,
        stories: 2,
        floors_used: 2,
        roof_on_floor: true,
        walls: vec![],
        thumbnail: None,
    });
    value
}
fn wall(points: [[i32; 2]; 2]) -> FacadeWall {
    FacadeWall {
        points,
        floor: 0,
        terrain_height: 0.,
        outside: OutsideSide::Left,
        room_provenance: AssetKey([6; 32]),
    }
}

#[test]
fn floor_layout_matches_three_by_two_cells_and_source_overlay_index() {
    let request =
        PreparedDerivative::new(facade_input(), DerivativeRenderLimits::default()).unwrap();
    let output = request.render().unwrap();
    assert_eq!(
        (
            output.images()[0].image.width,
            output.images()[0].image.height
        ),
        (384, 256)
    );
    assert_eq!(
        (
            output.images()[2].image.width,
            output.images()[2].image.height
        ),
        (512, 4)
    );
    let regions: Vec<_> = request.regions().collect();
    assert_eq!(regions.len(), 3);
    assert_eq!(regions[0].rect, [0, 0, 128, 128]);
    assert_eq!(regions[1].rect, [128, 0, 128, 128]);
    assert_eq!(regions[2].role, RegionRole::ObjectOverlay(2));
    assert_eq!(regions[2].rect, [256, 0, 128, 128]);
    // This intentionally describes GenerateFloor's slot `stories`, not the
    // separate GetFSOF raised-floor UV convention that always names slot 5.
    let center = regions[0]
        .clip_from_world
        .transform_vec4([115.5, 0., 115.5, 1.]);
    assert_eq!(center, [0., 0., 0.5, 1.]);
    let increasing_x = regions[0]
        .clip_from_world
        .transform_vec4([211.5, 0., 115.5, 1.]);
    assert!((increasing_x[0] + 1.).abs() < 1e-6);
}

#[test]
fn wall_first_fit_preserves_round_even_gaps_and_four_pixel_height_alignment() {
    let mut value = facade_input();
    if let DerivativeOutput::Facade(facade) = &mut value.output {
        facade.walls = vec![
            wall([[0, 0], [33, 0]]),
            wall([[0, 0], [35, 0]]),
            wall([[0, 0], [1024, 0]]),
        ];
    }
    let request = PreparedDerivative::new(value, DerivativeRenderLimits::default()).unwrap();
    let regions: Vec<_> = request
        .regions()
        .filter(|r| matches!(r.role, RegionRole::Wall(_)))
        .collect();
    assert_eq!(regions[0].rect, [0, 0, 16, 22]);
    assert_eq!(regions[1].rect, [18, 0, 18, 22]);
    assert_eq!(regions[2].rect, [0, 24, 512, 22]);
    let output = request.render().unwrap();
    assert_eq!(output.images()[2].image.height, 48);
    let center_y = 0.5 * 2.95 * 3. + 0.2;
    let center = regions[0]
        .clip_from_world
        .transform_vec4([33. / 32. * 3., center_y, 0., 1.]);
    assert!(center[0].abs() < 1e-6);
    assert!((center[1] - 2. / 22.).abs() < 1e-6);
    assert!((center[2] - 0.5).abs() < 1e-6);
}

#[test]
fn wall_outside_side_flips_camera_but_preserves_endpoint_uv_direction() {
    let mut left = facade_input();
    if let DerivativeOutput::Facade(facade) = &mut left.output {
        facade.walls.push(wall([[0, 0], [160, 0]]));
    }
    let mut right = left.clone();
    if let DerivativeOutput::Facade(facade) = &mut right.output {
        facade.walls[0].outside = OutsideSide::Right;
    }
    let left = PreparedDerivative::new(left, DerivativeRenderLimits::default()).unwrap();
    let right = PreparedDerivative::new(right, DerivativeRenderLimits::default()).unwrap();
    let a = left
        .regions()
        .find(|r| matches!(r.role, RegionRole::Wall(_)))
        .unwrap()
        .clip_from_world;
    let b = right
        .regions()
        .find(|r| matches!(r.role, RegionRole::Wall(_)))
        .unwrap()
        .clip_from_world;
    let point = [7.5, 4.625, 0., 1.];
    let ap = a.transform_vec4(point);
    let bp = b.transform_vec4(point);
    assert!((ap[0] - bp[0]).abs() < 1e-6);
    assert!((ap[2] - bp[2]).abs() < 1e-6);
    assert_ne!(a, b);
}

#[test]
fn floor_scissor_leaves_a_one_pixel_transparent_border() {
    let mut value = facade_input();
    value.draws.push(DerivativeDraw {
        owner: None,
        source_asset: AssetKey([4; 32]),
        model: Mat4::IDENTITY,
        material: 0,
        layer: DrawLayer::Floor(0),
        mesh: Mesh {
            vertices: [
                [19.5, 0., 19.5],
                [211.5, 0., 19.5],
                [211.5, 0., 211.5],
                [19.5, 0., 211.5],
            ]
            .into_iter()
            .map(|p| Vertex {
                position: Vec3::new(p[0], p[1], p[2]),
                normal: Vec3::new(0., 1., 0.),
                uv: Vec2::ZERO,
                color: [1.; 4],
            })
            .collect(),
            indices: vec![0, 1, 2, 0, 2, 3],
        },
    });
    let output = PreparedDerivative::new(value, DerivativeRenderLimits::default())
        .unwrap()
        .render()
        .unwrap();
    let image = &output.images()[0].image;
    assert_eq!(image.pixels[0], [0; 4]);
    assert_eq!(image.pixels[127], [0; 4]);
    assert_eq!(image.pixels[127 * 384 + 64], [0; 4]);
    assert_ne!(image.pixels[64 * 384 + 64][3], 0);
    assert_eq!(image.pixels.iter().filter(|p| p[3] > 0).count(), 126 * 126);
}

#[test]
fn malformed_facade_layouts_reject_zero_length_walls_and_oversized_atlases() {
    let mut value = facade_input();
    if let DerivativeOutput::Facade(facade) = &mut value.output {
        facade.walls.push(wall([[0, 0], [0, 0]]));
    }
    assert!(PreparedDerivative::new(value, DerivativeRenderLimits::default()).is_err());
    let mut value = facade_input();
    if let DerivativeOutput::Facade(facade) = &mut value.output {
        facade.floor_resolution_per_tile = u16::MAX;
    }
    assert!(PreparedDerivative::new(value, DerivativeRenderLimits::default()).is_err());
    let mut value = facade_input();
    if let DerivativeOutput::Facade(facade) = &mut value.output {
        facade.stories = 6;
        facade.floors_used = 6;
    }
    assert!(PreparedDerivative::new(value, DerivativeRenderLimits::default()).is_err());
}

#[test]
fn vertex_only_work_and_frame_entity_counts_are_bounded_before_transformation() {
    let mut value = input(1, 1);
    value.draws[0].mesh.indices.clear();
    assert!(matches!(
        PreparedDerivative::new(
            value,
            DerivativeRenderLimits {
                max_work_units: 1,
                ..DerivativeRenderLimits::default()
            }
        ),
        Err(DerivativeError::Limit("render work"))
    ));
    let mut value = input(1, 1);
    value.frame.entities.push(EntityProjection {
        reference: EntityRef {
            object_id: 1,
            generation: 1,
        },
        visual_revision: 1,
        transform: Transform::IDENTITY,
        previous_transform: None,
        asset: AssetKey([3; 32]),
        level: 1,
        visible: true,
        selectable: false,
    });
    assert!(matches!(
        PreparedDerivative::new(
            value,
            DerivativeRenderLimits {
                render: RenderLimits {
                    max_entities: 0,
                    ..RenderLimits::default()
                },
                ..DerivativeRenderLimits::default()
            }
        ),
        Err(DerivativeError::Limit("frame entities"))
    ));
}

#[test]
fn later_coplanar_source_draws_replace_earlier_materials() {
    let mut value = input(1, 1);
    value.materials.push(DerivativeMaterial::solid(
        [0., 1., 0., 1.],
        [0., 0., 1., 1.],
    ));
    let mut overlay = value.draws[0].clone();
    overlay.material = 1;
    value.draws.push(overlay);
    let output = PreparedDerivative::new(value, DerivativeRenderLimits::default())
        .unwrap()
        .render()
        .unwrap();
    assert_eq!(
        output.images()[0].image.pixels[8 * 16 + 8],
        [0, 255, 0, 255]
    );
    assert_eq!(
        output.images()[1].image.pixels[8 * 16 + 8],
        [0, 0, 255, 255]
    );
}
