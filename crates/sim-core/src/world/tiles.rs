use serde::{Deserialize, Serialize};

pub const SUBTILES: i32 = 16;
pub const FLOOR_WATER: u16 = 65_534;
pub const FLOOR_POOL: u16 = 65_535;

/// Semantic coordinates: x/y in tiles, floors numbered from one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TilePos {
    pub x: i16,
    pub y: i16,
    pub level: u8,
}

impl TilePos {
    pub const fn new(x: i16, y: i16, level: u8) -> Self {
        Self { x, y, level }
    }
    pub fn center(self) -> LotPosition {
        LotPosition {
            x: i32::from(self.x) * SUBTILES + 8,
            y: i32::from(self.y) * SUBTILES + 8,
            level: self.level,
        }
    }
    pub fn adjacent(self, direction: Cardinal) -> Option<Self> {
        let (dx, dy) = direction.offset();
        Some(Self {
            x: self.x.checked_add(dx)?,
            y: self.y.checked_add(dy)?,
            level: self.level,
        })
    }
}

/// One unit is 1/16 tile. Wider arithmetic is intentional; bounds are checked before use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LotPosition {
    pub x: i32,
    pub y: i32,
    pub level: u8,
}

impl LotPosition {
    pub const OUT_OF_WORLD: Self = Self {
        x: -32_768,
        y: -32_768,
        level: 1,
    };
    pub const fn new(x: i32, y: i32, level: u8) -> Self {
        Self { x, y, level }
    }
    pub fn is_out_of_world(self) -> bool {
        self == Self::OUT_OF_WORLD
    }
    pub fn tile(self) -> Option<TilePos> {
        Some(TilePos {
            x: i16::try_from(self.x.div_euclid(16)).ok()?,
            y: i16::try_from(self.y.div_euclid(16)).ok()?,
            level: self.level,
        })
    }
    pub fn offset(self, x: i32, y: i32, levels: i8) -> Option<Self> {
        Some(Self {
            x: self.x.checked_add(x)?,
            y: self.y.checked_add(y)?,
            level: u8::try_from(i16::from(self.level) + i16::from(levels)).ok()?,
        })
    }
    pub fn distance_squared(self, other: Self) -> u64 {
        let dx = i64::from(self.x) - i64::from(other.x);
        let dy = i64::from(self.y) - i64::from(other.y);
        let dz = (i64::from(self.level) - i64::from(other.level)) * 160;
        ((i128::from(dx) * i128::from(dx)
            + i128::from(dy) * i128::from(dy)
            + i128::from(dz) * i128::from(dz))
        .min(i128::from(u64::MAX))) as u64
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Cardinal {
    North,
    East,
    South,
    West,
}

impl Cardinal {
    pub const ALL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];
    pub const fn offset(self) -> (i16, i16) {
        match self {
            Self::North => (0, -1),
            Self::East => (1, 0),
            Self::South => (0, 1),
            Self::West => (-1, 0),
        }
    }
    pub const fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::West => Self::East,
        }
    }
    /// Source WallSegments numeric layout (TopLeft is west; TopRight is north).
    pub const fn wall_bit(self) -> u8 {
        match self {
            Self::West => 1,
            Self::North => 2,
            Self::East => 4,
            Self::South => 8,
        }
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Facing(pub u8);

impl Facing {
    pub const NORTH: Self = Self(0);
    pub const EAST: Self = Self(2);
    pub const SOUTH: Self = Self(4);
    pub const WEST: Self = Self(6);
    pub fn valid(self) -> bool {
        self.0 < 8
    }
    pub fn rotate(self, x: i32, y: i32) -> (i32, i32) {
        let (s, c) = match self.0 & 7 {
            0 => (0, 65_536),
            1 => (46_341, 46_341),
            2 => (65_536, 0),
            3 => (46_341, -46_341),
            4 => (0, -65_536),
            5 => (-46_341, -46_341),
            6 => (-65_536, 0),
            _ => (-46_341, 46_341),
        };
        (
            round_div_even(i64::from(x) * c - i64::from(y) * s, 65_536) as i32,
            round_div_even(i64::from(x) * s + i64::from(y) * c, 65_536) as i32,
        )
    }
}

pub(crate) fn round_div_even(value: i64, denominator: i64) -> i64 {
    let q = value.div_euclid(denominator);
    let r = value.rem_euclid(denominator);
    if r * 2 > denominator || (r * 2 == denominator && q & 1 != 0) {
        q + 1
    } else {
        q
    }
}

pub(crate) fn integer_sqrt(value: u64) -> u64 {
    if value < 2 {
        return value;
    }
    let mut low = 1_u64;
    let mut high = value.min(u64::from(u32::MAX)) + 1;
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if mid <= value / mid {
            low = mid;
        } else {
            high = mid;
        }
    }
    low
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Diagonal {
    #[default]
    None,
    Vertical,
    Horizontal,
}

impl Diagonal {
    /// VMContext.GetRoomAt: vertical x>y selects low word, horizontal x+y>15 selects high word.
    pub fn half_at(self, x: i32, y: i32) -> u8 {
        let (x, y) = (x.rem_euclid(16), y.rem_euclid(16));
        match self {
            Self::None => 0,
            Self::Vertical => u8::from(x <= y),
            Self::Horizontal => u8::from(x + y > 15),
        }
    }
    pub fn half_on_edge(self, edge: Cardinal) -> u8 {
        match self {
            Self::None => 0,
            Self::Vertical => u8::from(matches!(edge, Cardinal::West | Cardinal::South)),
            Self::Horizontal => u8::from(matches!(edge, Cardinal::East | Cardinal::South)),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WallTile {
    /// Source side bits 1,2,4,8. Adjacent sides are mirrored by LotModel::set_wall.
    pub sides: u8,
    /// Subset of sides separating indoor/outdoor space; fences omit these bits.
    pub room_separators: u8,
    pub occupied: u8,
    pub diagonal: Diagonal,
    pub diagonal_solid: bool,
    /// Floor pattern for each half in the same ordering as room words.
    pub half_floors: Option<[u16; 2]>,
}

impl WallTile {
    pub fn solid(sides: u8) -> Self {
        Self {
            sides,
            room_separators: sides,
            ..Self::default()
        }
    }
    pub fn valid(&self) -> bool {
        self.sides & !15 == 0
            && self.room_separators & !self.sides == 0
            && self.occupied & !self.sides == 0
            && (self.diagonal != Diagonal::None || self.half_floors.is_none())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile {
    pub floor: u16,
    pub wall: WallTile,
    pub object_support: bool,
    pub supported: bool,
}

impl Tile {
    pub fn floor_in_half(&self, half: u8) -> u16 {
        self.wall
            .half_floors
            .map_or(self.floor, |f| f[usize::from(half.min(1))])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildBounds {
    pub min_x: i16,
    pub min_y: i16,
    pub max_x: i16,
    pub max_y: i16,
    pub max_level: u8,
}

impl BuildBounds {
    pub fn contains(self, position: TilePos) -> bool {
        position.level > 0
            && position.level <= self.max_level
            && position.x >= self.min_x
            && position.y >= self.min_y
            && position.x < self.max_x
            && position.y < self.max_y
    }
}
