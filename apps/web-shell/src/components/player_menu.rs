use crate::{
    authoring_bridge::AuthorUi,
    bridge::{Overlay, Ui, selected_character},
    components::Icon,
};
use leptos::prelude::*;
#[component]
pub fn PlayerMenu() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let author = expect_context::<AuthorUi>();
    let open = RwSignal::new(false);
    let selected = RwSignal::new(None::<(&'static str, &'static str)>);
    view! {
        <div class="player-menu">
        <button id="preview-menu" class="chrome round" aria-label="Player menu" aria-expanded=move ||open.get().to_string() on:click=move |_|open.update(|v|*v = !*v)><Icon name="users"/></button>
        <Show when=move ||open.get()>
        <section class="player-menu-panel chrome" aria-label="Player surfaces" on:keydown=move |e:web_sys::KeyboardEvent|if e.key()=="Escape"{open.set(false);}>
        <header class="player-menu-heading"><h2>{move ||selected_character(ui).map(|c|c.name).unwrap_or("Player menu".into())}</h2><button class="chrome round small" aria-label="Close player menu" on:click=move |_|{open.set(false);crate::connected_bridge::focus("preview-menu");}><Icon name="x"/></button></header>
        <nav aria-label="Game surfaces">
        {[("Profile","The profile service is not connected. Your selected Sim’s saved name and description are shown below."),("People","The people directory, relationship and search service is not connected."),("Bookmarks","Saved people and property bookmarks require the account service."),("Chat","Live lot and private chat require the game session; message delivery is not connected."),("Inbox","Inbox reading and mail delivery require the account messaging service."),("Neighborhood","Neighborhood membership, voting and community surfaces require the city service."),("Property","Property ownership, roommates, permissions and transactions require the property service."),("EODs","Object-specific game dialogs open from simulation events. This preview has no connected EOD event stream.")].into_iter().map(move |(label,reason)|{let icon=match label {"Bookmarks"=>"map-pin","Inbox"=>"stack-2","Neighborhood"=>"building-community","Property"=>"home","EODs"=>"device-gamepad-2",_=>"users"};view!{<button class="chrome" aria-pressed=move ||selected.get().is_some_and(|(l,_)|l==label).to_string() on:click=move |_|selected.set(Some((label,reason)))><Icon name=icon/><span>{label}</span></button>}}).collect_view()}
        <button class="chrome" on:click=move |_|{ui.overlay.set(Overlay::Settings);open.set(false);}><Icon name="settings"/><span>"Options"</span></button>
        </nav>
        <Show when=move ||selected.get().is_some()><div class="player-surface-status" role="status"><h3>{move ||selected.get().map(|(l,_)|l)}</h3><p>{move ||selected.get().map(|(_,r)|r)}</p>
        <Show when=move ||selected.get().is_some_and(|(l,_)|l=="Profile")><p>{move ||author.state.with(|s|s.selected_profile().and_then(|id|s.projection().profile(id)).map(|p|p.description.clone()).filter(|d|!d.is_empty()).unwrap_or("No description saved.".into()))}</p></Show>
        </div></Show>
        <button class="chrome player-menu-footer" on:click=move |_|open.set(false)><Icon name="x"/><span>"Close"</span></button>
        </section></Show></div>
    }
}

#[component]
pub fn ConnectedPlayerMenu() -> impl IntoView {
    use crate::{connected_adapter::state::Panel, connected_bridge::ConnectedUi};
    let ui = expect_context::<ConnectedUi>();
    let open = RwSignal::new(false);
    view! {
        <div class="player-menu connected-player-menu">
            <button id="connected-menu" class="chrome round" aria-label="Player menu" aria-expanded=move ||open.get().to_string() on:click=move |_|open.update(|v|*v = !*v)><Icon name="users"/></button>
            <Show when=move ||open.get()><section class="player-menu-panel chrome" aria-label="Player menu" on:keydown=move |event:web_sys::KeyboardEvent|if event.key()=="Escape" {open.set(false);crate::connected_bridge::focus("connected-menu");}>
                <header class="player-menu-heading"><h2>{move ||ui.state.with(|s|s.active_entry().or_else(||s.selected_entry()).map(|e|e.name.clone()).unwrap_or_else(||"Your world".into()))}</h2><button class="chrome round small" aria-label="Close player menu" on:click=move |_|{open.set(false);crate::connected_bridge::focus("connected-menu");}><Icon name="x"/></button></header>
                <nav aria-label="Game surfaces">{[Panel::Profile,Panel::People,Panel::Bookmarks,Panel::Chat,Panel::Inbox,Panel::Property,Panel::Neighborhood,Panel::Wardrobe,Panel::Eods,Panel::Settings].into_iter().map(move |panel|view!{<button class="chrome" on:click=move |_|{ui.panel(panel);open.set(false);}><Icon name=panel.icon()/><span>{panel.label()}</span></button>}).collect_view()}</nav>
                <button class="chrome player-menu-footer" on:click=move |_|{open.set(false);ui.logout();}><Icon name="arrow-back-up"/><span>"Sign out"</span></button>
            </section></Show>
        </div>
    }
}
