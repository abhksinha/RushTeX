//! Building boxes: tex.web §1055–§1110 (the box, glue, kern, penalty, rule and unboxing
//! commands of `main_control`), §1241 (`\setbox`) and §1247 (`\wd`, `\ht`, `\dp`).
//!
//! While a box is open, the engine builds its list itself (`Machine::building`); the
//! outermost vertical and horizontal lists still go to the native pipeline as tokens.

use super::engine_eqtb::Glue;
use super::engine_node::{BoxRec, Kind, Level, Node, IGNORE_DEPTH, RUNNING};
use super::engine_pack::{BOX_MAX_DEPTH, EVERY_HBOX, EVERY_PAR, EVERY_VBOX, HANG_AFTER, HANG_INDENT, LOOSENESS, PAR_INDENT, SPACE_SKIP, XSPACE_SKIP};
use super::engine_prim::{Meaning, Prim};
use super::engine_run::{Machine, Mode, X};
use super::engine_catcode::Cat;
use super::engine_eqtb::{Reg, Region};
use super::engine_token::Token;
use crate::backend::scaled::xn_over_d;

/// §269 group codes of the box groups.
pub(super) const HBOX_GROUP: u8 = 2;
pub(super) const ADJUSTED_HBOX_GROUP: u8 = 3;
pub(super) const VBOX_GROUP: u8 = 4;
pub(super) const VTOP_GROUP: u8 = 5;

/// §1071 `box_context`: what happens to a box once it is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Ctx {
    /// Appended to the current list, shifted by this much (`\raise`, `\moveleft` ...).
    Shift(i32),
    /// `\setbox n`, `\global\setbox n`.
    Set { n: u32, global: bool },
    /// `\leaders`, `\cleaders`, `\xleaders`: the glue subtype (100, 101, 102).
    Leaders(u8),
    /// `\shipout`.
    Ship,
}

/// An open box: its context, its `to`/`spread` size (§645 `scan_spec`), and its group.
#[derive(Clone, Copy, Debug)]
pub(super) struct BoxFrame {
    ctx: Ctx,
    exact: bool,
    size: i32,
    group: u8,
}

/// §1058 glue of `\hfil` and its kin: (stretch, stretch order, shrink, shrink order).
fn fil_glue(p: Prim) -> Glue {
    const ONE: i32 = 65536;
    let (stretch, so, shrink, ro) = match p {
        Prim::Hfil | Prim::Vfil => (ONE, 1, 0, 0),
        Prim::Hfill | Prim::Vfill => (ONE, 2, 0, 0),
        Prim::Hss | Prim::Vss => (ONE, 1, ONE, 1),
        _ => (-ONE, 1, 0, 0),
    };
    Glue { width: 0, stretch, shrink, stretch_order: so, shrink_order: ro }
}

impl Machine<'_, '_> {
    /// Whether the engine is building a list (inside a box), rather than passing material
    /// to the native pipeline.
    #[inline]
    pub(super) fn building(&self) -> bool {
        self.typeset || !self.nest.is_empty()
    }

    /// §216 `push_nest`: a new list in `mode`.
    pub(super) fn push_nest(&mut self, mode: Mode, inner: bool, aux: i32) {
        self.nest.push(Level { mode: self.mode, inner: self.inner, clang: self.clang as u8, start: self.start, aux: self.aux, prev_graf: self.prev_graf, delim: self.delim });
        self.delim = -1;
        self.mode = mode;
        self.inner = inner;
        self.start = self.cur.len() as u32;
        self.aux = aux;
    }

    /// §217 `pop_nest`: back to the enclosing list (its nodes after `start` were packed).
    pub(super) fn pop_nest(&mut self) {
        if let Some(l) = self.nest.pop() {
            self.mode = l.mode;
            self.inner = l.inner;
            self.clang = l.clang as i32;
            self.start = l.start;
            self.aux = l.aux;
            self.prev_graf = l.prev_graf;
            self.delim = l.delim;
        }
    }

    /// §422: the innermost vertical list's `prev_graf`.
    pub(super) fn prev_graf_of_vlist(&self) -> i32 {
        if self.mode == Mode::Vertical {
            return self.prev_graf;
        }
        self.nest.iter().rev().find(|l| l.mode == Mode::Vertical).map_or(0, |l| l.prev_graf)
    }

    pub(super) fn set_prev_graf_of_vlist(&mut self, v: i32) {
        if self.mode == Mode::Vertical {
            self.prev_graf = v;
        } else if let Some(l) = self.nest.iter_mut().rev().find(|l| l.mode == Mode::Vertical) {
            l.prev_graf = v;
        }
    }

    /// The last node of the list being built, if any.
    fn tail(&self) -> Option<Node> {
        (self.cur.len() > self.start as usize).then(|| self.cur[self.cur.len() - 1])
    }

    /// The box commands; `false` when `p` is not one.
    pub(super) fn box_command(&mut self, x: X, p: Prim, global: bool) -> bool {
        let vertical = self.mode == Mode::Vertical;
        match p {
            Prim::SetBox => {
                let n = self.scan_box_num();
                self.scan_optional_equals();
                self.scan_box(Ctx::Set { n, global });
            }
            Prim::Hbox | Prim::Vbox | Prim::Vtop | Prim::BoxReg | Prim::Copy | Prim::LastBox | Prim::Vsplit => self.begin_box(p, Ctx::Shift(0)),
            Prim::Shipout => self.scan_box(Ctx::Ship),
            // §1073: `\moveleft`/`\moveright` in vertical lists, `\raise`/`\lower` in
            // horizontal ones; a positive shift is right or down.
            Prim::Raise | Prim::Lower | Prim::MoveLeft | Prim::MoveRight => {
                let d = self.scan_dimen(false).0;
                self.scan_box(Ctx::Shift(if matches!(p, Prim::Raise | Prim::MoveLeft) { -d } else { d }));
            }
            Prim::Wd | Prim::Ht | Prim::Dp => self.alter_box_dimen(p),
            Prim::Leaders | Prim::Cleaders | Prim::Xleaders => {
                let sub = 100 + (p as u8 - Prim::Leaders as u8);
                self.scan_box(Ctx::Leaders(sub));
            }
            _ if !self.building() => return false,
            // §1090: in a vertical list, horizontal material starts a paragraph.
            Prim::Hskip | Prim::Hfil | Prim::Hfill | Prim::Hss | Prim::HfilNeg | Prim::Vrule | Prim::Unhbox | Prim::Unhcopy | Prim::Char if vertical => {
                self.input.back(x.t);
                self.new_graf(true);
            }
            Prim::Indent | Prim::NoIndent if vertical => self.new_graf(p == Prim::Indent),
            Prim::ExSpace if vertical => {
                self.input.back(x.t);
                self.new_graf(true);
            }
            Prim::Accent | Prim::Discretionary | Prim::DiscHyphen if vertical => {
                self.input.back(x.t);
                self.new_graf(true);
            }
            Prim::Accent => self.make_accent(),
            Prim::Discretionary | Prim::DiscHyphen => self.append_discretionary(p == Prim::DiscHyphen),
            // §1041: `\ ` is interword glue at space factor 1000.
            Prim::ExSpace => {
                let sf = std::mem::replace(&mut self.aux, 1000);
                self.append_space();
                self.aux = sf;
            }
            // §1093: `\indent` inside a paragraph adds an empty box `\parindent` wide.
            Prim::Indent => {
                let b = self.e.nodes.add_box(BoxRec { width: self.dimen_param(PAR_INDENT), ..BoxRec::default() });
                self.cur.push(Node::new(Kind::Box, b as i32));
                self.aux = 1000;
            }
            Prim::NoIndent => {}
            // §1113: the italic correction of the last character, or of a ligature's
            // character.
            Prim::ItalCorr => {
                if let Some(t) = self.tail().filter(|t| matches!(t.kind, Kind::Char | Kind::Lig)) {
                    let f = &self.e.fonts.list[t.font as usize];
                    let ic = u8::try_from(t.a).ok().and_then(|c| f.dims(c)).map_or(0, |d| d[3]);
                    self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, ic) });
                }
            }
            // §1094: vertical material ends a paragraph; in a restricted horizontal list
            // (an `\hbox`) it does not belong.
            Prim::Vskip | Prim::Vfil | Prim::Vfill | Prim::Vss | Prim::VfilNeg | Prim::Hrule | Prim::Unvbox | Prim::Unvcopy if !vertical => {
                if self.inner {
                    self.error(format!("Missing }} inserted (\\{} in an \\hbox)", p.name()));
                    return true;
                }
                // §1095 `head_for_vmode`: the command again after an inserted `\par` token,
                // whatever `\par` means now (LaTeX's list `\par` restores the `\parshape`).
                self.input.back(x.t);
                let par = Token::cs(self.e.cs.intern("par"));
                self.input.back(par);
            }
            // §1094
            Prim::Par => {
                if !vertical {
                    self.end_graf();
                } else {
                    self.normal_paragraph();
                }
                self.maybe_build_page();
            }
            Prim::Hskip | Prim::Vskip => {
                let (g, fresh) = self.scan_glue_spec(false);
                let mut n = self.e.nodes.glue_node(g, 0);
                n.font = (fresh && g == Glue::default()) as u16;
                self.cur.push(n);
            }
            Prim::Hfil | Prim::Hfill | Prim::Hss | Prim::HfilNeg | Prim::Vfil | Prim::Vfill | Prim::Vss | Prim::VfilNeg => {
                let n = self.e.nodes.glue_node(fil_glue(p), 0);
                self.cur.push(n);
            }
            // §1061: an explicit kern.
            Prim::Kern => {
                let d = self.scan_dimen(false).0;
                self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, d) });
            }
            // §1103
            Prim::Penalty => {
                let v = self.scan_int();
                self.cur.push(Node::new(Kind::Penalty, v));
                self.maybe_build_page();
            }
            Prim::Hrule | Prim::Vrule => {
                let dims = self.scan_rule_spec(p);
                let n = self.e.nodes.rule_node(dims);
                self.cur.push(n);
                if vertical {
                    self.aux = IGNORE_DEPTH;
                } else if self.mode == Mode::Horizontal {
                    self.aux = 1000;
                }
            }
            Prim::Unhbox | Prim::Unhcopy | Prim::Unvbox | Prim::Unvcopy => self.unpackage(p),
            // §1105 `delete_last`.
            Prim::Unskip | Prim::Unkern | Prim::Unpenalty => {
                let kind = match p {
                    Prim::Unskip => Kind::Glue,
                    Prim::Unkern => Kind::Kern,
                    _ => Kind::Penalty,
                };
                if self.tail().is_some_and(|t| t.kind == kind && !t.is_margin_kern()) {
                    self.cur.pop();
                }
            }
            Prim::Char => {
                let c = self.scan_char_num();
                self.append_char(c);
            }
            _ => return false,
        }
        true
    }

    /// §436 `scan_register_num` for a box register.
    pub(super) fn scan_box_num(&mut self) -> u32 {
        let n = self.scan_int();
        if (0..=32767).contains(&n) {
            n as u32
        } else {
            self.error(format!("Bad register code ({n})"));
            0
        }
    }

    /// §1084 `scan_box`: a box specification (`\hbox{...}`, `\box n` ...), or a rule for
    /// leaders.
    fn scan_box(&mut self, ctx: Ctx) {
        let Some(x) = self.next_non_blank_non_relax() else { return };
        match x.m {
            Meaning::Prim(p @ (Prim::Hbox | Prim::Vbox | Prim::Vtop | Prim::BoxReg | Prim::Copy | Prim::LastBox | Prim::Vsplit)) => self.begin_box(p, ctx),
            Meaning::Prim(p @ (Prim::Hrule | Prim::Vrule)) if matches!(ctx, Ctx::Leaders(_)) => {
                let dims = self.scan_rule_spec(p);
                let Ctx::Leaders(sub) = ctx else { return };
                // Rule leaders: the rule's index, with the subtype 10 above box leaders.
                let r = self.e.nodes.rule_node(dims);
                self.leaders(sub + 10, r.a);
            }
            _ => {
                self.error("A <box> was supposed to be here");
                self.input.back(x.t);
            }
        }
    }

    /// §1079 `begin_box`.
    fn begin_box(&mut self, p: Prim, ctx: Ctx) {
        match p {
            // §1082 `\vsplit n to h`.
            Prim::Vsplit => {
                let n = self.scan_box_num();
                if !self.scan_keyword("to") {
                    self.error("Missing `to' inserted");
                }
                let h = self.scan_dimen(false).0;
                let b = self.vsplit(n, h);
                self.box_end(ctx, b);
            }
            Prim::BoxReg | Prim::Copy => {
                let n = self.scan_box_num();
                let b = self.e.eqtb.box_reg(n);
                if p == Prim::BoxReg {
                    // §1079: the register becomes void at its own level.
                    self.e.eqtb.replace_box(n, None);
                }
                self.box_end(ctx, b);
            }
            // §1080: the last node of the current list, if it is a box.
            Prim::LastBox => {
                let b = match self.tail() {
                    Some(t) if self.building() && t.kind == Kind::Box => {
                        self.cur.pop();
                        Some(self.with_shift(t.a as u32, 0))
                    }
                    _ => None,
                };
                self.box_end(ctx, b);
            }
            _ => {
                // §1083: the group, the size, then the list in its own mode.
                let horizontal = p == Prim::Hbox;
                let group = match p {
                    Prim::Hbox if !matches!(ctx, Ctx::Set { .. }) && self.mode == Mode::Vertical => ADJUSTED_HBOX_GROUP,
                    Prim::Hbox => HBOX_GROUP,
                    Prim::Vbox => VBOX_GROUP,
                    _ => VTOP_GROUP,
                };
                let (exact, size) = if self.scan_keyword("to") {
                    (true, self.scan_dimen(false).0)
                } else if self.scan_keyword("spread") {
                    (false, self.scan_dimen(false).0)
                } else {
                    (false, 0)
                };
                self.frames.push(BoxFrame { ctx, exact, size, group });
                self.begin_group(group);
                self.scan_left_brace();
                if horizontal {
                    self.push_nest(Mode::Horizontal, true, 1000);
                    self.insert_toks(EVERY_HBOX);
                } else {
                    self.normal_paragraph();
                    self.push_nest(Mode::Vertical, true, IGNORE_DEPTH);
                    self.insert_toks(EVERY_VBOX);
                }
            }
        }
    }

    /// §1167 `\vcenter`: an internal vertical list that will be centred on the axis.
    pub(super) fn begin_vcenter(&mut self) {
        let (exact, size) = if self.scan_keyword("to") {
            (true, self.scan_dimen(false).0)
        } else if self.scan_keyword("spread") {
            (false, self.scan_dimen(false).0)
        } else {
            (false, 0)
        };
        let group = super::engine_noad::VCENTER_GROUP;
        self.frames.push(BoxFrame { ctx: Ctx::Shift(0), exact, size, group });
        self.begin_group(group);
        self.scan_left_brace();
        self.normal_paragraph();
        self.push_nest(Mode::Vertical, true, IGNORE_DEPTH);
        self.insert_toks(EVERY_VBOX);
    }

    /// §403 `scan_left_brace`.
    pub(super) fn scan_left_brace(&mut self) {
        match self.next_non_blank_non_relax() {
            // By meaning: `\bgroup` is `\let` to `{`.
            Some(x) if matches!(x.m, Meaning::Char(c) if c.cat() == Some(Cat::BeginGroup)) => {}
            other => {
                self.error("Missing { inserted");
                if let Some(x) = other {
                    self.input.back(x.t);
                }
            }
        }
    }

    /// A token-list parameter (`\everyhbox` ...) read next.
    pub(super) fn insert_toks(&mut self, slot: u32) {
        let span = self.e.eqtb.toks(Reg { region: Region::Toks, slot });
        if !span.is_empty() {
            let list = self.e.toks.get(span).to_vec();
            self.back_list(&list);
        }
    }

    /// Whether the innermost group is a box's: its `}` packages the box.
    pub(super) fn in_box_group(&self) -> bool {
        matches!(self.groups.last(), Some(&(HBOX_GROUP | ADJUSTED_HBOX_GROUP | VBOX_GROUP | VTOP_GROUP | super::engine_noad::VCENTER_GROUP)))
    }

    /// §1086 `package`: the `}` of a box.
    pub(super) fn package(&mut self) {
        let Some(f) = self.frames.pop() else { return };
        let d = self.dimen_param(BOX_MAX_DEPTH);
        // A paragraph still open in a `\vbox` ends with it (§1100 calls `end_graf` first).
        if self.mode == Mode::Horizontal && !self.inner {
            self.end_graf();
        }
        self.end_group(f.group);
        let start = self.start as usize;
        // §1085: an `\hbox` in a vertical list hands its adjustments to that list.
        if f.group == ADJUSTED_HBOX_GROUP {
            self.adjust = Some(Vec::new());
        }
        // §1168: a `\vcenter` has no depth limit.
        let d = if f.group == super::engine_noad::VCENTER_GROUP { 0o7777777777 } else { d };
        let mut b = if self.mode == Mode::Horizontal { self.hpack(start, f.size, f.exact) } else { self.vpack(start, f.size, f.exact, d) };
        self.adjust_out = self.adjust.take();
        // §1087: a `\vtop` has the height of its first box or rule, the rest below.
        if f.group == VTOP_GROUP {
            let h = match self.e.nodes.get(b.list).first() {
                Some(n) if n.kind == Kind::Box => self.e.nodes.boxes[n.a as usize].height,
                Some(n) if n.kind == Kind::Rule => self.e.nodes.rules[n.a as usize][1],
                _ => 0,
            };
            b.depth = b.depth - h + b.height;
            b.height = h;
        }
        self.pop_nest();
        let idx = self.e.nodes.add_box(b);
        if f.group == super::engine_noad::VCENTER_GROUP {
            return self.end_vcenter(idx);
        }
        self.box_end(f.ctx, Some(idx));
    }

    /// Box `b` with shift `s`: the same record if it already has it, else a copy (boxes are
    /// never changed once made, since `\copy` shares them).
    fn with_shift(&mut self, b: u32, s: i32) -> u32 {
        let r = self.e.nodes.boxes[b as usize];
        if r.shift == s {
            b
        } else {
            self.e.nodes.add_box(BoxRec { shift: s, ..r })
        }
    }

    /// §1075 `box_end`.
    fn box_end(&mut self, ctx: Ctx, b: Option<u32>) {
        match ctx {
            Ctx::Shift(s) => {
                let Some(b) = b else { return };
                let b = self.with_shift(b, s);
                if !self.building() {
                    // Without `typeset`, the outer lists are the native pipeline's.
                    return;
                }
                if self.mode == Mode::Math {
                    // §1076: a box in a formula is the nucleus of an ordinary atom.
                    let nucleus = super::engine_mlist::Field::Box(b);
                    let i = self.add_noad(super::engine_mlist::Noad { nucleus, ..super::engine_mlist::Noad::new(super::engine_mlist::ORD) });
                    self.cur.push(super::engine_mlist::noad_node(i));
                } else if self.mode == Mode::Vertical {
                    self.append_to_vlist(b);
                    // §1076: what `\vadjust`, `\insert` and `\mark` left in an
                    // `\hbox` made in a vertical list follows it.
                    if let Some(mut adj) = self.adjust_out.take() {
                        self.cur.append(&mut adj);
                    }
                    self.maybe_build_page();
                } else {
                    self.aux = 1000;
                    self.cur.push(Node::new(Kind::Box, b as i32));
                }
            }
            Ctx::Set { n, global } => self.e.eqtb.set_box(n, b, global),
            Ctx::Ship => {
                if let Some(b) = b {
                    self.ship_out(b);
                }
            }
            Ctx::Leaders(sub) => {
                if let Some(b) = b {
                    self.leaders(sub, b as i32);
                }
            }
        }
    }

    /// §1078: leaders need glue of the list's own direction next.
    fn leaders(&mut self, sub: u8, what: i32) {
        let Some(x) = self.next_non_blank_non_relax() else { return };
        let horizontal_glue = matches!(x.m, Meaning::Prim(Prim::Hskip | Prim::Hfil | Prim::Hfill | Prim::Hss | Prim::HfilNeg));
        let vertical_glue = matches!(x.m, Meaning::Prim(Prim::Vskip | Prim::Vfil | Prim::Vfill | Prim::Vss | Prim::VfilNeg));
        let vertical = self.mode == Mode::Vertical;
        if (horizontal_glue && !vertical) || (vertical_glue && vertical) {
            let Meaning::Prim(p) = x.m else { return };
            let g = if matches!(p, Prim::Hskip | Prim::Vskip) { self.scan_glue(false) } else { fil_glue(p) };
            if self.building() {
                let mut n = self.e.nodes.glue_node(g, sub);
                n.b = what;
                self.cur.push(n);
            }
        } else {
            self.error("Leaders not followed by proper glue");
            self.input.back(x.t);
        }
    }

    /// §1056 `scan_rule_spec`: width, height, depth, running where not given.
    fn scan_rule_spec(&mut self, p: Prim) -> [i32; 3] {
        const DEFAULT_RULE: i32 = 26214;
        let mut r = if p == Prim::Vrule { [DEFAULT_RULE, RUNNING, RUNNING] } else { [RUNNING, DEFAULT_RULE, 0] };
        loop {
            let k = if self.scan_keyword("width") {
                0
            } else if self.scan_keyword("height") {
                1
            } else if self.scan_keyword("depth") {
                2
            } else {
                return r;
            };
            r[k] = self.scan_dimen(false).0;
        }
    }

    /// §1247 `alter_box_dimen`: a new record with the dimension changed, in place of the
    /// register's box at its level.
    fn alter_box_dimen(&mut self, p: Prim) {
        let n = self.scan_box_num();
        self.scan_optional_equals();
        let v = self.scan_dimen(false).0;
        if let Some(b) = self.e.eqtb.box_reg(n) {
            let mut r = self.e.nodes.boxes[b as usize];
            match p {
                Prim::Wd => r.width = v,
                Prim::Ht => r.height = v,
                _ => r.depth = v,
            }
            let nb = self.e.nodes.add_box(r);
            self.e.eqtb.replace_box(n, Some(nb));
        }
    }

    /// `\wd n`, `\ht n`, `\dp n` as values (§420).
    pub(super) fn box_dimen(&mut self, p: Prim) -> i32 {
        let n = self.scan_box_num();
        self.e.eqtb.box_reg(n).map_or(0, |b| {
            let r = &self.e.nodes.boxes[b as usize];
            match p {
                Prim::Wd => r.width,
                Prim::Ht => r.height,
                _ => r.depth,
            }
        })
    }

    /// §1110 `unpackage`: a box's list into the current list.
    fn unpackage(&mut self, p: Prim) {
        let n = self.scan_box_num();
        let Some(b) = self.e.eqtb.box_reg(n) else { return };
        let r = self.e.nodes.boxes[b as usize];
        let want_vertical = matches!(p, Prim::Unvbox | Prim::Unvcopy);
        if r.vertical != want_vertical {
            self.error("Incompatible list can't be unboxed");
            return;
        }
        let (a, e) = r.list;
        self.cur.extend_from_slice(&self.e.nodes.list[a as usize..e as usize]);
        if matches!(p, Prim::Unhbox | Prim::Unvbox) {
            self.e.eqtb.replace_box(n, None);
        }
    }

    /// §1070 `normal_paragraph`.
    pub(super) fn normal_paragraph(&mut self) {
        let int = |slot| Reg { region: Region::Int, slot };
        if self.e.eqtb.word(int(LOOSENESS)) != 0 {
            self.e.eqtb.set_word(int(LOOSENESS), 0, false);
        }
        let hang = Reg { region: Region::Dimen, slot: HANG_INDENT };
        if self.e.eqtb.word(hang) != 0 {
            self.e.eqtb.set_word(hang, 0, false);
        }
        if self.e.eqtb.word(int(HANG_AFTER)) != 1 {
            self.e.eqtb.set_word(int(HANG_AFTER), 1, false);
        }
        if !self.e.eqtb.par_shape().is_empty() {
            self.e.eqtb.set_par_shape(Vec::new(), false);
        }
    }

    /// `build_page` when the current list is the main vertical list (`if mode=vmode`).
    pub(super) fn maybe_build_page(&mut self) {
        if self.typeset && self.nest.is_empty() && self.mode == Mode::Vertical {
            self.build_page();
        }
    }

    /// §1091 `new_graf`: a paragraph starts inside a vertical box.
    pub(super) fn new_graf(&mut self, indent: bool) {
        // §1091: the main vertical list always gets `\parskip`; a box's only when not empty.
        if self.nest.is_empty() || self.cur.len() > self.start as usize {
            let g = self.e.nodes.glue_node(self.glue_param(super::engine_pack::PAR_SKIP), super::engine_pack::PAR_SKIP as u8 + 1);
            self.cur.push(g);
        }
        self.prev_graf = 0;
        // §1091 `set_cur_lang; clang:=cur_lang`.
        self.clang = self.cur_lang();
        self.push_nest(Mode::Horizontal, false, 1000);
        // §1091: the paragraph's language and hyphen minima, for `line_break` (§891).
        self.prev_graf = self.hyphen_mins() * 0o200000 + self.clang;
        if indent {
            let b = self.e.nodes.add_box(BoxRec { width: self.dimen_param(PAR_INDENT), ..BoxRec::default() });
            self.cur.push(Node::new(Kind::Box, b as i32));
        }
        self.insert_toks(EVERY_PAR);
        // `if nest_ptr=1 then build_page`: the `\parskip` glue goes to the page.
        if self.typeset && self.nest.len() == 1 {
            self.build_page();
        }
    }

    /// §1096 `end_graf`: the paragraph is broken into lines (`line_break`).
    pub(super) fn end_graf(&mut self) {
        if self.mode != Mode::Horizontal || self.inner {
            return;
        }
        // §1096: the error count starts again with each paragraph.
        self.error_count = 0;
        let start = self.start as usize;
        if self.cur.len() == start {
            self.pop_nest();
            return;
        }
        self.line_break(false);
        self.normal_paragraph();
    }

    /// §1034: a character in a horizontal list being built, with the space factor of §1034.
    /// A character in a vertical one starts a paragraph first (§1090).
    pub(super) fn append_char(&mut self, c: u8) {
        if self.mode == Mode::Math {
            let code = self.e.eqtb.code(super::engine_eqtb::Code::Math, c as u32) as u32;
            return self.set_math_char(code, c);
        }
        if self.mode == Mode::Vertical {
            self.input.back(Token::char(Cat::Other, c as char));
            return self.new_graf(true);
        }
        self.main_loop(c);
    }

    /// §1041–§1044 `app_space`: interword glue from `\spaceskip`, `\xspaceskip` or the font,
    /// adjusted by the space factor.
    pub(super) fn append_space(&mut self) {
        if self.mode != Mode::Horizontal {
            return;
        }
        let sf = self.aux;
        let xspace = self.glue_param(XSPACE_SKIP);
        let (g, sub) = if sf >= 2000 && xspace != Glue::default() {
            (xspace, XSPACE_SKIP as u8 + 1)
        } else {
            let space = self.glue_param(SPACE_SKIP);
            let mut g = if space != Glue::default() {
                space
            } else {
                let p = &self.e.fonts.list[self.cur_font() as usize].params;
                Glue { width: p[1], stretch: p[2], shrink: p[3], stretch_order: 0, shrink_order: 0 }
            };
            if sf != 1000 {
                if sf >= 2000 {
                    g.width += self.e.fonts.list[self.cur_font() as usize].params[6];
                }
                g.stretch = xn_over_d(g.stretch, sf, 1000);
                g.shrink = xn_over_d(g.shrink, 1000, sf);
            }
            (g, if space != Glue::default() && sf == 1000 { SPACE_SKIP as u8 + 1 } else { 0 })
        };
        let n = self.e.nodes.glue_node(g, sub);
        self.cur.push(n);
    }

    /// e-TeX's `\lastnodetype` of node `n`: §133's `type` plus one, 0 for a character and
    /// 15 for a noad (`unset_node + 2`).
    pub(super) fn node_type(&self, n: &Node) -> i32 {
        match n.kind {
            Kind::Char => 0,
            Kind::Box => 1 + self.e.nodes.boxes[n.a as usize].vertical as i32,
            Kind::Rule => 3,
            Kind::Ins => 4,
            Kind::Mark => 5,
            Kind::Adjust => 6,
            Kind::Lig => 7,
            Kind::Disc => 8,
            Kind::Whatsit => 9,
            Kind::Math => 10,
            Kind::Glue => 11,
            Kind::Kern => 12,
            Kind::Penalty => 13,
            Kind::Noad => 15,
        }
    }

    /// e-TeX's `\lastnodetype` (§424 as etex.ch extends it): -1 for an empty list or no
    /// mode, the page builder's for an empty main vertical list.
    pub(super) fn last_node_type(&self) -> i32 {
        if self.mode_zero {
            return -1;
        }
        match self.tail() {
            Some(t) => self.node_type(&t),
            None => self.page_last_type().unwrap_or(-1),
        }
    }

    /// `\lastskip`, `\lastkern`, `\lastpenalty` (§424): the last node of the list being built,
    /// if it is of that kind.
    pub(super) fn last_item(&self, kind: Kind) -> Option<Node> {
        self.tail().filter(|t| t.kind == kind && !t.is_margin_kern() && !self.mode_zero)
    }
}

#[cfg(test)]
mod tests {
    use super::super::engine_run::Machine;
    use super::super::Engine;

    /// Box dimensions exactly as pdfTeX gives them (checked with `scripts/engine_probe.py`);
    /// text without ligatures or kerns, which the engine's boxes do not apply yet.
    #[test]
    fn boxes_measure_as_pdftex() {
        let src = r"\catcode`\{=1 \catcode`\}=2 \font\tenrm=cmr10 \tenrm
            \baselineskip=12pt \lineskip=1pt \lineskiplimit=0pt
            \setbox1\hbox to 100pt{A\hfil B}
            \setbox2\hbox spread 5pt{x\hskip 3pt plus 2pt y}
            \setbox3\vbox{\hbox{Ag}\hbox{yx}\kern2pt\hrule height 1pt depth 0.5pt}
            \setbox4\vtop{\hbox{Ag}\hbox{yx}}
            \setbox5\hbox{\raise2pt\hbox{a}\lower3pt\hbox{b}\vrule width 1pt}
            \setbox6\copy1 \wd6=50pt
            \setbox9\hbox to 10pt{Wide text here}\count1=\badness
            \setbox10\vbox to 50pt{\hbox{a}\vfil\hbox{b}}
            \setbox11\hbox{\setbox12\hbox{q}\box12\box12}
            \setbox13\vbox{\moveright 5pt\hbox{m}\hbox{n}}
            \setbox14\hbox{\hbox{a}\setbox15\lastbox \kern\wd15}
            \setbox16\hbox{\leaders\hrule\hskip 20pt \cleaders\hbox{.}\hskip 10pt}
            {\setbox18\hbox{local}}
            \global\setbox19\hbox{\global\setbox20\hbox{inner}}
            \boxmaxdepth=1pt \setbox21\vbox{\hbox{y}}
            \message{1:\the\wd1,\the\ht1,\the\dp1;2:\the\wd2,\the\ht2,\the\dp2;3:\the\wd3,\the\ht3,\the\dp3;4:\the\wd4,\the\ht4,\the\dp4;5:\the\wd5,\the\ht5,\the\dp5;6:\the\wd6;9:\the\wd9,\the\count1;10:\the\wd10,\the\ht10;11:\the\wd11;13:\the\wd13,\the\ht13;14:\the\wd14;16:\the\wd16,\the\ht16;18:\the\wd18;19:\the\wd19;20:\the\wd20,\the\ht20;21:\the\ht21,\the\dp21;\ifvoid1 V\else N\fi\ifhbox3 H\else N\fi\ifvbox3 V\fi\ifvoid12 V\fi}";
        let mut e = Engine::default();
        e.eqtb.use_initex_catcodes();
        let mut m = Machine::new(&mut e, src);
        m.run();
        assert!(m.errors.is_empty(), "{:?}", m.errors);
        let want = "1:100.0pt,6.83331pt,0.0pt;2:18.5556pt,4.30554pt,1.94444pt;3:12.50003pt,24.27776pt,0.0pt;4:12.50003pt,6.83331pt,13.94444pt;5:11.55559pt,6.30554pt,3.0pt;6:50.0pt;9:10.0pt,1000000;10:5.55557pt,50.0pt;11:5.27779pt;13:13.33336pt,16.30554pt;14:5.00002pt;16:30.0pt,1.05554pt;18:0.0pt;19:0.0pt;20:22.25005pt,6.67859pt;21:5.24998pt,1.0pt;NNVV";
        assert!(m.log.contains(want), "log: {}", m.log);
    }
}
