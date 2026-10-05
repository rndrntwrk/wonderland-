use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearanceSelection {
    pub head: Option<ContentKey>,
    pub body: Option<ContentKey>,
    pub skin_tone: Option<ContentKey>,
    pub gender: Option<ContentKey>,
    pub decorations: BTreeMap<WardrobeCategoryId, ContentKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearanceOption {
    pub key: ContentKey,
    pub label: String,
    pub thumbnail: Option<String>,
    pub availability: Availability,
    /// Empty means unrestricted; a legacy adapter supplies collection membership.
    pub genders: Vec<ContentKey>,
    pub skin_tones: Vec<ContentKey>,
}

impl AppearanceOption {
    pub fn compatible(&self, selection: &AppearanceSelection) -> bool {
        (self.genders.is_empty()
            || selection.gender.as_ref().is_some_and(|key| self.genders.contains(key)))
            && (self.skin_tones.is_empty()
                || selection.skin_tone.as_ref().is_some_and(|key| self.skin_tones.contains(key)))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearanceRequirements {
    pub head: bool,
    pub body: bool,
    pub skin_tone: bool,
    pub gender: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearanceContent {
    pub source: ContentSourceId,
    pub revision: u64,
    pub heads: Vec<AppearanceOption>,
    pub bodies: Vec<AppearanceOption>,
    pub skin_tones: Vec<AppearanceOption>,
    pub genders: Vec<AppearanceOption>,
    pub requirements: AppearanceRequirements,
    /// Content readiness is independent from whether a key can be selected.
    pub rendering: Availability,
}

impl AppearanceContent {
    pub fn default_selection(&self) -> AppearanceSelection {
        fn first(options: &[AppearanceOption]) -> Option<ContentKey> {
            options.iter().find(|option| option.availability.is_available()).map(|option| option.key.clone())
        }
        let mut selection = AppearanceSelection {
            gender: first(&self.genders),
            skin_tone: first(&self.skin_tones),
            ..AppearanceSelection::default()
        };
        selection.head = self.heads.iter().find(|option| {
            option.availability.is_available() && option.compatible(&selection)
        }).map(|option| option.key.clone());
        selection.body = self.bodies.iter().find(|option| {
            option.availability.is_available() && option.compatible(&selection)
        }).map(|option| option.key.clone());
        selection
    }

    /// Creation rules apply to a submitted selection, not to saved appearance
    /// references whose content may currently be missing or unavailable.
    pub fn validate_selection(&self, selection: &AppearanceSelection) -> Result<(), AuthoringError> {
        for (selected, options, required, label) in [
            (&selection.head, &self.heads, self.requirements.head, "head"),
            (&selection.body, &self.bodies, self.requirements.body, "body"),
            (&selection.skin_tone, &self.skin_tones, self.requirements.skin_tone, "skin tone"),
            (&selection.gender, &self.genders, self.requirements.gender, "gender"),
        ] {
            let Some(key) = selected else {
                if required {
                    return Err(AuthoringError::InvalidAppearance(format!("Choose a {label}")));
                }
                continue;
            };
            let option = options.iter().find(|option| &option.key == key)
                .ok_or_else(|| AuthoringError::InvalidAppearance(format!("The selected {label} is not in the supplied content")))?;
            if let Availability::Unavailable { reason } = &option.availability {
                return Err(AuthoringError::Unavailable(reason.clone()));
            }
            if !option.compatible(selection) {
                return Err(AuthoringError::InvalidAppearance(format!("The selected {label} is incompatible with this gender or skin tone")));
            }
        }
        if !selection.decorations.is_empty() {
            return Err(AuthoringError::InvalidAppearance("Creation decorations must be supplied by an owned wardrobe".into()));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortraitReference {
    pub source: ContentSourceId,
    pub identity: String,
    pub look_id: String,
    pub asset_path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppearanceSlot {
    Head,
    Body,
    Decoration,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WardrobeCategory {
    pub id: WardrobeCategoryId,
    pub label: String,
    pub slot: AppearanceSlot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WardrobeAction {
    Change,
    SetDefault,
    Delete,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WardrobeActionOffer {
    pub action: WardrobeAction,
    pub availability: Availability,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedOutfit {
    pub id: OwnedOutfitId,
    pub content_key: ContentKey,
    pub category_id: WardrobeCategoryId,
    pub label: String,
    pub thumbnail: Option<String>,
    pub is_default: bool,
    pub actions: Vec<WardrobeActionOffer>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wardrobe {
    pub revision: u64,
    pub categories: Vec<WardrobeCategory>,
    pub outfits: Vec<OwnedOutfit>,
}

impl Wardrobe {
    pub fn outfit(&self, id: &OwnedOutfitId) -> Option<&OwnedOutfit> {
        self.outfits.iter().find(|outfit| &outfit.id == id)
    }
}
