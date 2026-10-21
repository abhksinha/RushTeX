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
    /// The forms it shows as (`/Fm<n>`, object).
    pub forms: Vec<(u32, i32)>,
}

/// A shipped `\pdfxform` (pdftex.web `pdf_ship_out` of a form): its object, its box's width
/// and height plus depth, content, the images and forms it shows, and its `attr` and
/// `resources` text.
#[derive(Debug, Clone, Default)]
pub struct ShippedForm {
    pub id: i32,
    pub width: i32,
    pub height: i32,
    pub content: Vec<u8>,
    pub images: Vec<(u32, i32)>,
    pub forms: Vec<(u32, i32)>,
    pub attr: Vec<u8>,
    pub resources: Vec<u8>,
}

/// pdfTeX's `/XObject` resources: forms, then images.
fn xobjects(buf: &mut Vec<u8>, forms: &[(u32, i32)], images: &[(u32, i32)]) {
    buf.extend_from_slice(b" /XObject <<");
    for (prefix, list) in [(&b" /Fm"[..], forms), (b" /Im", images)] {
        for &(n, obj) in list {
            buf.extend_from_slice(prefix);
            print_int(buf, n as i64);
            buf.push(b' ');
            print_int(buf, obj as i64);
            buf.extend_from_slice(b" 0 R");
        }
    }
    buf.extend_from_slice(b" >>");
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

/// A font's line in `pdftex.map`: its PostScript name, encoding file and font file, and
/// its `SlantFont` and `ExtendFont` effects in thousandths (0 for none).
#[derive(Debug, Default, PartialEq)]
struct MapEntry<'m> {
    ps: &'m str,
    enc: Option<&'m str>,
    file: Option<&'m str>,
    slant: i32,
    extend: i32,
}

/// `pdftex.map`'s lines by TFM name, indexed once per document instead of searched per font.
fn map_index(map: &str) -> std::collections::HashMap<&str, &str> {
    let mut index = std::collections::HashMap::with_capacity(map.len() / 48);
    for l in map.lines() {
        if let Some(name) = l.split_whitespace().next().filter(|n| !n.starts_with(['%', '#', '*', ';'])) {
            // mapfile.c keeps the first entry of a name.
            index.entry(name).or_insert(l);
        }
    }
    index
}

/// mapfile.c: a number before `SlantFont` or `ExtendFont`, read as a C `float`, in
/// thousandths rounded half away from zero.
fn effect(number: &str) -> Option<i32> {
    let d = number.parse::<f32>().ok()? * 1000.0;
    Some(if d > 0.0 { d + 0.5 } else { d - 0.5 } as i32)
}

/// The `pdftex.map` line of a TFM (pdftexdir/mapfile.c: the TFM name, then the PostScript
/// name unless the next field is a file or a quoted special, then `<file`s).
fn map_entry<'m>(line: &'m str) -> MapEntry<'m> {
    let mut e = MapEntry::default();
    let mut quoted = false;
    let mut prev = "";
    for (i, t) in line.split_whitespace().enumerate().skip(1) {
        // A quoted special (`" .167 SlantFont "`): only the effects are read from it.
        let quotes = t.matches('"').count();
        if quoted || quotes > 0 {
            quoted ^= quotes % 2 == 1;
            let word = t.trim_matches('"');
            match word {
                "SlantFont" => e.slant = effect(prev).unwrap_or(0),
                "ExtendFont" => e.extend = effect(prev).unwrap_or(0),
                _ => {}
            }
            prev = word;
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
    e
}

/// C's `printf("%g")` of a `float` value: six significant digits, trailing zeros dropped,
/// exponent form below 1e-4 or from 1e6.
fn fmt_g(v: f32) -> String {
    let v = v as f64;
    if v == 0.0 {
        return "0".to_string();
    }
    let sci = format!("{v:.5e}");
    let (mant, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let trim = |s: String| if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if !(-4..6).contains(&exp) {
        format!("{}e{}{:02}", trim(mant.to_string()), if exp < 0 { '-' } else { '+' }, exp.abs())
    } else {
        trim(format!("{:.*}", (5 - exp) as usize, v))
    }
}

/// writet1.c's `t1_scan_keys` for a font with effects: `/FontMatrix` slanted, then
/// extended (`t1_modify_fm`), `/ItalicAngle` less the slant's angle (`t1_modify_italic`), and
/// `-Slant_<s>`/`-Extend_<e>` added to `/FontName`. Lines are rewritten in place, in
/// `float`s as writet1.c computes them; the new cleartext and the italic angle.
fn t1_effects(clear: &[u8], slant: i32, extend: i32) -> (Vec<u8>, Option<i32>) {
    let mut out = Vec::with_capacity(clear.len() + 32);
    let mut angle = None;
    for line in clear.split_inclusive(|&c| c == b'\n' || c == b'\r') {
        if let Some(rest) = line.strip_prefix(b"/FontMatrix") {
            let Some(open) = rest.iter().position(|&c| c == b'[' || c == b'{') else {
                out.extend_from_slice(line);
                continue;
            };
            let close = if rest[open] == b'[' { b']' } else { b'}' };
            let end = rest.iter().position(|&c| c == close).unwrap_or(rest.len());
            let text = String::from_utf8_lossy(&rest[open + 1..end]).into_owned();
            let mut a = [0f32; 6];
            for (k, n) in text.split_whitespace().take(6).enumerate() {
                a[k] = n.parse().unwrap_or(0.0);
            }
            if slant != 0 {
                let s = slant as f64 * 1e-3;
                for (x, y) in [(0, 1), (2, 3), (4, 5)] {
                    a[x] = (a[x] as f64 + a[y] as f64 * s) as f32;
                }
            }
            if extend != 0 {
                let e = extend as f64 * 1e-3;
                for x in [0, 2, 4] {
                    a[x] = (a[x] as f64 * e) as f32;
                }
            }
            out.extend_from_slice(b"/FontMatrix");
            out.extend_from_slice(&rest[..=open]);
            for v in a {
                out.extend_from_slice(fmt_g(v).as_bytes());
                out.push(b' ');
            }
            out.extend_from_slice(&rest[end..]);
        } else if line.starts_with(b"/ItalicAngle") && slant != 0 {
            let sp = line.iter().position(|&c| c == b' ').unwrap_or(line.len() - 1);
            let tail = &line[sp + 1..];
            let len = tail.iter().position(|&c| !(c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.'))).unwrap_or(tail.len());
            let a0: f32 = std::str::from_utf8(&tail[..len]).ok().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let a = (a0 as f64 - (slant as f64 * 1e-3).atan() * (180.0 / std::f64::consts::PI)) as f32;
            out.extend_from_slice(&line[..=sp]);
            out.extend_from_slice(fmt_g(a).as_bytes());
            out.extend_from_slice(&tail[len..]);
            angle = Some((a as f64).round() as i32);
        } else if let Some(rest) = line.strip_prefix(b"/FontName") {
            let start = rest.iter().position(|&c| c == b'/').map_or(0, |k| k + 1);
            let len = rest[start..].iter().position(|&c| c == b' ' || c == b'\n' || c == b'\r').unwrap_or(rest.len() - start);
            out.extend_from_slice(b"/FontName");
            out.extend_from_slice(&rest[..start + len]);
            if slant != 0 {
                out.extend_from_slice(format!("-Slant_{slant}").as_bytes());
            }
            if extend != 0 {
                out.extend_from_slice(format!("-Extend_{extend}").as_bytes());
            }
            out.extend_from_slice(&rest[start + len..]);
        } else {
            out.extend_from_slice(line);
        }
    }
    (out, angle)
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
    /// The shipped forms.
    pub forms: &'m [ShippedForm],
}

/// The PDF of `pages`; `fonts[i]` is engine font `i` when a page uses it. `files` reads
/// `pdftex.map`, encoding and font files by name; `to_unicode`, when pdfTeX's
/// `\pdfgentounicode` is on, names the Unicode of a glyph for the ToUnicode maps of fonts
/// read from the map file.
pub fn write(pages: &[ShippedPage], fonts: &[Option<DocFont>], files: &dyn Fn(&str) -> Option<Vec<u8>>, to_unicode: ToUnicode, meta: &DocMeta) -> Vec<u8> {
    let map = files("pdftex.map").map(|b| b.iter().map(|&c| char::from(c)).collect::<String>()).unwrap_or_default();
    let map = map_index(&map);
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
    // One PDF font per given font, in order of the engine's fonts: the caller passes only
    // the font each file name's other sizes share (`engine_ship`'s `font_base`).
    let mut font_obj = vec![0usize; fonts.len()];
    for (i, f) in fonts.iter().enumerate() {
        let Some(f) = f else { continue };
        font_obj[i] = next;
        d.write_font(next, f, &map, files, to_unicode);
        next += 4;
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
        let own = if p.images.is_empty() && p.forms.is_empty() {
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
            xobjects(&mut d.buf, &p.forms, &p.images);
            d.buf.extend_from_slice(b" >>\n");
            d.end();
        }
        packed.clear();
        deflate.compress_zlib(&p.content, &mut packed);
        d.stream(content, b"", &packed, true);
    }
    // pdfTeX's `Write out Form stream header`, the resources given inline.
    let mut dict = Vec::new();
    for f in meta.forms {
        dict.clear();
        dict.extend_from_slice(b"\n/Type /XObject\n/Subtype /Form\n");
        if !f.attr.is_empty() {
            dict.extend_from_slice(&f.attr);
            dict.push(b'\n');
        }
        dict.extend_from_slice(b"/BBox [0 0 ");
        print_bp(&mut dict, f.width as i64);
        dict.push(b' ');
        print_bp(&mut dict, f.height as i64);
        dict.extend_from_slice(b"]\n/FormType 1\n/Matrix [1 0 0 1 0 0]\n/Resources ");
        dict.extend_from_slice(&font_dict);
        if !f.resources.is_empty() {
            dict.push(b'\n');
            dict.extend_from_slice(&f.resources);
            dict.push(b'\n');
        }
        if !f.images.is_empty() || !f.forms.is_empty() {
            xobjects(&mut dict, &f.forms, &f.images);
        }
        dict.extend_from_slice(b" >>\n");
        packed.clear();
        deflate.compress_zlib(&f.content, &mut packed);
        d.stream(f.id as usize, &dict, &packed, true);
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
    fn write_font(&mut self, id: usize, f: &DocFont, map: &std::collections::HashMap<&str, &str>, files: &dyn Fn(&str) -> Option<Vec<u8>>, to_unicode: ToUnicode) {
        let program = crate::backend::backend_optical::program_by_name(&f.name);
        let entry = map.get(f.name.as_str()).map(|l| map_entry(l));
        // A font from the map file carries its glyph names; its program is embedded whole.
        // With `\pdfgentounicode`, every font's ToUnicode comes from its glyph names (the
        // `.enc` file's, else the program's built-in encoding), as pdfTeX makes it.
        let enc = entry.as_ref().and_then(|e| e.enc).and_then(files).map(|b| b.iter().map(|&c| char::from(c)).collect::<String>());
        let wants_pfb = program.is_none() || (to_unicode.is_some() && enc.is_none());
        let mut pfb = if wants_pfb { entry.as_ref().and_then(|e| e.file).and_then(files).and_then(|b| pfb_segments(&b)) } else { None };
        // A map entry's `SlantFont`/`ExtendFont` change the embedded program (writet1.c).
        let mut italic = 0;
        if let (Some(e), Some((data, lens))) = (&entry, &mut pfb) {
            if e.slant != 0 || e.extend != 0 {
                let (clear, angle) = t1_effects(&data[..lens[0]], e.slant, e.extend);
                italic = angle.unwrap_or(0);
                data.splice(..lens[0], clear.iter().copied());
                lens[0] = clear.len();
            }
        }
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
            self.buf.extend_from_slice(format!(" /Flags 4 /FontBBox [{} {} {} {}] /Ascent {} /Descent {} /CapHeight {} /ItalicAngle {} /StemV 80 /FontFile ", bbox[0], bbox[1], bbox[2], bbox[3], bbox[3], bbox[1], bbox[3], italic).as_bytes());
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
        let map = "cmr10 CMR10 <cmr10.pfb\necrm1000 SFRM1000 \" T1Encoding ReEncodeFont \" <cm-super-t1.enc <sfrm1000.pfb\nmsam10 MSAM10 <[msam10.pfb\nfvsro8t BeraSerif-Roman \".167 SlantFont\" <fvsr8a.pfb\n";
        let index = map_index(map);
        let entry = |n: &str| index.get(n).map(|l| map_entry(l));
        assert_eq!(entry("cmr10"), Some(MapEntry { ps: "CMR10", enc: None, file: Some("cmr10.pfb"), ..Default::default() }));
        assert_eq!(entry("ecrm1000"), Some(MapEntry { ps: "SFRM1000", enc: Some("cm-super-t1.enc"), file: Some("sfrm1000.pfb"), ..Default::default() }));
        assert_eq!(entry("msam10").and_then(|e| e.file), Some("msam10.pfb"));
        assert_eq!(entry("fvsro8t").map(|e| (e.slant, e.extend)), Some((167, 0)));
        assert_eq!(entry("cmr1"), None);
        // writet1.c's rewrites, numbers as C's `%g` of a `float`.
        let (clear, angle) = t1_effects(b"/FontName /BeraSerif-Roman def\n/ItalicAngle 0 def\n/FontMatrix [0.001 0 0 0.001 0 0]readonly def\n", 167, 0);
        assert_eq!(String::from_utf8_lossy(&clear), "/FontName /BeraSerif-Roman-Slant_167 def\n/ItalicAngle -9.4809 def\n/FontMatrix [0.001 0 0.000167 0.001 0 0 ]readonly def\n");
        assert_eq!(angle, Some(-9));
        assert_eq!(encoding_names("% c\n/T1Encoding [\n/grave /acute % x\n/circumflex] def"), ["grave", "acute", "circumflex"]);
    }
}
