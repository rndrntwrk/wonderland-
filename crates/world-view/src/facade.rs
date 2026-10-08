//! C-owned, disposable visual derivatives of a validated loaded world.
//! The FSOf stores the supplied lighting state only, not gameplay or a save.
use crate::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    sync::Arc,
};
use wonderland_render_core::derivatives::{self as d, fsof, source};
use wonderland_render_core::reference::ReferenceSurface;
use wonderland_render_core::{AssetKey, FrameStamp, RenderFrame, RenderLimits};

const MAX_INPUT_BYTES: usize = 32 * 1024 * 1024;
const MAX_OUTPUT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_WORK: u64 = 100_000_000;
const MAX_PART_TRIANGLES: usize = 8192;
const MAX_DRAW_WORK: u64 = 2_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FacadeExportOptions {
    pub pixels_per_tile: u16,
}
impl Default for FacadeExportOptions {
    fn default() -> Self {
        Self { pixels_per_tile: 4 }
    }
}
pub struct WorldFacadeOutput {
    pub bytes: Vec<u8>,
    pub metadata_json: String,
    pub source_hash: String,
}
/// One immutable source snapshot and a cooperatively stepped CPU export.
/// Callers must cancel on a source replacement. It owns no live render tickets.
pub struct WorldFacadeJob {
    state: Option<FacadeState>,
}
struct FacadeState {
    world: Arc<WorldDocument>,
    renderer: WorldRenderer,
    scene: PreparedWorld,
    template: fsof::Fsof,
    regions: Vec<d::AtlasRegion>,
    request: d::FacadeRequest,
    region: usize,
    command: usize,
    surface: Option<ReferenceSurface>,
    hash: String,
    options: FacadeExportOptions,
    work: u64,
}
fn error(s: impl std::fmt::Display) -> WorldError {
    WorldError(format!("facade export: {s}"))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn limits() -> fsof::FsofLimits {
    fsof::FsofLimits {
        max_file_bytes: MAX_OUTPUT_BYTES,
        max_decoded_bytes: MAX_OUTPUT_BYTES,
        max_vertices: 100_000,
        max_indices: 300_000,
        max_dimension: 2048,
        max_texture_pixels: 2_000_000,
    }
}
struct Fingerprint {
    hash: Sha256,
    bytes: usize,
}
impl Write for Fingerprint {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|&n| n <= MAX_INPUT_BYTES)
            .ok_or_else(|| io::Error::other("facade source byte budget"))?;
        self.hash.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Fingerprint the exact normalized source and export options without rendering.
/// This is a cache/provenance key, never an authenticated lot or actor identity.
pub fn facade_source_hash(
    world: &WorldDocument,
    options: FacadeExportOptions,
) -> Result<AssetKey, WorldError> {
    world.validate()?;
    if world.provenance.origin.len() > 4096 || world.provenance.source_revision.len() > 256 {
        return Err(error("source provenance byte budget"));
    }
    if !(1..=8).contains(&options.pixels_per_tile)
        || world.lot.width > 128
        || world.lot.height > 128
    {
        return Err(error("resolution or lot budget"));
    }
    let mut fingerprint = Fingerprint {
        hash: Sha256::new(),
        bytes: 0,
    };
    fingerprint
        .write_all(b"wonderland-loaded-facade-v1\0")
        .map_err(error)?;
    serde_json::to_writer(&mut fingerprint, world).map_err(error)?;
    serde_json::to_writer(&mut fingerprint, &options).map_err(error)?;
    Ok(AssetKey(fingerprint.hash.finalize().into()))
}
impl WorldFacadeJob {
    pub fn new(
        world: Arc<WorldDocument>,
        options: FacadeExportOptions,
    ) -> Result<Self, WorldError> {
        let key = facade_source_hash(&world, options)?;
        let hash = hex(&key.0);
        let source = source_world(&world);
        let layout_options = source::SourceFacadeOptions {
            floor_tiles: world.lot.width.max(world.lot.height),
            floor_resolution_per_tile: options.pixels_per_tile,
            ground_subdivisions: 5,
            thumbnail: None,
            draw_tiles: vec![],
        };
        let request = source
            .facade_request(layout_options.clone())
            .map_err(error)?;
        // The frame is derivative-local metadata only. Zero means an offline
        // source with no admitted lot ID, never an authenticated server actor.
        let input = d::DerivativeInput {
            frame: RenderFrame {
                stamp: FrameStamp {
                    lot_id: world.revision.lot_id.unwrap_or(0),
                    epoch: world.revision.epoch,
                    tick: world.revision.tick,
                    architecture_revision: world.revision.architecture_revision,
                    content: world.revision.content,
                },
                entities: vec![],
                selected: None,
            },
            source_provenance: key,
            lighting: [d::LightingPass::DAY, d::LightingPass::DAY],
            materials: vec![],
            draws: vec![],
            output: d::DerivativeOutput::Facade(request.clone()),
        };
        // Reuse the source room/floor/wall layout and FSOf geometry; draw the
        // real ordered client scene below, not the generic derivative shader.
        let prepared = source
            .prepare(
                input,
                layout_options,
                d::DerivativeRenderLimits {
                    max_views: 256,
                    max_output_bytes: MAX_OUTPUT_BYTES,
                    ..Default::default()
                },
            )
            .map_err(error)?
            .into_request()
            .day_only();
        let regions = prepared.regions().cloned().collect::<Vec<_>>();
        let template = prepared
            .render()
            .map_err(error)?
            .to_fsof([0; 4])
            .map_err(error)?;
        template.validate(limits()).map_err(error)?;
        let scene = build_scene_with_budget(
            &world,
            ViewportControls {
                visible_level: world.lot.levels,
                walls: WallMode::Up,
                show_roofs: true,
                ..Default::default()
            },
            SceneBudget {
                max_parts: 16_384,
                max_vertices: 250_000,
                max_indices: 750_000,
                max_buffer_bytes: 64 * 1024 * 1024,
            },
        )?;
        if scene
            .parts
            .iter()
            .any(|p| p.mesh.indices.len() / 3 > MAX_PART_TRIANGLES)
        {
            return Err(error("one mesh exceeds cooperative draw budget"));
        }
        let mut work = 0_u64;
        for region in &regions {
            for part in &scene.parts {
                if included(part, &world, region.role, &request) {
                    let command_work = draw_work(part, region)?;
                    if command_work > MAX_DRAW_WORK {
                        return Err(error(
                            "one projected mesh exceeds cooperative raster budget",
                        ));
                    }
                    work = work
                        .checked_add(command_work)
                        .filter(|&n| n <= MAX_WORK)
                        .ok_or_else(|| error("raster work budget"))?;
                }
            }
        }
        let renderer = WorldRenderer::new(world.clone())?;
        Ok(Self {
            state: Some(FacadeState {
                world,
                renderer,
                scene,
                template,
                regions,
                request,
                region: 0,
                command: 0,
                surface: None,
                hash,
                options,
                work,
            }),
        })
    }
    pub fn cancel(&mut self) {
        self.state = None;
    }
    /// Performs at most max_draws selected mesh commands, or one empty atlas
    /// cell. A finished, failed, or cancelled job is terminal.
    pub fn step(&mut self, max_draws: usize) -> Result<Option<WorldFacadeOutput>, WorldError> {
        if !(1..=128).contains(&max_draws) {
            return Err(error("step budget"));
        }
        let result = self
            .state
            .as_mut()
            .ok_or_else(|| error("finished or cancelled"))?
            .step(max_draws);
        if !matches!(result, Ok(None)) {
            self.state = None;
        }
        result
    }
}
impl FacadeState {
    fn step(&mut self, max_draws: usize) -> Result<Option<WorldFacadeOutput>, WorldError> {
        let mut remaining = max_draws;
        while self.region < self.regions.len() {
            let region = &self.regions[self.region];
            let [x, y, w, h] = region.rect;
            if self.surface.is_none() {
                let mut surface = ReferenceSurface::new(w, h, &RenderLimits::default())?;
                surface.clear([0; 4]);
                self.surface = Some(surface);
            }
            let surface = self
                .surface
                .as_mut()
                .ok_or_else(|| error("surface unavailable"))?;
            while self.command < self.scene.parts.len() {
                if remaining == 0 {
                    return Ok(None);
                }
                let part = &self.scene.parts[self.command];
                self.command += 1;
                // Charge skipped commands too: no unbounded scans in one turn.
                remaining -= 1;
                if !included(part, &self.world, region.role, &self.request) {
                    continue;
                }
                self.renderer
                    .draw_derivative_part(surface, part, region.clip_from_world)?;
            }
            let wall = matches!(region.role, d::RegionRole::Wall(_));
            let (target, stride, height) = if wall {
                (
                    &mut self.template.wall_texture,
                    self.template.wall_width,
                    self.template.wall_height,
                )
            } else {
                (
                    &mut self.template.floor_texture,
                    self.template.floor_width,
                    self.template.floor_height,
                )
            };
            let inset = u32::from(!wall);
            for row in inset..h - inset {
                for col in inset..w - inset {
                    let src = (row * w + col) as usize;
                    let dst = (((row + y) * stride + x + col) * 4) as usize;
                    target[dst..dst + 4].copy_from_slice(&surface.image().pixels[src]);
                }
            }
            if wall {
                bleed(target, stride, height, region.rect);
            }
            self.surface = None;
            self.command = 0;
            self.region += 1;
            // Empty atlas cells also yield: progress does not depend on objects.
            if remaining == max_draws || remaining == 0 {
                return Ok(None);
            }
        }
        let bytes = self.template.encode(true, limits()).map_err(error)?;
        let sha256 = hex(&Sha256::digest(&bytes));
        let diagnostics: Vec<_> = self
            .scene
            .diagnostics
            .iter()
            .take(64)
            .map(|d| {
                serde_json::json!({
                    "code":d.code.chars().take(64).collect::<String>(),
                    "resource":d.resource.chars().take(128).collect::<String>(),
                    "message":d.message.chars().take(512).collect::<String>()
                })
            })
            .collect();
        let metadata_json = serde_json::to_string(&serde_json::json!({
            "schema":1,"kind":"presentation_facade","format":"FSOf v1 RGBA8 gzip",
            "source_hash":self.hash,"sha256":sha256,"bytes":bytes.len(),
            "provenance":self.world.provenance,"revision":self.world.revision,
            "options":self.options,"lighting_state":"supplied-only; no invented night state",
            "geometry":"source room topology and facade atlas layout",
            "work_units":self.work,"regions":self.regions.len(),
            "diagnostics":diagnostics,"diagnostics_total":self.scene.diagnostics.len(),
            "not_a_game_save":true
        }))
        .map_err(error)?;
        if metadata_json.len() > 65_536 {
            return Err(error("metadata byte budget"));
        }
        Ok(Some(WorldFacadeOutput {
            bytes,
            metadata_json,
            source_hash: self.hash.clone(),
        }))
    }
}
fn source_world(world: &WorldDocument) -> source::SourceWorld {
    let lot = &world.lot;
    let width = usize::from(lot.width);
    let altitude = lot
        .terrain
        .corners
        .chunks_exact(width + 1)
        .take(usize::from(lot.height))
        .flat_map(|row| row[..width].iter().copied())
        .collect();
    source::SourceWorld {
        width: lot.width,
        height: lot.height,
        stories: lot.levels,
        altitude,
        base_alt: lot.terrain.base_alt,
        fine_area: None,
        rooms: None,
        tiles: lot
            .tiles
            .iter()
            .map(|t| {
                let west_solid = matches!(t.wall.styles[0], 1 | 255);
                let north_solid = matches!(t.wall.styles[1], 1 | 255);
                source::SourceTile {
                    floor_pattern: t.floor,
                    indoors: t.indoors,
                    walls: [
                        t.wall.west && west_solid,
                        t.wall.north && north_solid,
                        t.wall.south,
                        t.wall.east,
                    ],
                    fences: [
                        t.wall.west && !west_solid,
                        t.wall.north && !north_solid,
                        false,
                        false,
                    ],
                    diagonal: t.diagonal.map(|v| match v {
                        WorldDiagonal::Horizontal => source::SourceDiagonal::Horizontal,
                        WorldDiagonal::Vertical => source::SourceDiagonal::Vertical,
                    }),
                }
            })
            .collect(),
    }
}
fn included(
    part: &ScenePart,
    world: &WorldDocument,
    role: d::RegionRole,
    request: &d::FacadeRequest,
) -> bool {
    let level = part.level.saturating_sub(1);
    let object = part.object.map(|i| world.objects[i].level);
    let roof = matches!(
        part.surface,
        Some(
            WorldSurface::Roof
                | WorldSurface::RoofRim
                | WorldSurface::RoofUnderside
                | WorldSurface::RoofEdge
        )
    );
    let floor = matches!(
        part.surface,
        Some(
            WorldSurface::Terrain
                | WorldSurface::Floor
                | WorldSurface::Pool
                | WorldSurface::Water
                | WorldSurface::BuildSupport
        )
    );
    match role {
        d::RegionRole::Thumbnail => true,
        d::RegionRole::Floor(f) => {
            (floor && level == f)
                || object == Some(f + 1)
                || (roof && ((f > 0 && level == f - 1) || (f == request.stories - 1 && level == f)))
        }
        d::RegionRole::ObjectOverlay(_) => object == Some(1),
        d::RegionRole::Wall(i) => {
            matches!(
                part.surface,
                Some(WorldSurface::Wall | WorldSurface::WallTop)
            ) || object
                .is_some_and(|l| i16::from(l) >= i16::from(request.walls[i as usize].floor) - 5)
        }
    }
}
fn draw_work(part: &ScenePart, region: &d::AtlasRegion) -> Result<u64, WorldError> {
    let clip = region.clip_from_world * part.transform;
    let mut lo = [1_f64; 2];
    let mut hi = [-1_f64; 2];
    for v in &part.mesh.vertices {
        let p = clip.transform_vec4([v.position.x, v.position.y, v.position.z, 1.]);
        if p.iter().any(|v| !v.is_finite()) || p[3] <= 0. {
            return Err(error("invalid derivative projection"));
        }
        for axis in 0..2 {
            let x = (f64::from(p[axis]) / f64::from(p[3])).clamp(-1., 1.);
            lo[axis] = lo[axis].min(x);
            hi[axis] = hi[axis].max(x);
        }
    }
    let pixels = (0..2)
        .map(|axis| {
            ((hi[axis] - lo[axis]).max(0.) / 2. * f64::from(region.rect[axis + 2])).ceil() as u64
                + 1
        })
        .product::<u64>();
    pixels
        .checked_mul((part.mesh.indices.len() / 3) as u64)
        // Homogeneous clipping can fan one source triangle into several.
        // Charge a conservative fan bound, not just the original triangle.
        .and_then(|v| v.checked_mul(8))
        .and_then(|v| v.checked_add(part.mesh.vertices.len() as u64))
        .ok_or_else(|| error("work overflow"))
}
fn bleed(bytes: &mut [u8], width: u32, height: u32, [x, y, w, h]: [u32; 4]) {
    let mut copy = |sx: u32, sy: u32, dx: u32, dy: u32| {
        let a = ((sy * width + sx) * 4) as usize;
        let b = ((dy * width + dx) * 4) as usize;
        let value: [u8; 4] = bytes[a..a + 4].try_into().expect("validated atlas pixel");
        bytes[b..b + 4].copy_from_slice(&value);
    };
    for row in y..y + h {
        if x > 0 {
            copy(x, row, x - 1, row);
        }
        if x + w < width {
            copy(x + w - 1, row, x + w, row);
        }
    }
    for col in x.saturating_sub(1)..=(x + w).min(width - 1) {
        if y > 0 {
            copy(col, y, col, y - 1);
        }
        if y + h < height {
            copy(col, y + h - 1, col, y + h);
        }
    }
}

mod admission;
pub use admission::verify_world_facade;
