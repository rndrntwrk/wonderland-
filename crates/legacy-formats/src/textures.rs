// SPDX-License-Identifier: MPL-2.0
//! CPU texture pixels for source MTEX/BMP image resources. Image formats and
//! byte precision are explicit; this is not a renderer or color-management API.

use crate::{reader::Reader, Error, ErrorKind, Limits, Result};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    /// The original PNG channel depth, before expansion to RGBA8. For 16-bit
    /// PNG, the high byte is retained; tools requiring exact 16-bit samples must
    /// keep the original encoded resource or reject that explicit conversion.
    pub source_bit_depth: u8,
    pub rgba: Vec<[u8; 4]>,
}

fn invalid(offset: usize, context: &str) -> Error {
    Error::new(ErrorKind::InvalidData, offset, context)
}

/// Decode a static PNG, including indexed/low-depth samples, transparency,
/// Adam7 and 16-bit input. Raw ancillary data is retained by the caller's input;
/// text/ICC data is not expanded or interpreted. Every chunk CRC and exact IEND
/// are checked before any image allocation.
pub fn decode_png(bytes: &[u8], limits: &Limits) -> Result<RgbaImage> {
    limits.check_input(bytes)?;
    limits.check_count(
        bytes.len(),
        limits.max_resource_bytes,
        0,
        "PNG resource bytes",
    )?;
    let mut r = Reader::new(bytes);
    if r.read_bytes(8)? != b"\x89PNG\r\n\x1a\n" {
        return Err(invalid(0, "PNG signature"));
    }
    let mut chunks = 0usize;
    let mut shape = None;
    loop {
        chunks = chunks
            .checked_add(1)
            .ok_or_else(|| invalid(r.position(), "PNG chunk count overflow"))?;
        limits.check_count(chunks, limits.max_entries, r.position(), "PNG chunk count")?;
        let n = r.u32_be()? as usize;
        let kind = r.read_bytes(4)?;
        let payload = r.read_bytes(n)?;
        let checksum = r.u32_be()?;
        let mut crc = crc32fast::Hasher::new();
        crc.update(kind);
        crc.update(payload);
        if crc.finalize() != checksum {
            return Err(invalid(r.position() - 4, "PNG chunk checksum"));
        }
        if chunks == 1 {
            if kind != b"IHDR" || payload.len() != 13 {
                return Err(invalid(8, "PNG first chunk must be IHDR"));
            }
            shape = Some((
                u32::from_be_bytes(payload[..4].try_into().expect("IHDR width")),
                u32::from_be_bytes(payload[4..8].try_into().expect("IHDR height")),
                payload[8],
            ));
        } else if kind == b"IHDR" {
            return Err(invalid(r.position(), "duplicate PNG IHDR"));
        }
        if matches!(kind, b"acTL" | b"fcTL" | b"fdAT") {
            return Err(Error::new(
                ErrorKind::UnsupportedVersion,
                r.position(),
                "APNG requires an animation decoder",
            ));
        }
        if kind == b"IEND" {
            if n != 0 || r.remaining() != 0 {
                return Err(invalid(r.position(), "PNG IEND/trailing bytes"));
            }
            break;
        }
    }
    let (width, height, source_bit_depth) = shape.ok_or_else(|| invalid(0, "PNG IHDR missing"))?;
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| invalid(0, "PNG dimensions overflow"))?;
    if width == 0 || height == 0 {
        return Err(invalid(0, "PNG dimensions must be positive"));
    }
    limits.check_count(pixels, limits.max_pixels, 0, "PNG pixels")?;
    let output = pixels
        .checked_mul(4)
        .ok_or_else(|| invalid(0, "PNG output size overflow"))?;
    // Reserve input, decoder output, final RGBA plane and fixed inflate state.
    // png::Limits additionally bounds its palette, row and metadata buffers.
    let retained = output
        .checked_mul(2)
        .and_then(|n| n.checked_add(bytes.len()))
        .and_then(|n| n.checked_add(65536))
        .ok_or_else(|| invalid(0, "PNG allocation overflow"))?;
    let decoder_budget = limits
        .max_total_decoded_bytes
        .checked_sub(retained)
        .ok_or_else(|| {
            Error::new(
                ErrorKind::LimitExceeded,
                0,
                "PNG input/output/workspace allocation",
            )
        })?;
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: decoder_budget,
        },
    );
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    let map = |e: png::DecodingError| match e {
        png::DecodingError::LimitsExceeded => {
            Error::new(ErrorKind::LimitExceeded, 0, "PNG decoder allocation limit")
        }
        _ => invalid(0, &format!("PNG decode: {e}")),
    };
    let mut reader = decoder.read_info().map_err(map)?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| invalid(0, "PNG output overflow"))?;
    if size > output {
        return Err(invalid(0, "PNG decoded output exceeds RGBA8 size"));
    }
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(size)
        .map_err(|_| Error::new(ErrorKind::LimitExceeded, 0, "PNG output allocation"))?;
    buffer.resize(size, 0);
    let info = reader.next_frame(&mut buffer).map_err(map)?;
    if info.width != width || info.height != height || info.bit_depth != png::BitDepth::Eight {
        return Err(invalid(0, "PNG decoded shape mismatch"));
    }
    reader.finish().map_err(map)?;
    let channels = info.color_type.samples();
    if info.buffer_size() != pixels * channels {
        return Err(invalid(0, "PNG decoded pixel length mismatch"));
    }
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(pixels)
        .map_err(|_| Error::new(ErrorKind::LimitExceeded, 0, "PNG RGBA allocation"))?;
    for p in buffer[..info.buffer_size()].chunks_exact(channels) {
        rgba.push(match info.color_type {
            png::ColorType::Grayscale => [p[0], p[0], p[0], 255],
            png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
            png::ColorType::Rgb => [p[0], p[1], p[2], 255],
            png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
            png::ColorType::Indexed => return Err(invalid(0, "indexed PNG did not expand")),
        });
    }
    Ok(RgbaImage {
        width,
        height,
        source_bit_depth,
        rgba,
    })
}
