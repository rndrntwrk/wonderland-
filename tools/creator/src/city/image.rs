// SPDX-License-Identifier: MPL-2.0
//! Exact 8-bit city image samples and source City Painter road/brush operations.
use super::MapLayer;
use std::io::{Cursor, Write};
use wonderland_legacy_formats::Limits;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CityImage {
    width: usize,
    height: usize,
    pixels: Vec<[u8; 4]>,
}
fn admit(width: usize, height: usize, input: usize, limits: &Limits) -> Result<usize, String> {
    let count = width
        .checked_mul(height)
        .ok_or("city dimensions overflow")?;
    if width == 0
        || height == 0
        || count > limits.max_pixels
        || input > limits.max_input_bytes
        || input
            .checked_mul(2)
            .and_then(|n| count.checked_mul(24).and_then(|p| p.checked_add(n)))
            .and_then(|n| n.checked_add(65536))
            .is_none_or(|n| n > limits.max_total_decoded_bytes)
    {
        return Err("city image working-copy limit exceeded".into());
    }
    Ok(count)
}
impl CityImage {
    pub fn from_rgba(
        width: usize,
        height: usize,
        pixels: Vec<[u8; 4]>,
        limits: &Limits,
    ) -> Result<Self, String> {
        let count = admit(width, height, 0, limits)?;
        if pixels.len() != count
            || pixels
                .capacity()
                .checked_mul(4)
                .is_none_or(|n| n > limits.max_total_decoded_bytes)
        {
            return Err("city pixel count/capacity mismatch".into());
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }
    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn pixels(&self) -> &[[u8; 4]] {
        &self.pixels
    }
    fn index(&self, x: usize, y: usize) -> Result<usize, String> {
        if x >= self.width || y >= self.height {
            return Err("city pixel out of bounds".into());
        }
        Ok(x + y * self.width)
    }
    pub fn pixel(&self, x: usize, y: usize) -> Result<[u8; 4], String> {
        Ok(self.pixels[self.index(x, y)?])
    }
    pub fn set_pixel(
        &mut self,
        x: usize,
        y: usize,
        rgb: [u8; 3],
        layer: MapLayer,
    ) -> Result<(), String> {
        let index = self.index(x, y)?;
        if !layer.accepts(rgb) {
            return Err("pixel is not in source layer palette".into());
        }
        self.pixels[index][..3].copy_from_slice(&rgb);
        Ok(())
    }
    pub fn validate(&self, layer: MapLayer, require_city_dimensions: bool) -> Result<(), String> {
        if require_city_dimensions && (self.width != 512 || self.height != 512) {
            return Err("FreeSO city maps must be 512 by 512".into());
        }
        for (i, p) in self.pixels.iter().enumerate() {
            if !layer.accepts([p[0], p[1], p[2]]) || p[3] != 255 {
                return Err(format!(
                    "invalid source city pixel at ({},{})",
                    i % self.width,
                    i / self.width
                ));
            }
        }
        Ok(())
    }
    pub fn decode(bytes: &[u8], limits: &Limits) -> Result<Self, String> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Self::png(bytes, limits)
        } else if bytes.starts_with(b"BM") {
            Self::bmp(bytes, limits)
        } else {
            Err("city image must be PNG or BMP".into())
        }
    }
    fn png(bytes: &[u8], limits: &Limits) -> Result<Self, String> {
        if bytes.len() < 33 || &bytes[12..16] != b"IHDR" {
            return Err("invalid PNG header".into());
        }
        let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap()) as usize;
        let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap()) as usize;
        let count = admit(width, height, bytes.len(), limits)?;
        let reserve = bytes.len() * 2 + count * 16;
        let mut decoder = png::Decoder::new_with_limits(
            Cursor::new(bytes),
            png::Limits {
                bytes: limits.max_total_decoded_bytes.saturating_sub(reserve),
            },
        );
        decoder.set_transformations(png::Transformations::EXPAND);
        decoder.set_ignore_text_chunk(true);
        decoder.set_ignore_iccp_chunk(true);
        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        if reader.info().animation_control.is_some() {
            return Err("animated PNG is not a city map".into());
        }
        if reader.info().bit_depth == png::BitDepth::Sixteen {
            return Err(
                "16-bit PNG requires an explicit precision conversion before city import".into(),
            );
        }
        let length = reader.output_buffer_size().ok_or("PNG output overflow")?;
        if length > count * 4 {
            return Err("PNG output exceeds city pixel budget".into());
        }
        let mut buffer = vec![0; length];
        let info = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
        if info.width as usize != width
            || info.height as usize != height
            || info.bit_depth != png::BitDepth::Eight
        {
            return Err("PNG output shape mismatch".into());
        }
        reader.finish().map_err(|e| e.to_string())?;
        let channels = info.color_type.samples();
        if info.buffer_size() != count * channels {
            return Err("PNG decoded length mismatch".into());
        }
        let pixels = buffer[..info.buffer_size()]
            .chunks_exact(channels)
            .map(|p| match info.color_type {
                png::ColorType::Grayscale => [p[0], p[0], p[0], 255],
                png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
                png::ColorType::Rgb => [p[0], p[1], p[2], 255],
                png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
                png::ColorType::Indexed => unreachable!("EXPAND removes indexed output"),
            })
            .collect();
        Ok(Self {
            width,
            height,
            pixels,
        })
    }
    fn bmp(bytes: &[u8], limits: &Limits) -> Result<Self, String> {
        if bytes.len() < 54 {
            return Err("truncated BMP header".into());
        }
        let u16at = |p| u16::from_le_bytes(bytes[p..p + 2].try_into().unwrap());
        let u32at = |p| u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
        let i32at = |p| i32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
        let dib = u32at(14) as usize;
        if u32at(2) as usize != bytes.len()
            || !matches!(dib, 40 | 108 | 124)
            || 14 + dib > bytes.len()
            || u16at(26) != 1
        {
            return Err("unsupported BMP envelope".into());
        }
        let w = i32at(18);
        let h = i32at(22);
        if w <= 0 || h == 0 || h == i32::MIN {
            return Err("invalid BMP dimensions".into());
        }
        let width = w as usize;
        let height = h.unsigned_abs() as usize;
        let count = admit(width, height, bytes.len(), limits)?;
        let bits = u16at(28) as usize;
        let compression = u32at(30);
        if !matches!(
            (bits, compression),
            (1 | 4 | 8 | 24 | 32, 0) | (8, 1) | (4, 2)
        ) {
            return Err("unsupported BMP bit depth/compression".into());
        }
        if compression != 0 && h < 0 {
            return Err("top-down RLE BMP is invalid".into());
        }
        let offset = u32at(10) as usize;
        if offset < 14 + dib || offset > bytes.len() {
            return Err("invalid BMP data offset".into());
        }
        let mut palette = Vec::new();
        if bits <= 8 {
            let colors = if u32at(46) == 0 {
                1usize << bits
            } else {
                u32at(46) as usize
            };
            if colors > 1usize << bits || 14 + dib + colors * 4 > offset {
                return Err("invalid BMP palette extent".into());
            }
            for c in bytes[14 + dib..14 + dib + colors * 4].chunks_exact(4) {
                palette.push([c[2], c[1], c[0], 255]);
            }
        }
        let mut pixels = vec![palette.first().copied().unwrap_or([0, 0, 0, 255]); count];
        let mut put = |x: usize, y: usize, c: usize| -> Result<(), String> {
            if x >= width || y >= height {
                return Err("RLE BMP pixel outside image".into());
            }
            pixels[x + (height - 1 - y) * width] =
                *palette.get(c).ok_or("BMP palette index out of range")?;
            Ok(())
        };
        if compression != 0 {
            let mut pos = offset;
            let mut x = 0usize;
            let mut y = 0usize;
            let mut ended = false;
            while pos < bytes.len() {
                if pos + 2 > bytes.len() {
                    return Err("truncated BMP RLE command".into());
                }
                let count = bytes[pos] as usize;
                let value = bytes[pos + 1];
                pos += 2;
                if count > 0 {
                    if x.checked_add(count).is_none_or(|n| n > width) {
                        return Err("BMP RLE run exceeds row".into());
                    }
                    for i in 0..count {
                        let color = if bits == 8 {
                            value as usize
                        } else if i % 2 == 0 {
                            (value >> 4) as usize
                        } else {
                            (value & 15) as usize
                        };
                        put(x + i, y, color)?;
                    }
                    x += count;
                } else {
                    match value {
                        0 => {
                            x = 0;
                            y = y.checked_add(1).ok_or("BMP row overflow")?;
                            if y > height {
                                return Err("BMP RLE rows exceed image".into());
                            }
                        }
                        1 => {
                            ended = true;
                            break;
                        }
                        2 => {
                            if pos + 2 > bytes.len() {
                                return Err("truncated BMP RLE delta".into());
                            }
                            x += bytes[pos] as usize;
                            y += bytes[pos + 1] as usize;
                            pos += 2;
                            if x > width || y >= height {
                                return Err("BMP RLE delta outside image".into());
                            }
                        }
                        n => {
                            let n = n as usize;
                            let raw = if bits == 8 { n } else { n.div_ceil(2) };
                            let padded = (raw + 1) & !1;
                            if pos + padded > bytes.len() || x + n > width {
                                return Err("truncated or overflowing BMP RLE absolute run".into());
                            }
                            for i in 0..n {
                                let color = if bits == 8 {
                                    bytes[pos + i] as usize
                                } else {
                                    let b = bytes[pos + i / 2];
                                    if i % 2 == 0 {
                                        (b >> 4) as usize
                                    } else {
                                        (b & 15) as usize
                                    }
                                };
                                put(x + i, y, color)?;
                            }
                            x += n;
                            pos += padded;
                        }
                    }
                }
            }
            if !ended {
                return Err("BMP RLE end marker missing".into());
            }
        } else {
            let stride = width
                .checked_mul(bits)
                .and_then(|n| n.checked_add(31))
                .map(|n| (n / 32) * 4)
                .ok_or("BMP stride overflow")?;
            let end = offset
                .checked_add(stride.checked_mul(height).ok_or("BMP storage overflow")?)
                .ok_or("BMP storage overflow")?;
            if end > bytes.len() {
                return Err("truncated BMP pixels".into());
            }
            for y in 0..height {
                for x in 0..width {
                    let row = offset + (if h < 0 { y } else { height - 1 - y }) * stride;
                    let color = if bits <= 8 {
                        let b = bytes[row + x * bits / 8];
                        let shift = 8 - bits - (x * bits % 8);
                        let i = ((b >> shift) & ((1u16 << bits) - 1) as u8) as usize;
                        *palette.get(i).ok_or("BMP palette index out of range")?
                    } else {
                        let p = row + x * (bits / 8);
                        [
                            bytes[p + 2],
                            bytes[p + 1],
                            bytes[p],
                            // BI_RGB32's fourth byte is unused, not alpha.
                            // BITMAPINFOHEADER biBitCount=32 (Microsoft).
                            255,
                        ]
                    };
                    pixels[x + y * width] = color;
                }
            }
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }
    /// Source BrushFunc footprint (width+0.5 denominator, cosine strength > 0).
    /// Categorical paint clips at image edges and preserves alpha and other pixels.
    pub fn brush(
        &mut self,
        x: i32,
        y: i32,
        radius: usize,
        rgb: [u8; 3],
        layer: MapLayer,
        limits: &Limits,
    ) -> Result<(), String> {
        admit(self.width, self.height, 0, limits)?;
        if radius > 512 || !layer.accepts(rgb) {
            return Err("invalid city brush radius or palette".into());
        }
        for dy in -(radius as i32)..=radius as i32 {
            for dx in -(radius as i32)..=radius as i32 {
                let distance = f64::from(dx * dx + dy * dy).sqrt() / (radius as f64 + 0.5);
                let strength = (distance * std::f64::consts::FRAC_PI_2).cos().max(0.0) as f32;
                let tx = i64::from(x) + i64::from(dx);
                let ty = i64::from(y) + i64::from(dy);
                if strength > 0.0
                    && tx >= 0
                    && ty >= 0
                    && (tx as usize) < self.width
                    && (ty as usize) < self.height
                {
                    self.set_pixel(tx as usize, ty as usize, rgb, layer)?;
                }
            }
        }
        Ok(())
    }
    /// DrawWall/EraseWall, with exact source direction tables and corner masks.
    /// Out-of-bounds strokes fail atomically; rows never wrap at an image edge.
    pub fn road_stroke(
        &mut self,
        x: i32,
        y: i32,
        length: usize,
        direction: u8,
        erase: bool,
        limits: &Limits,
    ) -> Result<(), String> {
        admit(self.width, self.height, 0, limits)?;
        if direction > 3 || length > 512 {
            return Err("invalid road direction or length".into());
        }
        let d = direction as usize;
        let start = [(0, 0), (0, 0), (-1, 0), (0, -1)][d];
        let sub = [(0, -1), (-1, 0), (0, -1), (-1, 0)][d];
        let step = [(1, 0), (0, 1), (-1, 0), (0, -1)][d];
        let main_seg = [8, 1, 8, 1][d];
        let sub_seg = [2, 4, 2, 4][d];
        let main_corner = [128, 32, 16, 16][d];
        let sub_corner = [64, 64, 32, 128][d];
        let main_end = [16, 16, 128, 32][d];
        let sub_end = [32, 128, 64, 64][d];
        let mut candidate = self.pixels.clone();
        let mut write = |x: i64, y: i64, add: u8, clear: u8, corner: bool| -> Result<(), String> {
            if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
                return Err("road stroke outside city image".into());
            }
            let p = &mut candidate[x as usize + y as usize * self.width];
            let mut value = p[0];
            let edges = match add {
                16 => 1 | 8,
                32 => 1 | 2,
                64 => 2 | 4,
                128 => 4 | 8,
                _ => 0,
            };
            if !corner || value & edges == 0 {
                value |= add;
            }
            value &= !clear;
            p[..3].fill(value);
            Ok(())
        };
        let mut px = i64::from(x) + start.0;
        let mut py = i64::from(y) + start.1;
        for (cx, cy, mask) in [
            (px - step.0, py - step.1, main_corner),
            (px - step.0 + sub.0, py - step.1 + sub.1, sub_corner),
        ] {
            write(
                cx,
                cy,
                if erase { 0 } else { mask },
                if erase { mask } else { 0 },
                !erase,
            )?;
        }
        for _ in 0..length {
            write(
                px,
                py,
                if erase { 0 } else { main_seg },
                if erase {
                    main_seg
                } else {
                    main_corner | main_end
                },
                false,
            )?;
            write(
                px + sub.0,
                py + sub.1,
                if erase { 0 } else { sub_seg },
                if erase { sub_seg } else { sub_corner | sub_end },
                false,
            )?;
            px += step.0;
            py += step.1;
        }
        for (cx, cy, mask) in [(px, py, main_end), (px + sub.0, py + sub.1, sub_end)] {
            write(
                cx,
                cy,
                if erase { 0 } else { mask },
                if erase { mask } else { 0 },
                !erase,
            )?;
        }
        self.pixels = candidate;
        Ok(())
    }
    pub fn encode_png(&self, limits: &Limits) -> Result<Vec<u8>, String> {
        admit(self.width, self.height, 0, limits)?;
        struct Bounded {
            bytes: Vec<u8>,
            max: usize,
        }
        impl Write for Bounded {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                if self
                    .bytes
                    .len()
                    .checked_add(b.len())
                    .is_none_or(|n| n > self.max)
                {
                    return Err(std::io::Error::other("PNG output byte limit exceeded"));
                }
                self.bytes.extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut output = Bounded {
            bytes: Vec::new(),
            max: limits.max_resource_bytes.min(limits.max_input_bytes),
        };
        let mut encoder = png::Encoder::new(
            &mut output,
            u32::try_from(self.width).map_err(|_| "PNG width overflow")?,
            u32::try_from(self.height).map_err(|_| "PNG height overflow")?,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        let buffer: Vec<u8> = self.pixels.iter().flatten().copied().collect();
        writer
            .write_image_data(&buffer)
            .map_err(|e| e.to_string())?;
        writer.finish().map_err(|e| e.to_string())?;
        Ok(output.bytes)
    }
    /// Canonical top-down opaque BI_RGB32 BITMAPINFOHEADER. Its unused fourth
    /// byte is zero; callers with transparency must export PNG instead.
    pub fn encode_bmp(&self, limits: &Limits) -> Result<Vec<u8>, String> {
        admit(self.width, self.height, 0, limits)?;
        if self.pixels.iter().any(|pixel| pixel[3] != 255) {
            return Err("BI_RGB BMP cannot store alpha; export PNG to retain transparency".into());
        }
        let length = self
            .pixels
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add(54))
            .ok_or("BMP length overflow")?;
        if length > limits.max_resource_bytes.min(limits.max_input_bytes)
            || length > u32::MAX as usize
        {
            return Err("BMP output byte limit exceeded".into());
        }
        let w = i32::try_from(self.width).map_err(|_| "BMP width overflow")?;
        let h = i32::try_from(self.height).map_err(|_| "BMP height overflow")?;
        let mut bytes = vec![0; 54];
        bytes[..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&(length as u32).to_le_bytes());
        bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
        bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&w.to_le_bytes());
        bytes[22..26].copy_from_slice(&(-h).to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&32u16.to_le_bytes());
        bytes[34..38].copy_from_slice(&((length - 54) as u32).to_le_bytes());
        for p in &self.pixels {
            bytes.extend_from_slice(&[p[2], p[1], p[0], 0]);
        }
        Ok(bytes)
    }
}
