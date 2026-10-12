//! Big operator and limit placement engine (TeX Appendix G Rule 13).
//!
//! Provides packed operator metrics, branchless limit switching,
//! variant selection, integral overhang correction, and limit centering.

use crate::backend::fonts::FontFamily;
use crate::math::math_ast::MathStyle;
use crate::math::math_box::MathBox;
use crate::math::math_fonts::MathFontParameters;
use crate::math::math_spacing::AtomLimits;

/// 16-byte packed metrics storing TeX Appendix G Rule 13 parameters ($\xi_9..\xi_{13}$).
///
/// Designed to fit four instances per 64-byte L1 cache line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct PackedBigOpMetrics {
    /// Minimum clearance above the nucleus to upper limit ($\xi_9$, big_op_spacing1).
    pub upper_clearance: f32,
    /// Minimum clearance below the nucleus to lower limit ($\xi_{10}$, big_op_spacing2).
    pub lower_clearance: f32,
    /// Minimum baseline rise of the upper limit above nucleus ($\xi_{11}$, big_op_spacing3).
    pub upper_padding: f32,
    /// Minimum baseline drop of the lower limit below nucleus ($\xi_{12}$, big_op_spacing4).
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

    /// Fast test for whether display limits apply given explicit AST flag.
    #[inline(always)]
    pub fn is_display_limits(limits: bool, style: MathStyle) -> bool {
        limits && style.is_display()
    }

    /// Fast operator classification for standard text operators (lim, max, min, det, etc.).
    #[inline(always)]
    pub fn is_text_operator(op: &str) -> bool {
        matches!(
            op,
            "lim" | "max" | "min" | "sup" | "inf" | "gcd" | "det" | "liminf" | "limsup" | "Pr"
        )
    }

    /// Fast operator classification for integral variants.
    #[inline(always)]
    pub fn is_integral_operator(op: &str) -> bool {
        matches!(op, "int" | "iint" | "iiint" | "oint")
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

    /// CMEX glyph code resolution for non-integral big operators.
    ///
    /// Branchless selection between text and display variants.
    #[inline(always)]
    pub fn resolve_cmex_code(op: &str, is_display: bool) -> u8 {
        let disp_idx = is_display as usize;
        let codes: [u8; 2] = match op {
            "prod" => [0x51, 0x59],
            "coprod" => [0x60, 0x61],
            "bigcap" => [0x54, 0x5C],
            "bigcup" => [0x53, 0x5B],
            "biguplus" => [0x55, 0x5D],
            "bigsqcup" => [0x46, 0x47],
            "bigodot" => [0x4A, 0x4B],
            "bigoplus" => [0x4C, 0x4D],
            "bigotimes" => [0x4E, 0x4F],
            "bigwedge" => [0x56, 0x5E],
            "bigvee" => [0x57, 0x5F],
            _ => [0x50, 0x58], // \sum fallback
        };
        codes[disp_idx]
    }

    /// CMEX glyph code resolution for integral operators.
    #[inline(always)]
    pub fn resolve_integral_cmex_code(op: &str, is_display: bool) -> u8 {
        let disp_idx = is_display as usize;
        let codes: [u8; 2] = if matches!(op, "oint" | "oiint" | "oiiint") {
            [0x48, 0x49]
        } else {
            [0x52, 0x5A]
        };
        codes[disp_idx]
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

    /// Calculates horizontal kern/offset between consecutive integral signs.
    #[inline(always)]
    pub fn integral_offset(single_w: f32, font_size: f32, is_display: bool) -> f32 {
        const INTEGRAL_KERN_FACTORS: [f32; 2] = [3.0 / 18.0, 6.0 / 18.0];
        single_w - INTEGRAL_KERN_FACTORS[is_display as usize] * font_size
    }

    /// Computes total width of single, double, or triple integral signs.
    #[inline(always)]
    pub fn total_integral_width(
        op: &str,
        single_w: f32,
        font_size: f32,
        is_display: bool,
    ) -> (f32, f32) {
        let offset = Self::integral_offset(single_w, font_size, is_display);
        let repeats = match op {
            "iint" | "oiint" => 1.0,
            "iiint" | "oiiint" => 2.0,
            _ => 0.0,
        };
        (single_w + repeats * offset, offset)
    }
}

/// Helper for TeX Appendix G Rule 13 vertical shift to center operator nucleus at the math axis.
pub struct Rule13OperatorAxis;

impl Rule13OperatorAxis {
    /// Computes vertical baseline offset `delta_y` centering the operator nucleus at `axis_h`.
    #[inline(always)]
    pub fn compute_delta_y(is_text_op: bool, axis_h: f32, op_h: f32, op_d: f32) -> f32 {
        if is_text_op {
            0.0
        } else {
            axis_h - (op_h - op_d) * 0.5
        }
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

    /// Computes absolute vertical positions `(sup_y, sub_y)` for limits given nucleus bounds.
    #[inline(always)]
    pub fn compute_limit_positions(
        op_top_y: f32,
        op_bot_y: f32,
        sup_d: f32,
        sub_h: f32,
        metrics: &PackedBigOpMetrics,
    ) -> (f32, f32) {
        let shift_up = metrics.upper_clearance.max(metrics.upper_padding - sup_d);
        let sup_y = op_top_y + shift_up + sup_d;

        let shift_down = metrics.lower_clearance.max(metrics.lower_padding - sub_h);
        let sub_y = op_bot_y - shift_down - sub_h;

        (sup_y, sub_y)
    }

    /// Computes total composite bounding box for an operator with limits.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn compute_limits_box(
        max_w: f32,
        origin_y: f32,
        op_top_y: f32,
        op_bot_y: f32,
        has_sup: bool,
        sup_y: f32,
        sup_h: f32,
        has_sub: bool,
        sub_y: f32,
        sub_d: f32,
        op_extra_pad: f32,
        font_size: f32,
    ) -> MathBox {
        let total_h = if has_sup {
            (sup_y + sup_h) - origin_y + op_extra_pad
        } else {
            op_top_y - origin_y
        };
        let total_d = if has_sub {
            origin_y - (sub_y - sub_d) + op_extra_pad
        } else {
            origin_y - op_bot_y
        };
        let op_padding = (0.35 * font_size).max(3.5);
        MathBox::with_dims(max_w + op_padding, total_h, total_d)
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

    /// Centers operator and limits taking italic correction into account.
    ///
    /// Returns `(max_w, op_x_rel, sup_x_rel, sub_x_rel)`.
    #[inline(always)]
    pub fn center_limits_styled(
        op_w: f32,
        sup_w: f32,
        sub_w: f32,
        has_sup: bool,
        has_sub: bool,
        half_ic: f32,
    ) -> (f32, f32, f32, f32) {
        let eff_sup_w = if has_sup { sup_w + half_ic } else { 0.0 };
        let eff_sub_w = if has_sub { sub_w + half_ic } else { 0.0 };
        let max_w = op_w.max(eff_sup_w).max(eff_sub_w);

        let op_x = 0.5 * (max_w - op_w);
        let sup_x = 0.5 * (max_w - sup_w) + half_ic;
        let sub_x = 0.5 * (max_w - sub_w) - half_ic;

        (max_w, op_x, sup_x, sub_x)
    }

    /// Centers operator and limits positioned relative to `origin_x`.
    ///
    /// Returns `(max_w, op_x, sup_x, sub_x)`.
    #[inline(always)]
    pub fn center_limits_positioned(
        origin_x: f32,
        op_w: f32,
        sup_w: f32,
        sub_w: f32,
        has_sup: bool,
        has_sub: bool,
        half_ic: f32,
    ) -> (f32, f32, f32, f32) {
        let eff_sup_w = if has_sup { sup_w + half_ic } else { 0.0 };
        let eff_sub_w = if has_sub { sub_w + half_ic } else { 0.0 };
        let max_w = op_w.max(eff_sup_w).max(eff_sub_w);

        let op_x = origin_x + 0.5 * (max_w - op_w);
        let sup_x = origin_x + 0.5 * (max_w - sup_w) + half_ic;
        let sub_x = origin_x + 0.5 * (max_w - sub_w) - half_ic;

        (max_w, op_x, sup_x, sub_x)
    }
}

/// High-performance resolver for BigOp metrics and side-script shifts.
pub struct BigOpShiftResolver;

impl BigOpShiftResolver {
    /// Resolves packed 16-byte Rule 13 limits metrics according to font family and size.
    #[inline(always)]
    pub fn resolve_limit_metrics(
        font_family: FontFamily,
        font_size: f32,
        scale: f32,
        params: &MathFontParameters,
    ) -> PackedBigOpMetrics {
        if font_family == FontFamily::ComputerModern {
            PackedBigOpMetrics::new(
                (params.big_op_spacing1 as f32) * scale,
                (params.big_op_spacing2 as f32) * scale,
                (params.big_op_spacing3 as f32) * scale,
                (params.big_op_spacing4 as f32) * scale,
            )
        } else {
            let c = crate::math::fonts::math_constants_for_family(font_family);
            PackedBigOpMetrics::new(
                ((c.upper_limit_gap_min as f32) * 0.001 * font_size).max(2.0),
                ((c.lower_limit_gap_min as f32) * 0.001 * font_size).max(2.0),
                ((c.upper_limit_baseline_rise_min as f32) * 0.001 * font_size).max(3.0),
                ((c.lower_limit_baseline_drop_min as f32) * 0.001 * font_size).max(3.0),
            )
        }
    }

    /// Computes horizontal and initial vertical shifts for BigOp side-scripts.
    ///
    /// Returns `(sup_x_rel, sub_x_rel, sup_shift, sub_shift)`.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn resolve_side_scripts(
        op: &str,
        is_integral: bool,
        is_display: bool,
        op_w: f32,
        op_h: f32,
        op_d: f32,
        op_ic: f32,
        delta_y: f32,
        font_size: f32,
        scale: f32,
        is_cm: bool,
        has_sup: bool,
        params: &MathFontParameters,
    ) -> (f32, f32, f32, f32) {
        if is_integral {
            const INT_SUP_FACTORS: [f32; 2] = [0.55, 0.95];
            const INT_SUB_FACTORS: [f32; 2] = [0.35, 0.75];
            let disp_idx = is_display as usize;
            let s_shift = (INT_SUP_FACTORS[disp_idx] * font_size).max((params.sup1 as f32) * scale);
            let b_shift = (INT_SUB_FACTORS[disp_idx] * font_size).max((params.sub1 as f32) * scale);

            let sx = op_w + (0.05 * font_size);
            let bx = if matches!(op, "oint" | "oiint" | "oiiint") {
                op_w + (0.02 * font_size)
            } else if matches!(op, "iint" | "iiint") {
                op_w - (0.12 * font_size)
            } else {
                op_w - (0.22 * font_size)
            };
            (sx, bx, s_shift, b_shift)
        } else {
            let sx = op_w + op_ic + 1.0;
            let bx = op_w + 1.0;
            let op_top_rel = delta_y + op_h;
            let op_bot_rel = delta_y - op_d;
            let s_shift = ((params.sup1 as f32) * scale)
                .max(op_top_rel - (params.sup_drop as f32) * scale);
            let mut b_shift = ((params.sub1 as f32) * scale)
                .max(-op_bot_rel + (params.sub_drop as f32) * scale);
            if has_sup && !is_cm {
                b_shift = b_shift.max(-op_bot_rel + 1.0 * scale);
            }
            (sx, bx, s_shift, b_shift)
        }
    }

    /// Enforces TeX Rule 18 vertical clearance between side-scripts.
    #[inline(always)]
    pub fn enforce_side_script_clearance(
        mut sup_shift: f32,
        mut sub_shift: f32,
        sup_d: f32,
        sub_h: f32,
        sub_sup_gap: f32,
        accent_base_height: f32,
    ) -> (f32, f32) {
        let gap = (sup_shift - sup_d) - (-sub_shift + sub_h);
        if gap < sub_sup_gap {
            let shortfall = sub_sup_gap - gap;
            sub_shift += shortfall;
            let min_sup_h = 0.8 * accent_base_height + sup_d;
            if sup_shift < min_sup_h {
                let push = min_sup_h - sup_shift;
                sup_shift += push;
                sub_shift -= push;
            }
        }
        (sup_shift, sub_shift)
    }

    /// Computes composite bounding box for an operator with side scripts.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn compute_side_scripts_box(
        op_w: f32,
        delta_y: f32,
        op_h: f32,
        op_d: f32,
        has_sup: bool,
        sup_x_rel: f32,
        sup_w: f32,
        sup_shift: f32,
        sup_h: f32,
        has_sub: bool,
        sub_x_rel: f32,
        sub_w: f32,
        sub_shift: f32,
        sub_d: f32,
    ) -> MathBox {
        let op_top_rel = delta_y + op_h;
        let op_bot_rel = delta_y - op_d;
        let right_bound = op_w
            .max(if has_sup { sup_x_rel + sup_w } else { 0.0 })
            .max(if has_sub { sub_x_rel + sub_w } else { 0.0 });
        let total_h = op_top_rel.max(if has_sup { sup_shift + sup_h } else { 0.0 });
        let total_d = (-op_bot_rel).max(if has_sub { sub_shift + sub_d } else { 0.0 });
        MathBox::with_dims(right_bound, total_h, total_d)
    }
}
