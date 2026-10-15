//! Pure TSO lot permission and join-reconciliation prerequisite.
//!
//! Source: FreeSO 4c6b3e8f5835b228723caea3c9f683c62f244f73,
//! `VMTSOLotState`, `VMNetSimJoinCmd` (66–120), `VMBuildableAreaInfo`
//! (104–168), `VMObjectQueries` (23–41), and `VMTSOObjectState`.
//!
//! These are disconnected, validated source projections, not another runtime
//! ownership store, a snapshot schema, authentication, or an admission grant.
//! An adapter must obtain authoritative lot/persistent-group data and preserve
//! source distinctions between avatar-stack use, per-part occupation and portals.
//! Existing runtime ownership/grouping fields alone do not supply that query.
//! Returned scheduler/light operations are obligations: this module never runs
//! them. Database refresh, permission-change commands, later inventory deletion,
//! and integration with replicated state remain separate prerequisites.

use crate::avatars::state::AvatarPermissions;
use crate::ids::{ObjectId, PersistentId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const COMMUNITY_CATEGORY: u8 = 11;
pub const JOB_CATEGORY: u8 = 255;
pub const DONATED: u8 = 1;
pub const OBJECT_LIMIT_EXCEEDED: u8 = 1 << 3;
pub const PENDING_ROOMMATE_DELETION: u8 = 1 << 4;
pub const OBJECT_LIMIT_THREAD_DISABLE: u8 = 1 << 5;
pub const MAX_PERMISSION_ENTRIES: usize = i16::MAX as usize;
pub const OBJECT_LIMIT_PER_PERSON: [i32; 14] = [
    75, 92, 109, 126, 143, 160, 177, 194, 211, 228, 245, 262, 279, 300,
];

/// Owner and membership identifiers are source-width values, including zero.
/// Build membership need not be a subset: join deliberately leaves an existing
/// member's build status (and orphan build entries) alone. Do not normalize it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotPermissionState {
    pub owner_id: PersistentId,
    pub property_category: u8,
    /// Source packed i32: size low byte, floor-upgrade count next byte; upper
    /// direction/reserved bits do not participate in the object-limit index.
    pub size: i32,
    pub roommates: BTreeSet<PersistentId>,
    pub build_roommates: BTreeSet<PersistentId>,
    /// Source cached values are not serialized by VMTSOLotState. A no-op join
    /// preserves even stale caches; only an actual membership change updates.
    pub object_limit: i32,
    pub limit_exceeded: bool,
    pub donate_limit: i32,
}

impl Default for LotPermissionState {
    fn default() -> Self {
        Self {
            owner_id: PersistentId(0),
            property_category: 0,
            size: 0,
            roommates: BTreeSet::new(),
            build_roommates: BTreeSet::new(),
            object_limit: 0,
            limit_exceeded: false,
            donate_limit: 2000,
        }
    }
}

impl LotPermissionState {
    pub fn is_community(&self) -> bool {
        self.property_category == COMMUNITY_CATEGORY
    }

    /// VMBuildableAreaInfo.GetObjectLimit. Special categories return before
    /// indexing; invalid normal indices are explicitly refused, never clamped.
    pub fn calculated_object_limit(&self) -> Result<i32, LotPermissionError> {
        if self.property_category == JOB_CATEGORY {
            return Ok(10000);
        }
        if self.is_community() {
            return Ok(100);
        }
        let index = ((self.size & 255) + ((self.size >> 8) & 255)) as usize;
        let per_person = OBJECT_LIMIT_PER_PERSON
            .get(index)
            .ok_or(LotPermissionError::InvalidSizeIndex(index as u16))?;
        let count = i32::try_from(self.roommates.len().max(1))
            .map_err(|_| LotPermissionError::ObjectLimitOverflow)?;
        per_person
            .checked_mul(count)
            .ok_or(LotPermissionError::ObjectLimitOverflow)
    }

    fn validate(&self) -> Result<(), LotPermissionError> {
        if self.roommates.len() > MAX_PERMISSION_ENTRIES
            || self.build_roommates.len() > MAX_PERMISSION_ENTRIES
        {
            return Err(LotPermissionError::MembershipBoundExceeded);
        }
        if self.object_limit < 0 || self.donate_limit < 0 {
            return Err(LotPermissionError::NegativeCachedLimit);
        }
        self.calculated_object_limit()?;
        Ok(())
    }
}

/// One source entity. Non-game parts must remain in registered groups: they
/// affect group counts and portal queries, but do not receive object flag or
/// ownership updates. Unregistered non-game entities may be omitted safely.
/// Owner/object/disabled metadata is ignored for non-game entities.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionObject {
    pub kind: PermissionEntityKind,
    pub persistent_id: PersistentId,
    pub owner_id: PersistentId,
    pub object_flags: u8,
    pub disabled_flags: u8,
    pub occupied: bool,
    pub portal: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionEntityKind {
    #[default]
    GameObject,
    Other,
}

/// One registered VMObjectQueries.MultitileByPersist entry. A group counts
/// once regardless of part count. Source part order is retained explicitly.
/// This bounded contract requires every part's persistent ID to equal its
/// registration key. Normal source placement/update does this; inconsistent
/// imported groups are explicitly refused, not asserted impossible in FreeSO.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedPermissionGroup {
    pub base_object: ObjectId,
    pub objects: Vec<ObjectId>,
    /// True only when an avatar thread stack has a game-object callee in this
    /// group. Queue ownership/general in_use are not interchangeable with it.
    pub in_avatar_stack: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotPermissionProjection {
    pub lot: LotPermissionState,
    /// VM.Entities is sorted by local ObjectID (VM.AddToObjList).
    pub objects: BTreeMap<ObjectId, PermissionObject>,
    /// These are registrations, not all groups found in the world. Persisted
    /// objects outside this query still participate in join's owner scan.
    pub persisted_groups: BTreeMap<PersistentId, PersistedPermissionGroup>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LotPermissionError {
    InvalidSizeIndex(u16),
    ObjectLimitOverflow,
    MembershipBoundExceeded,
    ObjectBoundExceeded,
    GroupBoundExceeded,
    NegativeCachedLimit,
    InvalidObjectId(ObjectId),
    UnsupportedObjectFlags(ObjectId),
    UnsupportedDisabledFlags(ObjectId),
    ZeroGroupPersistentId,
    EmptyGroup(PersistentId),
    MissingBaseObject(PersistentId),
    MissingGroupObject(ObjectId),
    RepeatedGroupObject(ObjectId),
    InconsistentGroupPersistentId(ObjectId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotObjectCapacity {
    pub user_objects: usize,
    pub donated_objects: usize,
    pub can_place_user_object: bool,
    pub can_place_donated_object: bool,
}

/// Ordered transition trace. Set* entries describe changes to `next` (including
/// source assignments whose value is unchanged). Required* entries are explicit
/// outstanding integration obligations, never evidence of executed effects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LotPermissionOperation {
    RequiredAvatarPermissionApplication {
        avatar_id: PersistentId,
        permissions: AvatarPermissions,
    },
    SetRoommateMembership {
        avatar_id: PersistentId,
        present: bool,
    },
    SetBuildMembership {
        avatar_id: PersistentId,
        present: bool,
    },
    SetObjectLimit(i32),
    SetDisabledFlags {
        object_id: ObjectId,
        flags: u8,
    },
    SetLimitExceeded(bool),
    RequiredSchedulerInterrupt(ObjectId),
    RequiredLightRefresh(ObjectId),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotPermissionTransition {
    pub next: LotPermissionProjection,
    pub operations: Vec<LotPermissionOperation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinPermissionTransition {
    pub effective_permissions: AvatarPermissions,
    pub membership_changed: bool,
    pub transition: LotPermissionTransition,
}

impl LotPermissionProjection {
    /// Refuses unsupported or inconsistent projections before cloning/mutation.
    /// This bounded contract deliberately does not repair unsupported registration
    /// state or infer omitted objects. ID zero in owner/membership sets is valid.
    pub fn validate(&self) -> Result<(), LotPermissionError> {
        self.lot.validate()?;
        if self.objects.len() > MAX_PERMISSION_ENTRIES {
            return Err(LotPermissionError::ObjectBoundExceeded);
        }
        if self.persisted_groups.len() > MAX_PERMISSION_ENTRIES {
            return Err(LotPermissionError::GroupBoundExceeded);
        }
        for (id, object) in &self.objects {
            if !id.is_valid() {
                return Err(LotPermissionError::InvalidObjectId(*id));
            }
            if object.kind == PermissionEntityKind::Other {
                continue;
            }
            if object.object_flags & !DONATED != 0 {
                return Err(LotPermissionError::UnsupportedObjectFlags(*id));
            }
            if object.disabled_flags & !0x7f != 0 {
                return Err(LotPermissionError::UnsupportedDisabledFlags(*id));
            }
        }
        let mut members = BTreeSet::new();
        for (pid, group) in &self.persisted_groups {
            if pid.0 == 0 {
                return Err(LotPermissionError::ZeroGroupPersistentId);
            }
            if group.objects.is_empty() {
                return Err(LotPermissionError::EmptyGroup(*pid));
            }
            if group.objects.len() > MAX_PERMISSION_ENTRIES {
                return Err(LotPermissionError::GroupBoundExceeded);
            }
            if !group.objects.contains(&group.base_object) {
                return Err(LotPermissionError::MissingBaseObject(*pid));
            }
            for id in &group.objects {
                let object = self
                    .objects
                    .get(id)
                    .ok_or(LotPermissionError::MissingGroupObject(*id))?;
                if !members.insert(*id) {
                    return Err(LotPermissionError::RepeatedGroupObject(*id));
                }
                if object.persistent_id != *pid {
                    return Err(LotPermissionError::InconsistentGroupPersistentId(*id));
                }
            }
        }
        Ok(())
    }

    fn counts_validated(&self) -> (usize, usize) {
        let donated = self
            .persisted_groups
            .values()
            .filter(|group| {
                let base = &self.objects[&group.base_object];
                base.kind == PermissionEntityKind::GameObject && base.object_flags & DONATED != 0
            })
            .count();
        let user = if self.lot.is_community() {
            self.persisted_groups.len() - donated
        } else {
            self.persisted_groups.len()
        };
        (user, donated)
    }

    /// Placement uses the source cache, not a freshly calculated limit. At the
    /// exact limit placement is denied although LimitExceeded is still false.
    pub fn placement_capacity(&self) -> Result<LotObjectCapacity, LotPermissionError> {
        self.validate()?;
        let (user_objects, donated_objects) = self.counts_validated();
        Ok(LotObjectCapacity {
            user_objects,
            donated_objects,
            can_place_user_object: user_objects < self.lot.object_limit as usize,
            can_place_donated_object: donated_objects < self.lot.donate_limit as usize,
        })
    }
}

/// Pure VMBuildableAreaInfo.UpdateOverbudgetObjects. No scheduler/light
/// obligation is added by this source operation, even when flags change.
pub fn update_overbudget(
    input: &LotPermissionProjection,
) -> Result<LotPermissionTransition, LotPermissionError> {
    input.validate()?;
    let mut result = LotPermissionTransition {
        next: input.clone(),
        operations: Vec::new(),
    };
    update_overbudget_validated(&mut result)?;
    Ok(result)
}

fn update_overbudget_validated(
    result: &mut LotPermissionTransition,
) -> Result<(), LotPermissionError> {
    let limit = result.next.lot.calculated_object_limit()?;
    result.next.lot.object_limit = limit;
    result
        .operations
        .push(LotPermissionOperation::SetObjectLimit(limit));
    // Community returns from the object-flag branch, not from the whole method:
    // caches still update, and stale limit-disable flags are NOT cleared.
    if !result.next.lot.is_community() {
        let mut groups: Vec<_> = result.next.persisted_groups.values().collect();
        groups.sort_by_key(|group| group.base_object);
        for (index, group) in groups.into_iter().enumerate() {
            let is_portal = result.next.objects[&group.base_object].kind
                == PermissionEntityKind::GameObject
                && group
                    .objects
                    .iter()
                    .any(|id| result.next.objects[id].portal);
            for id in &group.objects {
                let object = result.next.objects.get_mut(id).expect("validated group");
                if object.kind != PermissionEntityKind::GameObject {
                    continue;
                }
                if index >= limit as usize {
                    object.disabled_flags |= OBJECT_LIMIT_EXCEEDED;
                    object.disabled_flags &= !OBJECT_LIMIT_THREAD_DISABLE;
                    if !group.in_avatar_stack && !object.occupied && !is_portal {
                        object.disabled_flags |= OBJECT_LIMIT_THREAD_DISABLE;
                    }
                } else {
                    object.disabled_flags &= !(OBJECT_LIMIT_EXCEEDED | OBJECT_LIMIT_THREAD_DISABLE);
                }
                result
                    .operations
                    .push(LotPermissionOperation::SetDisabledFlags {
                        object_id: *id,
                        flags: object.disabled_flags,
                    });
            }
        }
    }
    let (user_objects, _) = result.next.counts_validated();
    result.next.lot.limit_exceeded = user_objects > limit as usize;
    result
        .operations
        .push(LotPermissionOperation::SetLimitExceeded(
            result.next.lot.limit_exceeded,
        ));
    Ok(())
}

/// VMNetSimJoinCmd's TSO permission slice, after object placement and before
/// persistent-avatar registration/chat/controller callbacks. `avatar_id` is the
/// portable AvatarState.PersistID, which Apply uses (not a separately claimed
/// ActorUID). Identity binding and the rest of Apply belong to the integrator.
///
/// Order: community role override, portable permission application obligation,
/// membership/build mutation, conditional overbudget pass, then owner-only
/// pending-deletion flags and interrupt/light obligations. OwnerID is untouched.
/// The caller's input is unchanged on both success and error.
pub fn reconcile_join(
    input: &LotPermissionProjection,
    avatar_id: PersistentId,
    incoming_permissions: AvatarPermissions,
) -> Result<JoinPermissionTransition, LotPermissionError> {
    input.validate()?;
    let mut effective_permissions = incoming_permissions;
    if input.lot.is_community()
        && incoming_permissions < AvatarPermissions::Owner
        && input.lot.roommates.contains(&avatar_id)
    {
        effective_permissions = if input.lot.build_roommates.contains(&avatar_id) {
            AvatarPermissions::BuildBuyRoommate
        } else {
            AvatarPermissions::Roommate
        };
    }
    let mut transition = LotPermissionTransition {
        next: input.clone(),
        operations: vec![
            LotPermissionOperation::RequiredAvatarPermissionApplication {
                avatar_id,
                permissions: effective_permissions,
            },
        ],
    };
    let old_count = transition.next.lot.roommates.len();
    if effective_permissions > AvatarPermissions::Visitor
        && effective_permissions < AvatarPermissions::Admin
    {
        if transition.next.lot.roommates.insert(avatar_id) {
            transition
                .operations
                .push(LotPermissionOperation::SetRoommateMembership {
                    avatar_id,
                    present: true,
                });
            let build = effective_permissions > AvatarPermissions::Roommate;
            if build {
                transition.next.lot.build_roommates.insert(avatar_id);
            } else {
                transition.next.lot.build_roommates.remove(&avatar_id);
            }
            transition
                .operations
                .push(LotPermissionOperation::SetBuildMembership {
                    avatar_id,
                    present: build,
                });
        }
    } else if effective_permissions != AvatarPermissions::Admin
        && transition.next.lot.roommates.remove(&avatar_id)
    {
        transition.next.lot.build_roommates.remove(&avatar_id);
        transition
            .operations
            .push(LotPermissionOperation::SetRoommateMembership {
                avatar_id,
                present: false,
            });
        transition
            .operations
            .push(LotPermissionOperation::SetBuildMembership {
                avatar_id,
                present: false,
            });
    }
    // A new insertion can exceed the bounded source signed-short set encoding.
    // Refuse before running a reducer; no partial result escapes on failure.
    transition.next.lot.validate()?;
    let membership_changed = old_count != transition.next.lot.roommates.len();
    if membership_changed {
        update_overbudget_validated(&mut transition)?;
        for (object_id, object) in &mut transition.next.objects {
            if object.kind != PermissionEntityKind::GameObject
                || object.persistent_id.0 == 0
                || object.owner_id != avatar_id
            {
                continue;
            }
            let old = object.disabled_flags;
            // Unlike VMChangePermissionsCmd, JOIN has no donated exemption.
            if effective_permissions < AvatarPermissions::Roommate {
                object.disabled_flags |= PENDING_ROOMMATE_DELETION;
            } else {
                object.disabled_flags &= !PENDING_ROOMMATE_DELETION;
            }
            transition
                .operations
                .push(LotPermissionOperation::SetDisabledFlags {
                    object_id: *object_id,
                    flags: object.disabled_flags,
                });
            if old != object.disabled_flags {
                transition
                    .operations
                    .push(LotPermissionOperation::RequiredSchedulerInterrupt(
                        *object_id,
                    ));
            }
            transition
                .operations
                .push(LotPermissionOperation::RequiredLightRefresh(*object_id));
        }
    }
    Ok(JoinPermissionTransition {
        effective_permissions,
        membership_changed,
        transition,
    })
}
