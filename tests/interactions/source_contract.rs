// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//! Synthetic source-contract fixtures. These do not run an original object or BHAV.
use wonderland_interactions_check::*;

fn actor() -> ActorFacts {
    ActorFacts {
        version: EntityVersion {
            key: EntityKey {
                slot: 1,
                generation: 4,
            },
            revision: 10,
        },
        is_avatar: true,
        species: Species::Human,
        permission: AvatarPermission::Visitor,
        carrying: false,
        ghost: false,
        owns_target: false,
        ts1_ungreeted_visitor: true,
        age: 25,
    }
}

fn target() -> TargetFacts {
    TargetFacts {
        version: EntityVersion {
            key: EntityKey {
                slot: 2,
                generation: 9,
            },
            revision: 20,
        },
        is_game_object: true,
        broken: false,
        disabled: false,
    }
}

fn definition(flags: u32, permissions: u32) -> InteractionDefinition {
    InteractionDefinition {
        key: InteractionKey {
            tta_index: 300,
            scope: InteractionScope::Local,
        },
        action: RoutineBinding {
            routine_id: 4096,
            code_owner_guid: 0x12345678,
        },
        check: Some(RoutineBinding {
            routine_id: 4097,
            code_owner_guid: 0x12345678,
        }),
        flags: ActionFlags(flags),
        permissions: PermissionFlags(permissions),
        label: Some("Fixture action".into()),
    }
}

#[test]
fn nonempty_defaults_exclude_visitors_but_allow_roommates() {
    let mut actor = actor();
    let action = definition(0, PermissionFlags::NON_EMPTY);
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor, &target(), &action, false),
        PermissionDecision::Denied
    );
    actor.permission = AvatarPermission::Roommate;
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor, &target(), &action, false),
        PermissionDecision::RunCheck
    );
}

#[test]
fn skip_permissions_checks_only_when_tso_run_check_always_is_set() {
    let mut action = definition(ActionFlags::SKIP_PERMISSIONS, 0);
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor(), &target(), &action, false),
        PermissionDecision::SkipCheck
    );
    action.flags.0 |= ActionFlags::RUN_CHECK_ALWAYS;
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor(), &target(), &action, false),
        PermissionDecision::RunCheck
    );
    action.flags.0 &= !ActionFlags::RUN_CHECK_ALWAYS;
    assert_eq!(
        permission_decision(LegacyMode::Ts1, &actor(), &target(), &action, false),
        PermissionDecision::RunCheck
    );
}

#[test]
fn tso_owner_positive_override_does_not_bypass_carrying_or_repair() {
    let mut actor = actor();
    actor.owns_target = true;
    actor.ghost = true;
    let action = definition(0, PermissionFlags::OBJECT_OWNER);
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor, &target(), &action, false),
        PermissionDecision::RunCheck
    );
    actor.carrying = true;
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor, &target(), &action, false),
        PermissionDecision::Denied
    );
    actor.carrying = false;
    let mut broken = target();
    broken.broken = true;
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor, &broken, &action, false),
        PermissionDecision::Denied
    );
}

#[test]
fn tso_global_repair_exception_and_repair_flag_match_executable_source() {
    let actor = actor();
    let mut broken = target();
    broken.broken = true;
    let mut action = definition(0, PermissionFlags::VISITORS);
    action.action.routine_id = 100;
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor, &broken, &action, false),
        PermissionDecision::RunCheck
    );
    action.flags.0 |= ActionFlags::IS_REPAIR;
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor, &broken, &action, false),
        PermissionDecision::RunCheck
    );
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor, &target(), &action, false),
        PermissionDecision::Denied
    );
}

#[test]
fn tso_debug_skips_check_for_admin_but_csr_flag_restricts_nonowner_nonadmin() {
    let mut actor = actor();
    let action = definition(ActionFlags::DEBUG, 0);
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor, &target(), &action, false),
        PermissionDecision::Denied
    );
    actor.permission = AvatarPermission::Admin;
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor, &target(), &action, false),
        PermissionDecision::SkipCheck
    );
    let action = definition(
        0,
        PermissionFlags::VISITORS | PermissionFlags::ROOMMATES | PermissionFlags::CSRS,
    );
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor, &target(), &action, false),
        PermissionDecision::RunCheck
    );
    actor.permission = AvatarPermission::Visitor;
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor, &target(), &action, false),
        PermissionDecision::Denied
    );
}

#[test]
fn pet_permissions_preserve_tso_admin_exception_and_ts1_human_pet_flag_quirk() {
    let mut actor = actor();
    let mut action = definition(
        ActionFlags::ALLOW_CATS,
        PermissionFlags::VISITORS | PermissionFlags::ROOMMATES,
    );
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor, &target(), &action, false),
        PermissionDecision::Denied
    );
    actor.species = Species::Cat;
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor, &target(), &action, false),
        PermissionDecision::RunCheck
    );
    action.flags.0 = 0;
    actor.permission = AvatarPermission::Admin;
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor, &target(), &action, false),
        PermissionDecision::RunCheck
    );
    assert_eq!(
        permission_decision(LegacyMode::Tso, &actor, &target(), &action, true),
        PermissionDecision::Denied
    );
    actor.species = Species::Human;
    action.flags.0 = ActionFlags::TS1_ALLOW_CATS | ActionFlags::ALLOW_VISITORS;
    assert_eq!(
        permission_decision(LegacyMode::Ts1, &actor, &target(), &action, false),
        PermissionDecision::RunCheck
    );
    actor.species = Species::Dog;
    assert_eq!(
        permission_decision(LegacyMode::Ts1, &actor, &target(), &action, false),
        PermissionDecision::Denied
    );
}

#[test]
fn ts1_age_visitor_and_debug_rules_ignore_tso_permission_flags() {
    let mut actor = actor();
    let mut action = definition(
        ActionFlags::ALLOW_VISITORS | ActionFlags::TS1_NO_CHILD,
        u32::MAX,
    );
    actor.age = 10;
    assert_eq!(
        permission_decision(LegacyMode::Ts1, &actor, &target(), &action, false),
        PermissionDecision::Denied
    );
    actor.age = 25;
    assert_eq!(
        permission_decision(LegacyMode::Ts1, &actor, &target(), &action, false),
        PermissionDecision::RunCheck
    );
    action.flags.0 = 0;
    assert_eq!(
        permission_decision(LegacyMode::Ts1, &actor, &target(), &action, false),
        PermissionDecision::Denied
    );
    actor.ts1_ungreeted_visitor = false;
    action.flags.0 = ActionFlags::DEBUG;
    actor.permission = AvatarPermission::Admin;
    assert_eq!(
        permission_decision(LegacyMode::Ts1, &actor, &target(), &action, false),
        PermissionDecision::Denied
    );
}
