//! multicol.sty on the GDL path: the `multicols` and `multicols*` environments at the
//! page's outer level, as multicol.sty v2.0b writes them (`\multicols`, `\mult@@cols`,
//! `\enough@room`, `\prepare@multicols`, `\set@mult@vsize`, `\endmulticols`,
//! `\endmulticols*`), and its output routines (`\multi@column@out`, `\speci@ls`,
//! `\balance@columns@out`, `\balance@columns`, `\page@sofar`, `\LR@column@boxes`,
//! `\remove@discardable@items`).
//!
//! multicol keeps its columns in box registers in pairs (`\mult@firstbox`, `\mult@gfirstbox`,
//! …) walked by `\process@cols`; here a page's columns are a short vector of box indexes,
//! built and set within one output routine. The page before the environment (`\partial@page`)
//! waits in a hidden box register, since it lives across commands. Not native yet (the
//! document runs on the engine): boxed mode (`multicols` in a box or a list), footnotes and
//! floats in the columns, `\columnbreak`, `\raggedcolumns`, marks in the columns, a balance
//! that fails (`\iftoo@bad`), and the optional heading argument.

use super::super::engine_eqtb::{Glue, Reg, Region};
use super::super::engine_node::{BoxRec, Kind, Node, IGNORE_DEPTH, RUNNING};
use super::super::engine_pack::BOX_MAX_DEPTH;
use super::super::engine_run::{Machine, Mode, SEMI_SIMPLE_GROUP, SIMPLE_GROUP};
use super::super::engine_value::MAX_DIMEN;
use super::{Op, DEFAULT_COLOR, FOOTINS, R_LINEWIDTH, R_TOTALLEFTMARGIN};

/// `\partial@page`, global in multicol: the page above the environment.
const PARTIAL_BOX: u32 = 32710;
/// A hidden register `\vsplit` works on (always void between uses).
const SPLIT_BOX: u32 = 32711;

/// `\premulticols` (50pt), `\postmulticols` (20pt), `\maxbalancingoverflow` (12pt),
/// `\multicolundershoot` (2pt); `\multicolovershoot` is 0pt.
const PREMULTICOLS: i32 = 50 * 65536;
const POSTMULTICOLS: i32 = 20 * 65536;
const MAX_BALANCING_OVERFLOW: i32 = 12 * 65536;
const UNDERSHOOT: i32 = 2 * 65536;
/// `\multicolsep`: `12pt plus 4pt minus 3pt`.
const MULTICOLSEP: Glue = Glue { width: 12 * 65536, stretch: 4 * 65536, shrink: 3 * 65536, stretch_order: 0, shrink_order: 0 };
/// `\c@columnbadness`, `\c@finalcolumnbadness`.
const COLUMN_BADNESS: i32 = 10000;
const FINAL_COLUMN_BADNESS: i32 = 9999;

/// Where the environment is: before `\output` is multicol's, collecting the page before it
/// (`\eject` under the `\partial@page` output), with columns, or after its `\endgroup`.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Phase {
    Before,
    Collect,
    Columns,
    Done,
}

/// One `multicols` environment.
#[derive(PartialEq, Clone)]
pub(super) struct Multicol {
    /// `\col@number`.
    n: i32,
    /// `multicols*`: `\balance@columns@out` is `\multi@column@out \penalty-\@Mvi`.
    star: bool,
    pub(super) phase: Phase,
    /// `\@colroom` inside the environment's group (`\advance\@colroom-\ht\partial@page`).
    colroom: i32,
    /// `\full@width`, `\multicol@leftmargin`.
    full_width: i32,
    leftmargin: i32,
    /// `multicols*`'s `\penalty-\@Mvi` waits for the end of `\multi@column@out`.
    star_penalty: bool,
}

impl Machine<'_, '_> {
    /// `\begin{multicols}{n}` (and `multicols*`, `star`), after `\begin`'s `\begingroup`.
    pub(super) fn multicols_begin(&mut self, star: bool) {
        if star {
            // `\begin{multicols}` inside `\multicols*`: its own `\begingroup`.
            self.begin_group(SEMI_SIMPLE_GROUP);
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let digits: String = arg.iter().map(|t| t.chr()).collect();
        let Some(n) = digits.trim().parse::<i32>().ok().filter(|n| (2..=20).contains(n)) else {
            return self.not_native("multicols: a column count other than 2 to 20");
        };
        if self.next_is('[') {
            return self.not_native("multicols: the optional heading");
        }
        if self.doc.as_ref().is_some_and(|d| d.multicol.is_some()) {
            return self.not_native("multicols in multicols");
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.multicol = Some(Box::new(Multicol { n, star, phase: Phase::Before, colroom: 0, full_width: 0, leftmargin: 0, star_penalty: false }));
        }
        let par = self.cur_par();
        self.run_ops(vec![par, Op::Call(|m| m.multicols_room())]);
    }

    fn mc(&self) -> &Multicol {
        self.doc.as_ref().and_then(|d| d.multicol.as_deref()).expect("a multicols environment")
    }

    fn mc_mut(&mut self) -> &mut Multicol {
        self.doc.as_mut().and_then(|d| d.multicol.as_deref_mut()).expect("a multicols environment")
    }

    /// `\mult@@cols` after its `\par`: `\ifinner` (boxed mode, not native), then
    /// `\enough@room\premulticols`.
    fn multicols_room(&mut self) {
        if !self.nest.is_empty() || self.mode != Mode::Vertical || self.rw(R_TOTALLEFTMARGIN) != 0 {
            return self.not_native("multicols in a box or a list");
        }
        let mut ops = self.enough_room();
        ops.push(Op::Call(|m| m.room_check(PREMULTICOLS, |m| m.multicols_start())));
        self.run_ops(ops);
    }

    /// `\enough@room`'s start (after its `\par`): `\bgroup\@nobreakfalse\addpenalty\z@
    /// \egroup` (`\@nobreakfalse` is global).
    fn enough_room(&mut self) -> Vec<Op> {
        if let Some(doc) = self.doc.as_mut() {
            doc.nobreak = false;
        }
        let mut ops = Vec::new();
        self.add_penalty(0, &mut ops);
        ops
    }

    /// `\page@free \pagegoal \advance\page@free -\pagetotal \ifdim \page@free <#1\newpage
    /// \fi`, then `then`.
    fn room_check(&mut self, need: i32, then: fn(&mut Machine)) {
        let free = self.page_dimen(0) - self.page_dimen(1);
        if free < need {
            return self.run_ops(vec![Op::Call(|m| m.new_page()), Op::Call(then)]);
        }
        then(self);
    }

    /// `#1\par\addvspace\multicolsep`, the depth correction, `\begingroup
    /// \prepare@multicols` up to its `\eject`.
    fn multicols_start(&mut self) {
        let par = self.cur_par();
        self.run_ops(vec![par, Op::Call(|m| m.multicols_prepare())]);
    }

    fn multicols_prepare(&mut self) {
        self.add_vspace(MULTICOLSEP);
        // `\ifdim\prevdepth=-\@m\p@ \else \@tempcnta\prevdepth \@tempcntb\baselineskip
        // \divide\@tempcnta\@tempcntb \advance\@tempcnta\@ne \dimen@\prevdepth
        // \advance\dimen@ -\@tempcnta\baselineskip \advance\dimen@\topskip \kern-\dimen@ \fi`.
        if self.aux != IGNORE_DEPTH {
            let b = self.e.eqtb.glue(param!("baselineskip")).width;
            if b == 0 {
                return self.not_native("multicols with \\baselineskip zero");
            }
            let count = self.aux / b + 1;
            let top = self.e.eqtb.glue(param!("topskip")).width;
            let d = self.aux - count.wrapping_mul(b) + top;
            self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, -d) });
        }
        // `\begingroup \prepare@multicols`: `\multicol@leftmargin\@totalleftmargin
        // \@totalleftmargin\z@ \parshape\z@`.
        self.begin_group(SEMI_SIMPLE_GROUP);
        let left = self.rw(R_TOTALLEFTMARGIN);
        self.mc_mut().leftmargin = left;
        self.set_rw(R_TOTALLEFTMARGIN, 0, false);
        if !self.e.eqtb.par_shape().is_empty() {
            self.e.eqtb.set_par_shape(Vec::new(), false);
        }
        // `\nointerlineskip {\topskip\z@\null}`: the box goes to the page in the group.
        self.aux = IGNORE_DEPTH;
        self.begin_group(SIMPLE_GROUP);
        self.e.eqtb.set_glue(param!("topskip"), Glue::default(), false);
        let start = self.cur.len();
        let b = self.hpack(start, 0, false);
        let b = self.e.nodes.add_box(b);
        self.append_to_vlist(b);
        // `\output{\global\setbox\partial@page\vbox{…}}\eject`.
        self.run_ops(vec![
            Op::Build,
            Op::Call(|m| {
                m.end_group(SIMPLE_GROUP);
                m.mc_mut().phase = Phase::Collect;
            }),
            Op::Penalty(-10000),
            Op::Call(|m| m.multicols_setup()),
        ]);
    }

    /// The rest of `\prepare@multicols` after its `\eject`, and `\mult@@cols`'s
    /// `\ignorespaces`.
    fn multicols_setup(&mut self) {
        let n = self.mc().n;
        // `\advance\@colroom-\ht\partial@page`, local to the group.
        let partial = self.e.eqtb.box_reg(PARTIAL_BOX).map_or(0, |b| self.e.nodes.boxes[b as usize].height);
        let colroom = self.doc.as_ref().map_or(0, |d| d.floats.colroom()) - partial;
        let mc = self.mc_mut();
        mc.colroom = colroom;
        mc.phase = Phase::Columns;
        self.mult_vsize(false);
        // `\output{\multi@column@out}` (`Phase::Columns`), `\init@mult@footins` (no
        // footnotes reach here: `multicols_collect` refused them), `\reinsert@footnotes`.
        // `\vbadness\@Mi \hbadness5000 \tolerance\multicoltolerance \pretolerance
        // \multicolpretolerance \setemergencystretch\col@number\hsize`.
        self.e.eqtb.set_word(param!("vbadness"), 10001, false);
        self.e.eqtb.set_word(param!("hbadness"), 5000, false);
        self.e.eqtb.set_word(param!("tolerance"), 9999, false);
        self.e.eqtb.set_word(param!("pretolerance"), -1, false);
        self.e.eqtb.set_word(param!("emergencystretch"), 4 * 65536 * n, false);
        // `\advance\baselineskip\multicolbaselineskip` adds 0pt. `\hsize\linewidth
        // \advance\hsize\columnsep \advance\hsize-\col@number\columnsep \divide\hsize
        // \col@number \full@width\linewidth \linewidth\hsize \columnwidth\hsize`.
        let line = self.rw(R_LINEWIDTH);
        let sep = self.e.eqtb.word(super::COLUMNSEP);
        let hsize = (line + sep - n * sep) / n;
        self.e.eqtb.set_word(param!("hsize"), hsize, false);
        self.mc_mut().full_width = line;
        self.set_rw(R_LINEWIDTH, hsize, false);
        self.e.eqtb.set_word(super::COLUMN_WIDTH, hsize, false);
        self.ignore_spaces();
    }

    /// `\set@mult@vsize` with the environment's `\@colroom`; `global` for `\set@mult@vsize
    /// \global` (whose last `\advance` makes the value global).
    fn mult_vsize(&mut self, global: bool) {
        let (n, colroom) = (self.mc().n, self.mc().colroom);
        let b = self.e.eqtb.glue(param!("baselineskip")).width;
        let t = b - self.e.eqtb.glue(param!("topskip")).width;
        // `\vsize\@colroom \advance\vsize\@tempdima \vsize\col@number\vsize
        // \advance\vsize-\@tempdima \advance\vsize\col@number\baselineskip`;
        // `\c@collectmore` is 0.
        let v = (colroom + t).wrapping_mul(n) - t + n.wrapping_mul(b);
        self.e.eqtb.set_word(param!("vsize"), v, global);
    }

    /// The output routine while multicol's is in force: the `\partial@page` routine of
    /// `\prepare@multicols`, else `\multi@column@out`.
    pub(super) fn multicols_output(&mut self) {
        if self.mc().phase == Phase::Collect {
            return self.multicols_collect();
        }
        let p = self.e.eqtb.word(param!("outputpenalty"));
        if p >= -10000 {
            return self.multi_column_out();
        }
        // `\speci@ls`.
        match p {
            -10006 => {
                // `\outputpenalty\@M \balance@columns@out`.
                self.e.eqtb.set_word(param!("outputpenalty"), 10000, false);
                if self.mc().star {
                    self.mc_mut().star_penalty = true;
                    return self.multi_column_out();
                }
                self.balance_out();
            }
            -10005 => self.not_native("\\columnbreak"),
            _ if p < -20000 => self.not_native("the document's end inside multicols"),
            -10001 => self.not_native("\\clearpage inside multicols"),
            _ => self.not_native("a float inside multicols"),
        }
    }

    /// `\global\setbox\partial@page\vbox{\unvbox\@cclv \global\setbox\last@line\lastbox}`.
    fn multicols_collect(&mut self) {
        if self.e.eqtb.box_reg(FOOTINS).is_some() {
            return self.not_native("footnotes before multicols");
        }
        if self.e.eqtb.box_reg(PARTIAL_BOX).is_some() {
            return self.not_native("multicols: a partial page saved twice");
        }
        let start = self.cur.len();
        if let Some(b) = self.e.eqtb.box_reg(255) {
            self.e.eqtb.replace_box(255, None);
            self.mc_unvbox(b);
        }
        // `\lastbox` (§1080): the last node, when it is a box (the `\null`).
        if self.cur.len() > start && self.cur.last().is_some_and(|n| n.kind == Kind::Box) {
            if let Some(n) = self.cur.pop() {
                self.e.nodes.flush_node(n);
            }
        }
        let d = self.dimen_param(BOX_MAX_DEPTH);
        let b = self.vpack(start, 0, false, d);
        let b = self.e.nodes.add_box(BoxRec { vertical: true, ..b });
        self.e.eqtb.set_box(PARTIAL_BOX, Some(b), true);
        self.mc_mut().phase = Phase::Columns;
    }

    /// `\unvbox` of box `b`, which dies, onto the current list.
    fn mc_unvbox(&mut self, b: u32) {
        let r = self.e.nodes.boxes[b as usize];
        self.cur.extend_from_slice(self.e.nodes.get(r.list));
        self.e.nodes.free_block(r.list);
        self.e.nodes.free_box(b);
    }

    /// `\vsplit` of the box `b` holds to height `h`: the top part; `b` keeps the rest.
    fn mc_split(&mut self, b: &mut Option<u32>, h: i32) -> Option<u32> {
        self.e.eqtb.replace_box(SPLIT_BOX, *b);
        let top = self.vsplit(SPLIT_BOX, h);
        *b = self.e.eqtb.box_reg(SPLIT_BOX);
        self.e.eqtb.replace_box(SPLIT_BOX, None);
        top
    }

    /// `\vbox to h{\unvbox b …}` (or natural, `exact` false), with `tail` glue at the end
    /// and `head` glue at the start, `\boxmaxdepth` from the parameter; `\badness` is set.
    fn mc_repack(&mut self, b: Option<u32>, h: i32, exact: bool, head: Option<Glue>, tail: Option<Glue>) -> u32 {
        let start = self.cur.len();
        if let Some(g) = head {
            let n = self.e.nodes.glue_node(g, 0);
            self.cur.push(n);
        }
        if let Some(b) = b {
            self.mc_unvbox(b);
        }
        if let Some(g) = tail {
            let n = self.e.nodes.glue_node(g, 0);
            self.cur.push(n);
        }
        let d = self.dimen_param(BOX_MAX_DEPTH);
        let r = self.vpack(start, h, exact, d);
        self.e.nodes.add_box(BoxRec { vertical: true, ..r })
    }

    fn mc_ht(&self, b: Option<u32>) -> i32 {
        b.map_or(0, |b| self.e.nodes.boxes[b as usize].height)
    }

    /// Whether a column holds a mark (multicol's mark structures are not native).
    fn has_mark(&self, b: u32) -> bool {
        let r = self.e.nodes.boxes[b as usize];
        self.e.nodes.get(r.list).iter().any(|n| matches!(n.kind, Kind::Mark | Kind::Ins | Kind::Adjust))
    }

    /// `\multi@column@out` with no forced column breaks, footnotes or `\@kludgeins`.
    fn multi_column_out(&mut self) {
        if self.e.eqtb.box_reg(FOOTINS).is_some() {
            return self.not_native("footnotes inside multicols");
        }
        let (n, dimen) = (self.mc().n, self.mc().colroom);
        let max = self.doc.as_ref().map_or(0, |d| d.class.dimen("@maxdepth"));
        // `\splittopskip\topskip \splitmaxdepth\@maxdepth \boxmaxdepth\@maxdepth`.
        let top = self.e.eqtb.glue(param!("topskip"));
        self.e.eqtb.set_glue(param!("splittopskip"), top, false);
        self.e.eqtb.set_word(param!("splitmaxdepth"), max, false);
        self.e.eqtb.set_word(param!("boxmaxdepth"), max, false);
        // `\process@cols\mult@firstbox{\setbox\count@\vsplit\@cclv to\dimen@ \setbox\count@
        // \vbox to\dimen@{\unvbox\count@}}`, then the same for `\mult@rightbox`.
        let mut rest = self.e.eqtb.box_reg(255);
        self.e.eqtb.replace_box(255, None);
        let mut cols = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let top = self.mc_split(&mut rest, dimen);
            cols.push(self.mc_repack(top, dimen, true, None, None));
        }
        if cols.iter().any(|&c| self.has_mark(c)) {
            return self.not_native("marks or insertions inside multicols");
        }
        // `\ifvoid\@cclv \else \unvbox\@cclv \ifnum\outputpenalty=\@M \else \penalty
        // \outputpenalty \fi \fi`.
        if let Some(b) = rest {
            self.mc_unvbox(b);
            let p = self.e.eqtb.word(param!("outputpenalty"));
            if p != 10000 {
                self.cur.push(Node::new(Kind::Penalty, p));
            }
        }
        // `\boxmaxdepth\maxdimen \setbox\@cclv\vbox{\unvbox\partial@page \page@sofar}`.
        self.e.eqtb.set_word(param!("boxmaxdepth"), MAX_DIMEN, false);
        let saved = self.aux;
        self.aux = IGNORE_DEPTH;
        let start = self.cur.len();
        if let Some(p) = self.e.eqtb.box_reg(PARTIAL_BOX) {
            self.e.eqtb.replace_box(PARTIAL_BOX, None);
            self.mc_unvbox(p);
        }
        self.page_sofar(&cols);
        let b = self.vpack(start, 0, false, MAX_DIMEN);
        self.aux = saved;
        let b = self.e.nodes.add_box(BoxRec { vertical: true, ..b });
        self.e.eqtb.replace_box(255, Some(b));
        // `\@makecol\@outputpage`; `after_opcol` ends in `multicols_tail`.
        self.output_column();
    }

    /// The end of `\multi@column@out` after `\@outputpage` and its float pages:
    /// `\global\@colroom\@colht … \process@deferreds` (`after_opcol`'s) and
    /// `\set@mult@vsize\global`; for `multicols*`, `\balance@columns@out`'s
    /// `\penalty-\@Mvi`.
    pub(super) fn multicols_tail(&mut self) {
        let colroom = self.doc.as_ref().map_or(0, |d| d.floats.colroom());
        self.mc_mut().colroom = colroom;
        self.mult_vsize(true);
        if std::mem::take(&mut self.mc_mut().star_penalty) {
            self.cur.push(Node::new(Kind::Penalty, -10006));
        }
    }

    /// `\balance@columns@out` of `multicols`.
    fn balance_out(&mut self) {
        if self.e.eqtb.box_reg(FOOTINS).is_some() {
            return self.not_native("footnotes inside multicols");
        }
        // `\setbox\mult@box\vbox{\unvbox\@cclv \remove@discardable@items}`.
        let start = self.cur.len();
        if let Some(b) = self.e.eqtb.box_reg(255) {
            self.e.eqtb.replace_box(255, None);
            self.mc_unvbox(b);
        }
        self.remove_discardable(start);
        let d = self.dimen_param(BOX_MAX_DEPTH);
        let b = self.vpack(start, 0, false, d);
        let mult = self.e.nodes.add_box(BoxRec { vertical: true, ..b });
        let Some(cols) = self.balance_columns(mult) else {
            return self.not_native("multicols: balancing failed (a normal page cut)");
        };
        if cols.iter().any(|&c| self.has_mark(c)) {
            return self.not_native("marks or insertions inside multicols");
        }
        // `\global\vsize\@colroom \global\advance\vsize\ht\partial@page`.
        let partial = self.e.eqtb.box_reg(PARTIAL_BOX);
        let v = self.mc().colroom + self.mc_ht(partial);
        self.e.eqtb.set_word(param!("vsize"), v, true);
        // `\unvbox\partial@page \page@sofar \penalty\z@`.
        if let Some(p) = partial {
            self.e.eqtb.replace_box(PARTIAL_BOX, None);
            self.mc_unvbox(p);
        }
        self.page_sofar(&cols);
        self.cur.push(Node::new(Kind::Penalty, 0));
    }

    /// `\remove@discardable@items` on the list from `start`: `\unpenalty` and `\unskip`
    /// until neither removes anything, a `0pt` glue before glue after a `\penalty10000`
    /// kept.
    fn remove_discardable(&mut self, start: usize) {
        let last = |m: &Self, k: Kind| m.cur.len() > start && m.cur.last().is_some_and(|n| n.kind == k);
        loop {
            if last(self, Kind::Penalty) {
                let n = self.cur.pop().expect("a penalty");
                self.e.nodes.flush_node(n);
            }
            // `\edef\@tempa{\the\lastskip}\unskip`.
            let a = if last(self, Kind::Glue) {
                let n = self.cur.pop().expect("a glue");
                let g = self.e.nodes.glue(&n);
                self.e.nodes.flush_node(n);
                g
            } else {
                Glue::default()
            };
            if a != Glue::default() {
                continue;
            }
            // `\ifx\@tempa\the@zero@skip`: `\edef\@tempb{\the\lastskip}`.
            let b = if last(self, Kind::Glue) { self.e.nodes.glue(self.cur.last().expect("a glue")) } else { Glue::default() };
            if b == Glue::default() {
                return;
            }
            let n = self.cur.pop().expect("a glue");
            self.e.nodes.flush_node(n);
            let p = if last(self, Kind::Penalty) { self.cur.last().map_or(0, |n| n.a) } else { 0 };
            if p == 10000 {
                // `\vskip\@tempb\vskip\@tempa\relax`.
                for g in [b, a] {
                    let n = self.e.nodes.glue_node(g, 0);
                    self.cur.push(n);
                }
                return;
            }
        }
    }

    /// `\balance@columns` (not boxed): the columns, or `None` when `\iftoo@bad` (or a forced
    /// break is left over).
    fn balance_columns(&mut self, mult: u32) -> Option<Vec<u32>> {
        let (n, colroom) = (self.mc().n, self.mc().colroom);
        let max = self.doc.as_ref().map_or(0, |d| d.class.dimen("@maxdepth"));
        // `\setbox\mult@box\vbox{\penalty-\@M \unvbox\mult@box}`.
        let start = self.cur.len();
        self.cur.push(Node::new(Kind::Penalty, -10000));
        self.mc_unvbox(mult);
        let d = self.dimen_param(BOX_MAX_DEPTH);
        let b = self.vpack(start, 0, false, d);
        let mut mult = Some(self.e.nodes.add_box(BoxRec { vertical: true, ..b }));
        // `\splittopskip\topskip plus\multicolundershoot minus\multicolovershoot
        // \splitmaxdepth\@maxdepth \boxmaxdepth\@maxdepth \setbox\@tempboxa\vsplit\mult@box
        // to\z@`.
        let top = self.e.eqtb.glue(param!("topskip")).width;
        self.e.eqtb.set_glue(param!("splittopskip"), Glue { width: top, stretch: UNDERSHOOT, ..Glue::default() }, false);
        self.e.eqtb.set_word(param!("splitmaxdepth"), max, false);
        self.e.eqtb.set_word(param!("boxmaxdepth"), max, false);
        if let Some(t) = self.mc_split(&mut mult, 0) {
            self.e.nodes.flush_box(t);
        }
        let b = self.e.eqtb.glue(param!("baselineskip")).width;
        let total = mult.map_or(0, |m| self.e.nodes.boxes[m as usize].height + self.e.nodes.boxes[m as usize].depth);
        let tempdima = total / n;
        let count = if b == 0 { 0 } else { tempdima / b };
        let mut dimen = count.wrapping_mul(b) + top;
        if dimen > tempdima {
            dimen -= b;
        }
        // `\c@minrows` 1, `\c@unbalance` 0.
        dimen = dimen.max(top);
        // `\vbadness\@M \vfuzz\maxdimen \last@try-\p@`.
        self.e.eqtb.set_word(param!("vbadness"), 10000, false);
        self.e.eqtb.set_word(param!("vfuzz"), MAX_DIMEN, false);
        let mut last_try = -65536;
        let (mut gfirst, mut gright): (Vec<u32>, Option<u32>);
        let mut leftover;
        loop {
            gright = mult.map(|m| self.e.nodes.copy_box(m));
            let mut too_bad = false;
            leftover = false;
            gfirst = Vec::with_capacity(n as usize - 1);
            for _ in 1..n {
                let t = self.mc_split(&mut gright, dimen);
                gfirst.push(self.mc_repack(t, dimen, true, None, None));
                if self.last_badness > COLUMN_BADNESS {
                    too_bad = true;
                }
            }
            // `\global\setbox\mult@grightbox\vbox{\unvbox\mult@grightbox}`,
            // `\setbox\mult@nat@firstbox\vbox{\unvcopy\mult@gfirstbox}`.
            let mut right = self.mc_repack(gright, 0, false, None, None);
            let copy = self.e.nodes.copy_box(gfirst[0]);
            let nat = self.mc_repack(Some(copy), 0, false, None, None);
            let nat_ht = self.mc_ht(Some(nat));
            self.e.nodes.flush_box(nat);
            if self.mc_ht(Some(right)) > dimen {
                too_bad = true;
            } else {
                // `\setbox\@tempboxa\copy\mult@grightbox \setbox\z@\vsplit\@tempboxa
                // to\maxdimen`.
                let mut tmp = Some(self.e.nodes.copy_box(right));
                if let Some(t) = self.mc_split(&mut tmp, MAX_DIMEN) {
                    self.e.nodes.flush_box(t);
                }
                match tmp {
                    None => {
                        right = self.mc_repack(Some(right), dimen, true, None, None);
                        if self.last_badness > FINAL_COLUMN_BADNESS {
                            let fil = Glue { stretch: 65536, stretch_order: 1, ..Glue::default() };
                            right = self.mc_repack(Some(right), dimen, true, None, Some(fil));
                        }
                    }
                    Some(t) => {
                        self.e.nodes.flush_box(t);
                        if dimen < colroom + MAX_BALANCING_OVERFLOW {
                            too_bad = true;
                        } else {
                            leftover = true;
                        }
                    }
                }
            }
            gright = Some(right);
            if nat_ht < dimen && nat_ht > last_try {
                too_bad = true;
                dimen = nat_ht;
                last_try = dimen;
                dimen -= 65536;
            }
            if !too_bad {
                break;
            }
            dimen += 65536;
            for c in gfirst.drain(..).chain(gright.take()) {
                self.e.nodes.flush_box(c);
            }
        }
        if let Some(m) = mult {
            self.e.nodes.flush_box(m);
        }
        if leftover {
            for c in gfirst.drain(..).chain(gright.take()) {
                self.e.nodes.flush_box(c);
            }
            return None;
        }
        dimen = dimen.min(colroom);
        // `\process@cols\mult@rightbox{… \vfuzz\z@ \setbox\count@\vbox to\dimen@{\vskip\z@
        // \@plus-\multicolundershoot \@minus-\multicolovershoot \unvbox\@tempcnta}…}`.
        let head = Glue { stretch: -UNDERSHOOT, ..Glue::default() };
        let mut cols = Vec::with_capacity(n as usize);
        let mut too_bad = false;
        for g in gfirst.into_iter().chain(gright) {
            self.e.eqtb.set_word(param!("vfuzz"), 0, false);
            let c = self.mc_repack(Some(g), dimen, true, Some(head), None);
            if self.last_badness > 10000 {
                // `\vfuzz\maxdimen \setbox\@tempboxa\vbox to\dimen@{\vskip
                // -\maxbalancingoverflow \unvcopy\count@}`.
                self.e.eqtb.set_word(param!("vfuzz"), MAX_DIMEN, false);
                let copy = self.e.nodes.copy_box(c);
                let t = self.mc_repack(Some(copy), dimen, true, Some(Glue { width: -MAX_BALANCING_OVERFLOW, ..Glue::default() }), None);
                if self.last_badness > 10000 {
                    too_bad = true;
                }
                self.e.nodes.flush_box(t);
            }
            cols.push(c);
        }
        if too_bad {
            for c in cols {
                self.e.nodes.flush_box(c);
            }
            return None;
        }
        Some(cols)
    }

    /// `\page@sofar` with `\LR@column@boxes`, on the current vertical list.
    fn page_sofar(&mut self, cols: &[u32]) {
        let hsize = self.dimen_param(super::super::engine_pack::HSIZE);
        let (full, left) = (self.mc().full_width, self.mc().leftmargin);
        // `\wd\count@\hsize`.
        for &c in cols {
            self.e.nodes.boxes[c as usize].width = hsize;
        }
        // `\nointerlineskip \setbox\z@\hbox{\multicolmindepthstring}\global\dimen\tw@\dp\z@`.
        self.aux = IGNORE_DEPTH;
        let font = self.cur_font();
        let [pw, ph, pd, _] = self.e.fonts.dims(font, b'p').unwrap_or([0; 4]);
        let mut dimen2 = pd;
        let rule = self.e.eqtb.word(super::COLUMNSEPRULE);
        let hss = Glue { stretch: 65536, stretch_order: 1, shrink: 65536, shrink_order: 1, ..Glue::default() };
        let colored = self.colored();
        // `\moveright\multicol@leftmargin \hbox to\full@width{\mc@align@columns \rlap{\phantom
        // \multicolmindepthstring}}`.
        let start = self.cur.len();
        for (k, &c) in cols.iter().enumerate() {
            dimen2 = dimen2.max(self.e.nodes.boxes[c as usize].depth);
            self.cur.push(Node::new(Kind::Box, c as i32));
            if k + 1 < cols.len() {
                // `\hss{\columnseprulecolor\vrule\@width\columnseprule}\hss`, the colour
                // `\normalcolor`.
                let g = self.e.nodes.glue_node(hss, 0);
                self.cur.push(g);
                if colored {
                    self.color_stack(Some(DEFAULT_COLOR));
                }
                let r = self.e.nodes.rule_node([rule, RUNNING, RUNNING]);
                self.cur.push(r);
                if colored {
                    self.color_stack(None);
                }
                let g = self.e.nodes.glue_node(hss, 0);
                self.cur.push(g);
            }
        }
        // `\rlap{\phantom p}`: `\hb@xt@\z@{\box\tw@\hss}`, box 2 an empty box the size of `p`.
        let inner = self.cur.len();
        let ph_box = self.e.nodes.add_box(BoxRec { width: pw, height: ph, depth: pd, ..BoxRec::default() });
        self.cur.push(Node::new(Kind::Box, ph_box as i32));
        let g = self.e.nodes.glue_node(hss, 0);
        self.cur.push(g);
        let rlap = self.hpack(inner, 0, true);
        let rlap = self.e.nodes.add_box(rlap);
        self.cur.push(Node::new(Kind::Box, rlap as i32));
        let row = self.hpack(start, full, true);
        let row = self.e.nodes.add_box(BoxRec { shift: left, ..row });
        self.append_to_vlist(row);
        self.e.eqtb.set_word(Reg::register(Region::Dimen, 2), dimen2, true);
        // `\prevdepth\z@ \kern-\dimen\tw@`.
        self.aux = 0;
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, -dimen2) });
    }

    /// `\end{multicols}` (`\endmulticols`; for `multicols*` `\endmulticols*` first).
    pub(super) fn multicols_end(&mut self, star: bool) {
        if self.doc.as_ref().and_then(|d| d.multicol.as_ref()).is_none_or(|m| m.phase != Phase::Columns || m.star != star) {
            return self.not_native("\\end{multicols} without its \\begin");
        }
        let par = self.cur_par();
        if star {
            return self.run_ops(vec![par, Op::Call(|m| m.multicols_end_star())]);
        }
        self.run_ops(vec![par, Op::Call(|m| m.multicols_end_balance())]);
    }

    /// `\endmulticols*` after its `\par`: `\ifdim\lastskip>\z@ \vskip-\lastskip \fi \ifdim
    /// \prevdepth>\z@ \vskip-…\fi \ifshr@nking\else \vfil\fi`, then `\end{multicols}`.
    fn multicols_end_star(&mut self) {
        let last = self.last_skip();
        if last.width > 0 {
            let n = self.e.nodes.glue_node(super::neg(last), 0);
            self.cur.push(n);
        }
        if self.aux > 0 {
            let d = self.aux.min(self.dimen_param(BOX_MAX_DEPTH));
            let n = self.e.nodes.glue_node(Glue { width: -d, ..Glue::default() }, 0);
            self.cur.push(n);
        }
        let n = self.e.nodes.glue_node(Glue { stretch: 65536, stretch_order: 1, ..Glue::default() }, 0);
        self.cur.push(n);
        let par = self.cur_par();
        self.run_ops(vec![par, Op::Call(|m| m.multicols_end_balance())]);
    }

    /// `\endmulticols` after its `\par`: `\penalty\z@ \penalty-\@Mvi` (the balancing
    /// output), `\ifvbox\partial@page \unvbox\partial@page\fi`, `\endgroup`, and what follows.
    fn multicols_end_balance(&mut self) {
        self.run_ops(vec![
            Op::Penalty(0),
            Op::Penalty(-10006),
            Op::Call(|m| {
                if let Some(p) = m.e.eqtb.box_reg(PARTIAL_BOX) {
                    m.e.eqtb.replace_box(PARTIAL_BOX, None);
                    m.mc_unvbox(p);
                }
                m.end_group(SEMI_SIMPLE_GROUP);
                m.mc_mut().phase = Phase::Done;
                // `\global\c@unbalance\z@ \reinsert@footnotes`.
                if m.e.eqtb.box_reg(FOOTINS).is_some() {
                    return m.not_native("footnotes after multicols");
                }
                // `\ifdim \pagegoal=\maxdimen \global\vsize\@colroom \else \enough@room
                // \postmulticols \fi`.
                if m.page_dimen(0) == MAX_DIMEN {
                    let colroom = m.doc.as_ref().map_or(0, |d| d.floats.colroom());
                    m.e.eqtb.set_word(param!("vsize"), colroom, true);
                    return m.multicols_close();
                }
                let par = m.cur_par();
                m.run_ops(vec![
                    par,
                    Op::Call(|m| {
                        let mut ops = m.enough_room();
                        ops.push(Op::Call(|m| m.room_check(POSTMULTICOLS, |m| m.multicols_close())));
                        m.run_ops(ops);
                    }),
                ]);
            }),
        ]);
    }

    /// `\addvspace\multicolsep \prevdepth\z@`, then `\end`'s `\endgroup` (two for
    /// `multicols*`).
    fn multicols_close(&mut self) {
        self.add_vspace(MULTICOLSEP);
        self.aux = 0;
        let star = self.doc.as_mut().and_then(|d| d.multicol.take()).is_some_and(|m| m.star);
        self.end_group(SEMI_SIMPLE_GROUP);
        if star {
            self.end_group(SEMI_SIMPLE_GROUP);
        }
    }
}
