//! The original server's accepted snapshot, with explicitly separate VM playback.
use crate::{
    avatar_content::ContentUi,
    components::Icon,
    connected_adapter::state::Panel,
    connected_authoring::{AuthoringPanelKind, ConnectedAuthoringPanel, SourceAuthoringUi},
    connected_bridge::ConnectedUi,
    live_world_adapter::{FrameError, LiveWorldIdentity, SnapshotPickAction, SourceFrameGate},
    snapshot_avatar::SnapshotAvatarProjection,
    snapshot_world::snapshot_world,
    source_needs::{SOURCE_NEED_LABELS, source_needs},
    vm_inbox::{InboxError, VmStream},
    world_renderer::WorldViewport,
};
use leptos::prelude::*;
use std::{collections::BTreeMap, sync::Arc};
use wonderland_avatar_content::ImportedContent;
use wonderland_game_services::{GatewayOperation, SessionState};
use wonderland_player_authoring::{EntityIdentity, Placement};
use wonderland_render_core::{AssetKey, RenderLimits, RgbaImage};
use wonderland_vm_protocol::{
    Snapshot,
    snapshot::{Appearance, EntityPlatform},
};
use wonderland_world_view::{
    ViewportControls, WallMode, WorldDiagnostic, WorldDocument, WorldPick, WorldPickTarget,
};

async fn decode_texture(
    content: &ImportedContent,
    key: AssetKey,
    remaining: usize,
) -> Result<RgbaImage, String> {
    use wasm_bindgen::JsValue;
    let source = content
        .textures
        .get(&key)
        .ok_or("Original avatar texture is absent.")?;
    let limits = RenderLimits::default();
    let promise = crate::avatar_renderer::decode_avatar_texture(
        &js_sys::Uint8Array::from(source.bytes.as_slice()),
        source.mime,
        remaining.min(limits.max_texture_pixels) as u32,
        limits.max_image_dimension,
    );
    let value = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|error| {
            js_sys::Reflect::get(&error, &JsValue::from_str("message"))
                .ok()
                .and_then(|value| value.as_string())
                .or_else(|| error.as_string())
                .unwrap_or_else(|| "Original avatar texture could not be decoded.".into())
        })?;
    let field = |name: &str| {
        js_sys::Reflect::get(&value, &JsValue::from_str(name))
            .map_err(|_| "Invalid original texture decoder result.".to_string())
    };
    let dimension = |name: &str| -> Result<u32, String> {
        let number = field(name)?
            .as_f64()
            .ok_or("Invalid original texture dimensions.")?;
        if !number.is_finite()
            || number <= 0.
            || number.fract() != 0.
            || number > f64::from(limits.max_image_dimension)
        {
            return Err("Invalid original texture dimensions.".into());
        }
        Ok(number as u32)
    };
    let (width, height) = (dimension("width")?, dimension("height")?);
    let bytes = js_sys::Uint8Array::new(&field("pixels")?);
    let count = RgbaImage::checked_pixel_count(width, height, &limits)
        .map_err(|issue| issue.to_string())?;
    if count > remaining || bytes.length() as usize != count * 4 {
        return Err("Original texture pixels do not match the bounded dimensions.".into());
    }
    let image = RgbaImage {
        width,
        height,
        pixels: bytes.to_vec().as_chunks::<4>().0.to_vec(),
    };
    content
        .validate_decoded_texture(key, &image)
        .map_err(|issue| issue.to_string())?;
    Ok(image)
}

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
    let content = expect_context::<ContentUi>();
    let authoring = expect_context::<SourceAuthoringUi>();
    let gate = StoredValue::new(SourceFrameGate::default());
    let world = RwSignal::new(None::<Arc<WorldDocument>>);
    let accepted_world = RwSignal::new(None::<Arc<WorldDocument>>);
    let snapshot = RwSignal::new(None::<Arc<Snapshot>>);
    let resource_generation = StoredValue::new(0u64);
    let texture_cache = StoredValue::new(BTreeMap::<AssetKey, RgbaImage>::new());
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
            accepted_world.set(None);
            snapshot.set(None);
            selected.set(None);
            stale.set(false);
            controls.set(ViewportControls::default());
            notice.set("Waiting for the property’s world state…".into());
        }
        ui.vm_pending.get();
        let Some(current) = current else {
            return;
        };
        let frames = match ui.drain_vm(VmStream {
            browser_epoch: current.browser_epoch,
            source_epoch: current.source_epoch,
            lot_incarnation: current.lot_incarnation,
        }) {
            Ok(frames) => frames,
            Err(InboxError::WrongStream) => return,
            Err(_) => {
                authoring.invalidate_world_projection();
                stale.set(true);
                ui.require_vm_recovery(
                    "World update buffering failed. Reconnect for a complete world state.",
                );
                return;
            }
        };
        for frame in frames {
            if frame.browser_epoch != current.browser_epoch
                || frame.source_epoch != current.source_epoch
                || frame.lot_incarnation != Some(current.lot_incarnation)
            {
                ui.require_vm_recovery(
                    "The world update no longer matches this session. Reconnect to continue.",
                );
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
                            notice.set("This snapshot contains an unsupported world clock. Reconnect to continue.".into());
                            ui.require_vm_recovery("This snapshot contains an unsupported world clock. Reconnect to continue.");
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
                                accepted_world.set(Some(Arc::new(document)));
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
                                    let motives = source_needs(&avatar.motives);
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
                                            entry.motives = motives;
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
                        "The world update belongs to another property. Reconnect to continue."
                            .into(),
                    );
                    ui.require_vm_recovery(
                        "The world update belongs to another property. Reconnect to continue.",
                    );
                    return;
                }
                Err(FrameError::InvalidProtocol) => {
                    stale.set(true);
                    authoring.invalidate_world_projection();
                    notice
                        .set("This world update could not be read. Reconnect to continue.".into());
                    ui.require_vm_recovery(
                        "This world update could not be read. Reconnect to continue.",
                    );
                    return;
                }
            }
        }
    });
    // Resource work has its own cancellation fence. It never re-admits a VM
    // frame or changes source object/architecture generations when files load.
    Effect::new(move |_| {
        let source = snapshot.get();
        let accepted = accepted_world.get();
        let bank = content.imported.get();
        let owner = identity.get();
        resource_generation.update_value(|value| *value = value.wrapping_add(1));
        let generation = resource_generation.get_value();
        selected.set(None);
        let (Some(source), Some(accepted)) = (source, accepted) else {
            world.set(None);
            return;
        };
        world.set(Some(Arc::clone(&accepted)));
        let Some(bank) = bank else {
            texture_cache.update_value(BTreeMap::clear);
            return;
        };
        let projection = match SnapshotAvatarProjection::prepare(&source, &accepted, &bank) {
            Ok(projection) => projection,
            Err(issue) => {
                let mut document = (*accepted).clone();
                document.diagnostics.push(WorldDiagnostic {
                    code: "unsupported_avatar_projection".into(),
                    resource: "avatars".into(),
                    message: issue.to_string(),
                });
                world.set(Some(Arc::new(document)));
                return;
            }
        };
        texture_cache
            .update_value(|cache| cache.retain(|key, _| projection.texture_keys().contains(key)));
        let mut decoded = texture_cache.get_value();
        wasm_bindgen_futures::spawn_local(async move {
            let mut issues = vec![];
            let mut pixels = decoded
                .values()
                .map(|image| image.pixels.len())
                .sum::<usize>();
            for &key in projection.texture_keys() {
                if resource_generation.try_with_value(|value| *value) != Some(generation) {
                    return;
                }
                if decoded.contains_key(&key) {
                    continue;
                }
                match decode_texture(
                    &bank,
                    key,
                    RenderLimits::default()
                        .max_texture_pixels
                        .saturating_sub(pixels),
                )
                .await
                {
                    Ok(image) => {
                        pixels += image.pixels.len();
                        decoded.insert(key, image);
                    }
                    Err(message) => issues.push(WorldDiagnostic {
                        code: "missing_avatar_texture".into(),
                        resource: format!("texture:{key:?}"),
                        message,
                    }),
                }
            }
            if resource_generation.try_with_value(|value| *value) != Some(generation)
                || identity.try_get_untracked() != Some(owner)
            {
                return;
            }
            texture_cache.set_value(decoded.clone());
            let mut document = (*accepted).clone();
            match projection.apply(&mut document, &bank, &decoded) {
                Ok(()) => {
                    document.diagnostics.extend(issues);
                    world.set(Some(Arc::new(document)));
                }
                Err(issue) => {
                    let mut document = (*accepted).clone();
                    document.diagnostics.push(WorldDiagnostic {
                        code: "unsupported_avatar_projection".into(),
                        resource: "avatars".into(),
                        message: issue.to_string(),
                    });
                    world.set(Some(Arc::new(document)));
                }
            }
        });
    });
    on_cleanup(move || {
        resource_generation.try_update_value(|value| *value = value.wrapping_add(1));
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
        let Some(document) = world.get_untracked() else {
            return;
        };
        let Some(action) = gate.with_value(|gate| {
            gate.pick_action(
                identity.get_untracked(),
                &document,
                &pick,
                stale.get_untracked(),
            )
        }) else {
            return;
        };
        match action {
            SnapshotPickAction::InspectAvatar(avatar_id) => {
                if let Some(avatar_id) = avatar_id {
                    ui.select_person(avatar_id);
                }
                selected.set(Some(pick));
                return;
            }
            SnapshotPickAction::RefreshRequired => {
                notice.set(
                    "The property has changed. Refresh its state before selecting an item to edit."
                        .into(),
                );
                return;
            }
            SnapshotPickAction::Authoring => {}
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
                <div class="source-control-group source-camera-controls" role="group" aria-label="Camera"><button class="chrome round small" aria-label="Rotate left" on:click=move |_|controls.update(|view|view.yaw_radians-=std::f32::consts::FRAC_PI_4)><Icon name="rotate-clockwise" class="icon-mirror"/></button><button class="chrome round small" aria-label="Rotate right" on:click=move |_|controls.update(|view|view.yaw_radians+=std::f32::consts::FRAC_PI_4)><Icon name="rotate-clockwise"/></button><button class="chrome round small" aria-label="Zoom out" on:click=move |_|controls.update(|view|view.zoom=(view.zoom/1.2).max(0.25))><Icon name="minus"/></button><button class="chrome round small" aria-label="Zoom in" on:click=move |_|controls.update(|view|view.zoom=(view.zoom*1.2).min(8.))><Icon name="plus"/></button></div>
                <div class="source-control-group source-floor-controls" role="group" aria-label="Visible floor"><button class="chrome round small" aria-label="Floor down" disabled={move ||controls.get().visible_level<=1} on:click=move |_|controls.update(|view|view.visible_level=view.visible_level.saturating_sub(1).max(1))><Icon name="chevron-down"/></button><span>{move ||controls.get().visible_level}</span><button class="chrome round small" aria-label="Floor up" disabled=move ||world.with(|world|world.as_ref().is_none_or(|world|controls.get().visible_level>=world.lot.levels)) on:click=move |_|{let levels=world.with_untracked(|world|world.as_ref().map(|world|world.lot.levels).unwrap_or(1));controls.update(|view|view.visible_level=(view.visible_level+1).min(levels));}><Icon name="chevron-up"/></button></div>
                <div class="source-control-group source-visibility-controls" role="group" aria-label="Wall visibility">{[(WallMode::Down,"Walls down"),(WallMode::Cutaway,"Cutaway"),(WallMode::Up,"Walls up")].into_iter().map(move |(mode,label)|view!{<button class="chrome" aria-pressed=move ||(controls.get().walls==mode).to_string() on:click=move |_|controls.update(|view|view.walls=mode)>{label}</button>}).collect_view()}<button class="chrome" aria-pressed=move ||controls.get().show_roofs.to_string() on:click=move |_|controls.update(|view|view.show_roofs = !view.show_roofs)>"Roof"</button></div>
                <div class="source-control-group source-activity-controls" role="group" aria-label="Property actions"><button class="chrome" on:click=move |_|authoring_panel.set(Some(AuthoringPanelKind::Catalog))><Icon name="shopping-cart"/>"Buy"</button><button class="chrome" on:click=move |_|authoring_panel.set(Some(AuthoringPanelKind::Build))><Icon name="hammer"/>"Build"</button><button class="chrome" on:click=move |_|needs_open.update(|open|*open = !*open)>"Needs"</button></div>
            </nav>
            <Show when=move ||authoring_panel.get().is_some()><aside class="connected-world-authoring chrome" aria-label="Edit this property"><header><h2>{move ||match authoring_panel.get(){Some(AuthoringPanelKind::Catalog)=>"Buy objects",Some(AuthoringPanelKind::Build)=>"Build",Some(AuthoringPanelKind::Object)=>"Object actions",_=>"Wardrobe"}}</h2><button class="chrome round small" aria-label="Close property editor" on:click=move |_|authoring_panel.set(None)><Icon name="x"/></button></header>{move ||authoring_panel.get().map(|kind|view!{<ConnectedAuthoringPanel kind/>})}</aside></Show>
            <Show when=move ||needs_open.get()><aside class="connected-world-needs chrome" aria-label="Your Sim’s needs"><header><h2>"Your Sim"</h2><button class="chrome round small" aria-label="Close needs" on:click=move |_|needs_open.set(false)><Icon name="x"/></button></header><p>{move ||ui.state.with(|state|state.active_entry().map(|entry|entry.name.clone()).unwrap_or_default())}</p><p>{move ||ui.state.with(|state|state.active_entry().and_then(|entry|entry.money).map(|money|format!("§ {money}")).unwrap_or_else(||"Waiting for balance".into()))}</p><div class="connected-source-needs">{SOURCE_NEED_LABELS.into_iter().enumerate().map(move |(index,label)|view!{<label><span>{label}</span>{move ||ui.state.with(|state|state.active_entry().and_then(|entry|entry.motives).map(|needs|view!{<progress max="100" value=((i32::from(needs[index])+100)/2).clamp(0,100)>{format!("{}",needs[index])}</progress>}.into_any()).unwrap_or_else(||view!{<span>"Unavailable"</span>}.into_any()))}</label>}).collect_view()}</div><button class="chrome" on:click=move |_|ui.panel(Panel::Profile)>"Open profile"</button></aside></Show>
        </section>
    }
}
