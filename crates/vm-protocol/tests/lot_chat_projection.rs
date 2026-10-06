//! Source chat events are a projection of decoded commands, not a simulated VM.
use wonderland_vm_protocol::{
    chat::*,
    snapshot::{ChatChannel, EntityPlatform},
    *,
};

fn channel(id: u8, view: u8, send: u8, flags: u8) -> ChatChannel {
    ChatChannel {
        id,
        name: format!("Channel{id}"),
        description: "Source channel".into(),
        minimum_view_permission: view,
        minimum_send_permission: send,
        flags,
        text_color: 0xff332211,
    }
}
fn snapshot(permission: u8, ignored: Vec<u32>) -> Snapshot {
    let mut snapshot = decode_snapshot(
        include_bytes!("fixtures/source-v38-object.fsov"),
        &DecodeLimits::default(),
    )
    .unwrap();
    snapshot.platform.lot_id = 0x40005;
    snapshot.platform.owner_id = 42;
    snapshot.platform.chat_channels = vec![
        channel(1, 0, 0, 0),
        channel(2, 1, 1, 2),
        channel(3, 2, 2, 0),
    ];
    let mut viewer = snapshot.entities[0].clone();
    viewer.object_id = 7;
    viewer.persist_id = 42;
    viewer.platform = EntityPlatform::Avatar {
        budget: 0,
        permissions: permission,
        ignored,
        jobs: vec![],
        flags: 0,
        chat_rgb: [11, 22, 33],
        chat_pitch: 0,
        chat_channel: 255,
    };
    let mut sender = viewer.clone();
    sender.object_id = 8;
    sender.persist_id = 99;
    sender.platform = EntityPlatform::Avatar {
        budget: 0,
        permissions: 4,
        ignored: vec![],
        jobs: vec![],
        flags: 0,
        chat_rgb: [44, 55, 66],
        chat_pitch: 0,
        chat_channel: 255,
    };
    snapshot.entities = vec![viewer, sender];
    let mut viewer_group = snapshot.multitile_groups[0].clone();
    viewer_group.name = "Source viewer".into();
    viewer_group.objects = vec![7];
    let mut sender_group = viewer_group.clone();
    sender_group.name = "Source sender <literal>".into();
    sender_group.objects = vec![8];
    snapshot.multitile_groups = vec![viewer_group, sender_group];
    snapshot
}
fn command(actor: u32, body: CommandBody) -> Command {
    let kind = match body {
        CommandBody::Chat { .. } => 4,
        CommandBody::StateSync { .. } => 12,
        CommandBody::AvatarJoin(_) => 0,
        CommandBody::SetIgnore { .. } => 31,
        CommandBody::ChangePermissions { .. } => 16,
        CommandBody::ChatEditChannel(_) => 40,
        CommandBody::ChatParameters { .. } => 39,
        _ => panic!("fixture body"),
    };
    Command {
        kind,
        actor_uid: Some(actor),
        offset: 0,
        consumed: 0,
        body,
    }
}
fn chat(actor: u32, text: &str, channel_id: u8) -> Command {
    command(
        actor,
        CommandBody::Chat {
            message: text.into(),
            channel_id,
        },
    )
}
fn tick(id: u32, commands: Vec<Command>) -> TickList {
    TickList {
        immediate_mode: false,
        ticks: vec![Tick {
            tick_id: id,
            random_seed: 0,
            commands,
        }],
        consumed: 0,
    }
}
fn sync(id: u32, permission: u8, ignored: Vec<u32>) -> TickList {
    tick(
        id,
        vec![command(
            0,
            CommandBody::StateSync {
                snapshot: Box::new(snapshot(permission, ignored)),
                traces: None,
            },
        )],
    )
}
fn ready(permission: u8, ignored: Vec<u32>) -> SourceChatObserver {
    let mut observer = SourceChatObserver::new(42, 0x40005);
    let initial = observer
        .observe_tick_list(sync(10, permission, ignored))
        .unwrap();
    assert!(initial.ready);
    assert!(
        initial.messages.is_empty(),
        "snapshot speech bubbles are not new chat events"
    );
    observer
}

#[test]
fn source_actor_names_colors_and_public_message_order_are_preserved() {
    let mut observer = ready(0, vec![]);
    let result = observer
        .observe_tick_list(tick(
            11,
            vec![chat(99, "Olá <b>source text</b>", 0), chat(42, "Reply", 0)],
        ))
        .unwrap();
    assert_eq!(result.messages.len(), 2);
    assert_eq!(result.messages[0].sender_uid, 99);
    assert_eq!(result.messages[0].sender_name, "Source sender <literal>");
    assert_eq!(result.messages[0].sender_color, [44, 55, 66]);
    assert_eq!(result.messages[0].text, "Olá <b>source text</b>");
    assert_eq!(result.messages[0].tick_id, Some(11));
    assert_eq!(result.messages[0].kind, SourceChatKind::Message);
    assert_eq!(result.messages[1].sender_uid, 42);
    assert_eq!(
        result
            .channels
            .iter()
            .map(|channel| channel.id)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert!(result.channels[0].show_by_default);
    assert!(!result.channels[1].show_by_default);
}

#[test]
fn private_channels_require_known_permissions_the_direct_flag_and_source_membership() {
    let mut visitor = ready(0, vec![]);
    assert!(visitor
        .observe_direct(chat(99, "Private", 0x82))
        .unwrap()
        .messages
        .is_empty());
    let mut roommate = ready(1, vec![]);
    let private = roommate.observe_direct(chat(99, "Private", 0x82)).unwrap();
    assert_eq!(private.messages.len(), 1);
    assert!(private.messages[0].private);
    assert_eq!(private.messages[0].channel_id, Some(2));
    assert!(roommate
        .observe_tick_list(tick(
            11,
            vec![
                chat(99, "Leaked broadcast", 0x82),
                chat(99, "Missing bit", 2)
            ]
        ))
        .unwrap()
        .messages
        .is_empty());
    for channel in [0x80, 0x81, 0x87, 6, 4] {
        assert!(roommate
            .observe_direct(chat(99, "Cannot view", channel))
            .unwrap()
            .messages
            .is_empty());
    }
    let mut admin = ready(4, vec![]);
    assert_eq!(
        admin
            .observe_direct(chat(99, "Admin", 0x87))
            .unwrap()
            .messages
            .len(),
        1
    );
    assert!(admin
        .observe_direct(chat(0, "No system impersonation", 0))
        .unwrap()
        .messages
        .is_empty());
    assert!(admin
        .observe_direct(chat(123, "Unknown sender", 0))
        .unwrap()
        .messages
        .is_empty());
}

#[test]
fn source_ignore_and_permission_updates_apply_before_the_next_chat_in_the_same_tick() {
    let mut observer = ready(1, vec![99]);
    assert!(observer
        .observe_direct(chat(99, "Ignored", 0))
        .unwrap()
        .messages
        .is_empty());
    let result = observer
        .observe_tick_list(tick(
            11,
            vec![
                command(
                    42,
                    CommandBody::SetIgnore {
                        target_uid: 99,
                        ignore: false,
                    },
                ),
                chat(99, "Now visible", 0),
                command(
                    0,
                    CommandBody::ChangePermissions {
                        target_uid: 42,
                        replace_uid: 0,
                        level: 0,
                        mode: 0,
                    },
                ),
            ],
        ))
        .unwrap();
    assert_eq!(result.messages.len(), 1);
    assert_eq!(result.messages[0].text, "Now visible");
    assert!(!result.channels.iter().any(|channel| channel.id == 2));
    assert!(observer
        .observe_direct(chat(99, "Now unauthorized", 0x82))
        .unwrap()
        .messages
        .is_empty());
}

#[test]
fn admitted_public_chat_is_not_reverified_after_an_earlier_same_tick_sender_demotion() {
    let mut observer = SourceChatObserver::new(42, 0x40005);
    let mut snapshot = snapshot(0, vec![]);
    snapshot.platform.chat_channels[0].minimum_send_permission = 1;
    observer
        .observe_tick_list(tick(
            10,
            vec![command(
                0,
                CommandBody::StateSync {
                    snapshot: Box::new(snapshot),
                    traces: None,
                },
            )],
        ))
        .unwrap();
    // VMServerDriver verifies the complete queue before executing any command.
    // This speaker was eligible when the native server admitted the message.
    let result = observer
        .observe_tick_list(tick(
            10,
            vec![
                command(
                    0,
                    CommandBody::ChangePermissions {
                        target_uid: 99,
                        replace_uid: 0,
                        level: 0,
                        mode: 0,
                    },
                ),
                chat(99, "Admitted before demotion", 1),
            ],
        ))
        .unwrap();
    assert_eq!(result.messages.len(), 1);
    assert_eq!(result.messages[0].text, "Admitted before demotion");
    assert!(!result.messages[0].private);
}

#[test]
fn avatar_names_require_one_distinct_source_group_despite_repeated_object_memberships() {
    let mut observer = SourceChatObserver::new(42, 0x40005);
    let mut snapshot = snapshot(0, vec![]);
    snapshot.multitile_groups[0].objects.extend([7, 7]);
    let mut conflicting = snapshot.multitile_groups[1].clone();
    conflicting.name = "Conflicting source name".into();
    snapshot.multitile_groups.push(conflicting);
    let projection = observer
        .observe_tick_list(tick(
            10,
            vec![command(
                0,
                CommandBody::StateSync {
                    snapshot: Box::new(snapshot),
                    traces: None,
                },
            )],
        ))
        .unwrap();
    assert!(
        projection.ready,
        "repeated membership in one group is unambiguous"
    );
    assert!(observer
        .observe_direct(chat(99, "Two possible source names", 0))
        .unwrap()
        .messages
        .is_empty());
    let self_chat = observer
        .observe_direct(chat(42, "One source name", 0))
        .unwrap();
    assert_eq!(self_chat.messages[0].sender_name, "Source viewer");
}

#[test]
fn historical_ticks_and_cached_resync_do_not_replay_chat_or_restore_stale_private_access() {
    let mut observer = ready(1, vec![]);
    let permission = command(
        0,
        CommandBody::ChangePermissions {
            target_uid: 42,
            replace_uid: 0,
            level: 0,
            mode: 0,
        },
    );
    assert_eq!(
        observer
            .observe_tick_list(tick(12, vec![chat(99, "Only once", 0), permission.clone()]))
            .unwrap()
            .messages
            .len(),
        1
    );
    assert!(observer
        .observe_tick_list(tick(12, vec![chat(99, "Only once", 0)]))
        .unwrap()
        .messages
        .is_empty());
    let old_snapshot = observer.observe_tick_list(sync(10, 1, vec![])).unwrap();
    assert!(!old_snapshot.ready);
    assert!(old_snapshot.channels.is_empty());
    assert!(observer
        .observe_direct(chat(99, "No stale permission", 0x82))
        .unwrap()
        .messages
        .is_empty());
    let caught_up = observer
        .observe_tick_list(tick(12, vec![chat(99, "Only once", 0), permission]))
        .unwrap();
    assert!(caught_up.ready);
    assert!(caught_up.messages.is_empty());
    assert!(!caught_up.channels.iter().any(|channel| channel.id == 2));
}

#[test]
fn identical_native_direct_deliveries_are_distinct_and_slash_commands_are_not_speech() {
    let mut observer = ready(1, vec![]);
    for _ in 0..2 {
        assert_eq!(
            observer
                .observe_direct(chat(99, "Same words", 0x82))
                .unwrap()
                .messages
                .len(),
            1
        );
    }
    let result = observer
        .observe_tick_list(tick(
            11,
            vec![
                chat(99, "", 0),
                chat(99, "/ban Someone", 0),
                chat(99, "/", 0),
            ],
        ))
        .unwrap();
    assert_eq!(result.messages.len(), 1);
    assert_eq!(result.messages[0].text, "/");
    let result = observer
        .observe_direct(chat(99, &"😀".repeat(101), 0))
        .unwrap();
    assert_eq!(result.messages[0].text.encode_utf16().count(), 200);
}

#[test]
fn join_refreshes_actor_identity_and_leave_commands_do_not_invent_completed_departures() {
    let mut observer = ready(0, vec![]);
    let prior_incarnation = observer
        .observe_direct(chat(99, "Before", 0))
        .unwrap()
        .messages[0]
        .sender_incarnation;
    let leave = Command {
        kind: 6,
        actor_uid: Some(99),
        offset: 0,
        consumed: 0,
        body: CommandBody::SourceFields {
            bytes: 99u32.to_le_bytes().to_vec(),
        },
    };
    assert!(observer
        .observe_tick_list(tick(11, vec![leave, chat(99, "Stale actor", 0)]))
        .unwrap()
        .messages
        .is_empty());
    let join = command(
        99,
        CommandBody::AvatarJoin(AvatarJoin {
            name: "New source name".into(),
            persist_id: 99,
            permissions: 0,
            ignored: vec![],
        }),
    );
    let result = observer
        .observe_tick_list(tick(12, vec![join, chat(99, "After", 0)]))
        .unwrap();
    assert_eq!(result.messages.len(), 2);
    assert_eq!(result.messages[0].kind, SourceChatKind::Join);
    assert_eq!(result.messages[1].sender_name, "New source name");
    assert!(result.messages[1].sender_incarnation > prior_incarnation);
}

#[test]
fn malformed_or_wrong_lot_frames_fail_closed_before_any_partial_chat_is_observed() {
    let mut observer = ready(4, vec![]);
    let mut bytes = vec![0, 1, 0, 0, 0, 11, 0, 0, 0];
    bytes.extend(0u64.to_le_bytes());
    bytes.extend(2i32.to_le_bytes());
    bytes.extend([4, 99, 0, 0, 0, 2, b'h', b'i', 0, 255]);
    assert!(observer
        .observe_frame(false, &bytes, &DecodeLimits::default())
        .is_err());
    assert!(!observer.projection().ready);
    assert!(observer
        .observe_direct(chat(99, "No stale state", 0x87))
        .unwrap()
        .messages
        .is_empty());
    let mut wrong = snapshot(4, vec![]);
    wrong.platform.lot_id += 1;
    assert!(observer
        .observe_tick_list(tick(
            12,
            vec![
                chat(99, "Not released", 0),
                command(
                    0,
                    CommandBody::StateSync {
                        snapshot: Box::new(wrong),
                        traces: None
                    }
                )
            ]
        ))
        .is_err());
}

#[test]
fn tick_rollover_remains_fresh_and_old_ticks_cannot_rewind_source_names() {
    let mut observer = SourceChatObserver::new(42, 0x40005);
    observer
        .observe_tick_list(sync(u32::MAX - 1, 0, vec![]))
        .unwrap();
    assert_eq!(
        observer
            .observe_tick_list(tick(u32::MAX, vec![chat(99, "Before wrap", 0)]))
            .unwrap()
            .messages
            .len(),
        1
    );
    assert_eq!(
        observer
            .observe_tick_list(tick(0, vec![chat(99, "After wrap", 0)]))
            .unwrap()
            .messages
            .len(),
        1
    );
    assert!(observer
        .observe_tick_list(tick(u32::MAX, vec![chat(99, "Old", 0)]))
        .unwrap()
        .messages
        .is_empty());
}

#[test]
fn repeated_source_names_cannot_expand_a_small_chat_packet_into_unbounded_output() {
    let mut observer = SourceChatObserver::new(42, 0x40005);
    let mut snapshot = snapshot(0, vec![]);
    snapshot.multitile_groups[1].name = "N".repeat(64 * 1024);
    observer
        .observe_tick_list(tick(
            10,
            vec![command(
                0,
                CommandBody::StateSync {
                    snapshot: Box::new(snapshot),
                    traces: None,
                },
            )],
        ))
        .unwrap();
    let result = observer.observe_tick_list(tick(
        11,
        (0..10).map(|_| chat(99, "small source body", 0)).collect(),
    ));
    assert_eq!(result.err().map(|error| error.kind), Some(ErrorKind::Limit));
    assert!(!observer.projection().ready);
}

#[test]
fn ambiguous_snapshot_avatar_ids_never_get_a_source_name_or_private_role() {
    let mut observer = SourceChatObserver::new(42, 0x40005);
    let mut snapshot = snapshot(4, vec![]);
    snapshot
        .entities
        .extend([snapshot.entities[0].clone(), snapshot.entities[0].clone()]);
    let result = observer
        .observe_tick_list(tick(
            10,
            vec![command(
                0,
                CommandBody::StateSync {
                    snapshot: Box::new(snapshot),
                    traces: None,
                },
            )],
        ))
        .unwrap();
    assert!(!result.ready);
    assert!(observer
        .observe_direct(chat(99, "Ambiguous viewer", 0x87))
        .unwrap()
        .messages
        .is_empty());
}

#[test]
fn retained_source_id_history_is_bounded_across_frames_and_recovers_from_a_fresh_snapshot() {
    let mut observer = SourceChatObserver::with_state_limits(
        42,
        0x40005,
        ChatStateLimits {
            max_entries: 20,
            max_text_bytes: 4096,
        },
    );
    let initial = observer.observe_tick_list(sync(10, 1, vec![])).unwrap();
    let mut rejected = false;
    for uid in 100..120 {
        let join = command(
            uid,
            CommandBody::AvatarJoin(AvatarJoin {
                name: format!("Visitor {uid}"),
                persist_id: uid,
                permissions: 0,
                ignored: vec![],
            }),
        );
        let leave = Command {
            kind: 6,
            actor_uid: Some(uid),
            offset: 0,
            consumed: 0,
            body: CommandBody::SourceFields {
                bytes: uid.to_le_bytes().to_vec(),
            },
        };
        if let Err(error) = observer.observe_tick_list(tick(uid, vec![join, leave])) {
            assert_eq!(error.kind, ErrorKind::Limit);
            rejected = true;
            break;
        }
    }
    assert!(
        rejected,
        "small valid frames must not grow retained identities indefinitely"
    );
    assert!(!observer.projection().ready);
    assert!(observer
        .observe_direct(chat(99, "Private after exhaustion", 0x82))
        .unwrap()
        .messages
        .is_empty());
    let recovered = observer.observe_tick_list(sync(200, 0, vec![])).unwrap();
    assert!(recovered.ready);
    assert!(recovered.viewer_incarnation > initial.viewer_incarnation);
    assert!(observer
        .observe_direct(chat(100, "Discarded identity", 0))
        .unwrap()
        .messages
        .is_empty());
}

#[test]
fn source_names_and_ignored_sets_cannot_accumulate_without_a_cross_frame_budget() {
    for ignored in [false, true] {
        let mut observer = SourceChatObserver::with_state_limits(
            42,
            0x40005,
            ChatStateLimits {
                max_entries: 20,
                max_text_bytes: 200,
            },
        );
        observer.observe_tick_list(sync(10, 0, vec![])).unwrap();
        let mut rejected = false;
        for uid in 100..120 {
            let join = command(
                uid,
                CommandBody::AvatarJoin(AvatarJoin {
                    name: if ignored {
                        "Visitor".into()
                    } else {
                        "N".repeat(60)
                    },
                    persist_id: uid,
                    permissions: 2,
                    ignored: if ignored {
                        (500..510).collect()
                    } else {
                        vec![]
                    },
                }),
            );
            if let Err(error) = observer.observe_tick_list(tick(uid, vec![join])) {
                assert_eq!(error.kind, ErrorKind::Limit);
                rejected = true;
                break;
            }
        }
        assert!(rejected);
        assert!(!observer.projection().ready);
    }
}

#[test]
fn snapshot_wrapper_names_the_next_real_tick_so_same_id_chat_and_permissions_apply_once() {
    let mut observer = ready(1, vec![]);
    let tick = tick(
        10,
        vec![
            chat(99, "First tick after snapshot", 0),
            command(
                0,
                CommandBody::ChangePermissions {
                    target_uid: 42,
                    replace_uid: 0,
                    level: 0,
                    mode: 0,
                },
            ),
        ],
    );
    let first = observer.observe_tick_list(tick.clone()).unwrap();
    assert_eq!(first.messages.len(), 1);
    assert!(!first.channels.iter().any(|channel| channel.private));
    assert!(observer
        .observe_tick_list(tick)
        .unwrap()
        .messages
        .is_empty());
}

#[test]
fn same_id_cached_snapshot_replays_the_first_role_update_without_restoring_private_access() {
    let mut observer = ready(1, vec![]);
    let update = tick(
        12,
        vec![
            command(
                0,
                CommandBody::ChangePermissions {
                    target_uid: 42,
                    replace_uid: 0,
                    level: 0,
                    mode: 0,
                },
            ),
            chat(99, "Received once", 0),
        ],
    );
    observer.observe_tick_list(update.clone()).unwrap();
    let cached = observer.observe_tick_list(sync(12, 1, vec![])).unwrap();
    assert!(!cached.ready);
    assert!(cached.channels.is_empty());
    let replayed = observer.observe_tick_list(update).unwrap();
    assert!(replayed.ready);
    assert!(replayed.messages.is_empty());
    assert!(!replayed.channels.iter().any(|channel| channel.private));
}

#[test]
fn a_cached_snapshot_from_before_viewer_join_preserves_its_incarnation_on_replay() {
    let mut observer = SourceChatObserver::new(42, 0x40005);
    let mut before_join = snapshot(0, vec![]);
    before_join
        .entities
        .retain(|entity| entity.persist_id != 42);
    before_join
        .multitile_groups
        .retain(|group| !group.objects.contains(&7));
    let old_sync = tick(
        42,
        vec![command(
            0,
            CommandBody::StateSync {
                snapshot: Box::new(before_join),
                traces: None,
            },
        )],
    );
    observer.observe_tick_list(old_sync.clone()).unwrap();
    let joined = tick(
        43,
        vec![command(
            42,
            CommandBody::AvatarJoin(AvatarJoin {
                name: "Joined viewer".into(),
                persist_id: 42,
                permissions: 1,
                ignored: vec![],
            }),
        )],
    );
    let original_incarnation = observer
        .observe_tick_list(joined.clone())
        .unwrap()
        .viewer_incarnation;
    let speech = tick(44, vec![chat(99, "Existing history", 0)]);
    observer.observe_tick_list(speech.clone()).unwrap();
    observer.observe_tick_list(old_sync).unwrap();
    observer.observe_tick_list(joined).unwrap();
    let restored = observer.observe_tick_list(speech).unwrap();
    assert!(restored.ready);
    assert!(restored.messages.is_empty());
    assert_eq!(restored.viewer_incarnation, original_incarnation);
}

#[test]
fn a_departure_after_speech_in_the_same_frame_still_publishes_loss_of_chat_authority() {
    let mut observer = ready(1, vec![]);
    let leave = Command {
        kind: 6,
        actor_uid: Some(42),
        offset: 0,
        consumed: 0,
        body: CommandBody::SourceFields {
            bytes: 42u32.to_le_bytes().to_vec(),
        },
    };
    let update = observer
        .observe_tick_list(tick(11, vec![chat(99, "Last visible words", 0), leave]))
        .unwrap();
    assert!(!update.ready);
    assert!(update.channels.is_empty());
    assert!(
        update.messages.is_empty(),
        "unavailable projections cannot retain message payloads rejected by the browser ledger"
    );
}
