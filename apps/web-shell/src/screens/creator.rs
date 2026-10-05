use crate::{
    authoring_bridge::AuthorUi,
    bridge::{Ui, focus_later},
    components::Icon,
};
use leptos::prelude::*;
use wonderland_contracts::authoring::*;

#[derive(Clone, Copy)]
enum CreatorPart { Head, Body, SkinTone, Gender }

#[component]
fn PartChoices(part: CreatorPart, label: &'static str) -> impl IntoView {
    let author = expect_context::<AuthorUi>();
    let options = move || author.state.with(|state| {
        let content = &state.projection().appearance_content;
        match part {
            CreatorPart::Head => content.heads.clone(),
            CreatorPart::Body => content.bodies.clone(),
            CreatorPart::SkinTone => content.skin_tones.clone(),
            CreatorPart::Gender => content.genders.clone(),
        }
    });
    view! {
        <fieldset class="appearance-options">
            <legend>{label}</legend>
            <Show when=move ||options().is_empty()>
                <p class="creator-validity">"Content is not loaded yet."</p>
            </Show>
            <div class="look-row">
                <For each=options key=|option|option.key.clone() children=move |option|{
                    let key = option.key.clone();
                    let chosen = option.key.clone();
                    let available = option.availability.is_available();
                    view! {
                        <button class="chrome look-card"
                            disabled=move ||author.busy() || !available
                            aria-pressed=move ||author.state.with(|state| {
                                let Some(AuthoringDraft::Creation(draft)) = state.draft() else { return false; };
                                let value = match part {
                                    CreatorPart::Head => &draft.appearance.head,
                                    CreatorPart::Body => &draft.appearance.body,
                                    CreatorPart::SkinTone => &draft.appearance.skin_tone,
                                    CreatorPart::Gender => &draft.appearance.gender,
                                };
                                value.as_ref() == Some(&chosen)
                            }).to_string()
                            on:click=move |_|author.send(match part {
                                CreatorPart::Head => AuthoringIntent::SelectHead(key.clone()),
                                CreatorPart::Body => AuthoringIntent::SelectBody(key.clone()),
                                CreatorPart::SkinTone => AuthoringIntent::SelectSkinTone(key.clone()),
                                CreatorPart::Gender => AuthoringIntent::SelectGender(key.clone()),
                            })>
                            {option.thumbnail.map(|path|view!{<img src=path alt=""/>})}
                            <span>{option.label}</span>
                        </button>
                    }
                }/>
            </div>
        </fieldset>
    }
}

#[component]
pub fn Creator() -> impl IntoView {
    let author = expect_context::<AuthorUi>();
    let ui = expect_context::<Ui>();
    let creation = move || author.state.with(|state| matches!(state.draft(), Some(AuthoringDraft::Creation(_))));
    let name = move || author.state.with(|state| match state.draft() {
        Some(AuthoringDraft::Creation(draft)) => draft.name.clone(),
        Some(AuthoringDraft::Outfit(draft)) => state.projection().profile(&draft.character_id)
            .map(|profile| profile.character.name.clone()).unwrap_or_default(),
        _ => String::new(),
    });
    let description = move || author.state.with(|state| match state.draft() {
        Some(AuthoringDraft::Creation(draft)) => draft.description.clone(),
        _ => String::new(),
    });
    let wardrobe = move || author.state.with(|state| match state.draft() {
        Some(AuthoringDraft::Outfit(draft)) => state.projection().profile(&draft.character_id)
            .map(|profile| profile.wardrobe.outfits.clone()).unwrap_or_default(),
        _ => vec![],
    });
    let cancel = move || {
        if author.busy() { return; }
        let id = author.state.with_untracked(|state| state.selected_profile().cloned());
        author.send(AuthoringIntent::Cancel);
        focus_later(id.map(|id| format!("avatar-{id}")).unwrap_or("create-sim".into()));
    };
    view! {
        <section class="avatars-screen creator-screen" aria-label="Character authoring"
            aria-busy=move ||author.busy().to_string()
            on:keydown=move |event:web_sys::KeyboardEvent|{
                if !event.is_composing() && event.key()=="Escape" { event.prevent_default(); cancel(); }
            }>
            <h1 id="screen-title" class="choose-title" tabindex="-1">
                {move ||if creation() { "Create a Sim" } else { "Wardrobe" }}
            </h1>
            <div class="creator-stage">
                <img class="stage-diamond" src="/assets/art/selection-diamond.png" alt=""/>
                <div class="creator-content-status chrome" role="status">
                    <Icon name="user"/>
                    <p>"A composed character preview needs the original avatar resources."</p>
                    <p>"Saved illustrated portraits are historical artwork and do not show a newly selected outfit."</p>
                </div>
            </div>
            <div class="creator-choices">
                <Show when=creation>
                    <PartChoices part=CreatorPart::Head label="Head"/>
                    <PartChoices part=CreatorPart::Body label="Body"/>
                    <PartChoices part=CreatorPart::SkinTone label="Skin tone"/>
                    <PartChoices part=CreatorPart::Gender label="Gender"/>
                    <label class="chrome nameplate"><span>"Name"</span>
                        <input id="sim-name" type="text" autocomplete="off" aria-describedby="creator-validity"
                            disabled=move ||author.busy() prop:value=name
                            on:input=move |event|author.send(AuthoringIntent::UpdateName(event_target_value(&event)))/>
                    </label>
                    <label class="chrome nameplate"><span>"Description"</span>
                        <textarea id="sim-description" disabled=move ||author.busy() prop:value=description
                            on:input=move |event|author.send(AuthoringIntent::UpdateDescription(event_target_value(&event)))></textarea>
                    </label>
                    <Show when=move ||author.state.with(|state| !state.projection().account.shards.is_empty())>
                        <label class="chrome nameplate"><span>"City"</span>
                            <select disabled=move ||author.busy()
                                on:change=move |event|author.send(AuthoringIntent::SelectShard(event_target_value(&event).into()))>
                                <For each=move ||author.state.with(|state|state.projection().account.shards.clone())
                                    key=|shard|shard.id.clone() children=move |shard|{
                                        let id=shard.id.clone();
                                        view!{<option value=shard.id.to_string() disabled=!shard.availability.is_available()
                                            selected=move ||author.state.with(|state|matches!(state.draft(),Some(AuthoringDraft::Creation(draft)) if draft.shard_id.as_ref()==Some(&id)))>{shard.label}</option>}
                                    }/>
                            </select>
                        </label>
                    </Show>
                </Show>
                <Show when=move ||!creation()>
                    <span class="outfit-name">{name}</span>
                    <Show when=move ||wardrobe().is_empty()><p>"Owned clothing will appear when your wardrobe is loaded."</p></Show>
                    <div class="look-row">
                        <For each=wardrobe key=|outfit|outfit.id.clone() children=move |outfit|{
                            let chosen=outfit.id.clone();
                            let id=outfit.id.clone();
                            view!{<button class="chrome look-card" disabled=move ||author.busy()
                                aria-pressed=move ||author.state.with(|state|matches!(state.draft(),Some(AuthoringDraft::Outfit(draft)) if draft.owned_outfit_id.as_ref()==Some(&chosen))).to_string()
                                on:click=move |_|author.send(AuthoringIntent::SelectOwnedOutfit(id.clone()))>
                                {outfit.thumbnail.map(|path|view!{<img src=path alt=""/>})}<span>{outfit.label}</span>
                            </button>}
                        }/>
                    </div>
                </Show>
                <p id="creator-validity" class="creator-validity" role="status">
                    {move ||author.state.with(|state|state.draft_validity().err().map(|error|error.to_string())
                        .unwrap_or_else(||if author.busy(){"Saving preview…".into()}else{"Ready to save the local preview".into()}))}
                </p>
                <div class="creator-actions">
                    <button class="chrome" disabled=move ||author.busy() on:click=move |_|cancel()>"Cancel"</button>
                    <button id="save-character" class="chrome primary"
                        disabled=move ||author.busy() || author.state.with(|state|state.draft_validity().is_err())
                        on:click=move |_|author.send(if creation(){AuthoringIntent::SubmitCreate}else{AuthoringIntent::SaveOutfit})>
                        <Icon name="check"/>{move ||if author.busy(){"Saving…"}else if creation(){"Create Sim"}else{"Save outfit"}}
                    </button>
                </div>
            </div>
            <span class="sr-only">{move ||ui.announcement.get()}</span>
        </section>
    }
}
