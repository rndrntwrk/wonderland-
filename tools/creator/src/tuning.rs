// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, obtain one at https://mozilla.org/MPL/2.0/.
//! OTF constant editing through the actual byte-preserving semantic XML codec.
use crate::{json_string, sha256};
use wonderland_legacy_formats::{
    semantic::{self, Otf},
    Limits,
};
pub struct TuningDocument {
    tuning: Otf,
}
impl TuningDocument {
    pub fn import(bytes: &[u8], limits: &Limits) -> Result<Self, String> {
        Ok(Self {
            tuning: semantic::decode_otf(bytes, limits).map_err(|e| e.to_string())?,
        })
    }
    pub fn export(&self, limits: &Limits) -> Result<Vec<u8>, String> {
        semantic::encode_otf(&self.tuning, limits).map_err(|e| e.to_string())
    }
    pub fn edit_constant(
        &mut self,
        expected_hash: &str,
        table_id: i32,
        key_id: i32,
        value: i32,
        limits: &Limits,
    ) -> Result<(), String> {
        if sha256(&self.export(limits)?) != expected_hash.to_ascii_lowercase() {
            return Err("OTF source SHA-256 conflict".into());
        }
        let mut tables = self
            .tuning
            .tables
            .iter()
            .enumerate()
            .filter(|(_, t)| t.id == table_id);
        let table = tables
            .next()
            .map(|(i, _)| i)
            .ok_or("OTF table ID not found")?;
        if tables.next().is_some() {
            return Err("ambiguous duplicate OTF table ID".into());
        }
        let mut keys = self.tuning.tables[table]
            .keys
            .iter()
            .enumerate()
            .filter(|(_, k)| k.id == key_id);
        let key = keys.next().map(|(i, _)| i).ok_or("OTF key ID not found")?;
        if keys.next().is_some() {
            return Err("ambiguous duplicate OTF key ID".into());
        }
        let mut candidate = self.tuning.clone();
        candidate.tables[table].keys[key].value = value;
        semantic::encode_otf(&candidate, limits).map_err(|e| e.to_string())?;
        self.tuning = candidate;
        Ok(())
    }
    pub fn metadata_json(&self, limits: &Limits) -> Result<String, String> {
        let mut out=format!("{{\"schema\":\"wonderland.creator.otf-metadata.v1\",\"source_sha256\":\"{}\",\"tables\":[",sha256(&self.export(limits)?));
        for (i, t) in self.tuning.tables.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"id\":{},\"name\":{},\"keys\":[",
                t.id,
                json_string(&t.name)
            ));
            for (j, k) in t.keys.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                out.push_str(&format!(
                    "{{\"id\":{},\"label\":{},\"value\":{}}}",
                    k.id,
                    json_string(&k.label),
                    k.value
                ));
            }
            out.push_str("]}");
            if out.len() > limits.max_total_decoded_bytes {
                return Err("OTF metadata byte limit exceeded".into());
            }
        }
        out.push_str("]}\n");
        Ok(out)
    }
}
