//! # Adobe Font Metrics Engine (`metrics`)
//!
//! Provides compile-time Adobe Font Metrics (AFM) for the standard PDF Type 1
//! Helvetica typeface family at 1000 units per em (WinAnsi encoding).
//!
//! ## Architectural Design
//!
//! - **Zero Heap Allocations**: Font width tables for ASCII 0..=127 are precomputed and stored
//!   directly in `.rodata` as flat `[u16; 128]` arrays (`ROMAN_WIDTHS`, `BOLD_WIDTHS`).
//! - **Branchless Lookups**: Direct array indexing for ASCII characters avoids branch mispredictions.
//! - **Single Multiplication Measurement**: `string_units` accumulates integer font units in a tight
//!   byte loop, performing only one floating-point multiplication at the end.
//! - **Oblique Width Equivalence**: In Adobe Type 1 typography, Helvetica-Oblique and
//!   Helvetica-BoldOblique share identical horizontal character advances with Roman and Bold.
//!
//! ## Public APIs
//!
//! - [`Face`]: Enum representing the 4 standard typographic styles (`Roman`, `Bold`, `Italic`, `BoldItalic`).
//! - [`char_units`]: Returns the raw metric width of a single character in 1000-units/em space.
//! - [`width`]: Returns the typographic width of a character in PDF points (`pt`) for a given font size.
//! - [`string_units`]: Computes the sum of metric units for an entire text slice in a fast ASCII loop.
//! - [`string_width`]: Calculates total rendered width of a text string in PDF points (`pt`).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Roman,
    Bold,
    Italic,
    BoldItalic,
}

impl Face {
    pub fn pdf_name(self) -> &'static str {
        match self {
            Face::Roman => "Helvetica",
            Face::Bold => "Helvetica-Bold",
            Face::Italic => "Helvetica-Oblique",
            Face::BoldItalic => "Helvetica-BoldOblique",
        }
    }

    pub fn resource(self) -> &'static str {
        match self {
            Face::Roman => "/Fr",
            Face::Bold => "/Fb",
            Face::Italic => "/Fi",
            Face::BoldItalic => "/Fbi",
        }
    }

    #[inline]
    pub fn with_bold(self, enable_bold: bool) -> Self {
        match (self, enable_bold) {
            (Face::Roman, true) | (Face::Bold, true) => Face::Bold,
            (Face::Italic, true) | (Face::BoldItalic, true) => Face::BoldItalic,
            (Face::Bold, false) | (Face::Roman, false) => Face::Roman,
            (Face::BoldItalic, false) | (Face::Italic, false) => Face::Italic,
        }
    }

    #[inline]
    pub fn with_italic(self, enable_italic: bool) -> Self {
        match (self, enable_italic) {
            (Face::Roman, true) | (Face::Italic, true) => Face::Italic,
            (Face::Bold, true) | (Face::BoldItalic, true) => Face::BoldItalic,
            (Face::Italic, false) | (Face::Roman, false) => Face::Roman,
            (Face::BoldItalic, false) | (Face::Bold, false) => Face::Bold,
        }
    }

    #[inline]
    pub fn is_bold(self) -> bool {
        matches!(self, Face::Bold | Face::BoldItalic)
    }
}

// Flat compile-time lookup tables for ASCII 0..=127 in .rodata
pub static ROMAN_WIDTHS: [u16; 128] = [
    // 0..=31 (control characters, default to space width 278 for whitespace like tab, else 0)
    0, 0, 0, 0, 0, 0, 0, 0, 0, 278, 278, 0, 0, 278, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    // 32 (' ') ..= 47 ('/')
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
];

pub static BOLD_WIDTHS: [u16; 128] = [
    // 0..=31
    0, 0, 0, 0, 0, 0, 0, 0, 0, 278, 278, 0, 0, 278, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    // 32 (' ') ..= 47 ('/')
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
];

#[inline]
pub fn char_units(character: char, font_face: Face) -> u16 {
    let character_code = character as u32;
    if character_code < 128 {
        if font_face.is_bold() {
            BOLD_WIDTHS[character_code as usize]
        } else {
            ROMAN_WIDTHS[character_code as usize]
        }
    } else {
        if font_face.is_bold() {
            bold_units_cold(character)
        } else {
            roman_units_cold(character)
        }
    }
}

/// Computes the width of `character` in PDF text space for `font_face` at `font_size_points`.
#[inline]
pub fn width(character: char, font_face: Face, font_size_points: f32) -> f32 {
    char_units(character, font_face) as f32 * (font_size_points / 1000.0)
}

/// Fast branchless sum of font metric units for `text`.
pub fn string_units(text: &str, font_face: Face) -> u32 {
    let byte_slice = text.as_bytes();
    let is_bold = font_face.is_bold();
    let width_table = if is_bold { &BOLD_WIDTHS } else { &ROMAN_WIDTHS };
    let mut accumulated_units: u32 = 0;
    let mut byte_index = 0;
    while byte_index < byte_slice.len() {
        let byte_value = byte_slice[byte_index];
        if byte_value < 128 {
            accumulated_units += width_table[byte_value as usize] as u32;
            byte_index += 1;
        } else {
            let remaining_text = &text[byte_index..];
            let character = remaining_text.chars().next().unwrap();
            byte_index += character.len_utf8();
            accumulated_units += if is_bold {
                bold_units_cold(character) as u32
            } else {
                roman_units_cold(character) as u32
            };
        }
    }
    accumulated_units
}

/// Computes width of `text` in PDF points with a single floating-point multiplication.
#[inline]
pub fn string_width(text: &str, font_face: Face, font_size_points: f32) -> f32 {
    string_units(text, font_face) as f32 * (font_size_points / 1000.0)
}

#[cold]
fn roman_units_cold(character: char) -> u16 {
    match character {
        '\u{00A0}' => 278,
        '–' | '—' => 556,
        '‘' | '’' => 191,
        '“' | '”' => 355,
        '…' => 834,
        '•' => 350,
        _ => 556,
    }
}

#[cold]
fn bold_units_cold(character: char) -> u16 {
    match character {
        '\u{00A0}' => 278,
        '–' | '—' => 556,
        '‘' | '’' => 238,
        '“' | '”' => 474,
        '…' => 834,
        '•' => 350,
        _ => 611,
    }
}
