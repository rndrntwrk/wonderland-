// SPDX-License-Identifier: MPL-2.0
//! Bounded, duplicate-rejecting JSON documents and explicit path edits.
use serde::{
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
    Deserialize, Serialize,
};
use serde_json::{Map, Number, Value};
use std::{cell::Cell, fmt, io::Write};
use wonderland_legacy_formats::Limits;

pub const MAX_EDITOR_JSON_BYTES: usize = 1024 * 1024;

pub fn admit(bytes: usize, limits: &Limits) -> Result<(), String> {
    if bytes
        > MAX_EDITOR_JSON_BYTES
            .min(limits.max_input_bytes)
            .min(limits.max_resource_bytes)
        || bytes
            .checked_mul(128)
            .and_then(|n| n.checked_add(4096))
            .is_none_or(|n| n > limits.max_total_decoded_bytes)
    {
        return Err("editor JSON working-copy limit exceeded".into());
    }
    Ok(())
}

struct Seed<'a> {
    limits: &'a Limits,
    count: &'a Cell<usize>,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        if self.depth > self.limits.max_depth.min(32) {
            return Err(de::Error::custom("JSON depth limit exceeded"));
        }
        let count = self
            .count
            .get()
            .checked_add(1)
            .ok_or_else(|| de::Error::custom("JSON node overflow"))?;
        if count > self.limits.max_entries {
            return Err(de::Error::custom("JSON node limit exceeded"));
        }
        self.count.set(count);
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("bounded JSON")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("nonfinite number"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        if v.len() > self.limits.max_string_bytes {
            return Err(E::custom("JSON string limit exceeded"));
        }
        Ok(Value::String(v.into()))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(Seed {
            depth: self.depth + 1,
            ..self
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if key.len() > self.limits.max_string_bytes {
                return Err(de::Error::custom("JSON key limit exceeded"));
            }
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate JSON key"));
            }
            values.insert(
                key,
                map.next_value_seed(Seed {
                    depth: self.depth + 1,
                    ..self
                })?,
            );
        }
        Ok(Value::Object(values))
    }
}

pub fn parse(bytes: &[u8], limits: &Limits) -> Result<Value, String> {
    admit(bytes.len(), limits)?;
    preserve_numbers(bytes)?;
    let count = Cell::new(0);
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = Seed {
        limits,
        count: &count,
        depth: 0,
    }
    .deserialize(&mut deserializer)
    .map_err(|e| e.to_string())?;
    deserializer.end().map_err(|e| e.to_string())?;
    Ok(value)
}

// Value stores noninteger JSON numbers as f64. A bounded lexical pass rejects
// tokens whose exact decimal value would be changed on reserialization. This
// protects unknown source fields without relying on serde's private number
// protocol or claiming that arbitrary-precision numbers are supported.
fn decimal(token: &str) -> Result<(bool, String, i64), String> {
    let negative = token.starts_with('-');
    let value = token.strip_prefix('-').unwrap_or(token);
    let (mantissa, exponent) = if let Some(at) = value.find(['e', 'E']) {
        (
            &value[..at],
            value[at + 1..]
                .parse::<i64>()
                .map_err(|_| "JSON numeric exponent out of range")?,
        )
    } else {
        (value, 0)
    };
    let fraction = mantissa
        .find('.')
        .map(|at| mantissa.len() - at - 1)
        .unwrap_or(0);
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let trimmed = digits.trim_start_matches('0');
    if trimmed.is_empty() {
        return Ok((negative, "0".into(), 0));
    }
    let significant = trimmed.trim_end_matches('0');
    let exponent = exponent
        .checked_sub(i64::try_from(fraction).map_err(|_| "JSON numeric length overflow")?)
        .and_then(|n| n.checked_add((trimmed.len() - significant.len()) as i64))
        .ok_or("JSON numeric exponent overflow")?;
    Ok((negative, significant.into(), exponent))
}
fn preserve_numbers(bytes: &[u8]) -> Result<(), String> {
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'"' {
            at += 1;
            while at < bytes.len() {
                match bytes[at] {
                    b'\\' => {
                        at = at.saturating_add(2);
                    }
                    b'"' => {
                        at += 1;
                        break;
                    }
                    _ => at += 1,
                }
            }
        } else if bytes[at] == b'-' || bytes[at].is_ascii_digit() {
            let start = at;
            at += 1;
            while at < bytes.len()
                && (bytes[at].is_ascii_digit()
                    || matches!(bytes[at], b'.' | b'e' | b'E' | b'+' | b'-'))
            {
                at += 1;
            }
            let token =
                std::str::from_utf8(&bytes[start..at]).map_err(|_| "invalid JSON number")?;
            let number: Number =
                serde_json::from_slice(&bytes[start..at]).map_err(|e| e.to_string())?;
            if decimal(token)? != decimal(&number.to_string())? {
                return Err("JSON number would lose precision; use an exact representable number or preserve it as text".into());
            }
        } else {
            at += 1;
        }
    }
    Ok(())
}

pub fn encode<T: Serialize>(value: &T, limits: &Limits) -> Result<Vec<u8>, String> {
    struct Bounded {
        bytes: Vec<u8>,
        cap: usize,
    }
    impl Write for Bounded {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if self
                .bytes
                .len()
                .checked_add(b.len())
                .is_none_or(|n| n > self.cap)
            {
                return Err(std::io::Error::other("editor JSON output limit exceeded"));
            }
            self.bytes.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let cap = MAX_EDITOR_JSON_BYTES
        .min(limits.max_resource_bytes)
        .min(limits.max_input_bytes)
        .min(limits.max_total_decoded_bytes.saturating_sub(4096) / 128);
    let mut writer = Bounded {
        bytes: Vec::new(),
        cap,
    };
    serde_json::to_writer(&mut writer, value).map_err(|e| e.to_string())?;
    Ok(writer.bytes)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsonEdit {
    pub path: Vec<String>,
    pub remove: bool,
    pub value: Value,
}
impl JsonEdit {
    pub fn set<const N: usize>(path: [&str; N], value: Value) -> Self {
        Self {
            path: path.into_iter().map(str::to_owned).collect(),
            remove: false,
            value,
        }
    }
    pub fn remove<const N: usize>(path: [&str; N]) -> Self {
        Self {
            path: path.into_iter().map(str::to_owned).collect(),
            remove: true,
            value: Value::Null,
        }
    }
}

fn index(key: &str, len: usize, append: bool) -> Result<usize, String> {
    let i = key
        .parse::<usize>()
        .map_err(|_| "JSON array index must be a canonical decimal integer")?;
    if i.to_string() != key || i > len || (!append && i == len) {
        return Err("JSON array index out of range".into());
    }
    Ok(i)
}
pub fn apply(tree: &Value, edits: &[JsonEdit], limits: &Limits) -> Result<Value, String> {
    if edits.len() > limits.max_entries {
        return Err("JSON edit count limit exceeded".into());
    }
    // Admission includes all edit values before cloning the source tree.
    let retained = encode(tree, limits)?.len();
    let edit_bytes = encode(&edits, limits)?.len();
    admit(
        retained
            .checked_add(edit_bytes)
            .ok_or("editor bytes overflow")?,
        limits,
    )?;
    let mut candidate = tree.clone();
    for edit in edits {
        if edit.path.is_empty() || edit.path.len() > limits.max_depth.min(32) {
            return Err("JSON edit needs a bounded nonempty path".into());
        }
        if edit.remove && !edit.value.is_null() {
            return Err("remove edit value must be null".into());
        }
        let mut node = &mut candidate;
        for part in &edit.path[..edit.path.len() - 1] {
            node = match node {
                Value::Object(map) => map.get_mut(part).ok_or("JSON path not found")?,
                Value::Array(array) => {
                    let i = index(part, array.len(), false)?;
                    &mut array[i]
                }
                _ => return Err("JSON path traverses a scalar".into()),
            };
        }
        let key = edit.path.last().unwrap();
        match node {
            Value::Object(map) => {
                if edit.remove {
                    map.remove(key).ok_or("JSON field not found")?;
                } else {
                    map.insert(key.clone(), edit.value.clone());
                }
            }
            Value::Array(array) => {
                let i = index(key, array.len(), !edit.remove)?;
                if edit.remove {
                    array.remove(i);
                } else if i == array.len() {
                    array.push(edit.value.clone());
                } else {
                    array[i] = edit.value.clone();
                }
            }
            _ => return Err("JSON edit target is a scalar".into()),
        }
    }
    // Reparse validates aggregate nodes, strings and depth even for programmatic edits.
    parse(&encode(&candidate, limits)?, limits)
}
