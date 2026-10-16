//! Narrow source-derived TSO metadata normalization. Explicit metadata imports
//! remain available for unsupported layouts; no appearance or startup is invented.
use crate::content::RuntimeMetadata;
use sim_core::{
    avatars::motives::TsoMotiveTuning,
    state::RoutingSlot,
    world::{slots::SlotSearch, Footprint, PlacementRules},
};
use wonderland_content_ir::manifest::Digest;
use wonderland_legacy_formats::{
    iff::{ChunkKey, IffFile},
    semantic::{decode_objd, decode_slot},
    tuning_dat, Limits,
};

/// VMContext chooses VMAvatar for Person (2), VMGameObject otherwise. This
/// bounded adapter supports single-tile definitions only. Placement flags start
/// at zero in VMEntity and must be established by the authentic Init behavior.
pub fn tso_object_metadata(
    file: &IffFile,
    object_chunk_id: u16,
    limits: &Limits,
) -> Result<RuntimeMetadata, String> {
    let chunk = file
        .chunks
        .iter()
        .find(|c| {
            c.key
                == ChunkKey {
                    kind: *b"OBJD",
                    id: object_chunk_id,
                }
        })
        .ok_or("missing source OBJD")?;
    let obj = decode_objd(&chunk.data, limits).map_err(|e| e.to_string())?;
    if obj.master_id() != 0 {
        return Err("source metadata adapter requires explicit multipart metadata".into());
    }
    let field = |name| {
        obj.field(name)
            .ok_or_else(|| format!("missing source OBJD field {name}"))
    };
    let is_avatar = field("ObjectType")? == 2;
    let footprint = if is_avatar {
        Footprint::rectangle(-3, -3, 3, 3)
    } else {
        let width = field("TileWidth")? / 2;
        let width = i16::try_from(if width == 0 { 8 } else { width })
            .map_err(|_| "source footprint width overflow")?;
        let mask = field("FootprintMask")?;
        Footprint::rectangle(
            -width + ((mask >> 12) & 15) as i16,
            -width + (mask & 15) as i16,
            width - ((mask >> 4) & 15) as i16,
            width - ((mask >> 8) & 15) as i16,
        )
    };
    if !footprint.valid() {
        return Err("source footprint unsupported by runtime bounds".into());
    }
    let placement_rules = PlacementRules {
        flags: 0,
        wall_flags: 0,
        allowed_heights: 0,
        weight: 0,
        level_offset: i8::try_from(field("LevelOffset")? as i16)
            .map_err(|_| "source level offset overflow")?,
        exclusive_wall: false,
        require_center: false,
        require_corner: false,
        disallow_corner: false,
        require_inside: false,
        require_outside: false,
        require_floor_hole: false,
        is_avatar,
        allow_person_intersection: false,
        disallow_person_intersection: false,
        zero_extent: false,
        ghost: false,
        ignored: Default::default(),
    };
    let mut routing_slots = Vec::new();
    if let Some(chunk) = file.chunks.iter().find(|c| {
        c.key
            == ChunkKey {
                kind: *b"SLOT",
                id: obj.slot_id(),
            }
    }) {
        let source = decode_slot(&chunk.data, limits).map_err(|e| e.to_string())?;
        for item in source.slots.iter().filter(|s| s.type_id == 3) {
            let mut offsets = [0; 3];
            for (out, bits) in offsets.iter_mut().zip(item.offset) {
                let value = f32::from_bits(bits.0);
                if !value.is_finite()
                    || value.fract() != 0.0
                    || !(-4096.0..=4096.0).contains(&value)
                {
                    return Err("source SLOT offset requires an unsupported fractional or out-of-range normalization".into());
                }
                *out = value as i32;
            }
            let [min_proximity, max_proximity, optimal_proximity] = item
                .effective_proximity(source.version)
                .map_err(|e| e.to_string())?;
            let flags = item.rsflags as u32;
            // The pinned VMSlotParser never reads Ground, MaxSize, I10,
            // Gradient or Height for type-3 searches. Average-object-location
            // is equivalent for the single-tile objects admitted above.
            let slot = RoutingSlot {
                search: SlotSearch {
                    min_proximity,
                    max_proximity,
                    optimal_proximity,
                    resolution: item.effective_resolution(),
                    directions: flags as u8,
                    offset_x: offsets[0],
                    offset_y: offsets[1],
                    offset_z: offsets[2],
                    absolute: flags & 512 != 0,
                    ignore_rooms: flags & 2048 != 0,
                    square: flags & (1 << 30) != 0,
                    equal_proximity_score: flags & (1 << 29) != 0,
                    random_scoring: flags & 8192 != 0,
                    standing: item.standing,
                    sitting: item.sitting,
                },
                facing: i8::try_from(item.effective_facing())
                    .map_err(|_| "source SLOT facing overflow")?,
                snap_to_direction: flags & 4096 != 0,
                snap_target_slot: if item.snap_target_slot < 0 {
                    None
                } else {
                    Some(
                        u16::try_from(item.snap_target_slot)
                            .map_err(|_| "source SLOT target overflow")?,
                    )
                },
            };
            slot.validate()?;
            routing_slots.push((
                u16::try_from(routing_slots.len()).map_err(|_| "source SLOT count overflow")?,
                slot,
            ));
        }
    } else if obj.slot_id() != 0 {
        return Err("source OBJD references missing SLOT".into());
    }
    Ok(RuntimeMetadata {
        footprint,
        placement_rules,
        master_guid: None,
        family: 0,
        routing_slots,
    })
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct TsoMotiveImport {
    pub source_sha256: Digest,
    pub tuning: TsoMotiveTuning,
}

/// VMAvatarMotiveDecay's float.Parse(InvariantCulture), float multiply by 1000,
/// then truncating integer conversion. Reject undefined overflow/nonfinite inputs.
pub fn tso_motive_tuning(bytes: &[u8], limits: &Limits) -> Result<TsoMotiveImport, String> {
    let entries = tuning_dat::decode(bytes, limits).map_err(|e| e.to_string())?;
    let tuning = motive_entries(&entries)?;
    Ok(TsoMotiveImport {
        source_sha256: Digest::of(bytes),
        tuning,
    })
}
fn motive_entries(entries: &tuning_dat::TuningEntries) -> Result<TsoMotiveTuning, String> {
    let read = |entry: &str, key: &str, fixed: bool| -> Result<i32, String> {
        let raw = entries
            .get(entry)
            .and_then(|e| e.get(key))
            .ok_or_else(|| format!("missing tuning.dat {entry}/{key}"))?;
        // InvariantCulture accepts thousands separators; source numbers are f32.
        let value = raw
            .trim()
            .replace(',', "")
            .parse::<f32>()
            .map_err(|_| format!("invalid tuning.dat number {entry}/{key}"))?;
        let value = if fixed { value * 1000.0_f32 } else { value };
        if !value.is_finite() || !(-2147483648.0..2147483648.0).contains(&f64::from(value)) {
            return Err(format!(
                "nonfinite or overflowing tuning.dat number {entry}/{key}"
            ));
        }
        Ok(value as i32)
    };
    let keys = [
        "HungerDecrementRatio",
        "ComfortDecrementActive",
        "HygieneDecrementAsleep",
        "HygieneDecrementAwake",
        "BladderDecrementAsleep",
        "BladderDecrementAwake",
        "HungerToBladderMultiplier",
        "EnergySpan",
        "WakeHours",
        "EntDecrementAwake",
        "SocialDecrementBase",
        "SocialDecrementMultiplier",
    ];
    let mut flat_sim = [0; 12];
    for (i, key) in keys.iter().enumerate() {
        flat_sim[i] = read("simmotives", key, i != 8)?;
    }
    let mut category_weights = [[0; 7]; 11];
    for (row, category) in category_weights.iter_mut().zip([
        "None",
        "Money",
        "Offbeat",
        "Romance",
        "Services",
        "Shopping",
        "Skills",
        "Welcome",
        "Games",
        "Entertain",
        "Residence",
    ]) {
        for (value, motive) in row.iter_mut().zip([
            "Hunger", "Comfort", "Hygiene", "Bladder", "Energy", "Fun", "Social",
        ]) {
            *value = read("lotmotives", &format!("{category}_{motive}Weight"), true)?;
        }
    }
    let tuning = TsoMotiveTuning {
        flat_sim,
        category_weights,
    };
    tuning
        .validate()
        .map_err(|e| format!("invalid source motive tuning: {e:?}"))?;
    Ok(tuning)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wonderland_legacy_formats::{
        iff::IffChunk,
        semantic::{encode_objd, Objd, OBJD_FIELDS},
    };
    #[test]
    fn slot_units_chronological_type_indices_and_fractional_rejection() {
        use wonderland_legacy_formats::{
            sprites::{encode_slot, SlotItem, SlotResource},
            vitaboy::F32Bits,
        };
        let mut obj = Objd {
            version: 142,
            fields: vec![0; 103],
            trailing: vec![],
        };
        obj.set_field("SlotID", 128).unwrap();
        let base = SlotItem {
            type_id: 3,
            offset: [F32Bits::from_f32(0.0); 3],
            standing: 1,
            sitting: 0,
            ground: 0,
            rsflags: 0x80ff,
            snap_target_slot: -123,
            min_proximity: Some(1),
            max_proximity: Some(2),
            optimal_proximity: Some(1),
            max_size: Some(0),
            i10: Some(0),
            gradient: Some(F32Bits::from_f32(0.5)),
            height: Some(0),
            facing: Some(-2),
            resolution: None,
        };
        let mut slot = SlotResource {
            reserved: 0,
            version: 9,
            magic: *b"TOLS",
            slots: vec![base.clone(), base],
            trailing: vec![],
        };
        slot.slots[0].type_id = 0;
        let file = |slot: &SlotResource| IffFile {
            header: [0; 64],
            chunks: vec![
                IffChunk {
                    key: ChunkKey {
                        kind: *b"OBJD",
                        id: 1,
                    },
                    flags: 0,
                    label: [0; 64],
                    data: encode_objd(&obj, &Limits::default()).unwrap(),
                },
                IffChunk {
                    key: ChunkKey {
                        kind: *b"SLOT",
                        id: 128,
                    },
                    flags: 0,
                    label: [0; 64],
                    data: encode_slot(slot, &Limits::default()).unwrap(),
                },
            ],
        };
        let runtime = tso_object_metadata(&file(&slot), 1, &Limits::default()).unwrap();
        assert_eq!(runtime.routing_slots.len(), 1);
        assert_eq!(runtime.routing_slots[0].0, 0);
        assert_eq!(runtime.routing_slots[0].1.search.min_proximity, 16);
        assert_eq!(runtime.routing_slots[0].1.snap_target_slot, None);
        slot.version = 100;
        for s in &mut slot.slots {
            s.resolution = Some(4);
        }
        let runtime = tso_object_metadata(&file(&slot), 1, &Limits::default()).unwrap();
        assert_eq!(runtime.routing_slots[0].1.search.min_proximity, 1);
        assert_eq!(runtime.routing_slots[0].1.search.resolution, 4);
        slot.slots[1].offset[0] = F32Bits::from_f32(0.5);
        assert!(tso_object_metadata(&file(&slot), 1, &Limits::default()).is_err());
    }
    #[test]
    fn footprint_uses_source_integer_half_width_mask_and_avatar_type() {
        let mut obj = Objd {
            version: 142,
            fields: vec![0; 103],
            trailing: vec![],
        };
        obj.set_field("TileWidth", 17).unwrap();
        obj.set_field("FootprintMask", 0x1234).unwrap();
        let file = |obj: &Objd| IffFile {
            header: [0; 64],
            chunks: vec![IffChunk {
                key: ChunkKey {
                    kind: *b"OBJD",
                    id: 1,
                },
                flags: 0,
                label: [0; 64],
                data: encode_objd(obj, &Limits::default()).unwrap(),
            }],
        };
        let runtime = tso_object_metadata(&file(&obj), 1, &Limits::default()).unwrap();
        assert_eq!(runtime.footprint, Footprint::rectangle(-7, -4, 5, 6));
        assert_eq!(runtime.placement_rules.flags, 0);
        assert_eq!(runtime.placement_rules.allowed_heights, 0);
        obj.set_field("ObjectType", 2).unwrap();
        assert_eq!(
            tso_object_metadata(&file(&obj), 1, &Limits::default())
                .unwrap()
                .footprint,
            Footprint::rectangle(-3, -3, 3, 3)
        );
        obj.set_field("MasterID", 5).unwrap();
        assert!(tso_object_metadata(&file(&obj), 1, &Limits::default()).is_err());
        assert!(OBJD_FIELDS.contains(&"FootprintMask"));
    }
    #[test]
    fn motives_require_source_keys_and_float32_truncation() {
        let mut entries = tuning_dat::TuningEntries::new();
        let keys = [
            "HungerDecrementRatio",
            "ComfortDecrementActive",
            "HygieneDecrementAsleep",
            "HygieneDecrementAwake",
            "BladderDecrementAsleep",
            "BladderDecrementAwake",
            "HungerToBladderMultiplier",
            "EnergySpan",
            "WakeHours",
            "EntDecrementAwake",
            "SocialDecrementBase",
            "SocialDecrementMultiplier",
        ];
        entries.insert(
            "simmotives".into(),
            keys.iter()
                .map(|k| (k.to_string(), "1.2349".into()))
                .collect(),
        );
        let mut lots = std::collections::BTreeMap::new();
        for c in [
            "None",
            "Money",
            "Offbeat",
            "Romance",
            "Services",
            "Shopping",
            "Skills",
            "Welcome",
            "Games",
            "Entertain",
            "Residence",
        ] {
            for m in [
                "Hunger", "Comfort", "Hygiene", "Bladder", "Energy", "Fun", "Social",
            ] {
                lots.insert(format!("{c}_{m}Weight"), "-0.1239".into());
            }
        }
        entries.insert("lotmotives".into(), lots);
        let tuning = motive_entries(&entries).unwrap();
        assert_eq!(tuning.flat_sim[0], 1234);
        assert_eq!(tuning.flat_sim[8], 1);
        assert_eq!(tuning.category_weights, [[-123; 7]; 11]);
        entries
            .get_mut("simmotives")
            .unwrap()
            .insert("WakeHours".into(), "NaN".into());
        assert!(motive_entries(&entries).is_err());
        entries.get_mut("simmotives").unwrap().remove("WakeHours");
        assert!(motive_entries(&entries).is_err());
    }
}
