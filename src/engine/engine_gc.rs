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

use super::engine_node::{Kind, Node};
#[cfg(all(test, feature = "oracle"))]
use super::engine_node::{leader, Nodes};
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
    /// Collects the token arena when it has outgrown its last live size twice over, and
    /// frees the boxes released since the last command. Nodes are never traced: model M
    /// frees each at its owner's free point (`2026-10-22-zero-gc-core-model.md`).
    #[inline]
    pub(super) fn maybe_collect(&mut self) {
        if self.e.toks.len() as u32 >= self.collect_at {
            // A watch session's checkpoint is due: its threshold was zeroed so that this test,
            // made between commands anyway, finds it (`engine_checkpoint`).
            if self.checkpoints.as_ref().is_some_and(|c| c.due) {
                return self.checkpoint_top();
            }
            self.collect_tokens();
        }
        if self.e.eqtb.has_dead_boxes() {
            self.flush_dead_boxes();
        }
    }

    /// Every holder of node-arena references outside the arena: the box registers and the
    /// save stack, the lists being built, the current page, the shipped pages a harness
    /// keeps, alignments' adjustments, open discretionaries and `\pdfxform`s. §814 `just_box` is not one: it is read only right after the line
    /// break that sets it, within one command.
    #[cfg(all(test, feature = "oracle"))]
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
        for x in &mut self.pdf_doc.forms {
            f(Root::Box(&mut x.box_rec));
        }
        let levels = self.nest.iter().map(|l| (l.mode, l.aux)).chain([(self.mode, self.aux)]);
        for (mode, aux) in levels {
            if mode == super::engine_run::Mode::Math && aux >= 0 {
                f(Root::Noad(aux as u32));
            }
        }
    }

    /// Flushes the boxes assignments and group ends released (§275 `eq_destroy`).
    pub(super) fn flush_dead_boxes(&mut self) {
        let mut dead = std::mem::take(&mut self.dead_buf);
        self.e.eqtb.take_dead_boxes(&mut dead);
        for b in dead.drain(..) {
            self.e.nodes.flush_box(b);
        }
        self.dead_buf = dead;
    }

    /// Model M's leak audit (`RT_LEAKS`): per node table, the entries neither free nor
    /// reachable from a root. Zero everywhere means every free point is in place.
    #[cfg(all(test, feature = "oracle"))]
    pub(super) fn node_leaks(&mut self) -> String {
        self.flush_dead_boxes();
        let nodes = std::mem::take(&mut self.e.nodes);
        let mut gc = NodeGc::default();
        gc.list.reset(nodes.list.len());
        gc.boxes.reset(nodes.boxes.len());
        gc.glues.reset(nodes.glues.len());
        gc.rules.reset(nodes.rules.len());
        gc.ligs.reset(nodes.lig_chars.len());
        gc.discs.reset(nodes.discs.len());
        gc.inserts.reset(nodes.inserts.len());
        let noads = std::mem::take(&mut self.noads);
        self.for_each_node_root(&mut |r| gc.mark(&nodes, &noads, r));
        self.noads = noads;
        let f = &nodes.free;
        let blocks = |v: &[Vec<u32>]| -> usize { v.iter().enumerate().map(|(c, s)| s.len() * super::engine_node::class_len(c)).sum() };
        let row = |name: &str, len: usize, free: usize, live: &Live| {
            let live: usize = live.bits.iter().map(|w| w.count_ones() as usize).sum();
            format!("{name} {} of {len} ", len as i64 - free as i64 - live as i64)
        };
        let s = [
            row("nodes", nodes.list.len(), blocks(&f.spans), &gc.list),
            row("boxes", nodes.boxes.len(), f.boxes.len(), &gc.boxes),
            row("glues", nodes.glues.len(), f.glues.len(), &gc.glues),
            row("rules", nodes.rules.len(), f.rules.len(), &gc.rules),
            row("ligs", nodes.lig_chars.len(), blocks(&f.ligs), &gc.ligs),
            row("discs", nodes.discs.len(), f.discs.len(), &gc.discs),
            row("inserts", nodes.inserts.len(), f.inserts.len(), &gc.inserts),
        ]
        .concat();
        #[cfg(feature = "ownership-check")]
        let s = s + &leak_sites(&nodes, &gc);
        self.e.nodes = nodes;
        format!("LEAKS {s}")
    }



    /// Every token span the run can still reach, each given to `f` (which may move it).
    fn for_each_root(&mut self, f: &mut dyn FnMut(&mut Span)) {
        self.e.eqtb.for_each_span(&mut *f);
        self.input.for_each_list(&mut *f);
        for a in &mut self.aligns {
            a.for_each_span(f);
        }
        self.page.for_each_span(f);
        for n in self.e.nodes.list.as_mut_slice().iter_mut().chain(self.cur.iter_mut()) {
            node_span(n, f);
        }
        for n in self.adjust.iter_mut().chain(self.adjust_out.iter_mut()).flatten() {
            node_span(n, f);
        }
    }

    pub(super) fn collect_tokens(&mut self) {
        // One live bit per arena token, set over every root span: its maximal runs are the
        // live ranges sorted and merged (touching spans join, as overlapping ones do), and a
        // token's new place is the number of live bits before it. No sort of the spans.
        let mut bits = vec![0u64; self.e.toks.len().div_ceil(64)];
        self.for_each_root(&mut |s| {
            if !s.is_empty() {
                set_bits(&mut bits, s.start, s.end);
            }
        });
        // `rank[w]`: the live tokens before word `w`.
        let mut live = std::mem::take(&mut self.collect_buf);
        live.clear();
        let mut rank = Vec::with_capacity(bits.len());
        let mut to = 0u32;
        let mut open: Option<u32> = None;
        for (w, &x) in bits.iter().enumerate() {
            rank.push(to);
            to += x.count_ones();
            let base = w as u32 * 64;
            let mut i = 0u32;
            while i < 64 {
                match open {
                    Some(start) => {
                        let z = !x >> i;
                        if z == 0 {
                            break;
                        }
                        i += z.trailing_zeros();
                        live.push((start, base + i, 0));
                        open = None;
                    }
                    None => {
                        let o = x >> i;
                        if o == 0 {
                            break;
                        }
                        i += o.trailing_zeros();
                        open = Some(base + i);
                    }
                }
            }
        }
        if let Some(start) = open {
            live.push((start, bits.len() as u32 * 64, 0));
        }
        let mut next = 0;
        for r in live.iter_mut() {
            r.2 = next;
            next += r.1 - r.0;
        }
        debug_assert_eq!(next, to);
        // Each span moves with the range holding its start: every token between the range's
        // start and the span's is live, so its new start is the live count before it (both
        // passes visit the same roots).
        self.for_each_root(&mut |s| {
            if s.is_empty() {
                *s = Span::default();
                return;
            }
            let (w, b) = ((s.start / 64) as usize, s.start % 64);
            let start = rank[w] + (bits[w] & ((1u64 << b) - 1)).count_ones();
            *s = Span { start, end: start + (s.end - s.start) };
        });
        self.e.toks.slide(&live);
        self.collect_at = to.saturating_mul(2).max(MIN_COLLECT);
        self.collect_buf = live;
    }
}

/// Sets bits `a..b` (`a < b`) of `bits`, growing it if a span ends past the arena.
fn set_bits(bits: &mut Vec<u64>, a: u32, b: u32) {
    let (wa, wb) = ((a / 64) as usize, ((b - 1) / 64) as usize);
    if wb >= bits.len() {
        bits.resize(wb + 1, 0);
    }
    let lo = !0u64 << (a % 64);
    let hi = !0u64 >> (63 - (b - 1) % 64);
    if wa == wb {
        bits[wa] |= lo & hi;
    } else {
        bits[wa] |= lo;
        bits[wa + 1..wb].fill(!0);
        bits[wb] |= hi;
    }
}

/// One reachable bit per entry of a node table (the leak audit's).
#[cfg(all(test, feature = "oracle"))]
#[derive(Debug, Default)]
struct Live {
    bits: Vec<u64>,
}

#[cfg(all(test, feature = "oracle"))]
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




}

/// A holder of node-arena references outside the arena.
#[cfg(all(test, feature = "oracle"))]
pub(super) enum Root<'a> {
    /// A list, `start..end` of [`Nodes::list`].
    List(&'a mut (u32, u32)),
    /// A box, an index into [`Nodes::boxes`].
    Box(&'a mut u32),
    /// A node of a list being built (its side-table indices).
    Node(&'a mut Node),
    /// An incompleat fraction noad (§1181: a math level's `aux`); noad indices never move.
    Noad(u32),
}

/// The leak audit's tables: what is reachable in each node table, and the lists still to
/// scan.
#[cfg(all(test, feature = "oracle"))]
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

#[cfg(all(test, feature = "oracle"))]
impl NodeGc {
    fn mark_box(&mut self, n: &Nodes, b: u32) {
        if (b as usize) < n.boxes.len() && self.boxes.set(b) {
            self.work.push(n.boxes[b as usize].list);
        }
    }

    fn mark_node(&mut self, n: &Nodes, noads: &[super::engine_mlist::Noad], x: Node) {
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
            // §698: a noad's fields.
            Kind::Noad => {
                if let Some(q) = noads.get(x.a as usize) {
                    for f in [q.nucleus, q.supscr, q.subscr, q.extra] {
                        match f {
                            super::engine_mlist::Field::Box(b) => self.mark_box(n, b),
                            super::engine_mlist::Field::Mlist(a, b) => self.work.push((a, b)),
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Marks a root and everything it reaches; each arena node is scanned once.
    fn mark(&mut self, n: &Nodes, noads: &[super::engine_mlist::Noad], r: Root) {
        match r {
            Root::List(l) => self.work.push(*l),
            Root::Box(b) => self.mark_box(n, *b),
            Root::Node(x) => self.mark_node(n, noads, *x),
            Root::Noad(q) => self.mark_node(n, noads, super::engine_mlist::noad_node(q)),
        }
        // A list keeps its whole block (model M's size classes) live.
        while let Some(l) = self.work.pop() {
            let (s, e) = Nodes::block(l);
            for k in s..e.min(n.list.len() as u32) {
                if self.list.set(k) {
                    self.mark_node(n, noads, n.list[k as usize]);
                }
            }
        }
    }


}

/// The ownership-check build's leaks by the source line that made them, most first.
#[cfg(all(test, feature = "oracle", feature = "ownership-check"))]
fn leak_sites(nodes: &Nodes, gc: &NodeGc) -> String {
    use std::collections::HashMap;
    let is = |l: &Live, i: usize| l.bits.get(i / 64).is_some_and(|w| w >> (i % 64) & 1 == 1);
    let mut sites: HashMap<String, usize> = HashMap::new();
    let free_boxes: std::collections::HashSet<u32> = nodes.free.boxes.iter().copied().collect();
    for (i, o) in nodes.origin.boxes.iter().enumerate() {
        if let Some(o) = o {
            if !is(&gc.boxes, i) && !free_boxes.contains(&(i as u32)) {
                *sites.entry(format!("box {o}")).or_default() += 1;
            }
        }
    }
    let free_glues: std::collections::HashSet<u32> = nodes.free.glues.iter().copied().collect();
    for (i, o) in nodes.origin.glues.iter().enumerate() {
        if let Some(o) = o {
            if !is(&gc.glues, i) && !free_glues.contains(&(i as u32)) {
                *sites.entry(format!("glue {o}")).or_default() += 1;
            }
        }
    }
    let free_lists: std::collections::HashSet<u32> = nodes.free.spans.iter().flatten().copied().collect();
    for (&s, o) in &nodes.origin.lists {
        if !is(&gc.list, s as usize) && !free_lists.contains(&s) {
            *sites.entry(format!("list {o}")).or_default() += 1;
        }
    }
    let free_ligs: std::collections::HashSet<u32> = nodes.free.ligs.iter().flatten().copied().collect();
    for (&s, o) in &nodes.origin.ligs {
        if !is(&gc.ligs, s as usize) && !free_ligs.contains(&s) {
            *sites.entry(format!("lig {o}")).or_default() += 1;
        }
    }
    let free_discs: std::collections::HashSet<u32> = nodes.free.discs.iter().copied().collect();
    for (&d, (o, freed)) in &nodes.origin.disc_seen {
        if !is(&gc.discs, d as usize) && !free_discs.contains(&d) {
            *sites.entry(format!("disc last {} {o}", if *freed { "freed in a list" } else { "stored" })).or_default() += 1;
        }
    }
    // Freed yet reachable: a dangling reference (the worst kind of bug here).
    for &s in &free_lists {
        if is(&gc.list, s as usize) {
            let o = nodes.origin.lists.get(&s).map_or("?".to_string(), |o| o.to_string());
            *sites.entry(format!("DANGLING list {o}")).or_default() += 1;
        }
    }
    for &b in &free_boxes {
        if is(&gc.boxes, b as usize) {
            let o = nodes.origin.boxes.get(b as usize).copied().flatten().map_or("?".to_string(), |o| o.to_string());
            *sites.entry(format!("DANGLING box {o}")).or_default() += 1;
        }
    }
    let mut v: Vec<_> = sites.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.iter().take(40).map(|(k, n)| format!("\n  {n:6} {k}")).collect()
}
