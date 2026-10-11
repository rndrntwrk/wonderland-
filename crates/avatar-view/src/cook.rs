//! C-local portable normalized format, preserving bit patterns and record order.
//! This is a dual-position runtime sidecar, not a glTF fidelity claim.
use crate::*;
use bincode::Options;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use wonderland_render_core::AssetKey;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CookedAvatar {
    pub version: u32,
    pub source_digest: AssetKey,
    pub rig_resource: ResourceKey,
    pub skeleton: Skeleton,
    pub meshes: Vec<(ResourceKey, SourceMesh)>,
    pub animations: Vec<(ResourceKey, Animation)>,
}
impl CookedAvatar {
    fn validate(&self, limits: AvatarLimits) -> Result<()> {
        if self.version != 1
            || self.meshes.len() > limits.max_parts
            || self.animations.len() > limits.max_motions
        {
            return Err(AvatarError::Limit("cooked version/resource count"));
        }
        let rig = Rig::new(self.skeleton.clone(), self.source_digest, limits)?;
        let mut ids = std::collections::BTreeSet::new();
        for (id, mesh) in &self.meshes {
            if !ids.insert(*id) {
                return Err(AvatarError::Invalid("duplicate mesh ID"));
            }
            PreparedMesh::prepare(&rig, mesh, self.source_digest, limits)?;
        }
        ids.clear();
        for (id, animation) in &self.animations {
            if !ids.insert(*id) {
                return Err(AvatarError::Invalid("duplicate animation ID"));
            }
            Clip::new(&rig, animation.clone(), self.source_digest, limits)?;
        }
        Ok(())
    }
    pub fn encode(&self, limits: AvatarLimits) -> Result<Vec<u8>> {
        let size = bincode::serialized_size(self).map_err(|_| AvatarError::Invalid("cook size"))?;
        if size > limits.max_metadata_bytes as u64 {
            return Err(AvatarError::Limit("cooked bytes"));
        }
        self.validate(limits)?;
        let payload = bincode::serialize(self).map_err(|_| AvatarError::Invalid("cook encode"))?;
        let digest = Sha256::digest(&payload);
        let mut output = Vec::with_capacity(payload.len() + 48);
        output.extend_from_slice(b"WCAV0001");
        output.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        output.extend_from_slice(&digest);
        output.extend(payload);
        Ok(output)
    }
    pub fn decode(bytes: &[u8], limits: AvatarLimits) -> Result<Self> {
        if bytes.len() < 48 || &bytes[..8] != b"WCAV0001" {
            return Err(AvatarError::Invalid("cooked header"));
        }
        let length = u64::from_le_bytes(bytes[8..16].try_into().expect("header slice"));
        if length > limits.max_metadata_bytes as u64 {
            return Err(AvatarError::Limit("cooked bytes"));
        }
        if length != bytes.len() as u64 - 48
            || Sha256::digest(&bytes[48..]).as_slice() != &bytes[16..48]
        {
            return Err(AvatarError::Invalid("cooked length/digest"));
        }
        let result: Self = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_limit(length)
            .reject_trailing_bytes()
            .deserialize(&bytes[48..])
            .map_err(|_| AvatarError::Invalid("cooked payload"))?;
        result.validate(limits)?;
        Ok(result)
    }
    /// Negative signed source marker IDs remain in the sidecar. An adapter to A's
    /// unsigned millisecond contract must reject them instead of a wrapping cast.
    pub fn validate_a_time_ids(&self) -> Result<()> {
        if self
            .animations
            .iter()
            .flat_map(|(_, a)| &a.motions)
            .flat_map(|m| &m.time_properties)
            .flat_map(|p| &p.items)
            .any(|p| p.id < 0)
        {
            return Err(AvatarError::Invalid("negative A time-property ID"));
        }
        Ok(())
    }
}
