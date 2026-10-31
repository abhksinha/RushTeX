//! `parity-testing`, rushtex's testing framework: glyph-level and zero-tolerance parity of
//! rushtex's PDFs against pdfLaTeX's, for the documents in `scripts/parity/corpus`. An independent checker: it shares no code with
//! rushtex (its own PDF reader, inflate, SHA-256 and exact arithmetic), and renders and
//! extracts text with MuPDF's `mutool` (the same MuPDF, 1.28.5, as the Python tools it replaced,
//! `parity.py` and `pdfexact.py`).
//!
//!   parity-testing score [DOC..|--all] [--build] [--dx PAGE:DX] [--save-baseline] [--tol T]
//!   parity-testing zero [DOC..|--all] [--pixels] [--dpi N] [--cached] [--why PAGE] [--build]
//!   parity-testing exact [DOC..|--all] [-v] [--build]
//!   parity-testing rank DOC [-n N]
//!   parity-testing lines DOC PAGE [--all] [--cached]
//!   parity-testing miss DOC PAGE [--dx DX]
//!   parity-testing boxes DOC PAGE [--all]
//!   parity-testing repro DOC A B | --body TEX [--name N] [--trace pages|lists] [--all] [--build]
//!   parity-testing tj DOC PAGE [--width W]
//!   parity-testing glyphs PDF                 (the extracted glyphs, to check against pymupdf)
//!   parity-testing incr DOC.. [--edits N] [--seed S] [-v]   (the watch session against fresh compiles, after random edits)
//!   parity-testing bench [FILE.tex [N] | docs | templates | compare OLD_BIN | all | check [BASE]]   (`cargo benchmark`)
//!   common: [-j N] parallel jobs (default: every core)
//!
//! `zero` interprets each page's content stream with exact decimal arithmetic (every operand
//! is a decimal literal and every division is by 100 or 1000, so `Dec` is exact): every glyph
//! (font, code, full rendering matrix), path and image placement with its colour, compared as
//! multisets. Pixels: both PDFs rendered by MuPDF at the same resolution; per page the MD5 of
//! the image first (the reference's are cached), and only pages whose hashes differ are
//! rendered again and compared pixel by pixel with SIMD. Structure: links, outlines, named
//! destinations, metadata.
//!
//! Text (`score`, `exact`, `lines`, `boxes`, ...) is pymupdf's `rawdict` to the bit: MuPDF's
//! trace gives every glyph, positions are computed in `f32` as MuPDF does, and its extraction
//! rules (ligatures, combining marks, fake bold, the media box) are applied. `lines` and `tj`
//! pair lines and operators with a port of difflib's `SequenceMatcher` (`autojunk=False`).
//!
//! A glyph matches (`score`) when one of ours lies within 0.1pt of its origin. References are
//! cached in `target/parity/ref` and rebuilt when the source (or the TeX installation) is newer.
//!
//! Where the time goes: rendering 300 dpi pages in MuPDF dominates everything else together,
//! so the design is parallel documents and pages, hashes before pixels, and cached reference
//! hashes. The SIMD pixel count is the small, last step.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

// ---- SHA-256 (FIPS 180-4), for the pinned references' `sources.tsv` -----------------------

fn sha256(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for block in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, c) in block.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*c);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    h.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:08x}");
        s
    })
}

// ---- inflate (RFC 1950/1951) ----------------------------------------------------------------

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u64,
    n: u32,
}

impl Bits<'_> {
    #[inline]
    fn need(&mut self, k: u32) -> Option<()> {
        while self.n < k {
            let b = *self.data.get(self.pos)?;
            self.pos += 1;
            self.bit |= (b as u64) << self.n;
            self.n += 8;
        }
        Some(())
    }
    #[inline]
    fn get(&mut self, k: u32) -> Option<u32> {
        if k == 0 {
            return Some(0);
        }
        self.need(k)?;
        let v = (self.bit & ((1u64 << k) - 1)) as u32;
        self.bit >>= k;
        self.n -= k;
        Some(v)
    }
}

/// A canonical Huffman code: counts per length and symbols in code order.
struct Huff {
    count: [u16; 16],
    sym: Vec<u16>,
}

impl Huff {
    fn new(lengths: &[u8]) -> Huff {
        let mut count = [0u16; 16];
        for &l in lengths {
            count[l as usize] += 1;
        }
        count[0] = 0;
        let mut offs = [0u16; 16];
        for i in 1..16 {
            offs[i] = offs[i - 1] + count[i - 1];
        }
        let mut sym = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                sym[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        Huff { count, sym }
    }

    #[inline]
    fn decode(&self, b: &mut Bits) -> Option<u16> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= b.get(1)? as i32;
            let count = self.count[len] as i32;
            if code - count < first {
                return self.sym.get((index + (code - first)) as usize).copied();
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        None
    }
}

const LBASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEXT: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DBASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DEXT: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// A zlib stream's data (the header is skipped; a raw deflate stream is tried as well). What
/// was decoded is returned even when the stream is truncated, as MuPDF does.
fn inflate(src: &[u8]) -> Vec<u8> {
    let raw = !(src.len() >= 2 && (src[0] & 0x0f) == 8 && (u16::from(src[0]) << 8 | u16::from(src[1])) % 31 == 0);
    let mut b = Bits { data: if raw { src } else { &src[2..] }, pos: 0, bit: 0, n: 0 };
    let mut out = Vec::with_capacity(src.len() * 4);
    let _ = inflate_blocks(&mut b, &mut out);
    out
}

fn inflate_blocks(b: &mut Bits, out: &mut Vec<u8>) -> Option<()> {
    loop {
        let last = b.get(1)?;
        match b.get(2)? {
            0 => {
                b.bit = 0;
                b.n = 0;
                let d = b.data.get(b.pos..b.pos + 4)?;
                let len = u16::from_le_bytes([d[0], d[1]]) as usize;
                b.pos += 4;
                out.extend_from_slice(b.data.get(b.pos..b.pos + len)?);
                b.pos += len;
            }
            1 => {
                let mut l = [0u8; 288];
                l[..144].fill(8);
                l[144..256].fill(9);
                l[256..280].fill(7);
                l[280..].fill(8);
                inflate_codes(b, out, &Huff::new(&l), &Huff::new(&[5u8; 30]))?;
            }
            2 => {
                let (hlit, hdist, hclen) = (b.get(5)? as usize + 257, b.get(5)? as usize + 1, b.get(4)? as usize + 4);
                const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                let mut cl = [0u8; 19];
                for &o in &ORDER[..hclen] {
                    cl[o] = b.get(3)? as u8;
                }
                let ch = Huff::new(&cl);
                let mut lens = vec![0u8; hlit + hdist];
                let mut i = 0;
                while i < hlit + hdist {
                    let s = ch.decode(b)?;
                    let (v, rep) = match s {
                        0..=15 => (s as u8, 1),
                        16 => (*lens.get(i.checked_sub(1)?)?, 3 + b.get(2)? as usize),
                        17 => (0, 3 + b.get(3)? as usize),
                        _ => (0, 11 + b.get(7)? as usize),
                    };
                    for _ in 0..rep {
                        *lens.get_mut(i)? = v;
                        i += 1;
                    }
                }
                inflate_codes(b, out, &Huff::new(&lens[..hlit]), &Huff::new(&lens[hlit..]))?;
            }
            _ => return None,
        }
        if last == 1 {
            return Some(());
        }
    }
}

fn inflate_codes(b: &mut Bits, out: &mut Vec<u8>, lit: &Huff, dist: &Huff) -> Option<()> {
    loop {
        let s = lit.decode(b)? as usize;
        if s < 256 {
            out.push(s as u8);
        } else if s == 256 {
            return Some(());
        } else {
            let s = s - 257;
            let len = *LBASE.get(s)? as usize + b.get(*LEXT.get(s)? as u32)? as usize;
            let d = dist.decode(b)? as usize;
            let back = *DBASE.get(d)? as usize + b.get(*DEXT.get(d)? as u32)? as usize;
            let start = out.len().checked_sub(back)?;
            for k in 0..len {
                let c = out[start + k];
                out.push(c);
            }
        }
    }
}

// ---- exact decimals -------------------------------------------------------------------------

/// `m / 10^e`, canonical (no trailing zero digit in `m` unless `e` is 0, and 0 has `e` 0), so
/// equal values are equal structurally and hash alike. Exact for every operand a content
/// stream holds and every operation the interpreter makes (products, sums, division by 100
/// and 1000); an operation that would overflow stops the document (it is never rounded).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
struct Dec {
    m: i128,
    e: u8,
}

const ZERO: Dec = Dec { m: 0, e: 0 };
const ONE: Dec = Dec { m: 1, e: 0 };

fn pow10(e: u8) -> i128 {
    10i128.checked_pow(e as u32).unwrap_or_else(|| overflow())
}

fn overflow() -> ! {
    panic!("exact arithmetic overflow")
}

impl Dec {
    fn norm(mut m: i128, mut e: u8) -> Dec {
        if m == 0 {
            return ZERO;
        }
        while e > 0 && m % 10 == 0 {
            m /= 10;
            e -= 1;
        }
        Dec { m, e }
    }
    fn int(v: i64) -> Dec {
        Dec { m: v as i128, e: 0 }
    }
    /// A PDF number as written (`[+-]?(\d+\.?\d*|\.\d+)`).
    fn parse(s: &[u8]) -> Option<Dec> {
        let (neg, s) = match s.first() {
            Some(b'-') => (true, &s[1..]),
            Some(b'+') => (false, &s[1..]),
            _ => (false, s),
        };
        let (mut m, mut e, mut dot, mut digits) = (0i128, 0u8, false, 0);
        for &c in s {
            match c {
                b'0'..=b'9' => {
                    m = m.checked_mul(10)?.checked_add((c - b'0') as i128)?;
                    digits += 1;
                    if dot {
                        e += 1;
                    }
                }
                b'.' if !dot => dot = true,
                _ => return None,
            }
        }
        (digits > 0).then(|| Dec::norm(if neg { -m } else { m }, e))
    }
    fn mul(self, o: Dec) -> Dec {
        Dec::norm(self.m.checked_mul(o.m).unwrap_or_else(|| overflow()), self.e.checked_add(o.e).unwrap_or_else(|| overflow()))
    }
    fn add(self, o: Dec) -> Dec {
        let e = self.e.max(o.e);
        let a = self.m.checked_mul(pow10(e - self.e)).unwrap_or_else(|| overflow());
        let b = o.m.checked_mul(pow10(e - o.e)).unwrap_or_else(|| overflow());
        Dec::norm(a.checked_add(b).unwrap_or_else(|| overflow()), e)
    }
    fn neg(self) -> Dec {
        Dec { m: -self.m, e: self.e }
    }
    fn sub(self, o: Dec) -> Dec {
        self.add(o.neg())
    }
    /// Division by `10^k` (`Tz`'s /100, glyph space's /1000).
    fn div10(self, k: u8) -> Dec {
        Dec::norm(self.m, self.e.checked_add(k).unwrap_or_else(|| overflow()))
    }
    fn f64(self) -> f64 {
        self.m as f64 / 10f64.powi(self.e as i32)
    }
    /// Rounded to `k` places, half to even (as Python's `round` of the exact value).
    fn round(self, k: u8) -> Dec {
        if self.e <= k {
            return self;
        }
        let d = pow10(self.e - k);
        let (q, r) = (self.m.div_euclid(d), self.m.rem_euclid(d));
        let q = match (2 * r).cmp(&d) {
            std::cmp::Ordering::Greater => q + 1,
            std::cmp::Ordering::Equal if q % 2 != 0 => q + 1,
            _ => q,
        };
        Dec::norm(q, k)
    }
}

impl std::fmt::Debug for Dec {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = self.m.unsigned_abs().to_string();
        let sign = if self.m < 0 { "-" } else { "" };
        let e = self.e as usize;
        if e == 0 {
            write!(f, "{sign}{s}")
        } else if s.len() > e {
            write!(f, "{sign}{}.{}", &s[..s.len() - e], &s[s.len() - e..])
        } else {
            write!(f, "{sign}0.{}{s}", "0".repeat(e - s.len()))
        }
    }
}

/// PDF's row-vector matrix `[a b c d e f]`.
type Mat = [Dec; 6];
const IDENTITY: Mat = [ONE, ZERO, ZERO, ONE, ZERO, ZERO];

/// `m × n`.
fn mul(m: &Mat, n: &Mat) -> Mat {
    let [a, b, c, d, e, f] = *m;
    let [aa, bb, cc, dd, ee, ff] = *n;
    [
        a.mul(aa).add(b.mul(cc)),
        a.mul(bb).add(b.mul(dd)),
        c.mul(aa).add(d.mul(cc)),
        c.mul(bb).add(d.mul(dd)),
        e.mul(aa).add(f.mul(cc)).add(ee),
        e.mul(bb).add(f.mul(dd)).add(ff),
    ]
}

fn apply(m: &Mat, x: Dec, y: Dec) -> (Dec, Dec) {
    (x.mul(m[0]).add(y.mul(m[2])).add(m[4]), x.mul(m[1]).add(y.mul(m[3])).add(m[5]))
}

fn translate(x: Dec, y: Dec) -> Mat {
    [ONE, ZERO, ZERO, ONE, x, y]
}

// ---- PDF objects ----------------------------------------------------------------------------

type Dict = Vec<(Vec<u8>, Obj)>;

#[derive(Clone, Debug, PartialEq)]
enum Obj {
    Null,
    Bool(bool),
    Num(Dec),
    Str(Vec<u8>),
    Name(Vec<u8>),
    Arr(Vec<Obj>),
    Dict(Dict),
    Ref(u32, u16),
    /// A stream: its dictionary and the offset of its first data byte in the file (or, for a
    /// stream inside an object stream, never: those cannot hold streams).
    Stream(Dict, usize),
}

impl Obj {
    fn get(&self, key: &str) -> Option<&Obj> {
        match self {
            Obj::Dict(d) | Obj::Stream(d, _) => d.iter().find(|(k, _)| k == key.as_bytes()).map(|(_, v)| v),
            _ => None,
        }
    }
    fn num(&self) -> Option<Dec> {
        match self {
            Obj::Num(n) => Some(*n),
            _ => None,
        }
    }
    fn int(&self) -> Option<i64> {
        self.num().filter(|n| n.e == 0).map(|n| n.m as i64)
    }
    fn name(&self) -> Option<&[u8]> {
        match self {
            Obj::Name(n) => Some(n),
            _ => None,
        }
    }
    fn arr(&self) -> &[Obj] {
        match self {
            Obj::Arr(a) => a,
            _ => &[],
        }
    }
    fn dict(&self) -> &[(Vec<u8>, Obj)] {
        match self {
            Obj::Dict(d) | Obj::Stream(d, _) => d,
            _ => &[],
        }
    }
    /// The object as `xref_get_key` prints a value (for image sizes kept as text).
    fn text(&self) -> String {
        match self {
            Obj::Null => "null".into(),
            Obj::Bool(b) => b.to_string(),
            Obj::Num(n) => format!("{n:?}"),
            Obj::Name(n) => format!("/{}", latin1(n)),
            Obj::Ref(n, g) => format!("{n} {g} R"),
            Obj::Str(s) => latin1(s),
            Obj::Arr(a) => format!("[{}]", a.iter().map(Obj::text).collect::<Vec<_>>().join(" ")),
            Obj::Dict(_) | Obj::Stream(..) => "<<>>".into(),
        }
    }
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

const WS: &[u8] = b" \t\r\n\x0c\x00";
const DELIM: &[u8] = b"()<>[]{}/%";

#[inline]
fn is_ws(c: u8) -> bool {
    WS.contains(&c)
}
#[inline]
fn is_regular(c: u8) -> bool {
    !is_ws(c) && !DELIM.contains(&c)
}

/// The PDF file syntax (§7.2–§7.3) over bytes.
struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Lexer<'a> {
    fn skip(&mut self) {
        while self.i < self.s.len() {
            let c = self.s[self.i];
            if is_ws(c) {
                self.i += 1;
            } else if c == b'%' {
                while self.i < self.s.len() && !b"\r\n".contains(&self.s[self.i]) {
                    self.i += 1;
                }
            } else {
                break;
            }
        }
    }
    fn word(&mut self) -> &'a [u8] {
        let st = self.i;
        while self.i < self.s.len() && is_regular(self.s[self.i]) {
            self.i += 1;
        }
        &self.s[st..self.i]
    }
    /// One object; `N G R` is read as a reference.
    fn obj(&mut self, depth: u32) -> Option<Obj> {
        self.skip();
        let c = *self.s.get(self.i)?;
        if depth > 64 {
            return None;
        }
        match c {
            b'/' => {
                self.i += 1;
                Some(Obj::Name(name_decode(self.word())))
            }
            b'(' => Some(Obj::Str(literal_string(self.s, &mut self.i))),
            b'<' if self.s.get(self.i + 1) == Some(&b'<') => {
                self.i += 2;
                let mut d = Vec::new();
                loop {
                    self.skip();
                    if self.s.get(self.i..self.i + 2) == Some(b">>") {
                        self.i += 2;
                        break;
                    }
                    let k = match self.obj(depth + 1)? {
                        Obj::Name(n) => n,
                        _ => continue,
                    };
                    let v = self.obj(depth + 1)?;
                    d.push((k, v));
                }
                Some(Obj::Dict(d))
            }
            b'<' => Some(Obj::Str(hex_string(self.s, &mut self.i))),
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                loop {
                    self.skip();
                    match self.s.get(self.i) {
                        Some(b']') => {
                            self.i += 1;
                            break;
                        }
                        None => break,
                        _ => a.push(self.obj(depth + 1)?),
                    }
                }
                Some(Obj::Arr(a))
            }
            _ => {
                let w = self.word();
                if w.is_empty() {
                    self.i += 1;
                    return Some(Obj::Null);
                }
                match w {
                    b"true" => Some(Obj::Bool(true)),
                    b"false" => Some(Obj::Bool(false)),
                    b"null" => Some(Obj::Null),
                    _ => {
                        let n = Dec::parse(w).unwrap_or(ZERO);
                        // `N G R`.
                        if n.e == 0 && n.m >= 0 {
                            let save = self.i;
                            self.skip();
                            let g = self.word();
                            if !g.is_empty() && g.iter().all(u8::is_ascii_digit) {
                                self.skip();
                                if self.s.get(self.i) == Some(&b'R') && self.s.get(self.i + 1).is_none_or(|&c| !is_regular(c)) {
                                    self.i += 1;
                                    return Some(Obj::Ref(n.m as u32, std::str::from_utf8(g).ok()?.parse().ok()?));
                                }
                            }
                            self.i = save;
                        }
                        Some(Obj::Num(n))
                    }
                }
            }
        }
    }
}

/// `#xx` escapes of a name.
fn name_decode(w: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(w.len());
    let mut i = 0;
    while i < w.len() {
        if w[i] == b'#' && i + 2 < w.len() {
            if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&w[i + 1..i + 3]).unwrap_or("x"), 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(w[i]);
        i += 1;
    }
    out
}

/// A literal string from `(` at `*i`, escapes resolved (§7.3.4.2).
fn literal_string(s: &[u8], i: &mut usize) -> Vec<u8> {
    let n = s.len();
    let (mut depth, mut out) = (1, Vec::new());
    *i += 1;
    while *i < n {
        let ch = s[*i];
        if ch == b'\\' {
            *i += 1;
            let Some(&e) = s.get(*i) else { break };
            if (b'0'..=b'7').contains(&e) {
                let mut j = *i;
                let mut v = 0u32;
                while j < n && j < *i + 3 && (b'0'..=b'7').contains(&s[j]) {
                    v = v * 8 + (s[j] - b'0') as u32;
                    j += 1;
                }
                out.push((v & 0xff) as u8);
                *i = j;
                continue;
            }
            *i += 1;
            if e == b'\r' || e == b'\n' {
                if e == b'\r' && s.get(*i) == Some(&b'\n') {
                    *i += 1;
                }
                continue;
            }
            out.push(match e {
                b'n' => 10,
                b'r' => 13,
                b't' => 9,
                b'b' => 8,
                b'f' => 12,
                _ => e,
            });
            continue;
        }
        if ch == b'(' {
            depth += 1;
        } else if ch == b')' {
            depth -= 1;
            if depth == 0 {
                *i += 1;
                break;
            }
        }
        out.push(ch);
        *i += 1;
    }
    out
}

/// A hex string from `<` at `*i` (an odd digit count gets a final 0).
fn hex_string(s: &[u8], i: &mut usize) -> Vec<u8> {
    let end = s[*i..].iter().position(|&c| c == b'>').map_or(s.len(), |p| *i + p);
    let digits: Vec<u8> = s[*i + 1..end].iter().copied().filter(|c| !is_ws(*c)).collect();
    *i = (end + 1).min(s.len());
    let hex = |c: u8| (c as char).to_digit(16).unwrap_or(0) as u8;
    digits.chunks(2).map(|p| hex(p[0]) << 4 | p.get(1).map_or(0, |&c| hex(c))).collect()
}

/// Where an object is: at a file offset, or the `k`th of object stream `n`.
#[derive(Clone, Copy, Debug)]
enum Entry {
    At(usize),
    InStream(u32, u32),
}

/// A PDF file, read lazily: objects are parsed when first asked for and kept.
struct Pdf {
    data: Vec<u8>,
    xref: HashMap<u32, Entry>,
    trailer: Dict,
    cache: std::cell::RefCell<HashMap<u32, std::rc::Rc<Obj>>>,
    pages: Vec<u32>,
}

impl Pdf {
    fn open(path: &Path) -> Result<Pdf, String> {
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut pdf = Pdf { data, xref: HashMap::new(), trailer: Vec::new(), cache: Default::default(), pages: Vec::new() };
        if pdf.read_xref().is_none() || pdf.trailer_get("Root").is_none() {
            pdf.xref.clear();
            pdf.reconstruct();
        }
        let root = pdf.resolve(&pdf.trailer_get("Root").ok_or_else(|| format!("{}: no document catalog", path.display()))?);
        if let Some(Obj::Ref(n, _)) = root.get("Pages") {
            let mut seen = Vec::new();
            pdf.collect_pages(*n, &mut seen, 0);
        }
        Ok(pdf)
    }

    fn trailer_get(&self, k: &str) -> Option<Obj> {
        self.trailer.iter().find(|(kk, _)| kk == k.as_bytes()).map(|(_, v)| v.clone())
    }

    fn collect_pages(&mut self, n: u32, seen: &mut Vec<u32>, depth: u32) {
        if seen.contains(&n) || depth > 64 {
            return;
        }
        seen.push(n);
        let node = self.get(n);
        let kids: Vec<Obj> = node.get("Kids").map(|k| self.resolve(k).arr().to_vec()).unwrap_or_default();
        let is_page = node.get("Type").and_then(Obj::name) == Some(b"Page") || (node.get("Kids").is_none() && node.get("Type").is_none());
        if is_page {
            self.pages.push(n);
            return;
        }
        for k in kids {
            if let Obj::Ref(m, _) = k {
                self.collect_pages(m, seen, depth + 1);
            }
        }
    }

    fn read_xref(&mut self) -> Option<()> {
        let tail = &self.data[self.data.len().saturating_sub(1100)..];
        let p = tail.windows(9).rposition(|w| w == b"startxref")?;
        let mut lx = Lexer { s: tail, i: p + 9 };
        let mut off = lx.obj(0)?.int()? as usize;
        let mut first = true;
        let mut visited = Vec::new();
        while !visited.contains(&off) {
            visited.push(off);
            let dict = self.xref_section(off)?;
            if let Some(Obj::Num(s)) = dict.iter().find(|(k, _)| k == b"XRefStm").map(|(_, v)| v) {
                let _ = self.xref_section(s.m as usize);
            }
            let prev = dict.iter().find(|(k, _)| k == b"Prev").and_then(|(_, v)| v.int());
            if first {
                self.trailer = dict;
                first = false;
            }
            match prev {
                Some(p) => off = p as usize,
                None => break,
            }
        }
        Some(())
    }

    /// One xref section (a table or a stream) at `off`: its entries go in unless a newer
    /// section had them; the trailer (or stream) dictionary is returned.
    fn xref_section(&mut self, off: usize) -> Option<Dict> {
        let mut lx = Lexer { s: &self.data, i: off };
        lx.skip();
        if self.data.get(lx.i..lx.i + 4) == Some(b"xref") {
            lx.i += 4;
            loop {
                lx.skip();
                if self.data.get(lx.i..lx.i + 7) == Some(b"trailer") {
                    lx.i += 7;
                    return match lx.obj(0)? {
                        Obj::Dict(d) => Some(d),
                        _ => None,
                    };
                }
                let start = lx.obj(0)?.int()? as u32;
                let count = lx.obj(0)?.int()? as u32;
                for k in 0..count {
                    lx.skip();
                    let row = self.data.get(lx.i..lx.i + 18)?;
                    let o: usize = std::str::from_utf8(&row[..10]).ok()?.trim().parse().ok()?;
                    let kind = row[17];
                    lx.i += 18;
                    if kind == b'n' && o > 0 {
                        self.xref.entry(start + k).or_insert(Entry::At(o));
                    } else {
                        // A free entry hides older ones too.
                        self.xref.entry(start + k).or_insert(Entry::At(0));
                    }
                }
            }
        }
        // An xref stream.
        let (dict, data) = self.parse_at(off).and_then(|(o, _)| match o {
            Obj::Stream(d, at) => {
                let s = Obj::Stream(d.clone(), at);
                Some((d, self.stream_bytes(&s)))
            }
            _ => None,
        })?;
        let o = Obj::Dict(dict.clone());
        let w: Vec<usize> = o.get("W")?.arr().iter().filter_map(|x| x.int()).map(|x| x as usize).collect();
        if w.len() != 3 {
            return None;
        }
        let size = o.get("Size").and_then(Obj::int).unwrap_or(0) as u32;
        let index: Vec<u32> = match o.get("Index") {
            Some(Obj::Arr(a)) => a.iter().filter_map(Obj::int).map(|x| x as u32).collect(),
            _ => vec![0, size],
        };
        let row = w[0] + w[1] + w[2];
        let field = |b: &[u8]| b.iter().fold(0usize, |a, &c| a << 8 | c as usize);
        let mut at = 0;
        for pair in index.chunks(2) {
            let [start, count] = [pair[0], *pair.get(1).unwrap_or(&0)];
            for k in 0..count {
                let Some(r) = data.get(at..at + row) else { break };
                at += row;
                let t = if w[0] == 0 { 1 } else { field(&r[..w[0]]) };
                let (a, b) = (field(&r[w[0]..w[0] + w[1]]), field(&r[w[0] + w[1]..]));
                let e = match t {
                    1 => Entry::At(a),
                    2 => Entry::InStream(a as u32, b as u32),
                    _ => Entry::At(0),
                };
                self.xref.entry(start + k).or_insert(e);
            }
        }
        Some(dict)
    }

    /// Without a usable xref: every `N G obj` in the file, the last one of each number kept;
    /// the trailer from the last `trailer` dictionary, else the catalog found.
    fn reconstruct(&mut self) {
        let d = &self.data;
        let mut i = 0;
        while let Some(p) = d[i..].windows(3).position(|w| w == b"obj").map(|p| p + i) {
            i = p + 3;
            let mut j = p;
            let mut nums = Vec::new();
            for _ in 0..2 {
                while j > 0 && is_ws(d[j - 1]) {
                    j -= 1;
                }
                let e = j;
                while j > 0 && d[j - 1].is_ascii_digit() {
                    j -= 1;
                }
                if j == e {
                    break;
                }
                nums.push(std::str::from_utf8(&d[j..e]).ok().and_then(|s| s.parse::<u32>().ok()));
            }
            if let [Some(_g), Some(n)] = nums[..] {
                self.xref.insert(n, Entry::At(j));
            }
        }
        if let Some(p) = d.windows(7).rposition(|w| w == b"trailer") {
            if let Some(Obj::Dict(t)) = (Lexer { s: d, i: p + 7 }).obj(0) {
                self.trailer = t;
            }
        }
        if self.trailer.iter().all(|(k, _)| k != b"Root") {
            let ids: Vec<u32> = self.xref.keys().copied().collect();
            for n in ids {
                if self.get(n).get("Type").and_then(Obj::name) == Some(b"Catalog") {
                    self.trailer.push((b"Root".to_vec(), Obj::Ref(n, 0)));
                    break;
                }
            }
        }
    }

    /// `N G obj …` at `off`: the object and its number.
    fn parse_at(&self, off: usize) -> Option<(Obj, u32)> {
        let mut lx = Lexer { s: &self.data, i: off };
        let n = lx.obj(0)?;
        let num = match n {
            Obj::Num(x) => x.m as u32,
            Obj::Ref(..) => return None,
            _ => return None,
        };
        lx.skip();
        lx.word();
        lx.skip();
        if self.data.get(lx.i..lx.i + 3) != Some(b"obj") {
            return None;
        }
        lx.i += 3;
        let o = lx.obj(0)?;
        lx.skip();
        if let Obj::Dict(d) = &o {
            if self.data.get(lx.i..lx.i + 6) == Some(b"stream") {
                let mut at = lx.i + 6;
                if self.data.get(at) == Some(&b'\r') {
                    at += 1;
                }
                if self.data.get(at) == Some(&b'\n') {
                    at += 1;
                }
                return Some((Obj::Stream(d.clone(), at), num));
            }
        }
        Some((o, num))
    }

    fn get(&self, n: u32) -> std::rc::Rc<Obj> {
        if let Some(o) = self.cache.borrow().get(&n) {
            return o.clone();
        }
        let o = match self.xref.get(&n) {
            Some(Entry::At(off)) if *off > 0 => self.parse_at(*off).map(|x| x.0).unwrap_or(Obj::Null),
            Some(Entry::InStream(s, k)) => self.object_from_stream(*s, *k).unwrap_or(Obj::Null),
            _ => Obj::Null,
        };
        let o = std::rc::Rc::new(o);
        self.cache.borrow_mut().insert(n, o.clone());
        o
    }

    fn object_from_stream(&self, s: u32, k: u32) -> Option<Obj> {
        let st = self.get(s);
        let data = self.stream_bytes(&st);
        let n = st.get("N")?.int()? as usize;
        let first = st.get("First")?.int()? as usize;
        let mut lx = Lexer { s: &data, i: 0 };
        let mut offs = Vec::with_capacity(n);
        for _ in 0..n {
            let _num = lx.obj(0)?;
            offs.push(lx.obj(0)?.int()? as usize);
        }
        let mut lx = Lexer { s: &data, i: first + *offs.get(k as usize)? };
        lx.obj(0)
    }

    fn resolve(&self, o: &Obj) -> Obj {
        let mut o = o.clone();
        for _ in 0..32 {
            match o {
                Obj::Ref(n, _) => o = (*self.get(n)).clone(),
                _ => return o,
            }
        }
        Obj::Null
    }

    /// A key of a dictionary, followed through references.
    fn key(&self, o: &Obj, k: &str) -> Obj {
        o.get(k).map_or(Obj::Null, |v| self.resolve(v))
    }

    /// A stream's data, decoded (Flate with PNG or TIFF predictors; other filters left as
    /// they are, which no check reads).
    fn stream_bytes(&self, o: &Obj) -> Vec<u8> {
        let Obj::Stream(d, at) = o else { return Vec::new() };
        let dict = Obj::Dict(d.clone());
        let len = self.key(&dict, "Length").int().unwrap_or(-1);
        let raw = if len >= 0 && *at + len as usize <= self.data.len() {
            &self.data[*at..*at + len as usize]
        } else {
            // A wrong length: up to `endstream`.
            let end = self.data[*at..].windows(9).position(|w| w == b"endstream").map_or(self.data.len(), |p| *at + p);
            &self.data[*at..end]
        };
        let filters: Vec<Vec<u8>> = match self.key(&dict, "Filter") {
            Obj::Name(n) => vec![n],
            Obj::Arr(a) => a.iter().filter_map(|x| x.name().map(<[u8]>::to_vec)).collect(),
            _ => Vec::new(),
        };
        let parms: Vec<Obj> = match self.key(&dict, "DecodeParms") {
            Obj::Arr(a) => a.iter().map(|x| self.resolve(x)).collect(),
            p @ Obj::Dict(_) => vec![p],
            _ => Vec::new(),
        };
        let mut out = raw.to_vec();
        for (i, f) in filters.iter().enumerate() {
            match f.as_slice() {
                b"FlateDecode" | b"Fl" => {
                    out = inflate(&out);
                    if let Some(p) = parms.get(i) {
                        out = predict(&out, p);
                    }
                }
                _ => break,
            }
        }
        out
    }

    fn page_index(&self, n: u32) -> Option<usize> {
        self.pages.iter().position(|&p| p == n)
    }
}

/// PNG (10–15) and TIFF (2) predictors of a Flate stream (§7.4.4.4).
fn predict(data: &[u8], p: &Obj) -> Vec<u8> {
    let pred = p.get("Predictor").and_then(Obj::int).unwrap_or(1);
    if pred < 2 {
        return data.to_vec();
    }
    let colors = p.get("Colors").and_then(Obj::int).unwrap_or(1) as usize;
    let bpc = p.get("BitsPerComponent").and_then(Obj::int).unwrap_or(8) as usize;
    let cols = p.get("Columns").and_then(Obj::int).unwrap_or(1) as usize;
    let bpp = (colors * bpc).div_ceil(8).max(1);
    let row = (colors * bpc * cols).div_ceil(8);
    if pred == 2 {
        let mut out = data.to_vec();
        for r in out.chunks_mut(row) {
            for i in bpp..r.len() {
                r[i] = r[i].wrapping_add(r[i - bpp]);
            }
        }
        return out;
    }
    let mut out = Vec::with_capacity(data.len());
    let mut prev = vec![0u8; row];
    for chunk in data.chunks(row + 1) {
        let (t, src) = (chunk[0], &chunk[1..]);
        let mut cur = vec![0u8; row];
        for i in 0..src.len().min(row) {
            let a = if i >= bpp { cur[i - bpp] } else { 0 };
            let b = prev[i];
            let c = if i >= bpp { prev[i - bpp] } else { 0 };
            cur[i] = src[i].wrapping_add(match t {
                1 => a,
                2 => b,
                3 => ((a as u16 + b as u16) / 2) as u8,
                4 => {
                    let p = a as i16 + b as i16 - c as i16;
                    let (pa, pb, pc) = ((p - a as i16).abs(), (p - b as i16).abs(), (p - c as i16).abs());
                    if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }
                }
                _ => 0,
            });
        }
        out.extend_from_slice(&cur[..src.len().min(row)]);
        prev = cur;
    }
    out
}

// ---- content streams ------------------------------------------------------------------------

/// A content-stream operand, as `pdfexact.py` reads them.
#[derive(Clone, Debug)]
enum Opd {
    Num(Dec),
    Str(Vec<u8>),
    Name(Vec<u8>),
    Arr(Vec<Opd>),
    /// A dictionary operand (`BDC`, `DP`), whose entries no operator here reads.
    Dict,
    Word,
}

impl Opd {
    fn num(&self) -> Option<Dec> {
        match self {
            Opd::Num(n) => Some(*n),
            _ => None,
        }
    }
}

/// Calls `f(op, operands)` for every operator of content stream `s` in order.
fn content_ops(s: &[u8], mut f: impl FnMut(&[u8], &[Opd])) {
    let n = s.len();
    let mut i = 0;
    let mut stack: Vec<Vec<Opd>> = vec![Vec::new()];
    while i < n {
        let c = s[i];
        if is_ws(c) {
            i += 1;
        } else if c == b'%' {
            while i < n && !b"\r\n".contains(&s[i]) {
                i += 1;
            }
        } else if c == b'(' {
            let v = literal_string(s, &mut i);
            stack.last_mut().unwrap().push(Opd::Str(v));
        } else if c == b'<' && s.get(i + 1) == Some(&b'<') {
            stack.push(Vec::new());
            i += 2;
        } else if c == b'>' && s.get(i + 1) == Some(&b'>') {
            stack.pop();
            if stack.is_empty() {
                stack.push(Vec::new());
            }
            stack.last_mut().unwrap().push(Opd::Dict);
            i += 2;
        } else if c == b'<' {
            let v = hex_string(s, &mut i);
            stack.last_mut().unwrap().push(Opd::Str(v));
        } else if c == b'[' {
            stack.push(Vec::new());
            i += 1;
        } else if c == b']' {
            let a = stack.pop().unwrap_or_default();
            if stack.is_empty() {
                stack.push(Vec::new());
            }
            stack.last_mut().unwrap().push(Opd::Arr(a));
            i += 1;
        } else if c == b'/' {
            let mut j = i + 1;
            while j < n && is_regular(s[j]) {
                j += 1;
            }
            stack.last_mut().unwrap().push(Opd::Name(s[i + 1..j].to_vec()));
            i = j;
        } else {
            let mut j = i;
            while j < n && is_regular(s[j]) {
                j += 1;
            }
            if j == i {
                i += 1;
                continue;
            }
            let w = &s[i..j];
            i = j;
            if let Some(d) = Dec::parse(w) {
                stack.last_mut().unwrap().push(Opd::Num(d));
            } else if w == b"true" || w == b"false" || w == b"null" {
                stack.last_mut().unwrap().push(Opd::Word);
            } else if w == b"BI" {
                let k = s[j..].windows(2).position(|x| x == b"EI").map(|p| p + j);
                stack.last_mut().unwrap().clear();
                f(b"BI", &[]);
                i = k.map_or(n, |k| k + 2);
            } else {
                let ops = std::mem::take(stack.last_mut().unwrap());
                f(w, &ops);
            }
        }
    }
}

/// A fill or stroke colour: its operator and operands (numbers first, then names).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Colour {
    op: Vec<u8>,
    nums: Vec<Dec>,
    names: Vec<Vec<u8>>,
}

fn colour(ops: &[Opd], op: &[u8]) -> Colour {
    Colour {
        op: op.to_vec(),
        nums: ops.iter().filter_map(Opd::num).collect(),
        names: ops.iter().filter_map(|o| if let Opd::Name(n) = o { Some(n.clone()) } else { None }).collect(),
    }
}

/// One path element in device space.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Seg {
    op: u8,
    pts: Vec<(Dec, Dec)>,
}

/// What a page draws.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Mark {
    Glyph { font: std::rc::Rc<str>, code: u8, m: Mat, col: Option<Colour> },
    Path { op: Vec<u8>, path: Vec<Seg>, col: Option<Colour> },
    Image { m: Mat, w: String, h: String },
}

impl Mark {
    fn colourless(&self) -> Mark {
        match self {
            Mark::Glyph { font, code, m, .. } => Mark::Glyph { font: font.clone(), code: *code, m: *m, col: None },
            Mark::Path { op, path, .. } => Mark::Path { op: op.clone(), path: path.clone(), col: None },
            i => i.clone(),
        }
    }
    fn is_glyph(&self) -> bool {
        matches!(self, Mark::Glyph { .. })
    }
}

/// A font as the interpreter needs it: its name less a subset tag, and its widths.
struct FontInfo {
    base: std::rc::Rc<str>,
    first: i64,
    widths: Vec<Dec>,
    matrix: Option<Mat>,
}

/// Every number of an array as `pdfexact.py` reads its text (a reference counts as its two
/// numbers).
fn array_numbers(a: &[Obj]) -> Vec<Dec> {
    let mut v = Vec::new();
    for x in a {
        match x {
            Obj::Num(n) => v.push(*n),
            Obj::Ref(n, g) => {
                v.push(Dec::int(*n as i64));
                v.push(Dec::int(*g as i64));
            }
            _ => {}
        }
    }
    v
}

impl FontInfo {
    fn new(pdf: &Pdf, id: u32) -> FontInfo {
        let f = pdf.get(id);
        let base = match f.get("BaseFont") {
            Some(Obj::Ref(n, _)) => pdf.get(*n).name().map(latin1).unwrap_or_default(),
            Some(Obj::Name(n)) => latin1(n),
            Some(o) => o.text(),
            None => "null".into(),
        };
        let b = base.as_bytes();
        let base = if b.len() > 7 && b[6] == b'+' && b[..6].iter().all(u8::is_ascii_uppercase) { base[7..].to_string() } else { base };
        let first = match f.get("FirstChar") {
            Some(Obj::Num(n)) if n.e == 0 => n.m as i64,
            _ => 0,
        };
        let widths = match f.get("Widths") {
            Some(Obj::Arr(a)) => array_numbers(a),
            Some(Obj::Ref(n, _)) => array_numbers(pdf.get(*n).arr()),
            _ => Vec::new(),
        };
        let matrix = match f.get("FontMatrix") {
            Some(Obj::Arr(a)) => {
                let m = array_numbers(a);
                m.try_into().ok()
            }
            _ => None,
        };
        FontInfo { base: base.into(), first, widths, matrix }
    }
    fn width(&self, code: u8) -> Dec {
        let k = code as i64 - self.first;
        let w = if k >= 0 && (k as usize) < self.widths.len() { self.widths[k as usize] } else { ZERO };
        match &self.matrix {
            Some(m) => w.mul(m[0]),
            None => w.div10(3),
        }
    }
}

/// The `Font` and `XObject` resources of object `id`, by name (indirect entries only).
fn resources(pdf: &Pdf, id: u32) -> [HashMap<Vec<u8>, u32>; 2] {
    let o = pdf.get(id);
    let res = pdf.key(&o, "Resources");
    ["Font", "XObject"].map(|kind| {
        let d = pdf.key(&res, kind);
        d.dict().iter().filter_map(|(k, v)| if let Obj::Ref(n, 0) = v { Some((k.clone(), *n)) } else { None }).collect()
    })
}

#[derive(Clone)]
struct GState {
    ctm: Mat,
    fill: Colour,
    stroke: Colour,
    font: Option<u32>,
    size: Dec,
    tc: Dec,
    tw: Dec,
    th: Dec,
    tl: Dec,
    rise: Dec,
}

/// One glyph as text extraction needs it: font object, code and rendering matrix.
#[derive(Clone, Debug)]
struct GlyphAt {
    code: u8,
    m: Mat,
}

struct Interp<'a> {
    pdf: &'a Pdf,
    fonts: HashMap<u32, Option<FontInfo>>,
    out: Vec<Mark>,
    glyphs: Vec<GlyphAt>,
}

impl Interp<'_> {
    fn run(&mut self, id: u32, stream: &[u8], ctm: Mat, depth: u32) {
        let [font_res, xobj_res] = resources(self.pdf, id);
        let gray = |op: &[u8]| Colour { op: op.to_vec(), nums: vec![ZERO], names: Vec::new() };
        let mut st = GState { ctm, fill: gray(b"g"), stroke: gray(b"G"), font: None, size: ZERO, tc: ZERO, tw: ZERO, th: ONE, tl: ZERO, rise: ZERO };
        let mut stack: Vec<GState> = Vec::new();
        let (mut tm, mut lm) = (IDENTITY, IDENTITY);
        let mut path: Vec<Seg> = Vec::new();
        let n = |a: &[Opd], k: usize| a.get(k).and_then(Opd::num).unwrap_or(ZERO);
        let m6 = |a: &[Opd]| -> Mat { [n(a, 0), n(a, 1), n(a, 2), n(a, 3), n(a, 4), n(a, 5)] };
        content_ops(stream, |op, a| match op {
            b"q" => stack.push(st.clone()),
            b"Q" => {
                if let Some(s) = stack.pop() {
                    st = s;
                }
            }
            b"cm" => st.ctm = mul(&m6(a), &st.ctm),
            b"g" | b"rg" | b"k" | b"sc" | b"scn" | b"cs" => st.fill = colour(a, op),
            b"G" | b"RG" | b"K" | b"SC" | b"SCN" | b"CS" => st.stroke = colour(a, op),
            b"BT" => {
                tm = IDENTITY;
                lm = IDENTITY;
            }
            b"Tf" => {
                let x = match a.first() {
                    Some(Opd::Name(nm)) => font_res.get(nm).copied(),
                    _ => None,
                };
                if let Some(x) = x {
                    let pdf = self.pdf;
                    self.fonts.entry(x).or_insert_with(|| Some(FontInfo::new(pdf, x)));
                }
                st.font = x;
                st.size = n(a, 1);
            }
            b"Tc" => st.tc = n(a, 0),
            b"Tw" => st.tw = n(a, 0),
            b"Tz" => st.th = n(a, 0).div10(2),
            b"TL" => st.tl = n(a, 0),
            b"Ts" => st.rise = n(a, 0),
            b"Td" | b"TD" => {
                if op == b"TD" {
                    st.tl = n(a, 1).neg();
                }
                lm = mul(&translate(n(a, 0), n(a, 1)), &lm);
                tm = lm;
            }
            b"Tm" => {
                lm = m6(a);
                tm = lm;
            }
            b"T*" => {
                lm = mul(&translate(ZERO, st.tl.neg()), &lm);
                tm = lm;
            }
            b"Tj" | b"TJ" | b"'" | b"\"" => {
                if op == b"'" || op == b"\"" {
                    if op == b"\"" {
                        st.tw = n(a, 0);
                        st.tc = n(a, 1);
                    }
                    lm = mul(&translate(ZERO, st.tl.neg()), &lm);
                    tm = lm;
                }
                let items: Vec<Opd> = match (op, a.last()) {
                    (b"TJ", Some(Opd::Arr(v))) => v.clone(),
                    (b"TJ", _) => Vec::new(),
                    (_, Some(x)) => vec![x.clone()],
                    _ => Vec::new(),
                };
                let font = st.font.and_then(|x| self.fonts.get(&x)).and_then(Option::as_ref);
                let base: std::rc::Rc<str> = font.map_or_else(|| "?".into(), |f| f.base.clone());
                let scale = [st.size.mul(st.th), ZERO, ZERO, st.size, ZERO, st.rise];
                for it in &items {
                    match it {
                        Opd::Num(d) => tm = mul(&translate(d.div10(3).neg().mul(st.size).mul(st.th), ZERO), &tm),
                        Opd::Str(bytes) => {
                            for &code in bytes {
                                let trm = mul(&mul(&scale, &tm), &st.ctm);
                                self.out.push(Mark::Glyph { font: base.clone(), code, m: trm, col: Some(st.fill.clone()) });
                                self.glyphs.push(GlyphAt { code, m: trm });
                                let w = font.map_or(ZERO, |f| f.width(code));
                                let tx = w.mul(st.size).add(st.tc).add(if code == 32 { st.tw } else { ZERO }).mul(st.th);
                                tm = mul(&translate(tx, ZERO), &tm);
                            }
                        }
                        _ => {}
                    }
                }
            }
            b"re" => {
                let (x, y, w, h) = (n(a, 0), n(a, 1), n(a, 2), n(a, 3));
                path.push(Seg { op: b'r', pts: vec![apply(&st.ctm, x, y), apply(&st.ctm, x.add(w), y.add(h))] });
            }
            b"m" | b"l" => path.push(Seg { op: op[0], pts: vec![apply(&st.ctm, n(a, 0), n(a, 1))] }),
            b"c" | b"v" | b"y" => {
                let pts = (0..a.len().saturating_sub(1)).step_by(2).map(|k| apply(&st.ctm, n(a, k), n(a, k + 1))).collect();
                path.push(Seg { op: op[0], pts });
            }
            b"h" => path.push(Seg { op: b'h', pts: Vec::new() }),
            b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" | b"S" | b"s" => {
                let col = if op == b"S" || op == b"s" { st.stroke.clone() } else { st.fill.clone() };
                self.out.push(Mark::Path { op: op.to_vec(), path: std::mem::take(&mut path), col: Some(col) });
            }
            b"n" => path.clear(),
            b"Do" => {
                let Some(Opd::Name(nm)) = a.first() else { return };
                let Some(&x) = xobj_res.get(nm) else { return };
                let xo = self.pdf.get(x);
                if xo.get("Subtype").and_then(Obj::name) == Some(b"Form") && depth < 8 {
                    let fm = match xo.get("Matrix") {
                        Some(Obj::Arr(v)) => array_numbers(v).try_into().unwrap_or(IDENTITY),
                        _ => IDENTITY,
                    };
                    let data = self.pdf.stream_bytes(&xo);
                    self.run(x, &data, mul(&fm, &st.ctm), depth + 1);
                } else {
                    let t = |k: &str| xo.get(k).map_or_else(|| "null".to_string(), Obj::text);
                    self.out.push(Mark::Image { m: st.ctm, w: t("Width"), h: t("Height") });
                }
            }
            _ => {}
        });
    }
}

/// Every mark of page `i`, and its glyphs in drawing order.
fn page_marks(pdf: &Pdf, i: usize, fonts: &mut HashMap<u32, Option<FontInfo>>) -> (Vec<Mark>, Vec<GlyphAt>) {
    let id = pdf.pages[i];
    let stream = page_content(pdf, i);
    let mut it = Interp { pdf, fonts: std::mem::take(fonts), out: Vec::new(), glyphs: Vec::new() };
    it.run(id, &stream, IDENTITY, 0);
    *fonts = it.fonts;
    (it.out, it.glyphs)
}

/// Page `i`'s content streams, decoded and joined.
fn page_content(pdf: &Pdf, i: usize) -> Vec<u8> {
    let page = pdf.get(pdf.pages[i]);
    let mut stream = Vec::new();
    match page.get("Contents") {
        Some(Obj::Ref(n, _)) => {
            let c = pdf.get(*n);
            match &*c {
                Obj::Arr(a) => {
                    for x in a {
                        if let Obj::Ref(m, _) = x {
                            stream.extend(pdf.stream_bytes(&pdf.get(*m)));
                        }
                    }
                }
                s => stream.extend(pdf.stream_bytes(s)),
            }
        }
        Some(Obj::Arr(a)) => {
            for x in a {
                if let Obj::Ref(m, _) = x {
                    stream.extend(pdf.stream_bytes(&pdf.get(*m)));
                }
            }
        }
        _ => {}
    }
    stream
}

fn counter<T: std::hash::Hash + Eq + Clone>(v: impl IntoIterator<Item = T>) -> HashMap<T, u32> {
    let mut m = HashMap::new();
    for x in v {
        *m.entry(x).or_insert(0) += 1;
    }
    m
}

fn intersect_sum<T: std::hash::Hash + Eq>(a: &HashMap<T, u32>, b: &HashMap<T, u32>, pick: impl Fn(&T) -> bool) -> u32 {
    a.iter().filter(|(k, _)| pick(k)).map(|(k, &v)| v.min(*b.get(k).unwrap_or(&0))).sum()
}

/// Per page: (ref glyphs, glyphs matched exactly, ref paths, paths matched, colour mismatches
/// among position-matched marks, our marks without a twin), as `compare_marks`.
fn compare_marks(a: &Pdf, b: &Pdf) -> Vec<[u32; 6]> {
    let (mut fa, mut fb) = (HashMap::new(), HashMap::new());
    (0..a.pages.len())
        .map(|i| {
            let ma = page_marks(a, i, &mut fa).0;
            let mb = if i < b.pages.len() { page_marks(b, i, &mut fb).0 } else { Vec::new() };
            let (ca, cb) = (counter(ma.iter().cloned()), counter(mb.iter().cloned()));
            let (pa, pb) = (counter(ma.iter().map(Mark::colourless)), counter(mb.iter().map(Mark::colourless)));
            let both = intersect_sum(&ca, &cb, |_| true);
            let pos_all = intersect_sum(&pa, &pb, |_| true);
            let g = ma.iter().filter(|m| m.is_glyph()).count() as u32;
            let gm = intersect_sum(&pa, &pb, Mark::is_glyph);
            let p = ma.len() as u32 - g;
            let pm = pos_all - gm;
            let extra: u32 = cb.iter().map(|(k, &v)| v.saturating_sub(*ca.get(k).unwrap_or(&0))).sum();
            [g, gm, p, pm, pos_all - both, extra]
        })
        .collect()
}

/// The first ref mark of `page` (1-based) without an exact twin, and our nearest glyph.
fn first_difference(a: &Pdf, b: &Pdf, page: usize) -> (Option<Mark>, Option<Mark>) {
    let (mut fa, mut fb) = (HashMap::new(), HashMap::new());
    let ma = page_marks(a, page - 1, &mut fa).0;
    let mb = page_marks(b, page - 1, &mut fb).0;
    let mut left = counter(mb.iter().cloned());
    for m in ma {
        if let Some(v) = left.get_mut(&m).filter(|v| **v > 0) {
            *v -= 1;
            continue;
        }
        let near = if let Mark::Glyph { code, m: mm, .. } = &m {
            left.iter()
                .filter(|(k, &v)| v > 0 && matches!(k, Mark::Glyph { code: c, .. } if c == code))
                .min_by_key(|(k, _)| {
                    let Mark::Glyph { m: km, .. } = k else { unreachable!() };
                    km[4].sub(mm[4]).f64().abs().to_bits() as u128 + km[5].sub(mm[5]).f64().abs().to_bits() as u128
                })
                .map(|(k, _)| k.clone())
        } else {
            None
        };
        return (Some(m), near);
    }
    (None, None)
}

// ---- structure: links, outlines, destinations, metadata ----------------------------------------

/// A PDF text string as Unicode: UTF-16BE or UTF-8 with a byte order mark, else
/// PDFDocEncoding (Latin-1 is exact for every code the corpus writes).
fn text_string(b: &[u8]) -> String {
    if b.starts_with(&[0xfe, 0xff]) {
        let u: Vec<u16> = b[2..].chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
        return String::from_utf16_lossy(&u);
    }
    if b.starts_with(&[0xef, 0xbb, 0xbf]) {
        return String::from_utf8_lossy(&b[3..]).into_owned();
    }
    latin1(b)
}

struct Structure {
    links: Vec<String>,
    outlines: Vec<String>,
    meta: Vec<String>,
    dests: Option<BTreeMap<String, String>>,
}

const META: [&str; 5] = ["Title", "Author", "Subject", "Keywords", "Creator"];

impl Pdf {
    /// Named destinations: the `Dests` name tree of the catalog's `Names`, then its `Dests`
    /// dictionary.
    fn named_dests(&self) -> BTreeMap<String, Obj> {
        let root = self.resolve(&self.trailer_get("Root").unwrap_or(Obj::Null));
        let mut out = BTreeMap::new();
        let names = self.key(&root, "Names");
        let tree = self.key(&names, "Dests");
        self.name_tree(&tree, &mut out, 0);
        for (k, v) in self.key(&root, "Dests").dict() {
            out.entry(latin1(k)).or_insert_with(|| self.resolve(v));
        }
        out
    }

    fn name_tree(&self, node: &Obj, out: &mut BTreeMap<String, Obj>, depth: u32) {
        if depth > 32 {
            return;
        }
        for pair in self.key(node, "Names").arr().chunks(2) {
            if let [Obj::Str(k), v] = pair {
                out.insert(latin1(k), self.resolve(v));
            }
        }
        for kid in self.key(node, "Kids").arr() {
            self.name_tree(&self.resolve(kid), out, depth + 1);
        }
    }

    /// A destination (array, or `<< /D … >>`) as page index and view.
    fn dest_text(&self, d: &Obj) -> Option<(Option<usize>, String)> {
        let d = self.resolve(d);
        let d = if let Obj::Dict(_) = d { self.key(&d, "D") } else { d };
        let a = d.arr();
        let page = match a.first() {
            Some(Obj::Ref(n, _)) => self.page_index(*n),
            Some(Obj::Num(n)) => Some(n.m as usize),
            _ => None,
        };
        let view = a.iter().skip(1).map(|x| match x {
            Obj::Num(n) => format!("{:?}", n.round(4)),
            o => o.text(),
        });
        Some((page, view.collect::<Vec<_>>().join(" ")))
    }

    /// Where an action or destination goes, as one string.
    fn target(&self, dests: &BTreeMap<String, Obj>, a: &Obj, d: Option<&Obj>) -> String {
        let a = self.resolve(a);
        if let Some(d) = d {
            return self.goto(dests, d);
        }
        match self.key(&a, "S").name() {
            Some(b"URI") => format!("uri {}", latin1(match self.key(&a, "URI") {
                Obj::Str(s) => s,
                _ => Vec::new(),
            }.as_slice())),
            Some(b"GoTo") => a.get("D").map_or_else(|| "goto ?".into(), |d| self.goto(dests, d)),
            Some(b"GoToR") => format!("gotor {} {}", self.key(&a, "F").text(), self.key(&a, "D").text()),
            Some(b"Named") => format!("named {}", self.key(&a, "N").text()),
            Some(s) => format!("action {}", latin1(s)),
            None => "none".into(),
        }
    }

    fn goto(&self, dests: &BTreeMap<String, Obj>, d: &Obj) -> String {
        match self.resolve(d) {
            Obj::Str(s) | Obj::Name(s) => format!("dest {}", latin1(&s)),
            other => match self.dest_text(&other) {
                Some((Some(p), _)) => format!("page {p}"),
                _ => {
                    let _ = dests;
                    "page ?".into()
                }
            },
        }
    }

    fn structure(&self) -> Structure {
        let dests = self.named_dests();
        let mut links = Vec::new();
        for (i, &p) in self.pages.iter().enumerate() {
            let page = self.get(p);
            for an in self.key(&page, "Annots").arr() {
                let an = self.resolve(an);
                if self.key(&an, "Subtype").name() != Some(b"Link") {
                    continue;
                }
                let r = array_numbers(self.key(&an, "Rect").arr());
                if r.len() != 4 {
                    continue;
                }
                let (x0, x1) = (r[0].min(r[2]), r[0].max(r[2]));
                let (y0, y1) = (r[1].min(r[3]), r[1].max(r[3]));
                let to = self.target(&dests, &self.key(&an, "A"), an.get("Dest"));
                links.push(format!("{i} {:?} {:?} {:?} {:?} {to}", x0.round(3), y0.round(3), x1.round(3), y1.round(3)));
            }
        }
        let root = self.resolve(&self.trailer_get("Root").unwrap_or(Obj::Null));
        let mut outlines = Vec::new();
        let first = self.key(&self.key(&root, "Outlines"), "First");
        self.outline(&dests, &first, 1, &mut outlines, &mut Vec::new());
        let info = self.resolve(&self.trailer_get("Info").unwrap_or(Obj::Null));
        let meta = META
            .iter()
            .map(|k| match self.key(&info, k) {
                Obj::Str(s) => text_string(&s),
                _ => String::new(),
            })
            .collect();
        let dests = Some(dests.iter().map(|(k, v)| (k.clone(), self.dest_text(v).map_or_else(|| "?".into(), |(p, view)| format!("{p:?} {view}")))).collect());
        Structure { links, outlines, meta, dests }
    }

    fn outline(&self, dests: &BTreeMap<String, Obj>, node: &Obj, level: u32, out: &mut Vec<String>, seen: &mut Vec<u32>) {
        let mut cur = node.clone();
        while let Obj::Dict(_) = cur {
            let title = match self.key(&cur, "Title") {
                Obj::Str(s) => text_string(&s),
                _ => String::new(),
            };
            let to = self.target(dests, &self.key(&cur, "A"), cur.get("Dest"));
            let to = match to.strip_prefix("dest ").and_then(|n| dests.get(n)) {
                Some(d) => self.dest_text(d).and_then(|x| x.0).map_or("page ?".into(), |p| format!("page {p}")),
                None => to,
            };
            out.push(format!("{level} {title} {to}"));
            let first = self.key(&cur, "First");
            self.outline(dests, &first, level + 1, out, seen);
            let next = cur.get("Next").cloned();
            match next {
                Some(Obj::Ref(n, _)) if !seen.contains(&n) && seen.len() < 100_000 => {
                    seen.push(n);
                    cur = (*self.get(n)).clone();
                }
                _ => break,
            }
        }
    }
}

/// Each part: (ref count, matched, equal), in the order links, outlines, meta, dests.
fn compare_structure(a: &Pdf, b: &Pdf) -> Vec<(&'static str, usize, usize, bool)> {
    let (sa, sb) = (a.structure(), b.structure());
    let mut out = Vec::new();
    for (k, x, y) in [("links", &sa.links, &sb.links), ("outlines", &sa.outlines, &sb.outlines)] {
        let (ca, cb) = (counter(x.iter().cloned()), counter(y.iter().cloned()));
        out.push((k, x.len(), intersect_sum(&ca, &cb, |_| true) as usize, ca == cb));
    }
    out.push(("meta", META.len(), sa.meta.iter().zip(&sb.meta).filter(|(x, y)| x == y).count(), sa.meta == sb.meta));
    let (da, db) = (sa.dests.clone().unwrap_or_default(), sb.dests.clone().unwrap_or_default());
    let same = da.iter().filter(|(k, v)| db.get(*k) == Some(v)).count();
    out.push(("dests", da.len(), same, sa.dests.is_some() && da == db));
    out
}

// ---- pixels ---------------------------------------------------------------------------------

/// The number of pixels that differ in any channel between two RGB images of one size.
///
/// Identical 4 KiB runs are skipped by slice comparison (the C library's vectorised
/// `memcmp`); a run that differs is counted 16 pixels at a time with NEON (`vld3q_u8`
/// splits the 48 bytes into R, G and B lanes, one compare per channel, one AND, and a
/// population count of the result), or with the portable loop elsewhere.
fn count_differing_pixels(a: &[u8], b: &[u8]) -> u64 {
    const RUN: usize = 4096 * 3;
    let mut total = 0;
    for (x, y) in a.chunks(RUN).zip(b.chunks(RUN)) {
        if x != y {
            total += count_rgb(x, y);
        }
    }
    total
}

#[cfg(target_arch = "aarch64")]
fn count_rgb(a: &[u8], b: &[u8]) -> u64 {
    use std::arch::aarch64::*;
    let n = a.len().min(b.len()) / 48 * 48;
    let mut diff = 0u64;
    // SAFETY: NEON is part of the aarch64 baseline; every load reads 48 bytes at `i` with
    // `i + 48 <= n <= len` for both slices.
    unsafe {
        let mut i = 0;
        while i < n {
            let x = vld3q_u8(a.as_ptr().add(i));
            let y = vld3q_u8(b.as_ptr().add(i));
            let same = vandq_u8(vandq_u8(vceqq_u8(x.0, y.0), vceqq_u8(x.1, y.1)), vceqq_u8(x.2, y.2));
            // Each equal pixel is 0xff: 16 - (count of set bits / 8) differ.
            diff += 16 - (vaddlvq_u8(vcntq_u8(same)) as u64 / 8);
            i += 48;
        }
    }
    diff + count_rgb_scalar(&a[n..], &b[n..])
}

#[cfg(not(target_arch = "aarch64"))]
fn count_rgb(a: &[u8], b: &[u8]) -> u64 {
    count_rgb_scalar(a, b)
}

fn count_rgb_scalar(a: &[u8], b: &[u8]) -> u64 {
    a.as_chunks::<3>().0.iter().zip(b.as_chunks::<3>().0).filter(|(x, y)| x != y).count() as u64
}

/// Per page, the MD5 MuPDF reports of the page rendered at `dpi` (`mutool draw -s 5`).
fn render_md5(pdf: &Path, dpi: u32) -> Result<Vec<String>, String> {
    let n = Pdf::open(pdf)?.pages.len();
    let k = JOBS.load(Ordering::Relaxed).clamp(1, n.max(1));
    let ranges: Vec<String> = (0..k).map(|i| (i * n / k + 1, (i + 1) * n / k)).filter(|r| r.0 <= r.1).map(|(a, b)| format!("{a}-{b}")).collect();
    let outs: Vec<std::io::Result<std::process::Output>> = std::thread::scope(|sc| {
        let hs: Vec<_> = ranges.iter().map(|r| sc.spawn(move || Command::new("mutool").args(["draw", "-q", "-r", &dpi.to_string(), "-c", "rgb", "-s", "5", "-o", "/dev/null", "-F", "pnm"]).arg(pdf).arg(r).output())).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut text = String::new();
    for out in outs {
        let out = out.map_err(|e| format!("mutool: {e}"))?;
        text += &String::from_utf8_lossy(&out.stderr);
        text += &String::from_utf8_lossy(&out.stdout);
    }
    let mut pages: Vec<(usize, String)> = text
        .lines()
        .filter(|l| l.starts_with("page "))
        .filter_map(|l| {
            let mut w = l.rsplitn(3, ' ');
            let md5 = w.next()?.to_string();
            let n = w.next()?.parse().ok()?;
            Some((n, md5))
        })
        .collect();
    pages.sort();
    // One hash per page, or the output was not understood: never a silent pass.
    if pages.len() != n || pages.iter().enumerate().any(|(i, p)| p.0 != i + 1 || p.1.len() != 32) {
        return Err(format!("{}: mutool gave {} page hashes for {n} pages", pdf.display(), pages.len()));
    }
    Ok(pages.into_iter().map(|p| p.1).collect())
}

/// The MD5s of `pdf`, from the cache when the file is unchanged.
fn cached_md5(pdf: &Path, dpi: u32, kind: &str) -> Result<Vec<String>, String> {
    let meta = std::fs::metadata(pdf).map_err(|e| format!("{}: {e}", pdf.display()))?;
    let stamp = format!("{} {:?} {dpi}", meta.len(), meta.modified().ok());
    let cache = out_dir().join("pixcache").join(kind).join(format!("{}.md5", pdf.file_stem().unwrap_or_default().to_string_lossy()));
    if let Ok(t) = std::fs::read_to_string(&cache) {
        let mut l = t.lines();
        if l.next() == Some(stamp.as_str()) {
            return Ok(l.map(str::to_string).collect());
        }
    }
    let v = render_md5(pdf, dpi)?;
    let _ = std::fs::create_dir_all(cache.parent().unwrap());
    let _ = std::fs::write(&cache, format!("{stamp}\n{}\n", v.join("\n")));
    Ok(v)
}

/// Page `page` (1-based) of `pdf` at `dpi` as (width, height, RGB samples).
fn render_page(pdf: &Path, page: usize, dpi: u32) -> Result<(usize, usize, Vec<u8>), String> {
    let out = Command::new("mutool")
        .args(["draw", "-q", "-r", &dpi.to_string(), "-c", "rgb", "-F", "pnm", "-o", "-"])
        .arg(pdf)
        .arg(page.to_string())
        .output()
        .map_err(|e| format!("mutool: {e}"))?;
    let d = out.stdout;
    // `P6\n<w> <h>\n255\n` then the samples.
    let mut fields = Vec::new();
    let mut i = 0;
    while fields.len() < 4 && i < d.len() {
        while i < d.len() && is_ws(d[i]) {
            i += 1;
        }
        let s = i;
        while i < d.len() && !is_ws(d[i]) {
            i += 1;
        }
        fields.push(String::from_utf8_lossy(&d[s..i]).into_owned());
    }
    i += 1;
    let (w, h): (usize, usize) = (fields.get(1).and_then(|x| x.parse().ok()).unwrap_or(0), fields.get(2).and_then(|x| x.parse().ok()).unwrap_or(0));
    Ok((w, h, d.get(i..).unwrap_or(&[]).to_vec()))
}

/// Differing pixels per page of `a` (any channel), as `pixel_diffs`: a page we lack counts
/// all its pixels, as does a page of another size.
fn pixel_diffs(ref_pdf: &Path, ours: &Path, dpi: u32, ours_kind: &str) -> Result<Vec<u64>, String> {
    let (ha, hb) = std::thread::scope(|sc| {
        let h = sc.spawn(|| cached_md5(ref_pdf, dpi, "ref"));
        let b = cached_md5(ours, dpi, ours_kind);
        (h.join().unwrap(), b)
    });
    let (ha, hb) = (ha?, hb?);
    let mut rows = Vec::with_capacity(ha.len());
    for (i, h) in ha.iter().enumerate() {
        if hb.get(i) == Some(h) {
            rows.push(0);
            continue;
        }
        let (wa, hha, pa) = render_page(ref_pdf, i + 1, dpi)?;
        if i >= hb.len() {
            rows.push((wa * hha) as u64);
            continue;
        }
        let (wb, hhb, pb) = render_page(ours, i + 1, dpi)?;
        rows.push(if (wa, hha) != (wb, hhb) { (wa * hha) as u64 } else { count_differing_pixels(&pa, &pb) });
    }
    Ok(rows)
}

// ---- text: glyphs as MuPDF extracts them --------------------------------------------------------

/// A visible glyph: origin (x, y down from the top), its text, size and font.
#[derive(Clone, Debug)]
struct Glyph {
    x: f64,
    y: f64,
    c: String,
    size: f64,
    /// Shared by every glyph of a span.
    font: std::sync::Arc<str>,
}

/// Python's `str.isspace` for one character (Rust's `is_whitespace` plus the four C0
/// separators).
fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

/// What `mutool draw -F trace` says of every glyph, page by page: (font, Unicode).
/// MuPDF's trace of every page, in page order: one `mutool` per range of pages, the ranges
/// run at once (MuPDF here is built without threads).
fn trace_text(pdf: &Path) -> Result<Vec<Vec<Glyph>>, String> {
    let n = Pdf::open(pdf)?.pages.len();
    let k = JOBS.load(Ordering::Relaxed).clamp(1, n.div_ceil(4).max(1));
    let ranges: Vec<(usize, usize)> = (0..k).map(|i| (i * n / k + 1, (i + 1) * n / k)).filter(|r| r.0 <= r.1).collect();
    let parts: Vec<Result<Vec<Vec<Glyph>>, String>> = std::thread::scope(|sc| {
        let hs: Vec<_> = ranges.iter().map(|&(a, b)| sc.spawn(move || trace_pages(pdf, &format!("{a}-{b}")))).collect();
        hs.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("trace panicked".into()))).collect()
    });
    let mut pages = Vec::with_capacity(n);
    for p in parts {
        pages.extend(p?);
    }
    Ok(pages)
}

/// The number of worker threads (`-j`), for the work split inside one document.
static JOBS: AtomicUsize = AtomicUsize::new(1);

fn trace_pages(pdf: &Path, range: &str) -> Result<Vec<Vec<Glyph>>, String> {
    // The trace is mostly paths: read it a line at a time, never whole.
    let mut child = Command::new("mutool").args(["draw", "-q", "-F", "trace", "-o", "-"]).arg(pdf).arg(range).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| format!("mutool: {e}"))?;
    let mut r = std::io::BufReader::with_capacity(1 << 16, child.stdout.take().unwrap());
    let mut buf = Vec::with_capacity(256);
    let attr = |s: &str, k: &str| -> Option<String> {
        let p = s.find(&format!(" {k}=\""))? + k.len() + 3;
        Some(xml_unescape(&s[p..p + s[p..].find('"')?]))
    };
    // MuPDF computes in `f32`: positions done the same way are pymupdf's to the bit.
    let nums = |s: Option<String>| -> Vec<f32> { s.map_or(Vec::new(), |v| v.split_whitespace().filter_map(|x| x.parse().ok()).collect()) };
    let identity: Vec<f32> = vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut pages: Vec<Vec<Glyph>> = Vec::new();
    let (mut ctm, mut trm, mut font) = (identity.clone(), vec![1.0, 0.0, 0.0, 1.0], std::sync::Arc::<str>::from(""));
    let mut base = (0f32, 0f32);
    let (mut page_w, mut page_h) = (f32::INFINITY, f32::INFINITY);
    loop {
        buf.clear();
        if std::io::BufRead::read_until(&mut r, b'\n', &mut buf).map_err(|e| format!("mutool: {e}"))? == 0 {
            break;
        }
        let line = String::from_utf8_lossy(&buf);
        let l = line.trim_start();
        if l.starts_with("<page ") {
            pages.push(Vec::new());
            if let [x0, y0, x1, y1] = nums(attr(l, "mediabox")).as_slice() {
                (page_w, page_h) = ((x1 - x0).abs(), (y1 - y0).abs());
            }
        } else if l.starts_with("<span ") {
            let f = attr(l, "font").unwrap_or_default();
            let f = f.rsplit('+').next().unwrap_or("");
            if *font != *f {
                font = f.into();
            }
            trm = nums(attr(l, "trm"));
        } else if l.starts_with("<g ") {
            let (Some(p), [a, b, c, d, e, f], [ta, tb, tc, td]) = (pages.last_mut(), ctm.as_slice(), trm.as_slice()) else { continue };
            let (gx, gy) = (nums(attr(l, "x")), nums(attr(l, "y")));
            let (Some(&x), Some(&y)) = (gx.first(), gy.first()) else { continue };
            let ch = attr(l, "unicode").unwrap_or_default();
            // A ligature's later characters (no glyph of their own) and a combining mark stand
            // where the previous base character ends, on its baseline, as MuPDF's text
            // extraction places them.
            let mark = attr(l, "glyph").is_none() || ch.chars().next().is_some_and(combining);
            let at = |x: f32, y: f32| (x * a + y * c + e, x * b + y * d + f);
            let adv = nums(attr(l, "adv")).first().copied().unwrap_or(0.0);
            // The glyph's matrix in device space, trm · ctm.
            let (ma, mb, mc, md) = (ta * a + tb * c, ta * b + tb * d, tc * a + td * c, tc * b + td * d);
            let (x, y) = if mark {
                base
            } else {
                let (x, y) = at(x, y);
                base = (x + adv * ma, y + adv * mb);
                (x, y)
            };
            // Its size, `fz_matrix_expansion`.
            let size = (ma * md - mb * mc).abs().sqrt();
            // Nor a glyph whose box lies wholly off the page (`mediabox-clip`).
            if !mark && (x >= page_w || base.0 <= 0.0 || y - size >= page_h || y + 0.3 * size <= 0.0) {
                continue;
            }
            let g = Glyph { x: x as f64, y: y as f64, c: ch, size: size as f64, font: font.clone() };
            // A character drawn again slightly offset (fake bold, `\pmb`) is extracted once; one
            // drawn again at the very same place is extracted twice.
            let tol = 0.1 * g.size;
            let near = |h: &Glyph| (h.x - g.x).abs() < tol && (h.y - g.y).abs() < tol && (h.x != g.x || h.y != g.y);
            if mark || !p.iter().rev().take(32).any(|h| h.c == g.c && h.font == g.font && h.size == g.size && near(h)) {
                p.push(g);
            }
        } else if l.starts_with("<fill_text ") || l.starts_with("<stroke_text ") || l.starts_with("<clip_text ") || l.starts_with("<clip_stroke_text ") || l.starts_with("<ignore_text ") {
            ctm = nums(attr(l, "transform"));
            if ctm.len() != 6 {
                ctm = identity.clone();
            }
        }
    }
    let _ = child.wait();
    Ok(pages)
}

/// Unicode's combining diacritical marks (general category Mn in the blocks fonts map to).
fn combining(c: char) -> bool {
    matches!(c as u32, 0x300..=0x36f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe20..=0xfe2f)
}

fn xml_unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::new();
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        let Some(e) = rest[p..].find(';') else {
            out.push_str(&rest[p..]);
            return out;
        };
        let ent = &rest[p + 1..p + e];
        match ent {
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "amp" => out.push('&'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                let v = ent.strip_prefix("#x").map(|h| u32::from_str_radix(h, 16)).or_else(|| ent.strip_prefix('#').map(str::parse));
                match v.and_then(Result::ok).and_then(char::from_u32) {
                    Some(c) => out.push(c),
                    None => out.push_str(&rest[p..p + e + 1]),
                }
            }
        }
        rest = &rest[p + e + 1..];
    }
    out.push_str(rest);
    out
}

/// Every visible glyph of every page, as pymupdf's `rawdict` gives them with its default
/// flags: MuPDF's text device (its trace) gives position, size, font and text, so the
/// positions are MuPDF's own; a character MuPDF cannot map (U+FFFD) is its code in the
/// font, as `use-cid-for-unknown-unicode` makes it, found by the exact interpreter at the
/// same place.
/// The interpreter's glyph codes of a page by position: (x, y, code) per 1pt cell.
type CodeGrid = HashMap<(i64, i64), Vec<(f64, f64, u8)>>;

/// Glyphs per page.
type Pages = Vec<Vec<Glyph>>;

fn doc_glyphs(path: &Path) -> Result<Vec<Vec<Glyph>>, String> {
    let trace = trace_text(path)?;
    let mut codes: Option<(Pdf, HashMap<u32, Option<FontInfo>>)> = None;
    let mut out = Vec::with_capacity(trace.len());
    for (i, page) in trace.into_iter().enumerate() {
        let mut near: Option<CodeGrid> = None;
        let mut v = Vec::with_capacity(page.len());
        for mut g in page {
            if g.c.is_empty() || g.c == "\u{fffd}" {
                let grid = near.get_or_insert_with(|| {
                    if codes.is_none() {
                        codes = Pdf::open(path).ok().map(|p| (p, HashMap::new()));
                    }
                    let mut grid = CodeGrid::new();
                    if let Some((pdf, fonts)) = codes.as_mut().filter(|c| i < c.0.pages.len()) {
                        let mb = page_box(pdf, pdf.pages[i]);
                        for a in page_marks(pdf, i, fonts).1 {
                            let (x, y) = (a.m[4].f64() - mb[0], mb[3] - a.m[5].f64());
                            grid.entry((x as i64, y as i64)).or_default().push((x, y, a.code));
                        }
                    }
                    grid
                });
                let mut best: Option<(f64, u8)> = None;
                for x in g.x as i64 - 1..=g.x as i64 + 1 {
                    for y in g.y as i64 - 1..=g.y as i64 + 1 {
                        for &(ax, ay, code) in grid.get(&(x, y)).map(Vec::as_slice).unwrap_or(&[]) {
                            let d = (ax - g.x).abs() + (ay - g.y).abs();
                            if d < 0.5 && best.is_none_or(|b| d < b.0) {
                                best = Some((d, code));
                            }
                        }
                    }
                }
                g.c = best.map_or_else(|| "\u{fffd}".into(), |b| (b.1 as char).to_string());
            }
            if !g.c.chars().all(py_space) {
                v.push(g);
            }
        }
        out.push(v);
    }
    Ok(out)
}

/// The page's MediaBox (inherited), as [x0, y0, x1, y1].
fn page_box(pdf: &Pdf, id: u32) -> [f64; 4] {
    let mut node = (*pdf.get(id)).clone();
    for _ in 0..32 {
        if let Some(b) = node.get("MediaBox") {
            let v = array_numbers(pdf.resolve(b).arr());
            if v.len() == 4 {
                let f: Vec<f64> = v.iter().map(|d| d.f64()).collect();
                return [f[0].min(f[2]), f[1].min(f[3]), f[0].max(f[2]), f[1].max(f[3])];
            }
        }
        match node.get("Parent") {
            Some(p) => node = pdf.resolve(p),
            None => break,
        }
    }
    [0.0, 0.0, 612.0, 792.0]
}

// ---- documents, references, our PDFs -------------------------------------------------------------

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}
fn corpus() -> PathBuf {
    root().join("scripts/parity/corpus")
}
fn out_dir() -> PathBuf {
    root().join("target/parity")
}
fn rushtex() -> PathBuf {
    root().join("target/release/rushtex")
}
const DEFAULT_DOCS: [&str; 4] = ["sample2e", "small2e", "lppl", "testmath"];
/// Both sides typeset the same date: 2026-10-20 00:00 UTC.
const FIXED_DATE: [(&str, &str); 2] = [("SOURCE_DATE_EPOCH", "1792454400"), ("FORCE_SOURCE_DATE", "1")];

fn corpus_docs() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(corpus())
        .map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "tex")).map(|p| p.file_stem().unwrap().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

fn source(doc: &str) -> Result<PathBuf, String> {
    let p = Path::new(doc);
    if p.extension().is_some_and(|x| x == "tex") && p.exists() {
        return std::fs::canonicalize(p).map_err(|e| e.to_string());
    }
    let p = corpus().join(format!("{doc}.tex"));
    if !p.exists() {
        return Err(format!("no document {doc} (looked in {})", corpus().display()));
    }
    Ok(p)
}

fn stem(p: &Path) -> String {
    p.file_stem().unwrap_or_default().to_string_lossy().into_owned()
}

fn mtime(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

static BUILD: std::sync::Once = std::sync::Once::new();

fn build() -> Result<(), String> {
    let mut err = Ok(());
    BUILD.call_once(|| {
        let r = Command::new("cargo").args(["build", "--release", "--quiet"]).current_dir(root()).output();
        if !r.as_ref().is_ok_and(|o| o.status.success()) {
            err = Err(format!("cargo build failed\n{}", r.map(|o| String::from_utf8_lossy(&o.stderr).into_owned()).unwrap_or_default()));
        }
    });
    err
}

/// pdfLaTeX run until the files it reads back stop changing (at most `passes` times).
fn run_pdflatex(tex: &Path, outdir: &Path, passes: u32, halt: bool) -> Result<PathBuf, String> {
    let _ = std::fs::create_dir_all(outdir);
    let s = stem(tex);
    let pdf = outdir.join(format!("{s}.pdf"));
    let _ = std::fs::remove_file(&pdf);
    let aux: Vec<PathBuf> = [".aux", ".toc", ".lof", ".lot", ".out"].iter().map(|e| outdir.join(format!("{s}{e}"))).collect();
    for f in &aux {
        let _ = std::fs::remove_file(f);
    }
    let state = || aux.iter().map(|f| std::fs::read(f).ok()).collect::<Vec<_>>();
    for _ in 0..passes {
        let before = state();
        let mut c = Command::new("pdflatex");
        c.arg("-interaction=nonstopmode");
        if halt {
            c.arg("-halt-on-error");
        }
        c.arg(format!("-output-directory={}", outdir.display())).arg(tex).current_dir(tex.parent().unwrap()).envs(FIXED_DATE).stdout(Stdio::null()).stderr(Stdio::null());
        let _ = c.status();
        if state() == before {
            break;
        }
    }
    if !pdf.exists() && halt {
        eprintln!("parity-testing: {}: pdflatex stopped on an error, retrying without -halt-on-error", tex.file_name().unwrap().to_string_lossy());
        return run_pdflatex(tex, outdir, passes, false);
    }
    if !pdf.exists() {
        let log = std::fs::read(outdir.join(format!("{s}.log"))).map(|b| latin1(&b)).unwrap_or_default();
        let err: Vec<&str> = log.lines().filter(|l| l.starts_with('!')).take(3).collect();
        return Err(format!("pdflatex failed on {}: {}", tex.display(), err.join(" | ")));
    }
    Ok(pdf)
}

/// When the TeX installation last changed (`ls-R`, `texlive.tlpdb` of each tree).
fn install_mtime() -> Option<std::time::SystemTime> {
    static T: std::sync::OnceLock<Option<std::time::SystemTime>> = std::sync::OnceLock::new();
    *T.get_or_init(|| {
        let mut stamps = Vec::new();
        for var in ["TEXMFHOME", "TEXMFDIST", "TEXMFLOCAL"] {
            let Ok(o) = Command::new("kpsewhich").arg(format!("-var-value={var}")).output() else { continue };
            if !o.status.success() {
                continue;
            }
            for tree in String::from_utf8_lossy(&o.stdout).split(':') {
                let t = tree.trim();
                let t = if let Some(r) = t.strip_prefix("~/") { Path::new(&std::env::var("HOME").unwrap_or_default()).join(r) } else { PathBuf::from(t) };
                for f in [t.join("ls-R"), t.join("tlpkg/texlive.tlpdb"), t.parent().unwrap_or(&t).join("tlpkg/texlive.tlpdb")] {
                    if let Some(m) = mtime(&f) {
                        stamps.push(m);
                    }
                }
            }
        }
        stamps.into_iter().max()
    })
}

fn pinned() -> bool {
    std::env::var("RUSHTEX_PINNED_REFS").as_deref() == Ok("1")
}

/// pdfLaTeX's PDF of `tex`, cached (or the pinned snapshot's, checked against its source).
fn ref_pdf(tex: &Path) -> Result<PathBuf, String> {
    let pdf = out_dir().join("ref").join(format!("{}.pdf", stem(tex)));
    let name = tex.file_name().unwrap().to_string_lossy().into_owned();
    if pinned() {
        static SOURCES: std::sync::OnceLock<Result<HashMap<String, String>, String>> = std::sync::OnceLock::new();
        let sources = SOURCES
            .get_or_init(|| {
                let f = out_dir().join("ref/sources.tsv");
                let t = std::fs::read_to_string(&f).map_err(|_| format!("{} missing: unpack the pinned snapshot", f.display()))?;
                Ok(t.lines().filter_map(|l| l.split_once('\t')).map(|(a, b)| (a.to_string(), b.to_string())).collect())
            })
            .as_ref()
            .map_err(Clone::clone)?;
        let digest = sha256(&std::fs::read(tex).map_err(|e| e.to_string())?);
        if !pdf.exists() || sources.get(&name) != Some(&digest) {
            return Err(format!("{name}: no pinned reference for this source; remake the snapshot (scripts/texlive/pin.py pack)"));
        }
        return Ok(pdf);
    }
    if let (Some(p), Some(t)) = (mtime(&pdf), mtime(tex)) {
        if p >= t && install_mtime().is_none_or(|i| p >= i) {
            return Ok(pdf);
        }
    }
    run_pdflatex(tex, &out_dir().join("ref"), 5, true)
}

/// pdfTeX's errors in the reference log, as `line message`, counted.
fn reference_errors(tex: &Path) -> Result<HashMap<String, u32>, String> {
    ref_pdf(tex)?;
    let log = std::fs::read(out_dir().join("ref").join(format!("{}.log", stem(tex)))).map(|b| latin1(&b)).unwrap_or_default();
    let lines: Vec<&str> = log.lines().collect();
    let mut out = HashMap::new();
    for (i, l) in lines.iter().enumerate() {
        if let Some(msg) = l.strip_prefix("! ") {
            let line = lines[i + 1..(i + 20).min(lines.len())].iter().find_map(|x| x.strip_prefix("l.").and_then(|r| r.split_once(' ')).filter(|(n, _)| n.chars().all(|c| c.is_ascii_digit())).map(|(n, _)| n.to_string())).unwrap_or_else(|| "?".into());
            *out.entry(format!("{line} {}", msg.strip_suffix('.').unwrap_or(msg))).or_insert(0) += 1;
        }
    }
    Ok(out)
}

/// The document through the `rushtex` command as users run it: the native (GDL) job where
/// its pre-scan accepts the document, else the TeX engine on the format in `target/parity`.
/// The engine's errors pass only when they are exactly pdfTeX's.
fn run_rushtex(tex: &Path, pdf: &Path) -> Result<(), String> {
    build()?;
    let _ = std::fs::create_dir_all(pdf.parent().unwrap());
    let _ = std::fs::remove_file(pdf);
    let r = Command::new(rushtex())
        .arg("--fmt")
        .arg(out_dir().join("pdflatex.rfmt"))
        .arg(tex)
        .arg("-o")
        .arg(pdf)
        .current_dir(root())
        .envs(FIXED_DATE)
        .env("RUSHTEX_CACHE", out_dir())
        // The errors in TeX's words, not explained for beginners (`engine_explain`).
        .env("RT_TEX_LOG", "1")
        .output()
        .map_err(|e| e.to_string())?;
    let stderr = String::from_utf8_lossy(&r.stderr);
    // `! l.<n> [<page>] <message>`.
    let errs: Vec<String> = stderr
        .lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("! l.")?;
            let (n, after) = rest.split_once(' ')?;
            (n.chars().all(|c| c.is_ascii_digit()) && after.starts_with('[')).then(|| l[2..].trim().to_string())
        })
        .collect();
    if !pdf.exists() || (!r.status.success() && errs.is_empty()) {
        let tail: String = stderr.chars().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect();
        return Err(format!("rushtex failed on {}: exit {} {tail}", tex.display(), r.status.code().unwrap_or(-1)));
    }
    if !errs.is_empty() {
        let mut ours: HashMap<String, u32> = HashMap::new();
        for e in &errs {
            // `l.<n> [<page>] msg` to `<n> msg`.
            let (n, rest) = e[2..].split_once(' ').unwrap_or(("", ""));
            let msg = rest.split_once("] ").map_or(rest, |x| x.1);
            *ours.entry(format!("{n} {msg}")).or_insert(0) += 1;
        }
        if ours != reference_errors(tex)? {
            return Err(format!("rushtex errors on {}: {}", tex.display(), errs.iter().take(3).cloned().collect::<Vec<_>>().join(" | ")));
        }
    }
    Ok(())
}

fn ours_path(tex: &Path) -> PathBuf {
    out_dir().join("rushtex").join(format!("{}.pdf", stem(tex)))
}

fn ours_pdf(tex: &Path) -> Result<PathBuf, String> {
    let pdf = ours_path(tex);
    run_rushtex(tex, &pdf)?;
    Ok(pdf)
}

// ---- parallel work, results printed in order ----------------------------------------------------

/// `f` over `items` on `jobs` threads; each result is handed to `done` in the items' order as
/// soon as it and all before it are ready.
fn par_ordered<T: Sync, R: Send>(items: &[T], jobs: usize, f: impl Fn(&T) -> R + Sync, mut done: impl FnMut(R) + Send) {
    let next = AtomicUsize::new(0);
    let slots: Mutex<(Vec<Option<R>>, usize)> = Mutex::new(((0..items.len()).map(|_| None).collect(), 0));
    let done = Mutex::new(&mut done);
    std::thread::scope(|s| {
        for _ in 0..jobs.max(1).min(items.len().max(1)) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= items.len() {
                    break;
                }
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&items[i])));
                let r = match r {
                    Ok(r) => r,
                    Err(_) => std::process::abort(),
                };
                let mut g = slots.lock().unwrap();
                g.0[i] = Some(r);
                while g.1 < items.len() {
                    let k = g.1;
                    let Some(r) = g.0[k].take() else { break };
                    g.1 += 1;
                    (done.lock().unwrap())(r);
                }
            });
        }
    });
}

/// Runs `f`, turning a panic (an exact-arithmetic overflow) into an error.
fn guarded<R>(f: impl FnOnce() -> Result<R, String>) -> Result<R, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|e| Err(e.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| e.downcast_ref::<String>().cloned()).unwrap_or_else(|| "panic".into())))
}

fn py_list<T: std::fmt::Display>(v: &[T]) -> String {
    format!("[{}]", v.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "))
}

// ---- zero -----------------------------------------------------------------------------------------

struct Opts {
    docs: Vec<String>,
    all: bool,
    build: bool,
    cached: bool,
    pixels: bool,
    dpi: u32,
    why: Option<usize>,
    jobs: usize,
    flags: HashMap<String, String>,
}

impl Opts {
    fn docs(&self) -> Vec<String> {
        if self.all {
            corpus_docs()
        } else if self.docs.is_empty() {
            DEFAULT_DOCS.iter().map(|s| s.to_string()).collect()
        } else {
            self.docs.clone()
        }
    }
    fn has(&self, k: &str) -> bool {
        self.flags.contains_key(k)
    }
    fn get(&self, k: &str) -> Option<&str> {
        self.flags.get(k).map(String::as_str)
    }
}

fn cmd_zero(o: &Opts) {
    if o.build {
        if let Err(e) = build() {
            die(&e);
        }
    }
    let mut summary = Vec::new();
    zero_each(o, |line, ok| {
        println!("{line}");
        if let Some(ok) = ok {
            summary.push(ok);
        }
    });
    if summary.len() > 1 {
        println!("zero-shift: {}/{} documents", summary.iter().filter(|x| **x).count(), summary.len());
    }
}

/// Each document's `zero` line, in order as soon as it and those before it are done, and
/// whether it is zero (`None` for a `--why` answer).
fn zero_each(o: &Opts, mut each: impl FnMut(&str, Option<bool>) + Send) {
    let docs = o.docs();
    let kind = "rushtex";
    par_ordered(
        &docs,
        o.jobs,
        |d| -> (String, Option<bool>) {
            let r = guarded(|| {
                let tex = source(d)?;
                let s = stem(&tex);
                let refp = ref_pdf(&tex)?;
                let mut ours = ours_path(&tex);
                if !(o.cached && ours.exists()) {
                    ours = ours_pdf(&tex)?;
                }
                let (a, b) = (Pdf::open(&refp)?, Pdf::open(&ours)?);
                if let Some(p) = o.why {
                    let (m, near) = first_difference(&a, &b, p);
                    return Ok((format!("{s} page {p}: first ref mark without an exact twin:\n  ref  {m:?}\n  ours {near:?}"), None));
                }
                let rows = compare_marks(&a, &b);
                let sum = |k: usize| rows.iter().map(|r| r[k]).sum::<u32>();
                let bad: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| r[1] < r[0] || r[3] < r[2] || r[4] > 0 || r[5] > 0).map(|(i, _)| i + 1).collect();
                let marks_ok = a.pages.len() == b.pages.len() && bad.is_empty();
                let mut line = format!("{s}: pages {}/{}  glyphs {}/{}  paths {}/{}  colour-diff {}  extra {}", b.pages.len(), a.pages.len(), sum(1), sum(0), sum(3), sum(2), sum(4), sum(5));
                if !bad.is_empty() {
                    line += &format!("  pages-off {}{}", py_list(&bad[..bad.len().min(12)]), if bad.len() > 12 { "..." } else { "" });
                }
                let st = compare_structure(&a, &b);
                let st_ok = st.iter().all(|x| x.3);
                line += "  | ";
                line += &st.iter().map(|(k, n, m, _)| format!("{k} {m}/{n}")).collect::<Vec<_>>().join(" ");
                let mut px_ok = true;
                if o.pixels {
                    let px = pixel_diffs(&refp, &ours, o.dpi, kind)?;
                    px_ok = px.iter().all(|&v| v == 0);
                    line += &format!("  | pixels@{} {}", o.dpi, px.iter().sum::<u64>());
                    if !px_ok {
                        let pages: Vec<usize> = px.iter().enumerate().filter(|(_, v)| **v > 0).map(|(i, _)| i + 1).take(12).collect();
                        line += &format!(" pages {}", py_list(&pages));
                    }
                }
                let ok = marks_ok && st_ok && px_ok;
                Ok((format!("{}{line}", if ok { "ZERO  " } else { "      " }), Some(ok)))
            });
            match r {
                Ok(x) => x,
                Err(e) => {
                    let s = Path::new(d).file_stem().map_or(d.clone(), |x| x.to_string_lossy().into_owned());
                    (format!("{s}: FAILS parity: {}", e.chars().take(160).collect::<String>()), Some(false))
                }
            }
        },
        |(line, ok)| each(&line, ok),
    );
}

fn die(msg: &str) -> ! {
    eprintln!("parity-testing: {msg}");
    std::process::exit(1)
}

// ---- glyph matching: score, exact, rank, miss ----------------------------------------------------

/// Indices of glyphs of `a` with no partner in `b` within `tol` (each partner used once).
fn match_missing(a: &[Glyph], b: &[Glyph], dx: f64, tol: f64) -> Vec<usize> {
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (k, g) in b.iter().enumerate() {
        grid.entry(((g.x + dx) as i64, g.y as i64)).or_default().push(k);
    }
    let mut used = vec![false; b.len()];
    let mut missing = Vec::new();
    'glyphs: for (i, g) in a.iter().enumerate() {
        let (gx, gy) = (g.x as i64, g.y as i64);
        for x in gx - 1..=gx + 1 {
            for y in gy - 1..=gy + 1 {
                for &k in grid.get(&(x, y)).map(Vec::as_slice).unwrap_or(&[]) {
                    if !used[k] && (b[k].x + dx - g.x).abs() <= tol && (b[k].y - g.y).abs() <= tol {
                        used[k] = true;
                        continue 'glyphs;
                    }
                }
            }
        }
        missing.push(i);
    }
    missing
}

/// Per page of `a`: (glyphs, matched).
/// Per page of the reference: (glyphs, matched); then both documents' page counts.
type PageScores = (Vec<(usize, usize)>, usize, usize);

fn page_scores(refp: &Path, ours: &Path, dx_from: Option<(usize, f64)>, tol: f64) -> Result<PageScores, String> {
    let (a, b) = std::thread::scope(|sc| {
        let h = sc.spawn(|| doc_glyphs(refp));
        let b = doc_glyphs(ours);
        (h.join().unwrap_or_else(|_| Err("extraction panicked".into())), b)
    });
    let (a, b) = (a?, b?);
    let rows = a
        .iter()
        .enumerate()
        .map(|(i, ga)| {
            let gb = b.get(i).map(Vec::as_slice).unwrap_or(&[]);
            let dx = dx_from.filter(|(p, _)| i + 1 >= *p).map_or(0.0, |x| x.1);
            (ga.len(), ga.len() - match_missing(ga, gb, dx, tol).len())
        })
        .collect();
    Ok((rows, a.len(), b.len()))
}

/// Residual buckets of `exact`, in pt.
const BUCKETS: [(f64, &str); 5] = [(0.002, "exact"), (0.01, "<0.01"), (0.1, "<0.1"), (1.0, "<1"), (f64::INFINITY, ">=1")];

/// How a ref glyph was paired: by the same character, by the same font only, or not at all.
enum Pair {
    Same(f64, bool),
    Text(f64),
    None,
}

/// Per ref glyph: its distance to the nearest unused glyph of ours with the same character
/// and whether font and size agree; then a second pass by font only (other extracted text).
fn residuals(a: &[Glyph], b: &[Glyph]) -> Vec<(Pair, usize)> {
    let mut grid: HashMap<(&str, i64, i64), Vec<usize>> = HashMap::new();
    for (k, g) in b.iter().enumerate() {
        grid.entry((g.c.as_str(), g.x as i64, g.y as i64)).or_default().push(k);
    }
    let mut used = vec![false; b.len()];
    let mut out = Vec::with_capacity(a.len());
    for (j, g) in a.iter().enumerate() {
        let mut best: Option<(f64, usize)> = None;
        for x in g.x as i64 - 1..=g.x as i64 + 1 {
            for y in g.y as i64 - 1..=g.y as i64 + 1 {
                for &k in grid.get(&(g.c.as_str(), x, y)).map(Vec::as_slice).unwrap_or(&[]) {
                    if used[k] {
                        continue;
                    }
                    let d = (b[k].x - g.x).abs().max((b[k].y - g.y).abs());
                    if best.is_none_or(|bb| d < bb.0) {
                        best = Some((d, k));
                    }
                }
            }
        }
        match best {
            Some((d, k)) => {
                used[k] = true;
                out.push((Pair::Same(d, b[k].font == g.font && (b[k].size - g.size).abs() < 1e-3), j));
            }
            None => out.push((Pair::None, j)),
        }
    }
    let left: Vec<usize> = (0..b.len()).filter(|&k| !used[k]).collect();
    for item in out.iter_mut() {
        if !matches!(item.0, Pair::None) {
            continue;
        }
        let g = &a[item.1];
        let mut best: Option<(f64, usize)> = None;
        for &k in &left {
            if used[k] || b[k].font != g.font {
                continue;
            }
            let d = (b[k].x - g.x).abs().max((b[k].y - g.y).abs());
            if d < 1.0 && best.is_none_or(|bb| d < bb.0) {
                best = Some((d, k));
            }
        }
        if let Some((d, k)) = best {
            used[k] = true;
            item.0 = Pair::Text(d);
        }
    }
    out
}

fn cmd_exact(o: &Opts) {
    if o.build {
        if let Err(e) = build() {
            die(&e);
        }
    }
    let docs = o.docs();
    par_ordered(
        &docs,
        o.jobs,
        |d| {
            let r = guarded(|| {
                let tex = source(d)?;
                let (a, b) = (doc_glyphs(&ref_pdf(&tex)?)?, doc_glyphs(&ours_pdf(&tex)?)?);
                let mut counts = [0usize; BUCKETS.len() + 1];
                let (mut font_bad, mut tot, mut text_only) = (0, 0, 0);
                let mut worst: (f64, usize, Option<Glyph>) = (0.0, 0, None);
                for (i, ga) in a.iter().enumerate() {
                    let gb = b.get(i).map(Vec::as_slice).unwrap_or(&[]);
                    for (p, j) in residuals(ga, gb) {
                        tot += 1;
                        let (r, same) = match p {
                            Pair::None => {
                                counts[BUCKETS.len()] += 1;
                                continue;
                            }
                            Pair::Text(r) => {
                                counts[BUCKETS.iter().position(|(t, _)| r < *t).unwrap()] += 1;
                                text_only += 1;
                                continue;
                            }
                            Pair::Same(r, s) => (r, s),
                        };
                        counts[BUCKETS.iter().position(|(t, _)| r < *t).unwrap()] += 1;
                        font_bad += usize::from(!same);
                        if r < 1.0 && r > worst.0 {
                            worst = (r, i + 1, Some(ga[j].clone()));
                        }
                    }
                }
                let ex = counts[0];
                let line = BUCKETS.iter().zip(counts).map(|((_, n), c)| format!("{n}:{c}")).collect::<Vec<_>>().join(" ");
                let mut s = format!("{}: exact {:.2}% ({ex}/{tot})  {line} none:{} other-text:{text_only}  font/size-mismatch:{font_bad}", stem(&tex), 100.0 * ex as f64 / tot.max(1) as f64, counts[BUCKETS.len()]);
                if let (Some(g), true) = (&worst.2, o.has("v")) {
                    s += &format!("\n  worst <1pt: {:.4}pt page {} {:?} at ({:.3},{:.3})", worst.0, worst.1, g.c, g.x, g.y);
                }
                Ok(s)
            });
            r.unwrap_or_else(|e| format!("{d}: FAILS parity: {}", e.chars().take(160).collect::<String>()))
        },
        |line| println!("{line}"),
    );
}

// ---- JSON, for the score files ---------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(BTreeMap<String, Json>),
}

fn json_parse(s: &str) -> Option<Json> {
    fn val(b: &[u8], i: &mut usize) -> Option<Json> {
        while *i < b.len() && b[*i].is_ascii_whitespace() {
            *i += 1;
        }
        match *b.get(*i)? {
            b'{' => {
                *i += 1;
                let mut m = BTreeMap::new();
                loop {
                    while b[*i].is_ascii_whitespace() || b[*i] == b',' {
                        *i += 1;
                    }
                    if b[*i] == b'}' {
                        *i += 1;
                        return Some(Json::Obj(m));
                    }
                    let Json::Str(k) = val(b, i)? else { return None };
                    while b[*i].is_ascii_whitespace() || b[*i] == b':' {
                        *i += 1;
                    }
                    m.insert(k, val(b, i)?);
                }
            }
            b'[' => {
                *i += 1;
                let mut v = Vec::new();
                loop {
                    while b[*i].is_ascii_whitespace() || b[*i] == b',' {
                        *i += 1;
                    }
                    if b[*i] == b']' {
                        *i += 1;
                        return Some(Json::Arr(v));
                    }
                    v.push(val(b, i)?);
                }
            }
            b'"' => {
                *i += 1;
                let mut out = String::new();
                while b[*i] != b'"' {
                    if b[*i] == b'\\' {
                        *i += 1;
                        match b[*i] {
                            b'n' => out.push('\n'),
                            b't' => out.push('\t'),
                            b'r' => out.push('\r'),
                            b'u' => {
                                let h = std::str::from_utf8(&b[*i + 1..*i + 5]).ok()?;
                                out.push(char::from_u32(u32::from_str_radix(h, 16).ok()?).unwrap_or('\u{fffd}'));
                                *i += 4;
                            }
                            c => out.push(c as char),
                        }
                        *i += 1;
                    } else {
                        let st = *i;
                        while b[*i] != b'"' && b[*i] != b'\\' {
                            *i += 1;
                        }
                        out.push_str(std::str::from_utf8(&b[st..*i]).ok()?);
                    }
                }
                *i += 1;
                Some(Json::Str(out))
            }
            b't' => {
                *i += 4;
                Some(Json::Bool(true))
            }
            b'f' => {
                *i += 5;
                Some(Json::Bool(false))
            }
            b'n' => {
                *i += 4;
                Some(Json::Null)
            }
            _ => {
                let st = *i;
                while *i < b.len() && (b[*i].is_ascii_digit() || b"+-.eE".contains(&b[*i])) {
                    *i += 1;
                }
                std::str::from_utf8(&b[st..*i]).ok()?.parse().ok().map(Json::Num)
            }
        }
    }
    val(s.as_bytes(), &mut 0)
}

/// A float as Python's `repr` writes it (shortest round trip, `.0` on integers).
fn py_float(x: f64) -> String {
    let s = format!("{x}");
    if x.is_finite() && !s.contains('.') && !s.contains('e') {
        s + ".0"
    } else {
        s
    }
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    let _ = write!(o, "\\u{u:04x}");
                }
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// A score entry: `{"pct": float, "pages": [int], "fail"?: str}`.
#[derive(Clone)]
struct ScoreEntry {
    pct: f64,
    pages: Vec<i64>,
    fail: Option<String>,
}

fn read_scores(p: &Path) -> BTreeMap<String, ScoreEntry> {
    let Some(Json::Obj(m)) = std::fs::read_to_string(p).ok().and_then(|t| json_parse(&t)) else { return BTreeMap::new() };
    m.into_iter()
        .filter_map(|(k, v)| {
            let Json::Obj(e) = v else { return None };
            let pct = match e.get("pct") {
                Some(Json::Num(x)) => *x,
                _ => 0.0,
            };
            let pages = match e.get("pages") {
                Some(Json::Arr(a)) => a.iter().filter_map(|x| if let Json::Num(n) = x { Some(*n as i64) } else { None }).collect(),
                _ => Vec::new(),
            };
            let fail = match e.get("fail") {
                Some(Json::Str(s)) => Some(s.clone()),
                _ => None,
            };
            Some((k, ScoreEntry { pct, pages, fail }))
        })
        .collect()
}

fn write_scores(p: &Path, m: &BTreeMap<String, ScoreEntry>, indent0: bool) {
    // Python's `json.dumps` keeps insertion order unless `sort_keys`; the files are read by
    // key, so sorted order is equivalent.
    let mut out = String::new();
    let entries: Vec<String> = m
        .iter()
        .map(|(k, e)| {
            let pages = if e.pages.is_empty() { "[]".to_string() } else if indent0 { format!("[\n{}\n]", e.pages.iter().map(i64::to_string).collect::<Vec<_>>().join(",\n")) } else { format!("[{}]", e.pages.iter().map(i64::to_string).collect::<Vec<_>>().join(", ")) };
            let mut fields = Vec::new();
            if let Some(f) = &e.fail {
                fields.push(format!("\"fail\": {}", json_str(f)));
            }
            fields.push(format!("\"pages\": {pages}"));
            fields.push(format!("\"pct\": {}", py_float(e.pct)));
            if !indent0 {
                // Insertion order of `{"pct", "pages", "fail"}`.
                let mut f = vec![format!("\"pct\": {}", py_float(e.pct)), format!("\"pages\": {pages}")];
                if let Some(x) = &e.fail {
                    f.push(format!("\"fail\": {}", json_str(x)));
                }
                fields = f;
            }
            let sep = if indent0 { ",\n" } else { ", " };
            let nl = if indent0 { "\n" } else { "" };
            format!("{}: {{{nl}{}{nl}}}", json_str(k), fields.join(sep))
        })
        .collect();
    if indent0 {
        out += &format!("{{\n{}\n}}\n", entries.join(",\n"));
    } else {
        out += &format!("{{{}}}", entries.join(", "));
    }
    let _ = std::fs::write(p, out);
}

fn cmd_score(o: &Opts) {
    let tol = o.get("tol").and_then(|t| t.parse().ok());
    let save_baseline = o.has("save-baseline") && tol.is_none();
    if o.build {
        if let Err(e) = build() {
            die(&e);
        }
    }
    let dx = o.get("dx").and_then(|s| s.split_once(':')).and_then(|(p, d)| Some((p.parse::<usize>().ok()?, d.parse::<f64>().ok()?)));
    let docs = o.docs();
    let last_path = out_dir().join("last_score.json");
    let baseline_path = root().join("scripts/parity/baseline.json");
    let last = if dx.is_none() { read_scores(&last_path) } else { BTreeMap::new() };
    let base = read_scores(&baseline_path);
    let now = Mutex::new(BTreeMap::new());
    par_ordered(
        &docs,
        o.jobs,
        |d| {
            let r = guarded(|| {
                let tex = source(d)?;
                let (rows, na, nb) = page_scores(&ref_pdf(&tex)?, &ours_pdf(&tex)?, dx, tol.unwrap_or(0.1))?;
                Ok((stem(&tex), rows, na, nb))
            });
            (d.clone(), r)
        },
        |(d, r)| {
            let mut now = now.lock().unwrap();
            match r {
                Err(e) => {
                    let stem = Path::new(&d).file_stem().map_or(d.clone(), |x| x.to_string_lossy().into_owned());
                    println!("{stem}: FAILS parity: {}", e.chars().take(160).collect::<String>());
                    if base.get(&stem).is_some_and(|b| b.pct > 0.0) {
                        println!("  REGRESSED vs baseline (compiled before)");
                    }
                    now.insert(stem, ScoreEntry { pct: 0.0, pages: Vec::new(), fail: Some(format!("parity: {e}").chars().take(200).collect()) });
                }
                Ok((s, rows, na, nb)) => {
                    let tot: usize = rows.iter().map(|r| r.0).sum();
                    let ok: usize = rows.iter().map(|r| r.1).sum();
                    let pct = 100.0 * ok as f64 / tot.max(1) as f64;
                    let pages: Vec<i64> = rows.iter().map(|(t, o)| (100 * o / (*t).max(1)) as i64).collect();
                    let prev = last.get(&s);
                    let delta = prev.map_or(String::new(), |p| format!(" ({:+.1})", pct - p.pct));
                    println!("{s}: pages {nb}/{na}, {ok}/{tot} = {pct:.1}%{delta}");
                    if pages.iter().any(|&p| p < 100) {
                        println!("  {}", pages.iter().enumerate().map(|(i, p)| format!("{}:{p}", i + 1)).collect::<Vec<_>>().join(" "));
                    }
                    if let Some(r) = base.get(&s).or(prev) {
                        let worse: Vec<String> = pages.iter().zip(&r.pages).enumerate().filter(|(_, (p, q))| p < q).map(|(i, (p, q))| format!("{}:{q}->{p}", i + 1)).collect();
                        if !worse.is_empty() {
                            println!("{}{}", if base.contains_key(&s) { "  REGRESSED vs baseline " } else { "  REGRESSED " }, worse.join(" "));
                        }
                    }
                    now.insert(s, ScoreEntry { pct, pages, fail: None });
                }
            }
        },
    );
    let now = now.into_inner().unwrap();
    if dx.is_none() {
        let _ = std::fs::create_dir_all(out_dir());
        let mut merged = last;
        merged.extend(now.clone());
        write_scores(&last_path, &merged, false);
        if save_baseline {
            let mut b = base;
            b.extend(now);
            write_scores(&baseline_path, &b, true);
            println!("baseline saved: scripts/parity/baseline.json");
        }
    }
}

fn cmd_rank(o: &Opts) {
    let doc = o.docs.first().cloned().unwrap_or_else(|| die("rank needs DOC"));
    let n: usize = o.get("n").and_then(|x| x.parse().ok()).unwrap_or(15);
    let r = guarded(|| {
        let tex = source(&doc)?;
        let (refp, ours) = (ref_pdf(&tex)?, ours_pdf(&tex)?);
        let (rows, _, _) = page_scores(&refp, &ours, None, 0.1)?;
        let (a, b) = (doc_glyphs(&refp)?, doc_glyphs(&ours)?);
        let mut order: Vec<usize> = (0..rows.len()).collect();
        order.sort_by_key(|&i| rows[i].1 as i64 - rows[i].0 as i64);
        for &i in order.iter().take(n) {
            let (t, ok) = rows[i];
            if t == ok {
                break;
            }
            let mut first = String::new();
            if i < b.len() {
                for (x, y) in baselines(&a[i], 6.0).iter().zip(baselines(&b[i], 6.0).iter()) {
                    if (y.0 - x.0).abs() > 0.1 || (y.1 - x.1).abs() > 0.1 {
                        first = format!("first drift y{:.1} dy{:+.2} dx{:+.2} {}", x.0, y.0 - x.0, y.1 - x.1, x.2.chars().take(30).collect::<String>());
                        break;
                    }
                }
            }
            println!("page {:3}: lost {:5} of {t:5}  {first}", i + 1, t - ok);
        }
        Ok(())
    });
    if let Err(e) = r {
        die(&e);
    }
}

fn cmd_miss(o: &Opts) {
    let doc = o.docs.first().cloned().unwrap_or_else(|| die("miss needs DOC PAGE"));
    let page: usize = o.docs.get(1).and_then(|p| p.parse().ok()).unwrap_or_else(|| die("miss needs DOC PAGE"));
    let dx: f64 = o.get("dx").and_then(|x| x.parse().ok()).unwrap_or(0.0);
    let r = guarded(|| {
        let tex = source(&doc)?;
        let (a, b) = (doc_glyphs(&ref_pdf(&tex)?)?, doc_glyphs(&ours_pdf(&tex)?)?);
        let (ga, gb) = (a.get(page - 1).cloned().unwrap_or_default(), b.get(page - 1).cloned().unwrap_or_default());
        let mut miss: Vec<&Glyph> = match_missing(&ga, &gb, dx, 0.1).into_iter().map(|i| &ga[i]).collect();
        let r1 = |y: f64| py_round(y, 1);
        // `sorted(a[i][:3] ...)` by (x, y, char), then a stable sort by line.
        miss.sort_by(|p, q| (p.x, p.y).partial_cmp(&(q.x, q.y)).unwrap().then_with(|| p.c.cmp(&q.c)));
        miss.sort_by(|p, q| (r1(p.y), p.x).partial_cmp(&(r1(q.y), q.x)).unwrap());
        let mut i = 0;
        while i < miss.len() {
            let y = r1(miss[i].y);
            let j = (i..miss.len()).find(|&j| r1(miss[j].y) != y).unwrap_or(miss.len());
            let text: String = miss[i..j].iter().map(|g| g.c.as_str()).collect::<String>().chars().take(60).collect();
            println!("{y:8.1} x{:7.2} n{:3}  {text}", miss[i].x, j - i);
            i = j;
        }
        Ok(())
    });
    if let Err(e) = r {
        die(&e);
    }
}

/// Lines of a page: (baseline rounded to 0.01pt, leftmost x, text sorted by x), glyphs of at
/// least `min_size` and not `|` or form feed.
fn baselines(page: &[Glyph], min_size: f64) -> Vec<(f64, f64, String)> {
    let mut m: BTreeMap<i64, Vec<(f64, &str)>> = BTreeMap::new();
    for g in page {
        if g.size >= min_size && g.c != "|" && g.c != "\x0c" {
            m.entry((py_round(g.y, 2) * 100.0).round() as i64).or_default().push((g.x, &g.c));
        }
    }
    m.into_iter()
        .map(|(y, mut v)| {
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            (y as f64 / 100.0, v[0].0, v.iter().map(|x| x.1).collect())
        })
        .collect()
}

// ---- command line ----------------------------------------------------------------------------------

const USAGE: &str = "usage: parity-testing COMMAND [DOC..] [--all] [--build] [--cached] [--pixels] [--dpi N] [--why N] [-j N]
commands: zero [--why PAGE], score [--save-baseline] [--tol T] [--dx PAGE:DX], exact [-v],
  rank DOC [-n N], lines DOC PAGE [--all] [--cached], miss DOC PAGE [--dx DX], boxes DOC PAGE [--all],
  tj DOC PAGE [--width N], repro DOC [A B] [--body TEXT] [--name N] [--trace pages|lists] [--all],
  glyphs PDF, bench (bench -h), incr DOC.. [--edits N] [--seed S] [-v]";

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| die(USAGE));
    let mut o = Opts { docs: Vec::new(), all: false, build: false, cached: false, pixels: false, dpi: 300, why: None, jobs: std::thread::available_parallelism().map_or(4, |n| n.get()), flags: HashMap::new() };
    let rest: Vec<String> = args.collect();
    let mut i = 0;
    while i < rest.len() {
        let a = rest[i].as_str();
        let mut value = || {
            i += 1;
            rest.get(i).cloned().unwrap_or_else(|| die(&format!("{a} needs a value")))
        };
        match a {
            "--all" => o.all = true,
            "--build" => o.build = true,
            "--cached" => o.cached = true,
            "--pixels" => o.pixels = true,
            "--dpi" => o.dpi = value().parse().unwrap_or_else(|_| die("--dpi N")),
            "--why" => o.why = Some(value().parse().unwrap_or_else(|_| die("--why N"))),
            "-j" | "--jobs" => o.jobs = value().parse::<usize>().unwrap_or_else(|_| die("-j N")).max(1),
            "--tol" | "--dx" | "--n" | "-n" | "--width" | "--body" | "--name" | "--trace" | "--timeout" | "--edits" | "--seed" => {
                let v = value();
                o.flags.insert(a.trim_start_matches('-').into(), v);
            }
            "-h" | "--help" => {
                println!("{}", if cmd == "bench" { BENCH_USAGE } else { USAGE });
                return;
            }
            f if f.starts_with('-') => {
                o.flags.insert(f.trim_start_matches('-').into(), String::new());
            }
            d => o.docs.push(d.into()),
        }
        i += 1;
    }
    // Documents run in parallel; a single document splits its own work instead.
    let many = matches!(cmd.as_str(), "zero" | "score" | "exact") && o.docs().len() > 1;
    JOBS.store(if many { 1 } else { o.jobs }, Ordering::Relaxed);
    match cmd.as_str() {
        "zero" => cmd_zero(&o),
        "score" => cmd_score(&o),
        "exact" => cmd_exact(&o),
        "rank" => cmd_rank(&o),
        "miss" => cmd_miss(&o),
        "glyphs" => cmd_glyphs(&o),
        "lines" => cmd_lines(&o),
        "boxes" => cmd_boxes(&o),
        "repro" => cmd_repro(&o),
        "tj" => cmd_tj(&o),
        "bench" => cmd_bench(&o),
        "incr" => cmd_incr(&o),
        _ => die(USAGE),
    }
}

/// `glyphs PDF`: every extracted glyph, `page x y size font char`, to check the extraction
/// against pymupdf's `rawdict`.
fn cmd_glyphs(o: &Opts) {
    let pdf = o.docs.first().unwrap_or_else(|| die("glyphs needs PDF"));
    let pages = doc_glyphs(Path::new(pdf)).unwrap_or_else(|e| die(&e));
    for (i, p) in pages.iter().enumerate() {
        for g in p {
            println!("{} {:?} {:?} {:?} {}", i + 1, g.x, g.y, g.size, g.c.chars().map(|c| format!("{:x}", c as u32)).collect::<Vec<_>>().join(","));
        }
    }
}

fn timeout_of(o: &Opts) -> std::time::Duration {
    std::time::Duration::from_secs(o.flags.get("timeout").map_or(300, |t| t.parse().unwrap_or_else(|_| die("--timeout S"))))
}

/// Instructions retired by rushtex and by pdfLaTeX on each of `docs`, a table written to
/// `out` in `docs`' order as rows complete: one rushtex binary, or two (before and after a
/// change: both measured on each document, pdfLaTeX once). Each document's folder is copied
/// to a scratch directory so neither side sees the other's files. rushtex: one warm-up run,
/// then one counted run, as a user runs it; whether the native job took it. pdfLaTeX: from
/// clean, rerun until the PDF stops changing; the runs up to the first that made the final
/// PDF, and the first run alone (the one-run floor). No BibTeX. Counts do not depend on how
/// many documents run at once, so `jobs` only changes the wall time.
fn instructions_table(docs: &[String], out: &Path, rushes: &[PathBuf], jobs: usize, timeout: std::time::Duration) {
    let scratch = std::env::temp_dir().join(format!("rushtex_bench_instr_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap_or_else(|e| die(&format!("{}: {e}", scratch.display())));
    let mut f = std::io::BufWriter::new(std::fs::File::create(out).unwrap_or_else(|e| die(&format!("{}: {e}", out.display()))));
    use std::io::Write as _;
    let before = if rushes.len() > 1 { "rushtex_before_instructions\t" } else { "" };
    let _ = writeln!(f, "doc\trushtex_mode\trushtex_pages\tpdflatex_pages\t{before}rushtex_instructions\tpdflatex_one_run\tpdflatex_cycle\tpdflatex_runs");
    let columns = 7 + rushes.len();
    let next = AtomicUsize::new(0);
    let (tx, rx) = std::sync::mpsc::channel::<(usize, Vec<String>)>();
    std::thread::scope(|sc| {
        for _ in 0..jobs.min(docs.len()) {
            let tx = tx.clone();
            let (next, scratch) = (&next, &scratch);
            sc.spawn(move || loop {
                let k = next.fetch_add(1, Ordering::Relaxed);
                let Some(d) = docs.get(k) else { break };
                let row = bench_doc(Path::new(d), rushes, scratch, timeout).unwrap_or_else(|e| {
                    let name = Path::new(d).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
                    let mut v = vec![name, format!("error: {e}").chars().take(80).collect()];
                    v.resize(columns, String::new());
                    v
                });
                let _ = tx.send((k, row));
            });
        }
        drop(tx);
        // In LIST's order: a finished row waits for the ones before it.
        let mut done: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        let mut at = 0;
        for (k, row) in rx {
            done.insert(k, row);
            while let Some(row) = done.remove(&at) {
                let _ = writeln!(f, "{}", row.join("\t"));
                let _ = f.flush();
                at += 1;
            }
        }
    });
    let _ = std::fs::remove_dir_all(&scratch);
}

/// One document's row: name, rushtex mode and pages (the last binary's), pdfLaTeX pages,
/// each binary's instructions, pdfLaTeX's first run and its runs up to the final PDF, and how
/// many those were.
fn bench_doc(tex: &Path, rushes: &[PathBuf], scratch: &Path, timeout: std::time::Duration) -> Result<Vec<String>, String> {
    let tex = std::path::absolute(tex).map_err(|e| e.to_string())?;
    let name = tex.file_stem().ok_or("no file name")?.to_string_lossy().into_owned();
    let dir = tex.parent().ok_or("no folder")?;
    // Templates are all `main.tex`: the folder names the document and its scratch copy.
    let slug = if name == "main" { dir.file_name().map_or(name.clone(), |d| d.to_string_lossy().into_owned()) } else { name.clone() };
    let work = scratch.join(&slug);
    let _ = std::fs::remove_dir_all(&work);
    copy_tree(dir, &work).map_err(|e| format!("copy {}: {e}", dir.display()))?;
    let src = work.join(tex.file_name().ok_or("no file name")?);
    // rushtex.
    let out = work.join("rushtex-out.pdf");
    let log = work.join("rushtex-why.log");
    let (mut mode, mut ins) = ("engine", Vec::new());
    for rush in rushes {
        let mut warm = Command::new(rush);
        warm.arg(&src).arg("-o").arg(&out).current_dir(&work).envs(FIXED_DATE).env("RT_NATIVE_WHY", "1");
        run_until(&mut warm, &log, timeout);
        mode = if std::fs::read(&log).is_ok_and(|b| b.windows(19).any(|w| w == b"native job accepted")) { "native" } else { "engine" };
        ins.push(measure(&[rush.as_os_str(), src.as_os_str(), "-o".as_ref(), out.as_os_str()], &work, timeout).0);
    }
    let r_pages = pdf_pages(&out);
    // pdfLaTeX, from clean until the PDF is final.
    let (counts, n) = pdflatex_cycle(&work, &src, timeout);
    let p_pages = pdf_pages(&work.join(format!("{name}.pdf")));
    let _ = std::fs::remove_dir_all(&work);
    let mut row = vec![slug, mode.into(), r_pages.to_string(), p_pages.to_string()];
    row.extend(ins.iter().map(u64::to_string));
    row.extend([counts[0].to_string(), counts[..n].iter().sum::<u64>().to_string(), n.to_string()]);
    Ok(row)
}

/// pdfLaTeX on `src` in `work` from clean, rerun until the PDF stops changing (at most 6
/// runs): each run's instructions, and how many runs made the final PDF (the confirming run
/// not counted).
fn pdflatex_cycle(work: &Path, src: &Path, timeout: std::time::Duration) -> (Vec<u64>, usize) {
    let name = stem(src);
    for e in ["aux", "toc", "lof", "lot", "out", "log", "pdf"] {
        let _ = std::fs::remove_file(work.join(format!("{name}.{e}")));
    }
    let pdf = work.join(format!("{name}.pdf"));
    let (mut counts, mut hashes) = (Vec::new(), Vec::new());
    while counts.len() < 6 {
        counts.push(measure(&["pdflatex".as_ref(), "-interaction=batchmode".as_ref(), "-no-shell-escape".as_ref(), src.as_os_str()], work, timeout).0);
        hashes.push(std::fs::read(&pdf).map_or(String::new(), |b| sha256(&b)));
        if hashes.len() > 1 && hashes[hashes.len() - 1] == hashes[hashes.len() - 2] {
            break;
        }
    }
    let last = hashes.last().cloned().unwrap_or_default();
    let n = hashes.iter().position(|h| *h == last).map_or(1, |i| i + 1);
    (counts, n)
}

/// The pages of a PDF, by this framework's reader; 0 when it does not open.
fn pdf_pages(pdf: &Path) -> usize {
    Pdf::open(pdf).map_or(0, |p| p.pages.len())
}

/// `src` copied to `dst` with its subfolders, symbolic links kept as links.
fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let (from, to) = (e.path(), dst.join(e.file_name()));
        let t = e.file_type()?;
        if t.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(std::fs::read_link(&from)?, &to)?;
        } else if t.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Runs `c` with stdout and stderr into `log`, killed after `timeout`; whether it exited 0.
fn run_until(c: &mut Command, log: &Path, timeout: std::time::Duration) -> bool {
    let Ok(file) = std::fs::File::create(log) else { return false };
    let Ok(err) = file.try_clone() else { return false };
    let Ok(mut child) = c.stdin(Stdio::null()).stdout(file).stderr(err).spawn() else { return false };
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(std::time::Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// (instructions retired, peak RSS in bytes, ok) of one run of `cmd` in `cwd` with the fixed
/// date, under `/usr/bin/time -l` on macOS or `perf stat` and `/usr/bin/time -f` on Linux;
/// 0 instructions on a timeout or when none are reported.
fn measure(cmd: &[&std::ffi::OsStr], cwd: &Path, timeout: std::time::Duration) -> (u64, u64, bool) {
    let mac = cfg!(target_os = "macos");
    let mut c = if mac {
        let mut c = Command::new("/usr/bin/time");
        c.arg("-l");
        c
    } else {
        let mut c = Command::new("perf");
        c.args(["stat", "-x,", "-e", "instructions:u", "/usr/bin/time", "-f", "RSS %M"]);
        c
    };
    c.args(cmd);
    // The child's own output goes to the log too; only the counters' lines are read.
    let log = cwd.join(format!(".measure-{:?}.log", std::thread::current().id()).replace(['(', ')'], ""));
    let ok = run_until(c.current_dir(cwd).envs(FIXED_DATE), &log, timeout);
    let text = std::fs::read(&log).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let _ = std::fs::remove_file(&log);
    let (mut ins, mut rss) = (0, 0);
    for l in text.lines().map(str::trim) {
        if mac {
            if let Some(n) = l.strip_suffix("instructions retired") {
                ins = n.trim().parse().unwrap_or(0);
            } else if let Some(n) = l.strip_suffix("maximum resident set size") {
                rss = n.trim().parse().unwrap_or(0);
            }
        } else if l.split(',').nth(2).is_some_and(|k| k.starts_with("instructions")) {
            ins = l.split(',').next().and_then(|n| n.parse().ok()).unwrap_or(0);
        } else if let Some(n) = l.strip_prefix("RSS ") {
            rss = n.trim().parse::<u64>().unwrap_or(0) * 1024;
        }
    }
    (ins, rss, ok)
}

// ---- bench: the benchmark (`cargo benchmark` runs it) ----------------------------------------------

const BENCH_USAGE: &str = "usage: cargo benchmark [FILE.tex [N] | docs [N] | templates | compare OLD_BIN | all [N] | check [BASE]] [-j N]
  (from the repository root; the alias runs `parity-testing bench` in testing/, built when needed)
  (no arguments)    at a terminal, a menu of these
  FILE.tex [N]      one document against pdfLaTeX, full report (N runs a median, default 3)
  docs [N]          docs/master.tex and every scripts/bench/*.tex
  templates         the template-corpus documents the native job accepts (target/bench/templates.tsv)
  compare OLD_BIN   the same templates, OLD_BIN against the current binary (target/bench/compare.tsv)
  all [N]           docs, then templates
  check [BASE]      for contributors: is the output still identical? (BASE: a git ref, e.g. origin/master)
  env: TIME_RUNS (timed runs, default 11), RUSHTEX_BIN (the rushtex binary)";

/// `bench …`: the benchmark of rushtex against pdfLaTeX (instructions retired, CPU and wall
/// time, peak RAM), and the contributor's check.
fn cmd_bench(o: &Opts) {
    // Absolute: the documents run in their copies' folders, where a relative path finds nothing.
    let rush = std::env::var_os("RUSHTEX_BIN").map_or_else(rushtex, PathBuf::from);
    let rush = std::fs::canonicalize(&rush).unwrap_or(rush);
    // No arguments at a terminal: the menu, which answers with the arguments it stands for.
    use std::io::IsTerminal as _;
    // Without a terminal (CI, scripts) a mode or a file must be named.
    let args = match (o.docs.is_empty(), std::io::stdin().is_terminal()) {
        (true, true) => bench_menu(),
        (true, false) => die(BENCH_USAGE),
        _ => o.docs.clone(),
    };
    let arg = |k: usize| args.get(k).map(String::as_str);
    let runs = |k: usize| arg(k).map_or(3, |n| n.parse::<usize>().unwrap_or_else(|_| die(BENCH_USAGE)).max(1));
    if !on_path("pdflatex") {
        die("pdflatex not found on PATH (install TeX Live)");
    }
    if arg(0) == Some("check") {
        return bench_check(o, &rush, arg(1));
    }
    if !rush.exists() && Command::new("cargo").args(["build", "--release"]).current_dir(root()).status().map_or(true, |s| !s.success()) {
        die("cargo build --release failed");
    }
    match arg(0) {
        Some("docs") => bench_docs(&rush, runs(1)),
        Some("templates") => bench_templates(o, &rush),
        Some("all") => {
            bench_docs(&rush, runs(1));
            bench_templates(o, &rush);
        }
        Some("compare") => {
            let old = PathBuf::from(arg(1).unwrap_or_else(|| die("bench compare OLD_BIN")));
            let docs = native_templates(&rush, o.jobs);
            let table = root().join("target/bench/compare.tsv");
            instructions_table(&docs, &table, &[old.clone(), rush.clone()], o.jobs, timeout_of(o));
            println!("Before ({}) and after ({}), target/bench/compare.tsv:", old.display(), rush.display());
            summarise(&table);
        }
        doc => {
            let tex = Path::new(doc.unwrap_or_default());
            if !tex.is_file() {
                die(BENCH_USAGE);
            }
            bench_report(tex, runs(1), &rush).unwrap_or_else(|e| die(&e));
        }
    }
}

fn bench_docs(rush: &Path, runs: usize) {
    let mut docs = vec![root().join("docs/master.tex")];
    let mut bench: Vec<PathBuf> = std::fs::read_dir(root().join("scripts/bench")).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "tex")).collect()).unwrap_or_default();
    bench.sort();
    docs.extend(bench);
    for d in docs {
        bench_report(&d, runs, rush).unwrap_or_else(|e| eprintln!("{}: {e}", d.display()));
    }
}

fn bench_templates(o: &Opts, rush: &Path) {
    let docs = native_templates(rush, o.jobs);
    let table = root().join("target/bench/templates.tsv");
    instructions_table(&docs, &table, &[rush.to_path_buf()], o.jobs, timeout_of(o));
    println!("Templates (target/bench/templates.tsv):");
    summarise(&table);
}

/// The template-corpus documents (`template-corpus/*/main.tex`) the native job accepts with
/// `rush`, listed in target/bench/native_templates.tsv. The arXiv corpus is not benchmarked.
fn native_templates(rush: &Path, jobs: usize) -> Vec<String> {
    let mut all: Vec<PathBuf> = std::fs::read_dir(root().join("template-corpus")).map(|d| d.flatten().map(|e| e.path().join("main.tex")).filter(|p| p.is_file()).collect()).unwrap_or_default();
    all.sort();
    let next = AtomicUsize::new(0);
    let accepted: Vec<Mutex<bool>> = all.iter().map(|_| Mutex::new(false)).collect();
    let logs = std::env::temp_dir().join(format!("rushtex_bench_native_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&logs);
    std::thread::scope(|sc| {
        for w in 0..jobs.max(1) {
            let (all, next, accepted, logs) = (&all, &next, &accepted, &logs);
            sc.spawn(move || loop {
                let k = next.fetch_add(1, Ordering::Relaxed);
                let Some(tex) = all.get(k) else { break };
                let log = logs.join(format!("{w}.log"));
                let mut c = Command::new(rush);
                c.arg("main.tex").args(["-o", "/dev/null"]).current_dir(tex.parent().unwrap()).env("RT_NATIVE_WHY", "1");
                run_until(&mut c, &log, std::time::Duration::from_secs(60));
                *accepted[k].lock().unwrap() = std::fs::read(&log).is_ok_and(|b| b.windows(19).any(|w| w == b"native job accepted"));
            });
        }
    });
    let _ = std::fs::remove_dir_all(&logs);
    let docs: Vec<String> = all.iter().zip(&accepted).filter(|(_, a)| *a.lock().unwrap()).map(|(p, _)| p.display().to_string()).collect();
    let dir = root().join("target/bench");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("native_templates.tsv"), docs.iter().map(|d| format!("{d}\n")).collect::<String>());
    println!("{} templates the native job accepts", docs.len());
    docs
}

/// Totals of an `instructions` table; for a before/after table also the mean saving.
fn summarise(table: &Path) {
    let text = std::fs::read_to_string(table).unwrap_or_default();
    let mut lines = text.lines();
    let two = lines.next().is_some_and(|h| h.contains("rushtex_before"));
    let k = usize::from(two);
    let (mut n, mut pages, mut before, mut r, mut p1, mut pc, mut runs, mut higher) = (0u64, 0u64, 0f64, 0f64, 0f64, 0f64, 0f64, 0);
    let (mut bad, mut eng) = (Vec::new(), Vec::new());
    for l in lines {
        let c: Vec<&str> = l.split('\t').collect();
        let num = |i: usize| c.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        if num(4 + k) <= 0.0 || num(6 + k) <= 0.0 {
            continue;
        }
        n += 1;
        pages += num(2) as u64;
        r += num(4 + k);
        p1 += num(5 + k);
        pc += num(6 + k);
        runs += num(7 + k);
        if two {
            before += num(4);
            higher += usize::from(num(5) > num(4));
        }
        if c[2] != c[3] {
            bad.push(c[0]);
        }
        if c[1] != "native" {
            eng.push(c[0]);
        }
    }
    if n == 0 {
        println!(" no rows");
        return;
    }
    let nf = n as f64;
    if two {
        println!(" RushTeX before mean {:.1}M, after mean {:.1}M: saved {:.2}M a document ({:.1}%), {higher} of {n} documents higher after", before / nf / 1e6, r / nf / 1e6, (before - r) / nf / 1e6, 100.0 * (before - r) / before);
    }
    println!(" {n} documents, {pages} pages; RushTeX mean {:.1}M", r / nf / 1e6);
    println!(" pdfLaTeX one run mean {:.0}M ({:.1}x), final PDF mean {:.0}M ({:.1}x, {:.2} runs)", p1 / nf / 1e6, p1 / r, pc / nf / 1e6, pc / r, runs / nf);
    if !bad.is_empty() {
        println!(" page counts differ: {}", bad.join(" "));
    }
    if !eng.is_empty() {
        println!(" ran on the engine: {}", eng.join(" "));
    }
}

fn on_path(tool: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(tool).is_file()))
}

/// The terminal's settings while the menu reads single keys (`stty raw -echo`), restored when
/// dropped, also on a panic. `stty` keeps this std only and free of the per-platform
/// `termios` layouts.
struct RawTerminal(String);

impl RawTerminal {
    fn new() -> Option<RawTerminal> {
        let saved = Command::new("stty").arg("-g").stdin(Stdio::inherit()).output().ok().filter(|o| o.status.success())?;
        let saved = String::from_utf8_lossy(&saved.stdout).trim().to_string();
        Command::new("stty").args(["raw", "-echo"]).stdin(Stdio::inherit()).status().ok().filter(|s| s.success())?;
        Some(RawTerminal(saved))
    }
}

impl Drop for RawTerminal {
    fn drop(&mut self) {
        let _ = Command::new("stty").arg(&self.0).stdin(Stdio::inherit()).status();
        print!("\x1b[?25h");
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
    }
}

/// A key of the menu.
enum Key {
    Up,
    Down,
    Enter,
    Quit,
    Digit(usize),
    Other,
}

fn read_key() -> Key {
    use std::io::Read as _;
    let mut b = [0u8; 1];
    let mut next = || std::io::stdin().read(&mut b).ok().filter(|&n| n == 1).map(|_| b[0]);
    match next() {
        None | Some(3 | 4 | b'q' | b'Q') => Key::Quit,
        Some(b'\r' | b'\n') => Key::Enter,
        Some(b'k') => Key::Up,
        Some(b'j') => Key::Down,
        Some(c @ b'1'..=b'9') => Key::Digit(usize::from(c - b'0')),
        // `ESC [ A` and `ESC [ B` (or `ESC O A/B`); a lone escape quits.
        Some(0x1b) => match (next(), next()) {
            (Some(b'[' | b'O'), Some(b'A')) => Key::Up,
            (Some(b'[' | b'O'), Some(b'B')) => Key::Down,
            _ => Key::Quit,
        },
        _ => Key::Other,
    }
}

/// The menu `cargo benchmark` shows with no arguments at a terminal: arrows (or `j`/`k`) move,
/// a digit or Enter chooses, `q` quits. Each entry is a command line of `bench`, printed
/// before it runs so it can be typed again (or used in CI) without the menu.
fn bench_menu() -> Vec<String> {
    let mut entries: Vec<(&str, &str)> = vec![
        ("master.tex against pdfLaTeX", "instructions, CPU and wall time, RAM, cost per page, glyph-to-glyph and pixel match"),
        ("Any document against pdfLaTeX", "the same report for a file you name"),
        ("Every bench document", "docs/master.tex and scripts/bench/*.tex, one report each"),
        ("Check my change", "unit tests, native against engine page boxes, match with pdfLaTeX, byte-identical to a base"),
        ("Before/after one change", "an old rushtex binary against the current one"),
    ];
    // The template corpus is not in the public tree: listed only where it is.
    if root().join("template-corpus").is_dir() {
        entries.push(("Templates against pdfLaTeX", "the template-corpus documents the native job accepts"));
    }
    let draw = |sel: usize, first: bool| {
        use std::io::Write as _;
        let mut s = String::new();
        // Back to the menu's first line, redrawn in place.
        if !first {
            s += &format!("\x1b[{}A", entries.len() + 3);
        }
        s += "\r\x1b[2K\x1b[1mRushTeX benchmark\x1b[0m\r\n";
        for (i, (name, what)) in entries.iter().enumerate() {
            let row = format!("{}. {name:<31} \x1b[2m{what}\x1b[0m", i + 1);
            s += &if i == sel { format!("\r\x1b[2K \x1b[7m>\x1b[0m \x1b[1m{row}\x1b[0m\r\n") } else { format!("\r\x1b[2K   {row}\r\n") };
        }
        s += "\r\x1b[2K\r\n\r\x1b[2K\x1b[2m↑/↓ move   1-9 or Enter choose   q quit\x1b[0m\r\n";
        print!("{s}");
        let _ = std::io::stdout().flush();
    };
    let chosen = {
        let Some(_raw) = RawTerminal::new() else { die("the menu needs a terminal: name a mode (cargo benchmark --help)") };
        print!("\x1b[?25l");
        let mut sel = 0;
        draw(sel, true);
        loop {
            match read_key() {
                Key::Up => sel = (sel + entries.len() - 1) % entries.len(),
                Key::Down => sel = (sel + 1) % entries.len(),
                Key::Digit(d) if d <= entries.len() => break Some(d - 1),
                Key::Enter => break Some(sel),
                Key::Quit => break None,
                _ => continue,
            }
            draw(sel, false);
        }
    };
    let Some(pick) = chosen else { std::process::exit(0) };
    println!("\n{}. {}", pick + 1, entries[pick].0);
    let ask = |q: &str, default: &str| -> String {
        use std::io::Write as _;
        print!("{q}{}: ", if default.is_empty() { String::new() } else { format!(" [{default}]") });
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line).unwrap_or(0) == 0 {
            std::process::exit(0);
        }
        let v = if line.trim().is_empty() { default.to_string() } else { line.trim().to_string() };
        if v.is_empty() {
            die(&format!("{q}: no answer"));
        }
        v
    };
    let args = match pick {
        0 => vec![root().join("docs/master.tex").display().to_string()],
        1 => vec![ask("Document", "docs/master.tex")],
        2 => vec!["docs".into()],
        3 => vec!["check".into(), ask("Base to compare with (a git ref)", "origin/master")],
        4 => vec!["compare".into(), ask("Old rushtex binary", "")],
        _ => vec!["templates".into()],
    };
    println!("Running: cargo benchmark {}\n", args.join(" "));
    args
}

/// User and system CPU time, in milliseconds, of the children waited for so far
/// (`getrusage(RUSAGE_CHILDREN)`): one child's is the difference around its run.
fn children_cpu_ms() -> f64 {
    #[repr(C)]
    struct Timeval {
        sec: i64,
        #[cfg(target_os = "macos")]
        usec: i32,
        #[cfg(not(target_os = "macos"))]
        usec: i64,
    }
    #[repr(C)]
    struct Rusage {
        utime: Timeval,
        stime: Timeval,
        rest: [i64; 14],
    }
    extern "C" {
        fn getrusage(who: i32, usage: *mut Rusage) -> i32;
    }
    let mut ru = std::mem::MaybeUninit::<Rusage>::zeroed();
    // SAFETY: `ru` is a `struct rusage` of this platform's layout, which the call fills.
    if unsafe { getrusage(-1, ru.as_mut_ptr()) } != 0 {
        return 0.0;
    }
    // SAFETY: zeroed, then filled by `getrusage`.
    let ru = unsafe { ru.assume_init() };
    let ms = |t: &Timeval| t.sec as f64 * 1e3 + t.usec as f64 / 1e3;
    ms(&ru.utime) + ms(&ru.stime)
}

fn load_average() -> f64 {
    extern "C" {
        fn getloadavg(loads: *mut f64, n: i32) -> i32;
    }
    let mut l = [0f64; 3];
    // SAFETY: room for the three averages asked for.
    if unsafe { getloadavg(l.as_mut_ptr(), 3) } < 1 {
        return 0.0;
    }
    l[0]
}

fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = s.len();
    if n == 0 {
        0.0
    } else if n % 2 == 1 {
        s[n / 2]
    } else {
        (s[n / 2 - 1] + s[n / 2]) / 2.0
    }
}

/// Median instructions and the largest peak RSS of `runs` runs of `cmd`.
fn median_of(cmd: &[&std::ffi::OsStr], cwd: &Path, runs: usize) -> (f64, u64) {
    let (mut ins, mut rss) = (Vec::new(), 0);
    for _ in 0..runs {
        let (i, r, _) = measure(cmd, cwd, std::time::Duration::from_secs(600));
        ins.push(i as f64);
        rss = rss.max(r);
    }
    (median(&ins), rss)
}

/// Median wall and CPU (user + sys) milliseconds of `k` runs of each command, after one
/// warm-up each, as (wall, cpu) per command. The commands' runs alternate, so a change in the
/// machine's load falls on all of them alike.
fn timed(cmds: &[Vec<&std::ffi::OsStr>], cwd: &Path, k: usize) -> Vec<(f64, f64)> {
    let run = |cmd: &[&std::ffi::OsStr]| {
        let t = std::time::Instant::now();
        let c0 = children_cpu_ms();
        let _ = Command::new(cmd[0]).args(&cmd[1..]).current_dir(cwd).envs(FIXED_DATE).stdout(Stdio::null()).stderr(Stdio::null()).status();
        (t.elapsed().as_secs_f64() * 1e3, children_cpu_ms() - c0)
    };
    for cmd in cmds {
        run(cmd);
    }
    let mut v = vec![(Vec::new(), Vec::new()); cmds.len()];
    for _ in 0..k {
        for (i, cmd) in cmds.iter().enumerate() {
            let (w, c) = run(cmd);
            v[i].0.push(w);
            v[i].1.push(c);
        }
    }
    v.iter().map(|(w, c)| (median(w), median(c))).collect()
}

/// `1234.5M` with thousands separators and one decimal (Python's `{:,.1f}`).
fn millions(x: f64) -> String {
    let s = format!("{:.1}", x.abs() / 1e6);
    let (int, frac) = s.split_once('.').unwrap_or((&s, "0"));
    let mut grouped = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("{}{grouped}.{frac}M", if x < 0.0 && s != "0.0" { "-" } else { "" })
}

/// RushTeX's value against pdfLaTeX's, one decimal, said the way round that is above 1.
fn ratio(r: f64, p: f64, better: &str, worse: &str) -> String {
    if r <= p {
        format!("{:.1}x {better}", p / r)
    } else {
        format!("{:.1}x {worse}", r / p)
    }
}

/// Every `\label`, `\ref`, `\pageref`, `\eqref`, `\cite`, `\bibitem` key renamed `key-i`, so
/// copy `i` of a body has its own (keys after an optional `*` and `[..]`, comma lists too).
fn rename_keys(text: &str, i: usize) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len() + 256);
    let mut at = 0;
    let mut k = 0;
    while k < b.len() {
        let found = (b[k] == b'\\')
            .then(|| {
                ["label", "ref", "pageref", "eqref", "cite", "bibitem"].iter().find_map(|name| {
                    let mut j = k + 1;
                    b[j..].starts_with(name.as_bytes()).then_some(())?;
                    j += name.len();
                    if b.get(j) == Some(&b'*') {
                        j += 1;
                    }
                    if b.get(j) == Some(&b'[') {
                        j += b[j..].iter().position(|&c| c == b']')? + 1;
                    }
                    (b.get(j) == Some(&b'{')).then_some(())?;
                    let close = j + 1 + b[j + 1..].iter().position(|&c| c == b'}')?;
                    Some((j + 1, close))
                })
            })
            .flatten();
        match found {
            Some((open, close)) => {
                out.push_str(&text[at..open]);
                out.push_str(&text[open..close].split(',').map(|key| format!("{}-{i}", key.trim())).collect::<Vec<_>>().join(","));
                out.push('}');
                at = close + 1;
                k = close + 1;
            }
            None => k += 1,
        }
    }
    out.push_str(&text[at..]);
    out
}

fn latin1_write(path: &Path, text: &str) -> std::io::Result<()> {
    std::fs::write(path, text.chars().map(|c| c as u32 as u8).collect::<Vec<u8>>())
}

fn command_line(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd).args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").trim().to_string()).unwrap_or_default()
}

/// One document's full report: rushtex (one run, no `.aux`) against pdfLaTeX (one run, and
/// the runs a user needs), instructions, CPU and wall time, RAM, PDF size, start-up, the
/// cost of a page from copies of the body (x1, x4, x10, each box-checked against the
/// engine), and the parity line against pdfLaTeX.
fn bench_report(tex: &Path, runs: usize, rush: &Path) -> Result<(), String> {
    let tex = std::path::absolute(tex).map_err(|e| e.to_string())?;
    let work = std::env::temp_dir().join(format!("rushtex_bench_{}_{}", std::process::id(), stem(&tex)));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let r = bench_report_in(&tex, runs, rush, &work);
    let _ = std::fs::remove_dir_all(&work);
    r
}

fn refs(v: &[std::ffi::OsString]) -> Vec<&std::ffi::OsStr> {
    v.iter().map(|s| s.as_os_str()).collect()
}

fn bench_report_in(tex: &Path, runs: usize, rush: &Path, work: &Path) -> Result<(), String> {
    let os = |p: &Path| p.as_os_str().to_owned();
    let name = stem(tex);
    let dir = tex.parent().ok_or("no folder")?;
    let src = work.join(tex.file_name().ok_or("no file name")?);
    std::fs::copy(tex, &src).map_err(|e| e.to_string())?;
    // Its BibTeX output, read in place of `\bibliography`.
    let bbl = tex.with_extension("bbl");
    if bbl.exists() {
        let _ = std::fs::copy(&bbl, work.join(bbl.file_name().unwrap()));
    }
    // The images the document includes, from the folders beside it (`figures/`).
    for d in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let has_images = |p: &Path| std::fs::read_dir(p).is_ok_and(|r| r.flatten().any(|f| f.path().extension().is_some_and(|x| x == "pdf" || x == "png")));
        if d.path().is_dir() && has_images(&d.path()) {
            copy_tree(&d.path(), &work.join(d.file_name())).map_err(|e| e.to_string())?;
        }
    }
    // rushtex: one run, no .aux.
    let rush_pdf = work.join("rush.pdf");
    let rush_cmd: Vec<std::ffi::OsString> = vec![os(rush), os(&src), "-o".into(), os(&rush_pdf)];
    let accepted = |cmd: &[std::ffi::OsString]| Command::new(&cmd[0]).args(&cmd[1..]).current_dir(work).envs(FIXED_DATE).env("RT_NATIVE_WHY", "1").output().is_ok_and(|o| String::from_utf8_lossy(&o.stderr).contains("native job accepted"));
    let native = accepted(&rush_cmd);
    let (r_ins, r_rss) = median_of(&refs(&rush_cmd), work, runs);
    // pdfLaTeX: the real cycle from clean, then one run with the .aux complete.
    let timeout = std::time::Duration::from_secs(600);
    let mut cycle = Vec::new();
    for _ in 0..runs {
        let (counts, n) = pdflatex_cycle(work, &src, timeout);
        cycle.push((counts[..n].iter().sum::<u64>() as f64, n));
    }
    let p_real = median(&cycle.iter().map(|c| c.0).collect::<Vec<_>>());
    let p_runs = cycle[0].1;
    let pdf_cmd: Vec<std::ffi::OsString> = vec!["pdflatex".into(), "-interaction=batchmode".into(), os(&src)];
    let (p_ins, p_rss) = median_of(&refs(&pdf_cmd), work, runs);
    let p_pdf = work.join(format!("{name}.pdf"));
    let k = std::env::var("TIME_RUNS").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(11).max(1);
    let load = load_average();
    let t = timed(&[refs(&rush_cmd), refs(&pdf_cmd)], work, k);
    let ((r_wall, r_cpu), (p_wall, p_cpu)) = (t[0], t[1]);
    let (rp, pp) = (pdf_pages(&rush_pdf), pdf_pages(&p_pdf));
    let size = |p: &Path| std::fs::metadata(p).map_or(0.0, |m| m.len() as f64 / 1024.0);
    let (rk, pk) = (size(&rush_pdf), size(&p_pdf));
    let text: String = std::fs::read(tex).map_err(|e| e.to_string())?.iter().map(|&b| char::from(b)).collect();

    // Start-up: the same preamble with a one-word body (process, class and packages, one
    // page), and the OS's own process floor.
    let (pre, body) = text.split_once("\\begin{document}").ok_or("no \\begin{document}")?;
    let mut body = body.rsplit_once("\\end{document}").map_or(body, |(b, _)| b).to_string();
    // The scaled copies are new jobs with no .bbl of their own: the bibliography goes in as text.
    if bbl.exists() {
        let bib: String = std::fs::read(&bbl).map_err(|e| e.to_string())?.iter().map(|&b| char::from(b)).collect::<String>().trim_end_matches('\n').to_string();
        body = replace_command(&body, "\\bibliography{", Some(&bib));
        body = replace_command(&body, "\\bibliographystyle{", None);
    }
    let word = work.join("word.tex");
    latin1_write(&word, &format!("{pre}\\begin{{document}}\nx\n\\end{{document}}\n")).map_err(|e| e.to_string())?;
    let word_rush: Vec<std::ffi::OsString> = vec![os(rush), os(&word), "-o".into(), os(&work.join("word-rush.pdf"))];
    let word_pdf: Vec<std::ffi::OsString> = vec!["pdflatex".into(), "-interaction=batchmode".into(), os(&word)];
    let (rb_ins, _) = median_of(&refs(&word_rush), work, runs);
    let (pb_ins, _) = median_of(&refs(&word_pdf), work, runs);
    let t = timed(&[refs(&word_rush), refs(&word_pdf)], work, k);
    let ((rb_wall, rb_cpu), (pb_wall, pb_cpu)) = (t[0], t[1]);
    let (os_ins, _) = median_of(&["/usr/bin/true".as_ref()], work, runs);

    // Scaling: the body after the title and contents repeated 1, 4 and 10 times (keys renamed
    // per copy), instructions against pages; the slope is the cost of a page, the intercept
    // the fixed cost. pdfLaTeX is measured on a run with its .aux complete.
    let cut = ["\\tableofcontents", "\\maketitle", "\\end{abstract}"].iter().filter_map(|m| body.find(m).map(|i| i + m.len())).max().unwrap_or(0);
    let (head, rest) = body.split_at(cut);
    // Each native copy is also box-checked against the engine (`native_probes.sh`): a native
    // run is not proof of a right page.
    let (mut fit, mut all_native, mut boxes) = (Vec::new(), true, Vec::new());
    for n in [1usize, 4, 10] {
        let f = work.join(format!("scale{n}.tex"));
        let copies: String = (0..n).map(|i| if i == 0 { rest.to_string() } else { rename_keys(rest, i) }).collect();
        latin1_write(&f, &format!("{pre}\\begin{{document}}{head}{copies}\\end{{document}}\n")).map_err(|e| e.to_string())?;
        let out = work.join(format!("scale{n}-rush.pdf"));
        let cmd: Vec<std::ffi::OsString> = vec![os(rush), os(&f), "-o".into(), os(&out)];
        all_native &= accepted(&cmd);
        let probe = Command::new("zsh").arg(root().join("scripts/native_probes.sh")).arg(&f).args(["-n", "0"]).current_dir(root()).env("RT_BIN", rush).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        boxes.push(format!("x{n} {}", if probe.lines().last().is_some_and(|l| l.ends_with("identical")) { "identical" } else { "DIFFERS" }));
        let (ri, _) = median_of(&refs(&cmd), work, runs);
        let latex: Vec<std::ffi::OsString> = vec!["pdflatex".into(), "-interaction=batchmode".into(), os(&f)];
        for _ in 0..2 {
            let _ = Command::new("pdflatex").arg("-interaction=batchmode").arg(&f).current_dir(work).envs(FIXED_DATE).output();
        }
        let (pi, _) = median_of(&refs(&latex), work, runs);
        fit.push((pdf_pages(&out) as f64, ri, pdf_pages(&work.join(format!("scale{n}.pdf"))) as f64, pi));
    }
    let line = |xs: &[f64], ys: &[f64]| {
        let (mx, my) = (xs.iter().sum::<f64>() / xs.len() as f64, ys.iter().sum::<f64>() / ys.len() as f64);
        let slope = xs.iter().zip(ys).map(|(x, y)| (x - mx) * (y - my)).sum::<f64>() / xs.iter().map(|x| (x - mx).powi(2)).sum::<f64>().max(1.0);
        (slope, my - slope * mx)
    };
    let (r_slope, r_icpt) = line(&fit.iter().map(|f| f.0).collect::<Vec<_>>(), &fit.iter().map(|f| f.1).collect::<Vec<_>>());
    let (p_slope, p_icpt) = line(&fit.iter().map(|f| f.2).collect::<Vec<_>>(), &fit.iter().map(|f| f.3).collect::<Vec<_>>());

    // The output against pdfLaTeX's, `zero --pixels` (glyphs, paths, links, pixels at 300
    // dpi), its line for the document.
    let mut parity = String::from("no result");
    let zo = Opts { docs: vec![tex.display().to_string()], all: false, build: false, cached: false, pixels: true, dpi: 300, why: None, jobs: JOBS.load(Ordering::Relaxed).max(1), flags: HashMap::new() };
    zero_each(&zo, |l, _| parity = l.trim().to_string());

    let mac = cfg!(target_os = "macos");
    let ver = command_line("pdflatex", &["--version"]);
    let rev = command_line("git", &["-C", &rush.parent().map_or(String::from("."), |p| p.display().to_string()), "rev-parse", "--short", "HEAD"]);
    let cpu_name = if mac { command_line("sysctl", &["-n", "machdep.cpu.brand_string"]) } else { command_line("uname", &["-p"]) };
    let cpu_name = if cpu_name.is_empty() || cpu_name == "unknown" { std::env::consts::ARCH.to_string() } else { cpu_name };
    let counter = if mac { "/usr/bin/time -l (counts the kernel's process set-up and dyld too)" } else { "perf stat -e instructions:u (user space only)" };
    let cwd = std::env::current_dir().unwrap_or_default();
    let shown = tex.strip_prefix(&cwd).map_or(tex.display().to_string(), |p| p.display().to_string());
    let rule = "=".repeat(84);
    let s = |n: usize| if n > 1 { "s" } else { "" };
    println!("{rule}");
    println!(" Machine: {cpu_name}, {} {}; instructions: {counter}", command_line("uname", &["-s"]), command_line("uname", &["-r"]));
    println!(" pdfLaTeX: {ver}; `pdflatex -interaction=batchmode`, no latexmk");
    println!("   (the benchmark reruns it itself until the PDF stops changing); RushTeX {rev}, one run");
    println!(" {shown}: {rp} pages (pdfLaTeX {pp}); RushTeX ran {}", if native { "natively" } else { "on the ENGINE (not native)" });
    println!(" Instructions: median of {runs}; times: median of {k} after a warm-up, runs alternating, load average {load:.1}; pdfLaTeX needed {p_runs} run{} for its final PDF", s(p_runs));
    println!(" Parity against pdfLaTeX: {parity}");
    println!("{rule}");
    let mb = |x: u64| format!("{:.1} MB", x as f64 / 1048576.0);
    let rows = [
        ("Instructions, 1 run".to_string(), millions(r_ins), millions(p_ins), ratio(r_ins, p_ins, "less", "more")),
        (format!("Instructions, real ({p_runs} run{})", s(p_runs)), millions(r_ins), millions(p_real), ratio(r_ins, p_real, "less", "more")),
        ("CPU time (user+sys), 1 run".into(), format!("{r_cpu:.1} ms"), format!("{p_cpu:.1} ms"), ratio(r_cpu, p_cpu, "less", "more")),
        ("Wall time, 1 run".into(), format!("{r_wall:.1} ms"), format!("{p_wall:.1} ms"), ratio(r_wall, p_wall, "less", "more")),
        ("Peak RAM (RSS)".into(), mb(r_rss), mb(p_rss), ratio(r_rss as f64, p_rss as f64, "less", "more")),
        ("PDF size".into(), format!("{rk:.1} KB"), format!("{pk:.1} KB"), ratio(rk, pk, "smaller", "larger")),
        ("Fixed cost (fit intercept)".into(), millions(r_icpt), millions(p_icpt), ratio(r_icpt, p_icpt, "less", "more")),
        ("Cost per page (fit slope)".into(), millions(r_slope), millions(p_slope), ratio(r_slope, p_slope, "less", "more")),
        ("Same preamble, one-word body".into(), millions(rb_ins), millions(pb_ins), ratio(rb_ins, pb_ins, "less", "more")),
        ("  its CPU / wall time".into(), format!("{rb_cpu:.1}/{rb_wall:.1} ms"), format!("{pb_cpu:.0}/{pb_wall:.0} ms"), String::new()),
        ("OS process floor (/usr/bin/true)".into(), millions(os_ins), millions(os_ins), String::new()),
    ];
    println!(" {:<34} {:>13} {:>13}   Factor", "Metric", "RushTeX", "pdfLaTeX");
    for (m, a, b, f) in rows {
        println!(" {m:<34} {a:>13} {b:>13}   {f}");
    }
    println!("{}", "-".repeat(84));
    let pts: Vec<String> = [1, 4, 10].iter().zip(&fit).map(|(n, f)| format!("x{n}: {} p {} / {} p {}", f.0, millions(f.1), f.2, millions(f.3))).collect();
    println!(" Fit points (RushTeX / pdfLaTeX): {}{}", pts.join("; "), if all_native { "" } else { "  [a RushTeX variant ran on the ENGINE]" });
    println!(" Box check of the copies against the engine: {}", boxes.join(", "));
    println!("{rule}");
    Ok(())
}

/// Every `cmd{…}` (`cmd` ends with `{`; the argument holds no `}`) replaced by `with`, or
/// removed.
fn replace_command(text: &str, cmd: &str, with: Option<&str>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(cmd) {
        let Some(close) = rest[i + cmd.len()..].find('}') else { break };
        out.push_str(&rest[..i]);
        out.push_str(with.unwrap_or(""));
        rest = &rest[i + cmd.len() + close + 1..];
    }
    out.push_str(rest);
    out
}

/// `bench check [BASE]`: the contributor's question, "is my output still identical?", in one
/// run: build, unit tests, the native job against the engine page box by page box, master
/// and `scripts/bench` against pdfLaTeX, and with BASE every PDF byte-identical to BASE's.
/// `PASS`/`FAIL` per step; exits 1 on a failure.
fn bench_check(o: &Opts, rush: &Path, base: Option<&str>) {
    let root = root();
    let mut fails = 0;
    let mut verdict = |ok: bool, what: String| {
        println!("  {}  {what}", if ok { "PASS" } else { "FAIL" });
        fails += usize::from(!ok);
    };
    for t in ["cargo", "pdflatex", "kpsewhich", "mutool", "zsh", "git"] {
        if !on_path(t) {
            die(&format!("{t} not found on PATH (needed: Rust, TeX Live, MuPDF's mutool, zsh, git)"));
        }
    }
    println!("1/6 Building rushtex");
    if !Command::new("cargo").args(["build", "--release", "--quiet"]).current_dir(&root).status().is_ok_and(|s| s.success()) {
        println!("  FAIL  build");
        std::process::exit(1);
    }
    // The engine's LaTeX format, made once (about 10 s): the probes run the engine on it.
    if !root.join("target/parity/pdflatex.rfmt").exists() {
        let _ = Command::new(rush).arg("scripts/native_probes/title.tex").args(["-o", "target/parity/format-check.pdf"]).current_dir(&root).env("RT_ENGINE", "1").env("RUSHTEX_CACHE", root.join("target/parity")).output();
    }
    println!("2/6 Unit tests");
    let out = Command::new("cargo").args(["test", "--release", "--lib", "--quiet"]).current_dir(&root).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr)).unwrap_or_default();
    let result = out.lines().find(|l| l.starts_with("test result")).unwrap_or("did not run").to_string();
    verdict(result.contains(" 0 failed"), format!("unit tests ({})", result.trim_start_matches("test result: ")));
    println!("3/6 Native job against the engine, page box by page box (scripts/native_probes.sh)");
    let probes = Command::new("zsh").arg("scripts/native_probes.sh").args(["-n", "6"]).current_dir(&root).env("RT_BIN", rush).output();
    let text = probes.as_ref().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let ok = probes.is_ok_and(|o| o.status.success());
    if !ok {
        for l in text.lines().filter(|l| !l.ends_with("identical")) {
            println!("        {l}");
        }
    }
    verdict(ok, if ok { format!("{} documents identical", text.lines().filter(|l| l.ends_with("identical")).count()) } else { "native probes".into() });
    println!("4/6 Against pdfLaTeX: glyphs, paths, links, pixels at 300 dpi (parity-testing zero)");
    let mut docs = vec![root.join("docs/master.tex").display().to_string()];
    let mut bench: Vec<String> = std::fs::read_dir(root.join("scripts/bench")).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "tex")).map(|p| p.display().to_string()).collect()).unwrap_or_default();
    bench.sort();
    docs.extend(bench);
    let zo = Opts { docs: docs.clone(), all: false, build: false, cached: false, pixels: true, dpi: 300, why: None, jobs: o.jobs, flags: HashMap::new() };
    JOBS.store(1, Ordering::Relaxed);
    let mut zero = 0;
    zero_each(&zo, |l, ok| {
        println!("        {l}");
        zero += usize::from(ok == Some(true));
    });
    verdict(zero == docs.len(), if zero == docs.len() { format!("{zero} documents identical to pdfLaTeX") } else { format!("{} of {} documents differ from pdfLaTeX", docs.len() - zero, docs.len()) });
    println!("5/6 The watch session after random edits, byte for byte against fresh compiles (parity-testing incr)");
    // master.tex on the native job, bare_jrnl (IEEEtran) on the engine: a few seconds each.
    for (doc, edits) in [(root.join("docs/master.tex"), 20), (corpus().join("bare_jrnl.tex"), 6)] {
        match incr_doc(rush, &doc, edits, 1, false) {
            Ok(line) => verdict(true, line.trim_start_matches("PASS ").to_string()),
            Err(report) => {
                for l in report.lines().skip(1) {
                    println!("        {l}");
                }
                verdict(false, report.lines().next().unwrap_or_default().trim_start_matches("FAIL ").to_string());
            }
        }
    }
    println!("6/6 Byte-identical to {}", base.unwrap_or("BASE (none given: cargo benchmark check origin/master)"));
    let Some(base) = base else {
        println!("  SKIP  no BASE given");
        return finish_check(fails);
    };
    let wt = root.join("target/check-base");
    let base_target = root.join("target/check-base-target");
    let git = |args: &[&std::ffi::OsStr]| Command::new("git").args(args).current_dir(&root).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
    git(&["worktree".as_ref(), "remove".as_ref(), "--force".as_ref(), wt.as_os_str()]);
    let built = git(&["worktree".as_ref(), "add".as_ref(), "--detach".as_ref(), "--quiet".as_ref(), wt.as_os_str(), base.as_ref()]) && Command::new("cargo").args(["build", "--release", "--quiet"]).current_dir(&wt).env("CARGO_TARGET_DIR", &base_target).status().is_ok_and(|s| s.success());
    if built {
        let (old, now) = (root.join("target/check-pdfs/base"), root.join("target/check-pdfs/now"));
        let _ = std::fs::remove_dir_all(root.join("target/check-pdfs"));
        let _ = std::fs::create_dir_all(&old);
        let _ = std::fs::create_dir_all(&now);
        let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("scripts/native_probes")).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "tex")).collect()).unwrap_or_default();
        files.sort();
        files.extend(docs.iter().map(PathBuf::from));
        let compile = |bin: &Path, f: &Path, out: &Path| Command::new(bin).arg(f).arg("-o").arg(out).current_dir(&root).envs(FIXED_DATE).stdout(Stdio::null()).stderr(Stdio::null()).status();
        let mut differ = Vec::new();
        for f in &files {
            let pdf = format!("{}.pdf", stem(f));
            let _ = compile(&base_target.join("release/rushtex"), f, &old.join(&pdf));
            let _ = compile(rush, f, &now.join(&pdf));
            let (a, b) = (std::fs::read(old.join(&pdf)).unwrap_or_default(), std::fs::read(now.join(&pdf)).unwrap_or_default());
            if a.is_empty() || a != b {
                differ.push(stem(f));
            }
        }
        verdict(differ.is_empty(), if differ.is_empty() { format!("{} PDFs byte-identical to {base}", files.len()) } else { format!("{} PDFs differ from {base}: {} (both in target/check-pdfs/)", differ.len(), differ.join(" ")) });
    } else {
        verdict(false, format!("building {base}"));
    }
    git(&["worktree".as_ref(), "remove".as_ref(), "--force".as_ref(), wt.as_os_str()]);
    finish_check(fails);
}

fn finish_check(fails: usize) {
    println!();
    if fails == 0 {
        println!("check: PASS");
    } else {
        println!("check: FAIL ({fails} step{})", if fails > 1 { "s" } else { "" });
        std::process::exit(1);
    }
}

// ---- difflib ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tag {
    Equal,
    Replace,
    Delete,
    Insert,
}

/// Python's `difflib.SequenceMatcher(None, a, b, autojunk=False)`: no junk and no popular
/// elements, so every element of `b` is indexed. The same longest-match search, the same
/// tie-breaking (earliest in `a`, then earliest in `b`) and the same block order, so the
/// opcodes are Python's.
struct SequenceMatcher<'a, T: Eq + std::hash::Hash> {
    a: &'a [T],
    b: &'a [T],
    b2j: HashMap<&'a T, Vec<usize>>,
}

impl<'a, T: Eq + std::hash::Hash> SequenceMatcher<'a, T> {
    fn new(a: &'a [T], b: &'a [T]) -> Self {
        let mut b2j: HashMap<&T, Vec<usize>> = HashMap::new();
        for (j, x) in b.iter().enumerate() {
            b2j.entry(x).or_default().push(j);
        }
        SequenceMatcher { a, b, b2j }
    }

    /// `find_longest_match(alo, ahi, blo, bhi)`: (i, j, size).
    fn find_longest_match(&self, alo: usize, ahi: usize, blo: usize, bhi: usize) -> (usize, usize, usize) {
        let (mut besti, mut bestj, mut bestsize) = (alo, blo, 0);
        // j2len[j] = length of the longest match ending with a[i-1] and b[j].
        let mut j2len: HashMap<usize, usize> = HashMap::new();
        let mut newj2len: HashMap<usize, usize> = HashMap::new();
        for i in alo..ahi {
            newj2len.clear();
            for &j in self.b2j.get(&self.a[i]).map(Vec::as_slice).unwrap_or(&[]) {
                if j < blo {
                    continue;
                }
                if j >= bhi {
                    break;
                }
                let k = j.checked_sub(1).and_then(|p| j2len.get(&p)).copied().unwrap_or(0) + 1;
                newj2len.insert(j, k);
                if k > bestsize {
                    (besti, bestj, bestsize) = (i + 1 - k, j + 1 - k, k);
                }
            }
            std::mem::swap(&mut j2len, &mut newj2len);
        }
        // Python's extension over non-junk neighbours; with no junk it finds nothing more,
        // and is kept so the search is the same.
        while besti > alo && bestj > blo && self.a[besti - 1] == self.b[bestj - 1] {
            (besti, bestj, bestsize) = (besti - 1, bestj - 1, bestsize + 1);
        }
        while besti + bestsize < ahi && bestj + bestsize < bhi && self.a[besti + bestsize] == self.b[bestj + bestsize] {
            bestsize += 1;
        }
        (besti, bestj, bestsize)
    }

    /// `get_matching_blocks()`, ending with the sentinel `(len(a), len(b), 0)`.
    fn matching_blocks(&self) -> Vec<(usize, usize, usize)> {
        let (la, lb) = (self.a.len(), self.b.len());
        let mut queue = vec![(0, la, 0, lb)];
        let mut blocks = Vec::new();
        while let Some((alo, ahi, blo, bhi)) = queue.pop() {
            let (i, j, k) = self.find_longest_match(alo, ahi, blo, bhi);
            if k > 0 {
                blocks.push((i, j, k));
                if alo < i && blo < j {
                    queue.push((alo, i, blo, j));
                }
                if i + k < ahi && j + k < bhi {
                    queue.push((i + k, ahi, j + k, bhi));
                }
            }
        }
        blocks.sort_unstable();
        let (mut i1, mut j1, mut k1) = (0, 0, 0);
        let mut out = Vec::with_capacity(blocks.len() + 1);
        for (i2, j2, k2) in blocks {
            if i1 + k1 == i2 && j1 + k1 == j2 {
                k1 += k2;
            } else {
                if k1 > 0 {
                    out.push((i1, j1, k1));
                }
                (i1, j1, k1) = (i2, j2, k2);
            }
        }
        if k1 > 0 {
            out.push((i1, j1, k1));
        }
        out.push((la, lb, 0));
        out
    }

    /// `get_opcodes()`: (tag, i1, i2, j1, j2).
    fn opcodes(&self) -> Vec<(Tag, usize, usize, usize, usize)> {
        let (mut i, mut j) = (0, 0);
        let mut out = Vec::new();
        for (ai, bj, size) in self.matching_blocks() {
            let tag = match (i < ai, j < bj) {
                (true, true) => Some(Tag::Replace),
                (true, false) => Some(Tag::Delete),
                (false, true) => Some(Tag::Insert),
                _ => None,
            };
            if let Some(t) = tag {
                out.push((t, i, ai, j, bj));
            }
            (i, j) = (ai + size, bj + size);
            if size > 0 {
                out.push((Tag::Equal, ai, i, bj, j));
            }
        }
        out
    }
}

// ---- small helpers in Python's terms ---------------------------------------------------------------

/// The first `n` characters (Python's `s[:n]`).
fn head(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Python's `str.splitlines()`: every line boundary Python knows, `\r\n` as one.
fn py_splitlines(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\r' => {
                if it.peek() == Some(&'\n') {
                    it.next();
                }
                out.push(std::mem::take(&mut cur));
            }
            '\n' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}' => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Python's `'%g' % x`.
fn py_g(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    let e = format!("{x:.5e}");
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let strip = |s: &str| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s.to_string()
        }
    };
    if !(-4..6).contains(&exp) {
        format!("{}e{}{:02}", strip(mant), if exp < 0 { '-' } else { '+' }, exp.abs())
    } else {
        strip(&format!("{x:.*}", (5 - exp) as usize))
    }
}

/// Python's `round(x, n)`: half to even on the exact binary value, as Rust's formatting
/// rounds.
fn py_round(x: f64, n: usize) -> f64 {
    format!("{x:.n$}").parse().unwrap_or(x)
}

/// `{x:+7.2f}`, with Python's `+nan`.
fn signed(x: f64, width: usize) -> String {
    if x.is_nan() {
        format!("{:>width$}", "+nan")
    } else {
        format!("{x:+width$.2}")
    }
}

/// The leading run of characters of `s` in `set` (a character class `[...]+`), and the rest.
fn run_of<'s>(s: &'s str, set: &str) -> (&'s str, &'s str) {
    let n = s.find(|c: char| !set.contains(c)).unwrap_or(s.len());
    s.split_at(n)
}

const NUM: &str = "-0123456789.";
const UNUM: &str = "0123456789.";

/// `fil*` (`plus`) or `fil+` (`strict`) at the start of `s`.
fn fil(s: &str, strict: bool) -> (&str, &str) {
    let pre = if strict { "fil" } else { "fi" };
    if !s.starts_with(pre) {
        return ("", s);
    }
    let n = pre.len() + s[pre.len()..].len() - s[pre.len()..].trim_start_matches('l').len();
    s.split_at(n)
}

fn f(s: &str) -> Option<f64> {
    s.parse().ok()
}

// ---- lines -----------------------------------------------------------------------------------------

const TOL: f64 = 0.1;

type Line = (f64, f64, String);

/// Ref and our baselines paired by their text (difflib), so an extra or missing line on one
/// side does not shift every pair after it.
fn pair_lines(ra: &[Line], rb: &[Line]) -> Vec<(Line, Line)> {
    let (ta, tb): (Vec<&str>, Vec<&str>) = (ra.iter().map(|r| r.2.as_str()).collect(), rb.iter().map(|r| r.2.as_str()).collect());
    let none = || (0.0, 0.0, String::new());
    let mut out = Vec::new();
    for (tag, i1, i2, j1, j2) in SequenceMatcher::new(&ta, &tb).opcodes() {
        if tag == Tag::Equal || (tag == Tag::Replace && i2 - i1 == j2 - j1) {
            out.extend(ra[i1..i2].iter().cloned().zip(rb[j1..j2].iter().cloned()));
        } else {
            out.extend(ra[i1..i2].iter().map(|a| (a.clone(), none())));
            out.extend(rb[j1..j2].iter().map(|b| (none(), b.clone())));
        }
    }
    out
}

fn print_page_lines(a: &[Vec<Glyph>], b: &[Vec<Glyph>], page: usize, all: bool) -> Result<(), String> {
    let (Some(ga), Some(gb)) = (page.checked_sub(1).and_then(|p| a.get(p)), page.checked_sub(1).and_then(|p| b.get(p))) else {
        return Err(format!("page {page} is out of range"));
    };
    let mut n = 0;
    for (x, y) in pair_lines(&baselines(ga, 6.0), &baselines(gb, 6.0)) {
        let (dy, dx) = (y.0 - x.0, y.1 - x.1);
        if all || dy.abs() > TOL || dx.abs() > TOL || x.2 != y.2 {
            println!("{:8.2} {} dx{}  {} | {}", x.0, signed(dy, 7), signed(dx, 6), head(&x.2, 40), head(&y.2, 40));
            n += 1;
        }
    }
    if n == 0 {
        println!("page {page}: all baselines within {TOL}pt");
    }
    Ok(())
}

/// Both documents' glyphs, extracted at once.
fn both_glyphs(refp: &Path, ours: &Path) -> Result<(Pages, Pages), String> {
    let (a, b) = std::thread::scope(|sc| {
        let h = sc.spawn(|| doc_glyphs(refp));
        let b = doc_glyphs(ours);
        (h.join().unwrap_or_else(|_| Err("extraction panicked".into())), b)
    });
    Ok((a?, b?))
}

fn doc_page(o: &Opts, what: &str) -> (PathBuf, usize) {
    let doc = o.docs.first().unwrap_or_else(|| die(&format!("{what} needs DOC PAGE")));
    let page = o.docs.get(1).and_then(|p| p.parse().ok()).unwrap_or_else(|| die(&format!("{what} needs DOC PAGE")));
    (source(doc).unwrap_or_else(|e| die(&e)), page)
}

fn cmd_lines(o: &Opts) {
    let (tex, page) = doc_page(o, "lines");
    let r = guarded(|| {
        let refp = ref_pdf(&tex)?;
        let ours = if o.cached { ours_path(&tex) } else { ours_pdf(&tex)? };
        let (a, b) = both_glyphs(&refp, &ours)?;
        print_page_lines(&a, &b, page, o.all)
    });
    if let Err(e) = r {
        die(&e);
    }
}

// ---- boxes: pdfTeX's shipped pages from \showbox ---------------------------------------------------

const PT_TO_BP: f64 = 72.0 / 72.27;

/// pdfTeX's log with every shipped page box shown in full (cached like the reference).
fn box_log(tex: &Path) -> Result<String, String> {
    let d = out_dir().join("boxes");
    let log = d.join(format!("{}.log", stem(tex)));
    if let (Some(l), Some(t)) = (mtime(&log), mtime(tex)) {
        if l >= t {
            return std::fs::read(&log).map(|b| latin1(&b)).map_err(|e| e.to_string());
        }
    }
    let _ = std::fs::create_dir_all(&d);
    let hook = r"\showboxdepth=100000 \showboxbreadth=10000000 \AddToHook{shipout/before}{\showbox\ShipoutBox}";
    let name = tex.file_name().unwrap().to_string_lossy();
    for _ in 0..2 {
        let _ = Command::new("pdflatex")
            .arg("-interaction=nonstopmode")
            .arg(format!("-output-directory={}", d.display()))
            .arg(format!("-jobname={}", stem(tex)))
            .arg(format!("{hook}\\input{{{name}}}"))
            .current_dir(tex.parent().unwrap())
            .env("max_print_line", "10000000")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    std::fs::read(&log).map(|b| latin1(&b)).map_err(|e| format!("{}: {e}", log.display()))
}

#[derive(Default)]
struct BoxNode {
    text: String,
    children: Vec<BoxNode>,
}

/// `^\.*\\[A-Z0-9]+/\S+ $`: a glyph line whose character, code 10, was printed raw.
fn glyph_nl(line: &str) -> bool {
    let t = line.trim_start_matches('.');
    let Some(t) = t.strip_prefix('\\') else { return false };
    let (id, t) = run_of(t, "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
    let Some(t) = t.strip_prefix('/').filter(|_| !id.is_empty()) else { return false };
    let Some(t) = t.strip_suffix(' ') else { return false };
    !t.is_empty() && !t.chars().any(py_space)
}

/// Each shipped page as a tree. Split on newlines only: pdfTeX prints glyph codes such as
/// the fi ligature (12) raw.
fn box_pages(log: &str) -> Vec<BoxNode> {
    let lines: Vec<&str> = log.split('\n').collect();
    let mut pages = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].starts_with("> \\box") && lines[i].ends_with('=') {
            let mut j = i + 1;
            let mut body = Vec::new();
            while j < lines.len() && !lines[j].is_empty() && !lines[j].starts_with("! OK") {
                if glyph_nl(lines[j]) && j + 1 < lines.len() {
                    body.push(format!("{}\n{}", lines[j], lines[j + 1]));
                    j += 2;
                    continue;
                }
                body.push(lines[j].to_string());
                j += 1;
            }
            pages.push(build_tree(&body));
            i = j;
        }
        i += 1;
    }
    pages
}

fn build_tree(body: &[String]) -> BoxNode {
    // `stack` holds (depth, path of child indices from the root).
    let mut root = BoxNode::default();
    let mut stack: Vec<(usize, Vec<usize>)> = Vec::new();
    for line in body {
        let depth = line.len() - line.trim_start_matches('.').len();
        while stack.last().is_some_and(|s| s.0 >= depth) {
            stack.pop();
        }
        let mut path = stack.last().map(|s| s.1.clone()).unwrap_or_default();
        let mut node = &mut root;
        for &k in &path {
            node = &mut node.children[k];
        }
        node.children.push(BoxNode { text: line[depth..].to_string(), children: Vec::new() });
        path.push(node.children.len() - 1);
        stack.push((depth, path));
    }
    if root.children.is_empty() {
        root
    } else {
        root.children.swap_remove(0)
    }
}

/// `BOX_RE`: `\[hv]box(H+D)xW[, glue set [- ]R[fil*]][, shifted S]`.
struct BoxHead {
    vertical: bool,
    h: f64,
    d: f64,
    /// -1 for shrinking, 1 for stretching, and the ratio and order of the glue set.
    set: (i32, f64, String),
    shift: Option<f64>,
}

fn box_head(t: &str) -> Option<BoxHead> {
    let t = t.strip_prefix('\\')?;
    let vertical = match t.as_bytes().first()? {
        b'h' => false,
        b'v' => true,
        _ => return None,
    };
    let t = t[1..].strip_prefix("box(")?;
    let (h, t) = run_of(t, NUM);
    let t = t.strip_prefix('+')?;
    let (d, t) = run_of(t, NUM);
    let t = t.strip_prefix(")x")?;
    let (w, mut t) = run_of(t, NUM);
    if h.is_empty() || d.is_empty() || w.is_empty() {
        return None;
    }
    let mut set = (1, 0.0, String::new());
    if let Some(g) = t.strip_prefix(", glue set ") {
        let (neg, g) = match g.strip_prefix("- ") {
            Some(g) => (true, g),
            None => (false, g),
        };
        let (r, g) = run_of(g, UNUM);
        if !r.is_empty() {
            let (o, g) = fil(g, false);
            set = (if neg { -1 } else { 1 }, f(r)?, o.to_string());
            t = g;
        }
    }
    let shift = match t.strip_prefix(", shifted ").map(|s| run_of(s, NUM).0).filter(|s| !s.is_empty()) {
        Some(s) => Some(f(s)?),
        None => None,
    };
    Some(BoxHead { vertical, h: f(h)?, d: f(d)?, set, shift })
}

/// `GLUE_RE`: `\glue[(\name)] N[ plus P[fil*]][ minus M[fil*]]`.
struct GlueM<'s> {
    name: Option<&'s str>,
    nat: f64,
    plus: Option<(f64, &'s str)>,
    minus: Option<(f64, &'s str)>,
}

fn glue_m(t: &str) -> Option<GlueM<'_>> {
    let mut t = t.strip_prefix("\\glue")?;
    let mut name = None;
    if let Some(n) = t.strip_prefix("(\\") {
        let (id, rest) = run_of(n, "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz@");
        if let Some(rest) = rest.strip_prefix(')').filter(|_| !id.is_empty()) {
            name = Some(&t[1..2 + id.len()]);
            t = rest;
        }
    }
    let (n, mut t) = run_of(t.strip_prefix(' ')?, NUM);
    if n.is_empty() {
        return None;
    }
    let plus = glue_part(" plus ", &mut t);
    let minus = glue_part(" minus ", &mut t);
    Some(GlueM { name, nat: f(n)?, plus, minus })
}

/// `key N[fil*]` at the start of `t`, consumed.
fn glue_part<'s>(key: &str, t: &mut &'s str) -> Option<(f64, &'s str)> {
    let s = t.strip_prefix(key)?;
    let (v, s) = run_of(s, NUM);
    if v.is_empty() {
        return None;
    }
    let (o, s) = fil(s, false);
    let v = v.parse().ok()?;
    *t = s;
    Some((v, o))
}

fn glue_amount(g: &GlueM, set: &(i32, f64, String)) -> f64 {
    let (sign, ratio, order) = set;
    let mut nat = g.nat;
    if *ratio != 0.0 {
        match (sign, g.plus, g.minus) {
            (1, Some((p, o)), _) if o == order => nat += ratio * p,
            (-1, _, Some((m, o))) if o == order => nat -= ratio * m,
            _ => {}
        }
    }
    nat
}

/// `\\[A-Z0-9]+/[^ ]+ (.)`: the character of a glyph line.
fn char_of(t: &str) -> Option<char> {
    let t = t.strip_prefix('\\')?;
    let (id, t) = run_of(t, "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
    let t = t.strip_prefix('/').filter(|_| !id.is_empty())?;
    let sp = t.find(' ').filter(|&p| p > 0)?;
    t[sp + 1..].chars().next()
}

/// Walks a vlist from `top` (pt below the page origin): appends (baseline, text, recipe)
/// for every hbox with characters, `recipe` being the vertical items since the last one.
fn box_lines(node: &BoxNode, top: f64, out: &mut Vec<(f64, String, Vec<String>)>, recipe: &mut Vec<String>) {
    let Some(m) = box_head(&node.text) else { return };
    let mut y = top;
    for c in &node.children {
        let t = c.text.as_str();
        if let Some(b) = box_head(t) {
            if b.vertical {
                box_lines(c, y, out, recipe);
            } else {
                let text = hbox_text(c);
                if !text.is_empty() {
                    let mut r = std::mem::take(recipe);
                    r.push(format!("box h{} d{}", py_g(b.h), py_g(b.d)));
                    out.push((y + b.h, text, r));
                }
                inner_vboxes(c, y + b.h, out, recipe);
            }
            y += b.h + b.d;
        } else if let Some(g) = glue_m(t) {
            let a = glue_amount(&g, &m.set);
            y += a;
            recipe.push(format!("{} {}", g.name.unwrap_or("glue"), py_g(a)));
        } else if let Some(k) = t.strip_prefix("\\kern").map(|k| k.strip_prefix(' ').unwrap_or(k)).and_then(|k| run_of(k, NUM).0.parse::<f64>().ok()) {
            y += k;
            recipe.push(format!("kern {}", py_g(k)));
        } else if let Some(r) = rule_dims(t) {
            y += r;
            recipe.push("rule".into());
        } else if t.starts_with("\\penalty") {
            recipe.push(t[1..].to_string());
        }
    }
}

/// `RULE_RE`: the height plus depth of `\rule(H+D)`, `*` counting nothing.
fn rule_dims(t: &str) -> Option<f64> {
    let t = t.strip_prefix("\\rule(")?;
    let (h, t) = run_of(t, "-0123456789.*");
    let (d, t) = run_of(t.strip_prefix('+')?, "-0123456789.*");
    t.strip_prefix(')')?;
    if h.is_empty() || d.is_empty() {
        return None;
    }
    let v = |s: &str| if s == "*" { Some(0.0) } else { f(s) };
    Some(v(h)? + v(d)?)
}

/// Lines of the vboxes inside an hbox at any depth (tabulars, minipages, columns).
fn inner_vboxes(hbox: &BoxNode, baseline: f64, out: &mut Vec<(f64, String, Vec<String>)>, recipe: &mut Vec<String>) {
    for v in &hbox.children {
        let Some(b) = box_head(&v.text) else { continue };
        let shift = b.shift.unwrap_or(0.0);
        if b.vertical {
            box_lines(v, baseline + shift - b.h, out, recipe);
        } else {
            inner_vboxes(v, baseline + shift, out, recipe);
        }
    }
}

/// Glyphs of an hbox, nested hboxes included (display formulas, labels).
fn hbox_text(node: &BoxNode) -> String {
    let mut out = String::new();
    for c in &node.children {
        if let Some(ch) = char_of(&c.text) {
            out.push(ch);
        } else if c.text.starts_with("\\hbox") {
            out += &hbox_text(c);
        }
    }
    out
}

/// The first of `vs` nearest to `y` (Python's `min` with a key).
fn nearest(vs: impl IntoIterator<Item = f64>, y: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    for v in vs {
        if best.is_none_or(|b| (v - y).abs() < (b - y).abs()) {
            best = Some(v);
        }
    }
    best
}

fn cmd_boxes(o: &Opts) {
    let (tex, page) = doc_page(o, "boxes");
    let r = guarded(|| {
        let pages = box_pages(&box_log(&tex)?);
        if page == 0 || page > pages.len() {
            die(&format!("{} has {} shipped pages", stem(&tex), pages.len()));
        }
        let mut tex_lines = Vec::new();
        box_lines(&pages[page - 1], 0.0, &mut tex_lines, &mut Vec::new());
        let (a, b) = both_glyphs(&ref_pdf(&tex)?, &ours_pdf(&tex)?)?;
        let ref_lines = baselines(a.get(page - 1).map(Vec::as_slice).unwrap_or(&[]), 0.0);
        let ours_y: Vec<f64> = baselines(b.get(page - 1).map(Vec::as_slice).unwrap_or(&[]), 0.0).iter().map(|l| l.0).collect();
        let mut shown = 0;
        for (y_pt, text, recipe) in &tex_lines {
            let y = (72.27 + y_pt) * PT_TO_BP;
            let ref_y = nearest(ref_lines.iter().map(|r| r.0), y).unwrap_or(y);
            let dy = nearest(ours_y.iter().copied(), ref_y).map_or(f64::NAN, |m| m - ref_y);
            if o.all || dy.is_nan() || dy.abs() > TOL {
                println!("{ref_y:8.2} {}  {:<40}  <- {}", signed(dy, 7), head(text, 40), recipe.join(", "));
                shown += 1;
            }
        }
        if shown == 0 {
            println!("page {page}: every TeX line within {TOL}pt");
        }
        Ok(())
    });
    if let Err(e) = r {
        die(&e);
    }
}

// ---- repro -----------------------------------------------------------------------------------------

fn preamble(tex: &Path) -> Vec<String> {
    let src = String::from_utf8_lossy(&std::fs::read(tex).unwrap_or_default()).into_owned();
    let lines = py_splitlines(&src);
    match lines.iter().position(|l| l.trim_start().starts_with("\\begin{document}")) {
        Some(i) => lines[..i].to_vec(),
        None => die(&format!("no \\begin{{document}} in {}", tex.display())),
    }
}

fn cmd_repro(o: &Opts) {
    if o.build {
        if let Err(e) = build() {
            die(&e);
        }
    }
    let doc = o.docs.first().unwrap_or_else(|| die("repro needs DOC"));
    let tex = source(doc).unwrap_or_else(|e| die(&e));
    let (a, b): (Option<usize>, Option<usize>) = (o.docs.get(1).and_then(|x| x.parse().ok()), o.docs.get(2).and_then(|x| x.parse().ok()));
    let body: Vec<String> = if let Some(body) = o.flags.get("body") {
        py_splitlines(body)
    } else if let (Some(a), Some(b)) = (a.filter(|&a| a > 0), b.filter(|&b| b > 0)) {
        let lines = py_splitlines(&String::from_utf8_lossy(&std::fs::read(&tex).unwrap_or_default()));
        lines[(a - 1).min(lines.len())..b.min(lines.len()).max((a - 1).min(lines.len()))].to_vec()
    } else {
        die("repro needs a line range A B or --body")
    };
    let d = out_dir().join("repro");
    let _ = std::fs::create_dir_all(&d);
    let name = o.get("name").filter(|n| !n.is_empty()).map_or_else(|| format!("{}_{}", stem(&tex), a.filter(|&a| a > 0).map_or("body".to_string(), |a| a.to_string())), str::to_string);
    let src = d.join(format!("{name}.tex"));
    let pre = preamble(&tex);
    let doc_of = |begin: &str, body: &[String]| -> String {
        let mut v = pre.clone();
        v.push(begin.to_string());
        v.extend(body.iter().cloned());
        v.push("\\end{document}".into());
        v.push(String::new());
        v.join("\n")
    };
    let _ = std::fs::write(&src, doc_of("\\begin{document}", &body));
    let trace = o.get("trace");
    let mut tex_src = src.clone();
    if let Some(tr) = trace {
        // TeX-only tracing goes in a copy, so rushtex never sees the primitives.
        let hook = match tr {
            "pages" => r"\tracingpages=1 \tracingonline=1",
            "lists" => r"\showboxdepth=3 \showboxbreadth=100 \tracingonline=1",
            _ => die("--trace pages|lists"),
        };
        let mut body_t = body.clone();
        if tr == "lists" {
            body_t.push("\\showlists".into());
        }
        tex_src = d.join(format!("{name}_trace.tex"));
        let _ = std::fs::write(&tex_src, doc_of(&format!("\\begin{{document}}{hook}"), &body_t));
    }
    let r = guarded(|| {
        // `\showlists` stops as an error: let TeX go on past it.
        let refp = run_pdflatex(&tex_src, &d, 1, trace != Some("lists"))?;
        let ours = d.join(format!("{name}_ours.pdf"));
        run_rushtex(&src, &ours)?;
        let (ga, gb) = both_glyphs(&refp, &ours)?;
        println!("{}  pages ref {} ours {}", src.display(), ga.len(), gb.len());
        for p in 1..=ga.len().min(gb.len()) {
            println!("-- page {p}");
            print_page_lines(&ga, &gb, p, o.all)?;
        }
        if let Some(tr) = trace {
            let log = py_splitlines(&String::from_utf8_lossy(&std::fs::read(d.join(format!("{}.log", stem(&tex_src)))).unwrap_or_default()));
            if tr == "pages" {
                println!("-- TeX\n{}", log.iter().filter(|l| l.starts_with('%')).cloned().collect::<Vec<_>>().join("\n"));
            } else if let Some(s) = log.iter().position(|l| l.starts_with("### vertical mode entered at line 0")) {
                let e = (s..log.len()).find(|&i| log[i].starts_with("prevdepth")).unwrap_or(log.len() - 1);
                println!("{}", log[s..=e].join("\n"));
            }
        }
        Ok(())
    });
    if let Err(e) = r {
        die(&e);
    }
}

// ---- tj --------------------------------------------------------------------------------------------

/// Text operators (TJ arrays, Td moves) of one page in stream order, font names folded to
/// `/F`: Python's `re.findall(r"\[[^\]]*\]TJ|-?[\d.]+ -?[\d.]+ Td", data)` after
/// `re.sub(r"/F\w+ ", "/F ", data)`.
fn text_ops(pdf: &Path, page: usize) -> Result<Vec<String>, String> {
    let p = Pdf::open(pdf)?;
    if page == 0 || page > p.pages.len() {
        return Err(format!("{}: no page {page}", pdf.display()));
    }
    let data: Vec<char> = latin1(&page_content(&p, page - 1)).chars().collect();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut s = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] == '/' && data.get(i + 1) == Some(&'F') {
            let mut j = i + 2;
            while j < data.len() && word(data[j]) {
                j += 1;
            }
            if j > i + 2 && data.get(j) == Some(&' ') {
                s.extend(['/', 'F', ' ']);
                i = j + 1;
                continue;
            }
        }
        s.push(data[i]);
        i += 1;
    }
    let digit = |c: char| c.is_ascii_digit() || c == '.';
    // `-?[\d.]+ ` from `k`: the index after the space.
    let num_sp = |k: usize| -> Option<usize> {
        let mut k = k + usize::from(s.get(k) == Some(&'-'));
        let st = k;
        while k < s.len() && digit(s[k]) {
            k += 1;
        }
        (k > st && s.get(k) == Some(&' ')).then_some(k + 1)
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s[i] == '[' {
            if let Some(j) = (i + 1..s.len()).find(|&j| s[j] == ']') {
                if s.get(j + 1) == Some(&'T') && s.get(j + 2) == Some(&'J') {
                    out.push(s[i..j + 3].iter().collect::<String>().trim().to_string());
                    i = j + 3;
                    continue;
                }
            }
        } else if let Some(e) = num_sp(i).and_then(num_sp) {
            // The second number ended in a space; it must be followed by `Td`.
            if s.get(e) == Some(&'T') && s.get(e + 1) == Some(&'d') {
                out.push(s[i..e + 2].iter().collect::<String>().trim().to_string());
                i = e + 2;
                continue;
            }
        }
        i += 1;
    }
    Ok(out)
}

fn cmd_tj(o: &Opts) {
    let (tex, page) = doc_page(o, "tj");
    let width: usize = o.get("width").and_then(|w| w.parse().ok()).unwrap_or(700);
    let r = guarded(|| {
        let (a, b) = (text_ops(&ref_pdf(&tex)?, page)?, text_ops(&ours_pdf(&tex)?, page)?);
        for (tag, i1, i2, j1, j2) in SequenceMatcher::new(&a, &b).opcodes() {
            if tag != Tag::Equal {
                println!("ref  {}", head(&a[i1..i2].join(" | "), width));
                println!("ours {}", head(&b[j1..j2].join(" | "), width));
                println!();
            }
        }
        Ok(())
    });
    if let Err(e) = r {
        die(&e);
    }
}

// ---- hlist: tex.web §619 `hlist_out` replayed on the box log ---------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(a: &str, b: &str) -> Vec<(Tag, usize, usize, usize, usize)> {
        let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
        SequenceMatcher::new(&a, &b).opcodes()
    }

    /// Opcodes as CPython 3's `difflib.SequenceMatcher(None, a, b, autojunk=False)` gives them.
    #[test]
    fn sequence_matcher_is_difflib() {
        use Tag::*;
        assert_eq!(ops("abxcd", "abcd"), [(Equal, 0, 2, 0, 2), (Delete, 2, 3, 2, 2), (Equal, 3, 5, 2, 4)]);
        assert_eq!(ops("qabxcd", "abycdf"), [(Delete, 0, 1, 0, 0), (Equal, 1, 3, 0, 2), (Replace, 3, 4, 2, 3), (Equal, 4, 6, 3, 5), (Insert, 6, 6, 5, 6)]);
        assert_eq!(ops("private Thread currentThread;", "private volatile Thread currentThread;"), [(Equal, 0, 6, 0, 6), (Insert, 6, 6, 6, 15), (Equal, 6, 29, 15, 38)]);
        assert_eq!(ops("", "abc"), [(Insert, 0, 0, 0, 3)]);
        assert_eq!(ops("abc", ""), [(Delete, 0, 3, 0, 0)]);
        assert_eq!(ops("aaaabbbbaaaa", "aabbbbbaaa"), [(Equal, 0, 2, 0, 2), (Replace, 2, 4, 2, 3), (Equal, 4, 11, 3, 10), (Delete, 11, 12, 10, 10)]);
        assert_eq!(ops("the quick brown fox", "the slow brown cat"), [(Equal, 0, 4, 0, 4), (Replace, 4, 9, 4, 8), (Equal, 9, 16, 8, 15), (Replace, 16, 19, 15, 18)]);
    }

    #[test]
    fn python_formatting() {
        assert_eq!(py_g(12.0), "12");
        assert_eq!(py_g(0.5), "0.5");
        assert_eq!(py_g(1.0e-5), "1e-05");
        assert_eq!(py_g(1234567.0), "1.23457e+06");
        assert_eq!(py_g(1.23456789), "1.23457");
        assert_eq!(py_g(-2.5), "-2.5");
        assert_eq!(signed(f64::NAN, 7), "   +nan");
        assert_eq!(py_repr("it's"), "\"it's\"");
        assert_eq!(py_splitlines("a\r\nb\x0cc\n"), ["a", "b", "c"]);
    }

    #[test]
    fn box_log_lines() {
        let b = box_head(r"\hbox(6.94+1.94)x345.0, glue set - 0.5fil, shifted 2.0").unwrap();
        assert_eq!((b.vertical, b.h, b.d, b.set.0, b.set.1, b.set.2.as_str(), b.shift), (false, 6.94, 1.94, -1, 0.5, "fil", Some(2.0)));
        let g = glue_m(r"\glue(\baselineskip) 1.94 plus 1.0fill minus 0.5").unwrap();
        assert_eq!((g.name, g.nat, g.plus, g.minus), (Some(r"\baselineskip"), 1.94, Some((1.0, "fill")), Some((0.5, ""))));
        assert!(glue_m(r"\glue(\nonscript)").is_none());
        assert_eq!(char_of(r"\OT1/cmr/m/n/10 A"), Some('A'));
        assert!(glyph_nl(r"..\OT1/cmr/m/n/10 "));
        assert_eq!(hbox_head(r"\hbox(6.94+0.0)x20.0, glue set 0.5fil"), Some(("20.0", "fil")));
        assert_eq!(rule_dims(r"\rule(0.4+*)x345.0"), Some(0.4));
    }
}

// ---- incr: the edit-fuzz test of the watch session ------------------------------------------
//
// `incr DOC.. [--edits N] [--seed S] [-v]`: the document's folder is copied to
// `target/parity/incr/<name>`, a watch session is started on it (`rushtex --watch-replay`,
// the compile `--watch` makes, with the state it keeps between compiles), and N random but
// realistic edits are made, one save each; after each save the session's PDF is compared byte
// for byte with a fresh one-shot compile of the same files. The edits are drawn from the seed,
// so a run is reproduced exactly; the first difference stops the run with the edit, its
// source lines, where the PDFs first differ and whether a two-save history reproduces it.

/// splitmix64: deterministic for a seed, and no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn word(&mut self) -> &'static str {
        INCR_WORDS[self.below(INCR_WORDS.len())]
    }
}

/// The words the edits type: plain, of various lengths, so lines break and hyphenate anew.
const INCR_WORDS: [&str; 24] = [
    "river", "garden", "theory", "measure", "silver", "window", "pattern", "careful", "number", "evening", "distant", "paper", "quietly", "structure", "an", "of", "independently", "results", "simple", "argument", "considerable", "light", "table", "nevertheless",
];

/// Environments whose text the edits leave alone: pictures, displayed mathematics and the
/// bibliography, where a word or a paragraph is not running text.
const INCR_SKIP_ENVS: [&str; 16] = ["tikzpicture", "picture", "equation", "align", "gather", "multline", "flalign", "alignat", "eqnarray", "displaymath", "math", "thebibliography", "filecontents", "comment", "array", "verbatim"];

/// What a removed paragraph may not hold: structure, definitions and labels, whose removal
/// is not an edit a writer makes in passing.
const INCR_KEEP: [&str; 21] = [
    "\\begin", "\\end", "\\item", "section", "\\chapter", "\\part", "\\label", "\\caption", "\\maketitle", "\\bibliograph", "\\appendix", "\\def", "command", "\\input", "\\include", "\\tableofcontents", "\\new", "\\let", "\\make", "matter", "\\use",
];

/// The places an edit can touch in one file's text: words of running text, the starts of
/// paragraphs (just after a blank line) and whole paragraphs that can go.
#[derive(Default)]
struct Spots {
    words: Vec<(usize, usize)>,
    breaks: Vec<usize>,
    paras: Vec<(usize, usize)>,
}

fn find_from(t: &[u8], pat: &[u8], from: usize) -> Option<usize> {
    t.get(from..)?.windows(pat.len()).position(|w| w == pat).map(|p| p + from)
}

/// The part of a file the edits touch: the main file's body, or all of an `\input` file.
fn body_of(t: &[u8], main: bool) -> (usize, usize) {
    if !main {
        return (0, t.len());
    }
    let s = find_from(t, b"\\begin{document}", 0).map_or(0, |p| p + 16);
    let e = t.windows(14).rposition(|w| w == b"\\end{document}").filter(|&e| e >= s).unwrap_or(t.len());
    (s, e)
}

/// Words at brace depth 0, outside brackets, mathematics, comments and the environments of
/// [`INCR_SKIP_ENVS`]; blank lines in the same state; and the paragraphs between two such
/// blank lines that hold none of [`INCR_KEEP`].
fn spots(t: &[u8], (s, e): (usize, usize)) -> Spots {
    let mut sp = Spots::default();
    let (mut depth, mut bracket, mut math, mut skip) = (0usize, 0usize, false, 0usize);
    let mut i = s;
    let mut last_break: Option<usize> = None;
    while i < e {
        match t[i] {
            b'%' => {
                while i < e && t[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'\\' => {
                let n = t[i + 1..e].iter().take_while(|c| c.is_ascii_alphabetic()).count();
                if n == 0 {
                    match t.get(i + 1) {
                        Some(b'(' | b'[') => math = true,
                        Some(b')' | b']') => math = false,
                        _ => {}
                    }
                    i += 2;
                    continue;
                }
                let name = &t[i + 1..i + 1 + n];
                i += 1 + n;
                if (name == b"begin" || name == b"end") && t.get(i) == Some(&b'{') {
                    let close = find_from(t, b"}", i).unwrap_or(e).min(e);
                    let env = String::from_utf8_lossy(&t[i + 1..close]).trim_end_matches('*').to_string();
                    if INCR_SKIP_ENVS.contains(&env.as_str()) {
                        skip = if name == b"begin" { skip + 1 } else { skip.saturating_sub(1) };
                    }
                    i = close + 1;
                }
                continue;
            }
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b'[' => bracket += 1,
            b']' => bracket = bracket.saturating_sub(1),
            b'$' => {
                math = !math;
                if t.get(i + 1) == Some(&b'$') {
                    i += 1;
                }
            }
            b'\n' => {
                bracket = 0;
                if t.get(i + 1) == Some(&b'\n') && i + 2 <= e {
                    let ok = depth == 0 && !math && skip == 0;
                    let p = i + 2;
                    if ok {
                        if let Some(b) = last_break {
                            let para = &t[b..p];
                            if para.iter().any(u8::is_ascii_alphabetic) && !INCR_KEEP.iter().any(|k| para.windows(k.len()).any(|w| w == k.as_bytes())) {
                                sp.paras.push((b, p));
                            }
                        }
                        sp.breaks.push(p);
                    }
                    last_break = ok.then_some(p);
                }
            }
            c if c.is_ascii_alphabetic() => {
                let n = t[i..e].iter().take_while(|c| c.is_ascii_alphabetic()).count();
                if n >= 3 && depth == 0 && bracket == 0 && !math && skip == 0 {
                    sp.words.push((i, i + n));
                }
                i += n;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    sp
}

/// The document's files as the edits change them: the main file first, then the files its
/// body reads by `\input{..}` or `\include{..}` that are found beside it.
struct IncrTree {
    names: Vec<String>,
    texts: Vec<Vec<u8>>,
    /// The first compile ran natively: a label edit then refers back to its label from a
    /// later page (a `\pageref` to a page not yet shipped sends the native job to the engine).
    native: bool,
}

impl IncrTree {
    fn load(dir: &Path, main: &str) -> Result<IncrTree, String> {
        let text = std::fs::read(dir.join(main)).map_err(|e| format!("{main}: {e}"))?;
        let mut tree = IncrTree { names: vec![main.to_string()], texts: vec![text], native: false };
        let (s, e) = body_of(&tree.texts[0], true);
        let body = tree.texts[0][s..e].to_vec();
        for pat in [&b"\\input{"[..], b"\\include{"] {
            let mut at = 0;
            while let Some(p) = find_from(&body, pat, at) {
                at = p + pat.len();
                let Some(close) = find_from(&body, b"}", at) else { break };
                let name = String::from_utf8_lossy(&body[at..close]).trim().to_string();
                let file = if name.ends_with(".tex") { name } else { format!("{name}.tex") };
                if !tree.names.contains(&file) {
                    if let Ok(t) = std::fs::read(dir.join(&file)) {
                        tree.names.push(file);
                        tree.texts.push(t);
                    }
                }
            }
        }
        Ok(tree)
    }
}

/// The line of byte `at`, from 1.
fn line_at(t: &[u8], at: usize) -> usize {
    1 + t[..at.min(t.len())].iter().filter(|&&c| c == b'\n').count()
}

/// A paragraph of `sentences` sentences of [`INCR_WORDS`], then a blank line.
fn incr_paragraph(rng: &mut Rng, sentences: usize) -> Vec<u8> {
    let mut p = String::new();
    for _ in 0..sentences {
        let n = 6 + rng.below(9);
        let words: Vec<&str> = (0..n).map(|_| rng.word()).collect();
        let s = words.join(" ");
        let _ = write!(p, "{}{}. ", s[..1].to_ascii_uppercase(), &s[1..]);
    }
    let mut v = p.trim_end().as_bytes().to_vec();
    v.extend_from_slice(b"\n\n");
    v
}

/// The kinds of edit, each with its weight in the draw.
const INCR_KINDS: [(&str, usize); 13] = [("insert word", 10), ("delete word", 8), ("replace word", 10), ("add paragraph", 8), ("remove paragraph", 6), ("section title", 7), ("input file", 10), ("several spots", 10), ("page", 8), ("title", 4), ("label", 8), ("undo", 6), ("save unchanged", 3)];

/// The edits a save in several spots, or in an `\input` file, is made of.
const INCR_SMALL: [&str; 4] = ["insert word", "delete word", "replace word", "add paragraph"];

/// One edit of kind `kind` in file `f`: what it did, or `None` when the file has no place
/// for it.
fn incr_edit(tree: &mut IncrTree, f: usize, kind: &str, rng: &mut Rng) -> Option<String> {
    let native = tree.native;
    let t = &mut tree.texts[f];
    let name = &tree.names[f];
    let range = body_of(t, f == 0);
    let sp = spots(t, range);
    fn pick<T: Copy>(v: &[T], rng: &mut Rng) -> Option<T> {
        (!v.is_empty()).then(|| v[rng.below(v.len())])
    }
    match kind {
        "insert word" => {
            let (_, e) = pick(&sp.words, rng)?;
            let w = rng.word();
            t.splice(e..e, format!(" {w}").bytes());
            Some(format!("insert word '{w}' at {name}:{}", line_at(t, e)))
        }
        "delete word" => {
            let (s, mut e) = pick(&sp.words, rng)?;
            let w = latin1(&t[s..e]);
            if t.get(e) == Some(&b' ') {
                e += 1;
            }
            t.drain(s..e);
            Some(format!("delete word '{w}' at {name}:{}", line_at(t, s)))
        }
        "replace word" => {
            let (s, e) = pick(&sp.words, rng)?;
            let old = latin1(&t[s..e]);
            let w = rng.word();
            t.splice(s..e, w.bytes());
            Some(format!("replace '{old}' by '{w}' at {name}:{}", line_at(t, s)))
        }
        "add paragraph" => {
            let p = pick(&sp.breaks, rng)?;
            let n = 2 + rng.below(5);
            t.splice(p..p, incr_paragraph(rng, n));
            Some(format!("add a paragraph of {n} sentences at {name}:{}", line_at(t, p)))
        }
        "remove paragraph" => {
            let (s, e) = pick(&sp.paras, rng)?;
            let (at, lines) = (line_at(t, s), line_at(t, e) - line_at(t, s));
            t.drain(s..e);
            Some(format!("remove the paragraph of {lines} lines at {name}:{at}"))
        }
        "section title" | "title" => {
            let heads: &[&str] = if kind == "title" { &["\\title{", "\\author{", "\\date{"] } else { &["\\section{", "\\section*{", "\\subsection{", "\\subsection*{", "\\subsubsection{", "\\chapter{", "\\chapter*{", "\\paragraph{"] };
            let (from, to) = if kind == "title" { (0, t.len()) } else { range };
            let mut at = Vec::new();
            for h in heads {
                let mut i = from;
                while let Some(p) = find_from(&t[..to], h.as_bytes(), i) {
                    at.push(p + h.len());
                    i = p + h.len();
                }
            }
            at.sort_unstable();
            let p = pick(&at, rng)?;
            // A word this test put in is taken out again half of the time.
            if let Some(w) = INCR_WORDS.iter().find(|w| t[p..].starts_with(format!("{w} ").as_bytes())).filter(|_| rng.below(2) == 0) {
                t.drain(p..p + w.len() + 1);
                return Some(format!("take '{w}' out of the heading at {name}:{}", line_at(t, p)));
            }
            let w = rng.word();
            t.splice(p..p, format!("{w} ").bytes());
            Some(format!("put '{w}' into the heading at {name}:{}", line_at(t, p)))
        }
        "label" => {
            // A section with a label and a reference to it (the `.aux` changes: a resumed
            // run must read the new one), or one this test put in taken out again with its
            // reference. In a native document the reference is a paragraph of its own some
            // pages on (a backward reference); else it follows the label (forward to the
            // label's page, which the engine path resolves from the `.aux`).
            const HEAD: &[u8] = b"\\section{Labelled ";
            let added: Vec<usize> = sp.breaks.iter().copied().filter(|&b| t[b..].starts_with(HEAD)).collect();
            if !added.is_empty() && rng.below(2) == 0 {
                let p = pick(&added, rng)?;
                let e = find_from(t, b"\n\n", p)? + 2;
                let key = find_from(&t[..e], b"\\label{", p).map(|l| t[l + 7..].iter().take_while(|&&c| c != b'}').copied().collect::<Vec<u8>>());
                t.drain(p..e);
                if let Some(r) = key.and_then(|k| find_from(t, &[&b"See Section~\\ref{"[..], &k, b"}"].concat(), 0)) {
                    let e = find_from(t, b"\n\n", r).map_or(t.len(), |e| e + 2);
                    t.drain(r..e);
                }
                return Some(format!("remove a labelled section at {name}:{}", line_at(t, p)));
            }
            let p = pick(&sp.breaks, rng)?;
            let (w, k) = (rng.word(), rng.below(1_000_000));
            let refer = format!("See Section~\\ref{{incr-{k}}} on page~\\pageref{{incr-{k}}}.\n\n");
            // About three pages of source past the label, so its page has shipped.
            let later = sp.breaks.iter().copied().find(|&b| b >= p + 12_000).filter(|_| native);
            if let Some(q) = later {
                t.splice(q..q, refer.bytes());
                t.splice(p..p, format!("\\section{{Labelled {w}}}\\label{{incr-{k}}}\n\n").bytes());
                return Some(format!("add a labelled section incr-{k} at {name}:{} and a reference back to it", line_at(t, p)));
            }
            if native {
                t.splice(p..p, format!("\\section{{Labelled {w}}}\\label{{incr-{k}}}\nSee Section~\\ref{{incr-{k}}}.\n\n").bytes());
            } else {
                t.splice(p..p, format!("\\section{{Labelled {w}}}\\label{{incr-{k}}}\n{refer}").bytes());
            }
            Some(format!("add a labelled section incr-{k} at {name}:{}", line_at(t, p)))
        }
        "page" => {
            const MARK: &[u8] = b"\\clearpage\n\n";
            let marks: Vec<usize> = sp.breaks.iter().copied().filter(|&b| t[b..].starts_with(MARK)).collect();
            if !marks.is_empty() && rng.below(2) == 0 {
                let p = pick(&marks, rng)?;
                t.drain(p..p + MARK.len());
                return Some(format!("remove a \\clearpage at {name}:{}", line_at(t, p)));
            }
            let p = pick(&sp.breaks, rng)?;
            if rng.below(3) == 0 {
                // A long paragraph: more than half a page.
                t.splice(p..p, incr_paragraph(rng, 24));
                return Some(format!("add a long paragraph (24 sentences) at {name}:{}", line_at(t, p)));
            }
            t.splice(p..p, MARK.iter().copied());
            Some(format!("add a \\clearpage at {name}:{}", line_at(t, p)))
        }
        _ => None,
    }
}

/// The watch session: `rushtex --watch-replay` on the copied folder, a line per save.
struct ReplaySession {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: std::io::BufReader<std::process::ChildStdout>,
}

impl ReplaySession {
    fn start(rush: &Path, work: &Path, main: &str, log: &Path) -> Result<ReplaySession, String> {
        let log = std::fs::File::create(log).map_err(|e| format!("{}: {e}", log.display()))?;
        let mut child = Command::new(rush).arg("--watch-replay").arg(main).args(["-o", INCR_WATCH_PDF]).current_dir(work).envs(FIXED_DATE).env("RT_NATIVE_WHY", "1").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(log).spawn().map_err(|e| format!("{}: {e}", rush.display()))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = std::io::BufReader::new(child.stdout.take().ok_or("no stdout")?);
        Ok(ReplaySession { child, stdin, stdout })
    }
    /// One save, compiled by the session.
    fn save(&mut self) -> Result<(), String> {
        use std::io::{BufRead as _, Write as _};
        writeln!(self.stdin, "save").and_then(|()| self.stdin.flush()).map_err(|e| format!("the watch session ended: {e}"))?;
        let mut line = String::new();
        self.stdout.read_line(&mut line).map_err(|e| e.to_string())?;
        if line.trim() != "compiled" {
            return Err("the watch session ended (see its log)".into());
        }
        Ok(())
    }
}

impl Drop for ReplaySession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

const INCR_WATCH_PDF: &str = "incr-watch.pdf";
const INCR_FRESH_PDF: &str = "incr-fresh.pdf";

/// The files of `tree` written to `dir`: those that differ from `old`, or all.
fn write_tree(dir: &Path, tree: &IncrTree, old: Option<&[Vec<u8>]>) {
    for (i, (n, t)) in tree.names.iter().zip(&tree.texts).enumerate() {
        if old.is_none_or(|o| o[i] != *t) {
            let _ = std::fs::write(dir.join(n), t);
        }
    }
}

/// A fresh one-shot compile of `main` in `work`: its PDF, or `None` when it wrote none.
fn incr_fresh(rush: &Path, work: &Path, main: &str) -> Option<Vec<u8>> {
    let _ = std::fs::remove_file(work.join(INCR_FRESH_PDF));
    let _ = Command::new(rush).arg(main).args(["-o", INCR_FRESH_PDF]).current_dir(work).envs(FIXED_DATE).stdout(Stdio::null()).stderr(Stdio::null()).status();
    std::fs::read(work.join(INCR_FRESH_PDF)).ok()
}

fn cmd_incr(o: &Opts) {
    let rush = std::env::var_os("RUSHTEX_BIN").map_or_else(rushtex, PathBuf::from);
    if o.build {
        build().unwrap_or_else(|e| die(&e));
    }
    if o.docs.is_empty() {
        die("usage: parity-testing incr DOC.. [--edits N] [--seed S] [-v]");
    }
    let edits: usize = o.get("edits").map_or(20, |v| v.parse().unwrap_or_else(|_| die("--edits N")));
    let seed: u64 = o.get("seed").map_or(1, |v| v.parse().unwrap_or_else(|_| die("--seed S")));
    let mut failed = 0;
    for d in &o.docs {
        let tex = source(d).unwrap_or_else(|e| die(&e));
        match incr_doc(&rush, &tex, edits, seed, o.has("v")) {
            Ok(line) => println!("{line}"),
            Err(report) => {
                println!("{report}");
                failed += 1;
            }
        }
    }
    if failed > 0 {
        std::process::exit(1);
    }
}

/// One document through `edits` saves: a line saying all were identical, or the report of
/// the first that was not.
fn incr_doc(rush: &Path, tex: &Path, edits: usize, seed: u64, verbose: bool) -> Result<String, String> {
    // The compiles run in the copied folder.
    let rush = &std::fs::canonicalize(rush).unwrap_or_else(|_| rush.to_path_buf());
    let main = tex.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let dir = tex.parent().unwrap_or(Path::new("."));
    // A template's file is `main.tex`: its folder names it.
    let name = if main == "main.tex" { dir.file_name().map_or_else(|| stem(tex), |d| d.to_string_lossy().into_owned()) } else { stem(tex) };
    let base = out_dir().join("incr");
    let work = base.join(&name);
    if base.starts_with(dir) {
        return Err(format!("FAIL {name}: its folder {} holds the test's own folder; give a document in a folder of its own", dir.display()));
    }
    let _ = std::fs::remove_dir_all(&work);
    copy_tree(dir, &work).map_err(|e| format!("FAIL {name}: copying {}: {e}", dir.display()))?;
    let mut tree = IncrTree::load(&work, &main)?;
    let log = base.join(format!("{name}.log"));
    // A binary just linked takes a few hundred ms more on its first start (the system checks
    // it): one compile first, so the session's first save is not charged for it.
    let _ = incr_fresh(rush, &work, &main);
    let mut session = ReplaySession::start(rush, &work, &main, &log)?;
    let mut rng = Rng(seed ^ 0x5eed_0000_0000_0000);
    let mut history: Vec<Vec<Vec<u8>>> = Vec::new();
    // Wall time of the saves after the first compile, and of the first compile.
    let (mut watch_s, mut fresh_s) = (0f64, 0f64);
    let (mut first_w, mut first_f) = (0f64, 0f64);
    let total: usize = INCR_KINDS.iter().map(|k| k.1).sum();
    for step in 0..=edits {
        let before = tree.texts.clone();
        // Save 0 is the first compile; each later one follows an edit.
        let mut what = None;
        for _ in 0..32 {
            if step == 0 {
                what = Some("the first compile".to_string());
                break;
            }
            let mut r = rng.below(total);
            let kind = INCR_KINDS.iter().find(|k| r < k.1 || { r -= k.1; false }).map_or("insert word", |k| k.0);
            what = match kind {
                "undo" if history.len() > 1 => {
                    let back = 1 + rng.below(3.min(history.len() - 1));
                    tree.texts = history[history.len() - 1 - back].clone();
                    Some(format!("undo the last {back} saves"))
                }
                "save unchanged" => Some("save with nothing changed".to_string()),
                "input file" if tree.names.len() > 1 => {
                    let f = 1 + rng.below(tree.names.len() - 1);
                    let k = INCR_SMALL[rng.below(INCR_SMALL.len())];
                    incr_edit(&mut tree, f, k, &mut rng)
                }
                "several spots" => {
                    let n = 2 + rng.below(3);
                    let done: Vec<String> = (0..n)
                        .filter_map(|_| {
                            let f = rng.below(tree.names.len());
                            let k = INCR_SMALL[rng.below(INCR_SMALL.len())];
                            incr_edit(&mut tree, f, k, &mut rng)
                        })
                        .collect();
                    (!done.is_empty()).then(|| format!("{} spot{} in one save: {}", done.len(), if done.len() == 1 { "" } else { "s" }, done.join("; ")))
                }
                "undo" | "input file" => None,
                k => incr_edit(&mut tree, 0, k, &mut rng),
            };
            if what.is_some() {
                break;
            }
        }
        let what = what.unwrap_or_else(|| "save with nothing changed".to_string());
        write_tree(&work, &tree, Some(&before));
        if what.starts_with("save with nothing") {
            let _ = std::fs::write(work.join(&main), &tree.texts[0]);
        }
        let _ = std::fs::remove_file(work.join(INCR_WATCH_PDF));
        let t0 = std::time::Instant::now();
        session.save().map_err(|e| format!("FAIL {name}: save {step}: {e} ({})", log.display()))?;
        watch_s += t0.elapsed().as_secs_f64();
        if step == 0 {
            tree.native = std::fs::read_to_string(&log).is_ok_and(|l| l.contains("native job accepted"));
        }
        let watch = std::fs::read(work.join(INCR_WATCH_PDF)).ok();
        let t1 = std::time::Instant::now();
        let fresh = incr_fresh(rush, &work, &main);
        fresh_s += t1.elapsed().as_secs_f64();
        if step == 0 {
            (first_w, first_f, watch_s, fresh_s) = (watch_s, fresh_s, 0.0, 0.0);
        }
        if verbose {
            println!("  {name} save {step}: {what}: {}", if watch == fresh { "identical" } else { "DIFFERS" });
        }
        if watch != fresh {
            drop(session);
            return Err(incr_report(rush, &work, &main, &tree, &before, step, edits, seed, &what));
        }
        history.push(tree.texts.clone());
    }
    drop(session);
    let pages = Pdf::open(&work.join(INCR_FRESH_PDF)).map_or(0, |p| p.pages.len());
    let saves = edits.max(1) as f64;
    // Which path each save ran (`RT_NATIVE_WHY=1` in the session's log).
    let native = std::fs::read_to_string(&log).map_or(0, |l| l.matches("native job accepted").count());
    Ok(format!("PASS {name}: {edits} edits, every save identical to a fresh compile (seed {seed}; {native} of {} saves native, {pages} pages at the end; first compile {:.0} ms in the session, {:.0} ms fresh; then {:.0} ms a save, {:.0} ms fresh)", edits + 1, 1e3 * first_w, 1e3 * first_f, 1e3 * watch_s / saves, 1e3 * fresh_s / saves))
}

/// The report of the first save whose PDF differs: the edit, the source lines it changed,
/// where the PDFs first differ, and whether two saves in a new session (the files before the
/// edit, then after) reproduce it. The PDFs and both sources are kept in
/// `target/parity/incr/<name>-fail/`.
#[allow(clippy::too_many_arguments)]
fn incr_report(rush: &Path, work: &Path, main: &str, tree: &IncrTree, before: &[Vec<u8>], step: usize, edits: usize, seed: u64, what: &str) -> String {
    let name = work.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let mut r = format!("FAIL {name}: save {step} of {edits} (seed {seed}) differs from a fresh compile\n  edit: {what}\n");
    // The changed lines, the common lines before and after left out.
    for (i, n) in tree.names.iter().enumerate() {
        let (a, b) = (&before[i], &tree.texts[i]);
        if a == b {
            continue;
        }
        for (line, gone, new) in line_hunks(a, b).into_iter().take(4) {
            let _ = writeln!(r, "  {n}, line {line}:");
            for (sign, ls) in [('-', gone), ('+', new)] {
                for l in ls.iter().take(4) {
                    let _ = writeln!(r, "    {sign} {}", latin1(l).chars().take(150).collect::<String>());
                }
                if ls.len() > 4 {
                    let _ = writeln!(r, "    {sign} ({} more lines)", ls.len() - 4);
                }
            }
        }
    }
    let _ = writeln!(r, "  PDF: {}", pdf_difference(&work.join(INCR_WATCH_PDF), &work.join(INCR_FRESH_PDF)));
    let keep = work.with_file_name(format!("{name}-fail"));
    let _ = std::fs::remove_dir_all(&keep);
    let _ = std::fs::create_dir_all(keep.join("before"));
    let _ = std::fs::create_dir_all(keep.join("after"));
    let _ = std::fs::copy(work.join(INCR_WATCH_PDF), keep.join("watch.pdf"));
    let _ = std::fs::copy(work.join(INCR_FRESH_PDF), keep.join("fresh.pdf"));
    for (i, n) in tree.names.iter().enumerate() {
        let flat = n.replace('/', "_");
        let _ = std::fs::write(keep.join("before").join(&flat), &before[i]);
        let _ = std::fs::write(keep.join("after").join(&flat), &tree.texts[i]);
    }
    // The shortest history: the files before the edit, then after, in a new session.
    let short = work.with_file_name(format!("{name}-short"));
    let _ = std::fs::remove_dir_all(&short);
    let fresh = std::fs::read(work.join(INCR_FRESH_PDF)).ok();
    let two = copy_tree(work, &short).ok().and_then(|()| {
        write_tree(&short, &IncrTree { names: tree.names.clone(), texts: before.to_vec(), native: tree.native }, None);
        let mut s = ReplaySession::start(rush, &short, main, &short.with_extension("log")).ok()?;
        s.save().ok()?;
        write_tree(&short, tree, None);
        let _ = std::fs::remove_file(short.join(INCR_WATCH_PDF));
        s.save().ok()?;
        Some(std::fs::read(short.join(INCR_WATCH_PDF)).ok() != fresh)
    });
    let _ = std::fs::remove_dir_all(&short);
    let _ = writeln!(
        r,
        "  {}",
        match (step, two) {
            (0, _) => "the first compile already differs: no history is needed".to_string(),
            (_, Some(true)) => "reproduces with two saves in a new session: the files before the edit, then after".to_string(),
            (_, Some(false)) => format!("needs the longer history: two saves (before, after) do not reproduce it; rerun with --edits {step} --seed {seed}"),
            (_, None) => "the two-save session could not run".to_string(),
        }
    );
    let _ = write!(r, "  kept: {} (watch.pdf, fresh.pdf, before/, after/); session log: {}", keep.display(), work.with_file_name(format!("{name}.log")).display());
    r
}

/// A changed place: its line, the lines gone and the lines new.
type Hunk<'a> = (usize, Vec<&'a [u8]>, Vec<&'a [u8]>);

/// The changed lines of `a` into `b`, hunk by hunk: the line (in `b`) where each starts, the
/// lines gone and the lines new (a longest common subsequence of the lines between the common
/// first and last lines).
fn line_hunks<'a>(a: &'a [u8], b: &'a [u8]) -> Vec<Hunk<'a>> {
    let (la, lb): (Vec<&[u8]>, Vec<&[u8]>) = (a.split(|&c| c == b'\n').collect(), b.split(|&c| c == b'\n').collect());
    let pre = la.iter().zip(&lb).take_while(|(x, y)| x == y).count();
    let suf = la[pre..].iter().rev().zip(lb[pre..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (x, y) = (&la[pre..la.len() - suf], &lb[pre..lb.len() - suf]);
    // lcs[i][j]: the longest common subsequence of x[i..] and y[j..].
    let w = y.len() + 1;
    let mut lcs = vec![0u32; (x.len() + 1) * w];
    for i in (0..x.len()).rev() {
        for j in (0..y.len()).rev() {
            lcs[i * w + j] = if x[i] == y[j] { lcs[(i + 1) * w + j + 1] + 1 } else { lcs[(i + 1) * w + j].max(lcs[i * w + j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    let mut hunk: Option<Hunk> = None;
    while i < x.len() || j < y.len() {
        if i < x.len() && j < y.len() && x[i] == y[j] {
            out.extend(hunk.take());
            i += 1;
            j += 1;
        } else if j < y.len() && (i == x.len() || lcs[i * w + j + 1] >= lcs[(i + 1) * w + j]) {
            hunk.get_or_insert_with(|| (pre + j + 1, Vec::new(), Vec::new())).2.push(y[j]);
            j += 1;
        } else {
            hunk.get_or_insert_with(|| (pre + j + 1, Vec::new(), Vec::new())).1.push(x[i]);
            i += 1;
        }
    }
    out.extend(hunk);
    out
}

/// Where two PDFs first differ: the page count, then the first object (by number) whose
/// value or decoded stream differs, with the first differing line of the stream; else the
/// byte where the files differ.
fn pdf_difference(a: &Path, b: &Path) -> String {
    let (pa, pb) = match (Pdf::open(a), Pdf::open(b)) {
        (Ok(x), Ok(y)) => (x, y),
        (x, y) => return format!("watch PDF: {}; fresh PDF: {}", x.map_or_else(|e| e, |_| "read".into()), y.map_or_else(|e| e, |_| "read".into())),
    };
    let mut r = String::new();
    if pa.pages.len() != pb.pages.len() {
        let _ = write!(r, "{} pages in the watch PDF, {} in the fresh one; ", pa.pages.len(), pb.pages.len());
    }
    let mut ids: Vec<u32> = pa.xref.keys().chain(pb.xref.keys()).copied().collect();
    ids.sort_unstable();
    ids.dedup();
    let dict = |o: &Obj| match o {
        Obj::Stream(d, _) => Obj::Dict(d.clone()),
        o => o.clone(),
    };
    for id in ids {
        let (oa, ob) = (pa.get(id), pb.get(id));
        let (sa, sb) = (pa.stream_bytes(&oa), pb.stream_bytes(&ob));
        if dict(&oa) == dict(&ob) && sa == sb {
            continue;
        }
        let page = pa.pages.iter().position(|&p| match pa.get(p).get("Contents") {
            Some(Obj::Ref(n, _)) => *n == id,
            Some(Obj::Arr(a)) => a.iter().any(|x| matches!(x, Obj::Ref(n, _) if *n == id)),
            _ => false,
        });
        let kind = match (page, pa.key(&dict(&oa), "Type").name()) {
            (Some(p), _) => format!("the content stream of page {}", p + 1),
            (None, Some(t)) => format!("/{}", latin1(t)),
            _ if matches!(*oa, Obj::Stream(..)) => "a stream".into(),
            _ => "an object".into(),
        };
        let _ = write!(r, "object {id} ({kind}) differs first");
        if sa != sb {
            let at = sa.iter().zip(&sb).position(|(x, y)| x != y).unwrap_or(sa.len().min(sb.len()));
            // The line holding it, cut to 60 bytes before and 100 after.
            let line = |s: &[u8]| {
                let k = at.min(s.len());
                let from = s[..k].iter().rposition(|&c| c == b'\n').map_or(0, |p| p + 1).max(k.saturating_sub(60));
                let to = s[k..].iter().position(|&c| c == b'\n').map_or(s.len(), |p| k + p).min(k + 100);
                latin1(&s[from..to])
            };
            let _ = write!(r, ", at byte {at} of its stream:\n    watch: {}\n    fresh: {}", line(&sa), line(&sb));
        } else {
            let show = |o: &Obj| format!("{:?}", dict(o)).chars().take(200).collect::<String>();
            let _ = write!(r, ":\n    watch: {}\n    fresh: {}", show(&oa), show(&ob));
        }
        return r;
    }
    let (da, db) = (&pa.data, &pb.data);
    let at = da.iter().zip(db).position(|(x, y)| x != y).unwrap_or(da.len().min(db.len()));
    let _ = write!(r, "every object equal; the files first differ at byte {at} (layout, cross-reference table or compression)");
    r
}
