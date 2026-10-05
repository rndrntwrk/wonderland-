// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Source-derived real-host acceptance cases. Native RNG is not System.Random.

use wonderland_eod_runtime::*;

struct Auth;
impl ConnectionAuthority for Auth {
    fn authenticated_actor(&self, connection: ConnectionId) -> Option<ActorId> {
        (!connection.0.is_multiple_of(100)).then_some(ActorId(connection.0 % 100))
    }
}

struct NoRegisters;
impl RegisterSource for NoRegisters {
    fn timer_registers(&self, _: InvokerId) -> Option<TimerRegisters> {
        None
    }
}
fn identity(epoch: u64) -> HostIdentity {
    HostIdentity {
        scope: HostScopeId(70),
        epoch,
    }
}
fn host() -> NativeHost {
    NativeHost::new(identity(1), HostLimits::default()).unwrap()
}
fn controller(host: &mut NativeHost, input: GameControllerInput) -> GameControllerTicket {
    host.connect_game_controller(GameControllerRequest {
        object: 100,
        invoker: InvokerId(900),
        input,
    })
    .unwrap()
}
fn join(
    host: &mut NativeHost,
    controller: GameControllerTicket,
    player: u64,
    input: GamePlayerInput,
) -> SessionTicket {
    host.join_game(
        &Auth,
        GamePlayerRequest {
            connection: ConnectionId(player),
            game: controller.instance_address(),
            invoker: InvokerId(200 + player as u32),
            avatar_object: 100 + player as i16,
            input,
        },
    )
    .unwrap()
}
fn text_msg<'a>(
    ticket: SessionTicket,
    plugin: PluginId,
    sequence: u64,
    event: &'static str,
    payload: &'a str,
) -> ClientMessage<'a> {
    ClientMessage {
        version: protocol::PROTOCOL_VERSION,
        ticket,
        plugin,
        sequence,
        event,
        payload: WirePayload::Text(payload),
    }
}
fn private(host: &mut NativeHost, player: u64, ticket: SessionTicket) -> Vec<(String, Vec<u8>)> {
    host.take_private(&Auth, ConnectionId(player), ticket)
        .unwrap()
        .into_iter()
        .map(|message| {
            (
                message.event().to_string(),
                match message.body() {
                    UiBody::Text(text) => text.as_bytes().to_vec(),
                    UiBody::Binary(bytes) => bytes.to_vec(),
                },
            )
        })
        .collect()
}
fn events(host: &mut NativeHost) -> Vec<(InvokerId, i16, Vec<i16>)> {
    host.take_public_events()
        .into_iter()
        .map(|event| event.source_event())
        .collect()
}
fn ticks(host: &mut NativeHost, count: usize) {
    for _ in 0..count {
        host.tick(&Auth, &NoRegisters).unwrap();
    }
}
fn clear(host: &mut NativeHost, tickets: &[SessionTicket]) {
    events(host);
    for (i, ticket) in tickets.iter().enumerate() {
        private(host, i as u64 + 1, *ticket);
    }
}

#[test]
fn paperchase_shared_choices_preserve_packed_events_and_strict_tick_thresholds() {
    let mut host = host();
    let controller = controller(
        &mut host,
        GameControllerInput::PaperChase {
            seed: PrivateSeed::new(42),
        },
    );
    let tickets: Vec<_> = (1..=3)
        .map(|player| {
            join(
                &mut host,
                controller,
                player,
                GamePlayerInput::PaperChase {
                    slot: player as i16,
                },
            )
        })
        .collect();
    let last = private(&mut host, 3, tickets[2]);
    assert_eq!(
        last,
        vec![
            ("eod_enter".into(), vec![]),
            ("paperchase_show".into(), vec![]),
            ("paperchase_players".into(), b"101\n102\n103".to_vec()),
            ("paperchase_state".into(), b"2".to_vec()),
            ("paperchase_state".into(), b"3".to_vec()),
            (
                "paperchase_letters".into(),
                b"-1\n-1\n-1\n-1\n-1\n-1\n-1".to_vec()
            ),
        ]
    );
    clear(&mut host, &tickets);
    for (i, ticket) in tickets.iter().enumerate() {
        host.receive(
            &Auth,
            ConnectionId(i as u64 + 1),
            text_msg(
                *ticket,
                registry::PAPER_CHASE_PLUGIN,
                1,
                "paperchase_chooseletter",
                &(i + 1).to_string(),
            ),
        )
        .unwrap();
    }
    let emitted = events(&mut host);
    assert_eq!(
        &emitted[..6],
        &[
            (InvokerId(900), 2, vec![257]),
            (InvokerId(900), 2, vec![514]),
            (InvokerId(900), 2, vec![771]),
            (InvokerId(900), 2, vec![257]),
            (InvokerId(900), 2, vec![514]),
            (InvokerId(900), 2, vec![771]),
        ]
    );
    assert_eq!(emitted[6].0, InvokerId(900));
    assert_eq!(emitted[6].1, 1);
    assert!((0..=3).contains(&emitted[6].2[0]));
    clear(&mut host, &tickets);
    ticks(&mut host, 420);
    assert!(events(&mut host).is_empty());
    ticks(&mut host, 1);
    assert_eq!(events(&mut host), vec![(InvokerId(900), 3, vec![])]);
    clear(&mut host, &tickets);
    ticks(&mut host, 90);
    assert!(events(&mut host).is_empty());
    ticks(&mut host, 1);
    assert_eq!(events(&mut host), vec![(InvokerId(900), 4, vec![])]);
}

#[test]
fn pizza_uses_controller_callbacks_private_hands_and_source_noop_close() {
    let mut host = host();
    let controller = controller(
        &mut host,
        GameControllerInput::PizzaMaker {
            seed: PrivateSeed::new(9),
        },
    );
    let tickets: Vec<_> = (1..=4)
        .map(|player| {
            join(
                &mut host,
                controller,
                player,
                GamePlayerInput::PizzaMaker {
                    station: player as u8 - 1,
                    tuning: PizzaTuning {
                        phone_wait_seconds: 1,
                        contribution_timeout_seconds: 2,
                        restart_delay_seconds: 1,
                        ..PizzaTuning::default()
                    },
                },
            )
        })
        .collect();
    clear(&mut host, &tickets);
    ticks(&mut host, 1);
    for (i, ticket) in tickets.iter().enumerate() {
        let output = private(&mut host, i as u64 + 1, *ticket);
        assert_eq!(
            output
                .iter()
                .map(|(event, _)| event.as_str())
                .collect::<Vec<_>>(),
            ["pizza_state", "pizza_hand", "pizza_time"]
        );
        assert_eq!(output[0].1, b"1");
        assert_eq!(output[1].1.len(), 9);
    }
    ticks(&mut host, 29);
    assert!(events(&mut host).is_empty());
    ticks(&mut host, 1);
    assert_eq!(events(&mut host), vec![(InvokerId(900), 1, vec![102])]);
    host.deliver_game_event(controller, InvokerId(900), GameVmInput::PizzaRespondPhone)
        .unwrap();
    clear(&mut host, &tickets);
    assert_eq!(
        host.receive(
            &Auth,
            ConnectionId(1),
            text_msg(tickets[0], registry::PIZZA_MAKER_PLUGIN, 1, "close", "")
        ),
        Ok(DispatchOutcome::Accepted)
    );
    assert!(events(&mut host).is_empty());
    host.receive(
        &Auth,
        ConnectionId(1),
        text_msg(
            tickets[0],
            registry::PIZZA_MAKER_PLUGIN,
            2,
            "ingredient",
            "0",
        ),
    )
    .unwrap();
    let output = private(&mut host, 1, tickets[0]);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].0, "pizza_contrib");
    host.deliver_game_event(controller, InvokerId(900), GameVmInput::PizzaAllContributed)
        .unwrap();
    host.deliver_game_event(controller, InvokerId(900), GameVmInput::PizzaRespondBake)
        .unwrap();
    assert_eq!(events(&mut host).last().unwrap().1, 4);
}

#[test]
fn maze_sends_map_only_to_logic_and_obeys_queued_phase_ticks() {
    let mut host = host();
    let controller = controller(
        &mut host,
        GameControllerInput::Maze {
            seed: PrivateSeed::new(8),
        },
    );
    let logic = join(
        &mut host,
        controller,
        1,
        GamePlayerInput::Maze {
            role: MazeRole::Logic,
        },
    );
    let charisma = join(
        &mut host,
        controller,
        2,
        GamePlayerInput::Maze {
            role: MazeRole::Charisma,
        },
    );
    clear(&mut host, &[logic, charisma]);
    ticks(&mut host, 2);
    let map = private(&mut host, 1, logic);
    let cell = private(&mut host, 2, charisma);
    assert_eq!(
        map.iter()
            .map(|(event, _)| event.as_str())
            .collect::<Vec<_>>(),
        [
            "TSOMaze_Mark_Walls",
            "TSOMaze_BlueIcon",
            "TSOMaze_GreenIcon",
            "TSOMaze_RedIcon",
            "TSOMaze_YellowIcon",
            "TSOMaze_ExitIcon"
        ]
    );
    assert_eq!(map[0].1.len(), 144);
    assert_eq!(cell.len(), 1);
    assert_eq!(cell[0].0, "TSOMaze_Init_Cell");
    assert_eq!(cell[0].1.len(), 2);
    ticks(&mut host, 240);
    assert!(
        private(&mut host, 2, charisma)
            .iter()
            .all(|(event, _)| event == "TSOMaze_Update_Timer")
    );
    ticks(&mut host, 1);
    assert!(private(&mut host, 2, charisma).is_empty());
    ticks(&mut host, 1);
    assert_eq!(private(&mut host, 2, charisma)[0].0, "TSOMaze_Update_Cell");
    assert!(events(&mut host).is_empty());
}

#[derive(Default, Clone)]
struct Store(Vec<u8>);
impl PrivateCheckpointStore for Store {
    fn write_private(&mut self, _: PrivateCheckpointKey, bytes: &[u8]) -> Result<(), StoreError> {
        self.0 = bytes.to_vec();
        Ok(())
    }
    fn read_private(
        &mut self,
        _: PrivateCheckpointKey,
        destination: &mut [u8],
    ) -> Result<PrivateRead, StoreError> {
        let count = destination.len().min(self.0.len());
        destination[..count].copy_from_slice(&self.0[..count]);
        Ok(PrivateRead {
            bytes_written: count,
            complete: count == self.0.len(),
        })
    }
}
fn restore(store: &mut Store, stamp: CheckpointStamp) -> Result<NativeHost, Error> {
    NativeHost::restore_from(store, identity(2), stamp, HostLimits::default())
}
fn saved(host: &mut NativeHost) -> (Store, CheckpointStamp) {
    let mut store = Store::default();
    let stamp = host.checkpoint_to(&mut store).unwrap();
    (store, stamp)
}
fn binary_msg<'a>(
    ticket: SessionTicket,
    sequence: u64,
    event: &'static str,
    body: &'a [u8],
) -> ClientMessage<'a> {
    ClientMessage {
        version: protocol::PROTOCOL_VERSION,
        ticket,
        plugin: registry::MAZE_PLUGIN,
        sequence,
        event,
        payload: WirePayload::Binary(body),
    }
}

#[test]
fn paperchase_restore_pauses_until_every_recorded_binding_returns() {
    let mut original = host();
    let control = controller(
        &mut original,
        GameControllerInput::PaperChase {
            seed: PrivateSeed::new(42),
        },
    );
    let tickets: Vec<_> = (1..=3)
        .map(|p| {
            join(
                &mut original,
                control,
                p,
                GamePlayerInput::PaperChase { slot: p as i16 },
            )
        })
        .collect();
    clear(&mut original, &tickets);
    for (i, ticket) in tickets.iter().enumerate() {
        original
            .receive(
                &Auth,
                ConnectionId(i as u64 + 1),
                text_msg(
                    *ticket,
                    registry::PAPER_CHASE_PLUGIN,
                    1,
                    "paperchase_chooseletter",
                    "1",
                ),
            )
            .unwrap();
    }
    ticks(&mut original, 300);
    clear(&mut original, &tickets);
    let (mut store, stamp) = saved(&mut original);
    assert_eq!(&store.0[..8], b"EODP\x03\x00\x01\x00");
    let mut resumed = restore(&mut store, stamp).unwrap();
    ticks(&mut resumed, 500);
    assert!(events(&mut resumed).is_empty());
    assert_eq!(
        resumed.deliver_game_event(control, InvokerId(900), GameVmInput::PizzaRespondPhone),
        Err(Error::WrongEpoch)
    );
    let rebound_control = resumed
        .rebind_game_controller(control.instance_address(), InvokerId(900))
        .unwrap();
    let first = resumed
        .rebind(
            &Auth,
            ConnectionId(101),
            tickets[0].instance_address(),
            TimerRegisters([0; 4]),
        )
        .unwrap();
    private(&mut resumed, 101, first);
    ticks(&mut resumed, 500);
    assert!(events(&mut resumed).is_empty());
    assert_eq!(
        resumed.deliver_game_event(
            rebound_control,
            InvokerId(900),
            GameVmInput::PizzaRespondPhone
        ),
        Err(Error::PluginNotReady)
    );
    for (i, ticket) in tickets.iter().enumerate().skip(1) {
        let rebound = resumed
            .rebind(
                &Auth,
                ConnectionId(i as u64 + 101),
                ticket.instance_address(),
                TimerRegisters([0; 4]),
            )
            .unwrap();
        private(&mut resumed, i as u64 + 101, rebound);
    }
    ticks(&mut resumed, 120);
    assert!(events(&mut resumed).is_empty());
    ticks(&mut resumed, 1);
    assert_eq!(events(&mut resumed), vec![(InvokerId(900), 3, vec![])]);
    assert_eq!(
        resumed.receive(
            &Auth,
            ConnectionId(101),
            text_msg(
                tickets[0],
                registry::PAPER_CHASE_PLUGIN,
                2,
                "paperchase_chooseletter",
                "1"
            )
        ),
        Err(Error::WrongEpoch)
    );
}

#[test]
fn pizza_restore_preserves_hands_rng_and_authoritative_callback_fences() {
    let mut original = host();
    let control = controller(
        &mut original,
        GameControllerInput::PizzaMaker {
            seed: PrivateSeed::new(7),
        },
    );
    let tickets: Vec<_> = (1..=4)
        .map(|p| {
            join(
                &mut original,
                control,
                p,
                GamePlayerInput::PizzaMaker {
                    station: p as u8 - 1,
                    tuning: PizzaTuning::default(),
                },
            )
        })
        .collect();
    clear(&mut original, &tickets);
    ticks(&mut original, 1);
    original
        .deliver_game_event(control, InvokerId(900), GameVmInput::PizzaRespondPhone)
        .unwrap();
    original
        .receive(
            &Auth,
            ConnectionId(1),
            text_msg(
                tickets[0],
                registry::PIZZA_MAKER_PLUGIN,
                1,
                "ingredient",
                "2",
            ),
        )
        .unwrap();
    clear(&mut original, &tickets);
    let (mut store, stamp) = saved(&mut original);
    let mut resumed = restore(&mut store, stamp).unwrap();
    assert_eq!(
        resumed.rebind_game_controller(control.instance_address(), InvokerId(901)),
        Err(Error::RecipientMismatch)
    );
    let fresh = resumed
        .rebind_game_controller(control.instance_address(), InvokerId(900))
        .unwrap();
    assert_eq!(
        resumed.deliver_game_event(fresh, InvokerId(901), GameVmInput::PizzaAllContributed),
        Err(Error::RecipientMismatch)
    );
    let mut rebound = Vec::new();
    for (i, ticket) in tickets.iter().enumerate() {
        let new = resumed
            .rebind(
                &Auth,
                ConnectionId(i as u64 + 101),
                ticket.instance_address(),
                TimerRegisters([0; 4]),
            )
            .unwrap();
        let view = private(&mut resumed, i as u64 + 101, new);
        assert_eq!(
            view.iter()
                .filter(|(event, _)| event == "pizza_hand")
                .count(),
            1
        );
        if i == 0 {
            assert!(
                view.iter()
                    .any(|(event, bytes)| event == "pizza_hand" && bytes.ends_with(b"--\n"))
            );
        }
        rebound.push(new);
    }
    original
        .deliver_game_event(control, InvokerId(900), GameVmInput::PizzaAllContributed)
        .unwrap();
    resumed
        .deliver_game_event(fresh, InvokerId(900), GameVmInput::PizzaAllContributed)
        .unwrap();
    assert_eq!(events(&mut original), events(&mut resumed));
    for i in 0..4 {
        assert_eq!(
            private(&mut original, i as u64 + 1, tickets[i]),
            private(&mut resumed, i as u64 + 101, rebound[i])
        );
    }
    let mut wrong_scope = fresh;
    wrong_scope.host_scope = HostScopeId(71);
    assert_eq!(
        resumed.deliver_game_event(wrong_scope, InvokerId(900), GameVmInput::PizzaRespondBake),
        Err(Error::WrongScope)
    );
    assert_eq!(
        resumed.receive(
            &Auth,
            ConnectionId(101),
            text_msg(
                rebound[0],
                registry::PIZZA_MAKER_PLUGIN,
                1,
                "PizzaRespondBake",
                "8"
            )
        ),
        Err(Error::EventNotAllowed)
    );
}

#[test]
fn pizza_callback_output_pressure_rolls_back_all_random_draws_and_cards() {
    let limits = HostLimits {
        max_private_messages: 16,
        ..HostLimits::default()
    };
    let mut pressured = NativeHost::new(identity(1), limits.clone()).unwrap();
    let mut clean = NativeHost::new(identity(1), limits).unwrap();
    let mut setups = Vec::new();
    for host in [&mut pressured, &mut clean] {
        let control = controller(
            host,
            GameControllerInput::PizzaMaker {
                seed: PrivateSeed::new(99),
            },
        );
        let mut tickets = Vec::new();
        for p in 1..=4 {
            tickets.push(join(
                host,
                control,
                p,
                GamePlayerInput::PizzaMaker {
                    station: p as u8 - 1,
                    tuning: PizzaTuning::default(),
                },
            ));
            clear(host, &tickets);
        }
        ticks(host, 1);
        clear(host, &tickets);
        host.deliver_game_event(control, InvokerId(900), GameVmInput::PizzaRespondPhone)
            .unwrap();
        setups.push((control, tickets));
    }
    // Four outstanding time/state pairs plus the 16-message Bake transition.
    assert_eq!(
        pressured.deliver_game_event(
            setups[0].0,
            InvokerId(900),
            GameVmInput::PizzaAllContributed
        ),
        Err(Error::QueueFull)
    );
    assert!(events(&mut pressured).is_empty());
    clear(&mut pressured, &setups[0].1);
    clear(&mut clean, &setups[1].1);
    pressured
        .deliver_game_event(
            setups[0].0,
            InvokerId(900),
            GameVmInput::PizzaAllContributed,
        )
        .unwrap();
    clean
        .deliver_game_event(
            setups[1].0,
            InvokerId(900),
            GameVmInput::PizzaAllContributed,
        )
        .unwrap();
    assert_eq!(events(&mut pressured), events(&mut clean));
    for i in 0..4 {
        assert_eq!(
            private(&mut pressured, i as u64 + 1, setups[0].1[i]),
            private(&mut clean, i as u64 + 1, setups[1].1[i])
        );
    }
    let (a, _) = saved(&mut pressured);
    let (b, _) = saved(&mut clean);
    assert_eq!(a.0, b.0);
}

#[test]
fn paperchase_bad_and_duplicate_choices_do_not_replace_the_first_choice() {
    let mut host = host();
    let control = controller(
        &mut host,
        GameControllerInput::PaperChase {
            seed: PrivateSeed::new(1),
        },
    );
    let tickets: Vec<_> = (1..=3)
        .map(|p| {
            join(
                &mut host,
                control,
                p,
                GamePlayerInput::PaperChase { slot: 0 },
            )
        })
        .collect();
    clear(&mut host, &tickets);
    host.receive(
        &Auth,
        ConnectionId(1),
        text_msg(
            tickets[0],
            registry::PAPER_CHASE_PLUGIN,
            1,
            "paperchase_chooseletter",
            "garbage",
        ),
    )
    .unwrap();
    assert!(events(&mut host).is_empty());
    host.receive(
        &Auth,
        ConnectionId(1),
        text_msg(
            tickets[0],
            registry::PAPER_CHASE_PLUGIN,
            2,
            "paperchase_chooseletter",
            "2",
        ),
    )
    .unwrap();
    clear(&mut host, &tickets);
    host.receive(
        &Auth,
        ConnectionId(1),
        text_msg(
            tickets[0],
            registry::PAPER_CHASE_PLUGIN,
            3,
            "paperchase_chooseletter",
            "3",
        ),
    )
    .unwrap();
    assert!(events(&mut host).is_empty());
    assert!(private(&mut host, 1, tickets[0]).is_empty());
    for (i, ticket) in tickets.iter().enumerate().skip(1) {
        host.receive(
            &Auth,
            ConnectionId(i as u64 + 1),
            text_msg(
                *ticket,
                registry::PAPER_CHASE_PLUGIN,
                1,
                "paperchase_chooseletter",
                "1",
            ),
        )
        .unwrap();
    }
    assert!(events(&mut host).contains(&(InvokerId(900), 2, vec![258])));
    assert_eq!(
        host.receive(
            &Auth,
            ConnectionId(1),
            text_msg(
                tickets[0],
                registry::PAPER_CHASE_PLUGIN,
                4,
                "paperchase_chooseletter",
                "1"
            )
        ),
        Err(Error::PluginNotReady)
    );
}

#[test]
fn controller_teardown_closes_all_seats_and_rejects_old_capabilities() {
    let mut host = host();
    let control = controller(
        &mut host,
        GameControllerInput::PaperChase {
            seed: PrivateSeed::new(4),
        },
    );
    let tickets: Vec<_> = (1..=3)
        .map(|p| {
            join(
                &mut host,
                control,
                p,
                GamePlayerInput::PaperChase { slot: p as i16 },
            )
        })
        .collect();
    clear(&mut host, &tickets);
    host.disconnect_invoker(InvokerId(900)).unwrap();
    assert_eq!(
        events(&mut host),
        vec![
            (InvokerId(900), -1, vec![]),
            (InvokerId(201), -1, vec![]),
            (InvokerId(202), -1, vec![]),
            (InvokerId(203), -1, vec![])
        ]
    );
    for (i, ticket) in tickets.iter().enumerate() {
        assert_eq!(
            private(&mut host, i as u64 + 1, *ticket),
            vec![("eod_leave".into(), vec![])]
        );
    }
    assert_eq!(
        host.deliver_game_event(control, InvokerId(900), GameVmInput::PizzaRespondPhone),
        Err(Error::StaleSession)
    );
    assert_eq!(
        host.receive(
            &Auth,
            ConnectionId(1),
            text_msg(tickets[0], registry::PAPER_CHASE_PLUGIN, 1, "close", "")
        ),
        Err(Error::StaleSession)
    );
}

// All fixtures in this helper contain game seats with no persistence bindings.
fn checkpoint_offsets(bytes: &[u8]) -> (Vec<usize>, Vec<(usize, usize)>) {
    let u32_at = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let mut at = 68;
    let mut players = Vec::new();
    for _ in 0..u32_at(64) {
        let length = u32_at(at + 66);
        players.push(at + 70);
        at += 71 + length;
    }
    let controllers = u32_at(at);
    at += 4 + controllers * 16;
    let count = u32_at(at);
    at += 4;
    let mut groups = Vec::new();
    for _ in 0..count {
        let len = u32_at(at + 54);
        groups.push((at, at + 58));
        at += 58 + len;
    }
    (players, groups)
}

#[test]
fn checkpoints_reject_dangling_and_duplicate_seats_and_impossible_card_state() {
    let mut host = host();
    let control = controller(
        &mut host,
        GameControllerInput::PizzaMaker {
            seed: PrivateSeed::new(77),
        },
    );
    let tickets: Vec<_> = (1..=4)
        .map(|p| {
            join(
                &mut host,
                control,
                p,
                GamePlayerInput::PizzaMaker {
                    station: p as u8 - 1,
                    tuning: PizzaTuning::default(),
                },
            )
        })
        .collect();
    clear(&mut host, &tickets);
    ticks(&mut host, 1);
    clear(&mut host, &tickets);
    let (store, stamp) = saved(&mut host);
    let (players, groups) = checkpoint_offsets(&store.0);
    for kind in 0..5 {
        let mut broken = store.clone();
        match kind {
            0 => broken.0[players[0]..players[0] + 8].copy_from_slice(&999u64.to_le_bytes()),
            1 => {
                let value: [u8; 8] = broken.0[groups[0].0 + 22..groups[0].0 + 30]
                    .try_into()
                    .unwrap();
                broken.0[groups[0].0 + 30..groups[0].0 + 38].copy_from_slice(&value);
            }
            2 => broken.0[groups[0].1 + 21] = 18,
            3 => broken.0[groups[0].1 + 40] = 121,
            4 => broken.0[groups[0].1 + 1] = 3, // Bake without contributions/result.
            _ => unreachable!(),
        }
        assert!(
            matches!(restore(&mut broken, stamp), Err(Error::InvalidCheckpoint)),
            "corruption {kind}"
        );
    }
}

#[test]
fn maze_restore_rejects_broken_graph_and_preserves_each_roles_view() {
    let mut host = host();
    let control = controller(
        &mut host,
        GameControllerInput::Maze {
            seed: PrivateSeed::new(73),
        },
    );
    let logic = join(
        &mut host,
        control,
        1,
        GamePlayerInput::Maze {
            role: MazeRole::Logic,
        },
    );
    let charisma = join(
        &mut host,
        control,
        2,
        GamePlayerInput::Maze {
            role: MazeRole::Charisma,
        },
    );
    ticks(&mut host, 244);
    clear(&mut host, &[logic, charisma]);
    let (store, stamp) = saved(&mut host);
    let (_, groups) = checkpoint_offsets(&store.0);
    let mut broken = store.clone();
    broken.0[groups[0].1 + 49] = 0;
    assert!(matches!(
        restore(&mut broken, stamp),
        Err(Error::InvalidCheckpoint)
    ));
    let mut broken = store.clone();
    broken.0[groups[0].1 + 2] = 2;
    assert!(
        matches!(restore(&mut broken, stamp), Err(Error::InvalidCheckpoint)),
        "Solving cannot queue itself"
    );
    let mut valid = store;
    let mut resumed = restore(&mut valid, stamp).unwrap();
    let new_logic = resumed
        .rebind(
            &Auth,
            ConnectionId(101),
            logic.instance_address(),
            TimerRegisters([0; 4]),
        )
        .unwrap();
    let new_charisma = resumed
        .rebind(
            &Auth,
            ConnectionId(102),
            charisma.instance_address(),
            TimerRegisters([0; 4]),
        )
        .unwrap();
    let map = private(&mut resumed, 101, new_logic);
    let cell = private(&mut resumed, 102, new_charisma);
    assert!(
        map.iter()
            .any(|(event, body)| event == "TSOMaze_Mark_Walls" && body.len() == 144)
    );
    assert!(cell.iter().all(|(event, _)| !event.contains("Icon")
        && event != "TSOMaze_Mark_Walls"
        && event != "TSOMaze_Draw_Solution"));
    assert!(
        resumed
            .take_private(&Auth, ConnectionId(102), new_logic)
            .is_err()
    );
    resumed
        .rebind_game_controller(control.instance_address(), InvokerId(900))
        .unwrap();
    let before = cell
        .iter()
        .find(|(event, _)| event == "TSOMaze_Update_Cell")
        .unwrap()
        .1
        .clone();
    resumed
        .receive(
            &Auth,
            ConnectionId(101),
            binary_msg(new_logic, 1, "TSOMaze_Button_Click", &[0]),
        )
        .unwrap();
    assert!(private(&mut resumed, 101, new_logic).is_empty());
    assert!(private(&mut resumed, 102, new_charisma).is_empty());
    resumed
        .receive(
            &Auth,
            ConnectionId(102),
            binary_msg(new_charisma, 1, "TSOMaze_Button_Click", &[255]),
        )
        .unwrap();
    assert_eq!(private(&mut resumed, 102, new_charisma)[0].1, before);
}

#[test]
fn pizza_checkpoint_cannot_change_the_result_before_payout() {
    let mut host = host();
    let control = controller(
        &mut host,
        GameControllerInput::PizzaMaker {
            seed: PrivateSeed::new(700),
        },
    );
    let tickets: Vec<_> = (1..=4)
        .map(|p| {
            join(
                &mut host,
                control,
                p,
                GamePlayerInput::PizzaMaker {
                    station: p as u8 - 1,
                    tuning: PizzaTuning::default(),
                },
            )
        })
        .collect();
    ticks(&mut host, 1);
    host.deliver_game_event(control, InvokerId(900), GameVmInput::PizzaRespondPhone)
        .unwrap();
    host.deliver_game_event(control, InvokerId(900), GameVmInput::PizzaAllContributed)
        .unwrap();
    clear(&mut host, &tickets);
    let (mut store, stamp) = saved(&mut host);
    let (_, groups) = checkpoint_offsets(&store.0);
    let result = &mut store.0[groups[0].1 + 5];
    *result = if *result == 1 { 2 } else { 1 };
    assert!(matches!(
        restore(&mut store, stamp),
        Err(Error::InvalidCheckpoint)
    ));
}

#[test]
fn maze_can_be_solved_through_native_role_messages_and_reveals_solution_only_to_logic() {
    use std::collections::BTreeSet;
    let mut host = host();
    let control = controller(
        &mut host,
        GameControllerInput::Maze {
            seed: PrivateSeed::new(2026),
        },
    );
    let logic = join(
        &mut host,
        control,
        1,
        GamePlayerInput::Maze {
            role: MazeRole::Logic,
        },
    );
    let charisma = join(
        &mut host,
        control,
        2,
        GamePlayerInput::Maze {
            role: MazeRole::Charisma,
        },
    );
    ticks(&mut host, 244);
    events(&mut host);
    private(&mut host, 1, logic);
    let initial = private(&mut host, 2, charisma);
    let mut cell = initial
        .iter()
        .rev()
        .find(|(event, _)| event == "TSOMaze_Update_Cell")
        .unwrap()
        .1
        .clone();
    let open_for_wall = [15u8, 14, 12, 10, 6, 8, 4, 2, 13, 9, 5, 1, 11, 3, 7, 0];
    let movement = [(-1, 0), (0, -1), (0, 1), (1, 0)];
    let opposite = [3, 2, 1, 0];
    let mut position = (0, 0);
    let mut visited = BTreeSet::from([position]);
    let mut backtrack = Vec::new();
    let mut solved = false;
    for sequence in 1..=576 {
        let open = open_for_wall[cell[0] as usize];
        let next = (0..4).find(|direction| {
            open & (1 << direction) != 0
                && !visited.contains(&(
                    position.0 + movement[*direction].0,
                    position.1 + movement[*direction].1,
                ))
        });
        let direction = if let Some(direction) = next {
            backtrack.push(opposite[direction]);
            direction
        } else {
            backtrack.pop().expect("connected maze still has an exit")
        };
        position = (
            position.0 + movement[direction].0,
            position.1 + movement[direction].1,
        );
        visited.insert(position);
        host.receive(
            &Auth,
            ConnectionId(2),
            binary_msg(
                charisma,
                sequence,
                "TSOMaze_Button_Click",
                &[direction as u8],
            ),
        )
        .unwrap();
        let output = private(&mut host, 2, charisma);
        assert!(
            output
                .iter()
                .all(|(event, _)| !event.contains("Icon") && event != "TSOMaze_Draw_Solution")
        );
        let received = output
            .iter()
            .find(|(event, _)| event == "TSOMaze_Update_Cell" || event == "TSOMaze_Final_Cell")
            .unwrap();
        cell = received.1.clone();
        if sequence == 1 {
            let (mut store, stamp) = saved(&mut host);
            restore(&mut store, stamp).unwrap();
        }
        if received.0 == "TSOMaze_Final_Cell" {
            solved = true;
            assert_eq!(cell[1], 0);
            break;
        }
        ticks(&mut host, 1);
    }
    assert!(solved);
    ticks(&mut host, 1);
    assert_eq!(events(&mut host), vec![(InvokerId(900), 2, vec![])]);
    let logic_output = private(&mut host, 1, logic);
    let solution = logic_output
        .iter()
        .find(|(event, _)| event == "TSOMaze_Draw_Solution")
        .unwrap();
    assert!(solution.1.len() >= 3 && solution.1.len() <= 81 && solution.1.len() % 2 == 1);
    assert!(
        private(&mut host, 2, charisma)
            .iter()
            .all(|(event, _)| event != "TSOMaze_Draw_Solution")
    );
    let (mut store, stamp) = saved(&mut host);
    restore(&mut store, stamp).unwrap();
}

#[test]
fn cooperative_invocations_require_the_typed_native_game_boundary() {
    let mut host = NativeHost::new(
        HostIdentity {
            scope: HostScopeId(70),
            epoch: 1,
        },
        HostLimits::default(),
    )
    .unwrap();
    for plugin in [0xCA418206, 0xEA47AE39, 0x4A245A22] {
        assert_eq!(
            host.connect(
                &Auth,
                ConnectRequest {
                    connection: ConnectionId(1),
                    plugin: PluginId(plugin),
                    object: 100,
                    invoker: InvokerId(200),
                    registers: TimerRegisters([0; 4]),
                }
            ),
            Err(Error::PluginInputRequired)
        );
    }
}

#[test]
fn maze_replacement_cannot_overwrite_the_queued_reset() {
    let mut host = host();
    let control = controller(
        &mut host,
        GameControllerInput::Maze {
            seed: PrivateSeed::new(602),
        },
    );
    let logic = join(
        &mut host,
        control,
        1,
        GamePlayerInput::Maze {
            role: MazeRole::Logic,
        },
    );
    let charisma = join(
        &mut host,
        control,
        2,
        GamePlayerInput::Maze {
            role: MazeRole::Charisma,
        },
    );
    ticks(&mut host, 244);
    clear(&mut host, &[logic, charisma]);
    host.disconnect(&Auth, ConnectionId(1), logic).unwrap();
    assert_eq!(
        private(&mut host, 1, logic),
        vec![("eod_leave".into(), vec![])]
    );
    let replacement = join(
        &mut host,
        control,
        3,
        GamePlayerInput::Maze {
            role: MazeRole::Logic,
        },
    );
    private(&mut host, 3, replacement);
    private(&mut host, 2, charisma);
    events(&mut host);
    host.receive(
        &Auth,
        ConnectionId(2),
        binary_msg(charisma, 1, "TSOMaze_Button_Click", &[255]),
    )
    .unwrap();
    assert!(private(&mut host, 2, charisma).is_empty());
    ticks(&mut host, 1);
    let fresh = private(&mut host, 3, replacement);
    assert_eq!(fresh[0].0, "TSOMaze_Show_Waiting");
    assert!(fresh.iter().any(|(event, _)| event == "TSOMaze_Mark_Walls"));
    let cell = private(&mut host, 2, charisma);
    assert_eq!(
        cell.iter()
            .map(|(event, _)| event.as_str())
            .collect::<Vec<_>>(),
        ["TSOMaze_Show_Waiting", "TSOMaze_Init_Cell"]
    );
    let (mut store, stamp) = saved(&mut host);
    restore(&mut store, stamp).unwrap();
}

#[test]
fn pizza_preserves_constructor_deck_tuning_and_automatic_phase_deadlines() {
    let mut host = host();
    let control = controller(
        &mut host,
        GameControllerInput::PizzaMaker {
            seed: PrivateSeed::new(8),
        },
    );
    events(&mut host);
    let (store, _) = saved(&mut host);
    let (_, groups) = checkpoint_offsets(&store.0);
    assert_eq!(store.0[groups[0].1 + 40], 120);
    let tuning = PizzaTuning {
        phone_wait_seconds: 1,
        contribution_timeout_seconds: 2,
        restart_delay_seconds: 1,
        cards_per_small_ingredient: 0,
        cards_per_medium_ingredient: -1,
        cards_per_large_ingredient: 32767,
        cards_per_bonus_ingredient_per_size: 0,
    };
    let tickets: Vec<_> = (1..=4)
        .map(|p| {
            join(
                &mut host,
                control,
                p,
                GamePlayerInput::PizzaMaker {
                    station: p as u8 - 1,
                    tuning,
                },
            )
        })
        .collect();
    clear(&mut host, &tickets);
    let (store, _) = saved(&mut host);
    let (_, groups) = checkpoint_offsets(&store.0);
    assert_eq!(
        store.0[groups[0].1 + 40],
        120,
        "invocation tuning never repopulates the source constructor pool"
    );
    ticks(&mut host, 1);
    clear(&mut host, &tickets);
    host.deliver_game_event(control, InvokerId(900), GameVmInput::PizzaRespondPhone)
        .unwrap();
    clear(&mut host, &tickets);
    ticks(&mut host, 59);
    assert!(events(&mut host).is_empty());
    ticks(&mut host, 1);
    let bake = events(&mut host);
    assert_eq!(
        bake.iter().map(|(_, code, _)| *code).collect::<Vec<_>>(),
        [2, 2, 2, 2, 3]
    );
    host.deliver_game_event(control, InvokerId(900), GameVmInput::PizzaRespondBake)
        .unwrap();
    assert_eq!(
        events(&mut host),
        vec![(InvokerId(900), 4, bake[4].2.clone())]
    );
    clear(&mut host, &tickets);
    ticks(&mut host, 29);
    assert!(events(&mut host).is_empty());
    ticks(&mut host, 1);
    assert_eq!(events(&mut host), vec![(InvokerId(900), 5, vec![])]);
    let output = private(&mut host, 1, tickets[0]);
    assert!(
        output
            .iter()
            .any(|(event, body)| event == "pizza_state" && body == b"0")
    );
    assert!(
        !output
            .iter()
            .any(|(event, body)| event == "pizza_state" && body == b"1")
    );
    for (i, ticket) in tickets.iter().enumerate().skip(1) {
        private(&mut host, i as u64 + 1, *ticket);
    }
    ticks(&mut host, 1);
    assert!(
        private(&mut host, 1, tickets[0])
            .iter()
            .any(|(event, body)| event == "pizza_state" && body == b"1")
    );
    clear(&mut host, &tickets);
    let (mut store, stamp) = saved(&mut host);
    restore(&mut store, stamp).unwrap();
}

#[test]
fn cooperative_timeout_and_revocation_purge_private_roles_and_resume_safely() {
    let limits = HostLimits {
        idle_timeout_ticks: 3,
        ..HostLimits::default()
    };
    let mut timed = NativeHost::new(identity(1), limits.clone()).unwrap();
    let control = controller(
        &mut timed,
        GameControllerInput::PaperChase {
            seed: PrivateSeed::new(1),
        },
    );
    let ticket = join(
        &mut timed,
        control,
        1,
        GamePlayerInput::PaperChase { slot: 1 },
    );
    clear(&mut timed, &[ticket]);
    let (mut store, stamp) = saved(&mut timed);
    let mut detached = NativeHost::restore_from(&mut store, identity(2), stamp, limits).unwrap();
    ticks(&mut detached, 2);
    assert!(events(&mut detached).is_empty());
    ticks(&mut detached, 1);
    assert_eq!(
        events(&mut detached),
        vec![(InvokerId(201), -1, vec![]), (InvokerId(900), -1, vec![])]
    );
    assert_eq!(
        detached.rebind_game_controller(control.instance_address(), InvokerId(900)),
        Err(Error::StaleSession)
    );

    struct Revoked;
    impl ConnectionAuthority for Revoked {
        fn authenticated_actor(&self, connection: ConnectionId) -> Option<ActorId> {
            if connection == ConnectionId(1) {
                None
            } else {
                Auth.authenticated_actor(connection)
            }
        }
    }
    let mut live = host();
    let control = controller(
        &mut live,
        GameControllerInput::Maze {
            seed: PrivateSeed::new(2),
        },
    );
    let logic = join(
        &mut live,
        control,
        1,
        GamePlayerInput::Maze {
            role: MazeRole::Logic,
        },
    );
    let charisma = join(
        &mut live,
        control,
        2,
        GamePlayerInput::Maze {
            role: MazeRole::Charisma,
        },
    );
    ticks(&mut live, 1);
    events(&mut live);
    live.tick(&Revoked, &NoRegisters).unwrap();
    assert_eq!(events(&mut live), vec![(InvokerId(201), -1, vec![])]);
    assert_eq!(
        live.take_private(&Auth, ConnectionId(1), logic),
        Err(Error::StaleSession)
    );
    let view = private(&mut live, 2, charisma);
    assert!(
        view.iter()
            .all(|(event, _)| event != "TSOMaze_Mark_Walls" && event != "TSOMaze_Draw_Solution")
    );
    assert!(
        view.iter()
            .any(|(event, _)| event == "TSOMaze_Show_Waiting")
    );
    live.disconnect(&Auth, ConnectionId(2), charisma).unwrap();
    assert_eq!(
        events(&mut live),
        vec![(InvokerId(202), -1, vec![]), (InvokerId(900), -1, vec![])]
    );
}

#[test]
fn game_role_and_envelope_rejections_leave_sequences_and_bindings_usable() {
    let mut host = host();
    let control = controller(
        &mut host,
        GameControllerInput::Maze {
            seed: PrivateSeed::new(6),
        },
    );
    let logic = join(
        &mut host,
        control,
        1,
        GamePlayerInput::Maze {
            role: MazeRole::Logic,
        },
    );
    assert_eq!(
        host.join_game(
            &Auth,
            GamePlayerRequest {
                connection: ConnectionId(2),
                game: control.instance_address(),
                invoker: InvokerId(202),
                avatar_object: 102,
                input: GamePlayerInput::Maze {
                    role: MazeRole::Logic
                }
            }
        ),
        Err(Error::ParticipantAlreadyConnected)
    );
    assert_eq!(
        host.join_game(
            &Auth,
            GamePlayerRequest {
                connection: ConnectionId(2),
                game: control.instance_address(),
                invoker: InvokerId(202),
                avatar_object: 102,
                input: GamePlayerInput::PaperChase { slot: 1 }
            }
        ),
        Err(Error::WrongPlugin)
    );
    assert_eq!(
        host.join_game(
            &Auth,
            GamePlayerRequest {
                connection: ConnectionId(2),
                game: control.instance_address(),
                invoker: InvokerId(900),
                avatar_object: 102,
                input: GamePlayerInput::Maze {
                    role: MazeRole::Charisma
                }
            }
        ),
        Err(Error::ParticipantAlreadyConnected)
    );
    assert_eq!(
        host.join_game(
            &Auth,
            GamePlayerRequest {
                connection: ConnectionId(2),
                game: control.instance_address(),
                invoker: InvokerId(202),
                avatar_object: 101,
                input: GamePlayerInput::Maze {
                    role: MazeRole::Charisma
                }
            }
        ),
        Err(Error::ParticipantAlreadyConnected)
    );
    let charisma = join(
        &mut host,
        control,
        2,
        GamePlayerInput::Maze {
            role: MazeRole::Charisma,
        },
    );
    ticks(&mut host, 244);
    clear(&mut host, &[logic, charisma]);
    assert_eq!(
        host.receive(
            &Auth,
            ConnectionId(1),
            binary_msg(charisma, 1, "TSOMaze_Button_Click", &[255])
        ),
        Err(Error::RecipientMismatch)
    );
    assert_eq!(
        host.receive(
            &Auth,
            ConnectionId(2),
            binary_msg(charisma, 1, "unknown", &[255])
        ),
        Err(Error::EventNotAllowed)
    );
    assert_eq!(
        host.receive(
            &Auth,
            ConnectionId(2),
            text_msg(
                charisma,
                registry::MAZE_PLUGIN,
                1,
                "TSOMaze_Button_Click",
                "0"
            )
        ),
        Err(Error::EventNotAllowed)
    );
    host.receive(
        &Auth,
        ConnectionId(2),
        binary_msg(charisma, 1, "TSOMaze_Button_Click", &[]),
    )
    .unwrap();
    assert!(private(&mut host, 2, charisma).is_empty());
    host.receive(
        &Auth,
        ConnectionId(2),
        binary_msg(charisma, 2, "TSOMaze_Button_Click", &[255]),
    )
    .unwrap();
    assert_eq!(private(&mut host, 2, charisma)[0].0, "TSOMaze_Update_Cell");
}

#[test]
fn maze_round_timeout_queues_reaction_and_restores_both_random_streams() {
    let limits = HostLimits {
        idle_timeout_ticks: 30000,
        ..HostLimits::default()
    };
    let mut original = NativeHost::new(identity(1), limits.clone()).unwrap();
    let control = controller(
        &mut original,
        GameControllerInput::Maze {
            seed: PrivateSeed::new(1001),
        },
    );
    let logic = join(
        &mut original,
        control,
        1,
        GamePlayerInput::Maze {
            role: MazeRole::Logic,
        },
    );
    let charisma = join(
        &mut original,
        control,
        2,
        GamePlayerInput::Maze {
            role: MazeRole::Charisma,
        },
    );
    ticks(&mut original, 244);
    let first_map = private(&mut original, 1, logic)
        .into_iter()
        .find(|(event, _)| event == "TSOMaze_Mark_Walls")
        .unwrap()
        .1;
    private(&mut original, 2, charisma);
    events(&mut original);
    ticks(&mut original, 8998);
    assert!(events(&mut original).is_empty());
    ticks(&mut original, 1);
    assert!(events(&mut original).is_empty());
    let timers = private(&mut original, 2, charisma);
    assert_eq!(
        timers.last().unwrap(),
        &("TSOMaze_Update_Timer".into(), 0i32.to_le_bytes().to_vec())
    );
    ticks(&mut original, 1);
    assert!(events(&mut original).is_empty());
    ticks(&mut original, 1);
    assert_eq!(events(&mut original), vec![(InvokerId(900), 1, vec![])]);
    assert!(
        private(&mut original, 1, logic)
            .iter()
            .any(|(event, body)| event == "TSOMaze_Show_Result" && body == &[0])
    );
    assert_eq!(
        private(&mut original, 2, charisma),
        vec![("TSOMaze_Show_Result".into(), vec![0])]
    );
    let (mut store, stamp) = saved(&mut original);
    let mut resumed = NativeHost::restore_from(&mut store, identity(2), stamp, limits).unwrap();
    resumed
        .rebind_game_controller(control.instance_address(), InvokerId(900))
        .unwrap();
    let new_logic = resumed
        .rebind(
            &Auth,
            ConnectionId(101),
            logic.instance_address(),
            TimerRegisters([0; 4]),
        )
        .unwrap();
    let new_charisma = resumed
        .rebind(
            &Auth,
            ConnectionId(102),
            charisma.instance_address(),
            TimerRegisters([0; 4]),
        )
        .unwrap();
    private(&mut resumed, 101, new_logic);
    private(&mut resumed, 102, new_charisma);
    ticks(&mut original, 300);
    ticks(&mut resumed, 300);
    assert!(private(&mut original, 1, logic).is_empty());
    assert!(private(&mut resumed, 101, new_logic).is_empty());
    ticks(&mut original, 1);
    ticks(&mut resumed, 1);
    let new_map = private(&mut original, 1, logic);
    assert_eq!(new_map, private(&mut resumed, 101, new_logic));
    assert_eq!(
        private(&mut original, 2, charisma),
        private(&mut resumed, 102, new_charisma)
    );
    assert_ne!(
        new_map
            .iter()
            .find(|(event, _)| event == "TSOMaze_Mark_Walls")
            .unwrap()
            .1,
        first_map
    );
    assert!(events(&mut original).is_empty());
    assert!(events(&mut resumed).is_empty());
}

#[test]
fn paperchase_join_pressure_rolls_back_the_draw_and_both_identity_counters() {
    let limits = HostLimits {
        max_private_messages: 14,
        ..HostLimits::default()
    };
    let mut pressured = NativeHost::new(identity(1), limits.clone()).unwrap();
    let mut clean = NativeHost::new(identity(1), limits).unwrap();
    let mut setups = Vec::new();
    for host in [&mut pressured, &mut clean] {
        let control = controller(
            host,
            GameControllerInput::PaperChase {
                seed: PrivateSeed::new(55),
            },
        );
        let one = join(host, control, 1, GamePlayerInput::PaperChase { slot: 1 });
        clear(host, &[one]);
        let two = join(host, control, 2, GamePlayerInput::PaperChase { slot: 2 });
        setups.push((control, [one, two]));
    }
    let request = |control: GameControllerTicket| GamePlayerRequest {
        connection: ConnectionId(3),
        game: control.instance_address(),
        invoker: InvokerId(203),
        avatar_object: 103,
        input: GamePlayerInput::PaperChase { slot: 3 },
    };
    assert_eq!(
        pressured.join_game(&Auth, request(setups[0].0)),
        Err(Error::QueueFull)
    );
    clear(&mut pressured, &setups[0].1);
    clear(&mut clean, &setups[1].1);
    let a = pressured.join_game(&Auth, request(setups[0].0)).unwrap();
    let b = clean.join_game(&Auth, request(setups[1].0)).unwrap();
    assert_eq!(a, b);
    assert_eq!(events(&mut pressured), events(&mut clean));
    for i in 0..2 {
        assert_eq!(
            private(&mut pressured, i as u64 + 1, setups[0].1[i]),
            private(&mut clean, i as u64 + 1, setups[1].1[i])
        );
    }
    assert_eq!(private(&mut pressured, 3, a), private(&mut clean, 3, b));
    let (a, _) = saved(&mut pressured);
    let (b, _) = saved(&mut clean);
    assert_eq!(a.0, b.0);
}

#[test]
fn maze_tick_output_pressure_does_not_apply_a_queued_phase_or_partial_role_map() {
    let limits = HostLimits {
        max_private_messages: 6,
        ..HostLimits::default()
    };
    let mut pressured = NativeHost::new(identity(1), limits.clone()).unwrap();
    let mut clean = NativeHost::new(identity(1), limits).unwrap();
    let mut setups = Vec::new();
    for host in [&mut pressured, &mut clean] {
        let control = controller(
            host,
            GameControllerInput::Maze {
                seed: PrivateSeed::new(33),
            },
        );
        let logic = join(
            host,
            control,
            1,
            GamePlayerInput::Maze {
                role: MazeRole::Logic,
            },
        );
        let charisma = join(
            host,
            control,
            2,
            GamePlayerInput::Maze {
                role: MazeRole::Charisma,
            },
        );
        setups.push([logic, charisma]);
    }
    assert_eq!(pressured.tick(&Auth, &NoRegisters), Err(Error::QueueFull));
    assert_eq!(pressured.public_projection().tick, 0);
    clear(&mut pressured, &setups[0]);
    clear(&mut clean, &setups[1]);
    ticks(&mut pressured, 1);
    ticks(&mut clean, 1);
    for i in 0..2 {
        assert_eq!(
            private(&mut pressured, i as u64 + 1, setups[0][i]),
            private(&mut clean, i as u64 + 1, setups[1][i])
        );
    }
    let (a, _) = saved(&mut pressured);
    let (b, _) = saved(&mut clean);
    assert_eq!(a.0, b.0);
}

#[test]
fn paperchase_checkpoint_cannot_invent_a_previous_guess_score() {
    let mut host = host();
    let control = controller(
        &mut host,
        GameControllerInput::PaperChase {
            seed: PrivateSeed::new(3),
        },
    );
    let tickets: Vec<_> = (1..=3)
        .map(|p| {
            join(
                &mut host,
                control,
                p,
                GamePlayerInput::PaperChase { slot: p as i16 },
            )
        })
        .collect();
    clear(&mut host, &tickets);
    let (mut store, stamp) = saved(&mut host);
    let (_, groups) = checkpoint_offsets(&store.0);
    for offset in [4, 6, 29] {
        store.0[groups[0].1 + offset..groups[0].1 + offset + 2]
            .copy_from_slice(&0i16.to_le_bytes());
    }
    assert!(matches!(
        restore(&mut store, stamp),
        Err(Error::InvalidCheckpoint)
    ));
}

#[test]
fn pizza_checkpoint_rejects_unreachable_deadlines_and_inactive_frames() {
    let mut host = host();
    let control = controller(
        &mut host,
        GameControllerInput::PizzaMaker {
            seed: PrivateSeed::new(10),
        },
    );
    let tickets: Vec<_> = (1..=4)
        .map(|p| {
            join(
                &mut host,
                control,
                p,
                GamePlayerInput::PizzaMaker {
                    station: p as u8 - 1,
                    tuning: PizzaTuning::default(),
                },
            )
        })
        .collect();
    for phase in 0..=4 {
        match phase {
            0 => {}
            1 => ticks(&mut host, 1),
            2 => host
                .deliver_game_event(control, InvokerId(900), GameVmInput::PizzaRespondPhone)
                .unwrap(),
            3 => host
                .deliver_game_event(control, InvokerId(900), GameVmInput::PizzaAllContributed)
                .unwrap(),
            4 => host
                .deliver_game_event(control, InvokerId(900), GameVmInput::PizzaRespondBake)
                .unwrap(),
            _ => unreachable!(),
        }
        clear(&mut host, &tickets);
        let (store, stamp) = saved(&mut host);
        let (_, groups) = checkpoint_offsets(&store.0);
        let mut invalid = store.clone();
        invalid.0[groups[0].1 + 2..groups[0].1 + 4].copy_from_slice(&32767i16.to_le_bytes());
        assert!(
            matches!(restore(&mut invalid, stamp), Err(Error::InvalidCheckpoint)),
            "phase {phase} cannot extend its deadline beyond tuning"
        );
        if phase == 0 || phase == 3 {
            let mut invalid = store;
            invalid.0[groups[0].1 + 4] = 1;
            assert!(
                matches!(restore(&mut invalid, stamp), Err(Error::InvalidCheckpoint)),
                "inactive phase {phase} cannot retain fractional countdown ticks"
            );
        }
    }
}

#[test]
fn pizza_restore_preserves_source_zero_negative_and_positive_timer_tuning() {
    for delay in [-2, -1, 0, 1] {
        let mut host = host();
        let control = controller(
            &mut host,
            GameControllerInput::PizzaMaker {
                seed: PrivateSeed::new(11),
            },
        );
        let tuning = PizzaTuning {
            phone_wait_seconds: delay,
            contribution_timeout_seconds: delay,
            restart_delay_seconds: delay,
            ..PizzaTuning::default()
        };
        let tickets: Vec<_> = (1..=4)
            .map(|p| {
                join(
                    &mut host,
                    control,
                    p,
                    GamePlayerInput::PizzaMaker {
                        station: p as u8 - 1,
                        tuning,
                    },
                )
            })
            .collect();
        ticks(&mut host, 1);
        for stage in 0..=5 {
            match stage {
                0 => {}
                1 => ticks(&mut host, 30),
                2 => host
                    .deliver_game_event(control, InvokerId(900), GameVmInput::PizzaRespondPhone)
                    .unwrap(),
                3 => host
                    .deliver_game_event(control, InvokerId(900), GameVmInput::PizzaAllContributed)
                    .unwrap(),
                4 => host
                    .deliver_game_event(control, InvokerId(900), GameVmInput::PizzaRespondBake)
                    .unwrap(),
                5 => ticks(&mut host, 1),
                _ => unreachable!(),
            }
            clear(&mut host, &tickets);
            let (mut store, stamp) = saved(&mut host);
            restore(&mut store, stamp)
                .unwrap_or_else(|error| panic!("delay {delay}, stage {stage}: {error:?}"));
        }
    }
}

#[test]
fn pizza_checkpoint_requires_the_pool_to_contain_each_returned_contribution() {
    let mut host = host();
    let control = controller(
        &mut host,
        GameControllerInput::PizzaMaker {
            seed: PrivateSeed::new(20),
        },
    );
    let tickets: Vec<_> = (1..=4)
        .map(|p| {
            join(
                &mut host,
                control,
                p,
                GamePlayerInput::PizzaMaker {
                    station: p as u8 - 1,
                    tuning: PizzaTuning::default(),
                },
            )
        })
        .collect();
    ticks(&mut host, 1);
    host.deliver_game_event(control, InvokerId(900), GameVmInput::PizzaRespondPhone)
        .unwrap();
    host.deliver_game_event(control, InvokerId(900), GameVmInput::PizzaAllContributed)
        .unwrap();
    clear(&mut host, &tickets);
    let (mut store, stamp) = saved(&mut host);
    let (_, groups) = checkpoint_offsets(&store.0);
    // Every contribution has already returned to the pool. This bonus card has
    // only two copies, so four such contributions cannot exist even though all
    // individual cards and the burned recipe result are otherwise valid.
    for offset in [24, 29, 34, 39] {
        store.0[groups[0].1 + offset] = 9;
    }
    store.0[groups[0].1 + 5] = 1;
    assert!(matches!(
        restore(&mut store, stamp),
        Err(Error::InvalidCheckpoint)
    ));
}

#[test]
fn maze_waiting_rebind_preserves_each_roles_uninitialized_private_view() {
    for role in [MazeRole::Logic, MazeRole::Charisma] {
        let mut original = host();
        let control = controller(
            &mut original,
            GameControllerInput::Maze {
                seed: PrivateSeed::new(30),
            },
        );
        let player = join(&mut original, control, 1, GamePlayerInput::Maze { role });
        clear(&mut original, &[player]);
        let (mut store, stamp) = saved(&mut original);
        let mut resumed = restore(&mut store, stamp).unwrap();
        let rebound = resumed
            .rebind(
                &Auth,
                ConnectionId(101),
                player.instance_address(),
                TimerRegisters([0; 4]),
            )
            .unwrap();
        let view = private(&mut resumed, 101, rebound);
        let show = if role == MazeRole::Logic {
            "TSOMaze_Show_Logic"
        } else {
            "TSOMaze_Show_Charisma"
        };
        assert_eq!(
            view,
            vec![
                ("eod_enter".into(), vec![]),
                (show.into(), vec![]),
                ("TSOMaze_Show_Waiting".into(), vec![])
            ],
            "uninitialized {role:?} cannot gain map or cell data on rebind"
        );
        resumed
            .rebind_game_controller(control.instance_address(), InvokerId(900))
            .unwrap();
        ticks(&mut resumed, 1);
        let next = private(&mut resumed, 101, rebound);
        if role == MazeRole::Logic {
            assert_eq!(next.len(), 6);
            assert_eq!(next[0].0, "TSOMaze_Mark_Walls");
        } else {
            assert!(
                next.is_empty(),
                "Charisma still waits for Logic after recovery resumes"
            );
        }
        assert!(events(&mut resumed).is_empty());
    }
}

#[test]
fn paperchase_checkpoint_restores_incomplete_lobbies_but_rejects_full_lobby() {
    for players in 0..=3 {
        let mut native = host();
        let control = controller(
            &mut native,
            GameControllerInput::PaperChase {
                seed: PrivateSeed::new(42),
            },
        );
        let tickets: Vec<_> = (1..=players)
            .map(|p| {
                join(
                    &mut native,
                    control,
                    p,
                    GamePlayerInput::PaperChase { slot: p as i16 },
                )
            })
            .collect();
        clear(&mut native, &tickets);
        let (mut store, stamp) = saved(&mut native);
        let (_, groups) = checkpoint_offsets(&store.0);
        let start = groups[0].1;
        if players < 3 {
            assert_eq!(store.0[start + 1], 1);
            restore(&mut store, stamp).unwrap();
        } else {
            assert_eq!(store.0[start + 1], 3);
            // The third admission starts Waiting atomically. Lobby with all
            // roles occupied could never start again because no join can occur.
            store.0[start + 1] = 1;
            store.0[start + 8..start + 11].fill(0);
            assert!(matches!(
                restore(&mut store, stamp),
                Err(Error::InvalidCheckpoint)
            ));
        }
    }
}

#[test]
fn paperchase_checkpoint_rng_exhaustion_rejects_join_atomically() {
    let mut native = host();
    let control = controller(
        &mut native,
        GameControllerInput::PaperChase {
            seed: PrivateSeed::new(42),
        },
    );
    let tickets: Vec<_> = (1..=2)
        .map(|p| {
            join(
                &mut native,
                control,
                p,
                GamePlayerInput::PaperChase { slot: p as i16 },
            )
        })
        .collect();
    clear(&mut native, &tickets);
    let (mut store, stamp) = saved(&mut native);
    let (_, groups) = checkpoint_offsets(&store.0);
    let start = groups[0].1;
    store.0[start + 39..start + 47].copy_from_slice(&(u64::MAX - 1).to_le_bytes());
    let mut resumed = restore(&mut store, stamp).unwrap();
    let control = resumed
        .rebind_game_controller(control.instance_address(), InvokerId(900))
        .unwrap();
    let rebound: Vec<_> = tickets
        .iter()
        .enumerate()
        .map(|(i, ticket)| {
            resumed
                .rebind(
                    &Auth,
                    ConnectionId(i as u64 + 1),
                    ticket.instance_address(),
                    TimerRegisters([0; 4]),
                )
                .unwrap()
        })
        .collect();
    clear(&mut resumed, &rebound);
    let (before, _) = saved(&mut resumed);
    let request = GamePlayerRequest {
        connection: ConnectionId(3),
        game: control.instance_address(),
        invoker: InvokerId(203),
        avatar_object: 103,
        input: GamePlayerInput::PaperChase { slot: 3 },
    };
    for _ in 0..2 {
        assert_eq!(
            resumed.join_game(&Auth, request),
            Err(Error::CounterExhausted)
        );
    }
    assert!(events(&mut resumed).is_empty());
    for (i, ticket) in rebound.iter().enumerate() {
        assert!(private(&mut resumed, i as u64 + 1, *ticket).is_empty());
    }
    let (mut after, after_stamp) = saved(&mut resumed);
    // A successful checkpoint advances only its revision at header bytes24..32.
    assert_eq!(&before.0[..24], &after.0[..24]);
    assert_eq!(&before.0[32..], &after.0[32..]);
    NativeHost::restore_from(&mut after, identity(3), after_stamp, HostLimits::default()).unwrap();
}

#[test]
fn maze_checkpoint_cannot_move_the_player_before_solving() {
    for elapsed in [0, 2] {
        let mut native = host();
        let control = controller(
            &mut native,
            GameControllerInput::Maze {
                seed: PrivateSeed::new(42),
            },
        );
        let logic = join(
            &mut native,
            control,
            1,
            GamePlayerInput::Maze {
                role: MazeRole::Logic,
            },
        );
        let charisma = join(
            &mut native,
            control,
            2,
            GamePlayerInput::Maze {
                role: MazeRole::Charisma,
            },
        );
        ticks(&mut native, elapsed);
        clear(&mut native, &[logic, charisma]);
        let (store, stamp) = saved(&mut native);
        let mut valid = store.clone();
        restore(&mut valid, stamp).unwrap();
        let (_, groups) = checkpoint_offsets(&store.0);
        let start = groups[0].1;
        assert_eq!(store.0[start + 1], u8::from(elapsed != 0));
        let origin = u16::from_le_bytes(store.0[start + 11..start + 13].try_into().unwrap());
        let exit = u16::from_le_bytes(store.0[start + 15..start + 17].try_into().unwrap());
        let replacement = (0..288u16)
            .find(|cell| *cell != origin && *cell != exit)
            .unwrap();
        let mut invalid = store;
        invalid.0[start + 11..start + 13].copy_from_slice(&replacement.to_le_bytes());
        assert!(
            matches!(restore(&mut invalid, stamp), Err(Error::InvalidCheckpoint)),
            "movement is impossible before Solving"
        );
    }
}
