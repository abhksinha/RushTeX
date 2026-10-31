//! The kernel's `eqnarray` and `eqnarray*` (latex.ltx `\eqnarray`, `\@eqncr`, `\@@eqncr`,
//! `\endeqnarray`), on the engine's own `\halign`.
//!
//! The alignment is `\halign to\displaywidth` with four fixed columns and fixed tabskip
//! glue, so unlike amsmath's `align` nothing waits for later rows: the rows are set as they
//! come. The preamble is built, not scanned: each column's templates are one native
//! (`\rt@eqau`, `\rt@eqav`) around the brace of its formula, and the tabskip glue the
//! kernel's preamble assigns is given as the alignment's (`init_align_cols`). The other
//! natives are `\\` with its penalty and space, and each row's end with its number.

use super::super::engine_box::{fil_glue, Ctx, HBOX_GROUP};
use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::Glue;
use super::super::engine_noad::DISPLAY_WIDTH;
use super::super::engine_prim::Prim;
use super::super::engine_run::{Machine, Mode, SEMI_SIMPLE_GROUP, SIMPLE_GROUP};
use super::super::engine_token::Token;
use super::super::END_TEMPLATE_ID;
use super::{EQ_ROW, R_ARRAYCOLSEP};

/// latex.ltx's `\jot`.
const JOT: i32 = 3 * 65536;

/// `\@centering`: `0pt plus1000pt`.
const CENTERING: Glue = Glue { width: 0, stretch: 1000 << 16, shrink: 0, stretch_order: 0, shrink_order: 0 };

/// One `eqnarray` being read.
#[derive(PartialEq, Clone)]
pub(super) struct EqnArray {
    /// `eqnarray*`: `\@eqncr` and `\endeqnarray` begin with `\nonumber`.
    starred: bool,
    /// `\@eqcnt`: the column the row has reached (0 to 3).
    eqcnt: u8,
    /// `\if@eqnsw`: the row takes a number.
    pub(super) eqnsw: bool,
    /// Open `tabular`s and arrays when it began: a `\\` inside one of theirs is theirs.
    tabs: usize,
    /// The `\noalign` after the last `\\`: `\@eqpen` and the optional space.
    gap: (i32, i32),
}

impl Machine<'_, '_> {
    /// `\begin{eqnarray}`: `\stepcounter{equation}` (`\@currentlabel` its value),
    /// `\global\@eqnswtrue \m@th \global\@eqcnt\z@ \tabskip\@centering \let\\\@eqncr
    /// \dollardollar@begin \everycr{}`, then `\halign to\displaywidth` ([`Self::eqnarray_halign`]).
    /// `\tabskip` is not set: the preamble the native builds reads nothing of it.
    pub(super) fn eqnarray_begin(&mut self, starred: bool) {
        if self.mode == Mode::Math {
            return self.not_native("an eqnarray inside math");
        }
        self.begin_group(SEMI_SIMPLE_GROUP);
        let Some(doc) = self.doc.as_mut() else { return };
        doc.endpe = false;
        if doc.eqnarray.is_some() {
            return self.not_native("an eqnarray inside an eqnarray");
        }
        let tabs = doc.tabs.len();
        doc.eqnarray = Some(EqnArray { starred, eqcnt: 0, eqnsw: true, tabs, gap: (0, 0) });
        self.equation_step();
        {
            let reg = param!("everycr");
            let span = self.e.toks.store(&[]);
            self.e.eqtb.set_toks(reg, span, false);
        }
        self.math_surround_zero();
        let dollar = Token::char(Cat::MathShift, '$');
        let t = [dollar, dollar, self.doc_id(doc!("rt@eqahalign"))];
        self.back_list(&t);
    }

    /// In the display, `\halign to\displaywidth` with the kernel's four columns:
    /// `\hskip\@centering$\displaystyle\tabskip\z@skip{##}$\@eqnsel &\global\@eqcnt\@ne
    /// \hskip\tw@\arraycolsep\hfil${##}$\hfil &\global\@eqcnt\tw@\hskip\tw@\arraycolsep
    /// $\displaystyle{##}$\hfil\tabskip\@centering &\global\@eqcnt\thr@@\hb@xt@\z@\bgroup
    /// \hss##\egroup\tabskip\z@skip\cr`.
    pub(super) fn eqnarray_halign(&mut self) {
        let (u, v) = (self.doc_id(doc!("rt@eqau")), self.doc_id(doc!("rt@eqav")));
        let (open, close, end) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'), Token::cs(END_TEMPLATE_ID));
        let math = (self.e.toks.store(&[u, open]), self.e.toks.store(&[close, v, end]));
        let number = (self.e.toks.store(&[u]), self.e.toks.store(&[v, end]));
        let z = Glue::default();
        let w = self.dimen_param(DISPLAY_WIDTH);
        self.init_align_cols(&[math, math, math, number], vec![CENTERING, z, z, CENTERING, z], true, w);
    }

    /// A column's template before its entry.
    pub(super) fn eqnarray_u(&mut self) {
        let col = self.align_column();
        if let Some(e) = self.doc.as_mut().and_then(|d| d.eqnarray.as_mut()) {
            e.eqcnt = col as u8;
        }
        let hfil = |m: &mut Self| m.build_glue(fil_glue(Prim::Hfil), false);
        let sep = 2 * self.rw(R_ARRAYCOLSEP);
        match col {
            0 => {
                self.build_glue(CENTERING, false);
                self.enter_text_math();
                self.build_style(0);
            }
            1 => {
                self.build_glue(Glue { width: sep, ..Glue::default() }, true);
                hfil(self);
                self.enter_text_math();
            }
            2 => {
                self.build_glue(Glue { width: sep, ..Glue::default() }, true);
                self.enter_text_math();
                self.build_style(0);
            }
            _ => {
                self.open_box(HBOX_GROUP, Ctx::Shift(0), true, 0, false);
                self.build_glue(fil_glue(Prim::Hss), false);
            }
        }
    }

    /// A column's template after its entry.
    pub(super) fn eqnarray_v(&mut self) {
        match self.align_column() {
            0 => self.after_math(),
            1 | 2 => {
                self.after_math();
                self.build_glue(fil_glue(Prim::Hfil), false);
            }
            _ => self.package(),
        }
    }

    /// Whether `\\` is the `\@eqncr` of an `eqnarray`.
    pub(super) fn in_eqnarray(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.eqnarray.as_ref().is_some_and(|e| e.tabs == d.tabs.len()))
    }

    /// `\@eqncr`: `{\ifnum0=`}\fi`, `\@ifstar` (`\@eqpen` `\@M`, else
    /// `\interdisplaylinepenalty`), `\@testopt` for the space; then `\@@eqncr` and
    /// `\noalign{\penalty\@eqpen\vskip\jot\vskip<space>}` ([`Self::eqnarray_gap`]).
    pub(super) fn eqnarray_cr(&mut self) {
        self.input.align_state += 1;
        let star = self.next_is('*');
        let skip = if self.next_is('[') { Some(self.bracket_arg()) } else { None };
        self.input.align_state -= 1;
        let skip = match skip {
            Some(arg) => match self.glue_arg(&arg) {
                Some(g) if g.stretch == 0 && g.shrink == 0 => g.width,
                Some(_) => return self.not_native("\\\\[...] in eqnarray with stretch"),
                None => return self.not_native("\\\\[...] in eqnarray that is not a length"),
            },
            None => 0,
        };
        // latex.ltx's `\interdisplaylinepenalty` is 100; amsmath sets it to `\@M`.
        let pen = if star { 10000 } else if self.ams() { self.display_pen() } else { 100 };
        if let Some(d) = self.doc.as_mut() {
            d.eqpen = pen;
        }
        if let Some(e) = self.doc.as_mut().and_then(|d| d.eqnarray.as_mut()) {
            e.gap = (pen, skip);
        }
        // The brace pair around the look-ahead: in a formula entry, an empty group, `{}`.
        let mut t = vec![Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}')];
        self.eqnarray_row(&mut t);
        t.push(self.doc_id(doc!("rt@eqgap")));
        self.back_list(&t);
    }

    /// The `\noalign` after a row: `\penalty\@eqpen \vskip\jot \vskip<space>`.
    pub(super) fn eqnarray_gap(&mut self) {
        let Some((pen, skip)) = self.doc.as_ref().and_then(|d| d.eqnarray.as_ref()).map(|e| e.gap) else { return };
        self.begin_group(super::super::engine_align::NO_ALIGN_GROUP);
        self.normal_paragraph();
        self.build_penalty(pen);
        self.build_glue(Glue { width: JOT, ..Glue::default() }, true);
        self.build_glue(Glue { width: skip, ..Glue::default() }, true);
        self.end_no_align();
    }

    /// `\endeqnarray`: `\@@eqncr \egroup \global\advance\c@equation\m@ne
    /// \dollardollar@end \@ignoretrue`, then `\end`'s group.
    pub(super) fn eqnarray_end(&mut self) {
        let mut t = Vec::new();
        self.eqnarray_row(&mut t);
        let dollar = Token::char(Cat::MathShift, '$');
        t.extend([Token::char(Cat::EndGroup, '}'), dollar, dollar, self.doc_id(doc!("rt@endeqnarray"))]);
        self.back_list(&t);
    }

    /// After the display: `\global\advance\c@equation\m@ne` (which nothing between reads),
    /// the `eqnarray` closed, then `\@ignoretrue` and `\end`'s group.
    pub(super) fn eqnarray_close(&mut self) {
        if let Some(doc) = self.doc.as_mut() {
            doc.equation -= 1;
            doc.eqnarray = None;
        }
        self.end_equation();
    }

    /// `\@@eqncr`: the `&`s up to the last column, `\@eqnnum\stepcounter{equation}` when the
    /// row is numbered ([`Self::eqnarray_number`]), `\global\@eqnswtrue\global\@eqcnt\z@\cr`.
    fn eqnarray_row(&mut self, t: &mut Vec<Token>) {
        let Some(e) = self.doc.as_ref().and_then(|d| d.eqnarray.as_ref()) else { return };
        let (cnt, numbered) = (e.eqcnt, e.eqnsw && !e.starred);
        for _ in cnt..3 {
            t.push(Token::char(Cat::AlignTab, '&'));
        }
        if numbered {
            t.push(self.doc_id(doc!("rt@eqnum")));
        }
        let cr = self.doc.as_ref().map_or(0, |d| d.cr_id);
        t.extend([self.doc_id(EQ_ROW), Token::cs(cr)]);
    }

    /// `\@eqnnum`: `{\normalfont \normalcolor (\theequation)}`; amsmath's `{\normalfont
    /// \normalcolor \tagform@\theequation}` (`\hbox{\m@th\normalfont(n\/)}`, where nothing
    /// reads `\mathsurround`); then `\stepcounter{equation}`.
    pub(super) fn eqnarray_number(&mut self) {
        let n = self.equation_text();
        let colored = self.colored();
        self.begin_group(SIMPLE_GROUP);
        self.normal_font();
        if colored {
            self.color_stack(Some(super::DEFAULT_COLOR));
        }
        if self.ams() {
            self.build_hbox(|m| m.equation_tag(&n));
        } else {
            self.eqnnum_text(&n);
        }
        self.end_group(SIMPLE_GROUP);
        if colored {
            self.color_stack(None);
        }
        self.equation_step();
    }

    /// `(n\/)` of amsmath's `\tagform@`; microtype's `eqnum` patch makes the parentheses
    /// `\leftprotrusion{(}` and `\rightprotrusion{)}`.
    pub(super) fn equation_tag(&mut self, n: &str) {
        self.protrusion_kern(b'(', false);
        self.build_text(format!("({n}").as_bytes());
        self.append_italic_correction();
        self.build_text(b")");
        self.protrusion_kern(b')', true);
    }

    /// `(n)` of `\@eqnnum`, patched as [`Self::equation_tag`] under microtype (where `)`
    /// is a word of its own: `\rightprotrusion` ends the one before it).
    pub(super) fn eqnnum_text(&mut self, n: &str) {
        if !self.microtype() {
            return self.build_text(format!("({n})").as_bytes());
        }
        self.protrusion_kern(b'(', false);
        self.build_text(format!("({n}").as_bytes());
        self.build_text(b")");
        self.protrusion_kern(b')', true);
    }

    /// `\global\@eqnswtrue\global\@eqcnt\z@`.
    pub(super) fn eqnarray_row_reset(&mut self) {
        if let Some(e) = self.doc.as_mut().and_then(|d| d.eqnarray.as_mut()) {
            e.eqnsw = true;
            e.eqcnt = 0;
        }
    }

    /// `\lefteqn{...}`: `\hbox to\z@{$\displaystyle ...$\hss}`, its end
    /// ([`Self::left_eqn_end`]) after the argument.
    pub(super) fn left_eqn(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        self.open_box(HBOX_GROUP, Ctx::Shift(0), true, 0, false);
        self.enter_text_math();
        self.build_style(0);
        arg.push(self.doc_id(doc!("rt@lefteqnend")));
        self.back_list(&arg);
    }

    pub(super) fn left_eqn_end(&mut self) {
        self.after_math();
        self.build_glue(fil_glue(Prim::Hss), false);
        self.package();
    }
}
