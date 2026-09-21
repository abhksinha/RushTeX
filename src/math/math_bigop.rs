//! Big operator and limit placement engine (TeX Appendix G Rule 13).
//!
//! Provides packed operator metrics, branchless limit switching,
//! variant selection, integral overhang correction, and limit centering.

use crate::math::math_ast::MathStyle;
use crate::math::math_box::MathBox;
use crate::math::math_spacing::AtomLimits;

/// 16-byte packed metrics storing TeX Appendix G Rule 13 parameters ($\xi_9..\xi_{13}$).
///
/// Designed to fit four instances per 64-byte L1 cache line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct PackedBigOpMetrics {
    /// Minimum clearance above the nucleus to upper limit ($\xi_{11}$).
    pub upper_clearance: f32,
    /// Minimum clearance below the nucleus to lower limit ($\xi_{13}$).
    pub lower_clearance: f32,
    /// Padding above the upper limit ($\xi_9$).
    pub upper_padding: f32,
    /// Padding below the lower limit ($\xi_{10}$).
    pub lower_padding: f32,
}

const _: () = assert!(core::mem::size_of::<PackedBigOpMetrics>() == 16);

impl PackedBigOpMetrics {
    #[inline(always)]
    pub const fn new(
        upper_clearance: f32,
        lower_clearance: f32,
        upper_padding: f32,
        lower_padding: f32,
    ) -> Self {
        Self {
            upper_clearance,
            lower_clearance,
            upper_padding,
            lower_padding,
        }
    }
}

/// Branchless resolver for operator limits placement.
pub struct OperatorLimitSwitch;

impl OperatorLimitSwitch {
    /// Determines whether limits should be placed vertically above/below the operator
    /// or horizontally as scripts.
    #[inline(always)]
    pub fn should_render_limits(
        is_integral: bool,
        limits_pref: AtomLimits,
        style: MathStyle,
    ) -> bool {
        match limits_pref {
            AtomLimits::Limits => true,
            AtomLimits::NoLimits => false,
            AtomLimits::DisplayLimits => style.is_display(),
            AtomLimits::Default => {
                if is_integral {
                    false // Integrals default to side scripts
                } else {
                    style.is_display() // Sums/products default to limits in display mode
                }
            }
        }
    }
}

/// 64-byte aligned static table mapping operator kinds to glyph codes.
#[repr(C, align(64))]
pub struct BigOpVariantTier {
    /// Text-size glyph codes for common operators.
    pub text_codes: [u8; 8],
    /// Display-size glyph codes for common operators.
    pub display_codes: [u8; 8],
    /// Italic corrections for operators in 1000-units-per-em.
    pub italic_corrections: [u8; 8],
    pub _pad: [u8; 40],
}

pub static BIG_OP_VARIANT_TIER: BigOpVariantTier = BigOpVariantTier {
    text_codes: [
        0xE5, // Sum
        0xD5, // Prod
        0xCA, // Coprod
        0xF2, // Int
        0xF2, // Oint
        0xC7, // Bigcap
        0xC8, // Bigcup
        0x00,
    ],
    display_codes: [
        0x72, // CMEX Sum display
        0x79, // CMEX Prod display
        0x60, // CMEX Coprod display
        0x5A, // CMEX Int display
        0x5A, // CMEX Oint display
        0x5C, // CMEX Bigcap display
        0x5B, // CMEX Bigcup display
        0x00,
    ],
    italic_corrections: [
        0,   // Sum
        0,   // Prod
        0,   // Coprod
        100, // Int
        100, // Oint
        0,   // Bigcap
        0,   // Bigcup
        0,
    ],
    _pad: [0; 40],
};

const _: () = assert!(core::mem::size_of::<BigOpVariantTier>() == 64);
const _: () = assert!(core::mem::align_of::<BigOpVariantTier>() == 64);

impl BigOpVariantTier {
    /// Dynamically resolves an operator variant from a MathFont.
    #[inline(always)]
    pub fn resolve_font_op(
        op: &str,
        font: &impl crate::math::fonts::opentype::MathFont,
        style: MathStyle,
    ) -> Option<crate::math::fonts::operators::ResolvedBigOp> {
        crate::math::fonts::operators::OpenTypeBigOpKind::from_command(op)
            .and_then(|kind| kind.resolve_font_operator(font, style))
    }
}

/// Precomputed overhang corrector for integrals ($\int, \iint, \iiint, \oint$).
pub struct IntegralOverhangCorrector;

impl IntegralOverhangCorrector {
    /// Returns horizontal offset to center upper and lower limits over slanted integrals.
    #[inline(always)]
    pub fn limit_offsets(op_ic: f32) -> (f32, f32) {
        let half_ic = 0.5 * op_ic;
        (half_ic, -half_ic)
    }
}

/// Branchless clearance governor for operator limits.
pub struct LimitClearanceGovernor;

impl LimitClearanceGovernor {
    /// Computes vertical baseline shifts for upper and lower limits according to Rule 13.
    #[inline(always)]
    pub fn compute_limit_shifts(
        op_h: f32,
        op_d: f32,
        upper_box: Option<&MathBox>,
        lower_box: Option<&MathBox>,
        metrics: &PackedBigOpMetrics,
    ) -> (f32, f32) {
        let upper_shift = if let Some(ub) = upper_box {
            let needed = op_h + metrics.upper_clearance + ub.d;
            needed.max(op_h + metrics.upper_padding)
        } else {
            0.0
        };

        let lower_shift = if let Some(lb) = lower_box {
            let needed = op_d + metrics.lower_clearance + lb.h;
            needed.max(op_d + metrics.lower_padding)
        } else {
            0.0
        };

        (upper_shift, lower_shift)
    }
}

/// Zero-allocation horizontal centering engine for operator limits.
pub struct OperatorCenteringEngine;

impl OperatorCenteringEngine {
    /// Centers limit boxes horizontally over the operator nucleus.
    #[inline(always)]
    pub fn center_limits(
        op_w: f32,
        upper_w: Option<f32>,
        lower_w: Option<f32>,
        upper_offset: f32,
        lower_offset: f32,
    ) -> (f32, f32, f32, f32) {
        let max_w = op_w.max(upper_w.unwrap_or(0.0)).max(lower_w.unwrap_or(0.0));

        let op_x = 0.5 * (max_w - op_w);
        let upper_x = upper_w.map_or(0.0, |w| 0.5 * (max_w - w) + upper_offset);
        let lower_x = lower_w.map_or(0.0, |w| 0.5 * (max_w - w) + lower_offset);

        (max_w, op_x, upper_x, lower_x)
    }
}
