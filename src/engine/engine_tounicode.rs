//! pdfTeX's glyph-name to Unicode table (`\pdfglyphtounicode`, pdftexdir/tounicode.c) and
//! the Unicode it gives a glyph name when `\pdfgentounicode` asks for ToUnicode maps.
//!
//! latex.ltx loads glyphtounicode.tex into the format, so the table is part of it: one
//! sorted vector of (name, hex) pairs, searched by bisection.

use super::engine_run::Machine;
#[cfg(feature = "oracle")]
use super::engine_snapshot::{Reader, Writer};

#[derive(Debug, Default)]
pub struct GlyphUnicode {
    pairs: Vec<(Box<str>, Box<str>)>,
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
        self.pairs.binary_search_by(|(n, _)| (**n).cmp(name)).ok().map(|k| &*self.pairs[k].1)
    }

    /// tounicode.c `set_glyph_unicode`: the table, then `uniXXXX…` (groups of four hex
    /// digits) and `uXXXX`–`uXXXXXX`, a name's part before `.`, and `a_b` as `a` then `b`.
    /// The result is space-separated UTF-16 hex, as in the table.
    pub fn unicode_of(&self, name: &str) -> Option<String> {
        if let Some(h) = self.get(name) {
            return Some(h.to_string());
        }
        if let Some((base, _)) = name.split_once('.') {
            return if base.is_empty() { None } else { self.unicode_of(base) };
        }
        if name.contains('_') {
            let parts: Option<Vec<String>> = name.split('_').map(|p| self.unicode_of(p)).collect();
            return parts.map(|p| p.join(" "));
        }
        let hex = |s: &str| s.len() >= 4 && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_lowercase());
        if let Some(h) = name.strip_prefix("uni").filter(|h| hex(h) && h.len() % 4 == 0) {
            return Some(h.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join(" "));
        }
        if let Some(h) = name.strip_prefix('u').filter(|h| hex(h) && h.len() <= 6) {
            let c = u32::from_str_radix(h, 16).ok()?;
            let mut units = [0u16; 2];
            return char::from_u32(c).map(|c| c.encode_utf16(&mut units).iter().map(|u| format!("{u:04X}")).collect::<Vec<_>>().join(" "));
        }
        None
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
        pairs.windows(2).all(|w| w[0].0 < w[1].0).then_some(GlyphUnicode { pairs })
    }
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
        assert_eq!(g.unicode_of("fi").as_deref(), Some("0066 0069"));
        assert_eq!(g.unicode_of("A.sc").as_deref(), Some("0041"));
        assert_eq!(g.unicode_of("A_fi").as_deref(), Some("0041 0066 0069"));
        assert_eq!(g.unicode_of("uni00410042").as_deref(), Some("0041 0042"));
        assert_eq!(g.unicode_of("u1D400").as_deref(), Some("D835 DC00"));
        assert_eq!(g.unicode_of("zzz"), None);
    }
}
