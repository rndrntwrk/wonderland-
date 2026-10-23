//! Private authentic NativePlayer/WebSocket probe. No host mutation commands.
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use wonderland_game_runtime::{
    EntityRef, ObjectId, RuntimeEvent, entity_key,
    live_wire::{
        encode_checkpoint_request,
        player::{ActionStatus, NativePlayer, PlayerBinding, PlayerUpdate},
    },
    sim_core::interactions::{InteractionScope, QueueEventKind},
};
const TIMER: u32 = 0x39D04C9C;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn bytes(value: &Value) -> Result<Vec<u8>> {
    let text = value.as_str().ok_or("hex missing")?;
    if text.len() % 2 != 0 || text.len() > 128 * 1024 * 1024 {
        return Err("hex bound".into());
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| Ok(u8::from_str_radix(std::str::from_utf8(p)?, 16)?))
        .collect()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn dec(value: &Value) -> Result<u64> {
    Ok(if let Some(text) = value.as_str() {
        text.parse()?
    } else {
        value.as_u64().ok_or("decimal missing")?
    })
}
fn main() -> Result<()> {
    let mut player: Option<NativePlayer> = None;
    let mut prepared = false;
    let mut selected: Option<u64> = None;
    let mut effects = 0usize;
    let mut faults = Vec::new();
    for line in std::io::stdin().lock().lines() {
        let v: Value = serde_json::from_str(&line?)?;
        let result = (|| -> Result<Value> {
            if v["kind"] == "open" {
                if player.is_some() {
                    return Err("client already open".into());
                }
                let b = &v["binding"];
                let p = NativePlayer::open(
                    &bytes(&v["hex"])?,
                    PlayerBinding {
                        source_epoch: dec(&b["source_epoch"])?,
                        lot_incarnation: dec(&b["lot_incarnation"])?,
                        lot_location: u32::try_from(dec(&b["lot_location"])?)?,
                        avatar_id: u32::try_from(dec(&b["avatar_id"])?)?,
                    },
                    1,
                )?;
                let outbound = encode_checkpoint_request(
                    p.checkpoint_request().ok_or("checkpoint request absent")?,
                )?;
                player = Some(p);
                return Ok(json!({"outbound":hex(&outbound)}));
            }
            let p = player.as_mut().ok_or("client not open")?;
            if v["kind"] == "receive" {
                if let PlayerUpdate::Ticks(outcomes) = p.receive_update(&bytes(&v["hex"])?)? {
                    for outcome in outcomes {
                        effects += outcome.effects.len();
                        for event in outcome.events {
                            match event {
                                RuntimeEvent::ThreadFault { entity, fault } => {
                                    faults.push(format!("{entity:?}: {fault:?}"))
                                }
                                RuntimeEvent::Interaction(event)
                                    if prepared
                                        && selected.is_none()
                                        && event.actor == entity_key(p.actor())
                                        && matches!(
                                            event.kind,
                                            QueueEventKind::Enqueued { .. }
                                        ) =>
                                {
                                    selected = event.action.map(|a| a.0)
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            if v["kind"] == "invoke" {
                if prepared {
                    return Err("one actual source interaction per client probe".into());
                }
                let target = EntityRef {
                    object_id: ObjectId(i16::try_from(dec(&v["object_id"])?)?),
                    generation: u32::try_from(dec(&v["generation"])?)?,
                };
                let projection = p.projection()?;
                if !projection
                    .entities
                    .iter()
                    .any(|entity| entity.reference == target && entity.guid == TIMER)
                {
                    return Err("not an actual original Timer target".into());
                }
                let index = u32::try_from(dec(&v["interaction"])?)?;
                let offers = p.offers(target)?;
                let choice = offers
                    .offers
                    .iter()
                    .find(|offer| {
                        offer.interaction.tta_index == index
                            && offer.interaction.scope == InteractionScope::Local
                    })
                    .ok_or("original authorized Timer offer absent")?;
                let choice = json!({"target":target,"interaction":choice.interaction,"param0":choice.param0,"label":choice.label});
                let offered = offers
                    .offers
                    .iter()
                    .find(|offer| {
                        offer.interaction.tta_index == index
                            && offer.interaction.scope == InteractionScope::Local
                    })
                    .unwrap();
                let action = p.prepare(target, offered.interaction, offered.param0)?;
                prepared = true;
                return Ok(json!({"action_hex":hex(&action),"selection":choice}));
            }
            let projection = p.projection()?;
            let mut timers = Vec::new();
            for entity in projection.entities.iter().filter(|e| e.guid == TIMER) {
                let offers = p.offers(entity.reference)?;
                timers.push(json!({"entity":entity.reference,"guid":entity.guid,"offers":offers.offers.iter().map(|o|json!({"interaction":o.interaction,"param0":o.param0,"label":o.label})).collect::<Vec<_>>()}));
            }
            let terminal =
                selected.and_then(|id| p.activity().find(|activity| activity.action == id));
            Ok(
                json!({"tick":projection.tick,"actor":p.actor(),"principal":p.principal().0,"state_hash":hex(&p.state_hash()?),"timers":timers,"accepted":p.status()==ActionStatus::Accepted,"rejected":p.status()==ActionStatus::Rejected,"selected_action":selected,"terminal":terminal.map(|a|a.result.label()),"external_effects":effects,"thread_faults":faults}),
            )
        })();
        println!(
            "{}",
            match result {
                Ok(ok) => json!({"id":v["id"],"ok":ok}),
                Err(error) => json!({"id":v["id"],"error":error.to_string()}),
            }
        );
        std::io::stdout().flush()?;
    }
    Ok(())
}
