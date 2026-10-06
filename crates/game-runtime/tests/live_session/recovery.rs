use super::*;
use wonderland_game_runtime::AcceptedCommand;
use wonderland_game_runtime::live_session::LiveError;

fn checkpoint<'a>(server: &GameRuntime, bytes: &'a [u8]) -> Checkpoint<'a> {
    Checkpoint {
        completed_tick: server.sim().state().completed_tick,
        state_hash: server.sim().state_hash().unwrap(),
        bytes,
    }
}

#[test]
fn replica_cannot_be_constructed_from_authority_or_zero_identity() {
    assert!(matches!(
        LiveReplica::new(
            runtime(RuntimeRole::Authority),
            identity(),
            ReplayLimits::default()
        ),
        Err(LiveError::NotReplica)
    ));
    for wrong in [
        StreamIdentity {
            browser_epoch: 0,
            ..identity()
        },
        StreamIdentity {
            source_epoch: 0,
            ..identity()
        },
        StreamIdentity {
            lot_incarnation: 0,
            ..identity()
        },
    ] {
        assert!(matches!(
            LiveReplica::new(
                runtime(RuntimeRole::Replica),
                wrong,
                ReplayLimits::default()
            ),
            Err(LiveError::InvalidIdentity)
        ));
    }
}

#[test]
fn invalid_limits_do_not_create_a_session() {
    let mut cases = Vec::new();
    for value in [0, 257, usize::MAX] {
        cases.push(ReplayLimits {
            max_batch_ticks: value,
            ..ReplayLimits::default()
        });
    }
    for value in [0, 65537, usize::MAX] {
        cases.push(ReplayLimits {
            max_batch_commands: value,
            ..ReplayLimits::default()
        });
    }
    for value in [0, 8 * 1024 * 1024 + 1, usize::MAX] {
        cases.push(ReplayLimits {
            max_batch_bytes: value,
            ..ReplayLimits::default()
        });
    }
    for value in [0, 140, usize::MAX] {
        cases.push(ReplayLimits {
            max_checkpoint_bytes: value,
            ..ReplayLimits::default()
        });
    }
    for limits in cases {
        assert!(matches!(
            LiveReplica::new(runtime(RuntimeRole::Replica), identity(), limits),
            Err(LiveError::InvalidLimits)
        ));
    }
}

#[test]
fn superseded_checkpoint_is_ignored_without_invalidating_the_new_request() {
    let server = runtime(RuntimeRole::Authority);
    let bytes = server.snapshot().unwrap();
    let mut client = client();
    let first = client.checkpoint_ticket().unwrap();
    let second = client.request_checkpoint(client.connection()).unwrap();
    assert_ne!(first, second);
    assert!(matches!(
        client.install_checkpoint(first, checkpoint(&server, &bytes), &[]),
        Err(LiveError::StaleCheckpoint)
    ));
    assert_eq!(client.checkpoint_ticket(), Some(second));
    client
        .install_checkpoint(second, checkpoint(&server, &bytes), &[])
        .unwrap();
    assert_eq!(client.status(), SessionStatus::Live);
}

#[test]
fn corrupt_checkpoint_does_not_destroy_last_known_state_or_consume_the_ticket() {
    let server = runtime(RuntimeRole::Authority);
    let mut bytes = server.snapshot().unwrap();
    let mut client = client();
    let ticket = client.checkpoint_ticket().unwrap();
    let before = client.runtime().unwrap().snapshot().unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    assert!(
        client
            .install_checkpoint(ticket, checkpoint(&server, &bytes), &[])
            .is_err()
    );
    assert_eq!(client.checkpoint_ticket(), Some(ticket));
    assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
    assert_eq!(client.status(), SessionStatus::AwaitingCheckpoint);
}

#[test]
fn incorrect_checkpoint_tick_and_hash_are_rejected() {
    let server = runtime(RuntimeRole::Authority);
    let bytes = server.snapshot().unwrap();
    let mut client = client();
    let mut value = checkpoint(&server, &bytes);
    value.completed_tick += 1;
    assert!(matches!(
        client.install_checkpoint(client.checkpoint_ticket().unwrap(), value, &[]),
        Err(LiveError::CheckpointMetadata)
    ));
    value = checkpoint(&server, &bytes);
    value.state_hash[4] ^= 1;
    assert!(matches!(
        client.install_checkpoint(client.checkpoint_ticket().unwrap(), value, &[]),
        Err(LiveError::CheckpointMetadata)
    ));
    install(&mut client, &server, &[]);
}

#[test]
fn gap_and_conflicting_duplicate_trigger_atomic_recovery() {
    let mut server = runtime(RuntimeRole::Authority);
    let mut client = client();
    install(&mut client, &server, &[]);
    let first = advance(&mut server);
    let second = advance(&mut server);
    let before = client.runtime().unwrap().snapshot().unwrap();
    assert!(client.apply_batch(client.connection(), &[second]).is_err());
    assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
    install(&mut client, &server, &[]);
    let before = client.runtime().unwrap().snapshot().unwrap();
    assert!(client.apply_batch(client.connection(), &[first]).is_err());
    assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
}

#[test]
fn recovery_can_replay_an_older_checkpoint_through_the_committed_anchor() {
    let mut server = runtime(RuntimeRole::Authority);
    let bytes = server.snapshot().unwrap();
    let initial = checkpoint(&server, &bytes);
    let mut client = client();
    install(&mut client, &server, &[]);
    let first = advance(&mut server);
    client
        .apply_batch(client.connection(), std::slice::from_ref(&first))
        .unwrap();
    client.suspend(client.connection()).unwrap();
    client.reconnect().unwrap();
    let second = advance(&mut server);
    let cursor = client
        .install_checkpoint(
            client.checkpoint_ticket().unwrap(),
            initial,
            &[first, second],
        )
        .unwrap();
    assert_eq!(cursor.completed_tick, 2);
    assert_eq!(cursor.state_hash, server.sim().state_hash().unwrap());
    assert_eq!(client.projection().unwrap(), server.projection());
}

#[test]
fn a_forked_checkpoint_cannot_replace_a_committed_same_tick() {
    let mut server = runtime(RuntimeRole::Authority);
    let mut fork = runtime(RuntimeRole::Authority);
    let mut client = client();
    let _ = advance(&mut server);
    install(&mut client, &server, &[]);
    fork.advance(vec![AcceptedCommand::SetTs1FamilyBudget(Some(100))])
        .unwrap();
    let bytes = fork.snapshot().unwrap();
    client.request_checkpoint(client.connection()).unwrap();
    assert!(matches!(
        client.install_checkpoint(
            client.checkpoint_ticket().unwrap(),
            checkpoint(&fork, &bytes),
            &[]
        ),
        Err(LiveError::HistoryConflict)
    ));
    assert_eq!(
        client.cursor().unwrap().state_hash,
        server.sim().state_hash().unwrap()
    );
}

#[test]
fn tail_history_must_match_the_committed_anchor_even_when_it_ends_ahead() {
    let mut server = runtime(RuntimeRole::Authority);
    let mut fork = runtime(RuntimeRole::Authority);
    let bytes = fork.snapshot().unwrap();
    let initial = checkpoint(&fork, &bytes);
    let mut client = client();
    let _ = advance(&mut server);
    install(&mut client, &server, &[]);
    let accepted = fork
        .sim()
        .next_tick(vec![AcceptedCommand::SetTs1FamilyBudget(Some(100))])
        .unwrap();
    let outcome = fork.apply_accepted(&accepted).unwrap();
    let first = TickFrame {
        accepted,
        state_hash: outcome.state_hash,
    };
    let second = advance(&mut fork);
    client.request_checkpoint(client.connection()).unwrap();
    assert!(matches!(
        client.install_checkpoint(
            client.checkpoint_ticket().unwrap(),
            initial,
            &[first, second]
        ),
        Err(LiveError::HistoryConflict)
    ));
    assert_eq!(
        client.runtime().unwrap().snapshot().unwrap(),
        server.snapshot().unwrap()
    );
}

#[test]
fn over_budget_tick_batches_do_not_mutate_state() {
    let mut server = runtime(RuntimeRole::Authority);
    let mut client = LiveReplica::new(
        runtime(RuntimeRole::Replica),
        identity(),
        ReplayLimits {
            max_batch_ticks: 1,
            ..ReplayLimits::default()
        },
    )
    .unwrap();
    install(&mut client, &server, &[]);
    let before = client.runtime().unwrap().snapshot().unwrap();
    let frames = [advance(&mut server), advance(&mut server)];
    assert!(matches!(
        client.apply_batch(client.connection(), &frames),
        Err(LiveError::Limit("batch ticks"))
    ));
    assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
    assert_eq!(client.status(), SessionStatus::AwaitingCheckpoint);
}

#[test]
fn oversized_checkpoint_is_rejected_before_decoding() {
    let server = runtime(RuntimeRole::Authority);
    let bytes = server.snapshot().unwrap();
    let mut client = LiveReplica::new(
        runtime(RuntimeRole::Replica),
        identity(),
        ReplayLimits {
            max_checkpoint_bytes: 141,
            ..ReplayLimits::default()
        },
    )
    .unwrap();
    assert!(matches!(
        client.install_checkpoint(
            client.checkpoint_ticket().unwrap(),
            checkpoint(&server, &bytes),
            &[]
        ),
        Err(LiveError::Limit("checkpoint bytes"))
    ));
}

#[test]
fn checkpoint_cannot_change_the_effect_stream_or_configured_effect_limits() {
    for namespace_changed in [true, false] {
        let mut config = RuntimeConfig::new(VmMode::Ts1, 11, 7, 123);
        if namespace_changed {
            config.effect_namespace += 1;
        } else {
            config.effect_limits.max_pending -= 1;
        }
        let alternate = GameRuntime::new(
            ContentSet::new(RoutineStore::new(), vec![], vec![], TuningSet::default()).unwrap(),
            LotModel::new(8, 8, 1).unwrap(),
            config,
            RuntimeRole::Authority,
        )
        .unwrap();
        let bytes = alternate.snapshot().unwrap();
        let mut client = client();
        let before = client.runtime().unwrap().snapshot().unwrap();
        assert!(matches!(
            client.install_checkpoint(
                client.checkpoint_ticket().unwrap(),
                checkpoint(&alternate, &bytes),
                &[]
            ),
            Err(LiveError::ChangedConfiguration)
        ));
        assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
    }
}

#[test]
fn foreign_checkpoint_identity_content_mode_and_limits_are_rejected() {
    for case in 0..5 {
        let mut config = RuntimeConfig::new(VmMode::Ts1, 11, 7, 123);
        let mut tuning = TuningSet::default();
        match case {
            0 => config.lot_id += 1,
            1 => config.authority_epoch += 1,
            2 => config.mode = VmMode::Tso,
            3 => config.limits.max_commands_per_tick -= 1,
            4 => {
                tuning.values.insert((1, 2, 3), 7);
            }
            _ => unreachable!(),
        }
        let server = GameRuntime::new(
            ContentSet::new(RoutineStore::new(), vec![], vec![], tuning).unwrap(),
            LotModel::new(8, 8, 1).unwrap(),
            config,
            RuntimeRole::Authority,
        )
        .unwrap();
        let bytes = server.snapshot().unwrap();
        let mut client = client();
        let before = client.runtime().unwrap().snapshot().unwrap();
        let ticket = client.checkpoint_ticket().unwrap();
        assert!(
            client
                .install_checkpoint(ticket, checkpoint(&server, &bytes), &[])
                .is_err()
        );
        assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
        assert_eq!(client.checkpoint_ticket(), Some(ticket));
    }
}

#[test]
fn invalid_checkpoint_tail_leaves_the_last_committed_state_untouched() {
    let mut server = runtime(RuntimeRole::Authority);
    let mut client = client();
    install(&mut client, &server, &[]);
    let before = client.runtime().unwrap().snapshot().unwrap();
    let first = advance(&mut server);
    let bytes = server.snapshot().unwrap();
    let start = checkpoint(&server, &bytes);
    let second = advance(&mut server);
    let mut third = advance(&mut server);
    third.state_hash[0] ^= 1;
    let ticket = client.request_checkpoint(client.connection()).unwrap();
    assert!(matches!(
        client.install_checkpoint(ticket, start, &[second, third]),
        Err(LiveError::HashMismatch { tick: 3 })
    ));
    assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
    assert_eq!(client.cursor().unwrap().completed_tick, 0);
    assert_eq!(client.checkpoint_ticket(), Some(ticket));
    // A failed handoff cannot leak the earlier candidate tick into a later retry.
    client
        .install_checkpoint(
            ticket,
            Checkpoint {
                completed_tick: 0,
                state_hash: client.cursor().unwrap().state_hash,
                bytes: &before,
            },
            &[first],
        )
        .unwrap();
    assert_eq!(client.cursor().unwrap().completed_tick, 1);
}

#[test]
fn command_and_binary_byte_budgets_reject_without_applying_any_command() {
    for (limits, expected) in [
        (
            ReplayLimits {
                max_batch_commands: 1,
                ..ReplayLimits::default()
            },
            "batch commands",
        ),
        (
            ReplayLimits {
                max_batch_bytes: 8,
                ..ReplayLimits::default()
            },
            "batch bytes",
        ),
    ] {
        let mut server = runtime(RuntimeRole::Authority);
        let mut client =
            LiveReplica::new(runtime(RuntimeRole::Replica), identity(), limits).unwrap();
        install(&mut client, &server, &[]);
        let before = client.runtime().unwrap().snapshot().unwrap();
        let accepted = server
            .sim()
            .next_tick(vec![
                AcceptedCommand::SetTs1FamilyBudget(Some(100)),
                AcceptedCommand::SetTs1FamilyBudget(Some(200)),
            ])
            .unwrap();
        let outcome = server.apply_accepted(&accepted).unwrap();
        assert!(
            matches!(client.apply_batch(client.connection(), &[TickFrame {
            accepted, state_hash: outcome.state_hash,
        }]), Err(LiveError::Limit(reason)) if reason == expected)
        );
        assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
    }
}

#[test]
fn callbacks_from_another_admission_cannot_touch_a_live_session() {
    let mut server = runtime(RuntimeRole::Authority);
    let mut client = client();
    install(&mut client, &server, &[]);
    for other_identity in [
        StreamIdentity {
            browser_epoch: 99,
            ..identity()
        },
        StreamIdentity {
            source_epoch: 99,
            ..identity()
        },
        StreamIdentity {
            lot_incarnation: 99,
            ..identity()
        },
    ] {
        let other = LiveReplica::new(
            runtime(RuntimeRole::Replica),
            other_identity,
            ReplayLimits::default(),
        )
        .unwrap();
        let before = client.runtime().unwrap().snapshot().unwrap();
        assert!(matches!(
            client.suspend(other.connection()),
            Err(LiveError::StaleConnection)
        ));
        assert!(matches!(
            client.request_checkpoint(other.connection()),
            Err(LiveError::StaleConnection)
        ));
        assert!(matches!(
            client.apply_batch(other.connection(), &[]),
            Err(LiveError::StaleConnection)
        ));
        assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
        assert_eq!(client.status(), SessionStatus::Live);
    }
    let frame = advance(&mut server);
    client.apply_batch(client.connection(), &[frame]).unwrap();
}
