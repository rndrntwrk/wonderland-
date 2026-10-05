use crate::{
    authoring_bridge::{AuthorUi, look_path},
    bridge::{Ui, focus_later},
    components::Icon,
};
use leptos::prelude::*;
use wonderland_contracts::authoring::*;
#[component]
pub fn Creator() -> impl IntoView {
    let author = expect_context::<AuthorUi>();
    let ui = expect_context::<Ui>();
    let creation = move || {
        author
            .state
            .with(|s| matches!(s.draft(), Some(AuthoringDraft::Creation(_))))
    };
    let identity = move || {
        author.state.with(|s| match s.draft() {
            Some(AuthoringDraft::Creation(d)) => d.identity,
            Some(AuthoringDraft::Outfit(d)) => s
                .projection()
                .profile(&d.character_id)
                .map(|p| p.identity)
                .unwrap_or(VisualIdentity::Maya),
            _ => VisualIdentity::Maya,
        })
    };
    let look = move || {
        author.state.with(|s| match s.draft() {
            Some(AuthoringDraft::Creation(d)) => d.look_id.clone(),
            Some(AuthoringDraft::Outfit(d)) => d.look_id.clone(),
            _ => VisualIdentity::Maya.look_id(LookStyle::Everyday),
        })
    };
    let name = move || {
        author.state.with(|s| match s.draft() {
            Some(AuthoringDraft::Creation(d)) => d.name.clone(),
            Some(AuthoringDraft::Outfit(d)) => s
                .projection()
                .profile(&d.character_id)
                .map(|p| p.character.name.clone())
                .unwrap_or_default(),
            _ => String::new(),
        })
    };
    let cancel = move || {
        if author.busy() {
            return;
        }
        let id = author
            .state
            .with_untracked(|s| s.selected_profile().cloned());
        author.send(AuthoringIntent::Cancel);
        focus_later(
            id.map(|id| format!("avatar-{id}"))
                .unwrap_or("create-sim".into()),
        );
    };
    view! {
    <section class="avatars-screen creator-screen" aria-label="Character authoring" aria-busy=move || author.busy().to_string() on:keydown=move |e:web_sys::KeyboardEvent|{if e.is_composing(){return;}
    if e.key()=="Escape"{e.prevent_default();cancel();}}>
    <h1 id="screen-title" class="choose-title" tabindex="-1">{move || if creation(){"Create a Sim"}else{"Change outfit"}}</h1>
    <div class="creator-stage"><img class="stage-diamond" src="/assets/art/selection-diamond.png" alt=""/><img class="creator-character" src=move ||look_path(identity(),&look()) alt=move ||format!("{} {} full-body preview",identity().label(),look().style_for(identity()).unwrap_or(LookStyle::Everyday).label()) draggable="false"/></div>
    <div class="creator-choices">
    <Show when=creation><div class="identity-row" role="group" aria-label="Appearance preset">{VisualIdentity::ALL.into_iter().map(|id|view!{<button class="chrome identity-card" aria-pressed=move ||(identity()==id).to_string() disabled=move ||author.busy() on:click=move |_|author.send(AuthoringIntent::SelectIdentity(id))><img src=format!("/assets/art/{}.png",id.as_str()) alt=""/><span>{id.label()}</span></button>}).collect_view()}</div></Show>
    <div class="look-row" role="group" aria-label="Choose a look">{LookStyle::ALL.into_iter().map(|style|view!{<button class="chrome look-card" aria-pressed=move ||(look()==identity().look_id(style)).to_string() disabled=move ||author.busy() on:click=move |_|author.send(AuthoringIntent::SelectLook(identity().look_id(style)))><img src=move ||look_path(identity(),&identity().look_id(style)) alt=""/><span>{style.label()}</span></button>}).collect_view()}</div>
    <Show when=creation fallback=move ||view!{<span class="outfit-name">{name}</span>}><label class="chrome nameplate"><span>"Name"</span><input id="sim-name" type="text" autocomplete="off" placeholder="Your Sim’s name" aria-describedby="creator-validity" disabled=move ||author.busy() prop:value=name on:input=move |event|author.send(AuthoringIntent::UpdateName(event_target_value(&event)))/></label></Show>
    <p id="creator-validity" class="creator-validity" role="status">{move ||author.state.with(|s|s.draft_validity().err().map(|e|e.to_string()).unwrap_or_else(||if author.busy(){"Saving preview…".into()}else{"Ready to save".into()}))}</p>
    <div class="creator-actions"><button class="chrome" disabled=move ||author.busy() on:click=move |_|cancel()>"Cancel"</button><button id="save-character" class="chrome primary" disabled=move ||author.busy()||author.state.with(|s|s.draft_validity().is_err()) on:click=move |_|author.send(if creation(){AuthoringIntent::SubmitCreate}else{AuthoringIntent::SaveOutfit})><Icon name="check"/>{move ||if author.busy(){"Saving…"}else if creation(){"Create Sim"}else{"Save outfit"}}</button></div>
    </div>
    <span class="sr-only">{move ||ui.announcement.get()}</span>
    </section>
    }
}
