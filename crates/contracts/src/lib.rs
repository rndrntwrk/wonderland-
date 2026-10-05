//! Bounded presentation data and DOM-free shell messages.
//!
//! These types are a versioned preview boundary, not a multiplayer wire protocol.
//! Eligibility is display data; authenticated adapters must independently authorize requests.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

macro_rules! string_id {
    ($($name:ident),+ $(,)?) => {$(
        #[doc = "Stable opaque presentation ID, serialized as a string."]
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);
        impl From<&str> for $name { fn from(value: &str) -> Self { Self(value.into()) } }
        impl From<String> for $name { fn from(value: String) -> Self { Self(value) } }
        impl AsRef<str> for $name { fn as_ref(&self) -> &str { &self.0 } }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { self.0.fmt(f) }
        }
    )+};
}
string_id!(CharacterId, PlaceId, ObjectId, ActionId, OperationId);

/// Stable object identity plus incarnation; never an engine entity index.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityRef {
    pub id: ObjectId,
    pub generation: u64,
}

/// Presentation eligibility only. Unavailable selections can expose their reason.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Availability {
    Available,
    Unavailable { reason: String },
}
impl Availability {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }
    pub fn ensure_available(&self) -> Result<(), UiError> {
        match self {
            Self::Available => Ok(()),
            Self::Unavailable { reason } => Err(UiError::Unavailable(reason.clone())),
        }
    }
}

/// Exactly the eight legacy needs, each displayed on a 0..=100 scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Need {
    Energy,
    Hunger,
    Fun,
    Social,
    Hygiene,
    Bladder,
    Comfort,
    Room,
}
impl Need {
    pub const ALL: [Self; 8] = [
        Self::Energy,
        Self::Hunger,
        Self::Fun,
        Self::Social,
        Self::Hygiene,
        Self::Bladder,
        Self::Comfort,
        Self::Room,
    ];
}

/// Normalized illustration coordinate, independent of viewport or DOM geometry.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Character {
    pub id: CharacterId,
    pub name: String,
    pub availability: Availability,
    pub money: i64,
    pub needs: BTreeMap<Need, u8>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub id: PlaceId,
    pub name: String,
    pub population: u32,
    pub availability: Availability,
    pub anchor: Anchor,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionOffer {
    pub id: ActionId,
    pub label: String,
    pub availability: Availability,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneObject {
    pub target: EntityRef,
    pub revision: u64,
    pub place_id: PlaceId,
    pub name: String,
    pub anchor: Anchor,
    pub offers: Vec<ActionOffer>,
}

/// Version 1 read-only presentation snapshot. Validate before exposing external data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UiProjection {
    pub version: u32,
    pub revision: u64,
    pub city_name: String,
    pub characters: Vec<Character>,
    pub places: Vec<Place>,
    pub objects: Vec<SceneObject>,
}

impl UiProjection {
    /// Validate bounds, identity uniqueness, references, needs, and coordinates.
    /// This is a post-deserialization boundary; adapters must also cap input bytes.
    pub fn validate(&self) -> Result<(), UiError> {
        fn fail(message: &str) -> UiError {
            UiError::InvalidProjection(message.into())
        }
        fn text(value: &str, limit: usize) -> bool {
            !value.trim().is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
        }
        fn id(value: &str) -> bool {
            text(value, 64)
                && value
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.:".contains(&c))
        }
        fn availability(value: &Availability) -> bool {
            match value {
                Availability::Available => true,
                Availability::Unavailable { reason } => text(reason, 256),
            }
        }
        fn anchor(value: Anchor) -> bool {
            value.x.is_finite()
                && value.y.is_finite()
                && (0.0..=1.0).contains(&value.x)
                && (0.0..=1.0).contains(&value.y)
        }
        if self.version != 1 || self.revision == 0 {
            return Err(fail("Unsupported version or zero projection revision"));
        }
        if !text(&self.city_name, 128)
            || self.characters.len() > 64
            || self.places.len() > 128
            || self.objects.len() > 1024
        {
            return Err(fail("Projection bounds exceeded"));
        }
        let mut characters = BTreeSet::new();
        for c in &self.characters {
            if !id(c.id.as_ref())
                || !characters.insert(&c.id)
                || !text(&c.name, 128)
                || !availability(&c.availability)
                || c.needs.len() != 8
                || Need::ALL.iter().any(|need| !c.needs.contains_key(need))
                || c.needs.values().any(|v| *v > 100)
            {
                return Err(fail("Invalid character, identity, or needs"));
            }
        }
        let mut places = BTreeSet::new();
        for p in &self.places {
            if !id(p.id.as_ref())
                || !places.insert(&p.id)
                || !text(&p.name, 128)
                || !availability(&p.availability)
                || !anchor(p.anchor)
            {
                return Err(fail("Invalid place or anchor"));
            }
        }
        let mut objects = BTreeSet::new();
        for o in &self.objects {
            if !id(o.target.id.as_ref())
                || !objects.insert(&o.target.id)
                || o.target.generation == 0
                || o.revision == 0
                || !places.contains(&o.place_id)
                || !text(&o.name, 128)
                || !anchor(o.anchor)
                || o.offers.len() > 16
            {
                return Err(fail("Invalid object or reference"));
            }
            let mut actions = BTreeSet::new();
            for a in &o.offers {
                if !id(a.id.as_ref())
                    || !actions.insert(&a.id)
                    || !text(&a.label, 128)
                    || !availability(&a.availability)
                {
                    return Err(fail("Invalid or duplicate offer"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "screen", rename_all = "snake_case")]
pub enum Screen {
    CharacterSelection,
    City,
    Lot { place_id: PlaceId },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "intent", content = "data", rename_all = "snake_case")]
pub enum UiIntent {
    SelectCharacter(CharacterId),
    Play,
    SelectPlace(PlaceId),
    Visit,
    SelectObject(EntityRef),
    DismissObject,
    TakeOffer {
        target: EntityRef,
        action_id: ActionId,
        expected_revision: u64,
    },
    Cancel {
        operation_id: OperationId,
    },
    Back,
}

/// The selected character is UI context, never authenticated actor authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RequestKind {
    Travel {
        character_id: CharacterId,
        place_id: PlaceId,
    },
    Interaction {
        character_id: CharacterId,
        place_id: PlaceId,
        target: EntityRef,
        action_id: ActionId,
        expected_revision: u64,
    },
    Cancellation {
        queue_operation_id: OperationId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiRequest {
    pub operation_id: OperationId,
    pub projection_revision: u64,
    pub kind: RequestKind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    Pending,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingRequest {
    pub request: UiRequest,
    pub status: RequestStatus,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueStatus {
    Active,
    CancellationPending,
}
/// Only acknowledged interactions enter this queue. It is not simulation state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedAction {
    pub operation_id: OperationId,
    pub target: EntityRef,
    pub action_id: ActionId,
    pub expected_revision: u64,
    pub label: String,
    pub status: QueueStatus,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum UiEvent {
    Accepted {
        operation_id: OperationId,
        projection_revision: u64,
    },
    Rejected {
        operation_id: OperationId,
        projection_revision: u64,
        reason: String,
    },
    CancellationAcknowledged {
        operation_id: OperationId,
        projection_revision: u64,
    },
    ProjectionUpdated {
        projection: UiProjection,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error", content = "detail", rename_all = "snake_case")]
pub enum UiError {
    InvalidProjection(String),
    NoSelection,
    WrongScreen,
    UnknownSelection,
    Unavailable(String),
    StaleTarget,
    UnofferedAction,
    Rejected(String),
    OperationLimit,
}
impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProjection(s) | Self::Unavailable(s) | Self::Rejected(s) => s.fmt(f),
            Self::NoSelection => {
                f.write_str("Select an available character, place, or object first")
            }
            Self::WrongScreen => f.write_str("This action is unavailable on the current screen"),
            Self::UnknownSelection => f.write_str("The selection is no longer available"),
            Self::StaleTarget => f.write_str("This object or action has changed; select it again"),
            Self::UnofferedAction => {
                f.write_str("This action is not offered by the selected object")
            }
            Self::OperationLimit => f.write_str("The preview request limit has been reached"),
        }
    }
}
impl std::error::Error for UiError {}
