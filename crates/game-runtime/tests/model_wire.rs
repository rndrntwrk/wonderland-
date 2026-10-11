//! Regression for original WLB1 model metadata; no alternative model schema.
use bincode::Options;
use wonderland_game_runtime::world_view::{ModelContext, ModelTextureSelector};
use wonderland_render_core::AssetKey;
fn codec() -> impl Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .reject_trailing_bytes()
}
#[test]
fn model_contexts_decode_their_existing_tagged_binary_layout() {
    for value in [
        ModelContext::Standalone,
        ModelContext::Vitaboy,
        ModelContext::Dgrp {
            effective_iff: AssetKey([7; 32]),
            chunk_id: 65535,
        },
    ] {
        let bytes = codec().serialize(&value).unwrap();
        let restored: ModelContext = codec().deserialize(&bytes).unwrap();
        assert_eq!(restored, value);
    }
}
#[test]
fn model_selectors_decode_their_existing_tagged_binary_layout() {
    for value in [
        ModelTextureSelector::Custom { id: 65535 },
        ModelTextureSelector::Sprite {
            rotation: 3,
            ordinal: 7,
        },
    ] {
        let bytes = codec().serialize(&value).unwrap();
        let restored: ModelTextureSelector = codec().deserialize(&bytes).unwrap();
        assert_eq!(restored, value);
    }
}
#[test]
fn model_metadata_binary_bytes_remain_exact() {
    let mut standalone = 10u64.to_le_bytes().to_vec();
    standalone.extend_from_slice(b"standalone");
    assert_eq!(
        codec().serialize(&ModelContext::Standalone).unwrap(),
        standalone
    );
    let mut custom = 6u64.to_le_bytes().to_vec();
    custom.extend_from_slice(b"custom");
    custom.extend_from_slice(&9u16.to_le_bytes());
    assert_eq!(
        codec()
            .serialize(&ModelTextureSelector::Custom { id: 9 })
            .unwrap(),
        custom
    );
}
#[test]
fn file_facing_json_retains_existing_model_tags() {
    let v = ModelContext::Dgrp {
        effective_iff: AssetKey([9; 32]),
        chunk_id: 23,
    };
    let json = serde_json::to_value(v).unwrap();
    assert_eq!(json["kind"], "dgrp");
    assert_eq!(serde_json::from_value::<ModelContext>(json).unwrap(), v);
    let v = ModelTextureSelector::Sprite {
        rotation: 2,
        ordinal: 17,
    };
    let json = serde_json::to_value(v).unwrap();
    assert_eq!(json["kind"], "sprite");
    assert_eq!(
        serde_json::from_value::<ModelTextureSelector>(json).unwrap(),
        v
    );
}
#[test]
fn unknown_model_tags_and_truncated_fields_reject() {
    let mut wrong = 4u64.to_le_bytes().to_vec();
    wrong.extend_from_slice(b"nope");
    assert!(codec().deserialize::<ModelContext>(&wrong).is_err());
    assert!(codec().deserialize::<ModelTextureSelector>(&wrong).is_err());
    let contexts = [
        ModelContext::Dgrp {
            effective_iff: AssetKey([7; 32]),
            chunk_id: 2,
        },
        ModelContext::Standalone,
    ];
    for value in contexts {
        let bytes = codec().serialize(&value).unwrap();
        for end in 0..bytes.len() {
            assert!(codec().deserialize::<ModelContext>(&bytes[..end]).is_err());
        }
    }
    let selectors = [
        ModelTextureSelector::Custom { id: 9 },
        ModelTextureSelector::Sprite {
            rotation: 1,
            ordinal: 2,
        },
    ];
    for value in selectors {
        let bytes = codec().serialize(&value).unwrap();
        for end in 0..bytes.len() {
            assert!(
                codec()
                    .deserialize::<ModelTextureSelector>(&bytes[..end])
                    .is_err()
            );
        }
    }
    // Context and texture tags may not be interchanged.
    assert!(
        codec()
            .deserialize::<ModelContext>(
                &codec()
                    .serialize(&ModelTextureSelector::Custom { id: 9 })
                    .unwrap()
            )
            .is_err()
    );
    assert!(
        codec()
            .deserialize::<ModelTextureSelector>(
                &codec().serialize(&ModelContext::Standalone).unwrap()
            )
            .is_err()
    );
}
#[test]
fn consecutive_contexts_do_not_consume_the_next_record() {
    let values = (
        ModelContext::Standalone,
        0xDEADBEEFu32,
        ModelContext::Dgrp {
            effective_iff: AssetKey([3; 32]),
            chunk_id: 19,
        },
        ModelContext::Vitaboy,
    );
    let bytes = codec().serialize(&values).unwrap();
    assert_eq!(
        codec()
            .deserialize::<(ModelContext, u32, ModelContext, ModelContext)>(&bytes)
            .unwrap(),
        values
    );
    let mut trailing = bytes;
    trailing.push(0);
    assert!(
        codec()
            .deserialize::<(ModelContext, u32, ModelContext, ModelContext)>(&trailing)
            .is_err()
    );
}
