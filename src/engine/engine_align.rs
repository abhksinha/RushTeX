//! Alignment: tex.web §768–§812 (`\halign`, its preamble, rows, columns and spans) and
//! §1126–§1133 (the commands that drive it), in our own form.
//!
//! TeX keeps the preamble as a linked list of glue nodes and "alignrecords", with span nodes
//! hanging off each record. Here an alignment is a [`Align`]: its columns in a vector (the
//! templates are spans of the engine's token arena, shared when the preamble repeats), the
//! tabskip glue between them in another, and each column's spanned widths as a short sorted
//! vector. A finished entry is an ordinary box node marked unset, whose stretch and shrink
//! wait in the alignment's side table until `fin_align` sets them. The brace count that ends
//! an entry (`align_state`) lives in the input stack, which counts the braces it reads.

use super::engine_catcode::Cat;
use super::engine_eqtb::{Glue, Reg, Region};
use super::engine_node::{BoxRec, Kind, Node, Sign, RUNNING};
use super::engine_pack::order;
use super::engine_prim::{Meaning, Prim};
use super::engine_run::{Machine, Mode, X};
use super::engine_token::{Span, Token};
use super::END_TEMPLATE_ID;

/// §269 group codes.
pub(super) const ALIGN_GROUP: u8 = 6;
pub(super) const NO_ALIGN_GROUP: u8 = 7;

/// §780: what ended an entry: an alignment character (its code), `\span`, `\cr`, `\crcr`.
pub(super) const SPAN_CODE: u32 = 0x20_0000;
const CR_CODE: u32 = SPAN_CODE + 1;
const CR_CR_CODE: u32 = SPAN_CODE + 2;
/// §770: the align state outside any entry, and while a preamble is read.
pub(super) const ALIGN_IDLE: i32 = 1_000_000;
const ALIGN_PREAMBLE: i32 = -1_000_000;
/// §138 `null_flag`: a column no entry has measured.
const NULL_FLAG: i32 = -0x4000_0000;
/// Box node subtypes for the unset boxes of §159: an entry (`b` indexes `Align::unsets`)
/// and a row.
pub(super) const UNSET_ENTRY: u8 = 1;
pub(super) const UNSET_ROW: u8 = 2;
/// §224 `tab_skip_code + 1`, the glue subtype of tabskip glue.
const TAB_SKIP_SUB: u8 = 12;

/// §769: a column of the preamble.
#[derive(Clone, Debug)]
struct Column {
    u: Span,
    v: Span,
    width: i32,
    /// §769 `extra_info`: whether the current entry began with `\omit`, then the code of
    /// what ended it.
    omit: bool,
    ended_by: u32,
    /// §798 span nodes: (columns spanned beyond this one, the widest such entry), by count.
    spans: Vec<(u16, i32)>,
}

/// §159: the stretch and shrink of an unset entry, and how many columns it spans beyond
/// its own.
#[derive(Clone, Copy, Debug)]
struct Unset {
    span: u16,
    stretch: i32,
    stretch_order: u8,
    shrink: i32,
    shrink_order: u8,
}

/// §770: one alignment in progress.
#[derive(Debug)]
pub(super) struct Align {
    cols: Vec<Column>,
    /// The tabskip glue before column 0, between the columns, and after the last.
    tabskips: Vec<Glue>,
    /// §770 `cur_align`, `cur_span`, `cur_loop` (the column a periodic preamble copies next).
    cur: usize,
    span_start: usize,
    cur_loop: Option<usize>,
    unsets: Vec<Unset>,
    /// The `to`/`spread` of `\halign`.
    exact: bool,
    size: i32,
    /// §772: the align state of the alignment around this one.
    saved_state: i32,
    /// §770 `cur_head..cur_tail`: what the current row's entries gave up to `hpack`.
    adjusts: Vec<Node>,
}

impl Align {
    /// The pending adjustment nodes, for `engine_gc`.
    #[cfg(all(test, feature = "oracle"))]
    pub(super) fn for_each_node(&mut self, f: &mut dyn FnMut(&mut Node)) {
        for n in &mut self.adjusts {
            f(n);
        }
    }

    /// The templates and the pending adjustment nodes' token spans, for `engine_gc`.
    pub(super) fn for_each_span(&mut self, f: &mut dyn FnMut(&mut Span)) {
        for c in &mut self.cols {
            f(&mut c.u);
            f(&mut c.v);
        }
        for n in &mut self.adjusts {
            super::engine_gc::node_span(n, f);
        }
    }
}

impl Machine<'_, '_> {
    /// §780: the code of `t` when it ends an entry (`tab_mark` or `car_ret`).
    pub(super) fn align_code(&self, t: Token) -> Option<u32> {
        if t.cs_id().is_none() {
            return (t.cat() == Some(Cat::AlignTab)).then_some(t.chr() as u32);
        }
        match self.meaning(t) {
            Meaning::Prim(Prim::Span) => Some(SPAN_CODE),
            Meaning::Prim(Prim::Cr) => Some(CR_CODE),
            Meaning::Prim(Prim::Crcr) => Some(CR_CR_CODE),
            Meaning::Char(c) if c.cat() == Some(Cat::AlignTab) => Some(c.chr() as u32),
            _ => None,
        }
    }

    /// §789 "Insert the ⟨v_j⟩ template": an entry ended with `code` at its own brace level.
    pub(super) fn insert_v_template(&mut self, code: u32) {
        let Some(a) = self.aligns.last_mut().filter(|_| !self.aligning) else {
            self.error("(interwoven alignment preambles are not allowed)");
            self.halted = true;
            return;
        };
        let col = &mut a.cols[a.cur];
        col.ended_by = code;
        if col.omit {
            // §790 `omit_template`: just `\endtemplate`.
            self.input.back(Token::cs(END_TEMPLATE_ID));
        } else {
            self.input.push_template(col.v, false);
        }
        self.input.align_state = ALIGN_IDLE;
    }

    /// §774 `init_align` for `\halign` in a vertical list.
    pub(super) fn init_align(&mut self) {
        let saved_state = self.input.align_state;
        self.input.align_state = ALIGN_PREAMBLE;
        let prev_depth = self.align_prev_depth();
        self.push_nest(Mode::Vertical, true, prev_depth);
        // §645 `scan_spec`, then the preamble inside the group it opens.
        let (exact, size) = if self.scan_keyword("to") {
            (true, self.scan_dimen(false).0)
        } else if self.scan_keyword("spread") {
            (false, self.scan_dimen(false).0)
        } else {
            (false, 0)
        };
        self.begin_group(ALIGN_GROUP);
        self.scan_left_brace();
        self.aligns.push(Align { cols: Vec::new(), tabskips: Vec::new(), cur: 0, span_start: 0, cur_loop: None, unsets: Vec::new(), exact, size, saved_state, adjusts: Vec::new() });
        self.scan_preamble();
        self.begin_group(ALIGN_GROUP);
        self.insert_toks_param("everycr");
        self.align_peek();
    }

    /// §774: in a display, only at its start; §775: the alignment's own list, internal
    /// vertical; `\prevdepth` carries in, from the list around a display.
    fn align_prev_depth(&mut self) -> i32 {
        if self.mode != Mode::Math {
            return self.aux;
        }
        if self.cur.len() > self.start as usize || self.aux >= 0 {
            self.error("Improper \\halign inside $$'s");
            self.cur.truncate(self.start as usize);
            self.aux = -1;
        }
        self.nest.last().map_or(0, |l| l.aux)
    }

    /// amsmath's `\halign{&...&...\cr` in a display (`\align@`), its two templates built
    /// by a native: a periodic preamble of columns `u[0]`/`v[0]` and `u[1]`/`v[1]`, all
    /// tabskip glue zero until [`Self::align_retab`] sets it.
    pub(super) fn init_align_periodic(&mut self, u: [Span; 2], v: [Span; 2]) {
        let saved_state = self.input.align_state;
        let prev_depth = self.align_prev_depth();
        self.push_nest(Mode::Vertical, true, prev_depth);
        self.begin_group(ALIGN_GROUP);
        let cols = (0..2).map(|k| Column { u: u[k], v: v[k], width: NULL_FLAG, omit: false, ended_by: 0, spans: Vec::new() }).collect();
        self.aligns.push(Align { cols, tabskips: vec![Glue::default(); 3], cur: 0, span_start: 0, cur_loop: Some(0), unsets: Vec::new(), exact: false, size: 0, saved_state, adjusts: Vec::new() });
        self.begin_group(ALIGN_GROUP);
        self.align_peek();
    }

    /// The natural widths of the innermost alignment's first `n` columns (0 for one no
    /// entry reached).
    pub(super) fn align_widths(&self, n: usize) -> Vec<i32> {
        let a = self.aligns.last();
        (0..n).map(|k| a.and_then(|a| a.cols.get(k)).map_or(0, |c| if c.width == NULL_FLAG { 0 } else { c.width })).collect()
    }

    /// The tabskip glue of the innermost alignment, and of every row built so far, set to
    /// `skip(k)` for the glue before column `k`; then each row's entry in column `col`
    /// rebuilt by `entry(row, old list)` and the column's width taken again. For an
    /// alignment whose glue and last entries depend on all its rows (amsmath's `align`,
    /// whose measuring pass this replaces).
    pub(super) fn align_retab<F>(&mut self, skip: &dyn Fn(usize) -> Glue, col: usize, mut entry: F)
    where
        F: FnMut(&mut Self, usize, Vec<Node>) -> Vec<Node>,
    {
        let Some(a) = self.aligns.last_mut() else { return };
        for (k, g) in a.tabskips.iter_mut().enumerate() {
            *g = skip(k);
        }
        let mut width = NULL_FLAG;
        let mut r = 0;
        for i in self.start as usize..self.cur.len() {
            let n = self.cur[i];
            if !(n.kind == Kind::Box && n.sub == UNSET_ROW) {
                continue;
            }
            let row = self.e.nodes.boxes[n.a as usize];
            let list = self.e.nodes.get(row.list).to_vec();
            let (mut k, mut c) = (0, 0);
            for m in &list {
                match m.kind {
                    Kind::Glue if m.sub == TAB_SKIP_SUB => {
                        self.e.nodes.glues[m.a as usize] = skip(k);
                        k += 1;
                    }
                    Kind::Box if m.sub == UNSET_ENTRY => {
                        if c == col {
                            let b = self.e.nodes.boxes[m.a as usize];
                            let old = self.e.nodes.get(b.list).to_vec();
                            self.e.nodes.free_block(b.list);
                            let new = entry(self, r, old);
                            let (mut w, mut h, mut d) = (0, 0, 0);
                            for x in &new {
                                match x.kind {
                                    Kind::Kern => w += x.a,
                                    Kind::Box => {
                                        let y = self.e.nodes.boxes[x.a as usize];
                                        (w, h, d) = (w + y.width, h.max(y.height - y.shift), d.max(y.depth + y.shift));
                                    }
                                    _ => {}
                                }
                            }
                            let list = self.e.nodes.store(&new);
                            self.e.nodes.boxes[m.a as usize] = BoxRec { width: w, height: h, depth: d, list, ..b };
                            width = width.max(w);
                            let rb = &mut self.e.nodes.boxes[n.a as usize];
                            (rb.height, rb.depth) = (rb.height.max(h), rb.depth.max(d));
                        }
                        c += 1;
                    }
                    _ => {}
                }
            }
            r += 1;
        }
        if let Some(c) = self.aligns.last_mut().and_then(|a| a.cols.get_mut(col)) {
            c.width = width;
        }
    }

    /// `\ialign` (`\everycr{}\tabskip\z@skip\halign{`) with a preamble of `n` columns
    /// built by a native instead of scanned: every column's templates are `u` and `v`
    /// (which find their column by [`Self::align_column`]), all tabskip glue zero. In the
    /// internal vertical list the native has opened; `\everycr` is empty there (the
    /// pre-scan refuses it).
    pub(super) fn init_align_with(&mut self, n: usize, u: Span, v: Span) {
        let saved_state = self.input.align_state;
        let prev_depth = self.aux;
        self.push_nest(Mode::Vertical, true, prev_depth);
        self.begin_group(ALIGN_GROUP);
        let cols = vec![Column { u, v, width: NULL_FLAG, omit: false, ended_by: 0, spans: Vec::new() }; n];
        self.aligns.push(Align { cols, tabskips: vec![Glue::default(); n + 1], cur: 0, span_start: 0, cur_loop: None, unsets: Vec::new(), exact: false, size: 0, saved_state, adjusts: Vec::new() });
        self.begin_group(ALIGN_GROUP);
        self.align_peek();
    }

    /// The column of the entry being read (§770 `cur_align`).
    pub(super) fn align_column(&self) -> usize {
        self.aligns.last().map_or(0, |a| a.cur)
    }

    /// §777: the preamble, to its `\cr`: each column's ⟨u_j⟩ and ⟨v_j⟩ templates, with
    /// the tabskip glue in force before each.
    fn scan_preamble(&mut self) {
        self.aligning = true;
        self.input.align_state = ALIGN_PREAMBLE;
        let tabskip = Reg { region: Region::Glue, slot: TAB_SKIP };
        let mut list = Vec::new();
        let mut ended = false;
        'scan: loop {
            let g = self.e.eqtb.glue(tabskip);
            let a = self.aligns.last_mut().expect("in an alignment");
            a.tabskips.push(g);
            if ended {
                break;
            }
            // §783: ⟨u_j⟩ up to `#`; spaces before it are dropped, and an alignment
            // character before anything else makes the preamble periodic.
            list.clear();
            loop {
                let Some(t) = self.get_preamble_token() else { break 'scan };
                if self.is_param(t) {
                    break;
                }
                if self.input.align_state == ALIGN_PREAMBLE {
                    if let Some(code) = self.align_code(t) {
                        let a = self.aligns.last_mut().expect("in an alignment");
                        if list.is_empty() && a.cur_loop.is_none() && code < SPAN_CODE {
                            a.cur_loop = Some(a.cols.len());
                            continue;
                        }
                        self.error("Missing # inserted in alignment preamble");
                        self.input.back(t);
                        break;
                    }
                }
                if !list.is_empty() || !self.is_spacer(t) {
                    list.push(t);
                }
            }
            let u = self.e.toks.store(&list);
            // §784: ⟨v_j⟩ up to the next alignment character or `\cr`, then `\endtemplate`.
            list.clear();
            let code = loop {
                let Some(t) = self.get_preamble_token() else { break 'scan };
                if self.input.align_state == ALIGN_PREAMBLE {
                    if let Some(code) = self.align_code(t) {
                        break code;
                    }
                }
                if self.is_param(t) {
                    self.error("Only one # is allowed per tab");
                    continue;
                }
                list.push(t);
            };
            list.push(Token::cs(END_TEMPLATE_ID));
            let v = self.e.toks.store(&list);
            let a = self.aligns.last_mut().expect("in an alignment");
            a.cols.push(Column { u, v, width: NULL_FLAG, omit: false, ended_by: 0, spans: Vec::new() });
            ended = code >= CR_CODE;
        }
        self.aligning = false;
    }

    /// §782 `get_preamble_token`: `\span` expands the token after it; `\tabskip`
    /// assignments take effect at once and are not part of a template.
    fn get_preamble_token(&mut self) -> Option<Token> {
        loop {
            let mut t = self.get_token()?;
            while self.meaning(t) == Meaning::Prim(Prim::Span) {
                t = self.get_token()?;
                match self.meaning_read(t) {
                    Meaning::Macro(mac) => {
                        self.macro_call(t, mac);
                        t = self.get_token()?;
                    }
                    Meaning::Prim(p) if p.expandable() => {
                        self.expand(p, t);
                        t = self.get_token()?;
                    }
                    Meaning::Native(i) if super::engine_native::native_expands(i) => {
                        self.expand_native(i);
                        t = self.get_token()?;
                    }
                    _ => {}
                }
            }
            if self.meaning(t) == Meaning::Reg(Reg { region: Region::Glue, slot: TAB_SKIP }) {
                self.scan_optional_equals();
                let g = self.scan_glue(false);
                let global = self.e.eqtb.word(Reg { region: Region::Int, slot: GLOBAL_DEFS }) > 0;
                self.e.eqtb.set_glue(Reg { region: Region::Glue, slot: TAB_SKIP }, g, global);
                continue;
            }
            return Some(t);
        }
    }

    fn is_param(&self, t: Token) -> bool {
        match t.cs_id() {
            None => t.cat() == Some(Cat::Param),
            Some(_) => matches!(self.meaning(t), Meaning::Char(c) if c.cat() == Some(Cat::Param)),
        }
    }

    fn is_spacer(&self, t: Token) -> bool {
        match t.cs_id() {
            None => t.cat() == Some(Cat::Space),
            Some(_) => matches!(self.meaning(t), Meaning::Char(c) if c.cat() == Some(Cat::Space)),
        }
    }

    /// The first non-blank token of an entry, looked at for `\omit` or `\noalign` (§785,
    /// §791): e-TeX expands no `\protected` macro here (`get_x_or_protected`), so one
    /// such as LaTeX's `\{` runs inside the entry's template, in its mode.
    fn next_entry_token(&mut self) -> Option<X> {
        loop {
            let x = self.get_x_token(true)?;
            if !super::engine_value::blank(x) {
                return Some(x);
            }
        }
    }

    /// §785 `align_peek`: what follows a `\cr` (or the preamble): `\noalign`, the
    /// alignment's `}`, an ignored `\crcr`, or a new row.
    fn align_peek(&mut self) {
        loop {
            self.input.align_state = ALIGN_IDLE;
            let Some(x) = self.next_entry_token() else { return };
            match x.m {
                Meaning::Prim(Prim::NoAlign) => {
                    self.scan_left_brace();
                    self.begin_group(NO_ALIGN_GROUP);
                    if self.mode == Mode::Vertical {
                        self.normal_paragraph();
                    }
                }
                Meaning::Char(c) if c.cat() == Some(Cat::EndGroup) => self.fin_align(),
                Meaning::Prim(Prim::Crcr) => continue,
                // A native's expansion here (`\end{tabular}`, `\hline`, a blank line).
                _ if self.doc.is_some() => match self.align_peek_native(x) {
                    Some(true) => continue,
                    Some(false) => {}
                    None => {
                        self.init_row();
                        self.init_col(x.t, x.m);
                    }
                },
                _ => {
                    self.init_row();
                    self.init_col(x.t, x.m);
                }
            }
            return;
        }
    }

    /// §786 `init_row`: a row, an internal horizontal list starting with the first tabskip.
    fn init_row(&mut self) {
        self.push_nest(Mode::Horizontal, true, 0);
        let g = self.aligns.last().expect("in an alignment").tabskips[0];
        let n = self.e.nodes.glue_node(g, TAB_SKIP_SUB);
        self.cur.push(n);
        let a = self.aligns.last_mut().expect("in an alignment");
        a.cur = 0;
        a.adjusts.clear();
        self.init_span(0);
    }

    /// §787 `init_span`: an entry starting at column `p`.
    fn init_span(&mut self, p: usize) {
        self.push_nest(Mode::Horizontal, true, 1000);
        self.aligns.last_mut().expect("in an alignment").span_start = p;
    }

    /// §788 `init_col`: the column's ⟨u_j⟩ template comes before the token `t` just peeked,
    /// unless that is `\omit`.
    fn init_col(&mut self, t: Token, m: Meaning) {
        // `\multicolumn` is `\multispan`'s `\omit`, then a native's spans and material.
        let multi = self.doc.is_some() && self.is_multicolumn(m);
        let a = self.aligns.last_mut().expect("in an alignment");
        let col = &mut a.cols[a.cur];
        col.omit = multi || m == Meaning::Prim(Prim::Omit);
        if multi {
            self.input.align_state = 0;
            return self.multicolumn();
        }
        if col.omit {
            self.input.align_state = 0;
        } else {
            let u = col.u;
            self.input.back(t);
            self.input.push_template(u, true);
        }
    }

    /// §791 `fin_col`: an entry ended (`\endtemplate` was reached); `true` when it ended
    /// the row.
    fn fin_col(&mut self) -> bool {
        let a = self.aligns.last_mut().expect("in an alignment");
        let cur = a.cur;
        // §792: past the preamble's end: a periodic preamble grows, else `\cr` is assumed.
        if cur + 1 == a.cols.len() && a.cols[cur].ended_by < CR_CODE {
            if let Some(k) = a.cur_loop {
                let col = Column { width: NULL_FLAG, omit: false, ended_by: 0, spans: Vec::new(), ..a.cols[k].clone() };
                a.cols.push(col);
                let g = a.tabskips[k + 1];
                a.tabskips.push(g);
                a.cur_loop = Some(k + 1);
            } else {
                a.cols[cur].ended_by = CR_CODE;
                self.error("Extra alignment tab has been changed to \\cr");
            }
        }
        let a = self.aligns.last().expect("in an alignment");
        let ended_by = a.cols[cur].ended_by;
        if ended_by != SPAN_CODE {
            self.end_group(ALIGN_GROUP);
            self.begin_group(ALIGN_GROUP);
            self.package_unset();
            // §795: the tabskip glue after the column.
            let g = self.aligns.last().expect("in an alignment").tabskips[cur + 1];
            let n = self.e.nodes.glue_node(g, TAB_SKIP_SUB);
            self.cur.push(n);
            if ended_by >= CR_CODE {
                return true;
            }
            self.init_span(cur + 1);
        }
        self.input.align_state = ALIGN_IDLE;
        let Some(x) = self.next_entry_token() else { return false };
        self.aligns.last_mut().expect("in an alignment").cur = cur + 1;
        self.init_col(x.t, x.m);
        false
    }

    /// §796: the entry becomes an unset box of its natural width; the widest entry of each
    /// column (or run of spanned columns) is remembered.
    fn package_unset(&mut self) {
        let start = self.start as usize;
        let (mut stretch, mut shrink) = ([0i32; 4], [0i32; 4]);
        for n in &self.cur[start..] {
            if n.kind == Kind::Glue {
                let g = self.e.nodes.glue(n);
                stretch[g.stretch_order as usize] += g.stretch;
                shrink[g.shrink_order as usize] += g.shrink;
            }
        }
        // §796: insertions, marks and `\vadjust` material leave the entry.
        self.adjust = Some(Vec::new());
        let b = self.hpack(start, 0, false);
        if let Some(mut adj) = self.adjust.take() {
            self.aligns.last_mut().expect("in an alignment").adjusts.append(&mut adj);
        }
        let w = b.width;
        let a = self.aligns.last_mut().expect("in an alignment");
        let (cur, first) = (a.cur, a.span_start);
        let span = (cur - first) as u16;
        if span > 0 {
            // §798: the widest entry spanning these columns.
            let spans = &mut a.cols[first].spans;
            match spans.binary_search_by_key(&span, |s| s.0) {
                Ok(i) => spans[i].1 = spans[i].1.max(w),
                Err(i) => spans.insert(i, (span, w)),
            }
        } else if w > a.cols[cur].width {
            a.cols[cur].width = w;
        }
        let (so, ho) = (order(&stretch), order(&shrink));
        a.unsets.push(Unset { span, stretch: stretch[so as usize], stretch_order: so, shrink: shrink[ho as usize], shrink_order: ho });
        let u = a.unsets.len() as i32 - 1;
        self.pop_nest();
        let idx = self.e.nodes.add_box(b);
        self.cur.push(Node { kind: Kind::Box, sub: UNSET_ENTRY, font: 0, a: idx as i32, b: u });
    }

    /// §799 `fin_row`: the row is packed at its natural width and goes on the alignment's
    /// list, still unset.
    fn fin_row(&mut self) {
        let start = self.start as usize;
        let b = self.hpack(start, 0, false);
        self.pop_nest();
        let idx = self.e.nodes.add_box(b);
        self.append_to_vlist(idx);
        if let Some(n) = self.cur.last_mut() {
            n.sub = UNSET_ROW;
        }
        let adj = std::mem::take(&mut self.aligns.last_mut().expect("in an alignment").adjusts);
        self.cur.extend_from_slice(&adj);
        self.insert_toks_param("everycr");
        self.align_peek();
    }

    /// §800 `fin_align`: the column widths are settled, the tabskip glue is set by packing
    /// a prototype row, and every row and entry is set to match.
    pub(super) fn fin_align(&mut self) {
        self.end_group(ALIGN_GROUP);
        self.end_group(ALIGN_GROUP);
        let Some(mut a) = self.aligns.pop() else { return };
        // §800: the rows of an alignment in a display are shifted by `\displayindent`.
        let o = match self.nest.last() {
            Some(l) if l.mode == Mode::Math && !l.inner => self.dimen_param(super::engine_noad::DISPLAY_INDENT),
            _ => 0,
        };
        // §801: an unused column has no width and no tabskip after it; §803: the widths of
        // spanned entries move on to the next column, less this column and its tabskip.
        for k in 0..a.cols.len() {
            if a.cols[k].width == NULL_FLAG {
                a.cols[k].width = 0;
                a.tabskips[k + 1] = Glue::default();
            }
            let spans = std::mem::take(&mut a.cols[k].spans);
            let t = a.cols[k].width + a.tabskips[k + 1].width;
            for (n, w) in spans {
                let w = w - t;
                let next = &mut a.cols[k + 1];
                if n == 1 {
                    next.width = next.width.max(w);
                } else {
                    match next.spans.binary_search_by_key(&(n - 1), |s| s.0) {
                        Ok(i) => next.spans[i].1 = next.spans[i].1.max(w),
                        Err(i) => next.spans.insert(i, (n - 1, w)),
                    }
                }
            }
        }
        // §804: the prototype row, packed as the `\halign` asked.
        let start = self.cur.len();
        for (k, col) in a.cols.iter().enumerate() {
            let g = self.e.nodes.glue_node(a.tabskips[k], TAB_SKIP_SUB);
            self.cur.push(g);
            let b = self.e.nodes.add_box(BoxRec { width: col.width, ..BoxRec::default() });
            self.cur.push(Node::new(Kind::Box, b as i32));
        }
        let g = self.e.nodes.glue_node(a.tabskips[a.cols.len()], TAB_SKIP_SUB);
        self.cur.push(g);
        let p = self.hpack(start, a.size, a.exact);
        // §805: every row and rule of the alignment's list.
        for i in self.start as usize..self.cur.len() {
            let n = self.cur[i];
            match n.kind {
                Kind::Box if n.sub == UNSET_ROW => {
                    let row = self.e.nodes.boxes[n.a as usize];
                    let list = self.set_row(&a, &p, &row);
                    let b = BoxRec { width: p.width, list, glue_set: p.glue_set, glue_sign: p.glue_sign, glue_order: p.glue_order, shift: o, ..row };
                    self.e.nodes.free_box(n.a as u32);
                    self.cur[i] = Node::new(Kind::Box, self.e.nodes.add_box(b) as i32);
                }
                Kind::Rule => {
                    // §806: running dimensions reach the alignment's edges.
                    let [w, h, d] = self.e.nodes.rules[n.a as usize];
                    if w == RUNNING || h == RUNNING || d == RUNNING {
                        let dims = [if w == RUNNING { p.width } else { w }, if h == RUNNING { p.height } else { h }, if d == RUNNING { p.depth } else { d }];
                        self.e.nodes.rules[n.a as usize] = dims;
                    }
                    // A shifted rule goes in a box of its natural size.
                    if o != 0 {
                        let [w, h, d] = self.e.nodes.rules[self.cur[i].a as usize];
                        let list = self.e.nodes.store(&[self.cur[i]]);
                        let b = self.e.nodes.add_box(BoxRec { width: w, height: h, depth: d, list, shift: o, ..BoxRec::default() });
                        self.cur[i] = Node::new(Kind::Box, b as i32);
                    }
                }
                _ => {}
            }
        }
        // §804: the prototype row has served.
        self.e.nodes.flush_list(p.list);
        self.input.align_state = a.saved_state;
        // §812: the list joins the enclosing one, keeping the alignment's `\prevdepth`.
        let aux = self.aux;
        self.pop_nest();
        if self.mode == Mode::Math {
            return self.finish_display_align(aux);
        }
        self.aux = aux;
    }

    /// §807–§810: a row's entries set to their columns' widths; a spanning entry is
    /// followed by tabskip glue and empty boxes for the columns it covers.
    fn set_row(&mut self, a: &Align, p: &BoxRec, row: &BoxRec) -> (u32, u32) {
        let old = self.e.nodes.get(row.list).to_vec();
        let start = self.cur.len();
        let mut col = 0;
        for n in old {
            if !(n.kind == Kind::Box && n.sub == UNSET_ENTRY) {
                self.cur.push(n);
                continue;
            }
            let un = a.unsets[n.b as usize];
            let w = a.cols[col].width;
            let mut t = w;
            let mut extra = Vec::new();
            for _ in 0..un.span {
                // §809: the tabskip glue as the prototype set it, then the next column.
                let v = a.tabskips[col + 1];
                extra.push(self.e.nodes.glue_node(v, TAB_SKIP_SUB));
                t += v.width;
                match p.glue_sign {
                    Sign::Stretching if v.stretch_order == p.glue_order => t += super::engine_ship::zround(p.glue_set * v.stretch as f64),
                    Sign::Shrinking if v.shrink_order == p.glue_order => t -= super::engine_ship::zround(p.glue_set * v.shrink as f64),
                    _ => {}
                }
                col += 1;
                let b = self.e.nodes.add_box(BoxRec { width: a.cols[col].width, ..BoxRec::default() });
                extra.push(Node::new(Kind::Box, b as i32));
                t += a.cols[col].width;
            }
            // §810: the entry is `w` wide, its glue set as if it were `t`.
            let mut b = BoxRec { height: row.height, depth: row.depth, shift: 0, ..self.e.nodes.boxes[n.a as usize] };
            let nat = b.width;
            (b.glue_sign, b.glue_order, b.glue_set) = if t == nat {
                (Sign::Normal, 0, 0.0)
            } else if t > nat {
                (Sign::Stretching, un.stretch_order, if un.stretch == 0 { 0.0 } else { (t - nat) as f64 / un.stretch as f64 })
            } else {
                let r = if un.shrink == 0 {
                    0.0
                } else if un.shrink_order == 0 && nat - t > un.shrink {
                    1.0
                } else {
                    (nat - t) as f64 / un.shrink as f64
                };
                (Sign::Shrinking, un.shrink_order, r)
            };
            b.width = w;
            // The unset entry's record gives way to the set one; its list moves.
            self.e.nodes.free_box(n.a as u32);
            let idx = self.e.nodes.add_box(b);
            self.cur.push(Node::new(Kind::Box, idx as i32));
            self.cur.extend_from_slice(&extra);
            col += 1;
        }
        let list = self.e.nodes.store(&self.cur[start..]);
        self.cur.truncate(start);
        // The unset row's nodes moved into the set one.
        self.e.nodes.free_block(row.list);
        list
    }

    /// §1131 `do_endv`: `\endtemplate` ends the entry.
    pub(super) fn do_endv(&mut self) {
        if self.groups.last() == Some(&ALIGN_GROUP) {
            self.end_graf();
            if self.fin_col() {
                self.fin_row();
            }
        } else {
            self.error("Extra }, or forgotten \\endtemplate");
        }
    }

    /// §1133: the `}` of `\noalign`.
    pub(super) fn end_no_align(&mut self) {
        self.end_graf();
        self.end_group(NO_ALIGN_GROUP);
        self.align_peek();
    }

    /// §1127 `align_error`: an alignment character or `\cr` outside an entry's level.
    pub(super) fn align_error(&mut self, t: Token) {
        if self.input.align_state.abs() > 2 {
            let name = self.name_of(t);
            self.error(format!("Misplaced {name}"));
            return;
        }
        self.input.back(t);
        let brace = if self.input.align_state < 0 {
            self.error("Missing { inserted");
            self.input.align_state += 1;
            Token::char(Cat::BeginGroup, '{')
        } else {
            self.error("Missing } inserted");
            self.input.align_state -= 1;
            Token::char(Cat::EndGroup, '}')
        };
        self.input.back(brace);
    }

    /// A token-list parameter (`\everycr`) read next.
    fn insert_toks_param(&mut self, name: &str) {
        if let Some(reg) = super::engine_names::param(name) {
            let span = self.e.eqtb.toks(reg);
            if !span.is_empty() {
                let list = self.e.toks.get(span).to_vec();
                self.back_list(&list);
            }
        }
    }
}

/// §224 `tab_skip_code`, §236 `global_defs_code`.
const TAB_SKIP: u32 = 11;
const GLOBAL_DEFS: u32 = 43;

#[cfg(test)]
mod tests {
    use super::super::engine_names::param;
    use super::*;

    #[test]
    fn slots_match_the_table() {
        assert_eq!(param("tabskip"), Some(Reg { region: Region::Glue, slot: TAB_SKIP }));
        assert_eq!(param("globaldefs"), Some(Reg { region: Region::Int, slot: GLOBAL_DEFS }));
    }
}
