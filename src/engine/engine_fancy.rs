//! fancyhdr on the GDL path, for a one-sided document: `\pagestyle{fancy}`, the head and
//! foot slots (`\fancyhf`, `\fancyhead`, `\fancyfoot`, `\lhead` … `\rfoot`) and the marks
//! its `\sectionmark` and `\subsectionmark` put in the page.
//!
//! fancyhdr stores a slot as `\f@nch@def` does (the text then `\strut`, or nothing) and
//! builds the head at output time as `\f@nch@head` does (fancyhdr.sty, 2025):
//!
//! `\hbox to\headwidth{\f@nch@vbox\headheight{\hbox to\headwidth{\rlap{\parbox[b]{w}{
//! \raggedright\leavevmode\ignorespaces L}}\hfill\parbox[b]{w}{\centering …C}\hfill
//! \llap{\parbox[b]{w}{\raggedleft …R}}}\vskip\headruleskip\relax\headrule}}\hss`
//!
//! the foot likewise with `\footrule` and `\vskip\footruleskip` above its slots. Each slot
//! is `\headwidth` wide, so `\f@nch@hfbox` always takes its centred form. `\f@nch@vbox`
//! lowers the box's height to `\headheight` (`\footskip`) when it is taller, and only that
//! box's (no `compatV3`). The slots' `\leavevmode` is `\indent` here: both start the
//! paragraph with an empty box `\parindent` wide.
//!
//! The package's own head slots, `\fancyplain{}{\slshape\rightmark}`, read marks back and
//! go to the engine; so do `\fancypagestyle`, offsets, the `[E]` slots of a two-sided
//! document and a heading title that is not plain text (its mark is `\protect`ed text).

use super::super::engine_catcode::Cat;
use super::super::engine_node::{Kind, Node};
use super::super::engine_run::{Machine, Mode};
use super::super::engine_token::Token;
use super::heading_number;

/// The head line built by the head's tokens, a hidden box register.
pub(super) const HEAD_BOX: u32 = 32705;

/// fancyhdr's state: the odd slots `\f@nch@olh`, `och`, `orh`, `olf`, `ocf`, `orf`, then the
/// even ones `elh` … `erf` (`None` for the package's `\rightmark`/`\leftmark` heads),
/// whether `\pagestyle{fancy}` is on,
/// `\g__mark_int`, the last heading's title for its mark, whether the page being output
/// is fancy, whether `\thispagestyle{fancy}` asked for it, and the `\@mkboth` titlesec holds
/// back until the heading's end (`\ttl@mk`).
#[derive(PartialEq, Clone)]
pub(super) struct Fancy {
    pub slots: [Option<Vec<Token>>; 12],
    pub on: bool,
    pub marks: u32,
    pub title: Vec<Token>,
    pub page: bool,
    pub this: bool,
    pub mkboth: Option<Vec<Token>>,
}

impl Machine<'_, '_> {
    /// `\usepackage{fancyhdr}`: `\f@nch@initialise` (`\fancyhf{}`, the two mark heads at
    /// the left and right of both sides, `\fancyfoot[c]{\rmfamily\thepage}`),
    /// `\headrulewidth` 0.4pt and `\footrulewidth` 0pt as macros.
    pub(super) fn fancy_load(&mut self) {
        let mut foot = self.tex_tokens("\\rmfamily\\thepage");
        foot.push(self.strut_cs());
        let side = [None, Some(Vec::new()), None, Some(Vec::new()), Some(foot), Some(Vec::new())];
        let slots = std::array::from_fn(|k| side[k % 6].clone());
        if let Some(doc) = self.doc.as_mut() {
            doc.fancy = Some(Box::new(Fancy { slots, on: false, marks: 0, title: Vec::new(), page: false, this: false, mkboth: None }));
        }
        let t = self.tex_tokens("\\def\\headrulewidth{0.4pt}\\def\\footrulewidth{0pt}");
        self.back_list(&t);
    }

    fn strut_cs(&mut self) -> Token {
        Token::cs(self.e.cs.intern("strut"))
    }

    /// `\pagestyle{fancy}` in the preamble. Its `\headwidth` is the `\textwidth` of the page
    /// (the native job refuses `\headwidth` itself).
    pub(super) fn fancy_on(&mut self) {
        if let Some(f) = self.doc.as_mut().and_then(|d| d.fancy.as_mut()) {
            f.on = true;
        }
    }

    /// `\fancyhf`, `\fancyhead`, `\fancyfoot` (`which` "", "h", "f") with their optional
    /// `[keys]`: each comma-separated key's `e`/`o`, `l`/`c`/`r` and `h`/`f` letters, all of
    /// a kind when it names none (`\f@nch@default`); the even slots are used two-sided.
    pub(super) fn fancy_hf(&mut self, which: &str) {
        let keys = match self.next_unblank() {
            Some(t) if t == Token::char(Cat::Other, '[') => self.bracket_arg(),
            Some(t) => {
                self.input.back(t);
                Vec::new()
            }
            None => Vec::new(),
        };
        let mut text = Vec::new();
        self.read_arg(&mut text);
        if self.doc.as_ref().is_some_and(|d| d.body) {
            return self.not_native("fancyhdr's head and foot set in the document body");
        }
        let keys: String = keys.iter().map(|t| t.chr().to_ascii_lowercase()).collect();
        let mut set = [false; 12];
        for key in keys.split(',') {
            let key = key.trim();
            if key.chars().any(|c| !"eolcrhf".contains(c)) {
                return self.not_native("a fancyhdr key that is not e, o, l, c, r, h or f");
            }
            let pick = |all: &str| -> String {
                let s: String = all.chars().filter(|&c| key.contains(c)).collect();
                if s.is_empty() { all.into() } else { s }
            };
            let hf = pick(if which.is_empty() { "hf" } else { which });
            for (side, eo) in "oe".chars().enumerate() {
                if !pick("eo").contains(eo) {
                    continue;
                }
                for (k, lcr) in "lcr".chars().enumerate() {
                    if pick("lcr").contains(lcr) {
                        for (j, c) in "hf".chars().enumerate() {
                            set[6 * side + 3 * j + k] |= hf.contains(c) && (which.is_empty() || which.contains(c));
                        }
                    }
                }
            }
        }
        self.fancy_set(set, text);
    }

    /// `\lhead[even]{odd}` and its five kin (`k` 0 to 5): `\f@nch@def\f@nch@olh{odd}
    /// \f@nch@def\f@nch@elh{even}`, the even text by default `\f@nch@olh` itself, so the
    /// even slot is the odd one and one more `\strut`.
    pub(super) fn fancy_slot(&mut self, k: usize) {
        let mut even = None;
        if let Some(t) = self.next_unblank() {
            if t == Token::char(Cat::Other, '[') {
                even = Some(self.bracket_arg());
            } else {
                self.input.back(t);
            }
        }
        let mut text = Vec::new();
        self.read_arg(&mut text);
        if self.doc.as_ref().is_some_and(|d| d.body) {
            return self.not_native("fancyhdr's head and foot set in the document body");
        }
        let mut set = [false; 12];
        set[k] = true;
        self.fancy_set(set, text);
        let mut set = [false; 12];
        set[6 + k] = true;
        match even {
            Some(e) => self.fancy_set(set, e),
            None => {
                // `\f@nch@olh\strut`: never empty.
                let strut = self.strut_cs();
                if let Some(f) = self.doc.as_mut().and_then(|d| d.fancy.as_mut()) {
                    let mut e = f.slots[k].clone().unwrap_or_default();
                    e.push(strut);
                    f.slots[6 + k] = Some(e);
                }
            }
        }
    }

    /// `\f@nch@def`: nothing for an empty text, else the text then `\strut`.
    fn fancy_set(&mut self, set: [bool; 12], mut text: Vec<Token>) {
        if !text.is_empty() {
            text.push(self.strut_cs());
        }
        let Some(f) = self.doc.as_mut().and_then(|d| d.fancy.as_mut()) else { return };
        for (slot, _) in f.slots.iter_mut().zip(set).filter(|p| p.1) {
            *slot = Some(text.clone());
        }
    }

    /// The first slot of page `page`'s side: `\@outputpage` takes `\@evenhead` and
    /// `\@evenfoot` on an even page of a two-sided document.
    fn fancy_side(&self, page: i32) -> usize {
        if self.doc.as_ref().is_some_and(|d| d.class.flag("twoside")) && page % 2 == 0 { 6 } else { 0 }
    }

    /// Whether page `page` being output has fancyhdr's head and foot (`\pagestyle{fancy}`
    /// with no `\thispagestyle`, or `\thispagestyle{fancy}`), declining when a slot of its
    /// side still reads marks back.
    pub(super) fn fancy_page(&mut self, this: bool, page: i32) -> bool {
        let side = self.fancy_side(page);
        let Some(doc) = self.doc.as_ref() else { return false };
        let Some(f) = doc.fancy.as_ref().filter(|f| f.on || this) else { return false };
        if f.slots[side..side + 6].iter().any(Option::is_none) {
            self.not_native("fancyhdr's \\leftmark and \\rightmark heads");
            return false;
        }
        if let Some(f) = self.doc.as_mut().and_then(|d| d.fancy.as_mut()) {
            f.page = true;
        }
        true
    }

    /// The head into [`HEAD_BOX`] and the foot into the output nest, as tokens:
    /// `\begingroup\normalfont\normalsize` (`\@outputpage`'s `\reset@font\normalsize`), `\thepage` as
    /// the page's digits, then `\rt@outputfoot`. The `\hss` after each line (`\f@nch@Oorh`,
    /// `\f@nch@Oorf`) comes before it on an even side (`\f@nch@Oelh`, `\f@nch@Oelf`).
    pub(super) fn fancy_tokens(&mut self, page: i32) -> Vec<Token> {
        let side = self.fancy_side(page);
        let Some(doc) = self.doc.as_ref() else { return Vec::new() };
        let class = doc.class;
        let slots: Vec<Vec<Token>> = doc.fancy.as_ref().map(|f| f.slots[side..side + 6].iter().map(|s| s.clone().unwrap_or_default()).collect()).unwrap_or_default();
        let (pre, post) = if side == 6 { ("\\hss", "") } else { ("", "\\hss") };
        let dim = |sp: i32| {
            let mut s = String::new();
            super::super::engine_value::print_scaled(sp, &mut s);
            s + "pt"
        };
        let w = dim(class.dimen("textwidth"));
        let thepage = self.e.cs.intern("thepage");
        let digits: Vec<Token> = page.to_string().chars().map(|c| Token::char(Cat::Other, c)).collect();
        let row = |m: &mut Machine<'_, '_>, from: usize| -> Vec<Token> {
            let mut t = m.tex_tokens(&format!("\\hbox to{w}{{\\rlap{{\\parbox[b]{{{w}}}{{\\raggedright\\indent\\ignorespaces "));
            for (k, (open, close)) in [("", "}}\\hfill"), (&format!("\\parbox[b]{{{w}}}{{\\centering\\indent\\ignorespaces ") as &str, "}\\hfill"), (&format!("\\llap{{\\parbox[b]{{{w}}}{{\\raggedleft\\indent\\ignorespaces ") as &str, "}}}")].into_iter().enumerate() {
                t.extend(m.tex_tokens(open));
                for &x in &slots[from + k] {
                    if x.cs_id() == Some(thepage) {
                        t.extend(&digits);
                    } else {
                        t.push(x);
                    }
                }
                t.extend(m.tex_tokens(close));
            }
            t
        };
        let mut t = self.tex_tokens(&format!("\\begingroup\\normalfont\\normalsize\\global\\setbox{HEAD_BOX}\\hbox to{}{{{pre}\\hbox to{w}{{\\setbox0\\vbox{{", dim(class.dimen("textwidth"))));
        let head = row(self, 0);
        t.extend(head);
        t.extend(self.tex_tokens(&format!("\\vskip0pt\\relax{{\\hrule height\\headrulewidth width{w}\\vskip-\\headrulewidth}}}}")));
        t.push(self.doc_id(doc!("rt@fancyclip")));
        t.extend(self.tex_tokens(&format!("0\\box0}}{post}}}{pre}\\hbox to{w}{{\\setbox0\\vbox{{\\setbox0\\vbox{{{{\\hrule width{w} height\\footrulewidth}}}}\\unvbox0")));
        t.push(self.doc_id(doc!("rt@fancyskip")));
        let foot = row(self, 3);
        t.extend(foot);
        t.extend(self.tex_tokens("}"));
        t.push(self.doc_id(doc!("rt@fancyclip")));
        t.extend(self.tex_tokens(&format!("1\\box0}}{post}\\endgroup")));
        t
    }

    /// `\vskip\footruleskip`: `.3\normalbaselineskip` (§455: the factor's fraction by
    /// `xn_over_d`), after the output's `\normalsize`.
    pub(super) fn fancy_skip(&mut self) {
        let b = self.e.eqtb.glue(super::NORMAL_BASELINE_SKIP).width;
        let w = crate::backend::scaled::xn_over_d(b, 19661, 0x10000);
        let n = self.e.nodes.glue_node(super::Glue { width: w, ..Default::default() }, 0);
        self.cur.push(n);
    }

    /// `\f@nch@vbox`'s `\ifdim\ht0>#1 … \ht0=#1`: box 0 no taller than `\headheight` (`0`)
    /// or `\footskip` (`1`).
    pub(super) fn fancy_clip(&mut self) {
        let foot = self.scan_int() == 1;
        let Some(class) = self.doc.as_ref().map(|d| d.class) else { return };
        let limit = class.dimen(if foot { "footskip" } else { "headheight" });
        if let Some(b) = self.e.eqtb.box_reg(0) {
            let r = &mut self.e.nodes.boxes[b as usize];
            r.height = r.height.min(limit);
        }
    }

    /// Under `\pagestyle{fancy}`, `\@mkboth` is `\markboth`. report's `\tableofcontents`
    /// puts it in its `\chapter*` title (`fancy_toc_marks`), report's `thebibliography` after
    /// it (`bib_marks`); article's decline. With titlesec, `\tableofcontents`'s `\@mkboth` in
    /// its `\section*` title is held by `\ttl@select` (`\ttl@gmk`) and put after the heading
    /// (`fancy_finmarks`).
    pub(super) fn fancy_mkboth(&mut self, toc: bool) -> bool {
        let (titlesec, report) = self.doc.as_ref().map_or((false, false), |d| (d.titlesec.is_some(), d.report));
        let Some(f) = self.doc.as_mut().and_then(|d| d.fancy.as_mut()).filter(|f| f.on) else { return false };
        if toc && titlesec {
            f.mkboth = Some(Vec::new());
            return false;
        }
        if toc && report {
            return false;
        }
        self.not_native("fancyhdr: the marks of \\tableofcontents or thebibliography");
        true
    }

    /// Whether `\pagestyle{fancy}` is on (`\@mkboth` is `\markboth`).
    pub(super) fn fancy_on_now(&self) -> bool {
        self.doc.as_ref().and_then(|d| d.fancy.as_ref()).is_some_and(|f| f.on)
    }

    /// `\markboth{\MakeUppercase\name}{\MakeUppercase\name}` under fancyhdr: both texts
    /// (classes 1, 2 and 3), `\name` expanded by the mark's `\protected@edef`.
    fn fancy_mkboth_upper(&mut self, name: &str) {
        let mut upper = vec![Token::cs(self.e.cs.intern("MakeUppercase   "))];
        upper.extend(self.tex_tokens(&format!("[]{{{name}}}")));
        let nobreak = self.doc.as_ref().is_some_and(|d| d.nobreak);
        self.fancy_push(vec![(1, upper.clone()), (2, upper.clone()), (3, upper)], nobreak);
    }

    /// `\ttl@finmarks`'s `\ttl@mk`: the held `\@mkboth{\MakeUppercase\contentsname}
    /// {\MakeUppercase\contentsname}`, a `\markboth` with both texts (classes 1, 2 and 3).
    pub(super) fn fancy_finmarks(&mut self) {
        if self.doc.as_mut().and_then(|d| d.fancy.as_mut()).and_then(|f| f.mkboth.take()).is_none() {
            return;
        }
        self.fancy_mkboth_upper("Contents");
    }

    /// report's `\tableofcontents` under fancyhdr: `\chapter*{\contentsname \@mkboth
    /// {\MakeUppercase\contentsname}{\MakeUppercase\contentsname}}`, the marks in the
    /// title's paragraph (they migrate out of its line, §655), no `\nobreak` there (`\ifvmode`).
    pub(super) fn fancy_toc_marks(&mut self) {
        self.fancy_mkboth_upper("Contents");
    }

    /// report's and book's `thebibliography` after `\chapter*{\bibname}`: `\@mkboth
    /// {\MakeUppercase\bibname}{\MakeUppercase\bibname}`, under `\pagestyle{fancy}` a
    /// `\markboth` with both texts (classes 1, 2 and 3), under book's `headings` the same
    /// marks, under `plain` and `empty` nothing (`\@gobbletwo`).
    pub(super) fn bib_marks(&mut self) {
        if self.fancy_on_now() {
            return self.fancy_mkboth_upper("Bibliography");
        }
        self.mkboth_upper("Bibliography");
    }

    /// `\sectionmark` and `\subsectionmark` as fancyhdr defines them, at an unstarred
    /// heading: `\markboth{\MakeUppercase{\thesection\hskip 1em\relax title}}{}` and
    /// `\markright{\thesubsection\hskip 1em\relax title}`; under report (`\f@nch@initialise`
    /// with `\chapter` defined) `\chaptermark`, `\markboth{\MakeUppercase{\@chapapp\
    /// \thechapter. \ title}}{}`, and `\sectionmark`, `\markright{\MakeUppercase{\thesection.
    /// \ title}}` (report's `\subsectionmark` gobbles). Each `\mark_insert:nn` steps
    /// `\g__mark_int` and puts `\marks<class>{\__mark_value:nn{n}{text}}` (classes 1
    /// `2e-left`, 2 `2e-right`, 3 `2e-right-nonempty` when the text is not empty), then
    /// `\nobreak` when `\if@nobreak`.
    pub(super) fn fancy_marks(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let level = doc.level;
        let report = doc.report;
        let Some(f) = doc.fancy.as_mut().filter(|_| level < 2) else { return };
        let title = std::mem::take(&mut f.title);
        // A UTF-8 character is its active bytes.
        if title.is_empty() || title.iter().any(|t| t.cs_id().is_some() || !matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space)) && !(t.cat() == Some(Cat::Active) && t.chr() as u32 >= 0x80)) {
            return self.not_native("fancyhdr: a heading whose mark is not plain text");
        }
        let number = heading_number(&doc.counters, level, doc.appendix);
        let (nobreak, appendix) = (doc.nobreak, doc.appendix);
        let mut text = Vec::new();
        if report {
            // `\@chapapp` is `\chaptername`, after report's `\appendix` `\appendixname`; `\ `
            // a control space.
            let space = Token::cs(self.e.cs.intern(" "));
            if level == 0 {
                text.extend(self.tex_tokens(if appendix { "Appendix" } else { "Chapter" }));
                text.push(space);
            }
            text.extend(self.tex_tokens(&format!("{number}. ")));
            text.push(space);
        } else {
            text.extend(self.tex_tokens(&number));
            text.extend(self.tex_tokens("\\hskip 1em\\relax "));
        }
        text.extend(title);
        let upper = |m: &mut Self, text: Vec<Token>| {
            let mut upper = vec![Token::cs(m.e.cs.intern("MakeUppercase   "))];
            upper.extend(m.tex_tokens("[]{"));
            upper.extend(text);
            upper.extend(m.tex_tokens("}"));
            upper
        };
        let marks: Vec<(u16, Vec<Token>)> = if level == 0 {
            vec![(1, upper(self, text)), (2, Vec::new())]
        } else if report {
            let text = upper(self, text);
            vec![(2, text.clone()), (3, text)]
        } else {
            vec![(2, text.clone()), (3, text)]
        };
        self.fancy_push(marks, nobreak);
    }

    /// Each mark as `\mark_insert:nn` puts it: `\g__mark_int` stepped, `\marks<class>
    /// {\__mark_value:nn{n}{text}}`, then `\nobreak` when `\if@nobreak`.
    fn fancy_push(&mut self, marks: Vec<(u16, Vec<Token>)>, nobreak: bool) {
        let value = Token::cs(self.e.cs.intern("__mark_value:nn"));
        for (class, text) in marks {
            let Some(f) = self.doc.as_mut().and_then(|d| d.fancy.as_mut()) else { return };
            f.marks += 1;
            let n = f.marks;
            let mut list = vec![value];
            list.extend(self.tex_tokens(&format!("{{{n}}}{{")));
            list.extend(text);
            list.extend(self.tex_tokens("}"));
            let span = self.e.toks.store(&list);
            self.cur.push(Node { kind: Kind::Mark, sub: 0, font: class, a: span.start as i32, b: span.end as i32 });
            // `\if@nobreak\ifvmode\nobreak\fi\fi`.
            if nobreak && self.mode == Mode::Vertical {
                self.cur.push(Node::new(Kind::Penalty, 10000));
            }
        }
    }
}
