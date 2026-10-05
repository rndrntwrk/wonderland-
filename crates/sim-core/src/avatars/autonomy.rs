//! VMFindBestAction scoring over B-owned, already checked interaction offers.
use super::advertisements::{InteractionCandidate, OfferStatus};
use super::motives::{Motive, MotiveState};
use super::state::PersonData;
use super::AvatarPlatform;
use crate::ids::EntityRef;
use crate::rng::SimRng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const WEIGHT_MOTIVES: [Motive; 9] = [
    Motive::Energy,
    Motive::Comfort,
    Motive::Hunger,
    Motive::Hygiene,
    Motive::Bladder,
    Motive::Mood,
    Motive::Room,
    Motive::Social,
    Motive::Fun,
];
const VARY_PERSON: [usize; 23] = [
    19, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 19, 10, 11, 12, 19, 19, 15, 19, 17, 18,
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreCurve {
    pub points: Vec<(i32, i32)>,
}
impl ScoreCurve {
    pub fn new(mut points: Vec<(i32, i32)>) -> Result<Self, AutonomyError> {
        points.sort_by_key(|p| p.0);
        let result = Self { points };
        result.validate()?;
        Ok(result)
    }
    pub fn validate(&self) -> Result<(), AutonomyError> {
        if self.points.is_empty()
            || self.points.len() > 4096
            || self.points.windows(2).any(|p| p[0].0 > p[1].0)
        {
            return Err(AutonomyError::InvalidCurve);
        }
        Ok(())
    }
    /// TS1Curve.GetPoint uses source f32 operations without FMA.
    pub fn sample(&self, input: f32) -> f32 {
        if input < self.points[0].0 as f32 {
            return self.points[0].1 as f32;
        }
        let i = self
            .points
            .iter()
            .rposition(|p| input >= p.0 as f32)
            .unwrap_or(0);
        if i == self.points.len() - 1 {
            return self.points[i].1 as f32;
        }
        let start = self.points[i];
        let end = self.points[i + 1];
        let amount = (input - start.0 as f32) / (end.0.wrapping_sub(start.0) as f32);
        (1.0 - amount) * start.1 as f32 + amount * end.1 as f32
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutonomyTuning {
    pub adult_curves: [ScoreCurve; 9],
    pub child_curves: [ScoreCurve; 9],
}
impl AutonomyTuning {
    pub fn validate(&self) -> Result<(), AutonomyError> {
        for curve in self.adult_curves.iter().chain(self.child_curves.iter()) {
            curve.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedPriority {
    pub priority: i16,
    pub target: EntityRef,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutonomyContext {
    pub platform: AvatarPlatform,
    pub motives: MotiveState,
    pub person_data: PersonData,
    pub x: i16,
    pub y: i16,
    pub level: i8,
    /// Queue order, not priority order; source returns the first higher priority.
    pub queued: Vec<QueuedPriority>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScoredAction {
    pub target: EntityRef,
    pub interaction_id: u8,
    pub param0: i16,
    pub source_order: u64,
    pub score: f32,
    pub auto_first: bool,
    /// Each returned menu variant adds the FIRST variant again in the source.
    pub duplicated_variant: u16,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AutonomySelection {
    AlreadyQueued(EntityRef),
    Selected(ScoredAction),
    NoValidTarget,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutonomyError {
    InvalidCurve,
    InvalidCandidate,
    DuplicateSourceOrder,
    InvalidAvatar,
    NonFiniteScore,
}

/// Immutable scoring is safe for a UI offer query: no RNG or avatar mutation.
/// The result is already stably ranked, use-count filtered and limited to four.
pub fn score_candidates(
    context: &AutonomyContext,
    candidates: &[InteractionCandidate],
    tuning: &AutonomyTuning,
) -> Result<Vec<ScoredAction>, AutonomyError> {
    tuning.validate()?;
    context
        .person_data
        .validate()
        .map_err(|_| AutonomyError::InvalidAvatar)?;
    context
        .motives
        .validate()
        .map_err(|_| AutonomyError::InvalidAvatar)?;
    if candidates.len() > 65_536 {
        return Err(AutonomyError::InvalidCandidate);
    }
    let pd = &context.person_data.values;
    let child = context.platform == AvatarPlatform::Ts1 && pd[58] < 18;
    let visitor = pd[32] == 1;
    let stray =
        context.platform == AvatarPlatform::Ts1 && pd[65] & (8 | 16) > 0 && pd[34] == 0 && visitor;
    let curves = if child {
        &tuning.child_curves
    } else {
        &tuning.adult_curves
    };
    let weight = 1.0_f32 / 9.0;
    let parts: [f32; 9] = std::array::from_fn(|i| {
        curves[i].sample(f32::from(context.motives.get(WEIGHT_MOTIVES[i]))) * weight
    });
    // Enumerable.Sum(IEnumerable<float>) accumulates as double, returns float.
    let base = parts.iter().map(|p| f64::from(*p)).sum::<f64>() as f32;
    let min_score = if pd[0] > 0 { 1e-6_f64 } else { 1e-7_f64 };
    let mut ordinals = BTreeSet::new();
    let mut ordered: Vec<_> = candidates.iter().collect();
    for candidate in &ordered {
        if !ordinals.insert(candidate.source_order) {
            return Err(AutonomyError::DuplicateSourceOrder);
        }
        if candidate.target.object_id.0 <= 0
            || candidate.target.generation == 0
            || !candidate.attenuation_value.is_finite()
            || candidate.advertisements.len() > 16
            || candidate.variants.len() > 4096
        {
            return Err(AutonomyError::InvalidCandidate);
        }
        let mut motives = BTreeSet::new();
        for ad in &candidate.advertisements {
            if !motives.insert(ad.motive) {
                return Err(AutonomyError::InvalidCandidate);
            }
        }
    }
    ordered.sort_by_key(|c| c.source_order);
    let mut valid: Vec<(ScoredAction, bool)> = Vec::new();
    for candidate in ordered {
        if !candidate.in_world
            || candidate.disabled
            || candidate.status != OfferStatus::Available
            || (candidate.occupied && !candidate.joining_available)
            || (stray && (candidate.is_game_object || !candidate.outside))
        {
            continue;
        }
        let Some(first) = candidate.variants.first() else {
            continue;
        };
        let mut active: Vec<_> = candidate
            .advertisements
            .iter()
            .filter(|ad| ad.delta != 0)
            .collect();
        // TTAB assigns MotiveIndex from array position, so ActiveMotiveEntries
        // is ascending even if an adapter supplied these records in another order.
        active.sort_by_key(|ad| ad.motive);
        if active.is_empty() {
            continue;
        }
        let dx = i32::from(context.x) - i32::from(candidate.x);
        let dy = i32::from(context.y) - i32::from(candidate.y);
        let dz = (i32::from(context.level) - i32::from(candidate.level)) * 320;
        let distance = ((f64::from(dx).powi(2) + f64::from(dy).powi(2) + f64::from(dz).powi(2))
            .sqrt() as f32)
            / 16.0;
        let mut score = base;
        for ad in active {
            let mut minimum = ad.minimum;
            let mut maximum = ad.delta;
            let mut personality = ad.personality_modifier as i16;
            if let Some(changes) = &first.motive_ad_changes {
                minimum = *changes.get(&(ad.motive as u32)).unwrap_or(&0);
                maximum = *changes.get(&((1 << 16) | ad.motive as u32)).unwrap_or(&0);
                personality = *changes.get(&((2 << 16) | ad.motive as u32)).unwrap_or(&0);
            }
            if maximum == 0 && minimum > 0 {
                maximum = minimum;
                minimum = 0;
            }
            maximum = maximum.wrapping_add(minimum);
            let current = context.motives.get(ad.motive);
            if minimum != 0 && current > minimum {
                continue;
            }
            let Some(index) = WEIGHT_MOTIVES.iter().position(|m| *m == ad.motive) else {
                continue;
            };
            score -= parts[index];
            let mut multiplier = 1.0_f32;
            if personality > 0 && (personality as usize) < VARY_PERSON.len() {
                multiplier = f32::from(pd[VARY_PERSON[personality as usize]]) / 1000.0;
                if personality < 13 {
                    if personality & 1 == 0 {
                        multiplier = 1.0 - multiplier;
                    }
                } else {
                    multiplier *= 2.0;
                }
            }
            score += curves[index]
                .sample(f32::from(current) + (f32::from(maximum) * multiplier) / 1000.0)
                * weight;
        }
        score -= base;
        let attenuation = if candidate.attenuation_code == 0 || candidate.attenuation_code >= 5 {
            candidate.attenuation_value
        } else if visitor {
            [0.0, 0.0, 0.01, 0.02, 0.03][candidate.attenuation_code as usize]
        } else {
            [0.0, 0.0, 0.1, 0.3, 0.6][candidate.attenuation_code as usize]
        };
        score /= 1.0 + attenuation * distance;
        if !score.is_finite() {
            return Err(AutonomyError::NonFiniteScore);
        }
        if f64::from(score) > min_score {
            for i in 0..candidate.variants.len() {
                valid.push((
                    ScoredAction {
                        target: candidate.target,
                        interaction_id: candidate.interaction_id as u8,
                        param0: first.param0,
                        source_order: candidate.source_order,
                        score,
                        auto_first: candidate.auto_first,
                        duplicated_variant: i as u16,
                    },
                    candidate.use_count == 0 || candidate.joining_available_after_narrowing,
                ));
            }
        }
    }
    valid.sort_by(|a, b| b.0.score.total_cmp(&a.0.score));
    Ok(valid
        .into_iter()
        .filter(|(_, eligible)| *eligible)
        .take(4)
        .map(|(action, _)| action)
        .collect())
}

/// Selection consumes exactly one source RNG draw when a positive candidate
/// exists and the best candidate is not AutoFirst. B still owns enqueueing and
/// revalidation of the selected action against its live object generation.
pub fn select(
    context: &AutonomyContext,
    candidates: &[InteractionCandidate],
    tuning: &AutonomyTuning,
    rng: &mut SimRng,
) -> Result<AutonomySelection, AutonomyError> {
    select_with_random(context, candidates, tuning, |bound| rng.next(bound))
}

/// Host adapter for the same source selection algorithm. Callback invocation
/// count and bound are identical to `select`; it never runs on the queued,
/// AutoFirst or empty-result paths.
pub fn select_with_random<F: FnMut(u64) -> u64>(
    context: &AutonomyContext,
    candidates: &[InteractionCandidate],
    tuning: &AutonomyTuning,
    mut next_random: F,
) -> Result<AutonomySelection, AutonomyError> {
    context
        .person_data
        .validate()
        .map_err(|_| AutonomyError::InvalidAvatar)?;
    if let Some(queued) = context
        .queued
        .iter()
        .find(|q| q.priority > context.person_data.values[33])
    {
        if queued.target.object_id.0 <= 0 || queued.target.generation == 0 {
            return Err(AutonomyError::InvalidCandidate);
        }
        return Ok(AutonomySelection::AlreadyQueued(queued.target));
    }
    let sorted = score_candidates(context, candidates, tuning)?;
    let Some(first) = sorted.first() else {
        return Ok(AutonomySelection::NoValidTarget);
    };
    if first.auto_first {
        return Ok(AutonomySelection::Selected(first.clone()));
    }
    let total = sorted.iter().map(|s| f64::from(s.score)).sum::<f64>() as f32;
    if !total.is_finite() || total <= 0.0 {
        return Err(AutonomyError::NonFiniteScore);
    }
    let random = next_random(10_000) as f32;
    let mut running = 0.0_f32;
    for action in &sorted {
        running += (action.score / total) * 10_000.0;
        if random <= running {
            return Ok(AutonomySelection::Selected(action.clone()));
        }
    }
    Ok(AutonomySelection::Selected(first.clone()))
}
