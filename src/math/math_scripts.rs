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
        let mut u = match style {
            MathStyle::Display => s13,
            s if s.is_cramped() => s15,
            _ => s14,
        };
        // Ensure u - sup_d >= 1/4 of num_h
        let min_u = sup_d + 0.25 * num_h;
        if u < min_u {
            u = min_u;
        }
        u
    }

    /// Computes initial subscript shift $v$ according to Rule 18b.
    #[inline(always)]
    pub fn initial_sub_shift(num_d: f32, sub_h: f32, s16: f32, s17: f32) -> f32 {
        let mut v = s16.max(s17);
        let min_v = sub_h - 0.8 * (num_d + s16);
        if v < min_v {
            v = min_v;
        }
        v
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
}
