use wonderland_web_shell::source_needs::{SOURCE_NEED_LABELS, source_needs};

#[test]
fn the_profile_and_lot_label_each_original_motive_with_its_own_value() {
    // Original VMMotive has non-display fields at 0..=4 and 10..=12.
    // Distinct values catch the previous profile's Energy/Hunger/etc. swap.
    let original = [
        900, 901, 902, 903, 904, -92, -61, -28, 3, 37, 910, 911, 912, 68, 84, 99,
    ];
    let labeled: Vec<_> = SOURCE_NEED_LABELS
        .into_iter()
        .zip(source_needs(&original).unwrap())
        .collect();
    assert_eq!(
        labeled,
        vec![
            ("Energy", -92),
            ("Comfort", -61),
            ("Hunger", -28),
            ("Hygiene", 3),
            ("Bladder", 37),
            ("Room", 68),
            ("Social", 84),
            ("Fun", 99),
        ]
    );
}

#[test]
fn incomplete_source_motives_stay_unknown_instead_of_becoming_healthy_values() {
    assert_eq!(source_needs(&[]), None);
    assert_eq!(source_needs(&[50; 15]), None);
}

#[test]
fn projecting_needs_preserves_original_signed_values_and_ignores_extra_fields() {
    let original = [-120; 20];
    assert_eq!(source_needs(&original), Some([-120; 8]));
}
