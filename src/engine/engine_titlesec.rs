//! titlesec on the GDL path, for article: `\section`, `\subsection` and `\subsubsection` as
//! titlesec 2.17 makes them (`\ttl@straight@i`, `\ttl@straight@ii`, `\ttl@select`), with
//! `\titleformat` (shapes hang, block and display), `\titleformat*`, `\titlespacing` and
//! `\titlerule`, and the package options that only change formats and spacing.
//!
//! Loading titlesec changes every heading, `\titleformat` or not. `\ttl@extract` turns
//! article's `\@startsection` arguments into a hang format `{\normalfont<size>\bfseries}
//! {\@seccntformat{level}}{\z@}{}` and `\titlespacing*{level}{\z@}{<before>}{<after>}`,
//! both skips evaluated at the load. A heading is then, after `\par`:
//! - `\ttl@labelling`: `\refstepcounter` (hyperref's anchor, vertical, before the skip);
//! - `\addpenalty\@secpenalty \addvspace<before>` unless `\if@nobreak` (`aftersep`);
//! - `\ttl@select`'s group: `\everypar{\setbox\z@\lastbox\ttl@strut}`, then the shape
//!   (`\ttlh@hang`): `\leftskip`, `\rightskip`, `\interlinepenalty\@M`, the format, and
//!   `{\parindent\z@ \begingroup \noindent \sbox\z@{label\strut\hspace{sep}} \hangindent
//!   \wd\z@ \box\z@ {title}\kern\z@\strut\@@par \endgroup \nobreak after\@@par}`;
//! - `\ttl@finmarks`: `\sectionmark`, `\addcontentsline` and `\nobreak` (unstarred only);
//! - `\par\nobreak\vspace{<after>}\@afterheading` (`\vspace` adds `\vskip\z@skip`).
//!
//! Under report (no package option) the three levels are report's, one below `\chapter`,
//! and `\titleformat{\chapter}` makes report's `\@makechapterhead` and
//! `\@makeschapterhead` titlesec's `\ttl@mkchap` (`\ttl@compatchapter`), with the spacing
//! titlesec gives `\chapter` at its load (`\titlespacing*\chapter{\z@}{50\p@}{40\p@}`,
//! `rubberchapters`): `\vspace*{before}`, `\ttl@select`'s group and shape, `\ttl@calc
//! \vspace{after}` and `\ttl@finmarks`; report's `\@chapter` keeps its counter, contents
//! line, `\chaptermark` and `\@afterheading` (`ttl_chapter_head`).
//!
//! Run-in shapes (article's `\paragraph`), `\part`, book and the other classes, the other
//! options and anything in a format argument beyond font switches and `\color` go to the
//! engine (the pre-scan, [`call_len`] and [`options_ok`]).

use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::Glue;
use super::super::engine_run::{Machine, Mode};
use super::super::engine_token::Token;
use super::{glue_add, heading_number, neg, Op, SECS, SEC_PENALTY};

/// A skip of `\titlespacing`: evaluated at the load (`\ttl@extract`), a multiple of
/// `\beforetitleunit`/`\aftertitleunit` (`*n`), or the document's text for `\setlength`.
#[derive(PartialEq, Clone)]
pub(super) enum Space {
    Fixed(Glue),
    Units(i32),
    Text(Vec<Token>),
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Shape {
    Hang,
    Block,
    Display,
    RunIn,
}

/// One level's `\ttlf@<level>` (shape and its four arguments, the after code) and
/// `\ttls@<level>` (left, right, before, after, and the indent flag: 0 for `\titlespacing*`).
#[derive(PartialEq, Clone)]
pub(super) struct Level {
    shape: Shape,
    format: Vec<Token>,
    label: Vec<Token>,
    sep: Vec<Token>,
    after_code: Vec<Token>,
    left: Vec<Token>,
    right: Vec<Token>,
    before: Space,
    after: Space,
    indent: bool,
    /// The before-code, font switches put before the title's group (`#4{#8}`).
    before_code: Vec<Token>,
    /// The label is still `\ttl@extract`'s `\@seccntformat{level}`, which `\titlelabel` redefines.
    default_label: bool,
}

/// titlesec's state: the three levels and report's `\chapter` ([`CHAPTER`]),
/// `\beforetitleunit` and `\aftertitleunit` as set at the load, the current heading's left
/// and right indents (`\ttl@makeline`, read by `\titlerule`), and whether
/// `\titleformat{\chapter}` made the chapter heads titlesec's (`\ttl@compatchapter`).
#[derive(PartialEq, Clone)]
pub(super) struct Titlesec {
    levels: [Level; 4],
    units: [Glue; 2],
    makeline: (Vec<Token>, Vec<Token>),
    pub chapter: bool,
    /// `\titlelabel{text}`: `\@seccntformat`'s text, `\thetitle` in it the level's number.
    seccnt: Option<Vec<Token>>,
    /// book's `\@mkboth{\MakeUppercase\contentsname}{…}` in a `\chapter*` title, held by
    /// `\ttl@select` (`\ttl@gmk`) until `\ttl@finmarks`: the name.
    pub held: Option<&'static str>,
}

/// `\ttl@box`, which holds a run-in title until its paragraph starts.
const RUNIN_BOX: u32 = 32714;

/// The index of report's `\chapter` in [`Titlesec::levels`].
const CHAPTER: usize = 3;

/// The options that only change formats and spacing: `compact`, the sizes, the
/// alignments, the fonts, and the defaults titlesec executes itself.
const OPTIONS: [&str; 22] = ["compact", "big", "medium", "small", "tiny", "raggedright", "raggedleft", "center", "rm", "sf", "tt", "md", "bf", "up", "it", "sl", "sc", "rubberchapters", "bottomtitles", "aftersep", "oldparttoc", "innermarks"];

/// Whether every option of `\usepackage[opts]{titlesec}` is one the native job follows.
pub(super) fn options_ok(opts: &str) -> bool {
    opts.split(',').map(str::trim).filter(|o| !o.is_empty()).all(|o| OPTIONS.contains(&o))
}

/// The font switches a format may hold.
const FORMAT_CS: [&str; 20] = ["normalfont", "rmfamily", "sffamily", "ttfamily", "mdseries", "bfseries", "upshape", "itshape", "slshape", "scshape", "tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE", "huge", "Huge"];

/// The paragraph shapes a format, label or before-code may set: the kernel's three
/// declarations and titlesec's `\filright`, `\filleft`, `\filcenter` (`\ttl@fil` and kin).
const SHAPE_CS: [&str; 6] = ["centering", "raggedright", "raggedleft", "filright", "filleft", "filcenter"];

/// The levels titlesec's straight class gives article, by name.
const LEVELS: [&str; 3] = ["section", "subsection", "subsubsection"];

/// Where a piece of code stands in `\titleformat`, for [`code_ok`]: the format (`#1`, in
/// vertical mode before the shape's paragraph), the before-code (`#4`, before `{title}`;
/// vertical only in the display shape), and the after-code (`#5`, vertical, after the title's
/// `\@@par`).
#[derive(Clone, Copy, PartialEq)]
enum Slot {
    Format,
    Before { vertical: bool },
    After,
}

/// The pre-scan's reading of `\titleformat`, `\titlespacing` and `\titlelabel` (`rest` after
/// the name): the length of the whole call when the native job follows every argument, else
/// `None`. `colors`: color.sty or xcolor is loaded (a `\color`). `report`: the class has
/// chapters (`\chapter` a level, `\thechapter` and `\chaptertitlename` in a label).
pub(super) fn call_len(name: &str, rest: &str, colors: bool, report: bool) -> Option<usize> {
    let mut s = rest.trim_start();
    if name == "titlelabel" {
        // `\def\@seccntformat##1{#1}`, with `\thetitle` the level's number.
        label_ok(arg(&mut s)?, colors, report).then_some(())?;
        return Some(rest.len() - s.len());
    }
    let star = s.starts_with('*');
    if star {
        s = s[1..].trim_start();
    }
    let level = arg(&mut s)?;
    let chapter = level.trim() == "\\chapter";
    if !LEVELS.iter().any(|l| level.trim().strip_prefix('\\') == Some(l)) && !(report && chapter) {
        return None;
    }
    if name == "titleformat" {
        if star {
            // `\ttl@format@s` changes a `\ttlf@chapter` that the load does not define.
            (!chapter).then_some(())?;
            code_ok(arg(&mut s)?, Slot::Format, colors).then_some(())?;
        } else {
            let mut shape = "hang";
            if let Some(sh) = opt(&mut s) {
                shape = sh.trim();
                matches!(shape, "hang" | "block" | "display" | "runin").then_some(())?;
            }
            let runin = shape == "runin";
            // A run-in format colours nothing in the box (`\color` in vertical mode).
            code_ok(arg(&mut s)?, Slot::Format, colors && !runin).then_some(())?;
            label_ok(arg(&mut s)?, colors, report).then_some(())?;
            glue_ok(arg(&mut s)?).then_some(())?;
            let before = arg(&mut s)?;
            let after = opt(&mut s);
            if runin {
                runin_code_ok(before, true).then_some(())?;
                after.is_none_or(|a| runin_code_ok(a, false)).then_some(())?;
            } else {
                code_ok(before, Slot::Before { vertical: shape == "display" }, colors).then_some(())?;
                after.is_none_or(|a| code_ok(a, Slot::After, colors)).then_some(())?;
            }
        }
    } else {
        glue_ok(arg(&mut s)?).then_some(())?;
        space_ok(arg(&mut s)?).then_some(())?;
        space_ok(arg(&mut s)?).then_some(())?;
        if let Some(right) = opt(&mut s) {
            glue_ok(right).then_some(())?;
        }
    }
    Some(rest.len() - s.len())
}

/// A braced argument after blanks, moving `s` past it.
fn arg<'a>(s: &mut &'a str) -> Option<&'a str> {
    let r = s.trim_start().strip_prefix('{')?;
    let k = super::balanced(r)?;
    *s = &r[k + 1..];
    Some(&r[..k])
}

/// An optional `[argument]` after blanks, moving `s` past it.
fn opt<'a>(s: &mut &'a str) -> Option<&'a str> {
    let r = s.trim_start().strip_prefix('[')?;
    let a = super::bracketed(r)?;
    *s = &r[a.len() + 1..];
    Some(a)
}

/// Control words of `text` with their one argument (`{…}` or `[…]`), blanks skipped and the
/// contents of a bare `{…}` group read in place (a group only bounds the assignments in it);
/// `None` for any other character.
fn words(text: &str) -> Option<Vec<(&str, Option<&str>)>> {
    let mut out = Vec::new();
    let mut s = text.trim_start();
    while !s.is_empty() {
        if s.starts_with('{') {
            out.extend(words(arg(&mut s)?)?);
            s = s.trim_start();
            continue;
        }
        let r = s.strip_prefix('\\')?;
        let n = r.bytes().take_while(u8::is_ascii_alphabetic).count();
        let name = &r[..n];
        s = &r[n..];
        let mut a = None;
        if s.trim_start().starts_with('{') && name != "MakeUppercase" {
            a = Some(arg(&mut s)?);
        } else if s.trim_start().starts_with('[') {
            a = Some(opt(&mut s)?);
        }
        out.push((name, a));
        s = s.trim_start();
    }
    Some(out)
}

fn color_name_ok(name: Option<&str>) -> bool {
    name.is_some_and(|n| !n.trim().is_empty() && n.trim().bytes().all(|c| c.is_ascii_alphanumeric()))
}

/// A format, before-code or after-code: font switches, paragraph shapes, `\color{name}`,
/// `\titlerule[h]`, `\vspace{glue}` where the code is in vertical mode, and `\MakeUppercase`
/// as the last word of the before-code (it takes `{title}`).
fn code_ok(text: &str, slot: Slot, colors: bool) -> bool {
    let Some(w) = words(text) else { return false };
    let last = w.len().wrapping_sub(1);
    w.iter().enumerate().all(|(k, &(n, a))| match n {
        "color" => colors && color_name_ok(a),
        "titlerule" => slot != Slot::Format && a.is_none_or(glue_ok),
        "vspace" => matches!(slot, Slot::Format | Slot::After | Slot::Before { vertical: true }) && a.is_some_and(glue_ok),
        "MakeUppercase" => matches!(slot, Slot::Before { .. }) && k == last,
        _ => (FORMAT_CS.contains(&n) || SHAPE_CS.contains(&n)) && a.is_none(),
    })
}

/// The before-code or after-code of a run-in title, both inside its box (`\ttlh@runin`'s
/// `\sbox\ttl@box{…#4{#8}#5\unskip}`): font switches, `\MakeUppercase` as the before-code's
/// last word, letters, digits, `.:;,()-`, spaces and braces.
fn runin_code_ok(text: &str, before: bool) -> bool {
    let mut s = text;
    while let Some(c) = s.chars().next() {
        if c == '\\' {
            let n = s[1..].bytes().take_while(u8::is_ascii_alphabetic).count();
            let name = &s[1..1 + n];
            s = &s[1 + n..];
            if !(FORMAT_CS.contains(&name) || before && name == "MakeUppercase" && s.trim().is_empty()) {
                return false;
            }
        } else if c.is_ascii_alphanumeric() || ".:;,()-{} ".contains(c) {
            s = &s[1..];
        } else {
            return false;
        }
    }
    true
}

/// A label: `\thesection` and its kin, `\thetitle`, `\quad`, font switches, paragraph shapes,
/// `\color{name}`, `\MakeUppercase{…}`, letters, digits, `.:()-`, spaces and braces; under
/// report also `\thechapter`, `\chaptertitlename` and a control space.
fn label_ok(text: &str, colors: bool, report: bool) -> bool {
    let mut s = text;
    while let Some(c) = s.chars().next() {
        if c == '\\' {
            if report && s[1..].starts_with(' ') {
                s = &s[2..];
                continue;
            }
            let n = s[1..].bytes().take_while(u8::is_ascii_alphabetic).count();
            let name = &s[1..1 + n];
            s = &s[1 + n..];
            if name == "color" {
                let mut r = s;
                if !(colors && color_name_ok(arg(&mut r))) {
                    return false;
                }
                s = r;
            } else if !(matches!(name, "quad" | "thetitle" | "MakeUppercase") || FORMAT_CS.contains(&name) || SHAPE_CS.contains(&name) || name.strip_prefix("the").is_some_and(|l| LEVELS.contains(&l)) || report && matches!(name, "thechapter" | "chaptertitlename")) {
                return false;
            }
        } else if c.is_ascii_alphanumeric() || ".:()-{} ".contains(c) {
            s = &s[1..];
        } else {
            return false;
        }
    }
    true
}

/// `*n` (a multiple of the unit) or glue.
fn space_ok(text: &str) -> bool {
    match text.trim().strip_prefix('*') {
        Some(n) => !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()),
        None => glue_ok(text),
    }
}

/// `<dimen> [plus <dimen>] [minus <dimen>]`, each a decimal number and a unit (`fil`,
/// `fill`, `filll` after `plus` or `minus`).
fn glue_ok(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    let mut parts = Vec::new();
    let mut rest = t;
    for key in ["plus", "minus"] {
        if let Some(k) = rest.find(key) {
            parts.push(&rest[..k]);
            rest = &rest[k + key.len()..];
        }
    }
    parts.push(rest);
    parts.iter().enumerate().all(|(i, p)| dimen_ok(p, i > 0))
}

/// Glue as `\the` prints it, to be scanned again (`50.0pt plus 1.0fil`).
fn glue_text(g: Glue) -> String {
    let mut s = String::new();
    super::super::engine_value::print_spec(g, "pt", &mut s);
    s
}

fn dimen_ok(text: &str, fil: bool) -> bool {
    let t = text.trim();
    let t = t.strip_prefix('-').unwrap_or(t).trim_start();
    let n = t.bytes().take_while(|c| c.is_ascii_digit() || *c == b'.').count();
    if n == 0 || t[..n].bytes().filter(|&c| c == b'.').count() > 1 {
        return false;
    }
    let unit = t[n..].trim();
    matches!(unit, "pt" | "em" | "ex" | "bp" | "cm" | "mm" | "in" | "pc" | "dd" | "cc" | "sp") || fil && matches!(unit, "fil" | "fill" | "filll")
}

impl Machine<'_, '_> {
    /// `\usepackage[opts]{titlesec}`: `\beforetitleunit` and `\aftertitleunit` in the
    /// font of the load, then `\ttl@extract` of article's three numbered levels, with
    /// `compact`'s `\ttl@space` and the format the font, size and alignment options give
    /// (`\ttl@fil\ttl@fonts\ttl@sizes{n}`, `\bfseries` when no font is named).
    pub(super) fn titlesec_load(&mut self, opts: &str) {
        let report = self.doc.as_ref().is_some_and(|d| d.report);
        if self.doc.as_ref().is_some_and(|d| d.koma || d.memoir) {
            return self.not_native("titlesec with KOMA-Script or memoir");
        }
        if !options_ok(opts) {
            return self.not_native("a titlesec option");
        }
        let opts: Vec<&str> = opts.split(',').map(str::trim).collect();
        let has = |o: &str| opts.contains(&o);
        let units = [self.glue_of("1ex plus .3ex minus .06ex"), self.glue_of("1ex plus .1ex")];
        let fonts: String = ["rm", "sf", "tt", "md", "bf", "up", "it", "sl", "sc"].iter().filter(|o| has(o)).map(|o| format!("\\{}{}", o, if matches!(*o, "rm" | "sf" | "tt") { "family" } else if matches!(*o, "md" | "bf") { "series" } else { "shape" })).collect();
        let sizes: Option<[&str; 3]> = if has("small") {
            Some(["large", "normalsize", "normalsize"])
        } else if has("tiny") {
            Some(["normalsize"; 3])
        } else if has("big") || has("medium") {
            Some(["Large", "large", "normalsize"])
        } else {
            None
        };
        let fil = ["raggedright", "raggedleft", "center"].iter().position(|o| has(o));
        let options = !fonts.is_empty() || sizes.is_some() || fil.is_some();
        let compact = has("compact");
        let levels = std::array::from_fn(|k| {
            let sec = &SECS[k];
            let size = ["Large", "large", "normalsize"][k];
            let format = if options {
                let fil = fil.map_or("", |f| ["\\filright", "\\filleft", "\\filcenter"][f]);
                let fonts = if fonts.is_empty() { "\\bfseries" } else { &fonts };
                format!("{fil}{fonts}\\{}", sizes.map_or(size, |s| s[k]))
            } else {
                format!("\\normalfont\\{size}\\bfseries")
            };
            let before = if compact { units[0] } else { neg(self.glue_of(sec.before)) };
            let after = if compact { units[1] } else { self.glue_of(sec.after) };
            Level { shape: Shape::Hang, format: self.tex_tokens(&format), label: self.tex_tokens(&format!("\\the{}\\quad", LEVELS[k])), sep: self.tex_tokens("0pt"), after_code: Vec::new(), left: self.tex_tokens("0pt"), right: self.tex_tokens("0pt"), before: Space::Fixed(before), after: Space::Fixed(after), indent: false, before_code: Vec::new(), default_label: true }
        });
        // `\titlespacing*\chapter{\z@}{50\p@}{\ttl@chapafter}` (40pt, `rubberchapters`; 30pt
        // under `compact`); the format stays the class's until `\titleformat{\chapter}`, unless
        // a font, size or alignment option set `\if@tempswa`: then `\titleformat\chapter[display]
        // {\ttl@fil\ttl@fonts\ttl@sizes6}{\@chapapp\space\thechapter}{.8\baselineskip}
        // {\ttl@sizes\z@}` (`\ttl@fil` is defined by then, empty or not, so never `\raggedright`).
        let pt = |v: i32| Glue { width: v << 16, ..Glue::default() };
        let mut chapter = Level { shape: Shape::Display, format: Vec::new(), label: Vec::new(), sep: self.tex_tokens("0pt"), after_code: Vec::new(), left: self.tex_tokens("0pt"), right: self.tex_tokens("0pt"), before: Space::Fixed(pt(50)), after: Space::Fixed(pt(if compact { 30 } else { 40 })), indent: false, before_code: Vec::new(), default_label: false };
        let chapter_format = report && options;
        if chapter_format {
            // `\ttl@sizes` 6 and 0: big (the default) `\huge` and `\Huge`, medium `\LARGE` and
            // `\huge`, small `\Large` and `\LARGE`, tiny `\normalsize` and `\large`.
            let (six, zero) = if has("small") {
                ("Large", "LARGE")
            } else if has("tiny") {
                ("normalsize", "large")
            } else if has("medium") {
                ("LARGE", "huge")
            } else {
                ("huge", "Huge")
            };
            let fil = fil.map_or("", |f| ["\\filright", "\\filleft", "\\filcenter"][f]);
            let fonts = if fonts.is_empty() { "\\bfseries" } else { &fonts };
            chapter.format = self.tex_tokens(&format!("{fil}{fonts}\\{six}"));
            chapter.label = self.tex_tokens("\\chaptertitlename\\ \\thechapter");
            chapter.sep = self.tex_tokens(".8\\baselineskip");
            chapter.before_code = self.tex_tokens(&format!("\\{zero}"));
        }
        let [a, b, c]: [Level; 3] = levels;
        if let Some(doc) = self.doc.as_mut() {
            doc.titlesec = Some(Box::new(Titlesec { levels: [a, b, c, chapter], units, makeline: (Vec::new(), Vec::new()), chapter: chapter_format, seccnt: None, held: None }));
        }
    }

    /// Whether the title holds nothing `\ttl@select` redefines or a shape changes:
    /// `\\` and `\newline` (`\ttl@changecentercr`), `\footnote`, `\label`, `\thanks`.
    pub(super) fn ttl_title_ok(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| self.ttl_tokens_ok(&d.title))
    }

    /// [`Machine::ttl_title_ok`] of the given title.
    fn ttl_tokens_ok(&self, title: &[Token]) -> bool {
        let bad: Vec<u32> = ["\\", "newline", "footnote", "label", "thanks", "markboth", "markright"].iter().filter_map(|n| self.e.cs.get(n)).collect();
        !title.iter().any(|t| t.cs_id().is_some_and(|id| bad.contains(&id)))
    }

    /// The level a `{\section}` argument names ([`CHAPTER`] for report's `{\chapter}`).
    fn ttl_level(&mut self) -> Option<usize> {
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let id = name.first().and_then(|t| t.cs_id())?;
        if self.doc.as_ref().is_some_and(|d| d.report) && self.e.cs.get("chapter") == Some(id) {
            return Some(CHAPTER);
        }
        let found = LEVELS.iter().position(|l| self.e.cs.get(l) == Some(id));
        if found.is_none() {
            self.not_native("titlesec: a level other than section, subsection, subsubsection");
        }
        found
    }

    /// `\titleformat{level}[shape]{format}{label}{sep}{before}[after]` (`\ttl@format@ii`)
    /// and `\titleformat*{level}{format}` (`\ttl@format@s`: the format alone).
    pub(super) fn title_format(&mut self) {
        let star = self.next_is('*');
        let Some(k) = self.ttl_level() else { return };
        let mut format = Vec::new();
        if star {
            self.read_arg(&mut format);
            if let Some(l) = self.doc.as_mut().and_then(|d| d.titlesec.as_mut()).map(|t| &mut t.levels[k]) {
                l.format = format;
            }
            return;
        }
        let shape = if self.next_is('[') {
            let s: String = self.bracket_arg().iter().map(|t| t.chr()).collect();
            match s.trim() {
                "hang" => Shape::Hang,
                "block" => Shape::Block,
                "display" => Shape::Display,
                "runin" => Shape::RunIn,
                _ => return self.not_native("a titlesec shape other than hang, block, display, runin"),
            }
        } else {
            Shape::Hang
        };
        let (mut label, mut sep, mut before) = (Vec::new(), Vec::new(), Vec::new());
        self.read_arg(&mut format);
        self.read_arg(&mut label);
        self.read_arg(&mut sep);
        self.read_arg(&mut before);
        let after_code = if self.next_is('[') { self.bracket_arg() } else { Vec::new() };
        if let Some(t) = self.doc.as_mut().and_then(|d| d.titlesec.as_mut()) {
            // `\csname ttl@compat\ttl@a\endcsname`: `\ttl@compatchapter` once, then `\relax`.
            t.chapter |= k == CHAPTER;
            let l = &mut t.levels[k];
            (l.shape, l.format, l.label, l.sep, l.before_code, l.after_code, l.default_label) = (shape, format, label, sep, before, after_code, false);
        }
    }

    /// `\titlelabel{text}`: `\def\@seccntformat##1{text}`, the label of every level that
    /// `\titleformat` has not given one (`\ttl@extract`'s `{\@seccntformat{level}}`).
    pub(super) fn title_label(&mut self) {
        let mut text = Vec::new();
        self.read_arg(&mut text);
        if let Some(t) = self.doc.as_mut().and_then(|d| d.titlesec.as_mut()) {
            t.seccnt = Some(text);
        }
    }

    /// `\titlespacing*{level}{left}{before}{after}[right]` (`\ttl@spacing@ii`): the starred
    /// form's indent flag is 0 (`\@afterindentfalse`).
    pub(super) fn title_spacing(&mut self) {
        let star = self.next_is('*');
        let Some(k) = self.ttl_level() else { return };
        let (mut left, mut before, mut after) = (Vec::new(), Vec::new(), Vec::new());
        self.read_arg(&mut left);
        self.read_arg(&mut before);
        self.read_arg(&mut after);
        let right = if self.next_is('[') { self.bracket_arg() } else { self.tex_tokens("0pt") };
        let space = |t: Vec<Token>| match t.first() {
            Some(s) if *s == Token::char(Cat::Other, '*') => Space::Units(t[1..].iter().fold(0, |n, d| n * 10 + d.chr().to_digit(10).unwrap_or(0) as i32)),
            _ => Space::Text(t),
        };
        let (before, after) = (space(before), space(after));
        if let Some(l) = self.doc.as_mut().and_then(|d| d.titlesec.as_mut()).map(|t| &mut t.levels[k]) {
            (l.left, l.right, l.before, l.after, l.indent) = (left, right, before, after, !star);
        }
    }

    /// `\ttl@assign`: a multiple of the unit (`\dimen@\the<unit>` then `#2\dimen@ plus
    /// #2\ttl@plus minus #2\ttl@minus`), or `\setlength` of the text in the current font.
    fn ttl_space(&mut self, k: usize, after: bool) -> Option<Glue> {
        let t = self.doc.as_ref()?.titlesec.as_ref()?;
        let l = &t.levels[k];
        match if after { &l.after } else { &l.before } {
            Space::Fixed(g) => Some(*g),
            Space::Units(n) => {
                let (u, n) = (t.units[after as usize], *n);
                Some(Glue { width: u.width * n, stretch: u.stretch * n, shrink: u.shrink * n, stretch_order: if n == 0 { 0 } else { u.stretch_order }, shrink_order: if n == 0 { 0 } else { u.shrink_order } })
            }
            Space::Text(text) => {
                let text = text.clone();
                self.eval_glue(&text)
            }
        }
    }

    /// `\setlength\@tempskipa{text}`: the text then `\relax`, scanned as glue.
    fn eval_glue(&mut self, text: &[Token]) -> Option<Glue> {
        let relax = self.named_cs(named!("relax"));
        let mut t = text.to_vec();
        t.push(relax);
        self.back_list(&t);
        let (g, _) = self.scan_glue_spec(false);
        if self.get_token() != Some(relax) {
            self.not_native("titlesec: a length that is not glue");
            return None;
        }
        Some(g)
    }

    /// `\section` and its kin after `\par` (the level, title and flags are in `doc`):
    /// `\ttl@labelling`, then `\ttl@straight@ii`'s skips.
    pub(super) fn ttl_head(&mut self) {
        let Some(doc) = self.doc.as_ref() else { return };
        let level = doc.level;
        // report's levels are one below `\chapter`.
        let k = level - doc.report as usize;
        if doc.numbered {
            self.heading_step(level);
            if self.hyper() {
                let counters = self.doc.as_ref().map(|d| d.counters).unwrap_or_default();
                self.heading_anchor(level, &counters);
            }
        }
        let (Some(mut before), Some(mut after)) = (self.ttl_space(k, false), self.ttl_space(k, true)) else { return };
        // parskip.sty's `\AtBeginDocument` patches of `\ttl@straight@ii`: `\advance\@tempskipa
        // -\parskip \addvspace\@tempskipa` and `\advance\@tempskipb-\parskip \vspace\@tempskipb`
        // (the first `\vspace{\@tempskipb}`, after the title; the run-in `\hspace` keeps it).
        if self.parskip() {
            let parskip = self.glue_param(super::super::engine_pack::PAR_SKIP);
            before = glue_add(before, neg(parskip));
            if self.doc.as_ref().and_then(|d| d.titlesec.as_ref()).is_none_or(|t| t.levels[k].shape != Shape::RunIn) {
                after = glue_add(after, neg(parskip));
            }
        }
        let Some(doc) = self.doc.as_mut() else { return };
        // `\@afterindenttrue \ifcase#5 \@afterindentfalse\fi`.
        doc.afterindent = doc.titlesec.as_ref().is_some_and(|t| t.levels[k].indent);
        doc.after = after;
        // `\if@nobreak \ttl@titlespace{\@tempskipa}` (`aftersep`: `\@gobble`).
        if doc.nobreak {
            return self.ttl_build();
        }
        doc.after_before = before;
        let mut ops = Vec::new();
        self.add_penalty(SEC_PENALTY, &mut ops);
        ops.push(Op::Call(|m| {
            let before = m.doc.as_ref().map(|d| d.after_before).unwrap_or_default();
            m.add_vspace(before);
            m.ttl_build();
        }));
        self.run_ops(ops);
    }

    /// `\ttl@select`'s group and the shape, as tokens, then `\rt@ttlend`.
    fn ttl_build(&mut self) {
        let t = self.ttl_shape(None);
        self.back_list(&t);
    }

    /// `\ttl@select`'s group and the shape, as tokens: for the heading in `doc` (its title
    /// taken) and `\rt@ttlend`, or for report's chapter head (`Some(title)`) without it.
    fn ttl_shape(&mut self, chapter: Option<Vec<Token>>) -> Vec<Token> {
        let Some(doc) = self.doc.as_mut() else { return Vec::new() };
        let report = doc.report as usize;
        let level = if chapter.is_some() { CHAPTER } else { doc.level - report };
        let numbered = doc.numbered;
        let title = match chapter.as_ref() {
            Some(t) => t.clone(),
            None => {
                let title = std::mem::take(&mut doc.title);
                if let Some(f) = doc.fancy.as_mut() {
                    f.title.clone_from(&title);
                }
                // book's two-sided `\sectionmark` (`ttl_end`).
                if doc.book {
                    doc.mark_title.clone_from(&title);
                }
                title
            }
        };
        let counters = doc.counters;
        let appendix = doc.appendix;
        let Some(ts) = doc.titlesec.as_mut() else { return Vec::new() };
        let l = &ts.levels[level];
        let (shape, format, mut label, sep, before_code, after_code, left, right) = (l.shape, l.format.clone(), l.label.clone(), l.sep.clone(), l.before_code.clone(), l.after_code.clone(), l.left.clone(), l.right.clone());
        if l.default_label {
            if let Some(text) = ts.seccnt.as_ref() {
                label.clone_from(text);
            }
        }
        // `\gdef\ttl@makeline`.
        ts.makeline = (left.clone(), right.clone());
        let open = Token::char(Cat::BeginGroup, '{');
        let close = Token::char(Cat::EndGroup, '}');
        let thes: Vec<Option<u32>> = LEVELS.iter().map(|l| self.e.cs.get(&format!("the{l}"))).collect();
        let quad = self.e.cs.get("quad");
        let (thechapter, chapname) = (self.e.cs.get("thechapter"), self.e.cs.get("chaptertitlename"));
        // `\thetitle` is `\gdef`ed by `\ttl@straight@i` only (the level's `\the<level>`); a
        // chapter head would print the last section's.
        if let Some(thetitle) = self.e.cs.get("thetitle") {
            if label.iter().any(|t| t.cs_id() == Some(thetitle)) {
                if level == CHAPTER {
                    self.not_native("titlesec: \\thetitle in a chapter label");
                    return Vec::new();
                }
                for t in label.iter_mut().filter(|t| t.cs_id() == Some(thetitle)) {
                    *t = thes[level].map_or(*t, Token::cs);
                }
            }
        }
        // titlesec's own commands as the natives the load and `\titleformat` stand for.
        let own: Vec<(u32, Token)> = [("titlerule", doc!("rt@titlerule")), ("filright", doc!("rt@filright")), ("filleft", doc!("rt@filleft")), ("filcenter", doc!("rt@filcenter"))].into_iter().filter_map(|(n, d)| Some((self.e.cs.get(n)?, self.doc_id(d)))).collect();
        let native = |code: Vec<Token>| -> Vec<Token> { code.into_iter().map(|t| own.iter().find(|o| t.cs_id() == Some(o.0)).map_or(t, |o| o.1)).collect() };
        let (format, label, before_code, after_code) = (native(format), native(label), native(before_code), native(after_code));
        let digits = |text: String| text.chars().map(|c| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c)).collect::<Vec<_>>();
        let mut lab = Vec::new();
        for t in &label {
            match thes.iter().position(|&id| id.is_some() && t.cs_id() == id) {
                Some(k) => lab.extend(digits(heading_number(&counters, k + report, appendix))),
                None if quad.is_some() && t.cs_id() == quad => lab.extend(self.tex_tokens("\\hskip1em\\relax ")),
                // report's `\thechapter`; `\chaptertitlename` is `\@chapapp`, `\appendixname`
                // after `\appendix`.
                None if report == 1 && thechapter.is_some() && t.cs_id() == thechapter => lab.extend(digits(heading_number(&counters, 0, appendix))),
                None if report == 1 && chapname.is_some() && t.cs_id() == chapname => lab.extend(self.tex_tokens(self.head_name(if appendix { super::babel::HEAD_APPENDIX } else { super::babel::HEAD_CHAPTER }))),
                None => lab.push(*t),
            }
        }
        let mut t = vec![self.named_cs(named!("begingroup")), self.doc_id(doc!("rt@ttlsetup"))];
        if shape == Shape::RunIn {
            // `\ttlh@runin`: `\global\@noskipsectrue`, then `#1{\global\sbox\ttl@box{\ttl@calc
            // \hspace{#6}\ifttl@label{\ttl@strut#2}\ttl@calc\hspace{#3}\fi #4{#8}#5\unskip}}`
            // (`\@svsechd` is `\unhbox\ttl@box`, `ttl_run_in`).
            if let Some(doc) = self.doc.as_mut() {
                doc.noskipsec = true;
            }
            t.extend(format);
            t.extend(self.tex_tokens(&format!("{{\\global\\setbox{RUNIN_BOX}=\\hbox{{")));
            // `\sbox`'s `\color@setgroup` (`\begingroup\set@color`) and `\color@endgroup`.
            let colored = self.colored();
            if colored {
                t.extend([self.named_cs(named!("begingroup")), self.doc_id(doc!("rt@setcolor"))]);
            }
            t.extend(self.tex_tokens("{\\hskip "));
            t.extend(left);
            t.extend(self.tex_tokens("\\relax}"));
            if numbered {
                t.extend(self.tex_tokens("{\\strut "));
                t.extend(lab);
                t.extend(self.tex_tokens("}{\\hskip "));
                t.extend(sep);
                t.extend(self.tex_tokens("\\relax}"));
            }
            t.extend(before_code.iter().copied());
            t.push(open);
            t.extend(title.iter().copied());
            t.push(close);
            t.extend(after_code);
            t.extend(self.tex_tokens("\\unskip"));
            if colored {
                t.push(self.named_cs(named!("endgroup")));
            }
            t.extend(self.tex_tokens("}}\\endgroup"));
            t.push(self.doc_id(doc!("rt@ttlend")));
            return t;
        }
        let assign = |m: &mut Self, t: &mut Vec<Token>, what: &str, v: &[Token]| {
            t.extend(m.tex_tokens(what));
            t.extend_from_slice(v);
            t.push(m.named_cs(named!("relax")));
        };
        assign(self, &mut t, "\\leftskip=", &left);
        assign(self, &mut t, "\\rightskip=", &right);
        t.extend(self.tex_tokens("\\interlinepenalty10000 "));
        t.extend(format);
        let colored = self.colored();
        match shape {
            Shape::Hang => {
                t.push(open);
                t.extend(self.tex_tokens("\\parindent=0pt\\begingroup "));
                if numbered {
                    t.extend(self.tex_tokens("\\noindent\\setbox0=\\hbox{\\begingroup "));
                    if colored {
                        t.push(self.doc_id(doc!("rt@setcolor")));
                    }
                    t.extend(lab);
                    t.extend(self.tex_tokens("\\strut{\\hskip "));
                    t.extend(sep);
                    t.extend(self.tex_tokens("\\relax}\\endgroup}\\hangindent=\\wd0 \\box0 "));
                }
                t.extend(before_code.iter().copied());
                t.push(open);
                t.extend(title.iter().copied());
                t.push(close);
                t.extend(self.tex_tokens("\\kern0pt\\strut\\par\\endgroup\\penalty10000 "));
                t.extend(after_code);
                t.extend(self.tex_tokens("\\par}"));
            }
            Shape::Block => {
                t.extend(self.tex_tokens("\\parindent=0pt\\indent "));
                if numbered {
                    t.push(open);
                    t.extend(lab);
                    t.push(close);
                    // `\setlength\@tempskipa{#3}\ifdim\@tempskipa=\z@\else\ttl@calc\hspace{#3}\fi`.
                    if self.eval_glue(&sep).is_none_or(|g| g.width != 0) {
                        t.extend(self.tex_tokens("{\\hskip "));
                        t.extend(sep);
                        t.extend(self.tex_tokens("\\relax}"));
                    }
                }
                t.extend(before_code.iter().copied());
                t.push(open);
                t.extend(title.iter().copied());
                t.push(close);
                t.extend(self.tex_tokens("\\kern0pt\\strut\\par\\penalty10000 "));
                t.extend(after_code);
                t.extend(self.tex_tokens("\\par "));
            }
            Shape::Display => {
                t.extend(self.tex_tokens("\\parindent=0pt "));
                if numbered {
                    t.push(open);
                    t.extend(lab);
                    t.extend(self.tex_tokens("\\strut\\par}\\penalty10000 {\\vskip "));
                    t.extend(sep);
                    t.extend(self.tex_tokens("\\relax\\vskip0pt}"));
                }
                t.extend(before_code.iter().copied());
                t.push(open);
                t.extend(title.iter().copied());
                t.push(close);
                t.extend(self.tex_tokens("\\kern0pt\\strut\\par\\penalty10000 "));
                t.extend(after_code);
                t.extend(self.tex_tokens("\\par "));
            }
            // Made above, before the paragraph shapes' assignments.
            Shape::RunIn => {}
        }
        t.push(self.named_cs(named!("endgroup")));
        if chapter.is_none() {
            t.push(self.doc_id(doc!("rt@ttlend")));
        }
        t
    }

    /// report's `\@makechapterhead{title}` (`numbered`) or `\@makeschapterhead` after
    /// `\titleformat{\chapter}`: `\ttl@mkchap` (`\ttl@mkchap@i`), `\vspace*{before}`, the
    /// indent flag, then `\ttl@topmode{after}{\ttl@select…}` (`rubberchapters`: the group,
    /// then `\ttl@calc\vspace{after}`), then `\ttl@finmarks` (the `\@mkboth` held from the
    /// title, `\tableofcontents`'s); report's `\@afterheading` follows.
    pub(super) fn ttl_chapter_head(&mut self, title: Vec<Token>) {
        if !self.ttl_tokens_ok(&title) {
            return self.not_native("titlesec: a chapter title with \\\\, \\footnote or \\label");
        }
        let (Some(before), Some(after)) = (self.ttl_space(CHAPTER, false), self.ttl_space(CHAPTER, true)) else { return };
        let Some(doc) = self.doc.as_mut() else { return };
        doc.afterindent = doc.titlesec.as_ref().is_some_and(|t| t.levels[CHAPTER].indent);
        let mut t = vec![self.doc_id(doc!("vspace")), Token::char(Cat::Other, '*'), Token::char(Cat::BeginGroup, '{')];
        t.extend(self.tex_tokens(&glue_text(before)));
        t.push(Token::char(Cat::EndGroup, '}'));
        t.extend(self.ttl_shape(Some(title)));
        // `\ttl@calc\vspace{after}`: `{\setlength\@tempskipa{after}\vspace\@tempskipa}`.
        t.extend(self.tex_tokens(&format!("{{\\vskip {}\\relax\\vskip0pt}}", glue_text(after))));
        t.push(self.doc_id(doc!("rt@ttlchapend")));
        self.back_list(&t);
    }

    /// The end of [`Machine::ttl_chapter_head`]: `\ttl@finmarks`, then report's `\@afterheading`.
    pub(super) fn ttl_chapter_end(&mut self) {
        self.fancy_finmarks();
        if let Some(name) = self.doc.as_mut().and_then(|d| d.titlesec.as_mut()).and_then(|t| t.held.take()) {
            self.mkboth_upper(name);
        }
        let t = vec![self.doc_id(doc!("rt@afterheading"))];
        self.back_list(&t);
    }

    /// `\ttl@select`'s `\everypar{\setbox\z@\lastbox\ttl@strut}` (outside an item's label).
    pub(super) fn ttl_setup(&mut self) {
        if self.doc.as_ref().is_some_and(|d| d.inlabel) {
            return self.not_native("titlesec: a heading right after \\item");
        }
        self.set_everypar(super::TTL_STRUT);
    }

    /// `\ttl@finmarks` (unstarred: `\sectionmark`, `\addcontentsline` and its `\nobreak`),
    /// then `\par\nobreak\vspace{\@tempskipb}\@afterheading\ignorespaces`.
    pub(super) fn ttl_end(&mut self) {
        let mut ops = Vec::new();
        if self.doc.as_ref().is_some_and(|d| d.toc) {
            self.fancy_marks();
            // `\ttl@write`'s `\sectionmark` under book's own `headings` (`section_mark`).
            if let Some(doc) = self.doc.as_mut().filter(|d| d.book && d.level == 1 && !d.fancy.as_ref().is_some_and(|f| f.defs)) {
                let title = std::mem::take(&mut doc.mark_title);
                let number = heading_number(&doc.counters, 1, doc.appendix);
                self.section_mark(&number, &title);
            }
            if self.doc.as_ref().is_some_and(|d| d.unsupported.is_some()) {
                return;
            }
            if self.hyper() {
                self.aux_write(0);
            }
            self.contents_write();
            ops.push(Op::Penalty(10000));
        }
        self.fancy_finmarks();
        let Some(after) = self.doc.as_ref().map(|d| d.after) else { return };
        // `\if@noskipsec`: `\global\@nobreakfalse` and the run-in `\everypar` (`ttl_run_in`).
        if self.doc.as_ref().is_some_and(|d| d.noskipsec) {
            ops.push(Op::Call(|m| {
                if let Some(doc) = m.doc.as_mut() {
                    doc.nobreak = false;
                }
                m.set_everypar(super::TTL_RUN_IN);
                if let Some(x) = m.next_non_blank() {
                    m.input.back(x.t);
                }
            }));
            return self.run_ops(ops);
        }
        ops.extend([self.cur_par(), Op::Penalty(10000), Op::Vskip(after), Op::Vskip(Glue::default())]);
        ops.push(Op::Call(|m| {
            if let Some(doc) = m.doc.as_mut() {
                doc.nobreak = true;
            }
            m.set_everypar(super::AFTER_HEADING);
            if let Some(x) = m.next_non_blank() {
                m.input.back(x.t);
            }
        }));
        self.run_ops(ops);
    }

    /// `\titlerule[h]` (`\ttl@rule@i`): in vertical mode `\titleline` (`\ttl@line`):
    /// `\vskip\topskip \hrule\@height\z@ \nobreak \vskip-\topskip`, then `\@makebox[\hsize][s]`
    /// of `\ttl@makeline{{\leaders\hrule height h\hfill\kern\z@}}` (the heading's left and
    /// right as `\hspace`s), `\par`, `\hrule height\z@ \nobreak`; in a paragraph the
    /// leaders alone.
    pub(super) fn title_rule(&mut self) {
        let h = if self.next_is('[') { self.bracket_arg() } else { self.tex_tokens(".4pt") };
        let mut leaders = self.tex_tokens("{\\leaders\\hrule height");
        leaders.extend(h);
        leaders.extend(self.tex_tokens("\\hfill\\kern0pt}"));
        if self.mode != Mode::Vertical {
            return self.back_list(&leaders);
        }
        let (left, right) = self.doc.as_ref().and_then(|d| d.titlesec.as_ref()).map(|t| t.makeline.clone()).unwrap_or_default();
        let mut t = self.tex_tokens("\\vskip\\topskip\\hrule height0pt\\penalty10000 \\vskip-\\topskip\\hbox to\\hsize{\\hskip ");
        t.extend(left);
        t.extend(self.tex_tokens("\\relax "));
        t.extend(leaders);
        t.extend(self.tex_tokens("\\hskip "));
        t.extend(right);
        t.extend(self.tex_tokens("\\relax}\\par\\hrule height0pt\\penalty10000 "));
        self.back_list(&t);
    }

    /// The run-in title's `\everypar` (`\ttl@straight@ii`): `\if@noskipsec \global
    /// \@noskipsecfalse \clubpenalty\@M \hskip-\parindent \begingroup \@svsechd\unskip
    /// {\hspace{\@tempskipb}}\endgroup \else \clubpenalty\@clubpenalty \everypar{}\fi`.
    pub(super) fn ttl_run_in(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        if !doc.noskipsec {
            let v = self.rw(super::R_CLUBPENALTY);
            self.set_int_named(param!("clubpenalty"), v);
            return self.set_everypar(0);
        }
        doc.noskipsec = false;
        let after = doc.after;
        self.set_int_named(param!("clubpenalty"), 10000);
        let t = self.tex_tokens(&format!("\\hskip-\\parindent\\begingroup\\unhbox{RUNIN_BOX}\\unskip{{\\hskip {}\\relax}}\\endgroup ", glue_text(after)));
        self.back_list(&t);
    }

    /// `\filright`, `\filleft`, `\filcenter` (`which` 0, 1, 2): `\advance\rightskip\z@
    /// \@plus 1fil`; `\advance\leftskip\z@\@plus 1fil \parfillskip\z@`; both.
    pub(super) fn ttl_fil(&mut self, which: u8) {
        let fil = Glue { stretch: 65536, stretch_order: 1, ..Glue::default() };
        if which != 1 {
            let r = param!("rightskip");
            let g = glue_add(self.e.eqtb.glue(r), fil);
            self.e.eqtb.set_glue(r, g, false);
        }
        if which != 0 {
            let r = param!("leftskip");
            let g = glue_add(self.e.eqtb.glue(r), fil);
            self.e.eqtb.set_glue(r, g, false);
            self.e.eqtb.set_glue(param!("parfillskip"), Glue::default(), false);
        }
    }
}
