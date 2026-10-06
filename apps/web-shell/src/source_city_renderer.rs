//! Browser presentation of original city terrain through the shared CPU renderer.
use crate::{
    components::Icon,
    source_city::{CityMapLot, CityViewport, SourceCity, overview_camera, supported_map},
};
use leptos::prelude::*;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

#[derive(Clone, Copy)]
struct CityFrame {
    viewport: CityViewport,
    camera: wonderland_render_3d::camera::CityCamera,
}
#[wasm_bindgen(inline_js = r#"
let cityChannels;
const cityObservers = new WeakMap();
export function nextCityPaint() { return new Promise(resolve => requestAnimationFrame(resolve)); }
export function observeSourceCity(canvas, focus) {
  disposeSourceCity(canvas);
  const state={frame:0,observer:null,notify:null};
  state.notify=()=>{
    if(state.frame)return;
    state.frame=requestAnimationFrame(()=>{state.frame=0;canvas.dispatchEvent(new Event('resize'));});
  };
  if(typeof ResizeObserver==='function') {
    state.observer=new ResizeObserver(state.notify);
    state.observer.observe(canvas);
    state.observer.observe(focus);
  }
  window.addEventListener('resize',state.notify);
  cityObservers.set(canvas,state);
  state.notify();
}
export function disposeSourceCity(canvas) {
  const state=cityObservers.get(canvas);
  if(!state)return;
  state.observer?.disconnect();
  window.removeEventListener('resize',state.notify);
  if(state.frame)cancelAnimationFrame(state.frame);
  cityObservers.delete(canvas);
}
export async function loadSourceCityChannels() {
  if (!cityChannels) cityChannels = (async () => {
    const names=['elevation','terraintype','foresttype','forestdensity','roadmap','vertexcolor'];
    return await Promise.all(names.map(async name => {
      const response=await fetch('/assets/cities/city_0100/'+name+'.png',{credentials:'same-origin'});
      if (!response.ok) throw new Error('City terrain could not be loaded ('+response.status+').');
      const bitmap=await createImageBitmap(await response.blob());
      try {
        if (bitmap.width!==512||bitmap.height!==512) throw new Error('City terrain has unexpected dimensions.');
        const canvas=document.createElement('canvas');canvas.width=canvas.height=512;
        const context=canvas.getContext('2d',{willReadFrequently:true});
        if(!context)throw new Error('Canvas is unavailable in this browser.');
        context.drawImage(bitmap,0,0);
        return new Uint8Array(context.getImageData(0,0,512,512).data);
      } finally {bitmap.close();}
    }));
  })().catch(error => {cityChannels=null;throw error;});
  return cityChannels;
}
export function paintSourceCity(canvas,bytes,width,height){
  if(canvas.width!==width||canvas.height!==height){canvas.width=width;canvas.height=height;}
  const context=canvas.getContext('2d',{alpha:false});
  if(!context)throw new Error('Canvas is unavailable in this browser.');
  context.putImageData(new ImageData(new Uint8ClampedArray(bytes),width,height),0,0);
  canvas.setAttribute('data-renderer','source-city-software-3d');
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name=nextCityPaint)]
    fn next_paint() -> js_sys::Promise;
    #[wasm_bindgen(js_name=observeSourceCity)]
    fn observe(canvas: &web_sys::HtmlCanvasElement, focus: &web_sys::HtmlElement);
    #[wasm_bindgen(js_name=disposeSourceCity)]
    fn dispose(canvas: &web_sys::HtmlCanvasElement);
    #[wasm_bindgen(js_name=loadSourceCityChannels)]
    fn load_channels() -> js_sys::Promise;
    #[wasm_bindgen(catch,js_name=paintSourceCity)]
    fn paint(
        canvas: &web_sys::HtmlCanvasElement,
        bytes: &js_sys::Uint8Array,
        width: u32,
        height: u32,
    ) -> Result<(), JsValue>;
}
fn error_message(error: JsValue) -> String {
    js_sys::Reflect::get(&error, &JsValue::from_str("message"))
        .ok()
        .and_then(|v| v.as_string())
        .or_else(|| error.as_string())
        .unwrap_or_else(|| "City terrain could not be loaded. Try again.".into())
}
fn zoom_camera(camera: RwSignal<wonderland_render_3d::camera::CityCamera>, factor: f32) {
    camera.update(|c| {
        c.zoom = (c.zoom * factor).clamp(6., 60.);
        c.target_zoom = c.zoom;
    });
}
#[component]
pub fn SourceCityMap(
    #[prop(into)] map: Signal<String>,
    #[prop(into)] lots: Signal<Vec<CityMapLot>>,
    #[prop(into)] selected: Signal<Option<u32>>,
    on_select: Callback<u32>,
) -> impl IntoView {
    // The caller derives the map name from connected state. Memoize its value
    // so directory replies and lot selection cannot reload terrain/reset camera.
    let map_name = Memo::new(move |_| map.get());
    let city = RwSignal::new(None::<Arc<SourceCity>>);
    let status = RwSignal::new("Loading city terrain…".to_string());
    let retry = RwSignal::new(0u32);
    let camera = RwSignal::new(overview_camera());
    let canvas = NodeRef::<leptos::html::Canvas>::new();
    let focus = NodeRef::<leptos::html::Div>::new();
    let frame = RwSignal::new(None::<CityFrame>);
    let requested = RwSignal::new(0u64);
    let resize = RwSignal::new(0u64);
    let generation = RwSignal::new(0u64);
    let drag = RwSignal::new(None::<(i32, i32, i32)>);
    let last_draw = RwSignal::new(0f64);
    Effect::new(move |_| {
        let name = map_name.get();
        retry.get();
        generation.update(|v| *v = v.wrapping_add(1));
        let expected = generation.get_untracked();
        city.set(None);
        frame.set(None);
        camera.set(overview_camera());
        if !supported_map(&name) {
            status.set("The terrain for this city is not available yet. Choose a place from the directory.".into());
            return;
        }
        status.set("Loading city terrain…".into());
        wasm_bindgen_futures::spawn_local(async move {
            let result = wasm_bindgen_futures::JsFuture::from(load_channels()).await;
            if generation.try_get_untracked() != Some(expected) {
                return;
            }
            match result {
                Ok(value) => {
                    let array = js_sys::Array::from(&value);
                    let channels = (0..array.length())
                        .map(|i| js_sys::Uint8Array::new(&array.get(i)).to_vec())
                        .collect::<Vec<_>>();
                    match SourceCity::from_rgba(&channels) {
                        Ok(loaded) => {
                            city.set(Some(Arc::new(loaded)));
                            status.set(String::new());
                        }
                        Err(error) => status.set(error),
                    }
                }
                Err(error) => status.set(error_message(error)),
            }
        });
    });
    Effect::new(move |_| {
        if let (Some(canvas), Some(focus)) = (canvas.get(), focus.get()) {
            observe(&canvas, &focus);
        }
    });
    Effect::new(move |_| {
        resize.get();
        requested.update(|value| *value = value.wrapping_add(1));
        let expected = requested.get_untracked();
        let Some(city) = city.get() else {
            return;
        };
        let camera = camera.get();
        let Some(canvas) = canvas.get() else {
            return;
        };
        let Some(focus) = focus.get() else {
            return;
        };
        wasm_bindgen_futures::spawn_local(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(next_paint()).await;
            if requested.try_get_untracked() != Some(expected) {
                return;
            }
            let rect = canvas.get_bounding_client_rect();
            let Some(viewport) = CityViewport::new(rect.width(), rect.height()) else {
                return;
            };
            let focused = focus.get_bounding_client_rect();
            let viewport = viewport.with_focus(
                focused.left() - rect.left(),
                focused.top() - rect.top(),
                focused.width(),
                focused.height(),
            );
            match city.render_in_viewport(camera, viewport) {
                Ok(surface) => {
                    if let Err(error) = paint(
                        &canvas,
                        &js_sys::Uint8Array::from(surface.image().pixels.as_flattened()),
                        viewport.width,
                        viewport.height,
                    ) {
                        frame.set(None);
                        status.set(error_message(error));
                    } else {
                        // Publish camera and dimensions together only after the
                        // matching terrain frame is painted; markers cannot lead it.
                        frame.set(Some(CityFrame { viewport, camera }));
                        status.set(String::new());
                    }
                }
                Err(error) => {
                    frame.set(None);
                    status.set(error);
                }
            }
        });
    });
    let pins = Memo::new(move |_| {
        let Some(city) = city.get() else {
            return vec![];
        };
        let Some(frame) = frame.get() else {
            return vec![];
        };
        let viewport = frame.viewport;
        let Some(matrix) = viewport.projection(frame.camera) else {
            return vec![];
        };
        lots.get()
            .into_iter()
            .filter_map(|lot| {
                city.pin(&lot, matrix, viewport.width, viewport.height)
                    .map(|pin| (lot, pin, viewport))
            })
            .collect::<Vec<_>>()
    });
    let focus_selected = move |_| {
        let Some(id) = selected.get_untracked() else {
            return;
        };
        let Some(lot) = lots
            .get_untracked()
            .into_iter()
            .find(|lot| lot.lot_id == id)
        else {
            return;
        };
        let Some(city) = city.get_untracked() else {
            return;
        };
        if let Some(pos) = city.lot_position(lot.location) {
            camera.update(|c| {
                c.center = wonderland_render_core::Vec2::new(pos.x, pos.z);
                c.cam_height = pos.y - 0.5;
                c.zoom = 9.;
                c.target_zoom = 9.;
            });
        }
    };
    let reset_overview = move || {
        camera.set(match (city.get_untracked(), frame.get_untracked()) {
            (Some(city), Some(frame)) => city.overview(frame.viewport),
            _ => overview_camera(),
        });
    };
    on_cleanup(move || {
        generation.try_update(|v| *v = v.wrapping_add(1));
        requested.try_update(|v| *v = v.wrapping_add(1));
        if let Some(canvas) = canvas.get_untracked() {
            dispose(&canvas);
        }
    });
    view! {
      <section class="source-city" aria-label="City map">
        <div class="source-city-viewport" tabindex="0" aria-label="City terrain. Drag to pan; use arrow keys to pan and plus or minus to zoom."
          class:dragging=move ||drag.get().is_some()
          on:keydown=move |e:web_sys::KeyboardEvent| {
            let delta=match e.key().as_str(){"ArrowLeft"=>Some((64.,0.)),"ArrowRight"=>Some((-64.,0.)),"ArrowUp"=>Some((0.,64.)),"ArrowDown"=>Some((0.,-64.)),_=>None};
            if let Some((x,y))=delta{e.prevent_default();if let Some(frame)=frame.get_untracked(){camera.update(|c|*c=frame.viewport.pan(*c,x,y));}}
            else{match e.key().as_str(){"+"|"="=>{e.prevent_default();zoom_camera(camera,0.85);},"-"=>{e.prevent_default();zoom_camera(camera,1.15);},"Home"=>{e.prevent_default();reset_overview();},_=>{}}}
          }
          on:wheel=move |e:web_sys::WheelEvent|{e.prevent_default();zoom_camera(camera,if e.delta_y()<0. {0.92}else{1.08});}
          on:pointerdown=move |e:web_sys::PointerEvent| {
            if e.button()!=0{return;}let target=event_target::<web_sys::Element>(&e);
            if target.closest("button").ok().flatten().is_some(){return;}
            if let Some(viewport)=canvas.get_untracked().and_then(|c|c.parent_element()){let _=viewport.set_pointer_capture(e.pointer_id());}
            drag.set(Some((e.pointer_id(),e.client_x(),e.client_y())));last_draw.set(e.time_stamp());
          }
          on:pointermove=move |e:web_sys::PointerEvent| {
            let Some((id,x,y))=drag.get_untracked() else{return;};if id!=e.pointer_id()||e.time_stamp()-last_draw.get_untracked()<90. {return;}
            if let Some(frame)=frame.get_untracked(){camera.update(|c|*c=frame.viewport.pan(*c,f64::from(e.client_x()-x),f64::from(e.client_y()-y)));}
            drag.set(Some((id,e.client_x(),e.client_y())));last_draw.set(e.time_stamp());
          }
          on:pointerup=move |e:web_sys::PointerEvent| {
            if let Some((id,x,y))=drag.get_untracked()&& id==e.pointer_id(){
                if let Some(frame)=frame.get_untracked(){camera.update(|c|*c=frame.viewport.pan(*c,f64::from(e.client_x()-x),f64::from(e.client_y()-y)));}
                drag.set(None);
                if let Some(viewport)=canvas.get_untracked().and_then(|c|c.parent_element()){let _=viewport.release_pointer_capture(id);}
            }
          }
          on:pointercancel=move |_|drag.set(None)
          on:lostpointercapture=move |_|drag.set(None)>
          <canvas class="source-city-terrain" node_ref=canvas width="1" height="1" aria-label="Original city terrain" style:visibility=move ||if frame.get().is_some(){"visible"}else{"hidden"} on:resize=move |_|{
            if let Some((id,_,_))=drag.get_untracked()&&let Some(viewport)=canvas.get_untracked().and_then(|c|c.parent_element()){let _=viewport.release_pointer_capture(id);}
            drag.set(None);resize.update(|value|*value=value.wrapping_add(1));
          }/>
          <div class="source-city-focus" node_ref=focus aria-hidden="true"></div>
          <div class="source-city-pins">
            <For each=move ||pins.get() key=|(lot,pin,viewport)|(lot.lot_id,lot.location,pin.point.x.to_bits(),pin.point.y.to_bits(),viewport.width,viewport.height,lot.name.clone(),lot.online) children=move |(lot,pin,viewport)| {
              let id=lot.lot_id;let name=lot.name.clone();let title=lot.name.clone();
              let online=lot.online.is_some_and(|n|n>0);let label=format!("Select {}",name);
              view!{<button class="source-city-pin" class:selected=move ||selected.get()==Some(id) class:online=online
                style=format!("left:{}%;top:{}%;",pin.point.x/viewport.width as f32*100.,pin.point.y/viewport.height as f32*100.)
                title=title aria-label=label aria-pressed=move ||selected.get()==Some(id)
                on:click=move |_|on_select.run(id)><span class="source-city-pin-dot" aria-hidden="true"></span><span class="source-city-pin-name">{name}</span></button>}
            }/>
          </div>
          <Show when=move ||!status.get().is_empty()><div class="source-city-status" role="status"><p>{move ||status.get()}</p><Show when=move ||supported_map(&map_name.get())&&city.get().is_none()&&!status.get().starts_with("Loading")><button class="chrome" on:click=move |_|retry.update(|v|*v=v.wrapping_add(1))>"Try again"</button></Show></div></Show>
        </div>
        <Show when=move ||city.get().is_some()&&status.get().is_empty()><div class="source-city-controls"><div class="source-city-zoom" role="group" aria-label="City map controls"><button class="chrome source-city-zoom-step" aria-label="Zoom in" title="Zoom in" on:click=move |_|zoom_camera(camera,0.82)><Icon name="plus"/></button><button class="chrome source-city-zoom-step" aria-label="Zoom out" title="Zoom out" on:click=move |_|zoom_camera(camera,1.2)><Icon name="minus"/></button><button class="chrome" on:click=move |_|reset_overview()>"Whole city"</button><button class="chrome" disabled=move ||selected.get().is_none() on:click=focus_selected>"Selected place"</button></div><span>"Drag to explore · Select a place"</span></div></Show>
      </section>
    }
}
