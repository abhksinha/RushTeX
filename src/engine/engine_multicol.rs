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
use super::{Op, FOOTINS, R_LINEWIDTH, R_TOTALLEFTMARGIN};

/// `\partial@page`, global in multicol: the page above the environment.
const PARTIAL_BOX: u32 = 32710;
/// A hidden register `\vsplit` works on (always void between uses).
const SPLIT_BOX: u32 = 32711;
/// `\colbreak@box`, global: the columns ended by `\columnbreak` so far on this page.
const COLBREAK_BOX: u32 = 32712;
/// `\mult@box` of boxed mode: the environment's text, set in a `\vbox`.
const MULT_BOX: u32 = 32713;

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
    /// `\if@boxedmulticols` (`\ifinner`): the text in `\mult@box`, balanced at the end.
    boxed: bool,
    /// `\if@minipage` when the boxed text began: its `\vbox` group keeps the
    /// `\@minipagefalse` of the first paragraph inside.
    minipage: bool,
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
            doc.multicol = Some(Box::new(Multicol { n, star, phase: Phase::Before, colroom: 0, full_width: 0, leftmargin: 0, star_penalty: false, boxed: false, minipage: false }));
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
        if self.inner && self.mode == Mode::Vertical && self.rw(R_TOTALLEFTMARGIN) == 0 && !self.mc().star {
            return self.multicols_boxed();
        }
        if !self.nest.is_empty() || self.mode != Mode::Vertical || self.rw(R_TOTALLEFTMARGIN) != 0 {
            return self.not_native("multicols in a box or a list");
        }
        let mut ops = self.enough_room();
        ops.push(Op::Call(|m| m.room_check(PREMULTICOLS, |m| m.multicols_start())));
        self.run_ops(ops);
    }

    /// Boxed mode (`\ifinner`, a box's internal vertical list): no `\enough@room`;
    /// `\addvspace\multicolsep`, the depth correction, `\begingroup \prepare@multicols`
    /// without its output routines (`\@colroom` stays the page's), then `\setbox\mult@box
    /// \vbox\bgroup \color@setgroup \ignorespaces`.
    fn multicols_boxed(&mut self) {
        self.add_vspace(MULTICOLSEP);
        if self.aux != IGNORE_DEPTH {
            return self.not_native("multicols (boxed) after a line");
        }
        self.begin_group(SEMI_SIMPLE_GROUP);
        let left = self.rw(R_TOTALLEFTMARGIN);
        let colroom = self.doc.as_ref().map_or(0, |d| d.floats.colroom());
        let minipage = self.doc.as_ref().is_some_and(|d| d.floats.minipage);
        let mc = self.mc_mut();
        (mc.leftmargin, mc.colroom, mc.boxed, mc.minipage, mc.phase) = (left, colroom, true, minipage, Phase::Columns);
        self.set_rw(R_TOTALLEFTMARGIN, 0, false);
        if !self.e.eqtb.par_shape().is_empty() {
            self.e.eqtb.set_par_shape(Vec::new(), false);
        }
        self.multicols_params();
        let mut t = vec![self.named_cs(named!("setbox"))];
        t.extend(MULT_BOX.to_string().chars().map(|c| super::Token::char(super::Cat::Other, c)));
        t.push(self.named_cs(named!("vbox")));
        t.push(super::Token::char(super::Cat::BeginGroup, '{'));
        t.push(self.named_cs(named!("begingroup")));
        if self.colored() {
            t.push(self.doc_id(doc!("rt@setcolor")));
        }
        t.push(self.named_cs(named!("ignorespaces")));
        self.back_list(&t);
    }

    /// `\endmulticols` in boxed mode after its `\par`: `\remove@discardable@items
    /// \color@endgroup\egroup`, then [`Self::multicols_boxed_out`].
    fn multicols_boxed_end(&mut self) {
        let start = self.start as usize;
        self.remove_discardable(start);
        if let Some(doc) = self.doc.as_mut() {
            doc.steps.push(vec![Op::Call(|m| m.multicols_boxed_out())]);
        }
        let t = [self.named_cs(named!("endgroup")), super::Token::char(super::Cat::EndGroup, '}'), self.doc_id(super::STEP)];
        self.back_list(&t);
    }

    /// `\balance@columns`, `\global\setbox\mc@boxedresult\vbox{\page@sofar}`, `\endgroup`,
    /// `\unvbox\mc@boxedresult` (no interline glue), `\addvspace\multicolsep \prevdepth\z@`.
    fn multicols_boxed_out(&mut self) {
        let minipage = self.mc().minipage;
        if let Some(doc) = self.doc.as_mut() {
            doc.floats.minipage = minipage;
        }
        let Some(mult) = self.e.eqtb.box_reg(MULT_BOX) else { return self.not_native("multicols (boxed): no text") };
        self.e.eqtb.replace_box(MULT_BOX, None);
        // A balance that fails leaves `\balance@columns`'s own columns: not native yet.
        let Ok(cols) = self.balance_columns(mult) else { return self.not_native("multicols (boxed): a balance that fails") };
        if cols.iter().any(|&c| self.has_mark(c)) {
            return self.not_native("marks or insertions inside multicols");
        }
        let aux = self.aux;
        self.page_sofar(&cols, true);
        self.aux = aux;
        self.end_group(SEMI_SIMPLE_GROUP);
        self.mc_mut().phase = Phase::Done;
        self.multicols_close();
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
        // `\advance\@colroom-\ht\partial@page`, local to the group.
        let partial = self.e.eqtb.box_reg(PARTIAL_BOX).map_or(0, |b| self.e.nodes.boxes[b as usize].height);
        let colroom = self.doc.as_ref().map_or(0, |d| d.floats.colroom()) - partial;
        let mc = self.mc_mut();
        mc.colroom = colroom;
        mc.phase = Phase::Columns;
        self.mult_vsize(false);
        // `\output{\multi@column@out}` (`Phase::Columns`), `\init@mult@footins` (no
        // footnotes reach here: `multicols_collect` refused them), `\reinsert@footnotes`.
        self.multicols_params();
        self.ignore_spaces();
    }

    /// The end of `\prepare@multicols`, both modes: `\vbadness\@Mi \hbadness5000
    /// \tolerance\multicoltolerance \pretolerance \multicolpretolerance
    /// \setemergencystretch\col@number\hsize`, the column width.
    fn multicols_params(&mut self) {
        let n = self.mc().n;
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
            -10005 => self.colbreak_out(),
            _ if p < -20000 => self.not_native("the document's end inside multicols"),
            -10001 => self.not_native("\\clearpage inside multicols"),
            _ => self.not_native("a float inside multicols"),
        }
    }

    /// multicol's `\columnbreak` (no optional argument: `\mc@break@pen` is `-\@Mv`):
    /// `\ifvmode \penalty-10005 \else \@bsphack \vadjust{\penalty-10005}\@esphack \fi`.
    pub(super) fn column_break(&mut self) {
        if self.doc.as_ref().and_then(|d| d.multicol.as_ref()).is_none_or(|m| m.phase != Phase::Columns) {
            return self.not_native("\\columnbreak outside multicols");
        }
        if self.next_is('[') {
            return self.not_native("\\columnbreak[n]");
        }
        match self.mode {
            Mode::Vertical => self.run_ops(vec![Op::Penalty(-10005)]),
            Mode::Horizontal => {
                let saved = (self.last_skip(), self.aux);
                if let Some(doc) = self.doc.as_mut() {
                    doc.sphack.push(saved);
                }
                self.build_adjust(|m| m.build_penalty(-10005));
                self.esphack_end();
            }
            _ => self.not_native("\\columnbreak in math"),
        }
    }

    /// `\speci@ls` at `\outputpenalty` `-\@Mv`: `\global\advance\vsize-\pagetotal`, then
    /// `\global\setbox\colbreak@box\vbox{\ifvoid\colbreak@box\else \unvbox\colbreak@box
    /// \penalty-\@Mv\fi \boxmaxdepth\@maxdepth \setbox\@cclv\vbox{\unvbox\@cclv
    /// \remove@discardable@items}\dimen@\dp\@cclv \unvbox\@cclv \kern-\dimen@}`,
    /// `\reinsert@footnotes`.
    fn colbreak_out(&mut self) {
        if self.e.eqtb.box_reg(FOOTINS).is_some() {
            return self.not_native("footnotes inside multicols");
        }
        let v = self.e.eqtb.word(param!("vsize")) - self.page_dimen(1);
        self.e.eqtb.set_word(param!("vsize"), v, true);
        let max = self.doc.as_ref().map_or(0, |d| d.class.dimen("@maxdepth"));
        let start = self.cur.len();
        if let Some(old) = self.e.eqtb.box_reg(COLBREAK_BOX) {
            self.e.eqtb.replace_box(COLBREAK_BOX, None);
            self.mc_unvbox(old);
            self.cur.push(Node::new(Kind::Penalty, -10005));
        }
        let inner = self.cur.len();
        if let Some(b) = self.e.eqtb.box_reg(255) {
            self.e.eqtb.replace_box(255, None);
            self.mc_unvbox(b);
        }
        self.remove_discardable(inner);
        let packed = self.vpack(inner, 0, false, max);
        let depth = packed.depth;
        let packed = self.e.nodes.add_box(BoxRec { vertical: true, ..packed });
        self.mc_unvbox(packed);
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, -depth) });
        let b = self.vpack(start, 0, false, max);
        let b = self.e.nodes.add_box(BoxRec { vertical: true, ..b });
        self.e.eqtb.set_box(COLBREAK_BOX, Some(b), true);
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
        // `\ifvoid\colbreak@box\else \setbox\@cclv\vbox{\unvbox\colbreak@box \penalty-\@Mv
        // \unvbox\@cclv}\fi`: the forced breaks re-added for splitting.
        if let Some(c) = self.e.eqtb.box_reg(COLBREAK_BOX) {
            self.e.eqtb.replace_box(COLBREAK_BOX, None);
            let start = self.cur.len();
            self.mc_unvbox(c);
            self.cur.push(Node::new(Kind::Penalty, -10005));
            if let Some(b) = self.e.eqtb.box_reg(255) {
                self.e.eqtb.replace_box(255, None);
                self.mc_unvbox(b);
            }
            let d = self.dimen_param(BOX_MAX_DEPTH);
            let b = self.vpack(start, 0, false, d);
            let b = self.e.nodes.add_box(BoxRec { vertical: true, ..b });
            self.e.eqtb.replace_box(255, Some(b));
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
        self.page_sofar(&cols, false);
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
        // `\setbox\mult@box\vbox{\ifvoid\colbreak@box\else \unvbox\colbreak@box
        // \penalty-\@Mv\fi \unvbox\@cclv \remove@discardable@items}`.
        let start = self.cur.len();
        if let Some(c) = self.e.eqtb.box_reg(COLBREAK_BOX) {
            self.e.eqtb.replace_box(COLBREAK_BOX, None);
            self.mc_unvbox(c);
            self.cur.push(Node::new(Kind::Penalty, -10005));
        }
        if let Some(b) = self.e.eqtb.box_reg(255) {
            self.e.eqtb.replace_box(255, None);
            self.mc_unvbox(b);
        }
        self.remove_discardable(start);
        let d = self.dimen_param(BOX_MAX_DEPTH);
        let b = self.vpack(start, 0, false, d);
        let mult = self.e.nodes.add_box(BoxRec { vertical: true, ..b });
        let cols = match self.balance_columns(mult) {
            Ok(cols) => cols,
            Err(mult) => return self.balance_failed(mult),
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
        self.page_sofar(&cols, false);
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

    /// `\balance@columns@out`'s `\iftoo@bad` branch ("Balancing failed ... cut a normal
    /// page"): `\setbox\@cclv\vbox{\vskip\topskip \vskip-\splittopskip \unvbox\mult@box
    /// \penalty-\@Mvi}\multi@column@out`, with `\splittopskip` and `\boxmaxdepth` as
    /// `\balance@columns` left them.
    fn balance_failed(&mut self, mult: Option<u32>) {
        let start = self.cur.len();
        let top = self.e.eqtb.glue(param!("topskip"));
        let split = self.e.eqtb.glue(param!("splittopskip"));
        for g in [top, super::neg(split)] {
            let n = self.e.nodes.glue_node(g, 0);
            self.cur.push(n);
        }
        if let Some(m) = mult {
            self.mc_unvbox(m);
        }
        self.cur.push(Node::new(Kind::Penalty, -10006));
        let d = self.dimen_param(BOX_MAX_DEPTH);
        let b = self.vpack(start, 0, false, d);
        let b = self.e.nodes.add_box(BoxRec { vertical: true, ..b });
        self.e.eqtb.replace_box(255, Some(b));
        self.multi_column_out();
    }

    /// `\balance@columns` (not boxed): the columns, or, when `\iftoo@bad` (or a forced
    /// break is left over), `\mult@box` as the balancing left it.
    fn balance_columns(&mut self, mult: u32) -> Result<Vec<u32>, Option<u32>> {
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
        // The last full trial: where each of its splits broke (each read from the box it
        // split, `None` for no box), the boxes its second and later splits split (kept as
        // copies), its first column's natural height, its right column's natural height, and
        // whether that column kept a forced break (`None`: not looked at, the column too
        // high). A trial whose splits break at the same places has the same columns, so when
        // the right one is higher than `\dimen@`, or keeps a forced break (a `\columnbreak`)
        // below `\@colroom` plus `\maxbalancingoverflow`, it is too bad again: it only moves
        // `\dimen@` (the `\last@try` step), and the copies and splits pdfTeX makes for it
        // change nothing the loop reads after it (the last trial, made in full, sets the split
        // marks).
        struct Trial {
            breaks: Vec<Option<Option<usize>>>,
            kept: Vec<Option<u32>>,
            nat_ht: i32,
            right_ht: i32,
            forced: Option<bool>,
        }
        let mut seen: Option<Trial> = None;
        loop {
            if let Some(t) = &seen {
                let (nat_ht, right_ht, forced) = (t.nat_ht, t.right_ht, t.forced);
                let same = (right_ht > dimen || forced == Some(true) && dimen < colroom + MAX_BALANCING_OVERFLOW) && {
                    let boxes: Vec<Option<u32>> = std::iter::once(mult).chain(t.kept.iter().copied()).collect();
                    let breaks = t.breaks.clone();
                    boxes.into_iter().zip(breaks).all(|(b, q)| b.map(|b| self.split_break(b, dimen)) == q)
                };
                if same {
                    if nat_ht < dimen && nat_ht > last_try {
                        dimen = nat_ht;
                        last_try = dimen;
                        dimen -= 65536;
                    }
                    dimen += 65536;
                    continue;
                }
            }
            if let Some(t) = seen.take() {
                for b in t.kept.into_iter().flatten() {
                    self.e.nodes.flush_box(b);
                }
            }
            let (mut breaks, mut kept) = (Vec::with_capacity(n as usize - 1), Vec::with_capacity(n as usize - 2));
            let mut forced = None;
            gright = mult.map(|m| self.e.nodes.copy_box(m));
            let mut too_bad = false;
            leftover = false;
            gfirst = Vec::with_capacity(n as usize - 1);
            for k in 1..n {
                breaks.push(gright.map(|b| self.split_break(b, dimen)));
                if k > 1 {
                    kept.push(gright.map(|b| self.e.nodes.copy_box(b)));
                }
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
            let right_ht = self.mc_ht(Some(right));
            if right_ht > dimen {
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
                        forced = Some(false);
                        right = self.mc_repack(Some(right), dimen, true, None, None);
                        if self.last_badness > FINAL_COLUMN_BADNESS {
                            let fil = Glue { stretch: 65536, stretch_order: 1, ..Glue::default() };
                            right = self.mc_repack(Some(right), dimen, true, None, Some(fil));
                        }
                    }
                    Some(t) => {
                        self.e.nodes.flush_box(t);
                        forced = Some(true);
                        if dimen < colroom + MAX_BALANCING_OVERFLOW {
                            too_bad = true;
                        } else {
                            leftover = true;
                        }
                    }
                }
            }
            gright = Some(right);
            seen = Some(Trial { breaks, kept, nat_ht, right_ht, forced });
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
        for b in seen.into_iter().flat_map(|t| t.kept).flatten() {
            self.e.nodes.flush_box(b);
        }
        if leftover {
            for c in gfirst.drain(..).chain(gright.take()) {
                self.e.nodes.flush_box(c);
            }
            return Err(mult);
        }
        // `\if@boxedmulticols\else \ifdim\dimen@>\@colroom \dimen@\@colroom\fi\fi`.
        if !self.mc().boxed {
            dimen = dimen.min(colroom);
        }
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
            return Err(mult);
        }
        if let Some(m) = mult {
            self.e.nodes.flush_box(m);
        }
        Ok(cols)
    }

    /// `\page@sofar` with `\LR@column@boxes`, on the current vertical list.
    fn page_sofar(&mut self, cols: &[u32], boxed: bool) {
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
                    self.push_default_color();
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
        // Boxed, the row starts `\mc@boxedresult`'s list, which `\unvbox` hands on as it is.
        if boxed {
            self.cur.push(Node::new(Kind::Box, row as i32));
        } else {
            self.append_to_vlist(row);
        }
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
        if self.mc().boxed {
            return self.run_ops(vec![par, Op::Call(|m| m.multicols_boxed_end())]);
        }
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
        // `\ifdim\pagegoal=\maxdimen \ifvoid\colbreak@box\else \unvbox\colbreak@box\fi\fi`.
        if self.page_dimen(0) == MAX_DIMEN {
            if let Some(c) = self.e.eqtb.box_reg(COLBREAK_BOX) {
                self.e.eqtb.replace_box(COLBREAK_BOX, None);
                self.mc_unvbox(c);
            }
        }
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
