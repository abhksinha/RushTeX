//! pdfTeX's glyph-name to Unicode table (`\pdfglyphtounicode`) as the format carries it:
//! `latex.ltx` loads `glyphtounicode.tex` into the format, and the native document job,
//! which loads no format, reads it from here when `\pdfgentounicode` asks for ToUnicode
//! maps (`engine_tounicode::GlyphUnicode`).
//!
//! `rushtex-gen glyphs` reads the pairs from the oracle's format and writes this table, then
//! checks that it reads back to the same pairs.
//!
//! Sections:
//! - `gu`: the glyph names as a perfect hash (`data_phf`), IDs in byte order of the names;
//! - `gu.u`: the Unicode of each name, space-separated UTF-16 hex as in the table;
//! - `gu.ue`: each value's end in `gu.u`, by ID.

use super::data_blob::{Blob, U32s};
use super::data_phf::Phf;

pub struct GlyphTable<'a> {
    names: Phf<'a>,
    hex: &'a str,
    hex_ends: U32s<'a>,
}

impl<'a> GlyphTable<'a> {
    pub fn from_blob(b: &Blob<'a>) -> Option<GlyphTable<'a>> {
        let t = GlyphTable { names: Phf::from_blob(b, "gu")?, hex: std::str::from_utf8(b.section("gu.u")?).ok()?, hex_ends: b.u32s("gu.ue")? };
        // Ends in bounds and rising, on character boundaries: `hex` slices without checks.
        let rising = t.hex_ends.iter().try_fold(0, |at, e| (e >= at && t.hex.is_char_boundary(e as usize)).then_some(e)).is_some_and(|last| last as usize <= t.hex.len());
        (rising && t.hex_ends.len() == t.names.len()).then_some(t)
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    fn hex(&self, id: usize) -> &'a str {
        let at = if id == 0 { 0 } else { self.hex_ends.get(id - 1) as usize };
        &self.hex[at..self.hex_ends.get(id) as usize]
    }

    /// The Unicode the table gives glyph `name` (its bytes), borrowed: one hash probe.
    pub fn get(&self, name: &[u8]) -> Option<&'a str> {
        self.names.get(name).map(|id| self.hex(id as usize))
    }

    /// Every pair, in order.
    pub fn pairs(&self) -> impl Iterator<Item = (&'a [u8], &'a str)> + '_ {
        (0..self.len()).map(|i| (self.names.name(i as u32), self.hex(i)))
    }
}

static GLYPHS_BLOB: &[u8] = include_bytes!("../../assets/tables/glyphs.rtdb");

/// The format's table, checked on first use; `None` if it is not a table of this build.
pub fn carried() -> Option<&'static GlyphTable<'static>> {
    static TABLE: std::sync::OnceLock<Option<GlyphTable<'static>>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| Blob::new(GLYPHS_BLOB).and_then(|b| GlyphTable::from_blob(&b))).as_ref()
}

/// The table of `pairs`, which must be sorted by name with no name twice.
#[cfg(any(test, feature = "gen"))]
pub fn write<'p>(w: &mut super::data_blob::BlobWriter, pairs: impl Iterator<Item = (&'p str, &'p str)>) -> Result<(), super::data_phf::BuildError> {
    let (mut names, mut hex, mut hex_ends) = (Vec::new(), Vec::new(), Vec::new());
    for (n, h) in pairs {
        names.push(n.as_bytes());
        hex.extend_from_slice(h.as_bytes());
        hex_ends.push(hex.len() as u32);
    }
    super::data_phf::write(w, "gu", &names)?;
    w.bytes("gu.u", hex);
    w.u32s("gu.ue", &hex_ends);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_read_back() {
        let pairs = [("A", "0041"), ("fi", "0066 0069"), ("notequal", "2260")];
        let mut w = super::super::data_blob::BlobWriter::default();
        write(&mut w, pairs.iter().copied()).unwrap();
        let bytes = w.finish();
        let t = GlyphTable::from_blob(&Blob::new(&bytes).unwrap()).unwrap();
        assert_eq!(t.len(), 3);
        assert_eq!(t.get(b"fi"), Some("0066 0069"));
        assert_eq!(t.get(b"notequal"), Some("2260"));
        assert_eq!(t.get(b"B"), None);
        assert_eq!(t.get(b"zzz"), None);
    }

    /// The carried table is the format's: it has `glyphtounicode.tex`'s names.
    #[test]
    fn carried_table_has_the_format_names() {
        let t = carried().expect("assets/tables/glyphs.rtdb is not a table of this build: run rushtex-gen glyphs");
        assert!(t.len() > 4000);
        assert_eq!(t.get(b"notequal"), Some("2260"));
        assert_eq!(t.get(b"A"), Some("0041"));
    }
}
