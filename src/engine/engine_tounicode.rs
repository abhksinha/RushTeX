//! pdfTeX's glyph-name to Unicode table (`\pdfglyphtounicode`, pdftexdir/tounicode.c) and
//! the Unicode it gives a glyph name when `\pdfgentounicode` asks for ToUnicode maps.
//!
//! latex.ltx loads glyphtounicode.tex into the format, so the table is part of it: one
//! sorted vector of (name, hex) pairs, searched by bisection.

use super::engine_run::Machine;
#[cfg(feature = "oracle")]
use super::engine_snapshot::{Reader, Writer};

#[derive(Default)]
pub struct GlyphUnicode {
    pairs: Vec<(Box<str>, Box<str>)>,
    /// The format's table carried in the binary (`data_glyphs`), under `pairs`: the native
    /// job's, which loads no format.
    carried: Option<&'static crate::data::data_glyphs::GlyphTable<'static>>,
}

impl std::fmt::Debug for GlyphUnicode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlyphUnicode").field("pairs", &self.pairs.len()).field("carried", &self.carried.map(|t| t.len())).finish()
    }
}

impl GlyphUnicode {
    /// `\pdfglyphtounicode{name}{hex}`: a later definition replaces an earlier one.
    pub fn define(&mut self, name: &str, hex: &str) {
        match self.pairs.binary_search_by(|(n, _)| (**n).cmp(name)) {
            Ok(k) => self.pairs[k].1 = hex.into(),
            Err(k) => self.pairs.insert(k, (name.into(), hex.into())),
        }
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.get_bytes(name.as_bytes())
    }

    fn get_bytes(&self, name: &[u8]) -> Option<&str> {
        match self.pairs.binary_search_by(|(n, _)| n.as_bytes().cmp(name)) {
            Ok(k) => Some(&*self.pairs[k].1),
            Err(_) => self.carried.and_then(|t| t.get(name)),
        }
    }

    /// Font `tfm`'s own entry for glyph `name` (`tfm:<tfm>/<name>`), the key made on the
    /// stack.
    fn get_tfm(&self, tfm: &str, name: &str) -> Option<&str> {
        let n = 5 + tfm.len() + name.len();
        let mut buf = [0u8; 128];
        if n > buf.len() {
            return self.get(&format!("tfm:{tfm}/{name}"));
        }
        buf[..4].copy_from_slice(b"tfm:");
        buf[4..4 + tfm.len()].copy_from_slice(tfm.as_bytes());
        buf[4 + tfm.len()] = b'/';
        buf[5 + tfm.len()..n].copy_from_slice(name.as_bytes());
        self.get_bytes(&buf[..n])
    }

    /// Whether font `tfm` has entries of its own (`tfm:<tfm>/…`) in the document's pairs or
    /// the carried table: only then does its ToUnicode map differ from another font's with
    /// the same glyph names.
    pub fn has_font_entries(&self, tfm: &str) -> bool {
        let prefix = format!("tfm:{tfm}/");
        let k = self.pairs.partition_point(|(n, _)| **n < *prefix);
        if self.pairs.get(k).is_some_and(|(n, _)| n.starts_with(&prefix)) {
            return true;
        }
        // The carried table is hashed: the fonts it has entries for, listed once.
        static FONTS: std::sync::OnceLock<Vec<&'static [u8]>> = std::sync::OnceLock::new();
        let Some(table) = self.carried else { return false };
        let fonts = FONTS.get_or_init(|| {
            let mut v: Vec<&[u8]> = table.pairs().filter_map(|(n, _)| n.strip_prefix(b"tfm:")).filter_map(|n| n.iter().position(|&c| c == b'/').map(|k| &n[..k])).collect();
            v.sort_unstable();
            v.dedup();
            v
        });
        fonts.binary_search(&tfm.as_bytes()).is_ok()
    }

    /// The format's table from the binary, for a job that loads no format.
    pub fn carry(&mut self) {
        self.carried = crate::data::data_glyphs::carried();
    }

    /// The pairs the format defined, sorted by name (for `rushtex-gen glyphs`).
    pub fn pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.pairs.iter().map(|(n, h)| (&**n, &**h))
    }

    /// Whether no entry exists (tounicode.c's `glyph_unicode_tree == NULL`: pdfTeX then
    /// writes no ToUnicode map).
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty() && self.carried.is_none_or(|t| t.is_empty())
    }

    /// tounicode.c `set_glyph_unicode`: the Unicode of glyph `name` of font `tfm`. The part
    /// before the first `.`; `a_b` as `a` then `b` (parts with none left out); the font's own
    /// entry (`tfm:<tfm>/<name>`), then the table; then `uniXXXX…` and `uXXXX`–`uXXXXXX`.
    pub fn glyph_code(&self, name: &str, tfm: &str) -> Option<GlyphCode> {
        let name = name.split('.').next().unwrap_or_default();
        if name.is_empty() {
            return None;
        }
        if name.contains('_') {
            let mut seq = String::new();
            for part in name.split('_') {
                match self.glyph_code(part, tfm) {
                    Some(GlyphCode::Seq(s)) => seq.push_str(&s),
                    Some(GlyphCode::Code(c)) => seq.push_str(&utf16be(c)),
                    None => {}
                }
            }
            return Some(GlyphCode::Seq(seq));
        }
        if let Some(value) = self.get_tfm(tfm, name).or_else(|| self.get(name)) {
            return entry_code(value);
        }
        if let Some(hex) = name.strip_prefix("uni") {
            let code = unicode_value(hex, true)?;
            return Some(if hex.len() == 4 { GlyphCode::Code(code) } else { GlyphCode::Seq(hex.to_string()) });
        }
        name.strip_prefix('u').and_then(|hex| unicode_value(hex, false)).map(GlyphCode::Code)
    }

    #[cfg(feature = "oracle")]
    pub(super) fn dump(&self, w: &mut Writer) {
        w.u32(self.pairs.len() as u32);
        for (n, h) in &self.pairs {
            w.str(n);
            w.str(h);
        }
    }

    #[cfg(feature = "oracle")]
    pub(super) fn undump(r: &mut Reader) -> Option<GlyphUnicode> {
        let n = r.u32()? as usize;
        let mut pairs = Vec::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            pairs.push((r.str()?.into_boxed_str(), r.str()?.into_boxed_str()));
        }
        pairs.windows(2).all(|w| w[0].0 < w[1].0).then_some(GlyphUnicode { pairs, carried: None })
    }
}

/// A code's Unicode as tounicode.c's `glyph_unicode_entry` holds it: one code point
/// (`code >= 0`), or UTF-16 hex digits (`UNI_STRING`, `UNI_EXTRA_STRING`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlyphCode {
    Code(u32),
    Seq(String),
}

/// tounicode.c `deftounicode`'s reading of a value: with a space, the digits as a string
/// (spaces dropped); else one code point, none past `10FFFF`.
fn entry_code(value: &str) -> Option<GlyphCode> {
    let value = value.trim_matches(' ');
    if value.contains(' ') {
        return Some(GlyphCode::Seq(value.chars().filter(|&c| c != ' ').collect()));
    }
    u32::from_str_radix(value, 16).ok().filter(|&c| c <= 0x10FFFF).map(GlyphCode::Code)
}

/// tounicode.c `check_unicode_value`: `hex` as groups of four upper-case digits, each
/// outside the surrogates (`multiple`, the last group's value), or as one code point of four
/// to six digits.
fn unicode_value(hex: &str, multiple: bool) -> Option<u32> {
    let l = hex.len();
    if l == 0 || (multiple && !l.is_multiple_of(4)) || (!multiple && !(4..=6).contains(&l)) || !hex.bytes().all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b)) {
        return None;
    }
    let valid = |c: u32, top: u32| c <= 0xD7FF || (0xE000..=top).contains(&c);
    if multiple {
        let mut code = 0;
        for group in hex.as_bytes().chunks(4) {
            code = u32::from_str_radix(std::str::from_utf8(group).ok()?, 16).ok()?;
            if !valid(code, 0xFFFF) {
                return None;
            }
        }
        return Some(code);
    }
    u32::from_str_radix(hex, 16).ok().filter(|&c| valid(c, 0x10FFFF))
}

/// tounicode.c `utf16be_str`: four hex digits, or a surrogate pair's eight.
pub fn utf16be(code: u32) -> String {
    if code <= 0xFFFF {
        return format!("{code:04X}");
    }
    let v = code - 0x10000;
    format!("{:04X}{:04X}", v / 0x400 + 0xD800, v % 0x400 + 0xDC00)
}

impl Machine<'_, '_> {
    /// pdfTeX's `\pdfglyphtounicode`: two general texts, expanded.
    pub(super) fn pdf_glyph_to_unicode(&mut self) {
        let name = self.general_text_string();
        let hex = self.general_text_string();
        self.e.glyph_unicode.define(&name, &hex);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_names() {
        let mut g = GlyphUnicode::default();
        g.define("fi", "0066 0069");
        g.define("A", "0041");
        g.define("tfm:cmr10/Z", "005A");
        let code = |n: &str| g.glyph_code(n, "cmr10");
        assert_eq!(code("fi"), Some(GlyphCode::Seq("00660069".into())));
        assert_eq!(code("A.sc"), Some(GlyphCode::Code(0x41)));
        assert_eq!(code("A_fi"), Some(GlyphCode::Seq("004100660069".into())));
        assert_eq!(code("A_zzz"), Some(GlyphCode::Seq("0041".into())));
        assert_eq!(code("Z"), Some(GlyphCode::Code(0x5A)));
        assert_eq!(g.glyph_code("Z", "cmr12"), None);
        assert_eq!(code("uni00410042"), Some(GlyphCode::Seq("00410042".into())));
        assert_eq!(code("uni0041"), Some(GlyphCode::Code(0x41)));
        assert_eq!(code("u1D400"), Some(GlyphCode::Code(0x1D400)));
        assert_eq!(code("uniD800"), None);
        assert_eq!(code("zzz"), None);
        assert_eq!(code(".notdef"), None);
        assert_eq!(utf16be(0x1D400), "D835DC00");
    }
}
