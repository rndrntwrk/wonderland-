// This Source Code Form is subject to the Mozilla Public License, v. 2.0.
// https://mozilla.org/MPL/2.0/
//! Read-only metadata census. Original asset bytes are never written to output.
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    error::Error as StdError,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use wonderland_content_ir::objects::sha256_hex;
use wonderland_legacy_formats::{
    iff::{self, ChunkKey, IffChunk, IffDocument, IffFile},
    reader::Reader,
    semantic::{
        decode_otf, decode_piff, decode_semantic, DecodedSemantic, PiffOperation, OBJD_FIELDS,
    },
    Error, ErrorKind, Limits, Result,
};

const SOURCE_BASELINE: &str = "4c6b3e8f5835b228723caea3c9f683c62f244f73";

#[derive(Default, Serialize)]
struct Summary {
    files: usize,
    iff_piff_files: usize,
    xml_otf_files: usize,
    excluded_font_files: usize,
    strict_envelopes_accepted: usize,
    strict_duplicate_envelopes: usize,
    invalid_envelopes: usize,
    indexed_files: usize,
    indexed_passthrough_verified: usize,
    semantic_counts: BTreeMap<String, usize>,
    opaque_counts: BTreeMap<String, usize>,
    semantic_failures: BTreeMap<String, usize>,
    bounded_retry_successes: BTreeMap<String, usize>,
    objects: usize,
    behaviors: usize,
    interaction_tables: usize,
}

fn count(map: &mut BTreeMap<String, usize>, kind: &str) {
    *map.entry(kind.to_owned()).or_default() += 1;
}
fn error_value(error: &Error) -> Value {
    json!({"kind":format!("{:?}",error.kind),"offset":error.offset,"context":error.context})
}
fn hex(bytes: &[u8]) -> String {
    sha256_hex(&Sha256::digest(bytes).into())
}
fn label(chunk: &IffChunk) -> String {
    let end = chunk
        .label
        .iter()
        .rposition(|b| *b != 0)
        .map_or(0, |i| i + 1);
    chunk.label[..end]
        .iter()
        .map(|&b| if b < 128 { char::from(b) } else { '?' })
        .collect()
}

/// C# IffFile.AddChunk appends duplicate known chunks to ByChunkType and keeps
/// only the first key in ByChunkId. The production decoder rejects ambiguity.
/// This census-only scan retains every bounded record after a Duplicate error.
fn scan_duplicate_envelope(bytes: &[u8], limits: &Limits) -> Result<IffFile> {
    limits.check_input(bytes)?;
    let mut r = Reader::new(bytes);
    let mut header = [0; 64];
    header.copy_from_slice(r.read_bytes(64)?);
    let mut chunks = Vec::new();
    let mut total = 0usize;
    while r.remaining() > 0 {
        limits.check_count(
            chunks.len() + 1,
            limits.max_entries,
            r.position(),
            "census IFF records",
        )?;
        let kind: [u8; 4] = r
            .read_bytes(4)?
            .try_into()
            .map_err(|_| Error::new(ErrorKind::InvalidData, r.position(), "census IFF type"))?;
        let size = (r.u32_be()? as usize).checked_sub(76).ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, r.position(), "census short chunk")
        })?;
        limits.check_count(
            size,
            limits.max_resource_bytes,
            r.position(),
            "census chunk bytes",
        )?;
        total = total
            .checked_add(size)
            .ok_or_else(|| Error::new(ErrorKind::Overflow, r.position(), "census decoded total"))?;
        limits.check_count(
            total,
            limits.max_total_decoded_bytes,
            r.position(),
            "census decoded total",
        )?;
        let id = r.u16_be()?;
        let flags = r.u16_be()?;
        let mut label = [0; 64];
        label.copy_from_slice(r.read_bytes(64)?);
        chunks.push(IffChunk {
            key: ChunkKey { kind, id },
            flags,
            label,
            data: r.read_bytes(size)?.to_vec(),
        });
    }
    Ok(IffFile { header, chunks })
}

fn probe(
    path: &str,
    bytes: &[u8],
    limits: &Limits,
    retry_limits: &Limits,
    summary: &mut Summary,
) -> Value {
    summary.files += 1;
    let mut record = json!({"path":path,"source_sha256":hex(bytes),"byte_len":bytes.len(),"objects":[],"behaviors":[],"interactions":[],"errors":[]});
    let row = record.as_object_mut().expect("record");
    if path.to_ascii_lowercase().ends_with(".otf") {
        if bytes.starts_with(b"OTTO") || bytes.starts_with(&[0, 1, 0, 0]) {
            summary.excluded_font_files += 1;
            row.insert("format".into(), json!("OpenType font"));
            row.insert(
                "envelope_status".into(),
                json!("excluded: extension is not an object tuning file"),
            );
            return record;
        }
        summary.xml_otf_files += 1;
        row.insert("format".into(), json!("XML OTF"));
        match decode_otf(bytes, limits) {
            Ok(otf) => {
                count(&mut summary.semantic_counts, "OTF");
                row.insert("envelope_status".into(), json!("accepted"));
                row.insert(
                    "tuning_tables".into(),
                    json!(otf
                        .tables
                        .iter()
                        .map(|t| json!({"id":t.id,"name":t.name,"key_count":t.keys.len()}))
                        .collect::<Vec<_>>()),
                );
            }
            Err(error) => {
                count(&mut summary.semantic_failures, "OTF");
                row.insert("errors".into(), json!([error_value(&error)]));
                row.insert("envelope_status".into(), json!("rejected"));
            }
        }
        return record;
    }
    summary.iff_piff_files += 1;
    row.insert(
        "format".into(),
        json!(if path.to_ascii_lowercase().ends_with(".piff") {
            "PIFF envelope"
        } else {
            "IFF envelope"
        }),
    );
    let mut duplicate_mode = false;
    let file = match iff::decode(bytes, limits) {
        Ok(file) => {
            summary.strict_envelopes_accepted += 1;
            row.insert("envelope_status".into(), json!("accepted"));
            file
        }
        Err(error) if error.kind == ErrorKind::Duplicate => {
            row.insert("strict_error".into(), error_value(&error));
            match scan_duplicate_envelope(bytes, limits) {
                Ok(file) => {
                    summary.strict_duplicate_envelopes += 1;
                    duplicate_mode = true;
                    row.insert(
                        "envelope_status".into(),
                        json!("source duplicates retained for census; strict decoder excludes"),
                    );
                    file
                }
                Err(error) => {
                    summary.invalid_envelopes += 1;
                    row.insert("envelope_status".into(), json!("rejected"));
                    row.insert("errors".into(), json!([error_value(&error)]));
                    return record;
                }
            }
        }
        Err(error) => {
            summary.invalid_envelopes += 1;
            row.insert("envelope_status".into(), json!("rejected"));
            row.insert("errors".into(), json!([error_value(&error)]));
            return record;
        }
    };
    let map_offset = u32::from_be_bytes(file.header[60..64].try_into().expect("header map"));
    let indexed = map_offset != 0 || file.chunks.iter().any(|c| c.key.kind == *b"rsmp");
    row.insert("indexed".into(), json!(indexed));
    row.insert("resource_map_offset".into(), json!(map_offset));
    row.insert("chunk_count".into(), json!(file.chunks.len()));
    if indexed {
        summary.indexed_files += 1;
    }
    if duplicate_mode {
        row.insert(
            "roundtrip_policy".into(),
            json!("strict IffDocument rejects duplicates; source bytes hashed read-only"),
        );
    } else if indexed {
        match IffDocument::decode(bytes, limits).and_then(|document| document.encode(limits)) {
            Ok(encoded) if encoded == bytes => {
                summary.indexed_passthrough_verified += 1;
                row.insert(
                    "roundtrip_policy".into(),
                    json!("IffDocument byte-exact passthrough verified; structural edits rejected"),
                );
            }
            Ok(_) => {
                row.insert(
                    "roundtrip_policy".into(),
                    json!("ERROR: indexed passthrough changed bytes"),
                );
            }
            Err(error) => {
                row.insert(
                    "roundtrip_policy".into(),
                    json!({"error":error_value(&error)}),
                );
            }
        }
    } else {
        row.insert(
            "roundtrip_policy".into(),
            json!(match iff::encode(&file, limits) {
                Ok(encoded) if encoded == bytes => "editable byte-exact envelope verified",
                _ => "ERROR: editable envelope roundtrip failed",
            }),
        );
    }
    let mut resources = BTreeMap::new();
    let mut semantics = BTreeMap::new();
    let mut opaque = BTreeMap::new();
    let mut versions = BTreeMap::new();
    let mut keys = BTreeMap::new();
    let mut objects = Vec::new();
    let mut behaviors = Vec::new();
    let mut interactions = Vec::new();
    let mut errors = Vec::new();
    let mut piffs = Vec::new();
    let mut globals = Vec::new();
    let mut function_tables = Vec::new();
    for (ordinal, chunk) in file.chunks.iter().enumerate() {
        let kind = String::from_utf8_lossy(&chunk.key.kind).into_owned();
        count(&mut resources, &kind);
        *keys.entry(chunk.key).or_insert(0usize) += 1;
        let decoded = match decode_semantic(chunk, limits) {
            Ok(value) => value,
            Err(error) => {
                count(&mut summary.semantic_failures, &kind);
                let mut failure = json!({"kind":kind,"chunk_id":chunk.key.id,"resource_ordinal":ordinal,"error":error_value(&error)});
                if chunk.key.kind == *b"PIFF" && error.kind == ErrorKind::LimitExceeded {
                    match decode_piff(&chunk.data, retry_limits) {
                        Ok(piff) => {
                            count(&mut summary.bounded_retry_successes, &kind);
                            let operations: usize = piff
                                .entries
                                .iter()
                                .map(|e| match &e.operation {
                                    PiffOperation::Patch { patches, .. } => patches.len(),
                                    _ => 0,
                                })
                                .sum();
                            failure["bounded_retry"] = json!({"status":"accepted","max_entries":retry_limits.max_entries,"version":piff.version,"entries":piff.entries.len(),"patch_operations":operations});
                        }
                        Err(error) => {
                            failure["bounded_retry"] =
                                json!({"status":"rejected","error":error_value(&error)})
                        }
                    }
                }
                errors.push(failure);
                continue;
            }
        };
        if let DecodedSemantic::Unknown { .. } = decoded {
            count(&mut opaque, &kind);
            count(&mut summary.opaque_counts, &kind);
            continue;
        }
        count(&mut semantics, &kind);
        count(&mut summary.semantic_counts, &kind);
        match decoded {
            DecodedSemantic::Objf(table) => {
                count(&mut versions, &format!("OBJf:{}", table.version));
                function_tables.push(json!({
                    "chunk_id": chunk.key.id, "resource_ordinal": ordinal,
                    "version": table.version, "padding": table.padding,
                    "trailing_bytes": table.trailing.len(),
                    "entries": table.functions.iter().map(|entry| json!({
                        "condition_function": entry.condition,
                        "action_function": entry.action,
                    })).collect::<Vec<_>>()
                }));
            }
            DecodedSemantic::Objd(obj) => {
                summary.objects += 1;
                count(&mut versions, &format!("OBJD:{}", obj.version));
                let fields = json!({
                    "object_type":obj.object_type(),"master_id":obj.master_id(),"sub_index":obj.sub_index(),
                    "tree_table_id":obj.tree_table_id(),"slot_id":obj.slot_id(),"catalog_strings_id":obj.catalog_strings_id(),
                    "room_flags":obj.field("RoomFlags"),"function_flags":obj.field("FunctionFlags"),"global":obj.field("Global"),
                    "uses_fn_table":obj.field("UsesFnTable"),"bit_field_1":obj.field("BitField1"),"body_string_id":obj.field("BodyStringID"),
                    "num_attributes":obj.field("NumAttributes"),"num_type_attributes":obj.field("NumTypeAttributes"),
                    "base_graphic_id":obj.field("BaseGraphicID"),"num_graphics":obj.field("NumGraphics"),
                    "dynamic_sprite_base_id":obj.field("DynamicSpriteBaseId"),"num_dynamic_sprites":obj.field("NumDynamicSprites"),
                    "function_subsort":obj.field("FunctionSubsort"),"chair_entry_flags":obj.field("ChairEntryFlags"),
                    "rating_hunger":obj.field("RatingHunger").map(|v|v as i16),"rating_comfort":obj.field("RatingComfort").map(|v|v as i16),
                    "rating_hygiene":obj.field("RatingHygiene").map(|v|v as i16),"rating_bladder":obj.field("RatingBladder").map(|v|v as i16),
                    "rating_energy":obj.field("RatingEnergy").map(|v|v as i16),"rating_fun":obj.field("RatingFun").map(|v|v as i16),
                    "rating_room":obj.field("RatingRoom").map(|v|v as i16),"rating_skill_flags":obj.field("RatingSkillFlags")
                });
                let refs: Vec<_> = OBJD_FIELDS
                    .iter()
                    .filter(|name| name.starts_with("BHAV_"))
                    .filter_map(|name| {
                        obj.field(name)
                            .filter(|id| *id != 0)
                            .map(|id| json!({"field":name,"id":id}))
                    })
                    .collect();
                objects.push(json!({"chunk_id":chunk.key.id,"resource_ordinal":ordinal,"guid":obj.guid(),"guid_hex":format!("{:08x}",obj.guid()),"version":obj.version,"label":label(chunk),"chunk_sha256":hex(&chunk.data),"raw_fields":fields,"bhav_refs":refs,"family":"unknown","family_evidence":null,"effective_content_id":null}));
            }
            DecodedSemantic::Bhav(bhav) => {
                summary.behaviors += 1;
                count(&mut versions, &format!("BHAV:{:04x}", bhav.format_version));
                let mut opcodes: BTreeMap<u16, usize> = BTreeMap::new();
                for instruction in &bhav.instructions {
                    *opcodes.entry(instruction.opcode).or_default() += 1;
                }
                behaviors.push(json!({"chunk_id":chunk.key.id,"resource_ordinal":ordinal,"label":label(chunk),"version":bhav.format_version,"kind":bhav.kind,"args":bhav.args,"locals":bhav.locals,"tree_version":bhav.tree_version,"instruction_count":bhav.instructions.len(),"opcode_counts":opcodes.iter().map(|(opcode,count)|json!({"opcode":opcode,"count":count})).collect::<Vec<_>>(),"direct_calls":opcodes.keys().filter(|id| **id >= 256).collect::<Vec<_>>()}));
            }
            DecodedSemantic::Ttab(ttab) => {
                summary.interaction_tables += 1;
                count(
                    &mut versions,
                    &format!(
                        "TTAB:{:?}:compression={:?}",
                        ttab.version, ttab.compression_code
                    ),
                );
                interactions.push(json!({"chunk_id":chunk.key.id,"resource_ordinal":ordinal,"version":ttab.version,"compression_code":ttab.compression_code,"entries":ttab.interactions.iter().map(|i|json!({"action_function":i.action_function,"test_function":i.test_function,"string_index":i.string_index,"flags":i.flags,"flags2":i.flags2,"autonomy_threshold":i.autonomy_threshold,"joining_index":i.joining_index,"motive_count":i.motives.len()})).collect::<Vec<_>>()}));
            }
            DecodedSemantic::Strings(strings) => {
                count(&mut versions, &format!("{kind}:{}", strings.format))
            }
            DecodedSemantic::Slot(slot) => count(&mut versions, &format!("SLOT:{}", slot.version)),
            DecodedSemantic::Glob(glob) => globals.push(
                json!({"chunk_id":chunk.key.id,"resource_ordinal":ordinal,"name":glob.name.text()}),
            ),
            DecodedSemantic::Piff(piff) => {
                count(&mut versions, &format!("PIFF:{}", piff.version));
                let operations: usize = piff
                    .entries
                    .iter()
                    .map(|e| match &e.operation {
                        PiffOperation::Patch { patches, .. } => patches.len(),
                        _ => 0,
                    })
                    .sum();
                piffs.push(json!({"chunk_id":chunk.key.id,"version":piff.version,"source_name":piff.source.text(),"entries":piff.entries.len(),"patch_operations":operations,"is_user_category":path.replace('\\',"/").contains("User/")}));
            }
            _ => {}
        }
    }
    let duplicates: Vec<_> = keys
        .into_iter()
        .filter(|(_, n)| *n > 1)
        .map(|(key, n)| json!({"kind":String::from_utf8_lossy(&key.kind),"id":key.id,"count":n}))
        .collect();
    row.insert("resource_counts".into(), json!(resources));
    row.insert("semantic_counts".into(), json!(semantics));
    row.insert("opaque_counts".into(), json!(opaque));
    row.insert("versions".into(), json!(versions));
    row.insert("duplicate_keys".into(), json!(duplicates));
    row.insert("objects".into(), json!(objects));
    row.insert("behaviors".into(), json!(behaviors));
    row.insert("interactions".into(), json!(interactions));
    row.insert("piffs".into(), json!(piffs));
    row.insert("errors".into(), json!(errors));
    row.insert("globals".into(), json!(globals));
    row.insert("function_tables".into(), json!(function_tables));
    row.insert("resources".into(), json!(file.chunks.iter().enumerate().map(|(ordinal,chunk)|json!({"kind":String::from_utf8_lossy(&chunk.key.kind),"id":chunk.key.id,"resource_ordinal":ordinal,"byte_len":chunk.data.len()})).collect::<Vec<_>>()));
    record
}

fn main() -> std::result::Result<(), Box<dyn StdError>> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().unwrap_or_else(|| ".".to_owned())).canonicalize()?;
    let output = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "docs/compat/content-corpus.json".to_owned()),
    );
    if args.next().is_some() {
        return Err("usage: content-census [repository-root] [output.json]".into());
    }
    if output.extension().and_then(|x| x.to_str()) != Some("json") {
        return Err("census output must be a JSON metadata file".into());
    }
    let output = if output.is_absolute() {
        output
    } else {
        root.join(output)
    };
    let command = Command::new("git")
        .args(["ls-tree", "-r", "-l", "-z", SOURCE_BASELINE])
        .current_dir(&root)
        .output()?;
    if !command.status.success() {
        return Err("pinned source tree enumeration failed".into());
    }
    let listing = String::from_utf8(command.stdout)?;
    let mut paths = Vec::new();
    for entry in listing.split('\0').filter(|line| !line.is_empty()) {
        let (metadata, path) = entry.split_once('\t').ok_or("invalid source tree record")?;
        let suffix = path.to_ascii_lowercase();
        if !suffix.ends_with(".iff") && !suffix.ends_with(".piff") && !suffix.ends_with(".otf") {
            continue;
        }
        let fields: Vec<_> = metadata.split_whitespace().collect();
        if fields.len() != 4 || fields[1] != "blob" {
            return Err("asset tree record is not a blob".into());
        }
        paths.push((path, fields[2], fields[3].parse::<u64>()?));
    }
    paths.sort_unstable_by_key(|(path, _, _)| *path);
    let limits = Limits::default();
    limits.check_count(paths.len(), limits.max_entries, 0, "corpus file count")?;
    let retry_limits = Limits {
        max_entries: 1_000_000,
        ..limits
    };
    let mut summary = Summary::default();
    let mut files = Vec::with_capacity(paths.len());
    for (path, blob, size) in paths {
        if size > limits.max_input_bytes as u64 {
            return Err(format!("source file exceeds input limit: {path}").into());
        }
        let object = Command::new("git")
            .args(["cat-file", "blob", blob])
            .current_dir(&root)
            .output()?;
        if !object.status.success() || object.stdout.len() as u64 != size {
            return Err(format!("pinned source blob read failed: {path}").into());
        }
        let mut result = probe(path, &object.stdout, &limits, &retry_limits, &mut summary);
        result["source_git_blob"] = json!(blob);
        files.push(result);
    }
    let document = json!({
        "schema_version":1,"source_baseline":SOURCE_BASELINE,"enumeration":"git ls-tree and cat-file at source_baseline; working-tree additions excluded","read_only_original_assets":true,
        "rights_statement":"Metadata evidence only. Presence in the source checkout does not establish redistribution rights.",
        "effective_identity_statement":"All object effective_content_id values remain null until an explicit ordered source/patch/tuning/dependency manifest is resolved.",
        "default_limits":limits,"retry_limits":retry_limits,"summary":summary,"files":files
    });
    let mut encoded = Vec::new();
    serde_json::to_writer(&mut encoded, &document)?;
    encoded.push(b'\n');
    if encoded.len() > 128 * 1024 * 1024 {
        return Err("census metadata output limit exceeded".into());
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = output.with_extension("json.tmp");
    fs::write(&temporary, encoded)?;
    fs::rename(temporary, &output)?;
    println!("{}", serde_json::to_string_pretty(&document["summary"])?);
    println!("metadata output: {}", display_relative(&output, &root));
    Ok(())
}

fn display_relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}
