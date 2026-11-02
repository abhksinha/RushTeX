//! KOMA-Script's scrreprt on the GDL path (`2026-10-31-w8-koma-classes.md`), on top of the
//! scrartcl base (`2026-10-31-native-koma.md`): the class table (`rushtex-gen class --class
//! scrreprt`), and scrreprt.cls's `\chapter` head and `\addchap` here. Everything else the
//! class changes against scrartcl or report stops in the pre-scan (`KOMA_LATER`).
//!
//! scrbook (`2026-11-01-w9-scrbook.md`) is scrreprt with these differences (`diff
//! scrreprt.cls scrbook.cls`), each native here or in the code it shares: `twoside=true`,
//! `\@openrighttrue` and `headings=openright` (`\chapter` starts with KOMA's
//! `\cleardoublepage`, a blank page in `empty` style), `bk1x.clo` sizes (the class table),
//! `\frontmatter`, `\mainmatter`, `\backmatter` (`koma_matter`; `\if@mainmatter` drops the
//! number of a chapter, and `\thesection`'s and `\theequation`'s `\thechapter.`, which
//! stop), `\pagestyle{headings}` at the class's end (`koma_page_tokens`, `\chaptermark`,
//! `\sectionmark`, `\@mkdouble`), no `abstract`.

use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::{Glue, Region};
use super::super::engine_node::Kind;
use super::super::engine_run::{Machine, Mode, SEMI_SIMPLE_GROUP, SIMPLE_GROUP};
use super::super::engine_token::Token;
use super::{Change, Op};

impl Machine<'_, '_> {
    /// `\subtitle`, `\subject`, `\publishers` (scrkernel-title: `\gdef\@subtitle{#1}`, ...).
    pub(super) fn koma_store(&mut self, i: usize) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        if let Some(doc) = self.doc.as_mut() {
            doc.pkg.koma.stored[i] = arg;
        }
    }

    /// scrreprt's `\maketitle` with `titlepage=true` (its default), no `\extratitle`,
    /// `\frontispiece`, `\titlehead`, `\dedication` or title backs (the pre-scan does not take
    /// them), one-sided:
    /// - `titlepage` (scrreprt.cls's own): `\cleardoublepage` (`\cleardoubleemptypage`:
    ///   `\clearpage{\pagestyle{empty}\clearpage}`, nothing left to ship by the second),
    ///   `\newpage`, `\thispagestyle{empty}`; its end `\newpage` only: the page counter is not
    ///   reset after the title (report's is);
    /// - `\setcounter{page}{1}`; `\let\footnote\thanks` and the footnote fonts (a note in the
    ///   title declines); `\setparsizes{\z@}{\z@}{\z@\@plus 1fil}`: `\parindent`, `\parskip`
    ///   0, `\parfillskip` `0pt plus 1fil`, local to the environment;
    /// - `\null\vfill`, `center` with `\@subject` (`\normalfont\normalcolor\bfseries\Large`,
    ///   then `\vskip 3em`) when given, `\@title` in `\usekomafont{title}` (`\sectfont`:
    ///   `\normalcolor\sffamily\bfseries`, no `\normalfont`) `\huge`, `\vskip 1em`,
    ///   `\@subtitle` (`\usekomafont{title}\large`) in a group, `\vskip 2em`, the author
    ///   tabular under `\Large\lineskip .75em`, `\vskip 1.5em`, `{\Large\@date\par}`, `\vskip
    ///   \z@ \@plus3fill`, `{\Large\@publishers\par}`, `\vskip 3em`; `\par`, `\vfill\null`;
    /// - `\setcounter{footnote}{0}`. `\and` is the kernel's (`\end{tabular}\hskip 1em
    ///   \@plus.17fil\begin{tabular}[t]{c}`).
    pub(super) fn koma_make_title(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let [title, author, date] = std::mem::take(&mut doc.stored);
        let [subtitle, subject, publishers] = std::mem::take(&mut doc.pkg.koma.stored);
        doc.thanks = Some(Vec::new());
        let (dated, authored) = (doc.dated, doc.authored);
        let foot = [self.named_id(named!("footnote")), self.doc.as_ref().map_or(0, |d| d.ids[doc!("thanks")])];
        if [&title, &author, &date, &subtitle, &subject, &publishers].iter().any(|l| l.iter().any(|t| t.cs_id().is_some_and(|c| foot.contains(&c)))) {
            return self.not_native("scrreprt: a footnote or \\thanks in the title page");
        }
        let date = if dated { date } else { self.tex_tokens("\\today") };
        let author = if authored { author } else { self.tex_tokens("\\relax") };
        // scrbook: KOMA's `\cleardoublepage` (`koma_clear_double_tail`), a blank page first
        // when the next one is even.
        let book = self.doc.as_ref().is_some_and(|d| d.pkg.koma.book);
        let mut t = if book { vec![self.doc_id(doc!("cleardoublepage"))] } else { vec![self.doc_id(doc!("clearpage")), self.doc_id(doc!("clearpage"))] };
        t.extend([self.named_cs(named!("begingroup")), self.doc_id(super::NEW_PAGE), self.doc_id(doc!("thispagestyle"))]);
        t.extend(self.tex_tokens("{empty}"));
        t.push(self.doc_id(doc!("setcounter")));
        t.extend(self.tex_tokens("{page}{1}\\parindent0pt\\parskip0pt\\parfillskip0pt plus1fil "));
        t.push(self.doc_id(doc!("null")));
        t.extend(self.tex_tokens("\\vfill\\begin{center}"));
        // microtype's `\leftprotrusion` after `center`'s `\item\relax` meets `\ifx` there.
        t.push(self.doc_id(doc!("rt@fi")));
        // `\sectfont`: `\normalcolor\sffamily\bfseries` (`normal_color`).
        let bold = |m: &mut Self, t: &mut Vec<Token>| t.extend([m.doc_id(doc!("rt@normalcolor")), m.doc_id(doc!("sffamily")), m.doc_id(doc!("bfseries"))]);
        if !subject.is_empty() {
            t.push(Token::char(Cat::BeginGroup, '{'));
            t.extend([self.doc_id(doc!("normalfont")), self.doc_id(doc!("rt@normalcolor")), self.doc_id(doc!("bfseries")), self.doc_id(doc!("Large"))]);
            t.extend(subject);
            t.extend([self.named_cs(named!("par")), Token::char(Cat::EndGroup, '}')]);
            t.extend(self.tex_tokens("\\vskip3em "));
        }
        t.push(Token::char(Cat::BeginGroup, '{'));
        bold(self, &mut t);
        t.push(self.doc_id(doc!("huge")));
        t.extend(title);
        t.extend([self.named_cs(named!("par")), Token::char(Cat::EndGroup, '}')]);
        t.extend(self.tex_tokens("\\vskip1em{"));
        if !subtitle.is_empty() {
            bold(self, &mut t);
            t.push(self.doc_id(doc!("large")));
            t.extend(subtitle);
            t.push(self.named_cs(named!("par")));
        }
        t.extend(self.tex_tokens("}\\vskip2em{"));
        t.push(self.doc_id(doc!("Large")));
        t.extend(self.tex_tokens("\\lineskip.75em \\begin{tabular}[t]{c}"));
        t.extend(author);
        t.extend(self.tex_tokens("\\end{tabular}"));
        t.extend([self.named_cs(named!("par")), Token::char(Cat::EndGroup, '}')]);
        t.extend(self.tex_tokens("\\vskip1.5em{"));
        t.push(self.doc_id(doc!("Large")));
        t.extend(date);
        t.extend([self.named_cs(named!("par")), Token::char(Cat::EndGroup, '}')]);
        t.extend(self.tex_tokens("\\vskip0pt plus3fill{"));
        t.push(self.doc_id(doc!("Large")));
        t.extend(publishers);
        t.extend([self.named_cs(named!("par")), Token::char(Cat::EndGroup, '}')]);
        t.extend(self.tex_tokens("\\vskip3em\\end{center}"));
        t.push(self.named_cs(named!("par")));
        t.extend(self.tex_tokens("\\vfill"));
        t.extend([self.doc_id(doc!("null")), self.doc_id(super::NEW_PAGE), self.named_cs(named!("endgroup"))]);
        t.push(self.doc_id(doc!("rt@titleend")));
        self.back_list(&t);
    }

    /// After scrreprt's `\tableofcontents` heading (`\chapter*{\contentsname}`, tocbasic's
    /// `\tocbasic@listhead`): `\ifvmode\@tempcnta\lastpenalty\@tempskipa\lastskip\unskip\fi`,
    /// the mark and `\endgroup`, `\ifvmode\penalty\@tempcnta\vskip\@tempskipa\fi`; then
    /// `\tocbasic@starttoc`'s parskip fake (`\@tempskipa\lastskip \vskip-\lastskip
    /// \addtolength{\@tempskipa}{\parskip}\vskip\@tempskipa`), and in its group `\parskip\z@`,
    /// `\parindent\z@`, `\parfillskip\z@\@plus 1fil` and microtype's `protrusion=false`
    /// (`\pdfprotrudechars` 0); then the first entry.
    pub(super) fn koma_toc_open(&mut self) {
        use super::super::engine_linebreak::PAR_FILL_SKIP;
        if self.mode == Mode::Vertical {
            let p = self.last_penalty();
            let s = self.last_skip();
            self.unskip();
            // `\@mkdouble{\MakeMarkcase{\contentsname}}` in the group (scrbook's `headings`).
            self.koma_mkdouble("Contents");
            self.vpenalty(p);
            self.vskip(s);
            let s = self.last_skip();
            let parskip = self.glue_param(super::super::engine_pack::PAR_SKIP);
            self.vskip(super::neg(s));
            self.vskip(super::glue_add(s, parskip));
        }
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.set_glue_named(param!("parskip"), Glue::default());
        self.set_local(Region::Dimen, super::super::engine_pack::PAR_INDENT, 0);
        self.set_skip(PAR_FILL_SKIP, Glue { stretch: 1 << 16, stretch_order: 1, ..Glue::default() });
        if self.microtype() {
            self.set_int_named(param!("pdfprotrudechars"), 0);
        }
        self.koma_toc_entry();
    }

    /// The next entry, as tocbasic's `tocline` style sets the `.toc` file's `\contentsline`
    /// (`\scr@dte@tocline`); after the last, `\@nobreakfalse` and the group's end.
    pub(super) fn koma_toc_entry(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let Some(title) = doc.toc_entry(doc.toc_next).map(|e| e.title.clone()) else {
            doc.nobreak = false;
            return self.end_group(SEMI_SIMPLE_GROUP);
        };
        let mut title_toks = Vec::new();
        self.title_tokens(&title, &mut title_toks);
        let mut t = vec![self.doc_id(doc!("rt@kmtocline"))];
        t.append(&mut title_toks);
        t.extend([self.doc_id(doc!("rt@kmtoctail")), self.named_cs(named!("par")), self.doc_id(doc!("rt@kmtocend"))]);
        self.back_list(&t);
    }

    /// `\scr@dte@tocline{level}`'s start: `\tocbasic@DependOnPenaltyAndTOCLevel` (a
    /// `\lastpenalty` below `20009-level` runs `\LastTOCLevelWasHigher`, equal to it
    /// `\LastTOCLevelWasSame`, both `\addpenalty{\@lowpenalty}` (51); above it
    /// `\LastTOCLevelWasLower`, nothing), then `\addvspace` the style's before-skip (chapter
    /// `1.0em \@plus\p@`, section and below `\z@ \@plus .2\p@`).
    pub(super) fn koma_toc_line(&mut self) {
        let Some(level) = self.koma_toc_level() else { return };
        let last = self.last_penalty();
        let mut ops = Vec::new();
        if self.mode == Mode::Vertical && last <= 20009 - level as i32 {
            self.add_penalty(51, &mut ops);
        }
        ops.push(Op::Call(|m| {
            let Some(level) = m.koma_toc_level() else { return };
            let g = m.glue_of(if level == 0 { "1.0em plus 1pt" } else { "0pt plus .2pt" });
            m.add_vspace(g);
            m.koma_toc_group();
        }));
        self.run_ops(ops);
    }

    /// The entry's group: `\leftskip` the indent, `\rightskip` `\@tocrmarg` (2.55em),
    /// `\parfillskip-\rightskip`, `\parindent` the indent, `\@afterindenttrue`,
    /// `\interlinepenalty\@M` (scrreprt's indents: chapter 0, section 1.5em, subsection
    /// 3.8em; number widths 1.5em, 2.3em, 3.2em), then `\leavevmode` (`rt@kmtochead`).
    fn koma_toc_group(&mut self) {
        use super::super::engine_linebreak::{INTER_LINE_PENALTY, LEFT_SKIP, PAR_FILL_SKIP, RIGHT_SKIP};
        let Some(level) = self.koma_toc_level() else { return };
        self.begin_group(SIMPLE_GROUP);
        let indent = self.glue_of(["0pt", "1.5em", "3.8em"][level.min(2)]).width;
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
        let t = self.doc_id(doc!("rt@kmtochead"));
        self.input.back(t);
    }

    /// `\leavevmode`, `\advance\leftskip` the number width, `\null\nobreak\hskip-\leftskip`,
    /// then `{` the entry format (chapter: `\usekomafont{chapterentry}`, `\sffamily
    /// \bfseries`) and `\numberline{N}`: `\hb@xt@` the number width `{N\autodot\hfil}`;
    /// an unnumbered entry (`\nonumberline`) has no box.
    pub(super) fn koma_toc_head(&mut self) {
        use super::super::engine_linebreak::LEFT_SKIP;
        if self.leave_vmode() {
            return;
        }
        let Some((level, number)) = self.toc_current() else { return };
        let w = self.glue_of(["1.5em", "2.3em", "3.2em"][level.min(2)]).width;
        let left = self.skip(LEFT_SKIP);
        let left = Glue { width: left.width + w, ..left };
        self.set_skip(LEFT_SKIP, left);
        self.build_hbox(|_| {});
        self.build_penalty(10000);
        self.build_glue(super::neg(left), false);
        self.begin_group(SIMPLE_GROUP);
        if level == 0 {
            // `\usekomafont{chapterentry}`: `\usekomafont{disposition}`, `\normalcolor\sffamily
            // \bfseries`; the pop at the entry's group end (`koma_toc_end`).
            if self.colored() {
                self.push_default_color();
            }
            self.font_change(Change::Default(1));
            self.font_change(Change::Series("bx"));
        }
        // hyperref's `\contentsline`: the link around `\numberline` and the title.
        self.contents_link_start(level, &number);
        if super::unnumbered(&number) || number.is_empty() {
            return;
        }
        self.build_hbox_to(w, |m| {
            m.build_text(number.as_bytes());
            m.build_glue(Glue { stretch: 1 << 16, stretch_order: 1, ..Glue::default() }, false);
        });
    }

    /// After the title: `\nobreak{\normalfont\normalsize` the line fill (chapter `\hfill`,
    /// below `\TOCLineLeaderFill`: `\leaders\hbox{$\m@th\mkern4.5mu\hbox{.}\mkern4.5mu$}
    /// \hfill`) `\nobreak` and the page number box `\hb@xt@\@pnumwidth{\hfil page}` in the
    /// page number format (chapter `\sffamily\bfseries`, below `\normalfont\normalcolor`)`}`.
    pub(super) fn koma_toc_tail(&mut self) {
        let Some((level, _)) = self.toc_current() else { return };
        self.contents_link_end();
        self.build_penalty(10000);
        self.begin_group(SIMPLE_GROUP);
        self.normal_font();
        self.size_command(super::NORMALSIZE);
        let fill = Glue { stretch: 1 << 16, stretch_order: 2, ..Glue::default() };
        if level == 0 {
            self.build_glue(fill, false);
        } else {
            self.build_leaders(fill, |m| {
                m.push_text_math();
                m.check_math_fonts();
                m.math_surround_zero();
                m.build_mkern(9 << 15);
                m.build_hbox(|m| m.build_text(b"."));
                m.build_mkern(9 << 15);
                m.after_math();
            });
        }
        self.build_penalty(10000);
        let w = self.glue_of("1.55em").width;
        self.begin_group(SIMPLE_GROUP);
        if level == 0 {
            self.font_change(Change::Default(1));
            self.font_change(Change::Series("bx"));
        }
        let colored = self.colored();
        self.build_hbox_to(w, |m| {
            m.build_glue(Glue { stretch: 1 << 16, stretch_order: 1, ..Glue::default() }, false);
            // The page number format's `\normalcolor`: a push in the box, its
            // `\aftergroup\reset@color` after the box.
            if colored {
                m.push_default_color();
            }
            m.build_text(b"0");
        });
        self.toc_page_box();
        if colored {
            self.color_stack(None);
        }
        self.end_group(SIMPLE_GROUP);
        self.end_group(SIMPLE_GROUP);
    }

    /// After the entry's `\@par`: the entry format's group and the entry's group end,
    /// `\tocbasic@SetPenaltyByTOCLevel` (`\penalty` `20009-level`), the next entry.
    pub(super) fn koma_toc_end(&mut self) {
        let Some(level) = self.koma_toc_level() else { return };
        self.end_group(SIMPLE_GROUP);
        self.end_group(SIMPLE_GROUP);
        // The chapter entry format's `\normalcolor`: `\reset@color` after the group.
        if level == 0 && self.colored() {
            self.color_stack(None);
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.afterindent = doc.toc_afterindent.pop().unwrap_or(doc.afterindent);
            doc.toc_next += 1;
        }
        self.vpenalty(20009 - level as i32);
        self.koma_toc_entry();
    }

    /// `\lastpenalty` (§424): the current list's last node, or the page builder's once the
    /// contributions are gone.
    fn last_penalty(&self) -> i32 {
        self.last_item(Kind::Penalty).or_else(|| self.page_last(Kind::Penalty)).map_or(0, |n| n.a)
    }

    fn koma_toc_level(&self) -> Option<usize> {
        self.toc_current().map(|e| e.0)
    }

    /// scrreprt's `\@makecaption` (`captions=oneline`, the default): `\vskip
    /// \abovecaptionskip`, then `\@@makecaption`: `\settowidth` of
    /// `\scr@@makesinglelinecaption` (`\strut\ignorespaces{{\tableformat\captionformat}}`
    /// the text `\unskip`; `\tableformat` `\tablename\nobreakspace\thetable\autodot`,
    /// `\captionformat` `:\ `) against `\hsize`; set in the box here, measured in
    /// `koma_caption_mid`.
    pub(super) fn koma_caption_body(&mut self) {
        use super::super::engine_box::{Ctx, HBOX_GROUP};
        if self.koma_front_numbers("a caption") {
            return;
        }
        self.aux_write(0);
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.par_box_restore();
        if self.fl().minipage {
            self.set_minipage();
        }
        self.size_command(super::NORMALSIZE);
        let g = self.rg(super::R_ABOVECAPTIONSKIP);
        self.build_glue(g, false);
        self.open_box(HBOX_GROUP, Ctx::Set { n: super::TEMP_BOX, global: false }, false, 0, false);
        let t = self.doc.as_ref().map_or(1, |d| d.floats.captype);
        let name = if t == 1 { (named!("figurename"), "Figure") } else { (named!("tablename"), "Table") };
        // With a colour package, `\usekomafont{caption}` and `{\usekomafont{captionlabel}..}`
        // are `\normalcolor`: a push first in the box (its pop after the line's box,
        // `koma_caption_mid`), and one around the label.
        let colored = self.colored();
        if colored {
            self.push_default_color();
        }
        let mut out = vec![self.doc_id(doc!("strut")), self.named_cs(named!("ignorespaces"))];
        if colored {
            out.extend([Token::char(Cat::BeginGroup, '{'), self.doc_id(doc!("rt@normalcolor"))]);
        }
        let id = self.named_id(name.0);
        if matches!(self.e.eqtb.meaning(id), super::super::engine_prim::Meaning::Macro(_)) {
            out.push(Token::cs(id));
        } else {
            out.extend(name.1.chars().map(|c| Token::char(Cat::Letter, c)));
        }
        out.push(self.named_cs(named!("nobreakspace")));
        let n = self.rw(super::R_FIGURE + t as usize - 1);
        let text = self.float_the(t as usize - 1, n).unwrap_or_default();
        out.extend(format!("{text}:").chars().map(|c| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c)));
        out.extend(self.tex_tokens("\\ "));
        if colored {
            out.push(Token::char(Cat::EndGroup, '}'));
        }
        out.push(self.named_cs(named!("ignorespaces")));
        // hyperref's `\@caption`: `\Hy@raisedlink{\hyper@@anchor{\@currentHref}{\relax}}` first.
        if self.hyper() {
            let name = self.float_href(t, n);
            let link = self.text_raised_link(name);
            out.extend(link);
        }
        let caption = self.fl().caption.clone();
        out.extend(caption);
        out.extend(self.tex_tokens("\\unskip "));
        out.push(self.doc_id(doc!("rt@kmcapmid")));
        self.back_list(&out);
    }

    /// The rest of `\@@makecaption` for a caption no wider than `\hsize`
    /// (`\scr@makesinglelinecaption`): `\hb@xt@\hsize{\hfill` the caption `\hfill}\par
    /// \kern\dp\strutbox`, `\kern-\dp\strutbox`; then `\@belowcaptionbreak` (`\nopagebreak`)
    /// `\vskip\belowcaptionskip\@belowcaptionbreak`, `\@caption`'s `\par` and `\endgroup`. A
    /// wider one (`\scr@makemultilinecaption`) is not native yet.
    pub(super) fn koma_caption_mid(&mut self) {
        use super::super::engine_node::Node;
        self.package();
        let Some(b) = self.e.eqtb.box_reg(super::TEMP_BOX) else { return };
        let hsize = self.dimen_param(super::super::engine_pack::HSIZE);
        if self.e.nodes.boxes[b as usize].width > hsize {
            return self.not_native("a KOMA caption wider than the line");
        }
        self.fl().minipage = false;
        self.e.eqtb.replace_box(super::TEMP_BOX, None);
        let r = self.e.nodes.boxes[b as usize];
        let fill = Glue { stretch: 1 << 16, stretch_order: 2, ..Glue::default() };
        let start = self.cur.len();
        let n = self.e.nodes.glue_node(fill, 0);
        self.cur.push(n);
        self.cur.extend_from_slice(self.e.nodes.get(r.list));
        self.e.nodes.free_block(r.list);
        self.e.nodes.free_box(b);
        let n = self.e.nodes.glue_node(fill, 0);
        self.cur.push(n);
        let h = self.hpack(start, hsize, true);
        let h = self.e.nodes.add_box(h);
        self.append_to_vlist(h);
        // `\usekomafont{caption}`'s `\normalcolor` ran in the `\hb@xt@\hsize` group: its
        // `\reset@color` after the box.
        if self.colored() {
            self.color_stack(None);
        }
        self.par();
        let d = self.e.eqtb.word(super::STRUT_DP);
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, d) });
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, -d) });
        self.cur.push(Node::new(Kind::Penalty, 10000));
        let g = self.rg(super::R_BELOWCAPTIONSKIP);
        self.build_glue(g, false);
        self.cur.push(Node::new(Kind::Penalty, 10000));
        self.par();
        self.end_group(SEMI_SIMPLE_GROUP);
    }

    /// scrreprt's `\addchap` (`\SecDef\@addchap\@saddchap`): `\@addchap[#1]#2` is
    /// `\chapter[{#1}]{#2}` with `\c@secnumdepth` at `\chapternumdepth-1` (no number, a
    /// contents line `\nonumberline`, the `lof`/`lot` gaps), `\@saddchap{#1}` is
    /// `\chapter*{#1}\addchapmark{}` (no mark under `plain`).
    pub(super) fn koma_addchap(&mut self) {
        let star = self.next_is('*');
        let mut t = vec![self.doc_id(doc!("chapter"))];
        if star {
            t.push(Token::char(Cat::Other, '*'));
            // scrbook: `\addchapmark{}` after the head.
            if self.doc.as_ref().is_some_and(|d| d.pkg.koma.book) {
                let mut title = Vec::new();
                self.read_arg(&mut title);
                t.push(Token::char(Cat::BeginGroup, '{'));
                t.extend(title);
                t.extend([Token::char(Cat::EndGroup, '}'), self.doc_id(doc!("rt@kmaddchapmk"))]);
            }
        } else if let Some(doc) = self.doc.as_mut() {
            doc.pkg.koma.addchap = true;
        }
        self.back_list(&t);
    }

    /// scrreprt's `\scr@makechapterhead{chapter}` (`headings=big`, `chapterprefix=false`),
    /// after `\scr@@startchapter`'s counter and `.aux` writes (`chapter_head`):
    /// - `\use@chapter@o@preamble`: in a group, `\vbox to\z@{\vskip-\baselineskip \vbox
    ///   to\@tempdima{\vfill\box\z@}\vss}\vskip-\parskip\vskip-\baselineskip`, `\@tempdima`
    ///   the height of `\vbox{\chapterheadstartvskip}` and `\box\z@` the empty preamble;
    /// - `\scr@@makechapterhead`: `\@tempskipa` is `\scr@chapter@beforeskip`
    ///   (`-3.3\baselineskip-\parskip`), negated (`afterindent=bysign`: `\@afterindentfalse`),
    ///   `\chapterheadstartvskip` (`\vspace{\@tempskipa}`: the skip and `\vskip\z@skip`);
    ///   in a group `\parindent\z@`, `\parfillskip\z@ plus 1fil`, `\normalfont
    ///   \usekomafont{disposition}` (`\normalcolor\sffamily\bfseries`), `\usekomafont
    ///   {chapter}` (`\huge`), `\raggedchapter` (`\raggedright`), `\chapterlinesformat`:
    ///   `\@hangfrom{\chapterformat}{\interlinepenalty\@M #2\@@par}`, `\chapterformat` being
    ///   `\mbox{\thechapter\autodot\enskip}` (no `\autodot` while every number is arabic),
    ///   empty without a number; then `\nobreak\par\nobreak` and `\vskip` the after-skip
    ///   (`1.725\baselineskip \@plus .115\baselineskip \@minus .192\baselineskip`);
    /// - `\@afterheading`.
    pub(super) fn koma_chapter_head(&mut self, number: &str, mut title: Vec<Token>) {
        let mut t = self.tex_tokens("{\\vbox to0pt{\\vskip-\\baselineskip\\vbox to\\dimexpr3.3\\baselineskip+\\parskip\\relax{\\vfill\\vbox{}}\\vss}\\vskip-\\parskip\\vskip-\\baselineskip}\\vskip\\glueexpr3.3\\baselineskip+\\parskip\\relax\\vskip0pt{\\parindent0pt\\parfillskip0pt plus1fil ");
        // `\usekomafont{disposition}`'s `\normalcolor` (`normal_color`).
        t.extend([self.doc_id(doc!("normalfont")), self.doc_id(doc!("rt@normalcolor")), self.doc_id(doc!("sffamily")), self.doc_id(doc!("bfseries")), self.doc_id(doc!("huge")), self.doc_id(doc!("raggedright"))]);
        t.extend(self.tex_tokens("\\setbox32701\\hbox{{"));
        if !number.is_empty() {
            t.extend(self.tex_tokens(&format!("\\hbox{{{number}\\hskip.5em}}")));
        }
        t.extend(self.tex_tokens("}}\\hangindent\\wd32701 \\noindent\\box32701 \\interlinepenalty10000 "));
        t.append(&mut title);
        t.push(self.named_cs(named!("par")));
        t.push(Token::char(Cat::EndGroup, '}'));
        t.extend([self.doc_id(doc!("nobreak")), self.named_cs(named!("par")), self.doc_id(doc!("nobreak"))]);
        t.extend(self.tex_tokens("\\vskip1.725\\baselineskip plus.115\\baselineskip minus.192\\baselineskip"));
        t.push(self.doc_id(doc!("rt@afterheading")));
        self.back_list(&t);
    }

    /// scrbook's `\cleardoublepage` after its first `\clearpage` (`\cleardoubleemptypage`:
    /// `\clearpage{\pagestyle{empty}\cleardoubleoddstandardpage}`, the latter the kernel's
    /// `\cleardoublepage`): a second `\clearpage` (its `\write\m@ne{}`), then
    /// [`Self::koma_blank`].
    pub(super) fn koma_clear_double_tail(&mut self) {
        let t = [self.doc_id(doc!("clearpage")), self.doc_id(doc!("rt@kmblank"))];
        self.back_list(&t);
    }

    /// `\if@twoside\ifodd\c@page\else\hbox{}\newpage\fi\fi` in `\pagestyle{empty}`'s group:
    /// the blank page ships in `empty` style (as `\thispagestyle{empty}` would; no mark is
    /// put in between).
    pub(super) fn koma_blank(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.class.flag("twoside") && doc.page % 2 == 0 {
            doc.special = Some(true);
            let mut t = self.tex_tokens("\\hbox{}");
            t.push(self.doc_id(super::NEW_PAGE));
            self.back_list(&t);
        }
    }

    /// scrbook's `\frontmatter` (`0`) and `\mainmatter` (`1`): `\if@twoside
    /// \cleardoubleoddpage\else\clearpage\fi`, `\@mainmatterfalse` or `true`,
    /// `\pagenumbering{roman}` or `{arabic}`; `\backmatter` (`2`): `\if@openright
    /// \cleardoubleoddpage\else\clearpage\fi \@mainmatterfalse`. `\cleardoubleoddpage` is
    /// `\cleardoubleoddemptypage`, the default `\cleardoublepage`. Their star and optional
    /// numbering (`\NewDocumentCommand{so}`) stop.
    pub(super) fn koma_matter(&mut self, which: u8) {
        if self.next_is('*') || self.next_is('[') {
            return self.not_native("scrbook: \\frontmatter* or a numbering argument");
        }
        let Some(doc) = self.doc.as_mut() else { return };
        doc.mainmatter = which == 1;
        let double = if which < 2 { doc.class.flag("twoside") } else { doc.openright };
        let mut t = vec![self.doc_id(if double { doc!("cleardoublepage") } else { doc!("clearpage") })];
        if which < 2 {
            t.push(self.doc_id(doc!("pagenumbering")));
            t.extend(self.tex_tokens(if which == 0 { "{roman}" } else { "{arabic}" }));
        }
        self.back_list(&t);
    }

    /// scrbook outside the main matter: `\thesection` and `\theequation` drop
    /// `\thechapter.` (`\if@mainmatter`), and a caption's number is the chapter's 0 or
    /// last; neither is native yet.
    pub(super) fn koma_front_numbers(&mut self, what: &str) -> bool {
        if self.doc.as_ref().is_some_and(|d| d.pkg.koma.book && !d.mainmatter) {
            self.not_native(&format!("scrbook: {what} outside the main matter"));
            return true;
        }
        false
    }

    /// scrbook's `\chaptermark{title}` (`\ps@headings`, set at the class's end and kept by
    /// `\ps@plain` and `\ps@empty`): two-sided `\markboth{\Ifnumbered{chapter}
    /// {\if@mainmatter\MakeMarkcase{\chaptermarkformat}\fi}{}\MakeMarkcase{title}}{}`, one-sided
    /// `\markright{…}` (whose argument is never empty as tokens: `mark_right_tokens`).
    /// `\MakeMarkcase` is `\@firstofone`; `\chaptermarkformat` is
    /// `\chapappifchapterprefix{\ }\thechapter\autodot\enskip`, which `\markboth`'s
    /// `\protected@edef` makes `N\hskip.5em\relax` (`chapterprefix=false`, no `\autodot`
    /// while the number is arabic). `number` is empty for an unnumbered chapter.
    pub(super) fn koma_chapter_mark(&mut self, number: &str, title: &[Token]) {
        if !Self::plain_mark_text(title) || number.bytes().any(|b| !b.is_ascii_digit()) {
            return self.not_native("scrbook: a chapter mark that is not plain text");
        }
        let mut text = Vec::new();
        if !number.is_empty() {
            text = self.tex_tokens(&format!("{number}\\hskip.5em\\relax"));
        }
        text.extend_from_slice(title);
        if self.doc.as_ref().is_some_and(|d| d.class.flag("twoside")) {
            self.mark_both(&text, &[]);
        } else {
            self.mark_right_tokens(&text);
        }
    }

    /// scrbook's two-sided `\sectionmark{title}`: `\markright{\Ifnumbered{section}
    /// {\MakeMarkcase{\sectionmarkformat}}{}\MakeMarkcase{title}}` (`\thesection\autodot
    /// \enskip`); one-sided it puts nothing.
    pub(super) fn koma_section_mark(&mut self, number: &str, title: &[Token]) {
        if !self.doc.as_ref().is_some_and(|d| d.class.flag("twoside")) {
            return;
        }
        if !Self::plain_mark_text(title) || number.bytes().any(|b| !b.is_ascii_digit() && b != b'.') {
            return self.not_native("scrbook: a section mark that is not plain text");
        }
        let mut text = if number.is_empty() { Vec::new() } else { self.tex_tokens(&format!("{number}\\hskip.5em\\relax")) };
        text.extend_from_slice(title);
        self.mark_right_tokens(&text);
    }

    /// `\@mkdouble{\MakeMarkcase{name}}` (the contents' and the bibliography's heads):
    /// `\@mkboth{name}{name}`, `\markboth` under `headings`, nothing under `plain` and
    /// `empty`.
    pub(super) fn koma_mkdouble(&mut self, name: &str) {
        if !self.doc.as_ref().is_some_and(|d| d.headings) {
            return;
        }
        let text = self.tex_tokens(name);
        self.mark_both(&text, &text);
    }

    /// `\@saddchap`'s `\addchapmark{}` after `\chapter*`: `\chaptermark{}` with
    /// `\c@secnumdepth` below the chapter's, `\markboth{}{}` two-sided.
    pub(super) fn koma_addchap_mark(&mut self) {
        if self.doc.as_ref().is_some_and(|d| d.pkg.koma.book) {
            self.koma_chapter_mark("", &[]);
        }
    }

    /// scrbook's page of style `headings` (`head`) or `plain` (scrkernel-pagestyles), in
    /// `\@outputpage`'s `\reset@font\normalsize`: the head into [`HEAD_BOX`] as `\hb@xt@
    /// \textwidth{\@thehead}` (empty under `plain`), the foot's material for `\hb@xt@
    /// \textwidth{\@thefoot}`. With `\@tempdima` the text width (`\set@tempdima@hw`, no
    /// `mpinclude`), no head or foot rule:
    /// - `\@oddhead`: `\hb@xt@\@tempdima{\vbox{\hb@xt@\@tempdima{{\headfont\hfil\strut
    ///   \rightmark\if@twoside\else\hfil\fi}}}}\hss`;
    /// - `\@evenhead`: `\hss\hb@xt@\@tempdima{\vbox{\hb@xt@\@tempdima{{\headfont\strut
    ///   \leftmark\hfil}}}}`;
    /// - `\@oddfoot` and `\@evenfoot` the same with `\pagemark` (`{\usekomafont{pagenumber}
    ///   {\thepage}}`, `\pnumfont`) for the mark.
    ///
    /// `\headfont` is `\normalfont\normalcolor\slshape`, `\pnumfont` `\normalfont\normalcolor`.
    pub(super) fn koma_page_tokens(&mut self, page: i32, head: bool) -> Vec<Token> {
        use super::book::{LEFT, RIGHT};
        let Some(doc) = self.doc.as_ref() else { return Vec::new() };
        let twoside = doc.class.flag("twoside");
        let even = twoside && page % 2 == 0;
        let w = doc.class.dimen("textwidth");
        let digits = super::book::label_digits(&self.the_page(page));
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let box_to = |m: &mut Self| m.tex_tokens(&format!("\\hbox to{w}sp{{\\vbox{{\\hbox to{w}sp{{{{"));
        let one = if twoside { "" } else { "\\hfil " };
        let mut t = self.tex_tokens(&format!("\\begingroup\\normalfont\\normalsize\\global\\setbox{}\\hbox to{w}sp{{", super::fancy::HEAD_BOX));
        if head {
            let mark = self.page_mark_text(if even { LEFT as u32 } else { RIGHT as u32 }, !even).unwrap_or_default();
            if even {
                t.extend(self.tex_tokens("\\hss "));
            }
            t.extend(box_to(self));
            t.extend(self.tex_tokens("\\normalfont "));
            t.push(self.doc_id(doc!("rt@normalcolor")));
            t.extend(self.tex_tokens(if even { "\\slshape\\strut " } else { "\\slshape\\hfil\\strut " }));
            t.extend(mark);
            t.extend(self.tex_tokens(if even { "\\hfil}}}}" } else { one }));
            if !even {
                t.extend(self.tex_tokens("}}}}\\hss "));
            }
        }
        t.push(close);
        let mut pagemark = vec![open];
        pagemark.extend(self.tex_tokens("\\normalfont "));
        pagemark.push(self.doc_id(doc!("rt@normalcolor")));
        pagemark.push(open);
        pagemark.extend(digits);
        pagemark.extend([close, close]);
        if even {
            t.extend(self.tex_tokens("\\hss "));
        }
        t.extend(box_to(self));
        if !even {
            t.extend(self.tex_tokens("\\hfil "));
        }
        t.extend(pagemark);
        t.extend(self.tex_tokens(if even { "\\hfil}}}}" } else { one }));
        if !even {
            t.extend(self.tex_tokens("}}}}\\hss "));
        }
        t.extend(self.tex_tokens("\\endgroup "));
        t
    }
}

/// scrbook's hooks (`2026-11-01-w9-scrbook.md`): KOMA's `\cleardoublepage` and the matters.
pub(super) const BOOK_HOOKS: super::ClassHooks = super::ClassHooks { clear_double_tail: |m| m.koma_clear_double_tail(), matter: |m, k| m.koma_matter(k), ..super::KERNEL_HOOKS };

/// KOMA-Script's natives, in `Meaning::Doc` order after the kernel's (`engine_doc::DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("addchap", |m| m.koma_addchap()),
    ("subtitle", |m| m.koma_store(0)),
    ("subject", |m| m.koma_store(1)),
    ("publishers", |m| m.koma_store(2)),
    ("rt@kmtocopen", |m| m.koma_toc_open()),
    ("rt@kmcapmid", |m| m.koma_caption_mid()),
    ("rt@kmtocline", |m| m.koma_toc_line()),
    ("rt@kmtochead", |m| m.koma_toc_head()),
    ("rt@kmtoctail", |m| m.koma_toc_tail()),
    ("rt@kmtocend", |m| m.koma_toc_end()),
    ("rt@kmblank", |m| m.koma_blank()),
    ("rt@kmaddchapmk", |m| m.koma_addchap_mark()),
];

/// What scrartcl.cls (KOMA-Script) defines otherwise than article.cls and the native job
/// does not take yet: the list is the difference of every control sequence's meaning after
/// `\begin{document}` under the two classes (`2026-10-31-native-koma.md`). Its headings
/// (`\scr@startsection`, `\sectfont`), `\maketitle`, footnotes (`\@makefntext` with
/// `\deffootnote`), captions and floats (`\@makecaption`, `\tb@atbegin@after`), the KOMA
/// table of contents (tocbasic), `\appendix`, `\cleardoublepage`, the `headings` page styles.
pub(super) const KOMA_LATER: &[&str] = &["subparagraph", "part", "maketitle", "footnotetext", "footnotemark", "caption", "tableofcontents", "listoffigures", "listoftables", "appendix", "cleardoublepage", "printindex", "markboth", "markright", "numberwithin"];
/// scrartcl's environments the native job does not take yet: `abstract`, `titlepage`, the
/// floats, the bibliography and the index. (Its lists are native: the labels go through
/// `\usekomafont` to the same fonts but `\labelenumii`, `\theenumii)`, and the description
/// label's `\descfont`; `quotation` has `\listparindent 1em`; `\@list@extra` changes
/// nothing while `\parskip` is not positive.)
pub(super) const KOMA_LATER_ENVS: &[&str] = &["abstract", "titlepage", "figure", "table", "figure*", "table*", "thebibliography", "theindex"];
/// Packages that act on what scrartcl changes: its page styles (fancyhdr), headings
/// (titlesec, hyperref's anchors), captions and floats (float, longtable), lists
/// (enumitem, enumerate), the paper (geometry reads it from the class options, KOMA's is a4
/// without one), `\parskip` (parskip), `\baselinestretch` (setspace, KOMA's is `1`), the
/// captions names (babel), the bibliography (natbib).
pub(super) const KOMA_LATER_PACKAGES: &[&str] = &["fancyhdr", "titlesec", "natbib", "babel", "parskip", "geometry", "enumitem", "float", "longtable", "setspace", "enumerate"];

/// KOMA-Script's state (`Packages::koma`).
#[derive(Default, Clone, PartialEq)]
pub(super) struct KomaState {
    /// scrreprt's `\addchap` (`koma_addchap`): the next `\chapter` has no number
    /// (`\c@secnumdepth` below the chapter level).
    pub(super) addchap: bool,
    /// KOMA-Script's `\subtitle`, `\subject` and `\publishers` (`koma_store`), for
    /// `koma_make_title`.
    pub(super) stored: [Vec<Token>; 3],
    /// scrbook (`2026-11-01-w9-scrbook.md`): two-sided `headings`, the matters, `openright`.
    pub(super) book: bool,
    /// scrbook: the next `\section`'s short title, for its mark.
    pub(super) short: Option<Vec<Token>>,
}
