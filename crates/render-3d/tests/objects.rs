use wonderland_render_3d::objects::*;
use wonderland_render_3d::reconstruction::MaskType;
use wonderland_render_core::{Aabb, AssetKey, EntityRef, RgbaImage, Vec2, Vec3};

fn identity(n: u8) -> ObjectMeshIdentity {
    ObjectMeshIdentity {
        effective_source: AssetKey([n; 32]),
        effective_content: AssetKey([2; 32]),
    }
}
fn triangle() -> NormalizedGeometry {
    NormalizedGeometry {
        vertices: [Vec3::ZERO, Vec3::new(2., 0., 0.), Vec3::new(2., 0., 1.)]
            .into_iter()
            .map(|position| FsomVertex {
                position,
                uv: Vec2::new(0.75, 0.5),
                normal: Vec3::new(0., -2., 0.),
            })
            .collect(),
        indices: vec![0, 1, 2],
    }
}
fn texture(selector: TextureSelector, n: u8) -> NormalizedTexture {
    NormalizedTexture {
        selector,
        effective_asset: AssetKey([n; 32]),
        uv_scale: Vec2::new(0.5, 0.75),
        image: RgbaImage {
            width: 1,
            height: 1,
            pixels: vec![[20, 40, 80, 128]],
        },
    }
}
fn fixture() -> NormalizedFsom {
    NormalizedFsom {
        identity: identity(1),
        context: FsomContext::Dgrp {
            effective_iff: AssetKey([3; 32]),
            chunk_id: 200,
        },
        format_version: 3,
        reconstruction_version: 0,
        groups: vec![vec![NormalizedPart {
            texture: 0,
            geometry: triangle(),
        }]],
        textures: vec![texture(
            TextureSelector::Sprite {
                rotation: 2,
                ordinal: 7,
            },
            4,
        )],
        bounds: Aabb::new(Vec3::ZERO, Vec3::new(2., 1., 1.)).unwrap(),
        depth_mask: None,
    }
}
fn prepare(source: NormalizedFsom) -> PreparedFsom {
    PreparedFsom::prepare(source, ObjectLimits::default()).unwrap()
}
fn instance() -> ObjectInstance {
    ObjectInstance {
        entity: EntityRef {
            object_id: 27,
            generation: 2,
        },
        visual_revision: 13,
        position_tiles: Vec3::new(2., 3., 4.),
        yaw_radians: 0.,
        dynamic_flags: [0, 0],
        room: 4,
        level: 2,
        directional_lighting: true,
    }
}

#[test]
fn dynamic_groups_preserve_both_64_bit_banks_and_static_group() {
    let mut source = fixture();
    source.groups.resize(
        129,
        vec![NormalizedPart {
            texture: 0,
            geometry: triangle(),
        }],
    );
    let prepared = prepare(source);
    let mut visual = instance();
    visual.dynamic_flags = [1u64 << 63, (1u64 << 63) | 1];
    let scene = prepared.scene(visual, ObjectTarget::Color).unwrap();
    assert_eq!(
        scene
            .draws
            .iter()
            .map(|d| d.group_part.unwrap().0)
            .collect::<Vec<_>>(),
        [0, 64, 65, 128]
    );
    assert!(scene
        .draws
        .iter()
        .all(|d| d.shader == ObjectShader::Directional));
}

#[test]
fn source_world_transform_applies_mesh_scale_yaw_and_tile_center_once() {
    let prepared = prepare(fixture());
    let mut visual = instance();
    visual.yaw_radians = std::f32::consts::FRAC_PI_2;
    let scene = prepared.scene(visual, ObjectTarget::Color).unwrap();
    // Source: scale 3, rotate Y by -pi/2, add (3*2+1.5, 3*4+0.1, 3*3+1.5).
    let point = scene.world.transform_point3(Vec3::new(1., 2., 0.));
    assert!((point.x - 7.5).abs() < 0.00001);
    assert!((point.y - 18.1).abs() < 0.00001);
    assert!((point.z - 13.5).abs() < 0.00001);
    assert_eq!(
        scene.entity,
        EntityRef {
            object_id: 27,
            generation: 2
        }
    );
    assert_eq!(scene.visual_revision, 13);
    assert!((scene.shader_level - 1.001).abs() < 0.000001);
    assert_eq!(
        scene.draws[0].material.unwrap().uv_scale(),
        Vec2::new(0.5, 0.75)
    );
}

#[test]
fn normal_mask_executes_source_depth_writes_and_stencil_clear() {
    let mut source = fixture();
    source.depth_mask = Some(NormalizedDepthMask {
        kind: MaskType::Normal,
        geometry: triangle(),
    });
    let prepared = prepare(source);
    let scene = prepared.scene(instance(), ObjectTarget::Color).unwrap();
    assert_eq!(
        scene.draws.iter().map(|d| d.shader).collect::<Vec<_>>(),
        [
            ObjectShader::MaskMark,
            ObjectShader::MaskFar,
            ObjectShader::Directional
        ]
    );
    let mut pixel = DepthStencilPixel {
        depth: 0.8,
        stencil: 0,
    };
    assert!(scene.draws[0]
        .apply_depth_stencil(&mut pixel, 0.4, true, 1.)
        .unwrap());
    // Actual DepthClear1 initializer writes depth, despite a contradictory comment.
    assert_eq!(
        pixel,
        DepthStencilPixel {
            depth: 0.4,
            stencil: 1
        }
    );
    assert!(scene.draws[1]
        .apply_depth_stencil(&mut pixel, 0.4, true, 1.)
        .unwrap());
    assert_eq!(
        pixel,
        DepthStencilPixel {
            depth: 1.,
            stencil: 0
        }
    );
    assert!(scene.draws[2]
        .apply_depth_stencil(&mut pixel, 0.7, false, 1.)
        .unwrap());
    assert_eq!(pixel.depth, 0.7);
    let mut occluded = DepthStencilPixel {
        depth: 0.2,
        stencil: 0,
    };
    assert!(!scene.draws[0]
        .apply_depth_stencil(&mut occluded, 0.4, true, 1.)
        .unwrap());
    assert!(!scene.draws[1]
        .apply_depth_stencil(&mut occluded, 0.4, true, 1.)
        .unwrap());
    assert_eq!(
        occluded,
        DepthStencilPixel {
            depth: 0.2,
            stencil: 0
        }
    );
}

#[test]
fn portal_final_group_is_forced_stencil_restricted_and_cleaned_after_drawing() {
    let mut source = fixture();
    source.groups.push(vec![NormalizedPart {
        texture: 0,
        geometry: triangle(),
    }]);
    source.depth_mask = Some(NormalizedDepthMask {
        kind: MaskType::Portal,
        geometry: triangle(),
    });
    let prepared = prepare(source);
    let scene = prepared.scene(instance(), ObjectTarget::Color).unwrap();
    assert_eq!(
        scene
            .draws
            .iter()
            .map(|d| d.group_part.map(|p| p.0))
            .collect::<Vec<_>>(),
        [None, None, Some(0), Some(1), None]
    );
    let mut pixel = DepthStencilPixel {
        depth: 0.8,
        stencil: 0,
    };
    scene.draws[0]
        .apply_depth_stencil(&mut pixel, 0.4, true, 1.)
        .unwrap();
    scene.draws[1]
        .apply_depth_stencil(&mut pixel, 0.4, true, 1.)
        .unwrap();
    assert_eq!(
        pixel,
        DepthStencilPixel {
            depth: 1.,
            stencil: 1
        }
    );
    assert!(scene.draws[3]
        .apply_depth_stencil(&mut pixel, 0.6, false, 1.)
        .unwrap());
    assert_eq!(
        pixel,
        DepthStencilPixel {
            depth: 0.6,
            stencil: 1
        }
    );
    scene.draws[4]
        .apply_depth_stencil(&mut pixel, 0.4, true, 1.)
        .unwrap();
    assert_eq!(
        pixel,
        DepthStencilPixel {
            depth: 0.6,
            stencil: 0
        }
    );
    assert!(!scene.draws[3]
        .apply_depth_stencil(&mut pixel, 0.5, false, 1.)
        .unwrap());
}

#[test]
fn depth_mark_respects_clockwise_face_and_source_less_equal_rule() {
    let mut source = fixture();
    source.depth_mask = Some(NormalizedDepthMask {
        kind: MaskType::Normal,
        geometry: triangle(),
    });
    let prepared = prepare(source);
    let scene = prepared.scene(instance(), ObjectTarget::Color).unwrap();
    let mut pixel = DepthStencilPixel {
        depth: 0.4,
        stencil: 1,
    };
    assert!(scene.draws[0]
        .apply_depth_stencil(&mut pixel, 0.4, false, 1.)
        .unwrap());
    assert_eq!(
        pixel,
        DepthStencilPixel {
            depth: 0.4,
            stencil: 0
        }
    );
}

#[test]
fn disabled_room_uses_source_grayscale_shader_without_regular_alpha_discard() {
    let prepared = prepare(fixture());
    let ordinary = prepared.scene(instance(), ObjectTarget::Color).unwrap();
    let mut pixel = DepthStencilPixel {
        depth: 1.,
        stencil: 0,
    };
    assert!(!ordinary.draws[0]
        .apply_depth_stencil(&mut pixel, 0.5, false, 0.009)
        .unwrap());
    assert!(ordinary.draws[0]
        .apply_depth_stencil(&mut pixel, 0.5, false, 0.01)
        .unwrap());
    let mut visual = instance();
    visual.room = 65533;
    let disabled = prepared.scene(visual, ObjectTarget::Color).unwrap();
    assert_eq!(disabled.draws[0].shader, ObjectShader::Disabled);
    assert!(disabled.draws[0]
        .apply_depth_stencil(&mut pixel, 0.4, false, 0.)
        .unwrap());
    visual.room = 65534;
    assert_eq!(
        prepared.scene(visual, ObjectTarget::Color).unwrap().draws[0].shader,
        ObjectShader::Basic
    );
}

#[test]
fn lightmap_uses_source_y_override_and_skips_portals() {
    let mut source = fixture();
    source.depth_mask = Some(NormalizedDepthMask {
        kind: MaskType::Normal,
        geometry: triangle(),
    });
    let prepared = prepare(source.clone());
    let scene = prepared
        .scene(
            instance(),
            ObjectTarget::Lightmap {
                level: 0,
                y_offset: 0.25,
            },
        )
        .unwrap();
    assert_eq!(scene.draws.len(), 1);
    assert_eq!(scene.draws[0].shader, ObjectShader::Lightmap);
    assert!((scene.world.cols[3][1] - 3.2).abs() < 0.000001);
    source.depth_mask.as_mut().unwrap().kind = MaskType::Portal;
    assert!(prepare(source)
        .scene(
            instance(),
            ObjectTarget::Lightmap {
                level: 0,
                y_offset: 0.
            }
        )
        .unwrap()
        .draws
        .is_empty());
}

#[test]
fn invalid_instance_or_fragment_cannot_emit_partial_draws_or_mutate_depth() {
    let prepared = prepare(fixture());
    let mut visual = instance();
    visual.position_tiles.x = f32::MAX;
    assert!(prepared.scene(visual, ObjectTarget::Color).is_err());
    visual = instance();
    visual.yaw_radians = f32::NAN;
    assert!(prepared.scene(visual, ObjectTarget::Color).is_err());
    let scene = prepared.scene(instance(), ObjectTarget::Color).unwrap();
    let mut pixel = DepthStencilPixel {
        depth: 0.5,
        stencil: 0,
    };
    assert!(scene.draws[0]
        .apply_depth_stencil(&mut pixel, f32::NAN, false, 1.)
        .is_err());
    assert_eq!(
        pixel,
        DepthStencilPixel {
            depth: 0.5,
            stencil: 0
        }
    );
}

#[test]
fn preserves_group_order_source_coordinates_and_resolved_material_binding() {
    let mut source = fixture();
    source
        .textures
        .push(texture(TextureSelector::Custom { id: 51 }, 5));
    source.groups[0].push(NormalizedPart {
        texture: 1,
        geometry: triangle(),
    });
    source.groups.push(Vec::new());
    source.groups.push(vec![NormalizedPart {
        texture: 1,
        geometry: triangle(),
    }]);
    let result = prepare(source);
    assert_eq!(
        result.groups().iter().map(Vec::len).collect::<Vec<_>>(),
        [2, 0, 1]
    );
    assert_eq!(result.groups()[0][1].texture_index(), 1);
    assert_eq!(
        result.textures()[1].selector(),
        TextureSelector::Custom { id: 51 }
    );
    assert_eq!(result.textures()[0].effective_asset(), AssetKey([4; 32]));
    assert_eq!(result.textures()[0].uv_scale(), Vec2::new(0.5, 0.75));
    let vertex = result.groups()[0][0].mesh().vertices[1];
    assert_eq!(vertex.position, Vec3::new(2., 0., 0.));
    assert_eq!(vertex.uv, Vec2::new(0.75, 0.5));
    assert_eq!(vertex.normal, Vec3::new(0., -2., 0.));
    assert_eq!(vertex.color, [1.; 4]);
    assert_eq!(result.groups()[0][0].mesh().indices, [0, 1, 2]);
}

#[test]
fn v1_normals_use_original_area_weighted_cross_product_and_normalization() {
    let mut source = fixture();
    source.format_version = 1;
    for v in &mut source.groups[0][0].geometry.vertices {
        v.normal = Vec3::ZERO;
    }
    let result = prepare(source);
    for vertex in &result.groups()[0][0].mesh().vertices {
        // (2,0,0) cross (0,0,1) = (0,-2,0), normalized = (0,-1,0).
        assert_eq!(vertex.normal, Vec3::new(0., -1., 0.));
    }
}

#[test]
fn portal_mask_can_extend_beyond_preserved_body_bounds() {
    let mut source = fixture();
    let mut mask = triangle();
    mask.vertices[0].position = Vec3::new(-50., 0., 0.);
    source.depth_mask = Some(NormalizedDepthMask {
        kind: MaskType::Portal,
        geometry: mask,
    });
    let result = prepare(source);
    assert_eq!(result.bounds().min, Vec3::ZERO);
    assert_eq!(result.depth_mask().unwrap().1.vertices[0].position.x, -50.);
}

#[test]
fn rejects_corrupt_versions_indices_materials_bounds_and_nonfinite_inputs() {
    let edits: [fn(&mut NormalizedFsom); 11] = [
        |v| v.format_version = 4,
        |v| v.reconstruction_version = 1,
        |v| v.reconstruction_version = u32::MAX,
        |v| v.groups[0][0].geometry.indices[2] = 3,
        |v| v.groups[0][0].texture = 1,
        |v| v.groups[0][0].geometry.vertices[0].position.x = f32::NAN,
        |v| v.textures[0].uv_scale.x = f32::INFINITY,
        |v| v.bounds.max.x = 1.,
        |v| {
            v.textures[0].selector = TextureSelector::Sprite {
                rotation: 4,
                ordinal: 0,
            }
        },
        |v| v.context = FsomContext::Standalone,
        |v| v.textures[0].image.pixels.clear(),
    ];
    for edit in edits {
        let mut source = fixture();
        edit(&mut source);
        assert!(PreparedFsom::prepare(source, ObjectLimits::default()).is_err());
    }
}

#[test]
fn aggregate_mesh_and_texture_budgets_include_every_group_and_mask() {
    let mut source = fixture();
    source.groups.push(vec![NormalizedPart {
        texture: 0,
        geometry: triangle(),
    }]);
    let mut limits = ObjectLimits::default();
    limits.render.max_vertices = 5;
    assert_eq!(
        PreparedFsom::prepare(source.clone(), limits).unwrap_err(),
        ObjectError::Limit("object vertices")
    );
    limits = ObjectLimits::default();
    limits.max_buffer_bytes = 315;
    // Two 3-vertex/3-index parts (2 * (3*48 + 3*4)) and one RGBA pixel = 316.
    assert_eq!(
        PreparedFsom::prepare(source, limits).unwrap_err(),
        ObjectError::Limit("object buffers")
    );
    let mut source = fixture();
    source.depth_mask = Some(NormalizedDepthMask {
        kind: MaskType::Normal,
        geometry: triangle(),
    });
    limits = ObjectLimits::default();
    limits.render.max_indices = 5;
    assert_eq!(
        PreparedFsom::prepare(source, limits).unwrap_err(),
        ObjectError::Limit("object indices")
    );
    let mut source = fixture();
    source
        .textures
        .push(texture(TextureSelector::Custom { id: 51 }, 5));
    limits = ObjectLimits::default();
    limits.max_total_texture_pixels = 1;
    // Each one-pixel image fits separately; the complete texture table does not.
    assert_eq!(
        PreparedFsom::prepare(source, limits).unwrap_err(),
        ObjectError::Limit("object texture pixels")
    );
}

#[test]
fn content_key_tracks_actual_buffers_binding_layout_and_effective_content() {
    let base = prepare(fixture()).key();
    let edits: [fn(&mut NormalizedFsom); 8] = [
        |v| v.identity.effective_content = AssetKey([9; 32]),
        |v| v.identity.effective_source = AssetKey([8; 32]),
        |v| v.textures[0].image.pixels[0][0] = 21,
        |v| v.textures[0].effective_asset = AssetKey([7; 32]),
        |v| v.textures[0].uv_scale.x = 0.25,
        |v| v.groups.push(Vec::new()),
        |v| v.groups[0][0].geometry.indices.swap(1, 2),
        |v| {
            v.depth_mask = Some(NormalizedDepthMask {
                kind: MaskType::Normal,
                geometry: triangle(),
            })
        },
    ];
    for edit in edits {
        let mut source = fixture();
        edit(&mut source);
        assert_ne!(base, prepare(source).key());
    }
}

#[test]
fn malformed_replacement_preserves_installed_identity_generation_and_geometry() {
    let mut slot = ObjectMeshSlot::new(7, instance().entity, ObjectLimits::default()).unwrap();
    let first = slot.request(identity(1), 2).unwrap();
    let first_key = slot.install(first, fixture()).unwrap();
    let next = slot.request(identity(9), 3).unwrap();
    let mut malformed = fixture();
    malformed.identity = identity(9);
    malformed.groups[0][0].geometry.indices[1] = 99;
    assert!(slot.install(next, malformed).is_err());
    assert_eq!(slot.current().unwrap().ticket(), first);
    assert_eq!(slot.current().unwrap().mesh().key(), first_key);
    assert_eq!(slot.current().unwrap().ticket().content_generation(), 2);
    assert_eq!(slot.pending(), Some(next));
    let mut corrected = fixture();
    corrected.identity = identity(9);
    assert!(slot.install(next, corrected).is_ok());
    assert_eq!(slot.current().unwrap().ticket().content_generation(), 3);
    assert_eq!(slot.pending(), None);
}

#[test]
fn request_serial_prevents_a_b_a_completion_revival() {
    let mut slot = ObjectMeshSlot::new(7, instance().entity, ObjectLimits::default()).unwrap();
    let old_a = slot.request(identity(1), 2).unwrap();
    slot.request(identity(9), 3).unwrap();
    let new_a = slot.request(identity(1), 4).unwrap();
    assert_eq!(
        slot.install(old_a, fixture()).unwrap_err(),
        ObjectError::StaleRequest
    );
    assert!(slot.current().is_none());
    assert!(slot.install(new_a, fixture()).is_ok());
    assert_eq!(
        slot.install(new_a, fixture()).unwrap_err(),
        ObjectError::StaleRequest
    );
    assert_eq!(
        slot.request(identity(1), 3).unwrap_err(),
        ObjectError::StaleRequest
    );
}

#[test]
fn reset_entity_and_session_fences_reject_old_completions() {
    let mut slot = ObjectMeshSlot::new(7, instance().entity, ObjectLimits::default()).unwrap();
    let old = slot.request(identity(1), 2).unwrap();
    slot.reset(instance().entity).unwrap();
    let fresh = slot.request(identity(1), 2).unwrap();
    assert_ne!(old, fresh);
    assert_eq!(
        slot.install(old, fixture()).unwrap_err(),
        ObjectError::StaleRequest
    );
    let mut other_session =
        ObjectMeshSlot::new(8, instance().entity, ObjectLimits::default()).unwrap();
    other_session.request(identity(1), 2).unwrap();
    assert_eq!(
        other_session.install(fresh, fixture()).unwrap_err(),
        ObjectError::StaleRequest
    );
    slot.reset(EntityRef {
        object_id: 27,
        generation: 3,
    })
    .unwrap();
    let next_entity = slot.request(identity(1), 2).unwrap();
    assert_eq!(
        slot.install(fresh, fixture()).unwrap_err(),
        ObjectError::StaleRequest
    );
    assert_eq!(next_entity.entity().generation, 3);
}

#[test]
fn result_with_wrong_effective_content_cannot_satisfy_a_ticket() {
    let mut slot = ObjectMeshSlot::new(7, instance().entity, ObjectLimits::default()).unwrap();
    let ticket = slot.request(identity(1), 2).unwrap();
    let mut wrong = fixture();
    wrong.identity.effective_content = AssetKey([10; 32]);
    assert_eq!(
        slot.install(ticket, wrong).unwrap_err(),
        ObjectError::StaleRequest
    );
    assert!(slot.current().is_none());
    assert_eq!(slot.pending(), Some(ticket));
}

#[test]
fn non_axis_v1_normal_bits_match_the_executed_original_dgrp3dvert() {
    let mut source = fixture();
    source.format_version = 1;
    source.bounds.max = Vec3::new(2., 3., 1.);
    source.groups[0][0].geometry.vertices.push(FsomVertex {
        position: Vec3::new(0., 3., 1.),
        uv: Vec2::ZERO,
        normal: Vec3::ZERO,
    });
    source.groups[0][0].geometry.indices.extend([0, 2, 3]);
    let prepared = prepare(source);
    // Literal uint bits emitted by unchanged DGRP3DVert.GenerateNormals(false)
    // linked to the repository's MonoGame assembly; not computed by this adapter.
    let source_bits = [
        [3200559654, 3204652057, 1061464614],
        [0, 3212836864, 0],
        [3200559654, 3204652057, 1061464614],
        [3202051512, 3197258021, 1062956472],
    ];
    for (v, expected) in prepared.groups()[0][0]
        .mesh()
        .vertices
        .iter()
        .zip(source_bits)
    {
        assert_eq!(
            [
                v.normal.x.to_bits(),
                v.normal.y.to_bits(),
                v.normal.z.to_bits()
            ],
            expected
        );
    }
}

#[test]
fn source_nonpremultiplied_blend_uses_source_alpha_for_alpha_too() {
    let prepared = prepare(fixture());
    let scene = prepared.scene(instance(), ObjectTarget::Color).unwrap();
    let blended = scene.draws[0]
        .pipeline
        .blend
        .blend_rgba([1., 0., 0., 0.5], [0., 0., 1., 0.75])
        .unwrap();
    // Executed MonoGame state: SourceAlpha/InverseSourceAlpha for RGB and alpha.
    // Alpha = 0.5*0.5 + 0.75*0.5 = 0.625 (not ordinary alpha-over's 0.875).
    assert_eq!(blended, [0.5, 0., 0.5, 0.625]);
    assert_eq!(
        ObjectBlend::NoColor.blend_rgba([1.; 4], [0.25; 4]).unwrap(),
        [0.25; 4]
    );
    assert_eq!(
        ObjectBlend::MaxGreen
            .blend_rgba([1., 0.8, 0., 0.2], [0.3, 0.1, 0.9, 0.7])
            .unwrap(),
        [0.3, 0.8, 0.9, 0.7]
    );
}

#[test]
fn shader_scales_uv_before_the_linear_clamp_sampler() {
    let prepared = prepare(fixture());
    let texture = &prepared.textures()[0];
    assert_eq!(
        texture.shader_uv(Vec2::new(0.75, 0.5)).unwrap(),
        Vec2::new(0.375, 0.375)
    );
    // Clamp is a sampler operation, not a premature per-vertex clamp.
    assert_eq!(
        texture.shader_uv(Vec2::new(4., -2.)).unwrap(),
        Vec2::new(2., -1.5)
    );
    assert_eq!(texture.sampling(), TextureSampling::LinearClampMipmaps);
    assert!(texture.shader_uv(Vec2::new(f32::NAN, 0.)).is_err());
}

#[test]
fn finite_extreme_body_mask_and_normal_values_cannot_overflow_scene_outputs() {
    let mut normal = fixture();
    normal.groups[0][0].geometry.vertices[0].normal.x = f32::MAX;
    assert!(prepare(normal)
        .scene(instance(), ObjectTarget::Color)
        .is_err());
    let mut body = fixture();
    body.bounds.max.x = f32::MAX;
    body.groups[0][0].geometry.vertices[1].position.x = f32::MAX;
    assert!(prepare(body)
        .scene(instance(), ObjectTarget::Color)
        .is_err());
    let mut mask = fixture();
    let mut geometry = triangle();
    geometry.vertices[1].position.x = f32::MAX;
    mask.depth_mask = Some(NormalizedDepthMask {
        kind: MaskType::Portal,
        geometry,
    });
    assert!(prepare(mask)
        .scene(instance(), ObjectTarget::Color)
        .is_err());
}

#[test]
fn masks_execute_without_body_groups_and_empty_portal_final_group_is_not_replaced() {
    let mut source = fixture();
    source.groups.clear();
    source.depth_mask = Some(NormalizedDepthMask {
        kind: MaskType::Portal,
        geometry: triangle(),
    });
    let prepared = prepare(source.clone());
    assert_eq!(
        prepared
            .scene(instance(), ObjectTarget::Color)
            .unwrap()
            .draws
            .len(),
        3
    );
    source.groups.push(vec![NormalizedPart {
        texture: 0,
        geometry: triangle(),
    }]);
    source.groups.push(Vec::new());
    let prepared = prepare(source);
    let scene = prepared.scene(instance(), ObjectTarget::Color).unwrap();
    assert_eq!(
        scene
            .draws
            .iter()
            .filter_map(|d| d.group_part)
            .collect::<Vec<_>>(),
        [(0, 0)]
    );
    // Only the literal last group is portal-restricted, never the last nonempty one.
    assert!(scene.draws[2].pipeline.stencil.is_none());
}

#[test]
fn lightmap_signed_level_extremes_are_widened_before_subtraction() {
    let prepared = prepare(fixture());
    let mut visual = instance();
    visual.level = -128;
    let scene = prepared
        .scene(
            visual,
            ObjectTarget::Lightmap {
                level: 127,
                y_offset: 0.,
            },
        )
        .unwrap();
    assert_eq!(scene.world.cols[3][1], -755.2);
}

#[test]
fn unsupported_masks_duplicate_bindings_and_degenerate_legacy_normals_reject() {
    let edits: [fn(&mut NormalizedFsom); 5] = [
        |v| {
            v.format_version = 2;
            v.depth_mask = Some(NormalizedDepthMask {
                kind: MaskType::Normal,
                geometry: triangle(),
            });
        },
        |v| {
            v.depth_mask = Some(NormalizedDepthMask {
                kind: MaskType::None,
                geometry: triangle(),
            })
        },
        |v| v.textures.push(v.textures[0].clone()),
        |v| {
            let duplicate = v.groups[0][0].clone();
            v.groups[0].push(duplicate);
        },
        |v| {
            v.format_version = 1;
            v.groups[0][0].geometry.indices = vec![0, 0, 0];
        },
    ];
    for edit in edits {
        let mut source = fixture();
        edit(&mut source);
        assert!(PreparedFsom::prepare(source, ObjectLimits::default()).is_err());
    }
}

#[test]
fn lightmap_shadow_target_has_no_depth_attachment_so_all_fragments_reach_max_blend() {
    let prepared = prepare(fixture());
    let scene = prepared
        .scene(
            instance(),
            ObjectTarget::Lightmap {
                level: 0,
                y_offset: 0.,
            },
        )
        .unwrap();
    let mut pixel = DepthStencilPixel {
        depth: 0.2,
        stencil: 0,
    };
    assert!(scene.draws[0]
        .apply_depth_stencil(&mut pixel, 0.8, false, 1.)
        .unwrap());
    assert!(scene.draws[0]
        .apply_depth_stencil(&mut pixel, 0.9, false, 1.)
        .unwrap());
    assert_eq!(
        pixel,
        DepthStencilPixel {
            depth: 0.2,
            stencil: 0
        }
    );
}
