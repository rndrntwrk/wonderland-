use super::{AuthoringError, ContentSourceId};
use serde::{Deserialize, Serialize};

/// Allocation safety for a single rectangular occupancy query, not lot policy.
pub const MAX_FOOTPRINT_CELLS: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GridCell {
    pub x: i16,
    pub y: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LotCell {
    pub cell: GridCell,
    pub level: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    pub fn clockwise(self) -> Self {
        match self {
            Self::North => Self::East,
            Self::East => Self::South,
            Self::South => Self::West,
            Self::West => Self::North,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridPose {
    pub cell: GridCell,
    #[serde(default)]
    pub level: i16,
    pub direction: Direction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotBounds {
    pub origin: GridCell,
    pub width: u16,
    pub depth: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotGeometry {
    pub source: ContentSourceId,
    pub revision: u64,
    pub bounds: LotBounds,
    pub levels: Vec<i16>,
    pub reserved: Vec<LotCell>,
}

impl LotGeometry {
    pub fn contains(&self, cell: LotCell) -> bool {
        let dx = i32::from(cell.cell.x) - i32::from(self.bounds.origin.x);
        let dy = i32::from(cell.cell.y) - i32::from(self.bounds.origin.y);
        dx >= 0 && dy >= 0
            && dx < i32::from(self.bounds.width)
            && dy < i32::from(self.bounds.depth)
            && self.levels.contains(&cell.level)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Footprint {
    pub width: u16,
    pub depth: u16,
}

impl Footprint {
    pub fn rotated(self, direction: Direction) -> Self {
        match direction {
            Direction::North | Direction::South => self,
            Direction::East | Direction::West => Self {
                width: self.depth,
                depth: self.width,
            },
        }
    }

    /// Rectangular occupancy in supplied lot coordinates and level. Canonical
    /// geometry and renderer picking still belong to the connected world adapter.
    pub fn cells(self, pose: GridPose, lot: &LotGeometry) -> Result<Vec<LotCell>, AuthoringError> {
        let footprint = self.rotated(pose.direction);
        if footprint.width == 0 || footprint.depth == 0 {
            return Err(AuthoringError::OutOfBounds);
        }
        if usize::from(footprint.width) * usize::from(footprint.depth) > MAX_FOOTPRINT_CELLS {
            return Err(AuthoringError::SafetyLimit);
        }
        let end_x = i32::from(pose.cell.x) + i32::from(footprint.width) - 1;
        let end_y = i32::from(pose.cell.y) + i32::from(footprint.depth) - 1;
        let last = GridCell {
            x: end_x.try_into().map_err(|_| AuthoringError::OutOfBounds)?,
            y: end_y.try_into().map_err(|_| AuthoringError::OutOfBounds)?,
        };
        if !lot.contains(LotCell { cell: pose.cell, level: pose.level })
            || !lot.contains(LotCell { cell: last, level: pose.level })
        {
            return Err(AuthoringError::OutOfBounds);
        }
        let cells: Vec<_> = (i32::from(pose.cell.y)..=end_y)
            .flat_map(|y| (i32::from(pose.cell.x)..=end_x).map(move |x| LotCell {
                cell: GridCell { x: x as i16, y: y as i16 },
                level: pose.level,
            }))
            .collect();
        if cells.iter().any(|cell| lot.reserved.contains(cell)) {
            return Err(AuthoringError::EntranceReserved);
        }
        Ok(cells)
    }
}
