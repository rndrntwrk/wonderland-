//! Trusted source join facts and replicated ownership. None of these values is a
//! browser credential. The importer must supply provenance-bound native facts;
//! absent facts never authorize an avatar or imply empty persistent queries.
use crate::{
    avatars::state::{AvatarPermissions, AvatarPersistState},
    ids::{EntityRef, PersistentId},
    lot_permissions::ReplicatedLotPermissions,
    state::{ContentDescriptor, SimState},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_OWNED_ACTORS: usize = 256;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceJoinMetadata {
    pub source_import_sha256: [u8; 32],
    pub content: ContentDescriptor,
    pub neighborhood_id: u32,
    /// Source packed city location, distinct from the database lot ID.
    pub lot_location: u32,
    /// Source CreateObjectInstance archetype classification, supplied by import.
    pub avatar_guids: BTreeSet<u32>,
    /// Exact effective source tuning comparison, including confirmed absence.
    pub april_fools_2019: bool,
    pub object_flags: BTreeMap<EntityRef, u8>,
    /// Exact query registration, not inferred from all persistent entities.
    pub registered_groups: BTreeMap<PersistentId, Vec<EntityRef>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedControl {
    pub principal: u64,
    pub account_id: u64,
    pub avatar_id: PersistentId,
    pub actor: EntityRef,
    pub control_generation: u64,
    pub session_ref: [u8; 32],
    pub admission_operation: String,
    pub reservation_digest: [u8; 32],
    pub export_digest: [u8; 32],
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OwnedAdmissionState {
    #[default]
    Uninitialized,
    Initialized {
        source: Box<SourceJoinMetadata>,
        revision: u64,
        permission_revision: u64,
        /// Monotonic high-water mark; principals are never reused after deletion.
        last_principal: u64,
        actors: BTreeMap<u64, OwnedControl>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedAvatarAdmission {
    pub control: OwnedControl,
    pub portable: AvatarPersistState,
    pub permission_revision: u64,
    pub permission_sha256: [u8; 32],
}
impl OwnedControl {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.principal == 0
            || self.account_id == 0
            || self.avatar_id.0 == 0
            || self.avatar_id.0 > 0x00ff_ffff
            || self.control_generation == 0
            || self.session_ref == [0; 32]
            || self.reservation_digest == [0; 32]
            || self.export_digest == [0; 32]
            || !(16..=96).contains(&self.admission_operation.len())
            || !self
                .admission_operation
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err("invalid owned control identity");
        }
        Ok(())
    }
}
impl OwnedAdmissionState {
    pub fn actors(&self) -> Option<&BTreeMap<u64, OwnedControl>> {
        match self {
            Self::Initialized { actors, .. } => Some(actors),
            _ => None,
        }
    }
    pub fn revision(&self) -> u64 {
        match self {
            Self::Initialized { revision, .. } => *revision,
            _ => 0,
        }
    }
    pub fn validate(&self, state: &SimState) -> Result<(), &'static str> {
        let Self::Initialized {
            source,
            revision,
            permission_revision,
            last_principal,
            actors,
        } = self
        else {
            return Ok(());
        };
        if state.mode != crate::vm::VmMode::Tso
            || !matches!(
                state.lot_permissions,
                ReplicatedLotPermissions::Initialized(_)
            )
            || source.content != state.content
            || source.source_import_sha256 == [0; 32]
            || *revision == 0
            || *permission_revision == 0
            || actors.len() > MAX_OWNED_ACTORS
            || source.object_flags.len() > 32767
            || source.registered_groups.len() > 32767
            || source.avatar_guids.is_empty()
            || source.avatar_guids.len() > 32767
            || source.avatar_guids.contains(&0)
        {
            return Err("invalid source admission metadata");
        }
        if source
            .object_flags
            .iter()
            .any(|(r, f)| !r.is_valid() || f & !1 != 0)
        {
            return Err("invalid donation metadata");
        }
        for entity in state.entities.values().filter(|e| !e.info.is_avatar) {
            if !source.object_flags.contains_key(&entity.info.reference)
                || state
                    .world
                    .object(entity.info.reference)
                    .map_or(true, |o| o.owner.is_none())
            {
                return Err("source object metadata unavailable");
            }
        }
        for reference in source.object_flags.keys() {
            let entity = state
                .entities
                .get(&reference.object_id)
                .ok_or("donation object missing")?;
            if !state.ids.is_live(*reference)
                || entity.info.reference != *reference
                || entity.info.is_avatar
            {
                return Err("donation object identity mismatch");
            }
        }
        let mut refs = BTreeSet::new();
        for (pid, group) in &source.registered_groups {
            if pid.0 == 0
                || group.is_empty()
                || group.len() > 32767
                || group.iter().any(|r| !r.is_valid() || !refs.insert(*r))
            {
                return Err("invalid source persistent registration");
            }
        }
        for (pid, group) in &source.registered_groups {
            let first = state
                .entities
                .get(&group[0].object_id)
                .ok_or("registered group missing")?;
            let supplied: BTreeSet<_> = group.iter().map(|r| r.object_id).collect();
            if first.info.group.iter().copied().collect::<BTreeSet<_>>() != supplied {
                return Err("registered group differs from native group");
            }
            for reference in group {
                let entity = state
                    .entities
                    .get(&reference.object_id)
                    .ok_or("registered part missing")?;
                if !state.ids.is_live(*reference)
                    || entity.info.reference != *reference
                    || entity.info.persistent_id != pid.0
                    || entity.info.base_object != first.info.base_object
                    || entity.info.group != first.info.group
                {
                    return Err("registered persistent group identity mismatch");
                }
            }
        }
        let mut avatars = BTreeSet::new();
        for (principal, control) in actors {
            control.validate()?;
            if *principal != control.principal
                || *principal > *last_principal
                || !avatars.insert(control.avatar_id)
            {
                return Err("duplicate owned actor binding");
            }
            let entity = state
                .entities
                .get(&control.actor.object_id)
                .ok_or("owned actor missing")?;
            if !state.ids.is_live(control.actor)
                || entity.info.reference != control.actor
                || entity.info.persistent_id != control.avatar_id.0
                || entity
                    .avatar
                    .as_ref()
                    .map_or(true, |a| a.persistent_id != control.avatar_id)
            {
                return Err("owned actor identity mismatch");
            }
            if state
                .interaction_access
                .get(&control.actor)
                .map_or(true, |a| {
                    a.principal.0 != control.principal || a.allow_hidden
                })
            {
                return Err("owned actor authority mismatch");
            }
        }
        Ok(())
    }
}
/// Opaque binding to exact initialized replicated lot policy. The observed tick
/// fences even equal policy values against a different committed predecessor.
pub fn permission_digest(state: &SimState) -> Result<[u8; 32], String> {
    if !matches!(
        state.lot_permissions,
        ReplicatedLotPermissions::Initialized(_)
    ) {
        return Err("lot permissions uninitialized".into());
    }
    Ok(crate::state::hash(&crate::state::canonical_bytes(
        &state.lot_permissions,
        1024 * 1024,
    )?))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoommateSource {
    pub lot_id: u64,
    pub permissions_level: u8,
    pub is_pending: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionSource {
    pub registered_at_ms: u64,
    pub observed_at_ms: u64,
    pub roommate_lots: Vec<RoommateSource>,
    pub moderation_level: u8,
    pub mayor_nhood: Option<u32>,
}
/// LotContainer.StateFromDB, followed by community promotion in VMNetSimJoinCmd.
/// An explicit empty DB result is valid; a missing source record cannot call this.
pub fn destination_permissions(
    state: &SimState,
    avatar: PersistentId,
    source: &PermissionSource,
) -> Result<(AvatarPermissions, u32), &'static str> {
    let ReplicatedLotPermissions::Initialized(lot) = &state.lot_permissions else {
        return Err("lot permissions uninitialized");
    };
    let OwnedAdmissionState::Initialized {
        source: metadata, ..
    } = &state.owned_admission
    else {
        return Err("source admission uninitialized");
    };
    if source.observed_at_ms < source.registered_at_ms
        || source.roommate_lots.len() > 32767
        || source
            .roommate_lots
            .iter()
            .any(|r| r.lot_id == 0 || r.permissions_level > 2)
    {
        return Err("invalid permission source");
    }
    let mut seen = BTreeSet::new();
    if source.roommate_lots.iter().any(|r| !seen.insert(r.lot_id)) {
        return Err("duplicate source roommate lot");
    }
    let mut flags = 0;
    if source.mayor_nhood == Some(metadata.neighborhood_id) {
        flags |= 4;
    }
    if source.roommate_lots.is_empty() && !lot.is_community() {
        flags |= 1;
    }
    if (source.observed_at_ms / 1000 - source.registered_at_ms / 1000) / 86400 < 7 {
        flags |= 2;
    }
    let mut role = if lot.is_community() {
        if lot.owner_id == avatar {
            AvatarPermissions::Owner
        } else {
            AvatarPermissions::Visitor
        }
    } else if let Some(member) = source
        .roommate_lots
        .iter()
        .find(|r| r.lot_id == state.lot_id && !r.is_pending)
    {
        match member.permissions_level {
            0 => AvatarPermissions::Roommate,
            1 => AvatarPermissions::BuildBuyRoommate,
            2 => AvatarPermissions::Owner,
            _ => unreachable!(),
        }
    } else {
        AvatarPermissions::Visitor
    };
    if source.moderation_level > 0 {
        role = AvatarPermissions::Admin;
    }
    Ok((role, flags))
}
