//! Connected game presentation. Preview providers and saves are intentionally absent.
#[path = "components/connected_player.rs"]
pub mod player;

use crate::{
    avatar_content::{
        AvatarMotion, AvatarStage, ContentLoader, ContentTarget, ContentUi, OriginalThumbnail,
    },
    components::{Icon, player_menu::ConnectedPlayerMenu},
    connected_adapter::{source_text, source_u32, state::*},
    connected_bridge::ConnectedUi,
    source_city::CityMapLot,
    source_city_renderer::SourceCityMap,
};
use leptos::prelude::*;
use wonderland_contracts::authoring::{AppearanceSelection, ContentKey};
use wonderland_game_services::*;

#[component]
pub fn ConnectedGame(
    #[prop(into)] gateway_url: String,
    #[prop(default = false)] native_lots: bool,
) -> impl IntoView {
    let ui = ConnectedUi::new(gateway_url);
    let content = ContentUi::new();
    provide_context(ui);
    provide_context(crate::connected_authoring::SourceAuthoringUi::attach(ui));
    provide_context(content);
    provide_context(AvatarMotion(Signal::derive(move || {
        ui.reduced_motion.get()
    })));
    provide_context(ContentTarget {
        busy: Signal::derive(move || ui.state.with(|s| s.busy("Create Sim"))),
        state_revision: Signal::derive(move || ui.state.with(|s| s.ledger.epoch)),
        content_revision: Signal::derive(move || {
            content
                .choices
                .with(|c| c.as_ref().map(|c| c.revision).unwrap_or(0))
        }),
        install: Callback::new(|_: wonderland_contracts::authoring::AppearanceContent| Ok(())),
    });
    let route = Memo::new(move |_| {
        ui.state.with(|s| {
            if s.panel == Some(Panel::Create) {
                2
            } else if s
                .session
                .as_ref()
                .is_some_and(|s| s.state == SessionState::LotReady)
            {
                3
            } else if s.in_city() {
                1
            } else {
                0
            }
        })
    });
    view! {
        <main class="game-shell connected-shell" class:reduce-motion=move ||ui.reduced_motion.get()>
            <Show when=move ||ui.state.with(|s|s.ledger.authenticated) fallback=||view!{<ConnectedLogin/>}>
                {move ||match route.get() {3=>if native_lots {view!{<crate::native_lot::NativeLot/>}.into_any()} else {view!{<crate::connected_world::ConnectedLotView/>}.into_any()},2=>view!{<player::ConnectedCreator/>}.into_any(),1=>view!{<ConnectedCity/>}.into_any(),_=>view!{<ConnectedRoster/>}.into_any()}}
                <ConnectedPlayerMenu/>
                <div class="top-tools"><crate::audio_bridge::SourceAudioControls/><button class="chrome round" aria-label="Options" on:click=move |_|ui.panel(Panel::Settings)><Icon name="settings"/></button></div>
                <player::ConnectedPanels/>
                <ConnectionNotice/>
            </Show>
            <Show when=move ||!ui.state.with(|s|s.notice.is_empty())>
                <div class="feedback connected-feedback" role="alert"><Icon name="alert-circle"/><span>{move ||ui.state.with(|s|s.notice.clone())}</span><button aria-label="Dismiss message" on:click=move |_|ui.state.update(|s|s.notice.clear())><Icon name="x"/></button></div>
            </Show>
            <div class="sr-only" role="status" aria-live="polite" aria-atomic="true">{move ||session_label(&ui.state.get())}</div>
        </main>
    }
}

#[component]
fn ConnectedLogin() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    view! {
        <section class="connected-login avatars-screen" aria-labelledby="connected-login-title">
            <div class="connected-login-art" aria-hidden="true"><img src="/assets/art/selection-diamond.png" alt=""/></div>
            <div class="connected-login-heading"><p>"Welcome to"</p><h1 id="connected-login-title">"Wonderland"</h1><p>"A world of people, places, and possibilities."</p></div>
            <form class="connected-login-card chrome" on:submit=move |event| {
                event.prevent_default();
                let secret=password.get_untracked();
                password.set(String::new());
                ui.login(username.get_untracked(),secret);
            }>
                <h2>"Come on in"</h2>
                <label for="connected-username">"Account name"</label><input id="connected-username" name="username" autocomplete="username" required=true autofocus=true disabled=move ||ui.state.with(|s|s.login_busy) prop:value=move ||username.get() on:input=move |e|username.set(event_target_value(&e))/>
                <label for="connected-password">"Password"</label><input id="connected-password" name="password" type="password" autocomplete="current-password" required=true disabled=move ||ui.state.with(|s|s.login_busy) prop:value=move ||password.get() on:input=move |e|password.set(event_target_value(&e))/>
                <button class="chrome primary" type="submit" disabled=move ||ui.state.with(|s|s.login_busy||s.health.as_ref().is_none_or(|h|!h.configured))><Icon name="player-play"/>{move ||if ui.state.with(|s|s.login_busy){"Signing in…"}else{"Sign in"}}</button>
                <Show when=move ||ui.state.with(|s|s.health.as_ref().is_none_or(|h|!h.configured))><p role="status">"Waiting for this world to be available."</p><button class="connected-text-button" type="button" on:click=move |_|ui.check_health()>"Try connection again"</button></Show>
            </form>
        </section>
    }
}

#[component]
fn ConnectedRoster() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let content = expect_context::<ContentUi>();
    let page = RwSignal::new(0usize);
    Effect::new(move |_| {
        let (index, total) = ui.state.with(|s| {
            (
                s.roster
                    .iter()
                    .position(|entry| Some(entry.avatar_id) == s.selected_avatar),
                s.roster.len(),
            )
        });
        page.update(|page| {
            *page = index
                .map(|i| i / 5)
                .unwrap_or((*page).min(total.saturating_sub(1) / 5))
        });
    });
    view! {
        <section class="avatars-screen connected-roster" aria-labelledby="connected-roster-title">
            <header class="connected-screen-heading"><h1 id="connected-roster-title">"Choose your Sim"</h1><p>{move ||ui.state.with(|s|format!("{} Sims in your account",s.roster.len()))}</p></header>
            <div class="connected-avatar-grid" role="group" aria-label="Account Sims">
                <For each=move ||ui.state.with(|s|s.roster.iter().skip(page.get()*5).take(5).cloned().collect::<Vec<_>>()) key=|entry|entry.avatar_id children=move |entry| {
                    let id=entry.avatar_id;
                    let appearance=roster_appearance(&entry);
                    let head=StoredValue::new(appearance.head.clone());
                    let skin=StoredValue::new(appearance.skin_tone.clone());
                    view! {
                        <button class="avatar-card connected-avatar-card" id=format!("connected-avatar-{id}") class:selected=move ||ui.state.with(|s|s.selected_avatar==Some(id)) aria-pressed=move ||ui.state.with(|s|s.selected_avatar==Some(id)).to_string() on:click=move |_|ui.select_avatar(id)>
                            <span class="card-portrait connected-portrait"><Show when=move ||content.imported.get().is_some()&&head.get_value().is_some() fallback=||view!{<Icon name="users" class="connected-silhouette"/>}>{move ||head.get_value().map(|head|view!{<OriginalThumbnail content_key=head skin=Signal::derive(move ||skin.get_value())/>})}</Show></span>
                            <span class="card-name">{entry.name}</span><span class="connected-card-city">{entry.shard_name}</span><img class="card-diamond" src="/assets/art/selection-diamond.png" alt=""/>
                        </button>
                    }
                }/>
                <button class="avatar-card create-card connected-avatar-card" on:click=move |_|ui.panel(Panel::Create)><span class="create-portrait"><Icon name="plus"/></span><span class="card-name">"Create a Sim"</span></button>
            </div>
            <Show when=move ||ui.state.with(|s|s.roster.len()>5)><nav class="connected-roster-pages" aria-label="Account Sim pages"><button class="chrome" disabled=move ||page.get()==0 on:click=move |_|{page.update(|p|*p=p.saturating_sub(1));if let Some(id)=ui.state.with_untracked(|s|s.roster.get(page.get_untracked()*5).map(|e|e.avatar_id)){ui.select_avatar(id);}}>"Previous"</button><span>{move ||ui.state.with(|s|format!("{} / {}",page.get()+1,s.roster.len().div_ceil(5)))}</span><button class="chrome" disabled=move ||ui.state.with(|s|(page.get()+1)*5>=s.roster.len()) on:click=move |_|{page.update(|p|*p+=1);if let Some(id)=ui.state.with_untracked(|s|s.roster.get(page.get_untracked()*5).map(|e|e.avatar_id)){ui.select_avatar(id);}}>"Next"</button></nav></Show>
            <Show when=move ||ui.state.with(|s|s.roster.is_empty())><div class="connected-empty-roster chrome"><h2>"Your story starts here"</h2><p>"Create a Sim to join a city."</p><button class="chrome" on:click=move |_|ui.refresh_roster()>"Refresh account"</button></div></Show>
            <div class="connected-roster-stage">
                <Show when=move ||ui.state.with(|s|s.selected_entry().is_some())>
                    <img class="stage-diamond" src="/assets/art/selection-diamond.png" alt=""/>
                    <Show when=move ||content.imported.get().is_some()&&ui.state.with(|s|s.selected_entry().is_some_and(|e|roster_appearance(e).skin_tone.is_some())) fallback=||view!{<div class="connected-stage-silhouette"><Icon name="users"/><span>"Load your game content to see your Sim."</span></div>}><AvatarStage appearance=Signal::derive(move ||ui.state.with(|s|s.selected_entry().map(roster_appearance).unwrap_or_default()))/></Show>
                    <div class="connected-selected-name"><h2>{move ||ui.state.with(|s|s.selected_entry().map(|e|e.name.clone()).unwrap_or_default())}</h2><p>{move ||ui.state.with(|s|s.selected_entry().map(|e|e.description.clone()).unwrap_or_default())}</p></div>
                </Show>
            </div>
            <div class="connected-roster-bottom">
                <div class="connected-shards" role="group" aria-label="Cities"><For each=move ||ui.state.with(|s|s.shards.clone()) key=|shard|shard.id children=move |shard| {
                    let name=shard.name.clone();let selected=name.clone();
                    view!{<button class="chrome connected-shard" aria-pressed=move ||ui.state.with(|s|s.selected_shard.as_ref()==Some(&selected)).to_string() on:click=move |_|ui.select_shard(name.clone())><Icon name="building-community"/><span><strong>{shard.name}</strong><small>{shard.status}</small></span></button>}
                }/></div>
                <div class="connected-roster-actions"><button id="connected-play" class="chrome primary play-button" disabled=move ||ui.state.with(|s|s.selected_entry().is_none()||!s.ledger.transport_ready||s.busy("Enter city")||s.has_home_intent()) on:click=move |_|ui.play()><Icon name="player-play"/>{move ||ui.state.with(|s|if s.busy("Enter city"){"Joining city…".into()}else{s.selected_entry().map(|e|format!("Play as {}",e.name)).unwrap_or_else(||"Choose a Sim".into())})}</button><button id="connected-go-home" class="chrome" disabled=move ||ui.state.with(|state|state.home_request().is_err()) title=move ||ui.state.with(|state|state.home_request().err().unwrap_or_else(||state.selected_entry().and_then(|entry|entry.home.as_ref()).map(|home|format!("Go to {}",home.name)).unwrap_or_default())) on:click=move |_|ui.go_home()><Icon name="home"/>{move ||ui.state.with(|state|if state.has_home_intent()||state.busy("Go home"){"Going home…"}else{"Go home"})}</button><button class="chrome" disabled=move ||ui.state.with(|s|s.selected_entry().is_none()) on:click=move |_|ui.panel(Panel::Wardrobe)><Icon name="hanger"/>"Wardrobe"</button><button class="chrome" on:click=move |_|ui.refresh_roster() aria-label="Refresh account roster"><Icon name="refresh"/></button></div>
                <player::OperationFeedback label="Enter city"/><player::OperationFeedback label="Go home city"/><player::OperationFeedback label="Go home"/>
                <details class="connected-content-details"><summary>"Game content"</summary><ContentLoader/></details>
            </div>
        </section>
    }
}

#[component]
fn ConnectedCity() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let directory = RwSignal::new("lots");
    let directory_open = RwSignal::new(false);
    let search = RwSignal::new(String::new());
    let filter = RwSignal::new("all".to_owned());
    let category = RwSignal::new(String::new());
    let selected_shard = Memo::new(move |_| ui.state.with(|s| s.shard().map(|s| s.id)));
    Effect::new(move |_| {
        let shard = selected_shard.get();
        if let Some(shard_id) = shard {
            ui.lots_page(1);
            ui.query("neighborhoods", DirectoryQuery::Neighborhoods { shard_id });
        }
    });
    let search_directory = move || {
        let Some(shard_id) = ui.state.with_untracked(|s| s.shard().map(|s| s.id)) else {
            return;
        };
        let name = search.get_untracked().trim().to_owned();
        let kind = directory.get_untracked();
        let query = match (kind, name.is_empty()) {
            ("people", true) => DirectoryQuery::AvatarPage {
                shard_id,
                page: 1,
                per_page: 50,
            },
            ("people", false) => DirectoryQuery::AvatarSearch { shard_id, name },
            ("neighborhoods", true) => DirectoryQuery::Neighborhoods { shard_id },
            ("neighborhoods", false) => DirectoryQuery::NeighborhoodSearch { shard_id, name },
            (_, true) => DirectoryQuery::LotPage {
                shard_id,
                page: 1,
                per_page: 50,
            },
            (_, false) => DirectoryQuery::LotSearch { shard_id, name },
        };
        filter.set("all".into());
        ui.query(kind, query);
    };
    view! {
        <section class="scene-screen connected-city" class:directory-expanded=move ||directory_open.get() aria-label="City">
            <SourceCityMap map=Signal::derive(move ||ui.state.with(|s|s.shard().map(|shard|shard.map.clone()).unwrap_or_default())) lots=Signal::derive(move ||ui.state.with(city_map_lots)) selected=Signal::derive(move ||ui.state.with(|s|s.ledger.selected_lot)) on_select=Callback::new(move |id|ui.select_lot(id))/>
            <header class="connected-city-heading chrome"><button class="chrome round small" aria-label="Switch Sim" on:click=move |_|ui.send(GatewayOperation::DisconnectCity,"Leave city",None)><Icon name="chevron-left"/></button><div class="connected-city-title"><h1 title=move ||ui.state.with(|s|s.selected_shard.clone().unwrap_or_else(||"City".into()))>{move ||ui.state.with(|s|s.selected_shard.clone().unwrap_or_else(||"City".into()))}</h1><p>{move ||session_label(&ui.state.get())}</p></div></header>
            <aside class="connected-discovery chrome" aria-label="Discover the city">
                <button class="connected-discovery-toggle" type="button" aria-expanded=move ||directory_open.get().to_string() aria-controls="connected-city-directory" on:click=move |_|directory_open.update(|open|*open = !*open)><Icon name="search"/><strong>"City directory"</strong><span>{move ||if directory_open.get(){"Hide"}else{"Show"}}</span><Icon name=Signal::derive(move ||if directory_open.get(){"chevron-down".to_owned()}else{"chevron-up".to_owned()})/></button>
                <div id="connected-city-directory" class="connected-discovery-body">
                <nav class="connected-tabs" aria-label="Discover"><For each=||vec![("lots","Places"),("people","People"),("neighborhoods","Neighborhoods")] key=|item|item.0 children=move |(key,label)|view!{<button class="chrome" aria-pressed=move ||(directory.get()==key).to_string() on:click=move |_|{directory.set(key);search.set(String::new());search_directory();}>{label}</button>}/></nav>
                <form class="connected-search" on:submit=move |event|{event.prevent_default();search_directory();}><Icon name="search"/><input aria-label="Search this city" placeholder="Find a place or person…" prop:value=move ||search.get() on:input=move |e|search.set(event_target_value(&e))/><button class="chrome" type="submit">"Find"</button></form>
                <Show when=move ||directory.get()!="neighborhoods"><div class="connected-filters"><button class="connected-text-button" aria-pressed=move ||(filter.get()=="all").to_string() on:click=move |_|{search.set(String::new());search_directory();}>"All"</button><button class="connected-text-button" aria-pressed=move ||(filter.get()=="online").to_string() on:click=move |_|{filter.set("online".into());let kind=directory.get_untracked();if kind=="people"{ui.query("people",DirectoryQuery::OnlineAvatars {compact:false});}else if let Some(shard_id)=ui.state.with_untracked(|s|s.shard().map(|s|s.id)){ui.query("lots",DirectoryQuery::OnlineLots {shard_id});}}>"Online now"</button><Show when=move ||directory.get()=="lots"><button class="connected-text-button" aria-pressed=move ||(filter.get()=="top").to_string() on:click=move |_|{filter.set("top".into());if let Some(shard_id)=ui.state.with_untracked(|s|s.shard().map(|s|s.id)){let category=category.get_untracked();ui.query("lots",DirectoryQuery::TopLots {shard_id,category:if category.is_empty(){None}else{Some(category)}});}}>"Top 100"</button></Show></div></Show>
                <Show when=move ||directory.get()=="lots"&&filter.get()=="top"><label class="connected-category-filter">"Category"<select aria-label="Top 100 category" prop:value=move ||category.get() on:change=move |event|{
                    let value=event_target_value(&event);category.set(value.clone());
                    if let Some(shard_id)=ui.state.with_untracked(|s|s.shard().map(|shard|shard.id)){ui.query("lots",DirectoryQuery::TopLots {shard_id,category:if value.is_empty(){None}else{Some(value)}});}
                }><For each=||vec![("","All categories"),("money","Money"),("offbeat","Offbeat"),("romance","Romance"),("services","Services"),("shopping","Shopping"),("skills","Skills"),("welcome","Welcome"),("games","Games"),("entertainment","Entertainment"),("residence","Residence"),("community","Community")] key=|category|category.0 children=|(value,label)|view!{<option value=value>{label}</option>}/></select></label></Show>
                <player::DirectoryList source=Signal::derive(move ||directory.get().to_owned())/>
                </div>
            </aside>
            <div class="connected-hud chrome"><button class="connected-hud-person" on:click=move |_|ui.panel(Panel::Profile)><Icon name="users"/><span class="connected-hud-identity"><strong>{move ||ui.state.with(|s|s.active_entry().map(|e|e.name.clone()).unwrap_or_else(||"Your Sim".into()))}</strong><small>{move ||ui.state.with(|s|s.active_entry().and_then(|e|e.money).map(|n|format!("§ {n}")).unwrap_or_else(||"Balance unavailable".into()))}</small></span></button><button class="connected-text-button" on:click=move |_|ui.panel(Panel::Profile)>"Needs"</button></div>
            <Show when=move ||ui.state.with(|s|s.session.as_ref().is_some_and(|s|matches!(s.state,SessionState::LotConnecting|SessionState::LotReady)))><div class="connected-lot-status chrome" role="status"><h2>{move ||ui.state.with(|s|if s.session.as_ref().is_some_and(|s|s.state==SessionState::LotReady){"Connected to property"}else{"Entering property…"})}</h2><player::CapabilityNote capability="live_world"/><button class="chrome" on:click=move |_|ui.send(GatewayOperation::LeaveLot,"Leave property",None)>"Return to city"</button><player::OperationFeedback label="Enter property"/></div></Show>
        </section>
    }
}

#[component]
fn ConnectionNotice() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    view! {<Show when=move ||ui.state.with(|s|!s.ledger.transport_ready)><div class="connected-connection-notice chrome" role="status"><Icon name="refresh"/><span>{move ||ui.state.with(|s|if s.socket_connecting{"Connecting to the world…"}else{"Connection interrupted"})}</span><button class="chrome" disabled=move ||ui.state.with(|s|s.socket_connecting) on:click=move |_|ui.reconnect()>"Reconnect"</button><button class="connected-text-button" on:click=move |_|ui.logout()>"Sign out"</button></div></Show>}
}

pub fn session_label(state: &ConnectedState) -> String {
    if state.login_busy {
        return "Signing in…".into();
    }
    if !state.ledger.authenticated {
        return "Signed out".into();
    }
    if !state.ledger.transport_ready {
        return if state.socket_connecting {
            "Connecting…"
        } else {
            "Disconnected"
        }
        .into();
    }
    match state.session.as_ref().map(|s| &s.state) {
        Some(SessionState::Authenticated) => "Choose a Sim to enter a city".into(),
        Some(SessionState::CityConnecting) => "Joining the city…".into(),
        Some(SessionState::CityReady) => "In the city".into(),
        Some(SessionState::LotConnecting) => "Entering the property…".into(),
        Some(SessionState::LotReady) => "Connected to the property".into(),
        Some(SessionState::Disconnected) => "City connection ended".into(),
        None => "Signed out".into(),
    }
}

pub fn roster_appearance(entry: &RosterEntry) -> AppearanceSelection {
    let skin =
        entry
            .appearance
            .as_deref()
            .and_then(|value| match value.to_ascii_lowercase().as_str() {
                "light" => Some("light"),
                "medium" => Some("medium"),
                "dark" => Some("dark"),
                _ => None,
            });
    AppearanceSelection {
        head: entry
            .head_key
            .map(|key| ContentKey::from(format!("vitaboy:{:016x}", key.0))),
        body: entry
            .body_key
            .map(|key| ContentKey::from(format!("vitaboy:{:016x}", key.0))),
        skin_tone: skin.map(|skin| ContentKey::from(format!("vitaboy:skin:{skin}"))),
        ..AppearanceSelection::default()
    }
}

fn city_map_lots(state: &ConnectedState) -> Vec<CityMapLot> {
    let mut rows = state.page("lots").rows;
    let property = state.data("property");
    if source_u32(&property, "shard_id") == state.shard().map(|s| s.id) {
        rows.push(property);
    }
    let mut lots = std::collections::BTreeMap::new();
    for row in rows {
        if let (Some(lot_id), Some(location)) = (
            source_u32(&row, "lot_id"),
            source_u32(&row, "location").or_else(|| source_u32(&row, "lot_location")),
        ) {
            let name = source_text(&row, "name");
            lots.insert(
                lot_id,
                CityMapLot {
                    lot_id,
                    location,
                    name: if name.is_empty() {
                        source_text(&row, "lot_name")
                    } else {
                        name
                    },
                    online: source_u32(&row, "avatars_in_lot"),
                },
            );
        }
    }
    lots.into_values().collect()
}
