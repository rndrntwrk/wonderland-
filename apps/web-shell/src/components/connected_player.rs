//! Contextual player panels fed only by gateway and original directory responses.
use crate::{
    avatar_content::{AvatarStage, ContentLoader, ContentUi, OriginalThumbnail},
    components::Icon,
    connected_adapter::{state::*, *},
    connected_bridge::{ConnectedUi, focus},
};
use leptos::leptos_dom::helpers::request_animation_frame;
use leptos::prelude::*;
use serde_json::Value;
use wonderland_contracts::authoring::{AppearanceSelection, ContentKey};
use wonderland_game_services::*;
use wonderland_vm_protocol::chat::SourceChatKind;

#[component]
pub fn ConnectedPanels() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let panel = Memo::new(move |_| ui.state.with(|s| s.panel.filter(|p| *p != Panel::Create)));
    let close = move || {
        ui.state.update(|s| s.panel = None);
        focus("connected-menu");
    };
    view! {
        <Show when=move ||panel.get().is_some()><section class="connected-panel chrome" aria-labelledby="connected-panel-title" on:keydown=move |e:web_sys::KeyboardEvent|if e.key()=="Escape"&&!e.is_composing(){e.prevent_default();close();}>
            <header class="connected-panel-heading"><Icon name=Signal::derive(move ||panel.get().map(|p|p.icon()).unwrap_or("users").to_owned())/><h2 id="connected-panel-title" tabindex="-1">{move ||panel.get().map(|p|p.label())}</h2><button class="chrome round small" aria-label="Close panel" on:click=move |_|close()><Icon name="x"/></button></header>
            <div class="connected-panel-body">{move ||match panel.get() {
                Some(Panel::Profile)=>view!{<ProfilePanel/>}.into_any(),Some(Panel::People)=>view!{<PeoplePanel/>}.into_any(),Some(Panel::Bookmarks)=>view!{<BookmarksPanel/>}.into_any(),Some(Panel::Chat)=>view!{<ChatPanel/>}.into_any(),Some(Panel::Inbox)=>view!{<InboxPanel/>}.into_any(),Some(Panel::Property)=>view!{<PropertyPanel/>}.into_any(),Some(Panel::Neighborhood)=>view!{<NeighborhoodPanel/>}.into_any(),Some(Panel::Wardrobe)=>view!{<WardrobePanel/>}.into_any(),Some(Panel::Eods)=>view!{<EodPanel/>}.into_any(),Some(Panel::Settings)=>view!{<OptionsPanel/>}.into_any(),_=>().into_any(),
            }}</div>
        </section></Show>
    }
}

#[component]
pub fn CapabilityNote(capability: &'static str) -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    view! {<Show when=move ||ui.state.with(|s|s.capability_reason(capability).is_some())><p class="connected-capability" role="status"><Icon name="lock"/>{move ||ui.state.with(|s|s.capability_reason(capability).unwrap_or_default())}</p></Show>}
}

#[component]
pub fn OperationFeedback(#[prop(into)] label: String) -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let label = StoredValue::new(label);
    let result = move || ui.state.with(|s| s.last_operation(&label.get_value()));
    view! {<Show when=move ||result().is_some()><p class="connected-operation" class:rejected=move ||result().is_some_and(|o|matches!(o.status,OperationStatus::Rejected(_))) class:unknown=move ||result().is_some_and(|o|matches!(o.status,OperationStatus::Unknown(_))) role="status">{move ||result().map(|o|match o.status {OperationStatus::Waiting=>"Sending…".into(),OperationStatus::Pending(message)|OperationStatus::Accepted(message)|OperationStatus::Rejected(message)|OperationStatus::Unknown(message)=>message}).unwrap_or_default()}</p></Show>}
}

fn can(ui: ConnectedUi, capability: &str, label: &str) -> bool {
    ui.state.with(|s| {
        s.ledger.transport_ready && s.capability_reason(capability).is_none() && !s.busy(label)
    })
}

#[component]
fn ReadStatus(#[prop(into)] source: Signal<String>) -> impl IntoView {
    let slot = source;
    let ui = expect_context::<ConnectedUi>();
    view! {<Show when=move ||ui.state.with(|s|matches!(s.read(&slot.get()).status,LoadState::Loading))><p class="connected-loading" role="status"><Icon name="loader-2"/>"Loading…"</p></Show><Show when=move ||ui.state.with(|s|matches!(s.read(&slot.get()).status,LoadState::Failed(_)))><div class="connected-read-error" role="alert"><p>{move ||ui.state.with(|s|match s.read(&slot.get()).status {LoadState::Failed(error)=>error,_=>String::new()})}</p><button class="chrome" on:click=move |_|{
        let source=slot.get_untracked();
        let query=ui.state.with_untracked(|s|s.read(&source).result.map(|result|result.query));
        if let Some(query)=query {ui.query(source,query);}
    }>"Try again"</button></div></Show>}
}

#[component]
pub fn DirectoryList(#[prop(into)] source: Signal<String>) -> impl IntoView {
    let slot = source;
    let ui = expect_context::<ConnectedUi>();
    let page = Memo::new(move |_| ui.state.with(|s| s.page(&slot.get())));
    view! {
        <ReadStatus source=slot/>
        <div class="connected-directory-list" role="list" aria-label="Directory results">
            <For each=move ||page.get().rows key=|row|row.to_string() children=move |row| {
                let avatar=source_u32(&row,"avatar_id");let lot=source_u32(&row,"lot_id");let neighborhood=source_u32(&row,"neighborhood_id");
                let title=source_text(&row,"name");let title=if title.is_empty(){source_text(&row,"lot_name")}else{title};
                let subtitle=source_u32(&row,"rank").map(|rank|format!("Rank {rank}")).or_else(||source_u32(&row,"avatars_in_lot").map(|n|format!("{n} visiting"))).unwrap_or_else(||source_text(&row,"description"));
                let kind=slot;
                let aria=format!("Open {title}");
                view!{<div role="listitem"><button class="connected-directory-row" disabled=move ||avatar.is_none()&&lot.is_none()&&neighborhood.is_none() aria-label=aria on:click=move |_|{
                    if let Some(id)=avatar {ui.select_person(id);} else if let Some(id)=lot {ui.select_lot(id);}else if let Some(id)=neighborhood {ui.select_neighborhood(id);}
                }><span class="connected-row-icon"><Icon name=Signal::derive(move ||if kind.get().contains("people"){"users"}else if kind.get()=="neighborhoods"{"building-community"}else{"home"}.to_owned())/></span><span class="connected-row-text"><strong>{if title.is_empty(){"Unnamed entry".into()}else{title}}</strong><small>{subtitle}</small></span><Icon name="chevron-right"/></button></div>}
            }/>
        </div>
        <Show when=move ||ui.state.with(|s|s.read(&slot.get()).status==LoadState::Ready)&&page.get().rows.is_empty()><div class="connected-empty"><Icon name="search"/><p>"No matches here yet."</p></div></Show>
        <Show when=move ||page.get().total_pages.is_some()><nav class="connected-page-nav" aria-label="Directory pages"><button class="chrome" disabled=move ||page.get().page<=1 on:click=move |_|{
            let slot=slot.get_untracked();let request=ui.state.with_untracked(|s|s.read(&slot).result.and_then(|r|DirectoryPage::next_query(&r.query,page.get_untracked().page.saturating_sub(1))));if let Some(request)=request {ui.query(slot,request);}
        }>"Previous"</button><span>{move ||{let page=page.get();format!("{} / {}",page.page,page.total_pages.unwrap_or(1).max(1))}}<small>{move ||page.get().total_items.map(|n|format!("{n} total"))}</small></span><button class="chrome" disabled=move ||page.get().total_pages.is_none_or(|total|page.get().page>=total) on:click=move |_|{
            let slot=slot.get_untracked();let request=ui.state.with_untracked(|s|s.read(&slot).result.and_then(|r|DirectoryPage::next_query(&r.query,page.get_untracked().page.saturating_add(1))));if let Some(request)=request {ui.query(slot,request);}
        }>"Next"</button></nav></Show>
    }
}

#[component]
fn PeoplePanel() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let search = RwSignal::new(String::new());
    view! {<form class="connected-search" on:submit=move |e|{e.prevent_default();if let Some(shard_id)=ui.state.with_untracked(|s|s.shard().map(|s|s.id)){let name=search.get_untracked().trim().to_owned();if name.is_empty(){ui.people_page(1);}else{ui.query("people",DirectoryQuery::AvatarSearch {shard_id,name});}}}><Icon name="search"/><input aria-label="Find a Sim" placeholder="Find a Sim…" prop:value=move ||search.get() on:input=move |e|search.set(event_target_value(&e))/><button class="chrome">"Find"</button></form><div class="connected-filters"><button class="connected-text-button" on:click=move |_|ui.people_page(1)>"All Sims"</button><button class="connected-text-button" on:click=move |_|ui.query("people",DirectoryQuery::OnlineAvatars{compact:false})>"Online now"</button></div><DirectoryList source=Signal::derive(||"people".to_owned())/>}
}

#[component]
fn ProfilePanel() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let confirm = RwSignal::new(false);
    let profile = Memo::new(move |_| ui.state.with(|s| s.data("profile")));
    let id = Memo::new(move |_| source_u32(&profile.get(), "avatar_id"));
    let found_location = Memo::new(move |_| {
        ui.state.with(|s| {
            let operation = s.last_operation("Find Sim")?;
            if !matches!(operation.status, OperationStatus::Accepted(_))
                || s.wire_epochs.get(&operation.stamp.operation_id).copied()
                    != s.session.as_ref().map(|session| session.epoch)
            {
                return None;
            }
            let response = s.results.get(&operation.stamp.operation_id)?;
            if source_u32(response, "avatar_id") != id.get() {
                return None;
            }
            source_u32(response, "lot_location").filter(|location| *location != 0)
        })
    });
    view! {
        <ReadStatus source=Signal::derive(||"profile".to_owned())/>
        <Show when=move ||id.get().is_some() fallback=||view!{<p class="connected-empty">"Select a Sim in the people directory."</p>}>
            <div class="connected-profile-hero"><span class="connected-profile-portrait"><Icon name="users"/></span><div><h3>{move ||source_text(&profile.get(),"name")}</h3><p>{move ||ui.state.with(|s|s.shards.iter().find(|shard|Some(shard.id)==source_u32(&profile.get(),"shard_id")).map(|shard|shard.name.clone()).unwrap_or_default())}</p></div></div>
            <p class="connected-description">{move ||{let text=source_text(&profile.get(),"description");if text.is_empty(){"No description yet.".into()}else{text}}}</p>
            <dl class="connected-facts"><dt>"Gender"</dt><dd>{move ||source_enum(&profile.get(),"gender",&["Male","Female"])}</dd><dt>"Current job"</dt><dd>{move ||match source_u32(&profile.get(),"current_job"){Some(0)=>"No current job".to_owned(),Some(_)=>"Job title has not been supplied".to_owned(),None=>"Not supplied".to_owned()}}</dd><dt>"Joined"</dt><dd>{move ||date_seconds(&profile.get(),"date")}</dd></dl>
            <div class="connected-action-row"><button class="chrome primary" disabled=move ||!can(ui,"private_message","Private message") on:click=move |_|{if let Some(id)=id.get_untracked(){ui.state.update(|s|s.select_private_conversation(id));}}>"Message"</button><button class="chrome" disabled=move ||!can(ui,"mail","Send mail") on:click=move |_|{if let Some(id)=id.get_untracked(){ui.state.update(|s|{s.ledger.selected_person=Some(id);s.panel=Some(Panel::Inbox);});ui.draft("mail:recipient",id.to_string());}}>"Write mail"</button><button class="chrome" disabled=move ||!can(ui,"city","Find Sim") on:click=move |_|if let Some(id)=id.get_untracked(){ui.send(GatewayOperation::FindAvatar {avatar_id:id},"Find Sim",None);}>"Find in world"</button></div>
            <OperationFeedback label="Find Sim"/><Show when=move ||found_location.get().is_some()><button class="chrome" on:click=move |_|{
                let location=found_location.get_untracked();let shard=ui.state.with_untracked(|s|s.shard().map(|s|s.id));if let (Some(location),Some(shard_id))=(location,shard){ui.query("property",DirectoryQuery::LotByLocation {shard_id,location});ui.state.update(|s|s.panel=Some(Panel::Property));}
            }>"Open their property"</button></Show>
            <CapabilityNote capability="private_message"/>
            <h4>"Relationships & bookmarks"</h4><CapabilityNote capability="bookmarks"/>
            <h4>"Skills & needs"</h4><Show when=move ||ui.state.with(|s|s.roster.iter().find(|entry|Some(entry.avatar_id)==id.get()).and_then(|entry|entry.motives).is_some()) fallback=||view!{<p class="connected-muted">"Live needs have not been supplied for this Sim."</p>}><div class="connected-needs">{move ||ui.state.with(|s|s.roster.iter().find(|entry|Some(entry.avatar_id)==id.get()).and_then(|entry|entry.motives).map(|needs|crate::source_needs::SOURCE_NEED_LABELS.into_iter().zip(needs).map(|(name,value)|view!{<span><strong>{name}</strong><span>{value}</span></span>}).collect_view()))}</div></Show>
            <CapabilityNote capability="profile_edit"/>
            <Show when=move ||ui.state.with(|s|s.session.as_ref().and_then(|s|s.avatar_id)==id.get()&&s.roster.iter().any(|entry|Some(entry.avatar_id)==id.get()))>
                <details class="connected-more"><summary>"Manage this Sim"</summary><p>"Retirement permanently removes this Sim from your account. The world checks whether retirement is allowed."</p><button class="chrome danger" disabled=move ||!can(ui,"retire_avatar","Retire Sim") on:click=move |_|confirm.set(true)>"Retire Sim…"</button><OperationFeedback label="Retire Sim"/></details>
                <ConfirmAction open=confirm title="Retire this Sim?" description="This permanently removes this Sim. Their retirement must be accepted by the world." confirm_label="Retire Sim" on_confirm=Callback::new(move |_|ui.send(GatewayOperation::RetireAvatar,"Retire Sim",None))/>
            </Show>
        </Show>
    }
}

#[component]
fn BookmarksPanel() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    view! {<div class="connected-empty"><Icon name="users"/><h3>"Your people & places"</h3><CapabilityNote capability="bookmarks"/><p>"Account bookmarks will appear here when this world supplies them."</p><button class="chrome" on:click=move |_|ui.panel(Panel::People)>"Browse people"</button></div>}
}

#[component]
fn PropertyPanel() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let data = Memo::new(move |_| ui.state.with(|s| s.data("property")));
    let location = Memo::new(move |_| source_u32(&data.get(), "location"));
    let invite = RwSignal::new(false);
    let confirm_remove = RwSignal::new(false);
    let remove_target = RwSignal::new(None::<(u32, u32)>);
    let refresh = move || {
        if let Some(lot_id) = source_u32(&data.get_untracked(), "lot_id") {
            ui.query("property", DirectoryQuery::Lot { lot_id });
        }
    };
    view! {
        <RoommateInvitations/><ReadStatus source=Signal::derive(||"property".to_owned())/>
        <Show when=move ||location.get().is_some() fallback=||view!{<p>"Select a property on the map or in the directory."</p><DirectoryList source=Signal::derive(||"lots".to_owned())/>}>
            <div class="connected-property-hero"><Icon name="home"/><h3>{move ||source_text(&data.get(),"name")}</h3><span>{move ||lot_category(&data.get())}</span></div><p class="connected-description">{move ||source_text(&data.get(),"description")}</p>
            <div class="connected-action-row"><button class="chrome primary" disabled=move ||!can(ui,"lot","Enter property") on:click=move |_|if let Some(lot_location)=location.get_untracked(){ui.send(GatewayOperation::JoinLot {lot_location,open_if_closed:false},"Enter property",None);}>"Visit"<Icon name="chevron-right"/></button><button class="chrome" on:click=move |_|refresh()><Icon name="refresh"/>"Refresh"</button><Show when=move ||ui.state.with(|s|s.session.as_ref().and_then(|s|s.avatar_id).filter(|id|*id!=0).is_some_and(|actor|source_u32(&data.get(),"owner_id")==Some(actor)))><button class="chrome" disabled=move ||!can(ui,"lot","Open property") on:click=move |_|if let Some(lot_location)=location.get_untracked(){ui.send(GatewayOperation::JoinLot {lot_location,open_if_closed:true},"Open property",None);}>"Open my property"</button></Show></div><OperationFeedback label="Enter property"/><OperationFeedback label="Open property"/><CapabilityNote capability="lot"/>
            <dl class="connected-facts"><dt>"Owner"</dt><dd><Show when=move ||source_u32(&data.get(),"owner_id").is_some() fallback=||"No owner listed">{move ||source_u32(&data.get(),"owner_id").map(|owner|view!{<PersonLink id=owner/>})}</Show></dd><dt>"Entry"</dt><dd>{move ||source_u32(&data.get(),"admit_mode").map(|mode|match mode {0=>"Open admission",1=>"Admit list",2=>"Ban list",3=>"Closed admission",_=>"Entry rules supplied by the world"}).unwrap_or("Not supplied")}</dd><dt>"Established"</dt><dd>{move ||date_seconds(&data.get(),"created_date")}</dd><dt>"Skill mode"</dt><dd>{move ||source_text(&data.get(),"skill_mode")}</dd></dl>
            <Show when=move ||source_u32(&data.get(),"neighborhood_id").is_some()><button class="chrome" on:click=move |_|if let Some(id)=source_u32(&data.get_untracked(),"neighborhood_id"){ui.select_neighborhood(id);}><Icon name="building-community"/>"Open neighborhood"</button></Show>
            <h4>"Residents"</h4><Show when=move ||data.get().get("roommates").and_then(Value::as_array).is_some_and(|v|!v.is_empty()) fallback=||view!{<p class="connected-muted">"No roommates are listed."</p>}><div class="connected-residents">{move ||data.get().get("roommates").and_then(Value::as_array).cloned().unwrap_or_default().into_iter().filter_map(|id|id.as_u64().and_then(|n|u32::try_from(n).ok())).map(move |id|view!{<div class="connected-resident"><PersonLink id=id/><Show when=move ||ui.state.with(|s|s.session.as_ref().and_then(|s|s.avatar_id).filter(|id|*id!=0).is_some_and(|actor|source_u32(&data.get(),"owner_id")==Some(actor)))><button class="connected-text-button" disabled=move ||!can(ui,"property","Remove roommate") on:click=move |_|if let Some(lot_location)=location.get_untracked(){remove_target.set(Some((id,lot_location)));confirm_remove.set(true);}>"Remove…"</button></Show></div>}).collect_view()}</div></Show>
            <Show when=move ||ui.state.with(|s|s.session.as_ref().and_then(|s|s.avatar_id).filter(|id|*id!=0).is_some_and(|actor|source_u32(&data.get(),"owner_id")==Some(actor)))><button class="chrome" on:click=move |_|invite.update(|v|*v = !*v)>"Invite a roommate"</button><Show when=move ||invite.get()><p>"Choose a Sim from People, then return to this property."</p><button class="chrome" on:click=move |_|ui.panel(Panel::People)>"Choose a Sim"</button><Show when=move ||ui.state.with(|s|s.ledger.selected_person.is_some())><button class="chrome primary" disabled=move ||!can(ui,"property","Invite roommate") on:click=move |_|{let target=ui.state.with_untracked(|s|s.ledger.selected_person);if let (Some(avatar_id),Some(lot_location))=(target,location.get_untracked()){ui.send(GatewayOperation::Roommate {action:RoommateAction::Invite,avatar_id,lot_location},"Invite roommate",None);}}>"Send roommate invitation"</button></Show></Show></Show>
            <Show when=move ||ui.state.with(|s|s.session.as_ref().and_then(|s|s.avatar_id).filter(|id|*id!=0).is_some_and(|actor|data.get().get("roommates").and_then(Value::as_array).is_some_and(|members|members.iter().any(|member|member.as_u64()==Some(actor as u64)))))>
                <button class="chrome danger" disabled=move ||!can(ui,"property","Remove roommate") on:click=move |_|{
                    let actor=ui.state.with_untracked(|s|s.session.as_ref().and_then(|s|s.avatar_id));
                    if let (Some(actor),Some(location))=(actor,location.get_untracked()) {remove_target.set(Some((actor,location)));confirm_remove.set(true);}
                }>"Move out…"</button>
            </Show>
            <OperationFeedback label="Invite roommate"/><OperationFeedback label="Remove roommate"/><CapabilityNote capability="property"/>
            <ConfirmAction open=confirm_remove title="Remove this resident?" description="The world will check the selected resident and property before removing their residence. An owner may need to close the property first." confirm_label="Remove resident" on_confirm=Callback::new(move |_|if let Some((avatar_id,lot_location))=remove_target.get_untracked(){ui.send(GatewayOperation::Roommate {action:RoommateAction::Kick,avatar_id,lot_location},"Remove roommate",None);})/>
            <details class="connected-more"><summary>"Ownership & house settings"</summary><p>"Purchase terms, admit and ban lists, donations, environment, and resizing need the property's current management data."</p><p class="connected-muted">"No purchase offer or management permission set has been supplied for this property."</p></details>
        </Show>
    }
}

#[component]
fn RoommateInvitations() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let confirm = RwSignal::new(false);
    let selected = RwSignal::new(None::<(u32, u32)>);
    view! {<details class="connected-more connected-invitations"><summary>"Roommate invitations"<span class="connected-unread">{move ||ui.state.with(|s|s.roommate_invitations.len().to_string())}</span></summary>
        <button class="chrome" disabled=move ||!can(ui,"property","Check invitations") on:click=move |_|ui.send(GatewayOperation::Roommate {action:RoommateAction::Poll,avatar_id:0,lot_location:0},"Check invitations",None)>"Check invitations"</button><OperationFeedback label="Check invitations"/>
        <For each=move ||ui.state.with(|s|s.roommate_invitations.iter().map(|(location,data)|(*location,data.clone())).collect::<Vec<_>>()) key=|entry|entry.0 children=move |(location,data)|{
            let owner=source_u32(&data,"avatar_id").unwrap_or(0);
            view!{<article class="connected-invitation"><p>"Invitation from "<PersonLink id=owner/></p><div class="connected-action-row"><button class="chrome" on:click=move |_|if let Some(shard_id)=ui.state.with_untracked(|s|s.shard().map(|shard|shard.id)){ui.query("property",DirectoryQuery::LotByLocation {shard_id,location});}>"View property"</button><button class="chrome primary" disabled=move ||!can(ui,"property","Accept invitation") on:click=move |_|{selected.set(Some((owner,location)));confirm.set(true);}>"Accept…"</button><button class="chrome" disabled=move ||!can(ui,"property","Decline invitation") on:click=move |_|ui.send(GatewayOperation::Roommate {action:RoommateAction::Decline,avatar_id:owner,lot_location:location},"Decline invitation",None)>"Decline"</button></div></article>}
        }/>
        <Show when=move ||ui.state.with(|s|s.roommate_invitations.is_empty())><p class="connected-muted">"No invitations have been received in this session."</p></Show>
        <OperationFeedback label="Accept invitation"/><OperationFeedback label="Decline invitation"/>
        <ConfirmAction open=confirm title="Move into this property?" description="The world will check this invitation and make this property your Sim's home if allowed." confirm_label="Accept invitation" on_confirm=Callback::new(move |_|if let Some((avatar_id,lot_location))=selected.get_untracked(){ui.send(GatewayOperation::Roommate {action:RoommateAction::Accept,avatar_id,lot_location},"Accept invitation",None);})/>
    </details>}
}

#[component]
fn PersonLink(id: u32) -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    view! {<button class="connected-text-button" on:click=move |_|ui.select_person(id)>{move ||ui.state.with(|s|s.roster.iter().find(|entry|entry.avatar_id==id).map(|entry|entry.name.clone()).unwrap_or_else(||format!("View Sim {id}")))}</button>}
}

#[component]
fn ConfirmAction(
    open: RwSignal<bool>,
    title: &'static str,
    description: &'static str,
    confirm_label: &'static str,
    on_confirm: Callback<()>,
) -> impl IntoView {
    let dialog = NodeRef::<leptos::html::Dialog>::new();
    Effect::new(move |_| {
        if let Some(dialog) = dialog.get() {
            if open.get() {
                if !dialog.open() {
                    let _ = dialog.show_modal();
                }
            } else {
                dialog.close();
            }
        }
    });
    view! {<dialog node_ref=dialog class="game-dialog connected-confirm" aria-label=title on:cancel=move |event:web_sys::Event|{event.prevent_default();open.set(false);}>
        <h2>{title}</h2><p>{description}</p><div class="connected-action-row"><button class="chrome" autofocus=true on:click=move |_|open.set(false)>"Cancel"</button><button class="chrome danger" on:click=move |_|{open.set(false);on_confirm.run(());}>{confirm_label}</button></div>
    </dialog>}
}

fn source_enum(data: &Value, key: &str, labels: &[&str]) -> String {
    match data.get(key) {
        Some(Value::Number(value)) => value
            .as_u64()
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| labels.get(index))
            .map(|label| (*label).to_owned())
            .unwrap_or_else(|| "Not supplied".into()),
        Some(Value::String(value)) => value.replace('_', " "),
        _ => "Not supplied".into(),
    }
}

fn date_seconds(data: &Value, key: &str) -> String {
    let Some(seconds) = data.get(key).and_then(Value::as_u64).filter(|n| *n != 0) else {
        return "Not supplied".into();
    };
    js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(seconds as f64 * 1000.))
        .to_locale_date_string("en-US", &js_sys::Object::new())
        .as_string()
        .unwrap_or_default()
}

fn lot_category(data: &Value) -> String {
    let value = source_text(data, "category");
    match value.as_str() {
        "0" => "Uncategorized",
        "1" => "Money",
        "2" => "Offbeat",
        "3" => "Romance",
        "4" => "Services",
        "5" => "Shopping",
        "6" => "Skills",
        "7" => "Welcome",
        "8" => "Games",
        "9" => "Entertainment",
        "10" => "Residence",
        "11" => "Community",
        _ => &value,
    }
    .into()
}

#[component]
fn OptionsPanel() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    view! {<label class="setting-row"><span>"Reduce motion"</span><input type="checkbox" prop:checked=move ||ui.reduced_motion.get() on:change=move |e|ui.reduced_motion.set(event_target_checked(&e))/></label><h3>"Getting around"</h3><p>"Choose people and properties to see their available actions. Use the map controls to move around the city. Press Escape to close a panel."</p><div class="connected-action-row"><button class="chrome" disabled=move ||!can(ui,"city","Leave city") on:click=move |_|ui.send(GatewayOperation::DisconnectCity,"Leave city",None)>"Switch Sim"</button><button class="chrome" on:click=move |_|ui.logout()>"Sign out"</button></div><OperationFeedback label="Leave city"/><details class="connected-more"><summary>"Recent actions"</summary><For each=move ||ui.state.with(|s|s.ledger.operations.values().rev().map(|o|(o.stamp.operation_id.clone(),o.label.clone())).collect::<Vec<_>>()) key=|o|o.0.clone() children=move |(_,label)|view!{<h4>{label.clone()}</h4><OperationFeedback label=label/>}/></details>}
}

#[component]
fn ChatPanel() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let lot = Memo::new(move |_| ui.state.with(|s| s.chat_lot));
    let log = NodeRef::<leptos::html::Div>::new();
    let source_messages = Memo::new(move |_| ui.state.with(|s| s.lot_chat.visible_messages()));
    let source_revision =
        Memo::new(move |_| ui.state.with(|s| (s.chat_lot, s.lot_chat.display_revision)));
    Effect::new(move |_| {
        let (is_lot, _) = source_revision.get();
        if !is_lot || log.get().is_none() {
            return;
        }
        let key = ui.state.with_untracked(|s| s.lot_chat.draft_key());
        request_animation_frame(move || {
            let Some((top, at_bottom)) = ui
                .state
                .try_with_untracked(|s| {
                    (s.chat_lot && s.panel == Some(Panel::Chat) && s.lot_chat.draft_key() == key)
                        .then_some((s.lot_chat.scroll_top, s.lot_chat.at_bottom))
                })
                .flatten()
            else {
                return;
            };
            if let Some(element) = log.get_untracked() {
                element.set_scroll_top(if at_bottom {
                    element.scroll_height()
                } else {
                    top
                });
                if at_bottom {
                    ui.state.try_update(|s| s.lot_chat.mark_read());
                }
            }
        });
    });
    let target = Memo::new(move |_| ui.state.with(|s| s.ledger.selected_person));
    let key = move || {
        if lot.get() {
            ui.state.with(|s| s.lot_chat.draft_key())
        } else {
            format!("private:{}", target.get().unwrap_or(0))
        }
    };
    let send = move || {
        let draft_key = key();
        let body = ui.state.with_untracked(|s| s.draft(&draft_key));
        if body.trim().is_empty() {
            return;
        }
        if lot.get_untracked() {
            if !ui.state.with_untracked(|s| s.lot_chat.ready) {
                return;
            }
            ui.send(
                GatewayOperation::LotChat { message: body },
                "Lot chat",
                Some(&draft_key),
            );
        } else if let Some(target_avatar_id) = target.get_untracked() {
            ui.send(
                GatewayOperation::PrivateMessage {
                    target_avatar_id,
                    message: body,
                    color: 0,
                },
                "Private message",
                Some(&draft_key),
            );
        }
    };
    let can_send = move || {
        can(
            ui,
            if lot.get() {
                "lot_chat"
            } else {
                "private_message"
            },
            if lot.get() {
                "Lot chat"
            } else {
                "Private message"
            },
        ) && if lot.get() {
            ui.state.with(|s| s.lot_chat.ready)
        } else {
            target.get().is_some()
        }
    };
    view! {
        <nav class="connected-tabs" aria-label="Chat type"><button class="chrome" aria-pressed=move ||(!lot.get()).to_string() on:click=move |_|ui.state.update(|s|{s.chat_lot=false;if let Some(person)=s.ledger.selected_person{s.ledger.select_conversation(person);}})>"Private messages"</button><button class="chrome" aria-pressed=move ||lot.get().to_string() on:click=move |_|ui.state.update(|s|{s.chat_lot=true;if s.lot_chat.at_bottom{s.lot_chat.mark_read();}})>"Lot chat"<span class="connected-unread">{move ||ui.state.with(|s|{let count=s.lot_chat.unread_count();if count>0 {count.to_string()} else {String::new()}})}</span></button></nav>
        <Show when=move ||!lot.get()>
            <div class="connected-conversations"><For each=move ||ui.state.with(|s|{
                let mut people=std::collections::BTreeSet::new();
                for event in &s.events {if event.family=="instant_message"&& let Some(id)=source_u32(&event.data,"from"){people.insert(id);}}
                for operation in s.sent.values(){if let GatewayOperation::PrivateMessage {target_avatar_id,..}=operation{people.insert(*target_avatar_id);}}
                if let Some(id)=s.ledger.selected_person{people.insert(id);}people.into_iter().collect::<Vec<_>>()
            }) key=|id|*id children=move |id|view!{<button class="chrome" aria-pressed=move ||(target.get()==Some(id)).to_string() on:click=move |_|ui.state.update(|s|s.select_private_conversation(id))>{move ||person_name(&ui.state.get(),id)}<span class="connected-unread">{move ||ui.state.with(|s|s.ledger.unread.get(&id).copied().filter(|n|*n>0).map(|n|n.to_string()).unwrap_or_default())}</span></button>}/><button class="chrome" on:click=move |_|ui.panel(Panel::People)><Icon name="plus"/>"Choose a Sim"</button></div>
        </Show>
        <Show when=move ||lot.get()&&ui.state.with(|s|s.lot_chat.channels.len()>1)>
            <details class="connected-more"><summary>"Show channels"</summary><div class="connected-action-row"><For each=move ||ui.state.with(|s|s.lot_chat.channels.clone()) key=|channel|(channel.id,channel.name.clone(),channel.description.clone(),channel.private) children=move |channel|{let id=channel.id;view!{<button class="chrome" type="button" title=channel.description aria-pressed=move ||ui.state.with(|s|s.lot_chat.channel_shown(id)).to_string() on:click=move |_|ui.state.update(|s|{s.lot_chat.toggle_channel(id);if s.lot_chat.at_bottom{s.lot_chat.mark_read();}})>{channel.name}{if channel.private {" · Private"} else {""}}</button>}}/></div></details>
        </Show>
        <div node_ref=log class="connected-chat-log" role="log" aria-live="polite" aria-relevant="additions" aria-label=move ||if lot.get(){"Lot messages"}else{"Private messages"} on:scroll=move |_|{
            if lot.get_untracked()&&ui.state.with_untracked(|s|s.lot_chat.ready)&&let Some(element)=log.get_untracked(){let top=element.scroll_top();let at_bottom=element.scroll_height()-element.client_height()-top<=2;ui.state.update(|s|{s.lot_chat.set_scroll(top,at_bottom);if at_bottom{s.lot_chat.mark_read();}});}
        }>
            <Show when=move ||lot.get()>
                <For each=move ||source_messages.get() key=|event|event.id.clone() children=move |event|{
                    let message=event.message;
                    let outgoing=ui.state.with_untracked(|s|s.session.as_ref().and_then(|session|session.avatar_id)==Some(message.sender_uid));
                    if message.kind==SourceChatKind::Join {
                        view!{<div class="connected-message"><small>{format!("{} joined the lot.",message.sender_name)}</small></div>}.into_any()
                    } else {
                        let channel=if message.channel_id==Some(0){String::new()}else{format!(" · {}{}",message.channel_name.unwrap_or_default(),if message.private{" (private)"}else{""})};
                        let color=format!("color:rgb({},{},{})",message.sender_color[0],message.sender_color[1],message.sender_color[2]);
                        view!{<div class="connected-message" class:outgoing=outgoing><small><strong style=color>{message.sender_name}</strong>{channel}</small><p>{message.text}</p></div>}.into_any()
                    }
                }/>
            </Show>
            <Show when=move ||!lot.get()>
            {move ||ui.state.with(|s|{
                let mut messages=Vec::new();
                if !lot.get(){for event in &s.events {if event.family=="instant_message"&&source_u32(&event.data,"from")==target.get(){messages.push((source_text(&event.data,"message"),false,"Received".to_owned()));}}}
                for (id,operation) in &s.sent {
                    let body=match operation {GatewayOperation::PrivateMessage {target_avatar_id,message,..} if !lot.get()&&Some(*target_avatar_id)==target.get()=>Some(message),_=>None};
                    if let Some(body)=body {
                        let status=s.ledger.operations.get(id).map(|o|match &o.status {OperationStatus::Accepted(_)=>"Delivered",OperationStatus::Rejected(_)=>"Not delivered",OperationStatus::Unknown(_)=>"Delivery unknown",_=>"Sending…"}).unwrap_or("Delivery unknown");
                        messages.push((body.clone(),true,status.into()));
                    }
                }
                messages.into_iter().map(|(body,outgoing,status)|view!{<div class="connected-message" class:outgoing=outgoing><p>{body}</p><small>{status}</small></div>}).collect_view()
            })}
            </Show>
        </div>
        <Show when=move ||lot.get()&&ui.state.with(|s|!s.lot_chat.at_bottom)><button class="chrome" type="button" on:click=move |_|{if let Some(element)=log.get_untracked(){element.set_scroll_top(element.scroll_height());ui.state.update(|s|{s.lot_chat.set_scroll(element.scroll_top(),true);s.lot_chat.mark_read();});}}>"Jump to latest"{move ||ui.state.with(|s|{let count=s.lot_chat.unread_count();if count>0{format!(" ({count} new)")}else{String::new()}})}</button></Show>
        <Show when=move ||lot.get()&&source_messages.get().is_empty()><p class="connected-empty">{move ||ui.state.with(|s|if s.lot_chat.ready {"No messages have arrived in the selected lot channels yet."}else{"Lot chat is waiting for the lot’s player information."})}</p></Show>
        <Show when=move ||!lot.get()&&target.get().is_none()><p class="connected-empty">"Choose a Sim to start a conversation."</p></Show>
        <form class="connected-compose" on:submit=move |event|{event.prevent_default();send();}><label for="connected-chat-message">{move ||if lot.get(){"Say something to the lot"}else{"Your message"}}</label><textarea id="connected-chat-message" rows="3" maxlength=move ||if lot.get(){200}else{1500} disabled=move ||!can_send() prop:value=move ||ui.state.with(|s|s.draft(&key())) on:input=move |event|ui.draft(&key(),event_target_value(&event))></textarea><button class="chrome primary" type="submit" disabled=move ||!can_send()||ui.state.with(|s|s.draft(&key()).trim().is_empty())>"Send"<Icon name="arrow-right"/></button></form>
        <Show when=move ||lot.get() fallback=||view!{<OperationFeedback label="Private message"/><CapabilityNote capability="private_message"/>}><OperationFeedback label="Lot chat"/><CapabilityNote capability="lot_chat"/></Show>
    }
}

#[component]
fn InboxPanel() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let compose = RwSignal::new(
        ui.state
            .with_untracked(|s| !s.draft("mail:recipient").is_empty()),
    );
    let confirm = RwSignal::new(false);
    let selected = Memo::new(move |_| {
        ui.state
            .with(|s| s.selected_mail.and_then(|id| s.inbox.get(&id).cloned()))
    });
    let recipient = Memo::new(move |_| {
        ui.state
            .with(|s| s.draft("mail:recipient").parse::<u32>().ok())
    });
    let subject_key = move || format!("mail:subject:{}", recipient.get().unwrap_or(0));
    let body_key = move || format!("mail:body:{}", recipient.get().unwrap_or(0));
    view! {
        <div class="connected-action-row"><button class="chrome" disabled=move ||!can(ui,"mail","Refresh inbox") on:click=move |_|ui.poll_mail()><Icon name="refresh"/>"Refresh inbox"</button><button class="chrome" on:click=move |_|{compose.set(true);if recipient.get_untracked().is_none(){ui.panel(Panel::People);}}>"Write mail"</button><Show when=move ||compose.get()><button class="connected-text-button" on:click=move |_|compose.set(false)>"Back to inbox"</button></Show></div>
        <OperationFeedback label="Refresh inbox"/><CapabilityNote capability="mail"/>
        <Show when=move ||!compose.get() fallback=move ||view!{
            <form class="connected-compose" on:submit=move |event|{event.prevent_default();if let Some(target_avatar_id)=recipient.get_untracked(){let subject=ui.state.with_untracked(|s|s.draft(&subject_key()));let body=ui.state.with_untracked(|s|s.draft(&body_key()));ui.send(GatewayOperation::MailSend {target_avatar_id,subject,body},"Send mail",Some(&body_key()));}}>
                <p>"To: "<strong>{move ||recipient.get().map(|id|person_name(&ui.state.get(),id)).unwrap_or_else(||"Choose a Sim".into())}</strong><button class="connected-text-button" type="button" on:click=move |_|ui.panel(Panel::People)>"Change"</button></p>
                <label for="connected-mail-subject">"Subject"</label><input id="connected-mail-subject" maxlength="128" required=true prop:value=move ||ui.state.with(|s|s.draft(&subject_key())) on:input=move |e|ui.draft(&subject_key(),event_target_value(&e))/>
                <label for="connected-mail-body">"Message"</label><textarea id="connected-mail-body" rows="7" maxlength="1500" required=true prop:value=move ||ui.state.with(|s|s.draft(&body_key())) on:input=move |e|ui.draft(&body_key(),event_target_value(&e))></textarea>
                <button class="chrome primary" disabled=move ||!can(ui,"mail","Send mail")||recipient.get().is_none()||ui.state.with(|s|s.draft(&body_key()).trim().is_empty())>"Send mail"<Icon name="arrow-right"/></button><OperationFeedback label="Send mail"/>
            </form>
        }>
            <div class="connected-mail-list" role="list" aria-label="Inbox"><For each=move ||ui.state.with(|s|s.inbox.iter().rev().map(|(id,message)|(*id,message.clone())).collect::<Vec<_>>()) key=|entry|entry.0 children=move |(id,message)|view!{<button role="listitem" class="connected-mail-row" class:unread=move ||ui.state.with(|s|!s.read_mail.contains(&id)) aria-pressed=move ||ui.state.with(|s|s.selected_mail==Some(id)).to_string() on:click=move |_|ui.state.update(|s|{s.selected_mail=Some(id);s.read_mail.insert(id);})><span><strong>{source_text(&message,"subject")}</strong><small>{source_text(&message,"sender_name")}</small></span><Icon name="chevron-right"/></button>}/></div>
            <Show when=move ||ui.state.with(|s|s.inbox.is_empty()&&!s.busy("Refresh inbox"))><p class="connected-empty">"No mail has been received in this session."</p></Show>
            <Show when=move ||selected.get().is_some()><article class="connected-mail-detail"><h3>{move ||selected.get().map(|m|source_text(&m,"subject"))}</h3><p class="connected-muted">{move ||selected.get().map(|m|format!("From {}",source_text(&m,"sender_name")))}</p><p class="connected-message-body">{move ||selected.get().map(|m|source_text(&m,"body"))}</p><div class="connected-action-row"><button class="chrome" on:click=move |_|if let Some(message)=selected.get_untracked()&& let Some(id)=source_u32(&message,"sender_id"){ui.draft("mail:recipient",id.to_string());let subject=source_text(&message,"subject");ui.draft(&format!("mail:subject:{id}"),format!("Re: {subject}"));compose.set(true);}>"Reply"</button><button class="chrome danger" disabled=move ||!can(ui,"mail","Delete mail") on:click=move |_|confirm.set(true)>"Delete…"</button></div><OperationFeedback label="Delete mail"/></article></Show>
            <ConfirmAction open=confirm title="Delete this message?" description="The world will be asked to delete this message. Refresh your inbox to confirm removal." confirm_label="Delete message" on_confirm=Callback::new(move |_|{if let Some(message_id)=ui.state.with_untracked(|s|s.selected_mail){ui.send(GatewayOperation::MailDelete {message_id},"Delete mail",None);}})/>
        </Show>
    }
}

fn person_name(state: &ConnectedState, id: u32) -> String {
    state
        .roster
        .iter()
        .find(|entry| entry.avatar_id == id)
        .map(|entry| entry.name.clone())
        .or_else(|| {
            state
                .reads
                .values()
                .filter_map(|slot| slot.result.as_ref())
                .flat_map(|result| DirectoryPage::from_result(result).rows)
                .find(|row| source_u32(row, "avatar_id") == Some(id))
                .map(|row| source_text(&row, "name"))
        })
        .unwrap_or_else(|| format!("Sim {id}"))
}

#[component]
fn NeighborhoodPanel() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let tab = RwSignal::new("bulletins");
    let data = Memo::new(move |_| ui.state.with(|s| s.data("neighborhood")));
    let id = Memo::new(move |_| source_u32(&data.get(), "neighborhood_id"));
    view! {
        <ReadStatus source=Signal::derive(||"neighborhood".to_owned())/>
        <Show when=move ||id.get().is_some() fallback=||view!{<p>"Choose a neighborhood."</p><DirectoryList source=Signal::derive(||"neighborhoods".to_owned())/>}>
            <div class="connected-neighborhood-hero"><Icon name="building-community"/><h3>{move ||source_text(&data.get(),"name")}</h3></div><p class="connected-description">{move ||source_text(&data.get(),"description")}</p>
            <dl class="connected-facts"><dt>"Mayor"</dt><dd>{move ||source_u32(&data.get(),"mayor_id").map(|id|view!{<PersonLink id=id/>})}</dd><dt>"Elected"</dt><dd>{move ||date_seconds(&data.get(),"mayor_elected_date")}</dd></dl>
            <div class="connected-action-row"><Show when=move ||source_u32(&data.get(),"town_hall_id").is_some()><button class="chrome" on:click=move |_|if let Some(id)=source_u32(&data.get_untracked(),"town_hall_id"){ui.select_lot(id);}>"Town hall"</button></Show><button class="chrome" on:click=move |_|if let Some(id)=id.get_untracked(){ui.select_neighborhood(id);}>"Refresh"</button></div>
            <nav class="connected-tabs" aria-label="Neighborhood sections"><For each=||vec![("bulletins","Bulletins"),("elections","Elections"),("people","People"),("lots","Places")] key=|v|v.0 children=move |(value,label)|view!{<button class="chrome" aria-pressed=move ||(tab.get()==value).to_string() on:click=move |_|{tab.set(value);if let (Some(neighborhood_id),Some(shard_id))=(id.get_untracked(),ui.state.with_untracked(|s|s.shard().map(|s|s.id))){if value=="people"{ui.query("neighborhood_people",DirectoryQuery::NeighborhoodAvatars {shard_id,neighborhood_id});}else if value=="lots"{ui.query("neighborhood_lots",DirectoryQuery::NeighborhoodLots {shard_id,neighborhood_id});}}}>{label}</button>}/></nav>
            {move ||match tab.get(){"bulletins"=>view!{<BulletinPanel/>}.into_any(),"elections"=>view!{<ElectionPanel/>}.into_any(),"people"=>view!{<DirectoryList source=Signal::derive(||"neighborhood_people".to_owned())/>}.into_any(),_=>view!{<DirectoryList source=Signal::derive(||"neighborhood_lots".to_owned())/>}.into_any()}}
        </Show>
    }
}

fn bulletin_request(action: BulletinAction, neighborhood_id: u32, value: u32) -> GatewayOperation {
    GatewayOperation::Bulletin {
        action,
        neighborhood_id,
        title: String::new(),
        message: String::new(),
        lot_id: 0,
        value,
    }
}

fn probe_allowed(state: &ConnectedState, label: &str) -> bool {
    state.last_operation(label).is_some_and(|operation| {
        matches!(operation.status, OperationStatus::Accepted(_))
            && state
                .wire_epochs
                .get(&operation.stamp.operation_id)
                .copied()
                == state.session.as_ref().map(|s| s.epoch)
    })
}

#[component]
fn BulletinPanel() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let compose = RwSignal::new(false);
    let selected = RwSignal::new(None::<u32>);
    let confirm = RwSignal::new(false);
    let id = Memo::new(move |_| ui.state.with(|s| s.selected_neighborhood));
    let title_key = move || format!("bulletin:title:{}", id.get().unwrap_or(0));
    let body_key = move || format!("bulletin:body:{}", id.get().unwrap_or(0));
    let probe = move || format!("Check posting {}", id.get().unwrap_or(0));
    let detail = Memo::new(move |_| ui.state.with(|s| s.data("bulletin")));
    view! {
        <div class="connected-filters"><For each=||vec![("all","All"),("mayor","Mayor"),("community","Community"),("system","Announcements")] key=|x|x.0 children=move |(filter,label)|view!{<button class="connected-text-button" on:click=move |_|if let Some(neighborhood_id)=id.get_untracked(){ui.query("bulletins",if filter=="all"{DirectoryQuery::Bulletins {neighborhood_id}}else{DirectoryQuery::BulletinsByType {neighborhood_id,bulletin_type:filter.into()}});}>{label}</button>}/></div>
        <div class="connected-action-row"><button class="chrome" disabled=move ||!can(ui,"bulletin",&probe()) on:click=move |_|{compose.set(true);if let Some(id)=id.get_untracked(){ui.send(bulletin_request(BulletinAction::CanPostMessage,id,1),&probe(),None);}}>"Write a bulletin"</button><button class="connected-text-button" on:click=move |_|if let Some(neighborhood_id)=id.get_untracked(){ui.query("bulletins",DirectoryQuery::Bulletins {neighborhood_id});}>"Refresh"</button></div>
        <Show when=move ||selected.get().is_some()><section class="connected-bulletin-detail" aria-labelledby="connected-bulletin-heading"><div class="connected-action-row"><h4 id="connected-bulletin-heading" tabindex="-1">"Selected bulletin"</h4><button class="chrome round small" aria-label="Close selected bulletin" on:click=move |_|selected.set(None)><Icon name="x"/></button></div><ReadStatus source=Signal::derive(||"bulletin".to_owned())/><Show when=move ||source_u32(&detail.get(),"bulletin_id")==selected.get()&&source_u32(&detail.get(),"neighborhood_id")==id.get()><h3>{move ||source_text(&detail.get(),"title")}</h3><p class="connected-muted">{move ||date_seconds(&detail.get(),"date")}</p><p class="connected-message-body">{move ||source_text(&detail.get(),"body")}</p>{move ||source_u32(&detail.get(),"avatar_id").map(|author|view!{<PersonLink id=author/>})}<Show when=move ||source_u32(&detail.get(),"lot_id").is_some()><button class="chrome" on:click=move |_|if let Some(lot_id)=source_u32(&detail.get_untracked(),"lot_id"){ui.select_lot(lot_id);}>"View attached property"</button></Show></Show></section></Show>
        <ReadStatus source=Signal::derive(||"bulletins".to_owned())/>
        <For each=move ||ui.state.with(|s|s.page("bulletins").rows) key=|row|source_u32(row,"bulletin_id") children=move |row|{
            let bulletin_id=source_u32(&row,"bulletin_id");let owner=source_u32(&row,"avatar_id");
            view!{<article class="connected-bulletin"><button class="connected-bulletin-title" on:click=move |_|if let (Some(neighborhood_id),Some(bulletin_id))=(id.get_untracked(),bulletin_id){selected.set(Some(bulletin_id));ui.query("bulletin",DirectoryQuery::Bulletin {neighborhood_id,bulletin_id});focus("connected-bulletin-heading");}><h4>{source_text(&row,"title")}</h4><small>{date_seconds(&row,"date")}</small></button><p class="connected-message-body">{source_text(&row,"body")}</p><div class="connected-action-row">{owner.map(|id|view!{<PersonLink id=id/>})}{source_u32(&row,"lot_id").map(|lot_id|view!{<button class="connected-text-button" on:click=move |_|ui.select_lot(lot_id)>"View property"</button>})}<Show when=move ||ui.state.with(|s|s.session.as_ref().and_then(|s|s.avatar_id).is_some_and(|actor|Some(actor)==owner||source_u32(&s.data("neighborhood"),"mayor_id")==Some(actor)))><button class="connected-text-button" disabled=move ||!can(ui,"bulletin","Delete bulletin") on:click=move |_|{selected.set(bulletin_id);confirm.set(true);}>"Delete…"</button></Show><Show when=move ||ui.state.with(|s|s.session.as_ref().and_then(|s|s.avatar_id).is_some_and(|actor|source_u32(&s.data("neighborhood"),"mayor_id")==Some(actor)))><button class="connected-text-button" disabled=move ||!can(ui,"bulletin","Promote bulletin") on:click=move |_|if let (Some(neighborhood_id),Some(value))=(id.get_untracked(),bulletin_id){ui.send(bulletin_request(BulletinAction::PromoteMessage,neighborhood_id,value),"Promote bulletin",None);}>"Promote"</button></Show></div></article>}
        }/>
        <Show when=move ||ui.state.with(|s|s.read("bulletins").status==LoadState::Ready&&s.page("bulletins").rows.is_empty())><p class="connected-empty">"No bulletins have been posted."</p></Show>
        <Show when=move ||compose.get()><form class="connected-compose" on:submit=move |event|{event.prevent_default();if let Some(neighborhood_id)=id.get_untracked(){let title=ui.state.with_untracked(|s|s.draft(&title_key()));let message=ui.state.with_untracked(|s|s.draft(&body_key()));ui.send(GatewayOperation::Bulletin {action:BulletinAction::PostMessage,neighborhood_id,title,message,lot_id:0,value:0},"Post bulletin",Some(&body_key()));}}><h4>"Post to your neighborhood"</h4>{move ||view!{<OperationFeedback label=probe()/>}}<label for="connected-bulletin-title">"Title"</label><input id="connected-bulletin-title" required=true prop:value=move ||ui.state.with(|s|s.draft(&title_key())) on:input=move |e|ui.draft(&title_key(),event_target_value(&e))/><label for="connected-bulletin-body">"Message"</label><textarea id="connected-bulletin-body" rows="5" required=true prop:value=move ||ui.state.with(|s|s.draft(&body_key())) on:input=move |e|ui.draft(&body_key(),event_target_value(&e))></textarea><button class="chrome primary" disabled=move ||!can(ui,"bulletin","Post bulletin")||!ui.state.with(|s|probe_allowed(s,&probe()))>"Post bulletin"</button><OperationFeedback label="Post bulletin"/></form></Show>
        <OperationFeedback label="Delete bulletin"/><OperationFeedback label="Promote bulletin"/><CapabilityNote capability="bulletin"/>
        <ConfirmAction open=confirm title="Delete this bulletin?" description="The world will check your permission and remove the selected bulletin if allowed." confirm_label="Delete bulletin" on_confirm=Callback::new(move |_|if let (Some(neighborhood_id),Some(value))=(id.get_untracked(),selected.get_untracked()){ui.send(bulletin_request(BulletinAction::DeleteMessage,neighborhood_id,value),"Delete bulletin",None);})/>
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CivicChoice {
    Vote,
    Nominate,
    Rate,
    Run,
    FreeVote,
}
impl CivicChoice {
    fn label(self) -> &'static str {
        match self {
            Self::Vote => "Vote",
            Self::Nominate => "Nominate",
            Self::Rate => "Rate mayor",
            Self::Run => "Run for mayor",
            Self::FreeVote => "Choose voting neighborhood",
        }
    }
    fn check(self) -> NeighborhoodAction {
        match self {
            Self::Vote => NeighborhoodAction::CanVote,
            Self::Nominate => NeighborhoodAction::CanNominate,
            Self::Rate => NeighborhoodAction::CanRate,
            Self::Run => NeighborhoodAction::CanRun,
            Self::FreeVote => NeighborhoodAction::CanFreeVote,
        }
    }
    fn commit(self) -> NeighborhoodAction {
        match self {
            Self::Vote => NeighborhoodAction::Vote,
            Self::Nominate => NeighborhoodAction::Nominate,
            Self::Rate => NeighborhoodAction::Rate,
            Self::Run => NeighborhoodAction::NominationRun,
            Self::FreeVote => NeighborhoodAction::FreeVote,
        }
    }
}

#[component]
fn ElectionPanel() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let mode = RwSignal::new(None::<CivicChoice>);
    let chosen = RwSignal::new(None::<u32>);
    let rating = RwSignal::new(0u32);
    let confirm = RwSignal::new(false);
    let id = Memo::new(move |_| ui.state.with(|s| s.selected_neighborhood));
    let data = Memo::new(move |_| ui.state.with(|s| s.data("elections")));
    let label = move || {
        format!(
            "{} eligibility {}",
            mode.get().map(|m| m.label()).unwrap_or("Election"),
            id.get().unwrap_or(0)
        )
    };
    let message_key = move || {
        format!(
            "civic:{}:{}",
            mode.get().map(|m| m.label()).unwrap_or("comment"),
            id.get().unwrap_or(0)
        )
    };
    let submit = move || {
        let (Some(mode), Some(neighborhood_id)) = (mode.get_untracked(), id.get_untracked()) else {
            return;
        };
        let target_avatar_id = if mode == CivicChoice::Rate {
            ui.state
                .with_untracked(|s| source_u32(&s.data("neighborhood"), "mayor_id"))
                .unwrap_or(0)
        } else {
            chosen.get_untracked().unwrap_or(0)
        };
        let message = ui.state.with_untracked(|s| s.draft(&message_key()));
        ui.send(
            GatewayOperation::Neighborhood {
                action: mode.commit(),
                target_avatar_id,
                neighborhood_id: if mode == CivicChoice::FreeVote {
                    chosen.get_untracked().unwrap_or(neighborhood_id)
                } else {
                    neighborhood_id
                },
                message,
                value: rating.get_untracked(),
            },
            mode.label(),
            Some(&message_key()),
        );
    };
    view! {
        <ReadStatus source=Signal::derive(||"elections".to_owned())/>
        <dl class="connected-facts"><dt>"Election"</dt><dd>{move ||source_enum(&data.get(),"current_state",&["Closed","Nominations open","Voting open","Ended","Awaiting recovery"])}</dd><dt>"Starts"</dt><dd>{move ||date_seconds(&data.get(),"start_date")}</dd><dt>"Ends"</dt><dd>{move ||date_seconds(&data.get(),"end_date")}</dd></dl>
        <div class="connected-candidates"><For each=move ||ui.state.with(|s|s.page("elections").rows) key=|row|source_u32(row,"candidate_avatar_id") children=move |row|view!{<article class="connected-candidate">{source_u32(&row,"candidate_avatar_id").map(|id|view!{<PersonLink id=id/>})}<p>{source_text(&row,"comment")}</p><small>{source_enum(&row,"state",&["Informed","Running","Disqualified","Not elected","Elected"])}</small></article>}/></div>
        <div class="connected-action-row">{[CivicChoice::Vote,CivicChoice::Nominate,CivicChoice::Rate,CivicChoice::Run,CivicChoice::FreeVote].into_iter().map(move |choice|view!{<button class="chrome" disabled=move ||!can(ui,"neighborhood",&label()) on:click=move |_|{
            mode.set(Some(choice));chosen.set(None);if let Some(neighborhood_id)=id.get_untracked(){let target_avatar_id=if choice==CivicChoice::Rate{ui.state.with_untracked(|s|source_u32(&s.data("neighborhood"),"mayor_id")).unwrap_or(0)}else{0};ui.send(GatewayOperation::Neighborhood {action:choice.check(),target_avatar_id,neighborhood_id,message:String::new(),value:0},&label(),None);}
        }>{choice.label()}</button>}).collect_view()}</div>
        <Show when=move ||mode.get().is_some()>{move ||view!{<OperationFeedback label=label()/>}}
            <Show when=move ||ui.state.with(|s|probe_allowed(s,&label()))>
                <Show when=move ||mode.get().is_some_and(|m|matches!(m,CivicChoice::Rate|CivicChoice::Run)) fallback=move ||view!{
                    <div class="connected-candidates"><For each=move ||ui.state.with(|s|{
                        let operation=s.last_operation(&label());
                        s.events.iter().rev().find(|event|event.family=="neighborhood_candidates"&&event.operation_id.as_ref()==operation.as_ref().map(|o|&o.stamp.operation_id)).and_then(|event|event.data.get("candidates").and_then(Value::as_array)).cloned().unwrap_or_default()
                    }) key=|row|source_u32(row,"avatar_id") children=move |row|{let candidate=source_u32(&row,"avatar_id");view!{<button class="chrome connected-candidate" aria-pressed=move ||(chosen.get()==candidate).to_string() on:click=move |_|chosen.set(candidate)><strong>{source_text(&row,"name")}</strong><p>{source_text(&row,"message")}</p></button>}}/></div>
                    <p class="connected-muted">"Choose from the eligible candidates returned by the world."</p>
                }>
                    <label class="connected-field">"Your comment"<textarea rows="3" maxlength="140" prop:value=move ||ui.state.with(|s|s.draft(&message_key())) on:input=move |e|ui.draft(&message_key(),event_target_value(&e))></textarea></label><Show when=move ||mode.get()==Some(CivicChoice::Rate)><label class="connected-field">{move ||format!("Rating: {} stars",rating.get() as f64/2.)}<input type="range" min="0" max="10" step="1" prop:value=move ||rating.get().to_string() on:input=move |e|rating.set(event_target_value(&e).parse().unwrap_or(0))/></label></Show>
                </Show>
                <button class="chrome primary" disabled=move ||!can(ui,"neighborhood",mode.get().map(|m|m.label()).unwrap_or("Vote"))||mode.get().is_some_and(|m|!matches!(m,CivicChoice::Rate|CivicChoice::Run))&&chosen.get().is_none() on:click=move |_|confirm.set(true)>{move ||mode.get().map(|m|m.label())}</button>
            </Show>
            {move ||mode.get().map(|mode|view!{<OperationFeedback label=mode.label()/>})}
        </Show><CapabilityNote capability="neighborhood"/>
        <ConfirmAction open=confirm title="Submit this choice?" description="The world will verify this selection and record it if you are eligible. A vote or nomination may be final for this election." confirm_label="Submit choice" on_confirm=Callback::new(move |_|submit())/>
    }
}

#[component]
fn WardrobePanel() -> impl IntoView {
    view! {<crate::connected_authoring::ConnectedAuthoringPanel kind=crate::connected_authoring::AuthoringPanelKind::Wardrobe/>}
}

#[component]
fn EodPanel() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let active = Memo::new(move |_| {
        ui.state.with(|s| {
            let session = s
                .session
                .as_ref()
                .filter(|session| session.state == SessionState::LotReady)?;
            let incarnation = session.lot_incarnation?;
            s.events
                .iter()
                .rev()
                .find(|event| {
                    event.family == "eod"
                        && event.data.get("lot_incarnation").and_then(Value::as_u64)
                            == Some(incarnation)
                        && matches!(
                            source_text(&event.data, "event_name").as_str(),
                            "eod_enter" | "eod_leave"
                        )
                })
                .filter(|event| source_text(&event.data, "event_name") == "eod_enter")
                .map(|event| event.data.clone())
        })
    });
    view! {<Show when=move ||active.get().is_some() fallback=||view!{<div class="connected-empty"><Icon name="device-gamepad-2"/><h3>"No object dialog is open"</h3><p>"Use an object in the lot to open its game, job, trade, or wardrobe controls."</p><CapabilityNote capability="eod"/></div>}>
        <h3>{move ||active.get().and_then(|data|source_u32(&data,"plugin_id")).map(eod_name).unwrap_or("Object dialog")}</h3>
        <Show when=move ||active.get().and_then(|data|source_u32(&data,"plugin_id")).is_some_and(|id|matches!(id,0x8b300068|0xcb492685|0x2b58020b)) fallback=||view!{<p class="connected-capability">"This object's session is connected. Its game controls are not yet available in this client."</p>}><WardrobePanel/></Show>
    </Show>}
}

fn eod_name(id: u32) -> &'static str {
    match id {
        0x2a6356a0 => "Sign",
        0x4a5be8ab => "Dance floor",
        0xea47ae39 => "Pizza maker",
        0xca418206 => "Paper chase",
        0x2b58020b => "Clothing rack management",
        0xcb492685 => "Clothing rack",
        0x8b300068 => "Dresser",
        0x0949e698 => "Scoreboard",
        0x0a69f29f => "Door permissions",
        0xcb2819cb => "Slots",
        0xaa5e36dc => "Costume trunk",
        0x2d642d39 => "War game",
        0xaa65fe9e => "Timer",
        0x895c1ceb => "Draw a card",
        0x8adfc7a2 => "Band",
        0x0b2a6b83 => "Roulette",
        0x897f82f5 => "Secure trade",
        0x2b2fc514 => "Blackjack",
        0x4a245a22 => "Maze job",
        0xec55d705 => "Nightclub dance floor",
        0x6c5c7555 => "DJ station",
        0x1000 => "Newspaper",
        0x1001 => "Hold'em",
        0x1003 => "Bulletin board",
        0x1004 => "Event",
        0x1005 | 0x1006 => "Game show",
        0x2000 => "Property selection",
        _ => "Object dialog",
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CreatePart {
    Head,
    Body,
    Skin,
    Gender,
}
impl CreatePart {
    fn key(self) -> &'static str {
        match self {
            Self::Head => "create:head",
            Self::Body => "create:body",
            Self::Skin => "create:skin",
            Self::Gender => "create:gender",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Head => "Head",
            Self::Body => "Body",
            Self::Skin => "Skin tone",
            Self::Gender => "Gender",
        }
    }
}

fn creation_appearance(state: &ConnectedState) -> AppearanceSelection {
    let key = |name| {
        let value = state.draft(name);
        if value.is_empty() {
            None
        } else {
            Some(ContentKey::from(value))
        }
    };
    AppearanceSelection {
        head: key("create:head"),
        body: key("create:body"),
        skin_tone: key("create:skin"),
        gender: key("create:gender"),
        ..AppearanceSelection::default()
    }
}

#[component]
fn ConnectedPartChoices(part: CreatePart) -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let content = expect_context::<ContentUi>();
    let page = RwSignal::new(0usize);
    let options = Memo::new(move |_| {
        content.choices.with(|c| {
            c.as_ref()
                .map(|c| match part {
                    CreatePart::Head => c.heads.clone(),
                    CreatePart::Body => c.bodies.clone(),
                    CreatePart::Skin => c.skin_tones.clone(),
                    CreatePart::Gender => c.genders.clone(),
                })
                .unwrap_or_default()
        })
    });
    Effect::new(move |_| {
        page.update(|page| *page = (*page).min(options.get().len().saturating_sub(1) / 18))
    });
    view! {<fieldset class="appearance-options" class:part-grid=matches!(part,CreatePart::Head|CreatePart::Body)><legend>{part.label()}</legend><div class="appearance-grid"><For each=move ||{options.get().into_iter().skip(page.get()*18).take(18).collect::<Vec<_>>()} key=|option|option.key.clone() children=move |option|{
        let key=option.key.clone();let selected=key.clone();let image_key=key.clone();let available=option.availability.is_available();let reason=crate::components::availability_reason(&option.availability).unwrap_or_default();
        view!{<button class="chrome look-card" aria-pressed=move ||ui.state.with(|s|s.draft(part.key())==selected.as_ref()).to_string() disabled=move ||!available||ui.state.with(|s|s.busy("Create Sim")) title=reason on:click=move |_|ui.draft(part.key(),key.to_string())><Show when=move ||matches!(part,CreatePart::Head|CreatePart::Body)><OriginalThumbnail content_key=image_key.clone() skin=Signal::derive(move ||ui.state.with(|s|creation_appearance(s).skin_tone))/></Show><span>{option.label}</span></button>}
    }/></div><Show when=move ||options.get().is_empty()><p class="connected-muted">"Load the original collections to choose this part."</p></Show><Show when=move ||{options.get().len()>18}><nav class="choice-pages" aria-label=format!("{} pages",part.label())><button class="chrome" disabled=move ||page.get()==0 on:click=move |_|page.update(|p|*p=p.saturating_sub(1))>"Previous"</button><span>{move ||format!("{} / {} · {} choices",page.get()+1,options.get().len().div_ceil(18),options.get().len())}</span><button class="chrome" disabled=move ||{(page.get()+1)*18>=options.get().len()} on:click=move |_|page.update(|p|*p+=1)>"Next"</button></nav></Show></fieldset>}
}

#[component]
pub fn ConnectedCreator() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let content = expect_context::<ContentUi>();
    let part = RwSignal::new(CreatePart::Head);
    let original_choices = Memo::new(move |_| content.choices.get());
    Effect::new(move |_| {
        if let Some(choices) = original_choices.get() {
            let default = choices.default_selection();
            ui.state.update(|s| {
                for (name, value) in [
                    ("create:head", default.head),
                    ("create:body", default.body),
                    ("create:skin", default.skin_tone),
                    ("create:gender", default.gender),
                ] {
                    if s.draft(name).is_empty()
                        && let Some(value) = value
                    {
                        s.ledger.drafts.insert(name.into(), value.to_string());
                    }
                }
            });
        }
    });
    let appearance = Signal::derive(move || ui.state.with(creation_appearance));
    let ready = move || {
        ui.state.with(|s| {
            s.session
                .as_ref()
                .is_some_and(|s| s.state == SessionState::CityReady && s.avatar_id == Some(0))
        })
    };
    let validation = move || {
        content.choices.with(|choices| match choices {
            None => Some("Load original game content to choose your Sim's appearance.".to_owned()),
            Some(choices) => choices
                .validate_selection(&appearance.get())
                .err()
                .map(|error| error.to_string()),
        })
    };
    let save = move || {
        if let Some(error) = validation() {
            ui.notice(&error);
            return;
        }
        let selected = appearance.get_untracked();
        let packed = |key: Option<ContentKey>| {
            key.and_then(|key| crate::source_identity::outfit_key(key.as_ref()))
                .map(|(file, kind)| DecimalU64((u64::from(file) << 32) | u64::from(kind)))
        };
        let (Some(head_key), Some(body_key)) = (packed(selected.head), packed(selected.body))
        else {
            ui.notice("Choose an original head and body.");
            return;
        };
        let gender = match selected.gender.as_ref().map(|key| key.as_ref()) {
            Some("vitaboy:gender:0") => Gender::Male,
            Some("vitaboy:gender:1") => Gender::Female,
            _ => {
                ui.notice("Choose a supported gender from the source collection.");
                return;
            }
        };
        let skin = match selected.skin_tone.as_ref().map(|key| key.as_ref()) {
            Some("vitaboy:skin:light") => SkinTone::Light,
            Some("vitaboy:skin:medium") => SkinTone::Medium,
            Some("vitaboy:skin:dark") => SkinTone::Dark,
            _ => {
                ui.notice("Choose a skin tone.");
                return;
            }
        };
        let name = ui.state.with_untracked(|s| s.draft("create:name"));
        let description = ui.state.with_untracked(|s| s.draft("create:description"));
        ui.send(
            GatewayOperation::CreateAvatar {
                name,
                description,
                gender,
                skin,
                head_key,
                body_key,
            },
            "Create Sim",
            Some("create:name"),
        );
    };
    view! {
        <section class="avatars-screen creator-screen connected-creator" aria-labelledby="connected-create-title"><h1 id="connected-create-title" class="choose-title">"Create a Sim"</h1>
            <div class="creator-stage"><img class="stage-diamond" src="/assets/art/selection-diamond.png" alt=""/><AvatarStage appearance=appearance/></div>
            <div class="creator-choices">
                <label class="connected-create-city">"City"<select prop:value=move ||ui.state.with(|s|s.selected_shard.clone().unwrap_or_default()) disabled=ready on:change=move |e|ui.state.update(|s|s.selected_shard=Some(event_target_value(&e)))><For each=move ||ui.state.with(|s|s.shards.clone()) key=|shard|shard.id children=|shard|view!{<option value=shard.name.clone()>{shard.name.clone()}</option>}/></select></label>
                <Show when=move ||!ready()><button class="chrome" disabled=move ||!can(ui,"city","Connect creator") on:click=move |_|if let Some(shard_name)=ui.state.with_untracked(|s|s.selected_shard.clone()){ui.send(GatewayOperation::ConnectCity {shard_name,avatar_id:0},"Connect creator",None);}>"Connect for character creation"</button><OperationFeedback label="Connect creator"/></Show>
                <ContentLoader/>
                <nav class="creator-part-tabs" aria-label="Character parts"><button class="chrome" aria-pressed=move ||(part.get()==CreatePart::Head).to_string() on:click=move |_|part.set(CreatePart::Head)>"Head"</button><button class="chrome" aria-pressed=move ||(part.get()==CreatePart::Body).to_string() on:click=move |_|part.set(CreatePart::Body)>"Body"</button></nav>
                <Show when=move ||part.get()==CreatePart::Head fallback=||view!{<ConnectedPartChoices part=CreatePart::Body/>}><ConnectedPartChoices part=CreatePart::Head/></Show>
                <div class="creator-traits"><ConnectedPartChoices part=CreatePart::Skin/><ConnectedPartChoices part=CreatePart::Gender/></div>
                <label class="chrome nameplate"><span>"Name"</span><input id="connected-create-name" autocomplete="off" prop:value=move ||ui.state.with(|s|s.draft("create:name")) disabled=move ||ui.state.with(|s|s.busy("Create Sim")) on:input=move |e|ui.draft("create:name",event_target_value(&e))/></label>
                <details class="creator-description"><summary>"Description"</summary><label class="chrome nameplate"><span>"About your Sim"</span><textarea prop:value=move ||ui.state.with(|s|s.draft("create:description")) disabled=move ||ui.state.with(|s|s.busy("Create Sim")) on:input=move |e|ui.draft("create:description",event_target_value(&e))></textarea></label></details>
                <p class="creator-validity" role="status">{move ||validation().unwrap_or_default()}</p><CapabilityNote capability="create_avatar"/><OperationFeedback label="Create Sim"/>
                <div class="creator-actions"><button class="chrome" disabled=move ||ui.state.with(|s|s.busy("Create Sim")) on:click=move |_|{ui.state.update(|s|s.panel=None);if ready(){ui.send(GatewayOperation::DisconnectCity,"Leave creator",None);}focus("connected-play");}>"Back to Sims"</button><button class="chrome primary" disabled=move ||!ready()||!can(ui,"create_avatar","Create Sim")||validation().is_some()||ui.state.with(|s|s.draft("create:name").trim().is_empty()) on:click=move |_|save()>"Create Sim"</button></div>
            </div>
        </section>
    }
}
