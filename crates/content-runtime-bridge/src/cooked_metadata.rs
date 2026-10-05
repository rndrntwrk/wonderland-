// SPDX-License-Identifier: MPL-2.0
//! Explicit portable metadata for a cooked runtime binding.
//!
//! Every struct accepts a JSON object only, rejects unknown and duplicate fields,
//! and requires every field, including nullable fields. Map and set inputs use
//! entry arrays; conversion rejects duplicate identities instead of overwriting
//! them. The caller admits the complete binding's byte and allocation budgets
//! before deserialization/conversion. These DTOs additionally enforce the pinned
//! runtime's local metadata bounds and preserve resource-order event sequences.

use crate::{
    content::{ImportOptions, RuntimeMetadata},
    sim_core::{
        avatars::{events::TimeProperty, motives::TsoMotiveTuning, timeline::AnimationMetadata},
        ids::{EntityRef, ObjectId},
        state::{AnimationKey, RoutingSlot, TuningSet},
        world::{slots::SlotSearch, Footprint, FootprintRect, PlacementRules},
    },
};
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet},
    fmt,
    marker::PhantomData,
};
use wonderland_content_ir::strings::LocaleSelection;
use wonderland_legacy_formats::semantic::TtabVariant;

// Derived Deserialize accepts positional sequences for structs. Passing only a
// MapAccess to its field visitor preserves its duplicate/unknown-field checks
// without accepting sequences or materializing an intermediate JSON value.
fn object_only<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    struct ObjectVisitor<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a JSON object")
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            T::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(ObjectVisitor(PhantomData))
}

// Supplying deserialize_with prevents Serde's implicit missing-Option => None
// behavior. The value remains nullable, but the binding must name the field.
fn required_field<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    T::deserialize(deserializer)
}

macro_rules! strict_object {
    ($(#[$meta:meta])* pub struct $name:ident { $(pub $field:ident: $ty:ty),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
        pub struct $name {
            $(pub $field: $ty,)*
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields {
                    $(#[serde(deserialize_with = "required_field")] $field: $ty,)*
                }
                let Fields { $($field,)* } = object_only(deserializer)?;
                Ok(Self { $($field,)* })
            }
        }
    };
}

macro_rules! mirror_object {
    ($(#[$meta:meta])* $name:ident => $runtime:ident { $(pub $field:ident: $ty:ty),* $(,)? }) => {
        strict_object! {
            $(#[$meta])*
            pub struct $name { $(pub $field: $ty,)* }
        }

        impl From<$runtime> for $name {
            fn from(value: $runtime) -> Self {
                let $runtime { $($field,)* } = value;
                Self { $($field,)* }
            }
        }

        impl From<$name> for $runtime {
            fn from(value: $name) -> Self {
                let $name { $($field,)* } = value;
                Self { $($field,)* }
            }
        }
    };
}

strict_object! {
    /// All integration metadata required by one imported object.
    pub struct RuntimeMetadataV1 {
        pub footprint: FootprintV1,
        pub placement_rules: PlacementRulesV1,
        pub master_guid: Option<u32>,
        pub family: i16,
        pub routing_slots: Vec<RoutingSlotEntryV1>,
    }
}

strict_object! {
    pub struct FootprintV1 {
        pub rects: Vec<FootprintRectV1>,
    }
}

mirror_object! {
    FootprintRectV1 => FootprintRect {
        pub min_x: i16,
        pub min_y: i16,
        pub max_x: i16,
        pub max_y: i16,
    }
}

strict_object! {
    pub struct PlacementRulesV1 {
        pub flags: u16,
        pub wall_flags: u16,
        pub allowed_heights: u16,
        pub weight: i16,
        pub level_offset: i8,
        pub exclusive_wall: bool,
        pub require_center: bool,
        pub require_corner: bool,
        pub disallow_corner: bool,
        pub require_inside: bool,
        pub require_outside: bool,
        pub require_floor_hole: bool,
        pub is_avatar: bool,
        pub allow_person_intersection: bool,
        pub disallow_person_intersection: bool,
        pub zero_extent: bool,
        pub ghost: bool,
        pub ignored: Vec<EntityRefV1>,
    }
}

strict_object! {
    pub struct EntityRefV1 {
        pub object_id: i16,
        pub generation: u32,
    }
}

strict_object! {
    pub struct RoutingSlotEntryV1 {
        pub index: u16,
        pub slot: RoutingSlotV1,
    }
}

strict_object! {
    pub struct RoutingSlotV1 {
        pub search: SlotSearchV1,
        pub facing: i8,
        pub snap_to_direction: bool,
        pub snap_target_slot: Option<u16>,
    }
}

mirror_object! {
    SlotSearchV1 => SlotSearch {
        pub min_proximity: i32,
        pub max_proximity: i32,
        pub optimal_proximity: i32,
        pub resolution: i32,
        pub directions: u8,
        pub offset_x: i32,
        pub offset_y: i32,
        pub offset_z: i32,
        pub absolute: bool,
        pub ignore_rooms: bool,
        pub square: bool,
        pub equal_proximity_score: bool,
        pub random_scoring: bool,
        pub standing: i32,
        pub sitting: i32,
    }
}

strict_object! {
    /// Shared explicit conversion options, including non-BCON runtime tuning.
    pub struct ImportOptionsV1 {
        pub locale: LocaleSelectionV1,
        pub ttab_variant: TtabVariantV1,
        pub runtime_tuning: TuningSetV1,
        pub animations: Vec<AnimationEntryV1>,
    }
}

mirror_object! {
    LocaleSelectionV1 => LocaleSelection {
        pub requested: u8,
        pub default_language: u8,
    }
}

/// An explicit codec choice. JSON accepts only the strings `standard` or `tsbo`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TtabVariantV1 {
    Standard,
    Tsbo,
}

impl<'de> Deserialize<'de> for TtabVariantV1 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct VariantVisitor;
        impl<'de> Visitor<'de> for VariantVisitor {
            type Value = TtabVariantV1;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("the string standard or tsbo")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "standard" => Ok(TtabVariantV1::Standard),
                    "tsbo" => Ok(TtabVariantV1::Tsbo),
                    _ => Err(E::unknown_variant(value, &["standard", "tsbo"])),
                }
            }
        }
        deserializer.deserialize_str(VariantVisitor)
    }
}

impl From<TtabVariant> for TtabVariantV1 {
    fn from(value: TtabVariant) -> Self {
        match value {
            TtabVariant::Standard => Self::Standard,
            TtabVariant::Tsbo => Self::Tsbo,
        }
    }
}

impl From<TtabVariantV1> for TtabVariant {
    fn from(value: TtabVariantV1) -> Self {
        match value {
            TtabVariantV1::Standard => Self::Standard,
            TtabVariantV1::Tsbo => Self::Tsbo,
        }
    }
}

strict_object! {
    pub struct TuningSetV1 {
        pub values: Vec<TuningValueV1>,
        pub tso_motives: Option<TsoMotiveTuningV1>,
        pub motive_limits: [i16; 16],
        pub relationship_multipliers: Vec<RelationshipMultiplierV1>,
        pub fire_enabled: bool,
    }
}

strict_object! {
    pub struct TuningValueV1 {
        pub owner: u32,
        pub table: u16,
        pub index: u16,
        pub value: i16,
    }
}

strict_object! {
    /// Finite IEEE-754 binary32 bits, preserving signed zero exactly.
    pub struct RelationshipMultiplierV1 {
        pub key: u8,
        pub value_bits: u32,
    }
}

mirror_object! {
    TsoMotiveTuningV1 => TsoMotiveTuning {
        pub flat_sim: [i32; 12],
        pub category_weights: [[i32; 7]; 11],
    }
}

strict_object! {
    pub struct AnimationEntryV1 {
        pub key: AnimationKeyV1,
        pub metadata: AnimationMetadataV1,
    }
}

mirror_object! {
    AnimationKeyV1 => AnimationKey {
        pub owner: u32,
        pub scope: u32,
        pub id: u16,
    }
}

strict_object! {
    pub struct AnimationMetadataV1 {
        pub resource: String,
        pub num_frames: i32,
        pub time_properties: Vec<TimePropertyV1>,
    }
}

strict_object! {
    /// The array order is the source event order; it is never sorted by time.
    pub struct TimePropertyV1 {
        pub time_ms: u32,
        pub properties: Vec<PropertyV1>,
    }
}

strict_object! {
    pub struct PropertyV1 {
        pub key: String,
        pub value: String,
    }
}

fn insert_unique<K: Ord, V>(
    map: &mut BTreeMap<K, V>,
    key: K,
    value: V,
    description: &str,
) -> Result<(), String> {
    match map.entry(key) {
        Entry::Vacant(entry) => {
            entry.insert(value);
            Ok(())
        }
        Entry::Occupied(_) => Err(format!("duplicate {description}")),
    }
}

impl RuntimeMetadataV1 {
    pub fn into_runtime(self) -> Result<RuntimeMetadata, String> {
        if self.footprint.rects.len() > 64 {
            return Err("runtime footprint rectangle count limit".into());
        }
        if self.routing_slots.len() > 65_536 {
            return Err("runtime routing slot count limit".into());
        }
        let footprint = Footprint {
            rects: self.footprint.rects.into_iter().map(Into::into).collect(),
        };
        if !footprint.valid() {
            return Err("invalid runtime footprint".into());
        }
        let placement_rules = self.placement_rules.into_runtime()?;
        let mut seen = BTreeSet::new();
        let mut routing_slots = Vec::with_capacity(self.routing_slots.len());
        for entry in self.routing_slots {
            if !seen.insert(entry.index) {
                return Err("duplicate routing slot index".into());
            }
            let slot = RoutingSlot {
                search: entry.slot.search.into(),
                facing: entry.slot.facing,
                snap_to_direction: entry.slot.snap_to_direction,
                snap_target_slot: entry.slot.snap_target_slot,
            };
            slot.validate()?;
            routing_slots.push((entry.index, slot));
        }
        Ok(RuntimeMetadata {
            footprint,
            placement_rules,
            master_guid: self.master_guid,
            family: self.family,
            routing_slots,
        })
    }
}

impl From<RuntimeMetadata> for RuntimeMetadataV1 {
    fn from(value: RuntimeMetadata) -> Self {
        let RuntimeMetadata {
            footprint,
            placement_rules,
            master_guid,
            family,
            routing_slots,
        } = value;
        Self {
            footprint: FootprintV1 {
                rects: footprint.rects.into_iter().map(Into::into).collect(),
            },
            placement_rules: placement_rules.into(),
            master_guid,
            family,
            routing_slots: routing_slots
                .into_iter()
                .map(|(index, slot)| RoutingSlotEntryV1 {
                    index,
                    slot: RoutingSlotV1 {
                        search: slot.search.into(),
                        facing: slot.facing,
                        snap_to_direction: slot.snap_to_direction,
                        snap_target_slot: slot.snap_target_slot,
                    },
                })
                .collect(),
        }
    }
}

impl PlacementRulesV1 {
    fn into_runtime(self) -> Result<PlacementRules, String> {
        if self.ignored.len() > 32_768 {
            return Err("placement ignored-reference count limit".into());
        }
        let mut ignored = BTreeSet::new();
        for entry in self.ignored {
            let entity = EntityRef {
                object_id: ObjectId(entry.object_id),
                generation: entry.generation,
            };
            if !entity.is_valid() {
                return Err("invalid placement ignored entity reference".into());
            }
            if !ignored.insert(entity) {
                return Err("duplicate placement ignored entity reference".into());
            }
        }
        let rules = PlacementRules {
            flags: self.flags,
            wall_flags: self.wall_flags,
            allowed_heights: self.allowed_heights,
            weight: self.weight,
            level_offset: self.level_offset,
            exclusive_wall: self.exclusive_wall,
            require_center: self.require_center,
            require_corner: self.require_corner,
            disallow_corner: self.disallow_corner,
            require_inside: self.require_inside,
            require_outside: self.require_outside,
            require_floor_hole: self.require_floor_hole,
            is_avatar: self.is_avatar,
            allow_person_intersection: self.allow_person_intersection,
            disallow_person_intersection: self.disallow_person_intersection,
            zero_extent: self.zero_extent,
            ghost: self.ghost,
            ignored,
        };
        if !rules.valid() {
            return Err("invalid runtime placement rules".into());
        }
        Ok(rules)
    }
}

impl From<PlacementRules> for PlacementRulesV1 {
    fn from(value: PlacementRules) -> Self {
        let PlacementRules {
            flags,
            wall_flags,
            allowed_heights,
            weight,
            level_offset,
            exclusive_wall,
            require_center,
            require_corner,
            disallow_corner,
            require_inside,
            require_outside,
            require_floor_hole,
            is_avatar,
            allow_person_intersection,
            disallow_person_intersection,
            zero_extent,
            ghost,
            ignored,
        } = value;
        Self {
            flags,
            wall_flags,
            allowed_heights,
            weight,
            level_offset,
            exclusive_wall,
            require_center,
            require_corner,
            disallow_corner,
            require_inside,
            require_outside,
            require_floor_hole,
            is_avatar,
            allow_person_intersection,
            disallow_person_intersection,
            zero_extent,
            ghost,
            ignored: ignored
                .into_iter()
                .map(|entry| EntityRefV1 {
                    object_id: entry.object_id.0,
                    generation: entry.generation,
                })
                .collect(),
        }
    }
}

impl ImportOptionsV1 {
    pub fn into_runtime(self) -> Result<ImportOptions, String> {
        if self.animations.len() > 65_536 {
            return Err("runtime animation count limit".into());
        }
        let runtime_tuning = self.runtime_tuning.into_runtime()?;
        let mut seen = BTreeSet::new();
        let mut animations = Vec::with_capacity(self.animations.len());
        for entry in self.animations {
            let key: AnimationKey = entry.key.into();
            if !seen.insert(key) {
                return Err("duplicate animation key".into());
            }
            animations.push((key, entry.metadata.into_runtime()?));
        }
        Ok(ImportOptions {
            locale: self.locale.into(),
            ttab_variant: self.ttab_variant.into(),
            runtime_tuning,
            animations,
        })
    }
}

impl From<ImportOptions> for ImportOptionsV1 {
    fn from(value: ImportOptions) -> Self {
        let ImportOptions {
            locale,
            ttab_variant,
            runtime_tuning,
            animations,
        } = value;
        Self {
            locale: locale.into(),
            ttab_variant: ttab_variant.into(),
            runtime_tuning: runtime_tuning.into(),
            animations: animations
                .into_iter()
                .map(|(key, metadata)| AnimationEntryV1 {
                    key: key.into(),
                    metadata: metadata.into(),
                })
                .collect(),
        }
    }
}

impl TuningSetV1 {
    fn into_runtime(self) -> Result<TuningSet, String> {
        if self.values.len() > 65_536 || self.relationship_multipliers.len() > 256 {
            return Err("runtime tuning entry count limit".into());
        }
        let mut values = BTreeMap::new();
        for entry in self.values {
            insert_unique(
                &mut values,
                (entry.owner, entry.table, entry.index),
                entry.value,
                "runtime tuning key",
            )?;
        }
        let mut relationship_multipliers = BTreeMap::new();
        for entry in self.relationship_multipliers {
            let value = f32::from_bits(entry.value_bits);
            if !value.is_finite() {
                return Err("relationship multiplier must be finite".into());
            }
            insert_unique(
                &mut relationship_multipliers,
                entry.key,
                value,
                "relationship multiplier key",
            )?;
        }
        let tso_motives: Option<TsoMotiveTuning> = self.tso_motives.map(Into::into);
        if let Some(tuning) = &tso_motives {
            tuning
                .validate()
                .map_err(|error| format!("TSO motive tuning: {error:?}"))?;
        }
        Ok(TuningSet {
            values,
            tso_motives,
            motive_limits: self.motive_limits,
            relationship_multipliers,
            fire_enabled: self.fire_enabled,
        })
    }
}

impl From<TuningSet> for TuningSetV1 {
    fn from(value: TuningSet) -> Self {
        let TuningSet {
            values,
            tso_motives,
            motive_limits,
            relationship_multipliers,
            fire_enabled,
        } = value;
        Self {
            values: values
                .into_iter()
                .map(|((owner, table, index), value)| TuningValueV1 {
                    owner,
                    table,
                    index,
                    value,
                })
                .collect(),
            tso_motives: tso_motives.map(Into::into),
            motive_limits,
            relationship_multipliers: relationship_multipliers
                .into_iter()
                .map(|(key, value)| RelationshipMultiplierV1 {
                    key,
                    value_bits: value.to_bits(),
                })
                .collect(),
            fire_enabled,
        }
    }
}

impl AnimationMetadataV1 {
    fn into_runtime(self) -> Result<AnimationMetadata, String> {
        if self.resource.is_empty()
            || self.resource.len() > 1024
            || !(0..=1_000_000).contains(&self.num_frames)
            || self.time_properties.len() > 65_536
        {
            return Err("runtime animation metadata bounds".into());
        }
        let mut time_properties = Vec::with_capacity(self.time_properties.len());
        for property in self.time_properties {
            if property.properties.len() > 256 {
                return Err("animation time-property entry count limit".into());
            }
            let mut properties = BTreeMap::new();
            for entry in property.properties {
                if entry.key.len() > 1024 || entry.value.len() > 16_384 {
                    return Err("animation time-property text bounds".into());
                }
                insert_unique(
                    &mut properties,
                    entry.key,
                    entry.value,
                    "animation time-property key",
                )?;
            }
            time_properties.push(TimeProperty {
                time_ms: property.time_ms,
                properties,
            });
        }
        let metadata = AnimationMetadata {
            resource: self.resource,
            num_frames: self.num_frames,
            time_properties,
        };
        metadata
            .validate()
            .map_err(|error| format!("animation metadata: {error:?}"))?;
        Ok(metadata)
    }
}

impl From<AnimationMetadata> for AnimationMetadataV1 {
    fn from(value: AnimationMetadata) -> Self {
        let AnimationMetadata {
            resource,
            num_frames,
            time_properties,
        } = value;
        Self {
            resource,
            num_frames,
            time_properties: time_properties
                .into_iter()
                .map(|property| TimePropertyV1 {
                    time_ms: property.time_ms,
                    properties: property
                        .properties
                        .into_iter()
                        .map(|(key, value)| PropertyV1 { key, value })
                        .collect(),
                })
                .collect(),
        }
    }
}
