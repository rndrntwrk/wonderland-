// SPDX-License-Identifier: MPL-2.0
//! Source-shaped upgrades.json authoring and source-ordered tuning expansion.
//! Runtime semantics: tso.content/Upgrades/Model/Runtime/RuntimeUpgradeFile.cs.
use crate::{
    json_support::{self, JsonEdit},
    sha256,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use wonderland_legacy_formats::Limits;

pub struct UpgradeDocument {
    original: Vec<u8>,
    tree: Value,
}
pub(crate) fn object(value: &Value) -> Result<(), String> {
    if !value.is_object() {
        Err("expected JSON object".into())
    } else {
        Ok(())
    }
}
pub(crate) fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("expected string {key}"))
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value], String> {
    match value.get(key) {
        None => Ok(&[]),
        Some(Value::Array(a)) => Ok(a),
        _ => Err(format!("expected array {key}")),
    }
}
fn i32value(value: &Value, key: &str, default: i32) -> Result<i32, String> {
    match value.get(key) {
        None => Ok(default),
        Some(v) => v
            .as_i64()
            .and_then(|n| n.try_into().ok())
            .ok_or_else(|| format!("expected signed 32-bit {key}")),
    }
}
fn optional_bool(value: &Value, key: &str, nullable: bool) -> Result<(), String> {
    match value.get(key) {
        None | Some(Value::Bool(_)) => Ok(()),
        Some(Value::Null) if nullable => Ok(()),
        _ => Err(format!("invalid boolean {key}")),
    }
}
fn target(value: &str) -> Result<(i32, i32), String> {
    let (table, index) = value
        .split_once(':')
        .ok_or("tuning target must be table:index")?;
    Ok((
        table.parse().map_err(|_| "invalid tuning table")?,
        index.parse().map_err(|_| "invalid tuning index")?,
    ))
}
fn replacement(value: &str) -> Result<(), String> {
    match value.as_bytes().first() {
        Some(b'V') => {
            value[1..]
                .parse::<i16>()
                .map_err(|_| "upgrade literal must fit i16")?;
        }
        Some(b'C') => {
            target(&value[1..])?;
        }
        _ => return Err("upgrade replacement must start with V or C".into()),
    }
    Ok(())
}
fn validate_subs(subs: &[Value], groups: usize) -> Result<(), String> {
    for sub in subs {
        object(sub)?;
        let old = text(sub, "Old")?;
        if let Some(group) = old.strip_prefix('G') {
            // The checked-in source file contains ignored group references.
            // GroupsIntoSubs discards them before attempting to parse New.
            if group
                .parse::<usize>()
                .ok()
                .is_none_or(|index| index >= groups)
            {
                continue;
            }
        } else {
            target(old)?;
        }
        replacement(text(sub, "New")?)?;
    }
    Ok(())
}
fn validate(tree: &Value) -> Result<(), String> {
    object(tree)?;
    if !matches!(i32value(tree, "Version", 2)?, 1 | 2) {
        return Err("unsupported upgrades version".into());
    }
    if !tree.get("Files").is_some_and(Value::is_array) {
        return Err("upgrades Files array is required".into());
    }
    let mut names = BTreeSet::new();
    for file in array(tree, "Files")? {
        object(file)?;
        let name = text(file, "Name")?;
        if name.is_empty() || !names.insert(name) {
            return Err("empty or ambiguous duplicate upgrade IFF name".into());
        }
        let groups = array(file, "Groups")?;
        for group in groups {
            object(group)?;
            if let Some(name) = group.get("Name") {
                if !name.is_string() {
                    return Err("group Name must be a string".into());
                }
            }
            for tuning in array(group, "Tuning")? {
                target(tuning.as_str().ok_or("group tuning must be a string")?)?;
            }
            if let Some(value) = group.get("DefaultValue").filter(|v| !v.is_null()) {
                replacement(value.as_str().ok_or("group default must be a string")?)?;
            }
        }
        validate_subs(array(file, "Subs")?, groups.len())?;
        let levels = array(file, "Upgrades")?;
        for level in levels {
            object(level)?;
            for key in ["Name", "Ad", "Description"] {
                if let Some(value) = level.get(key) {
                    if !value.is_string() {
                        return Err(format!("upgrade {key} must be a string"));
                    }
                }
            }
            if let Some(price) = level.get("Price") {
                let price = price.as_str().ok_or("price must be a string")?;
                if let Some(literal) = price.strip_prefix('$').or_else(|| price.strip_prefix('R')) {
                    literal
                        .parse::<i32>()
                        .map_err(|_| "upgrade price must fit i32")?;
                } else {
                    u32::from_str_radix(price, 16).map_err(|_| {
                        "upgrade price must be literal, relative or hexadecimal object GUID"
                    })?;
                }
            }
            optional_bool(level, "Hidden", true)?;
            validate_subs(array(level, "Subs")?, groups.len())?;
        }
        let mut ids = BTreeSet::new();
        for config in array(file, "Config")? {
            object(config)?;
            let guid = u32::from_str_radix(text(config, "GUID")?, 16)
                .map_err(|_| "invalid upgrade object GUID")?;
            if !ids.insert(guid) {
                return Err("duplicate upgrade object GUID".into());
            }
            let level = i32value(config, "Level", 0)?;
            if level < 0 || level as usize >= levels.len() {
                return Err("upgrade starting level out of range".into());
            }
            if config.get("Limit").is_some_and(|v| !v.is_null()) {
                let limit = i32value(config, "Limit", 0)?;
                if limit < level || limit as usize >= levels.len() {
                    return Err("upgrade limit out of range".into());
                }
            }
            optional_bool(config, "Special", true)?;
            optional_bool(config, "Reinit", false)?;
        }
    }
    Ok(())
}
impl UpgradeDocument {
    pub fn import(bytes: &[u8], limits: &Limits) -> Result<Self, String> {
        let tree = json_support::parse(bytes, limits)?;
        validate(&tree)?;
        Ok(Self {
            original: bytes.to_vec(),
            tree,
        })
    }
    pub fn tree(&self) -> &Value {
        &self.tree
    }
    pub fn export(&self, limits: &Limits) -> Result<Vec<u8>, String> {
        json_support::admit(self.original.len(), limits)?;
        Ok(self.original.clone())
    }
    pub fn source_sha256(&self) -> String {
        sha256(&self.original)
    }
    pub fn metadata_json(&self, limits: &Limits) -> Result<String, String> {
        json_support::admit(self.original.len(), limits)?;
        let result = serde_json::json!({"schema":"wonderland.creator.upgrades.v1","source_sha256":self.source_sha256(),"document":self.tree});
        String::from_utf8(json_support::encode(&result, limits)?).map_err(|e| e.to_string())
    }
    pub fn apply(
        &mut self,
        expected: &str,
        edits: &[JsonEdit],
        limits: &Limits,
    ) -> Result<(), String> {
        if expected.to_ascii_lowercase() != self.source_sha256() {
            return Err("upgrades source SHA-256 conflict".into());
        }
        let candidate = json_support::apply(&self.tree, edits, limits)?;
        validate(&candidate)?;
        if candidate == self.tree {
            return Ok(());
        }
        let bytes = json_support::encode(&candidate, limits)?;
        self.tree = candidate;
        self.original = bytes;
        Ok(())
    }
    /// Expand group defaults first, direct substitutions next, group targets last.
    /// Constant lookup receives exact signed table/index values. The caller must
    /// use the same global/private/semiglobal tuning snapshot as the source object.
    /// A missing source constant is zero, matching TryGetValue in LoadSubs.
    pub fn resolve_substitutions(
        &self,
        name: &str,
        level: Option<usize>,
        mut lookup: impl FnMut(i32, i32) -> Option<i16>,
    ) -> Result<BTreeMap<(i32, i32), i16>, String> {
        let file = array(&self.tree, "Files")?
            .iter()
            .find(|f| f["Name"] == name)
            .ok_or("upgrade IFF not found")?;
        let groups = array(file, "Groups")?;
        let subs = if let Some(level) = level {
            array(
                array(file, "Upgrades")?
                    .get(level)
                    .ok_or("upgrade level out of range")?,
                "Subs",
            )?
        } else {
            array(file, "Subs")?
        };
        let mut result = BTreeMap::new();
        let mut put = |old: &str, new: &str| -> Result<(), String> {
            let key = target(old)?;
            let value = if let Some(v) = new.strip_prefix('V') {
                v.parse::<i16>().map_err(|_| "invalid literal")?
            } else {
                let (t, i) = target(new.strip_prefix('C').ok_or("invalid replacement")?)?;
                lookup(t, i).unwrap_or(0)
            };
            result.insert(key, value);
            Ok(())
        };
        for group in groups {
            if let Some(default) = group.get("DefaultValue").and_then(Value::as_str) {
                for tuning in array(group, "Tuning")? {
                    put(tuning.as_str().unwrap(), default)?;
                }
            }
        }
        for sub in subs {
            let old = text(sub, "Old")?;
            if !old.starts_with('G') {
                put(old, text(sub, "New")?)?;
            }
        }
        for sub in subs {
            if let Some(group) = text(sub, "Old")?.strip_prefix('G') {
                let Some(group) = group.parse::<usize>().ok().and_then(|i| groups.get(i)) else {
                    continue;
                };
                for tuning in array(group, "Tuning")? {
                    put(tuning.as_str().unwrap(), text(sub, "New")?)?;
                }
            }
        }
        Ok(result)
    }
}
