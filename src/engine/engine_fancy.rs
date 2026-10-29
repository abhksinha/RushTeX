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
use super::super::engine_run::Machine;
use super::super::engine_token::Token;
use super::heading_number;

/// The head line built by the head's tokens, a hidden box register.
pub(super) const HEAD_BOX: u32 = 32705;

/// fancyhdr's state: the odd slots `\f@nch@olh`, `och`, `orh`, `olf`, `ocf`, `orf` (`None`
/// for the package's `\rightmark`/`\leftmark` heads), whether `\pagestyle{fancy}` is on,
/// `\g__mark_int`, the last heading's title for its mark, whether the page being output
/// is fancy, and whether `\thispagestyle{fancy}` asked for it.
pub(super) struct Fancy {
    pub slots: [Option<Vec<Token>>; 6],
    pub on: bool,
    pub marks: u32,
    pub title: Vec<Token>,
    pub page: bool,
    pub this: bool,
}

impl Machine<'_, '_> {
    /// `\usepackage{fancyhdr}`: `\f@nch@initialise` (one-sided: `\fancyhf{}`, the two
    /// mark heads, `\fancyfoot[c]{\rmfamily\thepage}`), `\headrulewidth` 0.4pt and
    /// `\footrulewidth` 0pt as macros.
    pub(super) fn fancy_load(&mut self) {
        let mut foot = self.tex_tokens("\\rmfamily\\thepage");
        foot.push(self.strut_cs());
        let slots = [None, Some(Vec::new()), None, Some(Vec::new()), Some(foot), Some(Vec::new())];
        if let Some(doc) = self.doc.as_mut() {
            doc.fancy = Some(Box::new(Fancy { slots, on: false, marks: 0, title: Vec::new(), page: false, this: false }));
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
    /// a kind when it names none (`\f@nch@default`); the even slots are not used one-sided.
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
        let mut set = [false; 6];
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
            if !pick("eo").contains('o') {
                continue;
            }
            for (k, lcr) in "lcr".chars().enumerate() {
                if pick("lcr").contains(lcr) {
                    for (j, c) in "hf".chars().enumerate() {
                        set[3 * j + k] |= hf.contains(c) && (which.is_empty() || which.contains(c));
                    }
                }
            }
        }
        self.fancy_set(set, text);
    }

    /// `\lhead[even]{odd}` and its five kin (`k` 0 to 5): the odd slot.
    pub(super) fn fancy_slot(&mut self, k: usize) {
        if let Some(t) = self.next_unblank() {
            if t == Token::char(Cat::Other, '[') {
                self.bracket_arg();
            } else {
                self.input.back(t);
            }
        }
        let mut text = Vec::new();
        self.read_arg(&mut text);
        if self.doc.as_ref().is_some_and(|d| d.body) {
            return self.not_native("fancyhdr's head and foot set in the document body");
        }
        let mut set = [false; 6];
        set[k] = true;
        self.fancy_set(set, text);
    }

    /// `\f@nch@def`: nothing for an empty text, else the text then `\strut`.
    fn fancy_set(&mut self, set: [bool; 6], mut text: Vec<Token>) {
        if !text.is_empty() {
            text.push(self.strut_cs());
        }
        let Some(f) = self.doc.as_mut().and_then(|d| d.fancy.as_mut()) else { return };
        for (slot, _) in f.slots.iter_mut().zip(set).filter(|p| p.1) {
            *slot = Some(text.clone());
        }
    }

    /// Whether the page being output has fancyhdr's head and foot (`\pagestyle{fancy}` with
    /// no `\thispagestyle`, or `\thispagestyle{fancy}`), declining when a slot still reads
    /// marks back.
    pub(super) fn fancy_page(&mut self, this: bool) -> bool {
        let Some(doc) = self.doc.as_ref() else { return false };
        let Some(f) = doc.fancy.as_ref().filter(|f| f.on || this) else { return false };
        if f.slots.iter().any(Option::is_none) {
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
    /// the page's digits, then `\rt@outputfoot`.
    pub(super) fn fancy_tokens(&mut self, page: i32) -> Vec<Token> {
        let Some(doc) = self.doc.as_ref() else { return Vec::new() };
        let class = doc.class;
        let slots: Vec<Vec<Token>> = doc.fancy.as_ref().map(|f| f.slots.iter().map(|s| s.clone().unwrap_or_default()).collect()).unwrap_or_default();
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
        let mut t = self.tex_tokens(&format!("\\begingroup\\normalfont\\normalsize\\global\\setbox{HEAD_BOX}\\hbox to{}{{\\hbox to{w}{{\\setbox0\\vbox{{", dim(class.dimen("textwidth"))));
        let head = row(self, 0);
        t.extend(head);
        t.extend(self.tex_tokens(&format!("\\vskip0pt\\relax{{\\hrule height\\headrulewidth width{w}\\vskip-\\headrulewidth}}}}")));
        t.push(self.doc_id(doc!("rt@fancyclip")));
        t.extend(self.tex_tokens(&format!("0\\box0}}\\hss}}\\hbox to{w}{{\\setbox0\\vbox{{\\setbox0\\vbox{{{{\\hrule width{w} height\\footrulewidth}}}}\\unvbox0")));
        t.push(self.doc_id(doc!("rt@fancyskip")));
        let foot = row(self, 3);
        t.extend(foot);
        t.extend(self.tex_tokens("}"));
        t.push(self.doc_id(doc!("rt@fancyclip")));
        t.extend(self.tex_tokens("1\\box0}\\hss\\endgroup"));
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

    /// Under `\pagestyle{fancy}`, `\@mkboth` is `\markboth`: `\tableofcontents` and
    /// `thebibliography` put marks the native job does not make yet, so they decline.
    pub(super) fn fancy_mkboth(&mut self) -> bool {
        let on = self.doc.as_ref().and_then(|d| d.fancy.as_ref()).is_some_and(|f| f.on);
        if on {
            self.not_native("fancyhdr: the marks of \\tableofcontents or thebibliography");
        }
        on
    }

    /// `\sectionmark` and `\subsectionmark` as fancyhdr defines them, at an unstarred
    /// heading: `\markboth{\MakeUppercase{\thesection\hskip 1em\relax title}}{}` and
    /// `\markright{\thesubsection\hskip 1em\relax title}`. Each `\mark_insert:nn` steps
    /// `\g__mark_int` and puts `\marks<class>{\__mark_value:nn{n}{text}}` (classes 1
    /// `2e-left`, 2 `2e-right`, 3 `2e-right-nonempty` when the text is not empty), then
    /// `\nobreak` when `\if@nobreak`.
    pub(super) fn fancy_marks(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let level = doc.level;
        let Some(f) = doc.fancy.as_mut().filter(|_| level < 2) else { return };
        let title = std::mem::take(&mut f.title);
        // A UTF-8 character is its active bytes.
        if title.is_empty() || title.iter().any(|t| t.cs_id().is_some() || !matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space)) && !(t.cat() == Some(Cat::Active) && t.chr() as u32 >= 0x80)) {
            return self.not_native("fancyhdr: a heading whose mark is not plain text");
        }
        let number = heading_number(&doc.counters, level, doc.appendix);
        let nobreak = doc.nobreak;
        let mut text = self.tex_tokens(&number);
        text.extend(self.tex_tokens("\\hskip 1em\\relax "));
        text.extend(title);
        let marks: Vec<(u16, Vec<Token>)> = if level == 0 {
            let mut upper = vec![Token::cs(self.e.cs.intern("MakeUppercase   "))];
            upper.extend(self.tex_tokens("[]{"));
            upper.extend(text);
            upper.extend(self.tex_tokens("}"));
            vec![(1, upper), (2, Vec::new())]
        } else {
            vec![(2, text.clone()), (3, text)]
        };
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
            if nobreak {
                self.cur.push(Node::new(Kind::Penalty, 10000));
            }
        }
    }
}
