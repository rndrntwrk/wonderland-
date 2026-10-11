//! CPU-only IFF visual resource readers, pinned to FreeSO revision
//! `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
//! Source: `tso.files/Formats/IFF/Chunks/{PALT,SPR,SPR2,DGRP,SLOT}.cs`.

use crate::vitaboy::{finite_le, invalid, unsupported, DecodeBudget, F32Bits};
use crate::{reader::Reader, Error, ErrorKind, Limits, Result};
use serde::{Deserialize, Serialize};

mod encode;
pub use encode::{encode_palt, encode_spr2, encode_spr2_with_palettes, EncodedSpr2, Spr2AlphaMode};
mod legacy_encode;
pub use legacy_encode::{encode_dgrp, encode_spr};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    pub version: u32,
    pub reserved: [u8; 8],
    pub colors: Vec<[u8; 4]>,
}

pub fn decode_palt(bytes: &[u8], limits: &Limits) -> Result<Palette> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let version = r.u32_le()?;
    // Source Read uses the same layout for both. Its writer emits 0; version 1
    // is also present in the pinned repository's original IFF resources.
    if version != 0 && version != 1 {
        return Err(unsupported(0, "unsupported PALT version"));
    }
    let n = r.u32_le()? as usize;
    budget.reserve::<[u8; 4]>(n, limits.max_entries, 4, "palette colors")?;
    let reserved = r.read_bytes(8)?.try_into().expect("eight bytes");
    if n.checked_mul(3) != Some(r.remaining()) {
        return Err(invalid(
            r.position(),
            "PALT color count does not match payload size",
        ));
    }
    let mut colors = Vec::with_capacity(n);
    for _ in 0..n {
        colors.push([r.u8()?, r.u8()?, r.u8()?, 255]);
    }
    Ok(Palette {
        version,
        reserved,
        colors,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawingGroup {
    pub version: u16,
    pub images: Vec<DrawingImage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawingImage {
    pub direction: u32,
    pub zoom: u32,
    pub sprites: Vec<DrawingSprite>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawingSprite {
    pub legacy_type: Option<u16>,
    pub sprite_id: u32,
    pub frame_index: u32,
    pub flags: u32,
    /// Integer on-disk coordinates, without the source Vector2 f32 rounding.
    pub sprite_offset: [i32; 2],
    /// X, Y, Z in object coordinates; absent fields use source +0.0 defaults.
    pub object_offset: [F32Bits; 3],
}

impl DrawingSprite {
    pub fn flip(&self) -> bool {
        self.flags & 1 != 0
    }
    pub fn luminous(&self) -> bool {
        self.flags & 4 != 0
    }
    pub fn effective_sprite_offset(&self) -> [F32Bits; 2] {
        self.sprite_offset.map(|n| F32Bits::from_f32(n as f32))
    }
}

impl DrawingGroup {
    /// Source DGRP.GetImage rotates the 8-bit direction by two bits per quarter
    /// turn, then chooses the first matching image in the original ordering.
    pub fn image(&self, direction: u32, zoom: u32, world_rotation: u32) -> Option<&DrawingImage> {
        if direction > 255 || world_rotation > 3 {
            return None;
        }
        let direction = (direction as u8).rotate_left(world_rotation * 2) as u32;
        self.images
            .iter()
            .find(|i| i.direction == direction && i.zoom == zoom)
    }
}

pub fn decode_dgrp(bytes: &[u8], limits: &Limits) -> Result<DrawingGroup> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let version = r.u16_le()?;
    if !(20000..=20004).contains(&version) {
        return Err(unsupported(0, "unsupported DGRP version"));
    }
    let n = if version < 20003 {
        r.u16_le()? as usize
    } else {
        r.u32_le()? as usize
    };
    budget.reserve::<DrawingImage>(n, limits.max_entries, 2, "DGRP images")?;
    let mut images = Vec::with_capacity(n);
    for _ in 0..n {
        let (direction, zoom, n) = if version < 20003 {
            let n = r.u16_le()? as usize;
            (r.u8()? as u32, r.u8()? as u32, n)
        } else {
            (r.u32_le()?, r.u32_le()?, r.u32_le()? as usize)
        };
        budget.reserve::<DrawingSprite>(n, limits.max_entries, r.position(), "DGRP sprites")?;
        let mut sprites = Vec::with_capacity(n);
        for _ in 0..n {
            let sprite = if version < 20003 {
                let legacy_type = Some(r.u16_le()?);
                let sprite_id = r.u16_le()? as u32;
                let frame_index = r.u16_le()? as u32;
                let flags = r.u16_le()? as u32;
                let sprite_offset = [r.i16_le()? as i32, r.i16_le()? as i32];
                let z = if version == 20001 {
                    finite_le(&mut r)?
                } else {
                    F32Bits(0)
                };
                DrawingSprite {
                    legacy_type,
                    sprite_id,
                    frame_index,
                    flags,
                    sprite_offset,
                    object_offset: [F32Bits(0), F32Bits(0), z],
                }
            } else {
                let sprite_id = r.u32_le()?;
                let frame_index = r.u32_le()?;
                let sprite_offset = [r.i32_le()?, r.i32_le()?];
                let z = finite_le(&mut r)?;
                let flags = r.u32_le()?;
                let (x, y) = if version == 20004 {
                    (finite_le(&mut r)?, finite_le(&mut r)?)
                } else {
                    (F32Bits(0), F32Bits(0))
                };
                DrawingSprite {
                    legacy_type: None,
                    sprite_id,
                    frame_index,
                    flags,
                    sprite_offset,
                    object_offset: [x, y, z],
                }
            };
            sprites.push(sprite);
        }
        images.push(DrawingImage {
            direction,
            zoom,
            sprites,
        });
    }
    if r.remaining() != 0 {
        return Err(unsupported(r.position(), "unrecognized trailing DGRP data"));
    }
    Ok(DrawingGroup { version, images })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpriteKind {
    Spr,
    Spr2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpriteByteOrder {
    LittleEndian,
    BigEndian,
}

impl SpriteByteOrder {
    fn u16(self, r: &mut Reader<'_>) -> Result<u16> {
        match self {
            Self::LittleEndian => r.u16_le(),
            Self::BigEndian => r.u16_be(),
        }
    }
    fn u32(self, r: &mut Reader<'_>) -> Result<u32> {
        match self {
            Self::LittleEndian => r.u32_le(),
            Self::BigEndian => r.u32_be(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpriteSet {
    pub kind: SpriteKind,
    pub version: u32,
    pub byte_order: SpriteByteOrder,
    pub default_palette_id: u32,
    /// SPR 1001's source reader reads frames to EOF, independent of this count.
    pub declared_frame_count: u32,
    pub frames: Vec<SpriteFrame>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpriteFrame {
    pub version: u32,
    pub source_offset: usize,
    pub encoded_size: usize,
    pub width: u16,
    pub height: u16,
    pub flags: u32,
    pub palette_id: u16,
    pub raw_palette_id: Option<u16>,
    pub transparent_index: Option<u16>,
    pub position: [i16; 2],
    pub reserved: u32,
    /// Row-major straight RGBA. Alpha multiplication/premultiplication is left
    /// to the renderer; these bytes match the source decoded Color[] values.
    pub rgba: Option<Vec<[u8; 4]>>,
    pub indices: Option<Vec<u8>>,
    pub depth: Option<Vec<u8>>,
    /// SPR# has no encoded depth; the source renderer supplies a constant 128.
    pub fallback_depth: Option<u8>,
}

fn color(palette: &Palette, index: usize, offset: usize) -> Result<[u8; 4]> {
    palette
        .colors
        .get(index)
        .copied()
        .ok_or_else(|| invalid(offset, "sprite palette index outside palette"))
}

fn shifted<T>(result: Result<T>, base: usize) -> Result<T> {
    result.map_err(|mut error| {
        error.offset = error.offset.saturating_add(base);
        error
    })
}

fn offsets(
    r: &mut Reader<'_>,
    count: usize,
    bytes_len: usize,
    order: SpriteByteOrder,
    budget: &mut DecodeBudget<'_>,
) -> Result<Vec<usize>> {
    budget.reserve::<usize>(
        count,
        budget.limits.max_frames,
        r.position(),
        "sprite frame offsets",
    )?;
    let table_end = r
        .position()
        .checked_add(
            count
                .checked_mul(4)
                .ok_or_else(|| Error::new(ErrorKind::Overflow, r.position(), "sprite offsets"))?,
        )
        .ok_or_else(|| Error::new(ErrorKind::Overflow, r.position(), "sprite offsets"))?;
    if table_end > bytes_len {
        return Err(Error::new(
            ErrorKind::Truncated,
            r.position(),
            "sprite offset table",
        ));
    }
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        let offset = order.u32(r)? as usize;
        if offset < table_end || offset >= bytes_len {
            return Err(invalid(
                r.position() - 4,
                "sprite frame offset outside payload",
            ));
        }
        if result.last().is_some_and(|&previous| offset <= previous) {
            return Err(Error::new(
                ErrorKind::Overlap,
                r.position() - 4,
                "overlapping or decreasing sprite frame offsets",
            ));
        }
        result.push(offset);
    }
    Ok(result)
}

fn pixel_count(width: u16, height: u16, budget: &DecodeBudget<'_>) -> Result<usize> {
    let n = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "sprite pixel count"))?;
    budget
        .limits
        .check_count(n, budget.limits.max_pixels, 0, "sprite pixels")?;
    Ok(n)
}

/// Decode SPR# using an explicitly supplied palette. Missing palettes are a
/// dependency error for the caller; this reader does not invent black colors.
pub fn decode_spr(bytes: &[u8], palette: &Palette, limits: &Limits) -> Result<SpriteSet> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let header = r.read_bytes(4)?;
    let order = if header[..2] == [0, 0] {
        SpriteByteOrder::BigEndian
    } else {
        SpriteByteOrder::LittleEndian
    };
    let version = match order {
        SpriteByteOrder::BigEndian => u32::from_be_bytes(header.try_into().unwrap()),
        SpriteByteOrder::LittleEndian => u32::from_le_bytes(header.try_into().unwrap()),
    };
    if version != 1000 && version != 1001 {
        return Err(unsupported(0, "unsupported SPR version"));
    }
    // The source's big-endian version expression loses the low byte; only its
    // offset-table (1000) branch is independently supported for big-endian SPR.
    if order == SpriteByteOrder::BigEndian && version == 1001 {
        return Err(unsupported(
            0,
            "big-endian SPR 1001 is not supported by pinned source",
        ));
    }
    let declared_frame_count = order.u32(&mut r)?;
    limits.check_count(
        declared_frame_count as usize,
        limits.max_frames,
        4,
        "SPR frames",
    )?;
    let default_palette_id = order.u32(&mut r)?;
    let mut frames = Vec::new();
    if version == 1000 {
        let count = declared_frame_count as usize;
        budget.reserve::<SpriteFrame>(count, limits.max_frames, 4, "SPR frames")?;
        let offsets = offsets(&mut r, count, bytes.len(), order, &mut budget)?;
        frames = Vec::with_capacity(count);
        for (i, &start) in offsets.iter().enumerate() {
            let end = offsets.get(i + 1).copied().unwrap_or(bytes.len());
            let mut frame = shifted(
                spr_frame(
                    &bytes[start..end],
                    version,
                    order,
                    default_palette_id as u16,
                    palette,
                    &mut budget,
                ),
                start,
            )?;
            frame.source_offset = start;
            frames.push(frame);
        }
        if count == 0 && r.remaining() != 0 {
            return Err(unsupported(r.position(), "data after empty SPR table"));
        }
    } else {
        // Preserve the source's EOF behavior even if declared_frame_count lies.
        while r.remaining() != 0 {
            let start = r.position();
            let frame_version = order.u32(&mut r)?;
            if frame_version != 1000 && frame_version != 1001 {
                return Err(unsupported(start, "unsupported SPR frame version"));
            }
            let size = order.u32(&mut r)? as usize;
            limits.check_count(frames.len() + 1, limits.max_frames, start, "SPR frames")?;
            budget.reserve::<SpriteFrame>(1, limits.max_frames, start, "SPR frames")?;
            let data_start = r.position();
            let data = r.read_bytes(size)?;
            let mut frame = shifted(
                spr_frame(
                    data,
                    frame_version,
                    order,
                    default_palette_id as u16,
                    palette,
                    &mut budget,
                ),
                data_start,
            )?;
            frame.source_offset = start;
            frame.encoded_size = size + 8;
            frames.push(frame);
        }
    }
    Ok(SpriteSet {
        kind: SpriteKind::Spr,
        version,
        byte_order: order,
        default_palette_id,
        declared_frame_count,
        frames,
    })
}

fn spr_frame(
    bytes: &[u8],
    version: u32,
    order: SpriteByteOrder,
    palette_id: u16,
    palette: &Palette,
    budget: &mut DecodeBudget<'_>,
) -> Result<SpriteFrame> {
    let mut r = Reader::new(bytes);
    let reserved = order.u32(&mut r)?;
    let height = order.u16(&mut r)?;
    let width = order.u16(&mut r)?;
    let n = pixel_count(width, height, budget)?;
    budget.reserve::<[u8; 4]>(n, budget.limits.max_pixels, 0, "SPR colors")?;
    budget.reserve::<u8>(n, budget.limits.max_pixels, 0, "SPR palette indices")?;
    let mut rgba = vec![[0; 4]; n];
    let mut indices = vec![0; n];
    let mut y = 0usize;
    loop {
        let at = r.position();
        let command = r.u8()?;
        let count = r.u8()? as usize;
        match command {
            0 | 0x10 => (),
            4 => {
                if y >= height as usize || count < 2 {
                    return Err(invalid(at, "SPR row outside frame or invalid byte count"));
                }
                let row_start = r.position();
                let mut row = Reader::new(r.read_bytes(count - 2)?);
                let mut x = 0usize;
                while row.remaining() != 0 {
                    let at = row_start + row.position();
                    let command = shifted(row.u8(), row_start)?;
                    let count = shifted(row.u8(), row_start)? as usize;
                    let end = x
                        .checked_add(count)
                        .ok_or_else(|| Error::new(ErrorKind::Overflow, at, "SPR run length"))?;
                    if end > width as usize {
                        return Err(invalid(at, "SPR pixel run crosses row boundary"));
                    }
                    match command {
                        1 => (),
                        2 => {
                            let index = shifted(row.u8(), row_start)?;
                            shifted(row.u8(), row_start)?; // padding, arbitrary source value
                            let color = color(palette, index as usize, at)?;
                            for px in x..end {
                                rgba[y * width as usize + px] = color;
                                indices[y * width as usize + px] = index;
                            }
                        }
                        3 => {
                            for px in x..end {
                                let index = shifted(row.u8(), row_start)?;
                                rgba[y * width as usize + px] = color(palette, index as usize, at)?;
                                indices[y * width as usize + px] = index;
                            }
                            if !count.is_multiple_of(2) {
                                shifted(row.u8(), row_start)?;
                            }
                        }
                        _ => return Err(unsupported(at, "unknown SPR pixel command")),
                    }
                    x = end;
                }
                y += 1;
            }
            5 => break,
            9 => {
                y = y
                    .checked_add(count)
                    .ok_or_else(|| Error::new(ErrorKind::Overflow, at, "SPR skipped rows"))?;
                if y > height as usize {
                    return Err(invalid(at, "SPR skipped rows cross frame boundary"));
                }
            }
            _ => return Err(unsupported(at, "unknown SPR row command")),
        }
    }
    // Frames sometimes have padding to the next offset. Only zero padding is
    // admitted so an unknown command stream cannot be silently discarded.
    if r.read_bytes(r.remaining())?.iter().any(|&b| b != 0) {
        return Err(unsupported(
            r.position(),
            "nonzero data after SPR end marker",
        ));
    }
    Ok(SpriteFrame {
        version,
        source_offset: 0,
        encoded_size: bytes.len(),
        width,
        height,
        flags: 1,
        palette_id,
        raw_palette_id: None,
        transparent_index: None,
        position: [0, 0],
        reserved,
        rgba: Some(rgba),
        indices: Some(indices),
        depth: None,
        fallback_depth: Some(128),
    })
}

/// Convenience decoder for resources whose frames use the supplied palette.
/// Use decode_spr2_with_palettes when frame palette IDs refer to different PALT
/// resources; the resolver receives each effective ID, including source fallbacks.
pub fn decode_spr2(bytes: &[u8], palette: &Palette, limits: &Limits) -> Result<SpriteSet> {
    decode_spr2_with_palettes(bytes, |_| Some(palette), limits)
}

pub fn decode_spr2_with_palettes<'p>(
    bytes: &[u8],
    mut palette: impl FnMut(u16) -> Option<&'p Palette>,
    limits: &Limits,
) -> Result<SpriteSet> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let version = r.u32_le()?;
    if version != 1000 && version != 1001 {
        return Err(unsupported(0, "unsupported SPR2 version"));
    }
    let (declared_frame_count, default_palette_id) = if version == 1000 {
        (r.u32_le()?, r.u32_le()?)
    } else {
        let palette = r.u32_le()?;
        (r.u32_le()?, palette)
    };
    let count = declared_frame_count as usize;
    budget.reserve::<SpriteFrame>(count, limits.max_frames, 4, "SPR2 frames")?;
    let mut frames = Vec::with_capacity(count);
    if version == 1000 {
        let offsets = offsets(
            &mut r,
            count,
            bytes.len(),
            SpriteByteOrder::LittleEndian,
            &mut budget,
        )?;
        for (i, &start) in offsets.iter().enumerate() {
            let end = offsets.get(i + 1).copied().unwrap_or(bytes.len());
            let mut frame = shifted(
                spr2_frame(
                    &bytes[start..end],
                    version,
                    default_palette_id as u16,
                    &mut palette,
                    &mut budget,
                ),
                start,
            )?;
            frame.source_offset = start;
            frames.push(frame);
        }
        if count == 0 && r.remaining() != 0 {
            return Err(unsupported(r.position(), "data after empty SPR2 table"));
        }
    } else {
        for _ in 0..count {
            let start = r.position();
            let frame_version = r.u32_le()?;
            if frame_version != 1001 {
                return Err(unsupported(start, "unsupported SPR2 frame version"));
            }
            let size = r.u32_le()? as usize;
            let data_start = r.position();
            let data = r.read_bytes(size)?;
            let mut frame = shifted(
                spr2_frame(
                    data,
                    version,
                    default_palette_id as u16,
                    &mut palette,
                    &mut budget,
                ),
                data_start,
            )?;
            frame.source_offset = start;
            frame.encoded_size = size + 8;
            frames.push(frame);
        }
        if r.remaining() != 0 {
            return Err(unsupported(r.position(), "trailing SPR2 data"));
        }
    }
    Ok(SpriteSet {
        kind: SpriteKind::Spr2,
        version,
        byte_order: SpriteByteOrder::LittleEndian,
        default_palette_id,
        declared_frame_count,
        frames,
    })
}

fn spr2_frame<'p>(
    bytes: &[u8],
    version: u32,
    default_palette_id: u16,
    palettes: &mut impl FnMut(u16) -> Option<&'p Palette>,
    budget: &mut DecodeBudget<'_>,
) -> Result<SpriteFrame> {
    let mut r = Reader::new(bytes);
    let width = r.u16_le()?;
    let height = r.u16_le()?;
    let flags = r.u32_le()?;
    if flags & !7 != 0 {
        return Err(unsupported(4, "unknown SPR2 channel flags"));
    }
    let raw_palette_id = r.u16_le()?;
    let palette_id = if version == 1000 || raw_palette_id == 0 || raw_palette_id == 0xa3a3 {
        default_palette_id
    } else {
        raw_palette_id
    };
    let transparent_index = r.u16_le()?;
    let y_position = r.i16_le()?;
    let x_position = r.i16_le()?;
    let has_pixels = flags & 1 != 0;
    let has_depth = flags & 2 != 0;
    if flags & 4 != 0 && !has_pixels {
        return Err(invalid(4, "SPR2 alpha channel requires color channel"));
    }
    let palette =
        palettes(palette_id).ok_or_else(|| invalid(8, "SPR2 palette dependency unavailable"))?;
    let mut transparent = color(palette, transparent_index as usize, 10)?;
    transparent[3] = 0;
    let n = pixel_count(width, height, budget)?;
    let mut rgba = if has_pixels {
        budget.reserve::<[u8; 4]>(n, budget.limits.max_pixels, 0, "SPR2 colors")?;
        Some(vec![[0; 4]; n])
    } else {
        None
    };
    let mut indices = if has_pixels {
        budget.reserve::<u8>(n, budget.limits.max_pixels, 0, "SPR2 indices")?;
        Some(vec![0; n])
    } else {
        None
    };
    let mut depth = if has_depth {
        budget.reserve::<u8>(n, budget.limits.max_pixels, 0, "SPR2 depth")?;
        Some(vec![0; n])
    } else {
        None
    };
    let mut y = 0usize;
    loop {
        let at = r.position();
        let marker = r.u16_le()?;
        let command = marker >> 13;
        let count = (marker & 0x1fff) as usize;
        match command {
            0 => {
                if y >= height as usize || count < 2 {
                    return Err(invalid(at, "SPR2 row outside frame or invalid byte count"));
                }
                let row_start = r.position();
                let mut row = Reader::new(r.read_bytes(count - 2)?);
                let mut x = 0usize;
                while row.remaining() != 0 {
                    let at = row_start + row.position();
                    let marker = shifted(row.u16_le(), row_start)?;
                    let command = marker >> 13;
                    let count = (marker & 0x1fff) as usize;
                    let end = x
                        .checked_add(count)
                        .ok_or_else(|| Error::new(ErrorKind::Overflow, at, "SPR2 run length"))?;
                    if end > width as usize {
                        return Err(invalid(at, "SPR2 pixel run crosses row boundary"));
                    }
                    if !has_pixels {
                        return Err(invalid(at, "SPR2 pixel command without color channel"));
                    }
                    if (command == 1 || command == 2) && !has_depth {
                        return Err(invalid(at, "SPR2 depth command without depth channel"));
                    }
                    match command {
                        1 | 2 => {
                            for x in x..end {
                                let z = shifted(row.u8(), row_start)?;
                                let index = shifted(row.u8(), row_start)?;
                                let mut color = color(palette, index as usize, at)?;
                                if command == 2 {
                                    let alpha = shifted(row.u8(), row_start)?;
                                    if alpha > 31 {
                                        return Err(invalid(
                                            at,
                                            "SPR2 alpha exceeds five-bit range",
                                        ));
                                    }
                                    // Exact byte results for the source's alpha * (255/31)
                                    // truncation, without introducing float conversion.
                                    color[3] = (u16::from(alpha) * 255 / 31) as u8;
                                }
                                let at = y * width as usize + x;
                                rgba.as_mut().unwrap()[at] = color;
                                indices.as_mut().unwrap()[at] = index;
                                depth.as_mut().unwrap()[at] = z;
                            }
                            if command == 2 && !count.is_multiple_of(2) {
                                shifted(row.u8(), row_start)?;
                            }
                        }
                        3 => {
                            for x in x..end {
                                let at = y * width as usize + x;
                                rgba.as_mut().unwrap()[at] = transparent;
                                indices.as_mut().unwrap()[at] = transparent_index as u8;
                                if let Some(depth) = &mut depth {
                                    depth[at] = 255;
                                }
                            }
                        }
                        6 => {
                            for x in x..end {
                                let index = shifted(row.u8(), row_start)?;
                                let at = y * width as usize + x;
                                rgba.as_mut().unwrap()[at] =
                                    color(palette, index as usize, row_start + row.position() - 1)?;
                                indices.as_mut().unwrap()[at] = index;
                                if let Some(depth) = &mut depth {
                                    depth[at] = 0;
                                }
                            }
                            if !count.is_multiple_of(2) {
                                shifted(row.u8(), row_start)?;
                            }
                        }
                        _ => return Err(unsupported(at, "unknown SPR2 pixel command")),
                    }
                    x = end;
                }
                // Source leaves remaining RGBA/indices at zero but fills depth.
                if let Some(depth) = &mut depth {
                    for x in x..width as usize {
                        depth[y * width as usize + x] = 255;
                    }
                }
                y += 1;
            }
            4 => {
                let end = y
                    .checked_add(count)
                    .ok_or_else(|| Error::new(ErrorKind::Overflow, at, "SPR2 skipped rows"))?;
                if end > height as usize {
                    return Err(invalid(at, "SPR2 skipped rows cross frame boundary"));
                }
                for at in y * width as usize..end * width as usize {
                    if let Some(rgba) = &mut rgba {
                        rgba[at] = transparent;
                    }
                    if let Some(indices) = &mut indices {
                        indices[at] = transparent_index as u8;
                    }
                    if let Some(depth) = &mut depth {
                        depth[at] = 255;
                    }
                }
                y = end;
            }
            5 => break,
            _ => return Err(unsupported(at, "unknown SPR2 row command")),
        }
    }
    if r.read_bytes(r.remaining())?.iter().any(|&b| b != 0) {
        return Err(unsupported(
            r.position(),
            "nonzero data after SPR2 end marker",
        ));
    }
    Ok(SpriteFrame {
        version,
        source_offset: 0,
        encoded_size: bytes.len(),
        width,
        height,
        flags,
        palette_id,
        raw_palette_id: Some(raw_palette_id),
        transparent_index: Some(transparent_index),
        position: [x_position, y_position],
        reserved: 0,
        rgba,
        indices,
        depth,
        fallback_depth: None,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotResource {
    pub reserved: u32,
    pub version: u32,
    pub magic: [u8; 4],
    /// The source Chronological list; grouping by type must not reorder it.
    pub slots: Vec<SlotItem>,
    pub trailing: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotItem {
    pub type_id: u16,
    pub offset: [F32Bits; 3],
    pub standing: i32,
    pub sitting: i32,
    pub ground: i32,
    pub rsflags: i32,
    pub snap_target_slot: i32,
    /// Original on-disk integers. Use effective_proximity for VM units.
    pub min_proximity: Option<i32>,
    pub max_proximity: Option<i32>,
    pub optimal_proximity: Option<i32>,
    pub max_size: Option<i32>,
    pub i10: Option<i32>,
    pub gradient: Option<F32Bits>,
    pub height: Option<i32>,
    pub facing: Option<i32>,
    pub resolution: Option<i32>,
}

impl SlotItem {
    pub fn effective_proximity(&self, version: u32) -> Result<[i32; 3]> {
        let values = [
            self.min_proximity.unwrap_or(0),
            self.max_proximity.unwrap_or(0),
            self.optimal_proximity.unwrap_or(0),
        ];
        if version > 9 {
            return Ok(values);
        }
        let mut result = [0; 3];
        for (target, value) in result.iter_mut().zip(values) {
            *target = value
                .checked_mul(16)
                .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "SLOT proximity scaling"))?;
        }
        Ok(result)
    }
    pub fn effective_height(&self) -> i32 {
        match self.height {
            Some(h) if h != 0 => h,
            _ => 5,
        }
    }
    pub fn effective_facing(&self) -> i32 {
        self.facing.unwrap_or(-2)
    }
    pub fn effective_resolution(&self) -> i32 {
        self.resolution.unwrap_or(16)
    }
    pub fn effective_max_size(&self) -> i32 {
        self.max_size.unwrap_or(100)
    }
}

fn slot_span(version: u32) -> Result<usize> {
    match version {
        4 | 5 => Ok(34),
        6 => Ok(54),
        7 => Ok(58),
        8 => Ok(62),
        9 => Ok(66),
        10 => Ok(70),
        _ => Err(unsupported(4, "unsupported SLOT version")),
    }
}

pub fn decode_slot(bytes: &[u8], limits: &Limits) -> Result<SlotResource> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let reserved = r.u32_le()?;
    let version = r.u32_le()?;
    let span = slot_span(version)?;
    let magic: [u8; 4] = r.read_bytes(4)?.try_into().expect("four bytes");
    if &magic != b"TOLS" {
        return Err(Error::new(ErrorKind::InvalidMagic, 8, "SLOT magic"));
    }
    let n = r.u32_le()? as usize;
    budget.reserve::<SlotItem>(n, limits.max_entries, 12, "SLOT items")?;
    let stored_size = n
        .checked_mul(span)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, 12, "SLOT item bytes"))?;
    if stored_size > r.remaining() {
        return Err(Error::new(
            ErrorKind::Truncated,
            r.position(),
            "SLOT item bytes",
        ));
    }
    let mut slots = Vec::with_capacity(n);
    for _ in 0..n {
        let type_id = r.u16_le()?;
        let offset = [finite_le(&mut r)?, finite_le(&mut r)?, finite_le(&mut r)?];
        let standing = r.i32_le()?;
        let sitting = r.i32_le()?;
        let ground = r.i32_le()?;
        let rsflags = r.i32_le()?;
        let snap_target_slot = r.i32_le()?;
        let (min_proximity, max_proximity, optimal_proximity, max_size, i10) = if version >= 6 {
            (
                Some(r.i32_le()?),
                Some(r.i32_le()?),
                Some(r.i32_le()?),
                Some(r.i32_le()?),
                Some(r.i32_le()?),
            )
        } else {
            (None, None, None, None, None)
        };
        let gradient = if version >= 7 {
            Some(finite_le(&mut r)?)
        } else {
            None
        };
        let height = if version >= 8 {
            Some(r.i32_le()?)
        } else {
            None
        };
        let facing = if version >= 9 {
            Some(r.i32_le()?)
        } else {
            None
        };
        let resolution = if version >= 10 {
            Some(r.i32_le()?)
        } else {
            None
        };
        let item = SlotItem {
            type_id,
            offset,
            standing,
            sitting,
            ground,
            rsflags,
            snap_target_slot,
            min_proximity,
            max_proximity,
            optimal_proximity,
            max_size,
            i10,
            gradient,
            height,
            facing,
            resolution,
        };
        item.effective_proximity(version)?;
        slots.push(item);
    }
    budget.reserve::<u8>(
        r.remaining(),
        limits.max_resource_bytes,
        r.position(),
        "SLOT trailing bytes",
    )?;
    let trailing = r.read_bytes(r.remaining())?.to_vec();
    Ok(SlotResource {
        reserved,
        version,
        magic,
        slots,
        trailing,
    })
}

/// Exact-version SLOT writer. Fields absent from that version must remain None;
/// unsupported edits are rejected instead of silently upgrading the resource.
pub fn encode_slot(resource: &SlotResource, limits: &Limits) -> Result<Vec<u8>> {
    let span = slot_span(resource.version)?;
    if &resource.magic != b"TOLS" {
        return Err(Error::new(ErrorKind::InvalidMagic, 8, "SLOT magic"));
    }
    limits.check_count(resource.slots.len(), limits.max_entries, 12, "SLOT items")?;
    let n = u32::try_from(resource.slots.len())
        .map_err(|_| Error::new(ErrorKind::Overflow, 12, "SLOT count"))?;
    let size = resource
        .slots
        .len()
        .checked_mul(span)
        .and_then(|n| n.checked_add(16))
        .and_then(|n| n.checked_add(resource.trailing.len()))
        .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "encoded SLOT size"))?;
    limits.check_count(
        size,
        limits
            .max_resource_bytes
            .min(limits.max_input_bytes)
            .min(limits.max_total_decoded_bytes),
        0,
        "encoded SLOT bytes",
    )?;
    let mut out = Vec::with_capacity(size);
    out.extend(resource.reserved.to_le_bytes());
    out.extend(resource.version.to_le_bytes());
    out.extend(resource.magic);
    out.extend(n.to_le_bytes());
    for slot in &resource.slots {
        slot.effective_proximity(resource.version)?;
        out.extend(slot.type_id.to_le_bytes());
        for f in slot.offset {
            if !f.is_finite() {
                return Err(invalid(out.len(), "non-finite SLOT offset"));
            }
            out.extend(f.0.to_le_bytes());
        }
        for n in [
            slot.standing,
            slot.sitting,
            slot.ground,
            slot.rsflags,
            slot.snap_target_slot,
        ] {
            out.extend(n.to_le_bytes());
        }
        for field in [
            slot.min_proximity,
            slot.max_proximity,
            slot.optimal_proximity,
            slot.max_size,
            slot.i10,
        ] {
            write_slot_field(&mut out, field, resource.version >= 6)?;
        }
        if let Some(f) = slot.gradient {
            if !f.is_finite() {
                return Err(invalid(out.len(), "non-finite SLOT gradient"));
            }
        }
        write_slot_field(
            &mut out,
            slot.gradient.map(|f| f.0 as i32),
            resource.version >= 7,
        )?;
        write_slot_field(&mut out, slot.height, resource.version >= 8)?;
        write_slot_field(&mut out, slot.facing, resource.version >= 9)?;
        write_slot_field(&mut out, slot.resolution, resource.version >= 10)?;
    }
    out.extend(&resource.trailing);
    Ok(out)
}

fn write_slot_field(out: &mut Vec<u8>, value: Option<i32>, present: bool) -> Result<()> {
    match (value, present) {
        (Some(value), true) => out.extend(value.to_le_bytes()),
        (None, false) => (),
        _ => {
            return Err(invalid(
                out.len(),
                "SLOT field presence differs from encoded version",
            ))
        }
    }
    Ok(())
}
