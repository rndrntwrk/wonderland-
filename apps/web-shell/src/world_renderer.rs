//! Browser canvas for the same depth-tested source geometry used by native tests.
use leptos::{ev, leptos_dom::helpers::window_event_listener, prelude::*};
use std::{collections::BTreeMap, sync::Arc};
use wasm_bindgen::prelude::*;
use wonderland_world_view::{
    ViewportControls, WorldDocument, WorldPick, WorldRenderStats, WorldRenderer,
};

#[wasm_bindgen(inline_js = r#"
const worlds = new WeakMap();
export function nextWorldPaint() { return new Promise(resolve => requestAnimationFrame(resolve)); }
export function paintSourceWorld(canvas, bytes, width, height) {
  let context=worlds.get(canvas);
  if(!context){context=canvas.getContext('2d',{alpha:false});if(!context)throw new Error('Canvas graphics are unavailable.');worlds.set(canvas,context);}
  if(canvas.width!==width||canvas.height!==height){canvas.width=width;canvas.height=height;}
  context.putImageData(new ImageData(new Uint8ClampedArray(bytes),width,height),0,0);
  canvas.setAttribute('data-renderer','source-software-3d');
}
export function disposeSourceWorld(canvas) {
  const context=worlds.get(canvas);if(context)context.clearRect(0,0,canvas.width,canvas.height);worlds.delete(canvas);
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name=nextWorldPaint)]
    fn next_paint() -> js_sys::Promise;
    #[wasm_bindgen(catch,js_name=paintSourceWorld)]
    fn paint(
        canvas: &web_sys::HtmlCanvasElement,
        bytes: &js_sys::Uint8Array,
        width: u32,
        height: u32,
    ) -> Result<(), JsValue>;
    #[wasm_bindgen(js_name=disposeSourceWorld)]
    fn dispose(canvas: &web_sys::HtmlCanvasElement);
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
) -> impl IntoView {
    let canvas = NodeRef::<leptos::html::Canvas>::new();
    let runtime = StoredValue::new(None::<WorldRenderer>);
    let pointers = StoredValue::new(BTreeMap::<i32, PointerTrack>::new());
    let requested = RwSignal::new(0u64);
    let resize = RwSignal::new(0u64);
    let busy = RwSignal::new(true);
    let error = RwSignal::new(String::new());
    let stats = RwSignal::new(WorldRenderStats::default());
    let draft_projection = Memo::new(move |_| {
        let outline = draft.map(|draft| draft.get()).unwrap_or_default();
        let dimensions = stats.get();
        if dimensions.width == 0 || dimensions.height == 0 || busy.get() {
            return (Vec::<(f32, f32)>::new(), false);
        }
        let document = world.get();
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
        let document = world.get();
        let settings = controls.get();
        resize.get();
        let Some(canvas) = canvas.get() else {
            return;
        };
        requested.update(|value| *value = value.saturating_add(1));
        let expected = requested.get_untracked();
        busy.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(next_paint()).await;
            if requested.try_get_untracked() != Some(expected) {
                return;
            }
            let (width, height) = surface_size(&canvas);
            let result = runtime.try_update_value(|slot| -> Result<WorldRenderStats, String> {
                if let Some(renderer) = slot {
                    renderer
                        .replace_document(Arc::clone(&document))
                        .map_err(|error| error.to_string())?;
                } else {
                    *slot = Some(
                        WorldRenderer::new(Arc::clone(&document))
                            .map_err(|error| error.to_string())?,
                    );
                }
                let renderer = slot.as_mut().expect("admitted world renderer");
                let result = renderer
                    .render(settings, width, height)
                    .map_err(|error| error.to_string())?;
                let image = renderer
                    .image()
                    .ok_or_else(|| "World drawing did not produce a frame.".to_string())?;
                paint(
                    &canvas,
                    &js_sys::Uint8Array::from(image.pixels.as_flattened()),
                    image.width,
                    image.height,
                )
                .map_err(js_message)?;
                Ok(result)
            });
            if requested.try_get_untracked() != Some(expected) {
                return;
            }
            match result {
                Some(Ok(rendered)) => {
                    stats.set(rendered);
                    error.set(String::new());
                    busy.set(false);
                }
                Some(Err(message)) => {
                    runtime.update_value(|slot| {
                        if let Some(renderer) = slot {
                            renderer.device_reset();
                        }
                    });
                    dispose(&canvas);
                    error.set(message);
                    busy.set(false);
                }
                None => {}
            }
        });
    });
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
        let picked = runtime.with_value(|slot| {
            slot.as_ref().and_then(|renderer| {
                renderer
                    .pick(x, y)
                    .and_then(|pick| renderer.resolve_pick(&pick))
            })
        });
        if let Some(picked) = picked {
            on_pick.run(picked);
        }
    };
    view! {
        <style>{include_str!("../public/world.css")}</style>
        <div class="world-viewport" class:world-busy=move ||busy.get() data-source-kind=move ||format!("{:?}",world.get().provenance.kind)>
            <canvas node_ref=canvas tabindex="0" aria-label="Source world. Drag to pan, Shift-drag to rotate, or pinch to zoom. Arrow keys pan; Q and E rotate; plus and minus zoom; Page Up and Page Down change floors; Enter selects the center tile."
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
                    let extent=world.with_untracked(|document|f32::from(document.lot.width.max(document.lot.height)));
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
                    let key=event.key();
                    if key=="Enter" { event.prevent_default();if let Some(canvas)=canvas.get_untracked() { choose(canvas.width()/2,canvas.height()/2); }return; }
                    if !matches!(key.as_str(),"ArrowLeft"|"ArrowRight"|"ArrowUp"|"ArrowDown"|"q"|"Q"|"e"|"E"|"+"|"="|"-"|"_"|"PageUp"|"PageDown"|"Home") { return; }
                    event.prevent_default();
                    let levels=world.with_untracked(|document|document.lot.levels);
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
            <div class="world-view-caption" aria-hidden="true"><span>"Source world"</span><span>{move ||format!("{} × {} tiles · Floor {}",world.get().lot.width,world.get().lot.height,controls.get().visible_level)}</span></div>
            <p class="world-view-hint">"Drag to move · Shift-drag to turn · Scroll or pinch to zoom"</p>
            <Show when=move ||!error.get().is_empty()><p class="world-view-error" role="alert">{move ||error.get()}</p></Show>
            <Show when=move ||!stats.get().diagnostics.is_empty()>
                <details class="world-resource-status"><summary>"Some original scenery is unavailable"</summary><div>{move ||stats.get().diagnostics.into_iter().map(|diagnostic|view!{<p>{diagnostic.message}</p>}).collect_view()}</div></details>
            </Show>
            <span class="world-render-evidence" data-frame-generation=move ||requested.get().to_string() data-triangles=move ||stats.get().triangles.to_string()>"Depth-tested source geometry"</span>
        </div>
    }
}
