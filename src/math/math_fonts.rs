//! Symbol metric tables and command-to-glyph mappings for mathematics.
//!
//! Maps standard LaTeX math commands directly to Adobe Type 1 Symbol font byte codes
//! and font faces, with zero dynamic heap allocations.

use crate::backend::backend_fonts::{self as metrics, Face};
use crate::math::math_ast::MathStyle;
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
        "dots" | "ldots" | "cdots" | "dotsb" | "dotsm" | "dotsi" | "dotso" | "dotsc"
        | "mathellipsis" => (MathAtomKind::Ord, Face::Roman, "...", None),
        "vdots" => (MathAtomKind::Ord, Face::Roman, ":", None),
        "ddots" => (MathAtomKind::Ord, Face::Roman, "...", None),

        _ => return None,
    };

    Some(MathSymbolDef {
        kind: def.0,
        face: def.1,
        glyph: def.2,
        code: def.3,
    })
}

/// 2-byte entry in the direct L1 dispatch table.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CmSymbolEntry {
    pub face: Face,
    pub glyph_code: u8,
}

const _: () = assert!(core::mem::size_of::<CmSymbolEntry>() == 2);

/// 64-byte hardware cache-line aligned 256-entry direct symbol dispatch table (512 bytes = 8 cache lines).
#[repr(C, align(64))]
#[derive(Clone, Copy, Debug)]
pub struct CmSymbolDispatch(pub [CmSymbolEntry; 256]);

const _: () = assert!(core::mem::size_of::<CmSymbolDispatch>() == 512);
const _: () = assert!(core::mem::align_of::<CmSymbolDispatch>() == 64);

const fn build_cm_symbol_dispatch() -> CmSymbolDispatch {
    let mut table = [CmSymbolEntry {
        face: Face::Roman,
        glyph_code: 0xFF,
    }; 256];

    // Greek Lowercase (CMMI10 - MathItalic)
    table[b'a' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x0B };
    table[b'b' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x0C };
    table[b'g' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x0D };
    table[b'd' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x0E };
    table[b'e' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x0F };
    table[b'z' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x10 };
    table[b'h' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x11 };
    table[b'q' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x12 };
    table[b'i' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x13 };
    table[b'k' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x14 };
    table[b'l' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x15 };
    table[b'm' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x16 };
    table[b'n' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x17 };
    table[b'x' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x18 };
    table[b'p' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x19 };
    table[b'r' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x1A };
    table[b's' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x1B };
    table[b't' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x1C };
    table[b'u' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x1D };
    table[b'f' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x1E };
    table[b'c' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x1F };
    table[b'y' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x20 };
    table[b'w' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x21 };

    // Greek Uppercase (CMR10 - Roman)
    table[b'G' as usize] = CmSymbolEntry { face: Face::Roman, glyph_code: 0x00 };
    table[b'D' as usize] = CmSymbolEntry { face: Face::Roman, glyph_code: 0x01 };
    table[b'Q' as usize] = CmSymbolEntry { face: Face::Roman, glyph_code: 0x02 };
    table[b'L' as usize] = CmSymbolEntry { face: Face::Roman, glyph_code: 0x03 };
    table[b'X' as usize] = CmSymbolEntry { face: Face::Roman, glyph_code: 0x04 };
    table[b'P' as usize] = CmSymbolEntry { face: Face::Roman, glyph_code: 0x05 };
    table[b'S' as usize] = CmSymbolEntry { face: Face::Roman, glyph_code: 0x06 };
    table[b'U' as usize] = CmSymbolEntry { face: Face::Roman, glyph_code: 0x07 };
    table[b'F' as usize] = CmSymbolEntry { face: Face::Roman, glyph_code: 0x08 };
    table[b'Y' as usize] = CmSymbolEntry { face: Face::Roman, glyph_code: 0x09 };
    table[b'W' as usize] = CmSymbolEntry { face: Face::Roman, glyph_code: 0x0A };

    // CMSY10 Symbols
    table[0x2D] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x00 }; // minus
    table[0xA5] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x31 }; // infty
    table[0xB1] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x06 }; // pm
    table[0xB4] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x02 }; // times
    table[0xB8] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x04 }; // div
    table[0xD7] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x01 }; // cdot
    table[0xC7] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x5C }; // cap
    table[0xC8] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x5B }; // cup
    table[0xC4] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x08 }; // oplus
    table[0xC5] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x0A }; // otimes
    table[0x2A] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x03 }; // ast
    table[0xDA] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x5F }; // vee
    table[0xD9] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x5E }; // wedge
    table[0xB0] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x0E }; // circ
    table[0xA3] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x14 }; // leq
    table[0xB3] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x15 }; // geq
    table[0xCE] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x32 }; // in
    table[0xCC] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x1A }; // subset
    table[0xCD] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x1B }; // supset
    table[0xD6] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x70 }; // radical
    table[0xD1] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x72 }; // nabla
    table[0xAE] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x21 }; // rightarrow
    table[0xAC] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x20 }; // leftarrow
    table[0xAB] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x24 }; // leftrightarrow
    table[0xDE] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x29 }; // Rightarrow
    table[0xDC] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x28 }; // Leftarrow
    table[0xDB] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x2C }; // Leftrightarrow
    table[0xAD] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x22 }; // uparrow
    table[0xAF] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x23 }; // downarrow
    table[0xDD] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x2A }; // Uparrow
    table[0xDF] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x2B }; // Downarrow
    table[0x22] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x38 }; // forall
    table[0x24] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x39 }; // exists
    table[0xD8] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x3A }; // neg / lnot
    table[0xC6] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x3B }; // emptyset
    table[0xC2] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x3C }; // Re
    table[0xC1] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x3D }; // Im
    table[0xC0] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x40 }; // aleph
    table[0x5E] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x3F }; // perp / bot
    table[0xBB] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x19 }; // approx
    table[0xBA] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x11 }; // equiv
    table[0x7E] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x18 }; // sim
    table[0x40] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x27 }; // simeq
    table[0xB5] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x2F }; // propto
    table[0xD0] = CmSymbolEntry { face: Face::Symbol, glyph_code: 0x27 }; // angle

    // CMMI10 (MathItalic)
    table[0xB6] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x40 }; // partial
    table[b'<' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x3C }; // <
    table[b'>' as usize] = CmSymbolEntry { face: Face::MathItalic, glyph_code: 0x3E }; // >

    CmSymbolDispatch(table)
}

pub static CM_SYMBOL_DISPATCH: CmSymbolDispatch = build_cm_symbol_dispatch();

pub static ASCII_GLYPH_STRS: [&str; 128] = [
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

/// Resolves an Adobe Symbol byte code into authentic Computer Modern Face and character string
/// via a single O(1) branchless L1 cache lookup.
#[inline(always)]
pub fn resolve_cm_symbol(code: u8) -> Option<(Face, &'static str, u8)> {
    let entry = CM_SYMBOL_DISPATCH.0[code as usize];
    if entry.glyph_code == 0xFF {
        None
    } else {
        Some((entry.face, ASCII_GLYPH_STRS[entry.glyph_code as usize], entry.glyph_code))
    }
}

/// Measures the rendered typographic width in points for a given math character and face.
#[inline(always)]
pub fn measure_char(ch: char, face: Face, size: f32, family: metrics::FontFamily) -> f32 {
    let scale = size * 0.001;
    if family == metrics::FontFamily::ComputerModern {
        let code = ch as u32;
        if code < 128 {
            let c = code as u8;
            match face {
                Face::MathItalic => return (cmmi10_metric(c).w as f32) * scale,
                Face::MathItalic7 => return (cmmi7_metric(c).w as f32) * scale,
                Face::MathItalic5 => return (cmmi5_metric(c).w as f32) * scale,
                Face::Italic => return (cmti10_metric(c).w as f32) * scale,
                Face::Bold => return (cmbx10_metric(c).w as f32) * scale,
                Face::Roman => return (cmr10_metric(c).w as f32) * scale,
                Face::Roman7 => return (cmr7_metric(c).w as f32) * scale,
                Face::Roman5 => return (cmr5_metric(c).w as f32) * scale,
                Face::AmsA => return (msam10_metric(c).w as f32) * scale,
                Face::AmsB => return (msbm10_metric(c).w as f32) * scale,
                Face::Symbol => {
                    let sc = if c == 0x2D { 0x00 } else { c };
                    return (cmsy10_metric(sc).w as f32) * scale;
                }
                Face::Symbol7 => {
                    let sc = if c == 0x2D { 0x00 } else { c };
                    return (cmsy7_metric(sc).w as f32) * scale;
                }
                Face::Symbol5 => {
                    let sc = if c == 0x2D { 0x00 } else { c };
                    return (cmsy5_metric(sc).w as f32) * scale;
                }
                _ => {}
            }
        }
        if face == Face::Symbol || face == Face::Symbol7 || face == Face::Symbol5 {
            if ch == '-' || ch == '−' {
                let w = match face {
                    Face::Symbol7 => cmsy7_metric(0x00).w,
                    Face::Symbol5 => cmsy5_metric(0x00).w,
                    _ => cmsy10_metric(0x00).w,
                };
                return (w as f32) * scale;
            }
            if let Some(byte) = metrics::unicode_to_symbol_byte(ch) {
                let code = if byte == 0x2D { 0x00 } else { byte };
                let w = match face {
                    Face::Symbol7 => cmsy7_metric(code).w,
                    Face::Symbol5 => cmsy5_metric(code).w,
                    _ => cmsy10_metric(code).w,
                };
                return (w as f32) * scale;
            }
        }
    }
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

/// Authoritative Computer Modern 7pt (CMSY7) font dimensions for Script style.
pub static CM_MATH_PARAMS_7PT: MathFontParameters = MathFontParameters {
    axis_height: 250,
    rule_thickness: 40,
    num1: 732,
    num2: 384,
    num3: 471,
    denom1: 752,
    denom2: 344,
    sup1: 503,
    sup2: 431,
    sup3: 286,
    sub1: 143,
    sub2: 286,
    sup_drop: 353,
    sub_drop: 71,
    delim1: 1700,
    delim2: 1157,
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
    quad: 1171,
    _reserved: [0; 4],
};

/// Authoritative Computer Modern 5pt (CMSY5) font dimensions for ScriptScript style.
pub static CM_MATH_PARAMS_5PT: MathFontParameters = MathFontParameters {
    axis_height: 250,
    rule_thickness: 40,
    num1: 925,
    num2: 387,
    num3: 504,
    denom1: 1025,
    denom2: 532,
    sup1: 504,
    sup2: 404,
    sup3: 294,
    sub1: 200,
    sub2: 400,
    sup_drop: 494,
    sub_drop: 100,
    delim1: 1980,
    delim2: 1420,
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
    quad: 1472,
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

/// Resolves the 64-byte aligned TeX Appendix G mathematical parameters for a given font family and style in O(1) time.
#[inline(always)]
pub fn math_font_parameters_styled(family: metrics::FontFamily, style: MathStyle) -> &'static MathFontParameters {
    if family == metrics::FontFamily::ComputerModern {
        match style {
            MathStyle::Display | MathStyle::CrampedDisplay | MathStyle::Text | MathStyle::CrampedText => &CM_MATH_PARAMS,
            MathStyle::Script | MathStyle::CrampedScript => &CM_MATH_PARAMS_7PT,
            MathStyle::ScriptScript | MathStyle::CrampedScriptScript => &CM_MATH_PARAMS_5PT,
        }
    } else {
        math_font_parameters(family)
    }
}

/// Canonical 12-bit mathcode table from plain.tex (512 bytes, resident in L1 cache).
///
/// Encodes (class << 12) | (family << 8) | char_code:
/// - class 0: Ord, 1: Op, 2: Bin, 3: Rel, 4: Open, 5: Close, 6: Punct, 7: Variable, 8: Active
/// - family 0: Roman, 1: MathItalic, 2: Symbol, 3: Extension
#[repr(C, align(64))]
pub struct MathcodeTable(pub [u16; 256]);

impl MathcodeTable {
    #[inline(always)]
    pub const fn lookup(&self, ch: char) -> Option<u16> {
        let c = ch as u32;
        if c < 256 {
            Some(self.0[c as usize])
        } else {
            None
        }
    }
}

pub static MATHCODE_TABLE: MathcodeTable = MathcodeTable([
    0x2201, 0x3223, 0x010B, 0x010C, 0x225E, 0x023A, 0x3232, 0x0119, // 0x00..0x07  ........
    0x0115, 0x010D, 0x010E, 0x3222, 0x2206, 0x2208, 0x0231, 0x0140, // 0x08..0x0F  ........
    0x321A, 0x321B, 0x225C, 0x225B, 0x0238, 0x0239, 0x220A, 0x3224, // 0x10..0x17  ........
    0x3220, 0x3221, 0x8000, 0x2205, 0x3214, 0x3215, 0x3211, 0x225F, // 0x18..0x1F  ........
    0x8000, 0x5021, 0x0022, 0x0023, 0x0024, 0x0025, 0x0026, 0x8000, // 0x20..0x27   !"#$%&'
    0x4028, 0x5029, 0x2203, 0x202B, 0x613B, 0x2200, 0x013A, 0x013D, // 0x28..0x2F  ()*+,-./
    0x7030, 0x7031, 0x7032, 0x7033, 0x7034, 0x7035, 0x7036, 0x7037, // 0x30..0x37  01234567
    0x7038, 0x7039, 0x303A, 0x603B, 0x313C, 0x303D, 0x313E, 0x503F, // 0x38..0x3F  89:;<=>?
    0x0040, 0x7141, 0x7142, 0x7143, 0x7144, 0x7145, 0x7146, 0x7147, // 0x40..0x47  @ABCDEFG
    0x7148, 0x7149, 0x714A, 0x714B, 0x714C, 0x714D, 0x714E, 0x714F, // 0x48..0x4F  HIJKLMNO
    0x7150, 0x7151, 0x7152, 0x7153, 0x7154, 0x7155, 0x7156, 0x7157, // 0x50..0x57  PQRSTUVW
    0x7158, 0x7159, 0x715A, 0x405B, 0x026E, 0x505D, 0x005E, 0x8000, // 0x58..0x5F  XYZ[\]^_
    0x0060, 0x7161, 0x7162, 0x7163, 0x7164, 0x7165, 0x7166, 0x7167, // 0x60..0x67  `abcdefg
    0x7168, 0x7169, 0x716A, 0x716B, 0x716C, 0x716D, 0x716E, 0x716F, // 0x68..0x6F  hijklmno
    0x7170, 0x7171, 0x7172, 0x7173, 0x7174, 0x7175, 0x7176, 0x7177, // 0x70..0x77  pqrstuvw
    0x7178, 0x7179, 0x717A, 0x4266, 0x026A, 0x5267, 0x007E, 0x1273, // 0x78..0x7F  xyz{|}~.
    0x0080, 0x0081, 0x0082, 0x0083, 0x0084, 0x0085, 0x0086, 0x0087, // 0x80..0x87  ........
    0x0088, 0x0089, 0x008A, 0x008B, 0x008C, 0x008D, 0x008E, 0x008F, // 0x88..0x8F  ........
    0x0090, 0x0091, 0x0092, 0x0093, 0x0094, 0x0095, 0x0096, 0x0097, // 0x90..0x97  ........
    0x0098, 0x0099, 0x009A, 0x009B, 0x009C, 0x009D, 0x009E, 0x009F, // 0x98..0x9F  ........
    0x00A0, 0x00A1, 0x00A2, 0x00A3, 0x00A4, 0x00A5, 0x00A6, 0x00A7, // 0xA0..0xA7  ........
    0x00A8, 0x00A9, 0x00AA, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x00AF, // 0xA8..0xAF  ........
    0x00B0, 0x00B1, 0x00B2, 0x00B3, 0x00B4, 0x00B5, 0x00B6, 0x00B7, // 0xB0..0xB7  ........
    0x00B8, 0x00B9, 0x00BA, 0x00BB, 0x00BC, 0x00BD, 0x00BE, 0x00BF, // 0xB8..0xBF  ........
    0x00C0, 0x00C1, 0x00C2, 0x00C3, 0x00C4, 0x00C5, 0x00C6, 0x00C7, // 0xC0..0xC7  ........
    0x00C8, 0x00C9, 0x00CA, 0x00CB, 0x00CC, 0x00CD, 0x00CE, 0x00CF, // 0xC8..0xCF  ........
    0x00D0, 0x00D1, 0x00D2, 0x00D3, 0x00D4, 0x00D5, 0x00D6, 0x00D7, // 0xD0..0xD7  ........
    0x00D8, 0x00D9, 0x00DA, 0x00DB, 0x00DC, 0x00DD, 0x00DE, 0x00DF, // 0xD8..0xDF  ........
    0x00E0, 0x00E1, 0x00E2, 0x00E3, 0x00E4, 0x00E5, 0x00E6, 0x00E7, // 0xE0..0xE7  ........
    0x00E8, 0x00E9, 0x00EA, 0x00EB, 0x00EC, 0x00ED, 0x00EE, 0x00EF, // 0xE8..0xEF  ........
    0x00F0, 0x00F1, 0x00F2, 0x00F3, 0x00F4, 0x00F5, 0x00F6, 0x00F7, // 0xF0..0xF7  ........
    0x00F8, 0x00F9, 0x00FA, 0x00FB, 0x00FC, 0x00FD, 0x00FE, 0x00FF, // 0xF8..0xFF  ........
]);

const _: () = assert!(core::mem::size_of::<MathcodeTable>() == 512);
const _: () = assert!(core::mem::align_of::<MathcodeTable>() == 64);

/// Returns the atom classification corresponding to a 12-bit mathcode.
#[inline(always)]
pub fn mathcode_atom_kind(raw: u16) -> MathAtomKind {
    match (raw >> 12) & 0x0F {
        0 => MathAtomKind::Ord,
        1 => MathAtomKind::Op,
        2 => MathAtomKind::Bin,
        3 => MathAtomKind::Rel,
        4 => MathAtomKind::Open,
        5 => MathAtomKind::Close,
        6 => MathAtomKind::Punct,
        _ => MathAtomKind::Ord, // Class 7 (Variable) is Ord; Class 8 (Active) is treated as Ord
    }
}

/// Returns the TeX math family index (0..15) corresponding to a 12-bit mathcode.
#[inline(always)]
pub fn mathcode_family(raw: u16) -> u8 {
    ((raw >> 8) & 0x0F) as u8
}

/// Returns the character byte code corresponding to a 12-bit mathcode.
#[inline(always)]
pub fn mathcode_char(raw: u16) -> u8 {
    (raw & 0xFF) as u8
}

/// Maps a base face and math style to the optical face for Computer Modern.
/// In TeX math mode:
/// - Display / Text (tier 0): 10pt (CMR10, CMMI10, CMSY10)
/// - Script (tier 1): 7pt (CMR7, CMMI7, CMSY7)
/// - ScriptScript (tier 2): 5pt (CMR5, CMMI5, CMSY5)
#[inline(always)]
pub fn resolve_optical_face(face: Face, style: MathStyle, family: metrics::FontFamily) -> Face {
    if family != metrics::FontFamily::ComputerModern {
        return face;
    }
    match style {
        MathStyle::Display | MathStyle::CrampedDisplay | MathStyle::Text | MathStyle::CrampedText => {
            face
        }
        MathStyle::Script | MathStyle::CrampedScript => match face {
            Face::Roman => Face::Roman7,
            Face::MathItalic => Face::MathItalic7,
            Face::Symbol => Face::Symbol7,
            _ => face,
        },
        MathStyle::ScriptScript | MathStyle::CrampedScriptScript => match face {
            Face::Roman => Face::Roman5,
            Face::MathItalic => Face::MathItalic5,
            Face::Symbol => Face::Symbol5,
            _ => face,
        },
    }
}

/// Returns the italic correction in 1000-units-per-em for a character and face.
#[inline(always)]
pub fn italic_correction(ch: char, face: Face, family: metrics::FontFamily) -> u16 {
    if family == metrics::FontFamily::ComputerModern {
        let code = ch as u32;
        if code < 128 {
            let c = code as u8;
            match face {
                Face::MathItalic => return cmmi10_metric(c).ic,
                Face::MathItalic7 => return cmmi7_metric(c).ic,
                Face::MathItalic5 => return cmmi5_metric(c).ic,
                Face::Italic => return cmti10_metric(c).ic,
                Face::Bold => return cmbx10_metric(c).ic,
                Face::Roman => return cmr10_metric(c).ic,
                Face::Roman7 => return cmr7_metric(c).ic,
                Face::Roman5 => return cmr5_metric(c).ic,
                Face::AmsA => return msam10_metric(c).ic,
                Face::AmsB => return msbm10_metric(c).ic,
                _ => return 0,
            }
        }
    }
    if face != Face::Italic && face != Face::BoldItalic && face != Face::MathItalic {
        return 0;
    }
    KERN_PAIR_LUT.italic_correction(ch)
}

/// Returns the italic correction in 1000-units-per-em for Adobe Symbol font glyphs.
#[inline(always)]
pub fn symbol_italic_correction(code: u8) -> u16 {
    symbol_italic_correction_family(code, metrics::FontFamily::Times)
}

/// Returns the italic correction in 1000-units-per-em for math symbols under a specific font family.
#[inline(always)]
pub fn symbol_italic_correction_family(code: u8, family: metrics::FontFamily) -> u16 {
    match code {
        0xF2 => {
            if family == metrics::FontFamily::ComputerModern {
                211 // CMEX10 integral italic correction
            } else {
                100 // Times / Symbol forward overhang
            }
        }
        _ => 0,
    }
}

/// Computes accurate typographic ink height and depth in 1000-units-per-em.
#[inline(always)]
pub fn char_height_depth(ch: char, face: Face, family: metrics::FontFamily) -> (u16, u16) {
    if family == metrics::FontFamily::ComputerModern {
        let code = ch as u32;
        if code < 128 {
            let c = code as u8;
            match face {
                Face::MathItalic => {
                    let m = cmmi10_metric(c);
                    return (m.h, m.d);
                }
                Face::MathItalic7 => {
                    let m = cmmi7_metric(c);
                    return (m.h, m.d);
                }
                Face::MathItalic5 => {
                    let m = cmmi5_metric(c);
                    return (m.h, m.d);
                }
                Face::Italic => {
                    let m = cmti10_metric(c);
                    return (m.h, m.d);
                }
                Face::Bold => {
                    let m = cmbx10_metric(c);
                    return (m.h, m.d);
                }
                Face::Roman => {
                    let m = cmr10_metric(c);
                    return (m.h, m.d);
                }
                Face::Roman7 => {
                    let m = cmr7_metric(c);
                    return (m.h, m.d);
                }
                Face::Roman5 => {
                    let m = cmr5_metric(c);
                    return (m.h, m.d);
                }
                Face::AmsA => {
                    let m = msam10_metric(c);
                    return (m.h, m.d);
                }
                Face::AmsB => {
                    let m = msbm10_metric(c);
                    return (m.h, m.d);
                }
                Face::Symbol => {
                    let sc = if c == 0x2D { 0x00 } else { c };
                    let m = cmsy10_metric(sc);
                    return (m.h, m.d);
                }
                Face::Symbol7 => {
                    let sc = if c == 0x2D { 0x00 } else { c };
                    let m = cmsy7_metric(sc);
                    return (m.h, m.d);
                }
                Face::Symbol5 => {
                    let sc = if c == 0x2D { 0x00 } else { c };
                    let m = cmsy5_metric(sc);
                    return (m.h, m.d);
                }
                _ => {}
            }
        }
        if face == Face::Symbol || face == Face::Symbol7 || face == Face::Symbol5 {
            if let Some(byte) = metrics::unicode_to_symbol_byte(ch) {
                let sc = if byte == 0x2D { 0x00 } else { byte };
                let m = match face {
                    Face::Symbol7 => cmsy7_metric(sc),
                    Face::Symbol5 => cmsy5_metric(sc),
                    _ => cmsy10_metric(sc),
                };
                return (m.h, m.d);
            }
        }
    }
    if ch.is_ascii_digit() {
        return (675, 0);
    }
    if face == Face::Italic || face == Face::BoldItalic || face == Face::MathItalic {
        match ch {
            'b' | 'd' | 'h' | 'k' | 'l' => (694, 0),
            'f' => (700, 205),
            't' => (611, 0),
            'g' | 'q' | 'y' => (450, 218),
            'p' => (450, 205),
            'j' => (680, 218),
            _ => (431, 0),
        }
    } else {
        (683, 0)
    }
}

/// Computes typographic height and depth in 1000-units-per-em for Adobe Symbol font glyphs.
#[inline(always)]
pub fn symbol_height_depth(code: u8) -> (u16, u16) {
    symbol_height_depth_family(code, metrics::FontFamily::Times)
}

/// Computes typographic height and depth in 1000-units-per-em for math symbols under a specific font family.
#[inline(always)]
pub fn symbol_height_depth_family(code: u8, family: metrics::FontFamily) -> (u16, u16) {
    if family == metrics::FontFamily::ComputerModern {
        if code == 0x2D {
            let m = cmsy10_metric(0x00);
            return (m.h, m.d);
        } else if code == 0xF2 {
            let m = cmex10_metric(0x52); // CMEX10 integral
            return (m.h, m.d);
        } else if code == 0xE5 || code == 0xD5 {
            let m = cmex10_metric(0x58); // CMEX10 sum
            return (m.h, m.d);
        }
    }
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
    if face != Face::Italic && face != Face::BoldItalic && face != Face::MathItalic {
        return 0;
    }
    italic_correction(ch, face, family) / 2
}

// =========================================================================
// Batch 3: Math Family & Font Resolution Architecture (Items 63–68)
// =========================================================================

/// Item 63: 48-entry flat array mapping 16 math families x 3 sizing tiers (Text, Script, ScriptScript).
/// Exactly 64 bytes (`align(64)`), fitting in a single L1 CPU cache line with zero heap allocation.
#[repr(C, align(64))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MathFamilyTable {
    pub fonts: [u8; 48],
    pub _padding: [u8; 16],
}

impl MathFamilyTable {
    pub const fn new() -> Self {
        Self {
            fonts: [0u8; 48],
            _padding: [0u8; 16],
        }
    }

    #[inline(always)]
    pub fn get(&self, fam: u8, size_tier: u8) -> u8 {
        let idx = (fam as usize & 0x0F) + (16 * (size_tier as usize & 0x03).min(2));
        self.fonts[idx]
    }

    #[inline(always)]
    pub fn set(&mut self, fam: u8, size_tier: u8, font_id: u8) {
        let idx = (fam as usize & 0x0F) + (16 * (size_tier as usize & 0x03).min(2));
        self.fonts[idx] = font_id;
    }
}

impl Default for MathFamilyTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Item 64: Transactional undo-log stack for restoring math family font assignments on group exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MathFamilyScopeEntry {
    pub fam: u8,
    pub size_tier: u8,
    pub old_font_id: u8,
    pub group_level: u8,
}

#[repr(C, align(64))]
#[derive(Clone, Copy, Debug)]
pub struct MathFamilyScopeStack {
    pub entries: [MathFamilyScopeEntry; 16],
    pub len: usize,
}

impl MathFamilyScopeStack {
    pub const fn new() -> Self {
        Self {
            entries: [MathFamilyScopeEntry {
                fam: 0,
                size_tier: 0,
                old_font_id: 0,
                group_level: 0,
            }; 16],
            len: 0,
        }
    }

    #[inline(always)]
    pub fn push(&mut self, fam: u8, size_tier: u8, old_font_id: u8, group_level: u8) {
        if self.len < 16 {
            self.entries[self.len] = MathFamilyScopeEntry {
                fam,
                size_tier,
                old_font_id,
                group_level,
            };
            self.len += 1;
        }
    }

    #[inline(always)]
    pub fn unwind_to_level(&mut self, level: u8, table: &mut MathFamilyTable) {
        while self.len > 0 && self.entries[self.len - 1].group_level > level {
            self.len -= 1;
            let entry = self.entries[self.len];
            table.set(entry.fam, entry.size_tier, entry.old_font_id);
        }
    }
}

impl Default for MathFamilyScopeStack {
    fn default() -> Self {
        Self::new()
    }
}

/// Item 65: Decoded 16-bit TeX \mathcode ("TFCC: Type/Class 3 bits, Family 4 bits, Char 8 bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodedMathCode {
    pub class: MathAtomKind,
    pub family: u8,
    pub char_code: u8,
}

pub struct MathCodeDecoder;

impl MathCodeDecoder {
    /// Decodes 16-bit mathcode integer into Class (0..7), Family (0..15), and Character (0..255).
    #[inline(always)]
    pub fn decode(code: u16) -> DecodedMathCode {
        let class_raw = ((code >> 12) & 0x07) as u8;
        let family = ((code >> 8) & 0x0F) as u8;
        let char_code = (code & 0xFF) as u8;
        let class = match class_raw {
            0 => MathAtomKind::Ord,
            1 => MathAtomKind::Op,
            2 => MathAtomKind::Bin,
            3 => MathAtomKind::Rel,
            4 => MathAtomKind::Open,
            5 => MathAtomKind::Close,
            6 => MathAtomKind::Punct,
            _ => MathAtomKind::Inner,
        };
        DecodedMathCode {
            class,
            family,
            char_code,
        }
    }
}

/// Item 66: Extended Unicode mathcode structure (\Umathcode: Class 3 bits, Family 8 bits, USV 21 bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtendedMathCode {
    pub class: MathAtomKind,
    pub family: u8,
    pub char_code: u32,
}

pub struct ExtendedMathCodeDecoder;

impl ExtendedMathCodeDecoder {
    #[inline(always)]
    pub fn decode(class_val: u8, family_val: u8, char_val: u32) -> ExtendedMathCode {
        let class = match class_val & 0x07 {
            0 => MathAtomKind::Ord,
            1 => MathAtomKind::Op,
            2 => MathAtomKind::Bin,
            3 => MathAtomKind::Rel,
            4 => MathAtomKind::Open,
            5 => MathAtomKind::Close,
            6 => MathAtomKind::Punct,
            _ => MathAtomKind::Inner,
        };
        ExtendedMathCode {
            class,
            family: family_val,
            char_code: char_val & 0x1FFFFF,
        }
    }
}

/// Item 67: Decoded 24-bit TeX \delcode: Small Family (4 bits), Small Char (8 bits),
/// Large Family (4 bits), Large Char (8 bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodedDelCode {
    pub small_fam: u8,
    pub small_char: u8,
    pub large_fam: u8,
    pub large_char: u8,
}

pub struct DelimiterCodeDecoder;

impl DelimiterCodeDecoder {
    #[inline(always)]
    pub fn decode(delcode: i32) -> Option<DecodedDelCode> {
        if delcode < 0 {
            return None;
        }
        let d = delcode as u32;
        let small_fam = ((d >> 20) & 0x0F) as u8;
        let small_char = ((d >> 12) & 0xFF) as u8;
        let large_fam = ((d >> 8) & 0x0F) as u8;
        let large_char = (d & 0xFF) as u8;
        Some(DecodedDelCode {
            small_fam,
            small_char,
            large_fam,
            large_char,
        })
    }
}

/// Item 68: Intercepts active math characters (mathcode 0x8000).
pub struct ActiveMathCodeInterceptor;

impl ActiveMathCodeInterceptor {
    #[inline(always)]
    pub const fn is_active(mathcode: u16) -> bool {
        mathcode == 0x8000
    }
}

// =========================================================================
// Batch 4: Dynamic Math Engine Parameters (Items 69–74)
// =========================================================================

pub const MATH_PARAM_AXIS: u8 = 0;
pub const MATH_PARAM_RULE_THICKNESS: u8 = 1;
pub const MATH_PARAM_NUM1: u8 = 2;
pub const MATH_PARAM_NUM2: u8 = 3;
pub const MATH_PARAM_NUM3: u8 = 4;
pub const MATH_PARAM_DENOM1: u8 = 5;
pub const MATH_PARAM_DENOM2: u8 = 6;
pub const MATH_PARAM_SUP1: u8 = 7;
pub const MATH_PARAM_SUP2: u8 = 8;
pub const MATH_PARAM_SUP3: u8 = 9;
pub const MATH_PARAM_SUB1: u8 = 10;
pub const MATH_PARAM_SUB2: u8 = 11;
pub const MATH_PARAM_SUP_DROP: u8 = 12;
pub const MATH_PARAM_SUB_DROP: u8 = 13;
pub const MATH_PARAM_DELIM1: u8 = 14;
pub const MATH_PARAM_DELIM2: u8 = 15;
pub const MATH_PARAM_COUNT: usize = 16;

/// Item 69: Direct table-driven storage for all engine math dimensions and spacing parameters
/// indexed by (param_id, style_id). Exactly 512 bytes aligned to 64 bytes.
#[repr(C, align(64))]
#[derive(Clone, Copy, Debug)]
pub struct MathParameterRegistry {
    pub params: [[i32; 8]; MATH_PARAM_COUNT],
}

impl MathParameterRegistry {
    pub const fn new() -> Self {
        Self {
            params: [[0; 8]; MATH_PARAM_COUNT],
        }
    }

    #[inline(always)]
    pub fn get(&self, param_id: u8, style: MathStyle) -> i32 {
        let pid = (param_id as usize).min(MATH_PARAM_COUNT - 1);
        let sid = (style as usize) & 0x07;
        self.params[pid][sid]
    }

    #[inline(always)]
    pub fn set(&mut self, param_id: u8, style: MathStyle, value: i32) {
        let pid = (param_id as usize).min(MATH_PARAM_COUNT - 1);
        let sid = (style as usize) & 0x07;
        self.params[pid][sid] = value;
    }
}

impl Default for MathParameterRegistry {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

/// Item 70: Scoped parameter restoration stack ensuring local \Umath parameter modifications unwind.
#[derive(Clone, Copy, Debug)]
pub struct MathParamScopeEntry {
    pub param_id: u8,
    pub style: MathStyle,
    pub old_value: i32,
    pub group_level: u8,
}

#[repr(C, align(64))]
#[derive(Clone, Copy, Debug)]
pub struct MathParamScopeStack {
    pub entries: [MathParamScopeEntry; 16],
    pub len: usize,
}

impl MathParamScopeStack {
    pub const fn new() -> Self {
        Self {
            entries: [MathParamScopeEntry {
                param_id: 0,
                style: MathStyle::Display,
                old_value: 0,
                group_level: 0,
            }; 16],
            len: 0,
        }
    }

    #[inline(always)]
    pub fn push(&mut self, param_id: u8, style: MathStyle, old_value: i32, group_level: u8) {
        if self.len < 16 {
            self.entries[self.len] = MathParamScopeEntry {
                param_id,
                style,
                old_value,
                group_level,
            };
            self.len += 1;
        }
    }

    #[inline(always)]
    pub fn unwind_to_level(&mut self, level: u8, registry: &mut MathParameterRegistry) {
        while self.len > 0 && self.entries[self.len - 1].group_level > level {
            self.len -= 1;
            let entry = self.entries[self.len];
            registry.set(entry.param_id, entry.style, entry.old_value);
        }
    }
}

impl Default for MathParamScopeStack {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

/// Item 71: Dynamic overrides of atom-to-atom spacing rules mapped to mu-glue specifications.
#[repr(C, align(64))]
#[derive(Clone, Copy, Debug)]
pub struct MathSpacingParameterOverride {
    pub custom_spacing: [[Option<u16>; 8]; 8],
}

impl MathSpacingParameterOverride {
    pub const fn new() -> Self {
        Self {
            custom_spacing: [[None; 8]; 8],
        }
    }

    #[inline(always)]
    pub fn get(&self, left: MathAtomKind, right: MathAtomKind) -> Option<u16> {
        self.custom_spacing[left as usize & 0x07][right as usize & 0x07]
    }

    #[inline(always)]
    pub fn set(&mut self, left: MathAtomKind, right: MathAtomKind, spacing_mu: u16) {
        self.custom_spacing[left as usize & 0x07][right as usize & 0x07] = Some(spacing_mu);
    }
}

impl Default for MathSpacingParameterOverride {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

/// Item 72: Dynamic runtime query of math axis height per font family and style tier.
pub struct MathAxisHeightParameter;

impl MathAxisHeightParameter {
    #[inline(always)]
    pub fn get(registry: &MathParameterRegistry, style: MathStyle, default_axis: f32) -> f32 {
        let val = registry.get(MATH_PARAM_AXIS, style);
        if val != 0 {
            val as f32 * 0.001
        } else {
            default_axis
        }
    }
}

/// Item 73: Configurable default fraction and radical rule thickness parameters with style-dependent scaling.
pub struct RuleThicknessParameter;

impl RuleThicknessParameter {
    #[inline(always)]
    pub fn get(registry: &MathParameterRegistry, style: MathStyle, default_thickness: f32) -> f32 {
        let val = registry.get(MATH_PARAM_RULE_THICKNESS, style);
        if val != 0 {
            val as f32 * 0.001
        } else {
            default_thickness
        }
    }
}

/// Item 74: Configurable sup_drop and sub_drop minimum script baselines for complex operator attachments.
pub struct ScriptDropParameters;

impl ScriptDropParameters {
    #[inline(always)]
    pub fn sup_drop(registry: &MathParameterRegistry, style: MathStyle, default_drop: f32) -> f32 {
        let val = registry.get(MATH_PARAM_SUP_DROP, style);
        if val != 0 {
            val as f32 * 0.001
        } else {
            default_drop
        }
    }

    #[inline(always)]
    pub fn sub_drop(registry: &MathParameterRegistry, style: MathStyle, default_drop: f32) -> f32 {
        let val = registry.get(MATH_PARAM_SUB_DROP, style);
        if val != 0 {
            val as f32 * 0.001
        } else {
            default_drop
        }
    }
}
