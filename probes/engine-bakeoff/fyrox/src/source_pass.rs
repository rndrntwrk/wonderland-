//! Draw the fixture in Fyrox's LDR target, after tone mapping and FXAA.
//!
//! The source renderer blends gamma-encoded, premultiplied sprite colors. The
//! Forward HDR path would tone-map both those colors and the object ID bytes.
use fyrox::{
    core::{
        algebra::{Vector3, Vector4},
        color::Color,
        instant::Instant,
        math::Rect,
        ImmutableString,
    },
    engine::GraphicsContext,
    graphics::{
        error::FrameworkError,
        framebuffer::{Attachment, GpuFrameBuffer},
        gpu_texture::PixelKind,
        read_buffer::GpuAsyncReadBuffer,
    },
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
pub type PickTransport = Rc<RefCell<PickQueue>>;
type Completion = (super::bridge::GpuPickRequest, Result<[u8; 4], String>);

struct PickTarget {
    framebuffer: GpuFrameBuffer,
    read_buffer: GpuAsyncReadBuffer,
}
#[derive(Default)]
pub struct PickQueue {
    request: Option<super::bridge::GpuPickRequest>,
    started: Option<Instant>,
    running: Option<super::bridge::GpuPickRequest>,
    completed: Option<Completion>,
    target: Option<PickTarget>,
    visible_pick_pass: bool,
}
impl PickQueue {
    pub fn sync(
        &mut self,
        request: Option<super::bridge::GpuPickRequest>,
        visible_pick_pass: bool,
    ) {
        if self.request != request {
            self.request = request;
            self.started = request.map(|_| Instant::now());
            self.completed = None;
        }
        self.visible_pick_pass = visible_pick_pass;
    }
    pub fn poll(&mut self) -> Option<Completion> {
        // Drain even cancelled requests while the scene is suspended. Fyrox owns
        // the PBO/fence; keeping at most one in flight avoids losing that fence
        // through an early drop. A reset never admits these old bytes.
        if let Some(request) = self.running {
            if let Some(bytes) = self
                .target
                .as_ref()
                .and_then(|target| target.read_buffer.try_read())
            {
                self.running = None;
                if self.request == Some(request) {
                    self.completed = Some((
                        request,
                        <[u8; 4]>::try_from(bytes.as_slice())
                            .map_err(|_| "GPU ID readback returned an invalid pixel size".into()),
                    ));
                }
            }
        }
        if self.completed.is_none()
            && self
                .started
                .is_some_and(|started| started.elapsed().as_secs_f32() >= 15.0)
        {
            if let Some(request) = self.request {
                self.completed = Some((
                    request,
                    Err(if self.running.is_some() {
                        "GPU ID readback deadline while awaiting PBO fence".into()
                    } else {
                        "GPU ID readback deadline while awaiting source pass".into()
                    }),
                ));
            }
        }
        self.completed.take()
    }
    pub fn target_count(&self) -> usize {
        usize::from(self.target.is_some())
    }
    pub fn running_count(&self) -> usize {
        usize::from(self.running.is_some())
    }
    pub fn reset_device(&mut self) {
        self.request = None;
        self.started = None;
        self.running = None;
        self.completed = None;
        self.target = None;
    }
}

struct SourcePass {
    frames: FrameCounter,
    picks: PickTransport,
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
        for (bundle, uniforms) in bundles.iter().zip(uniforms) {
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
        let mut picks = self.picks.borrow_mut();
        if let Some(request) = picks.request.filter(|_| picks.running.is_none()) {
            // A translated viewport rasterizes the requested logical pixel into
            // a 1x1 owned RGBA8/depth target. Projection, shader, draw order,
            // alpha/depth tests and source textures are the visible pass's own.
            let result = (|| -> Result<(), FrameworkError> {
                if picks.target.is_none() {
                    let color = ctx.server.create_2d_render_target(
                        "Wonderland offscreen ID",
                        PixelKind::RGBA8,
                        1,
                        1,
                    )?;
                    let depth = ctx.server.create_2d_render_target(
                        "Wonderland offscreen ID depth",
                        PixelKind::D24S8,
                        1,
                        1,
                    )?;
                    picks.target = Some(PickTarget {
                        framebuffer: ctx.server.create_frame_buffer(
                            Some(Attachment::depth_stencil(depth)),
                            vec![Attachment::color(color)],
                        )?,
                        read_buffer: ctx.server.create_async_read_buffer(
                            "Wonderland one-pixel ID PBO",
                            4,
                            1,
                        )?,
                    });
                }
                let target = picks.target.as_ref().unwrap();
                target.framebuffer.clear(
                    Rect::new(0, 0, 1, 1),
                    Some(Color::BLACK),
                    Some(1.0),
                    Some(0),
                );
                let mut pick_context = BundleRenderContext {
                    texture_cache: &mut *render_context.texture_cache,
                    render_pass_name: &pass_name,
                    frame_buffer: &target.framebuffer,
                    viewport: Rect::new(-(request.x as i32), -(479 - request.y as i32), 640, 480),
                    uniform_memory_allocator: &mut *render_context.uniform_memory_allocator,
                    resource_manager: ctx.resource_manager,
                    use_pom: false,
                    light_position: &light_position,
                    ambient_light: ctx.scene.rendering_options.ambient_lighting_color,
                    scene_depth: Some(ctx.depth_texture),
                    renderer_resources: ctx.renderer_resources,
                };
                let uniforms: Vec<_> = bundles
                    .iter()
                    .map(|bundle| {
                        bundle
                            .material
                            .data_ref()
                            .set_property("passFlags", Vector4::new(1.0, 0.0, 0.0, 0.0));
                        let uniforms = bundle.write_uniforms(&view_projection, &mut pick_context);
                        bundle.material.data_ref().set_property(
                            "passFlags",
                            Vector4::new(
                                if picks.visible_pick_pass { 1.0 } else { 0.0 },
                                0.0,
                                0.0,
                                0.0,
                            ),
                        );
                        uniforms
                    })
                    .collect();
                pick_context.uniform_memory_allocator.upload(ctx.server)?;
                for (bundle, uniforms) in bundles.iter().zip(uniforms) {
                    if let Some(uniforms) = uniforms {
                        bundle.render_to_frame_buffer(
                            ctx.server,
                            ctx.geometry_cache,
                            ctx.shader_cache,
                            &mut all_instances,
                            &mut pick_context,
                            uniforms,
                            &globals,
                        )?;
                    }
                }
                target
                    .read_buffer
                    .schedule_pixels_transfer(&*target.framebuffer, 0, None)?;
                ctx.server.flush();
                Ok(())
            })();
            match result {
                Ok(()) => picks.running = Some(request),
                Err(error) => picks.completed = Some((request, Err(error.to_string()))),
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

pub fn install(context: &mut GraphicsContext, frames: &FrameCounter, picks: &PickTransport) {
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
                picks: picks.clone(),
            })));
    }
}
