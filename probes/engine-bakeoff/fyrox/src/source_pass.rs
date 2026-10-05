//! Draw the fixture in Fyrox's LDR target, after tone mapping and FXAA.
//!
//! The source renderer blends gamma-encoded, premultiplied sprite colors. The
//! Forward HDR path would tone-map both those colors and the object ID bytes.
use fyrox::{
    core::{algebra::Vector3, color::Color, ImmutableString},
    engine::GraphicsContext,
    graphics::error::FrameworkError,
    renderer::{
        bundle::{BundleRenderContext, SurfaceInstanceData},
        RenderPassStatistics, SceneRenderPass, SceneRenderPassContext,
    },
    scene::mesh::RenderPath,
};
use std::{
    any::TypeId,
    cell::RefCell,
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

pub const PASS_NAME: &str = "WonderlandSource";
pub type FrameCounter = Arc<AtomicU64>;

struct SourcePass {
    frames: FrameCounter,
}

impl SceneRenderPass for SourcePass {
    fn on_ldr_render(
        &mut self,
        ctx: SceneRenderPassContext,
    ) -> Result<RenderPassStatistics, FrameworkError> {
        ctx.framebuffer.clear(
            ctx.observer.viewport,
            Some(
                ctx.scene
                    .rendering_options
                    .clear_color
                    .unwrap_or(Color::BLACK),
            ),
            Some(1.0),
            None,
        );

        // Every fixture mesh gets a distinct material and is inserted in source
        // draw order. Handles are used only to recover that disposable render
        // order; they never become game identities or selection results.
        let mut bundles: Vec<_> = ctx
            .bundle_storage
            .bundles
            .iter()
            .filter(|bundle| bundle.render_path == RenderPath::Forward)
            .collect();
        bundles.sort_by_key(|bundle| {
            bundle
                .instances
                .iter()
                .map(|instance| instance.node_handle.index())
                .min()
                .unwrap_or(u32::MAX)
        });

        let pass_name = ImmutableString::new(PASS_NAME);
        let light_position = Vector3::zeros();
        let mut render_context = BundleRenderContext {
            texture_cache: ctx.texture_cache,
            render_pass_name: &pass_name,
            frame_buffer: ctx.framebuffer,
            viewport: ctx.observer.viewport,
            uniform_memory_allocator: ctx.uniform_memory_allocator,
            resource_manager: ctx.resource_manager,
            use_pom: false,
            light_position: &light_position,
            ambient_light: ctx.scene.rendering_options.ambient_lighting_color,
            scene_depth: Some(ctx.depth_texture),
            renderer_resources: ctx.renderer_resources,
        };
        let globals = ctx
            .bundle_storage
            .write_global_uniform_blocks(&mut render_context);
        let view_projection = ctx.bundle_storage.observer_position.projection_matrix
            * ctx.bundle_storage.observer_position.view_matrix;
        let uniforms: Vec<_> = bundles
            .iter()
            .map(|bundle| bundle.write_uniforms(&view_projection, &mut render_context))
            .collect();
        render_context.uniform_memory_allocator.upload(ctx.server)?;

        let mut statistics = RenderPassStatistics::default();
        let mut all_instances = |_: &SurfaceInstanceData| true;
        for (bundle, uniforms) in bundles.into_iter().zip(uniforms) {
            if let Some(uniforms) = uniforms {
                statistics += bundle.render_to_frame_buffer(
                    ctx.server,
                    ctx.geometry_cache,
                    ctx.shader_cache,
                    &mut all_instances,
                    &mut render_context,
                    uniforms,
                    &globals,
                )?;
            }
        }
        if statistics.draw_calls > 0 {
            self.frames.fetch_add(1, Ordering::Relaxed);
        }
        #[cfg(target_arch = "wasm32")]
        super::bridge::metrics(serde_json::json!({
            "fixtureDrawCalls": statistics.draw_calls,
            "fixtureTriangles": statistics.triangles_rendered,
            "fixtureRenderedFrames": self.frames.load(Ordering::Relaxed)
        }));
        Ok(statistics)
    }

    fn source_type_id(&self) -> TypeId {
        TypeId::of::<super::Game>()
    }
}

pub fn install(context: &mut GraphicsContext, frames: &FrameCounter) {
    let GraphicsContext::Initialized(graphics) = context else {
        return;
    };
    if !graphics
        .renderer
        .render_passes()
        .iter()
        .any(|pass| pass.borrow().source_type_id() == TypeId::of::<super::Game>())
    {
        graphics
            .renderer
            .add_render_pass(Rc::new(RefCell::new(SourcePass {
                frames: frames.clone(),
            })));
    }
}
