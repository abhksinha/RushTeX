//! The PDF document around pages the engine shipped out (`engine_ship`): page tree, fonts
//! and content streams. Fonts are found by their file name (`cmr10`), not by a fixed face
//! list, so any font the engine loaded is written: embedded when its program is in the
//! binary, otherwise as `pdftex.map` says (a Type 1 file and an encoding vector, read through
//! the caller's file resolver, as pdfTeX reads them), and by name when neither applies.
//! Several sizes of one font share one PDF font, and its `/Widths` come from the first size
//! used, as pdfTeX writes them.

use crate::backend::backend_pdftex::{divide_scaled, pdf_font_size, print_bp, print_int, print_real};
use crate::engine::engine_tounicode::{GlyphCode, GlyphUnicode};

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

/// Six capitals naming a subset of font `name` holding characters `used` (pdfTeX's
/// `make_subset_tag` draws them from an MD5 of the same; any stable hash names it).
fn subset_tag(name: &str, used: &[u64; 4]) -> String {
    let mut h = fnv(name.as_bytes());
    for w in used {
        h = (h ^ w).wrapping_mul(0x100_0000_01b3);
    }
    (0..6).map(|_| {
        let c = b'A' + (h % 26) as u8;
        h /= 26;
        char::from(c)
    }).collect()
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
    /// The characters the pages used (pdfTeX's `pdf_char_marked`), as a 256-bit set.
    pub used: [u64; 4],
    /// A font without a map entry: drawn from its PK bitmaps as a Type 3 font.
    pub pk: Option<PkFont>,
}

/// A bitmap font as shipped: its PK file's resolution, `get_pk_font_scale`, and the
/// characters the pages used (pdfTeX's `pdf_char_marked`), as a 256-bit set.
#[derive(Debug, Clone)]
pub struct PkFont {
    pub dpi: i32,
    pub scale: i64,
    pub used: [u64; 4],
}

/// Objects written so far and where each starts.
struct Doc {
    buf: Vec<u8>,
    offsets: Vec<usize>,
    /// With `\pdfobjcompresslevel` above 0 (PDF 1.5 on), the objects that are not streams
    /// go into object streams, as pdfTeX's `pdf_os` writes them.
    pack: Option<Pack>,
    deflate: crate::backend::backend_deflate::DeflateCompressor,
}

/// The packed objects' text, each object's number and where its text starts; `open` while
/// one is being written (its text then goes to `body`, swapped with the document's).
#[derive(Default)]
struct Pack {
    body: Vec<u8>,
    ids: Vec<usize>,
    starts: Vec<usize>,
    open: bool,
}

/// pdfTeX's `pdf_os_max_objs`: objects per object stream.
const OBJECTS_PER_STREAM: usize = 100;

impl Doc {
    /// Object `id`, packed into an object stream where the document packs them.
    fn begin(&mut self, id: usize) {
        let Some(p) = &mut self.pack else { return self.begin_direct(id) };
        if self.offsets.len() <= id {
            self.offsets.resize(id + 1, 0);
        }
        self.offsets[id] = usize::MAX;
        std::mem::swap(&mut self.buf, &mut p.body);
        p.ids.push(id);
        p.starts.push(self.buf.len());
        p.open = true;
    }

    /// Object `id` written in the file itself (a stream, or any object when not packing).
    fn begin_direct(&mut self, id: usize) {
        if self.offsets.len() <= id {
            self.offsets.resize(id + 1, 0);
        }
        self.offsets[id] = self.buf.len();
        print_int(&mut self.buf, id as i64);
        self.buf.extend_from_slice(b" 0 obj\n");
    }

    fn end(&mut self) {
        match &mut self.pack {
            Some(p) if p.open => {
                std::mem::swap(&mut self.buf, &mut p.body);
                p.open = false;
            }
            _ => self.buf.extend_from_slice(b"endobj\n"),
        }
    }

    /// `data` zlib-compressed.
    fn deflated(&mut self, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len() / 2 + 64);
        self.deflate.compress_zlib(data, &mut out);
        out
    }

    /// The packed objects as object streams of at most `OBJECTS_PER_STREAM` objects each;
    /// each packed object's (stream, index), by object number.
    fn write_object_streams(&mut self) -> Vec<(u32, u32)> {
        let Some(p) = self.pack.take() else { return Vec::new() };
        let mut at = vec![(0u32, 0u32); self.offsets.len()];
        let mut text = Vec::new();
        for (chunk, ids) in p.ids.chunks(OBJECTS_PER_STREAM).enumerate() {
            let first = chunk * OBJECTS_PER_STREAM;
            let from = p.starts[first];
            let to = p.starts.get(first + ids.len()).copied().unwrap_or(p.body.len());
            let id = self.offsets.len();
            text.clear();
            for (k, &obj) in ids.iter().enumerate() {
                print_int(&mut text, obj as i64);
                text.push(b' ');
                print_int(&mut text, (p.starts[first + k] - from) as i64);
                text.push(b' ');
                at[obj] = (id as u32, k as u32);
            }
            let head = text.len();
            text.extend_from_slice(&p.body[from..to]);
            let data = self.deflated(&text);
            let dict = format!(" /Type /ObjStm /N {} /First {head}", ids.len());
            self.stream(id, dict.as_bytes(), &data, true);
        }
        at
    }

    fn reference(&mut self, id: usize) {
        print_int(&mut self.buf, id as i64);
        self.buf.extend_from_slice(b" 0 R");
    }

    fn stream(&mut self, id: usize, dict: &[u8], data: &[u8], deflated: bool) {
        self.begin_direct(id);
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

/// The TFM names `pdftex.map` gives an entry, as sorted FNV-1a hashes: whether a font is
/// scalable (mapfile.c `isscalable`) is then a binary search, the map text not kept.
pub fn map_names(map: &[u8]) -> Vec<u64> {
    let mut names: Vec<u64> = map
        .split(|&c| c == b'\n' || c == b'\r')
        .filter_map(|l| l.split(|c| c.is_ascii_whitespace()).find(|t| !t.is_empty()))
        .filter(|n| !matches!(n[0], b'%' | b'#' | b'*' | b';'))
        .map(fnv)
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// FNV-1a.
pub fn fnv(s: &[u8]) -> u64 {
    s.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
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

/// An encoding file read and its glyph names found once per job, shared by the fonts that
/// name it (and by the threads that subset them): the names as spans of the file's text,
/// the `/Differences` entries, and the ToUnicode mappings of the fonts with no entries of
/// their own (`tfm:`), made by the first such font.
struct Encoding {
    text: String,
    spans: Vec<(u32, u32)>,
    differences: Vec<u8>,
    unicode_body: std::sync::OnceLock<Vec<u8>>,
}

impl Encoding {
    fn new(text: String) -> Encoding {
        let base = text.as_ptr() as usize;
        let spans: Vec<(u32, u32)> = encoding_names(&text).iter().map(|n| ((n.as_ptr() as usize - base) as u32, (n.as_ptr() as usize - base + n.len()) as u32)).collect();
        let mut differences = Vec::with_capacity(spans.len() * 10);
        for &(a, b) in &spans {
            differences.extend_from_slice(b" /");
            differences.extend_from_slice(&text.as_bytes()[a as usize..b as usize]);
        }
        Encoding { text, spans, differences, unicode_body: std::sync::OnceLock::new() }
    }

    fn names(&self) -> Vec<&str> {
        self.spans.iter().map(|&(a, b)| &self.text[a as usize..b as usize]).collect()
    }
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

use crate::data::data_embed::builtin_encoding_names;

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

/// pdfTeX's generated ToUnicode CMap (tounicode.c `write_tounicode`) for font `tfm` with
/// encoding file `enc` (`None`: the program's own encoding), around `body`
/// (`tounicode_body`): only the name `<tfm>-<enc>` differs between fonts.
fn tounicode_cmap(tfm: &str, enc: Option<&str>, body: &[u8]) -> Vec<u8> {
    let enc = enc.map_or("builtin", |e| e.strip_suffix(".enc").unwrap_or(e));
    let mut s = Vec::with_capacity(body.len() + 640);
    let id = |s: &mut Vec<u8>| {
        s.extend_from_slice(tfm.as_bytes());
        s.push(b'-');
        s.extend_from_slice(enc.as_bytes());
    };
    s.extend_from_slice(b"%!PS-Adobe-3.0 Resource-CMap\n%%DocumentNeededResources: ProcSet (CIDInit)\n%%IncludeResource: ProcSet (CIDInit)\n%%BeginResource: CMap (TeX-");
    id(&mut s);
    s.extend_from_slice(b"-0)\n%%Title: (TeX-");
    id(&mut s);
    s.extend_from_slice(b"-0 TeX ");
    id(&mut s);
    s.extend_from_slice(b" 0)\n%%Version: 1.000\n%%EndComments\n/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo\n<< /Registry (TeX)\n/Ordering (");
    id(&mut s);
    s.extend_from_slice(b")\n/Supplement 0\n>> def\n/CMapName /TeX-");
    id(&mut s);
    s.extend_from_slice(b"-0 def\n/CMapType 2 def\n1 begincodespacerange\n<00> <FF>\nendcodespacerange\n");
    s.extend_from_slice(body);
    s.extend_from_slice(b"endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n%%EndResource\n%%EOF\n");
    s
}

/// `digits` hex digits of `v`, in capitals.
fn push_hex(s: &mut Vec<u8>, v: u32, digits: u32) {
    for k in (0..digits).rev() {
        s.push(b"0123456789ABCDEF"[(v >> (4 * k) & 15) as usize]);
    }
}

/// `utf16be` written into `s`.
fn push_utf16be(s: &mut Vec<u8>, code: u32) {
    if code <= 0xFFFF {
        push_hex(s, code, 4);
    } else {
        let v = code - 0x10000;
        push_hex(s, v / 0x400 + 0xD800, 4);
        push_hex(s, v % 0x400 + 0xDC00, 4);
    }
}

/// The mappings of font `tfm`'s ToUnicode CMap, whose codes have glyph `names`: runs of
/// consecutive code points as `bfrange`s, the other codes as `bfchar`s, each in groups of
/// at most 100.
fn tounicode_body(tfm: &str, names: &[&str], unicode: &GlyphUnicode) -> Vec<u8> {
    let gtab: Vec<Option<GlyphCode>> = (0..257).map(|c| names.get(c).filter(|_| c < 256).and_then(|n| unicode.glyph_code(n, tfm))).collect();
    let code = |c: usize| match gtab[c] {
        Some(GlyphCode::Code(u)) => Some(u),
        _ => None,
    };
    // `range_size`: a run of codes whose code points follow one another, the last byte of
    // the first not carried past 255 (`is_last_byte_valid`).
    let mut size = [0usize; 256];
    let mut c = 0;
    while c < 256 {
        match code(c) {
            None => {
                size[c] = usize::from(gtab[c].is_some());
                c += 1;
            }
            Some(_) => {
                let j = c;
                while c < 256 && code(c + 1).is_some_and(|n| code(c) == Some(n.wrapping_sub(1))) && (code(c).unwrap_or(0) & 0xFF) < (255 - (c - j)) as u32 {
                    c += 1;
                }
                c += 1;
                size[j] = c - j;
            }
        }
    }
    let starts = |single: bool| {
        let mut v = Vec::new();
        let mut c = 0;
        while c < 256 {
            if (size[c] == 1) == single && size[c] > 0 {
                v.push(c);
            }
            c += size[c].max(1);
        }
        v
    };
    let (ranges, chars) = (starts(false), starts(true));
    let mut s = Vec::with_capacity(4096);
    // Each kind is written at least once, as `0 beginbfrange` when there is none.
    let groups = |v: &[usize]| if v.is_empty() { vec![Vec::new()] } else { v.chunks(100).map(<[usize]>::to_vec).collect() };
    for group in groups(&ranges) {
        print_int(&mut s, group.len() as i64);
        s.extend_from_slice(b" beginbfrange\n");
        for &c in &group {
            s.push(b'<');
            push_hex(&mut s, c as u32, 2);
            s.extend_from_slice(b"> <");
            push_hex(&mut s, (c + size[c] - 1) as u32, 2);
            s.extend_from_slice(b"> <");
            push_utf16be(&mut s, code(c).unwrap_or(0));
            s.extend_from_slice(b">\n");
        }
        s.extend_from_slice(b"endbfrange\n");
    }
    for group in groups(&chars) {
        print_int(&mut s, group.len() as i64);
        s.extend_from_slice(b" beginbfchar\n");
        for &c in &group {
            s.push(b'<');
            push_hex(&mut s, c as u32, 2);
            s.extend_from_slice(b"> <");
            match &gtab[c] {
                Some(GlyphCode::Seq(q)) => s.extend_from_slice(q.as_bytes()),
                Some(GlyphCode::Code(u)) => push_utf16be(&mut s, *u),
                None => {}
            }
            s.extend_from_slice(b">\n");
        }
        s.extend_from_slice(b"endbfchar\n");
    }
    s
}

/// The four numbers after `key` in a font's cleartext (`/FontBBox {-40 -250 1009 750}`).
fn font_bbox(clear: &[u8]) -> [i32; 4] {
    let text = String::from_utf8_lossy(clear);
    let Some(at) = text.find("/FontBBox") else { return [0; 4] };
    let mut v = text[at + 9..].split(|c: char| !(c.is_ascii_digit() || c == '-' || c == '.')).filter(|t| !t.is_empty()).map(|t| t.parse::<f64>().unwrap_or(0.0) as i32);
    std::array::from_fn(|_| v.next().unwrap_or(0))
}

/// The table that gives a font's glyph names their Unicode (tounicode.c
/// `set_glyph_unicode`), for ToUnicode maps.
pub type ToUnicode<'f> = Option<&'f GlyphUnicode>;

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
    /// `\pdfobjcompresslevel`: above 0, objects other than streams are packed into object
    /// streams (with PDF 1.5 or later).
    pub objcompress: i32,
    /// The shipped forms.
    pub forms: &'m [ShippedForm],
}

/// Content bytes each compressing worker gets at least (about 8 pages, 0.25 ms on one
/// core). Measured on `docs/master.tex` (260 KB of streams): 0.65 ms on one thread, 0.30 ms
/// on four, 0.26 ms on eight; a spawn costs about 0.15M instructions, so four of 64 KB.
#[cfg(not(target_arch = "wasm32"))]
const PARALLEL_MIN_BYTES: usize = 1 << 16;

/// Fonts each worker gets at least: a font takes 0.05-0.2 ms to subset and compress, a
/// spawn about 0.15M instructions. 6 against 4: 0.4-0.8% fewer instructions on 26-font
/// documents for the same wall time; 8 gives no more.
#[cfg(not(target_arch = "wasm32"))]
const FONTS_PER_WORKER: usize = 6;

/// A Type 1 font's parts: read and named in order (`font_work`, which calls the file
/// resolver and the ToUnicode lookup), subset and compressed on any thread
/// (`FontWork::prepare`), then written in order (`Doc::write_font`).
struct FontWork<'f> {
    f: &'f DocFont,
    /// The built-in program, when the map file gives none.
    program: Option<&'static crate::backend::backend_cm_fonts::CmFontProgram>,
    base: String,
    enc: Option<std::sync::Arc<Encoding>>,
    /// Whether the font names its glyphs by the encoding's `/Differences`.
    differences: bool,
    stored_names: Option<Vec<u8>>,
    layout: Option<Vec<u8>>,
    italic: i32,
    /// The map file's program to embed, decrypted, and its segments' lengths.
    pfb: Option<(Vec<u8>, [usize; 3])>,
    /// The ToUnicode map, compressed by `prepare`.
    unicode: Option<Vec<u8>>,
    /// The embedded program: its stream, `/Length1` and `/Length2`, and `/FontBBox`.
    embedded: Option<(Vec<u8>, [usize; 2], [i32; 4])>,
}

/// Font `f`'s files read and its names, encoding and ToUnicode map made: everything that
/// needs the file resolver, in the fonts' order.
///
/// `encodings` keeps the encoding files read so far, by name.
fn font_work<'f>(f: &'f DocFont, map: &std::collections::HashMap<&str, &str>, files: &dyn Fn(&str) -> Option<Vec<u8>>, to_unicode: ToUnicode, encodings: &mut Vec<(String, std::sync::Arc<Encoding>)>) -> FontWork<'f> {
    let entry = map.get(f.name.as_str()).map(|l| map_entry(l));
    // The map file's Type 1 program, decrypted: `#clear` where the file layer has it,
    // else the file decrypted here. It is subset as pdfTeX subsets it, so a font the
    // map names is embedded from its own file even when a whole built-in program exists.
    let clear = |file: &str| {
        files(&format!("{file}#clear")).and_then(|b| pfb_segments(&b)).or_else(|| {
            let (mut data, lens) = pfb_segments(&files(file)?)?;
            crate::data::data_embed::decrypt_binary(&mut data[lens[0]..lens[0] + lens[1]]);
            Some((data, lens))
        })
    };
    let own = entry.as_ref().and_then(|e| e.file).and_then(clear);
    let program = if own.is_some() { None } else { crate::backend::backend_optical::program_by_name(&f.name) };
    // A font from the map file carries its glyph names; its program is embedded whole.
    // With `\pdfgentounicode`, every font's ToUnicode comes from its glyph names (the
    // `.enc` file's, else the program's built-in encoding), as pdfTeX makes it.
    let enc_name = entry.as_ref().and_then(|e| e.enc);
    let enc = enc_name.and_then(|name| {
        if let Some((_, e)) = encodings.iter().find(|(k, _)| k == name) {
            return Some(e.clone());
        }
        let e = std::sync::Arc::new(Encoding::new(files(name)?.iter().map(|&c| char::from(c)).collect()));
        encodings.push((name.to_string(), e.clone()));
        Some(e)
    });
    // A built-in program's glyph names, stored as text where the file layer has them
    // (`X.pfb#names`): the `.pfb` itself is not read then.
    // A program read from its file needs them too, to name the glyphs it keeps.
    let stored_names = if enc.is_none() && (program.is_none() || to_unicode.is_some()) { entry.as_ref().and_then(|e| e.file).and_then(|f| files(&format!("{f}#names"))) } else { None };
    let wants_pfb = program.is_none() || (to_unicode.is_some() && enc.is_none() && stored_names.is_none());
    // With a program of its own, only the file's clear text is used (its encoding):
    // `#head` asks for that alone, where the file layer can give it (`data_embed`).
    // Embedded, the program is subset (`backend_type1`) from its decrypted form: `#clear`
    // asks for that where the file layer has it, else the file is decrypted here.
    let read_head = |file: &str| files(&format!("{file}#head")).or_else(|| files(file)).and_then(|b| pfb_segments(&b));
    let mut pfb = match program {
        None => own,
        Some(_) if wants_pfb => entry.as_ref().and_then(|e| e.file).and_then(read_head),
        Some(_) => None,
    };
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
    // A subset is named with a tag of six capitals from the glyphs it holds (PDF 1.7
    // §5.5.3), as pdfTeX's `make_subset_tag` names it.
    let base = if program.is_none() && pfb.is_some() { format!("{}+{base}", subset_tag(&base, &f.used)) } else { base };
    let mut cmap = None;
    // A font from the map file with an encoding names its glyphs by `/Differences`.
    let differences = enc.is_some() && program.is_none();
    if let (Some(e), Some(u)) = (&enc, to_unicode) {
        // Fonts with the same encoding and no entries of their own share the mappings.
        cmap = Some(if u.has_font_entries(&f.name) {
            tounicode_cmap(&f.name, enc_name, &tounicode_body(&f.name, &e.names(), u))
        } else {
            tounicode_cmap(&f.name, enc_name, e.unicode_body.get_or_init(|| tounicode_body(&f.name, &e.names(), u)))
        });
    } else if let (Some(u), Some(text)) = (to_unicode, &stored_names) {
        let names: Vec<&str> = text.split(|&c| c == b'\n').map(|l| if l.is_empty() { ".notdef" } else { std::str::from_utf8(l).unwrap_or(".notdef") }).collect();
        cmap = Some(tounicode_cmap(&f.name, None, &tounicode_body(&f.name, &names, u)));
    } else if let (Some(u), Some((data, lens))) = (to_unicode, &pfb) {
        let names = builtin_encoding_names(&data[..lens[0]]);
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        cmap = Some(tounicode_cmap(&f.name, None, &tounicode_body(&f.name, &names, u)));
    }
    let pfb = if program.is_none() { pfb } else { None };
    let map = if program.is_some() && cmap.is_none() { crate::backend::backend_optical::tounicode_by_name(&f.name) } else { None };
    let unicode = map.map(crate::backend::backend_pdf::build_tounicode_cmap_bytes).or(cmap);
    // The layout `rushtex-gen` stored with a carried font, else the program is indexed when
    // it is subset.
    let layout = if pfb.is_some() { entry.as_ref().and_then(|e| e.file).and_then(|file| files(&format!("{file}#layout"))) } else { None };
    FontWork { f, program, base, enc, differences, stored_names, layout, italic, pfb, unicode, embedded: None }
}

impl FontWork<'_> {
    /// The ToUnicode map compressed, and the map file's program subset as pdfTeX subsets it
    /// (`writet1.c`), encrypted and compressed.
    fn prepare(&mut self, deflate: &mut crate::backend::backend_deflate::DeflateCompressor) {
        if let Some(text) = &mut self.unicode {
            let mut out = Vec::with_capacity(text.len() / 2 + 64);
            deflate.compress_zlib(text, &mut out);
            *text = out;
        }
        let Some((mut data, mut lens)) = self.pfb.take() else { return };
        let f = self.f;
        let used = |c: usize| f.used[c / 64] >> (c % 64) & 1 != 0;
        // pdfTeX's subset (`writet1.c`): the glyphs the pages used, by the encoding's
        // names (the `.enc` file's, else the program's own).
        // Borrowed from the encoding or the stored names: no string is made per glyph.
        let names: Vec<std::borrow::Cow<str>> = match (&self.enc, &self.stored_names) {
            (Some(enc), _) => enc.names().into_iter().map(std::borrow::Cow::Borrowed).collect(),
            (None, Some(text)) => text.split(|&c| c == b'\n').map(|l| if l.is_empty() { std::borrow::Cow::Borrowed(".notdef") } else { String::from_utf8_lossy(l) }).collect(),
            (None, None) => builtin_encoding_names(&data[..lens[0]]).into_iter().map(std::borrow::Cow::Owned).collect(),
        };
        let used: Vec<&str> = (0..256).filter(|&c| used(c)).filter_map(|c| names.get(c).map(|n| n.as_ref())).collect();
        let tag = self.base.split('+').next().unwrap_or_default();
        let clear = crate::backend::backend_type1::subset_clear(&data[..lens[0]], &|c: usize| f.used[c / 64] >> (c % 64) & 1 != 0, tag);
        data.splice(..lens[0], clear.iter().copied());
        lens[0] = clear.len();
        let binary = lens[0]..lens[0] + lens[1];
        // A subset comes with its charstrings encrypted; a font kept whole is encrypted
        // here, charstrings and eexec.
        let layout = self.layout.as_ref().and_then(|b| crate::backend::backend_type1::Index::from_u32s(&b.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect::<Vec<_>>()));
        let subset = match &layout {
            Some(ix) => crate::backend::backend_type1::subset_with(&data[binary.clone()], ix, &used, true),
            None => crate::backend::backend_type1::subset(&data[binary.clone()], &used),
        };
        let text = match subset {
            Some(mut text) => {
                crate::data::data_embed::encrypt_eexec(&mut text);
                text
            }
            // A carried font with a layout has its charstrings encrypted already.
            None if layout.is_some() => {
                let mut text = data[binary.clone()].to_vec();
                crate::data::data_embed::encrypt_eexec(&mut text);
                text
            }
            None => {
                let mut text = data[binary.clone()].to_vec();
                crate::data::data_embed::encrypt_binary(&mut text);
                text
            }
        };
        lens[1] = text.len();
        data.splice(binary, text);
        let bbox = font_bbox(&data[..lens[0]]);
        // The clear text is compressed; the eexec part, pseudo-random, is stored as is.
        let mut packed = Vec::with_capacity(data.len() + 64);
        deflate.compress_zlib_parts(&data[..lens[0]], &data[lens[0]..lens[0] + lens[1]], &mut packed);
        self.embedded = Some((packed, [lens[0], lens[1]], bbox));
    }
}

/// Every font's `prepare`, and `streams` (the pages' and forms' contents) compressed as
/// zlib streams, in order. For many fonts or a long document both are split over the same
/// workers: worker `k` prepares the `k`-th chunk of fonts and compresses the `k`-th run of
/// streams (contiguous, about equal bytes), with its own compressor (its 32 KB table stays
/// in that core's L1). Each font's and stream's bytes depend only on its own input, so the
/// file equals a serial run's.
fn prepare_all(fonts: &mut [&mut FontWork], streams: &[&[u8]]) -> Vec<Vec<u8>> {
    use crate::backend::backend_deflate::DeflateCompressor;
    let serial = |fonts: &mut [&mut FontWork], run: &[&[u8]]| {
        let mut deflate = DeflateCompressor::new();
        fonts.iter_mut().for_each(|w| w.prepare(&mut deflate));
        run.iter().map(|s| {
            let mut out = Vec::with_capacity(s.len() / 3 + 16);
            deflate.compress_zlib(s, &mut out);
            out
        }).collect::<Vec<_>>()
    };
    let total: usize = streams.iter().map(|s| s.len()).sum();
    #[cfg(target_arch = "wasm32")]
    let workers = 1;
    #[cfg(not(target_arch = "wasm32"))]
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get()).min((total / PARALLEL_MIN_BYTES).min(streams.len()).max(fonts.len() / FONTS_PER_WORKER));
    if workers <= 1 {
        return serial(fonts, streams);
    }
    // Run `k` ends at the first stream whose end passes `k + 1` workers' share of the
    // bytes; the last run takes the rest, and runs past the streams are empty.
    let mut runs: Vec<&[&[u8]]> = Vec::with_capacity(workers);
    let (mut start, mut sum) = (0, 0);
    for (i, s) in streams.iter().enumerate() {
        sum += s.len();
        if runs.len() + 1 < workers && sum * workers >= total * (runs.len() + 1) {
            runs.push(&streams[start..=i]);
            start = i + 1;
        }
    }
    runs.push(&streams[start..]);
    runs.resize(workers, &[]);
    let mut chunks = fonts.chunks_mut(fonts.len().div_ceil(workers).max(1));
    let mut jobs = runs.into_iter().map(|run| (chunks.next().unwrap_or_default(), run));
    let (first_fonts, first_run) = jobs.next().unwrap();
    std::thread::scope(|scope| {
        let handles: Vec<_> = jobs.map(|(fonts, run)| scope.spawn(move || serial(fonts, run))).collect();
        let mut out = serial(first_fonts, first_run);
        for h in handles {
            out.extend(h.join().unwrap());
        }
        out
    })
}

/// The PDF of `pages`; `fonts[i]` is engine font `i` when a page uses it. `files` reads
/// `pdftex.map`, encoding and font files by name; `to_unicode`, when pdfTeX's
/// `\pdfgentounicode` is on, names the Unicode of a glyph for the ToUnicode maps of fonts
/// read from the map file.
pub fn write(pages: &[ShippedPage], fonts: &[Option<DocFont>], files: &dyn Fn(&str) -> Option<Vec<u8>>, to_unicode: ToUnicode, meta: &DocMeta) -> Vec<u8> {
    let map = files("pdftex.map").map(|b| b.iter().map(|&c| char::from(c)).collect::<String>()).unwrap_or_default();
    let map = map_index(&map);
    // pdfTeX's header: `\pdfminorversion` (4 when unset or out of range).
    let minor = if (0..=9).contains(&meta.minor) && meta.minor != 0 { meta.minor } else { 4 };
    // Object streams need PDF 1.5 (pdftex.web `pdf_os_enable`).
    let pack = (meta.objcompress > 0 && minor >= 5).then(Pack::default);
    let mut d = Doc { buf: Vec::with_capacity(1 << 16), offsets: vec![0], pack, deflate: Default::default() };
    d.buf.extend_from_slice(b"%PDF-1.");
    print_int(&mut d.buf, minor as i64);
    d.buf.extend_from_slice(b"\n%\xD0\xD4\xC5\xD8\n");
    // The document's objects first, as numbered; then the catalog, the page tree, the
    // resources, one content stream per page, and the fonts.
    for (id, body) in meta.objects {
        // A `\pdfobj stream` is a stream: never packed.
        if body.windows(7).any(|w| w == b"stream\n" || w == b"stream\r") {
            d.begin_direct(*id as usize);
        } else {
            d.begin(*id as usize);
        }
        d.buf.extend_from_slice(body);
        d.end();
    }
    let (catalog, tree, resources) = (meta.base, meta.base + 1, meta.base + 2);
    let first_content = meta.base + 3;
    let mut next = first_content + pages.len();
    // One PDF font per given font, in order of the engine's fonts: the caller passes only
    // the font each file name's other sizes share (`engine_ship`'s `font_base`).
    let mut font_obj = vec![0usize; fonts.len()];
    // Type 1 fonts' files read in order; their programs subset and every stream compressed
    // (`prepare_all`, on several threads); then every object written in order.
    let mut encodings = Vec::new();
    let mut work: Vec<Option<FontWork>> = fonts.iter().map(|f| f.as_ref().filter(|f| f.pk.is_none()).map(|f| font_work(f, &map, files, to_unicode, &mut encodings))).collect();
    let streams: Vec<&[u8]> = pages.iter().map(|p| &p.content[..]).chain(meta.forms.iter().map(|f| &f.content[..])).collect();
    let mut packed = prepare_all(&mut work.iter_mut().flatten().collect::<Vec<_>>(), &streams).into_iter();
    for (i, f) in fonts.iter().enumerate() {
        let Some(f) = f else { continue };
        font_obj[i] = next;
        if let Some(pk) = &f.pk {
            let data = files(&format!("{}.{}pk", f.name, pk.dpi)).unwrap_or_default();
            next = d.write_type3(next, i, f, pk, &data);
            continue;
        }
        if let Some(w) = &work[i] {
            d.write_font(next, w);
        }
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
        d.stream(content, b"", &packed.next().unwrap(), true);
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
        d.stream(f.id as usize, &dict, &packed.next().unwrap(), true);
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
    if d.pack.is_some() {
        let at = d.write_object_streams();
        // The cross-reference stream (PDF 1.5 §3.4.7): type 0 free, 1 at an offset, 2 in an
        // object stream; field widths as the largest offset needs.
        let id = d.offsets.len();
        let xref = d.buf.len();
        d.offsets.push(xref);
        let n = id + 1;
        let w = (1..=8).find(|&b| b == 8 || xref >> (8 * b) == 0).unwrap_or(8);
        let mut rows = Vec::with_capacity(n * (w + 3));
        for (i, &o) in d.offsets.iter().enumerate() {
            let (kind, field, index) = match o {
                usize::MAX => (2u8, at[i].0 as usize, at[i].1 as u16),
                0 if i != id => (0, 0, if i == 0 { 65535 } else { 0 }),
                o => (1, o, 0),
            };
            rows.push(kind);
            rows.extend_from_slice(&field.to_be_bytes()[8 - w..]);
            rows.extend_from_slice(&index.to_be_bytes());
        }
        let data = d.deflated(&rows);
        let dict = format!(" /Type /XRef /Size {n} /W [1 {w} 2] /Root {catalog} 0 R /Info {info} 0 R");
        d.stream(id, dict.as_bytes(), &data, true);
        d.buf.extend_from_slice(b"startxref\n");
        print_int(&mut d.buf, xref as i64);
        d.buf.extend_from_slice(b"\n%%EOF\n");
        return d.buf;
    }
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
    /// writet3.c `writepk` and `writet3`: font `f` (engine font `n`) as a Type 3 font of its
    /// PK characters, the font dictionary at `id`, one glyph procedure per character after
    /// it; the next free object.
    fn write_type3(&mut self, id: usize, n: usize, f: &DocFont, pk: &PkFont, data: &[u8]) -> usize {
        use crate::backend::backend_pk::{pk_char_width, pk_chars};
        let chars = pk_chars(data, &pk.used).unwrap_or_default();
        let size = pdf_font_size(f.size);
        let mut widths = [0i64; 256];
        let mut procs = [0usize; 256];
        let mut bbox = [0i64; 4];
        let mut next = id + 1;
        let mut glyph = Vec::new();
        let mut packed = Vec::new();
        let mut deflate = crate::backend::backend_deflate::DeflateCompressor::new();
        for (k, c) in chars.iter().enumerate() {
            let w = pk_char_width(f.widths[c.code as usize] as i64, size, pk.scale);
            widths[c.code as usize] = w;
            let null = c.width < 1 || c.height < 1;
            let (cw, ch, xoff, yoff) = if null { ((w as f64 / 100.0).round() as i64, 1, 0, 0) } else { (c.width as i64, c.height as i64, c.xoff as i64, c.yoff as i64) };
            let (llx, lly) = (-xoff, yoff - ch + 1);
            let (urx, ury) = (cw + llx + 1, ch + lly);
            // `update_bbox`
            bbox = if k == 0 { [llx, lly, urx, ury] } else { [bbox[0].min(llx), bbox[1].min(lly), bbox[2].max(urx), bbox[3].max(ury)] };
            glyph.clear();
            print_real(&mut glyph, w, 2);
            glyph.extend_from_slice(format!(" 0 {llx} {lly} {urx} {ury} d1\n").as_bytes());
            if !null {
                glyph.extend_from_slice(format!("q\n{cw} 0 0 {ch} {llx} {lly} cm\nBI\n/W {cw}\n/H {ch}\n/IM true\n/BPC 1\n/D [1 0]\nID ").as_bytes());
                let (bytes, words) = (((cw + 7) / 8) as usize, ((cw + 15) / 16) as usize);
                for row in c.raster.chunks(words) {
                    for (j, &word) in row.iter().enumerate() {
                        glyph.push((word >> 8) as u8);
                        if j + 1 < words || 2 * words == bytes {
                            glyph.push(word as u8);
                        }
                    }
                }
                glyph.extend_from_slice(b"\nEI\nQ\n");
            }
            packed.clear();
            deflate.compress_zlib(&glyph, &mut packed);
            self.stream(next, b"", &packed, true);
            procs[c.code as usize] = next;
            next += 1;
        }
        let bit = |i: usize| pk.used[i / 64] >> (i % 64) & 1 == 1;
        let first = (0..256).find(|&i| bit(i)).unwrap_or(0);
        let last = (first..256).rev().find(|&i| bit(i)).unwrap_or(first);
        self.begin(id);
        self.buf.extend_from_slice(b"<< /Type /Font /Subtype /Type3 /Name /F");
        print_int(&mut self.buf, n as i64);
        self.buf.extend_from_slice(b" /FontMatrix [");
        print_real(&mut self.buf, pk.scale, 5);
        self.buf.extend_from_slice(b" 0 0 ");
        print_real(&mut self.buf, pk.scale, 5);
        self.buf.extend_from_slice(format!(" 0 0] /FontBBox [ {} {} {} {} ] /Resources << /ProcSet [ /PDF /ImageB ] >> /FirstChar {first} /LastChar {last} /Widths [", bbox[0], bbox[1], bbox[2], bbox[3]).as_bytes());
        for &w in &widths[first..=last] {
            print_real(&mut self.buf, w, 2);
            self.buf.push(b' ');
        }
        // The encoding names each character with a procedure `/a<code>`, `.notdef` between.
        self.buf.extend_from_slice(b"] /Encoding << /Type /Encoding /Differences [");
        print_int(&mut self.buf, first as i64);
        let mut notdef = false;
        for (i, &p) in procs.iter().enumerate().take(last + 1).skip(first) {
            if p == 0 {
                if !notdef || i == first {
                    if i != first {
                        self.buf.push(b' ');
                        print_int(&mut self.buf, i as i64);
                    }
                    self.buf.extend_from_slice(b"/.notdef");
                    notdef = true;
                }
            } else {
                if notdef {
                    self.buf.push(b' ');
                    print_int(&mut self.buf, i as i64);
                    notdef = false;
                }
                self.buf.extend_from_slice(format!("/a{i}").as_bytes());
            }
        }
        self.buf.extend_from_slice(b"] >> /CharProcs <<");
        for (i, &p) in procs.iter().enumerate() {
            if p != 0 {
                self.buf.extend_from_slice(format!(" /a{i} ").as_bytes());
                self.reference(p);
            }
        }
        self.buf.extend_from_slice(b" >> >>\n");
        self.end();
        next
    }

    fn write_font(&mut self, id: usize, w: &FontWork) {
        let f = w.f;
        let used = |c: usize| f.used[c / 64] >> (c % 64) & 1 != 0;
        let size = pdf_font_size(f.size);
        // `/Widths` from the first to the last character used, 0 for those not used, as
        // pdfTeX's `write_fontdict` writes them.
        let (first, last) = match ((0..256).find(|&c| used(c)), (0..256).rev().find(|&c| used(c))) {
            (Some(a), Some(b)) => (a, b),
            _ => (0, 0),
        };
        self.begin(id);
        self.buf.extend_from_slice(b"<< /Type /Font /Subtype /Type1 /BaseFont /");
        self.buf.extend_from_slice(w.base.as_bytes());
        self.buf.extend_from_slice(b" /FirstChar ");
        print_int(&mut self.buf, first as i64);
        self.buf.extend_from_slice(b" /LastChar ");
        print_int(&mut self.buf, last as i64);
        self.buf.extend_from_slice(b" /Widths [");
        for (c, &wd) in f.widths.iter().enumerate().take(last + 1).skip(first) {
            self.buf.push(b' ');
            print_real(&mut self.buf, if used(c) { divide_scaled(wd as i64, size, 4).0 } else { 0 }, 1);
        }
        self.buf.push(b']');
        if let Some(p) = w.program {
            if !p.diff_encoding.is_empty() {
                self.buf.extend_from_slice(b" /Encoding << /Type /Encoding /Differences [ ");
                self.buf.extend_from_slice(p.diff_encoding.as_bytes());
                self.buf.extend_from_slice(b" ] >>");
            }
            self.buf.extend_from_slice(b" /FontDescriptor ");
            self.reference(id + 1);
        }
        if let Some(enc) = w.enc.as_ref().filter(|_| w.differences) {
            self.buf.extend_from_slice(b" /Encoding << /Type /Encoding /Differences [ 0");
            self.buf.extend_from_slice(&enc.differences);
            self.buf.extend_from_slice(b" ] >>");
        }
        if w.embedded.is_some() {
            self.buf.extend_from_slice(b" /FontDescriptor ");
            self.reference(id + 1);
        }
        if w.unicode.is_some() {
            self.buf.extend_from_slice(b" /ToUnicode ");
            self.reference(id + 3);
        }
        self.buf.extend_from_slice(b" >>\n");
        self.end();
        if let Some(data) = &w.unicode {
            self.stream(id + 3, b"", data, true);
        }
        if let Some((packed, lens, bbox)) = &w.embedded {
            self.begin(id + 1);
            self.buf.extend_from_slice(b"<< /Type /FontDescriptor /FontName /");
            self.buf.extend_from_slice(w.base.as_bytes());
            self.buf.extend_from_slice(format!(" /Flags 4 /FontBBox [{} {} {} {}] /Ascent {} /Descent {} /CapHeight {} /ItalicAngle {} /StemV 80 /FontFile ", bbox[0], bbox[1], bbox[2], bbox[3], bbox[3], bbox[1], bbox[3], w.italic).as_bytes());
            self.reference(id + 2);
            self.buf.extend_from_slice(b" >>\n");
            self.end();
            // pdfTeX writes no trailer segment (the zeros and `cleartomark`): `/Length3 0`.
            let dict = format!(" /Length1 {} /Length2 {} /Length3 0", lens[0], lens[1]);
            self.stream(id + 2, dict.as_bytes(), packed, true);
            return;
        }
        let Some(p) = w.program else { return };
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

    #[test]
    fn streams_compress_alike_on_threads() {
        // Long enough for several workers, with an empty stream first, last and between,
        // and one stream bigger than a worker's share.
        let page = |k: usize| (0..9000 + 37 * k).map(|i| b"BT /F1 9.96 Tf 0 -11.96 Td [(text)-333(more)]TJ ET\n"[i % 51] ^ (k as u8 & 3)).collect::<Vec<u8>>();
        let mut pages: Vec<Vec<u8>> = (0..40).map(page).collect();
        pages[0].clear();
        pages[17].clear();
        pages[20] = pages[20].repeat(30);
        pages.push(Vec::new());
        let streams: Vec<&[u8]> = pages.iter().map(|p| &p[..]).collect();
        let mut deflate = crate::backend::backend_deflate::DeflateCompressor::new();
        let serial: Vec<Vec<u8>> = streams.iter().map(|s| {
            let mut out = Vec::new();
            deflate.compress_zlib(s, &mut out);
            out
        }).collect();
        assert_eq!(prepare_all(&mut [], &streams), serial);
    }
}
