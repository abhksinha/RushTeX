//! book.cls on the GDL path (`2026-10-31-native-book-class.md`). book.cls is report.cls
//! with these differences (`diff report.cls book.cls`), each native here or in the report
//! code it shares:
//!
//! - `twoside` and `openright` by default (`\ExecuteOptions{letterpaper,10pt,twoside,
//!   onecolumn,final,openright}`), `bk1x.clo` for `size1x.clo` (the class table);
//! - `\pagestyle{headings}` at the class's end: `\@oddhead{{\slshape\rightmark}\hfil
//!   \thepage}` (and `\@evenhead{\thepage\hfil\slshape\leftmark}` two-sided), empty feet,
//!   `\@mkboth` = `\markboth`, and `\chaptermark` (`\sectionmark` two-sided) putting marks;
//! - `\if@mainmatter`: `\frontmatter` (`\cleardoublepage \@mainmatterfalse
//!   \pagenumbering{roman}`), `\mainmatter` (`\cleardoublepage \@mainmattertrue
//!   \pagenumbering{arabic}`), `\backmatter` (`\cleardoublepage` or `\clearpage`,
//!   `\@mainmatterfalse`); outside the main matter `\@chapter` steps no counter, its
//!   contents line has no `\numberline` and `\@makechapterhead` no "Chapter N" line;
//! - `\chapter` and `titlepage` start with `\cleardoublepage` (`\if@openright`);
//! - no `abstract` environment.

use super::super::engine_catcode::Cat;
use super::super::engine_node::{Kind, Node};
use super::super::engine_run::{Machine, Mode};
use super::super::engine_token::Token;
use super::fancy::HEAD_BOX;
use super::NEW_PAGE;

/// `\pagenumbering`'s styles, each `\@<style>` (latex.ltx `\pagenumbering#1`:
/// `\gdef\thepage{\csname @#1\endcsname\c@page}`), and hyperref's page label type of each
/// (`\HyPL@arabic` `D`, ...).
pub(super) const PAGE_STYLES: [&str; 5] = ["arabic", "roman", "Roman", "alph", "Alph"];
const LABEL_TYPES: [&str; 5] = ["D", "r", "R", "a", "A"];

/// A page counter value with the numbering style it is printed in.
pub(super) fn page_value(n: i32, style: u8) -> i32 {
    n.wrapping_mul(8) | style as i32
}

/// `\thepage` of a [`page_value`]: `\@arabic`, `\@roman` (`\romannumeral`), `\@Roman`,
/// `\@alph`, `\@Alph` (empty outside 1 to 26, where LaTeX stops with `\@ctrerr`).
pub(super) fn page_text(v: i32) -> String {
    let (n, style) = (v >> 3, v & 7);
    match style {
        1 => super::roman(n),
        2 => super::roman(n).to_ascii_uppercase(),
        3 => super::alph(n, b'a').collect(),
        4 => super::alph(n, b'A').collect(),
        _ => n.to_string(),
    }
}

/// The l3mark classes LaTeX's marks use (`\markboth`, `\markright`): `2e-left`,
/// `2e-right`, `2e-right-nonempty`.
pub(super) const LEFT: u16 = 1;
pub(super) const RIGHT: u16 = 2;
const RIGHT_NONEMPTY: u16 = 3;

impl Machine<'_, '_> {
    pub(super) fn book(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.book)
    }

    /// `\thepage` of page `n` in the current numbering.
    pub(super) fn the_page(&self, n: i32) -> String {
        page_text(self.page_value(n))
    }

    pub(super) fn page_value(&self, n: i32) -> i32 {
        page_value(n, self.doc.as_ref().map_or(0, |d| d.page_style))
    }

    /// `\pagenumbering{style}`: `\global\c@page\@ne` and `\thepage` in the style.
    pub(super) fn page_numbering(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let name: String = arg.iter().map(|t| t.chr()).collect();
        let Some(style) = PAGE_STYLES.iter().position(|s| *s == name.trim()) else {
            return self.not_native("\\pagenumbering of another style");
        };
        if let Some(doc) = self.doc.as_mut() {
            doc.page = 1;
            doc.page_style = style as u8;
        }
    }

    /// `\cleardoublepage` (latex.ltx): `\clearpage`, then `\rt@cleardouble`.
    pub(super) fn clear_double_page(&mut self) {
        let t = [self.doc_id(doc!("clearpage")), self.doc_id(doc!("rt@cleardouble"))];
        self.back_list(&t);
    }

    /// After `\cleardoublepage`'s `\clearpage`: `\if@twoside \ifodd\c@page\else \hbox{}
    /// \newpage\fi\fi` (one column).
    pub(super) fn clear_double_tail(&mut self) {
        let Some(doc) = self.doc.as_ref() else { return };
        if doc.class.flag("twoside") && doc.page % 2 == 0 {
            let mut t = self.tex_tokens("\\hbox{}");
            t.push(self.doc_id(NEW_PAGE));
            self.back_list(&t);
        }
    }

    /// book's `\frontmatter` (`0`), `\mainmatter` (`1`) and `\backmatter` (`2`).
    pub(super) fn book_matter(&mut self, which: u8) {
        let Some(doc) = self.doc.as_mut() else { return };
        doc.mainmatter = which == 1;
        let clear = if which < 2 || doc.openright { doc!("cleardoublepage") } else { doc!("clearpage") };
        let mut t = vec![self.doc_id(clear)];
        if which < 2 {
            t.push(self.doc_id(doc!("pagenumbering")));
            t.extend(self.tex_tokens(if which == 0 { "{roman}" } else { "{arabic}" }));
        }
        self.back_list(&t);
    }

    /// report's and book's `\maketitle` (`\@titlepagetrue`): `\begin{titlepage}
    /// \let\footnote\thanks \null\vfil \vskip 60\p@ \begin{center}{\LARGE \@title \par}
    /// \vskip 3em {\large \lineskip .75em \begin{tabular}[t]{c}\@author \end{tabular}\par}
    /// \vskip 1.5em {\large \@date \par}\end{center}\par \@thanks \vfil\null
    /// \end{titlepage}\setcounter{footnote}{0}`. Its `\thanks` keep `\thefootnote` arabic,
    /// which the native `\thanks` does not make: they decline (`thanks`).
    pub(super) fn make_title_page(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let [title, author, date] = std::mem::take(&mut doc.stored);
        // In `\maketitle` for `\and` (`title_and`); emptied by `\rt@titleend`.
        doc.thanks = Some(Vec::new());
        let (dated, authored) = (doc.dated, doc.authored);
        let date = if dated { date } else { self.tex_tokens("\\today") };
        let author = if authored { author } else { self.tex_tokens("\\relax") };
        let mut t = self.tex_tokens("\\begin{titlepage}\\null\\vfil\\vskip60pt\\begin{center}{\\LARGE ");
        // `\@title` unexpanded: microtype's `\leftprotrusion` after `center`'s `\item\relax`
        // meets a command there, not the title's first character (no kern).
        t.push(self.doc_id(doc!("rt@fi")));
        t.extend(title);
        t.extend(self.tex_tokens("\\par}\\vskip3em{\\large\\lineskip.75em \\begin{tabular}[t]{c}"));
        t.extend(author);
        t.extend(self.tex_tokens("\\end{tabular}\\par}\\vskip1.5em{\\large "));
        t.extend(date);
        t.extend(self.tex_tokens("\\par}\\end{center}\\par\\vfil\\null\\end{titlepage}"));
        t.push(self.doc_id(doc!("rt@titleend")));
        self.back_list(&t);
    }

    /// `\mark_insert:nn{class}{text}`: `\g__mark_int` stepped, `\marks<class>{\__mark_value:nn
    /// {n}{text}}`, then `\nobreak` in vertical mode when `\if@nobreak`.
    fn latex_mark(&mut self, class: u16, text: &[Token]) {
        let Some(doc) = self.doc.as_mut() else { return };
        doc.marks += 1;
        let (n, nobreak) = (doc.marks, doc.nobreak);
        let mut list = vec![Token::cs(self.e.cs.intern("__mark_value:nn"))];
        list.extend(self.tex_tokens(&format!("{{{n}}}{{")));
        list.extend_from_slice(text);
        list.push(Token::char(Cat::EndGroup, '}'));
        let span = self.e.toks.store(&list);
        self.cur.push(Node { kind: Kind::Mark, sub: 0, font: class, a: span.start as i32, b: span.end as i32 });
        if nobreak && self.mode == Mode::Vertical {
            self.cur.push(Node::new(Kind::Penalty, 10000));
        }
    }

    /// `\markboth{left}{right}` (latex.ltx): `2e-left`, `2e-right`, and `2e-right-nonempty`
    /// when `right` is not empty.
    pub(super) fn mark_both(&mut self, left: &[Token], right: &[Token]) {
        self.latex_mark(LEFT, left);
        self.mark_right(right);
    }

    /// `\markright{right}`: `2e-right`, and `2e-right-nonempty` when it is not empty.
    pub(super) fn mark_right(&mut self, right: &[Token]) {
        self.latex_mark(RIGHT, right);
        if !right.is_empty() {
            self.latex_mark(RIGHT_NONEMPTY, right);
        }
    }

    /// `\MakeUppercase{text}` as a mark keeps it.
    fn uppercase_mark(&mut self, text: &[Token]) -> Vec<Token> {
        let mut t = vec![Token::cs(self.e.cs.intern("MakeUppercase   "))];
        t.extend(self.tex_tokens("[]{"));
        t.extend_from_slice(text);
        t.push(Token::char(Cat::EndGroup, '}'));
        t
    }

    /// Whether marks are text the head can typeset: letters, other characters and spaces
    /// (a UTF-8 character, a command or math in a title declines).
    fn plain_mark_text(text: &[Token]) -> bool {
        text.iter().all(|t| t.cs_id().is_none() && matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space)))
    }

    /// book's `\chaptermark{title}` at `\@chapter` (`headings`, as `\pagestyle{headings}`
    /// left it at the class's end: `\ps@plain` does not undo it): `\markboth{\MakeUppercase
    /// {Chapter N. \ title}}{}` two-sided, `\markright{…}` one-sided; outside the main
    /// matter the title alone (`\if@mainmatter`).
    pub(super) fn chapter_mark(&mut self, number: &str, title: &[Token]) {
        if !Self::plain_mark_text(title) {
            return self.not_native("book: a chapter title whose mark is not plain text");
        }
        let mut text = Vec::new();
        if !number.is_empty() {
            // `\@chapapp`: `\appendixname` after `\appendix`.
            let chapapp = if self.doc.as_ref().is_some_and(|d| d.appendix) { "Appendix" } else { "Chapter" };
            text = self.tex_tokens(&format!("{chapapp}\\ {number}. \\ "));
        }
        text.extend_from_slice(title);
        let mark = self.uppercase_mark(&text);
        if self.doc.as_ref().is_some_and(|d| d.class.flag("twoside")) {
            self.mark_both(&mark, &[]);
        } else {
            self.mark_right(&mark);
        }
    }

    /// book's two-sided `\sectionmark{title}` at an unstarred `\section`: `\markright
    /// {\MakeUppercase{N. \ title}}`; one-sided it is empty.
    pub(super) fn section_mark(&mut self, number: &str, title: &[Token]) {
        if !self.doc.as_ref().is_some_and(|d| (d.book || d.memoir) && d.class.flag("twoside")) {
            return;
        }
        if !Self::plain_mark_text(title) {
            return self.not_native("book: a section title whose mark is not plain text");
        }
        let mut text = self.tex_tokens(&format!("{number}. \\ "));
        text.extend_from_slice(title);
        let mark = self.uppercase_mark(&text);
        self.mark_right(&mark);
    }

    /// `\@mkboth{\MakeUppercase\name}{\MakeUppercase\name}` (`\tableofcontents`,
    /// `thebibliography`): marks under `headings`, nothing under `plain` and `empty`.
    pub(super) fn mkboth_upper(&mut self, name: &str) {
        if !self.doc.as_ref().is_some_and(|d| d.headings) {
            return;
        }
        let text = self.tex_tokens(name);
        let mark = self.uppercase_mark(&text);
        self.mark_both(&mark, &mark);
    }

    /// The text of the page's mark of `class` (`first` or last), as the head typesets it:
    /// `\__mark_value:nn{n}{text}` unwrapped, `\MakeUppercase[]{…}` applied to letters.
    /// `None` when no mark of the class was ever put.
    pub(super) fn page_mark_text(&mut self, class: u32, first: bool) -> Option<Vec<Token>> {
        let span = self.page.mark(class, if first { 1 } else { 2 })?;
        let list = self.e.toks.get(span).to_vec();
        // `\__mark_value:nn`, `{`, the number, `}`, `{`, text, `}`.
        let open = list.iter().skip(1).position(|t| t.cat() == Some(Cat::BeginGroup)).map(|k| k + 1)?;
        let close = list[open..].iter().position(|t| t.cat() == Some(Cat::EndGroup))? + open;
        let text = list.get(close + 2..list.len().saturating_sub(1))?;
        let upper = text.first().and_then(|t| t.cs_id()).is_some_and(|id| self.e.cs.name(id) == "MakeUppercase   ");
        let body = if upper { text.get(4..text.len().saturating_sub(1))? } else { text };
        Some(body.iter().map(|&t| if upper && t.cat() == Some(Cat::Letter) { Token::char(Cat::Letter, t.chr().to_ascii_uppercase()) } else { t }).collect())
    }

    /// book's `headings` head of page `page` into [`HEAD_BOX`] as tokens: `\begingroup
    /// \normalfont\normalsize` (`\@outputpage`'s `\reset@font\normalsize`), then `\hb@xt@
    /// \textwidth{\@thehead}` with `\@oddhead` `{\slshape\rightmark}\hfil\thepage` or,
    /// two-sided on an even page, `\@evenhead` `\thepage\hfil\slshape\leftmark`.
    pub(super) fn headings_tokens(&mut self, page: i32) -> Vec<Token> {
        let Some(doc) = self.doc.as_ref() else { return Vec::new() };
        let even = doc.class.flag("twoside") && page % 2 == 0;
        let w = doc.class.dimen("textwidth");
        let digits = label_digits(&self.the_page(page));
        let mark = self.page_mark_text(if even { LEFT as u32 } else { RIGHT as u32 }, !even).unwrap_or_default();
        let mut t = self.tex_tokens(&format!("\\begingroup\\normalfont\\normalsize\\global\\setbox{HEAD_BOX}\\hbox to{w}sp{{"));
        if even {
            t.extend(digits);
            t.extend(self.tex_tokens("\\hfil\\slshape "));
            t.extend(mark);
        } else {
            t.extend(self.tex_tokens("{\\slshape "));
            t.extend(mark);
            t.extend(self.tex_tokens("}\\hfil "));
            t.extend(digits);
        }
        t.extend(self.tex_tokens("}\\endgroup"));
        t
    }

    /// hyperref's `\HyPL@EveryPage` for the page being shipped (`\c@page` `page` in the
    /// current numbering): a `/PageLabels` entry when the type changes or the number does
    /// not follow the last (`\HyPL@StorePageLabel`), the absolute page then stepped.
    pub(super) fn page_label(&mut self, page: i32) {
        let Some(doc) = self.doc.as_mut() else { return };
        let style = doc.page_style;
        let (abs, last) = (doc.abs_page, doc.page_label_last);
        if page <= 0 {
            return self.not_native("hyperref: a page number below one");
        }
        if last != Some((style, page - 1)) {
            let st = if page == 1 { String::new() } else { format!(" /St {page}") };
            doc.page_labels.push_str(&format!("{abs}<</S/{}{st}>>", LABEL_TYPES[style as usize]));
        }
        doc.page_label_last = Some((style, page));
        doc.abs_page += 1;
    }

    /// `\HyPL@SetPageLabels` at `enddocument/afteraux`: `/PageLabels<</Nums[…]>>` in the
    /// catalog, unless the labels are `\HyPL@Useless` (`0<</S/D>>`).
    pub(super) fn set_page_labels(&mut self) {
        let Some(doc) = self.doc.as_ref() else { return };
        if doc.page_labels.is_empty() || doc.page_labels == "0<</S/D>>" {
            return;
        }
        let text = format!("/PageLabels<</Nums[{}]>>", doc.page_labels);
        self.add_catalog(&text, None);
    }
}

/// A page number's characters as other tokens (letters for roman numerals, as
/// `\romannumeral` gives them catcode 12: other).
pub(super) fn label_digits(text: &str) -> Vec<Token> {
    text.chars().map(|c| Token::char(Cat::Other, c)).collect()
}
