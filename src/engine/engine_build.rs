//! The GDL node builders (`guarded-direct-lowering.md`): the typed operations native code
//! lowers a document to, each the part of TeX's `main_control` that appends to the current
//! list once its operands are known. The primitives' own commands (`\vskip`, `\penalty`,
//! `\hrule`, `\vadjust`) scan their operands and call these, so a native command and the
//! primitive it stands for build the same nodes by one code path.
use super::engine_box::Ctx;
use super::engine_eqtb::{Glue, Reg, Region};
use super::engine_node::{Kind, Node, IGNORE_DEPTH};
use super::engine_run::{Machine, Mode};
use super::engine_token::Token;
use super::engine_value::MAX_DIMEN;

impl<'e, 'a> Machine<'e, 'a> {
    /// §1060 `append_glue` of a scanned specification: `fresh` is a specification written out
    /// (`\vskip 0pt`), not a parameter's or register's (`\vskip\z@skip`), which pdfTeX's
    /// protrusion tells apart when it is zero.
    pub(super) fn build_glue(&mut self, g: Glue, fresh: bool) {
        let mut n = self.e.nodes.glue_node(g, 0);
        n.font = (fresh && g == Glue::default()) as u16;
        self.cur.push(n);
    }

    /// §1103 `append_penalty`; in the outer vertical list the page builder runs.
    pub(super) fn build_penalty(&mut self, v: i32) {
        self.cur.push(Node::new(Kind::Penalty, v));
        self.maybe_build_page();
    }

    /// §1056: a rule of `[width, height, depth]` (`RUNNING` for a running dimension).
    pub(super) fn build_rule(&mut self, dims: [i32; 3]) {
        let n = self.e.nodes.rule_node(dims);
        self.cur.push(n);
        if self.mode == Mode::Vertical {
            self.aux = IGNORE_DEPTH;
        } else if self.mode == Mode::Horizontal {
            self.aux = 1000;
        }
    }

    /// §1097–§1100 `\vadjust{...}` whose material `fill` builds: an internal vertical list,
    /// packed, as an adjust node. The group `\vadjust` opens holds no assignment here, so
    /// it is not entered.
    pub(super) fn build_adjust(&mut self, fill: impl FnOnce(&mut Self)) {
        self.push_nest(Mode::Vertical, true, IGNORE_DEPTH);
        fill(self);
        let start = self.start as usize;
        let b = self.vpack(start, 0, false, MAX_DIMEN);
        self.pop_nest();
        self.cur.push(Node { kind: Kind::Adjust, sub: 0, font: 0, a: b.list.0 as i32, b: b.list.1 as i32 });
    }

    /// `\dimen n` set locally (`\dimen@\prevdepth`).
    pub(super) fn build_dimen_register(&mut self, n: u32, v: i32) {
        self.e.eqtb.set_word(Reg::register(Region::Dimen, n), v, false);
    }

    /// `\dimen n`'s value.
    pub(super) fn dimen_register(&self, n: u32) -> i32 {
        self.e.eqtb.word(Reg::register(Region::Dimen, n))
    }

    /// The glue an argument holds, as `\setlength\skip{#1}` scans it (`\skip=#1\relax`);
    /// `None` when something follows the glue in the argument, which the native job does not
    /// lower.
    pub(super) fn glue_arg(&mut self, arg: &[Token]) -> Option<Glue> {
        let relax = Token::cs(self.e.cs.get("relax")?);
        let mut list = Vec::with_capacity(arg.len() + 1);
        list.extend_from_slice(arg);
        list.push(relax);
        self.back_list(&list);
        self.scan_optional_equals();
        let g = self.scan_glue(false);
        (self.get_token() == Some(relax)).then_some(g)
    }

    /// Characters and spaces as the main loop takes them from the input (§1034–§1044):
    /// ligatures and kerns within each word, space factors, interword glue at each space.
    /// The word at the end of `text` ends there, as before a non-character token: a caller
    /// whose text runs on into the document's own characters leaves that last character a
    /// token. In vertical mode the first character starts a paragraph, as a typed letter
    /// does, before the run is taken.
    pub(super) fn build_text(&mut self, text: &[u8]) {
        if text.is_empty() {
            return;
        }
        if self.mode == Mode::Vertical {
            self.new_graf(true);
        }
        let outer = self.text_run.replace((text.to_vec(), 0));
        while let Some(c) = self.text_run.as_mut().and_then(|(bytes, i)| {
            let c = *bytes.get(*i)?;
            *i += 1;
            Some(c)
        }) {
            if c == b' ' {
                self.append_space();
            } else {
                self.append_char(c);
            }
        }
        self.text_run = outer;
    }

    /// §1083–§1086 `\hbox{...}` at its natural width, whose material `fill` builds, then
    /// §1075 `box_end` in the current list. `\everyhbox` is empty on the GDL path (the
    /// pre-scan refuses it) and the box's group holds no assignment, so neither is run.
    pub(super) fn build_hbox(&mut self, fill: impl FnOnce(&mut Self)) {
        let adjusted = self.mode == Mode::Vertical;
        self.push_nest(Mode::Horizontal, true, 1000);
        fill(self);
        // §1085: an `\hbox` in a vertical list hands its adjustments to that list.
        if adjusted {
            self.adjust = Some(Vec::new());
        }
        let start = self.start as usize;
        let b = self.hpack(start, 0, false);
        self.adjust_out = self.adjust.take();
        self.pop_nest();
        let idx = self.e.nodes.add_box(b);
        self.box_end(Ctx::Shift(0), Some(idx));
    }

    /// §1041 `\ `: interword glue at space factor 1000, the space factor kept.
    pub(super) fn build_control_space(&mut self) {
        let sf = std::mem::replace(&mut self.aux, 1000);
        self.append_space();
        self.aux = sf;
    }
}
