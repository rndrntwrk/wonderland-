//! Native live sessions execute unchanged original BHAV bytes in the actual A/B
//! runtime. The object definition, interaction table and grants below are declared
//! harness metadata, not complete original-object or network-auth qualification.
use sim_core::{
    interactions::{
        ActionFlags, ActionId, CancelIntent, FinishResult, InteractionDefinition, InteractionScope,
        PermissionFlags, QueueEventKind, RemovalReason, RoutineBinding,
    },
    state::{ContentSet, InteractionTable, ObjectDefinition, TuningSet},
    vm::{RoutineKey, RoutineScope, RoutineStore},
};
use wonderland_content_runtime_bridge::import_bhav;
use wonderland_game_runtime::{
    AcceptedCommand, EntityRef, Facing, GameRuntime, InteractionAccess, LotModel, PersistentId,
    PrincipalKey, QueryOptions, RuntimeConfig, RuntimeEvent, RuntimeRole, SpawnSpec, TilePos,
    VmMode,
    live_session::{
        Checkpoint, InteractionSelection, LiveReplica, ReplayLimits, SessionStatus, StreamIdentity,
        TickFrame,
    },
};
use wonderland_legacy_formats::{Limits, iff};

const OWNER: u32 = 0x0478_6aed;
const PRINCIPAL: PrincipalKey = PrincipalKey(42);

fn content() -> ContentSet {
    let file = iff::decode(
        include_bytes!(
            "../../../TSOClient/FSO.Content.TSO/Content/Objects/Casino_2-Tile_Bar_CC.iff"
        ),
        &Limits::default(),
    )
    .unwrap();
    let chunk = file
        .chunks
        .iter()
        .find(|chunk| chunk.key.kind == *b"BHAV" && chunk.key.id == 4110)
        .unwrap();
    let mut routines = RoutineStore::new();
    routines
        .insert(
            RoutineKey {
                scope: RoutineScope::Private(OWNER),
                id: 4110,
            },
            import_bhav(chunk, &Limits::default()).unwrap(),
        )
        .unwrap();
    let mut object = ObjectDefinition::new(OWNER, 4);
    object.attributes = vec![10, 20, 30, 40];
    ContentSet::new(routines, vec![object], vec![], TuningSet::default())
        .unwrap()
        .with_interaction_tables(vec![(
            OWNER,
            InteractionTable {
                local_table_present: true,
                definitions: vec![InteractionDefinition {
                    key: sim_core::interactions::InteractionKey {
                        tta_index: 7,
                        scope: InteractionScope::Local,
                    },
                    action: RoutineBinding {
                        routine_id: 4110,
                        code_owner_guid: OWNER,
                    },
                    check: None,
                    flags: ActionFlags(ActionFlags::ALLOW_VISITORS),
                    permissions: PermissionFlags::default(),
                    label: Some("Original routine".into()),
                }],
            },
        )])
        .unwrap()
}
fn runtime(role: RuntimeRole) -> GameRuntime {
    GameRuntime::new(
        content(),
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 11, 7, 123),
        role,
    )
    .unwrap()
}
fn step(server: &mut GameRuntime, commands: Vec<AcceptedCommand>) -> TickFrame {
    let accepted = server.sim().next_tick(commands).unwrap();
    let outcome = server.apply_accepted(&accepted).unwrap();
    TickFrame {
        accepted,
        state_hash: outcome.state_hash,
    }
}
fn initialized_server() -> (GameRuntime, EntityRef) {
    let mut server = runtime(RuntimeRole::Authority);
    let outcome = server
        .advance(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: OWNER,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        })])
        .unwrap();
    let actor = outcome
        .events
        .iter()
        .find_map(|event| match event {
            RuntimeEvent::Spawned(actor) => Some(*actor),
            _ => None,
        })
        .unwrap();
    step(
        &mut server,
        vec![AcceptedCommand::SetInteractionAuthority {
            actor,
            access: Some(InteractionAccess {
                principal: PRINCIPAL,
                allow_hidden: false,
            }),
        }],
    );
    (server, actor)
}
fn client(browser_epoch: u64) -> LiveReplica {
    LiveReplica::new(
        runtime(RuntimeRole::Replica),
        StreamIdentity {
            browser_epoch,
            source_epoch: 3,
            lot_incarnation: 4,
        },
        ReplayLimits::default(),
    )
    .unwrap()
}
fn install(client: &mut LiveReplica, server: &GameRuntime) {
    let bytes = server.snapshot().unwrap();
    client
        .install_checkpoint(
            client.checkpoint_ticket().unwrap(),
            Checkpoint {
                completed_tick: server.sim().state().completed_tick,
                state_hash: server.sim().state_hash().unwrap(),
                bytes: &bytes,
            },
            &[],
        )
        .unwrap();
}
fn select(client: &LiveReplica, actor: EntityRef, sequence: u64) -> InteractionSelection {
    let batch = client
        .offers(
            client.connection(),
            PRINCIPAL,
            actor,
            actor,
            QueryOptions::default(),
        )
        .unwrap();
    assert_eq!(batch.offers.len(), 1);
    assert_eq!(batch.offers[0].label, "Original routine");
    InteractionSelection {
        principal: PRINCIPAL,
        actor,
        target: actor,
        interaction: batch.offers[0].interaction,
        param0: batch.offers[0].param0,
        command_sequence: sequence,
    }
}

#[test]
fn original_action_executes_only_after_acceptance_and_matches_two_live_replicas() {
    let (mut server, actor) = initialized_server();
    let mut a = client(1);
    let mut b = client(2);
    install(&mut a, &server);
    install(&mut b, &server);
    let before = a.runtime().unwrap().snapshot().unwrap();
    let invocation = a
        .prepare_interaction(a.connection(), select(&a, actor, 1))
        .unwrap();
    assert_eq!(
        a.runtime().unwrap().snapshot().unwrap(),
        before,
        "preparing an intent cannot execute it"
    );
    let frame = step(
        &mut server,
        vec![AcceptedCommand::QueueInteraction(invocation)],
    );
    for replica in [&mut a, &mut b] {
        let outcomes = replica
            .apply_batch(replica.connection(), std::slice::from_ref(&frame))
            .unwrap();
        assert_eq!(outcomes.len(), 1);
        assert!(outcomes[0].effects.is_empty());
        assert!(outcomes[0].events.iter().any(|event| matches!(event, RuntimeEvent::Interaction(event) if matches!(event.kind, QueueEventKind::Finished { result: FinishResult::Succeeded }))));
        assert_eq!(
            replica.runtime().unwrap().sim().state().entities[&actor.object_id].attributes,
            [0, 0, 30, 0]
        );
        assert!(
            replica.runtime().unwrap().sim().state().interaction_queues[&actor.object_id]
                .entries()
                .is_empty()
        );
        assert_eq!(replica.projection().unwrap(), server.projection());
        assert_eq!(
            replica.runtime().unwrap().snapshot().unwrap(),
            server.snapshot().unwrap()
        );
        assert!(
            replica
                .apply_batch(replica.connection(), std::slice::from_ref(&frame))
                .unwrap()
                .is_empty(),
            "redelivery cannot repeat completion events"
        );
    }
}

#[test]
fn menu_selection_survives_ordinary_ticks_but_requeries_current_authority() {
    let (mut server, actor) = initialized_server();
    let mut client = client(1);
    install(&mut client, &server);
    let selection = select(&client, actor, 1);
    let tick = step(&mut server, vec![]);
    client.apply_batch(client.connection(), &[tick]).unwrap();
    let before = client.runtime().unwrap().snapshot().unwrap();
    client
        .prepare_interaction(client.connection(), selection)
        .unwrap();
    assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
    let revoke = step(
        &mut server,
        vec![AcceptedCommand::SetInteractionAuthority {
            actor,
            access: None,
        }],
    );
    client.apply_batch(client.connection(), &[revoke]).unwrap();
    assert!(
        client
            .prepare_interaction(client.connection(), selection)
            .is_err(),
        "an open menu must not retain revoked access"
    );
}

#[test]
fn reconnect_replays_original_action_without_republishing_historical_events() {
    let (mut server, actor) = initialized_server();
    let mut client = client(1);
    install(&mut client, &server);
    let bytes = server.snapshot().unwrap();
    let hash = server.sim().state_hash().unwrap();
    let at = server.sim().state().completed_tick;
    let invocation = client
        .prepare_interaction(client.connection(), select(&client, actor, 1))
        .unwrap();
    let token = client.connection();
    client.suspend(token).unwrap();
    let frame = step(
        &mut server,
        vec![AcceptedCommand::QueueInteraction(invocation)],
    );
    client.reconnect().unwrap();
    let recovered = client
        .install_checkpoint(
            client.checkpoint_ticket().unwrap(),
            Checkpoint {
                completed_tick: at,
                state_hash: hash,
                bytes: &bytes,
            },
            std::slice::from_ref(&frame),
        )
        .unwrap();
    assert_eq!(recovered.state_hash, frame.state_hash);
    assert_eq!(
        client.runtime().unwrap().snapshot().unwrap(),
        server.snapshot().unwrap()
    );
    assert!(
        client
            .apply_batch(token, std::slice::from_ref(&frame))
            .is_err()
    );
    assert!(
        client
            .apply_batch(client.connection(), &[frame])
            .unwrap()
            .is_empty()
    );
    assert_eq!(client.status(), SessionStatus::Live);
}

#[test]
fn invalid_tail_rolls_back_real_action_mutations_and_queue_completion() {
    let (mut server, actor) = initialized_server();
    let mut client = client(1);
    install(&mut client, &server);
    let before = client.runtime().unwrap().snapshot().unwrap();
    let invocation = client
        .prepare_interaction(client.connection(), select(&client, actor, 1))
        .unwrap();
    let action = step(
        &mut server,
        vec![AcceptedCommand::QueueInteraction(invocation)],
    );
    let mut invalid = step(&mut server, vec![]);
    invalid.state_hash[0] ^= 1;
    assert!(
        client
            .apply_batch(client.connection(), &[action, invalid])
            .is_err()
    );
    assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
    assert_eq!(
        client.runtime().unwrap().sim().state().entities[&actor.object_id].attributes,
        [10, 20, 30, 40]
    );
    assert_eq!(client.status(), SessionStatus::AwaitingCheckpoint);
    assert!(client.projection().is_err());
    install(&mut client, &server);
    assert_eq!(
        client.runtime().unwrap().snapshot().unwrap(),
        server.snapshot().unwrap()
    );
}

#[test]
fn accepted_cancellation_replays_without_executing_the_original_action() {
    let (mut server, actor) = initialized_server();
    let mut client = client(1);
    install(&mut client, &server);
    let invocation = client
        .prepare_interaction(client.connection(), select(&client, actor, 1))
        .unwrap();
    let cancel = CancelIntent {
        principal: PRINCIPAL,
        world_revision: invocation.seen.world_revision,
        actor: invocation.seen.actor,
        queue_revision: 1,
        command_sequence: 2,
        action: ActionId(1),
    };
    let frame = step(
        &mut server,
        vec![
            AcceptedCommand::QueueInteraction(invocation),
            AcceptedCommand::CancelInteraction(cancel),
        ],
    );
    let outcomes = client.apply_batch(client.connection(), &[frame]).unwrap();
    assert_eq!(
        client.runtime().unwrap().sim().state().entities[&actor.object_id].attributes,
        [10, 20, 30, 40]
    );
    assert!(outcomes[0].events.iter().any(|event| matches!(event, RuntimeEvent::Interaction(event) if matches!(event.kind, QueueEventKind::Removed { reason: RemovalReason::Cancelled }))));
    assert!(!outcomes[0].events.iter().any(|event| matches!(event, RuntimeEvent::Interaction(event) if matches!(event.kind, QueueEventKind::Started { .. }))));
    assert_eq!(
        client.runtime().unwrap().snapshot().unwrap(),
        server.snapshot().unwrap()
    );
}
