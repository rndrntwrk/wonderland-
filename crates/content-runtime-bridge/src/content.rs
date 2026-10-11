//! Source-backed resource conversion. Geometry and non-BCON runtime tuning are
//! explicit integration inputs, never inferred from an object's filename.
use crate::budget::ImportBudget;
use bincode::Options;
use sim_core::{
    avatars::timeline::AnimationMetadata,
    state::{
        AnimationKey, ContentSet, InteractionAdvertisement, ObjectDefinition, RoutingSlot,
        TuningSet,
    },
    vm::{RoutineKey, RoutineScope, RoutineStore, VmRoutine},
    world::{Footprint, PlacementRules},
};
use std::collections::{BTreeMap, BTreeSet};
use wonderland_content_ir::{
    objects::{resource_identity, ResolvedContent, ResourceNamespace},
    patches::iff_sha256,
    strings::{lookup_string, LocaleSelection},
    tuning::ResolvedTuning,
};
use wonderland_legacy_formats::{
    iff::{ChunkKey, IffChunk, IffFile},
    semantic::{
        decode_objd, decode_slot, decode_strings, decode_ttab_with_variant, Objd, TtabVariant,
    },
    Limits,
};

#[derive(Clone, Debug)]
pub struct RuntimeMetadata {
    pub footprint: Footprint,
    pub placement_rules: PlacementRules,
    pub master_guid: Option<u32>,
    pub family: i16,
    /// Already normalized by the routing/content owner; no float rounding is
    /// silently selected by this adapter.
    pub routing_slots: Vec<(u16, RoutingSlot)>,
}

pub struct ObjectImport<'a> {
    pub resolved: &'a ResolvedContent,
    pub object_chunk_id: u16,
    /// Explicit resource namespace identity, required when a GLOB is present.
    pub semiglobal_owner: Option<u32>,
    pub runtime: RuntimeMetadata,
}

#[derive(Clone, Debug)]
pub struct ImportOptions {
    pub locale: LocaleSelection,
    pub ttab_variant: TtabVariant,
    pub runtime_tuning: TuningSet,
    pub animations: Vec<(AnimationKey, AnimationMetadata)>,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            locale: LocaleSelection::default(),
            ttab_variant: TtabVariant::Standard,
            runtime_tuning: TuningSet::default(),
            animations: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ImportedInteraction {
    pub tta_index: u32,
    pub action: RoutineKey,
    pub check: Option<RoutineKey>,
    pub code_owner: u32,
    pub flags: u32,
    pub permissions: u32,
    pub label: Option<String>,
    pub advertisement: InteractionAdvertisement,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ImportedObject {
    pub guid: u32,
    pub object_chunk_id: u16,
    pub source_name: String,
    pub effective_identity: [u8; 32],
    pub semiglobal_owner: Option<u32>,
    /// None means absent; an empty table is Some(empty).
    pub interactions: Option<Vec<ImportedInteraction>>,
    pub global_interactions: Vec<ImportedInteraction>,
}

#[derive(Clone, Debug)]
pub struct ImportedContent {
    pub content: ContentSet,
    pub objects: Vec<ImportedObject>,
}

/// Internal borrowed conversion input. Only the source resolver wrapper and
/// verified cooked loader construct this view; neither invents the other's
/// provenance. Scope hashes describe the supplied ordered scope projection.
#[derive(Clone, Copy)]
pub(crate) struct ScopeView<'a> {
    pub(crate) name: &'a str,
    pub(crate) iff: &'a IffFile,
    pub(crate) identity: [u8; 32],
}

pub(crate) struct ContentView<'a> {
    pub(crate) iff: &'a IffFile,
    pub(crate) semiglobal_name: Option<&'a str>,
    pub(crate) semiglobal: Option<ScopeView<'a>>,
    pub(crate) global: Option<ScopeView<'a>>,
    pub(crate) tuning: &'a ResolvedTuning,
    pub(crate) resolved: Option<&'a ResolvedContent>,
}

pub(crate) struct ConversionObject<'a> {
    pub(crate) source: ContentView<'a>,
    pub(crate) object_chunk_id: u16,
    pub(crate) semiglobal_owner: Option<u32>,
    pub(crate) runtime: &'a RuntimeMetadata,
}

pub(crate) struct ConvertedObject {
    pub(crate) guid: u32,
    pub(crate) object_chunk_id: u16,
    pub(crate) semiglobal_owner: Option<u32>,
    pub(crate) interactions: Option<Vec<ImportedInteraction>>,
    pub(crate) global_interactions: Vec<ImportedInteraction>,
}

pub(crate) struct ConvertedContent {
    pub(crate) content: ContentSet,
    pub(crate) objects: Vec<ConvertedObject>,
}

pub fn import_content(
    objects: &[ObjectImport<'_>],
    options: ImportOptions,
    limits: &Limits,
) -> Result<ImportedContent, String> {
    if objects.is_empty() || objects.len() > 32767 || objects.len() > limits.max_entries {
        return Err("object import count outside bounds".into());
    }
    let mut budget = ImportBudget::new(limits);
    budget.entries::<ConversionObject<'_>>(objects.len())?;
    budget.entries::<ImportedObject>(objects.len())?;
    let mut inputs = Vec::with_capacity(objects.len());
    for input in objects {
        let resolved = input.resolved;
        budget.reserve(resolved.source_name.len())?;
        verify_resolved(resolved, limits, &mut budget)?;
        inputs.push(ConversionObject {
            source: ContentView {
                iff: &resolved.iff,
                semiglobal_name: resolved.semiglobal_name.as_deref(),
                semiglobal: resolved
                    .semiglobal
                    .as_ref()
                    .map(|s| resolved_scope(resolved, s, ResourceNamespace::Semiglobal))
                    .transpose()?,
                global: resolved
                    .global
                    .as_ref()
                    .map(|s| resolved_scope(resolved, s, ResourceNamespace::Global))
                    .transpose()?,
                tuning: &resolved.tuning,
                resolved: Some(resolved),
            },
            object_chunk_id: input.object_chunk_id,
            semiglobal_owner: input.semiglobal_owner,
            runtime: &input.runtime,
        });
    }
    let converted = convert_content(&inputs, options, limits, &mut budget)?;
    let reports = converted
        .objects
        .into_iter()
        .zip(objects)
        .map(|(object, input)| ImportedObject {
            guid: object.guid,
            object_chunk_id: object.object_chunk_id,
            source_name: input.resolved.source_name.clone(),
            effective_identity: input.resolved.identity,
            semiglobal_owner: object.semiglobal_owner,
            interactions: object.interactions,
            global_interactions: object.global_interactions,
        })
        .collect();
    Ok(ImportedContent {
        content: converted.content,
        objects: reports,
    })
}

fn resolved_scope<'a>(
    resolved: &ResolvedContent,
    source: &'a wonderland_content_ir::objects::SourceContent,
    namespace: ResourceNamespace,
) -> Result<ScopeView<'a>, String> {
    let identity = resolved
        .scope_provenance
        .iter()
        .find(|scope| scope.namespace == namespace)
        .ok_or("missing effective scope identity")?
        .effective_hash;
    Ok(ScopeView {
        name: &source.name,
        iff: &source.iff,
        identity,
    })
}

pub(crate) fn convert_content(
    objects: &[ConversionObject<'_>],
    options: ImportOptions,
    limits: &Limits,
    budget: &mut ImportBudget,
) -> Result<ConvertedContent, String> {
    if objects.is_empty() || objects.len() > 32767 || objects.len() > limits.max_entries {
        return Err("object import count outside bounds".into());
    }
    budget.tuning(&options.runtime_tuning)?;
    if options.animations.len() > 65536 || options.animations.len() > limits.max_entries {
        return Err("runtime animation count limit".into());
    }
    for (_, animation) in &options.animations {
        budget.animation(animation)?;
    }
    budget.entries::<(AnimationKey, AnimationMetadata)>(options.animations.capacity())?;
    budget.entries::<ConvertedObject>(objects.len())?;
    budget.entries::<ObjectDefinition>(objects.len())?;
    budget.entries::<(&ConversionObject<'_>, Objd)>(objects.len())?;
    let mut routines = BTreeMap::new();
    let mut strings = BTreeMap::new();
    let mut names = BTreeMap::new();
    let mut definitions = Vec::with_capacity(objects.len());
    let mut reports = Vec::with_capacity(objects.len());
    let mut tuning = options.runtime_tuning;
    let mut slots = BTreeMap::new();
    let mut store = RoutineStore::new();
    let mut guids = BTreeSet::new();
    let mut source_objects = Vec::with_capacity(objects.len());
    let mut shared_scopes = BTreeMap::new();

    for input in objects {
        budget.metadata(input.runtime)?;
        budget.entries::<u16>(103)?; // Largest fixed source RawData prefix.
        budget.map_entries::<u32, ()>(1)?;
        let raw = resource(input.source.iff, *b"OBJD", input.object_chunk_id)
            .ok_or_else(|| format!("missing OBJD {}", input.object_chunk_id))?;
        let object = decode_source_objd(&raw.data, &budget.limits(limits))?;
        if object.field("NumAttributes").unwrap_or(0) > 4096 {
            return Err("runtime attribute count exceeds 4096".into());
        }
        let guid = object.guid();
        if guid == 0 || !guids.insert(guid) {
            return Err("zero or duplicate object GUID".into());
        }
        if input.source.semiglobal_name.is_some() && input.source.semiglobal.is_none() {
            return Err(format!(
                "missing effective semiglobal {:?}",
                input.source.semiglobal_name
            ));
        }
        if input.semiglobal_owner.is_some() != input.source.semiglobal.is_some()
            || input.semiglobal_owner == Some(0)
        {
            return Err("semiglobal resource requires an explicit nonzero owner identity".into());
        }
        if let (Some(expected), Some(actual)) =
            (&input.source.semiglobal_name, &input.source.semiglobal)
        {
            let source = actual
                .name
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(actual.name);
            let source = source
                .get(..source.len().saturating_sub(4))
                .filter(|_| source[source.len().saturating_sub(4)..].eq_ignore_ascii_case(".iff"))
                .unwrap_or(source);
            if !source.eq_ignore_ascii_case(expected) {
                return Err("semiglobal source name does not match GLOB".into());
            }
        }
        if let Some(owner) = input.semiglobal_owner {
            budget.map_entries::<u32, u32>(1)?;
            store
                .bind_semiglobal(guid, owner)
                .map_err(|e| e.to_string())?;
        }
        import_scope(
            input.source.iff,
            RoutineScope::Private(guid),
            guid,
            limits,
            options.locale,
            &mut routines,
            &mut strings,
            budget,
        )?;
        import_names(
            input.source.iff,
            RoutineScope::Private(guid),
            guid,
            &mut names,
            budget,
        )?;
        if let (Some(source), Some(owner)) = (&input.source.semiglobal, input.semiglobal_owner) {
            budget.map_entries::<RoutineScope, [u8; 32]>(1)?;
            register_scope(
                source.identity,
                RoutineScope::SemiGlobal(owner),
                &mut shared_scopes,
            )?;
            import_scope(
                source.iff,
                RoutineScope::SemiGlobal(owner),
                owner,
                limits,
                options.locale,
                &mut routines,
                &mut strings,
                budget,
            )?;
            // Source TreeByName: private names win; semiglobal names fill gaps.
            import_names(
                source.iff,
                RoutineScope::SemiGlobal(owner),
                guid,
                &mut names,
                budget,
            )?;
        }
        if let Some(source) = &input.source.global {
            budget.map_entries::<RoutineScope, [u8; 32]>(1)?;
            register_scope(source.identity, RoutineScope::Global, &mut shared_scopes)?;
            import_scope(
                source.iff,
                RoutineScope::Global,
                0,
                limits,
                options.locale,
                &mut routines,
                &mut strings,
                budget,
            )?;
            import_names(source.iff, RoutineScope::Global, 0, &mut names, budget)?;
        }
        import_tuning(input, guid, &mut tuning.values, budget)?;
        for (index, slot) in &input.runtime.routing_slots {
            insert_consistent(&mut slots, (guid, *index), slot.clone(), "routing slot")?;
        }
        source_objects.push((input, object));
    }
    if objects.iter().any(|input| {
        input
            .semiglobal_owner
            .is_some_and(|owner| guids.contains(&owner))
    }) {
        return Err("semiglobal namespace identity collides with an object GUID".into());
    }
    for (key, routine) in routines {
        store.insert(key, routine).map_err(|e| e.to_string())?;
    }
    for (input, raw) in source_objects {
        let guid = raw.guid();
        let attribute_count = usize::from(raw.field("NumAttributes").unwrap_or(0))
            .max(strings.get(&(guid, 256)).map_or(0, Vec::len));
        if attribute_count > 4096 {
            return Err("runtime attribute count exceeds 4096".into());
        }
        budget.map_entries::<u32, ObjectDefinition>(1)?;
        budget.entries::<i16>(attribute_count + 80 + raw.fields.len() + 2)?;
        let mut definition = ObjectDefinition::new(guid, attribute_count as u16);
        definition.definition = runtime_definition(&raw);
        definition.object_data[8] = 1 << 8; // VMEntity constructor: ChairFacing.
        definition.animation_table_id = raw.field("AnimationTableID").unwrap_or(0);
        definition.body_string_id = raw.field("BodyStringID").unwrap_or(0);
        definition.level_offset = i8::try_from(raw.field("LevelOffset").unwrap_or(0) as i16)
            .map_err(|_| "OBJD level offset cannot fit runtime representation")?;
        definition.footprint = input.runtime.footprint.clone();
        definition.placement_rules = input.runtime.placement_rules.clone();
        definition.family = input.runtime.family;
        definition.master_guid = input.runtime.master_guid;
        if raw.master_id() != 0 && raw.sub_index() != -1 && input.runtime.master_guid.is_none() {
            return Err("multipart definition requires its explicit master GUID".into());
        }
        let master = if let Some(master_guid) = input.runtime.master_guid {
            if raw.master_id() == 0 || raw.sub_index() == -1 {
                return Err("master GUID supplied for a non-subobject definition".into());
            }
            // The chosen candidate can remain live while the next OBJD is
            // decoded. Reserve both fixed source prefixes before this scan.
            budget.entries::<Objd>(2)?;
            budget.entries::<u16>(2 * 103)?;
            let mut master = None;
            for chunk in &input.source.iff.chunks {
                if chunk.key.kind == *b"OBJD" {
                    let candidate = decode_source_objd(&chunk.data, &budget.limits(limits))?;
                    if candidate.guid() == master_guid {
                        master = Some(candidate);
                    }
                }
            }
            let master =
                master.ok_or("master definition is not in the effective object resource")?;
            if master.master_id() != raw.master_id() || master.sub_index() != -1 {
                return Err("master definition does not match source multipart identity".into());
            }
            budget.entries::<i16>(master.fields.len() + 2)?;
            definition.master_definition = runtime_definition(&master);
            Some(master)
        } else {
            None
        };
        if let Some(slot) = resource(input.source.iff, *b"SLOT", raw.slot_id()) {
            let slots =
                decode_slot(&slot.data, &budget.limits(limits)).map_err(|e| e.to_string())?;
            definition.slot_count =
                u16::try_from(slots.slots.iter().filter(|s| s.type_id == 0).count())
                    .map_err(|_| "containment slot count overflow")?;
        }
        budget.entries::<(u16, u16)>(512)?; // Maximum bounded transient function table.
        let functions = if raw.field("UsesFnTable").unwrap_or(0) == 0 {
            generated_functions(&raw)
        } else {
            let chunk = resource(input.source.iff, *b"OBJf", input.object_chunk_id)
                .ok_or("missing required OBJf function table")?;
            decode_functions(chunk, &budget.limits(limits))?
        };
        definition.entry_point_count = functions.len() as u16;
        for (index, (condition, action)) in functions.into_iter().enumerate() {
            if guid == 0xa9bb_3a76 && index == 17 {
                continue; // Explicit VMEntity source override for this definition.
            }
            if action != 0 {
                budget.map_entries::<u8, RoutineKey>(1)?;
                definition
                    .entry_points
                    .insert(index as u8, required_routine(&store, guid, action)?);
            }
            if condition != 0 {
                budget.map_entries::<u8, RoutineKey>(1)?;
                definition
                    .entry_conditions
                    .insert(index as u8, required_routine(&store, guid, condition)?);
            }
        }
        let interactions = import_interactions(
            input,
            &raw,
            master.as_ref(),
            &store,
            options.locale,
            options.ttab_variant,
            limits,
            budget,
        )?;
        let global_interactions = import_global_interactions(
            input,
            &raw,
            &store,
            options.locale,
            options.ttab_variant,
            limits,
            budget,
        )?;
        let roots = input
            .source
            .iff
            .chunks
            .iter()
            .filter(|chunk| chunk.key.kind == *b"BHAV")
            .map(|chunk| RoutineKey {
                scope: RoutineScope::Private(guid),
                id: chunk.key.id,
            })
            .chain(definition.entry_points.values().copied())
            .chain(definition.entry_conditions.values().copied())
            .chain(
                interactions
                    .iter()
                    .flatten()
                    .flat_map(|item| [Some(item.action), item.check].into_iter().flatten()),
            );
        verify_direct_calls(
            &store,
            input,
            guid,
            roots.chain(
                global_interactions
                    .iter()
                    .flat_map(|item| [Some(item.action), item.check].into_iter().flatten()),
            ),
            budget,
        )?;
        reports.push(ConvertedObject {
            guid,
            object_chunk_id: input.object_chunk_id,
            semiglobal_owner: input.semiglobal_owner,
            interactions,
            global_interactions,
        });
        definitions.push(definition);
    }
    reserve_validation_workspace(
        budget,
        &(
            &store,
            &definitions,
            &options.animations,
            &slots,
            &strings,
            &names,
            &tuning,
        ),
        definitions.len(),
        &options.animations,
    )?;
    let content = ContentSet::new(store, definitions, options.animations, tuning)?
        .with_strings(strings.into_iter().collect())?
        .with_named_trees(names.into_iter().collect())?
        .with_routing_slots(slots.into_iter().collect())?;
    Ok(ConvertedContent {
        content,
        objects: reports,
    })
}

fn resource(file: &IffFile, kind: [u8; 4], id: u16) -> Option<&IffChunk> {
    file.chunks
        .iter()
        .find(|chunk| chunk.key == ChunkKey { kind, id })
}

pub(crate) fn decode_source_objd(data: &[u8], limits: &Limits) -> Result<Objd, String> {
    let version = u32::from_le_bytes(
        data.get(..4)
            .ok_or("truncated OBJD version")?
            .try_into()
            .unwrap(),
    );
    // OBJD.cs reads this fixed RawData prefix even when round-trip tooling
    // preserves extra words. In particular 136 has no BHAV_Repair field.
    let words = match version {
        136 => 78,
        138 => 93,
        139 => 94,
        140 | 141 => 95,
        142 => 103,
        _ => return Err(format!("unsupported runtime OBJD version {version}")),
    };
    let source = data
        .get(..4 + words * 2)
        .ok_or("truncated source OBJD fields")?;
    decode_objd(source, limits).map_err(|error| error.to_string())
}

fn runtime_definition(object: &Objd) -> Vec<i16> {
    // VMMemory.GetEntityDefinitionVar exposes Version1/Version2 at 0/1,
    // followed by RawData[index - 2]. A indexes the supplied array directly.
    [
        ((object.version % 0xffff) as u16) as i16,
        (object.version >> 16) as i16,
    ]
    .into_iter()
    .chain(object.fields.iter().map(|word| *word as i16))
    .collect()
}

fn register_scope(
    identity: [u8; 32],
    runtime: RoutineScope,
    scopes: &mut BTreeMap<RoutineScope, [u8; 32]>,
) -> Result<(), String> {
    insert_consistent(scopes, runtime, identity, "scope identity")
}

fn verify_resolved(
    resolved: &ResolvedContent,
    limits: &Limits,
    budget: &mut ImportBudget,
) -> Result<(), String> {
    if resolved.iff.chunks.len() != resolved.resources.len() {
        return Err("resolved resource identity count changed".into());
    }
    let mut keys = BTreeSet::new();
    for (chunk, expected) in resolved.iff.chunks.iter().zip(&resolved.resources) {
        budget.map_entries::<ChunkKey, ()>(1)?;
        if !keys.insert(chunk.key)
            || resource_identity(chunk, &budget.limits(limits)).map_err(|e| e.to_string())?
                != *expected
        {
            return Err("effective resource identity changed after resolution".into());
        }
    }
    for (namespace, source) in [
        (ResourceNamespace::Semiglobal, &resolved.semiglobal),
        (ResourceNamespace::Global, &resolved.global),
    ] {
        if let Some(source) = source {
            let expected = resolved
                .scope_provenance
                .iter()
                .find(|s| s.namespace == namespace)
                .ok_or("missing effective scope identity")?;
            if source.name != expected.source_name
                || scoped_hash(&source.iff, limits, budget)? != expected.effective_hash
            {
                return Err("effective scope identity changed after resolution".into());
            }
        }
    }
    Ok(())
}

fn scoped_hash(
    file: &IffFile,
    limits: &Limits,
    budget: &mut ImportBudget,
) -> Result<[u8; 32], String> {
    // iff::encode checks decoded payload bytes separately from its 64-byte
    // envelope and 76-byte chunk headers. Admit its full output and temporary
    // duplicate-key set before calling it, including empty unknown chunks.
    let encoded_size = file.chunks.iter().try_fold(64usize, |size, chunk| {
        size.checked_add(76)
            .and_then(|size| size.checked_add(chunk.data.len()))
            .ok_or("scope hash workspace size overflow")
    })?;
    budget
        .map_entries::<ChunkKey, ()>(file.chunks.len())
        .and_then(|_| budget.reserve(encoded_size))
        .map_err(|error| format!("scope hash workspace: {error}"))?;
    // This call draws on the buffer reservation above, rather than asking a
    // second time for the same payload allocation from the remaining budget.
    let admitted = Limits {
        max_total_decoded_bytes: encoded_size,
        max_input_bytes: limits.max_input_bytes.min(encoded_size),
        ..*limits
    };
    iff_sha256(file, &admitted).map_err(|error| error.to_string())
}

fn validation_buffer_size(
    projected: &impl serde::Serialize,
    object_count: usize,
) -> Result<usize, String> {
    // A ContentSet serializes these supplied fields without struct tags. Our
    // object Vec omits each final map's GUID key. Suits, job uniforms,
    // interaction tables, build catalog and interaction ads start empty.
    // Fixed-int bincode adds 4 bytes per GUID and 8 per empty map.
    let bytes = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .with_limit(sim_core::state::MAX_CONTENT_BYTES)
        .serialized_size(projected)
        .map_err(|error| error.to_string())?;
    usize::try_from(bytes)
        .ok()
        .and_then(|bytes| {
            object_count
                .checked_mul(4)
                .and_then(|keys| bytes.checked_add(keys))
                .and_then(|bytes| bytes.checked_add(40))
        })
        .ok_or_else(|| "runtime validation buffer size overflow".into())
}

fn reserve_validation_workspace(
    budget: &mut ImportBudget,
    projected: &impl serde::Serialize,
    object_count: usize,
    animations: &[(AnimationKey, AnimationMetadata)],
) -> Result<(), String> {
    // A retains this lowercase resource index while serializing ContentSet.
    budget.map_entries::<String, &AnimationMetadata>(animations.len())?;
    for (_, animation) in animations {
        budget.reserve(animation.resource.len())?;
    }
    // bincode 1.3.3 first counts bytes, then allocates exactly that capacity.
    // The full final projection bounds all intermediate ContentSet builders;
    // their validation buffers are sequential, so one workspace is sufficient.
    budget.reserve(validation_buffer_size(projected, object_count)?)
}

fn insert_consistent<K: Ord, V: PartialEq>(
    map: &mut BTreeMap<K, V>,
    key: K,
    value: V,
    kind: &str,
) -> Result<(), String> {
    if let Some(previous) = map.get(&key) {
        if previous != &value {
            return Err(format!("conflicting shared {kind} identity"));
        }
    } else {
        if map.len() >= 65536 {
            return Err(format!("runtime {kind} count limit"));
        }
        map.insert(key, value);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // Explicit source context and output maps.
fn import_scope(
    file: &IffFile,
    scope: RoutineScope,
    owner: u32,
    limits: &Limits,
    locale: LocaleSelection,
    routines: &mut BTreeMap<RoutineKey, VmRoutine>,
    strings: &mut BTreeMap<(u32, u16), Vec<String>>,
    budget: &mut ImportBudget,
) -> Result<(), String> {
    for chunk in &file.chunks {
        match &chunk.key.kind {
            b"BHAV" => {
                let valid = match scope {
                    RoutineScope::Global => chunk.key.id < 4096,
                    RoutineScope::Private(_) => (4096..8192).contains(&chunk.key.id),
                    RoutineScope::SemiGlobal(_) => chunk.key.id >= 8192,
                };
                if !valid {
                    return Err("BHAV ID does not belong to its declared namespace".into());
                }
                let key = RoutineKey {
                    scope,
                    id: chunk.key.id,
                };
                budget.map_entries::<RoutineKey, VmRoutine>(1)?;
                budget.reserve(chunk.data.len())?; // Instruction output is smaller than its source envelope.
                insert_consistent(
                    routines,
                    key,
                    crate::import_bhav(chunk, &budget.limits(limits))?,
                    "routine",
                )?;
            }
            b"STR#" => {
                let table = decode_strings(&chunk.data, &budget.limits(limits))
                    .map_err(|e| e.to_string())?;
                budget.decoded_strings(&table)?;
                budget.map_entries::<(u32, u16), Vec<String>>(1)?;
                let count = (0..=65536)
                    .take_while(|index| lookup_string(&table, *index, locale).is_some())
                    .count();
                if count > 65536 {
                    return Err("runtime string table count limit".into());
                }
                budget.entries::<String>(count)?;
                let mut selected = Vec::with_capacity(count);
                for index in 0..count {
                    let value =
                        lookup_string(&table, index, locale).expect("selected count was checked");
                    selected.push(budget.decode_text(&value.item.value)?);
                }
                insert_consistent(strings, (owner, chunk.key.id), selected, "string table")?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn import_names(
    file: &IffFile,
    scope: RoutineScope,
    owner: u32,
    names: &mut BTreeMap<(u32, String), RoutineKey>,
    budget: &mut ImportBudget,
) -> Result<(), String> {
    for chunk in file.chunks.iter().filter(|c| c.key.kind == *b"BHAV") {
        budget.map_entries::<(u32, String), RoutineKey>(1)?;
        budget.reserve(chunk.label.len())?;
        let name: String = chunk
            .label
            .iter()
            .take_while(|b| **b != 0)
            .map(|b| if b.is_ascii() { char::from(*b) } else { '?' })
            .collect();
        let key = (owner, name);
        let full = names.len() >= 65536;
        if let std::collections::btree_map::Entry::Vacant(entry) = names.entry(key) {
            if full {
                return Err("named routine count limit".into());
            }
            entry.insert(RoutineKey {
                scope,
                id: chunk.key.id,
            });
        }
    }
    Ok(())
}

fn import_tuning(
    input: &ConversionObject<'_>,
    guid: u32,
    values: &mut BTreeMap<(u32, u16, u16), i16>,
    budget: &mut ImportBudget,
) -> Result<(), String> {
    let source = input.source.tuning;
    if source.has_semiglobal != input.semiglobal_owner.is_some() {
        return Err("resolved tuning semiglobal binding mismatch".into());
    }
    for (owner, cache) in [
        (guid, &source.private_cache),
        (
            input.semiglobal_owner.unwrap_or(guid),
            &source.semiglobal_cache,
        ),
        (0, &source.global_cache),
    ] {
        for (key, value) in cache {
            budget.map_entries::<(u32, u16, u16), i16>(1)?;
            let table = (key >> 16) as u16;
            let index = *key as u16;
            let value = source
                .replacements
                .get(&(i32::from(table), i32::from(index)))
                .unwrap_or(value)
                .value;
            insert_consistent(values, (owner, table, index), value, "tuning value")?;
        }
    }
    for ((table, index), value) in &source.replacements {
        budget.map_entries::<(u32, u16, u16), i16>(1)?;
        let table =
            u16::try_from(*table).map_err(|_| "replacement tuning table does not fit runtime")?;
        let index =
            u16::try_from(*index).map_err(|_| "replacement tuning index does not fit runtime")?;
        let owner = if table >= 8192 {
            input.semiglobal_owner.unwrap_or(guid)
        } else if table >= 4096 {
            guid
        } else {
            0
        };
        insert_consistent(values, (owner, table, index), value.value, "tuning value")?;
    }
    Ok(())
}

fn required_routine(store: &RoutineStore, owner: u32, id: u16) -> Result<RoutineKey, String> {
    store
        .resolve(owner, id)
        .ok_or_else(|| format!("missing routine {id} for code owner {owner:#010x}"))
}

fn verify_direct_calls(
    store: &RoutineStore,
    input: &ConversionObject<'_>,
    owner: u32,
    roots: impl Iterator<Item = RoutineKey>,
    budget: &mut ImportBudget,
) -> Result<(), String> {
    let mut pending = BTreeSet::new();
    for key in roots {
        if !pending.contains(&key) {
            budget.map_entries::<RoutineKey, ()>(2)?;
            pending.insert(key);
        }
    }
    let mut seen = BTreeSet::new();
    while let Some(key) = pending.pop_first() {
        if !seen.insert(key) {
            continue;
        }
        let source = match key.scope {
            RoutineScope::Private(guid) if guid == owner => Some(input.source.iff),
            RoutineScope::SemiGlobal(guid) if Some(guid) == input.semiglobal_owner => {
                input.source.semiglobal.as_ref().map(|s| s.iff)
            }
            RoutineScope::Global => input.source.global.as_ref().map(|s| s.iff),
            _ => None,
        };
        if source
            .and_then(|file| resource(file, *b"BHAV", key.id))
            .is_none()
        {
            return Err("required routine is absent from this object's effective scope".into());
        }
        let routine = store.get(key).ok_or("missing root routine")?;
        for instruction in routine.instructions() {
            if instruction.opcode >= 256 {
                let next = required_routine(store, owner, instruction.opcode)?;
                if !seen.contains(&next) && !pending.contains(&next) {
                    budget.map_entries::<RoutineKey, ()>(2)?;
                    pending.insert(next);
                }
            }
        }
    }
    Ok(())
}

fn decode_functions(chunk: &IffChunk, limits: &Limits) -> Result<Vec<(u16, u16)>, String> {
    let count = u32::from_le_bytes(
        chunk
            .data
            .get(12..16)
            .ok_or("OBJf truncated header")?
            .try_into()
            .unwrap(),
    ) as usize;
    if count > 256 || chunk.data.len() != 16 + count * 4 {
        return Err("OBJf runtime requires at most 256 functions and no trailing bytes".into());
    }
    let table = wonderland_legacy_formats::semantic::decode_objf(&chunk.data, limits)
        .map_err(|error| format!("OBJf: {error}"))?;
    Ok(table
        .functions
        .into_iter()
        .map(|entry| (entry.condition, entry.action))
        .collect())
}

fn generated_functions(object: &Objd) -> Vec<(u16, u16)> {
    // VMEntity.GenerateFunctionTable, source entries 0 through 32.
    let fields = [
        "BHAV_Init",
        "BHAV_MainID",
        "BHAV_Load",
        "BHAV_Cleanup",
        "BHAV_QueueSkipped",
        "BHAV_AllowIntersectionID",
        "BHAV_WallAdjacencyChanged",
        "BHAV_RoomChange",
        "BHAV_DynamicMultiTileUpdate",
        "BHAV_Place",
        "BHAV_Pickup",
        "BHAV_UserPlace",
        "BHAV_UserPickup",
        "BHAV_LevelInfo",
        "BHAV_ServingSurface",
        "BHAV_Portal",
        "BHAV_GardeningID",
        "BHAV_WashHandsID",
        "BHAV_PrepareFoodID",
        "BHAV_CookFoodID",
        "BHAV_PlaceSurfaceID",
        "BHAV_DisposeID",
        "BHAV_EatID",
        "BHAV_PickupFromSlotID",
        "BHAV_WashDishID",
        "BHAV_EatSurfaceID",
        "BHAV_SitID",
        "BHAV_StandID",
        "BHAV_Clean",
        "BHAV_Repair",
        "",
        "",
        "",
    ];
    fields
        .into_iter()
        .map(|field| (0, object.field(field).unwrap_or(0)))
        .collect()
}

#[allow(clippy::too_many_arguments)] // Source inheritance, locale and runtime resolution remain explicit.
fn import_interactions(
    input: &ConversionObject<'_>,
    object: &Objd,
    master: Option<&Objd>,
    store: &RoutineStore,
    locale: LocaleSelection,
    variant: TtabVariant,
    limits: &Limits,
    budget: &mut ImportBudget,
) -> Result<Option<Vec<ImportedInteraction>>, String> {
    let find = |id| {
        if let Some(chunk) = resource(input.source.iff, *b"TTAB", id) {
            return Some((input.source.iff, chunk, id));
        }
        let semi = input.source.semiglobal.as_ref()?.iff;
        resource(semi, *b"TTAB", id).map(|chunk| (semi, chunk, id))
    };
    // VMContext calls UseTreeTableOf(master), which preserves a child's own
    // existing table and otherwise selects the master's local/semiglobal TTAB.
    let selected = find(object.tree_table_id())
        .or_else(|| master.and_then(|master| find(master.tree_table_id())));
    let Some((file, chunk, id)) = selected else {
        return Ok(None);
    };
    decode_interaction_table(
        input,
        object,
        store,
        file,
        chunk,
        resource(file, *b"TTAs", id),
        locale,
        variant,
        limits,
        budget,
    )
    .map(Some)
}

#[allow(clippy::too_many_arguments)]
fn import_global_interactions(
    input: &ConversionObject<'_>,
    object: &Objd,
    store: &RoutineStore,
    locale: LocaleSelection,
    variant: TtabVariant,
    limits: &Limits,
    budget: &mut ImportBudget,
) -> Result<Vec<ImportedInteraction>, String> {
    let Some(global) = input.source.global.as_ref() else {
        return Ok(Vec::new());
    };
    // VMContext selects the first TTAB and first TTAs independently, in source
    // enumeration order. Their resource IDs need not match or equal one.
    let Some(table) = global.iff.chunks.iter().find(|c| c.key.kind == *b"TTAB") else {
        return Ok(Vec::new());
    };
    let labels = global.iff.chunks.iter().find(|c| c.key.kind == *b"TTAs");
    decode_interaction_table(
        input, object, store, global.iff, table, labels, locale, variant, limits, budget,
    )
}

#[allow(clippy::too_many_arguments)]
fn decode_interaction_table(
    input: &ConversionObject<'_>,
    object: &Objd,
    store: &RoutineStore,
    file: &IffFile,
    chunk: &IffChunk,
    labels_chunk: Option<&IffChunk>,
    locale: LocaleSelection,
    variant: TtabVariant,
    limits: &Limits,
    budget: &mut ImportBudget,
) -> Result<Vec<ImportedInteraction>, String> {
    // The legacy decoder's temporary duplicate-index set is not part of the
    // returned table. Reserve it before that decoder can allocate anything.
    let count = u16::from_le_bytes(
        chunk
            .data
            .get(..2)
            .ok_or("truncated TTAB count")?
            .try_into()
            .unwrap(),
    );
    budget.map_entries::<u32, ()>(usize::from(count))?;
    let table = decode_ttab_with_variant(&chunk.data, variant, &budget.limits(limits))
        .map_err(|e| e.to_string())?;
    budget.decoded_ttab(&table, chunk.data.len())?;
    // The bytes are authoritative. A cached semantic value or dialect may not
    // substitute altered permissions under the same effective resource hash.
    if let Some(resolved) = input
        .source
        .resolved
        .filter(|resolved| std::ptr::eq(file, &resolved.iff))
    {
        match resolved
            .semantic_resources
            .iter()
            .find(|s| s.key == chunk.key)
            .map(|s| &s.semantic)
        {
            Some(wonderland_legacy_formats::semantic::DecodedSemantic::Ttab(cached))
                if cached == &table => {}
            _ => return Err("resolved TTAB semantic identity missing".into()),
        }
    }
    let labels = labels_chunk
        .map(|chunk| decode_strings(&chunk.data, &budget.limits(limits)).map_err(|e| e.to_string()))
        .transpose()?;
    if let Some(labels) = &labels {
        budget.decoded_strings(labels)?;
    }
    budget.entries::<ImportedInteraction>(table.interactions.len())?;
    budget.map_entries::<u32, ()>(table.interactions.len())?;
    let mut output = Vec::with_capacity(table.interactions.len());
    let mut indices = BTreeSet::new();
    for interaction in table.interactions {
        if interaction.motives.len() > 16 {
            return Err("unsupported TTAB motive count".into());
        }
        budget.entries::<sim_core::avatars::advertisements::MotiveAdvertisement>(
            interaction.motives.len(),
        )?;
        if !indices.insert(interaction.string_index) {
            return Err("duplicate TTAIndex in interaction table".into());
        }
        let action = required_routine(store, object.guid(), interaction.action_function)?;
        let check = (interaction.test_function != 0)
            .then(|| required_routine(store, object.guid(), interaction.test_function))
            .transpose()?;
        let label = match &labels {
            Some(table) => {
                let value = &lookup_string(table, interaction.string_index as usize, locale)
                    .ok_or("missing TTAs string index")?
                    .item
                    .value;
                Some(budget.decode_text(value)?)
            }
            None => None,
        };
        output.push(ImportedInteraction {
            tta_index: interaction.string_index,
            action,
            check,
            code_owner: object.guid(),
            flags: interaction.flags,
            permissions: interaction.flags2,
            label,
            advertisement: InteractionAdvertisement {
                motives: interaction
                    .motives
                    .iter()
                    .enumerate()
                    .map(|(index, motive)| {
                        Ok(sim_core::avatars::advertisements::MotiveAdvertisement {
                            motive: *sim_core::avatars::motives::ALL_MOTIVES
                                .get(index)
                                .ok_or("unsupported TTAB motive index")?,
                            minimum: motive.minimum,
                            delta: motive.delta,
                            personality_modifier: motive.personality_modifier,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?,
                attenuation_code: interaction.attenuation_code,
                attenuation_value_bits: interaction.attenuation_value_bits,
                autonomy_threshold: interaction.autonomy_threshold,
                joining_index: interaction.joining_index,
            },
        });
    }
    Ok(output)
}

#[cfg(test)]
mod allocation_tests {
    use super::*;

    #[test]
    fn scope_hash_admits_empty_chunk_headers_and_duplicate_key_workspace() {
        let mut header = [0; 64];
        let signature = b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE JAMIE DOORNBOS & MAXIS 1";
        header[..signature.len()].copy_from_slice(signature);
        let file = IffFile {
            header,
            chunks: (0..1000)
                .map(|id| IffChunk {
                    key: ChunkKey { kind: *b"ZZZZ", id },
                    flags: 0,
                    label: [0; 64],
                    data: vec![],
                })
                .collect(),
        };
        let limits = Limits {
            max_total_decoded_bytes: 32 * 1024,
            ..Limits::default()
        };
        let mut budget = ImportBudget::new(&limits);
        assert!(scoped_hash(&file, &limits, &mut budget)
            .unwrap_err()
            .contains("scope hash workspace"));
        let generous = Limits::default();
        let actual = scoped_hash(&file, &generous, &mut ImportBudget::new(&generous)).unwrap();
        assert_eq!(actual, iff_sha256(&file, &generous).unwrap());
    }

    #[test]
    fn validation_workspace_matches_actual_content_and_rejects_insufficient_budget() {
        let store = RoutineStore::new();
        let definitions = vec![ObjectDefinition::new(123, 4)];
        let animations = vec![(
            AnimationKey {
                owner: 123,
                scope: 0,
                id: 1,
            },
            AnimationMetadata {
                resource: "EXAMPLE.anim".into(),
                num_frames: 3,
                time_properties: vec![],
            },
        )];
        let slots: BTreeMap<(u32, u16), RoutingSlot> = BTreeMap::new();
        let strings = BTreeMap::from([((123u32, 256u16), vec!["label".to_string()])]);
        let names: BTreeMap<(u32, String), RoutineKey> = BTreeMap::new();
        let tuning = TuningSet::default();
        let projected = (
            &store,
            &definitions,
            &animations,
            &slots,
            &strings,
            &names,
            &tuning,
        );
        let size = validation_buffer_size(&projected, definitions.len()).unwrap();
        let actual = ContentSet::new(
            store.clone(),
            definitions.clone(),
            animations.clone(),
            tuning.clone(),
        )
        .unwrap()
        .with_strings(strings.clone().into_iter().collect())
        .unwrap()
        .with_named_trees(vec![])
        .unwrap()
        .with_routing_slots(vec![])
        .unwrap();
        let encoded = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_little_endian()
            .serialize(&actual)
            .unwrap();
        assert_eq!(size, encoded.len());
        let limits = Limits {
            max_total_decoded_bytes: size - 1,
            ..Limits::default()
        };
        assert!(reserve_validation_workspace(
            &mut ImportBudget::new(&limits),
            &projected,
            1,
            &animations
        )
        .is_err());
    }
}
