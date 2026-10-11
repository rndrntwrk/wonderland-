//! Immutable Blueprint/WorldDocument adapter for the original derivative path.
//!
//! Exact source room tables take priority. When a normalized document has only
//! wall tiles, bounded half-tile connectivity recovers its presentation topology.
//! No room light, lamp contribution, or material texture is invented here.
use super::{
    fsof::{FacadeGeometry, FsofLimits, FsofMesh, FsofVertex},
    *,
};
use std::collections::{BTreeMap, BTreeSet};

const MAX_CELLS: usize = 262_144;
type Result<T> = std::result::Result<T, DerivativeError>;
pub type SourceLine = [[i32; 2]; 2];
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceDiagonal {
    Vertical,
    Horizontal,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceTile {
    pub floor_pattern: u16,
    /// West, north, south, east. Fences do not close room connectivity.
    pub walls: [bool; 4],
    pub fences: [bool; 4],
    pub diagonal: Option<SourceDiagonal>,
    /// Source classification, when available; never inferred from floor paint.
    pub indoors: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRoom {
    pub id: u16,
    pub base: u16,
    pub floor: u8,
    pub is_outside: bool,
    pub wall_lines: Vec<SourceLine>,
    pub fence_lines: Vec<SourceLine>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRooms {
    /// Indexed by room ID, including the zero sentinel.
    pub rooms: Vec<SourceRoom>,
    /// Row-major per zero-based floor. Low/high u16 identify diagonal halves.
    pub map: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceWorld {
    pub width: u16,
    pub height: u16,
    pub stories: u8,
    /// Blueprint.Altitude, width*height raw height samples. A WorldDocument
    /// adapter takes the first width samples of each (width+1)-corner row.
    pub altitude: Vec<i16>,
    pub base_alt: i16,
    pub tiles: Vec<SourceTile>,
    pub rooms: Option<SourceRooms>,
    pub fine_area: Option<Vec<bool>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceThumbnailMode {
    Tso,
    Ts1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFacadeOptions {
    pub floor_tiles: u16,
    pub floor_resolution_per_tile: u16,
    pub ground_subdivisions: u16,
    pub thumbnail: Option<SourceThumbnailMode>,
    /// Optional source tile for each draw: x,y,one-based level. Supply this
    /// for floor batches larger than one tile. Objects use frame transforms.
    pub draw_tiles: Vec<Option<[u16; 3]>>,
}
impl Default for SourceFacadeOptions {
    fn default() -> Self {
        Self {
            floor_tiles: 64,
            floor_resolution_per_tile: 2,
            ground_subdivisions: 5,
            thumbnail: Some(SourceThumbnailMode::Tso),
            draw_tiles: vec![],
        }
    }
}
#[derive(Clone, Debug)]
pub struct SourceThumbnailPlan {
    pub request: ThumbnailRequest,
    pub center_tile: Vec2,
    /// x,y,width,height. Default source slice is (6,6,width-13,height-13).
    pub buildable_bounds: [u16; 4],
    width: u16,
    height: u16,
    fine_area: Option<Vec<bool>>,
}
impl SourceThumbnailPlan {
    pub fn contains_tile(&self, x: u16, y: u16) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        if let Some(fine) = &self.fine_area {
            return fine[y as usize * self.width as usize + x as usize];
        }
        let [bx, by, w, h] = self.buildable_bounds;
        x >= bx && y >= by && x < bx + w && y < by + h
    }
}
pub struct PreparedSourceDerivative {
    request: PreparedDerivative,
    thumbnail: Option<SourceThumbnailPlan>,
}
impl PreparedSourceDerivative {
    pub fn request(&self) -> &PreparedDerivative {
        &self.request
    }
    pub fn thumbnail(&self) -> Option<&SourceThumbnailPlan> {
        self.thumbnail.as_ref()
    }
    pub fn into_request(self) -> PreparedDerivative {
        self.request
    }
}

impl SourceWorld {
    /// Validate every supplied source array and return the ground-layer area.
    pub fn validate(&self) -> Result<usize> {
        let area = self.width as usize * self.height as usize;
        let cells = area
            .checked_mul(self.stories as usize)
            .ok_or(DerivativeError::Limit("source cells"))?;
        if self.width < 2
            || self.height < 2
            || !(1..=5).contains(&self.stories)
            || cells > MAX_CELLS
        {
            return Err(DerivativeError::Limit("source world dimensions"));
        }
        if self.altitude.len() != area
            || self.tiles.len() != cells
            || self.fine_area.as_ref().is_some_and(|f| f.len() != area)
        {
            return Err(DerivativeError::Invalid("source world arrays"));
        }
        if let Some(rooms) = &self.rooms {
            self.validate_rooms(rooms)?;
        }
        Ok(area)
    }
    fn validate_rooms(&self, source: &SourceRooms) -> Result<()> {
        if source.rooms.is_empty()
            || source.rooms.len() > 65_536
            || source.map.len() != self.tiles.len()
        {
            return Err(DerivativeError::Invalid("source room arrays"));
        }
        let mut count = 0usize;
        for (id, room) in source.rooms.iter().enumerate() {
            if room.id as usize != id
                || room.base as usize >= source.rooms.len()
                || room.floor >= self.stories
            {
                return Err(DerivativeError::Invalid("source room identity"));
            }
            let base = &source.rooms[room.base as usize];
            if base.base != base.id || (id != 0 && base.floor != room.floor) {
                return Err(DerivativeError::Invalid("source room base"));
            }
            count = count
                .checked_add(room.wall_lines.len() + room.fence_lines.len())
                .ok_or(DerivativeError::Limit("source room lines"))?;
            if count > MAX_CELLS * 5 {
                return Err(DerivativeError::Limit("source room lines"));
            }
            for line in room.wall_lines.iter().chain(&room.fence_lines) {
                if line[0] == line[1]
                    || line.iter().any(|p| {
                        p[0] < 0
                            || p[1] < 0
                            || p[0] > self.width as i32 * 16
                            || p[1] > self.height as i32 * 16
                    })
                {
                    return Err(DerivativeError::Invalid("source room line"));
                }
            }
        }
        let area = self.width as usize * self.height as usize;
        for (index, &room) in source.map.iter().enumerate() {
            for id in [room & 0xffff, room >> 16] {
                if id as usize >= source.rooms.len()
                    || (id != 0 && source.rooms[id as usize].floor as usize != index / area)
                {
                    return Err(DerivativeError::Invalid("source room map"));
                }
            }
        }
        Ok(())
    }
    pub fn interp_altitude(&self, x: f32, y: f32) -> Result<f32> {
        self.validate()?;
        if !x.is_finite() || !y.is_finite() {
            return Err(DerivativeError::Invalid("source altitude position"));
        }
        Ok(self.altitude_at(x, y))
    }
    fn altitude_at(&self, x: f32, y: f32) -> f32 {
        let bx = x.clamp(1., self.width as f32 - 1.) as usize;
        let by = y.clamp(1., self.height as f32 - 1.) as usize;
        let nx = x.ceil().clamp(1., self.width as f32 - 1.) as usize;
        let ny = y.ceil().clamp(1., self.height as f32 - 1.) as usize;
        let xl = x % 1.;
        let yl = y % 1.;
        let w = self.width as usize;
        let lo =
            xl * self.altitude[by * w + nx] as f32 + (1. - xl) * self.altitude[by * w + bx] as f32;
        let hi =
            xl * self.altitude[ny * w + nx] as f32 + (1. - xl) * self.altitude[ny * w + bx] as f32;
        (yl * hi + (1. - yl) * lo - self.base_alt as f32) * (3. / 160.)
    }
    pub fn floors_used(&self) -> Result<u8> {
        let area = self.validate()?;
        let mut used = 1;
        for (level, tiles) in self.tiles.chunks_exact(area).enumerate().skip(1) {
            if tiles.iter().any(|t| {
                t.floor_pattern != 0
                    || t.diagonal.is_some()
                    || t.walls.iter().chain(&t.fences).any(|&w| w)
            }) {
                used = level as u8 + 1;
            }
        }
        Ok(used)
    }
    pub fn thumbnail_plan(&self, mode: SourceThumbnailMode) -> Result<SourceThumbnailPlan> {
        self.validate()?;
        let mut center = Vec2::new((self.width / 2) as f32, (self.height / 2) as f32);
        let bounds = if let Some(fine) = &self.fine_area {
            let mut lo = [self.width, self.height];
            let mut hi = [0, 0];
            let mut found = false;
            for (i, &yes) in fine.iter().enumerate() {
                if yes {
                    let x = (i % self.width as usize) as u16;
                    let y = (i / self.width as usize) as u16;
                    lo[0] = lo[0].min(x);
                    lo[1] = lo[1].min(y);
                    hi[0] = hi[0].max(x);
                    hi[1] = hi[1].max(y);
                    found = true;
                }
            }
            if !found {
                return Err(DerivativeError::Invalid("empty source fine area"));
            }
            let w = hi[0] - lo[0] + 1;
            let h = hi[1] - lo[1] + 1;
            center = Vec2::new(lo[0] as f32 + w as f32 / 2., lo[1] as f32 + h as f32 / 2.);
            let shift =
                self.altitude_at((lo[0] + w) as f32, (lo[1] + h) as f32) / 2.95 * (230. / 16.) / 4.;
            center.x -= shift;
            center.y -= shift;
            [lo[0], lo[1], w, h]
        } else {
            if self.width <= 13 || self.height <= 13 {
                return Err(DerivativeError::Invalid("source thumbnail buildable slice"));
            }
            [6, 6, self.width - 13, self.height - 13]
        };
        let (size, zoom) = match mode {
            SourceThumbnailMode::Tso => (576, 0.25),
            SourceThumbnailMode::Ts1 => (self.width as u32 * 16, 0.5),
        };
        // Canonical source viewport equals output size. The original viewport
        // offset and off-center projection cancel; preserve camera quantization.
        let sx = round_even(((center.x - center.y) * 16.) as f64) as f32 - 2.;
        let sy = round_even(((center.x + center.y) * 8. + 0.5) as f64) as f32;
        let cy = (sy / 8. - sx / 16.) / 2.;
        let cx = cy + sx / 16.;
        let rotate =
            Mat4::from_quat(Quat::from_axis_angle(Vec3::X, std::f32::consts::PI / 6.).unwrap())
                * Mat4::from_quat(
                    Quat::from_axis_angle(Vec3::Y, -std::f32::consts::FRAC_PI_4).unwrap(),
                );
        let half = size as f32 * (18f32.sqrt() / (64. * zoom));
        let depth = 1024. / zoom;
        let near = -(150. + depth - 64.);
        let far = depth;
        // WorldCamera intentionally has a negative near plane; generic public
        // Mat4::orthographic_rh rejects that to protect ordinary scene cameras.
        let projection = Mat4 {
            cols: [
                [1. / half, 0., 0., 0.],
                [0., 1. / half, 0., 0.],
                [0., 0., 1. / (near - far), 0.],
                [0., 0., near / (near - far), 1.],
            ],
        };
        let camera =
            projection * rotate * Mat4::from_translation(Vec3::new(-cx * 3., 0., -cy * 3.));
        Ok(SourceThumbnailPlan {
            request: ThumbnailRequest {
                width: size,
                height: size,
                clip_from_world: camera,
                clear: [0; 4],
            },
            center_tile: center,
            buildable_bounds: bounds,
            width: self.width,
            height: self.height,
            fine_area: self.fine_area.clone(),
        })
    }
    pub fn room_topology(&self) -> Result<SourceRooms> {
        self.validate()?;
        if let Some(rooms) = &self.rooms {
            return Ok(rooms.clone());
        }
        reconstruct_rooms(self)
    }
    pub fn facade_request(&self, options: SourceFacadeOptions) -> Result<FacadeRequest> {
        self.validate_options(&options)?;
        let rooms = self.room_topology()?;
        self.facade_from_rooms(&rooms, &options)
    }
    fn validate_options(&self, options: &SourceFacadeOptions) -> Result<()> {
        self.validate()?;
        if options.floor_tiles == 0
            || options.floor_tiles > 256
            || options.floor_resolution_per_tile == 0
            || options.ground_subdivisions == 0
            || options.ground_subdivisions > 64
        {
            return Err(DerivativeError::Limit("source facade options"));
        }
        Ok(())
    }
    fn facade_from_rooms(
        &self,
        rooms: &SourceRooms,
        options: &SourceFacadeOptions,
    ) -> Result<FacadeRequest> {
        let mut walls = Vec::new();
        let area = self.width as usize * self.height as usize;
        for room in &rooms.rooms {
            if room.base != room.id || !room.is_outside {
                continue;
            }
            for &points in room.wall_lines.iter().chain(&room.fence_lines) {
                let a = points[0];
                let b = points[1];
                let dx = (b[0] - a[0]) as f32;
                let dy = (b[1] - a[1]) as f32;
                let length = (dx * dx + dy * dy).sqrt();
                let cx = (a[0] + b[0]) as f32 / 32.;
                let cy = (a[1] + b[1]) as f32 / 32.;
                let tx = (cx - dy / length * 0.6) as i32;
                let ty = (cy + dx / length * 0.6) as i32;
                let mut outside = OutsideSide::Left;
                if tx >= 0 && ty >= 0 && tx < self.width as i32 && ty < self.height as i32 {
                    let id = (rooms.map[room.floor as usize * area
                        + ty as usize * self.width as usize
                        + tx as usize]
                        & 0xffff) as usize;
                    if !rooms.rooms[rooms.rooms[id].base as usize].is_outside {
                        outside = OutsideSide::Right;
                    }
                }
                let mut hash = Sha256::new();
                hash.update(b"source-exterior-room-v1\0");
                hash.update(room.id.to_le_bytes());
                hash.update(room.base.to_le_bytes());
                hash.update([room.floor, u8::from(room.is_outside)]);
                walls.push(FacadeWall {
                    points,
                    floor: room.floor,
                    terrain_height: self.altitude_at(cx, cy),
                    outside,
                    room_provenance: AssetKey(hash.finalize().into()),
                });
            }
        }
        Ok(FacadeRequest {
            lot_width: self.width,
            lot_height: self.height,
            floor_tiles: options.floor_tiles,
            floor_resolution_per_tile: options.floor_resolution_per_tile,
            stories: self.stories,
            floors_used: self.floors_used()?,
            roof_on_floor: true,
            walls,
            thumbnail: options
                .thumbnail
                .map(|m| self.thumbnail_plan(m).map(|p| p.request))
                .transpose()?,
        })
    }
    /// PreparedWorld supplies the actual full-height walls/roofs and explicit
    /// day/night material states. This adapter changes no simulation state.
    pub fn prepare(
        &self,
        mut input: DerivativeInput,
        options: SourceFacadeOptions,
        limits: DerivativeRenderLimits,
    ) -> Result<PreparedSourceDerivative> {
        self.validate_options(&options)?;
        self.preflight(&input, &options, limits)?;
        let rooms = self.room_topology()?;
        let facade = self.facade_from_rooms(&rooms, &options)?;
        let thumbnail = options
            .thumbnail
            .map(|m| self.thumbnail_plan(m))
            .transpose()?;
        input.source_provenance = self.source_digest(input.source_provenance, &options);
        input.output = DerivativeOutput::Facade(facade);
        let mut request = PreparedDerivative::new(input, limits)?;
        let geometry = self.geometry(&request, &rooms, &options, limits)?;
        let bytes = geometry.resident_bytes();
        if request
            .input_bytes
            .checked_add(bytes)
            .filter(|n| *n <= limits.max_input_bytes)
            .is_none()
            || request
                .output_bytes
                .checked_add(bytes)
                .filter(|n| *n <= limits.max_output_bytes)
                .is_none()
        {
            return Err(DerivativeError::Limit("source geometry bytes"));
        }
        request.reservation_bytes = request
            .reservation_bytes
            .checked_add(
                bytes
                    .checked_mul(2)
                    .ok_or(DerivativeError::Limit("source geometry bytes"))?,
            )
            .ok_or(DerivativeError::Limit("source geometry bytes"))?;
        request.facade_geometry = Some(geometry);
        if let Some(plan) = &thumbnail {
            filter_thumbnail(&mut request, plan, &options.draw_tiles)?;
        }
        Ok(PreparedSourceDerivative { request, thumbnail })
    }
    pub fn prepare_thumbnail(
        &self,
        mut input: DerivativeInput,
        mode: SourceThumbnailMode,
        draw_tiles: Vec<Option<[u16; 3]>>,
        limits: DerivativeRenderLimits,
    ) -> Result<PreparedDerivative> {
        let options = SourceFacadeOptions {
            thumbnail: Some(mode),
            draw_tiles,
            ..SourceFacadeOptions::default()
        };
        self.preflight(&input, &options, limits)?;
        let plan = self.thumbnail_plan(mode)?;
        input.source_provenance = self.source_digest(input.source_provenance, &options);
        input.output = DerivativeOutput::Thumbnail(plan.request);
        let mut prepared = PreparedDerivative::new(input, limits)?;
        filter_thumbnail(&mut prepared, &plan, &options.draw_tiles)?;
        Ok(prepared)
    }
    fn preflight(
        &self,
        input: &DerivativeInput,
        options: &SourceFacadeOptions,
        limits: DerivativeRenderLimits,
    ) -> Result<()> {
        self.validate()?;
        if !options.draw_tiles.is_empty() && options.draw_tiles.len() != input.draws.len() {
            return Err(DerivativeError::Invalid("source draw tiles"));
        }
        for p in options.draw_tiles.iter().flatten() {
            if p[0] >= self.width || p[1] >= self.height || p[2] == 0 || p[2] > self.stories as u16
            {
                return Err(DerivativeError::Invalid("source draw tile"));
            }
        }
        // Caller-owned source arrays plus bounded connectivity scratch. These
        // temporaries are released before the request enters the async queue.
        let mut source_bytes = self.tiles.capacity() as u64
            * std::mem::size_of::<SourceTile>() as u64
            + self.altitude.capacity() as u64 * 2
            + self.tiles.len() as u64 * 64;
        if let Some(rooms) = &self.rooms {
            source_bytes += rooms.map.capacity() as u64 * 4
                + rooms.rooms.capacity() as u64 * std::mem::size_of::<SourceRoom>() as u64;
            for room in &rooms.rooms {
                source_bytes +=
                    (room.wall_lines.capacity() + room.fence_lines.capacity()) as u64 * 16;
            }
        }
        if let Some(fine) = &self.fine_area {
            source_bytes += fine.capacity() as u64;
        }
        if source_bytes > limits.max_input_bytes {
            return Err(DerivativeError::Limit("source preparation bytes"));
        }
        Ok(())
    }
    fn source_digest(&self, previous: AssetKey, options: &SourceFacadeOptions) -> AssetKey {
        let mut h = InputHash(Sha256::new());
        h.bytes(b"source-world-derivative-v1\0");
        h.bytes(&previous.0);
        h.u64(self.width as u64);
        h.u64(self.height as u64);
        h.u8(self.stories);
        h.bytes(&self.base_alt.to_le_bytes());
        h.u64(self.altitude.len() as u64);
        for value in &self.altitude {
            h.bytes(&value.to_le_bytes());
        }
        h.u64(self.tiles.len() as u64);
        for tile in &self.tiles {
            h.bytes(&tile.floor_pattern.to_le_bytes());
            for &w in tile.walls.iter().chain(&tile.fences) {
                h.u8(u8::from(w));
            }
            h.u8(match tile.diagonal {
                None => 0,
                Some(SourceDiagonal::Vertical) => 1,
                Some(SourceDiagonal::Horizontal) => 2,
            });
            h.u8(match tile.indoors {
                None => 0,
                Some(false) => 1,
                Some(true) => 2,
            });
        }
        h.u8(u8::from(self.rooms.is_some()));
        if let Some(rooms) = &self.rooms {
            h.u64(rooms.rooms.len() as u64);
            for room in &rooms.rooms {
                h.bytes(&room.id.to_le_bytes());
                h.bytes(&room.base.to_le_bytes());
                h.u8(room.floor);
                h.u8(u8::from(room.is_outside));
                for lines in [&room.wall_lines, &room.fence_lines] {
                    h.u64(lines.len() as u64);
                    for line in lines {
                        for p in line {
                            for v in p {
                                h.bytes(&v.to_le_bytes());
                            }
                        }
                    }
                }
            }
            h.u64(rooms.map.len() as u64);
            for id in &rooms.map {
                h.bytes(&id.to_le_bytes());
            }
        }
        h.u8(u8::from(self.fine_area.is_some()));
        if let Some(fine) = &self.fine_area {
            for &tile in fine {
                h.u8(u8::from(tile));
            }
        }
        h.u64(options.floor_tiles as u64);
        h.u64(options.floor_resolution_per_tile as u64);
        h.u64(options.ground_subdivisions as u64);
        h.u8(match options.thumbnail {
            None => 0,
            Some(SourceThumbnailMode::Tso) => 1,
            Some(SourceThumbnailMode::Ts1) => 2,
        });
        h.u64(options.draw_tiles.len() as u64);
        for tile in &options.draw_tiles {
            h.u8(u8::from(tile.is_some()));
            if let Some(tile) = tile {
                for &v in tile {
                    h.u64(v as u64);
                }
            }
        }
        AssetKey(h.0.finalize().into())
    }
    fn geometry(
        &self,
        request: &PreparedDerivative,
        rooms: &SourceRooms,
        options: &SourceFacadeOptions,
        limits: DerivativeRenderLimits,
    ) -> Result<FacadeGeometry> {
        let facade = match &request.input.output {
            DerivativeOutput::Facade(f) => f,
            _ => return Err(DerivativeError::Invalid("source facade")),
        };
        let n = options.ground_subdivisions as usize;
        let count = (n + 1) * (n + 1);
        let copies = facade.floors_used as usize + 1;
        let roof_vertices = request
            .input
            .draws
            .iter()
            .filter(|d| matches!(d.layer, DrawLayer::Roof(_)))
            .map(|d| d.mesh.vertices.len())
            .sum::<usize>();
        let roof_indices = request
            .input
            .draws
            .iter()
            .filter(|d| matches!(d.layer, DrawLayer::Roof(_)))
            .map(|d| d.mesh.indices.len())
            .sum::<usize>();
        let vertices = count * copies + facade.walls.len() * 4 + roof_vertices;
        let indices = n * n * 6 * copies + facade.walls.len() * 6 + roof_indices;
        if vertices > limits.render.max_vertices
            || indices > limits.render.max_indices
            || (vertices as u64 * 32 + indices as u64 * 4) * 2 > limits.max_output_bytes
        {
            return Err(DerivativeError::Limit("source geometry"));
        }
        let mut result = FacadeGeometry::default();
        result
            .floor
            .vertices
            .try_reserve_exact(count * copies + roof_vertices)
            .map_err(|_| DerivativeError::Allocation)?;
        result
            .floor
            .indices
            .try_reserve_exact(n * n * 6 * copies + roof_indices)
            .map_err(|_| DerivativeError::Allocation)?;
        let area = self.width as usize * self.height as usize;
        let partition = 255 / (self.stories as u16 + 1);
        let mut indoors = vec![0u8; area];
        for level in 0..self.stories as usize {
            for (i, value) in indoors.iter_mut().enumerate() {
                let tile = level * area + i;
                if self.tiles[tile].floor_pattern > 0 {
                    *value = (partition * level as u16) as u8;
                }
                let map = rooms.map[tile];
                for id in [map & 0xffff, map >> 16] {
                    if id > 0 && !rooms.rooms[id as usize].is_outside {
                        *value = (partition * (level as u16 + 1)) as u8;
                    }
                }
            }
        }
        let size = options.floor_tiles as f32;
        let step = size / n as f32;
        let bx = (self.width as f32 - size) / 2.;
        let by = (self.height as f32 - size) / 2.;
        let mut ground = FsofMesh::default();
        ground
            .vertices
            .try_reserve_exact(count)
            .map_err(|_| DerivativeError::Allocation)?;
        ground
            .indices
            .try_reserve_exact(n * n * 6)
            .map_err(|_| DerivativeError::Allocation)?;
        for y in 0..=n {
            for x in 0..=n {
                let x0 = round_even(((x as f32 - 1.) * step + bx) as f64).max(0.) as usize;
                let y0 = round_even(((y as f32 - 1.) * step + by) as f64).max(0.) as usize;
                let x1 = round_even(((x as f32 + 1.) * step + bx) as f64)
                    .min(self.width as f64)
                    .max(0.) as usize;
                let y1 = round_even(((y as f32 + 1.) * step + by) as f64)
                    .min(self.height as f64)
                    .max(0.) as usize;
                let mut total = 0.;
                let mut samples = 0usize;
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        if indoors[yy * self.width as usize + xx] > 0 {
                            total += self.altitude_at(xx as f32, yy as f32);
                            samples += 1;
                        }
                    }
                }
                let px = bx + x as f32 * step;
                let py = by + y as f32 * step;
                let height = if samples > 0 {
                    total / samples as f32
                } else {
                    self.altitude_at(px, py)
                };
                ground.vertices.push(FsofVertex {
                    position: Vec3::new(px, height, py),
                    uv: Vec2::new(
                        1. / 3. - x as f32 / n as f32 / 3.,
                        0.5 - y as f32 / n as f32 / 2.,
                    ),
                    normal: Vec3::ZERO,
                });
            }
        }
        for y in 0..n {
            for x in 0..n {
                let b = (x + y * (n + 1)) as u32;
                let row = (n + 1) as u32;
                ground
                    .indices
                    .extend([b + 1 + row, b + 1, b, b, b + row, b + 1 + row]);
            }
        }
        for level in 0..facade.floors_used {
            append_ground(&mut result.floor, &ground, level as f32 * 2.95, level);
            if level == 0 {
                append_ground(&mut result.floor, &ground, 0.5 * 2.95, 5);
            }
        }
        for draw in &request.input.draws {
            if let DrawLayer::Roof(level) = draw.layer {
                let layer = (level as u16 + 1).min(4);
                let base = result.floor.vertices.len() as u32;
                let tc = Vec2::new(((layer % 3) + 1) as f32 / 3., ((layer / 3) + 1) as f32 / 2.);
                for vertex in &draw.mesh.vertices {
                    let p = draw.model.transform_point3(vertex.position);
                    result.floor.vertices.push(FsofVertex {
                        position: p / 3.,
                        uv: Vec2::new(
                            tc.x - (p.x - bx * 3.) / (size * 9.),
                            tc.y - (p.z - by * 3.) / (size * 6.),
                        ),
                        normal: Vec3::ZERO,
                    });
                }
                result
                    .floor
                    .indices
                    .extend(draw.mesh.indices.iter().map(|i| i + base));
            }
        }
        let wall_height = request
            .image_specs
            .iter()
            .find(|(role, _, _)| *role == ImageRole::WallDay)
            .ok_or(DerivativeError::Invalid("wall atlas"))?
            .2 as f32;
        result
            .wall
            .vertices
            .try_reserve_exact(facade.walls.len() * 4)
            .map_err(|_| DerivativeError::Allocation)?;
        result
            .wall
            .indices
            .try_reserve_exact(facade.walls.len() * 6)
            .map_err(|_| DerivativeError::Allocation)?;
        for plan in &request.plans {
            if let RegionRole::Wall(index) = plan.region.role {
                let wall = &facade.walls[index as usize];
                let [x, y, w, h] = plan.region.rect;
                let bottom = wall.floor as f32 * 2.95 + wall.terrain_height;
                let top = bottom + 2.95;
                let base = result.wall.vertices.len() as u32;
                for (p, yy, u, v) in [
                    (wall.points[0], top, x, y),
                    (wall.points[1], top, x + w, y),
                    (wall.points[1], bottom, x + w, y + h),
                    (wall.points[0], bottom, x, y + h),
                ] {
                    result.wall.vertices.push(FsofVertex {
                        position: Vec3::new(p[0] as f32 / 16., yy, p[1] as f32 / 16.),
                        uv: Vec2::new(u as f32 / 512., v as f32 / wall_height),
                        normal: Vec3::ZERO,
                    });
                }
                result
                    .wall
                    .indices
                    .extend([base + 2, base + 1, base, base, base + 3, base + 2]);
            }
        }
        generate_normals(&mut result.floor);
        generate_normals(&mut result.wall);
        result.validate(FsofLimits {
            max_vertices: limits.render.max_vertices,
            max_indices: limits.render.max_indices,
            max_decoded_bytes: limits.max_output_bytes,
            ..FsofLimits::default()
        })?;
        Ok(result)
    }
}
fn append_ground(out: &mut FsofMesh, ground: &FsofMesh, height: f32, cell: u8) {
    let base = out.vertices.len() as u32;
    let u = (cell % 3) as f32 / 3.;
    let v = (cell / 3) as f32 / 2.;
    out.vertices
        .extend(ground.vertices.iter().map(|p| FsofVertex {
            position: p.position + Vec3::new(0., height, 0.),
            uv: Vec2::new(p.uv.x + u, p.uv.y + v),
            normal: Vec3::ZERO,
        }));
    out.indices.extend(ground.indices.iter().map(|i| i + base));
}
fn generate_normals(mesh: &mut FsofMesh) {
    for tri in mesh.indices.chunks_exact(3) {
        let a = mesh.vertices[tri[0] as usize].position;
        let b = mesh.vertices[tri[1] as usize].position;
        let c = mesh.vertices[tri[2] as usize].position;
        let cross = (b - a).cross(c - b);
        for &i in tri {
            mesh.vertices[i as usize].normal = mesh.vertices[i as usize].normal + cross;
        }
    }
    for vertex in &mut mesh.vertices {
        vertex.normal = vertex.normal.normalize_or_zero();
    }
}

fn filter_thumbnail(
    request: &mut PreparedDerivative,
    plan: &SourceThumbnailPlan,
    tiles: &[Option<[u16; 3]>],
) -> Result<()> {
    let mut allowed = vec![true; request.input.draws.len()];
    for (index, draw) in request.input.draws.iter().enumerate() {
        match draw.layer {
            DrawLayer::Floor(_) | DrawLayer::GroundMask => {
                let tile = tiles.get(index).copied().flatten();
                let (x, y) = if let Some(tile) = tile {
                    (tile[0] as i32, tile[1] as i32)
                } else {
                    let mut lo = Vec3::new(f32::INFINITY, 0., f32::INFINITY);
                    let mut hi = Vec3::new(f32::NEG_INFINITY, 0., f32::NEG_INFINITY);
                    for vertex in &draw.mesh.vertices {
                        let p = draw.model.transform_point3(vertex.position);
                        lo.x = lo.x.min(p.x);
                        lo.z = lo.z.min(p.z);
                        hi.x = hi.x.max(p.x);
                        hi.z = hi.z.max(p.z);
                    }
                    if hi.x - lo.x > 3.001 || hi.z - lo.z > 3.001 {
                        return Err(DerivativeError::Invalid(
                            "source thumbnail tile placement required",
                        ));
                    }
                    (
                        (lo.x / 3. + 0.0001).floor() as i32,
                        (lo.z / 3. + 0.0001).floor() as i32,
                    )
                };
                allowed[index] = x >= 0
                    && y >= 0
                    && x <= u16::MAX as i32
                    && y <= u16::MAX as i32
                    && plan.contains_tile(x as u16, y as u16);
            }
            DrawLayer::Object(_) if plan.fine_area.is_some() => {
                let model = draw
                    .owner
                    .and_then(|owner| {
                        request
                            .input
                            .frame
                            .entities
                            .iter()
                            .find(|e| e.reference == owner)
                    })
                    .map(|e| e.transform.matrix() * draw.model)
                    .unwrap_or(draw.model);
                let p = model.transform_point3(Vec3::ZERO) / 3.;
                allowed[index] = p.x >= 0.
                    && p.z >= 0.
                    && p.x < plan.width as f32
                    && p.z < plan.height as f32
                    && plan.contains_tile(p.x as u16, p.z as u16);
            }
            _ => {}
        }
    }
    for view in &mut request.plans {
        if view.region.role == RegionRole::Thumbnail {
            view.commands.retain(|(index, _)| allowed[*index]);
        }
    }
    Ok(())
}

// Two cells per tile preserve both source diagonal room identities.
fn side_half(tile: &SourceTile, side: usize) -> usize {
    match tile.diagonal {
        None => 0,
        Some(SourceDiagonal::Vertical) => usize::from(side == 1 || side == 3),
        Some(SourceDiagonal::Horizontal) => usize::from(side == 2 || side == 3),
    }
}
fn root(parent: &mut [usize], mut at: usize) -> usize {
    while parent[at] != at {
        parent[at] = parent[parent[at]];
        at = parent[at];
    }
    at
}
fn union(parent: &mut [usize], a: usize, b: usize) {
    let a = root(parent, a);
    let b = root(parent, b);
    if a != b {
        parent[a.max(b)] = a.min(b);
    }
}
fn reconstruct_rooms(world: &SourceWorld) -> Result<SourceRooms> {
    let w = world.width as usize;
    let h = world.height as usize;
    let area = w * h;
    let mut rooms = vec![SourceRoom {
        id: 0,
        base: 0,
        floor: 0,
        is_outside: true,
        wall_lines: vec![],
        fence_lines: vec![],
    }];
    let mut map = vec![0u32; world.tiles.len()];
    for level in 0..world.stories as usize {
        let tiles = &world.tiles[level * area..(level + 1) * area];
        let mut parent: Vec<usize> = (0..area * 2).collect();
        for (i, tile) in tiles.iter().enumerate() {
            if tile.diagonal.is_none() {
                union(&mut parent, i * 2, i * 2 + 1);
            }
            let x = i % w;
            let y = i / w;
            if x + 1 < w && !tile.walls[3] && !tiles[i + 1].walls[0] {
                union(
                    &mut parent,
                    i * 2 + side_half(tile, 3),
                    (i + 1) * 2 + side_half(&tiles[i + 1], 0),
                );
            }
            if y + 1 < h && !tile.walls[2] && !tiles[i + w].walls[1] {
                union(
                    &mut parent,
                    i * 2 + side_half(tile, 2),
                    (i + w) * 2 + side_half(&tiles[i + w], 1),
                );
            }
        }
        let mut states = BTreeMap::<usize, (bool, bool)>::new();
        for (i, tile) in tiles.iter().enumerate() {
            let x = i % w;
            let y = i / w;
            for half in 0..2 {
                let r = root(&mut parent, i * 2 + half);
                let state = states.entry(r).or_default();
                if tile.indoors == Some(false) {
                    state.0 = true;
                }
                if tile.indoors == Some(true) {
                    state.1 = true;
                }
                for (side, boundary) in [x == 0, y == 0, y + 1 == h, x + 1 == w]
                    .into_iter()
                    .enumerate()
                {
                    if boundary && side_half(tile, side) == half && !tile.walls[side] {
                        state.0 = true;
                    }
                }
            }
        }
        let mut ids = BTreeMap::new();
        for (r, (outside, inside)) in states {
            if outside && inside {
                return Err(DerivativeError::Invalid("source indoor topology conflict"));
            }
            let id =
                u16::try_from(rooms.len()).map_err(|_| DerivativeError::Limit("source rooms"))?;
            ids.insert(r, id);
            rooms.push(SourceRoom {
                id,
                base: id,
                floor: level as u8,
                is_outside: outside,
                wall_lines: vec![],
                fence_lines: vec![],
            });
        }
        let mut half_ids = vec![0u16; area * 2];
        for (i, id) in half_ids.iter_mut().enumerate() {
            *id = ids[&root(&mut parent, i)];
        }
        for i in 0..area {
            map[level * area + i] = half_ids[i * 2] as u32
                | if tiles[i].diagonal.is_some() {
                    (half_ids[i * 2 + 1] as u32) << 16
                } else {
                    0
                };
        }
        let mut lines = BTreeSet::<(u16, bool, SourceLine)>::new();
        for (i, tile) in tiles.iter().enumerate() {
            let x = i % w;
            let y = i / w;
            let xx = x as i32 * 16;
            let yy = y as i32 * 16;
            for side in 0..4 {
                if !tile.walls[side] && !tile.fences[side] {
                    continue;
                }
                let points = match side {
                    0 => [[xx, yy], [xx, yy + 16]],
                    1 => [[xx, yy], [xx + 16, yy]],
                    2 => [[xx, yy + 16], [xx + 16, yy + 16]],
                    _ => [[xx + 16, yy], [xx + 16, yy + 16]],
                };
                let local = half_ids[i * 2 + side_half(tile, side)];
                let other = match side {
                    0 if x > 0 => Some((i - 1, 3)),
                    1 if y > 0 => Some((i - w, 2)),
                    2 if y + 1 < h => Some((i + w, 1)),
                    3 if x + 1 < w => Some((i + 1, 0)),
                    _ => None,
                };
                let other = other.map(|(j, s)| half_ids[j * 2 + side_half(&tiles[j], s)]);
                let room = if rooms[local as usize].is_outside {
                    Some(local)
                } else {
                    other.filter(|id| rooms[*id as usize].is_outside)
                };
                if let Some(room) = room {
                    lines.insert((room, tile.fences[side] && !tile.walls[side], points));
                }
            }
            if let Some(diagonal) = tile.diagonal {
                let a = half_ids[i * 2];
                let b = half_ids[i * 2 + 1];
                let room = if rooms[a as usize].is_outside {
                    Some(a)
                } else if rooms[b as usize].is_outside {
                    Some(b)
                } else {
                    None
                };
                if let Some(room) = room {
                    let points = match diagonal {
                        SourceDiagonal::Vertical => [[xx, yy], [xx + 16, yy + 16]],
                        SourceDiagonal::Horizontal => [[xx, yy + 16], [xx + 16, yy]],
                    };
                    lines.insert((room, false, points));
                }
            }
        }
        for (room, fence, line) in lines {
            if fence {
                rooms[room as usize].fence_lines.push(line);
            } else {
                rooms[room as usize].wall_lines.push(line);
            }
        }
    }
    for room in &mut rooms {
        merge_lines(&mut room.wall_lines);
        merge_lines(&mut room.fence_lines);
    }
    Ok(SourceRooms { rooms, map })
}
fn merge_lines(lines: &mut Vec<SourceLine>) {
    // Group by direction/intercept; source tile segments are unit cardinal or
    // diagonal edges. Sorting avoids quadratic endpoint searches on large lots.
    let key = |line: &SourceLine| {
        let dx = (line[1][0] - line[0][0]).signum();
        let dy = (line[1][1] - line[0][1]).signum();
        (dx, dy, dy * line[0][0] - dx * line[0][1], line[0])
    };
    lines.sort_by_key(key);
    let mut out: Vec<SourceLine> = Vec::with_capacity(lines.len());
    for &line in lines.iter() {
        if let Some(last) = out.last_mut() {
            if last[1] == line[0]
                && (last[1][0] - last[0][0]) * (line[1][1] - line[0][1])
                    == (last[1][1] - last[0][1]) * (line[1][0] - line[0][0])
            {
                last[1] = line[1];
                continue;
            }
        }
        out.push(line);
    }
    *lines = out;
}
