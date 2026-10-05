// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//! TTAB and VMThread.CheckAction source semantics, not object-specific policy.

use super::{EntityVersion, LegacyMode};

/// Raw TTAB bits remain intact, including unimplemented and dialect-overloaded bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActionFlags(pub u32);

impl ActionFlags {
    pub const ALLOW_VISITORS: u32 = 1;
    pub const JOINABLE: u32 = 1 << 1;
    pub const RUN_IMMEDIATELY: u32 = 1 << 2;
    pub const ALLOW_CONSECUTIVE: u32 = 1 << 3;
    pub const TS1_NO_CHILD: u32 = 1 << 4;
    pub const TS1_NO_DEMO_CHILD: u32 = 1 << 5;
    pub const TS1_NO_ADULT: u32 = 1 << 6;
    pub const DEBUG: u32 = 1 << 7;
    pub const AUTO_FIRST_SELECT: u32 = 1 << 8;
    pub const TS1_ALLOW_CATS: u32 = 1 << 9;
    pub const TS1_ALLOW_DOGS: u32 = 1 << 10;
    pub const LEAPFROG: u32 = 1 << 9;
    pub const MUST_RUN: u32 = 1 << 10;
    pub const ALLOW_DOGS: u32 = 1 << 11;
    pub const ALLOW_CATS: u32 = 1 << 12;
    pub const AVAILABLE_CARRYING: u32 = 1 << 16;
    pub const IS_REPAIR: u32 = 1 << 17;
    pub const RUN_CHECK_ALWAYS: u32 = 1 << 18;
    pub const AVAILABLE_WHEN_DEAD: u32 = 1 << 19;
    pub const DIRECT_CONTROL: u32 = 1 << 27;
    pub const SKIP_PERMISSIONS: u32 = 1 << 28;
    pub const PUSH_HEAD: u32 = 1 << 29;
    pub const PUSH_TAIL: u32 = 1 << 30;

    pub const fn has(self, bits: u32) -> bool {
        self.0 & bits != 0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PermissionFlags(pub u32);

impl PermissionFlags {
    pub const NON_EMPTY: u32 = 1;
    pub const OBJECT_OWNER: u32 = 1 << 1;
    pub const ROOMMATES: u32 = 1 << 2;
    pub const FRIENDS: u32 = 1 << 3;
    pub const VISITORS: u32 = 1 << 4;
    pub const GHOSTS: u32 = 1 << 5;
    pub const PARENTAL_CONTROL: u32 = 1 << 6;
    pub const CSRS: u32 = 1 << 7;

    pub const fn has(self, bits: u32) -> bool {
        self.0 & bits != 0
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum InteractionScope {
    Local,
    Global,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct InteractionKey {
    /// TTAB.TTAIndex, not its position in the table. Do not truncate to a menu byte.
    pub tta_index: u32,
    pub scope: InteractionScope,
}

/// A provider-resolved routine and the owner of its local resources.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RoutineBinding {
    pub routine_id: u16,
    pub code_owner_guid: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InteractionDefinition {
    pub key: InteractionKey,
    pub action: RoutineBinding,
    pub check: Option<RoutineBinding>,
    pub flags: ActionFlags,
    pub permissions: PermissionFlags,
    /// None means the TTAs resource is absent; Some("") is a valid empty label.
    pub label: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Species {
    Human,
    Cat,
    Dog,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[repr(u8)]
pub enum AvatarPermission {
    Visitor = 0,
    Roommate = 1,
    BuildBuyRoommate = 2,
    Owner = 3,
    Admin = 4,
}

/// Facts are captured from source VM state by the adapter, never supplied by a client.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActorFacts {
    pub version: EntityVersion,
    pub is_avatar: bool,
    pub species: Species,
    pub permission: AvatarPermission,
    pub carrying: bool,
    pub ghost: bool,
    /// Includes the donated-object mayor ownership rewrite in CheckAction.
    pub owns_target: bool,
    pub ts1_ungreeted_visitor: bool,
    pub age: i16,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TargetFacts {
    pub version: EntityVersion,
    pub is_game_object: bool,
    pub broken: bool,
    pub disabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PermissionDecision {
    Denied,
    SkipCheck,
    RunCheck,
}

pub fn permission_decision(
    mode: LegacyMode,
    actor: &ActorFacts,
    target: &TargetFacts,
    definition: &InteractionDefinition,
    autonomous: bool,
) -> PermissionDecision {
    use PermissionDecision::{Denied, RunCheck, SkipCheck};
    let flags = definition.flags;
    let skip_permissions = flags.has(ActionFlags::SKIP_PERMISSIONS);
    let pet = actor.species != Species::Human;
    if actor.is_avatar && !skip_permissions {
        match mode {
            LegacyMode::Ts1 => {
                if flags.has(ActionFlags::TS1_ALLOW_CATS | ActionFlags::TS1_ALLOW_DOGS) {
                    // Faithful to CheckTS1Action: its !IsPet rejection is commented out.
                    if (actor.species == Species::Cat && !flags.has(ActionFlags::TS1_ALLOW_CATS))
                        || (actor.species == Species::Dog
                            && !flags.has(ActionFlags::TS1_ALLOW_DOGS))
                    {
                        return Denied;
                    }
                } else if pet {
                    return Denied;
                }
                // CheckTS1Action hardcodes debugTrees = false.
                if flags.has(ActionFlags::DEBUG)
                    || (actor.age < 18 && flags.has(ActionFlags::TS1_NO_CHILD))
                    || (actor.age >= 18 && !pet && flags.has(ActionFlags::TS1_NO_ADULT))
                    || (actor.ts1_ungreeted_visitor && !flags.has(ActionFlags::ALLOW_VISITORS))
                {
                    return Denied;
                }
            }
            LegacyMode::Tso => {
                if actor.carrying && !flags.has(ActionFlags::AVAILABLE_CARRYING) {
                    return Denied;
                }
                if flags.has(ActionFlags::ALLOW_CATS | ActionFlags::ALLOW_DOGS) {
                    if !pet
                        || (actor.species == Species::Cat && !flags.has(ActionFlags::ALLOW_CATS))
                        || (actor.species == Species::Dog && !flags.has(ActionFlags::ALLOW_DOGS))
                    {
                        return Denied;
                    }
                } else if pet && (actor.permission < AvatarPermission::Admin || autonomous) {
                    return Denied;
                }
                let repair = flags.has(ActionFlags::IS_REPAIR);
                let global = definition.action.routine_id < 4096;
                if (!global || repair) && repair != target.broken {
                    return Denied;
                }
                if flags.has(ActionFlags::DEBUG) {
                    return if actor.permission == AvatarPermission::Admin {
                        SkipCheck
                    } else {
                        Denied
                    };
                }
                let mut compare = definition.permissions.0;
                if compare == PermissionFlags::NON_EMPTY {
                    compare |= PermissionFlags::FRIENDS
                        | PermissionFlags::ROOMMATES
                        | PermissionFlags::OBJECT_OWNER;
                }
                if flags.has(ActionFlags::AVAILABLE_WHEN_DEAD) {
                    compare |= PermissionFlags::GHOSTS;
                }
                if flags.has(ActionFlags::ALLOW_VISITORS) {
                    compare |= PermissionFlags::VISITORS;
                }
                let allowed = PermissionFlags(compare);
                // The executable source has a positive owner override, despite the
                // negative-owner wording in its preceding comments.
                let owner_override = (actor.owns_target || !target.is_game_object)
                    && allowed.has(PermissionFlags::OBJECT_OWNER);
                if !owner_override
                    && ((actor.permission == AvatarPermission::Visitor
                        && !allowed.has(PermissionFlags::VISITORS))
                        || (actor.permission >= AvatarPermission::Roommate
                            && !allowed.has(PermissionFlags::ROOMMATES))
                        || (actor.ghost && !allowed.has(PermissionFlags::GHOSTS))
                        || (allowed.has(PermissionFlags::CSRS)
                            && actor.permission != AvatarPermission::Admin))
                {
                    return Denied;
                }
                // FRIENDS and PARENTAL_CONTROL are retained bits, not invented checks.
            }
        }
    }
    if definition.check.is_some()
        && (mode == LegacyMode::Ts1
            || !skip_permissions
            || flags.has(ActionFlags::RUN_CHECK_ALWAYS))
    {
        RunCheck
    } else {
        SkipCheck
    }
}
