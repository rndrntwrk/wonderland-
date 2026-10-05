use super::motives::Motive;
use crate::ids::EntityRef;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotiveAdvertisement {
    pub motive: Motive,
    pub minimum: i16,
    pub delta: i16,
    pub personality_modifier: u16,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionVariant {
    pub param0: i16,
    /// Source keys: min=(0<<16)|motive, delta=(1<<16)|motive,
    /// personality=(2<<16)|motive. If present, missing keys yield zero,
    /// matching Dictionary.TryGetValue(out ...) in VMFindBestAction.
    pub motive_ad_changes: Option<BTreeMap<u32, i16>>,
}

/// B supplies CheckAction's completed result for this exact avatar/state,
/// including permissions, carrying, repairs, ghost/pet and check-tree rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OfferStatus {
    Available,
    PermissionDenied,
    CarryingDenied,
    RepairMismatch,
    CheckFailed,
    MissingAction,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InteractionCandidate {
    pub target: EntityRef,
    /// Stable entity/TTAB enumeration ordinal supplied by the offer provider.
    pub source_order: u64,
    pub interaction_id: u32,
    pub advertisements: Vec<MotiveAdvertisement>,
    pub variants: Vec<InteractionVariant>,
    pub status: OfferStatus,
    pub in_world: bool,
    pub disabled: bool,
    pub occupied: bool,
    pub use_count: i16,
    pub joining_available: bool,
    /// TakeTopActions compares JoiningIndex with the byte-narrowed pie ID,
    /// whereas the occupied check above compares the full TTAIndex.
    pub joining_available_after_narrowing: bool,
    pub is_game_object: bool,
    pub outside: bool,
    /// Legacy positions: x/y sixteenths of a tile, level as signed byte.
    pub x: i16,
    pub y: i16,
    pub level: i8,
    pub attenuation_code: u32,
    pub attenuation_value: f32,
    pub auto_first: bool,
}

impl InteractionCandidate {
    pub fn new(target: EntityRef, source_order: u64, interaction_id: u32) -> Self {
        Self {
            target,
            source_order,
            interaction_id,
            advertisements: Vec::new(),
            variants: vec![InteractionVariant::default()],
            status: OfferStatus::Available,
            in_world: true,
            disabled: false,
            occupied: false,
            use_count: 0,
            joining_available: false,
            joining_available_after_narrowing: false,
            is_game_object: true,
            outside: true,
            x: 0,
            y: 0,
            level: 1,
            attenuation_code: 1,
            attenuation_value: 0.0,
            auto_first: false,
        }
    }
    pub fn set_joining_indices(&mut self, indices: &[i32]) {
        self.joining_available = indices
            .iter()
            .any(|index| i64::from(*index) == i64::from(self.interaction_id));
        self.joining_available_after_narrowing =
            indices.contains(&i32::from(self.interaction_id as u8));
    }
}
