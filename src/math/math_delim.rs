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

use crate::math::math_tfm::DelimiterRecipe;

/// 64-byte aligned static table of CMEX10 discrete delimiter variant chains.
#[repr(C, align(64))]
pub struct CmexDelimiterTable {
    pub paren_left: [u8; 4],
    pub paren_right: [u8; 4],
    pub bracket_left: [u8; 4],
    pub bracket_right: [u8; 4],
    pub brace_left: [u8; 4],
    pub brace_right: [u8; 4],
    pub floor_left: [u8; 4],
    pub floor_right: [u8; 4],
    pub ceil_left: [u8; 4],
    pub ceil_right: [u8; 4],
    pub angle_left: [u8; 4],
    pub angle_right: [u8; 4],
    pub _pad: [u8; 16],
}

const _: () = assert!(core::mem::size_of::<CmexDelimiterTable>() == 64);
const _: () = assert!(core::mem::align_of::<CmexDelimiterTable>() == 64);

pub static CMEX_DELIMITER_TABLE: CmexDelimiterTable = CmexDelimiterTable {
    paren_left: [0x00, 0x10, 0x12, 0x20],
    paren_right: [0x01, 0x11, 0x13, 0x21],
    bracket_left: [0x02, 0x68, 0x14, 0x22],
    bracket_right: [0x03, 0x69, 0x15, 0x23],
    brace_left: [0x08, 0x6e, 0x1a, 0x28],
    brace_right: [0x09, 0x6f, 0x1b, 0x29],
    floor_left: [0x04, 0x6a, 0x16, 0x24],
    floor_right: [0x05, 0x6b, 0x17, 0x25],
    ceil_left: [0x06, 0x6c, 0x18, 0x26],
    ceil_right: [0x07, 0x6d, 0x19, 0x27],
    angle_left: [0x0a, 0x44, 0x1c, 0x2a],
    angle_right: [0x0b, 0x45, 0x1d, 0x2b],
    _pad: [0; 16],
};

pub static CMEX_PAREN_RECIPE: DelimiterRecipe = DelimiterRecipe { top: 0x30, mid: 0, bot: 0x40, rep: 0x42 };
pub static CMEX_PAREN_RIGHT_RECIPE: DelimiterRecipe = DelimiterRecipe { top: 0x31, mid: 0, bot: 0x41, rep: 0x43 };
pub static CMEX_BRACKET_RECIPE: DelimiterRecipe = DelimiterRecipe { top: 0x32, mid: 0, bot: 0x34, rep: 0x36 };
pub static CMEX_BRACKET_RIGHT_RECIPE: DelimiterRecipe = DelimiterRecipe { top: 0x33, mid: 0, bot: 0x35, rep: 0x37 };
pub static CMEX_BRACE_RECIPE: DelimiterRecipe = DelimiterRecipe { top: 0x38, mid: 0x3c, bot: 0x3a, rep: 0x3e };
pub static CMEX_BRACE_RIGHT_RECIPE: DelimiterRecipe = DelimiterRecipe { top: 0x39, mid: 0x3d, bot: 0x3b, rep: 0x3e };
pub static CMEX_FLOOR_RECIPE: DelimiterRecipe = DelimiterRecipe { top: 0, mid: 0, bot: 0x34, rep: 0x36 };
pub static CMEX_FLOOR_RIGHT_RECIPE: DelimiterRecipe = DelimiterRecipe { top: 0, mid: 0, bot: 0x35, rep: 0x37 };
pub static CMEX_CEIL_RECIPE: DelimiterRecipe = DelimiterRecipe { top: 0x32, mid: 0, bot: 0, rep: 0x36 };
pub static CMEX_CEIL_RIGHT_RECIPE: DelimiterRecipe = DelimiterRecipe { top: 0x33, mid: 0, bot: 0, rep: 0x37 };

/// Resolves a mathematical delimiter into its CMEX10 variant chain and extensible assembly recipe.
#[inline(always)]
pub fn resolve_cmex_delimiter(delim: &str, is_left: bool) -> Option<(&'static [u8], Option<DelimiterRecipe>)> {
    let res = match delim {
        "(" => (&CMEX_DELIMITER_TABLE.paren_left[..], Some(CMEX_PAREN_RECIPE)),
        ")" => (&CMEX_DELIMITER_TABLE.paren_right[..], Some(CMEX_PAREN_RIGHT_RECIPE)),
        "[" => {
            if is_left {
                (&CMEX_DELIMITER_TABLE.bracket_left[..], Some(CMEX_BRACKET_RECIPE))
            } else {
                (&CMEX_DELIMITER_TABLE.bracket_right[..], Some(CMEX_BRACKET_RIGHT_RECIPE))
            }
        }
        "]" => (&CMEX_DELIMITER_TABLE.bracket_right[..], Some(CMEX_BRACKET_RIGHT_RECIPE)),
        "{" | "\\{" => {
            if is_left {
                (&CMEX_DELIMITER_TABLE.brace_left[..], Some(CMEX_BRACE_RECIPE))
            } else {
                (&CMEX_DELIMITER_TABLE.brace_right[..], Some(CMEX_BRACE_RIGHT_RECIPE))
            }
        }
        "}" | "\\}" => (&CMEX_DELIMITER_TABLE.brace_right[..], Some(CMEX_BRACE_RIGHT_RECIPE)),
        "\\lfloor" => (&CMEX_DELIMITER_TABLE.floor_left[..], Some(CMEX_FLOOR_RECIPE)),
        "\\rfloor" => (&CMEX_DELIMITER_TABLE.floor_right[..], Some(CMEX_FLOOR_RIGHT_RECIPE)),
        "\\lceil" => (&CMEX_DELIMITER_TABLE.ceil_left[..], Some(CMEX_CEIL_RECIPE)),
        "\\rceil" => (&CMEX_DELIMITER_TABLE.ceil_right[..], Some(CMEX_CEIL_RIGHT_RECIPE)),
        "\\langle" | "<" => (&CMEX_DELIMITER_TABLE.angle_left[..], None),
        "\\rangle" | ">" => (&CMEX_DELIMITER_TABLE.angle_right[..], None),
        _ => return None,
    };
    Some(res)
}

/// Static table of 1-byte ASCII strings for zero-allocation delimiter glyph emission.
pub static BYTE_STRINGS: [&str; 128] = [
    "\x00", "\x01", "\x02", "\x03", "\x04", "\x05", "\x06", "\x07",
    "\x08", "\x09", "\x0a", "\x0b", "\x0c", "\x0d", "\x0e", "\x0f",
    "\x10", "\x11", "\x12", "\x13", "\x14", "\x15", "\x16", "\x17",
    "\x18", "\x19", "\x1a", "\x1b", "\x1c", "\x1d", "\x1e", "\x1f",
    " ", "!", "\"", "#", "$", "%", "&", "'",
    "(", ")", "*", "+", ",", "-", ".", "/",
    "0", "1", "2", "3", "4", "5", "6", "7",
    "8", "9", ":", ";", "<", "=", ">", "?",
    "@", "A", "B", "C", "D", "E", "F", "G",
    "H", "I", "J", "K", "L", "M", "N", "O",
    "P", "Q", "R", "S", "T", "U", "V", "W",
    "X", "Y", "Z", "[", "\\", "]", "^", "_",
    "`", "a", "b", "c", "d", "e", "f", "g",
    "h", "i", "j", "k", "l", "m", "n", "o",
    "p", "q", "r", "s", "t", "u", "v", "w",
    "x", "y", "z", "{", "|", "}", "~", "\x7f",
];

/// Returns a static string slice for a given 7-bit ASCII byte code.
#[inline(always)]
pub fn byte_str(b: u8) -> &'static str {
    if (b as usize) < BYTE_STRINGS.len() {
        BYTE_STRINGS[b as usize]
    } else {
        "?"
    }
}
