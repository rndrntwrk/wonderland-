//! One owned asynchronous copy of the engine's offscreen ID attachment.
//! No GPU handle or color index is admitted as a game identity here.
use super::bridge::GpuPickRequest;
use bevy::{
    camera::NormalizedRenderTarget,
    core_pipeline::{core_3d::Transparent3d, upscaling::ViewUpscalingPipeline},
    platform::time::Instant,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        camera::ExtractedCamera,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_phase::ViewSortedRenderPhases,
        render_resource::*,
        renderer::{RenderDevice, RenderQueue},
        texture::GpuImage,
        view::ExtractedView,
    },
};
use std::sync::{Arc, Mutex};

type Completion = (GpuPickRequest, Result<[u8; 4], String>);

#[derive(Resource, Clone, Default, ExtractResource)]
pub struct PickSource {
    pub image: Option<Handle<Image>>,
    pub revision: u64,
    pub request: Option<GpuPickRequest>,
    pub expected_draws: usize,
}
#[derive(Default)]
struct Mailbox {
    current: Option<GpuPickRequest>,
    completed: Option<Completion>,
}
#[derive(Resource, Clone, Default)]
pub struct PickResults(Arc<Mutex<Mailbox>>);
impl PickResults {
    pub fn current(&self, request: Option<GpuPickRequest>) {
        let mut mailbox = self.0.lock().unwrap();
        mailbox.current = request;
        if mailbox
            .completed
            .as_ref()
            .is_some_and(|(completed, _)| Some(*completed) != request)
        {
            mailbox.completed = None;
        }
    }
    pub fn take(&self) -> Option<Completion> {
        self.0.lock().unwrap().completed.take()
    }
    fn complete(&self, request: GpuPickRequest, result: Result<[u8; 4], String>) {
        let mut mailbox = self.0.lock().unwrap();
        if mailbox.current == Some(request) {
            mailbox.completed = Some((request, result));
        }
    }
}

struct CopyRequest {
    request: GpuPickRequest,
    started: Instant,
    buffer: Option<ReadbackBuffer>,
    finished: bool,
}
#[derive(Clone)]
struct ReadbackBuffer(Arc<Mutex<Option<Buffer>>>);
impl ReadbackBuffer {
    fn take(&self) -> Option<Buffer> {
        self.0.lock().unwrap().take()
    }
    fn cancel(&self) {
        if let Some(buffer) = self.take() {
            // Take ownership before unmapping: cancelling a pending native map
            // can invoke its callback synchronously. That callback must see an
            // empty owner, and must not unmap or destroy this buffer again.
            buffer.unmap();
            buffer.destroy();
        }
    }
}
#[derive(Resource, Default)]
struct Transfer {
    active: Option<CopyRequest>,
    ready_revision: u64,
    ready_frames: u8,
}
impl Drop for CopyRequest {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            // Cancels a pending map too. Its late callback carries the old request.
            buffer.cancel();
        }
    }
}
pub struct PickingPlugin;
impl Plugin for PickingPlugin {
    fn build(&self, app: &mut App) {
        let results = PickResults::default();
        app.insert_resource(results.clone())
            .init_resource::<PickSource>()
            .add_plugins(ExtractResourcePlugin::<PickSource>::default());
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .insert_resource(results)
                .init_resource::<Transfer>()
                .add_systems(Render, copy_id_pixel.in_set(RenderSystems::Cleanup));
        }
    }
}
fn copy_id_pixel(
    source: Res<PickSource>,
    results: Res<PickResults>,
    mut transfer: ResMut<Transfer>,
    images: Res<RenderAssets<GpuImage>>,
    pipelines: Res<PipelineCache>,
    phases: Res<ViewSortedRenderPhases<Transparent3d>>,
    views: Query<(&ExtractedCamera, &ExtractedView, &ViewUpscalingPipeline)>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    // Non-blocking polling drives native callbacks; browsers poll their own device.
    let _ = device.poll(PollType::Poll);
    if source.revision != transfer.ready_revision {
        transfer.ready_revision = source.revision;
        transfer.ready_frames = 0;
    }
    // Global PipelineCache waiting entries include optional compute work that
    // can stay queued forever on WebGL (e.g. sparse buffer updates). Readiness
    // belongs to this ID view: every source draw and its compiled pipeline must
    // have reached the phase that was rendered before this Cleanup system.
    // ViewUpscalingPipeline is installed only after Bevy's output preparation
    // blocks on that specific final blit pipeline, including the target format.
    let phase = views
        .iter()
        .find(|(camera, _, _)| {
            matches!(&camera.target,
        Some(NormalizedRenderTarget::Image(target)) if Some(&target.handle)==source.image.as_ref())
        })
        .and_then(|(_, view, _)| phases.0.get(&view.retained_view_entity));
    let queued_draws = phase.map_or(0, |phase| phase.items.len());
    let view_ready = phase.is_some_and(|phase| {
        phase.items.len() == source.expected_draws
            && phase
                .items
                .values()
                .all(|item| pipelines.get_render_pipeline(item.pipeline).is_some())
    });
    if view_ready {
        transfer.ready_frames = transfer.ready_frames.saturating_add(1);
    } else {
        transfer.ready_frames = 0;
    }
    if transfer.active.as_ref().map(|active| active.request) != source.request {
        transfer.ready_frames = 0;
        transfer.active = source.request.map(|request| CopyRequest {
            request,
            started: Instant::now(),
            buffer: None,
            finished: false,
        });
    }
    let ready_frames = transfer.ready_frames;
    let ready = ready_frames >= 2;
    let Some(active) = transfer.active.as_mut() else {
        return;
    };
    if active.finished {
        return;
    }
    if active.started.elapsed().as_secs_f32() >= 15.0 {
        results.complete(
            active.request,
            Err(if active.buffer.is_some() {
                "GPU ID readback deadline while awaiting map".into()
            } else {
                let pending: Vec<_> = pipelines.pipelines().filter(|pipeline| !matches!(pipeline.state, CachedPipelineState::Ok(_))).take(8).map(|pipeline| {
                    let label = match &pipeline.descriptor {
                        PipelineDescriptor::RenderPipelineDescriptor(descriptor) => descriptor.label.as_deref(),
                        PipelineDescriptor::ComputePipelineDescriptor(descriptor) => descriptor.label.as_deref(),
                    };
                    format!("{}: {:?}", label.unwrap_or("unlabelled"), pipeline.state)
                }).collect();
                format!("GPU ID readback deadline while awaiting rendered target: ready_frames={ready_frames}, image_ready={}, source_revision={}, request_revision={}, source_draws={queued_draws}/{}, source_view_ready={view_ready}, waiting_pipelines={}, pending={pending:?}",
                    source.image.as_ref().is_some_and(|image| images.get(image).is_some()), source.revision, active.request.revision, source.expected_draws, pipelines.waiting_pipelines().count())
            }),
        );
        if let Some(buffer) = active.buffer.take() {
            buffer.cancel();
        }
        active.finished = true;
        return;
    }
    if active.buffer.is_some() || !ready {
        return;
    }
    let Some(image) = source.image.as_ref().and_then(|image| images.get(image)) else {
        return;
    };
    if source.revision != active.request.revision {
        return;
    }
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Wonderland one-pixel GPU ID readback"),
        size: 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("Wonderland offscreen ID copy after render"),
    });
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &image.texture,
            mip_level: 0,
            origin: Origin3d {
                x: active.request.x,
                y: active.request.y,
                z: 0,
            },
            aspect: TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: &buffer,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(1),
            },
        },
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let request = active.request;
    let results = results.clone();
    let owner = ReadbackBuffer(Arc::new(Mutex::new(Some(buffer.clone()))));
    let callback_owner = owner.clone();
    buffer.slice(..).map_async(MapMode::Read, move |result| {
        let Some(mapped_buffer) = callback_owner.take() else {
            return;
        };
        let mapped = result.is_ok();
        let pixel = result
            .map_err(|error| format!("GPU ID map failed: {error}"))
            .and_then(|()| {
                let mapped = mapped_buffer.slice(..).get_mapped_range();
                <[u8; 4]>::try_from(&mapped[..4]).map_err(|error| error.to_string())
            });
        // A failed map is already unmapped. wgpu-core/WebGL rejects a second
        // unmap even though browser WebGPU implementations may tolerate it.
        if mapped {
            mapped_buffer.unmap();
        }
        mapped_buffer.destroy();
        results.complete(request, pixel);
    });
    active.buffer = Some(owner);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_gpu_callback_cannot_replace_the_latest_mailbox_request() {
        let results = PickResults::default();
        let first = GpuPickRequest {
            serial: 1,
            command: 9,
            revision: 1,
            x: 0,
            y: 0,
        };
        let second = GpuPickRequest {
            serial: 2,
            command: 10,
            revision: 1,
            x: 30,
            y: 20,
        };
        results.current(Some(first));
        results.current(Some(second));
        results.complete(first, Ok([1, 0, 0, 255]));
        assert!(results.take().is_none());
        results.complete(second, Ok([2, 0, 0, 255]));
        assert_eq!(results.take(), Some((second, Ok([2, 0, 0, 255]))));
        results.current(None);
        results.complete(second, Ok([0, 0, 0, 255]));
        assert!(results.take().is_none());
    }
}
