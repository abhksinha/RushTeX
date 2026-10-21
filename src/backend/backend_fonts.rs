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

impl FontFamily {
    #[inline(always)]
    pub fn default_math_ps_name(self) -> &'static str {
        match self {
            FontFamily::ComputerModern => "NewCMMath-Regular",
            FontFamily::Times => "STIXTwoMath-Regular",
            FontFamily::Helvetica => "FiraMath-Regular",
            FontFamily::Palatino => "Asana-Math",
        }
    }
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
    Extension = 8,
    MathItalic = 9,
    Roman7 = 10,
    Roman5 = 11,
    MathItalic7 = 12,
    MathItalic5 = 13,
    Symbol7 = 14,
    Symbol5 = 15,
    AmsA = 16,
    AmsB = 17,
    /// `\mathsf` (cmss10)
    MathSans = 18,
    /// `\mathsf` at script sizes (cmss8, LaTeX `<5-8>`)
    MathSans8 = 19,
    /// `\mathtt` at script sizes (cmtt8)
    MathMono8 = 20,
    /// `\mathfrak` (eufm10)
    Fraktur = 21,
    /// `\mathfrak` script (eufm7)
    Fraktur7 = 22,
    /// `\mathfrak` scriptscript (eufm5)
    Fraktur5 = 23,
    /// `\boldsymbol` letters (cmmib10)
    BoldMathItalic = 24,
    /// cmmib7
    BoldMathItalic7 = 25,
    /// cmmib5
    BoldMathItalic5 = 26,
    /// `\boldsymbol` symbols (cmbsy10)
    BoldSymbol = 27,
    /// cmbsy7
    BoldSymbol7 = 28,
    /// cmbsy5
    BoldSymbol5 = 29,
    /// amsmath script-size extension font (cmex7, also used at 5pt)
    Extension7 = 30,
    /// `\scshape` / `\textsc` (cmcsc10)
    SmallCaps = 31,
    /// `\slshape` / `\textsl` (cmsl10)
    Slanted = 32,
    /// Math design size (cmmi6): OML/OMS fd files load it at its own size.
    MathItalic6 = 33,
    /// Math design size (cmmi8): OML/OMS fd files load it at its own size.
    MathItalic8 = 34,
    /// Math design size (cmmi9): OML/OMS fd files load it at its own size.
    MathItalic9 = 35,
    /// Math design size (cmmi12): OML/OMS fd files load it at its own size.
    MathItalic12 = 36,
    /// Math design size (cmsy6): OML/OMS fd files load it at its own size.
    Symbol6 = 37,
    /// Math design size (cmsy8): OML/OMS fd files load it at its own size.
    Symbol8 = 38,
    /// Math design size (cmsy9): OML/OMS fd files load it at its own size.
    Symbol9 = 39,
    /// `\sffamily\bfseries` (cmssbx10, ot1cmss.fd `bx/n` at every size)
    SansBold = 40,
    /// TS1 text symbols (`\textcopyright`, `\textasteriskcentered`): tcrm1000, the other
    /// sizes are `OpticalMaster`s. Codes are `backend_ts1::TS1_SLOT_CODE` of the slots.
    TextComp = 41,
}

impl Face {
    pub const BOLD_MASK: u8 = 1;
    pub const ITALIC_MASK: u8 = 2;
    pub const MONO_MASK: u8 = 4;

    /// Number of faces; `used_faces` bit masks are `u64` and `PackedWordAttr` stores 6 bits,
    /// so this must stay <= 64.
    pub const COUNT: usize = 42;

    pub const ALL: [Face; Face::COUNT] = [
        Face::Roman,
        Face::Bold,
        Face::Italic,
        Face::BoldItalic,
        Face::Mono,
        Face::MonoBold,
        Face::Symbol,
        Face::Dingbats,
        Face::Extension,
        Face::MathItalic,
        Face::Roman7,
        Face::Roman5,
        Face::MathItalic7,
        Face::MathItalic5,
        Face::Symbol7,
        Face::Symbol5,
        Face::AmsA,
        Face::AmsB,
        Face::MathSans,
        Face::MathSans8,
        Face::MathMono8,
        Face::Fraktur,
        Face::Fraktur7,
        Face::Fraktur5,
        Face::BoldMathItalic,
        Face::BoldMathItalic7,
        Face::BoldMathItalic5,
        Face::BoldSymbol,
        Face::BoldSymbol7,
        Face::BoldSymbol5,
        Face::Extension7,
        Face::SmallCaps,
        Face::Slanted,
        Face::MathItalic6,
        Face::MathItalic8,
        Face::MathItalic9,
        Face::MathItalic12,
        Face::Symbol6,
        Face::Symbol8,
        Face::Symbol9,
        Face::SansBold,
        Face::TextComp,
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
            7 => Face::Dingbats,
            8 => Face::Extension,
            9 => Face::MathItalic,
            10 => Face::Roman7,
            11 => Face::Roman5,
            12 => Face::MathItalic7,
            13 => Face::MathItalic5,
            14 => Face::Symbol7,
            15 => Face::Symbol5,
            16 => Face::AmsA,
            17 => Face::AmsB,
            18 => Face::MathSans,
            19 => Face::MathSans8,
            20 => Face::MathMono8,
            21 => Face::Fraktur,
            22 => Face::Fraktur7,
            23 => Face::Fraktur5,
            24 => Face::BoldMathItalic,
            25 => Face::BoldMathItalic7,
            26 => Face::BoldMathItalic5,
            27 => Face::BoldSymbol,
            28 => Face::BoldSymbol7,
            29 => Face::BoldSymbol5,
            30 => Face::Extension7,
            31 => Face::SmallCaps,
            32 => Face::Slanted,
            33 => Face::MathItalic6,
            34 => Face::MathItalic8,
            35 => Face::MathItalic9,
            36 => Face::MathItalic12,
            37 => Face::Symbol6,
            38 => Face::Symbol8,
            39 => Face::Symbol9,
            40 => Face::SansBold,
            41 => Face::TextComp,
            _ => Face::Roman,
        }
    }

    #[inline(always)]
    pub fn pdf_name(self, family: FontFamily) -> &'static str {
        if self == Face::Symbol {
            return match family {
                FontFamily::ComputerModern => "CMSY10",
                _ => "Symbol",
            };
        }
        if self == Face::Symbol7 {
            return "CMSY7";
        }
        if self == Face::Symbol5 {
            return "CMSY5";
        }
        if self == Face::Extension {
            return "CMEX10";
        }
        if self == Face::Dingbats {
            return "ZapfDingbats";
        }
        if self == Face::MathItalic {
            return match family {
                FontFamily::ComputerModern => "CMMI10",
                FontFamily::Times => "Times-Italic",
                FontFamily::Helvetica => "Helvetica-Oblique",
                FontFamily::Palatino => "Palatino-Italic",
            };
        }
        if self == Face::MathItalic7 {
            return "CMMI7";
        }
        if self == Face::MathItalic5 {
            return "CMMI5";
        }
        if self == Face::Roman7 {
            return "CMR7";
        }
        if self == Face::Roman5 {
            return "CMR5";
        }
        if self == Face::AmsA {
            return "MSAM10";
        }
        if self == Face::AmsB {
            return "MSBM10";
        }
        if (self as u8) >= 18 {
            return EXTRA_FACES[self as usize - 18].name;
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
                    ["CMR10", "CMBX10", "CMTI10", "CMBX10", "CMTT10", "CMTT10"];
                CM_NAMES[self as usize]
            }
        }
    }

    #[inline(always)]
    pub fn resource(self) -> &'static str {
        const RES: [&str; 18] = [
            "/Fr", "/Fb", "/Fi", "/Fbi", "/Fc", "/Fcb", "/Fs", "/Fz", "/Fe", "/Fmi",
            "/Fr7", "/Fr5", "/Fmi7", "/Fmi5", "/Fs7", "/Fs5", "/Fmsa", "/Fmsb",
        ];
        match RES.get(self as usize) {
            Some(r) => r,
            None => EXTRA_FACES[self as usize - 18].resource,
        }
    }

    #[inline(always)]
    pub fn with_bold(self, enable_bold: bool) -> Self {
        match self {
            Face::MathSans if enable_bold => return Face::SansBold,
            Face::SansBold if !enable_bold => return Face::MathSans,
            _ => {}
        }
        // cmbxsl10 is not bundled: bold wins over the slant.
        if self == Face::Slanted {
            return if enable_bold { Face::Bold } else { self };
        }
        if (self as u8) >= 6 {
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
        // Italic and upright are other shapes than slanted.
        if self == Face::Slanted {
            return if enable_italic { Face::Italic } else { Face::Roman };
        }
        if self.is_mono() || (self as u8) >= 6 {
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
        match self {
            Face::MathSans if enable_mono => return Face::Mono,
            Face::SansBold if enable_mono => return Face::MonoBold,
            _ => {}
        }
        // cmsltt10 is not bundled: typewriter wins over the slant.
        if self == Face::Slanted {
            return if enable_mono { Face::Mono } else { self };
        }
        if (self as u8) >= 6 {
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
        // The masks only describe the six body faces; the faces past them are numbered
        // sequentially and must not alias their bits.
        (self as u8) < 6 && (self as u8 & Self::BOLD_MASK) != 0
    }

    #[inline(always)]
    pub fn is_italic(self) -> bool {
        (self as u8) < 6 && (self as u8 & Self::ITALIC_MASK) != 0
    }

    #[inline(always)]
    pub fn is_mono(self) -> bool {
        matches!(self, Face::Mono | Face::MonoBold | Face::MathMono8)
    }

    #[inline(always)]
    pub fn is_dingbats(self) -> bool {
        self == Face::Dingbats
    }

    #[inline(always)]
    pub fn is_extension(self) -> bool {
        self == Face::Extension
    }

    /// Bit of this face in a `used_faces` mask.
    #[inline(always)]
    pub const fn bit(self) -> u64 {
        1 << (self as u8)
    }

    /// Whether this is a medium-weight cmmi face.
    #[inline(always)]
    pub const fn is_math_italic(self) -> bool {
        self.bit() & Face::MATH_ITALIC_MASK != 0
    }

    /// Whether this is a medium-weight cmsy face (OMS layout: `-` sits at code 0).
    #[inline(always)]
    pub const fn is_symbol(self) -> bool {
        self.bit() & Face::SYMBOL_MASK != 0
    }

    /// Computer Modern faces whose glyphs use OT1-style (text) ToUnicode.
    pub const OT1_MASK: u64 = Face::Roman.bit()
        | Face::Bold.bit()
        | Face::Italic.bit()
        | Face::Roman7.bit()
        | Face::Roman5.bit()
        | Face::Mono.bit()
        | Face::MonoBold.bit()
        | Face::MathSans.bit()
        | Face::MathSans8.bit()
        | Face::MathMono8.bit()
        | Face::Fraktur.bit()
        | Face::Fraktur7.bit()
        | Face::Fraktur5.bit()
        | Face::SmallCaps.bit()
        | Face::Slanted.bit()
        | Face::SansBold.bit();
    /// Computer Modern faces in the OML (math italic) encoding.
    pub const OML_MASK: u64 = Face::MATH_ITALIC_MASK
        | Face::BoldMathItalic.bit()
        | Face::BoldMathItalic7.bit()
        | Face::BoldMathItalic5.bit();
    /// Computer Modern faces in the OMS (math symbol) encoding.
    pub const OMS_MASK: u64 = Face::SYMBOL_MASK
        | Face::BoldSymbol.bit()
        | Face::BoldSymbol7.bit()
        | Face::BoldSymbol5.bit();
    /// The medium-weight cmmi faces (every design size).
    pub const MATH_ITALIC_MASK: u64 = Face::MathItalic.bit()
        | Face::MathItalic7.bit()
        | Face::MathItalic5.bit()
        | Face::MathItalic6.bit()
        | Face::MathItalic8.bit()
        | Face::MathItalic9.bit()
        | Face::MathItalic12.bit();
    /// The medium-weight cmsy faces (every design size).
    pub const SYMBOL_MASK: u64 = Face::Symbol.bit()
        | Face::Symbol7.bit()
        | Face::Symbol5.bit()
        | Face::Symbol6.bit()
        | Face::Symbol8.bit()
        | Face::Symbol9.bit();
    /// Faces in the OMX (math extension) encoding.
    pub const OMX_MASK: u64 = Face::Extension.bit() | Face::Extension7.bit();

    /// Typewriter faces (cmtt), whose OT1 layout differs from cmr's in the ASCII range.
    pub const OT1TT_MASK: u64 = Face::Mono.bit() | Face::MonoBold.bit() | Face::MathMono8.bit();
}

const _: () = assert!(Face::COUNT <= 64 && Face::ALL.len() == Face::COUNT);

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
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556,
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
    // Slots 0x09/0x0A/0x0D are the Psi/Omega/fl glyphs (cmr10.tfm), not whitespace.
    625, 833, 778, 694, 667, 750, 722, 778, 722, 778, 722, 583, 556, 556, 833, 833, 278, 306, 500,
    500, 500, 500, 500, 750, 444, 500, 722, 778, 500, 903, 1014, 778, 278, 278, 500, 833, 500, 833,
    778, 278, 389, 389, 500, 778, 278, 333, 278, 500, 500, 500, 500, 500, 500, 500, 500, 500, 500,
    500, 278, 278, 278, 778, 472, 472, 778, 750, 708, 722, 764, 681, 653, 785, 750, 361, 514, 778,
    625, 917, 750, 778, 681, 778, 736, 556, 722, 750, 750, 1028, 750, 750, 611, 278, 500, 278, 500,
    278, 278, 500, 556, 444, 556, 444, 306, 500, 556, 278, 306, 528, 278, 833, 556, 500, 556, 528,
    392, 394, 389, 556, 528, 722, 528, 528, 444, 500, 1000, 500, 500, 500,
]);

pub static CMBX10_WIDTHS: AlignedTable<128> = AlignedTable([
    692, 958, 894, 806, 767, 900, 831, 894, 831, 894, 831, 671, 639, 639, 958, 958, 319, 351,
    575, 575, 575, 575, 575, 869, 511, 597, 831, 894, 575, 1042, 1169, 894, 319, 350, 603, 958,
    575, 958, 894, 319, 447, 447, 575, 894, 319, 383, 319, 575, 575, 575, 575, 575, 575, 575,
    575, 575, 575, 575, 319, 319, 350, 894, 543, 543, 894, 869, 818, 831, 882, 756, 724, 904,
    900, 436, 594, 901, 692, 1092, 900, 864, 786, 864, 862, 639, 800, 885, 869, 1189, 869, 869,
    703, 319, 603, 319, 575, 319, 319, 559, 639, 511, 639, 527, 351, 575, 639, 319, 351, 607,
    319, 958, 639, 575, 639, 607, 474, 454, 447, 639, 607, 831, 607, 607, 511, 575, 1150, 575,
    575, 575,
]);

pub static CMTI10_WIDTHS: AlignedTable<128> = AlignedTable([
    627, 818, 767, 692, 664, 743, 716, 767, 716, 767, 716, 613, 562, 588, 882, 894, 307, 332,
    511, 511, 511, 511, 511, 831, 460, 537, 716, 716, 511, 883, 985, 767, 256, 307, 514, 818,
    769, 818, 767, 307, 409, 409, 511, 767, 307, 358, 307, 511, 511, 511, 511, 511, 511, 511,
    511, 511, 511, 511, 307, 307, 307, 767, 511, 511, 767, 743, 704, 716, 755, 678, 653, 774,
    743, 386, 525, 769, 627, 897, 743, 767, 678, 767, 729, 562, 716, 743, 743, 999, 743, 743,
    613, 307, 514, 307, 511, 307, 307, 511, 460, 460, 511, 460, 307, 460, 511, 307, 307, 460,
    256, 818, 562, 511, 511, 460, 422, 409, 332, 537, 460, 664, 464, 486, 409, 511, 1022, 511,
    511, 511,
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

pub static CMR7_WIDTHS: AlignedTable<128> = AlignedTable([
    706, 938, 877, 782, 754, 843, 815, 877, 815, 877, 815, 678, 647, 647, 970, 970, 323, 354,
    569, 569, 569, 569, 569, 843, 508, 569, 815, 877, 569, 1014, 1137, 877, 323, 323, 569, 938,
    569, 938, 877, 323, 446, 446, 569, 877, 323, 385, 323, 569, 569, 569, 569, 569, 569, 569,
    569, 569, 569, 569, 323, 323, 323, 877, 539, 539, 877, 843, 799, 815, 860, 768, 737, 884,
    843, 413, 583, 874, 706, 1028, 843, 877, 768, 877, 829, 631, 815, 843, 843, 1151, 843, 843,
    692, 323, 569, 323, 569, 323, 323, 569, 631, 508, 631, 508, 354, 569, 631, 323, 354, 600,
    323, 938, 631, 569, 631, 600, 446, 453, 446, 631, 600, 815, 600, 600, 508, 569, 1139, 569,
    569, 569,
]);

pub static CMR5_WIDTHS: AlignedTable<128> = AlignedTable([
    831, 1097, 1028, 911, 889, 981, 958, 1028, 958, 1028, 958, 681, 681, 403, 403, 646, 403, 438,
    681, 681, 681, 681, 681, 981, 611, 681, 958, 1028, 681, 1178, 1317, 1028, 403, 403, 681, 1097,
    681, 1097, 1028, 403, 542, 542, 681, 1028, 403, 472, 403, 681, 681, 681, 681, 681, 681, 681,
    681, 681, 681, 681, 403, 403, 1028, 1028, 1028, 646, 1028, 981, 935, 958, 1004, 900, 865, 1033,
    981, 494, 692, 1015, 831, 1189, 981, 1028, 900, 1028, 969, 750, 958, 981, 981, 1328, 981, 981,
    819, 403, 681, 403, 681, 403, 403, 681, 750, 611, 750, 611, 438, 681, 750, 403, 438, 715,
    403, 1097, 750, 681, 750, 715, 542, 549, 542, 750, 715, 958, 715, 715, 611, 681, 1361, 681,
    681, 681,
]);

pub static CMMI7_WIDTHS: AlignedTable<128> = AlignedTable([
    694, 954, 869, 798, 844, 936, 886, 678, 770, 717, 880, 743, 648, 600, 519, 476, 520, 589,
    544, 423, 669, 678, 695, 573, 520, 668, 593, 662, 527, 633, 687, 714, 756, 720, 540, 690,
    950, 593, 439, 751, 1139, 1139, 1139, 1139, 339, 339, 585, 585, 585, 585, 585, 585, 585, 585,
    585, 585, 585, 585, 339, 339, 893, 585, 893, 585, 610, 859, 863, 819, 934, 839, 725, 889,
    936, 506, 632, 960, 784, 1089, 905, 869, 727, 900, 861, 701, 675, 778, 675, 1074, 937, 672,
    778, 462, 462, 462, 1139, 1139, 478, 620, 502, 511, 595, 542, 557, 557, 669, 404, 473, 607,
    361, 1014, 706, 564, 589, 524, 530, 539, 432, 675, 571, 826, 648, 579, 546, 399, 442, 730,
    585, 339,
]);

pub static CMMI5_WIDTHS: AlignedTable<128> = AlignedTable([
    833, 1153, 1048, 967, 1018, 1110, 1065, 840, 944, 893, 1061, 913, 791, 747, 654, 614, 667, 744,
    677, 550, 828, 840, 850, 712, 667, 831, 726, 815, 682, 792, 842, 865, 931, 886, 675, 855,
    1145, 726, 578, 918, 1361, 1361, 1361, 1361, 458, 458, 736, 736, 736, 736, 736, 736, 736, 736,
    736, 736, 736, 736, 458, 458, 1083, 736, 1083, 736, 749, 1036, 1037, 996, 1110, 1007, 867, 1064,
    1110, 627, 773, 1139, 956, 1284, 1076, 1048, 875, 1082, 1030, 856, 832, 944, 828, 1279, 1113, 824,
    943, 597, 597, 597, 1361, 1361, 597, 774, 633, 649, 740, 677, 684, 701, 828, 534, 588, 758,
    480, 1228, 881, 703, 740, 659, 671, 670, 564, 846, 722, 1009, 792, 731, 689, 534, 553, 889,
    736, 458,
]);

pub static CMSY7_WIDTHS: AlignedTable<128> = AlignedTable([
    893, 339, 893, 585, 893, 585, 893, 893, 893, 893, 893, 893, 893, 1139, 585, 585, 893, 893,
    893, 893, 893, 893, 893, 893, 893, 893, 893, 893, 1139, 1139, 893, 893, 1139, 1139, 585, 585,
    1139, 1139, 1139, 893, 1139, 1139, 708, 708, 1139, 1139, 1139, 893, 329, 1139, 770, 770, 1016, 1016,
    0, 0, 647, 647, 770, 585, 831, 831, 893, 893, 708, 918, 753, 620, 889, 616, 818, 689,
    979, 647, 782, 872, 792, 1343, 936, 906, 809, 936, 981, 702, 648, 718, 720, 1135, 819, 764,
    823, 770, 770, 770, 770, 770, 708, 708, 524, 524, 524, 524, 585, 585, 462, 462, 339, 585,
    585, 708, 585, 339, 938, 859, 954, 494, 770, 770, 893, 893, 524, 524, 524, 708, 893, 893,
    893, 893,
]);

pub static CMSY5_WIDTHS: AlignedTable<128> = AlignedTable([
    1083, 458, 1083, 736, 1083, 736, 1083, 1083, 1083, 1083, 1083, 1083, 1083, 1361, 736, 736, 1083, 1083,
    1083, 1083, 1083, 1083, 1083, 1083, 1083, 1083, 1083, 1083, 1361, 1361, 1083, 1083, 1361, 1361, 736, 736,
    1361, 1361, 1361, 1083, 1361, 1361, 875, 875, 1361, 1361, 1361, 1083, 441, 1361, 944, 944, 1222, 1222,
    0, 0, 806, 806, 944, 736, 1014, 1014, 1083, 1083, 875, 1113, 919, 782, 1084, 771, 988, 850,
    1195, 820, 958, 1055, 965, 1568, 1126, 1089, 998, 1132, 1196, 868, 822, 874, 899, 1368, 997, 929,
    991, 944, 944, 944, 944, 944, 875, 875, 667, 667, 667, 667, 736, 736, 597, 597, 458, 736,
    736, 875, 736, 458, 1097, 1036, 1153, 632, 944, 944, 1083, 1083, 667, 667, 667, 875, 1083, 1083,
    1083, 1083,
]);

pub static MSAM10_WIDTHS: AlignedTable<128> = AlignedTable([
    778, 778, 778, 778, 778, 278, 667, 667, 778, 778, 1000, 1000, 778, 722, 889, 611, 1000, 1000,
    1000, 1000, 833, 833, 417, 417, 417, 417, 1111, 1111, 1000, 1000, 500, 500, 1000, 1389, 1000, 1000,
    778, 778, 778, 778, 1111, 667, 667, 778, 778, 778, 778, 778, 778, 778, 778, 778, 778, 778,
    778, 778, 275, 500, 778, 778, 778, 778, 778, 778, 778, 778, 778, 778, 778, 778, 944, 500,
    722, 778, 778, 500, 500, 722, 722, 722, 778, 778, 778, 778, 778, 750, 1000, 1000, 833, 611,
    611, 611, 722, 722, 722, 778, 778, 778, 778, 778, 667, 667, 760, 760, 778, 778, 778, 778,
    778, 778, 1333, 1333, 500, 500, 947, 902, 667, 778, 778, 778, 500, 500, 833, 500, 556, 778,
    778, 778,
]);

pub static MSBM10_WIDTHS: AlignedTable<128> = AlignedTable([
    778, 778, 778, 778, 778, 778, 778, 778, 778, 778, 778, 778, 778, 778, 778, 778, 778, 778,
    778, 778, 778, 778, 778, 778, 778, 778, 778, 778, 778, 778, 889, 889, 778, 778, 778, 778,
    778, 778, 778, 778, 778, 778, 778, 778, 500, 278, 222, 389, 611, 722, 611, 722, 778, 778,
    778, 778, 1000, 1000, 1000, 1000, 1000, 1000, 778, 778, 556, 722, 667, 722, 722, 667, 611, 778,
    778, 389, 500, 778, 667, 944, 722, 778, 611, 778, 722, 556, 667, 722, 722, 1000, 722, 722,
    667, 1889, 2333, 1889, 2333, 0, 556, 639, 0, 0, 0, 0, 722, 556, 778, 667, 444, 667,
    778, 778, 778, 778, 222, 389, 778, 778, 778, 778, 778, 778, 1000, 1000, 778, 667, 556, 540,
    540, 429,
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
    549, 0, 439, 490, 631, 549, 549, 534, 500, 500, 556, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
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
        'φ' => b'j', // varphi (\u{03C6})
        '\u{03D5}' => b'f', // phi (\u{03D5})
        '\u{03D1}' => b'J', // vartheta
        '\u{03D6}' => b'v', // varpi
        '\u{03F1}' => b'r', // varrho
        '\u{03F5}' => b'e', // lunate epsilon
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
        '′' => 0xA2,
        '″' => 0xB2,
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
        '√' => 0xC3,
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
    if let Some(byte) = unicode_to_symbol_byte(character) {
        SYMBOL_WIDTHS[byte as usize]
    } else {
        let character_code = character as u32;
        if character_code < 256 {
            SYMBOL_WIDTHS[character_code as usize]
        } else {
            500
        }
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
pub static WIDTH_TABLE_MAP: [&AlignedTable<128>; 72] = [
    // Times (0..17)
    &TIMES_ROMAN_WIDTHS, &TIMES_BOLD_WIDTHS, &TIMES_ITALIC_WIDTHS, &TIMES_BOLD_ITALIC_WIDTHS,
    &COURIER_WIDTHS, &COURIER_WIDTHS, &TIMES_ROMAN_WIDTHS, &TIMES_ROMAN_WIDTHS,
    &TIMES_ROMAN_WIDTHS, &TIMES_ITALIC_WIDTHS, &TIMES_ROMAN_WIDTHS, &TIMES_ROMAN_WIDTHS,
    &TIMES_ROMAN_WIDTHS, &TIMES_ROMAN_WIDTHS, &TIMES_ROMAN_WIDTHS, &TIMES_ROMAN_WIDTHS,
    &TIMES_ROMAN_WIDTHS, &TIMES_ROMAN_WIDTHS,

    // Helvetica (18..35)
    &HELVETICA_ROMAN_WIDTHS, &HELVETICA_BOLD_WIDTHS, &HELVETICA_ROMAN_WIDTHS, &HELVETICA_BOLD_WIDTHS,
    &COURIER_WIDTHS, &COURIER_WIDTHS, &HELVETICA_ROMAN_WIDTHS, &HELVETICA_ROMAN_WIDTHS,
    &HELVETICA_ROMAN_WIDTHS, &HELVETICA_ROMAN_WIDTHS, &HELVETICA_ROMAN_WIDTHS, &HELVETICA_ROMAN_WIDTHS,
    &HELVETICA_ROMAN_WIDTHS, &HELVETICA_ROMAN_WIDTHS, &HELVETICA_ROMAN_WIDTHS, &HELVETICA_ROMAN_WIDTHS,
    &HELVETICA_ROMAN_WIDTHS, &HELVETICA_ROMAN_WIDTHS,

    // Palatino (36..53)
    &PALATINO_ROMAN_WIDTHS, &PALATINO_BOLD_WIDTHS, &PALATINO_ITALIC_WIDTHS, &PALATINO_BOLD_ITALIC_WIDTHS,
    &COURIER_WIDTHS, &COURIER_WIDTHS, &PALATINO_ROMAN_WIDTHS, &PALATINO_ROMAN_WIDTHS,
    &PALATINO_ROMAN_WIDTHS, &PALATINO_ITALIC_WIDTHS, &PALATINO_ROMAN_WIDTHS, &PALATINO_ROMAN_WIDTHS,
    &PALATINO_ROMAN_WIDTHS, &PALATINO_ROMAN_WIDTHS, &PALATINO_ROMAN_WIDTHS, &PALATINO_ROMAN_WIDTHS,
    &PALATINO_ROMAN_WIDTHS, &PALATINO_ROMAN_WIDTHS,

    // ComputerModern (54..71)
    &CMR10_WIDTHS, &CMBX10_WIDTHS, &CMTI10_WIDTHS, &CMBX10_WIDTHS,
    &CMTT10_WIDTHS, &CMTT10_WIDTHS, &CMSY10_WIDTHS, &CMR10_WIDTHS,
    &CMEX10_WIDTHS, &CMMI10_WIDTHS, &CMR7_WIDTHS, &CMR5_WIDTHS,
    &CMMI7_WIDTHS, &CMMI5_WIDTHS, &CMSY7_WIDTHS, &CMSY5_WIDTHS,
    &MSAM10_WIDTHS, &MSBM10_WIDTHS,
];

pub(crate) const fn widths_from_metrics(m: &[crate::math::math_tfm::PackedGlyphMetric; 128]) -> AlignedTable<128> {
    let mut w = [0u16; 128];
    let mut i = 0;
    while i < 128 {
        w[i] = m[i].width_units();
        i += 1;
    }
    AlignedTable(w)
}

/// PDF `/Widths` (1000 units per em of the design size) for the faces past `Face::AmsB`,
/// derived at compile time from their TFM metric tables (`math_tfm_faces`).
pub static EXTRA_FACE_WIDTHS: [AlignedTable<128>; Face::COUNT - 18] = {
    let mut out = [AlignedTable([0; 128]); Face::COUNT - 18];
    let mut i = 0;
    while i < out.len() {
        out[i] = widths_from_metrics(EXTRA_FACES[i].metrics);
        i += 1;
    }
    out
};

/// Everything the pipeline needs about one face past `Face::AmsB`, in one row: adding a
/// font is adding a `Face` variant and its row (`scripts/fontgen/gen_one.py` makes the
/// statics the row points at).
pub(crate) struct ExtraFace {
    /// PDF `/BaseFont`.
    pub(crate) name: &'static str,
    /// Page resource name.
    pub(crate) resource: &'static str,
    /// TFM metrics (w, h, d, ic).
    pub(crate) metrics: &'static [crate::math::math_tfm::PackedGlyphMetric; 128],
    /// The embedded Type 1C program.
    pub(crate) program: &'static crate::backend::backend_cm_fonts::CmFontProgram,
    /// Exact TFM widths and fontdimens of a text font.
    pub(crate) fix: Option<&'static crate::backend::backend_cm_fix::FixMetrics>,
    /// Its own ToUnicode map, for fonts outside the OT1/OML/OMS layouts (TS1, T1).
    pub(crate) tounicode: Option<&'static [u16; 128]>,
    /// Shaped as text: its ligature/kern program and encoding.
    pub(crate) text: Option<(&'static crate::backend::backend_ligkern::LigKernTable, crate::backend::backend_shape::TextEncoding)>,
}

pub(crate) static EXTRA_FACES: [ExtraFace; Face::COUNT - 18] = {
    use crate::backend::backend_cm_fix as fix;
    use crate::backend::backend_cm_fonts as cmf;
    use crate::backend::backend_ligkern as lk;
    use crate::backend::backend_shape::TextEncoding;
    use crate::backend::backend_ts1 as ts1;
    use crate::math::math_tfm_faces as t;
    [
    /* MathSans */ ExtraFace { name: "CMSS10", resource: "/Fss", metrics: &t::CMSS10_METRICS, program: &cmf::CMSS10_PROGRAM, fix: Some(&fix::CMSS10_FIX), tounicode: None, text: Some((&lk::CMSS10, TextEncoding::Ot1)) },
    /* MathSans8 */ ExtraFace { name: "CMSS8", resource: "/Fss8", metrics: &t::CMSS8_METRICS, program: &cmf::CMSS8_PROGRAM, fix: None, tounicode: None, text: None },
    /* MathMono8 */ ExtraFace { name: "CMTT8", resource: "/Ftt8", metrics: &t::CMTT8_METRICS, program: &cmf::CMTT8_PROGRAM, fix: None, tounicode: None, text: None },
    /* Fraktur */ ExtraFace { name: "EUFM10", resource: "/Fk", metrics: &t::EUFM10_METRICS, program: &cmf::EUFM10_PROGRAM, fix: None, tounicode: None, text: None },
    /* Fraktur7 */ ExtraFace { name: "EUFM7", resource: "/Fk7", metrics: &t::EUFM7_METRICS, program: &cmf::EUFM7_PROGRAM, fix: None, tounicode: None, text: None },
    /* Fraktur5 */ ExtraFace { name: "EUFM5", resource: "/Fk5", metrics: &t::EUFM5_METRICS, program: &cmf::EUFM5_PROGRAM, fix: None, tounicode: None, text: None },
    /* BoldMathItalic */ ExtraFace { name: "CMMIB10", resource: "/Fmb", metrics: &t::CMMIB10_METRICS, program: &cmf::CMMIB10_PROGRAM, fix: None, tounicode: None, text: None },
    /* BoldMathItalic7 */ ExtraFace { name: "CMMIB7", resource: "/Fmb7", metrics: &t::CMMIB7_METRICS, program: &cmf::CMMIB7_PROGRAM, fix: None, tounicode: None, text: None },
    /* BoldMathItalic5 */ ExtraFace { name: "CMMIB5", resource: "/Fmb5", metrics: &t::CMMIB5_METRICS, program: &cmf::CMMIB5_PROGRAM, fix: None, tounicode: None, text: None },
    /* BoldSymbol */ ExtraFace { name: "CMBSY10", resource: "/Fsb", metrics: &t::CMBSY10_METRICS, program: &cmf::CMBSY10_PROGRAM, fix: None, tounicode: None, text: None },
    /* BoldSymbol7 */ ExtraFace { name: "CMBSY7", resource: "/Fsb7", metrics: &t::CMBSY7_METRICS, program: &cmf::CMBSY7_PROGRAM, fix: None, tounicode: None, text: None },
    /* BoldSymbol5 */ ExtraFace { name: "CMBSY5", resource: "/Fsb5", metrics: &t::CMBSY5_METRICS, program: &cmf::CMBSY5_PROGRAM, fix: None, tounicode: None, text: None },
    /* Extension7 */ ExtraFace { name: "CMEX7", resource: "/Fe7", metrics: &t::CMEX7_METRICS, program: &cmf::CMEX7_PROGRAM, fix: None, tounicode: None, text: None },
    /* SmallCaps */ ExtraFace { name: "CMCSC10", resource: "/Fsc", metrics: &t::CMCSC10_METRICS, program: &cmf::CMCSC10_PROGRAM, fix: Some(&fix::CMCSC10_FIX), tounicode: None, text: Some((&lk::CMCSC10, TextEncoding::Ot1NoFfSlots)) },
    /* Slanted */ ExtraFace { name: "CMSL10", resource: "/Fsl", metrics: &t::CMSL10_METRICS, program: &cmf::CMSL10_PROGRAM, fix: Some(&fix::CMSL10_FIX), tounicode: None, text: Some((&lk::CMSL10, TextEncoding::Ot1)) },
    /* MathItalic6 */ ExtraFace { name: "CMMI6", resource: "/Fmi6", metrics: &t::CMMI6_METRICS, program: &cmf::CMMI6_PROGRAM, fix: None, tounicode: None, text: None },
    /* MathItalic8 */ ExtraFace { name: "CMMI8", resource: "/Fmi8", metrics: &t::CMMI8_METRICS, program: &cmf::CMMI8_PROGRAM, fix: None, tounicode: None, text: None },
    /* MathItalic9 */ ExtraFace { name: "CMMI9", resource: "/Fmi9", metrics: &t::CMMI9_METRICS, program: &cmf::CMMI9_PROGRAM, fix: None, tounicode: None, text: None },
    /* MathItalic12 */ ExtraFace { name: "CMMI12", resource: "/Fmi12", metrics: &t::CMMI12_METRICS, program: &cmf::CMMI12_PROGRAM, fix: None, tounicode: None, text: None },
    /* Symbol6 */ ExtraFace { name: "CMSY6", resource: "/Fs6", metrics: &t::CMSY6_METRICS, program: &cmf::CMSY6_PROGRAM, fix: None, tounicode: None, text: None },
    /* Symbol8 */ ExtraFace { name: "CMSY8", resource: "/Fs8", metrics: &t::CMSY8_METRICS, program: &cmf::CMSY8_PROGRAM, fix: None, tounicode: None, text: None },
    /* Symbol9 */ ExtraFace { name: "CMSY9", resource: "/Fs9", metrics: &t::CMSY9_METRICS, program: &cmf::CMSY9_PROGRAM, fix: None, tounicode: None, text: None },
    /* SansBold */ ExtraFace { name: "CMSSBX10", resource: "/Fssb", metrics: &t::CMSSBX10_METRICS, program: &cmf::CMSSBX10_PROGRAM, fix: Some(&fix::CMSSBX10_FIX), tounicode: None, text: Some((&lk::CMSSBX10, TextEncoding::Ot1)) },
    /* TextComp */ ExtraFace { name: "SFRM1000", resource: "/Ftc", metrics: &ts1::TCRM1000_METRICS, program: &ts1::TCRM1000_PROGRAM, fix: Some(&ts1::TCRM1000_FIX), tounicode: Some(&ts1::TS1_TOUNICODE), text: None },
    ]
};

/// Resolves the 64-byte aligned ASCII font width table for a given family and face in O(1) branchless time.
#[inline(always)]
pub fn resolve_width_table(family: FontFamily, font_face: Face) -> &'static [u16; 128] {
    if (font_face as usize) >= 18 {
        return &EXTRA_FACE_WIDTHS[font_face as usize - 18].0;
    }
    let idx = (family as usize) * 18 + (font_face as usize);
    if idx < WIDTH_TABLE_MAP.len() {
        &WIDTH_TABLE_MAP[idx].0
    } else {
        &TIMES_ROMAN_WIDTHS.0
    }
}

/// [`resolve_width_table`] for `face` at `size` pt: the design-size master's widths when
/// one applies ([`crate::backend::optical::OpticalMaster::select`]).
#[inline]
pub fn resolve_width_table_at(family: FontFamily, font_face: Face, size: f32) -> &'static [u16; 128] {
    match crate::backend::optical::OpticalMaster::select(family, font_face, size) {
        Some(m) => m.widths(),
        None => resolve_width_table(family, font_face),
    }
}

/// [`resolve_space_units`] for `face` at `size` pt (fontdimen 2 of the design-size master).
#[inline]
pub fn resolve_space_units_at(family: FontFamily, font_face: Face, size: f32) -> f32 {
    match crate::backend::optical::OpticalMaster::select(family, font_face, size) {
        Some(m) => m.glue()[0],
        None => resolve_space_units(family, font_face) as f32,
    }
}

/// [`resolve_extra_space_units`] for `face` at `size` pt (fontdimen 7 of the master).
#[inline]
pub fn resolve_extra_space_units_at(family: FontFamily, font_face: Face, size: f32) -> f32 {
    match crate::backend::optical::OpticalMaster::select(family, font_face, size) {
        Some(m) if !font_face.is_mono() => m.glue()[4],
        Some(_) => 0.0,
        None => resolve_extra_space_units(family, font_face) as f32,
    }
}

/// [`resolve_digit_units`] for `face` at `size` pt.
#[inline]
pub fn resolve_digit_units_at(family: FontFamily, font_face: Face, size: f32) -> u32 {
    match crate::backend::optical::OpticalMaster::select(family, font_face, size) {
        Some(m) => m.widths()[b'0' as usize] as u32,
        None => resolve_digit_units(family, font_face),
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
        FontFamily::Helvetica => 278,
        // fontdimen 2 of cmr10, cmbx10, cmti10 and cmcsc10.
        FontFamily::ComputerModern if font_face == Face::SmallCaps => 378,
        FontFamily::ComputerModern if font_face.is_bold() => 383,
        FontFamily::ComputerModern if font_face.is_italic() => 358,
        FontFamily::ComputerModern => 333,
    }
}

/// Resolves extra space (fontdimen 7) in 1000-units/em space with zero memory lookups.
/// x-height (fontdimen 5, the `ex` unit) in 1/1000 em: the TFM value for Computer Modern
/// (cmr10, cmbx10 and cmtt10 differ by under 1%), the AFM `XHeight` for the Base-14 faces.
#[inline(always)]
pub fn resolve_x_height_units(family: FontFamily, font_face: Face) -> f32 {
    match family {
        FontFamily::ComputerModern if font_face.is_bold() => 444.446,
        FontFamily::ComputerModern => 430.555,
        _ if font_face.is_mono() => 426.0,
        FontFamily::Times => 450.0,
        FontFamily::Helvetica => 523.0,
        FontFamily::Palatino => 469.0,
    }
}

/// According to TeX specifications (TeX by Topic Ch 20), extra space is added to the
/// interword space when space factor >= 2000 (e.g. after sentence-ending punctuation).
#[inline(always)]
pub fn resolve_extra_space_units(family: FontFamily, font_face: Face) -> u32 {
    if font_face.is_mono() {
        return 0;
    }
    match family {
        FontFamily::Times | FontFamily::Palatino => 83,
        FontFamily::Helvetica => 93,
        // fontdimen 7.
        FontFamily::ComputerModern if font_face == Face::SmallCaps => 117,
        FontFamily::ComputerModern => 111,
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
            // cmbx10 digits are 0.575 em.
            FontFamily::ComputerModern if font_face.is_bold() => 575,
            FontFamily::Times | FontFamily::Palatino | FontFamily::ComputerModern => 500,
            FontFamily::Helvetica => 556,
        }
    }
}

#[inline]
pub fn char_units_family(character: char, font_face: Face, family: FontFamily) -> u16 {
    let character_code = character as u32;
    if character_code < 128 && !matches!(font_face, Face::Symbol | Face::Symbol7 | Face::Symbol5 | Face::Dingbats) {
        let table = resolve_width_table(family, font_face);
        return table[character_code as usize];
    }
    if font_face.is_mono() {
        return match family {
            FontFamily::ComputerModern => 525,
            _ => 600,
        };
    }
    if font_face == Face::Extension {
        let code = character as u32;
        if code < 128 {
            return CMEX10_WIDTHS[code as usize];
        }
        return symbol_char_units(character);
    }
    if font_face == Face::Symbol {
        if family == FontFamily::ComputerModern {
            if character == '-' || character == '−' {
                return (crate::math::math_tfm::cmsy10_metric(0x00).w + 0.5) as u16;
            }
            if character == '′' || character == '\'' {
                return (crate::math::math_tfm::cmsy10_metric(0x30).w + 0.5) as u16;
            }
            if let Some(byte) = unicode_to_symbol_byte(character) {
                let code = if byte == 0x2D { 0x00 } else { byte };
                return (crate::math::math_tfm::cmsy10_metric(code).w + 0.5) as u16;
            }
            let code = character as u32;
            if code < 128 {
                let c = if code == 0x2D { 0x00 } else { code as u8 };
                return (crate::math::math_tfm::cmsy10_metric(c).w + 0.5) as u16;
            }
        }
        return symbol_char_units(character);
    }
    if font_face == Face::Symbol7 {
        if character == '-' || character == '−' {
            return (crate::math::math_tfm::cmsy7_metric(0x00).w + 0.5) as u16;
        }
        if character == '′' || character == '\'' {
            return (crate::math::math_tfm::cmsy7_metric(0x30).w + 0.5) as u16;
        }
        if let Some(byte) = unicode_to_symbol_byte(character) {
            let code = if byte == 0x2D { 0x00 } else { byte };
            return (crate::math::math_tfm::cmsy7_metric(code).w + 0.5) as u16;
        }
        let code = character as u32;
        if code < 128 {
            let c = if code == 0x2D { 0x00 } else { code as u8 };
            return (crate::math::math_tfm::cmsy7_metric(c).w + 0.5) as u16;
        }
        return symbol_char_units(character);
    }
    if font_face == Face::Symbol5 {
        if character == '-' || character == '−' {
            return (crate::math::math_tfm::cmsy5_metric(0x00).w + 0.5) as u16;
        }
        if character == '′' || character == '\'' {
            return (crate::math::math_tfm::cmsy5_metric(0x30).w + 0.5) as u16;
        }
        if let Some(byte) = unicode_to_symbol_byte(character) {
            let code = if byte == 0x2D { 0x00 } else { byte };
            return (crate::math::math_tfm::cmsy5_metric(code).w + 0.5) as u16;
        }
        let code = character as u32;
        if code < 128 {
            let c = if code == 0x2D { 0x00 } else { code as u8 };
            return (crate::math::math_tfm::cmsy5_metric(c).w + 0.5) as u16;
        }
        return symbol_char_units(character);
    }
    if font_face == Face::Dingbats {
        return dingbats_char_units(character);
    }
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

/// 8-Way SWAR unrolled fast path: processes 8 ASCII characters per iteration
/// with independent accumulators to exploit CPU superscalar instruction-level parallelism.
#[inline(always)]
pub fn string_units_table(text: &str, table: &[u16; 128]) -> u32 {
    let byte_slice = text.as_bytes();
    let mut acc0: u32 = 0;
    let mut acc1: u32 = 0;
    let mut acc2: u32 = 0;
    let mut acc3: u32 = 0;
    let mut byte_index = 0;
    let len = byte_slice.len();

    // 8-character 64-bit SWAR fast path
    while byte_index + 8 <= len {
        let chunk = u64::from_le_bytes(byte_slice[byte_index..byte_index + 8].try_into().unwrap());
        if (chunk & 0x8080_8080_8080_8080) == 0 {
            let b0 = (chunk & 0x7F) as usize;
            let b1 = ((chunk >> 8) & 0x7F) as usize;
            let b2 = ((chunk >> 16) & 0x7F) as usize;
            let b3 = ((chunk >> 24) & 0x7F) as usize;
            let b4 = ((chunk >> 32) & 0x7F) as usize;
            let b5 = ((chunk >> 40) & 0x7F) as usize;
            let b6 = ((chunk >> 48) & 0x7F) as usize;
            let b7 = ((chunk >> 56) & 0x7F) as usize;

            acc0 += table[b0] as u32;
            acc1 += table[b1] as u32;
            acc2 += table[b2] as u32;
            acc3 += table[b3] as u32;
            acc0 += table[b4] as u32;
            acc1 += table[b5] as u32;
            acc2 += table[b6] as u32;
            acc3 += table[b7] as u32;
            byte_index += 8;
        } else {
            // Non-ASCII fallback for first character in chunk
            let b0 = byte_slice[byte_index];
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

    // 4-character 32-bit SWAR path
    if byte_index + 4 <= len {
        let chunk = u32::from_le_bytes(byte_slice[byte_index..byte_index + 4].try_into().unwrap());
        if (chunk & 0x8080_8080) == 0 {
            let b0 = (chunk & 0x7F) as usize;
            let b1 = ((chunk >> 8) & 0x7F) as usize;
            let b2 = ((chunk >> 16) & 0x7F) as usize;
            let b3 = ((chunk >> 24) & 0x7F) as usize;

            acc0 += table[b0] as u32;
            acc1 += table[b1] as u32;
            acc2 += table[b2] as u32;
            acc3 += table[b3] as u32;
            byte_index += 4;
        }
    }

    // Scalar remainder loop for trailing 1..3 bytes
    while byte_index < len {
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
        let mono_unit = match family {
            FontFamily::ComputerModern => 525,
            _ => 600,
        };
        let byte_slice = text.as_bytes();
        let len = byte_slice.len();
        let mut idx = 0;
        let mut is_all_ascii = true;
        while idx + 8 <= len {
            let chunk = u64::from_le_bytes(byte_slice[idx..idx + 8].try_into().unwrap());
            if (chunk & 0x8080_8080_8080_8080) != 0 {
                is_all_ascii = false;
                break;
            }
            idx += 8;
        }
        if is_all_ascii {
            while idx < len {
                if byte_slice[idx] >= 128 {
                    is_all_ascii = false;
                    break;
                }
                idx += 1;
            }
        }
        let char_count = if is_all_ascii {
            len as u32
        } else {
            text.chars().count() as u32
        };
        return char_count * mono_unit;
    }
    if font_face == Face::Extension {
        return string_units_table(text, &CMEX10_WIDTHS.0);
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
