//! Executable cases derived from pinned FreeSO source; not original-assembly
//! differential execution and not an installed runtime admission feature.
use sim_core::avatars::state::AvatarPermissions;
use sim_core::ids::{ObjectId, PersistentId};
use sim_core::lot_permissions::*;
use std::collections::BTreeSet;

const AVATAR: PersistentId = PersistentId(42);
const OTHER: PersistentId = PersistentId(7);

fn roles() -> [AvatarPermissions; 5] {
    [
        AvatarPermissions::Visitor,
        AvatarPermissions::Roommate,
        AvatarPermissions::BuildBuyRoommate,
        AvatarPermissions::Owner,
        AvatarPermissions::Admin,
    ]
}

fn projection() -> LotPermissionProjection {
    LotPermissionProjection {
        lot: LotPermissionState {
            owner_id: OTHER,
            object_limit: 77, // Deliberately stale until source recomputation.
            limit_exceeded: true,
            ..LotPermissionState::default()
        },
        ..LotPermissionProjection::default()
    }
}

fn add_group(input: &mut LotPermissionProjection, pid: u32, parts: &[i16]) {
    for id in parts {
        input.objects.insert(
            ObjectId(*id),
            PermissionObject {
                persistent_id: PersistentId(pid),
                owner_id: AVATAR,
                ..PermissionObject::default()
            },
        );
    }
    input.persisted_groups.insert(
        PersistentId(pid),
        PersistedPermissionGroup {
            base_object: ObjectId(parts[0]),
            objects: parts.iter().copied().map(ObjectId).collect(),
            in_avatar_stack: false,
        },
    );
}

fn add_normal_groups(input: &mut LotPermissionProjection, count: i16) {
    for id in 1..=count {
        // Persist order intentionally opposes source base-object order.
        add_group(input, 10000 - id as u32, &[id]);
    }
}

fn required_effects(operations: &[LotPermissionOperation]) -> Vec<LotPermissionOperation> {
    operations
        .iter()
        .filter(|op| {
            matches!(
                op,
                LotPermissionOperation::RequiredSchedulerInterrupt(_)
                    | LotPermissionOperation::RequiredLightRefresh(_)
            )
        })
        .cloned()
        .collect()
}

#[test]
fn all_fourteen_limits_use_size_plus_floor_upgrade_index_and_full_roommate_count() {
    let mut lot = LotPermissionState::default();
    for (index, expected) in OBJECT_LIMIT_PER_PERSON.into_iter().enumerate() {
        for floors in 0..=index {
            lot.size = ((floors as i32) << 8) | (index - floors) as i32 | (3 << 16);
            for count in [0, 1, 3, 9] {
                lot.roommates = (0..count).map(PersistentId).collect();
                assert_eq!(
                    lot.calculated_object_limit().unwrap(),
                    expected * count.max(1) as i32
                );
            }
        }
    }
    lot.size = 14;
    assert_eq!(
        lot.calculated_object_limit(),
        Err(LotPermissionError::InvalidSizeIndex(14))
    );
    lot.size = -1;
    assert_eq!(
        lot.calculated_object_limit(),
        Err(LotPermissionError::InvalidSizeIndex(510))
    );
    lot.property_category = COMMUNITY_CATEGORY;
    assert_eq!(lot.calculated_object_limit(), Ok(100));
    lot.property_category = JOB_CATEGORY;
    assert_eq!(lot.calculated_object_limit(), Ok(10000));
    lot.property_category = 0;
    lot.size = -65536;
    assert_eq!(lot.calculated_object_limit(), Ok(75 * 9));
}

#[test]
fn residential_join_covers_every_role_and_preserves_existing_build_membership_quirks() {
    for incoming in roles() {
        for roommate in [false, true] {
            for builder in [false, true] {
                let mut input = projection();
                if roommate {
                    input.lot.roommates.insert(AVATAR);
                }
                if builder {
                    input.lot.build_roommates.insert(AVATAR);
                }
                let before = input.clone();
                let result = reconcile_join(&input, AVATAR, incoming).unwrap();
                assert_eq!(input, before);
                assert_eq!(result.effective_permissions, incoming);
                let next = &result.transition.next.lot;
                assert_eq!(next.owner_id, OTHER); // Even Owner never assigns OwnerID.
                let is_member = match incoming {
                    AvatarPermissions::Visitor => false,
                    AvatarPermissions::Admin => roommate,
                    _ => true,
                };
                let is_builder = match incoming {
                    AvatarPermissions::Visitor if roommate => false,
                    AvatarPermissions::Roommate if !roommate => false,
                    AvatarPermissions::BuildBuyRoommate | AvatarPermissions::Owner if !roommate => {
                        true
                    }
                    _ => builder,
                };
                assert_eq!(next.roommates.contains(&AVATAR), is_member);
                assert_eq!(next.build_roommates.contains(&AVATAR), is_builder);
                assert_eq!(result.membership_changed, roommate != is_member);
                if !result.membership_changed {
                    assert_eq!(result.transition.next, input);
                    assert_eq!(result.transition.operations.len(), 1);
                    assert_eq!(next.object_limit, 77);
                    assert!(next.limit_exceeded);
                } else {
                    assert_eq!(next.object_limit, 75);
                    assert!(!next.limit_exceeded);
                }
            }
        }
    }
}

#[test]
fn community_matrix_promotes_donors_and_benefactors_and_can_demote_incoming_builder() {
    for incoming in roles() {
        for donor in [false, true] {
            for benefactor in [false, true] {
                let mut input = projection();
                input.lot.property_category = COMMUNITY_CATEGORY;
                if donor {
                    input.lot.roommates.insert(AVATAR);
                }
                if benefactor {
                    input.lot.build_roommates.insert(AVATAR);
                }
                let result = reconcile_join(&input, AVATAR, incoming).unwrap();
                let effective = if incoming < AvatarPermissions::Owner && donor {
                    if benefactor {
                        AvatarPermissions::BuildBuyRoommate
                    } else {
                        AvatarPermissions::Roommate
                    }
                } else {
                    incoming
                };
                assert_eq!(result.effective_permissions, effective);
                assert_eq!(result.transition.next.lot.owner_id, OTHER);
                assert_eq!(
                    result.transition.operations[0],
                    LotPermissionOperation::RequiredAvatarPermissionApplication {
                        avatar_id: AVATAR,
                        permissions: effective,
                    }
                );
                if donor
                    || matches!(
                        incoming,
                        AvatarPermissions::Visitor | AvatarPermissions::Admin
                    )
                {
                    assert!(!result.membership_changed);
                    assert_eq!(result.transition.next, input);
                } else {
                    assert!(result.membership_changed);
                    assert!(result.transition.next.lot.roommates.contains(&AVATAR));
                    assert_eq!(result.transition.next.lot.object_limit, 100);
                }
            }
        }
    }
}

#[test]
fn zero_owner_membership_and_portable_join_identifier_remain_source_representable() {
    let mut input = projection();
    input.lot.owner_id = PersistentId(0);
    input.lot.roommates.insert(PersistentId(0));
    input.lot.build_roommates.insert(PersistentId(0));
    input.validate().unwrap();
    let joined = reconcile_join(&input, PersistentId(0), AvatarPermissions::Owner).unwrap();
    assert_eq!(joined.transition.next, input);
    let visitor = reconcile_join(&input, PersistentId(0), AvatarPermissions::Visitor).unwrap();
    assert!(visitor.membership_changed);
    assert_eq!(visitor.transition.next.lot.owner_id, PersistentId(0));
    assert!(visitor.transition.next.lot.roommates.is_empty());
}

#[test]
fn counts_are_registered_groups_and_community_donation_uses_only_base_flag() {
    let mut input = projection();
    add_group(&mut input, 1, &[1, 2]);
    add_group(&mut input, 2, &[3, 4]);
    input.objects.get_mut(&ObjectId(1)).unwrap().object_flags = DONATED;
    input.objects.get_mut(&ObjectId(4)).unwrap().object_flags = DONATED;
    input.objects.insert(
        ObjectId(5),
        PermissionObject {
            persistent_id: PersistentId(5),
            object_flags: DONATED,
            ..PermissionObject::default()
        },
    ); // Persisted but unregistered: no capacity count.
    input.lot.object_limit = 2;
    input.lot.donate_limit = 1;
    let capacity = input.placement_capacity().unwrap();
    assert_eq!(capacity.user_objects, 2);
    assert_eq!(capacity.donated_objects, 1);
    assert!(!capacity.can_place_user_object);
    assert!(!capacity.can_place_donated_object);
    input.lot.property_category = COMMUNITY_CATEGORY;
    let capacity = input.placement_capacity().unwrap();
    assert_eq!(capacity.user_objects, 1);
    assert_eq!(capacity.donated_objects, 1);
    assert!(capacity.can_place_user_object);
    assert!(!capacity.can_place_donated_object);
}

#[test]
fn mixed_groups_keep_non_game_members_base_counting_and_source_portal_guard() {
    for base_is_game in [false, true] {
        let mut input = projection();
        add_normal_groups(&mut input, 75);
        add_group(&mut input, 1, &[100, 101, 102]);
        let base = input.objects.get_mut(&ObjectId(100)).unwrap();
        base.kind = if base_is_game {
            PermissionEntityKind::GameObject
        } else {
            PermissionEntityKind::Other
        };
        base.object_flags = DONATED;
        base.disabled_flags = PENDING_ROOMMATE_DELETION;
        let portal = input.objects.get_mut(&ObjectId(102)).unwrap();
        portal.kind = PermissionEntityKind::Other;
        portal.portal = true;
        portal.disabled_flags = 255; // Non-game object metadata has no meaning.
        let result = update_overbudget(&input).unwrap();
        assert!(result.next.lot.limit_exceeded); // Mixed group is counted once.
        assert_eq!(
            result.next.objects[&ObjectId(102)],
            input.objects[&ObjectId(102)]
        );
        assert_eq!(
            result.next.objects[&ObjectId(101)].disabled_flags,
            OBJECT_LIMIT_EXCEEDED
                | if base_is_game {
                    0
                } else {
                    OBJECT_LIMIT_THREAD_DISABLE
                }
        );
        if !base_is_game {
            assert_eq!(
                result.next.objects[&ObjectId(100)],
                input.objects[&ObjectId(100)]
            );
        }
        input.lot.property_category = COMMUNITY_CATEGORY;
        let counts = input.placement_capacity().unwrap();
        assert_eq!(counts.donated_objects, usize::from(base_is_game));
        assert_eq!(counts.user_objects, 76 - usize::from(base_is_game));
        input.lot.roommates.insert(AVATAR);
        input.lot.property_category = 0;
        let joined = reconcile_join(&input, AVATAR, AvatarPermissions::Visitor).unwrap();
        assert_eq!(
            joined.transition.next.objects[&ObjectId(102)],
            input.objects[&ObjectId(102)]
        );
        assert!(!required_effects(&joined.transition.operations)
            .contains(&LotPermissionOperation::RequiredLightRefresh(ObjectId(102))));
        if !base_is_game {
            assert_eq!(
                joined.transition.next.objects[&ObjectId(100)],
                input.objects[&ObjectId(100)]
            );
            assert!(!required_effects(&joined.transition.operations)
                .contains(&LotPermissionOperation::RequiredLightRefresh(ObjectId(100))));
        }
    }
}

#[test]
fn entirely_non_game_registered_group_still_counts_and_receives_no_object_effects() {
    let mut input = projection();
    add_normal_groups(&mut input, 75);
    add_group(&mut input, 1, &[100, 101]);
    for id in [100, 101] {
        let object = input.objects.get_mut(&ObjectId(id)).unwrap();
        object.kind = PermissionEntityKind::Other;
        object.object_flags = DONATED;
        object.disabled_flags = 255;
    }
    let result = update_overbudget(&input).unwrap();
    assert!(result.next.lot.limit_exceeded);
    for id in [100, 101] {
        assert_eq!(
            result.next.objects[&ObjectId(id)],
            input.objects[&ObjectId(id)]
        );
        assert!(!result.operations.iter().any(|op| matches!(op,
            LotPermissionOperation::SetDisabledFlags { object_id, .. } if *object_id == ObjectId(id))));
    }
    input.lot.property_category = COMMUNITY_CATEGORY;
    let counts = input.placement_capacity().unwrap();
    assert_eq!(counts.user_objects, 76);
    assert_eq!(counts.donated_objects, 0);
}

#[test]
fn exactly_at_cap_disallows_placement_without_marking_lot_over_limit() {
    let mut input = projection();
    add_normal_groups(&mut input, 75);
    let result = update_overbudget(&input).unwrap();
    assert_eq!(result.next.lot.object_limit, 75);
    assert!(!result.next.lot.limit_exceeded);
    assert!(
        !result
            .next
            .placement_capacity()
            .unwrap()
            .can_place_user_object
    );
    assert!(required_effects(&result.operations).is_empty());
    add_group(&mut input, 76, &[76]);
    let result = update_overbudget(&input).unwrap();
    assert!(result.next.lot.limit_exceeded);
    assert_eq!(
        result.next.objects[&ObjectId(76)].disabled_flags,
        OBJECT_LIMIT_EXCEEDED | OBJECT_LIMIT_THREAD_DISABLE
    );
}

#[test]
fn budget_orders_by_base_object_and_preserves_source_stack_occupied_portal_distinctions() {
    for in_stack in [false, true] {
        for portal in [false, true] {
            let mut input = projection();
            add_normal_groups(&mut input, 75);
            add_group(&mut input, 1, &[100, 101]);
            let group = input.persisted_groups.get_mut(&PersistentId(1)).unwrap();
            group.objects = vec![ObjectId(101), ObjectId(100)];
            group.in_avatar_stack = in_stack;
            for object in input.objects.values_mut() {
                object.disabled_flags = 0x7f;
            }
            input.objects.get_mut(&ObjectId(101)).unwrap().occupied = true;
            input.objects.get_mut(&ObjectId(101)).unwrap().portal = portal;
            let result = update_overbudget(&input).unwrap();
            let retained = 0x7f & !(OBJECT_LIMIT_EXCEEDED | OBJECT_LIMIT_THREAD_DISABLE);
            for id in 1..=75 {
                assert_eq!(result.next.objects[&ObjectId(id)].disabled_flags, retained);
            }
            let tail = retained | OBJECT_LIMIT_EXCEEDED;
            assert_eq!(result.next.objects[&ObjectId(101)].disabled_flags, tail);
            assert_eq!(
                result.next.objects[&ObjectId(100)].disabled_flags,
                tail | if in_stack || portal {
                    0
                } else {
                    OBJECT_LIMIT_THREAD_DISABLE
                }
            );
            assert!(required_effects(&result.operations).is_empty());
            let assignments: Vec<_> = result
                .operations
                .iter()
                .filter_map(|op| {
                    if let LotPermissionOperation::SetDisabledFlags { object_id, .. } = op {
                        Some(*object_id)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(
                &assignments[..75],
                &(1..=75).map(ObjectId).collect::<Vec<_>>()
            );
            assert_eq!(&assignments[75..], &[ObjectId(101), ObjectId(100)]);
            assert_eq!(
                result.operations.first(),
                Some(&LotPermissionOperation::SetObjectLimit(75))
            );
            assert_eq!(
                result.operations.last(),
                Some(&LotPermissionOperation::SetLimitExceeded(true))
            );
        }
    }
}

#[test]
fn community_overbudget_updates_caches_but_never_changes_stale_disable_flags() {
    let mut input = projection();
    input.lot.property_category = COMMUNITY_CATEGORY;
    input.lot.size = -1; // Special branch precedes ordinary index validation.
    add_normal_groups(&mut input, 102);
    input.objects.get_mut(&ObjectId(1)).unwrap().object_flags = DONATED;
    for object in input.objects.values_mut() {
        object.disabled_flags = OBJECT_LIMIT_EXCEEDED | OBJECT_LIMIT_THREAD_DISABLE;
    }
    let result = update_overbudget(&input).unwrap();
    assert_eq!(result.next.objects, input.objects);
    assert_eq!(result.next.lot.object_limit, 100);
    assert!(result.next.lot.limit_exceeded); // 101 non-donated groups.
    assert_eq!(
        result.operations,
        vec![
            LotPermissionOperation::SetObjectLimit(100),
            LotPermissionOperation::SetLimitExceeded(true),
        ]
    );
    input.objects.get_mut(&ObjectId(2)).unwrap().object_flags = DONATED;
    let result = update_overbudget(&input).unwrap();
    assert!(!result.next.lot.limit_exceeded);
    assert!(
        !result
            .next
            .placement_capacity()
            .unwrap()
            .can_place_user_object
    );
}

#[test]
fn job_lot_uses_ten_thousand_limit_but_still_runs_normal_object_flag_reconciliation() {
    let mut input = projection();
    input.lot.property_category = JOB_CATEGORY;
    input.lot.size = -1;
    add_group(&mut input, 1, &[1]);
    let object = input.objects.get_mut(&ObjectId(1)).unwrap();
    object.object_flags = DONATED;
    object.disabled_flags =
        OBJECT_LIMIT_EXCEEDED | OBJECT_LIMIT_THREAD_DISABLE | PENDING_ROOMMATE_DELETION;
    let result = update_overbudget(&input).unwrap();
    assert_eq!(result.next.lot.object_limit, 10000);
    assert!(!result.next.lot.limit_exceeded);
    assert_eq!(
        result.next.objects[&ObjectId(1)].disabled_flags,
        PENDING_ROOMMATE_DELETION
    );
    assert_eq!(result.next.placement_capacity().unwrap().user_objects, 1);
    assert!(required_effects(&result.operations).is_empty());
}

#[test]
fn visitor_removal_sets_pending_on_donated_owned_parts_and_refreshes_unchanged_parts() {
    let mut input = projection();
    input.lot.roommates.insert(AVATAR);
    input.lot.build_roommates.insert(AVATAR);
    add_group(&mut input, 1, &[1, 2]);
    input.objects.get_mut(&ObjectId(1)).unwrap().object_flags = DONATED;
    input.objects.get_mut(&ObjectId(2)).unwrap().disabled_flags = PENDING_ROOMMATE_DELETION;
    add_group(&mut input, 2, &[3]);
    input.objects.get_mut(&ObjectId(3)).unwrap().owner_id = OTHER;
    input.objects.insert(
        ObjectId(4),
        PermissionObject {
            owner_id: AVATAR,
            ..PermissionObject::default()
        },
    ); // Nonpersistent: skip.
    input.objects.insert(
        ObjectId(5),
        PermissionObject {
            owner_id: AVATAR,
            persistent_id: PersistentId(90),
            ..PermissionObject::default()
        },
    ); // Persisted but unregistered: still in owner scan.
    let result = reconcile_join(&input, AVATAR, AvatarPermissions::Visitor).unwrap();
    let next = &result.transition.next;
    for id in [1, 2, 5] {
        assert_ne!(
            next.objects[&ObjectId(id)].disabled_flags & PENDING_ROOMMATE_DELETION,
            0
        );
    }
    for id in [3, 4] {
        assert_eq!(
            next.objects[&ObjectId(id)].disabled_flags & PENDING_ROOMMATE_DELETION,
            0
        );
    }
    assert_eq!(
        required_effects(&result.transition.operations),
        vec![
            LotPermissionOperation::RequiredSchedulerInterrupt(ObjectId(1)),
            LotPermissionOperation::RequiredLightRefresh(ObjectId(1)),
            LotPermissionOperation::RequiredLightRefresh(ObjectId(2)),
            LotPermissionOperation::RequiredSchedulerInterrupt(ObjectId(5)),
            LotPermissionOperation::RequiredLightRefresh(ObjectId(5)),
        ]
    );
    assert_eq!(next.lot.owner_id, OTHER);
}

#[test]
fn new_resident_clears_pending_but_existing_resident_or_admin_leaves_it_untouched() {
    for role in roles()
        .into_iter()
        .filter(|r| *r >= AvatarPermissions::Roommate)
    {
        for existing in [false, true] {
            let mut input = projection();
            if existing {
                input.lot.roommates.insert(AVATAR);
            }
            add_group(&mut input, 1, &[1, 2]);
            for obj in input.objects.values_mut() {
                obj.disabled_flags = PENDING_ROOMMATE_DELETION | 1;
            }
            let result = reconcile_join(&input, AVATAR, role).unwrap();
            if existing || role == AvatarPermissions::Admin {
                assert_eq!(result.transition.next, input);
                assert!(required_effects(&result.transition.operations).is_empty());
            } else {
                assert_eq!(
                    result.transition.next.objects[&ObjectId(1)].disabled_flags,
                    1
                );
                assert_eq!(
                    result.transition.next.objects[&ObjectId(2)].disabled_flags,
                    1
                );
                assert_eq!(
                    required_effects(&result.transition.operations),
                    vec![
                        LotPermissionOperation::RequiredSchedulerInterrupt(ObjectId(1)),
                        LotPermissionOperation::RequiredLightRefresh(ObjectId(1)),
                        LotPermissionOperation::RequiredSchedulerInterrupt(ObjectId(2)),
                        LotPermissionOperation::RequiredLightRefresh(ObjectId(2)),
                    ]
                );
            }
        }
    }
}

#[test]
fn join_computes_owner_flag_change_after_budget_and_does_not_interrupt_for_budget_alone() {
    let mut input = projection();
    input.lot.roommates.extend([AVATAR, OTHER]);
    add_normal_groups(&mut input, 76);
    for object in input.objects.values_mut() {
        object.owner_id = OTHER;
    }
    let owned = input.objects.get_mut(&ObjectId(76)).unwrap();
    owned.owner_id = AVATAR;
    owned.disabled_flags = PENDING_ROOMMATE_DELETION;
    let result = reconcile_join(&input, AVATAR, AvatarPermissions::Visitor).unwrap();
    assert_eq!(
        result.transition.next.objects[&ObjectId(76)].disabled_flags,
        PENDING_ROOMMATE_DELETION | OBJECT_LIMIT_EXCEEDED | OBJECT_LIMIT_THREAD_DISABLE
    );
    assert_eq!(
        required_effects(&result.transition.operations),
        vec![LotPermissionOperation::RequiredLightRefresh(ObjectId(76)),]
    );
    let ops = &result.transition.operations;
    assert!(matches!(
        ops[0],
        LotPermissionOperation::RequiredAvatarPermissionApplication { .. }
    ));
    assert!(matches!(
        ops[1],
        LotPermissionOperation::SetRoommateMembership { present: false, .. }
    ));
    assert!(matches!(
        ops[2],
        LotPermissionOperation::SetBuildMembership { present: false, .. }
    ));
    assert_eq!(ops[3], LotPermissionOperation::SetObjectLimit(75));
    assert_eq!(
        ops[ops.len() - 3],
        LotPermissionOperation::SetLimitExceeded(true)
    );
    assert!(matches!(
        ops[ops.len() - 2],
        LotPermissionOperation::SetDisabledFlags {
            object_id: ObjectId(76),
            ..
        }
    ));
}

#[test]
fn malformed_inputs_and_membership_growth_are_refused_atomically() {
    let mut base = projection();
    add_group(&mut base, 1, &[1, 2]);
    let mut cases = Vec::new();
    let mut c = base.clone();
    c.lot.size = 14;
    cases.push(c);
    let mut c = base.clone();
    c.lot.object_limit = -1;
    cases.push(c);
    let mut c = base.clone();
    c.lot.donate_limit = -1;
    cases.push(c);
    let mut c = base.clone();
    c.objects.get_mut(&ObjectId(1)).unwrap().object_flags = 2;
    cases.push(c);
    let mut c = base.clone();
    c.objects.get_mut(&ObjectId(1)).unwrap().disabled_flags = 128;
    cases.push(c);
    let mut c = base.clone();
    c.objects.insert(ObjectId(0), PermissionObject::default());
    cases.push(c);
    let mut c = base.clone();
    c.objects.remove(&ObjectId(2));
    cases.push(c);
    let mut c = base.clone();
    c.objects.get_mut(&ObjectId(2)).unwrap().persistent_id = PersistentId(2);
    cases.push(c);
    let mut c = base.clone();
    c.persisted_groups
        .get_mut(&PersistentId(1))
        .unwrap()
        .objects
        .clear();
    cases.push(c);
    let mut c = base.clone();
    c.persisted_groups
        .get_mut(&PersistentId(1))
        .unwrap()
        .base_object = ObjectId(3);
    cases.push(c);
    let mut c = base.clone();
    c.persisted_groups
        .get_mut(&PersistentId(1))
        .unwrap()
        .objects
        .push(ObjectId(1));
    cases.push(c);
    let mut c = base.clone();
    let group = c.persisted_groups.remove(&PersistentId(1)).unwrap();
    c.persisted_groups.insert(PersistentId(0), group);
    cases.push(c);
    let mut c = base.clone();
    let group = c.persisted_groups[&PersistentId(1)].clone();
    c.persisted_groups.insert(PersistentId(2), group);
    cases.push(c);
    for input in cases {
        let before = input.clone();
        assert!(update_overbudget(&input).is_err());
        assert!(reconcile_join(&input, AVATAR, AvatarPermissions::Roommate).is_err());
        assert_eq!(input, before);
    }
    let mut input = projection();
    input.lot.roommates = (100..100 + MAX_PERMISSION_ENTRIES as u32)
        .map(PersistentId)
        .collect();
    input.validate().unwrap();
    let before = input.clone();
    assert_eq!(
        reconcile_join(&input, AVATAR, AvatarPermissions::Roommate),
        Err(LotPermissionError::MembershipBoundExceeded)
    );
    assert_eq!(input, before);
    assert!(reconcile_join(&input, AVATAR, AvatarPermissions::Admin).is_ok());
    let mut input = projection();
    input.lot.build_roommates = (100..100 + MAX_PERMISSION_ENTRIES as u32)
        .map(PersistentId)
        .collect();
    let before = input.clone();
    assert_eq!(
        reconcile_join(&input, AVATAR, AvatarPermissions::BuildBuyRoommate),
        Err(LotPermissionError::MembershipBoundExceeded)
    );
    assert_eq!(input, before);
    input.lot.build_roommates.insert(AVATAR);
    assert_eq!(
        input.validate(),
        Err(LotPermissionError::MembershipBoundExceeded)
    );
}

#[test]
fn ordered_maps_and_replay_produce_identical_pure_results() {
    let mut first = projection();
    first.lot.roommates = BTreeSet::from([AVATAR, OTHER]);
    add_normal_groups(&mut first, 76);
    let mut second = projection();
    second.lot.roommates = BTreeSet::from([OTHER, AVATAR]);
    for id in (1..=76).rev() {
        add_group(&mut second, 10000 - id as u32, &[id]);
    }
    assert_eq!(first, second);
    let a = reconcile_join(&first, AVATAR, AvatarPermissions::Visitor).unwrap();
    let b = reconcile_join(&second, AVATAR, AvatarPermissions::Visitor).unwrap();
    assert_eq!(a, b);
    assert_eq!(
        bincode::serialize(&a).unwrap(),
        bincode::serialize(&b).unwrap()
    );
    let repeated = reconcile_join(&a.transition.next, AVATAR, AvatarPermissions::Visitor).unwrap();
    assert!(!repeated.membership_changed);
    assert_eq!(repeated.transition.next, a.transition.next);
    assert_eq!(repeated.transition.operations.len(), 1);
}
