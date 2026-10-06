//! Chat-only projection of FreeSO 4c6b3e8 commands. This never advances a VM.
//!
//! VMNetChatCmd supplies speech; snapshot avatar.Message is an existing bubble,
//! not chat history. VMNetSimLeaveCmd only starts a departure, so it cannot
//! produce a completed Leave event without executing the source simulation.
//!
//! Contract: admitted native frames from a connected TSO server whose ordinary
//! ticks advance are authoritative. StateSync establishes the state immediately
//! before its wrapper tick; cached snapshots replay roles without replaying chat.
//! Direct commands have no source sequence. Sender admission belongs to the
//! source server's Verify phase; viewer eligibility and ignores remain checked
//! here. Unknown identities, decode failures, and exhausted quotas hide chat
//! until a complete snapshot restores authority. This observer does not execute
//! avatar behavior, server slash commands, or paused sandbox/TS1 simulation.
use crate::{
    snapshot::{ChatChannel, EntityPlatform},
    Command, CommandBody, DecodeLimits, Error, ErrorKind, Result, Snapshot, TickList,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

// Source names are repeated in projected events. Bound that amplification,
// including fixed JSON fields, before cloning attribution into each event.
const MAX_PROJECTED_EVENT_BYTES: usize = 512 * 1024;

/// Transport resource quotas, not limits on a world's residents or player history.
/// Exceeding either discards authority until a complete source snapshot arrives.
#[derive(Clone, Debug)]
pub struct ChatStateLimits {
    pub max_entries: usize,
    pub max_text_bytes: usize,
}
impl Default for ChatStateLimits {
    fn default() -> Self {
        let limits = DecodeLimits::default();
        Self {
            max_entries: limits.max_total_entries,
            max_text_bytes: limits.max_total_string_bytes,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisibleChatChannel {
    pub id: u8,
    pub name: String,
    pub description: String,
    pub private: bool,
    pub show_by_default: bool,
    pub text_color: [u8; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceChatKind {
    Message,
    Join,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceChatMessage {
    pub kind: SourceChatKind,
    pub sender_uid: u32,
    pub sender_incarnation: u64,
    pub sender_name: String,
    pub sender_color: [u8; 3],
    pub channel_id: Option<u8>,
    pub channel_name: Option<String>,
    pub private: bool,
    pub text: String,
    pub tick_id: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatProjection {
    pub viewer_id: u32,
    pub viewer_incarnation: u64,
    pub lot_location: u32,
    pub ready: bool,
    pub channels: Vec<VisibleChatChannel>,
    pub messages: Vec<SourceChatMessage>,
}

/// The gateway assigns a sequence to each projection it sends. Original direct
/// commands have no sequence, so equal text in two native deliveries is retained.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotChatDelivery {
    pub lot_incarnation: u64,
    pub sequence: u64,
    pub projection: ChatProjection,
}

#[derive(Clone, Debug)]
struct Actor {
    incarnation: u64,
    name: String,
    permissions: u8,
    ignored: BTreeSet<u32>,
    color: [u8; 3],
}

#[derive(Clone, Debug)]
pub struct SourceChatObserver {
    limits: ChatStateLimits,
    ignored_entries: usize,
    actor_name_bytes: usize,
    viewer_id: u32,
    lot_location: u32,
    actors: BTreeMap<u32, Actor>,
    incarnations: BTreeMap<u32, (u64, Option<i16>)>,
    next_incarnation: u64,
    viewer_incarnation: u64,
    departing: BTreeSet<u32>,
    channels: BTreeMap<u8, ChatChannel>,
    seeded: bool,
    state_tick: Option<u32>,
    event_tick: Option<u32>,
    owner_id: u32,
    community_lot: bool,
    roommates: BTreeSet<u32>,
    build_roommates: BTreeSet<u32>,
}

fn newer(candidate: u32, previous: u32) -> bool {
    let distance = candidate.wrapping_sub(previous);
    distance != 0 && distance < (1_u32 << 31)
}

impl SourceChatObserver {
    pub fn new(viewer_id: u32, lot_location: u32) -> Self {
        Self::with_state_limits(viewer_id, lot_location, ChatStateLimits::default())
    }

    pub fn with_state_limits(viewer_id: u32, lot_location: u32, limits: ChatStateLimits) -> Self {
        Self {
            limits,
            ignored_entries: 0,
            actor_name_bytes: 0,
            viewer_id,
            lot_location,
            actors: BTreeMap::new(),
            incarnations: BTreeMap::new(),
            next_incarnation: 0,
            viewer_incarnation: 0,
            departing: BTreeSet::new(),
            channels: BTreeMap::new(),
            seeded: false,
            state_tick: None,
            event_tick: None,
            owner_id: 0,
            community_lot: false,
            roommates: BTreeSet::new(),
            build_roommates: BTreeSet::new(),
        }
    }

    /// Called after undecodable source input. Prior roles cannot authorize a
    /// private message until another complete source snapshot restores context.
    pub fn invalidate(&mut self) {
        self.seeded = false;
        self.actors.clear();
        self.incarnations.clear();
        self.departing.clear();
        self.channels.clear();
        self.roommates.clear();
        self.build_roommates.clear();
        self.owner_id = 0;
        self.ignored_entries = 0;
        self.actor_name_bytes = 0;
        self.state_tick = None;
    }

    pub fn observe_frame(
        &mut self,
        direct: bool,
        bytes: &[u8],
        limits: &DecodeLimits,
    ) -> Result<ChatProjection> {
        let result = if direct {
            crate::decode_direct_command(bytes, limits)
                .and_then(|command| self.observe_direct(command))
        } else {
            crate::decode_tick_list(bytes, limits).and_then(|list| self.observe_tick_list(list))
        };
        if result.is_err() {
            self.invalidate();
        }
        result
    }

    pub fn observe_direct(&mut self, command: Command) -> Result<ChatProjection> {
        self.observe_batches(vec![(None, vec![command])], true)
    }

    pub fn observe_tick_list(&mut self, list: TickList) -> Result<ChatProjection> {
        let batches = list
            .ticks
            .into_iter()
            .map(|tick| {
                (
                    if list.immediate_mode {
                        None
                    } else {
                        Some(tick.tick_id)
                    },
                    tick.commands,
                )
            })
            .collect();
        self.observe_batches(batches, false)
    }

    fn observe_batches(
        &mut self,
        batches: Vec<(Option<u32>, Vec<Command>)>,
        direct: bool,
    ) -> Result<ChatProjection> {
        // Inspect every decoded snapshot before mutating or exposing a message.
        if batches.iter().flat_map(|(_, commands)| commands).any(|command| {
            matches!(&command.body, CommandBody::StateSync { snapshot, .. } if snapshot.platform.lot_id != self.lot_location)
        }) {
            self.invalidate();
            return Err(Error { offset: 0, kind: ErrorKind::Invalid, context: "chat snapshot belongs to another lot" });
        }
        let mut messages = Vec::new();
        let mut projected_bytes = 0;
        for (tick, commands) in batches {
            let ordinary = commands.is_empty()
                || commands
                    .iter()
                    .any(|command| !matches!(command.body, CommandBody::StateSync { .. }));
            let emit = tick.is_none_or(|tick| self.event_tick.is_none_or(|last| newer(tick, last)));
            let mut apply =
                tick.is_none_or(|tick| self.state_tick.is_none_or(|last| newer(tick, last)));
            for command in commands {
                if let CommandBody::StateSync { snapshot, .. } = command.body {
                    // VMServerDriver increments TickID before InternalTick and
                    // saves afterward. Its snapshot wrapper names the NEXT real
                    // tick; TicksSinceSync starts with that same ID.
                    let baseline = tick.map(|tick| tick.wrapping_sub(1));
                    let historical = baseline.is_some_and(|baseline| {
                        self.event_tick.is_some_and(|last| newer(last, baseline))
                    });
                    self.load_snapshot(&snapshot, historical);
                    self.state_tick = baseline;
                    if let Some(baseline) = baseline {
                        if self.event_tick.is_none_or(|last| newer(baseline, last)) {
                            self.event_tick = Some(baseline);
                        }
                    }
                    apply = true;
                } else if apply {
                    if let Err(error) = self.apply_command(
                        command,
                        tick,
                        direct,
                        emit,
                        &mut messages,
                        &mut projected_bytes,
                    ) {
                        self.invalidate();
                        return Err(error);
                    }
                }
                if !self.within_state_budget() {
                    self.invalidate();
                    return Err(Error {
                        offset: 0,
                        kind: ErrorKind::Limit,
                        context: "source chat retained state budget",
                    });
                }
            }
            if apply && ordinary {
                if let Some(tick) = tick {
                    self.state_tick = Some(tick);
                }
            }
            if emit && ordinary {
                if let Some(tick) = tick {
                    self.event_tick = Some(tick);
                }
            }
        }
        let mut projection = self.projection();
        if projection.ready {
            projection.messages = messages;
        }
        Ok(projection)
    }

    fn allocate_incarnation(&mut self, uid: u32, object_id: Option<i16>, force: bool) -> u64 {
        let old = self.incarnations.get(&uid).copied();
        let incarnation = match old {
            Some((incarnation, old_object))
                if !force
                    && (object_id.is_none() || old_object.is_none() || old_object == object_id) =>
            {
                incarnation
            }
            _ => {
                self.next_incarnation = self.next_incarnation.saturating_add(1);
                self.next_incarnation
            }
        };
        self.incarnations.insert(
            uid,
            (
                incarnation,
                object_id.or(old.and_then(|(_, object)| object)),
            ),
        );
        if uid == self.viewer_id {
            self.viewer_incarnation = incarnation;
        }
        incarnation
    }

    fn load_snapshot(&mut self, snapshot: &Snapshot, historical: bool) {
        if !historical {
            let present: BTreeSet<_> = snapshot
                .entities
                .iter()
                .filter(|entity| matches!(entity.platform, EntityPlatform::Avatar { .. }))
                .map(|entity| entity.persist_id)
                .collect();
            self.incarnations.retain(|uid, _| present.contains(uid));
            self.departing.retain(|uid| present.contains(uid));
        }
        // A snapshot may contain many avatars and group memberships. Index each
        // membership once instead of rescanning every group for every avatar.
        // Repeated membership in the same group is valid; different groups make
        // the original source name ambiguous and unusable for attribution.
        let mut groups_by_object: BTreeMap<i16, Option<usize>> = BTreeMap::new();
        for (group_index, group) in snapshot.multitile_groups.iter().enumerate() {
            for object_id in &group.objects {
                groups_by_object
                    .entry(*object_id)
                    .and_modify(|unique| {
                        if *unique != Some(group_index) {
                            *unique = None;
                        }
                    })
                    .or_insert(Some(group_index));
            }
        }
        let mut actors = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for entity in &snapshot.entities {
            let EntityPlatform::Avatar {
                permissions,
                ignored,
                chat_rgb,
                ..
            } = &entity.platform
            else {
                continue;
            };
            if entity.persist_id == 0 || *permissions > 4 {
                continue;
            }
            let Some(group_index) = groups_by_object.get(&entity.object_id).copied().flatten()
            else {
                actors.remove(&entity.persist_id);
                continue;
            };
            let name = &snapshot.multitile_groups[group_index].name;
            if !seen.insert(entity.persist_id) || name.is_empty() {
                // Ambiguous original identity is never attributed to a person.
                actors.remove(&entity.persist_id);
                continue;
            }
            let old_object = self
                .incarnations
                .get(&entity.persist_id)
                .and_then(|(_, object)| *object);
            if self.departing.contains(&entity.persist_id) && old_object == Some(entity.object_id) {
                continue;
            }
            let object_id = if historical {
                old_object
            } else {
                Some(entity.object_id)
            };
            let incarnation = self.allocate_incarnation(entity.persist_id, object_id, false);
            actors.insert(
                entity.persist_id,
                Actor {
                    incarnation,
                    name: name.clone(),
                    permissions: *permissions,
                    ignored: ignored.iter().copied().collect(),
                    color: *chat_rgb,
                },
            );
        }
        self.actors = actors;
        self.ignored_entries = self
            .actors
            .values()
            .fold(0usize, |sum, actor| sum.saturating_add(actor.ignored.len()));
        self.actor_name_bytes = self
            .actors
            .values()
            .fold(0usize, |sum, actor| sum.saturating_add(actor.name.len()));
        self.channels.clear();
        for channel in &snapshot.platform.chat_channels {
            if (1..=4).contains(&channel.id)
                && channel.minimum_view_permission <= 4
                && channel.minimum_send_permission <= 4
                && channel.flags & 128 == 0
            {
                self.channels
                    .entry(channel.id)
                    .or_insert_with(|| channel.clone());
            }
        }
        self.owner_id = snapshot.platform.owner_id;
        self.community_lot = snapshot.platform.category == 11;
        self.roommates = snapshot.platform.roommates.iter().copied().collect();
        self.build_roommates = snapshot.platform.build_roommates.iter().copied().collect();
        self.seeded = true;
    }

    fn caught_up(&self) -> bool {
        match (self.state_tick, self.event_tick) {
            (None, Some(_)) => false,
            (Some(state), Some(event)) => !newer(event, state),
            _ => true,
        }
    }

    fn ready(&self) -> bool {
        self.seeded
            && self.viewer_id != 0
            && self.actors.contains_key(&self.viewer_id)
            && self.caught_up()
    }

    fn channel(&self, id: u8) -> Option<ChatChannel> {
        match id {
            0 => Some(ChatChannel {
                id,
                name: "Chat".into(),
                description: "Default Channel".into(),
                minimum_view_permission: 0,
                minimum_send_permission: 0,
                flags: 2,
                text_color: u32::MAX,
            }),
            7 => Some(ChatChannel {
                id,
                name: "Admin".into(),
                description: "Administrators Only".into(),
                minimum_view_permission: 4,
                minimum_send_permission: 4,
                flags: 0,
                text_color: u32::MAX,
            }),
            _ => self.channels.get(&id).cloned(),
        }
    }

    pub fn projection(&self) -> ChatProjection {
        let ready = self.ready();
        let permissions = self
            .actors
            .get(&self.viewer_id)
            .map(|actor| actor.permissions)
            .unwrap_or(0);
        let channels = if ready {
            (0..=7)
                .filter_map(|id| self.channel(id))
                .filter(|channel| channel.minimum_view_permission <= permissions)
                .map(|channel| VisibleChatChannel {
                    id: channel.id,
                    name: channel.name,
                    description: channel.description,
                    private: channel.minimum_view_permission > 0,
                    show_by_default: channel.flags & 2 != 0,
                    text_color: [
                        channel.text_color as u8,
                        (channel.text_color >> 8) as u8,
                        (channel.text_color >> 16) as u8,
                    ],
                })
                .collect()
        } else {
            vec![]
        };
        ChatProjection {
            viewer_id: self.viewer_id,
            viewer_incarnation: self.viewer_incarnation,
            lot_location: self.lot_location,
            ready,
            channels,
            messages: vec![],
        }
    }

    fn apply_command(
        &mut self,
        command: Command,
        tick: Option<u32>,
        direct: bool,
        emit: bool,
        messages: &mut Vec<SourceChatMessage>,
        projected_bytes: &mut usize,
    ) -> Result<()> {
        let uid = command.actor_uid.unwrap_or(0);
        match command.body {
            CommandBody::Chat {
                message,
                channel_id,
            } => {
                if !emit || !self.ready() {
                    return Ok(());
                }
                let Some(sender) = self.actors.get(&uid) else {
                    return Ok(());
                };
                let Some(viewer) = self.actors.get(&self.viewer_id) else {
                    return Ok(());
                };
                let id = channel_id & 0x7f;
                let Some(channel) = self.channel(id) else {
                    return Ok(());
                };
                let private = channel.minimum_view_permission > 0;
                // VMNetChatCmd.Verify only emits private channels as direct,
                // high-bit commands to eligible viewers. Never infer a whisper.
                // The source verifies the whole tick before executing it, so a
                // prior command's sender demotion cannot revoke admitted speech.
                if viewer.ignored.contains(&uid)
                    || viewer.permissions < channel.minimum_view_permission
                    || (private && (!direct || channel_id & 0x80 == 0))
                    || (!private && channel_id & 0x80 != 0)
                {
                    return Ok(());
                }
                let message = source_substring(&message, 200);
                if message.is_empty()
                    || (message.starts_with('/') && message.encode_utf16().count() > 1)
                {
                    return Ok(());
                }
                charge_event(projected_bytes, &[&sender.name, &message, &channel.name])?;
                messages.push(SourceChatMessage {
                    kind: SourceChatKind::Message,
                    sender_uid: uid,
                    sender_incarnation: sender.incarnation,
                    sender_name: sender.name.clone(),
                    sender_color: sender.color,
                    channel_id: Some(id),
                    channel_name: Some(channel.name),
                    private,
                    text: message,
                    tick_id: tick,
                });
            }
            CommandBody::AvatarJoin(join)
                if self.seeded
                    && uid != 0
                    && uid == join.persist_id
                    && join.permissions <= 4
                    && !join.name.is_empty() =>
            {
                let mut permissions = join.permissions;
                if self.community_lot && permissions < 3 && self.roommates.contains(&uid) {
                    permissions = if self.build_roommates.contains(&uid) {
                        2
                    } else {
                        1
                    };
                }
                let incarnation = self.allocate_incarnation(uid, None, emit);
                self.departing.remove(&uid);
                if permissions > 0 && permissions < 4 {
                    self.roommates.insert(uid);
                    if permissions > 1 {
                        self.build_roommates.insert(uid);
                    } else {
                        self.build_roommates.remove(&uid);
                    }
                } else if permissions != 4 {
                    self.roommates.remove(&uid);
                    self.build_roommates.remove(&uid);
                }
                let sender = Actor {
                    incarnation,
                    name: join.name,
                    permissions,
                    ignored: join.ignored.into_iter().collect(),
                    color: [255; 3],
                };
                self.remove_actor(uid);
                self.ignored_entries = self.ignored_entries.saturating_add(sender.ignored.len());
                self.actor_name_bytes = self.actor_name_bytes.saturating_add(sender.name.len());
                self.actors.insert(uid, sender.clone());
                if emit
                    && self.ready()
                    && self
                        .actors
                        .get(&self.viewer_id)
                        .is_some_and(|viewer| !viewer.ignored.contains(&uid))
                {
                    charge_event(projected_bytes, &[&sender.name])?;
                    messages.push(SourceChatMessage {
                        kind: SourceChatKind::Join,
                        sender_uid: uid,
                        sender_incarnation: incarnation,
                        sender_name: sender.name,
                        sender_color: sender.color,
                        channel_id: None,
                        channel_name: None,
                        private: false,
                        text: String::new(),
                        tick_id: tick,
                    });
                }
            }
            CommandBody::ChangePermissions {
                target_uid,
                level,
                mode,
                ..
            } if level <= 4 && mode <= 3 => {
                if mode == 3 {
                    return Ok(());
                } // OBJECTS_ONLY leaves avatar permissions intact.
                if mode == 1 || mode == 2 {
                    self.set_permissions(self.owner_id, 0);
                    if self.community_lot {
                        self.roommates.clear();
                        self.build_roommates.clear();
                    }
                }
                self.set_permissions(target_uid, level);
            }
            CommandBody::SetIgnore { target_uid, ignore } => {
                if let Some(actor) = self.actors.get_mut(&uid) {
                    // Preserve the original 128-entry source behavior, including
                    // its remove branch when SetIgnore is true at capacity.
                    if ignore && actor.ignored.len() < 128 {
                        if actor.ignored.insert(target_uid) {
                            self.ignored_entries = self.ignored_entries.saturating_add(1);
                        }
                    } else if actor.ignored.remove(&target_uid) {
                        self.ignored_entries = self.ignored_entries.saturating_sub(1);
                    }
                }
            }
            CommandBody::ChatParameters { color, .. } => {
                if let Some(actor) = self.actors.get_mut(&uid) {
                    actor.color = [color as u8, (color >> 8) as u8, (color >> 16) as u8];
                }
            }
            CommandBody::ChatEditChannel(mut channel)
                if (1..=4).contains(&channel.id)
                    && channel.minimum_view_permission <= 3
                    && channel.minimum_send_permission <= 3 =>
            {
                self.channels.remove(&channel.id);
                if channel.flags & 128 == 0 {
                    channel.flags &= 3;
                    channel.text_color |= 0xff000000;
                    channel.name = source_substring(&channel.name, 8);
                    channel.description = source_substring(&channel.description, 256);
                    self.channels.insert(channel.id, channel);
                }
            }
            CommandBody::SourceFields { .. } if command.kind == 6 && self.remove_actor(uid) => {
                self.departing.insert(uid);
            }
            _ => {}
        }
        Ok(())
    }

    fn set_permissions(&mut self, uid: u32, level: u8) {
        let present = self.actors.contains_key(&uid);
        if let Some(actor) = self.actors.get_mut(&uid) {
            actor.permissions = level;
        }
        self.roommates.remove(&uid);
        self.build_roommates.remove(&uid);
        if level >= 1 && (present || level < 4) {
            self.roommates.insert(uid);
        }
        if level >= 2 && (present || level < 4) {
            self.build_roommates.insert(uid);
        }
        if level == 3 {
            self.owner_id = uid;
        } else if self.owner_id == uid {
            self.owner_id = 0;
        }
    }

    fn remove_actor(&mut self, uid: u32) -> bool {
        if let Some(actor) = self.actors.remove(&uid) {
            self.ignored_entries = self.ignored_entries.saturating_sub(actor.ignored.len());
            self.actor_name_bytes = self.actor_name_bytes.saturating_sub(actor.name.len());
            true
        } else {
            false
        }
    }

    fn within_state_budget(&self) -> bool {
        let entries = [
            self.actors.len(),
            self.incarnations.len(),
            self.departing.len(),
            self.roommates.len(),
            self.build_roommates.len(),
            self.ignored_entries,
        ]
        .into_iter()
        .fold(0usize, usize::saturating_add);
        let bytes = self
            .channels
            .values()
            .fold(self.actor_name_bytes, |sum, channel| {
                sum.saturating_add(channel.name.len())
                    .saturating_add(channel.description.len())
            });
        entries <= self.limits.max_entries && bytes <= self.limits.max_text_bytes
    }
}

fn source_substring(value: &str, units: usize) -> String {
    String::from_utf16_lossy(&value.encode_utf16().take(units).collect::<Vec<_>>())
}

fn charge_event(used: &mut usize, fields: &[&str]) -> Result<()> {
    // Every source byte can expand to six JSON bytes. Reserving 512 bytes per
    // event covers fixed keys and numeric fields within the existing envelope.
    let amount = fields
        .iter()
        .try_fold(512usize, |sum, value| sum.checked_add(value.len()));
    *used = amount
        .and_then(|amount| used.checked_add(amount))
        .filter(|total| *total <= MAX_PROJECTED_EVENT_BYTES)
        .ok_or(Error {
            offset: 0,
            kind: ErrorKind::Limit,
            context: "source chat projection byte budget",
        })?;
    Ok(())
}
