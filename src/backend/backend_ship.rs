//! The PDF document around pages the engine shipped out (`engine_ship`): page tree, fonts
//! and content streams. Fonts are found by their file name (`cmr10`), not by a fixed face
//! list, so any font the engine loaded is written: embedded when its program is in the
//! binary, otherwise as `pdftex.map` says (a Type 1 file and an encoding vector, read through
//! the caller's file resolver, as pdfTeX reads them), and by name when neither applies.
//! Several sizes of one font share one PDF font, and its `/Widths` come from the first size
//! used, as pdfTeX writes them.

use crate::backend::backend_pdftex::{divide_scaled, pdf_font_size, print_bp, print_int, print_real};

/// One shipped page: its object number (given by the engine, which refers to it), its size
/// in scaled points, its content stream and its annotations' objects.
#[derive(Debug, Default, Clone)]
pub struct ShippedPage {
    pub id: i32,
    pub width: i32,
    pub height: i32,
    pub content: Vec<u8>,
    pub annots: Vec<i32>,
    /// The page's transparency group object (0 for none), and the images it shows as
    /// (`/Im<n>`, object).
    pub group: i32,
    pub images: Vec<(u32, i32)>,
}

/// A font the pages use: its file name, size and the TeX widths of its 256 characters.
#[derive(Debug, Clone)]
pub struct DocFont {
    pub name: String,
    pub size: i64,
    pub widths: [i32; 256],
}

/// Objects written so far and where each starts.
struct Doc {
    buf: Vec<u8>,
    offsets: Vec<usize>,
}

impl Doc {
    fn begin(&mut self, id: usize) {
        if self.offsets.len() <= id {
            self.offsets.resize(id + 1, 0);
        }
        self.offsets[id] = self.buf.len();
        print_int(&mut self.buf, id as i64);
        self.buf.extend_from_slice(b" 0 obj\n");
    }

    fn end(&mut self) {
        self.buf.extend_from_slice(b"endobj\n");
    }

    fn reference(&mut self, id: usize) {
        print_int(&mut self.buf, id as i64);
        self.buf.extend_from_slice(b" 0 R");
    }

    fn stream(&mut self, id: usize, dict: &[u8], data: &[u8], deflated: bool) {
        self.begin(id);
        self.buf.extend_from_slice(b"<<");
        self.buf.extend_from_slice(dict);
        if deflated {
            self.buf.extend_from_slice(b" /Filter /FlateDecode");
        }
        self.buf.extend_from_slice(b" /Length ");
        print_int(&mut self.buf, data.len() as i64);
        self.buf.extend_from_slice(b" >>\nstream\n");
        self.buf.extend_from_slice(data);
        self.buf.extend_from_slice(b"\nendstream\n");
        self.end();
    }
}

/// A font's line in `pdftex.map`: its PostScript name, encoding file and font file.
#[derive(Debug, Default, PartialEq)]
struct MapEntry<'m> {
    ps: &'m str,
    enc: Option<&'m str>,
    file: Option<&'m str>,
}

/// The `pdftex.map` line of TFM `name` (pdftexdir/mapfile.c: the TFM name, then the
/// PostScript name unless the next field is a file or a quoted special, then `<file`s).
fn map_entry<'m>(map: &'m str, name: &str) -> Option<MapEntry<'m>> {
    let line = map.lines().find(|l| l.split_whitespace().next() == Some(name))?;
    let mut e = MapEntry::default();
    let mut quoted = false;
    for (i, t) in line.split_whitespace().enumerate().skip(1) {
        // A quoted special (`" T1Encoding ReEncodeFont "`) is skipped whole.
        let quotes = t.matches('"').count();
        if quoted || quotes > 0 {
            quoted ^= quotes % 2 == 1;
            continue;
        }
        if let Some(f) = t.strip_prefix('<') {
            let f = f.trim_start_matches(['<', '[']);
            if f.ends_with(".enc") {
                e.enc = Some(f);
            } else if !f.is_empty() {
                e.file = Some(f);
            }
        } else if i == 1 {
            e.ps = t;
        }
    }
    Some(e)
}

/// The 256 glyph names of an encoding file (`/Name [ /g0 /g1 ... ] def`), read as pdfTeX's
/// `load_enc_file` reads them: from the first `[`, names begin at `/` and end at a blank,
/// `]` or the next `/` (`/enclmec[` and `/a/b` are both valid), and a `%` ends the line.
fn encoding_names(enc: &str) -> Vec<&str> {
    let mut names = Vec::with_capacity(256);
    let mut open = false;
    for line in enc.lines() {
        let line = line.split('%').next().unwrap_or("");
        let mut rest = line;
        if !open {
            let Some(k) = line.find('[') else { continue };
            open = true;
            rest = &line[k + 1..];
        }
        let b = rest.as_bytes();
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b']' => return names,
                b'/' => {
                    let start = i + 1;
                    i = start;
                    while i < b.len() && !matches!(b[i], b' ' | b'\t' | b'\r' | b']' | b'/') {
                        i += 1;
                    }
                    names.push(&rest[start..i]);
                }
                _ => i += 1,
            }
        }
    }
    names
}

/// A `.pfb` file's three segments (cleartext, binary, trailer), as `/Length1..3` count
/// them; `None` when it is not a PFB.
/// A Type 1 program's `/FontName` (`/FontName /NimbusRomNo9L-Regu def` in its cleartext).
fn font_name(clear: &[u8]) -> Option<String> {
    let at = clear.windows(9).position(|w| w == b"/FontName")? + 9;
    let rest = &clear[at..];
    let start = rest.iter().position(|&c| c == b'/')? + 1;
    let len = rest[start..].iter().position(|c| c.is_ascii_whitespace() || *c == b'/')?;
    Some(rest[start..start + len].iter().map(|&c| char::from(c)).collect())
}

/// A Type 1 program's built-in encoding (`dup 65 /A put` in its cleartext), by code.
/// The cleartext is Latin-1 (a copyright notice may hold `©`), so it is read bytewise.
fn builtin_encoding_names(clear: &[u8]) -> Vec<String> {
    let mut names = vec![String::from(".notdef"); 256];
    let text: String = clear.iter().map(|&c| char::from(c)).collect();
    let mut t = text.split_whitespace();
    while let Some(w) = t.next() {
        if w != "dup" {
            continue;
        }
        let (Some(code), Some(name)) = (t.next(), t.next()) else { break };
        if let (Ok(c), Some(n)) = (code.parse::<usize>(), name.strip_prefix('/')) {
            if c < 256 {
                names[c] = n.to_string();
            }
        }
    }
    names
}

/// Token text as PDF bytes: pdfTeX's strings are 8-bit.
fn latin1(s: &str) -> Vec<u8> {
    s.chars().map(|c| c as u32 as u8).collect()
}

fn pfb_segments(pfb: &[u8]) -> Option<(Vec<u8>, [usize; 3])> {
    let (mut data, mut lens, mut at, mut seg) = (Vec::with_capacity(pfb.len()), [0usize; 3], 0, 0);
    while at + 6 <= pfb.len() && pfb[at] == 0x80 && pfb[at + 1] != 3 {
        let n = u32::from_le_bytes([pfb[at + 2], pfb[at + 3], pfb[at + 4], pfb[at + 5]]) as usize;
        let body = pfb.get(at + 6..at + 6 + n)?;
        // Segments alternate cleartext and binary; consecutive ones of a kind add up.
        let kind = if pfb[at + 1] == 2 { 1 } else if seg == 0 { 0 } else { 2 };
        seg = seg.max(kind);
        lens[kind] += n;
        data.extend_from_slice(body);
        at += 6 + n;
    }
    (lens[0] > 0).then_some((data, lens))
}

/// pdfTeX's generated ToUnicode CMap (tounicode.c `write_tounicode`) for a font whose
/// codes have glyph `names`: one `bfchar` entry per code whose name has a Unicode.
fn tounicode_cmap(font: &str, names: &[&str], unicode: &dyn Fn(&str) -> Option<String>) -> Vec<u8> {
    let entries: Vec<(usize, String)> = names.iter().enumerate().filter_map(|(c, n)| unicode(n).map(|u| (c, u.replace(' ', "")))).collect();
    let mut s = format!("%!PS-Adobe-3.0 Resource-CMap\n/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo\n<< /Registry (TeX)\n/Ordering ({font})\n/Supplement 0\n>> def\n/CMapName /TeX-{font}-0 def\n/CMapType 2 def\n1 begincodespacerange\n<00> <FF>\nendcodespacerange\n");
    for chunk in entries.chunks(100) {
        s.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (c, u) in chunk {
            s.push_str(&format!("<{c:02X}> <{u}>\n"));
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n%%EndResource\n%%EOF\n");
    s.into_bytes()
}

/// The four numbers after `key` in a font's cleartext (`/FontBBox {-40 -250 1009 750}`).
fn font_bbox(clear: &[u8]) -> [i32; 4] {
    let text = String::from_utf8_lossy(clear);
    let Some(at) = text.find("/FontBBox") else { return [0; 4] };
    let mut v = text[at + 9..].split(|c: char| !(c.is_ascii_digit() || c == '-' || c == '.')).filter(|t| !t.is_empty()).map(|t| t.parse::<f64>().unwrap_or(0.0) as i32);
    std::array::from_fn(|_| v.next().unwrap_or(0))
}

/// Looks up a glyph name's Unicode (space-separated UTF-16 hex), for ToUnicode maps.
pub type ToUnicode<'f> = Option<&'f dyn Fn(&str) -> Option<String>>;

/// What the document says about itself: `\pdfminorversion`, and the text of `\pdfinfo`
/// and `\pdfcatalog` (pdfTeX's `pdf_info_toks`, `pdf_catalog_toks`).
#[derive(Default)]
pub struct DocMeta<'m> {
    pub minor: i32,
    pub info: &'m str,
    /// Catalog entries (`\pdfcatalog` text, outlines, names), the document's own objects
    /// (annotations, destinations, outlines, `\pdfobj`), and the first object number free
    /// after them, where this writer's objects begin.
    pub catalog: &'m [u8],
    pub objects: &'m [(i32, Vec<u8>)],
    pub base: usize,
}

/// The PDF of `pages`; `fonts[i]` is engine font `i` when a page uses it. `files` reads
/// `pdftex.map`, encoding and font files by name; `to_unicode`, when pdfTeX's
/// `\pdfgentounicode` is on, names the Unicode of a glyph for the ToUnicode maps of fonts
/// read from the map file.
pub fn write(pages: &[ShippedPage], fonts: &[Option<DocFont>], files: &dyn Fn(&str) -> Option<Vec<u8>>, to_unicode: ToUnicode, meta: &DocMeta) -> Vec<u8> {
    let map = files("pdftex.map").map(|b| b.iter().map(|&c| char::from(c)).collect::<String>()).unwrap_or_default();
    let mut d = Doc { buf: Vec::with_capacity(1 << 16), offsets: vec![0] };
    // pdfTeX's header: `\pdfminorversion` (4 when unset or out of range).
    let minor = if (0..=9).contains(&meta.minor) && meta.minor != 0 { meta.minor } else { 4 };
    d.buf.extend_from_slice(b"%PDF-1.");
    print_int(&mut d.buf, minor as i64);
    d.buf.extend_from_slice(b"\n%\xD0\xD4\xC5\xD8\n");
    // The document's objects first, as numbered; then the catalog, the page tree, the
    // resources, one content stream per page, and the fonts.
    for (id, body) in meta.objects {
        d.begin(*id as usize);
        d.buf.extend_from_slice(body);
        d.end();
    }
    let (catalog, tree, resources) = (meta.base, meta.base + 1, meta.base + 2);
    let first_content = meta.base + 3;
    let mut next = first_content + pages.len();
    // One PDF font per file name, in order of the engine's fonts.
    let mut by_name: Vec<(&str, usize)> = Vec::new();
    let mut font_obj = vec![0usize; fonts.len()];
    for (i, f) in fonts.iter().enumerate() {
        let Some(f) = f else { continue };
        font_obj[i] = match by_name.iter().find(|(n, _)| *n == f.name) {
            Some(&(_, id)) => id,
            None => {
                let id = next;
                next += 4;
                by_name.push((&f.name, id));
                d.write_font(id, f, &map, files, to_unicode);
                id
            }
        };
    }
    // The fonts are shared by every page; a page that shows images has its own resources
    // with them added, as pdfTeX lists only a page's own XObjects (a viewer treats a page
    // whose resources hold a soft-masked image as transparent).
    let mut font_dict = b"<< /ProcSet [/PDF /Text] /Font <<".to_vec();
    for (i, &id) in font_obj.iter().enumerate() {
        if id != 0 {
            font_dict.extend_from_slice(b" /F");
            print_int(&mut font_dict, i as i64);
            font_dict.push(b' ');
            print_int(&mut font_dict, id as i64);
            font_dict.extend_from_slice(b" 0 R");
        }
    }
    font_dict.extend_from_slice(b" >>");
    d.begin(resources);
    d.buf.extend_from_slice(&font_dict);
    d.buf.extend_from_slice(b" >>\n");
    d.end();
    let mut deflate = crate::backend::backend_deflate::DeflateCompressor::new();
    let mut packed = Vec::new();
    for (k, p) in pages.iter().enumerate() {
        let content = first_content + k;
        d.begin(p.id as usize);
        d.buf.extend_from_slice(b"<< /Type /Page /Contents ");
        d.reference(content);
        d.buf.extend_from_slice(b" /Resources ");
        let own = if p.images.is_empty() {
            resources
        } else {
            next += 1;
            next - 1
        };
        d.reference(own);
        d.buf.extend_from_slice(b" /MediaBox [0 0 ");
        print_bp(&mut d.buf, p.width as i64);
        d.buf.push(b' ');
        print_bp(&mut d.buf, p.height as i64);
        d.buf.extend_from_slice(b"] /Parent ");
        d.reference(tree);
        if p.group != 0 {
            d.buf.extend_from_slice(b" /Group ");
            d.reference(p.group as usize);
        }
        if !p.annots.is_empty() {
            d.buf.extend_from_slice(b" /Annots [");
            for &a in &p.annots {
                d.buf.push(b' ');
                d.reference(a as usize);
            }
            d.buf.extend_from_slice(b" ]");
        }
        d.buf.extend_from_slice(b" >>\n");
        d.end();
        if own != resources {
            d.begin(own);
            d.buf.extend_from_slice(&font_dict);
            d.buf.extend_from_slice(b" /XObject <<");
            for &(n, obj) in &p.images {
                d.buf.extend_from_slice(b" /Im");
                print_int(&mut d.buf, n as i64);
                d.buf.push(b' ');
                d.reference(obj as usize);
            }
            d.buf.extend_from_slice(b" >> >>\n");
            d.end();
        }
        packed.clear();
        deflate.compress_zlib(&p.content, &mut packed);
        d.stream(content, b"", &packed, true);
    }
    d.begin(tree);
    d.buf.extend_from_slice(b"<< /Type /Pages /Count ");
    print_int(&mut d.buf, pages.len() as i64);
    d.buf.extend_from_slice(b" /Kids [");
    for p in pages {
        d.buf.push(b' ');
        d.reference(p.id as usize);
    }
    d.buf.extend_from_slice(b" ] >>\n");
    d.end();
    d.begin(catalog);
    d.buf.extend_from_slice(b"<< /Type /Catalog /Pages ");
    d.reference(tree);
    d.buf.push(b'\n');
    d.buf.extend_from_slice(meta.catalog);
    d.buf.extend_from_slice(b">>\n");
    d.end();
    // pdfTeX's info dictionary: `\pdfinfo` first, then `/Creator`, `/Producer` and
    // `/Trapped` unless given. The dates are left out: the output depends on no clock.
    let info = d.offsets.len();
    d.begin(info);
    d.buf.extend_from_slice(b"<< ");
    d.buf.extend_from_slice(&latin1(meta.info));
    for (key, value) in [("/Creator", "(TeX)"), ("/Producer", "(RushTeX)"), ("/Trapped", "/False")] {
        if !meta.info.contains(key) {
            d.buf.extend_from_slice(format!("{key} {value} ").as_bytes());
        }
    }
    d.buf.extend_from_slice(b">>\n");
    d.end();
    let xref = d.buf.len();
    let n = d.offsets.len();
    d.buf.extend_from_slice(b"xref\n0 ");
    print_int(&mut d.buf, n as i64);
    d.buf.extend_from_slice(b"\n0000000000 65535 f \n");
    for &o in &d.offsets[1..] {
        // Ids reserved for a font's descriptor or map that it does not need stay free.
        let kind = if o == 0 { "f" } else { "n" };
        d.buf.extend_from_slice(format!("{o:010} 00000 {kind} \n").as_bytes());
    }
    d.buf.extend_from_slice(b"trailer\n<< /Size ");
    print_int(&mut d.buf, n as i64);
    d.buf.extend_from_slice(b" /Root ");
    d.reference(catalog);
    d.buf.extend_from_slice(b" /Info ");
    d.reference(info);
    d.buf.extend_from_slice(b" >>\nstartxref\n");
    print_int(&mut d.buf, xref as i64);
    d.buf.extend_from_slice(b"\n%%EOF\n");
    d.buf
}

impl Doc {
    /// The font, its descriptor, its program and its ToUnicode map as objects `id` to
    /// `id + 3`.
    fn write_font(&mut self, id: usize, f: &DocFont, map: &str, files: &dyn Fn(&str) -> Option<Vec<u8>>, to_unicode: ToUnicode) {
        let program = crate::backend::backend_optical::program_by_name(&f.name);
        let entry = map_entry(map, &f.name);
        // A font from the map file carries its glyph names; its program is embedded whole.
        // With `\pdfgentounicode`, every font's ToUnicode comes from its glyph names (the
        // `.enc` file's, else the program's built-in encoding), as pdfTeX makes it.
        let enc = entry.as_ref().and_then(|e| e.enc).and_then(files).map(|b| b.iter().map(|&c| char::from(c)).collect::<String>());
        let wants_pfb = program.is_none() || (to_unicode.is_some() && enc.is_none());
        let pfb = if wants_pfb { entry.as_ref().and_then(|e| e.file).and_then(files).and_then(|b| pfb_segments(&b)) } else { None };
        // An embedded program is named by its own `/FontName`, as pdfTeX names it.
        let base = match (program, &entry, &pfb) {
            (Some(p), _, _) => p.name.to_string(),
            (None, _, Some((data, lens))) if font_name(&data[..lens[0]]).is_some() => font_name(&data[..lens[0]]).unwrap_or_default(),
            (None, Some(e), _) if !e.ps.is_empty() => e.ps.to_string(),
            _ => f.name.to_ascii_uppercase(),
        };
        let size = pdf_font_size(f.size);
        let first = f.widths.iter().position(|&w| w != 0).unwrap_or(0);
        let last = f.widths.iter().rposition(|&w| w != 0).unwrap_or(0);
        self.begin(id);
        self.buf.extend_from_slice(b"<< /Type /Font /Subtype /Type1 /BaseFont /");
        self.buf.extend_from_slice(base.as_bytes());
        self.buf.extend_from_slice(b" /FirstChar ");
        print_int(&mut self.buf, first as i64);
        self.buf.extend_from_slice(b" /LastChar ");
        print_int(&mut self.buf, last as i64);
        self.buf.extend_from_slice(b" /Widths [");
        for &w in &f.widths[first..=last] {
            self.buf.push(b' ');
            print_real(&mut self.buf, divide_scaled(w as i64, size, 4).0, 1);
        }
        self.buf.push(b']');
        if let Some(p) = program {
            if !p.diff_encoding.is_empty() {
                self.buf.extend_from_slice(b" /Encoding << /Type /Encoding /Differences [ ");
                self.buf.extend_from_slice(p.diff_encoding.as_bytes());
                self.buf.extend_from_slice(b" ] >>");
            }
            self.buf.extend_from_slice(b" /FontDescriptor ");
            self.reference(id + 1);
        }
        let mut cmap = None;
        if let Some(enc) = &enc {
            let names = encoding_names(enc);
            cmap = to_unicode.map(|u| tounicode_cmap(&f.name, &names, u));
            if program.is_none() {
                self.buf.extend_from_slice(b" /Encoding << /Type /Encoding /Differences [ 0");
                for n in &names {
                    self.buf.extend_from_slice(b" /");
                    self.buf.extend_from_slice(n.as_bytes());
                }
                self.buf.extend_from_slice(b" ] >>");
            }
        } else if let (Some(u), Some((data, lens))) = (to_unicode, &pfb) {
            let names = builtin_encoding_names(&data[..lens[0]]);
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            cmap = Some(tounicode_cmap(&f.name, &names, u));
        }
        let pfb = if program.is_none() { pfb } else { None };
        if pfb.is_some() {
            self.buf.extend_from_slice(b" /FontDescriptor ");
            self.reference(id + 1);
        }
        let map = if program.is_some() && cmap.is_none() { crate::backend::backend_optical::tounicode_by_name(&f.name) } else { None };
        if map.is_some() || cmap.is_some() {
            self.buf.extend_from_slice(b" /ToUnicode ");
            self.reference(id + 3);
        }
        self.buf.extend_from_slice(b" >>\n");
        self.end();
        if let Some(map) = map {
            self.stream(id + 3, b"", &crate::backend::backend_pdf::build_tounicode_cmap_bytes(map), false);
        }
        if let Some(cmap) = &cmap {
            self.stream(id + 3, b"", cmap, false);
        }
        if let Some((data, lens)) = pfb {
            let bbox = font_bbox(&data[..lens[0]]);
            self.begin(id + 1);
            self.buf.extend_from_slice(b"<< /Type /FontDescriptor /FontName /");
            self.buf.extend_from_slice(base.as_bytes());
            self.buf.extend_from_slice(format!(" /Flags 4 /FontBBox [{} {} {} {}] /Ascent {} /Descent {} /CapHeight {} /ItalicAngle 0 /StemV 80 /FontFile ", bbox[0], bbox[1], bbox[2], bbox[3], bbox[3], bbox[1], bbox[3]).as_bytes());
            self.reference(id + 2);
            self.buf.extend_from_slice(b" >>\n");
            self.end();
            let dict = format!(" /Length1 {} /Length2 {} /Length3 {}", lens[0], lens[1], lens[2]);
            self.stream(id + 2, dict.as_bytes(), &data, false);
            return;
        }
        let Some(p) = program else { return };
        self.begin(id + 1);
        self.buf.extend_from_slice(b"<< /Type /FontDescriptor /FontName /");
        self.buf.extend_from_slice(p.name.as_bytes());
        self.buf.extend_from_slice(format!(" /Flags {} /FontBBox [{} {} {} {}] /Ascent {} /Descent {} /CapHeight {} /ItalicAngle {} /StemV {} /FontFile3 ", p.flags, p.bbox[0], p.bbox[1], p.bbox[2], p.bbox[3], p.ascent, p.descent, p.cap_height, p.italic_angle, p.stem_v).as_bytes());
        self.reference(id + 2);
        self.buf.extend_from_slice(b" >>\n");
        self.end();
        self.stream(id + 2, b" /Subtype /Type1C", p.compressed_stream, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_vectors() {
        let lm = "% comment [not this]\n/enclmec[\n/grave\n/acute % note\n/a/b ] def\n";
        assert_eq!(encoding_names(lm), ["grave", "acute", "a", "b"]);
        assert_eq!(encoding_names("/T1Encoding [ /x /.notdef\n/y] def"), ["x", ".notdef", "y"]);
    }

    #[test]
    fn map_lines() {
        let map = "cmr10 CMR10 <cmr10.pfb\necrm1000 SFRM1000 \" T1Encoding ReEncodeFont \" <cm-super-t1.enc <sfrm1000.pfb\nmsam10 MSAM10 <[msam10.pfb\n";
        assert_eq!(map_entry(map, "cmr10"), Some(MapEntry { ps: "CMR10", enc: None, file: Some("cmr10.pfb") }));
        assert_eq!(map_entry(map, "ecrm1000"), Some(MapEntry { ps: "SFRM1000", enc: Some("cm-super-t1.enc"), file: Some("sfrm1000.pfb") }));
        assert_eq!(map_entry(map, "msam10").and_then(|e| e.file), Some("msam10.pfb"));
        assert_eq!(map_entry(map, "cmr1"), None);
        assert_eq!(encoding_names("% c\n/T1Encoding [\n/grave /acute % x\n/circumflex] def"), ["grave", "acute", "circumflex"]);
    }
}
