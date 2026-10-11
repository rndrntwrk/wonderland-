// SPDX-License-Identifier: MPL-2.0
//! Offline source CityNeighbourhood JSON editing. No database writes or GUID generation.
use crate::{
    json_support::{self, JsonEdit},
    sha256,
};
use serde_json::Value;
use std::collections::BTreeSet;
use wonderland_legacy_formats::Limits;

pub struct NeighborhoodDocument {
    bytes: Vec<u8>,
    entries: Vec<Value>,
}
fn validate(tree: &Value) -> Result<(), String> {
    let entries = tree
        .as_array()
        .ok_or("neighborhood document must be an array")?;
    let mut ids = BTreeSet::new();
    for entry in entries {
        if !entry.is_object() {
            return Err("neighborhood entry must be an object".into());
        }
        let id = entry["GUID"]
            .as_str()
            .ok_or("neighborhood GUID is required")?;
        if id.is_empty() || !ids.insert(id) {
            return Err("empty or duplicate neighborhood GUID".into());
        }
        if !entry["Name"].is_string() {
            return Err("neighborhood Name must be a string".into());
        }
        if entry
            .get("Description")
            .is_some_and(|v| !v.is_null() && !v.is_string())
        {
            return Err("invalid neighborhood Description".into());
        }
        if !entry["Location"].is_object() {
            return Err("neighborhood Location must be an object".into());
        }
        for axis in ["X", "Y"] {
            if !entry["Location"][axis]
                .as_i64()
                .is_some_and(|n| (0..512).contains(&n))
            {
                return Err("neighborhood location outside 512x512 city".into());
            }
        }
        if let Some(color) = entry.get("Color").filter(|c| !c.is_null()) {
            if !color.is_object() {
                return Err("neighborhood Color must be object or null".into());
            }
            for channel in ["R", "G", "B", "A"] {
                if color
                    .get(channel)
                    .is_some_and(|v| v.as_u64().is_none_or(|n| n > 255))
                {
                    return Err("neighborhood color channel must fit u8".into());
                }
            }
            for channel in ["R", "G", "B"] {
                if color.get(channel).is_none() {
                    return Err("neighborhood color needs RGB channels".into());
                }
            }
        }
        if entry.get("DistanceMul").is_some_and(|v| {
            !v.as_f64()
                .is_some_and(|n| n.is_finite() && (n as f32).is_finite() && n > 0.0)
        }) {
            return Err("DistanceMul must be positive finite binary32".into());
        }
    }
    Ok(())
}
impl NeighborhoodDocument {
    pub fn import(bytes: &[u8], limits: &Limits) -> Result<Self, String> {
        let tree = json_support::parse(bytes, limits)?;
        validate(&tree)?;
        let Value::Array(entries) = tree else {
            unreachable!()
        };
        Ok(Self {
            bytes: bytes.to_vec(),
            entries,
        })
    }
    pub fn entries(&self) -> &[Value] {
        &self.entries
    }
    pub fn source_sha256(&self) -> String {
        sha256(&self.bytes)
    }
    pub fn export(&self, limits: &Limits) -> Result<Vec<u8>, String> {
        json_support::admit(self.bytes.len(), limits)?;
        Ok(self.bytes.clone())
    }
    pub fn apply(
        &mut self,
        expected: &str,
        edits: &[JsonEdit],
        limits: &Limits,
    ) -> Result<(), String> {
        if expected.to_ascii_lowercase() != self.source_sha256() {
            return Err("neighborhood source SHA-256 conflict".into());
        }
        json_support::admit(self.bytes.len(), limits)?;
        let tree = Value::Array(self.entries.clone());
        let candidate = json_support::apply(&tree, edits, limits)?;
        validate(&candidate)?;
        if candidate == tree {
            return Ok(());
        }
        let bytes = json_support::encode(&candidate, limits)?;
        let Value::Array(entries) = candidate else {
            unreachable!()
        };
        self.entries = entries;
        self.bytes = bytes;
        Ok(())
    }
    /// Source CityNeighGeom::NhoodNearest compares squared distance and keeps the
    /// first source entry on ties. Its stored DistanceMul is not used by this lookup.
    pub fn nearest(&self, x: i32, y: i32) -> Result<Option<usize>, String> {
        if !(0..512).contains(&x) || !(0..512).contains(&y) {
            return Err("neighborhood query outside city".into());
        }
        let mut best = None;
        let mut distance = i64::MAX;
        for (i, entry) in self.entries.iter().enumerate() {
            let dx = entry["Location"]["X"].as_i64().unwrap() - i64::from(x);
            let dy = entry["Location"]["Y"].as_i64().unwrap() - i64::from(y);
            let d = dx * dx + dy * dy;
            if d < distance {
                best = Some(i);
                distance = d;
            }
        }
        Ok(best)
    }
    pub fn assignment_map(&self, limits: &Limits) -> Result<Vec<Option<usize>>, String> {
        let bytes = 512usize * 512 * std::mem::size_of::<Option<usize>>();
        if bytes
            .checked_add(self.bytes.len() * 128)
            .is_none_or(|n| n > limits.max_total_decoded_bytes)
            || 512 * 512 > limits.max_pixels
        {
            return Err("neighborhood assignment limit exceeded".into());
        }
        let mut result = Vec::with_capacity(512 * 512);
        for y in 0..512 {
            for x in 0..512 {
                result.push(self.nearest(x, y)?);
            }
        }
        Ok(result)
    }
}
