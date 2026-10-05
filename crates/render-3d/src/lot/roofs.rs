use super::*;

pub(super) fn roofable(lot: &VisualLot, x: i32, y: i32, level: u8) -> bool {
    if level < 2 || level > lot.levels + 1 || x < 0 || y < 0 {
        return false;
    }
    let (tx, ty) = (x / 16, y / 16);
    if tx <= 0 || ty <= 0 || tx >= i32::from(lot.width) - 1 || ty >= i32::from(lot.height) - 1 {
        return false;
    }
    let indoor = |xx, yy, l| lot.get(xx, yy, l).map(|t| t.indoors).unwrap_or(false);
    let blocked = |xx, yy| {
        lot.get(xx, yy, level)
            .map(|t| t.indoors || t.floor != 0 || t.diagonal.is_some())
            .unwrap_or(false)
    };
    let (dx, dy) = (
        if x % 16 == 8 { 1 } else { -1 },
        if y % 16 == 8 { 1 } else { -1 },
    );
    let half = !indoor(tx, ty, level - 1);
    let mut only_diagonal = false;
    if half {
        let cardinal = indoor(tx + dx, ty, level - 1) || indoor(tx, ty + dy, level - 1);
        let diagonal = indoor(tx + dx, ty + dy, level - 1);
        if !cardinal && !diagonal {
            return false;
        }
        only_diagonal = !cardinal && diagonal;
    }
    if blocked(tx, ty) {
        return false;
    }
    !(half
        && (blocked(tx + dx, ty)
            || blocked(tx, ty + dy)
            || (only_diagonal && blocked(tx + dx, ty + dy))))
}
pub(super) fn rectangles(
    lot: &VisualLot,
    level: u8,
    budget: usize,
) -> Result<Vec<RoofRect>, Error> {
    if level < 2 || level > lot.levels + 1 {
        return Err(Error::InvalidInput("roof level"));
    }
    let w = usize::from(lot.width) * 2;
    let h = usize::from(lot.height) * 2;
    if w * h > budget {
        return Err(Error::BudgetExceeded("roof grid"));
    }
    let mut evaluated = vec![false; w * h];
    let mut result: Vec<RoofRect> = Vec::new();
    let mut checks = 0usize;
    let mut check = |x, y| -> Result<bool, Error> {
        checks += 1;
        if checks > budget {
            Err(Error::BudgetExceeded("roof spread"))
        } else {
            Ok(roofable(lot, x, y, level))
        }
    };
    for y in 2..h {
        for x in 2..w {
            if evaluated[y * w + x] {
                continue;
            }
            evaluated[y * w + x] = true;
            if !check(x as i32 * 8, y as i32 * 8)? {
                continue;
            }
            let mut r = RoofRect {
                x1: x as i32 * 8,
                y1: y as i32 * 8,
                x2: (x as i32 + 1) * 8,
                y2: (y as i32 + 1) * 8,
            };
            for dir in [0, 2, 1, 3] {
                loop {
                    let (sx, sy, ix, iy, n) = match dir {
                        0 => (r.x2, r.y1, 0, 8, (r.y2 - r.y1) / 8),
                        1 => (r.x2 - 8, r.y2, -8, 0, (r.x2 - r.x1) / 8),
                        2 => (r.x1 - 8, r.y2 - 8, 0, -8, (r.y2 - r.y1) / 8),
                        _ => (r.x1, r.y1 - 8, 8, 0, (r.x2 - r.x1) / 8),
                    };
                    let mut valid = true;
                    for i in 0..n {
                        if !check(sx + ix * i, sy + iy * i)? {
                            valid = false;
                            break;
                        }
                    }
                    if !valid {
                        break;
                    }
                    for i in 0..n {
                        let ex = (sx + ix * i) / 8;
                        let ey = (sy + iy * i) / 8;
                        if ex >= 0 && ey >= 0 && (ex as usize) < w && (ey as usize) < h {
                            evaluated[ey as usize * w + ex as usize] = true;
                        }
                    }
                    let mx = sx + ix * n / 2 + 4;
                    let my = sy + iy * n / 2 + 4;
                    let into = result.iter().find(|other| {
                        mx >= other.x1
                            && mx <= other.x2
                            && my >= other.y1
                            && my <= other.y2
                            && if dir % 2 == 0 {
                                r.y1 > other.y1 && r.y2 < other.y2
                            } else {
                                r.x1 > other.x1 && r.x2 < other.x2
                            }
                    });
                    match (dir, into) {
                        (0, Some(o)) => r.x2 = o.x2,
                        (1, Some(o)) => r.y2 = o.y2,
                        (2, Some(o)) => r.x1 = o.x1,
                        (3, Some(o)) => r.y1 = o.y1,
                        (0, None) => r.x2 += 8,
                        (1, None) => r.y2 += 8,
                        (2, None) => r.x1 -= 8,
                        (_, None) => r.y1 -= 8,
                        _ => unreachable!(),
                    }
                }
            }
            result.push(r);
        }
    }
    Ok(result)
}
pub(super) fn mesh(
    lot: &VisualLot,
    r: RoofRect,
    level: u8,
    style: RoofStyle,
    a: &mut Acc<'_>,
) -> Result<(), Error> {
    let rise = (r.x2 - r.x1).min(r.y2 - r.y1) / 2;
    let pos = |x: i32, y: i32, z: i32| {
        let tx = (x / 16).rem_euclid(i32::from(lot.width)) as usize;
        let ty = (y / 16).rem_euclid(i32::from(lot.height)) as usize;
        let alt = if x <= 0 || y <= 0 {
            0.
        } else {
            (f32::from(lot.altitude_centers[ty * usize::from(lot.width) + tx])
                - f32::from(lot.base_alt))
                * TERRAIN_FACTOR
                * 3.
        };
        Vec3::new(
            x as f32 * 3. / 16.,
            z as f32 * style.pitch * 3. / 16. + f32::from(level - 1) * 8.85 + alt,
            y as f32 * 3. / 16.,
        )
    };
    let outer = [
        pos(r.x1, r.y1, 0),
        pos(r.x2, r.y1, 0),
        pos(r.x2, r.y2, 0),
        pos(r.x1, r.y2, 0),
    ];
    let inner = [
        pos(r.x1 + rise, r.y1 + rise, rise),
        pos(r.x2 - rise, r.y1 + rise, rise),
        pos(r.x2 - rise, r.y2 - rise, rise),
        pos(r.x1 + rise, r.y2 - rise, rise),
    ];
    let mut roof = empty_mesh();
    let mut rim = empty_mesh();
    let mut edges = empty_mesh();
    for i in 0..4 {
        let j = (i + 1) % 4;
        let (l, r, ml, mr) = (outer[i], outer[j], inner[i], inner[j]);
        let mut normal = -(r - l).cross(mr - l).normalize_or_zero();
        if normal == Vec3::ZERO {
            normal = Vec3::Y;
        }
        let uv = |p: Vec3| match i {
            0 => Vec2::new(p.x * 2. / 3., -p.z),
            1 => Vec2::new(p.z * 2. / 3., p.x),
            2 => Vec2::new(p.x * 2. / 3., p.z),
            _ => Vec2::new(p.z * 2. / 3., -p.x),
        };
        let mut face = quad([l, r, mr, ml], normal, [uv(l), uv(r), uv(mr), uv(ml)], true);
        face.vertices[2].color = [1.25; 4];
        face.vertices[3].color = [1.25; 4];
        append(&mut roof, face);
        if style.advanced {
            let down = Vec3::Y * -0.5;
            let n = (r - l).cross(Vec3::Y).normalize_or_zero();
            let rim_uv = |p: Vec3, y| Vec2::new((p.x + p.z) * 0.66, y);
            append(
                &mut rim,
                quad(
                    [r + down, r, l, l + down],
                    n,
                    [
                        rim_uv(r, 0.73),
                        rim_uv(r, 0.4),
                        rim_uv(l, 0.4),
                        rim_uv(l, 0.73),
                    ],
                    true,
                ),
            );
            let lr = (r - l).normalize_or_zero();
            let dl = (l - ml).length();
            let li = l + lr * 0.7;
            let ri = r - lr * 0.7;
            let mli = ml + ((l - ml).normalize_or_zero() + lr) * 0.7;
            let mri = mr + ((r - mr).normalize_or_zero() - lr) * 0.7;
            let il = (mli - li).length();
            let id = (dl - il) * 0.1;
            let top = (mr - ml).length();
            let itop = (mri - mli).length();
            let ito = (top - itop) / 2.;
            let off = Vec3::Y * 0.005;
            let sets = [
                (
                    [mli, ml, l, li],
                    [
                        Vec2::new(-id * 0.5, 0.33),
                        Vec2::ZERO,
                        Vec2::new(-dl * 0.5, 0.),
                        Vec2::new(-(id + il) * 0.5, 0.33),
                    ],
                ),
                (
                    [mri, mr, ml, mli],
                    [
                        Vec2::new((ito + itop) * 0.5, 0.33),
                        Vec2::new(top * 0.5, 0.),
                        Vec2::ZERO,
                        Vec2::new(ito * 0.5, 0.33),
                    ],
                ),
                (
                    [mri, ri, r, mr],
                    [
                        Vec2::new((top + id) * 0.5, 0.33),
                        Vec2::new((top + id + il) * 0.5, 0.33),
                        Vec2::new((top + dl) * 0.5, 0.),
                        Vec2::new(top * 0.5, 0.),
                    ],
                ),
            ];
            for (points, uv) in sets {
                append(&mut edges, quad(points.map(|p| p + off), normal, uv, true));
            }
        }
    }
    a.add(SurfaceKind::Roof, style.material, 0, None, level, roof)?;
    if style.advanced {
        a.add(SurfaceKind::RoofRim, style.material, 0, None, level, rim)?;
        let down = Vec3::Y * -0.5;
        a.add(
            SurfaceKind::RoofUnderside,
            style.material,
            0,
            None,
            level,
            quad(
                [
                    outer[3] + down,
                    outer[2] + down,
                    outer[1] + down,
                    outer[0] + down,
                ],
                -Vec3::Y,
                [Vec2::new(0., 0.73); 4],
                true,
            ),
        )?;
        a.add(SurfaceKind::RoofEdge, style.material, 0, None, level, edges)?;
    }
    Ok(())
}
