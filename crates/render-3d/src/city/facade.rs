//! Deterministic mesh facades; no original assets or external renderer required.
use crate::{
    lot::{BuildOptions, VisualLot},
    Error,
};
use sha2::{Digest, Sha256};
use std::fmt::Write;
use wonderland_render_core::derivatives::{
    fsof::{Fsof, FsofLimits},
    ImageRole,
};
use wonderland_render_core::{Aabb, AssetKey, Mat4, Mesh, RgbaImage, Vec3};

pub struct SourceFacadeNight {
    pub floor: RgbaImage,
    pub wall: RgbaImage,
    pub light_color: [u8; 4],
}
/// Original facade geometry/material split, prepared in city coordinates.
pub struct TexturedFacade {
    pub floor: Mesh,
    pub wall: Mesh,
    pub floor_day: RgbaImage,
    pub wall_day: RgbaImage,
    pub night: Option<SourceFacadeNight>,
    pub bounds: Aabb,
    pub identity: AssetKey,
}
pub fn load_source_facade(
    bytes: &[u8],
    city: (u16, u16),
    corner_elevation: [u8; 4],
    y_squish: f32,
    limits: FsofLimits,
) -> Result<TexturedFacade, Error> {
    let transform = super::facade_transform(city, corner_elevation, y_squish)?;
    let normals = transform
        .inverse()
        .ok_or(Error::InvalidInput("source facade transform"))?
        .transpose();
    let source =
        Fsof::decode(bytes, limits).map_err(|_| Error::InvalidInput("source FSOf container"))?;
    let pixels = (source.floor_width as u64 * source.floor_height as u64
        + source.wall_width as u64 * source.wall_height as u64)
        * if source.night.is_some() { 2 } else { 1 };
    let geometry_vertices =
        source.geometry.floor.vertices.len() + source.geometry.wall.vertices.len();
    let geometry_indices = source.geometry.floor.indices.len() + source.geometry.wall.indices.len();
    if pixels
        .checked_mul(4)
        .and_then(|p| {
            p.checked_add(
                geometry_vertices as u64
                    * std::mem::size_of::<wonderland_render_core::Vertex>() as u64
                    + geometry_indices as u64 * 4,
            )
        })
        .filter(|n| *n <= limits.max_decoded_bytes)
        .is_none()
    {
        return Err(Error::InvalidInput("source facade resident budget"));
    }
    let render_limits = wonderland_render_core::RenderLimits {
        max_vertices: limits.max_vertices,
        max_indices: limits.max_indices,
        ..Default::default()
    };
    let mut floor = source
        .geometry
        .floor
        .to_mesh(render_limits)
        .map_err(|_| Error::InvalidInput("source facade floor"))?;
    let mut wall = source
        .geometry
        .wall
        .to_mesh(render_limits)
        .map_err(|_| Error::InvalidInput("source facade wall"))?;
    let mut low = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut high = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for mesh in [&mut floor, &mut wall] {
        for vertex in &mut mesh.vertices {
            // FSOf stores tile units already: apply the source city transform once.
            vertex.position = transform.transform_point3(vertex.position);
            vertex.normal = normals.transform_vector3(vertex.normal).normalize_or_zero();
            low.x = low.x.min(vertex.position.x);
            low.y = low.y.min(vertex.position.y);
            low.z = low.z.min(vertex.position.z);
            high.x = high.x.max(vertex.position.x);
            high.y = high.y.max(vertex.position.y);
            high.z = high.z.max(vertex.position.z);
        }
        mesh.validate(&render_limits)
            .map_err(|_| Error::InvalidInput("source facade transformed mesh"))?;
    }
    let bounds = Aabb::new(low, high).ok_or(Error::InvalidInput("empty source facade"))?;
    let image = |role| {
        source
            .texture(role, limits)
            .map_err(|_| Error::InvalidInput("source facade texture"))
    };
    let floor_day = image(ImageRole::FloorDay)?;
    let wall_day = image(ImageRole::WallDay)?;
    let night = source
        .night
        .as_ref()
        .map(|night| {
            Ok::<_, Error>(SourceFacadeNight {
                floor: image(ImageRole::FloorNight)?,
                wall: image(ImageRole::WallNight)?,
                light_color: night.light_color,
            })
        })
        .transpose()?;
    let mut hash = Sha256::new();
    hash.update(b"original-fsof-city-v1\0");
    hash.update(bytes);
    for column in transform.cols {
        for value in column {
            hash.update(value.to_bits().to_le_bytes());
        }
    }
    Ok(TexturedFacade {
        floor,
        wall,
        floor_day,
        wall_day,
        night,
        bounds,
        identity: AssetKey(hash.finalize().into()),
    })
}
pub struct Facade {
    pub mesh: Mesh,
    pub bounds: Aabb,
    pub identity: AssetKey,
    pub missing_assets: Vec<&'static str>,
}
pub fn bake_facade(
    lot: &VisualLot,
    options: &BuildOptions,
    content: AssetKey,
    revision: u64,
    y_squish: f32,
) -> Result<Facade, Error> {
    // build_lot returns graphics units. The legacy facade helper takes tile units.
    // Compose this conversion before the source rotation, squish, and city scale.
    let transform = super::facade_transform((0, 0), [0; 4], y_squish)?
        * Mat4::from_scale(Vec3::ONE / crate::lot::TILE_UNITS);
    let normal_transform = transform
        .inverse()
        .ok_or(Error::InvalidInput("facade scale"))?
        .transpose();
    let output = crate::lot::build_lot(lot, options)?;
    let mut mesh = Mesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    let mut h = Sha256::new();
    h.update(b"wonderland-facade-v2\0");
    h.update(content.0);
    h.update(revision.to_le_bytes());
    h.update(y_squish.to_bits().to_le_bytes());
    h.update((output.parts.len() as u64).to_le_bytes());
    for mut part in output.parts {
        h.update((part.mesh.vertices.len() as u64).to_le_bytes());
        h.update((part.mesh.indices.len() as u64).to_le_bytes());
        h.update([part.kind as u8, part.level]);
        h.update(part.material.to_le_bytes());
        h.update(part.style.to_le_bytes());
        let base = mesh.vertices.len() as u32;
        for v in &mut part.mesh.vertices {
            v.position = transform.transform_point3(v.position);
            v.normal = normal_transform
                .transform_vector3(v.normal)
                .normalize_or_zero();
            for f in [
                v.position.x,
                v.position.y,
                v.position.z,
                v.normal.x,
                v.normal.y,
                v.normal.z,
                v.uv.x,
                v.uv.y,
                v.color[0],
                v.color[1],
                v.color[2],
                v.color[3],
            ] {
                h.update(f.to_bits().to_le_bytes());
            }
        }
        for i in &part.mesh.indices {
            h.update(i.to_le_bytes());
        }
        mesh.indices
            .extend(part.mesh.indices.into_iter().map(|i| i + base));
        mesh.vertices.extend(part.mesh.vertices);
    }
    mesh.validate(&wonderland_render_core::RenderLimits::default())
        .map_err(|_| Error::InvalidInput("facade mesh"))?;
    let bounds = Aabb::from_points(&mesh.vertices.iter().map(|v| v.position).collect::<Vec<_>>())
        .ok_or(Error::InvalidInput("empty facade"))?;
    Ok(Facade {
        mesh,
        bounds,
        identity: AssetKey(h.finalize().into()),
        missing_assets: output.missing_assets,
    })
}
/// Export original synthetic/authorized data, reorienting OBJ faces to their
/// explicit normals because legacy terrain and roof draw winding differ.
pub fn to_obj(facade: &Facade) -> Result<String, Error> {
    facade
        .mesh
        .validate(&wonderland_render_core::RenderLimits::default())
        .map_err(|_| Error::InvalidInput("facade OBJ mesh"))?;
    let mut out=String::from("# Wonderland deterministic presentation facade v1\n# Geometry source winding normalized to explicit normals for OBJ export.\n");
    let _ = write!(out, "# identity ");
    for b in facade.identity.0 {
        let _ = write!(out, "{b:02x}");
    }
    out.push('\n');
    for v in &facade.mesh.vertices {
        let _ = writeln!(
            out,
            "v {:.9} {:.9} {:.9}",
            v.position.x, v.position.y, v.position.z
        );
    }
    for v in &facade.mesh.vertices {
        let _ = writeln!(out, "vt {:.9} {:.9}", v.uv.x, v.uv.y);
    }
    for v in &facade.mesh.vertices {
        let _ = writeln!(
            out,
            "vn {:.9} {:.9} {:.9}",
            v.normal.x, v.normal.y, v.normal.z
        );
    }
    for triangle in facade.mesh.indices.chunks_exact(3) {
        let mut tri = [triangle[0], triangle[1], triangle[2]];
        let (a, b, c) = (
            &facade.mesh.vertices[tri[0] as usize],
            &facade.mesh.vertices[tri[1] as usize],
            &facade.mesh.vertices[tri[2] as usize],
        );
        if (b.position - a.position)
            .cross(c.position - a.position)
            .dot(a.normal + b.normal + c.normal)
            < 0.
        {
            tri.swap(1, 2);
        }
        let [a, b, c] = tri.map(|i| i + 1);
        let _ = writeln!(out, "f {a}/{a}/{a} {b}/{b}/{b} {c}/{c}/{c}");
    }
    Ok(out)
}
