//! VMEntity.cs:339–406 at FreeSO 4c6b3e8f5835. Values come from the displayed
//! source camera and entity; no renderer/frame identity becomes an audio cue.
#[derive(Clone, Copy, Debug)]
pub enum CameraAudio {
    Iso {
        screen_offset: [f32; 2],
        world_pixel_width: f32,
        precise_zoom: f32,
        zoom: u8,
    },
    /// VisualPosition uses original tile axes, camera uses original graphics axes.
    ThreeD {
        visual_position: [f32; 3],
        target: [f32; 3],
        position: [f32; 3],
        zoom_3d: f32,
        precise_zoom: f32,
    },
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn length(a: [f32; 3]) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
fn unit(a: [f32; 3]) -> [f32; 3] {
    let n = length(a);
    if n == 0. {
        [0.; 3]
    } else {
        a.map(|v| v / n)
    }
}
pub fn source_gain_pan(
    camera: CameraAudio,
    entity_level: i8,
    view_level: i8,
    no_pan: bool,
    no_zoom: bool,
) -> Result<(f32, f32), &'static str> {
    let (mut gain, pan) = match camera {
        CameraAudio::Iso {
            screen_offset: [x, y],
            world_pixel_width: w,
            precise_zoom,
            zoom,
        } => {
            if ![x, y, w, precise_zoom].iter().all(|v| v.is_finite())
                || w <= 0.
                || precise_zoom < 0.
                || !(1..=3).contains(&zoom)
            {
                return Err("invalid original 2D camera");
            }
            let p = if no_pan { 0. } else { (x / w).clamp(-1., 1.) };
            let mut g = if no_pan {
                1.
            } else {
                1. - ((x * x + y * y).sqrt() / w).clamp(0., 1.)
            };
            g *= precise_zoom;
            if !no_zoom {
                g /= 4. - f32::from(zoom);
            }
            (g, p * p * p.signum())
        }
        CameraAudio::ThreeD {
            visual_position: v,
            target,
            position,
            zoom_3d,
            precise_zoom,
        } => {
            if !v
                .iter()
                .chain(target.iter())
                .chain(position.iter())
                .chain([zoom_3d, precise_zoom].iter())
                .all(|v| v.is_finite())
                || precise_zoom < 0.
            {
                return Err("invalid original 3D camera");
            }
            let source = [v[0] * 3., v[2] * 3., v[1] * 3.];
            let mut delta = sub(target, source);
            delta[2] /= 3.;
            let gain =
                (1.5 - length(delta) / 40.) * (10. / (zoom_3d * zoom_3d + 10.)) * precise_zoom;
            let look = unit(sub(target, position));
            let side = unit([-look[2], 0., look[0]]); // Cross(lookAt,Vector3.Up)
            let heading = unit(sub(source, position));
            let p = side[0] * heading[0] + side[1] * heading[1] + side[2] * heading[2];
            (
                gain,
                if no_pan {
                    0.
                } else {
                    p.abs().powf(2.25) * p.signum()
                },
            )
        }
    };
    if entity_level > view_level {
        gain /= 4.;
    } else if entity_level != view_level {
        gain /= 2.;
    }
    Ok((gain.clamp(0., 1.), pan.clamp(-1., 1.)))
}
