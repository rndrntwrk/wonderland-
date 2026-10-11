//! Transport buffering is not a substitute VM: no queued message may disappear
//! merely because two socket events arrive before the presentation effect runs.
use wonderland_web_shell::vm_inbox::{InboxError, InboxLimits, VmDelivery, VmInbox, VmStream};

fn stream() -> VmStream {
    VmStream {
        browser_epoch: 1,
        source_epoch: 2,
        lot_incarnation: 3,
    }
}
fn frame(byte: u8, size: usize) -> VmDelivery {
    VmDelivery {
        browser_epoch: 1,
        source_epoch: 2,
        lot_incarnation: Some(3),
        direct: byte.is_multiple_of(2),
        data: vec![byte; size],
    }
}
fn inbox() -> VmInbox {
    let mut inbox = VmInbox::new(InboxLimits {
        max_frames: 3,
        max_retained_bytes: 12,
    })
    .unwrap();
    inbox.bind(Some(stream()));
    inbox
}

#[test]
fn burst_drains_every_frame_in_socket_order_exactly_once() {
    let mut inbox = inbox();
    inbox.push(frame(1, 4)).unwrap();
    inbox.push(frame(2, 4)).unwrap();
    inbox.push(frame(3, 4)).unwrap();
    assert_eq!(inbox.retained_bytes(), 12);
    let frames = inbox.drain(stream()).unwrap();
    assert_eq!(
        frames
            .iter()
            .map(|f| (f.data[0], f.direct))
            .collect::<Vec<_>>(),
        [(1, false), (2, true), (3, false)]
    );
    assert_eq!(inbox.retained_bytes(), 0);
    assert!(inbox.drain(stream()).unwrap().is_empty());
}
#[test]
fn identical_direct_messages_are_not_deduplicated() {
    let mut inbox = inbox();
    inbox.push(frame(2, 1)).unwrap();
    inbox.push(frame(2, 1)).unwrap();
    assert_eq!(inbox.drain(stream()).unwrap().len(), 2);
}
#[test]
fn overflow_discards_partial_backlog_and_latches_recovery() {
    let mut inbox = inbox();
    for n in 0..3 {
        inbox.push(frame(n, 1)).unwrap();
    }
    assert!(matches!(
        inbox.push(frame(4, 1)),
        Err(InboxError::FrameLimit)
    ));
    assert_eq!(inbox.retained_bytes(), 0);
    assert!(matches!(
        inbox.drain(stream()),
        Err(InboxError::RecoveryRequired)
    ));
    assert!(matches!(
        inbox.push(frame(5, 1)),
        Err(InboxError::RecoveryRequired)
    ));
    // A repeated status update must not clear a poisoned stream.
    inbox.bind(Some(stream()));
    assert!(matches!(
        inbox.push(frame(6, 1)),
        Err(InboxError::RecoveryRequired)
    ));
    inbox.reset();
    inbox.bind(Some(stream()));
    inbox.push(frame(7, 1)).unwrap();
    assert_eq!(inbox.drain(stream()).unwrap()[0].data, [7]);
}
#[test]
fn retained_capacity_not_only_payload_length_counts_towards_budget() {
    let mut inbox = inbox();
    let mut f = frame(1, 1);
    f.data = Vec::with_capacity(13);
    f.data.push(1);
    assert!(matches!(inbox.push(f), Err(InboxError::ByteLimit)));
    assert!(matches!(
        inbox.drain(stream()),
        Err(InboxError::RecoveryRequired)
    ));
}
#[test]
fn aggregate_byte_overflow_does_not_evict_an_earlier_update() {
    let mut inbox = inbox();
    inbox.push(frame(1, 7)).unwrap();
    assert!(matches!(
        inbox.push(frame(2, 6)),
        Err(InboxError::ByteLimit)
    ));
    assert_eq!(inbox.retained_bytes(), 0);
    assert!(matches!(
        inbox.drain(stream()),
        Err(InboxError::RecoveryRequired)
    ));
}
#[test]
fn old_scope_push_or_drain_cannot_consume_the_current_lot() {
    let mut inbox = inbox();
    let old = stream();
    let next = VmStream {
        lot_incarnation: 4,
        ..old
    };
    inbox.push(frame(1, 1)).unwrap();
    inbox.bind(Some(next));
    assert_eq!(inbox.retained_bytes(), 0);
    let mut current = frame(2, 2);
    current.lot_incarnation = Some(4);
    inbox.push(current).unwrap();
    assert!(matches!(
        inbox.push(frame(3, 1)),
        Err(InboxError::WrongStream)
    ));
    assert!(matches!(inbox.drain(old), Err(InboxError::WrongStream)));
    assert_eq!(inbox.retained_bytes(), 2);
    assert_eq!(inbox.drain(next).unwrap()[0].data, [2, 2]);
}
#[test]
fn ordinary_same_scope_bind_keeps_pending_frames() {
    let mut inbox = inbox();
    inbox.push(frame(1, 1)).unwrap();
    inbox.bind(Some(stream()));
    assert_eq!(inbox.drain(stream()).unwrap().len(), 1);
}
#[test]
fn logout_and_unbound_stream_reject_updates() {
    let mut inbox = inbox();
    inbox.push(frame(1, 1)).unwrap();
    inbox.bind(None);
    assert_eq!(inbox.retained_bytes(), 0);
    assert!(matches!(
        inbox.push(frame(2, 1)),
        Err(InboxError::WrongStream)
    ));
    assert!(matches!(
        inbox.drain(stream()),
        Err(InboxError::WrongStream)
    ));
}
#[test]
fn invalid_or_missing_identity_is_not_a_stream() {
    for invalid in [
        VmStream {
            browser_epoch: 0,
            ..stream()
        },
        VmStream {
            source_epoch: 0,
            ..stream()
        },
        VmStream {
            lot_incarnation: 0,
            ..stream()
        },
    ] {
        let mut inbox = inbox();
        inbox.bind(Some(invalid));
        let mut f = frame(1, 1);
        f.browser_epoch = invalid.browser_epoch;
        f.source_epoch = invalid.source_epoch;
        f.lot_incarnation = Some(invalid.lot_incarnation);
        assert!(matches!(inbox.push(f), Err(InboxError::WrongStream)));
    }
    let mut inbox = inbox();
    let mut f = frame(1, 1);
    f.lot_incarnation = None;
    assert!(matches!(inbox.push(f), Err(InboxError::WrongStream)));
}
#[test]
fn invalid_limits_are_rejected_not_silently_clamped() {
    for limits in [
        InboxLimits {
            max_frames: 0,
            max_retained_bytes: 1,
        },
        InboxLimits {
            max_frames: 1,
            max_retained_bytes: 0,
        },
        InboxLimits {
            max_frames: usize::MAX,
            max_retained_bytes: 1,
        },
        InboxLimits {
            max_frames: 1,
            max_retained_bytes: usize::MAX,
        },
    ] {
        assert!(VmInbox::new(limits).is_err());
    }
}
