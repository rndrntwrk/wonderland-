use crate::{IsoError, Result};
use wonderland_render_core::{units, Mat4, Quat, Vec2, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Zoom {
    Far = 1,
    Medium = 2,
    Near = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Rotation {
    TopLeft = 0,
    TopRight = 1,
    BottomRight = 2,
    BottomLeft = 3,
}
impl Rotation {
    pub const ALL: [Self; 4] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomRight,
        Self::BottomLeft,
    ];
}
impl Zoom {
    pub const ALL: [Self; 3] = [Self::Far, Self::Medium, Self::Near];
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomMetrics {
    pub tile_size: Vec2,
    pub cadge_size: Vec2,
    pub baseline: f32,
    pub terrain_height: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projection {
    pub zoom: Zoom,
    pub rotation: Rotation,
    pub precise_zoom: f32,
    pub center_tile: Vec3,
    pub viewport: Vec2,
}
impl Projection {
    pub fn new(
        zoom: Zoom,
        rotation: Rotation,
        precise_zoom: f32,
        center_tile: Vec3,
        viewport: Vec2,
    ) -> Result<Self> {
        if !precise_zoom.is_finite()
            || precise_zoom <= 0.
            || !center_tile.is_finite()
            || !viewport.is_finite()
            || viewport.x <= 0.
            || viewport.y <= 0.
        {
            return Err(IsoError::Invalid("projection inputs"));
        }
        let p = Self {
            zoom,
            rotation,
            precise_zoom,
            center_tile,
            viewport,
        };
        if !p.project_tile(center_tile).is_finite() || !p.framebuffer_point(Vec3::ZERO).is_finite()
        {
            return Err(IsoError::Invalid("projection overflow"));
        }
        Ok(p)
    }
    pub fn metrics(self) -> ZoomMetrics {
        let (w, h, cw, ch, b, t) = match self.zoom {
            Zoom::Far => (32., 16., 34., 96., 87., -59.),
            Zoom::Medium => (64., 32., 68., 192., 174., -118.),
            Zoom::Near => (128., 64., 136., 384., 348., -235.),
        };
        ZoomMetrics {
            tile_size: Vec2::new(w, h),
            cadge_size: Vec2::new(cw, ch),
            baseline: b,
            terrain_height: t,
        }
    }
    /// Discrete source pixel coordinates, before scrolling or precise zoom.
    pub fn project_tile(self, tile: Vec3) -> Vec2 {
        let m = self.metrics();
        let w = m.tile_size.x * 0.5;
        let h = m.tile_size.y * 0.5;
        let v =
            tile.z * m.tile_size.x / std::f32::consts::SQRT_2 * (std::f32::consts::PI / 6.).cos();
        let (x, y) = match self.rotation {
            Rotation::TopLeft => ((tile.x - tile.y) * w, (tile.x + tile.y) * h),
            Rotation::TopRight => ((-tile.x - tile.y) * w, (tile.x - tile.y) * h),
            Rotation::BottomRight => ((-tile.x + tile.y) * w, (-tile.x - tile.y) * h),
            Rotation::BottomLeft => ((tile.x + tile.y) * w, (-tile.x + tile.y) * h),
        };
        Vec2::new(x, y - v)
    }
    pub fn inverse_ground(self, pixel: Vec2) -> Vec2 {
        let m = self.metrics();
        let x = pixel.x / (m.tile_size.x * 0.5);
        let y = pixel.y / (m.tile_size.y * 0.5);
        match self.rotation {
            Rotation::TopLeft => Vec2::new((y + x) * 0.5, (y - x) * 0.5),
            Rotation::TopRight => Vec2::new((y - x) * 0.5, (-y - x) * 0.5),
            Rotation::BottomRight => Vec2::new((-y - x) * 0.5, (-y + x) * 0.5),
            Rotation::BottomLeft => Vec2::new((-y + x) * 0.5, (y + x) * 0.5),
        }
    }
    pub fn framebuffer_point(self, tile: Vec3) -> Vec2 {
        (self.project_tile(tile) - self.project_tile(self.center_tile)) * self.precise_zoom
            + self.viewport * 0.5
    }
    pub fn point_screen_offset(self) -> Vec2 {
        self.viewport * 0.5 - self.project_tile(self.center_tile)
    }
    pub fn sprite_screen_offset(self) -> Vec2 {
        let m = self.metrics();
        let mut off = self.point_screen_offset() - Vec2::new(m.cadge_size.x * 0.5, m.baseline);
        off = off
            + match self.rotation {
                Rotation::TopLeft => Vec2::new(0., m.tile_size.y * 0.5),
                Rotation::TopRight => Vec2::new(-m.tile_size.x * 0.5, 0.),
                Rotation::BottomRight => Vec2::new(0., -m.tile_size.y * 0.5),
                Rotation::BottomLeft => Vec2::new(m.tile_size.x * 0.5, 0.),
            };
        Vec2::new(round_even(off.x), round_even(off.y))
    }
    /// Source controller snaps the projected center, applies half-pixel x bias,
    /// and returns a ground center, independently of cadge sprite alignment.
    pub fn camera_center(self) -> Vec3 {
        let p = self.project_tile(self.center_tile);
        let correction = -0.5 * (1u32 << (3 - self.zoom as u8)) as f32;
        let t = self.inverse_ground(Vec2::new(
            round_even(p.x) + correction,
            round_even(p.y + 0.5),
        ));
        Vec3::new(t.x, t.y, 0.)
    }
    pub fn draw_order(self, tile: Vec3) -> f32 {
        self.project_tile(tile).y
            + tile.z * self.metrics().tile_size.x / std::f32::consts::SQRT_2 * 2.
    }
    /// Source orthographic camera in the core column-vector convention. Source
    /// near is negative; construct its literal matrix rather than changing core's
    /// positive-near camera contract. Depth is 0..1, view looks down -Z.
    pub fn world_view_projection(self) -> Result<Mat4> {
        let (diagonal, depth) = match self.zoom {
            Zoom::Far => (64., 256.),
            Zoom::Medium => (128., 128.),
            Zoom::Near => (256., 64.),
        };
        let depth = depth * 4. / self.precise_zoom;
        let scale = (18.0f32).sqrt() / (diagonal * self.precise_zoom);
        let half = self.viewport * scale;
        let near = -(150. + depth - 64.);
        let far = depth;
        let projection = Mat4 {
            cols: [
                [1. / half.x, 0., 0., 0.],
                [0., 1. / half.y, 0., 0.],
                [0., 0., 1. / (near - far), 0.],
                [0., 0., near / (near - far), 1.],
            ],
        };
        let degrees = match self.rotation {
            Rotation::TopLeft => 315.0f32,
            Rotation::TopRight => 225.,
            Rotation::BottomRight => 135.,
            Rotation::BottomLeft => 45.,
        };
        let y = Quat::from_axis_angle(Vec3::Y, degrees.to_radians())
            .ok_or(IsoError::Invalid("camera rotation"))?;
        let x = Quat::from_axis_angle(Vec3::X, 30.0f32.to_radians())
            .ok_or(IsoError::Invalid("camera rotation"))?;
        let view = Mat4::from_quat(x)
            * Mat4::from_quat(y)
            * Mat4::from_translation(-units::tile_to_graphics(self.camera_center()));
        let result = projection * view;
        if result.cols.iter().flatten().any(|x| !x.is_finite()) {
            return Err(IsoError::Invalid("camera matrix overflow"));
        }
        Ok(result)
    }
}
pub fn round_even(value: f32) -> f32 {
    let lower = value.floor();
    let fraction = value - lower;
    if fraction < 0.5 {
        lower
    } else if fraction > 0.5 || lower % 2.0 != 0. {
        lower + 1.0
    } else {
        lower
    }
}
pub fn cache_grid_origin(pixel: Vec2, precise_zoom: f32) -> Result<Vec2> {
    if !pixel.is_finite() || !precise_zoom.is_finite() || precise_zoom <= 0. {
        return Err(IsoError::Invalid("cache grid inputs"));
    }
    let step = 512. / precise_zoom;
    let result = Vec2::new(
        (pixel.x / step).floor() * step,
        (pixel.y / step).floor() * step,
    );
    if !step.is_finite() || step == 0. || !result.is_finite() {
        return Err(IsoError::Invalid("cache grid overflow"));
    }
    Ok(result)
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CacheRestorePlacement {
    pub destination: crate::Rect,
    pub world_translation: Vec3,
}
pub fn cache_restore_placement(
    stored_pixel: Vec2,
    current_pixel: Vec2,
    stored_tile: Vec3,
    current_tile: Vec3,
    size: [u32; 2],
    precise_zoom: f32,
) -> Result<CacheRestorePlacement> {
    if !stored_pixel.is_finite()
        || !current_pixel.is_finite()
        || !stored_tile.is_finite()
        || !current_tile.is_finite()
        || !precise_zoom.is_finite()
        || precise_zoom <= 0.
        || size[0] == 0
        || size[1] == 0
    {
        return Err(IsoError::Invalid("cache restore placement inputs"));
    }
    let offset = (stored_pixel - current_pixel) * precise_zoom;
    let delta = units::tile_to_graphics(stored_tile - current_tile);
    if !offset.is_finite()
        || !delta.is_finite()
        || [offset.x, offset.y]
            .iter()
            .any(|x| *x < i32::MIN as f32 + 2. || *x >= i32::MAX as f32)
    {
        return Err(IsoError::Invalid("cache placement overflow"));
    }
    Ok(CacheRestorePlacement {
        destination: crate::Rect {
            x: offset.x.trunc() - 2.,
            y: offset.y.trunc(),
            width: size[0] as f32,
            height: size[1] as f32,
        },
        world_translation: delta,
    })
}
