use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wonderland_contracts::Screen;

use crate::{
    bridge::{Overlay, Ui},
    components::{Icon, hud::NeedsGrid},
    fixture::Scenario,
    screens::{avatars::Avatars, city::City, creator::Creator, home::HomeScreen, lot::Lot},
};

#[component]
pub fn App() -> impl IntoView {
    let query = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default();
    let value = web_sys::UrlSearchParams::new_with_str(&query)
        .ok()
        .and_then(|params| params.get("fixture"));
    let scenario = match value.as_deref() {
        Some("reject-travel") => Scenario::RejectTravel,
        Some("reject-interaction") => Scenario::RejectInteraction,
        Some("reject-cancellation") => Scenario::RejectCancellation,
        Some("empty-characters") => Scenario::EmptyCharacters,
        Some("unavailable-characters") => Scenario::UnavailableCharacters,
        _ => Scenario::Accept,
    };
    let ui = Ui::new(scenario);
    provide_context(ui);
    let author = crate::authoring_bridge::AuthorUi::new(ui, value.as_deref());
    provide_context(author);
    provide_context(crate::avatar_content::ContentUi::new());
    let character_editor = Memo::new(move |_| {
        author.state.with(|s| {
            matches!(
                s.draft(),
                Some(
                    wonderland_contracts::authoring::AuthoringDraft::Creation(_)
                        | wonderland_contracts::authoring::AuthoringDraft::Outfit(_)
                )
            )
        })
    });
    let screen = Memo::new(move |_| ui.state.with(|state| state.screen.clone()));
    Effect::new(move |_| {
        let active = ui.overlay.get();
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        for (id, target) in [
            ("settings-dialog", Overlay::Settings),
            ("needs-dialog", Overlay::Needs),
        ] {
            if let Some(dialog) = document
                .get_element_by_id(id)
                .and_then(|element| element.dyn_into::<web_sys::HtmlDialogElement>().ok())
            {
                if active == target {
                    if !dialog.open() {
                        let _ = dialog.show_modal();
                    }
                } else {
                    dialog.close();
                }
            }
        }
    });

    view! {
        <main class="game-shell" class:reduce-motion=move || ui.reduced_motion.get()>
            {move || if character_editor.get() { view! { <Creator/> }.into_any() } else { match screen.get() {
                Screen::CharacterSelection => view! { <Avatars/> }.into_any(),
                Screen::City => view! { <City/> }.into_any(),
                Screen::Lot { place_id } if place_id.as_ref() == "home" => view! { <HomeScreen/> }.into_any(),
                Screen::Lot { .. } => view! { <Lot/> }.into_any(),
            }}}
            <Show when=move || !author.storage_notice.get().is_empty()><p class="storage-notice" role="status">{move || author.storage_notice.get()}</p></Show>
            <crate::components::player_menu::PlayerMenu/>
            <div class="top-tools">
                <button class="chrome round unavailable" aria-label="Sound unavailable: preview has no audio" aria-disabled="true" title="Audio arrives with the game renderer" on:click=move |_| ui.explain("Audio arrives with the game renderer.")><Icon name="volume-off"/></button>
                <button id="settings" class="chrome round" aria-label="Settings" on:click=move |_| ui.overlay.set(Overlay::Settings)><Icon name="settings"/></button>
            </div>
            <span class="preview-label">"UI preview"</span>
            <div class="sr-only" role="status" aria-live="polite" aria-atomic="true">{move || ui.announcement.get()}</div>
            <Show when=move || ui.notice.get().is_some()>
                <div class="feedback" role="alert"><Icon name="alert-circle"/><span>{move || ui.notice.get().unwrap_or_default()}</span><button aria-label="Dismiss message" on:click=move |_| ui.notice.set(None)><Icon name="x"/></button></div>
            </Show>
            <dialog id="settings-dialog" class="game-dialog settings-dialog" aria-labelledby="settings-title" on:cancel=move |event: web_sys::Event| { event.prevent_default(); ui.close_overlay(); }>
                <div class="dialog-heading"><h2 id="settings-title">"Settings"</h2><button class="chrome round small" aria-label="Close settings" on:click=move |_| ui.close_overlay()><Icon name="x"/></button></div>
                <label class="setting-row"><span>"Reduce motion"</span><input type="checkbox" prop:checked=move || ui.reduced_motion.get() on:change=move |event| ui.reduced_motion.set(event_target_checked(&event))/></label>
                <p>"Drag the scene to look around. Use + and − to zoom, or reset the view."</p>
            </dialog>
            <dialog id="needs-dialog" class="game-dialog needs-dialog" aria-labelledby="needs-title" on:cancel=move |event: web_sys::Event| { event.prevent_default(); ui.close_overlay(); }>
                <div class="dialog-heading"><h2 id="needs-title">"All needs"</h2><button class="chrome round small" aria-label="Close all needs" on:click=move |_| ui.close_overlay()><Icon name="x"/></button></div>
                <NeedsGrid all=true/>
            </dialog>
        </main>
    }
}
