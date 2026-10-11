use std::collections::BTreeSet;
use wonderland_contracts::*;

fn fixture() -> UiProjection {
    serde_json::from_str(include_str!("../../../fixtures/ui/preview-v1.json")).unwrap()
}

#[test]
fn fixture_round_trips_with_all_eight_needs_and_bounded_anchors() {
    let projection = fixture();
    projection.validate().unwrap();
    assert_eq!(
        projection
            .characters
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["Maya", "Jules", "Nico", "Amara", "Leo"]
    );
    let expected = BTreeSet::from([
        Need::Energy,
        Need::Hunger,
        Need::Fun,
        Need::Social,
        Need::Hygiene,
        Need::Bladder,
        Need::Comfort,
        Need::Room,
    ]);
    for character in &projection.characters {
        assert_eq!(
            character.needs.keys().copied().collect::<BTreeSet<_>>(),
            expected
        );
    }
    let encoded = serde_json::to_string(&projection).unwrap();
    let decoded: UiProjection = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, projection);
}

#[test]
fn malformed_projection_is_rejected_at_validation_boundary() {
    let valid = fixture();
    let mut p = valid.clone();
    p.version = 2;
    assert!(p.validate().is_err());
    let mut p = valid.clone();
    p.characters[0].id = "".into();
    assert!(p.validate().is_err());
    let mut p = valid.clone();
    p.characters.push(p.characters[0].clone());
    assert!(p.validate().is_err());
    let mut p = valid.clone();
    p.characters[0].needs.remove(&Need::Room);
    assert!(p.validate().is_err());
    let mut p = valid.clone();
    p.characters[0].needs.insert(Need::Energy, 101);
    assert!(p.validate().is_err());
    let mut p = valid.clone();
    p.places[0].anchor.x = 1.1;
    assert!(p.validate().is_err());
    let mut p = valid.clone();
    p.objects[0].place_id = "unknown".into();
    assert!(p.validate().is_err());
    let mut p = valid.clone();
    let duplicate = p.objects[0].offers[0].clone();
    p.objects[0].offers.push(duplicate);
    assert!(p.validate().is_err());
    let mut p = valid.clone();
    p.characters[0].name = "a".repeat(129);
    assert!(p.validate().is_err());
    let mut p = valid;
    p.objects[0].offers = (0..17)
        .map(|n| ActionOffer {
            id: format!("action-{n}").into(),
            label: "Inspect".into(),
            availability: Availability::Available,
        })
        .collect();
    assert!(p.validate().is_err());
}
