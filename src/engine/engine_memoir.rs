//! memoir.cls on the GDL path (`2026-10-31-memoir-moderncv.md`), its first slice: the class
//! table (`rushtex-gen class --class memoir`), the page styles memoir builds with
//! `\makepagestyle`, the column's `\@texttop` (`\@mkidx`) and `\@makecol`, and the
//! differences of its `\@startsection`/`\M@sect` from the kernel's (in `section_skip`,
//! `section_start`, `section_marks`). Everything else memoir.cls defines stops in the pre-scan
//! (`MEMOIR_NATIVE`).

use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::{Glue, Region};
use super::super::engine_node::{BoxRec, Kind, Node};
use super::super::engine_run::Machine;
use super::super::engine_token::Token;
use super::book::{label_digits, LEFT, RIGHT};
use super::fancy::HEAD_BOX;
use super::{COLUMN_WIDTH, NORMALSIZE};

/// `0.3` as TeX scans it (§452 `round_decimals`): `\footruleskip`'s
/// `0.3\normalbaselineskip`.
const POINT_THREE: i32 = 19661;

/// memoir's `\@setclcnt` (memoir.cls:7544): a division name's counter value.
fn division_value(name: &str) -> Option<u8> {
    ["chapter", "section", "subsection", "subsubsection", "paragraph", "subparagraph"].iter().position(|n| *n == name).map(|i| i as u8)
}

/// The main matter's `\c@secnumdepth` a memoir preamble leaves: the last `\setsecnumdepth{..}`
/// (memoir.cls:7747, before `\begin{document}` also `maxsecnumdepth`), else 1 (`section`).
pub(super) fn secnum_in(preamble: &str) -> u8 {
    preamble.rmatch_indices("\\setsecnumdepth{").next().and_then(|(i, n)| preamble[i + n.len()..].split_once('}')).and_then(|(a, _)| division_value(a.trim())).unwrap_or(1)
}

impl Machine<'_, '_> {
    pub(super) fn memoir(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.memoir)
    }

    /// `\mem@ps@HLH`'s box (memoir.cls) for one head or foot, its three slots as tokens:
    /// `\hb@xt@ W{\m@mhe@dreset \vbox{[foot rule] \hbox{\rlap{\parbox[b]{W}{\raggedright
    /// \strut{L}\strut}}\hfill \parbox[b]{W}{\centering\strut{C}\strut}\hfill \llap{\parbox
    /// [b]{W}{\raggedleft\strut{R}\strut}}} [head rule]}}` between `\makeheadposition`'s
    /// `\hss` and `\hss` (`\mem@ps@entry@wrap` is empty, so each slot is a group;
    /// `\mem@hvboxm`'s `\setbox0\vbox{..}\box0` is the `\vbox` itself; `\baselinestretch` is
    /// 1 already). The head rule is `\hrule\@width W\@height0pt \vskip-0pt`, the foot rule
    /// `\vskip-\footruleskip\vskip-0pt \hrule\@width W\@height0pt \vskip\footruleskip`.
    fn memoir_slots(&mut self, head: bool, slots: [Vec<Token>; 3], strut: bool) -> Vec<Token> {
        let w = self.doc.as_ref().map_or(0, |d| d.class.dimen("textwidth"));
        // `\makepagestyle{empty}`: `\emptypsheadstrut` and `\emptypsfootstrut` are empty.
        let st = if strut { "\\strut" } else { "" };
        let mut t = self.tex_tokens(&format!("\\hss\\hbox to{w}sp{{\\normalsize\\vbox{{"));
        if !head {
            // `\footruleskip` is `0.3\normalbaselineskip`, the class's `\baselineskip` at
            // `\normalsize`.
            let bs = super::glue_words(self.size_row(NORMALSIZE).baselineskip).width;
            let skip = crate::backend::scaled::xn_over_d(bs, POINT_THREE, 0x10000);
            t.extend(self.tex_tokens(&format!("\\begingroup\\vskip-{skip}sp\\vskip-0sp\\hrule width{w}sp height0sp\\vskip{skip}sp\\endgroup")));
        }
        let [l, c, r] = slots;
        t.extend(self.tex_tokens(&format!("\\hbox{{\\rlap{{\\parbox[b]{{{w}sp}}{{\\raggedright{st}{{")));
        t.extend(l);
        t.extend(self.tex_tokens(&format!("}}{st}}}}}\\hfill\\parbox[b]{{{w}sp}}{{\\centering{st}{{")));
        t.extend(c);
        t.extend(self.tex_tokens(&format!("}}{st}}}\\hfill\\llap{{\\parbox[b]{{{w}sp}}{{\\raggedleft{st}{{")));
        t.extend(r);
        t.extend(self.tex_tokens(&format!("}}{st}}}}}}}")));
        if head {
            t.extend(self.tex_tokens(&format!("\\hrule width{w}sp height0sp\\vskip-0sp")));
        }
        t.extend(self.tex_tokens("}}\\hss "));
        t
    }

    /// The head and foot of page `page` under memoir's `headings` (the class's last
    /// `\pagestyle`), as tokens (`start_foot`): the head into [`HEAD_BOX`], the foot into the
    /// output nest, in `\@outputpage`'s `\reset@font\normalsize`. Two-sided: odd head
    /// `{\slshape\rightmark}{}{\thepage}`, even head `{\thepage}{}{\slshape\leftmark}`;
    /// one-sided the odd head on every page. The feet are empty slots. `special` is
    /// `\thispagestyle`'s: `plain` (`Some(false)`, also `chapter` and `title`, aliases of
    /// it) has empty heads and `{}{\thepage}{}` feet, `empty` (`Some(true)`, also `cleared`)
    /// empty slots without struts.
    pub(super) fn memoir_page_tokens(&mut self, page: i32, special: Option<bool>) -> Vec<Token> {
        let Some(doc) = self.doc.as_mut() else { return Vec::new() };
        doc.head_page = true;
        let even = doc.class.flag("twoside") && page % 2 == 0;
        let digits = label_digits(&self.the_page(page));
        let (head, foot) = match special {
            Some(true) => ([Vec::new(), Vec::new(), Vec::new()], [Vec::new(), Vec::new(), Vec::new()]),
            Some(false) => ([Vec::new(), Vec::new(), Vec::new()], [Vec::new(), digits, Vec::new()]),
            None => {
                let mark = self.page_mark_text(if even { LEFT as u32 } else { RIGHT as u32 }, !even).unwrap_or_default();
                let mut sl = self.tex_tokens("\\slshape ");
                sl.extend(mark);
                (if even { [digits, Vec::new(), sl] } else { [sl, Vec::new(), digits] }, [Vec::new(), Vec::new(), Vec::new()])
            }
        };
        let strut = special != Some(true);
        let mut t = self.tex_tokens(&format!("\\begingroup\\normalfont\\normalsize\\global\\setbox{HEAD_BOX}\\hbox{{"));
        t.extend(self.memoir_slots(true, head, strut));
        t.extend(self.tex_tokens("}"));
        t.extend(self.memoir_slots(false, foot, strut));
        t.extend(self.tex_tokens("\\endgroup "));
        t
    }

    /// memoir's `\chapter` (memoir.cls, not `article`): `\clearforchapter` (`\cleartorecto`
    /// under `openright`, `\clearpage` under `openany`), `\thispagestyle{chapter}` (`plain`),
    /// `\global\@topnum\z@`, `\m@mindentafterchapter` (`\@afterindentfalse`), then
    /// `\@ifstar{\@m@mschapter}{\@m@mchapter}` (`memoir_chapter_head`).
    pub(super) fn memoir_chapter(&mut self) {
        let recto = self.doc.as_ref().is_some_and(|d| d.openright);
        let mut t = vec![self.doc_id(if recto { doc!("cleardoublepage") } else { doc!("clearpage") }), self.doc_id(doc!("thispagestyle"))];
        t.extend(self.tex_tokens("{plain}"));
        t.push(self.doc_id(doc!("rt@chapter")));
        self.back_list(&t);
    }

    /// memoir's `\cleardoublepage` and `\cleartorecto` after their `\clearpage`: `\if@twoside
    /// \ifodd\c@page\else \hbox{}\thispagestyle{cleared}\newpage\fi\fi` (`cleared` is
    /// `empty`).
    pub(super) fn memoir_clear_double_tail(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.class.flag("twoside") && doc.page % 2 == 0 {
            let mut t = self.tex_tokens("\\hbox{}");
            t.push(self.doc_id(doc!("thispagestyle")));
            t.extend(self.tex_tokens("{empty}"));
            t.push(self.doc_id(super::NEW_PAGE));
            self.back_list(&t);
        }
    }

    /// memoir's `\frontmatter` (`\@smemfront\pagenumbering{roman}`: `\cleardoublepage
    /// \@mainmatterfalse \setcounter{secnumdepth}{-10}`), `\mainmatter` (`\@mainmattertrue
    /// \setcounter{secnumdepth}{\value{maxsecnumdepth}}`, `\cleardoublepage`,
    /// `\pagenumbering{arabic}`) and `\backmatter` (`\cleardoublepage` under `openright`, else
    /// `\clearpage`; `\@mainmatterfalse`, `secnumdepth` -10). `secnumdepth` follows
    /// `\if@mainmatter` here (`doc.mainmatter`): -10 outside, `maxsecnumdepth` (1) inside.
    /// The starred forms and the float counters (`\counterwithout{figure}{chapter}`) stop.
    pub(super) fn memoir_matter(&mut self, which: u8) {
        if self.next_is('*') {
            return self.not_native("memoir: \\frontmatter* or \\mainmatter*");
        }
        let Some(doc) = self.doc.as_mut() else { return };
        doc.mainmatter = which == 1;
        let clear = if which < 2 || doc.openright { doc!("cleardoublepage") } else { doc!("clearpage") };
        let mut t = Vec::new();
        // `\@smemmain`'s line `\setcounter{secnumdepth}{\value{maxsecnumdepth}}` ends in a
        // space token, before the clearing: a second space in a paragraph \mainmatter ends
        // (its `\par` removes only the last).
        if which == 1 {
            t.push(Token::char(Cat::Space, ' '));
        }
        t.push(self.doc_id(clear));
        if which < 2 {
            t.push(self.doc_id(doc!("pagenumbering")));
            t.extend(self.tex_tokens(if which == 0 { "{roman}" } else { "{arabic}" }));
        }
        self.back_list(&t);
    }

    /// memoir's `\@m@mchapter` and `\@m@mschapter` after `\chapter`'s clearing. Unstarred,
    /// `\@chapter`: `\refstepcounter{chapter}` in the main matter (`\ifm@m@And`), then
    /// `\chaptermark{\f@rhdr}` (`headings`' `\createmark{chapter}{right}` one-sided, `{left}`
    /// two-sided, as book's), `\insertchapterspace` (two `\addtocontents`: lof, lot),
    /// `\@makechapterhead`, `\@afterheading`, and only then `\addcontentsline{toc}{chapter}`.
    /// Starred, `\@schapter`: `\@makeschapterhead` and `\@afterheading`. The head, with the
    /// default chapter style (`\@chs@def@ult`): `\vspace*{\beforechapskip}` (50pt), then in a
    /// group `\parskip\z@ \parindent\z@ \memRTLraggedright \normalfont`, in the main matter
    /// `\chapnamefont \@chapapp \space \chapnumfont \thechapter` (`\normalfont\huge
    /// \bfseries`) `\par\nobreak\vskip \midchapskip` (20pt), then `\interlinepenalty\@M
    /// \normalfont\Huge\bfseries title` and a space, `\par\nobreak\vskip \afterchapskip`
    /// (40pt). The optional titles (`\chapter[toc][head]{..}`) stop.
    pub(super) fn memoir_chapter_head(&mut self) {
        let star = self.next_is('*');
        if self.next_is('[') {
            return self.not_native("memoir: a chapter's optional title");
        }
        let mut title = Vec::new();
        self.read_arg(&mut title);
        let Some(doc) = self.doc.as_mut() else { return };
        let numbered = !star && doc.mainmatter;
        doc.afterindent = false;
        doc.level = 0;
        doc.numbered = numbered;
        doc.toc = !star;
        let mut number = String::new();
        if numbered {
            number = self.chapter_step();
            self.numbered_chapter_anchor();
        } else {
            // hpdftex.def's `\@schapter`, or its `\@chapter` outside the main matter, then
            // memoir's `\MakeLinkTarget[chapter]{}`.
            self.unnumbered_chapter_anchor();
            self.make_link_target();
        }
        if !star {
            self.chapter_mark(&number, &title);
            self.aux_write(0);
            self.aux_write(0);
        }
        let mut t = vec![self.doc_id(doc!("vspace"))];
        t.extend(self.tex_tokens("*{50pt}{\\parskip0pt\\parindent0pt"));
        t.extend([self.doc_id(doc!("raggedright")), self.doc_id(doc!("normalfont"))]);
        if numbered && self.doc.as_ref().is_some_and(|d| d.memoir_style == 1) {
            // `section`: `\chapnumfont \thechapter\space`, the title in the same paragraph.
            t.extend([self.doc_id(doc!("normalfont")), self.doc_id(doc!("Huge")), self.doc_id(doc!("bfseries"))]);
            t.extend(self.tex_tokens(&number));
            t.push(Token::char(Cat::Space, ' '));
        } else if numbered {
            let chapapp = if self.doc.as_ref().is_some_and(|d| d.appendix) { "Appendix" } else { "Chapter" };
            t.extend([self.doc_id(doc!("normalfont")), self.doc_id(doc!("huge")), self.doc_id(doc!("bfseries"))]);
            t.extend(self.tex_tokens(&format!("{chapapp} ")));
            t.extend([self.doc_id(doc!("normalfont")), self.doc_id(doc!("huge")), self.doc_id(doc!("bfseries"))]);
            t.extend(self.tex_tokens(&number));
            t.extend([self.named_cs(named!("par")), self.doc_id(doc!("nobreak"))]);
            t.extend(self.tex_tokens("\\vskip20pt"));
        }
        t.extend(self.tex_tokens("\\interlinepenalty10000 "));
        t.extend([self.doc_id(doc!("normalfont")), self.doc_id(doc!("Huge")), self.doc_id(doc!("bfseries"))]);
        t.append(&mut title);
        t.push(Token::char(Cat::Space, ' '));
        t.extend([self.named_cs(named!("par")), self.doc_id(doc!("nobreak"))]);
        t.extend(self.tex_tokens("\\vskip40pt}"));
        t.push(self.doc_id(doc!("rt@afterheading")));
        if !star {
            t.push(self.doc_id(doc!("rt@memtoc")));
        }
        self.back_list(&t);
    }

    /// memoir's `\maketitle` (memoir.cls:2514), one column: `\par\begingroup` (`\@bsmarkseries`
    /// changes only `\thanks`' marks), `\ifdim\pagetotal>\z@\newpage\fi \global\@topnum\z@`,
    /// `\@maketitle`: `\ifdim\pagetotal>\z@\newpage\fi \null \vskip 2em \vspace*{\droptitle}`
    /// (0pt), `{\@bspretitle \@title \@bsposttitle}` (`\begin{center}\LARGE` .. `\par
    /// \end{center}\vskip 0.5em`), `{\@bspreauthor \@author \@bspostauthor}` (`\begin{center}
    /// \large \lineskip .5em \begin{tabular}[t]{c}` .. `\end{tabular}\par\end{center}`),
    /// `{\@bspredate \@date \@bspostdate}` (`\begin{center}\large` .. `\par\end{center}`), the
    /// four `\maketitlehook`s empty, `\par \vskip 1.5em`; then `\thispagestyle{title}`
    /// (`plain`), `\@thanks`, `\endgroup`, `\setcounter{footnote}{0}`.
    pub(super) fn memoir_make_title(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.floats.twocol {
            return self.not_native("memoir: \\maketitle in two columns");
        }
        let [title, author, date] = std::mem::take(&mut doc.stored);
        doc.thanks = Some(Vec::new());
        let (dated, authored) = (doc.dated, doc.authored);
        let date = if dated { date } else { self.tex_tokens("\\today") };
        let author = if authored { author } else { self.tex_tokens("\\relax") };
        let mut t = vec![self.named_cs(named!("par")), self.named_cs(named!("begingroup"))];
        t.extend(self.tex_tokens("\\ifdim\\pagetotal>0pt"));
        t.push(self.doc_id(super::NEW_PAGE));
        t.extend(self.tex_tokens("\\fi"));
        t.push(self.doc_id(doc!("rt@notopfloats")));
        t.extend(self.tex_tokens("\\ifdim\\pagetotal>0pt"));
        t.push(self.doc_id(super::NEW_PAGE));
        t.extend(self.tex_tokens("\\fi\\hbox{}\\vskip2em"));
        t.push(self.doc_id(doc!("vspace")));
        t.extend(self.tex_tokens("*{0pt}{\\begin{center}\\LARGE "));
        t.extend(title);
        t.extend(self.tex_tokens("\\par\\end{center}\\vskip0.5em}{\\begin{center}\\large\\lineskip.5em\\begin{tabular}[t]{c}"));
        t.extend(author);
        t.extend(self.tex_tokens("\\end{tabular}\\par\\end{center}}{\\begin{center}\\large "));
        t.extend(date);
        t.extend(self.tex_tokens("\\par\\end{center}}\\par\\vskip1.5em"));
        t.push(self.doc_id(doc!("thispagestyle")));
        t.extend(self.tex_tokens("{plain}"));
        t.extend([self.doc_id(super::TITLE_END), self.named_cs(named!("endgroup"))]);
        self.back_list(&t);
    }

    /// memoir's `\tableofcontents` (`\newlistof{tableofcontents}{toc}{\contentsname}`,
    /// memoir.cls:7074): `\ensureonecol \par \begingroup \MakeLinkTarget[chapter]{}`, unstarred
    /// `\addcontentsline{toc}{chapter}{\contentsname}`, then `\@tocmaketitle`
    /// (`memoir_toc_begin`), `\parskip\cftparskip` (0pt), `\@starttoc{toc}` (its group, the
    /// entries to `tocdepth` 1, `\@nobreakfalse`), `\endgroup`. The entries come from the
    /// source as for the kernel's (`table_of_contents`).
    /// `\chapterstyle{name}` (memoir.cls:3177, `\@nameuse{chs@name}`, each style starting
    /// `\@chs@def@ult`): `default` and `section` (memoir.cls:3196: no `\printchaptername`,
    /// `\chapternamenum` or `\afterchapternum`; `\printchapternum` `\chapnumfont \thechapter
    /// \space` with `\chapnumfont` `\normalfont\Huge\bfseries`). Others stop.
    pub(super) fn memoir_chapter_style(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let name: String = arg.iter().map(|t| t.chr()).collect();
        let style = match name.trim() {
            "default" => 0,
            "section" => 1,
            _ => return self.not_native("memoir: a \\chapterstyle other than default or section"),
        };
        if let Some(doc) = self.doc.as_mut() {
            doc.memoir_style = style;
        }
    }

    /// `\setsecnumdepth{division}` (in the preamble `secnumdepth` and `maxsecnumdepth`, the
    /// main matter's) and `\settocdepth{division}` (`\settocdepth@{\addtocontents}`: a
    /// `\changetocdepth{n}` line to the `.toc`, a `\write` node where it stands, which sets
    /// `tocdepth` when the contents are read). In the document body, or starred, they stop.
    pub(super) fn memoir_depth(&mut self, toc: bool) {
        if toc && self.next_is('*') {
            return self.not_native("memoir: \\settocdepth*");
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let name: String = arg.iter().map(|t| t.chr()).collect();
        let Some(v) = division_value(name.trim()) else { return self.not_native("memoir: a division depth other than chapter to subparagraph") };
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.body {
            return self.not_native("memoir: \\setsecnumdepth or \\settocdepth in the document");
        }
        if toc {
            doc.memoir_tocdepth = v;
            self.aux_write(0);
        } else {
            doc.memoir_secnum = v;
        }
    }

    pub(super) fn memoir_table_of_contents(&mut self) {
        if self.next_is('*') {
            return self.not_native("memoir: \\tableofcontents*");
        }
        let Some(doc) = self.doc.as_mut() else { return };
        let entries = match doc.contents.take() {
            Some(t) if !doc.defer && t.iter().any(|e| e.level <= doc.memoir_tocdepth as usize) => t,
            _ => return self.not_native("a \\tableofcontents the pre-scan cannot read whole"),
        };
        doc.defer = true;
        let depth = doc.memoir_tocdepth as usize;
        doc.toc_shown = entries.iter().enumerate().filter(|(_, e)| e.level <= depth).map(|(i, _)| i as u32).collect();
        doc.contents = Some(entries);
        doc.toc_next = 0;
        let t = [self.named_cs(named!("par")), self.doc_id(doc!("rt@memtocbegin"))];
        self.back_list(&t);
    }

    /// After `\tableofcontents`' `\par`: `\begingroup`, `\MakeLinkTarget`'s `\special{}`, the
    /// contents line's write, then `\@tocmaketitle`: `\chapterheadstart` (`\vspace*{50pt}`),
    /// `{\parindent\z@ \parskip\z@ \interlinepenalty\@M \printchapternonum \printchaptertitle
    /// {\contentsname} \tocmark \thispagestyle{chapter} \afterchaptertitle}` and
    /// `\@afterheading`; `\parskip\cftparskip`, the entries, `\endgroup`.
    pub(super) fn memoir_toc_begin(&mut self) {
        self.begin_group(super::SEMI_SIMPLE_GROUP);
        self.make_link_target();
        let Some(doc) = self.doc.as_mut() else { return };
        let saved = std::mem::replace(&mut doc.level, 0);
        self.bookmark_write(0);
        self.contents_write();
        if let Some(doc) = self.doc.as_mut() {
            doc.level = saved;
        }
        let mut t = vec![self.doc_id(doc!("vspace"))];
        t.extend(self.tex_tokens("*{50pt}{\\parindent0pt\\parskip0pt\\interlinepenalty10000 "));
        t.extend([self.doc_id(doc!("normalfont")), self.doc_id(doc!("Huge")), self.doc_id(doc!("bfseries"))]);
        t.extend(self.tex_tokens("Contents"));
        t.push(self.doc_id(doc!("rt@memtocmark")));
        t.push(self.doc_id(doc!("thispagestyle")));
        t.extend(self.tex_tokens("{plain}"));
        t.extend([self.named_cs(named!("par")), self.doc_id(doc!("nobreak"))]);
        t.extend(self.tex_tokens("\\vskip40pt}"));
        t.push(self.doc_id(doc!("rt@afterheading")));
        t.extend(self.tex_tokens("\\parskip0pt"));
        t.extend([self.doc_id(doc!("rt@tocopen")), self.named_cs(named!("endgroup"))]);
        self.back_list(&t);
    }

    /// `\tocmark` under `headings`: `\createplainmark{toc}{right}{\contentsname}` one-sided
    /// (`\markright{\memUChead{..}}`), `{both}` two-sided (`\markboth`).
    pub(super) fn memoir_toc_mark(&mut self) {
        let text = self.tex_tokens("Contents");
        let mark = self.uppercase_mark(&text);
        if self.doc.as_ref().is_some_and(|d| d.class.flag("twoside")) {
            self.mark_both(&mark, &mark);
        } else {
            self.mark_right(&mark);
        }
    }

    /// A contents line, `\l@chapapp` (chapter) or `\newlistentry`'s `\l@section`, up to its
    /// paragraph: chapter `\addpenalty{-\@highpenalty} \vskip\cftbeforechapterskip` (1em plus
    /// 1pt), section `\vskip\cftbeforesectionskip` (0pt plus .2pt); then in a group
    /// `\leftskip` the indent (0, `1.5em`), `\rightskip\@tocrmarg` (2.55em, the current
    /// font's), `\parfillskip-\rightskip`, `\parindent` the indent, `\@afterindenttrue`,
    /// `\interlinepenalty\@M`; `memoir_toc_head` after. The lengths `\cft..indent`,
    /// `\cft..numwidth` and `\cftbefore..skip` are registers memoir.cls set at load, in the
    /// class's font (`sec_glue`).
    pub(super) fn memoir_toc_open(&mut self) {
        let Some((level, _)) = self.toc_current() else { return };
        if level == 0 {
            let mut ops = Vec::new();
            self.add_penalty(-super::HIGH_PENALTY, &mut ops);
            ops.push(super::Op::Call(|m| {
                let g = m.sec_glue("1em plus 1pt");
                m.build_glue(g, false);
                m.memoir_toc_group();
            }));
            return self.run_ops(ops);
        }
        self.build_glue(Glue { stretch: 13107, ..Glue::default() }, false);
        self.memoir_toc_group();
    }

    fn memoir_toc_group(&mut self) {
        use super::super::engine_linebreak::{INTER_LINE_PENALTY, LEFT_SKIP, PAR_FILL_SKIP, RIGHT_SKIP};
        let Some((level, _)) = self.toc_current() else { return };
        self.begin_group(super::SIMPLE_GROUP);
        let indent = if level == 0 { 0 } else { self.sec_glue("1.5em").width };
        let right = self.glue_of("2.55em").width;
        self.set_skip(LEFT_SKIP, Glue { width: indent, ..Glue::default() });
        self.set_skip(RIGHT_SKIP, Glue { width: right, ..Glue::default() });
        self.set_skip(PAR_FILL_SKIP, Glue { width: -right, ..Glue::default() });
        self.set_local(Region::Dimen, super::super::engine_pack::PAR_INDENT, indent);
        if let Some(doc) = self.doc.as_mut() {
            doc.toc_afterindent.push(doc.afterindent);
            doc.afterindent = true;
        }
        self.set_local(Region::Int, INTER_LINE_PENALTY, 10000);
        let t = self.doc_id(doc!("rt@memtochead"));
        self.input.back(t);
    }

    /// `\leavevmode`, `\@tempdima` the width of `\cft..font\cft..name` (nothing) plus
    /// `\cft..numwidth` (1.5em, 2.3em), `\advance\leftskip\@tempdima \null\nobreak
    /// \hskip-\leftskip`, then `{\cft..font{` (`\bfseries`, `\normalfont`) and
    /// `\chapternumberline` or `\numberline`: `\hb@xt@\@tempdima{N\hfil}`.
    pub(super) fn memoir_toc_head(&mut self) {
        use super::super::engine_linebreak::LEFT_SKIP;
        if self.leave_vmode() {
            return;
        }
        let Some((level, number)) = self.toc_current() else { return };
        let w = self.sec_glue(if level == 0 { "1.5em" } else { "2.3em" }).width;
        let left = self.skip(LEFT_SKIP);
        let left = Glue { width: left.width + w, ..left };
        self.set_skip(LEFT_SKIP, left);
        self.build_hbox(|_| {});
        self.build_penalty(10000);
        self.build_glue(super::neg(left), false);
        self.begin_group(super::SIMPLE_GROUP);
        self.memoir_toc_font(level);
        // hyperref's `\contentsline`: `\l@chapter{\hyper@linkstart{link}{href}#2\hyper@linkend}`,
        // inside `{\cftchapterfont{#1}}`.
        self.contents_link_start(level, &number);
        // `\M@sect` past `secnumdepth` (the front and back matter, levels past `section`)
        // writes no `\numberline`.
        if number.is_empty() || super::unnumbered(&number) {
            return;
        }
        self.build_hbox_to(w, |m| {
            m.build_text(number.as_bytes());
            m.build_glue(Glue { stretch: 1 << 16, stretch_order: 1, ..Glue::default() }, false);
        });
        // microtype's `toc` patch under memoir: `\chapternumberline` and `\numberline` end
        // with `\leftprotrusion`.
        if self.microtype() {
            self.left_protrusion();
        }
    }

    /// `\cftchapterfont` (`\bfseries`) or `\cftsectionfont` (`\normalfont`).
    fn memoir_toc_font(&mut self, level: usize) {
        if level == 0 {
            self.font_change(super::Change::Series("bx"));
        } else {
            self.normal_font();
        }
    }

    /// After the title: `}\nobreak`, `{\cft..leader}` (`\bfseries` or `\normalfont`, then
    /// `\cftdotfill{\cft..dotsep}`: `\leaders\hbox{$\m@th\mkern d mu\hbox{.}\mkern d mu$}
    /// \hfill`, `d` 2000 (`\cftnodots`) for chapters, 4.5 for sections), `\nobreak`,
    /// `\cft..formatpnum`: `\hb@xt@\@pnumwidth{\hfil\cft..pagefont N}` (`\@pnumwidth` 1.55em).
    pub(super) fn memoir_toc_tail(&mut self) {
        let Some((level, _)) = self.toc_current() else { return };
        self.contents_link_end();
        self.end_group(super::SIMPLE_GROUP);
        self.build_penalty(10000);
        self.begin_group(super::SIMPLE_GROUP);
        self.memoir_toc_font(level);
        let mu = if level == 0 { 2000 << 16 } else { 9 << 15 };
        self.build_leaders(Glue { stretch: 1 << 16, stretch_order: 2, ..Glue::default() }, |m| {
            m.push_text_math();
            m.check_math_fonts();
            m.math_surround_zero();
            m.build_mkern(mu);
            m.build_hbox(|m| m.build_text(b"."));
            m.build_mkern(mu);
            m.after_math();
        });
        self.end_group(super::SIMPLE_GROUP);
        self.build_penalty(10000);
        let w = self.glue_of("1.55em").width;
        self.build_hbox_to(w, |m| {
            m.build_glue(Glue { stretch: 1 << 16, stretch_order: 1, ..Glue::default() }, false);
            m.begin_group(super::SIMPLE_GROUP);
            m.memoir_toc_font(level);
            m.build_text(b"0");
            m.end_group(super::SIMPLE_GROUP);
        });
        self.toc_page_box();
        // microtype's `toc` patch under memoir: `\cft..afterpnum` ends with `\noprotrusion`
        // (`\leavevmode\kern-\p@\kern\p@`, microtype.sty), no protrusion at the right.
        if self.microtype() {
            self.cur.extend([Node { sub: 1, ..Node::new(Kind::Kern, -65536) }, Node { sub: 1, ..Node::new(Kind::Kern, 65536) }]);
        }
    }

    /// memoir's `\epigraph{text}{source}` (memoir.cls:6804; the epigraph package is emulated):
    /// `\vspace{\beforeepigraphskip} {\epigraphsize\begin{\epigraphflush}\begin{minipage}
    /// {\epigraphwidth} \@epitext{text}\\ \@episource{source} \end{minipage}\end{\epigraphflush}
    /// \vspace{\afterepigraphskip}}`, with `\epigraphsize` `\small`, `\epigraphflush` and
    /// `\sourceflush` `flushright`, `\textflush` `flushleft`; `\@epitext` is `\begin{minipage}
    /// {\epigraphwidth}\begin{flushleft} text\par \@epirule \end{flushleft}\end{minipage}`
    /// (`\@epirule` `\rule[.5ex]{\epigraphwidth}{\epigraphrule}`, the rule .4pt), `\@episource`
    /// `\begin{minipage}{\epigraphwidth}\begin{flushright} source\par \end{flushright}
    /// \end{minipage}`. The lengths are memoir.cls's at load: the skips `.5\baselineskip` of the
    /// class's `\normalsize`, the width `.4\textwidth`. The spaces the source lines end with
    /// are kept where they are tokens.
    pub(super) fn memoir_epigraph(&mut self) {
        let (mut text, mut source) = (Vec::new(), Vec::new());
        self.read_arg(&mut text);
        self.read_arg(&mut source);
        let tw = self.doc.as_ref().map_or(0, |d| d.class.dimen("textwidth"));
        let w = crate::backend::scaled::xn_over_d(tw, 26214, 0x10000);
        let bs = super::glue_words(self.size_row(NORMALSIZE).baselineskip).width;
        let skip = crate::backend::scaled::xn_over_d(bs, 32768, 0x10000);
        let mut t = vec![self.doc_id(doc!("vspace"))];
        t.extend(self.tex_tokens(&format!("{{{skip}sp}} {{")));
        t.push(self.doc_id(doc!("small")));
        t.extend(self.tex_tokens(&format!("\\begin{{flushright}}\\begin{{minipage}}{{{w}sp}} \\begin{{minipage}}{{{w}sp}}\\begin{{flushleft}} ")));
        t.extend(text);
        t.extend(self.tex_tokens(&format!("\\par \\rule[.5ex]{{{w}sp}}{{.4pt}} \\end{{flushleft}}\\end{{minipage}}")));
        t.push(self.doc_id(doc!("\\")));
        t.extend(self.tex_tokens(&format!(" \\begin{{minipage}}{{{w}sp}} \\begin{{flushright}} ")));
        t.extend(source);
        t.extend(self.tex_tokens("\\par \\end{flushright}\\end{minipage} \\end{minipage}\\end{flushright} "));
        t.push(self.doc_id(doc!("vspace")));
        t.extend(self.tex_tokens(&format!("{{{skip}sp}}}}")));
        self.back_list(&t);
    }

    /// memoir's `\MakeLinkTarget[chapter]{}` (`link_target`).
    pub(super) fn make_link_target(&mut self) {
        self.link_target("chapter");
    }

    /// memoir's `\mem@makecol` with no footnotes, floats, side bar or side feet: `\setbox
    /// \@outputbox\box\@cclv`, then `\m@m@makecoltext`: `\vbox to\@colht{\@texttop \dimen@
    /// \dp\@outputbox \unvbox\@outputbox \vskip-\dimen@ \@textbottom}`, where `\@texttop` is
    /// `\@mkidx`: `\vbox to\z@{\rlap{\@rightidx \box\@indexbox}\vss}` (`\@leftidx`,
    /// `\hskip-\marginparsep \hskip-\marginparwidth`, on an even two-sided page), and
    /// `\@textbottom` `\relax` under the class's `\flushbottom`, `\vskip\z@\@plus.0001fil`
    /// under `\raggedbottom`.
    pub(super) fn memoir_column(&mut self) {
        let Some(doc) = self.doc.as_ref() else { return };
        let class = doc.class;
        let (page, colht) = (doc.page, doc.floats.colht);
        if self.e.eqtb.box_reg(super::FOOTINS).is_some() || doc.floats.has_bottom() || doc.floats.has_top() {
            return self.not_native("memoir: a column with footnotes or floats");
        }
        let Some(b) = self.e.eqtb.box_reg(255) else { return };
        let start = self.cur.len();
        let fil = Glue { stretch: 65536, stretch_order: 1, shrink: 65536, shrink_order: 1, ..Glue::default() };
        if class.flag("twoside") && page % 2 == 0 {
            for w in [self.rw(super::R_MARGINPARSEP), self.rw(super::R_MARGINPARWIDTH)] {
                let n = self.e.nodes.glue_node(Glue { width: -w, ..Glue::default() }, 0);
                self.cur.push(n);
            }
        } else {
            for w in [self.e.eqtb.word(COLUMN_WIDTH), self.rw(super::R_MARGINPARSEP)] {
                let n = self.e.nodes.glue_node(Glue { width: w, ..Glue::default() }, 0);
                self.cur.push(n);
            }
        }
        let n = self.e.nodes.glue_node(fil, 0);
        self.cur.push(n);
        let rlap = self.hpack(start, 0, true);
        let rlap = self.e.nodes.add_box(rlap);
        self.cur.push(Node::new(Kind::Box, rlap as i32));
        let n = self.e.nodes.glue_node(fil, 0);
        self.cur.push(n);
        let d = self.dimen_param(super::super::engine_pack::BOX_MAX_DEPTH);
        let idx = self.vpack(start, 0, true, d);
        let idx = self.e.nodes.add_box(BoxRec { vertical: true, ..idx });
        self.cur.push(Node::new(Kind::Box, idx as i32));
        let r = self.e.nodes.boxes[b as usize];
        self.cur.extend_from_slice(self.e.nodes.get(r.list));
        self.e.eqtb.replace_box(255, None);
        self.e.nodes.free_block(r.list);
        self.e.nodes.free_box(b);
        let n = self.e.nodes.glue_node(Glue { width: -r.depth, ..Glue::default() }, 0);
        self.cur.push(n);
        if class.flag("raggedbottom") {
            // `.0001` is 7sp (§452).
            let n = self.e.nodes.glue_node(Glue { stretch: 7, stretch_order: 1, ..Glue::default() }, 0);
            self.cur.push(n);
        }
        let column = self.vpack(start, colht, true, d);
        let column = self.e.nodes.add_box(BoxRec { vertical: true, ..column });
        if let Some(doc) = self.doc.as_mut() {
            doc.column = Some(column);
        }
        self.opcol();
    }
}

/// memoir.cls's natives, in `Meaning::Doc` order after the kernel's (`engine_doc::DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@memtocbegin", |m| m.memoir_toc_begin()),
    ("chapterstyle", |m| if m.memoir() { m.memoir_chapter_style() } else { m.not_native("\\chapterstyle outside memoir") }),
    ("setsecnumdepth", |m| if m.memoir() { m.memoir_depth(false) } else { m.not_native("\\setsecnumdepth outside memoir") }),
    ("settocdepth", |m| if m.memoir() { m.memoir_depth(true) } else { m.not_native("\\settocdepth outside memoir") }),
    ("rt@memtocmark", |m| m.memoir_toc_mark()),
    ("rt@memtocopen", |m| m.memoir_toc_open()),
    ("rt@memtochead", |m| m.memoir_toc_head()),
    ("rt@memtoctail", |m| m.memoir_toc_tail()),
    // memoir's `\@chapter`: `\addcontentsline{toc}{chapter}` after the head.
    ("rt@memtoc", |m| {
        m.bookmark_write(0);
        m.contents_write()
    }),
];

/// memoir.cls's commands in place of the kernel's (`engine_doc::class_hooks`).
pub(super) const HOOKS: super::ClassHooks = super::ClassHooks {
    // memoir.cls:5453, 5473: `\RequirePackage{array}` and `{booktabs}` at its load (its
    // lengths in the class's font: `\heavyrulewidth=.08em` is read here).
    setup: |m, _| {
        for p in ["array", "booktabs"] {
            m.load_package(p);
        }
    },
    make_title: |m| m.memoir_make_title(),
    table_of_contents: |m| m.memoir_table_of_contents(),
    epigraph: |m| m.memoir_epigraph(),
    clear_double_tail: |m| m.memoir_clear_double_tail(),
    matter: |m, k| m.memoir_matter(k),
    ..super::KERNEL_HOOKS
};

/// What the native job takes under memoir: the preamble's class, packages and title fields,
/// `\chapter` with the default chapter style and the three matters (`engine_memoir`),
/// `\section` to `\subsubsection` (memoir's `\@startsection` and `\M@sect`), text fonts and
/// sizes (the class table's `\@setfontsize` rows), `\today` (the kernel's, memoir.cls:12118).
/// The rest of memoir.cls's document commands (lists, `\footnote`, floats, `verbatim`, page
/// styles, the layout commands) stop until ported
/// (`2026-10-31-memoir-moderncv.md`).
pub(super) const MEMOIR_NATIVE: &[&str] = &["chapterstyle", "setsecnumdepth", "settocdepth", "setlength", "newcommand", "vspace", "vfill", "ldots", "frac", "sum", "mathrm", "max", "appendix", "addlinespace", "[", "]", "_", "^", "includegraphics", "centering", "toprule", "midrule", "bottomrule", "caption", "label", "ref", "pageref", "epigraph", "citet", "citep", "cite", "bibliographystyle", "bibliography", "TeX", "LaTeX", "maketitle", "tableofcontents", "addcontentsline", "chapter", "frontmatter", "mainmatter", "backmatter", "today", "documentclass", "usepackage", "title", "author", "date", "begin", "end", "section", "subsection", "subsubsection", "par", "textbf", "textit", "textsl", "textsc", "textup", "textmd", "textrm", "textsf", "texttt", "emph", "bfseries", "itshape", "em", "noindent", "tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE", "huge", "Huge", "\\"];
/// Commands memoir.cls defines that no package table has (`engine_memoir`): known only under
/// memoir.
pub(super) const MEMOIR_OWN: &[&str] = &["epigraph", "chapterstyle", "setsecnumdepth", "settocdepth"];
/// The packages taken under memoir: they change no command memoir.cls defines (memoir
/// emulates epigraph, so its `\usepackage` loads nothing).
pub(super) const MEMOIR_PACKAGES: &[&str] = &["inputenc", "fontenc", "lmodern", "natbib", "epigraph", "booktabs", "hyperref", "microtype", "graphicx", "amsmath"];
/// The environments taken under memoir.
pub(super) const MEMOIR_ENVS: &[&str] = &["document", "table", "tabular", "titlingpage"];
