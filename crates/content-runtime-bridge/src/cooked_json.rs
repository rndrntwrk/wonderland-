// SPDX-License-Identifier: MPL-2.0
//! JSON admission for the concrete cooked-binding DTOs, without a Value tree.
//!
//! The declared bound covers DTO deserialization plus one clone/conversion
//! workspace, including vector spare capacity and temporary map/set indexes.
//! Raw input also reserves parser scratch before parsing starts. This is an
//! admission policy for these DTOs, not a generic allocator accounting promise.
//! Field ranges, finite values, duplicate identities and array shapes remain the
//! typed DTO's responsibility.

use crate::budget::ImportBudget;
use serde::{
    de::{self, DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor},
    Deserializer, Serialize,
};
use std::{fmt, io};

const MAX_DEPTH: u8 = 32;
const NODE_BYTES: usize = 1024;
const STRING_BYTE_FACTOR: usize = 8;

/// Validate syntax and admit every JSON value before typed deserialization.
pub(crate) fn decode<T: DeserializeOwned>(
    bytes: &[u8],
    max_bytes: usize,
    budget: &mut ImportBudget,
) -> Result<T, String> {
    if bytes.len() > max_bytes {
        return Err("cooked JSON byte limit exceeded".into());
    }
    let scratch = bytes
        .len()
        .checked_mul(2)
        .ok_or("cooked JSON parser scratch size overflow")?;
    budget
        .reserve(scratch)
        .map_err(|_| "cooked JSON parser scratch budget exceeded")?;
    {
        let mut admission = Admission {
            budget,
            failure: None,
        };
        let mut parser = serde_json::Deserializer::from_slice(bytes);
        let result = ValueSeed {
            admission: &mut admission,
            depth: 1,
        }
        .deserialize(&mut parser)
        .and_then(|()| parser.end());
        if result.is_err() {
            return Err(admission.failure.unwrap_or("invalid cooked JSON").into());
        }
    }
    // Drop the first parser and its scratch before constructing the typed DTO.
    // No attacker-controlled field name or input is copied into the error.
    serde_json::from_slice(bytes).map_err(|_| "cooked JSON does not match its schema".into())
}

struct Admission<'a> {
    budget: &'a mut ImportBudget,
    failure: Option<&'static str>,
}

impl Admission<'_> {
    fn reject<E: de::Error>(&mut self, message: &'static str) -> E {
        self.failure = Some(message);
        E::custom(message)
    }

    fn reserve<E: de::Error>(&mut self, bytes: usize) -> Result<(), E> {
        self.budget
            .reserve(bytes)
            .map_err(|_| self.reject::<E>("cooked JSON allocation budget exceeded"))
    }

    fn node<E: de::Error>(&mut self, depth: u8) -> Result<(), E> {
        if depth > MAX_DEPTH {
            return Err(self.reject("cooked JSON depth limit exceeded"));
        }
        self.reserve(NODE_BYTES)
    }

    fn text<E: de::Error>(&mut self, text: &str) -> Result<(), E> {
        let bytes = text
            .len()
            .checked_mul(STRING_BYTE_FACTOR)
            .ok_or_else(|| self.reject::<E>("cooked JSON string size overflow"))?;
        self.reserve(bytes)
    }
}

struct ValueSeed<'a, 'budget> {
    admission: &'a mut Admission<'budget>,
    /// The root value is depth one; object keys do not add a nesting level.
    depth: u8,
}

impl<'de> DeserializeSeed<'de> for ValueSeed<'_, '_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        self.admission.node::<D::Error>(self.depth)?;
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ValueSeed<'_, '_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<(), E> {
        self.admission.text(value)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<(), A::Error> {
        while sequence
            .next_element_seed(ValueSeed {
                admission: &mut *self.admission,
                depth: self.depth + 1,
            })?
            .is_some()
        {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while map
            .next_key_seed(KeySeed {
                admission: &mut *self.admission,
            })?
            .is_some()
        {
            map.next_value_seed(ValueSeed {
                admission: &mut *self.admission,
                depth: self.depth + 1,
            })?;
        }
        Ok(())
    }
}

struct KeySeed<'a, 'budget> {
    admission: &'a mut Admission<'budget>,
}

impl<'de> DeserializeSeed<'de> for KeySeed<'_, '_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        // Charge key/index bookkeeping even though the scan retains no keys.
        self.admission.reserve::<D::Error>(NODE_BYTES)?;
        deserializer.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for KeySeed<'_, '_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON object key")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<(), E> {
        self.admission.text(value)
    }
}

/// Serialize directly into one admitted, exactly sized output buffer.
pub(crate) fn encode<T: Serialize>(value: &T, max_bytes: usize) -> Result<Vec<u8>, String> {
    let mut counter = CountingWriter {
        written: 0,
        max_bytes,
    };
    serde_json::to_writer(&mut counter, value)
        .map_err(|_| "cooked JSON serialization failed or byte limit exceeded")?;
    let expected = counter.written;
    let mut bytes = vec![0; expected];
    let written = {
        let mut writer = FixedWriter {
            bytes: &mut bytes,
            written: 0,
        };
        serde_json::to_writer(&mut writer, value)
            .map_err(|_| "cooked JSON serialization failed or changed length")?;
        writer.written
    };
    if written != expected {
        return Err("cooked JSON serialization changed length".into());
    }
    Ok(bytes)
}

struct CountingWriter {
    written: usize,
    max_bytes: usize,
}

impl io::Write for CountingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.written = self
            .written
            .checked_add(bytes.len())
            .filter(|written| *written <= self.max_bytes)
            .ok_or_else(|| io::Error::other("cooked JSON byte limit exceeded"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct FixedWriter<'a> {
    bytes: &'a mut [u8],
    written: usize,
}

impl io::Write for FixedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let end = self
            .written
            .checked_add(bytes.len())
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| io::Error::other("cooked JSON output length exceeded"))?;
        self.bytes[self.written..end].copy_from_slice(bytes);
        self.written = end;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};
    use crate::budget::ImportBudget;
    use crate::cooked_metadata::PropertyV1;
    use wonderland_legacy_formats::Limits;

    fn budget(max_total_decoded_bytes: usize) -> ImportBudget {
        ImportBudget::new(&Limits {
            max_total_decoded_bytes,
            ..Limits::default()
        })
    }

    #[test]
    fn escaped_metadata_output_uses_exact_capacity_and_exact_byte_limit() {
        let property = PropertyV1 {
            key: "sound".into(),
            value: "quote \" slash \\ line\n tab\t nul\0 snow 雪".into(),
        };
        let expected =
            r#"{"key":"sound","value":"quote \" slash \\ line\n tab\t nul\u0000 snow 雪"}"#
                .as_bytes();
        let encoded = encode(&property, expected.len()).unwrap();
        assert_eq!(encoded, expected);
        assert_eq!(encoded.capacity(), encoded.len());
        assert!(encode(&property, expected.len() - 1).is_err());
        let actual: PropertyV1 =
            decode(&encoded, expected.len(), &mut budget(1024 * 1024)).unwrap();
        assert_eq!(actual, property);
    }

    #[test]
    fn raw_byte_limit_rejects_metadata_before_admission_charges() {
        let bytes = br#"{"key":"sound","value":"tone"}"#;
        let mut admission = budget(1024 * 1024);
        let error = decode::<PropertyV1>(bytes, bytes.len() - 1, &mut admission).unwrap_err();
        assert_eq!(error, "cooked JSON byte limit exceeded");
        assert_eq!(admission.used(), 0);
    }

    #[test]
    fn parser_scratch_is_reserved_before_metadata_parsing() {
        let bytes = br#"{"key":"sound","value":"tone"}"#;
        let mut admission = budget(bytes.len() * 2 - 1);
        let error = decode::<PropertyV1>(bytes, bytes.len(), &mut admission).unwrap_err();
        assert_eq!(error, "cooked JSON parser scratch budget exceeded");
        assert_eq!(admission.used(), 0);
    }

    #[test]
    fn node_workspace_is_admitted_before_typed_metadata_loading() {
        let bytes = br#"{"key":"sound","value":"tone"}"#;
        let scratch_bytes = bytes.len() * 2;
        let mut admission = budget(scratch_bytes + 1);
        let error = decode::<PropertyV1>(bytes, bytes.len(), &mut admission).unwrap_err();
        assert_eq!(error, "cooked JSON allocation budget exceeded");
        assert_eq!(admission.used(), scratch_bytes);
    }

    #[test]
    fn decoded_escaped_strings_are_admitted_with_conversion_workspace() {
        let property = PropertyV1 {
            key: "sound".into(),
            value: "雪\\\n".repeat(512),
        };
        let bytes = encode(&property, 64 * 1024).unwrap();
        // This admits parser scratch and the object's nodes, but cannot admit
        // the decoded string and its clone/conversion workspace.
        let mut constrained = budget(bytes.len() * 2 + 16 * 1024);
        let error = decode::<PropertyV1>(&bytes, bytes.len(), &mut constrained).unwrap_err();
        assert_eq!(error, "cooked JSON allocation budget exceeded");
        let actual: PropertyV1 = decode(&bytes, bytes.len(), &mut budget(1024 * 1024)).unwrap();
        assert_eq!(actual, property);
    }

    #[test]
    fn recursive_unknown_payload_hits_depth_limit_before_typed_schema_loading() {
        let payload = |array_depth: usize| {
            format!(
                r#"{{"key":"sound","value":"tone","unexpected":{}"x"{}}}"#,
                "[".repeat(array_depth),
                "]".repeat(array_depth)
            )
        };
        // Root depth is one. Thirty arrays put the scalar at depth 32, so the
        // preflight succeeds and the concrete DTO rejects the unknown field.
        let at_limit = payload(30);
        let error = decode::<PropertyV1>(
            at_limit.as_bytes(),
            at_limit.len(),
            &mut budget(1024 * 1024),
        )
        .unwrap_err();
        assert_eq!(error, "cooked JSON does not match its schema");
        let beyond_limit = payload(31);
        let error = decode::<PropertyV1>(
            beyond_limit.as_bytes(),
            beyond_limit.len(),
            &mut budget(1024 * 1024),
        )
        .unwrap_err();
        assert_eq!(error, "cooked JSON depth limit exceeded");
    }
}
