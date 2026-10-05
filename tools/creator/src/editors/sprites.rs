// SPDX-License-Identifier: MPL-2.0
//! Source-bound SPR2 authoring. The interchange carries exact indexed planes;
//! the legacy decoder and writer remain the authority for binary semantics.
use crate::{resources::check_digest, sha256, ResourceDocument};
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{self, Write},
    marker::PhantomData,
    mem::size_of,
};
use wonderland_legacy_formats::{
    iff::{ChunkKey, IffChunk, IffFile},
    sprites::{self, Palette, Spr2AlphaMode, SpriteByteOrder, SpriteFrame, SpriteKind, SpriteSet},
    Limits,
};

pub const MAX_SPRITE_PACKAGE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
struct ObjectOnly<T>(T);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for ObjectOnly<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
            type Value = ObjectOnly<T>;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                T::deserialize(de::value::MapAccessDeserializer::new(map)).map(ObjectOnly)
            }
        }
        deserializer.deserialize_map(ObjectVisitor(PhantomData))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum AlphaMode {
    Exact,
    Source,
}
impl<'de> Deserialize<'de> for AlphaMode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ModeVisitor;
        impl Visitor<'_> for ModeVisitor {
            type Value = AlphaMode;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("the string exact or source")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "exact" => Ok(AlphaMode::Exact),
                    "source" => Ok(AlphaMode::Source),
                    _ => Err(E::unknown_variant(value, &["exact", "source"])),
                }
            }
        }
        deserializer.deserialize_str(ModeVisitor)
    }
}
impl From<Spr2AlphaMode> for AlphaMode {
    fn from(mode: Spr2AlphaMode) -> Self {
        match mode {
            Spr2AlphaMode::Exact => Self::Exact,
            Spr2AlphaMode::QuantizeLikeSource => Self::Source,
        }
    }
}
impl From<AlphaMode> for Spr2AlphaMode {
    fn from(mode: AlphaMode) -> Self {
        match mode {
            AlphaMode::Exact => Self::Exact,
            AlphaMode::Source => Self::QuantizeLikeSource,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageSpec {
    schema_version: u32,
    source_sha256: String,
    alpha_mode: AlphaMode,
    sprite: ObjectOnly<SpriteSpec>,
    palettes: Vec<ObjectOnly<PaletteSpec>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpriteSpec {
    id: u16,
    resource_sha256: String,
    format_version: u32,
    default_palette_id: u32,
    frames: Vec<ObjectOnly<FrameSpec>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameSpec {
    index: usize,
    width: u16,
    height: u16,
    flags: u32,
    raw_palette_id: u16,
    palette_id: u16,
    transparent_index: u16,
    position: [i16; 2],
    indices_hex: String,
    alpha_hex: String,
    #[serde(deserialize_with = "required_depth")]
    depth_hex: Option<String>,
}
fn required_depth<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaletteSpec {
    id: u16,
    resource_sha256: String,
    format_version: u32,
    reserved_hex: String,
    colors_rgb_hex: String,
}

/// A strict, self-contained editing package. Source identities are immutable;
/// callers may edit JSON to resize/reposition a frame and supply matching planes.
#[derive(Clone, Debug)]
pub struct SpritePackage {
    spec: PackageSpec,
}

#[derive(Clone, Debug, Serialize)]
pub struct SpriteResourceChange {
    pub kind_hex: String,
    pub id: u16,
    pub before_sha256: String,
    pub after_sha256: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct SpriteImportReport {
    pub schema_version: u32,
    pub source_sha256: String,
    pub output_sha256: String,
    pub changed_resources: Vec<SpriteResourceChange>,
    pub changed_frames: Vec<usize>,
    pub quantized_alpha_pixels: usize,
}

fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or_else(|| "sprite authoring size overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or_else(|| "sprite authoring size overflow".into())
}
fn bound(size: usize, max: usize, label: &str) -> Result<(), String> {
    if size > max {
        Err(format!("{label} limit exceeded"))
    } else {
        Ok(())
    }
}
fn package_limit(limits: &Limits) -> usize {
    MAX_SPRITE_PACKAGE_BYTES
        .min(limits.max_input_bytes)
        .min(limits.max_resource_bytes)
}
fn frame_limit(limits: &Limits) -> usize {
    limits.max_frames.min(limits.max_entries)
}

struct Budget {
    used: usize,
    limits: Limits,
}
impl Budget {
    fn new(limits: &Limits) -> Self {
        Self {
            used: 0,
            limits: *limits,
        }
    }
    fn claim(&mut self, bytes: usize, label: &str) -> Result<(), String> {
        self.used = add(self.used, bytes)?;
        bound(self.used, self.limits.max_total_decoded_bytes, label)
    }
    fn codec_limits(&self) -> Limits {
        Limits {
            max_total_decoded_bytes: self.limits.max_total_decoded_bytes - self.used,
            max_frames: frame_limit(&self.limits),
            ..self.limits
        }
    }
}

fn hex_string(bytes: &[u8]) -> Result<String, String> {
    let mut text = String::with_capacity(mul(bytes.len(), 2)?);
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    for &value in bytes {
        text.push(DIGITS[(value >> 4) as usize] as char);
        text.push(DIGITS[(value & 15) as usize] as char);
    }
    Ok(text)
}
fn check_hex(value: &str, length: usize, label: &str) -> Result<(), String> {
    if value.len() != mul(length, 2)? || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "{label} must contain exactly {} hexadecimal characters",
            mul(length, 2)?
        ));
    }
    Ok(())
}
fn nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        b'A'..=b'F' => value - b'A' + 10,
        _ => unreachable!("hex planes are validated before access"),
    }
}
fn byte_at(value: &str, index: usize) -> u8 {
    let bytes = value.as_bytes();
    (nibble(bytes[index * 2]) << 4) | nibble(bytes[index * 2 + 1])
}
fn hex_bytes(value: &str) -> Vec<u8> {
    let count = value.len() / 2;
    let mut bytes = Vec::with_capacity(count);
    for i in 0..count {
        bytes.push(byte_at(value, i));
    }
    bytes
}
fn quantized(alpha: u8) -> u8 {
    ((u16::from(alpha) * 31).div_ceil(255) * 255 / 31) as u8
}
fn check_alpha(alpha: u8, depth: Option<u8>, mode: AlphaMode) -> Result<(), String> {
    if alpha != 0 && alpha != 255 {
        if depth.is_none() {
            return Err("SPR2 partial alpha requires the depth channel".into());
        }
        if mode == AlphaMode::Exact && quantized(alpha) != alpha {
            return Err("SPR2 alpha requires explicit source quantization".into());
        }
    }
    Ok(())
}

impl SpritePackage {
    pub fn from_json(bytes: &[u8], limits: &Limits) -> Result<Self, String> {
        bound(
            bytes.len(),
            package_limit(limits),
            "sprite package JSON bytes",
        )?;
        // Scalar-only map visitors reject unknown values before buffering them.
        // This conservative reserve covers owned/escaped strings, typed vectors
        // (including geometric capacity) and the deserializer's escape scratch.
        bound(
            mul(bytes.len(), 16)?,
            limits.max_total_decoded_bytes,
            "sprite package JSON allocation",
        )?;
        let ObjectOnly(spec): ObjectOnly<PackageSpec> = serde_json::from_slice(bytes)
            .map_err(|error| format!("invalid sprite package JSON: {error}"))?;
        let package = Self { spec };
        package.validate(limits)?;
        Ok(package)
    }

    pub fn to_json(&self, limits: &Limits) -> Result<String, String> {
        self.validate(limits)?;
        let mut counter = JsonWriter {
            bytes: None,
            count: 0,
            max: package_limit(limits),
        };
        serde_json::to_writer(&mut counter, &self.spec).map_err(|error| error.to_string())?;
        let length = add(counter.count, 1)?;
        bound(length, package_limit(limits), "sprite package JSON bytes")?;
        bound(
            add(self.footprint()?, mul(length, 16)?)?,
            limits.max_total_decoded_bytes,
            "sprite package output allocation",
        )?;
        let mut bytes = Vec::with_capacity(length);
        let mut writer = JsonWriter {
            bytes: Some(&mut bytes),
            count: 0,
            max: length - 1,
        };
        serde_json::to_writer(&mut writer, &self.spec).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        String::from_utf8(bytes).map_err(|error| error.to_string())
    }

    pub fn source_sha256(&self) -> &str {
        &self.spec.source_sha256
    }

    pub fn set_pixel(
        &mut self,
        frame: usize,
        position: [usize; 2],
        index: u8,
        alpha: u8,
        depth: Option<u8>,
        limits: &Limits,
    ) -> Result<(), String> {
        self.validate(limits)?;
        let target = &self
            .spec
            .sprite
            .0
            .frames
            .get(frame)
            .ok_or("sprite frame index out of range")?
            .0;
        if position[0] >= usize::from(target.width) || position[1] >= usize::from(target.height) {
            return Err("sprite pixel coordinates out of range".into());
        }
        if target.depth_hex.is_some() != depth.is_some() {
            return Err("pixel depth must match the frame's depth channel".into());
        }
        let palette = &self
            .spec
            .palettes
            .iter()
            .find(|p| p.0.id == target.palette_id)
            .ok_or("sprite palette dependency unavailable")?
            .0;
        if usize::from(index) >= palette.colors_rgb_hex.len() / 6 {
            return Err("sprite pixel index is outside the palette".into());
        }
        if alpha == 0
            && (index != target.transparent_index as u8 || depth.is_some_and(|z| z != 255))
        {
            return Err(
                "transparent pixels require the source transparent index and depth 255".into(),
            );
        }
        check_alpha(alpha, depth, self.spec.alpha_mode)?;
        let pixel = add(mul(position[1], usize::from(target.width))?, position[0])?;
        let offset = mul(pixel, 2)?;
        let index_hex = hex_string(&[index])?;
        let alpha_hex = hex_string(&[alpha])?;
        let depth_hex = depth.map(|value| hex_string(&[value])).transpose()?;
        let target = &mut self.spec.sprite.0.frames[frame].0;
        target
            .indices_hex
            .replace_range(offset..offset + 2, &index_hex);
        target
            .alpha_hex
            .replace_range(offset..offset + 2, &alpha_hex);
        if let (Some(plane), Some(value)) = (&mut target.depth_hex, depth_hex) {
            plane.replace_range(offset..offset + 2, &value);
        }
        Ok(())
    }

    pub fn set_palette_color(&mut self, id: u16, index: usize, rgb: [u8; 3]) -> Result<(), String> {
        let palette = &mut self
            .spec
            .palettes
            .iter_mut()
            .find(|p| p.0.id == id)
            .ok_or("sprite palette dependency unavailable")?
            .0;
        if index >= palette.colors_rgb_hex.len() / 6 {
            return Err("palette color index out of range".into());
        }
        let offset = mul(index, 6)?;
        let encoded = hex_string(&rgb)?;
        palette
            .colors_rgb_hex
            .replace_range(offset..offset + 6, &encoded);
        Ok(())
    }

    pub fn set_alpha_mode(&mut self, mode: Spr2AlphaMode) -> Result<(), String> {
        let mode = mode.into();
        for ObjectOnly(frame) in &self.spec.sprite.0.frames {
            for i in 0..frame.alpha_hex.len() / 2 {
                check_alpha(
                    byte_at(&frame.alpha_hex, i),
                    frame.depth_hex.as_ref().map(|d| byte_at(d, i)),
                    mode,
                )?;
            }
        }
        self.spec.alpha_mode = mode;
        Ok(())
    }

    fn footprint(&self) -> Result<usize, String> {
        let sprite = &self.spec.sprite.0;
        let mut bytes = add(size_of::<Self>(), self.spec.source_sha256.capacity())?;
        bytes = add(bytes, sprite.resource_sha256.capacity())?;
        bytes = add(
            bytes,
            mul(sprite.frames.capacity(), size_of::<ObjectOnly<FrameSpec>>())?,
        )?;
        bytes = add(
            bytes,
            mul(
                self.spec.palettes.capacity(),
                size_of::<ObjectOnly<PaletteSpec>>(),
            )?,
        )?;
        for ObjectOnly(frame) in &sprite.frames {
            bytes = add(bytes, frame.indices_hex.capacity())?;
            bytes = add(bytes, frame.alpha_hex.capacity())?;
            bytes = add(bytes, frame.depth_hex.as_ref().map_or(0, String::capacity))?;
        }
        for ObjectOnly(palette) in &self.spec.palettes {
            bytes = add(bytes, palette.resource_sha256.capacity())?;
            bytes = add(bytes, palette.reserved_hex.capacity())?;
            bytes = add(bytes, palette.colors_rgb_hex.capacity())?;
        }
        Ok(bytes)
    }

    fn validate(&self, limits: &Limits) -> Result<(), String> {
        if self.spec.schema_version != 1 {
            return Err("unsupported sprite package schema_version; expected 1".into());
        }
        check_hex(&self.spec.source_sha256, 32, "source SHA-256")?;
        let sprite = &self.spec.sprite.0;
        check_hex(&sprite.resource_sha256, 32, "sprite resource SHA-256")?;
        if !matches!(sprite.format_version, 1000 | 1001) {
            return Err("unsupported SPR2 format version".into());
        }
        bound(
            sprite.frames.len(),
            frame_limit(limits),
            "sprite package frame count",
        )?;
        bound(
            self.spec.palettes.len(),
            limits.max_entries,
            "sprite package palette count",
        )?;
        // BTree bookkeeping is charged before construction, including a generous
        // node allowance rather than only the stored key/value widths.
        bound(
            add(
                self.footprint()?,
                mul(add(sprite.frames.len(), self.spec.palettes.len())?, 256)?,
            )?,
            limits.max_total_decoded_bytes,
            "sprite package retained allocation",
        )?;
        let mut palettes = BTreeMap::new();
        for ObjectOnly(palette) in &self.spec.palettes {
            if palettes.insert(palette.id, palette).is_some() {
                return Err("duplicate palette ID in sprite package".into());
            }
            check_hex(&palette.resource_sha256, 32, "palette resource SHA-256")?;
            if !matches!(palette.format_version, 0 | 1) {
                return Err("unsupported PALT format version".into());
            }
            check_hex(&palette.reserved_hex, 8, "palette reserved bytes")?;
            if !palette.colors_rgb_hex.len().is_multiple_of(6) {
                return Err("palette RGB plane must contain complete RGB triples".into());
            }
            let count = palette.colors_rgb_hex.len() / 6;
            bound(count, limits.max_entries, "palette color count")?;
            check_hex(&palette.colors_rgb_hex, mul(count, 3)?, "palette RGB plane")?;
        }
        let mut required = BTreeSet::new();
        let mut pixels = 0;
        for (index, ObjectOnly(frame)) in sprite.frames.iter().enumerate() {
            if frame.index != index {
                return Err(
                    "sprite frame indices must preserve exact order without duplicates".into(),
                );
            }
            if !matches!(frame.flags, 1 | 3 | 5 | 7) {
                return Err(
                    "sprite authoring supports only color channel flags 1, 3, 5 or 7".into(),
                );
            }
            let effective =
                if sprite.format_version == 1000 || matches!(frame.raw_palette_id, 0 | 0xa3a3) {
                    sprite.default_palette_id as u16
                } else {
                    frame.raw_palette_id
                };
            if effective != frame.palette_id {
                return Err("raw and effective SPR2 palette IDs disagree".into());
            }
            required.insert(effective);
            let palette = palettes
                .get(&effective)
                .ok_or("sprite palette dependency unavailable")?;
            let colors = palette.colors_rgb_hex.len() / 6;
            if usize::from(frame.transparent_index) >= colors {
                return Err("sprite transparent index is outside the palette".into());
            }
            let count = mul(usize::from(frame.width), usize::from(frame.height))?;
            pixels = add(pixels, count)?;
            bound(pixels, limits.max_pixels, "aggregate sprite package pixels")?;
            check_hex(&frame.indices_hex, count, "sprite index plane")?;
            check_hex(&frame.alpha_hex, count, "sprite alpha plane")?;
            if (frame.flags & 2 != 0) != frame.depth_hex.is_some() {
                return Err(
                    "depth_hex must be null exactly when the frame has no depth channel".into(),
                );
            }
            if let Some(depth) = &frame.depth_hex {
                check_hex(depth, count, "sprite depth plane")?;
            }
            for i in 0..count {
                let pixel_index = byte_at(&frame.indices_hex, i);
                let alpha = byte_at(&frame.alpha_hex, i);
                let depth = frame.depth_hex.as_ref().map(|d| byte_at(d, i));
                if usize::from(pixel_index) >= colors {
                    return Err("sprite pixel index is outside the palette".into());
                }
                if alpha == 0
                    && (pixel_index != frame.transparent_index as u8
                        || depth.is_some_and(|z| z != 255))
                {
                    return Err(
                        "transparent pixels require the source transparent index and depth 255"
                            .into(),
                    );
                }
                check_alpha(alpha, depth, self.spec.alpha_mode)?;
            }
        }
        if required.len() != palettes.len() || required.iter().any(|id| !palettes.contains_key(id))
        {
            return Err("sprite package must contain exactly its required palettes".into());
        }
        Ok(())
    }
}

struct JsonWriter<'a> {
    bytes: Option<&'a mut Vec<u8>>,
    count: usize,
    max: usize,
}
impl Write for JsonWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = self
            .count
            .checked_add(bytes.len())
            .filter(|&n| n <= self.max)
            .ok_or_else(|| io::Error::other("sprite package JSON output limit exceeded"))?;
        if let Some(output) = &mut self.bytes {
            output.extend_from_slice(bytes);
        }
        self.count = next;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn le_u16(bytes: &[u8], at: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..add(at, 2)?)
            .ok_or("truncated SPR2 header")?
            .try_into()
            .unwrap(),
    ))
}
fn le_u32(bytes: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..add(at, 4)?)
            .ok_or("truncated SPR2 header")?
            .try_into()
            .unwrap(),
    ))
}

/// Header-only dependency discovery, never a semantic validation result. The
/// complete existing decoder is always called with the discovered real PALT.
fn dependencies(
    bytes: &[u8],
    budget: &mut Budget,
) -> Result<(BTreeSet<u16>, usize, usize), String> {
    bound(
        bytes.len(),
        budget
            .limits
            .max_resource_bytes
            .min(budget.limits.max_input_bytes),
        "SPR2 resource bytes",
    )?;
    let version = le_u32(bytes, 0)?;
    if !matches!(version, 1000 | 1001) {
        return Err("unsupported SPR2 format version".into());
    }
    let count = le_u32(bytes, if version == 1000 { 4 } else { 8 })? as usize;
    let default = le_u32(bytes, if version == 1000 { 8 } else { 4 })? as u16;
    bound(
        count,
        frame_limit(&budget.limits),
        "sprite source frame count",
    )?;
    budget.claim(mul(count, 256)?, "sprite dependency bookkeeping")?;
    let table_end = add(12, if version == 1000 { mul(count, 4)? } else { 0 })?;
    bound(table_end, bytes.len(), "SPR2 frame table")?;
    let mut cursor = 12;
    let mut previous = None;
    let mut ids = BTreeSet::new();
    let mut pixels = 0;
    for i in 0..count {
        let (start, end) = if version == 1000 {
            let start = le_u32(bytes, add(12, mul(i, 4)?)?)? as usize;
            let end = if i + 1 < count {
                le_u32(bytes, add(12, mul(i + 1, 4)?)?)? as usize
            } else {
                bytes.len()
            };
            if start < table_end
                || start >= end
                || end > bytes.len()
                || previous.is_some_and(|p| start <= p)
            {
                return Err("SPR2 frame offsets are inconsistent".into());
            }
            previous = Some(start);
            (start, end)
        } else {
            if le_u32(bytes, cursor)? != 1001 {
                return Err("unsupported SPR2 frame version".into());
            }
            let size = le_u32(bytes, add(cursor, 4)?)? as usize;
            let start = add(cursor, 8)?;
            cursor = add(start, size)?;
            if cursor > bytes.len() {
                return Err("SPR2 frame size exceeds resource".into());
            }
            (start, cursor)
        };
        if add(start, 16)? > end {
            return Err("truncated SPR2 frame header".into());
        }
        pixels = add(
            pixels,
            mul(
                usize::from(le_u16(bytes, start)?),
                usize::from(le_u16(bytes, add(start, 2)?)?),
            )?,
        )?;
        bound(
            pixels,
            budget.limits.max_pixels,
            "aggregate sprite source pixels",
        )?;
        let raw = le_u16(bytes, add(start, 8)?)?;
        ids.insert(if version == 1000 || matches!(raw, 0 | 0xa3a3) {
            default
        } else {
            raw
        });
    }
    if (version == 1001 && cursor != bytes.len()) || (count == 0 && bytes.len() != 12) {
        return Err("trailing data after SPR2 frame table".into());
    }
    Ok((ids, count, pixels))
}

fn source_footprint(document: &ResourceDocument, limits: &Limits) -> Result<usize, String> {
    limits
        .check_input(document.source_bytes())
        .map_err(|error| error.to_string())?;
    bound(
        document.file().chunks.len(),
        limits.max_entries,
        "sprite source resource count",
    )?;
    mul(
        add(
            document.source_bytes().len(),
            mul(document.file().chunks.capacity(), size_of::<IffChunk>())?,
        )?,
        5,
    )
}

fn decode_source(
    document: &ResourceDocument,
    id: u16,
    budget: &mut Budget,
) -> Result<(SpriteSet, BTreeMap<u16, Palette>), String> {
    let chunk = document.chunk(ChunkKey { kind: *b"SPR2", id })?;
    let (ids, frames, pixels) = dependencies(&chunk.data, budget)?;
    let mut palettes = BTreeMap::new();
    for id in ids {
        let chunk = document.chunk(ChunkKey { kind: *b"PALT", id })?;
        let count = le_u32(&chunk.data, 4)? as usize;
        bound(count, budget.limits.max_entries, "palette color count")?;
        let local = budget.codec_limits();
        budget.claim(
            add(mul(count, 4)?, add(size_of::<Palette>(), 1024)?)?,
            "decoded source palettes",
        )?;
        let palette =
            sprites::decode_palt(&chunk.data, &local).map_err(|error| error.to_string())?;
        palettes.insert(id, palette);
    }
    let local = budget.codec_limits();
    budget.claim(
        add(
            mul(frames, add(size_of::<SpriteFrame>(), size_of::<usize>())?)?,
            mul(pixels, 6)?,
        )?,
        "decoded source sprite",
    )?;
    let set = sprites::decode_spr2_with_palettes(&chunk.data, |id| palettes.get(&id), &local)
        .map_err(|error| error.to_string())?;
    Ok((set, palettes))
}

fn encode_sprite(
    set: &SpriteSet,
    palettes: &BTreeMap<u16, Palette>,
    mode: AlphaMode,
    budget: &mut Budget,
) -> Result<sprites::EncodedSpr2, String> {
    let encoded = sprites::encode_spr2_with_palettes(
        set,
        |id| palettes.get(&id),
        mode.into(),
        &budget.codec_limits(),
    )
    .map_err(|error| error.to_string())?;
    budget.claim(
        add(encoded.bytes.capacity(), mul(set.frames.len(), 32)?)?,
        "encoded sprite working allocation",
    )?;
    Ok(encoded)
}

impl ResourceDocument {
    pub fn export_sprite(&self, id: u16, limits: &Limits) -> Result<SpritePackage, String> {
        let mut budget = Budget::new(limits);
        budget.claim(
            source_footprint(self, limits)?,
            "sprite source document copies",
        )?;
        let (set, palettes) = decode_source(self, id, &mut budget)?;
        // Establish that the selected resource can be expressed by this typed
        // interchange. Unsupported decoded channels remain raw-inspectable.
        drop(
            encode_sprite(&set, &palettes, AlphaMode::Exact, &mut budget).map_err(|error| {
                format!("SPR2 source is not representable for typed authoring: {error}")
            })?,
        );
        let mut package_size = add(size_of::<PackageSpec>(), 128)?;
        package_size = add(
            package_size,
            mul(set.frames.len(), size_of::<ObjectOnly<FrameSpec>>())?,
        )?;
        package_size = add(
            package_size,
            mul(
                palettes.len(),
                add(size_of::<ObjectOnly<PaletteSpec>>(), 80)?,
            )?,
        )?;
        for frame in &set.frames {
            let pixels = mul(usize::from(frame.width), usize::from(frame.height))?;
            package_size = add(
                package_size,
                mul(pixels, if frame.depth.is_some() { 6 } else { 4 })?,
            )?;
        }
        for palette in palettes.values() {
            package_size = add(package_size, mul(palette.colors.len(), 6)?)?;
        }
        budget.claim(package_size, "sprite package buffers")?;
        let mut frames = Vec::with_capacity(set.frames.len());
        for (index, frame) in set.frames.iter().enumerate() {
            let mut alpha = String::with_capacity(mul(frame.rgba.as_ref().unwrap().len(), 2)?);
            const DIGITS: &[u8; 16] = b"0123456789abcdef";
            for color in frame.rgba.as_ref().unwrap() {
                alpha.push(DIGITS[(color[3] >> 4) as usize] as char);
                alpha.push(DIGITS[(color[3] & 15) as usize] as char);
            }
            frames.push(ObjectOnly(FrameSpec {
                index,
                width: frame.width,
                height: frame.height,
                flags: frame.flags,
                raw_palette_id: frame.raw_palette_id.unwrap(),
                palette_id: frame.palette_id,
                transparent_index: frame.transparent_index.unwrap(),
                position: frame.position,
                indices_hex: hex_string(frame.indices.as_ref().unwrap())?,
                alpha_hex: alpha,
                depth_hex: frame
                    .depth
                    .as_ref()
                    .map(|value| hex_string(value))
                    .transpose()?,
            }));
        }
        let mut palette_specs = Vec::with_capacity(palettes.len());
        for (palette_id, palette) in palettes {
            let mut rgb = String::with_capacity(mul(palette.colors.len(), 6)?);
            const DIGITS: &[u8; 16] = b"0123456789abcdef";
            for color in &palette.colors {
                for &channel in &color[..3] {
                    rgb.push(DIGITS[(channel >> 4) as usize] as char);
                    rgb.push(DIGITS[(channel & 15) as usize] as char);
                }
            }
            palette_specs.push(ObjectOnly(PaletteSpec {
                id: palette_id,
                resource_sha256: sha256(
                    &self
                        .chunk(ChunkKey {
                            kind: *b"PALT",
                            id: palette_id,
                        })?
                        .data,
                ),
                format_version: palette.version,
                reserved_hex: hex_string(&palette.reserved)?,
                colors_rgb_hex: rgb,
            }));
        }
        let package = SpritePackage {
            spec: PackageSpec {
                schema_version: 1,
                source_sha256: sha256(self.source_bytes()),
                alpha_mode: AlphaMode::Exact,
                sprite: ObjectOnly(SpriteSpec {
                    id,
                    resource_sha256: sha256(&self.chunk(ChunkKey { kind: *b"SPR2", id })?.data),
                    format_version: set.version,
                    default_palette_id: set.default_palette_id,
                    frames,
                }),
                palettes: palette_specs,
            },
        };
        package.validate(limits)?;
        Ok(package)
    }

    pub fn import_sprite(
        &mut self,
        package: &SpritePackage,
        limits: &Limits,
    ) -> Result<SpriteImportReport, String> {
        let source_size = source_footprint(self, limits)?;
        let mut budget = Budget::new(limits);
        budget.claim(source_size, "sprite source document copies")?;
        budget.claim(package.footprint()?, "retained sprite package")?;
        budget.claim(
            mul(
                add(
                    package.spec.sprite.0.frames.len(),
                    package.spec.palettes.len(),
                )?,
                256,
            )?,
            "sprite package validation bookkeeping",
        )?;
        package.validate(limits)?;
        check_digest(
            &package.spec.source_sha256,
            &sha256(self.source_bytes()),
            "source",
        )?;
        let spec = &package.spec.sprite.0;
        let source_chunk = self.chunk(ChunkKey {
            kind: *b"SPR2",
            id: spec.id,
        })?;
        check_digest(
            &spec.resource_sha256,
            &sha256(&source_chunk.data),
            "sprite resource",
        )?;
        let (source_set, mut palettes) = decode_source(self, spec.id, &mut budget)?;
        if source_set.version != spec.format_version
            || source_set.default_palette_id != spec.default_palette_id
            || source_set.frames.len() != spec.frames.len()
        {
            return Err("sprite version/default palette/frame count conflict".into());
        }
        if palettes.len() != package.spec.palettes.len() {
            return Err("source palette dependency count conflict".into());
        }
        for (old, ObjectOnly(frame)) in source_set.frames.iter().zip(&spec.frames) {
            if old.raw_palette_id != Some(frame.raw_palette_id)
                || old.palette_id != frame.palette_id
                || old.transparent_index != Some(frame.transparent_index)
            {
                return Err("sprite frame palette identity conflict".into());
            }
        }
        // A hand-written package must not bypass the exporter's representability
        // check. Short source rows can retain hidden zero RGB which the canonical
        // writer would otherwise change at unedited transparent pixels.
        drop(
            encode_sprite(&source_set, &palettes, AlphaMode::Exact, &mut budget).map_err(
                |error| format!("SPR2 source is not representable for typed authoring: {error}"),
            )?,
        );
        budget.claim(
            mul(
                add(package.spec.palettes.len(), 1)?,
                add(size_of::<SpriteResourceChange>(), 136)?,
            )?,
            "sprite change report",
        )?;
        budget.claim(
            mul(spec.frames.len(), size_of::<usize>())?,
            "sprite frame change report",
        )?;
        let mut changes = Vec::with_capacity(package.spec.palettes.len() + 1);
        let mut replacements = BTreeMap::new();
        budget.claim(
            mul(add(package.spec.palettes.len(), 1)?, 256)?,
            "sprite replacement bookkeeping",
        )?;
        for ObjectOnly(palette_spec) in &package.spec.palettes {
            let palette = palettes
                .get_mut(&palette_spec.id)
                .ok_or("source palette dependency identity conflict")?;
            let key = ChunkKey {
                kind: *b"PALT",
                id: palette_spec.id,
            };
            let chunk = self.chunk(key)?;
            check_digest(
                &palette_spec.resource_sha256,
                &sha256(&chunk.data),
                "palette resource",
            )?;
            if palette.version != palette_spec.format_version
                || palette.colors.len() != palette_spec.colors_rgb_hex.len() / 6
                || hex_string(&palette.reserved)? != palette_spec.reserved_hex.to_ascii_lowercase()
            {
                return Err("palette version/reserved bytes/color count conflict".into());
            }
            let mut changed = false;
            for (index, color) in palette.colors.iter_mut().enumerate() {
                let rgb = [
                    byte_at(&palette_spec.colors_rgb_hex, index * 3),
                    byte_at(&palette_spec.colors_rgb_hex, index * 3 + 1),
                    byte_at(&palette_spec.colors_rgb_hex, index * 3 + 2),
                ];
                changed |= color[..3] != rgb;
                color[..3].copy_from_slice(&rgb);
            }
            if changed {
                let encoded = sprites::encode_palt(palette, &budget.codec_limits())
                    .map_err(|error| error.to_string())?;
                budget.claim(encoded.capacity(), "encoded palette allocation")?;
                changes.push(change(chunk, &encoded)?);
                replacements.insert(key, encoded);
            }
        }
        let mut pixels = 0;
        for ObjectOnly(frame) in &spec.frames {
            pixels = add(
                pixels,
                mul(usize::from(frame.width), usize::from(frame.height))?,
            )?;
        }
        budget.claim(
            add(
                mul(spec.frames.len(), size_of::<SpriteFrame>())?,
                mul(pixels, 6)?,
            )?,
            "authored sprite planes",
        )?;
        let mut frames = Vec::with_capacity(spec.frames.len());
        for ObjectOnly(frame) in &spec.frames {
            let palette = &palettes[&frame.palette_id];
            let indices = hex_bytes(&frame.indices_hex);
            let depth = frame.depth_hex.as_ref().map(|value| hex_bytes(value));
            let mut rgba = Vec::with_capacity(indices.len());
            for (i, &index) in indices.iter().enumerate() {
                let alpha = byte_at(&frame.alpha_hex, i);
                let mut color = palette.colors[if alpha == 0 {
                    usize::from(frame.transparent_index)
                } else {
                    usize::from(index)
                }];
                color[3] = alpha;
                rgba.push(color);
            }
            frames.push(SpriteFrame {
                version: spec.format_version,
                source_offset: 0,
                encoded_size: 0,
                width: frame.width,
                height: frame.height,
                flags: frame.flags,
                palette_id: frame.palette_id,
                raw_palette_id: Some(frame.raw_palette_id),
                transparent_index: Some(frame.transparent_index),
                position: frame.position,
                reserved: 0,
                rgba: Some(rgba),
                indices: Some(indices),
                depth,
                fallback_depth: None,
            });
        }
        let set = SpriteSet {
            kind: SpriteKind::Spr2,
            version: spec.format_version,
            byte_order: SpriteByteOrder::LittleEndian,
            default_palette_id: spec.default_palette_id,
            declared_frame_count: frames.len() as u32,
            frames,
        };
        let encoded = encode_sprite(&set, &palettes, package.spec.alpha_mode, &mut budget)?;
        let mut changed_frames = Vec::with_capacity(spec.frames.len());
        for (i, (source, authored)) in source_set.frames.iter().zip(&set.frames).enumerate() {
            if !same_frame(source, authored, package.spec.alpha_mode) {
                changed_frames.push(i);
            }
        }
        if !changed_frames.is_empty() {
            changes.push(change(source_chunk, &encoded.bytes)?);
            replacements.insert(source_chunk.key, encoded.bytes);
        }
        budget.claim(128, "sprite report hashes")?;
        let mut report = SpriteImportReport {
            schema_version: 1,
            source_sha256: sha256(self.source_bytes()),
            output_sha256: String::new(),
            changed_resources: changes,
            changed_frames,
            quantized_alpha_pixels: encoded.quantized_alpha_pixels,
        };
        if !replacements.is_empty() {
            // Keep the generic unknown-byte interchange untouched. Only these
            // already-validated typed replacements can reach this private commit.
            let mut candidate = IffFile {
                header: self.file().header,
                chunks: Vec::with_capacity(self.file().chunks.len()),
            };
            for chunk in &self.file().chunks {
                candidate.chunks.push(IffChunk {
                    key: chunk.key,
                    flags: chunk.flags,
                    label: chunk.label,
                    data: replacements
                        .remove(&chunk.key)
                        .unwrap_or_else(|| chunk.data.clone()),
                });
            }
            self.commit_candidate(candidate, budget.used - source_size, limits)?;
        }
        report.output_sha256 = sha256(self.source_bytes());
        Ok(report)
    }
}

fn change(source: &IffChunk, output: &[u8]) -> Result<SpriteResourceChange, String> {
    Ok(SpriteResourceChange {
        kind_hex: hex_string(&source.key.kind)?,
        id: source.key.id,
        before_sha256: sha256(&source.data),
        after_sha256: sha256(output),
    })
}

fn same_frame(source: &SpriteFrame, authored: &SpriteFrame, mode: AlphaMode) -> bool {
    source.width == authored.width
        && source.height == authored.height
        && source.flags == authored.flags
        && source.position == authored.position
        && source.palette_id == authored.palette_id
        && source.raw_palette_id == authored.raw_palette_id
        && source.transparent_index == authored.transparent_index
        && source.indices == authored.indices
        && source.depth == authored.depth
        && source
            .rgba
            .as_ref()
            .zip(authored.rgba.as_ref())
            .is_some_and(|(old, new)| {
                old.len() == new.len()
                    && old.iter().zip(new).all(|(a, b)| {
                        a[3] == if mode == AlphaMode::Source {
                            quantized(b[3])
                        } else {
                            b[3]
                        }
                    })
            })
}
