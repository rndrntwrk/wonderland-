//! Focus-scoped free-flight input. No gateway or simulation handle is held here.
use leptos::prelude::*;
use std::{collections::BTreeSet, sync::Arc};
use wasm_bindgen::{JsCast, prelude::*};
use wonderland_world_view::{
    ViewportControls, WorldDocument, advance_free_camera, stop_free_camera,
};

#[derive(Clone, Copy)]
pub struct FlightInput {
    keys: StoredValue<BTreeSet<String>>,
    controls: RwSignal<ViewportControls>,
}
fn flight_key(key: &str) -> String {
    match key.to_ascii_lowercase().as_str() {
        "arrowup" => "w",
        "arrowdown" => "s",
        "arrowleft" => "a",
        "arrowright" => "d",
        _ => key,
    }
    .to_ascii_lowercase()
}
impl FlightInput {
    pub fn press(self, key: &str) -> bool {
        let key = flight_key(key);
        if !matches!(
            key.as_str(),
            "w" | "a" | "s" | "d" | "q" | "e" | "shift" | "z" | "x"
        ) {
            return false;
        }
        self.keys.update_value(|keys| {
            keys.insert(key);
        });
        true
    }
    pub fn release(self, key: &str) {
        self.keys.update_value(|keys| {
            keys.remove(&flight_key(key));
        });
    }
    pub fn stop(self) {
        self.keys.try_update_value(BTreeSet::clear);
        if let Some(mut next) = self.controls.try_get_untracked() {
            let old = next;
            stop_free_camera(&mut next);
            if next != old {
                self.controls.try_set(next);
            }
        }
    }
}
pub fn install(
    world: RwSignal<Arc<WorldDocument>>,
    controls: RwSignal<ViewportControls>,
) -> FlightInput {
    let input = FlightInput {
        keys: StoredValue::new(BTreeSet::new()),
        controls,
    };
    let last = StoredValue::new(js_sys::Date::now());
    let previous_mode = StoredValue::new(false);
    Effect::new(move |_| {
        let mode = controls.get().free_camera.is_some();
        if previous_mode.with_value(|old| *old != mode) {
            previous_mode.set_value(mode);
            input.stop();
        }
    });
    let tick = Closure::<dyn FnMut()>::new(move || {
        let now = js_sys::Date::now();
        let elapsed = last.with_value(|last| ((now - last) / 1000.).clamp(0., 0.05) as f32);
        last.set_value(now);
        let visible = web_sys::window()
            .and_then(|w| w.document())
            .is_some_and(|d| !d.hidden());
        let Some(mut next) = controls.try_get_untracked() else {
            return;
        };
        if !visible || next.free_camera.is_none() {
            input.stop();
            return;
        }
        let keys = input.keys.with_value(Clone::clone);
        let axis = |positive: &str, negative: &str| {
            (i32::from(keys.contains(positive)) - i32::from(keys.contains(negative))) as f32
        };
        let movement = [axis("d", "a"), axis("w", "s"), axis("e", "q")];
        if let Some(state) = &mut next.free_camera {
            state.fov_y = (state.fov_y + axis("z", "x") * elapsed)
                .clamp(std::f32::consts::PI * 0.025, std::f32::consts::PI * 0.6);
        }
        let result = world.with_untracked(|world| {
            advance_free_camera(world, &mut next, movement, elapsed, keys.contains("shift"))
        });
        if result.is_err() {
            input.stop();
            return;
        }
        if controls.get_untracked() != next {
            controls.set(next);
        }
    });
    let window = web_sys::window().expect("browser window");
    let interval = window
        .set_interval_with_callback_and_timeout_and_arguments_0(tick.as_ref().unchecked_ref(), 16)
        .ok();
    let blur =
        leptos::leptos_dom::helpers::window_event_listener(leptos::ev::blur, move |_| input.stop());
    let document = window.document();
    let visibility = Closure::<dyn FnMut()>::new(move || {
        if web_sys::window()
            .and_then(|w| w.document())
            .is_none_or(|d| d.hidden())
        {
            input.stop();
        }
    });
    if let Some(document) = &document {
        let _ = document.add_event_listener_with_callback(
            "visibilitychange",
            visibility.as_ref().unchecked_ref(),
        );
    }
    let retained_tick = StoredValue::new_local(Some((tick, visibility)));
    on_cleanup(move || {
        input.stop();
        blur.remove();
        if let Some(id) = interval {
            window.clear_interval_with_handle(id);
        }
        retained_tick.try_update_value(|tick| {
            if let Some((_, visibility)) = tick.take()
                && let Some(document) = &document
            {
                let _ = document.remove_event_listener_with_callback(
                    "visibilitychange",
                    visibility.as_ref().unchecked_ref(),
                );
            }
        });
    });
    input
}
#[component]
pub fn FreeCameraToggle(
    #[prop(into)] world: Signal<Option<Arc<WorldDocument>>>,
    controls: RwSignal<ViewportControls>,
) -> impl IntoView {
    view! {
        <button class="chrome round small" aria-label=move ||if controls.get().free_camera.is_some() {"Orbit camera"} else {"Free camera"} title=move ||if controls.get().free_camera.is_some() {"Return to orbit camera"} else {"Explore with free camera"} aria-pressed=move ||controls.get().free_camera.is_some().to_string()
            disabled=move ||world.get().is_none()
            on:click=move |_| {
                let Some(world) = world.get_untracked() else { return; };
                let aspect = web_sys::window().and_then(|w|w.document())
                    .and_then(|d|d.query_selector(".world-viewport canvas").ok().flatten())
                    .map(|c| { let r=c.get_bounding_client_rect(); (r.width()/r.height().max(1.)) as f32 }).unwrap_or(1.);
                controls.update(|v| { let _=wonderland_world_view::toggle_free_camera(&world,v,aspect); });
            }><crate::components::Icon name="arrows-maximize"/></button>
    }
}
#[component]
pub fn FlightPad(input: FlightInput, controls: RwSignal<ViewportControls>) -> impl IntoView {
    view! {
        <Show when=move ||controls.get().free_camera.is_some()>
            <div class="world-flight-pad chrome" role="group" aria-label="Free camera movement">
                <span>"Drag to look · WASD to fly · Q/E height · Shift faster · Z/X lens"</span>
                { [("w","Fly forward"),("s","Fly backward"),("a","Fly left"),("d","Fly right"),("e","Fly up"),("q","Fly down")].into_iter().map(move |(key,label)| view!{
                    <button class="chrome" aria-label=label
                        on:pointerdown=move |event| { event.prevent_default();input.press(key); }
                        on:pointerup=move |_|input.release(key)
                        on:pointerleave=move |_|input.release(key)
                        on:pointercancel=move |_|input.stop()
                        on:keydown=move |event| { if event.key()==" "||event.key()=="Enter" {event.prevent_default();input.press(key);} }
                        on:keyup=move |_|input.release(key)
                        on:blur=move |_|input.stop()>{label}</button>
                }).collect_view()}
            </div>
        </Show>
    }
}
