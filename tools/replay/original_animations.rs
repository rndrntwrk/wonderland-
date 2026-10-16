//! Bounded original-animation input adapter for the private shipping cooker.
//!
//! Provider filenames, not Animation.Name, are resource identities. Runtime
//! keys retain the effective STR owner/table/index used to select each input.
//! Original bytes enter the same manifest as noncritical visual resources;
//! their source-derived timing enters the sealed runtime binding.
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, path::{Component, Path}};
use wonderland_asset_cooker::{fs_io, manifest::{CookLimits, PreparedResource}};
use wonderland_content_ir::manifest::{Digest, Origin, Provenance, Redistribution, ResourceCodec, ResourceKind};
use wonderland_content_runtime_bridge::sim_core::{
    avatars::{events::TimeProperty, timeline::AnimationMetadata},
    state::{AnimationKey, ContentSet},
};
use wonderland_legacy_formats::vitaboy::{self, Animation};

const MAX_INPUTS: usize = 64;
const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputManifest {
    schema_version: u32,
    animations: Vec<Input>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    owner: u32,
    table: u16,
    index: u16,
    resource: String,
    path: String,
    byte_len: usize,
    sha256: Digest,
}

#[derive(Default)]
pub struct AnimationInputs {
    pub metadata: Vec<(AnimationKey, AnimationMetadata)>,
    pub resources: Vec<PreparedResource>,
    pub receipt: Vec<Value>,
}

fn provider_name(name: &str) -> Result<(), String> {
    if name.len() <= 5 || name.len() > 1024 || !name.is_ascii()
        || !name.to_ascii_lowercase().ends_with(".anim")
        || name.bytes().any(|b| b.is_ascii_control() || b == b'/' || b == b'\\')
    {
        return Err("animation requires an unambiguous ASCII provider basename".into());
    }
    Ok(())
}

fn relative_path(path: &str) -> Result<&Path, String> {
    let path = Path::new(path);
    if path.as_os_str().is_empty() || !path.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err("animation input path must stay under its portable manifest directory".into());
    }
    Ok(path)
}

/// The source PropertyList indexer returns the first matching pair. Preserve
/// that rule and VMAnimationState's unsorted motion/list/item encounter order.
/// Negative source event times cannot be represented by the runtime DTO and
/// are rejected rather than wrapped, clamped, or silently discarded.
fn source_metadata(resource: &str, source: &Animation) -> Result<AnimationMetadata, String> {
    provider_name(resource)?;
    let num_frames = i32::try_from(source.num_frames).map_err(|_| "animation frame count overflow")?;
    if num_frames <= 0 || num_frames > 1_000_000
        || source.motions.iter().map(|m| m.frame_count).max().unwrap_or(0) != source.num_frames
    {
        return Err("animation requires positive source-derived NumFrames".into());
    }
    let mut time_properties = Vec::new();
    for property in source.time_properties_in_source_order() {
        if time_properties.len() >= 65_536 {
            return Err("animation time-property count limit".into());
        }
        let time_ms = u32::try_from(property.id).map_err(|_| "negative source time property is unsupported")?;
        let mut properties = BTreeMap::new();
        for (key, value) in property.properties.items.iter().flat_map(|item| &item.pairs) {
            if key.len() > 1024 || value.len() > 16_384 {
                return Err("animation time-property text limit".into());
            }
            properties.entry(key.clone()).or_insert_with(|| value.clone());
            if properties.len() > 256 {
                return Err("animation time-property key count limit".into());
            }
        }
        time_properties.push(TimeProperty { time_ms, properties });
    }
    let metadata = AnimationMetadata { resource: resource.into(), num_frames, time_properties };
    metadata.validate().map_err(|e| format!("source animation metadata: {e:?}"))?;
    Ok(metadata)
}

pub fn read(path: &Path, limits: &CookLimits) -> Result<AnimationInputs, String> {
    let bytes = fs_io::read_bounded(path, 1024 * 1024).map_err(|e| e.to_string())?;
    let manifest: InputManifest = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if manifest.schema_version != 1 || manifest.animations.is_empty() || manifest.animations.len() > MAX_INPUTS {
        return Err("original animation manifest version or input count".into());
    }
    let root = path.parent().ok_or("animation manifest directory missing")?.canonicalize().map_err(|e| e.to_string())?;
    let mut result = AnimationInputs::default();
    let mut keys = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut total_bytes = 0usize;
    for input in manifest.animations {
        provider_name(&input.resource)?;
        let key = AnimationKey { owner: input.owner, scope: u32::from(input.table), id: input.index };
        if !keys.insert(key) || !names.insert(input.resource.to_ascii_lowercase()) {
            return Err("duplicate animation STR reference or provider name".into());
        }
        let relative = relative_path(&input.path)?;
        if relative.file_name().and_then(|v| v.to_str()) != Some(input.resource.as_str()) {
            return Err("animation input basename differs from provider identity".into());
        }
        let selected = root.join(relative).canonicalize().map_err(|e| e.to_string())?;
        if !selected.starts_with(&root) {
            return Err("animation input resolves outside manifest directory".into());
        }
        if input.byte_len > MAX_INPUT_BYTES {
            return Err("original animation byte limit".into());
        }
        total_bytes = total_bytes.checked_add(input.byte_len).ok_or("animation aggregate overflow")?;
        if total_bytes > MAX_TOTAL_BYTES {
            return Err("animation aggregate byte limit".into());
        }
        let bytes = fs_io::read_bounded(&selected, input.byte_len).map_err(|e| e.to_string())?;
        if bytes.len() != input.byte_len || Digest::of(&bytes) != input.sha256 {
            return Err("original animation size or SHA-256 mismatch".into());
        }
        let decoded = vitaboy::decode_animation(&bytes, &limits.legacy).map_err(|e| e.to_string())?;
        let metadata = source_metadata(&input.resource, &decoded)?;
        let id = format!("original-animations/{}", input.resource.to_ascii_lowercase());
        result.receipt.push(json!({"resource":input.resource,"manifest_resource":id,"source_sha256":input.sha256,
            "byte_len":bytes.len(),"str_reference":{"owner":input.owner,"table":input.table,"index":input.index},
            "num_frames":metadata.num_frames,"time_property_count":metadata.time_properties.len(),
            "metadata_sha256":Digest::of(&serde_json::to_vec(&metadata).map_err(|e|e.to_string())?)}));
        result.resources.push(PreparedResource {
            id, group: "private-original-animations".into(), kind: ResourceKind::Visual,
            codec: ResourceCodec::VitaboyAnimation, payload: bytes, dependencies: vec![],
            simulation_critical: false, locale: None, variants: BTreeSet::new(),
            provenance: Provenance { origin: Origin::Original, source: input.resource,
                source_hash: input.sha256, patch_hashes: vec![], tuning_hash: None,
                license: None, redistribution: Redistribution::Restricted },
        });
        result.metadata.push((key, metadata));
    }
    Ok(result)
}

impl AnimationInputs {
    /// Run after the normal sealed loader, so the source references and actual
    /// runtime metadata are checked against the resulting effective content.
    pub fn verify_loaded(&self, content: &ContentSet) -> Result<(), String> {
        for (key, metadata) in &self.metadata {
            let base = content.string(key.owner, key.scope as u16, i32::from(key.id))
                .ok_or("animation STR reference absent from effective content")?;
            if !metadata.resource.eq_ignore_ascii_case(&format!("{base}.anim")) {
                return Err("animation provider differs from effective STR reference".into());
            }
            if content.animation_by_resource(&metadata.resource) != Some(metadata) {
                return Err("sealed runtime animation metadata differs from original bytes".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wonderland_legacy_formats::{vitaboy::{F32Bits, CoordinatePolicy, Motion, PropertyItem, PropertyList, TimePropertyList}};

    fn animation() -> Animation {
        let motion = Motion { unknown: 0, bone_name: "root".into(), frame_count: 7,
            duration_ms: F32Bits(0), translation_flag: 0, rotation_flag: 0,
            first_translation_index: -1, first_rotation_index: -1,
            properties_flag: 0, properties: vec![], time_properties_flag: 1,
            time_properties: vec![TimePropertyList { items: vec![
                vitaboy::TimeProperty { id: 200, properties: PropertyList { items: vec![
                    PropertyItem { pairs: vec![("xevt".into(), "1".into())] },
                    PropertyItem { pairs: vec![("xevt".into(), "2".into())] },
                ] } },
                vitaboy::TimeProperty { id: 0, properties: PropertyList { items: vec![] } },
            ] }] };
        Animation { version: 2, name: "unrelated-internal-name".into(), duration_ms: F32Bits(0),
            distance: F32Bits(0), is_moving: 0, translations: vec![], rotations: vec![],
            motions: vec![motion], num_frames: 7, coordinate_policy: CoordinatePolicy::FreeSo }
    }
    #[test]
    fn exact_source_frames_first_property_and_encounter_order() {
        let metadata = source_metadata("provider.anim", &animation()).unwrap();
        assert_eq!(metadata.resource, "provider.anim");
        assert_eq!(metadata.num_frames, 7);
        assert_eq!(metadata.time_properties.iter().map(|p|p.time_ms).collect::<Vec<_>>(), [200, 0]);
        assert_eq!(metadata.time_properties[0].properties["xevt"], "1");
    }
    #[test]
    fn negative_times_and_inconsistent_frames_are_rejected() {
        let mut source = animation(); source.motions[0].time_properties[0].items[0].id = -1;
        assert!(source_metadata("provider.anim", &source).is_err());
        source = animation(); source.num_frames = 8;
        assert!(source_metadata("provider.anim", &source).is_err());
        source = animation(); source.num_frames = 0; source.motions[0].frame_count = 0;
        assert!(source_metadata("provider.anim", &source).is_err());
    }
    #[test]
    fn provider_names_and_portable_paths_are_bounded() {
        for value in ["", "name", "../name.anim", "dir/name.anim", "dir\\name.anim", "n\n.anim", "é.anim"] {
            assert!(provider_name(value).is_err(), "{value:?}");
        }
        for value in ["", "/absolute.anim", "../outside.anim", "clips/../../outside.anim"] {
            assert!(relative_path(value).is_err(), "{value:?}");
        }
        assert!(provider_name("source.ANIM").is_ok());
        assert!(relative_path("selected/source.anim").is_ok());
    }
    #[test]
    fn original_bytes_and_unique_provider_references_are_required() {
        let root = std::env::temp_dir().join(format!("wonderland-original-animation-{}-{}",
            std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&root).unwrap();
        let limits = CookLimits::default();
        let bytes = vitaboy::encode_animation(&animation(), &limits.legacy).unwrap();
        std::fs::write(root.join("provider.anim"), &bytes).unwrap();
        let entry = json!({"owner":0,"table":150,"index":3,"resource":"provider.anim",
            "path":"provider.anim","byte_len":bytes.len(),"sha256":Digest::of(&bytes)});
        let path = root.join("inputs.json");
        let save = |entries: Vec<Value>| std::fs::write(&path,
            serde_json::to_vec(&json!({"schema_version":1,"animations":entries})).unwrap()).unwrap();
        save(vec![entry.clone()]);
        let imported = read(&path, &limits).unwrap();
        assert_eq!(imported.metadata[0].0, AnimationKey { owner: 0, scope: 150, id: 3 });
        assert_eq!(imported.resources[0].payload, bytes);
        assert_eq!(imported.resources[0].provenance.source_hash, Digest::of(&bytes));
        assert_eq!(imported.resources[0].provenance.redistribution, Redistribution::Restricted);
        let mut duplicate = entry.clone(); duplicate["index"] = json!(4);
        save(vec![entry.clone(), duplicate]);
        assert!(read(&path, &limits).unwrap_err_string().contains("duplicate"));
        let mut mismatch = entry.clone(); mismatch["sha256"] = json!(Digest::of(b"different bytes"));
        save(vec![mismatch]);
        assert!(read(&path, &limits).unwrap_err_string().contains("SHA-256"));
        let mut mismatched_name = entry; mismatched_name["resource"] = json!("another.anim");
        save(vec![mismatched_name]);
        assert!(read(&path, &limits).unwrap_err_string().contains("basename"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn effective_str_and_sealed_metadata_must_match_original_input() {
        use wonderland_content_runtime_bridge::sim_core::{state::TuningSet, vm::RoutineStore};
        let key = AnimationKey { owner: 0, scope: 150, id: 0 };
        let metadata = source_metadata("provider.anim", &animation()).unwrap();
        let inputs = AnimationInputs { metadata: vec![(key, metadata.clone())], ..Default::default() };
        let content = |name: &str, value: AnimationMetadata| {
            ContentSet::new(RoutineStore::new(), vec![], vec![(key, value)], TuningSet::default())
                .unwrap().with_strings(vec![((0, 150), vec![name.into()])]).unwrap()
        };
        assert!(inputs.verify_loaded(&content("provider", metadata.clone())).is_ok());
        assert!(inputs.verify_loaded(&content("different", metadata.clone())).unwrap_err().contains("STR reference"));
        let mut different = metadata; different.num_frames += 1;
        assert!(inputs.verify_loaded(&content("provider", different)).unwrap_err().contains("differs from original bytes"));
    }

    trait ErrorText { fn unwrap_err_string(self) -> String; }
    impl<T> ErrorText for Result<T, String> {
        fn unwrap_err_string(self) -> String { match self { Err(e) => e, Ok(_) => panic!("expected rejection") } }
    }
}
