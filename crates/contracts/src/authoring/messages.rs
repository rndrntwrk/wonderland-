use super::*;
use crate::OperationId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterDraft {
    /// Raw text is retained for ordinary Unicode/IME editing; submit normalizes it.
    pub name: String,
    pub description: String,
    pub shard_id: Option<ShardId>,
    pub appearance: AppearanceSelection,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutfitDraft {
    pub character_id: CharacterId,
    pub owned_outfit_id: Option<OwnedOutfitId>,
    pub action: WardrobeAction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", content = "id", rename_all = "snake_case")]
pub enum PlacementSource {
    Catalog(CatalogId),
    Move(OwnedInstanceId),
    Inventory(OwnedInstanceId),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacementDraft {
    pub actor_id: CharacterId,
    pub home_owner_id: CharacterId,
    pub source: PlacementSource,
    pub pose: GridPose,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "editor", content = "draft", rename_all = "snake_case")]
pub enum AuthoringDraft {
    Creation(CharacterDraft),
    Outfit(OutfitDraft),
    Placement(PlacementDraft),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "intent", content = "data", rename_all = "snake_case")]
pub enum AuthoringIntent {
    SelectProfile(CharacterId),
    OpenCreate,
    UpdateName(String),
    UpdateDescription(String),
    SelectShard(ShardId),
    SelectHead(ContentKey),
    SelectBody(ContentKey),
    SelectSkinTone(ContentKey),
    SelectGender(ContentKey),
    SelectOwnedOutfit(OwnedOutfitId),
    SelectWardrobeAction(WardrobeAction),
    SubmitCreate,
    OpenOutfit,
    SaveOutfit,
    OpenHome(CharacterId),
    SelectCatalog(CatalogId),
    SelectOwned(OwnedInstanceId),
    BeginMove,
    BeginPlace(OwnedInstanceId),
    SetCell(GridCell),
    SetLevel(i16),
    MoveCandidate { dx: i16, dy: i16 },
    RotateCandidate,
    ConfirmPlacement,
    StoreSelected,
    Cancel,
    Close,
}

/// Actor fields express UI context. They are never authenticated authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthoringRequestKind {
    CreateProfile {
        name: String,
        description: String,
        shard_id: Option<ShardId>,
        appearance: AppearanceSelection,
    },
    SetOutfit {
        actor_id: CharacterId,
        owned_outfit_id: OwnedOutfitId,
        action: WardrobeAction,
    },
    BuyAndPlace {
        actor_id: CharacterId,
        home_owner_id: CharacterId,
        catalog_id: CatalogId,
        pose: GridPose,
    },
    MoveInstance {
        actor_id: CharacterId,
        home_owner_id: CharacterId,
        instance_id: OwnedInstanceId,
        pose: GridPose,
    },
    StoreInstance {
        actor_id: CharacterId,
        home_owner_id: CharacterId,
        instance_id: OwnedInstanceId,
    },
    PlaceOwned {
        actor_id: CharacterId,
        home_owner_id: CharacterId,
        instance_id: OwnedInstanceId,
        pose: GridPose,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoringSourceRevisions {
    pub account: u64,
    pub appearance: u64,
    pub catalog: u64,
    pub wardrobe: Option<u64>,
    pub lot: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoringRequest {
    pub operation_id: OperationId,
    pub base_revision: u64,
    pub expected_sources: AuthoringSourceRevisions,
    pub kind: AuthoringRequestKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum AuthoringOutcome {
    ProfileCreated {
        character_id: CharacterId,
    },
    OutfitSaved {
        character_id: CharacterId,
    },
    Purchased {
        owner_id: CharacterId,
        instance_id: OwnedInstanceId,
    },
    Moved {
        owner_id: CharacterId,
        instance_id: OwnedInstanceId,
    },
    Stored {
        owner_id: CharacterId,
        instance_id: OwnedInstanceId,
    },
    Placed {
        owner_id: CharacterId,
        instance_id: OwnedInstanceId,
    },
}

/// A committed receipt describes what changed and carries all acknowledged data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AuthoringEvent {
    Committed {
        operation_id: OperationId,
        base_revision: u64,
        projection: AuthoringProjection,
        outcome: AuthoringOutcome,
    },
    Rejected {
        operation_id: OperationId,
        base_revision: u64,
        error: AuthoringError,
    },
    ProjectionReplaced {
        projection: AuthoringProjection,
    },
}

/// Read-only reducer receipt metadata; compare revisions before mirroring the shell.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoringCommit {
    pub operation_id: OperationId,
    pub base_revision: u64,
    pub revision: u64,
    pub outcome: AuthoringOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error", content = "detail", rename_all = "snake_case")]
pub enum AuthoringError {
    InvalidProjection(String),
    InvalidName(String),
    InvalidDescription(String),
    InvalidAppearance(String),
    UnknownOutfit,
    SafetyLimit,
    UnknownProfile,
    UnknownCatalogItem,
    UnknownInstance,
    NoSelection,
    WrongEditor,
    Busy,
    Unavailable(String),
    PermissionDenied,
    InsufficientFunds,
    ProfileLimit,
    InventoryLimit,
    OutOfBounds,
    EntranceReserved,
    Occupied,
    WrongPlacementState,
    StaleRevision,
    InvalidOperation,
    InvalidOutcome,
    OperationLimit,
    Rejected(String),
}

impl fmt::Display for AuthoringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProjection(reason)
            | Self::InvalidName(reason)
            | Self::InvalidDescription(reason)
            | Self::InvalidAppearance(reason)
            | Self::Unavailable(reason)
            | Self::Rejected(reason) => reason.fmt(f),
            Self::UnknownOutfit => f.write_str("Choose an outfit owned by this character"),
            Self::SafetyLimit => f.write_str("This data exceeds the client resource safety limit"),
            Self::UnknownProfile => f.write_str("This profile is no longer available"),
            Self::UnknownCatalogItem => f.write_str("This item is not in the supplied catalog"),
            Self::UnknownInstance => f.write_str("This item is not owned by the selected home"),
            Self::NoSelection => f.write_str("Select a profile, room, or item first"),
            Self::WrongEditor => f.write_str("Open the matching editor first"),
            Self::Busy => f.write_str("Wait for the current change to finish"),
            Self::PermissionDenied => f.write_str("Only this home's owner can change it"),
            Self::InsufficientFunds => f.write_str("There is not enough money for this item"),
            Self::ProfileLimit => f.write_str("The account has reached its supplied character creation capacity"),
            Self::InventoryLimit => {
                f.write_str("The home has reached its supplied ownership capacity")
            }
            Self::OutOfBounds => f.write_str("Place the whole item inside the room"),
            Self::EntranceReserved => f.write_str("Keep the lot’s reserved cells clear"),
            Self::Occupied => f.write_str("Another item occupies these cells"),
            Self::WrongPlacementState => {
                f.write_str("This item's stored or placed state has changed")
            }
            Self::StaleRevision => f.write_str("This preview has changed; refresh the selection"),
            Self::InvalidOperation => {
                f.write_str("This operation identity is invalid or has already been used")
            }
            Self::InvalidOutcome => f.write_str("The receipt does not match the requested change"),
            Self::OperationLimit => {
                f.write_str("This preview session has reached its operation limit")
            }
        }
    }
}

impl std::error::Error for AuthoringError {}
