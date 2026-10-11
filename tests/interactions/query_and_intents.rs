// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//! Snapshot and authority fixtures only; no actual VM/check-tree integration.
mod fixtures;
use fixtures::*;
use sim_core::interactions as wonderland_interactions_check;
use wonderland_interactions_check::*;

/// Independent source-parity fixture: the first check rejects after writing
/// temporary arrays; the second accepts only the configured register view.
struct TempArrayChecks {
    writes: (Option<i16>, Option<i32>),
    second_temps: (i16, i32),
    second_other_state: Option<(u64, u8)>,
}

impl wonderland_interactions_check::adapters::CheckTreeProvider for TempArrayChecks {
    fn evaluate(
        &self,
        request: CheckRequest<'_>,
        state: &mut CheckState,
        _output: &mut CheckOutput,
        budget: &mut CheckBudget,
    ) -> Result<CheckExit> {
        budget.spend(1)?;
        if request.definition.key.tta_index == 1 {
            if let Some(value) = self.writes.0 {
                state.temp_registers[0] = value;
            }
            if let Some(value) = self.writes.1 {
                state.temp_xl[0] = value;
            }
            if self.second_other_state.is_some() {
                state.random_below(1000);
                state.provider_state_mut()[0] = 42;
            }
            return Ok(CheckExit::ReturnFalse);
        }
        let temps_match = (state.temp_registers[0], state.temp_xl[0]) == self.second_temps;
        let other_state_matches = self.second_other_state.map_or(true, |expected| {
            (state.rng_seed, state.provider_state()[0]) == expected
        });
        Ok(if temps_match && other_state_matches {
            CheckExit::ReturnTrue
        } else {
            CheckExit::ReturnFalse
        })
    }
}

#[test]
fn ui_temp_arrays_reset_per_check_after_rejection_and_match_targeted_validation() {
    // Test each array independently so restoring only one cannot mask the defect.
    for writes in [(Some(99), None), (None, Some(199)), (Some(99), Some(199))] {
        let world = FixtureWorld::new(vec![definition(1), definition(2)]);
        let before = world.clone();
        let limits = InteractionLimits::default();
        let queue = queue(LegacyMode::Tso);
        let checks = TempArrayChecks {
            writes,
            second_temps: (10, 50),
            second_other_state: None,
        };
        validate_intent(
            &world,
            &checks,
            &queue,
            world.intent(&queue, 1, 2, 0),
            &limits,
        )
        .unwrap();
        let menu = query_offers(&world, &checks, world.query(), &limits).unwrap();
        assert_eq!(
            menu.offers
                .iter()
                .map(|offer| offer.interaction.tta_index)
                .collect::<Vec<_>>(),
            vec![2],
            "each out-of-tick EvaluateCheck clones both original arrays; writes={writes:?}"
        );
        assert_eq!(world, before);
    }
}

#[test]
fn in_tick_temp_arrays_keep_rejected_check_writes_for_later_entry() {
    let mut live = snapshot(LegacyMode::Tso, vec![definition(1), definition(2)]);
    let checks = TempArrayChecks {
        writes: (Some(99), Some(199)),
        second_temps: (99, 199),
        second_other_state: None,
    };
    let menu = query_in_tick(
        &mut live,
        &checks,
        QueryOptions::default(),
        &InteractionLimits::default(),
    )
    .unwrap();
    assert_eq!(
        menu.offers
            .iter()
            .map(|offer| offer.interaction.tta_index)
            .collect::<Vec<_>>(),
        vec![2],
        "in-tick EvaluateCheck shares both temporary arrays"
    );
    assert_eq!(live.state().temp_registers[0], 99);
    assert_eq!(live.state().temp_xl[0], 199);
}

#[test]
fn ui_temp_arrays_reset_without_resetting_rng_or_other_detached_state() {
    let world = FixtureWorld::new(vec![definition(1), definition(2)]);
    let before = world.clone();
    let mut expected_state = world.live.state().clone();
    expected_state.random_below(1000);
    let checks = TempArrayChecks {
        writes: (Some(99), Some(199)),
        second_temps: (10, 50),
        second_other_state: Some((expected_state.rng_seed, 42)),
    };
    let menu = query_offers(
        &world,
        &checks,
        world.query(),
        &InteractionLimits::default(),
    )
    .unwrap();
    assert_eq!(
        menu.offers
            .iter()
            .map(|offer| offer.interaction.tta_index)
            .collect::<Vec<_>>(),
        vec![2],
        "per-check cloning applies to temp arrays, not the rest of the detached query state"
    );
    assert_eq!(world, before, "the complete UI snapshot is still discarded");
}

#[test]
fn ui_query_isolates_rng_temps_ads_and_all_owned_provider_bytes() {
    let world = FixtureWorld::new(vec![definition(300)]);
    let before = world.clone();
    let checks = FixtureChecks {
        expected_origin: Some(CheckOrigin::Ui),
        ..Default::default()
    };
    let first = query_offers(
        &world,
        &checks,
        world.query(),
        &InteractionLimits::default(),
    )
    .unwrap();
    let second = query_offers(
        &world,
        &checks,
        world.query(),
        &InteractionLimits::default(),
    )
    .unwrap();
    assert_eq!(
        first, second,
        "repeated read-only queries must return identical offers"
    );
    assert_eq!(
        world, before,
        "every live value, including original check advertisements, is unchanged"
    );
    assert_eq!(
        first.offers[0].interaction.tta_index, 300,
        "no byte truncation of TTAB IDs"
    );
    assert_eq!(
        first.offers[0].advertisements,
        vec![
            AdvertisementChange {
                motive: 2,
                value: 3
            },
            AdvertisementChange {
                motive: 7,
                value: 15
            }
        ]
    );
}

#[test]
fn explicit_tick_query_persists_register_rng_and_provider_effects() {
    let mut live = snapshot(LegacyMode::Tso, vec![definition(1)]);
    let before = live.clone();
    let checks = FixtureChecks {
        expected_origin: Some(CheckOrigin::InTick),
        ..Default::default()
    };
    let batch = query_in_tick(
        &mut live,
        &checks,
        QueryOptions::default(),
        &InteractionLimits::default(),
    )
    .unwrap();
    assert_ne!(live.state().rng_seed, before.state().rng_seed);
    assert_eq!(
        live.state().temp_registers[3],
        before.state().temp_registers[3] + 1
    );
    assert_eq!(live.state().temp_xl[1], before.state().temp_xl[1] + 2);
    assert_eq!(
        live.state().provider_state()[0],
        before.state().provider_state()[0] + 1
    );
    assert_eq!(
        batch.offers[0].advertisements,
        live.state().advertisements()
    );
    assert_eq!(
        live.state().advertisements().len(),
        2,
        "fresh check ads replace prior-thread ads"
    );
}

#[test]
fn false_yield_abort_and_error_queries_leave_live_state_unchanged() {
    for behavior in [
        FixtureBehavior::Reject,
        FixtureBehavior::Yield,
        FixtureBehavior::Abort,
        FixtureBehavior::Error,
    ] {
        let world = FixtureWorld::new(vec![definition(1)]);
        let before = world.clone();
        let checks = FixtureChecks {
            behavior,
            ..Default::default()
        };
        let result = query_offers(
            &world,
            &checks,
            world.query(),
            &InteractionLimits::default(),
        );
        match behavior {
            FixtureBehavior::Reject | FixtureBehavior::Yield => {
                assert!(result.unwrap().offers.is_empty())
            }
            _ => assert!(matches!(result, Err(Error::RuntimeFailure(_)))),
        }
        assert_eq!(world, before);
    }
}

#[test]
fn variants_keep_source_order_names_and_param0_without_duplicating_default_ads() {
    let world = FixtureWorld::new(vec![definition(1)]);
    let checks = FixtureChecks {
        behavior: FixtureBehavior::Variants,
        ..Default::default()
    };
    let batch = query_offers(
        &world,
        &checks,
        world.query(),
        &InteractionLimits::default(),
    )
    .unwrap();
    assert_eq!(
        batch
            .offers
            .iter()
            .map(|offer| offer.param0)
            .collect::<Vec<_>>(),
        vec![7, -2]
    );
    assert_eq!(batch.offers[0].label, "Fixture dynamic label");
    assert_eq!(batch.offers[1].label, "Fixture 1");
    assert!(batch
        .offers
        .iter()
        .all(|offer| offer.advertisements.is_empty()));
}

#[test]
fn hidden_checks_reset_per_interaction_and_snapshot_only() {
    let world = FixtureWorld::new(vec![definition(1), definition(2)]);
    let checks = FixtureChecks {
        behavior: FixtureBehavior::HideFirst,
        ..Default::default()
    };
    let offers = query_offers(
        &world,
        &checks,
        world.query(),
        &InteractionLimits::default(),
    )
    .unwrap();
    assert_eq!(
        offers
            .offers
            .iter()
            .map(|offer| offer.interaction.tta_index)
            .collect::<Vec<_>>(),
        vec![2]
    );
    let mut query = world.query();
    query.options.include_hidden = true;
    assert_eq!(
        query_offers(&world, &checks, query, &InteractionLimits::default())
            .unwrap()
            .offers
            .len(),
        2
    );
    assert!(!world.live.state().hide_interaction);
    let mut hidden = world.clone();
    hidden.live.state_mut().out_of_world = true;
    assert!(query_offers(
        &hidden,
        &FixtureChecks::default(),
        hidden.query(),
        &InteractionLimits::default()
    )
    .unwrap()
    .offers
    .is_empty());
}

#[test]
fn local_and_global_tta_indices_stay_distinct_and_in_source_order() {
    let mut global = definition(300);
    global.key.scope = InteractionScope::Global;
    let world = FixtureWorld::new(vec![definition(300), global]);
    let batch = query_offers(
        &world,
        &FixtureChecks::default(),
        world.query(),
        &InteractionLimits::default(),
    )
    .unwrap();
    assert_eq!(
        batch
            .offers
            .iter()
            .map(|offer| offer.interaction.scope)
            .collect::<Vec<_>>(),
        vec![InteractionScope::Local, InteractionScope::Global]
    );
    let mut query = world.query();
    query.options.include_global = false;
    assert_eq!(
        query_offers(
            &world,
            &FixtureChecks::default(),
            query,
            &InteractionLimits::default()
        )
        .unwrap()
        .offers
        .len(),
        1
    );
}

#[test]
fn absent_local_ttab_hides_full_menu_but_preserves_single_global_validation() {
    let mut global = definition(1);
    global.key.scope = InteractionScope::Global;
    let mut world = FixtureWorld::new(vec![global]);
    world.live.set_local_table_present(false).unwrap();
    assert!(query_offers(
        &world,
        &FixtureChecks::default(),
        world.query(),
        &InteractionLimits::default()
    )
    .unwrap()
    .offers
    .is_empty());
    let queue = queue(LegacyMode::Tso);
    let mut intent = world.intent(&queue, 1, 1, 0);
    intent.interaction.scope = InteractionScope::Global;
    validate_intent(
        &world,
        &FixtureChecks::default(),
        &queue,
        intent,
        &InteractionLimits::default(),
    )
    .unwrap();
}

#[test]
fn missing_ttas_and_check_defaults_follow_get_pie_menu() {
    let mut no_ttas = definition(1);
    no_ttas.label = None;
    let mut no_check = no_ttas.clone();
    no_check.key = key(2);
    no_check.check = None;
    let world = FixtureWorld::new(vec![no_ttas, no_check]);
    let batch = query_offers(
        &world,
        &FixtureChecks::default(),
        world.query(),
        &InteractionLimits::default(),
    )
    .unwrap();
    assert_eq!(batch.offers.len(), 1);
    assert_eq!(batch.offers[0].label, "***MISSING***");
    assert_eq!(batch.offers[0].interaction, key(1));
}

#[test]
fn autonomous_check_arg_is_one_while_variant_does_not_enter_check_args() {
    let world = FixtureWorld::new(vec![definition(1)]);
    let mut query = world.query();
    query.options.autonomous = true;
    let checks = FixtureChecks {
        expected_args: [1, 0, 0, 0],
        ..Default::default()
    };
    query_offers(&world, &checks, query, &InteractionLimits::default()).unwrap();
    let queue = queue(LegacyMode::Tso);
    let checks = FixtureChecks {
        behavior: FixtureBehavior::Variants,
        expected_args: [0; 4],
        ..Default::default()
    };
    validate_intent(
        &world,
        &checks,
        &queue,
        world.intent(&queue, 1, 1, 7),
        &InteractionLimits::default(),
    )
    .unwrap();
}

#[test]
fn budgets_and_output_bounds_fail_even_when_provider_ignores_the_error() {
    let world = FixtureWorld::new(vec![definition(1)]);
    let before = world.clone();
    let limits = InteractionLimits {
        max_offers: 1,
        ..Default::default()
    };
    for behavior in [
        FixtureBehavior::IgnoreVariantBound,
        FixtureBehavior::IgnoreBudget,
    ] {
        let checks = FixtureChecks {
            behavior,
            ..Default::default()
        };
        assert!(matches!(
            query_offers(&world, &checks, world.query(), &limits),
            Err(Error::LimitExceeded(_))
        ));
        assert_eq!(world, before);
    }
    let limits = InteractionLimits {
        max_check_steps: 1,
        ..Default::default()
    };
    assert_eq!(
        query_offers(&world, &FixtureChecks::default(), world.query(), &limits),
        Err(Error::LimitExceeded("check instruction budget"))
    );
}

#[test]
fn snapshot_and_offer_resource_growth_is_bounded() {
    let limits = InteractionLimits {
        max_definitions: 1,
        max_label_bytes: 16,
        max_advertisements: 1,
        max_provider_state_bytes: 2,
        ..Default::default()
    };
    let mut state = CheckState::default();
    assert!(state.set_provider_state(&[1, 2, 3], &limits).is_err());
    assert!(state.provider_state().is_empty());
    state.set_advertisement(1, 1, &limits).unwrap();
    assert!(state.set_advertisement(2, 2, &limits).is_err());
    state.set_advertisement(1, 3, &limits).unwrap();
    assert_eq!(state.advertisements()[0].value, 3);
    let mut snapshot =
        InteractionSnapshot::new(LegacyMode::Tso, 100, actor(), target(), state, &limits).unwrap();
    snapshot.add_definition(definition(1), &limits).unwrap();
    assert!(snapshot.add_definition(definition(2), &limits).is_err());
    assert_eq!(snapshot.definitions().len(), 1);
    let mut huge_label = definition(3);
    huge_label.label = Some("this exceeds sixteen bytes".into());
    assert_eq!(
        snapshot.add_definition(huge_label, &limits),
        Err(Error::LimitExceeded("interaction label bytes"))
    );
    let world = FixtureWorld::new(vec![definition(1), definition(2)]);
    let limits = InteractionLimits {
        max_offers: 1,
        ..Default::default()
    };
    assert!(matches!(
        query_offers(&world, &FixtureChecks::default(), world.query(), &limits),
        Err(Error::LimitExceeded("interaction offers"))
    ));
}

#[test]
fn duplicate_ttab_keys_and_zero_test_binding_are_explicit_errors() {
    let limits = InteractionLimits::default();
    let mut snapshot = snapshot(LegacyMode::Tso, vec![definition(1)]);
    assert!(matches!(
        snapshot.add_definition(definition(1), &limits),
        Err(Error::InvalidSnapshot(_))
    ));
    let mut zero_check = definition(2);
    zero_check.check.as_mut().unwrap().routine_id = 0;
    assert!(matches!(
        snapshot.add_definition(zero_check, &limits),
        Err(Error::InvalidSnapshot(_))
    ));
}

#[test]
fn validation_checks_variant_ignores_occupied_only_in_detached_state_and_rechecks_at_start() {
    let mut world = FixtureWorld::new(vec![definition(1)]);
    world.live.state_mut().target_occupied = true;
    let checks = FixtureChecks {
        behavior: FixtureBehavior::RequireUnoccupied,
        ..Default::default()
    };
    let limits = InteractionLimits::default();
    assert!(query_offers(&world, &checks, world.query(), &limits)
        .unwrap()
        .offers
        .is_empty());
    let mut queue = queue(LegacyMode::Tso);
    let invalid = validate_intent(
        &world,
        &checks,
        &queue,
        world.intent(&queue, 1, 1, 7),
        &limits,
    );
    assert!(matches!(invalid, Err(Error::VariantUnavailable)));
    let valid = validate_intent(
        &world,
        &checks,
        &queue,
        world.intent(&queue, 1, 1, 0),
        &limits,
    )
    .unwrap();
    assert!(world.live.state().target_occupied);
    let mut runtime = FixtureRuntime::default();
    runtime.reject_check.insert(1); // Fixture for the authoritative occupied check.
    let id = queue
        .enqueue_validated(&world, &runtime, valid)
        .unwrap()
        .result;
    let result = queue.attempt_push(&mut runtime).unwrap();
    assert_eq!(result.result, PushOutcome::Empty);
    assert!(result.events.iter().any(|event| event.action == Some(id)
        && event.kind
            == QueueEventKind::Removed {
                reason: RemovalReason::CheckRejected
            }));
    assert!(runtime.frames.is_empty());
}

#[test]
fn validated_intent_uses_server_definition_and_four_source_action_args() {
    let world = FixtureWorld::new(vec![definition(1)]);
    let checks = FixtureChecks {
        behavior: FixtureBehavior::Variants,
        ..Default::default()
    };
    let mut queue = queue(LegacyMode::Tso);
    let valid = validate_intent(
        &world,
        &checks,
        &queue,
        world.intent(&queue, 1, 1, -2),
        &InteractionLimits::default(),
    )
    .unwrap();
    queue
        .enqueue_validated(&world, &FixtureRuntime::default(), valid)
        .unwrap();
    let action = &queue.entries()[0].invocation;
    assert_eq!(action.args, [-2, 0, 0, 0]);
    assert_eq!(action.priority, 50);
    assert_eq!(action.mode, QueueMode::Normal);
    assert!(!action.definition.flags.has(ActionFlags::SKIP_PERMISSIONS));
    assert_eq!(action.definition, definition(1));
}

#[test]
fn stale_entity_generation_entity_revision_and_world_revision_are_rejected() {
    let limits = InteractionLimits::default();
    for case in 0..4 {
        let mut world = FixtureWorld::new(vec![definition(1)]);
        let before = world.query();
        match case {
            0 => world.revision += 1,
            1 => world.live_target.as_mut().unwrap().key.generation += 1,
            2 => world.live_actor.as_mut().unwrap().revision += 1,
            3 => world.live_target = None,
            _ => unreachable!(),
        }
        let result = query_offers(&world, &FixtureChecks::default(), before, &limits);
        assert!(matches!(
            result,
            Err(Error::StaleWorldRevision { .. }) | Err(Error::StaleEntity(_))
        ));
    }
}

#[test]
fn offers_and_prepared_permits_do_not_bypass_later_authorization_changes() {
    let limits = InteractionLimits::default();
    let mut world = FixtureWorld::new(vec![definition(1)]);
    let mut queue = queue(LegacyMode::Tso);
    let valid = validate_intent(
        &world,
        &FixtureChecks::default(),
        &queue,
        world.intent(&queue, 1, 1, 0),
        &limits,
    )
    .unwrap();
    world.allowed = false; // Defense in depth even if a broken adapter omits a revision bump.
    assert!(matches!(
        queue.enqueue_validated(&world, &FixtureRuntime::default(), valid),
        Err(Error::Unauthorized)
    ));
    assert_eq!(queue.revision(), 0);
    assert!(queue.entries().is_empty());
    assert!(matches!(
        query_offers(&world, &FixtureChecks::default(), world.query(), &limits),
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        validate_intent(
            &world,
            &FixtureChecks::default(),
            &queue,
            world.intent(&queue, 1, 1, 0),
            &limits
        ),
        Err(Error::Unauthorized)
    ));
}

#[test]
fn permits_become_stale_after_queue_or_world_changes_without_consuming_commands() {
    let limits = InteractionLimits::default();
    let mut world = FixtureWorld::new(vec![definition(1)]);
    let mut queue = queue(LegacyMode::Tso);
    let runtime = FixtureRuntime::default();
    let valid = validate_intent(
        &world,
        &FixtureChecks::default(),
        &queue,
        world.intent(&queue, 1, 1, 0),
        &limits,
    )
    .unwrap();
    queue
        .enqueue_internal(invocation(2, 50, QueueMode::Normal, 0), &runtime)
        .unwrap();
    assert!(matches!(
        queue.enqueue_validated(&world, &runtime, valid),
        Err(Error::StaleQueueRevision { .. })
    ));
    assert_eq!(queue.last_command_sequence(), None);
    let valid = validate_intent(
        &world,
        &FixtureChecks::default(),
        &queue,
        world.intent(&queue, 1, 1, 0),
        &limits,
    )
    .unwrap();
    world.revision += 1;
    assert!(matches!(
        queue.enqueue_validated(&world, &runtime, valid),
        Err(Error::StaleWorldRevision { .. })
    ));
    assert_eq!(indices(&queue), vec![2]);
}

#[test]
fn repeated_gameplay_entries_are_legal_but_replayed_command_sequences_are_not() {
    let limits = InteractionLimits::default();
    let world = FixtureWorld::new(vec![definition(1)]);
    let mut queue = queue(LegacyMode::Tso);
    let runtime = FixtureRuntime::default();
    for sequence in 1..=2 {
        let valid = validate_intent(
            &world,
            &FixtureChecks::default(),
            &queue,
            world.intent(&queue, sequence, 1, 0),
            &limits,
        )
        .unwrap();
        queue.enqueue_validated(&world, &runtime, valid).unwrap();
    }
    assert_eq!(indices(&queue), vec![1, 1]);
    assert_ne!(queue.entries()[0].id, queue.entries()[1].id);
    assert!(matches!(
        validate_intent(
            &world,
            &FixtureChecks::default(),
            &queue,
            world.intent(&queue, 2, 1, 0),
            &limits
        ),
        Err(Error::Replay {
            sequence: 2,
            last: 2
        })
    ));
}

#[test]
fn cancellation_validates_owner_permission_revision_and_monotonic_operation_identity() {
    let mut world = FixtureWorld::new(vec![definition(1)]);
    let mut queue = queue(LegacyMode::Tso);
    let id = queue
        .enqueue_internal(
            invocation(1, 50, QueueMode::Normal, 0),
            &FixtureRuntime::default(),
        )
        .unwrap()
        .result;
    let mut cancel = world.cancel(&queue, 1, id);
    cancel.principal = PrincipalKey(99);
    assert_eq!(
        queue.cancel_intent(&world, cancel),
        Err(Error::Unauthorized)
    );
    cancel = world.cancel(&queue, 1, id);
    world.live_actor.as_mut().unwrap().key.generation += 1;
    assert!(matches!(
        queue.cancel_intent(&world, cancel),
        Err(Error::StaleEntity(_))
    ));
    world.live_actor = Some(actor_version());
    let success = queue.cancel_intent(&world, cancel).unwrap();
    assert_eq!(success.result, CancelOutcome::Removed);
    let retry = world.cancel(&queue, 1, id);
    assert!(matches!(
        queue.cancel_intent(&world, retry),
        Err(Error::Replay { .. })
    ));
}

#[test]
fn user_limit_counts_all_entries_and_does_not_remove_cleanup_on_rejection() {
    let world = FixtureWorld::new(vec![definition(1)]);
    let runtime = FixtureRuntime::default();
    let mut queue = queue(LegacyMode::Tso);
    for index in 0..20 {
        queue
            .enqueue_internal(
                invocation(index, 30, QueueMode::ParentExit, ActionFlags::MUST_RUN),
                &runtime,
            )
            .unwrap();
    }
    let before = queue.entries().to_vec();
    let revision = queue.revision();
    let valid = validate_intent(
        &world,
        &FixtureChecks::default(),
        &queue,
        world.intent(&queue, 1, 1, 0),
        &InteractionLimits::default(),
    )
    .unwrap();
    assert!(matches!(
        queue.enqueue_validated(&world, &runtime, valid),
        Err(Error::UserQueueFull)
    ));
    assert_eq!(queue.entries(), before);
    assert_eq!(queue.revision(), revision);
    assert_eq!(queue.last_command_sequence(), None);
}

#[test]
fn queue_snapshot_dialect_mismatch_is_explicit() {
    let world = FixtureWorld::new(vec![definition(1)]);
    let queue = queue(LegacyMode::Ts1);
    assert!(matches!(
        validate_intent(
            &world,
            &FixtureChecks::default(),
            &queue,
            world.intent(&queue, 1, 1, 0),
            &InteractionLimits::default()
        ),
        Err(Error::InvalidSnapshot(_))
    ));
}

#[test]
fn source_rng_zero_bound_does_not_advance_state_and_wrapping_is_deterministic() {
    let mut state = CheckState::default();
    state.rng_seed = u64::MAX;
    assert_eq!(state.random_below(0), 0);
    assert_eq!(state.rng_seed, u64::MAX);
    let mut other = state.clone();
    for _ in 0..20 {
        assert_eq!(state.random_below(1000), other.random_below(1000));
    }
    assert_ne!(state.rng_seed, u64::MAX);
}
