//! Boxes and the lists inside them: tex.web §133–§161 (the node types) and §211–§219 (the
//! semantic nest), in our own form.
//!
//! A node is 12 bytes: a kind, a subtype, a font and two words. What does not fit (a box's
//! dimensions and glue setting, a glue specification, a rule's three dimensions) lives in a
//! side table the node indexes. A finished list is a span of the node arena and never changes
//! afterwards, so `\copy` shares a box instead of copying its list, and changing `\wd` of a
//! box makes a new box record (copy on write). Lists being built sit back to back in one
//! growing buffer, one segment per nest level, so building a box allocates nothing per box.

use super::engine_eqtb::Glue;
use super::engine_run::Mode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    /// §134: character `a` of font `font`.
    Char,
    /// §135/§137: box `a` (an index into [`Nodes::boxes`]).
    Box,
    /// §138: rule `a` (an index into [`Nodes::rules`]).
    Rule,
    /// §149: glue `a` (an index into [`Nodes::glues`]); subtype §149's (0 normal, 1–18 the
    /// parameter it came from plus one, 100–102 leaders, with the leader box in `b`).
    Glue,
    /// §155: a kern of width `a`; subtype 0 normal, 1 explicit, 2 accent. pdfTeX's margin
    /// kerns are kerns of subtype [`MARGIN_LEFT`] or [`MARGIN_RIGHT`], with the protruding
    /// character `b` of font `font`.
    Kern,
    /// §157: penalty `a`.
    Penalty,
    /// §143: a ligature: character `a` of font `font`, made from the `sub >> 2` characters
    /// at `b` in [`Nodes::lig_chars`]; `sub & 3` is §143's subtype (a left boundary took
    /// part: 2, a right one: 1).
    Lig,
    /// §145: a discretionary: pre-break and post-break lists `a` in [`Nodes::discs`]
    /// (`-1` for both empty), and `sub` nodes after it that it replaces.
    Disc,
    /// §140: an insertion, record `a` in [`Nodes::inserts`].
    Ins,
    /// §141: a mark: the token span `a..b`, of class `font` (e-TeX's `\marks`).
    Mark,
    /// §142: `\vadjust` material: the node list `a..b`.
    Adjust,
    /// §1341: a whatsit of kind `sub` (see `engine_whatsit`), with `font` and `a..b`.
    Whatsit,
    /// §147: a math node of width `a` (`\mathsurround`); subtype 0 before, 1 after.
    Math,
    /// §681–§688: a noad of a math list being built, `a` indexing the machine's noad
    /// table (see `engine_mlist`). Noads never reach a box.
    Noad,
}

/// pdfTeX's `margin_kern_node` (`left_side`, `right_side`), as kern subtypes.
pub const MARGIN_LEFT: u8 = 4;
pub const MARGIN_RIGHT: u8 = 5;

impl Node {
    /// A pdfTeX margin kern (which `\unkern` and `\lastkern` do not see).
    #[inline]
    pub fn is_margin_kern(&self) -> bool {
        self.kind == Kind::Kern && self.sub >= MARGIN_LEFT
    }
}

/// §140: an insertion for box `n`: its material's height plus depth, `\splitmaxdepth`,
/// `\splittopskip` and `\floatingpenalty` when it was made, and its vertical list.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InsRec {
    pub n: u8,
    pub height: i32,
    pub depth: i32,
    pub split_top: Glue,
    pub float_cost: i32,
    pub list: (u32, u32),
}

/// One node (§133).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    pub kind: Kind,
    pub sub: u8,
    pub font: u16,
    pub a: i32,
    pub b: i32,
}

impl Node {
    pub const fn new(kind: Kind, a: i32) -> Node {
        Node { kind, sub: 0, font: 0, a, b: 0 }
    }
}

/// §135 `glue_sign`: how a box's glue is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Sign {
    #[default]
    Normal,
    Stretching,
    Shrinking,
}

/// §135: a box: `\hbox` (`vertical` false) or `\vbox`, its dimensions, how far it is shifted
/// (§135 `shift_amount`), its list, and how its glue is set.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoxRec {
    pub vertical: bool,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub shift: i32,
    pub list: (u32, u32),
    pub glue_set: f64,
    pub glue_sign: Sign,
    pub glue_order: u8,
}

/// The node arena and its side tables.
#[derive(Debug, Default)]
pub struct Nodes {
    pub list: Vec<Node>,
    pub boxes: Vec<BoxRec>,
    pub glues: Vec<Glue>,
    pub rules: Vec<[i32; 3]>,
    pub lig_chars: Vec<u8>,
    pub discs: Vec<[(u32, u32); 2]>,
    pub inserts: Vec<InsRec>,
    /// The free blocks and slots of each table (model M: one owner per list or record,
    /// freed where TeX frees it; nothing is traced or moved).
    pub free: Free,
    /// Reused scratch of `flush_list` (lists still to free) and `copy_list` (copies being
    /// built, one segment per nesting level).
    pub(super) work: Vec<(u32, u32)>,
    pub(super) copy_buf: Vec<Node>,
    /// The ownership-check build's record of where each list block, box and glue was made,
    /// so the leak audit names the line that leaks.
    #[cfg(feature = "ownership-check")]
    pub origin: Origins,
}

#[cfg(feature = "ownership-check")]
pub enum Table {
    Box,
    Glue,
}

#[cfg(feature = "ownership-check")]
#[derive(Debug, Default)]
pub struct Origins {
    pub lists: std::collections::HashMap<u32, &'static std::panic::Location<'static>>,
    /// Where each ligature's characters were stored, by their start.
    pub ligs: std::collections::HashMap<u32, &'static std::panic::Location<'static>>,
    /// Each discretionary record's last holder: where a list holding it was stored or
    /// freed, and whether that was the free.
    pub disc_seen: std::collections::HashMap<u32, (&'static std::panic::Location<'static>, bool)>,
    /// The size class each live block was stored with, by its start.
    pub classes: std::collections::HashMap<u32, usize>,
    pub boxes: Vec<Option<&'static std::panic::Location<'static>>>,
    pub glues: Vec<Option<&'static std::panic::Location<'static>>>,
}

#[cfg(feature = "ownership-check")]
impl Origins {
    #[track_caller]
    fn note(&mut self, t: Table, i: u32) {
        let v = match t {
            Table::Box => &mut self.boxes,
            Table::Glue => &mut self.glues,
        };
        if v.len() <= i as usize {
            v.resize(i as usize + 1, None);
        }
        v[i as usize] = Some(std::panic::Location::caller());
    }
}

/// Leaders glue (§149 subtypes 100–102) has its box in `b`; rule leaders (110–112) its rule.
#[inline]
pub fn leader(sub: u8) -> Option<bool> {
    match sub {
        100..=102 => Some(true),
        110..=112 => Some(false),
        _ => None,
    }
}

/// A freed box's list in the ownership-check build: outside every arena.
#[cfg(feature = "ownership-check")]
const DEAD_LIST: (u32, u32) = (u32::MAX - 1, u32::MAX);

/// What a freed block holds in the ownership-check build, so a read of it is caught.
#[cfg(feature = "ownership-check")]
const POISON: Node = Node { kind: Kind::Noad, sub: 0xEE, font: 0xDEAD, a: -0x0EAD, b: -0x0EAD };

/// Size classes of list and ligature blocks: exact lengths 1–16, then powers of two up to
/// 2³¹, so a freed block of a class fits any list of that class.
const CLASSES: usize = 16 + 27;

/// The class of a block holding `len` entries, and the class's block length.
#[inline]
pub fn size_class(len: usize) -> (usize, usize) {
    if len <= 16 {
        (len - 1, len)
    } else {
        let p = len.next_power_of_two();
        (16 + p.trailing_zeros() as usize - 5, p)
    }
}

/// The block length of size class `c`.
pub fn class_len(c: usize) -> usize {
    if c < 16 {
        c + 1
    } else {
        1 << (c - 16 + 5)
    }
}

/// What fills a block past its list: never read, since every span ends at its list.
const PAD: Node = Node { kind: Kind::Kern, sub: 0, font: 0, a: 0, b: 0 };

/// Free blocks by size class (their starts) and free fixed-size slots, per table.
#[derive(Debug)]
pub struct Free {
    pub spans: [Vec<u32>; CLASSES],
    pub ligs: [Vec<u32>; CLASSES],
    pub boxes: Vec<u32>,
    pub glues: Vec<u32>,
    pub rules: Vec<u32>,
    pub discs: Vec<u32>,
    pub inserts: Vec<u32>,
}

impl Default for Free {
    fn default() -> Free {
        Free { spans: std::array::from_fn(|_| Vec::new()), ligs: std::array::from_fn(|_| Vec::new()), boxes: Vec::new(), glues: Vec::new(), rules: Vec::new(), discs: Vec::new(), inserts: Vec::new() }
    }
}

impl Free {
}

/// A slot of `table` for `x`: a freed one when there is one, else a new one at the end.
#[inline]
fn slot<T>(table: &mut Vec<T>, free: &mut Vec<u32>, x: T) -> u32 {
    match free.pop() {
        Some(i) => {
            table[i as usize] = x;
            i
        }
        None => {
            table.push(x);
            table.len() as u32 - 1
        }
    }
}

/// §138: a rule dimension that is "running" (`null_flag`): it takes the size of the box.
pub const RUNNING: i32 = -0x4000_0000;

impl Nodes {
    /// The nodes of a finished list.
    #[inline]
    pub fn get(&self, list: (u32, u32)) -> &[Node] {
        let l = &self.list[list.0 as usize..list.1 as usize];
        #[cfg(feature = "ownership-check")]
        assert!(!l.contains(&POISON), "node list {list:?} read after it was freed");
        l
    }

    /// Stores a finished list in a block of its size class (a freed one when there is
    /// one). An empty list takes no block.
    #[cfg_attr(feature = "ownership-check", track_caller)]
    pub fn store(&mut self, nodes: &[Node]) -> (u32, u32) {
        if nodes.is_empty() {
            return (0, 0);
        }
        let (c, cap) = size_class(nodes.len());
        let start = match self.free.spans[c].pop() {
            Some(s) => s as usize,
            None => {
                let s = self.list.len();
                self.list.resize(s + cap, PAD);
                s
            }
        };
        self.list[start..start + nodes.len()].copy_from_slice(nodes);
        #[cfg(feature = "ownership-check")]
        {
            self.origin.lists.insert(start as u32, std::panic::Location::caller());
            self.origin.classes.insert(start as u32, c);
            for n in nodes.iter().filter(|n| n.kind == Kind::Disc && n.a >= 0) {
                self.origin.disc_seen.insert(n.a as u32, (std::panic::Location::caller(), false));
            }
        }
        (start as u32, (start + nodes.len()) as u32)
    }

    /// The block range holding list `l` (the list plus its class's padding).
    #[inline]
    pub fn block(l: (u32, u32)) -> (u32, u32) {
        if l.0 >= l.1 {
            return l;
        }
        (l.0, l.0 + size_class((l.1 - l.0) as usize).1 as u32)
    }

    #[cfg_attr(feature = "ownership-check", track_caller)]
    pub fn add_box(&mut self, b: BoxRec) -> u32 {
        let i = slot(&mut self.boxes, &mut self.free.boxes, b);
        #[cfg(feature = "ownership-check")]
        self.origin.note(Table::Box, i);
        i
    }

    #[cfg_attr(feature = "ownership-check", track_caller)]
    pub fn add_disc(&mut self, d: [(u32, u32); 2]) -> u32 {
        slot(&mut self.discs, &mut self.free.discs, d)
    }

    #[cfg_attr(feature = "ownership-check", track_caller)]
    pub fn add_insert(&mut self, r: InsRec) -> u32 {
        slot(&mut self.inserts, &mut self.free.inserts, r)
    }

    #[cfg_attr(feature = "ownership-check", track_caller)]
    pub fn add_rule(&mut self, dims: [i32; 3]) -> u32 {
        slot(&mut self.rules, &mut self.free.rules, dims)
    }

    #[cfg_attr(feature = "ownership-check", track_caller)]
    pub fn glue_node(&mut self, g: Glue, sub: u8) -> Node {
        let i = slot(&mut self.glues, &mut self.free.glues, g);
        #[cfg(feature = "ownership-check")]
        self.origin.note(Table::Glue, i);
        Node { kind: Kind::Glue, sub, font: 0, a: i as i32, b: 0 }
    }

    #[cfg_attr(feature = "ownership-check", track_caller)]
    pub fn rule_node(&mut self, dims: [i32; 3]) -> Node {
        Node::new(Kind::Rule, self.add_rule(dims) as i32)
    }

    /// A ligature of `c` in font `f` made from the characters `from`.
    #[cfg_attr(feature = "ownership-check", track_caller)]
    pub fn lig_node(&mut self, f: u16, c: u8, from: &[u8], hits: u8) -> Node {
        let at = self.lig_chars.len();
        self.lig_chars.extend_from_slice(from);
        self.lig_node_at(f, c, at, hits)
    }

    /// A ligature of `c` in font `f` made from the characters appended to `lig_chars` from
    /// `at`: they move into a freed block of their class when there is one, else stay as a
    /// new block at the end.
    #[cfg_attr(feature = "ownership-check", track_caller)]
    pub fn lig_node_at(&mut self, f: u16, c: u8, at: usize, hits: u8) -> Node {
        let len = self.lig_chars.len() - at;
        let mut start = at;
        if len > 0 {
            let (k, cap) = size_class(len);
            match self.free.ligs[k].pop() {
                Some(s) => {
                    start = s as usize;
                    self.lig_chars.copy_within(at..at + len, start);
                    self.lig_chars.truncate(at);
                }
                None => self.lig_chars.resize(at + cap, 0),
            }
        }
        #[cfg(feature = "ownership-check")]
        if len > 0 {
            self.origin.ligs.insert(start as u32, std::panic::Location::caller());
        }
        Node { kind: Kind::Lig, sub: (len.min(63) as u8) << 2 | hits, font: f, a: c as i32, b: start as i32 }
    }

    /// Frees list `l`'s block alone: its nodes have moved to a new owner (`\unvbox`).
    #[cfg_attr(feature = "ownership-check", track_caller)]
    pub fn free_block(&mut self, l: (u32, u32)) {
        if l.0 >= l.1 {
            return;
        }
        let (c, cap) = size_class((l.1 - l.0) as usize);
        #[cfg(feature = "ownership-check")]
        {
            assert!(self.list[l.0 as usize] != POISON, "node list {l:?} freed twice");
            // A span changed after it was stored (a sub-span, a shortened tail) would
            // free the wrong block.
            assert_eq!(self.origin.classes.remove(&l.0), Some(c), "node list {l:?} is not the span that was stored");
            for i in l.0..l.1 {
                let n = self.list[i as usize];
                if n.kind == Kind::Disc && n.a >= 0 {
                    self.origin.disc_seen.insert(n.a as u32, (std::panic::Location::caller(), true));
                }
            }
            self.list[l.0 as usize..l.0 as usize + cap].fill(POISON);
        }
        let _ = cap;
        self.free.spans[c].push(l.0);
    }

    /// Frees insertion record `i` alone: its list has moved to a new owner.
    pub fn free_insert(&mut self, i: u32) {
        self.free.inserts.push(i);
    }

    /// Frees discretionary record `d` alone: its texts have moved to a new owner.
    pub fn free_disc(&mut self, d: u32) {
        self.free.discs.push(d);
    }

    /// Frees box record `b` alone: its list has moved to a new owner.
    pub fn free_box(&mut self, b: u32) {
        #[cfg(feature = "ownership-check")]
        {
            assert!(self.boxes[b as usize].list != DEAD_LIST, "box {b} freed twice");
            // A read of the freed box's list now panics (out of the arena).
            self.boxes[b as usize].list = DEAD_LIST;
        }
        self.free.boxes.push(b);
    }

    /// §202 `flush_node_list`: list `l` and everything its nodes own.
    pub fn flush_list(&mut self, l: (u32, u32)) {
        let mut work = std::mem::take(&mut self.work);
        work.push(l);
        while let Some(l) = work.pop() {
            for i in l.0..l.1 {
                let x = self.list[i as usize];
                self.flush_owned(x, &mut work);
            }
            self.free_block(l);
        }
        self.work = work;
    }

    /// §202: frees what node `x` owns (its lists go on `work`); the node itself is a value.
    pub fn flush_node(&mut self, x: Node) {
        let mut work = std::mem::take(&mut self.work);
        self.flush_owned(x, &mut work);
        self.work = work;
        while let Some(l) = self.work.pop() {
            self.flush_list(l);
        }
    }

    /// §202 for one box: its list and its record.
    pub fn flush_box(&mut self, b: u32) {
        let l = self.boxes[b as usize].list;
        #[cfg(feature = "ownership-check")]
        assert!(l != DEAD_LIST, "box {b} flushed after it was freed");
        self.free_box(b);
        self.flush_list(l);
    }

    fn flush_owned(&mut self, x: Node, work: &mut Vec<(u32, u32)>) {
        match x.kind {
            Kind::Box => {
                work.push(self.boxes[x.a as usize].list);
                self.free_box(x.a as u32);
            }
            Kind::Rule => self.free.rules.push(x.a as u32),
            Kind::Glue => {
                self.free.glues.push(x.a as u32);
                match leader(x.sub) {
                    Some(true) => {
                        work.push(self.boxes[x.b as usize].list);
                        self.free_box(x.b as u32);
                    }
                    Some(false) => self.free.rules.push(x.b as u32),
                    None => {}
                }
            }
            Kind::Lig if x.sub >> 2 > 0 => self.free.ligs[size_class((x.sub >> 2) as usize).0].push(x.b as u32),
            Kind::Disc if x.a >= 0 => {
                let [pre, post] = self.discs[x.a as usize];
                work.push(pre);
                work.push(post);
                self.free.discs.push(x.a as u32);
            }
            Kind::Ins => {
                work.push(self.inserts[x.a as usize].list);
                self.free.inserts.push(x.a as u32);
            }
            Kind::Adjust => work.push((x.a as u32, x.b as u32)),
            Kind::Whatsit if super::engine_whatsit::sized(x.sub) => self.free.rules.push(x.a as u32),
            _ => {}
        }
    }

    /// §204 `copy_node_list`: a new list with copies of everything `l`'s nodes own.
    pub fn copy_list(&mut self, l: (u32, u32)) -> (u32, u32) {
        let mut buf = std::mem::take(&mut self.copy_buf);
        let mark = buf.len();
        self.copy_into(l, &mut buf);
        let c = self.store(&buf[mark..]);
        buf.truncate(mark);
        self.copy_buf = buf;
        c
    }

    /// §204: copies of `l`'s nodes, appended to `out` (`\unvcopy` copies straight into
    /// the list being built).
    pub fn copy_into(&mut self, l: (u32, u32), out: &mut Vec<Node>) {
        for i in l.0..l.1 {
            let x = self.list[i as usize];
            let c = self.copy_node(x);
            out.push(c);
        }
    }

    /// §204–§206 for one node: the node with copies of what it owns.
    pub fn copy_node(&mut self, mut x: Node) -> Node {
        match x.kind {
            Kind::Box => x.a = self.copy_box(x.a as u32) as i32,
            Kind::Rule => x.a = self.add_rule(self.rules[x.a as usize]) as i32,
            Kind::Glue => {
                let g = self.glues[x.a as usize];
                x.a = slot(&mut self.glues, &mut self.free.glues, g) as i32;
                match leader(x.sub) {
                    Some(true) => x.b = self.copy_box(x.b as u32) as i32,
                    Some(false) => x.b = self.add_rule(self.rules[x.b as usize]) as i32,
                    None => {}
                }
            }
            Kind::Lig if x.sub >> 2 > 0 => {
                let at = self.lig_chars.len();
                let from = x.b as usize..x.b as usize + (x.sub >> 2) as usize;
                self.lig_chars.extend_from_within(from);
                x.b = self.lig_node_at(x.font, x.a as u8, at, x.sub & 3).b;
            }
            Kind::Disc if x.a >= 0 => {
                let [pre, post] = self.discs[x.a as usize];
                let d = [self.copy_list(pre), self.copy_list(post)];
                x.a = self.add_disc(d) as i32;
            }
            Kind::Ins => {
                let mut r = self.inserts[x.a as usize];
                r.list = self.copy_list(r.list);
                x.a = self.add_insert(r) as i32;
            }
            Kind::Adjust => {
                let (a, b) = self.copy_list((x.a as u32, x.b as u32));
                (x.a, x.b) = (a as i32, b as i32);
            }
            Kind::Whatsit if super::engine_whatsit::sized(x.sub) => x.a = self.add_rule(self.rules[x.a as usize]) as i32,
            _ => {}
        }
        x
    }

    /// A copy of box `b` with a copy of its list (§206).
    pub fn copy_box(&mut self, b: u32) -> u32 {
        let mut r = self.boxes[b as usize];
        r.list = self.copy_list(r.list);
        self.add_box(r)
    }

    /// The characters a ligature was made from.
    pub fn lig_from(&self, n: &Node) -> &[u8] {
        let at = n.b as usize;
        &self.lig_chars[at..at + (n.sub >> 2) as usize]
    }

    #[inline]
    pub fn glue(&self, n: &Node) -> Glue {
        self.glues[n.a as usize]
    }
}

/// §212: one level of the semantic nest: the mode, where its list starts in the build
/// buffer, and `prev_depth` (vertical) or `space_factor` (horizontal) in `aux`.
#[derive(Clone, Copy, Debug)]
pub struct Level {
    pub mode: Mode,
    pub inner: bool,
    /// §212 `clang`: a horizontal list's current language (in the padding after `inner`).
    pub clang: u8,
    pub start: u32,
    pub aux: i32,
    pub prev_graf: i32,
    /// e-TeX's `delim_ptr` (math lists only): where the last `\left` or `\middle` sits.
    pub delim: i32,
}

/// §212 `ignore_depth`: `prev_depth` after a rule, so no interline glue comes next.
pub const IGNORE_DEPTH: i32 = -65_536_000;
