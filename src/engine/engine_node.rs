//! Boxes and the lists inside them: tex.web §133–§161 (the node types) and §211–§219 (the
//! semantic nest), in our own form.
//!
//! A node is 12 bytes: a kind, a subtype, a font and two words. What does not fit (a box's
//! dimensions and glue setting, a glue specification, a rule's three dimensions) lives in a
//! side table the node indexes. A finished list is a span of the node arena and never changes
//! afterwards, so `\copy` shares a box instead of copying its list, and changing `\wd` of a
//! box makes a new box record (copy on write). Lists being built sit back to back in one
//! growing buffer, one segment per nest level, so building a box allocates nothing per box.

use super::engine_chunk::Chunked;
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
    /// GDL: a word of font `font` made by the main loop, entry `a` of [`Nodes::words`]. It
    /// stands for the entry's character, ligature and kern nodes, which it owns none of;
    /// only the native job makes them, in paragraphs, and whatever looks inside a word
    /// reads the entry or expands it ([`Nodes::expand_words`]).
    Word,
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
#[derive(PartialEq, Debug, Default, Clone)]
pub struct Nodes {
    pub list: Chunked<Node>,
    pub boxes: Chunked<BoxRec>,
    pub glues: Chunked<Glue>,
    pub rules: Chunked<[i32; 3]>,
    pub lig_chars: Chunked<u8>,
    /// The word table's entries ([`Kind::Word`]).
    pub words: super::engine_words::WordStore,
    pub discs: Chunked<[(u32, u32); 2]>,
    pub inserts: Chunked<InsRec>,
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

/// The new values of the places where two node arenas of one layout differ, per table
/// ([`Nodes::edits_to`]).
#[derive(Default, Debug)]
pub struct NodeEdits {
    pub list: Vec<(usize, Node)>,
    pub boxes: Vec<(usize, BoxRec)>,
    pub glues: Vec<(usize, Glue)>,
    pub rules: Vec<(usize, [i32; 3])>,
    pub lig_chars: Vec<(usize, u8)>,
    pub discs: Vec<(usize, [(u32, u32); 2])>,
    pub inserts: Vec<(usize, InsRec)>,
}

impl NodeEdits {
    pub fn is_empty(&self) -> bool {
        self.list.is_empty() && self.boxes.is_empty() && self.glues.is_empty() && self.rules.is_empty() && self.lig_chars.is_empty() && self.discs.is_empty() && self.inserts.is_empty()
    }
}

/// Per place of a table, whether it is in use outside some box trees ([`Nodes::outside`]).
struct Live(Vec<bool>);

impl Live {
    fn off(&mut self, at: usize, len: usize) {
        let n = self.0.len();
        self.0[at.min(n)..(at + len).min(n)].fill(false);
    }

    fn on(&mut self, at: usize, len: usize) {
        let n = self.0.len();
        self.0[at.min(n)..(at + len).min(n)].fill(true);
    }

    /// The same places set, a shorter table's missing ones unset.
    fn same(&self, o: &Live) -> bool {
        let n = self.0.len().max(o.0.len());
        (0..n).all(|i| self.0.get(i).copied().unwrap_or(false) == o.0.get(i).copied().unwrap_or(false))
    }
}

/// [`Live`] for each table.
struct Places {
    list: Live,
    ligs: Live,
    boxes: Live,
    glues: Live,
    rules: Live,
    discs: Live,
    inserts: Live,
}

impl Places {
    fn new(n: &Nodes) -> Places {
        let all = |len: usize| Live(vec![true; len]);
        Places { list: all(n.list.count()), ligs: all(n.lig_chars.count()), boxes: all(n.boxes.count()), glues: all(n.glues.count()), rules: all(n.rules.count()), discs: all(n.discs.count()), inserts: all(n.inserts.count()) }
    }

    fn none(n: &Nodes) -> Places {
        let all = |len: usize| Live(vec![false; len]);
        Places { list: all(n.list.count()), ligs: all(n.lig_chars.count()), boxes: all(n.boxes.count()), glues: all(n.glues.count()), rules: all(n.rules.count()), discs: all(n.discs.count()), inserts: all(n.inserts.count()) }
    }
}

/// A pair of things to compare in two arenas ([`Nodes::same_outside`]).
enum Unit {
    List((u32, u32), (u32, u32)),
    Box(u32, u32),
    Glue(u32, u32),
    Rule(u32, u32),
    Disc(u32, u32),
    Ins(u32, u32),
}

/// What a set of box trees holds, per table ([`Nodes::owns`]).
#[derive(Default)]
struct Owned {
    list: Vec<(usize, usize)>,
    ligs: Vec<(usize, usize)>,
    boxes: Vec<usize>,
    glues: Vec<usize>,
    rules: Vec<usize>,
    discs: Vec<usize>,
    inserts: Vec<usize>,
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
#[derive(PartialEq, Debug, Clone)]
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
fn slot<T: Clone>(table: &mut Chunked<T>, free: &mut Vec<u32>, x: T) -> u32 {
    match free.pop() {
        Some(i) => {
            table.set(i as usize, x);
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
    /// A checkpoint's copy (`engine_checkpoint`): the tables frozen, sharing what did not
    /// change since the last checkpoint; scratch left empty.
    pub fn freeze(&mut self) -> Nodes {
        let Nodes { list, boxes, glues, rules, lig_chars, words, discs, inserts, free, work: _, copy_buf: _, #[cfg(feature = "ownership-check")] origin: _ } = self;
        Nodes { list: list.freeze(), boxes: boxes.freeze(), glues: glues.freeze(), rules: rules.freeze(), lig_chars: lig_chars.freeze(), words: words.freeze(), discs: discs.freeze(), inserts: inserts.freeze(), free: free.clone(), ..Default::default() }
    }

    /// Where frozen arena `new` differs from this one of the same layout (every table of
    /// the same length, the same free lists), as the new values; `None` when the layouts
    /// differ (`engine_checkpoint`'s early cutoff). Free places are left out (what they
    /// hold is never read), and so are word nodes whose entries, under different ids in the
    /// two runs' word tables, stand for the same nodes.
    pub fn edits_to(&self, new: &Nodes) -> Option<NodeEdits> {
        if self.free != new.free {
            return None;
        }
        fn take<T: Clone + PartialEq>(a: &Chunked<T>, b: &Chunked<T>) -> Option<Vec<(usize, T)>> {
            Some(a.differences(b)?.into_iter().map(|i| (i, b.at(i).cloned().unwrap_or_else(|| unreachable!()))).collect())
        }
        let mut e = NodeEdits { list: take(&self.list, &new.list)?, boxes: take(&self.boxes, &new.boxes)?, glues: take(&self.glues, &new.glues)?, rules: take(&self.rules, &new.rules)?, lig_chars: take(&self.lig_chars, &new.lig_chars)?, discs: take(&self.discs, &new.discs)?, inserts: take(&self.inserts, &new.inserts)? };
        // The word tables: an id both have whose entries differ makes each word node naming it
        // a difference too (the runs' tables part where the edit's words were entered).
        let stores = (self.words != new.words).then(|| (self.words.thaw(), new.words.thaw()));
        if let Some((a, b)) = &stores {
            let n = a.recs.len().min(b.recs.len());
            let changed: Vec<bool> = (0..n).map(|i| !a.same_entry(i as u32, b, i as u32)).collect();
            if changed.contains(&true) {
                let have: std::collections::HashSet<usize> = e.list.iter().map(|(i, _)| *i).collect();
                for i in 0..new.list.count() {
                    if let Some(x) = new.list.at(i).filter(|x| x.kind == Kind::Word && changed.get(x.a as usize).copied().unwrap_or(false)) {
                        if !have.contains(&i) {
                            e.list.push((i, *x));
                        }
                    }
                }
                e.list.sort_unstable_by_key(|(i, _)| *i);
            }
        }
        if e.is_empty() {
            return Some(e);
        }
        let f = &self.free;
        let blocks = |starts: &[Vec<u32>], len: usize| {
            let mut free = vec![false; len];
            for (c, v) in starts.iter().enumerate() {
                for &s in v {
                    let s = s as usize;
                    free[s.min(len)..(s + class_len(c)).min(len)].fill(true);
                }
            }
            free
        };
        let ids = |v: &[u32], len: usize| {
            let mut free = vec![false; len];
            for &i in v {
                if let Some(x) = free.get_mut(i as usize) {
                    *x = true;
                }
            }
            free
        };
        fn keep<T>(e: &mut Vec<(usize, T)>, free: &[bool]) {
            e.retain(|(i, _)| !free.get(*i).copied().unwrap_or(false));
        }
        if !e.list.is_empty() {
            keep(&mut e.list, &blocks(&f.spans, self.list.count()));
        }
        if !e.lig_chars.is_empty() {
            keep(&mut e.lig_chars, &blocks(&f.ligs, self.lig_chars.count()));
        }
        keep(&mut e.boxes, &ids(&f.boxes, self.boxes.count()));
        keep(&mut e.glues, &ids(&f.glues, self.glues.count()));
        keep(&mut e.rules, &ids(&f.rules, self.rules.count()));
        keep(&mut e.discs, &ids(&f.discs, self.discs.count()));
        keep(&mut e.inserts, &ids(&f.inserts, self.inserts.count()));
        if let Some((a, b)) = &stores {
            e.list.retain(|(i, n)| match self.list.at(*i) {
                Some(o) if n.kind == Kind::Word && o.kind == Kind::Word && (o.sub, o.font, o.b) == (n.sub, n.font, n.b) => !a.same_entry(o.a as u32, b, n.a as u32),
                _ => true,
            });
        }
        Some(e)
    }

    /// This frozen arena with `e`'s values written in; the word entries its word nodes
    /// name, in the word table `from` they were made in, are copied into this one's.
    pub fn patched(&self, e: &NodeEdits, from: &super::engine_words::WordStore) -> Nodes {
        let mut words = self.words.clone();
        let mut list = std::borrow::Cow::Borrowed(&e.list);
        if e.list.iter().any(|(_, n)| n.kind == Kind::Word) {
            let (mut to, from) = (self.words.thaw(), from.thaw());
            let mut copied: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
            list = std::borrow::Cow::Owned(e.list.iter().map(|&(i, n)| match n.kind {
                Kind::Word => (i, Node { a: *copied.entry(n.a).or_insert_with(|| to.copy_entry(&from, n.a as u32) as i32), ..n }),
                _ => (i, n),
            }).collect());
            words = to.freeze();
        }
        Nodes { list: self.list.patched(&list), boxes: self.boxes.patched(&e.boxes), glues: self.glues.patched(&e.glues), rules: self.rules.patched(&e.rules), lig_chars: self.lig_chars.patched(&e.lig_chars), discs: self.discs.patched(&e.discs), inserts: self.inserts.patched(&e.inserts), words, free: self.free.clone(), ..Default::default() }
    }

    /// Per table, the places of frozen arena `self` that are in use and outside the trees of
    /// boxes `roots` (neither free nor owned by them); `None` for a malformed tree.
    fn outside(&self, roots: &[u32]) -> Option<Places> {
        let mut p = Places::new(self);
        let f = &self.free;
        for (c, v) in f.spans.iter().enumerate() {
            for &s in v {
                p.list.off(s as usize, class_len(c));
            }
        }
        for (c, v) in f.ligs.iter().enumerate() {
            for &s in v {
                p.ligs.off(s as usize, class_len(c));
            }
        }
        for (t, v) in [(&mut p.boxes, &f.boxes), (&mut p.glues, &f.glues), (&mut p.rules, &f.rules), (&mut p.discs, &f.discs), (&mut p.inserts, &f.inserts)] {
            for &i in v {
                t.off(i as usize, 1);
            }
        }
        let mut stack: Vec<(u32, u32)> = Vec::new();
        let boxed = |b: u32, p: &mut Places, stack: &mut Vec<(u32, u32)>| -> Option<()> {
            p.boxes.off(b as usize, 1);
            stack.push(self.boxes.at(b as usize)?.list);
            Some(())
        };
        for &b in roots {
            boxed(b, &mut p, &mut stack)?;
        }
        while let Some(l) = stack.pop() {
            let (s, t) = Nodes::block(l);
            p.list.off(s as usize, (t - s) as usize);
            for i in l.0..l.1 {
                let n = *self.list.at(i as usize)?;
                match n.kind {
                    Kind::Box => boxed(n.a as u32, &mut p, &mut stack)?,
                    Kind::Glue => {
                        p.glues.off(n.a as usize, 1);
                        match leader(n.sub) {
                            Some(true) => boxed(n.b as u32, &mut p, &mut stack)?,
                            Some(false) => p.rules.off(n.b as usize, 1),
                            None => {}
                        }
                    }
                    Kind::Rule => p.rules.off(n.a as usize, 1),
                    Kind::Whatsit if super::engine_whatsit::sized(n.sub) => p.rules.off(n.a as usize, 1),
                    Kind::Lig if n.sub >> 2 > 0 => p.ligs.off(n.b as usize, size_class((n.sub >> 2) as usize).1),
                    Kind::Disc if n.a >= 0 => {
                        p.discs.off(n.a as usize, 1);
                        let [pre, post] = *self.discs.at(n.a as usize)?;
                        stack.extend([pre, post]);
                    }
                    Kind::Ins => {
                        p.inserts.off(n.a as usize, 1);
                        stack.push(self.inserts.at(n.a as usize)?.list);
                    }
                    Kind::Adjust => stack.push((n.a as u32, n.b as u32)),
                    Kind::Noad => return None,
                    _ => {}
                }
            }
        }
        Some(p)
    }

    /// Whether boxes `keep` are all in use outside the trees of boxes `mine`.
    pub fn outside_all(&self, mine: &[u32], keep: &[u32]) -> bool {
        self.outside(mine).is_some_and(|a| keep.iter().all(|&k| a.boxes.0.get(k as usize).copied().unwrap_or(false)))
    }

    /// Whether frozen arenas `self` and `other` hold the same outside the trees of boxes
    /// `mine` and `theirs` (each its own), boxes `keep` among it (`engine_checkpoint`'s early
    /// cutoff). Outside the trees, the places nothing there points to (the tops: what the
    /// machine's state, equal in the two runs, holds) must be the same in both, and each top
    /// the same tree in both, wherever its parts sit: an edit that changed a page's size
    /// moves every later allocation. A word node is compared by what its entry stands for,
    /// `words` being the two word tables, live. `roots` are the nodes the two states hold
    /// that may sit at different places (the current list and page): compared pair by pair first,
    /// every top they reach is matched through them; the other tops must sit at equal places.
    pub fn same_outside(&self, mine: &[u32], other: &Nodes, theirs: &[u32], keep: &[u32], words: (&super::engine_words::WordStore, &super::engine_words::WordStore), roots: &[(&[Node], &[Node])]) -> bool {
        let (Some(mut a), Some(mut b)) = (self.outside(mine), other.outside(theirs)) else { return false };
        if !keep.iter().all(|&k| a.boxes.0.get(k as usize).copied().unwrap_or(false) && b.boxes.0.get(k as usize).copied().unwrap_or(false)) {
            return false;
        }
        self.tops(&mut a);
        other.tops(&mut b);
        if roots.iter().any(|r| r.0.len() != r.1.len()) {
            return false;
        }
        let mut work: Vec<Unit> = Vec::new();
        for (p, q) in roots.iter().flat_map(|r| r.0.iter().zip(r.1)) {
            if !self.same_node(p, other, q, words, &mut work) {
                return false;
            }
            // A ligature's characters a root names: nothing in the arena points to them.
            if p.kind == Kind::Lig && p.sub >> 2 > 0 {
                a.ligs.off(p.b as usize, size_class((p.sub >> 2) as usize).1);
                b.ligs.off(q.b as usize, size_class((q.sub >> 2) as usize).1);
            }
        }
        if !self.walk(other, words, &mut work, Some((&mut a, &mut b))) {
            return false;
        }
        if !(a.list.same(&b.list) && a.ligs.same(&b.ligs) && a.boxes.same(&b.boxes) && a.glues.same(&b.glues) && a.rules.same(&b.rules) && a.discs.same(&b.discs) && a.inserts.same(&b.inserts)) {
            return false;
        }
        let top = |p: &Live| p.0.iter().enumerate().filter(|(_, &t)| t).map(|(i, _)| i as u32).collect::<Vec<u32>>();
        for i in top(&a.list) {
            work.push(Unit::List((i, i + 1), (i, i + 1)));
        }
        work.extend(top(&a.boxes).into_iter().map(|i| Unit::Box(i, i)));
        work.extend(top(&a.glues).into_iter().map(|i| Unit::Glue(i, i)));
        work.extend(top(&a.rules).into_iter().map(|i| Unit::Rule(i, i)));
        work.extend(top(&a.discs).into_iter().map(|i| Unit::Disc(i, i)));
        work.extend(top(&a.inserts).into_iter().map(|i| Unit::Ins(i, i)));
        for i in top(&a.ligs) {
            if self.lig_chars.at(i as usize) != other.lig_chars.at(i as usize) {
                return false;
            }
        }
        self.walk(other, words, &mut work, None)
    }

    /// The pairs on `work` and all they own alike in `self` and `other`; with `seen`, each
    /// place compared is taken off the two sides' tops.
    fn walk(&self, other: &Nodes, words: (&super::engine_words::WordStore, &super::engine_words::WordStore), work: &mut Vec<Unit>, mut seen: Option<(&mut Places, &mut Places)>) -> bool {
        while let Some(u) = work.pop() {
            if let Some((a, b)) = seen.as_mut() {
                match u {
                    Unit::List(x, y) => {
                        let ((s, t), (v, w)) = (Nodes::block(x), Nodes::block(y));
                        a.list.off(s as usize, (t - s) as usize);
                        b.list.off(v as usize, (w - v) as usize);
                    }
                    Unit::Box(i, j) => { a.boxes.off(i as usize, 1); b.boxes.off(j as usize, 1); }
                    Unit::Glue(i, j) => { a.glues.off(i as usize, 1); b.glues.off(j as usize, 1); }
                    Unit::Rule(i, j) => { a.rules.off(i as usize, 1); b.rules.off(j as usize, 1); }
                    Unit::Disc(i, j) => { a.discs.off(i as usize, 1); b.discs.off(j as usize, 1); }
                    Unit::Ins(i, j) => { a.inserts.off(i as usize, 1); b.inserts.off(j as usize, 1); }
                }
            }
            let ok = match u {
                Unit::List(x, y) => x.1 - x.0 == y.1 - y.0 && (0..x.1 - x.0).all(|k| match (self.list.at((x.0 + k) as usize), other.list.at((y.0 + k) as usize)) {
                    (Some(p), Some(q)) => self.same_node(p, other, q, words, work),
                    _ => false,
                }),
                Unit::Box(i, j) => match (self.boxes.at(i as usize), other.boxes.at(j as usize)) {
                    (Some(p), Some(q)) => {
                        work.push(Unit::List(p.list, q.list));
                        BoxRec { list: (0, 0), ..*p } == BoxRec { list: (0, 0), ..*q }
                    }
                    _ => false,
                },
                Unit::Glue(i, j) => matches!((self.glues.at(i as usize), other.glues.at(j as usize)), (Some(p), Some(q)) if p == q),
                Unit::Rule(i, j) => matches!((self.rules.at(i as usize), other.rules.at(j as usize)), (Some(p), Some(q)) if p == q),
                Unit::Disc(i, j) => match (self.discs.at(i as usize), other.discs.at(j as usize)) {
                    (Some(&[p0, p1]), Some(&[q0, q1])) => {
                        work.extend([Unit::List(p0, q0), Unit::List(p1, q1)]);
                        true
                    }
                    _ => false,
                },
                Unit::Ins(i, j) => match (self.inserts.at(i as usize), other.inserts.at(j as usize)) {
                    (Some(p), Some(q)) => {
                        work.push(Unit::List(p.list, q.list));
                        InsRec { list: (0, 0), ..*p } == InsRec { list: (0, 0), ..*q }
                    }
                    _ => false,
                },
            };
            if !ok {
                return false;
            }
        }
        true
    }

    /// Node `p` here and node `q` of `other` alike, what they own put on `work` to compare.
    fn same_node(&self, p: &Node, other: &Nodes, q: &Node, words: (&super::engine_words::WordStore, &super::engine_words::WordStore), work: &mut Vec<Unit>) -> bool {
        if (p.kind, p.sub, p.font) != (q.kind, q.sub, q.font) {
            return false;
        }
        match p.kind {
            Kind::Box => work.push(Unit::Box(p.a as u32, q.a as u32)),
            Kind::Rule => work.push(Unit::Rule(p.a as u32, q.a as u32)),
            Kind::Whatsit if super::engine_whatsit::sized(p.sub) => work.push(Unit::Rule(p.a as u32, q.a as u32)),
            Kind::Glue => {
                work.push(Unit::Glue(p.a as u32, q.a as u32));
                match leader(p.sub) {
                    Some(true) => work.push(Unit::Box(p.b as u32, q.b as u32)),
                    Some(false) => work.push(Unit::Rule(p.b as u32, q.b as u32)),
                    None => return p.b == q.b,
                }
                return true;
            }
            Kind::Lig if p.sub >> 2 > 0 => {
                let n = (p.sub >> 2) as usize;
                return p.a == q.a && (0..n).all(|k| self.lig_chars.at(p.b as usize + k).is_some_and(|c| other.lig_chars.at(q.b as usize + k) == Some(c)));
            }
            Kind::Disc if p.a >= 0 && q.a >= 0 => work.push(Unit::Disc(p.a as u32, q.a as u32)),
            Kind::Ins => work.push(Unit::Ins(p.a as u32, q.a as u32)),
            Kind::Adjust => work.push(Unit::List((p.a as u32, p.b as u32), (q.a as u32, q.b as u32))),
            Kind::Word => return p.b == q.b && words.0.same_entry(p.a as u32, words.1, q.a as u32),
            _ => return p.a == q.a && p.b == q.b,
        }
        p.b == q.b || matches!(p.kind, Kind::Adjust)
    }

    /// Of the places in use outside some trees (`p`), those nothing among them points to.
    fn tops(&self, p: &mut Places) {
        let mut child = Places::none(self);
        let block = |l: (u32, u32)| Nodes::block(l);
        let list_child = |c: &mut Places, l: (u32, u32)| {
            let (s, t) = block(l);
            c.list.on(s as usize, (t - s) as usize);
        };
        for i in 0..p.list.0.len() {
            if !p.list.0[i] {
                continue;
            }
            let Some(n) = self.list.at(i).copied() else { continue };
            match n.kind {
                Kind::Box => child.boxes.on(n.a as usize, 1),
                Kind::Rule => child.rules.on(n.a as usize, 1),
                Kind::Whatsit if super::engine_whatsit::sized(n.sub) => child.rules.on(n.a as usize, 1),
                Kind::Glue => {
                    child.glues.on(n.a as usize, 1);
                    match leader(n.sub) {
                        Some(true) => child.boxes.on(n.b as usize, 1),
                        Some(false) => child.rules.on(n.b as usize, 1),
                        None => {}
                    }
                }
                Kind::Lig if n.sub >> 2 > 0 => child.ligs.on(n.b as usize, size_class((n.sub >> 2) as usize).1),
                Kind::Disc if n.a >= 0 => child.discs.on(n.a as usize, 1),
                Kind::Ins => child.inserts.on(n.a as usize, 1),
                Kind::Adjust => list_child(&mut child, (n.a as u32, n.b as u32)),
                _ => {}
            }
        }
        for i in 0..p.boxes.0.len() {
            if p.boxes.0[i] {
                if let Some(r) = self.boxes.at(i) {
                    list_child(&mut child, r.list);
                }
            }
        }
        for i in 0..p.inserts.0.len() {
            if p.inserts.0[i] {
                if let Some(r) = self.inserts.at(i) {
                    list_child(&mut child, r.list);
                }
            }
        }
        for i in 0..p.discs.0.len() {
            if p.discs.0[i] {
                if let Some(&[pre, post]) = self.discs.at(i) {
                    list_child(&mut child, pre);
                    list_child(&mut child, post);
                }
            }
        }
        for (t, c) in [(&mut p.list, &child.list), (&mut p.ligs, &child.ligs), (&mut p.boxes, &child.boxes), (&mut p.glues, &child.glues), (&mut p.rules, &child.rules), (&mut p.discs, &child.discs), (&mut p.inserts, &child.inserts)] {
            for (x, &y) in t.0.iter_mut().zip(c.0.iter()) {
                *x &= !y;
            }
        }
    }

    /// A copy made here of box `b` of frozen arena `src` and everything it owns (its record
    /// returned, not stored): word entries copied from `words` (`src`'s word table, live)
    /// into this table, `memo` keeping each one's new id; marks' and whatsits' token lists
    /// from `src_toks` into `toks`. `None` for what a page never holds.
    pub fn graft_box(&mut self, src: &Nodes, words: &super::engine_words::WordStore, src_toks: &super::engine_token::Tokens, toks: &mut super::engine_token::Tokens, memo: &mut std::collections::HashMap<i32, i32>, b: u32) -> Option<BoxRec> {
        let mut r = *src.boxes.at(b as usize)?;
        r.list = self.graft_list(src, words, src_toks, toks, memo, r.list)?;
        Some(r)
    }

    fn graft_list(&mut self, src: &Nodes, words: &super::engine_words::WordStore, src_toks: &super::engine_token::Tokens, toks: &mut super::engine_token::Tokens, memo: &mut std::collections::HashMap<i32, i32>, l: (u32, u32)) -> Option<(u32, u32)> {
        let mut out = Vec::with_capacity(l.1.saturating_sub(l.0) as usize);
        for i in l.0..l.1 {
            let mut x = *src.list.at(i as usize)?;
            // A token list still in the same place here stays shared; else it is copied.
            let span = |x: &Node, toks: &mut super::engine_token::Tokens| -> Option<(i32, i32)> {
                let at = super::engine_token::Span { start: x.a as u32, end: x.b as u32 };
                let list = src_toks.span_of(at)?;
                if toks.span_of(at).is_some_and(|here| here == list) {
                    return Some((x.a, x.b));
                }
                let s = toks.store(&list);
                Some((s.start as i32, s.end as i32))
            };
            match x.kind {
                Kind::Box => {
                    let r = self.graft_box(src, words, src_toks, toks, memo, x.a as u32)?;
                    x.a = self.add_box(r) as i32;
                }
                Kind::Rule => x.a = self.add_rule(*src.rules.at(x.a as usize)?) as i32,
                Kind::Glue => {
                    x.a = self.glue_node(*src.glues.at(x.a as usize)?, x.sub).a;
                    match leader(x.sub) {
                        Some(true) => {
                            let r = self.graft_box(src, words, src_toks, toks, memo, x.b as u32)?;
                            x.b = self.add_box(r) as i32;
                        }
                        Some(false) => x.b = self.add_rule(*src.rules.at(x.b as usize)?) as i32,
                        None => {}
                    }
                }
                Kind::Lig if x.sub >> 2 > 0 => {
                    let chars: Option<Vec<u8>> = (x.b as usize..x.b as usize + (x.sub >> 2) as usize).map(|k| src.lig_chars.at(k).copied()).collect();
                    x.b = self.lig_node(x.font, x.a as u8, &chars?, x.sub & 3).b;
                }
                Kind::Disc if x.a >= 0 => {
                    let [pre, post] = *src.discs.at(x.a as usize)?;
                    let d = [self.graft_list(src, words, src_toks, toks, memo, pre)?, self.graft_list(src, words, src_toks, toks, memo, post)?];
                    x.a = self.add_disc(d) as i32;
                }
                Kind::Ins => {
                    let mut r = *src.inserts.at(x.a as usize)?;
                    r.list = self.graft_list(src, words, src_toks, toks, memo, r.list)?;
                    x.a = self.add_insert(r) as i32;
                }
                Kind::Adjust => {
                    let (a, b) = self.graft_list(src, words, src_toks, toks, memo, (x.a as u32, x.b as u32))?;
                    (x.a, x.b) = (a as i32, b as i32);
                }
                Kind::Whatsit if super::engine_whatsit::sized(x.sub) => x.a = self.add_rule(*src.rules.at(x.a as usize)?) as i32,
                Kind::Whatsit if super::engine_whatsit::holds_tokens(x.sub) => (x.a, x.b) = span(&x, toks)?,
                Kind::Mark => (x.a, x.b) = span(&x, toks)?,
                Kind::Word => {
                    let a = x.a;
                    x.a = *memo.entry(a).or_insert_with(|| self.words.copy_entry(words, a as u32) as i32);
                }
                Kind::Noad => return None,
                _ => {}
            }
            out.push(x);
        }
        Some(self.store(&out))
    }

    /// Whether every place of `e` belongs to the trees of boxes `roots` in this frozen arena:
    /// their records, list blocks, glue, rules, ligature characters, discretionaries and
    /// insertions.
    pub fn owns(&self, roots: &[u32], e: &NodeEdits) -> bool {
        let mut o = Owned::default();
        let mut stack: Vec<(u32, u32)> = Vec::new();
        let boxed = |b: u32, o: &mut Owned, stack: &mut Vec<(u32, u32)>| {
            o.boxes.push(b as usize);
            if let Some(r) = self.boxes.at(b as usize) {
                stack.push(r.list);
            }
        };
        for &b in roots {
            boxed(b, &mut o, &mut stack);
        }
        while let Some(l) = stack.pop() {
            let (s, t) = Nodes::block(l);
            o.list.push((s as usize, t as usize));
            for i in l.0..l.1 {
                let Some(n) = self.list.at(i as usize).copied() else { return false };
                match n.kind {
                    Kind::Box => boxed(n.a as u32, &mut o, &mut stack),
                    Kind::Glue => {
                        o.glues.push(n.a as usize);
                        match leader(n.sub) {
                            Some(true) => boxed(n.b as u32, &mut o, &mut stack),
                            Some(false) => o.rules.push(n.b as usize),
                            None => {}
                        }
                    }
                    Kind::Rule => o.rules.push(n.a as usize),
                    Kind::Lig => o.ligs.push((n.b as usize, n.b as usize + (n.sub >> 2) as usize)),
                    Kind::Disc if n.a >= 0 => {
                        o.discs.push(n.a as usize);
                        if let Some(&[pre, post]) = self.discs.at(n.a as usize) {
                            stack.extend([pre, post]);
                        }
                    }
                    Kind::Ins => {
                        o.inserts.push(n.a as usize);
                        if let Some(r) = self.inserts.at(n.a as usize) {
                            stack.push(r.list);
                        }
                    }
                    Kind::Adjust => stack.push((n.a as u32, n.b as u32)),
                    _ => {}
                }
            }
        }
        let within = |r: &[(usize, usize)], i: usize| r.iter().any(|&(s, t)| (s..t).contains(&i));
        e.list.iter().all(|(i, _)| within(&o.list, *i)) && e.lig_chars.iter().all(|(i, _)| within(&o.ligs, *i)) && e.boxes.iter().all(|(i, _)| o.boxes.contains(i)) && e.glues.iter().all(|(i, _)| o.glues.contains(i)) && e.rules.iter().all(|(i, _)| o.rules.contains(i)) && e.discs.iter().all(|(i, _)| o.discs.contains(i)) && e.inserts.iter().all(|(i, _)| o.inserts.contains(i))
    }

    /// A live copy of a frozen one.
    pub fn thaw(&self) -> Nodes {
        let Nodes { list, boxes, glues, rules, lig_chars, words, discs, inserts, free, work: _, copy_buf: _, #[cfg(feature = "ownership-check")] origin: _ } = self;
        Nodes { list: list.thaw(), boxes: boxes.thaw(), glues: glues.thaw(), rules: rules.thaw(), lig_chars: lig_chars.thaw(), words: words.thaw(), discs: discs.thaw(), inserts: inserts.thaw(), free: free.clone(), ..Default::default() }
    }

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
        self.list.slice_mut(start..start + nodes.len()).copy_from_slice(nodes);
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

    /// The characters a ligature was made from (a word entry's ligature: from the entry's).
    pub fn lig_from(&self, n: &Node) -> &[u8] {
        let at = n.b as usize;
        &self.lig_chars[at..at + (n.sub >> 2) as usize]
    }

    /// The nodes word `n` stands for. A ligature among them keeps its characters in the
    /// entry: read them with [`WordStore::lig_from`](super::engine_words::WordStore::lig_from).
    #[inline]
    pub fn word_items(&self, n: &Node) -> &[Node] {
        self.words.items(n.a as u32)
    }

    /// `list` with every word replaced by the nodes it stands for, each ligature with
    /// characters of its own, as the main loop would have made them.
    pub fn expand_words(&mut self, list: &mut Vec<Node>) {
        self.expand_words_if(list, false);
    }

    /// [`Self::expand_words`] for the words holding a discretionary alone (`only_disc`), or
    /// for every word.
    pub fn expand_words_if(&mut self, list: &mut Vec<Node>, only_disc: bool) {
        let expands = |ws: &super::engine_words::WordStore, n: &Node| n.kind == Kind::Word && (!only_disc || ws.rec(n.a as u32).disc);
        if !list.iter().any(|n| expands(&self.words, n)) {
            return;
        }
        let old = std::mem::take(list);
        list.reserve(old.len() * 4);
        for n in old {
            if !expands(&self.words, &n) {
                list.push(n);
                continue;
            }
            let (s, e) = self.words.range(n.a as u32);
            self.make_items(s as u32, e as u32, list);
        }
    }

    /// The nodes of word-store items `s..e`, each ligature and discretionary with what it
    /// owns made anew.
    pub fn make_items(&mut self, s: u32, e: u32, out: &mut Vec<Node>) {
        for i in s..e {
            let m = self.words.items[i as usize];
            match m.kind {
                Kind::Lig => {
                    let mut from = [0u8; 64];
                    let src = self.words.lig_from(&m);
                    from[..src.len()].copy_from_slice(src);
                    let lig = self.lig_node(m.font, m.a as u8, &from[..src.len()], m.sub & 3);
                    out.push(lig);
                }
                Kind::Disc if m.a >= 0 => {
                    let [pre, post] = self.words.discs[m.a as usize];
                    let mut list = Vec::new();
                    self.make_items(pre.0, pre.1, &mut list);
                    let pre = self.store(&list);
                    list.clear();
                    self.make_items(post.0, post.1, &mut list);
                    let post = self.store(&list);
                    out.push(Node { a: self.add_disc([pre, post]) as i32, ..m });
                }
                _ => out.push(m),
            }
        }
    }

    #[inline]
    pub fn glue(&self, n: &Node) -> Glue {
        self.glues[n.a as usize]
    }
}

/// §212: one level of the semantic nest: the mode, where its list starts in the build
/// buffer, and `prev_depth` (vertical) or `space_factor` (horizontal) in `aux`.
#[derive(PartialEq, Clone, Copy, Debug)]
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
