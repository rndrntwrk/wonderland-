use super::AuthoringError;
use serde::{Deserialize, Serialize};

pub const ROOM_WIDTH: i16 = 8;
pub const ROOM_DEPTH: i16 = 6;

/// Integer presentation coordinates, unrelated to canonical simulation positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GridCell {
    pub x: i16,
    pub y: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    pub direction: Direction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Footprint {
    pub width: u8,
    pub depth: u8,
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

    /// Return at most 48 cells, rejecting the room boundary and reserved entrance.
    pub fn cells(self, pose: GridPose) -> Result<Vec<GridCell>, AuthoringError> {
        let footprint = self.rotated(pose.direction);
        let end_x = pose
            .cell
            .x
            .checked_add(i16::from(footprint.width))
            .ok_or(AuthoringError::OutOfBounds)?;
        let end_y = pose
            .cell
            .y
            .checked_add(i16::from(footprint.depth))
            .ok_or(AuthoringError::OutOfBounds)?;
        if footprint.width == 0
            || footprint.depth == 0
            || pose.cell.x < 0
            || pose.cell.y < 0
            || end_x > ROOM_WIDTH
            || end_y > ROOM_DEPTH
        {
            return Err(AuthoringError::OutOfBounds);
        }
        if pose.cell == (GridCell { x: 0, y: 0 }) {
            return Err(AuthoringError::EntranceReserved);
        }
        Ok((pose.cell.y..end_y)
            .flat_map(|y| (pose.cell.x..end_x).map(move |x| GridCell { x, y }))
            .collect())
    }
}
