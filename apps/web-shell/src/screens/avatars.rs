use crate::{
    authoring_bridge::AuthorUi,
    bridge::{Ui, focus_later, selected_character},
    components::{Icon, availability_reason},
};
use leptos::prelude::*;
use wonderland_contracts::{UiIntent, authoring::AuthoringIntent};
#[component]
pub fn Avatars() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let author = expect_context::<AuthorUi>();
    let page = RwSignal::new(0usize);
    Effect::new(move |_| {
        let selected = ui.state.with(|s| s.selected_character.clone());
        let index = ui.state.with(|s| {
            s.projection
                .characters
                .iter()
                .position(|c| Some(&c.id) == selected.as_ref())
        });
        if let Some(index) = index {
            page.set(index / 5);
        }
    });
    let available = move || selected_character(ui).is_some_and(|c| c.availability.is_available());
    view! {
    <section class="avatars-screen" class:paged=move ||ui.state.with(|s|s.projection.characters.len()>5) aria-label="Character selection">
    <h1 id="screen-title" class="choose-title" tabindex="-1">"Choose your Sim"</h1>
    <div class="avatar-grid" role="group" aria-label="Choose a Sim">
    <For each=move ||ui.state.with(|s|s.projection.characters.iter().skip(page.get()*5).take(5).cloned().collect::<Vec<_>>()) key=|c|c.id.clone() children=move |c|{
    let id=c.id;let selection=id.clone();let pressed=id.clone();let click=id.clone();let name_id=id.clone();let label_id=id.clone();let image_id=id.clone();let reason=id.clone();
    view!{<button id=format!("avatar-{id}") class="avatar-card" class:selected=move ||ui.state.with(|s|s.selected_character.as_ref()==Some(&selection)) aria-pressed=move ||ui.state.with(|s|s.selected_character.as_ref()==Some(&pressed)).to_string() aria-label=move ||ui.state.with(|s|format!("Select {}",s.projection.characters.iter().find(|c|c.id==label_id).map(|c|c.name.clone()).unwrap_or_default())) title=move ||ui.state.with(|s|s.projection.characters.iter().find(|c|c.id==reason).and_then(|c|availability_reason(&c.availability)).unwrap_or_default()) on:click=move |_|{ui.send(UiIntent::SelectCharacter(click.clone()));author.select(click.clone());}>
    <span class="card-portrait"><img src=move ||author.path(&image_id) alt="" draggable="false"/></span><span class="card-name">{move ||ui.state.with(|s|s.projection.characters.iter().find(|c|c.id==name_id).map(|c|c.name.clone()).unwrap_or_default())}</span><img class="card-diamond" src="/assets/art/selection-diamond.png" alt=""/></button>}
    }/>
    <button id="create-sim" class="avatar-card create-card" disabled=move ||author.state.with(|s|s.projection().profiles.len()>=8) title="Up to eight preview Sims" on:click=move |_|{if let Some(c)=selected_character(ui){author.select(c.id);}author.send(AuthoringIntent::OpenCreate);focus_later("sim-name".into());}><span class="create-portrait"><Icon name="plus"/></span><span class="card-name">"Create a Sim"</span></button>
    </div>
    <Show when=move ||ui.state.with(|s|s.projection.characters.len()>5)><nav class="profile-pages" aria-label="Character pages"><button class="chrome" disabled=move ||page.get()==0 on:click=move |_|{let p=page.get_untracked()-1;page.set(p);let id=ui.state.with_untracked(|s|s.projection.characters.get(p*5).map(|c|c.id.clone()));if let Some(id)=id{ui.send(UiIntent::SelectCharacter(id));}}>"Previous"</button><span>{move ||format!("{} / {}",page.get()+1,ui.state.with(|s|s.projection.characters.len().div_ceil(5)))}</span><button class="chrome" disabled=move ||ui.state.with(|s|(page.get()+1)*5>=s.projection.characters.len()) on:click=move |_|{let p=page.get_untracked()+1;page.set(p);let id=ui.state.with_untracked(|s|s.projection.characters.get(p*5).map(|c|c.id.clone()));if let Some(id)=id{ui.send(UiIntent::SelectCharacter(id));}}>"Next"</button></nav></Show>
    <Show when=move ||ui.state.with(|s|s.projection.characters.is_empty())><div class="empty-characters chrome"><h2>"No Sims available"</h2><p>"Your Sims couldn't be loaded."</p><button class="chrome primary" on:click=move |_|ui.retry_characters()>"Try again"</button></div></Show>
    <div class="character-stage" aria-label="Character stage"><Show when=move ||selected_character(ui).is_some()><img class="stage-diamond" src="/assets/art/selection-diamond.png" alt=""/><img class="stage-character" src=move ||selected_character(ui).map(|c|author.path(&c.id)).unwrap_or_default() alt=move ||selected_character(ui).map(|c|c.name).unwrap_or_default() draggable="false"/></Show></div>
    <div class="avatar-actions"><Show when=move ||selected_character(ui).and_then(|c|availability_reason(&c.availability)).is_some()><div class="character-unavailable chrome"><p>{move ||selected_character(ui).and_then(|c|availability_reason(&c.availability)).unwrap_or_default()}</p><button class="chrome" on:click=move |_|ui.retry_characters()>"Try again"</button></div></Show><button class="chrome primary play-button" disabled=move ||!available() on:click=move |_|ui.send(UiIntent::Play)><Icon name="player-play"/>{move ||selected_character(ui).map(|c|format!("Play as {}",c.name)).unwrap_or("Choose a Sim".into())}</button><button class="chrome outfit-button" disabled=move ||!available() on:click=move |_|{if let Some(c)=selected_character(ui){author.select(c.id);author.send(AuthoringIntent::OpenOutfit);focus_later("save-character".into());}}><Icon name="hanger"/>"Change outfit"</button></div>
    </section>
    }
}
