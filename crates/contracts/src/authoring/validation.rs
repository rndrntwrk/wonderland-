use super::*;
use crate::UiProjection;
use std::collections::{BTreeMap, BTreeSet};

/// Normalize input without applying account creation policy to saved names.
pub fn normalize_profile_name(value: &str) -> Result<String, AuthoringError> {
    if value.chars().any(char::is_control) {
        return Err(AuthoringError::InvalidName("Names cannot contain control characters".into()));
    }
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AuthoringError::InvalidName("Enter a name".into()));
    }
    if trimmed.len() > 128 {
        return Err(AuthoringError::InvalidName("The name exceeds the 128-byte input safety limit".into()));
    }
    Ok(trimmed.into())
}

fn invalid(message: impl Into<String>) -> AuthoringError {
    AuthoringError::InvalidProjection(message.into())
}

/// Identity syntax bounds do not establish authority or restrict content membership.
pub fn is_valid_authoring_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
}

fn text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}

fn availability(value: &Availability) -> bool {
    match value {
        Availability::Available => true,
        Availability::Unavailable { reason } => text(reason, 256),
    }
}

fn asset(value: &str) -> bool {
    text(value, 2048)
        && ((value.starts_with('/') && !value.starts_with("//")) || value.starts_with("https://"))
        && !value.contains('\\') && !value.split('/').any(|part| part == "..")
}

fn optional_asset(value: &Option<String>) -> bool {
    value.as_ref().is_none_or(|value| asset(value))
}

fn appearance_syntax(value: &AppearanceSelection) -> bool {
    [&value.head, &value.body, &value.skin_tone, &value.gender]
        .into_iter().flatten().all(|key| is_valid_authoring_id(key.as_ref()))
        && value.decorations.len() <= 256
        && value.decorations.iter().all(|(category, key)| {
            is_valid_authoring_id(category.as_ref()) && is_valid_authoring_id(key.as_ref())
        })
}

impl LotGeometry {
    pub fn validate(&self) -> Result<(), AuthoringError> {
        let maximum_x = i32::from(self.bounds.origin.x) + i32::from(self.bounds.width) - 1;
        let maximum_y = i32::from(self.bounds.origin.y) + i32::from(self.bounds.depth) - 1;
        if !is_valid_authoring_id(self.source.as_ref()) || self.revision == 0
            || self.bounds.width == 0 || self.bounds.depth == 0
            || maximum_x > i32::from(i16::MAX) || maximum_y > i32::from(i16::MAX)
            || self.levels.is_empty() || self.levels.len() > 256
            || self.levels.iter().collect::<BTreeSet<_>>().len() != self.levels.len()
            || self.reserved.len() > MAX_FOOTPRINT_CELLS
            || self.reserved.iter().collect::<BTreeSet<_>>().len() != self.reserved.len()
            || self.reserved.iter().any(|cell| !self.contains(*cell))
        {
            return Err(invalid("Invalid supplied lot geometry or levels"));
        }
        Ok(())
    }
}

impl AuthoringProjection {
    /// Validate structural safety, stable references and snapshot consistency.
    /// Creation/ownership capacity and creation fields are request-time policies.
    pub fn validate(&self) -> Result<(), AuthoringError> {
        if self.version != AUTHORING_VERSION || self.revision == 0 {
            return Err(invalid("Unsupported authoring version or zero revision"));
        }
        if self.profiles.len() > MAX_PROFILE_RECORDS
            || self.catalog.len() > MAX_CONTENT_RECORDS
            || self.catalog_categories.len() > MAX_CONTENT_RECORDS
        {
            return Err(invalid("Authoring resource-safety record bounds exceeded"));
        }
        UiProjection {
            version: 1,
            revision: self.revision,
            city_name: "Authoring".into(),
            characters: self.profiles.iter().map(|profile| profile.character.clone()).collect(),
            places: vec![],
            objects: vec![],
        }.validate().map_err(|error| invalid(error.to_string()))?;

        let account = &self.account;
        if !is_valid_authoring_id(account.source.as_ref()) || account.revision == 0
            || account.account_id.as_ref().is_some_and(|id| !is_valid_authoring_id(id.as_ref()))
            || !availability(&account.creation)
            || account.shards.len() > MAX_CONTENT_RECORDS
            || account.fields.minimum_name_characters == 0
            || account.fields.maximum_name_characters < account.fields.minimum_name_characters
        {
            return Err(invalid("Invalid account capability projection"));
        }
        let mut shard_ids = BTreeSet::new();
        for shard in &account.shards {
            if !is_valid_authoring_id(shard.id.as_ref()) || !shard_ids.insert(&shard.id)
                || !text(&shard.label, 128) || !availability(&shard.availability)
            {
                return Err(invalid("Invalid or duplicate shard option"));
            }
        }
        if account.default_shard.as_ref().is_some_and(|id| !shard_ids.contains(id)) {
            return Err(invalid("Default shard is not in the supplied shard options"));
        }

        let content = &self.appearance_content;
        if !is_valid_authoring_id(content.source.as_ref()) || content.revision == 0
            || !availability(&content.rendering)
            || content.heads.len() + content.bodies.len() + content.skin_tones.len() + content.genders.len() > MAX_CONTENT_RECORDS
        {
            return Err(invalid("Invalid appearance content projection"));
        }
        let gender_keys: BTreeSet<_> = content.genders.iter().map(|option| &option.key).collect();
        let skin_keys: BTreeSet<_> = content.skin_tones.iter().map(|option| &option.key).collect();
        for options in [&content.heads, &content.bodies, &content.skin_tones, &content.genders] {
            let mut keys = BTreeSet::new();
            for option in options {
                if !is_valid_authoring_id(option.key.as_ref()) || !keys.insert(&option.key)
                    || !text(&option.label, 128) || !optional_asset(&option.thumbnail)
                    || !availability(&option.availability)
                    || option.genders.len() > 256 || option.skin_tones.len() > 256
                    || option.genders.iter().any(|key| !gender_keys.contains(key))
                    || option.skin_tones.iter().any(|key| !skin_keys.contains(key))
                {
                    return Err(invalid("Invalid appearance option or compatibility membership"));
                }
            }
        }

        if !is_valid_authoring_id(self.catalog_source.as_ref()) || self.catalog_revision == 0 {
            return Err(invalid("Invalid catalog source or revision"));
        }
        let mut categories = BTreeSet::new();
        for category in &self.catalog_categories {
            if !is_valid_authoring_id(category.id.as_ref()) || !categories.insert(&category.id)
                || !text(&category.label, 128)
            {
                return Err(invalid("Invalid or duplicate catalog category"));
            }
        }
        let mut catalog = BTreeMap::new();
        for item in &self.catalog {
            if !is_valid_authoring_id(item.id.as_ref()) || catalog.insert(&item.id, item).is_some()
                || !is_valid_authoring_id(item.source_key.as_ref())
                || !text(&item.name, 128) || !categories.contains(&item.category)
                || item.price < 0 || item.footprint.width == 0 || item.footprint.depth == 0
                || usize::from(item.footprint.width) * usize::from(item.footprint.depth) > MAX_FOOTPRINT_CELLS
                || item.rotations.is_empty() || item.rotations.len() > 4
                || item.rotations.iter().collect::<BTreeSet<_>>().len() != item.rotations.len()
                || !optional_asset(&item.thumbnail) || !availability(&item.availability)
            {
                return Err(invalid("Invalid or duplicate supplied catalog record"));
            }
        }
        let mut instances = BTreeSet::new();
        let mut outfits = BTreeSet::new();
        for profile in &self.profiles {
            if normalize_profile_name(&profile.character.name).as_ref() != Ok(&profile.character.name)
                || profile.character.money < 0
                || profile.description.len() > MAX_DESCRIPTION_BYTES
                || profile.description.chars().any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
                || profile.shard_id.as_ref().is_some_and(|id| !is_valid_authoring_id(id.as_ref()))
                || !appearance_syntax(&profile.appearance)
                || profile.home.owner_id != profile.character.id
                || !availability(&profile.home.permissions.purchase)
                || !availability(&profile.home.permissions.arrange)
            {
                return Err(invalid("Invalid saved profile, appearance, balance, ownership or permissions"));
            }
            if let Some(portrait) = &profile.portrait
                && (!is_valid_authoring_id(portrait.source.as_ref())
                    || !is_valid_authoring_id(&portrait.identity)
                    || !is_valid_authoring_id(&portrait.look_id)
                    || !asset(&portrait.asset_path))
            {
                return Err(invalid("Invalid preserved portrait reference"));
            }
            profile.home.lot.validate()?;
            let build = &profile.home.build;
            if !is_valid_authoring_id(build.source.as_ref()) || build.revision == 0
                || build.tools.len() > 256 || build.view_modes.len() > 16
                || !availability(&build.preview) || !availability(&build.commit)
            {
                return Err(invalid("Invalid build capability projection"));
            }
            let mut tools = BTreeSet::new();
            for tool in &build.tools {
                if !is_valid_authoring_id(tool.id.as_ref()) || !tools.insert(&tool.id)
                    || !text(&tool.label, 128) || !availability(&tool.availability)
                {
                    return Err(invalid("Invalid build tool capability"));
                }
            }
            for view in &build.view_modes {
                if !text(&view.label, 128) || !availability(&view.availability) {
                    return Err(invalid("Invalid lot view capability"));
                }
            }
            let wardrobe = &profile.wardrobe;
            if wardrobe.revision == 0 || wardrobe.categories.len() > 256 {
                return Err(invalid("Invalid wardrobe projection"));
            }
            let mut wardrobe_categories = BTreeSet::new();
            for category in &wardrobe.categories {
                if !is_valid_authoring_id(category.id.as_ref()) || !wardrobe_categories.insert(&category.id)
                    || !text(&category.label, 128)
                {
                    return Err(invalid("Invalid wardrobe category"));
                }
            }
            let mut defaults = BTreeSet::new();
            for outfit in &wardrobe.outfits {
                if !is_valid_authoring_id(outfit.id.as_ref()) || !outfits.insert(&outfit.id)
                    || outfits.len() > MAX_OWNED_RECORDS
                    || !is_valid_authoring_id(outfit.content_key.as_ref())
                    || !wardrobe_categories.contains(&outfit.category_id)
                    || !text(&outfit.label, 128) || !optional_asset(&outfit.thumbnail)
                    || (outfit.is_default && !defaults.insert(&outfit.category_id))
                    || outfit.actions.len() > 3
                    || outfit.actions.iter().map(|offer| offer.action).collect::<BTreeSet<_>>().len() != outfit.actions.len()
                    || outfit.actions.iter().any(|offer| !availability(&offer.availability))
                {
                    return Err(invalid("Invalid owned wardrobe identity, category or action"));
                }
            }
            let mut occupied = BTreeSet::new();
            for instance in &profile.home.instances {
                if !is_valid_authoring_id(instance.id.as_ref()) || !instances.insert(&instance.id)
                    || instances.len() > MAX_OWNED_RECORDS
                {
                    return Err(invalid("Invalid, duplicate or resource-excessive owned instance identity"));
                }
                let item = catalog.get(&instance.catalog_id)
                    .ok_or_else(|| invalid("Owned instance references missing catalog metadata"))?;
                if let Some(pose) = instance.placement {
                    let cells = item.footprint.cells(pose, &profile.home.lot)
                        .map_err(|error| invalid(error.to_string()))?;
                    if cells.into_iter().any(|cell| !occupied.insert(cell)) {
                        return Err(invalid("Placed instances overlap on the same level"));
                    }
                }
            }
        }
        Ok(())
    }
}

impl ArchitectureRequest {
    /// Preflight only. This does not fabricate an architecture preview or receipt.
    pub fn validate(&self, projection: &AuthoringProjection) -> Result<(), AuthoringError> {
        projection.validate()?;
        if !is_valid_authoring_id(self.operation_id.as_ref())
            || !is_valid_authoring_id(self.actor_id.as_ref())
            || !is_valid_authoring_id(self.home_owner_id.as_ref())
            || !is_valid_authoring_id(self.edit.tool_id.as_ref())
            || self.edit.content_key.as_ref().is_some_and(|key| !is_valid_authoring_id(key.as_ref()))
        {
            return Err(AuthoringError::InvalidOperation);
        }
        let profile = projection.profile(&self.actor_id).ok_or(AuthoringError::UnknownProfile)?;
        let home = projection.home(&self.home_owner_id).ok_or(AuthoringError::UnknownProfile)?;
        if self.base_revision != projection.revision
            || self.lot_revision != home.lot.revision
            || self.build_revision != home.build.revision
        {
            return Err(AuthoringError::StaleRevision);
        }
        if self.actor_id != self.home_owner_id {
            return Err(AuthoringError::PermissionDenied);
        }
        let tool = home.build.tools.iter().find(|tool| tool.id == self.edit.tool_id)
            .ok_or_else(|| AuthoringError::Unavailable("This build tool is not supplied".into()))?;
        for capability in [
            &profile.character.availability,
            &tool.availability,
            if self.phase == ArchitecturePhase::Preview { &home.build.preview } else { &home.build.commit },
        ] {
            if let Availability::Unavailable { reason } = capability {
                return Err(AuthoringError::Unavailable(reason.clone()));
            }
        }
        if !home.lot.contains(self.edit.from) || !home.lot.contains(self.edit.to) {
            return Err(AuthoringError::OutOfBounds);
        }
        Ok(())
    }
}
