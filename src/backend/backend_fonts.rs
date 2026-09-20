//! # Adobe Font Metrics Engine (`metrics`)
//!
//! Provides compile-time Adobe Font Metrics (AFM) for standard PDF Type 1
//! typefaces: Times-Roman (industry standard serif) and Helvetica (sans-serif)
//! at 1000 units per em (WinAnsi encoding).
//!
//! ## Architectural Design
//!
//! - **Zero Heap Allocations**: Font width tables for ASCII 0..=127 are precomputed and stored
//!   directly in `.rodata` as flat `[u16; 128]` arrays.
//! - **Branchless Lookups**: Direct array indexing for ASCII characters avoids branch mispredictions.
//! - **Single Multiplication Measurement**: `string_units_family` accumulates integer font units in a tight
//!   byte loop, performing only one floating-point multiplication at the end.
//! - **Industry Standard Default**: Defaults to `Times-Roman` (universal publishing standard for IEEE, ACM,
//!   Nature, Science) with zero PDF file size embedding overhead.
//!
//! ## Public APIs
//!
//! - [`FontFamily`]: Enum representing typeface families (`Times`, `Helvetica`).
//! - [`Face`]: Enum representing the typographic styles (`Roman`, `Bold`, `Italic`, `BoldItalic`, `Mono`, `MonoBold`).
//! - [`char_units_family`] / [`char_units`]: Returns the raw metric width of a character in 1000-units/em space.
//! - [`width_family`] / [`width`]: Returns typographic width of a character in PDF points (`pt`).
//! - [`string_units_family`] / [`string_units`]: Computes sum of metric units for text slice.
//! - [`string_width_family`] / [`string_width`]: Calculates total rendered width in PDF points (`pt`).

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum FontFamily {
    #[default]
    Times = 0,
    Helvetica = 1,
    Palatino = 2,
    ComputerModern = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Face {
    Roman = 0,
    Bold = 1,
    Italic = 2,
    BoldItalic = 3,
    Mono = 4,
    MonoBold = 5,
    Symbol = 6,
    Dingbats = 7,
}

impl Face {
    pub const BOLD_MASK: u8 = 1;
    pub const ITALIC_MASK: u8 = 2;
    pub const MONO_MASK: u8 = 4;

    pub const ALL: [Face; 8] = [
        Face::Roman,
        Face::Bold,
        Face::Italic,
        Face::BoldItalic,
        Face::Mono,
        Face::MonoBold,
        Face::Symbol,
        Face::Dingbats,
    ];

    #[inline(always)]
    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => Face::Roman,
            1 => Face::Bold,
            2 => Face::Italic,
            3 => Face::BoldItalic,
            4 => Face::Mono,
            5 => Face::MonoBold,
            6 => Face::Symbol,
            _ => Face::Dingbats,
        }
    }

    #[inline(always)]
    pub fn pdf_name(self, family: FontFamily) -> &'static str {
        if self == Face::Symbol {
            return "Symbol";
        }
        if self == Face::Dingbats {
            return "ZapfDingbats";
        }
        match family {
            FontFamily::Times => {
                const TIMES_NAMES: [&str; 6] = [
                    "Times-Roman",
                    "Times-Bold",
                    "Times-Italic",
                    "Times-BoldItalic",
                    "Courier",
                    "Courier-Bold",
                ];
                TIMES_NAMES[self as usize]
            }
            FontFamily::Helvetica => {
                const HELVETICA_NAMES: [&str; 6] = [
                    "Helvetica",
                    "Helvetica-Bold",
                    "Helvetica-Oblique",
                    "Helvetica-BoldOblique",
                    "Courier",
                    "Courier-Bold",
                ];
                HELVETICA_NAMES[self as usize]
            }
            FontFamily::Palatino => {
                const PALATINO_NAMES: [&str; 6] = [
                    "Palatino-Roman",
                    "Palatino-Bold",
                    "Palatino-Italic",
                    "Palatino-BoldItalic",
                    "Courier",
                    "Courier-Bold",
                ];
                PALATINO_NAMES[self as usize]
            }
            FontFamily::ComputerModern => {
                const CM_NAMES: [&str; 6] =
                    ["CMR10", "CMR10", "CMMI10", "CMMI10", "CMTT10", "CMTT10"];
                CM_NAMES[self as usize]
            }
        }
    }

    #[inline(always)]
    pub fn resource(self) -> &'static str {
        const RES: [&str; 8] = ["/Fr", "/Fb", "/Fi", "/Fbi", "/Fc", "/Fcb", "/Fs", "/Fz"];
        RES[self as usize]
    }

    #[inline(always)]
    pub fn with_bold(self, enable_bold: bool) -> Self {
        if self == Face::Symbol || self == Face::Dingbats {
            return self;
        }
        let val = self as u8;
        let new_val = if enable_bold {
            val | Self::BOLD_MASK
        } else {
            val & !Self::BOLD_MASK
        };
        match new_val {
            0 => Face::Roman,
            1 => Face::Bold,
            2 => Face::Italic,
            3 => Face::BoldItalic,
            4 => Face::Mono,
            _ => Face::MonoBold,
        }
    }

    #[inline(always)]
    pub fn with_italic(self, enable_italic: bool) -> Self {
        if self.is_mono() || self == Face::Symbol || self == Face::Dingbats {
            return self;
        }
        let val = self as u8;
        let new_val = if enable_italic {
            val | Self::ITALIC_MASK
        } else {
            val & !Self::ITALIC_MASK
        };
        match new_val {
            0 => Face::Roman,
            1 => Face::Bold,
            2 => Face::Italic,
            3 => Face::BoldItalic,
            4 => Face::Mono,
            _ => Face::MonoBold,
        }
    }

    #[inline(always)]
    pub fn with_mono(self, enable_mono: bool) -> Self {
        if self == Face::Symbol || self == Face::Dingbats {
            return self;
        }
        let is_b = (self as u8) & Self::BOLD_MASK;
        if enable_mono {
            if is_b != 0 {
                Face::MonoBold
            } else {
                Face::Mono
            }
        } else if is_b != 0 {
            Face::Bold
        } else {
            Face::Roman
        }
    }

    #[inline(always)]
    pub fn is_bold(self) -> bool {
        (self as u8 & Self::BOLD_MASK) != 0
    }

    #[inline(always)]
    pub fn is_italic(self) -> bool {
        (self as u8 & Self::ITALIC_MASK) != 0
    }

    #[inline(always)]
    pub fn is_mono(self) -> bool {
        (self as u8 & Self::MONO_MASK) != 0
    }

    #[inline(always)]
    pub fn is_symbol(self) -> bool {
        self == Face::Symbol
    }

    #[inline(always)]
    pub fn is_dingbats(self) -> bool {
        self == Face::Dingbats
    }
}

/// 64-byte hardware cache-line aligned font metric table.
/// Ensures 100% L1 cache-line locality and zero cache-line boundary crossings.
#[repr(align(64))]
#[derive(Clone, Copy, Debug)]
pub struct AlignedTable<const N: usize>(pub [u16; N]);

impl<const N: usize> std::ops::Deref for AlignedTable<N> {
    type Target = [u16; N];
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// Flat compile-time lookup tables for ASCII 0..=127 in .rodata (64-byte cache-line aligned)
pub static TIMES_ROMAN_WIDTHS: AlignedTable<128> = AlignedTable([
    // 0..=31 (control characters, default to space width 250 for tab/cr/lf, else 0)
    0, 0, 0, 0, 0, 0, 0, 0, 0, 250, 250, 0, 0, 250, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, // 32 (' ') ..= 47 ('/')
    250, 333, 408, 500, 500, 833, 778, 333, 333, 333, 500, 564, 250, 333, 250, 278,
    // 48 ('0') ..= 57 ('9'), 58 (':') ..= 63 ('?')
    500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 278, 278, 564, 564, 564, 444,
    // 64 ('@') ..= 79 ('O')
    921, 722, 667, 667, 722, 611, 556, 722, 722, 333, 389, 722, 611, 889, 722, 722,
    // 80 ('P') ..= 95 ('_')
    556, 722, 667, 556, 611, 722, 722, 944, 722, 722, 611, 333, 278, 333, 469, 500,
    // 96 ('`') ..= 111 ('o')
    333, 444, 500, 444, 500, 444, 333, 500, 500, 278, 278, 500, 278, 778, 500, 500,
    // 112 ('p') ..= 127 (DEL)
    500, 500, 333, 389, 278, 500, 500, 722, 500, 500, 444, 480, 200, 480, 541, 0,
]);

pub static TIMES_BOLD_WIDTHS: AlignedTable<128> = AlignedTable([
    // 0..=31
    0, 0, 0, 0, 0, 0, 0, 0, 0, 250, 250, 0, 0, 250, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, // 32 (' ') ..= 47 ('/')
    250, 333, 555, 500, 500, 1000, 833, 333, 333, 333, 500, 570, 250, 333, 250, 278,
    // 48 ('0') ..= 57 ('9'), 58 (':') ..= 63 ('?')
    500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 333, 333, 570, 570, 570, 500,
    // 64 ('@') ..= 79 ('O')
    930, 722, 667, 722, 722, 667, 611, 778, 778, 389, 500, 778, 667, 944, 722, 778,
    // 80 ('P') ..= 95 ('_')
    611, 778, 722, 556, 667, 722, 722, 1000, 722, 722, 667, 333, 278, 333, 581, 500,
    // 96 ('`') ..= 111 ('o')
    333, 500, 556, 444, 556, 444, 333, 500, 556, 278, 333, 556, 278, 833, 556, 500,
    // 112 ('p') ..= 127 (DEL)
    556, 556, 444, 389, 333, 556, 500, 722, 500, 500, 444, 394, 220, 394, 520, 0,
]);

pub static TIMES_ITALIC_WIDTHS: AlignedTable<128> = AlignedTable([
    // 0..=31
    0, 0, 0, 0, 0, 0, 0, 0, 0, 250, 250, 0, 0, 250, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, // 32 (' ') ..= 47 ('/')
    250, 333, 420, 500, 500, 833, 778, 333, 333, 333, 500, 675, 250, 333, 250, 278,
    // 48 ('0') ..= 57 ('9'), 58 (':') ..= 63 ('?')
    500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 333, 333, 675, 675, 675, 500,
    // 64 ('@') ..= 79 ('O')
    920, 611, 611, 667, 722, 611, 611, 722, 722, 333, 444, 667, 556, 833, 667, 722,
    // 80 ('P') ..= 95 ('_')
    611, 722, 611, 500, 556, 722, 611, 833, 611, 556, 556, 389, 278, 389, 422, 500,
    // 96 ('`') ..= 111 ('o')
    333, 500, 500, 444, 500, 444, 278, 500, 500, 278, 278, 444, 278, 722, 500, 500,
    // 112 ('p') ..= 127 (DEL)
    500, 500, 389, 389, 278, 500, 444, 667, 444, 444, 389, 400, 275, 400, 541, 0,
]);

pub static TIMES_BOLD_ITALIC_WIDTHS: AlignedTable<128> = AlignedTable([
    // 0..=31
    0, 0, 0, 0, 0, 0, 0, 0, 0, 250, 250, 0, 0, 250, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, // 32 (' ') ..= 47 ('/')
    250, 389, 555, 500, 500, 833, 778, 333, 333, 333, 500, 570, 250, 333, 250, 278,
    // 48 ('0') ..= 57 ('9'), 58 (':') ..= 63 ('?')
    500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 333, 333, 570, 570, 570, 500,
    // 64 ('@') ..= 79 ('O')
    832, 667, 667, 667, 722, 667, 667, 722, 778, 389, 500, 667, 611, 889, 722, 722,
    // 80 ('P') ..= 95 ('_')
    611, 722, 667, 556, 611, 722, 667, 889, 667, 611, 611, 333, 278, 333, 570, 500,
    // 96 ('`') ..= 111 ('o')
    333, 500, 500, 444, 500, 444, 333, 500, 556, 278, 278, 500, 278, 778, 556, 500,
    // 112 ('p') ..= 127 (DEL)
    500, 500, 389, 389, 278, 556, 444, 667, 500, 444, 389, 348, 220, 348, 570, 0,
]);

pub static HELVETICA_ROMAN_WIDTHS: AlignedTable<128> = AlignedTable([
    // 0..=31 (control characters, default to space width 278 for whitespace like tab, else 0)
    0, 0, 0, 0, 0, 0, 0, 0, 0, 278, 278, 0, 0, 278, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, // 32 (' ') ..= 47 ('/')
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278,
    // 48 ('0') ..= 57 ('9'), 58 (':') ..= 63 ('?')
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556,
    // 64 ('@') ..= 79 ('O')
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
    // 80 ('P') ..= 95 ('_')
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 500,
    // 96 ('`') ..= 111 ('o')
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
    // 112 ('p') ..= 127 (DEL)
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, 556,
]);

pub static HELVETICA_BOLD_WIDTHS: AlignedTable<128> = AlignedTable([
    // 0..=31
    0, 0, 0, 0, 0, 0, 0, 0, 0, 278, 278, 0, 0, 278, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, // 32 (' ') ..= 47 ('/')
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278,
    // 48 ('0') ..= 57 ('9'), 58 (':') ..= 63 ('?')
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611,
    // 64 ('@') ..= 79 ('O')
    975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778,
    // 80 ('P') ..= 95 ('_')
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556,
    // 96 ('`') ..= 111 ('o')
    333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611,
    // 112 ('p') ..= 127 (DEL)
    611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584, 611,
]);

// Backwards-compatible constants for code referencing ROMAN_WIDTHS / BOLD_WIDTHS:
pub static ROMAN_WIDTHS: AlignedTable<128> = TIMES_ROMAN_WIDTHS;
pub static BOLD_WIDTHS: AlignedTable<128> = TIMES_BOLD_WIDTHS;

// Palatino Typeface Family (pplr8a, pplb8a, pplri8a, pplbi8a)
pub static PALATINO_ROMAN_WIDTHS: AlignedTable<128> = AlignedTable([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 250, 250, 0, 0, 250, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 250, 278, 371, 500, 500, 840, 778, 278, 333, 333, 389, 606, 250, 333, 250, 606, 500, 500,
    500, 500, 500, 500, 500, 500, 500, 500, 250, 250, 606, 606, 606, 444, 747, 778, 611, 709, 774,
    611, 556, 763, 832, 337, 333, 726, 611, 946, 831, 786, 604, 786, 668, 525, 613, 778, 722, 1000,
    667, 667, 667, 333, 606, 333, 606, 500, 278, 500, 553, 444, 611, 479, 333, 556, 582, 291, 234,
    556, 291, 883, 582, 546, 601, 560, 395, 424, 326, 603, 565, 834, 516, 556, 500, 333, 606, 333,
    606, 0,
]);

pub static PALATINO_BOLD_WIDTHS: AlignedTable<128> = AlignedTable([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 250, 250, 0, 0, 250, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 250, 278, 402, 500, 500, 889, 833, 278, 333, 333, 444, 606, 250, 333, 250, 296, 500, 500,
    500, 500, 500, 500, 500, 500, 500, 500, 250, 250, 606, 606, 606, 444, 747, 778, 667, 722, 833,
    611, 556, 833, 833, 389, 389, 778, 611, 1000, 833, 833, 611, 833, 722, 611, 667, 778, 778,
    1000, 667, 667, 667, 333, 606, 333, 606, 500, 278, 500, 611, 444, 611, 500, 389, 556, 611, 333,
    333, 611, 333, 889, 611, 556, 611, 611, 389, 444, 333, 611, 556, 833, 500, 556, 500, 310, 606,
    310, 606, 0,
]);

pub static PALATINO_ITALIC_WIDTHS: AlignedTable<128> = AlignedTable([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 250, 250, 0, 0, 250, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 250, 333, 500, 500, 500, 889, 778, 278, 333, 333, 389, 606, 250, 333, 250, 296, 500, 500,
    500, 500, 500, 500, 500, 500, 500, 500, 250, 250, 606, 606, 606, 500, 747, 722, 611, 667, 778,
    611, 556, 722, 778, 333, 333, 667, 556, 944, 778, 778, 611, 778, 667, 556, 611, 778, 722, 944,
    722, 667, 667, 333, 606, 333, 606, 500, 278, 444, 463, 407, 500, 389, 278, 500, 500, 278, 278,
    444, 278, 778, 556, 444, 500, 463, 389, 389, 333, 556, 500, 722, 500, 500, 444, 333, 606, 333,
    606, 0,
]);

pub static PALATINO_BOLD_ITALIC_WIDTHS: AlignedTable<128> = AlignedTable([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 250, 250, 0, 0, 250, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 250, 333, 500, 500, 500, 889, 833, 278, 333, 333, 444, 606, 250, 389, 250, 315, 500, 500,
    500, 500, 500, 500, 500, 500, 500, 500, 250, 250, 606, 606, 606, 444, 833, 722, 667, 685, 778,
    611, 556, 778, 778, 389, 389, 722, 611, 944, 778, 833, 667, 833, 722, 556, 611, 778, 667, 1000,
    722, 611, 667, 333, 606, 333, 606, 500, 278, 556, 537, 444, 556, 444, 333, 500, 556, 333, 333,
    556, 333, 833, 556, 556, 556, 537, 389, 444, 389, 556, 556, 833, 500, 556, 500, 333, 606, 333,
    606, 0,
]);

// Computer Modern STEM Family (cmr10, cmmi10, cmsy10, cmex10, cmtt10)
pub static CMR10_WIDTHS: AlignedTable<128> = AlignedTable([
    625, 833, 778, 694, 667, 750, 722, 778, 722, 278, 278, 583, 556, 278, 833, 833, 278, 306, 500,
    500, 500, 500, 500, 750, 444, 500, 722, 778, 500, 903, 1014, 778, 278, 278, 500, 833, 500, 833,
    778, 278, 389, 389, 500, 778, 278, 333, 278, 500, 500, 500, 500, 500, 500, 500, 500, 500, 500,
    500, 278, 278, 278, 778, 472, 472, 778, 750, 708, 722, 764, 681, 653, 785, 750, 361, 514, 778,
    625, 917, 750, 778, 681, 778, 736, 556, 722, 750, 750, 1028, 750, 750, 611, 278, 500, 278, 500,
    278, 278, 500, 556, 444, 556, 444, 306, 500, 556, 278, 306, 528, 278, 833, 556, 500, 556, 528,
    392, 394, 389, 556, 528, 722, 528, 528, 444, 500, 1000, 500, 500, 500,
]);

pub static CMMI10_WIDTHS: AlignedTable<128> = AlignedTable([
    615, 833, 763, 694, 742, 831, 780, 583, 667, 651, 651, 640, 566, 651, 444, 406, 438, 497, 469,
    354, 576, 583, 603, 494, 438, 570, 517, 571, 437, 540, 596, 626, 651, 622, 466, 591, 828, 517,
    363, 654, 1000, 1000, 1000, 1000, 278, 278, 500, 500, 500, 500, 500, 500, 500, 500, 500, 500,
    500, 500, 278, 278, 778, 500, 778, 500, 531, 750, 759, 715, 828, 738, 643, 786, 831, 440, 555,
    849, 681, 970, 803, 763, 642, 791, 759, 613, 584, 683, 583, 944, 828, 581, 683, 389, 389, 389,
    1000, 1000, 417, 529, 429, 433, 520, 466, 490, 477, 576, 345, 412, 521, 298, 878, 600, 485,
    503, 446, 451, 469, 361, 572, 485, 716, 572, 490, 465, 322, 384, 636, 500, 278,
]);

pub static CMSY10_WIDTHS: AlignedTable<128> = AlignedTable([
    778, 278, 778, 500, 778, 500, 778, 778, 778, 1000, 1000, 778, 778, 1000, 500, 500, 778, 778,
    778, 778, 778, 778, 778, 778, 778, 778, 778, 778, 1000, 1000, 778, 778, 1000, 1000, 500, 500,
    1000, 1000, 1000, 778, 1000, 1000, 611, 611, 1000, 1000, 1000, 778, 275, 1000, 667, 667, 889,
    889, 0, 0, 556, 556, 667, 500, 722, 722, 778, 778, 611, 798, 657, 527, 771, 528, 719, 595, 845,
    545, 678, 762, 690, 1201, 820, 796, 696, 817, 848, 606, 545, 626, 613, 988, 713, 668, 725, 667,
    667, 667, 667, 667, 611, 611, 444, 444, 444, 444, 500, 500, 389, 389, 278, 500, 500, 611, 500,
    278, 833, 750, 833, 417, 667, 667, 778, 778, 444, 444, 444, 611, 778, 778, 778, 778,
]);

pub static CMEX10_WIDTHS: AlignedTable<128> = AlignedTable([
    458, 458, 417, 417, 472, 472, 472, 472, 583, 792, 792, 472, 333, 792, 578, 578, 597, 597, 736,
    736, 528, 528, 583, 583, 583, 583, 750, 750, 750, 750, 1044, 1044, 792, 792, 583, 583, 639,
    639, 639, 639, 806, 806, 806, 806, 1278, 1278, 811, 811, 875, 875, 667, 667, 667, 667, 667,
    667, 889, 889, 889, 889, 889, 889, 889, 667, 875, 875, 875, 875, 611, 611, 833, 1111, 472, 556,
    1111, 1511, 1111, 1511, 1111, 1511, 1056, 944, 472, 833, 833, 833, 833, 833, 1444, 1278, 556,
    1111, 1111, 1111, 1111, 1111, 944, 1278, 556, 1000, 1444, 556, 1000, 1444, 472, 472, 528, 528,
    528, 528, 667, 667, 1000, 1000, 1000, 1000, 1056, 1056, 1056, 778, 667, 667, 450, 450, 450,
    450, 778, 778,
]);

pub static CMTT10_WIDTHS: AlignedTable<128> = AlignedTable([
    525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525,
    525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525,
    525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525,
    525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525,
    525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525,
    525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525,
    525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525, 525,
]);

pub static COURIER_WIDTHS: AlignedTable<128> = AlignedTable([600; 128]);

// Adobe Standard 14 Symbol Font (Greek alphabet and STEM mathematical operators)
pub static SYMBOL_WIDTHS: AlignedTable<256> = AlignedTable([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 250, 250, 0, 0, 250, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 250, 333, 713, 500, 549, 833, 778, 439, 333, 333, 500, 549, 250, 549, 250, 278, 500, 500,
    500, 500, 500, 500, 500, 500, 500, 500, 278, 278, 549, 549, 549, 444, 549, 722, 667, 722, 612,
    611, 763, 603, 722, 333, 631, 722, 686, 889, 722, 722, 768, 741, 556, 592, 611, 690, 439, 768,
    645, 795, 611, 333, 863, 333, 658, 500, 500, 631, 549, 549, 494, 439, 521, 411, 603, 329, 603,
    549, 549, 576, 521, 549, 549, 521, 549, 603, 439, 576, 713, 686, 493, 686, 494, 480, 200, 480,
    549, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 750, 620, 247, 549, 167, 713, 500, 753, 753, 753, 753, 1042, 987, 603, 987, 603, 400,
    549, 411, 549, 549, 713, 494, 460, 549, 549, 549, 549, 1000, 603, 1000, 658, 823, 686, 795,
    987, 768, 768, 823, 768, 768, 713, 713, 713, 713, 713, 713, 713, 768, 713, 790, 790, 890, 823,
    549, 250, 713, 603, 603, 1042, 987, 603, 987, 603, 494, 329, 790, 790, 786, 713, 384, 384, 384,
    384, 384, 384, 494, 494, 494, 494, 0, 329, 274, 686, 686, 686, 384, 384, 384, 384, 384, 384,
    494, 494, 494, 0,
]);

// Adobe Standard 14 ZapfDingbats Font
pub static ZAPFDINGBATS_WIDTHS: AlignedTable<256> = AlignedTable([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 278, 278, 0, 0, 278, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 278, 974, 961, 974, 980, 719, 789, 790, 791, 690, 960, 939, 549, 855, 911, 933, 911, 945,
    974, 755, 846, 762, 761, 571, 677, 763, 760, 759, 754, 494, 552, 537, 577, 692, 786, 788, 788,
    790, 793, 794, 816, 823, 789, 841, 823, 833, 816, 831, 923, 744, 723, 749, 790, 792, 695, 776,
    768, 792, 759, 707, 708, 682, 701, 826, 815, 789, 789, 707, 687, 696, 689, 786, 787, 713, 791,
    785, 791, 873, 761, 762, 762, 759, 759, 892, 892, 788, 784, 438, 138, 277, 415, 392, 392, 668,
    668, 0, 390, 390, 317, 317, 276, 276, 509, 509, 410, 410, 234, 234, 334, 334, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 732, 544, 544, 910, 667, 760, 760, 776, 595, 694, 626,
    788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788,
    788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788, 788,
    788, 788, 894, 838, 1016, 458, 748, 924, 748, 918, 927, 928, 928, 834, 873, 828, 924, 924, 917,
    930, 931, 463, 883, 836, 836, 867, 867, 696, 696, 874, 0, 874, 760, 946, 771, 865, 771, 888,
    967, 888, 831, 873, 927, 970, 918, 0,
]);

/// Converts Unicode Greek and STEM mathematical symbols to standard Adobe Symbol font byte code.
#[inline]
pub fn unicode_to_symbol_byte(ch: char) -> Option<u8> {
    let b = match ch {
        // Greek lowercase
        'α' => b'a',
        'β' => b'b',
        'γ' => b'g',
        'δ' => b'd',
        'ε' => b'e',
        'ζ' => b'z',
        'η' => b'h',
        'θ' => b'q',
        'ι' => b'i',
        'κ' => b'k',
        'λ' => b'l',
        'μ' => b'm',
        'ν' => b'n',
        'ξ' => b'x',
        'ο' => b'o',
        'π' => b'p',
        'ρ' => b'r',
        'ς' => b'V',
        'σ' => b's',
        'τ' => b't',
        'υ' => b'u',
        'φ' => b'f',
        'χ' => b'c',
        'ψ' => b'y',
        'ω' => b'w',
        // Greek uppercase
        'Α' => b'A',
        'Β' => b'B',
        'Γ' => b'G',
        'Δ' => b'D',
        'Ε' => b'E',
        'Ζ' => b'Z',
        'Η' => b'H',
        'Θ' => b'Q',
        'Ι' => b'I',
        'Κ' => b'K',
        'Λ' => b'L',
        'Μ' => b'M',
        'Ν' => b'N',
        'Ξ' => b'X',
        'Ο' => b'O',
        'Π' => b'P',
        'Ρ' => b'R',
        'Σ' => b'S',
        'Τ' => b'T',
        'Υ' => b'U',
        'Φ' => b'F',
        'Χ' => b'C',
        'Ψ' => b'Y',
        'Ω' => b'W',
        // Common STEM operators & relations
        '∀' => 0x22,
        '∃' => 0x24,
        '∋' => 0x27,
        '∗' => 0x2A,
        '−' => 0x2D,
        '≅' => 0x40,
        '∴' => 0x5C,
        '⊥' => 0x5E,
        '∼' => 0x7E,
        '≤' => 0xA3,
        '∕' => 0xA4,
        '∞' => 0xA5,
        '±' => 0xB1,
        '≥' => 0xB3,
        '×' => 0xB4,
        '∝' => 0xB5,
        '∂' => 0xB6,
        '≠' => 0xB9,
        '≡' => 0xBA,
        '≈' => 0xBB,
        '⊕' => 0xC4,
        '⊗' => 0xC5,
        '∅' => 0xC6,
        '∩' => 0xC7,
        '∪' => 0xC8,
        '⊃' => 0xC9,
        '⊇' => 0xCA,
        '⊂' => 0xCC,
        '⊆' => 0xCD,
        '∈' => 0xCE,
        '∉' => 0xCF,
        '∇' => 0xD1,
        '∏' => 0xD5,
        '√' => 0xD6,
        '⋅' => 0xD7,
        '¬' => 0xD8,
        '∧' => 0xD9,
        '∨' => 0xDA,
        '⇔' => 0xDB,
        '⇐' => 0xDC,
        '⇑' => 0xDD,
        '⇒' => 0xDE,
        '⇓' => 0xDF,
        '∑' => 0xE5,
        '∫' => 0xF2,
        '←' => 0xAC,
        '↑' => 0xAD,
        '→' | '⟶' => 0xAE,
        '↓' => 0xAF,
        '↔' => 0xAB,
        '∬' | '∭' | '∮' => 0xF2,
        '·' => 0xD7,
        '∓' => 0xB1,
        '⟵' => 0xAC,
        other if (other as u32) >= 0x20 && (other as u32) <= 0xFF => other as u8,
        _ => return None,
    };
    Some(b)
}

#[inline]
pub fn symbol_char_units(character: char) -> u16 {
    let character_code = character as u32;
    if character_code < 256 {
        SYMBOL_WIDTHS[character_code as usize]
    } else if let Some(byte) = unicode_to_symbol_byte(character) {
        SYMBOL_WIDTHS[byte as usize]
    } else {
        500
    }
}

#[inline]
pub fn dingbats_char_units(character: char) -> u16 {
    let character_code = character as u32;
    if character_code < 256 {
        ZAPFDINGBATS_WIDTHS[character_code as usize]
    } else {
        788
    }
}

/// Resolves the 64-byte aligned ASCII font width table for a given family and face in O(1) time.
#[inline(always)]
pub fn resolve_width_table(family: FontFamily, font_face: Face) -> &'static [u16; 128] {
    if font_face.is_mono() {
        return match family {
            FontFamily::ComputerModern => &CMTT10_WIDTHS,
            _ => &COURIER_WIDTHS,
        };
    }
    match family {
        FontFamily::Times => match font_face {
            Face::Roman => &TIMES_ROMAN_WIDTHS,
            Face::Bold => &TIMES_BOLD_WIDTHS,
            Face::Italic => &TIMES_ITALIC_WIDTHS,
            Face::BoldItalic => &TIMES_BOLD_ITALIC_WIDTHS,
            _ => &TIMES_ROMAN_WIDTHS,
        },
        FontFamily::Helvetica => {
            if font_face.is_bold() {
                &HELVETICA_BOLD_WIDTHS
            } else {
                &HELVETICA_ROMAN_WIDTHS
            }
        }
        FontFamily::Palatino => match font_face {
            Face::Roman => &PALATINO_ROMAN_WIDTHS,
            Face::Bold => &PALATINO_BOLD_WIDTHS,
            Face::Italic => &PALATINO_ITALIC_WIDTHS,
            Face::BoldItalic => &PALATINO_BOLD_ITALIC_WIDTHS,
            _ => &PALATINO_ROMAN_WIDTHS,
        },
        FontFamily::ComputerModern => match font_face {
            Face::Roman | Face::Bold => &CMR10_WIDTHS,
            Face::Italic | Face::BoldItalic => &CMMI10_WIDTHS,
            Face::Mono | Face::MonoBold => &CMTT10_WIDTHS,
            Face::Symbol => &CMSY10_WIDTHS,
            Face::Dingbats => &CMR10_WIDTHS,
        },
    }
}

/// Resolves standard space width in 1000-units/em space with zero memory lookups.
#[inline(always)]
pub fn resolve_space_units(family: FontFamily, font_face: Face) -> u32 {
    if font_face.is_mono() {
        return match family {
            FontFamily::ComputerModern => 525,
            _ => 600,
        };
    }
    match family {
        FontFamily::Times | FontFamily::Palatino => 250,
        FontFamily::Helvetica | FontFamily::ComputerModern => 278,
    }
}

/// Resolves tabular digit ('0'..'9') width in 1000-units/em space with zero memory lookups.
#[inline(always)]
pub fn resolve_digit_units(family: FontFamily, font_face: Face) -> u32 {
    if font_face.is_mono() {
        match family {
            FontFamily::ComputerModern => 525,
            _ => 600,
        }
    } else {
        match family {
            FontFamily::Times | FontFamily::Palatino | FontFamily::ComputerModern => 500,
            FontFamily::Helvetica => 556,
        }
    }
}

#[inline]
pub fn char_units_family(character: char, font_face: Face, family: FontFamily) -> u16 {
    if font_face.is_mono() {
        return match family {
            FontFamily::ComputerModern => 525,
            _ => 600,
        };
    }
    if font_face == Face::Symbol {
        return symbol_char_units(character);
    }
    if font_face == Face::Dingbats {
        return dingbats_char_units(character);
    }
    let character_code = character as u32;
    if character_code < 128 {
        let table = resolve_width_table(family, font_face);
        table[character_code as usize]
    } else {
        match family {
            FontFamily::Times => times_units_cold(character, font_face),
            FontFamily::Helvetica => {
                if font_face.is_bold() {
                    helvetica_bold_units_cold(character)
                } else {
                    helvetica_roman_units_cold(character)
                }
            }
            FontFamily::Palatino => palatino_units_cold(character, font_face),
            FontFamily::ComputerModern => cm_units_cold(character, font_face),
        }
    }
}

#[inline]
pub fn char_units(character: char, font_face: Face) -> u16 {
    char_units_family(character, font_face, FontFamily::Times)
}

/// Computes the width of `character` in PDF text space for `font_face` at `font_size_points` with `family`.
#[inline]
pub fn width_family(
    character: char,
    font_face: Face,
    font_size_points: f32,
    family: FontFamily,
) -> f32 {
    char_units_family(character, font_face, family) as f32 * (font_size_points * 0.001)
}

/// Computes the width of `character` in PDF text space for `font_face` at `font_size_points` (default: Times).
#[inline]
pub fn width(character: char, font_face: Face, font_size_points: f32) -> f32 {
    width_family(character, font_face, font_size_points, FontFamily::Times)
}

/// 4-Way SWAR unrolled fast path: processes 4 ASCII characters per iteration
/// with independent accumulators to exploit CPU superscalar instruction-level parallelism.
#[inline]
pub fn string_units_table(text: &str, table: &[u16; 128]) -> u32 {
    let byte_slice = text.as_bytes();
    let mut acc0: u32 = 0;
    let mut acc1: u32 = 0;
    let mut acc2: u32 = 0;
    let mut acc3: u32 = 0;
    let mut byte_index = 0;

    while byte_index + 4 <= byte_slice.len() {
        let b0 = byte_slice[byte_index];
        let b1 = byte_slice[byte_index + 1];
        let b2 = byte_slice[byte_index + 2];
        let b3 = byte_slice[byte_index + 3];

        if (b0 | b1 | b2 | b3) < 128 {
            acc0 += table[b0 as usize] as u32;
            acc1 += table[b1 as usize] as u32;
            acc2 += table[b2 as usize] as u32;
            acc3 += table[b3 as usize] as u32;
            byte_index += 4;
        } else {
            // Non-ASCII fallback for first character in chunk
            if b0 < 128 {
                acc0 += table[b0 as usize] as u32;
                byte_index += 1;
            } else {
                let remaining_text = &text[byte_index..];
                let character = remaining_text.chars().next().unwrap();
                byte_index += character.len_utf8();
                acc0 += times_units_cold(character, Face::Roman) as u32;
            }
        }
    }

    // Scalar remainder loop for trailing 1..3 bytes
    while byte_index < byte_slice.len() {
        let byte_value = byte_slice[byte_index];
        if byte_value < 128 {
            acc0 += table[byte_value as usize] as u32;
            byte_index += 1;
        } else {
            let remaining_text = &text[byte_index..];
            let character = remaining_text.chars().next().unwrap();
            byte_index += character.len_utf8();
            acc0 += times_units_cold(character, Face::Roman) as u32;
        }
    }
    (acc0 + acc1) + (acc2 + acc3)
}

/// Fast branchless sum of font metric units for `text` with specified `family`.
pub fn string_units_family(text: &str, font_face: Face, family: FontFamily) -> u32 {
    if font_face.is_mono() {
        let byte_slice = text.as_bytes();
        let mut char_count = 0u32;
        let mut i = 0;
        while i < byte_slice.len() {
            let b = byte_slice[i];
            if b < 128 {
                char_count += 1;
                i += 1;
            } else {
                let character = text[i..].chars().next().unwrap();
                i += character.len_utf8();
                char_count += 1;
            }
        }
        let mono_unit = match family {
            FontFamily::ComputerModern => 525,
            _ => 600,
        };
        return char_count * mono_unit;
    }
    if font_face == Face::Symbol {
        let byte_slice = text.as_bytes();
        let mut acc = 0u32;
        let mut i = 0;
        while i < byte_slice.len() {
            let b = byte_slice[i];
            if b < 128 {
                acc += SYMBOL_WIDTHS[b as usize] as u32;
                i += 1;
            } else {
                let character = text[i..].chars().next().unwrap();
                i += character.len_utf8();
                acc += symbol_char_units(character) as u32;
            }
        }
        return acc;
    }
    if font_face == Face::Dingbats {
        let byte_slice = text.as_bytes();
        let mut acc = 0u32;
        let mut i = 0;
        while i < byte_slice.len() {
            let b = byte_slice[i];
            if b < 128 {
                acc += ZAPFDINGBATS_WIDTHS[b as usize] as u32;
                i += 1;
            } else {
                let character = text[i..].chars().next().unwrap();
                i += character.len_utf8();
                acc += dingbats_char_units(character) as u32;
            }
        }
        return acc;
    }
    let width_table = resolve_width_table(family, font_face);
    string_units_table(text, width_table)
}

/// Fast branchless sum of font metric units for `text` (default: Times).
#[inline]
pub fn string_units(text: &str, font_face: Face) -> u32 {
    string_units_family(text, font_face, FontFamily::Times)
}

/// Computes width of `text` in PDF points with a single floating-point multiplication.
#[inline]
pub fn string_width_family(
    text: &str,
    font_face: Face,
    font_size_points: f32,
    family: FontFamily,
) -> f32 {
    string_units_family(text, font_face, family) as f32 * (font_size_points * 0.001)
}

/// Computes width of `text` in PDF points with a single floating-point multiplication (default: Times).
#[inline]
pub fn string_width(text: &str, font_face: Face, font_size_points: f32) -> f32 {
    string_width_family(text, font_face, font_size_points, FontFamily::Times)
}

#[cold]
fn times_units_cold(character: char, font_face: Face) -> u16 {
    match character {
        '\u{00A0}' => 250,
        '–' => 500,
        '—' => {
            if font_face.is_italic() {
                889
            } else {
                1000
            }
        }
        '‘' | '’' => 333,
        '“' | '”' => match (font_face.is_bold(), font_face.is_italic()) {
            (false, false) => 444,
            (true, false) | (true, true) => 500,
            (false, true) => 556,
        },
        '…' => {
            if font_face.is_italic() && !font_face.is_bold() {
                889
            } else {
                1000
            }
        }
        '•' | '·' => 350,
        _ => {
            if let Some(b) = unicode_to_symbol_byte(character) {
                SYMBOL_WIDTHS[b as usize]
            } else {
                500
            }
        }
    }
}

#[cold]
fn helvetica_roman_units_cold(character: char) -> u16 {
    match character {
        '\u{00A0}' => 278,
        '–' | '—' => 556,
        '‘' | '’' => 191,
        '“' | '”' => 355,
        '…' => 834,
        '•' | '·' => 350,
        _ => {
            if let Some(b) = unicode_to_symbol_byte(character) {
                SYMBOL_WIDTHS[b as usize]
            } else {
                556
            }
        }
    }
}

#[cold]
fn helvetica_bold_units_cold(character: char) -> u16 {
    match character {
        '\u{00A0}' => 278,
        '–' | '—' => 556,
        '‘' | '’' => 238,
        '“' | '”' => 474,
        '…' => 834,
        '•' | '·' => 350,
        _ => {
            if let Some(b) = unicode_to_symbol_byte(character) {
                SYMBOL_WIDTHS[b as usize]
            } else {
                611
            }
        }
    }
}

#[cold]
fn palatino_units_cold(character: char, _font_face: Face) -> u16 {
    match character {
        '\u{00A0}' => 250,
        '–' => 500,
        '—' => 1000,
        '‘' | '’' => 278,
        '“' | '”' => 500,
        '…' => 1000,
        '•' | '·' => 350,
        _ => {
            if let Some(b) = unicode_to_symbol_byte(character) {
                SYMBOL_WIDTHS[b as usize]
            } else {
                500
            }
        }
    }
}

#[cold]
fn cm_units_cold(character: char, _font_face: Face) -> u16 {
    match character {
        '\u{00A0}' => 278,
        '–' => 500,
        '—' => 1000,
        '‘' | '’' => 278,
        '“' | '”' => 500,
        '…' => 1000,
        '•' | '·' => 350,
        _ => {
            if let Some(b) = unicode_to_symbol_byte(character) {
                SYMBOL_WIDTHS[b as usize]
            } else {
                500
            }
        }
    }
}
