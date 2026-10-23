//! Read-only whole-lane classification. Only complete native source coverage and
//! retained exact owner/session receipts can authorize the coordinator consumer.
use crate::{
    Result,
    eod::TimerAdapter,
    events::{EventEnvelope, SupervisorEvent},
    runtime::{Host, cursor, hex, unhex},
};
use bincode::Options;
use serde_json::{Value, json};
use wonderland_content_ir::manifest::Digest;
use wonderland_game_runtime::sim_core::runtime::leave_adapter::DepartureRemovalReason;
use wonderland_native_host_contracts::{DecimalId, DepartureRequestV1};

pub(crate) fn classify(host: &Host, adapter: &TimerAdapter, encoded: &str) -> Result<Value> {
    if !host.recovery_committed {
        return Err("event consumer awaits committed recovery".into());
    }
    let bytes = unhex(encoded)?;
    if bytes.len() < 8 || u64::from_le_bytes(bytes[..8].try_into()?) > 128 {
        return Err("owned event lane member bound".into());
    }
    let codec = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(256 * 1024)
        .reject_trailing_bytes();
    let events: Vec<EventEnvelope> = codec.deserialize(&bytes)?;
    if events.is_empty() || codec.serialize(&events)? != bytes {
        return Err("noncanonical owned event lane".into());
    }
    let state = host.committed.sim().state();
    let mut members = vec![];
    let mut pending = vec![];
    let mut previous = None;
    for envelope in events {
        let at = (envelope.tick, envelope.ordinal);
        if envelope.tick == 0
            || envelope.tick > state.completed_tick
            || previous.is_some_and(|p| p >= at)
        {
            return Err("owned event lane order/tick mismatch".into());
        }
        previous = Some(at);
        let (actor, generation, kind) = match envelope.event {
            SupervisorEvent::OwnedDepartureCaptured {
                actor,
                control_generation,
                tick,
            } if tick == envelope.tick => (actor, control_generation, "capture"),
            SupervisorEvent::OwnedDepartureDetached {
                actor,
                control_generation,
                tick,
                ..
            } if tick == envelope.tick => (actor, control_generation, "detach"),
            SupervisorEvent::OwnedEodDisconnectRequested {
                actor,
                control_generation,
            } => (actor, control_generation, "eod"),
            _ => {
                pending.push(json!({"tick":DecimalId(envelope.tick),"ordinal":envelope.ordinal,"reason":"unsupported_event_kind_or_tick"}));
                continue;
            }
        };
        let result = (|| -> Result<Value> {
            let record = state
                .owned_departures
                .records
                .get(&actor)
                .ok_or("retained full owner/departure proof missing")?;
            let owner = &record.request.control;
            if owner.actor != actor || owner.control_generation != generation {
                return Err("retained owner generation mismatch".into());
            }
            let request: DepartureRequestV1 = serde_json::from_value(
                json!({"schema_version":1,"departure_operation":record.request.departure_operation,"identity":{
                    "authority_id":hex(&record.request.authority_id),"lot_id":DecimalId(state.lot_id),"principal":DecimalId(owner.principal),"account_id":DecimalId(owner.account_id),"avatar_id":DecimalId(u64::from(owner.avatar_id.0)),
                    "actor":{"object_id":actor.object_id.0,"generation":actor.generation},"control_generation":DecimalId(generation),"session_ref":hex(&owner.session_ref),"admission_operation":owner.admission_operation,"reservation_digest":hex(&owner.reservation_digest),"export_digest":hex(&owner.export_digest)
                }}),
            )?;
            request.validate()?;
            let detail = match envelope.event {
                SupervisorEvent::OwnedDepartureCaptured { tick, .. } => {
                    if record.capture.as_ref().map(|c| c.captured_tick) != Some(tick) {
                        return Err("source capture receipt mismatch".into());
                    }
                    json!({})
                }
                SupervisorEvent::OwnedDepartureDetached { tick, reason, .. } => {
                    if record.detached_tick != Some(tick) || record.removal_reason != Some(reason) {
                        return Err("source detach receipt mismatch".into());
                    }
                    json!({"removal_reason":match reason {DepartureRemovalReason::SourceLeave=>"source_leave", DepartureRemovalReason::ForcedCleanup=>"forced_cleanup"}})
                }
                SupervisorEvent::OwnedEodDisconnectRequested { .. } => {
                    adapter.owned_disconnect_disposition(state, owner)?
                }
                _ => unreachable!(),
            };
            Ok(
                json!({"kind":kind,"tick":DecimalId(envelope.tick),"ordinal":envelope.ordinal,"request":request,"detail":detail}),
            )
        })();
        match result {
            Ok(member) => members.push(member),
            Err(error) => pending.push(json!({"tick":DecimalId(envelope.tick),"ordinal":envelope.ordinal,"reason":error.to_string()})),
        }
    }
    Ok(
        json!({"schema_version":1,"kind":"owned_event_lane_proof","binding":host.binding,"cursor":cursor(&host.committed)?,"native_eod_checkpoint_sha256":adapter.committed_digest()?,"events_sha256":Digest::of(&bytes).as_str(),"members":members,"pending":pending}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use wonderland_game_runtime::{EntityRef, ObjectId};
    #[test]
    fn owned_event_wire_contract_keeps_real_ordinals_and_append_only_tags() {
        let actor = EntityRef {
            object_id: ObjectId(2),
            generation: 3,
        };
        let events = vec![
            EventEnvelope {
                tick: 1,
                ordinal: 0,
                event: SupervisorEvent::OwnedEodDisconnectRequested {
                    actor,
                    control_generation: 2,
                },
            },
            EventEnvelope {
                tick: 1,
                ordinal: 1,
                event: SupervisorEvent::OwnedDepartureCaptured {
                    actor,
                    control_generation: 2,
                    tick: 1,
                },
            },
            EventEnvelope {
                tick: 1,
                ordinal: 2,
                event: SupervisorEvent::OwnedDepartureDetached {
                    actor,
                    control_generation: 2,
                    tick: 1,
                    reason: DepartureRemovalReason::SourceLeave,
                },
            },
        ];
        let bytes = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .serialize(&events)
            .unwrap();
        assert_eq!(
            hex(&bytes),
            "03000000000000000100000000000000000000000600000002000300000002000000000000000100000000000000010000000400000002000300000002000000000000000100000000000000010000000000000002000000050000000200030000000200000000000000010000000000000000000000"
        );
    }
}
