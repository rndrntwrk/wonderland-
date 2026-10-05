//! Fixed-camera affine floor adapter. Art coordinates are not canonical lot positions.
use crate::geometry::{Point, Size, WORLD_HEIGHT, WORLD_WIDTH};
use wonderland_contracts::authoring::*;
pub const FLOOR_ORIGIN: Point = Point { x: 811., y: 317. };
pub fn grid_point(x: f64, y: f64) -> Point {
    Point {
        x: FLOOR_ORIGIN.x + 86. * x - 94. * y,
        y: FLOOR_ORIGIN.y + 42. * x + 38. * y,
    }
}
pub fn cell_center(cell: GridCell) -> Point {
    grid_point(f64::from(cell.x) + 0.5, f64::from(cell.y) + 0.5)
}
pub fn footprint_points(footprint: Footprint, pose: GridPose) -> Vec<Point> {
    let f = footprint.rotated(pose.direction);
    let x = f64::from(pose.cell.x);
    let y = f64::from(pose.cell.y);
    let w = f64::from(f.width);
    let d = f64::from(f.depth);
    vec![
        grid_point(x, y),
        grid_point(x + w, y),
        grid_point(x + w, y + d),
        grid_point(x, y + d),
    ]
}
pub fn footprint_center(footprint: Footprint, pose: GridPose) -> Point {
    let f = footprint.rotated(pose.direction);
    grid_point(
        f64::from(pose.cell.x) + f64::from(f.width) / 2.,
        f64::from(pose.cell.y) + f64::from(f.depth) / 2.,
    )
}
/// Art-space union of the complete sprite image rectangle and floor footprint.
/// Full image bounds also protect visible alpha content without relying on transparent pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FurnitureBounds {
    pub min: Point,
    pub max: Point,
}
pub fn furniture_bounds(id: &str, footprint: Footprint, pose: GridPose) -> FurnitureBounds {
    let center = footprint_center(footprint, pose);
    let sprite = sprite_layout(id, pose.direction);
    let mut bounds = FurnitureBounds {
        min: Point {
            x: center.x - sprite.anchor.x,
            y: center.y - sprite.anchor.y,
        },
        max: Point {
            x: center.x - sprite.anchor.x + sprite.width,
            y: center.y - sprite.anchor.y + sprite.height,
        },
    };
    for p in footprint_points(footprint, pose) {
        bounds.min.x = bounds.min.x.min(p.x);
        bounds.min.y = bounds.min.y.min(p.y);
        bounds.max.x = bounds.max.x.max(p.x);
        bounds.max.y = bounds.max.y.max(p.y);
    }
    bounds
}
pub fn placement_bounds(
    projection: &AuthoringProjection,
    draft: &PlacementDraft,
) -> Option<FurnitureBounds> {
    let id = match &draft.source {
        PlacementSource::Catalog(id) => id,
        PlacementSource::Move(id) | PlacementSource::Inventory(id) => {
            &projection
                .home(&draft.home_owner_id)?
                .instance(id)?
                .catalog_id
        }
    };
    let item = projection.catalog_item(id)?;
    Some(furniture_bounds(id.as_ref(), item.footprint, draft.pose))
}
#[derive(Clone, Copy, Debug)]
pub struct HomeCamera {
    pub viewport: Size,
    pub zoom: f64,
    pub pan: Point,
}
impl HomeCamera {
    pub fn new(viewport: Size) -> Self {
        Self {
            viewport,
            zoom: 1.,
            pan: Point::default(),
        }
    }
    /// Contain the full room in a real viewport above the drawer; explicit overscroll allows edge reveal.
    pub fn scale(self) -> f64 {
        (self.viewport.width / WORLD_WIDTH).min(self.viewport.height / WORLD_HEIGHT) * self.zoom
    }
    pub fn origin(self) -> Point {
        Point {
            x: (self.viewport.width - WORLD_WIDTH * self.scale()) / 2. + self.pan.x,
            y: (self.viewport.height - WORLD_HEIGHT * self.scale()) / 2. + self.pan.y,
        }
    }
    pub fn project(self, point: Point) -> Point {
        let o = self.origin();
        Point {
            x: o.x + point.x * self.scale(),
            y: o.y + point.y * self.scale(),
        }
    }
    pub fn inverse(self, point: Point) -> Point {
        let o = self.origin();
        Point {
            x: (point.x - o.x) / self.scale(),
            y: (point.y - o.y) / self.scale(),
        }
    }
    pub fn pan_by(&mut self, delta: Point) {
        self.pan.x += delta.x;
        self.pan.y += delta.y;
        self.constrain();
    }
    fn constrain(&mut self) {
        let extent_x = WORLD_WIDTH * self.scale() / 2.;
        let extent_y = WORLD_HEIGHT * self.scale() / 2.;
        self.pan.x = self.pan.x.clamp(-extent_x, extent_x);
        self.pan.y = self.pan.y.clamp(-extent_y, extent_y);
    }
    pub fn zoom_at(&mut self, factor: f64, point: Point) {
        let world = self.inverse(point);
        self.zoom = (self.zoom * factor).clamp(1., 4.);
        let next = self.project(world);
        self.pan_by(Point {
            x: point.x - next.x,
            y: point.y - next.y,
        });
    }
    pub fn reveal(&mut self, point: Point, top: f64, bottom: f64) {
        let p = self.project(point);
        let left = 40.;
        let right = (self.viewport.width - 40.).max(left);
        let lower = (self.viewport.height - bottom).max(top);
        self.pan_by(Point {
            x: p.x.clamp(left, right) - p.x,
            y: p.y.clamp(top, lower) - p.y,
        });
    }
    /// Reveal the complete furniture/footprint within chrome and drawer exclusions.
    /// Oversized bounds zoom out only as needed; at minimum zoom, center any axis that still cannot fit.
    pub fn reveal_bounds(&mut self, bounds: FurnitureBounds, top: f64, bottom: f64) {
        let left = 40.;
        let right = (self.viewport.width - 40.).max(left + 1.);
        let lower = (self.viewport.height - bottom).max(top + 1.);
        let width = (bounds.max.x - bounds.min.x).max(1.);
        let height = (bounds.max.y - bounds.min.y).max(1.);
        let factor = ((right - left) / (width * self.scale()))
            .min((lower - top) / (height * self.scale()))
            .min(1.);
        self.zoom = (self.zoom * factor).clamp(1., 4.);
        let min = self.project(bounds.min);
        let max = self.project(bounds.max);
        fn delta(min: f64, max: f64, lower: f64, upper: f64) -> f64 {
            if max - min > upper - lower {
                (lower + upper - min - max) / 2.
            } else if min < lower {
                lower - min
            } else if max > upper {
                upper - max
            } else {
                0.
            }
        }
        self.pan_by(Point {
            x: delta(min.x, max.x, left, right),
            y: delta(min.y, max.y, top, lower),
        });
    }
    pub fn resize(&mut self, viewport: Size) {
        self.viewport = viewport;
        self.constrain();
    }
    pub fn reset(&mut self) {
        self.zoom = 1.;
        self.pan = Point::default();
    }
}
pub fn pick(camera: HomeCamera, screen: Point) -> Option<GridCell> {
    let p = camera.inverse(screen);
    let dx = p.x - FLOOR_ORIGIN.x;
    let dy = p.y - FLOOR_ORIGIN.y;
    let det = 86. * 38. + 94. * 42.;
    let x = (38. * dx + 94. * dy) / det;
    let y = (-42. * dx + 86. * dy) / det;
    if !x.is_finite() || !y.is_finite() || !(0. ..8.).contains(&x) || !(0. ..6.).contains(&y) {
        None
    } else {
        Some(GridCell {
            x: x.floor() as i16,
            y: y.floor() as i16,
        })
    }
}
#[derive(Clone, Debug)]
pub struct SpriteLayout {
    pub path: String,
    pub width: f64,
    pub height: f64,
    pub anchor: Point,
    pub mirror: bool,
    pub visible_width: f64,
}
pub fn sprite_layout(id: &str, direction: Direction) -> SpriteLayout {
    let back = matches!(direction, Direction::South | Direction::West);
    let mirror = matches!(direction, Direction::East | Direction::West);
    let (iw, ih, left, right, ax, ay, target) = match (id, back) {
        ("armchair", false) => (1254., 1254., 81., 1218., 0.51, 0.79, 145.),
        ("armchair", true) => (1254., 1254., 29., 1225., 0.51, 0.77, 145.),
        ("coffee-table", false) => (1536., 1024., 98., 1443., 0.50, 0.70, 230.),
        ("coffee-table", true) => (1536., 1024., 81., 1466., 0.50, 0.71, 230.),
        ("bookcase", false) => (1536., 1024., 290., 1264., 0.50, 0.83, 220.),
        ("bookcase", true) => (1536., 1024., 198., 1344., 0.50, 0.86, 220.),
        ("floor-lamp", _) => (1254., 1254., 413., 842., 0.50, 0.89, 62.),
        ("fern", _) => (1254., 1254., 50., 1220., 0.50, 0.85, 110.),
        _ => (1858., 846., 122., 1752., 0.50, 0.51, 330.),
    };
    let width = target / ((right - left) / iw);
    let height = width * ih / iw;
    let directional = matches!(id, "armchair" | "coffee-table" | "bookcase");
    let suffix = if back { "back" } else { "front" };
    SpriteLayout {
        path: if directional {
            format!("/assets/authoring/{id}-{suffix}.png")
        } else {
            format!("/assets/authoring/{id}.png")
        },
        width,
        height,
        anchor: Point {
            x: width * if mirror { 1. - ax } else { ax },
            y: height * ay,
        },
        mirror: directional && mirror,
        visible_width: target,
    }
}

/// Nearby room actions remain below fixed chrome and above the real scene/drawer boundary.
pub fn owned_action_position(camera: HomeCamera, ground: Point) -> Point {
    let point = camera.project(ground);
    crate::geometry::clamp_overlay(
        Point {
            x: point.x + 30.,
            y: point.y - 30.,
        },
        Size {
            width: (camera.viewport.width - 24.).min(330.),
            height: 64.,
        },
        camera.viewport,
        if camera.viewport.width <= 700. {
            200.
        } else {
            160.
        },
        20.,
    )
}
