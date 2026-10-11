// SPDX-License-Identifier: MPL-2.0
//! Authoring for original DGRP and SPR# layouts. DGRP preserves all stored
//! fields; SPR# command streams are canonicalized after an edit. Keep original
//! resource bytes for an unchanged SPR# if exact source command bytes matter.

use super::*;
use crate::vitaboy::encode::{encode, Writer};

fn u16(w: &mut Writer<'_>, value: u16, order: SpriteByteOrder) -> Result<()> {
    w.put(&match order {
        SpriteByteOrder::LittleEndian => value.to_le_bytes(),
        SpriteByteOrder::BigEndian => value.to_be_bytes(),
    })
}
fn u32(w: &mut Writer<'_>, value: u32, order: SpriteByteOrder) -> Result<()> {
    w.put(&match order {
        SpriteByteOrder::LittleEndian => value.to_le_bytes(),
        SpriteByteOrder::BigEndian => value.to_be_bytes(),
    })
}
fn narrow(value: u32) -> Result<u16> {
    u16::try_from(value).map_err(|_| invalid(0, "DGRP value exceeds stored u16"))
}
fn count16(value: usize, limits: &Limits) -> Result<u16> {
    limits.check_count(value, limits.max_entries, 0, "DGRP count")?;
    u16::try_from(value).map_err(|_| invalid(0, "DGRP count exceeds u16"))
}
fn count32(value: usize, limits: &Limits) -> Result<u32> {
    limits.check_count(value, limits.max_entries, 0, "DGRP count")?;
    u32::try_from(value).map_err(|_| invalid(0, "DGRP count exceeds u32"))
}

/// Preserve each of the five source layouts, including legacy type words,
/// integer sprite offsets and signed-zero float bits. An absent field must be
/// its decoder's exact +0.0 default; layouts are never silently upgraded.
pub fn encode_dgrp(group: &DrawingGroup, limits: &Limits) -> Result<Vec<u8>> {
    let le = SpriteByteOrder::LittleEndian;
    if !(20000..=20004).contains(&group.version) {
        return Err(unsupported(0, "unsupported DGRP version"));
    }
    let old = group.version < 20003;
    encode(limits, |w| {
        u16(w, group.version, le)?;
        w.retain::<DrawingImage>(group.images.capacity())?;
        if old {
            u16(w, count16(group.images.len(), limits)?, le)?;
        } else {
            u32(w, count32(group.images.len(), limits)?, le)?;
        }
        for image in &group.images {
            w.retain::<DrawingSprite>(image.sprites.capacity())?;
            if old {
                u16(w, count16(image.sprites.len(), limits)?, le)?;
                w.u8(u8::try_from(image.direction)
                    .map_err(|_| invalid(0, "DGRP direction exceeds u8"))?)?;
                w.u8(u8::try_from(image.zoom).map_err(|_| invalid(0, "DGRP zoom exceeds u8"))?)?;
            } else {
                u32(w, image.direction, le)?;
                u32(w, image.zoom, le)?;
                u32(w, count32(image.sprites.len(), limits)?, le)?;
            }
            for sprite in &image.sprites {
                if old {
                    u16(
                        w,
                        sprite
                            .legacy_type
                            .ok_or_else(|| invalid(0, "legacy DGRP type word missing"))?,
                        le,
                    )?;
                    u16(w, narrow(sprite.sprite_id)?, le)?;
                    u16(w, narrow(sprite.frame_index)?, le)?;
                    u16(w, narrow(sprite.flags)?, le)?;
                    for value in sprite.sprite_offset {
                        u16(
                            w,
                            i16::try_from(value)
                                .map_err(|_| invalid(0, "legacy DGRP offset exceeds i16"))?
                                as u16,
                            le,
                        )?;
                    }
                    if sprite.object_offset[0].0 != 0 || sprite.object_offset[1].0 != 0 {
                        return Err(invalid(0, "legacy DGRP has no X/Y object offset"));
                    }
                    if group.version == 20001 {
                        w.float(sprite.object_offset[2])?;
                    } else if sprite.object_offset[2].0 != 0 {
                        return Err(invalid(0, "DGRP layout has no Z object offset"));
                    }
                } else {
                    if sprite.legacy_type.is_some() {
                        return Err(invalid(0, "DGRP layout has no legacy type word"));
                    }
                    u32(w, sprite.sprite_id, le)?;
                    u32(w, sprite.frame_index, le)?;
                    for value in sprite.sprite_offset {
                        u32(w, value as u32, le)?;
                    }
                    w.float(sprite.object_offset[2])?;
                    u32(w, sprite.flags, le)?;
                    if group.version == 20004 {
                        w.float(sprite.object_offset[0])?;
                        w.float(sprite.object_offset[1])?;
                    } else if sprite.object_offset[0].0 != 0 || sprite.object_offset[1].0 != 0 {
                        return Err(invalid(0, "DGRP layout has no X/Y object offset"));
                    }
                }
            }
        }
        Ok(())
    })
}

fn spr_row(
    frame: &SpriteFrame,
    y: usize,
    mut output: impl FnMut(&[u8]) -> Result<()>,
) -> Result<usize> {
    let width = frame.width as usize;
    let indices = frame
        .indices
        .as_ref()
        .ok_or_else(|| invalid(0, "SPR palette indices missing"))?;
    let rgba = frame
        .rgba
        .as_ref()
        .ok_or_else(|| invalid(0, "SPR colors missing"))?;
    let mut x = 0;
    let mut size = 0usize;
    while x < width {
        let start = y * width + x;
        if rgba[start][3] == 0 {
            let mut count = 1;
            while x + count < width && count < 255 && rgba[start + count][3] == 0 {
                count += 1;
            }
            output(&[1, count as u8])?;
            size += 2;
            x += count;
        } else {
            let mut same = 1;
            while x + same < width
                && same < 255
                && rgba[start + same][3] == 255
                && indices[start + same] == indices[start]
            {
                same += 1;
            }
            if same >= 3 {
                output(&[2, same as u8, indices[start], 0])?;
                size += 4;
                x += same;
            } else {
                let mut count = 1;
                while x + count < width && count < 250 && rgba[start + count][3] == 255 {
                    if x + count + 2 < width
                        && rgba[start + count + 1][3] == 255
                        && rgba[start + count + 2][3] == 255
                        && indices[start + count] == indices[start + count + 1]
                        && indices[start + count] == indices[start + count + 2]
                    {
                        break;
                    }
                    count += 1;
                }
                output(&[3, count as u8])?;
                output(&indices[start..start + count])?;
                if count % 2 != 0 {
                    output(&[0])?;
                }
                size += 2 + count + count % 2;
                x += count;
            }
        }
        if size > 253 {
            return Err(invalid(0, "SPR encoded row exceeds one-byte row size"));
        }
    }
    Ok(size)
}

fn spr_commands(frame: &SpriteFrame, mut output: impl FnMut(&[u8]) -> Result<()>) -> Result<usize> {
    let mut size = 0usize;
    let mut y = 0;
    let width = frame.width as usize;
    let rgba = frame
        .rgba
        .as_ref()
        .ok_or_else(|| invalid(0, "SPR colors missing"))?;
    while y < frame.height as usize {
        if rgba[y * width..(y + 1) * width].iter().all(|p| p[3] == 0) {
            let mut count = 1;
            while y + count < frame.height as usize
                && count < 255
                && rgba[(y + count) * width..(y + count + 1) * width]
                    .iter()
                    .all(|p| p[3] == 0)
            {
                count += 1;
            }
            output(&[9, count as u8])?;
            size += 2;
            y += count;
        } else {
            let row_size = spr_row(frame, y, |_| Ok(()))?;
            output(&[4, (row_size + 2) as u8])?;
            spr_row(frame, y, &mut output)?;
            size += row_size + 2;
            y += 1;
        }
    }
    output(&[5, 0])?;
    size.checked_add(2)
        .ok_or_else(|| invalid(0, "SPR encoded size overflow"))
}

/// Encode SPR# 1000 in either admitted byte order, or little-endian 1001.
/// Only source-representable opaque palette pixels and transparent zero pixels
/// are accepted. Dimensions and field widths are validated before output.
pub fn encode_spr(set: &SpriteSet, palette: &Palette, limits: &Limits) -> Result<Vec<u8>> {
    if set.kind != SpriteKind::Spr
        || !matches!(set.version, 1000 | 1001)
        || (set.version == 1001 && set.byte_order == SpriteByteOrder::BigEndian)
    {
        return Err(unsupported(0, "unsupported SPR authoring layout"));
    }
    limits.check_count(set.frames.len(), limits.max_frames, 0, "SPR frame count")?;
    limits.check_count(
        set.declared_frame_count as usize,
        limits.max_frames,
        0,
        "SPR declared frame count",
    )?;
    if set.version == 1000 && set.declared_frame_count as usize != set.frames.len() {
        return Err(invalid(0, "SPR offset table frame count mismatch"));
    }
    if !matches!(palette.version, 0 | 1) || palette.colors.iter().any(|p| p[3] != 255) {
        return Err(invalid(0, "SPR requires an opaque source palette"));
    }
    limits.check_count(
        palette.colors.len(),
        limits.max_entries,
        0,
        "SPR palette colors",
    )?;
    encode(limits, |w| {
        w.retain::<SpriteFrame>(set.frames.capacity())?;
        w.retain::<[u8; 4]>(palette.colors.capacity())?;
        u32(w, set.version, set.byte_order)?;
        u32(w, set.declared_frame_count, set.byte_order)?;
        u32(w, set.default_palette_id, set.byte_order)?;
        let mut offset = 12usize
            .checked_add(if set.version == 1000 {
                set.frames
                    .len()
                    .checked_mul(4)
                    .ok_or_else(|| invalid(0, "SPR offset overflow"))?
            } else {
                0
            })
            .ok_or_else(|| invalid(0, "SPR offset overflow"))?;
        for frame in &set.frames {
            if frame.flags != 1
                || frame.palette_id != set.default_palette_id as u16
                || frame.raw_palette_id.is_some()
                || frame.transparent_index.is_some()
                || frame.position != [0, 0]
                || frame.depth.is_some()
                || frame.fallback_depth != Some(128)
                || (set.version == 1000 && frame.version != 1000)
                || !matches!(frame.version, 1000 | 1001)
            {
                return Err(invalid(0, "SPR frame metadata is not representable"));
            }
            let n = (frame.width as usize)
                .checked_mul(frame.height as usize)
                .ok_or_else(|| invalid(0, "SPR pixels overflow"))?;
            limits.check_count(n, limits.max_pixels, 0, "SPR frame pixels")?;
            let colors = frame
                .rgba
                .as_ref()
                .ok_or_else(|| invalid(0, "SPR colors missing"))?;
            let indices = frame
                .indices
                .as_ref()
                .ok_or_else(|| invalid(0, "SPR indices missing"))?;
            if colors.len() != n || indices.len() != n {
                return Err(invalid(0, "SPR pixel plane length mismatch"));
            }
            w.retain::<[u8; 4]>(colors.capacity())?;
            w.retain::<u8>(indices.capacity())?;
            for (&color, &index) in colors.iter().zip(indices) {
                match color[3] {
                    0 if color == [0, 0, 0, 0] && index == 0 => {}
                    255 if palette.colors.get(index as usize) == Some(&color) => {}
                    _ => return Err(invalid(0, "SPR pixel cannot preserve palette/alpha data")),
                }
            }
            let size = 8usize
                .checked_add(spr_commands(frame, |_| Ok(()))?)
                .ok_or_else(|| invalid(0, "SPR frame size overflow"))?;
            if set.version == 1000 {
                u32(
                    w,
                    u32::try_from(offset).map_err(|_| invalid(0, "SPR offset exceeds u32"))?,
                    set.byte_order,
                )?;
                offset = offset
                    .checked_add(size)
                    .ok_or_else(|| invalid(0, "SPR offset overflow"))?;
            }
        }
        for frame in &set.frames {
            if set.version == 1001 {
                let size = 8usize
                    .checked_add(spr_commands(frame, |_| Ok(()))?)
                    .ok_or_else(|| invalid(0, "SPR frame size overflow"))?;
                u32(w, frame.version, set.byte_order)?;
                u32(
                    w,
                    u32::try_from(size).map_err(|_| invalid(0, "SPR frame size exceeds u32"))?,
                    set.byte_order,
                )?;
            }
            u32(w, frame.reserved, set.byte_order)?;
            u16(w, frame.height, set.byte_order)?;
            u16(w, frame.width, set.byte_order)?;
            spr_commands(frame, |bytes| w.put(bytes))?;
        }
        Ok(())
    })
}
