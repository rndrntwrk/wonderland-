//! Queue admission with the real legacy decoder and original source golden bytes.
//! These checks preserve source traffic; they do not turn it into an A checkpoint.
use wonderland_web_shell::{
    live_world_adapter::{FrameError, LiveWorldIdentity, SourceFrameGate},
    vm_inbox::{InboxLimits, VmDelivery, VmInbox, VmStream},
};

const SNAPSHOT: &[u8] =
    include_bytes!("../../../crates/vm-protocol/tests/fixtures/source-v38-state-sync.bin");
fn identity() -> LiveWorldIdentity {
    LiveWorldIdentity {
        browser_epoch: 2,
        source_epoch: 3,
        lot_incarnation: 4,
        lot_location: 55,
        avatar_id: 7,
    }
}
fn stream() -> VmStream {
    let id = identity();
    VmStream {
        browser_epoch: id.browser_epoch,
        source_epoch: id.source_epoch,
        lot_incarnation: id.lot_incarnation,
    }
}
fn packet(id: u32, commands: &[&[u8]]) -> Vec<u8> {
    // Original VMNetTickList: immediate flag, count, TickID, RNG, command count.
    let mut bytes = vec![0];
    bytes.extend(1_i32.to_le_bytes());
    bytes.extend(id.to_le_bytes());
    bytes.extend(u64::MAX.to_le_bytes());
    bytes.extend((commands.len() as i32).to_le_bytes());
    for command in commands {
        bytes.extend(*command);
    }
    bytes
}
fn delivery(data: Vec<u8>, direct: bool) -> VmDelivery {
    VmDelivery {
        browser_epoch: stream().browser_epoch,
        source_epoch: stream().source_epoch,
        lot_incarnation: Some(stream().lot_incarnation),
        direct,
        data,
    }
}
fn inbox() -> VmInbox {
    let mut inbox = VmInbox::new(InboxLimits {
        max_frames: 8,
        max_retained_bytes: 1024 * 1024,
    })
    .unwrap();
    inbox.bind(Some(stream()));
    inbox
}
fn gate() -> SourceFrameGate {
    let mut gate = SourceFrameGate::default();
    gate.reset(Some(identity()));
    gate
}

#[test]
fn snapshot_and_ordinary_ticks_survive_one_deferred_browser_drain() {
    let mut inbox = inbox();
    // Source StateSync at 42 names the NEXT tick; the ordinary 42 must follow it.
    inbox
        .push(delivery(packet(42, &[SNAPSHOT]), false))
        .unwrap();
    inbox.push(delivery(packet(42, &[]), false)).unwrap();
    inbox.push(delivery(packet(43, &[]), false)).unwrap();
    let mut gate = gate();
    let updates: Vec<_> = inbox
        .drain(stream())
        .unwrap()
        .into_iter()
        .map(|frame| gate.admit(identity(), frame.direct, &frame.data).unwrap())
        .collect();
    assert_eq!(updates.len(), 3);
    assert_eq!(updates[0].snapshots.len(), 1);
    assert_eq!(updates[0].snapshots[0].platform.lot_id, 55);
    assert_eq!(updates[0].last_tick, Some(41));
    assert!(!updates[0].needs_refresh);
    assert_eq!(updates[1].last_tick, Some(42));
    assert_eq!(updates[2].last_tick, Some(43));
    assert!(updates[1].needs_refresh && updates[2].needs_refresh);
    assert_eq!(gate.last_tick(), Some(43));
    assert!(inbox.drain(stream()).unwrap().is_empty());
}

#[test]
fn equal_direct_source_events_are_distinct_but_tick_duplicates_remain_source_owned() {
    // Original EOD enter message for this actor, with a two-byte .NET string.
    let mut event = vec![18];
    event.extend(7_u32.to_le_bytes());
    event.extend(0x8b300068_u32.to_le_bytes());
    event.extend([
        9, b'e', b'o', b'd', b'_', b'e', b'n', b't', b'e', b'r', 0, 2, b'4', b'2',
    ]);
    let mut inbox = inbox();
    inbox.push(delivery(event.clone(), true)).unwrap();
    inbox.push(delivery(event, true)).unwrap();
    inbox.push(delivery(packet(42, &[]), false)).unwrap();
    inbox.push(delivery(packet(42, &[]), false)).unwrap();
    let mut gate = gate();
    let mut frames = inbox.drain(stream()).unwrap().into_iter();
    for _ in 0..2 {
        let frame = frames.next().unwrap();
        let result = gate.admit(identity(), frame.direct, &frame.data).unwrap();
        assert_eq!(result.eods.len(), 1);
        assert_eq!(result.eods[0].actor_uid, 7);
    }
    let frame = frames.next().unwrap();
    gate.admit(identity(), frame.direct, &frame.data).unwrap();
    let before = gate.clone();
    let frame = frames.next().unwrap();
    assert_eq!(
        gate.admit(identity(), frame.direct, &frame.data)
            .unwrap_err(),
        FrameError::StaleTick
    );
    assert_eq!(gate, before);
}

#[test]
fn latest_value_negative_control_loses_the_required_source_snapshot() {
    // Demonstrates the former single-slot contract independently of the new queue.
    let deliveries = [
        delivery(packet(42, &[SNAPSHOT]), false),
        delivery(packet(42, &[]), false),
        delivery(packet(43, &[]), false),
    ];
    let latest = deliveries.into_iter().last().unwrap();
    let mut gate = gate();
    let update = gate.admit(identity(), latest.direct, &latest.data).unwrap();
    assert!(update.snapshots.is_empty());
    assert!(update.needs_refresh);
}
