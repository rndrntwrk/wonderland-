//! Browser canvas for the same depth-tested source geometry used by native tests.
use leptos::{ev, leptos_dom::helpers::window_event_listener, prelude::*};
use std::{collections::BTreeMap, sync::Arc};
use wasm_bindgen::prelude::*;
use wonderland_world_view::{
    ViewportControls, WorldDocument, WorldError, WorldPick, WorldRenderStats, WorldRenderer,
};

use crate::world_capture::{WorldCaptureReceipt, capture_metadata};

#[wasm_bindgen(module = "/public/world-gpu.mjs")]
extern "C" {
    #[wasm_bindgen(js_name=nextWorldPaint)]
    fn next_paint() -> js_sys::Promise;
    #[wasm_bindgen(catch,js_name=paintSourceWorld)]
    fn paint(canvas: &web_sys::HtmlCanvasElement, frame: &str) -> Result<(), JsValue>;
    #[wasm_bindgen(js_name=sourceWorldFrameAvailable)]
    fn frame_available(canvas: &web_sys::HtmlCanvasElement) -> bool;
    #[wasm_bindgen(js_name=pickSourceWorld)]
    fn gpu_pick(canvas: &web_sys::HtmlCanvasElement, x: u32, y: u32) -> js_sys::Promise;
    #[wasm_bindgen(js_name=captureSourceWorld)]
    fn capture_png(
        canvas: &web_sys::HtmlCanvasElement,
        generation: &str,
        metadata: &str,
    ) -> js_sys::Promise;
    #[wasm_bindgen(js_name=clearSourceWorldCapture)]
    fn clear_capture(canvas: &web_sys::HtmlCanvasElement);
    #[wasm_bindgen(js_name=disposeSourceWorld)]
    fn dispose(canvas: &web_sys::HtmlCanvasElement);
}

#[derive(Clone, Default)]
pub enum WorldCaptureState {
    #[default]
    Unavailable,
    Idle,
    Preparing,
    Ready(WorldCaptureReceipt),
    Failed(String),
}
#[derive(Clone, Copy)]
pub struct WorldCaptureControls {
    command: RwSignal<(u64, bool)>,
    state: RwSignal<WorldCaptureState>,
}
impl Default for WorldCaptureControls {
    fn default() -> Self {
        Self {
            command: RwSignal::new((0, false)),
            state: RwSignal::new(WorldCaptureState::Unavailable),
        }
    }
}
impl WorldCaptureControls {
    fn send(self, capture: bool) {
        if let Some(serial) = self.command.get_untracked().0.checked_add(1) {
            self.command.set((serial, capture));
        } else {
            self.state.set(WorldCaptureState::Failed(
                "Reopen this view before capturing again.".into(),
            ));
        }
    }
}
#[component]
pub fn WorldCapturePanel(capture: WorldCaptureControls) -> impl IntoView {
    view! {
        <div class="world-capture-panel" aria-label="Source view photo">
            <button class="chrome" disabled=move || matches!(capture.state.get(), WorldCaptureState::Unavailable | WorldCaptureState::Preparing) on:click=move |_| capture.send(true)>"Capture PNG"</button>
            <p class="world-capture-status" role="status">{move || match capture.state.get() {
                WorldCaptureState::Unavailable => "Waiting for the view…".to_string(),
                WorldCaptureState::Idle => "Capture the scene without menus.".to_string(),
                WorldCaptureState::Preparing => "Preparing your photo…".to_string(),
                WorldCaptureState::Ready(_) => "Your photo is ready to save.".to_string(),
                WorldCaptureState::Failed(message) => message,
            }}</p>
            {move || match capture.state.get() {
                WorldCaptureState::Ready(receipt) => view! {
                    <figure class="world-capture-preview">
                        <img src=receipt.image_url.clone() alt="Captured source-world view"/>
                        <figcaption>{format!("{} × {} pixels · local source view",receipt.width,receipt.height)}</figcaption>
                    </figure>
                    <div class="world-capture-actions">
                        <a class="chrome" href=receipt.image_url download=receipt.filename>"Save PNG"</a>
                        <a class="chrome" href=receipt.metadata_url download=receipt.metadata_filename>"Photo details"</a>
                        <button class="chrome" on:click=move |_| capture.send(false)>"Discard photo"</button>
                    </div>
                }.into_any(),
                _ => ().into_any(),
            }}
        </div>
    }
}

/// Source imports are proposals until CPU preparation and device drawing agree.
/// The callback runs after the renderer borrow is released, for this exact Arc.
#[derive(Clone, Copy)]
pub struct WorldReplacementControls {
    pub document: Signal<Option<Arc<WorldDocument>>>,
    pub complete: Callback<(Arc<WorldDocument>, Result<(), String>)>,
}

#[derive(serde::Deserialize)]
struct GpuPickReceipt {
    generation: String,
    index: u32,
    x: u32,
    y: u32,
}

#[derive(Clone, Copy)]
struct PointerTrack {
    x: f32,
    y: f32,
    start_x: f32,
    start_y: f32,
    moved: bool,
    rotate: bool,
}
enum Gesture {
    Pan(f32, f32),
    Rotate(f32, f32),
    Pinch(f32, f32, f32),
}

fn pan(controls: &mut ViewportControls, x: f32, y: f32, extent: f32, width: f32) {
    let units = extent / width.max(1.) / controls.zoom.clamp(0.25, 8.);
    let (sin, cos) = controls.yaw_radians.sin_cos();
    controls.pan_x += units * (sin * x - cos * y * 1.5);
    controls.pan_y += units * (cos * x + sin * y * 1.5);
}
fn pixel(canvas: &web_sys::HtmlCanvasElement, x: i32, y: i32) -> Option<(u32, u32)> {
    let rect = canvas.get_bounding_client_rect();
    if rect.width() <= 0. || rect.height() <= 0. {
        return None;
    }
    let x = (f64::from(x) - rect.left()) / rect.width();
    let y = (f64::from(y) - rect.top()) / rect.height();
    if !(0. ..1.).contains(&x) || !(0. ..1.).contains(&y) {
        return None;
    }
    Some((
        (x * f64::from(canvas.width())).floor() as u32,
        (y * f64::from(canvas.height())).floor() as u32,
    ))
}
fn surface_size(canvas: &web_sys::HtmlCanvasElement) -> (u32, u32) {
    let rect = canvas.get_bounding_client_rect();
    let width = rect.width().max(1.);
    let height = rect.height().max(1.);
    // Resource bounds affect the canvas sampling resolution, never lot size,
    // number of floors, source identities, or construction permissions.
    let scale = (393_216. / (width * height))
        .sqrt()
        .min(1.)
        .min(960. / width)
        .min(720. / height);
    (
        (width * scale).round().max(1.) as u32,
        (height * scale).round().max(1.) as u32,
    )
}
fn js_message(error: JsValue) -> String {
    js_sys::Reflect::get(&error, &JsValue::from_str("message"))
        .ok()
        .and_then(|value| value.as_string())
        .or_else(|| error.as_string())
        .unwrap_or_else(|| "World graphics could not be displayed.".into())
}

/// `world` is accepted source architecture, independent of preview Home saves.
/// The caller owns controls and receives generation/revision-checked world picks.
#[component]
pub fn WorldViewport(
    #[prop(into)] world: Signal<Arc<WorldDocument>>,
    controls: RwSignal<ViewportControls>,
    on_pick: Callback<WorldPick>,
    #[prop(optional)] draft: Option<Signal<crate::world_draft::WorldDraftOutline>>,
    #[prop(optional)] capture: Option<WorldCaptureControls>,
    #[prop(optional)] replacement: Option<WorldReplacementControls>,
) -> impl IntoView {
    let canvas = NodeRef::<leptos::html::Canvas>::new();
    let displayed = RwSignal::new(world.get_untracked());
    let runtime = StoredValue::new(None::<WorldRenderer>);
    let painted = StoredValue::new(
        None::<(
            String,
            Arc<WorldDocument>,
            ViewportControls,
            WorldRenderStats,
        )>,
    );
    let pointers = StoredValue::new(BTreeMap::<i32, PointerTrack>::new());
    let requested = RwSignal::new(0u64);
    let pick_request = RwSignal::new(0u64);
    let resize = RwSignal::new(0u64);
    let busy = RwSignal::new(true);
    let retained_capture = StoredValue::new(None::<WorldCaptureState>);
    let error = RwSignal::new(String::new());
    let stats = RwSignal::new(WorldRenderStats::default());
    let draft_projection = Memo::new(move |_| {
        let outline = draft.map(|draft| draft.get()).unwrap_or_default();
        let dimensions = stats.get();
        if dimensions.width == 0 || dimensions.height == 0 || busy.get() {
            return (Vec::<(f32, f32)>::new(), false);
        }
        let document = displayed.get();
        let Ok(projection) = wonderland_world_view::camera_projection(
            &document,
            controls.get(),
            dimensions.width as f32 / dimensions.height as f32,
        ) else {
            return (Vec::new(), false);
        };
        let points: Option<Vec<_>> = outline
            .points
            .iter()
            .map(|point| {
                let clip = projection.transform_vec4([point.x, point.y, point.z, 1.]);
                if !clip.iter().all(|value| value.is_finite()) || clip[3] <= 0. {
                    return None;
                }
                Some((
                    (clip[0] / clip[3] + 1.) * 500.,
                    (1. - clip[1] / clip[3]) * 500.,
                ))
            })
            .collect();
        (points.unwrap_or_default(), outline.closed)
    });
    let resize_handle = window_event_listener(ev::resize, move |_| {
        resize.update(|value| *value = value.saturating_add(1));
    });
    on_cleanup(move || resize_handle.remove());
    Effect::new(move |_| {
        let admitted = world.get();
        let candidate = replacement.and_then(|request| request.document.get());
        let current_settings = controls.get();
        let settings = if candidate.is_some() {
            ViewportControls::default()
        } else {
            current_settings
        };
        let document = candidate.clone().unwrap_or(admitted);
        resize.get();
        let Some(canvas) = canvas.get() else {
            return;
        };
        let (width, height) = surface_size(&canvas);
        // A source admission callback changes the parent signals, but must not
        // reinstall an already painted frame or invalidate a retained photo.
        let unchanged = painted.with_value(|last| {
            last.as_ref().is_some_and(|(_, old, old_settings, stats)| {
                Arc::ptr_eq(old, &document)
                    && *old_settings == settings
                    && (stats.width, stats.height) == (width, height)
            })
        }) && frame_available(&canvas);
        if unchanged {
            // A reverted/superseded proposal must not paint after this no-op.
            if busy.get_untracked() {
                requested.update(|value| *value = value.saturating_add(1));
                busy.set(false);
                if let (Some(capture), Some(previous)) = (capture, retained_capture.get_value()) {
                    if matches!(previous, WorldCaptureState::Preparing) {
                        clear_capture(&canvas);
                        capture.state.set(WorldCaptureState::Idle);
                    } else {
                        capture.state.set(previous);
                    }
                }
                retained_capture.set_value(None);
            }
            if let (Some(request), Some(candidate)) = (replacement, candidate) {
                request.complete.run((candidate, Ok(())));
            }
            return;
        }
        requested.update(|value| *value = value.saturating_add(1));
        let expected = requested.get_untracked();
        busy.set(true);
        // Keep the last image, resources, source metadata and ready URL alive
        // throughout preparation. Only a successful device commit retires them.
        if let Some(capture) = capture {
            if retained_capture.get_value().is_none() {
                retained_capture.set_value(Some(capture.state.get_untracked()));
            }
            capture.state.set(WorldCaptureState::Unavailable);
        }
        wasm_bindgen_futures::spawn_local(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(next_paint()).await;
            if requested.try_get_untracked() != Some(expected) {
                return;
            }
            let (width, height) = surface_size(&canvas);
            let result = runtime.try_update_value(|slot| -> Result<WorldRenderStats, String> {
                let publish = |frame: &wonderland_world_view::WorldGpuFrame| {
                    let encoded = serde_json::to_string(frame)
                        .map_err(|error| WorldError(error.to_string()))?;
                    if encoded.len() > 128 * 1024 * 1024 {
                        return Err(WorldError(
                            "Source GPU frame exceeds the transfer budget.".into(),
                        ));
                    }
                    paint(&canvas, &encoded).map_err(|error| WorldError(js_message(error)))
                };
                let (generation, result) = if let Some(renderer) = slot {
                    renderer
                        .update_gpu(Arc::clone(&document), settings, width, height, publish)
                        .map_err(|error| error.to_string())?
                } else {
                    let mut renderer = WorldRenderer::new(Arc::clone(&document))
                        .map_err(|error| error.to_string())?;
                    let (frame, result) = renderer
                        .prepare_gpu(settings, width, height)
                        .map_err(|error| error.to_string())?;
                    publish(&frame).map_err(|error| error.to_string())?;
                    *slot = Some(renderer);
                    (frame.generation, result)
                };
                painted.set_value(Some((
                    generation,
                    Arc::clone(&document),
                    settings,
                    result.clone(),
                )));
                Ok(result)
            });
            if requested.try_get_untracked() != Some(expected) {
                return;
            }
            match result {
                Some(Ok(rendered)) => {
                    retained_capture.set_value(None);
                    if let Some(capture) = capture {
                        capture.state.set(WorldCaptureState::Idle);
                    }
                    displayed.set(Arc::clone(&document));
                    stats.set(rendered);
                    error.set(String::new());
                    busy.set(false);
                    if let (Some(request), Some(candidate)) = (replacement, candidate) {
                        request.complete.run((candidate, Ok(())));
                    }
                }
                Some(Err(message)) => {
                    if !frame_available(&canvas) {
                        // Genuine device loss / unsuccessful recovery is not a
                        // recoverable candidate refusal. Do not claim old picks.
                        runtime.update_value(|slot| {
                            if let Some(renderer) = slot {
                                renderer.device_reset();
                            }
                        });
                        painted.set_value(None);
                        dispose(&canvas);
                        if let Some(capture) = capture {
                            capture.state.set(WorldCaptureState::Unavailable);
                        }
                    } else if let (Some(capture), Some(previous)) =
                        (capture, retained_capture.get_value())
                    {
                        if matches!(previous, WorldCaptureState::Preparing) {
                            clear_capture(&canvas);
                            capture.state.set(WorldCaptureState::Idle);
                        } else {
                            capture.state.set(previous);
                        }
                    }
                    retained_capture.set_value(None);
                    error.set(message.clone());
                    busy.set(false);
                    if let (Some(request), Some(candidate)) = (replacement, candidate) {
                        request.complete.run((candidate, Err(message)));
                    }
                }
                None => {}
            }
        });
    });
    if let Some(capture) = capture {
        Effect::new(move |_| {
            let (serial, take_photo) = capture.command.get();
            if serial == 0 {
                return;
            }
            let Some(canvas) = canvas.get_untracked() else {
                return;
            };
            clear_capture(&canvas);
            if !take_photo {
                capture.state.set(if busy.get_untracked() {
                    WorldCaptureState::Unavailable
                } else {
                    WorldCaptureState::Idle
                });
                return;
            }
            let Some((generation, document, settings, dimensions)) = painted.get_value() else {
                return;
            };
            if busy.get_untracked()
                || !Arc::ptr_eq(&document, &displayed.get_untracked())
                || settings != controls.get_untracked()
            {
                capture.state.set(WorldCaptureState::Failed(
                    "The view is changing. Capture again when it settles.".into(),
                ));
                return;
            }
            let metadata = match capture_metadata(&document, settings, &dimensions) {
                Ok(metadata) => metadata,
                Err(message) => {
                    capture.state.set(WorldCaptureState::Failed(message));
                    return;
                }
            };
            let frame_request = requested.get_untracked();
            capture.state.set(WorldCaptureState::Preparing);
            wasm_bindgen_futures::spawn_local(async move {
                let result = wasm_bindgen_futures::JsFuture::from(capture_png(
                    &canvas,
                    &generation,
                    &metadata,
                ))
                .await;
                if requested.try_get_untracked() != Some(frame_request)
                    || capture.command.try_get_untracked() != Some((serial, true))
                {
                    return;
                }
                let receipt = result.map_err(js_message).and_then(|value| {
                    let encoded = value
                        .as_string()
                        .filter(|value| value.len() <= 8192)
                        .ok_or_else(|| "Invalid photo receipt.".to_string())?;
                    let receipt: WorldCaptureReceipt = serde_json::from_str(&encoded)
                        .map_err(|_| "Invalid photo receipt.".to_string())?;
                    if !receipt.matches(&generation, dimensions.width, dimensions.height) {
                        return Err("The captured photo did not match the displayed view.".into());
                    }
                    Ok(receipt)
                });
                match receipt {
                    Ok(receipt) => {
                        capture.state.try_set(WorldCaptureState::Ready(receipt));
                    }
                    Err(message) => {
                        clear_capture(&canvas);
                        capture.state.try_set(WorldCaptureState::Failed(message));
                    }
                }
            });
        });
    }
    on_cleanup(move || {
        requested.try_update(|value| *value = value.saturating_add(1));
        runtime.try_update_value(|slot| {
            if let Some(renderer) = slot {
                renderer.device_reset();
            }
        });
        if let Some(canvas) = canvas.get_untracked() {
            dispose(&canvas);
        }
    });
    let choose = move |x: u32, y: u32| {
        if busy.get_untracked() {
            return;
        }
        let Some(canvas) = canvas.get_untracked() else {
            return;
        };
        pick_request.update(|value| *value = value.saturating_add(1));
        let serial = pick_request.get_untracked();
        let frame_request = requested.get_untracked();
        wasm_bindgen_futures::spawn_local(async move {
            let result = wasm_bindgen_futures::JsFuture::from(gpu_pick(&canvas, x, y)).await;
            if requested.try_get_untracked() != Some(frame_request)
                || pick_request.try_get_untracked() != Some(serial)
                || busy.try_get_untracked() != Some(false)
            {
                return;
            }
            let encoded = match result {
                Ok(value) => value.as_string(),
                Err(reason) => {
                    let cancelled = js_sys::Reflect::get(&reason, &JsValue::from_str("name"))
                        .ok()
                        .and_then(|value| value.as_string())
                        .is_some_and(|name| name == "AbortError");
                    if !cancelled {
                        error.set(js_message(reason));
                    }
                    return;
                }
            };
            let Some(encoded) = encoded.filter(|value| value.len() <= 512) else {
                return;
            };
            let Ok(receipt) = serde_json::from_str::<GpuPickReceipt>(&encoded) else {
                return;
            };
            let Ok(generation) = receipt.generation.parse::<u64>() else {
                return;
            };
            if receipt.x != x || receipt.y != y {
                return;
            }
            let picked = runtime
                .try_with_value(|slot| {
                    slot.as_ref().and_then(|renderer| {
                        renderer.resolve_gpu_pick(generation, receipt.index, x, y)
                    })
                })
                .flatten();
            if let Some(picked) = picked {
                on_pick.run(picked);
            }
        });
    };
    view! {
        <style>{include_str!("../public/world.css")}</style>
        <div class="world-viewport" class:world-busy=move ||busy.get() data-source-kind=move ||format!("{:?}",displayed.get().provenance.kind)>
            <canvas node_ref=canvas tabindex="0" aria-label="Source world. Drag to pan, Shift-drag to rotate, or pinch to zoom. Arrow keys pan; Q and E rotate; plus and minus zoom; Page Up and Page Down change floors; Enter selects the center tile."
                on:webglcontextlost=move |_: web_sys::Event| {
                    requested.try_update(|value| *value = value.saturating_add(1));
                    painted.try_update_value(|value| *value = None);
                    retained_capture.try_update_value(|value| *value = None);
                    if let Some(capture) = capture { capture.state.try_set(WorldCaptureState::Unavailable); }
                }
                on:contextmenu=move |event|event.prevent_default()
                on:pointerdown=move |event| {
                    if event.button()!=0 && event.button()!=2 { return; }
                    event.prevent_default();
                    if let Some(canvas)=canvas.get_untracked() { let _=canvas.focus();let _=canvas.set_pointer_capture(event.pointer_id()); }
                    pointers.update_value(|map| {
                        let multiple=!map.is_empty();
                        if multiple { for pointer in map.values_mut() { pointer.moved=true; } }
                        map.insert(event.pointer_id(),PointerTrack { x:event.client_x() as f32,y:event.client_y() as f32,start_x:event.client_x() as f32,start_y:event.client_y() as f32,moved:multiple,rotate:event.shift_key()||event.button()==2 });
                    });
                }
                on:pointermove=move |event| {
                    let gesture=pointers.try_update_value(|map| {
                        let old:Vec<_>=map.values().map(|point|(point.x,point.y)).collect();
                        let pointer=map.get_mut(&event.pointer_id())?;
                        let x=event.client_x() as f32;let y=event.client_y() as f32;
                        let delta=(x-pointer.x,y-pointer.y);pointer.x=x;pointer.y=y;
                        pointer.moved|=(x-pointer.start_x).hypot(y-pointer.start_y)>4.;
                        let rotate=pointer.rotate;
                        let moved=pointer.moved;
                        let new:Vec<_>=map.values().map(|point|(point.x,point.y)).collect();
                        if !moved && new.len()==1 { return None; }
                        if old.len()>=2 && new.len()>=2 {
                            let before=(old[0].0-old[1].0).hypot(old[0].1-old[1].1).max(1.);
                            let after=(new[0].0-new[1].0).hypot(new[0].1-new[1].1).max(1.);
                            Some(Gesture::Pinch(after/before,delta.0/2.,delta.1/2.))
                        } else if rotate { Some(Gesture::Rotate(delta.0,delta.1)) }
                        else { Some(Gesture::Pan(delta.0,delta.1)) }
                    }).flatten();
                    let Some(gesture)=gesture else { return; };
                    let width=canvas.get_untracked().map(|canvas|canvas.get_bounding_client_rect().width() as f32).unwrap_or(800.);
                    let extent=displayed.with_untracked(|document|f32::from(document.lot.width.max(document.lot.height)));
                    controls.update(|controls| match gesture {
                        Gesture::Pan(x,y)=>pan(controls,x,y,extent,width),
                        Gesture::Rotate(x,y)=> { controls.yaw_radians=(controls.yaw_radians+x*0.008).rem_euclid(std::f32::consts::TAU);controls.pitch_radians=(controls.pitch_radians-y*0.006).clamp(0.,2.6); },
                        Gesture::Pinch(scale,x,y)=> { controls.zoom=(controls.zoom*scale).clamp(0.25,8.);pan(controls,x,y,extent,width); },
                    });
                }
                on:pointerup=move |event| {
                    let pointer=pointers.try_update_value(|map|map.remove(&event.pointer_id())).flatten();
                    if let Some(canvas)=canvas.get_untracked() {
                        let _=canvas.release_pointer_capture(event.pointer_id());
                        if pointer.is_some_and(|pointer|!pointer.moved&&!pointer.rotate) && let Some((x,y))=pixel(&canvas,event.client_x(),event.client_y()) { choose(x,y); }
                    }
                }
                on:pointercancel=move |event| { pointers.update_value(|map|{ map.remove(&event.pointer_id()); }); }
                on:wheel=move |event| { event.prevent_default();let scale=(-event.delta_y()*0.0015).exp().clamp(0.67,1.5) as f32;controls.update(|controls|controls.zoom=(controls.zoom*scale).clamp(0.25,8.)); }
                on:keydown=move |event| {
                    if event.is_composing() { return; }
                    let key=event.key();
                    if key=="Enter" { event.prevent_default();if let Some(canvas)=canvas.get_untracked() { choose(canvas.width()/2,canvas.height()/2); }return; }
                    if !matches!(key.as_str(),"ArrowLeft"|"ArrowRight"|"ArrowUp"|"ArrowDown"|"q"|"Q"|"e"|"E"|"+"|"="|"-"|"_"|"PageUp"|"PageDown"|"Home") { return; }
                    event.prevent_default();
                    let levels=displayed.with_untracked(|document|document.lot.levels);
                    controls.update(|controls|match key.as_str() {
                        "ArrowLeft"=>controls.pan_x-=1.,"ArrowRight"=>controls.pan_x+=1.,"ArrowUp"=>controls.pan_y-=1.,"ArrowDown"=>controls.pan_y+=1.,
                        "q"|"Q"=>controls.yaw_radians-=std::f32::consts::FRAC_PI_4,"e"|"E"=>controls.yaw_radians+=std::f32::consts::FRAC_PI_4,
                        "+"|"="=>controls.zoom=(controls.zoom*1.15).min(8.),"-"|"_"=>controls.zoom=(controls.zoom/1.15).max(0.25),
                        "PageUp"=>controls.visible_level=controls.visible_level.saturating_add(1).min(levels),"PageDown"=>controls.visible_level=controls.visible_level.saturating_sub(1).max(1),
                        "Home"=>{ let defaults=ViewportControls::default();controls.yaw_radians=defaults.yaw_radians;controls.pitch_radians=defaults.pitch_radians;controls.zoom=1.;controls.pan_x=0.;controls.pan_y=0.; },
                        _=>{},
                    });
                }
            />
            <svg class="world-draft-outline" viewBox="0 0 1000 1000" preserveAspectRatio="none" aria-hidden="true">
                <polyline points=move || {
                    let (mut points, closed)=draft_projection.get();
                    if closed&& let Some(first)=points.first().copied(){points.push(first);}
                    points.into_iter().map(|(x,y)|format!("{x},{y}")).collect::<Vec<_>>().join(" ")
                } fill=move ||if draft_projection.get().1{"#bfe75a44"}else{"none"}/>
            </svg>
            {move ||draft_projection.get().0.into_iter().map(|(x,y)|view!{<span class="world-draft-point" style= format!("left:{}%;top:{}%",x/10.,y/10.) aria-hidden="true"/>}).collect_view()}
            <div class="world-view-caption" aria-hidden="true"><span>"Source world"</span><span>{move ||format!("{} × {} tiles · Floor {}",displayed.get().lot.width,displayed.get().lot.height,controls.get().visible_level)}</span></div>
            <p class="world-view-hint">"Drag to move · Shift-drag to turn · Scroll or pinch to zoom"</p>
            <Show when=move ||!error.get().is_empty()><p class="world-view-error" role="alert">{move ||error.get()}</p></Show>
            <Show when=move ||!stats.get().diagnostics.is_empty()>
                <details class="world-resource-status"><summary>"Some original scenery is unavailable"</summary><div>{move ||stats.get().diagnostics.into_iter().map(|diagnostic|view!{<p>{diagnostic.message}</p>}).collect_view()}</div></details>
            </Show>
            <span class="world-render-evidence" data-frame-generation=move ||requested.get().to_string() data-triangles=move ||stats.get().triangles.to_string()>"GPU depth-tested source geometry"</span>
        </div>
    }
}
