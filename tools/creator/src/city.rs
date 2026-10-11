// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, obtain one at https://mozilla.org/MPL/2.0/.
//! Byte-preserving editing of the explicitly supported Windows BITMAPINFOHEADER format.
mod image;
mod neighborhood;
pub use image::CityImage;
pub use neighborhood::NeighborhoodDocument;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapLayer {
    Terrain,
    Elevation,
    ForestDensity,
    ForestType,
    Road,
    VertexColor,
}
impl MapLayer {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "terrain" => Ok(Self::Terrain),
            "elevation" => Ok(Self::Elevation),
            "forest-density" => Ok(Self::ForestDensity),
            "forest-type" => Ok(Self::ForestType),
            "road" => Ok(Self::Road),
            "vertex-color" => Ok(Self::VertexColor),
            _ => Err("unknown map layer".into()),
        }
    }
    fn accepts(self, rgb: [u8; 3]) -> bool {
        match self {
            Self::Terrain => [
                [0, 255, 0],
                [12, 0, 255],
                [255, 255, 255],
                [255, 0, 0],
                [255, 255, 0],
                [0, 0, 0],
            ]
            .contains(&rgb),
            Self::ForestType => [
                [0, 0x6a, 0x28],
                [0, 0xeb, 0x42],
                [255, 0xfc, 0],
                [255, 0, 0],
                [0, 0, 0],
            ]
            .contains(&rgb),
            _ => true,
        }
    }
}
/// Validated BMP geometry is read-only; editing pixels cannot invalidate storage layout.
///
/// ```compile_fail
/// use wonderland_creator::city::BmpMap;
/// fn invalidate_width(mut map: BmpMap) { map.width = 2; }
/// ```
///
/// ```compile_fail
/// use wonderland_creator::city::BmpMap;
/// fn invalidate_height(mut map: BmpMap) { map.height = 2; }
/// ```
#[derive(Clone)]
pub struct BmpMap {
    bytes: Vec<u8>,
    width: usize,
    height: usize,
    offset: usize,
    stride: usize,
    channels: usize,
    top_down: bool,
}
impl BmpMap {
    /// The width established by decoding the BMP header and validating its pixel storage.
    pub fn width(&self) -> usize {
        self.width
    }
    /// The height established by decoding the BMP header and validating its pixel storage.
    pub fn height(&self) -> usize {
        self.height
    }

    pub fn decode(bytes: &[u8], max_pixels: usize) -> Result<Self, String> {
        if bytes.len() > crate::workspace::MAX_FILE_BYTES {
            return Err("BMP input byte limit exceeded".into());
        }
        if bytes.len() < 54 || &bytes[..2] != b"BM" {
            return Err("truncated or invalid BMP signature".into());
        }
        let u16_at = |p| u16::from_le_bytes(bytes[p..p + 2].try_into().unwrap());
        let u32_at = |p| u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
        let i32_at = |p| i32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
        if u32_at(2) as usize != bytes.len()
            || u32_at(14) != 40
            || u16_at(26) != 1
            || u32_at(30) != 0
        {
            return Err("unsupported BMP size, DIB, planes or compression".into());
        }
        let w = i32_at(18);
        let h = i32_at(22);
        if w <= 0 || h == 0 || h == i32::MIN {
            return Err("invalid BMP dimensions".into());
        }
        let channels = match u16_at(28) {
            24 => 3,
            32 => 4,
            _ => return Err("only uncompressed RGB24/RGBA32 BMP is supported".into()),
        };
        let width = w as usize;
        let height = h.unsigned_abs() as usize;
        let pixels = width.checked_mul(height).ok_or("BMP dimensions overflow")?;
        if pixels > max_pixels {
            return Err("BMP pixel limit exceeded".into());
        }
        let stride = width
            .checked_mul(channels)
            .and_then(|n| n.checked_add(3))
            .ok_or("BMP stride overflow")?
            & !3;
        let offset = u32_at(10) as usize;
        if offset < 54 {
            return Err("BMP pixel data overlaps header".into());
        }
        let pixel_bytes = stride.checked_mul(height).ok_or("BMP length overflow")?;
        let end = offset
            .checked_add(pixel_bytes)
            .ok_or("BMP offset overflow")?;
        if end > bytes.len() {
            return Err("truncated BMP pixels".into());
        }
        if u32_at(34) != 0 && u32_at(34) as usize != pixel_bytes {
            return Err("BMP declared pixel size mismatch".into());
        }
        Ok(Self {
            bytes: bytes.to_vec(),
            width,
            height,
            offset,
            stride,
            channels,
            top_down: h < 0,
        })
    }
    fn position(&self, x: usize, y: usize) -> Result<usize, String> {
        if x >= self.width || y >= self.height {
            return Err("pixel out of bounds".into());
        }
        let row = if self.top_down {
            y
        } else {
            self.height - 1 - y
        };
        Ok(self.offset + row * self.stride + x * self.channels)
    }
    pub fn pixel(&self, x: usize, y: usize) -> Result<[u8; 3], String> {
        let p = self.position(x, y)?;
        Ok([self.bytes[p + 2], self.bytes[p + 1], self.bytes[p]])
    }
    pub fn set_pixel(
        &mut self,
        x: usize,
        y: usize,
        rgb: [u8; 3],
        layer: MapLayer,
    ) -> Result<(), String> {
        let p = self.position(x, y)?;
        if !layer.accepts(rgb) {
            return Err("pixel is not in the source map layer palette".into());
        }
        self.bytes[p..p + 3].copy_from_slice(&[rgb[2], rgb[1], rgb[0]]);
        Ok(())
    }
    pub fn validate(&self, layer: MapLayer, require_city_dimensions: bool) -> Result<(), String> {
        if require_city_dimensions && (self.width != 512 || self.height != 512) {
            return Err("FreeSO city data maps must be 512 by 512".into());
        }
        for y in 0..self.height {
            for x in 0..self.width {
                if !layer.accepts(self.pixel(x, y)?) {
                    return Err(format!("invalid map palette at ({x},{y})"));
                }
            }
        }
        Ok(())
    }
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }
    /// P6 PPM interchange preserves exact RGB samples; the alpha channel is omitted explicitly.
    pub fn ppm(&self) -> Vec<u8> {
        let mut out = format!("P6\n{} {}\n255\n", self.width, self.height).into_bytes();
        for y in 0..self.height {
            for x in 0..self.width {
                out.extend_from_slice(&self.pixel(x, y).unwrap());
            }
        }
        out
    }
}
