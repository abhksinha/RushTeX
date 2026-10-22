//! The format: tex.web §1299–§1329 (`\dump`, `store_fmt_file`, `load_fmt_file`) in our own
//! form. A format is the engine's state after a class or the kernel has run, written as bytes
//! and read back, so a document starts where the format build stopped instead of running
//! latex.ltx again.
//!
//! The dump first compacts the token arena (TeX frees a macro's old text on redefinition; our
//! arena only grows, so the live lists are copied out, keeping lists that several meanings
//! share shared). Numbers are LEB128 varints (signed ones zigzagged), since nearly all are
//! small: lengths, ids, font parameters, character codes. A fingerprint of the
//! primitive and parameter tables makes a format from another build of the engine refuse to
//! load rather than misread.

use std::collections::HashMap;

use super::engine_prim::{Macro, Meaning, Prim};
use super::engine_eqtb::{Glue, Reg, Region};
use super::engine_node::{BoxRec, Kind, Node, Nodes, Sign};
use super::engine_token::{Span, Token, Tokens};
use super::Engine;

const MAGIC: &[u8; 4] = b"RTXF";

/// Varint output.
#[derive(Default)]
pub struct Writer {
    pub bytes: Vec<u8>,
}

impl Writer {
    pub fn u8(&mut self, v: u8) {
        self.bytes.push(v);
    }

    /// LEB128: seven bits a byte, low first, the top bit set on all but the last.
    pub fn var(&mut self, mut v: u64) {
        while v >= 0x80 {
            self.bytes.push(v as u8 | 0x80);
            v >>= 7;
        }
        self.bytes.push(v as u8);
    }

    pub fn u16(&mut self, v: u16) {
        self.var(v as u64);
    }

    pub fn u32(&mut self, v: u32) {
        self.var(v as u64);
    }

    /// Zigzag, so small negative numbers stay short.
    pub fn i32(&mut self, v: i32) {
        self.u32(((v << 1) ^ (v >> 31)) as u32);
    }

    pub fn u64(&mut self, v: u64) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }

    pub fn count(&mut self, n: usize) {
        self.u32(n as u32);
    }

    pub fn str(&mut self, s: &str) {
        self.raw(s.as_bytes());
    }

    /// Bytes as they are, after their count: read back with one copy.
    pub fn raw(&mut self, b: &[u8]) {
        self.count(b.len());
        self.bytes.extend_from_slice(b);
    }

    pub fn all<T: Codec>(&mut self, items: &[T]) {
        self.count(items.len());
        for x in items {
            x.put(self);
        }
    }
}

/// Varint input; every read is `None` past the end or on a malformed number.
pub struct Reader<'b> {
    bytes: &'b [u8],
    at: usize,
}

impl<'b> Reader<'b> {
    pub fn new(bytes: &'b [u8]) -> Self {
        Reader { bytes, at: 0 }
    }

    pub fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let b = self.bytes.get(self.at..self.at + N)?.try_into().ok()?;
        self.at += N;
        Some(b)
    }

    pub fn u8(&mut self) -> Option<u8> {
        self.take::<1>().map(|b| b[0])
    }

    pub fn var(&mut self) -> Option<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.u8()?;
            v |= ((b & 0x7F) as u64) << shift;
            if b < 0x80 {
                return Some(v);
            }
        }
        None
    }

    pub fn u16(&mut self) -> Option<u16> {
        self.var()?.try_into().ok()
    }

    pub fn u32(&mut self) -> Option<u32> {
        self.var()?.try_into().ok()
    }

    pub fn i32(&mut self) -> Option<i32> {
        let z = self.u32()?;
        Some((z >> 1) as i32 ^ -((z & 1) as i32))
    }

    pub fn u64(&mut self) -> Option<u64> {
        self.take().map(u64::from_le_bytes)
    }

    /// A length, refused when the rest of the input could not hold that many bytes.
    pub fn count(&mut self) -> Option<usize> {
        let n = self.u32()? as usize;
        (n <= self.bytes.len() - self.at).then_some(n)
    }

    pub fn raw(&mut self) -> Option<Vec<u8>> {
        let n = self.count()?;
        let b = self.bytes.get(self.at..self.at + n)?.to_vec();
        self.at += n;
        Some(b)
    }

    pub fn str(&mut self) -> Option<String> {
        let n = self.count()?;
        let s = std::str::from_utf8(self.bytes.get(self.at..self.at + n)?).ok()?.to_string();
        self.at += n;
        Some(s)
    }

    pub fn all<T: Codec>(&mut self) -> Option<Vec<T>> {
        let n = self.count()?;
        (0..n).map(|_| T::take(self)).collect()
    }

    fn done(&self) -> bool {
        self.at == self.bytes.len()
    }
}

/// A value with a fixed byte form.
pub trait Codec: Sized {
    fn put(&self, w: &mut Writer);
    fn take(r: &mut Reader) -> Option<Self>;
}

impl Codec for i32 {
    fn put(&self, w: &mut Writer) {
        w.i32(*self);
    }
    fn take(r: &mut Reader) -> Option<Self> {
        r.i32()
    }
}

impl Codec for u8 {
    fn put(&self, w: &mut Writer) {
        w.u8(*self);
    }
    fn take(r: &mut Reader) -> Option<Self> {
        r.u8()
    }
}

impl Codec for u32 {
    fn put(&self, w: &mut Writer) {
        w.u32(*self);
    }
    fn take(r: &mut Reader) -> Option<Self> {
        r.u32()
    }
}

impl Codec for u16 {
    fn put(&self, w: &mut Writer) {
        w.u16(*self);
    }
    fn take(r: &mut Reader) -> Option<Self> {
        r.u16()
    }
}

/// A control sequence as `id·2+1`, a character as `(code·16+cat)·2`: one to three bytes
/// for nearly every token (the raw word puts the flag and catcode in the high bits).
impl Codec for Token {
    fn put(&self, w: &mut Writer) {
        let t = self.0;
        match self.cs_id() {
            Some(id) => w.var((id as u64) << 1 | 1),
            None => w.var(((t & 0x1F_FFFF) as u64) << 5 | ((t >> 21) as u64 & 15) << 1),
        }
    }
    fn take(r: &mut Reader) -> Option<Self> {
        let v = r.var()?;
        if v & 1 == 1 {
            let id = u32::try_from(v >> 1).ok().filter(|&id| id < super::engine_token::DONT_EXPAND)?;
            return Some(Token::cs(id));
        }
        let c = u32::try_from(v >> 5).ok().filter(|&c| c <= 0x1F_FFFF)?;
        Some(Token(((v >> 1) as u32 & 15) << 21 | c))
    }
}

impl Codec for Span {
    fn put(&self, w: &mut Writer) {
        w.u32(self.start);
        w.u32(self.end);
    }
    fn take(r: &mut Reader) -> Option<Self> {
        let (start, end) = (r.u32()?, r.u32()?);
        (start <= end).then_some(Span { start, end })
    }
}

impl Codec for Glue {
    fn put(&self, w: &mut Writer) {
        w.i32(self.width);
        w.i32(self.stretch);
        w.i32(self.shrink);
        w.u8(self.stretch_order);
        w.u8(self.shrink_order);
    }
    fn take(r: &mut Reader) -> Option<Self> {
        Some(Glue { width: r.i32()?, stretch: r.i32()?, shrink: r.i32()?, stretch_order: r.u8()?, shrink_order: r.u8()? })
    }
}

impl Codec for Reg {
    fn put(&self, w: &mut Writer) {
        w.u8(self.region as u8);
        w.u32(self.slot);
    }
    fn take(r: &mut Reader) -> Option<Self> {
        let region = match r.u8()? {
            0 => Region::Int,
            1 => Region::Dimen,
            2 => Region::Glue,
            3 => Region::Toks,
            4 => Region::Mu,
            _ => return None,
        };
        Some(Reg { region, slot: r.u32()? })
    }
}

impl Codec for Meaning {
    fn put(&self, w: &mut Writer) {
        match *self {
            Meaning::Unset => w.u8(0),
            Meaning::Undefined => w.u8(1),
            Meaning::Prim(p) => {
                w.u8(2);
                w.u8(p as u8);
            }
            Meaning::Macro(m) => {
                w.u8(3);
                m.text.put(w);
                w.u8(m.flags);
            }
            Meaning::Char(t) => {
                w.u8(4);
                t.put(w);
            }
            Meaning::Reg(reg) => {
                w.u8(5);
                reg.put(w);
            }
            Meaning::CharDef(c) => {
                w.u8(6);
                w.u32(c);
            }
            Meaning::MathCharDef(c) => {
                w.u8(7);
                w.u32(c);
            }
            Meaning::DontExpand => w.u8(8),
            Meaning::Font(f) => {
                w.u8(9);
                w.u16(f);
            }
            Meaning::Native(i) => {
                w.u8(10);
                w.u16(i);
            }
            Meaning::Kernel(i) => {
                w.u8(11);
                w.u16(i);
            }
        }
    }
    fn take(r: &mut Reader) -> Option<Self> {
        Some(match r.u8()? {
            0 => Meaning::Unset,
            1 => Meaning::Undefined,
            2 => Meaning::Prim(Prim::from_u8(r.u8()?)?),
            3 => Meaning::Macro(Macro { text: Span::take(r)?, flags: r.u8()? }),
            4 => Meaning::Char(Token::take(r)?),
            5 => Meaning::Reg(Reg::take(r)?),
            6 => Meaning::CharDef(r.u32()?),
            7 => Meaning::MathCharDef(r.u32()?),
            8 => Meaning::DontExpand,
            9 => Meaning::Font(r.u16()?),
            10 => Meaning::Native(r.u16()?),
            11 => Meaning::Kernel(r.u16()?),
            _ => return None,
        })
    }
}

impl Codec for Node {
    fn put(&self, w: &mut Writer) {
        w.u8(self.kind as u8);
        w.u8(self.sub);
        w.u16(self.font);
        w.i32(self.a);
        w.i32(self.b);
    }
    fn take(r: &mut Reader) -> Option<Self> {
        let kind = match r.u8()? {
            0 => Kind::Char,
            1 => Kind::Box,
            2 => Kind::Rule,
            3 => Kind::Glue,
            4 => Kind::Kern,
            5 => Kind::Penalty,
            6 => Kind::Lig,
            7 => Kind::Disc,
            8 => Kind::Ins,
            9 => Kind::Mark,
            10 => Kind::Adjust,
            11 => Kind::Whatsit,
            12 => Kind::Math,
            _ => return None,
        };
        Some(Node { kind, sub: r.u8()?, font: r.u16()?, a: r.i32()?, b: r.i32()? })
    }
}

impl Codec for BoxRec {
    fn put(&self, w: &mut Writer) {
        w.u8(self.vertical as u8);
        for v in [self.width, self.height, self.depth, self.shift] {
            w.i32(v);
        }
        w.u32(self.list.0);
        w.u32(self.list.1);
        w.u64(self.glue_set.to_bits());
        w.u8(self.glue_sign as u8);
        w.u8(self.glue_order);
    }
    fn take(r: &mut Reader) -> Option<Self> {
        let vertical = r.u8()? != 0;
        let (width, height, depth, shift) = (r.i32()?, r.i32()?, r.i32()?, r.i32()?);
        let list = (r.u32()?, r.u32()?);
        let glue_set = f64::from_bits(r.u64()?);
        let glue_sign = match r.u8()? {
            0 => Sign::Normal,
            1 => Sign::Stretching,
            2 => Sign::Shrinking,
            _ => return None,
        };
        Some(BoxRec { vertical, width, height, depth, shift, list, glue_set, glue_sign, glue_order: r.u8()? })
    }
}

impl Codec for [i32; 2] {
    fn put(&self, w: &mut Writer) {
        w.i32(self[0]);
        w.i32(self[1]);
    }
    fn take(r: &mut Reader) -> Option<Self> {
        Some([r.i32()?, r.i32()?])
    }
}

impl Codec for [i32; 3] {
    fn put(&self, w: &mut Writer) {
        for v in self {
            w.i32(*v);
        }
    }
    fn take(r: &mut Reader) -> Option<Self> {
        Some([r.i32()?, r.i32()?, r.i32()?])
    }
}

/// The node arena: every list, box record, glue specification and rule (§1311 dumps the
/// dynamic memory the same way, whole).
fn dump_nodes(n: &Nodes, w: &mut Writer) {
    w.all(&n.list);
    w.all(&n.boxes);
    w.all(&n.glues);
    w.all(&n.rules);
    w.all(&n.lig_chars);
    w.count(n.discs.len());
    for d in &n.discs {
        for (a, b) in d {
            w.u32(*a);
            w.u32(*b);
        }
    }
    w.count(n.inserts.len());
    for i in &n.inserts {
        w.u8(i.n);
        for v in [i.height, i.depth, i.float_cost] {
            w.i32(v);
        }
        i.split_top.put(w);
        w.u32(i.list.0);
        w.u32(i.list.1);
    }
}

/// Reads [`dump_nodes`]' form; every index a node or box holds must be in range.
fn undump_nodes(r: &mut Reader) -> Option<Nodes> {
    let (list, boxes, glues, rules, lig_chars) = (r.all()?, r.all()?, r.all()?, r.all()?, r.all()?);
    let count = r.count()?;
    let mut discs = Vec::with_capacity(count);
    for _ in 0..count {
        discs.push([(r.u32()?, r.u32()?), (r.u32()?, r.u32()?)]);
    }
    let count = r.count()?;
    let mut inserts = Vec::with_capacity(count);
    for _ in 0..count {
        let (n, height, depth, float_cost) = (r.u8()?, r.i32()?, r.i32()?, r.i32()?);
        let split_top = Glue::take(r)?;
        inserts.push(super::engine_node::InsRec { n, height, depth, split_top, float_cost, list: (r.u32()?, r.u32()?) });
    }
    let n = Nodes { list, boxes, glues, rules, lig_chars, discs, inserts, ..Default::default() };
    let span_ok = |s: &(u32, u32)| s.0 <= s.1 && s.1 as usize <= n.list.len();
    let in_list = |b: &BoxRec| b.list.0 <= b.list.1 && b.list.1 as usize <= n.list.len();
    let node_ok = |x: &Node| match x.kind {
        Kind::Box => (x.a as usize) < n.boxes.len(),
        Kind::Rule => (x.a as usize) < n.rules.len(),
        Kind::Glue => {
            (x.a as usize) < n.glues.len()
                && match x.sub {
                    0..=99 => true,
                    100..=109 => (x.b as usize) < n.boxes.len(),
                    _ => (x.b as usize) < n.rules.len(),
                }
        }
        Kind::Lig => x.b >= 0 && x.b as usize + (x.sub >> 2) as usize <= n.lig_chars.len(),
        Kind::Disc => x.a == -1 || (x.a >= 0 && (x.a as usize) < n.discs.len()),
        Kind::Ins => x.a >= 0 && (x.a as usize) < n.inserts.len(),
        Kind::Adjust => span_ok(&(x.a as u32, x.b as u32)),
        // Token spans: ordered (the token arena is checked on its own).
        Kind::Mark => x.a >= 0 && x.a <= x.b,
        // pdfTeX's whatsits from `\pdfdest` on (`engine_pdfdoc`, `engine_image`) index side
        // tables, not tokens.
        Kind::Whatsit => x.sub == super::engine_whatsit::LANGUAGE || (x.sub >= super::engine_pdfdoc::DEST && x.a >= 0) || (x.a >= 0 && x.a <= x.b),
        _ => true,
    };
    (n.inserts.iter().all(|i| span_ok(&i.list)) && n.discs.iter().flatten().all(span_ok) && n.boxes.iter().all(in_list) && n.list.iter().all(node_ok)).then_some(n)
}

/// FNV-1a over the names of the engine's primitives and parameters, in the order their
/// numbers come from: a format records the numbering it was written with.
fn fingerprint() -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |s: &str| {
        for &b in s.as_bytes().iter().chain(b"\0") {
            h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
    };
    for b in 0..=255u8 {
        if let Some(p) = Prim::from_u8(b) {
            eat(p.name());
        }
    }
    for (name, _) in super::engine_tables::PRIMITIVES.iter() {
        eat(name);
    }
    for (name, region, slot) in super::engine_tables::PARAMS.iter() {
        eat(name);
        eat(&format!("{}:{slot}", *region as u8));
    }
    // The font table's contents: the format stores pinned fonts by table ID, so a
    // regenerated table must refuse formats made with another one.
    eat(&format!("{:016x}", crate::data::data_tfm::fonts().map_or(0, |t| t.content_id())));
    h
}

impl Engine {
    /// Copies the token lists still in use into a fresh arena, so the lists of every
    /// redefined macro are dropped. Lists shared by several meanings stay shared. Only safe
    /// between documents or at `\dump`, when nothing being read refers into the arena.
    pub fn compact(&mut self) {
        let Engine { eqtb, toks, .. } = self;
        let mut fresh = Tokens::default();
        let mut moved: HashMap<(u32, u32), Span> = HashMap::new();
        eqtb.for_each_span(|s| {
            *s = if s.is_empty() { Span::default() } else { *moved.entry((s.start, s.end)).or_insert_with(|| fresh.store(toks.get(*s))) };
        });
        *toks = fresh;
    }

    /// §1302 `store_fmt_file`: the engine as bytes. `None` inside a group (§1304: "You can't
    /// dump inside a group").
    pub fn dump(&mut self) -> Option<Vec<u8>> {
        if !self.eqtb.at_top() {
            return None;
        }
        self.compact();
        let mut w = Writer::default();
        w.bytes.extend_from_slice(MAGIC);
        w.u64(fingerprint());
        self.cs.dump(&mut w);
        self.toks.dump(&mut w);
        self.eqtb.dump(&mut w);
        self.names.dump(&mut w);
        self.fonts.dump(&mut w);
        dump_nodes(&self.nodes, &mut w);
        self.hyph.dump(&mut w);
        self.glyph_unicode.dump(&mut w);
        Some(w.bytes)
    }

    /// §1303 `load_fmt_file`: an engine from [`Engine::dump`]'s bytes; `None` when they are not
    /// a format of this build.
    pub fn undump(bytes: &[u8]) -> Option<Engine> {
        let mut r = Reader::new(bytes);
        if r.take::<4>()? != *MAGIC || r.u64()? != fingerprint() {
            return None;
        }
        let cs = super::engine_token::CsTable::undump(&mut r)?;
        let toks = Tokens::undump(&mut r)?;
        let eqtb = super::engine_eqtb::Eqtb::undump(&mut r, toks.len() as u32)?;
        let names = super::engine_names::Names::undump(&mut r)?;
        let fonts = super::engine_font::Fonts::undump(&mut r)?;
        let nodes = undump_nodes(&mut r)?;
        let hyph = super::engine_hyph::Hyph::undump(&mut r)?;
        let glyph_unicode = super::engine_tounicode::GlyphUnicode::undump(&mut r)?;
        // The fixed ids (`CUR_FONT_ID`, the active characters) must be where this build puts them.
        let fixed = cs.get(super::engine_font::CUR_FONT) == Some(super::CUR_FONT_ID);
        let boxes_ok = (0..32768).all(|n| eqtb.box_reg(n).is_none_or(|b| (b as usize) < nodes.boxes.len()));
        (r.done() && fixed && boxes_ok).then_some(Engine { eqtb, names, cs, toks, fonts, nodes, hyph, glyph_unicode })
    }
}

#[cfg(test)]
mod tests {
    use super::super::engine_run::Machine;
    use super::*;

    /// `\meaning` of every control sequence.
    fn meanings(e: &mut Engine) -> Vec<String> {
        let m = Machine::new(e, "");
        (0..m.e.cs.len() as u32)
            .map(|id| {
                let mut s = String::new();
                m.print_meaning(Token::cs(id), &mut s);
                s
            })
            .collect()
    }

    #[test]
    fn a_format_reads_back_as_it_was_written() {
        let src = r"\catcode`\{=1 \catcode`\}=2 \catcode`\#=6
            \def\a#1{[#1]}\def\a#1{(#1)}\let\b\a \countdef\c=5 \c=7 \dimen3=2pt \skip4=1pt plus 1fil
            \toks2{x\a y}\lccode`\A=`\z \font\f=cmr10 at 12pt \fontdimen2\f=3pt \chardef\d=65
            \muskip1=3mu plus 1fill \edef\e{\a z}";
        let mut e = Engine::default();
        e.eqtb.use_initex_catcodes();
        Machine::new(&mut e, src).run();
        let before = meanings(&mut e);
        let stored = e.toks.len();
        let bytes = e.dump().expect("at top level");
        assert!(e.toks.len() < stored, "the old text of \\a is dropped");
        assert_eq!(meanings(&mut e), before, "compacting keeps every meaning");
        let mut back = Engine::undump(&bytes).expect("reads back");
        assert_eq!(meanings(&mut back), before);
        let mut m = Machine::new(&mut back, r"\a q\the\c\the\dimen3 \the\skip4 \the\toks2 \the\lccode`\A \the\fontdimen2\f\the\muskip1 ");
        m.run();
        assert_eq!(m.out_text(), "(q)72.0pt1.0pt plus 1.0filx(y)1223.0pt3.0mu plus 1.0fill");
        assert!(Engine::undump(&bytes[..bytes.len() - 1]).is_none());
    }
}
