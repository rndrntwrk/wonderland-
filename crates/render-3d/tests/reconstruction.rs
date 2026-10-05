use wonderland_render_3d::{reconstruction::resolution::*, reconstruction::*, Error};
use wonderland_render_core::{Aabb, AssetKey, Mesh, RenderLimits, Vec2, Vec3, Vertex};
fn key(b: u8) -> AssetKey {
    AssetKey([b; 32])
}
fn sprite() -> ReconstructionSprite {
    ReconstructionSprite {
        source: key(1),
        sprite_id: 100,
        rotation: 0,
        width: 2,
        height: 2,
        sprite_offset: Vec2::ZERO,
        object_offset: Vec3::ZERO,
        flip: false,
        rgba: vec![[255, 128, 0, 255]; 4],
        depth: Some(vec![254; 4]),
        dynamic_index: None,
    }
}
fn params() -> ReconstructionParams {
    let mut p = ReconstructionParams::default();
    p.simplify = false;
    p
}
fn candidate(id: f32) -> MeshCandidate {
    let v = |x, y| Vertex {
        position: Vec3::new(x, y, 0.),
        normal: Vec3::Z,
        uv: Vec2::ZERO,
        color: [1.; 4],
    };
    MeshCandidate {
        mesh: Mesh {
            vertices: vec![v(id, 0.), v(id + 1., 0.), v(id, 1.)],
            indices: vec![0, 1, 2],
        },
        format_version: 3,
        reconstruction_version: 0,
        cache_key: None,
        mask: MaskType::None,
    }
}

// Catches filename parameters overriding embedded parameters and wrong DoorFix quadrants.
#[test]
fn parameters_range_and_door_fix_precedence_are_exact() {
    let mut embedded = params();
    embedded.rotations = [false, true, false, false];
    embedded.start_dgrp = 10;
    embedded.end_dgrp = 20;
    let mut named = params();
    named.rotations = [true, false, false, false];
    assert_eq!(select_params(10, Some(&embedded), Some(&named)), embedded);
    assert_eq!(select_params(20, Some(&embedded), Some(&named)), embedded);
    assert_eq!(
        select_params(21, Some(&embedded), Some(&named)),
        ReconstructionParams::default()
    );
    assert_eq!(select_params(30, None, Some(&named)), named);
    embedded.door_fix = true;
    assert_eq!(selected_rotations(&embedded, 1), [0, 3]);
    assert_eq!(selected_rotations(&embedded, 0), [1, 2]);
    assert_eq!(selected_rotations(&embedded, 0x101), [0, 3]);
}

// Catches smoothing across discontinuities or accepting only one diagonal check.
#[test]
fn triangulation_rejects_discontinuities_and_requires_both_quad_diagonals() {
    let p = [
        Some(Vec3::ZERO),
        Some(Vec3::new(0.03, 0., 0.)),
        Some(Vec3::new(0.03, 0.03, 0.)),
        Some(Vec3::new(0., 0.03, 0.)),
    ];
    assert_eq!(triangulate_quad(p, 0.065).unwrap(), [0, 1, 2, 0, 2, 3]);
    let mut three = p;
    three[3] = None;
    assert_eq!(triangulate_quad(three, 0.065).unwrap(), [0, 1, 2]);
    three[2] = None;
    assert!(triangulate_quad(three, 0.065).unwrap().is_empty());
    let exact = [
        Some(Vec3::ZERO),
        Some(Vec3::new(0.065, 0., 0.)),
        Some(Vec3::new(0.0325, 0., 0.)),
        None,
    ];
    assert_eq!(triangulate_quad(exact, 0.065).unwrap(), [0, 1, 2]);
    let mut above = exact;
    above[1] = Some(Vec3::new(0.065001, 0., 0.));
    assert!(triangulate_quad(above, 0.065).unwrap().is_empty());
    let mut other = p;
    other[3] = Some(Vec3::new(-1., 0., 0.));
    assert!(triangulate_quad(other, 0.065).unwrap().is_empty());
    assert!(triangulate_quad(p, f32::NAN).is_err());
}

// Catches byte254 being mistaken for absent depth and applying alpha semantics implicitly.
#[test]
fn depth_alpha_and_missing_resources_complete_without_async_deadlock() {
    let mut s = sprite();
    s.rgba[0][3] = 0;
    let out = reconstruct(&[s.clone()], &params(), 0, ReconstructionOptions::default()).unwrap();
    assert_eq!(out.parts[0].mesh.indices.len(), 6);
    assert_eq!(out.parts[0].mesh.vertices[0].color[3], 0.);
    assert_eq!(out.completed, 1);
    let mut o = ReconstructionOptions::default();
    o.alpha = AlphaPolicy::DropTransparent;
    assert_eq!(
        reconstruct(&[s.clone()], &params(), 0, o).unwrap().parts[0]
            .mesh
            .indices
            .len(),
        3
    );
    s.depth.as_mut().unwrap()[0] = 255;
    assert_eq!(
        reconstruct(&[s.clone()], &params(), 0, ReconstructionOptions::default())
            .unwrap()
            .parts[0]
            .mesh
            .indices
            .len(),
        3
    );
    s.depth = None;
    let out = reconstruct(&[s], &params(), 0, o).unwrap();
    assert!(out.parts.is_empty());
    assert_eq!(out.missing_depth, [100]);
    assert_eq!(out.completed, 1);
}

// Catches wrong matrix order, offsets, flip pivot and a second content-axis negation.
#[test]
fn projection_preserves_literal_source_matrix_and_flip_direction() {
    let s = sprite();
    let base = project_pixel(&s, &params(), 0., 0., 254).unwrap();
    assert!((base.x - (-0.3607488)).abs() < 0.0001);
    assert!((base.y - (-0.3203505)).abs() < 0.0001);
    assert!((base.z - (-0.3607488)).abs() < 0.0001);
    let mut shifted = s.clone();
    shifted.object_offset = Vec3::new(16., 32., 5.);
    let moved = project_pixel(&shifted, &params(), 0., 0., 254).unwrap();
    let delta = moved - base;
    assert!(
        (delta.x - 1.).abs() < 1e-5 && (delta.y - 1.).abs() < 1e-5 && (delta.z - 2.).abs() < 1e-5
    );
    let mut flipped = s.clone();
    flipped.flip = true;
    let fp = project_pixel(&flipped, &params(), 0., 0., 254).unwrap();
    let rp = project_pixel(&s, &params(), 2., 0., 254).unwrap();
    assert!((fp - rp).length() < 1e-6);
}

// Catches counter bounds being measured before final correction and lost negative edge clamping.
#[test]
fn counter_fix_uses_nearest_border_pixel_and_final_bounds() {
    let mut points = [
        Vec3::new(0.39, 1., 2.),
        Vec3::new(0.45, 3., 4.),
        Vec3::new(-0.39, 5., 6.),
        Vec3::new(-0.8, 7., 8.),
    ];
    counter_correct(&mut points, &[(0, 0), (1, 0), (0, 10), (20, 10)], 0).unwrap();
    assert!((points[1].x - (0.39 + 1. / 71.55)).abs() < 1e-6);
    assert_eq!((points[1].y, points[1].z), (1., 2.));
    assert_eq!(points[3].x, -0.498);
    assert_eq!((points[3].y, points[3].z), (7., 8.));
    let bounds = Aabb::from_points(&points).unwrap();
    let contact = contact_translation(bounds, Vec3::new(10., 2., 20.)).unwrap();
    let translated = bounds
        .transformed(wonderland_render_core::Mat4::from_translation(contact))
        .unwrap();
    assert!((translated.min.y - 2.).abs() < 1e-6);
}

// Catches stale patched-source or parameter cache reuse and ambiguous byte concatenation.
#[test]
fn effective_content_and_all_derivation_choices_change_the_hash() {
    let s = sprite();
    let p = params();
    let o = ReconstructionOptions::default();
    let k = derivation_key(&[s.clone()], &p, 0, o, key(2), key(3)).unwrap();
    assert_ne!(
        k,
        derivation_key(&[s.clone()], &p, 0, o, key(2), key(4)).unwrap()
    );
    let mut q = p.clone();
    q.blender_tweak = true;
    assert_ne!(
        k,
        derivation_key(&[s.clone()], &q, 0, o, key(2), key(3)).unwrap()
    );
    let mut changed = s.clone();
    changed.rgba[0][0] = 254;
    assert_ne!(
        k,
        derivation_key(&[changed], &p, 0, o, key(2), key(3)).unwrap()
    );
    let mut changed = s.clone();
    changed.depth.as_mut().unwrap()[0] = 253;
    assert_ne!(
        k,
        derivation_key(&[changed], &p, 0, o, key(2), key(3)).unwrap()
    );
    let mut other = o;
    other.alpha = AlphaPolicy::DropTransparent;
    assert_ne!(
        k,
        derivation_key(&[s], &p, 0, other, key(2), key(3)).unwrap()
    );
}

// Catches unresolved simplification being silently reported as legacy parity.
#[test]
fn simplification_is_an_explicit_unavailable_derivation_stage() {
    let out = reconstruct(
        &[sprite()],
        &ReconstructionParams::default(),
        0,
        ReconstructionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        out.warnings,
        [ReconstructionWarning::SimplifierUnavailable {
            target_triangles: 0,
            iterations: 125,
            aggressiveness: 3.5
        }]
    );
    out.parts[0]
        .mesh
        .validate(&RenderLimits::default())
        .unwrap();
    let mut bad = sprite();
    bad.depth = Some(vec![0; 3]);
    assert!(reconstruct(&[bad], &params(), 0, ReconstructionOptions::default()).is_err());
}

// Catches override/cache precedence, corrupted candidates, old generated caches and patch changes.
#[test]
fn resolver_falls_through_corrupt_candidates_and_content_changes_evict_memory_identity() {
    let mut r = MeshResolver::new(10000, 4);
    let k = key(9);
    let mut c = Candidates::default();
    c.user_override = Some(candidate(1.));
    c.authored_override = Some(candidate(2.));
    c.embedded = Some(candidate(3.));
    let mut generated = candidate(4.);
    generated.reconstruction_version = 2;
    generated.cache_key = Some(k);
    c.generated_cache = Some(generated);
    assert_eq!(
        r.resolve(k, &c, || Ok(candidate(5.))).unwrap().source,
        ResolutionSource::UserOverride
    );
    assert_eq!(
        r.resolve(k, &c, || panic!("memory should resolve"))
            .unwrap()
            .source,
        ResolutionSource::Memory
    );
    c.user_override.as_mut().unwrap().mesh.indices[0] = 99;
    let out = r.resolve(k, &c, || Ok(candidate(5.))).unwrap();
    assert_eq!(out.source, ResolutionSource::AuthoredOverride);
    assert_eq!(out.rejected.len(), 1);
    c.user_override = None;
    c.authored_override = None;
    c.embedded = None;
    let out = r.resolve(k, &c, || Ok(candidate(5.))).unwrap();
    assert_eq!(out.source, ResolutionSource::GeneratedCache);
    r.clear_generated(k);
    assert_eq!(
        r.resolve(k, &c, || Ok(candidate(5.))).unwrap().source,
        ResolutionSource::Reconstructed
    );
    c.user_override = Some(candidate(6.));
    assert_eq!(
        r.resolve(k, &c, || Ok(candidate(5.))).unwrap().source,
        ResolutionSource::UserOverride
    );
    c.user_override = None;
    assert_eq!(
        r.resolve(key(10), &c, || Ok(candidate(7.))).unwrap().source,
        ResolutionSource::Reconstructed
    );
    assert!(r.resident_bytes() <= 10000);
}

// Catches accepting empty/corrupt meshes and treating authored version0 as stale generated data.
#[test]
fn resolver_rejects_empty_and_old_meshes_while_accepting_authored() {
    let mut r = MeshResolver::new(0, 0);
    let mut c = Candidates::default();
    let mut old = candidate(1.);
    old.reconstruction_version = 1;
    c.embedded = Some(old);
    let mut empty = candidate(2.);
    empty.mesh.indices.clear();
    c.user_override = Some(empty);
    let out = r.resolve(key(1), &c, || Ok(candidate(3.))).unwrap();
    assert_eq!(out.source, ResolutionSource::Reconstructed);
    assert_eq!(out.rejected.len(), 2);
    assert_eq!(r.resident_bytes(), 0);
    let mut bad = candidate(0.);
    bad.mesh.vertices[0].position.x = f32::NAN;
    assert!(r
        .resolve(key(2), &Candidates::default(), || Ok(bad))
        .is_err());
}

// Catches portal-last-group being hidden by dynamic bits or leaked into ordinary light maps.
#[test]
fn portal_depth_pass_order_and_dynamic_group_visibility_are_explicit() {
    assert_eq!(
        depth_passes(MaskType::Portal, 3, [0; 2], false).unwrap(),
        [
            DepthPass::ClearDepth0,
            DepthPass::ClearDepth1,
            DepthPass::Body(0),
            DepthPass::PortalFinal(2),
            DepthPass::ClearPortalStencil
        ]
    );
    assert!(depth_passes(MaskType::Portal, 3, [u64::MAX; 2], true)
        .unwrap()
        .is_empty());
    assert_eq!(
        depth_passes(MaskType::None, 3, [2, 0], false).unwrap(),
        [DepthPass::Body(0), DepthPass::Body(2)]
    );
    assert!(matches!(
        depth_passes(MaskType::None, 130, [0; 2], false),
        Err(Error::InvalidInput(_))
    ));
}

// Catches unbounded quadratic counter correction on adversarial depth images.
#[test]
fn counter_correction_declares_a_work_limit_before_modifying_vertices() {
    let mut p = vec![Vec3::new(0.39, 0., 0.); 5000];
    p.extend(vec![Vec3::new(0.8, 0., 0.); 5000]);
    let pixels: Vec<_> = (0..10000).map(|i| (i, 0)).collect();
    let before = p.clone();
    assert!(matches!(
        counter_correct(&mut p, &pixels, 0),
        Err(Error::BudgetExceeded(_))
    ));
    assert_eq!(p, before);
}

// Catches cache eviction violating the byte ceiling or reusing removed identities.
#[test]
fn resolver_lru_bound_evicts_the_least_recently_used_mesh() {
    let mut r = MeshResolver::new(300, 1);
    let c = Candidates::default();
    r.resolve(key(1), &c, || Ok(candidate(1.))).unwrap();
    r.resolve(key(2), &c, || Ok(candidate(2.))).unwrap();
    assert!(r.resident_bytes() <= 300);
    assert_eq!(
        r.resolve(key(1), &c, || Ok(candidate(3.))).unwrap().source,
        ResolutionSource::Reconstructed
    );
}
