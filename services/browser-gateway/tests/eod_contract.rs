use wonderland_browser_gateway::eod::EodGate;
use wonderland_game_services::ErrorCode;

#[test]
fn source_snapshot_does_not_consume_its_next_ordinary_tick_or_replay_prior_eods() {
    let mut gate = EodGate::new(42);
    let mut snapshot =
        include_bytes!("../../../crates/vm-protocol/tests/fixtures/source-v38-state-sync-tick.bin")
            .to_vec();
    snapshot[0] = 0;
    gate.observe_for_lot(false, &snapshot, 55).unwrap();
    assert!(
        gate.observe(false, &tick(41, 42, 0x8b300068, "eod_enter"))
            .unwrap()
            .is_empty()
    );
    let first = gate
        .observe(false, &tick(42, 42, 0x8b300068, "eod_enter"))
        .unwrap();
    assert_eq!(
        first.len(),
        1,
        "Source StateSync uses the next real tick ID, not an executed tick"
    );
    assert!(gate.authorize(first[0].incarnation, 0x8b300068).is_ok());
    assert!(
        gate.observe(false, &tick(42, 42, 0x8b300068, "eod_leave"))
            .unwrap()
            .is_empty()
    );
    gate.observe_for_lot(false, &snapshot, 55).unwrap();
    assert!(gate.active_plugin().is_none());
    assert!(
        gate.observe(false, &tick(42, 42, 0x8b300068, "eod_enter"))
            .unwrap()
            .is_empty()
    );
    assert!(
        gate.active_plugin().is_none(),
        "A cached snapshot cannot replay an earlier EOD grant"
    );
    assert_eq!(
        gate.observe(false, &tick(43, 42, 0x8b300068, "eod_enter"))
            .unwrap()
            .len(),
        1
    );
}

fn tick(tick: u32, actor: u32, plugin: u32, event: &str) -> Vec<u8> {
    let mut bytes = vec![0, 1, 0, 0, 0];
    bytes.extend(tick.to_le_bytes());
    bytes.extend([0; 8]);
    bytes.extend([1, 0, 0, 0, 18]);
    bytes.extend(actor.to_le_bytes());
    bytes.extend(plugin.to_le_bytes());
    bytes.push(event.len() as u8);
    bytes.extend(event.as_bytes());
    bytes.extend([0, 0]);
    bytes
}

#[test]
fn eod_actor_plugin_and_incarnation_come_only_from_original_server_events() {
    let mut gate = EodGate::new(42);
    assert_eq!(
        gate.authorize(0, 0x8b300068).unwrap_err().code,
        ErrorCode::Unauthorized
    );
    assert!(
        gate.observe(false, &tick(1, 43, 0x8b300068, "eod_enter"))
            .unwrap()
            .is_empty()
    );
    assert!(gate.active_plugin().is_none());
    let events = gate
        .observe(false, &tick(2, 42, 0x8b300068, "eod_enter"))
        .unwrap();
    assert_eq!(events.len(), 1);
    let incarnation = events[0].incarnation;
    assert!(gate.authorize(incarnation, 0x8b300068).is_ok());
    assert_eq!(
        gate.authorize(incarnation, 0xcb492685).unwrap_err().code,
        ErrorCode::Unauthorized
    );
    assert!(
        gate.observe(false, &tick(1, 42, 0x8b300068, "eod_leave"))
            .unwrap()
            .is_empty()
    );
    assert!(gate.authorize(incarnation, 0x8b300068).is_ok());
    gate.observe(false, &tick(3, 42, 0x8b300068, "eod_leave"))
        .unwrap();
    assert_eq!(
        gate.authorize(incarnation, 0x8b300068).unwrap_err().code,
        ErrorCode::Unauthorized
    );
}

#[test]
fn unknown_tick_cannot_preserve_permission_past_a_potential_missed_leave() {
    let mut gate = EodGate::new(42);
    let events = gate
        .observe(false, &tick(1, 42, 0x8b300068, "eod_enter"))
        .unwrap();
    let incarnation = events[0].incarnation;
    let mut unknown = tick(2, 42, 0x8b300068, "eod_enter");
    unknown[21] = 255;
    assert!(gate.observe(false, &unknown).is_err());
    assert!(gate.authorize(incarnation, 0x8b300068).is_err());
}

#[test]
fn original_server_set_outfit_uses_uid_even_when_actor_prefix_is_zero() {
    let mut gate = EodGate::new(42);
    let mut bytes = vec![0, 1, 0, 0, 0, 1, 0, 0, 0];
    bytes.extend([0; 8]);
    bytes.extend([1, 0, 0, 0, 33, 0, 0, 0, 0, 42, 0, 0, 0, 2, 0]);
    bytes.extend(9_007_199_254_740_993u64.to_le_bytes());
    gate.observe(false, &bytes).unwrap();
    assert_eq!(gate.source_events.len(), 1);
    assert_eq!(gate.source_events[0]["uid"], 42);
    assert_eq!(gate.source_events[0]["asset_id"], "9007199254740993");
    gate.observe(false, &bytes).unwrap();
    assert!(gate.source_events.is_empty());
}

#[test]
fn source_snapshot_clears_active_eod_and_carries_only_its_source_lot() {
    let mut gate = EodGate::new(42);
    let events = gate
        .observe(false, &tick(1, 42, 0x8b300068, "eod_enter"))
        .unwrap();
    let incarnation = events[0].incarnation;
    gate.observe(
        true,
        include_bytes!("../../../crates/vm-protocol/tests/fixtures/source-v38-state-sync.bin"),
    )
    .unwrap();
    assert!(gate.state_sync);
    assert!(gate.authorize(incarnation, 0x8b300068).is_err());
    gate.observe(false, &tick(2, 42, 0x8b300068, "eod_enter"))
        .unwrap();
    assert!(gate.active_plugin().is_some());
    gate.observe(false, &tick(2, 42, 0x8b300068, "eod_leave"))
        .unwrap();
    assert!(
        gate.active_plugin().is_some(),
        "duplicate tick cannot apply an EOD leave"
    );
}

#[test]
fn wrong_lot_snapshot_cannot_confirm_refresh_or_retain_eod_authority() {
    let mut gate = EodGate::new(42);
    gate.observe(false, &tick(1, 42, 0x8b300068, "eod_enter"))
        .unwrap();
    assert!(
        gate.observe_for_lot(
            true,
            include_bytes!("../../../crates/vm-protocol/tests/fixtures/source-v38-state-sync.bin"),
            99
        )
        .is_err()
    );
    assert!(gate.active_plugin().is_none());
    assert!(!gate.state_sync);
    gate.observe_for_lot(
        true,
        include_bytes!("../../../crates/vm-protocol/tests/fixtures/source-v38-state-sync.bin"),
        55,
    )
    .unwrap();
    assert!(gate.state_sync);
}

#[test]
fn asynchronous_older_state_sync_is_applied_without_replaying_historical_eod_permissions() {
    let mut gate = EodGate::new(42);
    gate.observe(false, &tick(103, 42, 0x8b300068, "eod_enter"))
        .unwrap();
    let mut snapshot =
        include_bytes!("../../../crates/vm-protocol/tests/fixtures/source-v38-state-sync-tick.bin")
            .to_vec();
    snapshot[0] = 0;
    snapshot[5..9].copy_from_slice(&102u32.to_le_bytes());
    gate.observe_for_lot(false, &snapshot, 55).unwrap();
    assert!(
        gate.state_sync,
        "LastSync captured at102 can legitimately arrive after tick103"
    );
    assert!(gate.active_plugin().is_none());
    assert_eq!(gate.last_tick(), Some(103));
    for old_tick in [102, 103] {
        assert!(
            gate.observe(false, &tick(old_tick, 42, 0x8b300068, "eod_enter"))
                .unwrap()
                .is_empty()
        );
        assert!(
            gate.active_plugin().is_none(),
            "snapshot history cannot restore an old EOD grant"
        );
    }
    gate.observe_for_lot(false, &snapshot, 55).unwrap();
    assert!(
        gate.state_sync,
        "the original server may reuse its cached LastSync for a requested resync"
    );
    assert_eq!(gate.last_tick(), Some(103));
}

#[test]
fn immediate_leave_has_no_simulation_tick_sequence_and_revokes_the_active_plugin() {
    let mut gate = EodGate::new(42);
    gate.observe(false, &tick(10, 42, 0x8b300068, "eod_enter"))
        .unwrap();
    let mut leave = tick(0, 42, 0x8b300068, "eod_leave");
    leave[0] = 1;
    let events = gate.observe(false, &leave).unwrap();
    assert_eq!(events.len(), 1);
    assert!(gate.active_plugin().is_none());
    assert_eq!(gate.last_tick(), Some(10));
}
