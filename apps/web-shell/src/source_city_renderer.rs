//! Browser presentation of original city terrain through the shared CPU renderer.
use crate::source_city::{
    CityMapLot, SourceCity, camera_matrix, overview_camera, pan_camera, supported_map,
};
use leptos::prelude::*;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

const WIDTH: u32 = 768;
const HEIGHT: u32 = 480;
#[wasm_bindgen(inline_js = r#"
let cityChannels;
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
  canvas.width=width;canvas.height=height;
  const context=canvas.getContext('2d',{alpha:false});
  if(!context)throw new Error('Canvas is unavailable in this browser.');
  context.putImageData(new ImageData(new Uint8ClampedArray(bytes),width,height),0,0);
}
"#)]
extern "C" {
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
        c.zoom = (c.zoom * factor).clamp(6., 26.);
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
    let city = RwSignal::new(None::<Arc<SourceCity>>);
    let status = RwSignal::new("Loading city terrain…".to_string());
    let retry = RwSignal::new(0u32);
    let camera = RwSignal::new(overview_camera());
    let canvas = NodeRef::<leptos::html::Canvas>::new();
    let generation = RwSignal::new(0u64);
    let drag = RwSignal::new(None::<(i32, i32, i32)>);
    let last_draw = RwSignal::new(0f64);
    Effect::new(move |_| {
        let name = map.get();
        retry.get();
        generation.update(|v| *v = v.wrapping_add(1));
        let expected = generation.get_untracked();
        city.set(None);
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
        let Some(city) = city.get() else {
            return;
        };
        let camera = camera.get();
        let Some(canvas) = canvas.get() else {
            return;
        };
        match city.render(camera, WIDTH, HEIGHT) {
            Ok(surface) => {
                let bytes = surface
                    .image()
                    .pixels
                    .iter()
                    .flat_map(|p| p.iter().copied())
                    .collect::<Vec<_>>();
                if let Err(error) = paint(
                    &canvas,
                    &js_sys::Uint8Array::from(bytes.as_slice()),
                    WIDTH,
                    HEIGHT,
                ) {
                    status.set(error_message(error));
                }
            }
            Err(error) => status.set(error),
        }
    });
    let pins = Memo::new(move |_| {
        let Some(city) = city.get() else {
            return vec![];
        };
        let Some(matrix) = camera_matrix(camera.get(), WIDTH, HEIGHT) else {
            return vec![];
        };
        lots.get()
            .into_iter()
            .filter_map(|lot| city.pin(&lot, matrix, WIDTH, HEIGHT).map(|pin| (lot, pin)))
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
                c.cam_height = pos.y;
                c.zoom = 9.;
                c.target_zoom = 9.;
            });
        }
    };
    on_cleanup(move || {
        generation.try_update(|v| *v = v.wrapping_add(1));
    });
    view! {
      <section class="source-city" aria-label="City map">
        <div class="source-city-viewport" tabindex="0" aria-label="City terrain. Drag to pan; use arrow keys to pan and plus or minus to zoom."
          class:dragging=move ||drag.get().is_some()
          on:keydown=move |e:web_sys::KeyboardEvent| {
            let delta=match e.key().as_str(){"ArrowLeft"=>Some((64.,0.)),"ArrowRight"=>Some((-64.,0.)),"ArrowUp"=>Some((0.,64.)),"ArrowDown"=>Some((0.,-64.)),_=>None};
            if let Some((x,y))=delta{e.prevent_default();camera.update(|c|*c=pan_camera(*c,x,y,WIDTH,HEIGHT));}
            else{match e.key().as_str(){"+"|"="=>{e.prevent_default();zoom_camera(camera,0.85);},"-"=>{e.prevent_default();zoom_camera(camera,1.15);},"Home"=>{e.prevent_default();camera.set(overview_camera());},_=>{}}}
          }
          on:wheel=move |e:web_sys::WheelEvent|{e.prevent_default();zoom_camera(camera,if e.delta_y()<0. {0.92}else{1.08});}
          on:pointerdown=move |e:web_sys::PointerEvent| {
            if e.button()!=0{return;}let target=event_target::<web_sys::Element>(&e);
            if target.closest("button").ok().flatten().is_some(){return;}
            if let Some(canvas)=canvas.get_untracked(){let _=canvas.parent_element().unwrap().set_pointer_capture(e.pointer_id());}
            drag.set(Some((e.pointer_id(),e.client_x(),e.client_y())));last_draw.set(e.time_stamp());
          }
          on:pointermove=move |e:web_sys::PointerEvent| {
            let Some((id,x,y))=drag.get_untracked() else{return;};if id!=e.pointer_id()||e.time_stamp()-last_draw.get_untracked()<90. {return;}
            let scale=canvas.get_untracked().map(|c|WIDTH as f32/c.get_bounding_client_rect().width() as f32).unwrap_or(1.);
            camera.update(|c|*c=pan_camera(*c,(e.client_x()-x) as f32*scale,(e.client_y()-y) as f32*scale,WIDTH,HEIGHT));drag.set(Some((id,e.client_x(),e.client_y())));last_draw.set(e.time_stamp());
          }
          on:pointerup=move |e:web_sys::PointerEvent| {
            if let Some((id,x,y))=drag.get_untracked()&& id==e.pointer_id(){
                let scale=canvas.get_untracked().map(|c|WIDTH as f32/c.get_bounding_client_rect().width() as f32).unwrap_or(1.);
                camera.update(|c|*c=pan_camera(*c,(e.client_x()-x) as f32*scale,(e.client_y()-y) as f32*scale,WIDTH,HEIGHT));drag.set(None);
            }
          }
          on:pointercancel=move |_|drag.set(None)>
          <canvas class="source-city-terrain" node_ref=canvas width=WIDTH height=HEIGHT aria-label="Original city terrain"/>
          <div class="source-city-pins">
            <For each=move ||pins.get() key=|(lot,pin)|(lot.lot_id,lot.location,pin.point.x.to_bits(),pin.point.y.to_bits(),lot.name.clone(),lot.online) children=move |(lot,pin)| {
              let id=lot.lot_id;let name=lot.name.clone();let title=lot.name.clone();
              let online=lot.online.is_some_and(|n|n>0);let label=format!("Select {}",name);
              view!{<button class="source-city-pin" class:selected=move ||selected.get()==Some(id) class:online=online
                style=format!("left:{}%;top:{}%;",pin.point.x/WIDTH as f32*100.,pin.point.y/HEIGHT as f32*100.)
                title=title aria-label=label aria-pressed=move ||selected.get()==Some(id)
                on:click=move |_|on_select.run(id)><span class="source-city-pin-dot" aria-hidden="true"></span><span class="source-city-pin-name">{name}</span></button>}
            }/>
          </div>
          <Show when=move ||!status.get().is_empty()><div class="source-city-status" role="status"><p>{move ||status.get()}</p><Show when=move ||supported_map(&map.get())&&city.get().is_none()&&!status.get().starts_with("Loading")><button class="chrome" on:click=move |_|retry.update(|v|*v=v.wrapping_add(1))>"Try again"</button></Show></div></Show>
        </div>
        <Show when=move ||city.get().is_some()><div class="source-city-controls"><div class="source-city-zoom"><button class="chrome" aria-label="Zoom in" on:click=move |_|zoom_camera(camera,0.82)>"+"</button><button class="chrome" aria-label="Zoom out" on:click=move |_|zoom_camera(camera,1.2)>"−"</button><button class="chrome" on:click=move |_|camera.set(overview_camera())>"Whole city"</button><button class="chrome" disabled=move ||selected.get().is_none() on:click=focus_selected>"Selected place"</button></div><span>"Drag to explore · Select a place"</span></div></Show>
      </section>
    }
}
