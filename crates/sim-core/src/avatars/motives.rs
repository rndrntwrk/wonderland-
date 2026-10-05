//! Motive indices, clamps, source-width accumulation and the two platform clocks.
use super::AvatarPlatform;
use crate::numeric::{legacy_f64_to_i16, legacy_f64_to_i32};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum Motive {
    HappyLife = 0,
    HappyWeek = 1,
    HappyDay = 2,
    Mood = 3,
    UnusedPhysical = 4,
    Energy = 5,
    Comfort = 6,
    Hunger = 7,
    Hygiene = 8,
    Bladder = 9,
    UnusedMental = 10,
    SleepState = 11,
    UnusedStress = 12,
    Room = 13,
    Social = 14,
    Fun = 15,
}

pub const ALL_MOTIVES: [Motive; 16] = [
    Motive::HappyLife,
    Motive::HappyWeek,
    Motive::HappyDay,
    Motive::Mood,
    Motive::UnusedPhysical,
    Motive::Energy,
    Motive::Comfort,
    Motive::Hunger,
    Motive::Hygiene,
    Motive::Bladder,
    Motive::UnusedMental,
    Motive::SleepState,
    Motive::UnusedStress,
    Motive::Room,
    Motive::Social,
    Motive::Fun,
];
pub const DECAY_MOTIVES: [Motive; 7] = [
    Motive::Hunger,
    Motive::Comfort,
    Motive::Hygiene,
    Motive::Bladder,
    Motive::Energy,
    Motive::Fun,
    Motive::Social,
];

impl TryFrom<u8> for Motive {
    type Error = MotiveError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        ALL_MOTIVES
            .get(value as usize)
            .copied()
            .ok_or(MotiveError::InvalidIndex(value))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MotiveChange {
    pub per_hour_change: i16,
    pub max_value: i16,
    pub fractional: f64,
    /// The first assignment in a tick wins. Source constructors leave this false.
    pub ticked: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MotiveState {
    pub values: [i16; 16],
    pub limits: [i16; 16],
    pub changes: [MotiveChange; 16],
}

impl Default for MotiveState {
    fn default() -> Self {
        let mut values = [100; 16];
        values[Motive::SleepState as usize] = 0;
        Self {
            values,
            limits: [100; 16],
            changes: std::array::from_fn(|_| MotiveChange::default()),
        }
    }
}

impl MotiveState {
    pub fn validate(&self) -> Result<(), MotiveError> {
        if self
            .changes
            .iter()
            .any(|c| !c.fractional.is_finite() || c.fractional.abs() > i32::MAX as f64)
        {
            return Err(MotiveError::InvalidContinuation);
        }
        Ok(())
    }
    pub fn get(&self, motive: Motive) -> i16 {
        self.values[motive as usize]
    }
    pub fn set(&mut self, motive: Motive, value: i16) {
        let index = motive as usize;
        self.values[index] = value
            .min(self.values[index].max(self.limits[index]))
            .max(-100);
    }
    pub fn scale_max(&self, motive: Motive, max: i16) -> i16 {
        (i32::from(max) - 100 + i32::from(self.limits[motive as usize])) as i16
    }
    pub fn has_change(&self, motive: Motive) -> bool {
        self.changes[motive as usize].per_hour_change != 0
    }
    pub fn set_change(&mut self, motive: Motive, rate: i16, max: i16) -> bool {
        let change = &mut self.changes[motive as usize];
        if !change.ticked {
            return false;
        }
        change.per_hour_change = rate;
        change.max_value = max;
        change.ticked = false;
        true
    }
    pub fn clear_changes(&mut self) {
        for change in &mut self.changes {
            change.per_hour_change = 0;
            change.max_value = i16::MAX;
        }
    }
    /// VMSetMotiveChange.Once: addition narrows to short before testing the cap.
    pub fn change_once(&mut self, motive: Motive, rate: i16, max: i16) {
        let old = self.get(motive);
        if beyond(old, max, rate) {
            return;
        }
        let mut value = old.wrapping_add(rate);
        if beyond(value, max, rate) {
            value = max;
        }
        self.set(motive, value);
    }
    pub fn tick_changes(&mut self, platform: AvatarPlatform) {
        for motive in ALL_MOTIVES {
            let old = self.get(motive);
            let change = &mut self.changes[motive as usize];
            change.ticked = true;
            if change.per_hour_change == 0 {
                continue;
            }
            let mut rate = (f64::from(change.per_hour_change) / 60.0) / 30.0;
            if platform == AvatarPlatform::Tso {
                rate /= 5.0;
            }
            change.fractional += rate;
            if change.fractional.abs() < 1.0 {
                continue;
            }
            if beyond(old, change.max_value, change.per_hour_change) {
                continue;
            }
            // C# floating conversion truncates, then short compound addition wraps.
            let mut value = old.wrapping_add(legacy_f64_to_i16(change.fractional));
            change.fractional %= 1.0;
            if beyond(value, change.max_value, change.per_hour_change) {
                value = change.max_value;
            }
            self.set(motive, value);
        }
    }
}

fn beyond(value: i16, max: i16, rate: i16) -> bool {
    (rate > 0 && value > max) || (rate < 0 && value < max)
}

/// The C# cast occurs before division. Do not replace with `(a*b/1000) as i32`.
pub fn fractional_multiply(input: i32, fraction: i32) -> i32 {
    ((i64::from(input) * i64::from(fraction)) as i32) / 1000
}
pub fn fixed_1000(value: f32) -> i32 {
    legacy_f64_to_i32(f64::from(value * 1000.0))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotiveError {
    InvalidIndex(u8),
    MissingTsoTuning,
    InvalidTuning,
    UnsupportedRateMotive(Motive),
    InvalidContinuation,
}

/// Exact flattened global tuning inputs. No fabricated TSO tuning is substituted
/// when the B-owned global resources have not been supplied.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TsoMotiveTuning {
    /// Hunger ratio; comfort active; hygiene asleep/awake; bladder asleep/awake;
    /// hunger-to-bladder; energy span; integer wake hours; fun; social base/mul.
    pub flat_sim: [i32; 12],
    /// Categories None, Money, Offbeat, Romance, Services, Shopping, Skills,
    /// Welcome, Games, Entertain, Residence, in DECAY_MOTIVES order.
    pub category_weights: [[i32; 7]; 11],
}
impl TsoMotiveTuning {
    pub fn validate(&self) -> Result<(), MotiveError> {
        let denominator = 60_i32.wrapping_mul(self.flat_sim[8]);
        if denominator == 0 || (denominator == -1 && self.flat_sim[7] == i32::MIN) {
            return Err(MotiveError::InvalidTuning);
        }
        Ok(())
    }
    pub fn scale_rate(
        &self,
        mut rate: i32,
        motive: Motive,
        category: u8,
    ) -> Result<i32, MotiveError> {
        if rate < 0 {
            return Ok(rate);
        }
        if category == 4 && motive as u8 > 0 {
            rate = rate.wrapping_mul(3) / 2;
        }
        if motive == Motive::Comfort {
            return Ok(rate);
        }
        let index = DECAY_MOTIVES
            .iter()
            .position(|m| *m == motive)
            .ok_or(MotiveError::UnsupportedRateMotive(motive))?;
        let weight =
            self.category_weights[if category > 10 { 0 } else { category as usize }][index];
        let numerator = rate.wrapping_mul(1000);
        if weight == 0 || (weight == -1 && numerator == i32::MIN) {
            return Err(MotiveError::InvalidTuning);
        }
        Ok(numerator / weight)
    }
}
pub fn scale_ts1_rate(rate: i32, motive: Motive) -> i32 {
    if motive == Motive::Energy && rate > 0 {
        rate.wrapping_mul(180 / (24 - 16))
    } else {
        rate
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecayContext {
    pub minute: i32,
    pub hour: i32,
    pub room_score: i16,
    pub category: u8,
    pub cheats: i16,
    pub hidden: i16,
    pub active_personality: i16,
    pub outgoing_personality: i16,
}
impl Default for DecayContext {
    fn default() -> Self {
        Self {
            minute: 0,
            hour: 0,
            room_score: 100,
            category: 0,
            cheats: 0,
            hidden: 0,
            active_personality: 0,
            outgoing_personality: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TsoMotiveDecay {
    pub last_minute: i32,
    pub fractions: [i16; 7],
    pub last_category: i32,
    pub cached_weights: [i32; 7],
    pub tuning: Option<TsoMotiveTuning>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ts1MotiveDecay {
    pub last_minute: i32,
    pub fractions: [i16; 7],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
// Keep the public snapshot state inline; boxing solely for lint size would change its API.
#[allow(clippy::large_enum_variant)]
pub enum MotiveDecay {
    Tso(TsoMotiveDecay),
    Ts1(Ts1MotiveDecay),
}

impl MotiveDecay {
    pub fn tso(tuning: Option<TsoMotiveTuning>) -> Self {
        Self::Tso(TsoMotiveDecay {
            last_minute: 0,
            fractions: [0; 7],
            last_category: -1,
            cached_weights: [0; 7],
            tuning,
        })
    }
    pub fn ts1() -> Self {
        Self::Ts1(Ts1MotiveDecay::default())
    }
    pub fn fractions(&self) -> &[i16; 7] {
        match self {
            Self::Tso(s) => &s.fractions,
            Self::Ts1(s) => &s.fractions,
        }
    }
    pub fn validate(&self) -> Result<(), MotiveError> {
        if let Self::Tso(state) = self {
            if let Some(tuning) = &state.tuning {
                tuning.validate()?;
            }
            if !(-1..=255).contains(&state.last_category) {
                return Err(MotiveError::InvalidContinuation);
            }
        }
        Ok(())
    }
    pub fn tick(
        &mut self,
        motives: &mut MotiveState,
        ctx: &DecayContext,
    ) -> Result<(), MotiveError> {
        match self {
            Self::Tso(s) => s.tick(motives, ctx),
            Self::Ts1(s) => {
                s.tick(motives, ctx);
                Ok(())
            }
        }
    }
}

impl TsoMotiveDecay {
    fn tick(&mut self, m: &mut MotiveState, c: &DecayContext) -> Result<(), MotiveError> {
        // Validate required tuning before mutation so missing resources do not
        // leave a partially applied tick in the caller's authoritative state.
        if c.minute != self.last_minute && c.cheats <= 0 {
            self.tuning
                .as_ref()
                .ok_or(MotiveError::MissingTsoTuning)?
                .validate()?;
        }
        m.set(Motive::Room, c.room_score);
        if c.minute == self.last_minute || c.cheats > 0 {
            return Ok(());
        }
        let tuning = self.tuning.as_ref().ok_or(MotiveError::MissingTsoTuning)?;
        self.last_minute = c.minute;
        if i32::from(c.category) != self.last_category {
            self.cached_weights = tuning.category_weights[if c.category > 10 {
                0
            } else {
                c.category as usize
            }];
            self.last_category = i32::from(c.category);
        }
        let f = tuning.flat_sim;
        let awake = usize::from(m.get(Motive::SleepState) == 0);
        let mut sum = 0_i32;
        for (i, motive) in DECAY_MOTIVES.into_iter().enumerate() {
            let weight = self.cached_weights[i];
            let mut value = m.get(motive);
            // Recomputed on each iteration: bladder observes hunger after its
            // earlier decay in this same minute.
            let hunger = fractional_multiply(
                f[0].wrapping_mul(100 + i32::from(m.get(Motive::Hunger))),
                self.cached_weights[0],
            );
            let fraction = match i {
                0 => hunger,
                1 => fractional_multiply(f[1], weight),
                2 => fractional_multiply(f[2 + awake], weight),
                3 => fractional_multiply(f[4 + awake], weight)
                    .wrapping_add(fractional_multiply(hunger, f[6])),
                4 => f[7] / 60_i32.wrapping_mul(f[8]),
                5 => {
                    if awake == 0 {
                        0
                    } else {
                        fractional_multiply(f[9], weight)
                    }
                }
                6 => {
                    f[10].wrapping_add(fractional_multiply(
                        f[11].wrapping_mul(100 + i32::from(value)),
                        weight,
                    )) / 2
                }
                _ => unreachable!(),
            };
            self.fractions[i] = self.fractions[i].wrapping_add(fraction as i16);
            if self.fractions[i] >= 1000 {
                value = value.wrapping_sub(self.fractions[i] / 1000);
                self.fractions[i] %= 1000;
                value = value.max(-100);
                m.set(motive, value);
            }
            sum += i32::from(value);
        }
        sum += i32::from(c.room_score);
        m.set(Motive::Mood, (sum / 8) as i16);
        Ok(())
    }
}

impl Ts1MotiveDecay {
    fn tick(&mut self, m: &mut MotiveState, c: &DecayContext) {
        m.set(Motive::Room, c.room_score);
        if c.minute / 2 == self.last_minute || c.hidden > 0 {
            return;
        }
        self.last_minute = c.minute / 2;
        let sleeping = m.get(Motive::SleepState) != 0;
        let mut sum = 0_i32;
        for (i, motive) in DECAY_MOTIVES.into_iter().enumerate() {
            if m.has_change(motive) && motive != Motive::Energy {
                continue;
            }
            let mut value = m.get(motive);
            let hunger = fixed_1000(0.0021).wrapping_mul(100 + i32::from(m.get(Motive::Hunger)));
            let fraction = match i {
                0 => hunger,
                1 => {
                    if c.active_personality > 666 {
                        fixed_1000(0.4)
                    } else if c.active_personality < 666 {
                        fixed_1000(0.6)
                    } else {
                        fixed_1000(0.5)
                    }
                }
                2 => {
                    if sleeping {
                        fixed_1000(0.08)
                    } else {
                        fixed_1000(0.17)
                    }
                }
                3 => (if sleeping {
                    fixed_1000(0.15)
                } else {
                    fixed_1000(0.3)
                })
                .wrapping_add(fractional_multiply(hunger, fixed_1000(0.3))),
                4 => {
                    if sleeping && c.hour == 7 && value >= 80 {
                        m.set(Motive::SleepState, 0);
                    }
                    if sleeping {
                        if m.get(Motive::SleepState) == -1 {
                            -643 * 2
                        } else if c.hour >= 7 {
                            fixed_1000(0.01)
                        } else {
                            0
                        }
                    } else {
                        fixed_1000(180.0) / (30 * 16)
                    }
                }
                5 => {
                    if sleeping {
                        0
                    } else {
                        fixed_1000(0.25)
                    }
                }
                // 0.000125 becomes zero before multiplication, a source quirk.
                6 => fixed_1000(0.055).wrapping_add(
                    fixed_1000(0.000125).wrapping_mul(i32::from(c.outgoing_personality)),
                ),
                _ => unreachable!(),
            };
            self.fractions[i] = self.fractions[i].wrapping_add(fraction as i16);
            if self.fractions[i] >= 1000 {
                value = value.wrapping_sub(self.fractions[i] / 1000);
                self.fractions[i] %= 1000;
                value = value.max(-100);
                m.set(motive, value);
            }
            if self.fractions[i] < 0 {
                let amount = 1 - self.fractions[i] / 1000;
                value = value.wrapping_add(amount);
                self.fractions[i] = self.fractions[i].wrapping_add(amount.wrapping_mul(1000));
                m.set(motive, value);
            }
            sum += i32::from(value);
        }
        sum += i32::from(c.room_score);
        m.set(Motive::Mood, (sum / 8) as i16);
    }
}
