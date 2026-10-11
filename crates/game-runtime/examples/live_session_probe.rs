//! Cross-target conformance, NOT a network server or production content loader.
//! All scenario setup is explicit; the selected original BHAV bytes are unchanged.
#[path = "../tests/live_session/support.rs"]
mod support;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sim_core::vm::{EntityField, MemoryAddress};
use std::sync::OnceLock;
use support::{PRINCIPAL, content, identity, install, pair_with_content, source_selection};
use wonderland_game_runtime::live_session::{
    Checkpoint, LiveReplica, ReplayLimits, SessionStatus, TickFrame,
};
use wonderland_game_runtime::{
    AcceptedCommand, EntityRef, GameRuntime, LotModel, QueryOptions, RuntimeConfig, RuntimeRole,
    TickOutcome, VmMode,
};

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(text, "{byte:02x}").unwrap();
    }
    text
}
fn record(replica: &LiveReplica) -> Value {
    let runtime = replica.runtime().unwrap();
    let projection = replica.projection().unwrap();
    let snapshot = runtime.snapshot().unwrap();
    json!({
        "tick": projection.tick,
        "state_hash": hex(&runtime.sim().state_hash().unwrap()),
        "snapshot_sha256": hex(&Sha256::digest(&snapshot)),
        "snapshot_hex": hex(&snapshot),
        "entities": projection.entities,
        "queues": projection.queues,
    })
}
fn advance(server: &mut GameRuntime, commands: Vec<AcceptedCommand>) -> (TickFrame, TickOutcome) {
    let accepted = server.sim().next_tick(commands).unwrap();
    let outcome = server.apply_accepted(&accepted).unwrap();
    let frame = TickFrame {
        accepted,
        state_hash: outcome.state_hash,
    };
    (frame, outcome)
}
fn apply(replica: &mut LiveReplica, frame: &TickFrame, outcome: &TickOutcome) {
    let result = replica
        .apply_batch(replica.connection(), std::slice::from_ref(frame))
        .unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].events, outcome.events);
    assert_eq!(result[0].instructions, outcome.instructions);
    assert_eq!(result[0].state_hash, outcome.state_hash);
    assert!(result[0].effects.is_empty());
}
fn invoke(replica: &LiveReplica, actor: EntityRef) -> wonderland_game_runtime::InteractionIntent {
    let before = replica.runtime().unwrap().snapshot().unwrap();
    let result = replica
        .prepare_interaction(replica.connection(), source_selection(replica, actor, 0))
        .unwrap();
    assert_eq!(replica.runtime().unwrap().snapshot().unwrap(), before);
    result
}

fn original_action_and_recovery() -> Value {
    let (mut server, mut a, actor) =
        support::server_and_client("Casino_2-Tile_Bar_CC.iff", 4110, false);
    let mut b = LiveReplica::new(
        GameRuntime::new(
            server.sim().content().clone(),
            LotModel::new(8, 8, 1).unwrap(),
            RuntimeConfig::new(VmMode::Ts1, 11, 7, 123),
            RuntimeRole::Replica,
        )
        .unwrap(),
        wonderland_game_runtime::live_session::StreamIdentity {
            browser_epoch: 99,
            ..identity()
        },
        ReplayLimits::default(),
    )
    .unwrap();
    install(&mut b, &server, &[]);
    let initial_bytes = server.snapshot().unwrap();
    let initial = Checkpoint {
        completed_tick: 2,
        state_hash: server.sim().state_hash().unwrap(),
        bytes: &initial_bytes,
    };
    let selection = source_selection(&a, actor, 0);
    let mut tail = Vec::new();
    let mut trace = Vec::new();
    let mut completed = Value::Null;
    for index in 0..61 {
        let commands = if index == 1 {
            // Selection was displayed before an ordinary tick. Resolve it again
            // with the exact source key/Param0, not a guessed menu index.
            let before = a.runtime().unwrap().snapshot().unwrap();
            let intent = a.prepare_interaction(a.connection(), selection).unwrap();
            assert_eq!(a.runtime().unwrap().snapshot().unwrap(), before);
            vec![AcceptedCommand::QueueInteraction(intent)]
        } else {
            vec![]
        };
        let (frame, outcome) = advance(&mut server, commands);
        apply(&mut a, &frame, &outcome);
        apply(&mut b, &frame, &outcome);
        assert_eq!(
            a.runtime().unwrap().snapshot().unwrap(),
            server.snapshot().unwrap()
        );
        assert_eq!(
            b.runtime().unwrap().snapshot().unwrap(),
            server.snapshot().unwrap()
        );
        trace.push(
            json!({"tick": outcome.tick, "hash": hex(&outcome.state_hash),
            "events": format!("{:?}", outcome.events), "instructions": outcome.instructions}),
        );
        if index == 1 {
            assert_eq!(
                a.runtime().unwrap().sim().state().entities[&actor.object_id].attributes,
                [0, 0, 30, 0]
            );
            assert!(a.projection().unwrap().queues[0].entries.is_empty());
            assert!(a.prepare_interaction(a.connection(), selection).is_err());
            completed = record(&a);
        }
        tail.push(frame);
    }
    // No presentation output is returned for the already accepted latest tick.
    assert!(
        a.apply_batch(a.connection(), &tail[tail.len() - 1..])
            .unwrap()
            .is_empty()
    );
    let old_socket = b.connection();
    b.suspend(old_socket).unwrap();
    b.reconnect().unwrap();
    let (next, next_outcome) = advance(&mut server, vec![]);
    apply(&mut a, &next, &next_outcome);
    tail.push(next.clone());
    assert!(
        b.apply_batch(old_socket, std::slice::from_ref(&next))
            .is_err()
    );
    assert!(b.projection().is_err());
    let recovered = b
        .install_checkpoint(b.checkpoint_ticket().unwrap(), initial, &tail)
        .unwrap();
    assert_eq!(recovered.completed_tick, 64);
    assert_eq!(recovered.state_hash, next.state_hash);
    assert_eq!(
        b.runtime().unwrap().snapshot().unwrap(),
        server.snapshot().unwrap()
    );
    // A bad second tick must roll back the valid first tick and publish neither.
    let before = a.runtime().unwrap().snapshot().unwrap();
    let (first, first_outcome) = advance(&mut server, vec![]);
    let (second, second_outcome) = advance(&mut server, vec![]);
    let mut corrupt = second.clone();
    corrupt.state_hash[0] ^= 1;
    assert!(
        a.apply_batch(a.connection(), &[first.clone(), corrupt])
            .is_err()
    );
    assert_eq!(a.runtime().unwrap().snapshot().unwrap(), before);
    assert_eq!(a.status(), SessionStatus::AwaitingCheckpoint);
    apply(&mut b, &first, &first_outcome);
    apply(&mut b, &second, &second_outcome);
    install(&mut a, &server, &[]);
    assert_eq!(
        a.runtime().unwrap().snapshot().unwrap(),
        b.runtime().unwrap().snapshot().unwrap()
    );
    assert!(
        a.apply_batch(a.connection(), std::slice::from_ref(&second))
            .unwrap()
            .is_empty()
    );
    json!({"source": "Casino_2-Tile_Bar_CC.iff/BHAV/4110", "attributes": [0,0,30,0],
        "trace": trace, "completed_action": completed, "recovered_tick": recovered.completed_tick,
        "atomic_failure_preserved": true, "duplicate_outcomes": 0,
        "replica_a": record(&a), "replica_b": record(&b)})
}

fn dynamic_offers() -> Value {
    let (_, client, actor) = support::server_and_client("fso_christmas_flag.iff", 4108, true);
    let before = client.runtime().unwrap().snapshot().unwrap();
    let batch = client
        .offers(
            client.connection(),
            PRINCIPAL,
            actor,
            actor,
            QueryOptions::default(),
        )
        .unwrap();
    let offers: Vec<_> = batch
        .offers
        .iter()
        .map(|offer| {
            json!({
                "label": offer.label, "param0": offer.param0, "key": offer.interaction,
            })
        })
        .collect();
    assert_eq!(
        offers
            .iter()
            .map(|offer| (
                offer["label"].as_str().unwrap(),
                offer["param0"].as_i64().unwrap()
            ))
            .collect::<Vec<_>>(),
        [
            ("Debug/Set Team/None", 0),
            ("Debug/Set Team/Elves", 1),
            ("Debug/Set Team/Reindeer", 2)
        ]
    );
    for param0 in 0..=2 {
        let selection = source_selection(&client, actor, param0);
        let intent = client
            .prepare_interaction(client.connection(), selection)
            .unwrap();
        assert_eq!(intent.param0, param0);
    }
    let mut invented = source_selection(&client, actor, 0);
    invented.param0 = 37;
    assert!(
        client
            .prepare_interaction(client.connection(), invented)
            .is_err()
    );
    assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
    json!({"source":"fso_christmas_flag.iff/BHAV/4108", "offers":offers,
        "invented_parameter_rejected":true, "final":record(&client)})
}

fn original_needs() -> Value {
    let (mut server, mut client, actor) =
        pair_with_content(content("cursebook_set_permission.iff", 4107, false), true);
    let (frame, outcome) = advance(
        &mut server,
        [5, 7, 8]
            .into_iter()
            .map(|index| AcceptedCommand::WriteMemory {
                address: MemoryAddress::Entity {
                    entity: actor,
                    field: EntityField::Motive,
                    index,
                },
                value: 0,
            })
            .collect(),
    );
    apply(&mut client, &frame, &outcome);
    let needs_before = client.projection().unwrap().entities[0]
        .needs
        .clone()
        .unwrap();
    assert_eq!(
        (
            needs_before.energy,
            needs_before.hunger,
            needs_before.hygiene
        ),
        (50, 50, 50)
    );
    let intent = invoke(&client, actor);
    let (frame, outcome) = advance(&mut server, vec![AcceptedCommand::QueueInteraction(intent)]);
    apply(&mut client, &frame, &outcome);
    let projection = client.projection().unwrap();
    let needs_after = projection.entities[0].needs.clone().unwrap();
    assert_eq!(
        (needs_after.energy, needs_after.hunger, needs_after.hygiene),
        (100, 100, 100)
    );
    assert_eq!(needs_after.room, None);
    assert!(projection.queues[0].entries.is_empty());
    assert_eq!(
        client.runtime().unwrap().snapshot().unwrap(),
        server.snapshot().unwrap()
    );
    json!({"source":"cursebook_set_permission.iff/BHAV/4107", "before":needs_before,"after":needs_after,
        "ordered_events":format!("{:?}",outcome.events),"final":record(&client)})
}
fn run() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schema":1, "protocol":"native-A-completed-tick", "fixture":"declared-harness-original-BHAV",
        "actions": original_action_and_recovery(), "menu": dynamic_offers(), "needs": original_needs(),
    })).unwrap()
}
static OUTPUT: OnceLock<Vec<u8>> = OnceLock::new();

// SAFETY: unique probe-only export names, with an immutable process-lifetime
// buffer. No caller-provided pointer is dereferenced, and the host cannot free it.
#[unsafe(no_mangle)]
pub extern "C" fn wonderland_live_probe_ptr() -> *const u8 {
    OUTPUT.get_or_init(run).as_ptr()
}
#[unsafe(no_mangle)]
pub extern "C" fn wonderland_live_probe_len() -> usize {
    OUTPUT.get_or_init(run).len()
}
fn main() {
    println!("{}", std::str::from_utf8(OUTPUT.get_or_init(run)).unwrap());
}
