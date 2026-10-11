#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaPass {
    Basic,
    ColorDepth,
    Wall,
    ObjectIdDepth,
    ObjectIdSimple,
    Restore,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GammaMode {
    Basic,
    Advanced,
}

pub fn alpha_survives(alpha: u8, pass: AlphaPass) -> bool {
    match pass {
        AlphaPass::Basic | AlphaPass::ObjectIdSimple => alpha != 0,
        AlphaPass::ColorDepth | AlphaPass::Wall | AlphaPass::Restore => alpha > 2,
        AlphaPass::ObjectIdDepth => alpha >= 26,
    }
}

/// Returns premultiplied RGB. Restore input is already premultiplied and bypasses
/// lighting. ID passes use this coverage predicate but bind a separate ID color.
/// Automatic engine sRGB conversion must be disabled for this transfer policy.
pub fn shade_fragment(
    color: [u8; 4],
    mask: Option<u8>,
    light: [f32; 3],
    room: u16,
    gamma: GammaMode,
    pass: AlphaPass,
) -> Option<[f32; 4]> {
    if light.iter().any(|x| !x.is_finite() || *x < 0.0) {
        return None;
    }
    let alpha = if pass == AlphaPass::Wall {
        mask?
    } else {
        color[3]
    };
    if !alpha_survives(alpha, pass) {
        return None;
    }
    let a = f32::from(alpha) / 255.0;
    let mut rgb = [
        f32::from(color[0]) / 255.0,
        f32::from(color[1]) / 255.0,
        f32::from(color[2]) / 255.0,
    ];
    if pass == AlphaPass::Restore {
        return Some([rgb[0], rgb[1], rgb[2], a]);
    }
    if pass == AlphaPass::Wall {
        for i in 0..3 {
            rgb[i] = gamma_multiply(rgb[i], light[i], gamma);
        }
    } else {
        match room {
            65534 => {
                rgb = rgb.map(|x| 1.0 - x);
            }
            65533 => {
                let gray = rgb[0] * 0.2989 + rgb[1] * 0.587 + rgb[2] * 0.114;
                rgb = [gray; 3];
            }
            65535 => {}
            _ if room % 256 == 0 => {}
            _ => {
                for i in 0..3 {
                    rgb[i] = gamma_multiply(rgb[i], light[i], gamma);
                }
            }
        }
    }
    Some([rgb[0] * a, rgb[1] * a, rgb[2] * a, a])
}

// Keep the original shader coefficients verbatim for source parity review.
#[allow(clippy::excessive_precision)]
pub fn gamma_multiply(color: f32, light: f32, mode: GammaMode) -> f32 {
    match mode {
        GammaMode::Basic => (color.powf(2.2) * light).powf(1.0 / 2.2),
        GammaMode::Advanced => {
            let c = color * (color * (color * 0.305306011 + 0.682171111) + 0.012522878) * light;
            let s1 = c.sqrt();
            let s2 = s1.sqrt();
            let s3 = s2.sqrt();
            0.662002687 * s1 + 0.684122060 * s2 - 0.323583601 * s3 - 0.0225411470 * c
        }
    }
}

/// Packed little-endian room-map texel, with the upper room's high bit marking
/// horizontal diagonals. Caller supplies fractional tile coordinates.
pub fn room_at(packed: u32, fraction: [f32; 2]) -> u16 {
    let room1 = packed as u16;
    let upper = (packed >> 16) as u16;
    let room2 = upper & 0x7fff;
    if room1 == room2 {
        room1
    } else if upper > 32767 {
        if fraction[0] + fraction[1] >= 1.0 {
            room2
        } else {
            room1
        }
    } else if fraction[0] - fraction[1] > 0.0 {
        room1
    } else {
        room2
    }
}

/// Wall shadow reduces RGB and alpha; floor/object shadow reduces alpha only.
/// Shadow samples/powers are explicit, prevalidated normalized provider inputs.
pub fn point_light(
    color: [f32; 4],
    distance_over_radius: f32,
    wall: f32,
    floor: f32,
    powers: [f32; 2],
) -> [f32; 4] {
    let c = (1.0 - distance_over_radius).clamp(0.0, 1.0).powf(2.2) * (1.0 - wall * powers[0]);
    let a = c * (1.0 - floor * powers[1]);
    [
        color[0] * c,
        color[1] * c,
        color[2] * c,
        color[3] * a.min(1.0),
    ]
}

/// Source float packing; d=1 wraps to black. It is not a u16 endpoint codec.
pub fn pack_depth(d: f32) -> [f32; 4] {
    let frac = |x: f32| x - x.floor();
    let v = [frac(d), frac(d * 255.0), frac(d * 65025.0), 0.0];
    [v[0] - v[1] / 255.0, v[1] - v[2] / 255.0, v[2], 1.0]
}
pub fn unpack_depth(c: [f32; 4]) -> f32 {
    c[0] + c[1] / 255.0 + c[2] / 65025.0
}
