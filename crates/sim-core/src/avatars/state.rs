use super::events::AnimationCue;
use super::lifecycle::{AvatarLifecycle, LeaveContext, LifecycleError, LifecycleRequest};
use super::motives::{DecayContext, Motive, MotiveDecay, MotiveError, MotiveState};
use super::outfits::{DefaultSuits, OutfitError, OutfitReference, OutfitState};
use super::skills::{SkillPolicy, SKILLS};
use super::social::{RelationshipState, RelationshipTarget, SocialError};
use super::timeline::{
    AnimationCommand, AnimationError, AnimationMetadata, AnimationMode, AnimationResult,
    AnimationTimeline,
};
use super::AvatarPlatform;
use crate::ids::{EntityRef, PersistentId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const PERSON_DATA_COUNT: usize = 101;
pub const TEMPLATE_PERSON: u32 = 0x7fd96b54;
pub const PERSIST_PERSON_DATA_MAP: [usize; 23] = [
    10, 11, 12, 15, 17, 18, 65, 68, 99, 61, 70, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 100,
];

/// Raw short slots remain available for TS1 aliases and BHAV-owned variables.
/// A Vec permits serde 1.0.195 encoding; validate enforces exactly 101 slots.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonData {
    pub values: Vec<i16>,
}
impl Default for PersonData {
    fn default() -> Self {
        let mut values = vec![0; PERSON_DATA_COUNT];
        values[7] = 1000;
        Self { values }
    }
}
impl PersonData {
    pub fn validate(&self) -> Result<(), AvatarError> {
        if self.values.len() == PERSON_DATA_COUNT {
            Ok(())
        } else {
            Err(AvatarError::InvalidPersonDataLength)
        }
    }
    pub fn read(&self, index: u16) -> Result<i16, AvatarError> {
        self.values
            .get(usize::from(index))
            .copied()
            .ok_or(AvatarError::PersonDataIndex(index))
    }
    pub fn write_raw(&mut self, index: u16, value: i16) -> Result<(), AvatarError> {
        let slot = self
            .values
            .get_mut(usize::from(index))
            .ok_or(AvatarError::PersonDataIndex(index))?;
        *slot = value;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum AvatarPermissions {
    Visitor = 0,
    Roommate = 1,
    BuildBuyRoommate = 2,
    Owner = 3,
    Admin = 4,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobInfo {
    pub experience: i16,
    pub level: i16,
    pub sick_days: i16,
    pub status_flags: i16,
}

/// Source head seeking changes person-data when its 15-tick blend ends even in
/// headless mode. Only the optional pose interpolation is presentation state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HeadSeekState {
    pub weight: f32,
    pub target: Option<EntityRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AvatarState {
    pub entity: EntityRef,
    pub persistent_id: PersistentId,
    pub platform: AvatarPlatform,
    pub object_guid: u32,
    pub name: String,
    pub person_data: PersonData,
    pub motives: MotiveState,
    pub decay: MotiveDecay,
    pub animations: AnimationTimeline,
    pub head_seek: HeadSeekState,
    pub outfits: OutfitState,
    pub relationships: RelationshipState,
    pub lifecycle: AvatarLifecycle,
    pub permissions: AvatarPermissions,
    pub avatar_flags: u32,
    pub job_info: BTreeMap<i16, JobInfo>,
    pub ignored_avatars: BTreeSet<PersistentId>,
    /// Projection for behavior checks. This value is not spending authority.
    pub budget_mirror: u32,
    pub force_enable_skill: bool,
    pub skill_mode: u8,
    pub lot_category: u8,
    pub has_thread: bool,
    pub message: String,
    pub message_timeout: i32,
    pub display_flags: i16,
    pub walk_animations: Vec<String>,
    pub swim_animations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AvatarError {
    InvalidEntity,
    InvalidPersonDataLength,
    PersonDataIndex(u16),
    InvalidJobId(i16),
    InvalidState,
    Motive(MotiveError),
    Animation(AnimationError),
    Outfit(OutfitError),
    Lifecycle(LifecycleError),
    Social(SocialError),
}
impl From<MotiveError> for AvatarError {
    fn from(e: MotiveError) -> Self {
        Self::Motive(e)
    }
}
impl From<AnimationError> for AvatarError {
    fn from(e: AnimationError) -> Self {
        Self::Animation(e)
    }
}
impl From<OutfitError> for AvatarError {
    fn from(e: OutfitError) -> Self {
        Self::Outfit(e)
    }
}
impl From<LifecycleError> for AvatarError {
    fn from(e: LifecycleError) -> Self {
        Self::Lifecycle(e)
    }
}
impl From<SocialError> for AvatarError {
    fn from(e: SocialError) -> Self {
        Self::Social(e)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonWrite {
    pub written: bool,
    pub queue_dirty: bool,
    pub outfit_request: Option<u16>,
    pub ghost: Option<bool>,
    pub display_flags: Option<i16>,
    pub money_headline: Option<i16>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvatarTickContext {
    pub minute: i32,
    pub hour: i32,
    pub room_score: i16,
    pub category: u8,
    pub has_thread: bool,
    pub thread_paused: bool,
    pub hidden: i16,
    pub out_of_world: bool,
    pub container_out_of_world: bool,
    pub leave_action_available: bool,
    pub leave_action_already_queued: bool,
}
impl Default for AvatarTickContext {
    fn default() -> Self {
        Self {
            minute: 0,
            hour: 0,
            room_score: 100,
            category: 0,
            has_thread: true,
            thread_paused: false,
            hidden: 0,
            out_of_world: false,
            container_out_of_world: true,
            leave_action_available: false,
            leave_action_already_queued: false,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvatarTickOutput {
    pub animation_cues: Vec<AnimationCue>,
    pub lifecycle_requests: Vec<LifecycleRequest>,
    pub message_cleared: bool,
}

impl AvatarState {
    pub fn new(entity: EntityRef, persistent_id: PersistentId, platform: AvatarPlatform) -> Self {
        Self {
            entity,
            persistent_id,
            platform,
            object_guid: TEMPLATE_PERSON,
            name: String::new(),
            person_data: PersonData::default(),
            motives: MotiveState::default(),
            decay: if platform == AvatarPlatform::Tso {
                MotiveDecay::tso(None)
            } else {
                MotiveDecay::ts1()
            },
            animations: AnimationTimeline::default(),
            head_seek: HeadSeekState::default(),
            outfits: OutfitState::default(),
            relationships: RelationshipState::default(),
            lifecycle: AvatarLifecycle::default(),
            permissions: AvatarPermissions::Visitor,
            avatar_flags: 0,
            job_info: BTreeMap::new(),
            ignored_avatars: BTreeSet::new(),
            budget_mirror: 0,
            force_enable_skill: false,
            skill_mode: 0,
            lot_category: 0,
            has_thread: true,
            message: String::new(),
            message_timeout: 0,
            display_flags: 0,
            walk_animations: vec![String::new(); 50],
            swim_animations: vec![String::new(); 50],
        }
    }
    pub fn validate(&self) -> Result<(), AvatarError> {
        if self.entity.object_id.0 <= 0 || self.entity.generation == 0 {
            return Err(AvatarError::InvalidEntity);
        }
        self.person_data.validate()?;
        self.motives.validate()?;
        self.decay.validate()?;
        self.animations.validate()?;
        self.outfits.validate()?;
        self.relationships.validate()?;
        self.lifecycle.validate()?;
        if self.name.len() > 4096
            || self.message.len() > 65_536
            || self.job_info.len() > 65_536
            || self.ignored_avatars.len() > 65_536
            || self.walk_animations.len() != 50
            || self.swim_animations.len() != 50
            || self
                .walk_animations
                .iter()
                .chain(self.swim_animations.iter())
                .any(|s| s.len() > 1024)
            || !self.head_seek.weight.is_finite()
            || !(0.0..=15.0).contains(&self.head_seek.weight)
            || self
                .head_seek
                .target
                .is_some_and(|target| target.object_id.0 <= 0 || target.generation == 0)
            || matches!(
                (&self.platform, &self.decay),
                (AvatarPlatform::Tso, MotiveDecay::Ts1(_))
                    | (AvatarPlatform::Ts1, MotiveDecay::Tso(_))
            )
        {
            return Err(AvatarError::InvalidState);
        }
        Ok(())
    }
    pub fn skill_policy(&self) -> SkillPolicy {
        SkillPolicy {
            platform: self.platform,
            persistent_id: self.persistent_id,
            permissions: self.permissions,
            force_enable: self.force_enable_skill,
            new_player: self.avatar_flags & 2 != 0,
            lot_category: self.lot_category,
            skill_mode: self.skill_mode,
            has_thread: self.has_thread,
        }
    }
    pub fn read_motive(&self, index: u8) -> Result<i16, AvatarError> {
        Ok(self.motives.get(Motive::try_from(index)?))
    }
    pub fn write_motive(&mut self, index: u8, value: i16) -> Result<(), AvatarError> {
        self.motives.set(Motive::try_from(index)?, value);
        Ok(())
    }
    /// VMAnimateSim entry point including its person-data side effect on reset.
    /// The content adapter still resolves animation scopes and reset posture.
    pub fn apply_animation(
        &mut self,
        command: &AnimationCommand,
        metadata: Option<&AnimationMetadata>,
    ) -> Result<AnimationResult, AvatarError> {
        self.person_data.validate()?;
        let mut next = self.clone();
        if command.reset
            && command.mode != AnimationMode::ClearCarryAndWait
            && next.person_data.values[42] == 1
        {
            next.person_data.values[42] = 4;
        }
        let result = next.animations.apply(command, metadata)?;
        next.validate()?;
        *self = next;
        Ok(result)
    }
    /// VMSetMotiveChange adapter. The bool reports whether the per-tick gate
    /// accepted a new continuous rate; the source primitive returns true either
    /// way. ClearAll ignores every other operand, including an invalid motive.
    pub fn apply_motive_change(
        &mut self,
        index: u8,
        raw_rate: i16,
        raw_max: i16,
        clear_all: bool,
        once: bool,
    ) -> Result<bool, AvatarError> {
        if clear_all {
            self.motives.clear_changes();
            return Ok(true);
        }
        let motive = Motive::try_from(index)?;
        let rate = if self.platform == AvatarPlatform::Ts1 {
            super::motives::scale_ts1_rate(i32::from(raw_rate), motive)
        } else if raw_rate < 0 {
            i32::from(raw_rate)
        } else {
            match &self.decay {
                MotiveDecay::Tso(decay) => decay
                    .tuning
                    .as_ref()
                    .ok_or(MotiveError::MissingTsoTuning)?
                    .scale_rate(i32::from(raw_rate), motive, self.lot_category)?,
                _ => return Err(AvatarError::InvalidState),
            }
        } as i16;
        let maximum = self.motives.scale_max(motive, raw_max);
        if once {
            self.motives.change_once(motive, rate, maximum);
            Ok(true)
        } else {
            Ok(self.motives.set_change(motive, rate, maximum))
        }
    }
    pub fn read_person_data(&self, index: u16) -> Result<i16, AvatarError> {
        self.person_data.validate()?;
        let raw = self.person_data.read(index)?;
        let job = self.job_info.get(&self.person_data.values[91]);
        Ok(match index {
            92 => job.map_or(0, |j| j.level),
            93 => job.map_or(0, |j| j.experience),
            94 => job.map_or(0, |j| j.sick_days),
            98 => job.map_or(0, |j| j.status_flags),
            21 => {
                if self.permissions >= AvatarPermissions::BuildBuyRoommate {
                    2
                } else if self.permissions >= AvatarPermissions::Roommate {
                    1
                } else {
                    0
                }
            }
            61 | 99 if self.platform == AvatarPlatform::Tso => {
                self.relationships.outgoing_friend_count()
            }
            70 => self.skill_policy().script_lock_mask(),
            _ => raw,
        })
    }
    pub fn write_person_data(
        &mut self,
        index: u16,
        value: i16,
    ) -> Result<PersonWrite, AvatarError> {
        self.person_data.validate()?;
        self.person_data.read(index)?;
        let mut result = PersonWrite::default();
        match index {
            91 => {
                if value > 5 {
                    return Err(AvatarError::InvalidJobId(value));
                }
                self.job_info.entry(value).or_insert(JobInfo {
                    status_flags: 1,
                    ..JobInfo::default()
                });
            }
            92 | 93 | 94 | 98 => {
                if let Some(job) = self.job_info.get_mut(&self.person_data.values[91]) {
                    match index {
                        92 => job.level = value,
                        93 => job.experience = value,
                        94 => job.sick_days = value,
                        98 => job.status_flags = value,
                        _ => unreachable!(),
                    }
                    result.written = true;
                }
                return Ok(result);
            }
            33 => result.queue_dirty = self.has_thread,
            41 => self.head_seek.target = None,
            1 => {
                if value != i16::MIN {
                    result.money_headline = Some(value);
                }
            }
            74 => {
                self.display_flags = value;
                result.display_flags = Some(value);
                return Ok(result);
            }
            70 => return Ok(result), // raw skill-lock allocation uses the dedicated setter
            68 => result.ghost = Some(value > 0),
            10 | 11 | 12 | 15 | 17 | 18 => {
                let skill = SKILLS
                    .into_iter()
                    .find(|s| s.person_index() == usize::from(index))
                    .ok_or(AvatarError::InvalidState)?;
                let write = self
                    .skill_policy()
                    .write(&mut self.person_data, skill, value);
                result.written = !write.blocked;
                return Ok(result);
            }
            8 => {
                if self.platform == AvatarPlatform::Ts1 {
                    result.outfit_request = Some(value as u16);
                } else {
                    let job = self.job_info.get(&self.person_data.values[91]);
                    if let Ok(outfit) = self.outfits.resolve_tso(
                        value as u16,
                        self.person_data.values[65],
                        self.person_data.values[91],
                        job.map_or(0, |j| j.level),
                    ) {
                        self.outfits.body = Some(outfit);
                    }
                }
            }
            _ => {}
        }
        self.person_data.values[usize::from(index)] = value;
        result.written = true;
        Ok(result)
    }
    pub fn skill_locks(&self) -> i16 {
        self.person_data.values[70]
    }
    pub fn set_skill_locks(&mut self, count: i16) {
        self.person_data.values[70] = count;
    }
    /// VMChangeSuitOrAccessory's object/Update branch changes default and body
    /// without writing CurrentOutfit. B resolves callee STR 304/temp zero.
    pub fn set_default_daywear(&mut self, outfit: OutfitReference) -> Result<(), AvatarError> {
        outfit.validate()?;
        self.outfits.defaults.daywear = outfit.clone();
        self.outfits.body = Some(outfit);
        Ok(())
    }
    /// The suit is already resolved. The source writes the original operand's
    /// suit index, even when a temp register selected a different actual body.
    pub fn set_body_outfit(
        &mut self,
        outfit: OutfitReference,
        current_outfit: i16,
    ) -> Result<(), AvatarError> {
        outfit.validate()?;
        self.person_data.validate()?;
        self.person_data.values[8] = current_outfit;
        self.outfits.body = Some(outfit);
        Ok(())
    }
    /// Names are exact source keys: the primitive's `foo.apr` and animation
    /// time-property `foo` are distinct entries, as in the original HashSet.
    pub fn set_accessory_name(
        &mut self,
        appearance: String,
        remove: bool,
    ) -> Result<(), AvatarError> {
        if appearance.len() > 16_384
            || (!remove
                && !self.animations.bound_appearances.contains(&appearance)
                && self.animations.bound_appearances.len() >= 4096)
        {
            return Err(AnimationError::InvalidContinuation.into());
        }
        if remove {
            self.animations.bound_appearances.remove(&appearance);
        } else {
            self.animations.bound_appearances.insert(appearance);
        }
        Ok(())
    }
    pub fn is_pet(&self) -> bool {
        self.person_data.values[65] & (8 | 16) > 0
    }
    pub fn is_dog(&self) -> bool {
        self.person_data.values[65] & 8 > 0
    }
    pub fn is_cat(&self) -> bool {
        self.person_data.values[65] & 16 > 0
    }
    pub fn is_child(&self) -> bool {
        self.person_data.values[58] < 18
    }
    pub fn set_message(&mut self, message: String) -> Result<(), AvatarError> {
        if message.len() > 65_536 {
            return Err(AvatarError::InvalidState);
        }
        // String.Length is UTF-16 code units in C#.
        self.message_timeout = 150 + (message.encode_utf16().count() / 2) as i32;
        self.message = message;
        self.person_data.values[26] = 1;
        Ok(())
    }
    pub fn request_leave(&mut self, context: LeaveContext) -> Vec<LifecycleRequest> {
        self.lifecycle.request_leave(context)
    }
    pub fn reset(&mut self) -> Vec<LifecycleRequest> {
        self.person_data.values[33] = 0;
        self.animations.animations.clear();
        self.animations.bound_appearances.clear();
        vec![LifecycleRequest::ForceEodDisconnect]
    }
    /// Invoke after the entity's VMThread dispatch, matching VMAvatar.Tick's
    /// base.Tick -> thread -> motives -> timeline -> restoration -> timeout.
    pub fn tick(&mut self, context: &AvatarTickContext) -> Result<AvatarTickOutput, AvatarError> {
        self.validate()?;
        // Make missing content and malformed continuations atomic to callers.
        let mut next = self.clone();
        let output = next.tick_inner(context)?;
        next.validate()?;
        *self = next;
        Ok(output)
    }
    fn tick_inner(&mut self, c: &AvatarTickContext) -> Result<AvatarTickOutput, AvatarError> {
        self.has_thread = c.has_thread;
        self.lot_category = c.category;
        let mut output = AvatarTickOutput::default();
        if !self.message.is_empty() {
            let old = self.message_timeout;
            self.message_timeout = self.message_timeout.wrapping_sub(1);
            if old > 0 && self.message_timeout == 0 {
                self.person_data.values[26] = 0;
                self.message.clear();
                output.message_cleared = true;
            }
        }
        if c.has_thread && c.thread_paused {
            return Ok(output);
        }
        if self.person_data.values[98] == 0 {
            self.person_data.values[98] = 1;
        }
        if c.has_thread {
            self.decay.tick(
                &mut self.motives,
                &DecayContext {
                    minute: c.minute,
                    hour: c.hour,
                    room_score: c.room_score,
                    category: c.category,
                    cheats: self.person_data.values[29],
                    hidden: c.hidden,
                    active_personality: self.person_data.values[3],
                    outgoing_personality: self.person_data.values[6],
                },
            )?;
            if self.platform == AvatarPlatform::Tso
                && c.out_of_world
                && c.container_out_of_world
                && (self.persistent_id.0 > 0 || self.is_pet())
            {
                output
                    .lifecycle_requests
                    .push(LifecycleRequest::RelocateAtMailbox);
            }
        }
        output.animation_cues = self.animations.tick()?;
        self.tick_head_seek();
        self.motives.tick_changes(self.platform);
        self.person_data.values[27] = self.person_data.values[27].wrapping_add(1);
        if self.lifecycle.kill_timeout >= 0 {
            let requests = self.lifecycle.tick(LeaveContext {
                action_available: c.leave_action_available,
                action_already_queued: c.leave_action_already_queued,
            });
            if self.lifecycle.kill_timeout <= super::lifecycle::FORCE_DELETE_TIMEOUT {
                self.display_flags = 1;
                self.motives.set(Motive::SleepState, 0);
            }
            output.lifecycle_requests.extend(requests);
        }
        Ok(output)
    }
    /// Legacy neighbor inheritance preserves the avatar's gender/type/schedule,
    /// which is necessary for cats and service NPCs using shared person tables.
    pub fn inherit_neighbor(
        &mut self,
        neighbor_data: PersonData,
        neighbor_id: i16,
        current_family: Option<i16>,
    ) -> Result<(), AvatarError> {
        neighbor_data.validate()?;
        let gender = self.person_data.values[65];
        let person_type = self.person_data.values[32];
        let schedule = self.person_data.values[35];
        self.person_data = neighbor_data;
        self.person_data.values[65] = gender;
        self.person_data.values[31] = neighbor_id;
        self.person_data.values[32] = if person_type == 0 {
            i16::from(Some(self.person_data.values[61]) != current_family)
        } else {
            person_type
        };
        self.person_data.values[35] = schedule;
        self.person_data.values[34] = 0;
        Ok(())
    }
    /// Call with the live generation resolved from person-data slot 41 before
    /// each avatar tick; None reproduces the source's missing-target path.
    pub fn resolve_head_seek_target(
        &mut self,
        target: Option<EntityRef>,
    ) -> Result<(), AvatarError> {
        if target.is_some_and(|t| {
            t.object_id.0 <= 0 || t.generation == 0 || t.object_id.0 != self.person_data.values[41]
        }) {
            return Err(AvatarError::InvalidEntity);
        }
        self.head_seek.target = target;
        Ok(())
    }
    fn tick_head_seek(&mut self) {
        let seek = self.person_data.values[42];
        if seek <= 0 || seek >= 8 || !self.has_thread {
            return;
        }
        if seek == 1
            && self
                .head_seek
                .target
                .is_some_and(|t| t.object_id.0 == self.person_data.values[41])
        {
            self.head_seek.weight = (self.head_seek.weight + 1.0).min(15.0);
            if self.person_data.values[45] > 0 {
                self.person_data.values[45] -= 1;
                if self.person_data.values[45] == 0 {
                    self.person_data.values[42] = 4;
                }
            }
        } else if seek == 1 || seek == 4 {
            self.head_seek.weight -= 1.0;
            if self.head_seek.weight <= 0.0 {
                self.head_seek.weight = 0.0;
                self.person_data.values[42] = 8;
                self.person_data.values[43] = 0;
            }
        }
    }
}

/// Off-lot avatar data boundary. In-lot continuation snapshots serialize
/// AvatarState directly and must not apply the sleep/outfit persistence resets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvatarPersistState {
    pub name: String,
    pub persistent_id: PersistentId,
    pub default_suits: DefaultSuits,
    pub permissions: AvatarPermissions,
    pub avatar_flags: u32,
    pub budget_mirror: u32,
    pub body: Option<OutfitReference>,
    pub head: Option<OutfitReference>,
    pub skin_tone: u8,
    pub is_worker: bool,
    pub custom_guid: u32,
    pub motive_data: [i16; 16],
    pub person_data: [i16; 27],
    pub relationships: BTreeMap<PersistentId, Vec<i16>>,
    pub job_info: BTreeMap<i16, JobInfo>,
    pub ignored_avatars: BTreeSet<PersistentId>,
}
impl AvatarState {
    pub fn persistent_state(&self) -> Result<AvatarPersistState, AvatarError> {
        let mut motives = self.motives.values;
        motives[Motive::SleepState as usize] = 0;
        let mut person = [0; 27];
        for (destination, source) in PERSIST_PERSON_DATA_MAP.into_iter().enumerate() {
            person[destination] = self.read_person_data(source as u16)?;
        }
        person[10] = self.skill_locks();
        Ok(AvatarPersistState {
            name: self.name.clone(),
            persistent_id: self.persistent_id,
            default_suits: self.outfits.defaults.clone(),
            permissions: self.permissions,
            avatar_flags: self.avatar_flags,
            budget_mirror: self.budget_mirror,
            body: self.outfits.body_for_persistence(),
            head: self.outfits.head.clone(),
            skin_tone: self.outfits.skin_tone,
            is_worker: false,
            custom_guid: if self.object_guid == TEMPLATE_PERSON {
                0
            } else {
                self.object_guid
            },
            motive_data: motives,
            person_data: person,
            relationships: self
                .relationships
                .values
                .iter()
                .filter_map(|(target, values)| {
                    if let RelationshipTarget::Persistent(id) = target {
                        Some((*id, values.clone()))
                    } else {
                        None
                    }
                })
                .collect(),
            job_info: self.job_info.clone(),
            ignored_avatars: self.ignored_avatars.clone(),
        })
    }
    pub fn apply_persistent_state(
        &mut self,
        persist: &AvatarPersistState,
    ) -> Result<(), AvatarError> {
        let mut next = self.clone();
        next.outfits.skin_tone = persist.skin_tone;
        next.force_enable_skill = true;
        for (source, destination) in PERSIST_PERSON_DATA_MAP.into_iter().enumerate() {
            if persist.custom_guid == 0 || destination != 65 {
                next.write_person_data(destination as u16, persist.person_data[source])?;
            }
        }
        next.force_enable_skill = false;
        if persist.custom_guid == 0 {
            next.person_data.values[60] = i16::from(persist.skin_tone);
            next.outfits.defaults = persist.default_suits.clone();
            next.outfits.body = persist.body.clone();
            next.outfits.head = persist.head.clone();
        }
        next.name = persist.name.clone();
        next.permissions = persist.permissions;
        next.avatar_flags = persist.avatar_flags;
        next.budget_mirror = persist.budget_mirror;
        next.persistent_id = persist.persistent_id;
        next.object_guid = if persist.custom_guid == 0 {
            TEMPLATE_PERSON
        } else {
            persist.custom_guid
        };
        next.motives.values = persist.motive_data;
        next.relationships
            .values
            .retain(|k, _| !matches!(k, RelationshipTarget::Persistent(_)));
        for (id, values) in &persist.relationships {
            next.relationships
                .values
                .insert(RelationshipTarget::Persistent(*id), values.clone());
        }
        next.job_info = persist.job_info.clone();
        if persist.is_worker {
            next.write_person_data(98, 1)?;
        }
        next.set_skill_locks(persist.person_data[10]);
        next.ignored_avatars = persist.ignored_avatars.clone();
        next.validate()?;
        *self = next;
        Ok(())
    }
}
