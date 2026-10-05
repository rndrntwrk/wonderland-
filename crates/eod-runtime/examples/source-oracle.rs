// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Controlled NativeHost adapter for the unchanged C# handler component oracle.
//! No scenario changes source handler state through private implementation APIs.

use std::{cell::RefCell, collections::BTreeMap, io::Read, rc::Rc};
use wonderland_eod_runtime::{persistence::*, *};

type Outcome<T> = Result<T, String>;
const MAX_INPUT: usize = 2 * 1024 * 1024;
const MAX_LINE: usize = 300 * 1024;

#[derive(Clone, Default)]
struct Trace(
    Rc<RefCell<(usize, usize)>>,
    #[cfg(test)] Rc<RefCell<Vec<Vec<String>>>>,
);
impl Trace {
    fn row(&self, fields: &[String]) {
        let row = fields.join("\t");
        let mut count = self.0.borrow_mut();
        count.0 += row.len() + 1;
        count.1 += 1;
        assert!(row.len() <= MAX_LINE && count.0 <= 16 * 1024 * 1024 && count.1 <= 100_000);
        #[cfg(test)]
        self.1.borrow_mut().push(fields.to_vec());
        println!("{row}");
    }
    fn provider(&self, kind: &str, key: PluginDataKey, data: Option<&[u8]>) {
        let mut fields = vec![
            "PROVIDER".into(),
            kind.into(),
            key.persistent_object.to_string(),
            format!("{:08x}", key.plugin.0),
        ];
        match kind {
            "request" => {}
            "result" => {
                fields.push(u8::from(data.is_some()).to_string());
                fields.push(hex(data.unwrap_or_default()));
            }
            "save" => fields.push(hex(data.expect("save payload"))),
            _ => panic!("invalid provider trace kind"),
        }
        self.row(&fields);
    }
}

fn hex(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "-".into();
    }
    assert!(bytes.len() <= 128 * 1024);
    const DIGITS: &[u8] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(DIGITS[usize::from(byte >> 4)]));
        result.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    result
}

fn unhex(value: &str) -> Outcome<Vec<u8>> {
    if value == "-" {
        return Ok(vec![]);
    }
    if value.len() > 256 * 1024 || !value.len().is_multiple_of(2) {
        return Err("InvalidFixtureHexLength".into());
    }
    let nibble = |byte| match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err("InvalidFixtureHex".to_owned()),
    };
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok((nibble(pair[0])? << 4) | nibble(pair[1])?))
        .collect()
}

fn number<T: std::str::FromStr>(value: &str) -> Outcome<T> {
    value.parse().map_err(|_| "InvalidFixtureInteger".into())
}

fn object_id(value: &str) -> Outcome<u32> {
    let value = number::<u32>(value)?;
    if value == 0 || value > i16::MAX as u32 {
        return Err("InvalidFixtureObjectId".into());
    }
    Ok(value)
}

fn plugin(value: &str) -> Outcome<PluginId> {
    Ok(PluginId(match value {
        "timer" => 0xaa65fe9e,
        "dance" => 0x4a5be8ab,
        "signs" => 0x2a6356a0,
        "scoreboard" => 0x0949e698,
        "door" => 0x0a69f29f,
        _ => return Err("UnsupportedFixturePlugin".into()),
    }))
}

fn registers(value: &str) -> Outcome<[i16; 8]> {
    let fields: Vec<_> = value.split(',').collect();
    if !(4..=8).contains(&fields.len()) {
        return Err("InvalidFixtureRegisterCount".into());
    }
    let mut registers = [0; 8];
    for (slot, value) in registers.iter_mut().zip(fields) {
        *slot = number(value)?;
    }
    Ok(registers)
}

#[derive(Default)]
struct Authority(BTreeMap<ConnectionId, ActorId>);
impl ConnectionAuthority for Authority {
    fn authenticated_actor(&self, connection: ConnectionId) -> Option<ActorId> {
        self.0.get(&connection).copied()
    }
}

#[derive(Default)]
struct Registers(BTreeMap<InvokerId, TimerRegisters>);
impl RegisterSource for Registers {
    fn timer_registers(&self, invoker: InvokerId) -> Option<TimerRegisters> {
        self.0.get(&invoker).copied()
    }
}

#[derive(Default)]
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
        let length = self.0.len().min(destination.len());
        destination[..length].copy_from_slice(&self.0[..length]);
        Ok(PrivateRead {
            bytes_written: length,
            complete: length == self.0.len(),
        })
    }
}

struct PendingRead {
    snapshot: Option<(u64, Vec<u8>)>,
    ready: bool,
}
struct Provider {
    trace: Trace,
    data: BTreeMap<PluginDataKey, (u64, Vec<u8>)>,
    pending: BTreeMap<PluginDataKey, PendingRead>,
    receipts: BTreeMap<PluginWriteId, (PluginDataWrite, PluginWriteReceipt)>,
}
impl Provider {
    fn new(trace: Trace) -> Self {
        Self {
            trace,
            data: BTreeMap::new(),
            pending: BTreeMap::new(),
            receipts: BTreeMap::new(),
        }
    }
    fn release(&mut self) -> Outcome<()> {
        if self.pending.is_empty() {
            return Err("NoDeferredLoad".into());
        }
        for read in self.pending.values_mut() {
            read.ready = true;
        }
        Ok(())
    }
}
impl PluginDataProvider for Provider {
    fn load(
        &mut self,
        host: HostIdentity,
        key: PluginDataKey,
        destination: &mut [u8],
    ) -> Result<PluginDataRead, PersistenceFailure> {
        assert_eq!(host, identity());
        // A polling adapter retains one logical asynchronous request. Retrying
        // a not-yet-released callback is not a second storage operation.
        if !self.pending.contains_key(&key) {
            assert!(self.pending.len() < 128);
            self.pending.insert(
                key,
                PendingRead {
                    snapshot: self.data.get(&key).cloned(),
                    ready: false,
                },
            );
            self.trace.provider("request", key, None);
        }
        if !self.pending[&key].ready {
            return Err(PersistenceFailure::Retryable);
        }
        let read = self.pending.remove(&key).expect("pending read");
        self.trace.provider(
            "result",
            key,
            read.snapshot.as_ref().map(|(_, bytes)| bytes.as_slice()),
        );
        let (exists, revision, bytes) = match read.snapshot {
            None => (false, 0, vec![]),
            Some((revision, bytes)) => (true, revision, bytes),
        };
        let length = bytes.len().min(destination.len());
        destination[..length].copy_from_slice(&bytes[..length]);
        Ok(PluginDataRead {
            revision,
            exists,
            bytes_written: length,
            complete: length == bytes.len(),
        })
    }
    fn write(
        &mut self,
        host: HostIdentity,
        request: &PluginDataWrite,
    ) -> Result<PluginWriteReceipt, PersistenceFailure> {
        assert_eq!(host, identity());
        assert_eq!(request.key().scope, host.scope);
        assert_ne!(request.actor().0, 0);
        if let Some((prior, receipt)) = self.receipts.get(&request.id()) {
            assert!(prior == request, "conflicting immutable write identity");
            return Ok(*receipt);
        }
        let revision = self.data.get(&request.key()).map_or(0, |(rev, _)| *rev);
        assert_eq!(request.expected_revision(), revision);
        self.trace
            .provider("save", request.key(), Some(request.bytes()));
        self.data
            .insert(request.key(), (revision + 1, request.bytes().to_vec()));
        let receipt = PluginWriteReceipt {
            id: request.id(),
            decision: PluginWriteDecision::Applied {
                revision: revision + 1,
            },
        };
        self.receipts
            .insert(request.id(), (request.clone(), receipt));
        Ok(receipt)
    }
}

fn identity() -> HostIdentity {
    HostIdentity {
        scope: HostScopeId(987),
        epoch: 1,
    }
}

struct Binding {
    invoker: InvokerId,
    plugin: PluginId,
    connection: ConnectionId,
    actor: ActorId,
    ticket: Option<SessionTicket>,
    sequence: u64,
    closed: bool,
}
struct Runtime {
    host: NativeHost,
    auth: Authority,
    registers: Registers,
    bindings: BTreeMap<String, Binding>,
    provider: Provider,
    store: Store,
    trace: Trace,
}
impl Runtime {
    fn new(trace: Trace) -> Self {
        Self {
            host: NativeHost::new(identity(), HostLimits::default()).expect("fixture host"),
            auth: Authority::default(),
            registers: Registers::default(),
            bindings: BTreeMap::new(),
            provider: Provider::new(trace.clone()),
            store: Store::default(),
            trace,
        }
    }
    fn connect(&mut self, fields: &[&str]) -> Outcome<()> {
        if self.bindings.len() >= 128 || self.bindings.contains_key(fields[3]) {
            return Err("InvalidFixtureBinding".into());
        }
        let plugin = plugin(fields[4])?;
        let object = object_id(fields[5])?;
        let persistent_object = number(fields[6])?;
        let invoker = InvokerId(object_id(fields[7])?);
        let actor = ActorId(u64::from(number::<u32>(fields[8])?));
        let avatar_object = object_id(fields[9])? as i16;
        let permission: u8 = number(fields[10])?;
        if permission > 4 || !matches!(fields[11], "0" | "1") {
            return Err("InvalidFixtureAuthority".into());
        }
        let authorized = fields[11] == "1";
        let values = registers(fields[12])?;
        let connection = ConnectionId(self.bindings.len() as u64 + 1);
        self.auth.0.insert(connection, actor);
        let current = TimerRegisters([values[0], values[1], values[2], values[3]]);
        self.registers.0.insert(invoker, current);
        let ticket = if fields[4] == "timer" {
            self.host.connect(
                &self.auth,
                ConnectRequest {
                    connection,
                    plugin,
                    object,
                    invoker,
                    registers: current,
                },
            )
        } else {
            let input = match fields[4] {
                "dance" => PluginInput::DanceFloor { avatar_object },
                "scoreboard" => PluginInput::Scoreboard { persistent_object },
                "signs" => PluginInput::Signs {
                    persistent_object,
                    input: SignsInput {
                        mode: match values[0] {
                            0 => SignsMode::Erase,
                            1 => SignsMode::Write,
                            2 => SignsMode::Read,
                            3 => SignsMode::OwnerPermissions,
                            4 => SignsMode::OwnerWrite,
                            _ => return Err("InvalidFixtureSignsMode".into()),
                        },
                        max_length: u16::try_from(values[1]).map_err(|_| "InvalidFixtureLength")?,
                        is_roommate: permission >= 1,
                        owner_authorized: authorized,
                    },
                },
                "door" => PluginInput::PermissionDoor {
                    persistent_object,
                    input: DoorInput {
                        mode: match values[0] {
                            0 => DoorMode::Edit,
                            1 => DoorMode::View,
                            2 => DoorMode::CodeInput,
                            _ => return Err("InvalidFixtureDoorMode".into()),
                        },
                        max_fee: values[1],
                        permission_state: values[2],
                        door_fee: values[3],
                        flags: values[4],
                        edit_authorized: authorized,
                    },
                },
                _ => return Err("UnsupportedFixturePlugin".into()),
            };
            self.host.connect_plugin(
                &self.auth,
                PluginConnectRequest {
                    connection,
                    object,
                    invoker,
                    input,
                },
            )
        }
        .map_err(|error| error.to_string())?;
        self.bindings.insert(
            fields[3].into(),
            Binding {
                invoker,
                plugin,
                connection,
                actor,
                ticket: Some(ticket),
                sequence: 1,
                closed: false,
            },
        );
        Ok(())
    }
    fn execute(&mut self, fields: &[&str]) -> Outcome<()> {
        match fields[2] {
            "reset" => {}
            "seed" => {
                let key = PluginDataKey {
                    scope: identity().scope,
                    plugin: plugin(fields[3])?,
                    persistent_object: number(fields[4])?,
                };
                self.provider.data.insert(key, (1, unhex(fields[5])?));
            }
            "connect" => return self.connect(fields),
            "controller" => {
                let object = object_id(fields[4])?;
                let invoker = InvokerId(object_id(fields[5])?);
                self.host
                    .connect_dance_controller(object, invoker)
                    .map_err(|error| error.to_string())?;
                if self.bindings.contains_key(fields[3]) {
                    return Err("DuplicateFixtureBinding".into());
                }
                self.bindings.insert(
                    fields[3].into(),
                    Binding {
                        invoker,
                        plugin: plugin("dance")?,
                        connection: ConnectionId(0),
                        actor: ActorId(0),
                        ticket: None,
                        sequence: 1,
                        closed: false,
                    },
                );
            }
            "release" | "policy_release" => self.provider.release()?,
            "registers" => {
                let binding = self
                    .bindings
                    .get(fields[3])
                    .ok_or("UnknownFixtureBinding")?;
                let values = registers(fields[4])?;
                self.registers.0.insert(
                    binding.invoker,
                    TimerRegisters([values[0], values[1], values[2], values[3]]),
                );
            }
            "tick" => {
                let count: u32 = number(fields[3])?;
                if !(1..=10000).contains(&count) {
                    return Err("InvalidFixtureTickCount".into());
                }
                for _ in 0..count {
                    self.host
                        .tick(&self.auth, &self.registers)
                        .map_err(|error| error.to_string())?;
                }
            }
            "text" | "binary" | "policy_binary" => {
                let binding = self
                    .bindings
                    .get_mut(fields[3])
                    .ok_or("UnknownFixtureBinding")?;
                let bytes = unhex(fields[5])?;
                let payload = if fields[2] == "text" {
                    WirePayload::Text(
                        std::str::from_utf8(&bytes).map_err(|_| "InvalidFixtureUtf8")?,
                    )
                } else {
                    WirePayload::Binary(&bytes)
                };
                self.host
                    .receive(
                        &self.auth,
                        binding.connection,
                        ClientMessage {
                            version: protocol::PROTOCOL_VERSION,
                            ticket: binding.ticket.ok_or("ControllerHasNoUi")?,
                            plugin: binding.plugin,
                            sequence: binding.sequence,
                            event: fields[4],
                            payload,
                        },
                    )
                    .map_err(|error| error.to_string())?;
                binding.sequence += 1;
            }
            "disconnect" => {
                let binding = self
                    .bindings
                    .get(fields[3])
                    .ok_or("UnknownFixtureBinding")?;
                self.host
                    .disconnect_invoker(binding.invoker)
                    .map_err(|error| error.to_string())?;
            }
            _ => return Err("UnknownFixtureOperation".into()),
        }
        Ok(())
    }
    fn drain(&mut self) -> Outcome<()> {
        for event in self.host.take_public_events() {
            let (invoker, code, temps) = event.source_event();
            self.trace.row(&[
                "VM".into(),
                invoker.0.to_string(),
                code.to_string(),
                if temps.is_empty() {
                    "-".into()
                } else {
                    temps
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                },
            ]);
        }
        for binding in self.bindings.values_mut().filter(|binding| !binding.closed) {
            let Some(ticket) = binding.ticket else {
                continue;
            };
            for message in self
                .host
                .take_private(&self.auth, binding.connection, ticket)
                .map_err(|error| error.to_string())?
            {
                let (kind, bytes) = match message.body() {
                    UiBody::Text(text) => ("text", text.as_bytes()),
                    UiBody::Binary(bytes) => ("binary", bytes),
                };
                self.trace.row(&[
                    "UI".into(),
                    binding.actor.0.to_string(),
                    format!("{:08x}", message.plugin().0),
                    kind.into(),
                    message.event().into(),
                    hex(bytes),
                ]);
                if message.event() == "eod_leave" {
                    binding.closed = true;
                }
            }
        }
        Ok(())
    }
    fn finish_stage(&mut self) -> Vec<String> {
        let mut errors = vec![];
        if let Err(error) = self.drain() {
            // Failure to observe a channel invalidates the adapter boundary.
            // Keep this distinct from an expected native operation error.
            errors.push(format!("InitialDrainFailure{error}"));
            return errors;
        }
        // This test store models a completed event barrier. It establishes that
        // native writes were checkpoint-gated, not that a real VM was committed.
        let persistence = self
            .host
            .checkpoint_to(&mut self.store)
            .map_err(|error| error.to_string())
            .and_then(|_| match self.host.drive_persistence(&mut self.provider) {
                Ok(_) | Err(Error::PersistenceUnavailable) => Ok(()),
                Err(error) => Err(error.to_string()),
            });
        if let Err(error) = persistence {
            errors.push(error);
        }
        // A later provider failure does not undo earlier completed loads, and
        // their real VM/private effects must remain visible in this stage.
        if let Err(error) = self.drain() {
            errors.push(format!("FinalDrainFailure{error}"));
        }
        errors
    }
    fn complete_stage(&mut self, operation: Outcome<()>) -> Vec<String> {
        let mut errors: Vec<_> = operation.err().into_iter().collect();
        // Even an expected rejected operation may accidentally have queued
        // output or a write. Always observe the complete boundary, retaining
        // both the primary error and any independent completion failure.
        errors.extend(self.finish_stage());
        errors
    }
}

fn run() -> Outcome<()> {
    let mut input = vec![];
    std::io::stdin()
        .take((MAX_INPUT + 1) as u64)
        .read_to_end(&mut input)
        .map_err(|error| error.to_string())?;
    if input.len() > MAX_INPUT
        || !input.is_ascii()
        || input.contains(&0)
        || input.last() != Some(&b'\n')
    {
        return Err("InvalidBoundedFixtureInput".into());
    }
    let input = std::str::from_utf8(&input).map_err(|_| "InvalidFixtureInput")?;
    let trace = Trace::default();
    let mut runtime = None;
    for (index, line) in input.lines().enumerate() {
        if line.len() > MAX_LINE || index >= 4096 {
            return Err("FixtureLineOrStepLimit".into());
        }
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() < 3 {
            return Err("InvalidFixtureArity".into());
        }
        let arity = match fields[2] {
            "reset" | "release" | "policy_release" => 0,
            "tick" | "disconnect" => 1,
            "registers" => 2,
            "seed" | "controller" | "text" | "binary" | "policy_binary" => 3,
            "connect" => 10,
            _ => return Err("UnknownFixtureOperation".into()),
        };
        if fields.len() != arity + 3 {
            return Err("InvalidFixtureArity".into());
        }
        trace.row(&["BEGIN".into(), fields[0].into(), fields[1].into()]);
        if fields[2] == "reset" {
            runtime = Some(Runtime::new(trace.clone()));
        }
        let runtime = runtime.as_mut().ok_or("FixtureResetRequired")?;
        let operation = runtime.execute(&fields);
        for error in runtime.complete_stage(operation) {
            if fields[2].starts_with("policy_") {
                trace.row(&["ERROR".into(), error]);
            } else {
                return Err(format!("{}:{}: {error}", fields[0], fields[1]));
            }
        }
        trace.row(&["END".into(), fields[0].into(), fields[1].into()]);
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;

    #[test]
    fn operation_error_still_observes_real_native_disconnect_outputs() {
        let trace = Trace::default();
        let mut runtime = Runtime::new(trace.clone());
        runtime
            .execute(&[
                "error_boundary",
                "0",
                "connect",
                "timer",
                "timer",
                "11",
                "0",
                "101",
                "1001",
                "201",
                "1",
                "1",
                "0,0,1,2",
            ])
            .unwrap();
        assert!(runtime.complete_stage(Ok(())).is_empty());
        trace.1.borrow_mut().clear();
        runtime.host.disconnect_invoker(InvokerId(101)).unwrap();
        assert_eq!(
            runtime.complete_stage(Err("PluginNotReady".into())),
            vec!["PluginNotReady"]
        );
        assert_eq!(
            *trace.1.borrow(),
            vec![
                vec!["VM", "101", "-1", "-"],
                vec!["UI", "1001", "aa65fe9e", "text", "eod_leave", "-"],
            ]
        );
    }

    fn scoreboard(runtime: &mut Runtime, second: bool) {
        let (label, object, persistent, invoker, actor, avatar) = if second {
            ("second", "12", "78", "102", "1002", "202")
        } else {
            ("first", "11", "77", "101", "1001", "201")
        };
        runtime
            .execute(&[
                "error_boundary",
                "0",
                "connect",
                label,
                "scoreboard",
                object,
                persistent,
                invoker,
                actor,
                avatar,
                "0",
                "0",
                "0,0,0,0",
            ])
            .unwrap();
    }

    #[test]
    fn operation_error_still_checkpoints_and_observes_a_no_ui_color_save() {
        let trace = Trace::default();
        let mut runtime = Runtime::new(trace.clone());
        scoreboard(&mut runtime, false);
        assert!(runtime.complete_stage(Ok(())).is_empty());
        runtime.provider.release().unwrap();
        assert!(runtime.complete_stage(Ok(())).is_empty());
        trace.1.borrow_mut().clear();
        runtime
            .execute(&[
                "error_boundary",
                "1",
                "text",
                "first",
                "scoreboard_updatecolor",
                "5248532c507572706c65",
            ])
            .unwrap();
        assert!(trace.1.borrow().is_empty());
        assert_eq!(
            runtime.complete_stage(Err("InjectedOperationError".into())),
            vec!["InjectedOperationError"]
        );
        assert_eq!(
            *trace.1.borrow(),
            vec![
                vec!["VM", "101", "4", "5"],
                vec!["PROVIDER", "save", "77", "0949e698", "000500000000"],
            ]
        );
    }

    #[test]
    fn later_provider_error_preserves_earlier_load_ui_and_primary_error() {
        for primary in [None, Some("InjectedOperationError")] {
            let trace = Trace::default();
            let mut runtime = Runtime::new(trace.clone());
            scoreboard(&mut runtime, false);
            scoreboard(&mut runtime, true);
            runtime.drain().unwrap();
            trace.1.borrow_mut().clear();
            // Two controlled provider results are ready in native instance order.
            // The first completes normally and queues actual private UI; the
            // second contains a truncated record and fails source-data decoding.
            for (persistent_object, snapshot) in [(77, None), (78, Some((1, vec![0])))] {
                runtime.provider.pending.insert(
                    PluginDataKey {
                        scope: identity().scope,
                        plugin: plugin("scoreboard").unwrap(),
                        persistent_object,
                    },
                    PendingRead {
                        snapshot,
                        ready: true,
                    },
                );
            }
            let mut expected_errors: Vec<String> = primary.into_iter().map(str::to_owned).collect();
            expected_errors.push("InvalidPluginData".into());
            assert_eq!(
                runtime.complete_stage(primary.map_or(Ok(()), |error| Err(error.into()))),
                expected_errors
            );
            assert_eq!(
                *trace.1.borrow(),
                vec![
                    vec!["PROVIDER", "result", "77", "0949e698", "0", "-"],
                    vec!["PROVIDER", "result", "78", "0949e698", "1", "00"],
                    vec![
                        "UI",
                        "1001",
                        "0949e698",
                        "binary",
                        "scoreboard_state",
                        "000100000000"
                    ],
                ]
            );
        }
    }
}
