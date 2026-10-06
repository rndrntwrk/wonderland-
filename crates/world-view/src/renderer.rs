use crate::*;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};
use wonderland_render_core::{
    AssetKey, EntityProjection, EntityRef, FrameStamp, Mat4, Quat, RenderFrame, RenderLimits,
    RgbaImage, Transform, Vec3,
    frame::FrameStore,
    reference::{DepthComparison, FragmentOptions, ReferenceSurface, TextureAddress},
};

#[derive(Clone, Debug, Default)]
pub struct WorldRenderStats {
    pub width: u32,
    pub height: u32,
    pub parts: usize,
    pub triangles: usize,
    pub diagnostics: Vec<WorldDiagnostic>,
}

struct DisplayedRaster {
    color: ReferenceSurface,
    /// Private index IDs never enter FrameStore or any live operation.
    hit_ids: ReferenceSurface,
    hits: Vec<WorldPickTarget>,
    revision: WorldRevision,
    generation: u64,
}
pub struct WorldRenderer {
    document: Arc<WorldDocument>,
    frames: Option<FrameStore>,
    prepared: Option<(ViewportControls, PreparedWorld)>,
    raster: Option<DisplayedRaster>,
    generation: u64,
}
impl WorldRenderer {
    pub fn new(document: Arc<WorldDocument>) -> Result<Self, WorldError> {
        document.validate()?;
        let frames = if let Some(lot_id) = document.revision.lot_id {
            let mut frames = FrameStore::new(RenderLimits::default());
            frames.reset(lot_id, document.revision.epoch);
            frames
                .admit(render_frame(&document)?)
                .map_err(|error| WorldError(error.to_string()))?;
            Some(frames)
        } else {
            if document
                .objects
                .iter()
                .any(|object| object.entity.is_some())
            {
                return Err(WorldError(
                    "live entity references require an admitted lot identity".into(),
                ));
            }
            None
        };
        Ok(Self {
            document,
            frames,
            prepared: None,
            raster: None,
            generation: 0,
        })
    }
    pub fn replace_document(&mut self, document: Arc<WorldDocument>) -> Result<(), WorldError> {
        document.validate()?;
        if Arc::ptr_eq(&self.document, &document) || *self.document == *document {
            return Ok(());
        }
        let same_boundary = (document.revision.lot_id, document.revision.epoch)
            == (self.document.revision.lot_id, self.document.revision.epoch)
            && document.provenance.origin == self.document.provenance.origin;
        if same_boundary
            && document.provenance.kind == WorldSourceKind::LegacySnapshot
            && self.document.provenance.kind == WorldSourceKind::LegacySnapshot
            && document.revision.architecture_revision
                <= self.document.revision.architecture_revision
            && !snapshot_resource_update(&self.document, &document)
        {
            return Err(WorldError(
                "snapshot presentation generation must increase on refresh".into(),
            ));
        }
        if same_boundary && document.revision.lot_id.is_some() {
            if document.revision.architecture_revision
                == self.document.revision.architecture_revision
                && document.lot != self.document.lot
            {
                return Err(WorldError(
                    "live architecture changed without a new architecture revision".into(),
                ));
            }
            let previous_entities: BTreeMap<_, _> = self
                .document
                .objects
                .iter()
                .filter_map(|object| object.entity.map(|entity| (entity, object)))
                .collect();
            for object in &document.objects {
                if let Some(previous) = object
                    .entity
                    .and_then(|entity| previous_entities.get(&entity))
                    && object.visual_revision == previous.visual_revision
                    && (object.dynamic_flags != previous.dynamic_flags
                        || object.source_guid != previous.source_guid
                        || object.model != previous.model
                        || object.room != previous.room)
                {
                    return Err(WorldError(
                        "live object presentation changed without a new visual revision".into(),
                    ));
                }
            }
            self.frames
                .as_mut()
                .ok_or_else(|| WorldError("live frame store unavailable".into()))?
                .admit(render_frame(&document)?)
                .map_err(|error| WorldError(error.to_string()))?;
        } else {
            let replacement = Self::new(Arc::clone(&document))?;
            self.frames = replacement.frames;
        }
        self.document = document;
        self.prepared = None;
        self.raster = None;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| WorldError("world render generation exhausted".into()))?;
        Ok(())
    }
    pub fn render(
        &mut self,
        controls: ViewportControls,
        width: u32,
        height: u32,
    ) -> Result<WorldRenderStats, WorldError> {
        crate::scene::validate_controls(&self.document, controls)?;
        if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 1_048_576 {
            return Err(WorldError(
                "software world surface exceeds its rendering budget".into(),
            ));
        }
        let rebuild = self.prepared.as_ref().is_none_or(|(previous, _)| {
            previous.visible_level != controls.visible_level
                || previous.walls != controls.walls
                || previous.show_roofs != controls.show_roofs
                || (controls.walls != WallMode::Up
                    && wonderland_render_3d::camera::cut_rotation(previous.yaw_radians).ok()
                        != wonderland_render_3d::camera::cut_rotation(controls.yaw_radians).ok())
        });
        if rebuild {
            self.prepared = Some((controls, build_scene(&self.document, controls)?));
        }
        let scene = &self.prepared.as_ref().expect("prepared scene").1;
        let projection = camera_projection(&self.document, controls, width as f32 / height as f32)?;
        let limits = RenderLimits::default();
        let mut color = ReferenceSurface::new(width, height, &limits)?;
        let mut hit_ids = ReferenceSurface::new(width, height, &limits)?;
        color.clear([229, 235, 222, 255]);
        hit_ids.clear([0, 0, 0, 0]);
        let mut hits = vec![];
        let mut triangles = 0;
        let mut drawn_parts = 0;
        for part in &scene.parts {
            let matrix = projection * part.transform;
            if outside_frustum(&part.mesh, matrix) {
                continue;
            }
            let target = if let Some(index) = part.object {
                let object = &self.document.objects[index];
                object.selectable.then_some(WorldPickTarget::Object {
                    entity: object.entity,
                    source_guid: object.source_guid,
                    source_record: object
                        .blueprint
                        .map(|source| source.record)
                        .or_else(|| object.snapshot.map(|source| source.record)),
                })
            } else if let (Some((x, y, level)), Some(surface)) = (part.tile, part.surface) {
                (level == controls.visible_level
                    && matches!(
                        surface,
                        WorldSurface::Terrain
                            | WorldSurface::Floor
                            | WorldSurface::Water
                            | WorldSurface::Pool
                            | WorldSurface::BuildSupport
                    ))
                .then_some(WorldPickTarget::Tile {
                    x,
                    y,
                    level,
                    surface,
                })
            } else {
                None
            };
            let hit = target.map(|target| {
                hits.push(target);
                EntityRef {
                    object_id: hits.len() as u32,
                    generation: 1,
                }
            });
            let entity = part
                .object
                .and_then(|index| self.document.objects[index].entity);
            let depth = if part.object.is_some() {
                DepthComparison::LessEqual
            } else {
                DepthComparison::Less
            };
            color.set_depth_comparison(depth);
            hit_ids.set_depth_comparison(depth);
            let options = FragmentOptions {
                alpha_cutoff: 2,
                // Source Vitaboy.fx wraps both UV axes. Address the interpolated
                // fragments, preserving original out-of-range mesh coordinates.
                texture_address: if part
                    .object
                    .and_then(|index| self.document.objects[index].model)
                    .is_some_and(|index| {
                        matches!(self.document.models[index].context, ModelContext::Vitaboy)
                    }) {
                    TextureAddress::Wrap
                } else {
                    TextureAddress::Clamp
                },
                ..Default::default()
            };
            if let Some(texture) = &part.texture {
                color.draw_textured_mesh(&part.mesh, matrix, texture, entity, options, &limits)?;
                hit_ids.draw_textured_mesh(&part.mesh, matrix, texture, hit, options, &limits)?;
            } else {
                color.draw_mesh(&part.mesh, matrix, entity, options, &limits)?;
                hit_ids.draw_mesh(&part.mesh, matrix, hit, options, &limits)?;
            }
            triangles += part.mesh.indices.len() / 3;
            drawn_parts += 1;
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| WorldError("world render generation exhausted".into()))?;
        self.generation = generation;
        self.raster = Some(DisplayedRaster {
            color,
            hit_ids,
            hits,
            revision: self.document.revision,
            generation,
        });
        Ok(WorldRenderStats {
            width,
            height,
            parts: drawn_parts,
            triangles,
            diagnostics: scene.diagnostics.clone(),
        })
    }
    pub fn image(&self) -> Option<&RgbaImage> {
        self.raster.as_ref().map(|raster| raster.color.image())
    }
    pub fn pick(&self, x: u32, y: u32) -> Option<WorldPick> {
        let raster = self.raster.as_ref()?;
        if raster.generation != self.generation || raster.revision != self.document.revision {
            return None;
        }
        let hit = raster.hit_ids.id_at(x, y)?;
        let target = raster
            .hits
            .get(hit.object_id.checked_sub(1)? as usize)?
            .clone();
        if let WorldPickTarget::Object {
            entity: Some(entity),
            ..
        } = &target
        {
            if raster.color.id_at(x, y) != Some(*entity) {
                return None;
            }
            let frames = self.frames.as_ref()?;
            let ticket = frames.pick_ticket(*entity)?;
            frames.resolve_pick(&ticket)?;
        }
        Some(WorldPick {
            revision: raster.revision,
            frame_generation: raster.generation,
            target,
            screen: [x, y],
        })
    }
    pub fn resolve_pick(&self, pick: &WorldPick) -> Option<WorldPick> {
        if pick.revision != self.document.revision || pick.frame_generation != self.generation {
            return None;
        }
        self.pick(pick.screen[0], pick.screen[1])
            .filter(|current| current == pick)
    }
    pub fn device_reset(&mut self) {
        if let Some(frames) = &mut self.frames {
            frames.device_reset();
        }
        self.raster = None;
        self.prepared = None;
        self.generation = self.generation.saturating_add(1);
    }
}

// Loading source bytes can finish after the snapshot itself. That changes the
// display's content fence, never its accepted VM state or source generations.
fn snapshot_resource_update(previous: &WorldDocument, next: &WorldDocument) -> bool {
    previous.provenance == next.provenance
        && previous.revision.lot_id == next.revision.lot_id
        && previous.revision.epoch == next.revision.epoch
        && previous.revision.tick == next.revision.tick
        && previous.revision.architecture_revision == next.revision.architecture_revision
        && previous.lot == next.lot
        && previous.source_counts == next.source_counts
        && previous.category == next.category
        && previous.sounds == next.sounds
        && previous.objects.len() == next.objects.len()
        && previous
            .objects
            .iter()
            .zip(&next.objects)
            .all(|(before, after)| {
                let mut source = after.clone();
                source.model = before.model;
                *before == source
            })
        && (previous.revision.content != next.revision.content
            || (previous.models == next.models
                && previous.materials == next.materials
                && previous.objects == next.objects))
}

fn render_frame(document: &WorldDocument) -> Result<RenderFrame, WorldError> {
    let entities = document
        .objects
        .iter()
        .filter_map(|object| object.entity.map(|reference| (object, reference)))
        .map(|(object, reference)| {
            let asset = object
                .model
                .map(|index| document.models[index].effective_source)
                .unwrap_or_else(|| {
                    AssetKey(Sha256::digest(object.source_guid.to_le_bytes()).into())
                });
            Ok(EntityProjection {
                reference,
                visual_revision: object.visual_revision,
                transform: Transform {
                    translation: wonderland_render_core::units::tile_to_graphics(
                        object.position_tiles,
                    ) + Vec3::new(1.5, 0.1, 1.5),
                    rotation: Quat::from_axis_angle(Vec3::Y, -object.yaw_radians)
                        .ok_or_else(|| WorldError("invalid source object yaw".into()))?,
                    scale: Vec3::new(3., 3., 3.),
                },
                previous_transform: None,
                asset,
                level: i16::from(object.level),
                visible: object.visible,
                selectable: object.selectable,
            })
        })
        .collect::<Result<Vec<_>, WorldError>>()?;
    Ok(RenderFrame {
        stamp: FrameStamp {
            lot_id: document
                .revision
                .lot_id
                .ok_or_else(|| WorldError("missing live lot identity".into()))?,
            epoch: document.revision.epoch,
            tick: document.revision.tick,
            architecture_revision: document.revision.architecture_revision,
            content: document.revision.content,
        },
        entities,
        selected: None,
    })
}

fn outside_frustum(mesh: &wonderland_render_core::Mesh, matrix: Mat4) -> bool {
    let mut outside = [true; 6];
    for vertex in &mesh.vertices {
        let [x, y, z, w] =
            matrix.transform_vec4([vertex.position.x, vertex.position.y, vertex.position.z, 1.]);
        for (index, value) in [x < -w, x > w, y < -w, y > w, z < 0., z > w]
            .into_iter()
            .enumerate()
        {
            outside[index] &= value;
        }
    }
    outside.into_iter().any(|value| value)
}
