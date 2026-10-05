//! Contextual authoring backed exclusively by source VM/EOD data and receipts.
use crate::connected_adapter::OperationStatus;
use crate::connected_bridge::ConnectedUi;
use leptos::prelude::*;
use wonderland_game_services::{GatewayOperation, SessionState};
use wonderland_player_authoring::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthoringPanelKind {
    Wardrobe,
    Catalog,
    Object,
    Build,
}
#[derive(Clone, Copy)]
pub struct SourceAuthoringUi {
    pub current_actor: RwSignal<Option<SourceActorLot>>,
    pub state: RwSignal<AuthoringState>,
    pub dispatch: Callback<AuthoringIntent>,
    pub selected_outfit: RwSignal<Option<u32>>,
    pub selected_catalog: RwSignal<Option<u32>>,
    pub selected_inventory: RwSignal<Option<u32>>,
    pub selected_entity: RwSignal<Option<EntityIdentity>>,
    pub placement: RwSignal<Option<Placement>>,
    pub selected_build: RwSignal<Option<BuildResource>>,
    pub architecture_draft: RwSignal<Vec<ArchitectureCommand>>,
    pub build_draft: RwSignal<BuildTileDraft>,
    pub notice: RwSignal<String>,
}
fn player_error(error: &AuthoringError) -> String {
    match error {
        AuthoringError::Pending => {
            "Waiting for the world's response. Your choices are saved.".into()
        }
        AuthoringError::Permission | AuthoringError::EodOwnership => {
            "This action is not available for the selected item or your current permissions.".into()
        }
        AuthoringError::Stale => "The selected item or lot changed. Select it again.".into(),
        AuthoringError::Price => "Choose an amount permitted by this shop.".into(),
        AuthoringError::Missing(_) => {
            "Open the matching object dialog or wait for the lot's items to load.".into()
        }
        _ => "The world could not use this selection. Choose an item and position again.".into(),
    }
}
impl SourceAuthoringUi {
    /// Actual provider entry: install only a decoded authoritative source snapshot.
    /// This constructor never creates inventory, price, ownership or build data.
    pub fn new(dispatch: Callback<AuthoringIntent>) -> Self {
        Self {
            current_actor: RwSignal::new(None),
            state: RwSignal::new(AuthoringState::default()),
            dispatch,
            selected_outfit: RwSignal::new(None),
            selected_catalog: RwSignal::new(None),
            selected_inventory: RwSignal::new(None),
            selected_entity: RwSignal::new(None),
            placement: RwSignal::new(None),
            selected_build: RwSignal::new(None),
            architecture_draft: RwSignal::new(vec![]),
            build_draft: RwSignal::new(BuildTileDraft::default()),
            notice: RwSignal::new(String::new()),
        }
    }
    pub fn install(self, snapshot: AuthoringSnapshot) {
        self.state.update(|state| {
            if let Err(error) = state.install(snapshot) {
                self.notice.set(player_error(&error));
            }
        });
    }
    /// Called only after the world adapter verifies current lot incarnation.
    /// The generation is local presentation freshness, not a server revision.
    pub fn observe_snapshot(
        self,
        source: &wonderland_vm_protocol::snapshot::Snapshot,
        presentation_generation: u64,
    ) -> Result<(), AuthoringError> {
        let actor = self
            .current_actor
            .get_untracked()
            .ok_or(AuthoringError::Missing("source lot session"))?;
        let result = self
            .state
            .try_update(|s| s.observe_snapshot(actor, source, presentation_generation))
            .ok_or(AuthoringError::Stale)?;
        if result.is_ok() {
            self.selected_entity.set(None);
            self.placement.set(None);
            self.architecture_draft.set(vec![]);
            self.build_draft.update(BuildTileDraft::cancel);
            self.notice.set(String::new());
        }
        result
    }
    pub fn can(self, intent: &AuthoringIntent) -> bool {
        self.state.with(|state| {
            (state.pending.is_none() || matches!(intent, AuthoringIntent::CloseEod))
                && state.prepare(intent).is_ok()
        })
    }
    pub fn invalidate_world_projection(self) {
        self.state
            .update(AuthoringState::invalidate_world_projection);
        self.selected_catalog.set(None);
        self.selected_inventory.set(None);
        self.selected_entity.set(None);
        self.placement.set(None);
        self.architecture_draft.set(vec![]);
        self.selected_build.set(None);
        self.notice
            .set("The world changed. Refresh the lot to choose another edit.".into());
        self.build_draft.update(BuildTileDraft::cancel);
    }
    pub fn abandon_wait(self) {
        self.state.update(AuthoringState::abandon_pending);
    }
    pub fn reset_build_draft(self) {
        self.build_draft.update(BuildTileDraft::cancel);
        self.architecture_draft.set(vec![]);
        self.notice.set(String::new());
    }
    pub fn cancel_build(self) {
        self.reset_build_draft();
        self.selected_build.set(None);
    }
    pub fn select_build(self, resource: BuildResource) {
        self.reset_build_draft();
        self.selected_catalog.set(None);
        self.selected_inventory.set(None);
        self.selected_entity.set(None);
        self.placement.set(None);
        self.selected_build.set(Some(resource));
    }
    /// Consume a scene tile only while a building resource is selected. World
    /// freshness, original permission and supplied resource membership are checked
    /// before any draft is created. This never submits or edits displayed geometry.
    pub fn pick_build_tile(self, x: u16, y: u16, level: u8) -> bool {
        let Some(resource) = self.selected_build.get_untracked() else {
            return false;
        };
        if let Some(reason) = tile_build_unavailable_reason(&resource) {
            self.notice.set(reason.into());
            return true;
        }
        let tile = BuildTile { x, y, level };
        let context = self.state.with_untracked(|s| {
            if !s.world_projection_valid {
                return Err(AuthoringError::Missing("current world projection"));
            }
            if s.pending.is_some() {
                return Err(AuthoringError::Pending);
            }
            let snapshot = s
                .snapshot
                .as_ref()
                .ok_or(AuthoringError::Missing("source authoring projection"))?;
            if snapshot.permission.unwrap_or(0) < 2 {
                return Err(AuthoringError::Permission);
            }
            if !snapshot.build_resources.contains(&resource) {
                return Err(AuthoringError::Missing("source building resource"));
            }
            Ok((
                BuildDraftContext {
                    actor: snapshot.actor.clone(),
                    generation: s
                        .presentation_generation
                        .ok_or(AuthoringError::Missing("current world projection"))?,
                    bounds: snapshot
                        .bounds
                        .clone()
                        .ok_or(AuthoringError::Missing("source lot bounds"))?,
                },
                s.diagonal_floor_tiles
                    .as_ref()
                    .map(|tiles| tiles.contains(&(x, y, level))),
            ))
        });
        let result = context.and_then(|(context, diagonal)| {
            self.build_draft
                .try_update(|d| d.pick(&context, &resource, tile, diagonal))
                .ok_or(AuthoringError::Stale)?
        });
        match result {
            Ok(BuildPick::Started) => {
                self.architecture_draft.set(vec![]);
                self.notice.set(String::new());
            }
            Ok(BuildPick::Ready) => {
                self.architecture_draft.set(
                    self.build_draft
                        .get_untracked()
                        .command
                        .into_iter()
                        .collect(),
                );
                self.notice.set(String::new());
            }
            Err(error) => {
                self.architecture_draft.set(vec![]);
                self.notice.set(match error{AuthoringError::Missing("current world projection")=>"The world changed. Refresh the lot before building.".into(),AuthoringError::Missing("diagonal floor half selection")=>"This floor has two diagonal halves. Use a larger rectangle or select a whole room.".into(),AuthoringError::Invalid("snapped wall endpoint outside source lot")=>"The wall snaps beyond this lot. Choose a closer end vertex.".into(),AuthoringError::Missing("pool requires rectangle tool")=>"Choose a rectangle tool for a pool or water area.".into(),_=>player_error(&error)});
            }
        }
        true
    }
    /// Connect to typed source events; raw VM bytes never become assumed authority.
    pub fn attach(connected: ConnectedUi) -> Self {
        let state = RwSignal::new(AuthoringState::default());
        let notice = RwSignal::new(String::new());
        let pending_gateway = RwSignal::new(None::<(u64, String)>);
        let dispatch = Callback::new(move |intent: AuthoringIntent| {
            let result = state.try_update(|s| s.submit(intent));
            let Some(result) = result else {
                return;
            };
            let request = match result {
                Ok(r) => r,
                Err(error) => {
                    notice.set(player_error(&error));
                    return;
                }
            };
            let operation = match request.wire {
                AuthoringWire::Eod {
                    incarnation,
                    plugin_id,
                    event_name,
                    text,
                } => GatewayOperation::Eod {
                    incarnation,
                    plugin_id,
                    event_name,
                    text: Some(text),
                    binary: None,
                },
                AuthoringWire::LotCommand { data } => GatewayOperation::LotCommand {
                    lot_incarnation: request.actor.incarnation,
                    data,
                },
            };
            let label = format!("Edit item {}", request.operation_id);
            connected.send(operation, &label, None);
            let stamp = connected.state.with_untracked(|s| {
                s.last_operation(&label)
                    .map(|o| o.stamp.operation_id.clone())
            });
            if let Some(id) = stamp {
                pending_gateway.set(Some((request.operation_id, id)));
                notice.set(String::new());
            } else {
                state.update(|s| {
                    let _ = s.receive(AuthoringReceipt::Rejected {
                        operation_id: request.operation_id,
                        actor: request.actor,
                        message: "This action is unavailable in the current world session.".into(),
                    });
                });
            }
        });
        let mut ui = Self::new(dispatch);
        ui.state = state;
        ui.notice = notice;
        let cursor = RwSignal::new(0usize);
        Effect::new(move |_| {
            let connected_state = connected.state.get();
            let actor = connected_state
                .session
                .as_ref()
                .filter(|s| s.state == SessionState::LotReady)
                .and_then(|s| {
                    Some(SourceActorLot {
                        avatar_id: s.avatar_id?,
                        lot_id: None,
                        location: s.lot_location?,
                        epoch: s.epoch,
                        incarnation: s.lot_incarnation?,
                    })
                });
            let Some(mut actor) = actor else {
                ui.current_actor.set(None);
                state.update(|s| {
                    s.presentation_generation = None;
                    s.diagonal_floor_tiles = None;
                    s.world_projection_valid = false;
                    if s.snapshot.take().is_some() {
                        s.abandon_pending();
                        s.status = OperationState::Unknown(
                            "The lot session ended before this action was confirmed.".into(),
                        );
                    }
                });
                cursor.set(connected_state.events.len());
                return;
            };
            state.with_untracked(|s| {
                if let Some(snapshot) = &s.snapshot
                    && snapshot.actor.avatar_id == actor.avatar_id
                    && snapshot.actor.location == actor.location
                    && snapshot.actor.epoch == actor.epoch
                    && snapshot.actor.incarnation == actor.incarnation
                {
                    actor.lot_id = snapshot.actor.lot_id;
                }
            });
            ui.current_actor.set(Some(actor.clone()));
            state.update(|s| {
                if s.snapshot.as_ref().is_some_and(|snap| snap.actor != actor) {
                    s.presentation_generation = None;
                    s.diagonal_floor_tiles = None;
                    s.world_projection_valid = false;
                    s.snapshot = None;
                    s.abandon_pending();
                    s.status = OperationState::Unknown(
                        "The lot session changed before this action was confirmed.".into(),
                    );
                }
            });
            let first = cursor.get_untracked().min(connected_state.events.len());
            for event in &connected_state.events[first..] {
                if event
                    .data
                    .get("lot_incarnation")
                    .and_then(serde_json::Value::as_u64)
                    != Some(actor.incarnation)
                {
                    continue;
                }
                if event.family == "eod" {
                    if let Ok(message) =
                        serde_json::from_value::<SourceEodMessage>(event.data.clone())
                    {
                        state.update(|s| {
                            if let Err(error) = s.observe_eod(actor.clone(), &message) {
                                notice.set(player_error(&error));
                            }
                        });
                    }
                } else if event.family == "set_outfit" {
                    let fields = (
                        event.data.get("uid").and_then(serde_json::Value::as_u64),
                        event.data.get("scope").and_then(serde_json::Value::as_i64),
                        event
                            .data
                            .get("asset_id")
                            .and_then(serde_json::Value::as_str)
                            .and_then(|s| s.parse::<u64>().ok()),
                    );
                    if let (Some(uid), Some(scope), Some(asset)) = fields
                        && let (Ok(uid), Ok(scope)) = (u32::try_from(uid), i16::try_from(scope))
                    {
                        state.update(|s| {
                            let _ = s.observe_set_outfit(&actor, uid, scope, asset);
                        });
                    }
                }
            }
            cursor.set(connected_state.events.len());
            if let Some((operation_id, id)) = pending_gateway.get_untracked() {
                if let Some(operation) = connected_state.ledger.operations.get(&id) {
                    let receipt = match &operation.status {
                        OperationStatus::Rejected(message) => Some(AuthoringReceipt::Rejected {
                            operation_id,
                            actor: actor.clone(),
                            message: message.clone(),
                        }),
                        OperationStatus::Unknown(message) => Some(AuthoringReceipt::Unknown {
                            operation_id,
                            actor: actor.clone(),
                            message: message.clone(),
                        }),
                        OperationStatus::Accepted(_) => Some(AuthoringReceipt::Unknown {
                            operation_id,
                            actor: actor.clone(),
                            message: "Waiting for the item's confirmed state from the world."
                                .into(),
                        }),
                        _ => None,
                    };
                    if let Some(receipt) = receipt {
                        state.update(|s| {
                            let _ = s.receive(receipt);
                        });
                    }
                }
                if state.with_untracked(|s| s.pending.is_none()) {
                    pending_gateway.set(None);
                }
            }
        });
        ui
    }
}
fn category_name(category: u8) -> &'static str {
    match category {
        0 => "Daywear",
        2 => "Swimwear",
        5 => "Sleepwear",
        8 => "Head",
        9 => "Back",
        10 => "Shoes",
        11 => "Tail",
        _ => "Clothing",
    }
}
#[component]
fn AuthoringStatus(ui: SourceAuthoringUi) -> impl IntoView {
    view! {<div class="connected-authoring-status" role="status">{move ||{
     if !ui.notice.get().is_empty(){return ui.notice.get();}
     ui.state.with(|s|match &s.status{OperationState::Idle=>String::new(),OperationState::Pending=>"Waiting for the world…".into(),OperationState::Unknown(message)|OperationState::Rejected(message)=>message.clone(),OperationState::Accepted{amount:Some(amount)}if *amount<0=>format!("Confirmed · Refund {}",-i64::from(*amount)),OperationState::Accepted{amount:Some(amount)}=>format!("Confirmed · Cost {}",amount),OperationState::Accepted{amount:None}=>"Confirmed by the world.".into()})
    }}</div><Show when=move ||ui.state.with(|s|s.pending.is_some())><button class="chrome" on:click=move |_|{ui.abandon_wait();ui.notice.set(String::new());}>"Stop waiting"</button><p>"This edit may already have reached the world. Check the current item before trying again."</p></Show>}
}
#[component]
pub fn ConnectedAuthoringPanel(kind: AuthoringPanelKind) -> impl IntoView {
    let context = use_context::<SourceAuthoringUi>();
    match context {
  Some(ui)=>view!{<section class="connected-authoring"><AuthoringStatus ui=ui/>{match kind{AuthoringPanelKind::Wardrobe=>view!{<WardrobeControls ui=ui/>}.into_any(),AuthoringPanelKind::Catalog=>view!{<CatalogControls ui=ui/>}.into_any(),AuthoringPanelKind::Object=>view!{<ObjectControls ui=ui/>}.into_any(),AuthoringPanelKind::Build=>view!{<BuildControls ui=ui/>}.into_any()}}</section>}.into_any(),
  None=>view!{<p class="connected-authoring-empty">{match kind{AuthoringPanelKind::Wardrobe=>"Use a dresser or clothing rack in the lot to open your clothes.",AuthoringPanelKind::Catalog=>"Open a lot with buying permission to choose catalog items.",AuthoringPanelKind::Object=>"Select an object in the lot to move it or return it to inventory.",AuthoringPanelKind::Build=>"Open a lot with building permission to choose floors and walls."}}</p>}.into_any(),
 }
}
#[component]
fn WardrobeControls(ui: SourceAuthoringUi) -> impl IntoView {
    let category = RwSignal::new(None::<u8>);
    let outfit = Memo::new(move |_| {
        ui.state.with(|s| {
            s.snapshot.as_ref().and_then(|s| {
                s.outfits
                    .iter()
                    .find(|o| Some(o.outfit_id) == ui.selected_outfit.get())
                    .cloned()
            })
        })
    });
    let plugin = move || {
        ui.state.with(|s| {
            s.snapshot
                .as_ref()
                .and_then(|s| s.eod.as_ref())
                .map(|e| e.plugin_id)
        })
    };
    let items = move || {
        ui.state.with(|s| {
            s.snapshot
                .as_ref()
                .map(|s| {
                    s.outfits
                        .iter()
                        .filter(|o| category.get().is_none_or(|c| o.category == c))
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
    };
    let selection_intent = move |action: u8| {
        outfit.get().map(|row| match action {
            0 => AuthoringIntent::Wear {
                outfit_id: row.outfit_id,
            },
            1 => AuthoringIntent::SetDefault {
                outfit_id: row.outfit_id,
            },
            2 => AuthoringIntent::DeleteOutfit {
                outfit_id: row.outfit_id,
            },
            3 => AuthoringIntent::RackTry {
                outfit_id: row.outfit_id,
            },
            _ => AuthoringIntent::RackBuy {
                outfit_id: row.outfit_id,
                wear_now: false,
            },
        })
    };
    view! {
     <Show when=move ||plugin().is_some() fallback=||view!{<p class="connected-authoring-empty">"Use a dresser or clothing rack in the lot to see its clothes."</p>}>
      <div class="connected-authoring-tabs"><button class="chrome" class:selected=move ||category.get().is_none() on:click=move |_|category.set(None)>"All"</button><For each={move ||ui.state.with(|s|{let mut categories=s.snapshot.as_ref().map(|s|s.outfits.iter().map(|o|o.category).collect::<Vec<_>>()).unwrap_or_default();categories.sort();categories.dedup();categories})} key=|c|*c children=move |c|view!{<button class="chrome" class:selected=move ||category.get()==Some(c) on:click=move |_|category.set(Some(c))>{category_name(c)}</button>}/></div>
      <Show when=move ||ui.state.with(|s|s.snapshot.as_ref().is_some_and(|s|s.outfits_loaded)) fallback=||view!{<p>"Loading clothes…"</p>}>
       <div class="connected-owned-grid"><For each=items key=|row|(row.outfit_id,row.asset_id,row.sale_price,row.label.clone()) children=move |row|{
        let id=row.outfit_id;let thumbnail=row.thumbnail.clone();let label=row.label.clone().unwrap_or_else(||category_name(row.category).into());
        view!{<button class="connected-owned-item" class:selected=move ||ui.selected_outfit.get()==Some(id) aria-pressed=move ||ui.selected_outfit.get()==Some(id) on:click=move |_|ui.selected_outfit.set(Some(id))><Show when=move ||thumbnail.is_some() fallback=||view!{<span class="connected-item-symbol" aria-hidden="true">"♧"</span>}><img src=row.thumbnail.clone().unwrap_or_default() alt=""/></Show><strong>{label}</strong><Show when=move ||plugin()!=Some(DRESSER_PLUGIN)><span>{format!("{}",row.sale_price)}</span></Show></button>}
       }/></div>
       <Show when=move ||items().is_empty()><p>"There are no clothes in this category."</p></Show>
      </Show>
      <Show when=move ||outfit.get().is_some()><div class="connected-authoring-actions">
       <Show when=move ||plugin()==Some(DRESSER_PLUGIN)><button class="chrome" disabled=move ||selection_intent(0).is_none_or(|i|!ui.can(&i)) on:click=move |_|if let Some(i)=selection_intent(0){ui.dispatch.run(i)}>"Wear"</button><button class="chrome" disabled=move ||selection_intent(1).is_none_or(|i|!ui.can(&i)) on:click=move |_|if let Some(i)=selection_intent(1){ui.dispatch.run(i)}>"Make default"</button><button class="chrome" disabled=move ||selection_intent(2).is_none_or(|i|!ui.can(&i)) on:click=move |_|if let Some(i)=selection_intent(2){ui.dispatch.run(i)}>"Delete outfit"</button></Show>
       <Show when=move ||plugin()==Some(RACK_CUSTOMER_PLUGIN)><button class="chrome" disabled=move ||selection_intent(3).is_none_or(|i|!ui.can(&i)) on:click=move |_|if let Some(i)=selection_intent(3){ui.dispatch.run(i)}>"Try on"</button><button class="chrome" disabled=move ||selection_intent(4).is_none_or(|i|!ui.can(&i)) on:click=move |_|if let Some(i)=selection_intent(4){ui.dispatch.run(i)}>"Buy"</button></Show>
      </div></Show>
      <Show when=move ||plugin()==Some(RACK_OWNER_PLUGIN)><RackOwnerControls ui=ui/></Show>
      <button class="chrome" disabled=move ||!ui.can(&AuthoringIntent::CloseEod) on:click=move |_|ui.dispatch.run(AuthoringIntent::CloseEod)>"Close clothes"</button>
     </Show>
    }
}
#[component]
fn RackOwnerControls(ui: SourceAuthoringUi) -> impl IntoView {
    let price = RwSignal::new(String::new());
    Effect::new(move |_| {
        let selected = ui.selected_outfit.get();
        let amount = ui.state.with(|s| {
            s.snapshot
                .as_ref()
                .and_then(|s| s.outfits.iter().find(|o| Some(o.outfit_id) == selected))
                .map(|o| o.sale_price.to_string())
        });
        price.set(amount.unwrap_or_default());
    });
    let update = move || {
        Some(AuthoringIntent::RackSetPrice {
            outfit_id: ui.selected_outfit.get()?,
            price: price.get().parse::<i32>().ok()?,
        })
    };
    let delete = move || {
        Some(AuthoringIntent::RackDelete {
            outfit_id: ui.selected_outfit.get()?,
        })
    };
    view! {<Show when=move ||ui.state.with(|s|s.snapshot.as_ref().and_then(|s|s.eod.as_ref()).is_some_and(|e|e.object_owner_id==Some(e.actor_id))) fallback=||view!{<p>"Rack ownership has not arrived from the world yet."</p>}><label>"Price"<input type="number" min="1" max="999999" prop:value=move ||price.get() on:input=move |e|price.set(event_target_value(&e))/></label><div class="connected-authoring-actions"><button class="chrome" disabled=move ||update().is_none_or(|i|!ui.can(&i)) on:click=move |_|if let Some(i)=update(){ui.dispatch.run(i)}>"Update price"</button><button class="chrome" disabled=move ||delete().is_none_or(|i|!ui.can(&i)) on:click=move |_|if let Some(i)=delete(){ui.dispatch.run(i)}>"Remove from rack"</button></div></Show>}
}
#[component]
fn CatalogControls(ui: SourceAuthoringUi) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let selected = move || {
        ui.state.with(|s| {
            s.snapshot.as_ref().and_then(|s| {
                s.catalog
                    .iter()
                    .find(|o| Some(o.guid) == ui.selected_catalog.get())
                    .cloned()
            })
        })
    };
    let buy = move || {
        Some(AuthoringIntent::Buy {
            guid: selected()?.guid,
            placement: ui.placement.get()?,
            desired_mode: 1,
            upgrade: 0,
        })
    };
    let inventory = move || {
        Some(AuthoringIntent::PlaceInventory {
            persist_id: ui.selected_inventory.get()?,
            placement: ui.placement.get()?,
            desired_mode: 1,
        })
    };
    view! {<label class="connected-authoring-search">"Find an item"<input type="search" prop:value=move ||query.get() on:input=move |e|query.set(event_target_value(&e))/></label>
     <div class="connected-owned-grid"><For each={move ||ui.state.with(|s|s.snapshot.as_ref().map(|s|s.catalog.iter().filter(|row|row.name.to_lowercase().contains(&query.get().to_lowercase())).cloned().collect::<Vec<_>>()).unwrap_or_default())} key=|row|(row.guid,row.price,row.name.clone(),row.disable_level) children=move |row|{let guid=row.guid;view!{<button class="connected-owned-item" class:selected=move ||ui.selected_catalog.get()==Some(guid) disabled={row.disable_level>2} on:click=move |_|{ui.cancel_build();ui.selected_catalog.set(Some(guid));ui.selected_inventory.set(None);}><span class="connected-item-symbol" aria-hidden="true">"□"</span><strong>{row.name}</strong><span>{row.price.to_string()}</span></button>}}/></div>
     <Show when=move ||ui.state.with(|s|s.snapshot.as_ref().is_none_or(|s|s.catalog.is_empty()))><p>"Catalog items have not been supplied by this lot yet."</p></Show>
     <Show when=move ||selected().is_some()><p>"Choose a spot in the lot for this item."</p><div class="connected-authoring-actions"><button class="chrome" disabled=move ||buy().is_none_or(|i|!ui.can(&i)) on:click=move |_|if let Some(i)=buy(){ui.dispatch.run(i)}>"Buy and place"</button><button class="chrome" disabled=move ||ui.placement.get().is_none() on:click=move |_|ui.placement.update(|p|if let Some(p)=p{p.direction=p.direction.rotate_left(2);})>"Rotate"</button></div></Show>
     <details><summary>"Owned inventory"</summary><div class="connected-owned-grid"><For each=move ||ui.state.with(|s|s.snapshot.as_ref().map(|s|s.inventory.clone()).unwrap_or_default()) key=|row|(row.persist_id,row.name.clone()) children=move |row|{let id=row.persist_id;view!{<button class="connected-owned-item" class:selected=move ||ui.selected_inventory.get()==Some(id) on:click=move |_|{ui.cancel_build();ui.selected_inventory.set(Some(id));ui.selected_catalog.set(None);}><span class="connected-item-symbol" aria-hidden="true">"□"</span><strong>{row.name}</strong></button>}}/></div><button class="chrome" disabled=move ||inventory().is_none_or(|i|!ui.can(&i)) on:click=move |_|if let Some(i)=inventory(){ui.dispatch.run(i)}>"Place owned item"</button></details>
    }
}
#[component]
fn ObjectControls(ui: SourceAuthoringUi) -> impl IntoView {
    let object = move || {
        ui.state.with(|s| {
            s.snapshot.as_ref().and_then(|s| {
                s.objects
                    .iter()
                    .find(|o| Some(o.entity.clone()) == ui.selected_entity.get())
                    .cloned()
            })
        })
    };
    let action = move |kind: u8| {
        let entity = object()?.entity;
        Some(match kind {
            0 => AuthoringIntent::Move {
                entity,
                placement: ui.placement.get()?,
            },
            1 => AuthoringIntent::SendToInventory { entity },
            _ => AuthoringIntent::Delete {
                entity,
                desired_mode: 2,
            },
        })
    };
    view! {<Show when=move ||object().is_some() fallback=||view!{<p>"Select an object in the lot to move it or return it to its owner's inventory."</p>}><h3>{move ||object().map(|o|o.name).unwrap_or_default()}</h3><div class="connected-authoring-actions"><button class="chrome" disabled=move ||action(0).is_none_or(|i|!ui.can(&i)) on:click=move |_|if let Some(i)=action(0){ui.dispatch.run(i)}>"Move here"</button><button class="chrome" disabled=move ||action(1).is_none_or(|i|!ui.can(&i)) on:click=move |_|if let Some(i)=action(1){ui.dispatch.run(i)}>"Return to inventory"</button><button class="chrome" disabled=move ||action(2).is_none_or(|i|!ui.can(&i)) on:click=move |_|if let Some(i)=action(2){ui.dispatch.run(i)}>"Sell or return"</button></div><Show when=move ||object().is_some_and(|o|o.movable.is_none())><p>"This object’s editing options have not arrived from the world yet."</p></Show><p>"Choose another spot in the lot to move this object. The world confirms any sale or refund."</p></Show>}
}
#[component]
fn BuildControls(ui: SourceAuthoringUi) -> impl IntoView {
    let commit = move || AuthoringIntent::Architecture {
        commands: ui.architecture_draft.get(),
    };
    view! {
      <Show when=move ||!ui.state.with(|s|s.world_projection_valid)><p>"Refresh the lot before choosing a building area."</p></Show>
      <div class="connected-owned-grid"><For each=move ||ui.state.with(|s|s.snapshot.as_ref().map(|s|s.build_resources.clone()).unwrap_or_default()) key=|row|(row.tool,row.pattern,row.style,row.name.clone()) children=move |row|{
        let selection=row.clone();let highlight=row.clone();let reason=tile_build_unavailable_reason(&row);let label=row.name.clone();
        view!{<button class="connected-owned-item" class:selected=move ||ui.selected_build.get().as_ref()==Some(&highlight) aria-pressed=move ||ui.selected_build.get().as_ref().is_some_and(|r|r.name==label) disabled=move ||reason.is_some()||ui.state.with(|s|!s.world_projection_valid||s.pending.is_some()) on:click=move |_|ui.select_build(selection.clone())><span class="connected-item-symbol" aria-hidden="true">"▧"</span><strong>{row.name}</strong><span>{row.price.map(|p|p.to_string()).unwrap_or_default()}</span><span>{reason.unwrap_or_default()}</span></button>}
      }/></div>
      <Show when=move ||ui.selected_build.get().is_some()><p>{move ||ui.selected_build.get().map(|r|{
        let draft=ui.build_draft.get();let instruction=if draft.command.is_some(){"Check the selected area, then apply your change."}else if draft.start.is_some(){"Choose the end tile."}else if matches!(r.tool,4|6){"Choose a tile in the room to fill."}else if matches!(r.tool,0..=2){"Choose the start vertex, then the end vertex. Each marked vertex is the tile's top-left corner."}else{"Choose the start tile, then the end tile."};format!("{} · {}",r.name,instruction)
      }).unwrap_or_default()}</p><p class="connected-build-preview" aria-live="polite">{move ||{
        let draft=ui.build_draft.get();match (draft.start,draft.end,draft.command){(Some(start),Some(end),Some(command))=>{if command.kind==5{format!("Floor area: {} × {} tiles · level {}",command.x2+1,command.y2+1,start.level)}else{format!("Selected: ({}, {}) to ({}, {}) · level {}",start.x,start.y,end.x,end.y,start.level)}},(Some(start),_,_)=>format!("Start: ({}, {}) · level {}",start.x,start.y,start.level),_=>String::new()}
      }}</p></Show>
      <Show when=move ||ui.state.with(|s|s.snapshot.as_ref().is_none_or(|s|s.build_resources.is_empty()))><p>"Floor and wall choices have not been supplied by this lot yet."</p></Show>
      <div class="connected-authoring-actions"><button class="chrome" disabled=move ||!ui.can(&commit()) on:click=move |_|ui.dispatch.run(commit())>"Apply change"</button><button class="chrome" on:click=move |_|ui.reset_build_draft()>"Clear draft"</button><button class="chrome" disabled=move ||ui.selected_build.get().is_none() on:click=move |_|ui.cancel_build()>"Stop building"</button></div>
    }
}
