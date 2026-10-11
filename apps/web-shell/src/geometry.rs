//! Camera geometry shared by scene imagery, semantic picks, and anchored overlays.
use wonderland_contracts::Anchor;

pub const WORLD_WIDTH: f64 = 1672.0;
pub const WORLD_HEIGHT: f64 = 941.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub viewport: Size,
    pub zoom: f64,
    pub pan: Point,
}

impl Camera {
    pub fn new(viewport: Size) -> Self {
        Self {
            viewport,
            zoom: 1.0,
            pan: Point::default(),
        }
    }
    pub fn scale(self) -> f64 {
        (self.viewport.width / WORLD_WIDTH).max(self.viewport.height / WORLD_HEIGHT) * self.zoom
    }
    pub fn origin(self) -> Point {
        Point {
            x: (self.viewport.width - WORLD_WIDTH * self.scale()) / 2.0 + self.pan.x,
            y: (self.viewport.height - WORLD_HEIGHT * self.scale()) / 2.0 + self.pan.y,
        }
    }
    pub fn project(self, anchor: Anchor) -> Point {
        let origin = self.origin();
        Point {
            x: origin.x + f64::from(anchor.x) * WORLD_WIDTH * self.scale(),
            y: origin.y + f64::from(anchor.y) * WORLD_HEIGHT * self.scale(),
        }
    }
    pub fn pan_by(&mut self, delta: Point) {
        self.pan.x += delta.x;
        self.pan.y += delta.y;
        self.constrain();
    }
    pub fn zoom_at(&mut self, factor: f64, point: Point) {
        let old_zoom = self.zoom;
        self.zoom = (self.zoom * factor).clamp(1.0, 2.8);
        let ratio = self.zoom / old_zoom;
        self.pan.x = (self.pan.x + self.viewport.width / 2.0 - point.x) * ratio + point.x
            - self.viewport.width / 2.0;
        self.pan.y = (self.pan.y + self.viewport.height / 2.0 - point.y) * ratio + point.y
            - self.viewport.height / 2.0;
        self.constrain();
    }
    pub fn focus(&mut self, anchor: Anchor) {
        let point = self.project(anchor);
        self.pan_by(Point {
            x: self.viewport.width / 2.0 - point.x,
            y: self.viewport.height / 2.0 - point.y,
        });
    }
    /// Recenter focused picks hidden outside the view or beneath fixed chrome.
    pub fn reveal(&mut self, anchor: Anchor, top: f64, bottom: f64) {
        let point = self.project(anchor);
        if point.x < 60.0
            || point.x > self.viewport.width - 60.0
            || point.y < top
            || point.y > self.viewport.height - bottom
        {
            self.focus(anchor);
        }
    }
    pub fn resize(&mut self, viewport: Size) {
        self.viewport = viewport;
        self.constrain();
    }
    pub fn reset(&mut self) {
        self.zoom = 1.0;
        self.pan = Point::default();
    }

    fn constrain(&mut self) {
        let x = ((WORLD_WIDTH * self.scale() - self.viewport.width) / 2.0).max(0.0);
        let y = ((WORLD_HEIGHT * self.scale() - self.viewport.height) / 2.0).max(0.0);
        self.pan.x = self.pan.x.clamp(-x, x);
        self.pan.y = self.pan.y.clamp(-y, y);
    }
}

pub fn clamp_overlay(preferred: Point, size: Size, viewport: Size, top: f64, bottom: f64) -> Point {
    Point {
        x: preferred
            .x
            .clamp(12.0, (viewport.width - size.width - 12.0).max(12.0)),
        y: preferred
            .y
            .clamp(top, (viewport.height - bottom - size.height).max(top)),
    }
}
