//! amsart.cls on the GDL path (`2026-10-31-native-journal-classes.md`): its front matter
//! (`\title`, `\author`, `\address`, `\curraddr`, `\email`, `\urladdr`, `\keywords`,
//! `\subjclass`, `\date`), the `abstract` box, `\maketitle`, the page styles `firstpage`,
//! `plain` and `headings`, the addresses at the end, and its `~`.
//!
//! The class's parameters (sizes from `\@typesizes`, `\@adjustvertspacing`'s display skips,
//! `\@listI`, the page geometry of `\calclayout`, `\footnotesep` from `\AtBeginDocument`) are
//! the class table (`rushtex-gen class --class amsart`). What is code is here: amsart.cls's
//! macros (v2.20.6) as tokens of commands the native job has, with the document's own text
//! spliced in, as the macros expand them. Its sections, theorems and footnote rule are
//! branches of the article natives (`section_start`, `ams_theorem_head`, `output_column`).
//!
//! Not native yet (each stops in the pre-scan or declines): lists, footnotes, floats,
//! `thebibliography`, run-in `\subsection`s, `\thanks`, `\dedicatory`, `\contrib`, a `\\`,
//! math or accented letters in the title or an author, an abstract after `\maketitle`, and
//! `\maketitle` after the first page.

use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::Glue;
use super::super::engine_node::{Kind, Node};
use super::super::engine_run::{Machine, Mode, SEMI_SIMPLE_GROUP};
use super::super::engine_token::Token;
use super::{Change, List, Label, Make, FOOTINS, R_ATTOPSEP, R_FOOTNOTESEP, R_ITEMINDENT, R_LABELWIDTH, R_LEFTMARGIN, R_LINEWIDTH, R_LISTPARINDENT, R_PARSEP, R_RIGHTMARGIN, R_TOTALLEFTMARGIN, R_ATRIGHTSKIP};

/// Kinds of the `\addresses` entries: the `\author{}` between two authors, then
/// `\address`, `\curraddr`, `\email`, `\urladdr`.
pub(super) const AUTHOR: u8 = 0;
pub(super) const ADDRESS: u8 = 1;
pub(super) const CURRADDR: u8 = 2;
pub(super) const EMAIL: u8 = 3;
pub(super) const URLADDR: u8 = 4;

/// The box register holding `\abstractbox` (a register no document can name).
pub(super) const ABSTRACT_BOX: u32 = 32706;

/// `\normalparindent`, `\normaltopskip`: 12pt and 10pt, fixed by the class.
const NORMAL_PARINDENT: i32 = 12 << 16;
const NORMAL_TOPSKIP: i32 = 10 << 16;

/// A page style of the next page out: `\thispagestyle{plain}` (at class load) or
/// `{firstpage}` (`\maketitle`).
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Special {
    Plain,
    FirstPage,
}

#[derive(PartialEq, Clone, Default)]
pub(super) struct Ams {
    title: Option<Vec<Token>>,
    short_title: Vec<Token>,
    authors: Vec<Vec<Token>>,
    short_authors: Vec<Vec<Token>>,
    /// `\addresses`: kind, optional argument, text.
    addresses: Vec<(u8, Vec<Token>, Vec<Token>)>,
    keywords: Option<Vec<Token>>,
    /// `\subjclass[edition]{text}`.
    subjclass: Option<(Vec<Token>, Vec<Token>)>,
    date: Option<Vec<Token>>,
    /// `abstract` filled `\abstractbox`.
    abstract_set: bool,
    titled: bool,
    /// The heads' texts after `\maketitle`'s `\markboth`: `\leftmark` (the authors,
    /// uppercased) and `\rightmark` (the short title); empty before it.
    left: Vec<Token>,
    right: Vec<Token>,
    pub(super) special: Option<Special>,
    /// The page being output has its head in `fancy::HEAD_BOX` and its foot typeset.
    pub(super) page: bool,
}

impl Ams {
    /// The class at `\documentclass`: `\pagestyle{headings}\thispagestyle{plain}`.
    pub(super) fn new() -> Ams {
        Ams { special: Some(Special::Plain), ..Ams::default() }
    }
}

/// `\uppercase` of a list (§1288): each character with a `\uccode` (here `a`–`z`) takes it;
/// `None` when the text has what `\uppercasenonmath` or `\MakeUppercase` treat otherwise:
/// math, a character above 127 (UTF-8 input), or `\ss`, `\i` and the other letters
/// `\upchars@` redefines, or a `\\`.
fn upper(list: &[Token], m: &Machine<'_, '_>) -> Option<Vec<Token>> {
    let mut out = Vec::with_capacity(list.len());
    for &t in list {
        match t.cat() {
            Some(Cat::MathShift) => return None,
            Some(_) if t.chr() as u32 > 127 => return None,
            Some(Cat::Letter | Cat::Other) if t.chr().is_ascii_lowercase() => out.push(Token::char(t.cat()?, t.chr().to_ascii_uppercase())),
            Some(_) => out.push(t),
            None => {
                let id = t.cs_id()?;
                if ["ss", "i", "j", "ae", "oe", "o", "aa", "l", "Mc", "\\", "thanks", "footnote", "and"].contains(&m.e.cs.name(id)) {
                    return None;
                }
                out.push(t);
            }
        }
    }
    Some(out)
}

impl Machine<'_, '_> {
    pub(super) fn ams_on(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.ams.is_some())
    }

    fn ams_mut(&mut self) -> Option<&mut Ams> {
        self.doc.as_mut().and_then(|d| d.ams.as_deref_mut())
    }

    /// `[opt]` when present.
    fn opt_arg(&mut self) -> Option<Vec<Token>> {
        self.next_is('[').then(|| self.bracket_arg())
    }

    /// `\title[short]{title}` (`\@dblarg`): `\shorttitle` is the title without one.
    pub(super) fn ams_title(&mut self) {
        let short = self.opt_arg();
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let Some(a) = self.ams_mut() else { return };
        a.short_title = short.unwrap_or_else(|| arg.clone());
        a.title = Some(arg);
    }

    /// `\author[short]{name}`: added to `\authors` (an `\and` between), a later one puts
    /// `\author{}` into `\addresses`; `\shortauthors` likewise (`\@dblarg`).
    pub(super) fn ams_author(&mut self) {
        let short = self.opt_arg();
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let Some(a) = self.ams_mut() else { return };
        if !a.authors.is_empty() {
            a.addresses.push((AUTHOR, Vec::new(), Vec::new()));
        }
        let short = short.unwrap_or_else(|| arg.clone());
        a.authors.push(arg);
        if !short.is_empty() {
            a.short_authors.push(short);
        }
    }

    /// `\address`, `\curraddr`, `\email`, `\urladdr` `[opt]{text}`: `\g@addto@macro
    /// \addresses{\address{#1}{#2}}` and kin.
    pub(super) fn ams_address(&mut self, kind: u8) {
        let opt = self.opt_arg().unwrap_or_default();
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        match self.ams_mut() {
            Some(a) => a.addresses.push((kind, opt, arg)),
            None => self.not_native("an amsart command outside amsart"),
        }
    }

    /// `\keywords{text}`, `\date{text}`.
    pub(super) fn ams_keywords(&mut self, date: bool) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let Some(a) = self.ams_mut() else { return self.not_native("an amsart command outside amsart") };
        if date {
            a.date = Some(arg);
        } else {
            a.keywords = Some(arg);
        }
    }

    /// `\subjclass[2020]{text}`: one of the editions amsart names (1991, 2000, 2010, 2020).
    pub(super) fn ams_subjclass(&mut self) {
        let ed = self.opt_arg().unwrap_or_else(|| "2020".chars().map(|c| Token::char(Cat::Other, c)).collect());
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let year: String = ed.iter().map(|t| t.chr()).collect();
        if !matches!(year.as_str(), "1991" | "2000" | "2010" | "2020") || ed.iter().any(|t| t.cat().is_none()) {
            return self.not_native("\\subjclass of an edition amsart does not name");
        }
        if let Some(a) = self.ams_mut() {
            a.subjclass = Some((ed, arg));
        }
    }

    /// `\begin{abstract}` (`\begin`'s group open): `\global\setbox\abstractbox=\vtop
    /// \bgroup \normalfont\Small \list{}{\labelwidth\z@ \leftmargin3pc \rightmargin
    /// \leftmargin \listparindent\normalparindent \itemindent\z@ \parsep\z@\@plus\p@}
    /// \item[\hskip\labelsep\scshape\abstractname.]`. A group of the native's own inside
    /// the box takes `\end`'s `\endgroup` that `end_list` does.
    pub(super) fn ams_abstract_begin(&mut self) {
        if self.ams_mut().is_none_or(|a| a.titled || a.abstract_set) {
            return self.not_native("amsart: an abstract after \\maketitle, or a second one");
        }
        if self.mode != Mode::Vertical {
            return self.not_native("amsart: an abstract in a paragraph");
        }
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.open_box(super::super::engine_box::VTOP_GROUP, super::super::engine_box::Ctx::Set { n: ABSTRACT_BOX, global: true }, false, 0, false);
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.normal_font();
        self.size_command(2);
        let list = List { label: Label::Empty, make: Make::Mklab, numbered: false, counter: 0, centercr: false, auto_item: false, endpe_false: false };
        if !self.list_open(list) {
            return;
        }
        self.set_rw(R_LABELWIDTH, 0, false);
        self.set_rw(R_LEFTMARGIN, 36 << 16, false);
        self.set_rw(R_RIGHTMARGIN, 36 << 16, false);
        self.set_rw(R_LISTPARINDENT, NORMAL_PARINDENT, false);
        self.set_rw(R_ITEMINDENT, 0, false);
        self.set_rg(R_PARSEP, Glue { stretch: 1 << 16, ..Glue::default() });
        self.trivlist(|m| {
            m.list_tail();
            let t = [m.doc_id(doc!("item")), Token::char(Cat::Other, '['), m.doc_id(doc!("rt@amsabsitem")), m.named_cs(named!("abstractname")), Token::char(Cat::Other, '.'), Token::char(Cat::Other, ']')];
            m.back_list(&t);
        });
    }

    /// `\end{abstract}`: `\endlist` (with the native's group), `\egroup` (the box), then
    /// `\end`'s `\endgroup` (`ams_abstract_end2`).
    pub(super) fn ams_abstract_end(&mut self) {
        let t = [Token::char(Cat::EndGroup, '}'), self.doc_id(doc!("rt@amsabsend2"))];
        self.back_list(&t);
        self.end_list();
    }

    pub(super) fn ams_abstract_end2(&mut self) {
        if let Some(a) = self.ams_mut() {
            a.abstract_set = true;
        }
        self.end_group(SEMI_SIMPLE_GROUP);
    }

    /// The tokens of `text` with `\uppercase`d letters, or a decline.
    fn ams_upper(&mut self, text: &[Token]) -> Option<Vec<Token>> {
        let up = upper(text, self);
        if up.is_none() {
            self.not_native("amsart: math, accents or \\\\ in an uppercased title or author");
        }
        up
    }

    /// `\nxandlist` of `names` with the three separators (`\andify`, `\author@andify`).
    fn ams_andify(&mut self, names: &[Vec<Token>], two: &str, mid: &str, last: &str) -> Vec<Token> {
        let mut t = Vec::new();
        for (k, n) in names.iter().enumerate() {
            if k > 0 {
                let sep = if names.len() == 2 { two } else if k + 1 == names.len() { last } else { mid };
                // The separators' `~` is the active character (`tex_tokens` reads it as text).
                t.extend(self.tex_tokens(sep).into_iter().map(|x| if x.cat().is_some() && x.chr() == '~' { Token::char(Cat::Active, '~') } else { x }));
            }
            t.extend_from_slice(n);
        }
        t
    }

    /// amsart's `\maketitle`: `\par`, `\thispagestyle{firstpage}`, `\uppercasenonmath
    /// \shorttitle`, `\andify\shortauthors`, then in a group `\@maketitle` and
    /// `\markboth{\MakeUppercase{\shortauthors}}{\shorttitle}`; `\c@footnote\z@`.
    /// `\@maketitle`: `\normalfont\normalsize`, `\@adminfootnotes` (date, subject class,
    /// keywords: footnotes without marks), `\@mkboth` (amsart's own `\markboth`, one
    /// `\mark`), `\global\topskip42\p@`, `\@settitle` (`center`, `\baselineskip14\p@`,
    /// `\bfseries`, the title uppercased), `\@setauthors`, `\@setabstract`, `\normalsize`,
    /// `\vskip` 34pt less `\baselineskip`.
    pub(super) fn ams_make_title(&mut self) {
        let Some(doc) = self.doc.as_ref() else { return };
        if doc.page != 1 {
            return self.not_native("amsart: \\maketitle after the first page");
        }
        let Some(a) = doc.ams.as_deref() else { return };
        let Some(title) = a.title.clone() else { return self.not_native("amsart: \\maketitle without \\title") };
        if a.titled || a.authors.is_empty() {
            return self.not_native("amsart: a second \\maketitle, or one without \\author");
        }
        let (short_title, authors, short_authors) = (a.short_title.clone(), a.authors.clone(), a.short_authors.clone());
        let notes: Vec<Vec<Token>> = [a.date.clone().map(|d| (0, d, Vec::new())), a.subjclass.clone().map(|(e, s)| (1, s, e)), a.keywords.clone().map(|k| (2, k, Vec::new()))].into_iter().flatten().filter(|(_, text, _)| !text.is_empty()).map(|(k, text, ed)| {
            let mut t = Vec::new();
            match k {
                // `\@setdate`: `\datename\ \@date\@addpunct.`, `\datename` `\textit{Date}:`.
                0 => t.extend(self.tex_tokens("\\textit{Date}:\\ ")),
                // `\@setsubjclass`: `{\itshape\subjclassname.}\enspace\@subjclass\@addpunct.`,
                // `\subjclassname` `\textup{2020} Mathematics Subject Classification`.
                1 => {
                    t.extend(self.tex_tokens("{\\itshape\\textup{"));
                    t.extend(ed);
                    t.extend(self.tex_tokens("} Mathematics Subject Classification.}\\enspace "));
                }
                // `\@setkeywords`: `{\itshape \keywordsname.}\enspace \@keywords\@addpunct.`.
                _ => t.extend(self.tex_tokens("{\\itshape Key words and phrases.}\\enspace ")),
            }
            t.extend(text);
            t.push(self.doc_id(doc!("rt@addpunct")));
            t
        }).collect();
        // `\uppercasenonmath`: `{\protect\@upprep TEXT}`.
        let Some(up_title) = self.ams_upper(&title) else { return };
        let Some(up_short) = self.ams_upper(&short_title) else { return };
        let andified = self.ams_andify(&authors, "\\unskip{} and~", "\\unskip,\\penalty-1{} \\ignorespaces ", "\\unskip,\\penalty-2{} and~");
        let Some(up_authors) = self.ams_upper(&andified) else { return };
        let short = self.ams_andify(&short_authors, "\\unskip{} and~", "\\unskip, ", "\\unskip, and~");
        let Some(up_short_authors) = self.ams_upper(&short) else { return };
        // In the heads `~` is the kernel's `\nobreakspace{}` (`\@resetactivechars`).
        let tilde = Token::char(Cat::Active, '~');
        let nbsp = self.named_cs(named!("nobreakspace"));
        let left: Vec<Token> = up_short_authors.iter().flat_map(|&t| if t == tilde { vec![nbsp, Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}')] } else { vec![t] }).collect();
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut right = vec![open, self.doc_id(doc!("rt@amsupprep"))];
        right.extend(up_short);
        right.push(close);
        if let Some(a) = self.ams_mut() {
            a.titled = true;
            a.special = Some(Special::FirstPage);
            a.left = left;
            a.right = right;
        }
        let mut t = vec![self.named_cs(named!("par")), self.named_cs(named!("begingroup"))];
        t.extend(self.tex_tokens("\\normalfont\\normalsize "));
        // `\@footnotetext{…}` with `\@makefnmark` `\relax`.
        for n in notes {
            t.push(self.named_cs(named!("insert")));
            t.extend(FOOTINS.to_string().chars().map(|c| Token::char(Cat::Other, c)));
            t.extend([open, self.doc_id(doc!("rt@amsfn"))]);
            t.extend(n);
            t.extend([self.doc_id(doc!("rt@amsfnend"))]);
        }
        t.push(self.doc_id(doc!("rt@amsmark")));
        t.extend(self.tex_tokens("\\global\\topskip42pt\\relax\\begin{center}\\baselineskip14pt\\relax\\bfseries{"));
        t.push(self.doc_id(doc!("rt@amsupprep")));
        t.extend(up_title);
        t.push(close);
        t.extend(self.tex_tokens("\\end{center}"));
        // `\@setauthors`: `\begingroup \trivlist \centering\footnotesize \@topsep30\p@
        // \advance\@topsep-\baselineskip \item\relax \MakeUppercase{\authors}
        // \endtrivlist\endgroup`.
        t.push(self.doc_id(doc!("rt@amsauthors")));
        t.extend(up_authors);
        t.push(self.doc_id(doc!("rt@amsendauthors")));
        t.push(self.doc_id(doc!("rt@amssetabstract")));
        t.extend(self.tex_tokens("\\normalsize "));
        t.extend([self.doc_id(doc!("rt@amstitleend")), self.named_cs(named!("endgroup")), self.doc_id(doc!("rt@fnreset"))]);
        self.back_list(&t);
    }

    /// `\@footnotetext`'s setup in the insertion: `\normalfont\footnotesize
    /// \interlinepenalty\interfootnotelinepenalty \splittopskip\footnotesep \splitmaxdepth
    /// \dp\strutbox \floatingpenalty\@MM \hsize\columnwidth \@parboxrestore
    /// \parindent\normalparindent \sloppy` (amsart's: `\tolerance9999
    /// \emergencystretch3em`), then `\@makefntext` (`\indent\@makefnmark`, the mark
    /// `\relax`) and `\rule\z@\footnotesep\ignorespaces`.
    pub(super) fn ams_fn_setup(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        doc.footnote_flags.push(doc.nobreak);
        doc.nobreak = false;
        let width = doc.class.dimen("textwidth");
        self.normal_font();
        self.size_command(2);
        // `\interfootnotelinepenalty` is the kernel's 100.
        self.set_int_named(param!("interlinepenalty"), 100);
        let sep = self.rw(R_FOOTNOTESEP);
        self.set_glue_named(param!("splittopskip"), Glue { width: sep, ..Glue::default() });
        let strut_depth = self.strut_depth();
        self.set_dimen_named(param!("splitmaxdepth"), strut_depth);
        self.set_int_named(param!("floatingpenalty"), 20000);
        self.set_dimen_named(param!("hsize"), width);
        // `\@arrayparboxrestore`.
        if let Some((id, m)) = self.doc.as_ref().map(|d| (d.par_id, d.par_prim)) {
            self.e.eqtb.set_meaning(id, m, false);
        }
        self.set_everypar(0);
        self.set_rw(R_LINEWIDTH, width, false);
        self.set_rw(R_TOTALLEFTMARGIN, 0, false);
        for r in [param!("parskip"), param!("leftskip"), param!("rightskip")] {
            self.set_glue_named(r, Glue::default());
        }
        self.set_rg(R_ATRIGHTSKIP, Glue::default());
        self.set_glue_named(param!("parfillskip"), Glue { stretch: 65536, stretch_order: 1, ..Glue::default() });
        let one = self.dimen_of("1pt");
        self.set_glue_named(param!("lineskip"), Glue { width: one, ..Glue::default() });
        let b = self.e.eqtb.glue(super::NORMAL_BASELINE_SKIP);
        self.set_glue_named(param!("baselineskip"), b);
        self.set_dimen_named(param!("parindent"), NORMAL_PARINDENT);
        self.set_int_named(param!("tolerance"), 9999);
        let e = self.dimen_of("3em");
        self.set_dimen_named(param!("emergencystretch"), e);
        let mut list = vec![self.named_cs(named!("indent")), self.named_cs(named!("hbox"))];
        list.push(Token::char(Cat::BeginGroup, '{'));
        list.push(self.named_cs(named!("vrule")));
        for (k, v) in [("width", 0), ("height", sep), ("depth", 0)] {
            self.word_tokens(k, &mut list);
            Self::sp_tokens(v, &mut list);
        }
        list.extend([Token::char(Cat::EndGroup, '}'), self.named_cs(named!("ignorespaces"))]);
        self.back_list(&list);
    }

    /// `\unskip\strut\par` and the insertion's `}`.
    pub(super) fn ams_fn_end(&mut self) {
        self.unskip();
        if self.mode == Mode::Horizontal {
            let (h, d) = (self.strut_height(), self.strut_depth());
            let r = self.e.nodes.rule_node([0, h, d]);
            self.cur.push(r);
        }
        self.par();
        if let Some(doc) = self.doc.as_mut() {
            if let Some(nobreak) = doc.footnote_flags.pop() {
                doc.nobreak = nobreak;
            }
        }
        self.input.back(Token::char(Cat::EndGroup, '}'));
    }

    /// `\@upprep`: `\spaceskip1.3\fontdimen2\font plus1.3\fontdimen3\font` (§455: `1.3` is
    /// the unit plus §452's fraction 19661, by `xn_over_d`).
    pub(super) fn ams_upprep(&mut self) {
        let p = self.e.fonts.params(self.cur_font());
        let f = |x: i32| x + crate::backend::scaled::xn_over_d(x, 19661, 0x10000);
        let (w, s) = (f(p.get(1).copied().unwrap_or(0)), f(p.get(2).copied().unwrap_or(0)));
        self.set_glue_named(param!("spaceskip"), Glue { width: w, stretch: s, ..Glue::default() });
    }

    /// `\@mkboth{\shortauthors}{\shorttitle}`: amsart's `\markboth`, one `\mark` (its text
    /// is read by no head of the native job: the kernel's marks below are).
    pub(super) fn ams_mark(&mut self, class: u16) {
        let span = self.e.toks.store(&[]);
        self.cur.push(Node { kind: Kind::Mark, sub: 0, font: class, a: span.start as i32, b: span.end as i32 });
    }

    /// `\@setauthors` up to the authors: `\begingroup \trivlist` (amsart's: `\parsep
    /// \parskip`), `\centering\footnotesize \@topsep30\p@ \advance\@topsep-\baselineskip
    /// \item\relax`.
    pub(super) fn ams_authors(&mut self) {
        self.begin_group(SEMI_SIMPLE_GROUP);
        let parskip = self.glue_param(super::super::engine_pack::PAR_SKIP);
        self.set_rg(R_PARSEP, parskip);
        if let Some(doc) = self.doc.as_mut() {
            doc.lists.push(List { label: Label::Empty, make: Make::Plain, numbered: false, counter: 0, centercr: true, auto_item: false, endpe_false: false });
        }
        self.trivlist(|m| {
            for r in [R_LABELWIDTH, R_LEFTMARGIN, R_ITEMINDENT] {
                m.set_rw(r, 0, false);
            }
            m.ragged_skips(0);
            m.size_command(2);
            let b = m.glue_param(super::super::engine_pack::BASELINE_SKIP).width;
            m.set_rg(R_ATTOPSEP, Glue { width: (30 << 16) - b, ..Glue::default() });
            if let Some(doc) = m.doc.as_mut() {
                doc.noitemarg = true;
            }
            m.item_start(None);
        });
    }

    /// `\@setabstract`'s `\@setabstracta`: with an abstract, `\skip@20\p@
    /// \advance\skip@-\lastskip \advance\skip@-\baselineskip \vskip\skip@ \box\abstractbox
    /// \prevdepth\z@`.
    pub(super) fn ams_set_abstract(&mut self) {
        let Some(b) = self.e.eqtb.box_reg(ABSTRACT_BOX) else { return };
        let last = self.last_skip();
        let bs = self.glue_param(super::super::engine_pack::BASELINE_SKIP);
        let g = super::glue_add(super::glue_add(Glue { width: 20 << 16, ..Glue::default() }, super::neg(last)), super::neg(bs));
        self.vskip(g);
        self.e.eqtb.replace_box(ABSTRACT_BOX, None);
        let r = self.e.nodes.boxes[b as usize];
        self.e.nodes.free_box(b);
        let b = self.e.nodes.add_box(r);
        self.append_to_vlist(b);
        self.aux = 0;
    }

    /// The end of `\@maketitle`: `\dimen@34\p@ \advance\dimen@-\baselineskip
    /// \vskip\dimen@`; then the kernel's `\markboth` (`\mark_insert:nn` of `2e-left`,
    /// `2e-right` and `2e-right-nonempty`: three marks).
    pub(super) fn ams_title_end(&mut self) {
        let b = self.glue_param(super::super::engine_pack::BASELINE_SKIP).width;
        self.vskip(Glue { width: (34 << 16) - b, ..Glue::default() });
        for class in [1, 2, 3] {
            self.ams_mark(class);
        }
    }

    /// amsart's `~` (`\nonbreakingspace`): `\unskip\nobreak\ \ignorespaces`.
    pub(super) fn ams_tie(&mut self) {
        if self.mode != Mode::Horizontal {
            return self.not_native("amsart: ~ outside a paragraph");
        }
        self.unskip();
        self.cur.push(Node::new(Kind::Penalty, 10000));
        self.build_control_space();
        self.ignore_spaces();
    }

    /// The head and foot of the page being output, as tokens (`start_foot`): the head into
    /// `fancy::HEAD_BOX`, the foot into the output nest, in `\@outputpage`'s
    /// `\reset@font\normalsize`. `firstpage`: head `\@serieslogo\hss` (no logo), foot
    /// `\normalfont\scriptsize \hfil\thepage\hfil \global\topskip\normaltopskip`; `plain`:
    /// the same foot, no head, and `\ps@empty`'s `\global\topskip\normaltopskip`;
    /// `headings`: `\normalfont\scriptsize \hfil \rightmark{}{}\hfil \llap{\thepage}` on an
    /// odd page, `\rlap{\thepage}\hfil \leftmark{}{}\hfil` on an even one, no foot.
    pub(super) fn ams_page_tokens(&mut self, page: i32) -> Vec<Token> {
        let Some(a) = self.ams_mut() else { return Vec::new() };
        a.page = true;
        let special = a.special.take();
        let (left, right) = (a.left.clone(), a.right.clone());
        let Some(class) = self.doc.as_ref().map(|d| d.class) else { return Vec::new() };
        let mut w = String::new();
        super::super::engine_value::print_scaled(class.dimen("textwidth"), &mut w);
        let digits = page.to_string();
        let mut t = self.tex_tokens(&format!("\\begingroup\\normalfont\\normalsize\\global\\setbox{}\\hbox to{w}pt{{", super::fancy::HEAD_BOX));
        match special {
            Some(Special::FirstPage) => t.extend(self.tex_tokens("\\hss}")),
            Some(Special::Plain) => t.extend(self.tex_tokens("}")),
            None if page % 2 == 1 => {
                t.extend(self.tex_tokens("\\normalfont\\scriptsize\\hfil "));
                t.extend(right);
                t.extend(self.tex_tokens(&format!("\\hfil\\hbox to0pt{{\\hss {digits}}}}}")));
            }
            None => {
                t.extend(self.tex_tokens(&format!("\\normalfont\\scriptsize\\hbox to0pt{{{digits}\\hss}}\\hfil ")));
                t.extend(left);
                t.extend(self.tex_tokens("\\hfil}"));
            }
        }
        if special.is_some() {
            let mut s = String::new();
            super::super::engine_value::print_scaled(NORMAL_TOPSKIP, &mut s);
            t.extend(self.tex_tokens(&format!("\\normalfont\\scriptsize\\hfil {digits}\\hfil\\global\\topskip{s}pt\\relax ")));
        }
        t.extend(self.tex_tokens("\\endgroup "));
        t
    }

    /// amsart's `\footnoterule`: `\kern-.4\p@ \hrule\@width 5pc\kern11\p@\kern-\footnotesep`.
    pub(super) fn ams_footnote_rule(&mut self) {
        let k = self.dimen_of("-.4pt");
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, k) });
        let rule = self.e.nodes.rule_node([60 << 16, 26214, 0]);
        self.cur.push(rule);
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, 11 << 16) });
        let sep = self.rw(R_FOOTNOTESEP);
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, -sep) });
    }

    /// `\enddoc@text` at `\end{document}` (`\AtEndDocument`): `\@setaddresses` when there
    /// are addresses: `\par \nobreak \begingroup \footnotesize \interlinepenalty\@M`, then
    /// each entry: `\author` `\nobreak\addvspace\bigskipamount`; `\address` `\begingroup
    /// \par\addvspace\bigskipamount\indent \@ifnotempty{#1}{(\ignorespaces#1\unskip) }
    /// {\scshape\ignorespaces#2}\par\endgroup`; `\curraddr`, `\email`, `\urladdr`
    /// (non-empty) `\begingroup \nobreak\indent` `{\itshape Current address}`, `Email
    /// address`, `URL` `\@ifnotempty{#1}{, \ignorespaces#1\unskip}\/:\space` (`\ttfamily`
    /// for the last two) `#2\par\endgroup`; `\endgroup`.
    pub(super) fn ams_end_tokens(&mut self) -> Vec<Token> {
        let Some(a) = self.ams_mut() else { return Vec::new() };
        let entries = std::mem::take(&mut a.addresses);
        if entries.is_empty() {
            return Vec::new();
        }
        if entries.iter().any(|(_, o, x)| o.iter().chain(x).any(|t| t.cs_id().is_some_and(|id| ["\\", "thanks", "footnote"].contains(&self.e.cs.name(id))) || t.cat() == Some(Cat::Active))) {
            self.not_native("amsart: \\\\ or ~ in an address");
            return Vec::new();
        }
        let mut t = self.tex_tokens("\\par");
        t.push(self.doc_id(doc!("nobreak")));
        t.extend(self.tex_tokens("\\begingroup\\footnotesize\\interlinepenalty10000 "));
        for (kind, opt, text) in entries {
            match kind {
                AUTHOR => t.extend([self.doc_id(doc!("nobreak")), self.doc_id(doc!("rt@amsbigskip"))]),
                ADDRESS => {
                    t.extend(self.tex_tokens("\\begingroup\\par"));
                    t.push(self.doc_id(doc!("rt@amsbigskip")));
                    t.extend(self.tex_tokens("\\indent "));
                    if !opt.is_empty() {
                        t.extend(self.tex_tokens("(\\ignorespaces "));
                        t.extend(opt);
                        t.extend(self.tex_tokens("\\unskip) "));
                    }
                    t.extend(self.tex_tokens("{\\scshape\\ignorespaces "));
                    t.extend(text);
                    t.extend(self.tex_tokens("}\\par\\endgroup "));
                }
                _ if text.is_empty() => {}
                _ => {
                    t.extend(self.tex_tokens("\\begingroup"));
                    t.push(self.doc_id(doc!("nobreak")));
                    t.extend(self.tex_tokens(match kind {
                        CURRADDR => "\\indent{\\itshape Current address}",
                        EMAIL => "\\indent{\\itshape Email address}",
                        _ => "\\indent{\\itshape URL}",
                    }));
                    if !opt.is_empty() {
                        t.extend(self.tex_tokens(", \\ignorespaces "));
                        t.extend(opt);
                        t.extend(self.tex_tokens("\\unskip"));
                    }
                    t.extend(self.tex_tokens("\\/: "));
                    if kind != CURRADDR {
                        t.push(self.doc_id(doc!("ttfamily")));
                    }
                    t.extend(text);
                    t.extend(self.tex_tokens("\\par\\endgroup "));
                }
            }
        }
        t.push(self.named_cs(named!("endgroup")));
        t
    }

    /// `\addvspace\bigskipamount`, `\bigskipamount` as the size command in force set it
    /// (`\@adjustvertspacing`: `.7\baselineskip plus.7\baselineskip`, §452's 45875).
    pub(super) fn ams_big_skip(&mut self) {
        let b = self.glue_param(super::super::engine_pack::BASELINE_SKIP).width;
        let v = crate::backend::scaled::xn_over_d(b, 45875, 0x10000);
        self.add_vspace(Glue { width: v, stretch: v, ..Glue::default() });
    }

    /// `\hskip\labelsep\scshape` in the abstract's label.
    pub(super) fn ams_abstract_item(&mut self) {
        self.label_sep();
        self.font_declaration(Change::Shape("sc"), false);
    }
}
