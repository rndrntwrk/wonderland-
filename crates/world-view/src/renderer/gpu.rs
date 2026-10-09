//! Bounded, immutable draw packets. GPU IDs are frame-local indexes, not authority.
use super::*;
use serde::Serialize;
use wonderland_render_core::Mesh;

const MAX_UPLOAD_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
pub struct WorldGpuMesh {
    /// Interleaved position.xyz, UV.xy, encoded source color.rgba.
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
}
#[derive(Clone, Debug, Serialize)]
pub struct WorldGpuLight {
    pub texture: usize,
    pub matrix: [f32; 16],
}
#[derive(Clone, Debug, Serialize)]
pub struct WorldGpuDraw {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light: Option<WorldGpuLight>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture_address: Option<&'static str>,
    pub pipeline: Option<wonderland_render_core::reference::FragmentPipeline>,
    pub mesh: usize,
    pub texture: Option<usize>,
    pub matrix: [f32; 16],
    pub pick_id: u32,
    pub depth_equal: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct WorldGpuFrame {
    pub schema: u32,
    pub width: u32,
    pub height: u32,
    /// Decimal avoids JavaScript's lossy integer conversion above 2^53.
    pub generation: String,
    pub meshes: Vec<WorldGpuMesh>,
    pub textures: Vec<RgbaImage>,
    pub draws: Vec<WorldGpuDraw>,
}
pub(super) struct DisplayedGpu {
    pub hits: Vec<WorldPickTarget>,
    pub revision: WorldRevision,
    pub generation: u64,
    pub width: u32,
    pub height: u32,
}
fn reserve(bytes: &mut usize, amount: usize) -> Result<(), WorldError> {
    *bytes = bytes
        .checked_add(amount)
        .filter(|total| *total <= MAX_UPLOAD_BYTES)
        .ok_or_else(|| WorldError("source GPU upload budget exceeded".into()))?;
    Ok(())
}
impl WorldRenderer {
    pub fn prepare_gpu(
        &mut self,
        controls: ViewportControls,
        width: u32,
        height: u32,
    ) -> Result<(WorldGpuFrame, WorldRenderStats), WorldError> {
        crate::scene::validate_controls(&self.document, controls)?;
        if width == 0
            || height == 0
            || width > 4096
            || height > 4096
            || u64::from(width) * u64::from(height) > 1_048_576
        {
            return Err(WorldError("source GPU surface budget exceeded".into()));
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
        let scene = &self.prepared.as_ref().expect("prepared source scene").1;
        let projection = camera_projection(&self.document, controls, width as f32 / height as f32)?;
        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| WorldError("world render generation exhausted".into()))?;
        let addressed = scene
            .parts
            .iter()
            .any(|p| self.texture_address_for_part(p) == TextureAddress::Wrap);
        let mut packet = WorldGpuFrame {
            schema: if addressed {
                4
            } else if self.lighting.is_some() {
                3
            } else {
                2
            },
            width,
            height,
            generation: generation.to_string(),
            meshes: vec![],
            textures: vec![],
            draws: vec![],
        };
        // Pointer keys are disposable deduplication only, never serialized IDs.
        let mut meshes = BTreeMap::<*const Mesh, usize>::new();
        let mut textures = BTreeMap::<*const RgbaImage, usize>::new();
        let mut bytes = width as usize * height as usize * 8;
        let light_texture = if let Some(light) = &self.lighting {
            let image = light.image();
            image
                .validate(&RenderLimits::default())
                .map_err(|e| WorldError(e.to_string()))?;
            reserve(&mut bytes, image.pixels.len() * 4)?;
            packet.textures.push(image.clone());
            Some(0)
        } else {
            None
        };

        let mut hits = vec![];
        let mut object_hits = BTreeMap::<usize, u32>::new();
        let mut triangles = 0;
        for part in &scene.parts {
            let matrix = projection * part.transform;
            if part.pipeline.and_then(|p| p.forced_depth).is_none()
                && outside_frustum(&part.mesh, matrix)
            {
                continue;
            }
            if !matrix.cols.iter().flatten().all(|value| value.is_finite()) {
                return Err(WorldError("source GPU matrix is not finite".into()));
            }
            if packet.draws.len() >= 262_144 {
                return Err(WorldError("source GPU draw budget exceeded".into()));
            }
            reserve(&mut bytes, 192 + usize::from(addressed) * 8)?;
            let mesh = match meshes.get(&Arc::as_ptr(&part.mesh)) {
                Some(index) => *index,
                None => {
                    reserve(
                        &mut bytes,
                        part.mesh.vertices.len() * 36 + part.mesh.indices.len() * 4,
                    )?;
                    let index = packet.meshes.len();
                    let mut vertices = Vec::with_capacity(part.mesh.vertices.len() * 9);
                    for vertex in &part.mesh.vertices {
                        vertices.extend_from_slice(&[
                            vertex.position.x,
                            vertex.position.y,
                            vertex.position.z,
                            vertex.uv.x,
                            vertex.uv.y,
                            vertex.color[0],
                            vertex.color[1],
                            vertex.color[2],
                            vertex.color[3],
                        ]);
                    }
                    packet.meshes.push(WorldGpuMesh {
                        vertices,
                        indices: part.mesh.indices.clone(),
                    });
                    meshes.insert(Arc::as_ptr(&part.mesh), index);
                    index
                }
            };
            let texture = if let Some(image) = &part.texture {
                Some(match textures.get(&Arc::as_ptr(image)) {
                    Some(index) => *index,
                    None => {
                        if image.width > 4096
                            || image.height > 4096
                            || packet.textures.len() >= 65_535
                        {
                            return Err(WorldError("source GPU texture budget exceeded".into()));
                        }
                        reserve(&mut bytes, image.pixels.len() * 4)?;
                        let index = packet.textures.len();
                        packet.textures.push(image.as_ref().clone());
                        textures.insert(Arc::as_ptr(image), index);
                        index
                    }
                })
            } else {
                None
            };
            let target = if part.pipeline.is_some_and(|p| {
                p.blend == wonderland_render_core::reference::FragmentBlend::NoColor
            }) {
                None
            } else {
                pick_target(&self.document, part, controls)
            };
            let pick_id = match target {
                Some(target) => {
                    if let Some(object) = part.object {
                        *object_hits.entry(object).or_insert_with(|| {
                            hits.push(target);
                            hits.len() as u32
                        })
                    } else {
                        hits.push(target);
                        hits.len() as u32
                    }
                }
                None => 0,
            };
            let light = if let Some((_, uv)) = self.light_for_part(part)? {
                for v in &part.mesh.vertices {
                    if !uv
                        .transform_vec4([v.position.x, v.position.y, v.position.z, 1.])
                        .iter()
                        .all(|n| n.is_finite())
                    {
                        return Err(WorldError("source light UV is not finite".into()));
                    }
                }
                reserve(&mut bytes, 80)?;
                Some(WorldGpuLight {
                    texture: light_texture.expect("prepared light texture"),
                    matrix: std::array::from_fn(|i| uv.cols[i / 4][i % 4]),
                })
            } else {
                None
            };
            packet.draws.push(WorldGpuDraw {
                texture_address: addressed.then(|| match self.texture_address_for_part(part) {
                    TextureAddress::Clamp => "clamp",
                    TextureAddress::Wrap => "wrap",
                }),
                light,
                pipeline: part.pipeline,
                mesh,
                texture,
                matrix: std::array::from_fn(|i| matrix.cols[i / 4][i % 4]),
                pick_id,
                depth_equal: part.pipeline.map_or(part.object.is_some(), |p| {
                    p.depth_compare == DepthComparison::LessEqual
                }),
            });
            triangles += part.mesh.indices.len() / 3;
        }
        let stats = WorldRenderStats {
            width,
            height,
            parts: packet.draws.len(),
            triangles,
            diagnostics: scene.diagnostics.clone(),
        };
        self.generation = generation;
        self.raster = None;
        self.gpu = Some(DisplayedGpu {
            hits,
            revision: self.document.revision,
            generation,
            width,
            height,
        });
        Ok((packet, stats))
    }

    /// The browser must supply the actual offscreen RGB24 readback for this frame.
    /// This resolves a visual selection only; the server still validates actions.
    pub fn resolve_gpu_pick(
        &self,
        generation: u64,
        index: u32,
        x: u32,
        y: u32,
    ) -> Option<WorldPick> {
        let displayed = self.gpu.as_ref()?;
        if generation != self.generation
            || generation != displayed.generation
            || displayed.revision != self.document.revision
            || x >= displayed.width
            || y >= displayed.height
        {
            return None;
        }
        let target = displayed.hits.get(index.checked_sub(1)? as usize)?.clone();
        if let WorldPickTarget::Object {
            entity: Some(entity),
            ..
        } = &target
        {
            let frames = self.frames.as_ref()?;
            frames.resolve_pick(&frames.pick_ticket(*entity)?)?;
        }
        Some(WorldPick {
            revision: displayed.revision,
            frame_generation: generation,
            target,
            screen: [x, y],
        })
    }
}
