//! Map source object commands, retaining the original order and state exactly.
use wonderland_render_3d::objects as source;
use wonderland_render_core::reference as target;

pub(crate) fn pipeline(value: source::ObjectPipeline) -> target::FragmentPipeline {
    let op = |value| match value {
        source::StencilOperation::Keep => target::StencilOperation::Keep,
        source::StencilOperation::Zero => target::StencilOperation::Zero,
        source::StencilOperation::Replace => target::StencilOperation::Replace,
    };
    let face = |value: source::StencilFace| target::StencilFace {
        compare: match value.compare {
            source::StencilComparison::Always => target::StencilComparison::Always,
            source::StencilComparison::Equal => target::StencilComparison::Equal,
        },
        pass: op(value.pass),
        fail: op(value.fail),
        depth_fail: op(value.depth_fail),
    };
    target::FragmentPipeline {
        depth_compare: match value.depth_compare {
            source::DepthComparison::LessEqual => target::DepthComparison::LessEqual,
            source::DepthComparison::Always => target::DepthComparison::Always,
        },
        depth_write: value.depth_write,
        forced_depth: value.forced_depth,
        stencil: value.stencil.map(|state| target::StencilState {
            reference: state.reference,
            clockwise: face(state.clockwise),
            counterclockwise: face(state.counterclockwise),
        }),
        blend: match value.blend {
            source::ObjectBlend::NoColor => target::FragmentBlend::NoColor,
            source::ObjectBlend::NonPremultiplied => target::FragmentBlend::NonPremultiplied,
            // This adapter consumes ObjectTarget::Color only. Lightmap drawing
            // is a distinct shader and surface, never silently a color command.
            source::ObjectBlend::MaxGreen => {
                unreachable!("color source cannot emit lightmap blending")
            }
        },
    }
}
