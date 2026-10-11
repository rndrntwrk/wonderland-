//! Differential fragment traces against the unchanged source-derived object
//! command interpreter. This does not claim original GPU or texture-filter parity.
use super::*;
use wonderland_render_core::{RenderLimits, reference as cpu};
#[path = "../../examples/support/masked.rs"]
mod fixture;

fn vertices(counterclockwise: bool, depth: f32, color: [u8; 4]) -> [cpu::RasterVertex; 3] {
    let mut result = [(0., 0.), (2., 0.), (0., 2.)].map(|(x, y)| cpu::RasterVertex {
        position: Vec3::new(x, y, depth),
        reciprocal_w: 1.,
        color: color.map(|v| f32::from(v) / 255.),
    });
    if counterclockwise {
        result.swap(1, 2);
    }
    result
}

#[test]
fn ordered_material_adapter_matches_source_fragment_oracle() {
    let mut cases = 0;
    for kind in [ModelMaskKind::Normal, ModelMaskKind::Portal] {
        let document = fixture::masked(kind);
        let model = prepare_model(&document.models[0]).unwrap();
        let scene = model
            .scene(
                source_objects::ObjectInstance {
                    entity: document.objects[0].entity.unwrap(),
                    visual_revision: 1,
                    position_tiles: document.objects[0].position_tiles,
                    yaw_radians: 0.,
                    dynamic_flags: [0, 0],
                    room: 0,
                    level: 1,
                    directional_lighting: false,
                },
                source_objects::ObjectTarget::Color,
            )
            .unwrap();
        for (ordinal, draw) in scene.draws.iter().enumerate() {
            for initial_stencil in [0, 1, 3] {
                for initial_depth in [0., 0.4, 1.] {
                    for depth in [0., 0.4, 0.8, 1.] {
                        for ccw in [false, true] {
                            for alpha in [0, 1, 2, 3, 64, 128, 255] {
                                // Mask shaders produce opaque output without sampling.
                                if draw.pipeline.blend == source_objects::ObjectBlend::NoColor
                                    && alpha != 255
                                {
                                    continue;
                                }
                                let mut reference = source_objects::DepthStencilPixel {
                                    depth: initial_depth,
                                    stencil: initial_stencil,
                                };
                                let accepted = draw
                                    .apply_depth_stencil(
                                        &mut reference,
                                        depth,
                                        ccw,
                                        f32::from(alpha) / 255.,
                                    )
                                    .unwrap();
                                let mut surface =
                                    cpu::ReferenceSurface::new(2, 2, &RenderLimits::default())
                                        .unwrap();
                                let before = [11, 33, 55, 255];
                                let old = Some(EntityRef {
                                    object_id: 1,
                                    generation: 1,
                                });
                                let next = Some(EntityRef {
                                    object_id: 2,
                                    generation: 1,
                                });
                                surface
                                    .write_fragment(
                                        0,
                                        0,
                                        initial_depth,
                                        before,
                                        old,
                                        Default::default(),
                                    )
                                    .unwrap();
                                let face = cpu::StencilFace {
                                    compare: cpu::StencilComparison::Always,
                                    pass: cpu::StencilOperation::Replace,
                                    fail: cpu::StencilOperation::Keep,
                                    depth_fail: cpu::StencilOperation::Keep,
                                };
                                surface
                                    .set_pipeline(Some(cpu::FragmentPipeline {
                                        depth_compare: cpu::DepthComparison::Always,
                                        depth_write: false,
                                        forced_depth: None,
                                        stencil: Some(cpu::StencilState {
                                            reference: initial_stencil,
                                            clockwise: face,
                                            counterclockwise: face,
                                        }),
                                        blend: cpu::FragmentBlend::NoColor,
                                    }))
                                    .unwrap();
                                surface
                                    .write_fragment(
                                        0,
                                        0,
                                        initial_depth,
                                        [255; 4],
                                        old,
                                        Default::default(),
                                    )
                                    .unwrap();
                                surface
                                    .set_pipeline(Some(crate::materials::pipeline(draw.pipeline)))
                                    .unwrap();
                                let color = [200, 40, 10, alpha];
                                let touched = surface
                                    .draw_triangle(
                                        vertices(ccw, depth, color),
                                        next,
                                        cpu::FragmentOptions {
                                            alpha_cutoff: 2,
                                            ..Default::default()
                                        },
                                    )
                                    .unwrap();
                                let label = format!(
                                    "{kind:?} pass {ordinal}, stencil {initial_stencil}, depth {initial_depth}/{depth}, ccw {ccw}, alpha {alpha}"
                                );
                                assert_eq!(touched > 0, accepted, "{label}");
                                assert_eq!(
                                    surface.depth_at(0, 0),
                                    Some(reference.depth),
                                    "{label}"
                                );
                                assert_eq!(
                                    surface.stencil_at(0, 0),
                                    Some(reference.stencil),
                                    "{label}"
                                );
                                let writes = accepted
                                    && draw.pipeline.blend != source_objects::ObjectBlend::NoColor;
                                assert_eq!(
                                    surface.id_at(0, 0),
                                    if writes { next } else { old },
                                    "{label}"
                                );
                                let expected = if writes {
                                    draw.pipeline
                                        .blend
                                        .blend_rgba(
                                            color.map(|v| f32::from(v) / 255.),
                                            before.map(|v| f32::from(v) / 255.),
                                        )
                                        .unwrap()
                                        .map(|v| (v * 255.).round() as u8)
                                } else {
                                    before
                                };
                                assert_eq!(surface.pixel(0, 0), Some(expected), "{label}");
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 1872);
}
