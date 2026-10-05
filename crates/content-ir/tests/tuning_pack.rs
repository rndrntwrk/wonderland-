use wonderland_content_ir::{tuning::*, tuning_pack};
use wonderland_legacy_formats::{
    iff::{ChunkKey, IffChunk, IffFile},
    Limits,
};

fn fixture() -> ResolvedTuning {
    let chunk = |id| IffChunk {
        key: ChunkKey { kind: *b"BCON", id },
        flags: 0,
        label: [0; 64],
        data: vec![2, 0, 7, 0, 0xff, 0xff],
    };
    let mut header = [0; 64];
    let magic = b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE JAMIE DOORNBOS & MAXIS 1";
    header[..magic.len()].copy_from_slice(magic);
    let private = IffFile {
        header,
        chunks: vec![chunk(4096)],
    };
    let semi = IffFile {
        header,
        chunks: vec![chunk(8192)],
    };
    let global = IffFile {
        header,
        chunks: vec![chunk(256)],
    };
    let inputs = TuningInputs {
        dynamic_private: vec![DynamicOverride {
            table: 0,
            index: 0,
            value_bits: 400.9f32.to_bits(),
        }],
        upgrades: vec![TuningOverride {
            table: 8192,
            index: 1,
            value: -42,
        }],
        ..TuningInputs::default()
    };
    resolve_tuning(
        &private,
        Some(&semi),
        Some(&global),
        &inputs,
        &Limits::default(),
    )
    .unwrap()
}

#[test]
fn effective_tuning_pack_preserves_every_encoded_lookup_and_identity() {
    let limits = Limits::default();
    let original = fixture();
    let bytes = tuning_pack::encode(&original, &limits).unwrap();
    assert_eq!(&bytes[..8], b"WLTUNE\0\0");
    assert_eq!(&bytes[8..16], &[1, 0, 0, 0, 1, 0, 0, 0]);
    assert_eq!(&bytes[16..48], &original.identity);
    assert_eq!(
        &bytes[48..64],
        &[2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0]
    );
    let restored = tuning_pack::decode(&bytes, &limits).unwrap();
    assert_eq!(restored, original);
    for operand in 0..=u16::MAX {
        assert_eq!(
            restored.lookup_encoded(operand),
            original.lookup_encoded(operand)
        );
    }
    assert_eq!(restored.value_or_zero(0), 400);
    assert_eq!(restored.value_or_zero((64 << 7) | 1), -42);
    assert_eq!(tuning_pack::encode(&restored, &limits).unwrap(), bytes);
}

#[test]
fn tuning_pack_rejects_truncation_duplicate_keys_flags_and_preallocation_attacks() {
    let limits = Limits::default();
    let original = tuning_pack::encode(&fixture(), &limits).unwrap();
    for end in 0..original.len() {
        assert!(
            tuning_pack::decode(&original[..end], &limits).is_err(),
            "prefix {end}"
        );
    }
    let mut corrupt = original.clone();
    corrupt[13] = 1;
    assert!(tuning_pack::decode(&corrupt, &limits).is_err());
    let mut corrupt = original.clone();
    corrupt[72..76].copy_from_slice(&original[64..68]);
    assert!(tuning_pack::decode(&corrupt, &limits).is_err());
    let mut corrupt = original.clone();
    corrupt[48..52].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(tuning_pack::decode(&corrupt, &limits).is_err());
    let mut corrupt = original.clone();
    corrupt[70] = 255;
    assert!(tuning_pack::decode(&corrupt, &limits).is_err());
    let mut corrupt = original.clone();
    corrupt.push(0);
    assert!(tuning_pack::decode(&corrupt, &limits).is_err());
    let bounded = Limits {
        max_total_decoded_bytes: 300,
        ..limits
    };
    assert!(tuning_pack::encode(&fixture(), &bounded).is_err());
    assert!(tuning_pack::decode(&original, &bounded).is_err());
}
