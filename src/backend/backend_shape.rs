//! # Text shaping (`shape`)
//!
//! Applies a text font's ligature/kern program to a word, the way TeX's main control loop
//! does while it builds character nodes (`tex.web` §1034–§1040): adjacent characters of one
//! font that form a ligature are replaced by the ligature glyph (which may ligature again with
//! the next character, e.g. `f`+`f`→`ff`, `ff`+`i`→`ffi`, `-`+`-`→`–`, `–`+`-`→`—`), and an
//! implicit kern is inserted between characters the font kerns.
//!
//! Layout measures words with [`TextFont::width_units`] and the PDF backend emits them with
//! [`TextFont::shape`], so the advance of every emitted run equals its measured width.
//!
//! - Computer Modern text faces use the TFM programs of [`crate::backend::backend_ligkern`];
//!   glyphs are addressed by their OT1 font position.
//! - Base-14 text faces (Times, Helvetica) use the AFM `KPX`/`L` data of
//!   [`crate::backend::backend_afm_ligkern`] plus TeX's standard text ligatures; glyphs are
//!   addressed by WinAnsi code, with TeX's convention that source `'`/`` ` `` are the curly
//!   quotes.
//!
//! Faces without a program here (typewriter, Palatino, symbol/math faces) are not shaped.

#![forbid(unsafe_code)]

use crate::backend::backend_afm_ligkern as afm;
use crate::backend::backend_fonts::{self as metrics, Face, FontFamily};
use crate::backend::backend_ligkern::{self as cm, LigKernTable};

/// How a font addresses its glyphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextEncoding {
    /// OT1 (Computer Modern text fonts with the `ff`…`ffl` ligature slots 0x0B–0x0F).
    Ot1,
    /// OT1 layout of `cmr5`, whose slots 0x0B–0x0F hold arrows/quotes instead of ligatures.
    Ot1NoFfSlots,
    /// WinAnsiEncoding, plus `fi`/`fl` in the undefined slots 0x81/0x8D.
    WinAnsi,
}

/// One output glyph of a shaped word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    /// A glyph addressed by its code in the font's encoding.
    Code(u8),
    /// A character the font program does not cover; it interrupts ligatures and kerns and is
    /// emitted and measured exactly as unshaped text.
    Raw(char),
}

/// A text face together with its ligature/kern program and code-indexed advance widths.
#[derive(Clone, Copy)]
pub struct TextFont {
    program: &'static LigKernTable,
    widths: &'static [u16],
    /// Exact TFM widths, when the font has them (Computer Modern).
    fix: Option<&'static crate::backend::backend_cm_fix::FixMetrics>,
    encoding: TextEncoding,
    face: Face,
    family: FontFamily,
}

/// Returns the shaping font for `face` in `family`, or `None` when the face is not shaped.
#[inline]
pub fn text_font(family: FontFamily, face: Face) -> Option<TextFont> {
    let (program, widths, encoding): (&'static LigKernTable, &'static [u16], TextEncoding) =
        match (family, face) {
            (FontFamily::ComputerModern, Face::Roman) => {
                (&cm::CMR10, &metrics::CMR10_WIDTHS.0, TextEncoding::Ot1)
            }
            (FontFamily::ComputerModern, Face::Bold) => {
                (&cm::CMBX10, &metrics::CMBX10_WIDTHS.0, TextEncoding::Ot1)
            }
            (FontFamily::ComputerModern, Face::Italic) => {
                (&cm::CMTI10, &metrics::CMTI10_WIDTHS.0, TextEncoding::Ot1)
            }
            (FontFamily::ComputerModern, Face::Roman7) => {
                (&cm::CMR7, &metrics::CMR7_WIDTHS.0, TextEncoding::Ot1)
            }
            (FontFamily::ComputerModern, Face::Roman5) => {
                (&cm::CMR5, &metrics::CMR5_WIDTHS.0, TextEncoding::Ot1NoFfSlots)
            }
            // cmcsc10 has no f-ligatures; slots 0x0B–0x0F hold arrows and quotes as in cmr5.
            (FontFamily::ComputerModern, Face::SmallCaps) => (
                &cm::CMCSC10,
                &metrics::EXTRA_FACE_WIDTHS[Face::SmallCaps as usize - 18].0,
                TextEncoding::Ot1NoFfSlots,
            ),
            (FontFamily::Times, Face::Roman) => {
                (&afm::TIMES_ROMAN, &afm::TIMES_ROMAN_WIDTHS, TextEncoding::WinAnsi)
            }
            (FontFamily::Times, Face::Bold) => {
                (&afm::TIMES_BOLD, &afm::TIMES_BOLD_WIDTHS, TextEncoding::WinAnsi)
            }
            (FontFamily::Times, Face::Italic) => {
                (&afm::TIMES_ITALIC, &afm::TIMES_ITALIC_WIDTHS, TextEncoding::WinAnsi)
            }
            (FontFamily::Times, Face::BoldItalic) => (
                &afm::TIMES_BOLD_ITALIC,
                &afm::TIMES_BOLD_ITALIC_WIDTHS,
                TextEncoding::WinAnsi,
            ),
            (FontFamily::Helvetica, Face::Roman) => {
                (&afm::HELVETICA, &afm::HELVETICA_WIDTHS, TextEncoding::WinAnsi)
            }
            (FontFamily::Helvetica, Face::Bold) => {
                (&afm::HELVETICA_BOLD, &afm::HELVETICA_BOLD_WIDTHS, TextEncoding::WinAnsi)
            }
            (FontFamily::Helvetica, Face::Italic) => (
                &afm::HELVETICA_OBLIQUE,
                &afm::HELVETICA_OBLIQUE_WIDTHS,
                TextEncoding::WinAnsi,
            ),
            (FontFamily::Helvetica, Face::BoldItalic) => (
                &afm::HELVETICA_BOLD_OBLIQUE,
                &afm::HELVETICA_BOLD_OBLIQUE_WIDTHS,
                TextEncoding::WinAnsi,
            ),
            _ => return None,
        };
    Some(TextFont {
        program,
        widths,
        fix: crate::backend::scaled::fix_metrics(family, face, 10.0),
        encoding,
        face,
        family,
    })
}

/// [`text_font`] for `face` at `size` pt: the design-size master's program and widths when
/// one applies.
#[inline]
pub fn text_font_at(family: FontFamily, face: Face, size: f32) -> Option<TextFont> {
    match crate::backend::optical::OpticalMaster::select(family, face, size) {
        Some(m) => Some(TextFont {
            program: m.ligkern()?,
            widths: m.widths(),
            fix: Some(m.fix()),
            encoding: if m.no_ff_slots() { TextEncoding::Ot1NoFfSlots } else { TextEncoding::Ot1 },
            face,
            family,
        }),
        None => text_font(family, face),
    }
}

/// Width of a word of `face` at `size` pt in points, as TeX measures it: shaped with
/// exact TFM widths for Computer Modern (unshaped typewriter faces too), AFM widths
/// otherwise.
#[inline]
pub fn word_width_pt(text: &str, face: Face, size: f32, family: FontFamily) -> f32 {
    if let Some(font) = text_font_at(family, face, size) {
        return font.width_pt(text, size);
    }
    if let Some(fix) = crate::backend::scaled::fix_metrics(family, face, size) {
        if text.is_ascii() {
            let scaler = crate::backend::scaled::Scaler::at_pt(size);
            let sp: i64 = text.bytes().map(|b| scaler.scale(fix.widths[b as usize] as i32) as i64).sum();
            return crate::backend::scaled::to_pt(sp);
        }
    }
    text_units_at(text, face, size, family) * size * 0.001
}

/// [`text_units`] for `face` at `size` pt.
#[inline]
pub fn text_units_at(text: &str, face: Face, size: f32, family: FontFamily) -> f32 {
    match text_font_at(family, face, size) {
        Some(font) => font.width_units(text),
        None => metrics::string_units_table(text, metrics::resolve_width_table_at(family, face, size)) as f32,
    }
}

/// Width of a word in 1000-units/em, with ligatures and kerns when the face is shaped.
#[inline]
pub fn text_units(text: &str, face: Face, family: FontFamily) -> f32 {
    match text_font(family, face) {
        Some(font) => font.width_units(text),
        None => metrics::string_units_family(text, face, family) as f32,
    }
}

/// Whether a PDF font dictionary for `face` in `family` must carry the WinAnsi `/Differences`
/// that give the `fi`/`fl` ligatures their slots.
#[inline]
pub fn needs_ligature_encoding(family: FontFamily, face: Face) -> bool {
    matches!(family, FontFamily::Times | FontFamily::Helvetica)
        && matches!(face, Face::Roman | Face::Bold | Face::Italic | Face::BoldItalic)
}

impl TextFont {
    /// Code of `c` in this font's encoding, or `None` when the program does not cover it.
    #[inline(always)]
    fn code(&self, c: char) -> Option<u8> {
        let cp = c as u32;
        if cp < 0x80 {
            return self.ascii_code(cp as u8);
        }
        match self.encoding {
            TextEncoding::WinAnsi => winansi_code(c),
            TextEncoding::Ot1 => ot1_code(c, true),
            TextEncoding::Ot1NoFfSlots => ot1_code(c, false),
        }
    }

    /// Code of an ASCII byte (`None` for control characters).
    #[inline(always)]
    fn ascii_code(&self, b: u8) -> Option<u8> {
        let map = if self.encoding == TextEncoding::WinAnsi {
            &WINANSI_ASCII
        } else {
            &OT1_ASCII
        };
        match map[(b & 0x7F) as usize] {
            0 => None,
            code => Some(code),
        }
    }

    /// Runs the ligature/kern program over `text` (a word without spaces), calling
    /// `sink(glyph, kern)` for every output glyph, where `kern` is the implicit kern that
    /// follows the glyph, in 1/10000 em. Allocation-free.
    #[inline]
    pub fn shape<F: FnMut(Glyph, i32)>(&self, text: &str, mut sink: F) {
        // TFM ligature programs here only use `=:` (op 0): the pair is replaced by the
        // ligature, which stays current and may combine with the following character.
        let mut pending: Option<u8> = None;
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            let (c, code) = if b < 0x80 {
                i += 1;
                (b as char, self.ascii_code(b))
            } else {
                // Non-ASCII: decode one scalar (cold for typical body text).
                let c = text[i..].chars().next().unwrap_or('\u{FFFD}');
                i += c.len_utf8();
                (c, self.code(c))
            };
            match code {
                Some(code) => {
                    if let Some(left) = pending {
                        if let Some((_, lig)) = self.program.lig(left, code) {
                            pending = Some(lig);
                            continue;
                        }
                        sink(Glyph::Code(left), self.program.kern_units(left, code));
                    }
                    pending = Some(code);
                }
                None => {
                    if let Some(left) = pending.take() {
                        sink(Glyph::Code(left), 0);
                    }
                    sink(Glyph::Raw(c), 0);
                }
            }
        }
        if let Some(left) = pending {
            sink(Glyph::Code(left), 0);
        }
    }

    /// Shaped width of `text` in 1000-units/em (fractional: kerns are finer than 1/1000 em).
    #[inline]
    pub fn width_units(&self, text: &str) -> f32 {
        let mut tenths: i32 = 0;
        let mut raw: u32 = 0;
        self.shape(text, |g, kern| {
            match g {
                Glyph::Code(code) => tenths += self.glyph_units(code) as i32 * 10,
                Glyph::Raw(c) => raw += metrics::char_units_family(c, self.face, self.family) as u32,
            }
            tenths += kern;
        });
        tenths as f32 * 0.1 + raw as f32
    }

    /// Shaped width of `text` at `size` pt, in scaled points. Computer Modern glyph widths
    /// are the TFM fix_words scaled as TeX scales them; kerns keep their 1/10000 em.
    #[inline]
    pub fn width_sp(&self, text: &str, size: f32) -> i32 {
        let Some(fix) = self.fix else {
            return crate::backend::scaled::to_sp(self.width_units(text) * size * 0.001);
        };
        let scaler = crate::backend::scaled::Scaler::at_pt(size);
        let size_sp = crate::backend::scaled::to_sp(size) as i64;
        let mut sp: i64 = 0;
        let mut kern_tenths: i64 = 0;
        let mut raw: u32 = 0;
        self.shape(text, |g, kern| {
            match g {
                Glyph::Code(code) => sp += scaler.scale(fix.widths[code as usize & 127] as i32) as i64,
                Glyph::Raw(c) => raw += metrics::char_units_family(c, self.face, self.family) as u32,
            }
            kern_tenths += kern as i64;
        });
        (sp + (kern_tenths * size_sp + 5000).div_euclid(10000) + (raw as i64 * size_sp + 500) / 1000) as i32
    }

    /// [`Self::width_sp`] in points.
    #[inline]
    pub fn width_pt(&self, text: &str, size: f32) -> f32 {
        crate::backend::scaled::to_pt(self.width_sp(text, size) as i64)
    }

    /// Advance width of glyph `code` in 1000-units/em.
    #[inline(always)]
    pub fn glyph_units(&self, code: u8) -> u16 {
        self.widths.get(code as usize).copied().unwrap_or(0)
    }
}

/// ASCII byte -> OT1 code (identity for printable ASCII; 0 = not covered).
static OT1_ASCII: [u8; 128] = ascii_map(false);
/// ASCII byte -> WinAnsi code, with TeX's `'`/`` ` `` as quoteright/quoteleft.
static WINANSI_ASCII: [u8; 128] = ascii_map(true);

const fn ascii_map(winansi: bool) -> [u8; 128] {
    let mut m = [0u8; 128];
    let mut b = 0x20;
    while b < 0x7F {
        m[b] = b as u8;
        b += 1;
    }
    if winansi {
        m[b'\'' as usize] = 0x92;
        m[b'`' as usize] = 0x91;
    }
    m
}

/// OT1 position of a non-ASCII character in the Computer Modern text fonts.
#[inline]
fn ot1_code(c: char, has_ff_slots: bool) -> Option<u8> {
    Some(match c {
        '–' => 0x7B,
        '—' => 0x7C,
        '‘' => 0x60,
        '’' => 0x27,
        '“' => 0x5C,
        '”' => 0x22,
        '¡' => 0x3C,
        '¿' => 0x3E,
        'ı' => 0x10,
        'ȷ' => 0x11,
        'ß' => 0x19,
        'æ' => 0x1A,
        'œ' => 0x1B,
        'ø' => 0x1C,
        'Æ' => 0x1D,
        'Œ' => 0x1E,
        'Ø' => 0x1F,
        'ﬀ' if has_ff_slots => 0x0B,
        'ﬁ' if has_ff_slots => 0x0C,
        'ﬂ' if has_ff_slots => 0x0D,
        'ﬃ' if has_ff_slots => 0x0E,
        'ﬄ' if has_ff_slots => 0x0F,
        _ => return None,
    })
}

/// WinAnsiEncoding code of a non-ASCII character (`fi`/`fl` in their ligature slots).
#[inline]
fn winansi_code(c: char) -> Option<u8> {
    let cp = c as u32;
    if (0xA0..=0xFF).contains(&cp) {
        return Some(cp as u8);
    }
    Some(match c {
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8A,
        '‹' => 0x8B,
        'Œ' => 0x8C,
        'Ž' => 0x8E,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9A,
        '›' => 0x9B,
        'œ' => 0x9C,
        'ž' => 0x9E,
        'Ÿ' => 0x9F,
        'ﬁ' => afm::WINANSI_FI,
        'ﬂ' => afm::WINANSI_FL,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyphs(font: &TextFont, text: &str) -> Vec<(Glyph, i32)> {
        let mut out = Vec::new();
        font.shape(text, |g, k| out.push((g, k)));
        out
    }

    #[test]
    fn cm_ligatures_chain_like_tex() {
        let f = text_font(FontFamily::ComputerModern, Face::Roman).unwrap();
        let g = glyphs(&f, "office");
        let codes: Vec<Glyph> = g.iter().map(|e| e.0).collect();
        assert_eq!(
            codes,
            vec![Glyph::Code(b'o'), Glyph::Code(0x0E), Glyph::Code(b'c'), Glyph::Code(b'e')]
        );
        let dash: Vec<Glyph> = glyphs(&f, "a---b").iter().map(|e| e.0).collect();
        assert_eq!(dash, vec![Glyph::Code(b'a'), Glyph::Code(0x7C), Glyph::Code(b'b')]);
        let en: Vec<Glyph> = glyphs(&f, "--").iter().map(|e| e.0).collect();
        assert_eq!(en, vec![Glyph::Code(0x7B)]);
        let q: Vec<Glyph> = glyphs(&f, "``x''").iter().map(|e| e.0).collect();
        assert_eq!(q, vec![Glyph::Code(0x5C), Glyph::Code(b'x'), Glyph::Code(0x22)]);
    }

    #[test]
    fn cm_kerns_match_tfm() {
        let f = text_font(FontFamily::ComputerModern, Face::Roman).unwrap();
        assert_eq!(glyphs(&f, "AV")[0], (Glyph::Code(b'A'), -1111));
        assert_eq!(glyphs(&f, "od")[0], (Glyph::Code(b'o'), 278));
        // A V A T A R: AV -1111, VA -1111, AT -833, TA -833, AR none
        let w = f.width_units("AVATAR");
        let plain: u32 = "AVATAR".bytes().map(|b| metrics::CMR10_WIDTHS.0[b as usize] as u32).sum();
        assert!((w - (plain as f32 - 388.8)).abs() < 0.01, "{w} vs {plain}");
        // ffi ligature is narrower than f + f + i
        assert_eq!(f.width_units("ffi"), 833.0);
    }

    #[test]
    fn non_program_chars_interrupt_shaping() {
        let f = text_font(FontFamily::ComputerModern, Face::Roman).unwrap();
        let g = glyphs(&f, "f\u{e9}i");
        assert_eq!(g, vec![(Glyph::Code(b'f'), 0), (Glyph::Raw('é'), 0), (Glyph::Code(b'i'), 0)]);
        let u = glyphs(&f, "\u{201c}x\u{2014}");
        assert_eq!(u[0].0, Glyph::Code(0x5C));
        assert_eq!(u[2].0, Glyph::Code(0x7C));
    }

    #[test]
    fn base14_afm_kerns_and_ligatures() {
        let f = text_font(FontFamily::Times, Face::Roman).unwrap();
        // Times-Roman.afm: KPX A V -135, ligature f i -> fi
        assert_eq!(glyphs(&f, "AV")[0], (Glyph::Code(b'A'), -1350));
        let fi: Vec<Glyph> = glyphs(&f, "fit").iter().map(|e| e.0).collect();
        assert_eq!(fi, vec![Glyph::Code(afm::WINANSI_FI), Glyph::Code(b't')]);
        assert_eq!(f.width_units("fi"), 556.0);
        // TeX quotes and dashes
        let q: Vec<Glyph> = glyphs(&f, "``a''--b").iter().map(|e| e.0).collect();
        assert_eq!(
            q,
            vec![Glyph::Code(0x93), Glyph::Code(b'a'), Glyph::Code(0x94), Glyph::Code(0x96), Glyph::Code(b'b')]
        );
        assert!(text_font(FontFamily::Times, Face::Mono).is_none());
        assert!(text_font(FontFamily::Palatino, Face::Roman).is_none());
    }

    #[test]
    fn base14_ascii_widths_agree_with_metric_tables() {
        for (family, face) in [
            (FontFamily::Times, Face::Roman),
            (FontFamily::Times, Face::Bold),
            (FontFamily::Times, Face::Italic),
            (FontFamily::Times, Face::BoldItalic),
            (FontFamily::Helvetica, Face::Roman),
            (FontFamily::Helvetica, Face::Bold),
        ] {
            let f = text_font(family, face).unwrap();
            let table = metrics::resolve_width_table(family, face);
            // ' and ` are TeX's curly quotes (quoteright/quoteleft), not WinAnsi's straight
            // quotesingle/grave, so their widths legitimately differ from the ASCII tables.
            for b in (0x21u8..0x7F).filter(|b| !matches!(b, b'\'' | b'`')) {
                let code = f.code(b as char).unwrap();
                assert_eq!(
                    f.glyph_units(code),
                    table[b as usize],
                    "{family:?} {face:?} {:?}",
                    b as char
                );
            }
        }
    }

    #[test]
    fn all_programs_use_simple_ligatures() {
        for t in [
            &cm::CMR10, &cm::CMR7, &cm::CMR5, &cm::CMBX10, &cm::CMTI10, &afm::TIMES_ROMAN,
            &afm::TIMES_BOLD, &afm::TIMES_ITALIC, &afm::TIMES_BOLD_ITALIC, &afm::HELVETICA,
            &afm::HELVETICA_BOLD, &afm::HELVETICA_OBLIQUE, &afm::HELVETICA_BOLD_OBLIQUE,
        ] {
            assert!(t.ligs.iter().all(|l| l.1 == 0));
            assert!(t.kerns.windows(2).all(|w| w[0].0 < w[1].0));
            assert!(t.ligs.windows(2).all(|w| w[0].0 < w[1].0));
        }
    }
}
