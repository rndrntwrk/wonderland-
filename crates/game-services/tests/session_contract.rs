use wonderland_game_services::*;

#[test]
fn late_response_cannot_complete_a_new_epoch_operation() {
    let mut ledger = OperationLedger::new(7);
    ledger.begin(7, "buy-old", "purchase").unwrap();
    assert_eq!(ledger.reset(8), ["buy-old"]);
    ledger.begin(8, "buy-new", "purchase").unwrap();
    assert_eq!(ledger.complete(7, "purchase"), None);
    assert_eq!(ledger.complete(8, "purchase"), Some("buy-new".into()));
}

#[test]
fn uncorrelated_legacy_operations_are_serialized_and_never_replayed() {
    let mut ledger = OperationLedger::new(1);
    ledger.begin(1, "first", "purchase").unwrap();
    assert_eq!(
        ledger.begin(1, "second", "purchase").unwrap_err().code,
        ErrorCode::OperationPending
    );
    assert_eq!(ledger.complete(1, "purchase"), Some("first".into()));
    assert_eq!(
        ledger.begin(1, "first", "purchase").unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        ledger.begin(0, "stale", "purchase").unwrap_err().code,
        ErrorCode::StaleEpoch
    );
}

#[test]
fn separate_sessions_do_not_share_response_correlations() {
    let mut alice = OperationLedger::new(1);
    let mut bob = OperationLedger::new(1);
    alice.begin(1, "alice", "mail").unwrap();
    assert_eq!(bob.complete(1, "mail"), None);
    assert_eq!(alice.complete(1, "mail"), Some("alice".into()));
}

#[test]
fn disconnecting_within_an_epoch_does_not_enable_replaying_a_completed_write() {
    let mut ledger = OperationLedger::new(1);
    ledger.begin(1, "purchase-once", "purchase").unwrap();
    ledger.complete(1, "purchase");
    ledger.reset(1);
    assert_eq!(
        ledger
            .begin(1, "purchase-once", "purchase")
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );
}
