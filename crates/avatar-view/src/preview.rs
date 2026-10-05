//! Bounded offscreen CPU reference previews. Engine GPU targets/callbacks belong
//! to the engine adapter; this pool owns scenes, image/depth/ID buffers only.
use crate::*;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};
use wonderland_render_core::{
    math::*,
    reference::{FragmentOptions, ReferenceSurface},
    AssetKey, RenderLimits, RgbaImage,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PreviewId(pub u64);
#[derive(Clone, Copy, Debug, Default)]
pub struct PreviewStats {
    pub targets: usize,
    pub cpu_bytes: usize,
    pub scenes: usize,
}
struct Scene {
    appearance: Arc<AppearanceBundle>,
    rig: Rig,
    pose: Pose,
    width: u32,
    height: u32,
    key: AssetKey,
    surface: Option<ReferenceSurface>,
}
pub struct PreviewPool {
    scenes: BTreeMap<PreviewId, Scene>,
    max: usize,
    max_bytes: usize,
    next: u64,
    generation: u64,
}
fn surface_bytes(width: u32, height: u32) -> Result<usize> {
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return Err(AvatarError::Limit("preview dimensions"));
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| {
            n.checked_mul(8 + std::mem::size_of::<Option<wonderland_render_core::EntityRef>>())
        })
        .ok_or(AvatarError::Limit("preview bytes"))
}
fn render_scene(scene: &mut Scene, generation: u64) -> Result<()> {
    scene.appearance.validate()?;
    if scene.appearance.rig_key != scene.rig.key() {
        return Err(AvatarError::Invalid("preview rig"));
    }
    scene.pose.rebuild(&scene.rig)?;
    let mut meshes = Vec::new();
    let mut points = Vec::new();
    for part in &scene.appearance.parts {
        let mesh = part.mesh.skin(&scene.pose, Mat4::IDENTITY)?;
        points.extend(mesh.vertices.iter().map(|v| v.position));
        meshes.push(mesh);
    }
    let bounds = Aabb::from_points(&points).ok_or(AvatarError::Invalid("empty preview"))?;
    let center = bounds.min * 0.5 + bounds.max * 0.5;
    let radius = ((bounds.max - bounds.min).length() * 0.5).max(0.1);
    let eye = center + Vec3::new(radius * 0.7, radius * 0.25, radius * 3.0);
    let view =
        Mat4::look_at_rh(eye, center, Vec3::Y).ok_or(AvatarError::Invalid("preview camera"))?;
    let aspect = scene.width as f32 / scene.height as f32;
    let half_y = radius * 1.12 * if aspect < 1.0 { 1.0 / aspect } else { 1.0 };
    let half_x = half_y * aspect;
    let projection =
        Mat4::orthographic_rh(-half_x, half_x, -half_y, half_y, 0.01, radius * 8.0 + 1.0)
            .ok_or(AvatarError::Invalid("preview projection"))?;
    let limits = RenderLimits::default();
    let mut surface = ReferenceSurface::new(scene.width, scene.height, &limits)
        .map_err(|_| AvatarError::Limit("preview allocation"))?;
    let mut hash = Sha256::new();
    hash.update(b"C-avatar-reference-preview-v1");
    hash.update(scene.rig.key().0);
    hash.update(generation.to_le_bytes());
    hash.update(scene.width.to_le_bytes());
    hash.update(scene.height.to_le_bytes());
    hash.update([scene.appearance.skin as u8]);
    for matrix in &scene.pose.palette {
        for v in matrix.cols.iter().flatten() {
            hash.update(v.to_bits().to_le_bytes());
        }
    }
    for (part, mesh) in scene.appearance.parts.iter().zip(&mut meshes) {
        hash.update(part.mesh.cache_key.0);
        hash.update(part.texture.0);
        hash.update([part.role as u8]);
        hash.update(part.appearance.packed().to_le_bytes());
        // Diagnostic material colors; decoded licensed texture pixels are a B/engine gate.
        let color = match part.role {
            PartRole::Head => [0.90, 0.64, 0.42, 1.0],
            _ => [0.27, 0.60, 0.86, 1.0],
        };
        for vertex in &mut mesh.vertices {
            vertex.color = color;
        }
        surface
            .draw_mesh(
                mesh,
                projection * view,
                None,
                FragmentOptions::default(),
                &limits,
            )
            .map_err(|_| AvatarError::Invalid("preview raster"))?;
    }
    scene.key = AssetKey(hash.finalize().into());
    scene.surface = Some(surface);
    Ok(())
}
impl PreviewPool {
    pub fn new(max: usize, max_bytes: usize) -> Self {
        Self {
            scenes: BTreeMap::new(),
            max,
            max_bytes,
            next: 0,
            generation: 0,
        }
    }
    pub fn open(
        &mut self,
        appearance: Arc<AppearanceBundle>,
        rig: &Rig,
        pose: &Pose,
        width: u32,
        height: u32,
    ) -> Result<PreviewId> {
        if self.scenes.len() >= self.max {
            return Err(AvatarError::Limit("preview count"));
        }
        let bytes = surface_bytes(width, height)?;
        if self
            .reserved_bytes()?
            .checked_add(bytes)
            .ok_or(AvatarError::Limit("preview bytes"))?
            > self.max_bytes
        {
            return Err(AvatarError::Limit("preview bytes"));
        }
        let next = self
            .next
            .checked_add(1)
            .ok_or(AvatarError::Limit("preview IDs"))?;
        let id = PreviewId(next);
        let mut scene = Scene {
            appearance,
            rig: rig.clone(),
            pose: pose.clone(),
            width,
            height,
            key: AssetKey([0; 32]),
            surface: None,
        };
        render_scene(&mut scene, self.generation)?;
        self.scenes.insert(id, scene);
        self.next = next;
        Ok(id)
    }
    fn reserved_bytes(&self) -> Result<usize> {
        self.scenes.values().try_fold(0usize, |n, s| {
            n.checked_add(surface_bytes(s.width, s.height)?)
                .ok_or(AvatarError::Limit("preview total"))
        })
    }
    pub fn close(&mut self, id: PreviewId) -> Result<()> {
        self.scenes
            .remove(&id)
            .map(|_| ())
            .ok_or(AvatarError::Stale)
    }
    pub fn key(&self, id: PreviewId) -> Result<AssetKey> {
        let scene = self.scenes.get(&id).ok_or(AvatarError::Stale)?;
        if scene.surface.is_none() {
            return Err(AvatarError::Stale);
        }
        Ok(scene.key)
    }
    pub fn image(&self, id: PreviewId) -> Result<&RgbaImage> {
        self.scenes
            .get(&id)
            .and_then(|s| s.surface.as_ref())
            .map(|s| s.image())
            .ok_or(AvatarError::Stale)
    }
    pub fn device_reset(&mut self, generation: u64) -> Result<()> {
        if generation < self.generation {
            return Err(AvatarError::Stale);
        }
        self.generation = generation;
        for s in self.scenes.values_mut() {
            s.surface = None;
        }
        Ok(())
    }
    pub fn render(&mut self, id: PreviewId) -> Result<()> {
        let scene = self.scenes.get_mut(&id).ok_or(AvatarError::Stale)?;
        render_scene(scene, self.generation)
    }
    pub fn update_pose(&mut self, id: PreviewId, pose: &Pose) -> Result<()> {
        let scene = self.scenes.get_mut(&id).ok_or(AvatarError::Stale)?;
        let mut staged = Scene {
            appearance: scene.appearance.clone(),
            rig: scene.rig.clone(),
            pose: pose.clone(),
            width: scene.width,
            height: scene.height,
            key: scene.key,
            surface: None,
        };
        render_scene(&mut staged, self.generation)?;
        *scene = staged;
        Ok(())
    }
    pub fn stats(&self) -> PreviewStats {
        let targets = self.scenes.values().filter(|s| s.surface.is_some()).count();
        let cpu_bytes = self
            .scenes
            .values()
            .filter(|s| s.surface.is_some())
            .map(|s| surface_bytes(s.width, s.height).expect("validated preview size"))
            .sum();
        PreviewStats {
            targets,
            cpu_bytes,
            scenes: self.scenes.len(),
        }
    }
    pub fn reset(&mut self) {
        self.scenes.clear();
    }
}
