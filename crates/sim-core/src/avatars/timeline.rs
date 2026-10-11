//! VMAnimationState, VMAvatar.Tick and VMAnimateSim's headless semantics.
use super::events::{AnimationCue, TimeProperty};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};

const MAX_TIME_PROPERTIES: usize = 65_536;
const MAX_ANIMATIONS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationMetadata {
    pub resource: String,
    pub num_frames: i32,
    pub time_properties: Vec<TimeProperty>,
}

impl AnimationMetadata {
    pub fn validate(&self) -> Result<(), AnimationError> {
        if self.resource.is_empty()
            || self.resource.len() > 1024
            || !(0..=1_000_000).contains(&self.num_frames)
            || self.time_properties.len() > MAX_TIME_PROPERTIES
        {
            return Err(AnimationError::InvalidMetadata);
        }
        for property in &self.time_properties {
            if property.properties.len() > 256
                || property
                    .properties
                    .iter()
                    .any(|(k, v)| k.len() > 1024 || v.len() > 16_384)
            {
                return Err(AnimationError::InvalidMetadata);
            }
            for key in ["righthand", "lefthand"] {
                if let Some(value) = property.properties.get(key) {
                    if value.trim().parse::<i16>().is_err() {
                        return Err(AnimationError::InvalidGesture {
                            key: key.into(),
                            value: value.clone(),
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum AnimationMode {
    PlayAndWait = 0,
    Loop = 1,
    Carry = 2,
    ClearCarryAndWait = 3,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationCommand {
    pub mode: AnimationMode,
    pub resource: String,
    /// Animation ID zero. The caller resolves posture's walk animation.
    pub reset: bool,
    pub backwards: bool,
    pub hurryable: bool,
    pub walk_style: i16,
    pub expected_events: u8,
    /// Slot zero occupancy, supplied for reset's default carry behavior.
    pub carrying: bool,
    pub default_carry: Option<AnimationMetadata>,
}

impl AnimationCommand {
    pub fn play(resource: impl Into<String>) -> Self {
        Self {
            mode: AnimationMode::PlayAndWait,
            resource: resource.into(),
            reset: false,
            backwards: false,
            hurryable: false,
            walk_style: 0,
            expected_events: 0,
            carrying: false,
            default_carry: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimationResult {
    Wait,
    Event(i16),
    Complete,
    CompleteNextTick,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimationError {
    InvalidMetadata,
    InvalidGesture { key: String, value: String },
    ResourceMismatch,
    InvalidContinuation,
    MissingDefaultCarry,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimationState {
    pub metadata: AnimationMetadata,
    pub current_frame: f32,
    pub event_queue: VecDeque<i16>,
    pub events_run: u8,
    pub end_reached: bool,
    pub backwards: bool,
    pub speed: f32,
    pub weight: f32,
    pub looping: bool,
    /// Half-open range of unconsumed resource-order time properties.
    pub remaining_start: usize,
    pub remaining_end: usize,
}

impl AnimationState {
    pub fn new(metadata: AnimationMetadata, backwards: bool) -> Result<Self, AnimationError> {
        metadata.validate()?;
        let frame = if backwards {
            metadata.num_frames as f32
        } else {
            0.0
        };
        let end = metadata.time_properties.len();
        Ok(Self {
            metadata,
            current_frame: frame,
            event_queue: VecDeque::new(),
            events_run: 0,
            end_reached: false,
            backwards,
            speed: 1.0,
            weight: 1.0,
            looping: false,
            remaining_start: 0,
            remaining_end: end,
        })
    }

    pub fn validate(&self) -> Result<(), AnimationError> {
        self.metadata.validate()?;
        if !self.current_frame.is_finite()
            || !self.speed.is_finite()
            || !self.weight.is_finite()
            || self.current_frame.abs() > 16_000_000.0
            || self.speed.abs() > 1_000_000.0
            || self.remaining_start > self.remaining_end
            || self.remaining_end > self.metadata.time_properties.len()
            || self.event_queue.len() > MAX_TIME_PROPERTIES + 255
        {
            return Err(AnimationError::InvalidContinuation);
        }
        Ok(())
    }

    /// Import helper for the legacy marshal which did not save its consumption
    /// cursor. Rust snapshots serialize the cursor directly and must not call it.
    pub fn discard_legacy_elapsed_properties(&mut self) {
        let time = (self.current_frame * 1000.0) / 30.0;
        if self.backwards {
            while self.remaining_start < self.remaining_end
                && self.metadata.time_properties[self.remaining_end - 1].time_ms as f32 >= time
            {
                self.remaining_end -= 1;
            }
        } else {
            while self.remaining_start < self.remaining_end
                && self.metadata.time_properties[self.remaining_start].time_ms as f32 <= time
            {
                self.remaining_start += 1;
            }
        }
    }

    fn advance(&mut self) -> Vec<TimeProperty> {
        let mut fired = Vec::new();
        if !self.end_reached && self.weight != 0.0 {
            if self.backwards {
                self.current_frame -= self.speed;
            } else {
                self.current_frame += self.speed;
            }
            let time = (self.current_frame * 1000.0) / 30.0;
            if self.backwards {
                while self.remaining_start < self.remaining_end {
                    let prop = &self.metadata.time_properties[self.remaining_end - 1];
                    if (prop.time_ms as f32) < time {
                        break;
                    }
                    fired.push(prop.clone());
                    self.remaining_end -= 1;
                }
            } else {
                while self.remaining_start < self.remaining_end {
                    let prop = &self.metadata.time_properties[self.remaining_start];
                    if prop.time_ms as f32 > time {
                        break;
                    }
                    fired.push(prop.clone());
                    self.remaining_start += 1;
                }
            }
        }
        // SilentFrameProgress casts toward zero before testing completion. In
        // reverse, frame -0.2 is still frame zero and has not completed.
        let frame = self.current_frame as i32;
        if frame < 0 || frame >= self.metadata.num_frames {
            if self.looping {
                if self.backwards {
                    self.current_frame += self.metadata.num_frames as f32;
                } else {
                    self.current_frame -= self.metadata.num_frames as f32;
                }
                // Source wraps once and never reloads time properties.
            } else {
                self.end_reached = true;
            }
        }
        fired
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AnimationTimeline {
    pub animations: Vec<AnimationState>,
    pub carry: Option<AnimationState>,
    pub left_hand: i16,
    pub right_hand: i16,
    pub bound_appearances: BTreeSet<String>,
}

impl AnimationTimeline {
    pub fn validate(&self) -> Result<(), AnimationError> {
        if self.animations.len() > MAX_ANIMATIONS
            || self.bound_appearances.len() > 4096
            || self.bound_appearances.iter().any(|s| s.len() > 16_384)
        {
            return Err(AnimationError::InvalidContinuation);
        }
        for state in &self.animations {
            state.validate()?;
        }
        if let Some(carry) = &self.carry {
            carry.validate()?;
        }
        Ok(())
    }

    /// Invoke from a VM animation primitive. This does not advance simulation
    /// time. The caller writes Event into the specified VM local/parameter.
    pub fn apply(
        &mut self,
        command: &AnimationCommand,
        metadata: Option<&AnimationMetadata>,
    ) -> Result<AnimationResult, AnimationError> {
        self.validate()?;
        let mut next = self.clone();
        let result = next.apply_inner(command, metadata)?;
        next.validate()?;
        *self = next;
        Ok(result)
    }

    fn apply_inner(
        &mut self,
        command: &AnimationCommand,
        metadata: Option<&AnimationMetadata>,
    ) -> Result<AnimationResult, AnimationError> {
        if let Some(meta) = metadata {
            meta.validate()?;
            if meta.resource != command.resource {
                return Err(AnimationError::ResourceMismatch);
            }
        }
        if command.reset {
            if command.mode == AnimationMode::ClearCarryAndWait {
                self.carry = None;
                return Ok(AnimationResult::Complete);
            }
            if command.carrying && self.carry.is_none() && metadata.is_some() {
                command
                    .default_carry
                    .as_ref()
                    .ok_or(AnimationError::MissingDefaultCarry)?
                    .validate()?;
            }
            self.animations.clear();
            if let Some(meta) = metadata {
                let mut state = AnimationState::new(meta.clone(), command.backwards)?;
                state.speed = 30.0 / 25.0;
                state.looping = true;
                self.animations.push(state);
                self.left_hand = 0;
                self.right_hand = 0;
                if command.carrying {
                    if self.carry.is_none() {
                        self.carry = Some(AnimationState::new(
                            command
                                .default_carry
                                .clone()
                                .ok_or(AnimationError::MissingDefaultCarry)?,
                            false,
                        )?);
                    }
                } else {
                    self.carry = None;
                }
            }
            return Ok(AnimationResult::Complete);
        }
        let Some(meta) = metadata else {
            return Ok(AnimationResult::CompleteNextTick);
        };
        match command.mode {
            AnimationMode::Carry => {
                self.carry = Some(AnimationState::new(meta.clone(), false)?);
                Ok(AnimationResult::Complete)
            }
            AnimationMode::Loop => {
                let mut state = AnimationState::new(meta.clone(), command.backwards)?;
                state.speed = Self::command_speed(command);
                state.looping = true;
                self.animations = vec![state];
                self.left_hand = 0;
                self.right_hand = 0;
                Ok(AnimationResult::Complete)
            }
            AnimationMode::PlayAndWait | AnimationMode::ClearCarryAndWait => {
                if command.mode == AnimationMode::ClearCarryAndWait {
                    self.carry = None;
                }
                if self.animations.first().map(|s| &s.metadata) != Some(meta) {
                    let mut state = AnimationState::new(meta.clone(), command.backwards)?;
                    state.speed = Self::command_speed(command);
                    self.animations = vec![state];
                    self.left_hand = 0;
                    self.right_hand = 0;
                    return Ok(AnimationResult::Wait);
                }
                let current = &mut self.animations[0];
                if current.end_reached {
                    while current.events_run < command.expected_events {
                        current.event_queue.push_back(i16::from(current.events_run));
                        current.events_run += 1;
                    }
                }
                if let Some(event) = current.event_queue.pop_front() {
                    Ok(AnimationResult::Event(event))
                } else if current.end_reached {
                    self.animations.clear();
                    Ok(AnimationResult::Complete)
                } else {
                    Ok(AnimationResult::Wait)
                }
            }
        }
    }

    fn command_speed(command: &AnimationCommand) -> f32 {
        let speed = 30.0_f32 / 25.0;
        if command.hurryable && command.walk_style == 1 {
            speed * 2.0
        } else {
            speed
        }
    }

    pub fn tick(&mut self) -> Result<Vec<AnimationCue>, AnimationError> {
        self.validate()?;
        let mut next = self.clone();
        let cues = next.tick_inner()?;
        next.validate()?;
        *self = next;
        Ok(cues)
    }

    fn tick_inner(&mut self) -> Result<Vec<AnimationCue>, AnimationError> {
        let mut cues = Vec::new();
        for index in 0..self.animations.len() {
            let properties = self.animations[index].advance();
            for property in properties {
                let p = &property.properties;
                if let Some(value) = p.get("xevt") {
                    let code = value.trim().parse::<i16>().unwrap_or(0);
                    // HandleTimePropsEvent targets CurrentAnimationState even
                    // when a later blend layer emitted this record.
                    self.animations[0].event_queue.push_back(code);
                    if code < 100 {
                        self.animations[0].events_run =
                            self.animations[0].events_run.wrapping_add(1);
                    }
                    cues.push(AnimationCue::Xevt {
                        animation: index,
                        code,
                    });
                }
                if let Some(value) = p.get("righthand") {
                    let code =
                        value
                            .trim()
                            .parse()
                            .map_err(|_| AnimationError::InvalidGesture {
                                key: "righthand".into(),
                                value: value.clone(),
                            })?;
                    self.right_hand = code;
                    cues.push(AnimationCue::RightHand(code));
                }
                if let Some(value) = p.get("lefthand") {
                    let code =
                        value
                            .trim()
                            .parse()
                            .map_err(|_| AnimationError::InvalidGesture {
                                key: "lefthand".into(),
                                value: value.clone(),
                            })?;
                    self.left_hand = code;
                    cues.push(AnimationCue::LeftHand(code));
                }
                if let Some(value) = p.get("sound") {
                    cues.push(AnimationCue::Sound(value.clone()));
                }
                if let Some(value) = p.get("dress") {
                    self.bound_appearances.insert(value.clone());
                    cues.push(AnimationCue::Dress(value.clone()));
                }
                if let Some(value) = p.get("undress") {
                    self.bound_appearances.remove(value);
                    cues.push(AnimationCue::Undress(value.clone()));
                }
            }
        }
        // The source evaluates carry's silent frame but never advances it or
        // processes its time properties, so no continuation changes here.
        Ok(cues)
    }
}
