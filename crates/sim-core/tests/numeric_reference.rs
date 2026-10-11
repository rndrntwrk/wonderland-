//! Numeric vectors are extracted from the pinned C# source and the separately
//! runnable Mono probe in tools/swarm-a/reference-numeric.cs. This is not a
//! claim that a complete FreeSO executable/content corpus was exercised.
use serde::Serialize;
use sim_core::ids::{EntityRef, IdAllocator, IdError, ObjectId, PersistentId};
use sim_core::numeric::*;
use sim_core::rng::SimRng;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn integer_promotion_wrapping_and_narrowing_match_source() {
    assert_eq!(wrapping_add_i32(i32::MAX, 1), i32::MIN);
    assert_eq!(wrapping_sub_i32(i32::MIN, 1), i32::MAX);
    assert_eq!(wrapping_mul_i32(i32::MAX, 2), -2);
    assert_eq!(narrow_i16(32768), -32768);
    assert_eq!(narrow_i16(65535), -1);
    assert_eq!(narrow_i16(65536), 0);
    assert_eq!(narrow_u16(-1), 65535);

    // VMExpression compares the promoted temporary after the write, rather
    // than comparing the narrowed short which the destination now contains.
    let incremented = wrapping_add_i32(i16::MAX as i32, 1);
    assert_eq!(narrow_i16(incremented), i16::MIN);
    assert!(!(incremented < 0));
    let decremented = wrapping_sub_i32(i16::MIN as i32, 1);
    assert_eq!(narrow_i16(decremented), i16::MAX);
    assert!(!(decremented > 0));
}

#[test]
fn division_and_remainder_match_extracted_mono_faults() {
    assert_eq!(div_i32(17, 0), Ok(-1));
    assert_eq!(div_i32(-7, 3), Ok(-2));
    assert_eq!(div_i32(7, -3), Ok(-2));
    assert_eq!(rem_i32(-7, 3), Ok(-1));
    assert_eq!(rem_i32(7, -3), Ok(1));
    assert_eq!(rem_i32(7, 0), Err(NumericError::DivideByZero));
    for result in [
        div_i32(i32::MIN, -1),
        rem_i32(i32::MIN, -1),
        mod_i32(i32::MIN, -1),
    ] {
        assert_eq!(result, Err(NumericError::DivisionOverflow));
    }
}

#[test]
fn normalized_modulo_preserves_source_order_and_overflow() {
    assert_eq!(mod_i32(-7, 0), Ok(-7));
    assert_eq!(mod_i32(-7, 3), Ok(2));
    assert_eq!(mod_i32(7, -3), Ok(-2));
    assert_eq!(mod_i32(-7, -3), Ok(-1));
    assert_eq!(mod_i32(1, i32::MAX), Ok(-1));
    assert_eq!(mod_i32(i32::MAX - 1, i32::MAX), Ok(-3));
}

#[test]
fn shifts_and_one_based_flags_keep_csharp_masks_and_sign_test() {
    assert_eq!(shl_i32(1, -1), i32::MIN);
    assert_eq!(shl_i32(1, 32), 1);
    assert_eq!(shr_i32(i32::MIN, 31), -1);
    assert_eq!(shr_i32(-7, 32), -7);
    assert_eq!(shl_u64(1, 64), 1);
    assert_eq!(shr_u64(u64::MAX, -1), 1);
    assert_eq!(flag_mask_1based(0), i32::MIN);
    assert_eq!(flag_mask_1based(33), 1);
    assert_eq!(set_flag_i32(0, 16), 32768);
    assert_eq!(clear_flag_i32(-1, 16), -32769);
    assert!(is_flag_set_i32(-1, 16));
    assert!(is_flag_set_i32(1, 33));
    assert!(!is_flag_set_i32(-1, 32));
    assert!(!is_flag_set_i32(-1, 0));
}

#[test]
fn ties_to_even_preserves_negative_zero_and_special_values() {
    for (input, expected) in [
        (-2.5, -2.0),
        (-1.5, -2.0),
        (-0.5, -0.0),
        (-0.0, -0.0),
        (0.5, 0.0),
        (1.5, 2.0),
        (2.5, 2.0),
        (0.49999999999999994, 0.0),
        (0.5000000000000001, 1.0),
        (4503599627370495.5, 4503599627370496.0),
        (9007199254740992.0, 9007199254740992.0),
    ] {
        assert_eq!(round_ties_even(input).to_bits(), f64::to_bits(expected));
    }
    for bits in [0x7ff0000000000000, 0xfff0000000000000, 0xfff8000000000000] {
        assert_eq!(round_ties_even(f64::from_bits(bits)).to_bits(), bits);
    }
}

#[test]
fn float_narrowing_and_sqrt_match_pinned_mono_policy() {
    for (input, expected_i32, expected_i16) in [
        (32767.9, 32767, 32767),
        (32768.0, 32768, -32768),
        (46340.0, 46340, -19196),
        (65535.0, 65535, -1),
        (65536.0, 65536, 0),
        (-32769.0, -32769, 32767),
        (2147483647.0, i32::MAX, -1),
        (2147483647.9, i32::MAX, -1),
        (2147483648.0, i32::MIN, 0),
        (-2147483649.0, i32::MIN, 0),
        (f64::NAN, i32::MIN, 0),
        (f64::INFINITY, i32::MIN, 0),
        (f64::NEG_INFINITY, i32::MIN, 0),
    ] {
        assert_eq!(legacy_f64_to_i32(input), expected_i32);
        assert_eq!(legacy_f64_to_i16(input), expected_i16);
    }
    for (input, expected) in [
        (-1, 0),
        (0, 0),
        (1, 1),
        (2, 1),
        (15, 3),
        (16, 4),
        (i32::MAX, -19196),
    ] {
        assert_eq!(sqrt_i16(input), expected);
    }
}

#[test]
fn rng_matches_extracted_csharp_state_and_output_vectors() {
    let cases = [
        (0, [(0, 0), (0, 0), (0, 0), (0, 0), (0, 0)]),
        (
            1,
            [
                (1, 0),
                (33554433, 0),
                (1126174793148417, 17),
                (3659449627584515, 53803),
                (2306758490171379329, 0),
            ],
        ),
        (
            u64::MAX,
            [
                (u64::MAX, 0),
                (18442240611487580160, 0),
                (13835058914275621887, 79),
                (5758697399176404478, 43157),
                (11811533921607156160, 0),
            ],
        ),
        (
            0x123456789abcdef0,
            [
                (0x123456789abcdef0, 0),
                (7624347718526603213, 0),
                (4561998992951631716, 44),
                (265876972068855838, 21513),
                (8363960998174787318, 0),
            ],
        ),
    ];
    for (initial, expected) in cases {
        let mut rng = SimRng::new(initial);
        for (bound, (state, output)) in [0, 1, 100, 65535, 1].into_iter().zip(expected) {
            assert_eq!(rng.next(bound), output);
            assert_eq!(rng.state(), state);
        }
    }
}

#[test]
fn rng_tick_entity_mix_wraps_and_can_unstick_zero_seed() {
    let mut rng = SimRng::new(u64::MAX);
    rng.mix_entity_count(2);
    assert_eq!(rng.state(), 1);
    let mut zero = SimRng::new(0);
    assert_eq!(zero.next(100), 0);
    assert_eq!(zero.state(), 0);
    zero.mix_entity_count(4);
    zero.next(1);
    assert_ne!(zero.state(), 0);
}

#[test]
fn allocator_uses_smallest_free_id_and_rejects_stale_and_double_release() {
    let mut ids = IdAllocator::new();
    let first = ids.allocate().unwrap();
    let second = ids.allocate().unwrap();
    let third = ids.allocate().unwrap();
    assert_eq!(
        first,
        EntityRef {
            object_id: ObjectId(1),
            generation: 1
        }
    );
    assert_eq!(second.object_id, ObjectId(2));
    assert_eq!(third.object_id, ObjectId(3));
    ids.release(second).unwrap();
    ids.release(first).unwrap();
    let reused = ids.allocate().unwrap();
    assert_eq!(
        reused,
        EntityRef {
            object_id: ObjectId(1),
            generation: 2
        }
    );
    assert!(matches!(
        ids.validate(first),
        Err(IdError::StaleReference { .. })
    ));
    assert!(matches!(
        ids.release(first),
        Err(IdError::StaleReference { .. })
    ));
    assert_eq!(
        ids.resolve(ObjectId(2)),
        Err(IdError::NotAllocated(ObjectId(2)))
    );
    ids.release(reused).unwrap();
    assert_eq!(ids.release(reused), Err(IdError::AlreadyReleased(reused)));
    assert_eq!(ids.len(), 1);
    assert_eq!(ids.resolve(third.object_id), Ok(third));
}

#[test]
fn allocator_rejects_zero_negative_unknown_and_zero_generation_refs() {
    let mut ids = IdAllocator::new();
    assert!(ids.is_empty());
    for id in [ObjectId(0), ObjectId(-1), ObjectId(i16::MIN)] {
        let reference = EntityRef {
            object_id: id,
            generation: 1,
        };
        assert_eq!(ids.validate(reference), Err(IdError::InvalidObjectId(id)));
        assert_eq!(ids.release(reference), Err(IdError::InvalidObjectId(id)));
        assert_eq!(ids.resolve(id), Err(IdError::InvalidObjectId(id)));
    }
    assert_eq!(
        ids.resolve(ObjectId(1)),
        Err(IdError::UnknownObject(ObjectId(1)))
    );
    let live = ids.allocate().unwrap();
    assert_eq!(
        ids.validate(EntityRef {
            generation: 0,
            ..live
        }),
        Err(IdError::InvalidGeneration)
    );
    assert_eq!(ids.resolve(live.object_id), Ok(live));
}

#[test]
fn exhausted_allocator_never_allocates_zero_and_recovers_released_slot() {
    let mut ids = IdAllocator::new();
    let first = ids.allocate().unwrap();
    for expected in 2..=i16::MAX {
        assert_eq!(ids.allocate().unwrap().object_id, ObjectId(expected));
    }
    assert_eq!(ids.len(), i16::MAX as usize);
    let before = bincode::serialize(&ids).unwrap();
    assert_eq!(ids.allocate(), Err(IdError::Exhausted));
    assert_eq!(bincode::serialize(&ids).unwrap(), before);
    ids.release(first).unwrap();
    assert_eq!(
        ids.allocate().unwrap(),
        EntityRef {
            object_id: first.object_id,
            generation: 2
        }
    );
    assert_eq!(ids.allocate(), Err(IdError::Exhausted));
}

#[derive(Serialize)]
struct AllocatorWire {
    next_unused: i32,
    generations: BTreeMap<ObjectId, u32>,
    live: BTreeSet<ObjectId>,
    free: BTreeSet<ObjectId>,
}

fn one_slot_wire(generation: u32) -> AllocatorWire {
    AllocatorWire {
        next_unused: 2,
        generations: BTreeMap::from([(ObjectId(1), generation)]),
        live: BTreeSet::from([ObjectId(1)]),
        free: BTreeSet::new(),
    }
}

#[test]
fn generation_overflow_retires_slot_without_revalidating_old_reference() {
    let encoded = bincode::serialize(&one_slot_wire(u32::MAX)).unwrap();
    let mut ids: IdAllocator = bincode::deserialize(&encoded).unwrap();
    let last_generation = EntityRef {
        object_id: ObjectId(1),
        generation: u32::MAX,
    };
    ids.release(last_generation).unwrap();
    assert_eq!(ids.allocate().unwrap().object_id, ObjectId(2));
    assert_eq!(
        ids.validate(last_generation),
        Err(IdError::NotAllocated(ObjectId(1)))
    );
    ids.validate_state().unwrap();
}

#[test]
fn allocator_deserialization_rejects_inconsistent_state() {
    let mut invalid = Vec::new();
    let mut item = one_slot_wire(1);
    item.next_unused = 0;
    invalid.push(item);
    let mut item = one_slot_wire(1);
    item.next_unused = 3; // There is no generation record for previously allocated ID 2.
    invalid.push(item);
    invalid.push(one_slot_wire(0));
    let mut item = one_slot_wire(1);
    item.free.insert(ObjectId(1)); // Simultaneously live and free.
    invalid.push(item);
    let mut item = one_slot_wire(u32::MAX);
    item.live.clear();
    item.free.insert(ObjectId(1)); // An exhausted generation must be retired.
    invalid.push(item);
    let mut item = one_slot_wire(1);
    item.live.insert(ObjectId(0));
    invalid.push(item);
    for wire in invalid {
        assert!(bincode::deserialize::<IdAllocator>(&bincode::serialize(&wire).unwrap()).is_err());
    }
}

#[test]
fn numeric_identity_and_rng_state_round_trip_preserves_next_behavior() {
    let mut ids = IdAllocator::new();
    let first = ids.allocate().unwrap();
    ids.allocate().unwrap();
    ids.release(first).unwrap();
    let mut rng = SimRng::new(0x123456789abcdef0);
    rng.next(17);
    let original = (ids, rng, PersistentId(u32::MAX), ObjectId(-1));
    let mut restored: (IdAllocator, SimRng, PersistentId, ObjectId) =
        bincode::deserialize(&bincode::serialize(&original).unwrap()).unwrap();
    assert_eq!(original, restored);
    let mut continuous = original;
    assert_eq!(continuous.0.allocate(), restored.0.allocate());
    assert_eq!(continuous.1.next(65535), restored.1.next(65535));
    assert_eq!(continuous, restored);
}
