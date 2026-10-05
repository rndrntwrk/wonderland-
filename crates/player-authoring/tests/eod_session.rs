use wonderland_player_authoring::*;
fn actor() -> SourceActorLot {
    SourceActorLot {
        avatar_id: 42,
        lot_id: None,
        location: 0x123456,
        epoch: 5,
        incarnation: 8,
    }
}
fn event(incarnation: u64, name: &str) -> SourceEodMessage {
    SourceEodMessage {
        actor_uid: 42,
        plugin_id: DRESSER_PLUGIN,
        event_name: name.into(),
        text: Some(String::new()),
        binary: None,
        incarnation,
    }
}
fn stock(ids: &[u32]) -> Vec<u8> {
    let mut bytes = (ids.len() as i32).to_be_bytes().to_vec();
    for id in ids {
        bytes.extend(id.to_be_bytes());
        bytes.extend((u64::MAX - u64::from(*id)).to_be_bytes());
        bytes.extend(1000i32.to_be_bytes());
        bytes.extend(500i32.to_be_bytes());
        bytes.extend(1u16.to_be_bytes());
        bytes.extend(42u32.to_be_bytes());
        bytes.push(0);
        bytes.extend(0u16.to_be_bytes());
    }
    bytes
}
#[test]
fn only_actual_enter_owns_a_dresser_session_and_outfits() {
    let mut state = AuthoringState::default();
    let mut outfits = event(10, "set_outfits");
    outfits.text = None;
    outfits.binary = Some(stock(&[1, 2]));
    assert!(!state.observe_eod(actor(), &outfits).unwrap());
    assert!(state.observe_eod(actor(), &event(10, "eod_enter")).unwrap());
    assert!(state.observe_eod(actor(), &outfits).unwrap());
    assert_eq!(state.snapshot.as_ref().unwrap().outfits.len(), 2);
    let request = state
        .submit(AuthoringIntent::Wear { outfit_id: 2 })
        .unwrap();
    assert!(matches!(
        request.wire,
        AuthoringWire::Eod {
            incarnation: 10,
            ..
        }
    ));
    let mut foreign = outfits.clone();
    foreign.actor_uid = 99;
    assert!(!state.observe_eod(actor(), &foreign).unwrap());
}
#[test]
fn deletion_confirmation_requires_actual_owned_stock_change_and_current_incarnation() {
    let mut state = AuthoringState::default();
    state.observe_eod(actor(), &event(10, "eod_enter")).unwrap();
    let mut outfits = event(10, "set_outfits");
    outfits.text = None;
    outfits.binary = Some(stock(&[1, 2]));
    state.observe_eod(actor(), &outfits).unwrap();
    state
        .submit(AuthoringIntent::DeleteOutfit { outfit_id: 2 })
        .unwrap();
    state
        .observe_eod(actor(), &event(10, "dresser_refresh_default"))
        .unwrap();
    assert!(state.pending.is_some());
    outfits.binary = Some(stock(&[1]));
    state.observe_eod(actor(), &outfits).unwrap();
    assert_eq!(state.status, OperationState::Accepted { amount: None });
    assert!(state.pending.is_none());
    state.observe_eod(actor(), &event(11, "eod_enter")).unwrap();
    outfits.incarnation = 10;
    assert!(!state.observe_eod(actor(), &outfits).unwrap());
    assert!(state.snapshot.as_ref().unwrap().outfits.is_empty());
}
#[test]
fn wear_receipt_requires_matching_source_setoutfit_actor_scope_and_asset() {
    let mut state = AuthoringState::default();
    state.observe_eod(actor(), &event(10, "eod_enter")).unwrap();
    let mut outfits = event(10, "set_outfits");
    outfits.text = None;
    outfits.binary = Some(stock(&[1, 2]));
    state.observe_eod(actor(), &outfits).unwrap();
    state
        .submit(AuthoringIntent::Wear { outfit_id: 2 })
        .unwrap();
    assert!(
        !state
            .observe_set_outfit(&actor(), 99, 22, u64::MAX - 2)
            .unwrap()
    );
    state
        .observe_set_outfit(&actor(), 42, 23, u64::MAX - 2)
        .unwrap();
    assert!(state.pending.is_some());
    state
        .observe_set_outfit(&actor(), 42, 22, u64::MAX - 2)
        .unwrap();
    assert_eq!(state.status, OperationState::Accepted { amount: None });
}
#[test]
fn actual_dialog_exit_expires_unconfirmed_action_without_claiming_acceptance() {
    let mut state = AuthoringState::default();
    state.observe_eod(actor(), &event(10, "eod_enter")).unwrap();
    let mut outfits = event(10, "set_outfits");
    outfits.binary = Some(stock(&[1, 2]));
    state.observe_eod(actor(), &outfits).unwrap();
    state
        .submit(AuthoringIntent::Wear { outfit_id: 2 })
        .unwrap();
    state.observe_eod(actor(), &event(10, "eod_leave")).unwrap();
    assert!(matches!(state.status, OperationState::Unknown(_)));
    assert!(state.pending.is_none());
    assert!(state.snapshot.as_ref().unwrap().eod.is_none());
}
#[test]
fn rack_try_confirms_only_original_dynamic_costume_effect_and_can_close_pending() {
    let mut state = AuthoringState::default();
    let mut enter = event(10, "eod_enter");
    enter.plugin_id = RACK_CUSTOMER_PLUGIN;
    state.observe_eod(actor(), &enter).unwrap();
    let mut show = enter.clone();
    show.event_name = "rack_show".into();
    show.text = Some("0".into());
    state.observe_eod(actor(), &show).unwrap();
    let mut rows = stock(&[1]);
    rows[24..26].copy_from_slice(&2u16.to_be_bytes());
    rows[26..30].copy_from_slice(&777u32.to_be_bytes());
    let mut outfits = enter.clone();
    outfits.event_name = "set_outfits".into();
    outfits.binary = Some(rows);
    state.observe_eod(actor(), &outfits).unwrap();
    state
        .submit(AuthoringIntent::RackTry { outfit_id: 1 })
        .unwrap();
    state
        .observe_set_outfit(&actor(), 42, 22, u64::MAX - 1)
        .unwrap();
    assert!(state.pending.is_some());
    state
        .observe_set_outfit(&actor(), 42, 25, u64::MAX - 1)
        .unwrap();
    assert!(state.pending.is_none());
    assert!(matches!(state.status, OperationState::Accepted { .. }));
    state
        .submit(AuthoringIntent::RackTry { outfit_id: 1 })
        .unwrap();
    assert!(state.submit(AuthoringIntent::CloseEod).is_ok());
    assert_eq!(state.unknown_operations.len(), 1);
}
