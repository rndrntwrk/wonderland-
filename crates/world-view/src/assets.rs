use crate::WorldError;
use std::collections::BTreeMap;
use wonderland_render_3d::lot::PoolAssets;
use wonderland_render_core::{Mesh, RenderLimits, RgbaImage, Vec2, Vec3, Vertex};

pub fn parse_source_pool_obj(source: &str) -> Result<Mesh, WorldError> {
    let invalid = || WorldError("invalid or oversized authored pool OBJ".into());
    if source.len() > 4 * 1024 * 1024 {
        return Err(invalid());
    }
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut normals = Vec::new();
    let mut output = Mesh {
        vertices: vec![],
        indices: vec![],
    };
    let mut mapped = BTreeMap::new();
    let mut group = String::new();
    let mut selected_group = None;
    for line in source.lines() {
        let mut words = line.split_whitespace();
        let Some(command) = words.next() else {
            continue;
        };
        let mut scalar = || -> Result<f32, WorldError> {
            let value = words
                .next()
                .ok_or_else(invalid)?
                .parse::<f32>()
                .map_err(|_| invalid())?;
            if !value.is_finite() {
                return Err(invalid());
            }
            Ok(value)
        };
        match command {
            "v" => positions.push(Vec3::new(scalar()?, scalar()?, scalar()?)),
            "vt" => uvs.push(Vec2::new(scalar()?, scalar()?)),
            "vn" => normals.push(Vec3::new(scalar()?, scalar()?, scalar()?)),
            "o" => group = words.next().unwrap_or("").into(),
            "f" => {
                if selected_group.is_none() {
                    selected_group = Some(group.clone());
                }
                if selected_group.as_ref() != Some(&group) {
                    continue;
                }
                let corners = words
                    .map(|word| {
                        let indices = word
                            .split('/')
                            .map(|index| index.parse::<usize>().map_err(|_| invalid()))
                            .collect::<Result<Vec<_>, _>>()?;
                        if indices.len() != 3 || indices.contains(&0) {
                            return Err(invalid());
                        }
                        let key = (indices[0], indices[1], indices[2]);
                        if let Some(&mapped) = mapped.get(&key) {
                            return Ok(mapped);
                        }
                        let position = *positions.get(key.0 - 1).ok_or_else(invalid)?
                            + Vec3::new(0.5, 0., 0.5);
                        let uv = *uvs.get(key.1 - 1).ok_or_else(invalid)?;
                        let normal = *normals.get(key.2 - 1).ok_or_else(invalid)?;
                        let index = output.vertices.len() as u32;
                        output.vertices.push(Vertex {
                            position,
                            normal,
                            uv: Vec2::new(uv.x, 1. - uv.y),
                            color: [1.; 4],
                        });
                        mapped.insert(key, index);
                        Ok(index)
                    })
                    .collect::<Result<Vec<_>, WorldError>>()?;
                if corners.len() < 3 || corners.len() > 32 {
                    return Err(invalid());
                }
                for index in 1..corners.len() - 1 {
                    output
                        .indices
                        .extend([corners[0], corners[index], corners[index + 1]]);
                }
            }
            _ => {} // Source OBJ comments, smoothing and the ignored material library.
        }
        if positions.len() > 65_536
            || uvs.len() > 65_536
            || normals.len() > 65_536
            || output.indices.len() > 393_216
        {
            return Err(invalid());
        }
    }
    if output.indices.is_empty() {
        return Err(invalid());
    }
    output
        .validate(&RenderLimits::default())
        .map_err(|_| invalid())?;
    Ok(output)
}
pub fn source_pool_assets() -> Result<(PoolAssets, RgbaImage), WorldError> {
    macro_rules! obj {
        ($file:literal) => {
            include_str!(concat!(
                "../../../apps/web-shell/public/assets/world/pool/",
                $file
            ))
        };
    }
    let tiles = [
        obj!("pool_hq_0.obj"),
        obj!("pool_hq_1.obj"),
        obj!("pool_hq_2.obj"),
        obj!("pool_hq_3.obj"),
        obj!("pool_hq_4.obj"),
        obj!("pool_hq_5.obj"),
        obj!("pool_hq_6.obj"),
        obj!("pool_hq_7.obj"),
        obj!("pool_hq_8.obj"),
        obj!("pool_hq_9.obj"),
        obj!("pool_hq_10.obj"),
        obj!("pool_hq_11.obj"),
        obj!("pool_hq_12.obj"),
        obj!("pool_hq_13.obj"),
        obj!("pool_hq_14.obj"),
        obj!("pool_hq_15.obj"),
    ]
    .into_iter()
    .map(parse_source_pool_obj)
    .collect::<Result<Vec<_>, _>>()?;
    let corners = [
        obj!("poolcorner_hq_0.obj"),
        obj!("poolcorner_hq_1.obj"),
        obj!("poolcorner_hq_2.obj"),
        obj!("poolcorner_hq_3.obj"),
    ]
    .into_iter()
    .map(parse_source_pool_obj)
    .collect::<Result<Vec<_>, _>>()?;
    let pixels = include_bytes!("../../../apps/web-shell/public/assets/world/pool/pool.rgba")
        .chunks_exact(4)
        .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
        .collect();
    Ok((
        PoolAssets { tiles, corners },
        RgbaImage {
            width: 512,
            height: 256,
            pixels,
        },
    ))
}
