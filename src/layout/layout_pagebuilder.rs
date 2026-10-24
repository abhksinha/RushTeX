//! TeX's page builder (tex.web §980–§1028) over the immediate layout.
//!
//! Material is set as it arrives and may run past the bottom of the text block. Every
//! legal breakpoint gets TeX's cost: the badness of the page if it ended there (zero while
//! it fits, `\raggedbottom` having infinite stretch; the badness of shrinking the page's
//! vertical glue when it is too long) plus the breakpoint's penalty (club, widow, broken,
//! `\@secpenalty`, …). The cheapest breakpoint so far is remembered; when the page can no
//! longer shrink enough, it is broken there: the ops after that breakpoint move to the next
//! page, and if the page ended overfull its glue shrinks.

use crate::layout::engine::{Layouter, Paint, Sp};

/// `RUSHTEX_TRACE_PAGES` set: every breakpoint is written to stderr like pdfTeX's
/// `\tracingpages` (overflow is `t - g`), and every page break, for comparison with a
/// pdfTeX log.
fn trace_pages() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("RUSHTEX_TRACE_PAGES").is_some())
}

/// `RUSHTEX_TRACE_VLIST` set: every box (baseline) placed on the vertical list is written to
/// stderr with the distance from the previous one and the glue between them, like the
/// recipe `parity-testing boxes` prints from pdfTeX's `\showbox`. `gap` minus the listed items is
/// what the layout moved directly (interline glue and fixed skips).
pub(crate) fn trace_vlist() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("RUSHTEX_TRACE_VLIST").is_some())
}

/// TeX's `awful_bad`.
const AWFUL_BAD: i32 = 0x3FFF_FFFF;
/// `inf_bad`: a page this bad costs `deplorable`.
const INF_BAD: i32 = 10_000;
const DEPLORABLE: i32 = 100_000;
/// A penalty this high forbids a break; this low forces one.
pub(crate) const INF_PENALTY: i32 = 10_000;
pub(crate) const EJECT_PENALTY: i32 = -10_000;

/// latex.ltx / article.cls penalties.
pub(crate) const CLUB_PENALTY: i32 = 150;
pub(crate) const WIDOW_PENALTY: i32 = 150;
/// LaTeX's `\displaywidowpenalty`.
pub(crate) const DISPLAY_WIDOW_PENALTY: i32 = 50;
pub(crate) const BROKEN_PENALTY: i32 = 100;
/// `\@secpenalty`, before a sectioning command.
pub(crate) const SEC_PENALTY: i32 = -300;

#[derive(Clone, Copy)]
struct Breakpoint {
    ops_len: u32,
    pool_len: u32,
    /// Boxes contributed to the page before the breakpoint.
    seq: u32,
    /// `y` when the breakpoint was reached.
    y: Sp,
    /// Bottom of the last box before the breakpoint.
    bottom: Sp,
    /// Depth of the last box before the breakpoint.
    depth: f32,
    /// A forced break (`\newpage`): the page ends with `\vfil`.
    forced: bool,
    /// How far the page's last baseline was below the text block, and the page's shrink.
    overflow: f32,
    shrink: f32,
    /// Baseline and height of the first box after the breakpoint.
    first: Option<(Sp, f32)>,
    /// Shrinkable glue items before the breakpoint.
    marks_len: u32,
}

/// Page-builder state of the current page.
pub(crate) struct PageBuilder {
    /// Vertical glue that can stretch or shrink: (ops before it, boxes before it, shrink,
    /// stretch).
    shrink_marks: Vec<(u32, u32, f32, f32)>,
    shrink_total: f32,
    stretch_total: f32,
    /// `RUSHTEX_TRACE_VLIST`: the glue since the last box, and that box's baseline.
    trace: String,
    trace_y: Sp,
    best: Option<Breakpoint>,
    least_cost: i32,
    /// Boxes contributed to the current page.
    seq: u32,
    /// Baseline of the last box on the page (the page total runs to it, tex.web §1002).
    pub(crate) last_baseline: Sp,
    /// Table-of-contents entries recorded on this page: (entry, boxes before its heading).
    toc_marks: Vec<(usize, u32)>,
    /// `\label` whatsits recorded on this page: (label, boxes before it).
    label_marks: Vec<(u16, u32)>,
    /// The last item is a display's \belowdisplayskip, which follows its
    /// \postdisplaypenalty and so is no breakpoint.
    pub(crate) after_display: bool,
    /// The page's last item is the stretchable glue at this index of `shrink_marks`.
    last_glue: Option<usize>,
    /// The last item on the page is glue or a penalty (discardable, tex.web §148): glue
    /// after it is no legal breakpoint (§973).
    pub(crate) tail_discardable: bool,
    /// Natural width of the glue since the last box, and whether there is any: TeX's
    /// `\pagetotal` then holds the box's depth and that glue (tex.web §1004).
    glue_w: f32,
    glue_seen: bool,
    /// Natural width of `last_glue`.
    last_glue_w: f32,
    /// Shrink of `\textfloatsep` below the top floats: not part of the page so far (the
    /// float area is taken off `\pagegoal` instead), but `\@makecol` shrinks it with the
    /// rest when the page is set.
    float_sep_shrink: f32,
    /// Heights of the insertions on the page (footnotes with their `\skip\footins`), taken
    /// off the page goal.
    ins_total: f32,
}

impl PageBuilder {
    pub(crate) fn new() -> Self {
        Self {
            shrink_marks: Vec::new(),
            shrink_total: 0.0,
            stretch_total: 0.0,
            trace: String::new(),
            trace_y: Sp(0),
            best: None,
            least_cost: AWFUL_BAD,
            seq: 0,
            last_baseline: Sp::pt(16383.0),
            toc_marks: Vec::new(),
            label_marks: Vec::new(),
            after_display: false,
            last_glue: None,
            tail_discardable: false,
            glue_w: 0.0,
            glue_seen: false,
            last_glue_w: 0.0,
            float_sep_shrink: 0.0,
            ins_total: 0.0,
        }
    }

    /// A fresh page whose first baseline would be `top`.
    pub(crate) fn reset(&mut self, top: Sp) {
        if trace_vlist() {
            eprintln!("%v page top={top}");
            self.trace.clear();
            self.trace_y = top;
        }
        self.shrink_marks.clear();
        self.shrink_total = 0.0;
        self.stretch_total = 0.0;
        self.best = None;
        self.least_cost = AWFUL_BAD;
        self.seq = 0;
        self.last_baseline = top;
        self.toc_marks.clear();
        self.label_marks.clear();
        self.after_display = false;
        self.last_glue = None;
        self.tail_discardable = false;
        self.glue_w = 0.0;
        self.glue_seen = false;
        self.float_sep_shrink = 0.0;
        self.ins_total = 0.0;
    }
}

fn fmt_glue(word: &str, v: f32) -> String {
    if v == 0.0 {
        String::new()
    } else if v.is_infinite() {
        format!("{word} 1fil")
    } else {
        format!("{word} {v:.5}")
    }
}

/// TeX's badness of stretching or shrinking glue `s` by `t` (§108), on points.
fn badness_pt(t: f32, s: f32) -> i32 {
    crate::layout::layout_linebreak::badness(
        crate::backend::scaled::to_sp(t),
        crate::backend::scaled::to_sp(s),
    )
}

impl<'a> Layouter<'a> {
    /// Vertical glue of natural width `w` that may stretch by `stretch` and shrink by
    /// `shrink` (discarded at the top of a page).
    pub(crate) fn vglue(&mut self, w: f32, stretch: f32, shrink: f32) {
        if self.at_page_top() {
            return;
        }
        if trace_vlist() && !self.is_dry_run {
            use std::fmt::Write;
            let _ = write!(self.page_builder.trace, " glue {w:.5}{}{}", fmt_glue(" plus", stretch), fmt_glue(" minus", shrink));
        }
        self.y -= w;
        self.note_glue(w);
        if w > 0.0 {
            self.page_builder.last_glue = None;
        }
        if shrink > 0.0 || stretch > 0.0 {
            let pb = &mut self.page_builder;
            pb.shrink_marks.push((self.current_page.ops.len() as u32, pb.seq, shrink, stretch));
            pb.shrink_total += shrink;
            pb.stretch_total += stretch;
            // Glue of zero natural width (LaTeX's default `\parskip`) is no `\lastskip`
            // for `\addvspace` (`\ifdim\lastskip=\z@`).
            if w > 0.0 {
                pb.last_glue = Some(pb.shrink_marks.len() - 1);
                pb.last_glue_w = w;
            }
        }
    }

    /// Glue that sits above all the text on the page (`\textfloatsep` below the top
    /// floats): it does not count for the choice of the break, but shrinks with the page
    /// when it is set.
    pub(crate) fn page_top_glue(&mut self, stretch: f32, shrink: f32) {
        let pb = &mut self.page_builder;
        pb.shrink_marks.insert(0, (0, 0, shrink, stretch));
        pb.float_sep_shrink += shrink;
        if let Some(best) = pb.best.as_mut() {
            best.marks_len += 1;
        }
        if let Some(k) = pb.last_glue.as_mut() {
            *k += 1;
        }
    }

    /// An insertion of total height `h` with the glue `(stretch, shrink)` it brings (its
    /// class's skip, for the first one on the page).
    pub(crate) fn add_insertion(&mut self, h: f32, glue: (f32, f32)) {
        let pb = &mut self.page_builder;
        pb.ins_total += h;
        pb.stretch_total += glue.0;
        pb.shrink_total += glue.1;
    }

    #[inline(always)]
    pub(crate) fn page_builder_seq(&self) -> u32 {
        self.page_builder.seq
    }

    /// Bottom of the room for text on the page: `\pagegoal` less the insertions.
    #[inline(always)]
    pub(crate) fn goal_bottom(&self) -> Sp {
        Sp::pt(self.margin_bottom) + self.page_builder.ins_total
    }

    /// `\vskip-\lastskip`: the last glue is cancelled with its stretch and shrink, as
    /// `\addvspace` does before adding a larger skip. Its natural width is the caller's.
    /// latex.ltx `\vspace` ends with `\vskip\z@skip`: `\lastskip` is 0 afterwards, so a
    /// following `\addvspace` or `\addpenalty` neither merges with nor hops over the skip.
    /// Glue of natural width `w` joined the page.
    pub(crate) fn note_glue(&mut self, w: f32) {
        let pb = &mut self.page_builder;
        pb.glue_w += w;
        pb.glue_seen = true;
        pb.tail_discardable = true;
    }

    /// A mark or whatsit joined the page: `\lastskip` is zero, and the item is not
    /// discardable (tex.web §148), so glue after it is a breakpoint.
    pub(crate) fn note_vmark(&mut self) {
        self.page_builder.last_glue = None;
        self.page_builder.tail_discardable = false;
    }

    pub(crate) fn zero_lastskip(&mut self) {
        self.page_builder.last_glue = None;
        self.page_builder.tail_discardable = true;
    }

    pub(crate) fn cancel_last_glue(&mut self) {
        let pb = &mut self.page_builder;
        if let Some(k) = pb.last_glue.take() {
            let m = &mut pb.shrink_marks[k];
            pb.shrink_total -= m.2;
            pb.stretch_total -= m.3;
            m.2 = 0.0;
            m.3 = 0.0;
        }
    }

    /// `\endtrivlist`: a positive `\lastskip` (`last`) is replaced by itself plus `d`, the
    /// list's `\parskip` less `\@outerparskip`. Returns the new `\lastskip`.
    pub(crate) fn endtrivlist_skip(&mut self, last: f32, d: (f32, f32, f32)) -> f32 {
        let (st, sh) = match self.page_builder.last_glue {
            Some(k) => (self.page_builder.shrink_marks[k].3, self.page_builder.shrink_marks[k].2),
            None => (0.0, 0.0),
        };
        self.cancel_last_glue();
        self.y += last;
        self.note_glue(-last);
        let w = last + d.0;
        self.vglue(w, (st + d.1).max(0.0), (sh + d.2).max(0.0));
        w
    }

    /// `\addvspace` replaced the last skip by a larger one (`\vskip-\lastskip\vskip#1`):
    /// the glue just added stands for all of `w`, the new `\lastskip`.
    pub(crate) fn set_lastskip_width(&mut self, w: f32) {
        if self.page_builder.last_glue.is_some() {
            self.page_builder.last_glue_w = w;
        }
    }

    /// Records that a box of height `h` with its baseline at `baseline` joined the page.
    pub(crate) fn note_box(&mut self, baseline: Sp, h: f32) {
        if trace_vlist() && !self.is_dry_run {
            let pb = &mut self.page_builder;
            eprintln!("%v box seq={} y={baseline} ysp={} h={h:.5} gap={} <-{}", pb.seq, baseline.0, pb.trace_y - baseline, pb.trace);
            pb.trace.clear();
            pb.trace_y = baseline;
        }
        let pb = &mut self.page_builder;
        if let Some(best) = pb.best.as_mut() {
            if best.first.is_none() {
                best.first = Some((baseline, h));
            }
        }
        pb.seq += 1;
        pb.last_baseline = baseline;
        pb.after_display = false;
        pb.last_glue = None;
        pb.tail_discardable = false;
        pb.glue_w = 0.0;
        pb.glue_seen = false;
        self.ignore_depth = false;
        self.strut_height = 0.0;
    }

    /// The page number a heading set now will be printed on, corrected if the page breaks
    /// before it later.
    /// A `\label` whatsit before the next box: its page is this one unless that box moves to
    /// the next page.
    pub(crate) fn note_page_label(&mut self, id: u16) {
        let i = id as usize;
        if self.registry.label_pages.len() <= i {
            self.registry.label_pages.resize(i + 1, 0);
        }
        self.registry.label_pages[i] = self.current_page_number();
        let seq = self.page_builder.seq;
        self.page_builder.label_marks.push((id, seq));
    }

    pub(crate) fn note_toc_entry(&mut self, idx: usize) {
        let seq = self.page_builder.seq;
        self.page_builder.toc_marks.push((idx, seq));
    }

    /// latex.ltx `\addpenalty`: after a skip, the penalty goes in front of it (`\vskip
    /// -\lastskip \penalty #1 \vskip \lastskip`), so the page that ends there does not
    /// hold the skip's stretch and shrink, and the skip is discarded after the break.
    pub(crate) fn page_addpenalty(&mut self, penalty: i32) {
        let pb = &mut self.page_builder;
        let k = match pb.last_glue {
            Some(k) if k + 1 == pb.shrink_marks.len() => k,
            _ => return self.page_breakpoint(penalty),
        };
        let mark = pb.shrink_marks.pop().expect("last glue mark");
        pb.shrink_total -= mark.2;
        pb.stretch_total -= mark.3;
        // `\vskip -(\lastskip + \prevdepth)`: neither the skip nor the depth is on the page.
        let (glue_w, glue_seen, last_w) = (pb.glue_w, pb.glue_seen, pb.last_glue_w);
        pb.glue_w -= last_w;
        if pb.glue_w.abs() < 0.001 {
            pb.glue_seen = false;
        }
        let page = self.current_page_number();
        self.page_breakpoint(penalty);
        if self.current_page_number() == page {
            let pb = &mut self.page_builder;
            pb.shrink_marks.push(mark);
            pb.shrink_total += mark.2;
            pb.stretch_total += mark.3;
            pb.last_glue = Some(k);
            pb.glue_w = glue_w;
            pb.glue_seen = glue_seen;
        }
    }

    /// A legal breakpoint with penalty `penalty` (tex.web §1005).
    pub(crate) fn page_breakpoint(&mut self, penalty: i32) {
        self.page_builder.tail_discardable = true;
        if penalty >= INF_PENALTY || self.page_builder.seq == 0 {
            return;
        }
        // The last box's depth beyond \maxdepth (.5\topskip) counts towards the page
        // total (tex.web §1003).
        let max_depth = 0.5 * self.top_skip;
        let pb = &self.page_builder;
        let overflow = if pb.glue_seen {
            (self.goal_bottom() - pb.last_baseline + self.prev_depth.max(0.0) + pb.glue_w).to_pt()
        } else {
            (self.goal_bottom() - pb.last_baseline + (self.prev_depth - max_depth).max(0.0)).to_pt()
        };
        let shrink = self.page_builder.shrink_total;
        // The page is not yet followed by \raggedbottom's fil glue, so an underfull page
        // costs the badness of stretching its finite glue.
        let b = if overflow < -0.005 {
            badness_pt(-overflow, self.page_builder.stretch_total)
        } else if overflow <= 0.005 {
            0
        } else if overflow > shrink + 0.005 {
            AWFUL_BAD
        } else {
            badness_pt(overflow, shrink)
        };
        let c = if b < AWFUL_BAD {
            if penalty <= EJECT_PENALTY {
                penalty
            } else if b < INF_BAD {
                b + penalty
            } else {
                DEPLORABLE
            }
        } else {
            b
        };
        if trace_pages() {
            let pb = &self.page_builder;
            eprintln!(
                "% pg={} over={:.2} plus {:.2} minus {:.2} p={penalty} b={b} c={c}",
                self.current_page_number(),
                overflow,
                pb.stretch_total,
                shrink
            );
        }
        if c <= self.page_builder.least_cost {
            self.page_builder.least_cost = c;
            self.page_builder.best = Some(Breakpoint {
                ops_len: self.current_page.ops.len() as u32,
                pool_len: self.current_page.text_pool.len() as u32,
                seq: self.page_builder.seq,
                y: self.y,
                bottom: self.page_builder.last_baseline - self.prev_depth,
                depth: self.prev_depth,
                forced: penalty <= EJECT_PENALTY,
                overflow: overflow.max(0.0),
                shrink,
                first: None,
                marks_len: self.page_builder.shrink_marks.len() as u32,
            });
        }
        if c == AWFUL_BAD || penalty <= EJECT_PENALTY {
            let fired_here = self.page_builder.best.is_some_and(|bp| bp.seq == self.page_builder.seq);
            self.fire_page();
            // The material after the old best breakpoint is on the new page now, and this
            // breakpoint is considered again there.
            if !fired_here && penalty < INF_PENALTY {
                self.page_breakpoint(penalty);
            }
        }
    }

    /// Breaks the page at the best breakpoint (TeX's `fire_up`).
    fn fire_page(&mut self) {
        let Some(best) = self.page_builder.best else {
            self.new_page();
            return;
        };
        // Glue on the page shrinks when it ended overfull. The page builder counts each
        // footnote's height plus depth (§1009), but `\@makecol` unboxes the text and the
        // footnotes into one `\vbox to \@colht`: the text's last depth is inside it, the
        // last footnote's depth hangs below it.
        let shrink = best.shrink + self.page_builder.float_sep_shrink;
        let overflow = match self.last_footnote_depth(best.seq) {
            Some(foot_depth) => (best.overflow + best.depth - foot_depth).max(0.0),
            None => best.overflow,
        };
        if trace_vlist() && !self.is_dry_run {
            let to_sp = crate::backend::scaled::to_sp;
            eprintln!("%v fire seq={} overflow={overflow:.5} shrink={shrink:.5} ({}sp / {}sp) last_baseline={} depth={}sp", best.seq, to_sp(overflow), to_sp(shrink), (best.bottom + best.depth).0, to_sp(best.depth));
        }
        if overflow > 0.0 && shrink > 0.0 && !self.is_dry_run {
            // `vpack` sets glue_set = excess / total shrink (a double), and `vlist_out` moves
            // each item by round(glue_set × the shrink accumulated above it) (§625), in sp.
            let to_sp = crate::backend::scaled::to_sp;
            let ratio = to_sp(overflow) as f64 / to_sp(shrink) as f64;
            let marks = &self.page_builder.shrink_marks[..best.marks_len as usize];
            let mut cum = 0i64;
            let mut k = 0;
            let end = best.ops_len as usize;
            for (i, op) in self.current_page.ops[..end].iter_mut().enumerate() {
                while k < marks.len() && marks[k].0 as usize <= i {
                    cum += to_sp(marks[k].2) as i64;
                    k += 1;
                }
                if cum > 0 {
                    op.shift_y(Sp((ratio * cum as f64).round() as i32));
                }
            }
        }
        // The footnotes start `\skip\footins` below the text's last box; both move up with
        // the shrinking glue.
        let moved_foot = {
            let ratio = if overflow > 0.0 && shrink > 0.0 { overflow / shrink } else { 0.0 };
            let text_shrink: f32 = self.page_builder.shrink_marks[..best.marks_len as usize].iter().map(|m| m.2).sum();
            let (skip, skip_shrink) = self.footins_glue();
            let top = best.bottom + ratio * text_shrink - (skip - ratio * skip_shrink);
            self.split_footnotes(best.seq, (!best.forced).then_some(top))
        };
                let moved_ops: Vec<Paint> = self.current_page.ops.split_off(best.ops_len as usize);
        let moved_pool: Vec<&'a str> = self.current_page.text_pool.split_off(best.pool_len as usize);
        // Glue after the first moved box stays with it; glue before it is discarded.
        let moved_marks: Vec<(u32, u32, f32, f32)> = self.page_builder.shrink_marks[best.marks_len as usize..]
            .iter()
            .filter(|m| m.1 > best.seq)
            .map(|&(o, s, w, st)| (o.saturating_sub(best.ops_len), s - best.seq, w, st))
            .collect();
        let moved_boxes = self.page_builder.seq - best.seq;
        let last_baseline = self.page_builder.last_baseline;
        for &(idx, seq) in &self.page_builder.toc_marks {
            if seq >= best.seq {
                if let Some(e) = self.registry.toc_entries.get_mut(idx) {
                    e.page += 1;
                }
            }
        }
        for &(id, seq) in &self.page_builder.label_marks {
            if seq >= best.seq {
                if let Some(p) = self.registry.label_pages.get_mut(id as usize) {
                    *p += 1;
                }
            }
        }
        let old_y = self.y;
        // The boxes that move keep the paragraph they belong to: a display after them still
        // sees its last line (`\predisplaysize`, `\prevgraf`, `\prevdepth`).
        let par_state = (self.has_preceding_text, self.last_line_width, self.last_line_indent, self.prev_graf, self.prev_depth);

        self.new_page();

        // The glue before the first moved box is discarded; that box sits at \topskip.
        match best.first {
            Some((baseline, h)) => {
                let target = self.page_start_y() - (h - self.top_skip).max(0.0);
                let dy = target - baseline;
                let base = self.current_page.text_pool.len() as u32;
                self.current_page.text_pool.extend_from_slice(&moved_pool);
                let ops_base = self.current_page.ops.len() as u32;
                for mut op in moved_ops {
                    if let Paint::GlyphRun { part_start, .. } = &mut op {
                        *part_start = *part_start - best.pool_len + base;
                    }
                    op.shift_y(dy);
                    self.current_page.ops.push(op);
                }
                self.y = old_y + dy;
                (self.has_preceding_text, self.last_line_width, self.last_line_indent, self.prev_graf, self.prev_depth) = par_state;
                let pb = &mut self.page_builder;
                pb.seq = moved_boxes;
                pb.last_baseline = last_baseline + dy;
                for (o, s, w, st) in moved_marks {
                    pb.shrink_marks.push((o + ops_base, s, w, st));
                    pb.shrink_total += w;
                    pb.stretch_total += st;
                }
            }
            None => {
                // Only glue (and colour) followed the breakpoint.
                let base = self.current_page.text_pool.len() as u32;
                self.current_page.text_pool.extend_from_slice(&moved_pool);
                for mut op in moved_ops {
                    if let Paint::GlyphRun { part_start, .. } = &mut op {
                        *part_start = *part_start - best.pool_len + base;
                    }
                    self.current_page.ops.push(op);
                }
                let _ = (best.y, old_y);
            }
        }
        self.reinsert_footnotes(moved_foot);
    }

    /// `\clearpage` at the end: a page that ran past the bottom breaks at its best place.
    pub(crate) fn settle_page(&mut self) {
        let mut guard = 0;
        while self.page_builder.seq > 0
            && self.page_builder.last_baseline < self.margin_bottom - 0.005
            && guard < 64
        {
            guard += 1;
            let overflow = self.goal_bottom() - self.page_builder.last_baseline;
            if overflow <= self.page_builder.shrink_total + 0.005 {
                // It fits by shrinking: end the page here.
                self.page_breakpoint(EJECT_PENALTY);
                break;
            }
            self.fire_page();
        }
    }
}
