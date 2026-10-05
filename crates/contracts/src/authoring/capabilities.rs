use super::*;
use crate::OperationId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CapacityPolicy {
    Unknown,
    Limited { maximum: u32 },
    Unlimited,
}

impl CapacityPolicy {
    /// None is unknown policy, not an unlimited grant.
    pub fn allows_addition(&self, existing: usize) -> Option<bool> {
        match self {
            Self::Unknown => None,
            Self::Limited { maximum } => Some((existing as u64) < u64::from(*maximum)),
            Self::Unlimited => Some(true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameAlphabet {
    Unicode,
    AsciiLettersAndSpaces,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileFieldPolicy {
    pub minimum_name_characters: u16,
    pub maximum_name_characters: u16,
    pub name_alphabet: NameAlphabet,
    pub maximum_description_characters: u32,
}

impl ProfileFieldPolicy {
    pub fn validate_fields(&self, name: &str, description: &str) -> Result<(), AuthoringError> {
        let normalized = normalize_profile_name(name)?;
        let count = normalized.chars().count();
        if normalized != name
            || count < usize::from(self.minimum_name_characters)
            || count > usize::from(self.maximum_name_characters)
            || (self.name_alphabet == NameAlphabet::AsciiLettersAndSpaces
                && !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphabetic() || byte == b' '))
        {
            return Err(AuthoringError::InvalidName(format!(
                "The account requires a name of {}–{} characters{}",
                self.minimum_name_characters,
                self.maximum_name_characters,
                if self.name_alphabet == NameAlphabet::AsciiLettersAndSpaces {
                    " using ASCII letters and spaces"
                } else {
                    ""
                }
            )));
        }
        if description.len() > MAX_DESCRIPTION_BYTES
            || description.chars().count() as u64 > u64::from(self.maximum_description_characters)
            || description
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        {
            return Err(AuthoringError::InvalidDescription(format!(
                "Use a description of at most {} characters",
                self.maximum_description_characters
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardOption {
    pub id: ShardId,
    pub label: String,
    pub availability: Availability,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountCapabilities {
    pub source: ContentSourceId,
    pub revision: u64,
    pub account_id: Option<AccountId>,
    pub shards: Vec<ShardOption>,
    pub default_shard: Option<ShardId>,
    pub creation: Availability,
    pub profile_capacity: CapacityPolicy,
    pub fields: ProfileFieldPolicy,
}

impl AccountCapabilities {
    pub fn ensure_creation(&self, existing: usize) -> Result<(), AuthoringError> {
        if let Availability::Unavailable { reason } = &self.creation {
            return Err(AuthoringError::Unavailable(reason.clone()));
        }
        match self.profile_capacity.allows_addition(existing) {
            Some(true) => Ok(()),
            Some(false) => Err(AuthoringError::ProfileLimit),
            None => Err(AuthoringError::Unavailable(
                "The account service has not supplied character creation capacity".into(),
            )),
        }
    }

    pub fn validate_shard(&self, selected: &Option<ShardId>) -> Result<(), AuthoringError> {
        match selected {
            None if self.shards.is_empty() => Ok(()),
            Some(id) => match self.shards.iter().find(|shard| &shard.id == id) {
                Some(ShardOption {
                    availability: Availability::Available,
                    ..
                }) => Ok(()),
                Some(ShardOption {
                    availability: Availability::Unavailable { reason },
                    ..
                }) => Err(AuthoringError::Unavailable(reason.clone())),
                None => Err(AuthoringError::Unavailable(
                    "Choose a supplied city or shard".into(),
                )),
            },
            None => Err(AuthoringError::Unavailable("Choose a city or shard".into())),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildToolKind {
    Hand,
    Terrain,
    Water,
    Walls,
    Wallpaper,
    Stairs,
    Fireplaces,
    Plants,
    Floors,
    Doors,
    Windows,
    Roof,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildTool {
    pub id: BuildToolId,
    pub kind: BuildToolKind,
    pub label: String,
    pub availability: Availability,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LotViewMode {
    WallsDown,
    Cutaway,
    WallsUp,
    Roof,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotViewOffer {
    pub mode: LotViewMode,
    pub label: String,
    pub availability: Availability,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildCapabilities {
    pub source: ContentSourceId,
    pub revision: u64,
    pub tools: Vec<BuildTool>,
    pub view_modes: Vec<LotViewOffer>,
    pub preview: Availability,
    pub commit: Availability,
}

/// A source-backed edit description. The UI does not interpret these fields as
/// an implemented architecture engine; a connected adapter must preview/commit it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureEdit {
    pub tool_id: BuildToolId,
    pub content_key: Option<ContentKey>,
    pub from: LotCell,
    pub to: LotCell,
    pub direction: Direction,
    pub value: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchitecturePhase {
    Preview,
    Commit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureRequest {
    pub operation_id: OperationId,
    pub base_revision: u64,
    pub lot_revision: u64,
    pub build_revision: u64,
    pub actor_id: CharacterId,
    pub home_owner_id: CharacterId,
    pub phase: ArchitecturePhase,
    pub edit: ArchitectureEdit,
}
