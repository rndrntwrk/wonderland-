// SPDX-License-Identifier: MPL-2.0
//! Guarded standalone Vitaboy authoring through the actual source codecs.
use crate::{
    json_support::{self, JsonEdit},
    sha256,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use wonderland_legacy_formats::{reconstruction, vitaboy, Limits};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssetKind {
    Mesh,
    Animation,
    Skeleton,
    Binding,
    Appearance,
    Outfit,
    PurchasableOutfit,
    Collection,
    HandGroup,
    Fsom,
    Nbhm,
}
impl AssetKind {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "mesh" => Ok(Self::Mesh),
            "animation" => Ok(Self::Animation),
            "skeleton" => Ok(Self::Skeleton),
            "binding" => Ok(Self::Binding),
            "appearance" => Ok(Self::Appearance),
            "outfit" => Ok(Self::Outfit),
            "purchasable-outfit" => Ok(Self::PurchasableOutfit),
            "collection" => Ok(Self::Collection),
            "hand-group" => Ok(Self::HandGroup),
            "fsom" => Ok(Self::Fsom),
            "nbhm" => Ok(Self::Nbhm),
            _ => Err("unknown Vitaboy asset kind".into()),
        }
    }
}
pub struct AssetDocument {
    kind: AssetKind,
    bytes: Vec<u8>,
}
fn decode(kind: AssetKind, bytes: &[u8], limits: &Limits) -> Result<Value, String> {
    admit(kind, bytes, limits)?;
    macro_rules! model {
        ($decoder:ident) => {
            serde_json::to_value(vitaboy::$decoder(bytes, limits).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())
        };
    }
    match kind {
        AssetKind::Mesh => model!(decode_mesh),
        AssetKind::Animation => model!(decode_animation),
        AssetKind::Skeleton => model!(decode_skeleton),
        AssetKind::Binding => model!(decode_binding),
        AssetKind::Appearance => model!(decode_appearance),
        AssetKind::Outfit => model!(decode_outfit),
        AssetKind::PurchasableOutfit => model!(decode_purchasable_outfit),
        AssetKind::Collection => model!(decode_collection),
        AssetKind::HandGroup => model!(decode_hand_group),
        AssetKind::Fsom => serde_json::to_value(
            reconstruction::decode_fsom(bytes, limits).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string()),
        AssetKind::Nbhm => serde_json::to_value(
            reconstruction::decode_nbhm(bytes, limits).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string()),
    }
}
fn admit(kind: AssetKind, bytes: &[u8], limits: &Limits) -> Result<(), String> {
    if bytes.len() > limits.max_input_bytes.min(limits.max_resource_bytes)
        || bytes
            .len()
            .checked_mul(128)
            .and_then(|n| n.checked_add(65536))
            .is_none_or(|n| n > limits.max_total_decoded_bytes)
    {
        return Err("asset document working-copy limit exceeded".into());
    }
    if kind == AssetKind::Fsom {
        super::meshes::admit_fsom(bytes, 128, limits)?;
    }
    Ok(())
}
fn encode(kind: AssetKind, tree: Value, limits: &Limits) -> Result<Vec<u8>, String> {
    macro_rules! model {
        ($ty:ident,$encoder:ident) => {{
            let model: vitaboy::$ty =
                serde_json::from_value(tree.clone()).map_err(|e| e.to_string())?;
            if serde_json::to_value(&model).map_err(|e| e.to_string())? != tree {
                return Err("asset edit contains unknown or omitted fields".into());
            }
            vitaboy::$encoder(&model, limits).map_err(|e| e.to_string())
        }};
    }
    match kind {
        AssetKind::Mesh => model!(Mesh, encode_mesh),
        AssetKind::Animation => model!(Animation, encode_animation),
        AssetKind::Skeleton => model!(Skeleton, encode_skeleton),
        AssetKind::Binding => model!(Binding, encode_binding),
        AssetKind::Appearance => model!(Appearance, encode_appearance),
        AssetKind::Outfit => model!(Outfit, encode_outfit),
        AssetKind::PurchasableOutfit => model!(PurchasableOutfit, encode_purchasable_outfit),
        AssetKind::Collection => model!(Collection, encode_collection),
        AssetKind::HandGroup => model!(HandGroup, encode_hand_group),
        AssetKind::Fsom => {
            let model: reconstruction::FsomMesh =
                serde_json::from_value(tree.clone()).map_err(|e| e.to_string())?;
            if serde_json::to_value(&model).map_err(|e| e.to_string())? != tree {
                return Err("FSOm metadata shape changed".into());
            }
            reconstruction::encode_fsom(&model, limits).map_err(|e| e.to_string())
        }
        AssetKind::Nbhm => {
            let model: reconstruction::Nbhm =
                serde_json::from_value(tree.clone()).map_err(|e| e.to_string())?;
            if serde_json::to_value(&model).map_err(|e| e.to_string())? != tree {
                return Err("NBHm metadata shape changed".into());
            }
            reconstruction::encode_nbhm(&model, limits).map_err(|e| e.to_string())
        }
    }
}
impl AssetDocument {
    pub fn import(kind: AssetKind, bytes: &[u8], limits: &Limits) -> Result<Self, String> {
        admit(kind, bytes, limits)?;
        // Decoded structures and JSON are transient; the editable document retains exact source bytes.
        decode(kind, bytes, limits)?;
        Ok(Self {
            kind,
            bytes: bytes.to_vec(),
        })
    }
    pub fn kind(&self) -> AssetKind {
        self.kind
    }
    pub fn source_sha256(&self) -> String {
        sha256(&self.bytes)
    }
    pub fn export(&self, limits: &Limits) -> Result<Vec<u8>, String> {
        admit(self.kind, &self.bytes, limits)?;
        Ok(self.bytes.clone())
    }
    pub fn data_json(&self, limits: &Limits) -> Result<Value, String> {
        let data = decode(self.kind, &self.bytes, limits)?;
        // Admission before exposing a second owned copy or serializing an edit package.
        json_support::encode(&data, limits)?;
        Ok(data)
    }
    pub fn metadata_json(&self, limits: &Limits) -> Result<String, String> {
        let data = self.data_json(limits)?;
        let metadata = json!({"schema":"wonderland.creator.vitaboy.v1","kind":self.kind,"source_sha256":self.source_sha256(),"float_encoding":"ieee754-binary32-u32-bits","data":data});
        String::from_utf8(json_support::encode(&metadata, limits)?).map_err(|e| e.to_string())
    }
    pub fn apply(
        &mut self,
        expected: &str,
        edits: &[JsonEdit],
        limits: &Limits,
    ) -> Result<(), String> {
        if expected.to_ascii_lowercase() != self.source_sha256() {
            return Err("asset source SHA-256 conflict".into());
        }
        let current = self.data_json(limits)?;
        let candidate = json_support::apply(&current, edits, limits)?;
        if candidate == current {
            return Ok(());
        }
        let bytes = encode(self.kind, candidate.clone(), limits)?;
        let reopened = decode(self.kind, &bytes, limits)?;
        if candidate != reopened {
            return Err(
                "asset edited metadata is not representable by source binary format".into(),
            );
        }
        // Full validation and reopen complete before publishing the candidate.
        self.bytes = bytes;
        Ok(())
    }
}
