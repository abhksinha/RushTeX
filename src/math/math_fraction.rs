//! Fraction and generalized binomial geometric layout engine (TeX Appendix G Rule 10).
//!
//! Provides packed fraction metrics, branchless clearance evaluation,
//! zero-allocation horizontal alignment, and binomial delimiter caches.

use crate::math::math_ast::MathStyle;
use crate::math::math_box::MathBox;

/// 16-byte packed AoS structure storing numerator shift, denominator shift,
/// rule thickness, and clearance delta for fractions.
///
/// Designed to fit exactly 4 packed structures per 64-byte L1 cache line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct PackedFractionMetrics {
    /// Vertical shift of the numerator baseline above the formula axis.
    pub num_shift: f32,
    /// Vertical shift of the denominator baseline below the formula axis.
    pub den_shift: f32,
    /// Typographic thickness of the fraction bar (0.0 for generalized binomials).
    pub rule_thickness: f32,
    /// Actual vertical clearance between numerator/denominator and the fraction bar.
    pub clearance: f32,
}

const _: () = assert!(core::mem::size_of::<PackedFractionMetrics>() == 16);

impl PackedFractionMetrics {
    /// Creates a new packed fraction metrics record.
    #[inline(always)]
    pub const fn new(num_shift: f32, den_shift: f32, rule_thickness: f32, clearance: f32) -> Self {
        Self {
            num_shift,
            den_shift,
            rule_thickness,
            clearance,
        }
    }
}

/// Branchless governor evaluating TeX Appendix G Rule 10 vertical clearances.
pub struct FractionClearanceGovernor;

impl FractionClearanceGovernor {
    /// Minimum clearance above the fraction bar: $3\theta$ in display style, $\theta$ otherwise.
    #[inline(always)]
    pub fn min_clearance_above(rule_thickness: f32, is_display: bool) -> f32 {
        let mult = if is_display { 3.0 } else { 1.0 };
        mult * rule_thickness
    }

    /// Minimum clearance below the fraction bar: $\theta$ in all styles.
    #[inline(always)]
    pub fn min_clearance_below(rule_thickness: f32, _is_display: bool) -> f32 {
        rule_thickness
    }

    /// Minimum clearance for atop/binomial constructions (no rule):
    /// $7\theta$ in display style, $3\theta$ in non-display styles.
    #[inline(always)]
    pub fn min_clearance_atop(rule_thickness: f32, is_display: bool) -> f32 {
        let mult = if is_display { 7.0 } else { 3.0 };
        mult * rule_thickness
    }

    /// Resolves actual numerator and denominator shifts obeying Appendix G Rule 10.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn resolve_shifts(
        style: MathStyle,
        axis_h: f32,
        rule_t: f32,
        num_d: f32,
        den_h: f32,
        num_shift_param: f32,
        den_shift_param: f32,
        has_rule: bool,
    ) -> (f32, f32) {
        let is_display = style.is_display();

        if has_rule {
            let half_t = 0.5 * rule_t;
            let min_num_clearance = Self::min_clearance_above(rule_t, is_display);
            let min_den_clearance = Self::min_clearance_below(rule_t, is_display);

            // Numerator shift: ensure (num_shift - num_d) - (axis_h + half_t) >= min_num_clearance
            let mut num_shift = num_shift_param;
            let actual_num_clearance = (num_shift - num_d) - (axis_h + half_t);
            if actual_num_clearance < min_num_clearance {
                num_shift += min_num_clearance - actual_num_clearance;
            }

            // Denominator shift: ensure (axis_h - half_t) - (den_h - den_shift) >= min_den_clearance
            let mut den_shift = den_shift_param;
            let actual_den_clearance = (axis_h - half_t) - (den_h - den_shift);
            if actual_den_clearance < min_den_clearance {
                den_shift += min_den_clearance - actual_den_clearance;
            }

            (num_shift, den_shift)
        } else {
            let min_atop_clearance = Self::min_clearance_atop(rule_t, is_display);
            let mut num_shift = num_shift_param;
            let mut den_shift = den_shift_param;
            let actual_clearance = (num_shift - num_d) - (den_h - den_shift);

            if actual_clearance < min_atop_clearance {
                let delta = 0.5 * (min_atop_clearance - actual_clearance);
                num_shift += delta;
                den_shift += delta;
            }

            (num_shift, den_shift)
        }
    }
}

/// Zero-allocation horizontal alignment resolver for fraction elements.
pub struct NumeratorDenominatorAligner;

impl NumeratorDenominatorAligner {
    /// Computes total width, numerator offset, and denominator offset for centering.
    #[inline(always)]
    pub fn align_centered(num_w: f32, den_w: f32) -> (f32, f32, f32) {
        let total_w = num_w.max(den_w);
        let num_x = 0.5 * (total_w - num_w);
        let den_x = 0.5 * (total_w - den_w);
        (total_w, num_x, den_x)
    }
}

/// 64-byte static lookup table mapping binomial/fraction height tiers to delimiter variants.
#[repr(C, align(64))]
pub struct BinomialDelimiterCache {
    /// Height thresholds for delimiter tiers in points for 10pt base font.
    pub height_tiers: [f32; 8],
    /// Adobe Symbol / CMEX glyph codes corresponding to each tier.
    pub glyph_codes: [u8; 8],
    pub _pad: [u8; 24],
}

pub static BINOMIAL_DELIMITER_CACHE: BinomialDelimiterCache = BinomialDelimiterCache {
    height_tiers: [10.0, 12.0, 15.0, 18.0, 24.0, 30.0, 36.0, 48.0],
    glyph_codes: [
        0x28, // '(' regular Roman
        0x28, // '(' regular Roman
        0x70, // CMEX small
        0x71, // CMEX medium
        0x72, // CMEX large
        0x73, // CMEX xlarge
        0xF3, // Extensible top
        0xF3, // Extensible top
    ],
    _pad: [0; 24],
};

const _: () = assert!(core::mem::size_of::<BinomialDelimiterCache>() == 64);
const _: () = assert!(core::mem::align_of::<BinomialDelimiterCache>() == 64);

impl BinomialDelimiterCache {
    /// Resolves the delimiter glyph code for a given binomial content height.
    #[inline(always)]
    pub fn resolve_glyph(&self, content_h: f32) -> u8 {
        let mut idx = 0usize;
        while idx < 7 && content_h > self.height_tiers[idx] {
            idx += 1;
        }
        self.glyph_codes[idx]
    }
}

/// Generalized fraction engine executing single-pass layouts for `\frac`, `\binom`, and `\genfrac`.
pub struct GeneralizedFractionEngine;

impl GeneralizedFractionEngine {
    /// Computes the bounding box of a fraction without intermediate tree transformations.
    #[inline(always)]
    pub fn compute_box(
        num_box: MathBox,
        den_box: MathBox,
        num_shift: f32,
        den_shift: f32,
        rule_t: f32,
        has_rule: bool,
    ) -> MathBox {
        let total_w = num_box.w.max(den_box.w);
        let top = num_shift + num_box.h;
        let bot = den_shift + den_box.d;

        let (total_h, total_d) = if has_rule {
            (top.max(rule_t), bot.max(rule_t))
        } else {
            (top, bot)
        };

        MathBox::with_dims(total_w, total_h, total_d)
    }
}

/// Evaluation pipeline for continued fractions `\cfrac`.
pub struct ContinuedFractionPipeline;

impl ContinuedFractionPipeline {
    /// Forces display style for continued fraction levels to prevent script shrinking.
    #[inline(always)]
    pub const fn cfrac_style() -> MathStyle {
        MathStyle::Display
    }
}
