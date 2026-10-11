//! Engine-neutral conversion arithmetic. Testable with the reference compiler.
pub fn sprite_depth(byte: u8, back: f32, front: f32) -> f32 {
    back + (1.0 - f32::from(byte) / 255.0) / 0.4 * (front - back)
}
/// A transient ID-buffer index. The public mapping supplies the stable EntityRef.
pub fn id_color(id: u32) -> [f32; 4] {
    [
        ((id & 255) as f32) / 255.0,
        (((id >> 8) & 255) as f32) / 255.0,
        (((id >> 16) & 255) as f32) / 255.0,
        1.0,
    ]
}
pub fn physical_to_fixture(x: f64, y: f64, width: f64, height: f64) -> Option<(u32, u32)> {
    if ![x, y, width, height].iter().all(|v| v.is_finite())
        || width <= 0.0
        || height <= 0.0
        || x < 0.0
        || y < 0.0
        || x >= width
        || y >= height
    {
        return None;
    }
    Some((
        (x / width * 640.0).floor() as u32,
        (y / height * 480.0).floor() as u32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_depth_endpoints_and_quarter_range_are_not_normalized_lerp() {
        assert!((sprite_depth(255, 0.8, 0.4) - 0.8).abs() < 1e-6);
        assert!((sprite_depth(153, 0.8, 0.4) - 0.4).abs() < 1e-6);
        assert!((sprite_depth(0, 0.8, 0.4) + 0.2).abs() < 1e-6);
    }
    #[test]
    fn id_pass_bytes_are_little_endian_and_opaque() {
        assert_eq!(
            id_color(0x341205),
            [5.0 / 255.0, 18.0 / 255.0, 52.0 / 255.0, 1.0]
        );
    }
    #[test]
    fn scaled_dom_coordinates_and_exclusive_bounds() {
        assert_eq!(
            physical_to_fixture(100.0, 50.0, 1280.0, 960.0),
            Some((50, 25))
        );
        assert_eq!(physical_to_fixture(1280.0, 0.0, 1280.0, 960.0), None);
        assert_eq!(physical_to_fixture(-1.0, 0.0, 640.0, 480.0), None);
        assert_eq!(physical_to_fixture(f64::NAN, 0.0, 640.0, 480.0), None);
        assert_eq!(physical_to_fixture(1.0, 1.0, 0.0, 480.0), None);
    }
}
