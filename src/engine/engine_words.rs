//! GDL's word table: the main loop's result for a word read straight from the source.
//!
//! §1034–§1040 turn a word's characters into character, ligature and kern nodes (and an
//! empty discretionary after the font's hyphen character in a paragraph). That result
//! depends only on the font (its lig/kern program and boundary character), its hyphen
//! character, whether the list is a paragraph, and the characters, when the word starts
//! afresh (no `\noboundary` before it) and ends at a token that is not a character. So when
//! the next characters of the file being read form such a word, its nodes are made once and
//! reused every later time: the characters are neither tokenized nor run through the
//! lig/kern program again. The space factor still follows each character's `\sfcode`
//! (§1034), which may change between uses.
//!
//! In the native job's paragraphs a word is one [`Kind::Word`] node standing for its
//! entry, whose width, height and depth are summed once: line breaking, packing and
//! shipping read the entry instead of every character. Elsewhere (the engine, restricted
//! horizontal lists, a word with a discretionary) the entry's nodes are copied.
//!
//! `RT_WORD_CHECK=1` makes every word the slow way as well and stops at the first that
//! differs from its entry; `RT_WORD_NODES=0` copies the nodes everywhere.

use super::engine_chunk::Chunked;
use super::engine_catcode::Cat;
use super::engine_eqtb::{Reg, Region};
use super::engine_hyph::WordHash;
use super::engine_names::END_LINE_CHAR;
use super::engine_node::{Kind, Node};
use super::engine_run::{Machine, Mode};
use super::engine_token::Token;
use std::collections::HashMap;

/// A word entry's nodes and its natural size (§656 for its characters and kerns).
#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub struct WordRec {
    pub start: u32,
    pub end: u32,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    /// The nodes hold a discretionary (a break), so the word is never one node.
    pub disc: bool,
}

codec_struct!(WordRec { start, end, width, height, depth, disc });

/// Every entry's nodes; a ligature's `b` is where its characters start in `ligs`. A
/// hyphenated form's discretionary has its `a` in `discs`, whose pre- and post-break
/// texts are ranges of `items` too.
#[derive(PartialEq, Debug, Default, Clone)]
pub struct WordStore {
    pub recs: Chunked<WordRec>,
    pub items: Chunked<Node>,
    pub ligs: Chunked<u8>,
    pub discs: Chunked<[(u32, u32); 2]>,
    /// Per entry (indexed by its id; absent or empty for a plain word), the range of its
    /// discretionaries' rows in `rows`, which only the second pass reads.
    pub brk: Chunked<(u32, u32)>,
    pub rows: Chunked<BreakRow>,
    /// `capture_list`'s lists while they are read, a stack: an inner list is written past
    /// its outer one and taken off when it moves to `items`.
    stack: Vec<Node>,
}

codec_struct!(WordStore { recs, items, ligs, discs, brk, rows, stack });

/// One discretionary of a hyphenated form, as §869 and §840 read it, all widths natural
/// (no glue in a word, so no stretch or shrink): read whole, one row per break.
#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub struct BreakRow {
    /// The width of the form's nodes before the discretionary (replaced ones included).
    pub before: i32,
    /// The pre-break text's width (§869 `disc_width`).
    pub pre: i32,
    /// The width of the `sub` nodes it replaces, and of its post-break text (§840).
    pub rep: i32,
    pub post: i32,
    /// The discretionary's place among the form's items.
    pub at: u8,
    /// [`BreakRow::PRE_EMPTY`], [`BreakRow::POST_EMPTY`], [`BreakRow::ENDS_WORD`].
    pub flags: u8,
}

codec_struct!(BreakRow { before, pre, rep, post, at, flags });

impl BreakRow {
    /// An empty pre-break text: `\exhyphenpenalty`, no pre-break width (§869).
    pub const PRE_EMPTY: u8 = 1;
    /// An empty post-break text: §840 discards what follows the replaced nodes.
    pub const POST_EMPTY: u8 = 2;
    /// The replaced nodes reach the form's end: what §840 then reads is the paragraph's.
    pub const ENDS_WORD: u8 = 4;
}

/// A live copy of a frozen word store, by its chunk lists' addresses ([`WordStore::thawed`]).
type ThawedCopy = ([Option<usize>; 6], std::rc::Rc<WordStore>);

impl WordStore {
    /// A checkpoint's copy, sharing what did not change since the last one, and a live copy
    /// of one (`engine_chunk`); the scratch stack is empty between commands.
    pub fn freeze(&mut self) -> WordStore {
        let WordStore { recs, items, ligs, discs, brk, rows, stack: _ } = self;
        WordStore { recs: recs.freeze(), items: items.freeze(), ligs: ligs.freeze(), discs: discs.freeze(), brk: brk.freeze(), rows: rows.freeze(), stack: Vec::new() }
    }

    pub fn thaw(&self) -> WordStore {
        let WordStore { recs, items, ligs, discs, brk, rows, stack: _ } = self;
        WordStore { recs: recs.thaw(), items: items.thaw(), ligs: ligs.thaw(), discs: discs.thaw(), brk: brk.thaw(), rows: rows.thaw(), stack: Vec::new() }
    }

    /// A live copy of this frozen store to read, shared with the last few asked for: the
    /// cutoff tests of a save thaw the same checkpoints' stores again and again. A copy
    /// holds the chunk lists it is keyed by, so a key is never reused while it is kept.
    pub fn thawed(&self) -> std::rc::Rc<WordStore> {
        thread_local! {
            static THAWED: std::cell::RefCell<Vec<ThawedCopy>> = const { std::cell::RefCell::new(Vec::new()) };
        }
        let key = [self.recs.frozen_id(), self.items.frozen_id(), self.ligs.frozen_id(), self.discs.frozen_id(), self.brk.frozen_id(), self.rows.frozen_id()];
        if key.iter().any(Option::is_none) {
            return std::rc::Rc::new(self.thaw());
        }
        THAWED.with_borrow_mut(|c| {
            if let Some(k) = c.iter().position(|(k, _)| *k == key) {
                let hit = c.remove(k);
                c.push(hit);
            } else {
                if c.len() == 4 {
                    c.remove(0);
                }
                c.push((key, std::rc::Rc::new(self.thaw())));
            }
            c.last().map(|(_, w)| w.clone()).unwrap_or_else(|| unreachable!())
        })
    }

    pub fn thaw_into(&self, live: &mut WordStore) {
        let WordStore { recs, items, ligs, discs, brk, rows, stack: _ } = self;
        recs.thaw_into(&mut live.recs);
        items.thaw_into(&mut live.items);
        ligs.thaw_into(&mut live.ligs);
        discs.thaw_into(&mut live.discs);
        brk.thaw_into(&mut live.brk);
        rows.thaw_into(&mut live.rows);
        live.stack.clear();
    }

    #[inline]
    pub fn range(&self, id: u32) -> (usize, usize) {
        let r = &self.recs[id as usize];
        (r.start as usize, r.end as usize)
    }

    #[inline]
    pub fn items(&self, id: u32) -> &[Node] {
        let (s, e) = self.range(id);
        &self.items[s..e]
    }

    #[inline]
    pub fn rec(&self, id: u32) -> &WordRec {
        &self.recs[id as usize]
    }

    /// The characters an entry's ligature was made from.
    pub fn lig_from(&self, n: &Node) -> &[u8] {
        let at = n.b as usize;
        &self.ligs[at..at + (n.sub >> 2) as usize]
    }

    /// The store's lengths, to undo what [`Self::capture`] added.
    fn marks(&self) -> [usize; 5] {
        [self.recs.len(), self.items.len(), self.ligs.len(), self.discs.len(), self.rows.len()]
    }

    fn truncate(&mut self, [r, i, l, d, w]: [usize; 5]) {
        self.recs.truncate(r);
        self.items.truncate(i);
        self.ligs.truncate(l);
        self.discs.truncate(d);
        self.rows.truncate(w);
        self.brk.truncate(r);
    }

    /// Entry `id`'s break rows (none for a plain word).
    #[inline]
    pub fn rows(&self, id: u32) -> &[BreakRow] {
        self.brk.get(id as usize).map_or(&[], |&(s, e)| &self.rows[s as usize..e as usize])
    }

    /// A hyphenated word as a new entry, its size from `dims`: this store's items `own[0]`,
    /// then `list` (nodes §903 made, owning what is in `sc`), then this store's items `own[1]`.
    /// The kept items are copied here directly, never made as nodes. `None` for a node an
    /// entry cannot hold (the store is left as it was).
    fn capture(&mut self, sc: &Scratch, own: [(u32, u32); 2], list: &[Node], dims: &impl Fn(&Node) -> [i32; 4]) -> Option<u32> {
        let marks = self.marks();
        let base = self.stack.len();
        let mut main = || {
            for i in own[0].0..own[0].1 {
                self.push_own(i)?;
            }
            self.push_nodes(sc, list)?;
            for i in own[1].0..own[1].1 {
                self.push_own(i)?;
            }
            let s = self.items.len() as u32;
            self.items.extend_from_slice(&self.stack[base..]);
            self.stack.truncate(base);
            Some((s, self.items.len() as u32))
        };
        let main = main();
        let Some((start, end)) = main else {
            self.truncate(marks);
            self.stack.clear();
            return None;
        };
        let width_of = |ws: &Self, (s, e): (u32, u32)| -> i32 {
            ws.items[s as usize..e as usize].iter().map(|n| match n.kind {
                Kind::Char | Kind::Lig => dims(n)[0],
                Kind::Kern => n.a,
                _ => 0,
            }).sum()
        };
        let (mut width, mut height, mut depth) = (0, 0, 0);
        let row_start = self.rows.len() as u32;
        let len = (end - start) as usize;
        if len > u8::MAX as usize {
            self.truncate(marks);
            return None;
        }
        for (k, i) in (start..end).enumerate() {
            let n = self.items[i as usize];
            match n.kind {
                Kind::Char | Kind::Lig => {
                    let [w, h, d, _] = dims(&n);
                    height = height.max(h);
                    depth = depth.max(d);
                    width += w;
                }
                Kind::Kern => width += n.a,
                Kind::Disc => {
                    let r = n.sub as usize;
                    let (pre, post) = if n.a >= 0 { let [p, q] = self.discs[n.a as usize]; (width_of(self, p), Some((width_of(self, q), q.0 == q.1, p.0 == p.1))) } else { (0, None) };
                    let (post, post_empty, pre_empty) = post.unwrap_or((0, true, true));
                    let rep_end = (i + 1 + r as u32).min(end);
                    let rep = width_of(self, (i + 1, rep_end));
                    let mut flags = 0;
                    if pre_empty {
                        flags |= BreakRow::PRE_EMPTY;
                    }
                    if post_empty {
                        flags |= BreakRow::POST_EMPTY;
                    }
                    if k + 1 + r >= len {
                        flags |= BreakRow::ENDS_WORD;
                    }
                    self.rows.push(BreakRow { before: width, pre, rep, post, at: k as u8, flags });
                }
                _ => {}
            }
        }
        let id = self.recs.len() as u32;
        self.recs.push(WordRec { start, end, width, height, depth, disc: true });
        if self.brk.len() < id as usize {
            self.brk.resize(id as usize, (0, 0));
        }
        self.brk.push((row_start, self.rows.len() as u32));
        Some(id)
    }

    /// `list`'s nodes appended to `items` (a discretionary's texts first): their range.
    fn capture_list(&mut self, sc: &Scratch, list: &[Node]) -> Option<(u32, u32)> {
        let base = self.stack.len();
        self.push_nodes(sc, list)?;
        let s = self.items.len() as u32;
        self.items.extend_from_slice(&self.stack[base..]);
        self.stack.truncate(base);
        Some((s, self.items.len() as u32))
    }

    /// This store's items `s..e` appended to `items` again, as [`Self::capture_list`] would
    /// append the nodes `Nodes::make_items` makes of them: their range.
    fn own_list(&mut self, (s, e): (u32, u32)) -> Option<(u32, u32)> {
        let base = self.stack.len();
        for i in s..e {
            self.push_own(i)?;
        }
        let s = self.items.len() as u32;
        self.items.extend_from_slice(&self.stack[base..]);
        self.stack.truncate(base);
        Some((s, self.items.len() as u32))
    }

    /// Item `i` of this store onto `stack`, what it owns copied (a ligature's characters,
    /// a discretionary's texts appended to `items`), as `capture_list` takes the node
    /// `Nodes::make_items` makes of it.
    fn push_own(&mut self, i: u32) -> Option<()> {
        let n = self.items[i as usize];
        match n.kind {
            Kind::Char => self.stack.push(n),
            Kind::Kern if n.sub == 0 => self.stack.push(n),
            Kind::Lig => {
                let at = self.ligs.len();
                let from = n.b as usize;
                self.ligs.extend_from_within(from..from + (n.sub >> 2) as usize);
                self.stack.push(Node { b: at as i32, ..n });
            }
            Kind::Disc if n.a < 0 => self.stack.push(n),
            Kind::Disc => {
                let [pre, post] = self.discs[n.a as usize];
                let pre = self.own_list(pre)?;
                let post = self.own_list(post)?;
                self.discs.push([pre, post]);
                self.stack.push(Node { a: self.discs.len() as i32 - 1, ..n });
            }
            _ => return None,
        }
        Some(())
    }

    /// `list`'s nodes onto `stack` (a discretionary's texts appended to `items`).
    fn push_nodes(&mut self, sc: &Scratch, list: &[Node]) -> Option<()> {
        for n in list {
            match n.kind {
                Kind::Char => self.stack.push(*n),
                // A kern §903 makes is a font kern; another would be a break (`kern_break`).
                Kind::Kern if n.sub == 0 => self.stack.push(*n),
                Kind::Lig => {
                    let at = self.ligs.len() as i32;
                    self.ligs.extend_from_slice(sc.lig_from(n));
                    self.stack.push(Node { b: at, ..*n });
                }
                Kind::Disc if n.a < 0 => self.stack.push(*n),
                Kind::Disc => {
                    let [pre, post] = sc.texts(n.a as usize);
                    let pre = self.capture_list(sc, pre)?;
                    let post = self.capture_list(sc, post)?;
                    self.discs.push([pre, post]);
                    self.stack.push(Node { a: self.discs.len() as i32 - 1, ..*n });
                }
                _ => return None,
            }
        }
        Some(())
    }

    /// Whether entry `a` here and entry `b` of live store `other` stand for the same nodes
    /// (a watch session's early cutoff: two runs give one word different ids).
    pub fn same_entry(&self, a: u32, other: &WordStore, b: u32) -> bool {
        let (Some(x), Some(y)) = (self.recs.get(a as usize), other.recs.get(b as usize)) else { return false };
        if (x.width, x.height, x.depth, x.disc) != (y.width, y.height, y.depth, y.disc) || self.rows(a) != other.rows(b) {
            return false;
        }
        let (i, j) = (&self.items[x.start as usize..x.end as usize], &other.items[y.start as usize..y.end as usize]);
        if i == j && !i.iter().any(|n| n.kind == Kind::Lig || (n.kind == Kind::Disc && n.a >= 0)) {
            return true;
        }
        let (mut p, mut q) = (Vec::new(), Vec::new());
        self.text((x.start, x.end), &mut p);
        other.text((y.start, y.end), &mut q);
        p == q
    }

    /// Entry `id` of live store `from` appended here as a new entry: its id.
    pub fn copy_entry(&mut self, from: &WordStore, id: u32) -> u32 {
        let r = from.recs[id as usize];
        let (start, end) = self.copy_items(from, (r.start, r.end));
        let new = self.recs.len() as u32;
        self.recs.push(WordRec { start, end, ..r });
        let rows = from.rows(id);
        if !rows.is_empty() {
            let at = self.rows.len() as u32;
            self.rows.extend_from_slice(rows);
            if self.brk.len() < new as usize {
                self.brk.resize(new as usize, (0, 0));
            }
            self.brk.push((at, self.rows.len() as u32));
        }
        new
    }

    /// Item range `r` of `from` appended here, a discretionary's texts first (as
    /// [`Self::capture_list`] lays them out): its range.
    fn copy_items(&mut self, from: &WordStore, (s, e): (u32, u32)) -> (u32, u32) {
        let mut out = Vec::with_capacity((e - s) as usize);
        for n in &from.items[s as usize..e as usize] {
            match n.kind {
                Kind::Lig => {
                    let at = self.ligs.len() as i32;
                    self.ligs.extend_from_slice(from.lig_from(n));
                    out.push(Node { b: at, ..*n });
                }
                Kind::Disc if n.a >= 0 => {
                    let [pre, post] = from.discs[n.a as usize];
                    let pre = self.copy_items(from, pre);
                    let post = self.copy_items(from, post);
                    self.discs.push([pre, post]);
                    out.push(Node { a: self.discs.len() as i32 - 1, ..*n });
                }
                _ => out.push(*n),
            }
        }
        let at = self.items.len() as u32;
        self.items.extend_from_slice(&out);
        (at, self.items.len() as u32)
    }

    /// A canonical text of item range `r`, to compare two entries.
    fn text(&self, (s, e): (u32, u32), out: &mut Vec<i64>) {
        for n in &self.items[s as usize..e as usize] {
            out.extend([n.kind as i64, n.sub as i64, n.font as i64]);
            match n.kind {
                Kind::Lig => out.extend(self.lig_from(n).iter().map(|&c| c as i64).chain([n.a as i64])),
                Kind::Disc if n.a >= 0 => {
                    let [pre, post] = self.discs[n.a as usize];
                    out.push(-1);
                    self.text(pre, out);
                    out.push(-2);
                    self.text(post, out);
                    out.push(-3);
                }
                _ => out.push(n.a as i64),
            }
        }
    }
}

/// The ligatures' characters and the discretionaries' texts of nodes §903 made for a word
/// of the store (`Machine::hyphenate_word`), kept here instead of in the node store: the
/// entry made of them copies them, and nothing is made only to be freed.
#[derive(Default, PartialEq, Clone)]
pub struct Scratch {
    lists: Vec<Node>,
    ligs: Vec<u8>,
    discs: Vec<[(u32, u32); 2]>,
}

impl Scratch {
    pub(super) fn clear(&mut self) {
        self.lists.clear();
        self.ligs.clear();
        self.discs.clear();
    }

    /// A ligature of `c` in font `f` made from `from`, as `Nodes::lig_node` makes it.
    pub(super) fn lig(&mut self, f: u16, c: u8, from: &[u8], hits: u8) -> Node {
        let at = self.ligs.len() as i32;
        self.ligs.extend_from_slice(from);
        Node { kind: Kind::Lig, sub: (from.len().min(63) as u8) << 2 | hits, font: f, a: c as i32, b: at }
    }

    /// A discretionary's texts: its index.
    pub(super) fn disc(&mut self, pre: &[Node], post: &[Node]) -> u32 {
        let s = self.lists.len() as u32;
        self.lists.extend_from_slice(pre);
        let m = self.lists.len() as u32;
        self.lists.extend_from_slice(post);
        self.discs.push([(s, m), (m, self.lists.len() as u32)]);
        self.discs.len() as u32 - 1
    }

    pub(super) fn lig_from(&self, n: &Node) -> &[u8] {
        let at = n.b as usize;
        &self.ligs[at..at + (n.sub >> 2) as usize]
    }

    /// Discretionary `d`'s texts.
    pub(super) fn texts(&self, d: usize) -> [&[Node]; 2] {
        let [p, q] = self.discs[d];
        [&self.lists[p.0 as usize..p.1 as usize], &self.lists[q.0 as usize..q.1 as usize]]
    }
}

/// The characters after the first that a word's key packs into a `u128`.
const SHORT: usize = 9;

/// A `u128` key hashed by one folded 64×64-bit multiply of its halves.
#[derive(Clone, Copy, Default)]
pub struct PackedHash;

impl std::hash::BuildHasher for PackedHash {
    type Hasher = Folded;
    fn build_hasher(&self) -> Folded {
        Folded(0)
    }
}

pub struct Folded(u64);

impl std::hash::Hasher for Folded {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
    }
    fn write_u128(&mut self, v: u128) {
        let m = ((v as u64) ^ 0x243f_6a88_85a3_08d3) as u128 * (((v >> 64) as u64) ^ 0x1319_8a2e_0370_7344) as u128;
        self.0 = (m as u64) ^ ((m >> 64) as u64);
    }
    /// A packed `u64` key (`Words::forms`): one folded multiply, not eight FNV rounds.
    fn write_u64(&mut self, v: u64) {
        let m = (v ^ 0x243f_6a88_85a3_08d3) as u128 * 0x1319_8a2e_0370_7344u128;
        self.0 = (m as u64) ^ ((m >> 64) as u64);
    }
}

#[derive(PartialEq, Clone)]
pub struct Words {
    /// Key: font, hyphen character, paragraph, the characters; value: the entry. Words of
    /// up to [`SHORT`] characters after the first are in `short`, keyed as one `u128`.
    map: HashMap<Box<[u8]>, u32, WordHash>,
    short: HashMap<u128, u32, PackedHash>,
    short_key: Option<u128>,
    /// The key being looked up, and the word's characters after the first, reused.
    key: Vec<u8>,
    run: Vec<u8>,
    /// `hyphenate_word`'s list, kept for their room.
    temp: Vec<Node>,
    /// An empty word store, swapped with the machine's while `hyphenate_word` writes into
    /// it (`mem::take` made a fresh store, six shared-chunk allocations, per new form).
    spare: WordStore,
    check: bool,
    nodes: bool,
    /// Per entry, language and hyphen minima: what §894–§903 make of the word.
    forms: HashMap<u64, Form, PackedHash>,
}

codec_struct!(Words { map, short, short_key, key, run, temp, check, nodes, forms; spare });

#[derive(PartialEq, Clone, Copy)]
enum Form {
    /// No hyphen: the word stays one node.
    Whole,
    /// The hyphenated form, an entry of its own (holding discretionaries).
    Word(u32),
    /// Nodes an entry cannot hold: hyphenated every time.
    Again,
}

codec_data_enum!(Form { 0 Whole, 1 Word(a), 2 Again });

impl Default for Words {
    fn default() -> Self {
        let check = std::env::var_os("RT_WORD_CHECK").is_some_and(|v| v == "1");
        let nodes = std::env::var_os("RT_WORD_NODES").is_none_or(|v| v != "0");
        Words { map: HashMap::default(), short: HashMap::default(), short_key: None, key: Vec::new(), run: Vec::new(), temp: Vec::new(), spare: WordStore::default(), check, nodes, forms: HashMap::default() }
    }
}

impl Machine<'_, '_> {
    /// `RT_WORD_CHECK=1`: every shortcut of the word table is checked against the slow way.
    pub(super) fn words_checked(&self) -> bool {
        self.words.check
    }

    /// The word that starts with `first` and continues with the characters next in the file,
    /// appended from the table (made and entered the first time). `false` when the word is
    /// not one the table can hold; the main loop then reads it token by token.
    ///
    /// In a native job's paragraph, the space after the word and the word after that are
    /// taken here too, as the main control takes their tokens (§1030: `char_token`, then
    /// `app_space` for the space and the main loop for the character), without a pass
    /// through the token loop for each.
    pub(super) fn word(&mut self, first: u8) -> bool {
        if !self.word_one(first) {
            return false;
        }
        if self.doc.is_none() || self.mode != Mode::Horizontal || self.inner || !self.building() {
            return true;
        }
        let elc = self.e.eqtb.word(Reg { region: Region::Int, slot: END_LINE_CHAR });
        loop {
            let cats = self.e.eqtb.cats();
            let Some(lx) = self.input.file_next() else { return true };
            let Some(next) = lx.space_then_char(cats, elc) else { return true };
            self.dont_expand = false;
            self.note_command(Token::char(Cat::Space, ' '));
            self.append_space();
            let Some((c, cat)) = next else { return true };
            self.note_command(Token::char(cat, c as char));
            if !self.word_one(c) {
                // Not a word the table holds: the main loop reads it token by token.
                self.main_loop(c);
                return true;
            }
        }
    }

    fn word_one(&mut self, first: u8) -> bool {
        if self.cancel_boundary || self.halted {
            return false;
        }
        let f = self.cur_font();
        let paragraph = self.mode == Mode::Horizontal && !self.inner;
        let font = &self.e.fonts.list[f as usize];
        let hyphen = font.hyphen_char;
        let cats = self.e.eqtb.cats();
        let Some(lx) = self.input.file_next() else { return false };
        let Some(end) = lx.word_run(cats) else { return false };
        let w = &mut self.words;
        let hyphen = match u8::try_from(hyphen) {
            Ok(h) => [1, h],
            Err(_) => [0, 0],
        };
        // A short word's key is one `u128`: font, hyphen character, paragraph, first
        // character, up to nine more and their count; compared and hashed as one value.
        let mut packed = [0u8; 16];
        let [f0, f1] = f.to_le_bytes();
        packed[..6].copy_from_slice(&[f0, f1, hyphen[0], hyphen[1], paragraph as u8, first]);
        w.run.clear();
        for (i, c) in lx.run_bytes(end).enumerate() {
            w.run.push(c);
            if i < SHORT {
                packed[6 + i] = c;
            }
        }
        let n = w.run.len();
        w.short_key = (n <= SHORT).then(|| {
            packed[15] = n as u8;
            u128::from_le_bytes(packed)
        });
        let found = match w.short_key {
            Some(k) => w.short.get(&k).copied(),
            None => {
                w.key.clear();
                w.key.extend_from_slice(&[f0, f1, hyphen[0], hyphen[1], paragraph as u8, first]);
                w.key.extend_from_slice(&w.run);
                w.map.get(w.key.as_slice()).copied()
            }
        };
        // §581: a missing character ends the main loop early; such a word is not entered.
        // An entry was made from characters the font has, which it keeps.
        if found.is_none() && (!font.metrics.exists(first) || !w.run.iter().all(|&c| font.metrics.exists(c))) {
            return false;
        }
        if paragraph {
            self.fix_language();
        }
        // A word is one node in the native job's paragraphs.
        let as_node = self.words.nodes && paragraph && self.doc.is_some();
        match found {
            Some(id) if !self.words.check => self.append_word(first, id, as_node),
            _ => {
                let start = self.cur.len();
                let run = std::mem::take(&mut self.words.run);
                self.text_run = Some((run, 0));
                self.main_loop(first);
                let (run, used) = self.text_run.take().expect("the word's run");
                debug_assert_eq!(used, n, "a word whose characters all exist is read whole");
                self.words.run = run;
                if let Some(id) = self.enter_word(start, found) {
                    if as_node && !self.e.nodes.words.rec(id).disc {
                        for k in start..self.cur.len() {
                            let x = self.cur[k];
                            self.e.nodes.flush_node(x);
                        }
                        self.cur.truncate(start);
                        self.cur.push(Node { kind: Kind::Word, sub: 0, font: f, a: id as i32, b: 0 });
                    }
                }
            }
        }
        if let Some(lx) = self.input.file_next() {
            lx.skip_run(n);
        }
        true
    }

    /// Entry `id` appended, as one node or as its nodes, and the space factor of each
    /// character (§1034).
    fn append_word(&mut self, first: u8, id: u32, as_node: bool) {
        self.adjust_space_factor(first);
        for i in 0..self.words.run.len() {
            let c = self.words.run[i];
            self.adjust_space_factor(c);
        }
        let rec = *self.e.nodes.words.rec(id);
        if as_node && !rec.disc {
            let f = self.e.nodes.words.items[rec.start as usize].font;
            self.cur.push(Node { kind: Kind::Word, sub: 0, font: f, a: id as i32, b: 0 });
            return;
        }
        let start = self.cur.len();
        self.cur.push(Node { kind: Kind::Word, sub: 0, font: 0, a: id as i32, b: 0 });
        let mut tail = self.cur.split_off(start);
        self.e.nodes.expand_words(&mut tail);
        self.cur.append(&mut tail);
    }

    /// The nodes the main loop just made from `start` entered under the current key, and
    /// the entry; `None` for nodes a word cannot hold. With `RT_WORD_CHECK=1` and an entry
    /// `found` already there, the nodes are compared with it instead.
    fn enter_word(&mut self, start: usize, found: Option<u32>) -> Option<u32> {
        let nodes = &mut self.e.nodes;
        let (ws, lig_chars) = (&mut nodes.words, &nodes.lig_chars);
        let s = ws.items.len();
        let lig_mark = ws.ligs.len();
        let (mut width, mut height, mut depth, mut disc) = (0, 0, 0, false);
        let fonts = &self.e.fonts;
        let dims = |n: &Node| u8::try_from(n.a).ok().and_then(|c| fonts.dims(n.font, c)).unwrap_or_default();
        for n in &self.cur[start..] {
            match n.kind {
                Kind::Char | Kind::Lig => {
                    let [w, h, d, _] = dims(n);
                    width += w;
                    height = height.max(h);
                    depth = depth.max(d);
                    if n.kind == Kind::Char {
                        ws.items.push(*n);
                    } else {
                        let at = ws.ligs.len() as i32;
                        ws.ligs.extend_from_slice(&lig_chars[n.b as usize..n.b as usize + (n.sub >> 2) as usize]);
                        ws.items.push(Node { b: at, ..*n });
                    }
                }
                Kind::Kern if n.sub == 0 => {
                    width += n.a;
                    ws.items.push(*n);
                }
                Kind::Disc if n.a == -1 => {
                    disc = true;
                    ws.items.push(*n);
                }
                // Not a node the main loop makes for a word: not entered.
                _ => {
                    ws.items.truncate(s);
                    ws.ligs.truncate(lig_mark);
                    return None;
                }
            }
        }
        let e = ws.items.len();
        let rec = WordRec { start: s as u32, end: e as u32, width, height, depth, disc };
        match found {
            None => {
                let id = ws.recs.len() as u32;
                ws.recs.push(rec);
                match self.words.short_key {
                    Some(k) => {
                        self.words.short.insert(k, id);
                    }
                    None => {
                        self.words.map.insert(self.words.key.as_slice().into(), id);
                    }
                }
                Some(id)
            }
            Some(id) => {
                let old = *ws.rec(id);
                let same = |a: &Node, b: &Node| (a.kind, a.sub, a.font, a.a) == (b.kind, b.sub, b.font, b.a) && (a.kind == Kind::Lig || a.b == b.b);
                let ligs = |r: (usize, usize)| -> Vec<u8> { ws.items[r.0..r.1].iter().filter(|n| n.kind == Kind::Lig).flat_map(|n| ws.lig_from(n).iter().copied()).collect() };
                let (os, oe) = (old.start as usize, old.end as usize);
                let ok = oe - os == e - s && ws.items[os..oe].iter().zip(&ws.items[s..e]).all(|(a, b)| same(a, b)) && ligs((os, oe)) == ligs((s, e)) && (old.width, old.height, old.depth, old.disc) == (width, height, depth, disc);
                assert!(ok, "RT_WORD_CHECK: the word table's entry for {:?} differs from the main loop's nodes", String::from_utf8_lossy(&self.words.run));
                ws.items.truncate(s);
                ws.ligs.truncate(lig_mark);
                Some(id)
            }
        }
    }

    /// Before §894 runs on a paragraph: the words whose hyphenation could read nodes outside
    /// them are expanded. A word is kept whole when TeX's walk from a glue can reach it
    /// only as the node right after that glue (the node before it is none of a character,
    /// ligature, font kern, whatsit or word, which §896 passes through) and, after a glue,
    /// when a glue, penalty, kern other than a font kern, insertion, adjustment or mark
    /// follows it: §896 and §897 stop at any of them as at a penalty, and §899 accepts each,
    /// so §897–§899 end at the word's end (`hyphenate_word` puts a penalty there).
    pub(super) fn expand_words_near(&mut self, par: &mut Vec<Node>) {
        let through = |n: &Node| matches!(n.kind, Kind::Char | Kind::Lig | Kind::Whatsit | Kind::Word) || (n.kind == Kind::Kern && n.sub == 0);
        let unsure = |par: &[Node], i: usize| match i.checked_sub(1).map(|j| &par[j]) {
            None => false,
            Some(p) if through(p) => true,
            Some(p) if p.kind == Kind::Glue => !par.get(i + 1).is_some_and(|n| matches!(n.kind, Kind::Glue | Kind::Penalty | Kind::Ins | Kind::Adjust | Kind::Mark) || (n.kind == Kind::Kern && n.sub != 0)),
            Some(_) => false,
        };
        if !(0..par.len()).any(|i| par[i].kind == Kind::Word && unsure(par, i)) {
            return;
        }
        let old = std::mem::take(par);
        par.reserve(old.len() * 2);
        for i in 0..old.len() {
            let n = old[i];
            if n.kind == Kind::Word && unsure(&old, i) {
                // Its nodes (`expand_words`), made straight into the paragraph.
                let (s, e) = self.e.nodes.words.range(n.a as u32);
                self.e.nodes.make_items(s as u32, e as u32, par);
            } else {
                par.push(n);
            }
        }
    }

    /// §894–§903 for the word node after glue `g`, which a glue or penalty follows
    /// ([`Machine::expand_words_near`]): the word is hyphenated as its nodes would be, alone
    /// between that glue and a penalty, which is all §896–§899 read of the paragraph. A word
    /// without hyphens stays as it is; a hyphenated one becomes the entry of its form, which
    /// replaces the word node's entry (returned). What a word becomes is kept per entry,
    /// language and hyphen minima. A form an entry cannot hold goes on `edits` as nodes.
    pub(super) fn hyphenate_word(&mut self, par: &[Node], g: usize, lang: usize, l_hyf: usize, r_hyf: usize, edits: &mut Vec<(usize, usize, Vec<Node>)>) -> Option<u32> {
        let word = par[g + 1];
        let key = (word.a as u64) << 24 | (lang as u64 & 0xFF) << 16 | (l_hyf as u64 & 0xFF) << 8 | (r_hyf as u64 & 0xFF);
        let known = self.words.forms.get(&key).copied();
        match known {
            Some(Form::Whole) if !self.words.check => return None,
            Some(Form::Word(id)) if !self.words.check => return Some(id),
            _ => {}
        }
        // §899 wants `l_hyf + r_hyf` letters in the word (§897 takes characters whose `\lccode`
        // is not 0, a ligature's included): a word with fewer is kept whole without its nodes
        // being made and hyphenated.
        if known.is_none() && !self.words.check {
            let ws = &self.e.nodes.words;
            let letters: usize = ws.items(word.a as u32).iter().map(|n| match n.kind {
                Kind::Char => usize::from(self.lc(n.a as u32) != 0),
                Kind::Lig => ws.lig_from(n).iter().filter(|&&c| self.lc(c as u32) != 0).count(),
                _ => 0,
            }).sum();
            if letters < l_hyf + r_hyf {
                self.words.forms.insert(key, Form::Whole);
                return None;
            }
        }
        super::engine_doc::piece(1,|| self.hyphenate_new_word(par, g, word, key, known, lang, l_hyf, r_hyf, edits))
    }

    /// [`Machine::hyphenate_word`] for a word whose form is not known yet (or checked).
    #[allow(clippy::too_many_arguments)]
    fn hyphenate_new_word(&mut self, par: &[Node], g: usize, word: Node, key: u64, known: Option<Form>, lang: usize, l_hyf: usize, r_hyf: usize, edits: &mut Vec<(usize, usize, Vec<Node>)>) -> Option<u32> {
        // `temp` and `nodes` are the table's, kept for their room.
        let mut temp = std::mem::take(&mut self.words.temp);
        temp.clear();
        // The glue and the word's items as they are in the store: §894-§903 read a
        // ligature's characters from the store (`try_hyphenate`'s `items`), so no node is
        // made for a word kept whole, and the nodes §903 replaces own nothing to flush.
        temp.push(par[g]);
        let (s, e) = self.e.nodes.words.range(word.a as u32);
        temp.extend_from_slice(&self.e.nodes.words.items[s..e]);
        temp.push(Node::new(Kind::Penalty, 0));
        let (mut lang, mut l_hyf, mut r_hyf) = (lang, l_hyf, r_hyf);
        let mut one = Vec::new();
        self.try_hyphenate(&temp, true, 0, &mut lang, &mut l_hyf, &mut r_hyf, &mut one);
        let last = temp.len() - 1;
        let Some((keep, hb, out)) = one.pop() else {
            assert!(!matches!(known, Some(Form::Word(..))), "RT_WORD_CHECK: a word's hyphenated form is not hyphenated again");
            self.words.forms.insert(key, Form::Whole);
            self.words.temp = temp;
            return None;
        };
        // §903: the replaced nodes go. The kept items before and after them are copied
        // into the new entry straight from the store (`temp[i]` is the store's item
        // `s + i - 1`); nodes are made of them only for a form no entry can hold.
        let own = [(s as u32, (s + keep - 1) as u32), ((s + hb) as u32, (s + last - 1) as u32)];
        self.words.temp = temp;
        let fonts = &self.e.fonts;
        let dims = |n: &Node| u8::try_from(n.a).ok().and_then(|c| fonts.dims(n.font, c)).unwrap_or_default();
        std::mem::swap(&mut self.e.nodes.words, &mut self.words.spare);
        let ws = &mut self.words.spare;
        let marks = ws.marks();
        let form = ws.capture(self.e.hyph.scratch(), own, &out, &dims).map_or(Form::Again, Form::Word);
        let form = match (known, form) {
            (Some(Form::Word(old)), Form::Word(new)) => {
                let (mut a, mut b) = (Vec::new(), Vec::new());
                let r = |id: u32| (ws.recs[id as usize].start, ws.recs[id as usize].end);
                ws.text(r(old), &mut a);
                ws.text(r(new), &mut b);
                assert!(a == b && (ws.recs[old as usize].width, ws.recs[old as usize].height, ws.recs[old as usize].depth) == (ws.recs[new as usize].width, ws.recs[new as usize].height, ws.recs[new as usize].depth), "RT_WORD_CHECK: a word's hyphenated form differs from §903's nodes");
                ws.truncate(marks);
                Form::Word(old)
            }
            (Some(Form::Whole), _) => panic!("RT_WORD_CHECK: a word kept whole is hyphenated"),
            _ => {
                self.words.forms.insert(key, form);
                form
            }
        };
        std::mem::swap(&mut self.e.nodes.words, &mut self.words.spare);
        match form {
            Form::Word(id) => {
                // The entry holds the form; what §903 made for it owns nothing outside
                // the scratch.
                self.e.hyph.give_back(out);
                Some(id)
            }
            _ => {
                let mut nodes = Vec::new();
                self.e.nodes.make_items(own[0].0, own[0].1, &mut nodes);
                let sc = std::mem::take(self.e.hyph.scratch_mut());
                self.e.nodes.make_scratch(&sc, &out, &mut nodes);
                *self.e.hyph.scratch_mut() = sc;
                self.e.nodes.make_items(own[1].0, own[1].1, &mut nodes);
                self.e.hyph.give_back(out);
                edits.push((g + 1, g + 1, nodes));
                None
            }
        }
    }
}
