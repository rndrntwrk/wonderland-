//! The same affine camera renders the artwork, semantic picks, and menu anchors.
use crate::{
    components::Icon,
    geometry::{Camera, Point, Size, WORLD_HEIGHT, WORLD_WIDTH},
};
use leptos::{ev, leptos_dom::helpers::window_event_listener, prelude::*};
use wasm_bindgen::JsCast;

pub fn use_camera() -> RwSignal<Camera> {
    let camera = RwSignal::new(Camera::new(viewport_size()));
    let listener = window_event_listener(ev::resize, move |_| {
        camera.update(|camera| camera.resize(viewport_size()))
    });
    on_cleanup(move || listener.remove());
    camera
}

fn viewport_size() -> Size {
    let window = web_sys::window();
    Size {
        width: window
            .as_ref()
            .and_then(|window| window.inner_width().ok())
            .and_then(|width| width.as_f64())
            .unwrap_or(1440.0),
        height: window
            .and_then(|window| window.inner_height().ok())
            .and_then(|height| height.as_f64())
            .unwrap_or(900.0),
    }
}

#[component]
pub fn WorldScene(
    camera: RwSignal<Camera>,
    #[prop(into)] art: String,
    #[prop(into)] description: String,
    on_empty: Callback<()>,
    children: Children,
) -> impl IntoView {
    let element = NodeRef::<leptos::html::Div>::new();
    let drag = RwSignal::new(None::<Point>);
    let moved = RwSignal::new(false);
    let start = move |event: web_sys::PointerEvent| {
        if !event.is_primary() || event.button() != 0 {
            return;
        }
        if event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
            .and_then(|target| target.closest("button").ok().flatten())
            .is_some()
        {
            return;
        }
        moved.set(false);
        drag.set(Some(Point {
            x: f64::from(event.client_x()),
            y: f64::from(event.client_y()),
        }));
        if let Some(element) = element.get() {
            let _ = element.set_pointer_capture(event.pointer_id());
        }
    };
    let move_pointer = move |event: web_sys::PointerEvent| {
        if let Some(previous) = drag.get_untracked() {
            let point = Point {
                x: f64::from(event.client_x()),
                y: f64::from(event.client_y()),
            };
            let delta = Point {
                x: point.x - previous.x,
                y: point.y - previous.y,
            };
            if delta.x.abs() + delta.y.abs() > 2.0 {
                moved.set(true);
            }
            camera.update(|camera| camera.pan_by(delta));
            drag.set(Some(point));
        }
    };
    let release = move |event: web_sys::PointerEvent| {
        drag.set(None);
        if let Some(element) = element.get() {
            let _ = element.release_pointer_capture(event.pointer_id());
        }
    };
    view! {
        <div id="scene-canvas" class="world-viewport" class:dragging=move || drag.get().is_some() node_ref=element tabindex="0" role="group" aria-label="Scene view. Drag to pan, use arrow keys to pan, plus and minus to zoom, or Home to reset."
            on:pointerdown=start on:pointermove=move_pointer on:pointerup=release on:pointercancel=release
            on:click=move |event| {
                let button = event.target().and_then(|target| target.dyn_into::<web_sys::Element>().ok()).and_then(|target| target.closest("button").ok().flatten()).is_some();
                if !button && !moved.get_untracked() { on_empty.run(()); }
            }
            on:wheel=move |event: web_sys::WheelEvent| {
                event.prevent_default();
                let factor = if event.delta_y() < 0.0 { 1.12 } else { 1.0 / 1.12 };
                camera.update(|camera| camera.zoom_at(factor, Point { x: f64::from(event.client_x()), y: f64::from(event.client_y()) }));
            }
            on:keydown=move |event: web_sys::KeyboardEvent| {
                let direct = event.target().and_then(|target| target.dyn_into::<web_sys::Element>().ok()).is_some_and(|target| target.id() == "scene-canvas");
                if !direct { return; }
                let delta = match event.key().as_str() {
                    "ArrowLeft" => Some(Point { x: 60.0, y: 0.0 }), "ArrowRight" => Some(Point { x: -60.0, y: 0.0 }),
                    "ArrowUp" => Some(Point { x: 0.0, y: 60.0 }), "ArrowDown" => Some(Point { x: 0.0, y: -60.0 }), _ => None,
                };
                if let Some(delta) = delta { event.prevent_default(); camera.update(|camera| camera.pan_by(delta)); }
                else if matches!(event.key().as_str(), "+" | "=" | "-" | "Home") {
                    event.prevent_default();
                    camera.update(|camera| {
                        if event.key() == "Home" { camera.reset(); }
                        else { camera.zoom_at(if event.key() == "-" { 1.0 / 1.2 } else { 1.2 }, Point { x: camera.viewport.width / 2.0, y: camera.viewport.height / 2.0 }); }
                    });
                }
            }>
            <div class="world-plane" style=move || {
                let camera = camera.get(); let origin = camera.origin();
                format!("width:{WORLD_WIDTH}px;height:{WORLD_HEIGHT}px;transform:translate({}px,{}px) scale({});--camera-scale:{}", origin.x, origin.y, camera.scale(), camera.scale())
            }>
                <img class="world-art" src=art alt=description draggable="false"/>
                {children()}
            </div>
        </div>
    }
}

#[component]
pub fn CameraControls(camera: RwSignal<Camera>) -> impl IntoView {
    view! {
        <nav class="camera-controls chrome" aria-label="Scene camera">
            <button class="chrome round" aria-label="Zoom out" disabled=move || { camera.get().zoom <= 1.0 } on:click=move |_| camera.update(|camera| camera.zoom_at(1.0 / 1.2, Point { x: camera.viewport.width / 2.0, y: camera.viewport.height / 2.0 }))><Icon name="minus"/></button>
            <button class="chrome round" aria-label="Zoom in" disabled=move || { camera.get().zoom >= 2.8 } on:click=move |_| camera.update(|camera| camera.zoom_at(1.2, Point { x: camera.viewport.width / 2.0, y: camera.viewport.height / 2.0 }))><Icon name="plus"/></button>
            <button class="chrome round" aria-label="Reset view" on:click=move |_| camera.update(|camera| camera.reset())><Icon name="rotate-clockwise"/></button>
        </nav>
    }
}
