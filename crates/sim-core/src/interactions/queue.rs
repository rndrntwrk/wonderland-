// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//! Bounded VMThread queue state transitions. No object behavior or frame engine.

use super::adapters::{AuthorityOperation, QueueRuntime, WorldProvider};
use super::query::{check_actor_stamp, check_world_stamp, require_authority, validate_definition};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i16)]
pub enum QueuePriority {
    Maximum = 100,
    Autonomous = 2,
    UserDriven = 50,
    ParentIdle = 40,
    ParentExit = 30,
    Idle = 0,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum QueueMode {
    Normal = 0,
    ParentIdle = 1,
    ParentExit = 2,
    Idle = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushPriority {
    Inherited,
    Maximum,
    Autonomous,
    UserDriven,
    ParentIdle,
    ParentExit,
    Idle,
}

/// VMPushInteraction's priority and mode mapping. Inherited is at least one.
pub fn resolve_push_priority(
    priority: PushPriority,
    current_action_priority: Option<i16>,
) -> (i16, QueueMode) {
    match priority {
        PushPriority::Inherited => (
            current_action_priority.unwrap_or(1).max(1),
            QueueMode::Normal,
        ),
        PushPriority::Maximum => (100, QueueMode::Normal),
        PushPriority::Autonomous => (2, QueueMode::Normal),
        PushPriority::UserDriven => (50, QueueMode::Normal),
        PushPriority::ParentIdle => (40, QueueMode::ParentIdle),
        PushPriority::ParentExit => (30, QueueMode::ParentExit),
        PushPriority::Idle => (0, QueueMode::Idle),
    }
}

/// An internal invocation is trusted interpreter input. Network/UI commands may
/// only use validate_intent + enqueue_validated; they never supply flags or code.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActionInvocation {
    pub actor: EntityKey,
    pub target: EntityKey,
    pub stack_object: EntityKey,
    pub icon_owner: Option<EntityKey>,
    pub definition: InteractionDefinition,
    pub args: [i16; 4],
    pub priority: i16,
    pub mode: QueueMode,
    /// Opaque provider-local callback handle; no durable callback runs here.
    pub callback: Option<u64>,
    pub interaction_result: i8,
    pub result_check_counter: u16,
}

impl ActionInvocation {
    pub(crate) fn user(
        actor: EntityKey,
        target: EntityKey,
        definition: InteractionDefinition,
        param0: i16,
    ) -> Self {
        Self {
            actor,
            target,
            stack_object: target,
            icon_owner: None,
            definition,
            args: [param0, 0, 0, 0],
            priority: QueuePriority::UserDriven as i16,
            mode: QueueMode::Normal,
            callback: None,
            interaction_result: -1,
            result_check_counter: 0,
        }
    }

    /// Source VMPushInteraction: applies SkipPermissions and continuation flags.
    pub fn pushed(
        actor: EntityKey,
        target: EntityKey,
        mut definition: InteractionDefinition,
        priority: PushPriority,
        current_action_priority: Option<i16>,
        push_head: bool,
        push_tail: bool,
    ) -> Self {
        definition.flags.0 |= ActionFlags::SKIP_PERMISSIONS;
        if push_head {
            definition.flags.0 |= ActionFlags::PUSH_HEAD;
        }
        if push_tail {
            definition.flags.0 |= ActionFlags::PUSH_TAIL;
        }
        let mut action = Self::user(actor, target, definition, 0);
        (action.priority, action.mode) = resolve_push_priority(priority, current_action_priority);
        action
    }

    pub fn effective_icon_owner(&self) -> EntityKey {
        self.icon_owner.unwrap_or(self.target)
    }
}

/// Provider-local ID, deliberately non-wrapping (legacy VMQueuedAction.UID wraps).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct ActionId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QueuedAction {
    pub id: ActionId,
    pub invocation: ActionInvocation,
    pub notify_idle: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemovalReason {
    Cancelled,
    ParentIdleCancelled,
    CheckRejected,
    DeadTarget,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinishResult {
    Succeeded,
    Failed,
    Aborted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeStage {
    Check,
    Start,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueueEventKind {
    Enqueued { index: usize },
    CancelRequested,
    IconChanged { target: EntityKey },
    NotifyIdle { value: bool },
    PriorityChanged { value: i16 },
    ActorCancellationChanged { value: bool },
    EntryPriorityChanged { value: i16 },
    Removed { reason: RemovalReason },
    Started { immediate: bool },
    StartRejected,
    RuntimeFailed { stage: RuntimeStage, error: Error },
    Finished { result: FinishResult },
    CallbackRequested { callback: u64 },
    ResetAvatarInteractionStateRequested,
    EodDisconnectRequested,
    QueueSkippedEntryPointRequested { target: EntityKey },
}

/// Sequence is strictly increasing for this actor-generation queue. All events
/// in one returned transition share its new revision and preserve source order.
/// Swarm F attaches the 30 Hz tick/command envelope; no events are sent here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueEvent {
    pub sequence: u64,
    pub queue_revision: u64,
    pub actor: EntityKey,
    pub action: Option<ActionId>,
    pub kind: QueueEventKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueTransition<T> {
    pub result: T,
    pub queue_revision: u64,
    pub events: Vec<QueueEvent>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelOutcome {
    Removed,
    Retained,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PushOutcome {
    Empty,
    PriorityBlocked,
    Started(ActionId),
    StartRejected(ActionId),
    RuntimeFailed {
        action: ActionId,
        stage: RuntimeStage,
        error: Error,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CancelIntent {
    pub principal: PrincipalKey,
    pub world_revision: u64,
    pub actor: EntityVersion,
    pub queue_revision: u64,
    pub command_sequence: u64,
    pub action: ActionId,
}

/// One queue per actor generation. Preserve counters on save/restore, or advance
/// the actor generation before replacement. This type is not a serialization ABI.
#[derive(Debug, PartialEq, Eq, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActionQueue {
    owner: EntityKey,
    mode: LegacyMode,
    check_thread: bool,
    limits: InteractionLimits,
    entries: Vec<QueuedAction>,
    active_len: usize,
    current_priority: i16,
    interaction_cancelled: bool,
    revision: u64,
    next_action_id: u64,
    next_event_sequence: u64,
    last_command_sequence: Option<u64>,
}

struct EventLog {
    actor: EntityKey,
    revision: u64,
    next_sequence: u64,
    bound: usize,
    events: Vec<QueueEvent>,
}

impl EventLog {
    fn push(&mut self, action: Option<ActionId>, kind: QueueEventKind) -> Result<()> {
        if self.events.len() >= self.bound {
            return Err(Error::LimitExceeded("queue transition events"));
        }
        let sequence = self.next_sequence;
        self.next_sequence = sequence.checked_add(1).ok_or(Error::CounterExhausted)?;
        self.events.push(QueueEvent {
            sequence,
            queue_revision: self.revision,
            actor: self.actor,
            action,
            kind,
        });
        Ok(())
    }
}

impl ActionQueue {
    pub fn new(owner: EntityKey, mode: LegacyMode, limits: InteractionLimits) -> Result<Self> {
        Self::make(owner, mode, limits, false)
    }

    pub fn new_check_thread(
        owner: EntityKey,
        mode: LegacyMode,
        limits: InteractionLimits,
    ) -> Result<Self> {
        Self::make(owner, mode, limits, true)
    }

    fn make(
        owner: EntityKey,
        mode: LegacyMode,
        limits: InteractionLimits,
        check_thread: bool,
    ) -> Result<Self> {
        if limits.max_queue_entries == 0 || limits.max_user_queue_entries > limits.max_queue_entries
        {
            return Err(Error::LimitExceeded("invalid queue limits"));
        }
        limits
            .max_queue_entries
            .checked_add(1)
            .and_then(|n| n.checked_mul(8))
            .and_then(|n| n.checked_add(64))
            .ok_or(Error::CounterExhausted)?;
        Ok(Self {
            owner,
            mode,
            check_thread,
            limits,
            entries: Vec::new(),
            active_len: 0,
            current_priority: 0,
            interaction_cancelled: false,
            revision: 0,
            next_action_id: 1,
            next_event_sequence: 1,
            last_command_sequence: None,
        })
    }

    /// Semantic validation required before a deserialized queue is executable.
    pub fn validate(&self) -> Result<()> {
        if self.owner.slot == 0
            || self.owner.slot > i16::MAX as u32
            || self.owner.generation == 0
            || self.active_len > self.entries.len()
            || self.entries.len() > self.limits.max_queue_entries
            || self.limits.max_queue_entries == 0
            || self.limits.max_queue_entries > 65536
            || self.limits.max_user_queue_entries > self.limits.max_queue_entries
            || self.next_action_id == 0
            || self.next_event_sequence == 0
        {
            return Err(Error::InvalidSnapshot("queue bounds/identity"));
        }
        let mut ids = std::collections::BTreeSet::new();
        for entry in &self.entries {
            if entry.invocation.actor != self.owner
                || entry.id.0 == 0
                || entry.id.0 >= self.next_action_id
                || !ids.insert(entry.id)
            {
                return Err(Error::InvalidSnapshot("queue action identity"));
            }
            for key in [
                Some(entry.invocation.target),
                Some(entry.invocation.stack_object),
                entry.invocation.icon_owner,
            ]
            .into_iter()
            .flatten()
            {
                if key.slot == 0 || key.slot > i16::MAX as u32 || key.generation == 0 {
                    return Err(Error::InvalidSnapshot("queue entity reference"));
                }
            }
            validate_definition(&entry.invocation.definition, &self.limits)?;
        }
        Ok(())
    }
    pub fn set_active_icon(&mut self, target: EntityKey) -> Result<QueueTransition<()>> {
        let index = self
            .active_len
            .checked_sub(1)
            .ok_or(Error::NoActiveAction)?;
        let (mut staged, mut log) = self.stage()?;
        staged.entries[index].invocation.icon_owner = Some(target);
        log.push(
            Some(staged.entries[index].id),
            QueueEventKind::IconChanged { target },
        )?;
        staged.commit(self, log, ())
    }
    pub fn owner(&self) -> EntityKey {
        self.owner
    }
    pub fn mode(&self) -> LegacyMode {
        self.mode
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn entries(&self) -> &[QueuedAction] {
        &self.entries
    }
    pub fn active_len(&self) -> usize {
        self.active_len
    }
    pub fn active(&self) -> Option<&QueuedAction> {
        self.active_len
            .checked_sub(1)
            .map(|index| &self.entries[index])
    }
    pub fn current_priority(&self) -> i16 {
        self.current_priority
    }
    pub fn interaction_cancelled(&self) -> bool {
        self.interaction_cancelled
    }
    pub fn last_command_sequence(&self) -> Option<u64> {
        self.last_command_sequence
    }

    /// UIInteractionQueue's normal (non-debug) presentation: idle stays hidden,
    /// parent exit appears only at index zero, and only the first parent idle is
    /// visible. Queue index zero is the source UI's definition of active here.
    pub fn entry_visible(&self, index: usize) -> Option<bool> {
        self.entries.get(index).map(|entry| {
            entry.invocation.mode != QueueMode::Idle
                && (index == 0 || entry.invocation.mode != QueueMode::ParentExit)
                && (entry.invocation.mode != QueueMode::ParentIdle
                    || !self.entries[..index]
                        .iter()
                        .any(|earlier| earlier.invocation.mode == QueueMode::ParentIdle))
        })
    }

    pub(crate) fn validate_command(
        &self,
        actor: EntityKey,
        revision: u64,
        sequence: u64,
    ) -> Result<()> {
        if actor != self.owner {
            return Err(Error::StaleEntity(actor));
        }
        if revision != self.revision {
            return Err(Error::StaleQueueRevision {
                expected: revision,
                actual: self.revision,
            });
        }
        if let Some(last) = self.last_command_sequence {
            if sequence <= last {
                return Err(Error::Replay { sequence, last });
            }
        }
        Ok(())
    }

    pub fn enqueue_validated<W: WorldProvider, R: QueueRuntime>(
        &mut self,
        world: &W,
        runtime: &R,
        validated: ValidatedIntent,
    ) -> Result<QueueTransition<ActionId>> {
        let intent = validated.intent;
        self.validate_command(
            intent.seen.actor.key,
            intent.queue_revision,
            intent.command_sequence,
        )?;
        check_world_stamp(world, intent.seen)?;
        require_authority(
            world,
            intent.principal,
            self.owner,
            AuthorityOperation::Invoke {
                target: intent.seen.target.key,
                interaction: intent.interaction,
            },
        )?;
        if validated.mode != self.mode {
            return Err(Error::InvalidSnapshot(
                "validated intent queue dialect mismatch",
            ));
        }
        if self.entries.len() >= self.limits.max_user_queue_entries {
            return Err(Error::UserQueueFull);
        }
        self.enqueue(validated.action, runtime, Some(intent.command_sequence))
    }

    /// Trusted VMPushInteraction entry, never a client-facing authorization bypass.
    pub fn enqueue_internal<R: QueueRuntime>(
        &mut self,
        action: ActionInvocation,
        runtime: &R,
    ) -> Result<QueueTransition<ActionId>> {
        self.enqueue(action, runtime, None)
    }

    fn enqueue<R: QueueRuntime>(
        &mut self,
        action: ActionInvocation,
        runtime: &R,
        sequence: Option<u64>,
    ) -> Result<QueueTransition<ActionId>> {
        if self.entries.len() >= self.limits.max_queue_entries {
            return Err(Error::QueueFull);
        }
        if action.actor != self.owner {
            return Err(Error::StaleEntity(action.actor));
        }
        validate_definition(&action.definition, &self.limits)?;
        let (mut staged, mut log) = self.stage()?;
        let id = ActionId(staged.next_action_id);
        staged.next_action_id = staged
            .next_action_id
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        let mut entry = QueuedAction {
            id,
            invocation: action,
            notify_idle: false,
        };
        let immediate = !staged.check_thread
            && entry
                .invocation
                .definition
                .flags
                .has(ActionFlags::RUN_IMMEDIATELY);
        let (index, ts1_skip_after) = if immediate {
            entry.invocation.mode = QueueMode::Idle;
            (staged.entries.len(), false)
        } else {
            staged.insertion_index(&entry)
        };
        staged.entries.insert(index, entry);
        log.push(Some(id), QueueEventKind::Enqueued { index })?;
        if ts1_skip_after {
            // The C# loop repeatedly cancels the same retained item forever when
            // MustRun/ParentExit prevents removal. Visit its original tail once.
            let tail: Vec<_> = staged.entries[index + 1..]
                .iter()
                .map(|entry| entry.id)
                .collect();
            for id in tail {
                if let Some(index) = staged.entries.iter().position(|entry| entry.id == id) {
                    staged.cancel_at(index, &mut log)?;
                }
            }
        }
        if !immediate {
            staged.evaluate_priorities(runtime, &mut log)?;
        }
        if let Some(sequence) = sequence {
            staged.last_command_sequence = Some(sequence);
        }
        staged.commit(self, log, id)
    }

    fn insertion_index(&self, entry: &QueuedAction) -> (usize, bool) {
        let action = &entry.invocation;
        if self.entries.is_empty() {
            return (0, false);
        }
        if action.definition.flags.has(ActionFlags::PUSH_HEAD) {
            return (self.active_len, false);
        }
        let leapfrog =
            self.mode == LegacyMode::Tso && action.definition.flags.has(ActionFlags::LEAPFROG);
        if (action.definition.flags.has(ActionFlags::PUSH_TAIL) || leapfrog)
            && action.mode != QueueMode::ParentExit
        {
            let index = (self.active_len..self.entries.len())
                .find(|&index| self.entries[index].invocation.priority < action.priority)
                .unwrap_or(self.entries.len());
            return (index, false);
        }
        let mut hit_parent_end = action.mode != QueueMode::ParentIdle;
        for index in (self.active_len..self.entries.len()).rev() {
            let existing = &self.entries[index].invocation;
            if hit_parent_end
                && (action.priority <= existing.priority || existing.mode == QueueMode::ParentExit)
            {
                return (
                    index + 1,
                    self.mode == LegacyMode::Ts1 && action.priority <= existing.priority,
                );
            }
            if existing.mode == QueueMode::ParentExit {
                hit_parent_end = true;
            }
        }
        (self.active_len, false)
    }

    pub fn cancel_intent<W: WorldProvider>(
        &mut self,
        world: &W,
        intent: CancelIntent,
    ) -> Result<QueueTransition<CancelOutcome>> {
        self.validate_command(
            intent.actor.key,
            intent.queue_revision,
            intent.command_sequence,
        )?;
        check_actor_stamp(world, intent.world_revision, intent.actor)?;
        require_authority(
            world,
            intent.principal,
            self.owner,
            AuthorityOperation::Cancel {
                action_id: intent.action.0,
            },
        )?;
        self.cancel(intent.action, Some(intent.command_sequence))
    }

    /// Scheduler/interpreter-only cancellation, e.g. TS1 queue-skip continuations.
    pub fn cancel_internal(&mut self, action: ActionId) -> Result<QueueTransition<CancelOutcome>> {
        self.cancel(action, None)
    }

    fn cancel(
        &mut self,
        action: ActionId,
        sequence: Option<u64>,
    ) -> Result<QueueTransition<CancelOutcome>> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.id == action)
            .ok_or(Error::MissingAction(action.0))?;
        let (mut staged, mut log) = self.stage()?;
        let result = staged.cancel_at(index, &mut log)?;
        if let Some(sequence) = sequence {
            staged.last_command_sequence = Some(sequence);
        }
        staged.commit(self, log, result)
    }

    fn cancel_at(&mut self, index: usize, log: &mut EventLog) -> Result<CancelOutcome> {
        let id = self.entries[index].id;
        if self.entries[index].invocation.mode == QueueMode::ParentIdle
            && self.entries[index + 1..self.active_len.max(index + 1)]
                .iter()
                .any(|entry| entry.invocation.mode == QueueMode::ParentIdle)
        {
            // The literal source would delete a protected active descendant and
            // leave a stack/queue mismatch. This unsupported state faults before
            // side effects; the scheduler must unwind it explicitly.
            return Err(Error::InvalidSnapshot(
                "parent-idle cancellation would remove an active descendant",
            ));
        }
        if index == 0 {
            log.push(Some(id), QueueEventKind::EodDisconnectRequested)?;
        }
        log.push(Some(id), QueueEventKind::CancelRequested)?;
        self.set_notify(index, true, log)?;
        if self.entries[index].invocation.mode == QueueMode::ParentIdle {
            let mut after = index + 1;
            while after < self.entries.len() {
                match self.entries[after].invocation.mode {
                    QueueMode::ParentIdle if after >= self.active_len => {
                        self.remove_at(after, RemovalReason::ParentIdleCancelled, log)?;
                        continue;
                    }
                    QueueMode::ParentExit => {
                        // Pending exits are marked cancelled, kept, and allowed
                        // to execute their source cleanup check/action.
                        self.set_notify(after, true, log)?;
                        self.set_entry_priority(after, 0, log)?;
                    }
                    _ => {}
                }
                after += 1;
            }
        }
        let action = &self.entries[index].invocation;
        let can_skip = !action.definition.flags.has(ActionFlags::MUST_RUN);
        if can_skip
            && index >= self.active_len
            && (action.mode == QueueMode::Normal
                || action.definition.flags.has(ActionFlags::DIRECT_CONTROL))
        {
            let target = action.target;
            self.remove_at(index, RemovalReason::Cancelled, log)?;
            if self.mode == LegacyMode::Ts1 {
                log.push(
                    Some(id),
                    QueueEventKind::QueueSkippedEntryPointRequested { target },
                )?;
            }
            Ok(CancelOutcome::Removed)
        } else {
            self.set_cancelled(true, log)?;
            self.set_priority_value(0, log)?;
            Ok(CancelOutcome::Retained)
        }
    }

    /// Re-evaluate priority at the scheduler's QueueDirty point. Does not invent
    /// EnsureDirectControlAction's object-specific implicit interaction.
    pub fn refresh_priorities<R: QueueRuntime>(
        &mut self,
        runtime: &R,
    ) -> Result<QueueTransition<()>> {
        let (mut staged, mut log) = self.stage()?;
        staged.evaluate_priorities(runtime, &mut log)?;
        staged.commit(self, log, ())
    }

    /// Source person-data Priority writes must pass through this hook.
    pub fn set_current_priority<R: QueueRuntime>(
        &mut self,
        priority: i16,
        runtime: &R,
    ) -> Result<QueueTransition<()>> {
        let (mut staged, mut log) = self.stage()?;
        staged.set_priority_value(priority, &mut log)?;
        staged.evaluate_priorities(runtime, &mut log)?;
        staged.commit(self, log, ())
    }

    /// Synchronize authoritative VMEntityFlags.InteractionCanceled writes made
    /// by primitives outside this module. A repeated value produces no event.
    pub fn set_interaction_cancelled(&mut self, cancelled: bool) -> Result<QueueTransition<()>> {
        let (mut staged, mut log) = self.stage()?;
        staged.set_cancelled(cancelled, &mut log)?;
        staged.commit(self, log, ())
    }

    fn evaluate_priorities<R: QueueRuntime>(
        &mut self,
        runtime: &R,
        log: &mut EventLog,
    ) -> Result<()> {
        let Some(active_index) = self.active_len.checked_sub(1) else {
            return Ok(());
        };
        let mut compare = i32::from(self.current_priority);
        if compare == QueuePriority::Autonomous as i32 {
            compare -= 1;
        }
        self.set_notify(
            active_index,
            self.entries[active_index].invocation.priority != 0 && compare == 0,
            log,
        )?;
        let mut index = self.active_len;
        while index < self.entries.len() {
            if !runtime.target_is_alive(self.entries[index].invocation.target) {
                self.remove_at(index, RemovalReason::DeadTarget, log)?;
                continue;
            }
            if i32::from(self.entries[index].invocation.priority) > compare {
                self.set_notify(active_index, true, log)?;
                self.set_cancelled(true, log)?;
            }
            index += 1;
        }
        Ok(())
    }

    /// VMThread.AttemptPush. Failed check entries are removed (even MustRun);
    /// frame-start failure keeps the entry and never expands the active prefix.
    pub fn attempt_push<R: QueueRuntime>(
        &mut self,
        runtime: &mut R,
    ) -> Result<QueueTransition<PushOutcome>> {
        let (mut staged, mut log) = self.stage()?;
        let mut compare = i32::from(staged.current_priority);
        if compare <= 2 {
            compare -= 1;
        }
        let result = loop {
            let index = staged.active_len;
            let Some(entry) = staged.entries.get(index) else {
                break PushOutcome::Empty;
            };
            let id = entry.id;
            if i32::from(entry.invocation.priority) <= compare {
                break PushOutcome::PriorityBlocked;
            }
            if !runtime.target_is_alive(entry.invocation.target) {
                staged.remove_at(index, RemovalReason::DeadTarget, &mut log)?;
                continue;
            }
            if entry.notify_idle {
                staged.set_cancelled(true, &mut log)?;
            }
            let action = staged.entries[index].invocation.clone();
            let allow = if staged.check_thread {
                Ok(true)
            } else if action.mode == QueueMode::ParentIdle && staged.interaction_cancelled {
                Ok(false)
            } else {
                runtime.check_action(&action)
            };
            match allow {
                Ok(false) => {
                    staged.remove_at(index, RemovalReason::CheckRejected, &mut log)?;
                    continue;
                }
                Err(error) => {
                    log.push(
                        Some(id),
                        QueueEventKind::RuntimeFailed {
                            stage: RuntimeStage::Check,
                            error: error.clone(),
                        },
                    )?;
                    break PushOutcome::RuntimeFailed {
                        action: id,
                        stage: RuntimeStage::Check,
                        error,
                    };
                }
                Ok(true) => {}
            }
            staged.set_cancelled(false, &mut log)?;
            staged.set_priority_value(action.priority, &mut log)?;
            match runtime.start_action(&action) {
                Ok(true) => {
                    staged.active_len += 1;
                    log.push(Some(id), QueueEventKind::Started { immediate: false })?;
                    break PushOutcome::Started(id);
                }
                Ok(false) => {
                    log.push(Some(id), QueueEventKind::StartRejected)?;
                    break PushOutcome::StartRejected(id);
                }
                Err(error) => {
                    log.push(
                        Some(id),
                        QueueEventKind::RuntimeFailed {
                            stage: RuntimeStage::Start,
                            error: error.clone(),
                        },
                    )?;
                    break PushOutcome::RuntimeFailed {
                        action: id,
                        stage: RuntimeStage::Start,
                        error,
                    };
                }
            }
        };
        staged.commit(self, log, result)
    }

    /// VMThread.TryRunImmediately runs before ordinary Allow Push. Like source,
    /// an already-active first immediate item blocks another immediate injection.
    pub fn try_run_immediately<R: QueueRuntime>(
        &mut self,
        runtime: &mut R,
    ) -> Result<QueueTransition<PushOutcome>> {
        let (mut staged, mut log) = self.stage()?;
        let result = loop {
            let Some(index) = staged.entries.iter().position(|entry| {
                entry
                    .invocation
                    .definition
                    .flags
                    .has(ActionFlags::RUN_IMMEDIATELY)
            }) else {
                break PushOutcome::Empty;
            };
            if index < staged.active_len {
                break PushOutcome::PriorityBlocked;
            }
            let id = staged.entries[index].id;
            let action = staged.entries[index].invocation.clone();
            if !runtime.target_is_alive(action.target) {
                staged.remove_at(index, RemovalReason::DeadTarget, &mut log)?;
                continue;
            }
            match runtime.check_action(&action) {
                Ok(false) => {
                    staged.remove_at(index, RemovalReason::CheckRejected, &mut log)?;
                    continue;
                }
                Err(error) => {
                    log.push(
                        Some(id),
                        QueueEventKind::RuntimeFailed {
                            stage: RuntimeStage::Check,
                            error: error.clone(),
                        },
                    )?;
                    break PushOutcome::RuntimeFailed {
                        action: id,
                        stage: RuntimeStage::Check,
                        error,
                    };
                }
                Ok(true) => {}
            }
            match runtime.start_action(&action) {
                Ok(true) => {
                    let entry = staged.entries.remove(index);
                    staged.entries.insert(staged.active_len, entry);
                    staged.active_len += 1;
                    log.push(Some(id), QueueEventKind::Started { immediate: true })?;
                    break PushOutcome::Started(id);
                }
                Ok(false) => {
                    // Source ignores Push(false) here. Retaining the queued item
                    // instead prevents a fictitious active frame.
                    log.push(Some(id), QueueEventKind::StartRejected)?;
                    break PushOutcome::StartRejected(id);
                }
                Err(error) => {
                    log.push(
                        Some(id),
                        QueueEventKind::RuntimeFailed {
                            stage: RuntimeStage::Start,
                            error: error.clone(),
                        },
                    )?;
                    break PushOutcome::RuntimeFailed {
                        action: id,
                        stage: RuntimeStage::Start,
                        error,
                    };
                }
            }
        };
        staged.commit(self, log, result)
    }

    /// Called after the matching action frame has returned/unwound. Failure and
    /// abort pop exactly the active leaf; waiting ParentExit cleanup remains.
    pub fn finish_active<R: QueueRuntime>(
        &mut self,
        result: FinishResult,
        runtime: &R,
    ) -> Result<QueueTransition<ActionId>> {
        let index = self
            .active_len
            .checked_sub(1)
            .ok_or(Error::NoActiveAction)?;
        let (mut staged, mut log) = self.stage()?;
        let id = staged.entries[index].id;
        log.push(Some(id), QueueEventKind::Finished { result })?;
        if staged.entries[index].invocation.mode != QueueMode::ParentIdle {
            staged.set_cancelled(false, &mut log)?;
        }
        if let Some(callback) = staged.entries[index].invocation.callback {
            log.push(Some(id), QueueEventKind::CallbackRequested { callback })?;
        }
        staged.active_len -= 1;
        staged.remove_at(index, RemovalReason::Finished, &mut log)?;
        if !staged.check_thread && index == 0 {
            log.push(
                Some(id),
                QueueEventKind::ResetAvatarInteractionStateRequested,
            )?;
        }
        let priority = staged.active().map_or(0, |entry| entry.invocation.priority);
        staged.set_priority_value(priority, &mut log)?;
        staged.evaluate_priorities(runtime, &mut log)?;
        staged.commit(self, log, id)
    }

    fn remove_at(&mut self, index: usize, reason: RemovalReason, log: &mut EventLog) -> Result<()> {
        if index < self.active_len {
            return Err(Error::InvalidSnapshot(
                "cannot remove an active queue ancestor",
            ));
        }
        let entry = self.entries.remove(index);
        log.push(Some(entry.id), QueueEventKind::Removed { reason })
    }

    fn set_notify(&mut self, index: usize, value: bool, log: &mut EventLog) -> Result<()> {
        let entry = &mut self.entries[index];
        if entry.notify_idle != value {
            entry.notify_idle = value;
            log.push(Some(entry.id), QueueEventKind::NotifyIdle { value })?;
        }
        Ok(())
    }

    fn set_entry_priority(&mut self, index: usize, value: i16, log: &mut EventLog) -> Result<()> {
        let entry = &mut self.entries[index];
        if entry.invocation.priority != value {
            entry.invocation.priority = value;
            log.push(
                Some(entry.id),
                QueueEventKind::EntryPriorityChanged { value },
            )?;
        }
        Ok(())
    }

    fn set_priority_value(&mut self, value: i16, log: &mut EventLog) -> Result<()> {
        if self.current_priority != value {
            self.current_priority = value;
            log.push(None, QueueEventKind::PriorityChanged { value })?;
        }
        Ok(())
    }

    fn set_cancelled(&mut self, value: bool, log: &mut EventLog) -> Result<()> {
        if self.interaction_cancelled != value {
            self.interaction_cancelled = value;
            log.push(None, QueueEventKind::ActorCancellationChanged { value })?;
        }
        Ok(())
    }

    fn stage(&self) -> Result<(Self, EventLog)> {
        // Reserve the complete possible counter range before calling a provider.
        let bound = self
            .limits
            .max_queue_entries
            .checked_add(1)
            .and_then(|n| n.checked_mul(8))
            .and_then(|n| n.checked_add(64))
            .ok_or(Error::CounterExhausted)?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        self.next_event_sequence
            .checked_add(u64::try_from(bound).map_err(|_| Error::CounterExhausted)?)
            .ok_or(Error::CounterExhausted)?;
        let staged = Self {
            owner: self.owner,
            mode: self.mode,
            check_thread: self.check_thread,
            limits: self.limits,
            entries: self.entries.clone(),
            active_len: self.active_len,
            current_priority: self.current_priority,
            interaction_cancelled: self.interaction_cancelled,
            revision: self.revision,
            next_action_id: self.next_action_id,
            next_event_sequence: self.next_event_sequence,
            last_command_sequence: self.last_command_sequence,
        };
        Ok((
            staged,
            EventLog {
                actor: self.owner,
                revision,
                next_sequence: self.next_event_sequence,
                bound,
                events: Vec::new(),
            },
        ))
    }

    fn commit<T>(
        mut self,
        destination: &mut Self,
        log: EventLog,
        result: T,
    ) -> Result<QueueTransition<T>> {
        if self.active_len > self.entries.len()
            || self.entries.len() > self.limits.max_queue_entries
        {
            return Err(Error::InvalidSnapshot("queue active-prefix invariant"));
        }
        // A no-op scheduler poll does not stale a previously validated command.
        if !log.events.is_empty() || self.last_command_sequence != destination.last_command_sequence
        {
            self.revision = log.revision;
            self.next_event_sequence = log.next_sequence;
        }
        let queue_revision = self.revision;
        *destination = self;
        Ok(QueueTransition {
            result,
            queue_revision,
            events: log.events,
        })
    }
}
