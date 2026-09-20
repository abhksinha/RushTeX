//! Delimiter sizing and extensible assembly engine (TeX Appendix G Rule 19).
//!
//! Provides delimiter variant resolution, 4-piece extensible assembly,
//! null delimiter elimination, Rule 19 sizing formulas, and 1-byte delimiter pair matching.

use crate::math::math_box::MathBox;

/// 1-byte packed encoding of delimiter pairs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct DelimiterPairMatch(pub u8);

impl DelimiterPairMatch {
    pub const NULL: u8 = 0;
    pub const PAREN: u8 = 1;
    pub const BRACKET: u8 = 2;
    pub const BRACE: u8 = 3;
    pub const ANGLE: u8 = 4;
    pub const BAR: u8 = 5;
    pub const DOUBLE_BAR: u8 = 6;
    pub const FLOOR_CEIL: u8 = 7;

    #[inline(always)]
    pub const fn new(code: u8) -> Self {
        Self(code & 0x0F)
    }

    /// Whether this delimiter is a null delimiter (`.` in `\left.` or `\right.`).
    #[inline(always)]
    pub const fn is_null(self) -> bool {
        self.0 == Self::NULL
    }
}

/// 64-byte aligned static table mapping target heights to delimiter variants.
#[repr(C, align(64))]
pub struct DelimiterVariantResolver {
    /// Height thresholds for fixed delimiter sizes: small, medium, large, xlarge.
    pub thresholds: [f32; 4],
    /// Glyph indices in CMEX10 for each tier.
    pub cmex_indices: [u8; 4],
    pub _pad: [u8; 44],
}

pub static DELIMITER_VARIANT_RESOLVER: DelimiterVariantResolver = DelimiterVariantResolver {
    thresholds: [12.0, 18.0, 24.0, 30.0],
    cmex_indices: [0x00, 0x10, 0x20, 0x30],
    _pad: [0; 44],
};

const _: () = assert!(core::mem::size_of::<DelimiterVariantResolver>() == 64);
const _: () = assert!(core::mem::align_of::<DelimiterVariantResolver>() == 64);

impl DelimiterVariantResolver {
    /// Resolves whether a delimiter can be rendered with a single glyph or needs extensible assembly.
    #[inline(always)]
    pub fn resolve_variant(&self, target_h: f32) -> (usize, bool) {
        if target_h <= self.thresholds[0] {
            (0, false)
        } else if target_h <= self.thresholds[1] {
            (1, false)
        } else if target_h <= self.thresholds[2] {
            (2, false)
        } else if target_h <= self.thresholds[3] {
            (3, false)
        } else {
            (3, true) // Requires extensible assembly
        }
    }
}

/// Branchless bypass for null delimiters (`.` in `\left.` or `\right.`).
pub struct NullDelimiterEliminator;

impl NullDelimiterEliminator {
    /// Returns true if the delimiter is null (`"."` or `""`).
    #[inline(always)]
    pub fn is_null(delim: &str) -> bool {
        delim == "." || delim.is_empty()
    }

    /// Returns zero-sized box for null delimiters.
    #[inline(always)]
    pub fn null_box() -> MathBox {
        MathBox::zero()
    }
}

/// Branchless governor evaluating TeX Appendix G Rule 19 delimiter sizing formulas.
pub struct DelimiterSizingGovernor;

impl DelimiterSizingGovernor {
    /// Evaluates Rule 19 target delimiter height:
    /// $\max(h - \text{axis\_h}, d + \text{axis\_h}) \times 2 \times 0.9$
    #[inline(always)]
    pub fn target_height(content_h: f32, content_d: f32, axis_h: f32, min_h: f32) -> f32 {
        let delta1 = content_h - axis_h;
        let delta2 = content_d + axis_h;
        let max_delta = delta1.max(delta2);
        (2.0 * max_delta * 0.9).max(min_h)
    }
}

/// Single-pass assembly engine for 4-piece extensible delimiters (top, mid, bot, rep).
pub struct ExtensibleAssemblyEngine;

impl ExtensibleAssemblyEngine {
    /// Calculates the number of repeater pieces needed to cover target height.
    #[inline(always)]
    pub fn calculate_repeats(target_h: f32, piece_h: f32, fixed_h: f32) -> usize {
        if target_h <= fixed_h || piece_h <= 0.0 {
            0
        } else {
            let needed = target_h - fixed_h;
            (needed / piece_h).ceil() as usize
        }
    }
}
