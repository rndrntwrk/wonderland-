//! Player-scoped private pipe adapter. Authentication and durable custody remain
//! at the coordinator; exact replicated actor identity is rechecked here.
use crate::{
    Result,
    runtime::{Host, checkpoint, hex, unhex},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use wonderland_content_ir::manifest::Digest;
use wonderland_game_runtime::{
    AcceptedCommand, EntityRef, ObjectId, PersistentId, PrincipalKey,
    live_wire::{self, player},
    sim_core::{
        avatars::{
            outfits::{DefaultSuits, OutfitReference},
            state::{AvatarPersistState, JobInfo},
        },
        owned_admission::*,
    },
};
use wonderland_native_host_contracts::*;

pub(crate) fn digest32(input: &str) -> Result<[u8; 32]> {
    let value = unhex(input)?;
    value.try_into().map_err(|_| "expected SHA256".into())
}
pub(crate) fn actors(runtime: &wonderland_game_runtime::GameRuntime) -> Value {
    let rows:Vec<_> = runtime.sim().state().owned_admission.actors().into_iter().flat_map(|a|a.values()).map(|a|json!({
        "principal":DecimalId(a.principal),"account_id":DecimalId(a.account_id),"avatar_id":DecimalId(u64::from(a.avatar_id.0)),
        "actor":{"object_id":a.actor.object_id.0,"generation":a.actor.generation},"control_generation":DecimalId(a.control_generation),
        "session_ref":hex(&a.session_ref),"admission_operation":a.admission_operation,"reservation_digest":hex(&a.reservation_digest),"export_digest":hex(&a.export_digest)
    })).collect();
    json!(rows)
}
/// Read-only placement facts from the exact committed actor/control generation.
/// Custody and placement are separate: a source-compatible join may remain OOW.
pub(crate) fn actor_placements(runtime: &wonderland_game_runtime::GameRuntime) -> Result<Value> {
    let state = runtime.sim().state();
    let mut rows = Vec::new();
    for control in state
        .owned_admission
        .actors()
        .into_iter()
        .flat_map(|actors| actors.values())
    {
        let entity = state
            .entities
            .get(&control.actor.object_id)
            .ok_or("owned placement actor missing")?;
        if !state.ids.is_live(control.actor)
            || entity.info.reference != control.actor
            || entity.info.persistent_id != control.avatar_id.0
            || !entity.info.is_avatar
            || entity.info.dead
        {
            return Err("owned placement identity mismatch".into());
        }
        let position = entity.info.position;
        let in_world =
            state
                .world
                .lot
                .contains_position(wonderland_game_runtime::LotPosition::new(
                    i32::from(position.x),
                    i32::from(position.y),
                    u8::try_from(position.level)?,
                ));
        rows.push(json!({"principal":DecimalId(control.principal),"avatar_id":DecimalId(u64::from(control.avatar_id.0)),
            "control_generation":DecimalId(control.control_generation),
            "actor":{"object_id":control.actor.object_id.0,"generation":control.actor.generation},
            "position":{"x":position.x,"y":position.y,"level":position.level},"in_world":in_world}));
    }
    Ok(json!(rows))
}
pub(crate) fn registry_hash(runtime: &wonderland_game_runtime::GameRuntime) -> Result<String> {
    Ok(Digest::of(&serde_json::to_vec(&actors(runtime))?)
        .as_str()
        .to_owned())
}
pub(crate) fn source_location(runtime: &wonderland_game_runtime::GameRuntime) -> Option<u32> {
    match &runtime.sim().state().owned_admission {
        OwnedAdmissionState::Initialized { source, .. } => Some(source.lot_location),
        _ => None,
    }
}
pub(crate) fn permission(runtime: &wonderland_game_runtime::GameRuntime) -> Result<Value> {
    let state = runtime.sim().state();
    let OwnedAdmissionState::Initialized {
        permission_revision,
        ..
    } = &state.owned_admission
    else {
        return Ok(Value::Null);
    };
    Ok(json!({"revision":DecimalId(*permission_revision),"sha256":hex(&permission_digest(state)?)}))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Portable {
    name: String,
    persistent_id: DecimalId,
    default_suits: Suits,
    budget_mirror: DecimalId,
    body: Option<DecimalId>,
    head: Option<DecimalId>,
    skin_tone: u8,
    is_worker: bool,
    custom_guid: u32,
    motive_data: [i16; 16],
    person_data: [i16; 27],
    relationships: Vec<Relationship>,
    job_info: Vec<Job>,
    ignored_avatars: Vec<DecimalId>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Suits {
    daywear: DecimalId,
    swimwear: DecimalId,
    sleepwear: DecimalId,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Relationship {
    target: DecimalId,
    values: Vec<i16>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    id: i16,
    experience: i16,
    level: i16,
    sick_days: i16,
    status_flags: i16,
}
fn pid(value: DecimalId) -> Result<PersistentId> {
    Ok(PersistentId(u32::try_from(value.0)?))
}
fn convert_portable(
    export: &ReservedAvatarExportV2,
    runtime: &wonderland_game_runtime::GameRuntime,
) -> Result<AvatarPersistState> {
    if export.portable_json.len() > 65536
        || Digest::of(export.portable_json.as_bytes()).as_str() != export.portable_sha256
    {
        return Err("portable digest/bound mismatch".into());
    }
    let raw: Value = serde_json::from_str(&export.portable_json)?;
    if serde_json::to_string(&raw)? != export.portable_json {
        return Err("portable JSON is not canonical".into());
    }
    let p: Portable = serde_json::from_value(raw)?;
    if p.persistent_id != export.avatar_id {
        return Err("portable avatar differs from export".into());
    }
    let input = &export.permission_source;
    if input.created_at_ms > input.observed_at_ms
        || input.observed_at_ms > export.expires_at
        || export.expires_at - input.observed_at_ms > 60000
        || input.roommate_lots.iter().any(|r| r.is_pending > 1)
    {
        return Err("invalid source time or membership flags".into());
    }
    let permission_source = PermissionSource {
        registered_at_ms: input.registered_at_ms,
        observed_at_ms: input.observed_at_ms,
        roommate_lots: input
            .roommate_lots
            .iter()
            .map(|r| RoommateSource {
                lot_id: r.lot_id.0,
                permissions_level: r.permissions_level,
                is_pending: r.is_pending != 0,
            })
            .collect(),
        moderation_level: input.moderation_level,
        mayor_nhood: input.mayor_nhood.map(|v| u32::try_from(v.0)).transpose()?,
    };
    let (permissions, avatar_flags) = destination_permissions(
        runtime.sim().state(),
        pid(p.persistent_id)?,
        &permission_source,
    )?;
    let mut relationships = BTreeMap::new();
    for r in p.relationships {
        if relationships.insert(pid(r.target)?, r.values).is_some() {
            return Err("duplicate relationship".into());
        }
    }
    let mut job_info = BTreeMap::new();
    for j in p.job_info {
        if job_info
            .insert(
                j.id,
                JobInfo {
                    experience: j.experience,
                    level: j.level,
                    sick_days: j.sick_days,
                    status_flags: j.status_flags,
                },
            )
            .is_some()
        {
            return Err("duplicate job".into());
        }
    }
    let mut ignored_avatars = BTreeSet::new();
    for id in p.ignored_avatars {
        if !ignored_avatars.insert(pid(id)?) {
            return Err("duplicate ignore".into());
        }
    }
    Ok(AvatarPersistState {
        name: p.name,
        persistent_id: pid(p.persistent_id)?,
        default_suits: DefaultSuits {
            daywear: OutfitReference::Id(p.default_suits.daywear.0),
            swimwear: OutfitReference::Id(p.default_suits.swimwear.0),
            sleepwear: OutfitReference::Id(p.default_suits.sleepwear.0),
        },
        permissions,
        avatar_flags,
        budget_mirror: u32::try_from(p.budget_mirror.0)?,
        body: p.body.map(|v| OutfitReference::Id(v.0)),
        head: p.head.map(|v| OutfitReference::Id(v.0)),
        skin_tone: p.skin_tone,
        is_worker: p.is_worker,
        custom_guid: p.custom_guid,
        motive_data: p.motive_data,
        person_data: p.person_data,
        relationships,
        job_info,
        ignored_avatars,
    })
}
impl Host {
    pub(crate) fn owned_control(&self, request: &OwnedControlV2) -> Result<&OwnedControl> {
        if !self.recovery_committed {
            return Err("recovery not committed".into());
        }
        let control = self
            .committed
            .sim()
            .state()
            .owned_admission
            .actors()
            .and_then(|a| a.get(&request.principal.0))
            .ok_or("owned actor not admitted")?;
        if u64::from(control.avatar_id.0) != request.avatar_id.0
            || control.control_generation != request.control_generation.0
            || control.session_ref != digest32(&request.session_ref)?
        {
            return Err("owned control binding mismatch".into());
        }
        self.committed
            .sim()
            .state()
            .owned_admission
            .validate(self.committed.sim().state())?;
        Ok(control)
    }
    pub(crate) fn player_bootstrap(
        &self,
        control: OwnedControlV2,
        binding: PlayerBindingV1,
    ) -> Result<Value> {
        let actor = self.owned_control(&control)?;
        let OwnedAdmissionState::Initialized { source, .. } =
            &self.committed.sim().state().owned_admission
        else {
            return Err("source admission uninitialized".into());
        };
        if binding.source_epoch.0 != actor.control_generation
            || binding.lot_incarnation != self.binding.lot_incarnation
            || binding.avatar_id != actor.avatar_id.0
            || binding.lot_location != source.lot_location
        {
            return Err("player binding mismatch".into());
        }
        let state = self.committed.sim().state();
        let appearance = self
            .appearance
            .clone()
            .ok_or("source appearance bundle unavailable")?;
        let packet = player::encode_bootstrap(&player::Bootstrap {
            binding: player::PlayerBinding {
                source_epoch: binding.source_epoch.0,
                lot_incarnation: binding.lot_incarnation.0,
                lot_location: binding.lot_location,
                avatar_id: binding.avatar_id,
            },
            principal: PrincipalKey(actor.principal),
            actor: actor.actor,
            content: self.committed.sim().content().clone(),
            appearance,
            lot: state.world.lot.clone(),
            mode: state.mode,
            lot_id: state.lot_id,
            authority_epoch: state.authority_epoch,
            effect_namespace: state.effects.namespace(),
            limits: state.limits.clone(),
            effect_limits: state.effects.limits(),
        })?;
        Ok(json!({"kind":"player_bootstrap","packet_hex":hex(&packet)}))
    }
    pub(crate) fn player_checkpoint(
        &self,
        control: OwnedControlV2,
        request_hex: String,
    ) -> Result<Value> {
        self.owned_control(&control)?;
        let request = live_wire::decode_checkpoint_request(&unhex(&request_hex)?)?;
        let snapshot = checkpoint(&self.committed)?;
        let state = self.committed.sim().state();
        let packet = live_wire::encode_checkpoint(
            request.id,
            wonderland_game_runtime::live_session::Checkpoint {
                bytes: &snapshot,
                completed_tick: state.completed_tick,
                state_hash: self.committed.sim().state_hash()?,
            },
            &[],
            live_wire::WireLimits::default(),
        )?;
        Ok(json!({"kind":"player_checkpoint","packet_hex":hex(&packet)}))
    }
    pub(crate) fn admission_command(
        &self,
        principal: DecimalId,
        export: &ReservedAvatarExportV2,
    ) -> Result<AcceptedCommand> {
        if export.schema_version != 2
            || export.lot_id.0 != self.committed.sim().state().lot_id
            || export.account_revision.0 == 0
            || export.source_revisions.owned_assets != 1
            || export.content_release != self.binding.binding_sha256
        {
            return Err("owned export binding mismatch".into());
        }
        for hash in [
            &export.authority_id,
            &export.content_release,
            &export.protocol_release,
            &export.source_revisions.wardrobe,
            &export.source_revisions.ledger,
            &export.source_revisions.progression,
            &export.source_revisions.permissions,
        ] {
            digest32(hash)?;
        }
        let mut value = serde_json::to_value(export)?;
        value
            .as_object_mut()
            .ok_or("export object")?
            .remove("export_digest");
        if Digest::of(&serde_json::to_vec(&value)?).as_str() != export.export_digest {
            return Err("owned export digest mismatch".into());
        }
        let portable = convert_portable(export, &self.committed)?;
        let control = OwnedControl {
            principal: principal.0,
            account_id: export.account_id.0,
            avatar_id: pid(export.avatar_id)?,
            actor: EntityRef {
                object_id: ObjectId(0),
                generation: 0,
            },
            control_generation: export.control_generation.0,
            session_ref: digest32(&export.session_ref)?,
            admission_operation: export.admission_operation.clone(),
            reservation_digest: digest32(&export.reservation_digest)?,
            export_digest: digest32(&export.export_digest)?,
        };
        control.validate()?;
        Ok(AcceptedCommand::AdmitOwnedAvatar(OwnedAvatarAdmission {
            control,
            portable,
            permission_revision: export.permission_binding.revision.0,
            permission_sha256: digest32(&export.permission_binding.sha256)?,
        }))
    }
    pub(crate) fn action_command(
        &self,
        control: &OwnedControlV2,
        bytes: &[u8],
    ) -> Result<AcceptedCommand> {
        let owned = self.owned_control(control)?;
        Ok(
            match player::admission::revalidate_action(
                &self.committed,
                PrincipalKey(owned.principal),
                owned.actor,
                bytes,
            )? {
                player::PlayerAction::Invoke(v) => AcceptedCommand::QueueInteraction(v),
                player::PlayerAction::Cancel(v) => AcceptedCommand::CancelInteraction(v),
                player::PlayerAction::Goto(v) => AcceptedCommand::SourceGoto(v),
            },
        )
    }
}

#[cfg(test)]
#[path = "player_tests.rs"]
mod tests;
