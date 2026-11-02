//! colortbl.sty 1.0k on the GDL path, beyond the entry boxes of `engine_tabular`
//! ([`super::tabular::Cell::CtOpen`], `ct_close`) and `\rowcolors`: `\rowcolor`, `\cellcolor`,
//! `\columncolor` and `\arrayrulecolor`. Each sets the colour a `\gdef` would (`\CT@row@color`,
//! `\CT@cell@color`, `\CT@column@color`, `\CT@arc@`); the entry's end and the rules read it.
//! Only a colour name (a `\definecolor` one, a predefined one or an xcolor mix) is native: the
//! `[model]` forms and the overhangs `[l][r]` decline.

use super::super::engine_align::NO_ALIGN_GROUP;
use super::super::engine_run::{Machine, SIMPLE_GROUP};

impl Machine<'_, '_> {
    /// A colour argument that is a name: `None` (declined) for commands or a `[model]`.
    fn ct_name(&mut self) -> Option<String> {
        if self.next_is('[') {
            self.not_native("colortbl: a colour with a model");
            return None;
        }
        let mut t = Vec::new();
        self.read_arg(&mut t);
        let name = t.iter().map(|x| x.cs_id().is_none().then(|| x.chr())).collect::<Option<String>>().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        if name.is_none() {
            self.not_native("colortbl: a colour with commands");
        }
        name
    }

    /// The innermost alignment is a `tabular` whose `\everycr` is colortbl's (not nested in
    /// an entry, not an `array`, not a longtable).
    fn ct_outer(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.tabs.len() == 1 && !d.tabs[0].math) && !self.in_longtable()
    }

    /// `\color{c}` inside a group colortbl opened: `\current@color` (local) and `\set@color`'s
    /// push; the pop is the caller's after the group (`\reset@color` by `\aftergroup`).
    pub(super) fn ct_push(&mut self, spec: &str) -> bool {
        let Some(lit) = self.color_literal(None, spec, false) else {
            self.not_native("a colour the native job does not know");
            return false;
        };
        let Some(doc) = self.doc.as_mut() else { return false };
        let i = match doc.colors.iter().position(|c| *c == lit) {
            Some(i) => i,
            None => {
                doc.colors.push(lit.clone());
                doc.colors.len() - 1
            }
        };
        self.e.eqtb.set_word(super::CURRENT_COLOR, i as i32 + 1, false);
        self.color_stack(Some(&lit));
        true
    }

    /// A rule colortbl draws as `{\CT@arc@ <rule>}` (`\hline`, `|`, `\cline`, booktabs'):
    /// with `\arrayrulecolor` in force the group, the colour's push, the rule, the pop.
    pub(super) fn ct_arc_rule(&mut self, rule: impl FnOnce(&mut Self)) {
        let Some(c) = self.doc.as_ref().and_then(|d| d.pkg.ct.arc.clone()) else { return rule(self) };
        self.begin_group(SIMPLE_GROUP);
        if !self.ct_push(&c) {
            return self.end_group(SIMPLE_GROUP);
        }
        rule(self);
        self.end_group(SIMPLE_GROUP);
        self.color_stack(None);
    }

    /// `\rowcolor{c}` at a row's start: `\noalign{\ifnum0=`}\fi \global\let\CT@do@color
    /// \CT@@do@color`, `\CT@rowb` (`\gdef\CT@row@color{\CT@color{c}}`), `\CT@rowc`'s
    /// `\@ifnextchar[` (spaces passed) and the `}`. The row's entries then draw it
    /// (`ct_close`); the next `\cr`'s `\CT@everycr` clears it.
    pub(super) fn row_color(&mut self) {
        if !self.ct_outer() {
            return self.not_native("\\rowcolor in a nested tabular, an array or a longtable");
        }
        self.begin_group(NO_ALIGN_GROUP);
        self.normal_paragraph();
        let Some(c) = self.ct_name() else { return };
        if self.next_is('[') {
            return self.not_native("\\rowcolor with overhangs");
        }
        if let Some(d) = self.doc.as_mut() {
            d.pkg.ct.row = Some(c);
        }
        self.end_no_align();
    }

    /// `\cellcolor{c}` in an entry: `\gdef\CT@cell@color{\CT@color{c}\global\let
    /// \CT@cell@color\relax}`, which the entry's end draws once. `\CT@start` clears it and
    /// `\CT@end` puts it back, so it belongs to the innermost tabular.
    pub(super) fn cell_color(&mut self) {
        if self.doc.as_ref().and_then(|d| d.tabs.last()).is_none_or(|t| t.math) || self.in_longtable() {
            return self.not_native("\\cellcolor outside a tabular entry");
        }
        let Some(c) = self.ct_name() else { return };
        if let Some(t) = self.doc.as_mut().and_then(|d| d.tabs.last_mut()) {
            t.ct_cell = Some(c);
        }
    }

    /// `\columncolor{c}` in a column's `>{...}`: `\@classz`'s `\CT@extract` takes it out of the
    /// tokens (spaces after it go with `\CT@extractd`'s `\@testopt`) into `\CT@column@color`,
    /// which the entry's end draws. Read here where the tokens run, it has no other effect,
    /// so it is the entry's column colour.
    pub(super) fn column_color(&mut self) {
        if self.doc.as_ref().and_then(|d| d.tabs.last()).is_none_or(|t| t.math) || self.in_longtable() {
            return self.not_native("\\columncolor outside a tabular column");
        }
        let Some(c) = self.ct_name() else { return };
        if self.next_is('[') {
            return self.not_native("\\columncolor with overhangs");
        }
        if let Some(t) = self.doc.as_mut().and_then(|d| d.tabs.last_mut()) {
            t.ct_col = Some(c);
        }
    }

    /// `\arrayrulecolor{c}` outside a tabular: `\CT@arc`'s `{\gdef\CT@arc@{\color{c}}}` (an
    /// empty group adds nothing). In an entry `\baselineskip` may be 0 (its `\noalign`) or not:
    /// declined.
    pub(super) fn arrayrule_color(&mut self) {
        if self.doc.as_ref().is_some_and(|d| !d.tabs.is_empty()) {
            return self.not_native("\\arrayrulecolor inside a tabular entry");
        }
        if self.glue_param(super::super::engine_pack::BASELINE_SKIP).width == 0 {
            return self.not_native("\\arrayrulecolor with \\baselineskip 0");
        }
        if let Some(c) = self.ct_name() {
            if let Some(d) = self.doc.as_mut() {
                d.pkg.ct.arc = Some(c);
            }
        }
    }

    /// `\arrayrulecolor{c}` at a row's start: `\@array`'s `\baselineskip\z@` makes `\CT@arc`
    /// `\noalign{\gdef\CT@arc@{\color{c}}}`; `\CT@end` puts the old one back.
    pub(super) fn arrayrule_color_row(&mut self) {
        if self.in_longtable() || self.doc.as_ref().and_then(|d| d.tabs.last()).is_none_or(|t| t.math) {
            return self.not_native("\\arrayrulecolor in an array or a longtable");
        }
        self.begin_group(NO_ALIGN_GROUP);
        self.normal_paragraph();
        let Some(c) = self.ct_name() else { return };
        if let Some(d) = self.doc.as_mut() {
            d.pkg.ct.arc = Some(c);
        }
        self.end_no_align();
    }
}

/// colortbl's state (`Packages::ct`).
#[derive(Default, Clone, PartialEq)]
pub(super) struct Colortbl {
    /// Each `\rowcolors{first}{odd}{even}` (`ROWCOLORS` names one), `\rownum`, and
    /// `\CT@row@color` (the row's colour, global).
    pub(super) specs: Vec<(i32, Option<String>, Option<String>)>,
    pub(super) num: i32,
    pub(super) row: Option<String>,
    /// `\CT@arc@`: the `\arrayrulecolor` of the rules (global).
    pub(super) arc: Option<String>,
}
