// SPDX-License-Identifier: MPL-2.0
//! Canonical CPU-only authoring based on PALT.Write, SPR2Frame.Write and
//! SPR2FrameEncoder.WriteFrame at the pinned FreeSO source revision.
//! Unchanged encoded resources should use their original bytes for passthrough.

use super::{Palette, SpriteByteOrder, SpriteFrame, SpriteKind, SpriteSet};
use crate::{Error, ErrorKind, Limits, Result};

const FIELD_MAX: usize = 0x1fff;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Spr2AlphaMode {
    /// Reject alpha values that the source five-bit encoding cannot retain.
    #[default]
    Exact,
    /// Apply the original writer's ceiling quantization; report changed pixels.
    QuantizeLikeSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedSpr2 {
    pub bytes: Vec<u8>,
    pub quantized_alpha_pixels: usize,
}

fn invalid(context: &str) -> Error {
    Error::new(ErrorKind::InvalidData, 0, context)
}

fn add(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "sprite encoder size overflow"))
}

fn mul(a: usize, b: usize) -> Result<usize> {
    a.checked_mul(b)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "sprite encoder size overflow"))
}

fn check_output(size: usize, retained: usize, limits: &Limits) -> Result<()> {
    limits.check_count(size, limits.max_input_bytes, 0, "encoded sprite bytes")?;
    limits.check_count(
        size,
        limits.max_resource_bytes,
        0,
        "encoded sprite resource bytes",
    )?;
    limits.check_count(
        add(size, retained)?,
        limits.max_total_decoded_bytes,
        0,
        "sprite authoring retained and encoded bytes",
    )?;
    if size > u32::MAX as usize {
        return Err(Error::new(
            ErrorKind::Overflow,
            0,
            "SPR2 offset exceeds u32",
        ));
    }
    Ok(())
}

fn check_palette(palette: &Palette, limits: &Limits) -> Result<()> {
    if !matches!(palette.version, 0 | 1) {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            0,
            "unsupported PALT version",
        ));
    }
    limits.check_count(
        palette.colors.len(),
        limits.max_entries,
        0,
        "palette color count",
    )?;
    if palette.colors.iter().any(|color| color[3] != 255) {
        return Err(invalid(
            "PALT has no alpha channel; palette colors must be opaque",
        ));
    }
    Ok(())
}

/// Encode PALT 0/1, preserving the declared version, reserved bytes and RGB.
/// Unlike the original writer, this rejects alpha loss rather than discarding it.
pub fn encode_palt(palette: &Palette, limits: &Limits) -> Result<Vec<u8>> {
    check_palette(palette, limits)?;
    let count = u32::try_from(palette.colors.len())
        .map_err(|_| Error::new(ErrorKind::Overflow, 4, "PALT color count exceeds u32"))?;
    let size = add(16, mul(palette.colors.len(), 3)?)?;
    check_output(size, mul(palette.colors.len(), 4)?, limits)?;
    let mut bytes = Vec::with_capacity(size);
    bytes.extend_from_slice(&palette.version.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend_from_slice(&palette.reserved);
    for color in &palette.colors {
        bytes.extend_from_slice(&color[..3]);
    }
    Ok(bytes)
}

/// Encode an SPR2 whose frames share the supplied palette, without alpha loss.
pub fn encode_spr2(set: &SpriteSet, palette: &Palette, limits: &Limits) -> Result<Vec<u8>> {
    Ok(encode_spr2_with_palettes(set, |_| Some(palette), Spr2AlphaMode::Exact, limits)?.bytes)
}

struct FramePlan {
    raw_palette_id: u16,
    body_size: usize,
}

/// Encode SPR2 1000 (offset table) or 1001 (sized frame records).
/// The palette resolver is called once per frame with its effective palette ID.
/// Source offsets and encoded sizes are recomputed; other representable frame
/// metadata is retained. The source's nonrepresentable transparency and channel
/// combinations are rejected. Long 13-bit runs/skips are split; rows that exceed
/// the format's 13-bit byte count are rejected instead of wrapping.
pub fn encode_spr2_with_palettes<'p>(
    set: &SpriteSet,
    mut palettes: impl FnMut(u16) -> Option<&'p Palette>,
    alpha_mode: Spr2AlphaMode,
    limits: &Limits,
) -> Result<EncodedSpr2> {
    if set.kind != SpriteKind::Spr2 || set.byte_order != SpriteByteOrder::LittleEndian {
        return Err(invalid(
            "SPR2 authoring requires a little-endian SPR2 resource",
        ));
    }
    if !matches!(set.version, 1000 | 1001) {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            0,
            "unsupported SPR2 version",
        ));
    }
    if set.declared_frame_count as usize != set.frames.len() {
        return Err(invalid("SPR2 frame count differs from the declared count"));
    }
    limits.check_count(set.frames.len(), limits.max_frames, 0, "SPR2 frame count")?;
    let mut retained = mul(set.frames.len(), std::mem::size_of::<SpriteFrame>())?;
    retained = add(
        retained,
        mul(set.frames.len(), std::mem::size_of::<FramePlan>())?,
    )?;
    let mut size = add(
        12,
        if set.version == 1000 {
            mul(set.frames.len(), 4)?
        } else {
            0
        },
    )?;
    check_output(size, retained, limits)?;
    let mut plans = Vec::with_capacity(set.frames.len());
    let mut quantized_alpha_pixels = 0usize;
    for frame in &set.frames {
        let palette = palettes(frame.palette_id)
            .ok_or_else(|| invalid("SPR2 palette dependency unavailable"))?;
        check_palette(palette, limits)?;
        let (raw_palette_id, footprint, changed) =
            validate_frame(set, frame, palette, alpha_mode, limits)?;
        retained = add(retained, footprint)?;
        let body_size = add(16, command_bytes(frame, None)?)?;
        size = add(
            size,
            add(body_size, if set.version == 1001 { 8 } else { 0 })?,
        )?;
        check_output(size, retained, limits)?;
        quantized_alpha_pixels = add(quantized_alpha_pixels, changed)?;
        plans.push(FramePlan {
            raw_palette_id,
            body_size,
        });
    }

    // No output allocation or serialization until every frame and total budget
    // has passed validation. Encoding only consumes the prevalidated arrays.
    let mut bytes = Vec::with_capacity(size);
    bytes.extend_from_slice(&set.version.to_le_bytes());
    if set.version == 1000 {
        bytes.extend_from_slice(&set.declared_frame_count.to_le_bytes());
        bytes.extend_from_slice(&set.default_palette_id.to_le_bytes());
        let mut offset = 12 + 4 * plans.len();
        for plan in &plans {
            bytes.extend_from_slice(&(offset as u32).to_le_bytes());
            offset += plan.body_size;
        }
    } else {
        bytes.extend_from_slice(&set.default_palette_id.to_le_bytes());
        bytes.extend_from_slice(&set.declared_frame_count.to_le_bytes());
    }
    for (frame, plan) in set.frames.iter().zip(&plans) {
        if set.version == 1001 {
            bytes.extend_from_slice(&1001u32.to_le_bytes());
            bytes.extend_from_slice(&(plan.body_size as u32).to_le_bytes());
        }
        bytes.extend_from_slice(&frame.width.to_le_bytes());
        bytes.extend_from_slice(&frame.height.to_le_bytes());
        bytes.extend_from_slice(&frame.flags.to_le_bytes());
        bytes.extend_from_slice(&plan.raw_palette_id.to_le_bytes());
        bytes.extend_from_slice(&frame.transparent_index.unwrap().to_le_bytes());
        bytes.extend_from_slice(&frame.position[1].to_le_bytes());
        bytes.extend_from_slice(&frame.position[0].to_le_bytes());
        command_bytes(frame, Some(&mut bytes))?;
    }
    debug_assert_eq!(bytes.len(), size);
    Ok(EncodedSpr2 {
        bytes,
        quantized_alpha_pixels,
    })
}

fn validate_frame(
    set: &SpriteSet,
    frame: &SpriteFrame,
    palette: &Palette,
    alpha_mode: Spr2AlphaMode,
    limits: &Limits,
) -> Result<(u16, usize, usize)> {
    if frame.version != set.version || frame.reserved != 0 || frame.fallback_depth.is_some() {
        return Err(invalid(
            "frame metadata is not representable in the selected SPR2 layout",
        ));
    }
    if frame.flags & !7 != 0 || frame.flags & 1 == 0 {
        return Err(invalid(
            "SPR2 authoring requires color and known channel flags",
        ));
    }
    let raw = frame.raw_palette_id.unwrap_or(frame.palette_id);
    let effective = if set.version == 1000 || raw == 0 || raw == 0xa3a3 {
        set.default_palette_id as u16
    } else {
        raw
    };
    if effective != frame.palette_id {
        return Err(invalid("raw and effective SPR2 palette IDs disagree"));
    }
    let transparent_index = frame
        .transparent_index
        .ok_or_else(|| invalid("SPR2 frame is missing its transparent palette index"))?;
    let transparent = palette
        .colors
        .get(transparent_index as usize)
        .ok_or_else(|| invalid("transparent color is outside the palette"))?;
    let n = mul(frame.width as usize, frame.height as usize)?;
    limits.check_count(n, limits.max_pixels, 0, "SPR2 frame pixels")?;
    let rgba = frame
        .rgba
        .as_ref()
        .ok_or_else(|| invalid("SPR2 colors are missing"))?;
    let indices = frame
        .indices
        .as_ref()
        .ok_or_else(|| invalid("SPR2 palette indices are missing"))?;
    if rgba.len() != n || indices.len() != n {
        return Err(invalid("SPR2 arrays differ from frame dimensions"));
    }
    if (frame.flags & 2 != 0) != frame.depth.is_some()
        || frame.depth.as_ref().is_some_and(|depth| depth.len() != n)
    {
        return Err(invalid(
            "SPR2 depth array differs from dimensions or channel flags",
        ));
    }
    let footprint = mul(n, 5 + usize::from(frame.depth.is_some()))?;
    limits.check_count(
        footprint,
        limits.max_total_decoded_bytes,
        0,
        "SPR2 authoring pixels",
    )?;
    let mut changed = 0usize;
    for i in 0..n {
        let color = rgba[i];
        let index = indices[i] as usize;
        let depth = frame.depth.as_ref().map(|depth| depth[i]);
        let palette_color = palette
            .colors
            .get(index)
            .ok_or_else(|| invalid("SPR2 pixel index is outside the palette"))?;
        if color[3] == 0 {
            if color[..3] != transparent[..3]
                || indices[i] != transparent_index as u8
                || depth.is_some_and(|z| z != 255)
            {
                return Err(invalid(
                    "transparent pixels must retain the source transparent RGB/index/depth",
                ));
            }
        } else {
            if color[..3] != palette_color[..3] {
                return Err(invalid(
                    "SPR2 pixel RGB does not match its explicit palette index",
                ));
            }
            if color[3] != 255 {
                if depth.is_none() {
                    return Err(invalid(
                        "SPR2 partial alpha requires the encoded depth channel",
                    ));
                }
                let decoded = (u16::from(quantized(color[3])) * 255 / 31) as u8;
                if color[3] != decoded {
                    if alpha_mode == Spr2AlphaMode::Exact {
                        return Err(invalid("SPR2 alpha requires explicit source quantization"));
                    }
                    changed += 1;
                }
            }
        }
    }
    Ok((raw, footprint, changed))
}

fn quantized(alpha: u8) -> u8 {
    (u16::from(alpha) * 31).div_ceil(255) as u8
}

fn command(frame: &SpriteFrame, pixel: usize) -> u16 {
    match frame.rgba.as_ref().unwrap()[pixel][3] {
        0 => 3,
        255 if frame.depth.as_ref().is_none_or(|depth| depth[pixel] == 0) => 6,
        255 => 1,
        _ => 2,
    }
}

fn write_u16(output: &mut Option<&mut Vec<u8>>, value: u16) {
    if let Some(output) = output {
        output.extend_from_slice(&value.to_le_bytes());
    }
}

fn write_u8(output: &mut Option<&mut Vec<u8>>, value: u8) {
    if let Some(output) = output {
        output.push(value);
    }
}

fn blank_rows(mut count: usize, output: &mut Option<&mut Vec<u8>>) -> usize {
    let size = 2 * count.div_ceil(FIELD_MAX);
    while count != 0 {
        let n = count.min(FIELD_MAX);
        write_u16(output, 0x8000 | n as u16);
        count -= n;
    }
    size
}

fn row_runs(frame: &SpriteFrame, start: usize, mut output: Option<&mut Vec<u8>>) -> Result<usize> {
    let end = start + frame.width as usize;
    let mut pixel = start;
    let mut size = 0usize;
    while pixel < end {
        let cmd = command(frame, pixel);
        let run_start = pixel;
        pixel += 1;
        while pixel < end && pixel - run_start < FIELD_MAX && command(frame, pixel) == cmd {
            pixel += 1;
        }
        let count = pixel - run_start;
        let payload = count
            * match cmd {
                1 => 2,
                2 => 3,
                6 => 1,
                _ => 0,
            };
        size = add(size, add(2, payload + payload % 2)?)?;
        write_u16(&mut output, (cmd << 13) | count as u16);
        for i in run_start..pixel {
            if cmd == 1 || cmd == 2 {
                write_u8(&mut output, frame.depth.as_ref().unwrap()[i]);
            }
            if matches!(cmd, 1 | 2 | 6) {
                write_u8(&mut output, frame.indices.as_ref().unwrap()[i]);
            }
            if cmd == 2 {
                write_u8(&mut output, quantized(frame.rgba.as_ref().unwrap()[i][3]));
            }
        }
        if !payload.is_multiple_of(2) {
            write_u8(&mut output, 0);
        }
    }
    Ok(size)
}

fn command_bytes(frame: &SpriteFrame, mut output: Option<&mut Vec<u8>>) -> Result<usize> {
    let width = frame.width as usize;
    // Height is independent of the pixel budget for width-zero frames. Emit
    // their empty rows directly so max_frames cannot multiply a 65,535-step
    // empty-row scan without consuming any of max_pixels.
    if width == 0 {
        let size = blank_rows(frame.height as usize, &mut output);
        write_u16(&mut output, 0xa000);
        return add(size, 2);
    }
    let rgba = frame.rgba.as_ref().unwrap();
    let mut blanks = 0usize;
    let mut size = 0usize;
    for y in 0..frame.height as usize {
        let start = y * width;
        if rgba[start..start + width].iter().all(|color| color[3] == 0) {
            blanks += 1;
            continue;
        }
        size = add(size, blank_rows(blanks, &mut output))?;
        blanks = 0;
        let row_size = add(2, row_runs(frame, start, None)?)?;
        if row_size > FIELD_MAX {
            return Err(Error::new(
                ErrorKind::LimitExceeded,
                0,
                "SPR2 encoded row exceeds 13-bit byte count",
            ));
        }
        size = add(size, row_size)?;
        write_u16(&mut output, row_size as u16);
        if let Some(output) = &mut output {
            row_runs(frame, start, Some(output))?;
        }
    }
    size = add(size, blank_rows(blanks, &mut output))?;
    write_u16(&mut output, 0xa000);
    add(size, 2)
}
