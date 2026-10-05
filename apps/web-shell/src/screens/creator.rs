use crate::{
    authoring_bridge::AuthorUi,
    avatar_content::{AvatarStage, ContentLoader, OriginalThumbnail},
    bridge::{Ui, focus_later},
    components::Icon,
};
use leptos::prelude::*;
use wonderland_contracts::authoring::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CreatorPart {
    Head,
    Body,
    SkinTone,
    Gender,
}

#[component]
fn PartChoices(part: CreatorPart, label: &'static str) -> impl IntoView {
    let author = expect_context::<AuthorUi>();
    let page = RwSignal::new(0usize);
    let options = move || {
        author.state.with(|state| {
            let content = &state.projection().appearance_content;
            match part {
                CreatorPart::Head => content.heads.clone(),
                CreatorPart::Body => content.bodies.clone(),
                CreatorPart::SkinTone => content.skin_tones.clone(),
                CreatorPart::Gender => content.genders.clone(),
            }
        })
    };
    Effect::new(move |_| {
        let count = options().len();
        page.update(|p| *p = (*p).min(count.saturating_sub(1) / 18));
    });
    view! {
        <fieldset class="appearance-options" class:part-grid=matches!(part,CreatorPart::Head|CreatorPart::Body)>
            <legend>{label}</legend>
            <Show when=move ||options().is_empty()>
                <p class="creator-validity">"Content is not loaded yet."</p>
            </Show>
            <div class="look-row appearance-grid">
                <For each=move ||{options().into_iter().skip(page.get()*18).take(18).collect::<Vec<_>>() } key=move |option|(option.key.clone(),author.state.with(|s|s.projection().appearance_content.revision)) children=move |option|{
                    let key = option.key.clone();
                    let chosen = option.key.clone();
                    let available = option.availability.is_available();
                    let preview_key=option.key.clone();
                    let explanation=crate::components::availability_reason(&option.availability);

                    view! {
                        <button class="chrome look-card"
                            disabled=move ||author.busy()
                            aria-disabled=(!available).to_string()
                            title=explanation.clone().unwrap_or_default()
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
                            on:click=move |_|{if let Some(reason)=&explanation{expect_context::<Ui>().explain(reason.clone());return;}author.send(match part {
                                CreatorPart::Head => AuthoringIntent::SelectHead(key.clone()),
                                CreatorPart::Body => AuthoringIntent::SelectBody(key.clone()),
                                CreatorPart::SkinTone => AuthoringIntent::SelectSkinTone(key.clone()),
                                CreatorPart::Gender => AuthoringIntent::SelectGender(key.clone()),
                            })}>
                            <Show when=move ||matches!(part,CreatorPart::Head|CreatorPart::Body)><OriginalThumbnail content_key=preview_key.clone() skin=Signal::derive(move ||author.state.with(|s|match s.draft(){Some(AuthoringDraft::Creation(d))=>d.appearance.skin_tone.clone(),_=>None}))/></Show>
                            <span>{option.label}</span>
                        </button>
                    }
                }/>
            </div>
            <Show when=move ||{options().len()>18}><nav class="choice-pages" aria-label=format!("{} pages",label)><button class="chrome" disabled=move ||page.get()==0 on:click=move |_|page.update(|p|*p=p.saturating_sub(1))>"Previous"</button><span>{move ||format!("{} / {} · {} choices",page.get()+1,options().len().div_ceil(18),options().len())}</span><button class="chrome" disabled=move ||{(page.get()+1)*18>=options().len()} on:click=move |_|page.update(|p|*p+=1)>"Next"</button></nav></Show>
        </fieldset>
    }
}

#[component]
pub fn Creator() -> impl IntoView {
    let author = expect_context::<AuthorUi>();
    let ui = expect_context::<Ui>();
    let part = RwSignal::new(CreatorPart::Head);
    let creation = move || {
        author
            .state
            .with(|state| matches!(state.draft(), Some(AuthoringDraft::Creation(_))))
    };
    let name = move || {
        author.state.with(|state| match state.draft() {
            Some(AuthoringDraft::Creation(draft)) => draft.name.clone(),
            Some(AuthoringDraft::Outfit(draft)) => state
                .projection()
                .profile(&draft.character_id)
                .map(|profile| profile.character.name.clone())
                .unwrap_or_default(),
            _ => String::new(),
        })
    };
    let description = move || {
        author.state.with(|state| match state.draft() {
            Some(AuthoringDraft::Creation(draft)) => draft.description.clone(),
            _ => String::new(),
        })
    };
    let wardrobe = move || {
        author.state.with(|state| match state.draft() {
            Some(AuthoringDraft::Outfit(draft)) => state
                .projection()
                .profile(&draft.character_id)
                .map(|profile| profile.wardrobe.outfits.clone())
                .unwrap_or_default(),
            _ => vec![],
        })
    };
    let confirm_delete = RwSignal::new(false);
    let cancel = move || {
        if author.busy() {
            return;
        }
        let id = author
            .state
            .with_untracked(|state| state.selected_profile().cloned());
        author.send(AuthoringIntent::Cancel);
        focus_later(
            id.map(|id| format!("avatar-{id}"))
                .unwrap_or("create-sim".into()),
        );
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
                <AvatarStage appearance=Signal::derive(move ||author.state.with(|s|match s.draft(){Some(AuthoringDraft::Creation(d))=>d.appearance.clone(),Some(AuthoringDraft::Outfit(d))=>s.projection().profile(&d.character_id).map(|p|crate::source_identity::wardrobe_preview(p,d)).unwrap_or_default(),_=>AppearanceSelection::default()}))/>

            </div>
            <div class="creator-choices">
                <ContentLoader/>
                <Show when=creation>
                    <nav class="creator-part-tabs" aria-label="Character parts">
                        <button class="chrome" aria-pressed=move ||(part.get()==CreatorPart::Head).to_string() on:click=move |_|part.set(CreatorPart::Head)>"Head"</button>
                        <button class="chrome" aria-pressed=move ||(part.get()==CreatorPart::Body).to_string() on:click=move |_|part.set(CreatorPart::Body)>"Body"</button>
                    </nav>
                    <Show when=move ||part.get()==CreatorPart::Head fallback=||view!{<PartChoices part=CreatorPart::Body label="Body"/>}><PartChoices part=CreatorPart::Head label="Head"/></Show>
                    <div class="creator-traits"><PartChoices part=CreatorPart::SkinTone label="Skin tone"/><PartChoices part=CreatorPart::Gender label="Gender"/></div>
                    <label class="chrome nameplate"><span>"Name"</span>
                        <input id="sim-name" type="text" autocomplete="off" aria-describedby="creator-validity"
                            disabled=move ||author.busy() prop:value=name
                            on:input=move |event|author.send(AuthoringIntent::UpdateName(event_target_value(&event)))/>
                    </label>
                    <details class="creator-description"><summary>"Description (optional)"</summary><label class="chrome nameplate"><span>"Description"</span>
                        <textarea id="sim-description" disabled=move ||author.busy() prop:value=description
                            on:input=move |event|author.send(AuthoringIntent::UpdateDescription(event_target_value(&event)))></textarea>
                    </label></details>
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
                <Show when=move ||!creation()><nav class="wardrobe-actions" aria-label="Owned outfit actions"><For each=move ||author.state.with(|s|match s.draft(){Some(AuthoringDraft::Outfit(d))=>s.projection().profile(&d.character_id).and_then(|p|d.owned_outfit_id.as_ref().and_then(|id|p.wardrobe.outfit(id))).map(|o|o.actions.clone()).unwrap_or_default(),_=>vec![]}) key=|a|format!("{:?}",a.action) children=move |a|{let action=a.action;let reason=crate::components::availability_reason(&a.availability);view!{<button class="chrome" aria-disabled=reason.is_some().to_string() aria-pressed=move ||author.state.with(|s|matches!(s.draft(),Some(AuthoringDraft::Outfit(d)) if d.action==action)).to_string() disabled=move ||author.busy() on:click=move |_|{if let Some(reason)=&reason{ui.explain(reason.clone());return;}confirm_delete.set(false);author.send(AuthoringIntent::SelectWardrobeAction(action));}>{match action{WardrobeAction::Change=>"Wear",WardrobeAction::SetDefault=>"Set default",WardrobeAction::Delete=>"Delete"}}</button>}}/></nav></Show>
                <Show when=move ||confirm_delete.get()><div class="delete-confirm chrome" role="alert"><p>"Delete this owned outfit? The service must accept the change before it is removed."</p><button class="chrome" on:click=move |_|confirm_delete.set(false)>"Keep outfit"</button><button class="chrome" disabled=move ||author.busy() on:click=move |_|{confirm_delete.set(false);author.send(AuthoringIntent::SaveOutfit);}>"Confirm deletion"</button></div></Show>
                <p id="creator-validity" class="creator-validity" role="status">
                    {move ||author.state.with(|state|state.draft_validity().err().map(|error|error.to_string())
                        .unwrap_or_else(||if author.busy(){"Saving preview…".into()}else{"Ready to save the local preview".into()}))}
                </p>
                <div class="creator-actions">
                    <button class="chrome" disabled=move ||author.busy() on:click=move |_|cancel()>"Cancel"</button>
                    <button id="save-character" class="chrome primary"
                        disabled=move ||author.busy() || author.state.with(|state|state.draft_validity().is_err())
                        on:click=move |_|{let deleting=author.state.with_untracked(|s|matches!(s.draft(),Some(AuthoringDraft::Outfit(d)) if d.action==WardrobeAction::Delete));if deleting{confirm_delete.set(true);}else{author.send(if creation(){AuthoringIntent::SubmitCreate}else{AuthoringIntent::SaveOutfit});}}>
                        <Icon name="check"/>{move ||if author.busy(){"Saving…"}else if creation(){"Create Sim"}else{"Save outfit"}}
                    </button>
                </div>
            </div>
            <span class="sr-only">{move ||ui.announcement.get()}</span>
        </section>
    }
}
