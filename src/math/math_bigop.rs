//! Big operator and limit placement engine (TeX Appendix G Rule 13).
//!
//! Provides packed operator metrics, branchless limit switching,
//! variant selection, integral overhang correction, and limit centering.

use crate::backend::fonts::FontFamily;
use crate::math::math_ast::MathStyle;
use crate::math::math_box::MathBox;
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

    /// Whether an operator with `\limits` mode `mode` (`OP_LIMITS_*`) takes limits in `style`.
    #[inline(always)]
    pub fn takes_limits(mode: u8, style: MathStyle) -> bool {
        match mode & crate::math::math_ast::OP_LIMITS_MODE_MASK {
            crate::math::math_ast::OP_LIMITS_LIMITS => true,
            crate::math::math_ast::OP_LIMITS_NOLIMITS => false,
            _ => style.is_display(),
        }
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
        matches!(op, "int" | "iint" | "iiint" | "iiiint" | "idotsint" | "oint")
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

    /// amsmath `\intdots@` between the two signs of `\idotsint`: three `\cdotp` a thin
    /// space apart in display style (`\@cdots`), 1.5mu apart otherwise.
    #[inline(always)]
    pub fn intdots_width(font_size: f32, is_display: bool) -> f32 {
        const CDOT_W: f32 = 0.277_779;
        let gap = if is_display { 3.0 } else { 1.5 };
        3.0 * CDOT_W * font_size + 2.0 * gap * font_size / 18.0
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
        if op == "idotsint" {
            return (2.0 * single_w + Self::intdots_width(font_size, is_display) + font_size / 3.0, offset);
        }
        let repeats = match op {
            "iint" | "oiint" => 1.0,
            "iiint" | "oiiint" => 2.0,
            "iiiint" => 3.0,
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
            // §749: shift_amount := half(height − depth) − axis_height, moved down.
            use crate::backend::scaled::{half, sp_pt, to_sp};
            sp_pt(to_sp(axis_h) - half(to_sp(op_h) - to_sp(op_d)))
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
        // TeX §750: the limits box is exactly as wide as its widest component.
        MathBox::with_dims(max_w, total_h, total_d)
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
        params: &crate::math::math_fonts::MathParams,
    ) -> PackedBigOpMetrics {
        if font_family == FontFamily::ComputerModern {
            PackedBigOpMetrics::new(
                params.big_op_spacing1,
                params.big_op_spacing2,
                params.big_op_spacing3,
                params.big_op_spacing4,
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
    /// Places side scripts on an operator without limits (TeX §749, §756; mlist.c `make_op`).
    ///
    /// With a subscript the nucleus loses its italic correction `op_ic`, so the subscript is
    /// tucked under the slant and the superscript sits `op_ic` to its right. Vertical shifts are
    /// Rule 18 for a non-character nucleus (sup_drop/sub_drop apply).
    ///
    /// Returns `(sup_x_rel, sub_x_rel, sup_shift, sub_shift)`.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn resolve_side_scripts(
        op_w: f32,
        op_ic: f32,
        nuc_h: f32,
        nuc_d: f32,
        has_sup: bool,
        has_sub: bool,
        style: MathStyle,
        sup_d: f32,
        sub_h: f32,
        params: &crate::math::math_fonts::MathParams,
        drops: (f32, f32),
    ) -> (f32, f32, f32, f32) {
        let sub_x = if has_sub { op_w - op_ic } else { op_w };
        let x_height = params.accent_base_height;
        let m = crate::math::math_scripts::ScriptShiftResolver::resolve(
            has_sup, has_sub, style, false, nuc_h, nuc_d, sup_d, sub_h, x_height, params, drops,
        );
        (op_w, sub_x, m.sup_shift, m.sub_shift)
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
