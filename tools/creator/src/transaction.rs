// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, obtain one at https://mozilla.org/MPL/2.0/.
//! Strict, bounded transaction interchange. Payload paths are workspace-relative.
use crate::{
    resources::check_digest, sha256, Edit, ResourceGuard, ResourceOperation, ResourceTransaction,
    Workspace,
};
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer,
};
use std::{fmt, marker::PhantomData};
use wonderland_legacy_formats::{
    iff::{ChunkKey, IffChunk},
    Limits,
};

pub const MAX_TRANSACTION_SPEC_BYTES: usize = 1024 * 1024;

/// Serde's derived structs also accept positional sequences. Every struct at a
/// JSON schema boundary goes through deserialize_map, then its derived visitor
/// receives MapAccess directly: no sequence form and no intermediate Content.
struct ObjectOnly<T>(T);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for ObjectOnly<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
            type Value = ObjectOnly<T>;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                T::deserialize(de::value::MapAccessDeserializer::new(map)).map(ObjectOnly)
            }
        }
        deserializer.deserialize_map(ObjectVisitor(PhantomData))
    }
}

/// Optional schema fields may be absent; when present they must deserialize as
/// T. In particular, a JSON null must not turn a forbidden field into absence.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

fn required<T, E: de::Error>(value: Option<T>, field: &'static str) -> Result<T, E> {
    value.ok_or_else(|| E::missing_field(field))
}

fn allowed_fields<E: de::Error>(
    fields: &[(&'static str, bool)],
    allowed: &'static [&'static str],
) -> Result<(), E> {
    for (name, present) in fields {
        if *present && !allowed.contains(name) {
            return Err(E::unknown_field(name, allowed));
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransactionSpec {
    schema_version: u32,
    source_sha256: String,
    operations: Vec<OperationSpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeySpec {
    kind_hex: String,
    id: u16,
}
impl KeySpec {
    fn key(self) -> Result<ChunkKey, String> {
        Ok(ChunkKey {
            kind: decode_hex(&self.kind_hex, "resource kind")?,
            id: self.id,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GuardSpec {
    resource_sha256: String,
    // Missing differs from an explicit null: every resource operation must
    // declare its expectation even when the format has no version field.
    #[serde(deserialize_with = "required_version")]
    format_version: Option<u32>,
}
fn required_version<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u32>, D::Error> {
    Option::<u32>::deserialize(deserializer)
}
impl GuardSpec {
    fn guard(self) -> Result<ResourceGuard, String> {
        check_digest(&self.resource_sha256, &self.resource_sha256, "resource")?;
        Ok(ResourceGuard {
            resource_hash: self.resource_sha256,
            format_version: self.format_version,
        })
    }
}

enum OperationSpec {
    Add {
        key: KeySpec,
        flags: u16,
        label_hex: String,
        payload_file: String,
        payload_sha256: String,
    },
    Remove {
        key: KeySpec,
        expected: GuardSpec,
    },
    Edit {
        key: KeySpec,
        expected: GuardSpec,
        edit: EditSpec,
    },
    Metadata {
        key: KeySpec,
        expected: GuardSpec,
        new_key: KeySpec,
        flags: u16,
        label_hex: String,
    },
}

// Plain typed fields stream directly from the input. Unknown names are rejected
// by the derived map visitor BEFORE next_value, including when op comes later.
// Internally tagged serde enums cannot be used here: they buffer arbitrary
// unknown values into Content before the variant's deny_unknown_fields runs.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OperationFields {
    op: String,
    key: ObjectOnly<KeySpec>,
    #[serde(default, deserialize_with = "present")]
    expected: Option<ObjectOnly<GuardSpec>>,
    #[serde(default, deserialize_with = "present")]
    new_key: Option<ObjectOnly<KeySpec>>,
    #[serde(default, deserialize_with = "present")]
    flags: Option<u16>,
    #[serde(default, deserialize_with = "present")]
    label_hex: Option<String>,
    #[serde(default, deserialize_with = "present")]
    payload_file: Option<String>,
    #[serde(default, deserialize_with = "present")]
    payload_sha256: Option<String>,
    #[serde(default, deserialize_with = "present")]
    edit: Option<EditSpec>,
}

impl<'de> Deserialize<'de> for OperationSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = ObjectOnly::<OperationFields>::deserialize(deserializer)?.0;
        let present = [
            ("expected", fields.expected.is_some()),
            ("new_key", fields.new_key.is_some()),
            ("flags", fields.flags.is_some()),
            ("label_hex", fields.label_hex.is_some()),
            ("payload_file", fields.payload_file.is_some()),
            ("payload_sha256", fields.payload_sha256.is_some()),
            ("edit", fields.edit.is_some()),
        ];
        Ok(match fields.op.as_str() {
            "add" => {
                allowed_fields::<D::Error>(
                    &present,
                    &["flags", "label_hex", "payload_file", "payload_sha256"],
                )?;
                Self::Add {
                    key: fields.key.0,
                    flags: required(fields.flags, "flags")?,
                    label_hex: required(fields.label_hex, "label_hex")?,
                    payload_file: required(fields.payload_file, "payload_file")?,
                    payload_sha256: required(fields.payload_sha256, "payload_sha256")?,
                }
            }
            "remove" => {
                allowed_fields::<D::Error>(&present, &["expected"])?;
                Self::Remove {
                    key: fields.key.0,
                    expected: required(fields.expected, "expected")?.0,
                }
            }
            "metadata" => {
                allowed_fields::<D::Error>(
                    &present,
                    &["expected", "new_key", "flags", "label_hex"],
                )?;
                Self::Metadata {
                    key: fields.key.0,
                    expected: required(fields.expected, "expected")?.0,
                    new_key: required(fields.new_key, "new_key")?.0,
                    flags: required(fields.flags, "flags")?,
                    label_hex: required(fields.label_hex, "label_hex")?,
                }
            }
            "edit" => {
                allowed_fields::<D::Error>(&present, &["expected", "edit"])?;
                Self::Edit {
                    key: fields.key.0,
                    expected: required(fields.expected, "expected")?.0,
                    edit: required(fields.edit, "edit")?,
                }
            }
            _ => {
                return Err(de::Error::unknown_variant(
                    &fields.op,
                    &["add", "remove", "metadata", "edit"],
                ))
            }
        })
    }
}

enum EditSpec {
    BhavBranch {
        instruction: usize,
        true_pointer: u8,
        false_pointer: u8,
    },
    BhavOperand {
        instruction: usize,
        operand_hex: String,
    },
    String {
        set: usize,
        index: usize,
        value: String,
    },
    Tuning {
        index: usize,
        value: u16,
    },
    Slot {
        index: usize,
        offset: [f32; 3],
    },
    Palette {
        index: usize,
        rgb: [u8; 3],
    },
    Unknown {
        payload_file: String,
        payload_sha256: String,
    },
}

// "value" is a string for a string edit and u16 for tuning. Parse just these
// scalar types directly, without an untagged enum's arbitrary-value buffering.
enum EditValue {
    Text(String),
    Constant(u16),
}
impl<'de> Deserialize<'de> for EditValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ValueVisitor;
        impl Visitor<'_> for ValueVisitor {
            type Value = EditValue;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a string or an unsigned 16-bit integer")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(EditValue::Text(value.to_owned()))
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(EditValue::Text(value))
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                u16::try_from(value)
                    .map(EditValue::Constant)
                    .map_err(|_| E::custom("tuning value must fit u16"))
            }
        }
        deserializer.deserialize_any(ValueVisitor)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditFields {
    op: String,
    #[serde(default, deserialize_with = "present")]
    instruction: Option<usize>,
    #[serde(default, deserialize_with = "present")]
    true_pointer: Option<u8>,
    #[serde(default, deserialize_with = "present")]
    false_pointer: Option<u8>,
    #[serde(default, deserialize_with = "present")]
    operand_hex: Option<String>,
    #[serde(default, deserialize_with = "present")]
    set: Option<usize>,
    #[serde(default, deserialize_with = "present")]
    index: Option<usize>,
    #[serde(default, deserialize_with = "present")]
    value: Option<EditValue>,
    #[serde(default, deserialize_with = "present")]
    offset: Option<[f32; 3]>,
    #[serde(default, deserialize_with = "present")]
    rgb: Option<[u8; 3]>,
    #[serde(default, deserialize_with = "present")]
    payload_file: Option<String>,
    #[serde(default, deserialize_with = "present")]
    payload_sha256: Option<String>,
}
impl<'de> Deserialize<'de> for EditSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = ObjectOnly::<EditFields>::deserialize(deserializer)?.0;
        let present = [
            ("instruction", fields.instruction.is_some()),
            ("true_pointer", fields.true_pointer.is_some()),
            ("false_pointer", fields.false_pointer.is_some()),
            ("operand_hex", fields.operand_hex.is_some()),
            ("set", fields.set.is_some()),
            ("index", fields.index.is_some()),
            ("value", fields.value.is_some()),
            ("offset", fields.offset.is_some()),
            ("rgb", fields.rgb.is_some()),
            ("payload_file", fields.payload_file.is_some()),
            ("payload_sha256", fields.payload_sha256.is_some()),
        ];
        Ok(match fields.op.as_str() {
            "bhav-branch" => {
                allowed_fields::<D::Error>(
                    &present,
                    &["instruction", "true_pointer", "false_pointer"],
                )?;
                Self::BhavBranch {
                    instruction: required(fields.instruction, "instruction")?,
                    true_pointer: required(fields.true_pointer, "true_pointer")?,
                    false_pointer: required(fields.false_pointer, "false_pointer")?,
                }
            }
            "bhav-operand" => {
                allowed_fields::<D::Error>(&present, &["instruction", "operand_hex"])?;
                Self::BhavOperand {
                    instruction: required(fields.instruction, "instruction")?,
                    operand_hex: required(fields.operand_hex, "operand_hex")?,
                }
            }
            "string" => {
                allowed_fields::<D::Error>(&present, &["set", "index", "value"])?;
                let EditValue::Text(value) = required(fields.value, "value")? else {
                    return Err(de::Error::custom("string edit value must be a string"));
                };
                Self::String {
                    set: required(fields.set, "set")?,
                    index: required(fields.index, "index")?,
                    value,
                }
            }
            "tuning" => {
                allowed_fields::<D::Error>(&present, &["index", "value"])?;
                let EditValue::Constant(value) = required(fields.value, "value")? else {
                    return Err(de::Error::custom(
                        "tuning edit value must be an unsigned 16-bit integer",
                    ));
                };
                Self::Tuning {
                    index: required(fields.index, "index")?,
                    value,
                }
            }
            "slot" => {
                allowed_fields::<D::Error>(&present, &["index", "offset"])?;
                Self::Slot {
                    index: required(fields.index, "index")?,
                    offset: required(fields.offset, "offset")?,
                }
            }
            "palette" => {
                allowed_fields::<D::Error>(&present, &["index", "rgb"])?;
                Self::Palette {
                    index: required(fields.index, "index")?,
                    rgb: required(fields.rgb, "rgb")?,
                }
            }
            "unknown" => {
                allowed_fields::<D::Error>(&present, &["payload_file", "payload_sha256"])?;
                Self::Unknown {
                    payload_file: required(fields.payload_file, "payload_file")?,
                    payload_sha256: required(fields.payload_sha256, "payload_sha256")?,
                }
            }
            _ => {
                return Err(de::Error::unknown_variant(
                    &fields.op,
                    &[
                        "bhav-branch",
                        "bhav-operand",
                        "string",
                        "tuning",
                        "slot",
                        "palette",
                        "unknown",
                    ],
                ))
            }
        })
    }
}

/// Decode exactly N bytes. Hex interchange avoids inventing a label or four-byte
/// resource-kind encoding, and validates before taking any string slices.
pub fn decode_hex<const N: usize>(value: &str, field: &str) -> Result<[u8; N], String> {
    if value.len() != N.checked_mul(2).ok_or("hex length overflow")?
        || !value.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(format!(
            "{field} must contain exactly {} hexadecimal characters",
            N * 2
        ));
    }
    let mut bytes = [0; N];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)
            .map_err(|_| format!("invalid {field} hex"))?;
    }
    Ok(bytes)
}

impl ResourceTransaction {
    /// Parse version 1 JSON with unknown fields/types rejected at every level.
    /// Sidecar payloads are read through Workspace and bound to declared hashes.
    /// This constructs a transaction only; ResourceDocument::transact validates
    /// its snapshot guards and publishes the complete candidate atomically.
    pub fn from_json(bytes: &[u8], workspace: &Workspace, limits: &Limits) -> Result<Self, String> {
        limits.check_input(bytes).map_err(|e| e.to_string())?;
        limits
            .check_count(
                bytes.len(),
                MAX_TRANSACTION_SPEC_BYTES,
                0,
                "transaction JSON bytes",
            )
            .map_err(|e| e.to_string())?;
        // The reserve is for the field-only parser above: owned scalar strings
        // are bounded by input bytes; serde_json retains one escape scratch
        // buffer; the only growable collection is the typed operation list.
        // Required operation fields bound each stored entry's wire footprint;
        // both that list's geometric capacity and the converted list fit this
        // reserve. Unknown values are never parsed, so an attacker cannot create
        // an arbitrary Content/Value tree inside a rejected field. Sidecar Vec
        // capacities are separately charged below, before another sidecar read.
        let mut retained = bytes
            .len()
            .checked_mul(16)
            .ok_or("transaction JSON allocation overflow")?;
        limits
            .check_count(
                retained,
                limits.max_total_decoded_bytes,
                0,
                "transaction JSON allocation",
            )
            .map_err(|e| e.to_string())?;
        let ObjectOnly(spec): ObjectOnly<TransactionSpec> =
            serde_json::from_slice(bytes).map_err(|e| format!("invalid transaction JSON: {e}"))?;
        if spec.schema_version != 1 {
            return Err("unsupported transaction schema_version; expected 1".into());
        }
        check_digest(&spec.source_sha256, &spec.source_sha256, "source")?;
        limits
            .check_count(
                spec.operations.len(),
                limits.max_entries,
                0,
                "transaction operations",
            )
            .map_err(|e| e.to_string())?;
        let mut operations = Vec::with_capacity(spec.operations.len());
        for operation in spec.operations {
            operations.push(match operation {
                OperationSpec::Add {
                    key,
                    flags,
                    label_hex,
                    payload_file,
                    payload_sha256,
                } => ResourceOperation::Add {
                    chunk: IffChunk {
                        key: key.key()?,
                        flags,
                        label: decode_hex(&label_hex, "resource label")?,
                        data: read_payload(
                            workspace,
                            &payload_file,
                            &payload_sha256,
                            &mut retained,
                            limits,
                        )?,
                    },
                },
                OperationSpec::Remove { key, expected } => ResourceOperation::Remove {
                    key: key.key()?,
                    expected: expected.guard()?,
                },
                OperationSpec::Metadata {
                    key,
                    expected,
                    new_key,
                    flags,
                    label_hex,
                } => ResourceOperation::SetMetadata {
                    key: key.key()?,
                    expected: expected.guard()?,
                    new_key: new_key.key()?,
                    flags,
                    label: decode_hex(&label_hex, "resource label")?,
                },
                OperationSpec::Edit {
                    key,
                    expected,
                    edit,
                } => ResourceOperation::Edit {
                    key: key.key()?,
                    expected: expected.guard()?,
                    edit: match edit {
                        EditSpec::BhavBranch {
                            instruction,
                            true_pointer,
                            false_pointer,
                        } => Edit::BhavBranch {
                            instruction,
                            true_pointer,
                            false_pointer,
                        },
                        EditSpec::BhavOperand {
                            instruction,
                            operand_hex,
                        } => Edit::BhavOperand {
                            instruction,
                            operand: decode_hex(&operand_hex, "BHAV operand")?,
                        },
                        EditSpec::String { set, index, value } => {
                            limits
                                .check_count(
                                    value.len(),
                                    limits.max_string_bytes,
                                    0,
                                    "transaction string bytes",
                                )
                                .map_err(|e| e.to_string())?;
                            Edit::StringValue { set, index, value }
                        }
                        EditSpec::Tuning { index, value } => Edit::TuningConstant { index, value },
                        EditSpec::Slot { index, offset } => Edit::SlotOffset { index, offset },
                        EditSpec::Palette { index, rgb } => Edit::PaletteColor { index, rgb },
                        EditSpec::Unknown {
                            payload_file,
                            payload_sha256,
                        } => Edit::UnknownBytes(read_payload(
                            workspace,
                            &payload_file,
                            &payload_sha256,
                            &mut retained,
                            limits,
                        )?),
                    },
                },
            });
        }
        Ok(Self {
            source_hash: spec.source_sha256,
            operations,
        })
    }
}

fn read_payload(
    workspace: &Workspace,
    path: &str,
    expected: &str,
    retained: &mut usize,
    limits: &Limits,
) -> Result<Vec<u8>, String> {
    check_digest(expected, expected, "payload")?;
    let remaining = limits
        .max_total_decoded_bytes
        .checked_sub(*retained)
        .ok_or("transaction payload allocation limit exceeded")?;
    let data = workspace.read_limited(path, limits.max_resource_bytes.min(remaining))?;
    *retained = retained
        .checked_add(data.capacity())
        .ok_or("transaction payload allocation overflow")?;
    limits
        .check_count(
            *retained,
            limits.max_total_decoded_bytes,
            0,
            "transaction payload allocation",
        )
        .map_err(|e| e.to_string())?;
    check_digest(expected, &sha256(&data), "payload")?;
    Ok(data)
}
