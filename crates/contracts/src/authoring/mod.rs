//! Versioned authoring data supplied by account, content and lot adapters.
//!
//! Opaque content, ownership and operation identities remain distinct. These
//! projections describe capabilities; a connected service must authorize requests.

mod appearance;
mod capabilities;
mod geometry;
mod legacy;
mod messages;
mod validation;

pub use appearance::*;
pub use capabilities::*;
pub use geometry::*;
pub use legacy::migrate_v1_projection;
pub use messages::*;
pub use validation::{is_valid_authoring_id, normalize_profile_name};

use crate::{Availability, Character, CharacterId};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const AUTHORING_VERSION: u32 = 2;
/// Resource-safety bounds, never game policy or account creation capacity.
pub const MAX_PROFILE_RECORDS: usize = 1024;
pub const MAX_LOT_RECORDS: usize = 1024;
pub const MAX_CONTENT_RECORDS: usize = 65_536;
pub const MAX_OWNED_RECORDS: usize = 65_536;
pub const MAX_AUTHORING_JSON_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_DESCRIPTION_BYTES: usize = 16 * 1024;

string_id!(
    ContentKey,
    ContentSourceId,
    AccountId,
    ShardId,
    CatalogId,
    CatalogCategoryId,
    OwnedInstanceId,
    WardrobeCategoryId,
    OwnedOutfitId,
    BuildToolId
);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogCategory {
    pub id: CatalogCategoryId,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogItem {
    pub id: CatalogId,
    /// Original source key, independent from an owned instance's identity.
    pub source_key: ContentKey,
    pub name: String,
    pub category: CatalogCategoryId,
    pub price: i64,
    pub footprint: Footprint,
    /// Supplied allowed orientations, in the order used by the Rotate control.
    pub rotations: Vec<Direction>,
    pub thumbnail: Option<String>,
    pub availability: Availability,
}

impl CatalogItem {
    pub fn can_rotate(&self) -> bool {
        self.rotations.len() > 1
    }

    pub fn next_direction(&self, current: Direction) -> Option<Direction> {
        let index = self
            .rotations
            .iter()
            .position(|direction| *direction == current)?;
        self.rotations
            .get((index + 1) % self.rotations.len())
            .copied()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedInstance {
    pub id: OwnedInstanceId,
    pub catalog_id: CatalogId,
    /// Original object ownership is independent of the lot containing it.
    /// Unknown ownership never grants inventory placement rights.
    #[serde(default)]
    pub owner_id: Option<CharacterId>,
    /// None is owned inventory. Storing never discards this identity.
    pub placement: Option<GridPose>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomePermissions {
    pub purchase: Availability,
    pub arrange: Availability,
}

/// Actor-scoped authority supplied for this lot and its current revision.
/// The UI's selected actor is context, never authentication for a live adapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotGrant {
    pub actor_id: CharacterId,
    pub purchase: Option<PurchaseGrant>,
    pub arrange: Availability,
    pub build: Availability,
    /// Owners whose stored objects this actor may place on this lot.
    /// An empty scope grants no inventory access.
    pub inventory_owners: Vec<CharacterId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseGrant {
    pub availability: Availability,
    pub payer_id: CharacterId,
    pub object_owner_id: CharacterId,
    /// None explicitly permits every supplied category; Some(empty) permits none.
    pub categories: Option<Vec<CatalogCategoryId>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Home {
    pub owner_id: CharacterId,
    /// Lot-wide service availability; it does not authorize an actor by itself.
    pub permissions: HomePermissions,
    #[serde(default)]
    pub grants: Vec<LotGrant>,
    pub instance_capacity: CapacityPolicy,
    pub lot: LotGeometry,
    pub build: BuildCapabilities,
    pub instances: Vec<OwnedInstance>,
}

impl Home {
    pub fn instance(&self, id: &OwnedInstanceId) -> Option<&OwnedInstance> {
        self.instances.iter().find(|instance| &instance.id == id)
    }

    pub fn grant(&self, actor_id: &CharacterId) -> Option<&LotGrant> {
        self.grants.iter().find(|grant| &grant.actor_id == actor_id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoringProfile {
    pub character: Character,
    pub description: String,
    pub shard_id: Option<ShardId>,
    pub appearance: AppearanceSelection,
    /// Preserved artwork only. This never supplies an outfit or renderer resource.
    pub portrait: Option<PortraitReference>,
    pub wardrobe: Wardrobe,
    pub home: Home,
}

/// Complete acknowledged data. Drafts, pending operations and decoded rendering
/// resources are excluded. Revisions identify the source data used by requests.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoringProjection {
    pub version: u32,
    pub revision: u64,
    pub account: AccountCapabilities,
    pub appearance_content: AppearanceContent,
    pub catalog_source: ContentSourceId,
    pub catalog_revision: u64,
    pub catalog_categories: Vec<CatalogCategory>,
    pub profiles: Vec<AuthoringProfile>,
    /// Supplied target lots whose owners need not be playable account profiles.
    #[serde(default)]
    pub shared_homes: Vec<Home>,
    pub catalog: Vec<CatalogItem>,
}

impl AuthoringProjection {
    pub fn profile(&self, id: &CharacterId) -> Option<&AuthoringProfile> {
        self.profiles
            .iter()
            .find(|profile| &profile.character.id == id)
    }

    pub fn home(&self, owner_id: &CharacterId) -> Option<&Home> {
        self.homes().find(|home| &home.owner_id == owner_id)
    }

    pub fn homes(&self) -> impl Iterator<Item = &Home> {
        self.profiles
            .iter()
            .map(|profile| &profile.home)
            .chain(&self.shared_homes)
    }

    pub fn owned_instance(&self, id: &OwnedInstanceId) -> Option<(&Home, &OwnedInstance)> {
        self.homes()
            .find_map(|home| home.instance(id).map(|instance| (home, instance)))
    }

    pub fn inventory_for<'a>(
        &'a self,
        owner_id: &'a CharacterId,
    ) -> impl Iterator<Item = &'a OwnedInstance> {
        self.homes()
            .flat_map(|home| &home.instances)
            .filter(move |instance| {
                instance.placement.is_none() && instance.owner_id.as_ref() == Some(owner_id)
            })
    }

    pub fn catalog_item(&self, id: &CatalogId) -> Option<&CatalogItem> {
        self.catalog.iter().find(|item| &item.id == id)
    }

    pub fn creation_allowed(&self) -> Result<(), AuthoringError> {
        self.account.ensure_creation(self.profiles.len())?;
        if self.profiles.len() >= MAX_PROFILE_RECORDS {
            return Err(AuthoringError::SafetyLimit);
        }
        Ok(())
    }

    pub fn source_revisions(&self, kind: &AuthoringRequestKind) -> AuthoringSourceRevisions {
        let (wardrobe, lot) = match kind {
            AuthoringRequestKind::SetOutfit { actor_id, .. } => (
                self.profile(actor_id)
                    .map(|profile| profile.wardrobe.revision),
                None,
            ),
            AuthoringRequestKind::BuyAndPlace { home_owner_id, .. }
            | AuthoringRequestKind::MoveInstance { home_owner_id, .. }
            | AuthoringRequestKind::StoreInstance { home_owner_id, .. }
            | AuthoringRequestKind::PlaceOwned { home_owner_id, .. } => {
                (None, self.home(home_owner_id).map(|home| home.lot.revision))
            }
            AuthoringRequestKind::CreateProfile { .. } => (None, None),
        };
        AuthoringSourceRevisions {
            account: self.account.revision,
            appearance: self.appearance_content.revision,
            catalog: self.catalog_revision,
            wardrobe,
            lot,
        }
    }
}
