// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Version 4 private native-family envelope. The complete legacy format 2/3 host
//! remains an exact bounded inner record; no transport incarnation is serialized.
use crate::{
    host::{HandlerState, Instance, Participant, State},
    native_provider::*,
    plugins::{
        FamilyState, NativeGroup, ProviderOperation,
        codec::{Reader, Writer},
        common::*,
    },
    *,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn validate_state(state: &State, limits: &HostLimits) -> Result<(), Error> {
    let count = state.instances.len()
        + state.controllers.len()
        + state.games.len()
        + state.native_groups.len();
    if count > limits.max_instances {
        return Err(Error::InstanceLimit);
    }
    if count > limits.max_participants {
        return Err(Error::ParticipantLimit);
    }
    let timers = state
        .instances
        .values()
        .filter(|i| matches!(i.handler, HandlerState::Timer(_)))
        .count()
        + state.games.len()
        + state.native_groups.len();
    if timers > limits.max_timers {
        return Err(Error::TimerLimit);
    }
    if state.native_operations.len()
        + state.writes.len()
        + state
            .instances
            .values()
            .filter(|i| i.persistence.is_some())
            .count()
        > limits.max_persistence_records
    {
        return Err(Error::PersistenceLimit);
    }
    if state.next_native_operation == 0 {
        return Err(Error::InvalidCheckpoint);
    }
    let mut total = 0usize;
    let mut reserved = BTreeMap::new();
    for (id, group) in &state.native_groups {
        for avatar in group.handler.reserved_avatars() {
            if avatar == 0 || reserved.insert(avatar, *id).is_some() {
                return Err(Error::InvalidCheckpoint);
            }
            if state.native_groups.iter().any(|(other, g)| {
                other != id && g.roster.iter().flatten().any(|m| m.avatar_id == avatar)
            }) {
                return Err(Error::InvalidCheckpoint);
            }
        }
    }
    let peers: Vec<_> = state
        .native_groups
        .iter()
        .map(|(id, g)| PeerGroup {
            group: *id,
            plugin: g.handler.plugin(),
            object: g.object,
            cluster: g.cluster,
            closing: g.closing,
            roster: g.roster,
        })
        .collect();
    for (id, group) in &state.native_groups {
        if group.object == 0
            || group.cluster == 0
            || group.invoker.0 == 0
            || id.0 == 0
            || id.0 >= state.next_instance
        {
            return Err(Error::InvalidCheckpoint);
        }
        if group
            .handler
            .trusted_object()
            .is_some_and(|object| object != group.object)
            || !group.handler.validate(&group.roster)
        {
            return Err(Error::InvalidCheckpoint);
        }
        if !group.handler.validate_peers(*id, group.cluster, &peers) {
            return Err(Error::InvalidCheckpoint);
        }
        let expected = group.handler.pending_operations();
        let actual: Vec<_> = state
            .native_operations
            .values()
            .filter(|r| r.request.id.group == *id)
            .map(|r| (r.request.callback, r.request.operation.clone()))
            .collect();
        let expected_map: BTreeMap<_, _> = expected.iter().cloned().collect();
        let actual_map: BTreeMap<_, _> = actual.iter().cloned().collect();
        if expected_map.len() != expected.len()
            || actual_map.len() != actual.len()
            || expected_map != actual_map
        {
            return Err(Error::InvalidCheckpoint);
        }
        let size = group.handler.save_private()?.len();
        if size > limits.max_plugin_data_bytes {
            return Err(Error::PersistenceTooLarge);
        }
        total = total.checked_add(size).ok_or(Error::PersistenceLimit)?;
        for seat in 0..MAX_MEMBERS {
            match (group.members[seat], group.roster[seat]) {
                (None, None) => {}
                (Some(mid), Some(member)) => {
                    let instance = state.instances.get(&mid).ok_or(Error::InvalidCheckpoint)?;
                    if member.seat as usize != seat
                        || member.avatar_object <= 0
                        || member.avatar_id == 0
                        || member.actor != instance.participant.actor
                        || instance.object != group.object
                        || instance.plugin != group.handler.plugin()
                        || !matches!(instance.handler,HandlerState::NativeParticipant{group:g,seat:s,avatar_object:a} if g==*id&&s as usize==seat&&a==member.avatar_object)
                    {
                        return Err(Error::InvalidCheckpoint);
                    }
                }
                _ => return Err(Error::InvalidCheckpoint),
            }
        }
        if group.closing && group.members.iter().any(Option::is_some) {
            return Err(Error::InvalidCheckpoint);
        }
    }
    let mut callbacks = BTreeSet::new();
    for (id, record) in &state.native_operations {
        let request = &record.request;
        let group = state
            .native_groups
            .get(&id.group)
            .ok_or(Error::InvalidCheckpoint)?;
        if *id != request.id
            || id.operation == 0
            || id.operation >= state.next_native_operation
            || id.origin_epoch == 0
            || request.callback == 0
            || request.plugin != group.handler.plugin()
            || request.object != group.object
            || !callbacks.insert((id.group, request.callback))
        {
            return Err(Error::InvalidCheckpoint);
        }
        let same_family = matches!(
            (&group.handler, &request.operation),
            (FamilyState::Casino(_), ProviderOperation::Casino(_))
                | (FamilyState::Social(_), ProviderOperation::Social(_))
                | (FamilyState::Service(_), ProviderOperation::Service(_))
        );
        if !same_family {
            return Err(Error::InvalidCheckpoint);
        }
        let size = request.operation.private_bytes()?.len();
        if size > limits.max_plugin_data_bytes {
            return Err(Error::PersistenceTooLarge);
        }
        total = total.checked_add(size).ok_or(Error::PersistenceLimit)?;
    }
    for binding in state
        .instances
        .values()
        .filter_map(|i| i.persistence.as_ref())
    {
        total = total
            .checked_add(binding.bytes.len())
            .ok_or(Error::PersistenceLimit)?;
    }
    for record in state.writes.values() {
        total = total
            .checked_add(record.request.bytes.len())
            .ok_or(Error::PersistenceLimit)?;
    }
    if total > limits.max_total_persistence_bytes {
        return Err(Error::PersistenceLimit);
    }
    let mut command_bytes = 0usize;
    for command in &state.native_commands {
        if !command.command.valid() {
            return Err(Error::InvalidPluginInput);
        }
        command_bytes = command_bytes
            .checked_add(match &command.command {
                NativeCommand::BatchGraphics { objects, graphics } => {
                    objects.len() * 2 + graphics.len()
                }
                _ => 32,
            })
            .ok_or(Error::QueueFull)?;
    }
    if command_bytes > limits.max_private_output_bytes {
        return Err(Error::QueueFull);
    }
    Ok(())
}
pub(crate) fn encode(host: &NativeHost, stamp: CheckpointStamp) -> Result<Vec<u8>, Error> {
    validate_state(&host.state, &host.limits)?;
    let mut legacy = host.state.clone();
    legacy
        .instances
        .retain(|_, i| !matches!(i.handler, HandlerState::NativeParticipant { .. }));
    legacy.native_groups.clear();
    legacy.native_operations.clear();
    legacy.native_commands.clear();
    legacy.next_native_operation = 1;
    let legacy_host = NativeHost {
        identity: host.identity,
        limits: host.limits.clone(),
        state: legacy,
    };
    let inner = crate::checkpoint::encode_v2(&legacy_host, stamp)?;
    let mut w = Writer::default();
    w.fixed(b"EODP");
    w.u16(4);
    w.u16(1);
    w.bytes(&inner);
    let members: Vec<_> = host
        .state
        .instances
        .iter()
        .filter(|(_, i)| matches!(i.handler, HandlerState::NativeParticipant { .. }))
        .collect();
    w.u32(members.len() as u32);
    for (id, instance) in members {
        let HandlerState::NativeParticipant {
            group,
            seat,
            avatar_object,
        } = instance.handler
        else {
            unreachable!()
        };
        let member = host
            .state
            .native_groups
            .get(&group)
            .and_then(|g| g.roster[usize::from(seat)])
            .ok_or(Error::InvalidCheckpoint)?;
        let p = &instance.participant;
        w.u64(id.0);
        w.u32(instance.plugin.0);
        w.u32(instance.object);
        w.u64(p.actor.0);
        w.u32(p.invoker.0);
        w.u64(p.generation);
        w.u64(p.last_activity);
        w.u64(p.next_sequence);
        w.u64(p.rate_tick);
        w.u32(p.messages_this_tick);
        w.u64(group.0);
        w.u8(seat);
        w.i16(avatar_object);
        w.u32(member.avatar_id);
        write_member_input(&mut w, member.input);
    }
    w.u32(host.state.native_groups.len() as u32);
    for (id, g) in &host.state.native_groups {
        w.u64(id.0);
        w.u32(g.handler.plugin().0);
        w.u32(g.object);
        w.u64(g.cluster);
        w.u32(g.invoker.0);
        w.bool(g.closing);
        for member in g.members {
            w.u64(member.map_or(0, |id| id.0));
        }
        w.bytes(&g.handler.save_private()?);
        if w.0.len() > host.limits.max_checkpoint_bytes {
            return Err(Error::CheckpointTooLarge);
        }
    }
    w.u64(host.state.next_native_operation);
    w.u32(host.state.native_operations.len() as u32);
    for (id, record) in &host.state.native_operations {
        w.u64(id.scope.0);
        w.u64(id.origin_epoch);
        w.u64(id.group.0);
        w.u64(id.operation);
        w.u32(record.request.plugin.0);
        w.u32(record.request.object);
        w.u64(record.request.callback);
        w.bytes(&record.request.operation.private_bytes()?);
        if w.0.len() > host.limits.max_checkpoint_bytes {
            return Err(Error::CheckpointTooLarge);
        }
    }
    if w.0.len() > host.limits.max_checkpoint_bytes {
        return Err(Error::CheckpointTooLarge);
    }
    Ok(w.0)
}
pub(crate) fn decode(
    bytes: &[u8],
    identity: HostIdentity,
    stamp: CheckpointStamp,
    limits: HostLimits,
) -> Result<NativeHost, Error> {
    let mut r = Reader::new(bytes);
    if r.take(4)? != b"EODP" || r.u16()? != 4 {
        return Err(Error::UnsupportedCheckpointVersion);
    }
    if r.u16()? != 1 {
        return Err(Error::UnsupportedPluginSchema);
    }
    let inner = r.bytes(limits.max_checkpoint_bytes)?;
    if !matches!(inner.get(..6), Some(b"EODP\x02\x00" | b"EODP\x03\x00")) {
        return Err(Error::InvalidCheckpoint);
    }
    let mut host = crate::checkpoint::decode(&inner, identity, stamp, limits.clone())?;
    let state = &mut host.state;
    let mut identities: BTreeSet<_> = state
        .instances
        .keys()
        .chain(state.controllers.keys())
        .chain(state.games.keys())
        .copied()
        .collect();
    let mut actors: BTreeSet<_> = state
        .instances
        .values()
        .map(|i| i.participant.actor)
        .collect();
    let mut invokers: BTreeSet<_> = state
        .instances
        .values()
        .map(|i| i.participant.invoker)
        .chain(state.controllers.values().map(|i| i.invoker))
        .chain(state.games.values().map(|i| i.invoker))
        .collect();
    let mut generations: BTreeSet<_> = state
        .instances
        .values()
        .map(|i| i.participant.generation)
        .collect();
    let mut avatars: BTreeSet<_> = state
        .instances
        .values()
        .filter_map(|i| i.handler.avatar_object())
        .collect();
    let mut persist_avatars = BTreeSet::new();
    let mut member_data = BTreeMap::new();
    let count = r.count(
        limits
            .max_participants
            .saturating_sub(state.instances.len()),
    )?;
    for _ in 0..count {
        let id = InstanceId(r.u64()?);
        let plugin = PluginId(r.u32()?);
        let object = r.u32()?;
        let actor = ActorId(r.u64()?);
        let invoker = InvokerId(r.u32()?);
        let generation = r.u64()?;
        let last_activity = r.u64()?;
        let next_sequence = r.u64()?;
        let rate_tick = r.u64()?;
        let messages_this_tick = r.u32()?;
        let group = InstanceId(r.u64()?);
        let seat = r.u8()?;
        let avatar_object = r.i16()?;
        let avatar_id = r.u32()?;
        let input = read_member_input(&mut r)?;
        if id.0 == 0
            || id.0 >= state.next_instance
            || object == 0
            || actor.0 == 0
            || invoker.0 == 0
            || generation == 0
            || generation >= state.next_session
            || next_sequence == 0
            || last_activity > state.tick
            || rate_tick > state.tick
            || messages_this_tick > limits.max_messages_per_tick
            || seat as usize >= MAX_MEMBERS
            || group.0 == 0
            || group.0 >= state.next_instance
            || avatar_object <= 0
            || avatar_id == 0
            || !identities.insert(id)
            || !actors.insert(actor)
            || !invokers.insert(invoker)
            || !generations.insert(generation)
            || !avatars.insert(avatar_object)
            || !persist_avatars.insert(avatar_id)
        {
            return Err(Error::InvalidCheckpoint);
        }
        last_activity
            .checked_add(limits.idle_timeout_ticks)
            .ok_or(Error::InvalidCheckpoint)?;
        member_data.insert(
            id,
            Member {
                seat,
                actor,
                avatar_object,
                avatar_id,
                input,
            },
        );
        state.instances.insert(
            id,
            Instance {
                plugin,
                object,
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
                handler: HandlerState::NativeParticipant {
                    group,
                    seat,
                    avatar_object,
                },
                persistence: None,
            },
        );
    }
    let count = r.count(limits.max_instances.saturating_sub(identities.len()))?;
    if count == 0 {
        return Err(Error::InvalidCheckpoint);
    }
    let mut used_members = BTreeSet::new();
    let mut group_objects = BTreeSet::new();
    for _ in 0..count {
        let id = InstanceId(r.u64()?);
        let plugin = PluginId(r.u32()?);
        let object = r.u32()?;
        let cluster = r.u64()?;
        let invoker = InvokerId(r.u32()?);
        let closing = r.bool()?;
        if id.0 == 0
            || id.0 >= state.next_instance
            || object == 0
            || cluster == 0
            || invoker.0 == 0
            || !identities.insert(id)
            || !invokers.insert(invoker)
            || !group_objects.insert((plugin, object))
        {
            return Err(Error::InvalidCheckpoint);
        }
        let mut members = [None; MAX_MEMBERS];
        let mut roster = [None; MAX_MEMBERS];
        for seat in 0..MAX_MEMBERS {
            let mid = InstanceId(r.u64()?);
            if mid.0 != 0 {
                let member = *member_data.get(&mid).ok_or(Error::InvalidCheckpoint)?;
                let instance = state.instances.get(&mid).ok_or(Error::InvalidCheckpoint)?;
                if member.seat as usize != seat
                    || !used_members.insert(mid)
                    || instance.object != object
                    || instance.plugin != plugin
                    || !matches!(instance.handler,HandlerState::NativeParticipant{group,..} if group==id)
                {
                    return Err(Error::InvalidCheckpoint);
                }
                members[seat] = Some(mid);
                roster[seat] = Some(member);
            }
        }
        let payload = r.bytes(limits.max_plugin_data_bytes.min(MAX_STATE_BYTES))?;
        let handler = FamilyState::restore_private(plugin, &payload)?;
        state.native_groups.insert(
            id,
            NativeGroup {
                object,
                cluster,
                invoker,
                attached: false,
                closing,
                members,
                roster,
                handler,
            },
        );
    }
    if used_members.len() != member_data.len() {
        return Err(Error::InvalidCheckpoint);
    }
    state.next_native_operation = r.u64()?;
    if state.next_native_operation == 0 {
        return Err(Error::InvalidCheckpoint);
    }
    let count = r.count(limits.max_persistence_records)?;
    let mut operations = BTreeSet::new();
    for _ in 0..count {
        let id = NativeOperationId {
            scope: HostScopeId(r.u64()?),
            origin_epoch: r.u64()?,
            group: InstanceId(r.u64()?),
            operation: r.u64()?,
        };
        let plugin = PluginId(r.u32()?);
        let object = r.u32()?;
        let callback = r.u64()?;
        if id.scope != identity.scope
            || id.origin_epoch == 0
            || id.origin_epoch > stamp.epoch
            || id.operation == 0
            || id.operation >= state.next_native_operation
            || !operations.insert(id.operation)
        {
            return Err(Error::InvalidCheckpoint);
        }
        let opbytes = r.bytes(limits.max_plugin_data_bytes.min(MAX_OPERATION_BYTES))?;
        let mut or = Reader::new(&opbytes);
        let operation = ProviderOperation::restore(&mut or)?;
        or.finish()?;
        if state
            .native_operations
            .insert(
                id,
                NativeOperationRecord {
                    request: NativeProviderRequest {
                        id,
                        plugin,
                        object,
                        callback,
                        operation,
                    },
                    prepared: true,
                },
            )
            .is_some()
        {
            return Err(Error::InvalidCheckpoint);
        }
    }
    r.finish()?;
    validate_state(state, &limits)?;
    Ok(host)
}
fn write_member_input(w: &mut Writer, v: MemberInput) {
    w.u8(v.role);
    for r in v.registers {
        w.i16(r)
    }
    for r in v.skills {
        w.i16(r)
    }
    w.bool(v.owner_authorized);
    w.u8(v.gender);
    w.u8(v.skin);
}
fn read_member_input(r: &mut Reader<'_>) -> Result<MemberInput, Error> {
    let role = r.u8()?;
    let mut registers = [0; 16];
    for v in &mut registers {
        *v = r.i16()?
    }
    let mut skills = [0; 6];
    for v in &mut skills {
        *v = r.i16()?
    }
    Ok(MemberInput {
        role,
        registers,
        skills,
        owner_authorized: r.bool()?,
        gender: r.u8()?,
        skin: r.u8()?,
    })
}
