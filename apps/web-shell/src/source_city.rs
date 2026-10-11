//! Source city channels in native tile units, shared camera and depth rasterization.
//! Original content: TSOClient/FSO.Content.TSO/Content/Cities/city_0100.
use wonderland_render_3d::{
    camera::CityCamera,
    city::{
        CityBoundary, CityMap, CityPixel, TerrainClass, in_bounds, terrain_class, unpack_location,
    },
};
use wonderland_render_core::reference::{FragmentOptions, ReferenceSurface};
use wonderland_render_core::{Mat4, Mesh, RenderLimits, Vec2, Vec3, Vertex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CityMapLot {
    pub lot_id: u32,
    pub location: u32,
    pub name: String,
    pub online: Option<u32>,
}

/// One measured viewport owns the sampling size, projection and pointer scale.
/// The focus rectangle leaves room for the existing city controls and directory;
/// it shifts the camera projection without stretching or rescaling source tiles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CityViewport {
    pub width: u32,
    pub height: u32,
    css_width: f64,
    css_height: f64,
    focus: [f64; 4],
}
impl CityViewport {
    pub fn new(css_width: f64, css_height: f64) -> Option<Self> {
        if !css_width.is_finite() || !css_height.is_finite() || css_width <= 0. || css_height <= 0.
        {
            return None;
        }
        // Bound CPU rasterization, not the original city or its directory.
        let scale = (393_216. / css_width / css_height)
            .sqrt()
            .min(1.)
            .min(960. / css_width)
            .min(960. / css_height);
        Some(Self {
            width: (css_width * scale).floor().max(1.) as u32,
            height: (css_height * scale).floor().max(1.) as u32,
            css_width,
            css_height,
            focus: [0., 0., css_width, css_height],
        })
    }
    pub fn with_focus(mut self, left: f64, top: f64, width: f64, height: f64) -> Self {
        if [left, top, width, height].iter().any(|v| !v.is_finite()) || width <= 0. || height <= 0.
        {
            return self;
        }
        let right = (left + width).clamp(0., self.css_width);
        let bottom = (top + height).clamp(0., self.css_height);
        let left = left.clamp(0., self.css_width);
        let top = top.clamp(0., self.css_height);
        if right > left && bottom > top {
            self.focus = [left, top, right - left, bottom - top];
        }
        self
    }
    pub fn projection(self, camera: CityCamera) -> Option<Mat4> {
        let [left, top, width, height] = self.focus;
        let offset = Vec3::new(
            ((left + width / 2.) / self.css_width * 2. - 1.) as f32,
            (1. - (top + height / 2.) / self.css_height * 2.) as f32,
            0.,
        );
        Some(Mat4::from_translation(offset) * camera_matrix(camera, self.width, self.height)?)
    }
    pub fn pan(self, camera: CityCamera, dx: f64, dy: f64) -> CityCamera {
        pan_camera(
            camera,
            (dx * f64::from(self.width) / self.css_width) as f32,
            (dy * f64::from(self.height) / self.css_height) as f32,
            self.width,
            self.height,
        )
    }
    fn contains_focus(self, point: Vec2) -> bool {
        let [left, top, width, height] = self.focus;
        // A compact phone map still keeps a small visible edge without forcing
        // an even more distant camera merely to preserve a desktop-size margin.
        let margin = 12_f64.min(width / 8.).min(height / 16.);
        let x = f64::from(point.x) * self.css_width / f64::from(self.width);
        let y = f64::from(point.y) * self.css_height / f64::from(self.height);
        x >= left + margin
            && x <= left + width - margin
            && y >= top + margin
            && y <= top + height - margin
    }
}
/// Shard.map is the original numeric city identifier; paths are never accepted.
pub fn supported_map(name: &str) -> bool {
    matches!(name.trim(), "100" | "0100" | "city_0100")
}
pub fn tile_coordinates(location: u32) -> Option<(u16, u16)> {
    let (x, y) = unpack_location(location);
    in_bounds(i32::from(x), i32::from(y), CityBoundary::RendererDiamond).then_some((x, y))
}
/// Projection uses precisely the matrix submitted to ReferenceSurface.
pub fn project_point(matrix: Mat4, point: Vec3, width: u32, height: u32) -> Option<Vec2> {
    let p = matrix.transform_vec4([point.x, point.y, point.z, 1.]);
    if width == 0
        || height == 0
        || p.iter().any(|v| !v.is_finite())
        || p[3] <= 0.
        || p[2] < 0.
        || p[2] > p[3]
    {
        return None;
    }
    Some(Vec2::new(
        (p[0] / p[3] + 1.) * 0.5 * width as f32,
        (1. - p[1] / p[3]) * 0.5 * height as f32,
    ))
}
pub fn overview_camera() -> CityCamera {
    CityCamera {
        center: Vec2::new(256., 256.),
        zoom: 19.,
        target_zoom: 19.,
        cam_height: 4.,
        ..Default::default()
    }
}
pub fn camera_matrix(camera: CityCamera, width: u32, height: u32) -> Option<Mat4> {
    if width == 0 || height == 0 {
        return None;
    }
    let mut pose = camera.pose().ok()?;
    // Overview must encompass the full 512-tile city, beyond the near-lot default.
    pose.far = 2000_f32.max((pose.position - pose.target).length() + 1024.);
    pose.view_projection(width as f32 / height as f32).ok()
}
pub fn overview_matrix(width: u32, height: u32) -> Option<Mat4> {
    camera_matrix(overview_camera(), width, height)
}
#[derive(Clone, Debug)]
pub struct SourceCity {
    pub map: CityMap,
    pub vertex_colors: Vec<[u8; 4]>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CityPin {
    pub lot_id: u32,
    pub location: u32,
    pub point: Vec2,
}
impl SourceCity {
    /// Fit the original terrain, including its edge tiles, into the visible map
    /// area when the player explicitly asks to see the whole city.
    pub fn overview(&self, viewport: CityViewport) -> CityCamera {
        let mut boundary = Vec::new();
        for y in (0..512_u16).step_by(4) {
            let columns: Vec<_> = (0..512_u16)
                .step_by(4)
                .filter(|x| in_bounds(i32::from(*x), i32::from(y), CityBoundary::RendererDiamond))
                .collect();
            for x in [columns.first(), columns.last()].into_iter().flatten() {
                boundary.extend(self.tile_vertices(*x, y, 4).into_iter().map(|v| v.position));
            }
        }
        let mut camera = overview_camera();
        let (mut low, mut high) = (6., 60.);
        for _ in 0..18 {
            camera.zoom = (low + high) / 2.;
            camera.target_zoom = camera.zoom;
            let fits = viewport.projection(camera).is_some_and(|matrix| {
                boundary.iter().all(|position| {
                    project_point(matrix, *position, viewport.width, viewport.height)
                        .is_some_and(|point| viewport.contains_focus(point))
                })
            });
            if fits {
                high = camera.zoom;
            } else {
                low = camera.zoom;
            }
        }
        camera.zoom = high;
        camera.target_zoom = high;
        camera
    }
    /// Channels in source load order: elevation, terrain, forest type, density,
    /// road flags, vertex color. Browser PNG decoding never rescales these images.
    pub fn from_rgba(channels: &[Vec<u8>]) -> Result<Self, String> {
        const SIZE: usize = 512 * 512 * 4;
        if channels.len() != 6 || channels.iter().any(|c| c.len() != SIZE) {
            return Err("City terrain channels must each be 512 × 512 RGBA pixels.".into());
        }
        let rgba =
            |c: usize, i: usize| -> [u8; 4] { channels[c][i * 4..i * 4 + 4].try_into().unwrap() };
        let pixels = (0..512 * 512)
            .map(|i| CityPixel {
                elevation: channels[0][i * 4],
                terrain: rgba(1, i),
                forest: rgba(2, i),
                density: channels[3][i * 4],
                road: channels[4][i * 4],
            })
            .collect();
        Ok(Self {
            map: CityMap {
                width: 512,
                height: 512,
                pixels,
            },
            vertex_colors: (0..512 * 512).map(|i| rgba(5, i)).collect(),
        })
    }
    fn sample_x(x: u16, y: u16) -> u16 {
        let y = i32::from(y);
        let start = (y - 306).abs();
        let end = if y < 205 { 307 + y } else { 717 - y };
        i32::from(x).clamp(start, end).clamp(0, 511) as u16
    }
    fn position(&self, x: u16, y: u16) -> Vec3 {
        let sample_y = y.min(511);
        let sample = Self::sample_x(x.min(511), sample_y);
        Vec3::new(
            x as f32,
            self.map.pixels[sample_y as usize * 512 + sample as usize].elevation as f32 / 12.,
            y as f32,
        )
    }
    pub fn tile_vertices(&self, x: u16, y: u16, step: u16) -> Vec<Vertex> {
        [(x, y), (x + step, y), (x + step, y + step), (x, y + step)]
            .into_iter()
            .map(|(xx, yy)| {
                let pos = self.position(xx, yy);
                let i =
                    yy.min(511) as usize * 512 + Self::sample_x(xx.min(511), yy.min(511)) as usize;
                let rgba = self.vertex_colors[i];
                Vertex {
                    position: pos,
                    normal: Vec3::Y,
                    uv: Vec2::ZERO,
                    color: rgba.map(|v| v as f32 / 255.),
                }
            })
            .collect()
    }
    pub fn lot_position(&self, location: u32) -> Option<Vec3> {
        let (x, y) = tile_coordinates(location)?;
        let corners = self.tile_vertices(x, y, 1);
        Some(Vec3::new(
            x as f32 + 0.5,
            corners.iter().map(|v| v.position.y).sum::<f32>() / 4. + 0.08,
            y as f32 + 0.5,
        ))
    }
    pub fn pin(&self, lot: &CityMapLot, matrix: Mat4, width: u32, height: u32) -> Option<CityPin> {
        let point = project_point(matrix, self.lot_position(lot.location)?, width, height)?;
        (point.x >= 0. && point.x <= width as f32 && point.y >= 0. && point.y <= height as f32)
            .then_some(CityPin {
                lot_id: lot.lot_id,
                location: lot.location,
                point,
            })
    }
    /// LOD samples original elevation/color vertices; culling shares pin projection.
    /// The software surface performs triangle clipping and strict depth testing.
    pub fn render(
        &self,
        camera: CityCamera,
        width: u32,
        height: u32,
    ) -> Result<ReferenceSurface, String> {
        let matrix = camera_matrix(camera, width, height).ok_or("Invalid city camera.")?;
        self.render_projected(camera, matrix, width, height)
    }
    pub fn render_in_viewport(
        &self,
        camera: CityCamera,
        viewport: CityViewport,
    ) -> Result<ReferenceSurface, String> {
        let matrix = viewport.projection(camera).ok_or("Invalid city camera.")?;
        self.render_projected(camera, matrix, viewport.width, viewport.height)
    }
    fn render_projected(
        &self,
        camera: CityCamera,
        matrix: Mat4,
        width: u32,
        height: u32,
    ) -> Result<ReferenceSurface, String> {
        let step: u16 = if camera.zoom >= 17. {
            4
        } else if camera.zoom >= 11. {
            2
        } else {
            1
        };
        let mut mesh = Mesh {
            vertices: vec![],
            indices: vec![],
        };
        for y in (0..512).step_by(step as usize) {
            for x in (0..512).step_by(step as usize) {
                if !in_bounds(x as i32, y as i32, CityBoundary::RendererDiamond) {
                    continue;
                }
                let pixel = self.map.pixels[y as usize * 512 + x as usize];
                if terrain_class(pixel.terrain) == TerrainClass::Void {
                    continue;
                }
                let vertices = self.tile_vertices(x, y, step);
                let projections: Vec<_> = vertices
                    .iter()
                    .filter_map(|v| project_point(matrix, v.position, width, height))
                    .collect();
                if projections.is_empty()
                    || projections.iter().all(|p| p.x < -8.)
                    || projections.iter().all(|p| p.x > width as f32 + 8.)
                    || projections.iter().all(|p| p.y < -8.)
                    || projections.iter().all(|p| p.y > height as f32 + 8.)
                {
                    continue;
                }
                let base = mesh.vertices.len() as u32;
                mesh.vertices.extend(vertices);
                mesh.indices
                    .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        let limits = RenderLimits::default();
        let mut surface =
            ReferenceSurface::new(width, height, &limits).map_err(|e| e.to_string())?;
        surface.clear([31, 54, 57, 255]);
        if !mesh.indices.is_empty() {
            surface
                .draw_mesh(&mesh, matrix, None, FragmentOptions::default(), &limits)
                .map_err(|e| e.to_string())?;
        }
        Ok(surface)
    }
}

pub fn pan_camera(mut camera: CityCamera, dx: f32, dy: f32, width: u32, height: u32) -> CityCamera {
    if !dx.is_finite() || !dy.is_finite() {
        return camera;
    }
    let Some(matrix) = camera_matrix(camera, width, height) else {
        return camera;
    };
    let p = Vec3::new(camera.center.x, camera.cam_height + 0.5, camera.center.y);
    let Some(origin) = project_point(matrix, p, width, height) else {
        return camera;
    };
    let Some(px) = project_point(matrix, p + Vec3::X, width, height) else {
        return camera;
    };
    let Some(pz) = project_point(matrix, p + Vec3::Z, width, height) else {
        return camera;
    };
    let a = px - origin;
    let b = pz - origin;
    let det = a.x * b.y - b.x * a.y;
    if det.abs() > 0.000001 {
        camera.center.x = (camera.center.x - (dx * b.y - b.x * dy) / det).clamp(0., 511.);
        camera.center.y = (camera.center.y - (a.x * dy - dx * a.y) / det).clamp(0., 511.);
    }
    camera
}
#[cfg(test)]
mod tests {
    use super::*;
    use wonderland_render_3d::city::{pack_location, unpack_location};
    #[test]
    fn supports_only_the_available_source_map() {
        for name in ["100", "0100", "city_0100"] {
            assert!(supported_map(name), "{name}");
        }
        for name in ["1", "1000", "city_0001", "sunset", "../city_0100", ""] {
            assert!(!supported_map(name));
        }
    }
    #[test]
    fn packed_location_keeps_high_word_x_and_low_word_y() {
        let packed = pack_location(184, 328);
        assert_eq!(packed, 12_058_952);
        assert_eq!(tile_coordinates(packed), Some((184, 328)));
        assert_eq!(unpack_location(packed), (184, 328));
    }
    #[test]
    fn invalid_coordinates_are_not_clamped_to_an_unrelated_lot() {
        for packed in [
            pack_location(65535, 328),
            pack_location(184, 512),
            pack_location(0, 0),
        ] {
            assert_eq!(tile_coordinates(packed), None);
        }
    }
    #[test]
    fn original_channel_assets_really_are_512_square() {
        for bytes in [
            include_bytes!("../public/assets/cities/city_0100/elevation.png").as_slice(),
            include_bytes!("../public/assets/cities/city_0100/vertexcolor.png").as_slice(),
            include_bytes!("../public/assets/cities/city_0100/terraintype.png").as_slice(),
            include_bytes!("../public/assets/cities/city_0100/roadmap.png").as_slice(),
            include_bytes!("../public/assets/cities/city_0100/forestdensity.png").as_slice(),
            include_bytes!("../public/assets/cities/city_0100/foresttype.png").as_slice(),
        ] {
            assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
            assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 512);
            assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 512);
        }
    }
    #[test]
    fn decoded_channels_reject_mismatched_dimensions() {
        assert!(SourceCity::from_rgba(&vec![vec![0; 4]; 6]).is_err());
    }
    #[test]
    fn lot_identity_and_spatial_location_are_separate() {
        let city = test_city();
        let lot = CityMapLot {
            lot_id: 42,
            location: pack_location(184, 328),
            name: "Actual place".into(),
            online: None,
        };
        let matrix = overview_matrix(800, 400).unwrap();
        let pin = city.pin(&lot, matrix, 800, 400).expect("valid actual tile");
        assert_eq!(pin.lot_id, 42);
        assert_eq!(pin.location, pack_location(184, 328));
    }
    fn test_city() -> SourceCity {
        let n = 512 * 512 * 4;
        let channels = vec![
            vec![0; n],
            vec![0; n],
            vec![0; n],
            vec![0; n],
            vec![0; n],
            vec![0; n],
        ];
        SourceCity::from_rgba(&channels).unwrap()
    }
    #[test]
    fn mesh_positions_use_source_elevation_units() {
        let mut city = test_city();
        for pixel in &mut city.map.pixels {
            pixel.elevation = 120;
            pixel.terrain = [0, 255, 0, 255];
        }
        let expected = wonderland_render_3d::city::build_near_patch_parts(
            &city.map,
            (184, 328),
            (1, 1),
            1,
            wonderland_render_3d::city::CityBoundary::RendererDiamond,
            100,
        )
        .unwrap();
        let actual = city.tile_vertices(184, 328, 1);
        assert_eq!(actual.len(), 4);
        let points: Vec<_> = expected[0]
            .mesh
            .vertices
            .iter()
            .map(|v| v.position)
            .collect();
        for v in actual {
            assert!(points.contains(&v.position), "{:?}", v.position);
        }
    }
    #[test]
    fn last_city_row_preserves_geometric_extent_while_clamping_samples() {
        let city = test_city();
        let vertices = city.tile_vertices(205, 511, 1);
        assert_eq!(vertices[2].position.z, 512.);
    }
    #[test]
    fn pan_moves_the_projected_city_in_the_drag_direction() {
        let camera = overview_camera();
        let point = Vec3::new(256., 4.5, 256.);
        let before =
            project_point(camera_matrix(camera, 800, 400).unwrap(), point, 800, 400).unwrap();
        let moved = pan_camera(camera, 32., -16., 800, 400);
        let after =
            project_point(camera_matrix(moved, 800, 400).unwrap(), point, 800, 400).unwrap();
        assert!((after.x - before.x - 32.).abs() < 2. && (after.y - before.y + 16.).abs() < 2.);
    }
    #[test]
    fn source_camera_and_pin_projection_share_the_same_matrix() {
        let camera = overview_camera();
        let matrix = camera.pose().unwrap().view_projection(2.).unwrap();
        let target = camera.pose().unwrap().target;
        let pin = project_point(matrix, target, 800, 400).expect("camera center is visible");
        assert!((pin.x - 400.).abs() < 0.01 && (pin.y - 200.).abs() < 0.01);
        assert!(project_point(matrix, Vec3::new(f32::NAN, 0., 0.), 800, 400).is_none());
    }

    #[test]
    fn viewport_sampling_preserves_portrait_and_landscape_aspects_with_bounded_memory() {
        for (width, height) in [(390., 844.), (1364., 936.), (844., 390.), (3840., 2160.)] {
            let viewport = CityViewport::new(width, height).unwrap();
            assert!(viewport.width * viewport.height <= 393_216);
            assert!(viewport.width <= 960 && viewport.height <= 960);
            let scale_x = f64::from(viewport.width) / width;
            let scale_y = f64::from(viewport.height) / height;
            assert!((scale_x - scale_y).abs() <= 1. / width.min(height));
        }
        let portrait = CityViewport::new(390., 844.).unwrap();
        assert_eq!((portrait.width, portrait.height), (390, 844));
        for (width, height) in [
            (0., 800.),
            (390., 0.),
            (-1., 800.),
            (f64::NAN, 800.),
            (390., f64::INFINITY),
        ] {
            assert!(CityViewport::new(width, height).is_none());
        }
    }

    #[test]
    fn selected_source_location_projects_into_the_unobscured_city_area() {
        let city = test_city();
        let viewport = CityViewport::new(390., 844.)
            .unwrap()
            .with_focus(12., 205., 366., 174.);
        let position = city.lot_position(pack_location(184, 328)).unwrap();
        let mut camera = overview_camera();
        camera.center = Vec2::new(position.x, position.z);
        camera.cam_height = position.y - 0.5;
        let matrix = viewport.projection(camera).unwrap();
        let point = project_point(matrix, position, viewport.width, viewport.height).unwrap();
        assert!((point.x - 195.).abs() < 0.01);
        assert!((point.y - 292.).abs() < 0.01);
        let lot = CityMapLot {
            lot_id: 73,
            location: pack_location(184, 328),
            name: "Source lot".into(),
            online: None,
        };
        assert_eq!(
            city.pin(&lot, matrix, viewport.width, viewport.height)
                .unwrap()
                .point,
            point
        );
    }

    #[test]
    fn css_pointer_drag_stays_aligned_after_portrait_landscape_resize() {
        for (width, height) in [(390., 844.), (1364., 936.), (844., 390.)] {
            let viewport = CityViewport::new(width, height).unwrap().with_focus(
                width * 0.2,
                height * 0.15,
                width * 0.7,
                height * 0.5,
            );
            let camera = overview_camera();
            let position = camera.pose().unwrap().target;
            let project = |camera| {
                let point = project_point(
                    viewport.projection(camera).unwrap(),
                    position,
                    viewport.width,
                    viewport.height,
                )
                .unwrap();
                (
                    f64::from(point.x) * width / f64::from(viewport.width),
                    f64::from(point.y) * height / f64::from(viewport.height),
                )
            };
            let before = project(camera);
            let after = project(viewport.pan(camera, 18., -12.));
            assert!(
                (after.0 - before.0 - 18.).abs() < 1.5,
                "{width} × {height}: {before:?} → {after:?}"
            );
            assert!(
                (after.1 - before.1 + 12.).abs() < 1.5,
                "{width} × {height}: {before:?} → {after:?}"
            );
        }
    }

    #[test]
    fn whole_city_fits_original_edge_tiles_above_or_beside_the_directory() {
        let mut city = test_city();
        for pixel in &mut city.map.pixels {
            pixel.elevation = 120;
        }
        for viewport in [
            CityViewport::new(320., 600.)
                .unwrap()
                .with_focus(12., 205., 296., 40.),
            CityViewport::new(390., 844.)
                .unwrap()
                .with_focus(12., 205., 366., 174.),
            CityViewport::new(1364., 936.)
                .unwrap()
                .with_focus(389., 192., 953., 620.),
        ] {
            let camera = city.overview(viewport);
            let matrix = viewport.projection(camera).unwrap();
            for (x, y) in [(304, 4), (508, 204), (208, 508), (4, 304)] {
                assert!(in_bounds(x.into(), y.into(), CityBoundary::RendererDiamond));
                for vertex in city.tile_vertices(x, y, 4) {
                    let point =
                        project_point(matrix, vertex.position, viewport.width, viewport.height)
                            .expect("source edge survives the camera's depth range");
                    assert!(viewport.contains_focus(point), "{viewport:?}: {point:?}");
                }
            }
        }
    }

    #[test]
    fn source_pin_lands_on_the_same_rasterized_terrain_after_viewport_resize() {
        let mut city = test_city();
        for pixel in &mut city.map.pixels {
            pixel.terrain = [0, 255, 0, 255];
        }
        city.vertex_colors.fill([103, 151, 82, 255]);
        let lot = CityMapLot {
            lot_id: 42,
            location: pack_location(184, 328),
            name: "Actual terrain".into(),
            online: None,
        };
        for viewport in [
            CityViewport::new(390., 844.)
                .unwrap()
                .with_focus(12., 205., 366., 174.),
            CityViewport::new(1364., 936.)
                .unwrap()
                .with_focus(389., 192., 953., 620.),
        ] {
            let camera = city.overview(viewport);
            let surface = city.render_in_viewport(camera, viewport).unwrap();
            let pin = city
                .pin(
                    &lot,
                    viewport.projection(camera).unwrap(),
                    viewport.width,
                    viewport.height,
                )
                .unwrap();
            let image = surface.image();
            assert_eq!(
                (image.width, image.height),
                (viewport.width, viewport.height)
            );
            let pixel =
                image.pixels[pin.point.y as usize * image.width as usize + pin.point.x as usize];
            assert_ne!(
                pixel,
                [31, 54, 57, 255],
                "source pin must hit terrain, not the empty canvas"
            );
        }
    }
}
