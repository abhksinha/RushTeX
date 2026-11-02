//! apacite (2013/07/21 v6.03) citations natively, at its defaults (`apaciteclassic`), for
//! keys no `.bbl` defines: `\cite`, `\citeNP`, `\citeA`, `\citeauthor`, `\citeauthorNP`,
//! `\citeyear`, `\citeyearNP` and their `full` and `short` forms.
//!
//! Each command sets the punctuation `\@@cite<#1>[#2]#3` puts around the keys (apacite.sty
//! lines 258-519): `\mbox{\@BBOP}`, then for each key `\@ifauthorsunequalc@de` (the first)
//! or `\@ifauthorsequalc@de` (the rest, whose `\b@key` is the same `?` as the one before),
//! then `{\@BBN #2}`, `{\@BAY}` and `{\@BBCP}`. An undefined key's `\b@key` and `\Y@key` are
//! `?`, so the `full` and `short` forms (which differ in `\BCA` only) print the same. The
//! punctuation stays the package's macros (`\BBOP`, `\BBAY`, ..., from its table), so a
//! document's `\renewcommand` of them holds.
//!
//! The punctuation redefined in the preamble (`english.apc` restores it), a prefix (`<..>`), math, a `.bbl`'s `\bibitem`s, and the packages whose hooks change the
//! commands (hyperref, natbib, backref, bibunits, babel's `.apc` files, url) decline.

use super::super::engine_catcode::Cat;
use super::super::engine_run::{Machine, Mode};
use super::super::engine_token::Token;

/// One command's punctuation: `\@BBOP` (in the `\mbox`), `\@BBAY`, `\@BBY`, `\@BAY`,
/// `\@BBCP`, and whether it cites authors (`\if@A@cite`) and years (`\if@Y@cite`).
#[derive(Clone, Copy)]
pub(super) struct Form {
    open: &'static str,
    bbay: &'static str,
    bby: &'static str,
    bay: &'static str,
    close: &'static str,
    authors: bool,
    years: bool,
}

/// `\cite` (`\@cite`), `\citeNP`, `\citeA` (`\@citeA`), `\citeauthor` (`\@citeauthor`, and
/// `\@citeauthorNP`, which differs in `\@BBA` only), `\citeyear`, `\citeyearNP`.
const CITE: Form = Form { open: "\\BBOP", bbay: "\\BBAY", bby: "\\BBYY", bay: "", close: "\\BBCP", authors: true, years: true };
const CITE_NP: Form = Form { open: "", close: "", ..CITE };
const CITE_A: Form = Form { open: "", bbay: " \\BBOP", bby: "\\BBYY", bay: "\\BBCP", close: "", authors: true, years: true };
const CITE_AUTHOR: Form = Form { open: "", bbay: "", bby: "", bay: "", close: "", authors: true, years: false };
const CITE_YEAR: Form = Form { open: "\\BBOP", bbay: "", bby: "\\BBYY", bay: "", close: "\\BBCP", authors: false, years: true };
const CITE_YEAR_NP: Form = Form { open: "", close: "", ..CITE_YEAR };

/// The natives in `DOC` (`DOC_PARTS`), reached through `PACKAGE_NATIVES`.
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@apacite", |m| m.apa_cite(CITE)),
    ("rt@apaciteNP", |m| m.apa_cite(CITE_NP)),
    ("rt@apaciteA", |m| m.apa_cite(CITE_A)),
    ("rt@apaciteauthor", |m| m.apa_cite(CITE_AUTHOR)),
    ("rt@apaciteyear", |m| m.apa_cite(CITE_YEAR)),
    ("rt@apaciteyearNP", |m| m.apa_cite(CITE_YEAR_NP)),
];

/// The punctuation as apacite.sty defines it and as `english.apc` sets it again in the
/// package's last `\AtBeginDocument` (`\languagename.apc`: the format's hyphen.cfg defines
/// `\iflanguage` and `\languagename` `english`, so it is read without babel too).
const PUNCTUATION: [(&str, &str); 9] = [("BBOP", "("), ("BBCP", ")"), ("BBAA", "\\&"), ("BBAB", "and"), ("BAP", " "), ("BBAY", ", "), ("BBYY", ", "), ("BBC", "; "), ("BBN", ", ")];

impl Machine<'_, '_> {
    /// `english.apc` at `\begin{document}` puts the punctuation back as it was: a preamble's
    /// redefinition is lost there, which the native job does not model, so it declines.
    pub(super) fn apacite_begin_document(&mut self) {
        if !self.doc.as_ref().is_some_and(|d| d.packages.contains(&"apacite")) {
            return;
        }
        for (name, text) in PUNCTUATION {
            let id = self.e.cs.intern(name);
            if self.macro_body(id).as_deref() != Some(text) {
                return self.not_native("apacite's punctuation redefined in the preamble");
            }
        }
    }

    /// `\cite[post]{keys}` and the others: the tokens `\@@cite` leaves for undefined keys.
    fn apa_cite(&mut self, f: Form) {
        if self.mode == Mode::Math {
            return self.not_native("an apacite citation in math");
        }
        if let Some(p) = self.doc.as_ref().and_then(|d| d.packages.iter().find(|p| matches!(**p, "hyperref" | "natbib" | "backref" | "bibunits" | "babel" | "url"))) {
            return self.not_native(&format!("apacite with {p}"));
        }
        if self.next_is('<') {
            return self.not_native("an apacite citation with a prefix");
        }
        let post = if self.next_is('[') { self.bracket_arg() } else { Vec::new() };
        let mut keys = Vec::new();
        self.read_arg(&mut keys);
        let text: String = keys.iter().map(|t| t.chr()).collect();
        if keys.iter().any(|t| !matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space))) || text.split(',').any(|k| k.trim().is_empty()) {
            return self.not_native("an apacite citation with a blank key or commands in its keys");
        }
        if self.doc.as_ref().is_some_and(|d| d.labels.keys().chain(d.predicted.keys()).any(|k| k.starts_with(super::BIB))) {
            return self.not_native("an apacite citation with a bibliography");
        }
        let mut s = format!("\\mbox{{{}}}", f.open);
        for k in 0..text.split(',').count() {
            if k == 0 {
                // `\@ifauthorsunequalc@de` with `\if@F@cite` true: `{\b@key}{\@BBAY}`, `{\Y@key}`.
                if f.authors {
                    s.push_str("{?}");
                    if f.years {
                        s.push_str(&format!("{{{}}}", f.bbay));
                    }
                }
            } else if f.years {
                // `\@ifauthorsequalc@de`: `{\@BBY}`.
                s.push_str(&format!("{{{}}}", f.bby));
            }
            if f.years {
                s.push_str("{?}");
            }
        }
        let mut t = self.tex_tokens(&s);
        if !post.is_empty() {
            t.push(Token::char(Cat::BeginGroup, '{'));
            t.extend(self.tex_tokens("\\BBN "));
            t.extend(post);
            t.push(Token::char(Cat::EndGroup, '}'));
        }
        let end = if f.years { format!("{{{}}}{{{}}}", f.bay, f.close) } else { format!("{{{}}}", f.close) };
        t.extend(self.tex_tokens(&end));
        self.back_list(&t);
    }
}
