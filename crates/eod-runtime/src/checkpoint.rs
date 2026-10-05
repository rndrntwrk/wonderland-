// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Private persistence boundary. A store must authenticate and protect these
//! bytes, write atomically, and retain a trusted latest stamp to prevent rollback.
//! There is deliberately no generic checkpoint byte getter or Debug dump.

use crate::{
    host::{Instance, Participant, State},
    registry::TIMER_PLUGIN,
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
        if !self.state.public.is_empty() || !self.state.private.is_empty() {
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
            writer.i16(instance.timer.minutes);
            writer.i16(instance.timer.seconds);
            writer.u8(u8::from(instance.timer.running));
            writer.u8(instance.timer.mode);
            writer.u8(u8::from(instance.timer.updated_after_stop));
            writer.i32(instance.timer.tock);
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

fn decode(
    bytes: &[u8],
    identity: HostIdentity,
    expected: CheckpointStamp,
    limits: HostLimits,
) -> Result<NativeHost, Error> {
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
                timer,
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
            instances,
            public: vec![],
            private: vec![],
        },
    })
}

struct Writer(Vec<u8>);
impl Writer {
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
