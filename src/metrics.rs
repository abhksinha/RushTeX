//! Adobe Font Metrics for Helvetica / Helvetica-Bold (WinAnsi, 1000 units/em).
//! Italic/Oblique shares the Roman widths (true for Helvetica-Oblique).

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
    pub fn with_bold(self, on: bool) -> Self {
        match (self, on) {
            (Face::Roman, true) | (Face::Bold, true) => Face::Bold,
            (Face::Italic, true) | (Face::BoldItalic, true) => Face::BoldItalic,
            (Face::Bold, false) | (Face::Roman, false) => Face::Roman,
            (Face::BoldItalic, false) | (Face::Italic, false) => Face::Italic,
        }
    }

    #[inline]
    pub fn with_italic(self, on: bool) -> Self {
        match (self, on) {
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
pub fn char_units(ch: char, face: Face) -> u16 {
    let c = ch as u32;
    if c < 128 {
        if face.is_bold() {
            BOLD_WIDTHS[c as usize]
        } else {
            ROMAN_WIDTHS[c as usize]
        }
    } else {
        if face.is_bold() {
            bold_units_cold(ch)
        } else {
            roman_units_cold(ch)
        }
    }
}

/// Width of `ch` in PDF text space for `face` at `size` points.
#[inline]
pub fn width(ch: char, face: Face, size: f32) -> f32 {
    char_units(ch, face) as f32 * (size / 1000.0)
}

/// Fast branchless sum of font metric units for string `s`.
pub fn string_units(s: &str, face: Face) -> u32 {
    let bytes = s.as_bytes();
    let is_bold = face.is_bold();
    let table = if is_bold { &BOLD_WIDTHS } else { &ROMAN_WIDTHS };
    let mut total: u32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < 128 {
            total += table[b as usize] as u32;
            i += 1;
        } else {
            let rest = &s[i..];
            let c = rest.chars().next().unwrap();
            i += c.len_utf8();
            total += if is_bold {
                bold_units_cold(c) as u32
            } else {
                roman_units_cold(c) as u32
            };
        }
    }
    total
}

/// Computes width of string `s` in PDF points with a single floating-point multiplication.
#[inline]
pub fn string_width(s: &str, face: Face, size: f32) -> f32 {
    string_units(s, face) as f32 * (size / 1000.0)
}

#[cold]
fn roman_units_cold(ch: char) -> u16 {
    match ch {
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
fn bold_units_cold(ch: char) -> u16 {
    match ch {
        '\u{00A0}' => 278,
        '–' | '—' => 556,
        '‘' | '’' => 238,
        '“' | '”' => 474,
        '…' => 834,
        '•' => 350,
        _ => 611,
    }
}
