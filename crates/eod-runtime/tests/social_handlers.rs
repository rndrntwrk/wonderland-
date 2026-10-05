// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Actual NativeHost acceptance. C# source literals are independently retained
//! in fixtures/eod/social; external effects here are real typed host outputs.
use wonderland_eod_runtime::{
    plugins::{common::MemberInput, social::*},
    *,
};

struct Auth;
impl ConnectionAuthority for Auth {
    fn authenticated_actor(&self, connection: ConnectionId) -> Option<ActorId> {
        let actor = connection.0 % 1000;
        (actor != 0).then_some(ActorId(actor))
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
        scope: HostScopeId(810),
        epoch,
    }
}
fn limits() -> HostLimits {
    HostLimits {
        idle_timeout_ticks: 30 * 3600,
        ..HostLimits::default()
    }
}
fn host() -> NativeHost {
    NativeHost::new(identity(1), limits()).unwrap()
}
#[derive(Clone, Copy)]
struct Controller {
    ticket: NativeControllerTicket,
    invoker: InvokerId,
}
fn controller(host: &mut NativeHost, object: u32, cluster: u64, config: Config) -> Controller {
    let invoker = InvokerId(10000 + object);
    let ticket = host
        .connect_native(NativeCreateRequest {
            object,
            cluster,
            invoker,
            input: NativePluginInput::Social(config),
        })
        .unwrap();
    Controller { ticket, invoker }
}
#[derive(Clone)]
struct Player {
    connection: ConnectionId,
    ticket: SessionTicket,
    plugin: PluginId,
    sequence: u64,
}
fn player(
    host: &mut NativeHost,
    control: Controller,
    plugin: PluginId,
    id: u64,
    role: u8,
) -> Player {
    let input = MemberInput {
        role,
        skills: [100; 6],
        ..MemberInput::default()
    };
    let connection = ConnectionId(id);
    let ticket = host
        .join_native(
            &Auth,
            NativeJoinRequest {
                connection,
                group: control.ticket.instance_address(),
                invoker: InvokerId(20000 + id as u32),
                avatar_object: 100 + id as i16,
                avatar_id: id as u32,
                input,
            },
        )
        .unwrap();
    Player {
        connection,
        ticket,
        plugin,
        sequence: 1,
    }
}
fn names(ids: &[u32]) -> Vec<AvatarName> {
    ids.iter()
        .map(|id| AvatarName {
            avatar_id: *id,
            name: format!("Avatar {}", 100 + id),
        })
        .collect()
}
fn send(
    host: &mut NativeHost,
    player: &mut Player,
    event: &str,
    payload: WirePayload<'_>,
) -> Result<DispatchOutcome, Error> {
    let result = host.receive(
        &Auth,
        player.connection,
        ClientMessage {
            version: protocol::PROTOCOL_VERSION,
            ticket: player.ticket,
            plugin: player.plugin,
            sequence: player.sequence,
            event,
            payload,
        },
    );
    if result.is_ok() {
        player.sequence += 1;
    }
    result
}
fn binary(host: &mut NativeHost, player: &mut Player, event: &str, payload: &[u8]) {
    send(host, player, event, WirePayload::Binary(payload)).unwrap();
}
fn native(host: &mut NativeHost, control: Controller, input: VmInput) -> Result<(), Error> {
    host.deliver_native_event(
        control.ticket,
        control.invoker,
        NativeVmInput::Social(input),
    )
}
fn private(host: &mut NativeHost, player: &Player) -> Vec<(String, Vec<u8>)> {
    host.take_private(&Auth, player.connection, player.ticket)
        .unwrap()
        .into_iter()
        .map(|m| {
            (
                m.event().to_owned(),
                match m.body() {
                    UiBody::Text(text) => text.as_bytes().to_vec(),
                    UiBody::Binary(bytes) => bytes.to_vec(),
                },
            )
        })
        .collect()
}
type Event = (PluginId, InvokerId, i16, Vec<i16>);
fn events(host: &mut NativeHost) -> Vec<Event> {
    host.take_public_events()
        .into_iter()
        .filter_map(|e| match e {
            PublicVmEvent::NativePlugin {
                invoker,
                plugin,
                code,
                args,
            } => Some((plugin, invoker, code, args)),
            _ => None,
        })
        .collect()
}
fn clear(host: &mut NativeHost, players: &[Player]) {
    events(host);
    host.take_native_commands();
    for player in players {
        private(host, player);
    }
}
fn ticks(host: &mut NativeHost, count: usize) {
    for _ in 0..count {
        host.tick(&Auth, &NoRegisters).unwrap();
    }
}
fn read_strings(bytes: &[u8]) -> Vec<String> {
    let mut at = 0;
    let mut values = Vec::new();
    while at < bytes.len() {
        let mut len = 0usize;
        let mut shift = 0;
        loop {
            let byte = bytes[at];
            at += 1;
            len |= usize::from(byte & 127) << shift;
            shift += 7;
            if byte & 128 == 0 {
                break;
            }
        }
        values.push(
            std::str::from_utf8(&bytes[at..at + len])
                .unwrap()
                .to_owned(),
        );
        at += len;
    }
    values
}
fn find(messages: &[(String, Vec<u8>)], name: &str) -> Vec<Vec<u8>> {
    messages
        .iter()
        .filter(|(event, _)| event == name)
        .map(|(_, body)| body.clone())
        .collect()
}

#[test]
fn war_final_different_pieces_continue_and_emit_one_game_result() {
    let mut h = host();
    let c = controller(&mut h, 1, 1, Config::WarGame);
    let mut players = [
        player(&mut h, c, WAR_GAME, 1, 0),
        player(&mut h, c, WAR_GAME, 2, 1),
    ];
    clear(&mut h, &players);
    // Eight legal deciding rounds retain only Blue Artillery and Red Cavalry.
    for (blue, red) in [
        (0, 3),
        (1, 0),
        (2, 0),
        (3, 2),
        (3, 4),
        (2, 4),
        (2, 1),
        (4, 1),
    ] {
        binary(&mut h, &mut players[0], "WarGame_Piece_Selection", &[blue]);
        binary(&mut h, &mut players[1], "WarGame_Piece_Selection", &[red]);
        let outcome = events(&mut h);
        assert_eq!(outcome.len(), 1);
        assert_eq!(outcome[0].2, 1);
        native(&mut h, c, VmInput::WarNextRound).unwrap();
        for p in &players {
            let messages = private(&mut h, p);
            assert_eq!(find(&messages, "WarGame_Resume").len(), 1);
        }
    }
    binary(&mut h, &mut players[0], "WarGame_Piece_Selection", &[0]);
    binary(&mut h, &mut players[1], "WarGame_Piece_Selection", &[1]);
    assert_eq!(events(&mut h), [(WAR_GAME, c.invoker, 1, vec![0])]);
    native(&mut h, c, VmInput::WarNextRound).unwrap();
    assert_eq!(events(&mut h), [(WAR_GAME, c.invoker, 2, vec![0])]);
    assert_eq!(
        native(&mut h, c, VmInput::WarNextRound),
        Err(Error::PluginNotReady)
    );
    assert!(events(&mut h).is_empty());
}

#[test]
fn war_tie_uses_exact_150_accepted_ticks_and_rejects_early_selection() {
    let mut h = host();
    let c = controller(&mut h, 1, 1, Config::WarGame);
    let mut players = [
        player(&mut h, c, WAR_GAME, 1, 0),
        player(&mut h, c, WAR_GAME, 2, 1),
    ];
    clear(&mut h, &players);
    binary(&mut h, &mut players[0], "WarGame_Piece_Selection", &[2]);
    binary(&mut h, &mut players[1], "WarGame_Piece_Selection", &[2]);
    for p in &players {
        assert_eq!(find(&private(&mut h, p), "WarGame_Tie"), [vec![2, 2]]);
    }
    assert_eq!(
        send(
            &mut h,
            &mut players[0],
            "WarGame_Piece_Selection",
            WirePayload::Binary(&[0])
        ),
        Err(Error::PluginNotReady)
    );
    ticks(&mut h, 149);
    for p in &players {
        assert!(private(&mut h, p).is_empty());
    }
    assert!(events(&mut h).is_empty());
    ticks(&mut h, 1);
    for p in &players {
        assert_eq!(find(&private(&mut h, p), "WarGame_Resume"), [vec![5, 5]]);
    }
}

#[test]
fn war_invalid_piece_forces_first_remaining_and_output_rejection_keeps_sequence() {
    let mut custom = limits();
    custom.max_private_messages = 12;
    let mut h = NativeHost::new(identity(1), custom).unwrap();
    let c = controller(&mut h, 1, 1, Config::WarGame);
    let mut players = [
        player(&mut h, c, WAR_GAME, 1, 0),
        player(&mut h, c, WAR_GAME, 2, 1),
    ];
    clear(&mut h, &players);
    for _ in 0..3 {
        native(&mut h, c, VmInput::WarNextGame).unwrap();
    }
    binary(&mut h, &mut players[0], "WarGame_Piece_Selection", &[99]);
    assert_eq!(
        send(
            &mut h,
            &mut players[1],
            "WarGame_Piece_Selection",
            WirePayload::Binary(&[1])
        ),
        Err(Error::QueueFull)
    );
    assert!(events(&mut h).is_empty());
    for p in &players {
        private(&mut h, p);
    }
    binary(&mut h, &mut players[1], "WarGame_Piece_Selection", &[1]);
    assert_eq!(events(&mut h), [(WAR_GAME, c.invoker, 1, vec![0])]);
    assert_eq!(
        find(&private(&mut h, &players[0]), "WarGame_Victory"),
        [vec![0, 1]]
    );
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
        let n = destination.len().min(self.0.len());
        destination[..n].copy_from_slice(&self.0[..n]);
        Ok(PrivateRead {
            bytes_written: n,
            complete: n == self.0.len(),
        })
    }
}
fn saved(h: &mut NativeHost) -> (Store, CheckpointStamp) {
    let mut store = Store::default();
    let stamp = h.checkpoint_to(&mut store).unwrap();
    (store, stamp)
}
fn restore(store: &mut Store, stamp: CheckpointStamp) -> NativeHost {
    NativeHost::restore_from(store, identity(2), stamp, limits()).unwrap()
}
fn rebind(h: &mut NativeHost, p: &Player) -> Player {
    let connection = ConnectionId(p.connection.0 + 1000);
    let ticket = h
        .rebind(
            &Auth,
            connection,
            p.ticket.instance_address(),
            TimerRegisters([0; 4]),
        )
        .unwrap();
    Player {
        connection,
        ticket,
        plugin: p.plugin,
        sequence: 1,
    }
}

#[test]
fn war_private_recovery_pauses_timer_until_controller_and_both_roles_rebind() {
    let mut h = host();
    let c = controller(&mut h, 1, 1, Config::WarGame);
    let mut players = [
        player(&mut h, c, WAR_GAME, 1, 0),
        player(&mut h, c, WAR_GAME, 2, 1),
    ];
    clear(&mut h, &players);
    for p in &mut players {
        binary(&mut h, p, "WarGame_Piece_Selection", &[1]);
    }
    ticks(&mut h, 73);
    clear(&mut h, &players);
    let (mut store, stamp) = saved(&mut h);
    assert_eq!(&store.0[..8], b"EODP\x04\x00\x01\x00");
    let mut h = restore(&mut store, stamp);
    ticks(&mut h, 200);
    assert!(events(&mut h).is_empty());
    assert_eq!(
        native(&mut h, c, VmInput::WarNextGame),
        Err(Error::WrongEpoch)
    );
    h.rebind_native_controller(c.ticket.instance_address(), c.invoker)
        .unwrap();
    let first = rebind(&mut h, &players[0]);
    private(&mut h, &first);
    ticks(&mut h, 200);
    assert!(private(&mut h, &first).is_empty());
    let second = rebind(&mut h, &players[1]);
    private(&mut h, &second);
    ticks(&mut h, 76);
    assert!(private(&mut h, &first).is_empty());
    ticks(&mut h, 1);
    assert_eq!(
        find(&private(&mut h, &first), "WarGame_Resume"),
        [vec![5, 5]]
    );
}

fn band_game(h: &mut NativeHost) -> (Controller, Vec<Player>) {
    let c = controller(
        h,
        20,
        20,
        Config::Band {
            seed: Seed::new(42),
            names: names(&[1, 2, 3, 4]),
        },
    );
    let players: Vec<_> = (1..=4)
        .map(|id| player(h, c, BAND, id, id as u8 - 1))
        .collect();
    clear(h, &players);
    (c, players)
}
fn first_sequence(h: &mut NativeHost, players: &[Player]) -> Vec<u8> {
    ticks(h, 301);
    let sequence = find(&private(h, &players[0]), "Band_Sequence")
        .pop()
        .unwrap();
    for p in &players[1..] {
        private(h, p);
    }
    assert_eq!(sequence.len(), 1);
    sequence
}
fn play_sequence(h: &mut NativeHost, players: &mut [Player], sequence: &[u8]) {
    // Rehearsal creation consumes its first native timer tick.
    ticks(h, 45 * (sequence.len() + 2));
    for p in &*players {
        let messages = private(h, p);
        assert_eq!(find(&messages, "Band_Performance").len(), 1);
    }
    for note in sequence {
        assert!((1..=8).contains(note));
        let role = usize::from((note - 1) / 2);
        binary(h, &mut players[role], "Band_Note", &[*note]);
    }
}
fn intermission(h: &mut NativeHost, c: Controller, players: &[Player]) {
    ticks(h, 1);
    for p in players {
        let messages = private(h, p);
        assert_eq!(find(&messages, "Band_Electric").len(), 1);
    }
    native(h, c, VmInput::BandAnimationsFinished).unwrap();
    ticks(h, 1);
    for p in players {
        assert_eq!(find(&private(h, p), "Band_Intermission").len(), 1);
    }
}
fn rock_on(h: &mut NativeHost, players: &mut [Player]) -> Vec<u8> {
    for p in &mut players[..2] {
        binary(h, p, "Band_Decision", &[1]);
    }
    ticks(h, 2);
    let sequence = find(&private(h, &players[0]), "Band_Sequence")
        .pop()
        .unwrap();
    for p in &players[1..] {
        private(h, p);
    }
    sequence
}

#[test]
fn band_instrument_ownership_and_majority_sellout_use_source_payout_events() {
    let mut h = host();
    let (c, mut players) = band_game(&mut h);
    let sequence = first_sequence(&mut h, &players);
    ticks(&mut h, 135);
    for p in &players {
        private(&mut h, p);
    }
    let role = usize::from((sequence[0] - 1) / 2);
    let wrong_role = (role + 1) % 4;
    binary(&mut h, &mut players[wrong_role], "Band_Note", &sequence);
    assert!(events(&mut h).is_empty());
    binary(&mut h, &mut players[role], "Band_Note", &sequence);
    assert_eq!(events(&mut h), [(BAND, c.invoker, 7, vec![role as i16])]);
    intermission(&mut h, c, &players);
    for p in &mut players[..3] {
        binary(&mut h, p, "Band_Decision", &[0]);
    }
    ticks(&mut h, 1);
    assert_eq!(
        events(&mut h),
        [
            (BAND, c.invoker, 5, vec![1]),
            (BAND, c.invoker, 6, vec![3]),
            (BAND, c.invoker, 3, vec![40])
        ]
    );
    let messages = private(&mut h, &players[0]);
    assert_eq!(read_strings(&find(&messages, "Band_Win")[0]), ["40", "1"]);
}

#[test]
fn band_all_twenty_five_sequences_complete_with_source_final_payout() {
    let mut h = host();
    let (c, mut players) = band_game(&mut h);
    let mut sequence = first_sequence(&mut h, &players);
    for length in 1..=25 {
        assert_eq!(sequence.len(), length);
        play_sequence(&mut h, &mut players, &sequence);
        if length == 25 {
            assert_eq!(
                events(&mut h),
                [
                    (BAND, c.invoker, 5, vec![25]),
                    (BAND, c.invoker, 6, vec![72]),
                    (BAND, c.invoker, 3, vec![7344])
                ]
            );
            assert_eq!(
                read_strings(&find(&private(&mut h, &players[0]), "Band_Win")[0]),
                ["7344", "25"]
            );
        } else {
            assert_eq!(events(&mut h).len(), 1);
            intermission(&mut h, c, &players);
            sequence = rock_on(&mut h, &mut players);
        }
    }
}

#[test]
fn band_failure_after_five_notes_pays_minimum_only_once_after_native_callback() {
    let mut h = host();
    let (c, mut players) = band_game(&mut h);
    let mut sequence = first_sequence(&mut h, &players);
    for _ in 0..5 {
        play_sequence(&mut h, &mut players, &sequence);
        events(&mut h);
        intermission(&mut h, c, &players);
        sequence = rock_on(&mut h, &mut players);
    }
    assert_eq!(sequence.len(), 6);
    ticks(&mut h, 45 * 8);
    clear(&mut h, &players);
    let wrong = if sequence[0] == 1 { 2 } else { 1 };
    binary(&mut h, &mut players[0], "Band_Note", &[wrong]);
    assert_eq!(events(&mut h), [(BAND, c.invoker, 2, Vec::new())]);
    ticks(&mut h, 1);
    native(&mut h, c, VmInput::BandAnimationsFinished).unwrap();
    assert_eq!(
        events(&mut h),
        [
            (BAND, c.invoker, 5, vec![5]),
            (BAND, c.invoker, 6, vec![14]),
            (BAND, c.invoker, 3, vec![624])
        ]
    );
    assert_eq!(
        native(&mut h, c, VmInput::BandAnimationsFinished),
        Err(Error::PluginNotReady)
    );
    assert!(events(&mut h).is_empty());
}

#[test]
fn band_note_timeout_and_partial_lobby_do_not_start_unowned_games() {
    let mut h = host();
    let c = controller(
        &mut h,
        20,
        20,
        Config::Band {
            seed: Seed::new(42),
            names: names(&[1, 2, 3, 4]),
        },
    );
    let mut players: Vec<_> = (1..=3)
        .map(|id| player(&mut h, c, BAND, id, id as u8 - 1))
        .collect();
    clear(&mut h, &players);
    ticks(&mut h, 310);
    assert!(events(&mut h).is_empty());
    assert!(private(&mut h, &players[0]).is_empty());
    players.push(player(&mut h, c, BAND, 4, 3));
    clear(&mut h, &players);
    first_sequence(&mut h, &players);
    ticks(&mut h, 135);
    clear(&mut h, &players);
    ticks(&mut h, 298);
    assert!(events(&mut h).is_empty());
    ticks(&mut h, 1);
    assert_eq!(events(&mut h), [(BAND, c.invoker, 2, Vec::new())]);
    assert_eq!(
        find(&private(&mut h, &players[0]), "Band_Timeout"),
        [Vec::<u8>::new()]
    );
}

#[test]
fn band_rebind_shows_saved_game_before_phase_controls_without_advancing_it() {
    for (phase, phase_event) in [
        ("lobby", None),
        ("preshow", None),
        ("rehearsal", Some("Band_Sequence")),
        ("performance", Some("Band_Performance")),
        ("electric", Some("Band_Electric")),
        ("intermission", Some("Band_Intermission")),
        ("minimum_payment", None),
        ("finale", None),
    ] {
        let mut h = host();
        let (c, mut players) = band_game(&mut h);
        if phase == "preshow" {
            ticks(&mut h, 1);
        } else if phase != "lobby" {
            let mut sequence = first_sequence(&mut h, &players);
            if matches!(phase, "minimum_payment" | "finale") {
                for _ in 0..5 {
                    play_sequence(&mut h, &mut players, &sequence);
                    events(&mut h);
                    intermission(&mut h, c, &players);
                    sequence = rock_on(&mut h, &mut players);
                }
                ticks(&mut h, 45 * (sequence.len() + 2));
                clear(&mut h, &players);
                let wrong = if sequence[0] == 1 { 2 } else { 1 };
                binary(&mut h, &mut players[0], "Band_Note", &[wrong]);
                ticks(&mut h, 1);
                if phase == "finale" {
                    native(&mut h, c, VmInput::BandAnimationsFinished).unwrap();
                    ticks(&mut h, 1);
                }
            } else if phase != "rehearsal" {
                ticks(&mut h, 45 * (sequence.len() + 2));
                if phase != "performance" {
                    for note in &sequence {
                        binary(
                            &mut h,
                            &mut players[usize::from((note - 1) / 2)],
                            "Band_Note",
                            &[*note],
                        );
                    }
                    ticks(&mut h, 1);
                    if phase == "intermission" {
                        native(&mut h, c, VmInput::BandAnimationsFinished).unwrap();
                        ticks(&mut h, 1);
                    }
                }
            }
        }
        clear(&mut h, &players);
        let (mut store, stamp) = saved(&mut h);
        let mut recovered = restore(&mut store, stamp);
        recovered
            .rebind_native_controller(c.ticket.instance_address(), c.invoker)
            .unwrap();
        let restored: Vec<_> = players.iter().map(|p| rebind(&mut recovered, p)).collect();
        for p in &restored {
            let replay = private(&mut recovered, p);
            let position = |event| replay.iter().position(|(name, _)| name == event);
            if phase == "lobby" {
                assert!(position("Band_Show").is_none());
            } else {
                let init = position("Band_UI_Init").unwrap();
                let skill = position("Band_Game_Reset_Skill")
                    .expect("restore the saved skill before showing Band");
                let show = position("Band_Show").expect("UI_Init hides the game until Band_Show");
                assert!(init < skill && skill < show, "{phase}");
                assert_eq!(read_strings(&replay[skill].1), ["4"]);
                if let Some(event) = phase_event {
                    assert!(show < position(event).unwrap(), "{phase}");
                }
            }
        }
        assert!(
            events(&mut recovered).is_empty(),
            "rebind must not request another payout or round"
        );
        assert!(recovered.take_native_commands().is_empty());
        ticks(&mut h, 1);
        ticks(&mut recovered, 1);
        assert_eq!(events(&mut recovered), events(&mut h), "{phase}");
        for (before, after) in players.iter().zip(&restored) {
            assert_eq!(
                private(&mut recovered, after),
                private(&mut h, before),
                "{phase}"
            );
        }
    }
}

fn buzzer(h: &mut NativeHost, cluster: u64) -> (Controller, Player, Vec<Controller>, Vec<Player>) {
    let host_control = controller(h, 30, cluster, Config::BuzzerHost);
    let mut controls = Vec::new();
    let mut players = Vec::new();
    for id in 1..=4 {
        let c = controller(
            h,
            30 + id as u32,
            cluster,
            Config::BuzzerPlayer {
                names: names(&[id as u32]),
            },
        );
        controls.push(c);
        players.push(player(h, c, BUZZER_PLAYER, id, 0));
    }
    let host_player = player(h, host_control, BUZZER_HOST, 5, 0);
    clear(h, &players);
    private(h, &host_player);
    (host_control, host_player, controls, players)
}
fn enable(h: &mut NativeHost, host_player: &mut Player, index: usize) {
    binary(
        h,
        host_player,
        "Buzzer_Host_ToggleEnablePlayer",
        &(index as i32).to_le_bytes(),
    );
}

#[test]
fn buzzer_couples_real_registered_players_and_preserves_first_three_autoenable() {
    let mut h = host();
    let (c, mut hp, controls, players) = buzzer(&mut h, 30);
    binary(
        &mut h,
        &mut hp,
        "Buzzer_Host_PlayerCorrect",
        &0i32.to_le_bytes(),
    );
    let roster = find(&private(&mut h, &hp), "Buzzer_Host_Roster")
        .pop()
        .unwrap();
    assert_eq!(
        read_strings(&roster),
        [
            "101", "100", "1", "102", "0", "1", "103", "0", "1", "104", "0", "0"
        ]
    );
    let effects = events(&mut h);
    assert!(effects.contains(&(BUZZER_HOST, c.invoker, 2, vec![101])));
    assert!(effects.contains(&(BUZZER_PLAYER, controls[0].invoker, 2, vec![1])));
    assert!(
        !effects
            .iter()
            .any(|(plugin, _, code, _)| *plugin == BUZZER_PLAYER && *code == 5)
    );
    assert_eq!(
        find(&private(&mut h, &players[0]), "Buzzer_Player_Score"),
        [100i16.to_le_bytes().to_vec()]
    );
    native(&mut h, controls[0], VmInput::BuzzerPlayerSync).unwrap();
    let effects = events(&mut h);
    for (i, control) in controls.iter().enumerate() {
        assert!(effects.contains(&(
            BUZZER_PLAYER,
            control.invoker,
            5,
            vec![if i == 0 { 100 } else { 0 }]
        )));
    }
    assert!(effects.contains(&(BUZZER_PLAYER, controls[1].invoker, 4, vec![1])));
    assert!(!effects.contains(&(BUZZER_PLAYER, controls[3].invoker, 4, vec![1])));
}

#[test]
fn buzzer_first_late_locked_and_timeout_match_source_tick_window() {
    let mut h = host();
    let (_, mut hp, controls, mut players) = buzzer(&mut h, 30);
    for i in 0..4 {
        enable(&mut h, &mut hp, i);
    }
    binary(&mut h, &mut hp, "Buzzer_Host_A_ToggleMaster", &[1]);
    clear(&mut h, &players);
    private(&mut h, &hp);
    binary(&mut h, &mut players[0], "Buzzer_Player_Buzzed", &[]);
    binary(&mut h, &mut players[1], "Buzzer_Player_Buzzed", &[]);
    assert_eq!(
        find(&private(&mut h, &players[0]), "BuzzerEOD_Buzzed"),
        [1i16.to_le_bytes().to_vec()]
    );
    assert_eq!(
        find(&private(&mut h, &players[1]), "BuzzerEOD_Buzzed"),
        [0i16.to_le_bytes().to_vec()]
    );
    events(&mut h);
    ticks(&mut h, 29);
    assert!(!events(&mut h).iter().any(|(_, _, code, _)| *code == 3));
    ticks(&mut h, 1);
    let effects = events(&mut h);
    for control in &controls[1..] {
        assert!(effects.contains(&(BUZZER_PLAYER, control.invoker, 3, vec![])));
    }
    for p in &players {
        private(&mut h, p);
    }
    private(&mut h, &hp);
    binary(&mut h, &mut players[2], "Buzzer_Player_Buzzed", &[]);
    assert!(events(&mut h).is_empty());
    ticks(&mut h, 19 * 30);
    assert_eq!(
        find(&private(&mut h, &hp), "Buzzer_Host_Tip"),
        [b"38".to_vec()]
    );
}

#[test]
fn buzzer_scores_and_winner_wait_for_their_native_callbacks() {
    let mut h = host();
    let (hc, mut hp, controls, players) = buzzer(&mut h, 30);
    binary(
        &mut h,
        &mut hp,
        "Buzzer_Host_A_PlayerScore2",
        &9990i16.to_le_bytes(),
    );
    assert!(events(&mut h).contains(&(BUZZER_PLAYER, controls[2].invoker, 5, vec![9990])));
    binary(
        &mut h,
        &mut hp,
        "Buzzer_Host_PlayerCorrect",
        &2i32.to_le_bytes(),
    );
    assert_eq!(
        find(&private(&mut h, &players[2]), "Buzzer_Player_Score").last(),
        Some(&9999i16.to_le_bytes().to_vec())
    );
    events(&mut h);
    binary(
        &mut h,
        &mut hp,
        "Buzzer_Host_A_DeclareWinner",
        &2i32.to_le_bytes(),
    );
    let effects = events(&mut h);
    assert!(effects.contains(&(BUZZER_HOST, hc.invoker, 4, vec![103])));
    assert!(
        !effects
            .iter()
            .any(|(plugin, _, code, _)| *plugin == BUZZER_PLAYER && *code == 6)
    );
    native(&mut h, hc, VmInput::BuzzerHostDeclareWinner).unwrap();
    assert_eq!(
        events(&mut h),
        [(BUZZER_PLAYER, controls[2].invoker, 6, vec![2])]
    );
    native(&mut h, hc, VmInput::BuzzerHostDeclareWinner).unwrap();
    assert!(events(&mut h).is_empty());
    native(&mut h, controls[2], VmInput::BuzzerPlayerSync).unwrap();
    let effects = events(&mut h);
    for (i, c) in controls.iter().enumerate() {
        if i != 2 {
            assert!(effects.contains(&(BUZZER_PLAYER, c.invoker, 6, vec![1])));
        }
    }
}

#[test]
fn buzzer_malformed_inputs_do_not_consume_sequence_or_cross_clusters() {
    let mut h = host();
    let (_, mut hp, _, players) = buzzer(&mut h, 30);
    let other_c = controller(&mut h, 50, 50, Config::BuzzerPlayer { names: names(&[6]) });
    let other = player(&mut h, other_c, BUZZER_PLAYER, 6, 0);
    clear(&mut h, &players);
    private(&mut h, &hp);
    private(&mut h, &other);
    assert_eq!(
        send(
            &mut h,
            &mut hp,
            "Buzzer_Host_PlayerCorrect",
            WirePayload::Binary(&[0])
        ),
        Err(Error::InvalidMessage)
    );
    assert_eq!(
        send(
            &mut h,
            &mut hp,
            "Buzzer_Host_PlayerCorrect",
            WirePayload::Binary(&4i32.to_le_bytes())
        ),
        Err(Error::InvalidMessage)
    );
    binary(
        &mut h,
        &mut hp,
        "Buzzer_Host_PlayerCorrect",
        &0i32.to_le_bytes(),
    );
    assert!(private(&mut h, &other).is_empty());
    enable(&mut h, &mut hp, 0);
    binary(&mut h, &mut hp, "Buzzer_Host_A_ToggleMaster", &[1]);
    private(&mut h, &hp);
    binary(
        &mut h,
        &mut hp,
        "Buzzer_Host_A_GlobalScore",
        &200i16.to_le_bytes(),
    );
    let messages = private(&mut h, &hp);
    assert_eq!(find(&messages, "Buzzer_Host_Error"), [vec![0; 32]]);
    assert_eq!(
        find(&messages, "Buzzer_Host_B_GlobalScore"),
        [100i16.to_le_bytes().to_vec()]
    );
}

#[test]
fn buzzer_partial_cluster_restore_cannot_drop_a_peer_score_operation() {
    let mut h = host();
    let (hc, hp, controls, players) = buzzer(&mut h, 30);
    clear(&mut h, &players);
    private(&mut h, &hp);
    let (mut store, stamp) = saved(&mut h);
    let mut h = restore(&mut store, stamp);
    let hc = Controller {
        ticket: h
            .rebind_native_controller(hc.ticket.instance_address(), hc.invoker)
            .unwrap(),
        ..hc
    };
    let mut hp = rebind(&mut h, &hp);
    private(&mut h, &hp);
    events(&mut h);
    assert_eq!(
        send(
            &mut h,
            &mut hp,
            "Buzzer_Host_A_PlayerScore0",
            WirePayload::Binary(&555i16.to_le_bytes())
        ),
        Err(Error::PluginNotReady)
    );
    assert!(events(&mut h).is_empty());
    let mut rebound = Vec::new();
    for (control, p) in controls.iter().zip(&players) {
        h.rebind_native_controller(control.ticket.instance_address(), control.invoker)
            .unwrap();
        rebound.push(rebind(&mut h, p));
    }
    clear(&mut h, &rebound);
    private(&mut h, &hp);
    binary(
        &mut h,
        &mut hp,
        "Buzzer_Host_A_PlayerScore0",
        &555i16.to_le_bytes(),
    );
    assert_eq!(
        find(&private(&mut h, &rebound[0]), "Buzzer_Player_Score"),
        [555i16.to_le_bytes().to_vec()]
    );
    assert!(events(&mut h).contains(&(BUZZER_PLAYER, controls[0].invoker, 5, vec![555])));
    native(&mut h, hc, VmInput::BuzzerHostJudgmentFinished).unwrap();
}

#[test]
fn buzzer_controller_teardown_releases_claims_for_replacement_host() {
    let mut h = host();
    let (hc, mut hp, _, players) = buzzer(&mut h, 30);
    enable(&mut h, &mut hp, 0);
    binary(&mut h, &mut hp, "Buzzer_Host_A_ToggleMaster", &[1]);
    clear(&mut h, &players);
    private(&mut h, &hp);
    h.disconnect_invoker(hc.invoker).unwrap();
    assert_eq!(
        find(&private(&mut h, &players[0]), "BuzzerEOD_Master"),
        [vec![0]]
    );
    let replacement = controller(&mut h, 39, 30, Config::BuzzerHost);
    let new_host = player(&mut h, replacement, BUZZER_HOST, 6, 0);
    let roster = find(&private(&mut h, &new_host), "Buzzer_Host_Roster")
        .pop()
        .unwrap();
    let fields = read_strings(&roster);
    assert_eq!(
        [&fields[0], &fields[3], &fields[6], &fields[9]],
        ["101", "102", "103", "104"]
    );
}

#[test]
fn buzzer_answerer_disconnect_disables_round_and_retains_other_players() {
    let mut h = host();
    let (hc, mut hp, controls, mut players) = buzzer(&mut h, 30);
    for slot in 0..4 {
        enable(&mut h, &mut hp, slot);
    }
    binary(&mut h, &mut hp, "Buzzer_Host_A_ToggleMaster", &[1]);
    binary(&mut h, &mut players[0], "Buzzer_Player_Buzzed", &[]);
    clear(&mut h, &players);
    private(&mut h, &hp);
    h.disconnect(&Auth, players[0].connection, players[0].ticket)
        .unwrap();
    let output = events(&mut h);
    assert!(output.contains(&(BUZZER_HOST, hc.invoker, 3, vec![0])));
    assert_eq!(
        find(&private(&mut h, &players[1]), "BuzzerEOD_Master"),
        [vec![0]]
    );
    for control in &controls[1..] {
        assert!(output.contains(&(BUZZER_PLAYER, control.invoker, 4, vec![0])));
    }
}

#[test]
fn buzzer_private_recovery_preserves_options_master_and_deferred_reactions() {
    let mut h = host();
    let (hc, mut hp, controls, players) = buzzer(&mut h, 30);
    binary(
        &mut h,
        &mut hp,
        "Buzzer_Host_A_AnswerTime",
        &37i16.to_le_bytes(),
    );
    binary(&mut h, &mut hp, "Buzzer_Host_A_Deduct", &[1]);
    binary(
        &mut h,
        &mut hp,
        "Buzzer_Host_PlayerCorrect",
        &0i32.to_le_bytes(),
    );
    binary(&mut h, &mut hp, "Buzzer_Host_A_ToggleMaster", &[1]);
    clear(&mut h, &players);
    private(&mut h, &hp);
    let (mut store, stamp) = saved(&mut h);
    let mut recovered = restore(&mut store, stamp);
    recovered
        .rebind_native_controller(hc.ticket.instance_address(), hc.invoker)
        .unwrap();
    let recovered_host = rebind(&mut recovered, &hp);
    let host_ui = private(&mut recovered, &recovered_host);
    assert_eq!(
        find(&host_ui, "Buzzer_Host_B_AnswerTime"),
        [37i16.to_le_bytes().to_vec()]
    );
    assert_eq!(find(&host_ui, "Buzzer_Host_B_Deduct"), [vec![1]]);
    assert_eq!(find(&host_ui, "Buzzer_Host_B_ToggleMaster"), [vec![1]]);
    let mut restored_controls = Vec::new();
    let mut restored_players = Vec::new();
    for (control, p) in controls.iter().zip(&players) {
        restored_controls.push(Controller {
            ticket: recovered
                .rebind_native_controller(control.ticket.instance_address(), control.invoker)
                .unwrap(),
            ..*control
        });
        restored_players.push(rebind(&mut recovered, p));
    }
    assert_eq!(
        find(
            &private(&mut recovered, &restored_players[0]),
            "BuzzerEOD_Master"
        ),
        [vec![1]]
    );
    clear(&mut recovered, &restored_players);
    private(&mut recovered, &recovered_host);
    native(
        &mut recovered,
        restored_controls[0],
        VmInput::BuzzerPlayerSync,
    )
    .unwrap();
    let output = events(&mut recovered);
    assert!(output.contains(&(BUZZER_PLAYER, controls[0].invoker, 5, vec![100])));
    for control in &controls[1..3] {
        assert!(output.contains(&(BUZZER_PLAYER, control.invoker, 4, vec![1])));
    }
    native(
        &mut recovered,
        restored_controls[0],
        VmInput::BuzzerPlayerSync,
    )
    .unwrap();
    assert!(
        !events(&mut recovered)
            .iter()
            .any(|(_, _, code, _)| *code == 4)
    );
}

#[test]
fn buzzer_search_rotation_and_numeric_option_limits_preserve_source_encodings() {
    let mut h = host();
    let (_, mut hp, _, players) = buzzer(&mut h, 30);
    let extra = controller(&mut h, 35, 30, Config::BuzzerPlayer { names: names(&[6]) });
    let extra_player = player(&mut h, extra, BUZZER_PLAYER, 6, 0);
    private(&mut h, &extra_player);
    private(&mut h, &hp);
    binary(
        &mut h,
        &mut hp,
        "Buzzer_Host_FindNewPlayer",
        &0i32.to_le_bytes(),
    );
    let roster = find(&private(&mut h, &hp), "Buzzer_Host_Roster")
        .pop()
        .unwrap();
    assert_eq!(read_strings(&roster)[0], "106");
    binary(
        &mut h,
        &mut hp,
        "Buzzer_Host_MovePlayerRight",
        &0i32.to_le_bytes(),
    );
    let roster = find(&private(&mut h, &hp), "Buzzer_Host_Roster")
        .pop()
        .unwrap();
    assert_eq!(read_strings(&roster)[3], "106");
    for (event, value, response, limit) in [
        (
            "Buzzer_Host_A_BuzzerTime",
            1,
            "Buzzer_Host_B_BuzzerTime",
            "Buzzer_Host_B_UnderBuzzerTime",
        ),
        (
            "Buzzer_Host_A_AnswerTime",
            121,
            "Buzzer_Host_B_AnswerTime",
            "Buzzer_Host_B_OverAnswerTime",
        ),
        (
            "Buzzer_Host_A_GlobalScore",
            -1,
            "Buzzer_Host_B_GlobalScore",
            "Buzzer_Host_B_UnderGlobalScore",
        ),
    ] {
        binary(&mut h, &mut hp, event, &(value as i16).to_le_bytes());
        let ui = private(&mut h, &hp);
        assert_eq!(find(&ui, limit), [Vec::<u8>::new()]);
        assert_eq!(find(&ui, response).len(), 1);
    }
    clear(&mut h, &players);
}

fn floor_tiles() -> Vec<FloorTile> {
    (0..9)
        .flat_map(|y| {
            (0..9).map(move |x| FloorTile {
                object: 1000 + x + y * 9,
                x,
                y,
            })
        })
        .collect()
}
fn graphics(h: &mut NativeHost) -> Vec<Vec<u8>> {
    h.take_native_commands()
        .into_iter()
        .filter_map(|command| match command.command {
            NativeCommand::BatchGraphics { graphics, .. } => Some(graphics),
            _ => None,
        })
        .collect()
}

#[test]
fn nightclub_floor_emits_actual_graphic_commands_with_source_blink_and_heart() {
    let mut h = host();
    let floor = controller(
        &mut h,
        60,
        60,
        Config::NCDanceFloor {
            seed: Seed::new(1),
            tiles: floor_tiles(),
        },
    );
    events(&mut h);
    native(
        &mut h,
        floor,
        VmInput::FloorAnimation {
            animation: 1,
            color: 3,
            direction: 0,
        },
    )
    .unwrap();
    ticks(&mut h, 1);
    assert!(graphics(&mut h).is_empty());
    ticks(&mut h, 3);
    assert_eq!(graphics(&mut h), [vec![3; 81]]);
    ticks(&mut h, 30);
    let frames = graphics(&mut h);
    assert_eq!(frames.len(), 10);
    assert_eq!(frames.last(), Some(&vec![4; 81]));
    native(
        &mut h,
        floor,
        VmInput::FloorAnimation {
            animation: 5,
            color: 3,
            direction: 0,
        },
    )
    .unwrap();
    ticks(&mut h, 3);
    let output = graphics(&mut h).pop().unwrap();
    let expected: Vec<u8> = [
        ".........",
        ".###.###.",
        "#########",
        "#########",
        "#########",
        ".#######.",
        "..#####..",
        "...###...",
        "....#....",
    ]
    .iter()
    .flat_map(|row| row.bytes().map(|v| if v == b'#' { 4 } else { 10 }))
    .collect();
    assert_eq!(output, expected);
    assert_eq!(
        native(
            &mut h,
            floor,
            VmInput::FloorRatings {
                ratings: [-1, 0, 0, 0]
            }
        ),
        Err(Error::InvalidPluginInput)
    );
}

#[test]
fn nightclub_late_floor_discovery_binds_original_controller_and_rejects_alias() {
    let mut h = host();
    let club = controller(
        &mut h,
        90,
        90,
        Config::Nightclub {
            seed: Seed::new(1),
            portals: Vec::new(),
        },
    );
    controller(
        &mut h,
        91,
        90,
        Config::NCDanceFloor {
            seed: Seed::new(2),
            tiles: floor_tiles(),
        },
    );
    assert_eq!(
        h.connect_native(NativeCreateRequest {
            object: 92,
            cluster: 90,
            invoker: InvokerId(10092),
            input: NativePluginInput::Social(Config::Nightclub {
                seed: Seed::new(3),
                portals: Vec::new()
            }),
        }),
        Err(Error::InvalidPluginInput)
    );
    native(
        &mut h,
        club,
        VmInput::NightclubRoundStart { dancers: vec![700] },
    )
    .unwrap();
    ticks(&mut h, 1);
    assert!(h.take_native_commands().iter().any(|output| matches!(
        output.command,
        NativeCommand::ForceInteraction { caller: 700, .. }
    )));
}

#[test]
fn nightclub_floor_geometry_and_queue_sources_are_bounded_before_admission() {
    let mut h = host();
    let original = FloorTile {
        object: 100,
        x: 0,
        y: 0,
    };
    for tiles in [
        vec![original, original],
        vec![
            original,
            FloorTile {
                object: 101,
                ..original
            },
        ],
        vec![
            original,
            FloorTile {
                object: 101,
                x: 64,
                y: 0,
            },
        ],
        vec![FloorTile {
            object: 0,
            ..original
        }],
        vec![original; 4097],
    ] {
        assert_eq!(
            h.connect_native(NativeCreateRequest {
                object: 95,
                cluster: 95,
                invoker: InvokerId(10095),
                input: NativePluginInput::Social(Config::NCDanceFloor {
                    seed: Seed::new(1),
                    tiles
                }),
            }),
            Err(Error::InvalidPluginInput)
        );
    }
    let c = controller(
        &mut h,
        95,
        95,
        Config::NCDanceFloor {
            seed: Seed::new(1),
            tiles: vec![original],
        },
    );
    assert_eq!(
        native(
            &mut h,
            c,
            VmInput::DanceQueueObservation {
                avatar_id: 1,
                active_uid: Some(1),
                interaction: 6,
                on_this_platform: true,
                queue_active: true,
            }
        ),
        Err(Error::InvalidPluginInput)
    );
}

#[test]
fn dj_button_routes_source_attribute_swap_and_never_indexes_digit_three() {
    let mut h = host();
    let dj = controller(
        &mut h,
        70,
        70,
        Config::DjStation {
            seed: Seed::new(1),
            group: 2,
            tile_x: 9,
            tile_y: 1,
        },
    );
    let mut p = player(&mut h, dj, DJ_STATION, 1, 0);
    clear(&mut h, std::slice::from_ref(&p));
    send(&mut h, &mut p, "press_button", WirePayload::Text("003")).unwrap();
    assert_eq!(events(&mut h), [(DJ_STATION, dj.invoker, 11, vec![48])]);
    assert_eq!(
        find(&private(&mut h, &p), "dj_active"),
        [b"300|000|000|000".to_vec()]
    );
    send(&mut h, &mut p, "press_button", WirePayload::Text("131")).unwrap();
    assert!(events(&mut h).is_empty());
    assert!(private(&mut h, &p).is_empty());
    send(&mut h, &mut p, "press_button", WirePayload::Text("102")).unwrap();
    assert_eq!(events(&mut h), [(DJ_STATION, dj.invoker, 10, vec![32])]);
}

#[test]
fn nightclub_links_real_floor_dj_and_dance_queue_observations() {
    let mut h = host();
    let floor = controller(
        &mut h,
        80,
        80,
        Config::NCDanceFloor {
            seed: Seed::new(3),
            tiles: floor_tiles(),
        },
    );
    let club = controller(
        &mut h,
        81,
        80,
        Config::Nightclub {
            seed: Seed::new(4),
            portals: vec![600, 601],
        },
    );
    assert!(
        h.take_native_commands()
            .iter()
            .any(|command| command.command
                == NativeCommand::BatchGraphics {
                    objects: vec![600, 601],
                    graphics: vec![255, 255]
                })
    );
    let dj = controller(
        &mut h,
        82,
        80,
        Config::DjStation {
            seed: Seed::new(5),
            group: 0,
            tile_x: 0,
            tile_y: 0,
        },
    );
    let dj_user = player(&mut h, dj, DJ_STATION, 1, 0);
    let platform = controller(&mut h, 83, 80, Config::DancePlatform { group: 1 });
    let mut dancer = player(&mut h, platform, DANCE_PLATFORM, 2, 0);
    clear(&mut h, &[dj_user.clone(), dancer.clone()]);
    native(
        &mut h,
        club,
        VmInput::NightclubRoundStart {
            dancers: vec![700, 701],
        },
    )
    .unwrap();
    assert_eq!(find(&private(&mut h, &dj_user), "dj_active").len(), 1);
    ticks(&mut h, 1);
    let commands = h.take_native_commands();
    let dance = commands
        .iter()
        .find_map(|command| match command.command {
            NativeCommand::ForceInteraction {
                caller,
                callee,
                interaction,
            } if [700, 701].contains(&caller) && caller == callee => Some(interaction - 4),
            _ => None,
        })
        .unwrap();
    native(
        &mut h,
        platform,
        VmInput::DanceQueueObservation {
            avatar_id: 2,
            active_uid: Some(99),
            interaction: dance + 6,
            on_this_platform: true,
            queue_active: true,
        },
    )
    .unwrap();
    ticks(&mut h, 1);
    events(&mut h);
    native(
        &mut h,
        platform,
        VmInput::DanceQueueObservation {
            avatar_id: 2,
            active_uid: None,
            interaction: 0,
            on_this_platform: false,
            queue_active: false,
        },
    )
    .unwrap();
    ticks(&mut h, 1);
    assert!(
        events(&mut h)
            .iter()
            .any(|(plugin, invoker, code, args)| *plugin == DANCE_PLATFORM
                && *invoker == platform.invoker
                && *code == 1
                && args[0] > 25)
    );
    send(&mut h, &mut dancer, "press_button", WirePayload::Text("2")).unwrap();
    assert!(events(&mut h).contains(&(DANCE_PLATFORM, platform.invoker, 2, vec![102])));
    assert_eq!(
        native(
            &mut h,
            platform,
            VmInput::DanceQueueObservation {
                avatar_id: 1,
                active_uid: Some(1),
                interaction: 6,
                on_this_platform: true,
                queue_active: true
            }
        ),
        Err(Error::NotAuthorized)
    );
    native(&mut h, club, VmInput::NightclubRoundEnd).unwrap();
    h.take_native_commands();
    ticks(&mut h, 6);
    assert!(
        !h.take_native_commands()
            .iter()
            .any(|command| matches!(command.command, NativeCommand::ForceInteraction { .. }))
    );
    native(
        &mut h,
        floor,
        VmInput::FloorAnimation {
            animation: 0,
            color: 0,
            direction: 0,
        },
    )
    .unwrap();
}

#[test]
fn nightclub_private_recovery_preserves_rng_raster_and_round_outputs() {
    let mut h = host();
    let floor = controller(
        &mut h,
        80,
        80,
        Config::NCDanceFloor {
            seed: Seed::new(3),
            tiles: floor_tiles(),
        },
    );
    let club = controller(
        &mut h,
        81,
        80,
        Config::Nightclub {
            seed: Seed::new(4),
            portals: vec![600],
        },
    );
    let dj = controller(
        &mut h,
        82,
        80,
        Config::DjStation {
            seed: Seed::new(5),
            group: 0,
            tile_x: 0,
            tile_y: 0,
        },
    );
    let dj_user = player(&mut h, dj, DJ_STATION, 1, 0);
    let platform = controller(&mut h, 83, 80, Config::DancePlatform { group: 1 });
    let dancer = player(&mut h, platform, DANCE_PLATFORM, 2, 0);
    native(
        &mut h,
        floor,
        VmInput::FloorAnimation {
            animation: 2,
            color: 3,
            direction: 0,
        },
    )
    .unwrap();
    native(
        &mut h,
        club,
        VmInput::NightclubRoundStart {
            dancers: vec![700, 701],
        },
    )
    .unwrap();
    native(
        &mut h,
        platform,
        VmInput::DanceQueueObservation {
            avatar_id: 2,
            active_uid: Some(57),
            interaction: 6,
            on_this_platform: true,
            queue_active: true,
        },
    )
    .unwrap();
    ticks(&mut h, 37);
    clear(&mut h, &[dj_user.clone(), dancer.clone()]);
    let (mut store, stamp) = saved(&mut h);
    ticks(&mut h, 170);
    let expected_events = events(&mut h);
    let expected_commands: Vec<_> = h
        .take_native_commands()
        .into_iter()
        .map(|output| (output.plugin, output.invoker, output.command))
        .collect();
    let expected_dj = private(&mut h, &dj_user);
    let expected_dancer = private(&mut h, &dancer);
    let mut recovered = restore(&mut store, stamp);
    for control in [floor, club, dj, platform] {
        recovered
            .rebind_native_controller(control.ticket.instance_address(), control.invoker)
            .unwrap();
    }
    let restored_dj = rebind(&mut recovered, &dj_user);
    let restored_dancer = rebind(&mut recovered, &dancer);
    clear(
        &mut recovered,
        &[restored_dj.clone(), restored_dancer.clone()],
    );
    ticks(&mut recovered, 170);
    assert_eq!(events(&mut recovered), expected_events);
    let actual_commands: Vec<_> = recovered
        .take_native_commands()
        .into_iter()
        .map(|output| (output.plugin, output.invoker, output.command))
        .collect();
    assert!(!actual_commands.is_empty());
    assert_eq!(actual_commands, expected_commands);
    assert_eq!(private(&mut recovered, &restored_dj), expected_dj);
    assert_eq!(private(&mut recovered, &restored_dancer), expected_dancer);
}
