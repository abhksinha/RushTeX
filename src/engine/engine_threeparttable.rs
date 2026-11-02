//! threeparttable.sty (v3.0, 2003/06/13) on the GDL path: a `threeparttable` holding one
//! `tabular`, then `tablenotes`, with `\tnote` marks; the caption outside it (a `\caption`
//! inside, which the package holds until the table is measured, goes to the engine).
//!
//! `\threeparttable` is `\relax \ifvmode \noindent \fi \TPT@common`, which opens a `\vtop`
//! with `\parindent\z@ \color@begingroup \topsep\z@ \@enumdepth\z@`. The `tabular` hook
//! (`\TPT@begintabbox`) puts the table in `\begingroup \setbox\@tempboxb\hbox\bgroup` and,
//! when it ends, `\TPT@endtabbox` measures it: `\hsize` is its width (at least
//! `\TPTminimum`, 4em), `\@parboxrestore`, the box, `\prevdepth\z@`, `\endgroup`, the width
//! again (`\TPT@hsize`) and, after `\end`'s group, again with `\par` (`\TPT@sethsize`).
//! With nothing before the table in the `\vtop`, the box goes straight to the list: it is
//! the first item, so no interline glue depends on the settings made before it.

use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::Glue;
use super::super::engine_node::Kind;
use super::super::engine_run::{Machine, Mode, SEMI_SIMPLE_GROUP};
use super::super::engine_token::Token;
use super::{List, Make, Label, Op, R_ENUMDEPTH, R_ITEMINDENT, R_ITEMSEP, R_LABELSEP, R_LABELWIDTH, R_LEFTMARGIN, R_PARSEP, R_PARTOPSEP, R_TABCOLSEP, R_TOPSEP, STEP};

/// Where a `threeparttable` is: 0 outside one, 1 before its table, 2 in it, 3 after it
/// (`\TPToverlap` is `\relax` at 1 and 3, `\rlap` at 2, gobbles its argument at 0).
pub(super) const BEFORE: u8 = 1;
pub(super) const IN_TABLE: u8 = 2;
pub(super) const AFTER: u8 = 3;

pub(super) const NATIVES: &[(&str, super::Native)] = &[("tnote", |m| m.tpt_tnote()), ("rt@tptcapend", |m| m.tpt_caption_end())];

impl Machine<'_, '_> {
    pub(super) fn tpt_phase(&self) -> u8 {
        self.doc.as_ref().map_or(0, |d| d.pkg.tpt)
    }

    fn set_tpt_phase(&mut self, p: u8) {
        if let Some(d) = self.doc.as_mut() {
            d.pkg.tpt = p;
        }
    }

    /// Tokens, then `then` (a [`STEP`]).
    fn tpt_then(&mut self, mut t: Vec<Token>, then: super::Native) {
        if let Some(d) = self.doc.as_mut() {
            d.steps.push(vec![Op::Call(then)]);
        }
        t.push(self.doc_id(STEP));
        self.back_list(&t);
    }

    /// `\begin{threeparttable}[t]` after `\begin`'s group: `\noindent` in vertical mode,
    /// then `\vtop\bgroup` and the rest of `\TPT@common`. `[b]` and `[c]` decline.
    pub(super) fn tpt_begin(&mut self) {
        if let Some(o) = self.next_is('[').then(|| self.bracket_arg()) {
            if o.len() != 1 || o[0].cs_id().is_some() || o[0].chr() != 't' {
                return self.not_native("a threeparttable at the bottom or centre");
            }
        }
        if self.in_tabular() || self.tpt_phase() != 0 {
            return self.not_native("a threeparttable in a table");
        }
        // caption's `\threeparttable` (caption.sty:1415): `\caption@setanchor`, with
        // hyperref `hypcap`'s anchor here.
        if self.caption_pkg() && self.hyper() {
            return self.not_native("caption: a threeparttable with hyperref");
        }
        let mut t = Vec::new();
        if self.mode == Mode::Vertical {
            t.push(self.named_cs(named!("noindent")));
        }
        t.extend([self.named_cs(named!("vtop")), Token::char(Cat::BeginGroup, '{')]);
        self.tpt_then(t, |m| {
            m.set_dimen_named(param!("parindent"), 0);
            if m.colored() {
                m.begin_group(SEMI_SIMPLE_GROUP);
            }
            m.set_rg(R_TOPSEP, Glue::default());
            m.set_rw(R_ENUMDEPTH, 0, false);
            m.set_tpt_phase(BEFORE);
        });
    }

    /// `\begin{tabular}` in a `threeparttable` (`\TPT@begintabbox`), after `\begin`'s
    /// group: `\begingroup \setbox\@tempboxb\hbox\bgroup`, the box appended when it ends
    /// ([`Self::tpt_end_table`]); `None` when the native path does not take it (a second
    /// table, or anything already in the `\vtop`).
    pub(super) fn tpt_table_open(&mut self) -> Option<Vec<Token>> {
        if self.tpt_phase() != BEFORE || self.mode != Mode::Vertical || self.cur.len() > self.start as usize {
            return None;
        }
        self.set_tpt_phase(IN_TABLE);
        Some(self.tex_tokens("\\begingroup\\hbox{"))
    }

    /// `\TPT@caption`'s text: `\strut\ignorespaces#3\ifhmode\unskip\@finalstrut\strutbox\fi`.
    pub(super) fn tpt_caption_text(&mut self, text: Vec<Token>) -> Vec<Token> {
        let mut t = vec![self.doc_id(doc!("strut")), self.named_cs(named!("ignorespaces"))];
        t.extend(text);
        t.push(self.doc_id(doc!("rt@tptcapend")));
        t
    }

    /// `\ifhmode\unskip\@finalstrut\strutbox\fi`: `\unskip \nobreak \vrule\@width\z@
    /// \@height\z@ \@depth\dp\strutbox`.
    fn tpt_caption_end(&mut self) {
        if self.mode != Mode::Horizontal {
            return;
        }
        self.unskip();
        self.unskip();
        self.build_penalty(10000);
        let dp = self.strut_depth();
        self.build_rule([0, 0, dp]);
    }

    /// `\TPT@caption` before the table: `\gdef\TPT@docapt{...}`, `\let\label
    /// \TPT@gatherlabel \abovecaptionskip\z@skip`, `\ignorespaces`.
    pub(super) fn tpt_hold_caption(&mut self, text: Vec<Token>) {
        let Some(d) = self.doc.as_mut() else { return };
        if d.pkg.tpt_caption.is_some() {
            return self.not_native("two captions in a threeparttable");
        }
        d.pkg.tpt_caption = Some(text);
        self.set_rg(super::R_ABOVECAPTIONSKIP, Glue::default());
        let t = self.named_cs(named!("ignorespaces"));
        self.input.back(t);
    }

    pub(super) fn tpt_holds_caption(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.pkg.tpt == BEFORE && d.pkg.tpt_caption.is_some())
    }

    /// `\TPT@gatherlabel`: `\g@addto@macro\TPT@docapt{\label{#1}}`.
    pub(super) fn tpt_gather_label(&mut self, key: Vec<Token>) {
        let label = self.doc_id(doc!("label"));
        let Some(d) = self.doc.as_mut() else { return };
        d.pkg.tpt_labels.push(label);
        d.pkg.tpt_labels.push(Token::char(Cat::BeginGroup, '{'));
        d.pkg.tpt_labels.extend(key);
        d.pkg.tpt_labels.push(Token::char(Cat::EndGroup, '}'));
    }

    /// After the table's alignment and its own box: the `\hbox`'s `\egroup`, then
    /// `\TPT@endtabbox`; then `\end`'s `\endgroup`, `\TPT@sethsize` and `\@ignoretrue`.
    /// A held caption (`\TPT@docapt`, then `\vskip.2\baselineskip \par`) goes before the
    /// box, which then comes after it as `\box\@tempboxb` would.
    pub(super) fn tpt_table_close(&mut self) {
        let t = vec![Token::char(Cat::EndGroup, '}')];
        self.tpt_then(t, |m| {
            let Some(b) = m.cur.last().filter(|n| n.kind == Kind::Box && m.cur.len() == m.start as usize + 1).map(|n| n.a) else { return m.not_native("a threeparttable without its table box") };
            let w = m.e.nodes.boxes[b as usize].width;
            let (em, _) = m.em_ex();
            let h = w.max(4 * em as i32);
            let Some(d) = m.doc.as_mut() else { return };
            d.pkg.tpt_hsize = h;
            d.pkg.tpt = AFTER;
            let Some(caption) = d.pkg.tpt_caption.take() else { return m.tpt_table_end() };
            let labels = std::mem::take(&mut d.pkg.tpt_labels);
            d.pkg.tpt_box = b as u32;
            m.cur.pop();
            m.aux = super::super::engine_node::IGNORE_DEPTH;
            m.tpt_hsize();
            m.tpt_then(labels, |m| {
                let g = m.glue_param(super::super::engine_pack::BASELINE_SKIP).width;
                let g = crate::backend::scaled::xn_over_d(g, 13107, 0x10000);
                m.build_glue(Glue { width: g, ..Glue::default() }, false);
                let b = m.doc.as_ref().map_or(0, |d| d.pkg.tpt_box);
                m.append_to_vlist(b);
                m.tpt_table_end();
            });
            m.fl().caption = caption;
            m.run_ops(vec![Op::Par, Op::Call(|m| m.caption_body())]);
        });
    }

    /// `\ifvmode \prevdepth\z@ \fi \endgroup`, `\TPT@hsize`; `\end`'s `\endgroup`,
    /// `\TPT@sethsize` and `\@ignoretrue`.
    fn tpt_table_end(&mut self) {
        let m = self;
        {
            // `\ifvmode \prevdepth\z@ \fi \endgroup`, `\TPT@hsize`.
            m.aux = 0;
            m.end_group(SEMI_SIMPLE_GROUP);
            m.tpt_hsize();
            // `\end`'s `\endgroup`, then `\TPT@sethsize` (`\TPT@hsize\par`); `\if@ignore`.
            m.end_group(SEMI_SIMPLE_GROUP);
            m.tpt_hsize();
            let t = [m.named_cs(named!("par")), m.named_cs(named!("ignorespaces"))];
            m.back_list(&t);
        }
    }

    /// `\TPT@hsize`: `\hsize` the table's width, `\@parboxrestore`.
    fn tpt_hsize(&mut self) {
        let h = self.doc.as_ref().map_or(0, |d| d.pkg.tpt_hsize);
        self.set_dimen_named(param!("hsize"), h);
        self.par_box_restore();
    }

    /// `\begin{tablenotes}` after `\begin`'s group: `\par \prevdepth\z@ \TPT@hsize`, then
    /// `\list{}{\topsep\z@skip \partopsep\z@skip \itemsep\z@ \parsep\z@ \itemindent\z@
    /// \TPTnoteSettings \let\makelabel\TPTnoteLabel}`, `\TPTnoteSettings` being
    /// `\leftmargin 1.5em \labelwidth .5em \labelsep .2em \rightskip\tabcolsep
    /// \leftskip\tabcolsep`. Options decline.
    pub(super) fn tpt_notes_begin(&mut self) {
        if self.tpt_phase() != AFTER || self.mode != Mode::Vertical {
            return self.not_native("tablenotes outside a threeparttable's end");
        }
        if self.next_is('[') {
            return self.not_native("tablenotes options");
        }
        self.aux = 0;
        self.tpt_hsize();
        let list = List { label: Label::Empty, make: Make::Tnote, numbered: false, counter: 0, centercr: false, auto_item: false, endpe_false: false };
        if !self.list_open(list) {
            return;
        }
        for r in [R_TOPSEP, R_PARTOPSEP, R_ITEMSEP, R_PARSEP] {
            self.set_rg(r, Glue::default());
        }
        self.set_rw(R_ITEMINDENT, 0, false);
        // `1.5em`, `.5em`, `.2em`: §453's fractions 32768 and 13107 of the quad.
        let (em, _) = self.em_ex();
        let em = em as i32;
        let frac = |f: i32| crate::backend::scaled::xn_over_d(em, f, 0x10000);
        self.set_rw(R_LEFTMARGIN, em + frac(32768), false);
        self.set_rw(R_LABELWIDTH, frac(32768), false);
        self.set_rw(R_LABELSEP, frac(13107), false);
        let sep = Glue { width: self.rw(R_TABCOLSEP), ..Glue::default() };
        self.set_glue_named(param!("rightskip"), sep);
        self.set_glue_named(param!("leftskip"), sep);
        self.trivlist(|m| m.list_tail());
    }

    /// `\end{threeparttable}`: `\TPT@close` (`\ifhmode \unskip \fi \color@endgroup
    /// \egroup`), then `\end`'s `\endgroup`.
    pub(super) fn tpt_end(&mut self) {
        if self.tpt_phase() != AFTER {
            return self.not_native("a threeparttable without a table");
        }
        if self.mode == Mode::Horizontal {
            self.unskip();
        }
        if self.colored() {
            self.end_group(SEMI_SIMPLE_GROUP);
        }
        self.set_tpt_phase(0);
        let t = [Token::char(Cat::EndGroup, '}'), self.named_cs(named!("endgroup"))];
        self.back_list(&t);
    }

    /// `\tnote{x}`: `\protect\TPToverlap{\textsuperscript{\TPTtagStyle{x}}}`.
    fn tpt_tnote(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let phase = self.tpt_phase();
        if phase == 0 {
            return;
        }
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = Vec::with_capacity(arg.len() + 6);
        if phase == IN_TABLE {
            t.push(self.doc_id(doc!("rlap")));
        }
        t.extend([open, self.doc_id(doc!("textsuperscript")), open]);
        t.extend(arg);
        t.extend([close, close]);
        self.back_list(&t);
    }
}
