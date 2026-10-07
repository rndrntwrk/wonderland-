use crate::*;
use wonderland_render_core::{EntityRef, Mesh};

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedBatch {
    pub material: MaterialKey,
    pub sprite_indices: Vec<usize>,
    pub mesh: Mesh,
}
/// Stable pass/depth order, then adjacent compatible material runs. Resources
/// never regroup nonadjacent transparent draws. A portable u16 quad limit is
/// enforced even though the shared CPU Mesh uses u32 indices.
pub fn make_batches(sprites: &[PreparedSprite], max_quads: usize) -> Result<Vec<PreparedBatch>> {
    make_batches_with_limits(
        sprites,
        max_quads,
        &wonderland_render_core::RenderLimits::default(),
    )
}
pub fn make_batches_with_limits(
    sprites: &[PreparedSprite],
    max_quads: usize,
    limits: &wonderland_render_core::RenderLimits,
) -> Result<Vec<PreparedBatch>> {
    if max_quads == 0 || max_quads > 16383 {
        return Err(IsoError::Limit("portable batch quad count"));
    }
    if sprites.len() > limits.max_vertices / 4 || sprites.len() > limits.max_indices / 6 {
        return Err(IsoError::Limit("aggregate batch geometry"));
    }
    if sprites.iter().any(|s| {
        !s.draw_order.is_finite()
            || s.mesh.vertices.len() != 4
            || s.mesh.indices != [0, 1, 3, 1, 2, 3]
    }) {
        return Err(IsoError::Invalid("batch input"));
    }
    for s in sprites {
        s.mesh
            .validate(limits)
            .map_err(|_| IsoError::Invalid("batch vertices"))?;
    }
    let mut indices: Vec<_> = (0..sprites.len()).collect();
    indices.sort_by(|&a, &b| {
        sprites[a]
            .material
            .mode
            .cmp(&sprites[b].material.mode)
            .then_with(|| sprites[a].draw_order.total_cmp(&sprites[b].draw_order))
    });
    let mut batches: Vec<PreparedBatch> = vec![];
    for index in indices {
        let sprite = &sprites[index];
        let new = batches.last().map_or(true, |b| {
            b.material != sprite.material || b.sprite_indices.len() >= max_quads
        });
        if new {
            batches.push(PreparedBatch {
                material: sprite.material,
                sprite_indices: vec![],
                mesh: Mesh {
                    vertices: vec![],
                    indices: vec![],
                },
            });
        }
        let b = batches.last_mut().expect("created batch");
        let base = b.mesh.vertices.len() as u32;
        b.sprite_indices.push(index);
        b.mesh.vertices.extend_from_slice(&sprite.mesh.vertices);
        b.mesh
            .indices
            .extend(sprite.mesh.indices.iter().map(|i| i + base));
    }
    Ok(batches)
}
/// Software color/depth pass groups are contiguous nonoverlapping runs. A later
/// sprite cannot move ahead of an overlapping earlier sprite through bin reuse.
pub fn make_software_batches(
    sprites: &[PreparedSprite],
    max_quads: usize,
) -> Result<Vec<PreparedBatch>> {
    let batches = make_batches(sprites, max_quads)?;
    let mut out: Vec<PreparedBatch> = vec![];
    for batch in batches {
        let mut run: Option<PreparedBatch> = None;
        for index in batch.sprite_indices {
            if run.as_ref().is_some_and(|b| {
                b.sprite_indices
                    .iter()
                    .any(|i| sprites[*i].rect.intersects(sprites[index].rect))
            }) {
                out.push(run.take().expect("nonempty software run"));
            }
            let b = run.get_or_insert_with(|| PreparedBatch {
                material: batch.material,
                sprite_indices: vec![],
                mesh: Mesh {
                    vertices: vec![],
                    indices: vec![],
                },
            });
            let base = b.mesh.vertices.len() as u32;
            b.sprite_indices.push(index);
            b.mesh
                .vertices
                .extend_from_slice(&sprites[index].mesh.vertices);
            b.mesh
                .indices
                .extend(sprites[index].mesh.indices.iter().map(|i| i + base));
        }
        if let Some(run) = run {
            out.push(run);
        }
    }
    Ok(out)
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynamicResidency {
    pub reference: EntityRef,
    pub changed_at: f64,
    pub force_dynamic: bool,
}
impl DynamicResidency {
    /// Source's five half-second ring is represented by monotonic presentation
    /// time. Exact 2.5s boundary is deliberate and independent of VM ticks.
    pub fn layer(self, now: f64) -> Result<SpriteLayer> {
        if !now.is_finite() || !self.changed_at.is_finite() || now < self.changed_at {
            return Err(IsoError::Invalid("residency clock"));
        }
        Ok(if self.force_dynamic || now - self.changed_at < 2.5 {
            SpriteLayer::Dynamic
        } else {
            SpriteLayer::Static
        })
    }
}
