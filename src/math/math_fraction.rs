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

/// High-performance unified resolver for TeX Appendix G Rule 10 fraction and atop vertical shifts.
pub struct FractionShiftResolver;

impl FractionShiftResolver {
    /// Resolves actual numerator and denominator shifts obeying Appendix G Rule 10 in a single pass.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn resolve(
        style: MathStyle,
        has_rule: bool,
        axis_h: f32,
        rule_t: f32,
        num_d: f32,
        den_h: f32,
        scale: f32,
        params: &crate::math::math_fonts::MathFontParameters,
    ) -> PackedFractionMetrics {
        let disp_idx = style.is_display() as usize;

        if has_rule {
            let num_param = [params.num2, params.num1][disp_idx];
            let den_param = [params.denom2, params.denom1][disp_idx];
            let clr_param = [params.frac_clearance_text, params.frac_clearance_display][disp_idx];

            let num_shift = (num_param as f32) * scale;
            let den_shift = (den_param as f32) * scale;
            // Rule 15d: φ is 3θ in display style and θ otherwise; the font's own value holds
            // for its default rule, an explicit `\above` thickness scales it.
            let default_t = (params.rule_thickness as f32) * scale;
            let min_clr = if default_t > 0.0 && (rule_t - default_t).abs() > 1e-4 && rule_t > 0.0 {
                [1.0, 3.0][disp_idx] * rule_t
            } else {
                (clr_param as f32) * scale
            };

            let half_t = 0.5 * rule_t;
            let actual_num_shift = num_shift.max(axis_h + half_t + min_clr + num_d);
            let actual_den_shift = den_shift.max(half_t + min_clr + den_h - axis_h);

            PackedFractionMetrics::new(actual_num_shift, actual_den_shift, rule_t, min_clr)
        } else {
            let num_param = [params.num3, params.num1][disp_idx];
            let den_param = [params.denom2, params.denom1][disp_idx];
            let mult = [3.0, 7.0][disp_idx];
            let min_gap = (params.rule_thickness as f32 * mult) * scale;

            let num_shift = (num_param as f32) * scale;
            let den_shift = (den_param as f32) * scale;

            let mut actual_num_shift = num_shift.max(axis_h + num_d);
            let mut actual_den_shift = den_shift.max(den_h - axis_h);

            let gap = (actual_num_shift - num_d) - (-actual_den_shift + den_h);
            if gap < min_gap {
                let shortfall = min_gap - gap;
                actual_num_shift += shortfall * 0.5;
                actual_den_shift += shortfall * 0.5;
            }

            PackedFractionMetrics::new(actual_num_shift, actual_den_shift, 0.0, min_gap)
        }
    }
}

/// Branchless governor evaluating TeX Appendix G Rule 15 vertical clearances.
pub struct FractionClearanceGovernor;

impl FractionClearanceGovernor {
    /// Minimum clearance above the fraction bar: $3\theta$ in display style, $\theta$ otherwise.
    #[inline(always)]
    pub fn min_clearance_above(rule_thickness: f32, is_display: bool) -> f32 {
        let mult = [1.0, 3.0][is_display as usize];
        mult * rule_thickness
    }

    /// Minimum clearance below the fraction bar: $3\theta$ in display style, $\theta$ otherwise.
    #[inline(always)]
    pub fn min_clearance_below(rule_thickness: f32, is_display: bool) -> f32 {
        let mult = [1.0, 3.0][is_display as usize];
        mult * rule_thickness
    }

    /// Minimum clearance for atop/binomial constructions (no rule):
    /// $7\theta$ in display style, $3\theta$ in non-display styles.
    #[inline(always)]
    pub fn min_clearance_atop(rule_thickness: f32, is_display: bool) -> f32 {
        let mult = [3.0, 7.0][is_display as usize];
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

            let num_shift = num_shift_param.max(axis_h + half_t + min_num_clearance + num_d);
            let den_shift = den_shift_param.max(half_t + min_den_clearance + den_h - axis_h);

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

    /// Computes total width with padding, numerator offset, and denominator offset for centering.
    #[inline(always)]
    pub fn align_centered_padded(num_w: f32, den_w: f32, pad: f32) -> (f32, f32, f32) {
        let total_w = num_w.max(den_w) + 2.0 * pad;
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
    /// Resolves the delimiter glyph code for a given binomial content height via branchless threshold counts.
    #[inline(always)]
    pub fn resolve_glyph(&self, content_h: f32) -> u8 {
        let count = (content_h > self.height_tiers[0]) as usize
            + (content_h > self.height_tiers[1]) as usize
            + (content_h > self.height_tiers[2]) as usize
            + (content_h > self.height_tiers[3]) as usize
            + (content_h > self.height_tiers[4]) as usize
            + (content_h > self.height_tiers[5]) as usize
            + (content_h > self.height_tiers[6]) as usize;
        self.glyph_codes[count]
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

/// 64-byte aligned static metric table for skewed/slashed fractions (\Uskewed, \sfrac).
/// Matches TeX Live mlist.c lines 3023-3094 with canonical negative slash kerning.
#[repr(C, align(64))]
pub struct PackedSfracMetrics {
    pub num_raise: f32,
    pub den_drop: f32,
    pub slash_kern_before: f32,
    pub slash_kern_after: f32,
    pub _pad: [f32; 12],
}

pub static PACKED_SFRAC_METRICS: PackedSfracMetrics = PackedSfracMetrics {
    num_raise: 0.36,
    den_drop: 0.02,
    slash_kern_before: -0.12,
    slash_kern_after: -0.10,
    _pad: [0.0; 12],
};

const _: () = assert!(core::mem::size_of::<PackedSfracMetrics>() == 64);
const _: () = assert!(core::mem::align_of::<PackedSfracMetrics>() == 64);

/// Layout governor for skewed / slashed fractions (\Uskewed, \sfrac).
/// Matches TeX Live mlist.c lines 3023-3094.
pub struct SkewedFractionGovernor;

impl SkewedFractionGovernor {
    /// Computes horizontal and vertical displacements for skewed fractions.
    /// Returns (shift_up, shift_down, slash_x, den_x, total_w).
    #[inline(always)]
    pub fn compute_skewed_geometry(
        num_w: f32,
        den_w: f32,
        slash_w: f32,
        _axis_h: f32,
        font_size: f32,
    ) -> (f32, f32, f32, f32, f32) {
        let shift_up = PACKED_SFRAC_METRICS.num_raise * font_size;
        let shift_down = PACKED_SFRAC_METRICS.den_drop * font_size;
        let slash_x = (num_w + PACKED_SFRAC_METRICS.slash_kern_before * font_size).max(0.0);
        let den_x = (slash_x + slash_w + PACKED_SFRAC_METRICS.slash_kern_after * font_size).max(0.0);
        let total_w = den_x + den_w;
        (shift_up, shift_down, slash_x, den_x, total_w)
    }
}


