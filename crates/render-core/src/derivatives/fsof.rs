//! Original `FSOf` version 1 container, without engine/GPU ownership.
//!
//! Matches TSOClient/tso.files/RC/FSOF.cs: a nine-byte uncompressed header,
//! optional gzip body, RGBA8/BC3 bytes, then two Position/UV/Normal meshes.
use super::{DerivativeError, ImageRole};
use crate::{Mesh, RenderLimits, RgbaImage, Vec2, Vec3, Vertex};
use flate2::{bufread::GzDecoder, write::GzEncoder, Compression};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

type Result<T> = std::result::Result<T, DerivativeError>;
fn invalid() -> DerivativeError {
    DerivativeError::Invalid("FSOf structure")
}
fn limit() -> DerivativeError {
    DerivativeError::Limit("FSOf bytes/counts")
}
fn io(_: std::io::Error) -> DerivativeError {
    DerivativeError::Invalid("FSOf I/O or gzip checksum")
}

#[derive(Clone, Copy, Debug)]
pub struct FsofLimits {
    pub max_file_bytes: u64,
    pub max_decoded_bytes: u64,
    /// Sum across both meshes, not a per-mesh allowance.
    pub max_vertices: usize,
    pub max_indices: usize,
    pub max_dimension: u32,
    /// Sum of all day/night texture pixels, including BC3 decode capacity.
    pub max_texture_pixels: u64,
}
impl Default for FsofLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 128 * 1024 * 1024,
            max_decoded_bytes: 128 * 1024 * 1024,
            max_vertices: 2_000_000,
            max_indices: 6_000_000,
            max_dimension: 4096,
            max_texture_pixels: 32 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextureCompression {
    Rgba8,
    Dxt5,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FsofVertex {
    pub position: Vec3,
    pub uv: Vec2,
    /// Preserved exactly on read/write. The source normal generator leaves
    /// non-finite normals on unused vertices; `to_mesh` repairs those locally.
    pub normal: Vec3,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FsofMesh {
    pub vertices: Vec<FsofVertex>,
    pub indices: Vec<u32>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FacadeGeometry {
    pub floor: FsofMesh,
    pub wall: FsofMesh,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsofNight {
    pub floor_texture: Vec<u8>,
    pub wall_texture: Vec<u8>,
    /// Source XNA Color.PackedValue little-endian RGBA.
    pub light_color: [u8; 4],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fsof {
    pub compression: TextureCompression,
    pub floor_width: u32,
    pub floor_height: u32,
    pub wall_width: u32,
    pub wall_height: u32,
    pub floor_texture: Vec<u8>,
    pub wall_texture: Vec<u8>,
    pub night: Option<FsofNight>,
    pub geometry: FacadeGeometry,
}

struct Reader<R> {
    inner: R,
    used: u64,
    limits: FsofLimits,
    vertices: usize,
    indices: usize,
}
impl<R: Read> Reader<R> {
    fn available(&self, n: usize) -> Result<()> {
        if self
            .used
            .checked_add(n as u64)
            .filter(|n| *n <= self.limits.max_decoded_bytes)
            .is_none()
        {
            return Err(limit());
        }
        Ok(())
    }
    fn read<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.available(N)?;
        let mut a = [0; N];
        self.inner.read_exact(&mut a).map_err(io)?;
        self.used += N as u64;
        Ok(a)
    }
    fn count(&mut self) -> Result<usize> {
        let n = i32::from_le_bytes(self.read()?);
        usize::try_from(n).map_err(|_| invalid())
    }
    fn texture(&mut self, expected: usize) -> Result<Vec<u8>> {
        if self.count()? != expected {
            return Err(invalid());
        }
        self.available(expected)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(expected)
            .map_err(|_| DerivativeError::Allocation)?;
        bytes.resize(expected, 0);
        self.inner.read_exact(&mut bytes).map_err(io)?;
        self.used += expected as u64;
        Ok(bytes)
    }
    fn mesh(&mut self) -> Result<FsofMesh> {
        let count = self.count()?;
        self.vertices = self
            .vertices
            .checked_add(count)
            .filter(|n| *n <= self.limits.max_vertices)
            .ok_or_else(limit)?;
        self.available(count.checked_mul(32).ok_or_else(limit)?)?;
        let mut vertices = Vec::new();
        vertices
            .try_reserve_exact(count)
            .map_err(|_| DerivativeError::Allocation)?;
        for _ in 0..count {
            let mut values = [0.; 8];
            for value in &mut values {
                *value = f32::from_le_bytes(self.read()?);
            }
            if values[..5].iter().any(|f| !f.is_finite()) {
                return Err(invalid());
            }
            vertices.push(FsofVertex {
                position: Vec3::new(values[0], values[1], values[2]),
                uv: Vec2::new(values[3], values[4]),
                normal: Vec3::new(values[5], values[6], values[7]),
            });
        }
        let count = self.count()?;
        self.indices = self
            .indices
            .checked_add(count)
            .filter(|n| *n <= self.limits.max_indices)
            .ok_or_else(limit)?;
        if count % 3 != 0 {
            return Err(invalid());
        }
        self.available(count.checked_mul(4).ok_or_else(limit)?)?;
        let mut indices = Vec::new();
        indices
            .try_reserve_exact(count)
            .map_err(|_| DerivativeError::Allocation)?;
        for _ in 0..count {
            let index = self.count()?;
            if index >= vertices.len() {
                return Err(invalid());
            }
            indices.push(index as u32);
        }
        Ok(FsofMesh { vertices, indices })
    }
    fn finish(&mut self) -> Result<()> {
        let mut tail = [0];
        if self.inner.read(&mut tail).map_err(io)? != 0 {
            return Err(invalid());
        }
        Ok(())
    }
}

fn dimensions(
    width: u32,
    height: u32,
    compression: TextureCompression,
    limits: FsofLimits,
) -> Result<usize> {
    if width == 0 || height == 0 || width > limits.max_dimension || height > limits.max_dimension {
        return Err(invalid());
    }
    let bytes = match compression {
        TextureCompression::Rgba8 => u64::from(width) * u64::from(height) * 4,
        TextureCompression::Dxt5 => {
            u64::from(width).div_ceil(4) * u64::from(height).div_ceil(4) * 16
        }
    };
    if bytes > limits.max_decoded_bytes || bytes > i32::MAX as u64 {
        return Err(limit());
    }
    usize::try_from(bytes).map_err(|_| limit())
}
fn pixel_budget(fw: u32, fh: u32, ww: u32, wh: u32, night: bool, limits: FsofLimits) -> Result<()> {
    let pixels = (u64::from(fw) * u64::from(fh) + u64::from(ww) * u64::from(wh))
        * (if night { 2 } else { 1 });
    if pixels > limits.max_texture_pixels {
        return Err(limit());
    }
    Ok(())
}
fn read_body<R: Read>(inner: R, limits: FsofLimits) -> Result<Fsof> {
    let mut reader = Reader {
        inner,
        used: 0,
        limits,
        vertices: 0,
        indices: 0,
    };
    let compression = match reader.count()? {
        0 => TextureCompression::Rgba8,
        1 => TextureCompression::Dxt5,
        _ => return Err(invalid()),
    };
    let fw = reader.count()? as u32;
    let fh = reader.count()? as u32;
    let ww = reader.count()? as u32;
    let wh = reader.count()? as u32;
    let night = match reader.read::<1>()?[0] {
        0 => false,
        1 => true,
        _ => return Err(invalid()),
    };
    let floor_len = dimensions(fw, fh, compression, limits)?;
    let wall_len = dimensions(ww, wh, compression, limits)?;
    pixel_budget(fw, fh, ww, wh, night, limits)?;
    let floor_texture = reader.texture(floor_len)?;
    let wall_texture = reader.texture(wall_len)?;
    let night = if night {
        Some(FsofNight {
            floor_texture: reader.texture(floor_len)?,
            wall_texture: reader.texture(wall_len)?,
            light_color: reader.read()?,
        })
    } else {
        None
    };
    let geometry = FacadeGeometry {
        floor: reader.mesh()?,
        wall: reader.mesh()?,
    };
    reader.finish()?;
    Ok(Fsof {
        compression,
        floor_width: fw,
        floor_height: fh,
        wall_width: ww,
        wall_height: wh,
        floor_texture,
        wall_texture,
        night,
        geometry,
    })
}

impl Fsof {
    pub fn decode(bytes: &[u8], limits: FsofLimits) -> Result<Self> {
        if bytes.len() as u64 > limits.max_file_bytes {
            return Err(limit());
        }
        if bytes.len() < 9 || &bytes[..4] != b"FSOf" || bytes[4..8] != 1i32.to_le_bytes() {
            return Err(invalid());
        }
        match bytes[8] {
            0 => read_body(&bytes[9..], limits),
            1 => {
                // BufRead decoder stops exactly after the first member, so
                // concatenated members and trailing payload cannot be ignored.
                let mut decoder = GzDecoder::new(&bytes[9..]);
                let out = read_body(&mut decoder, limits)?;
                if !decoder.get_ref().is_empty() {
                    return Err(invalid());
                }
                Ok(out)
            }
            _ => Err(invalid()),
        }
    }
    pub fn validate(&self, limits: FsofLimits) -> Result<u64> {
        let floor = dimensions(
            self.floor_width,
            self.floor_height,
            self.compression,
            limits,
        )?;
        let wall = dimensions(self.wall_width, self.wall_height, self.compression, limits)?;
        pixel_budget(
            self.floor_width,
            self.floor_height,
            self.wall_width,
            self.wall_height,
            self.night.is_some(),
            limits,
        )?;
        if self.floor_texture.len() != floor || self.wall_texture.len() != wall {
            return Err(invalid());
        }
        let mut size = 21u64 + 8 + floor as u64 + wall as u64;
        if let Some(night) = &self.night {
            if night.floor_texture.len() != floor || night.wall_texture.len() != wall {
                return Err(invalid());
            }
            size = size
                .checked_add(12 + floor as u64 + wall as u64)
                .ok_or_else(limit)?;
        }
        self.geometry.validate(limits)?;
        for mesh in [&self.geometry.floor, &self.geometry.wall] {
            size = size
                .checked_add(8 + mesh.vertices.len() as u64 * 32 + mesh.indices.len() as u64 * 4)
                .ok_or_else(limit)?;
        }
        if size > limits.max_decoded_bytes {
            return Err(limit());
        }
        Ok(size)
    }
    pub fn encode(&self, compressed: bool, limits: FsofLimits) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        self.write(&mut out, compressed, limits)?;
        Ok(out)
    }
    /// Validates all fields before the first byte is written. The compressed
    /// stream is additionally capped while writing, without staging the body.
    pub fn write(&self, writer: impl Write, compressed: bool, limits: FsofLimits) -> Result<()> {
        let size = self.validate(limits)?;
        if !compressed
            && size
                .checked_add(9)
                .filter(|n| *n <= limits.max_file_bytes)
                .is_none()
        {
            return Err(limit());
        }
        let mut writer = LimitedWriter {
            inner: writer,
            left: limits.max_file_bytes,
        };
        writer.write_all(b"FSOf\x01\0\0\0").map_err(io)?;
        writer.write_all(&[u8::from(compressed)]).map_err(io)?;
        if compressed {
            let mut gzip = GzEncoder::new(writer, Compression::default());
            self.write_body(&mut gzip)?;
            gzip.finish().map_err(io)?;
        } else {
            self.write_body(&mut writer)?;
        }
        Ok(())
    }
    fn write_body(&self, out: &mut impl Write) -> Result<()> {
        let int = |out: &mut dyn Write, value: usize| -> Result<()> {
            out.write_all(&(value as i32).to_le_bytes()).map_err(io)
        };
        int(
            out,
            if self.compression == TextureCompression::Dxt5 {
                1
            } else {
                0
            },
        )?;
        for n in [
            self.floor_width,
            self.floor_height,
            self.wall_width,
            self.wall_height,
        ] {
            int(out, n as usize)?;
        }
        out.write_all(&[u8::from(self.night.is_some())])
            .map_err(io)?;
        for bytes in [&self.floor_texture, &self.wall_texture] {
            int(out, bytes.len())?;
            out.write_all(bytes).map_err(io)?;
        }
        if let Some(night) = &self.night {
            for bytes in [&night.floor_texture, &night.wall_texture] {
                int(out, bytes.len())?;
                out.write_all(bytes).map_err(io)?;
            }
            out.write_all(&night.light_color).map_err(io)?;
        }
        for mesh in [&self.geometry.floor, &self.geometry.wall] {
            int(out, mesh.vertices.len())?;
            for vertex in &mesh.vertices {
                let p = vertex.position;
                let uv = vertex.uv;
                let n = vertex.normal;
                for f in [p.x, p.y, p.z, uv.x, uv.y, n.x, n.y, n.z] {
                    out.write_all(&f.to_le_bytes()).map_err(io)?;
                }
            }
            int(out, mesh.indices.len())?;
            for &index in &mesh.indices {
                int(out, index as usize)?;
            }
        }
        Ok(())
    }
    pub fn texture(&self, role: ImageRole, limits: FsofLimits) -> Result<RgbaImage> {
        self.validate(limits)?;
        let (width, height, bytes) = match role {
            ImageRole::FloorDay => (self.floor_width, self.floor_height, &self.floor_texture),
            ImageRole::WallDay => (self.wall_width, self.wall_height, &self.wall_texture),
            ImageRole::FloorNight => (
                self.floor_width,
                self.floor_height,
                &self.night.as_ref().ok_or_else(invalid)?.floor_texture,
            ),
            ImageRole::WallNight => (
                self.wall_width,
                self.wall_height,
                &self.night.as_ref().ok_or_else(invalid)?.wall_texture,
            ),
            _ => return Err(invalid()),
        };
        let count = usize::try_from(u64::from(width) * u64::from(height)).map_err(|_| limit())?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(count)
            .map_err(|_| DerivativeError::Allocation)?;
        match self.compression {
            TextureCompression::Rgba8 => {
                pixels.extend(bytes.chunks_exact(4).map(|p| [p[0], p[1], p[2], p[3]]))
            }
            TextureCompression::Dxt5 => {
                pixels.resize(count, [0; 4]);
                for (index, block) in bytes.chunks_exact(16).enumerate() {
                    let bx = index as u32 % width.div_ceil(4) * 4;
                    let by = index as u32 / width.div_ceil(4) * 4;
                    let decoded = bc3(block);
                    for y in 0..4 {
                        for x in 0..4 {
                            if bx + x < width && by + y < height {
                                pixels[((by + y) * width + bx + x) as usize] =
                                    decoded[(y * 4 + x) as usize];
                            }
                        }
                    }
                }
            }
        }
        Ok(RgbaImage {
            width,
            height,
            pixels,
        })
    }
}
struct LimitedWriter<W> {
    inner: W,
    left: u64,
}
impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.left {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "FSOf file byte limit",
            ));
        }
        let n = self.inner.write(bytes)?;
        self.left -= n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
impl FacadeGeometry {
    pub fn validate(&self, limits: FsofLimits) -> Result<()> {
        let vertices = self
            .floor
            .vertices
            .len()
            .checked_add(self.wall.vertices.len())
            .ok_or_else(limit)?;
        let indices = self
            .floor
            .indices
            .len()
            .checked_add(self.wall.indices.len())
            .ok_or_else(limit)?;
        if vertices > limits.max_vertices || indices > limits.max_indices {
            return Err(limit());
        }
        for mesh in [&self.floor, &self.wall] {
            if mesh.vertices.len() > i32::MAX as usize
                || mesh.indices.len() > i32::MAX as usize
                || mesh.indices.len() % 3 != 0
                || mesh
                    .indices
                    .iter()
                    .any(|&i| i as usize >= mesh.vertices.len())
                || mesh
                    .vertices
                    .iter()
                    .any(|v| !v.position.is_finite() || !v.uv.is_finite())
            {
                return Err(invalid());
            }
        }
        if self.resident_bytes() > limits.max_decoded_bytes {
            return Err(limit());
        }
        Ok(())
    }
    pub fn resident_bytes(&self) -> u64 {
        std::mem::size_of::<Self>() as u64
            + [&self.floor, &self.wall]
                .iter()
                .map(|m| m.vertices.capacity() as u64 * 32 + m.indices.capacity() as u64 * 4)
                .sum::<u64>()
    }
}
impl FsofMesh {
    pub fn to_mesh(&self, limits: RenderLimits) -> Result<Mesh> {
        if self.vertices.len() > limits.max_vertices || self.indices.len() > limits.max_indices {
            return Err(limit());
        }
        let mut mesh = Mesh {
            vertices: self
                .vertices
                .iter()
                .map(|v| Vertex {
                    position: v.position,
                    normal: if v.normal.is_finite() {
                        v.normal
                    } else {
                        Vec3::ZERO
                    },
                    uv: v.uv,
                    color: [1.; 4],
                })
                .collect(),
            indices: self.indices.clone(),
        };
        mesh.validate(&limits)?;
        let mut normals = vec![Vec3::ZERO; mesh.vertices.len()];
        for tri in mesh.indices.chunks_exact(3) {
            let a = mesh.vertices[tri[0] as usize].position;
            let b = mesh.vertices[tri[1] as usize].position;
            let c = mesh.vertices[tri[2] as usize].position;
            let n = (b - a).cross(c - b);
            if n.is_finite() {
                for &i in tri {
                    normals[i as usize] = normals[i as usize] + n;
                }
            }
        }
        for (vertex, normal) in mesh.vertices.iter_mut().zip(normals) {
            if vertex.normal == Vec3::ZERO {
                vertex.normal = normal.normalize_or_zero();
            }
        }
        Ok(mesh)
    }
}

fn bc3(block: &[u8]) -> [[u8; 4]; 16] {
    let mut alpha = [0u8; 8];
    alpha[0] = block[0];
    alpha[1] = block[1];
    if alpha[0] > alpha[1] {
        for i in 2..8 {
            alpha[i] =
                (((8 - i) as u16 * alpha[0] as u16 + (i - 1) as u16 * alpha[1] as u16) / 7) as u8;
        }
    } else {
        for i in 2..6 {
            alpha[i] =
                (((6 - i) as u16 * alpha[0] as u16 + (i - 1) as u16 * alpha[1] as u16) / 5) as u8;
        }
        alpha[6] = 0;
        alpha[7] = 255;
    }
    let rgb = |lo, hi| {
        let v = u16::from_le_bytes([lo, hi]);
        let r = ((v >> 11) & 31) as u8;
        let g = ((v >> 5) & 63) as u8;
        let b = (v & 31) as u8;
        [
            (r << 3) | (r >> 2),
            (g << 2) | (g >> 4),
            (b << 3) | (b >> 2),
        ]
    };
    let mut colors = [[0u8; 3]; 4];
    colors[0] = rgb(block[8], block[9]);
    colors[1] = rgb(block[10], block[11]);
    for c in [0, 1, 2] {
        colors[2][c] = ((2 * colors[0][c] as u16 + colors[1][c] as u16) / 3) as u8;
        colors[3][c] = ((colors[0][c] as u16 + 2 * colors[1][c] as u16) / 3) as u8;
    }
    let mut a_bits = 0u64;
    for (i, &byte) in block[2..8].iter().enumerate() {
        a_bits |= (byte as u64) << (i * 8);
    }
    let colors_bits = u32::from_le_bytes([block[12], block[13], block[14], block[15]]);
    let mut pixels = [[0u8; 4]; 16];
    for (i, pixel) in pixels.iter_mut().enumerate() {
        let color = colors[((colors_bits >> (i * 2)) & 3) as usize];
        *pixel = [
            color[0],
            color[1],
            color[2],
            alpha[((a_bits >> (i * 3)) & 7) as usize],
        ];
    }
    pixels
}
