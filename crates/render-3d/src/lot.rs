//! Supplemental visual projection; these inputs never redefine collision.
use crate::Error;
use std::collections::BTreeSet;
use wonderland_render_core::{Mesh, RenderLimits, Vec2, Vec3, Vertex};
mod roofs;
mod walls;

pub const TILE_UNITS: f32 = 3.;
pub const STORY_UNITS: f32 = 2.95;
pub const TERRAIN_FACTOR: f32 = 3. / 160.;
pub const POOL: u16 = 65535;
pub const WATER: u16 = 65534;
pub const SUPPORTED_AIR: u16 = 65503;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TileCoord {
    pub x: u16,
    pub y: u16,
    pub level: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Diagonal {
    Vertical,
    Horizontal,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WallAppearance {
    /// West, north, south, east. Diagonals use south/east face patterns.
    pub patterns: [u16; 4],
    /// West/north; north also supplies diagonal style. Thickness uses source style.
    pub styles: [u16; 2],
    pub object_styles: [u16; 2],
    pub west: bool,
    pub north: bool,
    pub south: bool,
    pub east: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VisualTile {
    pub floor: u16,
    pub half_floors: Option<[u16; 2]>,
    pub diagonal: Option<Diagonal>,
    pub indoors: bool,
    pub supported: bool,
    pub wall: WallAppearance,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoofStyle {
    pub material: u32,
    pub pitch: f32,
    pub advanced: bool,
}
#[derive(Clone, Debug)]
pub struct VisualLot {
    pub width: u16,
    pub height: u16,
    pub levels: u8,
    /// Row-major (width+1)*(height+1), A corner order TL,TR,BL,BR.
    pub terrain: Vec<i16>,
    pub base_alt: i16,
    pub altitude_centers: Vec<i16>,
    pub grass: Vec<u8>,
    pub terrain_light: [f32; 4],
    pub terrain_dark: [f32; 4],
    /// Row-major tiles, then one-based levels.
    pub tiles: Vec<VisualTile>,
    pub roof: Option<RoofStyle>,
}
impl VisualLot {
    /// Synthetic flat input helper; real adapters supply all cosmetic fields.
    pub fn flat(width: u16, height: u16, levels: u8) -> Result<Self, Error> {
        let cells = check_dimensions(width, height, levels)?;
        let area = usize::from(width) * usize::from(height);
        Ok(Self {
            width,
            height,
            levels,
            terrain: vec![0; (usize::from(width) + 1) * (usize::from(height) + 1)],
            base_alt: 0,
            altitude_centers: vec![0; area],
            grass: vec![0; area],
            terrain_light: [0.4, 0.7, 0.25, 1.],
            terrain_dark: [0.45, 0.3, 0.15, 1.],
            tiles: vec![VisualTile::default(); cells],
            roof: None,
        })
    }
    pub fn tile_index(&self, p: TileCoord) -> Option<usize> {
        check_dimensions(self.width, self.height, self.levels).ok()?;
        if p.x >= self.width || p.y >= self.height || p.level == 0 || p.level > self.levels {
            return None;
        }
        Some(
            ((usize::from(p.level) - 1) * usize::from(self.height) + usize::from(p.y))
                * usize::from(self.width)
                + usize::from(p.x),
        )
    }
    /// Bilinear visual contact in final graphics coordinates, not collision.
    pub fn contact_height(&self, x: f32, y: f32, level: u8) -> Result<f32, Error> {
        self.validate()?;
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.
            || y < 0.
            || x > f32::from(self.width)
            || y > f32::from(self.height)
            || level == 0
            || level > self.levels + 1
        {
            return Err(Error::InvalidInput("visual contact position"));
        }
        Ok(self.height_at(x, y, level))
    }
    fn validate(&self) -> Result<(), Error> {
        let count = check_dimensions(self.width, self.height, self.levels)?;
        let area = usize::from(self.width) * usize::from(self.height);
        if self.tiles.len() != count
            || self.terrain.len() != (usize::from(self.width) + 1) * (usize::from(self.height) + 1)
            || self.grass.len() != area
            || self.altitude_centers.len() != area
        {
            return Err(Error::InvalidInput("visual lot array lengths"));
        }
        if self
            .terrain_light
            .iter()
            .chain(self.terrain_dark.iter())
            .any(|v| !v.is_finite())
        {
            return Err(Error::InvalidInput("terrain color"));
        }
        if let Some(r) = self.roof {
            if !r.pitch.is_finite() || r.pitch < 0. || r.pitch > 4. {
                return Err(Error::InvalidInput("roof pitch"));
            }
        }
        Ok(())
    }
    fn get(&self, x: i32, y: i32, level: u8) -> Option<&VisualTile> {
        if x < 0 || y < 0 || x >= i32::from(self.width) || y >= i32::from(self.height) {
            return None;
        }
        self.tile_index(TileCoord {
            x: x as u16,
            y: y as u16,
            level,
        })
        .and_then(|i| self.tiles.get(i))
    }
    fn raw(&self, x: usize, y: usize) -> f32 {
        f32::from(self.terrain[y * (usize::from(self.width) + 1) + x])
    }
    fn raw_bilinear(&self, x: f32, y: f32) -> f32 {
        let x = x.clamp(0., f32::from(self.width));
        let y = y.clamp(0., f32::from(self.height));
        let bx = (x.floor() as usize).min(usize::from(self.width) - 1);
        let by = (y.floor() as usize).min(usize::from(self.height) - 1);
        let u = x - bx as f32;
        let v = y - by as f32;
        (self.raw(bx, by) * (1. - u) + self.raw(bx + 1, by) * u) * (1. - v)
            + (self.raw(bx, by + 1) * (1. - u) + self.raw(bx + 1, by + 1) * u) * v
    }
    fn height_at(&self, x: f32, y: f32, level: u8) -> f32 {
        ((self.raw_bilinear(x, y) - f32::from(self.base_alt)) * TERRAIN_FACTOR
            + f32::from(level - 1) * STORY_UNITS)
            * TILE_UNITS
    }
    fn normal_at(&self, x: usize, y: usize) -> Vec3 {
        // Source shading has unit horizontal steps and graphics-height differences.
        // Preserve the x>1/y>1 backward-neighbor rule, fixing rectangular bounds.
        let h = |xx, yy| self.raw(xx, yy) * TERRAIN_FACTOR * TILE_UNITS;
        let mut n = Vec3::ZERO;
        if x < usize::from(self.width) {
            n = n + Vec3::new(h(x, y) - h(x + 1, y), 1., 0.);
        }
        if x > 1 {
            n = n + Vec3::new(h(x - 1, y) - h(x, y), 1., 0.);
        }
        if y < usize::from(self.height) {
            n = n + Vec3::new(0., 1., h(x, y) - h(x, y + 1));
        }
        if y > 1 {
            n = n + Vec3::new(0., 1., h(x, y - 1) - h(x, y));
        }
        let n = n.normalize_or_zero();
        if n == Vec3::ZERO {
            Vec3::Y
        } else {
            n
        }
    }
}
fn check_dimensions(width: u16, height: u16, levels: u8) -> Result<usize, Error> {
    if width == 0 || height == 0 || width > 256 || height > 256 || levels == 0 || levels > 16 {
        return Err(Error::InvalidInput("lot dimensions"));
    }
    let cells = usize::from(width) * usize::from(height) * usize::from(levels);
    if cells > 262144 {
        return Err(Error::InvalidInput("lot tile count"));
    }
    Ok(cells)
}
#[derive(Clone, Copy, Debug)]
pub struct GeometryBudget {
    pub max_cells: usize,
    pub max_vertices: usize,
    pub max_indices: usize,
    pub max_roof_evaluations: usize,
}
impl Default for GeometryBudget {
    fn default() -> Self {
        Self {
            max_cells: 262144,
            max_vertices: 2000000,
            max_indices: 6000000,
            max_roof_evaluations: 4000000,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Cutaway {
    pub level: u8,
    pub rotation: u8,
    pub tiles: Vec<bool>,
}
#[derive(Clone, Debug)]
pub struct PoolAssets {
    pub tiles: Vec<Mesh>,
    pub corners: Vec<Mesh>,
}
#[derive(Clone, Debug)]
pub struct BuildOptions {
    pub visible_level: u8,
    pub show_roofs: bool,
    pub build_mode: bool,
    pub cutaway: Option<Cutaway>,
    pub pool_assets: Option<PoolAssets>,
    pub budget: GeometryBudget,
}
impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            visible_level: u8::MAX,
            show_roofs: true,
            build_mode: false,
            cutaway: None,
            pool_assets: None,
            budget: GeometryBudget::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SurfaceKind {
    Terrain,
    Floor,
    Wall,
    WallTop,
    Pool,
    Water,
    Roof,
    RoofRim,
    RoofUnderside,
    RoofEdge,
    BuildSupport,
}
#[derive(Clone, Debug)]
pub struct MeshPart {
    pub kind: SurfaceKind,
    pub material: u32,
    pub style: u16,
    pub level: u8,
    pub tile: Option<TileCoord>,
    pub mesh: Mesh,
}
#[derive(Clone, Debug)]
pub struct LotMeshOutput {
    pub parts: Vec<MeshPart>,
    pub roof_rectangles: Vec<(u8, RoofRect)>,
    pub missing_assets: Vec<&'static str>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoofRect {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}
pub fn floor_half_indices(diagonal: Diagonal, half: usize) -> [u32; 3] {
    match (diagonal, half) {
        (Diagonal::Vertical, 0) => [0, 1, 2],
        (Diagonal::Vertical, _) => [2, 3, 0],
        (Diagonal::Horizontal, 0) => [0, 1, 3],
        (Diagonal::Horizontal, _) => [1, 2, 3],
    }
}
pub fn source_side_to_half(diagonal: Diagonal, side: bool) -> usize {
    match diagonal {
        Diagonal::Vertical => usize::from(!side),
        Diagonal::Horizontal => usize::from(side),
    }
}
pub fn pool_selector(neighbors: u8) -> (u8, Vec<u8>) {
    let mut selector = 0;
    for (bit, src) in [(1, 1), (2, 64), (4, 16), (8, 4)] {
        if neighbors & src != 0 {
            selector |= bit;
        }
    }
    let mut corners = Vec::new();
    for (c, cardinal, diagonal) in [(0, 65, 128), (1, 80, 32), (2, 20, 8), (3, 5, 2)] {
        if neighbors & cardinal == cardinal && neighbors & diagonal == 0 {
            corners.push(c);
        }
    }
    (selector, corners)
}
pub fn pool_neighbors(lot: &VisualLot, p: TileCoord) -> u8 {
    let mut mask = 0;
    for (i, (dx, dy)) in [
        (0, -1),
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
    ]
    .into_iter()
    .enumerate()
    {
        let x = i32::from(p.x) + dx;
        let y = i32::from(p.y) + dy;
        if x <= 0
            || x >= i32::from(lot.width) - 1
            || y <= 0
            || y >= i32::from(lot.height) - 1
            || lot
                .get(x, y, p.level)
                .map(|t| t.floor == POOL)
                .unwrap_or(false)
        {
            mask |= 1 << i;
        }
    }
    mask
}
pub fn roofable(lot: &VisualLot, x: i32, y: i32, level: u8) -> bool {
    lot.validate().is_ok() && roofs::roofable(lot, x, y, level)
}
pub fn roof_rectangles(lot: &VisualLot, level: u8, budget: usize) -> Result<Vec<RoofRect>, Error> {
    lot.validate()?;
    roofs::rectangles(lot, level, budget)
}
pub fn build_lot(lot: &VisualLot, options: &BuildOptions) -> Result<LotMeshOutput, Error> {
    build_internal(lot, options, None)
}
/// Representative original synthetic scene for reference rasterization only.
pub fn synthetic_lot() -> VisualLot {
    let mut lot = VisualLot::flat(6, 6, 2).expect("fixed fixture dimensions");
    for y in 0..=6 {
        for x in 0..=6 {
            lot.terrain[y * 7 + x] = (x as i16 * 2) - (y as i16);
        }
    }
    for y in 0..6 {
        for x in 0..6 {
            lot.altitude_centers[y * 6 + x] = (x as i16 * 2) - (y as i16);
            lot.grass[y * 6 + x] = ((x * 31 + y * 17) % 256) as u8;
        }
    }
    for y in 2..4 {
        for x in 2..4 {
            let t = &mut lot.tiles[y * 6 + x];
            t.indoors = true;
            t.floor = 4;
            t.wall.patterns = [8, 9, 10, 11];
            t.wall.styles = [1, 1];
            t.wall.west = x == 2;
            t.wall.north = y == 2;
        }
    }
    lot.tiles[3 * 6 + 3].diagonal = Some(Diagonal::Vertical);
    lot.tiles[3 * 6 + 3].half_floors = Some([5, 6]);
    lot.tiles[2 * 6 + 4].diagonal = Some(Diagonal::Horizontal);
    lot.tiles[2 * 6 + 4].half_floors = Some([7, 8]);
    lot.tiles[2 * 6 + 4].wall.styles[1] = 7;
    lot.tiles[2 * 6 + 4].wall.patterns = [0, 0, 12, 13];
    lot.tiles[4 * 6 + 1].floor = POOL;
    lot.tiles[4 * 6 + 2].floor = POOL;
    lot.tiles[4 * 6 + 4].floor = WATER;
    lot.tiles[36 + 2 * 6 + 2].floor = 14;
    lot.tiles[36 + 2 * 6 + 2].wall.north = true;
    lot.tiles[36 + 2 * 6 + 2].wall.styles[1] = 255;
    lot.tiles[36 + 2 * 6 + 2].wall.patterns[1] = 15;
    lot.roof = Some(RoofStyle {
        material: 16,
        pitch: 0.5,
        advanced: true,
    });
    lot
}
pub fn grass_shade(lot: &VisualLot, x: i32, y: i32) -> f32 {
    let idx = (i64::from(y) - 1) * i64::from(lot.width) + i64::from(x) - 1;
    if idx < 0 {
        1.
    } else {
        lot.grass
            .get(idx as usize)
            .map(|v| f32::from(*v) / 255.)
            .unwrap_or(1.)
    }
}
pub fn grass_eligible(tile: &VisualTile) -> bool {
    tile.floor == 0 && tile.diagonal.is_none()
}
#[derive(Clone, Debug, Default)]
pub struct DirtySet {
    pub terrain_vertices: BTreeSet<(u16, u16)>,
    pub floors: BTreeSet<TileCoord>,
    pub walls: BTreeSet<TileCoord>,
    pub rooms: BTreeSet<TileCoord>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RebuildPlan {
    pub tiles: BTreeSet<TileCoord>,
    pub roof_levels: BTreeSet<u8>,
    pub evaluated_cells: usize,
}
pub fn plan_rebuild(
    lot: &VisualLot,
    dirty: &DirtySet,
    budget: usize,
) -> Result<RebuildPlan, Error> {
    lot.validate()?;
    let mut plan = RebuildPlan {
        tiles: BTreeSet::new(),
        roof_levels: BTreeSet::new(),
        evaluated_cells: 0,
    };
    for &(vx, vy) in &dirty.terrain_vertices {
        if vx > lot.width || vy > lot.height {
            return Err(Error::InvalidInput("dirty terrain vertex"));
        }
        for l in 1..=lot.levels {
            for y in i32::from(vy) - 2..=i32::from(vy) + 1 {
                for x in i32::from(vx) - 2..=i32::from(vx) + 1 {
                    insert_valid(lot, &mut plan.tiles, x, y, l);
                }
            }
        }
        for l in 2..=lot.levels + 1 {
            plan.roof_levels.insert(l);
        }
    }
    for p in dirty
        .floors
        .iter()
        .chain(dirty.walls.iter())
        .chain(dirty.rooms.iter())
    {
        if lot.tile_index(*p).is_none() {
            return Err(Error::InvalidInput("dirty tile"));
        }
        for y in i32::from(p.y) - 1..=i32::from(p.y) + 1 {
            for x in i32::from(p.x) - 1..=i32::from(p.x) + 1 {
                insert_valid(lot, &mut plan.tiles, x, y, p.level);
            }
        }
        if p.level > 1 {
            plan.roof_levels.insert(p.level);
        }
        plan.roof_levels.insert(p.level + 1);
    }
    // Whole affected levels deliberately bound rectangle-spread propagation.
    plan.evaluated_cells = plan.tiles.len()
        + plan.roof_levels.len() * usize::from(lot.width) * usize::from(lot.height) * 4;
    if plan.evaluated_cells > budget {
        return Err(Error::BudgetExceeded("dirty component/roof cells"));
    }
    Ok(plan)
}
fn insert_valid(lot: &VisualLot, into: &mut BTreeSet<TileCoord>, x: i32, y: i32, l: u8) {
    if x >= 0 && y >= 0 && x < i32::from(lot.width) && y < i32::from(lot.height) {
        into.insert(TileCoord {
            x: x as u16,
            y: y as u16,
            level: l,
        });
    }
}
pub fn rebuild_dirty(
    lot: &VisualLot,
    options: &BuildOptions,
    dirty: &DirtySet,
) -> Result<(RebuildPlan, LotMeshOutput), Error> {
    let plan = plan_rebuild(lot, dirty, options.budget.max_roof_evaluations)?;
    let output = build_internal(lot, options, Some(&plan))?;
    Ok((plan, output))
}

struct Acc<'a> {
    out: LotMeshOutput,
    options: &'a BuildOptions,
    vertices: usize,
    indices: usize,
}
impl<'a> Acc<'a> {
    fn add(
        &mut self,
        kind: SurfaceKind,
        material: u32,
        style: u16,
        p: Option<TileCoord>,
        level: u8,
        mesh: Mesh,
    ) -> Result<(), Error> {
        let vertices = self
            .vertices
            .checked_add(mesh.vertices.len())
            .ok_or(Error::BudgetExceeded("vertices"))?;
        let indices = self
            .indices
            .checked_add(mesh.indices.len())
            .ok_or(Error::BudgetExceeded("indices"))?;
        if vertices > self.options.budget.max_vertices || indices > self.options.budget.max_indices
        {
            return Err(Error::BudgetExceeded("lot mesh"));
        }
        mesh.validate(&RenderLimits::default())
            .map_err(|_| Error::InvalidInput("generated mesh"))?;
        self.vertices = vertices;
        self.indices = indices;
        self.out.parts.push(MeshPart {
            kind,
            material,
            style,
            tile: p,
            level,
            mesh,
        });
        Ok(())
    }
}
fn build_internal(
    lot: &VisualLot,
    o: &BuildOptions,
    filter: Option<&RebuildPlan>,
) -> Result<LotMeshOutput, Error> {
    lot.validate()?;
    if lot.tiles.len() > o.budget.max_cells {
        return Err(Error::BudgetExceeded("lot cells"));
    }
    if o.visible_level == 0 {
        return Err(Error::InvalidInput("visible level"));
    }
    if let Some(c) = &o.cutaway {
        if c.level == 0
            || c.level > lot.levels
            || c.rotation > 3
            || c.tiles.len() != usize::from(lot.width) * usize::from(lot.height)
        {
            return Err(Error::InvalidInput("cutaway"));
        }
    }
    if let Some(pool) = &o.pool_assets {
        if pool.tiles.len() != 16 || pool.corners.len() != 4 {
            return Err(Error::InvalidInput("pool asset array"));
        }
        for mesh in pool.tiles.iter().chain(pool.corners.iter()) {
            mesh.validate(&RenderLimits::default())
                .map_err(|_| Error::InvalidInput("pool asset mesh"))?;
            if mesh.indices.is_empty() {
                return Err(Error::InvalidInput("empty pool asset"));
            }
        }
    }
    let mut a = Acc {
        out: LotMeshOutput {
            parts: Vec::new(),
            roof_rectangles: Vec::new(),
            missing_assets: Vec::new(),
        },
        options: o,
        vertices: 0,
        indices: 0,
    };
    let coords: Vec<_> = if let Some(f) = filter {
        f.tiles.iter().copied().collect()
    } else {
        (1..=lot.levels.min(o.visible_level))
            .flat_map(|level| {
                (0..lot.height)
                    .flat_map(move |y| (0..lot.width).map(move |x| TileCoord { x, y, level }))
            })
            .collect()
    };
    for p in coords {
        if p.level > o.visible_level {
            continue;
        }
        let t = &lot.tiles[lot
            .tile_index(p)
            .ok_or(Error::InvalidInput("rebuild tile"))?];
        if p.level == 1 {
            a.add(
                SurfaceKind::Terrain,
                0,
                0,
                Some(p),
                1,
                tile_mesh(lot, p, None, true, 0.),
            )?;
        }
        if t.floor == POOL {
            add_pool(lot, p, &mut a)?;
        } else if let Some(d) = t.diagonal {
            for (half, id) in t
                .half_floors
                .unwrap_or([t.floor; 2])
                .into_iter()
                .enumerate()
            {
                add_floor(lot, p, t, id, Some(floor_half_indices(d, half)), &mut a)?;
            }
        } else {
            add_floor(lot, p, t, t.floor, None, &mut a)?;
        }
        walls::build(lot, p, t, &mut a)?;
    }
    if o.show_roofs {
        if let Some(style) = lot.roof {
            for level in 2..=lot.levels + 1 {
                if level > o.visible_level.saturating_add(1)
                    || filter
                        .map(|f| !f.roof_levels.contains(&level))
                        .unwrap_or(false)
                {
                    continue;
                }
                let rects = roofs::rectangles(lot, level, o.budget.max_roof_evaluations)?;
                for rect in rects {
                    roofs::mesh(lot, rect, level, style, &mut a)?;
                    a.out.roof_rectangles.push((level, rect));
                }
            }
        }
    }
    Ok(a.out)
}
fn add_floor(
    lot: &VisualLot,
    p: TileCoord,
    t: &VisualTile,
    id: u16,
    half: Option<[u32; 3]>,
    a: &mut Acc<'_>,
) -> Result<(), Error> {
    if id == 0 {
        if p.level > 1 && t.supported && a.options.build_mode {
            return a.add(
                SurfaceKind::BuildSupport,
                u32::from(SUPPORTED_AIR),
                0,
                Some(p),
                p.level,
                tile_mesh(lot, p, half, false, 0.),
            );
        }
        return Ok(());
    }
    let (kind, rise) = if id == WATER {
        (SurfaceKind::Water, 0.05)
    } else {
        (SurfaceKind::Floor, 0.)
    };
    a.add(
        kind,
        u32::from(id),
        0,
        Some(p),
        p.level,
        tile_mesh(lot, p, half, false, rise),
    )
}
fn tile_mesh(
    lot: &VisualLot,
    p: TileCoord,
    half: Option<[u32; 3]>,
    terrain: bool,
    rise: f32,
) -> Mesh {
    let x = f32::from(p.x);
    let y = f32::from(p.y);
    let coords = [(x, y), (x + 1., y), (x + 1., y + 1.), (x, y + 1.)];
    let uvs = [
        Vec2::new((x - y + 1.) / 2., (x + y) / 2.),
        Vec2::new((x - y + 2.) / 2., (x + y + 1.) / 2.),
        Vec2::new((x - y + 1.) / 2., (x + y + 2.) / 2.),
        Vec2::new((x - y) / 2., (x + y + 1.) / 2.),
    ];
    let vertices = coords
        .into_iter()
        .enumerate()
        .map(|(i, (x, y))| {
            let shade = grass_shade(lot, x as i32, y as i32);
            let mut color = [1.; 4];
            if terrain {
                for (c, (l, d)) in color
                    .iter_mut()
                    .zip(lot.terrain_light.iter().zip(lot.terrain_dark.iter()))
                {
                    *c = *l * (1. - shade) + *d * shade;
                }
            }
            Vertex {
                position: Vec3::new(x * 3., lot.height_at(x, y, p.level) + rise, y * 3.),
                normal: lot.normal_at(x as usize, y as usize),
                uv: uvs[i],
                color,
            }
        })
        .collect();
    Mesh {
        vertices,
        indices: half
            .map(|h| h.to_vec())
            .unwrap_or_else(|| vec![0, 1, 2, 2, 3, 0]),
    }
}
fn append(dst: &mut Mesh, src: Mesh) {
    let base = dst.vertices.len() as u32;
    dst.indices
        .extend(src.indices.into_iter().map(|i| i + base));
    dst.vertices.extend(src.vertices);
}
fn empty_mesh() -> Mesh {
    Mesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    }
}
fn quad(points: [Vec3; 4], normal: Vec3, uv: [Vec2; 4], reverse: bool) -> Mesh {
    Mesh {
        vertices: points
            .into_iter()
            .zip(uv)
            .map(|(position, uv)| Vertex {
                position,
                normal,
                uv,
                color: [1.; 4],
            })
            .collect(),
        indices: if reverse {
            vec![2, 1, 0, 0, 3, 2]
        } else {
            vec![0, 1, 2, 0, 2, 3]
        },
    }
}
fn unit_uv() -> [Vec2; 4] {
    [
        Vec2::new(0., 0.),
        Vec2::new(1., 0.),
        Vec2::new(1., 1.),
        Vec2::new(0., 1.),
    ]
}
fn add_pool(lot: &VisualLot, p: TileCoord, a: &mut Acc<'_>) -> Result<(), Error> {
    let neighbors = pool_neighbors(lot, p);
    let (selector, corners) = pool_selector(neighbors);
    let mut mesh = empty_mesh();
    if let Some(assets) = &a.options.pool_assets {
        let mut selected = vec![&assets.tiles[15]];
        if selector != 15 {
            selected.push(&assets.tiles[usize::from(selector)]);
        }
        for c in corners {
            selected.push(&assets.corners[usize::from(c)]);
        }
        for part in selected {
            if mesh.vertices.len() + part.vertices.len() + a.vertices
                > a.options.budget.max_vertices
                || mesh.indices.len() + part.indices.len() + a.indices
                    > a.options.budget.max_indices
            {
                return Err(Error::BudgetExceeded("composed pool mesh"));
            }
            let mut part = part.clone();
            for v in &mut part.vertices {
                let x = f32::from(p.x) + v.position.x;
                let y = f32::from(p.y) + v.position.z;
                v.position = Vec3::new(
                    x * 3.,
                    v.position.y * 3. + lot.height_at(x, y, p.level),
                    y * 3.,
                );
            }
            for tri in part.indices.chunks_exact_mut(3) {
                tri.swap(1, 2);
            }
            append(&mut mesh, part);
        }
    } else {
        const MISSING:&str="Content/3D/floor/pool_hq_0..15.obj, poolcorner_hq_0..3.obj, pool.png (synthetic fallback)";
        if !a.out.missing_assets.contains(&MISSING) {
            a.out.missing_assets.push(MISSING);
        }
        mesh = tile_mesh(lot, p, None, false, -3.);
        let top = tile_mesh(lot, p, None, false, 0.05);
        let pts: Vec<_> = top.vertices.iter().map(|v| v.position).collect();
        for (i, bit) in [1, 4, 16, 64].into_iter().enumerate() {
            if neighbors & bit == 0 {
                let from = pts[i];
                let to = pts[(i + 1) % 4];
                let normal = (to - from).cross(Vec3::Y).normalize_or_zero();
                append(
                    &mut mesh,
                    quad(
                        [from, to, to - Vec3::Y * 3., from - Vec3::Y * 3.],
                        normal,
                        unit_uv(),
                        false,
                    ),
                );
            }
        }
        append(&mut mesh, top);
    }
    a.add(
        SurfaceKind::Pool,
        u32::from(POOL),
        0,
        Some(p),
        p.level,
        mesh,
    )
}
