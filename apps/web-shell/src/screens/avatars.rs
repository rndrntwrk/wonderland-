use crate::{
    bridge::{Ui, selected_character},
    components::{Icon, availability_reason, portrait_path},
};
use leptos::prelude::*;
use wonderland_contracts::UiIntent;

#[component]
pub fn Avatars() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let available = move || {
        selected_character(ui).is_some_and(|character| character.availability.is_available())
            && ui.state.with(|state| state.projection.validate().is_ok())
    };
    view! {
        <section class="avatars-screen" aria-label="Character selection">
            <h1 id="screen-title" class="choose-title" tabindex="-1">"Choose your Sim"</h1>
            <div class="avatar-grid" role="group" aria-label="Choose a Sim">
                <For each=move || ui.state.with(|state| state.projection.characters.clone()) key=|character| character.id.clone() children=move |character| {
                    let id = character.id.clone();
                    let selection_id = id.clone();
                    let click_id = id.clone();
                    let availability_id = id.clone();
                    let name = character.name.clone();
                    view! {
                        <button id=format!("avatar-{id}") class="avatar-card" class:selected=move || ui.state.with(|state| state.selected_character.as_ref() == Some(&selection_id)) aria-pressed=move || ui.state.with(|state| state.selected_character.as_ref() == Some(&id)).to_string() aria-label=format!("Select {}", character.name) title=move || ui.state.with(|state| state.projection.characters.iter().find(|character| character.id == availability_id).and_then(|character| availability_reason(&character.availability)).unwrap_or_default()) on:click=move |_| { ui.send(UiIntent::SelectCharacter(click_id.clone())); ui.announcement.set(format!("{name} selected.")); }>
                            <span class="card-portrait"><img src=portrait_path(character.id.as_ref()) alt="" draggable="false"/></span>
                            <span class="card-name">{character.name.clone()}</span>
                            <img class="card-diamond" src="/assets/art/selection-diamond.png" alt=""/>
                        </button>
                    }
                }/>
                <button class="avatar-card create-card unavailable" aria-disabled="true" aria-describedby="create-reason" on:click=move |_| ui.explain("Create a Sim arrives with character creation.")>
                    <span class="create-portrait"><Icon name="plus"/></span><span class="card-name">"Create a Sim"</span>
                    <span id="create-reason" class="unavailable-tip">"Character creation coming later"</span>
                </button>
            </div>
            <Show when=move || ui.state.with(|state| state.projection.characters.is_empty())>
                <div class="empty-characters chrome"><h2>"No Sims available"</h2><p>"Your Sims couldn't be loaded."</p><button class="chrome primary" on:click=move |_| ui.retry_characters()><Icon name="refresh"/>"Try again"</button></div>
            </Show>
            <div class="character-stage" aria-label=move || selected_character(ui).map(|character| format!("{} on the character stage", character.name)).unwrap_or_else(|| "Character stage".into())>
                <Show when=move || selected_character(ui).is_some()>
                    <img class="stage-diamond" src="/assets/art/selection-diamond.png" alt=""/>
                    <img class="stage-character" src=move || selected_character(ui).map(|character| portrait_path(character.id.as_ref())).unwrap_or_default() alt=move || selected_character(ui).map(|character| character.name).unwrap_or_default() draggable="false"/>
                </Show>
            </div>
            <div class="avatar-actions">
                <Show when=move || selected_character(ui).and_then(|character| availability_reason(&character.availability)).is_some()>
                    <div class="character-unavailable chrome"><p>{move || selected_character(ui).and_then(|character| availability_reason(&character.availability)).unwrap_or_default()}</p><button class="chrome" on:click=move |_| ui.retry_characters()>"Try again"</button></div>
                </Show>
                <button class="chrome primary play-button" disabled=move || !available() on:click=move |_| ui.send(UiIntent::Play)><Icon name="player-play"/>{move || selected_character(ui).map(|character| format!("Play as {}", character.name)).unwrap_or_else(|| "Choose a Sim".into())}</button>
                <button class="chrome outfit-button unavailable" aria-disabled="true" aria-describedby="outfit-reason" on:click=move |_| ui.explain("Outfit editing arrives with character creation.")><Icon name="hanger"/><span>"Change outfit"</span><span id="outfit-reason" class="unavailable-tip">"Outfit editing coming later"</span></button>
            </div>
        </section>
    }
}
