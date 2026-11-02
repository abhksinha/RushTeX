//! TikZ pictures on the GDL path (`2026-10-31-tikz-design.md`, stages 1 and 2): `\draw`,
//! `\fill`, `\filldraw` and `\path[draw|fill]` of straight paths (`--`, `-|`, `|-`,
//! `-- cycle`), `rectangle` (also with `rounded corners`) and `circle`, between points
//! `(x,y)` or, in a `remember picture,overlay` picture, `current page` corners; path options
//! `line width`, the `thin`/`thick` family, colours, `draw=`/`fill=` colours and the dash
//! styles. Anything else in a picture goes to the engine.
//!
//! The node list is pgf's, as pdfTeX gets it (pgfcorescopes.code.tex `\pgf@picture`,
//! `\endpgfpicture`; pgfsys.code.tex `\pgfsys@typesetpicturebox`):
//!
//! `\hbox(h)xw{\glue(-minx) \hbox to0pt shifted miny{q, 0 G, 0 g, 0.3985 w, \hbox to0pt{q,
//! paths.., glue, Q, [\hbox to0pt{\pdfsavepos \write}], \hss}, n, Q, \hss}}`
//!
//! with (minx, miny) and (w, h) the picture's bounding box (`\pgf@protocolsizes` of every
//! path point, a stroked path's widened by half its line width, pgfcorepathusage.code.tex
//! `\pgfusepath`); an `overlay` picture's is empty, all zero.
//!
//! Each path is `q`, its options' literals in written order (tikz's `\tikz@addoption`; a
//! colour is LaTeX's `\color`, a colour-stack push; `draw=`/`fill=` are pgf's `RG`/`rg`
//! literals), the path, `S`, `f` or `B`, a `\glue 0.0`, the colour pops
//! (`\aftergroup\reset@color`), `Q`; a path without options has no `q` and `Q`
//! (`rounded corners` is no option: `\pgfsetcornersarced` runs at once). A `current page`
//! point depends on where the picture lands on the page (`\pgf@shape@interpictureshift`
//! against the position pgf saved with `\pdfsavepos` on the previous run); the native job is
//! one pass, so it is a [`what::PGF_POINT`] whatsit, written in the ship walk from the
//! literal's own position, which is the saved position: every box and glue between them is
//! of size zero.
//!
//! **The `\glue 0.0` items** are space tokens typeset in `\nullfont` (`\pgf@picture` sets it)
//! and some of them are taken away again by `\unskip`; each one's source:
//! - pgfsys.code.tex:601 `\def\pgfsys@end@idscope{` has no `%`: every `\endpgfidscope` (the
//!   end of each path, tikz.code.tex:2418, and of the picture's scope, `\endtikz@scope@env`)
//!   typesets one space, before the path's colour pops and its `Q`;
//! - the source's own space after the picture's `[options]` (spaces after a path's `;` are
//!   eaten by tikz's parser, and without options `\pgfutil@ifnextchar[` skips them);
//! - pgfmoduleshapes.code.tex:1050 and :1066, the rectangle shape's `north west` and
//!   `south east` anchors, open with `{` and a line end (`north east`, `south west` do not);
//! - tikz.code.tex:3268, the `rectangle` operation, likewise; each `--` one more
//!   (`\tikz@lineto`); `-|`, `|-`, `cycle` and `circle` none;
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
use crate::backend::backend_scaled::xn_over_d;
use pgfmath::fmul;

#[path = "engine_pgfmath.rs"]
mod pgfmath;

#[path = "engine_tikz_node.rs"]
mod node;

/// A `current page` anchor (pgfmoduleshapes.code.tex:1137): the page's rectangle from the
/// origin to (`\paperwidth`, `\paperheight`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Anchor {
    NorthWest,
    NorthEast,
    SouthWest,
    SouthEast,
    North,
    South,
    West,
    East,
    Center,
}

impl Anchor {
    /// The anchor's point on the page rectangle (0, 0) .. (`pw`, `ph`), as the rectangle
    /// shape computes it: a middle is `.5` of each side, TeX's truncating factor.
    fn point(self, pw: i32, ph: i32) -> (i32, i32) {
        let half = |v: i32| xn_over_d(v, 32768, 65536);
        match self {
            Anchor::NorthWest => (0, ph),
            Anchor::NorthEast => (pw, ph),
            Anchor::SouthWest => (0, 0),
            Anchor::SouthEast => (pw, 0),
            Anchor::North => (half(pw), ph),
            Anchor::South => (half(pw), 0),
            Anchor::West => (0, half(ph)),
            Anchor::East => (pw, half(ph)),
            Anchor::Center => (half(pw), half(ph)),
        }
    }
}

/// The spaces a point typesets: the rectangle shape's anchor code that opens with `{` and a
/// line end (pgfmoduleshapes.code.tex:1012-1068; not `east`, `north east`, `south west`).
fn anchor_space(q: &Pt) -> u32 {
    matches!(q, Pt::Page(c) if matches!(c.anchor, Anchor::NorthWest | Anchor::SouthEast | Anchor::North | Anchor::South | Anchor::West | Anchor::Center)) as u32
}

/// The `\unskip`s of a point's `xshift`/`yshift` keys.
fn shifts(q: &Pt) -> u32 {
    if let Pt::Page(c) = q {
        c.shifts
    } else {
        0
    }
}

/// `([xshift=..,yshift=..]current page.<anchor>)`: the anchor, the sum of its shifts in sp,
/// and how many shift keys it has (each an `\unskip`).
#[derive(Clone, Copy, Debug)]
struct Coord {
    anchor: Anchor,
    shift: [i32; 2],
    shifts: u32,
}

/// A point of a path.
#[derive(Clone, Copy, Debug)]
enum Pt {
    /// A `current page` corner: placed where the page ships.
    Page(Coord),
    /// `(x,y)` in the picture's own coordinates, in sp.
    At(i32, i32),
}

/// A path operation after the path's first point.
#[derive(Debug)]
enum Seg {
    /// `-- (p)`.
    Line(Pt),
    /// `-| (p)`: horizontal, then vertical.
    Hv(Pt),
    /// `|- (p)`: vertical, then horizontal.
    Vh(Pt),
    /// `rectangle (p)`.
    Rect(Pt),
    /// `circle (r)`, `ellipse (a and b)`, `circle[radius=..]`: the radius along each axis in sp
    /// (a number times each unit vector), and the spaces its form leaves.
    Circle(i32, i32, u32),
    /// `-- cycle`.
    Cycle,
    /// `grid (p)`: the corner, and the steps along each axis in sp (`step`, `xstep`, `ystep`).
    Grid(Pt, [i32; 2]),
    /// A point alone: a moveto.
    Move(Pt),
    /// `.. controls (a) and (b) .. (c)`, or a piece of a smooth plot: the two controls and
    /// the end (picture points, before the transformation), and the spaces it typesets.
    Curve([(i32, i32); 3], u32),
    /// `arc (s:e:r)`, `arc (s:e:rx and ry)` or `arc[..]`: the angles in sp, the radii (lengths in
    /// sp, or numbers of the unit vectors when `unitless`), and tikz's last point after it.
    /// `axes` are the unitless radii times the unit vectors (`\pgfpointxy{r}{0}`, `{0}{r}`).
    /// `delta`: an angle computed from `delta angle` (`\pgfmathsetmacro`, its line's space).
    Arc { s: i32, e: i32, r: [i32; 2], unitless: bool, axes: [i32; 2], to: (i32, i32), delta: bool },
}

/// One entry of a dash pattern: a length, or `\pgflinewidth` as the options leave it.
#[derive(Clone, Copy, Debug)]
enum Dash {
    Len(i32),
    LineWidth,
}

/// A path option, applied in written order at `\tikz@options`.
#[derive(Debug)]
enum PathOpt {
    /// `line width=<dim>` or a `thin`/`thick` style, in sp.
    Width(i32),
    /// A colour (tikz's `.unknown` handler, `\tikz@compat@color@set`): the spec as written.
    Color(String),
    /// `draw=<colour>` (`\pgfsetstrokecolor`).
    Stroke(String),
    /// `fill=<colour>` (`\pgfsetfillcolor`).
    Fill(String),
    /// A dash style (`dash pattern=on .. off ..`, phase 0pt).
    Dash(&'static [Dash]),
    /// `draw opacity=` (`true`, `\pgfsetstrokeopacity`) or `fill opacity=`: the value as
    /// `\pgfmathparse` leaves it.
    Opacity(bool, String),
}

/// `\draw[opts] <path>;` and its kin: one record read whole.
#[derive(Debug)]
struct PathCmd {
    fill: bool,
    draw: bool,
    opts: Vec<PathOpt>,
    /// `rounded corners=<r>`, in sp; 0 without.
    round: i32,
    start: Pt,
    /// Each operation, and whether its point becomes tikz's last point (`+(..)` does not).
    segs: Vec<(Seg, bool)>,
    /// The picture's transformation and the path's own.
    tf: Tf,
    /// How many `\unskip`s the path's options make (`xshift`, `yshift`).
    unskips: u32,
    /// `<-`, `->`: a `to` tip at the start, at the end.
    arrows: [bool; 2],
    /// Whether the path's own options set the tips (else the picture's).
    own_arrows: bool,
    /// Its tips' kind: `to`, `stealth` (pgfcorearrows.code.tex:1118), `latex` (:1198) or
    /// arrows.meta's `Latex`.
    tip: Tip,
    /// Its nodes moved by `above=<length>` and kin, each by the number of segments before
    /// it: each takes back a pending space there (`\tikz@possibly@transform`'s shift).
    nudges: Vec<usize>,
    /// Spaces its points typeset (node anchors, node borders), before its operators.
    spaces: u32,
    /// `\pgflinewidth` before its options: the picture's.
    base_width: i32,
    /// Its nodes (`-- node {..}`, `(p) node {..}`), placed: drawn after it.
    nodes: Vec<node::NodeCmd>,
    /// `\tikz@grid@x`, `\tikz@grid@y` as a grid would read them: 1cm, or the path's `step`.
    step: [i32; 2],
    /// Its `edge` operations' paths, drawn after it (tikz.code.tex:3040 `\tikz@do@edge`).
    edges: Vec<PathCmd>,
    /// An edge's path: whether it is the first and the last of its path's.
    edge: Option<(bool, bool)>,
    /// A path of one node's name only (edges from it): no moveto (the border waits for a
    /// segment that never comes), its paint operator alone.
    bare: bool,
    /// `shorten <`, `shorten >` (`\pgf@shorten@start@additional`, `..end..`): each end drawn
    /// back by that much more, a tip with it.
    shorten: [i32; 2],
}

/// What a picture holds, in order.
#[derive(Debug)]
enum Item {
    Path(PathCmd),
    /// `\coordinate (name) at (p);`: its name is resolved while reading.
    Coordinate,
    /// `\node`: its text box, shape and place (`engine_tikz_node`).
    Node(node::NodeCmd),
    /// A space token reaching the picture's list (`\glue 0.0` in `\nullfont`): one at the start
    /// of a `\foreach` body in braces, one after a `\foreach`.
    Space,
    /// `\begin{scope}[options]`: `\pgfscope`, its options' line widths, and whether a space
    /// follows the options (one without them is skipped looking for `[`).
    ScopeBegin(Vec<i32>, bool),
    /// `\end{scope}`: its `\endpgfidscope` space, `\endpgfscope`, and whether a space follows.
    ScopeEnd(bool),
    /// A tree's child (tikz.code.tex:4639 `\tikz@childnode`): `\hbox{\pgfscope`, then the
    /// spaces before its `\node` ([`node::child_forms`] and one more).
    ChildBegin(u32),
    /// The child's end: its edge path's `\endpgfidscope` space, `\endpgfscope`, the box.
    ChildEnd,
    /// An edge path's `\tikz@figbox` with the grandchildren, after its paint and before its
    /// `\endpgfidscope` space.
    FigOpen,
    FigClose,
}

/// A picture of the grammar above: whether it is `remember picture,overlay`, whether a space
/// follows its `[options]`, and its paths.
#[derive(Debug)]
pub(super) struct Picture {
    overlay: bool,
    /// `remember picture` (with `overlay`): its position marked for `current page`; an
    /// `overlay` picture alone has no mark and no page points.
    remember: bool,
    space: bool,
    /// Each line width the picture's options set (`thick`, `line width=`), in order: set in
    /// its scope; the last is each path's.
    widths: Vec<i32>,
    items: Vec<Item>,
    /// `baseline=(node.anchor)`: the anchor's y (`\pgf@shift@baseline`) and the spaces its
    /// code typesets at the picture's end (`\endpgfpicture`'s `\pgf@process{\pgf@baseline}`).
    baseline: Option<(i32, u32)>,
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

    /// The text up to the `]` closing a key list (no nested brackets).
    fn bracketed(&mut self) -> Option<&'a str> {
        let k = self.s.find(']')?;
        let inner = &self.s[..k];
        // Braces only balanced and one deep (`shift={(1,2)}`); the keys' readers check them.
        // Commands only within braces (a style's `font=\small`), which the reader of the
        // style's keys checks again.
        let mut depth = 0i32;
        for ch in inner.chars() {
            depth += match ch {
                '{' => 1,
                '}' => -1,
                _ => 0,
            };
            if !(0..=1).contains(&depth) || ch == '\\' && depth == 0 {
                return None;
            }
        }
        if depth != 0 || inner.contains('[') {
            return None;
        }
        self.s = &self.s[k + 1..];
        Some(inner)
    }

    /// The text up to the `)` closing a coordinate (no nesting).
    fn parenthesized(&mut self) -> Option<&'a str> {
        let k = self.s.find(')')?;
        let inner = &self.s[..k];
        if inner.contains(['(', '{', '}', '$']) {
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

/// `line width=<value>` (tikz.code.tex `\tikz@semiaddlinewidth`, `\pgfmathsetlength`): a
/// length, or a plain number, which pgfmath reads as that many points (`line width=1.5`).
pub(super) fn width_value(v: &str) -> Option<i32> {
    length(v).or_else(|| number(v).filter(|w| *w >= 0))
}

/// tikz's `x=1cm`, `y=1cm`: `\pgf@xx`, `\pgf@yy` of 1cm.
const UNIT: i32 = 1864679;

/// A number as pgfmath leaves it (`\pgfmathparse`, `\pgfmathsetlength{0.3}`): a length in pt,
/// in sp; its printed form reads back as the same.
fn number(v: &str) -> Option<i32> {
    let v = v.trim();
    let body = v.strip_prefix('-').unwrap_or(v);
    if body.is_empty() || body == "." || !body.bytes().all(|b| b.is_ascii_digit() || b == b'.') || body.matches('.').count() > 1 {
        return None;
    }
    let sp = crate::backend::backend_scaled::scan_dimen(body, 0, 0)?;
    if sp >= 1 << 24 {
        return None;
    }
    Some(if v.starts_with('-') { -(sp as i32) } else { sp as i32 })
}

/// A number without a unit times a unit vector, as `\pgfpointxy` computes it:
/// `\pgf@x=<number>\pgf@xx`.
fn times_xx(v: &str, unit: i32) -> Option<i32> {
    Some(fmul(number(v)?, unit))
}

/// pgf's transformation matrix: `\pgf@pt@aa`, `\pgf@pt@ab`, `\pgf@pt@ba`, `\pgf@pt@bb` (entries in sp
/// as `\pgf@sys@tonumber` prints them) and its shift (`\pgf@pt@x`, `\pgf@pt@y`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Tf {
    aa: i32,
    ab: i32,
    ba: i32,
    bb: i32,
    e: i32,
    f: i32,
}

impl Tf {
    const ID: Tf = Tf { aa: 1 << 16, ab: 0, ba: 0, bb: 1 << 16, e: 0, f: 0 };

    /// The linear part of `\pgf@pos@transform`: `x = aa·x + ba·y`, `y = bb·y + ab·x`.
    fn linear(&self, x: i32, y: i32) -> (i32, i32) {
        (fmul(self.aa, x) + fmul(self.ba, y), fmul(self.bb, y) + fmul(self.ab, x))
    }

    /// `\pgf@pos@transform`: the linear part, then the shift.
    fn apply(&self, x: i32, y: i32) -> (i32, i32) {
        let (x, y) = self.linear(x, y);
        (x + self.e, y + self.f)
    }

    /// `\pgf@transformcm` (pgfcoretransformations.code.tex:236): the given matrix (entries as
    /// `\pgfmathsetlength` reads them) times the current one, each product a factor of a
    /// length; the shift is the given point transformed.
    fn cm(self, m: [i32; 4], x: i32, y: i32) -> Tf {
        let (aa, ab) = (fmul(self.aa, m[0]) + fmul(self.ba, m[1]), fmul(self.ab, m[0]) + fmul(self.bb, m[1]));
        let (ba, bb) = (fmul(self.aa, m[2]) + fmul(self.ba, m[3]), fmul(self.ab, m[2]) + fmul(self.bb, m[3]));
        let (e, f) = self.apply(x, y);
        Tf { aa, ab, ba, bb, e, f }
    }

    /// `\pgftransformshift{(x,y)}`: `\pgf@transformcm{1}{0}{0}{1}`.
    fn shift(self, x: i32, y: i32) -> Tf {
        self.cm([1 << 16, 0, 0, 1 << 16], x, y)
    }

    /// `\pgftransformscale{s}`.
    fn scale(self, s: i32) -> Tf {
        self.cm([s, 0, 0, s], 0, 0)
    }

    /// `\pgftransformxscale{s}`, `\pgftransformyscale{s}` (pgfcoretransformations.code.tex:385):
    /// `\pgftransformcm{s}{0}{0}{1.0}` and `{1.0}{0}{0}{s}`.
    fn scale_axis(self, s: i32, y: bool) -> Tf {
        if y { self.cm([1 << 16, 0, 0, s], 0, 0) } else { self.cm([s, 0, 0, 1 << 16], 0, 0) }
    }

    /// `\pgftransformrotate{a}` (:412): pgfmath's cosine and sine, the sine negated as a length.
    fn rotate(self, a: i32) -> Tf {
        let (c, s) = (pgfmath::cos(a), pgfmath::sin(a));
        self.cm([c, s, -s, c], 0, 0)
    }

    /// `\pgftransforminvert` (pgfcoretransformations.code.tex:156): the shift negated; `aa`,
    /// `bb` by `\pgfmathreciprocal@` when `ab`, `ba` are zero, else each entry times the
    /// reciprocal of the determinant (`aa·bb − ab·ba` as factors of lengths); then the shift
    /// through the new matrix. Refused for a reciprocal of a value not above 0.01 (the
    /// port of `\pgfmathreciprocal@` reads positive numbers only).
    fn invert(self) -> Option<Tf> {
        if self == Tf::ID {
            return Some(Tf::ID);
        }
        let recip = |v: i32| (v > 655).then(|| pgfmath::reciprocal(v));
        let (aa, ab, ba, bb) = if !self.turns() {
            (recip(self.aa)?, 0, 0, recip(self.bb)?)
        } else {
            let r = recip(fmul(self.bb, self.aa) - fmul(self.ab, self.ba))?;
            (fmul(self.bb, r), fmul(self.ab, -r), fmul(self.ba, -r), fmul(self.aa, r))
        };
        let t = Tf { aa, ab, ba, bb, e: 0, f: 0 };
        let (e, f) = t.linear(-self.e, -self.f);
        Some(Tf { e, f, ..t })
    }

    /// Whether the matrix turns or slants (`ab`, `ba` not zero).
    fn turns(&self) -> bool {
        self.ab != 0 || self.ba != 0
    }
}

/// What a picture's points may name: LaTeX's length registers (by a lookup the caller
/// gives: the native job reads them, the pre-scan only asks whether they exist) and the
/// picture's `\coordinate`s so far.
struct Ctx<'d> {
    dims: &'d dyn Fn(&str) -> Option<i32>,
    names: Vec<(String, (i32, i32))>,
    /// `\pgf@xx`, `\pgf@yy`: tikz's `x=1cm`, `y=1cm` (`\pgfsetxvec{\pgfpoint{1cm}{0pt}}`), or the
    /// picture's `x=`, `y=`.
    unit: [i32; 2],
    /// The picture's own transformation (`scale=`).
    tf: Tf,
    /// Node `k`'s text box (width, height, depth), once typeset; zeros before.
    boxes: &'d dyn Fn(usize) -> [i32; 3],
    /// The quad of the picture's font (a node's `.3333em` inner sep).
    em: i32,
    /// The named nodes so far, and how many nodes.
    nodes: Vec<node::Shape>,
    node_count: usize,
    /// `node distance` (positioning: `1cm and 1cm`): along x, along y.
    distance: [i32; 2],
    /// `\pgflinewidth` in the picture (its `thick`, `line width=`), 0.4pt without.
    width: i32,
    /// The picture's `->`, `<-`, `<->`: each path's tips unless it names its own.
    arrows: [bool; 2],
    /// The picture's `>=` (`\pgfarrowsdeclarealias`): the tip `>` and `<` stand for.
    gt: Tip,
    /// The picture's `<name>/.style={<keys>}`: a name a node's or path's options may use.
    styles: Vec<(String, String)>,
    /// Spaces the points read so far typeset (a node anchor's code), taken by the reader.
    spaces: std::cell::Cell<u32>,
    /// The last point read was a named node itself (`(a)`): its index in `nodes`, taken by
    /// the reader (tikz's `\iftikz@shapeborder`).
    shape_ref: std::cell::Cell<Option<usize>>,
    /// A named coordinate was read (its canvas point taken back through `tf`).
    named: std::cell::Cell<bool>,
    /// `\tikzleveldistance`, `\tikzsiblingdistance` (15mm each, tikz.code.tex:1418), or the
    /// picture's `level distance=`, `sibling distance=`.
    /// Trees' level and sibling distances, and the growth's angle (`grow=`, whole degrees).
    tree: [i32; 3],
    /// The picture's or scope's `shorten <`, `shorten >` (`\pgfsetshortenstart`, `..end`):
    /// each open path's, added to its own (pgfcorepathusage.code.tex:333).
    shorten: [i32; 2],
    /// The picture's `auto` (`\tikz@install@auto@anchor`): a path label's anchor then comes
    /// from its segment's direction, not drawn here, so labels decline.
    auto: bool,
}

/// The length registers a point may name (`(\linewidth,0)`): pgfmath prints the length in
/// pt, which reads back as itself.
const REGISTERS: [&str; 6] = ["linewidth", "textwidth", "textheight", "columnwidth", "paperwidth", "paperheight"];

/// One coordinate of `(x,y)`: `\tikz@checkunit` parses it with pgfmath; without a unit it is
/// that number times the unit vector (`\pgfpointxy`), else the length (`\pgfpoint`). tikz.code.tex `\tikz@@@parse@regular` treats the two
/// coordinates apart (`\pgfpointadd{\pgfpoint{x}{0pt}}{\pgfpointxy{0}{y}}` when only one has a
/// unit; the other's term is an exact zero), so each is computed alone.
fn component(v: &str, ctx: &Ctx, axis: usize) -> Option<i32> {
    let reg = |name: &str| REGISTERS.contains(&name).then(|| (ctx.dims)(name)).flatten();
    let (v, units) = pgfmath::parse(v, &reg)?;
    // Far below `\maxdimen`, so no sum of the picture overflows.
    Some(if units { v } else { fmul(v, ctx.unit[axis]) }).filter(|v| v.abs() < 1 << 28)
}

/// An angle as pgfmath parses a number: in degrees, as a length in pt (below `16384pt`, TeX's
/// largest), in sp.
fn angle(v: &str) -> Option<i32> {
    let v = v.trim();
    let body = v.strip_prefix('-').unwrap_or(v);
    if body.is_empty() || body == "." || !body.bytes().all(|b| b.is_ascii_digit() || b == b'.') || body.matches('.').count() > 1 {
        return None;
    }
    let sp = crate::backend::backend_scaled::scan_dimen(body, 0, 0).filter(|sp| *sp < 1 << 30)? as i32;
    Some(if v.starts_with('-') { -sp } else { sp })
}

/// `(a:r)` and `(a:r1 and r2)` (tikz.code.tex:5288): with units `\pgfpointpolar`, the
/// radii's lengths times pgfmath's cosine and sine of the angle as factors; without,
/// `\pgfpointpolarxy`, the radii in pt so multiplied, then each a factor of its unit vector.
fn polar(inner: &str, ctx: &Ctx) -> Option<(i32, i32)> {
    let (a, r) = inner.split_once(':')?;
    let angle = angle(a)?;
    let (r1, r2) = r.split_once(" and ").unwrap_or((r, r));
    let (c, s) = (pgfmath::cos(angle), pgfmath::sin(angle));
    match (number(r1), number(r2)) {
        (Some(p), Some(q)) => Some((fmul(fmul(c, p), ctx.unit[0]), fmul(fmul(s, q), ctx.unit[1]))),
        _ => Some((fmul(c, length(r1)?), fmul(s, length(r2)?))),
    }
}

/// A circle's or ellipse's radii (tikz.code.tex:3542): `(r)` (`\tikz@do@circle`, no space),
/// `(a and b)` (`\tikz@do@ellipse`, whose `#2{` line leaves a space), `[radius=r]`,
/// `[x radius=a, y radius=b]` (`\tikz@circle@opt`, one more after its call). A number is a
/// multiple of each unit vector (`\pgfpointxy`), a length both axes' length; both radii of
/// one kind.
fn circle_args(c: &mut Cursor, ctx: &Ctx) -> Option<(i32, i32, u32)> {
    let (a, b, spaces) = if c.eat("(") {
        let inner = c.parenthesized()?;
        match inner.split_once(" and ") {
            Some((a, b)) => (a.trim().to_string(), b.trim().to_string(), 1),
            None => (inner.trim().to_string(), inner.trim().to_string(), 0),
        }
    } else if c.eat("[") {
        let (mut a, mut b) = (None, None);
        for item in c.bracketed()?.split(',') {
            let (k, v) = item.split_once('=')?;
            let v = v.trim().to_string();
            match k.trim() {
                "radius" => (a, b) = (Some(v.clone()), Some(v)),
                "x radius" => a = Some(v),
                "y radius" => b = Some(v),
                _ => return None,
            }
        }
        (a?, b?, 2)
    } else {
        return None;
    };
    match (number(&a), number(&b)) {
        (Some(_), Some(_)) => Some((times_xx(&a, ctx.unit[0])?, times_xx(&b, ctx.unit[1])?, spaces)),
        _ => Some((length(&a)?, length(&b)?, spaces)),
    }
}

/// The top-level items of a key list: commas inside braces or parentheses stay.
/// The keys tikz.code.tex declares with `\tikzoption` (`\pgfkeysdef`, most with
/// `.@def` `\pgfkeysvaluerequired`): a `.style` of one of these names leaves the
/// value check in place, so `\node[step]` after `step/.style={draw}` draws nothing (probe).
const TIKZ_OPTIONS: [&str; 170] = ["above", "above left", "above left of", "above of", "above right", "above right of", "allow upside down", "ampersand replacement", "anchor", "arrows", "at", "auto", "ball color", "baseline", "below", "below left", "below left of", "below of", "below right", "below right of", "bend", "bend pos", "bottom color", "cap", "cells", "child anchor", "clip", "cm", "color", "column sep", "const plot", "const plot mark left", "const plot mark mid", "const plot mark right", "dash", "dash pattern", "dash phase", "domain", "double", "double distance", "draw", "draw opacity", "edge from parent path", "even odd rule", "execute at begin cell", "execute at begin matrix", "execute at begin node", "execute at begin picture", "execute at begin scope", "execute at begin to", "execute at empty cell", "execute at end cell", "execute at end matrix", "execute at end node", "execute at end picture", "execute at end scope", "execute at end to", "fill", "fill opacity", "font", "grow", "growth function", "growth parent anchor", "id", "inner color", "join", "jump mark left", "jump mark mid", "jump mark right", "left", "left color", "left of", "level distance", "line cap", "line join", "line width", "mark", "mark indices", "mark phase", "mark repeat", "mark size", "matrix", "matrix anchor", "middle color", "miter limit", "node distance", "node font", "nodes", "nonzero rule", "only marks", "opacity", "outer color", "parabola height", "parametric", "parent anchor", "path only", "polar comb", "pos", "prefix", "range", "raw gnuplot", "remember picture", "reset cm", "right", "right color", "right of", "rotate", "rotate around", "rounded corners", "row sep", "samples", "samples at", "save path", "scale", "scale around", "set style", "shading", "shading angle", "shape", "sharp corners", "sharp cycle", "sharp plot", "shift", "shift only", "sibling distance", "sloped", "smooth", "smooth cycle", "step", "swap", "tension", "text", "text badly centered", "text badly ragged", "text badly ragged left", "text centered", "text depth", "text height", "text justified", "text opacity", "text ragged", "text ragged left", "text width", "to path", "top color", "transform shape", "trim left", "trim right", "use as bounding box", "use path", "variable", "x", "xbar", "xbar interval", "xcomb", "xrange", "xscale", "xshift", "xslant", "xstep", "y", "ybar", "ybar interval", "ycomb", "yrange", "yscale", "yshift", "yslant", "ystep", "z"];

/// An options list with each item that names one of the picture's styles replaced by the
/// style's keys (pgfkeys' `.style`, one level: a style's keys naming another are not).
fn styled(list: &str, ctx: &Ctx) -> String {
    if ctx.styles.is_empty() {
        return list.to_string();
    }
    key_items(list).into_iter().map(|item| ctx.styles.iter().rev().find(|(n, _)| n == item.trim()).map_or(item, |(_, v)| v.as_str())).collect::<Vec<_>>().join(",")
}

/// `<name>/.style={<keys>}` (pgfkeys' `.style` handler): the name and its keys, one level of
/// braces and no parameter. Refused: a name of a tikz key (`\tikzoption`, [`TIKZ_OPTIONS`]:
/// `.style` leaves its `.@def` value check, so `\node[step]` after `step/.style={draw}` draws
/// nothing; probe), and an `every <thing>` style other than `every node` (tikz runs `every
/// path`, `every picture`, ... where the grammar would not apply them).
fn style_def(n: &str, v: &str) -> Option<(String, String)> {
    let name = n.trim().strip_suffix("/.style")?.trim();
    if name.is_empty() || !name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == ' ') || TIKZ_OPTIONS.contains(&name) || (name.starts_with("every ") && !matches!(name, "every node" | "every picture" | "every loop")) {
        return None;
    }
    let v = v.trim().strip_prefix('{')?.strip_suffix('}')?;
    // `every loop/.style={}`: loops without `every loop`'s tip and shortening (`to_curve`'s
    // caller); other keys there are not taken.
    if name == "every loop" && !v.trim().is_empty() {
        return None;
    }
    if v.contains(['{', '}', '#']) {
        return None;
    }
    // pgfkeys skips empty items (`inner sep=2pt, }`).
    Some((name.to_string(), key_items(v).into_iter().map(str::trim).filter(|k| !k.is_empty()).collect::<Vec<_>>().join(",")))
}

/// `\tikzset{<list>}` (tikz.code.tex:73 `\pgfqkeys{/tikz}`) of style definitions only: each
/// item [`style_def`]'s, empty items skipped. They are pgfkeys' macros, defined globally to
/// the document when set in the preamble, and typeset nothing.
pub(super) fn tikzset_styles(list: &str) -> Option<Vec<(String, String)>> {
    key_items(list).into_iter().map(str::trim).filter(|k| !k.is_empty()).map(|k| k.split_once('=').and_then(|(n, v)| style_def(n, v))).collect()
}

/// After `\tikzset`: the length of its `{list}` argument when [`tikzset_styles`] takes it.
pub(super) fn tikzset_call_len(after: &str) -> Option<usize> {
    let k = group_len(after)?;
    tikzset_styles(&after[1..k - 1])?;
    Some(k)
}

/// After `\tikzstyle`: tikz.code.tex:1524-1550 `\tikzstyle{name}=[keys]` (`\tikz@style@parseA
/// #1#2=#3[#4]`), `\tikz@style@parseD`'s `\pgfkeys{/tikz/#1/.style={#4}}`; `#2` blank (a `+`
/// appends, a `[` sets a default: refused), `#3` blank. The length read and the style.
pub(super) fn tikzstyle_call(after: &str) -> Option<(usize, (String, String))> {
    let n = group_len(after)?;
    let name = &after[1..n - 1];
    let rest = &after[n..];
    let eq = rest.find('=')?;
    if !rest[..eq].trim().is_empty() {
        return None;
    }
    let r2 = &rest[eq + 1..];
    let open = r2.find('[')?;
    if !r2[..open].trim().is_empty() {
        return None;
    }
    let mut depth = 0i32;
    let close = r2[open..].char_indices().find(|&(_, ch)| {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        ch == ']' && depth == 0
    })?.0 + open;
    let def = style_def(&format!("{name}/.style"), &format!("{{{}}}", &r2[open + 1..close]))?;
    Some((n + eq + 1 + close + 1, def))
}

/// A `{..}` group at the start of `s`: its length with both braces (`\{`, `\}` refused).
fn group_len(s: &str) -> Option<usize> {
    if !s.starts_with('{') {
        return None;
    }
    let mut depth = 0i32;
    for (k, ch) in s.char_indices() {
        match ch {
            '\\' if s[k + 1..].starts_with(['{', '}']) => return None,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(k + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// The styles a document's preamble defines with `\tikzset` and `\tikzstyle` at brace depth
/// zero, in order (a later definition of a name wins, as pgfkeys redefines it). Read by the
/// pre-scan for every picture of the body; one the grammar does not take is left out (the
/// pre-scan stops at it anyway).
pub(super) fn preamble_styles(src: &str) -> Vec<(String, String)> {
    let pre = &src[..src.find("\\begin{document}").unwrap_or(src.len())];
    let Some(pre) = strip_comments(pre) else { return Vec::new() };
    let (mut out, mut depth, mut k) = (Vec::new(), 0i32, 0);
    let b = pre.as_bytes();
    while k < b.len() {
        match b[k] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'\\' => {
                let r = &pre[k + 1..];
                let name_len = r.bytes().take_while(u8::is_ascii_alphabetic).count();
                let after = r[name_len..].trim_start();
                if depth == 0 && &r[..name_len] == "usetikzlibrary" {
                    if group_len(after).is_some_and(|n| names_library(&after[1..n - 1], "automata")) {
                        out.extend(AUTOMATA_STYLES.iter().map(|(a, b)| (a.to_string(), b.to_string())));
                    }
                } else if depth == 0 && &r[..name_len] == "tikzset" {
                    if let Some(n) = group_len(after) {
                        out.extend(tikzset_styles(&after[1..n - 1]).unwrap_or_default());
                    }
                } else if depth == 0 && &r[..name_len] == "tikzstyle" {
                    if let Some((_, d)) = tikzstyle_call(after) {
                        out.push(d);
                    }
                }
                k += 1 + name_len.max(1);
                continue;
            }
            _ => {}
        }
        k += 1;
    }
    out
}

fn key_items(list: &str) -> Vec<&str> {
    let (mut items, mut depth, mut from) = (Vec::new(), 0i32, 0);
    for (k, ch) in list.char_indices() {
        match ch {
            '{' | '(' => depth += 1,
            '}' | ')' => depth -= 1,
            ',' if depth == 0 => {
                items.push(&list[from..k]);
                from = k + 1;
            }
            _ => {}
        }
    }
    items.push(&list[from..]);
    items
}

/// An arc's angles and radii (tikz.code.tex:3649 `\tikz@arc@opt`, :3671 `\tikz@@arcto`):
/// `(s:e:r)`, `(s:e:rx and ry)`, or `[start angle=, end angle=, delta angle=, radius=,
/// x radius=, y radius=]` (a missing angle is the other plus or less the delta). Both radii
/// lengths or both numbers.
fn arc_args(c: &mut Cursor) -> Option<(i32, i32, [i32; 2], bool, bool)> {
    let mut delta = false;
    let (s, e, r1, r2) = if c.eat("(") {
        let inner = c.parenthesized()?;
        let mut it = inner.splitn(3, ':');
        let (s, e, r) = (angle(it.next()?)?, angle(it.next()?)?, it.next()?);
        let (r1, r2) = r.split_once(" and ").unwrap_or((r, r));
        (s, e, r1.trim().to_string(), r2.trim().to_string())
    } else if c.eat("[") {
        let (mut s, mut e, mut d, mut rx, mut ry) = (None, None, None, None, None);
        for item in c.bracketed()?.split(',') {
            let (k, v) = item.split_once('=')?;
            let v = v.trim();
            match k.trim() {
                "start angle" => s = Some(angle(v)?),
                "end angle" => e = Some(angle(v)?),
                "delta angle" => d = Some(angle(v)?),
                "radius" => (rx, ry) = (Some(v.to_string()), Some(v.to_string())),
                "x radius" => rx = Some(v.to_string()),
                "y radius" => ry = Some(v.to_string()),
                _ => return None,
            }
        }
        let (s, e) = match (s, e, d) {
            (Some(s), Some(e), None) => (s, e),
            (Some(s), None, Some(d)) => {
                delta = true;
                (s, s.checked_add(d)?)
            }
            (None, Some(e), Some(d)) => {
                delta = true;
                (e.checked_sub(d)?, e)
            }
            _ => return None,
        };
        (s, e, rx?, ry?)
    } else {
        return None;
    };
    match (number(&r1), number(&r2)) {
        (Some(a), Some(b)) => Some((s, e, [a, b], true, delta)),
        _ => Some((s, e, [length(&r1)?, length(&r2)?], false, delta)),
    }
}

/// An arc's polar point at angle `a` (`\tikz@do@arc`): `\pgfpointpolar` of the lengths, or
/// `\pgfpointpolarxy` of the numbers.
fn arc_point(a: i32, r: [i32; 2], unitless: bool, ctx: &Ctx) -> (i32, i32) {
    let (x, y) = (fmul(pgfmath::cos(a), r[0]), fmul(pgfmath::sin(a), r[1]));
    if unitless {
        (fmul(x, ctx.unit[0]), fmul(y, ctx.unit[1]))
    } else {
        (x, y)
    }
}

/// A point: `(x,y)`, `(a:r)`, `(name)` of a `\coordinate`, `+(..)` or `++(..)` relative to `last`, or
/// (in a `remember picture,overlay` picture) `([shifts]current page.<anchor>)`. The second
/// value is whether the point becomes tikz's last point (`+` leaves it).
fn point(c: &mut Cursor, ctx: &Ctx, last: (i32, i32)) -> Option<(Pt, bool)> {
    let rel = if c.eat("++") {
        2
    } else if c.eat("+") {
        1
    } else {
        0
    };
    if !c.eat("(") {
        return None;
    }
    if rel == 0 && (c.s.starts_with('[') || c.s.starts_with("current page.")) {
        return coord(c).map(|p| (p, true));
    }
    if rel == 0 && c.s.starts_with('$') {
        return calc_point(c, ctx).map(|q| (q, true));
    }
    let inner = c.parenthesized()?;
    let (x, y) = match inner.split_once(',') {
        Some((x, y)) => (component(x, ctx, 0)?, component(y, ctx, 1)?),
        None if inner.contains(':') => polar(inner, ctx)?,
        None if rel != 0 => return None,
        None => {
            let name = inner.trim();
            if let Some(q) = ctx.names.iter().rev().find(|(n, _)| n == name).map(|(_, q)| *q) {
                // `\pgfpointanchor`: the coordinate's point on the canvas through the inverse
                // of the current transformation (the path's own transformations refused by
                // the path reader, `named`).
                ctx.named.set(true);
                ctx.tf.invert()?.apply(q.0, q.1)
            } else if ctx.tf != Tf::ID {
                // Nodes are kept on the canvas: not under a picture's transformation.
                return None;
            } else if let Some(k) = ctx.nodes.iter().position(|s| s.name == name) {
                // A node itself: its centre for now; the path puts it on the border.
                ctx.shape_ref.set(Some(k));
                ctx.nodes[k].anchor("center")?.0
            } else {
                // `(name.anchor)`: `\pgfpointanchor`, the anchor's code typeset.
                let (n, a) = name.split_once('.')?;
                let ((x, y), sp) = ctx.nodes.iter().find(|s| s.name == n)?.anchor(a)?;
                ctx.spaces.set(ctx.spaces.get() + sp);
                (x, y)
            }
        }
    };
    let (x, y) = if rel > 0 { (last.0.checked_add(x)?, last.1.checked_add(y)?) } else { (x, y) };
    Some((Pt::At(x, y), rel != 1))
}

/// The calc library's `($(a) + (b) - (c)$)`, the `(` already read (tikzlibrarycalc.code.tex
/// `\tikz@cc@parse`): each term a point of the picture, `\pgfpointadd`ed in sp; a node's name
/// is its `center` (`\pgfpointanchor`, the anchor's space typeset), not its border. Not
/// under a transformation, no factors or partway points (`!`).
fn calc_point(c: &mut Cursor, ctx: &Ctx) -> Option<Pt> {
    let s = c.s.strip_prefix('$')?;
    let end = s.find("$)")?;
    let mut t = Cursor { s: s[..end].trim() };
    if ctx.tf != Tf::ID || t.s.contains(['!', '*', '$']) {
        return None;
    }
    let saved = ctx.shape_ref.take();
    let mut sum = (0i32, 0i32);
    let mut sign = 1;
    loop {
        if !t.s.starts_with('(') {
            return None;
        }
        let (Pt::At(x, y), _) = point(&mut t, ctx, (0, 0))? else { return None };
        if let Some(k) = ctx.shape_ref.take() {
            ctx.spaces.set(ctx.spaces.get() + ctx.nodes[k].anchor("center")?.1);
        }
        sum = (sum.0.checked_add(sign * x)?, sum.1.checked_add(sign * y)?);
        t.s = t.s.trim_start();
        if t.s.is_empty() {
            break;
        }
        sign = if t.eat("+") { 1 } else if t.eat("-") { -1 } else { return None };
        t.s = t.s.trim_start();
    }
    ctx.shape_ref.set(saved);
    c.s = &s[end + 2..];
    (sum.0.abs() < 1 << 28 && sum.1.abs() < 1 << 28).then_some(Pt::At(sum.0, sum.1))
}

/// `([shifts]current page.<anchor>)`, the `(` already read.
fn coord(c: &mut Cursor) -> Option<Pt> {
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
        // tikz skips blanks before the name (`([xshift=1cm]` and a line end): no glue.
        c.blanks()?;
    }
    if !c.eat("current page.") {
        return None;
    }
    let anchor = [("north west", Anchor::NorthWest), ("north east", Anchor::NorthEast), ("south west", Anchor::SouthWest), ("south east", Anchor::SouthEast), ("north", Anchor::North), ("south", Anchor::South), ("west", Anchor::West), ("east", Anchor::East), ("center", Anchor::Center)].into_iter().find(|(n, _)| c.eat(n))?.1;
    c.eat(")").then_some(Pt::Page(Coord { anchor, shift, shifts }))
}

/// tikz.code.tex:1575–1581, the line width styles.
const WIDTHS: [(&str, &str); 7] = [("ultra thin", "0.1pt"), ("very thin", "0.2pt"), ("thin", "0.4pt"), ("semithick", "0.6pt"), ("thick", "0.8pt"), ("very thick", "1.2pt"), ("ultra thick", "1.6pt")];

/// tikz.code.tex:1584–1589, the dash styles: `on <a> off <b>` (`\pgflinewidth` read when the
/// options run).
const DASHES: [(&str, &[Dash]); 6] = [
    ("dotted", &[Dash::LineWidth, Dash::Len(131072)]),
    ("densely dotted", &[Dash::LineWidth, Dash::Len(65536)]),
    ("loosely dotted", &[Dash::LineWidth, Dash::Len(262144)]),
    ("dashed", &[Dash::Len(196608), Dash::Len(196608)]),
    ("densely dashed", &[Dash::Len(196608), Dash::Len(131072)]),
    ("loosely dashed", &[Dash::Len(196608), Dash::Len(393216)]),
];

/// A colour as an option value or key: a name and xcolor's `!` mixes.
fn color_spec(s: &str) -> Option<String> {
    let s = s.trim();
    (s.starts_with(|ch: char| ch.is_ascii_alphabetic()) && s.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '!' || ch == '.')).then(|| s.to_string())
}

/// A path's `[options]`: into `p`'s modes, options and corner radius.
fn path_options(list: &str, p: &mut PathCmd, ctx: &Ctx) -> Option<()> {
    // tikz's `\tikz@addtransform`: in written order after the picture's.
    for item in key_items(list) {
        let item = item.trim();
        if let Some((k, v)) = item.split_once('=') {
            match k.trim() {
                "line width" => p.opts.push(PathOpt::Width(width_value(v)?)),
                "shorten <" => p.shorten[0] = length(v).filter(|l| *l >= 0)?,
                "shorten >" => p.shorten[1] = length(v).filter(|l| *l >= 0)?,
                "draw" => {
                    p.draw = true;
                    p.opts.push(PathOpt::Stroke(color_spec(v)?));
                }
                "fill" => {
                    p.fill = true;
                    p.opts.push(PathOpt::Fill(color_spec(v)?));
                }
                "rounded corners" => p.round = length(v).filter(|r| *r > 0)?,
                // tikz.code.tex:189, 243, 248: `\pgfsetstrokeopacity`, `\pgfsetfillopacity`,
                // `opacity` both (stroke first), each `\pgfmathparse`d; values in 0..1.
                "draw opacity" | "fill opacity" | "opacity" => {
                    let v = v.trim();
                    let (o, units) = pgfmath::parse(v, &|_| None)?;
                    if units || !(0..=1 << 16).contains(&o) {
                        return None;
                    }
                    // pgfmath's fast lane keeps a plain number as written (`.25`, `1`; probe).
                    let o = if v.bytes().all(|b| b.is_ascii_digit() || b == b'.') { v.to_string() } else { pgfmath::print(o) };
                    let k = k.trim();
                    if k != "fill opacity" {
                        p.opts.push(PathOpt::Opacity(true, o.clone()));
                    }
                    if k != "draw opacity" {
                        p.opts.push(PathOpt::Opacity(false, o));
                    }
                }
                // tikz.code.tex:471: `step=<value>` both (a point refused), `xstep`, `ystep`;
                // a grid reads them with `\tikz@checkunit` (a number of the unit vector, or a
                // length).
                "step" if !v.trim().starts_with('(') => p.step = [component(v, ctx, 0)?, component(v, ctx, 1)?],
                "xstep" => p.step[0] = component(v, ctx, 0)?,
                "ystep" => p.step[1] = component(v, ctx, 1)?,
                // pgfcoretransformations.code.tex:368: `\pgfpoint{#1}{+0pt}`, whose `+0pt` is
                // an `\unskip`.
                "xshift" => {
                    p.tf = p.tf.shift(length(v)?, 0);
                    p.unskips += 1;
                }
                "yshift" => {
                    p.tf = p.tf.shift(0, length(v)?);
                    p.unskips += 1;
                }
                "scale" => p.tf = p.tf.scale(number(v)?),
                "xscale" => p.tf = p.tf.scale_axis(number(v)?, false),
                "yscale" => p.tf = p.tf.scale_axis(number(v)?, true),
                // `shift={(x,y)}` (tikz.code.tex `\tikz@addtransform{\pgftransformshift{..}}`):
                // the point read as any point, then shifted to through the matrix; no space.
                "shift" => {
                    let v = v.trim();
                    let inner = v.strip_prefix('{').and_then(|v| v.strip_suffix('}')).unwrap_or(v);
                    let mut pc = Cursor { s: inner.trim() };
                    if pc.s.starts_with('+') {
                        return None;
                    }
                    let (Pt::At(x, y), _) = point(&mut pc, ctx, (0, 0))? else { return None };
                    if !pc.s.trim().is_empty() {
                        return None;
                    }
                    p.tf = p.tf.shift(x, y);
                }
                // `\pgftransformrotate`: no space, no scope.
                "rotate" => p.tf = p.tf.rotate(angle(v)?),
                _ => return None,
            }
        } else if let Some((_, w)) = WIDTHS.iter().find(|(n, _)| *n == item) {
            p.opts.push(PathOpt::Width(length(w)?));
        } else if let Some((_, d)) = DASHES.iter().find(|(n, _)| *n == item) {
            p.opts.push(PathOpt::Dash(d));
        } else {
            match item {
                "draw" => p.draw = true,
                "fill" => p.fill = true,
                "rounded corners" => p.round = 4 << 16,
                // tikz.code.tex `\tikz@processarrows`: `-` splits the start tip from the end
                // tip; `<`, `>` are `to` (`\pgfarrowsdeclarealias{<}{>}{to}{to}`).
                // `>` is the picture's `>=` (`to` without one).
                "->" => (p.arrows, p.own_arrows, p.tip) = ([false, true], true, ctx.gt),
                "<-" => (p.arrows, p.own_arrows, p.tip) = ([true, false], true, ctx.gt),
                "<->" => (p.arrows, p.own_arrows, p.tip) = ([true, true], true, ctx.gt),
                "-" => (p.arrows, p.own_arrows, p.tip) = ([false; 2], true, ctx.gt),
                "-stealth" => (p.arrows, p.own_arrows, p.tip) = ([false, true], true, Tip::Stealth),
                "stealth-" => (p.arrows, p.own_arrows, p.tip) = ([true, false], true, Tip::Stealth),
                "stealth-stealth" => (p.arrows, p.own_arrows, p.tip) = ([true, true], true, Tip::Stealth),
                "-latex" => (p.arrows, p.own_arrows, p.tip) = ([false, true], true, Tip::OldLatex),
                "latex-" => (p.arrows, p.own_arrows, p.tip) = ([true, false], true, Tip::OldLatex),
                "latex-latex" => (p.arrows, p.own_arrows, p.tip) = ([true, true], true, Tip::OldLatex),
                "-stealth'" => (p.arrows, p.own_arrows, p.tip) = ([false, true], true, Tip::StealthPrime),
                "stealth'-" => (p.arrows, p.own_arrows, p.tip) = ([true, false], true, Tip::StealthPrime),
                "stealth'-stealth'" => (p.arrows, p.own_arrows, p.tip) = ([true, true], true, Tip::StealthPrime),
                "-Latex" => (p.arrows, p.own_arrows, p.tip) = ([false, true], true, Tip::Latex),
                "Latex-" => (p.arrows, p.own_arrows, p.tip) = ([true, false], true, Tip::Latex),
                "Latex-Latex" => (p.arrows, p.own_arrows, p.tip) = ([true, true], true, Tip::Latex),
                // Keys of tikz or pgf (`solid`, `help lines`, `->`, shapes ...) are not colours.
                // tikz.code.tex:1558 `help lines/.style={color=gray,line width=0.2pt}`; `color=`
                // is `\pgfsetcolor`: pgf's stroke and fill colours (`RG`, `rg`), no stack push.
                "help lines" => {
                    p.opts.push(PathOpt::Stroke("gray".into()));
                    p.opts.push(PathOpt::Fill("gray".into()));
                    p.opts.push(PathOpt::Width(13107));
                }
                "solid" | "none" | "circle" | "rectangle" => return None,
                _ => p.opts.push(PathOpt::Color(color_spec(item)?)),
            }
        }
    }
    Some(())
}

/// Whether the path's current point is a `current page` point (a grid's first corner).
fn ends_on_page(p: &PathCmd) -> bool {
    let page = |q: &Pt| matches!(q, Pt::Page(_));
    match p.segs.last() {
        None => page(&p.start),
        Some((Seg::Line(q) | Seg::Hv(q) | Seg::Vh(q) | Seg::Rect(q) | Seg::Move(q) | Seg::Grid(q, _), _)) => page(q),
        Some(_) => false,
    }
}

/// tikz's grid step without `step`: 1cm.
const GRID_STEP: i32 = 1864679;

/// `\pgfpathgrid` (pgfcorepathconstruct.code.tex:1127) between the corners `lo`, `hi` (sorted):
/// for each axis whose step is above `.01pt`, the first line at `\divide`d multiple of the step
/// (`\c@pgf@counta` times it, advanced by one step when below the corner), drawn before the
/// test, then while below the far corner; then one more `0.01pt` short of it when that is
/// below. Horizontal lines first, from the left corner; each a moveto and a lineto, in user
/// space.
fn grid_lines(lo: (i32, i32), hi: (i32, i32), step: [i32; 2]) -> Option<Vec<[(i32, i32); 2]>> {
    let ticks = |a: i32, b: i32, st: i32| -> Option<Vec<i32>> {
        let mut out = Vec::new();
        if st <= 655 {
            return Some(out);
        }
        let mut t = (a / st) as i64 * st as i64;
        if t < a as i64 {
            t += st as i64;
        }
        loop {
            out.push(i32::try_from(t).ok()?);
            t += st as i64;
            if t >= b as i64 || out.len() > 4000 {
                break;
            }
        }
        if t - 655 < b as i64 {
            out.push(i32::try_from(t - 655).ok()?);
        }
        Some(out)
    };
    let mut lines: Vec<[(i32, i32); 2]> = ticks(lo.1, hi.1, step[1])?.into_iter().map(|y| [(lo.0, y), (hi.0, y)]).collect();
    lines.extend(ticks(lo.0, hi.0, step[0])?.into_iter().map(|x| [(x, lo.1), (x, hi.1)]));
    Some(lines)
}

/// A picture point's coordinates; a page point is never tikz's last point for a relative
/// one (refused), so it stands as the origin.
fn at_of(q: Pt) -> (i32, i32) {
    match q {
        Pt::At(x, y) => (x, y),
        Pt::Page(_) => (0, 0),
    }
}

/// One path command after its name: `[options] <point> <operations> ;`.
fn path(c: &mut Cursor, ctx: &Ctx, mut p: PathCmd, overlay: bool) -> Option<PathCmd> {
    c.blanks()?;
    if c.eat("[") {
        let opts = c.bracketed()?;
        // `every node/.style={..}` on a path without nodes styles nothing.
        let stmt = &c.s[..c.s.find(';').unwrap_or(c.s.len())];
        let kept: Vec<&str> = key_items(opts).into_iter().filter(|k| stmt.contains("node") || !k.trim().starts_with("every node/.style")).collect();
        if kept.len() == key_items(opts).len() || kept.iter().any(|k| !k.trim().is_empty()) {
            path_options(&styled(&kept.join(","), ctx), &mut p, ctx)?;
        }
        c.blanks()?;
    }
    // `\path` paints nothing: only as a point and its nodes, without options (below), or
    // with edges, drawn in its options' scope as `\draw`'s are (probes m9, n3).
    let undrawn_opts = !(p.draw || p.fill) && !p.opts.is_empty();
    // tikz's `\tikz@lastx`, `\tikz@lasty` start at the origin.
    ctx.spaces.set(0);
    ctx.shape_ref.set(None);
    let mut plot_segs = Vec::new();
    // The text of the last point, absolute and written alone (an `edge`'s `\tikztostart`).
    let mut last_src: Option<&str> = None;
    let mut edges: Vec<PathCmd> = Vec::new();
    let start = if c.s.starts_with("plot") && !c.s[4..].starts_with(|ch: char| ch.is_ascii_alphabetic()) {
        let (s, segs) = smooth_plot(c, ctx)?;
        plot_segs = segs;
        Pt::At(s.0, s.1)
    } else {
        let before = c.s;
        let q = point(c, ctx, (0, 0))?.0;
        last_src = Some(&before[..before.len() - c.s.len()]);
        q
    };
    // The last point was written as `(x,y)` or `(a:r)`: not a node nor a named coordinate
    // (`\tikz@to@modify` takes those as shapes).
    let mut plain_last = last_src.is_some_and(plain_point);
    p.start = start;
    // Named nodes among the points: (index of the point, 0 the start, and the node).
    let mut refs: Vec<(usize, usize)> = ctx.shape_ref.take().map(|k| (0, k)).into_iter().collect();
    let mut last = at_of(start);
    // `\tikz@lastx` (every point) beside `last`, `\tikz@lastxsaved` (not moved by `+`).
    let mut cur = last;
    let mut sub_start = last;
    if let Some(Seg::Curve(q, _)) = plot_segs.last() {
        (last, cur) = (q[2], q[2]);
    }
    p.segs.extend(plot_segs.into_iter().map(|s| (s, true)));
    // Its nodes: each with the segment it is on (its timer's) and whether it is within one.
    let mut on_path: Vec<(node::NodeCmd, usize, bool)> = Vec::new();
    let word = |c: &Cursor, w: &str| c.s.starts_with(w) && !c.s[w.len()..].starts_with(|ch: char| ch.is_ascii_alphabetic());
    loop {
        c.blanks()?;
        if c.eat(";") {
            break;
        }
        let was_plain = std::mem::replace(&mut plain_last, false);
        // `to[out=a,in=b]`, `to[bend left]`, `to[bend right=a]` between two points written as
        // coordinates: the topaths library's `\tikz@to@curve@path`, a curve (tikzlibrarytopaths
        // .code.tex `\tikz@to@compute`, `\tikz@to@compute@relative`).
        if word(c, "to") && c.s[2..].trim_start().starts_with('[') {
            c.eat("to");
            c.blanks()?;
            c.eat("[");
            let opts = c.bracketed()?;
            c.blanks()?;
            let looped = key_items(opts).into_iter().any(|k| k.trim().starts_with("loop"));
            let from_edge = key_items(opts).into_iter().any(|k| k.trim() == EDGE_MARK);
            // Its nodes (`\tikztonodes` after the curve): on it, midway by its timer; a loop's
            // side places them (only an edge's, which put it in).
            while word(c, "node") {
                if looped && !from_edge {
                    return None;
                }
                c.eat("node");
                on_path.push((node::parse_on_path(c, ctx, overlay)?, p.segs.len() + 1, true));
                c.blanks()?;
            }
            // A node at the start: the point before (the path's start or a moveto) alone.
            let from_node = refs.last().filter(|r| r.0 == p.segs.len()).map(|r| r.1);
            let moved_to = p.segs.last().is_none_or(|(s, m)| *m && matches!(s, Seg::Move(_)));
            if !(was_plain || from_node.is_some() && moved_to) || last != cur || ctx.tf != Tf::ID || !c.s.starts_with('(') {
                return None;
            }
            let before = c.s;
            // A loop's target is its start (`\let\tikztotarget=\tikztostart`), whatever is
            // written, `()` too.
            let (x, y, to_node) = if looped && c.eat("()") {
                (last.0, last.1, from_node)
            } else {
                let (q, _) = point(c, ctx, last)?;
                let Pt::At(x, y) = q else { return None };
                let to_node = ctx.shape_ref.take();
                if to_node.is_none() && !plain_point(&before[..before.len() - c.s.len()]) {
                    return None;
                }
                (x, y, if looped { from_node } else { to_node })
            };
            if looped {
                // `every loop`: `->,shorten >=1pt` on an edge's own path; a loop in a path of
                // its own (probe h4: no tip, other spaces) is not taken.
                if from_node.is_none() || !from_edge {
                    return None;
                }
                if !ctx.styles.iter().any(|(n, _)| n == "every loop") {
                    (p.arrows, p.own_arrows, p.tip) = ([p.arrows[0], true], true, ctx.gt);
                    p.shorten[1] = 1 << 16;
                }
            }
            let a = from_node.map_or(End::At(last), End::Node);
            let b = to_node.map_or(End::At((x, y)), End::Node);
            let (start, q, spaces) = to_curve(opts, a, b, &ctx.nodes)?;
            if from_node.is_some() {
                refs.pop();
                match p.segs.last_mut() {
                    Some((seg, _)) => *seg = Seg::Move(Pt::At(start.0, start.1)),
                    None => p.start = Pt::At(start.0, start.1),
                }
            }
            plain_last = to_node.is_none();
            let seg = Seg::Curve(q, spaces);
            (last, cur) = (q[2], q[2]);
            p.segs.push((seg, true));
            continue;
        }
        // `edge [options] (target)`: a path of its own from the last point (a node's name: its
        // border) to the target, `\path[style=every edge,<options>] (start) -- (target)`
        // (`every edge` is `draw`); the last point stays. Not with nodes on it, not after a
        // point that is not written alone, not on a path of options.
        if word(c, "edge") {
            c.eat("edge");
            c.blanks()?;
            let local = if c.eat("[") { c.bracketed()? } else { "" };
            c.blanks()?;
            // A placement key among its options (`edge[right]`) sets `\tikz@anchor` in the edge
            // path's scope, where its labels start from: the same as `node[right]` on each
            // (probe l1 against l2). With a value it would shift the path: not taken.
            let (mut place, rest): (Vec<&str>, Vec<&str>) = key_items(local).into_iter().map(str::trim).filter(|k| !k.is_empty()).partition(|k| EDGE_PLACES.contains(k));
            // `loop <side>`'s side too (its style's first key).
            if let Some(side) = key_items(local).into_iter().map(str::trim).find_map(|k| k.strip_prefix("loop ").filter(|s| matches!(*s, "above" | "below" | "left" | "right"))) {
                place.insert(0, side);
            }
            let rest = rest.join(",");
            // Its nodes (`\tikztonodes`, after the target in `-- (\tikztotarget)
            // \tikztonodes`) are labels, midway as `-- node {..} (target)` puts them.
            let mut labels = String::new();
            while word(c, "node") {
                let from = c.s;
                c.eat("node");
                c.blanks()?;
                if c.eat("[") {
                    c.bracketed()?;
                    c.blanks()?;
                }
                braced(c)?;
                let piece = &from[..from.len() - c.s.len()];
                if place.is_empty() {
                    labels.push_str(piece);
                } else {
                    labels.push_str(&format!("node[{}]{}", place.join(","), &piece[4..]));
                }
                labels.push(' ');
                c.blanks()?;
            }
            if !c.eat("(") || c.s.starts_with(['[', '$']) {
                return None;
            }
            let target = c.parenthesized()?;
            let start = last_src.filter(|s| s.starts_with('('))?;
            // Options of the path: its scope's, which the edges and their labels are drawn in;
            // with labels only widths, which leave the text as it was (probes n5-n8).
            if !labels.is_empty() && p.opts.iter().any(|o| !matches!(o, PathOpt::Width(_))) || !on_path.is_empty() || local.contains(['[', ']', '{', '}']) {
                return None;
            }
            // `bend left`, `bend right`, `out`, `in` among them switch the `to path` to the
            // topaths curve: drawn as `to[..]` is.
            let curve_key = |k: &str| {
                let k = k.split_once('=').map_or(k, |(k, _)| k).trim();
                matches!(k, "bend left" | "bend right" | "out" | "in" | "looseness" | "min distance" | "loop" | "loop above" | "loop below" | "loop left" | "loop right")
            };
            let (bend, rest): (Vec<&str>, Vec<&str>) = key_items(&rest).into_iter().map(str::trim).filter(|k| !k.is_empty()).partition(|k| curve_key(k));
            let rest = rest.join(",");
            let op = if bend.is_empty() { "--".to_string() } else { format!("to[{},{EDGE_MARK}]", bend.join(",")) };
            let text = if rest.is_empty() { format!("{start} {op} {labels}({target});") } else { format!("[{rest}] {start} {op} {labels}({target});") };
            let saved = (ctx.spaces.take(), ctx.shape_ref.take());
            let e = PathCmd { fill: false, draw: true, opts: Vec::new(), round: 0, start: Pt::At(0, 0), segs: Vec::new(), tf: ctx.tf, unskips: 0, spaces: 0, arrows: p.arrows, own_arrows: false, tip: p.tip, nudges: Vec::new(), base_width: line_width(&p), nodes: Vec::new(), step: [GRID_STEP; 2], edges: Vec::new(), edge: None, bare: false, shorten: ctx.shorten };
            let e = path(&mut Cursor { s: &text }, ctx, e, overlay);
            ctx.spaces.set(saved.0);
            ctx.shape_ref.set(saved.1);
            let e = e?;
            if !e.edges.is_empty() {
                return None;
            }
            edges.push(e);
            continue;
        }
        // `node` after a point: at it, or with `pos=` on the segment before it.
        if word(c, "node") {
            c.eat("node");
            on_path.push((node::parse_on_path(c, ctx, overlay)?, p.segs.len(), false));
            continue;
        }
        // `to (p)` without options: `\tikz@do@to` puts in the default `to path`, `-- (\tikztotarget)
        // \tikztonodes` (tikz.code.tex:1334), its nodes labels midway: the same list as `--`
        // (probes o0-o3). Not to a relative point (scanned to an absolute one) or `cycle`.
        let to = word(c, "to") && c.s[2..].trim_start().starts_with(['(', 'n']);
        let (seg, moves) = if to || c.eat("--") {
            if to {
                c.eat("to");
            }
            c.blanks()?;
            // `-- node {..} <point>` (`\tikz@collect@label@onpath`): on the segment to come.
            while word(c, "node") {
                c.eat("node");
                on_path.push((node::parse_on_path(c, ctx, overlay)?, p.segs.len() + 1, true));
                c.blanks()?;
            }
            if !to && c.eat("cycle") {
                (Seg::Cycle, true)
            } else {
                if to && !c.s.starts_with('(') {
                    return None;
                }
                let (q, m) = point(c, ctx, last)?;
                (Seg::Line(q), m)
            }
        } else if c.eat("..") {
            // tikz.code.tex `\tikz@curveto`: `controls (a) and (b) .. (c)`, points of the
            // picture, no relative ones (they count from the controls).
            c.blanks()?;
            if !c.eat("controls") {
                return None;
            }
            let mut q = [(0, 0); 3];
            // `controls (a) .. (c)`: `\tikz@curveA` lets the second support be the first.
            let mut k = 0;
            while k < 3 {
                c.blanks()?;
                if !c.s.starts_with('(') {
                    return None;
                }
                let (Pt::At(x, y), _) = point(c, ctx, last)? else { return None };
                q[k] = (x, y);
                if k < 2 {
                    c.blanks()?;
                    if k == 0 && c.eat("..") {
                        q[1] = q[0];
                        k = 1;
                    } else if !c.eat(if k == 0 { "and" } else { ".." }) {
                        return None;
                    }
                }
                k += 1;
            }
            (Seg::Curve(q, 0), true)
        } else if c.s.starts_with("-|") || c.s.starts_with("|-") {
            let hv = c.eat("-|") || !c.eat("|-");
            c.blanks()?;
            let (q, m) = point(c, ctx, last)?;
            (if hv { Seg::Hv(q) } else { Seg::Vh(q) }, m)
        } else if c.eat("rectangle") {
            c.blanks()?;
            let (q, m) = point(c, ctx, last)?;
            (Seg::Rect(q), m)
        } else if c.s.starts_with("grid") && !c.s[4..].starts_with(|ch: char| ch.is_ascii_alphabetic()) {
            // `\tikz@grid`: options of its own are refused.
            c.eat("grid");
            c.blanks()?;
            let (q, m) = point(c, ctx, last)?;
            // Between `current page` points the lines fall at multiples of the step from the
            // picture's origin, so their number (two spaces each) depends on where the picture
            // lands: refused.
            let Pt::At(..) = q else { return None };
            if ends_on_page(&p) {
                return None;
            }
            let (a, b) = (at_of(q), cur);
            if grid_lines((a.0.min(b.0), a.1.min(b.1)), (a.0.max(b.0), a.1.max(b.1)), p.step)?.len() > 2000 {
                return None;
            }
            (Seg::Grid(q, p.step), m)
        } else if c.eat("circle") || c.eat("ellipse") {
            c.blanks()?;
            let (rx, ry, spaces) = circle_args(c, ctx)?;
            if rx <= 0 || ry <= 0 {
                return None;
            }
            (Seg::Circle(rx, ry, spaces), false)
        } else if c.eat("arc") {
            c.blanks()?;
            let (s, e, r, unitless, delta) = arc_args(c)?;
            // `\tikz@arcfinal`: the last point moves by the end's polar point less the start's.
            let (p0, p1) = (arc_point(s, r, unitless, ctx), arc_point(e, r, unitless, ctx));
            let to = (cur.0.checked_add(p1.0 - p0.0)?, cur.1.checked_add(p1.1 - p0.1)?);
            let axes = [fmul(r[0], ctx.unit[0]), fmul(r[1], ctx.unit[1])];
            (Seg::Arc { s, e, r, unitless, axes, to, delta }, true)
        } else if c.s.starts_with('(') || c.s.starts_with('+') {
            // A point alone: tikz's moveto.
            let before = c.s;
            let (q, m) = point(c, ctx, last)?;
            last_src = Some(&before[..before.len() - c.s.len()]);
            plain_last = last_src.is_some_and(plain_point) && ctx.shape_ref.get().is_none();
            (Seg::Move(q), m)
        } else {
            return None;
        };
        match seg {
            Seg::Cycle => (last, cur) = (sub_start, sub_start),
            Seg::Arc { to, .. } => (last, cur) = (to, to),
            Seg::Curve(q, _) => (last, cur) = (q[2], q[2]),
            Seg::Line(q) | Seg::Hv(q) | Seg::Vh(q) | Seg::Move(q) | Seg::Rect(q) | Seg::Grid(q, _) => {
                cur = at_of(q);
                if moves {
                    last = cur;
                }
            }
            Seg::Circle(..) => {}
        }
        if let Seg::Move(q) | Seg::Rect(q) = seg {
            sub_start = at_of(q);
        }
        p.segs.push((seg, moves));
        if let Some(k) = ctx.shape_ref.take() {
            refs.push((p.segs.len(), k));
        }
    }
    // A point and its nodes alone (`\draw (p) node {..};`): pgf paints the lone moveto
    // (`m S`); `\path` writes nothing for it (the node list of `\node at (p)`, probe).
    let mut bare_names = 0;
    if !edges.is_empty() {
        // Edges from one point (a node's name, or a coordinate's moveto): the path itself is
        // that point only.
        // Or several such names, each a moveto (`(a) edge (b) (b) edge (c)`).
        let names = refs.len();
        if !on_path.is_empty() || !p.segs.is_empty() && (names != p.segs.len() + 1 || p.segs.iter().any(|(s, _)| !matches!(s, Seg::Move(_)))) {
            return None;
        }
        p.bare = names > 0;
        bare_names = refs.iter().map(|r| ctx.nodes[r.1].scan_space()).sum();
        refs.clear();
        // Its tips are its edges' (taken above); the point itself has none.
        p.arrows = [false; 2];
        p.edges = edges;
    } else if undrawn_opts || (p.segs.is_empty() || !(p.draw || p.fill)) && on_path.is_empty() || !(p.draw || p.fill) && !p.segs.is_empty() {
        return None;
    }
    // A bare name's shape scan typesets its space (half of what a border's takes; a
    // circle's none).
    p.spaces = ctx.spaces.take() + bare_names;
    // The path's points, nodes' on their borders: a segment's timer runs between them
    // (measured: from a start node's border, not its centre).
    let pts = |p: &PathCmd| -> Option<Vec<(i32, i32)>> {
        let mut v = vec![if let Pt::At(x, y) = p.start { (x, y) } else { return None }];
        for (s, m) in &p.segs {
            match s {
                Seg::Line(Pt::At(x, y)) if *m => v.push((*x, *y)),
                Seg::Curve(q, _) if *m => v.push(q[2]),
                // A circle leaves `\tikz@lastx` at its centre: a node after it sits there.
                Seg::Circle(..) => v.push(*v.last()?),
                _ => return None,
            }
        }
        Some(v)
    };
    if !on_path.is_empty() {
        pts(&p)?;
    }
    if !refs.is_empty() {
        borders(&mut p, &refs, &ctx.nodes)?;
    }
    if !on_path.is_empty() {
        // Nodes of a path not moved, in its scope (its options' width and colour, not its
        // opacity); each on a segment's line, or at a point that is no node.
        // A lone point's nodes in a transformed picture sit as `\node at` there does (its
        // point through the picture's matrix).
        let lone = p.segs.is_empty() && p.tf == ctx.tf;
        if p.opts.iter().any(|o| matches!(o, PathOpt::Opacity(..))) || p.tf != Tf::ID && !lone || p.unskips != 0 {
            return None;
        }
        let after = pts(&p)?;
        for (mut n, k, mid) in on_path {
            if p.tf != Tf::ID && (n.nudged() || n.timed()) {
                return None;
            }
            n.in_path_scope(&p.opts, p.base_width)?;
            if n.nudged() {
                if mid {
                    return None;
                }
                p.nudges.push(k);
            }
            if mid || n.timed() {
                // `\tikz@timer@line`: from the point before to the segment's end (a circle's
                // timer is not a line's).
                if k == 0 || matches!(p.segs[k - 1].0, Seg::Circle(..)) {
                    return None;
                }
                match p.segs[k - 1].0 {
                    // `\tikz@timer@curve`: `\pgfpointcurveattime`.
                    Seg::Curve(q, _) => n.put_curve(after[k - 1], q, mid),
                    _ => n.put(after[k - 1], after[k], mid),
                }
            } else {
                if refs.iter().any(|r| r.0 == k) {
                    return None;
                }
                n.put(after[k], after[k], false);
            }
            if p.tf != Tf::ID {
                let Pt::At(x, y) = n.at else { return None };
                let (x, y) = p.tf.apply(x, y);
                n.at = Pt::At(x, y);
            }
            p.nodes.push(n);
        }
    }
    // `current page` points only in an overlay picture, only on a rectangle's corners or a
    // line; rounded corners only on rectangles of picture points, a cycle only after a line.
    let page = |q: &Pt| matches!(q, Pt::Page(_));
    let mut any_page = page(&p.start);
    for (k, (s, _)) in p.segs.iter().enumerate() {
        match s {
            Seg::Line(q) | Seg::Rect(q) => any_page |= page(q),
            Seg::Cycle if k == 0 || !matches!(p.segs[k - 1].0, Seg::Line(_) | Seg::Arc { .. }) => return None,
            _ => {}
        }
    }
    // Rounded corners on rectangles, or on one run of straight lines (closed by `cycle` or not).
    if p.round != 0 && !p.segs.iter().all(|(s, _)| matches!(s, Seg::Rect(_))) && round_polyline(&p).is_none() {
        return None;
    }
    // Tips only on a stroked path of straight lines whose end segments are longer than the
    // tips take off (`\pgf@arrows@the@lineend`).
    // The picture's tips on a path not stroked, or ending closed or on a moveto (a rectangle,
    // a circle, a grid): none drawn (probe).
    if !p.own_arrows && (!p.draw || matches!(p.segs.last(), None | Some((Seg::Rect(_) | Seg::Cycle | Seg::Circle(..) | Seg::Grid(..), _)))) {
        p.arrows = [false; 2];
    }
    // Nor any shortening (`\pgfprocesscheckclosed`, `tips=on proper draw`): the picture's
    // `shorten >` leaves its circles and rectangles whole.
    if !p.draw || matches!(p.segs.last(), None | Some((Seg::Rect(_) | Seg::Cycle | Seg::Circle(..) | Seg::Grid(..), _))) {
        p.shorten = [0; 2];
    }
    if p.arrows != [false; 2] || p.shorten != [0; 2] {
        if !p.draw || p.round != 0 {
            return None;
        }
        let [first, last] = tip_ends(&p)?;
        let n = if p.segs.iter().all(|(s, _)| matches!(s, Seg::Line(_))) { p.segs.len() + 1 } else { 3 };
        let ext = tip_extends(&p).0;
        let (l0, l1) = (reach(first.0, first.1), reach(last.0, last.1));
        let take = |k: usize| ext * p.arrows[k] as i32 + p.shorten[k];
        if l0 <= take(0) + if n == 2 { take(1) } else { 0 } || l1 <= take(1) + if n == 2 { take(0) } else { 0 } {
            return None;
        }
    }
    // A turned matrix: circles (`\pgfpathellipse` with turned axes) and rounded corners
    // (cut along turned sides) are not taken.
    if p.tf.turns() && (p.round != 0 || p.segs.iter().any(|(s, _)| matches!(s, Seg::Circle(..)))) {
        return None;
    }
    let moved = p.tf != Tf::ID;
    if any_page && (!overlay || moved || p.round != 0 || p.segs.iter().any(|(s, m)| !m || !matches!(s, Seg::Line(_) | Seg::Rect(_)))) {
        return None;
    }
    Some(p)
}

/// A path whose first or last point is a named node: tikz's `\iftikz@shapeborder` points
/// (tikz.code.tex:2647-2840). The line to a node ends on its border toward the last point
/// (the start node's centre), then the start's moveto waits no more: on its border toward
/// that end. Each border typesets two spaces (the `\pgfpointborderrectangle` call the
/// rectangle's `\anchorborder` writes with a line end between its arguments, and the
/// shape scan's). Only straight lines and nodes at the ends.
fn borders(p: &mut PathCmd, refs: &[(usize, usize)], nodes: &[node::Shape]) -> Option<()> {
    let n = p.segs.len();
    if p.segs.iter().any(|(s, m)| !m || !matches!(s, Seg::Line(_))) || refs.iter().any(|&(i, _)| i != 0 && i != n) || p.tf != Tf::ID {
        return None;
    }
    let pt = |q: &Pt| if let Pt::At(x, y) = *q { Some((x, y)) } else { None };
    let start_node = refs.iter().find(|r| r.0 == 0).map(|r| &nodes[r.1]);
    let end_node = refs.iter().find(|r| r.0 == n).map(|r| &nodes[r.1]);
    let before_end = if n == 1 { pt(&p.start)? } else { pt(if let Seg::Line(q) = &p.segs[n - 2].0 { q } else { return None })? };
    if let Some(e) = end_node {
        let end = e.border(before_end)?;
        p.segs[n - 1].0 = Seg::Line(Pt::At(end.0, end.1));
        p.spaces += e.border_spaces();
    }
    if let Some(s) = start_node {
        let toward = pt(if let Seg::Line(q) = &p.segs[0].0 { q } else { return None })?;
        let start = s.border(toward)?;
        p.start = Pt::At(start.0, start.1);
        p.spaces += s.border_spaces();
    }
    Some(())
}

/// The path's last line width (`\pgflinewidth` after its options), the picture's without one.
fn line_width(p: &PathCmd) -> i32 {
    p.opts.iter().rev().find_map(|o| if let PathOpt::Width(w) = o { Some(*w) } else { None }).unwrap_or(p.base_width)
}

/// The canvas points of a path of straight lines to picture points (`--`, `-|`, `|-`): its
/// start, each corner and each end, transformed as pgf protocols them.
fn polyline(p: &PathCmd) -> Option<Vec<(i32, i32)>> {
    points(p, &p.segs)
}

/// The ends a path's tips sit on, each with the point it points away from
/// (pgfcorepathusage.code.tex `\pgf@prep@straightend`: the second last point, a curve's
/// support, the one before when they coincide; tips are not precise here, so a curve's end
/// moves along that line and its supports stay): the start, then the end. Straight lines, or
/// curves and lines under the identity.
fn tip_ends(p: &PathCmd) -> Option<[TipEnd; 2]> {
    if p.segs.iter().all(|(s, _)| matches!(s, Seg::Line(_) | Seg::Hv(_) | Seg::Vh(_))) {
        let pts = polyline(p)?;
        let n = pts.len();
        if n < 2 {
            return None;
        }
        return Some([(pts[0], pts[1]), (pts[n - 1], pts[n - 2])]);
    }
    if p.tf != Tf::ID || p.segs.iter().any(|(s, m)| !*m || !matches!(s, Seg::Line(Pt::At(..)) | Seg::Curve(..))) {
        return None;
    }
    let Pt::At(x, y) = p.start else { return None };
    let start = (x, y);
    // The points along the path: each segment's supports and end.
    let mut v = vec![start];
    for (s, _) in &p.segs {
        match s {
            Seg::Line(Pt::At(x, y)) => v.push((*x, *y)),
            Seg::Curve(q, _) => v.extend_from_slice(q),
            _ => return None,
        }
    }
    let n = v.len();
    let away = |end: (i32, i32), rest: &mut dyn Iterator<Item = (i32, i32)>| rest.take(3).find(|q| *q != end);
    let s_ref = away(start, &mut v[1..].iter().copied())?;
    let e_ref = away(v[n - 1], &mut v[..n - 1].iter().rev().copied())?;
    Some([(start, s_ref), (v[n - 1], e_ref)])
}

/// [`polyline`] of the operations `segs`.
fn points(p: &PathCmd, segs: &[(Seg, bool)]) -> Option<Vec<(i32, i32)>> {
    let Pt::At(x, y) = p.start else { return None };
    let mut user = vec![(x, y)];
    for (s, _) in segs {
        let (q, hv) = match s {
            Seg::Line(q) => (q, None),
            Seg::Hv(q) => (q, Some(true)),
            Seg::Vh(q) => (q, Some(false)),
            _ => return None,
        };
        let Pt::At(x1, y1) = *q else { return None };
        let (x0, y0) = user[user.len() - 1];
        if let Some(h) = hv {
            user.push(if h { (x1, y0) } else { (x0, y1) });
        }
        user.push((x1, y1));
    }
    Some(user.into_iter().map(|(x, y)| p.tf.apply(x, y)).collect())
}

/// `plot[smooth] coordinates {(p1)(p2)..}` at a path's start (pgflibraryplothandlers.code.tex
/// `\pgfplothandlercurveto`, tension 0.5: `\pgf@plottension` `0.13875`): the first point a
/// moveto; then for each point after the second, the difference from the point two back
/// times the tension, the curve to the middle point ending against it, the next curve
/// starting along it; the last curve's second control its end. Points of the picture (no
/// relative ones); `n - 2` spaces (probe).
fn smooth_plot(c: &mut Cursor, ctx: &Ctx) -> Option<((i32, i32), Vec<Seg>)> {
    c.eat("plot");
    c.blanks()?;
    if !c.eat("[") || c.bracketed()?.trim() != "smooth" {
        return None;
    }
    c.blanks()?;
    if !c.eat("coordinates") {
        return None;
    }
    c.blanks()?;
    if !c.eat("{") {
        return None;
    }
    let mut pts = Vec::new();
    loop {
        c.blanks()?;
        if c.eat("}") {
            break;
        }
        if !c.s.starts_with('(') {
            return None;
        }
        let (Pt::At(x, y), _) = point(c, ctx, (0, 0))? else { return None };
        if ctx.shape_ref.get().is_some() {
            return None;
        }
        pts.push((x, y));
    }
    if pts.len() < 2 || pts.len() > 1000 {
        return None;
    }
    let t = |v: i32| times_frac(b"13875", v);
    let (mut first, mut support, mut second) = (pts[0], pts[0], pts[1]);
    let mut segs = Vec::new();
    for &cur in &pts[2..] {
        let (dx, dy) = (t(cur.0 - first.0), t(cur.1 - first.1));
        segs.push(Seg::Curve([support, (second.0 - dx, second.1 - dy), second], 0));
        support = (second.0 + dx, second.1 + dy);
        (first, second) = (second, cur);
    }
    segs.push(Seg::Curve([support, second, second], 0));
    if let Some(Seg::Curve(_, s)) = segs.first_mut() {
        *s = pts.len() as u32 - 2;
    }
    Some((pts[0], segs))
}

/// How far `b` is from `a` along the longer axis.
fn reach(a: (i32, i32), b: (i32, i32)) -> i32 {
    (b.0 - a.0).abs().max((b.1 - a.1).abs())
}

/// A point written `(x,y)` or `(a:r)`, absolute.
fn plain_point(src: &str) -> bool {
    src.starts_with('(') && src.contains([',', ':']) && !src.contains(['$', '['])
}

/// `\tikz@to@compute@distance@main` (tikzlibrarytopaths.code.tex:179): the vector's
/// components made positive, its longer normalised component over 255 dividing 16 times the
/// matching one (TeX's truncating `\divide`), 16 times that, then `0.3915` of it (before the
/// looseness and the distances' bounds).
fn to_reach(a: (i32, i32), b: (i32, i32)) -> Option<i32> {
    let (dx, dy) = ((b.0.checked_sub(a.0)?).abs(), (b.1.checked_sub(a.1)?).abs());
    let (px, py) = normalised(dx, dy);
    let (k, v) = if px > py { (px, dx) } else { (py, dy) };
    let xa = if k == 0 {
        dx
    } else {
        let k = k / 255;
        if k == 0 {
            return None;
        }
        16i32.checked_mul(16i32.checked_mul(v)? / k)?
    };
    // `\pgf@x=0.3915\pgf@xa`: the factor's `round_decimals` is 25657.
    Some(times_frac(b"3915", xa))
}

/// A tip's end and the point it points away from.
type TipEnd = ((i32, i32), (i32, i32));

/// A `to`'s curve: its start, its supports and end, the spaces its macros leave.
type Curve = ((i32, i32), [(i32, i32); 3], u32);

/// An end of a `to`: a point of the picture or a node (its index).
#[derive(Clone, Copy)]
enum End {
    At((i32, i32)),
    Node(usize),
}

/// The control points of `to[<options>]` from `a` to `b` and the spaces its macros leave:
/// `out`/`in` (`\tikz@to@compute`: `([shift=({out}:d)]a)`, `([shift=({in}:d)]b)`; two
/// `\tikz@to@modify` line ends and the `\edef`'s, probes o4, q1, q2) or `bend left`, `bend
/// right` (`\tikz@to@compute@relative`: the polar points through `\pgftransformcm` of the
/// normalised vector from `a` to `b`, shifted to each end; its two groups' line ends, one more
/// for each of `bend right`'s two `\pgfmathsetmacro` lines, probes o5, q3).
fn to_curve(opts: &str, ea: End, eb: End, nodes: &[node::Shape]) -> Option<Curve> {
    let deg = |v: i32| v << 16;
    let (mut out, mut inn, mut relative, mut spaces) = (deg(45), deg(135), false, 0);
    let (mut set_out, mut set_in) = (false, false);
    // `looseness` (a factor as `\pgfmathsetmacro` prints it), `min distance`.
    let (mut loose, mut min) = (1 << 16, 0);
    let mut looped = false;
    for item in key_items(opts) {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let (k, v) = match item.split_once('=') {
            Some((k, v)) => (k.trim(), Some(v.trim())),
            None => (item, None),
        };
        match (k, v) {
            // An edge's labels have its loop's side already.
            (EDGE_MARK, None) => {}
            ("out", Some(v)) => (out, set_out) = (angle(v)?, true),
            ("looseness", Some(v)) => loose = angle(v).filter(|l| *l > 0)?,
            ("min distance", Some(v)) => min = length(v)?,
            // tikzlibrarytopaths.code.tex:356: `loop <side>` is `<side>,out=..,in=..,loop`
            // (the side a placement for its labels, taken by the caller); `loop` makes the
            // target the start and puts in `[looseness=8,min distance=5mm,every loop]` after
            // the options (`every loop`'s `->,shorten >=1pt` the caller's too).
            ("loop", None) => looped = true,
            ("loop above", None) | ("loop below", None) | ("loop left", None) | ("loop right", None) => {
                let (o, i) = match k {
                    "loop above" => (105, 75),
                    "loop below" => (285, 255),
                    "loop left" => (195, 165),
                    _ => (15, -15),
                };
                (out, inn, set_out, set_in, looped) = (deg(o), deg(i), true, true, true);
            }
            ("in", Some(v)) => (inn, set_in) = (angle(v)?, true),
            ("bend left", v) | ("bend right", v) => {
                // `\pgfmathsetmacro\tikz@to@bend{#1}` (whole degrees here), default 30.
                let bend = match v {
                    Some(v) => deg(v.parse::<i32>().ok().filter(|d| d.abs() <= 360)?),
                    None => deg(30),
                };
                out = if k == "bend left" { bend } else { -bend };
                inn = deg(180) - out;
                relative = true;
                if k == "bend right" {
                    spaces += 2;
                }
            }
            _ => return None,
        }
    }
    if relative && (set_out || set_in) || !relative && !(set_out && set_in) {
        return None;
    }
    let shape = |e: End| if let End::Node(k) = e { nodes.get(k) } else { None };
    let centre = |e: End| match e {
        End::At(q) => Some(q),
        End::Node(k) => Some(nodes.get(k)?.anchor("center")?.0),
    };
    let (ca, cb) = (centre(ea)?, centre(eb)?);
    // With a node: the spaces of the borders as a line between the ends takes and one more,
    // and, measured against that line (probes w..), `out`/`in` two, `bend left` three and one
    // for a rectangle at the start, `bend right` two more.
    let with_node = matches!(ea, End::Node(_)) || matches!(eb, End::Node(_));
    let rect = |e: End| shape(e).map_or(0, |s| s.border_spaces());
    let node_spaces = rect(ea) + rect(eb) + 1 + if relative { spaces + 3 + rect(ea) / 2 } else { 2 };
    let (first, second) = if relative {
        // `\tikz@@@to@compute@relative`: each node's border toward the other's centre turned by
        // `out` about it (by 180 and `in` at the target).
        let first = match shape(ea) {
            Some(s) => s.border(Tf::ID.shift(ca.0, ca.1).rotate(out).shift(-ca.0, -ca.1).apply(cb.0, cb.1))?,
            None => ca,
        };
        let second = match shape(eb) {
            Some(s) => s.border(Tf::ID.shift(cb.0, cb.1).rotate(deg(180)).rotate(inn).shift(-cb.0, -cb.1).apply(ca.0, ca.1))?,
            None => cb,
        };
        (first, second)
    } else {
        // `\tikz@to@modify`: a node's `<name>.<out>` (`\pgf@sh@reanchor`: the border toward
        // `\pgfqpointpolar{<whole degrees>}{1pt}`).
        let first = match shape(ea) {
            Some(s) => s.border_angle(out >> 16),
            None => ca,
        };
        let second = match shape(eb) {
            Some(s) => s.border_angle(inn >> 16),
            None => cb,
        };
        (first, second)
    };
    if looped {
        (loose, min) = (8 << 16, length("5mm")?);
    }
    let x = to_reach(first, second)?;
    // `\pgf@xa=<looseness>\pgf@x`, then at least `min distance`, at most `10000pt`.
    let d = fmul(loose, x).max(min).min(10000 << 16);
    let pol = |ang: i32| (fmul(pgfmath::cos(ang), d), fmul(pgfmath::sin(ang), d));
    let (p1, p2) = (pol(out), pol(inn));
    let (c1, c2) = if relative {
        let (xc, yc) = normalised(second.0 - first.0, second.1 - first.1);
        let m = [xc, yc, -yc, xc];
        spaces += 2;
        (Tf::ID.shift(first.0, first.1).cm(m, 0, 0).apply(p1.0, p1.1), Tf::ID.shift(second.0, second.1).cm(m, 0, 0).apply(p2.0, p2.1))
    } else {
        spaces += 3;
        ((first.0.checked_add(p1.0)?, first.1.checked_add(p1.1)?), (second.0.checked_add(p2.0)?, second.1.checked_add(p2.1)?))
    };
    if with_node {
        spaces = node_spaces;
    }
    if relative {
        return Some((first, [c1, c2, second], spaces));
    }
    // `.. controls (c1) and (c2) .. (b)` after `(a)`: a node's border toward the control next
    // to it.
    let start = match shape(ea) {
        Some(s) => s.border(c1)?,
        None => first,
    };
    let end = match shape(eb) {
        Some(s) => s.border(c2)?,
        None => second,
    };
    Some((start, [c1, c2, end], spaces))
}

/// `\pgfpointnormalised` (pgfcorepoints.code.tex:944) of the vector: pgfmath's cosine and
/// sine of its `atan2`.
fn normalised(x: i32, y: i32) -> (i32, i32) {
    let a = pgfmath::atan2(y, x);
    (pgfmath::cos(a), pgfmath::sin(a))
}

/// The path's tip's shortening of the line (its tip end less its line end,
/// `\pgf@arrow@cont@short`), its tip end and its back end.
fn tip_extends(p: &PathCmd) -> (i32, i32, i32) {
    let w = line_width(p);
    let half = times_frac(b"5", w);
    match p.tip {
        Tip::Stealth => {
            let a = stealth_a(w);
            (5 * a, 5 * a, -3 * a)
        }
        // `\pgfarrowsleftextend{+-1\pgfutil@tempdima}`, `\pgfarrowsrightextend{+9..}`.
        Tip::OldLatex => {
            let a = stealth_a(w);
            (9 * a, 9 * a, -a)
        }
        // pgflibraryarrows.code.tex:522: `\pgfarrowsrightextend{2a+.5\pgflinewidth}`, left
        // `-(6a+.5\pgflinewidth)`.
        Tip::StealthPrime => {
            let a = stealth_a(w);
            (2 * a + half, 2 * a + half, -(6 * a + half))
        }
        Tip::Latex => {
            let t = Latex::new(w).length - half;
            (t, t, -half)
        }
        // Line end `-.5` of the line width.
        Tip::Cm => (2 * half, half, -Cm::new(w).a - half),
        Tip::To => (to_extend(w), to_extend(w), to_back(w)),
    }
}

/// A path's tips. `to` is `Cm` under arrows.meta (its `To /.tip = {Computer Modern
/// Rightarrow}`, pgflibraryarrows.meta.code.tex:1598, and pgfcorearrows.code.tex:1072
/// `> /.tip = To`).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Tip {
    To,
    Stealth,
    OldLatex,
    StealthPrime,
    Latex,
    Cm,
}

/// arrows.meta's `Computer Modern Rightarrow` at line width `w` with its defaults
/// (pgflibraryarrows.meta.code.tex:663): `length = +1.6pt 2.2`, `width' = +0pt 2.096774`,
/// `line width = 0pt 1 1`, `round`; `tempdima` the length less the line width, `tempdimb`
/// the width less it.
struct Cm {
    a: i32,
    b: i32,
}

impl Cm {
    fn new(w: i32) -> Cm {
        let length = number("1.6").unwrap_or(0) + fmul(number("2.2").unwrap_or(0), w);
        let width = fmul(number("2.096774").unwrap_or(0), length);
        Cm { a: length - w, b: width - w }
    }

    /// Its code (no dash, round cap and join, two curves stroked) in one protocol literal.
    fn code(&self) -> String {
        let (a, b) = (self.a, self.b);
        let k = |f: &str, v: i32| what::pgf_bp(fmul(number(f).unwrap_or(0), v));
        format!(
            "q [ ] 0.0 d 1 J 1 j {} {} m {} {} {} {} 0.0 0.0 c {} {} {} {} {} {} c S Q  ",
            what::pgf_bp(-a),
            k("0.5", b),
            k("-0.81731", a),
            k("0.2", b),
            k("-0.41019", a),
            k("0.05833333", b),
            k("-0.41019", a),
            k("-0.05833333", b),
            k("-0.81731", a),
            k("-0.2", b),
            what::pgf_bp(-a),
            k("-0.5", b)
        )
    }

    /// Its hull: the tip's two points, the upper ends' two and their mirror images.
    fn hull(&self, w: i32) -> [(i32, i32); 6] {
        let half = times_frac(b"5", w);
        let y = times_frac(b"5", self.b) + half;
        [(half, half), (half, -half), (-self.a + half, y), (-self.a + half, -y), (-self.a - half, y), (-self.a - half, -y)]
    }
}

/// arrows.meta's `Latex` at line width `w` with its defaults (pgflibraryarrows.meta.code.tex:
/// 809): `length = +3pt 4.5 .8`, `width' = +0pt .75`, `line width = +0pt 1 1`, so the
/// tip's line width is the path's (the cap at `.2` of the length is above it for every
/// width below 6pt); `tempdima` and `tempdimb` as its setup code computes them.
struct Latex {
    length: i32,
    width: i32,
    a: i32,
    b: i32,
}

impl Latex {
    fn new(w: i32) -> Latex {
        let half = |v: i32| times_frac(b"5", v);
        // `\pgfarrowslinewidthdependent`, `\pgfarrowslengthdependent`.
        let length = 3 * 65536 + fmul(4 * 65536 + 32768, w);
        let width = fmul(49152, length);
        // The front miter: `sqrt(9 (length / width)^2 + 1)` of the line width.
        let quot = pgfmath::divide(length, width);
        let x = 9 * fmul(quot, quot) + 65536;
        let xc = fmul(pgfmath::sqrt(x), w);
        let a = length - half(xc) - half(w);
        // The back miter: the normalised `(.3 length, .2333333 width)`, its `y + 1` over its `x`.
        let (nx, ny) = normalised(times_frac(b"3", length), times_frac(b"2333333", width));
        let b = -half(fmul(pgfmath::divide(ny + 65536, nx), w)) + half(width);
        Latex { length, width, a, b }
    }

    /// Its code (`\pgfsetdash{}{+0pt}`, `\pgfsetmiterjoin`, the line width the path's so not
    /// set, two curves and a line closed, `\pgfusepathqfillstroke`) in one protocol literal.
    fn code(&self) -> String {
        let (a, b) = (self.a, self.b);
        let k = |f: &str, v: i32| what::pgf_bp(fmul(number(f).unwrap_or(0), v));
        format!(
            "q [ ] 0.0 d 0 j {} 0.0 m {} {} {} {} 0.0 {} c 0.0 {} l {} {} {} {} {} 0.0 c h B Q  ",
            what::pgf_bp(a),
            k("0.877192", a),
            k("0.077922", b),
            k("0.337381", a),
            k("0.519480", b),
            what::pgf_bp(b),
            what::pgf_bp(-b),
            k("0.337381", a),
            k("-0.519480", b),
            k("0.877192", a),
            k("-0.077922", b),
            what::pgf_bp(a)
        )
    }

    /// Its convex hull (`\pgfarrowshullpoint`): the tip end, the back corners.
    fn hull(&self, w: i32) -> [(i32, i32); 3] {
        let half = |v: i32| times_frac(b"5", v);
        [(self.length - half(w), 0), (-half(w), half(self.width)), (-half(w), -half(self.width))]
    }
}

/// The `stealth` tip's `tempdima`: `0.28pt` advanced by `.3\pgflinewidth` (no inner line).
fn stealth_a(w: i32) -> i32 {
    18350 + times_frac(b"3", w)
}

/// The `stealth` tip's cached code (pgfcorearrows.code.tex:1129): a quadrilateral from
/// `5a` on the axis filled (`\pgfusepathqfill`), in one protocol literal like `to`'s.
fn stealth_tip(w: i32) -> String {
    let a = stealth_a(w);
    let b = what::pgf_bp;
    format!("q {} 0.0 m {} {} l 0.0 0.0 l {} {} l f Q  ", b(5 * a), b(-3 * a), b(4 * a), b(-3 * a), b(-4 * a))
}

/// The legacy `latex` tip's cached code (pgfcorearrows.code.tex:1209): `tempdima` as
/// `stealth`'s, two curves and a line from `9a` on the axis, filled, one protocol literal.
fn old_latex_tip(w: i32) -> String {
    let a = stealth_a(w);
    let k = |e: &str| what::pgf_bp(fmul(number(e).unwrap_or(0), a));
    let pt = |x: &str, y: &str| format!("{} {} ", k(x), k(y));
    let tip = what::pgf_bp(9 * a);
    format!("q {tip} 0.0 m {}{}{}c {}l {}{}{tip} 0.0 c f Q  ", pt("6.3333", ".5"), pt("2", "2"), pt("-1", "3.75"), pt("-1", "-3.75"), pt("2", "-2"), pt("6.3333", "-.5"))
}

/// The arrows library's `stealth'` cached code (pgflibraryarrows.code.tex:522): no dash,
/// round join, three curves from `2a` on the axis, closed, filled and stroked.
fn stealth_prime_tip(w: i32) -> String {
    let a = stealth_a(w);
    let k = |e: &str| what::pgf_bp(fmul(number(e).unwrap_or(0), a));
    let pt = |x: &str, y: &str| format!("{} {} ", k(x), k(y));
    format!("q [ ] 0.0 d 1 j {}m {}{}{}c {}{}{}c {}{}{}c h B Q  ", pt("2", "0"), pt("-.5", ".5"), pt("-3", "1.5"), pt("-6", "3.25"), pt("-3", "1"), pt("-3", "-1"), pt("-6", "-3.25"), pt("-3", "-1.5"), pt("-.5", "-.5"), pt("2", "0"))
}

/// The `to` tip's left extend (its back end) at line width `w`: `-0.84pt` advanced by
/// `-1.3\pgflinewidth`.
fn to_back(w: i32) -> i32 {
    -55050 - fmul(85197, w)
}

/// The `to` tip's right extend at line width `w` (pgfcorearrows.code.tex:1145,
/// `0.21pt` advanced by `.625\pgflinewidth`): what it takes off the line's end, and where its
/// origin stands back from the end.
fn to_extend(w: i32) -> i32 {
    13763 + times_frac(b"625", w)
}

/// The `to` tip's cached code (pgfcorearrows.code.tex:1152, instantiated with the transform
/// reset into one protocol literal, `\pgf@arrows@instantiate`): `tempdima` is `0.28pt`
/// advanced by `.3\pgflinewidth`, the line width `0.8\pgflinewidth`, no dash, round cap and
/// join, two curves stroked; each `\pgfsys@invoke` ends in a space and the literal adds one.
fn to_tip(w: i32) -> String {
    let t = 18350 + times_frac(b"3", w);
    let k = |e: &str| what::pgf_bp(fmul(number(e).unwrap_or(0), t));
    let pt = |x: &str, y: &str| format!("{} {} ", k(x), k(y));
    format!(
        "q {} w [ ] 0.0 d 1 J 1 j {}m {}{}{}c {}{}{}c S Q  ",
        what::pgf_bp(times_frac(b"8", w)),
        pt("-3", "4"),
        pt("-2.75", "2.5"),
        pt("0", "0.25"),
        pt("0.75", "0"),
        pt("0", "-0.25"),
        pt("-2.75", "-2.5"),
        pt("-3", "-4")
    )
}

/// A tip's placement (`\pgftransformarrow`, then `\pgflowlevelsynccm`): the rotation of its
/// direction `(c, s)` as `\pgf@sys@tonumber` prints it and its origin in bp.
fn tip_cm((c, s): (i32, i32), (x, y): (i32, i32)) -> String {
    let n = pgfmath::print;
    format!("{} {} {} {} {} {} cm ", n(c), n(s), n(-s), n(c), what::pgf_bp(x), what::pgf_bp(y))
}

/// The line widths whose `to` tip the document has instantiated (pgf's global
/// `pgf@@ar@to@<width>@...` names): a count, then each width.
const ARROW_WIDTHS: u32 = 32600;

/// The pre-scan's question: whether a picture's source text is of the grammar, every length
/// register 1pt, each macro parameter `#1`..`#9` (a picture in a `\newcommand`) a number or
/// a colour (`1` or `black`, every mix tried for up to four of them); the native job reads
/// the picture again with the arguments and refuses it then if need be.
/// Marks a `to` written for an `edge`: its labels carry the loop's side.
const EDGE_MARK: &str = "\u{1}";

/// The placement keys without a value (tikz.code.tex `\tikzoption{above}[]` and kin): an
/// anchor for the nodes that follow.
const EDGE_PLACES: [&str; 8] = ["above", "below", "left", "right", "above left", "above right", "below left", "below right"];

/// tikz's tree distances, 15mm (tikz.code.tex:1418).
const TREE_DISTANCE: i32 = 2797019;

/// The children of a tree's node `parent` at `level` (tikz.code.tex:4549 `\tikz@children`,
/// `\tikz@children@collected`): `level/.style`, `level <n>/.style` (distances only), then
/// each `child {node .. {..} <its children>}`: its wrapper, its node at [`node::child_point`],
/// its `edge from parent` (`\path[draw] (parent) -- (child)`, one space more for line 4666's
/// end, and the child's `center` anchor's spaces when it has children, whose wrappers go in
/// the edge path's figbox); `edge from parent [options] <labels>` written in the child.
/// Not: child options, `foreach`, `edge from parent` styled, `every child`, `grow` in a style.
fn children(c: &mut Cursor, ctx: &mut Ctx, items: &mut Vec<Item>, parent: &str, level: u32, overlay: bool) -> Option<()> {
    if ctx.styles.iter().any(|(k, _)| k.starts_with("edge from parent") || k.starts_with("every child") || k.starts_with("grow")) {
        return None;
    }
    let saved = ctx.tree;
    for key in ["level".to_string(), format!("level {level}")] {
        let Some(v) = ctx.styles.iter().rev().find(|(k, _)| *k == key).map(|(_, v)| v.clone()) else { continue };
        for k in key_items(&v).into_iter().map(str::trim).filter(|k| !k.is_empty()) {
            match k.split_once('=').map(|(a, b)| (a.trim(), b.trim())) {
                Some(("level distance", v)) => ctx.tree[0] = node::tree_length(v, ctx)?,
                Some(("sibling distance", v)) => ctx.tree[1] = node::tree_length(v, ctx)?,
                _ => return None,
            }
        }
    }
    let word = |c: &Cursor, w: &str| c.s.starts_with(w) && !c.s[w.len()..].starts_with(|ch: char| ch.is_ascii_alphabetic());
    let mut kids = Vec::new();
    loop {
        c.blanks()?;
        if !word(c, "child") {
            break;
        }
        c.eat("child");
        c.blanks()?;
        kids.push(braced(c)?);
    }
    let centre = ctx.nodes.iter().find(|s| s.name == parent)?.anchor("center")?.0;
    let n = kids.len() as i32;
    for (i, text) in kids.into_iter().enumerate() {
        let k = i as i32 + 1;
        let at = node::child_point(centre, k, n, ctx.tree)?;
        let mut cc = Cursor { s: text };
        cc.blanks()?;
        if !word(&cc, "node") {
            return None;
        }
        cc.eat("node");
        let spaces = 1 + node::child_forms(cc.s)?;
        let child = node::child(&mut cc, ctx, overlay, at, format!("{parent}-{k}"))?;
        let name = child.name.clone()?;
        items.push(Item::ChildBegin(spaces));
        items.push(Item::Node(child));
        cc.blanks()?;
        let grand = word(&cc, "child");
        let centre_spaces = if grand { ctx.nodes.iter().find(|s| s.name == name)?.anchor("center")?.1 } else { 0 };
        // The grandchildren (made before the edge, `\tikz@childnode` runs the edge path last).
        let mut inner = Vec::new();
        if grand {
            children(&mut cc, ctx, &mut inner, &name, level + 1, overlay)?;
            cc.blanks()?;
        }
        // `edge from parent [options] <nodes>` written last in the child: the edge path's
        // options, its nodes labels midway (`\tikz@edge@to@parent@path` then them).
        let (mut opts, mut labels) = ("", String::new());
        if cc.eat("edge from parent") {
            cc.blanks()?;
            if cc.eat("[") {
                opts = cc.bracketed()?;
                cc.blanks()?;
            }
            while word(&cc, "node") {
                let from = cc.s;
                cc.eat("node");
                cc.blanks()?;
                if cc.eat("[") {
                    cc.bracketed()?;
                    cc.blanks()?;
                }
                braced(&mut cc)?;
                labels.push_str(&from[..from.len() - cc.s.len()]);
                labels.push(' ');
                cc.blanks()?;
            }
        }
        if !cc.s.is_empty() {
            return None;
        }
        let e = PathCmd { fill: false, draw: true, opts: Vec::new(), round: 0, start: Pt::At(0, 0), segs: Vec::new(), tf: ctx.tf, unskips: 0, spaces: 0, arrows: ctx.arrows, own_arrows: false, tip: ctx.gt, nudges: Vec::new(), base_width: ctx.width, nodes: Vec::new(), step: [GRID_STEP; 2], edges: Vec::new(), edge: None, bare: false, shorten: ctx.shorten };
        // Labels on an edge whose child has children go elsewhere in its box (probe x4).
        if grand && !labels.is_empty() {
            return None;
        }
        // `draw=none`: the path paints nothing, its spaces stay, two more for its options
        // (probe x1).
        let hidden = opts.trim() == "draw=none";
        let text = if opts.trim().is_empty() || hidden { format!("({parent}) -- {labels}({name});") } else { format!("[{opts}] ({parent}) -- {labels}({name});") };
        let mut e = path(&mut Cursor { s: &text }, ctx, e, overlay)?;
        e.spaces += 1 + centre_spaces;
        if hidden {
            if !labels.is_empty() {
                return None;
            }
            items.extend((0..e.spaces + 2).map(|_| Item::Space));
        } else {
            // Its labels after it, as a path's nodes (`parse_with`'s).
            let labels = std::mem::take(&mut e.nodes);
            items.push(Item::Path(e));
            let n = labels.len();
            for (j, mut l) in labels.into_iter().enumerate() {
                l.on_path = Some((j == 0, j + 1 == n));
                l.path_node = true;
                items.push(Item::Node(l));
            }
            ctx.node_count += n;
        }
        if grand {
            items.push(Item::FigOpen);
            items.append(&mut inner);
            items.push(Item::FigClose);
        }
        items.push(Item::ChildEnd);
    }
    ctx.tree = saved;
    Some(())
}

pub(super) fn accepts(text: &str, styles: &[(String, String)]) -> bool {
    let b = text.as_bytes();
    let mut params: Vec<u8> = (0..b.len().saturating_sub(1)).filter(|&k| b[k] == b'#' && (b'1'..=b'9').contains(&b[k + 1])).map(|k| b[k + 1]).collect();
    params.sort_unstable();
    params.dedup();
    if params.len() > 4 {
        return false;
    }
    (0..1u32 << params.len()).any(|mix| {
        let mut t = String::with_capacity(text.len());
        let mut it = text.chars().peekable();
        while let Some(c) = it.next() {
            match it.peek().and_then(|d| params.iter().position(|p| *p as char == *d)) {
                Some(i) if c == '#' => {
                    it.next();
                    t.push_str(if mix >> i & 1 == 0 { "1" } else { "black" });
                }
                _ => t.push(c),
            }
        }
        parse_with(&t, &|_| Some(1 << 16), &|_| [0; 3], 0, styles).is_some()
    })
}

/// Reads a picture's text after `\begin{tikzpicture}` up to (not with) `\end{tikzpicture}`;
/// `None` when it is not of the grammar this module draws.
#[cfg(test)]
pub(super) fn parse(text: &str, dims: &dyn Fn(&str) -> Option<i32>) -> Option<Picture> {
    parse_with(text, dims, &|_| [0; 3], 0, &[])
}

/// [`parse`] with the nodes' text boxes and the font's quad known: named nodes' rectangles
/// are those that will be drawn.
pub(super) fn parse_with(text: &str, dims: &dyn Fn(&str) -> Option<i32>, boxes: &dyn Fn(usize) -> [i32; 3], em: i32, styles: &[(String, String)]) -> Option<Picture> {
    let mut ctx = Ctx { dims, names: Vec::new(), unit: [UNIT; 2], tf: Tf::ID, boxes, em, nodes: Vec::new(), node_count: 0, distance: [UNIT; 2], width: 26214, arrows: [false; 2], gt: Tip::To, styles: styles.to_vec(), spaces: Default::default(), shape_ref: Default::default(), named: Default::default(), tree: [TREE_DISTANCE, TREE_DISTANCE, -90], shorten: [0; 2], auto: false };
    let clean = strip_comments(text)?;
    let mut c = Cursor { s: &clean };
    let mut baseline: Option<String> = None;
    // `\tikzpicture`'s `[#1]`, right after the name: none, or exactly `remember picture` and
    // `overlay`. Without it, `\pgfutil@ifnextchar[` skips the blanks.
    // Or `x=<length>`, `y=<length>` (`\pgfsetxvec`), `scale=<number>`, in written order.
    let mut widths = Vec::new();
    // `\scope[every picture,<options>]` (tikz.code.tex `\tikz@picture`): the preamble's
    // `every picture` keys before the picture's own.
    let own = if c.eat("[") { Some(c.bracketed()?) } else { None };
    let every = ctx.styles.iter().rev().find(|(k, _)| k == "every picture").map(|(_, v)| v.clone());
    let (overlay, remember, space) = if own.is_some() || every.is_some() {
        let list = format!("{},{}", every.unwrap_or_default(), own.unwrap_or(""));
        let (mut remember, mut overlay, mut shape_tf) = (false, false, false);
        for k in key_items(&list).into_iter().map(str::trim).filter(|k| !k.is_empty()) {
            match k.split_once('=').map(|(a, b)| (a.trim(), b)) {
                None if k == "remember picture" => remember = true,
                None if k == "overlay" => overlay = true,
                // `\tikz@semiaddlinewidth` in the picture's scope.
                // Each a `\pgfsetlinewidth`, so a `w` literal (probe: `[very thick,line width=1.5pt]`
                // writes both, `[thin]` its `0.3985 w`).
                None if WIDTHS.iter().any(|(n, _)| *n == k) => {
                    ctx.width = length(WIDTHS.iter().find(|(n, _)| *n == k)?.1)?;
                    widths.push(ctx.width);
                }
                Some(("line width", v)) => {
                    ctx.width = width_value(v)?;
                    widths.push(ctx.width);
                }
                None if k == "->" => ctx.arrows = [false, true],
                None if k == "<-" => ctx.arrows = [true, false],
                None if k == "<->" => ctx.arrows = [true, true],
                None if k == "-" => ctx.arrows = [false; 2],
                Some(("shorten <", v)) => ctx.shorten[0] = length(v).filter(|l| *l >= 0)?,
                Some(("shorten >", v)) => ctx.shorten[1] = length(v).filter(|l| *l >= 0)?,
                // tikz.code.tex `\tikzoption{auto}[]`: defines how a label is anchored,
                // typesets nothing.
                None if k == "auto" => ctx.auto = true,
                // `>=Latex` (arrows.meta) before any tips in the list; the others anywhere (the tip
                // `>` is named in the sequence, made when a path is drawn: probe a1).
                // `>=stealth`, `>=latex` (pgfcorearrows' own tips): `\pgfdeclarearrow{name=<->,
                // means=..}` (tikz.code.tex:1195), instantiated under their own names.
                Some((">", v)) if matches!(v.trim(), "stealth" | "latex" | "stealth'") || ctx.arrows == [false; 2] && v.trim() == "Latex" => {
                    ctx.gt = match v.trim() {
                        "Latex" => Tip::Latex,
                        "stealth" => Tip::Stealth,
                        "stealth'" => Tip::StealthPrime,
                        _ => Tip::OldLatex,
                    }
                }
                // tikz.code.tex:88 `\tikz@baseline@coordinate`: a node's anchor, read at the end.
                // :91 `\tikz@baseline@simple`, a length: `\pgfsetbaseline{#1}`, the point
                // `\pgfpoint{0pt}{#1}` read at the end (pgfcorescopes.code.tex:373).
                Some(("baseline", v)) => baseline = Some(match v.trim().strip_prefix('(') {
                    Some(r) => r.strip_suffix(')')?.to_string(),
                    None => format!("\u{1}{}", v.trim()),
                }),
                None if k == "baseline" => baseline = Some("\u{1}0pt".to_string()),
                // `<name>/.style={<keys>}`: no space typeset (probe).
                Some((n, v)) if n.ends_with("/.style") => ctx.styles.push(style_def(n, v)?),
                Some(("x", v)) => ctx.unit[0] = length(v)?,
                Some(("y", v)) => ctx.unit[1] = length(v)?,
                Some(("scale", v)) => ctx.tf = ctx.tf.scale(number(v)?),
                // tikz.code.tex:321, 323: `\tikz@addtransform{\pgftransformxscale{#1}}`.
                Some(("xscale", v)) => ctx.tf = ctx.tf.scale_axis(number(v)?, false),
                Some(("yscale", v)) => ctx.tf = ctx.tf.scale_axis(number(v)?, true),
                Some(("rotate", v)) => ctx.tf = ctx.tf.rotate(angle(v)?),
                // `\tikz@lib@place@parse@nums`: `<y> and <x>`, or one length for both.
                Some(("node distance", v)) => {
                    ctx.distance = match v.split_once(" and ") {
                        Some((y, x)) => [length(x)?, length(y)?],
                        None => [length(v)?; 2],
                    };
                }
                Some(("level distance", v)) => ctx.tree[0] = node::tree_length(v, &ctx)?,
                Some(("sibling distance", v)) => ctx.tree[1] = node::tree_length(v, &ctx)?,
                // `\tikz@set@growth` (tikz.code.tex:1397): a direction's angle or whole degrees,
                // set at level 0, so no level of children is `\tikz@special@level`.
                Some(("grow", v)) => ctx.tree[2] = node::grow_angle(v)?,
                // Nodes under the identity: the same with it (probes ts0, ts1).
                None if k == "transform shape" => shape_tf = true,
                _ => return None,
            }
        }
        // `remember picture` only with `overlay`; `overlay` alone without `current page`.
        if remember && !overlay || overlay && !remember && c.s.contains("current page") || shape_tf && ctx.tf != Tf::ID {
            return None;
        }
        (overlay, remember, own.is_some() && c.blanks()?)
    } else {
        (false, false, false)
    };
    let mut items = Vec::new();
    // The open scopes: what each `\end{scope}` puts back.
    let mut scopes = Vec::new();
    // The text `\foreach` loops are unrolled into, read on from where each loop stood.
    let mut unrolled;
    let mut budget = 1 << 20;
    loop {
        // Between commands a blank line's `\par` does nothing in the picture's box (restricted
        // horizontal mode, tex.web §1094 `end_graf`): skipped, raw or as the machine shows it
        // (probes x2, w1-w6).
        loop {
            c.s = c.s.trim_start_matches([' ', '\t', '\n', '\r']);
            if !c.s.starts_with("\\par") || c.s[4..].starts_with(|ch: char| ch.is_ascii_alphabetic()) {
                break;
            }
            c.eat("\\par");
        }
        if c.s.is_empty() {
            break;
        }
        if c.s.starts_with("\\foreach") && !c.s[8..].starts_with(|ch: char| ch.is_ascii_alphabetic()) {
            let next = foreach(c.s, &mut budget)?;
            unrolled = next;
            c = Cursor { s: &unrolled };
            continue;
        }
        if let Some(after) = c.s.strip_prefix("\\pgfmathtruncatemacro").filter(|a| !a.starts_with(|ch: char| ch.is_ascii_alphabetic())) {
            let (next, space) = truncate_macro(after, &ctx)?;
            if space {
                items.push(Item::Space);
            }
            unrolled = next;
            c = Cursor { s: &unrolled };
            continue;
        }
        if c.eat(SPACE) {
            items.push(Item::Space);
            continue;
        }
        // `\centering` in the picture: assignments local to it, which no node sees (a
        // text width's minipage runs `\@parboxrestore`); not with `\\` in the picture.
        if c.s.starts_with("\\centering") && !c.s[10..].starts_with(|ch: char| ch.is_ascii_alphabetic()) && !text.contains("\\\\") {
            c.eat("\\centering");
            continue;
        }
        // `\tikzset{<name>/.style={..},..}`, `\tikzstyle{<name>}=[..]` in the picture: pgfkeys'
        // macros, local to the picture or scope, typesetting nothing; a space after the group
        // is the source's own (probes s4-s8).
        if let Some(after) = c.s.strip_prefix("\\tikzset").filter(|a| !a.starts_with(|ch: char| ch.is_ascii_alphabetic())) {
            let after = after.trim_start_matches([' ', '\n']);
            let k = tikzset_call_len(after)?;
            ctx.styles.extend(tikzset_styles(&after[1..k - 1])?);
            c.s = &after[k..];
            if c.s.starts_with([' ', '\n']) {
                items.push(Item::Space);
            }
            continue;
        }
        if let Some(after) = c.s.strip_prefix("\\tikzstyle").filter(|a| !a.starts_with(|ch: char| ch.is_ascii_alphabetic())) {
            let after = after.trim_start_matches([' ', '\n']);
            let (k, def) = tikzstyle_call(after)?;
            ctx.styles.push(def);
            c.s = &after[k..];
            if c.s.starts_with([' ', '\n']) {
                items.push(Item::Space);
            }
            continue;
        }
        if c.eat("\\node") {
            let mut n = node::parse(&mut c, &mut ctx, overlay)?;
            let labels = std::mem::take(&mut n.label_nodes);
            let root = n.tree.then(|| n.name.clone()).flatten();
            items.push(Item::Node(n));
            items.extend(labels.into_iter().map(Item::Node));
            if let Some(root) = root {
                children(&mut c, &mut ctx, &mut items, &root, 1, overlay)?;
                // The root's figbox, opened after its node box.
                items.push(Item::FigClose);
                c.blanks()?;
                if !c.eat(";") {
                    return None;
                }
            }
            continue;
        }
        // `\begin{scope}[options]`: tikz.code.tex `\scope`, its options local to it. Of the
        // picture's options: units, shifts, scales, widths, tips, styles.
        if c.eat("\\begin{scope}") {
            scopes.push((ctx.unit, ctx.tf, ctx.width, ctx.arrows, ctx.gt, ctx.styles.len(), ctx.distance, ctx.shorten));
            let mut widths = Vec::new();
            let mut space = false;
            if c.eat("[") {
                for k in key_items(c.bracketed()?).into_iter().map(str::trim).filter(|k| !k.is_empty()) {
                    match k.split_once('=').map(|(a, b)| (a.trim(), b)) {
                        None if WIDTHS.iter().any(|(n, _)| *n == k) => {
                            ctx.width = length(WIDTHS.iter().find(|(n, _)| *n == k)?.1)?;
                            widths.push(ctx.width);
                        }
                        Some(("line width", v)) => {
                            ctx.width = width_value(v)?;
                            widths.push(ctx.width);
                        }
                        None if k == "->" => ctx.arrows = [false, true],
                        None if k == "<-" => ctx.arrows = [true, false],
                        None if k == "<->" => ctx.arrows = [true, true],
                        None if k == "-" => ctx.arrows = [false; 2],
                        Some(("shorten <", v)) => ctx.shorten[0] = length(v).filter(|l| *l >= 0)?,
                        Some(("shorten >", v)) => ctx.shorten[1] = length(v).filter(|l| *l >= 0)?,
                        Some((n, v)) if n.ends_with("/.style") => ctx.styles.push(style_def(n, v)?),
                        Some(("x", v)) => ctx.unit[0] = length(v)?,
                        Some(("y", v)) => ctx.unit[1] = length(v)?,
                        Some(("xshift", v)) => ctx.tf = ctx.tf.shift(length(v)?, 0),
                        Some(("yshift", v)) => ctx.tf = ctx.tf.shift(0, length(v)?),
                        Some(("scale", v)) => ctx.tf = ctx.tf.scale(number(v)?),
                        Some(("xscale", v)) => ctx.tf = ctx.tf.scale_axis(number(v)?, false),
                        Some(("yscale", v)) => ctx.tf = ctx.tf.scale_axis(number(v)?, true),
                        Some(("rotate", v)) => ctx.tf = ctx.tf.rotate(angle(v)?),
                        Some(("node distance", v)) => {
                            ctx.distance = match v.split_once(" and ") {
                                Some((y, x)) => [length(x)?, length(y)?],
                                None => [length(v)?; 2],
                            };
                        }
                        _ => return None,
                    }
                }
                space = c.blanks()?;
            }
            items.push(Item::ScopeBegin(widths, space));
            continue;
        }
        if c.eat("\\end{scope}") {
            let (unit, tf, width, arrows, gt, styles, distance, shorten) = scopes.pop()?;
            (ctx.unit, ctx.tf, ctx.width, ctx.arrows, ctx.gt, ctx.distance, ctx.shorten) = (unit, tf, width, arrows, gt, distance, shorten);
            ctx.styles.truncate(styles);
            items.push(Item::ScopeEnd(c.blanks()?));
            continue;
        }
        // `\coordinate (name) at <point>;`: a node of shape `coordinate`, of no size.
        if c.eat("\\coordinate") {
            c.blanks()?;
            if !c.eat("(") {
                return None;
            }
            let name = c.parenthesized()?.trim().to_string();
            if name.is_empty() || !name.chars().all(|ch| ch.is_ascii_alphanumeric()) {
                return None;
            }
            c.blanks()?;
            if !c.eat("at") {
                return None;
            }
            c.blanks()?;
            let (q, _) = point(&mut c, &ctx, (0, 0))?;
            let Pt::At(x, y) = q else { return None };
            c.blanks()?;
            if !c.eat(";") {
                return None;
            }
            // The coordinate shape's centre on the canvas: its point through the transformation.
            ctx.names.push((name, ctx.tf.apply(x, y)));
            items.push(Item::Coordinate);
            continue;
        }
        // `\draw` is `\path[draw]`, `\fill` `\path[fill]`, `\filldraw` `\path[fill,draw]`.
        let (fill, draw) = if c.eat("\\filldraw") {
            (true, true)
        } else if c.eat("\\draw") {
            (false, true)
        } else if c.eat("\\fill") {
            (true, false)
        } else if c.eat("\\path") {
            (false, false)
        } else {
            return None;
        };
        if c.s.starts_with(|ch: char| ch.is_ascii_alphabetic()) {
            return None;
        }
        let p = PathCmd { fill, draw, opts: Vec::new(), round: 0, start: Pt::At(0, 0), segs: Vec::new(), tf: ctx.tf, unskips: 0, spaces: 0, arrows: ctx.arrows, own_arrows: false, tip: ctx.gt, nudges: Vec::new(), base_width: ctx.width, nodes: Vec::new(), step: [GRID_STEP; 2], edges: Vec::new(), edge: None, bare: false, shorten: ctx.shorten };
        ctx.named.set(false);
        let mut p = path(&mut c, &ctx, p, overlay)?;
        // A named coordinate is taken back through the picture's transformation, not the path's.
        if ctx.named.get() && p.tf != ctx.tf {
            return None;
        }
        // The path's nodes after it, numbered after it for their text boxes.
        let nodes = std::mem::take(&mut p.nodes);
        let edges = std::mem::take(&mut p.edges);
        items.push(Item::Path(p));
        let count = edges.len();
        for (k, mut e) in edges.into_iter().enumerate() {
            e.edge = Some((k == 0, k + 1 == count));
            // An edge's labels: in its own path's node box, inside the edge's.
            let labels = std::mem::take(&mut e.nodes);
            items.push(Item::Path(e));
            let n = labels.len();
            for (j, mut l) in labels.into_iter().enumerate() {
                l.on_path = Some((j == 0, j + 1 == n));
                l.path_node = true;
                items.push(Item::Node(l));
            }
            ctx.node_count += n;
        }
        let count = nodes.len();
        for (k, mut n) in nodes.into_iter().enumerate() {
            n.on_path = Some((k == 0, k + 1 == count));
            n.path_node = true;
            items.push(Item::Node(n));
        }
        ctx.node_count += count;
    }
    if !scopes.is_empty() {
        return None;
    }
    let nodes = items.iter().filter(|i| matches!(i, Item::Node(_))).count();
    // The baseline's point: `\pgfpointanchor` of a node with the picture untransformed.
    let baseline = match baseline {
        // A length: in pt, or of the font's `ex`, `em` (`\pgfmathsetlength` reads them as TeX
        // does; the machine passes the font's `ex` as `dims("ex")`).
        Some(b) if b.starts_with('\u{1}') => {
            let v = &b[1..];
            let y = match v.strip_suffix("ex").or_else(|| v.strip_suffix("em")) {
                Some(num) => node::factor_times(number(num)?, if v.ends_with("ex") { (ctx.dims)("ex")? } else { ctx.em }),
                None => length(v)?,
            };
            if overlay {
                return None;
            }
            Some((y, 0))
        }
        Some(b) => {
            let (n, a) = b.trim().split_once('.')?;
            if ctx.tf != Tf::ID || overlay {
                return None;
            }
            let ((_, y), sp) = ctx.nodes.iter().find(|s| s.name == n)?.anchor(a)?;
            Some((y, sp))
        }
        None => None,
    };
    (!items.is_empty() && nodes <= node::MAX_NODES).then_some(Picture { overlay, remember, space, widths, items, baseline })
}

/// Where a space token of an unrolled `\foreach` reaches the list.
const SPACE: &str = "\u{1}";

/// pgffor.code.tex's `\foreach` at `text` (a command's place): the loop unrolled, then the rest
/// of `text`. `\foreach \v/\w in {list} body`: `\pgffor@collectbody` takes a nested
/// `\foreach` header with its body, `{..}` (braces stripped) or up to `;`. Each value (blanks
/// before it skipped by `\pgfutil@ifnextchar`, those after kept) is split at `/` over the
/// variables and put for each `\v` (and the blanks a control word swallows); `a,...,b`
/// counts in pt from the value before (by 1pt, or by its distance from the one before that)
/// while not past `b`, printed by `\the` less a `.0` (`\pgffor@doloop`). The body runs after
/// `\pgffor@beginhook` (`\relax`): a space opening it is in the list, and so is one after the
/// loop. Values with macros, `[options]`, lists in a macro, and `...` with words are refused.
fn foreach(text: &str, budget: &mut usize) -> Option<String> {
    let Parts { vars, count, list, body, braced, rest } = foreach_parts(text)?;
    // A nested loop counting into the same name would be put in too.
    if count.is_some_and(|(n, _)| body.contains(&format!("count=\\{n}")) || vars.contains(&n)) {
        return None;
    }
    let mut values: Vec<String> = Vec::new();
    let items = split_top(list, ',');
    let mut k = 0;
    while k < items.len() {
        let v = items[k].trim_start();
        if v.trim_end() == "..." {
            let num = |t: &str| decimal(t).and_then(|t| length(&format!("{t}pt")));
            let last = num(values.last()?)?;
            let end = num(items.get(k + 1)?.trim())?;
            let step = match values.len() {
                1 => if end > last { 65536 } else { -65536 },
                n => last - num(&values[n - 2])?,
            };
            if step == 0 || vars.len() != 1 {
                return None;
            }
            let mut at = last;
            loop {
                at = at.checked_add(step)?;
                if (step < 0 && at < end) || (step > 0 && at > end) || values.len() > 10000 {
                    break;
                }
                let t = pgfmath::print(at);
                values.push(t.strip_suffix(".0").map(str::to_string).unwrap_or(t));
            }
            k += 2;
            continue;
        }
        if v.is_empty() || v.contains("...") {
            return None;
        }
        values.push(v.to_string());
        k += 1;
    }
    let mut out = String::new();
    for (k, value) in values.iter().enumerate() {
        let parts = if vars.len() == 1 { vec![value.as_str()] } else { value.split('/').collect() };
        if parts.len() != vars.len() {
            return None;
        }
        let mut b = body.to_string();
        for (var, part) in vars.iter().zip(&parts) {
            b = substitute(&b, var, part)?;
        }
        // `\pgfmathparse{int(\i+1)}` before each pass: the integer as pgfmath prints it.
        if let Some((name, from)) = count {
            b = substitute(&b, name, &(from + k as i32).to_string())?;
        }
        // One space token, or a `\par` (refused).
        let lead = &b[..b.len() - b.trim_start().len()];
        if lead.matches('\n').count() > 1 {
            return None;
        }
        if braced && !lead.is_empty() {
            out.push_str(SPACE);
        }
        out.push_str(b.trim_start());
        *budget = budget.checked_sub(b.len())?;
    }
    if rest.starts_with([' ', '\t', '\n']) {
        out.push_str(SPACE);
    }
    out.push_str(rest);
    Some(out)
}

/// `\pgfmathtruncatemacro{\name}{expr}` (pgfmathcalc.code.tex:227) in a picture: the integer
/// put for `\name` in the text after it, up to its next definition (each pass of a
/// `\foreach` body defines it before use; it is local to the pass). A blank after the
/// closing brace is a space token in the list (`true`).
fn truncate_macro(after: &str, ctx: &Ctx) -> Option<(String, bool)> {
    let mut c = Cursor { s: after.trim_start() };
    let name = braced(&mut c)?.trim().strip_prefix('\\').filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_alphabetic()))?;
    c.s = c.s.trim_start();
    let expr = braced(&mut c)?;
    let rest = c.s;
    let reg = |n: &str| REGISTERS.contains(&n).then(|| (ctx.dims)(n)).flatten();
    let value = pgfmath::truncate(expr, &reg)?.to_string();
    let space = rest.starts_with([' ', '\t', '\n']);
    // The next `\pgfmathtruncatemacro {\name}` (blanks as the macro's tokens print them).
    let mut end = rest.len();
    let mut from = 0;
    while let Some(k) = rest[from..].find("\\pgfmathtruncatemacro") {
        let at = from + k;
        let tail = rest[at + 21..].trim_start().strip_prefix('{').map(str::trim_start).and_then(|t| t.strip_prefix('\\')).and_then(|t| t.strip_prefix(name));
        if tail.is_some_and(|t| !t.starts_with(|ch: char| ch.is_ascii_alphabetic())) {
            end = at;
            break;
        }
        from = at + 21;
    }
    let (scope, later) = rest.split_at(end);
    Some((substitute(scope, name, &value)? + later, space))
}

/// A `\foreach`'s [`Parts`].
fn foreach_parts(text: &str) -> Option<Parts<'_>> {
    let mut c = Cursor { s: text.strip_prefix("\\foreach")? };
    let mut vars = Vec::new();
    let mut count = None;
    loop {
        c.blanks()?;
        if c.s.starts_with("in") && !c.s[2..].starts_with(|ch: char| ch.is_ascii_alphabetic()) {
            c.eat("in");
            break;
        }
        // `[count=\i]`, `[count=\i from n]` (`\pgffor@count@parse`): the only option.
        if !vars.is_empty() && count.is_none() && c.eat("[") {
            let k = c.s.find(']')?;
            let opt = c.s[..k].trim();
            c.s = &c.s[k + 1..];
            let (name, from) = opt.strip_prefix("count")?.trim_start().strip_prefix('=')?.trim().split_once(" from ").map_or((opt.split_once('=')?.1.trim(), "1"), |(n, f)| (n.trim(), f.trim()));
            let name = name.strip_prefix('\\').filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_alphabetic()))?;
            count = Some((name, from.parse::<i32>().ok().filter(|f| f.abs() < 1 << 20)?));
            continue;
        }
        if !vars.is_empty() && !c.eat("/") {
            return None;
        }
        c.blanks()?;
        let name = c.s.strip_prefix('\\')?;
        let n = name.len() - name.trim_start_matches(|ch: char| ch.is_ascii_alphabetic()).len();
        if n == 0 {
            return None;
        }
        vars.push(&name[..n]);
        c.s = &name[n..];
    }
    c.blanks()?;
    let list = braced(&mut c)?;
    c.blanks()?;
    let start = c.s;
    let is_braced = if c.s.starts_with("\\foreach") {
        c.s = foreach_parts(c.s)?.rest;
        false
    } else if c.s.starts_with('{') {
        let body = braced(&mut c)?;
        return Some(Parts { vars, count, list, body, braced: true, rest: c.s });
    } else {
        let k = top_semicolon(c.s)?;
        c.s = &c.s[k + 1..];
        false
    };
    Some(Parts { vars, count, list, body: &start[..start.len() - c.s.len()], braced: is_braced, rest: c.s })
}

/// A `\foreach`'s pieces: its variables, its `count` variable and first value, its list, its
/// body (whether that was in braces) and the text after it.
struct Parts<'a> {
    vars: Vec<&'a str>,
    count: Option<(&'a str, i32)>,
    list: &'a str,
    body: &'a str,
    braced: bool,
    rest: &'a str,
}

/// The inside of the balanced `{..}` the cursor stands on.
fn braced<'a>(c: &mut Cursor<'a>) -> Option<&'a str> {
    let s = c.s.strip_prefix('{')?;
    let mut depth = 1;
    for (k, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    c.s = &s[k + 1..];
                    return Some(&s[..k]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The first `;` outside braces (a delimited macro argument's end).
fn top_semicolon(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (k, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            ';' if depth == 0 => return Some(k),
            _ => {}
        }
    }
    None
}

/// The items of `s` split at `sep` outside braces and parentheses.
fn split_top(s: &str, sep: char) -> Vec<&str> {
    let (mut items, mut depth, mut from) = (Vec::new(), 0i32, 0);
    for (k, ch) in s.char_indices() {
        match ch {
            '{' | '(' => depth += 1,
            '}' | ')' => depth -= 1,
            _ if ch == sep && depth == 0 => {
                items.push(&s[from..k]);
                from = k + 1;
            }
            _ => {}
        }
    }
    items.push(&s[from..]);
    items
}

/// A plain decimal (`-1.5`, `3`), trimmed.
fn decimal(t: &str) -> Option<&str> {
    let t = t.trim();
    let d = t.strip_prefix('-').unwrap_or(t);
    (!d.is_empty() && d.matches('.').count() <= 1 && d.chars().all(|ch| ch.is_ascii_digit() || ch == '.')).then_some(t)
}

/// `\name` (not followed by a letter, with the blanks after it) replaced by `value`; `None`
/// when a nested `\foreach` binds the same name again.
fn substitute(body: &str, name: &str, value: &str) -> Option<String> {
    let (mut out, mut rest) = (String::with_capacity(body.len()), body);
    while let Some(k) = rest.find('\\') {
        out.push_str(&rest[..k]);
        let after = &rest[k + 1..];
        let n = after.len() - after.trim_start_matches(|ch: char| ch.is_ascii_alphabetic()).len();
        if n == 0 {
            // A control symbol: copied whole.
            let w = after.chars().next().map_or(0, char::len_utf8);
            out.push_str(&rest[k..k + 1 + w]);
            rest = &after[w..];
            continue;
        }
        let word = &after[..n];
        rest = &after[n..];
        if word == name {
            out.push_str(value);
            // TeX skips blanks after a control word, and one line end.
            let blanks = rest.len() - rest.trim_start_matches([' ', '\t']).len();
            rest = &rest[blanks..];
            if let Some(r) = rest.strip_prefix('\n') {
                rest = r.trim_start_matches([' ', '\t']);
            }
        } else {
            if word == "foreach" {
                let head = rest.split(" in").next().unwrap_or("");
                if head.split('/').any(|v| v.trim() == format!("\\{name}")) {
                    return None;
                }
            }
            out.push('\\');
            out.push_str(word);
        }
    }
    out.push_str(rest);
    Some(out)
}

/// 1 inside the group a picture opened to start its paragraph ([`Machine::tikz_picture`]);
/// grouping restores it.
const TIKZ_GROUP: Reg = Reg::register(Region::Int, 32690);

/// `\hoffset` and `\voffset`.
fn offsets(m: &Machine) -> [i32; 2] {
    [param!("hoffset"), param!("voffset")].map(|r| m.e.eqtb.word(r))
}

/// The tikz libraries whose files only define macros and keys (read in TeX Live 2026's pgf):
/// no register, box or layer is allocated, so loading one leaves every count, dimen and box
/// number as pdfLaTeX has it and the page unchanged. `topaths` is always loaded
/// (tikz.code.tex:5595). `shapes.geometric` reads pgflibraryshapes.geometric (shapes and
/// `\pgfutil@g@addto@macro` only). `arrows.meta` allocates four dimens
/// (pgflibraryarrows.meta.code.tex:21-24) and otherwise declares tips and keys: the native
/// job keeps no `\count11` to allocate from and a register's number changes nothing on the
/// page (as booktabs' registers); of its tips `Latex` is drawn, and `To` (so `->`) becomes
/// [`Cm`]. `calendar` reads pgfcalendar.code.tex (and pgfint.code.tex): eleven `\newcount`s
/// and a `\newif`, the rest definitions and keys; its `\calendar` is not of the grammar.
/// `arrows` reads pgflibraryarrows.code.tex: one `\newdimen\arrowsize` (:233) and
/// `\pgfarrowsdeclare`s of new names only (`latex'`, `stealth'`, `triangle 45`, `o`, ...;
/// `to`, `stealth`, `latex` stay pgfcorearrows' own), so tips drawn natively are unchanged and
/// its own names are outside the grammar.
/// Not here: `backgrounds` (`\pgfdeclarelayer`'s box, a layer drawn apart).
/// `automata` (it loads `shapes.multipart`, shapes only) adds the styles of [`AUTOMATA_STYLES`].
const LIBRARIES: [&str; 8] = ["positioning", "calc", "topaths", "shapes.geometric", "arrows.meta", "calendar", "arrows", "automata"];

/// tikzlibraryautomata.code.tex's `state` (`state without output`: `circle,draw,minimum
/// size=2.5em,every state`, `every state` empty until a document sets it, which the grammar
/// refuses: [`style_def`]).
const AUTOMATA_STYLES: [(&str, &str); 2] = [("state without output", "circle,draw,minimum size=2.5em"), ("state", "circle,draw,minimum size=2.5em")];

/// Whether `\usetikzlibrary{list}` text names the library `name`.
fn names_library(list: &str, name: &str) -> bool {
    list.split(',').any(|n| n.trim() == name)
}

/// After `\usetikzlibrary`: the bytes of its `{list}` argument when every name in it is one
/// of [`LIBRARIES`] (tikz.code.tex:5558 `\use@@tikzlibrary`: the list is split at commas and
/// each name stripped of spaces; an empty name is skipped).
pub(super) fn library_call_len(after: &str) -> Option<usize> {
    let list = after.strip_prefix('{')?;
    let k = list.find('}')?;
    list[..k].split(',').map(str::trim).all(|n| n.is_empty() || LIBRARIES.contains(&n)).then_some(k + 2)
}

/// TeX's `<factor><internal dimen>` for a written factor below one (`0.55228475\pgf@x`,
/// `.5\pgflinewidth`): `xn_over_d(v, round_decimals(digits), 2^16)`, truncated toward zero
/// as §107 does for either sign.
fn times_frac(digits: &[u8], v: i32) -> i32 {
    xn_over_d(v, crate::backend::backend_scaled::round_decimals(digits), 65536)
}

/// A picture's bounding box: `\pgf@picminx` .. `\pgf@picmaxy`, empty while `min > max`.
struct Bbox {
    min: [i32; 2],
    max: [i32; 2],
}

impl Bbox {
    /// `\pgf@protocolsizes`.
    fn add(&mut self, x: i32, y: i32) {
        self.min = [self.min[0].min(x), self.min[1].min(y)];
        self.max = [self.max[0].max(x), self.max[1].max(y)];
    }
}

/// A path's output once its points are known: literal operators with picture points
/// (`At`) or page points (`Page`, resolved in the ship walk).
enum Op {
    Lit(String),
    Page(u8, i32, i32),
}

/// `x y op ` of a picture point (`\pgfsys@moveto`, `\pgfsys@lineto`).
fn op_at(op: char, x: i32, y: i32) -> Op {
    Op::Lit(format!("{} {} {op} ", what::pgf_bp(x), what::pgf_bp(y)))
}

/// `\pgfpathellipse` (pgfcorepathconstruct.code.tex:907) about `(cx, cy)` with axes `(a, 0)`
/// and `(0, b)`: four quarter curves, each support `0.55228475` of an axis off the end
/// point, the close, then the moveto at the centre.
fn ellipse(cx: i32, cy: i32, a: i32, b: i32) -> Vec<Op> {
    let (ka, kb) = (times_frac(b"55228475", a), times_frac(b"55228475", b));
    vec![
        op_at('m', cx + a, cy),
        curve([(cx + a, cy + kb), (cx + ka, cy + b), (cx, cy + b)]),
        curve([(cx - ka, cy + b), (cx - a, cy + kb), (cx - a, cy)]),
        curve([(cx - a, cy - kb), (cx - ka, cy - b), (cx, cy - b)]),
        curve([(cx + ka, cy - b), (cx + a, cy - kb), (cx + a, cy)]),
        Op::Lit("h ".into()),
        op_at('m', cx, cy),
    ]
}

/// `\pgfsys@curveto`: `x1 y1 x2 y2 x3 y3 c `.
fn curve(p: [(i32, i32); 3]) -> Op {
    let mut s = String::new();
    for (x, y) in p {
        s.push_str(&what::pgf_bp(x));
        s.push(' ');
        s.push_str(&what::pgf_bp(y));
        s.push(' ');
    }
    s.push_str("c ");
    Op::Lit(s)
}

/// pgfcorepathprocessing.code.tex `\pgfprocess@@specialround`: the corner `b` between `a`
/// and `c` (axis-parallel sides) cut at `r` along each side: the line's new end and the
/// curve. `\pgfpointlineatdistance` normalises the side by its angle, exactly ±1 or 0 on an
/// axis, so each cut point is the corner moved by `r`; each support is
/// `0.5522847` of the way from the cut point to the corner.
/// A closed polygon of four corners rounded by `r`, as `\pgfprocessround` writes it: the
/// moveto at the first side's cut point, each corner (the first last), `h`, then the
/// subpath's own moveto at the third corner (`\pgfpathrectangle`'s corner).
fn rounded_rect(c: [(i32, i32); 4], r: i32) -> Vec<String> {
    let toward = |a: (i32, i32), b: (i32, i32)| (a.0 + r * (b.0 - a.0).signum(), a.1 + r * (b.1 - a.1).signum());
    let mut ops = vec![op_at('m', toward(c[0], c[1]).0, toward(c[0], c[1]).1)];
    for k in 1..=4 {
        ops.extend(round_corner(c[k - 1], c[k % 4], c[(k + 1) % 4], r));
    }
    ops.push(Op::Lit("h ".into()));
    ops.push(op_at('m', c[2].0, c[2].1));
    ops.into_iter().map(|o| if let Op::Lit(s) = o { s } else { String::new() }).collect()
}

fn round_corner(a: (i32, i32), b: (i32, i32), c: (i32, i32), r: i32) -> [Op; 2] {
    let toward = |from: (i32, i32), to: (i32, i32)| (from.0 + r * (to.0 - from.0).signum(), from.1 + r * (to.1 - from.1).signum());
    let (s, e) = (toward(b, a), toward(b, c));
    let k = |p: (i32, i32)| (p.0 + times_frac(b"5522847", b.0 - p.0), p.1 + times_frac(b"5522847", b.1 - p.1));
    [op_at('l', s.0, s.1), curve([k(s), k(e), e])]
}

/// `\pgfprocessround` (pgfcorepathprocessing.code.tex:280) of a run of straight lines with
/// `rounded corners`, on the canvas: each corner followed by a line (or by the `cycle`'s
/// close) cut at the radius along each side by `\pgfpointlineatdistance` (the side's
/// `\pgfpointnormalised` times the radius), the curve's supports `0.5522847` of the way to the
/// corner; the ends of an open run stay. Closed: the corner at the start is rounded last
/// (`\pgfprocess@specialroundclose`) and its curve's end is the moveto. `None` for any other
/// path, or for a side of length zero.
fn round_polyline(p: &PathCmd) -> Option<Vec<Op>> {
    let closed = matches!(p.segs.last(), Some((Seg::Cycle, _)));
    let pts = points(p, &p.segs[..p.segs.len() - closed as usize])?;
    let n = pts.len();
    if n < 2 || (closed && (n < 3 || pts[0] == pts[n - 1])) || pts.windows(2).any(|w| w[0] == w[1]) {
        return None;
    }
    let r = p.round;
    let cut = |b: (i32, i32), to: (i32, i32)| {
        let (nx, ny) = normalised(to.0 - b.0, to.1 - b.1);
        (b.0 + fmul(nx, r), b.1 + fmul(ny, r))
    };
    let corner = |a: (i32, i32), b: (i32, i32), c: (i32, i32), out: &mut Vec<Op>| {
        let (s, e) = (cut(b, a), cut(b, c));
        let k = |q: (i32, i32)| (q.0 + times_frac(b"5522847", b.0 - q.0), q.1 + times_frac(b"5522847", b.1 - q.1));
        out.push(op_at('l', s.0, s.1));
        out.push(curve([k(s), k(e), e]));
    };
    let mut out = Vec::new();
    if closed {
        let m = cut(pts[0], pts[1]);
        out.push(op_at('m', m.0, m.1));
        for i in 1..=n {
            corner(pts[i - 1], pts[i % n], pts[(i + 1) % n], &mut out);
        }
        out.push(Op::Lit("h ".into()));
    } else {
        out.push(op_at('m', pts[0].0, pts[0].1));
        for i in 1..n - 1 {
            corner(pts[i - 1], pts[i], pts[i + 1], &mut out);
        }
        out.push(op_at('l', pts[n - 1].0, pts[n - 1].1));
    }
    Some(out)
}

/// `\pgfpatharc`'s pieces (pgfcorepathconstruct.code.tex:289): while the angles are more than
/// 90 apart, a piece of 90 (more than 115 apart) or 60 toward the end; then the rest. With
/// how many pieces went up, each of which leaves a space (the `}` ending its line).
fn arc_pieces(mut a: i32, b: i32) -> (Vec<(i32, i32)>, u32) {
    let (mut pieces, mut ups) = (Vec::new(), 0);
    loop {
        let d = (a - b).abs();
        if d <= 90 << 16 {
            break;
        }
        let step = if d > 115 << 16 { 90 << 16 } else { 60 << 16 };
        if b > a {
            pieces.push((a, a + step));
            a += step;
            ups += 1;
        } else {
            pieces.push((a, a - step));
            a -= step;
        }
    }
    pieces.push((a, b));
    (pieces, ups)
}

/// `\pgf@arc`: one piece from `from` (the path's last point, on the canvas) as a curve: the
/// supports `k` times the radii off each end at right angles, `k` = `0.55228475` for 90
/// degrees, else pgfmath's `1.333333333*tan(.25*d)`; each polar point through the matrix.
/// The curve's operator and its three points; the end is the path's new last point.
fn arc_piece(tf: Tf, a: i32, b: i32, r: [i32; 2], from: (i32, i32)) -> [(i32, i32); 3] {
    let d = (b - a).abs();
    let k = if d == 90 << 16 {
        crate::backend::backend_scaled::round_decimals(b"55228475")
    } else {
        // `.25*d` and `1.333333333*t`: pgfmath's multiply, the second operand printed as the
        // factor of the first.
        fmul(pgfmath::tan(fmul(d, 16384)), crate::backend::backend_scaled::round_decimals(b"333333333") + (1 << 16))
    };
    let (ta, tb) = (fmul(k, r[0]), fmul(k, r[1]));
    let polar = |angle: i32, x: i32, y: i32| tf.linear(fmul(pgfmath::cos(angle), x), fmul(pgfmath::sin(angle), y));
    let up = if b > a { 90 << 16 } else { -(90 << 16) };
    let p = polar(a + up, ta, tb);
    let c1 = (from.0 + p.0, from.1 + p.1);
    let (pa, pb) = (polar(a, r[0], r[1]), polar(b, r[0], r[1]));
    let end = (from.0 - pa.0 + pb.0, from.1 - pa.1 + pb.1);
    let q = polar(b - up, ta, tb);
    [c1, (end.0 + q.0, end.1 + q.1), end]
}

impl Machine<'_, '_> {
    /// `\usetikzlibrary{list}` of [`LIBRARIES`]: their macros are pgf's, which the native job
    /// never runs, so loading them changes nothing it keeps. Any other library declines.
    pub(super) fn use_tikz_library(&mut self) {
        if self.next_is('[') {
            return self.not_native("\\usetikzlibrary[list]");
        }
        let mut t = Vec::new();
        self.read_arg(&mut t);
        let list: Option<String> = t.iter().map(|x| x.cs_id().is_none().then(|| x.chr())).collect();
        if !list.as_ref().is_some_and(|l| l.split(',').map(str::trim).all(|n| n.is_empty() || LIBRARIES.contains(&n))) {
            return self.not_native("a tikz library that allocates registers or is not read yet");
        }
        let has = |name: &str| list.as_ref().is_some_and(|l| l.split(',').any(|n| n.trim() == name));
        let (positioning, meta, arrows) = (has("positioning"), has("arrows.meta"), has("arrows"));
        if let Some(d) = self.doc.as_mut() {
            d.pkg.tikz_positioning |= positioning;
            d.pkg.tikz_arrows_meta |= meta;
            d.pkg.tikz_arrows_lib |= arrows;
        }
        if has("automata") {
            self.keep_tikz_styles(Some(AUTOMATA_STYLES.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()));
        }
    }
    /// `\tikzset{list}` in the preamble, of style definitions only ([`tikzset_styles`]):
    /// kept for every picture after it. In the body, or of other keys, declines.
    pub(super) fn tikz_set(&mut self) {
        let mut t = Vec::new();
        self.read_arg(&mut t);
        let mut s = String::new();
        self.show(&t, &mut s);
        self.keep_tikz_styles(tikzset_styles(&s));
    }

    /// `\tikzstyle{name}=[keys]` in the preamble ([`tikzstyle_call`]).
    pub(super) fn tikz_style(&mut self) {
        let mut t = Vec::new();
        self.read_arg(&mut t);
        let mut name = String::new();
        self.show(&t, &mut name);
        if !self.next_is('=') || !self.next_is('[') {
            return self.not_native("\\tikzstyle without =[keys]");
        }
        let keys = self.bracket_arg();
        let mut s = String::new();
        self.show(&keys, &mut s);
        self.keep_tikz_styles(style_def(&format!("{name}/.style"), &format!("{{{s}}}")).map(|d| vec![d]));
    }

    fn keep_tikz_styles(&mut self, styles: Option<Vec<(String, String)>>) {
        match (styles, self.doc.as_mut()) {
            (Some(v), Some(d)) if !d.body => d.pkg.tikz_styles.extend(v),
            _ => self.not_native("a tikz style the native job does not take, or one set in the body"),
        }
    }

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
        let text = if self.input.raw_peek().is_none() {
            match self.tikz_tokens() {
                Some(t) => t,
                None => return self.not_native("a tikzpicture in a macro without its end"),
            }
        } else {
            match self.input.raw_until("\\end{tikzpicture}", elc) {
                Some(t) => t,
                None => return self.not_native("a tikzpicture without its end in the same file"),
            }
        };
        self.tikz_draw(text, false);
    }

    /// A picture read from a token list (a macro's body, `\newcommand{\card}{\begin{tikzpicture}
    /// ...}`): its tokens up to `\end{tikzpicture}`, shown as text. Line ends and comments
    /// were dealt with when the list was made, as reading the file deals with them, so the
    /// text reads as the source would; a control word shows with a space after it, which the
    /// grammar takes as the source's own. `None` when the list ends first.
    fn tikz_tokens(&mut self) -> Option<String> {
        let mut toks: Vec<Token> = Vec::new();
        let end: Vec<char> = "{tikzpicture}".chars().collect();
        loop {
            let t = self.get_token()?;
            toks.push(t);
            if toks.len() > end.len() {
                let k = toks.len() - 1 - end.len();
                if toks[k].cs_id().is_some_and(|id| self.e.cs.name(id) == "end") && toks[k + 1..].iter().zip(&end).all(|(t, &c)| t.cs_id().is_none() && t.chr() == c) {
                    toks.truncate(k);
                    break;
                }
            }
        }
        let mut s = String::new();
        self.show(&toks, &mut s);
        Some(s)
    }

    /// `\tikz[opts] <command>;`: tikz.code.tex's `\tikz` collects the command up to its `;`
    /// and typesets it as a `tikzpicture` (probe: the same box): put back as one.
    pub(super) fn tikz_short(&mut self) {
        let mut t = vec![self.doc_id(doc!("begin")), Token::char(Cat::BeginGroup, '{')];
        t.extend("tikzpicture".chars().map(|c| Token::char(Cat::Letter, c)));
        t.push(Token::char(Cat::EndGroup, '}'));
        if self.next_is('[') {
            t.push(Token::char(Cat::Other, '['));
            t.extend(self.bracket_arg());
            t.push(Token::char(Cat::Other, ']'));
            // `\tikz@opt`'s `\pgfutil@ifnextchar\bgroup` skips the blanks after the options,
            // which the environment would keep as a space (probes u4-u7).
            if let Some(tok) = self.next_unblank() {
                // `\tikz[opts]{<commands>}` (tikz.code.tex:1771 `\tikz@`): the commands in a
                // group of the picture, as the environment's would be (its `\let\pgf@temp=`
                // takes the brace; a blank after it is the picture's, probes z13-z15).
                if tok.cat() == Some(Cat::BeginGroup) {
                    self.input.back(tok);
                    let mut body = Vec::new();
                    self.read_arg(&mut body);
                    t.extend(body);
                    t.push(Token::cs(self.e.cs.intern("end")));
                    t.push(Token::char(Cat::BeginGroup, '{'));
                    t.extend("tikzpicture".chars().map(|c| Token::char(Cat::Letter, c)));
                    t.push(Token::char(Cat::EndGroup, '}'));
                    return self.back_list(&t);
                }
                self.input.back(tok);
            }
        }
        let mut depth = 0;
        loop {
            let Some(tok) = self.get_token() else { return self.not_native("\\tikz without its ;") };
            t.push(tok);
            if tok.cs_id().is_none() {
                match tok.chr() {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    ';' if depth == 0 => break,
                    _ => {}
                }
            }
        }
        t.push(Token::cs(self.e.cs.intern("end")));
        t.push(Token::char(Cat::BeginGroup, '{'));
        t.extend("tikzpicture".chars().map(|c| Token::char(Cat::Letter, c)));
        t.push(Token::char(Cat::EndGroup, '}'));
        self.back_list(&t);
    }

    /// `\rt@tikzdraw`: the picture whose nodes' text boxes are now typeset.
    pub(super) fn tikz_resume(&mut self) {
        let Some(text) = self.doc.as_mut().and_then(|d| d.pkg.tikz.take()) else {
            return self.not_native("\\rt@tikzdraw without a picture");
        };
        self.tikz_draw(text, true);
    }

    /// The picture of source `text`. Its nodes' text boxes are typeset first, by the machine
    /// (`ready` false: their source is read as a file, then `\rt@tikzdraw` draws the picture).
    fn tikz_draw(&mut self, text: String, ready: bool) {
        // The length registers the picture names, read before it is parsed.
        let mut vals: Vec<(&str, i32)> = Vec::new();
        for n in REGISTERS {
            if text.contains(&format!("\\{n}")) {
                // `\paperwidth`, `\paperheight` are not the machine's: the PDF page's, which
                // pdftex.def sets to them.
                let page = ["paperwidth", "paperheight"].iter().position(|p| *p == n);
                if let Some(k) = page {
                    vals.push((n, self.e.eqtb.word(Reg { region: Region::Dimen, slot: super::PDF_PAGE_WIDTH + k as u32 })));
                } else if let Some(v) = self.length_of(&format!("\\{n}")) {
                    vals.push((n, v));
                }
            }
        }
        // The font's `ex` (a `baseline=` length's unit).
        let ex = { let f = self.cur_font(); self.e.fonts.params(f).get(4).copied().unwrap_or(0) };
        let dims = |n: &str| if n == "ex" { Some(ex) } else { vals.iter().find(|(k, _)| *k == n).map(|(_, v)| *v) };
        // Once the nodes' text boxes are typeset, named nodes are read at their size.
        let em = { let f = self.cur_font(); self.e.fonts.params(f).get(5).copied().unwrap_or(0) };
        let sizes: Vec<[i32; 3]> = if ready { (0..node::MAX_NODES as u32).map(|k| self.e.eqtb.box_reg(node::NODE_BOX + k).map_or([0; 3], |b| { let r = &self.e.nodes.boxes[b as usize]; [r.width, r.height, r.depth] })).collect() } else { Vec::new() };
        let boxes = |k: usize| sizes.get(k).copied().unwrap_or([0; 3]);
        let styles = self.doc.as_ref().map(|d| d.pkg.tikz_styles.clone()).unwrap_or_default();
        let Some(mut pic) = parse_with(&text, &dims, &boxes, em, &styles) else {
            return self.not_native("a tikzpicture the native job does not draw");
        };
        let meta = self.doc.as_ref().is_some_and(|d| d.pkg.tikz_arrows_meta);
        if meta {
            for i in pic.items.iter_mut() {
                if let Item::Path(p) = i {
                    if p.tip == Tip::To {
                        p.tip = Tip::Cm;
                    }
                }
            }
        }
        // `right=of a` without the positioning library is tikz's own key, the value a shift.
        if pic.items.iter().any(|i| matches!(i, Item::Node(n) if n.placed_of())) && !self.doc.as_ref().is_some_and(|d| d.pkg.tikz_positioning) {
            return self.not_native("a tikz placement with of, without the positioning library");
        }
        // `Latex` is arrows.meta's (an error without it); `stealth` it leaves as it is (probe).
        if !meta && pic.items.iter().any(|i| matches!(i, Item::Path(p) if p.arrows != [false; 2] && p.tip == Tip::Latex)) {
            return self.not_native("a tikz Latex tip without the arrows.meta library");
        }
        // A `"` in a node's text under babel: a language's shorthand may make it active
        // (ngerman's `"u`), not the font's quote (probe k5).
        if self.doc.as_ref().is_some_and(|d| d.babel.is_some()) && pic.items.iter().any(|i| matches!(i, Item::Node(n) if n.text.contains('"'))) {
            return self.not_native("a \" in a tikz node under babel");
        }
        // `\pgfsetcolor{.}` (pgfcoregraphicstate.code.tex `\pgf@setcolor`): the current colour's
        // xcolor model and values through `\pgfsys@color@<model>`, stroke then fill
        // (pgfsys-common-pdf.def: `v RG`, `v rg`; `v G`, `v g`), the values as xcolor keeps them.
        let set_color = match super::xcolor::Spec::from_literal(&self.current_color()) {
            Some(c) => {
                let v = c.vals.join(" ");
                if c.rgb {
                    [format!("{v} RG "), format!("{v} rg ")]
                } else if c.cmyk() {
                    [format!("{v} K "), format!("{v} k ")]
                } else {
                    [format!("{v} G "), format!("{v} g ")]
                }
            }
            None => return self.not_native("a tikzpicture in a colour of another model"),
        };
        let nodes: Vec<&node::NodeCmd> = pic.items.iter().filter_map(|i| if let Item::Node(n) = i { Some(n) } else { None }).collect();
        if !nodes.is_empty() && !ready {
            let mut src = String::new();
            for (k, n) in nodes.iter().enumerate() {
                node::text_box_source(n, k, self.e.eqtb.word(Reg { region: Region::Dimen, slot: super::PDF_PAGE_WIDTH }), &mut src);
            }
            if let Some(d) = self.doc.as_mut() {
                d.pkg.tikz = Some(text);
            }
            let t = self.doc_id(doc!("rt@tikzdraw"));
            self.input.back(t);
            self.input.trim();
            self.input.push_file(src, None);
            return;
        }
        // The nodes' colour literals: their options' (as a path's) and their text's.
        let mut node_lits: Vec<(Vec<String>, Option<String>)> = Vec::with_capacity(nodes.len());
        for n in &nodes {
            let mut lits = Vec::new();
            for (spec, half) in n.colors() {
                let Some(l) = self.color_literal(None, spec, false) else {
                    return self.not_native("a colour the native job does not know");
                };
                match half {
                    None => lits.push(l),
                    Some(stroke) => match pgf_color(&l, stroke) {
                        Some(h) => lits.push(h),
                        None => return self.not_native("a colour the native job does not know"),
                    },
                }
            }
            let text_lit = match n.text_color() {
                Some(spec) => match self.color_literal(None, spec, false) {
                    Some(l) if pgf_color(&l, true).is_some() => Some(l),
                    _ => return self.not_native("a colour the native job does not know"),
                },
                None => None,
            };
            node_lits.push((lits, text_lit));
        }
        // Each colour option's literal, by xcolor as `\color` makes it: the colour-stack
        // push of a colour key, pgf's `RG` or `rg` half of it for `draw=`, `fill=`.
        let paths: Vec<&PathCmd> = pic.items.iter().filter_map(|i| if let Item::Path(p) = i { Some(p) } else { None }).collect();
        let mut colors: Vec<Vec<String>> = Vec::with_capacity(paths.len());
        for p in &paths {
            let mut lits = Vec::new();
            for o in &p.opts {
                let (spec, half) = match o {
                    PathOpt::Color(s) => (s, None),
                    PathOpt::Stroke(s) => (s, Some(true)),
                    PathOpt::Fill(s) => (s, Some(false)),
                    _ => continue,
                };
                let Some(l) = self.color_literal(None, spec, false) else {
                    return self.not_native("a colour the native job does not know");
                };
                let l = match half {
                    None => l,
                    Some(stroke) => match pgf_color(&l, stroke) {
                        Some(h) => h,
                        None => return self.not_native("a colour the native job does not know"),
                    },
                };
                lits.push(l);
            }
            colors.push(lits);
        }
        // Room for the widths of [`ARROW_WIDTHS`] below `TIKZ_GROUP`'s register.
        if self.e.eqtb.word(Reg::register(Region::Int, ARROW_WIDTHS)) + paths.iter().filter(|p| p.arrows != [false; 2]).count() as i32 > 80 {
            return self.not_native("tikz arrows of many line widths");
        }
        // `\pgfsys@thepagewidth`, `\pgfsys@thepageheight` (pgfsys-pdftex.def): `\paperwidth`
        // and `\paperheight`, which pdftex.def also gives the PDF page.
        let pw = self.e.eqtb.word(Reg { region: Region::Dimen, slot: super::PDF_PAGE_WIDTH });
        let ph = self.e.eqtb.word(Reg { region: Region::Dimen, slot: super::PDF_PAGE_WIDTH + 1 });
        if pic.overlay && (pw <= 0 || ph <= 0) {
            return self.not_native("a tikzpicture without the paper size");
        }
        // `pgfpageorigin` is (`\hoffset`, `\voffset`): an anchor's point on the page.
        let [ho, vo] = offsets(self);
        let page_point = |c: &Coord| {
            let (x, y) = c.anchor.point(pw, ph);
            (x + ho + c.shift[0], y + vo + c.shift[1])
        };
        let mut bbox = Bbox { min: [i32::MAX; 2], max: [i32::MIN; 2] };
        // Each path's operators, built first: the bounding box must be known before the box.
        let mut ops: Vec<Vec<Op>> = Vec::with_capacity(paths.len());
        // Each grid's line count, in order: two spaces a line (`\pgf@pos@transform`'s lines).
        let mut grids: Vec<u32> = Vec::new();
        let mut tips: Vec<Vec<String>> = Vec::with_capacity(paths.len());
        for p in &paths {
            let mut out = Vec::new();
            let mut path_box = Bbox { min: [i32::MAX; 2], max: [i32::MIN; 2] };
            let tf = p.tf;
            let emit = |op: char, q: Pt, out: &mut Vec<Op>, bb: &mut Bbox| match q {
                Pt::At(x, y) => {
                    let (x, y) = tf.apply(x, y);
                    bb.add(x, y);
                    out.push(op_at(op, x, y));
                }
                Pt::Page(c) => {
                    let (x, y) = page_point(&c);
                    out.push(Op::Page(op as u8, x, y));
                }
            };
            let at = |q: Pt| match q {
                Pt::At(x, y) => (x, y),
                Pt::Page(c) => page_point(&c),
            };
            // tikz's `\tikz@flush@moveto`: the first point's moveto.
            emit('m', p.start, &mut out, &mut path_box);
            let mut last = p.start;
            let mut sub_start = at(p.start);
            // `\pgf@path@lastx` after an arc: its end as computed, not the point transformed.
            let mut arc_end: Option<(i32, i32)> = None;
            // tikz's `\tikz@lastx`: every point, `+(..)` too (only `\tikz@lastxsaved`, which
            // relative points start from, stays), is where `-|`, `rectangle` and `circle` start.
            for (s, _) in &p.segs {
                let after_arc = arc_end.take();
                match *s {
                    Seg::Arc { s, e, r, unitless, axes, to, .. } => {
                        let mut from = after_arc.unwrap_or_else(|| {
                            let u = at(last);
                            tf.apply(u.0, u.1)
                        });
                        // `\pgfpatharcaxes` (unitless): `\pgftransformtriangle` onto the
                        // axes, then radius 1pt.
                        let (atf, r) = if unitless { (tf.cm([axes[0], 0, 0, axes[1]], 0, 0), [1 << 16; 2]) } else { (tf, r) };
                        for (a, b) in arc_pieces(s, e).0 {
                            let pts = arc_piece(atf, a, b, r, from);
                            for (x, y) in pts {
                                path_box.add(x, y);
                            }
                            out.push(curve(pts));
                            from = pts[2];
                        }
                        arc_end = Some(from);
                        last = Pt::At(to.0, to.1);
                    }
                    Seg::Line(q) => {
                        emit('l', q, &mut out, &mut path_box);
                        last = q;
                    }
                    Seg::Move(q) => {
                        emit('m', q, &mut out, &mut path_box);
                        sub_start = at(q);
                        last = q;
                    }
                    // `\pgfpathcurveto`: each point protocolled.
                    Seg::Curve(q, _) => {
                        let pts = q.map(|(x, y)| tf.apply(x, y));
                        for (x, y) in pts {
                            path_box.add(x, y);
                        }
                        out.push(curve(pts));
                        last = Pt::At(q[2].0, q[2].1);
                    }
                    Seg::Hv(q) | Seg::Vh(q) => {
                        let ((x0, y0), (x1, y1)) = (at(last), at(q));
                        let corner = if matches!(s, Seg::Hv(_)) { (x1, y0) } else { (x0, y1) };
                        emit('l', Pt::At(corner.0, corner.1), &mut out, &mut path_box);
                        emit('l', q, &mut out, &mut path_box);
                        last = q;
                    }
                    Seg::Cycle => {
                        out.push(Op::Lit("h ".into()));
                        // `\pgfpathclose` then the moveto `cycle` leaves at the subpath's start.
                        last = Pt::At(sub_start.0, sub_start.1);
                    }
                    Seg::Rect(q) => {
                        // `\tikz@rectB`: moveto, three linetos, closepath, moveto.
                        let (user_x2, user_y2) = at(q);
                        let ((mut x1, mut y1), (mut x2, mut y2)) = (at(last), (user_x2, user_y2));
                        let mut corners = [(x1, y1), (x1, y2), (x2, y2), (x2, y1)];
                        if let (Pt::At(..), Pt::At(..)) = (last, q) {
                            // Each corner through the matrix (a turned one moves both
                            // coordinates).
                            corners = corners.map(|(x, y)| tf.apply(x, y));
                            for (x, y) in corners {
                                path_box.add(x, y);
                            }
                            ((x1, y1), (x2, y2)) = (corners[0], corners[2]);
                        }
                        let on_page = matches!(q, Pt::Page(_));
                        let put = |op: char, x: i32, y: i32| if on_page { Op::Page(op as u8, x, y) } else { op_at(op, x, y) };
                        if p.round == 0 {
                            for (k, (x, y)) in corners.into_iter().enumerate() {
                                out.push(put(if k == 0 { 'm' } else { 'l' }, x, y));
                            }
                            out.push(Op::Lit("h ".into()));
                        } else {
                            // `\pgfprocessround` (pgfcorepathprocessing.code.tex): each
                            // corner, the closing one last; the subpath's moveto becomes the
                            // closing curve's end.
                            let r = p.round;
                            let c = [(x1, y1), (x1, y2), (x2, y2), (x2, y1)];
                            // The first side, (x1, y1) to (x1, y2), is vertical.
                            let first_end = (x1, y1 + r * (y2 - y1).signum());
                            out.push(op_at('m', first_end.0, first_end.1));
                            for k in 1..=4 {
                                let [l, cv] = round_corner(c[k - 1], c[k % 4], c[(k + 1) % 4], r);
                                out.push(l);
                                out.push(cv);
                            }
                            out.push(Op::Lit("h ".into()));
                        }
                        out.push(put('m', x2, y2));
                        sub_start = (user_x2, user_y2);
                        last = q;
                    }
                    Seg::Grid(q, step) => {
                        let ((x0, y0), (x1, y1)) = (at(last), at(q));
                        let Some(lines) = grid_lines((x0.min(x1), y0.min(y1)), (x0.max(x1), y0.max(y1)), step) else {
                            return self.not_native("a tikz grid out of range");
                        };
                        grids.push(lines.len() as u32);
                        for line in lines {
                            for (k, (x, y)) in line.into_iter().enumerate() {
                                emit(if k == 0 { 'm' } else { 'l' }, Pt::At(x, y), &mut out, &mut path_box);
                            }
                        }
                        // The moveto to the far corner, tikz's last point.
                        emit('m', q, &mut out, &mut path_box);
                        last = q;
                    }
                    Seg::Circle(rx, ry, _) => {
                        // `\pgfpathellipse` (pgfcorepathconstruct.code.tex:907) about the
                        // current point (tikz's `\pgftransformshift{\tikz@last@position}`,
                        // the point transformed once) with axes (a, 0) and (0, b), the radii
                        // transformed (`\pgfpointtransformed` less the shift): four quarter
                        // curves, each support `0.55228475` of an axis off the end point.
                        let user = at(last);
                        let (cx, cy) = tf.apply(user.0, user.1);
                        let (a, b) = (fmul(tf.aa, rx), fmul(tf.bb, ry));
                        path_box.add(cx - a, cy - b);
                        path_box.add(cx + a, cy + b);
                        out.extend(ellipse(cx, cy, a, b));
                        sub_start = user;
                    }
                }
            }
            // `rounded corners` on straight lines: the path as `\pgfprocessround` rewrites it
            // (the picture size took the corners).
            if p.round != 0 && !p.segs.iter().all(|(s, _)| matches!(s, Seg::Rect(_))) {
                match round_polyline(p) {
                    Some(r) => out = r,
                    None => return self.not_native("tikz rounded corners the native job does not draw"),
                }
            }
            // `\pgfusepath`: a stroked path widens the picture by half its line width.
            if !pic.overlay && path_box.min[0] <= path_box.max[0] {
                let half = if p.draw { times_frac(b"5", line_width(p)) } else { 0 };
                bbox.add(path_box.min[0] - half, path_box.min[1] - half);
                bbox.add(path_box.max[0] + half, path_box.max[1] + half);
            }
            // The tips (`\pgf@stroke@inner@line` with arrows, pgfcorepathusage.code.tex): the
            // path's ends drawn back by the tip's line end, after the picture size took the
            // whole path; then each tip at its end, the start's first, its origin where the
            // line now ends.
            let mut path_tips = Vec::new();
            if p.arrows != [false; 2] || p.shorten != [0; 2] {
                let Some(mut ends) = tip_ends(p) else { return self.not_native("a tikz arrow the native job does not draw") };
                let single_line = p.segs.len() == 1 && matches!(p.segs[0].0, Seg::Line(_));
                let last_curve = match p.segs.last() {
                    Some((Seg::Curve(q, _), _)) => Some(*q),
                    _ => None,
                };
                let (ext, tip_end, back) = tip_extends(p);
                // The end first (`\pgf@prepare@end@of@path`), then the start on the path as
                // shortened; the tips are drawn start first.
                let mut placed = [None, None];
                for k in [1, 0] {
                    if !p.arrows[k] && p.shorten[k] == 0 {
                        continue;
                    }
                    let (tip, refp) = ends[k];
                    // `\pgfpointlineatdistance`: toward the neighbour by the tip's extend and
                    // the end's `shorten`.
                    let take = ext * p.arrows[k] as i32 + p.shorten[k];
                    let (nx, ny) = normalised(refp.0 - tip.0, refp.1 - tip.1);
                    let moved = (tip.0 + fmul(nx, take), tip.1 + fmul(ny, take));
                    if k == 1 && single_line {
                        ends[0].1 = moved;
                    }
                    let slot = if k == 0 { 0 } else { out.len() - 1 };
                    out[slot] = match last_curve {
                        Some(q) if k == 1 => curve([q[0], q[1], moved]),
                        _ => op_at(if k == 0 { 'm' } else { 'l' }, moved.0, moved.1),
                    };
                    if !p.arrows[k] {
                        continue;
                    }
                    // `\pgftransformarrow{ref}{tip}`, then `\pgf@arrow@draw@arrow`'s shift by the
                    // total length (tip end less back end, and the `shorten`) and the drawer's by
                    // the back end.
                    let (c, s) = normalised(tip.0 - refp.0, tip.1 - refp.1);
                    let total = tip_end - back + p.shorten[k];
                    let at = (tip.0 + fmul(c, -total) + fmul(c, -back), tip.1 + fmul(s, -total) + fmul(s, -back));
                    placed[k] = Some(tip_cm((c, s), at));
                    // `Latex`'s hull widens the picture (`\pgf@arrows@rigid@hull`, pgfcorearrows
                    // .code.tex:946): each point by the tip's matrix (`\pgf@pt@aa` .. as
                    // printed, the origin its shift).
                    let hull = match p.tip {
                        Tip::Latex => Latex::new(line_width(p)).hull(line_width(p)).to_vec(),
                        Tip::Cm => Cm::new(line_width(p)).hull(line_width(p)).to_vec(),
                        _ => Vec::new(),
                    };
                    if !pic.overlay {
                        for (hx, hy) in hull {
                            bbox.add(at.0 + fmul(c, hx) - fmul(s, hy), at.1 + fmul(s, hx) + fmul(c, hy));
                        }
                    }
                }
                path_tips.extend(placed.into_iter().flatten());
            }
            tips.push(path_tips);
            ops.push(out);
        }
        // Each node's geometry from its text box; the inner sep's `em` is the quad of the font
        // the picture is in.
        let mut geos = Vec::with_capacity(nodes.len());
        for (k, n) in nodes.iter().enumerate() {
            let dims = self.e.eqtb.box_reg(node::NODE_BOX + k as u32).map_or([0; 3], |b| { let r = &self.e.nodes.boxes[b as usize]; [r.width, r.height, r.depth] });
            let at = match n.at {
                Pt::At(x, y) => (x, y),
                Pt::Page(c) => page_point(&c),
            };
            geos.push(n.geometry(dims, em, n.line_width(), at, &mut bbox));
        }
        // `\endpgfpicture`: an empty picture (coordinates only) is of size zero.
        if bbox.min[0] > bbox.max[0] {
            bbox = Bbox { min: [0; 2], max: [0; 2] };
        }
        let fil = Glue { stretch: 65536, stretch_order: 1, shrink: 65536, shrink_order: 1, ..Glue::default() };
        let outer = self.cur.len();
        // `\hskip-\pgf@picminx`.
        let minx = if pic.overlay { 0 } else { bbox.min[0] };
        let g = self.e.nodes.glue_node(Glue { width: -minx, ..Glue::default() }, 0);
        self.cur.push(g);
        let pic_start = self.cur.len();
        // `\pgfsys@beginscope`, `\pgfsetcolor{.}`, `\pgfsetlinewidth{0.4pt}`.
        self.pdf_literal("q ");
        for l in &set_color {
            self.pdf_literal(l);
        }
        self.pdf_literal("0.3985 w ");
        let layer = self.cur.len();
        // tikz's `\scope[every picture,#1]`: `\pgfscope`, then its options' widths.
        self.pdf_literal("q ");
        for w in &pic.widths {
            self.pdf_literal(&format!("{} w ", what::pgf_bp(*w)));
        }
        let mut pending = pic.space as u32;
        let mut grid_counts = grids.into_iter();
        let mut drawn = paths.iter().zip(&colors).zip(ops.into_iter().zip(tips));
        let mut next_node = 0;
        let mut group = 0;
        // A path's scope with nodes: closed after them (its pushes).
        let mut deferred: Option<usize> = None;
        // An edge's own scope while its labels are typeset (the path's stays in `deferred`).
        let mut edge_deferred: Option<usize> = None;
        // Edges: the pending spaces of their path, their box, the current one's.
        let (mut edge_saved, mut edge_group, mut edge_box) = (0, 0, 0);
        // An edge whose labels are still to come (closed after them): whether it is the last.
        let mut edge_open: Option<bool> = None;
        // Trees: the open figboxes (start, the spaces pending after them), the child wrappers.
        let mut figs: Vec<(usize, u32)> = Vec::new();
        let mut wrappers: Vec<usize> = Vec::new();
        let positioning = self.doc.as_ref().is_some_and(|d| d.pkg.tikz_positioning);
        for (i, item) in pic.items.iter().enumerate() {
            if let Item::Node(n) = item {
                // A `\node` is a path of its own: its `\tikz@figbox` goes in after the spaces
                // pending and before its `\endpgfidscope`'s. A path's nodes go in one box
                // after it, at once: the path's `\endpgfidscope` space is not yet typeset.
                if let (Some((true, _)), true) = (n.on_path, n.path_node) {
                    pending = pending.saturating_sub(1);
                }
                if n.place_unskip() && !positioning {
                    pending = pending.saturating_sub(1);
                }
                pending += anchor_space(&n.at) + n.spaces;
                pending = pending.saturating_sub(shifts(&n.at));
                self.space_glue(&mut pending);
                if let Some((true, _)) = n.on_path {
                    group = self.cur.len();
                }
                let k = next_node;
                let fig = self.cur.len();
                self.tikz_node(n, node::NODE_BOX + k as u32, &geos[k], &node_lits[k].0, node_lits[k].1.as_deref(), &set_color);
                pending = 1;
                // A tree's root: its figbox takes the children; its idscope space after it.
                if n.tree {
                    figs.push((fig, 1));
                    pending = 0;
                }
                match n.on_path {
                    Some((_, true)) => {
                        self.natural_box(group, None);
                        // The path's `\endpgfidscope` space, then its scope's end (an edge's
                        // own, when they are its labels).
                        if let Some(pushes) = if edge_open.is_some() { edge_deferred.take() } else { deferred.take() } {
                            self.space_glue(&mut pending);
                            for _ in 0..pushes {
                                self.color_stack(None);
                            }
                            self.pdf_literal("Q ");
                        }
                        if let Some(last) = edge_open.take() {
                            self.edge_close(&mut pending, edge_box, last.then_some(edge_group), edge_saved);
                            if let Some(pushes) = if last { deferred.take() } else { None } {
                                self.space_glue(&mut pending);
                                for _ in 0..pushes {
                                    self.color_stack(None);
                                }
                                self.pdf_literal("Q ");
                            }
                        }
                    }
                    Some((_, false)) => pending = 0,
                    None => {}
                }
                next_node += 1;

                continue;
            }
            if let Item::Space = item {
                pending += 1;
                continue;
            }
            match item {
                Item::ChildBegin(spaces) => {
                    self.space_glue(&mut pending);
                    wrappers.push(self.cur.len());
                    self.pdf_literal("q ");
                    pending = *spaces;
                    continue;
                }
                Item::ChildEnd => {
                    self.space_glue(&mut pending);
                    self.pdf_literal("Q ");
                    let Some(w) = wrappers.pop() else { break };
                    self.natural_box(w, None);
                    continue;
                }
                Item::FigOpen => {
                    figs.push((self.cur.len(), std::mem::take(&mut pending)));
                    continue;
                }
                Item::FigClose => {
                    let Some((f, saved)) = figs.pop() else { break };
                    self.natural_box(f, None);
                    pending = saved;
                    continue;
                }
                _ => {}
            }
            if let Item::ScopeBegin(widths, space) = item {
                self.space_glue(&mut pending);
                self.pdf_literal("q ");
                for w in widths {
                    self.pdf_literal(&format!("{} w ", what::pgf_bp(*w)));
                }
                pending = *space as u32;
                continue;
            }
            if let Item::ScopeEnd(space) = item {
                pending += 1;
                self.space_glue(&mut pending);
                self.pdf_literal("Q ");
                pending = *space as u32;
                continue;
            }
            let Item::Path(_) = item else {
                // A `coordinate` node's box (pgfmoduleshapes.code.tex, shape `coordinate`):
                // `\hbox{\hbox{q, glue, Q}}`, then its path's idscope space.
                self.space_glue(&mut pending);
                let node = self.cur.len();
                self.pdf_literal("q ");
                pending = 1;
                self.space_glue(&mut pending);
                self.pdf_literal("Q ");
                self.pack_box(node, true);
                self.pack_box(node, true);
                pending = 1;
                continue;
            };
            let Some(((p, lits), (out, path_tips))) = drawn.next() else { break };
            // An edge: `\hbox{\hbox{\pgfscope <its path> \endpgfscope}}`, added to its path's
            // node box after the path, before the path's `\endpgfidscope` space.
            if let Some((first, _)) = p.edge {
                edge_saved = pending;
                pending = 0;
                if first {
                    edge_group = self.cur.len();
                }
                edge_box = self.cur.len();
                self.pdf_literal("q ");
            }
            pending = pending.saturating_sub(p.unskips);
            pending += anchor_space(&p.start) + p.spaces;
            pending = pending.saturating_sub(shifts(&p.start));
            let nudged = |k: usize| if positioning { 0 } else { p.nudges.iter().filter(|&&j| j == k).count() as u32 };
            pending = pending.saturating_sub(nudged(0));
            for (i, (s, _)) in p.segs.iter().enumerate() {
                if i > 0 {
                    pending = pending.saturating_sub(nudged(i));
                }
                match s {
                    Seg::Line(q) => {
                        pending += 1;
                        pending += anchor_space(q);
                        pending = pending.saturating_sub(shifts(q));
                    }
                    Seg::Rect(q) => {
                        pending += 1;
                        pending += anchor_space(q);
                        pending = pending.saturating_sub(shifts(q));
                    }
                    Seg::Cycle => pending += 1,
                    // tikz.code.tex:3687 `\tikz@do@arc`, `\tikz@@@arcfinal`: two spaces, two
                    // more after the unit case's `\edef`s, four after the unitless case's.
                    Seg::Arc { s, e, unitless, delta, .. } => pending += if *unitless { 6 } else { 4 } + arc_pieces(*s, *e).1 + *delta as u32,
                    Seg::Circle(_, _, spaces) => pending += spaces,
                    Seg::Curve(_, spaces) => pending += spaces,
                    Seg::Grid(q, _) => {
                        pending += anchor_space(q);
                        pending = pending.saturating_sub(shifts(q));
                        // `\tikz@gridB`'s `\tikzset` line, then two a line.
                        pending += 1 + 2 * grid_counts.next().unwrap_or(0);
                    }
                    Seg::Hv(_) | Seg::Vh(_) | Seg::Move(_) => {}
                }
            }
            pending = pending.saturating_sub(nudged(p.segs.len()));
            self.space_glue(&mut pending);
            // tikz.code.tex:2279: the scope only when the path has options.
            let scoped = !p.opts.is_empty();
            if scoped {
                self.pdf_literal("q ");
            }
            // The options run in order; `\pgflinewidth` is already the last width when they
            // start (`\tikz@semiaddlinewidth` sets it as each key is read).
            let mut width = line_width(p);
            let (mut k, mut pushes) = (0, 0);
            for o in &p.opts {
                match o {
                    PathOpt::Width(w) => {
                        width = *w;
                        self.pdf_literal(&format!("{} w ", what::pgf_bp(*w)));
                    }
                    PathOpt::Color(_) => {
                        self.color_stack(Some(&lits[k]));
                        k += 1;
                        pushes += 1;
                    }
                    PathOpt::Stroke(_) | PathOpt::Fill(_) => {
                        self.pdf_literal(&lits[k]);
                        k += 1;
                    }
                    PathOpt::Opacity(stroke, o) => self.pgf_opacity(*stroke, o),
                    PathOpt::Dash(d) => {
                        // pgfsys-common-pdf.def `\pgfsys@setdash`.
                        let mut s = String::from("[ ");
                        for e in d.iter() {
                            s.push_str(&what::pgf_bp(match e {
                                Dash::Len(l) => *l,
                                Dash::LineWidth => width,
                            }));
                            s.push(' ');
                        }
                        s.push_str("] 0.0 d ");
                        self.pdf_literal(&s);
                    }
                }
            }
            // A tip met first in the document (`\pgf@arrows@instantiate`) leaves two `\unskip`s,
            // then four spaces, in the list.
            let width = line_width(p);
            if p.arrows != [false; 2] {
                // `stealth`'s widths kept as `-(w + 1)`; its first instance typesets five
                // spaces after one `\unskip` (probes). `Latex`'s as `w + 2^28`: twelve spaces and no `\unskip`;
                // `Cm`'s as `w + 2^29`: seven, no `\unskip` (probes).
                let key = match p.tip {
                    Tip::To => width,
                    Tip::Stealth => -(width + 1),
                    Tip::OldLatex => -(width + 1) - (1 << 28),
                    Tip::StealthPrime => -(width + 1) - (2 << 28),
                    Tip::Latex => width + (1 << 28),
                    Tip::Cm => width + (1 << 29),
                };
                let reg = |k: i32| Reg::register(Region::Int, ARROW_WIDTHS + k as u32);
                let count = self.e.eqtb.word(reg(0));
                if !(1..=count).any(|k| self.e.eqtb.word(reg(k)) == key) {
                    self.e.eqtb.set_word(reg(count + 1), key, true);
                    self.e.eqtb.set_word(reg(0), count + 1, true);
                    // `stealth` takes one of the pending spaces, `to` two (probes: an unscoped
                    // `-stealth` after another path keeps one more `\glue 0.0` than `to`'s rule).
                    match p.tip {
                        Tip::To => {
                            self.unskip();
                            self.unskip();
                        }
                        Tip::Stealth | Tip::OldLatex => self.unskip(),
                        // The arrows library's `stealth'` takes two, as `to` does (probe a1).
                        Tip::StealthPrime => {
                            self.unskip();
                            self.unskip();
                        }
                        Tip::Latex | Tip::Cm => {}
                    }
                    let mut spaces = match p.tip {
                        Tip::To => 4,
                        Tip::Stealth | Tip::OldLatex | Tip::StealthPrime => 5,
                        Tip::Latex => 12,
                        Tip::Cm => 7,
                    };
                    self.space_glue(&mut spaces);
                }
            }
            for op in out.into_iter().filter(|_| (p.draw || p.fill) && !p.bare) {
                match op {
                    Op::Lit(s) => self.pdf_literal(&s),
                    Op::Page(o, x, y) => self.pgf_point(o, x, y),
                }
            }
            // `\pgfsys@fill`, `\pgfsys@stroke`, `\pgfsys@fillstroke`; `\path` nothing.
            match (p.fill, p.draw) {
                (true, false) => self.pdf_literal("f "),
                (false, true) => self.pdf_literal("S "),
                (true, true) => self.pdf_literal("B "),
                (false, false) => {}
            }
            let tip = match p.tip {
                Tip::To => to_tip(width),
                Tip::Stealth => stealth_tip(width),
                Tip::OldLatex => old_latex_tip(width),
                Tip::StealthPrime => stealth_prime_tip(width),
                Tip::Latex => Latex::new(width).code(),
                Tip::Cm => Cm::new(width).code(),
            };
            let tip_fill = if path_tips.is_empty() {
                None
            } else {
                match tip_fill(p, lits, &set_color[1]) {
                    Some(f) => f,
                    None => return self.not_native("tikz tips in colours the native job cannot compare"),
                }
            };
            for cm in &path_tips {
                self.pdf_literal("q ");
                if let Some(f) = &tip_fill {
                    self.pdf_literal(f);
                }
                for l in [cm, &tip, "Q "] {
                    self.pdf_literal(l);
                }
            }
            // The `\endpgfidscope` space stays pending without a scope: a later shift may
            // take it.
            pending += 1;
            // Its nodes are typeset in its scope: the scope ends after their box.
            if scoped && matches!(pic.items.get(i + 1), Some(Item::Node(n)) if n.path_node) || scoped && p.edge.is_none() && matches!(pic.items.get(i + 1), Some(Item::Path(e)) if e.edge.is_some()) {
                if p.edge.is_some() {
                    edge_deferred = Some(pushes);
                } else {
                    deferred = Some(pushes);
                }
            } else if scoped {
                self.space_glue(&mut pending);
                for _ in 0..pushes {
                    self.color_stack(None);
                }
                self.pdf_literal("Q ");
            }
            if let Some((_, last)) = p.edge {
                if matches!(pic.items.get(i + 1), Some(Item::Node(n)) if n.path_node) {
                    edge_open = Some(last);
                } else {
                    self.edge_close(&mut pending, edge_box, last.then_some(edge_group), edge_saved);
                    // The path's scope ends after its edges' box.
                    if let Some(pushes) = if last { deferred.take() } else { None } {
                        self.space_glue(&mut pending);
                        for _ in 0..pushes {
                            self.color_stack(None);
                        }
                        self.pdf_literal("Q ");
                    }
                }
            }
        }
        // `\endscope`: its `\endpgfidscope`'s space, then `\endpgfscope`.
        pending += 1;
        self.space_glue(&mut pending);
        self.pdf_literal("Q ");
        // The baseline anchor's spaces, in the main layer's box (probe).
        let mut baseline_spaces = pic.baseline.map_or(0, |b| b.1);
        self.space_glue(&mut baseline_spaces);
        if pic.remember {
            // `\hbox to0pt{\pgfsys@markposition{\pgfpictureid}}`: `\pdfsavepos` and the `.aux`
            // `\write` of `\pgfsyspdfmark`, kept for its place only.
            let mark = self.cur.len();
            self.save_pos();
            self.cur.push(Node { kind: Kind::Whatsit, sub: what::WRITE, font: what::NULL_STREAM, a: 0, b: 0 });
            self.pack_box(mark, true);
        }
        let g = self.e.nodes.glue_node(fil, 0);
        self.cur.push(g);
        self.pack_box(layer, true);
        // `\pgfsys@discardpath`, `\pgfsys@endscope`, then `\hss`.
        self.pdf_literal("n ");
        self.pdf_literal("Q ");
        let g = self.e.nodes.glue_node(fil, 0);
        self.cur.push(g);
        if pic.overlay {
            self.pack_box(pic_start, true);
            self.pack_box(outer, false);
        } else {
            // `\setbox\pgfpic=\hbox{\hskip-\pgf@picminx\lower\pgf@picminy\box\pgfpic}`, then
            // its height, width and depth set to the bounding box's.
            let mut b = self.hpack(pic_start, 0, true);
            b.shift = bbox.min[1];
            let b = self.e.nodes.add_box(b);
            self.cur.push(Node::new(Kind::Box, b as i32));
            let mut b = self.hpack(outer, 0, false);
            b.width = bbox.max[0] - bbox.min[0];
            b.height = bbox.max[1] - bbox.min[1];
            b.depth = 0;
            // `\raise-\pgf@ya`: the baseline's y less the bottom.
            if let Some((y, _)) = pic.baseline {
                b.shift = y - bbox.min[1];
            }
            let b = self.e.nodes.add_box(b);
            self.cur.push(Node::new(Kind::Box, b as i32));
        }
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
    /// An edge's end: its path's `\endpgfidscope` space, `\endpgfscope`, its box, and with
    /// the last edge the box of them all; then its path's pending spaces again.
    fn edge_close(&mut self, pending: &mut u32, edge_box: usize, group: Option<usize>, saved: u32) {
        self.space_glue(pending);
        self.pdf_literal("Q ");
        self.pack_box(edge_box, true);
        if let Some(g) = group {
            self.pack_box(g, true);
        }
        *pending = saved;
    }

    fn pack_box(&mut self, start: usize, exact: bool) {
        let b = self.hpack(start, 0, exact);
        let b = self.e.nodes.add_box(b);
        self.cur.push(Node::new(Kind::Box, b as i32));
    }
}

/// pgf's `\pgfsetstrokecolor` (`stroke`) or `\pgfsetfillcolor` literal from `\color`'s
/// (`1 0 0 rg 1 0 0 RG`, `1 g 1 G`, `c m y k k c m y k K`): the operands with
/// pgfsys-common-pdf.def's operator (`RG`, `G`, `K` or `rg`, `g`, `k`).
/// pgfcorearrows.code.tex:925 `\pgf@arrows@color@setup`: a tip's fill colour is the stroke
/// colour (`\pgfsetfillcolor{pgfstrokecolor}`, its `rg` form) when `\pgf@strokecolor@global`
/// and `\pgf@fillcolor@global` (xcolor's definitions) differ. Each is the picture's colour,
/// a colour key (both), or `draw=` / `fill=`; the same key, or the same colour text, is the
/// same definition, different literals different ones; equal literals of different
/// colour texts refused (`None`).
fn tip_fill(p: &PathCmd, lits: &[String], picture_fill: &str) -> Option<Option<String>> {
    let (mut stroke, mut fill) = ((".", picture_fill.to_string()), (".", picture_fill.to_string()));
    let mut k = 0;
    for o in &p.opts {
        match o {
            PathOpt::Color(c) => {
                let f = pgf_color(&lits[k], false)?;
                (stroke, fill) = ((c.as_str(), f.clone()), (c.as_str(), f));
            }
            PathOpt::Stroke(c) => stroke = (c.as_str(), lits[k].to_ascii_lowercase()),
            PathOpt::Fill(c) => fill = (c.as_str(), lits[k].clone()),
            _ => continue,
        }
        k += 1;
    }
    if stroke.0 == fill.0 {
        Some(None)
    } else if stroke.1 != fill.1 {
        Some(Some(stroke.1))
    } else {
        None
    }
}

impl Machine<'_, '_> {
    /// pgfsys-common-pdf.def `\pgfsys@stroke@opacity`, `\pgfsys@fill@opacity`: the
    /// ExtGState `/pgf@CA<v>` or `/pgf@ca<v>` added to pgf's resources the first time, then
    /// `/pgf@ca<v> gs`.
    pub(super) fn pgf_opacity(&mut self, stroke: bool, v: &str) {
        let (name, key) = if stroke { ("CA", "/CA") } else { ("ca", "/ca") };
        self.pdf_doc.pgf_extgs(&format!("/pgf@{name}{v} << {key} {v} >>"));
        self.pdf_literal(&format!("/pgf@{name}{v} gs "));
    }
}

fn pgf_color(lit: &str, stroke: bool) -> Option<String> {
    let words: Vec<&str> = lit.split_whitespace().collect();
    let k = words.iter().position(|w| matches!(*w, "rg" | "g" | "k"))?;
    let op = if stroke { words[k].to_ascii_uppercase() } else { words[k].to_string() };
    let mut s = words[..k].join(" ");
    s.push(' ');
    s.push_str(&op);
    s.push(' ');
    Some(s)
}


/// TikZ's natives, in `Meaning::Doc` order after the kernel's (`engine_doc::DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@tikzdraw", |m| m.tikz_resume()),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn first(p: &Picture) -> &PathCmd {
        p.items.iter().find_map(|i| if let Item::Path(q) = i { Some(q) } else { None }).expect("a path")
    }

    #[test]
    fn reads_the_overlay_frames() {
        let p = parse("[remember picture,overlay]\n  \\draw[line width=1.5pt,gold]\n    ([xshift=1cm,yshift=-1cm]current page.north west)\n    rectangle ([xshift=-1cm,yshift=1cm]current page.south east);\n", &|_| Some(1 << 16)).expect("a frame");
        assert!(p.space && p.overlay);
        assert_eq!(p.items.len(), 1);
        assert!(matches!(first(&p).start, Pt::Page(Coord { shift: [1864679, -1864679], .. })));
        assert!(matches!(first(&p).segs[0].0, Seg::Rect(Pt::Page(Coord { shifts: 2, .. }))));
        assert!(matches!(first(&p).opts[0], PathOpt::Width(98304)));
        let p = parse("[remember picture,overlay]\\fill[reportblue!15] ([yshift=-42mm]current page.north west) rectangle\n ([yshift=-47mm]current page.north east);", &|_| Some(1 << 16)).expect("a band");
        assert!(!p.space && first(&p).fill);
        // Not of the grammar: another option, a node, a blank line, a `+` length.
        assert!(parse("[overlay]\\draw (current page.north west) rectangle (current page.south east);", &|_| Some(1 << 16)).is_none());
        assert!(parse("[remember picture,overlay]\\node[draw] at (0,0) {x};", &|_| Some(1 << 16)).is_none());
        assert!(parse("[remember picture,overlay]\n\n\\draw (current page.north west) rectangle (current page.south east);", &|_| Some(1 << 16)).is_none());
        assert!(parse("[remember picture,overlay]\\draw[line width=+1pt] (current page.north west) rectangle (current page.south east);", &|_| Some(1 << 16)).is_none());
        assert!(parse("[remember picture,overlay]%\n\\draw[thick] (current page.north west) rectangle (current page.south east);", &|_| Some(1 << 16)).is_some_and(|p| !p.space));
        // Page points outside an overlay picture.
        assert!(parse("\\draw (current page.north west) rectangle (current page.south east);", &|_| Some(1 << 16)).is_none());
    }

    #[test]
    fn reads_inline_paths() {
        let p = parse("\n  \\draw[red!65,line width=.5pt] (.68,3.20)--(7.95,3.20);\n", &|_| Some(1 << 16)).expect("a line");
        assert!(!p.overlay && !p.space);
        assert!(matches!(first(&p).start, Pt::At(x, y) if what::pgf_bp(x) == "19.27562" && what::pgf_bp(y) == "90.7097"));
        let p = parse("\\draw[fill=navy,draw=navy,rounded corners=2pt] (0,0) rectangle (8.6,5.4);", &|_| Some(1 << 16)).expect("a card");
        assert!(first(&p).fill && first(&p).draw && first(&p).round == 131072);
        assert!(parse("\\draw (0,0) circle (13mm); \\draw[thick,dashed] (0,0) -- (1cm,10pt) -- cycle;", &|_| Some(1 << 16)).is_some());
        assert!(parse("\\draw[rounded corners] (0,0) -- (1,1);", &|_| Some(1 << 16)).is_some());
        assert!(parse("\\draw[rounded corners] (0,0) -- (1,1) rectangle (2,2);", &|_| Some(1 << 16)).is_none());
        assert!(parse("\\draw[rounded corners] (0,0) -- (1,1) -- (1,1);", &|_| Some(1 << 16)).is_none());
        assert!(parse("\\draw (0,0) -- (\\hsize,1pt);", &|_| Some(1 << 16)).is_none());
        for pic in ["\\draw (1,1) -- ++(1,0) -- +(0,1);", "\\coordinate (a) at (1,1); \\draw (a) -- (2,2);", "\\draw (0,0) -- (1,1) ++(1,0) -- (3,0);", "\\draw (0,0) rectangle ++(2,1);", "\\draw (0,0) -- (0.5,\\textwidth);", "\\draw (1,1) circle (1) ++(1,0) circle (0.5);", "\\coordinate (z) at (5,5);"] {
            assert!(parse(pic, &|_| Some(1 << 16)).is_some(), "{pic}");
        }
        assert!(parse("\\draw[->] (0,0) -- (1,1) -- cycle;", &|_| Some(1 << 16)).is_none());
        assert!(parse("\\path (0,0) -- (1,1);", &|_| Some(1 << 16)).is_none());
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
        // pdfLaTeX's: 8.6 and 5.4 times 1cm, a 2pt corner's support, a 13mm circle's.
        assert_eq!(what::pgf_bp(times_xx("8.6", UNIT).expect("8.6")), "243.78271");
        assert_eq!(what::pgf_bp(times_xx("5.4", UNIT).expect("5.4")), "153.07259");
        assert_eq!(what::pgf_bp(131072 - times_frac(b"5522847", 131072)), "0.89207");
        let r = length("13mm").expect("13mm");
        assert_eq!(what::pgf_bp(times_frac(b"55228475", r)), "20.35242");
        assert_eq!(pgf_color("0.12158 0.22745 0.37256 rg 0.12158 0.22745 0.37256 RG", true).as_deref(), Some("0.12158 0.22745 0.37256 RG "));
        assert_eq!(pgf_color("1 g 1 G", false).as_deref(), Some("1 g "));
    }

    #[test]
    fn reads_arrows() {
        let dims = |_: &str| None;
        assert_eq!(first(&parse(r"\draw[<->] (0,0) -- (1,0);", &dims).expect("<->")).arrows, [true, true]);
        assert_eq!(first(&parse(r"\draw[->,thick] (0,0) -| (1,1);", &dims).expect("-|")).arrows, [false, true]);
        assert!(parse(r"\draw[->] (0,0) -- (1,1);", &dims).is_some());
        // The picture's tips and width are each path's unless it names its own.
        let pic = parse(r"[->,thick]\draw (0,0) -- (1,0);\draw[<-] (0,1) -- (1,1);", &dims).expect("picture tips");
        assert_eq!(pic.widths, [52429]);
        let paths: Vec<_> = pic.items.iter().filter_map(|i| if let Item::Path(p) = i { Some((p.arrows, line_width(p))) } else { None }).collect();
        assert_eq!(paths, [([false, true], 52429), ([true, false], 52429)]);
        // A style stands for its keys; one named as a tikz key is refused.
        assert!(parse(r"[box/.style={draw}]\draw[box] (0,0) -- (1,0);", &dims).is_some_and(|p| matches!(&p.items[0], Item::Path(q) if q.draw)));
        assert!(parse(r"[step/.style={draw}]\draw[step] (0,0) -- (1,0);", &dims).is_none());
        // Fills alone, tips longer than the line and closed paths with own tips are refused.
        for t in [r"\fill[->] (0,0) -- (1,0);", r"\draw[<->] (0,0) -- (0.5pt,0);", r"\draw[->] (0,0) circle (1);"] {
            assert!(parse(t, &dims).is_none(), "{t}");
        }
        // pdfLaTeX's tip at 0.4pt and the line's 0.46pt drawn back.
        assert_eq!(to_tip(26214), "q 0.31879 w [ ] 0.0 d 1 J 1 j -1.19551 1.59401 m -1.09587 0.99626 0.0 0.09961 0.29886 0.0 c 0.0 -0.09961 -1.09587 -0.99626 -1.19551 -1.59401 c S Q  ");
        assert_eq!(what::pgf_bp(UNIT - to_extend(26214)), "27.88852");
    }

    #[test]
    fn reads_polar_points() {
        // pdfLaTeX's `(30:1)` and `++(-45:2) -- +(200:3mm)`.
        let ctx = Ctx { dims: &|_| None, names: Vec::new(), unit: [UNIT; 2], tf: Tf::ID, boxes: &|_| [0; 3], em: 0, nodes: Vec::new(), node_count: 0, distance: [0; 2], width: 26214, arrows: [false; 2], gt: Tip::To, styles: Vec::new(), spaces: Default::default(), shape_ref: Default::default(), named: Default::default(), tree: [0, 0, -90], shorten: [0; 2], auto: false };
        let (x, y) = polar("30:1", &ctx).expect("30:1");
        assert_eq!((what::pgf_bp(x), what::pgf_bp(y)), ("24.5491".into(), "14.17339".into()));
        assert_eq!(polar("30:1cm", &ctx), Some((x, y)));
        let (x, y) = polar("-45:2", &ctx).expect("-45:2");
        let (u, v) = polar("200:3mm", &ctx).expect("200:3mm");
        assert_eq!((what::pgf_bp(x + u), what::pgf_bp(y + v)), ("32.09726".into(), "-42.99707".into()));
        assert!(polar("north:1", &ctx).is_none());
    }

    #[test]
    fn unrolls_foreach() {
        let mut budget = 1 << 20;
        let mut un = |t: &str| foreach(t, &mut budget);
        assert_eq!(un(r"\foreach \x in {1,2} {\draw (\x,0);} \path;").as_deref(), Some("\\draw (1,0);\\draw (2,0);\u{1} \\path;"));
        // Dots count in pt from the value before, by the step of the two before; `\the` less `.0`.
        assert_eq!(un(r"\foreach \x in {0,0.5,...,1.6} \fill (\x ,1);").as_deref(), Some("\\fill (0,1);\\fill (0.5,1);\\fill (1,1);\\fill (1.5,1);"));
        assert_eq!(un(r"\foreach \x in {3,...,1}{ \draw (\x,0);}").as_deref(), Some("\u{1}\\draw (3,0);\u{1}\\draw (2,0);\u{1}\\draw (1,0);"));
        assert_eq!(un(r"\foreach \x/\y in {1/a, 2/b} \draw (\x,\y);").as_deref(), Some("\\draw (1,a);\\draw (2,b);"));
        // A nested loop is the body; `\xa` is not `\x`.
        assert_eq!(un(r"\foreach \x in {1,2} \foreach \y in {\x} {\draw (\x,\y) \xa;}").as_deref(), Some("\\foreach \\y in {1} {\\draw (1,\\y) \\xa;}\\foreach \\y in {2} {\\draw (2,\\y) \\xa;}"));
        // `count`: from 1, or from its `from`.
        assert_eq!(un(r"\foreach \x [count=\i] in {a,b} \draw (\i,\x);").as_deref(), Some("\\draw (1,a);\\draw (2,b);"));
        assert_eq!(un(r"\foreach \x [count=\i from -1] in {a,b} \draw (\i,\x);").as_deref(), Some("\\draw (-1,a);\\draw (0,b);"));
        for t in [r"\foreach \x in \list {}", r"\foreach \x [evaluate=\x] in {1} {}", r"\foreach \x [count=\x] in {1} {}", r"\foreach \x in {a,...,e} {}", r"\foreach \x/\y in {1} {}", r"\foreach \x in {1} {\foreach \x in {2} {}}"] {
            assert!(un(t).is_none(), "{t}");
        }
    }

    #[test]
    fn reads_arcs_and_turns() {
        let dims = |_: &str| None;
        // `\pgfpatharc`'s pieces: 170 degrees up is 90 then 80; 360 down is four of 90.
        assert_eq!(arc_pieces(30 << 16, 200 << 16), (vec![(30 << 16, 120 << 16), (120 << 16, 200 << 16)], 1));
        assert_eq!(arc_pieces(0, -(360 << 16)).0.len(), 4);
        // pdfLaTeX's `(0,0) arc (0:90:1)`: one curve.
        let [c1, c2, e] = arc_piece(Tf::ID.cm([UNIT, 0, 0, UNIT], 0, 0), 0, 90 << 16, [1 << 16; 2], (0, 0));
        assert_eq!([c1, c2, e].map(|(x, y)| format!("{} {}", what::pgf_bp(x), what::pgf_bp(y))), ["0.0 15.6557", "-12.69109 28.3468", "-28.3468 28.3468"]);
        let p = parse(r"\draw (1,0) arc [start angle=0, delta angle=90, radius=1] -- cycle;", &dims).expect("arc[]");
        assert!(matches!(first(&p).segs[0].0, Seg::Arc { delta: true, unitless: true, .. }));
        // Turned: rectangles and lines, not circles or rounded corners.
        assert!(parse(r"\draw[rotate=30] (0,0) rectangle (1,1);", &dims).is_some());
        assert!(parse(r"\draw[rotate=30] (0,0) circle (1);", &dims).is_none());
        assert!(parse(r"\draw[rotate=30,rounded corners] (0,0) rectangle (1,1);", &dims).is_none());
        // pdfLaTeX's `rotate=30` of (1,0).
        let (x, y) = Tf::ID.rotate(30 << 16).apply(UNIT, 0);
        assert_eq!((what::pgf_bp(x), what::pgf_bp(y)), ("24.5491".into(), "14.17339".into()));
    }

    #[test]
    fn reads_trees() {
        let dims = |_: &str| None;
        // A child's wrapper, node, edge, and the grandchildren in the edge's figbox.
        let p = parse(r"\node (a) {a} child {node {b} child {node (c) {c}}} child {node {d}}; \draw (c) -- (a-2);", &dims).expect("a tree");
        let kinds: Vec<u8> = p.items.iter().map(|i| match i { Item::Node(_) => b'n', Item::Path(_) => b'p', Item::ChildBegin(_) => b'(', Item::ChildEnd => b')', Item::FigOpen => b'[', Item::FigClose => b']', _ => b'?' }).collect();
        assert_eq!(kinds, b"n(np[(np)])(np)]p");
        // The wrapper's spaces: 4658's line end, then one per name, options and text.
        assert!(matches!(p.items[1], Item::ChildBegin(2)) && matches!(p.items[5], Item::ChildBegin(3)));
        // `\pgfpointpolar{-180}{2(.5\tikzsiblingdistance)}` of an odd 15mm: one sp right.
        assert_eq!(node::child_point((0, 0), 1, 1, [TREE_DISTANCE, TREE_DISTANCE, -90]), Some((1, -TREE_DISTANCE)));
        // `edge from parent` written in the child, `grow=` in the picture's options.
        assert!(parse(r"\node {a} child {node {b} edge from parent[draw=none]};", &dims).is_some());
        assert!(parse("[grow=right]\n\\node {a} child {node {b}};", &dims).is_some());
        // Not: child options, growth in a style, a scale, a level style of other keys.
        assert!(parse(r"\node {a} child[grow=right] {node {b}};", &dims).is_none());
        assert!(parse("[level 1/.style={grow=up}]\n\\node {a} child {node {b}};", &dims).is_none());
        assert!(parse(r"\begin{scope}[scale=2] \node {a} child {node {b}}; \end{scope}", &dims).is_none());
        assert!(parse("[level/.style={red}]\n\\node {a} child {node {b}};", &dims).is_none());
        assert!(parse("[scale=1.0, transform shape]\n\\node {a} child {node {b}};", &dims).is_some());
    }

    /// The statement census (a diagnostic, never run by default): `RT_TIKZ_CENSUS=<list>`
    /// names one document folder per line; each folder's `.tex` files are read as one source
    /// (its preamble styles), every `tikzpicture` in it is tried as the pre-scan tries it
    /// ([`accepts`]), and a refused picture is cut into statements (top-level `;`, scope
    /// lines apart) kept while the prefix still reads, so each refused statement prints once.
    /// Run with `cargo test --release --no-default-features --target-dir target/noeng --lib
    /// statement_census -- --ignored --nocapture`: fetched documents never meet the engine.
    #[test]
    #[ignore]
    fn statement_census() {
        let Ok(list) = std::env::var("RT_TIKZ_CENSUS") else { return };
        let (mut pics, mut whole, mut refused) = (0, 0, 0);
        for dir in std::fs::read_to_string(list).unwrap_or_default().lines().filter(|l| !l.trim().is_empty()) {
            let mut files: Vec<_> = walk(std::path::Path::new(dir.trim()));
            files.sort();
            let src: String = files.iter().map(|f| String::from_utf8_lossy(&std::fs::read(f).unwrap_or_default()).into_owned()).collect::<Vec<_>>().join("\n");
            let styles = preamble_styles(&src);
            let name = std::path::Path::new(dir.trim()).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let mut rest = src.as_str();
            let mut k = 0;
            while let Some(a) = rest.find("\\begin{tikzpicture}") {
                let r = &rest[a + 19..];
                let Some(e) = r.find("\\end{tikzpicture}") else { break };
                let body = &r[..e];
                rest = &r[e..];
                pics += 1;
                if accepts(body, &styles) {
                    whole += 1;
                    println!("{name}-{k:03}\tACCEPTED");
                } else {
                    let (head, stmts) = statements(body);
                    let mut kept = String::new();
                    for st in stmts {
                        let mut t = format!("{head}{kept}{st}");
                        let open = t.matches("\\begin{scope}").count().saturating_sub(t.matches("\\end{scope}").count());
                        t.push_str(&"\\end{scope}".repeat(open));
                        if accepts(&t, &styles) {
                            kept.push_str(&st);
                        } else {
                            refused += 1;
                            println!("{name}-{k:03}\t{}", st.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(160).collect::<String>());
                        }
                    }
                }
                k += 1;
            }
        }
        println!("census: {pics} pictures, {whole} fully accepted, {refused} refused statements");

        fn walk(d: &std::path::Path) -> Vec<std::path::PathBuf> {
            let mut v = Vec::new();
            for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    v.extend(walk(&p));
                } else if p.extension().is_some_and(|x| x == "tex") {
                    v.push(p);
                }
            }
            v
        }
        // The picture's options (a leading `[..]`) and its statements: up to each top-level
        // `;`, a `\begin{scope}[..]` or `\end{scope}` alone.
        fn statements(body: &str) -> (String, Vec<String>) {
            let clean = strip_comments(body).unwrap_or_else(|| body.to_string());
            let lead = clean.len() - clean.trim_start().len();
            let (head, s) = match clean[lead..].starts_with('[').then(|| close(&clean, lead)).flatten() {
                Some(j) => (clean[..j].to_string(), &clean[j..]),
                None => (String::new(), &clean[..]),
            };
            let mut out = Vec::new();
            let (mut d, mut start, mut i) = (0i32, 0, 0);
            let bytes = s.as_bytes();
            while i < bytes.len() {
                match bytes[i] {
                    b'{' | b'[' => d += 1,
                    b'}' | b']' => d -= 1,
                    b';' if d <= 0 => {
                        out.push(s[start..=i].to_string());
                        start = i + 1;
                    }
                    b'\\' if d <= 0 && (s[i..].starts_with("\\begin{scope}") || s[i..].starts_with("\\end{scope}")) => {
                        if !s[start..i].trim().is_empty() {
                            out.push(s[start..i].to_string());
                        }
                        let begin = s[i..].starts_with("\\begin");
                        let mut j = i + if begin { 13 } else { 11 };
                        let off = j + (s[j..].len() - s[j..].trim_start().len());
                        if begin && s[off..].starts_with('[') {
                            j = close(s, off).unwrap_or(j);
                        }
                        out.push(s[i..j].to_string());
                        start = j;
                        i = j;
                        continue;
                    }
                    _ => {}
                }
                i += 1;
            }
            if !s[start..].trim().is_empty() {
                out.push(s[start..].to_string());
            }
            (head, out)
        }
        // Just past the bracket group opening at `at`.
        fn close(s: &str, at: usize) -> Option<usize> {
            let mut d = 0;
            for (q, c) in s[at..].char_indices() {
                match c {
                    '[' | '{' => d += 1,
                    ']' | '}' => d -= 1,
                    _ => {}
                }
                if d == 0 {
                    return Some(at + q + 1);
                }
            }
            None
        }
    }
}
