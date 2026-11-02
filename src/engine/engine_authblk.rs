//! authblk.sty 1.3 (2001/02/27) on the GDL path, its footnote mode with explicit marks:
//! `\author[mark]{name}` and `\affil[mark]{text}`. A mark sets `\c@Maxaffil` to zero, so
//! `\@author` is `\AB@authlist\\[\affilsep]\AB@affillist` (the `\ifnum\value{affil}>
//! \value{Maxaffil}` branch), and authblk's `\maketitle` makes article's `\begin{tabular}[t]{c}`
//! around it a `center` environment (`\renewenvironment{tabular}[2][]{\begin{center}}
//! {\end{center}}` in a group around `\AB@maketitle`).
//!
//! What the lists hold when typeset (`\protect` is `\relax` then, and the `\relax`es change
//! nothing here):
//! - `\AB@authlist`: per author k of n, `\@nameuse{@sep k}\Authfont name\AB@authnote{mark}`,
//!   with `\@sep1` empty, `\@sep k` `\Authsep` (`, `) for 1 < k < n and the last `\Authand`
//!   (` and `) for two authors, else `\Authands` (`, and `): each `\author` call defines the
//!   separator of the one before (`\ifnum\value{authors}>1`) and its own (`>2`).
//!   `\AB@authnote{m}` is `\textsuperscript{\normalfont m}`, `\Authfont` `\normalfont`.
//! - `\AB@affillist`: per affiliation, `\Affilfont` (the first) or `\\\Affilfont`
//!   (`\AB@affilsepx`, the others), `\AB@affilnote{mark}\Affilfont text`.
//! - `\affilsep` is `1em` of the font in force when the package loads (`\setlength`).
//!
//! Anything else (an author without a mark, which takes the block mode, package options,
//! `\\`, `\and`, `\thanks` or commands in a name, changed `\Authsep` and kin) goes to the
//! engine.

use super::super::engine_run::Machine;
use super::super::engine_token::Token;

/// authblk's own names a document may change; the native job keeps their package values.
pub(super) const NAMES: [&str; 8] = ["Authsep", "Authands", "Authand", "Authfont", "Affilfont", "affilsep", "authorcr", "AB@affilsep"];

/// The authors and affiliations so far, each its mark and text, and `\affilsep`.
#[derive(PartialEq, Clone, Default)]
pub(super) struct Authblk {
    authors: Vec<(Vec<Token>, Vec<Token>)>,
    affils: Vec<(Vec<Token>, Vec<Token>)>,
    affilsep: i32,
}

/// After `\author` or `\affil`: whether the call is `[mark]{text}` of plain text the native
/// job takes (letters, digits, spaces, punctuation; the text is scanned on as text).
pub(super) fn call_ok(after: &str) -> bool {
    let plain = |s: &str, extra: &str| !s.trim().is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(c) || c as u32 >= 0x80);
    let Some((mark, rest)) = after.trim_start().strip_prefix('[').and_then(|r| r.split_once(']')) else { return false };
    let Some((text, _)) = rest.trim_start().strip_prefix('{').and_then(|r| r.split_once('}')) else { return false };
    plain(mark, "*,") && plain(text, " \t\r\n.,;:-'()/")
}

impl Machine<'_, '_> {
    /// `\usepackage{authblk}`: `\setlength{\affilsep}{1em}` in the font of the moment.
    pub(super) fn authblk_load(&mut self) {
        let Some(sep) = self.length_of("1em") else { return self.not_native("authblk's \\affilsep") };
        if let Some(doc) = self.doc.as_mut() {
            doc.authblk = Some(Box::new(Authblk { affilsep: sep, ..Authblk::default() }));
        }
    }

    /// authblk's `\author[mark]{name}` (`affil` false) or `\affil[mark]{text}`.
    pub(super) fn authblk_entry(&mut self, affil: bool) {
        if !self.next_is('[') {
            return self.not_native("authblk: an author or affiliation without a mark");
        }
        let mark = self.bracket_arg();
        let mut text = Vec::new();
        self.read_arg(&mut text);
        if mark.is_empty() || mark.iter().chain(&text).any(|t| t.cs_id().is_some()) {
            return self.not_native("authblk: a command in an author or affiliation");
        }
        let Some(a) = self.doc.as_mut().and_then(|d| d.authblk.as_deref_mut()) else { return };
        if affil { a.affils.push((mark, text)) } else { a.authors.push((mark, text)) }
        if let Some(doc) = self.doc.as_mut() {
            doc.authored = true;
        }
    }

    /// What article's `\begin{tabular}[t]{c}\@author\end{tabular}` becomes under authblk:
    /// `\begin`'s group, `\begin{center}`, the two lists with `\\[\affilsep]` between, and
    /// `\end{center}`. `None` without authblk.
    pub(super) fn authblk_title(&mut self) -> Option<Vec<Token>> {
        let a = self.doc.as_ref()?.authblk.as_deref()?.clone();
        // No affiliation: `\@author` is `\AB@author`, the plain form; no author: nothing.
        if a.authors.is_empty() || a.affils.is_empty() {
            self.not_native("authblk without authors or affiliations");
            return Some(Vec::new());
        }
        let mut t = self.tex_tokens("\\begingroup\\begin{center}");
        let n = a.authors.len();
        for (k, (mark, name)) in a.authors.iter().enumerate() {
            let sep = match k {
                0 => "",
                _ if k + 1 < n => ", ",
                _ if n == 2 => " and ",
                _ => ", and ",
            };
            let sep = format!("{sep}\\normalfont ");
            t.extend(self.tex_tokens(&sep));
            t.extend(name);
            t.extend(self.tex_tokens("\\textsuperscript{\\normalfont "));
            t.extend(mark);
            t.extend(self.tex_tokens("}"));
        }
        t.extend(self.tex_tokens(&format!("\\\\[{}sp]", a.affilsep)));
        for (k, (mark, text)) in a.affils.iter().enumerate() {
            t.extend(self.tex_tokens(if k == 0 { "\\normalfont\\textsuperscript{\\normalfont " } else { "\\\\\\normalfont\\textsuperscript{\\normalfont " }));
            t.extend(mark);
            t.extend(self.tex_tokens("}\\normalfont "));
            t.extend(text);
        }
        t.extend(self.tex_tokens("\\end{center}\\endgroup"));
        Some(t)
    }
}
