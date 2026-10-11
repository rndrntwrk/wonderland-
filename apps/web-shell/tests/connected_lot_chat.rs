use wonderland_game_services::*;
use wonderland_vm_protocol::chat::*;
use wonderland_web_shell::connected_adapter::state::{ConnectedState, Panel};

fn session() -> SessionProjection {
    SessionProjection {
        epoch: 10,
        state: SessionState::LotReady,
        avatar_id: Some(42),
        shard_name: Some("Source city".into()),
        lot_location: Some(55),
        lot_incarnation: Some(9),
        capabilities: vec![],
    }
}
fn state() -> ConnectedState {
    let mut state = ConnectedState::default();
    let epoch = state.ledger.begin_login();
    assert!(state.accept_account(epoch, session(), vec![], vec![]));
    state.ledger.open_transport(epoch);
    state
}
fn channel(id: u8, private: bool, show_by_default: bool) -> VisibleChatChannel {
    VisibleChatChannel {
        id,
        name: format!("Channel {id}"),
        description: "Source description".into(),
        private,
        show_by_default,
        text_color: [255; 3],
    }
}
fn message(sender: u32, channel: u8, text: &str) -> SourceChatMessage {
    SourceChatMessage {
        kind: SourceChatKind::Message,
        sender_uid: sender,
        sender_incarnation: 2,
        sender_name: "Source sender".into(),
        sender_color: [10, 20, 30],
        channel_id: Some(channel),
        channel_name: Some(format!("Channel {channel}")),
        private: channel == 2,
        text: text.into(),
        tick_id: Some(11),
    }
}
fn delivery(sequence: u64, messages: Vec<SourceChatMessage>) -> LotChatDelivery {
    LotChatDelivery {
        lot_incarnation: 9,
        sequence,
        projection: ChatProjection {
            viewer_id: 42,
            viewer_incarnation: 1,
            lot_location: 55,
            ready: true,
            channels: vec![
                channel(0, false, true),
                channel(1, false, false),
                channel(2, true, true),
            ],
            messages,
        },
    }
}
fn receive(state: &mut ConnectedState, delivery: LotChatDelivery) -> bool {
    state.receive(
        state.ledger.epoch,
        GatewayEnvelope {
            epoch: 10,
            operation_id: None,
            event: GatewayEvent::SourceEvent {
                family: "lot_chat".into(),
                source_code: None,
                data: serde_json::to_value(delivery).unwrap(),
            },
        },
    )
}

#[test]
fn source_messages_are_ordered_deduplicated_and_do_not_pollute_private_message_events() {
    let mut state = state();
    let first = delivery(1, vec![message(99, 0, "First"), message(42, 0, "Reply")]);
    assert!(receive(&mut state, first.clone()));
    assert!(!receive(&mut state, first));
    assert!(receive(
        &mut state,
        delivery(2, vec![message(99, 0, "First")])
    ));
    assert_eq!(
        state
            .lot_chat
            .visible_messages()
            .iter()
            .map(|m| m.message.text.as_str())
            .collect::<Vec<_>>(),
        vec!["First", "Reply", "First"]
    );
    assert_eq!(state.lot_chat.unread_count(), 2);
    assert!(state.events.is_empty());
    assert!(state.ledger.unread.is_empty());
}

#[test]
fn source_epoch_actor_lot_location_incarnation_and_sequence_fence_every_delivery() {
    let mut state = state();
    assert!(receive(
        &mut state,
        delivery(2, vec![message(99, 0, "Current")])
    ));
    for kind in 0..4 {
        let mut stale = delivery(3, vec![message(99, 0, "Must not appear")]);
        match kind {
            0 => stale.lot_incarnation = 8,
            1 => stale.projection.viewer_id = 43,
            2 => stale.projection.lot_location = 56,
            _ => stale.projection.viewer_incarnation = 0,
        }
        assert!(!receive(&mut state, stale));
    }
    assert!(!receive(
        &mut state,
        delivery(1, vec![message(99, 0, "Old sequence")])
    ));
    let stale = GatewayEnvelope {
        epoch: 9,
        operation_id: None,
        event: GatewayEvent::SourceEvent {
            family: "lot_chat".into(),
            source_code: None,
            data: serde_json::to_value(delivery(3, vec![message(99, 0, "Old epoch")])).unwrap(),
        },
    };
    assert!(!state.receive(state.ledger.epoch, stale.clone()));
    assert!(!state.receive(state.ledger.epoch + 1, stale));
    assert_eq!(state.lot_chat.visible_messages().len(), 1);
}

#[test]
fn source_filters_hide_nondefault_and_revoked_private_channels_without_destroying_drafts() {
    let mut state = state();
    assert!(receive(
        &mut state,
        delivery(
            1,
            vec![
                message(99, 0, "Main"),
                message(99, 1, "Hidden by source default"),
                message(99, 2, "Roommates")
            ]
        )
    ));
    let key = state.lot_chat.draft_key();
    state
        .ledger
        .drafts
        .insert(key.clone(), "Unsent draft".into());
    assert_eq!(state.lot_chat.visible_messages().len(), 2);
    state.lot_chat.toggle_channel(1);
    assert_eq!(state.lot_chat.visible_messages().len(), 3);
    let mut revoked = delivery(2, vec![]);
    revoked
        .projection
        .channels
        .retain(|channel| !channel.private);
    assert!(receive(&mut state, revoked));
    assert_eq!(state.lot_chat.visible_messages().len(), 2);
    assert_eq!(state.lot_chat.unread_count(), 1);
    assert_eq!(state.draft(&key), "Unsent draft");
}

#[test]
fn reading_earlier_messages_preserves_scroll_and_accumulates_unread_until_the_end_is_shown() {
    let mut state = state();
    state.panel = Some(Panel::Chat);
    state.chat_lot = true;
    assert!(receive(
        &mut state,
        delivery(1, vec![message(99, 0, "Already visible")])
    ));
    assert_eq!(state.lot_chat.unread_count(), 0);
    state.lot_chat.set_scroll(28, false);
    assert!(receive(
        &mut state,
        delivery(2, vec![message(99, 0, "New while scrolled up")])
    ));
    assert_eq!(state.lot_chat.scroll_top, 28);
    assert!(!state.lot_chat.at_bottom);
    assert_eq!(state.lot_chat.unread_count(), 1);
    state.lot_chat.set_scroll(100, true);
    state.lot_chat.mark_read();
    assert_eq!(state.lot_chat.unread_count(), 0);
    assert!(receive(
        &mut state,
        delivery(3, vec![message(99, 0, "At the end")])
    ));
    assert_eq!(state.lot_chat.unread_count(), 0);
}

#[test]
fn actor_reincarnation_and_new_lot_admission_clear_history_and_keep_drafts_scoped_to_the_old_lot() {
    let mut state = state();
    assert!(receive(
        &mut state,
        delivery(1, vec![message(99, 2, "Old private history")])
    ));
    let old_key = state.lot_chat.draft_key();
    state
        .ledger
        .drafts
        .insert(old_key.clone(), "Private draft".into());
    let mut next_actor = delivery(2, vec![message(99, 0, "New incarnation")]);
    next_actor.projection.viewer_incarnation = 3;
    assert!(receive(&mut state, next_actor));
    assert_eq!(state.lot_chat.visible_messages().len(), 1);
    assert_ne!(state.lot_chat.draft_key(), old_key);
    assert_eq!(state.draft(&old_key), "Private draft");
    let mut session = session();
    session.lot_incarnation = Some(10);
    assert!(state.receive(
        state.ledger.epoch,
        GatewayEnvelope {
            epoch: 10,
            operation_id: None,
            event: GatewayEvent::Session { session }
        }
    ));
    assert!(state.lot_chat.visible_messages().is_empty());
    assert_eq!(state.lot_chat.unread_count(), 0);
    assert!(!receive(
        &mut state,
        delivery(99, vec![message(99, 0, "From prior admission")])
    ));
}

#[test]
fn transport_loss_hides_private_channels_and_rejects_deliveries_until_reconnected() {
    let mut state = state();
    assert!(receive(
        &mut state,
        delivery(1, vec![message(99, 2, "Private")])
    ));
    state
        .ledger
        .close_transport(state.ledger.epoch, "Interrupted");
    state.clear_live_hud();
    assert!(!state.lot_chat.ready);
    assert!(state.lot_chat.visible_messages().is_empty());
    assert!(!receive(
        &mut state,
        delivery(2, vec![message(99, 0, "Late socket")])
    ));
}

#[test]
fn explicit_private_message_entry_points_leave_the_lot_tab_and_read_only_the_selected_conversation()
{
    let mut state = state();
    state.panel = Some(Panel::Chat);
    state.chat_lot = true;
    state.ledger.selected_person = Some(99);
    assert!(state.receive(state.ledger.epoch, GatewayEnvelope { epoch: 10, operation_id: None, event: GatewayEvent::SourceEvent { family: "instant_message".into(), source_code: None, data: serde_json::json!({"type":0,"from":99,"ack_id":"private-receipt","message":"For you only"}) } }));
    assert_eq!(state.ledger.unread.get(&99), Some(&1));
    state.ledger.unread.insert(100, 3);
    state.select_private_conversation(99);
    assert_eq!(state.panel, Some(Panel::Chat));
    assert!(
        !state.chat_lot,
        "the Profile Message action must never compose public lot chat"
    );
    assert_eq!(state.ledger.selected_person, Some(99));
    assert_eq!(state.ledger.unread.get(&99), Some(&0));
    assert_eq!(state.ledger.unread.get(&100), Some(&3));
}
