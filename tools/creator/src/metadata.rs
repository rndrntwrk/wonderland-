// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, obtain one at https://mozilla.org/MPL/2.0/.
use crate::{
    resources::{resource_version, validate_resource},
    sha256, ResourceDocument,
};
use wonderland_legacy_formats::{semantic, Limits};
pub fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\u{20}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
impl ResourceDocument {
    pub fn metadata_json(&self, limits: &Limits) -> Result<String, String> {
        let mut out=format!("{{\"schema\":\"wonderland.creator.resource-metadata.v1\",\"source_sha256\":\"{}\",\"header_hex\":\"{}\",\"chunks\":[",sha256(&self.export(limits)?),hex(&self.file().header));
        for (i, c) in self.file().chunks.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let validation = validate_resource(c, limits)
                .map(|_| "supported-valid-or-raw-unknown".to_owned())
                .unwrap_or_else(|e| e);
            let version = resource_version(c, limits)
                .ok()
                .flatten()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".into());
            out.push_str(&format!("{{\"kind\":{},\"kind_hex\":\"{}\",\"id\":{},\"flags\":{},\"label_hex\":\"{}\",\"bytes\":{},\"sha256\":\"{}\",\"format_version\":{},\"validation\":{}",json_string(&String::from_utf8_lossy(&c.key.kind)),hex(&c.key.kind),c.key.id,c.flags,hex(&c.label),c.data.len(),sha256(&c.data),version,json_string(&validation)));
            if c.key.kind == *b"BHAV" {
                if let Ok(bhav) = semantic::decode_bhav(&c.data, limits) {
                    out.push_str(&format!(
                        ",\"instruction_count\":{},\"instructions\":[",
                        bhav.instructions.len()
                    ));
                    for (n, inst) in bhav.instructions.iter().enumerate() {
                        if n > 0 {
                            out.push(',');
                        }
                        out.push_str(&format!("{{\"index\":{n},\"opcode\":{},\"true\":{},\"false\":{},\"operand_hex\":\"{}\"}}",inst.opcode,inst.true_pointer,inst.false_pointer,hex(&inst.operand)));
                    }
                    out.push(']');
                    if let Ok(cfg) = crate::validate_cfg(&bhav) {
                        out.push_str(&format!(
                            ",\"unreachable\":{:?},\"alternate_error_instructions\":{:?}",
                            cfg.unreachable, cfg.alternate_error_instructions
                        ));
                    }
                }
            }
            if matches!(&c.key.kind, b"STR#" | b"CTSS" | b"TTAs") {
                if let Ok(strings) = semantic::decode_strings(&c.data, limits) {
                    out.push_str(",\"string_sets\":[");
                    for (set_no, set) in strings.sets.iter().enumerate() {
                        if set_no > 0 {
                            out.push(',');
                        }
                        out.push('[');
                        for (index, item) in set.iter().enumerate() {
                            if index > 0 {
                                out.push(',');
                            }
                            out.push_str(&format!("{{\"language\":{},\"encoding\":{},\"value\":{},\"comment\":{},\"value_hex\":\"{}\"}}",item.language,json_string(&format!("{:?}",item.value.encoding)),json_string(&item.value.text()),json_string(&item.comment.text()),hex(&item.value.bytes)));
                        }
                        out.push(']');
                    }
                    out.push(']');
                }
            }
            if c.key.kind == *b"BCON" {
                if let Ok(v) = semantic::decode_bcon(&c.data, limits) {
                    out.push_str(&format!(",\"constants\":{:?}", v.constants));
                }
            }
            if c.key.kind == *b"SLOT" {
                if let Ok(v) = wonderland_legacy_formats::sprites::decode_slot(&c.data, limits) {
                    out.push_str(&format!(",\"slot_count\":{}", v.slots.len()));
                }
            }
            if c.key.kind == *b"PALT" {
                if let Ok(v) = wonderland_legacy_formats::sprites::decode_palt(&c.data, limits) {
                    out.push_str(",\"colors_rgb\":[");
                    for (index, color) in v.colors.iter().enumerate() {
                        if index > 0 {
                            out.push(',');
                        }
                        out.push_str(&format!("[{},{},{}]", color[0], color[1], color[2]));
                    }
                    out.push(']');
                }
            }
            out.push('}');
            if out.len() > limits.max_total_decoded_bytes {
                return Err("metadata output byte limit exceeded".into());
            }
        }
        out.push_str("]}\n");
        Ok(out)
    }
}
