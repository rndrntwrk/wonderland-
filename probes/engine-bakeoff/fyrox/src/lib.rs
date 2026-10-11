//! Fyrox 1.0.1 native OpenGL / browser WebGL2 adapter for C's shared fixture.
#![forbid(unsafe_code)]
#[path = "../../web/bridge.rs"]
mod bridge;
#[path = "../../web/math.rs"]
mod math;
mod source_pass;
#[path = "../../web/upload.rs"]
mod upload;

use fyrox::{
    asset::untyped::ResourceKind,
    core::{
        algebra::{UnitQuaternion, Vector2, Vector3, Vector4},
        color::Color,
        math::TriangleDefinition,
        pool::Handle,
        reflect::prelude::*,
        uuid::Uuid,
        visitor::prelude::*,
    },
    dpi::LogicalSize,
    engine::{executor::Executor, GraphicsContext, GraphicsContextParams},
    event::{ElementState, Event, WindowEvent},
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
    material::{
        shader::{ShaderResource, ShaderResourceExtension},
        Material, MaterialResource, MaterialResourceExtension,
    },
    plugin::{
        error::{GameError, GameResult},
        Plugin, PluginContext,
    },
    resource::texture::{
        TextureKind, TextureMagnificationFilter, TextureMinificationFilter, TexturePixelKind,
        TextureResource, TextureResourceExtension, TextureWrapMode,
    },
    scene::{
        base::BaseBuilder,
        camera::{
            CameraBuilder, Exposure, OrthographicProjection, PerspectiveProjection, Projection,
        },
        mesh::{
            buffer::{TriangleBuffer, VertexBuffer},
            surface::{SurfaceBuilder, SurfaceData, SurfaceResource},
            vertex::StaticVertex,
            MeshBuilder, RenderPath,
        },
        transform::TransformBuilder,
        Scene,
    },
    window::WindowAttributes,
};

// Fyrox 1.0.1 resizes its render targets on WindowEvent::Resized, but its
// GraphicsServer::set_frame_size does not resize the WASM canvas backing store.
// Match the backing store to that already accepted physical renderer size.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(
    inline_js = "export function sync_fyrox_canvas(width,height){const canvas=document.getElementById('engine-canvas');if(!canvas)throw new Error('Fyrox canvas is missing');if(canvas.width!==width)canvas.width=width;if(canvas.height!==height)canvas.height=height;}"
)]
extern "C" {
    fn sync_fyrox_canvas(width: u32, height: u32);
}

#[derive(Default, Visit, Reflect)]
#[reflect(non_cloneable)]
struct Game {
    scene: Handle<Scene>,
    #[visit(skip)]
    #[reflect(hidden)]
    state: Option<bridge::State>,
    #[visit(skip)]
    #[reflect(hidden)]
    materials: Vec<MaterialResource>,
    #[visit(skip)]
    #[reflect(hidden)]
    render_visits: u64,
    #[visit(skip)]
    #[reflect(hidden)]
    observed_frames: u64,
    #[visit(skip)]
    #[reflect(hidden)]
    source_frames: source_pass::FrameCounter,
    #[visit(skip)]
    #[reflect(hidden)]
    picks: source_pass::PickTransport,
}
impl std::fmt::Debug for Game {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Game")
            .field("scene", &self.scene)
            .field("render_visits", &self.render_visits)
            .finish()
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn run() {
    // Executor(Some(EventLoop)) creates a real graphics context. Headless None is deliberately absent.
    let event_loop = match EventLoop::new() {
        Ok(v) => v,
        Err(e) => {
            bridge::fatal(&e.to_string());
        }
    };
    let attributes = WindowAttributes::default()
        .with_title("Wonderland Swarm C — Fyrox 1.0.1")
        .with_inner_size(LogicalSize::new(640.0, 480.0));
    let mut executor = Executor::from_params(
        Some(event_loop),
        GraphicsContextParams {
            window_attributes: attributes,
            msaa_sample_count: None,
            ..Default::default()
        },
    );
    executor.set_resource_hot_reloading_enabled(false);
    executor.set_desired_update_rate(60.0); // Presentation cadence, unrelated to the immutable 30-Hz fixture stamp.
    executor.add_plugin(Game::default());
    executor.run();
}
impl Plugin for Game {
    fn init(&mut self, _scene_path: Option<&str>, context: PluginContext) -> GameResult {
        source_pass::install(context.graphics_context, &self.source_frames, &self.picks);
        let state = bridge::initial_config()
            .and_then(bridge::State::new)
            .map_err(GameError::str)?;
        let (scene, materials) = create_scene(&state).map_err(GameError::str)?;
        self.scene = context.scenes.add(scene);
        self.materials = materials;
        state.publish("fyrox-1.0.1", 0);
        self.state = Some(state);
        Ok(())
    }
    fn on_graphics_context_initialized(&mut self, context: PluginContext) -> GameResult {
        self.picks.borrow_mut().reset_device();
        source_pass::install(context.graphics_context, &self.source_frames, &self.picks);
        Ok(())
    }
    fn update(&mut self, context: &mut PluginContext) -> GameResult {
        let Some(state) = self.state.as_mut() else {
            return Ok(());
        };
        let mut rebuild = false;
        for command in bridge::commands() {
            rebuild |= state.apply(command);
        }
        self.picks
            .borrow_mut()
            .sync(state.pending_gpu_pick(), state.pick_pass);
        if let Some((request, pixel)) = self.picks.borrow_mut().poll() {
            state.complete_gpu_pick(request, pixel);
        }
        self.picks
            .borrow_mut()
            .sync(state.pending_gpu_pick(), state.pick_pass);
        if rebuild {
            let (scene, materials) = create_scene(state).map_err(GameError::str)?;
            let old = self.scene;
            self.scene = context.scenes.add(scene);
            self.materials = materials;
            context.scenes.remove(old);
            // This isolated probe owns the entire renderer scene. Drop disposable GPU caches on replacement.
            if let GraphicsContext::Initialized(graphics) = context.graphics_context {
                graphics.renderer.flush();
            }
        }
        let flags = Vector4::new(if state.pick_pass { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0);
        for material in &self.materials {
            material.data_ref().set_property("passFlags", flags);
        }
        let scene = context.scenes.try_get_mut(self.scene)?;
        scene
            .rendering_options
            .get_value_mut_and_mark_modified()
            .clear_color = Some(if state.pick_pass {
            Color::BLACK
        } else {
            Color::opaque(22, 29, 40)
        });
        if *scene.enabled == state.suspended {
            scene.enabled.set_value_and_mark_modified(!state.suspended);
        }
        if !state.suspended {
            state.update_count += 1;
        }
        #[cfg(target_arch = "wasm32")]
        state.publish("fyrox-1.0.1", self.render_visits);
        #[cfg(not(target_arch = "wasm32"))]
        if rebuild || state.update_count == 1 {
            state.publish("fyrox-1.0.1", self.render_visits);
        }
        if state.config.frames > 0 && self.observed_frames >= state.config.frames {
            state.publish("fyrox-1.0.1", self.render_visits);
            context.loop_controller.exit();
        }
        Ok(())
    }
    fn before_rendering(&mut self, context: PluginContext) -> GameResult {
        self.render_visits += 1;
        if let GraphicsContext::Initialized(graphics) = context.graphics_context {
            #[cfg(target_arch = "wasm32")]
            {
                let (width, height) = graphics.renderer.get_frame_size();
                sync_fyrox_canvas(width, height);
                let window_size = graphics.window.inner_size();
                bridge::metrics(serde_json::json!({
                    "engineViewport": {
                        "rendererWidth": width,
                        "rendererHeight": height,
                        "windowWidth": window_size.width,
                        "windowHeight": window_size.height,
                        "windowScaleFactor": graphics.window.scale_factor()
                    }
                }));
            }
            let stats = graphics.renderer.get_statistics();
            // Count only completed custom passes that issued actual fixture draw calls.
            self.observed_frames = self
                .source_frames
                .load(std::sync::atomic::Ordering::Relaxed);
            #[cfg(target_arch = "wasm32")]
            bridge::metrics(serde_json::json!({
                "engineResourceOwnership": {
                    "materials": self.materials.len(),
                    "fixtureScenes": usize::from(self.scene.is_some()),
                    "offscreenIdTargets": self.picks.borrow().target_count(),
                    "pickReadbackBuffers": self.picks.borrow().target_count(),
                    "pendingPickReadbacks": self.picks.borrow().running_count()
                },
                "engineRendererStatistics": {
                    "drawCalls": stats.geometry.draw_calls,
                    "triangles": stats.geometry.triangles_rendered,
                    "cpuRenderSeconds": stats.pure_frame_time,
                    "textureCacheEntries": stats.texture_cache_size,
                    "geometryCacheEntries": stats.geometry_cache_size,
                    "shaderCacheEntries": stats.shader_cache_size,
                    "uniformBufferCacheEntries": stats.uniform_buffer_cache_size
                }
            }));
            #[cfg(not(target_arch = "wasm32"))]
            if self.observed_frames == 1 {
                println!("WONDERLAND_RENDER_OBSERVATION {{\"actualBackend\":\"opengl\",\"drawCalls\":{},\"triangles\":{},\"cpuRenderSeconds\":{},\"adapter\":\"see engine GL initialization log\"}}",stats.geometry.draw_calls,stats.geometry.triangles_rendered,stats.pure_frame_time);
            }
        }
        Ok(())
    }
    fn on_os_event(&mut self, event: &Event<()>, context: PluginContext) -> GameResult {
        #[cfg(not(target_arch = "wasm32"))]
        if let Event::WindowEvent {
            event: WindowEvent::KeyboardInput { event, .. },
            ..
        } = event
        {
            if event.state == ElementState::Pressed && !event.repeat {
                let mode = match event.physical_key {
                    PhysicalKey::Code(KeyCode::Digit1) => Some("full2d"),
                    PhysicalKey::Code(KeyCode::Digit2) => Some("hybrid2d"),
                    PhysicalKey::Code(KeyCode::Digit3) => Some("full3d"),
                    _ => None,
                };
                if let (Some(mode), Some(state)) = (mode, self.state.as_mut()) {
                    if state.apply(bridge::Command {
                        seq: 0,
                        action: bridge::Action::SetMode { mode: mode.into() },
                    }) {
                        let (scene, materials) = create_scene(state).map_err(GameError::str)?;
                        let old = self.scene;
                        self.scene = context.scenes.add(scene);
                        self.materials = materials;
                        context.scenes.remove(old);
                        if let GraphicsContext::Initialized(g) = context.graphics_context {
                            g.renderer.flush();
                        }
                        state.publish("fyrox-1.0.1", self.render_visits);
                    }
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (event, context);
        Ok(())
    }
}

fn create_scene(state: &bridge::State) -> Result<(Scene, Vec<MaterialResource>), String> {
    let prepared = upload::prepare(state)?;
    let projection = upload::projection(state)?;
    let shader = ShaderResource::from_str(
        Uuid::new_v4(),
        include_str!("source.shader"),
        ResourceKind::Embedded,
    )
    .map_err(|e| e.to_string())?;
    let mut scene = Scene::new();
    scene.set_skybox(None);
    scene
        .rendering_options
        .get_value_mut_and_mark_modified()
        .clear_color = Some(Color::opaque(22, 29, 40));
    let mut materials = Vec::new();
    let c = state.scene.camera;
    let eye = vec3([c.eye.x, c.eye.y, c.eye.z]);
    let forward = vec3([c.target.x, c.target.y, c.target.z]) - eye;
    // Fyrox's look_vector is local +Z, while C's view looks down -Z. The shader still receives C's exact view-projection.
    let rotation = UnitQuaternion::face_towards(&forward, &vec3([c.up.x, c.up.y, c.up.z]));
    let view = if c.orthographic {
        Projection::Orthographic(OrthographicProjection {
            z_near: c.near,
            z_far: c.far,
            vertical_size: c.vertical_size,
        })
    } else {
        Projection::Perspective(PerspectiveProjection {
            fov: c.fov_y_radians,
            z_near: c.near,
            z_far: c.far,
        })
    };
    CameraBuilder::new(
        BaseBuilder::new()
            .with_name("fixture-camera")
            .with_local_transform(
                TransformBuilder::new()
                    .with_local_position(eye)
                    .with_local_rotation(rotation)
                    .build(),
            ),
    )
    .with_projection(view)
    .with_exposure(Exposure::Manual(1.0))
    .with_color_grading_enabled(false)
    .build(&mut scene.graph);
    for draw in prepared {
        let vertices: Vec<_> = draw
            .positions
            .iter()
            .enumerate()
            .map(|(i, p)| StaticVertex {
                position: vec3(*p),
                normal: vec3(draw.normals[i]),
                tex_coord: Vector2::from(draw.uvs[i]),
                // The custom shader assigns location 3 to source RGBA; tangent storage is not used for lighting.
                tangent: Vector4::from(draw.colors[i]),
            })
            .collect();
        let vertex_buffer =
            VertexBuffer::new(vertices.len(), vertices).map_err(|e| format!("{e:?}"))?;
        let triangles = draw
            .indices
            .chunks_exact(3)
            .map(|t| TriangleDefinition([t[0], t[1], t[2]]))
            .collect();
        let data = SurfaceResource::new_embedded(SurfaceData::new(
            vertex_buffer,
            TriangleBuffer::new(triangles),
        ));
        let mut material = Material::from_shader(shader.clone());
        for (i, column) in projection.cols.iter().enumerate() {
            material.set_property(format!("vp{i}"), Vector4::from(*column));
        }
        material.set_property("color", Vector4::from(draw.color));
        material.set_property("lightCutoff", Vector4::from(draw.light_cutoff));
        material.set_property("sprite", Vector4::from(draw.sprite));
        material.set_property("idColor", Vector4::from(draw.id_color));
        material.set_property("unlit", if draw.unlit { 1.0f32 } else { 0.0 });
        material.set_property(
            "passFlags",
            Vector4::new(if state.pick_pass { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0),
        );
        material.bind("colorImage", texture(draw.image)?);
        material.bind("depthAlpha", texture(draw.depth_alpha)?);
        let material = MaterialResource::new(material);
        let surface = SurfaceBuilder::new(data)
            .with_material(material.clone())
            .build();
        MeshBuilder::new(
            BaseBuilder::new()
                .with_name(draw.name)
                .with_frustum_culling(false),
        )
        .with_surfaces(vec![surface])
        .with_render_path(RenderPath::Forward)
        .build(&mut scene.graph);
        materials.push(material);
    }
    Ok((scene, materials))
}
fn vec3(v: [f32; 3]) -> Vector3<f32> {
    Vector3::new(v[0], v[1], v[2])
}
fn texture(source: wonderland_render_core::RgbaImage) -> Result<TextureResource, String> {
    let texture = TextureResource::from_bytes(
        Uuid::new_v4(),
        TextureKind::Rectangle {
            width: source.width,
            height: source.height,
        },
        TexturePixelKind::RGBA8,
        source.pixels.into_iter().flatten().collect(),
        ResourceKind::Embedded,
    )
    .ok_or("invalid uploaded texture")?;
    {
        let mut data = texture.data_ref();
        data.set_minification_filter(TextureMinificationFilter::Nearest);
        data.set_magnification_filter(TextureMagnificationFilter::Nearest);
        data.set_s_wrap_mode(TextureWrapMode::ClampToEdge);
        data.set_t_wrap_mode(TextureWrapMode::ClampToEdge);
    }
    Ok(texture)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn camera_positive_z_matches_fixture_look_direction() {
        let forward = Vector3::new(-3.0, -2.0, -5.0).normalize();
        let rotation = UnitQuaternion::face_towards(&forward, &Vector3::y());
        assert!((rotation * Vector3::z() - forward).norm() < 1e-5);
    }
    #[test]
    fn embedded_source_material_is_valid_released_ron() {
        ShaderResource::from_str(
            Uuid::new_v4(),
            include_str!("source.shader"),
            ResourceKind::Embedded,
        )
        .unwrap();
    }
    #[test]
    fn embedded_only_fixture_registry_matches_fyrox_serialization() {
        use fyrox::asset::registry::{RegistryContainer, RegistryContainerExt};
        assert_eq!(
            RegistryContainer::default()
                .serialize_to_string()
                .unwrap()
                .trim(),
            include_str!("../../web/data/resources.registry").trim()
        );
    }
}
