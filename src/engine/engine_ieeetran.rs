//! IEEEtran.cls (V1.8b, 2015/08/26) on the GDL path (`2026-10-31-w8-journal-classes.md`):
//! the `journal` and `conference` modes in two columns, their `\maketitle` (`\twocolumn[
//! \IEEEquantizevspace{\@maketitle}…]`), the `abstract` and `IEEEkeywords` environments,
//! `\section` and `\subsection` (`\@sect` with `\@seccntformat` `\thesectiondis\hskip0.5em`),
//! `\appendix`, the page styles `headings` and `IEEEtitlepagestyle`, and `\cite`'s undefined
//! key.
//!
//! The class's parameters (`\@setfontsize` sizes, the page of `\IEEEquantizetextheight` and
//! `\IEEEsettopmargin`, the two-column `\sloppy\flushbottom`, `\@IEEEtunefonts`'s Times
//! `\fontdimen`s) are the class table (`rushtex-gen class --class IEEEtran`, the `conference`
//! option in the table's landscape bit). What is code is here: IEEEtran.cls's macros as
//! tokens of commands the native job has, with the document's own text spliced in.
//!
//! Not native yet (each stops in the pre-scan or declines): `technote`, `peerreview`,
//! `compsoc`, `transmag`, `draftcls`, one column, two-sided heads, `\thanks`, footnotes,
//! floats, lists, `\IEEEPARstart`, `\markboth`, displayed math, `\label`/`\ref`,
//! `thebibliography`, more than one key in a `\cite`.

use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::Glue;
use super::super::engine_run::{Machine, SEMI_SIMPLE_GROUP};
use super::super::engine_token::Token;
use super::{CLEAR_PAGE_TAIL, NEW_PAGE};

/// The box register holding `\@IEEEquantizeboxA` (a register no document can name).
const QUANT_BOX: u32 = 32705;

/// `\columnsep 1pc` (IEEEtran.cls: "2 x 21pc + 1pc = 43pc"; compsoc is not native).
pub(super) const COLUMNSEP: i32 = 12 << 16;

/// `\small`: the class's `\@IEEEabskeysecsize` for 10pt to 12pt.
const SMALL: usize = 3;

#[derive(PartialEq, Clone, Default)]
pub(super) struct Ieee {
    /// The `conference` option (`\ifCLASSOPTIONconference`).
    pub(super) conference: bool,
    /// `\@ptsize`: 0, 1 or 2 for 10pt, 11pt and 12pt (`\sublargesize`).
    size: usize,
    /// `\appendix` ran: `\section` throws its argument away (`\@IEEEdestroythesectionargument`).
    pub(super) appendix: bool,
    /// `\maketitle` ran.
    titled: bool,
    /// `\@IEEEquantizeheightC`: the skip after the title.
    quant: i32,
    /// The page being output has its head in `fancy::HEAD_BOX`.
    pub(super) page: bool,
}

impl Ieee {
    pub(super) fn new(conference: bool, size: usize) -> Ieee {
        Ieee { conference, size, ..Ieee::default() }
    }
}

/// `\Roman`: `\@Roman` of `n` (`\romannumeral` uppercased).
fn roman(mut n: i32) -> String {
    let mut s = String::new();
    for (v, r) in [(1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")] {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

impl Machine<'_, '_> {
    pub(super) fn ieee_on(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.ieee.is_some())
    }

    fn ieee_mut(&mut self) -> Option<&mut Ieee> {
        self.doc.as_mut().and_then(|d| d.ieee.as_deref_mut())
    }

    fn ieee_conference(&self) -> bool {
        self.doc.as_ref().and_then(|d| d.ieee.as_ref()).is_some_and(|i| i.conference)
    }

    /// Characters as tokens: letters are letters, the rest other characters, a space a space.
    fn ieee_text(&self, s: &str, t: &mut Vec<Token>) {
        t.extend(s.chars().map(|c| match c {
            ' ' => Token::char(Cat::Space, ' '),
            c if c.is_ascii_alphabetic() => Token::char(Cat::Letter, c),
            c => Token::char(Cat::Other, c),
        }));
    }

    /// `\@maketitle` (journal and conference, not compsoc, not transmag, not technote):
    ///
    /// `\newpage \bgroup\par\vskip\IEEEtitletopspace\vskip\IEEEtitletopspaceextra\centering
    /// \vskip0.2em{\Huge\@title\par}\relax \vskip1.0em\par` then the journal's
    /// `{\lineskip.5em\sublargesize\@author\@IEEEspecialpapernotice\par{}}\relax`, and
    /// `\par\addvspace{0.5\baselineskip}\egroup`. `\IEEEtitletopspace` is
    /// `0.5\baselineskip`, `\IEEEtitletopspaceextra` `0pt`, `\@IEEEspecialpapernotice`
    /// `\relax`. Without `newpage` it is the measured object, whose
    /// `\IEEEquantizedisabletitlecmds` make `\newpage` `\relax`.
    fn ieee_title_body(&mut self, title: &[Token], author: &[Token], newpage: bool, t: &mut Vec<Token>) {
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let cs = |m: &Self, n: usize| m.named_cs(n);
        if newpage {
            t.push(self.doc_id(NEW_PAGE));
        }
        t.extend([open, cs(self, named!("par")), cs(self, named!("vskip"))]);
        self.ieee_text("0.5", t);
        t.extend([cs(self, named!("baselineskip")), cs(self, named!("vskip"))]);
        self.ieee_text("0pt ", t);
        t.extend([self.doc_id(doc!("centering")), cs(self, named!("vskip"))]);
        self.ieee_text("0.2em", t);
        t.extend([open, self.doc_id(doc!("Huge"))]);
        t.extend_from_slice(title);
        t.extend([cs(self, named!("par")), close, cs(self, named!("relax")), cs(self, named!("vskip"))]);
        self.ieee_text("1.0em", t);
        t.push(cs(self, named!("par")));
        t.extend([open, cs(self, named!("lineskip"))]);
        self.ieee_text(".5em", t);
        t.push(self.doc_id(doc!("rt@ieeesublarge")));
        t.extend_from_slice(author);
        t.extend([cs(self, named!("relax")), cs(self, named!("par")), open, close, close, cs(self, named!("relax")), cs(self, named!("par")), self.doc_id(doc!("rt@ieeetitleend")), close]);
    }

    /// IEEEtran's `\maketitle` in two columns (not technote, not peerreview): `\par
    /// \begingroup\normalfont` … `\normalsize \twocolumn[{\IEEEquantizevspace{\@maketitle}
    /// [\IEEEquantizedisabletitlecmds]{0pt}[-\topskip]{\baselineskip}{\@IEEENORMtitlevspace}
    /// {\@IEEEMINtitlevspace}\@IEEEaftertitletext}] \thispagestyle{IEEEtitlepagestyle}
    /// \@thanks \endgroup \setcounter{footnote}{0}`. The `\footnotesize\footnotesep` of its
    /// group serve `\thanks`, which is not native.
    pub(super) fn ieee_make_title(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        if !doc.floats.twocol {
            return self.not_native("IEEEtran: \\maketitle in one column");
        }
        let [title, author, _] = std::mem::take(&mut doc.stored);
        let authored = doc.authored;
        let Some(i) = self.ieee_mut() else { return };
        if std::mem::replace(&mut i.titled, true) {
            return self.not_native("IEEEtran: a second \\maketitle");
        }
        if i.conference {
            return self.not_native("IEEEtran: the conference title (\\@IEEEauthorhalign)");
        }
        let author = if authored { author } else { Vec::new() };
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let cs = |m: &Self, n: usize| m.named_cs(n);
        let mut t = vec![cs(self, named!("par")), cs(self, named!("begingroup")), self.doc_id(doc!("normalfont")), self.doc_id(doc!("normalsize"))];
        t.extend([self.doc_id(NEW_PAGE), self.doc_id(CLEAR_PAGE_TAIL), self.doc_id(doc!("rt@topnewpage"))]);
        // `{\IEEEquantizevspace`: its `\begingroup`, the object in `\@IEEEquantizeboxA`.
        t.extend([open, cs(self, named!("begingroup")), cs(self, named!("setbox"))]);
        self.ieee_text(&QUANT_BOX.to_string(), &mut t);
        t.extend([cs(self, named!("vbox")), open, cs(self, named!("begingroup"))]);
        self.ieee_title_body(&title, &author, false, &mut t);
        t.extend([cs(self, named!("relax")), cs(self, named!("endgroup")), close, self.doc_id(doc!("rt@ieeequant")), cs(self, named!("endgroup"))]);
        // `\@IEEEquantizeobjectout\relax \vskip\@IEEEquantizeheightC`.
        self.ieee_title_body(&title, &author, true, &mut t);
        t.extend([cs(self, named!("relax")), self.doc_id(doc!("rt@ieeequantskip")), close]);
        t.extend([self.doc_id(doc!("rt@topnewpageend")), cs(self, named!("endgroup")), self.doc_id(doc!("rt@fnreset"))]);
        self.back_list(&t);
    }

    /// `\par\addvspace{0.5\baselineskip}` at `\@maketitle`'s end.
    pub(super) fn ieee_title_end(&mut self) {
        let b = self.glue_param(super::super::engine_pack::BASELINE_SKIP).width;
        let g = Glue { width: self.ieee_factor("0.5", b), ..Glue::default() };
        self.add_vspace(g);
    }

    /// `factor\dimen` as TeX scans it (§455: `<factor><internal dimen>`).
    fn ieee_factor(&self, factor: &str, v: i32) -> i32 {
        let (em, ex) = self.em_ex();
        super::super::engine_scan::scan_dimen_ref(&format!("{factor}\\x"), em, ex, &|_| Some(Glue { width: v, ..Glue::default() })).unwrap_or(0)
    }

    /// `\@ixIEEEquantizevspace` for the title: the object's height (`\ht` of the `\vbox`);
    /// its `\vtop` has height 0 (the object starts with `\vskip`); `\prevdepth` is -1000pt
    /// at the top of `\@topnewpage`'s box, so the first line's height is irrelevant. Then
    /// with offset `-\topskip`, nominal and minimum `2.5\baselineskip` and `2\baselineskip`
    /// (journal), unit `\baselineskip`: the multiple of the unit closest to the height plus
    /// the nominal skip, no less than the minimum.
    pub(super) fn ieee_quant(&mut self) {
        let h = self.e.eqtb.box_reg(QUANT_BOX).map_or(0, |b| self.e.nodes.boxes[b as usize].height);
        if self.aux > super::super::engine_node::IGNORE_DEPTH {
            return self.not_native("IEEEtran: a title after material");
        }
        let unit = self.glue_param(super::super::engine_pack::BASELINE_SKIP).width;
        let (nom, min) = if self.ieee_conference() { (self.ieee_factor("1", unit), self.ieee_factor("0.75", unit)) } else { (self.ieee_factor("2.5", unit), self.ieee_factor("2", unit)) };
        let topskip = self.glue_param(super::super::engine_page::TOP_SKIP).width;
        if unit == 0 {
            return self.not_native("IEEEtran: a zero \\baselineskip");
        }
        let a = h - topskip + nom;
        let multiple = a / unit;
        let a = a - unit * multiple;
        let b = unit - a;
        let mut c = if a < b { nom - a } else { nom + b };
        if c < min {
            c += a + b;
        }
        if let Some(i) = self.ieee_mut() {
            i.quant = c;
        }
    }

    pub(super) fn ieee_quant_skip(&mut self) {
        let c = self.ieee_mut().map_or(0, |i| i.quant);
        self.build_glue(Glue { width: c, ..Glue::default() }, false);
    }

    /// `\sublargesize`: `\@setfontsize{\sublargesize}{11}{13.4pt}` at 10pt, `{12}{14pt}` at
    /// 11pt, `{14}{17pt}` at 12pt.
    pub(super) fn ieee_sublarge(&mut self) {
        let size = self.doc.as_ref().and_then(|d| d.ieee.as_ref()).map_or(0, |i| i.size);
        let (f, b) = [("11", "13.4pt"), ("12", "14pt"), ("14", "17pt")][size.min(2)];
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![self.doc_id(doc!("fontsize")), open];
        self.ieee_text(f, &mut t);
        t.extend([close, open]);
        self.ieee_text(b, &mut t);
        t.extend([close, self.named_cs(named!("selectfont"))]);
        self.back_list(&t);
    }

    /// `\abstract` and `\IEEEkeywords` in two columns: `\normalfont \@IEEEabskeysecsize
    /// \bfseries\textit{\abstractname}---\relax \@IEEEgobbleleadPARNLSP`, in `\begin`'s
    /// group (`\IEEEkeywordsname` is `Index Terms`).
    pub(super) fn ieee_abstract_begin(&mut self, keywords: bool) {
        if !self.two_columns() {
            return self.not_native("IEEEtran: an abstract in one column");
        }
        self.begin_group(SEMI_SIMPLE_GROUP);
        if let Some(doc) = self.doc.as_mut() {
            doc.endpe = false;
        }
        self.normal_font();
        self.size_command(SMALL);
        self.font_change(super::Change::Series("bx"));
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![self.doc_id(doc!("textit")), open];
        if keywords {
            self.ieee_text("Index Terms", &mut t);
        } else {
            t.push(self.named_cs(named!("abstractname")));
        }
        t.push(close);
        self.ieee_text("---", &mut t);
        t.extend([self.named_cs(named!("relax")), self.doc_id(doc!("rt@ieeegobble"))]);
        self.back_list(&t);
    }

    /// `\endabstract`: `\vspace{1.34ex}` (`{0ex}` in conference mode) `\par`;
    /// `\endIEEEkeywords`: `\vspace{0.67ex}\par`. Their `\normalfont\normalsize` are undone by
    /// `\end`'s `\endgroup`, which follows.
    pub(super) fn ieee_abstract_end(&mut self, keywords: bool) {
        let amount = if keywords { "0.67ex" } else if self.ieee_conference() { "0ex" } else { "1.34ex" };
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![self.doc_id(doc!("vspace")), open];
        self.ieee_text(amount, &mut t);
        t.extend([close, self.named_cs(named!("par")), self.doc_id(doc!("rt@ieeeabsend"))]);
        self.back_list(&t);
    }

    /// `\@IEEEgobbleleadPARNLSP`: the spaces, `\par`s and `\\`s before the first other token.
    pub(super) fn ieee_gobble(&mut self) {
        let (par, nl) = (self.named_id(named!("par")), self.named_id(named!("\\")));
        while let Some(t) = self.get_token() {
            let skip = t.cat() == Some(Cat::Space) || t.cs_id().is_some_and(|id| id == par || id == nl || self.e.cs.name(id) == " ");
            if !skip {
                self.input.back(t);
                return;
            }
        }
    }

    /// The `\@startsection` skips of `\section` (level 1) and `\subsection` (2): journal
    /// `3.0ex plus 1.5ex minus 1.5ex` and `3.5ex plus 1.5ex minus 1.5ex`, conference
    /// `1.5ex plus 1.5ex minus 0.5ex` for both; after `0.7ex plus 1ex minus 0ex` and
    /// `0.7ex plus .5ex minus 0ex`.
    pub(super) fn ieee_section_skips(&self, level: usize) -> (Glue, Glue) {
        let before = match (self.ieee_conference(), level) {
            (true, _) => "1.5ex plus 1.5ex minus 0.5ex",
            (false, 0) => "3.0ex plus 1.5ex minus 1.5ex",
            _ => "3.5ex plus 1.5ex minus 1.5ex",
        };
        let after = if level == 0 { "0.7ex plus 1ex minus 0ex" } else { "0.7ex plus .5ex minus 0ex" };
        (self.glue_of(before), self.glue_of(after))
    }

    /// IEEEtran's `\@sect` (numbered) and `\@ssect` (starred) for a display heading, after
    /// `\refstepcounter`, in the native's group (`\begingroup`): `#6{\relax\noindent{\hskip
    /// \z@\relax\@svsec}{\interlinepenalty\@M #8\@@par}}` with `\@svsec` `\thesectiondis
    /// \hskip0.5em\relax` (`I.`, `\thesubsectiondis` `A.`), `#6` `\normalfont\normalsize
    /// \centering\scshape` (`\section`) or `\normalfont\normalsize\itshape`; `\@ssect`:
    /// `\noindent #4{\relax{\hskip\z@}{\interlinepenalty\@M #5\@@par}}`. Then the native's
    /// `\endgroup`, `\addcontentsline` and `\@xsect` (`section_end`).
    pub(super) fn ieee_section_start(&mut self, level: usize, numbered: bool, count: i32) {
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let cs = |m: &Self, n: usize| m.named_cs(n);
        let mut t = Vec::new();
        let font = |m: &mut Self, t: &mut Vec<Token>| {
            t.extend([m.doc_id(doc!("normalfont")), m.doc_id(doc!("normalsize"))]);
            if level == 0 {
                t.extend([m.doc_id(doc!("centering")), m.doc_id(doc!("scshape"))]);
            } else {
                t.push(m.doc_id(doc!("itshape")));
            }
        };
        if !numbered {
            t.push(cs(self, named!("noindent")));
        }
        font(self, &mut t);
        t.extend([open, cs(self, named!("relax"))]);
        if numbered {
            t.extend([cs(self, named!("noindent")), open, cs(self, named!("hskip"))]);
            self.ieee_text("0pt", &mut t);
            t.push(cs(self, named!("relax")));
            let number = if level == 0 { roman(count) } else { ((b'A' + (count - 1).clamp(0, 25) as u8) as char).to_string() };
            self.ieee_text(&number, &mut t);
            t.push(Token::char(Cat::Other, '.'));
            t.push(cs(self, named!("hskip")));
            self.ieee_text("0.5em", &mut t);
            t.extend([cs(self, named!("relax")), close]);
        } else {
            t.extend([open, cs(self, named!("hskip"))]);
            self.ieee_text("0pt", &mut t);
            t.push(close);
        }
        t.extend([open, cs(self, named!("interlinepenalty"))]);
        self.ieee_text("10000 ", &mut t);
        // hpdftex's `\@ssect`: the anchor first in the title (`section_anchor_tokens`).
        t.extend(self.section_anchor_tokens());
        let title = self.doc.as_mut().map(|d| std::mem::take(&mut d.title)).unwrap_or_default();
        t.extend(title);
        t.extend([cs(self, named!("par")), close, close, self.doc_id(super::SEC_END)]);
        self.back_list(&t);
    }

    /// `\appendix` (no optional heading): `\par`, the counters reset, `\refstepcounter
    /// {section}`, `\@IEEEappendixsavesection*{\appendixname}\addcontentsline{toc}{section}
    /// {\appendixname}`, and `\section` from then on drops its argument.
    pub(super) fn ieee_appendix(&mut self) {
        if self.next_is('[') {
            return self.not_native("IEEEtran: \\appendix with a heading");
        }
        // IEEEtran's `\appendix` sets `\theHsection` (`Appendix.A`) and `\Hy@chapapp` itself;
        // its anchors under hyperref are not ported.
        if self.hyper() {
            return self.not_native("IEEEtran: \\appendix under hyperref");
        }
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.ieee.as_ref().is_some_and(|i| i.appendix) {
            return self.not_native("IEEEtran: a second \\appendix");
        }
        doc.counters.fill(0);
        doc.counters[0] = 1;
        if let Some(i) = doc.ieee.as_deref_mut() {
            i.appendix = true;
        }
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![self.named_cs(named!("par")), self.doc_id(doc!("section")), Token::char(Cat::Other, '*'), open];
        self.ieee_text("Appendix", &mut t);
        t.extend([close, self.doc_id(doc!("rt@ieeetoc"))]);
        self.back_list(&t);
    }

    /// `\addcontentsline` after the appendix heading: the `.aux` `\write` (`aux_write` adds
    /// `\protected@write`'s `\if@nobreak\ifvmode\nobreak\fi\fi`).
    pub(super) fn ieee_toc(&mut self) {
        self.contents_write();
    }

    /// The head of the page being output (`start_foot`), into `fancy::HEAD_BOX`, in
    /// `\@outputpage`'s `\reset@font\normalsize`: one-sided `headings` and
    /// `IEEEtitlepagestyle` both `\hbox{}\@IEEEheaderstyle\leftmark\hfil\thepage` (the
    /// mark empty: `\markboth` is not native), `\@IEEEheaderstyle` `\normalfont\scriptsize`;
    /// no foot. Conference pages have neither: `None`.
    pub(super) fn ieee_page_tokens(&mut self, page: i32) -> Option<Vec<Token>> {
        if self.ieee_conference() {
            return None;
        }
        if let Some(i) = self.ieee_mut() {
            i.page = true;
        }
        let w = self.doc.as_ref().map_or(0, |d| d.class.dimen("textwidth"));
        let mut s = String::new();
        super::super::engine_value::print_scaled(w, &mut s);
        let digits = self.the_page(page);
        let mut t = self.tex_tokens(&format!("\\begingroup\\global\\setbox{}\\hbox to{s}pt{{\\hbox{{}}", super::fancy::HEAD_BOX));
        t.extend([self.doc_id(doc!("normalfont")), self.doc_id(doc!("scriptsize"))]);
        t.extend(self.tex_tokens(&format!("\\hfil {digits}}}\\endgroup ")));
        Some(t)
    }

    /// `\end{abstract}`'s and `\end{IEEEkeywords}`'s `\endgroup`.
    pub(super) fn ieee_abstract_close(&mut self) {
        self.end_group(SEMI_SIMPLE_GROUP);
    }
}


/// IEEEtran.cls's natives, in `Meaning::Doc` order after the kernel's (`engine_doc::DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@ieeesublarge", |m| m.ieee_sublarge()),
    ("rt@ieeetitleend", |m| m.ieee_title_end()),
    ("rt@ieeequant", |m| m.ieee_quant()),
    ("rt@ieeequantskip", |m| m.ieee_quant_skip()),
    ("rt@ieeegobble", |m| m.ieee_gobble()),
    ("rt@ieeeabsend", |m| m.ieee_abstract_close()),
    ("rt@ieeetoc", |m| m.ieee_toc()),
];

#[cfg(test)]
mod tests {
    use super::roman;

    #[test]
    fn roman_numerals() {
        assert_eq!(roman(1), "I");
        assert_eq!(roman(4), "IV");
        assert_eq!(roman(9), "IX");
        assert_eq!(roman(14), "XIV");
    }
}

/// IEEEtran.cls's commands in place of the kernel's (`engine_doc::class_hooks`).
pub(super) const HOOKS: super::ClassHooks = super::ClassHooks {
    // IEEEtran.cls: `\columnsep 1pc`; the `conference` option and the size.
    setup: |m, opts| {
        let o: Vec<&str> = opts.split(',').map(str::trim).collect();
        let size = ["10pt", "11pt", "12pt"].iter().position(|s| o.contains(s)).unwrap_or(0);
        if let Some(doc) = m.doc.as_mut() {
            doc.ieee = Some(Box::new(Ieee::new(o.contains(&"conference"), size)));
        }
        m.e.eqtb.set_word(super::COLUMNSEP, COLUMNSEP, true);
        // `\renewcommand{\sfdefault}{phv}`, `{\rmdefault}{ptm}`, `{\ttdefault}{pcr}`
        // (`font_defaults` reads them at `\begin{document}`).
        let t = m.tex_tokens("\\long\\def\\sfdefault{phv}\\long\\def\\rmdefault{ptm}\\long\\def\\ttdefault{pcr}");
        m.back_list(&t);
    },
    make_title: |m| m.ieee_make_title(),
    appendix: |m| m.ieee_appendix(),
    ..super::KERNEL_HOOKS
};

/// What the native job takes under IEEEtran (`engine_ieeetran`): the preamble's class,
/// packages and title fields, `\maketitle`, `\section`, `\subsection`, `\appendix`, text
/// fonts, `\cite` and the bibliography commands (no `.bbl` is read). The rest of IEEEtran.cls's
/// document commands (lists, floats, footnotes, `\thanks`, displays, `\IEEEPARstart`, page
/// styles, `\label`) stop until ported.
pub(super) const IEEE_NATIVE: &[&str] = &["documentclass", "usepackage", "title", "author", "maketitle", "begin", "end", "section", "subsection", "appendix", "cite", "bibliographystyle", "bibliography", "par", "textbf", "textit", "textsl", "textsc", "textup", "textmd", "textrm", "textsf", "texttt", "emph", "bfseries", "itshape", "em", "noindent", "ldots", "dots"];
/// The environments taken under IEEEtran.
pub(super) const IEEE_ENVS: &[&str] = &["document", "abstract", "IEEEkeywords"];
/// The packages taken under IEEEtran: they change no command the native job takes there.
pub(super) const IEEE_PACKAGES: &[&str] = &["amsmath", "amssymb", "amsfonts", "graphicx", "url", "float", "hyperref"];
