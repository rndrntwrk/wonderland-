//! Native FileProvider filename convention and persisted Vitaboy outfit IDs.
pub fn original_file_key(name: &str) -> Option<(u32, u32)> {
    let basename = name.rsplit('/').next()?;
    let stem = basename.rsplit_once('.')?.0;
    let segment = stem.rsplit('.').next()?;
    let segment = segment.strip_prefix("0x").unwrap_or(segment);
    packed(segment)
}
pub fn outfit_key(key: &str) -> Option<(u32, u32)> {
    packed(key.strip_prefix("vitaboy:")?)
}
fn packed(value: &str) -> Option<(u32, u32)> {
    if value.len() != 16 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u64::from_str_radix(value, 16).ok()?;
    Some(((value >> 32) as u32, value as u32))
}

/// Compose a temporary wardrobe selection for the stage without mutating the
/// acknowledged profile. Default/delete operations do not change worn clothing.
pub fn wardrobe_preview(
    profile: &wonderland_contracts::authoring::AuthoringProfile,
    draft: &wonderland_contracts::authoring::OutfitDraft,
) -> wonderland_contracts::authoring::AppearanceSelection {
    use wonderland_contracts::authoring::*;
    let mut appearance = profile.appearance.clone();
    if draft.action != WardrobeAction::Change {
        return appearance;
    }
    let Some(outfit) = draft
        .owned_outfit_id
        .as_ref()
        .and_then(|id| profile.wardrobe.outfit(id))
    else {
        return appearance;
    };
    let Some(category) = profile
        .wardrobe
        .categories
        .iter()
        .find(|c| c.id == outfit.category_id)
    else {
        return appearance;
    };
    match category.slot {
        AppearanceSlot::Head => appearance.head = Some(outfit.content_key.clone()),
        AppearanceSlot::Body => appearance.body = Some(outfit.content_key.clone()),
        AppearanceSlot::Decoration => {
            appearance
                .decorations
                .insert(category.id.clone(), outfit.content_key.clone());
        }
    }
    appearance
}
