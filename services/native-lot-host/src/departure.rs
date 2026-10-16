//! Private owned-drain adapter and source StateToDb/SaveAvatar projection.
//! Only accepted runtime observations become proposal evidence; account save
//! receipts and custody release remain the coordinator/account authority owners.
use crate::Result;
use std::collections::BTreeMap;
use wonderland_content_ir::manifest::Digest;
use wonderland_game_runtime::{
    PersistentId,
    sim_core::avatars::{outfits::OutfitReference, state::AvatarPersistState},
};
use wonderland_native_host_contracts::*;

fn asset_id(value: &OutfitReference) -> Result<DecimalId> {
    match value {
        OutfitReference::Id(id) => Ok(DecimalId(*id)),
        _ => Err("source lot-save requires an ID outfit reference".into()),
    }
}
/// `portable` must be the real AvatarState::persistent_state() capture. The
/// dirty map contains only existing outgoing persistent matrices selected by
/// the authoritative dirty set; skipped missing matrices must not be invented.
/// No defaults, permissions, ledger, inventory, ownership or custody mutations.
pub(crate) fn produce_source_avatar_lot_save(
    identity: &DepartureIdentityV1,
    departure_operation: &str,
    captured_tick: u64,
    portable: &AvatarPersistState,
    changed_relationships: &BTreeMap<PersistentId, Vec<i16>>,
) -> Result<SourceAvatarLotSaveV1> {
    DepartureRequestV1 {
        schema_version: 1,
        identity: identity.clone(),
        departure_operation: departure_operation.to_owned(),
    }
    .validate()?;
    if identity.avatar_id.0 != u64::from(portable.persistent_id.0) {
        return Err("source lot-save avatar identity mismatch".into());
    }
    if captured_tick == 0 {
        return Err("source lot-save capture tick must be nonzero".into());
    }
    if portable.motive_data[11] != 0 {
        return Err("source lot-save requires normalized persistent SleepState".into());
    }
    if changed_relationships.len() > SOURCE_RELATIONSHIP_LIMIT {
        return Err("source relationship capacity".into());
    }
    let changed_relationships = changed_relationships
        .iter()
        .map(|(target, values)| {
            if target.0 == 0 || values.len() > SOURCE_RELATIONSHIP_LIMIT {
                return Err("source relationship target or value capacity".into());
            }
            if portable.relationships.get(target) != Some(values) {
                return Err("source dirty relationship capture mismatch".into());
            }
            Ok(SourceRelationshipV1 {
                target: DecimalId(u64::from(target.0)),
                values: values.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let p = &portable.person_data;
    // Rust's signed division truncates toward zero; `as` has the required
    // unchecked C# byte/ushort narrowing, including negative source shorts.
    let avatar_columns = SourceAvatarColumnsV1 {
        motive_data_hex: crate::runtime::hex(
            &portable
                .motive_data
                .iter()
                .flat_map(|m| m.to_be_bytes())
                .collect::<Vec<_>>(),
        ),
        skilllock: p[10] as u8,
        lock_body: (p[11] / 100) as u16,
        lock_charisma: (p[12] / 100) as u16,
        lock_cooking: (p[13] / 100) as u16,
        lock_creativity: (p[14] / 100) as u16,
        lock_logic: (p[15] / 100) as u16,
        lock_mechanical: (p[16] / 100) as u16,
        skill_cooking: p[0] as u16,
        skill_charisma: p[1] as u16,
        skill_mechanical: p[2] as u16,
        skill_creativity: p[3] as u16,
        skill_body: p[4] as u16,
        skill_logic: p[5] as u16,
        body: asset_id(&portable.default_suits.daywear)?,
        body_sleepwear: asset_id(&portable.default_suits.sleepwear)?,
        body_swimwear: asset_id(&portable.default_suits.swimwear)?,
        body_current: asset_id(
            portable
                .body
                .as_ref()
                .ok_or("source lot-save body is missing")?,
        )?,
        current_job: p[21] as u16,
        is_ghost: p[7] as u16,
        ticker_death: p[17] as u16,
        ticker_gardener: p[18] as u16,
        ticker_maid: p[19] as u16,
        ticker_repairman: p[20] as u16,
    };
    let current_job_row = if avatar_columns.current_job == 0 {
        None
    } else {
        portable.job_info.get(&p[21]).map(|job| SourceJobRowV1 {
            job_type: avatar_columns.current_job,
            job_experience: job.experience as u16,
            job_level: job.level as u16,
            job_sickdays: job.sick_days as u16,
            job_statusflags: job.status_flags as u16,
        })
    };
    let record = SourceAvatarLotSaveV1 {
        schema_version: 1,
        source_pin: SOURCE_LOT_SAVE_PIN.to_owned(),
        identity: identity.clone(),
        departure_operation: departure_operation.to_owned(),
        save_sequence: DecimalId(1),
        captured_tick: DecimalId(captured_tick),
        capture_reason: SourceCaptureReasonV1::FirstLeaveTick,
        avatar_columns,
        current_job_row,
        changed_relationships,
    };
    record.validate()?;
    Ok(record)
}
pub(crate) fn canonical_source_avatar_lot_save(record: &SourceAvatarLotSaveV1) -> Result<Vec<u8>> {
    Ok(record.canonical_bytes()?)
}
pub(crate) fn source_avatar_lot_save_sha256(record: &SourceAvatarLotSaveV1) -> Result<String> {
    Ok(Digest::of(&canonical_source_avatar_lot_save(record)?)
        .as_str()
        .to_owned())
}

/// Revalidate the exact installed generation, not the frozen account export.
/// Expired sessions still need cleanup; this private request grants no input.
pub(crate) fn owned_drain_command(
    host: &crate::runtime::Host,
    request: &DepartureRequestV1,
    fence: &ControlFenceV1,
) -> Result<wonderland_game_runtime::AcceptedCommand> {
    use wonderland_game_runtime::sim_core::runtime::leave_adapter::OwnedLeaveRequest;
    use wonderland_game_runtime::{AcceptedCommand, EntityRef, ObjectId};
    fence.matches_departure(request)?;
    let state = host.committed.sim().state();
    let identity = &request.identity;
    if identity.lot_id.0 != state.lot_id || host.binding.source_pin != SOURCE_LOT_SAVE_PIN {
        return Err("owned drain lot or source pin mismatch".into());
    }
    state.owned_admission.validate(state)?;
    let actor = EntityRef {
        object_id: ObjectId(i16::try_from(identity.actor.object_id)?),
        generation: identity.actor.generation,
    };
    if state
        .owned_departures
        .records
        .values()
        .any(|record| record.detached_tick.is_none() && record.request.control.actor != actor)
    {
        return Err("another owned departure must drain first".into());
    }
    if state
        .owned_departures
        .records
        .get(&actor)
        .is_some_and(|record| record.detached_tick.is_some())
    {
        return Err("owned departure already detached; recover its committed receipt".into());
    }
    let control = state
        .owned_admission
        .actors()
        .and_then(|actors| actors.get(&identity.principal.0))
        .ok_or("owned drain actor not installed")?;
    if control.account_id != identity.account_id.0
        || u64::from(control.avatar_id.0) != identity.avatar_id.0
        || control.actor != actor
        || control.control_generation != identity.control_generation.0
        || crate::runtime::hex(&control.session_ref) != identity.session_ref
        || control.admission_operation != identity.admission_operation
        || crate::runtime::hex(&control.reservation_digest) != identity.reservation_digest
        || crate::runtime::hex(&control.export_digest) != identity.export_digest
    {
        return Err("owned drain installed control mismatch".into());
    }
    let leave = OwnedLeaveRequest {
        authority_id: crate::player::digest32(&identity.authority_id)?,
        control: control.clone(),
        departure_operation: request.departure_operation.clone(),
    };
    leave.validate()?;
    if state
        .owned_departures
        .records
        .get(&actor)
        .is_some_and(|record| record.request != leave)
    {
        return Err("owned drain retained generation mismatch".into());
    }
    Ok(AcceptedCommand::RequestOwnedLeave(Box::new(leave)))
}

/// Queue cancellation can request EOD teardown for an installed peer as well
/// as the leaver. Retain the exact generation request without treating it as a
/// departure, a session association, or a disconnect acknowledgement.
pub(crate) fn validate_eod_consequences(
    before: &wonderland_game_runtime::GameRuntime,
    after: &wonderland_game_runtime::GameRuntime,
    events: &[crate::events::EventEnvelope],
) -> Result<()> {
    use crate::events::SupervisorEvent;
    let old = before.sim().state();
    let new = after.sim().state();
    let mut seen = std::collections::BTreeSet::new();
    for envelope in events {
        let SupervisorEvent::OwnedEodDisconnectRequested {
            actor,
            control_generation,
        } = &envelope.event
        else {
            continue;
        };
        let exact_installed = [old, new].into_iter().any(|state| {
            state.owned_admission.actors().is_some_and(|actors| {
                actors.values().any(|control| {
                    control.actor == *actor && control.control_generation == *control_generation
                })
            })
        });
        let key = (*actor, *control_generation);
        if !exact_installed
            || envelope.tick != new.completed_tick
            || !new.owned_departures.eod_disconnect_requests.contains(&key)
            || old.owned_departures.eod_disconnect_requests.contains(&key)
            || !seen.insert(key)
        {
            return Err("owned EOD consequence lacks exact new generation-bound request".into());
        }
    }
    Ok(())
}

/// Export only new observations from this accepted tick. Retained captures are
/// not emitted twice and event ordinals are indices in the real runtime stream.
pub(crate) fn departure_evidence(
    before: &wonderland_game_runtime::GameRuntime,
    after: &wonderland_game_runtime::GameRuntime,
    request: &DepartureRequestV1,
    events: &[crate::events::EventEnvelope],
) -> Result<DepartureEvidenceV1> {
    use crate::events::SupervisorEvent;
    use wonderland_game_runtime::sim_core::runtime::leave_adapter::DepartureRemovalReason;
    use wonderland_game_runtime::{EntityRef, ObjectId};
    request.validate()?;
    validate_eod_consequences(before, after, events)?;
    let state = after.sim().state();
    let tick = state.completed_tick;
    if before.sim().state().completed_tick.checked_add(1) != Some(tick) {
        return Err("owned drain must advance exactly one accepted tick".into());
    }
    let identity = &request.identity;
    let actor = EntityRef {
        object_id: ObjectId(i16::try_from(identity.actor.object_id)?),
        generation: identity.actor.generation,
    };
    let record = state
        .owned_departures
        .records
        .get(&actor)
        .ok_or("owned drain observation missing")?;
    if record.request.departure_operation != request.departure_operation
        || crate::runtime::hex(&record.request.authority_id) != identity.authority_id
        || record.request.control.control_generation != identity.control_generation.0
        || record.request.control.actor != actor
    {
        return Err("owned drain retained observation mismatch".into());
    }
    let mut save_ordinal = None;
    let mut detach_observation = None;
    for envelope in events {
        let (event_actor, generation) = match &envelope.event {
            SupervisorEvent::OwnedDepartureCaptured {
                actor,
                control_generation,
                ..
            }
            | SupervisorEvent::OwnedDepartureDetached {
                actor,
                control_generation,
                ..
            } => (*actor, *control_generation),
            _ => continue,
        };
        if event_actor != actor
            || generation != identity.control_generation.0
            || envelope.tick != tick
        {
            return Err("owned drain observed an unrelated departure".into());
        }
        match &envelope.event {
            SupervisorEvent::OwnedDepartureCaptured { tick: observed, .. } => {
                if *observed != tick
                    || save_ordinal
                        .replace(u16::try_from(envelope.ordinal)?)
                        .is_some()
                {
                    return Err("owned drain duplicate or mistimed save event".into());
                }
            }
            SupervisorEvent::OwnedDepartureDetached {
                tick: observed,
                reason,
                ..
            } => {
                if *observed != tick
                    || record.removal_reason != Some(*reason)
                    || detach_observation.is_some()
                {
                    return Err("owned drain duplicate or mistimed detach event".into());
                }
                detach_observation = Some(DepartureDetachObservationV1 {
                    detached_tick: DecimalId(*observed),
                    removal_reason: match reason {
                        DepartureRemovalReason::SourceLeave => {
                            DepartureRemovalReasonV1::SourceLeave
                        }
                        DepartureRemovalReason::ForcedCleanup => {
                            DepartureRemovalReasonV1::ForcedCleanup
                        }
                    },
                    event_ordinal: u16::try_from(envelope.ordinal)?,
                });
            }
            _ => unreachable!(),
        }
    }
    let captured_now = record
        .capture
        .as_ref()
        .is_some_and(|capture| capture.captured_tick == tick);
    if captured_now != save_ordinal.is_some()
        || record.detached_tick.is_some() != detach_observation.is_some()
        || record
            .detached_tick
            .is_some_and(|detached| detached != tick)
    {
        return Err("owned drain missing current save or detach evidence".into());
    }
    let save = save_ordinal
        .map(|ordinal| -> Result<DepartureSaveObservationV1> {
            let capture = record
                .capture
                .as_ref()
                .ok_or("owned save capture missing")?;
            let save_record = produce_source_avatar_lot_save(
                identity,
                &request.departure_operation,
                capture.captured_tick,
                &capture.portable,
                &capture.changed_relationships,
            )?;
            let save_sha256 = source_avatar_lot_save_sha256(&save_record)?;
            Ok(DepartureSaveObservationV1 {
                save_record,
                save_sha256,
                event_ordinal: ordinal,
            })
        })
        .transpose()?;
    let status = if detach_observation.is_some() {
        DepartureLifecycleStatusV1::Detached
    } else if record.blocker.is_some() {
        DepartureLifecycleStatusV1::Blocked
    } else {
        DepartureLifecycleStatusV1::Leaving
    };
    let result = DepartureEvidenceV1 {
        schema_version: 1,
        identity: identity.clone(),
        departure_operation: request.departure_operation.clone(),
        lifecycle: DepartureLifecycleV1 {
            observed_tick: DecimalId(tick),
            status,
            blocker: record.blocker.map(|blocker| blocker.code().to_owned()),
        },
        save,
        detach: detach_observation,
        entity_refs_after: state
            .entities
            .values()
            .map(|entity| -> Result<DepartureEntityRefV1> {
                Ok(DepartureEntityRefV1 {
                    object_id: u16::try_from(entity.info.reference.object_id.0)?,
                    generation: entity.info.reference.generation,
                })
            })
            .collect::<Result<Vec<_>>>()?,
    };
    result.validate()?;
    Ok(result)
}

/// Coordinator fences and runtime registry changes have separate revisions.
/// Count the actual registry diff; never predict it from an admission flag.
pub(crate) fn control_revision_after(
    before: &wonderland_game_runtime::GameRuntime,
    after: &wonderland_game_runtime::GameRuntime,
    revision: u64,
    departure: Option<&DepartureEvidenceV1>,
) -> Result<u64> {
    let before = before
        .sim()
        .state()
        .owned_admission
        .actors()
        .ok_or("owned registry missing")?;
    let state = after.sim().state();
    state.owned_admission.validate(state)?;
    let after = state
        .owned_admission
        .actors()
        .ok_or("owned registry missing")?;
    let mut changes = 0_u64;
    let mut removed = 0;
    for (principal, control) in before {
        match after.get(principal) {
            Some(next) if next != control => return Err("installed actor identity changed".into()),
            Some(_) => {}
            None => {
                let evidence =
                    departure.ok_or("ordinary controlled commit cannot remove actors")?;
                if evidence.identity.principal.0 != *principal
                    || i16::try_from(evidence.identity.actor.object_id)?
                        != control.actor.object_id.0
                    || evidence.identity.actor.generation != control.actor.generation
                    || evidence.identity.control_generation.0 != control.control_generation
                    || evidence.detach.is_none()
                    || state
                        .entities
                        .get(&control.actor.object_id)
                        .is_some_and(|entity| entity.info.reference == control.actor)
                {
                    return Err("owned drain removal lacks exact detach evidence".into());
                }
                removed += 1;
                changes += 1;
            }
        }
    }
    for principal in after.keys() {
        if !before.contains_key(principal) {
            if departure.is_some() {
                return Err("owned drain cannot install actors".into());
            }
            changes += 1;
        }
    }
    if departure.is_some_and(|evidence| evidence.detach.is_some()) != (removed == 1) {
        return Err("owned drain detach registry mismatch".into());
    }
    revision
        .checked_add(changes)
        .ok_or_else(|| "control revision exhausted".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeSet;
    use wonderland_game_runtime::sim_core::avatars::{
        outfits::DefaultSuits,
        state::{AvatarPermissions, JobInfo},
    };

    fn identity() -> DepartureIdentityV1 {
        DepartureIdentityV1 {
            authority_id: "1".repeat(64),
            account_id: DecimalId(1),
            avatar_id: DecimalId(2),
            lot_id: DecimalId(3),
            control_generation: DecimalId(u64::MAX),
            session_ref: "2".repeat(64),
            admission_operation: "admission_producer_0001".into(),
            reservation_digest: "3".repeat(64),
            export_digest: "4".repeat(64),
            principal: DecimalId(5),
            actor: DepartureEntityRefV1 {
                object_id: 6,
                generation: 7,
            },
        }
    }
    // Authored boundary inputs. These are never used as admission defaults.
    fn portable() -> AvatarPersistState {
        AvatarPersistState {
            name: "Not in the whitelist".into(),
            persistent_id: PersistentId(2),
            default_suits: DefaultSuits {
                daywear: OutfitReference::Id(203),
                swimwear: OutfitReference::Id(201),
                sleepwear: OutfitReference::Id(202),
            },
            permissions: AvatarPermissions::Admin,
            avatar_flags: u32::MAX,
            budget_mirror: u32::MAX,
            body: Some(OutfitReference::Id(u64::MAX)),
            head: Some(OutfitReference::Id(987)),
            skin_tone: 2,
            is_worker: false,
            custom_guid: 0,
            motive_data: [
                -32768, -1, 32767, 10, 10, 10, 10, 10, 10, 10, 10, 0, 10, 10, 10, 10,
            ],
            person_data: [
                -32768, -1, 32767, 1, -199, 100, 1, -3, 3, 4, -1, -199, -99, -32768, 32767, 199,
                -100, -1, 32767, -32768, -2, -1, 1, 2, 3, 4, 5,
            ],
            relationships: BTreeMap::from([
                (PersistentId(98), vec![321]),
                (PersistentId(99), vec![-32768]),
                (PersistentId(100), vec![]),
                (PersistentId(u32::MAX), vec![1, -1]),
            ]),
            job_info: BTreeMap::from([
                (
                    -1,
                    JobInfo {
                        experience: -1,
                        level: -32768,
                        sick_days: 1,
                        status_flags: -2,
                    },
                ),
                (
                    3,
                    JobInfo {
                        experience: 7,
                        level: 8,
                        sick_days: 9,
                        status_flags: 10,
                    },
                ),
            ]),
            ignored_avatars: BTreeSet::from([PersistentId(99)]),
        }
    }
    fn dirty(p: &AvatarPersistState) -> BTreeMap<PersistentId, Vec<i16>> {
        p.relationships
            .iter()
            .filter(|(k, _)| k.0 != 98)
            .map(|(k, v)| (*k, v.clone()))
            .collect()
    }
    fn produce(p: &AvatarPersistState) -> Result<SourceAvatarLotSaveV1> {
        let i = identity();
        produce_source_avatar_lot_save(&i, &departure_operation_v1(&i)?, 11, p, &dirty(p))
    }
    #[test]
    fn source_save_whitelist_and_unchecked_casts_match_authored_golden() {
        let p = portable();
        let before = p.clone();
        let record = produce(&p).unwrap();
        assert_eq!(
            p, before,
            "pure producer must not consume or clear dirty state"
        );
        let value = serde_json::to_value(&record).unwrap();
        assert_eq!(
            value["avatar_columns"],
            json!({
                "motive_data_hex": "8000ffff7fff000a000a000a000a000a000a000a000a0000000a000a000a000a",
                "skilllock": 255, "lock_body": 65535, "lock_charisma": 0, "lock_cooking": 65209,
                "lock_creativity": 327, "lock_logic": 1, "lock_mechanical": 65535,
                "skill_cooking": 32768, "skill_charisma": 65535, "skill_mechanical": 32767,
                "skill_creativity": 1, "skill_body": 65337, "skill_logic": 100,
                "body": "203", "body_sleepwear": "202", "body_swimwear": "201", "body_current": "18446744073709551615",
                "current_job": 65535, "is_ghost": 65533, "ticker_death": 65535, "ticker_gardener": 32767,
                "ticker_maid": 32768, "ticker_repairman": 65534
            })
        );
        assert_eq!(value["avatar_columns"].as_object().unwrap().len(), 24);
        assert_eq!(value.as_object().unwrap().len(), 10);
        assert_eq!(
            value["current_job_row"],
            json!({"job_type":65535,"job_experience":65535,
            "job_level":32768,"job_sickdays":1,"job_statusflags":65534})
        );
        assert_eq!(
            value["changed_relationships"],
            json!([
                {"target":"99","values":[-32768]}, {"target":"100","values":[]},
                {"target":"4294967295","values":[1,-1]}
            ])
        );
        let canonical = canonical_source_avatar_lot_save(&record).unwrap();
        assert_eq!(canonical.len(), 1642);
        assert_eq!(
            serde_json::from_slice::<SourceAvatarLotSaveV1>(&canonical).unwrap(),
            record
        );
        assert_eq!(
            source_avatar_lot_save_sha256(&record).unwrap(),
            "f15d8348e4c6c36ca9e3e4dd8f18570567f277e6c8859990f91e63556de93fa5"
        );
    }
    #[test]
    fn only_the_current_signed_key_is_selected_and_missing_or_zero_job_is_null() {
        let mut p = portable();
        p.person_data[21] = 3;
        assert_eq!(
            produce(&p).unwrap().current_job_row.unwrap().job_experience,
            7
        );
        p.person_data[21] = 4;
        assert!(produce(&p).unwrap().current_job_row.is_none());
        p.person_data[21] = 0;
        p.job_info.insert(0, JobInfo::default());
        assert!(produce(&p).unwrap().current_job_row.is_none());
    }
    #[test]
    fn identity_operation_tick_and_sleep_mismatches_fail_closed() {
        let p = portable();
        let i = identity();
        let op = departure_operation_v1(&i).unwrap();
        let mut wrong = i.clone();
        wrong.avatar_id = DecimalId(3);
        assert!(
            produce_source_avatar_lot_save(
                &wrong,
                &departure_operation_v1(&wrong).unwrap(),
                11,
                &p,
                &dirty(&p)
            )
            .is_err()
        );
        assert!(
            produce_source_avatar_lot_save(&i, "wrong_operation_0001", 11, &p, &dirty(&p)).is_err()
        );
        assert!(produce_source_avatar_lot_save(&i, &op, 0, &p, &dirty(&p)).is_err());
        let mut sleeping = p;
        sleeping.motive_data[11] = 1;
        assert!(produce(&sleeping).is_err());
    }
    #[test]
    fn no_asset_ids_are_guessed_for_missing_name_or_legacy_outfits() {
        let p = portable();
        let invalid = [
            OutfitReference::Name("not-an-id".into()),
            OutfitReference::Legacy {
                definition: "not-an-id".into(),
                head: false,
                hand_group: None,
            },
        ];
        for outfit in invalid {
            for field in 0..4 {
                let mut candidate = p.clone();
                match field {
                    0 => candidate.default_suits.daywear = outfit.clone(),
                    1 => candidate.default_suits.sleepwear = outfit.clone(),
                    2 => candidate.default_suits.swimwear = outfit.clone(),
                    _ => candidate.body = Some(outfit.clone()),
                }
                assert!(produce(&candidate).is_err());
            }
        }
        let mut absent = p;
        absent.body = None;
        assert!(produce(&absent).is_err());
    }
    #[test]
    fn dirty_capture_values_must_match_the_actual_portable_state() {
        let p = portable();
        let i = identity();
        let op = departure_operation_v1(&i).unwrap();
        for incorrect in [
            BTreeMap::from([(PersistentId(99), vec![0])]),
            BTreeMap::from([(PersistentId(101), vec![])]),
            BTreeMap::from([(PersistentId(0), vec![])]),
        ] {
            assert!(produce_source_avatar_lot_save(&i, &op, 11, &p, &incorrect).is_err());
        }
    }
    #[test]
    fn relationship_capacity_is_rejected_without_truncation() {
        let mut p = portable();
        p.relationships = (1..=257).map(|id| (PersistentId(id), vec![])).collect();
        let i = identity();
        let op = departure_operation_v1(&i).unwrap();
        assert!(produce_source_avatar_lot_save(&i, &op, 11, &p, &p.relationships).is_err());
        p.relationships = BTreeMap::from([(PersistentId(1), vec![0; 257])]);
        assert!(produce_source_avatar_lot_save(&i, &op, 11, &p, &p.relationships).is_err());
        p.relationships = (1..=256)
            .map(|id| (PersistentId(id), vec![i16::MIN; 256]))
            .collect();
        assert!(produce_source_avatar_lot_save(&i, &op, 11, &p, &p.relationships).is_err());
        p.relationships = (1..=256).map(|id| (PersistentId(id), vec![])).collect();
        assert_eq!(
            produce_source_avatar_lot_save(&i, &op, 11, &p, &p.relationships)
                .unwrap()
                .changed_relationships
                .len(),
            256
        );
        p.relationships = BTreeMap::from([(PersistentId(u32::MAX), vec![i16::MIN; 256])]);
        assert_eq!(
            produce_source_avatar_lot_save(&i, &op, 11, &p, &p.relationships)
                .unwrap()
                .changed_relationships[0]
                .values
                .len(),
            256
        );
    }
    #[test]
    fn non_whitelisted_portable_mutations_never_change_the_save() {
        let mut p = portable();
        let baseline = produce(&p).unwrap();
        p.name = "Different".into();
        p.permissions = AvatarPermissions::Visitor;
        p.avatar_flags = 0;
        p.budget_mirror = 0;
        p.head = None;
        p.skin_tone = 0;
        p.is_worker = true;
        p.custom_guid = 123;
        p.ignored_avatars.clear();
        for slot in [6, 8, 9, 22, 23, 24, 25, 26] {
            p.person_data[slot] = i16::MIN;
        }
        p.job_info.get_mut(&3).unwrap().level = 42;
        p.relationships.get_mut(&PersistentId(98)).unwrap()[0] = 0;
        assert_eq!(produce(&p).unwrap(), baseline);
    }
}
