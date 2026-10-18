//! Superscript and subscript placement engine (TeX Appendix G Rule 18).
//!
//! Provides packed script metrics, branchless clearance governors,
//! single-pass shift optimization, and script nesting limiters.

use crate::math::math_ast::MathStyle;
use crate::math::math_box::MathBox;

/// 16-byte packed AoS structure storing superscript and subscript shift parameters.
///
/// Designed to fit exactly four instances per 64-byte L1 cache line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct PackedScriptMetrics {
    /// Calculated vertical shift of the superscript baseline above nucleus baseline ($u$).
    pub sup_shift: f32,
    /// Calculated vertical shift of the subscript baseline below nucleus baseline ($v$).
    pub sub_shift: f32,
    /// Superscript drop baseline offset ($\sigma_{18}$).
    pub sup_drop: f32,
    /// Subscript drop baseline offset ($\sigma_{19}$).
    pub sub_drop: f32,
}

const _: () = assert!(core::mem::size_of::<PackedScriptMetrics>() == 16);

impl PackedScriptMetrics {
    #[inline(always)]
    pub const fn new(sup_shift: f32, sub_shift: f32, sup_drop: f32, sub_drop: f32) -> Self {
        Self {
            sup_shift,
            sub_shift,
            sup_drop,
            sub_drop,
        }
    }
}

/// Branchless governor enforcing Rule 18c and 18d vertical clearances.
pub struct ScriptClearanceGovernor;

impl ScriptClearanceGovernor {
    /// Enforces minimum clearance between bottom of superscript and top of subscript ($\ge 4\theta$).
    #[inline(always)]
    pub fn enforce_combined_clearance(
        mut sup_shift: f32,
        mut sub_shift: f32,
        sup_d: f32,
        sub_h: f32,
        rule_t: f32,
        max_sub_top_drop: f32,
    ) -> (f32, f32) {
        let min_clearance = 4.0 * rule_t;
        let actual_clearance = (sup_shift - sup_d) - (sub_h - sub_shift);

        if actual_clearance < min_clearance {
            let deficit = min_clearance - actual_clearance;
            sub_shift += deficit;

            // Rule 18e: Top of subscript must not exceed 4/5 of the math axis height
            let sub_top = sub_h - sub_shift;
            if sub_top > max_sub_top_drop {
                let excess = sub_top - max_sub_top_drop;
                sub_shift += excess;
                sup_shift += excess;
            }
        }

        (sup_shift, sub_shift)
    }
}

/// Precomputed 8-byte table mapping MathStyle discriminant (0..8) to index into [sup2, sup1, sup3].
/// - 0 (Display) -> 1 (sup1)
/// - 1 (CrampedDisplay) -> 2 (sup3)
/// - 2 (Text) -> 0 (sup2)
/// - 3 (CrampedText) -> 2 (sup3)
/// - 4 (Script) -> 0 (sup2)
/// - 5 (CrampedScript) -> 2 (sup3)
/// - 6 (ScriptScript) -> 0 (sup2)
/// - 7 (CrampedScriptScript) -> 2 (sup3)
pub const SCRIPT_STYLE_TO_SUP_IDX: [u8; 8] = [1, 2, 0, 2, 0, 2, 0, 2];

/// High-performance unified resolver for TeX Appendix G Rule 18 subscript and superscript vertical shifts.
pub struct ScriptShiftResolver;

impl ScriptShiftResolver {
    /// Computes candidate shifts and enforces Rules 18a-18e constraints in a unified pass.
    ///
    /// `drops` holds `(sup_drop, sub_drop)` in points, taken from the font of the *script*
    /// size as in TeX §756 (`sup_drop(t)`, `sub_drop(t)`), not from the nucleus size.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn resolve(
        has_sup: bool,
        has_sub: bool,
        style: MathStyle,
        is_char: bool,
        effective_h: f32,
        effective_d: f32,
        sup_d: f32,
        sub_h: f32,
        x_height: f32,
        scale: f32,
        params: &crate::math::math_fonts::MathFontParameters,
        drops: (f32, f32),
    ) -> PackedScriptMetrics {
        // Rule 18a: Candidate superscript shift
        let sup_idx = SCRIPT_STYLE_TO_SUP_IDX[(style as usize) & 7] as usize;
        let sup_param = [params.sup2, params.sup1, params.sup3][sup_idx];
        let sup_shift_std = (sup_param as f32) * scale;
        let sup_drop = drops.0;
        let mut sup_shift = if is_char {
            sup_shift_std
        } else {
            sup_shift_std.max(effective_h - sup_drop)
        };

        // Rule 18b: Candidate subscript shift
        let sub_param = [params.sub1, params.sub2][has_sup as usize];
        let sub_shift_std = (sub_param as f32) * scale;
        let sub_drop = drops.1;
        let mut sub_shift = if is_char {
            sub_shift_std
        } else {
            sub_shift_std.max(effective_d + sub_drop)
        };

        // Rule 18c: Superscript elevation constraint (bottom at least 1/4 x-height above baseline)
        if has_sup {
            let min_sup_elevation = 0.25 * x_height + sup_d;
            sup_shift = sup_shift.max(min_sup_elevation);
        }

        // Rule 18d: Subscript drop constraint (top cannot exceed 4/5 x-height below baseline)
        if has_sub {
            let min_sub_drop = sub_h - 0.8 * x_height;
            if min_sub_drop > 0.0 {
                sub_shift = sub_shift.max(min_sub_drop);
            }
        }

        // Rule 18e: Clearance constraint between superscript bottom and subscript top
        if has_sup && has_sub {
            let gap = (sup_shift - sup_d) - (-sub_shift + sub_h);
            let min_clearance = (params.sub_sup_gap as f32) * scale;
            if gap < min_clearance {
                let shortfall = min_clearance - gap;
                sub_shift += shortfall;
                let min_sup_h = 0.8 * x_height + sup_d;
                if sup_shift < min_sup_h {
                    let push = min_sup_h - sup_shift;
                    sup_shift += push;
                    sub_shift -= push;
                }
            }
        }

        PackedScriptMetrics::new(sup_shift, sub_shift, sup_drop, sub_drop)
    }
}

/// Single-pass shift optimizer for subscripts and superscripts.
pub struct SubSupShiftOptimizer;

impl SubSupShiftOptimizer {
    /// Computes initial superscript shift $u$ according to Rule 18a.
    #[inline(always)]
    pub fn initial_sup_shift(
        style: MathStyle,
        num_h: f32,
        sup_d: f32,
        s13: f32,
        s14: f32,
        s15: f32,
    ) -> f32 {
        let sup_idx = SCRIPT_STYLE_TO_SUP_IDX[(style as usize) & 7] as usize;
        let u = [s14, s13, s15][sup_idx];
        let min_u = sup_d + 0.25 * num_h;
        u.max(min_u)
    }

    /// Computes initial subscript shift $v$ according to Rule 18b.
    #[inline(always)]
    pub fn initial_sub_shift(num_d: f32, sub_h: f32, s16: f32, s17: f32) -> f32 {
        let v = s16.max(s17);
        let min_v = sub_h - 0.8 * (num_d + s16);
        v.max(min_v)
    }
}

/// 64-byte aligned static table for kerning before scripts.
#[repr(C, align(64))]
pub struct KernBeforeScriptLut {
    /// Script kerning offsets for uppercase letters 'A'..='Z' (26 entries).
    pub upper_kerns: [i8; 26],
    /// Script kerning offsets for lowercase letters 'a'..='z' (26 entries).
    pub lower_kerns: [i8; 26],
    pub _pad: [u8; 12],
}

pub static KERN_BEFORE_SCRIPT_LUT: KernBeforeScriptLut = KernBeforeScriptLut {
    upper_kerns: [
        -30, // 'A'
        0,   // 'B'
        0,   // 'C'
        0,   // 'D'
        0,   // 'E'
        -40, // 'F'
        0,   // 'G'
        0,   // 'H'
        0,   // 'I'
        -30, // 'J'
        0,   // 'K'
        0,   // 'L'
        0,   // 'M'
        0,   // 'N'
        0,   // 'O'
        -50, // 'P'
        0,   // 'Q'
        0,   // 'R'
        0,   // 'S'
        -60, // 'T'
        0,   // 'U'
        -60, // 'V'
        -50, // 'W'
        0,   // 'X'
        -60, // 'Y'
        0,   // 'Z'
    ],
    lower_kerns: [0; 26], // Lowercase scripts typically need no negative kerning
    _pad: [0; 12],
};

const _: () = assert!(core::mem::size_of::<KernBeforeScriptLut>() == 64);
const _: () = assert!(core::mem::align_of::<KernBeforeScriptLut>() == 64);

impl KernBeforeScriptLut {
    /// Returns kerning in 1000-units-per-em before a subscript.
    #[inline(always)]
    pub fn sub_kern(&self, ch: char) -> i32 {
        if ch.is_ascii_uppercase() {
            let idx = (ch as u8 - b'A') as usize;
            self.upper_kerns[idx] as i32
        } else {
            0
        }
    }
}

/// Fast-path limiter saturating script nesting depth.
pub struct ScriptNestingLimiter;

impl ScriptNestingLimiter {
    /// Transitions to script style, saturating at ScriptScript ($SS$).
    #[inline(always)]
    pub fn next_script_style(style: MathStyle) -> MathStyle {
        style.sup_style()
    }
}

/// 32-byte non-owning pair of superscript and subscript spatial measurements.
///
/// Designed to fit in exactly half of a 64-byte L1 cache line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C, align(32))]
pub struct TransientScriptPair {
    pub sup_box: MathBox,
    pub sub_box: MathBox,
}

const _: () = assert!(core::mem::size_of::<TransientScriptPair>() == 32);
const _: () = assert!(core::mem::align_of::<TransientScriptPair>() == 32);

impl TransientScriptPair {
    #[inline(always)]
    pub const fn new(sup_box: MathBox, sub_box: MathBox) -> Self {
        Self { sup_box, sub_box }
    }

    /// Computes the horizontal right boundary of the nucleus with attached scripts.
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    pub fn right_bound(
        &self,
        has_sup: bool,
        has_sub: bool,
        origin_x: f32,
        nuc_w: f32,
        sup_x: f32,
        sub_x: f32,
        script_space: f32,
    ) -> f32 {
        let sup_r = if has_sup {
            sup_x + self.sup_box.w + script_space
        } else {
            0.0
        };
        let sub_r = if has_sub {
            sub_x + self.sub_box.w + script_space
        } else {
            0.0
        };
        (origin_x + nuc_w).max(sup_r).max(sub_r)
    }

    /// Height of the nucleus-plus-scripts hlist. A lowered subscript can still rise above the
    /// baseline (e.g. `{}_C`), and hpack takes the maximum over every box.
    #[inline(always)]
    pub fn total_height(&self, has_sup: bool, has_sub: bool, nuc_h: f32, sup_shift: f32, sub_shift: f32) -> f32 {
        let mut h = nuc_h;
        if has_sup {
            h = h.max(sup_shift + self.sup_box.h);
        }
        if has_sub {
            h = h.max(self.sub_box.h - sub_shift);
        }
        h
    }

    /// Depth of the nucleus-plus-scripts hlist (a raised superscript may still descend).
    #[inline(always)]
    pub fn total_depth(&self, has_sup: bool, has_sub: bool, nuc_d: f32, sup_shift: f32, sub_shift: f32) -> f32 {
        let mut d = nuc_d;
        if has_sub {
            d = d.max(sub_shift + self.sub_box.d);
        }
        if has_sup {
            d = d.max(self.sup_box.d - sup_shift);
        }
        d
    }
}
