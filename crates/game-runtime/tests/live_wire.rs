//! Native protocol tests with declared harness metadata around unchanged source BHAV.
#[path = "live_session/support.rs"]
mod support;
use support::{server_and_client, source_selection};
use wonderland_game_runtime::live_session::{Checkpoint, SessionStatus, TickFrame};
use wonderland_game_runtime::live_wire::{
    NativeWire, Packet, Received, WireError, WireLimits, decode_packet, encode_checkpoint,
    encode_ticks,
};
use wonderland_game_runtime::{AcceptedCommand, GameRuntime};

fn pair() -> (GameRuntime, NativeWire, wonderland_game_runtime::EntityRef) {
    let (server, replica, actor) = server_and_client("Casino_2-Tile_Bar_CC.iff", 4110, false);
    let wire = NativeWire::new(replica, WireLimits::default()).unwrap();
    (server, wire, actor)
}
fn checkpoint(wire: &NativeWire, server: &GameRuntime, tail: &[TickFrame]) -> Vec<u8> {
    let bytes = server.snapshot().unwrap();
    encode_checkpoint(
        wire.checkpoint_request().unwrap().id,
        Checkpoint {
            completed_tick: server.sim().state().completed_tick,
            state_hash: server.sim().state_hash().unwrap(),
            bytes: &bytes,
        },
        tail,
        WireLimits::default(),
    )
    .unwrap()
}
fn install(wire: &mut NativeWire, server: &GameRuntime) {
    let bytes = checkpoint(wire, server, &[]);
    assert!(matches!(
        wire.receive(wire.connection(), &bytes).unwrap(),
        Received::Checkpoint(_)
    ));
}
fn tick(server: &mut GameRuntime, commands: Vec<AcceptedCommand>) -> TickFrame {
    let accepted = server.sim().next_tick(commands).unwrap();
    let outcome = server.apply_accepted(&accepted).unwrap();
    TickFrame {
        accepted,
        state_hash: outcome.state_hash,
    }
}

#[test]
fn native_checkpoint_is_borrowed_and_u64_metadata_is_exact() {
    let checkpoint = Checkpoint {
        completed_tick: u64::MAX,
        state_hash: [21; 32],
        bytes: &[1, 2, 3],
    };
    let packet = encode_checkpoint(u64::MAX, checkpoint, &[], WireLimits::default()).unwrap();
    let Packet::Checkpoint {
        request_id,
        checkpoint: actual,
        tail,
    } = decode_packet(&packet, WireLimits::default()).unwrap()
    else {
        panic!("checkpoint expected")
    };
    assert_eq!(request_id, u64::MAX);
    assert_eq!(actual.completed_tick, u64::MAX);
    assert_eq!(actual.bytes, checkpoint.bytes);
    assert_eq!(actual.state_hash, [21; 32]);
    assert!(tail.is_empty());
    let start = packet.as_ptr() as usize;
    let end = start + packet.len();
    assert!((start..end).contains(&(actual.bytes.as_ptr() as usize)));
}

#[test]
fn accepted_ticks_roundtrip_and_execute_original_behavior() {
    let (mut server, mut wire, actor) = pair();
    install(&mut wire, &server);
    let before = wire.replica().runtime().unwrap().snapshot().unwrap();
    let selection = source_selection(wire.replica(), actor, 0);
    let intent = wire
        .replica()
        .prepare_interaction(wire.connection(), selection)
        .unwrap();
    assert_eq!(
        before,
        wire.replica().runtime().unwrap().snapshot().unwrap()
    );
    let frames = [
        tick(&mut server, vec![AcceptedCommand::QueueInteraction(intent)]),
        tick(&mut server, vec![]),
    ];
    let encoded = encode_ticks(&frames, WireLimits::default()).unwrap();
    let Packet::Ticks(decoded) = decode_packet(&encoded, WireLimits::default()).unwrap() else {
        panic!()
    };
    assert_eq!(decoded, frames);
    let Received::Ticks(outcomes) = wire.receive(wire.connection(), &encoded).unwrap() else {
        panic!()
    };
    assert_eq!(outcomes.len(), 2);
    assert_eq!(
        server.snapshot().unwrap(),
        wire.replica().runtime().unwrap().snapshot().unwrap()
    );
    assert!(outcomes.iter().all(|o| o.effects.is_empty()));
    assert!(outcomes.iter().any(|o| o.instructions > 0));
    let repeated_last_tick = encode_ticks(&frames[1..], WireLimits::default()).unwrap();
    let Received::Ticks(outcomes) = wire
        .receive(wire.connection(), &repeated_last_tick)
        .unwrap()
    else {
        panic!()
    };
    assert!(
        outcomes.is_empty(),
        "replayed deliveries must not replay presentation outcomes"
    );
}

#[test]
fn one_bad_hash_rolls_back_the_entire_wire_batch_and_requests_recovery() {
    let (mut server, mut wire, _) = pair();
    install(&mut wire, &server);
    let before = wire.replica().runtime().unwrap().snapshot().unwrap();
    let mut frames = [tick(&mut server, vec![]), tick(&mut server, vec![])];
    frames[1].state_hash[0] ^= 1;
    let packet = encode_ticks(&frames, WireLimits::default()).unwrap();
    assert!(wire.receive(wire.connection(), &packet).is_err());
    assert_eq!(
        before,
        wire.replica().runtime().unwrap().snapshot().unwrap()
    );
    assert_eq!(wire.replica().status(), SessionStatus::AwaitingCheckpoint);
    assert!(wire.checkpoint_request().is_some());
    install(&mut wire, &server);
    assert_eq!(
        server.snapshot().unwrap(),
        wire.replica().runtime().unwrap().snapshot().unwrap()
    );
}

#[test]
fn reconnect_fences_old_callbacks_before_decoding_their_payload() {
    let (server, mut wire, _) = pair();
    let old_connection = wire.connection();
    let old_packet = checkpoint(&wire, &server, &[]);
    let old_request = wire.checkpoint_request().unwrap().id;
    wire.disconnect(old_connection).unwrap();
    let connection = wire.reconnect().unwrap();
    let current_request = wire.checkpoint_request().unwrap().id;
    assert!(current_request > old_request);
    assert!(matches!(
        wire.receive(old_connection, b"malformed"),
        Err(WireError::StaleConnection)
    ));
    assert!(matches!(
        wire.receive(connection, &old_packet),
        Err(WireError::StaleResponse)
    ));
    assert!(matches!(
        wire.disconnect(old_connection),
        Err(WireError::StaleConnection)
    ));
    assert_eq!(wire.checkpoint_request().unwrap().id, current_request);
    install(&mut wire, &server);
    assert_eq!(wire.replica().status(), SessionStatus::Live);
}

#[test]
fn malformed_current_delivery_suspends_authority_and_never_falls_back() {
    let (server, mut wire, _) = pair();
    install(&mut wire, &server);
    let before = wire.replica().runtime().unwrap().snapshot().unwrap();
    assert!(wire.receive(wire.connection(), b"FSOv\0\0\0\0").is_err());
    assert_eq!(wire.replica().status(), SessionStatus::Suspended);
    assert!(wire.replica().projection().is_err());
    assert!(wire.checkpoint_request().is_none());
    assert_eq!(
        before,
        wire.replica().runtime().unwrap().snapshot().unwrap()
    );
}

#[test]
fn ticks_before_checkpoint_fail_closed_instead_of_being_dropped() {
    let (mut server, mut wire, _) = pair();
    let packet = encode_ticks(&[tick(&mut server, vec![])], WireLimits::default()).unwrap();
    assert!(wire.receive(wire.connection(), &packet).is_err());
    assert_eq!(wire.replica().status(), SessionStatus::Suspended);
}

#[test]
fn checkpoint_tail_commits_without_emitting_historical_events() {
    let (mut server, mut wire, actor) = pair();
    let bytes = server.snapshot().unwrap();
    let completed_tick = server.sim().state().completed_tick;
    let state_hash = server.sim().state_hash().unwrap();
    // Prepare on a separate admitted replica; the receiver is still recovering.
    let (_, other, _) = server_and_client("Casino_2-Tile_Bar_CC.iff", 4110, false);
    let intent = other
        .prepare_interaction(other.connection(), source_selection(&other, actor, 0))
        .unwrap();
    let frames = [tick(
        &mut server,
        vec![AcceptedCommand::QueueInteraction(intent)],
    )];
    let packet = encode_checkpoint(
        wire.checkpoint_request().unwrap().id,
        Checkpoint {
            completed_tick,
            state_hash,
            bytes: &bytes,
        },
        &frames,
        WireLimits::default(),
    )
    .unwrap();
    assert!(matches!(
        wire.receive(wire.connection(), &packet).unwrap(),
        Received::Checkpoint(_)
    ));
    assert_eq!(
        wire.replica().runtime().unwrap().snapshot().unwrap(),
        server.snapshot().unwrap()
    );
}

#[test]
fn checkpoint_corruption_does_not_replace_committed_state() {
    let (server, mut wire, _) = pair();
    let before = wire.replica().runtime().unwrap().snapshot().unwrap();
    let mut packet = checkpoint(&wire, &server, &[]);
    packet[32 + 8] ^= 1; // Authority checkpoint hash.
    assert!(wire.receive(wire.connection(), &packet).is_err());
    assert_eq!(
        before,
        wire.replica().runtime().unwrap().snapshot().unwrap()
    );
    assert_eq!(wire.replica().status(), SessionStatus::Suspended);
}

#[test]
fn header_rejects_wrong_version_reserved_flags_kind_and_length() {
    let packet = encode_ticks(&[], WireLimits::default()).unwrap();
    for offset in [0, 3, 8, 9, 15, 16, 24] {
        let mut corrupt = packet.clone();
        corrupt[offset] ^= 0xff;
        assert!(
            decode_packet(&corrupt, WireLimits::default()).is_err(),
            "offset {offset}"
        );
    }
    let mut extra = packet.clone();
    extra.push(0);
    assert!(decode_packet(&extra, WireLimits::default()).is_err());
    for end in 0..packet.len() {
        assert!(decode_packet(&packet[..end], WireLimits::default()).is_err());
    }
}

#[test]
fn checkpoint_request_zero_is_never_valid() {
    let raw = [1];
    assert!(
        encode_checkpoint(
            0,
            Checkpoint {
                completed_tick: 0,
                state_hash: [0; 32],
                bytes: &raw
            },
            &[],
            WireLimits::default()
        )
        .is_err()
    );
}

#[test]
fn count_and_payload_limits_are_checked_before_decoding() {
    let (mut server, _, _) = pair();
    let frames = [tick(&mut server, vec![]), tick(&mut server, vec![])];
    let bytes = encode_ticks(&frames, WireLimits::default()).unwrap();
    let mut limits = WireLimits::default();
    limits.replay.max_batch_ticks = 1;
    assert!(decode_packet(&bytes, limits).is_err());
    assert!(encode_ticks(&frames, limits).is_err());
    let limits = WireLimits {
        max_tick_bytes: 1,
        ..WireLimits::default()
    };
    assert!(decode_packet(&bytes, limits).is_err());
    assert!(encode_ticks(&frames, limits).is_err());
    let mut huge_count = encode_ticks(&[], WireLimits::default()).unwrap();
    huge_count[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode_packet(&huge_count, WireLimits::default()).is_err());
}

#[test]
fn accepted_u64_values_are_not_rounded_through_javascript_numbers() {
    let (mut server, _, _) = pair();
    let mut frame = tick(&mut server, vec![]);
    frame.accepted.lot_id = u64::MAX;
    frame.accepted.epoch = u64::MAX - 1;
    frame.accepted.tick = 9_007_199_254_740_993;
    frame.accepted.rng_before = u64::MAX - 2;
    let bytes = encode_ticks(std::slice::from_ref(&frame), WireLimits::default()).unwrap();
    let Packet::Ticks(decoded) = decode_packet(&bytes, WireLimits::default()).unwrap() else {
        panic!()
    };
    assert_eq!(decoded, [frame]);
}

#[test]
fn arbitrary_truncation_never_partially_advances_a_live_replica() {
    let (mut server, _, _) = pair();
    let bytes = encode_ticks(&[tick(&mut server, vec![])], WireLimits::default()).unwrap();
    for end in 0..bytes.len() {
        assert!(decode_packet(&bytes[..end], WireLimits::default()).is_err());
    }
}

#[test]
fn history_older_than_the_retained_runtime_tick_requires_recovery() {
    let (mut server, mut wire, _) = pair();
    install(&mut wire, &server);
    let frames = [tick(&mut server, vec![]), tick(&mut server, vec![])];
    let packet = encode_ticks(&frames, WireLimits::default()).unwrap();
    wire.receive(wire.connection(), &packet).unwrap();
    let before = wire.replica().runtime().unwrap().snapshot().unwrap();
    assert!(wire.receive(wire.connection(), &packet).is_err());
    assert_eq!(
        before,
        wire.replica().runtime().unwrap().snapshot().unwrap()
    );
    assert_eq!(wire.replica().status(), SessionStatus::AwaitingCheckpoint);
}

#[test]
fn tuple_keyed_maps_and_all_command_bits_survive_binary_roundtrip() {
    use std::collections::{BTreeMap, BTreeSet};
    let (mut server, _, actor) = pair();
    let mut frame = tick(&mut server, vec![]);
    frame.accepted.commands = vec![AcceptedCommand::SetInteractionProjection {
        entity: actor,
        queued_users: BTreeSet::from([actor]),
        active_advertisements: Some(BTreeMap::from([((4, 501), -32768), ((7, 9), 32767)])),
    }];
    let packet = encode_ticks(std::slice::from_ref(&frame), WireLimits::default()).unwrap();
    let Packet::Ticks(decoded) = decode_packet(&packet, WireLimits::default()).unwrap() else {
        panic!()
    };
    assert_eq!(decoded, [frame]);
}

#[test]
fn semantic_aliasing_and_duplicate_set_entries_are_not_canonical_wire_data() {
    use bincode::Options;
    use std::collections::{BTreeMap, BTreeSet};
    let (mut server, _, actor) = pair();
    let mut frame = tick(&mut server, vec![]);
    frame.accepted.commands = vec![AcceptedCommand::SetInteractionProjection {
        entity: actor,
        queued_users: BTreeSet::new(),
        active_advertisements: Some(BTreeMap::from([((4, 501), 9), ((7, 9), 2)])),
    }];
    let original = encode_ticks(&[frame], WireLimits::default()).unwrap();
    // Last two map entries are fixed u8/u16/i16 values. Duplicate the first key.
    let mut corrupt = original.clone();
    let length = corrupt.len();
    let first_key = corrupt[length - 10..length - 7].to_vec();
    corrupt[length - 5..length - 2].copy_from_slice(&first_key);
    let payload = &corrupt[32 + 4 + 36..];
    let options = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .reject_trailing_bytes();
    let permissive: wonderland_game_runtime::AcceptedTick = options.deserialize(payload).unwrap();
    if let AcceptedCommand::SetInteractionProjection {
        active_advertisements: Some(ref values),
        ..
    } = permissive.commands[0]
    {
        assert_eq!(
            values.len(),
            1,
            "negative control must actually collapse a duplicate source key"
        );
    } else {
        panic!()
    }
    assert!(decode_packet(&corrupt, WireLimits::default()).is_err());
    assert!(decode_packet(&original, WireLimits::default()).is_ok());
}

#[test]
fn decode_work_depth_and_allocation_credit_are_independent_limits() {
    let (mut server, _, _) = pair();
    let frames = [tick(&mut server, vec![])];
    let packet = encode_ticks(&frames, WireLimits::default()).unwrap();
    for limits in [
        WireLimits {
            max_decode_items: 1,
            ..WireLimits::default()
        },
        WireLimits {
            max_decode_depth: 1,
            ..WireLimits::default()
        },
        WireLimits {
            max_decode_credit: 1,
            ..WireLimits::default()
        },
    ] {
        assert!(decode_packet(&packet, limits).is_err());
    }
}

#[test]
fn huge_nested_collection_hint_rejects_before_vector_preallocation() {
    // An empty AcceptedTick ends in the commands vector's fixed u64 count.
    let (mut server, _, _) = pair();
    let mut packet = encode_ticks(&[tick(&mut server, vec![])], WireLimits::default()).unwrap();
    let len = packet.len();
    packet[len - 8..].copy_from_slice(&u64::MAX.to_le_bytes());
    let error = decode_packet(&packet, WireLimits::default())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("collection hint exceeds remaining work"),
        "{error}"
    );
}

#[test]
fn recovery_request_has_direction_and_exact_full_width_cursor() {
    use wonderland_game_runtime::live_session::ReplicaCursor;
    use wonderland_game_runtime::live_wire::{
        CheckpointRequest, decode_checkpoint_request, encode_checkpoint_request,
    };
    let request = CheckpointRequest {
        id: u64::MAX,
        cursor: Some(ReplicaCursor {
            lot_id: u64::MAX,
            authority_epoch: u64::MAX - 1,
            completed_tick: u64::MAX - 2,
            state_hash: [17; 32],
        }),
    };
    let packet = encode_checkpoint_request(request).unwrap();
    assert_eq!(packet.len(), 89);
    let decoded = decode_checkpoint_request(&packet).unwrap();
    assert_eq!(decoded.id, request.id);
    assert_eq!(decoded.cursor, request.cursor);
    assert!(
        decode_packet(&packet, WireLimits::default()).is_err(),
        "client request is never an authoritative server delivery"
    );
    for length in 0..packet.len() {
        assert!(decode_checkpoint_request(&packet[..length]).is_err());
    }
    let mut extra = packet.clone();
    extra.push(0);
    assert!(decode_checkpoint_request(&extra).is_err());
    let mut noncanonical = packet;
    noncanonical[32] = 2;
    assert!(decode_checkpoint_request(&noncanonical).is_err());
    assert!(decode_checkpoint_request(&encode_ticks(&[], WireLimits::default()).unwrap()).is_err());
    assert!(
        encode_checkpoint_request(CheckpointRequest {
            id: 0,
            cursor: None
        })
        .is_err()
    );
    let absent = encode_checkpoint_request(CheckpointRequest {
        id: 1,
        cursor: None,
    })
    .unwrap();
    assert_eq!(absent.len(), 33);
    assert!(decode_checkpoint_request(&absent).unwrap().cursor.is_none());
}

#[test]
fn closed_or_disconnected_wire_never_recovers_without_explicit_reconnect() {
    let (server, mut wire, _) = pair();
    install(&mut wire, &server);
    let token = wire.connection();
    wire.disconnect(token).unwrap();
    assert!(wire.checkpoint_request().is_none());
    assert!(
        wire.receive(token, &encode_ticks(&[], WireLimits::default()).unwrap())
            .is_err()
    );
    assert_eq!(wire.replica().status(), SessionStatus::Suspended);
    wire.close();
    assert!(wire.replica().runtime().is_none());
    assert!(wire.checkpoint_request().is_none());
    assert!(wire.reconnect().is_err());
    assert!(wire.receive(token, b"").is_err());
}

#[test]
fn wire_limits_must_admit_native_checkpoint_and_empty_batch_envelopes() {
    use wonderland_game_runtime::live_session::ReplayLimits;
    use wonderland_game_runtime::sim_core::snapshot::{SNAPSHOT_CHECKSUM_LEN, SNAPSHOT_HEADER_LEN};
    for replay in [
        ReplayLimits {
            max_batch_bytes: 4,
            ..ReplayLimits::default()
        },
        ReplayLimits {
            max_checkpoint_bytes: SNAPSHOT_HEADER_LEN + SNAPSHOT_CHECKSUM_LEN,
            ..ReplayLimits::default()
        },
    ] {
        let limits = WireLimits {
            replay,
            ..WireLimits::default()
        };
        assert!(limits.max_packet_bytes().is_err());
        assert!(encode_ticks(&[], limits).is_err());
    }
    let minimum_batch = WireLimits {
        replay: ReplayLimits {
            max_batch_bytes: 8,
            ..ReplayLimits::default()
        },
        ..WireLimits::default()
    };
    assert!(encode_ticks(&[], minimum_batch).is_ok());
}
