use super::{FrameContext, VmFault};
use crate::avatars::outfits::OutfitReference;
use crate::ids::EntityRef;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StringSource {
    CodeOwner,
    CodeOwnerSemiGlobal,
    CalleeSemiGlobal,
    Global,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringLookup {
    pub context: FrameContext,
    pub source: StringSource,
    pub table: u16,
    pub index: i32,
    pub params: Vec<i16>,
    pub locals: Vec<i16>,
    pub temps: [i16; 20],
    pub temp_xl: [i32; 2],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefreshKind {
    Graphic,
    Light,
    RoomScore,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresentationRequest {
    Refresh {
        target: EntityRef,
        kind: RefreshKind,
    },
    ShowString {
        target: EntityRef,
        message: String,
        history: bool,
    },
    Balloon {
        target: EntityRef,
        icon: Option<EntityRef>,
        index: i8,
        group: u8,
        duration: i16,
        headline_type: u8,
        flags: u8,
        clear: bool,
        preserve_money_on_zero_duration: bool,
    },
    ActionName {
        caller: EntityRef,
        name: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionString {
    pub name: String,
    pub parameter0: i16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuitLookup {
    pub context: FrameContext,
    pub scope: u8,
    pub resolved_data: u8,
    pub original_data: u8,
    pub default_update: bool,
    pub update_index: i16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolvedSuit {
    Accessory(String),
    Reference(OutfitReference),
    Id(u64),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppearanceOperation {
    DefaultDaywear {
        target: EntityRef,
        outfit: OutfitReference,
    },
    Body {
        target: EntityRef,
        outfit: OutfitReference,
        current_outfit: i16,
    },
    AccessoryName {
        target: EntityRef,
        appearance: String,
        remove: bool,
    },
    /// This is the equipped decoration, not the stored rack/inventory suit slot.
    Decoration {
        target: EntityRef,
        slot: u8,
        outfit_id: u64,
        remove: bool,
    },
}
pub fn bounded_string(value: String) -> Result<String, VmFault> {
    if value.len() > 65536 {
        Err(VmFault::InvalidContent("VM string exceeds 64KiB".into()))
    } else {
        Ok(value)
    }
}
