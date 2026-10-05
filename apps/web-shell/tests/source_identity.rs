use wonderland_web_shell::source_identity::*;
#[test]
fn original_named_resources_keep_packed_file_identity() {
    assert_eq!(
        original_file_key("avatardata/outfits/fancy.000000f30000000d.oft"),
        Some((0xf3, 0x0d))
    );
    assert_eq!(
        original_file_key("000000f30000000d.oft"),
        Some((0xf3, 0x0d))
    );
    assert_eq!(original_file_key("fancy.oft"), None);
    assert_eq!(original_file_key("fancy.000000f30000000d.extra.oft"), None);
    assert_eq!(original_file_key("fancy.000000f30000000g.oft"), None);
}
#[test]
fn saved_outfit_key_does_not_depend_on_collection_membership() {
    assert_eq!(outfit_key("vitaboy:000000f30000000d"), Some((0xf3, 13)));
    assert_eq!(outfit_key("purchasable:000000f30000000d"), None);
    assert_eq!(outfit_key("maya-smart"), None);
}

#[test]
fn wardrobe_stage_previews_owned_slot_without_changing_saved_appearance() {
    use wonderland_contracts::authoring::*;
    let mut projection = wonderland_client_app::authoring::preview_authoring_projection();
    let profile = &mut projection.profiles[0];
    profile.appearance.body = Some("vitaboy:000000010000000d".into());
    profile.wardrobe.categories = vec![WardrobeCategory {
        id: "source-daywear".into(),
        label: "Daywear".into(),
        slot: AppearanceSlot::Body,
    }];
    profile.wardrobe.outfits = vec![OwnedOutfit {
        id: "owned-new".into(),
        content_key: "vitaboy:000000020000000d".into(),
        category_id: "source-daywear".into(),
        label: "New outfit".into(),
        thumbnail: None,
        is_default: false,
        actions: vec![],
    }];
    let mut draft = OutfitDraft {
        character_id: profile.character.id.clone(),
        owned_outfit_id: Some("owned-new".into()),
        action: WardrobeAction::Change,
    };
    assert_eq!(
        wardrobe_preview(profile, &draft).body,
        Some("vitaboy:000000020000000d".into())
    );
    assert_eq!(
        profile.appearance.body,
        Some("vitaboy:000000010000000d".into())
    );
    draft.action = WardrobeAction::SetDefault;
    assert_eq!(wardrobe_preview(profile, &draft), profile.appearance);
    draft.action = WardrobeAction::Delete;
    assert_eq!(wardrobe_preview(profile, &draft), profile.appearance);
}
