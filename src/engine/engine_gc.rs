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
    /// Collects the token arena when it has outgrown the last live size twice over.
    #[inline]
    pub(super) fn maybe_collect(&mut self) {
        if self.e.toks.len() as u32 >= self.collect_at {
            self.collect_tokens();
        }
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
