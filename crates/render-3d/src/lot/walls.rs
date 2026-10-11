use super::*;

fn thick(style: u16) -> bool {
    style == 1 || style == 255
}
fn effective(w: &WallAppearance, index: usize) -> u16 {
    if w.object_styles[index] != 0 {
        w.object_styles[index]
    } else {
        w.styles[index]
    }
}
fn neighbor_thick(lot: &VisualLot, p: TileCoord, dx: i32, dy: i32, axis: usize) -> bool {
    lot.get(i32::from(p.x) + dx, i32::from(p.y) + dy, p.level)
        .map(|t| thick(t.wall.styles[axis]) && if axis == 0 { t.wall.west } else { t.wall.north })
        .unwrap_or(false)
}

pub(super) fn build(
    lot: &VisualLot,
    p: TileCoord,
    t: &VisualTile,
    a: &mut Acc<'_>,
) -> Result<(), Error> {
    let w = &t.wall;
    let th = 0.075;
    if w.west {
        if thick(w.styles[0]) {
            let back = if p.y > 0 && !neighbor_thick(lot, p, 0, -1, 0) {
                -th
            } else {
                0.
            };
            let front = if p.y < lot.height - 1 && !neighbor_thick(lot, p, 0, 1, 0) {
                th
            } else {
                0.
            };
            if back != 0. {
                line(
                    lot,
                    p,
                    [th, back + 0.005],
                    [-th, back + 0.005],
                    w.patterns[0],
                    1,
                    5,
                    0.,
                    th * 2.,
                    a,
                )?;
            }
            if front != 0. {
                line(
                    lot,
                    p,
                    [-th, 1. + front - 0.005],
                    [th, 1. + front - 0.005],
                    w.patterns[0],
                    1,
                    5,
                    0.,
                    th * 2.,
                    a,
                )?;
            }
            if p.x > 0 {
                let pat = lot
                    .get(i32::from(p.x) - 1, i32::from(p.y), p.level)
                    .map(|t| t.wall.patterns[3])
                    .unwrap_or(0);
                line(
                    lot,
                    p,
                    [-th, back],
                    [-th, 1. + front],
                    pat,
                    effective(w, 0),
                    5,
                    -back,
                    -(1. + front),
                    a,
                )?;
            }
            line(
                lot,
                p,
                [th, 1. + front],
                [th, back],
                w.patterns[0],
                effective(w, 0),
                0,
                -front,
                1. - back,
                a,
            )?;
        } else {
            line(
                lot,
                p,
                [0., 1.],
                [0., 0.],
                w.patterns[0],
                effective(w, 0),
                4,
                0.,
                1.,
                a,
            )?;
        }
    }
    if w.north {
        if thick(w.styles[1]) {
            let back = if p.x > 0 && !neighbor_thick(lot, p, -1, 0, 1) {
                -th
            } else {
                0.
            };
            let front = if p.x < lot.width - 1 && !neighbor_thick(lot, p, 1, 0, 1) {
                th
            } else {
                0.
            };
            if back != 0. {
                line(
                    lot,
                    p,
                    [back + 0.005, -th],
                    [back + 0.005, th],
                    w.patterns[1],
                    1,
                    5,
                    0.,
                    th * 2.,
                    a,
                )?;
            }
            if front != 0. {
                line(
                    lot,
                    p,
                    [1. + front - 0.005, th],
                    [1. + front - 0.005, -th],
                    w.patterns[1],
                    1,
                    5,
                    0.,
                    th * 2.,
                    a,
                )?;
            }
            if p.y > 0 {
                let pat = lot
                    .get(i32::from(p.x), i32::from(p.y) - 1, p.level)
                    .map(|t| t.wall.patterns[2])
                    .unwrap_or(0);
                line(
                    lot,
                    p,
                    [1. + front, -th],
                    [back, -th],
                    pat,
                    effective(w, 1),
                    5,
                    front,
                    -(1. - back),
                    a,
                )?;
            }
            line(
                lot,
                p,
                [back, th],
                [1. + front, th],
                w.patterns[1],
                effective(w, 1),
                1,
                back,
                1. + front,
                a,
            )?;
        } else {
            line(
                lot,
                p,
                [0., 0.],
                [1., 0.],
                w.patterns[1],
                effective(w, 1),
                4,
                0.,
                1.,
                a,
            )?;
        }
    }
    if w.south && !neighbor_thick(lot, p, 0, 1, 1) {
        let style = lot
            .get(i32::from(p.x), i32::from(p.y) + 1, p.level)
            .map(|t| effective(&t.wall, 1))
            .unwrap_or(0);
        line(
            lot,
            p,
            [1., 1.],
            [0., 1.],
            w.patterns[2],
            style,
            4,
            0.,
            1.,
            a,
        )?;
    }
    if w.east && !neighbor_thick(lot, p, 1, 0, 0) {
        let style = lot
            .get(i32::from(p.x) + 1, i32::from(p.y), p.level)
            .map(|t| effective(&t.wall, 0))
            .unwrap_or(0);
        line(
            lot,
            p,
            [1., 0.],
            [1., 1.],
            w.patterns[3],
            style,
            4,
            0.,
            1.,
            a,
        )?;
    }
    if let Some(d) = t.diagonal {
        let s = effective(w, 1);
        if thick(w.styles[1]) {
            match d {
                Diagonal::Horizontal => {
                    line(
                        lot,
                        p,
                        [th, 1. + th],
                        [1. + th, th],
                        w.patterns[3],
                        s,
                        2,
                        0.,
                        1.,
                        a,
                    )?;
                    line(
                        lot,
                        p,
                        [1. - th, -th],
                        [-th, 1. - th],
                        w.patterns[2],
                        s,
                        5,
                        0.,
                        1.,
                        a,
                    )?;
                    line(
                        lot,
                        p,
                        [-th, 1. - th],
                        [th, 1. + th],
                        w.patterns[3],
                        1,
                        5,
                        0.,
                        th * 2.,
                        a,
                    )?;
                    line(
                        lot,
                        p,
                        [1. + th, th],
                        [1. - th, -th],
                        w.patterns[2],
                        1,
                        5,
                        0.,
                        th * 2.,
                        a,
                    )?;
                }
                Diagonal::Vertical => {
                    line(
                        lot,
                        p,
                        [-th, th],
                        [1. - th, 1. + th],
                        w.patterns[2],
                        s,
                        3,
                        0.,
                        1.,
                        a,
                    )?;
                    line(
                        lot,
                        p,
                        [1. + th, 1. - th],
                        [th, -th],
                        w.patterns[3],
                        s,
                        5,
                        0.,
                        1.,
                        a,
                    )?;
                    line(
                        lot,
                        p,
                        [th, -th],
                        [-th, th],
                        w.patterns[2],
                        1,
                        5,
                        0.,
                        th * 2.,
                        a,
                    )?;
                    line(
                        lot,
                        p,
                        [1. - th, 1. + th],
                        [1. + th, 1. - th],
                        w.patterns[3],
                        1,
                        5,
                        0.,
                        th * 2.,
                        a,
                    )?;
                }
            }
        } else {
            let (from, to, pat1, pat2) = match d {
                Diagonal::Horizontal => ([0., 1.], [1., 0.], w.patterns[3], w.patterns[2]),
                Diagonal::Vertical => ([0., 0.], [1., 1.], w.patterns[2], w.patterns[3]),
            };
            line(lot, p, from, to, pat1, s, 4, 0., 1., a)?;
            line(lot, p, to, from, pat2, s, 4, 0., 1., a)?;
        }
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn line(
    lot: &VisualLot,
    p: TileCoord,
    from: [f32; 2],
    to: [f32; 2],
    pattern: u16,
    style: u16,
    top_mode: u8,
    start: f32,
    end: f32,
    a: &mut Acc<'_>,
) -> Result<(), Error> {
    let position = |xy: [f32; 2]| {
        let x = xy[0] + f32::from(p.x);
        let y = xy[1] + f32::from(p.y);
        Vec3::new(x * 3., lot.height_at(x, y, p.level), y * 3.)
    };
    let p1 = position(from);
    let p2 = position(to);
    let cut = |v: Vec3| {
        if top_mode == 4 {
            return 0.98;
        }
        if let Some(c) = &a.options.cutaway {
            if c.level == p.level {
                let (dx, dy) =
                    [(-1., -1.), (-1., 1.), (1., 1.), (1., -1.)][usize::from(c.rotation)];
                let x = (v.x / 3. + dx * 0.66) as i32;
                let y = (v.z / 3. + dy * 0.66) as i32;
                if x >= 0
                    && y >= 0
                    && x < i32::from(lot.width)
                    && y < i32::from(lot.height)
                    && c.tiles[y as usize * usize::from(lot.width) + x as usize]
                {
                    return 0.12;
                }
            }
        }
        1.
    };
    let h1 = cut(p1);
    let h2 = cut(p2);
    let up = Vec3::Y * 8.85;
    let normal = (p2 - p1).cross(Vec3::Y).normalize_or_zero();
    let l = f32::from(p.level - 1);
    let col = if from[0] == to[0] {
        [0.85, 0.85, 0.85, 1.]
    } else {
        [1.; 4]
    };
    let mut mesh = Mesh {
        vertices: vec![
            Vertex {
                position: p1,
                normal,
                uv: Vec2::new(start, l),
                color: col,
            },
            Vertex {
                position: p2,
                normal,
                uv: Vec2::new(end, l),
                color: col,
            },
            Vertex {
                position: p1 + up * h1,
                normal,
                uv: Vec2::new(start, l + h1),
                color: col,
            },
            Vertex {
                position: p2 + up * h2,
                normal,
                uv: Vec2::new(end, l + h2),
                color: col,
            },
        ],
        indices: vec![0, 2, 1, 2, 3, 1],
    };
    a.add(
        SurfaceKind::Wall,
        u32::from(pattern),
        style,
        Some(p),
        p.level,
        mesh.clone(),
    )?;
    if top_mode < 4 {
        let back = match top_mode {
            0 => Vec3::new(-0.45, 0., 0.),
            1 => Vec3::new(0., 0., -0.45),
            2 => Vec3::new(-0.45, 0., -0.45),
            _ => Vec3::new(0.45, 0., -0.45),
        };
        let a1 = p1 + up * (h1 - 0.001);
        let a2 = p2 + up * (h2 - 0.001);
        for (v, pos) in mesh.vertices.iter_mut().zip([a1, a2, a1 + back, a2 + back]) {
            v.position = pos;
            v.normal = Vec3::Y;
            v.color = [151. / 255., 120. / 255., 76. / 255., 1.];
            v.uv = Vec2::ZERO;
        }
        a.add(SurfaceKind::WallTop, 0, 0, Some(p), p.level, mesh)?;
    }
    Ok(())
}
