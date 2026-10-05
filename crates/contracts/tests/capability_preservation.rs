use serde_json::{Value, json};
use wonderland_contracts::authoring::AuthoringProjection;

fn fixture_value() -> Value {
    serde_json::from_str(include_str!("../../../fixtures/ui/authoring-v1.json")).unwrap()
}

#[test]
fn supplied_seventh_catalog_record_and_price_are_accepted() {
    let mut value = fixture_value();
    let mut item = value["catalog"][0].clone();
    item["id"] = json!("object:0xA971");
    item["name"] = json!("Source piano");
    item["price"] = json!(4200);
    value["catalog"].as_array_mut().unwrap().push(item);
    let projection: AuthoringProjection = serde_json::from_value(value).unwrap();
    assert!(
        projection.validate().is_ok(),
        "supplied catalog content is not limited to six authored records"
    );
}

#[test]
fn existing_money_above_the_preview_starting_balance_is_preserved() {
    let mut value = fixture_value();
    value["profiles"][0]["character"]["money"] = json!(9_876_543);
    let projection: AuthoringProjection = serde_json::from_value(value).unwrap();
    assert!(
        projection.validate().is_ok(),
        "starting money is not an account balance ceiling"
    );
    assert_eq!(projection.profiles[0].character.money, 9_876_543);
}

#[test]
fn saved_profiles_above_the_old_preview_capacity_remain_readable() {
    let mut value = fixture_value();
    for i in 5..11 {
        let mut profile = value["profiles"][0].clone();
        profile["character"]["id"] = json!(format!("saved-profile-{i}"));
        profile["home"]["owner_id"] = json!(format!("saved-profile-{i}"));
        value["profiles"].as_array_mut().unwrap().push(profile);
    }
    let projection: AuthoringProjection = serde_json::from_value(value).unwrap();
    assert!(
        projection.validate().is_ok(),
        "a creation policy cannot delete or invalidate saved profiles"
    );
    assert_eq!(projection.profiles.len(), 11);
}

#[test]
fn independent_appearance_fields_and_description_survive_the_contract() {
    let mut value = fixture_value();
    value["profiles"][0]["description"] = json!("A creator with separately selected parts.");
    value["profiles"][0]["appearance"] = json!({
        "head": "0000000034000000:0000000D",
        "body": "00000000F3000000:0000000D",
        "skin_tone": "medium",
        "gender": "female",
        "decorations": {}
    });
    let projection: AuthoringProjection = serde_json::from_value(value).unwrap();
    let saved = serde_json::to_value(projection).unwrap();
    assert_eq!(
        saved["profiles"][0]["appearance"]["head"],
        json!("0000000034000000:0000000D")
    );
    assert_eq!(
        saved["profiles"][0]["appearance"]["body"],
        json!("00000000F3000000:0000000D")
    );
    assert_eq!(
        saved["profiles"][0]["appearance"]["skin_tone"],
        json!("medium")
    );
    assert_eq!(
        saved["profiles"][0]["appearance"]["gender"],
        json!("female")
    );
    assert_eq!(
        saved["profiles"][0]["description"],
        json!("A creator with separately selected parts.")
    );
}

#[test]
fn supplied_lot_geometry_and_level_allow_placements_outside_the_illustrated_room() {
    let mut value = fixture_value();
    value["profiles"][0]["home"]["lot"] = json!({
        "source": "source-lot",
        "revision": 7,
        "bounds": {"origin": {"x": 0, "y": 0}, "width": 64, "depth": 48},
        "levels": [0, 1, 2, 3],
        "reserved": []
    });
    value["profiles"][0]["home"]["instances"] = json!([{
        "id": "piano-owned-91",
        "catalog_id": "armchair",
        "placement": {"cell": {"x": 31, "y": 42}, "level": 3, "direction": "north"}
    }]);
    let projection: AuthoringProjection = serde_json::from_value(value).unwrap();
    assert!(
        projection.validate().is_ok(),
        "placement uses the supplied lot bounds and levels"
    );
    let saved = serde_json::to_value(projection).unwrap();
    assert_eq!(
        saved["profiles"][0]["home"]["instances"][0]["placement"]["level"],
        json!(3)
    );
}

#[test]
fn stored_possessions_above_the_old_preview_cap_remain_readable() {
    let mut value = fixture_value();
    let instances = (0..80)
        .map(|i| {
            json!({
                "id": format!("saved-object-{i}"),
                "catalog_id": "fern",
                "placement": null
            })
        })
        .collect::<Vec<_>>();
    value["profiles"][0]["home"]["instances"] = json!(instances);
    let projection: AuthoringProjection = serde_json::from_value(value).unwrap();
    assert!(
        projection.validate().is_ok(),
        "a fixture count must not discard saved possessions"
    );
    assert_eq!(projection.profiles[0].home.instances.len(), 80);
}
