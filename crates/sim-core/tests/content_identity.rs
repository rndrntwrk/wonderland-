use bincode::Options;
use sim_core::{
    avatars::timeline::AnimationMetadata,
    interactions::{
        ActionFlags, InteractionDefinition, InteractionKey, InteractionScope, PermissionFlags,
        RoutineBinding,
    },
    state::{
        AnimationKey, ContentSet, InteractionAdvertisement, InteractionTable, LegacyJobUniform,
        ObjectDefinition, RoutingSlot, TuningSet, MAX_CONTENT_BYTES,
    },
    vm::{ResolvedSuit, RoutineKey, RoutineScope, RoutineStore, VmInstruction, VmRoutine},
    world::slots::SlotSearch,
};
use std::collections::BTreeMap;

// The pre-cache serialized field sequence. This fixture also lets an untrusted
// decoder supply invalid content without adding a mutation API to ContentSet.
type WireContent = (
    RoutineStore,
    BTreeMap<u32, ObjectDefinition>,
    BTreeMap<AnimationKey, AnimationMetadata>,
    BTreeMap<(u32, u16), RoutingSlot>,
    BTreeMap<(u32, u16), Vec<String>>,
    BTreeMap<(u32, String), RoutineKey>,
    BTreeMap<(u32, u8, u16), ResolvedSuit>,
    BTreeMap<(i16, i16), LegacyJobUniform>,
    TuningSet,
    BTreeMap<u32, InteractionTable>,
    BTreeMap<u32, u32>,
    BTreeMap<(u32, InteractionKey), InteractionAdvertisement>,
);

fn encode(value: &impl serde::Serialize) -> Vec<u8> {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .serialize(value)
        .unwrap()
}

fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> T {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .reject_trailing_bytes()
        .deserialize(bytes)
        .unwrap()
}

fn key() -> InteractionKey {
    InteractionKey {
        tta_index: 0,
        scope: InteractionScope::Local,
    }
}

fn table() -> InteractionTable {
    InteractionTable {
        local_table_present: true,
        definitions: vec![InteractionDefinition {
            key: key(),
            action: RoutineBinding {
                code_owner_guid: 100,
                routine_id: 4096,
            },
            check: None,
            flags: ActionFlags::default(),
            permissions: PermissionFlags::default(),
            label: Some("source action".into()),
        }],
    }
}

fn advertisement() -> InteractionAdvertisement {
    InteractionAdvertisement {
        motives: vec![],
        attenuation_code: 0,
        attenuation_value_bits: 0,
        autonomy_threshold: 0,
        joining_index: 0,
    }
}

fn content() -> ContentSet {
    let mut routines = RoutineStore::new();
    routines
        .insert(
            RoutineKey {
                scope: RoutineScope::Private(100),
                id: 4096,
            },
            VmRoutine::new(4096, 0, 0, vec![VmInstruction::new(2, 254, 255, [0; 8])]).unwrap(),
        )
        .unwrap();
    ContentSet::new(
        routines,
        vec![ObjectDefinition::new(100, 0), ObjectDefinition::new(200, 0)],
        vec![],
        TuningSet::default(),
    )
    .unwrap()
    .with_interaction_tables(vec![(100, table())])
    .unwrap()
}

fn slot() -> RoutingSlot {
    RoutingSlot {
        search: SlotSearch::default(),
        facing: 0,
        snap_to_direction: false,
        snap_target_slot: None,
    }
}

type Builder = fn(ContentSet) -> Result<ContentSet, String>;

#[test]
fn every_builder_rebinds_a_warm_cloned_content_identity() {
    let original = content();
    original.validate().unwrap();
    let before = original.descriptor().unwrap();
    let builders: &[(&str, Builder)] = &[
        ("catalog", |c| c.with_build_catalog(vec![(1, 100)])),
        ("advertisements", |c| {
            c.with_interaction_advertisements(vec![((100, key()), advertisement())])
        }),
        ("tables", |c| {
            c.with_interaction_tables(vec![(
                200,
                InteractionTable {
                    local_table_present: true,
                    definitions: vec![],
                },
            )])
        }),
        ("slots", |c| c.with_routing_slots(vec![((100, 0), slot())])),
        ("uniforms", |c| {
            c.with_legacy_job_uniforms(vec![(
                (1, 0),
                LegacyJobUniform {
                    male_mesh: "source-mesh".into(),
                    female_mesh: None,
                    texture: "source-texture".into(),
                },
            )])
        }),
        ("strings", |c| {
            c.with_strings(vec![((100, 1), vec!["source string".into()])])
        }),
        ("named trees", |c| {
            c.with_named_trees(vec![(
                (100, "source tree".into()),
                RoutineKey {
                    scope: RoutineScope::Private(100),
                    id: 4096,
                },
            )])
        }),
        ("suits", |c| {
            c.with_suits(vec![(
                (100, 0, 0),
                ResolvedSuit::Accessory("source-suit".into()),
            )])
        }),
    ];
    for (name, build) in builders {
        let changed = build(original.clone()).unwrap();
        changed.validate().unwrap();
        let after = changed.descriptor().unwrap();
        assert_ne!(before.content_hash, after.content_hash, "{name}");
        assert_eq!(before.tuning_hash, after.tuning_hash, "{name}");
        assert_eq!(
            before,
            original.descriptor().unwrap(),
            "{name} changed its parent"
        );
    }
}

#[test]
fn warmed_validation_cannot_admit_invalid_builder_content() {
    let original = content();
    original.validate().unwrap();
    original.descriptor().unwrap();
    let builders: &[(&str, Builder)] = &[
        ("catalog", |c| c.with_build_catalog(vec![(1, 999)])),
        ("advertisements", |c| {
            c.with_interaction_advertisements(vec![((200, key()), advertisement())])
        }),
        ("tables", |c| {
            c.with_interaction_tables(vec![(999, table())])
        }),
        ("slots", |c| {
            let mut invalid = slot();
            invalid.search.resolution = 0;
            c.with_routing_slots(vec![((100, 0), invalid)])
        }),
        ("uniforms", |c| {
            c.with_legacy_job_uniforms(vec![(
                (1, 0),
                LegacyJobUniform {
                    male_mesh: "x".repeat(16385),
                    female_mesh: None,
                    texture: String::new(),
                },
            )])
        }),
        ("strings", |c| {
            c.with_strings(vec![((100, 1), vec!["x".repeat(65537)])])
        }),
        ("named trees", |c| {
            c.with_named_trees(vec![(
                (100, "missing tree".into()),
                RoutineKey {
                    scope: RoutineScope::Private(100),
                    id: 4097,
                },
            )])
        }),
        ("suits", |c| {
            c.with_suits(vec![(
                (100, 0, 0),
                ResolvedSuit::Accessory("x".repeat(16385)),
            )])
        }),
    ];
    for (name, build) in builders {
        assert!(
            build(original.clone()).is_err(),
            "{name} reused stale validation"
        );
    }
}

#[test]
fn warm_and_cold_content_keep_pre_cache_bytes_and_equality() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ContentSet>();
    let empty = ContentSet::new(RoutineStore::new(), vec![], vec![], TuningSet::default()).unwrap();
    // Two RoutineStore maps, seven content maps, then tuning: one map,
    // Option::None, sixteen i16 motive limits, one map, bool, three maps.
    let mut expected = vec![0; 146];
    for index in 0..16 {
        expected[81 + 2 * index] = 100;
    }
    assert_eq!(encode(&empty), expected);
    let cold: ContentSet = decode(&expected);
    let warm = cold.clone();
    warm.validate().unwrap();
    warm.descriptor().unwrap();
    assert_eq!(cold, warm);
    assert_eq!(encode(&cold), expected);
    assert_eq!(encode(&warm), expected);
    assert_eq!(encode(&warm.clone()), expected);
    assert_eq!(decode::<ContentSet>(&encode(&warm)), cold);
}

#[test]
fn deserialized_invalid_content_retains_errors_until_a_builder_repairs_it() {
    let valid = content();
    valid.validate().unwrap();
    valid.descriptor().unwrap();
    let mut wire: WireContent = decode(&encode(&valid));
    wire.9.clear();
    wire.11.insert((100, key()), advertisement());
    let invalid: ContentSet = decode(&encode(&wire));
    let before = invalid.descriptor().unwrap();
    let error = invalid.validate().unwrap_err();
    assert!(error.contains("interaction advertisement"));
    assert_eq!(invalid.validate().unwrap_err(), error);
    assert_eq!(invalid.clone().validate().unwrap_err(), error);
    let repaired = invalid
        .clone()
        .with_interaction_tables(vec![(100, table())])
        .unwrap();
    repaired.validate().unwrap();
    assert_ne!(before, repaired.descriptor().unwrap());
    assert_eq!(invalid.validate().unwrap_err(), error);
}

#[test]
fn descriptor_size_errors_survive_repeated_queries_and_clone() {
    let mut wire: WireContent = decode(&encode(&content()));
    wire.4
        .insert((100, 1), vec!["x".repeat(MAX_CONTENT_BYTES as usize)]);
    let oversized: ContentSet = decode(&encode(&wire));
    let error = oversized.descriptor().unwrap_err();
    assert_eq!(oversized.descriptor().unwrap_err(), error);
    assert_eq!(oversized.clone().descriptor().unwrap_err(), error);
    assert!(oversized.validate().is_err());
}
