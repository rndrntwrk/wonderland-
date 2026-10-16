//! Numeric compatibility for original Vitaboy animation, separate from the
//! generic unit-quaternion render-core API. Source contract: FSOMonoGame
//! 74f7ef558111e62a7d0a2228b6f9714980e000fa, Quaternion.cs:731-763 and
//! Matrix.cs:679-706. The original samples and resulting poses are not normalized.
use crate::{AvatarError, Result};
use wonderland_render_core::math::{Mat4, Quat};

pub(crate) fn norm_squared(q: Quat) -> f32 {
    ((q.x * q.x + q.y * q.y) + q.z * q.z) + q.w * q.w
}
pub(crate) fn finite_rotation(q: Quat) -> bool {
    q.is_finite() && norm_squared(q).is_finite()
}

pub(crate) fn slerp(left: Quat, right: Quat, amount: f32) -> Result<Quat> {
    if !finite_rotation(left) || !finite_rotation(right) || !amount.is_finite() {
        return Err(AvatarError::Invalid("source interpolation input"));
    }
    let inner = ((left.x * right.x + left.y * right.y) + left.z * right.z) + left.w * right.w;
    let negative = inner < 0.0;
    let distance = if negative { -inner } else { inner };
    let (left_weight, right_weight) = if distance > 0.999999 {
        (1.0 - amount, if negative { -amount } else { amount })
    } else {
        // Source uses System.Math (double), narrowing at these exact boundaries.
        let angle = (distance as f64).acos() as f32;
        let reciprocal_sine = (1.0 / (angle as f64).sin()) as f32;
        let a = (((1.0 - amount) * angle) as f64).sin() as f32;
        let b = ((amount * angle) as f64).sin();
        (
            a * reciprocal_sine,
            (if negative { -b } else { b }) as f32 * reciprocal_sine,
        )
    };
    let value = Quat::new(
        left_weight * left.x + right_weight * right.x,
        left_weight * left.y + right_weight * right.y,
        left_weight * left.z + right_weight * right.z,
        left_weight * left.w + right_weight * right.w,
    );
    if !finite_rotation(value) {
        return Err(AvatarError::Invalid("source interpolation overflow"));
    }
    Ok(value)
}

pub(crate) fn rotation_matrix(q: Quat) -> Mat4 {
    let xx = q.x * q.x;
    let yy = q.y * q.y;
    let zz = q.z * q.z;
    let xy = q.x * q.y;
    let zw = q.z * q.w;
    let zx = q.z * q.x;
    let yw = q.y * q.w;
    let yz = q.y * q.z;
    let xw = q.x * q.w;
    // Source row-vector matrix, transposed into render-core column storage.
    Mat4 {
        cols: [
            [1.0 - 2.0 * (yy + zz), 2.0 * (xy + zw), 2.0 * (zx - yw), 0.0],
            [2.0 * (xy - zw), 1.0 - 2.0 * (zz + xx), 2.0 * (yz + xw), 0.0],
            [2.0 * (zx + yw), 2.0 * (yz - xw), 1.0 - 2.0 * (yy + xx), 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_branch_threshold_differs_from_generic_slerp() {
        let left = Quat::IDENTITY;
        let right = Quat::new(0.0, 0.02, 0.0, 0.9999);
        let actual = slerp(left, right, 0.5).unwrap();
        // Analytic midpoint: (left+right)/sqrt(2+2*dot), without normalization.
        let scale = 1.0f64 / (2.0f64 + 2.0 * right.w as f64).sqrt();
        assert!((actual.y as f64 - right.y as f64 * scale).abs() < 2e-7);
        assert!((actual.w as f64 - (1.0 + right.w as f64) * scale).abs() < 2e-7);
        assert!((actual.w - left.slerp(right, 0.5).w).abs() > 1e-5);
    }
}
