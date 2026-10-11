//! F-owned *static* resource preflight, not a replacement content loader or VM.
//! Uses the shipping IFF/OBJD/OBJf/TTAB/BHAV decoders, scope lookup and BHAV
//! validator. A closed static graph does not qualify dynamic primitives, source
//! installation/patches, runtime metadata, rights, rendering or placed gameplay.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use wonderland_content_ir::objects::{ResourceNamespace, ResourceScope};
use wonderland_content_runtime_bridge::import_bhav;
use wonderland_legacy_formats::{
    Limits,
    iff::{self, IffFile},
    semantic::{OBJD_FIELDS, decode_glob, decode_objd, decode_objf, decode_ttab},
};

const MAX_NODES: usize = 4096;
const MAX_EDGES: usize = 32768;

pub fn limits() -> Limits {
    Limits {
        max_input_bytes: 8 * 1024 * 1024,
        max_resource_bytes: 4 * 1024 * 1024,
        max_entries: 8192,
        max_total_decoded_bytes: 16 * 1024 * 1024,
        max_string_bytes: 65536,
        ..Limits::default()
    }
}
fn require(ok: bool, message: impl Into<String>) -> Result<(), String> {
    if ok { Ok(()) } else { Err(message.into()) }
}
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Debug)]
pub struct Resource {
    pub name: String,
    pub sha256: String,
    pub file: IffFile,
}
impl Resource {
    pub fn decode(name: &str, bytes: &[u8], expected_sha256: &str) -> Result<Self, String> {
        require(
            !name.is_empty() && name.len() <= 1024 && !name.chars().any(char::is_control),
            "Invalid resource label",
        )?;
        require(
            expected_sha256.len() == 64
                && expected_sha256
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
            "Invalid resource digest",
        )?;
        require(
            bytes.len() <= limits().max_input_bytes,
            "Resource byte limit",
        )?;
        require(
            hash(bytes) == expected_sha256,
            format!("Source digest changed: {name}"),
        )?;
        let file = iff::decode(bytes, &limits()).map_err(|e| format!("{name}: {e}"))?;
        Ok(Self {
            name: name.into(),
            sha256: expected_sha256.into(),
            file,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ObjectRecord {
    pub chunk_id: u16,
    pub guid: u32,
    pub master_id: u16,
    pub sub_index: i16,
    pub declared_attributes: u16,
    pub uses_function_table: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Edge {
    pub from: String,
    pub instruction: usize,
    pub to: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Audit {
    pub schema: u8,
    pub scope: &'static str,
    pub source_name: String,
    pub source_sha256: String,
    pub semiglobal_required: Option<String>,
    pub supplied_scopes: BTreeMap<String, String>,
    pub objects: Vec<ObjectRecord>,
    pub declared_roots: Vec<String>,
    pub static_complete: bool,
    pub missing: Vec<String>,
    pub visited: Vec<String>,
    pub edges: Vec<Edge>,
    pub primitive_counts: BTreeMap<u16, usize>,
    pub whole_object_runtime_qualified: bool,
    pub unassessed: Vec<&'static str>,
}
fn namespace(id: u16) -> ResourceNamespace {
    if id >= 8192 {
        ResourceNamespace::Semiglobal
    } else if id >= 4096 {
        ResourceNamespace::Private
    } else {
        ResourceNamespace::Global
    }
}
fn label(scope: ResourceNamespace, id: u16) -> String {
    format!(
        "{}:{id}",
        match scope {
            ResourceNamespace::Private => "private",
            ResourceNamespace::Semiglobal => "semiglobal",
            ResourceNamespace::Global => "global",
        }
    )
}
fn scope_from_key(key: &str) -> ResourceNamespace {
    match key {
        "private" => ResourceNamespace::Private,
        "semiglobal" => ResourceNamespace::Semiglobal,
        _ => ResourceNamespace::Global,
    }
}
fn key(id: u16) -> (String, u16) {
    let text = label(namespace(id), id);
    (text.split_once(':').expect("controlled label").0.into(), id)
}
fn basename(name: &str) -> &str {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let split = base.len().saturating_sub(4);
    if base
        .get(split..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(".iff"))
    {
        base.get(..split).expect("ASCII suffix boundary")
    } else {
        base
    }
}
fn validate_file(resource: &Resource) -> Result<(), String> {
    require(
        resource.file.chunks.len() <= limits().max_entries,
        "Resource entry limit",
    )?;
    let mut keys = BTreeSet::new();
    let mut total = 0usize;
    for chunk in &resource.file.chunks {
        require(
            chunk.key.kind != *b"PIFF",
            "Unapplied patch is not an effective base resource",
        )?;
        require(
            keys.insert(chunk.key),
            format!("Ambiguous duplicate resource in {}", resource.name),
        )?;
        require(
            chunk.data.len() <= limits().max_resource_bytes,
            "Resource chunk limit",
        )?;
        total = total
            .checked_add(chunk.data.len())
            .ok_or("Resource length overflow")?;
        require(
            total <= limits().max_total_decoded_bytes,
            "Resource aggregate limit",
        )?;
    }
    Ok(())
}

pub fn inspect(
    source: &Resource,
    semi: Option<&Resource>,
    global: Option<&Resource>,
) -> Result<Audit, String> {
    validate_file(source)?;
    for resource in [semi, global].into_iter().flatten() {
        validate_file(resource)?;
    }
    let files = ResourceScope {
        private: &source.file,
        semiglobal: semi.map(|s| &s.file),
        global: global.map(|s| &s.file),
    };
    let globs: Vec<_> = source
        .file
        .chunks
        .iter()
        .filter(|c| c.key.kind == *b"GLOB")
        .collect();
    require(
        globs.len() <= 1,
        "Multiple GLOB declarations require explicit source selection",
    )?;
    let wanted = globs
        .first()
        .map(|c| {
            decode_glob(&c.data, &limits())
                .map(|g| g.name.text())
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    if let Some(name) = &wanted {
        require(
            !name.is_empty() && !name.chars().any(char::is_control),
            "Empty/invalid semiglobal name",
        )?;
    }
    if let Some(supplied) = semi {
        require(
            wanted
                .as_ref()
                .is_some_and(|w| basename(&supplied.name).eq_ignore_ascii_case(w)),
            "Supplied semiglobal does not match the original GLOB",
        )?;
    }
    let mut pending = BTreeSet::<(String, u16)>::new();
    let mut missing = BTreeSet::<String>::new();
    if semi.is_none()
        && let Some(name) = &wanted
    {
        missing.insert(format!("semiglobal resource: {name}"));
    }
    let mut objects = Vec::new();
    let mut guids = BTreeSet::new();
    for chunk in source.file.chunks.iter().filter(|c| c.key.kind == *b"OBJD") {
        let o = decode_objd(&chunk.data, &limits())
            .map_err(|e| format!("OBJD:{}: {e}", chunk.key.id))?;
        require(
            o.guid() != 0 && guids.insert(o.guid()),
            "Zero or duplicated object GUID",
        )?;
        require(objects.len() < 1024, "Object cohort limit")?;
        let uses = o.field("UsesFnTable").unwrap_or(0) != 0;
        objects.push(ObjectRecord {
            chunk_id: chunk.key.id,
            guid: o.guid(),
            master_id: o.master_id(),
            sub_index: o.sub_index(),
            declared_attributes: o.field("NumAttributes").unwrap_or(0),
            uses_function_table: uses,
        });
        if uses {
            match files.lookup(ResourceNamespace::Private, *b"OBJf", chunk.key.id) {
                None => {
                    missing.insert(format!(
                        "OBJf:{} required by OBJD:{}",
                        chunk.key.id, chunk.key.id
                    ));
                }
                Some(c) => {
                    let f = decode_objf(&c.data, &limits()).map_err(|e| e.to_string())?;
                    require(
                        f.functions.len() <= 256 && f.trailing.is_empty(),
                        "OBJf outside runtime table bounds",
                    )?;
                    for entry in f.functions {
                        for id in [entry.condition, entry.action] {
                            if id != 0 {
                                pending.insert(key(id));
                            }
                        }
                    }
                }
            }
        } else {
            // Deliberately conservative union of declared BHAV fields, NOT a
            // second VMEntity.GenerateFunctionTable or its special GUID overrides.
            for field in OBJD_FIELDS
                .iter()
                .filter(|field| field.starts_with("BHAV_"))
            {
                if let Some(id) = o.field(field).filter(|id| *id != 0) {
                    pending.insert(key(id));
                }
            }
        }
    }
    require(
        !objects.is_empty(),
        "No original object definitions in primary resource",
    )?;
    for object in &objects {
        if object.master_id != 0 && object.sub_index != -1 {
            let matches = objects
                .iter()
                .filter(|m| m.master_id == object.master_id && m.sub_index == -1)
                .count();
            if matches != 1 {
                missing.insert(format!(
                    "unique master OBJD for group {} (child {})",
                    object.master_id, object.chunk_id
                ));
            }
        }
    }
    // Every private routine is a conservative root (including currently unused
    // branches), matching the bridge's strict local routine import expectation.
    for chunk in source.file.chunks.iter().filter(|c| c.key.kind == *b"BHAV") {
        pending.insert(("private".into(), chunk.key.id));
    }
    // Local TTAB declarations all contribute; this is a conservative superset,
    // not permission, hidden-flag or autonomous menu evaluation.
    let tables = source
        .file
        .chunks
        .iter()
        .filter(|c| c.key.kind == *b"TTAB")
        .chain(
            semi.into_iter()
                .flat_map(|s| s.file.chunks.iter().filter(|c| c.key.kind == *b"TTAB")),
        )
        .chain(
            global
                .into_iter()
                .filter_map(|g| g.file.chunks.iter().find(|c| c.key.kind == *b"TTAB")),
        );
    for chunk in tables {
        let table = decode_ttab(&chunk.data, &limits())
            .map_err(|e| format!("TTAB:{}: {e}", chunk.key.id))?;
        for interaction in table.interactions {
            for id in [interaction.action_function, interaction.test_function] {
                if id != 0 {
                    pending.insert(key(id));
                }
            }
        }
    }
    require(pending.len() <= MAX_NODES, "Root count limit")?;
    let declared_roots = pending.iter().map(|(s, id)| format!("{s}:{id}")).collect();
    let mut seen = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut edges = Vec::new();
    let mut primitives = BTreeMap::new();
    while let Some((ns, id)) = pending.pop_first() {
        if !seen.insert((ns.clone(), id)) {
            continue;
        }
        require(seen.len() <= MAX_NODES, "Static graph node limit")?;
        let from = format!("{ns}:{id}");
        let Some(chunk) = files.lookup(scope_from_key(&ns), *b"BHAV", id) else {
            missing.insert(from);
            continue;
        };
        let routine = import_bhav(chunk, &limits()).map_err(|e| format!("{from}: {e}"))?;
        visited.insert(from.clone());
        for (index, instruction) in routine.instructions().iter().enumerate() {
            if instruction.opcode >= 256 {
                require(edges.len() < MAX_EDGES, "Static call edge limit")?;
                let to = key(instruction.opcode);
                let target = label(namespace(instruction.opcode), instruction.opcode);
                if files.lookup_bhav(instruction.opcode).is_none() {
                    missing.insert(format!("{target} required by {from} instruction {index}"));
                }
                edges.push(Edge {
                    from: from.clone(),
                    instruction: index,
                    to: target,
                });
                if !seen.contains(&to) {
                    pending.insert(to);
                }
            } else {
                *primitives.entry(instruction.opcode).or_insert(0) += 1;
            }
        }
    }
    objects.sort_by_key(|o| o.chunk_id);
    let mut supplied_scopes = BTreeMap::from([("private".into(), source.sha256.clone())]);
    if let Some(s) = semi {
        supplied_scopes.insert("semiglobal".into(), s.sha256.clone());
    }
    if let Some(s) = global {
        supplied_scopes.insert("global".into(), s.sha256.clone());
    }
    Ok(Audit {
        schema: 1,
        scope: "conservative-static-resource-preflight",
        source_name: source.name.clone(),
        source_sha256: source.sha256.clone(),
        semiglobal_required: wanted,
        supplied_scopes,
        objects,
        declared_roots,
        static_complete: missing.is_empty(),
        missing: missing.into_iter().collect(),
        visited: visited.into_iter().collect(),
        edges,
        primitive_counts: primitives,
        whole_object_runtime_qualified: false,
        unassessed: vec![
            "content installation, patch ordering and tuning",
            "dynamic tree/entity/object dispatch and provider results",
            "normalized source footprints, placement, routing and animation metadata",
            "source permissions, full object behavior and external effects",
            "asset rights and release qualification",
        ],
    })
}
