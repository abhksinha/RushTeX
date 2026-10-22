//! Reclaiming the token arena during a run. TeX frees a macro's old text when it is
//! redefined (§275 `eq_destroy`, `delete_token_ref`); our arena only grows, so a run that
//! redefines macros in a loop (expl3's token-list functions, `\g@addto@macro`) would keep
//! every old text. Instead, when the arena reaches twice what was live after the last
//! collection, the live lists slide down in place over the dead ones. No reference counts
//! on the hot path (one comparison per command), no second arena and no map: the arena's
//! capacity is reused, so the run's memory stays at about twice its live tokens and the
//! allocator sees no churn.
//!
//! A collection runs only between the commands of the outer main loop (`Machine::run`),
//! where no Rust frame holds a span. The roots are the equivalents and the save stack, the
//! token lists being read (from their current position), the alignment templates, the
//! marks, and the mark and whatsit nodes of every node list. Overlapping or nested spans
//! (a macro body inside its text, a list being read from its middle) fall in one merged
//! live range and keep their offsets in it, so sharing survives without bookkeeping.

use super::engine_node::{leader, Kind, Node, Nodes};
use super::engine_run::Machine;
use super::engine_token::Span;

/// No collection below this arena length: the format alone holds about 460,000 tokens.
pub(super) const MIN_COLLECT: u32 = 1 << 20;

/// Applies `f` to the token span of a mark node or of a whatsit that holds a token list.
#[inline]
pub(super) fn node_span(n: &mut Node, f: &mut dyn FnMut(&mut Span)) {
    let holds = match n.kind {
        Kind::Mark => true,
        Kind::Whatsit => super::engine_whatsit::holds_tokens(n.sub),
        _ => false,
    };
    if holds {
        let mut s = Span { start: n.a as u32, end: n.b as u32 };
        f(&mut s);
        n.a = s.start as i32;
        n.b = s.end as i32;
    }
}

impl Machine<'_, '_> {
    /// Collects the token arena, and the node arena, when either has outgrown its last live
    /// size twice over.
    #[inline]
    pub(super) fn maybe_collect(&mut self) {
        if self.e.toks.len() as u32 >= self.collect_at {
            self.collect_tokens();
        }
        if self.e.nodes.list.len() as u32 >= self.node_collect_at {
            self.collect_nodes();
        }
    }

    /// Every holder of node-arena references outside the arena: the box registers and the
    /// save stack, the lists being built, the current page, the shipped pages a harness
    /// keeps, alignments' adjustments, open discretionaries, the noads of an open formula,
    /// `\pdfxform`s, and §814 `just_box`.
    fn for_each_node_root(&mut self, f: &mut dyn FnMut(Root)) {
        self.e.eqtb.for_each_box(|b| f(Root::Box(b)));
        for x in self.cur.iter_mut().chain(self.adjust.iter_mut().flatten()).chain(self.adjust_out.iter_mut().flatten()) {
            f(Root::Node(x));
        }
        for a in &mut self.aligns {
            a.for_each_node(&mut |x| f(Root::Node(x)));
        }
        self.page.for_each_node_root(f);
        for (_, pre, post) in &mut self.disc_saved {
            f(Root::List(pre));
            f(Root::List(post));
        }
        for q in &mut self.noads {
            for field in [&mut q.nucleus, &mut q.supscr, &mut q.subscr, &mut q.extra] {
                match field {
                    super::engine_mlist::Field::Box(b) => f(Root::Box(b)),
                    super::engine_mlist::Field::Mlist(a, b) => {
                        let mut l = (*a, *b);
                        f(Root::List(&mut l));
                        (*a, *b) = l;
                    }
                    _ => {}
                }
            }
        }
        for x in &mut self.pdf_doc.forms {
            f(Root::Box(&mut x.box_rec));
        }
        f(Root::Box(&mut self.just_box));
    }

    /// Mark and compact the node arena and its side tables, in place: one live bit per
    /// entry, marked from the roots; then each table keeps its live entries in order and
    /// every index becomes its rank among them. Shared lists and boxes stay shared.
    fn collect_nodes(&mut self) {
        let nodes = std::mem::take(&mut self.e.nodes);
        let mut gc = std::mem::take(&mut self.node_gc);
        gc.list.reset(nodes.list.len());
        gc.boxes.reset(nodes.boxes.len());
        gc.glues.reset(nodes.glues.len());
        gc.rules.reset(nodes.rules.len());
        gc.ligs.reset(nodes.lig_chars.len());
        gc.discs.reset(nodes.discs.len());
        gc.inserts.reset(nodes.inserts.len());
        self.for_each_node_root(&mut |r| gc.mark(&nodes, r));
        for l in [&mut gc.list, &mut gc.boxes, &mut gc.glues, &mut gc.rules, &mut gc.ligs, &mut gc.discs, &mut gc.inserts] {
            l.seal();
        }
        let mut nodes = nodes;
        gc.list.keep(&mut nodes.list);
        gc.boxes.keep(&mut nodes.boxes);
        gc.glues.keep(&mut nodes.glues);
        gc.rules.keep(&mut nodes.rules);
        gc.ligs.keep(&mut nodes.lig_chars);
        gc.discs.keep(&mut nodes.discs);
        gc.inserts.keep(&mut nodes.inserts);
        for x in &mut nodes.list {
            gc.remap_node(x);
        }
        for b in &mut nodes.boxes {
            b.list = gc.list.span(b.list);
        }
        for d in &mut nodes.discs {
            *d = [gc.list.span(d[0]), gc.list.span(d[1])];
        }
        for i in &mut nodes.inserts {
            i.list = gc.list.span(i.list);
        }
        nodes.free.clear();
        self.for_each_node_root(&mut |r| gc.remap(r));
        self.node_collect_at = (nodes.list.len() as u32).saturating_mul(2).max(MIN_NODE_COLLECT);
        self.e.nodes = nodes;
        self.node_gc = gc;
    }

    /// Every token span the run can still reach, each given to `f` (which may move it).
    fn for_each_root(&mut self, f: &mut dyn FnMut(&mut Span)) {
        self.e.eqtb.for_each_span(&mut *f);
        self.input.for_each_list(&mut *f);
        for a in &mut self.aligns {
            a.for_each_span(f);
        }
        self.page.for_each_span(f);
        for n in self.e.nodes.list.iter_mut().chain(self.cur.iter_mut()) {
            node_span(n, f);
        }
        for n in self.adjust.iter_mut().chain(self.adjust_out.iter_mut()).flatten() {
            node_span(n, f);
        }
    }

    fn collect_tokens(&mut self) {
        // The live ranges, sorted and merged, each with its new start.
        let mut live = std::mem::take(&mut self.collect_buf);
        live.clear();
        self.for_each_root(&mut |s| {
            if !s.is_empty() {
                live.push((s.start, s.end, 0));
            }
        });
        live.sort_unstable_by_key(|r| r.0);
        let mut merged = 0;
        for i in 0..live.len() {
            let (s, e, _) = live[i];
            if merged > 0 && s <= live[merged - 1].1 {
                live[merged - 1].1 = live[merged - 1].1.max(e);
            } else {
                live[merged] = (s, e, 0);
                merged += 1;
            }
        }
        live.truncate(merged);
        let mut to = 0;
        for r in live.iter_mut() {
            r.2 = to;
            to += r.1 - r.0;
        }
        // Each span moves with the range holding its start (both passes visit the same
        // roots, so every start lies in a range).
        self.for_each_root(&mut |s| {
            if s.is_empty() {
                *s = Span::default();
                return;
            }
            let (rs, _, rto) = live[live.partition_point(|r| r.0 <= s.start) - 1];
            let delta = s.start - rs;
            *s = Span { start: rto + delta, end: rto + delta + (s.end - s.start) };
        });
        self.e.toks.slide(&live);
        self.collect_at = to.saturating_mul(2).max(MIN_COLLECT);
        self.collect_buf = live;
    }
}

/// No node collection below this many nodes in the arena (about 3 MB).
pub(super) const MIN_NODE_COLLECT: u32 = 1 << 18;

/// One live bit per entry of a node table, and after `seal` the live entries before each
/// 64-entry word, so an entry's new index is a prefix plus one popcount.
#[derive(Debug, Default)]
struct Live {
    bits: Vec<u64>,
    before: Vec<u32>,
}

impl Live {
    fn reset(&mut self, len: usize) {
        self.bits.clear();
        self.bits.resize(len / 64 + 2, 0);
    }

    /// Marks entry `i`; whether it was unmarked.
    #[inline]
    fn set(&mut self, i: u32) -> bool {
        let (w, m) = ((i / 64) as usize, 1u64 << (i % 64));
        let new = self.bits[w] & m == 0;
        self.bits[w] |= m;
        new
    }

    fn seal(&mut self) {
        self.before.clear();
        let mut n = 0;
        for w in &self.bits {
            self.before.push(n);
            n += w.count_ones();
        }
    }

    /// The new index of entry `i`: the live entries before it.
    #[inline]
    fn rank(&self, i: u32) -> u32 {
        let w = (i / 64) as usize;
        self.before[w] + (self.bits[w] & ((1u64 << (i % 64)) - 1)).count_ones()
    }

    #[inline]
    fn span(&self, (s, e): (u32, u32)) -> (u32, u32) {
        if s >= e {
            (0, 0)
        } else {
            (self.rank(s), self.rank(s) + (e - s))
        }
    }

    /// Keeps the live entries of `v`, in order, in place.
    fn keep<T: Copy>(&self, v: &mut Vec<T>) {
        let mut j = 0;
        for (w, &bits) in self.bits.iter().enumerate() {
            let mut b = bits;
            while b != 0 {
                let i = w * 64 + b.trailing_zeros() as usize;
                b &= b - 1;
                if i < v.len() {
                    v[j] = v[i];
                    j += 1;
                }
            }
        }
        v.truncate(j);
    }
}

/// A holder of node-arena references outside the arena.
pub(super) enum Root<'a> {
    /// A list, `start..end` of [`Nodes::list`].
    List(&'a mut (u32, u32)),
    /// A box, an index into [`Nodes::boxes`].
    Box(&'a mut u32),
    /// A node of a list being built (its side-table indices).
    Node(&'a mut Node),
}

/// The node collector's reused tables: what is live in each node table, and the lists
/// still to scan.
#[derive(Debug, Default)]
pub(super) struct NodeGc {
    list: Live,
    boxes: Live,
    glues: Live,
    rules: Live,
    ligs: Live,
    discs: Live,
    inserts: Live,
    work: Vec<(u32, u32)>,
}

impl NodeGc {
    fn mark_box(&mut self, n: &Nodes, b: u32) {
        if (b as usize) < n.boxes.len() && self.boxes.set(b) {
            self.work.push(n.boxes[b as usize].list);
        }
    }

    fn mark_node(&mut self, n: &Nodes, x: Node) {
        match x.kind {
            Kind::Box => self.mark_box(n, x.a as u32),
            Kind::Rule => {
                self.rules.set(x.a as u32);
            }
            Kind::Glue => {
                self.glues.set(x.a as u32);
                match leader(x.sub) {
                    Some(true) => self.mark_box(n, x.b as u32),
                    Some(false) => {
                        self.rules.set(x.b as u32);
                    }
                    None => {}
                }
            }
            Kind::Lig => {
                let (s, e) = Nodes::block((x.b as u32, x.b as u32 + (x.sub >> 2) as u32));
                for i in s..e {
                    self.ligs.set(i);
                }
            }
            Kind::Disc if x.a >= 0 && self.discs.set(x.a as u32) => {
                let [pre, post] = n.discs[x.a as usize];
                self.work.push(pre);
                self.work.push(post);
            }
            Kind::Ins if self.inserts.set(x.a as u32) => self.work.push(n.inserts[x.a as usize].list),
            Kind::Adjust => self.work.push((x.a as u32, x.b as u32)),
            Kind::Whatsit if super::engine_whatsit::sized(x.sub) => {
                self.rules.set(x.a as u32);
            }
            _ => {}
        }
    }

    /// Marks a root and everything it reaches; each arena node is scanned once.
    fn mark(&mut self, n: &Nodes, r: Root) {
        match r {
            Root::List(l) => self.work.push(*l),
            Root::Box(b) => self.mark_box(n, *b),
            Root::Node(x) => self.mark_node(n, *x),
        }
        // A list keeps its whole block (model M's size classes) live.
        while let Some(l) = self.work.pop() {
            let (s, e) = Nodes::block(l);
            for k in s..e.min(n.list.len() as u32) {
                if self.list.set(k) {
                    self.mark_node(n, n.list[k as usize]);
                }
            }
        }
    }

    fn remap_node(&self, x: &mut Node) {
        match x.kind {
            Kind::Box => x.a = self.boxes.rank(x.a as u32) as i32,
            Kind::Rule => x.a = self.rules.rank(x.a as u32) as i32,
            Kind::Glue => {
                x.a = self.glues.rank(x.a as u32) as i32;
                match leader(x.sub) {
                    Some(true) => x.b = self.boxes.rank(x.b as u32) as i32,
                    Some(false) => x.b = self.rules.rank(x.b as u32) as i32,
                    None => {}
                }
            }
            Kind::Lig => x.b = self.ligs.rank(x.b as u32) as i32,
            Kind::Disc if x.a >= 0 => x.a = self.discs.rank(x.a as u32) as i32,
            Kind::Ins => x.a = self.inserts.rank(x.a as u32) as i32,
            Kind::Adjust => {
                let (a, b) = self.list.span((x.a as u32, x.b as u32));
                (x.a, x.b) = (a as i32, b as i32);
            }
            Kind::Whatsit if super::engine_whatsit::sized(x.sub) => x.a = self.rules.rank(x.a as u32) as i32,
            _ => {}
        }
    }

    fn remap(&self, r: Root) {
        match r {
            Root::List(l) => *l = self.list.span(*l),
            Root::Box(b) => *b = self.boxes.rank(*b),
            Root::Node(x) => self.remap_node(x),
        }
    }
}
