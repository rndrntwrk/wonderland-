use super::{
    lot::LotModel,
    tiles::{Cardinal, Diagonal, LotPosition, TilePos, FLOOR_POOL, FLOOR_WATER},
};
use crate::ids::EntityRef;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RoomId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RoomCell {
    pub tile: TilePos,
    pub half: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Room {
    pub id: RoomId,
    pub level: u8,
    pub outside: bool,
    pub is_pool: bool,
    pub is_water: bool,
    pub unroutable: bool,
    pub area_half_tiles: u32,
    /// Only fence/floor-category adjacency; solid walls are crossed through portals.
    pub adjacent: BTreeSet<RoomId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomMap {
    pub cells: BTreeMap<RoomCell, RoomId>,
    pub rooms: BTreeMap<RoomId, Room>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PortalId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Portal {
    pub id: PortalId,
    pub entity: EntityRef,
    pub entry: LotPosition,
    pub exit: LotPosition,
    pub bidirectional: bool,
    pub enabled: bool,
    pub cost: u32,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PortalTraversal {
    pub id: PortalId,
    pub reverse: bool,
}

impl Portal {
    pub fn endpoints(&self, reverse: bool) -> (LotPosition, LotPosition) {
        if reverse {
            (self.exit, self.entry)
        } else {
            (self.entry, self.exit)
        }
    }
}

fn category(floor: u16, level: u8) -> u8 {
    match floor {
        FLOOR_POOL => 2,
        FLOOR_WATER => 3,
        0 if level > 1 => 1,
        _ => 0,
    }
}

impl RoomMap {
    pub(crate) fn regenerate(lot: &LotModel) -> Self {
        let mut result = Self::default();
        let mut next_id = 1_u32;
        for level in 1..=lot.levels() {
            for y in 0..lot.height() {
                for x in 0..lot.width() {
                    let tile = TilePos::new(x as i16, y as i16, level);
                    for half in 0..2 {
                        let start = RoomCell { tile, half };
                        if result.cells.contains_key(&start) {
                            continue;
                        }
                        let id = RoomId(next_id);
                        next_id += 1;
                        let floor = lot.tile(tile).expect("bounded tile").floor_in_half(half);
                        let floor_category = category(floor, level);
                        let mut queue = VecDeque::from([start]);
                        result.cells.insert(start, id);
                        let mut outside = false;
                        let mut area = 0;
                        while let Some(cell) = queue.pop_front() {
                            area += 1;
                            let current = lot.tile(cell.tile).expect("bounded cell");
                            if current.wall.diagonal == Diagonal::None {
                                let other = RoomCell {
                                    tile: cell.tile,
                                    half: 1 - cell.half,
                                };
                                if let std::collections::btree_map::Entry::Vacant(e) =
                                    result.cells.entry(other)
                                {
                                    e.insert(id);
                                    queue.push_back(other);
                                }
                            }
                            for edge in Cardinal::ALL {
                                if current.wall.diagonal != Diagonal::None
                                    && current.wall.diagonal.half_on_edge(edge) != cell.half
                                {
                                    continue;
                                }
                                if lot.edge_blocked(cell.tile, edge) {
                                    continue;
                                }
                                let neighbor =
                                    cell.tile.adjacent(edge).filter(|p| lot.contains(*p));
                                let Some(neighbor) = neighbor else {
                                    outside = true;
                                    continue;
                                };
                                let other_tile = lot.tile(neighbor).expect("bounded neighbor");
                                let other_half =
                                    other_tile.wall.diagonal.half_on_edge(edge.opposite());
                                if category(other_tile.floor_in_half(other_half), level)
                                    != floor_category
                                {
                                    continue;
                                }
                                let next = RoomCell {
                                    tile: neighbor,
                                    half: other_half,
                                };
                                if let std::collections::btree_map::Entry::Vacant(entry) =
                                    result.cells.entry(next)
                                {
                                    entry.insert(id);
                                    queue.push_back(next);
                                }
                            }
                        }
                        result.rooms.insert(
                            id,
                            Room {
                                id,
                                level,
                                outside,
                                is_pool: floor == FLOOR_POOL,
                                is_water: floor == FLOOR_WATER,
                                unroutable: floor_category == 1,
                                area_half_tiles: area,
                                adjacent: BTreeSet::new(),
                            },
                        );
                    }
                }
            }
        }
        // Fences and differing floor categories partition room records but transmit outside status.
        let cells: Vec<_> = result.cells.keys().copied().collect();
        for cell in cells {
            let current = lot.tile(cell.tile).expect("bounded cell");
            let my_room = result.cells[&cell];
            if current.wall.diagonal != Diagonal::None && !current.wall.diagonal_solid {
                let other = result.cells[&RoomCell {
                    tile: cell.tile,
                    half: 1 - cell.half,
                }];
                result.link(my_room, other);
            }
            for edge in Cardinal::ALL {
                if current.wall.diagonal != Diagonal::None
                    && current.wall.diagonal.half_on_edge(edge) != cell.half
                {
                    continue;
                }
                if lot.edge_separates_room(cell.tile, edge) {
                    continue;
                }
                if let Some(neighbor) = cell.tile.adjacent(edge).filter(|p| lot.contains(*p)) {
                    let half = lot
                        .tile(neighbor)
                        .expect("bounded neighbor")
                        .wall
                        .diagonal
                        .half_on_edge(edge.opposite());
                    let other = result.cells[&RoomCell {
                        tile: neighbor,
                        half,
                    }];
                    result.link(my_room, other);
                } else {
                    result.rooms.get_mut(&my_room).expect("known room").outside = true;
                }
            }
        }
        let mut queue: VecDeque<_> = result
            .rooms
            .values()
            .filter(|r| r.outside)
            .map(|r| r.id)
            .collect();
        while let Some(id) = queue.pop_front() {
            let adjacent: Vec<_> = result.rooms[&id].adjacent.iter().copied().collect();
            for other in adjacent {
                let room = result.rooms.get_mut(&other).expect("known adjacency");
                if !room.outside {
                    room.outside = true;
                    queue.push_back(other);
                }
            }
        }
        result
    }
    fn link(&mut self, a: RoomId, b: RoomId) {
        if a != b {
            self.rooms
                .get_mut(&a)
                .expect("known room")
                .adjacent
                .insert(b);
            self.rooms
                .get_mut(&b)
                .expect("known room")
                .adjacent
                .insert(a);
        }
    }
    pub fn room(&self, id: RoomId) -> Option<&Room> {
        self.rooms.get(&id)
    }
}
