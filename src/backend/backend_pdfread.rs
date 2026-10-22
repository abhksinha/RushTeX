//! A small PDF reader for including pages as images (pdfTeX's `\pdfximage` of a PDF, which
//! pdfTeX does with xpdf in pdftoepdf.cc): the cross-reference table or stream (with `/Prev`
//! and `/XRefStm`), object streams, `FlateDecode` with PNG predictors, and the page tree
//! with inherited attributes (xpdf `Page.cc` `PageAttrs`).
//!
//! The file stays one byte buffer. Objects are parsed when asked for; a stream keeps the
//! range of its raw bytes, which are copied to the output undecoded. Only object streams
//! and cross-reference streams are ever decoded, each once.

use std::collections::HashMap;
use std::ops::Range;

/// A PDF object.
#[derive(Clone, Debug, PartialEq)]
pub enum Obj {
    Null,
    Bool(bool),
    Int(i64),
    Real(f64),
    Str(Vec<u8>),
    Name(Vec<u8>),
    Array(Vec<Obj>),
    Dict(Dict),
    Ref(u32, u16),
    /// A stream's dictionary and the range of its raw data in the file.
    Stream(Dict, Range<usize>),
}

/// A dictionary, in the file's key order (xpdf keeps it, and pdfTeX copies in that order).
pub type Dict = Vec<(Vec<u8>, Obj)>;

/// The value of `key` in `d`.
pub fn get<'d>(d: &'d Dict, key: &[u8]) -> Option<&'d Obj> {
    d.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

impl Obj {
    pub fn as_dict(&self) -> Option<&Dict> {
        match self {
            Obj::Dict(d) | Obj::Stream(d, _) => Some(d),
            _ => None,
        }
    }

    pub fn as_num(&self) -> Option<f64> {
        match *self {
            Obj::Int(i) => Some(i as f64),
            Obj::Real(r) => Some(r),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match *self {
            Obj::Int(i) => Some(i),
            _ => None,
        }
    }
}

/// Where object `n` lives.
#[derive(Clone, Copy, Debug)]
enum Entry {
    Free,
    At(usize),
    /// In object stream `.0`, at index `.1`.
    InStream(u32, u32),
}

fn is_white(c: u8) -> bool {
    matches!(c, 0 | 9 | 10 | 12 | 13 | 32)
}

fn is_delim(c: u8) -> bool {
    matches!(c, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

/// A parser over a byte buffer.
struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Lexer<'a> {
    fn skip(&mut self) {
        while let Some(&c) = self.s.get(self.i) {
            if is_white(c) {
                self.i += 1;
            } else if c == b'%' {
                while self.s.get(self.i).is_some_and(|&c| c != b'\n' && c != b'\r') {
                    self.i += 1;
                }
            } else {
                break;
            }
        }
    }

    fn word(&mut self) -> &'a [u8] {
        let start = self.i;
        while self.s.get(self.i).is_some_and(|&c| !is_white(c) && !is_delim(c)) {
            self.i += 1;
        }
        &self.s[start..self.i]
    }

    fn keyword(&mut self, k: &[u8]) -> bool {
        self.skip();
        if self.s[self.i.min(self.s.len())..].starts_with(k) {
            self.i += k.len();
            true
        } else {
            false
        }
    }

    /// One object; `n R` references are recognised by looking ahead.
    fn obj(&mut self) -> Option<Obj> {
        self.skip();
        let c = *self.s.get(self.i)?;
        match c {
            b'/' => {
                self.i += 1;
                let raw = self.word();
                let mut name = Vec::with_capacity(raw.len());
                let mut k = 0;
                while k < raw.len() {
                    if raw[k] == b'#' && k + 3 <= raw.len() {
                        if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&raw[k + 1..k + 3]).ok()?, 16) {
                            name.push(v);
                            k += 3;
                            continue;
                        }
                    }
                    name.push(raw[k]);
                    k += 1;
                }
                Some(Obj::Name(name))
            }
            b'(' => self.literal(),
            b'<' if self.s.get(self.i + 1) == Some(&b'<') => {
                self.i += 2;
                let mut d = Dict::new();
                loop {
                    self.skip();
                    if self.s.get(self.i..self.i + 2)? == b">>" {
                        self.i += 2;
                        break;
                    }
                    let Obj::Name(k) = self.obj()? else { return None };
                    let v = self.obj()?;
                    d.push((k, v));
                }
                Some(Obj::Dict(d))
            }
            b'<' => {
                self.i += 1;
                let mut out = Vec::new();
                let mut hi: Option<u8> = None;
                loop {
                    let c = *self.s.get(self.i)?;
                    self.i += 1;
                    if c == b'>' {
                        break;
                    }
                    let Some(v) = (c as char).to_digit(16) else { continue };
                    match hi.take() {
                        None => hi = Some(v as u8),
                        Some(h) => out.push(h << 4 | v as u8),
                    }
                }
                if let Some(h) = hi {
                    out.push(h << 4);
                }
                Some(Obj::Str(out))
            }
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                loop {
                    self.skip();
                    if *self.s.get(self.i)? == b']' {
                        self.i += 1;
                        break;
                    }
                    a.push(self.obj()?);
                }
                Some(Obj::Array(a))
            }
            _ => {
                let w = self.word();
                if w.is_empty() {
                    self.i += 1;
                    return None;
                }
                match w {
                    b"true" => return Some(Obj::Bool(true)),
                    b"false" => return Some(Obj::Bool(false)),
                    b"null" => return Some(Obj::Null),
                    _ => {}
                }
                let text = std::str::from_utf8(w).ok()?;
                if w.contains(&b'.') {
                    return text.parse::<f64>().ok().or_else(|| if text == "." || text == "-." { Some(0.0) } else { None }).map(Obj::Real);
                }
                let n: i64 = text.parse().ok()?;
                // `n g R`
                let save = self.i;
                self.skip();
                let g = self.word();
                if !g.is_empty() && g.iter().all(u8::is_ascii_digit) {
                    self.skip();
                    if self.s.get(self.i) == Some(&b'R') && self.s.get(self.i + 1).is_none_or(|&c| is_white(c) || is_delim(c)) {
                        self.i += 1;
                        let g: u16 = std::str::from_utf8(g).ok()?.parse().ok()?;
                        return Some(Obj::Ref(n as u32, g));
                    }
                }
                self.i = save;
                Some(Obj::Int(n))
            }
        }
    }

    fn literal(&mut self) -> Option<Obj> {
        self.i += 1;
        let mut out = Vec::new();
        let mut depth = 1;
        loop {
            let c = *self.s.get(self.i)?;
            self.i += 1;
            match c {
                b'(' => {
                    depth += 1;
                    out.push(c);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(c);
                }
                b'\\' => {
                    let e = *self.s.get(self.i)?;
                    self.i += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'\r' => {
                            if self.s.get(self.i) == Some(&b'\n') {
                                self.i += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut v = (e - b'0') as u32;
                            for _ in 0..2 {
                                match self.s.get(self.i) {
                                    Some(&d @ b'0'..=b'7') => {
                                        v = v * 8 + (d - b'0') as u32;
                                        self.i += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(v as u8);
                        }
                        _ => out.push(e),
                    }
                }
                _ => out.push(c),
            }
        }
        Some(Obj::Str(out))
    }
}

/// An open PDF file.
pub struct PdfFile {
    data: Vec<u8>,
    xref: Vec<Entry>,
    pub trailer: Dict,
    /// Decoded object streams: their data and the offsets of their objects.
    streams: HashMap<u32, (Vec<u8>, Vec<usize>)>,
}

/// A page with its inherited attributes resolved (xpdf `PageAttrs`).
pub struct Page {
    pub dict: Dict,
    pub resources: Option<Dict>,
    /// Media, crop, bleed, trim and art boxes, as `[x1, y1, x2, y2]` in C `float`.
    pub boxes: [[f32; 4]; 5],
    pub rotate: i64,
}

impl PdfFile {
    pub fn open(data: Vec<u8>) -> Option<PdfFile> {
        let mut f = PdfFile { data, xref: Vec::new(), trailer: Dict::new(), streams: HashMap::new() };
        let tail = &f.data[f.data.len().saturating_sub(1024)..];
        let k = tail.windows(9).rposition(|w| w == b"startxref")?;
        let mut lx = Lexer { s: tail, i: k + 9 };
        let start = lx.obj()?.as_int()? as usize;
        let mut seen = Vec::new();
        let mut next = Some(start);
        while let Some(at) = next.take() {
            if seen.contains(&at) || seen.len() > 64 {
                break;
            }
            seen.push(at);
            let trailer = f.read_xref(at)?;
            if let Some(stm) = get(&trailer, b"XRefStm").and_then(Obj::as_int) {
                f.read_xref(stm as usize);
            }
            next = get(&trailer, b"Prev").and_then(Obj::as_int).map(|p| p as usize);
            if f.trailer.is_empty() {
                f.trailer = trailer;
            }
        }
        Some(f)
    }

    /// Records the entries of the section at `at` that no newer section gave.
    fn set(&mut self, n: usize, e: Entry) {
        if self.xref.len() <= n {
            self.xref.resize(n + 1, Entry::Free);
            self.xref[n] = e;
        } else if matches!(self.xref[n], Entry::Free) {
            self.xref[n] = e;
        }
    }

    /// One cross-reference section (table or stream); its trailer dictionary.
    fn read_xref(&mut self, at: usize) -> Option<Dict> {
        let mut lx = Lexer { s: &self.data, i: at };
        if lx.keyword(b"xref") {
            let mut entries = Vec::new();
            loop {
                lx.skip();
                if lx.keyword(b"trailer") {
                    break;
                }
                let first = lx.obj()?.as_int()? as usize;
                let count = lx.obj()?.as_int()? as usize;
                for k in 0..count {
                    let off = lx.obj()?.as_int()? as usize;
                    let _gen = lx.obj()?;
                    lx.skip();
                    let kind = lx.word();
                    entries.push((first + k, if kind == b"n" { Entry::At(off) } else { Entry::Free }));
                }
            }
            let trailer = lx.obj()?.as_dict()?.clone();
            for (n, e) in entries {
                if !matches!(e, Entry::Free) {
                    self.set(n, e);
                } else if self.xref.len() <= n {
                    self.xref.resize(n + 1, Entry::Free);
                }
            }
            return Some(trailer);
        }
        // A cross-reference stream: `n g obj << /Type /XRef ... >> stream`.
        let Obj::Stream(d, range) = self.parse_at(at)? else { return None };
        let data = decode(&d, &self.data[range])?;
        let w: Vec<usize> = match get(&d, b"W")? {
            Obj::Array(a) => a.iter().map(|v| v.as_int().unwrap_or(0) as usize).collect(),
            _ => return None,
        };
        let size = get(&d, b"Size").and_then(Obj::as_int).unwrap_or(0);
        let index: Vec<i64> = match get(&d, b"Index") {
            Some(Obj::Array(a)) => a.iter().filter_map(Obj::as_int).collect(),
            _ => vec![0, size],
        };
        let row = w.iter().sum::<usize>();
        let field = |r: &[u8], k: usize| -> u64 {
            let start: usize = w[..k].iter().sum();
            r[start..start + w[k]].iter().fold(0u64, |v, &b| v << 8 | b as u64)
        };
        let mut rows = data.chunks_exact(row.max(1));
        for pair in index.chunks(2) {
            let [first, count] = [pair[0], *pair.get(1).unwrap_or(&0)];
            for k in 0..count {
                let Some(r) = rows.next() else { break };
                let kind = if w[0] == 0 { 1 } else { field(r, 0) };
                let e = match kind {
                    1 => Entry::At(field(r, 1) as usize),
                    2 => Entry::InStream(field(r, 1) as u32, field(r, 2) as u32),
                    _ => Entry::Free,
                };
                let n = (first + k) as usize;
                if !matches!(e, Entry::Free) {
                    self.set(n, e);
                } else if self.xref.len() <= n {
                    self.xref.resize(n + 1, Entry::Free);
                }
            }
        }
        Some(d)
    }

    /// The object `n g obj ... endobj` at byte `at`.
    fn parse_at(&self, at: usize) -> Option<Obj> {
        let mut lx = Lexer { s: &self.data, i: at };
        lx.obj()?;
        lx.obj()?;
        if !lx.keyword(b"obj") {
            return None;
        }
        let o = lx.obj()?;
        let Obj::Dict(d) = o else { return Some(o) };
        if !lx.keyword(b"stream") {
            return Some(Obj::Dict(d));
        }
        // The data starts after the end of line following `stream`.
        if self.data.get(lx.i) == Some(&b'\r') {
            lx.i += 1;
        }
        if self.data.get(lx.i) == Some(&b'\n') {
            lx.i += 1;
        }
        let start = lx.i;
        let len = match get(&d, b"Length") {
            Some(Obj::Int(n)) => Some(*n as usize),
            Some(Obj::Ref(n, _)) => self.get(*n).and_then(|o| o.as_int()).map(|n| n as usize),
            _ => None,
        };
        let end = match len.filter(|&l| self.data.get(start + l..).is_some_and(endstream_follows)) {
            Some(l) => start + l,
            // A wrong /Length: up to `endstream`, less its end of line.
            None => {
                let k = self.data[start..].windows(9).position(|w| w == b"endstream")? + start;
                let mut e = k;
                if e > start && self.data[e - 1] == b'\n' {
                    e -= 1;
                }
                if e > start && self.data[e - 1] == b'\r' {
                    e -= 1;
                }
                e
            }
        };
        Some(Obj::Stream(d, start..end))
    }

    /// Object number `n` (generation ignored, as xpdf finds objects by number).
    pub fn get(&self, n: u32) -> Option<Obj> {
        match *self.xref.get(n as usize)? {
            Entry::Free => None,
            Entry::At(at) => self.parse_at(at),
            Entry::InStream(s, k) => {
                let (data, offs) = self.streams.get(&s)?;
                let mut lx = Lexer { s: data, i: *offs.get(k as usize)? };
                lx.obj()
            }
        }
    }

    /// Decodes the object streams the cross-reference names, once each.
    pub fn load_object_streams(&mut self) {
        let mut wanted: Vec<u32> = self.xref.iter().filter_map(|e| if let Entry::InStream(s, _) = e { Some(*s) } else { None }).collect();
        wanted.sort_unstable();
        wanted.dedup();
        for s in wanted {
            let Some(Obj::Stream(d, range)) = self.get(s) else { continue };
            let Some(data) = decode(&d, &self.data[range]) else { continue };
            let n = get(&d, b"N").and_then(Obj::as_int).unwrap_or(0) as usize;
            let first = get(&d, b"First").and_then(Obj::as_int).unwrap_or(0) as usize;
            let mut lx = Lexer { s: &data, i: 0 };
            let mut offs = Vec::with_capacity(n);
            for _ in 0..n {
                let (Some(_), Some(off)) = (lx.obj(), lx.obj().and_then(|o| o.as_int())) else { break };
                offs.push(first + off as usize);
            }
            self.streams.insert(s, (data, offs));
        }
    }

    /// `o` with a reference followed.
    pub fn resolve(&self, o: &Obj) -> Obj {
        match o {
            Obj::Ref(n, _) => self.get(*n).unwrap_or(Obj::Null),
            _ => o.clone(),
        }
    }

    /// The raw bytes of a stream.
    pub fn raw(&self, range: &Range<usize>) -> &[u8] {
        &self.data[range.clone()]
    }

    fn catalog(&self) -> Option<Dict> {
        self.resolve(get(&self.trailer, b"Root")?).as_dict().cloned()
    }

    /// The number of pages (the root `/Pages` node's `/Count`).
    pub fn page_count(&self) -> i64 {
        self.catalog()
            .and_then(|c| get(&c, b"Pages").map(|p| self.resolve(p)))
            .and_then(|p| p.as_dict().and_then(|d| get(d, b"Count")).and_then(|c| self.resolve(c).as_int()))
            .unwrap_or(0)
    }

    /// Page `n` (from 1), with the attributes it inherits; its object number in `.1`.
    pub fn page(&self, n: i64) -> Option<(Page, u32)> {
        let root = get(&self.catalog()?, b"Pages")?.clone();
        let mut node = root;
        let mut skip = n - 1;
        let mut inherited: [Option<Obj>; 4] = [None, None, None, None];
        const KEYS: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];
        for _ in 0..64 {
            let num = if let Obj::Ref(r, _) = node { Some(r) } else { None };
            let d = self.resolve(&node).as_dict()?.clone();
            for (k, key) in KEYS.iter().enumerate() {
                if let Some(v) = get(&d, key) {
                    inherited[k] = Some(v.clone());
                }
            }
            let is_pages = get(&d, b"Kids").is_some();
            if !is_pages {
                if skip != 0 {
                    return None;
                }
                return Some((self.page_attrs(d, &inherited), num.unwrap_or(0)));
            }
            let Obj::Array(kids) = self.resolve(get(&d, b"Kids")?) else { return None };
            let mut next = None;
            for kid in kids {
                let kd = self.resolve(&kid);
                let count = kd.as_dict().and_then(|d| if get(d, b"Kids").is_some() { get(d, b"Count").and_then(|c| self.resolve(c).as_int()) } else { Some(1) }).unwrap_or(1);
                if skip < count {
                    next = Some(kid);
                    break;
                }
                skip -= count;
            }
            node = next?;
        }
        None
    }

    fn page_attrs(&self, dict: Dict, inherited: &[Option<Obj>; 4]) -> Page {
        let rect = |o: Option<&Obj>| -> Option<[f32; 4]> {
            let Obj::Array(a) = self.resolve(o?) else { return None };
            if a.len() != 4 {
                return None;
            }
            let v: Vec<f64> = a.iter().map(|x| self.resolve(x).as_num()).collect::<Option<_>>()?;
            // xpdf `readBox`: clamped and ordered.
            let c = |x: f64| x.clamp(-1e9, 1e9) as f32;
            let (x1, x2) = if v[0] <= v[2] { (c(v[0]), c(v[2])) } else { (c(v[2]), c(v[0])) };
            let (y1, y2) = if v[1] <= v[3] { (c(v[1]), c(v[3])) } else { (c(v[3]), c(v[1])) };
            Some([x1, y1, x2, y2])
        };
        let media = rect(get(&dict, b"MediaBox").or(inherited[1].as_ref())).unwrap_or([0.0, 0.0, 612.0, 792.0]);
        // xpdf `clipBoxes`: the other boxes clipped to the media box.
        let clip = |b: [f32; 4]| [b[0].max(media[0]), b[1].max(media[1]), b[2].min(media[2]), b[3].min(media[3])];
        let crop = rect(get(&dict, b"CropBox").or(inherited[2].as_ref())).map(clip).unwrap_or(media);
        let other = |key: &[u8]| rect(get(&dict, key)).map(clip).unwrap_or(crop);
        let boxes = [media, crop, other(b"BleedBox"), other(b"TrimBox"), other(b"ArtBox")];
        let rotate = get(&dict, b"Rotate").or(inherited[3].as_ref()).map(|r| self.resolve(r)).and_then(|r| r.as_int()).unwrap_or(0).rem_euclid(360);
        let resources = get(&dict, b"Resources").or(inherited[0].as_ref()).and_then(|r| self.resolve(r).as_dict().cloned());
        Page { dict, resources, boxes, rotate }
    }
}

fn endstream_follows(r: &[u8]) -> bool {
    let mut i = 0;
    while r.get(i).is_some_and(|&c| is_white(c)) {
        i += 1;
    }
    r[i.min(r.len())..].starts_with(b"endstream")
}

/// The decoded data of a stream: `FlateDecode` (with a PNG or TIFF-less predictor) or none.
pub fn decode(d: &Dict, raw: &[u8]) -> Option<Vec<u8>> {
    let filter = match get(d, b"Filter") {
        None => return Some(raw.to_vec()),
        Some(Obj::Name(n)) => n.clone(),
        Some(Obj::Array(a)) if a.len() == 1 => match &a[0] {
            Obj::Name(n) => n.clone(),
            _ => return None,
        },
        Some(Obj::Array(a)) if a.is_empty() => return Some(raw.to_vec()),
        _ => return None,
    };
    if filter != b"FlateDecode" {
        return None;
    }
    let mut out = Vec::new();
    super::backend_inflate::zlib_decompress(raw, &mut out)?;
    let parms = match get(d, b"DecodeParms") {
        Some(Obj::Dict(p)) => p.clone(),
        Some(Obj::Array(a)) => a.first().and_then(|p| p.as_dict().cloned()).unwrap_or_default(),
        _ => return Some(out),
    };
    let int = |k: &[u8], dflt: i64| get(&parms, k).and_then(Obj::as_int).unwrap_or(dflt);
    let predictor = int(b"Predictor", 1);
    if predictor < 10 {
        return Some(out);
    }
    let (colors, bits, columns) = (int(b"Colors", 1) as usize, int(b"BitsPerComponent", 8) as usize, int(b"Columns", 1) as usize);
    let stride = (colors * bits * columns).div_ceil(8);
    let bpp = (colors * bits).div_ceil(8).max(1);
    let rows = out.len() / (stride + 1);
    let mut at = 0;
    super::backend_png::unfilter(&out, &mut at, rows, stride, bpp)
}

/// pdftoepdf.cc `convertNumToPDF`: a real with at most six decimals, trailing zeros and a
/// bare dot dropped, below 0.5e-6 in magnitude `0`.
fn print_real(out: &mut Vec<u8>, mut n: f64) {
    const EPS: f64 = 0.5e-6;
    if n.abs() < EPS {
        out.push(b'0');
        return;
    }
    if n < 0.0 {
        out.push(b'-');
        n = -n;
    }
    n += EPS;
    let i = n.floor() as i64;
    out.extend_from_slice(i.to_string().as_bytes());
    let mut f = ((n - i as f64) * 1e6).floor() as i64;
    if f != 0 {
        let mut digits = 6;
        while f % 10 == 0 {
            f /= 10;
            digits -= 1;
        }
        out.push(b'.');
        out.extend_from_slice(format!("{f:0digits$}").as_bytes());
    }
}

/// `sprintf("%.8f")` through utils.c `stripzeros`.
fn print_fixed8(out: &mut Vec<u8>, x: f64) {
    let s = format!("{x:.8}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    out.extend_from_slice(if s == "-0" { b"0" } else { s.as_bytes() });
}

/// pdftoepdf.cc `copyName`.
fn print_name(out: &mut Vec<u8>, name: &[u8]) {
    out.push(b'/');
    for &c in name {
        if c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'-' | b'+') {
            out.push(c);
        } else {
            out.extend_from_slice(format!("#{c:02X}").as_bytes());
        }
    }
}

/// `pdf_newline`: an end of line unless the output just had one.
fn newline(out: &mut Vec<u8>) {
    if out.last() != Some(&b'\n') {
        out.push(b'\n');
    }
}

/// The objects an included page refers to, copied once each: pdftoepdf.cc's `inObjList`,
/// numbered in the order first met and written first in, first out.
pub struct Copier<'f> {
    f: &'f PdfFile,
    numbers: HashMap<u32, i32>,
    queue: Vec<u32>,
}

impl<'f> Copier<'f> {
    pub fn new(f: &'f PdfFile) -> Self {
        Copier { f, numbers: HashMap::new(), queue: Vec::new() }
    }

    /// `addOther`: the output number of source object `n`.
    fn number(&mut self, n: u32, next: &mut i32) -> i32 {
        *self.numbers.entry(n).or_insert_with(|| {
            self.queue.push(n);
            *next += 1;
            *next - 1
        })
    }

    /// pdftoepdf.cc `copyObject`.
    fn copy(&mut self, out: &mut Vec<u8>, o: &Obj, next: &mut i32) {
        match o {
            Obj::Null => out.extend_from_slice(b"null"),
            Obj::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
            Obj::Int(i) => out.extend_from_slice(i.to_string().as_bytes()),
            Obj::Real(r) => print_real(out, *r),
            Obj::Str(s) if !s.contains(&0) => {
                out.push(b'(');
                for &c in s {
                    match c {
                        b'(' | b')' | b'\\' => out.extend_from_slice(&[b'\\', c]),
                        c if !(0x20..=0x7F).contains(&c) => out.extend_from_slice(format!("\\{c:03o}").as_bytes()),
                        c => out.push(c),
                    }
                }
                out.push(b')');
            }
            Obj::Str(s) => {
                out.push(b'<');
                for &c in s {
                    out.extend_from_slice(format!("{c:02x}").as_bytes());
                }
                out.push(b'>');
            }
            Obj::Name(n) => print_name(out, n),
            Obj::Array(a) => {
                out.push(b'[');
                for v in a {
                    if !matches!(v, Obj::Name(_)) {
                        out.push(b' ');
                    }
                    self.copy(out, v, next);
                }
                out.push(b']');
            }
            Obj::Dict(d) => {
                out.extend_from_slice(b"<<\n");
                self.copy_entries(out, d, next);
                out.extend_from_slice(b">>");
            }
            Obj::Stream(d, range) => {
                out.extend_from_slice(b"<<\n");
                self.copy_entries(out, d, next);
                out.extend_from_slice(b">>\nstream\n");
                out.extend_from_slice(self.f.raw(range));
                out.extend_from_slice(b"\nendstream");
            }
            Obj::Ref(n, _) => {
                let k = self.number(*n, next);
                out.extend_from_slice(format!("{k} 0 R").as_bytes());
            }
        }
    }

    /// `copyDict`: `/Key value` lines.
    fn copy_entries(&mut self, out: &mut Vec<u8>, d: &Dict, next: &mut i32) {
        for (k, v) in d {
            print_name(out, k);
            out.push(b' ');
            self.copy(out, v, next);
            out.push(b'\n');
        }
    }

    /// `writeRefs`: every object met so far and those they lead to.
    fn write_refs(&mut self, objs: &mut Vec<(i32, Vec<u8>)>, next: &mut i32) {
        let mut i = 0;
        while i < self.queue.len() {
            let n = self.queue[i];
            i += 1;
            let o = self.f.get(n).unwrap_or(Obj::Null);
            let mut out = Vec::new();
            self.copy(&mut out, &o, next);
            out.push(b'\n');
            objs.push((self.numbers[&n], out));
        }
    }
}

/// What `\pdfximage` of a PDF page needs to write it.
pub struct FormSpec<'a> {
    pub page: i64,
    /// 0 media, 1 crop, 2 bleed, 3 trim, 4 art.
    pub page_box: usize,
    pub attr: &'a [u8],
    pub file_name: &'a str,
}

/// pdftoepdf.cc `write_epdf`: page `spec.page` of `f` as form XObject `obj` and the objects
/// it refers to (numbered from `*next`), appended to `objs`. The page's single content stream
/// is copied undecoded; several are decoded, joined and deflated again.
pub fn write_form(f: &PdfFile, spec: &FormSpec, obj: i32, objs: &mut Vec<(i32, Vec<u8>)>, next: &mut i32) -> Option<()> {
    let (page, _) = f.page(spec.page)?;
    let mut c = Copier::new(f);
    let mut out = b"<<\n".to_vec();
    if !spec.attr.is_empty() {
        out.extend_from_slice(spec.attr);
        out.push(b'\n');
    }
    out.extend_from_slice(b"/Type /XObject\n/Subtype /Form\n/FormType 1\n/PTEX.FileName ");
    c.copy(&mut out, &Obj::Str(spec.file_name.as_bytes().to_vec()), next);
    out.extend_from_slice(format!("\n/PTEX.PageNumber {}\n", spec.page).as_bytes());
    if let Some(Obj::Ref(n, _)) = get(&f.trailer, b"Info") {
        let k = c.number(*n, next);
        out.extend_from_slice(format!("/PTEX.InfoDict {k} 0 R\n").as_bytes());
    }
    let b = page.boxes[spec.page_box].map(|v| v as f64);
    let matrix = match page.rotate {
        90 => Some([0.0, -1.0, 1.0, 0.0, b[0] - b[1], b[1] + b[2]]),
        180 => Some([-1.0, 0.0, 0.0, -1.0, b[0] + b[2], b[1] + b[3]]),
        270 => Some([0.0, 1.0, -1.0, 0.0, b[0] + b[3], b[1] - b[0]]),
        _ => None,
    };
    let list = |out: &mut Vec<u8>, key: &[u8], v: &[f64]| {
        out.extend_from_slice(key);
        out.extend_from_slice(b" [");
        for (k, x) in v.iter().enumerate() {
            if k > 0 {
                out.push(b' ');
            }
            print_fixed8(out, *x);
        }
        out.extend_from_slice(b"]\n");
    };
    if let Some(m) = matrix {
        list(&mut out, b"/Matrix", &m);
    }
    list(&mut out, b"/BBox", &b);
    for key in [&b"LastModified"[..], b"Metadata", b"PieceInfo", b"SeparationInfo", b"Group"] {
        if let Some(v) = get(&page.dict, key) {
            newline(&mut out);
            print_name(&mut out, key);
            out.push(b' ');
            c.copy(&mut out, v, next);
        }
    }
    if let Some(res) = &page.resources {
        newline(&mut out);
        out.extend_from_slice(b"/Resources <<\n");
        for (k, v) in res {
            let v = f.resolve(v);
            match (k.as_slice(), &v) {
                (b"Font", Obj::Dict(fonts)) => {
                    out.extend_from_slice(b"/Font << ");
                    for (tag, font) in fonts {
                        print_name(&mut out, tag);
                        out.push(b' ');
                        c.copy(&mut out, font, next);
                    }
                    out.extend_from_slice(b">>\n");
                }
                (b"ProcSet", Obj::Array(a)) => {
                    out.extend_from_slice(b"/ProcSet [ ");
                    for p in a {
                        c.copy(&mut out, p, next);
                        out.push(b' ');
                    }
                    out.extend_from_slice(b"]\n");
                }
                (b"Subtype", Obj::Name(_)) | (_, Obj::Dict(_)) => {
                    print_name(&mut out, k);
                    out.push(b' ');
                    c.copy(&mut out, &v, next);
                }
                _ => {}
            }
        }
        out.extend_from_slice(b">>\n");
    }
    match get(&page.dict, b"Contents").map(|o| f.resolve(o)) {
        Some(Obj::Stream(d, range)) => {
            for key in [&b"Length"[..], b"Filter", b"DecodeParms"] {
                if let Some(v) = get(&d, key).map(|v| f.resolve(v)) {
                    print_name(&mut out, key);
                    out.push(b' ');
                    c.copy(&mut out, &v, next);
                    out.push(b'\n');
                } else if key == b"Filter" {
                    break;
                }
            }
            out.extend_from_slice(b">>\nstream\n");
            out.extend_from_slice(f.raw(&range));
            out.extend_from_slice(b"\nendstream\n");
        }
        other => {
            let mut data = Vec::new();
            if let Some(Obj::Array(parts)) = other {
                for (k, p) in parts.iter().enumerate() {
                    if let Obj::Stream(d, range) = f.resolve(p) {
                        data.extend_from_slice(&decode(&d, f.raw(&range))?);
                    }
                    if k + 1 < parts.len() {
                        newline(&mut data);
                    }
                }
            }
            let mut packed = Vec::new();
            super::backend_deflate::DeflateCompressor::new().compress_zlib(&data, &mut packed);
            out.extend_from_slice(format!("/Length {}\n/Filter /FlateDecode\n>>\nstream\n", packed.len()).as_bytes());
            out.extend_from_slice(&packed);
            out.extend_from_slice(b"\nendstream\n");
        }
    }
    objs.push((obj, out));
    c.write_refs(objs, next);
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objects_parse() {
        let mut lx = Lexer { s: b"<< /A 1 0 R /B [1 2.5 (a\\)b) <414> /N#20x] /C true >>", i: 0 };
        let Some(Obj::Dict(d)) = lx.obj() else { panic!() };
        assert_eq!(get(&d, b"A"), Some(&Obj::Ref(1, 0)));
        assert_eq!(get(&d, b"B"), Some(&Obj::Array(vec![Obj::Int(1), Obj::Real(2.5), Obj::Str(b"a)b".to_vec()), Obj::Str(b"A@".to_vec()), Obj::Name(b"N x".to_vec())])));
        assert_eq!(get(&d, b"C"), Some(&Obj::Bool(true)));
    }

    #[test]
    fn classic_xref_reads_a_page() {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offs = Vec::new();
        for body in ["<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 100 50] >>", "<< /Type /Page /Parent 2 0 R /Rotate 90 >>"] {
            offs.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", offs.len()).as_bytes());
        }
        let x = pdf.len();
        pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
        for o in &offs {
            pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n").as_bytes());
        let f = PdfFile::open(pdf).unwrap();
        assert_eq!(f.page_count(), 1);
        let (p, n) = f.page(1).unwrap();
        assert_eq!(n, 3);
        assert_eq!(p.boxes[1], [0.0, 0.0, 100.0, 50.0]);
        assert_eq!(p.rotate, 90);
    }
}
