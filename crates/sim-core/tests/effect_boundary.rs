use sim_core::effects::{
    CancelResult, EffectBook, EffectError, EffectKind, EffectLimits, EffectPayload, EffectRequest,
    EffectResolution, EffectResolved, EffectValue, OperationId, MAX_EFFECT_BYTES,
    MAX_PENDING_OPERATIONS, MAX_TERMINAL_OPERATIONS,
};
use sim_core::ids::{EntityRef, ObjectId};

fn target(generation: u32) -> EntityRef {
    EntityRef {
        object_id: ObjectId(7),
        generation,
    }
}

fn limits() -> EffectLimits {
    EffectLimits {
        max_pending: 4,
        max_terminal: 3,
        max_request_bytes: 16,
        max_response_bytes: 16,
    }
}

fn book() -> EffectBook {
    EffectBook::new(0x0102_0304_0506_0708, limits()).unwrap()
}

fn issue(book: &mut EffectBook, target: EntityRef) -> EffectRequest {
    book.issue(
        target,
        10,
        2,
        EffectKind::Int32,
        EffectPayload::Bytes(vec![1, 2, 3]),
    )
    .unwrap()
}

fn response(request: &EffectRequest) -> EffectResolved {
    EffectResolved {
        operation_id: request.operation_id,
        target: request.target,
        apply_tick: 12,
        delivery_epoch: 2,
        committed_epoch: 2,
        value: EffectValue::Int32(42),
    }
}

fn assert_atomic<T: std::fmt::Debug>(
    book: &mut EffectBook,
    call: impl FnOnce(&mut EffectBook) -> Result<T, EffectError>,
) -> EffectError {
    let before = bincode::serialize(book).unwrap();
    let result = call(book).unwrap_err();
    assert_eq!(bincode::serialize(book).unwrap(), before);
    result
}

// The effect book's fixed-width header is namespace followed by the nonce
// high-water mark. Malformed snapshots exercise validation after decoding, not
// a privileged mutation API that production callers could use to reuse IDs.
fn with_snapshot_nonce(book: &EffectBook, nonce: u64) -> EffectBook {
    let mut snapshot = bincode::serialize(book).unwrap();
    snapshot[8..16].copy_from_slice(&nonce.to_le_bytes());
    bincode::deserialize(&snapshot).unwrap()
}

#[test]
fn stable_ids_do_not_encode_epochs_and_survive_snapshot_replay() {
    let mut continuous = book();
    let first = issue(&mut continuous, target(1));
    assert_eq!(
        first.operation_id.0,
        [1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 0, 0, 0, 1]
    );
    assert_eq!(first.operation_id.namespace(), continuous.namespace());
    assert_eq!(first.operation_id.nonce(), 1);

    let mut restored: EffectBook =
        bincode::deserialize(&bincode::serialize(&continuous).unwrap()).unwrap();
    restored.validate().unwrap();
    let after_takeover = restored.redispatch(first.operation_id, 9).unwrap();
    assert_eq!(after_takeover.request, first);
    assert_eq!(after_takeover.dispatch_epoch, 9);
    assert_eq!(after_takeover.request.issued_epoch, 2);

    let next_continuous = continuous
        .issue(target(1), 11, 2, EffectKind::Bool, EffectPayload::Int32(4))
        .unwrap();
    let next_restored = restored
        .issue(target(1), 11, 9, EffectKind::Bool, EffectPayload::Int32(4))
        .unwrap();
    assert_eq!(next_continuous.operation_id, next_restored.operation_id);
    assert_eq!(next_restored.operation_id.nonce(), 2);
}

#[test]
fn historical_commit_is_accepted_once_through_current_epoch_delivery() {
    let mut book = book();
    let request = issue(&mut book, target(1));
    let mut result = response(&request);
    result.delivery_epoch = 5;
    assert_eq!(
        book.resolve(result.clone(), 12, 5, Some(target(1))),
        Ok(EffectResolution::Applied {
            operation_id: request.operation_id,
            target: target(1),
            value: EffectValue::Int32(42),
        })
    );
    assert_eq!(book.pending_len(), 0);
    let applied = bincode::serialize(&book).unwrap();

    // A later accepted delivery can carry the same durable outcome even after
    // the original entity was deleted. Duplicate never exposes a resume value.
    result.delivery_epoch = 6;
    result.apply_tick = 15;
    assert_eq!(
        book.resolve(result, 15, 6, None),
        Ok(EffectResolution::Duplicate)
    );
    assert_eq!(bincode::serialize(&book).unwrap(), applied);
    book.validate_context(15, 6).unwrap();
}

#[test]
fn stale_deliveries_and_impossible_commit_epochs_do_not_consume_request() {
    let mut book = book();
    let request = issue(&mut book, target(1));
    for (delivery, committed, current) in [(2, 2, 3), (4, 2, 3), (3, 4, 3), (3, 1, 3)] {
        let mut result = response(&request);
        result.delivery_epoch = delivery;
        result.committed_epoch = committed;
        assert_atomic(&mut book, |book| {
            book.resolve(result, 12, current, Some(target(1)))
        });
        assert_eq!(book.request(request.operation_id), Some(&request));
    }
    assert!(book.redispatch(request.operation_id, 1).is_err());
}

#[test]
fn responses_cannot_resume_deleted_recycled_or_different_targets() {
    let mut book = book();
    let request = issue(&mut book, target(1));
    for live in [None, Some(target(2))] {
        assert_eq!(
            assert_atomic(&mut book, |book| {
                book.resolve(response(&request), 12, 2, live)
            }),
            EffectError::StaleTarget {
                expected: target(1),
                live,
            }
        );
    }
    let mut wrong_generation = response(&request);
    wrong_generation.target = target(2);
    assert_atomic(&mut book, |book| {
        book.resolve(wrong_generation, 12, 2, Some(target(2)))
    });
    let mut wrong_object = response(&request);
    wrong_object.target.object_id = ObjectId(8);
    assert_atomic(&mut book, |book| {
        book.resolve(wrong_object, 12, 2, Some(target(1)))
    });
    assert_eq!(book.pending_len(), 1);
}

#[test]
fn response_kind_and_tick_are_checked_before_resume() {
    let mut book = book();
    let request = issue(&mut book, target(1));
    let mut wrong_kind = response(&request);
    wrong_kind.value = EffectValue::Bool(true);
    assert_eq!(
        assert_atomic(&mut book, |book| {
            book.resolve(wrong_kind, 12, 2, Some(target(1)))
        }),
        EffectError::ResponseTypeMismatch {
            expected: EffectKind::Int32,
            actual: EffectKind::Bool,
        }
    );
    for (scheduled, current) in [(12, 11), (12, 13), (9, 9)] {
        let mut wrong_tick = response(&request);
        wrong_tick.apply_tick = scheduled;
        assert_atomic(&mut book, |book| {
            book.resolve(wrong_tick, current, 2, Some(target(1)))
        });
    }
    assert!(matches!(
        book.resolve(response(&request), 12, 2, Some(target(1))),
        Ok(EffectResolution::Applied { .. })
    ));
}

#[test]
fn conflicting_duplicate_values_or_committed_epochs_fail_atomically() {
    let mut book = book();
    let request = issue(&mut book, target(1));
    let result = response(&request);
    book.resolve(result.clone(), 12, 2, Some(target(1)))
        .unwrap();
    let mut changed_value = result.clone();
    changed_value.value = EffectValue::Int32(43);
    assert_eq!(
        assert_atomic(&mut book, |book| {
            book.resolve(changed_value, 12, 2, Some(target(1)))
        }),
        EffectError::ConflictingDuplicate(request.operation_id)
    );
    let mut changed_commit = result;
    changed_commit.delivery_epoch = 3;
    changed_commit.committed_epoch = 3;
    assert_eq!(
        assert_atomic(&mut book, |book| {
            book.resolve(changed_commit, 12, 3, Some(target(1)))
        }),
        EffectError::ConflictingDuplicate(request.operation_id)
    );
}

#[test]
fn opaque_responses_are_bounded_and_typed_without_register_mutations() {
    let mut book = book();
    let request = book
        .issue(
            target(1),
            10,
            2,
            EffectKind::Bytes,
            EffectPayload::Bool(true),
        )
        .unwrap();
    let mut result = response(&request);
    result.value = EffectValue::Bytes(vec![0; 17]);
    assert_atomic(&mut book, |book| {
        book.resolve(result.clone(), 12, 2, Some(target(1)))
    });
    result.value = EffectValue::Bytes(vec![0; 16]);
    assert_eq!(
        book.resolve(result, 12, 2, Some(target(1))),
        Ok(EffectResolution::Applied {
            operation_id: request.operation_id,
            target: target(1),
            value: EffectValue::Bytes(vec![0; 16]),
        })
    );
}

#[test]
fn request_bounds_and_pending_capacity_fail_without_burning_nonces() {
    let mut book = book();
    assert_atomic(&mut book, |book| {
        book.issue(
            target(1),
            10,
            2,
            EffectKind::Bool,
            EffectPayload::Bytes(vec![0; 17]),
        )
    });
    assert_eq!(book.last_nonce(), 0);
    for generation in 1..=4 {
        issue(&mut book, target(generation));
    }
    assert_atomic(&mut book, |book| {
        book.issue(
            target(5),
            10,
            2,
            EffectKind::Bool,
            EffectPayload::Bool(true),
        )
    });
    assert_eq!(book.last_nonce(), 4);
    let first = book.pending_requests().next().unwrap().clone();
    book.cancel(first.operation_id, first.target, 12, 2)
        .unwrap();
    assert_eq!(issue(&mut book, target(5)).operation_id.nonce(), 5);
}

#[test]
fn invalid_identity_is_rejected_for_issue_response_and_cancellation() {
    let mut book = book();
    for invalid in [
        target(0),
        EntityRef {
            object_id: ObjectId(0),
            generation: 1,
        },
        EntityRef {
            object_id: ObjectId(-1),
            generation: 1,
        },
    ] {
        assert_atomic(&mut book, |book| {
            book.issue(invalid, 10, 2, EffectKind::Bool, EffectPayload::Bool(true))
        });
        assert_atomic(&mut book, |book| book.cancel_target(invalid, 10, 2));
    }
    let request = issue(&mut book, target(1));
    let mut invalid = response(&request);
    invalid.target = target(0);
    assert_atomic(&mut book, |book| {
        book.resolve(invalid, 12, 2, Some(target(1)))
    });
}

#[test]
fn cancellation_closes_only_the_matching_generation() {
    let mut book = book();
    let old = issue(&mut book, target(1));
    let another_old = issue(&mut book, target(1));
    let recycled = issue(&mut book, target(2));
    assert_atomic(&mut book, |book| {
        book.cancel(old.operation_id, target(2), 12, 2)
    });
    assert_eq!(
        book.cancel_target(target(1), 12, 3).unwrap(),
        vec![old.operation_id, another_old.operation_id]
    );
    assert_eq!(book.request(recycled.operation_id), Some(&recycled));
    assert_eq!(book.pending_len(), 1);
    let cancelled = bincode::serialize(&book).unwrap();
    assert_eq!(
        book.cancel(old.operation_id, target(1), 13, 3),
        Ok(CancelResult::AlreadyCancelled)
    );
    assert_eq!(bincode::serialize(&book).unwrap(), cancelled);
    let mut late = response(&old);
    late.delivery_epoch = 3;
    assert_eq!(
        assert_atomic(&mut book, |book| book.resolve(late, 12, 3, Some(target(1)))),
        EffectError::OperationCancelled(old.operation_id)
    );
    assert!(book.redispatch(old.operation_id, 3).is_err());
}

#[test]
fn invalid_cancellation_context_does_not_partially_cancel_a_group() {
    let mut book = book();
    issue(&mut book, target(1));
    book.issue(
        target(1),
        20,
        4,
        EffectKind::Bool,
        EffectPayload::Bool(true),
    )
    .unwrap();
    assert_atomic(&mut book, |book| book.cancel_target(target(1), 12, 4));
    assert_atomic(&mut book, |book| book.cancel_target(target(1), 20, 3));
    assert_eq!(book.pending_len(), 2);
    assert_eq!(book.cancel_target(target(1), 20, 4).unwrap().len(), 2);
}

#[test]
fn terminal_eviction_is_completion_ordered_and_never_reuses_ids() {
    let mut small = limits();
    small.max_terminal = 2;
    let mut book = EffectBook::new(1, small).unwrap();
    let first = issue(&mut book, target(1));
    let second = issue(&mut book, target(1));
    let third = issue(&mut book, target(1));
    for request in [&third, &first, &second] {
        book.resolve(response(request), 12, 2, Some(target(1)))
            .unwrap();
    }
    assert_eq!(book.terminal_len(), 2);
    assert_eq!(
        assert_atomic(&mut book, |book| {
            book.resolve(response(&third), 12, 2, Some(target(1)))
        }),
        EffectError::RetiredOperation(third.operation_id)
    );
    assert_eq!(
        book.resolve(response(&first), 12, 2, Some(target(1))),
        Ok(EffectResolution::Duplicate)
    );
    let mut restored: EffectBook =
        bincode::deserialize(&bincode::serialize(&book).unwrap()).unwrap();
    assert_eq!(issue(&mut restored, target(1)).operation_id.nonce(), 4);
    restored.validate().unwrap();
}

#[test]
fn unknown_foreign_and_unissued_operation_ids_never_resume() {
    let mut book = book();
    let request = issue(&mut book, target(1));
    for operation_id in [
        OperationId::from_parts(999, 1),
        OperationId::from_parts(book.namespace(), 0),
        OperationId::from_parts(book.namespace(), 2),
    ] {
        let mut result = response(&request);
        result.operation_id = operation_id;
        assert_eq!(
            assert_atomic(&mut book, |book| {
                book.resolve(result, 12, 2, Some(target(1)))
            }),
            EffectError::UnknownOperation(operation_id)
        );
    }
}

#[test]
fn persisted_counter_exhaustion_never_wraps_or_reuses_zero() {
    let mut book = with_snapshot_nonce(&book(), u64::MAX - 1);
    book.validate().unwrap();
    let final_request = issue(&mut book, target(1));
    assert_eq!(final_request.operation_id.nonce(), u64::MAX);
    assert_eq!(
        assert_atomic(&mut book, |book| {
            book.issue(
                target(1),
                10,
                2,
                EffectKind::Bool,
                EffectPayload::Bool(false),
            )
        }),
        EffectError::OperationIdExhausted
    );
    book.cancel(final_request.operation_id, target(1), 12, 2)
        .unwrap();
    assert_atomic(&mut book, |book| {
        book.issue(
            target(1),
            10,
            2,
            EffectKind::Bool,
            EffectPayload::Bool(false),
        )
    });
    book.validate().unwrap();
}

#[test]
fn snapshot_validation_rejects_counter_regression_and_future_context() {
    let mut valid = book();
    let first = issue(&mut valid, target(1));
    let mut regressed = with_snapshot_nonce(&valid, 0);
    assert!(regressed.validate().is_err());
    assert_atomic(&mut regressed, |book| {
        book.resolve(response(&first), 12, 2, Some(target(1)))
    });
    assert!(valid.validate_context(9, 2).is_err());
    assert!(valid.validate_context(10, 1).is_err());
    valid.validate_context(10, 2).unwrap();
    valid
        .resolve(response(&first), 12, 2, Some(target(1)))
        .unwrap();
    assert!(valid.validate_context(11, 2).is_err());
    valid.validate_context(12, 2).unwrap();
}

#[test]
fn snapshot_validation_rejects_inflated_limits_and_oversized_live_payload() {
    let mut valid = book();
    issue(&mut valid, target(1));
    // Namespace and high-water nonce occupy 16 bytes. Each persisted limit
    // occupies four bytes, so shrinking the request bound exposes hostile data
    // that would otherwise bypass the issue-time payload check on restoration.
    let mut snapshot = bincode::serialize(&valid).unwrap();
    snapshot[24..28].copy_from_slice(&2_u32.to_le_bytes());
    let restored: EffectBook = bincode::deserialize(&snapshot).unwrap();
    assert!(restored.validate().is_err());
    let mut snapshot = bincode::serialize(&valid).unwrap();
    snapshot[16..20].copy_from_slice(&(MAX_PENDING_OPERATIONS + 1).to_le_bytes());
    let restored: EffectBook = bincode::deserialize(&snapshot).unwrap();
    assert!(restored.validate().is_err());
}

#[test]
fn snapshot_validation_rejects_mismatched_request_ids_and_terminal_order() {
    let mut valid = book();
    let first = issue(&mut valid, target(1));
    let second = issue(&mut valid, target(1));

    let mut snapshot = bincode::serialize(&valid).unwrap();
    // The map key and the immutable request each encode the operation ID.
    // Corrupt only the second occurrence to expose a key/request mismatch.
    let request_id_position = snapshot
        .windows(16)
        .enumerate()
        .filter(|(_, bytes)| *bytes == first.operation_id.0)
        .nth(1)
        .unwrap()
        .0;
    snapshot[request_id_position..request_id_position + 16].copy_from_slice(&second.operation_id.0);
    let restored: EffectBook = bincode::deserialize(&snapshot).unwrap();
    assert!(restored.validate().is_err());

    valid
        .resolve(response(&first), 12, 2, Some(target(1)))
        .unwrap();
    valid
        .resolve(response(&second), 12, 2, Some(target(1)))
        .unwrap();
    let mut snapshot = bincode::serialize(&valid).unwrap();
    let last_order_entry = snapshot.len() - 16;
    snapshot[last_order_entry..].copy_from_slice(&first.operation_id.0);
    let restored: EffectBook = bincode::deserialize(&snapshot).unwrap();
    assert!(restored.validate().is_err());

    // Terminal response values also remain subject to restored byte limits.
    let mut snapshot = bincode::serialize(&valid).unwrap();
    snapshot[28..32].copy_from_slice(&3_u32.to_le_bytes());
    let restored: EffectBook = bincode::deserialize(&snapshot).unwrap();
    assert!(restored.validate().is_err());
}

#[test]
fn configured_limits_are_bounded_before_the_book_is_created() {
    for invalid in [
        EffectLimits {
            max_pending: 0,
            ..limits()
        },
        EffectLimits {
            max_terminal: 0,
            ..limits()
        },
        EffectLimits {
            max_pending: MAX_PENDING_OPERATIONS + 1,
            ..limits()
        },
        EffectLimits {
            max_terminal: MAX_TERMINAL_OPERATIONS + 1,
            ..limits()
        },
        EffectLimits {
            max_request_bytes: MAX_EFFECT_BYTES + 1,
            ..limits()
        },
        EffectLimits {
            max_response_bytes: MAX_EFFECT_BYTES + 1,
            ..limits()
        },
    ] {
        assert!(EffectBook::new(1, invalid).is_err());
    }
}

#[test]
fn snapshot_preserves_terminal_dedup_and_cancelled_outcomes() {
    let mut original = book();
    let completed = issue(&mut original, target(1));
    let cancelled = issue(&mut original, target(1));
    original
        .resolve(response(&completed), 12, 2, Some(target(1)))
        .unwrap();
    original
        .cancel(cancelled.operation_id, target(1), 12, 2)
        .unwrap();
    let mut restored: EffectBook =
        bincode::deserialize(&bincode::serialize(&original).unwrap()).unwrap();
    assert_eq!(restored, original);
    restored.validate().unwrap();
    assert_eq!(
        restored.resolve(response(&completed), 12, 2, Some(target(1))),
        Ok(EffectResolution::Duplicate)
    );
    assert_eq!(
        restored.resolve(response(&cancelled), 12, 2, Some(target(1))),
        Err(EffectError::OperationCancelled(cancelled.operation_id))
    );
    assert_eq!(restored, original);
}
