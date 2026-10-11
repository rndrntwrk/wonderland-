//! Native/WASM conformance through the actual WLR1 codec and LiveReplica.
//! Controlled declared metadata around unchanged source BHAVs, not a game server.
#[path = "../tests/live_session/support.rs"]
mod support;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use support::{PRINCIPAL, content, identity, pair_with_content, source_selection};
use wonderland_game_runtime::live_session::{
    Checkpoint, LiveReplica, ReplayLimits, SessionStatus, TickFrame,
};
use wonderland_game_runtime::live_wire::{
    CheckpointRequest, NativeWire, Received, WireLimits, decode_checkpoint_request,
    encode_checkpoint, encode_checkpoint_request, encode_ticks,
};
use wonderland_game_runtime::sim_core::vm::{EntityField, MemoryAddress};
use wonderland_game_runtime::{
    AcceptedCommand, GameRuntime, LotModel, QueryOptions, RuntimeConfig, RuntimeRole, TickOutcome,
    VmMode,
};

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").unwrap();
    }
    text
}
fn record(wire: &NativeWire) -> Value {
    let runtime = wire.replica().runtime().unwrap();
    let projection = wire.replica().projection().unwrap();
    let snapshot = runtime.snapshot().unwrap();
    json!({"tick":projection.tick, "state_hash":hex(&runtime.sim().state_hash().unwrap()),
        "snapshot_hex":hex(&snapshot), "snapshot_sha256":hex(&Sha256::digest(&snapshot)),
        "entities":projection.entities, "queues":projection.queues})
}
fn install(wire: &mut NativeWire, server: &GameRuntime) {
    let request = wire.checkpoint_request().unwrap();
    let decoded = decode_checkpoint_request(&encode_checkpoint_request(request).unwrap()).unwrap();
    assert_eq!(decoded.id, request.id);
    assert_eq!(decoded.cursor, request.cursor);
    let snapshot = server.snapshot().unwrap();
    let raw = encode_checkpoint(
        decoded.id,
        Checkpoint {
            completed_tick: server.sim().state().completed_tick,
            state_hash: server.sim().state_hash().unwrap(),
            bytes: &snapshot,
        },
        &[],
        WireLimits::default(),
    )
    .unwrap();
    assert!(matches!(
        wire.receive(wire.connection(), &raw).unwrap(),
        Received::Checkpoint(_)
    ));
}
fn advance(server: &mut GameRuntime, commands: Vec<AcceptedCommand>) -> (TickFrame, TickOutcome) {
    let accepted = server.sim().next_tick(commands).unwrap();
    let outcome = server.apply_accepted(&accepted).unwrap();
    (
        TickFrame {
            accepted,
            state_hash: outcome.state_hash,
        },
        outcome,
    )
}
fn apply(wire: &mut NativeWire, frame: &TickFrame, expected: &TickOutcome) -> String {
    let bytes = encode_ticks(std::slice::from_ref(frame), WireLimits::default()).unwrap();
    let Received::Ticks(outcomes) = wire.receive(wire.connection(), &bytes).unwrap() else {
        panic!()
    };
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].events, expected.events);
    assert_eq!(outcomes[0].instructions, expected.instructions);
    assert_eq!(outcomes[0].state_hash, expected.state_hash);
    assert!(outcomes[0].effects.is_empty());
    hex(&Sha256::digest(&bytes))
}
fn actions() -> Value {
    let (mut server, a, actor) =
        support::server_and_client("Casino_2-Tile_Bar_CC.iff", 4110, false);
    let b = LiveReplica::new(
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
    let mut a = NativeWire::new(a, WireLimits::default()).unwrap();
    let mut b = NativeWire::new(b, WireLimits::default()).unwrap();
    install(&mut a, &server);
    install(&mut b, &server);
    let start_bytes = server.snapshot().unwrap();
    let start = Checkpoint {
        completed_tick: 2,
        state_hash: server.sim().state_hash().unwrap(),
        bytes: &start_bytes,
    };
    let selection = source_selection(a.replica(), actor, 0);
    let mut tail = Vec::new();
    let mut trace = Vec::new();
    let mut completed = Value::Null;
    for index in 0..60 {
        let commands = if index == 1 {
            let before = a.replica().runtime().unwrap().snapshot().unwrap();
            let intent = a
                .replica()
                .prepare_interaction(a.connection(), selection)
                .unwrap();
            assert_eq!(a.replica().runtime().unwrap().snapshot().unwrap(), before);
            vec![AcceptedCommand::QueueInteraction(intent)]
        } else {
            vec![]
        };
        let (frame, outcome) = advance(&mut server, commands);
        let wire_hash = apply(&mut a, &frame, &outcome);
        assert_eq!(apply(&mut b, &frame, &outcome), wire_hash);
        assert_eq!(
            a.replica().runtime().unwrap().snapshot().unwrap(),
            server.snapshot().unwrap()
        );
        assert_eq!(
            b.replica().runtime().unwrap().snapshot().unwrap(),
            server.snapshot().unwrap()
        );
        if index == 1 {
            assert_eq!(
                server.sim().state().entities[&actor.object_id].attributes,
                [0, 0, 30, 0]
            );
            assert!(
                a.replica().projection().unwrap().queues[0]
                    .entries
                    .is_empty()
            );
            assert!(
                a.replica()
                    .prepare_interaction(a.connection(), selection)
                    .is_err()
            );
            completed = record(&a);
        }
        trace.push(json!({"tick":outcome.tick,"hash":hex(&outcome.state_hash),
            "packet_sha256":wire_hash,"events":format!("{:?}",outcome.events),"instructions":outcome.instructions}));
        tail.push(frame);
    }
    let old = b.connection();
    b.disconnect(old).unwrap();
    b.reconnect().unwrap();
    let (next, outcome) = advance(&mut server, vec![]);
    apply(&mut a, &next, &outcome);
    tail.push(next);
    assert!(b.receive(old, b"malformed obsolete socket").is_err());
    assert_eq!(b.replica().status(), SessionStatus::AwaitingCheckpoint);
    let recovery_packet = encode_checkpoint(
        b.checkpoint_request().unwrap().id,
        start,
        &tail,
        WireLimits::default(),
    )
    .unwrap();
    let Received::Checkpoint(recovered) = b.receive(b.connection(), &recovery_packet).unwrap()
    else {
        panic!()
    };
    assert_eq!(recovered.completed_tick, 63);
    assert_eq!(record(&a), record(&b));
    let before = a.replica().runtime().unwrap().snapshot().unwrap();
    let (first, one) = advance(&mut server, vec![]);
    let (second, two) = advance(&mut server, vec![]);
    let mut bad = second.clone();
    bad.state_hash[0] ^= 1;
    let packet = encode_ticks(&[first.clone(), bad], WireLimits::default()).unwrap();
    assert!(a.receive(a.connection(), &packet).is_err());
    assert_eq!(a.replica().runtime().unwrap().snapshot().unwrap(), before);
    assert_eq!(a.replica().status(), SessionStatus::AwaitingCheckpoint);
    apply(&mut b, &first, &one);
    apply(&mut b, &second, &two);
    install(&mut a, &server);
    assert_eq!(record(&a), record(&b));
    let duplicate = encode_ticks(&[second], WireLimits::default()).unwrap();
    let Received::Ticks(duplicate) = a.receive(a.connection(), &duplicate).unwrap() else {
        panic!()
    };
    assert!(duplicate.is_empty());
    json!({"source":"Casino_2-Tile_Bar_CC.iff/BHAV/4110","attributes":[0,0,30,0],
        "trace":trace,"completed_action":completed,"recovered_tick":recovered.completed_tick,
        "recovery_packet_sha256":hex(&Sha256::digest(&recovery_packet)),
        "atomic_failure_preserved":true,"duplicate_outcomes":duplicate.len(),
        "replica_a":record(&a),"replica_b":record(&b)})
}
fn needs() -> Value {
    let (mut server, replica, actor) =
        pair_with_content(content("cursebook_set_permission.iff", 4107, false), true);
    let mut wire = NativeWire::new(replica, WireLimits::default()).unwrap();
    install(&mut wire, &server);
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
    apply(&mut wire, &frame, &outcome);
    let before = wire.replica().projection().unwrap().entities[0]
        .needs
        .clone()
        .unwrap();
    assert_eq!((before.energy, before.hunger, before.hygiene), (50, 50, 50));
    let selection = source_selection(wire.replica(), actor, 0);
    let intent = wire
        .replica()
        .prepare_interaction(wire.connection(), selection)
        .unwrap();
    let (frame, outcome) = advance(&mut server, vec![AcceptedCommand::QueueInteraction(intent)]);
    let packet_hash = apply(&mut wire, &frame, &outcome);
    let after = wire.replica().projection().unwrap().entities[0]
        .needs
        .clone()
        .unwrap();
    assert_eq!((after.energy, after.hunger, after.hygiene), (100, 100, 100));
    assert_eq!(after.room, None);
    assert_eq!(
        wire.replica().runtime().unwrap().snapshot().unwrap(),
        server.snapshot().unwrap()
    );
    json!({"source":"cursebook_set_permission.iff/BHAV/4107","before":before,"after":after,
        "packet_sha256":packet_hash,"final":record(&wire)})
}
fn menu() -> Value {
    let (server, replica, actor) = support::server_and_client("fso_christmas_flag.iff", 4108, true);
    let mut wire = NativeWire::new(replica, WireLimits::default()).unwrap();
    install(&mut wire, &server);
    let before = wire.replica().runtime().unwrap().snapshot().unwrap();
    let batch = wire
        .replica()
        .offers(
            wire.connection(),
            PRINCIPAL,
            actor,
            actor,
            QueryOptions::default(),
        )
        .unwrap();
    let offers: Vec<_> = batch
        .offers
        .iter()
        .map(|offer| json!({"label":offer.label,"param0":offer.param0}))
        .collect();
    for param0 in 0..=2 {
        let intent = wire
            .replica()
            .prepare_interaction(
                wire.connection(),
                source_selection(wire.replica(), actor, param0),
            )
            .unwrap();
        assert_eq!(intent.param0, param0);
    }
    let mut bad = source_selection(wire.replica(), actor, 0);
    bad.param0 = 37;
    assert!(
        wire.replica()
            .prepare_interaction(wire.connection(), bad)
            .is_err()
    );
    assert_eq!(
        wire.replica().runtime().unwrap().snapshot().unwrap(),
        before
    );
    json!({"source":"fso_christmas_flag.iff/BHAV/4108","offers":offers,
        "invented_parameter_rejected":true,"final":record(&wire)})
}
fn run() -> Vec<u8> {
    let request = CheckpointRequest {
        id: u64::MAX,
        cursor: Some(wonderland_game_runtime::live_session::ReplicaCursor {
            lot_id: u64::MAX,
            authority_epoch: u64::MAX - 1,
            completed_tick: u64::MAX - 2,
            state_hash: [17; 32],
        }),
    };
    let bytes = encode_checkpoint_request(request).unwrap();
    let decoded = decode_checkpoint_request(&bytes).unwrap();
    assert_eq!(decoded.id, request.id);
    assert_eq!(decoded.cursor, request.cursor);
    serde_json::to_vec(
        &json!({"schema":1,"protocol":"native-WLR1","fixture":"declared-harness-original-BHAV",
        "request_id":decoded.id.to_string(),"request_hex":hex(&bytes),
        "actions":actions(),"needs":needs(),"menu":menu()}),
    )
    .unwrap()
}
static OUTPUT: OnceLock<Vec<u8>> = OnceLock::new();
// Unique probe-only exports. Immutable process-lifetime bytes; no supplied pointers are read.
#[unsafe(no_mangle)]
pub extern "C" fn wonderland_live_wire_probe_ptr() -> *const u8 {
    OUTPUT.get_or_init(run).as_ptr()
}
#[unsafe(no_mangle)]
pub extern "C" fn wonderland_live_wire_probe_len() -> usize {
    OUTPUT.get_or_init(run).len()
}
fn main() {
    println!("{}", std::str::from_utf8(OUTPUT.get_or_init(run)).unwrap());
}
