//! Breaking vertical lists into pages: tex.web §967–§979 (`prune_page_top`, `vert_break`,
//! `vsplit`), §980–§1028 (the page builder, `fire_up`, the output routine), §1097–§1101
//! (`\insert`, `\vadjust`, `\mark`) and §1054 (`\end`), in our own form.
//!
//! The contribution list is the outer vertical list in the build buffer; the current page is
//! a vector of its own. TeX's page-insertion records (§981) become a small vector sorted by
//! box number, and its node pointers become indices into the page. Marks are token spans,
//! never freed, so no reference counts are needed; e-TeX's mark classes are a short vector.

use super::engine_eqtb::{Glue, Reg, Region};
use super::engine_node::{BoxRec, InsRec, Kind, Node, IGNORE_DEPTH};
use super::engine_pack::badness;
use super::engine_prim::{Meaning, Prim};
use super::engine_run::{Machine, Mode};
use super::engine_whatsit::sized;

/// pdfTeX's `\ignoreprimitiveerror` (§236 slot).
const IGNORE_PRIMITIVE_ERROR: u32 = 102;
use super::engine_token::{Span, Token};

const INF_BAD: i32 = 10_000;
const INF_PENALTY: i32 = 10_000;
const EJECT_PENALTY: i32 = -10_000;
const AWFUL_BAD: i32 = 0o7777777777;
/// §974 `deplorable`.
const DEPLORABLE: i32 = 100_000;
const MAX_DIMEN: i32 = 0o7777777777;

/// §980 `page_contents`.
const EMPTY: u8 = 0;
const INSERTS_ONLY: u8 = 1;
const BOX_THERE: u8 = 2;

/// §269 group codes.
pub(super) const INSERT_GROUP: u8 = 11;
pub(super) const OUTPUT_GROUP: u8 = 8;

// Parameter slots (tex.web §236, §247, §224, §230).
const OUTPUT_PENALTY: u32 = 39;
const MAX_DEAD_CYCLES: u32 = 40;
const FLOATING_PENALTY: u32 = 42;
const HOLDING_INSERTS: u32 = 53;
const HSIZE: u32 = 3;
const VSIZE: u32 = 4;
const MAX_DEPTH: u32 = 5;
const SPLIT_MAX_DEPTH: u32 = 6;
const TOP_SKIP: u32 = 9;
const SPLIT_TOP_SKIP: u32 = 10;
const OUTPUT_ROUTINE: u32 = 0;

/// The five marks of a class (§382): top, first, bot, split first, split bot.
pub(super) type Marks = [Option<Span>; 5];

/// §981: a page-insertion record for box `n`.
#[derive(Clone, Copy, Debug)]
struct PageIns {
    n: u8,
    split_up: bool,
    height: i32,
    /// Page indices of the latest and the best latest insertion of this class.
    last_ins: Option<usize>,
    best_ins: Option<usize>,
    /// Where the insertion `broken_ins` breaks, as an index into its list (`None`: at
    /// its end).
    broken_ptr: Option<usize>,
    broken_ins: usize,
}

/// §980–§982: the current page and the page builder's state.
#[derive(Debug, Default)]
pub(super) struct Page {
    list: Vec<Node>,
    contents: u8,
    /// §982 `page_so_far`: goal, total, stretch by order (2–5), shrink, depth.
    pub so_far: [i32; 8],
    max_depth: i32,
    best_break: Option<usize>,
    least_cost: i32,
    best_size: i32,
    last_glue: Option<Glue>,
    last_penalty: i32,
    last_kern: i32,
    /// e-TeX's `last_node_type` plus one (0: nothing contributed since the page began).
    last_type: i32,
    pub insert_penalties: i32,
    pub output_active: bool,
    pub dead_cycles: i32,
    ins: Vec<PageIns>,
    /// The marks of each class that has had one, class 0 first.
    pub marks: Vec<(u32, Marks)>,
    /// Pages shipped out (§638), and their boxes, in order, when `keep_shipped` is set (a
    /// harness reading them after the run; otherwise they are freed).
    pub pages: usize,
    pub keep_shipped: bool,
    pub shipped: Vec<u32>,
    /// The shipped pages as PDF content, and for each font the font whose PDF object it
    /// shares (pdfTeX's `pdf_font_num`, `NOT_USED` until the font is first shipped).
    pub pdf: Vec<super::engine_ship::ShippedPage>,
    pub font_base: Vec<u16>,
    /// Per font: the characters shipped (pdfTeX's `pdf_char_marked`), and its PK scale
    /// (writet3.c `get_pk_font_scale`) when it is a bitmap font, 0 when scalable; with the
    /// map's TFM names (`backend_ship::map_names`), read at the first shipout.
    pub font_used: Vec<[u64; 4]>,
    pub font_pk: Vec<i64>,
    pub map_names: Option<Vec<u64>>,
}

impl Page {
    /// The current page's nodes and the shipped boxes kept, for `engine_gc`.
    #[cfg(all(test, feature = "oracle"))]
    pub(super) fn for_each_node_root(&mut self, f: &mut dyn FnMut(super::engine_gc::Root)) {
        for n in &mut self.list {
            f(super::engine_gc::Root::Node(n));
        }
        for b in &mut self.shipped {
            f(super::engine_gc::Root::Box(b));
        }
    }

    /// The marks and the current page's nodes' token spans, for `engine_gc`.
    pub(super) fn for_each_span(&mut self, f: &mut dyn FnMut(&mut Span)) {
        for (_, marks) in &mut self.marks {
            for m in marks.iter_mut().flatten() {
                f(m);
            }
        }
        for n in &mut self.list {
            super::engine_gc::node_span(n, f);
        }
    }

    pub(super) fn marks_mut(&mut self, class: u32) -> &mut Marks {
        let i = match self.marks.iter().position(|m| m.0 == class) {
            Some(i) => i,
            None => {
                self.marks.push((class, [None; 5]));
                self.marks.len() - 1
            }
        };
        &mut self.marks[i].1
    }

    pub(super) fn mark(&self, class: u32, which: usize) -> Option<Span> {
        self.marks.iter().find(|m| m.0 == class).and_then(|m| m.1[which])
    }

    /// §991 "Start a new current page".
    fn start_new(&mut self) {
        self.contents = EMPTY;
        self.list.clear();
        self.last_glue = None;
        self.last_penalty = 0;
        self.last_kern = 0;
        self.last_type = 0;
        self.so_far[7] = 0;
        self.max_depth = 0;
    }
}

impl Machine<'_, '_> {
    fn int_p(&self, slot: u32) -> i32 {
        self.e.eqtb.word(Reg { region: Region::Int, slot })
    }

    fn dimen_p(&self, slot: u32) -> i32 {
        self.e.eqtb.word(Reg { region: Region::Dimen, slot })
    }

    fn glue_p(&self, slot: u32) -> Glue {
        self.e.eqtb.glue(Reg { region: Region::Glue, slot })
    }

    /// The height, depth and width of a box or rule node.
    fn hdw(&self, n: &Node) -> (i32, i32, i32) {
        match n.kind {
            Kind::Box => {
                let b = &self.e.nodes.boxes[n.a as usize];
                (b.height, b.depth, b.width)
            }
            Kind::Rule => {
                let [w, h, d] = self.e.nodes.rules[n.a as usize];
                (h, d, w)
            }
            _ => (0, 0, 0),
        }
    }

    /// `vpack` of an arbitrary list, natural or to height `h` (`exact`), depth at most `d`.
    #[cfg_attr(feature = "ownership-check", track_caller)]
    pub(super) fn vpack_list(&mut self, list: &[Node], h: i32, exact: bool, d: i32) -> BoxRec {
        let start = self.cur.len();
        self.cur.extend_from_slice(list);
        self.vpack(start, h, exact, d)
    }

    /// §968 `prune_page_top`: glue, kerns and penalties at the top go; `\splittopskip`
    /// (here `top`) comes before the first box or rule.
    fn prune_page_top(&mut self, list: &[Node], top: Glue) -> Vec<Node> {
        let mut out = Vec::with_capacity(list.len() + 1);
        for (i, n) in list.iter().enumerate() {
            match n.kind {
                Kind::Box | Kind::Rule => {
                    let h = self.hdw(n).0;
                    let g = Glue { width: if top.width > h { top.width - h } else { 0 }, ..top };
                    out.push(self.e.nodes.glue_node(g, SPLIT_TOP_SKIP as u8 + 1));
                    out.extend_from_slice(&list[i..]);
                    return out;
                }
                // §968: the discarded nodes are flushed.
                Kind::Glue | Kind::Kern | Kind::Penalty => self.e.nodes.flush_node(*n),
                _ => out.push(*n),
            }
        }
        out
    }

    /// §970 `vert_break`: the best place to break `list` for height `h` with depth at most
    /// `d` (`None` for its end), and the height plus depth there (§971
    /// `best_height_plus_depth`).
    fn vert_break(&mut self, list: &[Node], h: i32, d: i32) -> (Option<usize>, i32) {
        let mut act = [0i32; 7];
        let mut prev_dp = 0;
        let mut least_cost = AWFUL_BAD;
        let (mut best_place, mut best_hd) = (None, 0);
        let mut prev_kind: Option<Kind> = None;
        let mut i = 0;
        loop {
            let p = list.get(i).copied();
            // §972, §973: the penalty at a legal breakpoint, or the node's measurements.
            let pi = match p {
                None => Some(EJECT_PENALTY),
                Some(n) => match n.kind {
                    Kind::Box | Kind::Rule => {
                        let (ht, dp, _) = self.hdw(&n);
                        act[1] += prev_dp + ht;
                        prev_dp = dp;
                        None
                    }
                    Kind::Whatsit if sized(n.sub) => {
                        let [_, ht, dp] = self.e.nodes.rules[n.a as usize];
                        act[1] += prev_dp + ht;
                        prev_dp = dp;
                        None
                    }
                    Kind::Glue if prev_kind.is_some_and(precedes_break) => Some(0),
                    Kind::Kern if list.get(i + 1).is_some_and(|m| m.kind == Kind::Glue) => Some(0),
                    Kind::Penalty => Some(n.a),
                    _ => None,
                },
            };
            if let Some(pi) = pi {
                // §974
                if pi < INF_PENALTY {
                    let b = if act[1] < h {
                        if act[3] != 0 || act[4] != 0 || act[5] != 0 {
                            0
                        } else {
                            badness(h - act[1], act[2])
                        }
                    } else if act[1] - h > act[6] {
                        AWFUL_BAD
                    } else {
                        badness(act[1] - h, act[6])
                    };
                    let b = if b < AWFUL_BAD {
                        if pi <= EJECT_PENALTY {
                            pi
                        } else if b < INF_BAD {
                            b + pi
                        } else {
                            DEPLORABLE
                        }
                    } else {
                        b
                    };
                    if b <= least_cost {
                        best_place = if p.is_none() { None } else { Some(i) };
                        least_cost = b;
                        best_hd = act[1] + prev_dp;
                    }
                    if b == AWFUL_BAD || pi <= EJECT_PENALTY {
                        return (best_place, best_hd);
                    }
                }
            }
            let Some(n) = p else { return (best_place, best_hd) };
            // §976: glue and kerns update the heights.
            match n.kind {
                Kind::Glue => {
                    let mut g = self.e.nodes.glue(&n);
                    act[2 + g.stretch_order as usize] += g.stretch;
                    act[6] += g.shrink;
                    if g.shrink_order != 0 && g.shrink != 0 {
                        // pdfTeX's `print_ignored_err`: bit 1 of `\ignoreprimitiveerror`
                        // (latex.ltx sets it) sends this one to the log only.
                        if self.e.eqtb.word(Reg { region: Region::Int, slot: IGNORE_PRIMITIVE_ERROR }) & 1 == 0 {
                            self.error("Infinite glue shrinkage found in box being split");
                        }
                        g.shrink_order = 0;
                    }
                    act[1] += prev_dp + g.width;
                    prev_dp = 0;
                }
                Kind::Kern => {
                    act[1] += prev_dp + n.a;
                    prev_dp = 0;
                }
                _ => {}
            }
            // §972 `not_found`
            if prev_dp > d {
                act[1] += prev_dp - d;
                prev_dp = d;
            }
            prev_kind = Some(n.kind);
            i += 1;
        }
    }

    /// §977 `vsplit`: the top of box `n`, `h` high, leaving the rest in the box.
    pub(super) fn vsplit(&mut self, n: u32, h: i32) -> Option<u32> {
        for (_, m) in self.page.marks.iter_mut() {
            m[3] = None;
            m[4] = None;
        }
        let v = self.e.eqtb.box_reg(n)?;
        let b = self.e.nodes.boxes[v as usize];
        if !b.vertical {
            self.error("\\vsplit needs a \\vbox");
            return None;
        }
        let list = self.e.nodes.get(b.list).to_vec();
        let (q, _) = self.vert_break(&list, h, self.dimen_p(SPLIT_MAX_DEPTH));
        let cut = q.unwrap_or(list.len());
        // §979: the marks before the break.
        for m in &list[..cut] {
            if m.kind == Kind::Mark {
                let span = Span { start: m.a as u32, end: m.b as u32 };
                let marks = self.page.marks_mut(m.font as u32);
                if marks[3].is_none() {
                    marks[3] = Some(span);
                }
                marks[4] = Some(span);
            }
        }
        let rest = self.prune_page_top(&list[cut..], self.glue_p(SPLIT_TOP_SKIP));
        let left = if rest.is_empty() {
            None
        } else {
            let r = self.vpack_list(&rest, 0, false, MAX_DIMEN);
            Some(self.e.nodes.add_box(r))
        };
        self.e.eqtb.replace_box(n, left);
        // The old box's nodes moved to the two new ones (or were pruned).
        self.e.nodes.free_block(b.list);
        self.e.nodes.free_box(v);
        let top = self.vpack_list(&list[..cut], h, true, self.dimen_p(SPLIT_MAX_DEPTH));
        Some(self.e.nodes.add_box(top))
    }

    /// §987 `freeze_page_specs`.
    fn freeze_page_specs(&mut self, s: u8) {
        let p = &mut self.page;
        p.contents = s;
        p.so_far = [0; 8];
        p.so_far[0] = self.e.eqtb.word(Reg { region: Region::Dimen, slot: VSIZE });
        p.max_depth = self.e.eqtb.word(Reg { region: Region::Dimen, slot: MAX_DEPTH });
        p.least_cost = AWFUL_BAD;
    }

    /// The contribution list (§995 `contrib_head`): the main vertical list, the bottom
    /// segment of the build buffer, taken out; the lists above it move down.
    fn take_contrib(&mut self) -> Vec<Node> {
        let len = match self.nest.get(1) {
            Some(l) => l.start,
            None if self.nest.is_empty() => self.cur.len() as u32,
            None => self.start,
        };
        if !self.nest.is_empty() {
            self.start -= len;
            for l in &mut self.nest[1..] {
                l.start -= len;
            }
        }
        self.cur.drain(..len as usize).collect()
    }

    /// Puts `list` back as the contribution list, under the lists being built.
    fn put_contrib(&mut self, list: Vec<Node>) {
        let len = list.len() as u32;
        self.cur.splice(0..0, list);
        if !self.nest.is_empty() {
            self.start += len;
            for l in &mut self.nest[1..] {
                l.start += len;
            }
        }
    }

    /// §994 `build_page`: the contribution list (the main vertical list) moves to the
    /// current page until it is empty or a page is output.
    pub(super) fn build_page(&mut self) {
        if self.page.output_active {
            return;
        }
        let mut contrib = self.take_contrib();
        if contrib.is_empty() {
            return;
        }
        let mut i = 0;
        while i < contrib.len() {
            let p = contrib[i];
            // §996
            self.page.last_glue = None;
            self.page.last_penalty = 0;
            self.page.last_kern = 0;
            self.page.last_type = self.node_type(&p) + 1;
            match p.kind {
                Kind::Glue => self.page.last_glue = Some(self.e.nodes.glue(&p)),
                Kind::Penalty => self.page.last_penalty = p.a,
                Kind::Kern => self.page.last_kern = p.a,
                _ => {}
            }
            let tail_breaks = self.page.list.last().is_some_and(|t| precedes_break(t.kind));
            // §1000: the node's effect on the page; `Some(pi)` at a legal breakpoint.
            enum Next {
                Pi(i32),
                Heights,
                Contribute,
                Discard,
            }
            let next = match p.kind {
                Kind::Box | Kind::Rule if self.page.contents < BOX_THERE => {
                    // §1001: `\topskip` before the page's first box.
                    if self.page.contents == EMPTY {
                        self.freeze_page_specs(BOX_THERE);
                    } else {
                        self.page.contents = BOX_THERE;
                    }
                    let top = self.glue_p(TOP_SKIP);
                    let h = self.hdw(&p).0;
                    let g = Glue { width: if top.width > h { top.width - h } else { 0 }, ..top };
                    let q = self.e.nodes.glue_node(g, TOP_SKIP as u8 + 1);
                    contrib.insert(i, q);
                    continue;
                }
                Kind::Box | Kind::Rule => {
                    // §1002
                    let (h, d, _) = self.hdw(&p);
                    self.page.so_far[1] += self.page.so_far[7] + h;
                    self.page.so_far[7] = d;
                    Next::Contribute
                }
                Kind::Glue | Kind::Kern | Kind::Penalty if self.page.contents < BOX_THERE => Next::Discard,
                Kind::Glue if tail_breaks => Next::Pi(0),
                Kind::Glue => Next::Heights,
                Kind::Kern => match contrib.get(i + 1) {
                    None => {
                        // A kern waits for what follows it.
                        contrib.drain(..i);
                        self.put_contrib(contrib);
                        return;
                    }
                    Some(m) if m.kind == Kind::Glue => Next::Pi(0),
                    Some(_) => Next::Heights,
                },
                Kind::Penalty => Next::Pi(p.a),
                Kind::Ins => {
                    self.page_insert(&p);
                    Next::Contribute
                }
                // pdfTeX: an image whatsit adds to `page_total` like a box, without `\topskip`.
                Kind::Whatsit if sized(p.sub) => {
                    let [_, h, d] = self.e.nodes.rules[p.a as usize];
                    self.page.so_far[1] += self.page.so_far[7] + h;
                    self.page.so_far[7] = d;
                    Next::Contribute
                }
                _ => Next::Contribute,
            };
            let heights = match next {
                Next::Discard => {
                    // §1000 `done1`: the node is flushed.
                    self.e.nodes.flush_node(p);
                    i += 1;
                    continue;
                }
                Next::Contribute => false,
                Next::Heights => true,
                Next::Pi(pi) => {
                    // §1005
                    if pi < INF_PENALTY {
                        let s = &self.page.so_far;
                        let b = if s[1] < s[0] {
                            if s[3] != 0 || s[4] != 0 || s[5] != 0 {
                                0
                            } else {
                                badness(s[0] - s[1], s[2])
                            }
                        } else if s[1] - s[0] > s[6] {
                            AWFUL_BAD
                        } else {
                            badness(s[1] - s[0], s[6])
                        };
                        let mut c = if b < AWFUL_BAD {
                            if pi <= EJECT_PENALTY {
                                pi
                            } else if b < INF_BAD {
                                b + pi + self.page.insert_penalties
                            } else {
                                DEPLORABLE
                            }
                        } else {
                            b
                        };
                        if self.page.insert_penalties >= 10_000 {
                            c = AWFUL_BAD;
                        }
                        if c <= self.page.least_cost {
                            self.page.best_break = Some(self.page.list.len());
                            self.page.best_size = self.page.so_far[0];
                            self.page.least_cost = c;
                            for r in &mut self.page.ins {
                                r.best_ins = r.last_ins;
                            }
                        }
                        if c == AWFUL_BAD || pi <= EJECT_PENALTY {
                            let rest = contrib.split_off(i);
                            contrib = self.fire_up(rest);
                            if self.page.output_active {
                                return;
                            }
                            // The page was shipped out; carry on with what came back.
                            i = 0;
                            continue;
                        }
                    }
                    matches!(p.kind, Kind::Glue | Kind::Kern)
                }
            };
            if heights {
                // §1004
                let w = if p.kind == Kind::Kern {
                    p.a
                } else {
                    let g = self.e.nodes.glue(&p);
                    self.page.so_far[2 + g.stretch_order as usize] += g.stretch;
                    self.page.so_far[6] += g.shrink;
                    if g.shrink_order != 0 && g.shrink != 0 {
                        self.error("Infinite glue shrinkage found on current page");
                    }
                    g.width
                };
                self.page.so_far[1] += self.page.so_far[7] + w;
                self.page.so_far[7] = 0;
            }
            // §1003
            if self.page.so_far[7] > self.page.max_depth {
                self.page.so_far[1] += self.page.so_far[7] - self.page.max_depth;
                self.page.so_far[7] = self.page.max_depth;
            }
            self.page.list.push(p);
            i += 1;
        }
    }

    /// §1008: an insertion reaches the current page.
    fn page_insert(&mut self, p: &Node) {
        if self.page.contents == EMPTY {
            self.freeze_page_specs(INSERTS_ONLY);
        }
        let ins = self.e.nodes.inserts[p.a as usize];
        let n = ins.n;
        let r = match self.page.ins.binary_search_by_key(&n, |r| r.n) {
            Ok(r) => r,
            Err(at) => {
                // §1009: a new class: its box so far, and its `\skip`.
                self.ensure_vbox(n as u32);
                let height = self.e.eqtb.box_reg(n as u32).map_or(0, |b| {
                    let b = &self.e.nodes.boxes[b as usize];
                    b.height + b.depth
                });
                self.page.ins.insert(at, PageIns { n, split_up: false, height, last_ins: None, best_ins: None, broken_ptr: None, broken_ins: 0 });
                let h = self.scaled_by_count(height, n);
                let q = self.e.eqtb.glue(Reg::register(Region::Glue, n as u32));
                let s = &mut self.page.so_far;
                s[0] -= h + q.width;
                s[2 + q.stretch_order as usize] += q.stretch;
                s[6] += q.shrink;
                if q.shrink_order != 0 && q.shrink != 0 {
                    self.error(format!("Infinite glue shrinkage inserted from \\skip{n}"));
                }
                at
            }
        };
        if self.page.ins[r].split_up {
            self.page.insert_penalties += ins.float_cost;
            return;
        }
        let at = self.page.list.len();
        self.page.ins[r].last_ins = Some(at);
        let s = &self.page.so_far;
        let delta = s[0] - s[1] - s[7] + s[6];
        let h = self.scaled_by_count(ins.height, n);
        let limit = self.e.eqtb.word(Reg::register(Region::Dimen, n as u32));
        if (h <= 0 || h <= delta) && ins.height + self.page.ins[r].height <= limit {
            self.page.so_far[0] -= h;
            self.page.ins[r].height += ins.height;
        } else {
            // §1010: split the insertion.
            let count = self.e.eqtb.word(Reg::register(Region::Int, n as u32));
            let mut w = if count <= 0 {
                MAX_DIMEN
            } else {
                let s = &self.page.so_far;
                let w = s[0] - s[1] - s[7];
                if count != 1000 {
                    x_over_n(w, count).wrapping_mul(1000)
                } else {
                    w
                }
            };
            w = w.min(limit - self.page.ins[r].height);
            let list = self.e.nodes.get(ins.list).to_vec();
            let (q, mut best_hd) = self.vert_break(&list, w, ins.depth);
            self.page.ins[r].height += best_hd;
            if count != 1000 {
                best_hd = x_over_n(best_hd, 1000).wrapping_mul(count);
            }
            self.page.so_far[0] -= best_hd;
            let rec = &mut self.page.ins[r];
            rec.split_up = true;
            rec.broken_ptr = q;
            rec.broken_ins = at;
            match q {
                None => self.page.insert_penalties += EJECT_PENALTY,
                Some(k) if list[k].kind == Kind::Penalty => self.page.insert_penalties += list[k].a,
                Some(_) => {}
            }
        }
    }

    /// `h` scaled by `\count n` / 1000 (§1009, §1008).
    fn scaled_by_count(&self, h: i32, n: u8) -> i32 {
        let count = self.e.eqtb.word(Reg::register(Region::Int, n as u32));
        if count == 1000 {
            h
        } else {
            x_over_n(h, 1000).wrapping_mul(count)
        }
    }

    /// §993 `ensure_vbox`: an insertion box must be a `\vbox`.
    fn ensure_vbox(&mut self, n: u32) {
        if let Some(b) = self.e.eqtb.box_reg(n) {
            if !self.e.nodes.boxes[b as usize].vertical {
                self.error("Insertions can only be added to a vbox");
                // §993 `box_error`: the box is flushed.
                self.e.eqtb.replace_box(n, None);
                self.e.nodes.flush_box(b);
            }
        }
    }

    /// §1012 `fire_up`: the page is broken at its best place into `\box255`, insertions go
    /// to their boxes, and the output routine runs (or the page is shipped out). `rest` is
    /// the contribution list from the node that triggered it; the new contribution list is
    /// returned, or (when the output routine starts) put back under it.
    fn fire_up(&mut self, mut rest: Vec<Node>) -> Vec<Node> {
        // §1013 `\outputpenalty`.
        let reg = Reg { region: Region::Int, slot: OUTPUT_PENALTY };
        let best = self.page.best_break.unwrap_or(usize::MAX);
        let at_best: Option<&mut Node> = if best < self.page.list.len() { self.page.list.get_mut(best) } else { rest.first_mut() };
        match at_best {
            Some(n) if n.kind == Kind::Penalty => {
                let v = n.a;
                n.a = INF_PENALTY;
                self.e.eqtb.set_word(reg, v, true);
            }
            _ => self.e.eqtb.set_word(reg, INF_PENALTY, true),
        }
        for (_, m) in self.page.marks.iter_mut() {
            if m[2].is_some() {
                m[0] = m[2];
                m[1] = None;
            }
        }
        // §1014: the page ends before `best` (the whole page when it was the new node).
        let cut = best.min(self.page.list.len());
        if let Some(b) = self.e.eqtb.box_reg(255) {
            self.error("\\box255 is not void");
            self.e.eqtb.replace_box(255, None);
            self.e.nodes.flush_box(b);
        }
        self.page.insert_penalties = 0;
        let holding = self.int_p(HOLDING_INSERTS) > 0;
        // §1018: each class with a best insertion gets a queue: its box's list so far.
        let mut queues: Vec<Option<Vec<Node>>> = vec![None; self.page.ins.len()];
        if !holding {
            for (k, r) in self.page.ins.clone().iter().enumerate() {
                if r.best_ins.is_some() {
                    self.ensure_vbox(r.n as u32);
                    let list = self.e.eqtb.box_reg(r.n as u32).map(|b| self.e.nodes.get(self.e.nodes.boxes[b as usize].list).to_vec());
                    queues[k] = Some(list.unwrap_or_default());
                }
            }
        }
        let page = std::mem::take(&mut self.page.list);
        let mut held: Vec<Node> = Vec::new();
        let mut kept: Vec<Node> = Vec::with_capacity(page.len());
        for (idx, p) in page[..cut].iter().enumerate() {
            match p.kind {
                Kind::Ins if !holding => {
                    // §1020
                    let ins = self.e.nodes.inserts[p.a as usize];
                    let r = self.page.ins.iter().position(|r| r.n == ins.n).expect("a page insertion record");
                    let mut wait = true;
                    let mut node = *p;
                    if self.page.ins[r].best_ins.is_some() {
                        wait = false;
                        let list = self.e.nodes.get(ins.list).to_vec();
                        // The insertion's nodes move into the queue (or the remainder).
                        self.e.nodes.free_block(ins.list);
                        self.e.nodes.free_insert(p.a as u32);
                        let rec = self.page.ins[r];
                        if rec.best_ins == Some(idx) {
                            // §1021: the last insertion of the class on this page, perhaps split.
                            let mut take = list.len();
                            if rec.split_up && rec.broken_ins == idx {
                                if let Some(bp) = rec.broken_ptr {
                                    take = bp;
                                    let remainder = self.prune_page_top(&list[bp..], ins.split_top);
                                    if !remainder.is_empty() {
                                        let b = self.vpack_list(&remainder, 0, false, MAX_DIMEN);
                                        // Only the height is needed; the remainder's nodes
                                        // are owned by the new insertion's list.
                                        self.e.nodes.free_block(b.list);
                                        let stored = self.e.nodes.store(&remainder);
                                        let rec2 = InsRec { height: b.height + b.depth, list: stored, ..ins };
                                        node.a = self.e.nodes.add_insert(rec2) as i32;
                                        wait = true;
                                    }
                                }
                            }
                            let q = queues[r].as_mut().expect("a queue");
                            q.extend_from_slice(&list[..take]);
                            self.page.ins[r].best_ins = None;
                            let q = queues[r].take().unwrap_or_default();
                            let b = self.vpack_list(&q, 0, false, MAX_DIMEN);
                            let b = self.e.nodes.add_box(b);
                            // §1021: the class's box, whose list began the queue, is replaced.
                            if let Some(old) = self.e.eqtb.box_reg(ins.n as u32) {
                                self.e.nodes.free_block(self.e.nodes.boxes[old as usize].list);
                                self.e.nodes.free_box(old);
                            }
                            self.e.eqtb.replace_box(ins.n as u32, Some(b));
                        } else if let Some(q) = queues[r].as_mut() {
                            q.extend_from_slice(&list);
                        }
                    }
                    if wait {
                        held.push(node);
                        self.page.insert_penalties += 1;
                    }
                }
                Kind::Mark => {
                    // §1016
                    let span = Span { start: p.a as u32, end: p.b as u32 };
                    let m = self.page.marks_mut(p.font as u32);
                    if m[1].is_none() {
                        m[1] = Some(span);
                    }
                    m[2] = Some(span);
                    kept.push(*p);
                }
                _ => kept.push(*p),
            }
        }
        // §1017: the rest of the page goes back on the contribution list.
        let mut back: Vec<Node> = page[cut..].to_vec();
        back.append(&mut rest);
        let b = self.vpack_list(&kept, self.page.best_size, true, self.page.max_depth);
        let b = self.e.nodes.add_box(b);
        self.e.eqtb.replace_box(255, Some(b));
        self.page.start_new();
        self.page.list = held;
        self.page.ins.clear();
        self.page.best_break = None;
        for (_, m) in self.page.marks.iter_mut() {
            if m[0].is_some() && m[1].is_none() {
                m[1] = m[0];
            }
        }
        let output = self.e.eqtb.toks(Reg { region: Region::Toks, slot: OUTPUT_ROUTINE });
        if !output.is_empty() {
            if self.page.dead_cycles >= self.int_p(MAX_DEAD_CYCLES) {
                self.error(format!("Output loop---{} consecutive dead cycles", self.page.dead_cycles));
            } else {
                // §1025: the output routine runs in its own internal vertical list.
                self.put_contrib(back);
                self.page.output_active = true;
                self.page.dead_cycles += 1;
                self.push_nest(Mode::Vertical, true, IGNORE_DEPTH);
                let list = self.e.toks.get(output).to_vec();
                self.back_list(&list);
                self.begin_group(OUTPUT_GROUP);
                self.normal_paragraph();
                self.scan_left_brace();
                return Vec::new();
            }
        }
        // §1023: the default output routine.
        let mut contrib = std::mem::take(&mut self.page.list);
        contrib.append(&mut back);
        if let Some(b) = self.e.eqtb.box_reg(255) {
            self.ship_out(b);
        }
        self.e.eqtb.replace_box(255, None);
        contrib
    }

    /// §1100: the `}` of the output routine.
    pub(super) fn resume_page_builder(&mut self) {
        self.end_graf();
        self.end_group(OUTPUT_GROUP);
        self.page.output_active = false;
        self.page.insert_penalties = 0;
        if self.e.eqtb.box_reg(255).is_some() {
            self.error("Output routine didn't use all of \\box255");
            self.e.eqtb.replace_box(255, None);
        }
        // Held-over insertions, then what the routine left, then the old contributions.
        let start = self.start as usize;
        let out: Vec<Node> = self.cur.drain(start..).collect();
        self.pop_nest();
        let old = self.take_contrib();
        let mut contrib = std::mem::take(&mut self.page.list);
        contrib.extend_from_slice(&out);
        contrib.extend_from_slice(&old);
        self.put_contrib(contrib);
        self.build_page();
    }

    /// §638 `ship_out`: the page is done; its `\write`s, `\openout`s and `\closeout`s
    /// act now.
    pub(super) fn ship_out(&mut self, b: u32) {
        self.out_whats(b, true, &mut [].iter());
        let positions = self.ship_page(b);
        self.out_whats(b, false, &mut positions.iter());
        self.page.dead_cycles = 0;
        self.page.pages += 1;
        if self.page.keep_shipped {
            self.page.shipped.push(b);
        } else {
            // §640: the shipped box is flushed.
            self.e.nodes.flush_box(b);
        }
    }

    /// §1097 `begin_insert_or_adjust`.
    pub(super) fn begin_insert_or_adjust(&mut self, insert: bool) {
        let n = if insert {
            let n = self.scan_int();
            match n {
                0..=254 => n as u8,
                255 => {
                    self.error("You can't \\insert255");
                    0
                }
                _ => {
                    self.error(format!("Bad register code ({n})"));
                    0
                }
            }
        } else {
            255
        };
        self.insert_numbers.push(n);
        self.begin_group(INSERT_GROUP);
        self.scan_left_brace();
        self.normal_paragraph();
        self.push_nest(Mode::Vertical, true, IGNORE_DEPTH);
    }

    /// §1100: the `}` of `\insert` or `\vadjust`.
    pub(super) fn end_insert(&mut self) {
        self.end_graf();
        let split_top = self.glue_p(SPLIT_TOP_SKIP);
        let depth = self.dimen_p(SPLIT_MAX_DEPTH);
        let float_cost = self.int_p(FLOATING_PENALTY);
        self.end_group(INSERT_GROUP);
        let n = self.insert_numbers.pop().unwrap_or(255);
        let start = self.start as usize;
        let b = self.vpack(start, 0, false, MAX_DIMEN);
        self.pop_nest();
        let node = if n < 255 {
            Node::new(Kind::Ins, self.e.nodes.add_insert(InsRec { n, height: b.height + b.depth, depth, split_top, float_cost, list: b.list }) as i32)
        } else {
            Node { kind: Kind::Adjust, sub: 0, font: 0, a: b.list.0 as i32, b: b.list.1 as i32 }
        };
        self.cur.push(node);
        if self.nest.is_empty() {
            self.build_page();
        }
    }

    /// §1101 `make_mark`: `\mark{...}`, or e-TeX's `\marks n{...}`.
    pub(super) fn make_mark(&mut self, class: bool) {
        let c = if class {
            let c = self.scan_int();
            if !(0..=32767).contains(&c) {
                self.error(format!("Bad register code ({c})"));
                0
            } else {
                c as u16
            }
        } else {
            0
        };
        let span = self.scan_braced_text(true);
        self.cur.push(Node { kind: Kind::Mark, sub: 0, font: c, a: span.start as i32, b: span.end as i32 });
    }

    /// §386: `\topmark` and its kin expand to the mark's tokens.
    pub(super) fn insert_mark(&mut self, p: Prim) {
        let which = match p {
            Prim::TopMark | Prim::TopMarks => 0,
            Prim::FirstMark | Prim::FirstMarks => 1,
            Prim::BotMark | Prim::BotMarks => 2,
            Prim::SplitFirstMark | Prim::SplitFirstMarks => 3,
            _ => 4,
        };
        let class = if matches!(p, Prim::TopMarks | Prim::FirstMarks | Prim::BotMarks | Prim::SplitFirstMarks | Prim::SplitBotMarks) { self.scan_int().clamp(0, 32767) as u32 } else { 0 };
        if let Some(span) = self.page.mark(class, which) {
            let list = self.e.toks.get(span).to_vec();
            self.back_list(&list);
        }
    }

    /// §1054 `its_all_over`: `\end` stops when everything is out; otherwise it fills the
    /// page and tries again.
    pub(super) fn its_all_over(&mut self, t: Token) -> bool {
        if self.page.list.is_empty() && self.cur.is_empty() && self.page.dead_cycles == 0 {
            return true;
        }
        self.input.back(t);
        let b = self.e.nodes.add_box(BoxRec { width: self.dimen_p(HSIZE), ..BoxRec::default() });
        self.cur.push(Node::new(Kind::Box, b as i32));
        let fill = self.e.nodes.glue_node(Glue { stretch: 1 << 16, stretch_order: 2, ..Glue::default() }, 0);
        self.cur.push(fill);
        self.cur.push(Node::new(Kind::Penalty, -0o10000000000));
        self.build_page();
        false
    }

    /// §421 `\pagegoal` and its kin.
    pub(super) fn page_dimen(&self, k: usize) -> i32 {
        if self.page.contents == EMPTY && !self.page.output_active {
            if k == 0 {
                MAX_DIMEN
            } else {
                0
            }
        } else {
            self.page.so_far[k]
        }
    }

    /// §424: `\lastpenalty`, `\lastkern`, `\lastskip` of the main vertical list come from the
    /// page builder once its contributions are gone.
    pub(super) fn page_last(&self, kind: Kind) -> Option<Node> {
        if !(self.nest.is_empty() && self.mode == Mode::Vertical && self.cur.is_empty()) || self.mode_zero {
            return None;
        }
        Some(match kind {
            Kind::Penalty => Node::new(Kind::Penalty, self.page.last_penalty),
            Kind::Kern => Node::new(Kind::Kern, self.page.last_kern),
            _ => return None,
        })
    }

    /// e-TeX's `last_node_type` of the main vertical list, from the page builder.
    pub(super) fn page_last_type(&self) -> Option<i32> {
        (self.nest.is_empty() && self.mode == Mode::Vertical && self.cur.is_empty() && !self.mode_zero).then_some(self.page.last_type - 1)
    }

    pub(super) fn page_last_glue(&self) -> Option<Glue> {
        (self.nest.is_empty() && self.mode == Mode::Vertical && self.cur.is_empty() && !self.mode_zero).then_some(self.page.last_glue).flatten()
    }

    /// Meaning check for the commands of this file, as `command` dispatches them.
    pub(super) fn page_command(&mut self, x_t: Token, m: Meaning) -> bool {
        let Meaning::Prim(p) = m else { return false };
        match p {
            Prim::Insert => self.begin_insert_or_adjust(true),
            Prim::Vadjust if self.mode != Mode::Vertical => self.begin_insert_or_adjust(false),
            Prim::Mark => self.make_mark(false),
            Prim::Marks => self.make_mark(true),
            Prim::End | Prim::Dump if self.nest.is_empty() => {
                if self.its_all_over(x_t) {
                    self.halted = true;
                }
            }
            _ => return false,
        }
        true
    }
}

/// §148 `precedes_break`: a node before which glue is a legal breakpoint.
fn precedes_break(k: Kind) -> bool {
    !matches!(k, Kind::Glue | Kind::Kern | Kind::Penalty)
}

/// §106 `x_over_n`: `x / n`, truncated toward zero.
fn x_over_n(x: i32, n: i32) -> i32 {
    if n == 0 {
        0
    } else {
        x / n
    }
}

#[cfg(test)]
mod tests {
    use super::super::engine_names::param;
    use super::*;

    #[test]
    fn slots_match_the_table() {
        for (name, region, slot) in [
            ("outputpenalty", Region::Int, OUTPUT_PENALTY),
            ("maxdeadcycles", Region::Int, MAX_DEAD_CYCLES),
            ("floatingpenalty", Region::Int, FLOATING_PENALTY),
            ("holdinginserts", Region::Int, HOLDING_INSERTS),
            ("hsize", Region::Dimen, HSIZE),
            ("vsize", Region::Dimen, VSIZE),
            ("maxdepth", Region::Dimen, MAX_DEPTH),
            ("splitmaxdepth", Region::Dimen, SPLIT_MAX_DEPTH),
            ("topskip", Region::Glue, TOP_SKIP),
            ("splittopskip", Region::Glue, SPLIT_TOP_SKIP),
            ("output", Region::Toks, OUTPUT_ROUTINE),
        ] {
            assert_eq!(param(name), Some(Reg { region, slot }), "{name}");
        }
    }
}
