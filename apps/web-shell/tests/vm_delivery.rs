use wonderland_web_shell::vm_delivery::{
    DeliveryError, DeliveryIdentity, VmDelivery, VmDeliveryQueue,
};

fn identity() -> DeliveryIdentity {
    DeliveryIdentity {
        browser_epoch: 2,
        source_epoch: 3,
        lot_incarnation: 4,
    }
}

fn frame(value: u8) -> VmDelivery {
    VmDelivery {
        browser_epoch: 2,
        source_epoch: 3,
        lot_incarnation: Some(4),
        direct: value.is_multiple_of(2),
        data: vec![value].into_boxed_slice(),
    }
}

fn queue(frames: usize, bytes: usize) -> VmDeliveryQueue {
    let mut queue = VmDeliveryQueue::with_limits(frames, bytes);
    queue.bind(Some(identity()));
    queue
}

#[test]
fn burst_delivers_every_frame_once_in_source_order() {
    let mut queue = queue(64, 64);
    for value in 0..64 {
        queue.push(frame(value)).unwrap();
    }
    for value in 0..64 {
        assert_eq!(queue.pop(), Some(frame(value)));
    }
    assert_eq!(queue.pop(), None);
    assert_eq!(queue.pending_bytes(), 0);
    assert_eq!(queue.pending_frames(), 0);
}

#[test]
fn byte_identical_native_deliveries_are_not_deduplicated() {
    let mut queue = queue(4, 4);
    queue.push(frame(18)).unwrap();
    queue.push(frame(18)).unwrap();
    assert_eq!(queue.pop(), Some(frame(18)));
    assert_eq!(queue.pop(), Some(frame(18)));
    assert_eq!(queue.pop(), None);
}

#[test]
fn frame_limit_never_evicts_an_unconsumed_prefix() {
    let mut queue = queue(2, 100);
    queue.push(frame(1)).unwrap();
    queue.push(frame(2)).unwrap();
    assert_eq!(queue.push(frame(3)), Err(DeliveryError::Capacity));
    assert!(queue.requires_reconnect());
    assert_eq!(queue.pop(), None);
    assert_eq!(queue.pending_bytes(), 0);
    assert_eq!(queue.push(frame(4)), Err(DeliveryError::NeedsReconnect));
}

#[test]
fn byte_limit_is_independent_from_frame_count() {
    let mut queue = queue(10, 3);
    let mut item = frame(7);
    item.data = vec![1, 2, 3].into_boxed_slice();
    queue.push(item).unwrap();
    assert_eq!(queue.pending_bytes(), 3);
    assert_eq!(queue.push(frame(8)), Err(DeliveryError::Capacity));
    assert!(queue.requires_reconnect());
    assert_eq!(queue.pending_frames(), 0);
}

#[test]
fn one_oversized_frame_invalidates_the_stream() {
    let mut queue = queue(10, 2);
    let mut item = frame(7);
    item.data = vec![1, 2, 3].into_boxed_slice();
    assert_eq!(queue.push(item), Err(DeliveryError::Capacity));
    assert!(queue.requires_reconnect());
}

#[test]
fn empty_payload_invalidates_the_stream() {
    let mut queue = queue(10, 20);
    queue.push(frame(1)).unwrap();
    let mut item = frame(2);
    item.data = Box::default();
    assert_eq!(queue.push(item), Err(DeliveryError::EmptyPayload));
    assert!(queue.requires_reconnect());
    assert_eq!(queue.pop(), None);
}

#[test]
fn wrong_epoch_or_lot_cannot_poison_or_replace_the_current_stream() {
    let mut queue = queue(10, 20);
    queue.push(frame(1)).unwrap();
    for field in 0..4 {
        let mut item = frame(2);
        match field {
            0 => item.browser_epoch += 1,
            1 => item.source_epoch += 1,
            2 => item.lot_incarnation = Some(5),
            _ => item.lot_incarnation = None,
        }
        assert_eq!(queue.push(item), Err(DeliveryError::WrongIdentity));
    }
    assert!(!queue.requires_reconnect());
    assert_eq!(queue.pop(), Some(frame(1)));
    assert_eq!(queue.pop(), None);
}

#[test]
fn identical_binding_preserves_pending_frames() {
    let mut queue = queue(10, 20);
    queue.push(frame(1)).unwrap();
    queue.bind(Some(identity()));
    assert_eq!(queue.pop(), Some(frame(1)));
}

#[test]
fn identical_binding_cannot_clear_an_overload() {
    let mut queue = queue(1, 10);
    queue.push(frame(1)).unwrap();
    assert_eq!(queue.push(frame(2)), Err(DeliveryError::Capacity));
    queue.bind(Some(identity()));
    assert!(queue.requires_reconnect());
    assert_eq!(queue.push(frame(3)), Err(DeliveryError::NeedsReconnect));
}

#[test]
fn new_lot_drops_the_old_pending_stream() {
    let mut queue = queue(10, 20);
    queue.push(frame(1)).unwrap();
    let mut next = identity();
    next.lot_incarnation += 1;
    queue.bind(Some(next));
    assert_eq!(queue.pop(), None);
    assert_eq!(queue.push(frame(2)), Err(DeliveryError::WrongIdentity));
    let mut item = frame(3);
    item.lot_incarnation = Some(next.lot_incarnation);
    queue.push(item.clone()).unwrap();
    assert_eq!(queue.pop(), Some(item));
}

#[test]
fn clear_releases_pending_data_and_requires_explicit_binding() {
    let mut queue = queue(10, 20);
    queue.push(frame(1)).unwrap();
    queue.clear();
    assert_eq!(queue.pending_bytes(), 0);
    assert_eq!(queue.pending_frames(), 0);
    assert_eq!(queue.push(frame(2)), Err(DeliveryError::Unbound));
    queue.bind(Some(identity()));
    queue.push(frame(3)).unwrap();
    assert_eq!(queue.pop(), Some(frame(3)));
}

#[test]
fn reconnect_can_rebind_the_same_source_after_clearing() {
    let mut queue = queue(1, 10);
    queue.push(frame(1)).unwrap();
    assert_eq!(queue.push(frame(2)), Err(DeliveryError::Capacity));
    queue.clear();
    queue.bind(Some(identity()));
    assert!(!queue.requires_reconnect());
    queue.push(frame(3)).unwrap();
    assert_eq!(queue.pop(), Some(frame(3)));
}

#[test]
fn zero_lot_is_not_a_valid_binding() {
    let mut queue = queue(10, 20);
    let mut invalid = identity();
    invalid.lot_incarnation = 0;
    queue.bind(Some(invalid));
    let mut item = frame(1);
    item.lot_incarnation = Some(0);
    assert_eq!(queue.push(item), Err(DeliveryError::Unbound));
}

#[test]
fn consumed_bytes_release_capacity_for_subsequent_updates() {
    let mut queue = queue(2, 2);
    for value in 0..=255 {
        queue.push(frame(value)).unwrap();
        assert_eq!(queue.pending_bytes(), 1);
        assert_eq!(queue.pop(), Some(frame(value)));
        assert_eq!(queue.pending_bytes(), 0);
    }
}

#[test]
fn burst_preserves_empty_autonomy_ticks_at_the_actual_source_gate() {
    use wonderland_web_shell::live_world_adapter::{LiveWorldIdentity, SourceFrameGate};
    let live = LiveWorldIdentity {
        browser_epoch: 2,
        source_epoch: 3,
        lot_incarnation: 4,
        lot_location: 0x00f00101,
        avatar_id: 7,
    };
    let mut gate = SourceFrameGate::default();
    gate.reset(Some(live));
    let mut queue = queue(16, 1024);
    for tick in 41_u32..=48 {
        // Original VMNetTickList: non-immediate, one tick, source RNG, no commands.
        let mut bytes = vec![0, 1, 0, 0, 0];
        bytes.extend(tick.to_le_bytes());
        bytes.extend(u64::MAX.to_le_bytes());
        bytes.extend(0_i32.to_le_bytes());
        let mut delivery = frame(1);
        delivery.direct = false;
        delivery.data = bytes.into_boxed_slice();
        queue.push(delivery).unwrap();
    }
    let mut generations = Vec::new();
    while let Some(delivery) = queue.pop() {
        let update = gate.admit(live, delivery.direct, &delivery.data).unwrap();
        generations.push((update.generation, update.last_tick));
        assert!(update.needs_refresh);
    }
    assert_eq!(
        generations,
        (1_u64..=8).zip((41_u32..=48).map(Some)).collect::<Vec<_>>()
    );
}

#[test]
fn epoch_change_clears_queued_bytes_even_when_lot_number_is_reused() {
    for browser_changed in [false, true] {
        let mut queue = queue(4, 4);
        queue.push(frame(1)).unwrap();
        let mut next = identity();
        if browser_changed {
            next.browser_epoch += 1;
        } else {
            next.source_epoch += 1;
        }
        queue.bind(Some(next));
        assert_eq!(queue.pending_bytes(), 0);
        assert_eq!(queue.pop(), None);
        assert_eq!(queue.push(frame(2)), Err(DeliveryError::WrongIdentity));
    }
}

#[test]
fn explicit_unbind_discards_pending_and_overload_state() {
    let mut queue = queue(1, 1);
    queue.push(frame(1)).unwrap();
    let _ = queue.push(frame(2));
    queue.bind(None);
    assert!(!queue.requires_reconnect());
    assert_eq!(queue.pending_frames(), 0);
    assert_eq!(queue.push(frame(3)), Err(DeliveryError::Unbound));
}
