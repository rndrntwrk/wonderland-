use super::{
    placement::PlacementRules,
    tiles::{Facing, LotPosition, TilePos},
};
use crate::ids::{EntityRef, PersistentId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_FOOTPRINT_RECTS: usize = 64;
pub const MAX_FOOTPRINT_EXTENT: i16 = 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FootprintRect {
    pub min_x: i16,
    pub min_y: i16,
    pub max_x: i16,
    pub max_y: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldRect {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
    pub level: u8,
}

impl WorldRect {
    pub fn intersects(self, other: Self) -> bool {
        self.level == other.level
            && self.min_x < other.max_x
            && self.max_x > other.min_x
            && self.min_y < other.max_y
            && self.max_y > other.min_y
    }
    pub fn tiles(self) -> Vec<TilePos> {
        if self.min_x >= self.max_x || self.min_y >= self.max_y {
            return Vec::new();
        }
        let (min_x, max_x, min_y, max_y) = (
            self.min_x.div_euclid(16),
            (self.max_x - 1).div_euclid(16),
            self.min_y.div_euclid(16),
            (self.max_y - 1).div_euclid(16),
        );
        if min_x < i32::from(i16::MIN)
            || max_x > i32::from(i16::MAX)
            || min_y < i32::from(i16::MIN)
            || max_y > i32::from(i16::MAX)
            || max_x - min_x > 256
            || max_y - min_y > 256
        {
            return Vec::new();
        }
        let mut out = Vec::new();
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                out.push(TilePos::new(x as i16, y as i16, self.level));
            }
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Footprint {
    pub rects: Vec<FootprintRect>,
}

impl Default for Footprint {
    fn default() -> Self {
        Self::rectangle(-4, -4, 4, 4)
    }
}

impl Footprint {
    pub fn rectangle(min_x: i16, min_y: i16, max_x: i16, max_y: i16) -> Self {
        Self {
            rects: vec![FootprintRect {
                min_x,
                min_y,
                max_x,
                max_y,
            }],
        }
    }
    pub fn valid(&self) -> bool {
        self.rects.len() <= MAX_FOOTPRINT_RECTS
            && self.rects.iter().all(|r| {
                r.min_x < r.max_x
                    && r.min_y < r.max_y
                    && [r.min_x, r.max_x, r.min_y, r.max_y]
                        .iter()
                        .all(|&n| (-MAX_FOOTPRINT_EXTENT..=MAX_FOOTPRINT_EXTENT).contains(&n))
            })
    }
    pub fn at(&self, position: LotPosition, facing: Facing) -> Vec<WorldRect> {
        self.rects
            .iter()
            .map(|r| {
                let corners = [
                    (r.min_x, r.min_y),
                    (r.max_x, r.min_y),
                    (r.min_x, r.max_y),
                    (r.max_x, r.max_y),
                ];
                let rotated: Vec<_> = corners
                    .iter()
                    .map(|&(x, y)| facing.rotate(i32::from(x), i32::from(y)))
                    .collect();
                WorldRect {
                    min_x: position
                        .x
                        .saturating_add(rotated.iter().map(|p| p.0).min().unwrap_or(0)),
                    max_x: position
                        .x
                        .saturating_add(rotated.iter().map(|p| p.0).max().unwrap_or(0)),
                    min_y: position
                        .y
                        .saturating_add(rotated.iter().map(|p| p.1).min().unwrap_or(0)),
                    max_y: position
                        .y
                        .saturating_add(rotated.iter().map(|p| p.1).max().unwrap_or(0)),
                    level: position.level,
                }
            })
            .collect()
    }
    pub fn max_size(&self) -> u16 {
        self.rects
            .iter()
            .map(|r| {
                (i32::from(r.max_x) - i32::from(r.min_x))
                    .max(i32::from(r.max_y) - i32::from(r.min_y)) as u16
            })
            .max()
            .unwrap_or(0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObstacleMotion {
    #[default]
    Static,
    MovingAvatar,
    StationaryAvatar,
    BeingShooed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldObject {
    pub entity: EntityRef,
    pub owner: Option<PersistentId>,
    pub position: LotPosition,
    pub facing: Facing,
    pub footprint: Footprint,
    pub rules: PlacementRules,
    pub revision: u64,
    pub multitile_group: Option<EntityRef>,
    pub entrypoints: BTreeSet<u8>,
    pub motion: ObstacleMotion,
    pub in_use: bool,
    pub for_sale: bool,
}

impl WorldObject {
    pub fn new(entity: EntityRef, position: LotPosition) -> Self {
        Self {
            entity,
            owner: None,
            position,
            facing: Facing::NORTH,
            footprint: Footprint::default(),
            rules: PlacementRules::default(),
            revision: 0,
            multitile_group: None,
            entrypoints: BTreeSet::new(),
            motion: ObstacleMotion::Static,
            in_use: false,
            for_sale: false,
        }
    }
    pub fn rects(&self) -> Vec<WorldRect> {
        if self.rules.zero_extent || self.position.is_out_of_world() {
            Vec::new()
        } else {
            self.footprint.at(self.position, self.facing)
        }
    }
    pub fn valid(&self) -> bool {
        self.entity.object_id.0 > 0
            && self.entity.generation > 0
            && self.facing.valid()
            && self.footprint.valid()
            && self.rules.valid()
            && self
                .multitile_group
                .map_or(true, |r| r.object_id.0 > 0 && r.generation > 0)
    }
    pub fn intersects(&self, other: &Self) -> bool {
        self.rects()
            .iter()
            .any(|a| other.rects().iter().any(|b| a.intersects(*b)))
    }
}
