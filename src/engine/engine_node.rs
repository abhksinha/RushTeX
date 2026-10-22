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
}

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
    /// Forgets every free block and slot (the oracle's collector, `engine_gc`, compacts the
    /// tables and so drops them).
    pub fn clear(&mut self) {
        for v in self.spans.iter_mut().chain(self.ligs.iter_mut()) {
            v.clear();
        }
        for v in [&mut self.boxes, &mut self.glues, &mut self.rules, &mut self.discs, &mut self.inserts] {
            v.clear();
        }
    }
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
        &self.list[list.0 as usize..list.1 as usize]
    }

    /// Stores a finished list in a block of its size class (a freed one when there is
    /// one). An empty list takes no block.
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

    pub fn add_box(&mut self, b: BoxRec) -> u32 {
        slot(&mut self.boxes, &mut self.free.boxes, b)
    }

    pub fn add_disc(&mut self, d: [(u32, u32); 2]) -> u32 {
        slot(&mut self.discs, &mut self.free.discs, d)
    }

    pub fn add_insert(&mut self, r: InsRec) -> u32 {
        slot(&mut self.inserts, &mut self.free.inserts, r)
    }

    pub fn add_rule(&mut self, dims: [i32; 3]) -> u32 {
        slot(&mut self.rules, &mut self.free.rules, dims)
    }

    pub fn glue_node(&mut self, g: Glue, sub: u8) -> Node {
        let i = slot(&mut self.glues, &mut self.free.glues, g);
        Node { kind: Kind::Glue, sub, font: 0, a: i as i32, b: 0 }
    }

    pub fn rule_node(&mut self, dims: [i32; 3]) -> Node {
        Node::new(Kind::Rule, self.add_rule(dims) as i32)
    }

    /// A ligature of `c` in font `f` made from the characters `from`.
    pub fn lig_node(&mut self, f: u16, c: u8, from: &[u8], hits: u8) -> Node {
        let at = self.lig_chars.len();
        self.lig_chars.extend_from_slice(from);
        self.lig_node_at(f, c, at, hits)
    }

    /// A ligature of `c` in font `f` made from the characters appended to `lig_chars` from
    /// `at`: they move into a freed block of their class when there is one, else stay as a
    /// new block at the end.
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
        Node { kind: Kind::Lig, sub: (len.min(63) as u8) << 2 | hits, font: f, a: c as i32, b: start as i32 }
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
