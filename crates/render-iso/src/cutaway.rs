use crate::*;
use wonderland_render_core::Vec3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CutawayMap {
    pub width: u32,
    pub height: u32,
    pub down: Vec<bool>,
}
impl CutawayMap {
    pub fn new(width: u32, height: u32, down: Vec<bool>, max_tiles: usize) -> Result<Self> {
        let count = (width as usize)
            .checked_mul(height as usize)
            .ok_or(IsoError::Limit("cutaway dimensions"))?;
        if count > max_tiles || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err(IsoError::Limit("cutaway tiles"));
        }
        if width == 0 || height == 0 || down.len() != count {
            return Err(IsoError::Invalid("cutaway dimensions"));
        }
        Ok(Self {
            width,
            height,
            down,
        })
    }
    fn valid(&self) -> Result<()> {
        if self.width == 0
            || self.height == 0
            || self.down.len()
                != (self.width as usize)
                    .checked_mul(self.height as usize)
                    .ok_or(IsoError::Limit("cutaway dimensions"))?
        {
            return Err(IsoError::Invalid("cutaway dimensions"));
        }
        Ok(())
    }
    fn at(&self, x: i32, y: i32) -> Option<bool> {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            None
        } else {
            self.down
                .get(y as usize * self.width as usize + x as usize)
                .copied()
        }
    }
    pub fn wall_cuts(
        &self,
        x: i32,
        y: i32,
        level: i16,
        visible_level: i16,
        wall: WallCutInput,
    ) -> Result<WallCuts> {
        self.valid()?;
        let down = self
            .at(x, y)
            .ok_or(IsoError::Invalid("cutaway coordinate"))?;
        if level < 1 || visible_level < 1 || level > visible_level {
            return Err(IsoError::Invalid("cutaway floor"));
        }
        if !down || level < visible_level {
            return Ok(WallCuts::default());
        }
        let up = |dx: i32, dy: i32| !self.at(x + dx, y + dy).unwrap_or(false);
        let py = up(0, 1);
        let px = up(1, 0);
        let ny = up(0, -1);
        let nx = up(-1, 0);
        let sn = up(-1, -1);
        let spy = up(-1, 1);
        let spx = up(1, -1);
        let top_left = if !wall.top_left_thick || wall.top_left_style == 255 || nx {
            WallCut::Up
        } else if py || spy {
            if ny || sn {
                WallCut::Down
            } else {
                WallCut::DownRightUpLeft
            }
        } else if ny || sn {
            WallCut::DownLeftUpRight
        } else {
            WallCut::Down
        };
        let top_right = if wall.top_right_style != 1 {
            WallCut::Up
        } else {
            match wall.shape {
                WallShape::HorizontalDiagonal => slope(py || nx, ny || px),
                WallShape::VerticalDiagonal => slope(py || px, ny || nx),
                WallShape::Cardinal => {
                    if ny {
                        WallCut::Up
                    } else if px || spx {
                        if nx || sn {
                            WallCut::Down
                        } else {
                            WallCut::DownLeftUpRight
                        }
                    } else if nx || sn {
                        WallCut::DownRightUpLeft
                    } else {
                        WallCut::Down
                    }
                }
            }
        };
        Ok(WallCuts {
            top_left,
            top_right,
        })
    }
    pub fn object_hidden(
        &self,
        position: Vec3,
        level: i16,
        visible_level: i16,
        build_mode: u8,
        adjacent: WallSide,
    ) -> Result<bool> {
        self.valid()?;
        if !position.is_finite() {
            return Err(IsoError::Invalid("cutaway object position"));
        }
        if level != visible_level || level < 1 || build_mode > 1 {
            return Ok(false);
        }
        let (x, y) = (round_even(position.x), round_even(position.y));
        if x < 0. || y < 0. || x >= self.width as f32 || y >= self.height as f32 {
            return Ok(false);
        }
        let (dx, dy) = match adjacent {
            WallSide::TopLeft => (-1, 0),
            WallSide::TopRight => (0, -1),
            WallSide::BottomLeft => (0, 1),
            WallSide::BottomRight => (1, 0),
            WallSide::None => (0, 0),
        };
        Ok(self.at(x as i32, y as i32) == Some(true)
            && self.at(x as i32 + dx, y as i32 + dy) == Some(true))
    }
    pub fn rotated_cuts(
        &self,
        cuts: &[WallCuts],
        x: u32,
        y: u32,
        rotation: Rotation,
        diagonal: bool,
    ) -> Result<WallCuts> {
        self.valid()?;
        if cuts.len() != self.down.len() || x >= self.width || y >= self.height {
            return Err(IsoError::Invalid("wall cut array"));
        }
        let local = cuts[y as usize * self.width as usize + x as usize];
        let next_x = if x + 1 < self.width {
            Some(cuts[y as usize * self.width as usize + x as usize + 1])
        } else {
            None
        };
        let next_y = if y + 1 < self.height {
            Some(cuts[(y as usize + 1) * self.width as usize + x as usize])
        } else {
            None
        };
        if diagonal {
            return Ok(if rotation as u8 > 1 {
                WallCuts {
                    top_left: flip(local.top_left),
                    top_right: flip(local.top_right),
                }
            } else {
                local
            });
        }
        Ok(match rotation {
            Rotation::TopLeft => local,
            Rotation::TopRight => WallCuts {
                top_left: flip(next_y.unwrap_or_default().top_right),
                top_right: local.top_left,
            },
            Rotation::BottomRight => WallCuts {
                top_left: flip(next_x.unwrap_or_default().top_left),
                top_right: flip(next_y.map_or(local.top_left, |c| c.top_right)),
            },
            Rotation::BottomLeft => WallCuts {
                top_left: local.top_right,
                top_right: flip(next_x.unwrap_or_default().top_left),
            },
        })
    }
}
fn slope(left: bool, right: bool) -> WallCut {
    match (left, right) {
        (true, false) => WallCut::DownRightUpLeft,
        (false, true) => WallCut::DownLeftUpRight,
        _ => WallCut::Down,
    }
}
fn flip(cut: WallCut) -> WallCut {
    match cut {
        WallCut::DownLeftUpRight => WallCut::DownRightUpLeft,
        WallCut::DownRightUpLeft => WallCut::DownLeftUpRight,
        _ => cut,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallSide {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    None,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallShape {
    Cardinal,
    HorizontalDiagonal,
    VerticalDiagonal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WallCutInput {
    pub top_left_thick: bool,
    pub top_left_style: u16,
    pub top_right_style: u16,
    pub shape: WallShape,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum WallCut {
    #[default]
    Up = 0,
    DownLeftUpRight = 1,
    DownRightUpLeft = 2,
    Down = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WallCuts {
    pub top_left: WallCut,
    pub top_right: WallCut,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CutStyle {
    pub style: u16,
    pub overlay: Option<u16>,
    pub down_mask: bool,
    pub omit: bool,
}
pub fn cut_style(cut: WallCut, original: u16, door: bool) -> CutStyle {
    let (style, overlay, down_mask) = match cut {
        WallCut::Up => (original, None, false),
        WallCut::Down => (original, None, true),
        WallCut::DownLeftUpRight => (7, Some(252), false),
        WallCut::DownRightUpLeft => (8, Some(253), false),
    };
    CutStyle {
        style,
        overlay,
        down_mask,
        omit: door && cut != WallCut::Up,
    }
}
pub fn generated_wall_depth(
    zoom: Zoom,
    shape: WallShape,
    side: WallSide,
) -> Result<(u32, u32, Vec<u8>)> {
    let scale = match zoom {
        Zoom::Near => 1u32,
        Zoom::Medium => 2,
        Zoom::Far => 4,
    };
    let (w, h, start, dx): (u32, u32, f32, f32) = match shape {
        WallShape::Cardinal => {
            let h = match zoom {
                Zoom::Near => 271,
                Zoom::Medium => 135,
                Zoom::Far => 67,
            };
            match side {
                WallSide::TopLeft => (64 / scale, h, 74., scale as f32),
                WallSide::TopRight => (64 / scale, h, 135., -(scale as f32)),
                _ => return Err(IsoError::Invalid("wall depth side")),
            }
        }
        WallShape::HorizontalDiagonal => (128 / scale, 240 / scale, 89.5, 0.),
        WallShape::VerticalDiagonal => (16 / scale, 232 / scale, 45., 0.),
    };
    let dy = scale as f32 * 0.5;
    let mut result = Vec::with_capacity((w * h) as usize);
    let mut row = start;
    for _ in 0..h {
        let mut column = row;
        for _ in 0..w {
            result.push(round_even(column.min(255.)) as u8);
            column += dx;
        }
        row += dy;
    }
    Ok((w, h, result))
}
