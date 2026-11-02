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
//! go to the engine; so do offsets and a heading title that is not plain text (its mark is
//! `\protect`ed text). `\fancypagestyle{plain}{defs}` is native (`fancy_pagestyle`); other
//! names and a base style decline.

/// `\fancypagestyle{plain}{defs}` (`\f@nch@@pagestyle`): `\ps@plain` is `\let\f@nch@gbl\relax
/// \ps@f@nch@fancyproto defs\relax`, run in `\@outputpage`'s group for a page that
/// `\thispagestyle{plain}` asked for, so it starts from the slots as they stand (the
/// preamble's: the native job refuses them in the body) and its definitions stay in that
/// group. `slots`: what the defs set (`None` where they set nothing); `rules`: the defs'
/// `\renewcommand`s of `\headrulewidth` and `\footrulewidth`, run in the output group.
#[derive(PartialEq, Clone)]
pub(super) struct PlainStyle {
    pub slots: [Option<Option<Vec<Token>>>; 12],
    pub rules: Vec<Token>,
}

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
/// back until the heading's end (`\ttl@mk`); `defs`: whether the mark commands are still
/// `\f@nch@initialise`'s (book's `\pagestyle{headings}` puts its own back).
#[derive(PartialEq, Clone)]
pub(super) struct Fancy {
    pub slots: [Option<Vec<Token>>; 12],
    pub on: bool,
    pub marks: u32,
    pub title: Vec<Token>,
    pub page: bool,
    pub this: bool,
    pub mkboth: Option<Vec<Token>>,
    pub defs: bool,
    /// `\fancypagestyle{plain}`'s definitions ([`PlainStyle`]).
    pub plain: Option<Box<PlainStyle>>,
    /// While `\fancypagestyle`'s defs run: the slots before them and which slots they set.
    pub recording: Option<Box<Recording>>,
}

/// The twelve slots before `\fancypagestyle`'s defs, and which of them the defs set.
pub type Recording = ([Option<Vec<Token>>; 12], [bool; 12]);

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
            doc.fancy = Some(Box::new(Fancy { slots, on: false, marks: 0, title: Vec::new(), page: false, this: false, mkboth: None, defs: true, plain: None, recording: None }));
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
                    if let Some(r) = f.recording.as_mut() {
                        r.1[6 + k] = true;
                    }
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
        if let Some(r) = f.recording.as_mut() {
            for (m, s) in r.1.iter_mut().zip(set) {
                *m |= s;
            }
        }
    }

    /// `\fancypagestyle{plain}{defs}` in the preamble: the defs' slot commands run now on the
    /// slots as they stand, what they set is kept ([`PlainStyle`]) and the slots are put back
    /// (`rt@fancypsend`); their `\renewcommand{\headrulewidth}{…}` and `\footrulewidth` are
    /// kept to run in the output group. Another style name, a `[base]` style, or any other
    /// command in the defs is not native.
    pub(super) fn fancy_pagestyle(&mut self) {
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let name: String = name.iter().map(|t| t.chr()).collect();
        if name.trim() != "plain" || self.next_is('[') {
            return self.not_native("\\fancypagestyle other than {plain} with no base style");
        }
        let mut defs = Vec::new();
        self.read_arg(&mut defs);
        if self.doc.as_ref().is_some_and(|d| d.body) {
            return self.not_native("\\fancypagestyle in the document body");
        }
        let id = |m: &mut Self, n: &str| m.e.cs.intern(n);
        let renew = id(self, "renewcommand");
        let rules = [id(self, "headrulewidth"), id(self, "footrulewidth")];
        let allowed: Vec<u32> = ["fancyhf", "fancyhead", "fancyfoot", "lhead", "chead", "rhead", "lfoot", "cfoot", "rfoot"].iter().map(|n| id(self, n)).collect();
        // Split the defs into slot commands (with their arguments) and the rule widths.
        let (mut run, mut kept) = (Vec::new(), Vec::new());
        let mut k = 0;
        let group_end = |t: &[Token], from: usize| -> Option<usize> {
            let mut depth = 0;
            for (j, x) in t.iter().enumerate().skip(from) {
                match x.cat() {
                    Some(Cat::BeginGroup) => depth += 1,
                    Some(Cat::EndGroup) => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(j + 1);
                        }
                    }
                    _ if depth == 0 => return Some(j + 1),
                    _ => {}
                }
            }
            None
        };
        while k < defs.len() {
            let t = defs[k];
            if t.cat() == Some(Cat::Space) {
                k += 1;
                continue;
            }
            match t.cs_id() {
                Some(c) if c == renew => {
                    // `\renewcommand{\headrulewidth}{dim}` or `\renewcommand\headrulewidth{dim}`.
                    let Some(a) = group_end(&defs, k + 1) else { break };
                    let inner: Vec<Token> = defs[k + 1..a].iter().copied().filter(|x| x.cs_id().is_some()).collect();
                    let Some(b) = group_end(&defs, a) else { break };
                    if inner.len() != 1 || !rules.contains(&inner[0].cs_id().unwrap_or(0)) || defs[a..b].iter().any(|x| x.cs_id().is_some()) {
                        return self.not_native("\\fancypagestyle: a definition other than the head and foot slots and rule widths");
                    }
                    kept.extend_from_slice(&defs[k..b]);
                    k = b;
                }
                Some(c) if allowed.contains(&c) => {
                    run.push(t);
                    k += 1;
                    // `[keys]`, then the text.
                    while k < defs.len() && defs[k].cat() == Some(Cat::Space) {
                        k += 1;
                    }
                    if defs.get(k) == Some(&Token::char(Cat::Other, '[')) {
                        while k < defs.len() && defs[k] != Token::char(Cat::Other, ']') {
                            run.push(defs[k]);
                            k += 1;
                        }
                        if k < defs.len() {
                            run.push(defs[k]);
                            k += 1;
                        }
                    }
                    let Some(b) = group_end(&defs, k) else { break };
                    run.extend_from_slice(&defs[k..b]);
                    k = b;
                }
                _ => return self.not_native("\\fancypagestyle: a definition other than the head and foot slots and rule widths"),
            }
        }
        let Some(f) = self.doc.as_mut().and_then(|d| d.fancy.as_mut()) else { return };
        f.recording = Some(Box::new((f.slots.clone(), [false; 12])));
        f.plain = Some(Box::new(PlainStyle { slots: Default::default(), rules: kept }));
        run.push(self.doc_id(doc!("rt@fancypsend")));
        self.back_list(&run);
    }

    /// The end of [`Machine::fancy_pagestyle`]'s defs: the slots they set into
    /// [`PlainStyle::slots`], the slots before them back.
    pub(super) fn fancy_pagestyle_end(&mut self) {
        let Some(f) = self.doc.as_mut().and_then(|d| d.fancy.as_mut()) else { return };
        let Some(r) = f.recording.take() else { return };
        let (saved, mask) = *r;
        let now = std::mem::replace(&mut f.slots, saved);
        if let Some(p) = f.plain.as_mut() {
            for (k, s) in now.into_iter().enumerate() {
                p.slots[k] = mask[k].then_some(s);
            }
        }
    }

    /// The slots of one side (`from` 0 or 6) of a fancy page, `\fancypagestyle{plain}`'s
    /// over the others when `plain`.
    fn fancy_side_slots(&self, from: usize, plain: bool) -> Vec<Option<Vec<Token>>> {
        let Some(f) = self.doc.as_ref().and_then(|d| d.fancy.as_ref()) else { return Vec::new() };
        let p = f.plain.as_ref().filter(|_| plain);
        (from..from + 6).map(|k| p.and_then(|p| p.slots[k].clone()).unwrap_or_else(|| f.slots[k].clone())).collect()
    }

    /// The first slot of page `page`'s side: `\@outputpage` takes `\@evenhead` and
    /// `\@evenfoot` on an even page of a two-sided document.
    fn fancy_side(&self, page: i32) -> usize {
        if self.doc.as_ref().is_some_and(|d| d.class.flag("twoside")) && page % 2 == 0 { 6 } else { 0 }
    }

    /// Whether page `page` being output has fancyhdr's head and foot (`\pagestyle{fancy}`
    /// with no `\thispagestyle`, or `\thispagestyle{fancy}`), declining when a slot of its
    /// side still reads marks back.
    pub(super) fn fancy_page(&mut self, this: bool, page: i32, plain: bool) -> bool {
        let side = self.fancy_side(page);
        let Some(doc) = self.doc.as_ref() else { return false };
        if !doc.fancy.as_ref().is_some_and(|f| f.on || this || plain) {
            return false;
        }
        if self.fancy_side_slots(side, plain).iter().any(Option::is_none) {
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
    pub(super) fn fancy_tokens(&mut self, page: i32, plain: bool) -> Vec<Token> {
        let side = self.fancy_side(page);
        let slots: Vec<Vec<Token>> = self.fancy_side_slots(side, plain).into_iter().map(Option::unwrap_or_default).collect();
        let rules = self.doc.as_ref().and_then(|d| d.fancy.as_ref()).and_then(|f| f.plain.as_ref()).filter(|_| plain).map(|p| p.rules.clone()).unwrap_or_default();
        let Some(doc) = self.doc.as_ref() else { return Vec::new() };
        let class = doc.class;
        let (pre, post) = if side == 6 { ("\\hss", "") } else { ("", "\\hss") };
        let dim = |sp: i32| {
            let mut s = String::new();
            super::super::engine_value::print_scaled(sp, &mut s);
            s + "pt"
        };
        let w = dim(class.dimen("textwidth"));
        let thepage = self.e.cs.intern("thepage");
        // `\thepage` in the `\pagenumbering` style (book's `\frontmatter` roman).
        let digits: Vec<Token> = super::book::page_text(self.page_value(page)).chars().map(|c| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c)).collect();
        // latex.ltx:18325 `\leftmark` is `\mark_use_last:nn{page}{2e-left}`, `\rightmark`
        // `\mark_use_first:nn{page}{2e-right}`: the page's marks as the head reads them.
        let (leftmark, rightmark) = (self.e.cs.intern("leftmark"), self.e.cs.intern("rightmark"));
        let left = self.page_mark_text(super::book::LEFT as u32, false).unwrap_or_default();
        let right = self.page_mark_text(super::book::RIGHT as u32, true).unwrap_or_default();
        // `\nouppercase{..}` (`\f@nch@reset`): its argument in a group where `\MakeUppercase`
        // is `\f@nch@noUppercase` (`[2][]{#2}`) and its robust inner `\MakeUppercase\ \ \ `
        // `[#1]#2{#2}`: the marks' uppercasing gives its argument's tokens back.
        let nouppercase = self.e.cs.intern("nouppercase");
        let names = [self.e.cs.intern("MakeUppercase"), self.e.cs.intern("MakeUppercase   ")];
        let uppercase = self.e.cs.intern("uppercase");
        let raw = |m: &mut Self, class: u16, first: bool| m.page_mark_parts(class as u32, first).map_or(Some(Vec::new()), |(t, _)| no_uppercase(&t, names, uppercase));
        let (Some(left_lower), Some(right_lower)) = (raw(self, super::book::LEFT, false), raw(self, super::book::RIGHT, true)) else {
            self.not_native("fancyhdr's \\nouppercase of a mark with \\uppercase");
            return Vec::new();
        };
        let row = |m: &mut Machine<'_, '_>, from: usize| -> Vec<Token> {
            let mut t = m.tex_tokens(&format!("\\hbox to{w}{{\\rlap{{\\parbox[b]{{{w}}}{{\\raggedright\\indent\\ignorespaces "));
            for (k, (open, close)) in [("", "}}\\hfill"), (&format!("\\parbox[b]{{{w}}}{{\\centering\\indent\\ignorespaces ") as &str, "}\\hfill"), (&format!("\\llap{{\\parbox[b]{{{w}}}{{\\raggedleft\\indent\\ignorespaces ") as &str, "}}}")].into_iter().enumerate() {
                t.extend(m.tex_tokens(open));
                // The depth of braces inside `\nouppercase`'s argument (0 outside it).
                let (mut lower, mut armed) = (0usize, false);
                for &x in &slots[from + k] {
                    if armed {
                        armed = false;
                        t.push(Token::char(Cat::BeginGroup, '{'));
                        if x.cat() == Some(Cat::BeginGroup) {
                            lower = 1;
                            continue;
                        }
                        // An argument of one token: the group closes after it.
                        if x.cs_id() == Some(leftmark) {
                            t.extend(&left_lower);
                        } else if x.cs_id() == Some(rightmark) {
                            t.extend(&right_lower);
                        } else if x.cs_id() == Some(thepage) {
                            t.extend(&digits);
                        } else {
                            t.push(x);
                        }
                        t.push(Token::char(Cat::EndGroup, '}'));
                        continue;
                    }
                    if lower > 0 {
                        match x.cat() {
                            Some(Cat::BeginGroup) => lower += 1,
                            Some(Cat::EndGroup) => lower -= 1,
                            _ => {}
                        }
                    }
                    if x.cs_id() == Some(nouppercase) {
                        armed = true;
                    } else if x.cs_id() == Some(thepage) {
                        t.extend(&digits);
                    } else if x.cs_id() == Some(leftmark) {
                        t.extend(if lower > 0 { &left_lower } else { &left });
                    } else if x.cs_id() == Some(rightmark) {
                        t.extend(if lower > 0 { &right_lower } else { &right });
                    } else {
                        t.push(x);
                    }
                }
                t.extend(m.tex_tokens(close));
            }
            t
        };
        let mut t = self.tex_tokens("\\begingroup");
        // `\ps@plain`'s rule widths, local to the output group.
        t.extend(rules);
        t.extend(self.tex_tokens(&format!("\\normalfont\\normalsize\\global\\setbox{HEAD_BOX}\\hbox to{}{{{pre}\\hbox to{w}{{\\setbox0\\vbox{{", dim(class.dimen("textwidth")))));
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
    /// puts it in its `\chapter*` title (`fancy_toc_marks`), article's in its `\section*` title,
    /// report's `thebibliography` after it (`bib_marks`); article's bibliography declines. With titlesec, `\tableofcontents`'s `\@mkboth` in
    /// its `\section*` title is held by `\ttl@select` (`\ttl@gmk`) and put after the heading
    /// (`fancy_finmarks`).
    pub(super) fn fancy_mkboth(&mut self, toc: bool) -> bool {
        // Under report only when `\titleformat{\chapter}` made the `\chapter*` titlesec's.
        let (titlesec, report) = self.doc.as_ref().map_or((false, false), |d| (d.titlesec.as_ref().is_some_and(|t| !d.report || t.chapter), d.report));
        let plain_class = self.doc.as_ref().is_some_and(|d| !d.koma && !d.memoir && d.ams.is_none() && d.ieee.is_none());
        let Some(f) = self.doc.as_mut().and_then(|d| d.fancy.as_mut()).filter(|f| f.on) else { return false };
        if toc && titlesec {
            f.mkboth = Some(Vec::new());
            return false;
        }
        // article's `\section*{\contentsname \@mkboth{..}{..}}` as report's `\chapter*`: the
        // marks in the title's paragraph (`fancy_toc_marks`).
        if toc && (report || plain_class) {
            return false;
        }
        self.not_native("fancyhdr: the marks of \\tableofcontents or thebibliography");
        true
    }

    /// Whether `\pagestyle{fancy}` is on (`\@mkboth` is `\markboth`).
    /// Whether `\chaptermark` and `\sectionmark` are fancyhdr's (`Fancy::defs`).
    pub(super) fn fancy_defs(&self) -> bool {
        self.doc.as_ref().and_then(|d| d.fancy.as_ref()).is_some_and(|f| f.defs)
    }

    pub(super) fn fancy_on_now(&self) -> bool {
        self.doc.as_ref().and_then(|d| d.fancy.as_ref()).is_some_and(|f| f.on)
    }

    /// `\markboth{\MakeUppercase\name}{\MakeUppercase\name}` under fancyhdr: both texts
    /// (classes 1, 2 and 3), `\name` expanded by the mark's `\protected@edef`.
    pub(super) fn fancy_mkboth_upper(&mut self, name: &str) {
        let mut upper = vec![Token::cs(self.e.cs.intern("MakeUppercase   "))];
        upper.extend(self.tex_tokens(&format!("[]{{{name}}}")));
        let nobreak = self.doc.as_ref().is_some_and(|d| d.nobreak);
        self.fancy_push(vec![(1, upper.clone()), (2, upper.clone()), (3, upper)], nobreak);
    }

    /// `\ttl@finmarks`'s `\ttl@mk`: the held `\@mkboth{\MakeUppercase\contentsname}
    /// {\MakeUppercase\contentsname}`, a `\markboth` with both texts (classes 1, 2 and 3).
    pub(super) fn fancy_finmarks(&mut self) {
        let Some(name) = self.doc.as_mut().and_then(|d| d.fancy.as_mut()).and_then(|f| f.mkboth.take()) else {
            return;
        };
        // The name held: `Contents` unless natbib's `\bibsection` held its own.
        let name: String = name.iter().map(|t| t.chr()).collect();
        self.fancy_mkboth_upper(if name.is_empty() { self.head_name(super::babel::HEAD_CONTENTS) } else { &name });
    }

    /// report's `\tableofcontents` under fancyhdr: `\chapter*{\contentsname \@mkboth
    /// {\MakeUppercase\contentsname}{\MakeUppercase\contentsname}}`, the marks in the
    /// title's paragraph (they migrate out of its line, §655), no `\nobreak` there (`\ifvmode`).
    pub(super) fn fancy_toc_marks(&mut self) {
        self.fancy_mkboth_upper(self.head_name(super::babel::HEAD_CONTENTS));
    }

    /// report's and book's `thebibliography` after `\chapter*{\bibname}`: `\@mkboth
    /// {\MakeUppercase\bibname}{\MakeUppercase\bibname}`, under `\pagestyle{fancy}` a
    /// `\markboth` with both texts (classes 1, 2 and 3), under book's `headings` the same
    /// marks, under `plain` and `empty` nothing (`\@gobbletwo`).
    pub(super) fn bib_marks(&mut self) {
        // scrbook: natbib's `\bibsection` is `\bib@heading`'s `\@mkdouble{\MakeMarkcase{\bibname}}`.
        if self.doc.as_ref().is_some_and(|d| d.pkg.koma.book) {
            return self.koma_mkdouble("Bibliography");
        }
        let name = self.head_name(super::babel::HEAD_BIB);
        if self.fancy_on_now() {
            return self.fancy_mkboth_upper(name);
        }
        self.mkboth_upper(name);
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
        let (nobreak, appendix, numbered) = (doc.nobreak, doc.appendix, doc.numbered);
        let mut text = Vec::new();
        // `\ifnum \c@secnumdepth >\z@` (`>\@ne`, report's `>\m@ne`): the number only for a
        // level `secnumdepth` numbers, which is the heading's own `\@sect` test.
        if numbered && report {
            // `\@chapapp` is `\chaptername`, after report's `\appendix` `\appendixname`; `\ `
            // a control space.
            let space = Token::cs(self.e.cs.intern(" "));
            if level == 0 {
                let chapapp = self.head_name(if appendix { super::babel::HEAD_APPENDIX } else { super::babel::HEAD_CHAPTER });
                text.extend(self.tex_tokens(chapapp));
                text.push(space);
            }
            text.extend(self.tex_tokens(&format!("{number}. ")));
            text.push(space);
        } else if numbered {
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

/// A mark's text as fancyhdr's `\nouppercase` reads it: each `\MakeUppercase` (`names`:
/// the command and its robust inner one) with its optional `[..]` and its argument gives
/// the argument's tokens back. `None` when the text has `\uppercase` (made `\relax` there).
fn no_uppercase(text: &[Token], names: [u32; 2], uppercase: u32) -> Option<Vec<Token>> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let x = text[i];
        i += 1;
        if x.cs_id() == Some(uppercase) {
            return None;
        }
        if !x.cs_id().is_some_and(|c| names.contains(&c)) {
            out.push(x);
            continue;
        }
        if text.get(i).is_some_and(|t| t.cat().is_some() && t.chr() == '[') {
            i += text[i..].iter().position(|t| t.cat().is_some() && t.chr() == ']')? + 1;
        }
        match text.get(i) {
            Some(t) if t.cat() == Some(Cat::BeginGroup) => {
                let mut depth = 0usize;
                let start = i + 1;
                loop {
                    match text.get(i)?.cat() {
                        Some(Cat::BeginGroup) => depth += 1,
                        Some(Cat::EndGroup) => depth -= 1,
                        _ => {}
                    }
                    i += 1;
                    if depth == 0 {
                        break;
                    }
                }
                out.extend_from_slice(&text[start..i - 1]);
            }
            Some(&t) => {
                out.push(t);
                i += 1;
            }
            None => return None,
        }
    }
    Some(out)
}
