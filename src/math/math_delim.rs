//! Delimiter sizing and extensible assembly engine (TeX Appendix G Rule 19).
//!
//! Provides delimiter variant resolution, 4-piece extensible assembly,
//! null delimiter elimination, Rule 19 sizing formulas, and 1-byte delimiter pair matching.

use crate::backend::fonts::Face;
use crate::math::math_ast::MathStyle;
use crate::math::math_box::MathBox;
use crate::math::math_layout::MathLayoutContext;
use crate::math::math_tfm::DelimiterRecipe;

/// 16-byte packed metrics storing dimensions and repeat counts for extensible delimiter assemblies.
///
/// Designed to fit four instances per 64-byte L1 cache line.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct ExtensibleAssemblyMetrics {
    /// Typographic advance width of the assembled delimiter.
    pub width: f32,
    /// Total vertical height of the stacked assembly.
    pub total_h: f32,
    /// Height of a single repeating piece ($h + d$).
    pub piece_h: f32,
    /// Number of repeaters per half (or total if no middle piece).
    pub n_rep: u32,
}

const _: () = assert!(core::mem::size_of::<ExtensibleAssemblyMetrics>() == 16);

impl ExtensibleAssemblyMetrics {
    #[inline(always)]
    pub const fn new(width: f32, total_h: f32, piece_h: f32, n_rep: u32) -> Self {
        Self {
            width,
            total_h,
            piece_h,
            n_rep,
        }
    }
}

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
        let count = (target_h > self.thresholds[0]) as usize
            + (target_h > self.thresholds[1]) as usize
            + (target_h > self.thresholds[2]) as usize
            + (target_h > self.thresholds[3]) as usize;
        if count < 4 {
            (count, false)
        } else {
            (3, true) // Requires extensible assembly
        }
    }

    /// Resolves delimiter tier index for CMEX10 variants with font scaling branchlessly.
    #[inline(always)]
    pub fn resolve_tier_scaled(&self, target_h: f32, scale: f32, has_recipe: bool, chain_len: usize) -> usize {
        let t0 = self.thresholds[0] * scale;
        let t1 = self.thresholds[1] * scale;
        let t2 = self.thresholds[2] * scale;
        let t3 = self.thresholds[3] * scale;
        let count = (target_h > t0) as usize
            + (target_h > t1) as usize
            + (target_h > t2) as usize
            + (target_h > t3) as usize;
        if count < 4 {
            count
        } else if !has_recipe {
            chain_len.saturating_sub(1)
        } else {
            4
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
    /// Evaluates Rule 19 target delimiter height according to canonical TeX and mlist.c:
    /// $\max(2 \delta_1 \times 0.901, 2 \delta_1 - 5\text{pt})$ where $\delta_1 = \max(h - a, d + a)$.
    #[inline(always)]
    pub fn target_height(content_h: f32, content_d: f32, axis_h: f32, min_h: f32) -> f32 {
        Self::target_height_with_shortfall(content_h, content_d, axis_h, min_h, 5.0)
    }

    /// Evaluates Rule 19 target delimiter height with explicit delimiter shortfall.
    #[inline(always)]
    pub fn target_height_with_shortfall(
        content_h: f32,
        content_d: f32,
        axis_h: f32,
        min_h: f32,
        shortfall: f32,
    ) -> f32 {
        let delta1 = (content_h - axis_h).max(content_d + axis_h).max(0.0);
        let z = 2.0 * delta1;
        let delta = z * 0.901;
        let delta2 = z - shortfall;
        delta.max(delta2).max(min_h)
    }

    /// Minimum delimiter extent for generalized fractions (\atopwithdelims, \binom) according to Rule 15e.
    #[inline(always)]
    pub fn atop_min_delimiter(is_display: bool, delim1: u16, scale: f32, font_size: f32) -> f32 {
        if is_display {
            (delim1 as f32) * scale
        } else {
            1.20 * font_size
        }
    }

    /// Computes composite bounding box for a delimited formula.
    #[inline(always)]
    pub fn compute_delimited_box(
        open_w: f32,
        open_h: f32,
        content_w: f32,
        content_h: f32,
        content_d: f32,
        close_w: f32,
        close_h: f32,
        target_h: f32,
        axis_h: f32,
    ) -> MathBox {
        let total_w = open_w + content_w + close_w;
        let actual_delim_h = open_h.max(close_h).max(target_h);
        let half_h = (axis_h + actual_delim_h * 0.5).max(content_h);
        let half_d = (actual_delim_h * 0.5 - axis_h).max(content_d);
        MathBox::with_dims(total_w, half_h, half_d)
    }
}

/// Helper for rendering vertical bar and double vertical bar delimiters.
pub struct DelimiterBarRenderer;

impl DelimiterBarRenderer {
    /// Renders single or double vertical bar rules according to TeX Rule 19.
    #[inline(always)]
    pub fn render_bar<'a, 'p>(
        ctx: &mut MathLayoutContext<'a, 'p>,
        delim: &str,
        x: f32,
        y_center: f32,
        target_h: f32,
        font_size: f32,
        rule_t: f32,
    ) -> Option<(f32, f32)> {
        if delim == "|" {
            let bar_h = target_h;
            let bar_y = y_center - bar_h * 0.5;
            let bar_t = rule_t;
            let bar_w = 0.278 * font_size;
            let bar_x = x + (bar_w - bar_t).max(0.0) * 0.5;
            if !ctx.is_measuring() {
                ctx.emit_rule(bar_x, bar_y, bar_t, bar_h);
            }
            Some((bar_w, bar_h))
        } else if delim == "\\|" || delim == "||" {
            let bar_h = target_h;
            let bar_y = y_center - bar_h * 0.5;
            let bar_t = rule_t;
            let sep_bars = 2.0 * bar_t;
            let bar_w = 0.398 * font_size;
            let bar_x = x + (bar_w - (2.0 * bar_t + sep_bars)).max(0.0) * 0.5;
            if !ctx.is_measuring() {
                ctx.emit_rule(bar_x, bar_y, bar_t, bar_h);
                ctx.emit_rule(bar_x + bar_t + sep_bars, bar_y, bar_t, bar_h);
            }
            Some((bar_w, bar_h))
        } else {
            None
        }
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

    /// Calculates the number of repeater pieces needed to cover target height,
    /// correctly accounting for symmetric top/bottom expansion when a middle piece is present.
    #[inline(always)]
    pub fn calculate_repeats_mid(target_h: f32, piece_h: f32, fixed_h: f32, has_mid: bool) -> usize {
        if target_h <= fixed_h || piece_h <= 0.0 {
            0
        } else {
            let denom = if has_mid { 2.0 * piece_h } else { piece_h };
            ((target_h - fixed_h) / denom).ceil() as usize
        }
    }

    /// Computes dimensions and repeat count for an extensible delimiter assembly.
    #[inline(always)]
    pub fn compute_assembly_geometry(
        recipe: DelimiterRecipe,
        target_h: f32,
        font_size: f32,
    ) -> ExtensibleAssemblyMetrics {
        let scale = font_size * 0.001;
        let m_rep = crate::math::math_tfm::cmex10_metric(recipe.rep);
        let width = (m_rep.w as f32) * scale;
        let piece_h = ((m_rep.h + m_rep.d) as f32) * scale;

        let h_top = if recipe.top != 0 {
            let m = crate::math::math_tfm::cmex10_metric(recipe.top);
            ((m.h + m.d) as f32) * scale
        } else {
            0.0
        };
        let h_bot = if recipe.bot != 0 {
            let m = crate::math::math_tfm::cmex10_metric(recipe.bot);
            ((m.h + m.d) as f32) * scale
        } else {
            0.0
        };
        let h_mid = if recipe.mid != 0 {
            let m = crate::math::math_tfm::cmex10_metric(recipe.mid);
            ((m.h + m.d) as f32) * scale
        } else {
            0.0
        };

        let fixed_h = h_top + h_bot + h_mid;
        let has_mid = recipe.mid != 0;
        let n_rep = Self::calculate_repeats_mid(target_h, piece_h, fixed_h, has_mid);
        let total_h = if has_mid {
            fixed_h + (2 * n_rep) as f32 * piece_h
        } else {
            fixed_h + (n_rep as f32) * piece_h
        };

        ExtensibleAssemblyMetrics {
            width,
            total_h,
            piece_h,
            n_rep: n_rep as u32,
        }
    }

    /// Emits all glyph pieces of an extensible delimiter assembly.
    #[inline(always)]
    pub fn emit_assembly<'a, 'p>(
        ctx: &mut MathLayoutContext<'a, 'p>,
        recipe: DelimiterRecipe,
        n_rep: usize,
        x: f32,
        y_center: f32,
        total_h: f32,
        font_size: f32,
    ) {
        if ctx.is_measuring() {
            return;
        }

        let scale = font_size * 0.001;
        let has_mid = recipe.mid != 0;
        let mut curr_top_y = y_center + total_h * 0.5;

        let mut emit_piece = |code: u8, top_y: &mut f32| {
            if code != 0 {
                let m = crate::math::math_tfm::cmex10_metric(code);
                let baseline_y = *top_y - (m.h as f32) * scale;
                let s = byte_str(code);
                ctx.emit_glyph_run(x, baseline_y, Face::Extension, font_size, s);
                *top_y = baseline_y - (m.d as f32) * scale;
            }
        };

        // TeX Appendix G Rule 19 vertical stack:
        // 1. Top piece
        emit_piece(recipe.top, &mut curr_top_y);

        // 2. Upper repeaters
        for _ in 0..n_rep {
            emit_piece(recipe.rep, &mut curr_top_y);
        }

        // 3. Middle piece (if any)
        emit_piece(recipe.mid, &mut curr_top_y);

        // 4. Lower repeaters (if middle piece exists)
        if has_mid {
            for _ in 0..n_rep {
                emit_piece(recipe.rep, &mut curr_top_y);
            }
        }

        // 5. Bottom piece
        emit_piece(recipe.bot, &mut curr_top_y);
    }
}

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
    pub slash: [u8; 4],
    pub backslash: [u8; 4],
    pub vert: [u8; 4],
    pub vert_double: [u8; 4],
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
    slash: [0x0e, 0x2e, 0x1e, 0x2c],
    backslash: [0x0f, 0x2f, 0x1f, 0x2d],
    vert: [0x0c, 0x0c, 0x0c, 0x0c],
    vert_double: [0x0d, 0x0d, 0x0d, 0x0d],
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
pub static CMEX_VERT_RECIPE: DelimiterRecipe = DelimiterRecipe { top: 0, mid: 0, bot: 0, rep: 0x0c };
pub static CMEX_VERT_DOUBLE_RECIPE: DelimiterRecipe = DelimiterRecipe { top: 0, mid: 0, bot: 0, rep: 0x0d };

/// Resolves a mathematical delimiter into its CMEX10 variant chain and extensible assembly recipe.
#[inline(always)]
pub fn resolve_cmex_delimiter(delim: &str, is_left: bool) -> Option<(&'static [u8], Option<DelimiterRecipe>)> {
    if delim.len() == 1 {
        let b = delim.as_bytes()[0];
        let res = match b {
            b'(' => (&CMEX_DELIMITER_TABLE.paren_left[..], Some(CMEX_PAREN_RECIPE)),
            b')' => (&CMEX_DELIMITER_TABLE.paren_right[..], Some(CMEX_PAREN_RIGHT_RECIPE)),
            b'[' => if is_left {
                (&CMEX_DELIMITER_TABLE.bracket_left[..], Some(CMEX_BRACKET_RECIPE))
            } else {
                (&CMEX_DELIMITER_TABLE.bracket_right[..], Some(CMEX_BRACKET_RIGHT_RECIPE))
            },
            b']' => (&CMEX_DELIMITER_TABLE.bracket_right[..], Some(CMEX_BRACKET_RIGHT_RECIPE)),
            b'{' => if is_left {
                (&CMEX_DELIMITER_TABLE.brace_left[..], Some(CMEX_BRACE_RECIPE))
            } else {
                (&CMEX_DELIMITER_TABLE.brace_right[..], Some(CMEX_BRACE_RIGHT_RECIPE))
            },
            b'}' => (&CMEX_DELIMITER_TABLE.brace_right[..], Some(CMEX_BRACE_RIGHT_RECIPE)),
            b'<' => (&CMEX_DELIMITER_TABLE.angle_left[..], None),
            b'>' => (&CMEX_DELIMITER_TABLE.angle_right[..], None),
            b'/' => (&CMEX_DELIMITER_TABLE.slash[..], None),
            b'\\' => (&CMEX_DELIMITER_TABLE.backslash[..], None),
            b'|' => (&CMEX_DELIMITER_TABLE.vert[..], Some(CMEX_VERT_RECIPE)),
            _ => return None,
        };
        return Some(res);
    }
    let res = match delim {
        "\\{" => {
            if is_left {
                (&CMEX_DELIMITER_TABLE.brace_left[..], Some(CMEX_BRACE_RECIPE))
            } else {
                (&CMEX_DELIMITER_TABLE.brace_right[..], Some(CMEX_BRACE_RIGHT_RECIPE))
            }
        }
        "\\}" => (&CMEX_DELIMITER_TABLE.brace_right[..], Some(CMEX_BRACE_RIGHT_RECIPE)),
        "\\lfloor" => (&CMEX_DELIMITER_TABLE.floor_left[..], Some(CMEX_FLOOR_RECIPE)),
        "\\rfloor" => (&CMEX_DELIMITER_TABLE.floor_right[..], Some(CMEX_FLOOR_RIGHT_RECIPE)),
        "\\lceil" => (&CMEX_DELIMITER_TABLE.ceil_left[..], Some(CMEX_CEIL_RECIPE)),
        "\\rceil" => (&CMEX_DELIMITER_TABLE.ceil_right[..], Some(CMEX_CEIL_RIGHT_RECIPE)),
        "\\langle" => (&CMEX_DELIMITER_TABLE.angle_left[..], None),
        "\\rangle" => (&CMEX_DELIMITER_TABLE.angle_right[..], None),
        "\\backslash" => (&CMEX_DELIMITER_TABLE.backslash[..], None),
        "\\vert" | "\\mid" => (&CMEX_DELIMITER_TABLE.vert[..], Some(CMEX_VERT_RECIPE)),
        "\\|" | "\\Vert" | "\\parallel" => (&CMEX_DELIMITER_TABLE.vert_double[..], Some(CMEX_VERT_DOUBLE_RECIPE)),
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

/// Compact 16-byte packed metrics for over/under delimiter gaps (mlist.c make_over/under_delimiter).
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct OverUnderGapEntry {
    /// Minimum vertical clearance between delimiter and nucleus (over_delimiter_vgap / under_delimiter_vgap).
    pub vgap: f32,
    /// Baseline-to-baseline distance (over_delimiter_bgap / under_delimiter_bgap).
    pub bgap: f32,
    pub _reserved: [f32; 2],
}

const _: () = assert!(core::mem::size_of::<OverUnderGapEntry>() == 16);

/// 64-byte aligned table defining over/under delimiter clearances across math styles.
#[derive(Clone, Copy, Debug)]
#[repr(C, align(64))]
pub struct OverUnderGapTable {
    pub display: OverUnderGapEntry,
    pub text: OverUnderGapEntry,
    pub script: OverUnderGapEntry,
    pub script_script: OverUnderGapEntry,
}

impl OverUnderGapTable {
    /// O(1) branchless style lookup into the 64-byte aligned gap table.
    #[inline(always)]
    pub fn entry(&self, style: MathStyle) -> &OverUnderGapEntry {
        match ((style as usize) >> 1).min(3) {
            0 => &self.display,
            1 => &self.text,
            2 => &self.script,
            _ => &self.script_script,
        }
    }
}

pub static OVER_UNDER_GAP_TABLE: OverUnderGapTable = OverUnderGapTable {
    display: OverUnderGapEntry { vgap: 3.0, bgap: 6.0, _reserved: [0.0; 2] },
    text: OverUnderGapEntry { vgap: 3.0, bgap: 6.0, _reserved: [0.0; 2] },
    script: OverUnderGapEntry { vgap: 2.1, bgap: 4.2, _reserved: [0.0; 2] },
    script_script: OverUnderGapEntry { vgap: 1.5, bgap: 3.0, _reserved: [0.0; 2] },
};

const _: () = assert!(core::mem::size_of::<OverUnderGapTable>() == 64);
const _: () = assert!(core::mem::align_of::<OverUnderGapTable>() == 64);

/// Engine implementing mlist.c make_delimiter_over, make_delimiter_under, and wrapup_over_under_delimiter.
pub struct OverUnderDelimiterEngine;

impl OverUnderDelimiterEngine {
    /// Computes shift_up for an over-delimiter (e.g. \overbrace, \overbracket) over a nucleus box.
    #[inline(always)]
    pub fn compute_over_shift(delim_h: f32, delim_d: f32, nuc_h: f32, scale: f32, style: MathStyle) -> f32 {
        let entry = OVER_UNDER_GAP_TABLE.entry(style);
        let bgap = entry.bgap * scale;
        let vgap = entry.vgap * scale;
        (bgap - delim_h - delim_d).max(vgap + nuc_h)
    }

    /// Computes shift_down for an under-delimiter (e.g. \underbrace, \underbracket) below a nucleus box.
    #[inline(always)]
    pub fn compute_under_shift(delim_h: f32, delim_d: f32, nuc_d: f32, scale: f32, style: MathStyle) -> f32 {
        let entry = OVER_UNDER_GAP_TABLE.entry(style);
        let bgap = entry.bgap * scale;
        let vgap = entry.vgap * scale;
        (bgap - delim_h - delim_d).max(vgap + nuc_d)
    }

    /// Implements mlist.c wrapup_over_under_delimiter:
    /// Constructs enclosing vertical box dimensions from (shift_up, shift_down, x, y).
    #[inline(always)]
    pub fn wrapup_dims(x_w: f32, x_h: f32, y_w: f32, y_d: f32, shift_up: f32, shift_down: f32) -> MathBox {
        let w = x_w.max(y_w);
        let h = shift_up + x_h;
        let d = y_d + shift_down;
        MathBox::with_dims(w, h, d)
    }
}

/// Compact 16-byte packed metrics record for extensible arrows (\xrightarrow, \xleftarrow, \overleftrightarrow).
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExtensibleArrowMetrics {
    pub target_w: f32,
    pub shaft_y: f32,
    pub rule_start_x: f32,
    pub rule_w: f32,
}

const _: () = assert!(core::mem::size_of::<ExtensibleArrowMetrics>() == 16);
const _: () = assert!(core::mem::align_of::<ExtensibleArrowMetrics>() == 16);

/// Engine computing geometry for extensible arrows and vector shafts.
pub struct ExtensibleArrowEngine;

impl ExtensibleArrowEngine {
    /// Computes target width, shaft baseline, and rule coordinates in a single pass.
    #[inline(always)]
    pub fn compute_arrow_geometry(
        origin_x: f32,
        origin_y: f32,
        axis_h: f32,
        sup_w: f32,
        sub_w: f32,
        arrow_w: f32,
        font_size: f32,
        is_left: bool,
        is_both: bool,
    ) -> (ExtensibleArrowMetrics, f32, f32) {
        let pad = 0.5 * font_size;
        let min_w = 1.4 * font_size;
        let target_w = (sup_w.max(sub_w) + 2.0 * pad).max(min_w);
        let shaft_y = origin_y + axis_h;
        let left_x = origin_x;
        let right_x = origin_x + target_w - arrow_w;

        let (rule_start_x, rule_w) = if is_both {
            let start = left_x + arrow_w * 0.5;
            let w = (right_x + arrow_w * 0.5 - start).max(1.0);
            (start, w)
        } else if is_left {
            let start = left_x + arrow_w * 0.5;
            let w = (origin_x + target_w - start).max(1.0);
            (start, w)
        } else {
            let start = origin_x;
            let w = (right_x + arrow_w * 0.5 - origin_x).max(1.0);
            (start, w)
        };

        (
            ExtensibleArrowMetrics {
                target_w,
                shaft_y,
                rule_start_x,
                rule_w,
            },
            left_x,
            right_x,
        )
    }
}

/// Compact 32-byte packed metrics record for horizontal extensible braces (\overbrace, \underbrace).
#[repr(C, align(32))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HorizontalBraceMetrics {
    pub target_w: f32,
    pub brace_h: f32,
    pub gap: f32,
    pub corner_w: f32,
    pub min_brace_w: f32,
    pub rule_seg_w: f32,
    pub nuc_offset_x: f32,
    pub brace_y: f32,
}

const _: () = assert!(core::mem::size_of::<HorizontalBraceMetrics>() == 32);
const _: () = assert!(core::mem::align_of::<HorizontalBraceMetrics>() == 32);

/// Engine computing geometry and segmentation for extensible horizontal braces.
pub struct HorizontalBraceEngine;

impl HorizontalBraceEngine {
    /// Computes geometry for an overbrace above a nucleus.
    #[inline(always)]
    pub fn compute_overbrace_metrics(
        nuc_w: f32,
        nuc_h: f32,
        origin_y: f32,
        corner_w: f32,
        brace_h: f32,
        gap: f32,
    ) -> HorizontalBraceMetrics {
        let min_brace_w = 4.0 * corner_w;
        let target_w = nuc_w.max(min_brace_w);
        let nuc_offset_x = (target_w - nuc_w) * 0.5;
        let brace_y = origin_y + nuc_h + gap;
        let rule_seg_w = ((target_w - min_brace_w) * 0.5).max(0.0);

        HorizontalBraceMetrics {
            target_w,
            brace_h,
            gap,
            corner_w,
            min_brace_w,
            rule_seg_w,
            nuc_offset_x,
            brace_y,
        }
    }

    /// Computes geometry for an underbrace below a nucleus.
    #[inline(always)]
    pub fn compute_underbrace_metrics(
        nuc_w: f32,
        nuc_d: f32,
        origin_y: f32,
        corner_w: f32,
        brace_h: f32,
        gap: f32,
    ) -> HorizontalBraceMetrics {
        let min_brace_w = 4.0 * corner_w;
        let target_w = nuc_w.max(min_brace_w);
        let nuc_offset_x = (target_w - nuc_w) * 0.5;
        let brace_y = origin_y - nuc_d - gap - brace_h;
        let rule_seg_w = ((target_w - min_brace_w) * 0.5).max(0.0);

        HorizontalBraceMetrics {
            target_w,
            brace_h,
            gap,
            corner_w,
            min_brace_w,
            rule_seg_w,
            nuc_offset_x,
            brace_y,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delimiter_branchless_tier_resolution() {
        let resolver = &DELIMITER_VARIANT_RESOLVER;

        // Exactly at thresholds
        assert_eq!(resolver.resolve_variant(12.0), (0, false));
        assert_eq!(resolver.resolve_variant(18.0), (1, false));
        assert_eq!(resolver.resolve_variant(24.0), (2, false));
        assert_eq!(resolver.resolve_variant(30.0), (3, false));
        assert_eq!(resolver.resolve_variant(30.1), (3, true));

        // Scaled resolution
        assert_eq!(resolver.resolve_tier_scaled(10.0, 1.0, true, 4), 0);
        assert_eq!(resolver.resolve_tier_scaled(15.0, 1.0, true, 4), 1);
        assert_eq!(resolver.resolve_tier_scaled(20.0, 1.0, true, 4), 2);
        assert_eq!(resolver.resolve_tier_scaled(28.0, 1.0, true, 4), 3);
        assert_eq!(resolver.resolve_tier_scaled(35.0, 1.0, true, 4), 4);
        assert_eq!(resolver.resolve_tier_scaled(35.0, 1.0, false, 4), 3);
    }

    #[test]
    fn test_delimiter_ascii_dispatch_parity() {
        // 1-byte ASCII vs string command equivalents
        assert_eq!(resolve_cmex_delimiter("(", true).unwrap().0, resolve_cmex_delimiter("(", false).unwrap().0);
        assert_eq!(resolve_cmex_delimiter("{", true).unwrap().0, resolve_cmex_delimiter("\\{", true).unwrap().0);
        assert_eq!(resolve_cmex_delimiter("}", false).unwrap().0, resolve_cmex_delimiter("\\}", false).unwrap().0);
        assert_eq!(resolve_cmex_delimiter("|", false).unwrap().0, resolve_cmex_delimiter("\\vert", false).unwrap().0);
        assert_eq!(resolve_cmex_delimiter("<", false).unwrap().0, resolve_cmex_delimiter("\\langle", false).unwrap().0);
        assert_eq!(resolve_cmex_delimiter(">", false).unwrap().0, resolve_cmex_delimiter("\\rangle", false).unwrap().0);
    }

    #[test]
    fn test_over_under_gap_table_indexing() {
        assert_eq!(OVER_UNDER_GAP_TABLE.entry(MathStyle::Display).vgap, 3.0);
        assert_eq!(OVER_UNDER_GAP_TABLE.entry(MathStyle::CrampedDisplay).vgap, 3.0);
        assert_eq!(OVER_UNDER_GAP_TABLE.entry(MathStyle::Text).vgap, 3.0);
        assert_eq!(OVER_UNDER_GAP_TABLE.entry(MathStyle::CrampedText).vgap, 3.0);
        assert_eq!(OVER_UNDER_GAP_TABLE.entry(MathStyle::Script).vgap, 2.1);
        assert_eq!(OVER_UNDER_GAP_TABLE.entry(MathStyle::CrampedScript).vgap, 2.1);
        assert_eq!(OVER_UNDER_GAP_TABLE.entry(MathStyle::ScriptScript).vgap, 1.5);
        assert_eq!(OVER_UNDER_GAP_TABLE.entry(MathStyle::CrampedScriptScript).vgap, 1.5);
    }

    #[test]
    fn test_extensible_repeats_mid() {
        assert_eq!(ExtensibleAssemblyEngine::calculate_repeats_mid(40.0, 10.0, 20.0, false), 2);
        assert_eq!(ExtensibleAssemblyEngine::calculate_repeats_mid(40.0, 10.0, 20.0, true), 1);
        assert_eq!(ExtensibleAssemblyEngine::calculate_repeats_mid(15.0, 10.0, 20.0, true), 0);
    }

    #[test]
    fn test_extensible_arrow_and_horizontal_brace_acceleration() {
        // 1. ExtensibleArrowEngine geometry
        let (arrow_geom, left_x, right_x) = ExtensibleArrowEngine::compute_arrow_geometry(
            0.0,
            10.0,
            2.5,
            20.0,
            15.0,
            5.0,
            10.0,
            false,
            false,
        );
        assert_eq!(arrow_geom.shaft_y, 12.5);
        assert_eq!(arrow_geom.target_w, 30.0); // 20 + 2 * 5.0
        assert_eq!(left_x, 0.0);
        assert_eq!(right_x, 25.0);
        assert_eq!(arrow_geom.rule_start_x, 0.0);
        assert_eq!(arrow_geom.rule_w, 27.5);

        // 2. HorizontalBraceEngine geometry
        let over_geom = HorizontalBraceEngine::compute_overbrace_metrics(
            30.0,
            8.0,
            0.0,
            4.0,
            6.0,
            3.0,
        );
        assert_eq!(over_geom.target_w, 30.0);
        assert_eq!(over_geom.brace_y, 11.0); // 8.0 + 3.0
        assert_eq!(over_geom.min_brace_w, 16.0);
        assert_eq!(over_geom.rule_seg_w, 7.0); // (30 - 16) * 0.5

        let under_geom = HorizontalBraceEngine::compute_underbrace_metrics(
            30.0,
            2.0,
            0.0,
            4.0,
            6.0,
            3.0,
        );
        assert_eq!(under_geom.target_w, 30.0);
        assert_eq!(under_geom.brace_y, -11.0); // -2.0 - 3.0 - 6.0
        assert_eq!(under_geom.rule_seg_w, 7.0);
    }
}

