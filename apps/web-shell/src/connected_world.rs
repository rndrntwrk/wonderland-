//! The original server's accepted snapshot, with explicitly separate VM playback.
use crate::{
    components::Icon,
    connected_adapter::state::Panel,
    connected_authoring::{AuthoringPanelKind, ConnectedAuthoringPanel, SourceAuthoringUi},
    connected_bridge::ConnectedUi,
    live_world_adapter::{FrameError, LiveWorldIdentity, SourceFrameGate},
    snapshot_world::snapshot_world,
    world_renderer::WorldViewport,
};
use leptos::prelude::*;
use std::sync::Arc;
use wonderland_game_services::{GatewayOperation, SessionState};
use wonderland_player_authoring::{EntityIdentity, Placement};
use wonderland_vm_protocol::{
    Snapshot,
    snapshot::{Appearance, EntityPlatform},
};
use wonderland_world_view::{
    ViewportControls, WallMode, WorldDocument, WorldPick, WorldPickTarget,
};

#[derive(Clone)]
struct QueueCard {
    uid: u16,
    name: String,
    cancellable: bool,
    cancelling: bool,
}

fn own_queue(snapshot: &Snapshot, avatar: u32) -> Option<Vec<QueueCard>> {
    let index = snapshot.entities.iter().position(|entity| {
        entity.persist_id == avatar && matches!(entity.appearance, Appearance::Avatar(_))
    })?;
    let thread = snapshot.threads.get(index)?;
    let mut parent_idle = false;
    Some(
        thread
            .queue
            .iter()
            .enumerate()
            .filter_map(|(index, action)| {
                // UIInteractionQueue.cs:112–150 and VMQueueMode's original numeric values.
                let visible = action.mode != 3
                    && (index == 0 || action.mode != 2)
                    && (!parent_idle || action.mode != 1);
                if action.mode == 1 {
                    parent_idle = true;
                }
                visible.then(|| QueueCard {
                    uid: action.uid,
                    name: action.name.clone().unwrap_or_else(|| "Action".into()),
                    cancellable: action.mode != 2,
                    cancelling: action.notify_idle,
                })
            })
            .collect(),
    )
}

#[component]
pub fn ConnectedLotView() -> impl IntoView {
    let ui = expect_context::<ConnectedUi>();
    let authoring = expect_context::<SourceAuthoringUi>();
    let gate = StoredValue::new(SourceFrameGate::default());
    let world = RwSignal::new(None::<Arc<WorldDocument>>);
    let snapshot = RwSignal::new(None::<Arc<Snapshot>>);
    let controls = RwSignal::new(ViewportControls::default());
    let selected = RwSignal::new(None::<WorldPick>);
    let stale = RwSignal::new(false);
    let notice = RwSignal::new("Waiting for the property’s world state…".to_string());
    let authoring_panel = RwSignal::new(None::<AuthoringPanelKind>);
    let needs_open = RwSignal::new(false);
    let identity = Memo::new(move |_| {
        ui.state.with(|state| {
            let session = state.session.as_ref()?;
            if !state.ledger.authenticated
                || !state.ledger.transport_ready
                || session.state != SessionState::LotReady
            {
                return None;
            }
            Some(LiveWorldIdentity {
                browser_epoch: state.ledger.epoch,
                source_epoch: session.epoch,
                lot_incarnation: session.lot_incarnation?,
                lot_location: session.lot_location?,
                avatar_id: session.avatar_id?,
            })
        })
    });
    Effect::new(move |_| {
        let current = identity.get();
        let changed = gate.with_value(|gate| gate.identity() != current);
        if changed {
            gate.update_value(|gate| gate.reset(current));
            authoring.invalidate_world_projection();
            world.set(None);
            snapshot.set(None);
            selected.set(None);
            stale.set(false);
            controls.set(ViewportControls::default());
            notice.set("Waiting for the property’s world state…".into());
        }
        let frame = ui.latest_vm.get();
        let (Some(current), Some(frame)) = (current, frame) else {
            return;
        };
        if frame.browser_epoch != current.browser_epoch
            || frame.source_epoch != current.source_epoch
            || frame.lot_incarnation != Some(current.lot_incarnation)
        {
            return;
        }
        let Some(result) =
            gate.try_update_value(|gate| gate.admit(current, frame.direct, &frame.data))
        else {
            return;
        };
        match result {
            Ok(mut update) => {
                let mut displayed_snapshot = false;
                if let Some(source) = update.snapshots.pop() {
                    // This presentation field retains the source clock. It is
                    // not the VMNetTickList sequence or an A-runtime revision.
                    let Ok(clock_tick) = u64::try_from(source.context.clock.ticks) else {
                        stale.set(true);
                        authoring.invalidate_world_projection();
                        notice.set("This snapshot contains an unsupported world clock. Request a new snapshot to continue.".into());
                        return;
                    };
                    match snapshot_world(
                        &source,
                        current.source_epoch,
                        clock_tick,
                        update.generation,
                    ) {
                        Ok(document) => {
                            if authoring
                                .observe_snapshot(&source, update.generation)
                                .is_err()
                            {
                                authoring.invalidate_world_projection();
                            }
                            let levels = document.lot.levels;
                            controls.update(|controls| {
                                controls.visible_level = controls.visible_level.clamp(1, levels)
                            });
                            world.set(Some(Arc::new(document)));
                            selected.set(None);
                            if let Some(entity) = source
                                .entities
                                .iter()
                                .find(|entity| entity.persist_id == current.avatar_id)
                                && let (
                                    EntityPlatform::Avatar { budget, .. },
                                    Appearance::Avatar(avatar),
                                ) = (&entity.platform, &entity.appearance)
                            {
                                let motives = [5, 6, 7, 8, 9, 13, 14, 15]
                                    .map(|index| avatar.motives.get(index).copied());
                                ui.state.update(|state| {
                                    if state.ledger.epoch != current.browser_epoch
                                        || state.session.as_ref().is_none_or(|session| {
                                            session.epoch != current.source_epoch
                                                || session.lot_incarnation
                                                    != Some(current.lot_incarnation)
                                                || session.avatar_id != Some(current.avatar_id)
                                        })
                                    {
                                        return;
                                    }
                                    if let Some(entry) = state
                                        .roster
                                        .iter_mut()
                                        .find(|entry| entry.avatar_id == current.avatar_id)
                                    {
                                        entry.money = Some(i64::from(*budget));
                                        entry.motives = motives
                                            .into_iter()
                                            .collect::<Option<Vec<_>>>()
                                            .and_then(|values| values.try_into().ok());
                                    }
                                });
                            }
                            snapshot.set(Some(Arc::new(source)));
                            notice.set(String::new());
                            displayed_snapshot = true;
                        }
                        Err(_) => {
                            stale.set(true);
                            authoring.invalidate_world_projection();
                            notice.set("This world snapshot needs source content or geometry the browser cannot display yet.".into());
                        }
                    }
                }
                if update.needs_refresh {
                    stale.set(true);
                    authoring.invalidate_world_projection();
                } else if displayed_snapshot {
                    stale.set(false);
                }
            }
            Err(FrameError::WrongSession | FrameError::StaleTick) => {}
            Err(FrameError::WrongLot) => {
                stale.set(true);
                authoring.invalidate_world_projection();
                notice.set(
                    "The world update belongs to another property. Reconnect to continue.".into(),
                );
            }
            Err(FrameError::InvalidProtocol) => {
                stale.set(true);
                authoring.invalidate_world_projection();
                notice.set(
                    "This world update could not be read. Request a new snapshot or reconnect."
                        .into(),
                );
            }
        }
    });
    on_cleanup(move || {
        if let Some(owner) = gate.try_with_value(|gate| gate.identity()).flatten() {
            ui.state.try_update(|state| {
                // A new incarnation may already have supplied its own values.
                if state.session.as_ref().is_some_and(|session| {
                    session.state == SessionState::LotReady
                        && session.lot_incarnation != Some(owner.lot_incarnation)
                }) {
                    return;
                }
                if let Some(entry) = state
                    .roster
                    .iter_mut()
                    .find(|entry| entry.avatar_id == owner.avatar_id)
                {
                    entry.money = None;
                    entry.motives = None;
                }
            });
        }
    });
    let on_pick = Callback::new(move |pick: WorldPick| {
        if stale.get_untracked() {
            notice.set(
                "The property has changed. Refresh its state before selecting an item to edit."
                    .into(),
            );
            return;
        }
        let Some(document) = world.get_untracked() else {
            return;
        };
        if document.revision != pick.revision {
            return;
        }
        match &pick.target {
            WorldPickTarget::Tile { x, y, level, .. } => {
                if authoring.pick_build_tile(*x, *y, *level) {
                    selected.set(Some(pick));
                    return;
                }
                if let (Ok(x), Ok(y), Ok(level)) = (
                    i16::try_from(i32::from(*x) * 16 + 8),
                    i16::try_from(i32::from(*y) * 16 + 8),
                    i8::try_from(*level),
                ) {
                    let direction = authoring
                        .placement
                        .get_untracked()
                        .map(|p| p.direction)
                        .unwrap_or(1);
                    authoring.placement.set(Some(Placement {
                        x,
                        y,
                        level,
                        direction,
                    }));
                }
            }
            WorldPickTarget::Object {
                source_record: Some(record),
                ..
            } => {
                if let Some(source) = document.objects.iter().find_map(|object| {
                    object
                        .snapshot
                        .as_ref()
                        .filter(|source| source.record == *record)
                }) {
                    authoring.selected_entity.set(Some(EntityIdentity {
                        object_id: source.object_id,
                        incarnation: source.presentation_generation,
                    }));
                    authoring_panel.set(Some(AuthoringPanelKind::Object));
                }
            }
            _ => {}
        }
        selected.set(Some(pick));
    });
    let refresh = move |_| {
        if let Some(identity) = identity.get_untracked() {
            ui.send(
                GatewayOperation::RequestWorldSnapshot {
                    lot_incarnation: identity.lot_incarnation,
                },
                "Refresh world",
                None,
            );
        }
    };
    view! {
        <section class="source-world-screen connected-world" aria-label="Connected property">
            {move ||world.get().map(|document| {
                let outline_world=Arc::clone(&document);
                view!{<WorldViewport world=Signal::derive(move ||Arc::clone(&document)) controls on_pick draft=Signal::derive(move ||crate::world_draft::draft_outline(&outline_world,&authoring.build_draft.get()))/>}
            })}
            <header class="source-world-header chrome"><button class="chrome round small" aria-label="Return to city" on:click=move |_|ui.send(GatewayOperation::LeaveLot,"Leave property",None)><Icon name="chevron-left"/></button><div><span class="eyebrow">"CONNECTED PROPERTY"</span><h1>{move ||snapshot.get().map(|source|source.platform.name.clone()).filter(|name|!name.is_empty()).unwrap_or_else(||"Entering property".into())}</h1><p>{move ||if stale.get(){"Last server snapshot · refresh to update"}else{"Server snapshot view"}}</p></div><button class="chrome" disabled=move ||identity.get().is_none()||ui.state.with(|state|state.busy("Refresh world")) on:click=refresh><Icon name="refresh"/>"Refresh"</button></header>
            <Show when=move ||!notice.get().is_empty()><p class="source-world-notice chrome" role="status">{move ||notice.get()}</p></Show>
            <div class="connected-world-queue" aria-label="Your action queue">
                <For each={move ||snapshot.get().and_then(|source|identity.get().and_then(|identity|own_queue(&source,identity.avatar_id))).unwrap_or_default()} key=|action|(action.uid,action.name.clone(),action.cancellable,action.cancelling) children=move |action| {
                    let uid=action.uid;
                    view!{<div class="connected-world-action chrome"><Icon name="player-play"/><span>{action.name}</span><Show when=move ||action.cancelling><small>"Cancellation requested"</small></Show><button class="chrome round small" aria-label="Cancel this action" disabled=move ||!action.cancellable||action.cancelling||stale.get()||identity.get().is_none() on:click=move |_|{if stale.get_untracked(){return;}
                        if let Some(identity)=identity.get_untracked(){ui.send(GatewayOperation::CancelInteraction {lot_incarnation:identity.lot_incarnation,action_uid:uid},"Cancel action",None);}}><Icon name="x"/></button></div>}
                }/>
            </div>
            <nav class="source-world-tools chrome" aria-label="Property controls">
                <div class="source-control-group"><button class="chrome round small" aria-label="Rotate left" on:click=move |_|controls.update(|view|view.yaw_radians-=std::f32::consts::FRAC_PI_4)><Icon name="rotate-clockwise" class="icon-mirror"/></button><button class="chrome round small" aria-label="Rotate right" on:click=move |_|controls.update(|view|view.yaw_radians+=std::f32::consts::FRAC_PI_4)><Icon name="rotate-clockwise"/></button><button class="chrome round small" aria-label="Zoom out" on:click=move |_|controls.update(|view|view.zoom=(view.zoom/1.2).max(0.25))><Icon name="minus"/></button><button class="chrome round small" aria-label="Zoom in" on:click=move |_|controls.update(|view|view.zoom=(view.zoom*1.2).min(8.))><Icon name="plus"/></button></div>
                <div class="source-control-group"><button class="chrome" disabled={move ||controls.get().visible_level<=1} on:click=move |_|controls.update(|view|view.visible_level=view.visible_level.saturating_sub(1).max(1))>"− Floor"</button><span>{move ||controls.get().visible_level}</span><button class="chrome" disabled=move ||world.with(|world|world.as_ref().is_none_or(|world|controls.get().visible_level>=world.lot.levels)) on:click=move |_|{let levels=world.with_untracked(|world|world.as_ref().map(|world|world.lot.levels).unwrap_or(1));controls.update(|view|view.visible_level=(view.visible_level+1).min(levels));}>"+ Floor"</button></div>
                <div class="source-control-group">{[(WallMode::Down,"Walls down"),(WallMode::Cutaway,"Cutaway"),(WallMode::Up,"Walls up")].into_iter().map(move |(mode,label)|view!{<button class="chrome" aria-pressed=move ||(controls.get().walls==mode).to_string() on:click=move |_|controls.update(|view|view.walls=mode)>{label}</button>}).collect_view()}<button class="chrome" aria-pressed=move ||controls.get().show_roofs.to_string() on:click=move |_|controls.update(|view|view.show_roofs = !view.show_roofs)>"Roof"</button></div>
                <div class="source-control-group"><button class="chrome" on:click=move |_|authoring_panel.set(Some(AuthoringPanelKind::Catalog))><Icon name="shopping-cart"/>"Buy"</button><button class="chrome" on:click=move |_|authoring_panel.set(Some(AuthoringPanelKind::Build))><Icon name="hammer"/>"Build"</button><button class="chrome" on:click=move |_|needs_open.update(|open|*open = !*open)>"Needs"</button></div>
            </nav>
            <Show when=move ||authoring_panel.get().is_some()><aside class="connected-world-authoring chrome" aria-label="Edit this property"><header><h2>{move ||match authoring_panel.get(){Some(AuthoringPanelKind::Catalog)=>"Buy objects",Some(AuthoringPanelKind::Build)=>"Build",Some(AuthoringPanelKind::Object)=>"Object actions",_=>"Wardrobe"}}</h2><button class="chrome round small" aria-label="Close property editor" on:click=move |_|authoring_panel.set(None)><Icon name="x"/></button></header>{move ||authoring_panel.get().map(|kind|view!{<ConnectedAuthoringPanel kind/>})}</aside></Show>
            <Show when=move ||needs_open.get()><aside class="connected-world-needs chrome" aria-label="Your Sim’s needs"><header><h2>"Your Sim"</h2><button class="chrome round small" aria-label="Close needs" on:click=move |_|needs_open.set(false)><Icon name="x"/></button></header><p>{move ||ui.state.with(|state|state.active_entry().map(|entry|entry.name.clone()).unwrap_or_default())}</p><p>{move ||ui.state.with(|state|state.active_entry().and_then(|entry|entry.money).map(|money|format!("§ {money}")).unwrap_or_else(||"Waiting for balance".into()))}</p><div class="connected-source-needs">{["Energy","Comfort","Hunger","Hygiene","Bladder","Room","Social","Fun"].into_iter().enumerate().map(move |(index,label)|view!{<label><span>{label}</span>{move ||ui.state.with(|state|state.active_entry().and_then(|entry|entry.motives).map(|needs|view!{<progress max="100" value=((i32::from(needs[index])+100)/2).clamp(0,100)>{format!("{}",needs[index])}</progress>}.into_any()).unwrap_or_else(||view!{<span>"Unavailable"</span>}.into_any()))}</label>}).collect_view()}</div><button class="chrome" on:click=move |_|ui.panel(Panel::Profile)>"Open profile"</button></aside></Show>
        </section>
    }
}
