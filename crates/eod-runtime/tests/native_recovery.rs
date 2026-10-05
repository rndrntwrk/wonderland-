// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Real-host regressions for retained, potentially cyclic peer dependencies.
use wonderland_eod_runtime::{
    plugins::{service, social::*},
    *,
};

struct Auth;
impl ConnectionAuthority for Auth {
    fn authenticated_actor(&self, connection: ConnectionId) -> Option<ActorId> {
        Some(ActorId(connection.0 % 1000))
    }
}
struct Registers;
impl RegisterSource for Registers {
    fn timer_registers(&self, _: InvokerId) -> Option<TimerRegisters> {
        None
    }
}
#[derive(Clone, Default)]
struct Store(Vec<u8>);
impl PrivateCheckpointStore for Store {
    fn write_private(&mut self, _: PrivateCheckpointKey, bytes: &[u8]) -> Result<(), StoreError> {
        self.0 = bytes.to_vec();
        Ok(())
    }
    fn read_private(
        &mut self,
        _: PrivateCheckpointKey,
        into: &mut [u8],
    ) -> Result<PrivateRead, StoreError> {
        let n = into.len().min(self.0.len());
        into[..n].copy_from_slice(&self.0[..n]);
        Ok(PrivateRead {
            bytes_written: n,
            complete: n == self.0.len(),
        })
    }
}
fn identity(epoch: u64) -> HostIdentity {
    HostIdentity {
        scope: HostScopeId(991),
        epoch,
    }
}
fn host() -> NativeHost {
    NativeHost::new(identity(1), HostLimits::default()).unwrap()
}
fn control(
    h: &mut NativeHost,
    object: u32,
    cluster: u64,
    config: NativePluginInput,
) -> NativeControllerTicket {
    h.connect_native(NativeCreateRequest {
        object,
        cluster,
        invoker: InvokerId(object),
        input: config,
    })
    .unwrap()
}
fn social(h: &mut NativeHost, object: u32, cluster: u64, config: Config) -> NativeControllerTicket {
    control(h, object, cluster, NativePluginInput::Social(config))
}
fn player(h: &mut NativeHost, c: NativeControllerTicket, id: u64) -> SessionTicket {
    h.join_native(
        &Auth,
        NativeJoinRequest {
            connection: ConnectionId(id),
            group: c.instance_address(),
            invoker: InvokerId(1000 + id as u32),
            avatar_object: id as i16,
            avatar_id: id as u32,
            input: MemberInput::default(),
        },
    )
    .unwrap()
}
fn rebind(h: &mut NativeHost, t: SessionTicket, id: u64) -> SessionTicket {
    h.rebind(
        &Auth,
        ConnectionId(1000 + id),
        t.instance_address(),
        TimerRegisters([0; 4]),
    )
    .unwrap()
}
fn private(h: &mut NativeHost, t: SessionTicket, c: u64) -> Vec<(String, Vec<u8>)> {
    h.take_private(&Auth, ConnectionId(c), t)
        .unwrap()
        .into_iter()
        .map(|m| {
            (
                m.event().to_owned(),
                match m.body() {
                    UiBody::Binary(b) => b.to_vec(),
                    UiBody::Text(s) => s.as_bytes().to_vec(),
                },
            )
        })
        .collect()
}
fn message(
    h: &mut NativeHost,
    t: SessionTicket,
    c: u64,
    plugin: PluginId,
    event: &str,
    data: &[u8],
) -> Result<DispatchOutcome, Error> {
    h.receive(
        &Auth,
        ConnectionId(c),
        ClientMessage {
            version: protocol::PROTOCOL_VERSION,
            ticket: t,
            plugin,
            sequence: 1,
            event,
            payload: WirePayload::Binary(data),
        },
    )
}
fn native(
    h: &mut NativeHost,
    c: NativeControllerTicket,
    invoker: u32,
    input: VmInput,
) -> Result<(), Error> {
    h.deliver_native_event(c, InvokerId(invoker), NativeVmInput::Social(input))
}
fn ticks(h: &mut NativeHost, n: usize) {
    for _ in 0..n {
        h.tick(&Auth, &Registers).unwrap();
    }
}
fn commands(h: &mut NativeHost) -> Vec<(PluginId, InvokerId, NativeCommand)> {
    h.take_native_commands()
        .into_iter()
        .map(|c| (c.plugin, c.invoker, c.command))
        .collect()
}

#[test]
fn retained_floor_fences_silent_club_rng_ticks_and_native_callbacks_after_restore() {
    let mut h = host();
    let floor = social(
        &mut h,
        10,
        99,
        Config::NCDanceFloor {
            seed: Seed::new(1),
            tiles: vec![FloorTile {
                object: 500,
                x: 0,
                y: 0,
            }],
        },
    );
    let club = social(
        &mut h,
        11,
        99,
        Config::Nightclub {
            seed: Seed::new(2),
            portals: vec![600],
        },
    );
    native(
        &mut h,
        club,
        11,
        VmInput::NightclubRoundStart { dancers: vec![400] },
    )
    .unwrap();
    h.take_public_events();
    commands(&mut h);
    let mut store = Store::default();
    let stamp = h.checkpoint_to(&mut store).unwrap();
    ticks(&mut h, 1);
    let expected = commands(&mut h);
    assert!(!expected.is_empty());
    let mut recovered =
        NativeHost::restore_from(&mut store, identity(2), stamp, HostLimits::default()).unwrap();
    let rebound = recovered
        .rebind_native_controller(club.instance_address(), InvokerId(11))
        .unwrap();
    ticks(&mut recovered, 9);
    assert!(
        commands(&mut recovered).is_empty(),
        "the retained detached floor must fence all club RNG/tick advancement"
    );
    assert_eq!(
        native(&mut recovered, rebound, 11, VmInput::NightclubRoundEnd),
        Err(Error::PluginNotReady)
    );
    recovered
        .rebind_native_controller(floor.instance_address(), InvokerId(10))
        .unwrap();
    ticks(&mut recovered, 1);
    assert_eq!(commands(&mut recovered), expected);
}

#[test]
fn buzzer_countdown_subframes_wait_for_required_player_controller_and_member() {
    let mut h = host();
    let hc = social(&mut h, 30, 30, Config::BuzzerHost);
    let pc = social(
        &mut h,
        31,
        30,
        Config::BuzzerPlayer {
            names: vec![AvatarName {
                avatar_id: 1,
                name: "one".into(),
            }],
        },
    );
    let pt = player(&mut h, pc, 1);
    let ht = player(&mut h, hc, 2);
    message(
        &mut h,
        ht,
        2,
        BUZZER_HOST,
        "Buzzer_Host_A_ToggleMaster",
        &[1],
    )
    .unwrap();
    private(&mut h, pt, 1);
    private(&mut h, ht, 2);
    h.take_public_events();
    commands(&mut h);
    let mut store = Store::default();
    let stamp = h.checkpoint_to(&mut store).unwrap();
    let mut h =
        NativeHost::restore_from(&mut store, identity(2), stamp, HostLimits::default()).unwrap();
    h.rebind_native_controller(hc.instance_address(), InvokerId(30))
        .unwrap();
    let ht = rebind(&mut h, ht, 2);
    private(&mut h, ht, 1002);
    ticks(&mut h, 29);
    h.rebind_native_controller(pc.instance_address(), InvokerId(31))
        .unwrap();
    ticks(&mut h, 1);
    assert!(
        private(&mut h, ht, 1002).is_empty(),
        "no second or partial second may advance before the player binds"
    );
    let pt = rebind(&mut h, pt, 1);
    private(&mut h, pt, 1001);
    ticks(&mut h, 29);
    assert!(
        private(&mut h, ht, 1002).is_empty(),
        "recovery must resume with the original countdown subframe"
    );
    ticks(&mut h, 1);
    assert!(
        private(&mut h, ht, 1002)
            .iter()
            .any(|(event, _)| event == "BuzzerEOD_Timer")
    );
}

#[test]
fn transitive_club_dependency_fences_dj_but_unrelated_same_cluster_property_progresses() {
    let mut h = host();
    let floor = social(
        &mut h,
        10,
        99,
        Config::NCDanceFloor {
            seed: Seed::new(1),
            tiles: vec![FloorTile {
                object: 500,
                x: 0,
                y: 0,
            }],
        },
    );
    let club = social(
        &mut h,
        11,
        99,
        Config::Nightclub {
            seed: Seed::new(2),
            portals: vec![],
        },
    );
    let dj = social(
        &mut h,
        12,
        99,
        Config::DjStation {
            seed: Seed::new(3),
            group: 0,
            tile_x: 0,
            tile_y: 0,
        },
    );
    let dt = player(&mut h, dj, 1);
    let property = control(
        &mut h,
        20,
        99,
        NativePluginInput::Service(service::Config::PropertySelect),
    );
    let property_user = player(&mut h, property, 2);
    private(&mut h, dt, 1);
    private(&mut h, property_user, 2);
    h.take_public_events();
    commands(&mut h);
    let mut store = Store::default();
    let stamp = h.checkpoint_to(&mut store).unwrap();
    let mut h =
        NativeHost::restore_from(&mut store, identity(2), stamp, HostLimits::default()).unwrap();
    for (c, id) in [(club, 11), (dj, 12), (property, 20)] {
        h.rebind_native_controller(c.instance_address(), InvokerId(id))
            .unwrap();
    }
    let dt = rebind(&mut h, dt, 1);
    let property_user = rebind(&mut h, property_user, 2);
    private(&mut h, dt, 1001);
    private(&mut h, property_user, 1002);
    h.take_public_events();
    commands(&mut h);
    assert_eq!(
        h.receive(
            &Auth,
            ConnectionId(1001),
            ClientMessage {
                version: protocol::PROTOCOL_VERSION,
                ticket: dt,
                plugin: DJ_STATION,
                sequence: 1,
                event: "press_button",
                payload: WirePayload::Text("0|0")
            }
        ),
        Err(Error::PluginNotReady)
    );
    ticks(&mut h, 1);
    assert!(
        private(&mut h, property_user, 1002)
            .iter()
            .any(|(event, _)| event == "property_show")
    );
    assert!(private(&mut h, dt, 1001).is_empty());
    h.rebind_native_controller(floor.instance_address(), InvokerId(10))
        .unwrap();
    ticks(&mut h, 1);
}

#[test]
fn authoritative_teardown_can_clear_a_detached_dependency_from_its_bound_peer() {
    for closing_floor in [true, false] {
        let mut h = host();
        let floor = social(
            &mut h,
            10,
            99,
            Config::NCDanceFloor {
                seed: Seed::new(1),
                tiles: vec![FloorTile {
                    object: 500,
                    x: 0,
                    y: 0,
                }],
            },
        );
        let club = social(
            &mut h,
            11,
            99,
            Config::Nightclub {
                seed: Seed::new(2),
                portals: vec![],
            },
        );
        h.take_public_events();
        commands(&mut h);
        let mut store = Store::default();
        let stamp = h.checkpoint_to(&mut store).unwrap();
        let mut h = NativeHost::restore_from(&mut store, identity(2), stamp, HostLimits::default())
            .unwrap();
        let (bound, bound_id, closed, closed_id) = if closing_floor {
            (club, 11, floor, 10)
        } else {
            (floor, 10, club, 11)
        };
        h.rebind_native_controller(bound.instance_address(), InvokerId(bound_id))
            .unwrap();
        assert_eq!(h.disconnect_invoker(InvokerId(closed_id)), Ok(()));
        assert_eq!(
            h.rebind_native_controller(closed.instance_address(), InvokerId(closed_id)),
            Err(Error::StaleSession)
        );
        h.take_public_events();
        commands(&mut h);
        h.checkpoint_to(&mut Store::default()).unwrap();
        ticks(&mut h, 1);
    }
}
