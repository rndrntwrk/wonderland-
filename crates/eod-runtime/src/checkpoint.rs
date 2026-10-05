// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Private persistence boundary. A store must authenticate and protect these
//! bytes, write atomically, and retain a trusted latest stamp to prevent rollback.
//! There is deliberately no generic checkpoint byte getter or Debug dump.

use crate::{
    games::{GameState, MAX_GAME_BYTES, SharedGame},
    host::{DanceController, HandlerState, Instance, Participant, State},
    persistence::{Binding, BindingStatus, WriteRecord, *},
    registry::{self, TIMER_PLUGIN},
    source_plugins::{PermissionDoor, Scoreboard, Signs},
    timer::Timer,
    *,
};
use std::collections::{BTreeMap, BTreeSet};

const FORMAT_VERSION: u16 = 1;
const TIMER_SCHEMA: u16 = 1;
const HEADER_SIZE: usize = 68;
const INSTANCE_SIZE: usize = 75;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CheckpointKind {
    Host,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrivateCheckpointKey {
    pub scope: HostScopeId,
    pub kind: CheckpointKind,
}

/// Public metadata only. Obtain the latest expected stamp from trusted durable
/// storage, never from an untrusted checkpoint or a reconnecting client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckpointStamp {
    pub scope: HostScopeId,
    pub epoch: u64,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrivateRead {
    pub bytes_written: usize,
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreError {
    Unavailable,
    TooLarge,
    Corrupt,
}

/// Private-only provider, distinct from common VM snapshots and public outputs.
/// The bounded destination prevents the host from allocating an input-specified
/// buffer. Return `complete: false` if the stored checkpoint does not fit.
pub trait PrivateCheckpointStore {
    fn write_private(&mut self, key: PrivateCheckpointKey, bytes: &[u8]) -> Result<(), StoreError>;
    fn read_private(
        &mut self,
        key: PrivateCheckpointKey,
        destination: &mut [u8],
    ) -> Result<PrivateRead, StoreError>;
}

impl NativeHost {
    /// The adapter must first apply public events and deliver private outputs at
    /// the same VM checkpoint barrier. Merely draining queues is not a VM commit.
    pub fn checkpoint_to(
        &mut self,
        store: &mut impl PrivateCheckpointStore,
    ) -> Result<CheckpointStamp, Error> {
        if !self.state.native_commands.is_empty()
            || !self.state.public.is_empty()
            || !self.state.private.is_empty()
            || self.state.instances.values().any(|instance| {
                instance
                    .persistence
                    .as_ref()
                    .is_some_and(|binding| binding.status == BindingStatus::Reconcile)
            })
        {
            return Err(Error::CheckpointBusy);
        }
        let revision = self
            .state
            .checkpoint_revision
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        let stamp = CheckpointStamp {
            scope: self.identity.scope,
            epoch: self.identity.epoch,
            revision,
        };
        if !self.state.native_groups.is_empty()
            || !self.state.native_operations.is_empty()
            || !self.state.controllers.is_empty()
            || !self.state.games.is_empty()
            || !self.state.writes.is_empty()
            || self
                .state
                .instances
                .values()
                .any(|instance| !matches!(instance.handler, HandlerState::Timer(_)))
        {
            let bytes =
                if self.state.native_groups.is_empty() && self.state.native_operations.is_empty() {
                    encode_v2(self, stamp)?
                } else {
                    crate::native_checkpoint::encode(self, stamp)?
                };
            store
                .write_private(
                    PrivateCheckpointKey {
                        scope: stamp.scope,
                        kind: CheckpointKind::Host,
                    },
                    &bytes,
                )
                .map_err(map_store_error)?;
            self.state.checkpoint_revision = revision;
            for record in self.state.native_operations.values_mut() {
                record.prepared = true;
            }
            for record in self.state.writes.values_mut() {
                record.prepared = true;
            }
            return Ok(stamp);
        }
        let size = self
            .state
            .instances
            .len()
            .checked_mul(INSTANCE_SIZE)
            .and_then(|n| n.checked_add(HEADER_SIZE))
            .ok_or(Error::CheckpointTooLarge)?;
        if size > self.limits.max_checkpoint_bytes {
            return Err(Error::CheckpointTooLarge);
        }
        let mut writer = Writer(Vec::with_capacity(size));
        writer.0.extend_from_slice(b"EODP");
        writer.u16(FORMAT_VERSION);
        writer.u16(TIMER_SCHEMA);
        writer.u64(stamp.scope.0);
        writer.u64(stamp.epoch);
        writer.u64(stamp.revision);
        writer.u64(self.state.tick);
        writer.u64(self.state.next_instance);
        writer.u64(self.state.next_session);
        writer.u64(self.limits.idle_timeout_ticks);
        writer.u32(self.state.instances.len() as u32);
        for (id, instance) in &self.state.instances {
            let participant = &instance.participant;
            let HandlerState::Timer(timer) = &instance.handler else {
                return Err(Error::InvalidCheckpoint);
            };
            writer.u64(id.0);
            writer.u32(instance.plugin.0);
            writer.u32(instance.object);
            writer.u64(participant.actor.0);
            writer.u32(participant.invoker.0);
            writer.u64(participant.generation);
            writer.u64(participant.last_activity);
            writer.u64(participant.next_sequence);
            writer.u64(participant.rate_tick);
            writer.u32(participant.messages_this_tick);
            writer.i16(timer.minutes);
            writer.i16(timer.seconds);
            writer.u8(u8::from(timer.running));
            writer.u8(timer.mode);
            writer.u8(u8::from(timer.updated_after_stop));
            writer.i32(timer.tock);
        }
        debug_assert_eq!(writer.0.len(), size);
        let key = PrivateCheckpointKey {
            scope: stamp.scope,
            kind: CheckpointKind::Host,
        };
        store
            .write_private(key, &writer.0)
            .map_err(map_store_error)?;
        self.state.checkpoint_revision = revision;
        Ok(stamp)
    }

    /// All restored sessions are detached. A newer epoch is mandatory, and only
    /// the recorded actor can rebind; no transport connection is serialized.
    pub fn restore_from(
        store: &mut impl PrivateCheckpointStore,
        identity: HostIdentity,
        expected: CheckpointStamp,
        limits: HostLimits,
    ) -> Result<Self, Error> {
        limits.validate()?;
        if identity.scope.0 == 0
            || identity.epoch == 0
            || expected.epoch == 0
            || expected.revision == 0
        {
            return Err(Error::InvalidIdentity);
        }
        if identity.scope != expected.scope {
            return Err(Error::CheckpointStampMismatch);
        }
        if identity.epoch <= expected.epoch {
            return Err(Error::WrongEpoch);
        }
        let key = PrivateCheckpointKey {
            scope: identity.scope,
            kind: CheckpointKind::Host,
        };
        let mut bytes = vec![0; limits.max_checkpoint_bytes];
        let read = store
            .read_private(key, &mut bytes)
            .map_err(map_store_error)?;
        if !read.complete || read.bytes_written > bytes.len() {
            return Err(Error::CheckpointTooLarge);
        }
        bytes.truncate(read.bytes_written);
        decode(&bytes, identity, expected, limits)
    }
}

fn map_store_error(error: StoreError) -> Error {
    match error {
        StoreError::TooLarge => Error::CheckpointTooLarge,
        StoreError::Corrupt => Error::InvalidCheckpoint,
        StoreError::Unavailable => Error::StoreFailure,
    }
}

pub(crate) fn decode(
    bytes: &[u8],
    identity: HostIdentity,
    expected: CheckpointStamp,
    limits: HostLimits,
) -> Result<NativeHost, Error> {
    if bytes.get(..6) == Some(&b"EODP\x04\x00"[..]) {
        return crate::native_checkpoint::decode(bytes, identity, expected, limits);
    }
    if bytes.get(..6) == Some(&b"EODP\x02\x00"[..]) || bytes.get(..6) == Some(&b"EODP\x03\x00"[..])
    {
        return decode_v2(bytes, identity, expected, limits);
    }
    let mut reader = Reader { bytes, position: 0 };
    if reader.take(4)? != b"EODP" {
        return Err(Error::InvalidCheckpoint);
    }
    if reader.u16()? != FORMAT_VERSION {
        return Err(Error::UnsupportedCheckpointVersion);
    }
    if reader.u16()? != TIMER_SCHEMA {
        return Err(Error::UnsupportedPluginSchema);
    }
    let stamp = CheckpointStamp {
        scope: HostScopeId(reader.u64()?),
        epoch: reader.u64()?,
        revision: reader.u64()?,
    };
    if stamp != expected {
        return Err(Error::CheckpointStampMismatch);
    }
    let tick = reader.u64()?;
    let next_instance = reader.u64()?;
    let next_session = reader.u64()?;
    let idle_timeout = reader.u64()?;
    let count = reader.u32()? as usize;
    if idle_timeout != limits.idle_timeout_ticks
        || next_instance == 0
        || next_session == 0
        || count > limits.max_instances
        || count > limits.max_participants
        || count > limits.max_timers
    {
        return Err(Error::InvalidCheckpoint);
    }
    let expected_length = count
        .checked_mul(INSTANCE_SIZE)
        .and_then(|n| n.checked_add(HEADER_SIZE))
        .ok_or(Error::InvalidCheckpoint)?;
    if expected_length != bytes.len() {
        return Err(Error::InvalidCheckpoint);
    }
    let mut instances = BTreeMap::new();
    let mut actors = BTreeSet::new();
    let mut invokers = BTreeSet::new();
    let mut generations = BTreeSet::new();
    for _ in 0..count {
        let id = InstanceId(reader.u64()?);
        let plugin = PluginId(reader.u32()?);
        if plugin != TIMER_PLUGIN {
            return Err(if registry::lookup(plugin).is_some() {
                Error::UnverifiedPlugin
            } else {
                Error::UnregisteredPlugin
            });
        }
        let object = reader.u32()?;
        let actor = ActorId(reader.u64()?);
        let invoker = InvokerId(reader.u32()?);
        let generation = reader.u64()?;
        let last_activity = reader.u64()?;
        let next_sequence = reader.u64()?;
        let rate_tick = reader.u64()?;
        let messages_this_tick = reader.u32()?;
        let timer = Timer {
            minutes: reader.i16()?,
            seconds: reader.i16()?,
            running: reader.boolean()?,
            mode: reader.u8()?,
            updated_after_stop: reader.boolean()?,
            tock: reader.i32()?,
        };
        if id.0 == 0
            || id.0 >= next_instance
            || object == 0
            || actor.0 == 0
            || invoker.0 == 0
            || generation == 0
            || generation >= next_session
            || next_sequence == 0
            || last_activity > tick
            || rate_tick > tick
            || messages_this_tick > limits.max_messages_per_tick
            || timer.mode > 1
            || last_activity
                .checked_add(idle_timeout)
                .is_none_or(|deadline| deadline <= tick)
            || !actors.insert(actor)
            || !invokers.insert(invoker)
            || !generations.insert(generation)
            || instances.contains_key(&id)
        {
            return Err(Error::InvalidCheckpoint);
        }
        instances.insert(
            id,
            Instance {
                plugin,
                object,
                handler: HandlerState::Timer(timer),
                persistence: None,
                participant: Participant {
                    actor,
                    connection: None,
                    invoker,
                    generation,
                    last_activity,
                    next_sequence,
                    rate_tick,
                    messages_this_tick,
                },
            },
        );
    }
    if reader.position != bytes.len() {
        return Err(Error::InvalidCheckpoint);
    }
    Ok(NativeHost {
        identity,
        limits,
        state: State {
            tick,
            next_instance,
            next_session,
            checkpoint_revision: stamp.revision,
            next_write: 1,
            instances,
            controllers: BTreeMap::new(),
            games: BTreeMap::new(),
            native_groups: BTreeMap::new(),
            native_operations: BTreeMap::new(),
            next_native_operation: 1,
            native_commands: vec![],
            writes: BTreeMap::new(),
            public: vec![],
            private: vec![],
        },
    })
}

// Format 2 stores per-handler schemas, native controller bindings and immutable
// plugin-data intents. No connection IDs or public/private delivery queues occur.
pub(crate) fn encode_v2(host: &NativeHost, stamp: CheckpointStamp) -> Result<Vec<u8>, Error> {
    let limit = host.limits.max_checkpoint_bytes;
    let mut writer = Writer(Vec::new());
    check_write_size(0, HEADER_SIZE, limit)?;
    writer.0.extend_from_slice(b"EODP");
    let format = if host.state.games.is_empty() { 2 } else { 3 };
    writer.u16(format);
    writer.u16(1);
    writer.u64(stamp.scope.0);
    writer.u64(stamp.epoch);
    writer.u64(stamp.revision);
    writer.u64(host.state.tick);
    writer.u64(host.state.next_instance);
    writer.u64(host.state.next_session);
    writer.u64(host.limits.idle_timeout_ticks);
    writer.u32(host.state.instances.len() as u32);
    for (id, instance) in &host.state.instances {
        let payload = encode_handler(&instance.handler);
        if payload.len() > host.limits.max_plugin_data_bytes + 128 {
            return Err(Error::CheckpointTooLarge);
        }
        let binding_size = instance
            .persistence
            .as_ref()
            .map_or(0, |binding| 18 + binding.bytes.len());
        check_write_size(writer.0.len(), 71 + payload.len() + binding_size, limit)?;
        let participant = &instance.participant;
        writer.u64(id.0);
        writer.u32(instance.plugin.0);
        writer.u32(instance.object);
        writer.u64(participant.actor.0);
        writer.u32(participant.invoker.0);
        writer.u64(participant.generation);
        writer.u64(participant.last_activity);
        writer.u64(participant.next_sequence);
        writer.u64(participant.rate_tick);
        writer.u32(participant.messages_this_tick);
        writer.u16(1); // Per-plugin native checkpoint schema, not source persistence schema.
        writer.blob(&payload);
        writer.u8(u8::from(instance.persistence.is_some()));
        if let Some(binding) = &instance.persistence {
            writer.u8(match binding.status {
                BindingStatus::Loading => 0,
                BindingStatus::Ready => 1,
                BindingStatus::Reconcile => return Err(Error::CheckpointBusy),
            });
            writer.u32(binding.key.persistent_object);
            writer.u64(binding.revision);
            writer.u8(u8::from(binding.exists));
            writer.blob(&binding.bytes);
        }
    }
    check_write_size(
        writer.0.len(),
        16 + host.state.controllers.len() * 16,
        limit,
    )?;
    writer.u32(host.state.controllers.len() as u32);
    for (id, controller) in &host.state.controllers {
        writer.u64(id.0);
        writer.u32(controller.object);
        writer.u32(controller.invoker.0);
    }
    if format == 3 {
        check_write_size(writer.0.len(), 4, limit)?;
        writer.u32(host.state.games.len() as u32);
        for (id, game) in &host.state.games {
            let payload = game.handler.save_private();
            if payload.len() > MAX_GAME_BYTES {
                return Err(Error::CheckpointTooLarge);
            }
            check_write_size(writer.0.len(), 58 + payload.len() + 12, limit)?;
            writer.u64(id.0);
            writer.u32(game.handler.plugin().0);
            writer.u32(game.object);
            writer.u32(game.invoker.0);
            writer.u16(1);
            for seat in game.seats {
                writer.u64(seat.map_or(0, |id| id.0));
            }
            writer.blob(&payload);
        }
    }
    writer.u64(host.state.next_write);
    writer.u32(host.state.writes.len() as u32);
    for (id, record) in &host.state.writes {
        let request = &record.request;
        check_write_size(writer.0.len(), 53 + request.bytes.len(), limit)?;
        writer.u64(id.origin_epoch);
        writer.u64(id.instance.0);
        writer.u64(id.operation);
        writer.u32(id.key.plugin.0);
        writer.u32(id.key.persistent_object);
        writer.u64(request.actor.0);
        writer.u64(request.expected_revision);
        writer.u8(u8::from(record.conflicted));
        writer.blob(&request.bytes);
    }
    Ok(writer.0)
}

fn check_write_size(current: usize, extra: usize, limit: usize) -> Result<(), Error> {
    if current.checked_add(extra).is_none_or(|size| size > limit) {
        Err(Error::CheckpointTooLarge)
    } else {
        Ok(())
    }
}

fn encode_handler(handler: &HandlerState) -> Vec<u8> {
    match handler {
        HandlerState::Timer(timer) => {
            let mut writer = Writer(vec![]);
            writer.i16(timer.minutes);
            writer.i16(timer.seconds);
            writer.u8(u8::from(timer.running));
            writer.u8(timer.mode);
            writer.u8(u8::from(timer.updated_after_stop));
            writer.i32(timer.tock);
            writer.0
        }
        HandlerState::DanceFloor { avatar_object } => avatar_object.to_le_bytes().to_vec(),
        HandlerState::Signs(state) => state.save_private(),
        HandlerState::Scoreboard(state) => state.save_private(),
        HandlerState::PermissionDoor(state) => state.save_private(),
        HandlerState::NativeParticipant { .. } => {
            unreachable!("native participants use private format4")
        }
        HandlerState::GameParticipant {
            game,
            slot,
            avatar_object,
        } => {
            let mut writer = Writer(Vec::with_capacity(11));
            writer.u64(game.0);
            writer.u8(*slot);
            writer.i16(*avatar_object);
            writer.0
        }
    }
}

fn decode_handler(plugin: PluginId, payload: &[u8]) -> Result<HandlerState, Error> {
    let registration = registry::lookup(plugin).ok_or(Error::UnregisteredPlugin)?;
    if registration.runtime == registry::RuntimeStatus::UnsupportedUnverified {
        return Err(Error::UnverifiedPlugin);
    }
    Ok(match plugin {
        registry::TIMER_PLUGIN => {
            let mut reader = Reader {
                bytes: payload,
                position: 0,
            };
            let timer = Timer {
                minutes: reader.i16()?,
                seconds: reader.i16()?,
                running: reader.boolean()?,
                mode: reader.u8()?,
                updated_after_stop: reader.boolean()?,
                tock: reader.i32()?,
            };
            if timer.mode > 1 || reader.position != payload.len() {
                return Err(Error::InvalidCheckpoint);
            }
            HandlerState::Timer(timer)
        }
        registry::DANCE_FLOOR_PLUGIN => {
            let avatar_object =
                i16::from_le_bytes(payload.try_into().map_err(|_| Error::InvalidCheckpoint)?);
            if avatar_object <= 0 {
                return Err(Error::InvalidCheckpoint);
            }
            HandlerState::DanceFloor { avatar_object }
        }
        registry::SIGNS_PLUGIN => HandlerState::Signs(
            Signs::restore_private(payload).map_err(|_| Error::InvalidCheckpoint)?,
        ),
        registry::SCOREBOARD_PLUGIN => HandlerState::Scoreboard(
            Scoreboard::restore_private(payload).map_err(|_| Error::InvalidCheckpoint)?,
        ),
        registry::PERMISSION_DOOR_PLUGIN => HandlerState::PermissionDoor(
            PermissionDoor::restore_private(payload).map_err(|_| Error::InvalidCheckpoint)?,
        ),
        registry::PAPER_CHASE_PLUGIN | registry::PIZZA_MAKER_PLUGIN | registry::MAZE_PLUGIN => {
            let mut reader = Reader {
                bytes: payload,
                position: 0,
            };
            let game = InstanceId(reader.u64()?);
            let slot = reader.u8()?;
            let avatar_object = reader.i16()?;
            let count = if plugin == registry::PAPER_CHASE_PLUGIN {
                3
            } else if plugin == registry::PIZZA_MAKER_PLUGIN {
                4
            } else {
                2
            };
            if game.0 == 0
                || slot >= count
                || avatar_object <= 0
                || reader.position != payload.len()
            {
                return Err(Error::InvalidCheckpoint);
            }
            HandlerState::GameParticipant {
                game,
                slot,
                avatar_object,
            }
        }
        _ => return Err(Error::UnverifiedPlugin),
    })
}

fn decode_v2(
    bytes: &[u8],
    identity: HostIdentity,
    expected: CheckpointStamp,
    limits: HostLimits,
) -> Result<NativeHost, Error> {
    let mut reader = Reader { bytes, position: 0 };
    if reader.take(4)? != b"EODP" {
        return Err(Error::UnsupportedCheckpointVersion);
    }
    let format = reader.u16()?;
    if !matches!(format, 2 | 3) {
        return Err(Error::UnsupportedCheckpointVersion);
    }
    if reader.u16()? != 1 {
        return Err(Error::UnsupportedPluginSchema);
    }
    let stamp = CheckpointStamp {
        scope: HostScopeId(reader.u64()?),
        epoch: reader.u64()?,
        revision: reader.u64()?,
    };
    if stamp != expected {
        return Err(Error::CheckpointStampMismatch);
    }
    let tick = reader.u64()?;
    let next_instance = reader.u64()?;
    let next_session = reader.u64()?;
    let idle_timeout = reader.u64()?;
    let count = reader.u32()? as usize;
    if idle_timeout != limits.idle_timeout_ticks
        || next_instance == 0
        || next_session == 0
        || count > limits.max_instances
        || count > limits.max_participants
    {
        return Err(Error::InvalidCheckpoint);
    }
    let mut instances = BTreeMap::new();
    let mut actors = BTreeSet::new();
    let mut invokers = BTreeSet::new();
    let mut generations = BTreeSet::new();
    let mut storage_keys = BTreeSet::new();
    let mut avatars = BTreeSet::new();
    let mut timers = 0;
    let mut storage_bytes = 0usize;
    for _ in 0..count {
        let id = InstanceId(reader.u64()?);
        let plugin = PluginId(reader.u32()?);
        let object = reader.u32()?;
        let actor = ActorId(reader.u64()?);
        let invoker = InvokerId(reader.u32()?);
        let generation = reader.u64()?;
        let last_activity = reader.u64()?;
        let next_sequence = reader.u64()?;
        let rate_tick = reader.u64()?;
        let messages_this_tick = reader.u32()?;
        if reader.u16()? != 1 {
            return Err(Error::UnsupportedPluginSchema);
        }
        let payload = reader.blob(limits.max_plugin_data_bytes + 128)?;
        let handler = decode_handler(plugin, payload)?;
        if matches!(handler, HandlerState::Timer(_)) {
            timers += 1;
        }
        if let Some(avatar_object) = handler.avatar_object()
            && !avatars.insert(avatar_object)
        {
            return Err(Error::InvalidCheckpoint);
        }
        let persistence = if reader.boolean()? {
            let status = match reader.u8()? {
                0 => BindingStatus::Loading,
                1 => BindingStatus::Reconcile,
                _ => return Err(Error::InvalidCheckpoint),
            };
            let key = PluginDataKey {
                scope: identity.scope,
                plugin,
                persistent_object: reader.u32()?,
            };
            let revision = reader.u64()?;
            let exists = reader.boolean()?;
            let data = reader.blob(limits.max_plugin_data_bytes)?;
            storage_bytes = storage_bytes
                .checked_add(data.len())
                .ok_or(Error::InvalidCheckpoint)?;
            if storage_bytes > limits.max_total_persistence_bytes
                || key.persistent_object == 0
                || !storage_keys.insert(key)
                || (exists && revision == 0)
                || (!exists && (revision != 0 || !data.is_empty()))
                || (status == BindingStatus::Loading
                    && (exists || revision != 0 || !data.is_empty()))
            {
                return Err(Error::InvalidCheckpoint);
            }
            Some(Binding {
                key,
                revision,
                exists,
                bytes: data.to_vec(),
                status,
            })
        } else {
            None
        };
        let loaded = match &handler {
            HandlerState::Signs(state) => Some(state.is_loaded()),
            HandlerState::Scoreboard(state) => Some(state.is_loaded()),
            HandlerState::PermissionDoor(state) => Some(state.is_loaded()),
            _ => None,
        };
        match (loaded, &persistence) {
            (Some(loaded), Some(binding))
                if loaded == (binding.status != BindingStatus::Loading) => {}
            (None, None) => {}
            _ => return Err(Error::InvalidCheckpoint),
        }
        if id.0 == 0
            || id.0 >= next_instance
            || object == 0
            || actor.0 == 0
            || invoker.0 == 0
            || generation == 0
            || generation >= next_session
            || next_sequence == 0
            || last_activity > tick
            || rate_tick > tick
            || messages_this_tick > limits.max_messages_per_tick
            || last_activity
                .checked_add(idle_timeout)
                .is_none_or(|deadline| deadline <= tick)
            || !actors.insert(actor)
            || !invokers.insert(invoker)
            || !generations.insert(generation)
            || instances.contains_key(&id)
            || timers > limits.max_timers
        {
            return Err(Error::InvalidCheckpoint);
        }
        instances.insert(
            id,
            Instance {
                plugin,
                object,
                handler,
                persistence,
                participant: Participant {
                    actor,
                    connection: None,
                    invoker,
                    generation,
                    last_activity,
                    next_sequence,
                    rate_tick,
                    messages_this_tick,
                },
            },
        );
    }
    let controller_count = reader.u32()? as usize;
    if controller_count > limits.max_instances.saturating_sub(count)
        || controller_count > limits.max_participants.saturating_sub(count)
    {
        return Err(Error::InvalidCheckpoint);
    }
    let mut controllers = BTreeMap::new();
    let mut floors = BTreeSet::new();
    for _ in 0..controller_count {
        let id = InstanceId(reader.u64()?);
        let object = reader.u32()?;
        let invoker = InvokerId(reader.u32()?);
        if id.0 == 0
            || id.0 >= next_instance
            || object == 0
            || invoker.0 == 0
            || instances.contains_key(&id)
            || controllers.contains_key(&id)
            || !floors.insert(object)
            || !invokers.insert(invoker)
        {
            return Err(Error::InvalidCheckpoint);
        }
        controllers.insert(
            id,
            DanceController {
                object,
                invoker,
                attached: false,
            },
        );
    }
    let group_count = if format == 3 {
        reader.u32()? as usize
    } else {
        0
    };
    if (format == 3 && group_count == 0)
        || group_count
            > limits
                .max_instances
                .saturating_sub(count + controller_count)
        || group_count
            > limits
                .max_participants
                .saturating_sub(count + controller_count)
        || group_count > limits.max_timers.saturating_sub(timers)
    {
        return Err(Error::InvalidCheckpoint);
    }
    let mut games = BTreeMap::new();
    let mut game_objects = BTreeSet::new();
    let mut game_seats = BTreeSet::new();
    for _ in 0..group_count {
        let id = InstanceId(reader.u64()?);
        let plugin = PluginId(reader.u32()?);
        let object = reader.u32()?;
        let invoker = InvokerId(reader.u32()?);
        if reader.u16()? != 1 {
            return Err(Error::UnsupportedPluginSchema);
        }
        let mut seats = [None; 4];
        for seat in &mut seats {
            let value = reader.u64()?;
            *seat = (value != 0).then_some(InstanceId(value));
        }
        let payload = reader.blob(MAX_GAME_BYTES)?;
        let handler = GameState::restore_private(plugin, payload)?;
        if id.0 == 0
            || id.0 >= next_instance
            || object == 0
            || invoker.0 == 0
            || instances.contains_key(&id)
            || controllers.contains_key(&id)
            || games.contains_key(&id)
            || !invokers.insert(invoker)
            || !game_objects.insert(object)
            || seats[handler.seats()..].iter().any(Option::is_some)
        {
            return Err(Error::InvalidCheckpoint);
        }
        let mut roster = [0; 4];
        for (slot, seat) in seats.iter().enumerate() {
            if let Some(seat) = seat {
                let instance = instances.get(seat).ok_or(Error::InvalidCheckpoint)?;
                let HandlerState::GameParticipant {
                    game,
                    slot: recorded_slot,
                    avatar_object,
                } = instance.handler
                else {
                    return Err(Error::InvalidCheckpoint);
                };
                if game != id
                    || usize::from(recorded_slot) != slot
                    || instance.plugin != plugin
                    || instance.object != object
                    || !game_seats.insert(*seat)
                {
                    return Err(Error::InvalidCheckpoint);
                }
                roster[slot] = avatar_object;
            }
        }
        if !handler.validate_roster(&roster) {
            return Err(Error::InvalidCheckpoint);
        }
        games.insert(
            id,
            SharedGame {
                object,
                invoker,
                attached: false,
                seats,
                handler,
            },
        );
    }
    for (id, instance) in &instances {
        if matches!(instance.handler, HandlerState::GameParticipant { .. })
            && !game_seats.contains(id)
        {
            return Err(Error::InvalidCheckpoint);
        }
    }
    let next_write = reader.u64()?;
    let write_count = reader.u32()? as usize;
    if next_write == 0
        || write_count
            > limits
                .max_persistence_records
                .saturating_sub(storage_keys.len())
    {
        return Err(Error::InvalidCheckpoint);
    }
    let mut writes = BTreeMap::new();
    let mut operations = BTreeSet::new();
    let mut pending_keys = BTreeSet::new();
    for _ in 0..write_count {
        let origin_epoch = reader.u64()?;
        let instance = InstanceId(reader.u64()?);
        let operation = reader.u64()?;
        let plugin = PluginId(reader.u32()?);
        let key = PluginDataKey {
            scope: identity.scope,
            plugin,
            persistent_object: reader.u32()?,
        };
        let actor = ActorId(reader.u64()?);
        let expected_revision = reader.u64()?;
        let conflicted = reader.boolean()?;
        let data = reader.blob(limits.max_plugin_data_bytes)?;
        storage_bytes = storage_bytes
            .checked_add(data.len())
            .ok_or(Error::InvalidCheckpoint)?;
        if storage_bytes > limits.max_total_persistence_bytes
            || origin_epoch == 0
            || origin_epoch > stamp.epoch
            || instance.0 == 0
            || instance.0 >= next_instance
            || operation == 0
            || operation >= next_write
            || !operations.insert(operation)
            || !pending_keys.insert(key)
            || key.persistent_object == 0
            || actor.0 == 0
            || expected_revision == u64::MAX
            || controllers.contains_key(&instance)
            || games.contains_key(&instance)
            || !matches!(
                plugin,
                registry::SIGNS_PLUGIN
                    | registry::SCOREBOARD_PLUGIN
                    | registry::PERMISSION_DOOR_PLUGIN
            )
        {
            return Err(Error::InvalidCheckpoint);
        }
        if let Some(current) = instances.get(&instance) {
            let binding = current
                .persistence
                .as_ref()
                .ok_or(Error::InvalidCheckpoint)?;
            if binding.key != key
                || binding.revision != expected_revision
                || binding.status == BindingStatus::Loading
                || current.participant.actor != actor
            {
                return Err(Error::InvalidCheckpoint);
            }
        } else if storage_keys.contains(&key) {
            return Err(Error::InvalidCheckpoint);
        }
        validate_write_bytes(plugin, data)?;
        let id = PluginWriteId {
            key,
            origin_epoch,
            instance,
            operation,
        };
        writes.insert(
            id,
            WriteRecord {
                request: PluginDataWrite {
                    id,
                    actor,
                    expected_revision,
                    bytes: data.to_vec(),
                },
                prepared: true,
                conflicted,
            },
        );
    }
    if reader.position != bytes.len() {
        return Err(Error::InvalidCheckpoint);
    }
    // A provider receipt must never reconcile bytes that disagree with the
    // private handler state reconstructed for the VM/UI. Source redaction and
    // the Door Edit cached-code quirk are validated by each handler explicitly.
    for (id, instance) in &instances {
        let Some(binding) = &instance.persistence else {
            continue;
        };
        if binding.status == BindingStatus::Loading {
            continue;
        }
        let pending = writes
            .values()
            .find(|record| record.request.id.instance == *id);
        let bytes = if let Some(record) = pending {
            Some(record.request.bytes.as_slice())
        } else {
            binding.exists.then_some(binding.bytes.as_slice())
        };
        let valid = match &instance.handler {
            HandlerState::Signs(state) => {
                state.matches_persistence(bytes, pending.is_some())
                    && pending.is_none_or(|record| {
                        state.preserves_write_permissions(
                            binding.exists.then_some(binding.bytes.as_slice()),
                            &record.request.bytes,
                        )
                    })
            }
            HandlerState::Scoreboard(state) => state.matches_persistence(bytes, pending.is_some()),
            HandlerState::PermissionDoor(state) => {
                state.matches_persistence(bytes, pending.is_some())
            }
            _ => false,
        };
        if !valid {
            return Err(Error::InvalidCheckpoint);
        }
    }
    let mut host = NativeHost::new(identity, limits)?;
    host.commit(State {
        tick,
        next_instance,
        next_session,
        checkpoint_revision: stamp.revision,
        next_write,
        instances,
        controllers,
        games,
        native_groups: BTreeMap::new(),
        native_operations: BTreeMap::new(),
        next_native_operation: 1,
        native_commands: vec![],
        writes,
        public: vec![],
        private: vec![],
    })?;
    Ok(host)
}

fn validate_write_bytes(plugin: PluginId, data: &[u8]) -> Result<(), Error> {
    let valid = match plugin {
        registry::SIGNS_PLUGIN => Signs::canonical_write(data),
        registry::SCOREBOARD_PLUGIN => Scoreboard::canonical_write(data),
        registry::PERMISSION_DOOR_PLUGIN => {
            let code = crate::source_plugins::parse_source_u32(data)
                .map_err(|_| Error::InvalidCheckpoint)?;
            if code > 999_999_999 || code.to_string().as_bytes() != data {
                return Err(Error::InvalidCheckpoint);
            }
            return Ok(());
        }
        _ => return Err(Error::InvalidCheckpoint),
    };
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidCheckpoint)
    }
}

struct Writer(Vec<u8>);
impl Writer {
    fn blob(&mut self, bytes: &[u8]) {
        self.u32(bytes.len() as u32);
        self.0.extend_from_slice(bytes);
    }
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }
    fn u16(&mut self, value: u16) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn i16(&mut self, value: i16) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn i32(&mut self, value: i32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Reader<'a> {
    fn blob(&mut self, limit: usize) -> Result<&'a [u8], Error> {
        let length = self.u32()? as usize;
        if length > limit {
            return Err(Error::InvalidCheckpoint);
        }
        self.take(length)
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let end = self
            .position
            .checked_add(count)
            .ok_or(Error::InvalidCheckpoint)?;
        let result = self
            .bytes
            .get(self.position..end)
            .ok_or(Error::InvalidCheckpoint)?;
        self.position = end;
        Ok(result)
    }
    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    fn boolean(&mut self) -> Result<bool, Error> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::InvalidCheckpoint),
        }
    }
    fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| Error::InvalidCheckpoint)?,
        ))
    }
    fn i16(&mut self) -> Result<i16, Error> {
        Ok(i16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| Error::InvalidCheckpoint)?,
        ))
    }
    fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| Error::InvalidCheckpoint)?,
        ))
    }
    fn i32(&mut self) -> Result<i32, Error> {
        Ok(i32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| Error::InvalidCheckpoint)?,
        ))
    }
    fn u64(&mut self) -> Result<u64, Error> {
        Ok(u64::from_le_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| Error::InvalidCheckpoint)?,
        ))
    }
}
