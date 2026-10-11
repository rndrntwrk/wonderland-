use super::tiles::TilePos;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureRevision {
    pub architecture: u64,
    pub rooms: u64,
    pub terrain: u64,
    pub portals: u64,
    pub support: u64,
}

/// Exact tile sets, not a single bounding rectangle spanning unrelated edits.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirtyRegions {
    pub walls: BTreeSet<TilePos>,
    pub floors: BTreeSet<TilePos>,
    pub terrain: BTreeSet<TilePos>,
    pub rooms: BTreeSet<TilePos>,
    pub support: BTreeSet<TilePos>,
    pub routing: BTreeSet<TilePos>,
    pub build_bounds: BTreeSet<TilePos>,
}

impl DirtyRegions {
    pub fn is_empty(&self) -> bool {
        self.walls.is_empty()
            && self.floors.is_empty()
            && self.terrain.is_empty()
            && self.rooms.is_empty()
            && self.support.is_empty()
            && self.routing.is_empty()
            && self.build_bounds.is_empty()
    }
    pub fn merge(&mut self, other: Self) {
        self.walls.extend(other.walls);
        self.floors.extend(other.floors);
        self.terrain.extend(other.terrain);
        self.rooms.extend(other.rooms);
        self.support.extend(other.support);
        self.routing.extend(other.routing);
        self.build_bounds.extend(other.build_bounds);
    }
}
