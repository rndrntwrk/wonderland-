//! Qualify the actual runtime metadata boundary, including non-JSON map keys.
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use wonderland_content_ir::strings::LocaleSelection;
use wonderland_content_runtime_bridge::{
    content::{ImportOptions, RuntimeMetadata},
    cooked_metadata::{ImportOptionsV1, RuntimeMetadataV1},
    sim_core::{
        avatars::{events::TimeProperty, motives::TsoMotiveTuning, timeline::AnimationMetadata},
        ids::{EntityRef, ObjectId},
        state::{AnimationKey, RoutingSlot, TuningSet},
        world::{slots::SlotSearch, Footprint, FootprintRect, PlacementRules},
    },
};
use wonderland_legacy_formats::semantic::TtabVariant;

fn metadata() -> RuntimeMetadata {
    RuntimeMetadata {
        footprint: Footprint {
            rects: vec![
                FootprintRect {
                    min_x: -8,
                    min_y: -4,
                    max_x: 6,
                    max_y: 7,
                },
                FootprintRect {
                    min_x: 4,
                    min_y: -7,
                    max_x: 10,
                    max_y: -1,
                },
            ],
        },
        // Exercise every source flag as supplied metadata. A's metadata
        // validator permits these independent flags; placement execution is
        // outside this serialization/conversion boundary.
        placement_rules: PlacementRules {
            flags: 0x2fff,
            wall_flags: 0x0555,
            allowed_heights: 0x0248,
            weight: -12,
            level_offset: -2,
            exclusive_wall: true,
            require_center: true,
            require_corner: true,
            disallow_corner: true,
            require_inside: true,
            require_outside: true,
            require_floor_hole: true,
            is_avatar: true,
            allow_person_intersection: true,
            disallow_person_intersection: true,
            zero_extent: true,
            ghost: true,
            ignored: BTreeSet::from([
                EntityRef {
                    object_id: ObjectId(3),
                    generation: 5,
                },
                EntityRef {
                    object_id: ObjectId(300),
                    generation: 11,
                },
            ]),
        },
        master_guid: Some(0x1020_3040),
        family: -73,
        routing_slots: vec![(
            11,
            RoutingSlot {
                search: SlotSearch {
                    min_proximity: 3,
                    max_proximity: 17,
                    optimal_proximity: 9,
                    resolution: 4,
                    directions: 0x53,
                    offset_x: -51,
                    offset_y: 73,
                    offset_z: -13,
                    absolute: true,
                    ignore_rooms: true,
                    square: true,
                    equal_proximity_score: true,
                    random_scoring: true,
                    standing: 17,
                    sitting: -4,
                },
                facing: -2,
                snap_to_direction: true,
                snap_target_slot: Some(3),
            },
        )],
    }
}

fn options() -> ImportOptions {
    ImportOptions {
        locale: LocaleSelection {
            requested: 2,
            default_language: 4,
        },
        ttab_variant: TtabVariant::Tsbo,
        runtime_tuning: TuningSet {
            values: BTreeMap::from([
                ((0, 256, 2), -17),
                ((0x1122_3344, 4096, 5), 311),
                ((0x5566_7788, 8192, 7), -22),
            ]),
            tso_motives: Some(TsoMotiveTuning {
                flat_sim: [101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112],
                category_weights: std::array::from_fn(|category| {
                    std::array::from_fn(|motive| (category * 7 + motive + 1) as i32)
                }),
            }),
            motive_limits: [
                -100, -91, -82, -73, -64, -55, -46, -37, -28, -19, -10, -1, 8, 17, 26, 35,
            ],
            relationship_multipliers: BTreeMap::from([
                (0, -0.0_f32),
                (17, 1.25_f32),
                (255, -2.75_f32),
            ]),
            fire_enabled: true,
        },
        animations: vec![(
            AnimationKey {
                owner: 0x1122_3344,
                scope: 3,
                id: 17,
            },
            AnimationMetadata {
                resource: "authored-animation".into(),
                num_frames: 23,
                time_properties: vec![
                    TimeProperty {
                        time_ms: 200,
                        properties: BTreeMap::from([
                            ("sound".into(), "tone\nsnow 雪".into()),
                            ("xevt".into(), "7".into()),
                        ]),
                    },
                    TimeProperty {
                        time_ms: 40,
                        properties: BTreeMap::from([
                            ("dress".into(), "casual".into()),
                            ("lefthand".into(), "-3".into()),
                            ("righthand".into(), "12".into()),
                        ]),
                    },
                ],
            },
        )],
    }
}

fn metadata_json() -> Value {
    serde_json::to_value(RuntimeMetadataV1::from(metadata())).unwrap()
}

fn options_json() -> Value {
    serde_json::to_value(ImportOptionsV1::from(options())).unwrap()
}

#[test]
fn every_explicit_object_field_survives_json_into_actual_runtime_metadata() {
    let expected = metadata();
    let bytes = serde_json::to_vec(&RuntimeMetadataV1::from(expected.clone())).unwrap();
    let actual = serde_json::from_slice::<RuntimeMetadataV1>(&bytes)
        .unwrap()
        .into_runtime()
        .unwrap();
    assert_eq!(actual.footprint, expected.footprint);
    assert_eq!(actual.placement_rules, expected.placement_rules);
    assert_eq!(actual.master_guid, Some(0x1020_3040));
    assert_eq!(actual.family, -73);
    assert_eq!(actual.routing_slots, expected.routing_slots);
}

#[test]
fn all_runtime_options_and_tso_tuning_survive_json_conversion() {
    let expected = options();
    let bytes = serde_json::to_vec(&ImportOptionsV1::from(expected.clone())).unwrap();
    let actual = serde_json::from_slice::<ImportOptionsV1>(&bytes)
        .unwrap()
        .into_runtime()
        .unwrap();
    assert_eq!(actual.locale, expected.locale);
    assert_eq!(actual.ttab_variant, TtabVariant::Tsbo);
    assert_eq!(actual.runtime_tuning, expected.runtime_tuning);
    assert_eq!(actual.animations, expected.animations);
    assert_eq!(actual.runtime_tuning.values[&(0x1122_3344, 4096, 5)], 311);
    assert_eq!(actual.runtime_tuning.values[&(0x5566_7788, 8192, 7)], -22);
}

#[test]
fn float_bits_and_descending_animation_encounter_order_are_preserved() {
    let encoded = options_json();
    assert_eq!(
        encoded["runtime_tuning"]["relationship_multipliers"][0]["value_bits"],
        json!(0x8000_0000_u32)
    );
    let actual = serde_json::from_value::<ImportOptionsV1>(encoded)
        .unwrap()
        .into_runtime()
        .unwrap();
    assert_eq!(
        actual.runtime_tuning.relationship_multipliers[&0].to_bits(),
        0x8000_0000
    );
    assert_eq!(
        actual.animations[0]
            .1
            .time_properties
            .iter()
            .map(|property| property.time_ms)
            .collect::<Vec<_>>(),
        [200, 40]
    );
    assert_eq!(
        actual.animations[0].1.time_properties[0].properties["xevt"],
        "7"
    );
}

#[test]
fn duplicate_ignored_entity_references_are_rejected_instead_of_coalesced() {
    let mut dto = RuntimeMetadataV1::from(metadata());
    let duplicate = dto.placement_rules.ignored[0].clone();
    dto.placement_rules.ignored.push(duplicate);
    assert!(dto
        .into_runtime()
        .unwrap_err()
        .contains("duplicate placement ignored entity reference"));
}

#[test]
fn duplicate_routing_slot_indices_never_coalesce_or_overwrite() {
    for different in [false, true] {
        let mut dto = RuntimeMetadataV1::from(metadata());
        let mut duplicate = dto.routing_slots[0].clone();
        if different {
            duplicate.slot.facing = 7;
        }
        dto.routing_slots.push(duplicate);
        assert!(dto
            .into_runtime()
            .unwrap_err()
            .contains("duplicate routing slot"));
    }
}

#[test]
fn duplicate_tuning_tuple_keys_never_coalesce_or_overwrite() {
    for different in [false, true] {
        let mut dto = ImportOptionsV1::from(options());
        let mut duplicate = dto.runtime_tuning.values[0].clone();
        if different {
            duplicate.value = 99;
        }
        dto.runtime_tuning.values.push(duplicate);
        assert!(dto
            .into_runtime()
            .unwrap_err()
            .contains("duplicate runtime tuning key"));
    }
}

#[test]
fn duplicate_relationship_multiplier_keys_never_coalesce_or_overwrite() {
    for different in [false, true] {
        let mut dto = ImportOptionsV1::from(options());
        let mut duplicate = dto.runtime_tuning.relationship_multipliers[0].clone();
        if different {
            duplicate.value_bits = 2.5_f32.to_bits();
        }
        dto.runtime_tuning.relationship_multipliers.push(duplicate);
        assert!(dto
            .into_runtime()
            .unwrap_err()
            .contains("duplicate relationship multiplier key"));
    }
}

#[test]
fn duplicate_animation_keys_never_coalesce_or_overwrite() {
    for different in [false, true] {
        let mut dto = ImportOptionsV1::from(options());
        let mut duplicate = dto.animations[0].clone();
        if different {
            duplicate.metadata.resource = "different-animation".into();
        }
        dto.animations.push(duplicate);
        assert!(dto
            .into_runtime()
            .unwrap_err()
            .contains("duplicate animation key"));
    }
}

#[test]
fn duplicate_time_property_keys_never_coalesce_or_overwrite() {
    for different in [false, true] {
        let mut dto = ImportOptionsV1::from(options());
        let properties = &mut dto.animations[0].metadata.time_properties[0].properties;
        let mut duplicate = properties[0].clone();
        if different {
            duplicate.value = "different-sound".into();
        }
        properties.push(duplicate);
        assert!(dto
            .into_runtime()
            .unwrap_err()
            .contains("duplicate animation time-property key"));
    }
}

#[test]
fn nullable_fields_require_explicit_presence_and_accept_explicit_null() {
    for (path, field) in [
        ("", "master_guid"),
        ("/routing_slots/0/slot", "snap_target_slot"),
    ] {
        let mut value = metadata_json();
        value
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<RuntimeMetadataV1>(value).is_err(),
            "{field}"
        );
    }
    let mut value = options_json();
    value["runtime_tuning"]
        .as_object_mut()
        .unwrap()
        .remove("tso_motives");
    assert!(serde_json::from_value::<ImportOptionsV1>(value).is_err());

    let mut value = metadata_json();
    value["master_guid"] = Value::Null;
    value["routing_slots"][0]["slot"]["snap_target_slot"] = Value::Null;
    let actual = serde_json::from_value::<RuntimeMetadataV1>(value)
        .unwrap()
        .into_runtime()
        .unwrap();
    assert_eq!(actual.master_guid, None);
    assert_eq!(actual.routing_slots[0].1.snap_target_slot, None);
    let mut value = options_json();
    value["runtime_tuning"]["tso_motives"] = Value::Null;
    assert!(serde_json::from_value::<ImportOptionsV1>(value)
        .unwrap()
        .into_runtime()
        .unwrap()
        .runtime_tuning
        .tso_motives
        .is_none());
}

#[test]
fn nested_object_metadata_rejects_undeclared_fields() {
    let base = metadata_json();
    serde_json::from_value::<RuntimeMetadataV1>(base.clone()).unwrap();
    for path in [
        "",
        "/footprint",
        "/footprint/rects/0",
        "/placement_rules",
        "/placement_rules/ignored/0",
        "/routing_slots/0",
        "/routing_slots/0/slot",
        "/routing_slots/0/slot/search",
    ] {
        let mut value = base.clone();
        value
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(true));
        assert!(
            serde_json::from_value::<RuntimeMetadataV1>(value).is_err(),
            "{path}"
        );
    }
}

#[test]
fn nested_options_and_event_properties_reject_undeclared_fields() {
    let base = options_json();
    serde_json::from_value::<ImportOptionsV1>(base.clone()).unwrap();
    for path in [
        "",
        "/locale",
        "/runtime_tuning",
        "/runtime_tuning/values/0",
        "/runtime_tuning/tso_motives",
        "/runtime_tuning/relationship_multipliers/0",
        "/animations/0",
        "/animations/0/key",
        "/animations/0/metadata",
        "/animations/0/metadata/time_properties/0",
        "/animations/0/metadata/time_properties/0/properties/0",
    ] {
        let mut value = base.clone();
        value
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(true));
        assert!(
            serde_json::from_value::<ImportOptionsV1>(value).is_err(),
            "{path}"
        );
    }
}

#[test]
fn positional_metadata_cannot_bypass_named_field_checks() {
    let base = metadata_json();
    let slot = &base["routing_slots"][0]["slot"];
    let cases = [
        ("/footprint/rects/0", json!([-8, -4, 6, 7])),
        ("/placement_rules/ignored/0", json!([3, 5])),
        (
            "/routing_slots/0/slot",
            json!([slot["search"], -2, true, 3]),
        ),
    ];
    for (path, positional) in cases {
        let mut value = base.clone();
        *value.pointer_mut(path).unwrap() = positional;
        assert!(
            serde_json::from_value::<RuntimeMetadataV1>(value).is_err(),
            "{path}"
        );
    }
}

#[test]
fn positional_options_and_enum_objects_cannot_bypass_named_field_checks() {
    let base = options_json();
    let event = &base["animations"][0]["metadata"]["time_properties"][0];
    let property = &event["properties"][0];
    let cases = [
        ("/locale", json!([2, 4])),
        ("/animations/0/key", json!([0x1122_3344_u32, 3, 17])),
        (
            "/animations/0/metadata/time_properties/0",
            json!([200, event["properties"]]),
        ),
        (
            "/animations/0/metadata/time_properties/0/properties/0",
            json!([property["key"], property["value"]]),
        ),
        ("/ttab_variant", json!({"tsbo": null})),
    ];
    for (path, positional) in cases {
        let mut value = base.clone();
        *value.pointer_mut(path).unwrap() = positional;
        assert!(
            serde_json::from_value::<ImportOptionsV1>(value).is_err(),
            "{path}"
        );
    }
}

#[test]
fn nonfinite_relationship_bits_never_reach_runtime_tuning() {
    for value_bits in [
        f32::INFINITY.to_bits(),
        f32::NEG_INFINITY.to_bits(),
        f32::NAN.to_bits(),
    ] {
        let mut dto = ImportOptionsV1::from(options());
        dto.runtime_tuning.relationship_multipliers[0].value_bits = value_bits;
        assert!(dto.into_runtime().unwrap_err().contains("finite"));
    }
}
