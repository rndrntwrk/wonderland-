use wonderland_client_app::authoring::{AuthoringState, preview_authoring_projection};
use wonderland_contracts::authoring::*;
use wonderland_web_shell::persistence::*;
#[test]
fn bounded_acknowledged_roundtrip_excludes_draft_pending() {
    let snapshot = preview_authoring_projection();
    let mut state = AuthoringState::new(snapshot.clone());
    state.dispatch(AuthoringIntent::OpenCreate).unwrap();
    state
        .dispatch(AuthoringIntent::UpdateName("draft secret".into()))
        .unwrap();
    state.dispatch(AuthoringIntent::SubmitCreate).unwrap();
    let json = encode_snapshot(state.projection()).unwrap();
    assert!(!json.contains("draft secret"));
    assert!(!json.contains("operation_id"));
    assert_eq!(decode_snapshot(&json).unwrap(), snapshot);
}
#[test]
fn rejects_corrupt_version_invalid_and_oversize() {
    assert!(decode_snapshot("broken").is_err());
    let json = encode_snapshot(&preview_authoring_projection()).unwrap();
    assert!(decode_snapshot(&json.replacen("\"version\":2", "\"version\":9", 1)).is_err());
    assert!(decode_snapshot(&" ".repeat(MAX_AUTHORING_JSON_BYTES + 1)).is_err());
    let mut p = preview_authoring_projection();
    p.profiles[0].character.money = -1;
    assert!(encode_snapshot(&p).is_err());
}

#[test]
fn stale_session_preserves_newer_saved_profile_and_stays_temporary() {
    use std::cell::RefCell;
    let original = encode_snapshot(&preview_authoring_projection()).unwrap();
    let storage = RefCell::new(Some(original.clone()));
    let mut first = SaveSession::new(Some(original.clone()));
    let mut stale = SaveSession::new(Some(original));
    let mut provider = wonderland_client_app::authoring::PreviewAuthoringProvider::new(
        preview_authoring_projection(),
    )
    .unwrap();
    let mut state = AuthoringState::new(preview_authoring_projection());
    state.dispatch(AuthoringIntent::OpenCreate).unwrap();
    state
        .dispatch(AuthoringIntent::UpdateName("Newer profile".into()))
        .unwrap();
    let request = state
        .dispatch(AuthoringIntent::SubmitCreate)
        .unwrap()
        .remove(0);
    state.receive(provider.handle(&request)).unwrap();
    let newer = encode_snapshot(state.projection()).unwrap();
    assert_eq!(
        first.compare_and_save(
            &newer,
            || Ok(storage.borrow().clone()),
            |value| {
                *storage.borrow_mut() = Some(value.into());
                Ok(())
            }
        ),
        SaveOutcome::Saved
    );
    // Tab B independently accepted a purchase from the snapshot it loaded.
    let mut stale_state = AuthoringState::new(preview_authoring_projection());
    let mut stale_provider = wonderland_client_app::authoring::PreviewAuthoringProvider::new(
        preview_authoring_projection(),
    )
    .unwrap();
    stale_state
        .dispatch(AuthoringIntent::SelectProfile("maya".into()))
        .unwrap();
    stale_state
        .dispatch(AuthoringIntent::OpenHome("maya".into()))
        .unwrap();
    stale_state
        .dispatch(AuthoringIntent::SelectCatalog("fern".into()))
        .unwrap();
    let outfit = stale_state
        .dispatch(AuthoringIntent::ConfirmPlacement)
        .unwrap()
        .remove(0);
    stale_state.receive(stale_provider.handle(&outfit)).unwrap();
    let older = encode_snapshot(stale_state.projection()).unwrap();
    assert_eq!(
        stale.compare_and_save(
            &older,
            || Ok(storage.borrow().clone()),
            |_| panic!("a conflict must never write")
        ),
        SaveOutcome::Temporary
    );
    assert_eq!(*storage.borrow(), Some(newer));
    assert!(!stale.writable());
    assert!(stale.notice().to_lowercase().contains("another tab"));
    assert!(stale.notice().contains("Reload"));
    assert_eq!(
        stale.compare_and_save(
            &older,
            || panic!("disabled session must not read"),
            |_| panic!("disabled session must not write")
        ),
        SaveOutcome::Temporary
    );
}

#[test]
fn failed_write_is_retryable_and_success_clears_only_transient_notice() {
    let mut session = SaveSession::new(None);
    let json = encode_snapshot(&preview_authoring_projection()).unwrap();
    let failure = session.compare_and_save(&json, || Ok(None), |_| Err("quota".into()));
    assert_eq!(failure, SaveOutcome::WriteFailed);
    assert!(session.writable());
    assert!(session.notice().contains("not saved"));
    assert!(!failure.announcement().contains("change saved"));
    assert_eq!(
        session.compare_and_save(&json, || Ok(None), |_| Ok(())),
        SaveOutcome::Saved
    );
    assert!(session.notice().is_empty());
    assert_eq!(
        SaveOutcome::Saved.announcement(),
        "Preview change saved on this device."
    );
    let mut invalid = SaveSession::temporary("Invalid saved preview; existing data is preserved.");
    assert_eq!(
        invalid.compare_and_save(&json, || panic!("disabled"), |_| panic!("disabled")),
        SaveOutcome::Temporary
    );
    assert!(invalid.notice().contains("Invalid saved preview"));
}

#[test]
fn comparison_uses_exact_observed_bytes_and_retry_detects_intervening_save() {
    let json = encode_snapshot(&preview_authoring_projection()).unwrap();
    let mut session = SaveSession::new(Some(json.clone()));
    assert_eq!(
        session.compare_and_save(&json, || Ok(Some(json.clone())), |_| Err("quota".into())),
        SaveOutcome::WriteFailed
    );
    assert_eq!(
        session.compare_and_save(
            &json,
            || Ok(Some(format!(" {json}"))),
            |_| panic!("exact comparison must preserve changed storage")
        ),
        SaveOutcome::Temporary
    );
}

#[test]
fn v1_migration_keeps_all_profiles_money_possessions_and_portraits_without_outfit_fabrication() {
    let mut value: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/ui/authoring-v1.json")).unwrap();
    for index in 5..11 {
        let mut profile = value["profiles"][0].clone();
        profile["character"]["id"] = serde_json::json!(format!("legacy-{index}"));
        profile["home"]["owner_id"] = serde_json::json!(format!("legacy-{index}"));
        value["profiles"].as_array_mut().unwrap().push(profile);
    }
    value["profiles"][0]["character"]["money"] = serde_json::json!(9_876_543);
    value["profiles"][0]["character"]["name"] = serde_json::json!("Zoë 李");
    value["profiles"][1]["look_id"] = serde_json::json!("jules-smart");
    value["profiles"][0]["home"]["instances"] = serde_json::json!(
        (0..80)
            .map(|index| serde_json::json!({
                "id": format!("old-owned-{index}"), "catalog_id": "fern", "placement": null
            }))
            .collect::<Vec<_>>()
    );
    let raw = serde_json::json!({"version": 1, "snapshot": value}).to_string();
    let migrated = decode_snapshot(&raw).unwrap();
    assert_eq!(migrated.version, AUTHORING_VERSION);
    assert_eq!(migrated.profiles.len(), 11);
    assert_eq!(migrated.profiles[0].character.money, 9_876_543);
    assert_eq!(migrated.profiles[0].character.name, "Zoë 李");
    assert_eq!(migrated.profiles[0].home.instances.len(), 80);
    assert_eq!(
        migrated.profiles[0].home.instances[79].id.as_ref(),
        "old-owned-79"
    );
    let portrait = migrated.profiles[1].portrait.as_ref().unwrap();
    assert_eq!(portrait.identity, "jules");
    assert_eq!(portrait.look_id, "jules-smart");
    assert_eq!(portrait.asset_path, "/assets/authoring/jules-smart.png");
    for profile in &migrated.profiles {
        assert!(profile.appearance.head.is_none());
        assert!(profile.appearance.body.is_none());
        assert!(profile.wardrobe.outfits.is_empty());
    }
    assert!(migrated.creation_allowed().is_err());
    let upgraded = encode_snapshot(&migrated).unwrap();
    assert_eq!(decode_snapshot(&upgraded).unwrap(), migrated);
    let mut session = SaveSession::new(Some(raw.clone()));
    assert_eq!(
        session.compare_and_save(&upgraded, || Ok(Some(raw.clone())), |_| Ok(())),
        SaveOutcome::Saved
    );
}
