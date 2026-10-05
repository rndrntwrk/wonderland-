//! Conservative admission for converted/retained content, not an exact RSS meter.
//! The runtime separately enforces its final canonical serialized-content cap.
//! Legacy text is decoded directly into its admitted fixed-capacity buffer.
use crate::content::RuntimeMetadata;
use sim_core::{
    avatars::{events::TimeProperty, timeline::AnimationMetadata},
    ids::EntityRef,
    state::{AnimationKey, RoutingSlot, TuningSet, MAX_CONTENT_BYTES},
    world::FootprintRect,
};
use std::mem::size_of;
use wonderland_legacy_formats::{
    semantic::{
        LegacyString, StringItem, StringOrder, Strings, TextEncoding, Ttab, TtabInteraction,
        TtabMotive,
    },
    Limits,
};

pub(crate) struct ImportBudget {
    limit: usize,
    used: usize,
}

impl ImportBudget {
    pub(crate) fn new(limits: &Limits) -> Self {
        Self {
            limit: limits
                .max_total_decoded_bytes
                .min(usize::try_from(MAX_CONTENT_BYTES).unwrap_or(usize::MAX)),
            used: 0,
        }
    }

    pub(crate) fn reserve(&mut self, bytes: usize) -> Result<(), String> {
        let next = self
            .used
            .checked_add(bytes)
            .filter(|next| *next <= self.limit)
            .ok_or("content import allocation budget exceeded")?;
        self.used = next;
        Ok(())
    }

    pub(crate) fn entries<T>(&mut self, count: usize) -> Result<(), String> {
        let bytes = count
            .checked_mul(size_of::<T>())
            .ok_or("content import allocation budget overflow")?;
        self.reserve(bytes)
    }

    pub(crate) fn map_entries<K, V>(&mut self, count: usize) -> Result<(), String> {
        // Conservatively include tree nodes and temporary collection overhead.
        let bytes = size_of::<K>()
            .checked_add(size_of::<V>())
            .and_then(|entry| entry.checked_add(128))
            .and_then(|entry| entry.checked_mul(count))
            .ok_or("content import map allocation budget overflow")?;
        self.reserve(bytes)
    }

    pub(crate) fn text(&mut self, raw: &LegacyString) -> Result<(), String> {
        // Each invalid UTF-8 byte can become a three-byte replacement character.
        // Latin-1 and ASCII conversions fit within the same conservative bound.
        let bytes = raw
            .bytes
            .len()
            .checked_mul(3)
            .and_then(|bytes| bytes.checked_add(size_of::<String>()))
            .ok_or("content import text allocation budget overflow")?;
        self.reserve(bytes)
    }

    pub(crate) fn decode_text(&mut self, raw: &LegacyString) -> Result<String, String> {
        let capacity = raw
            .bytes
            .len()
            .checked_mul(3)
            .ok_or("content import text allocation budget overflow")?;
        self.text(raw)?;
        // The general legacy conversion can grow geometrically. Allocate the
        // admitted upper bound once, then write directly without an intermediate
        // owned lossy string or any capacity growth.
        let mut decoded = String::with_capacity(capacity);
        match raw.encoding {
            TextEncoding::Ascii => {
                for &byte in &raw.bytes {
                    decoded.push(if byte < 128 { char::from(byte) } else { '?' });
                }
            }
            TextEncoding::Latin1 => {
                for &byte in &raw.bytes {
                    decoded.push(char::from(byte));
                }
            }
            TextEncoding::Utf8 => {
                let mut remaining = raw.bytes.as_slice();
                while !remaining.is_empty() {
                    match std::str::from_utf8(remaining) {
                        Ok(valid) => {
                            decoded.push_str(valid);
                            break;
                        }
                        Err(error) => {
                            let (valid, invalid) = remaining.split_at(error.valid_up_to());
                            decoded.push_str(
                                std::str::from_utf8(valid)
                                    .expect("Utf8Error::valid_up_to identifies a valid prefix"),
                            );
                            decoded.push('\u{fffd}');
                            let Some(skip) = error.error_len() else {
                                break;
                            };
                            remaining = &invalid[skip..];
                        }
                    }
                }
            }
        }
        Ok(decoded)
    }

    /// Charge retained decoded storage before allocating converted output or
    /// starting another decoder. Spare capacity remains allocated and counts.
    pub(crate) fn decoded_strings(&mut self, strings: &Strings) -> Result<(), String> {
        self.entries::<Strings>(1)?;
        self.entries::<Vec<StringItem>>(strings.sets.capacity())?;
        for set in strings
            .sets
            .iter()
            .chain(std::iter::once(&strings.unassigned))
        {
            self.entries::<StringItem>(set.capacity())?;
            for item in set {
                self.reserve(item.value.bytes.capacity())?;
                self.reserve(item.comment.bytes.capacity())?;
            }
        }
        self.entries::<StringOrder>(strings.order.capacity())?;
        self.reserve(strings.trailing.capacity())
    }

    /// `source_bytes` is the length passed to the TTAB decoder. That decoder
    /// retains a private `bytes.to_vec()` copy; charge its entire envelope even
    /// when an authored table has no original. Other buffers use actual capacity.
    pub(crate) fn decoded_ttab(&mut self, table: &Ttab, source_bytes: usize) -> Result<(), String> {
        self.entries::<Ttab>(1)?;
        self.reserve(source_bytes)?;
        self.entries::<TtabInteraction>(table.interactions.capacity())?;
        for interaction in &table.interactions {
            self.entries::<TtabMotive>(interaction.motives.capacity())?;
        }
        self.reserve(table.trailing.capacity())
    }

    pub(crate) fn limits(&self, limits: &Limits) -> Limits {
        Limits {
            max_total_decoded_bytes: limits.max_total_decoded_bytes.min(self.limit - self.used),
            ..*limits
        }
    }

    pub(crate) fn animation(&mut self, animation: &AnimationMetadata) -> Result<(), String> {
        animation
            .validate()
            .map_err(|error| format!("animation: {error:?}"))?;
        self.map_entries::<AnimationKey, AnimationMetadata>(1)?;
        // Animation metadata is moved into ContentSet, retaining spare capacity.
        self.reserve(animation.resource.capacity())?;
        self.entries::<TimeProperty>(animation.time_properties.capacity())?;
        for property in &animation.time_properties {
            self.map_entries::<String, String>(property.properties.len())?;
            for (key, value) in &property.properties {
                self.reserve(key.capacity())?;
                self.reserve(value.capacity())?;
            }
        }
        Ok(())
    }

    pub(crate) fn tuning(&mut self, tuning: &TuningSet) -> Result<(), String> {
        if tuning.values.len() > 65_536
            || tuning
                .relationship_multipliers
                .values()
                .any(|value| !value.is_finite())
        {
            return Err("invalid runtime tuning bounds".into());
        }
        if let Some(motives) = &tuning.tso_motives {
            motives
                .validate()
                .map_err(|error| format!("tuning: {error:?}"))?;
        }
        // TsoMotiveTuning contains only fixed arrays, already included here.
        self.entries::<TuningSet>(1)?;
        self.map_entries::<(u32, u16, u16), i16>(tuning.values.len())?;
        self.map_entries::<u8, f32>(tuning.relationship_multipliers.len())
    }

    pub(crate) fn metadata(&mut self, metadata: &RuntimeMetadata) -> Result<(), String> {
        if !metadata.footprint.valid() || !metadata.placement_rules.valid() {
            return Err("invalid runtime metadata geometry or placement rules".into());
        }
        if metadata.routing_slots.len() > 65_536 {
            return Err("runtime metadata routing slot count limit".into());
        }
        for (_, slot) in &metadata.routing_slots {
            slot.validate()?;
        }
        self.entries::<RuntimeMetadata>(1)?;
        self.entries::<FootprintRect>(metadata.footprint.rects.len())?;
        self.map_entries::<EntityRef, ()>(metadata.placement_rules.ignored.len())?;
        self.map_entries::<(u32, u16), RoutingSlot>(metadata.routing_slots.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::{
        avatars::events::TimeProperty,
        ids::ObjectId,
        world::{Footprint, PlacementRules},
    };
    use std::collections::BTreeMap;
    use wonderland_legacy_formats::semantic::{decode_strings, decode_ttab};

    fn budget(bytes: usize) -> ImportBudget {
        ImportBudget::new(&Limits {
            max_total_decoded_bytes: bytes,
            ..Limits::default()
        })
    }

    #[test]
    fn cumulative_admission_uses_the_smaller_configured_or_runtime_limit() {
        let mut configured = budget(8);
        configured.reserve(6).unwrap();
        assert!(configured.reserve(3).is_err());
        configured.reserve(2).unwrap();
        assert!(configured.reserve(1).is_err());

        let mut runtime = ImportBudget::new(&Limits::default());
        runtime.reserve(32 * 1024 * 1024).unwrap();
        assert!(runtime.reserve(1).is_err());
    }

    #[test]
    fn overflowing_admissions_fail_without_consuming_the_remaining_budget() {
        let mut remaining = budget(16);
        remaining.reserve(8).unwrap();
        assert!(remaining.reserve(usize::MAX).is_err());
        assert!(remaining.entries::<u64>(usize::MAX / 8 + 1).is_err());
        assert!(remaining
            .map_entries::<u64, u64>(usize::MAX / 144 + 1)
            .is_err());
        remaining.reserve(8).unwrap();
        assert_eq!(
            remaining.limits(&Limits::default()).max_total_decoded_bytes,
            0
        );
    }

    #[test]
    fn map_admission_includes_node_storage() {
        let mut remaining = budget(128);
        assert!(remaining.map_entries::<u8, u8>(1).is_err());
        remaining.entries::<u8>(128).unwrap();
    }

    #[test]
    fn legacy_text_reserves_worst_case_replacement_bytes_before_conversion() {
        let raw = LegacyString {
            bytes: vec![0xff; 64],
            encoding: TextEncoding::Utf8,
        };
        let mut remaining = budget(191);
        assert!(remaining.text(&raw).is_err());
        remaining.reserve(191).unwrap();
        budget(256).text(&raw).unwrap();
        assert_eq!(raw.text().len(), 192);
    }

    #[test]
    fn decoded_legacy_string_capacity_fits_its_admitted_allocation() {
        let raw = LegacyString {
            bytes: vec![0xff; 64],
            encoding: TextEncoding::Utf8,
        };
        let mut remaining = budget(192 + size_of::<String>());
        let text = remaining.decode_text(&raw).unwrap();
        assert_eq!(text.len(), 192);
        assert!(
            text.capacity() <= 192,
            "retained capacity {} exceeds admitted 192 bytes",
            text.capacity()
        );
        assert_eq!(
            remaining.limits(&Limits::default()).max_total_decoded_bytes,
            0
        );
    }

    #[test]
    fn fixed_capacity_decoding_matches_legacy_text_for_every_encoding() {
        let cases: &[(TextEncoding, &[u8])] = &[
            (TextEncoding::Utf8, b""),
            (TextEncoding::Utf8, b"plain ascii"),
            (TextEncoding::Utf8, b"left\0right"),
            (TextEncoding::Utf8, "\u{e9}\u{1d11e}\u{1f49a}".as_bytes()),
            (TextEncoding::Utf8, b"A\xf0\x9f\x92\xa9B\xffC\xe2\x82"),
            (TextEncoding::Utf8, b"\xc0\xaf"),
            (TextEncoding::Utf8, b"\xed\xa0\x80"),
            (TextEncoding::Utf8, b"\xf0\x28\x8c\x28"),
            (TextEncoding::Utf8, b"\xe2\x82A"),
            (TextEncoding::Utf8, b"\xf0\x9f\x92"),
            (TextEncoding::Latin1, b"\xff"),
            (TextEncoding::Latin1, b"A\x80\xff"),
            (TextEncoding::Ascii, b"A"),
            (TextEncoding::Ascii, b"A\x80\xff"),
        ];
        let mut remaining = budget(4096);
        let mut retained = 0;
        for (encoding, bytes) in cases {
            let raw = LegacyString {
                bytes: bytes.to_vec(),
                encoding: *encoding,
            };
            let text = remaining.decode_text(&raw).unwrap();
            assert_eq!(text, raw.text(), "{encoding:?}: {bytes:?}");
            assert!(
                text.capacity() <= 3 * bytes.len(),
                "{encoding:?}: {bytes:?}"
            );
            retained += text.capacity() + size_of::<String>();
        }
        let admitted = 4096 - remaining.limits(&Limits::default()).max_total_decoded_bytes;
        assert!(retained <= admitted);
    }

    #[test]
    fn fixed_capacity_decoding_rejects_an_insufficient_budget() {
        let raw = LegacyString {
            bytes: vec![0xff; 64],
            encoding: TextEncoding::Utf8,
        };
        let mut remaining = budget(191);
        assert!(remaining.decode_text(&raw).unwrap_err().contains("budget"));
        remaining.reserve(191).unwrap();
    }

    fn strings_source() -> Vec<u8> {
        // STR -1, one UTF-8 NUL-terminated item, 64 invalid bytes.
        let mut bytes = vec![0xff, 0xff, 1, 0];
        bytes.extend_from_slice(&[0xff; 64]);
        bytes.push(0);
        bytes
    }

    fn ttab_source() -> Vec<u8> {
        // Uncompressed version 8, one interaction, eight motives, preserved tail.
        let mut bytes = vec![1, 0, 8, 0];
        bytes.extend_from_slice(&4110u16.to_le_bytes());
        bytes.extend_from_slice(&4110u16.to_le_bytes());
        for value in [8u32, 1, 0, 0, 0, 0, 0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[0; 48]);
        bytes.extend_from_slice(&[0; 32]);
        bytes
    }

    fn overlap_limits() -> Limits {
        // Hand-sized for both supported word widths: 768 bytes on native 64-bit,
        // 512 on WASM32. Each decoded fixture and its next stage fits separately.
        Limits {
            max_total_decoded_bytes: 256 + 64 * size_of::<usize>(),
            ..Limits::default()
        }
    }

    #[test]
    fn decoded_strings_remain_charged_while_output_text_is_allocated() {
        let limits = overlap_limits();
        let strings = decode_strings(&strings_source(), &limits).unwrap();
        let raw = &strings.sets[0][0].value;
        ImportBudget::new(&limits).decode_text(raw).unwrap();
        let mut remaining = ImportBudget::new(&limits);
        remaining.decoded_strings(&strings).unwrap();
        assert!(remaining.decode_text(raw).unwrap_err().contains("budget"));
    }

    #[test]
    fn decoded_ttab_remains_charged_when_the_next_string_table_is_decoded() {
        let limits = overlap_limits();
        let source = ttab_source();
        let table = decode_ttab(&source, &limits).unwrap();
        let next_source = strings_source();
        decode_strings(&next_source, &limits).unwrap();
        let mut remaining = ImportBudget::new(&limits);
        remaining.decoded_ttab(&table, source.len()).unwrap();
        assert!(decode_strings(&next_source, &remaining.limits(&limits)).is_err());
    }

    #[test]
    fn decoded_string_storage_counts_spare_capacity_in_every_container() {
        let grow: [fn(&mut Strings); 7] = [
            |value| value.sets.reserve_exact(512),
            |value| value.sets[0].reserve_exact(512),
            |value| value.sets[0][0].value.bytes.reserve_exact(4096),
            |value| value.sets[0][0].comment.bytes.reserve_exact(4096),
            |value| value.unassigned.reserve_exact(512),
            |value| value.order.reserve_exact(512),
            |value| value.trailing.reserve_exact(4096),
        ];
        for (index, grow) in grow.into_iter().enumerate() {
            let mut value = decode_strings(&strings_source(), &Limits::default()).unwrap();
            budget(4096).decoded_strings(&value).unwrap();
            grow(&mut value);
            assert!(
                budget(4096).decoded_strings(&value).is_err(),
                "container {index} capacity was omitted"
            );
        }
    }

    #[test]
    fn decoded_ttab_storage_counts_spare_capacity_in_every_public_container() {
        let grow: [fn(&mut Ttab); 3] = [
            |value| value.interactions.reserve_exact(256),
            |value| value.interactions[0].motives.reserve_exact(1024),
            |value| value.trailing.reserve_exact(4096),
        ];
        let source = ttab_source();
        for (index, grow) in grow.into_iter().enumerate() {
            let mut value = decode_ttab(&source, &Limits::default()).unwrap();
            budget(4096).decoded_ttab(&value, source.len()).unwrap();
            grow(&mut value);
            assert!(
                budget(4096).decoded_ttab(&value, source.len()).is_err(),
                "container {index} capacity was omitted"
            );
        }
    }

    #[test]
    fn decoded_ttab_charges_the_preserved_original_source_buffer() {
        let source = vec![0; 1002]; // Empty TTAB with 1000 preserved trailing bytes.
        let table = decode_ttab(&source, &Limits::default()).unwrap();
        assert!(budget(1500).decoded_ttab(&table, source.len()).is_err());
    }

    #[test]
    fn nested_animation_properties_and_runtime_tuning_share_the_budget() {
        let animation = AnimationMetadata {
            resource: "fixture.anim".into(),
            num_frames: 1,
            time_properties: vec![TimeProperty {
                time_ms: 0,
                properties: BTreeMap::from([("sound".into(), "x".repeat(1024))]),
            }],
        };
        assert!(budget(512)
            .animation(&animation)
            .unwrap_err()
            .contains("budget"));
        let mut tuning = TuningSet::default();
        for index in 0..100 {
            tuning.values.insert((1, 4096, index), 0);
        }
        assert!(budget(4096).tuning(&tuning).unwrap_err().contains("budget"));
    }

    #[test]
    fn moved_animation_storage_counts_existing_spare_capacity() {
        let grow: [fn(&mut AnimationMetadata); 4] = [
            |value| value.resource.reserve_exact(4096),
            |value| value.time_properties.reserve_exact(512),
            |value| {
                let properties = &mut value.time_properties[0].properties;
                let (mut key, text) = properties.remove_entry("sound").unwrap();
                key.reserve_exact(4096);
                properties.insert(key, text);
            },
            |value| {
                value.time_properties[0]
                    .properties
                    .get_mut("sound")
                    .unwrap()
                    .reserve_exact(4096)
            },
        ];
        for (index, grow) in grow.into_iter().enumerate() {
            let mut value = AnimationMetadata {
                resource: "fixture.anim".into(),
                num_frames: 1,
                time_properties: vec![TimeProperty {
                    time_ms: 0,
                    properties: BTreeMap::from([("sound".into(), "x".into())]),
                }],
            };
            budget(4096).animation(&value).unwrap();
            grow(&mut value);
            assert!(
                budget(4096).animation(&value).is_err(),
                "animation container {index} capacity was omitted"
            );
        }
    }

    #[test]
    fn metadata_counts_placement_sets_and_validates_geometry_first() {
        let mut metadata = RuntimeMetadata {
            footprint: Footprint::default(),
            placement_rules: PlacementRules::default(),
            master_guid: None,
            family: 0,
            routing_slots: vec![],
        };
        budget(512).metadata(&metadata).unwrap();
        for id in 1..=100 {
            metadata.placement_rules.ignored.insert(EntityRef {
                object_id: ObjectId(id),
                generation: 1,
            });
        }
        assert!(budget(4096)
            .metadata(&metadata)
            .unwrap_err()
            .contains("budget"));
        metadata.footprint = Footprint::rectangle(4, 4, -4, -4);
        assert!(budget(0)
            .metadata(&metadata)
            .unwrap_err()
            .contains("geometry"));
    }
}
