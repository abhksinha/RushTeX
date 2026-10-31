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
//! Run-in shapes (article's `\paragraph`), `\part`, the report class, the other options
//! and anything in a format argument beyond font switches and `\color` go to the engine
//! (the pre-scan, [`call_len`] and [`options_ok`]).

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
}

/// titlesec's state: the three levels, `\beforetitleunit` and `\aftertitleunit` as set at
/// the load, and the current heading's left and right indents (`\ttl@makeline`, read by
/// `\titlerule`).
#[derive(PartialEq, Clone)]
pub(super) struct Titlesec {
    levels: [Level; 3],
    units: [Glue; 2],
    makeline: (Vec<Token>, Vec<Token>),
}

/// The options that only change formats and spacing: `compact`, the sizes, the
/// alignments, the fonts, and the defaults titlesec executes itself.
const OPTIONS: [&str; 22] = ["compact", "big", "medium", "small", "tiny", "raggedright", "raggedleft", "center", "rm", "sf", "tt", "md", "bf", "up", "it", "sl", "sc", "rubberchapters", "bottomtitles", "aftersep", "oldparttoc", "innermarks"];

/// Whether every option of `\usepackage[opts]{titlesec}` is one the native job follows.
pub(super) fn options_ok(opts: &str) -> bool {
    opts.split(',').map(str::trim).filter(|o| !o.is_empty()).all(|o| OPTIONS.contains(&o))
}

/// The font switches a format may hold, and `\color{name}`.
const FORMAT_CS: [&str; 20] = ["normalfont", "rmfamily", "sffamily", "ttfamily", "mdseries", "bfseries", "upshape", "itshape", "slshape", "scshape", "tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE", "huge", "Huge"];

/// The levels titlesec's straight class gives article, by name.
const LEVELS: [&str; 3] = ["section", "subsection", "subsubsection"];

/// The pre-scan's reading of `\titleformat` and `\titlespacing` (`rest` after the name):
/// the length of the whole call when the native job follows every argument, else `None`.
/// `colors`: color.sty or xcolor is loaded (a format's `\color`).
pub(super) fn call_len(name: &str, rest: &str, colors: bool) -> Option<usize> {
    let mut s = rest.trim_start();
    let star = s.starts_with('*');
    if star {
        s = s[1..].trim_start();
    }
    let level = arg(&mut s)?;
    if !LEVELS.iter().any(|l| level.trim().strip_prefix('\\') == Some(l)) {
        return None;
    }
    if name == "titleformat" {
        if star {
            format_ok(arg(&mut s)?, colors).then_some(())?;
        } else {
            if let Some(shape) = opt(&mut s) {
                matches!(shape.trim(), "hang" | "block" | "display").then_some(())?;
            }
            format_ok(arg(&mut s)?, colors).then_some(())?;
            label_ok(arg(&mut s)?).then_some(())?;
            glue_ok(arg(&mut s)?).then_some(())?;
            arg(&mut s)?.trim().is_empty().then_some(())?;
            if let Some(after) = opt(&mut s) {
                after_ok(after, colors).then_some(())?;
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

/// Control words and `{…}` groups of `text`, blanks skipped; `None` for anything else.
fn words(text: &str) -> Option<Vec<(&str, Option<&str>)>> {
    let mut out = Vec::new();
    let mut s = text.trim_start();
    while !s.is_empty() {
        let r = s.strip_prefix('\\')?;
        let n = r.bytes().take_while(u8::is_ascii_alphabetic).count();
        let name = &r[..n];
        s = &r[n..];
        let mut a = None;
        if s.trim_start().starts_with('{') {
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

fn format_ok(text: &str, colors: bool) -> bool {
    words(text).is_some_and(|w| w.iter().all(|&(n, a)| if n == "color" { colors && color_name_ok(a) } else { FORMAT_CS.contains(&n) && a.is_none() }))
}

/// A label: `\thesection` and its kin, `\quad`, letters, digits, `.:()-` and spaces.
fn label_ok(text: &str) -> bool {
    let mut s = text;
    while let Some(c) = s.chars().next() {
        if c == '\\' {
            let n = s[1..].bytes().take_while(u8::is_ascii_alphabetic).count();
            let name = &s[1..1 + n];
            if !(name == "quad" || name.strip_prefix("the").is_some_and(|l| LEVELS.contains(&l))) {
                return false;
            }
            s = &s[1 + n..];
        } else if c.is_ascii_alphanumeric() || ".:()- ".contains(c) {
            s = &s[1..];
        } else {
            return false;
        }
    }
    true
}

/// `\color{name}`, `\titlerule` and `\titlerule[dimen]`.
fn after_ok(text: &str, colors: bool) -> bool {
    words(text).is_some_and(|w| w.iter().all(|&(n, a)| match n {
        "color" => colors && color_name_ok(a),
        "titlerule" => a.is_none_or(glue_ok),
        _ => false,
    }))
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
        if self.doc.as_ref().is_some_and(|d| d.report || d.packages.contains(&"parskip")) {
            return self.not_native("titlesec with the report class or parskip");
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
                let fil = fil.map_or("", |f| ["\\rt@filright", "\\rt@filleft", "\\rt@filcenter"][f]);
                let fonts = if fonts.is_empty() { "\\bfseries" } else { &fonts };
                format!("{fil}{fonts}\\{}", sizes.map_or(size, |s| s[k]))
            } else {
                format!("\\normalfont\\{size}\\bfseries")
            };
            let before = if compact { units[0] } else { neg(self.glue_of(sec.before)) };
            let after = if compact { units[1] } else { self.glue_of(sec.after) };
            Level { shape: Shape::Hang, format: self.tex_tokens(&format), label: self.tex_tokens(&format!("\\the{}\\quad", LEVELS[k])), sep: self.tex_tokens("0pt"), after_code: Vec::new(), left: self.tex_tokens("0pt"), right: self.tex_tokens("0pt"), before: Space::Fixed(before), after: Space::Fixed(after), indent: false }
        });
        if let Some(doc) = self.doc.as_mut() {
            doc.titlesec = Some(Box::new(Titlesec { levels, units, makeline: (Vec::new(), Vec::new()) }));
        }
    }

    /// Whether the title holds nothing `\ttl@select` redefines or a shape changes:
    /// `\\` and `\newline` (`\ttl@changecentercr`), `\footnote`, `\label`, `\thanks`.
    pub(super) fn ttl_title_ok(&self) -> bool {
        let Some(doc) = self.doc.as_ref() else { return false };
        let bad: Vec<u32> = ["\\", "newline", "footnote", "label", "thanks", "markboth", "markright"].iter().filter_map(|n| self.e.cs.get(n)).collect();
        !doc.title.iter().any(|t| t.cs_id().is_some_and(|id| bad.contains(&id)))
    }

    /// The level a `{\section}` argument names.
    fn ttl_level(&mut self) -> Option<usize> {
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let id = name.first().and_then(|t| t.cs_id())?;
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
                _ => return self.not_native("a titlesec shape other than hang, block, display"),
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
        if !before.is_empty() {
            return self.not_native("a titlesec before-code");
        }
        if let Some(l) = self.doc.as_mut().and_then(|d| d.titlesec.as_mut()).map(|t| &mut t.levels[k]) {
            (l.shape, l.format, l.label, l.sep, l.after_code) = (shape, format, label, sep, after_code);
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
        if doc.numbered {
            self.heading_step(level);
            if self.hyper() {
                let counters = self.doc.as_ref().map(|d| d.counters).unwrap_or_default();
                self.heading_anchor(level, &counters);
            }
        }
        let (Some(before), Some(after)) = (self.ttl_space(level, false), self.ttl_space(level, true)) else { return };
        let Some(doc) = self.doc.as_mut() else { return };
        // `\@afterindenttrue \ifcase#5 \@afterindentfalse\fi`.
        doc.afterindent = doc.titlesec.as_ref().is_some_and(|t| t.levels[level].indent);
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
        let Some(doc) = self.doc.as_mut() else { return };
        let level = doc.level;
        let numbered = doc.numbered;
        let title = std::mem::take(&mut doc.title);
        if let Some(f) = doc.fancy.as_mut() {
            f.title.clone_from(&title);
        }
        let counters = doc.counters;
        let appendix = doc.appendix;
        let Some(ts) = doc.titlesec.as_mut() else { return };
        let l = &ts.levels[level];
        let (shape, format, label, sep, after_code, left, right) = (l.shape, l.format.clone(), l.label.clone(), l.sep.clone(), l.after_code.clone(), l.left.clone(), l.right.clone());
        // `\gdef\ttl@makeline`.
        ts.makeline = (left.clone(), right.clone());
        let open = Token::char(Cat::BeginGroup, '{');
        let close = Token::char(Cat::EndGroup, '}');
        let thes: Vec<Option<u32>> = LEVELS.iter().map(|l| self.e.cs.get(&format!("the{l}"))).collect();
        let quad = self.e.cs.get("quad");
        let mut lab = Vec::new();
        for t in &label {
            match thes.iter().position(|&id| id.is_some() && t.cs_id() == id) {
                Some(k) => lab.extend(heading_number(&counters, k, appendix).chars().map(|c| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c))),
                None if quad.is_some() && t.cs_id() == quad => lab.extend(self.tex_tokens("\\hskip1em\\relax ")),
                None => lab.push(*t),
            }
        }
        let rule = self.e.cs.get("titlerule");
        let after_code: Vec<Token> = after_code.iter().map(|&t| if rule.is_some() && t.cs_id() == rule { self.doc_id(doc!("rt@titlerule")) } else { t }).collect();
        let mut t = vec![self.named_cs(named!("begingroup")), self.doc_id(doc!("rt@ttlsetup"))];
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
                t.push(open);
                t.extend(title);
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
                t.push(open);
                t.extend(title);
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
                t.push(open);
                t.extend(title);
                t.push(close);
                t.extend(self.tex_tokens("\\kern0pt\\strut\\par\\penalty10000 "));
                t.extend(after_code);
                t.extend(self.tex_tokens("\\par "));
            }
        }
        t.push(self.named_cs(named!("endgroup")));
        t.push(self.doc_id(doc!("rt@ttlend")));
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
