use super::{
    invalidation::{ArchitectureRevision, DirtyRegions},
    rooms::{Portal, PortalId, PortalTraversal, RoomCell, RoomId, RoomMap},
    tiles::{BuildBounds, Cardinal, LotPosition, Tile, TilePos, WallTile},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_LOT_DIMENSION: u16 = 256;
pub const MAX_LOT_LEVELS: u8 = 16;
pub const MAX_LOT_TILES: usize = 262_144;
pub const MAX_PORTALS: usize = 8_192;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LotError {
    InvalidDimensions,
    OutOfBounds,
    InvalidWall,
    InvalidBuildBounds,
    InvalidPortal,
    PortalLimit,
    RevisionExhausted,
    InvalidState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortalConnection {
    pub from: RoomId,
    pub to: RoomId,
    pub traversal: PortalTraversal,
}

/// Headless semantic architecture. Edits are trusted geometry operations; user build policy lives in build.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotModel {
    width: u16,
    height: u16,
    levels: u8,
    tiles: Vec<Tile>,
    terrain_vertices: Vec<i16>,
    grass: Vec<u8>,
    build_bounds: BuildBounds,
    fine_buildable: Option<BTreeSet<(i16, i16)>>,
    revision: ArchitectureRevision,
    dirty: DirtyRegions,
    room_map: RoomMap,
    portals: BTreeMap<PortalId, Portal>,
}

impl LotModel {
    pub fn new(width: u16, height: u16, levels: u8) -> Result<Self, LotError> {
        if width == 0
            || height == 0
            || levels == 0
            || width > MAX_LOT_DIMENSION
            || height > MAX_LOT_DIMENSION
            || levels > MAX_LOT_LEVELS
        {
            return Err(LotError::InvalidDimensions);
        }
        // Check each dimension first: hostile u16/u8 maxima overflow usize on wasm32.
        let len = usize::from(width) * usize::from(height) * usize::from(levels);
        if len > MAX_LOT_TILES {
            return Err(LotError::InvalidDimensions);
        }
        let mut result = Self {
            width,
            height,
            levels,
            tiles: vec![Tile::default(); len],
            terrain_vertices: vec![0; (usize::from(width) + 1) * (usize::from(height) + 1)],
            grass: vec![0; usize::from(width) * usize::from(height)],
            build_bounds: BuildBounds {
                min_x: 0,
                min_y: 0,
                max_x: width as i16,
                max_y: height as i16,
                max_level: levels,
            },
            fine_buildable: None,
            revision: ArchitectureRevision::default(),
            dirty: DirtyRegions::default(),
            room_map: RoomMap::default(),
            portals: BTreeMap::new(),
        };
        result.room_map = RoomMap::regenerate(&result);
        for tile in &mut result.tiles[..usize::from(width) * usize::from(height)] {
            tile.supported = true;
        }
        Ok(result)
    }
    pub fn width(&self) -> u16 {
        self.width
    }
    pub fn height(&self) -> u16 {
        self.height
    }
    pub fn levels(&self) -> u8 {
        self.levels
    }
    pub fn revision(&self) -> ArchitectureRevision {
        self.revision
    }
    pub fn dirty(&self) -> &DirtyRegions {
        &self.dirty
    }
    pub fn take_dirty(&mut self) -> DirtyRegions {
        std::mem::take(&mut self.dirty)
    }
    pub fn rooms(&self) -> &RoomMap {
        &self.room_map
    }
    pub fn portals(&self) -> &BTreeMap<PortalId, Portal> {
        &self.portals
    }
    pub fn build_bounds(&self) -> BuildBounds {
        self.build_bounds
    }
    pub fn contains(&self, position: TilePos) -> bool {
        position.x >= 0
            && position.y >= 0
            && position.x < self.width as i16
            && position.y < self.height as i16
            && position.level > 0
            && position.level <= self.levels
    }
    pub fn contains_position(&self, position: LotPosition) -> bool {
        position.tile().map_or(false, |p| self.contains(p))
    }
    pub fn buildable(&self, position: TilePos) -> bool {
        self.contains(position)
            && self.build_bounds.contains(position)
            && self
                .fine_buildable
                .as_ref()
                .map_or(true, |fine| fine.contains(&(position.x, position.y)))
    }
    fn index(&self, position: TilePos) -> Option<usize> {
        if !self.contains(position) {
            return None;
        }
        Some(
            (usize::from(position.level) - 1) * usize::from(self.width) * usize::from(self.height)
                + position.y as usize * usize::from(self.width)
                + position.x as usize,
        )
    }
    pub fn tile(&self, position: TilePos) -> Option<&Tile> {
        self.index(position).and_then(|i| self.tiles.get(i))
    }
    fn revision_available(&self) -> Result<(), LotError> {
        let r = self.revision;
        if [r.architecture, r.rooms, r.terrain, r.portals, r.support].contains(&u64::MAX) {
            Err(LotError::RevisionExhausted)
        } else {
            Ok(())
        }
    }
    pub fn room_at(&self, position: LotPosition) -> Option<RoomId> {
        let tile = position.tile()?;
        let data = self.tile(tile)?;
        self.room_map
            .cells
            .get(&RoomCell {
                tile,
                half: data.wall.diagonal.half_at(position.x, position.y),
            })
            .copied()
    }
    pub fn floor_at(&self, position: LotPosition) -> Option<u16> {
        let tile = self.tile(position.tile()?)?;
        Some(tile.floor_in_half(tile.wall.diagonal.half_at(position.x, position.y)))
    }
    pub fn edge_blocked(&self, tile: TilePos, direction: Cardinal) -> bool {
        self.tile(tile)
            .map_or(true, |t| t.wall.sides & direction.wall_bit() != 0)
            || tile
                .adjacent(direction)
                .and_then(|p| self.tile(p))
                .map_or(false, |t| {
                    t.wall.sides & direction.opposite().wall_bit() != 0
                })
    }
    pub fn edge_separates_room(&self, tile: TilePos, direction: Cardinal) -> bool {
        self.tile(tile)
            .map_or(true, |t| t.wall.room_separators & direction.wall_bit() != 0)
            || tile
                .adjacent(direction)
                .and_then(|p| self.tile(p))
                .map_or(false, |t| {
                    t.wall.room_separators & direction.opposite().wall_bit() != 0
                })
    }
    pub fn set_wall(&mut self, position: TilePos, wall: WallTile) -> Result<bool, LotError> {
        let index = self.index(position).ok_or(LotError::OutOfBounds)?;
        if !wall.valid() {
            return Err(LotError::InvalidWall);
        }
        if self.tiles[index].wall == wall {
            return Ok(false);
        }
        self.revision_available()?;
        self.tiles[index].wall = wall.clone();
        self.dirty.walls.insert(position);
        self.dirty.routing.insert(position);
        for edge in Cardinal::ALL {
            let Some(other) = position.adjacent(edge) else {
                continue;
            };
            let Some(index) = self.index(other) else {
                continue;
            };
            let target = &mut self.tiles[index].wall;
            let original = target.clone();
            let bit = edge.opposite().wall_bit();
            target.sides = (target.sides & !bit)
                | if wall.sides & edge.wall_bit() != 0 {
                    bit
                } else {
                    0
                };
            target.room_separators = (target.room_separators & !bit)
                | if wall.room_separators & edge.wall_bit() != 0 {
                    bit
                } else {
                    0
                };
            target.occupied = (target.occupied & !bit)
                | if wall.occupied & edge.wall_bit() != 0 {
                    bit
                } else {
                    0
                };
            if *target != original {
                self.dirty.walls.insert(other);
                self.dirty.routing.insert(other);
            }
        }
        self.revision.architecture += 1;
        self.recompute_rooms_and_support();
        Ok(true)
    }
    pub fn set_floor(&mut self, position: TilePos, pattern: u16) -> Result<bool, LotError> {
        let index = self.index(position).ok_or(LotError::OutOfBounds)?;
        if self.tiles[index].floor == pattern {
            return Ok(false);
        }
        self.revision_available()?;
        self.tiles[index].floor = pattern;
        self.dirty.floors.insert(position);
        self.dirty.routing.insert(position);
        self.revision.architecture += 1;
        self.recompute_rooms_and_support();
        Ok(true)
    }
    pub fn set_object_support(
        &mut self,
        position: TilePos,
        support: bool,
    ) -> Result<bool, LotError> {
        let index = self.index(position).ok_or(LotError::OutOfBounds)?;
        if self.tiles[index].object_support == support {
            return Ok(false);
        }
        self.revision_available()?;
        self.tiles[index].object_support = support;
        self.revision.architecture += 1;
        self.dirty.support.insert(position);
        self.recompute_support();
        Ok(true)
    }
    pub fn set_build_bounds(
        &mut self,
        bounds: BuildBounds,
        fine: Option<BTreeSet<(i16, i16)>>,
    ) -> Result<bool, LotError> {
        if bounds.min_x < 0
            || bounds.min_y < 0
            || bounds.max_x > self.width as i16
            || bounds.max_y > self.height as i16
            || bounds.min_x >= bounds.max_x
            || bounds.min_y >= bounds.max_y
            || bounds.max_level == 0
            || bounds.max_level > self.levels
            || fine.as_ref().map_or(false, |f| {
                f.iter().any(|&(x, y)| {
                    x < 0 || y < 0 || x >= self.width as i16 || y >= self.height as i16
                })
            })
        {
            return Err(LotError::InvalidBuildBounds);
        }
        if self.build_bounds == bounds && self.fine_buildable == fine {
            return Ok(false);
        }
        self.revision_available()?;
        let positions = self.positions();
        let previous: BTreeSet<_> = positions
            .iter()
            .filter(|&&p| self.buildable(p))
            .copied()
            .collect();
        self.build_bounds = bounds;
        self.fine_buildable = fine;
        for position in positions {
            if previous.contains(&position) != self.buildable(position) {
                self.dirty.build_bounds.insert(position);
            }
        }
        self.revision.architecture += 1;
        Ok(true)
    }
    pub fn terrain_vertex(&self, x: u16, y: u16) -> Option<i16> {
        if x > self.width || y > self.height {
            return None;
        }
        self.terrain_vertices
            .get(usize::from(y) * (usize::from(self.width) + 1) + usize::from(x))
            .copied()
    }
    pub fn set_terrain_vertex(&mut self, x: u16, y: u16, height: i16) -> Result<bool, LotError> {
        if x > self.width || y > self.height {
            return Err(LotError::OutOfBounds);
        }
        let index = usize::from(y) * (usize::from(self.width) + 1) + usize::from(x);
        if self.terrain_vertices[index] == height {
            return Ok(false);
        }
        self.revision_available()?;
        self.terrain_vertices[index] = height;
        for dy in [-1_i16, 0] {
            for dx in [-1_i16, 0] {
                let p = TilePos::new(x as i16 + dx, y as i16 + dy, 1);
                if self.contains(p) {
                    self.dirty.terrain.insert(p);
                    self.dirty.routing.insert(p);
                }
            }
        }
        self.revision.architecture += 1;
        self.revision.terrain += 1;
        Ok(true)
    }
    pub fn terrain_corners(&self, tile: TilePos) -> Option<[i16; 4]> {
        if !self.contains(tile) {
            return None;
        }
        let (x, y) = (tile.x as u16, tile.y as u16);
        Some([
            self.terrain_vertex(x, y)?,
            self.terrain_vertex(x + 1, y)?,
            self.terrain_vertex(x, y + 1)?,
            self.terrain_vertex(x + 1, y + 1)?,
        ])
    }
    pub fn terrain_sloped(&self, tile: TilePos) -> bool {
        self.terrain_corners(tile)
            .map_or(false, |v| v.iter().any(|h| *h != v[0]))
    }
    pub fn set_grass(&mut self, x: i16, y: i16, grass: u8) -> Result<bool, LotError> {
        let position = TilePos::new(x, y, 1);
        let index = self.index(position).ok_or(LotError::OutOfBounds)?;
        if self.grass[index] == grass {
            return Ok(false);
        }
        self.revision_available()?;
        self.grass[index] = grass;
        self.dirty.terrain.insert(position);
        self.revision.terrain += 1;
        self.revision.architecture += 1;
        Ok(true)
    }
    pub fn upsert_portal(&mut self, mut portal: Portal) -> Result<bool, LotError> {
        if portal.id.0 == 0
            || portal.entity.object_id.0 <= 0
            || portal.entity.generation == 0
            || !self.contains_position(portal.entry)
            || !self.contains_position(portal.exit)
            || portal.entry == portal.exit
            || portal.cost == 0
        {
            return Err(LotError::InvalidPortal);
        }
        if let Some(old) = self.portals.get(&portal.id) {
            portal.revision = old.revision;
            if *old == portal {
                return Ok(false);
            }
        } else if self.portals.len() >= MAX_PORTALS {
            return Err(LotError::PortalLimit);
        }
        self.revision_available()?;
        if let Some(old) = self.portals.get(&portal.id) {
            self.dirty.routing.extend([
                old.entry.tile().expect("validated"),
                old.exit.tile().expect("validated"),
            ]);
        }
        self.revision.architecture += 1;
        self.revision.portals += 1;
        portal.revision = self.revision.portals;
        self.dirty.routing.extend([
            portal.entry.tile().expect("validated"),
            portal.exit.tile().expect("validated"),
        ]);
        self.portals.insert(portal.id, portal);
        Ok(true)
    }
    pub fn remove_portal(&mut self, id: PortalId) -> Result<bool, LotError> {
        if !self.portals.contains_key(&id) {
            return Ok(false);
        }
        self.revision_available()?;
        let old = self.portals.remove(&id).expect("checked portal");
        self.dirty.routing.extend([
            old.entry.tile().expect("validated"),
            old.exit.tile().expect("validated"),
        ]);
        self.revision.architecture += 1;
        self.revision.portals += 1;
        Ok(true)
    }
    pub fn portal_graph(&self) -> Vec<PortalConnection> {
        let mut result = Vec::new();
        for portal in self.portals.values().filter(|p| p.enabled) {
            if let (Some(from), Some(to)) = (self.room_at(portal.entry), self.room_at(portal.exit))
            {
                result.push(PortalConnection {
                    from,
                    to,
                    traversal: PortalTraversal {
                        id: portal.id,
                        reverse: false,
                    },
                });
                if portal.bidirectional {
                    result.push(PortalConnection {
                        from: to,
                        to: from,
                        traversal: PortalTraversal {
                            id: portal.id,
                            reverse: true,
                        },
                    });
                }
            }
        }
        result
    }
    pub fn positions(&self) -> Vec<TilePos> {
        let mut out = Vec::with_capacity(self.tiles.len());
        for level in 1..=self.levels {
            for y in 0..self.height {
                for x in 0..self.width {
                    out.push(TilePos::new(x as i16, y as i16, level));
                }
            }
        }
        out
    }
    fn recompute_rooms_and_support(&mut self) {
        let next = RoomMap::regenerate(self);
        if next != self.room_map {
            for (cell, id) in &next.cells {
                let previous = self.room_map.cells.get(cell);
                if previous != Some(id)
                    || previous.and_then(|i| self.room_map.rooms.get(i)) != next.rooms.get(id)
                {
                    self.dirty.rooms.insert(cell.tile);
                    self.dirty.routing.insert(cell.tile);
                }
            }
            self.room_map = next;
            self.revision.rooms += 1;
        }
        self.recompute_support();
    }
    fn recompute_support(&mut self) {
        let mut changed = false;
        for position in self.positions().into_iter().filter(|p| p.level > 1) {
            let below = TilePos {
                level: position.level - 1,
                ..position
            };
            let indoor_below = |p: TilePos| {
                self.room_map
                    .cells
                    .get(&RoomCell { tile: p, half: 0 })
                    .and_then(|r| self.room_map.rooms.get(r))
                    .map_or(false, |r| !r.outside)
            };
            let mut supported =
                self.tile(below).expect("bounded").object_support || indoor_below(below);
            if !supported {
                let near_floor = Cardinal::ALL
                    .iter()
                    .filter_map(|&d| position.adjacent(d))
                    .any(|p| self.tile(p).map_or(false, |t| t.floor != 0));
                if near_floor {
                    'search: for dy in -2_i16..=2 {
                        for dx in -2_i16..=2 {
                            let p = TilePos::new(below.x + dx, below.y + dy, below.level);
                            if let Some(tile) = self.tile(p) {
                                if indoor_below(p)
                                    || (tile.object_support && dx.abs() < 2 && dy.abs() < 2)
                                {
                                    supported = true;
                                    break 'search;
                                }
                            }
                        }
                    }
                }
            }
            let i = self.index(position).expect("bounded");
            if self.tiles[i].supported != supported {
                self.tiles[i].supported = supported;
                self.dirty.support.insert(position);
                changed = true;
            }
        }
        if changed {
            self.revision.support += 1;
        }
    }
    pub fn validate(&self) -> Result<(), LotError> {
        if self.width == 0
            || self.height == 0
            || self.levels == 0
            || self.width > MAX_LOT_DIMENSION
            || self.height > MAX_LOT_DIMENSION
            || self.levels > MAX_LOT_LEVELS
        {
            return Err(LotError::InvalidState);
        }
        let count = usize::from(self.width) * usize::from(self.height) * usize::from(self.levels);
        if count > MAX_LOT_TILES
            || self.tiles.len() != count
            || self.terrain_vertices.len()
                != (usize::from(self.width) + 1) * (usize::from(self.height) + 1)
            || self.grass.len() != usize::from(self.width) * usize::from(self.height)
            || self.tiles.iter().any(|t| !t.wall.valid())
            || self.portals.len() > MAX_PORTALS
        {
            return Err(LotError::InvalidState);
        }
        let b = self.build_bounds;
        if b.min_x < 0
            || b.min_y < 0
            || b.max_x > self.width as i16
            || b.max_y > self.height as i16
            || b.min_x >= b.max_x
            || b.min_y >= b.max_y
            || b.max_level == 0
            || b.max_level > self.levels
            || self.fine_buildable.as_ref().map_or(false, |f| {
                f.iter()
                    .any(|&(x, y)| !self.contains(TilePos::new(x, y, 1)))
            })
        {
            return Err(LotError::InvalidState);
        }
        if self.portals.iter().any(|(id, p)| {
            *id != p.id
                || id.0 == 0
                || p.entity.object_id.0 <= 0
                || p.entity.generation == 0
                || p.cost == 0
                || p.entry == p.exit
                || !self.contains_position(p.entry)
                || !self.contains_position(p.exit)
                || p.revision > self.revision.portals
        }) {
            return Err(LotError::InvalidState);
        }
        for set in [
            &self.dirty.walls,
            &self.dirty.floors,
            &self.dirty.terrain,
            &self.dirty.rooms,
            &self.dirty.support,
            &self.dirty.routing,
            &self.dirty.build_bounds,
        ] {
            if set.iter().any(|p| !self.contains(*p)) {
                return Err(LotError::InvalidState);
            }
        }
        for p in self.positions() {
            for edge in Cardinal::ALL {
                if let Some(other) = p.adjacent(edge).and_then(|p| self.tile(p)) {
                    let me = &self.tile(p).expect("bounded").wall;
                    let (a, b) = (edge.wall_bit(), edge.opposite().wall_bit());
                    if (me.sides & a != 0) != (other.wall.sides & b != 0)
                        || (me.room_separators & a != 0) != (other.wall.room_separators & b != 0)
                        || (me.occupied & a != 0) != (other.wall.occupied & b != 0)
                    {
                        return Err(LotError::InvalidState);
                    }
                }
            }
        }
        if RoomMap::regenerate(self) != self.room_map {
            return Err(LotError::InvalidState);
        }
        let mut check = self.clone();
        check.revision.support = 0;
        check.recompute_support();
        if check.tiles != self.tiles {
            return Err(LotError::InvalidState);
        }
        Ok(())
    }
}
