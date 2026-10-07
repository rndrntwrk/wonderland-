//! Optional ordered material state for the CPU oracle and disposable GPU packets.
//! No engine handles or persistent game state are represented here.
use super::{DepthComparison, ReferenceError};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StencilComparison {
    Always,
    Equal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StencilOperation {
    Keep,
    Zero,
    Replace,
}
impl StencilOperation {
    pub(super) fn apply(self, current: u8, reference: u8) -> u8 {
        match self {
            Self::Keep => current,
            Self::Zero => 0,
            Self::Replace => reference,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct StencilFace {
    pub compare: StencilComparison,
    pub pass: StencilOperation,
    pub fail: StencilOperation,
    pub depth_fail: StencilOperation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct StencilState {
    pub reference: u8,
    /// Original projected winding, before triangle fill canonicalization.
    pub clockwise: StencilFace,
    pub counterclockwise: StencilFace,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FragmentBlend {
    SourceOver,
    /// SourceAlpha / InverseSourceAlpha for RGB AND alpha (XNA semantics).
    NonPremultiplied,
    /// A depth/stencil-only command preserves both color and selection IDs.
    NoColor,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct FragmentPipeline {
    pub depth_compare: DepthComparison,
    pub depth_write: bool,
    /// Applied in homogeneous clip space before clipping, not only at fragment
    /// write. The source MaskFar shader emits z = w.
    pub forced_depth: Option<f32>,
    pub stencil: Option<StencilState>,
    pub blend: FragmentBlend,
}
impl FragmentPipeline {
    pub fn validate(self) -> Result<(), ReferenceError> {
        if self
            .forced_depth
            .is_some_and(|v| !v.is_finite() || !(0. ..=1.).contains(&v))
        {
            return Err(ReferenceError::Invalid("forced material depth"));
        }
        Ok(())
    }
}
