//! Authoritative source locomotion, independent of renderer or wall-clock time.
//!
//! VMRoutingFrame (source 4c6b3e8) supplies acceleration, steering, blend weights
//! and turn boundaries. VMPathLineSegment supplies integer 0x8000-subtile
//! interpolation. The existing grid pathfinder still chooses the line endpoints;
//! this module does not claim VMRectRouter/Bezier path-selection parity.
use crate::ids::EntityRef;
use crate::world::{tiles::integer_sqrt, Facing, LotPosition};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceRoutingState {
    pub routes: BTreeMap<u64, RouteMotionRecord>,
    pub avatars: BTreeMap<EntityRef, SourceAvatarMotion>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RouteMotionRecord {
    Pending { actor: EntityRef, auto_run: bool },
    Unavailable { actor: EntityRef, reason: String },
    Active(Box<SourceRouteMotion>),
}
impl RouteMotionRecord {
    pub fn actor(&self) -> EntityRef {
        match self {
            Self::Pending { actor, .. } | Self::Unavailable { actor, .. } => *actor,
            Self::Active(value) => value.actor,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceAvatarMotion {
    pub position: LotPosition,
    pub facing: Facing,
    pub radians: f32,
    pub visual_position: [f32; 3],
    pub velocity: [f32; 3],
    pub turn_velocity: f64,
}

impl SourceAvatarMotion {
    pub fn validate(&self) -> bool {
        self.radians.is_finite() && self.radians.abs() <= 100.
            && self.facing == facing(self.radians)
            && self.turn_velocity.is_finite()
            && self.visual_position.iter().chain(&self.velocity).all(|n| n.is_finite() && n.abs() <= 100_000.)
            // Integer SetPosition truncates the same high-precision source
            // position. Allow only its one-subtile envelope plus f32 slack.
            && (self.visual_position[0] * 16. - self.position.x as f32).abs() <= 1.001
            && (self.visual_position[1] * 16. - self.position.y as f32).abs() <= 1.001
            && self.visual_position[2] == (f32::from(self.position.level) - 1.) * 2.95
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotionPhase {
    Ready,
    BeginTurn,
    Walking,
    EndTurn,
    Complete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotionAnimation {
    None,
    Walk,
    Turn(u8),
    Stand,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotionResources {
    /// Standing, full locomotion, half locomotion, then source turn indices4..9.
    pub names: [String; 9],
    pub turn_frames: [i32; 6],
    pub walk_style: i16,
    pub in_pool: bool,
    pub pet_slow: bool,
    pub is_pet: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLine {
    pub from: LotPosition,
    pub to: LotPosition,
    pub frames: i32,
    pub total_frames: i32,
    pub last: bool,
}
impl SourceLine {
    pub fn new(
        from: LotPosition,
        to: LotPosition,
        last: bool,
        resources: &MotionResources,
    ) -> Self {
        // Source truncates sqrt(hiP distance)/0x8000 BEFORE multiplying by10.
        let mut total = (integer_sqrt(from.distance_squared(to)) as i32 * 10).max(1);
        if resources.in_pool || resources.pet_slow {
            total *= 2;
        }
        total = (total / if resources.walk_style == 1 { 3 } else { 1 }).max(1);
        Self {
            from,
            to,
            frames: 0,
            total_frames: total,
            last,
        }
    }
    pub fn position(&self) -> LotPosition {
        let coordinate = |from: i32, to: i32| {
            let from = i64::from(from) * 0x8000;
            let diff = i64::from(to) * 0x8000 - from;
            ((from + diff * i64::from(self.frames) / i64::from(self.total_frames)) / 0x8000) as i32
        };
        LotPosition::new(
            coordinate(self.from.x, self.to.x),
            coordinate(self.from.y, self.to.y),
            self.from.level,
        )
    }
    pub fn visual(&self) -> [f32; 3] {
        let f = self.frames as f32 / self.total_frames as f32;
        [
            (self.from.x as f32 + (self.to.x - self.from.x) as f32 * f) / 16.,
            (self.from.y as f32 + (self.to.y - self.from.y) as f32 * f) / 16.,
            f32::from(self.from.level - 1) * 2.95,
        ]
    }
}

/// Immutable playback ownership selected or retained by StartWalkAnimation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceWalkLayer {
    pub resource: String,
    pub speed: f32,
    pub looping: bool,
    pub backwards: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceRouteMotion {
    pub actor: EntityRef,
    pub resources: MotionResources,
    pub restore_walk_style: Option<i16>,
    pub phase: MotionPhase,
    pub line: Option<SourceLine>,
    pub velocity: u8,
    /// The source writes weights before steering can reduce next-tick velocity.
    pub blend_velocity: u8,
    pub radians: f32,
    pub target_radians: f64,
    pub turn_tweak: f32,
    pub animation: MotionAnimation,
    pub animation_revision: u64,
    /// Source may retain a preexisting three-layer set by its first two names.
    pub walk_reused: bool,
    pub walk_layers: Option<[SourceWalkLayer; 3]>,
    pub visual: SourceAvatarMotion,
}
impl SourceRouteMotion {
    pub fn new(
        actor: EntityRef,
        position: LotPosition,
        facing: Facing,
        resources: MotionResources,
    ) -> Self {
        let radians = f32::from(facing.0) * std::f32::consts::FRAC_PI_4;
        Self {
            actor,
            resources,
            restore_walk_style: None,
            phase: MotionPhase::Ready,
            line: None,
            velocity: 0,
            blend_velocity: 0,
            radians,
            target_radians: f64::from(radians),
            turn_tweak: 0.,
            animation: MotionAnimation::None,
            animation_revision: 0,
            walk_reused: false,
            walk_layers: None,
            visual: SourceAvatarMotion {
                position,
                facing,
                radians,
                visual_position: [
                    position.x as f32 / 16.,
                    position.y as f32 / 16.,
                    f32::from(position.level - 1) * 2.95,
                ],
                velocity: [0.; 3],
                turn_velocity: 0.,
            },
        }
    }
    fn select_animation(&mut self, animation: MotionAnimation) {
        self.animation = animation;
        self.animation_revision += 1;
    }
    pub fn begin_tick(&mut self) {
        self.visual.velocity = [0.; 3];
        self.visual.turn_velocity = 0.;
    }
    pub fn turn_selection(difference: f64) -> Option<(u8, f32)> {
        let abs = difference.abs();
        let off = u8::from(difference <= 0.);
        let (index, tweak) = if abs >= std::f64::consts::PI - 0.01 {
            (0, 0.)
        } else if abs >= std::f64::consts::FRAC_PI_2 - 0.01 {
            (2, (abs - std::f64::consts::FRAC_PI_2) as f32)
        } else if abs >= std::f64::consts::FRAC_PI_4 - 0.01 {
            (4, (abs - std::f64::consts::FRAC_PI_4) as f32)
        } else {
            return None;
        };
        Some((index + off, if off == 1 { -tweak } else { tweak }))
    }
    fn start_turn(&mut self, target: f64, end: bool) -> bool {
        self.target_radians = target;
        let Some((index, tweak)) =
            Self::turn_selection(difference(f64::from(self.radians), target))
        else {
            return false;
        };
        let speed = if self.resources.walk_style == 1 {
            2.
        } else {
            1.
        };
        self.turn_tweak = tweak / (self.resources.turn_frames[index as usize] as f32 / speed);
        self.phase = if end {
            MotionPhase::EndTurn
        } else {
            MotionPhase::BeginTurn
        };
        self.select_animation(MotionAnimation::Turn(index));
        true
    }
    pub fn turn_tick(&mut self, ended: bool) -> bool {
        if !ended {
            self.radians += self.turn_tweak;
            self.visual.turn_velocity = f64::from(self.turn_tweak);
            self.sync_visual(self.visual.position);
            return false;
        }
        self.radians = self.target_radians as f32;
        if self.phase == MotionPhase::EndTurn {
            self.phase = MotionPhase::Complete;
            self.select_animation(MotionAnimation::Stand);
        } else {
            self.phase = MotionPhase::Walking;
            self.select_animation(MotionAnimation::Walk);
        }
        self.sync_visual(self.visual.position);
        true
    }
    pub fn begin_line(&mut self, from: LotPosition, to: LotPosition, last: bool) -> bool {
        self.line = Some(SourceLine::new(from, to, last, &self.resources));
        if self.phase == MotionPhase::Ready {
            self.velocity = 0;
            let target = f64::from(to.x - from.x).atan2(f64::from(from.y - to.y));
            if self.start_turn(target, false) {
                // BEGIN_WALK returns CONTINUE, so BEGIN_TURN applies the first
                // tweak before this tick's first animation-frame advance.
                self.turn_tick(false);
                return false;
            }
            self.radians = target as f32;
            self.phase = MotionPhase::Walking;
            self.select_animation(MotionAnimation::Walk);
            return true;
        }
        // AdvanceWaypoint while WALKING preserves the previous direction; the
        // source steering below converges it to the next line's tangent.
        self.target_radians = f64::from(self.radians);
        true
    }
    pub fn begin_end(&mut self, facing: Option<Facing>) -> bool {
        if self.phase == MotionPhase::Complete {
            return true;
        }
        if let Some(facing) = facing {
            let target = f64::from(facing.0) * std::f64::consts::FRAC_PI_4;
            let turn_only = self.phase == MotionPhase::Ready;
            if self.start_turn(target, true) {
                // TURN_ONLY returns CONTINUE; a final WALKING step returns
                // NEXT_TICK after EndWalk and does not apply this first tweak.
                if turn_only {
                    self.turn_tick(false);
                }
                return false;
            }
            self.radians = target as f32;
        }
        self.phase = MotionPhase::Complete;
        self.sync_visual(self.visual.position);
        true
    }
    pub fn advance_line(&mut self) -> (LotPosition, bool) {
        let line = self.line.as_mut().expect("source line initialized");
        if line.last && line.total_frames - line.frames <= 28 {
            self.velocity = self.velocity.max(1);
            if self.velocity > 1 {
                self.velocity -= 1;
            }
        } else if self.velocity < 8 {
            self.velocity += 1;
        }
        self.blend_velocity = self.velocity;
        line.frames = (line.frames + i32::from(self.velocity)).min(line.total_frames);
        let position = line.position();
        let visual_position = line.visual();
        let done = line.frames == line.total_frames;
        let vx = (line.to.x - line.from.x) as f32 / line.total_frames as f32 / 16.
            * f32::from(self.velocity);
        let vy = (line.to.y - line.from.y) as f32 / line.total_frames as f32 / 16.
            * f32::from(self.velocity);
        self.radians = self.target_radians as f32;
        let new_target = f64::from(vx).atan2(f64::from(-vy));
        let mut diff = difference(new_target, f64::from(self.radians));
        let abs = diff.abs();
        if abs > 0.25 {
            diff = if diff > 0. { 0.25 } else { -0.25 };
            if abs > 0.5 {
                if self.velocity > 4 {
                    self.velocity -= 2;
                }
                if abs > 1. {
                    if self.velocity > 2 {
                        self.velocity -= 2;
                    }
                    if abs > std::f64::consts::FRAC_PI_2 {
                        self.velocity = 0;
                    }
                }
            }
        }
        self.target_radians = normalize(f64::from(self.radians) - diff);
        self.sync_visual(position);
        self.visual.visual_position = visual_position;
        self.visual.velocity = [vx, vy, 0.];
        self.visual.turn_velocity = diff;
        (position, done)
    }
    pub fn wait_tick(&mut self) {
        self.velocity = self.velocity.saturating_sub(1);
        self.blend_velocity = self.velocity;
        if self.animation != MotionAnimation::Walk {
            self.select_animation(MotionAnimation::Walk);
        }
    }
    pub fn replan(&mut self) {
        self.line = None;
        self.phase = MotionPhase::Ready;
        self.turn_tweak = 0.;
    }
    pub fn sync_visual(&mut self, position: LotPosition) {
        self.visual.position = position;
        self.visual.facing = facing(self.radians);
        self.visual.radians = self.radians;
    }
    pub fn validate(&self) -> bool {
        self.actor.is_valid()
            && self.visual.validate()
            && self.visual.radians == self.radians
            && (!self.resources.pet_slow || self.resources.is_pet)
            && self.walk_layers.as_ref().map_or(true, |layers| {
                layers.iter().all(|layer| {
                    !layer.resource.is_empty()
                        && layer.resource.len() <= 1024
                        && layer.speed.is_finite()
                })
            })
            && self.animation_revision <= 4_000_000
            && self.velocity <= 8
            && self.blend_velocity <= 8
            && self.radians.is_finite()
            && self.target_radians.is_finite()
            && self.turn_tweak.is_finite()
            && self.radians.abs() <= 100.
            && self.target_radians.abs() <= 100.
            && self
                .resources
                .names
                .iter()
                .all(|s| !s.is_empty() && s.len() <= 1024)
            && self
                .resources
                .turn_frames
                .iter()
                .all(|n| (1..=1_000_000).contains(n))
            && self.line.as_ref().map_or(true, |l| {
                l.from.level == l.to.level
                    && l.from != l.to
                    && l.total_frames > 0
                    && (0..=l.total_frames).contains(&l.frames)
                    && l.total_frames
                        == SourceLine::new(l.from, l.to, l.last, &self.resources).total_frames
            })
            && match (self.phase, self.animation) {
                (MotionPhase::BeginTurn | MotionPhase::EndTurn, MotionAnimation::Turn(i)) => i < 6,
                (MotionPhase::BeginTurn | MotionPhase::EndTurn, _) => false,
                (_, MotionAnimation::Turn(i)) => i < 6,
                _ => true,
            }
    }
}
pub fn normalize(radians: f64) -> f64 {
    let value = (radians % std::f64::consts::TAU + std::f64::consts::TAU) % std::f64::consts::TAU;
    if value > std::f64::consts::PI {
        value - std::f64::consts::TAU
    } else {
        value
    }
}
pub fn difference(from: f64, to: f64) -> f64 {
    normalize(to - from)
}
pub fn facing(radians: f32) -> Facing {
    // VMAvatar.Direction uses Math.Round (ties-to-even), not integer truncation.
    let notch =
        crate::numeric::round_ties_even(f64::from(radians) / std::f64::consts::FRAC_PI_4) as i32;
    Facing(notch.rem_euclid(8) as u8)
}
