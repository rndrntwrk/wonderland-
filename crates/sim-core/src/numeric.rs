//! Explicit C# arithmetic for the pinned FreeSO interpreter.
//!
//! VMExpression uses promoted i32 intermediates even when its destination is
//! a short. Callers must preserve read/write order and narrow only at the write.
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumericError {
    DivideByZero,
    DivisionOverflow,
}

impl fmt::Display for NumericError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DivideByZero => "integer remainder divisor is zero",
            Self::DivisionOverflow => "signed integer division or remainder overflow",
        })
    }
}

impl std::error::Error for NumericError {}

pub const fn narrow_i16(value: i32) -> i16 {
    value as i16
}

pub const fn narrow_u16(value: i32) -> u16 {
    value as u16
}

pub const fn wrapping_add_i32(lhs: i32, rhs: i32) -> i32 {
    lhs.wrapping_add(rhs)
}

pub const fn wrapping_sub_i32(lhs: i32, rhs: i32) -> i32 {
    lhs.wrapping_sub(rhs)
}

pub const fn wrapping_mul_i32(lhs: i32, rhs: i32) -> i32 {
    lhs.wrapping_mul(rhs)
}

/// VMExpression's division: zero writes -1, and MIN/-1 is a legacy fault.
pub fn div_i32(lhs: i32, rhs: i32) -> Result<i32, NumericError> {
    if rhs == 0 {
        return Ok(-1);
    }
    lhs.checked_div(rhs).ok_or(NumericError::DivisionOverflow)
}

/// Raw C# integer remainder, before VMExpression's additional normalization.
pub fn rem_i32(lhs: i32, rhs: i32) -> Result<i32, NumericError> {
    if rhs == 0 {
        return Err(NumericError::DivideByZero);
    }
    lhs.checked_rem(rhs).ok_or(NumericError::DivisionOverflow)
}

/// Exact source formula, including signed i32 overflow during the addition.
/// A zero divisor leaves the original value unchanged.
pub fn mod_i32(lhs: i32, rhs: i32) -> Result<i32, NumericError> {
    if rhs == 0 {
        return Ok(lhs);
    }
    rem_i32(rem_i32(lhs, rhs)?.wrapping_add(rhs), rhs)
}

pub const fn shl_i32(value: i32, count: i32) -> i32 {
    value.wrapping_shl((count as u32) & 31)
}

pub const fn shr_i32(value: i32, count: i32) -> i32 {
    value.wrapping_shr((count as u32) & 31)
}

pub const fn shl_u64(value: u64, count: i32) -> u64 {
    value.wrapping_shl((count as u32) & 63)
}

pub const fn shr_u64(value: u64, count: i32) -> u64 {
    value.wrapping_shr((count as u32) & 63)
}

pub const fn flag_mask_1based(bit: i32) -> i32 {
    shl_i32(1, bit.wrapping_sub(1))
}

pub const fn set_flag_i32(lhs: i32, bit: i32) -> i32 {
    lhs | flag_mask_1based(bit)
}

pub const fn clear_flag_i32(lhs: i32, bit: i32) -> i32 {
    lhs & !flag_mask_1based(bit)
}

/// The source uses `> 0`, so a set signed-i32 sign bit still tests false.
pub const fn is_flag_set_i32(lhs: i32, bit: i32) -> bool {
    (lhs & flag_mask_1based(bit)) > 0
}

/// Math.Round(value)'s default ties-to-even rule, compatible with Rust 1.75.
/// Signed zero and non-finite inputs are preserved.
pub fn round_ties_even(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let integer = value.trunc();
    let fraction = (value - integer).abs();
    if fraction < 0.5 || (fraction == 0.5 && integer % 2.0 == 0.0) {
        integer
    } else {
        integer + value.signum()
    }
}

/// Explicit Mono 6.8 amd64 policy for unchecked C# floating-to-i32 casts.
/// C# leaves non-finite and out-of-range results unspecified; the extracted
/// runtime probe returns MIN. Rust's saturating `as` alone would differ.
pub fn legacy_f64_to_i32(value: f64) -> i32 {
    let integer = value.trunc();
    if (-2147483648.0..2147483648.0).contains(&integer) {
        integer as i32
    } else {
        i32::MIN
    }
}

/// Mono converts through i32 before narrowing the low sixteen bits.
pub fn legacy_f64_to_i16(value: f64) -> i16 {
    narrow_i16(legacy_f64_to_i32(value))
}

/// VMExpression operator 20: `(short)Math.Sqrt(rhs)`.
pub fn sqrt_i16(rhs: i32) -> i16 {
    legacy_f64_to_i16(f64::from(rhs).sqrt())
}
