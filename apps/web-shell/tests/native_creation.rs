use wonderland_contracts::{Availability, authoring::*};
use wonderland_web_shell::native_creation::*;
fn content() -> AppearanceContent {
    let option = |key: &str| AppearanceOption {
        key: key.into(),
        label: key.into(),
        thumbnail: None,
        availability: Availability::Available,
        genders: vec!["vitaboy:gender:0".into(), "vitaboy:gender:1".into()],
        skin_tones: vec!["vitaboy:skin:dark".into(), "vitaboy:skin:light".into()],
    };
    AppearanceContent {
        source: "fixture".into(),
        revision: 1,
        heads: vec![
            option("vitaboy:002000010000000d"),
            option("vitaboy:000000030000000d"),
        ],
        bodies: vec![option("vitaboy:ffffffffffffffff")],
        genders: vec![option("vitaboy:gender:0"), option("vitaboy:gender:1")],
        skin_tones: vec![option("vitaboy:skin:light"), option("vitaboy:skin:dark")],
        requirements: AppearanceRequirements {
            head: true,
            body: true,
            skin_tone: true,
            gender: true,
        },
        rendering: Availability::Available,
    }
}
fn catalog() -> CreationCatalog {
    CreationCatalog {
        release: "b".repeat(64),
        available_slots: 3,
        entries: vec![
            Entry {
                key: "9007203549708301".into(),
                part: "head".into(),
                gender: "female".into(),
                skins: vec!["dark".into()],
            },
            Entry {
                key: "18446744073709551615".into(),
                part: "body".into(),
                gender: "female".into(),
                skins: vec!["dark".into()],
            },
        ],
    }
}
#[test]
fn creator_keeps_original_packed_keys_and_only_eligible_parts() {
    let source = content();
    let c = catalog();
    let filtered = c.choices(&source);
    assert_eq!(filtered.heads.len(), 1);
    let selection = filtered.default_selection();
    let spec = c
        .specification(&source, &selection, "New Traveller", "Original parts")
        .unwrap();
    assert_eq!(spec.head_key, "9007203549708301");
    assert_eq!(spec.body_key, "18446744073709551615");
    assert_eq!(spec.gender, "female");
    assert_eq!(spec.skin, "dark");
    assert_eq!(source.heads.len(), 2);
}
#[test]
fn creator_rejects_capacity_unowned_parts_and_wrong_gender() {
    let source = content();
    let mut c = catalog();
    let mut a = c.choices(&source).default_selection();
    a.head = Some("vitaboy:000000030000000d".into());
    assert!(c.specification(&source, &a, "New Traveller", "").is_err());
    a = c.choices(&source).default_selection();
    a.gender = Some("vitaboy:gender:0".into());
    assert!(c.specification(&source, &a, "New Traveller", "").is_err());
    a = c.choices(&source).default_selection();
    c.available_slots = 0;
    assert!(c.specification(&source, &a, "New Traveller", "").is_err());
}
#[test]
fn unrestricted_source_metadata_is_narrowed_to_server_eligibility() {
    let mut source = content();
    source.heads[0].genders.clear();
    source.heads[0].skin_tones.clear();
    let c = catalog();
    let choices = c.choices(&source);
    assert_eq!(choices.heads.len(), 1);
    let spec = c
        .specification(&source, &choices.default_selection(), "New Traveller", "")
        .unwrap();
    assert_eq!(spec.gender, "female");
    assert_eq!(spec.skin, "dark");
}
