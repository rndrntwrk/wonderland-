use crate::{
    bridge::{Ui, focus_later},
    components::{
        Icon, availability_reason,
        chrome::SceneHeader,
        hud::Hud,
        scene::{CameraControls, WorldScene, use_camera},
    },
    geometry::{Point, Size, clamp_overlay},
};
use leptos::prelude::*;
use wonderland_contracts::*;

#[component]
pub fn City() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let camera = use_camera();
    let placard_open = RwSignal::new(
        ui.state
            .with_untracked(|state| state.selected_place.is_some()),
    );
    let selected = Memo::new(move |_| {
        ui.state.with(|state| {
            state
                .projection
                .places
                .iter()
                .find(|place| Some(&place.id) == state.selected_place.as_ref())
                .cloned()
        })
    });
    let dismiss = move || {
        placard_open.set(false);
        if let Some(place) = selected.get_untracked() {
            focus_later(format!("place-{}", place.id));
        }
    };
    view! {
        <section class="scene-screen city-screen" aria-label="City map" on:keydown=move |event: web_sys::KeyboardEvent| { if event.key() == "Escape" { event.prevent_default(); dismiss(); } }>
            <WorldScene camera=camera art="/assets/art/city-map.png" description="Quack's Creek, a coastal town with a café, park, arcade and homes" on_empty=Callback::new(move |_| placard_open.set(false))>
                <For each=move || ui.state.with(|state| state.projection.places.clone()) key=|place| place.id.clone() children=move |place| {
                    let id = place.id.clone(); let select_id = id.clone(); let pressed_id = id.clone(); let label_id = id.clone();
                    let anchor = place.anchor;
                    let focus_anchor = anchor;
                    let footprint = match id.as_ref() { "park" => "park", "arcade" => "arcade", "home" => "home", _ => "cafe" };
                    view! {
                        <button id=format!("place-{id}") class=format!("place-target footprint-{footprint}") class:selected=move || ui.state.with(|state| state.selected_place.as_ref() == Some(&pressed_id))
                            style=format!("left:{}%;top:{}%", anchor.x * 100.0, anchor.y * 100.0)
                            aria-label=format!("Select {}", place.name) aria-pressed=move || ui.state.with(|state| state.selected_place.as_ref() == Some(&label_id)).to_string()
                            on:focus=move |_| camera.update(|camera| camera.reveal(focus_anchor, if camera.viewport.width <= 700.0 { 142.0 } else { 76.0 }, 137.0))
                            on:click=move |_| { ui.send(UiIntent::SelectPlace(select_id.clone())); placard_open.set(true); }>
                            <span class="lot-footprint" aria-hidden="true"></span>
                            <span class="place-label" style=move || format!("transform:translate(-50%,-100%) scale({})", 1.0 / camera.get().scale())><Icon name=place_icon(place.id.as_ref())/>{place.name.clone()}</span>
                        </button>
                    }
                }/>
            </WorldScene>
            <SceneHeader title=Signal::derive(move || ui.state.with(|state| state.projection.city_name.clone())) icon="building-community" back_label="Back to character selection"/>
            <nav class="mobile-destinations" aria-label="Map destinations">
                <For each=move || ui.state.with(|state| state.projection.places.clone()) key=|place| place.id.clone() children=move |place| {
                    let id = place.id.clone(); let press_id = id.clone(); let anchor = place.anchor;
                    view! { <button class="place-chip" aria-pressed=move || ui.state.with(|state| state.selected_place.as_ref() == Some(&press_id)).to_string() on:click=move |_| { camera.update(|camera| camera.focus(anchor)); ui.send(UiIntent::SelectPlace(id.clone())); placard_open.set(true); }><Icon name=place_icon(place.id.as_ref())/>{place.name}</button> }
                }/>
            </nav>
            <Show when=move || placard_open.get() && selected.get().is_some()>
                <div class="place-placard chrome" role="group" aria-label="Selected destination" style=move || {
                    let camera = camera.get();
                    let Some(place) = selected.get() else { return String::new(); };
                    let point = camera.project(place.anchor);
                    let height = if place.availability.is_available() { 80.0 } else { 116.0 };
                    let position = clamp_overlay(Point { x: point.x - 150.0, y: point.y - 74.0 * camera.scale() - height }, Size { width: 300.0, height }, camera.viewport, if camera.viewport.width <= 700.0 { 142.0 } else { 76.0 }, 137.0);
                    format!("left:{}px;top:{}px", position.x, position.y)
                }>
                    <div class="place-summary"><Icon name=move || selected.get().map(|place| place_icon(place.id.as_ref())).unwrap_or("map-pin").to_string()/><div><h2>{move || selected.get().map(|place| place.name).unwrap_or_default()}</h2><p><Icon name="users"/>{move || selected.get().map(|place| format!("{} visiting", place.population)).unwrap_or_default()}</p></div></div>
                    <button class="visit-button" disabled=move || selected.get().is_none_or(|place| !place.availability.is_available()) || ui.state.with(|state| state.pending_requests.values().any(|pending| matches!(pending.request.kind, RequestKind::Travel { .. }))) on:click=move |_| ui.send(UiIntent::Visit)>
                        {move || if ui.state.with(|state| state.pending_requests.values().any(|pending| matches!(pending.request.kind, RequestKind::Travel { .. }))) { "Visiting…" } else { "Visit" }}<Icon name="chevron-right"/>
                    </button>
                    <Show when=move || selected.get().and_then(|place| availability_reason(&place.availability)).is_some()><p class="place-unavailable">{move || selected.get().and_then(|place| availability_reason(&place.availability)).unwrap_or_default()}</p></Show>
                </div>
            </Show>
            <Hud/>
            <CameraControls camera=camera/>
        </section>
    }
}

fn place_icon(id: &str) -> &'static str {
    match id {
        "harbor-cafe" => "coffee",
        "park" => "tree",
        "arcade" => "device-gamepad-2",
        "home" => "home",
        _ => "map-pin",
    }
}
