use super::*;
use crate::UiProjection;
use std::collections::BTreeSet;

/// Trim ordinary Unicode whitespace without truncating or normalizing the name.
/// Controls anywhere in the input are invalid, including leading/trailing controls.
pub fn normalize_profile_name(value: &str) -> Result<String, AuthoringError> {
    let trimmed = value.trim();
    if value.chars().any(char::is_control) {
        return Err(AuthoringError::InvalidName(
            "Names cannot contain control characters".into(),
        ));
    }
    if trimmed.is_empty() {
        return Err(AuthoringError::InvalidName("Enter a name".into()));
    }
    if trimmed.len() > 128 || trimmed.chars().count() > 32 {
        return Err(AuthoringError::InvalidName(
            "Use a name of 1–32 characters and at most 128 UTF-8 bytes".into(),
        ));
    }
    Ok(trimmed.into())
}

fn invalid(message: impl Into<String>) -> AuthoringError {
    AuthoringError::InvalidProjection(message.into())
}

/// Shared snapshot/request/receipt policy: 1–64 ASCII bytes containing only
/// letters, digits, `-`, `_`, `.`, or `:`. It does not establish identity authority.
pub fn is_valid_authoring_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
}

fn valid_availability(value: &Availability) -> bool {
    match value {
        Availability::Available => true,
        Availability::Unavailable { reason } => {
            !reason.trim().is_empty()
                && reason.len() <= 256
                && !reason.chars().any(char::is_control)
        }
    }
}

fn authored_record(id: &CatalogId) -> Option<(&'static str, CatalogCategory, i64, Footprint)> {
    use CatalogCategory::*;
    let (name, category, price, width, depth) = match id.as_ref() {
        "armchair" => ("Harbor armchair", Living, 180, 1, 1),
        "coffee-table" => ("Oak coffee table", Living, 120, 2, 1),
        "floor-lamp" => ("Linen floor lamp", Lighting, 90, 1, 1),
        "fern" => ("Potted fern", Decor, 45, 1, 1),
        "bookcase" => ("Oak bookcase", Storage, 260, 2, 1),
        "woven-rug" => ("Woven rug", Decor, 160, 2, 2),
        _ => return None,
    };
    Some((name, category, price, Footprint { width, depth }))
}

impl AuthoringProjection {
    /// Validate the whole bounded snapshot before exposing, applying, or saving it.
    /// Adapters must separately cap serialized input *before* deserialization.
    pub fn validate(&self) -> Result<(), AuthoringError> {
        if self.version != AUTHORING_VERSION || self.revision == 0 {
            return Err(invalid("Unsupported authoring version or zero revision"));
        }
        if self.profiles.len() > MAX_PROFILES || self.catalog.len() > 6 {
            return Err(invalid("Authoring profile or catalog bounds exceeded"));
        }
        // Reuse the original Character validation, including typed need coverage.
        UiProjection {
            version: 1,
            revision: self.revision,
            city_name: "Preview".into(),
            characters: self
                .profiles
                .iter()
                .map(|profile| profile.character.clone())
                .collect(),
            places: vec![],
            objects: vec![],
        }
        .validate()
        .map_err(|error| invalid(error.to_string()))?;

        let mut catalog_ids = BTreeSet::new();
        for item in &self.catalog {
            if !catalog_ids.insert(&item.id)
                || !valid_availability(&item.availability)
                || authored_record(&item.id)
                    != Some((
                        item.name.as_str(),
                        item.category,
                        item.price,
                        item.footprint,
                    ))
            {
                return Err(invalid(
                    "Invalid, changed, or duplicate authored catalog record",
                ));
            }
        }
        let mut instance_ids = BTreeSet::new();
        for profile in &self.profiles {
            if normalize_profile_name(&profile.character.name).as_ref()
                != Ok(&profile.character.name)
                || profile.look_id.style_for(profile.identity).is_none()
                || !(0..=PREVIEW_STARTING_BUDGET).contains(&profile.character.money)
                || profile.home.owner_id != profile.character.id
                || profile.home.instances.len() > MAX_OWNED_INSTANCES
                || !valid_availability(&profile.home.permissions.purchase)
                || !valid_availability(&profile.home.permissions.arrange)
            {
                return Err(invalid(
                    "Invalid profile name, look, budget, home ownership, or permissions",
                ));
            }
            let mut occupied = BTreeSet::new();
            for instance in &profile.home.instances {
                if !is_valid_authoring_id(instance.id.as_ref())
                    || !instance_ids.insert(&instance.id)
                {
                    return Err(invalid("Invalid or duplicate owned instance identity"));
                }
                let item = self
                    .catalog_item(&instance.catalog_id)
                    .ok_or_else(|| invalid("Owned instance references a missing catalog item"))?;
                if let Some(pose) = instance.placement {
                    let cells = item
                        .footprint
                        .cells(pose)
                        .map_err(|error| invalid(error.to_string()))?;
                    if cells.into_iter().any(|cell| !occupied.insert(cell)) {
                        return Err(invalid("Placed instances overlap"));
                    }
                }
            }
        }
        Ok(())
    }
}
