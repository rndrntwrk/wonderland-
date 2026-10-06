//! Bevy 0.19.1 presentation adapter. All world inputs come from the frozen C fixture.
#![forbid(unsafe_code)]
#[cfg(all(feature = "webgpu", feature = "webgl2"))]
compile_error!(
    "Build Bevy WebGPU and WebGL2 as separate artifacts; Bevy overrides WebGL2 when both are enabled."
);
#[cfg(all(
    target_arch = "wasm32",
    not(any(feature = "webgpu", feature = "webgl2"))
))]
compile_error!("A WASM Bevy probe must select webgpu or webgl2 explicitly.");

#[path = "../../web/bridge.rs"]
mod bridge;
#[path = "../../web/math.rs"]
mod math;
mod picking;
#[path = "../../web/upload.rs"]
mod upload;

use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    camera::{
        RenderTarget, ScalingMode,
        visibility::{NoFrustumCulling, RenderLayers},
    },
    core_pipeline::tonemapping::Tonemapping,
    image::ImageSampler,
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology},
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        error_handler::{RenderErrorHandler, RenderErrorPolicy},
        render_resource::*,
        renderer::RenderAdapterInfo,
    },
    shader::ShaderRef,
    window::WindowResolution,
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[derive(Resource)]
struct Probe(bridge::State);
#[derive(Component)]
struct FixtureNode;
#[derive(Component)]
struct PickCamera;
#[derive(Resource, Default)]
struct Ownership {
    meshes: Vec<Handle<Mesh>>,
    materials: Vec<Handle<SourceMaterial>>,
    pick_materials: Vec<Handle<SourceMaterial>>,
    images: Vec<Handle<Image>>,
}
#[derive(Resource, Clone, Default)]
struct RenderVisits(Arc<AtomicU64>);

#[derive(Clone, Debug, ShaderType)]
struct Parameters {
    view_projection: Mat4,
    color: Vec4,
    light_cutoff: Vec4,
    sprite: Vec4,
    id_color: Vec4,
    flags: Vec4,
}
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
struct SourceMaterial {
    // Preserve the reference draw order; this affects sorting, never fragment depth.
    source_order_bias: f32,
    #[uniform(0)]
    parameters: Parameters,
    #[texture(1)]
    #[sampler(2)]
    image: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    depth_alpha: Handle<Image>,
}
impl Material for SourceMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://bevy_gate/source.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "embedded://bevy_gate/source.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }
    fn depth_bias(&self) -> f32 {
        self.source_order_bias
    }
    fn specialize(
        _: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(3),
        ])?];
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(true);
            depth.depth_compare = Some(CompareFunction::GreaterEqual);
        }
        Ok(())
    }
}
struct Sources;
impl Plugin for Sources {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "source.wgsl");
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn run() {
    let state = match bridge::initial_config().and_then(bridge::State::new) {
        Ok(s) => s,
        Err(e) => {
            bridge::fatal(&e);
        }
    };
    let visits = RenderVisits::default();
    let mut app = App::new();
    app.insert_resource(Probe(state))
        .insert_resource(visits.clone())
        .init_resource::<Ownership>()
        .insert_resource(RenderErrorHandler(|error, main_world, _| {
            bridge::fail(&format!(
                "Bevy renderer {:?}: {}",
                error.ty, error.description
            ));
            main_world.write_message(AppExit::error());
            RenderErrorPolicy::StopRendering
        }))
        .insert_resource(ClearColor(Color::srgb_u8(22, 29, 40)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Wonderland Swarm C — Bevy 0.19.1".into(),
                resolution: WindowResolution::new(640, 480),
                canvas: Some("#engine-canvas".into()),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            Sources,
            MaterialPlugin::<SourceMaterial>::default(),
            picking::PickingPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, update);
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app
            .insert_resource(visits)
            .add_systems(Render, count_render.in_set(RenderSystems::Cleanup));
    }
    let result = app.run();
    #[cfg(not(target_arch = "wasm32"))]
    if !matches!(result, AppExit::Success) {
        std::process::exit(1);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = result;
}
fn count_render(visits: Res<RenderVisits>, adapter: Res<RenderAdapterInfo>) {
    let previous = visits.0.fetch_add(1, Ordering::Relaxed);
    #[cfg(not(target_arch = "wasm32"))]
    if previous == 0 {
        println!(
            "WONDERLAND_RENDER_OBSERVATION {}",
            serde_json::json!({
                "actualBackend": format!("{:?}", adapter.backend),
                "name": adapter.name,
                "driver": adapter.driver,
                "driverInfo": adapter.driver_info,
                "deviceType": format!("{:?}", adapter.device_type),
                "evidence": "engine-selected adapter and render-schedule visit; pixel gate is separate"
            })
        );
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (previous, adapter);
}

fn setup(
    mut commands: Commands,
    probe: Res<Probe>,
    mut owned: ResMut<Ownership>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SourceMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut pick_source: ResMut<picking::PickSource>,
) {
    if let Err(e) = spawn_scene(
        &mut commands,
        &probe.0,
        &mut owned,
        &mut meshes,
        &mut materials,
        &mut images,
        &mut pick_source,
    ) {
        bridge::fatal(&e);
    }
    probe.0.publish("bevy-0.19.1", 0);
}
fn spawn_scene(
    commands: &mut Commands,
    state: &bridge::State,
    owned: &mut Ownership,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<SourceMaterial>,
    images: &mut Assets<Image>,
    pick_source: &mut picking::PickSource,
) -> Result<(), String> {
    let prepared = upload::prepare(state)?;
    pick_source.expected_draws = prepared.len();
    let vp = Mat4::from_cols_array_2d(&upload::projection(state)?.cols);
    let camera = state.scene.camera;
    let eye = Vec3::new(camera.eye.x, camera.eye.y, camera.eye.z);
    let target = Vec3::new(camera.target.x, camera.target.y, camera.target.z);
    let projection = if camera.orthographic {
        Projection::Orthographic(OrthographicProjection {
            near: camera.near,
            far: camera.far,
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: camera.vertical_size,
            },
            ..OrthographicProjection::default_3d()
        })
    } else {
        Projection::Perspective(PerspectiveProjection {
            fov: camera.fov_y_radians,
            near: camera.near,
            far: camera.far,
            ..default()
        })
    };
    let camera_entity = commands
        .spawn((
            FixtureNode,
            Camera3d::default(),
            projection.clone(),
            Msaa::Off,
            CompositingSpace::Srgb,
            Tonemapping::None,
            Transform::from_translation(eye)
                .looking_at(target, Vec3::new(camera.up.x, camera.up.y, camera.up.z)),
        ))
        .id();
    // Bevy 0.19.1 requests alternate sRGB views for an Rgba8Unorm main
    // target, but WebGL2 lacks VIEW_FORMATS. Half-float storage needs no
    // alternate view. Srgb compositing still keeps source RGB encoded, and
    // Tonemapping::None bypasses the HDR tone mapper entirely.
    #[cfg(feature = "webgl2")]
    commands.entity(camera_entity).insert(bevy::camera::Hdr);
    #[cfg(not(feature = "webgl2"))]
    let _ = camera_entity;
    // The ID camera owns a separate attachment and render layer. Its material
    // flags are immutable; an ordinary selection never toggles the visible pass.
    let mut pick_image = Image::new_target_texture(640, 480, TextureFormat::Rgba8UnormSrgb, None);
    pick_image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    pick_image.sampler = ImageSampler::nearest();
    let pick_target = images.add(pick_image);
    let pick_camera = commands
        .spawn((
            FixtureNode,
            PickCamera,
            Camera3d::default(),
            Camera {
                order: -1,
                is_active: false,
                clear_color: Color::BLACK.into(),
                ..default()
            },
            RenderTarget::Image(pick_target.clone().into()),
            projection,
            Msaa::Off,
            CompositingSpace::Srgb,
            Tonemapping::None,
            RenderLayers::layer(1),
            Transform::from_translation(eye)
                .looking_at(target, Vec3::new(camera.up.x, camera.up.y, camera.up.z)),
        ))
        .id();
    #[cfg(feature = "webgl2")]
    commands.entity(pick_camera).insert(bevy::camera::Hdr);
    #[cfg(not(feature = "webgl2"))]
    let _ = pick_camera;
    pick_source.image = Some(pick_target.clone());
    pick_source.revision = state.revision;
    owned.images.push(pick_target);
    for (draw_order, draw) in prepared.into_iter().enumerate() {
        let mesh = meshes.add(
            Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, draw.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, draw.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, draw.uvs)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, draw.colors)
            .with_inserted_indices(Indices::U32(draw.indices)),
        );
        let color = images.add(image(draw.image));
        let depth = images.add(image(draw.depth_alpha));
        let material = materials.add(SourceMaterial {
            // Fixture geometry is bounded by this finite camera range. A gap of twice
            // that range keeps each source draw ahead of every later source draw.
            source_order_bias: draw_order as f32 * (2.0 * camera.far + 1.0),
            parameters: Parameters {
                view_projection: vp,
                color: Vec4::from_array(draw.color),
                light_cutoff: Vec4::from_array(draw.light_cutoff),
                sprite: Vec4::from_array(draw.sprite),
                id_color: Vec4::from_array(draw.id_color),
                flags: Vec4::new(
                    if state.pick_pass { 1.0 } else { 0.0 },
                    if draw.unlit { 1.0 } else { 0.0 },
                    0.0,
                    0.0,
                ),
            },
            image: color.clone(),
            depth_alpha: depth.clone(),
        });
        let mut id_material = materials.get(&material).unwrap().clone();
        id_material.parameters.flags.x = 1.0;
        let id_material = materials.add(id_material);
        commands.spawn((
            FixtureNode,
            Name::new(draw.name.clone()),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::default(),
            NoFrustumCulling,
        ));
        commands.spawn((
            FixtureNode,
            Name::new(format!("{}-offscreen-id", draw.name)),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(id_material.clone()),
            Transform::default(),
            NoFrustumCulling,
            RenderLayers::layer(1),
        ));
        owned.meshes.push(mesh);
        owned.materials.push(material);
        owned.pick_materials.push(id_material);
        owned.images.extend([color, depth]);
    }
    Ok(())
}
fn image(source: wonderland_render_core::RgbaImage) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: source.width,
            height: source.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        source.pixels.into_iter().flatten().collect(),
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::nearest();
    image
}
fn update(
    mut commands: Commands,
    mut probe: ResMut<Probe>,
    visits: Res<RenderVisits>,
    nodes: Query<Entity, With<FixtureNode>>,
    mut cameras: Query<(&mut Camera, Option<&PickCamera>), With<FixtureNode>>,
    mut clear: ResMut<ClearColor>,
    mut owned: ResMut<Ownership>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SourceMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut pick_source: ResMut<picking::PickSource>,
    pick_results: Res<picking::PickResults>,
    keys: Res<ButtonInput<KeyCode>>,
    mut exit: MessageWriter<AppExit>,
) {
    let mut rebuild = false;
    let mut inputs = bridge::commands();
    #[cfg(not(target_arch = "wasm32"))]
    for (key, mode) in [
        (KeyCode::Digit1, "full2d"),
        (KeyCode::Digit2, "hybrid2d"),
        (KeyCode::Digit3, "full3d"),
    ] {
        if keys.just_pressed(key) {
            inputs.push(bridge::Command {
                seq: 0,
                action: bridge::Action::SetMode { mode: mode.into() },
            });
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = &keys;
    for input in inputs {
        rebuild |= probe.0.apply(input);
    }
    if rebuild {
        for entity in &nodes {
            commands.entity(entity).despawn();
        }
        for handle in owned.meshes.drain(..) {
            meshes.remove(handle.id());
        }
        for handle in owned.materials.drain(..) {
            materials.remove(handle.id());
        }
        for handle in owned.pick_materials.drain(..) {
            materials.remove(handle.id());
        }
        for handle in owned.images.drain(..) {
            images.remove(handle.id());
        }
        if let Err(e) = spawn_scene(
            &mut commands,
            &probe.0,
            &mut owned,
            &mut meshes,
            &mut materials,
            &mut images,
            &mut pick_source,
        ) {
            bridge::fatal(&e);
        }
    }
    if let Some((request, pixel)) = pick_results.take() {
        probe.0.complete_gpu_pick(request, pixel);
    }
    pick_source.request = probe.0.pending_gpu_pick();
    pick_results.current(pick_source.request);
    let pass = if probe.0.pick_pass { 1.0 } else { 0.0 };
    clear.0 = if probe.0.pick_pass {
        Color::BLACK
    } else {
        Color::srgb_u8(22, 29, 40)
    };
    for handle in &owned.materials {
        if materials
            .get(handle)
            .is_some_and(|m| m.parameters.flags.x != pass)
        {
            if let Some(mut material) = materials.get_mut(handle) {
                material.parameters.flags.x = pass;
            }
        }
    }
    for (mut camera, pick_camera) in &mut cameras {
        camera.is_active =
            !probe.0.suspended && (pick_camera.is_none() || pick_source.request.is_some());
    }
    if !probe.0.suspended {
        probe.0.update_count += 1;
    }
    let rendered = visits.0.load(Ordering::Relaxed);
    #[cfg(target_arch = "wasm32")]
    {
        bridge::metrics(serde_json::json!({
            "engineResourceOwnership": {
                "meshes": owned.meshes.len(),
                "materials": owned.materials.len() + owned.pick_materials.len(),
                "images": owned.images.len(),
                "offscreenIdTargets": usize::from(pick_source.image.is_some()),
                "maximumPickReadbackBuffers": 1
            }
        }));
        probe.0.publish("bevy-0.19.1", rendered);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if rebuild
        || probe.0.update_count == 1
        || (probe.0.config.frames > 0 && rendered >= probe.0.config.frames)
    {
        probe.0.publish("bevy-0.19.1", rendered);
    }
    if probe.0.config.frames > 0 && rendered >= probe.0.config.frames {
        exit.write(AppExit::Success);
    }
}
