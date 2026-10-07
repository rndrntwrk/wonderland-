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
            avatars: state
                .entities
                .values()
                .filter_map(|entity| {
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
                })
                .collect(),
        }
    }
}
