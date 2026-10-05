// SPDX-License-Identifier: MPL-2.0
//! CPU-only FSOm and NBHm source layouts. Source: tso.files/RC/{DGRP3DMesh,
//! DGRP3DGeometry,DGRP3DVert,NBHm}.cs at
//! 4c6b3e8f5835b228723caea3c9f683c62f244f73. No graphics object, texture lookup,
//! geometry reconstruction, normal generation or filesystem access occurs here.

use crate::vitaboy::encode::{encode, Writer};
use crate::{
    reader::Reader,
    vitaboy::{finite_le, invalid, unsupported, DecodeBudget, F32Bits, Vector3Bits},
    Error, ErrorKind, Limits, Result,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsomVertex {
    pub position: Vector3Bits,
    pub texture_coordinate: [F32Bits; 2],
    /// Version 1 has no stored normal. Its exact source default is +0.0;
    /// renderer normal generation is a separate operation.
    pub normal: Vector3Bits,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsomGeometry {
    pub pixel_sprite: u16,
    pub pixel_direction: u16,
    pub vertices: Vec<FsomVertex>,
    pub indices: Vec<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FsomTextureReference {
    Sprite { index: u16, rotation: u16 },
    CustomTexture { id: u16 },
}

impl FsomGeometry {
    pub fn texture_reference(&self) -> FsomTextureReference {
        if self.pixel_direction == u16::MAX {
            FsomTextureReference::CustomTexture {
                id: self.pixel_sprite,
            }
        } else {
            FsomTextureReference::Sprite {
                index: self.pixel_sprite,
                rotation: self.pixel_direction,
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsomMesh {
    pub version: i32,
    pub reconstruct_version: i32,
    pub name: String,
    /// Source dynamic group ordering, retaining empty groups and submesh order.
    pub groups: Vec<Vec<FsomGeometry>>,
    /// 0 = none, 1 = normal, 2 = portal.
    pub mask_type: i32,
    pub depth_mask: Option<FsomGeometry>,
    pub bounds: [Vector3Bits; 2],
}

impl FsomMesh {
    /// The original renderer rejects nonzero reconstruction versions below 2.
    /// Editing/inspection retains them so callers can make that decision.
    pub fn is_current_reconstruction(&self) -> bool {
        self.reconstruct_version == 0 || self.reconstruct_version >= 2
    }
}

fn count(r: &mut Reader<'_>, limit: usize, limits: &Limits, label: &str) -> Result<usize> {
    let value = r.i32_le()?;
    if value < 0 {
        return Err(invalid(r.position() - 4, label));
    }
    limits.check_count(value as usize, limit, r.position() - 4, label)?;
    Ok(value as usize)
}
fn vector(r: &mut Reader<'_>) -> Result<Vector3Bits> {
    Ok([finite_le(r)?, finite_le(r)?, finite_le(r)?])
}
fn add_total(
    total: &mut usize,
    value: usize,
    limit: usize,
    limits: &Limits,
    label: &str,
) -> Result<()> {
    *total = total
        .checked_add(value)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, label))?;
    limits.check_count(*total, limit, 0, label)
}

#[derive(Default)]
struct Counts {
    geometries: usize,
    vertices: usize,
    indices: usize,
}

fn geometry(
    r: &mut Reader<'_>,
    version: i32,
    budget: &mut DecodeBudget<'_>,
    counts: &mut Counts,
) -> Result<FsomGeometry> {
    let limits = budget.limits;
    add_total(
        &mut counts.geometries,
        1,
        limits.max_entries,
        limits,
        "FSOm total geometries",
    )?;
    let pixel_sprite = r.u16_le()?;
    let pixel_direction = r.u16_le()?;
    let n = count(r, limits.max_vertices, limits, "FSOm vertices")?;
    add_total(
        &mut counts.vertices,
        n,
        limits.max_vertices,
        limits,
        "FSOm total vertices",
    )?;
    budget.reserve::<FsomVertex>(n, limits.max_vertices, r.position(), "FSOm vertices")?;
    let mut vertices = Vec::with_capacity(n);
    for _ in 0..n {
        let position = vector(r)?;
        let texture_coordinate = [finite_le(r)?, finite_le(r)?];
        let normal = if version > 1 {
            vector(r)?
        } else {
            [F32Bits(0); 3]
        };
        vertices.push(FsomVertex {
            position,
            texture_coordinate,
            normal,
        });
    }
    let limit = limits.max_entries.saturating_mul(3);
    let n = count(r, limit, limits, "FSOm triangle indices")?;
    if !n.is_multiple_of(3) {
        return Err(invalid(
            r.position() - 4,
            "FSOm indices must form complete triangles",
        ));
    }
    add_total(&mut counts.indices, n, limit, limits, "FSOm total indices")?;
    budget.reserve::<i32>(n, limit, r.position(), "FSOm triangle indices")?;
    let mut indices = Vec::with_capacity(n);
    for _ in 0..n {
        let index = r.i32_le()?;
        if index < 0 || index as usize >= vertices.len() {
            return Err(invalid(r.position() - 4, "FSOm index outside vertices"));
        }
        indices.push(index);
    }
    Ok(FsomGeometry {
        pixel_sprite,
        pixel_direction,
        vertices,
        indices,
    })
}

fn bounds_valid(bounds: &[Vector3Bits; 2]) -> Result<()> {
    if bounds.iter().flatten().any(|f| !f.is_finite())
        || (0..3).any(|i| bounds[0][i].get() > bounds[1][i].get())
    {
        return Err(invalid(0, "invalid FSOm bounds"));
    }
    Ok(())
}

/// Decode an uncompressed FSOm stream as written by DGRP3DMesh.Save(Stream).
pub fn decode_fsom_payload(bytes: &[u8], limits: &Limits) -> Result<FsomMesh> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    if r.read_bytes(4)? != b"FSOm" {
        return Err(invalid(0, "FSOm signature"));
    }
    let version = r.i32_le()?;
    if !(1..=3).contains(&version) {
        return Err(unsupported(4, "unsupported FSOm version"));
    }
    let reconstruct_version = r.i32_le()?;
    let len = r.u8()? as usize;
    budget.reserve::<u8>(len, limits.max_string_bytes, r.position(), "FSOm name")?;
    let raw_name = r.read_bytes(len)?;
    if !raw_name.is_ascii() {
        return Err(unsupported(r.position() - len, "non-ASCII FSOm name"));
    }
    let name = String::from_utf8(raw_name.to_vec()).expect("ASCII is UTF-8");
    let n = count(&mut r, limits.max_entries, limits, "FSOm dynamic groups")?;
    budget.reserve::<Vec<FsomGeometry>>(
        n,
        limits.max_entries,
        r.position(),
        "FSOm dynamic groups",
    )?;
    let mut groups = Vec::with_capacity(n);
    let mut counts = Counts::default();
    for _ in 0..n {
        let n = count(&mut r, limits.max_entries, limits, "FSOm submeshes")?;
        budget.reserve::<FsomGeometry>(n, limits.max_entries, r.position(), "FSOm submeshes")?;
        let mut group = Vec::with_capacity(n);
        for _ in 0..n {
            group.push(geometry(&mut r, version, &mut budget, &mut counts)?);
        }
        groups.push(group);
    }
    let mask_type = if version > 2 { r.i32_le()? } else { 0 };
    if !(0..=2).contains(&mask_type) {
        return Err(unsupported(r.position() - 4, "unsupported FSOm mask type"));
    }
    let depth_mask = if mask_type != 0 {
        Some(geometry(&mut r, version, &mut budget, &mut counts)?)
    } else {
        None
    };
    let bounds = [vector(&mut r)?, vector(&mut r)?];
    bounds_valid(&bounds)?;
    if r.remaining() != 0 {
        return Err(unsupported(r.position(), "unrecognized trailing FSOm data"));
    }
    Ok(FsomMesh {
        version,
        reconstruct_version,
        name,
        groups,
        mask_type,
        depth_mask,
        bounds,
    })
}

fn le16(w: &mut Writer<'_>, n: u16) -> Result<()> {
    w.put(&n.to_le_bytes())
}
fn le32(w: &mut Writer<'_>, n: i32) -> Result<()> {
    w.put(&n.to_le_bytes())
}
fn write_count(
    w: &mut Writer<'_>,
    n: usize,
    limit: usize,
    limits: &Limits,
    label: &str,
) -> Result<()> {
    limits.check_count(n, limit, 0, label)?;
    le32(
        w,
        i32::try_from(n).map_err(|_| invalid(0, "FSOm/NBHm count exceeds i32"))?,
    )
}
fn write_geometry(
    w: &mut Writer<'_>,
    value: &FsomGeometry,
    version: i32,
    limits: &Limits,
    counts: &mut Counts,
) -> Result<()> {
    add_total(
        &mut counts.geometries,
        1,
        limits.max_entries,
        limits,
        "FSOm total geometries",
    )?;
    add_total(
        &mut counts.vertices,
        value.vertices.len(),
        limits.max_vertices,
        limits,
        "FSOm total vertices",
    )?;
    le16(w, value.pixel_sprite)?;
    le16(w, value.pixel_direction)?;
    w.retain::<FsomVertex>(value.vertices.capacity())?;
    write_count(
        w,
        value.vertices.len(),
        limits.max_vertices,
        limits,
        "FSOm vertices",
    )?;
    for vertex in &value.vertices {
        for f in vertex.position.into_iter().chain(vertex.texture_coordinate) {
            w.float(f)?;
        }
        if version > 1 {
            for f in vertex.normal {
                w.float(f)?;
            }
        } else if vertex.normal != [F32Bits(0); 3] {
            return Err(invalid(0, "FSOm v1 has no stored normal fields"));
        }
    }
    if !value.indices.len().is_multiple_of(3) {
        return Err(invalid(0, "FSOm indices must form complete triangles"));
    }
    add_total(
        &mut counts.indices,
        value.indices.len(),
        limits.max_entries.saturating_mul(3),
        limits,
        "FSOm total indices",
    )?;
    w.retain::<i32>(value.indices.capacity())?;
    write_count(
        w,
        value.indices.len(),
        limits.max_entries.saturating_mul(3),
        limits,
        "FSOm indices",
    )?;
    for &index in &value.indices {
        if index < 0 || index as usize >= value.vertices.len() {
            return Err(invalid(0, "FSOm index outside vertices"));
        }
        le32(w, index)?;
    }
    Ok(())
}

pub fn encode_fsom_payload(value: &FsomMesh, limits: &Limits) -> Result<Vec<u8>> {
    if !(1..=3).contains(&value.version) {
        return Err(unsupported(4, "unsupported FSOm version"));
    }
    if !(0..=2).contains(&value.mask_type)
        || (value.mask_type != 0) != value.depth_mask.is_some()
        || (value.version < 3 && value.mask_type != 0)
    {
        return Err(invalid(0, "FSOm mask type and geometry disagree"));
    }
    bounds_valid(&value.bounds)?;
    encode(limits, |w| write_fsom_payload(w, value, limits))
}

fn write_fsom_payload(w: &mut Writer<'_>, value: &FsomMesh, limits: &Limits) -> Result<()> {
    w.put(b"FSOm")?;
    le32(w, value.version)?;
    le32(w, value.reconstruct_version)?;
    w.string(&value.name, false)?;
    w.retain::<Vec<FsomGeometry>>(value.groups.capacity())?;
    write_count(
        w,
        value.groups.len(),
        limits.max_entries,
        limits,
        "FSOm groups",
    )?;
    let mut counts = Counts::default();
    for group in &value.groups {
        w.retain::<FsomGeometry>(group.capacity())?;
        write_count(w, group.len(), limits.max_entries, limits, "FSOm submeshes")?;
        for geometry in group {
            write_geometry(w, geometry, value.version, limits, &mut counts)?;
        }
    }
    if value.version > 2 {
        le32(w, value.mask_type)?;
    }
    if let Some(mask) = &value.depth_mask {
        write_geometry(w, mask, value.version, limits, &mut counts)?;
    }
    for &f in value.bounds.iter().flatten() {
        w.float(f)?;
    }
    Ok(())
}

// miniz_oxide's fixed dictionary/tables are below this conservative workspace
// allowance. The backend is pure Rust, pinned through Cargo.lock, and allocates
// no additional output buffer; reads fill the exact checked ISIZE allocation.
const INFLATE_WORKSPACE: usize = 256 * 1024;

fn gzip_payload(bytes: &[u8], limits: &Limits) -> Result<Vec<u8>> {
    limits.check_input(bytes)?;
    limits.check_count(
        bytes.len(),
        limits.max_resource_bytes,
        0,
        "gzip resource bytes",
    )?;
    let mut r = Reader::new(bytes);
    if r.read_bytes(3)? != [0x1f, 0x8b, 8] {
        return Err(invalid(0, "gzip signature/method"));
    }
    let flags = r.u8()?;
    if flags & 0xe0 != 0 {
        return Err(invalid(3, "reserved gzip flags"));
    }
    r.read_bytes(6)?;
    if flags & 4 != 0 {
        let len = r.u16_le()? as usize;
        r.read_bytes(len)?;
    }
    for flag in [8, 16] {
        if flags & flag != 0 {
            let mut length = 0usize;
            while r.u8()? != 0 {
                length += 1;
                limits.check_count(
                    length,
                    limits.max_string_bytes,
                    r.position(),
                    "gzip optional string",
                )?;
            }
        }
    }
    if flags & 2 != 0 {
        let checksum = crc32fast::hash(&bytes[..r.position()]) as u16;
        if r.u16_le()? != checksum {
            return Err(invalid(r.position() - 2, "gzip header checksum"));
        }
    }
    let end = bytes
        .len()
        .checked_sub(8)
        .ok_or_else(|| invalid(0, "gzip trailer missing"))?;
    if r.position() > end {
        return Err(invalid(r.position(), "gzip payload/trailer overlap"));
    }
    let mut tail = Reader::new(&bytes[end..]);
    let checksum = tail.u32_le()?;
    let expected = tail.u32_le()? as usize;
    limits.check_count(
        expected,
        limits.max_resource_bytes,
        end + 4,
        "inflated resource bytes",
    )?;
    let allocation = expected
        .checked_add(1)
        .ok_or_else(|| invalid(0, "gzip output size overflow"))?;
    let total = allocation
        .checked_add(bytes.len())
        .and_then(|n| n.checked_add(INFLATE_WORKSPACE))
        .ok_or_else(|| invalid(0, "gzip allocation overflow"))?;
    limits.check_count(
        total,
        limits.max_total_decoded_bytes,
        0,
        "gzip source/output/workspace allocation",
    )?;
    let compressed = &bytes[r.position()..end];
    let mut output = Vec::new();
    output
        .try_reserve_exact(allocation)
        .map_err(|_| Error::new(ErrorKind::LimitExceeded, 0, "gzip output allocation"))?;
    output.resize(allocation, 0);
    let mut decoder = flate2::Decompress::new(false);
    let status = decoder
        .decompress(compressed, &mut output, flate2::FlushDecompress::Finish)
        .map_err(|_| invalid(r.position(), "invalid deflate data"))?;
    if status != flate2::Status::StreamEnd
        || decoder.total_out() != expected as u64
        || decoder.total_in() != compressed.len() as u64
    {
        return Err(invalid(
            r.position(),
            "incomplete deflate stream, length mismatch or trailing data",
        ));
    }
    output.truncate(expected);
    if crc32fast::hash(&output) != checksum {
        return Err(invalid(end, "gzip payload checksum"));
    }
    Ok(output)
}

/// Decode exactly one bounded gzip-wrapped FSOm member, as stored in an FSOM
/// IFF chunk or .fsom file. Header CRC, payload CRC, ISIZE and stream end are
/// checked. Additional members/trailing bytes are rejected.
pub fn decode_fsom(bytes: &[u8], limits: &Limits) -> Result<FsomMesh> {
    let payload = gzip_payload(bytes, limits)?;
    let retained = bytes
        .len()
        .checked_add(payload.capacity())
        .ok_or_else(|| invalid(0, "gzip retention overflow"))?;
    let mut remaining = *limits;
    remaining.max_total_decoded_bytes = limits
        .max_total_decoded_bytes
        .checked_sub(retained)
        .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, 0, "FSOm decoded allocation"))?;
    decode_fsom_payload(&payload, &remaining)
}

/// Deterministic gzip wrapper with lossless stored DEFLATE blocks. Compression
/// choices and timestamps cannot perturb release identity. Readers accept both
/// stored and compressed original DEFLATE streams.
pub fn encode_fsom(value: &FsomMesh, limits: &Limits) -> Result<Vec<u8>> {
    let (raw_size, retained) =
        crate::vitaboy::encode::measure(limits, |w| write_fsom_payload(w, value, limits))?;
    // Run public layout validation before constructing either output buffer.
    if !(1..=3).contains(&value.version) {
        return Err(unsupported(4, "unsupported FSOm version"));
    }
    if !(0..=2).contains(&value.mask_type)
        || (value.mask_type != 0) != value.depth_mask.is_some()
        || (value.version < 3 && value.mask_type != 0)
    {
        return Err(invalid(0, "FSOm mask type and geometry disagree"));
    }
    bounds_valid(&value.bounds)?;
    let blocks = raw_size.max(1).div_ceil(65535);
    let size = raw_size
        .checked_add(
            blocks
                .checked_mul(5)
                .ok_or_else(|| invalid(0, "gzip size overflow"))?,
        )
        .and_then(|n| n.checked_add(18))
        .ok_or_else(|| invalid(0, "gzip size overflow"))?;
    let total = retained
        .checked_add(raw_size)
        .and_then(|n| n.checked_add(size))
        .ok_or_else(|| invalid(0, "gzip authoring budget overflow"))?;
    limits.check_count(
        total,
        limits.max_total_decoded_bytes,
        0,
        "FSOm model/payload/gzip allocation",
    )?;
    limits.check_count(
        size,
        limits.max_resource_bytes,
        0,
        "gzip output resource bytes",
    )?;
    limits.check_count(size, limits.max_input_bytes, 0, "gzip output bytes")?;
    let payload = encode_fsom_payload(value, limits)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| Error::new(ErrorKind::LimitExceeded, 0, "gzip output allocation"))?;
    bytes.extend_from_slice(&[0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255]);
    if payload.is_empty() {
        bytes.extend_from_slice(&[1, 0, 0, 255, 255]);
    } else {
        for (i, chunk) in payload.chunks(65535).enumerate() {
            bytes.push(u8::from(i + 1 == blocks));
            let length = chunk.len() as u16;
            bytes.extend_from_slice(&length.to_le_bytes());
            bytes.extend_from_slice(&(!length).to_le_bytes());
            bytes.extend_from_slice(chunk);
        }
    }
    bytes.extend_from_slice(&crc32fast::hash(&payload).to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    Ok(bytes)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NbhmHouse {
    pub house_number: i16,
    pub position: Vector3Bits,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nbhm {
    pub version: i32,
    pub houses: Vec<NbhmHouse>,
    pub has_model: u8,
}
impl Nbhm {
    /// Source dictionary assignments retain the last occurrence of a house ID.
    /// The stored vector still preserves every record for an exact round trip.
    pub fn house(&self, id: i16) -> Option<&NbhmHouse> {
        self.houses.iter().rev().find(|h| h.house_number == id)
    }
}

pub fn decode_nbhm(bytes: &[u8], limits: &Limits) -> Result<Nbhm> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    if r.read_bytes(4)? != b"NBHm" {
        return Err(invalid(0, "NBHm signature"));
    }
    let version = r.i32_le()?;
    let n = count(&mut r, limits.max_entries, limits, "NBHm houses")?;
    budget.reserve::<NbhmHouse>(n, limits.max_entries, r.position(), "NBHm houses")?;
    let mut houses = Vec::with_capacity(n);
    for _ in 0..n {
        houses.push(NbhmHouse {
            house_number: r.i16_le()?,
            position: vector(&mut r)?,
        });
    }
    let has_model = r.u8()?;
    if r.remaining() != 0 {
        return Err(unsupported(
            r.position(),
            "unrecognized NBHm model/trailing data",
        ));
    }
    Ok(Nbhm {
        version,
        houses,
        has_model,
    })
}

pub fn encode_nbhm(value: &Nbhm, limits: &Limits) -> Result<Vec<u8>> {
    encode(limits, |w| {
        w.put(b"NBHm")?;
        le32(w, value.version)?;
        w.retain::<NbhmHouse>(value.houses.capacity())?;
        write_count(
            w,
            value.houses.len(),
            limits.max_entries,
            limits,
            "NBHm houses",
        )?;
        for house in &value.houses {
            le16(w, house.house_number as u16)?;
            for f in house.position {
                w.float(f)?;
            }
        }
        w.u8(value.has_model)
    })
}
