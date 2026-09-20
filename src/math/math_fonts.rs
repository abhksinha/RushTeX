//! Symbol metric tables and command-to-glyph mappings for mathematics.
//!
//! Maps standard LaTeX math commands directly to Adobe Type 1 Symbol font byte codes
//! and font faces, with zero dynamic heap allocations.

use crate::backend::fonts::{self as metrics, Face};
use crate::math::math_spacing::{MathAtomKind, KERN_PAIR_LUT};
pub use crate::math::math_tfm::*;

/// Resolved symbol specification.
#[derive(Clone, Copy, Debug)]
pub struct MathSymbolDef {
    pub kind: MathAtomKind,
    pub face: Face,
    pub glyph: &'static str,
    pub code: Option<u8>,
}

/// Resolves a LaTeX math command (e.g. "alpha", "sum", "partial") to its font face,
/// atom kind, and Adobe Symbol byte code.
pub fn lookup_math_command(cmd: &str) -> Option<MathSymbolDef> {
    let def = match cmd {
        // Greek Lowercase
        "alpha" => (MathAtomKind::Ord, Face::Symbol, "a", Some(b'a')),
        "beta" => (MathAtomKind::Ord, Face::Symbol, "b", Some(b'b')),
        "gamma" => (MathAtomKind::Ord, Face::Symbol, "g", Some(b'g')),
        "delta" => (MathAtomKind::Ord, Face::Symbol, "d", Some(b'd')),
        "epsilon" | "varepsilon" => (MathAtomKind::Ord, Face::Symbol, "e", Some(b'e')),
        "zeta" => (MathAtomKind::Ord, Face::Symbol, "z", Some(b'z')),
        "eta" => (MathAtomKind::Ord, Face::Symbol, "h", Some(b'h')),
        "theta" | "vartheta" => (MathAtomKind::Ord, Face::Symbol, "q", Some(b'q')),
        "iota" => (MathAtomKind::Ord, Face::Symbol, "i", Some(b'i')),
        "kappa" => (MathAtomKind::Ord, Face::Symbol, "k", Some(b'k')),
        "lambda" => (MathAtomKind::Ord, Face::Symbol, "l", Some(b'l')),
        "mu" => (MathAtomKind::Ord, Face::Symbol, "m", Some(b'm')),
        "nu" => (MathAtomKind::Ord, Face::Symbol, "n", Some(b'n')),
        "xi" => (MathAtomKind::Ord, Face::Symbol, "x", Some(b'x')),
        "pi" | "varpi" => (MathAtomKind::Ord, Face::Symbol, "p", Some(b'p')),
        "rho" => (MathAtomKind::Ord, Face::Symbol, "r", Some(b'r')),
        "sigma" | "varsigma" => (MathAtomKind::Ord, Face::Symbol, "s", Some(b's')),
        "tau" => (MathAtomKind::Ord, Face::Symbol, "t", Some(b't')),
        "upsilon" => (MathAtomKind::Ord, Face::Symbol, "u", Some(b'u')),
        "phi" | "varphi" => (MathAtomKind::Ord, Face::Symbol, "f", Some(b'f')),
        "chi" => (MathAtomKind::Ord, Face::Symbol, "c", Some(b'c')),
        "psi" => (MathAtomKind::Ord, Face::Symbol, "y", Some(b'y')),
        "omega" => (MathAtomKind::Ord, Face::Symbol, "w", Some(b'w')),

        // Greek Uppercase
        "Gamma" => (MathAtomKind::Ord, Face::Symbol, "G", Some(b'G')),
        "Delta" => (MathAtomKind::Ord, Face::Symbol, "D", Some(b'D')),
        "Theta" => (MathAtomKind::Ord, Face::Symbol, "Q", Some(b'Q')),
        "Lambda" => (MathAtomKind::Ord, Face::Symbol, "L", Some(b'L')),
        "Xi" => (MathAtomKind::Ord, Face::Symbol, "X", Some(b'X')),
        "Pi" => (MathAtomKind::Ord, Face::Symbol, "P", Some(b'P')),
        "Sigma" => (MathAtomKind::Ord, Face::Symbol, "S", Some(b'S')),
        "Upsilon" => (MathAtomKind::Ord, Face::Symbol, "U", Some(b'U')),
        "Phi" => (MathAtomKind::Ord, Face::Symbol, "F", Some(b'F')),
        "Psi" => (MathAtomKind::Ord, Face::Symbol, "Y", Some(b'Y')),
        "Omega" => (MathAtomKind::Ord, Face::Symbol, "W", Some(b'W')),

        // Calculus & Physics
        "partial" => (MathAtomKind::Ord, Face::Symbol, "\u{00B6}", Some(0xB6)),
        "nabla" => (MathAtomKind::Ord, Face::Symbol, "\u{00D1}", Some(0xD1)),
        "infty" => (MathAtomKind::Ord, Face::Symbol, "\u{00A5}", Some(0xA5)),
        "int" => (MathAtomKind::Op, Face::Symbol, "\u{00F2}", Some(0xF2)),
        "iint" => (
            MathAtomKind::Op,
            Face::Symbol,
            "\u{00F2}\u{00F2}",
            Some(0xF2),
        ),
        "sum" => (MathAtomKind::Op, Face::Symbol, "\u{00E5}", Some(0xE5)),
        "prod" => (MathAtomKind::Op, Face::Symbol, "\u{00D5}", Some(0xD5)),

        // Binary Operators
        "times" => (MathAtomKind::Bin, Face::Symbol, "\u{00B4}", Some(0xB4)),
        "pm" => (MathAtomKind::Bin, Face::Symbol, "\u{00B1}", Some(0xB1)),
        "mp" => (MathAtomKind::Bin, Face::Symbol, "\u{00B1}", Some(0xB1)),
        "div" => (MathAtomKind::Bin, Face::Symbol, "\u{00B8}", Some(0xB8)),
        "cdot" => (MathAtomKind::Bin, Face::Symbol, "\u{00B7}", Some(0xD7)),
        "cap" => (MathAtomKind::Bin, Face::Symbol, "\u{00C7}", Some(0xC7)),
        "cup" => (MathAtomKind::Bin, Face::Symbol, "\u{00C8}", Some(0xC8)),
        "oplus" => (MathAtomKind::Bin, Face::Symbol, "\u{00C4}", Some(0xC4)),
        "otimes" => (MathAtomKind::Bin, Face::Symbol, "\u{00C5}", Some(0xC5)),
        "ast" | "star" => (MathAtomKind::Bin, Face::Symbol, "*", Some(0x2A)),
        "vee" | "lor" => (MathAtomKind::Bin, Face::Symbol, "\u{00DA}", Some(0xDA)),
        "wedge" | "land" => (MathAtomKind::Bin, Face::Symbol, "\u{00D9}", Some(0xD9)),
        "circ" => (MathAtomKind::Bin, Face::Symbol, "\u{00B0}", Some(0xB0)),
        "bullet" => (MathAtomKind::Bin, Face::Symbol, "\u{00B7}", Some(0xD7)),
        "setminus" => (MathAtomKind::Bin, Face::Symbol, "\\", Some(b'\\')),

        // Standard Functions (Roman Operators)
        "sin" => (MathAtomKind::Op, Face::Roman, "sin", None),
        "cos" => (MathAtomKind::Op, Face::Roman, "cos", None),
        "tan" => (MathAtomKind::Op, Face::Roman, "tan", None),
        "cot" => (MathAtomKind::Op, Face::Roman, "cot", None),
        "sec" => (MathAtomKind::Op, Face::Roman, "sec", None),
        "csc" => (MathAtomKind::Op, Face::Roman, "csc", None),
        "arcsin" => (MathAtomKind::Op, Face::Roman, "arcsin", None),
        "arccos" => (MathAtomKind::Op, Face::Roman, "arccos", None),
        "arctan" => (MathAtomKind::Op, Face::Roman, "arctan", None),
        "sinh" => (MathAtomKind::Op, Face::Roman, "sinh", None),
        "cosh" => (MathAtomKind::Op, Face::Roman, "cosh", None),
        "tanh" => (MathAtomKind::Op, Face::Roman, "tanh", None),
        "log" => (MathAtomKind::Op, Face::Roman, "log", None),
        "ln" => (MathAtomKind::Op, Face::Roman, "ln", None),
        "exp" => (MathAtomKind::Op, Face::Roman, "exp", None),
        "lg" => (MathAtomKind::Op, Face::Roman, "lg", None),
        "deg" => (MathAtomKind::Op, Face::Roman, "deg", None),
        "dim" => (MathAtomKind::Op, Face::Roman, "dim", None),
        "hom" => (MathAtomKind::Op, Face::Roman, "hom", None),
        "ker" => (MathAtomKind::Op, Face::Roman, "ker", None),
        "arg" => (MathAtomKind::Op, Face::Roman, "arg", None),
        "coth" => (MathAtomKind::Op, Face::Roman, "coth", None),
        "Pr" => (MathAtomKind::Op, Face::Roman, "Pr", None),

        // Relations
        "le" | "leq" => (MathAtomKind::Rel, Face::Symbol, "\u{00A3}", Some(0xA3)),
        "ge" | "geq" => (MathAtomKind::Rel, Face::Symbol, "\u{00B3}", Some(0xB3)),
        "ne" | "neq" => (MathAtomKind::Rel, Face::Symbol, "\u{00B9}", Some(0xB9)),
        "approx" => (MathAtomKind::Rel, Face::Symbol, "\u{00BB}", Some(0xBB)),
        "equiv" => (MathAtomKind::Rel, Face::Symbol, "\u{00BA}", Some(0xBA)),
        "sim" => (MathAtomKind::Rel, Face::Symbol, "~", Some(0x7E)),
        "simeq" | "cong" => (MathAtomKind::Rel, Face::Symbol, "\u{0040}", Some(0x40)),
        "propto" => (MathAtomKind::Rel, Face::Symbol, "\u{00B5}", Some(0xB5)),
        "in" => (MathAtomKind::Rel, Face::Symbol, "\u{00CE}", Some(0xCE)),
        "notin" => (MathAtomKind::Rel, Face::Symbol, "\u{00CF}", Some(0xCF)),
        "subset" => (MathAtomKind::Rel, Face::Symbol, "\u{00CC}", Some(0xCC)),
        "supset" => (MathAtomKind::Rel, Face::Symbol, "\u{00C9}", Some(0xC9)),
        "subseteq" => (MathAtomKind::Rel, Face::Symbol, "\u{00CD}", Some(0xCD)),
        "supseteq" => (MathAtomKind::Rel, Face::Symbol, "\u{00CA}", Some(0xCA)),
        "perp" => (MathAtomKind::Rel, Face::Symbol, "^", Some(0x5E)),
        "mid" => (MathAtomKind::Rel, Face::Symbol, "|", Some(b'|')),

        // Arrows & Logic
        "to" | "rightarrow" | "longrightarrow" | "mapsto" => {
            (MathAtomKind::Rel, Face::Symbol, "\u{00AE}", Some(0xAE))
        }
        "leftarrow" | "longleftarrow" => (MathAtomKind::Rel, Face::Symbol, "\u{00AC}", Some(0xAC)),
        "Rightarrow" | "implies" => (MathAtomKind::Rel, Face::Symbol, "\u{00DE}", Some(0xDE)),
        "Leftarrow" => (MathAtomKind::Rel, Face::Symbol, "\u{00DC}", Some(0xDC)),
        "Leftrightarrow" | "iff" | "longleftrightarrow" => {
            (MathAtomKind::Rel, Face::Symbol, "\u{00DB}", Some(0xDB))
        }
        "uparrow" => (MathAtomKind::Rel, Face::Symbol, "\u{00AD}", Some(0xAD)),
        "downarrow" => (MathAtomKind::Rel, Face::Symbol, "\u{00AF}", Some(0xAF)),
        "Uparrow" => (MathAtomKind::Rel, Face::Symbol, "\u{00DD}", Some(0xDD)),
        "Downarrow" => (MathAtomKind::Rel, Face::Symbol, "\u{00DF}", Some(0xDF)),
        "forall" => (MathAtomKind::Ord, Face::Symbol, "\u{0022}", Some(0x22)),
        "exists" => (MathAtomKind::Ord, Face::Symbol, "\u{0024}", Some(0x24)),
        "neg" | "lnot" => (MathAtomKind::Ord, Face::Symbol, "\u{00D8}", Some(0xD8)),
        "angle" => (MathAtomKind::Ord, Face::Symbol, "\u{00D0}", Some(0xD0)),
        "langle" => (MathAtomKind::Open, Face::Symbol, "\u{00E1}", Some(0xE1)),
        "rangle" => (MathAtomKind::Close, Face::Symbol, "\u{00F1}", Some(0xF1)),
        "lfloor" => (MathAtomKind::Open, Face::Symbol, "\u{00EB}", Some(0xEB)),
        "rfloor" => (MathAtomKind::Close, Face::Symbol, "\u{00FB}", Some(0xFB)),
        "lceil" => (MathAtomKind::Open, Face::Symbol, "\u{00E9}", Some(0xE9)),
        "rceil" => (MathAtomKind::Close, Face::Symbol, "\u{00F9}", Some(0xF9)),
        "vert" => (MathAtomKind::Ord, Face::Roman, "|", Some(b'|')),
        "Vert" => (MathAtomKind::Ord, Face::Roman, "||", None),
        "aleph" => (MathAtomKind::Ord, Face::Symbol, "\u{00C0}", Some(0xC0)),
        "Re" => (MathAtomKind::Ord, Face::Symbol, "\u{00C2}", Some(0xC2)),
        "Im" => (MathAtomKind::Ord, Face::Symbol, "\u{00C1}", Some(0xC1)),
        "wp" => (MathAtomKind::Ord, Face::Symbol, "\u{00C3}", Some(0xC3)),
        "emptyset" => (MathAtomKind::Ord, Face::Symbol, "\u{00C6}", Some(0xC6)),
        "surd" => (MathAtomKind::Ord, Face::Symbol, "\u{00D6}", Some(0xD6)),
        "dots" | "ldots" | "cdots" => (MathAtomKind::Ord, Face::Roman, "...", None),

        _ => return None,
    };

    Some(MathSymbolDef {
        kind: def.0,
        face: def.1,
        glyph: def.2,
        code: def.3,
    })
}

/// Measures the rendered typographic width in points for a given math character and face.
#[inline(always)]
pub fn measure_char(ch: char, face: Face, size: f32, family: metrics::FontFamily) -> f32 {
    let scale = size * 0.001;
    if face == Face::Symbol {
        if let Some(code) = metrics::unicode_to_symbol_byte(ch) {
            let units = metrics::SYMBOL_WIDTHS[code as usize];
            return (units as f32) * scale;
        }
    }
    let units = metrics::char_units_family(ch, face, family);
    (units as f32) * scale
}

/// Measures the rendered typographic width in points for an Adobe Symbol font byte code.
#[inline(always)]
pub fn measure_symbol_byte(code: u8, size: f32) -> f32 {
    let units = metrics::SYMBOL_WIDTHS[code as usize];
    (units as f32) * (size * 0.001)
}

/// TeX Appendix G mathematical layout font dimensions.
///
/// Hardware cache-line aligned (64 bytes) to guarantee 100% L1 cache locality
/// and prevent cache line boundary splits during mathematical layout passes.
#[repr(C, align(64))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MathFontParameters {
    /// Sigma_22: Axis height above baseline (e.g. 250 units)
    pub axis_height: u16,
    /// Xi_8: Default rule thickness (e.g. 56 units)
    pub rule_thickness: u16,
    /// Sigma_8: Numerator shift in display styles (num1)
    pub num1: u16,
    /// Sigma_9: Numerator shift in text styles (num2)
    pub num2: u16,
    /// Sigma_10: Numerator shift for atop in non-display (num3)
    pub num3: u16,
    /// Sigma_11: Denominator shift in display styles (denom1)
    pub denom1: u16,
    /// Sigma_12: Denominator shift in text styles (denom2)
    pub denom2: u16,
    /// Sigma_13: Superscript shift in display style (sup1)
    pub sup1: u16,
    /// Sigma_14: Superscript shift in text/script styles (sup2)
    pub sup2: u16,
    /// Sigma_15: Superscript shift in cramped styles (sup3)
    pub sup3: u16,
    /// Sigma_16: Subscript shift when no superscript (sub1)
    pub sub1: u16,
    /// Sigma_17: Subscript shift with superscript (sub2)
    pub sub2: u16,
    /// Sigma_18: Superscript drop (sup_drop)
    pub sup_drop: u16,
    /// Sigma_19: Subscript drop (sub_drop)
    pub sub_drop: u16,
    /// Sigma_20: Display minimum delimiter extent (delim1)
    pub delim1: u16,
    /// Sigma_21: Text minimum delimiter extent (delim2)
    pub delim2: u16,
    /// Xi_9: Minimum clearance above display operator (big_op_spacing1)
    pub big_op_spacing1: u16,
    /// Xi_10: Minimum clearance below display operator (big_op_spacing2)
    pub big_op_spacing2: u16,
    /// Xi_11: Minimum upper limit baseline distance (big_op_spacing3)
    pub big_op_spacing3: u16,
    /// Xi_12: Minimum lower limit baseline distance (big_op_spacing4)
    pub big_op_spacing4: u16,
    /// Xi_13: Padding above upper limit / below lower limit (big_op_spacing5)
    pub big_op_spacing5: u16,
    /// Radical clearance in display style (Xi_8 + 1/4 axis)
    pub rad_clearance_display: u16,
    /// Radical clearance in text style
    pub rad_clearance_text: u16,
    /// Fraction clearance in display style (3 * Xi_8)
    pub frac_clearance_display: u16,
    /// Fraction clearance in text style (1 * Xi_8)
    pub frac_clearance_text: u16,
    /// Rule 18 minimum gap between superscript bottom and subscript top (4 * Xi_8)
    pub sub_sup_gap: u16,
    /// Reference accent base height (x-height)
    pub accent_base_height: u16,
    /// Standard baseline-to-baseline matrix row spacing
    pub matrix_row_spacing: u16,
    /// Standard inter-column separation for matrices
    pub matrix_col_spacing: u16,
    /// 1 em in font design units
    pub quad: u16,
    /// Cache line padding to strictly guarantee 64 bytes total struct size
    pub _reserved: [u8; 4],
}

const _: () = assert!(core::mem::size_of::<MathFontParameters>() == 64);
const _: () = assert!(core::mem::align_of::<MathFontParameters>() == 64);

pub static TIMES_MATH_PARAMS: MathFontParameters = MathFontParameters {
    axis_height: 250,
    rule_thickness: 56,
    num1: 680,
    num2: 390,
    num3: 440,
    denom1: 680,
    denom2: 350,
    sup1: 420,
    sup2: 360,
    sup3: 290,
    sub1: 150,
    sub2: 247,
    sup_drop: 386,
    sub_drop: 50,
    delim1: 2390,
    delim2: 1000,
    big_op_spacing1: 111,
    big_op_spacing2: 166,
    big_op_spacing3: 200,
    big_op_spacing4: 600,
    big_op_spacing5: 100,
    rad_clearance_display: 170,
    rad_clearance_text: 70,
    frac_clearance_display: 168,
    frac_clearance_text: 56,
    sub_sup_gap: 224,
    accent_base_height: 450,
    matrix_row_spacing: 1200,
    matrix_col_spacing: 1000,
    quad: 1000,
    _reserved: [0; 4],
};

pub static CM_MATH_PARAMS: MathFontParameters = MathFontParameters {
    axis_height: 250,
    rule_thickness: 40,
    num1: 677,
    num2: 394,
    num3: 444,
    denom1: 686,
    denom2: 345,
    sup1: 413,
    sup2: 363,
    sup3: 289,
    sub1: 150,
    sub2: 247,
    sup_drop: 386,
    sub_drop: 50,
    delim1: 2390,
    delim2: 1010,
    big_op_spacing1: 111,
    big_op_spacing2: 167,
    big_op_spacing3: 200,
    big_op_spacing4: 600,
    big_op_spacing5: 100,
    rad_clearance_display: 148,
    rad_clearance_text: 50,
    frac_clearance_display: 120,
    frac_clearance_text: 40,
    sub_sup_gap: 160,
    accent_base_height: 431,
    matrix_row_spacing: 1200,
    matrix_col_spacing: 1000,
    quad: 1000,
    _reserved: [0; 4],
};

pub static HELVETICA_MATH_PARAMS: MathFontParameters = MathFontParameters {
    axis_height: 260,
    rule_thickness: 56,
    num1: 680,
    num2: 400,
    num3: 450,
    denom1: 680,
    denom2: 350,
    sup1: 420,
    sup2: 360,
    sup3: 290,
    sub1: 150,
    sub2: 250,
    sup_drop: 386,
    sub_drop: 50,
    delim1: 2390,
    delim2: 1000,
    big_op_spacing1: 111,
    big_op_spacing2: 166,
    big_op_spacing3: 200,
    big_op_spacing4: 600,
    big_op_spacing5: 100,
    rad_clearance_display: 170,
    rad_clearance_text: 70,
    frac_clearance_display: 168,
    frac_clearance_text: 56,
    sub_sup_gap: 224,
    accent_base_height: 523,
    matrix_row_spacing: 1200,
    matrix_col_spacing: 1000,
    quad: 1000,
    _reserved: [0; 4],
};

pub static PALATINO_MATH_PARAMS: MathFontParameters = MathFontParameters {
    axis_height: 250,
    rule_thickness: 52,
    num1: 680,
    num2: 400,
    num3: 450,
    denom1: 680,
    denom2: 350,
    sup1: 420,
    sup2: 360,
    sup3: 290,
    sub1: 150,
    sub2: 250,
    sup_drop: 386,
    sub_drop: 50,
    delim1: 2390,
    delim2: 1000,
    big_op_spacing1: 111,
    big_op_spacing2: 166,
    big_op_spacing3: 200,
    big_op_spacing4: 600,
    big_op_spacing5: 100,
    rad_clearance_display: 170,
    rad_clearance_text: 70,
    frac_clearance_display: 156,
    frac_clearance_text: 52,
    sub_sup_gap: 208,
    accent_base_height: 469,
    matrix_row_spacing: 1200,
    matrix_col_spacing: 1000,
    quad: 1000,
    _reserved: [0; 4],
};

/// Resolves the 64-byte aligned TeX Appendix G mathematical parameters for a given font family in O(1) time.
#[inline(always)]
pub fn math_font_parameters(family: metrics::FontFamily) -> &'static MathFontParameters {
    match family {
        metrics::FontFamily::Times => &TIMES_MATH_PARAMS,
        metrics::FontFamily::ComputerModern => &CM_MATH_PARAMS,
        metrics::FontFamily::Helvetica => &HELVETICA_MATH_PARAMS,
        metrics::FontFamily::Palatino => &PALATINO_MATH_PARAMS,
    }
}

/// Returns the italic correction in 1000-units-per-em for a character and face.
#[inline(always)]
pub fn italic_correction(ch: char, face: Face, _family: metrics::FontFamily) -> u16 {
    if face != Face::Italic && face != Face::BoldItalic {
        return 0;
    }
    KERN_PAIR_LUT.italic_correction(ch)
}

/// Returns the italic correction in 1000-units-per-em for Adobe Symbol font glyphs.
#[inline(always)]
pub fn symbol_italic_correction(code: u8) -> u16 {
    match code {
        0xF2 => 100, // Integral has forward overhang at top
        _ => 0,
    }
}

/// Computes accurate typographic ink height and depth in 1000-units-per-em.
#[inline(always)]
pub fn char_height_depth(ch: char, face: Face, _family: metrics::FontFamily) -> (u16, u16) {
    if ch.is_ascii_digit() {
        return (675, 0);
    }
    if face == Face::Italic || face == Face::BoldItalic {
        match ch {
            'b' | 'd' | 'h' | 'k' | 'l' => (694, 0),
            'f' => (700, 205),
            't' => (611, 0),
            'g' | 'q' | 'y' => (450, 218),
            'p' => (450, 205),
            'j' => (680, 218),
            'a'..='z' => (450, 0),
            'A'..='P' | 'R'..='Z' => (676, 0),
            'Q' => (676, 120),
            '+' | '-' | '=' | '<' | '>' => (500, 0),
            _ => (600, 100),
        }
    } else {
        match ch {
            'b' | 'd' | 'f' | 'h' | 'k' | 'l' => (694, 0),
            't' => (611, 0),
            'g' | 'p' | 'q' | 'y' => (450, 218),
            'j' => (650, 218),
            'a'..='z' => (450, 0),
            'A'..='P' | 'R'..='Z' => (676, 0),
            'Q' => (676, 120),
            '+' | '-' | '=' | '<' | '>' => (500, 0),
            _ => (600, 100),
        }
    }
}

/// Computes typographic height and depth in 1000-units-per-em for Adobe Symbol font glyphs.
#[inline(always)]
pub fn symbol_height_depth(code: u8) -> (u16, u16) {
    match code {
        0xF2 => (850, 350),        // Integral
        0xE5 | 0xD5 => (750, 250), // Sum, Prod
        0xD6 => (913, 35),         // Radical
        0xB6 => (690, 200),        // partial
        0xD1 => (680, 0),          // nabla
        0xA5 => (500, 0),          // infty
        0xE1 | 0xF1 => (750, 250), // angle brackets \langle, \rangle
        0xEB | 0xFB => (690, 218), // floor brackets \lfloor, \rfloor
        0xE9 | 0xF9 => (690, 218), // ceiling brackets \lceil, \rceil
        // Binary operators
        0xB1 | 0xB4 | 0xB8 | 0xD7 | 0xC4 | 0xC5 | 0xDA | 0xD9 | 0x2A => (500, 50),
        // Relations
        0xA3 | 0xB3 | 0xB9 | 0xBB | 0xBA | 0xCE | 0xCF | 0xCC | 0xC9 | 0xCD | 0xCA => (540, 40),
        // Horizontal arrows
        0xAE | 0xAC | 0xDE | 0xDC | 0xDB => (380, 120),
        // Vertical arrows
        0xAD | 0xAF | 0xDD | 0xDF => (680, 200),
        // Lowercase Greek
        b'a' | b'c' | b'e' | b'i' | b'k' | b'm' | b'n' | b'o' | b'p' | b's' | b't' | b'u'
        | b'v' | b'w' => (450, 0),
        b'b' | b'g' | b'h' | b'q' | b'r' | b'f' | b'y' => (450, 218),
        b'd' | b'l' | b'x' => (690, 0),
        b'z' => (690, 218),
        // Uppercase Greek
        b'A'..=b'Z' => (680, 0),
        _ => (500, 100),
    }
}

/// AFM-derived radical glyph metrics for standard radical constructions.
/// Cache-aligned to 16 bytes (1/4 cache line).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadicalMetrics {
    /// Overbar junction y-offset in design units (857)
    pub junction_y: u16,
    /// Vertex bottom depth in design units below baseline (35)
    pub vertex_depth: u16,
    /// Vertical span from bottom vertex to overbar junction (892)
    pub vspan: u16,
    /// Horizontal rule start offset from glyph origin (480)
    pub rule_start_x: u16,
    /// Horizontal content start offset (540)
    pub content_start_x: u16,
    /// Overbar thickness in design units (56)
    pub overbar_thickness: u16,
    pub _reserved: [u8; 4],
}

const _: () = assert!(core::mem::size_of::<RadicalMetrics>() == 16);

/// Delimiter geometric scaling parameters.
/// Cache-aligned to 16 bytes (1/4 cache line).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DelimiterMetrics {
    /// Minimum curve scale in 1000ths (1150)
    pub min_scale: u16,
    /// Maximum curve scale in 1000ths (3800)
    pub max_scale: u16,
    /// Height denominator factor in 1000ths (1020)
    pub height_factor: u16,
    /// Vertical center offset in 1000ths of font size (250)
    pub center_offset_y: u16,
    /// Bracket cap width in 1000ths of font size (280)
    pub bracket_cap_w: u16,
    /// Extra bar height extension in 1000ths of font size (150)
    pub bar_extra_h: u16,
    /// Rule thickness scaling multiplier in 1000ths (1150)
    pub thickness_scale: u16,
    pub _reserved: [u8; 2],
}

const _: () = assert!(core::mem::size_of::<DelimiterMetrics>() == 16);

/// Big operator sizing and limit placement metrics.
/// Cache-aligned to 16 bytes (1/4 cache line).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BigOpMetrics {
    /// Display size multiplier in 1000ths (1300 for sum, 1350 for int)
    pub display_scale: u16,
    /// Nominal glyph height in 1000ths of font size (750)
    pub text_height: u16,
    /// Nominal glyph depth in 1000ths of font size (250)
    pub text_depth: u16,
    /// Superscript / upper limit horizontal staggering offset (+120 for int)
    pub sup_kern: i16,
    /// Subscript / lower limit horizontal staggering offset (-220 for int)
    pub sub_kern: i16,
    /// Minimum vertical elevation for superscript in 1000ths of size
    pub sup_min_shift: u16,
    /// Minimum vertical depression for subscript in 1000ths of size
    pub sub_min_shift: u16,
    pub _reserved: [u8; 2],
}

const _: () = assert!(core::mem::size_of::<BigOpMetrics>() == 16);

/// Typographic accent placement metrics.
/// Cache-aligned to 32 bytes (1/2 cache line).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AccentMetrics {
    /// Accent glyph character string
    pub glyph: &'static str,
    /// Font face for accent
    pub face: Face,
    /// Whether drawn as a geometric rule or glyph
    pub is_rule: bool,
    /// Clearance lift above nucleus in 1000ths of font size (20, 50, 80)
    pub lift: u16,
    /// Additional bounding box height contribution in 1000ths of font size (200, 250)
    pub extra_height: u16,
    /// Glyph font size scale factor in 1000ths (650, 750)
    pub scale: u16,
    /// Width ratio relative to nucleus or font size in 1000ths
    pub width_ratio: u16,
    pub _reserved: [u8; 6],
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<AccentMetrics>() == 32);

#[cfg(target_pointer_width = "32")]
const _: () = assert!(core::mem::size_of::<AccentMetrics>() == 24);

/// Content-sensitive matrix grid metrics.
/// Cache-aligned to 16 bytes (1/4 cache line).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatrixMetrics {
    /// Minimum row height in 1000ths of font size (700)
    pub min_row_height: u16,
    /// Minimum row depth in 1000ths of font size (250)
    pub min_row_depth: u16,
    /// Inter-row separation in 1000ths of font size (350)
    pub row_sep: u16,
    /// Delimiter target height margin in 1000ths of font size (200)
    pub extra_target_h: u16,
    /// Lateral delimiter separation for vertical bars in 1000ths (300)
    pub vbar_sep: u16,
    /// Lateral delimiter separation for brackets in 1000ths (240)
    pub bracket_sep: u16,
    /// Lateral delimiter separation for parentheses/braces in 1000ths (180)
    pub default_sep: u16,
    pub _reserved: [u8; 2],
}

const _: () = assert!(core::mem::size_of::<MatrixMetrics>() == 16);

/// Planck constant (hbar) bar placement metrics.
/// Cache-aligned to 16 bytes (1/4 cache line).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HBarMetrics {
    pub bar_w: u16,
    pub bar_x: u16,
    pub bar_y: u16,
    pub _reserved: [u8; 10],
}

const _: () = assert!(core::mem::size_of::<HBarMetrics>() == 16);

pub static STANDARD_RADICAL_METRICS: RadicalMetrics = RadicalMetrics {
    junction_y: 857,
    vertex_depth: 35,
    vspan: 892,
    rule_start_x: 480,
    content_start_x: 540,
    overbar_thickness: 56,
    _reserved: [0; 4],
};

pub static STANDARD_DELIMITER_METRICS: DelimiterMetrics = DelimiterMetrics {
    min_scale: 1150,
    max_scale: 3800,
    height_factor: 1020,
    center_offset_y: 250,
    bracket_cap_w: 280,
    bar_extra_h: 150,
    thickness_scale: 1150,
    _reserved: [0; 2],
};

pub static STANDARD_MATRIX_METRICS: MatrixMetrics = MatrixMetrics {
    min_row_height: 700,
    min_row_depth: 250,
    row_sep: 350,
    extra_target_h: 200,
    vbar_sep: 300,
    bracket_sep: 240,
    default_sep: 180,
    _reserved: [0; 2],
};

pub static STANDARD_HBAR_METRICS: HBarMetrics = HBarMetrics {
    bar_w: 350,
    bar_x: 50,
    bar_y: 520,
    _reserved: [0; 10],
};

#[inline(always)]
pub fn radical_metrics(_family: metrics::FontFamily) -> &'static RadicalMetrics {
    &STANDARD_RADICAL_METRICS
}

#[inline(always)]
pub fn delimiter_metrics(_family: metrics::FontFamily) -> &'static DelimiterMetrics {
    &STANDARD_DELIMITER_METRICS
}

#[inline(always)]
pub fn matrix_metrics(_family: metrics::FontFamily) -> &'static MatrixMetrics {
    &STANDARD_MATRIX_METRICS
}

#[inline(always)]
pub fn hbar_metrics(_family: metrics::FontFamily) -> &'static HBarMetrics {
    &STANDARD_HBAR_METRICS
}

/// Returns big operator sizing and limit placement metrics.
#[inline(always)]
pub fn big_op_metrics(op: &str, _family: metrics::FontFamily) -> BigOpMetrics {
    let is_text_op = matches!(
        op,
        "lim" | "max" | "min" | "sup" | "inf" | "gcd" | "det" | "liminf" | "limsup" | "Pr"
    );
    let is_integral = matches!(op, "int" | "iint" | "iiint" | "oint");

    if is_text_op {
        BigOpMetrics {
            display_scale: 1000,
            text_height: 700,
            text_depth: 0,
            sup_kern: 0,
            sub_kern: 0,
            sup_min_shift: 420,
            sub_min_shift: 150,
            _reserved: [0; 2],
        }
    } else if is_integral {
        BigOpMetrics {
            display_scale: 1350,
            text_height: 850,
            text_depth: 350,
            sup_kern: 120,
            sub_kern: -220,
            sup_min_shift: 700,
            sub_min_shift: 500,
            _reserved: [0; 2],
        }
    } else {
        BigOpMetrics {
            display_scale: 1300,
            text_height: 750,
            text_depth: 250,
            sup_kern: 0,
            sub_kern: 0,
            sup_min_shift: 420,
            sub_min_shift: 150,
            _reserved: [0; 2],
        }
    }
}

/// Resolves authoritative typographic accent placement metrics.
#[inline(always)]
pub fn lookup_accent_metrics(accent: &str, _family: metrics::FontFamily) -> Option<AccentMetrics> {
    let metrics = match accent {
        "vec" => AccentMetrics {
            glyph: "\u{00AE}",
            face: Face::Symbol,
            is_rule: false,
            lift: 50,
            extra_height: 250,
            scale: 650,
            width_ratio: 550,
            _reserved: [0; 6],
        },
        "hat" | "widehat" => AccentMetrics {
            glyph: "^",
            face: Face::Roman,
            is_rule: false,
            lift: 20,
            extra_height: 250,
            scale: 750,
            width_ratio: 400,
            _reserved: [0; 6],
        },
        "bar" => AccentMetrics {
            glyph: "",
            face: Face::Roman,
            is_rule: true,
            lift: 80,
            extra_height: 200,
            scale: 1000,
            width_ratio: 850,
            _reserved: [0; 6],
        },
        "overline" => AccentMetrics {
            glyph: "",
            face: Face::Roman,
            is_rule: true,
            lift: 120,
            extra_height: 120,
            scale: 1000,
            width_ratio: 1000,
            _reserved: [0; 6],
        },
        "underline" => AccentMetrics {
            glyph: "",
            face: Face::Roman,
            is_rule: true,
            lift: 120,
            extra_height: 120,
            scale: 1000,
            width_ratio: 1000,
            _reserved: [0; 6],
        },
        "tilde" | "widetilde" => AccentMetrics {
            glyph: "~",
            face: Face::Roman,
            is_rule: false,
            lift: 20,
            extra_height: 250,
            scale: 750,
            width_ratio: 420,
            _reserved: [0; 6],
        },
        "check" => AccentMetrics {
            glyph: "v",
            face: Face::Roman,
            is_rule: false,
            lift: 20,
            extra_height: 250,
            scale: 650,
            width_ratio: 350,
            _reserved: [0; 6],
        },
        "breve" => AccentMetrics {
            glyph: "u",
            face: Face::Roman,
            is_rule: false,
            lift: 20,
            extra_height: 250,
            scale: 650,
            width_ratio: 350,
            _reserved: [0; 6],
        },
        "acute" => AccentMetrics {
            glyph: "\'",
            face: Face::Roman,
            is_rule: false,
            lift: 20,
            extra_height: 250,
            scale: 750,
            width_ratio: 300,
            _reserved: [0; 6],
        },
        "grave" => AccentMetrics {
            glyph: "`",
            face: Face::Roman,
            is_rule: false,
            lift: 20,
            extra_height: 250,
            scale: 750,
            width_ratio: 300,
            _reserved: [0; 6],
        },
        "dot" => AccentMetrics {
            glyph: "",
            face: Face::Roman,
            is_rule: true,
            lift: 80,
            extra_height: 200,
            scale: 1500,
            width_ratio: 0,
            _reserved: [0; 6],
        },
        "ddot" => AccentMetrics {
            glyph: "",
            face: Face::Roman,
            is_rule: true,
            lift: 80,
            extra_height: 200,
            scale: 1500,
            width_ratio: 0,
            _reserved: [0; 6],
        },
        "dddot" => AccentMetrics {
            glyph: "",
            face: Face::Roman,
            is_rule: true,
            lift: 80,
            extra_height: 200,
            scale: 1500,
            width_ratio: 0,
            _reserved: [0; 6],
        },
        "ddddot" => AccentMetrics {
            glyph: "",
            face: Face::Roman,
            is_rule: true,
            lift: 80,
            extra_height: 200,
            scale: 1500,
            width_ratio: 0,
            _reserved: [0; 6],
        },
        "mathring" => AccentMetrics {
            glyph: "\u{00B0}",
            face: Face::Symbol,
            is_rule: false,
            lift: 30,
            extra_height: 250,
            scale: 650,
            width_ratio: 300,
            _reserved: [0; 6],
        },
        "overrightarrow" => AccentMetrics {
            glyph: "\u{00AE}",
            face: Face::Symbol,
            is_rule: false,
            lift: 70,
            extra_height: 220,
            scale: 650,
            width_ratio: 1000,
            _reserved: [0; 6],
        },
        "overleftarrow" => AccentMetrics {
            glyph: "\u{00AC}",
            face: Face::Symbol,
            is_rule: false,
            lift: 70,
            extra_height: 220,
            scale: 650,
            width_ratio: 1000,
            _reserved: [0; 6],
        },
        _ => return None,
    };
    Some(metrics)
}

/// Returns optical horizontal top accent attachment offset in 1000-units-per-em.
#[inline(always)]
pub fn glyph_top_accent_offset(ch: char, face: Face, family: metrics::FontFamily) -> u16 {
    if face != Face::Italic && face != Face::BoldItalic {
        return 0;
    }
    italic_correction(ch, face, family) / 2
}
