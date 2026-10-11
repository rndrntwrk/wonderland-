use crate::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildTile {
    pub x: u16,
    pub y: u16,
    pub level: u8,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildDraftContext {
    pub actor: SourceActorLot,
    pub generation: u64,
    pub bounds: LotBounds,
}
#[derive(Clone, Debug, Default)]
pub struct BuildTileDraft {
    pub context: Option<BuildDraftContext>,
    pub resource: Option<BuildResource>,
    pub start: Option<BuildTile>,
    pub end: Option<BuildTile>,
    pub command: Option<ArchitectureCommand>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildPick {
    Started,
    Ready,
}
impl BuildTileDraft {
    pub fn cancel(&mut self) {
        *self = Self::default();
    }
    /// Two-click equivalent of the original source start/end drag. Tile values
    /// denote the tile's top-left vertex for wall tools, full cells for floors.
    pub fn pick(
        &mut self,
        context: &BuildDraftContext,
        resource: &BuildResource,
        tile: BuildTile,
        diagonal: Option<bool>,
    ) -> Result<BuildPick, AuthoringError> {
        if tile_build_unavailable_reason(resource).is_some() {
            return Err(AuthoringError::Unsupported(resource.tool));
        }
        if context.generation == 0
            || tile.x >= context.bounds.width
            || tile.y >= context.bounds.height
            || tile.level == 0
            || tile.level > context.bounds.levels
            || tile.level > 127
        {
            return Err(AuthoringError::Invalid("source build extent"));
        }
        if self.context.as_ref().is_some_and(|old| old != context) {
            self.cancel();
            return Err(AuthoringError::Stale);
        }
        if self.resource.as_ref().is_some_and(|old| old != resource) || self.command.is_some() {
            self.cancel();
        }
        self.context = Some(context.clone());
        self.resource = Some(resource.clone());
        if [4, 6].contains(&resource.tool) {
            if resource.tool == 6 && resource.pattern >= 65534 {
                return Err(AuthoringError::Missing("pool requires rectangle tool"));
            }
            self.start = Some(tile);
            self.end = Some(tile);
            self.command = Some(ArchitectureCommand {
                kind: resource.tool,
                x: i32::from(tile.x),
                y: i32::from(tile.y),
                level: tile.level as i8,
                x2: 0,
                y2: 0,
                pattern: resource.pattern,
                style: 0,
            });
            return Ok(BuildPick::Ready);
        }
        let Some(start) = self.start else {
            self.start = Some(tile);
            return Ok(BuildPick::Started);
        };
        if tile.level != start.level {
            self.cancel();
            return Err(AuthoringError::Stale);
        }
        let mut command = ArchitectureCommand {
            kind: resource.tool,
            x: i32::from(start.x.min(tile.x)),
            y: i32::from(start.y.min(tile.y)),
            level: tile.level as i8,
            x2: i32::from(start.x.abs_diff(tile.x)),
            y2: i32::from(start.y.abs_diff(tile.y)),
            pattern: resource.pattern,
            style: if resource.tool == 5 {
                0
            } else {
                resource.style
            },
        };
        let mut end = tile;
        if [0, 1].contains(&resource.tool) {
            let dx = i32::from(tile.x) - i32::from(start.x);
            let dy = i32::from(tile.y) - i32::from(start.y);
            let length = (f64::from(dx) * f64::from(dx) + f64::from(dy) * f64::from(dy))
                .sqrt()
                .round_ties_even() as i32;
            if length == 0 {
                return Err(AuthoringError::Invalid("zero length wall"));
            }
            let direction = (f64::from(dy).atan2(f64::from(dx)) / (std::f64::consts::PI / 4.0))
                .round_ties_even()
                .rem_euclid(8.0) as usize;
            let units = [
                (1, 0),
                (1, 1),
                (0, 1),
                (-1, 1),
                (-1, 0),
                (-1, -1),
                (0, -1),
                (1, -1),
            ];
            let (ux, uy) = units[direction];
            let x = i32::from(start.x) + ux * length;
            let y = i32::from(start.y) + uy * length;
            if x < 0
                || y < 0
                || x >= i32::from(context.bounds.width)
                || y >= i32::from(context.bounds.height)
            {
                return Err(AuthoringError::Invalid(
                    "snapped wall endpoint outside source lot",
                ));
            }
            command.x = i32::from(start.x);
            command.y = i32::from(start.y);
            command.x2 = length;
            command.y2 = direction as i32;
            end = BuildTile {
                x: x as u16,
                y: y as u16,
                level: tile.level,
            };
        } else if resource.tool == 2 && start == tile {
            return Err(AuthoringError::Invalid("empty wall rectangle"));
        } else if resource.tool == 5 && start == tile && resource.pattern < 65534 {
            match diagonal {
                Some(false) => {}
                Some(true) => return Err(AuthoringError::Missing("diagonal floor half selection")),
                None => return Err(AuthoringError::Missing("source diagonal floor geometry")),
            }
        }
        self.end = Some(end);
        self.command = Some(command);
        Ok(BuildPick::Ready)
    }
}
pub fn tile_build_unavailable_reason(resource: &BuildResource) -> Option<&'static str> {
    match resource.tool {
        0 | 1 | 2 | 4 | 5 | 6 => None,
        3 => Some("Choose a wall side before painting one wall."),
        7 | 8 => Some("Terrain editing needs a height selection."),
        9 => Some("Grass painting needs an intensity selection."),
        _ => Some("This building tool needs its original controls."),
    }
}
