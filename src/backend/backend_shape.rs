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
pub(crate) enum TextEncoding {
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
    /// An accented letter built as TeX's `\\accent` does (tex.web §1123–§1125): the accent
    /// glyph over the base glyph, as wide as the base. Interrupts ligatures and kerns.
    Accent { base: u8, accent: u8 },
}

/// A text face together with its ligature/kern program and code-indexed advance widths.
#[derive(Clone, Copy)]
pub struct TextFont {
    program: &'static LigKernTable,
    widths: &'static [u16],
    /// Exact TFM widths, when the font has them (Computer Modern).
    fix: Option<&'static crate::backend::backend_cm_fix::FixMetrics>,
    /// The TFM data the PDF `/Widths` of `widths` are written from (see
    /// [`crate::backend::scaled::exact_widths`]).
    pdf_exact: Option<crate::backend::scaled::ExactWidths>,
    /// The font's TFM (heights and parameters for `\accent`), Computer Modern only.
    tfm: Option<&'static crate::backend::backend_tfm::Tfm>,
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
            // The faces past `Face::AmsB` that are text fonts (cmss10, cmcsc10, ...): their own
            // kerns and ligatures (tex.web §1036). cmcsc10 has no f-ligature slots.
            (FontFamily::ComputerModern, f) if (f as usize) >= 18 => {
                let row = &metrics::EXTRA_FACES[f as usize - 18];
                let (lk, enc) = row.text?;
                (lk, &metrics::EXTRA_FACE_WIDTHS[f as usize - 18].0, enc)
            }
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
        pdf_exact: crate::backend::scaled::exact_widths(widths),
        tfm: if family == FontFamily::ComputerModern { crate::math::math_tfm::face_tfm(face) } else { None },
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
            pdf_exact: Some(crate::backend::scaled::ExactWidths::of_master(m)),
            tfm: crate::math::math_tfm::master_tfm(m),
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

/// The italic correction (`\/`) after `word` of `face` at `size`, in points: that of the
/// word's last glyph after ligatures (tex.web §1113 takes the tail character node, so
/// `ffi` uses the ligature's correction, not `i`'s); 0 after a character the font lacks.
pub fn word_italic_correction_pt(word: &str, face: Face, size: f32, family: FontFamily) -> f32 {
    let code = match text_font_at(family, face, size) {
        Some(font) => {
            let mut last = None;
            font.shape(word, |g, _| last = Some(g));
            match last {
                Some(Glyph::Code(code) | Glyph::Accent { base: code, .. }) => code as char,
                _ => return 0.0,
            }
        }
        None => match word.chars().next_back() {
            Some(c) => c,
            None => return 0.0,
        },
    };
    crate::backend::scaled::italic_correction_pt(family, face, size, code)
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
    /// follows the glyph, as a TFM fix_word (2^-20 em). Allocation-free.
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
                        sink(Glyph::Code(left), self.program.kern_fix(left, code));
                    }
                    pending = Some(code);
                }
                None => {
                    if let Some(left) = pending.take() {
                        sink(Glyph::Code(left), 0);
                    }
                    match ot1_accented(c).filter(|_| self.encoding != TextEncoding::WinAnsi) {
                        Some((base, accent)) => sink(Glyph::Accent { base, accent }, 0),
                        None => sink(Glyph::Raw(c), 0),
                    }
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
                Glyph::Code(code) | Glyph::Accent { base: code, .. } => tenths += self.glyph_units(code) as i32 * 10,
                Glyph::Raw(c) => raw += metrics::char_units_family(c, self.face, self.family) as u32,
            }
            tenths += crate::backend::backend_ligkern::fix_to_tenths(kern);
        });
        tenths as f32 * 0.1 + raw as f32
    }

    /// Shaped width of `text` at `size` pt, in scaled points: glyph widths and kerns are the
    /// TFM fix_words scaled as TeX scales them (§571).
    #[inline]
    pub fn width_sp(&self, text: &str, size: f32) -> i32 {
        let Some(fix) = self.fix else {
            return crate::backend::scaled::to_sp(self.width_units(text) * size * 0.001);
        };
        let scaler = crate::backend::scaled::Scaler::at_pt(size);
        let size_sp = crate::backend::scaled::to_sp(size) as i64;
        let mut sp: i64 = 0;
        let mut raw: u32 = 0;
        self.shape(text, |g, kern| {
            match g {
                Glyph::Code(code) | Glyph::Accent { base: code, .. } => {
                    sp += scaler.scale(fix.widths[code as usize & 127] as i32) as i64
                }
                Glyph::Raw(c) => raw += metrics::char_units_family(c, self.face, self.family) as u32,
            }
            if kern != 0 {
                sp += scaler.scale(kern) as i64;
            }
        });
        (sp + (raw as i64 * size_sp + 500) / 1000) as i32
    }

    /// Height and depth of the hbox TeX packs from `text` (§653), in scaled points: the
    /// largest TFM height and depth among its glyphs, an accented glyph's accent raised by
    /// the base's height over the x-height (§1123). `None` without TFM data or for a glyph
    /// outside the font.
    pub fn height_depth_sp(&self, text: &str, scaler: &crate::backend::scaled::Scaler) -> Option<(i32, i32)> {
        let tfm = self.tfm?;
        let x_height = tfm.param(5);
        let (mut h, mut d, mut ok) = (0i32, 0i32, true);
        self.shape(text, |g, _| match g {
            Glyph::Code(code) => {
                h = h.max(tfm.height(code));
                d = d.max(tfm.depth(code));
            }
            Glyph::Accent { base, accent } => {
                let bh = tfm.height(base);
                h = h.max(bh).max(tfm.height(accent) + (bh - x_height).max(0));
                d = d.max(tfm.depth(base));
            }
            Glyph::Raw(_) => ok = false,
        });
        // Scaling is monotonic, so the largest fix_word gives the largest dimension.
        ok.then(|| (scaler.scale(h), scaler.scale(d)))
    }

    /// [`Self::width_sp`] in points.
    #[inline]
    pub fn width_pt(&self, text: &str, size: f32) -> f32 {
        crate::backend::scaled::to_pt(self.width_sp(text, size) as i64)
    }

    /// Placement of an `\\accent` in 1/10000 em: (offset of the accent glyph from the
    /// base glyph's origin, rise). The accent is centred on the base and raised by the base's
    /// height above the x-height, shifted along the slant (tex.web §1125); a cedilla hangs
    /// below the base unraised.
    pub fn accent_placement(&self, base: u8, accent: u8) -> (i32, i32) {
        let em = |fix_word: i64| (fix_word * 10000 + (1 << 19)) >> 20;
        let (w_b, w_a, slant, x) = match self.fix {
            Some(f) => (
                em(f.widths[base as usize & 127] as i64),
                em(f.widths[accent as usize & 127] as i64),
                f.params[0] as f64 / (1u64 << 20) as f64,
                em(f.params[4] as i64),
            ),
            None => (self.glyph_units(base) as i64 * 10, self.glyph_units(accent) as i64 * 10, 0.0, 4306),
        };
        let (h_units, _) = crate::math::math_fonts::char_height_depth(base as char, self.face, self.family);
        let h = (h_units * 10.0).round() as i64;
        let rise = if accent == 0x18 { 0 } else { h - x };
        let delta = (w_b - w_a) / 2 + (rise as f64 * slant).round() as i64;
        (delta as i32, rise as i32)
    }

    /// `\accent` (tex.web §1123–§1125) in scaled points at `scaler`'s size: the kern before
    /// the accent and the accent's rise. The accent is raised by the base's height less the
    /// x-height and moved along the slant: `round((w - a)/2 + h t - x s)`; a cedilla hangs
    /// centred below the base, unraised. `None` without TFM data.
    pub fn accent_sp(&self, base: u8, accent: u8, scaler: &crate::backend::scaled::Scaler) -> Option<(i64, i64)> {
        let t = self.tfm?;
        let (w, a, h) = (scaler.scale(t.width(base)) as f64, scaler.scale(t.width(accent)) as f64, scaler.scale(t.height(base)));
        let x = scaler.scale(t.param(5));
        // §575 keeps the slant as a fix_word over 16, a pure number in units of 2^-16.
        let slant = (t.param(1) >> 4) as f64 / 65536.0;
        if accent == 0x18 {
            return Some((((w - a) / 2.0).round() as i64, 0));
        }
        let delta = ((w - a) / 2.0 + h as f64 * slant - x as f64 * slant).round() as i64;
        Some((delta, (h - x) as i64))
    }

    /// Advance width of glyph `code` in 1000-units/em.
    #[inline(always)]
    pub fn glyph_units(&self, code: u8) -> u16 {
        self.widths.get(code as usize).copied().unwrap_or(0)
    }

    /// TeX width of glyph `code` in scaled points: the TFM fix_word scaled by `scaler`
    /// (§571), or the AFM width for fonts without TFM data.
    #[inline(always)]
    pub fn glyph_sp(&self, code: u8, scaler: &crate::backend::scaled::Scaler, size_sp: i64) -> i64 {
        match self.fix {
            Some(m) => scaler.scale(m.widths[code as usize & 127] as i32) as i64,
            None => (self.glyph_units(code) as i64 * size_sp + 500) / 1000,
        }
    }

    /// Advance of glyph `code` in 1/10000 em as the font's PDF `/Widths` declare it.
    #[inline(always)]
    pub fn glyph_tenths(&self, code: u8) -> i64 {
        match self.pdf_exact {
            Some(m) => crate::backend::scaled::width_tenths(m, code, 0),
            None => self.glyph_units(code) as i64 * 10,
        }
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

/// OT1 has no accented letters: TeX builds them with `\\accent` (OT1enc.def), base and
/// accent glyph. Returns (base code, accent code) of a precomposed letter.
#[inline]
fn ot1_accented(c: char) -> Option<(u8, u8)> {
    Some(match c {
        'À' => (b'A', 0x12),
        'à' => (b'a', 0x12),
        'È' => (b'E', 0x12),
        'è' => (b'e', 0x12),
        'Ì' => (b'I', 0x12),
        'ì' => (0x10, 0x12),
        'Ò' => (b'O', 0x12),
        'ò' => (b'o', 0x12),
        'Ù' => (b'U', 0x12),
        'ù' => (b'u', 0x12),
        'Ỳ' => (b'Y', 0x12),
        'ỳ' => (b'y', 0x12),
        'Ǹ' => (b'N', 0x12),
        'ǹ' => (b'n', 0x12),
        'Ẁ' => (b'W', 0x12),
        'ẁ' => (b'w', 0x12),
        'Á' => (b'A', 0x13),
        'á' => (b'a', 0x13),
        'É' => (b'E', 0x13),
        'é' => (b'e', 0x13),
        'Í' => (b'I', 0x13),
        'í' => (0x10, 0x13),
        'Ó' => (b'O', 0x13),
        'ó' => (b'o', 0x13),
        'Ú' => (b'U', 0x13),
        'ú' => (b'u', 0x13),
        'Ý' => (b'Y', 0x13),
        'ý' => (b'y', 0x13),
        'Ć' => (b'C', 0x13),
        'ć' => (b'c', 0x13),
        'Ǵ' => (b'G', 0x13),
        'ǵ' => (b'g', 0x13),
        'Ḱ' => (b'K', 0x13),
        'ḱ' => (b'k', 0x13),
        'Ĺ' => (b'L', 0x13),
        'ĺ' => (b'l', 0x13),
        'Ń' => (b'N', 0x13),
        'ń' => (b'n', 0x13),
        'Ŕ' => (b'R', 0x13),
        'ŕ' => (b'r', 0x13),
        'Ś' => (b'S', 0x13),
        'ś' => (b's', 0x13),
        'Ź' => (b'Z', 0x13),
        'ź' => (b'z', 0x13),
        'Ẃ' => (b'W', 0x13),
        'ẃ' => (b'w', 0x13),
        'Â' => (b'A', 0x5E),
        'â' => (b'a', 0x5E),
        'Ê' => (b'E', 0x5E),
        'ê' => (b'e', 0x5E),
        'Î' => (b'I', 0x5E),
        'î' => (0x10, 0x5E),
        'Ô' => (b'O', 0x5E),
        'ô' => (b'o', 0x5E),
        'Û' => (b'U', 0x5E),
        'û' => (b'u', 0x5E),
        'Ŷ' => (b'Y', 0x5E),
        'ŷ' => (b'y', 0x5E),
        'Ĉ' => (b'C', 0x5E),
        'ĉ' => (b'c', 0x5E),
        'Ĝ' => (b'G', 0x5E),
        'ĝ' => (b'g', 0x5E),
        'Ŝ' => (b'S', 0x5E),
        'ŝ' => (b's', 0x5E),
        'Ẑ' => (b'Z', 0x5E),
        'ẑ' => (b'z', 0x5E),
        'Ĥ' => (b'H', 0x5E),
        'ĥ' => (b'h', 0x5E),
        'Ĵ' => (b'J', 0x5E),
        'ĵ' => (0x11, 0x5E),
        'Ŵ' => (b'W', 0x5E),
        'ŵ' => (b'w', 0x5E),
        'Ä' => (b'A', 0x7F),
        'ä' => (b'a', 0x7F),
        'Ë' => (b'E', 0x7F),
        'ë' => (b'e', 0x7F),
        'Ï' => (b'I', 0x7F),
        'ï' => (0x10, 0x7F),
        'Ö' => (b'O', 0x7F),
        'ö' => (b'o', 0x7F),
        'Ü' => (b'U', 0x7F),
        'ü' => (b'u', 0x7F),
        'Ÿ' => (b'Y', 0x7F),
        'ÿ' => (b'y', 0x7F),
        'ẗ' => (b't', 0x7F),
        'Ḧ' => (b'H', 0x7F),
        'ḧ' => (b'h', 0x7F),
        'Ẅ' => (b'W', 0x7F),
        'ẅ' => (b'w', 0x7F),
        'Ã' => (b'A', 0x7E),
        'ã' => (b'a', 0x7E),
        'Ẽ' => (b'E', 0x7E),
        'ẽ' => (b'e', 0x7E),
        'Ĩ' => (b'I', 0x7E),
        'ĩ' => (0x10, 0x7E),
        'Õ' => (b'O', 0x7E),
        'õ' => (b'o', 0x7E),
        'Ũ' => (b'U', 0x7E),
        'ũ' => (b'u', 0x7E),
        'Ỹ' => (b'Y', 0x7E),
        'ỹ' => (b'y', 0x7E),
        'Ñ' => (b'N', 0x7E),
        'ñ' => (b'n', 0x7E),
        'Ā' => (b'A', 0x16),
        'ā' => (b'a', 0x16),
        'Ē' => (b'E', 0x16),
        'ē' => (b'e', 0x16),
        'Ī' => (b'I', 0x16),
        'ī' => (0x10, 0x16),
        'Ō' => (b'O', 0x16),
        'ō' => (b'o', 0x16),
        'Ū' => (b'U', 0x16),
        'ū' => (b'u', 0x16),
        'Ȳ' => (b'Y', 0x16),
        'ȳ' => (b'y', 0x16),
        'Ḡ' => (b'G', 0x16),
        'ḡ' => (b'g', 0x16),
        'Ȧ' => (b'A', 0x5F),
        'ȧ' => (b'a', 0x5F),
        'Ė' => (b'E', 0x5F),
        'ė' => (b'e', 0x5F),
        'İ' => (b'I', 0x5F),
        'Ȯ' => (b'O', 0x5F),
        'ȯ' => (b'o', 0x5F),
        'Ẏ' => (b'Y', 0x5F),
        'ẏ' => (b'y', 0x5F),
        'Ċ' => (b'C', 0x5F),
        'ċ' => (b'c', 0x5F),
        'Ġ' => (b'G', 0x5F),
        'ġ' => (b'g', 0x5F),
        'Ṅ' => (b'N', 0x5F),
        'ṅ' => (b'n', 0x5F),
        'Ṙ' => (b'R', 0x5F),
        'ṙ' => (b'r', 0x5F),
        'Ṡ' => (b'S', 0x5F),
        'ṡ' => (b's', 0x5F),
        'Ṫ' => (b'T', 0x5F),
        'ṫ' => (b't', 0x5F),
        'Ż' => (b'Z', 0x5F),
        'ż' => (b'z', 0x5F),
        'Ḋ' => (b'D', 0x5F),
        'ḋ' => (b'd', 0x5F),
        'Ḣ' => (b'H', 0x5F),
        'ḣ' => (b'h', 0x5F),
        'Ẇ' => (b'W', 0x5F),
        'ẇ' => (b'w', 0x5F),
        'Ă' => (b'A', 0x15),
        'ă' => (b'a', 0x15),
        'Ĕ' => (b'E', 0x15),
        'ĕ' => (b'e', 0x15),
        'Ĭ' => (b'I', 0x15),
        'ĭ' => (0x10, 0x15),
        'Ŏ' => (b'O', 0x15),
        'ŏ' => (b'o', 0x15),
        'Ŭ' => (b'U', 0x15),
        'ŭ' => (b'u', 0x15),
        'Ğ' => (b'G', 0x15),
        'ğ' => (b'g', 0x15),
        'Ǎ' => (b'A', 0x14),
        'ǎ' => (b'a', 0x14),
        'Ě' => (b'E', 0x14),
        'ě' => (b'e', 0x14),
        'Ǐ' => (b'I', 0x14),
        'ǐ' => (0x10, 0x14),
        'Ǒ' => (b'O', 0x14),
        'ǒ' => (b'o', 0x14),
        'Ǔ' => (b'U', 0x14),
        'ǔ' => (b'u', 0x14),
        'Č' => (b'C', 0x14),
        'č' => (b'c', 0x14),
        'Ǧ' => (b'G', 0x14),
        'ǧ' => (b'g', 0x14),
        'Ǩ' => (b'K', 0x14),
        'ǩ' => (b'k', 0x14),
        'Ľ' => (b'L', 0x14),
        'ľ' => (b'l', 0x14),
        'Ň' => (b'N', 0x14),
        'ň' => (b'n', 0x14),
        'Ř' => (b'R', 0x14),
        'ř' => (b'r', 0x14),
        'Š' => (b'S', 0x14),
        'š' => (b's', 0x14),
        'Ť' => (b'T', 0x14),
        'ť' => (b't', 0x14),
        'Ž' => (b'Z', 0x14),
        'ž' => (b'z', 0x14),
        'Ď' => (b'D', 0x14),
        'ď' => (b'd', 0x14),
        'Ȟ' => (b'H', 0x14),
        'ȟ' => (b'h', 0x14),
        'ǰ' => (0x11, 0x14),
        'Ő' => (b'O', 0x7D),
        'ő' => (b'o', 0x7D),
        'Ű' => (b'U', 0x7D),
        'ű' => (b'u', 0x7D),
        'Ȩ' => (b'E', 0x18),
        'ȩ' => (b'e', 0x18),
        'Ç' => (b'C', 0x18),
        'ç' => (b'c', 0x18),
        'Ģ' => (b'G', 0x18),
        'ģ' => (b'g', 0x18),
        'Ķ' => (b'K', 0x18),
        'ķ' => (b'k', 0x18),
        'Ļ' => (b'L', 0x18),
        'ļ' => (b'l', 0x18),
        'Ņ' => (b'N', 0x18),
        'ņ' => (b'n', 0x18),
        'Ŗ' => (b'R', 0x18),
        'ŗ' => (b'r', 0x18),
        'Ş' => (b'S', 0x18),
        'ş' => (b's', 0x18),
        'Ţ' => (b'T', 0x18),
        'ţ' => (b't', 0x18),
        'Ḑ' => (b'D', 0x18),
        'ḑ' => (b'd', 0x18),
        'Ḩ' => (b'H', 0x18),
        'ḩ' => (b'h', 0x18),
        'Å' => (b'A', 0x17),
        'å' => (b'a', 0x17),
        'Ů' => (b'U', 0x17),
        'ů' => (b'u', 0x17),
        'ẙ' => (b'y', 0x17),
        'ẘ' => (b'w', 0x17),
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
        font.shape(text, |g, k| out.push((g, crate::backend::backend_ligkern::fix_to_tenths(k))));
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
        let g = glyphs(&f, "f\u{20ac}i");
        assert_eq!(g, vec![(Glyph::Code(b'f'), 0), (Glyph::Raw('€'), 0), (Glyph::Code(b'i'), 0)]);
        // An accented letter is the OT1 letter with the accent glyph over it (\accent).
        let g = glyphs(&f, "f\u{e9}i");
        assert_eq!(g[1].0, Glyph::Accent { base: b'e', accent: 0x13 });
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
