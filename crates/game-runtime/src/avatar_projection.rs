//! Read-only current avatar presentation. No simulation commands or visual history.
use crate::sim_core::avatars::{outfits::OutfitState, timeline::AnimationState};
use crate::{EntityRef, GameRuntime};
use serde::Serialize;
use wonderland_render_core::AssetKey;
use wonderland_world_view::WorldRevision;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AvatarAnimation {
    pub resource: String,
    pub num_frames: u32,
    pub current_frame: f32,
    pub speed: f32,
    pub weight: f32,
    pub backwards: bool,
    pub end_reached: bool,
    pub looping: bool,
}
impl From<&AnimationState> for AvatarAnimation {
    fn from(state: &AnimationState) -> Self {
        Self {
            resource: state.metadata.resource.clone(),
            num_frames: state.metadata.num_frames as u32,
            current_frame: state.current_frame,
            speed: state.speed,
            weight: state.weight,
            backwards: state.backwards,
            end_reached: state.end_reached,
            looping: state.looping,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct AvatarTimeline {
    pub layers: Vec<AvatarAnimation>,
    pub carry: Option<AvatarAnimation>,
    pub left_hand: i16,
    pub right_hand: i16,
    pub bound_appearances: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AvatarVisual {
    pub entity: EntityRef,
    pub guid: u32,
    pub visual_revision: u64,
    pub container: Option<(EntityRef, u16)>,
    pub scale_percent: i16,
    pub display_flags: i16,
    pub ghost: bool,
    pub outfits: OutfitState,
    pub animations: AvatarTimeline,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AvatarVisualFrame {
    pub revision: WorldRevision,
    pub avatars: Vec<AvatarVisual>,
}
impl GameRuntime {
    /// Immutable current-state sampling, not serialized pre-save visual bone history.
    pub fn avatar_visual_frame(&self) -> AvatarVisualFrame {
        let state = self.sim().state();
        AvatarVisualFrame {
            revision: WorldRevision {
                lot_id: Some(state.lot_id),
                epoch: state.authority_epoch,
                tick: state.completed_tick,
                architecture_revision: state.world.lot.revision().architecture,
                content: AssetKey(state.content.content_hash),
            },
            avatars: state.entities.values().filter_map(visual).collect(),
        }
    }
}

fn visual(entity: &crate::sim_core::state::EntityState) -> Option<AvatarVisual> {
    let avatar = entity.avatar.as_ref()?;
    Some(AvatarVisual {
        entity: entity.info.reference,
        guid: entity.info.guid,
        visual_revision: entity.revision,
        container: entity.container,
        scale_percent: avatar.read_person_data(63).unwrap_or(0),
        display_flags: avatar.read_person_data(74).unwrap_or(0),
        ghost: avatar.read_person_data(68).unwrap_or(0) > 0,
        outfits: avatar.outfits.clone(),
        animations: AvatarTimeline {
            layers: avatar
                .animations
                .animations
                .iter()
                .map(AvatarAnimation::from)
                .collect(),
            carry: avatar.animations.carry.as_ref().map(AvatarAnimation::from),
            left_hand: avatar.animations.left_hand,
            right_hand: avatar.animations.right_hand,
            bound_appearances: avatar
                .animations
                .bound_appearances
                .iter()
                .cloned()
                .collect(),
        },
    })
}

/// A bounded, optional trace of accepted presentation inputs. It never executes
/// poses or changes consensus. None means the consumer must reset visual history.
/// Limits account encoded metadata + fixed Rust records (not total browser heap).
pub(crate) struct AvatarCapture {
    frames: Option<Vec<AvatarVisualFrame>>,
    bytes: u64,
    records: usize,
}
impl AvatarCapture {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            frames: enabled.then(Vec::new),
            bytes: 8 * 1024 * 1024,
            records: 16_384,
        }
    }
    pub(crate) fn finish(self) -> Option<Vec<AvatarVisualFrame>> {
        self.frames
    }
    pub(crate) fn observe(&mut self, runtime: &GameRuntime) {
        if self.frames.is_none() {
            return;
        }
        let result = self.capture(runtime);
        if let Some(frame) = result {
            if let Some(frames) = self.frames.as_mut() {
                frames.push(frame);
            }
        } else {
            // Discard the WHOLE trace, not a misleading accepted prefix. Native
            // transitions still commit; the optional visual history is unavailable.
            self.frames = None;
        }
    }
    fn capture(&mut self, runtime: &GameRuntime) -> Option<AvatarVisualFrame> {
        self.bytes = self
            .bytes
            .checked_sub(std::mem::size_of::<AvatarVisualFrame>() as u64)?;
        let state = runtime.sim().state();
        let mut avatars = Vec::new();
        for entity in state.entities.values().filter(|e| e.avatar.is_some()) {
            self.records = self.records.checked_sub(1)?;
            // Only one bounded-source avatar is cloned before its cost is checked;
            // never clone an entire large population before discovering overflow.
            let value = visual(entity)?;
            let size = bincode::serialized_size(&value)
                .ok()?
                .checked_add(std::mem::size_of::<AvatarVisual>() as u64)?
                .checked_add(
                    (value.animations.layers.len() * std::mem::size_of::<AvatarAnimation>()) as u64,
                )?
                .checked_add(
                    (value.animations.bound_appearances.len() * std::mem::size_of::<String>())
                        as u64,
                )?;
            self.bytes = self.bytes.checked_sub(size)?;
            avatars.try_reserve(1).ok()?;
            avatars.push(value);
        }
        Some(AvatarVisualFrame {
            revision: WorldRevision {
                lot_id: Some(state.lot_id),
                epoch: state.authority_epoch,
                tick: state.completed_tick,
                architecture_revision: state.world.lot.revision().architecture,
                content: AssetKey(state.content.content_hash),
            },
            avatars,
        })
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;
    fn runtime() -> GameRuntime {
        use crate::sim_core::{
            state::{ContentSet, TuningSet},
            vm::RoutineStore,
        };
        use crate::{LotModel, RuntimeConfig, RuntimeRole, VmMode};
        GameRuntime::new(
            ContentSet::new(RoutineStore::new(), vec![], vec![], TuningSet::default()).unwrap(),
            LotModel::new(8, 8, 1).unwrap(),
            RuntimeConfig::new(VmMode::Ts1, 1, 1, 0),
            RuntimeRole::Authority,
        )
        .unwrap()
    }
    #[test]
    fn capture_budget_exhaustion_drops_the_entire_trace_without_mutating_the_runtime() {
        let runtime = runtime();
        let before = runtime.snapshot().unwrap();
        let mut capture = AvatarCapture::new(true);
        capture.observe(&runtime);
        assert_eq!(capture.frames.as_ref().unwrap().len(), 1);
        capture.bytes = 0;
        capture.observe(&runtime);
        assert!(capture.frames.is_none());
        capture.observe(&runtime);
        assert!(capture.finish().is_none());
        assert_eq!(runtime.snapshot().unwrap(), before);
    }
    #[test]
    fn disabled_capture_allocates_no_per_tick_avatar_records() {
        let runtime = runtime();
        let mut capture = AvatarCapture::new(false);
        let budget = capture.bytes;
        for _ in 0..100 {
            capture.observe(&runtime);
        }
        assert_eq!(capture.bytes, budget);
        assert!(capture.finish().is_none());
    }
}
