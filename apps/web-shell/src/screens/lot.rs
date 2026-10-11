use crate::{
    bridge::{Ui, focus_later, selected_character},
    components::{
        Icon,
        actions::ObjectActions,
        chrome::SceneHeader,
        hud::Hud,
        queue::ActionQueue,
        scene::{CameraControls, WorldScene, use_camera},
    },
};
use leptos::prelude::*;
use wonderland_contracts::*;

#[component]
pub fn Lot() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let author = expect_context::<crate::authoring_bridge::AuthorUi>();
    let camera = use_camera();
    let selected = Memo::new(move |_| {
        ui.state.with(|state| {
            state
                .projection
                .objects
                .iter()
                .find(|object| Some(&object.target) == state.selected_object.as_ref())
                .cloned()
        })
    });
    view! {
        <section class="scene-screen lot-screen" aria-label="Harbor Café lot" on:keydown=move |event: web_sys::KeyboardEvent| { if event.key() == "Escape" && selected.get_untracked().is_some() { event.prevent_default(); ui.dismiss_object(); } }>
            <WorldScene camera=camera art="/assets/art/cafe-scene.png" description="Harbor Café interior and a patio overlooking the ocean" on_empty=Callback::new(move |_| { if selected.get_untracked().is_some() { ui.dismiss_object(); } })>
                <div class="lot-character-ring" aria-hidden="true"></div>
                <img class="lot-character" src=move || selected_character(ui).map(|character| author.path(&character.id)).unwrap_or_default() alt=move || selected_character(ui).map(|character| character.name).unwrap_or_default() draggable="false"/>
                <img class="lot-character-diamond" src="/assets/art/selection-diamond.png" alt=""/>
                <For each=move || ui.state.with(|state| state.projection.objects.iter().filter(|object| matches!(&state.screen, Screen::Lot { place_id } if place_id == &object.place_id)).cloned().collect::<Vec<_>>()) key=|object| (object.target.id.clone(), object.target.generation) children=move |object| {
                    let target = object.target.clone(); let selected_target = target.clone(); let pressed_target = target.clone();
                    let focus_anchor = object.anchor;
                    view! {
                        <button id=format!("object-{}", object.target.id) class="object-pick" class:selected=move || ui.state.with(|state| state.selected_object.as_ref() == Some(&selected_target)) aria-label=format!("Select {}", object.name.to_lowercase()) aria-pressed=move || ui.state.with(|state| state.selected_object.as_ref() == Some(&pressed_target)).to_string() aria-controls="object-actions"
                            style=format!("left:{}%;top:{}%", object.anchor.x * 100.0, object.anchor.y * 100.0)
                            on:focus=move |_| camera.update(|camera| { let narrow = camera.viewport.width <= 700.0; camera.reveal(focus_anchor, if narrow { 145.0 } else { 78.0 }, if narrow { 242.0 } else { 150.0 }); })
                            on:click=move |_| { ui.send(UiIntent::SelectObject(target.clone())); focus_later("action-make-coffee".into()); }>
                            <img src="/assets/art/coffee-machine.png" alt="" draggable="false"/>
                            <span class="object-name" style=move || format!("transform:translateX(-50%) scale({})", 1.0 / camera.get().scale())>"Coffee machine"</span>
                        </button>
                    }
                }/>
            </WorldScene>
            <SceneHeader title=Signal::derive(|| String::from("Harbor Café")) icon="coffee" back_label="Back to city"/>
            <ActionQueue/>
            {move || selected.get().map(|object| view! { <ObjectActions camera=camera object=object/> })}
            <Hud lot=true/>
            <div class="lot-modes chrome" role="group" aria-label="Game mode">
                <button class="chrome primary live-mode" aria-pressed="true" aria-label="Live view"><Icon name="home"/><span>"Live"</span></button>
                <button class="chrome unavailable" aria-disabled="true" aria-describedby="build-reason" on:click=move |_| ui.explain("Build mode arrives with object placement.")><Icon name="hammer"/><span>"Build"</span><span id="build-reason" class="unavailable-tip">"Object placement coming later"</span></button>
                <button class="chrome unavailable" aria-disabled="true" aria-describedby="buy-reason" on:click=move |_| ui.explain("Buy mode arrives with the catalog.")><Icon name="shopping-cart"/><span>"Buy"</span><span id="buy-reason" class="unavailable-tip">"Catalog coming later"</span></button>
            </div>
            <CameraControls camera=camera/>
        </section>
    }
}
