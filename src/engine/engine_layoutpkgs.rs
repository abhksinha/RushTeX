//! Small layout packages on the GDL path: xspace (2014/10/28 v1.13), and the hooks
//! indentfirst (2023/07/02 v1.03) and placeins (2005/04/18 v2.2) need elsewhere.
//!
//! xspace's `\xspace` is `\futurelet\@let@token\@xspace`: a letter next gives `\space`; one
//! of `\@xspace@exceptions@tlp` (`,.'/?;:!~-)`, `\ `, `\/`, `\bgroup`, `\egroup`, a space,
//! `\space`, `\footnote`, `\footnotemark`) gives nothing; any other unexpandable token gives
//! `\space`; an expandable one is expanded once and looked at again, which the native job
//! does not follow (a command other than those, an active character but `~`: declined).
//!
//! placeins' `\FloatBarrier` (no options): `\par`; with a float on `\@botlist` or
//! `\@deferlist`, `\newpage` and `\FloatBarrier` once more, whose floats still held give
//! `\clearpage`; then `\@fb@topbarrier`, `\suppressfloats[t]` (`\global\@topnum\z@`).
//! Two columns decline.
//!
//! indentfirst is `\let\@afterindentfalse\@afterindenttrue`: no heading drops the first
//! paragraph's indent (`Doc::afterindent` is read as true, `rt@afterheadingpar`).
//!
//! fullpage (1999/02/23 v1.1) sets lengths only: `\ExecuteOptions{in,plain}` then the
//! document's options in their order (`\ProcessOptions*`), each style option a `\pagestyle`;
//! `\ifFP@plain` zeroes `\headheight` and `\headsep`, `\ifFP@empty` `\footskip`; then
//! `\textwidth` is `\paperwidth` less two margins, `\oddsidemargin` and `\evensidemargin`
//! the margin less 1in, `\textheight` `\paperheight` less the head, the foot and two
//! margins, `\topmargin` the margin less 1in. `\FP@margin` is a `\newlength` no native
//! document can read (GDL R9).

use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::{Reg, Region};
use super::super::engine_run::Machine;
use super::super::engine_token::Token;

pub(super) const NATIVES: &[(&str, super::Native)] = &[("rt@xspace", |m| m.xspace()), ("rt@floatbarrier", |m| m.float_barrier()), ("rt@fbheld", |m| m.float_barrier_held(false)), ("rt@fbagain", |m| m.float_barrier_held(true)), ("rt@fbtop", |m| m.float_barrier_top())];

/// What `\@xspace` does before token `t` (a character's category and code, or a command's
/// name): `Some(true)` a space, `Some(false)` nothing, `None` not followed natively.
fn xspace_before(cat: Option<Cat>, c: char, name: Option<&str>) -> Option<bool> {
    match (cat, name) {
        (Some(Cat::Letter), _) => Some(true),
        (Some(Cat::Other), _) => Some(!",.'/?;:!-)".contains(c)),
        (Some(Cat::BeginGroup | Cat::EndGroup | Cat::Space), _) => Some(false),
        (Some(Cat::Active), _) => (c == '~').then_some(false),
        (Some(Cat::MathShift | Cat::AlignTab | Cat::Superscript | Cat::Subscript | Cat::Param), _) => Some(true),
        (None, Some(" " | "/" | "bgroup" | "egroup" | "space" | "footnote" | "footnotemark")) => Some(false),
        // `\par` is a primitive: not expandable, not an exception.
        // `\begin` is `\protected`; `\end` expands (`\romannumeral`) to the `\protected` `\end `.
        (None, Some("par" | "begin" | "end")) => Some(true),
        // The native job reads `\end{env}` as its `\endenv`.
        (None, Some(n)) if n.starts_with("end") => Some(true),
        _ => None,
    }
}

/// fullpage's options (fullpage.sty's `\DeclareOption`s).
pub(super) fn fullpage_options_ok(opts: &str) -> bool {
    opts.split(',').map(str::trim).all(|o| matches!(o, "" | "in" | "cm" | "plain" | "empty" | "headings" | "myheadings"))
}

/// 1in and 1.5cm as TeX scans them (§458: `xn_over_d` by 7227/100 and by 7227/254).
const ONE_IN: i32 = 4736286;
const CM_15: i32 = 2797019;

impl Machine<'_, '_> {
    /// `\usepackage[opts]{fullpage}`, after the class (whose paper `\paperwidth` is), and not
    /// after geometry (its `\Gm@process` was at its load), nor under KOMA's typearea or memoir.
    pub(super) fn fullpage_load(&mut self, opts: &str) {
        let Some(doc) = self.doc.as_ref() else { return };
        if !fullpage_options_ok(opts) || doc.geometry.is_some() || doc.koma || doc.memoir || doc.pkg.a0poster {
            return self.not_native("fullpage with this option or class, or after geometry");
        }
        let [pw, ph] = super::PAPER_SIZES[(doc.options & 7) as usize];
        let [pw, ph] = if doc.options & 8 != 0 { [ph, pw] } else { [pw, ph] };
        self.load_package("fullpage");
        let (mut margin, mut plain, mut empty) = (ONE_IN, true, false);
        for o in ["in", "plain"].into_iter().chain(opts.split(',').map(str::trim).filter(|o| !o.is_empty())) {
            match o {
                "in" => margin = ONE_IN,
                "cm" => margin = CM_15,
                _ => {
                    (plain, empty) = (matches!(o, "plain" | "empty"), o == "empty");
                    self.set_page_style(o);
                }
            }
        }
        let reg = |i: u32| Reg { region: Region::Dimen, slot: super::GEOMETRY + i };
        if plain {
            self.e.eqtb.set_word(reg(5), 0, false);
            self.e.eqtb.set_word(reg(6), 0, false);
        }
        if empty {
            self.e.eqtb.set_word(reg(7), 0, false);
        }
        let [head, sep, foot] = [5, 6, 7].map(|i| self.e.eqtb.word(reg(i)));
        let side = margin - ONE_IN;
        for (i, v) in [(0, pw - 2 * margin), (2, side), (3, side), (1, ph - head - sep - foot - 2 * margin), (4, side)] {
            self.e.eqtb.set_word(reg(i), v, false);
        }
    }

    /// `\FloatBarrier`:`\par`, then `rt@fbheld`.
    fn float_barrier(&mut self) {
        let t = [self.named_cs(named!("par")), self.doc_id(doc!("rt@fbheld"))];
        self.back_list(&t);
    }

    /// After the `\par`: nothing held gives `\@fb@topbarrier`; else the first time
    /// `\newpage\FloatBarrier` (with `\@fltovf` `\relax`), the second `\clearpage`.
    fn float_barrier_held(&mut self, again: bool) {
        let Some(held) = self.doc.as_ref().and_then(|d| d.floats.held()) else { return self.not_native("\\FloatBarrier in two columns") };
        if !held {
            return self.float_barrier_top();
        }
        let t = if again {
            vec![self.doc_id(doc!("clearpage")), self.doc_id(doc!("rt@fbtop"))]
        } else {
            vec![self.doc_id(doc!("newpage")), self.named_cs(named!("par")), self.doc_id(doc!("rt@fbagain")), self.doc_id(doc!("rt@fbtop"))]
        };
        self.back_list(&t);
    }

    /// `\@fb@topbarrier`: `\suppressfloats[t]`.
    fn float_barrier_top(&mut self) {
        self.fl().topnum = 0;
    }

    /// `\xspace`: the next token looked at, not taken (`\futurelet`), and `\space` (a space
    /// token) before it when `\@xspace` would give one.
    fn xspace(&mut self) {
        let Some(t) = self.get_token() else { return };
        self.input.back(t);
        let name = t.cs_id().map(|id| self.e.cs.name(id).to_string());
        match xspace_before(t.cat(), t.chr(), name.as_deref()) {
            Some(true) => self.input.back(Token::char(Cat::Space, ' ')),
            Some(false) => {}
            None => self.not_native(&format!("\\xspace before a command it would expand ({})", name.as_deref().unwrap_or("an active character"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xspace_exceptions() {
        assert_eq!(xspace_before(Some(Cat::Letter), 'a', None), Some(true));
        assert_eq!(xspace_before(Some(Cat::Other), '.', None), Some(false));
        assert_eq!(xspace_before(Some(Cat::Other), '(', None), Some(true));
        assert_eq!(xspace_before(Some(Cat::Active), '~', None), Some(false));
        assert_eq!(xspace_before(None, '\0', Some("footnote")), Some(false));
        assert_eq!(xspace_before(None, '\0', Some("textbf")), None);
    }
}
