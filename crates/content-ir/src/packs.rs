// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
//! Portable immutable resource packs: explicit little-endian header/index and contiguous bytes.
use crate::manifest::{
    validate_id, AssetManifest, Digest, ManifestError, ManifestLimits, ManifestResult,
    ResourceKind, PACK_VERSION,
};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use wonderland_legacy_formats::reader::Reader;

pub const PACK_MAGIC: &[u8; 8] = b"WLDPACK\0";
pub const HEADER_LEN: usize = 32;

#[derive(Clone, Debug)]
pub struct PackLimits {
    pub max_pack_bytes: usize,
    pub max_resource_bytes: usize,
    pub max_resources: usize,
    pub max_id_bytes: usize,
}
impl Default for PackLimits {
    fn default() -> Self {
        Self {
            max_pack_bytes: 128 * 1024 * 1024,
            max_resource_bytes: 32 * 1024 * 1024,
            max_resources: 100_000,
            max_id_bytes: 512,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PackResource {
    pub id: String,
    pub kind: ResourceKind,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackEntry {
    pub id: String,
    pub kind: ResourceKind,
    pub content_hash: Digest,
    pub offset: usize,
    pub byte_len: usize,
}
#[derive(Debug)]
pub struct PackIndex<'a> {
    bytes: &'a [u8],
    hash: Digest,
    entries: BTreeMap<String, PackEntry>,
    payload_start: usize,
}
impl<'a> PackIndex<'a> {
    pub fn hash(&self) -> &Digest {
        &self.hash
    }
    pub fn entries(&self) -> &BTreeMap<String, PackEntry> {
        &self.entries
    }
    pub fn get(&self, id: &str) -> ManifestResult<&'a [u8]> {
        let entry = self
            .entries
            .get(id)
            .ok_or_else(|| error("resource is absent from pack"))?;
        let start = self
            .payload_start
            .checked_add(entry.offset)
            .ok_or_else(|| error("resource offset overflow"))?;
        let end = start
            .checked_add(entry.byte_len)
            .ok_or_else(|| error("resource length overflow"))?;
        self.bytes
            .get(start..end)
            .ok_or_else(|| error("resource range is outside pack"))
    }
    pub fn validate_manifest(
        &self,
        manifest: &AssetManifest,
        limits: &ManifestLimits,
    ) -> ManifestResult<()> {
        manifest.validate(limits)?;
        let pack = manifest
            .packs
            .get(&self.hash)
            .ok_or_else(|| error("pack hash is not selected by this manifest"))?;
        if pack.byte_len != self.bytes.len() as u64 || pack.resources.len() != self.entries.len() {
            return Err(error("pack size or member count disagrees with manifest"));
        }
        for (id, entry) in &self.entries {
            let resource = manifest
                .resources
                .get(id)
                .ok_or_else(|| error("pack contains a resource absent from manifest"))?;
            if resource.pack != self.hash
                || resource.content_hash != entry.content_hash
                || resource.kind != entry.kind
            {
                return Err(error(
                    "pack member hash, kind or owner disagrees with manifest",
                ));
            }
        }
        Ok(())
    }
}

pub fn build_pack(resources: &[PackResource], limits: &PackLimits) -> ManifestResult<Vec<u8>> {
    if resources.is_empty()
        || resources.len() > limits.max_resources
        || resources.len() > u32::MAX as usize
    {
        return Err(error("invalid pack resource count"));
    }
    let mut ordered: Vec<_> = resources.iter().collect();
    ordered.sort_by(|a, b| a.id.cmp(&b.id));
    let mut index_len = 0usize;
    let mut payload_len = 0usize;
    let mut previous: Option<&str> = None;
    for resource in &ordered {
        validate_id(&resource.id, limits.max_id_bytes.min(u16::MAX as usize))?;
        if previous == Some(&resource.id) {
            return Err(error("duplicate resource ID in pack"));
        }
        previous = Some(&resource.id);
        if resource.bytes.len() > limits.max_resource_bytes {
            return Err(error("resource byte limit exceeded"));
        }
        index_len = index_len
            .checked_add(52)
            .and_then(|n| n.checked_add(resource.id.len()))
            .ok_or_else(|| error("pack index size overflow"))?;
        payload_len = payload_len
            .checked_add(resource.bytes.len())
            .ok_or_else(|| error("pack payload size overflow"))?;
    }
    let total = HEADER_LEN
        .checked_add(index_len)
        .and_then(|n| n.checked_add(payload_len))
        .ok_or_else(|| error("pack size overflow"))?;
    if total > limits.max_pack_bytes {
        return Err(error("pack byte limit exceeded"));
    }
    let mut output = Vec::with_capacity(total);
    output.extend_from_slice(PACK_MAGIC);
    output.extend_from_slice(&PACK_VERSION.to_le_bytes());
    output.extend_from_slice(&(ordered.len() as u32).to_le_bytes());
    output.extend_from_slice(&(index_len as u64).to_le_bytes());
    output.extend_from_slice(&(payload_len as u64).to_le_bytes());
    let mut offset = 0usize;
    for resource in &ordered {
        output.extend_from_slice(&(resource.id.len() as u16).to_le_bytes());
        output.extend_from_slice(resource.id.as_bytes());
        output.push(kind_byte(resource.kind));
        output.push(0);
        output.extend_from_slice(&Sha256::digest(&resource.bytes));
        output.extend_from_slice(&(offset as u64).to_le_bytes());
        output.extend_from_slice(&(resource.bytes.len() as u64).to_le_bytes());
        offset += resource.bytes.len(); // checked by total payload calculation above
    }
    for resource in &ordered {
        output.extend_from_slice(&resource.bytes);
    }
    Ok(output)
}
pub fn verify_pack<'a>(
    bytes: &'a [u8],
    expected: &Digest,
    limits: &PackLimits,
) -> ManifestResult<PackIndex<'a>> {
    if bytes.len() > limits.max_pack_bytes || bytes.len() < HEADER_LEN {
        return Err(error("pack size outside permitted range"));
    }
    if &Digest::of(bytes) != expected {
        return Err(error("pack digest mismatch"));
    }
    let mut reader = Reader::new(bytes);
    if read(reader.read_bytes(8))? != PACK_MAGIC {
        return Err(error("invalid pack magic"));
    }
    if read(reader.u32_le())? != PACK_VERSION {
        return Err(error("unsupported pack version"));
    }
    let count = read(reader.u32_le())? as usize;
    if count == 0 || count > limits.max_resources {
        return Err(error("pack resource count limit exceeded"));
    }
    let index_len = size(read(reader.u64_le())?)?;
    let payload_len = size(read(reader.u64_le())?)?;
    let payload_start = HEADER_LEN
        .checked_add(index_len)
        .ok_or_else(|| error("pack index length overflow"))?;
    if payload_start.checked_add(payload_len) != Some(bytes.len())
        || count.checked_mul(52).is_none_or(|n| n > index_len)
    {
        return Err(error("pack header/index/payload lengths disagree"));
    }
    let mut index_reader = Reader::new(
        bytes
            .get(HEADER_LEN..payload_start)
            .ok_or_else(|| error("index lies outside pack"))?,
    );
    let mut entries = BTreeMap::new();
    let mut cursor = 0usize;
    let mut previous: Option<String> = None;
    for _ in 0..count {
        let id_len = read(index_reader.u16_le())? as usize;
        if id_len == 0 || id_len > limits.max_id_bytes {
            return Err(error("pack identifier byte limit exceeded"));
        }
        let id = std::str::from_utf8(read(index_reader.read_bytes(id_len))?)
            .map_err(|_| error("pack identifier is not UTF-8"))?
            .to_string();
        validate_id(&id, limits.max_id_bytes)?;
        if previous.as_ref().is_some_and(|p| p >= &id) {
            return Err(error("pack IDs are duplicated or not in canonical order"));
        }
        previous = Some(id.clone());
        let kind = byte_kind(read(index_reader.u8())?)?;
        if read(index_reader.u8())? != 0 {
            return Err(error("reserved pack flags must be zero"));
        }
        let stored_hash = read(index_reader.read_bytes(32))?;
        let offset = size(read(index_reader.u64_le())?)?;
        let byte_len = size(read(index_reader.u64_le())?)?;
        if offset != cursor || byte_len > limits.max_resource_bytes {
            return Err(error(
                "noncanonical, overlapping or oversized resource range",
            ));
        }
        cursor = cursor
            .checked_add(byte_len)
            .ok_or_else(|| error("resource range overflow"))?;
        if cursor > payload_len {
            return Err(error("resource lies outside payload"));
        }
        let data = &bytes[payload_start + offset..payload_start + cursor];
        if Sha256::digest(data).as_slice() != stored_hash {
            return Err(error("pack resource digest mismatch"));
        }
        entries.insert(
            id.clone(),
            PackEntry {
                id,
                kind,
                content_hash: Digest::of(data),
                offset,
                byte_len,
            },
        );
    }
    if index_reader.remaining() != 0 || cursor != payload_len {
        return Err(error(
            "pack index or payload contains unclaimed trailing bytes",
        ));
    }
    Ok(PackIndex {
        bytes,
        hash: expected.clone(),
        entries,
        payload_start,
    })
}

fn size(value: u64) -> ManifestResult<usize> {
    usize::try_from(value).map_err(|_| error("length is not representable on this target"))
}
fn read<T>(result: wonderland_legacy_formats::Result<T>) -> ManifestResult<T> {
    result.map_err(|e| ManifestError(e.to_string()))
}
fn error(message: &str) -> ManifestError {
    ManifestError(message.into())
}
fn kind_byte(kind: ResourceKind) -> u8 {
    match kind {
        ResourceKind::Semantic => 0,
        ResourceKind::Visual => 1,
        ResourceKind::Audio => 2,
        ResourceKind::Opaque => 3,
    }
}
fn byte_kind(value: u8) -> ManifestResult<ResourceKind> {
    match value {
        0 => Ok(ResourceKind::Semantic),
        1 => Ok(ResourceKind::Visual),
        2 => Ok(ResourceKind::Audio),
        3 => Ok(ResourceKind::Opaque),
        _ => Err(error("unsupported resource kind")),
    }
}
