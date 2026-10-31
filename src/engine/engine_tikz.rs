//! TikZ's `remember picture,overlay` page frames on the GDL path (`2026-10-31-tikz-design.md`,
//! stage 1): `\begin{tikzpicture}[remember picture,overlay]` with `\draw` and `\fill` of one
//! `rectangle` between two `current page` anchors, each with optional `xshift`/`yshift`, and
//! path options `line width=<dim>` and colours. Anything else in a picture goes to the engine.
//!
//! The node list is pgf's, as pdfTeX gets it (pgfcorescopes.code.tex `\pgf@picture`,
//! `\endpgfpicture`; pgfsys.code.tex `\pgfsys@typesetpicturebox`):
//!
//! `\hbox{\glue(-minx) \hbox to0pt{q, 0 G, 0 g, 0.3985 w, \hbox to0pt{q, paths.., glue, Q,
//! \hbox to0pt{\pdfsavepos \write}, \hss}, n, Q, \hss}}`
//!
//! Each path is `q`, its options' literals in written order (tikz's `\tikz@addoption`; a
//! colour is LaTeX's `\color`, a colour-stack push), the path, `S` or `f`, a `\glue 0.0`, the
//! colour pops (`\aftergroup\reset@color`), `Q`; a path without options has no `q` and `Q`. A path's coordinates depend on where the
//! picture lands on the page (`\pgf@shape@interpictureshift` against the position pgf saved
//! with `\pdfsavepos` on the previous run); the native job is one pass, so they are
//! [`what::PGF_POINT`] whatsits, written in the ship walk from the literal's own position,
//! which is the saved position: every box and glue between them is of size zero.
//!
//! **The `\glue 0.0` items** are space tokens typeset in `\nullfont` (`\pgf@picture` sets it)
//! and some of them are taken away again by `\unskip`; each one's source:
//! - pgfsys.code.tex:601 `\def\pgfsys@end@idscope{` has no `%`: every `\endpgfidscope` (the
//!   end of each path, tikz.code.tex:2418, and of the picture's scope, `\endtikz@scope@env`)
//!   typesets one space, before the path's colour pops and its `Q`;
//! - the source's own space after the picture's `[options]` (spaces after a path's `;` are
//!   eaten by tikz's parser);
//! - pgfmoduleshapes.code.tex:1050 and :1066, the rectangle shape's `north west` and
//!   `south east` anchors, open with `{` and a line end (`north east`, `south west` do not);
//! - tikz.code.tex:3268, the `rectangle` operation, likewise;
//! - `xshift`/`yshift` (pgfcoretransformations.code.tex:368, `\pgfpoint{#1}{+0pt}`): the `+0pt`
//!   takes `\pgfmathsetlength`'s quick path (pgfmathcalc.code.tex:36), `\pgf@y +0pt\unskip`,
//!   which removes the last item when it is glue.
//!
//! So the spaces of the source and of a path's coordinates are counted, the shifts take
//! them back, and those left before the path's first literal stay in the list.

use super::super::engine_eqtb::{Glue, Reg, Region};
use super::super::engine_node::{Kind, Node};
use super::super::engine_run::{Machine, Mode, SEMI_SIMPLE_GROUP};
use super::super::engine_token::Token;
use super::super::engine_catcode::Cat;
use super::super::engine_whatsit as what;

/// A `current page` anchor (pgfmoduleshapes.code.tex:1137): the page's rectangle from the
/// origin to (`\paperwidth`, `\paperheight`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Anchor {
    NorthWest,
    NorthEast,
    SouthWest,
    SouthEast,
}

/// `([xshift=..,yshift=..]current page.<anchor>)`: the anchor, the sum of its shifts in sp,
/// and how many shift keys it has (each an `\unskip`).
#[derive(Clone, Copy, Debug)]
struct Coord {
    anchor: Anchor,
    shift: [i32; 2],
    shifts: u32,
}

/// A path option, applied in written order at `\tikz@options`.
#[derive(Debug)]
enum PathOpt {
    /// `line width=<dim>`, in sp.
    Width(i32),
    /// A colour (tikz's `.unknown` handler, `\tikz@compat@color@set`): the spec as written.
    Color(String),
}

/// `\draw[opts] (from) rectangle (to);` or `\fill`: one record read whole.
#[derive(Debug)]
struct PathCmd {
    fill: bool,
    opts: Vec<PathOpt>,
    from: Coord,
    to: Coord,
}

/// A picture of the grammar above: whether a space follows its `[options]`, and its paths.
#[derive(Debug)]
pub(super) struct Picture {
    space: bool,
    paths: Vec<PathCmd>,
}

/// The source with comments removed as TeX reads them: `%`, the rest of its line, the line
/// end and the next line's leading blanks. `None` for `\%` (not of the grammar anyway).
fn strip_comments(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '%' => {
                for d in it.by_ref() {
                    if d == '\n' {
                        break;
                    }
                }
                while it.peek().is_some_and(|d| *d == ' ' || *d == '\t') {
                    it.next();
                }
            }
            '\\' if it.peek() == Some(&'%') => return None,
            _ => out.push(c),
        }
    }
    Some(out)
}

/// A cursor over the comment-free text.
struct Cursor<'a> {
    s: &'a str,
}

impl<'a> Cursor<'a> {
    /// Skips blanks; `Some(true)` when there were any, `None` at a blank line (`\par`).
    fn blanks(&mut self) -> Option<bool> {
        let n = self.s.len() - self.s.trim_start_matches([' ', '\t', '\n', '\r']).len();
        let run = &self.s[..n];
        self.s = &self.s[n..];
        (run.matches('\n').count() < 2).then_some(n > 0)
    }

    fn eat(&mut self, lit: &str) -> bool {
        match self.s.strip_prefix(lit) {
            Some(r) => {
                self.s = r;
                true
            }
            None => false,
        }
    }

    /// The text up to the `]` closing a key list (no nested brackets or braces).
    fn bracketed(&mut self) -> Option<&'a str> {
        let k = self.s.find(']')?;
        let inner = &self.s[..k];
        if inner.contains(['[', '{', '}', '\\']) {
            return None;
        }
        self.s = &self.s[k + 1..];
        Some(inner)
    }
}

/// A pgfmath length as tikz's keys write it (`1.5pt`, `-1.2cm`, `.4pt`): pgfmath reads the
/// digits and the unit with TeX (`\pgfmath@dimen=\pgfmath@number cm`, pgfmathparser.code.tex)
/// and prints the result, which reads back as the same length; a leading `-` is its negation.
/// A leading `+` would take `\pgfmathsetlength`'s quick path (an `\unskip`): not taken.
fn length(v: &str) -> Option<i32> {
    let v = v.trim();
    let body = v.strip_prefix('-').unwrap_or(v);
    let digits = body.len() - body.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.').len();
    let (num, unit) = body.split_at(digits);
    if num.is_empty() || num == "." || num.matches('.').count() > 1 || !matches!(unit, "pt" | "cm" | "mm" | "in" | "bp") {
        return None;
    }
    let sp = crate::backend::backend_scaled::scan_dimen(v, 0, 0)?;
    // Far below `\maxdimen`, so no sum of the picture overflows.
    (sp.abs() < 1 << 28).then_some(sp as i32)
}

/// `([shifts]current page.<anchor>)`, the `(` already read.
fn coord(c: &mut Cursor) -> Option<Coord> {
    let mut shift = [0i32; 2];
    let mut shifts = 0;
    if c.eat("[") {
        for item in c.bracketed()?.split(',') {
            let (k, v) = item.split_once('=')?;
            let i = match k.trim() {
                "xshift" => 0,
                "yshift" => 1,
                _ => return None,
            };
            shift[i] += length(v)?;
            shifts += 1;
        }
    }
    if !c.eat("current page.") {
        return None;
    }
    let anchor = [("north west", Anchor::NorthWest), ("north east", Anchor::NorthEast), ("south west", Anchor::SouthWest), ("south east", Anchor::SouthEast)].into_iter().find(|(n, _)| c.eat(n))?.1;
    c.eat(")").then_some(Coord { anchor, shift, shifts })
}

/// Reads a picture's text after `\begin{tikzpicture}` up to (not with) `\end{tikzpicture}`;
/// `None` when it is not of the grammar this module draws.
pub(super) fn parse(text: &str) -> Option<Picture> {
    let clean = strip_comments(text)?;
    let mut c = Cursor { s: &clean };
    // `\tikzpicture`'s `[#1]`, right after the name: exactly `remember picture` and `overlay`.
    if !c.eat("[") {
        return None;
    }
    let mut keys: Vec<&str> = c.bracketed()?.split(',').map(str::trim).collect();
    keys.sort_unstable();
    if keys != ["overlay", "remember picture"] {
        return None;
    }
    let space = c.blanks()?;
    let mut paths = Vec::new();
    loop {
        c.blanks()?;
        if c.s.is_empty() {
            break;
        }
        let fill = if c.eat("\\draw") {
            false
        } else if c.eat("\\fill") {
            true
        } else {
            return None;
        };
        if c.s.starts_with(|ch: char| ch.is_ascii_alphabetic()) {
            return None;
        }
        c.blanks()?;
        let mut opts = Vec::new();
        if c.eat("[") {
            for item in c.bracketed()?.split(',') {
                let item = item.trim();
                if let Some((k, v)) = item.split_once('=') {
                    if k.trim() != "line width" {
                        return None;
                    }
                    opts.push(PathOpt::Width(length(v)?));
                } else if !item.is_empty() && item.starts_with(|ch: char| ch.is_ascii_alphabetic()) && item.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '!' || ch == '.') {
                    opts.push(PathOpt::Color(item.to_string()));
                } else {
                    return None;
                }
            }
            c.blanks()?;
        }
        if !c.eat("(") {
            return None;
        }
        let from = coord(&mut c)?;
        c.blanks()?;
        if !c.eat("rectangle") {
            return None;
        }
        c.blanks()?;
        if !c.eat("(") {
            return None;
        }
        let to = coord(&mut c)?;
        c.blanks()?;
        if !c.eat(";") {
            return None;
        }
        paths.push(PathCmd { fill, opts, from, to });
    }
    (!paths.is_empty()).then_some(Picture { space, paths })
}

/// 1 inside the group a picture opened to start its paragraph ([`Machine::tikz_picture`]);
/// grouping restores it.
const TIKZ_GROUP: Reg = Reg::register(Region::Int, 32714);

/// `\hoffset` and `\voffset`.
fn offsets(m: &Machine) -> [i32; 2] {
    [param!("hoffset"), param!("voffset")].map(|r| m.e.eqtb.word(r))
}

impl Machine<'_, '_> {
    /// `\begin{tikzpicture}` of [`parse`]'s grammar: pgf's node list, then `\leavevmode`
    /// (`\pgfsys@typesetpicturebox`) and the box. In vertical mode the paragraph starts first
    /// (`\everypar` runs before the box goes in) and the environment is read again. That
    /// `\leavevmode` is inside the picture's groups (`\begin`, `\tikzpicture`, `\pgfpicture`),
    /// so what `\everypar` sets locally (`\@afterheading`'s `\clubpenalty\@M`, `\everypar{}`)
    /// is undone at `\end{tikzpicture}`: the paragraph starts in a group closed after the box.
    pub(super) fn tikz_picture(&mut self) {
        match self.mode {
            Mode::Math => return self.not_native("a tikzpicture in math"),
            Mode::Vertical => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                self.e.eqtb.set_word(TIKZ_GROUP, 1, false);
                let mut t = vec![self.doc_id(doc!("begin")), Token::char(Cat::BeginGroup, '{')];
                t.extend("tikzpicture".chars().map(|c| Token::char(Cat::Letter, c)));
                t.push(Token::char(Cat::EndGroup, '}'));
                self.back_list(&t);
                self.new_graf(true);
                return;
            }
            Mode::Horizontal => {}
        }
        let elc = self.e.eqtb.word(Reg { region: Region::Int, slot: super::super::engine_names::END_LINE_CHAR });
        let Some(text) = self.input.raw_until("\\end{tikzpicture}", elc) else {
            return self.not_native("a tikzpicture without its end in the same file");
        };
        let Some(pic) = parse(&text) else {
            return self.not_native("a tikzpicture the native job does not draw");
        };
        // `\pgfsetcolor{.}` writes the current colour as pgf's `0 G` and `0 g` for black only.
        if self.e.eqtb.word(super::CURRENT_COLOR) != 0 {
            return self.not_native("a tikzpicture in a colour");
        }
        // Each colour's push literal, by xcolor as `\color` makes it.
        let mut colors: Vec<Vec<String>> = Vec::with_capacity(pic.paths.len());
        for p in &pic.paths {
            let mut lits = Vec::new();
            for o in &p.opts {
                if let PathOpt::Color(spec) = o {
                    let Some(l) = self.color_literal(None, spec, false) else {
                        return self.not_native("a colour the native job does not know");
                    };
                    lits.push(l);
                }
            }
            colors.push(lits);
        }
        // `\pgfsys@thepagewidth`, `\pgfsys@thepageheight` (pgfsys-pdftex.def): `\paperwidth`
        // and `\paperheight`, which pdftex.def also gives the PDF page.
        let pw = self.e.eqtb.word(Reg { region: Region::Dimen, slot: super::PDF_PAGE_WIDTH });
        let ph = self.e.eqtb.word(Reg { region: Region::Dimen, slot: super::PDF_PAGE_WIDTH + 1 });
        if pw <= 0 || ph <= 0 {
            return self.not_native("a tikzpicture without the paper size");
        }
        // `pgfpageorigin` is (`\hoffset`, `\voffset`): an anchor's point on the page.
        let [ho, vo] = offsets(self);
        let point = |c: &Coord| {
            let (x, y) = match c.anchor {
                Anchor::NorthWest => (0, ph),
                Anchor::NorthEast => (pw, ph),
                Anchor::SouthWest => (0, 0),
                Anchor::SouthEast => (pw, 0),
            };
            (x + ho + c.shift[0], y + vo + c.shift[1])
        };
        let anchor_space = |c: &Coord| matches!(c.anchor, Anchor::NorthWest | Anchor::SouthEast) as u32;
        let fil = Glue { stretch: 65536, stretch_order: 1, shrink: 65536, shrink_order: 1, ..Glue::default() };
        let outer = self.cur.len();
        // `\hskip-\pgf@picminx`: the overlay picture's bounding box is empty, all zero.
        let g = self.e.nodes.glue_node(Glue::default(), 0);
        self.cur.push(g);
        let pic_start = self.cur.len();
        // `\pgfsys@beginscope`, `\pgfsetcolor{.}`, `\pgfsetlinewidth{0.4pt}`.
        for l in ["q ", "0 G ", "0 g ", "0.3985 w "] {
            self.pdf_literal(l);
        }
        let layer = self.cur.len();
        // tikz's `\scope[every picture,#1]`: `\pgfscope`.
        self.pdf_literal("q ");
        let mut pending = pic.space as u32;
        for (p, lits) in pic.paths.iter().zip(&colors) {
            pending += anchor_space(&p.from);
            pending = pending.saturating_sub(p.from.shifts);
            pending += 1;
            pending += anchor_space(&p.to);
            pending = pending.saturating_sub(p.to.shifts);
            self.space_glue(&mut pending);
            // tikz.code.tex:2279: the scope only when the path has options.
            let scoped = !p.opts.is_empty();
            if scoped {
                self.pdf_literal("q ");
            }
            let mut k = 0;
            for o in &p.opts {
                match o {
                    PathOpt::Width(w) => self.pdf_literal(&format!("{} w ", what::pgf_bp(*w))),
                    PathOpt::Color(_) => {
                        self.color_stack(Some(&lits[k]));
                        k += 1;
                    }
                }
            }
            // tikz's moveto, then `\tikz@rectB`: moveto, three linetos, closepath, moveto.
            let ((x1, y1), (x2, y2)) = (point(&p.from), point(&p.to));
            for (op, x, y) in [(b'm', x1, y1), (b'm', x1, y1), (b'l', x1, y2), (b'l', x2, y2), (b'l', x2, y1)] {
                self.pgf_point(op, x, y);
            }
            self.pdf_literal("h ");
            self.pgf_point(b'm', x2, y2);
            self.pdf_literal(if p.fill { "f " } else { "S " });
            // The `\endpgfidscope` space stays pending without a scope: a later shift may
            // take it.
            pending += 1;
            if scoped {
                self.space_glue(&mut pending);
                for _ in 0..k {
                    self.color_stack(None);
                }
                self.pdf_literal("Q ");
            }
        }
        // `\endscope`: its `\endpgfidscope`'s space, then `\endpgfscope`.
        pending += 1;
        self.space_glue(&mut pending);
        self.pdf_literal("Q ");
        // `\hbox to0pt{\pgfsys@markposition{\pgfpictureid}}`: `\pdfsavepos` and the `.aux`
        // `\write` of `\pgfsyspdfmark`, kept for its place only.
        let mark = self.cur.len();
        self.save_pos();
        self.cur.push(Node { kind: Kind::Whatsit, sub: what::WRITE, font: what::NULL_STREAM, a: 0, b: 0 });
        self.pack_box(mark, true);
        let g = self.e.nodes.glue_node(fil, 0);
        self.cur.push(g);
        self.pack_box(layer, true);
        // `\pgfsys@discardpath`, `\pgfsys@endscope`, then `\hss`.
        self.pdf_literal("n ");
        self.pdf_literal("Q ");
        let g = self.e.nodes.glue_node(fil, 0);
        self.cur.push(g);
        self.pack_box(pic_start, true);
        self.pack_box(outer, false);
        self.aux = 1000;
        if self.e.eqtb.word(TIKZ_GROUP) == 1 {
            self.end_group(SEMI_SIMPLE_GROUP);
        }
    }

    /// The `pending` spaces typeset in `\nullfont`, each a `\glue 0.0`.
    fn space_glue(&mut self, pending: &mut u32) {
        for _ in 0..std::mem::take(pending) {
            let g = self.e.nodes.glue_node(Glue::default(), 0);
            self.cur.push(g);
        }
    }

    /// The list from `start` as an hbox, `to0pt` (`exact`) or at its natural width.
    fn pack_box(&mut self, start: usize, exact: bool) {
        let b = self.hpack(start, 0, exact);
        let b = self.e.nodes.add_box(b);
        self.cur.push(Node::new(Kind::Box, b as i32));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_overlay_frames() {
        let p = parse("[remember picture,overlay]\n  \\draw[line width=1.5pt,gold]\n    ([xshift=1cm,yshift=-1cm]current page.north west)\n    rectangle ([xshift=-1cm,yshift=1cm]current page.south east);\n").expect("a frame");
        assert!(p.space);
        assert_eq!(p.paths.len(), 1);
        assert_eq!(p.paths[0].from.shift, [1864679, -1864679]);
        assert_eq!(p.paths[0].to.shifts, 2);
        assert!(matches!(p.paths[0].opts[0], PathOpt::Width(98304)));
        let p = parse("[remember picture,overlay]\\fill[reportblue!15] ([yshift=-42mm]current page.north west) rectangle\n ([yshift=-47mm]current page.north east);").expect("a band");
        assert!(!p.space && p.paths[0].fill);
        // Not of the grammar: another option, a node, a blank line, a `+` length.
        assert!(parse("[overlay]\\draw (current page.north west) rectangle (current page.south east);").is_none());
        assert!(parse("[remember picture,overlay]\\node at (0,0) {x};").is_none());
        assert!(parse("[remember picture,overlay]\n\n\\draw (current page.north west) rectangle (current page.south east);").is_none());
        assert!(parse("[remember picture,overlay]\\draw[line width=+1pt] (current page.north west) rectangle (current page.south east);").is_none());
        assert!(parse("[remember picture,overlay]%\n\\draw[thick] (current page.north west) rectangle (current page.south east);").is_some_and(|p| !p.space));
    }

    #[test]
    fn prints_pgf_bp() {
        // `\pgf@sys@bp`: 0.4pt, 0.8pt, 1.5pt, 1cm, and zero.
        assert_eq!(what::pgf_bp(26214), "0.3985");
        assert_eq!(what::pgf_bp(52429), "0.79701");
        assert_eq!(what::pgf_bp(98304), "1.49442");
        assert_eq!(what::pgf_bp(1864679), "28.3468");
        assert_eq!(what::pgf_bp(0), "0.0");
        assert_eq!(what::pgf_bp(-4843471 + 1864679), "-45.28352");
    }
}
