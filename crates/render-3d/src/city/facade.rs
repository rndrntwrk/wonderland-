//! Deterministic mesh facades; no original assets or external renderer required.
use crate::{
    lot::{BuildOptions, VisualLot},
    Error,
};
use sha2::{Digest, Sha256};
use std::fmt::Write;
use wonderland_render_core::{Aabb, AssetKey, Mesh};
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
    let transform = super::facade_transform((0, 0), [0; 4], y_squish)?;
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
    h.update(b"wonderland-facade-v1\0");
    h.update(content.0);
    h.update(revision.to_le_bytes());
    h.update(y_squish.to_bits().to_le_bytes());
    for mut part in output.parts {
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
pub fn to_obj(facade: &Facade) -> String {
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
    out
}
