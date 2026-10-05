//! Versioned character and room authoring for the bounded UI preview.
//!
//! These presentation identities and cells are not account IDs, engine entity IDs,
//! canonical lot coordinates, or authenticated rights. A live adapter must supply
//! coordinated authoritative projections and independently authorize each command.

mod geometry;
mod messages;
mod validation;

pub use geometry::*;
pub use messages::*;
pub use validation::{is_valid_authoring_id, normalize_profile_name};

use crate::{Availability, Character, CharacterId};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const AUTHORING_VERSION: u32 = 1;
pub const MAX_PROFILES: usize = 8;
pub const MAX_OWNED_INSTANCES: usize = 64;
pub const PREVIEW_STARTING_BUDGET: i64 = 1_250;
/// Cap both input and output bytes before parsing or saving local preview JSON.
pub const MAX_AUTHORING_JSON_BYTES: usize = 256 * 1024;

string_id!(LookId, CatalogId, OwnedInstanceId);

/// Authored visual presets; several created profiles may share a preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualIdentity {
    Maya,
    Jules,
    Nico,
    Amara,
    Leo,
}

impl VisualIdentity {
    pub const ALL: [Self; 5] = [Self::Maya, Self::Jules, Self::Nico, Self::Amara, Self::Leo];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Maya => "maya",
            Self::Jules => "jules",
            Self::Nico => "nico",
            Self::Amara => "amara",
            Self::Leo => "leo",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Maya => "Maya",
            Self::Jules => "Jules",
            Self::Nico => "Nico",
            Self::Amara => "Amara",
            Self::Leo => "Leo",
        }
    }

    pub fn look_id(self, style: LookStyle) -> LookId {
        format!("{}-{}", self.as_str(), style.as_str()).into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LookStyle {
    Everyday,
    Smart,
    Active,
}

impl LookStyle {
    pub const ALL: [Self; 3] = [Self::Everyday, Self::Smart, Self::Active];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Everyday => "everyday",
            Self::Smart => "smart",
            Self::Active => "active",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Everyday => "Everyday",
            Self::Smart => "Smart",
            Self::Active => "Active",
        }
    }
}

impl LookId {
    pub fn style_for(&self, identity: VisualIdentity) -> Option<LookStyle> {
        LookStyle::ALL
            .into_iter()
            .find(|style| identity.look_id(*style) == *self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogCategory {
    Living,
    Lighting,
    Decor,
    Storage,
}

impl CatalogCategory {
    pub const ALL: [Self; 4] = [Self::Living, Self::Lighting, Self::Decor, Self::Storage];

    pub fn label(self) -> &'static str {
        match self {
            Self::Living => "Living",
            Self::Lighting => "Lighting",
            Self::Decor => "Decor",
            Self::Storage => "Storage",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogItem {
    pub id: CatalogId,
    pub name: String,
    pub category: CatalogCategory,
    pub price: i64,
    pub footprint: Footprint,
    pub availability: Availability,
}

impl CatalogItem {
    /// Only these three objects have meaningful directional preview artwork.
    pub fn can_rotate(&self) -> bool {
        matches!(self.id.as_ref(), "armchair" | "coffee-table" | "bookcase")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedInstance {
    pub id: OwnedInstanceId,
    pub catalog_id: CatalogId,
    /// `None` is owned inventory. Storing never discards this instance identity.
    pub placement: Option<GridPose>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomePermissions {
    pub purchase: Availability,
    pub arrange: Availability,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Home {
    pub owner_id: CharacterId,
    pub permissions: HomePermissions,
    pub instances: Vec<OwnedInstance>,
}

impl Home {
    pub fn instance(&self, id: &OwnedInstanceId) -> Option<&OwnedInstance> {
        self.instances.iter().find(|instance| &instance.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoringProfile {
    pub character: Character,
    pub identity: VisualIdentity,
    pub look_id: LookId,
    pub home: Home,
}

/// Complete acknowledged data. Drafts, pending requests, and grants are excluded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoringProjection {
    pub version: u32,
    pub revision: u64,
    pub profiles: Vec<AuthoringProfile>,
    pub catalog: Vec<CatalogItem>,
}

impl AuthoringProjection {
    pub fn profile(&self, id: &CharacterId) -> Option<&AuthoringProfile> {
        self.profiles
            .iter()
            .find(|profile| &profile.character.id == id)
    }

    pub fn home(&self, owner_id: &CharacterId) -> Option<&Home> {
        self.profile(owner_id).map(|profile| &profile.home)
    }

    pub fn catalog_item(&self, id: &CatalogId) -> Option<&CatalogItem> {
        self.catalog.iter().find(|item| &item.id == id)
    }
}
