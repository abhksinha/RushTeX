//! Float placement following LaTeX's output routine (latex.ltx `\@addtocurcol`,
//! `\@startcolumn`): a float goes in the text (`h`), at the top of the current page if it
//! still fits there (`t`), or waits on the deferred list for the top of a later page. Floats
//! keep their order, at most `\topnumber` sit at the top of a page, and together they take
//! at most `\topfraction` of the text height. A float too tall for that gets a page of its
//! own (`p`).
//!
//! A float is set once into its own op list with its top edge at y = 0; placing it only
//! offsets those ops. The text already on a page moves down when a top float joins it.

use crate::frontend::ast::Inline;
use crate::layout::engine::{Layouter, Paint, Sp};
use crate::tables::model::{FLOAT_FORCE, FLOAT_HERE, FLOAT_PAGE, FLOAT_TOP};

/// Placement bit marking a `table` float, whose order is kept apart from the figures'.
pub(crate) const FLOAT_TABLE_KIND: u8 = 0x40;
/// `\floatpagefraction` (article.cls: .5).
const FLOAT_PAGE_FRACTION: f32 = 0.5;
/// `\@fpsep` (8pt plus 2fil) between the floats of a float page, which `\@fptop` and
/// `\@fpbot` (0pt plus 1fil) enclose.
const FP_SEP: f32 = 8.0;

/// `\topnumber` (article.cls: 2).
const TOP_NUMBER: u8 = 2;
/// `\topfraction` (article.cls: .7).
const TOP_FRACTION: f32 = 0.7;
/// `\floatsep` (10pt class: 12pt plus 2pt minus 2pt) between two top floats.
const FLOAT_SEP: f32 = 12.0;
/// `\textfloatsep` (20pt plus 2pt minus 4pt) between the top floats and the text.
const TEXT_FLOAT_SEP: f32 = 20.0;
const TEXT_FLOAT_SEP_STRETCH: f32 = 2.0;
const TEXT_FLOAT_SEP_SHRINK: f32 = 4.0;
/// `\intextsep` (12pt plus 2pt minus 2pt) above and below an `h` float.
const IN_TEXT_SEP: f32 = 12.0;

/// `\skip\footins`: 9pt plus 4pt minus 2pt in size10.clo, 10pt in size11.clo, 10.8pt in
/// size12.clo (other sizes scale the 10pt value).
fn footins_skip(class_size: f32) -> (f32, f32, f32) {
    let w = if (class_size - 10.95).abs() < 0.1 {
        10.0
    } else if (class_size - 12.0).abs() < 0.1 {
        10.8
    } else {
        0.9 * class_size
    };
    (w, 4.0, 2.0)
}

/// A float set into its own ops, top edge at y = 0.
pub(crate) struct FloatBox<'a> {
    ops: Vec<Paint>,
    pool: Vec<&'a str>,
    /// Height of the box from its top edge to its bottom edge.
    height: f32,
    /// Depth of the box's last line (part of `height`), for boxes set to a baseline.
    depth: f32,
    placement: u8,
}

/// Floats of the current page and those deferred to later pages.
pub(crate) struct FloatState<'a> {
    /// LaTeX's `\@deferlist`.
    deferred: std::collections::VecDeque<FloatBox<'a>>,
    /// Ops of the top floats on the current page, merged into it when it is completed.
    page_ops: Vec<Paint>,
    page_pool: Vec<&'a str>,
    /// Height of the top floats on the current page, with the `\floatsep`s between them.
    pub(crate) area_h: f32,
    /// LaTeX's `\@topnum`: top floats the current page still accepts.
    pub(crate) top_left: u8,
    /// Footnotes (`\footins` insertions) of the current page, with the number of boxes on
    /// the page when each was inserted.
    foot: Vec<(u32, FloatBox<'a>)>,
    /// Top of the footnotes, when the page builder fixed it at the page break.
    foot_top: Option<Sp>,
    /// Footnotes set ahead of the paragraph that holds their marks, keyed by the mark's
    /// address; each is inserted after the line its mark ends up in.
    pending_foot: Vec<(usize, FloatBox<'a>)>,
}

impl FloatState<'_> {
    pub(crate) fn new() -> Self {
        Self {
            deferred: std::collections::VecDeque::new(),
            page_ops: Vec::new(),
            page_pool: Vec::new(),
            area_h: 0.0,
            top_left: TOP_NUMBER,
            foot: Vec::new(),
            foot_top: None,
            pending_foot: Vec::new(),
        }
    }
}

impl Paint {
    /// Moves the op horizontally by `dx`.
    pub(crate) fn shift_x(&mut self, dx: Sp) {
        match self {
            Paint::GlyphRun { x, .. }
            | Paint::Marker { x, .. }
            | Paint::Rule { x, .. }
            | Paint::CaptionTag { x, .. }
            | Paint::TableCaptionTag { x, .. }
            | Paint::PageNumber { x, .. }
            | Paint::SectionNumber { x, .. }
            | Paint::FigureTag { x, .. }
            | Paint::TableTag { x, .. }
            | Paint::EquationNumber { x, .. }
            | Paint::VectorGlyph { x, .. }
            | Paint::CidGlyphRun { x, .. } => *x += dx,
            Paint::Color { .. } => {}
        }
    }

    /// Moves the op vertically by `dy`.
    pub(crate) fn shift_y(&mut self, dy: Sp) {
        match self {
            Paint::GlyphRun { y, .. }
            | Paint::Marker { y, .. }
            | Paint::Rule { y, .. }
            | Paint::CaptionTag { y, .. }
            | Paint::TableCaptionTag { y, .. }
            | Paint::PageNumber { y, .. }
            | Paint::SectionNumber { y, .. }
            | Paint::FigureTag { y, .. }
            | Paint::TableTag { y, .. }
            | Paint::EquationNumber { y, .. }
            | Paint::VectorGlyph { y, .. }
            | Paint::CidGlyphRun { y, .. } => *y += dy,
            Paint::Color { .. } => {}
        }
    }
}

/// Appends `ops`, which index into `pool`, to a page with its own text pool.
fn append_ops<'a>(dst_ops: &mut Vec<Paint>, dst_pool: &mut Vec<&'a str>, ops: &[Paint], pool: &[&'a str], dy: Sp) {
    let base = dst_pool.len() as u32;
    dst_pool.extend_from_slice(pool);
    dst_ops.reserve(ops.len());
    for &op in ops {
        let mut op = op;
        if let Paint::GlyphRun { part_start, .. } = &mut op {
            *part_start += base;
        }
        op.shift_y(dy);
        dst_ops.push(op);
    }
}

/// The page bottom while a box is set apart from the page (`-\maxdimen`): nothing in it
/// breaks the page, and positions stay within TeX's dimensions.
const BOX_BOTTOM: f32 = -16383.0;

impl<'a> Layouter<'a> {
    /// `\textfloatsep` below the top floats, or nothing on a page without them.
    #[inline(always)]
    pub(crate) fn top_float_offset(&self) -> f32 {
        if self.floats.area_h > 0.0 { self.floats.area_h + TEXT_FLOAT_SEP } else { 0.0 }
    }

    /// Sets a float with `set`, which lays it out from `self.y`, into its own ops. `top_y`
    /// is the `self.y` at which the box's top edge comes out at y = 0. The page cannot
    /// break inside a float.
    pub(crate) fn capture_float(&mut self, top_y: Sp, placement: u8, set: impl FnOnce(&mut Self)) -> FloatBox<'a> {
        let ops = std::mem::take(&mut self.current_page.ops);
        let pool = std::mem::take(&mut self.current_page.text_pool);
        let (y, bottom, depth) = (self.y, self.margin_bottom, self.prev_depth);
        // The box is set apart from the page: its lines are no page material.
        let builder = std::mem::replace(&mut self.page_builder, crate::layout::layout_pagebuilder::PageBuilder::new());
        self.y = top_y;
        self.margin_bottom = BOX_BOTTOM;
        set(self);
        let height = (-self.y).to_pt();
        self.margin_bottom = bottom;
        self.y = y;
        self.prev_depth = depth;
        self.page_builder = builder;
        FloatBox {
            ops: std::mem::replace(&mut self.current_page.ops, ops),
            pool: std::mem::replace(&mut self.current_page.text_pool, pool),
            height,
            depth: std::mem::take(&mut self.box_depth),
            placement,
        }
    }

    #[inline]
    fn text_height(&self) -> f32 {
        (self.text_top() - self.margin_bottom).to_pt()
    }

    /// Whether `f` may join the top floats of the current page (`\@addtotoporbot`).
    fn fits_top(&self, f: &FloatBox<'a>) -> bool {
        if f.placement & FLOAT_TOP == 0 || self.floats.top_left == 0 {
            return false;
        }
        let sep = if self.floats.area_h > 0.0 { FLOAT_SEP } else { 0.0 };
        let area = self.floats.area_h + sep + f.height;
        f.placement & FLOAT_FORCE != 0 || area <= TOP_FRACTION * self.text_height()
    }

    /// Adds `f` below the top floats of the current page; the text on the page moves down.
    fn place_top(&mut self, f: FloatBox<'a>) {
        let old_start = self.page_start_y();
        let sep = if self.floats.area_h > 0.0 { FLOAT_SEP } else { 0.0 };
        let top = self.text_top() - self.floats.area_h - sep;
        append_ops(&mut self.floats.page_ops, &mut self.floats.page_pool, &f.ops, &f.pool, top);
        if self.floats.area_h == 0.0 {
            // \@makecol sets floats, \textfloatsep and text to \@colht, so the
            // separator's plus 2pt minus 4pt joins the page's glue.
            self.page_top_glue(TEXT_FLOAT_SEP_STRETCH, TEXT_FLOAT_SEP_SHRINK);
        }
        self.floats.area_h += sep + f.height;
        self.floats.top_left -= 1;
        let dy = old_start - self.page_start_y();
        if dy != Sp(0) {
            for op in &mut self.current_page.ops {
                op.shift_y(-dy);
            }
            self.y -= dy;
        }
    }

    /// Places a float ended in vertical mode, then leaves what `\end@float` leaves in the
    /// list once `\@specialoutput` has put the page back: `\addpenalty\interlinepenalty`
    /// (`\nobreak` after a heading) and `\penalty10000`, so `\lastskip` is zero for what
    /// follows.
    pub(crate) fn add_vmode_float(&mut self, f: FloatBox<'a>) {
        self.add_float(f);
        if !self.after_heading {
            self.page_addpenalty(0);
        }
        self.zero_lastskip();
    }

    /// Places a finished float (`\end{table}`): here, at the top of this page, or later.
    pub(crate) fn add_float(&mut self, f: FloatBox<'a>) {
        if !self.floats.deferred.is_empty() {
            self.floats.deferred.push_back(f);
            return;
        }
        let at_top = self.at_page_top();
        // Bottom of the material so far: the last baseline and its depth.
        let last_bottom = self.y + self.base_leading - self.prev_depth;
        if f.placement & FLOAT_HERE != 0 {
            // `\@addtocurcol`: `\vskip\intextsep \box\@currbox \vskip\intextsep`, appended by
            // the output routine to a list whose `\prevdepth` is ignored, so no interline
            // glue; at the top of a page the glue above it is discarded. The main list's
            // `\prevdepth` stays that of the line before the float.
            let box_top = if at_top { self.page_start_y() + self.top_skip } else { last_bottom - IN_TEXT_SEP };
            if box_top - f.height >= self.margin_bottom - 0.01 || at_top && f.placement & FLOAT_FORCE != 0 {
                append_ops(&mut self.current_page.ops, &mut self.current_page.text_pool, &f.ops, &f.pool, box_top);
                self.y = box_top - f.height - IN_TEXT_SEP - self.base_leading + self.prev_depth;
                // The `\intextsep` glue is the last skip, not one before the float.
                self.zero_lastskip();
                return;
            }
        }
        if self.fits_top(&f) {
            let sep = if self.floats.area_h > 0.0 { FLOAT_SEP } else { TEXT_FLOAT_SEP };
            if at_top || last_bottom - (sep + f.height) >= self.margin_bottom - 0.01 {
                self.place_top(f);
                return;
            }
        }
        self.floats.deferred.push_back(f);
    }

    /// Fills the top of a fresh page from the deferred list (`\@startcolumn`); a float too
    /// tall for the top area takes the page alone.
    pub(crate) fn start_page_floats(&mut self) {
        self.floats.area_h = 0.0;
        self.floats.top_left = TOP_NUMBER;
        if self.try_float_page() {
            return;
        }
        while let Some(f) = self.floats.deferred.front() {
            if self.fits_top(f) {
                let f = self.floats.deferred.pop_front().unwrap();
                self.place_top(f);
            } else if self.floats.area_h == 0.0 {
                // A float page: the float at the top, no text.
                let f = self.floats.deferred.pop_front().unwrap();
                let top = self.text_top();
                append_ops(&mut self.floats.page_ops, &mut self.floats.page_pool, &f.ops, &f.pool, top);
                self.floats.area_h = f.height;
                self.new_page();
                return;
            } else {
                break;
            }
        }
    }

    /// `\@tryfcolumn`: the first deferred float allowed on a float page (`p`) collects the
    /// later ones that fit with it, `\@fpsep` apart; if together they take more than
    /// `\floatpagefraction` of the column they make the page, spread by the fil glue. A float
    /// that fails holds back the later floats of its type. The rest stay deferred, in order.
    fn try_float_page(&mut self) -> bool {
        let colht = self.text_height();
        let kind_bit = |f: &FloatBox<'a>| if f.placement & FLOAT_TABLE_KIND != 0 { 2u8 } else { 1u8 };
        let blocked = |f: &FloatBox<'a>, failed: u8| failed & kind_bit(f) != 0 || f.placement & FLOAT_PAGE == 0;
        let deferred = &self.floats.deferred;
        let mut failed = 0u8;
        let mut chosen: Vec<usize> = Vec::new();
        for i in 0..deferred.len() {
            let f = &deferred[i];
            if blocked(f, failed) || f.height > colht {
                failed |= kind_bit(f);
                continue;
            }
            chosen.clear();
            chosen.push(i);
            let (mut h, mut fail) = (f.height, failed);
            for (j, g) in deferred.iter().enumerate().skip(i + 1) {
                if blocked(g, fail) || h + g.height + FP_SEP > colht {
                    fail |= kind_bit(g);
                } else {
                    chosen.push(j);
                    h += g.height + FP_SEP;
                }
            }
            if h > FLOAT_PAGE_FRACTION * colht {
                // Each fil of `\@fptop`, `\@fpsep` and `\@fpbot` takes the excess over 2n fils,
                // rounded to sp as `vlist_out` sets glue (§625).
                use crate::backend::scaled::{sp_pt, to_sp};
                let excess = (to_sp(colht) - to_sp(h)) as f64;
                let fil = sp_pt((excess / (2 * chosen.len()) as f64).round() as i32);
                let mut top = self.text_top() - fil;
                let mut rest = std::collections::VecDeque::with_capacity(deferred.len() - chosen.len());
                let mut next = chosen.iter().peekable();
                for (k, f) in std::mem::take(&mut self.floats.deferred).into_iter().enumerate() {
                    if next.peek() == Some(&&k) {
                        next.next();
                        append_ops(&mut self.floats.page_ops, &mut self.floats.page_pool, &f.ops, &f.pool, top);
                        top -= f.height + FP_SEP + 2.0 * fil;
                    } else {
                        rest.push_back(f);
                    }
                }
                self.floats.deferred = rest;
                self.floats.area_h = colht;
                self.new_page();
                return true;
            }
            failed |= kind_bit(f);
        }
        false
    }

    /// Moves the current page's top floats into it, before the page is completed.
    pub(crate) fn merge_page_floats(&mut self) {
        let ops = std::mem::take(&mut self.floats.page_ops);
        let pool = std::mem::take(&mut self.floats.page_pool);
        if !ops.is_empty() {
            append_ops(&mut self.current_page.ops, &mut self.current_page.text_pool, &ops, &pool, Sp(0));
        }
    }

    /// Sets the footnotes in `inls` into boxes before the paragraph is broken into lines.
    pub(crate) fn set_footnotes<'doc>(&mut self, inls: &'doc [Inline<'a>]) {
        for inl in inls {
            match inl {
                Inline::Footnote(f) => {
                    let b = self.capture_float(Sp(0), 0, |l| l.layout_footnote_body(f));
                    self.floats.pending_foot.push((&f.mark as *const _ as usize, b));
                }
                Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x) => self.set_footnotes(x),
                _ => {}
            }
        }
    }

    /// article.cls `\@thanks`: the title's footnotes, inserted after the title's boxes.
    pub(crate) fn insert_thanks<'doc>(&mut self, inls: &'doc [Inline<'a>]) {
        for inl in inls {
            match inl {
                Inline::Footnote(f) => {
                    let b = self.capture_float(Sp(0), 0, |l| l.layout_footnote_body(f));
                    let seq = self.page_builder_seq();
                    self.insert_footnote(seq, b);
                }
                Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x) => self.insert_thanks(x),
                _ => {}
            }
        }
    }

    /// Inserts the pending footnote whose mark is `mark` (the line holding it was just
    /// added to the page).
    pub(crate) fn insert_footnote_of(&mut self, mark: usize) {
        if let Some(k) = self.floats.pending_foot.iter().position(|(m, _)| *m == mark) {
            let (_, b) = self.floats.pending_foot.swap_remove(k);
            let seq = self.page_builder_seq();
            self.insert_footnote(seq, b);
        }
    }

    /// TeX's insertion into `\footins` (tex.web §1008): the first one on a page takes
    /// `\skip\footins` and every one its height off the page goal.
    fn insert_footnote(&mut self, seq: u32, b: FloatBox<'a>) {
        if self.margin_bottom <= BOX_BOTTOM {
            // A footnote inside a float or another box: not supported, dropped.
            return;
        }
        let (w, stretch, shrink) = footins_skip(self.class_font_size);
        let first = self.floats.foot.is_empty();
        self.add_insertion(b.height + if first { w } else { 0.0 }, if first { (stretch, shrink) } else { (0.0, 0.0) });
        self.floats.foot.push((seq, b));
    }

    /// Depth of the last footnote that stays on a page broken after `best_seq` boxes.
    pub(crate) fn last_footnote_depth(&self, best_seq: u32) -> Option<f32> {
        self.floats.foot.iter().rev().find(|(s, _)| *s <= best_seq).map(|(_, b)| b.depth)
    }

    /// At a page break after `best_seq` boxes: the footnotes of the boxes after it go to the
    /// next page. `top` is where the footnotes start on this page.
    pub(crate) fn split_footnotes(&mut self, best_seq: u32, top: Option<Sp>) -> Vec<(u32, FloatBox<'a>)> {
        let moved: Vec<(u32, FloatBox<'a>)> = {
            let (stay, go): (Vec<_>, Vec<_>) = std::mem::take(&mut self.floats.foot).into_iter().partition(|(s, _)| *s <= best_seq);
            self.floats.foot = stay;
            go
        };
        self.floats.foot_top = top;
        moved.into_iter().map(|(s, b)| (s - best_seq, b)).collect()
    }

    #[inline(always)]
    pub(crate) fn pending_foot_is_empty(&self) -> bool {
        self.floats.pending_foot.is_empty()
    }
}

impl<'a> Layouter<'a> {
    /// Puts footnotes moved from the previous page on this one.
    pub(crate) fn reinsert_footnotes(&mut self, moved: Vec<(u32, FloatBox<'a>)>) {
        for (seq, b) in moved {
            self.insert_footnote(seq, b);
        }
    }

    /// `\skip\footins` of the footnotes on this page (0 without any) and its shrink.
    pub(crate) fn footins_glue(&self) -> (f32, f32) {
        if self.floats.foot.is_empty() {
            (0.0, 0.0)
        } else {
            let (w, _, shrink) = footins_skip(self.class_font_size);
            (w, shrink)
        }
    }

    /// `\@makecol` with footnotes: `\vskip\skip\footins`, `\footnoterule` (`\kern-3pt
    /// \hrule width .4\columnwidth \kern2.6pt`) and the footnotes, below the page's text.
    pub(crate) fn place_footnotes(&mut self) {
        if self.floats.foot.is_empty() {
            self.floats.foot_top = None;
            return;
        }
        let foot = std::mem::take(&mut self.floats.foot);
        // A page ended by `\newpage` or `\clearpage` has `\vfil` above its break: the
        // footnotes go to the bottom of the text block, the last one's baseline on it.
        let top = match self.floats.foot_top.take() {
            Some(t) => t,
            None => {
                let total: f32 = foot.iter().map(|(_, b)| b.height).sum();
                Sp::pt(self.margin_bottom) + total - foot.last().map_or(0.0, |(_, b)| b.depth)
            }
        };
        if self.is_dry_run {
            return;
        }
        let (x0, x1) = self.text_span;
        self.current_page.ops.push(Paint::Rule { x: x0.into(), y: (top + 3.0 - 0.4), w: (0.4 * (x1 - x0)).into(), h: crate::layout::layout_engine::Sp::pt(0.4), fill: true });
        let mut y = top;
        for (_, b) in &foot {
            append_ops(&mut self.current_page.ops, &mut self.current_page.text_pool, &b.ops, &b.pool, y);
            y -= b.height;
        }
    }

    /// `\clearpage` at the end of the document: every deferred float is output.
    pub(crate) fn flush_floats(&mut self) {
        while !self.floats.deferred.is_empty() {
            self.new_page();
        }
    }
}
