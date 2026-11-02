//! TikZ's `\node` on the GDL path (`2026-10-31-tikz-design.md`, stage 2: nodes): a rectangle
//! node of one line of text, `\node[options] at (x,y) {text};`, in a picture of
//! `engine_tikz`'s grammar. Anything else in a node goes to the engine.
//!
//! A node is a path with no paint whose `node` operation builds `\tikz@figbox` (tikz.code.tex
//! `\tikz@fig@continue`), set at the path's end before its `\endpgfidscope` space:
//!
//! `\hbox{\hbox{q, options, glue.., background path, q, \hbox(0+0)x0{\glue x,
//! \hbox shifted -y{a b c d 0 0 cm, q, \hbox(0+0)x0{\hbox{RG, rg, \box text, glue}}, Q, glue},
//! \hss}, Q, glue, glue, pops, Q}}`
//!
//! - `q` .. `Q` are `\pgfscope`, its options (`\tikz@options`: `draw=`, `fill=`, widths and
//!   colours, as on paths) and `\endpgfscope`; the last two glues the space of
//!   `\pgfsys@hboxsynced`'s and of the node's own `\endpgfidscope` (pgfsys.code.tex:601).
//! - The leading glues are spaces of the rectangle shape's code typeset in `\nullfont`: the
//!   anchor's (`\pgf@sh@reanchor`) twice, `\pgfmultipartnode` computing it for the shift and
//!   again for `\pgfsavepgf@process`, when it opens with `{` and a line end
//!   (pgfmoduleshapes.code.tex:1012-1068: `center`, `north`, `south`, `west`, `north west`,
//!   `south east`; not `east`, `north east`, `south west`), and three more whatever the anchor;
//!   the last of them is the background path's `\endpgfidscope` (`\tikz@finish`), which goes
//!   after its paint when the path is scoped. Measured on pdfLaTeX for every anchor.
//! - The background path (`\backgroundpath`, pgfmoduleshapes.code.tex:1112) is
//!   `\pgfpathrectanglecorners{southwest + outer sep}{northeast - outer sep}`, one `re`
//!   (`\pgf@normalrect`: the transformation is a translation), painted `S`, `f` or `B` and
//!   scoped with the options again when the node has options; unpainted it writes nothing.
//! - The text box is `\pgfsys@hboxsynced` (pgfsys-pdftex.def:27): `\hskip x \raise y\hbox{
//!   \pgflowlevelsynccm \pgfsys@hbox}` with (x, y) the node's translation, the box's width,
//!   height and depth then set to zero; inside, the text box recoloured
//!   (`\pgfsetcolor{.}`: `RG`, `rg` of the text colour, else of black) with
//!   `\endpgfidscope`'s space after it.
//! - The text box itself (`\tikz@do@fig`): `\hbox{font{\color{text}\ignorespaces text}}`
//!   with `\aftergroup\unskip`, typeset by the machine before the picture is drawn.
//!
//! Sizes (pgfmoduleshapes.code.tex:937-1010, the saved anchors `\northeast`, `\southwest`):
//! the text box widened by twice the inner sep (`.3333em` of the font the node starts in, as
//! TeX reads `0.3333em`), at least the minimum size (`1pt`), half of it each way about the
//! text's middle (`.5\pgf@x`, TeX's truncating `<factor><dimen>`), plus the outer sep
//! (`.5\pgflinewidth` through pgfmath's multiply).

use super::{color_spec, length, point, Ctx, Cursor, PathOpt, Pt, WIDTHS};
use super::super::super::engine_eqtb::Glue;
use super::super::super::engine_node::{Kind, Node};
use super::super::super::engine_run::Machine;
use super::super::super::engine_whatsit as what;
use crate::backend::backend_scaled::{round_decimals, xn_over_d};

/// A node's anchor (`anchor=`): the rectangle's compass points and `center`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum NodeAnchor {
    Center,
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

const ANCHORS: [(&str, NodeAnchor); 9] = [
    ("center", NodeAnchor::Center),
    ("north", NodeAnchor::North),
    ("south", NodeAnchor::South),
    ("east", NodeAnchor::East),
    ("west", NodeAnchor::West),
    ("north east", NodeAnchor::NorthEast),
    ("north west", NodeAnchor::NorthWest),
    ("south east", NodeAnchor::SouthEast),
    ("south west", NodeAnchor::SouthWest),
];

impl NodeAnchor {
    /// Whether the anchor's code opens with `{` and a line end: a space typeset each time it
    /// is computed.
    fn space(self) -> u32 {
        matches!(self, NodeAnchor::Center | NodeAnchor::North | NodeAnchor::South | NodeAnchor::West | NodeAnchor::NorthWest | NodeAnchor::SouthEast) as u32
    }

    /// The circle's: its diagonal anchors' code starts with a line end (probe).
    fn circle_space(self) -> u32 {
        matches!(self, NodeAnchor::NorthWest | NodeAnchor::NorthEast | NodeAnchor::SouthWest | NodeAnchor::SouthEast) as u32
    }
}

/// The text commands a node's text and `font=` may hold: they are read by the machine like
/// any text, and the pre-scan takes them as native.
const TEXT_CS: [&str; 37] = [
    "includegraphics", "sffamily", "rmfamily", "ttfamily", "bfseries", "mdseries", "itshape", "upshape", "slshape", "scshape", "normalfont", "tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large",
    "LARGE", "huge", "Huge", "today", "textbf", "textit", "emph", "par", "vspace", "textsc", "textsf", "textrm", "texttt", "textsl", "textup", "rule", "linewidth", "textwidth", "strut",
];

/// Kernel commands a node's formula may not hold: what numbers, refers, writes, reads files,
/// defines, assigns or ends the paragraph.
const FORMULA_NOT: [&str; 48] = [
    "label", "ref", "pageref", "eqref", "cite", "nocite", "footnote", "footnotemark", "footnotetext", "thanks", "index", "glossary", "marginpar", "def", "edef", "gdef", "xdef", "let", "futurelet", "global", "long", "outer",
    "newcommand", "renewcommand", "providecommand", "newenvironment", "renewenvironment", "newcounter", "setcounter", "stepcounter", "refstepcounter", "addtocounter", "setlength", "addtolength", "write", "immediate", "openout",
    "input", "include", "par", "begin", "end", "makeatletter", "makeatother", "catcode", "csname", "expandafter", "the",
];

/// A node's inline formula: letters, digits, `(),.+-=^_'|<>/*![]:;`, balanced braces, the
/// control symbols `\,` `\;` `\!` `\{` `\}` `\|`, and control words of the kernel's tables
/// that no package changes (`\alpha`, `\infty`, `\frac`, `\le`: `table_names` has the
/// kernel's bit, `blocked_names` none), which the machine sets as in a paragraph. Not the
/// kernel's commands that number, refer, write or define ([`FORMULA_NOT`]): the pre-scan
/// skips an accepted picture, so they would pass by its checks.
fn formula_ok(f: &str) -> bool {
    let b = f.as_bytes();
    let (mut i, mut depth) = (0, 0i32);
    while i < b.len() {
        match b[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            b'\\' if b.get(i + 1).is_some_and(|c| b",;!{}|".contains(c)) => i += 1,
            b'\\' => {
                let n = b[i + 1..].iter().take_while(|c| c.is_ascii_alphabetic()).count();
                let name = &f[i + 1..i + 1 + n];
                if n == 0 || b.get(i + 1 + n) == Some(&b'@') || FORMULA_NOT.contains(&name) || super::super::table_names(name) & super::super::KERNEL_BIT == 0 || super::super::blocked_names(name) != 0 {
                    return false;
                }
                i += n;
            }
            c if c.is_ascii_alphanumeric() || b" (),.+-=^_'|<>/*![]:;".contains(&c) => {}
            _ => return false,
        }
        i += 1;
    }
    depth == 0
}

/// Text of the grammar: plain characters, groups and [`TEXT_CS`]; `None` otherwise.
fn text_ok(s: &str) -> bool {
    let b = s.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            // `\\`, and `\\[<length>]`: LaTeX's line break, the machine's like any text.
            b'\\' if b.get(i + 1) == Some(&b'\\') && b.get(i + 2) != Some(&b'*') => i += 1,
            // `\&`: the character.
            b'\\' if b.get(i + 1) == Some(&b'&') => i += 1,
            b'\\' => {
                let n = b[i + 1..].iter().take_while(|c| c.is_ascii_alphabetic()).count();
                // qrcode's `\qrcode*[options]{text}` (`Machine::qrcode` reads it).
                if &s[i + 1..i + 1 + n] == "qrcode" {
                    let after = &s[i + 1 + n..];
                    let Some(len) = super::super::qrcode::call_len(after) else { return false };
                    i = s.len() - after.len() + len;
                    continue;
                }
                // A name followed by `@` reads differently after `\makeatletter`.
                if !TEXT_CS.contains(&&s[i + 1..i + 1 + n]) || b.get(i + 1 + n) == Some(&b'@') {
                    return false;
                }
                i += n;
            }
            // `$..$`: an inline formula, set by the machine in the node's box like one in a
            // paragraph ([`formula_ok`]).
            b'$' => {
                let Some(k) = b[i + 1..].iter().position(|&c| c == b'$') else { return false };
                if k == 0 || !formula_ok(&s[i + 1..i + 1 + k]) {
                    return false;
                }
                i += k + 1;
            }
            // `|` (OT1's dash), quotes (the font's ligatures, `"` its right double quote), and
            // UTF-8 characters, which inputenc reads as in the text.
            c if c.is_ascii_alphanumeric() || c >= 0x80 || b" \n.,:;!?'`\"()-/+=*@[]~|".contains(&c) => {}
            _ => return false,
        }
        i += 1;
    }
    depth == 0
}

/// `\node[options] at (point) {text};`, read whole.
#[derive(Debug)]
pub(super) struct NodeCmd {
    /// `\tikz@options` in written order: widths, `draw=`, `fill=`, colours.
    opts: Vec<PathOpt>,
    draw: bool,
    fill: bool,
    anchor: NodeAnchor,
    pub(super) at: Pt,
    /// `text=` or a colour key: the colour of the text and of `\pgfsetcolor{.}`.
    text_color: Option<String>,
    /// `font=`, as written.
    font: String,
    pub(super) text: String,
    /// `minimum width`, `minimum height`.
    min: [i32; 2],
    /// `inner xsep`, `inner ysep`; `None` for `.3333em`.
    inner: [Option<i32>; 2],
    /// `text width=`, as TeX reads it (`\pgfmathsetlength`: a length, or a factor of a
    /// length register, which pgfmath hands TeX as `<number>\register`).
    width: Option<String>,
    /// `align=`: the paragraph settings (`\tikz@text@action`) of a node with a text width.
    align: Align,
    /// `rotate=90` (`\tikz@addtransform{\pgftransformrotate{90}}`): pgfmath's sine and cosine
    /// of 90 are `1.0` and `0.0`, so the matrix maps (x, y) to (-y, x) exactly.
    rotate: bool,
    /// Whether `align=` was given: without a text width, tikz then sets the text as an
    /// `\halign` (`\tikz@halign@check`), not of the grammar.
    aligned: bool,
    /// `(name)` or `name=`: the node's rectangle is kept for later points ([`Shape`]).
    pub(super) name: Option<String>,
    /// `above`, `right=<len>`, `below=of <name>` ...: the direction's factors and the
    /// value, read once the options are (`node distance` may follow).
    place: Option<([i32; 2], String)>,
    /// Spaces the node's points typeset before it (an anchor's code, `of`'s scan).
    pub(super) spaces: u32,
    /// `rounded corners[=<r>]`: the background's corners rounded by `r` (no option, no scope).
    round: i32,
    /// `\pgflinewidth` before the node's options: the picture's.
    base_width: i32,
    /// `pos=` (`midway`, `near start`, ...): `\tikz@time` as TeX scans pgfmath's result, in sp.
    time: Option<i32>,
    /// A node of a path (`-- node {..}`): whether it opens and closes the path's nodes, which
    /// share one box after the path.
    pub(super) on_path: Option<(bool, bool)>,
    /// Of a path (`on_path` also groups a node with its labels).
    pub(super) path_node: bool,
    /// `label=<angle>:<text>`: the angle in degrees and the text, in written order.
    labels: Vec<(i32, String)>,
    /// The label nodes made of them once the node is placed: drawn after it.
    pub(super) label_nodes: Vec<NodeCmd>,
    /// tikz's deprecated `<direction> of=<name>` (`\tikz@of`): the node and the angle.
    of: Option<(String, i32)>,
    /// A path node's `above=<length>` and kin: the shift added to its place on the path.
    nudge: (i32, i32),
    /// `circle` (`shape=circle`): pgf's circle shape, else the rectangle.
    circle: bool,
    /// The root of a tree (`child` after it): its `\tikz@figbox` stays open for the children.
    pub(super) tree: bool,
}

/// A named node's rectangle, kept for later points (`\pgf@sh@np@<name>`: the saved anchors
/// `\southwest`, `\northeast` in the node's own coordinates; `\pgf@sh@nt@<name>`: its
/// translation, the matrix being the identity).
#[derive(Clone, Debug)]
pub(super) struct Shape {
    pub(super) name: String,
    shift: [i32; 2],
    outline: Outline,
}

/// A node's saved anchors: the rectangle's `\southwest`, `\northeast`, or the circle's
/// `\centerpoint` and `\radius`, outer sep included.
#[derive(Clone, Copy, Debug)]
enum Outline {
    Rect([i32; 2], [i32; 2]),
    Circle([i32; 2], i32),
}

impl Outline {
    /// Anchor `a` in the node's coordinates.
    fn point(self, a: NodeAnchor) -> [i32; 2] {
        match self {
            Outline::Rect(sw, ne) => anchor_point(a, sw, ne),
            Outline::Circle(c, r) => circle_anchor(a, c, r),
        }
    }

    /// The spaces anchor `a`'s code typesets.
    fn space(self, a: NodeAnchor) -> u32 {
        match self {
            Outline::Rect(..) => a.space(),
            Outline::Circle(..) => a.circle_space(),
        }
    }
}

/// The circle's anchor `a` (pgfmoduleshapes.code.tex:1263-1296): `\centerpoint` moved by the
/// radius, the diagonals by `0.707107` of it.
fn circle_anchor(a: NodeAnchor, [cx, cy]: [i32; 2], r: i32) -> [i32; 2] {
    let k = super::times_frac(b"707107", r);
    match a {
        NodeAnchor::Center => [cx, cy],
        NodeAnchor::North => [cx, cy + r],
        NodeAnchor::South => [cx, cy - r],
        NodeAnchor::West => [cx - r, cy],
        NodeAnchor::East => [cx + r, cy],
        NodeAnchor::NorthWest => [cx - k, cy + k],
        NodeAnchor::NorthEast => [cx + k, cy + k],
        NodeAnchor::SouthWest => [cx - k, cy - k],
        NodeAnchor::SouthEast => [cx + k, cy - k],
    }
}

/// `\pgfpointnormalised` (pgfcorepoints.code.tex:944): the cosine and sine of pgfmath's
/// `atan2` of the point.
fn normalised(x: i32, y: i32) -> (i32, i32) {
    let a = super::pgfmath::atan2(y, x);
    (super::pgfmath::cos(a), super::pgfmath::sin(a))
}

/// `<r as \pgf@sys@tonumber>\dimen`: TeX's factor of `r`'s printed value times `v`.
fn times_printed(r: i32, v: i32) -> i32 {
    (r >> 16) * v + xn_over_d(v, r & 0xffff, 65536)
}

/// The rectangle's anchor `a` from its saved anchors (pgfmoduleshapes.code.tex:1012-1068).
fn anchor_point(a: NodeAnchor, sw: [i32; 2], ne: [i32; 2]) -> [i32; 2] {
    match a {
        NodeAnchor::Center => [half(sw[0]) + half(ne[0]), half(sw[1]) + half(ne[1])],
        NodeAnchor::North => [half(ne[0]) + half(sw[0]), ne[1]],
        NodeAnchor::South => [half(sw[0]) + half(ne[0]), sw[1]],
        NodeAnchor::West => [sw[0], half(sw[1]) + half(ne[1])],
        NodeAnchor::East => [ne[0], half(ne[1]) + half(sw[1])],
        NodeAnchor::NorthWest => [sw[0], ne[1]],
        NodeAnchor::NorthEast => ne,
        NodeAnchor::SouthEast => [ne[0], sw[1]],
        NodeAnchor::SouthWest => sw,
    }
}

impl Shape {
    /// `\pgfpointanchor{name}{a}`: the anchor through the node's translation, and the spaces
    /// its code typesets.
    pub(super) fn anchor(&self, a: &str) -> Option<((i32, i32), u32)> {
        // The rectangle's `base` (pgfmoduleshapes.code.tex:1026), the circle's the same
        // (:1250): its centre's code, then `\pgf@y=0pt`, the text's baseline.
        if a == "base" {
            let c = self.outline.point(NodeAnchor::Center);
            return Some(((c[0] + self.shift[0], self.shift[1]), self.outline.space(NodeAnchor::Center)));
        }
        let a = ANCHORS.iter().find(|(n, _)| *n == a)?.1;
        let p = self.outline.point(a);
        Some(((p[0] + self.shift[0], p[1] + self.shift[1]), self.outline.space(a)))
    }

    /// The spaces a border point typesets: the rectangle's two (its `\anchorborder`'s line end
    /// and the shape scan's), the circle's none (probe).
    pub(super) fn border_spaces(&self) -> u32 {
        2 * self.scan_space()
    }

    /// The space the shape's scan for an anchor typesets (`of name` reads one): the
    /// rectangle's one, the circle's none (probe).
    pub(super) fn scan_space(&self) -> u32 {
        match self.outline {
            Outline::Rect(..) => 1,
            Outline::Circle(..) => 0,
        }
    }

    /// `\pgfpointshapeborder{name}{t}` (pgfmoduleshapes.code.tex:597): `t` into the node's
    /// coordinates less its `center`, the rectangle's `\anchorborder` (:1069) and back.
    /// `None` near the centre (pgf returns the centre with a warning).
    pub(super) fn border(&self, (tx, ty): (i32, i32)) -> Option<(i32, i32)> {
        let c = self.outline.point(NodeAnchor::Center);
        let (xa, ya) = (tx - self.shift[0] - c[0], ty - self.shift[1] - c[1]);
        if xa.abs() < 1 << 16 && ya.abs() < 1 << 16 {
            return None;
        }
        Some(self.border_dir((xa, ya)))
    }

    /// A numeric anchor `<name>.<a>` (pgfmoduleshapes.code.tex:487 `\pgf@sh@reanchor`): the
    /// border toward `\pgfqpointpolar{a}{1pt}`, `a` whole degrees.
    pub(super) fn border_angle(&self, a: i32) -> (i32, i32) {
        let a = a << 16;
        self.border_dir((super::pgfmath::cos(a), super::pgfmath::sin(a)))
    }

    /// The shape's `\anchorborder` toward direction `(xa, ya)` from its centre, on the
    /// picture.
    fn border_dir(&self, (xa, ya): (i32, i32)) -> (i32, i32) {
        match self.outline {
            Outline::Rect(sw, ne) => {
                let h = [half(ne[0] - sw[0]), half(ne[1] - sw[1])];
                let (x, y) = border_rectangle((xa, ya), (h[0], h[1]));
                (x + sw[0] + h[0] + self.shift[0], y + sw[1] + h[1] + self.shift[1])
            }
            // The circle's `\anchorborder` (:1297): `\pgfpointborderellipse` of equal radii,
            // the direction normalised and scaled by the printed radius, then the centre.
            Outline::Circle(c, r) => {
                let (nx, ny) = normalised(xa, ya);
                (c[0] + times_printed(r, nx) + self.shift[0], c[1] + times_printed(r, ny) + self.shift[1])
            }
        }
    }
}

/// `\pgfpointborderrectangle` (pgfcorepoints.code.tex:972): where the ray from the origin
/// toward `d` leaves the rectangle of upper right corner `(xb, yb)`, by TeX's arithmetic
/// (`.125` of each direction as an integer, `8192pt` divided by it, `\pgf@sys@tonumber`
/// factors).
fn border_rectangle((x0, y0): (i32, i32), (xb, yb): (i32, i32)) -> (i32, i32) {
    let (mut x, mut y) = (x0.abs(), y0.abs());
    let ca = super::times_frac(b"125", x);
    let cb = super::times_frac(b"125", y);
    const K: i32 = 8192 << 16;
    if cb < ca {
        if ca < 255 {
            (x, y) = (0, yb);
        } else {
            y = super::fmul(xb, super::fmul(K / ca, y));
            if y < yb {
                x = xb;
            } else if cb < 255 {
                (x, y) = (xb, 0);
            } else {
                x = super::fmul(yb, super::fmul(K / cb, x));
                y = yb;
            }
        }
    } else if cb < 255 {
        (x, y) = (xb, 0);
    } else {
        x = super::fmul(yb, super::fmul(K / cb, x));
        if x < xb {
            y = yb;
        } else if ca < 255 {
            (x, y) = (0, yb);
        } else {
            y = super::fmul(xb, super::fmul(K / ca, y));
            x = xb;
        }
    }
    (if x0 < 0 { -x } else { x }, if y0 < 0 { -y } else { y })
}

/// `align=` with a text width (tikz.code.tex:1135-1150, `\tikz@text@action`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Align {
    /// `text ragged`, also the default.
    Left,
    /// `text centered`.
    Center,
    /// `text ragged left`.
    Right,
}

/// A `text width=` value: a length, or a decimal factor of `\textwidth`, `\linewidth`,
/// `\paperwidth`, `\columnwidth` (pgfmathparser.code.tex:444 `\pgfmath@dimen=\pgfmath@number
/// \pgfmath@token`: TeX's own `<factor><dimen>`).
fn text_width(v: &str) -> Option<String> {
    if length(v).is_some() {
        return Some(v.to_string());
    }
    let k = v.find('\\')?;
    let (num, reg) = v.split_at(k);
    let num = num.trim();
    if !num.bytes().all(|c| c.is_ascii_digit() || c == b'.') || num.matches('.').count() > 1 || num == "." || !matches!(reg.trim(), "\\textwidth" | "\\linewidth" | "\\paperwidth" | "\\columnwidth") {
        return None;
    }
    Some(format!("{num}{}", reg.trim()))
}

/// A `minimum width` (pgfmodulenodes `\pgfmathsetlength` of the key's value): a length, or
/// pgfmath of one with the page's registers (`0.43\textwidth`).
fn min_length(v: &str, ctx: &Ctx) -> Option<i32> {
    // `<number>em`: `\pgfmathsetlength`'s quick path, TeX's scan of the unit in the font
    // around the node (its `font=` is the text box's only).
    let em = |v: &str| {
        let num = v.trim().strip_suffix("em")?;
        (!num.is_empty() && num.bytes().all(|c| c.is_ascii_digit() || c == b'.') && num.matches('.').count() <= 1).then_some(())?;
        let sp = crate::backend::backend_scaled::scan_dimen(v, ctx.em as i64, 0)?;
        (sp.abs() < 1 << 28).then_some(sp as i32)
    };
    length(v).or_else(|| em(v)).or_else(|| {
        let reg = |name: &str| super::REGISTERS.contains(&name).then(|| (ctx.dims)(name)).flatten();
        let (v, units) = super::pgfmath::parse(v, &reg)?;
        units.then_some(v).filter(|v| v.abs() < 1 << 28)
    })
}

/// The placement keys (tikzlibrarypositioning.code.tex:12-23): the anchor they set and the
/// direction's factors. Diagonal ones read a value differently with the library and without:
/// only bare.
const PLACES: [(&str, NodeAnchor, [i32; 2]); 8] = [
    ("above", NodeAnchor::South, [0, 1]),
    ("below", NodeAnchor::North, [0, -1]),
    ("left", NodeAnchor::East, [-1, 0]),
    ("right", NodeAnchor::West, [1, 0]),
    ("above left", NodeAnchor::SouthEast, [0, 0]),
    ("above right", NodeAnchor::SouthWest, [0, 0]),
    ("below left", NodeAnchor::NorthEast, [0, 0]),
    ("below right", NodeAnchor::NorthWest, [0, 0]),
];

/// A node name of letters, digits, `_` and `-`, words joined by single spaces (`(step 1)`,
/// `(x_0)`: tikz keeps the name as written, `\pgf@sh@ns@step 1`).
fn node_name(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty() && !s.contains("  ") && s.chars().all(|ch| ch.is_ascii_alphanumeric() || " _-".contains(ch))).then(|| s.to_string())
}

/// `\tikz@lib@place@handle@@`: a value `<len>` shifts the node by it along the direction
/// (`\pgftransformshift{\pgfqpoint{f_x len}{f_y len}}`); `[<len>] of <name>` puts it at the
/// named node's opposite anchor and shifts it by the length or `node distance`
/// (`\tikz@lib@place@of@`: the name scanned as a shape, its `center` computed: one space).
fn place(n: &mut NodeCmd, f: [i32; 2], v: &str, ctx: &mut Ctx, at: bool) -> Option<()> {
    if f == [0, 0] {
        return None;
    }
    let (dist, of) = match v.split_once("of ") {
        Some((d, name)) => (d.trim(), Some(name.trim())),
        None => (v, None),
    };
    let d = if dist.is_empty() { if f[0] != 0 { ctx.distance[0] } else { ctx.distance[1] } } else { length(dist)? };
    let Pt::At(mut x, mut y) = n.at else { return None };
    if let Some(name) = of {
        if at {
            return None;
        }
        let opposite = match n.anchor {
            NodeAnchor::South => "north",
            NodeAnchor::North => "south",
            NodeAnchor::East => "west",
            _ => "east",
        };
        let shape = ctx.nodes.iter().find(|s| s.name == name)?;
        let ((ax, ay), sp) = shape.anchor(opposite)?;
        (x, y) = (ax, ay);
        n.spaces += shape.scan_space() + sp;
    }
    n.at = Pt::At(x.checked_add(f[0] * d)?, y.checked_add(f[1] * d)?);
    Some(())
}

/// The text up to the `]` closing a node's options: commands allowed (`font=`), no brackets
/// or braces.
fn options_text<'a>(c: &mut Cursor<'a>) -> Option<&'a str> {
    let k = c.s.find(']')?;
    let inner = &c.s[..k];
    if inner.contains(['[', '{', '}', '%']) {
        return None;
    }
    c.s = &c.s[k + 1..];
    Some(inner)
}

/// A node's options into `n`.
/// A `pos=` value: a plain number from 0 to 1, as sp of the `pt` TeX reads it as.
fn time(v: &str) -> Option<i32> {
    if v.is_empty() || !v.bytes().all(|c| c.is_ascii_digit() || c == b'.') {
        return None;
    }
    let t = crate::backend::backend_scaled::scan_dimen(&format!("{v}pt"), 0, 0)?;
    (0..=65536).contains(&t).then_some(t as i32)
}

fn node_options(list: &str, n: &mut NodeCmd, ctx: &Ctx) -> Option<()> {
    for item in list.split(',') {
        let item = item.trim();
        if let Some((k, v)) = item.split_once('=') {
            let v = v.trim();
            match k.trim() {
                "anchor" => n.anchor = ANCHORS.iter().find(|(a, _)| *a == v)?.1,
                "text" => n.text_color = Some(color_spec(v)?),
                "font" => {
                    if !v.starts_with('\\') || !text_ok(v) || v.contains(['{', '}']) {
                        return None;
                    }
                    n.font = v.to_string();
                }
                "line width" => n.opts.push(PathOpt::Width(super::width_value(v)?)),
                "draw" => {
                    n.draw = true;
                    n.opts.push(PathOpt::Stroke(color_spec(v)?));
                }
                "fill" => {
                    n.fill = true;
                    n.opts.push(PathOpt::Fill(color_spec(v)?));
                }
                "minimum width" => n.min[0] = min_length(v, ctx)?,
                "minimum height" => n.min[1] = min_length(v, ctx)?,
                "minimum size" => n.min = [min_length(v, ctx)?; 2],
                // `\pgfmathsetlength` of the value: a plain number is that many points
                // (`inner sep=0`).
                "inner xsep" => n.inner[0] = Some(super::width_value(v)?),
                "inner ysep" => n.inner[1] = Some(super::width_value(v)?),
                "inner sep" => n.inner = [Some(super::width_value(v)?); 2],
                "text width" => n.width = Some(text_width(v)?),
                "align" => {
                    n.align = match v {
                        "left" => Align::Left,
                        "center" => Align::Center,
                        "right" => Align::Right,
                        _ => return None,
                    };
                    n.aligned = true;
                }
                "rotate" if v == "90" => n.rotate = true,
                "rounded corners" => n.round = length(v).filter(|r| *r > 0)?,
                "shape" => {
                    n.circle = match v {
                        "circle" => true,
                        "rectangle" => false,
                        _ => return None,
                    }
                }
                // tikz.code.tex:1099 `\pgfmathsetmacro\tikz@time`: a number, printed back as
                // pgfmath prints it, read again as `\pgf@temp\pgf@xa` reads it.
                "pos" => n.time = Some(time(v)?),
                "label" => n.labels.push(label_spec(v)?),
                k if OLD_OF.iter().any(|(o, _)| *o == k) => {
                    n.anchor = NodeAnchor::Center;
                    n.of = Some((node_name(v)?, OLD_OF.iter().find(|(o, _)| *o == k)?.1));
                }
                "name" => n.name = Some(node_name(v)?),
                k if PLACES.iter().any(|(p, ..)| *p == k) && n.place.is_none() => {
                    let (_, a, f) = PLACES.iter().find(|(p, ..)| *p == k)?;
                    n.anchor = *a;
                    n.place = Some((*f, v.to_string()));
                }
                _ => return None,
            }
        } else if let Some((_, a, _)) = PLACES.iter().find(|(p, ..)| *p == item) {
            // Without a value: the anchor only (tikz.code.tex `\tikz@def@place`, the same in
            // the positioning library).
            n.anchor = *a;
        } else if let Some((_, w)) = WIDTHS.iter().find(|(name, _)| *name == item) {
            n.opts.push(PathOpt::Width(length(w)?));
        } else {
            match item {
                "draw" => n.draw = true,
                "fill" => n.fill = true,
                "rounded corners" => n.round = 4 << 16,
                // tikz.code.tex:1142: `\tikz@text@action` only, run in a text width's minipage.
                "text centered" => n.align = Align::Center,
                "circle" => n.circle = true,
                "rectangle" => n.circle = false,
                // tikz.code.tex:1614-1620, styles of `pos=`.
                "at start" => n.time = Some(0),
                "very near start" => n.time = Some(8192),
                "near start" => n.time = Some(16384),
                "midway" => n.time = Some(32768),
                "near end" => n.time = Some(49152),
                "very near end" => n.time = Some(57344),
                "at end" => n.time = Some(65536),
                // tikz's `.unknown` colour: `\tikz@addoption{\pgfutil@color}` and the text's.
                _ if !ANCHORS.iter().any(|(a, _)| *a == item) && !matches!(item, "solid" | "none" | "circle" | "rectangle" | "rotate" | "align" | "overlay") => {
                    let c = color_spec(item)?;
                    n.text_color = Some(c.clone());
                    n.opts.push(PathOpt::Color(c));
                }
                _ => return None,
            }
        }
    }
    Some(())
}

/// One `\node` after its name: options and `at (point)` in either order, then `{text}` and
/// `;`. Overlay pictures and `current page` points are not of the grammar yet.
pub(super) fn parse(c: &mut Cursor, ctx: &mut Ctx, overlay: bool) -> Option<NodeCmd> {
    let (mut n, at) = body(c, ctx, overlay, false)?;
    // A tree's root (`\node .. child {..}`, tikz.code.tex:4549 `\tikz@children`): its
    // children are read by the picture's reader, under the identity only.
    n.tree = !c.eat(";");
    let word = |c: &Cursor| c.s.starts_with("child") && !c.s[5..].starts_with(|ch: char| ch.is_ascii_alphabetic());
    if n.time.is_some() || n.tree && (!word(c) || ctx.tf != super::Tf::ID || !n.labels.is_empty()) {
        return None;
    }
    // Its children's edges start at its border: unnamed, it is kept under a name of no source.
    if n.tree && n.name.is_none() {
        n.name = Some(format!("\u{2}{}", ctx.node_count));
    }
    // Under a picture's transformation (`scale=`, `rotate=`) only the node's place moves
    // (`\pgftransformshift` of its point, then `\pgftransformresetnontranslations`): its
    // text and shape stay.
    if ctx.tf != super::Tf::ID {
        let Pt::At(x, y) = n.at else { return None };
        // A label's point is taken back through the inverse and out again (rounding): not
        // taken.
        if n.of.is_some() || n.place.is_some() || !n.labels.is_empty() {
            return None;
        }
        let (x, y) = ctx.tf.apply(x, y);
        n.at = Pt::At(x, y);
    }
    if let Some((name, angle)) = n.of.take() {
        // `\tikz@node@at` is the named node's centre (its anchor's code typeset), the
        // shift `\pgfpointpolar` of the distance.
        if at || n.place.is_some() {
            return None;
        }
        let ((cx, cy), sp) = ctx.nodes.iter().find(|s| s.name == name)?.anchor("center")?;
        let (d, a) = (ctx.distance[1], angle << 16);
        n.at = Pt::At(cx + factor_times(super::pgfmath::cos(a), d), cy + factor_times(super::pgfmath::sin(a), d));
        n.spaces += sp;
    }
    if let Some((f, v)) = n.place.clone() {
        place(&mut n, f, &v, ctx, at)?;
    }
    if n.name.is_some() {
        // A named node's shape: no rotation, no page point (`\pgf@sh@nt@` would turn it, the
        // interpicture shift move it), no name met twice.
        let Pt::At(x, y) = n.at else { return None };
        if n.rotate || ctx.nodes.iter().any(|s| Some(&s.name) == n.name.as_ref()) {
            return None;
        }
        let k = ctx.node_count;
        let shape = n.shape((ctx.boxes)(k), ctx.em, (x, y))?;
        // `\tikz@children@collected`'s `\pgfpointanchor{\tikzparentnode}{center}`: the
        // anchor's code typesets its spaces before the node's figbox.
        if n.tree {
            n.spaces += shape.anchor("center")?.1;
        }
        ctx.nodes.push(shape);
    }
    let k = ctx.node_count;
    ctx.node_count += 1;
    if !n.labels.is_empty() {
        labels(&mut n, k, ctx)?;
    }
    Some(n)
}

/// The node's labels (`append after command`): each a node at the node's border in its
/// direction (`\pgfpointanchor{\tikzlastnode}{<angle>}`, the shape's `\anchorborder` of
/// `\pgfqpointpolar{<angle>}{1pt}`), its anchor the one facing the node (the label
/// distance 0pt, so no shift). They are drawn with the node in its box; each typesets two
/// spaces and the node's `center` anchor's before the node (probe).
fn labels(n: &mut NodeCmd, k: usize, ctx: &mut Ctx) -> Option<()> {
    let Pt::At(x, y) = n.at else { return None };
    if n.rotate {
        return None;
    }
    let (outline, _) = n.frame((ctx.boxes)(k), ctx.em, n.line_width());
    let shape = Shape { name: String::new(), shift: n.shift(outline, (x, y)), outline };
    let c = shape.anchor("center")?.0;
    let count = n.labels.len();
    for (i, (angle, text)) in std::mem::take(&mut n.labels).into_iter().enumerate() {
        let a = angle << 16;
        let p = shape.border_dir((super::pgfmath::cos(a), super::pgfmath::sin(a)));
        let (nx, ny) = normalised(p.0 - c.0, p.1 - c.1);
        let mut l = every_node(ctx)?;
        l.text = text;
        l.anchor = auto_anchor(ny, -nx);
        l.at = Pt::At(p.0, p.1);
        l.on_path = Some((false, i + 1 == count));
        n.label_nodes.push(l);
        ctx.node_count += 1;
    }
    n.spaces += count as u32 * (2 + outline.space(NodeAnchor::Center));
    n.on_path = Some((true, false));
    Some(())
}

/// A `node` of a path after `node` (`\tikz@collect@label@onpath`): options and `{text}`,
/// no name, no `at`, no positioning by a node; its place is set by the path, an
/// `above=<length>` (and kin) moving it from there by the length.
pub(super) fn parse_on_path(c: &mut Cursor, ctx: &Ctx, overlay: bool) -> Option<NodeCmd> {
    if ctx.auto {
        return None;
    }
    let (mut n, _) = body(c, ctx, overlay, true)?;
    if let Some((f, v)) = n.place.take() {
        if v.contains("of") || v.trim().is_empty() {
            return None;
        }
        let d = length(v.trim())?;
        n.nudge = (f[0] * d, f[1] * d);
    }
    (n.name.is_none() && n.of.is_none() && n.labels.is_empty() && !overlay).then_some(n)
}

/// A tree's child node (tikz.code.tex:4639 `\tikz@childnode`): `node [options] (name)
/// {text}` after `child {`, a `\node` statement of its own at `at` (the parent's centre
/// shifted by `\tikz@grow`, [`child_point`]), named `name` (`\tikzparentnode-<k>`) unless
/// it names itself. No `at`, positioning, labels or `pos=`.
pub(super) fn child(c: &mut Cursor, ctx: &mut Ctx, overlay: bool, (x, y): (i32, i32), name: String) -> Option<NodeCmd> {
    let (mut n, at) = body(c, ctx, overlay, false)?;
    if at || n.time.is_some() || n.of.is_some() || n.place.is_some() || !n.labels.is_empty() || n.rotate || overlay {
        return None;
    }
    n.at = Pt::At(x, y);
    let name = n.name.get_or_insert(name).clone();
    if ctx.nodes.iter().any(|s| s.name == name) {
        return None;
    }
    let shape = n.shape((ctx.boxes)(ctx.node_count), ctx.em, (x, y))?;
    ctx.nodes.push(shape);
    ctx.node_count += 1;
    Some(n)
}

/// The spaces a child's `node` forms typeset (tikz.code.tex:4718 `\tikz@p@c@s`): one per
/// `(name)`, `[options]` and the `{text}`, each macro's line ending without `%`; `at` not
/// taken.
pub(super) fn child_forms(s: &str) -> Option<u32> {
    let mut c = Cursor { s };
    let mut k = 0;
    loop {
        c.blanks()?;
        k += 1;
        if c.eat("[") {
            options_text(&mut c)?;
        } else if c.eat("(") {
            c.parenthesized()?;
        } else {
            return c.s.starts_with('{').then_some(k);
        }
    }
}

/// `\tikz@grow@direction` (tikz.code.tex:1436), the growth's angle `a` (`grow=down` -90):
/// child `k` of `n` at the parent's centre shifted by `\pgfpointpolar{a}{<level distance>}`,
/// by `\pgfpointpolar{a-90}{(n+1)\pgf@xc}` with `\pgf@xc=.5\tikzsiblingdistance`, and by
/// `\pgfpointpolar{a+90}{k\tikzsiblingdistance}`; each `\pgftransformshift` adds exactly under
/// the identity, each polar point is pgfmath's cosine and sine times the length.
pub(super) fn child_point((x, y): (i32, i32), k: i32, n: i32, [ld, sd, grow]: [i32; 3]) -> Option<(i32, i32)> {
    let half = times_printed(32768, sd);
    let mut p = (x, y);
    // `\tikz@angle@grow`, `@left` (less 90), `@right` (more 90).
    for (a, d) in [(grow, ld), (grow - 90, (n + 1).checked_mul(half)?), (grow + 90, k.checked_mul(sd)?)] {
        let a = a << 16;
        p = (p.0.checked_add(factor_times(super::pgfmath::cos(a), d))?, p.1.checked_add(factor_times(super::pgfmath::sin(a), d))?);
    }
    Some(p)
}

/// `grow=`: `\tikz@grow@direction@<name>` or a whole number of degrees.
pub(super) fn grow_angle(v: &str) -> Option<i32> {
    let v = v.trim();
    Some(match v {
        "down" | "south" => -90,
        "up" | "north" => 90,
        "left" | "west" => 180,
        "right" | "east" => 0,
        "north east" => 45,
        "north west" => 135,
        "south east" => -45,
        "south west" => -135,
        _ => v.parse::<i32>().ok().filter(|a| a.abs() <= 3600)?,
    })
}

/// `level distance=`, `sibling distance=`: `\pgfmathsetlength`, `em` in the picture's font.
pub(super) fn tree_length(v: &str, ctx: &Ctx) -> Option<i32> {
    min_length(v, ctx)
}

/// A node of no options at the origin.
fn blank(ctx: &Ctx) -> NodeCmd {
    NodeCmd { opts: Vec::new(), draw: false, fill: false, anchor: NodeAnchor::Center, at: Pt::At(0, 0), text_color: None, font: String::new(), text: String::new(), min: [65536; 2], inner: [None; 2], width: None, align: Align::Left, aligned: false, rotate: false, name: None, place: None, spaces: 0, round: 0, base_width: ctx.width, time: None, on_path: None, path_node: false, labels: Vec::new(), label_nodes: Vec::new(), of: None, nudge: (0, 0), circle: false, tree: false }
}

/// A node with the picture's `every node` style applied (`\tikz@node@options` runs
/// `every node/.try` before the node's own options; probe: the same as writing them).
fn every_node(ctx: &Ctx) -> Option<NodeCmd> {
    let mut n = blank(ctx);
    if let Some((_, v)) = ctx.styles.iter().rev().find(|(k, _)| k == "every node") {
        node_options(&super::styled(v, ctx), &mut n, ctx)?;
    }
    Some(n)
}

/// tikz.code.tex:1027-1042, the deprecated placements: `<key> of=<name>` at the node's
/// `center` moved by `\pgfpointpolar{<angle>}{<node distance's first value>}`.
const OLD_OF: [(&str, i32); 8] = [("above of", 90), ("below of", -90), ("left of", 180), ("right of", 0), ("above left of", 135), ("below left of", -135), ("above right of", 45), ("below right of", -45)];

/// TeX's `<factor><dimen>` of a factor printed by pgfmath, negative ones too.
pub(super) fn factor_times(f: i32, v: i32) -> i32 {
    if f < 0 { -times_printed(-f, v) } else { times_printed(f, v) }
}

/// `label=` (tikz.code.tex:825-914): `<position>:<text>`, or the text alone above; the
/// position a direction word (`\tikz@label@angle@is@<word>`) or whole degrees.
fn label_spec(v: &str) -> Option<(i32, String)> {
    let (pos, text) = v.split_once(':').unwrap_or(("above", v));
    let angle = match pos.trim() {
        "right" => 0,
        "above right" => 45,
        "above" => 90,
        "above left" => 135,
        "left" => 180,
        "below left" => 225,
        "below" => 270,
        "below right" => 315,
        a => a.parse::<i32>().ok().filter(|d| (0..360).contains(d))?,
    };
    (!text.is_empty() && text_ok(text) && !text.contains(['{', '}', '\n'])).then(|| (angle, text.to_string()))
}

/// `\tikz@auto@anchor` (tikz.code.tex:4485) of the label's direction turned, `(y, -x)`: the
/// anchor facing the node.
fn auto_anchor(x: i32, y: i32) -> NodeAnchor {
    const E: i32 = 3277; // 0.05pt
    if x > E {
        if y > E { NodeAnchor::SouthEast } else if y < -E { NodeAnchor::SouthWest } else { NodeAnchor::South }
    } else if x < -E {
        if y > E { NodeAnchor::NorthEast } else if y < -E { NodeAnchor::NorthWest } else { NodeAnchor::North }
    } else if y > 0 {
        NodeAnchor::East
    } else {
        NodeAnchor::West
    }
}

/// Options, name and `at (point)` in either order, then `{text}`.
fn body(c: &mut Cursor, ctx: &Ctx, overlay: bool, on_path: bool) -> Option<(NodeCmd, bool)> {
    if c.s.starts_with(|ch: char| ch.is_ascii_alphabetic()) {
        return None;
    }
    let mut n = every_node(ctx)?;
    let mut at = false;
    loop {
        c.blanks()?;
        // `\tikz@fig@scan@options`: each `[..]` group is `\tikzset` in turn, then the scan
        // goes on (tikz.code.tex:3834), so `[draw][red]` is `[draw,red]`.
        if c.eat("[") {
            node_options(&super::styled(options_text(c)?, ctx), &mut n, ctx)?;
        } else if n.name.is_none() && c.eat("(") {
            n.name = Some(node_name(c.parenthesized()?)?);
        } else if !at && !on_path && c.s.starts_with("at") && !c.s[2..].starts_with(|ch: char| ch.is_ascii_alphabetic()) {
            c.eat("at");
            c.blanks()?;
            // A point of the picture's grammar, but not one relative to tikz's last point.
            if !c.s.starts_with('(') {
                return None;
            }
            n.at = point(c, ctx, (0, 0))?.0;
            n.spaces += ctx.spaces.take();
            // `at (name)`: a node's centre, scanned as a shape's border.
            if ctx.shape_ref.take().is_some() {
                return None;
            }
            if matches!(n.at, Pt::Page(_)) && !overlay {
                return None;
            }
            at = true;
        } else {
            break;
        }
    }
    if !c.eat("{") {
        return None;
    }
    // The text to its closing brace.
    let mut depth = 1;
    let k = c.s.char_indices().find(|&(_, ch)| {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        depth == 0
    })?.0;
    let text = &c.s[..k];
    // A blank line would be `\par`.
    if !text_ok(text) || text.contains("\n\n") || text.lines().any(|l| l.trim().is_empty() && !l.is_empty()) {
        return None;
    }
    n.text = text.to_string();
    c.s = &c.s[k + 1..];
    c.blanks()?;
    // `align=` without a text width: the lines at `\\` set as an `\halign`, of text
    // without colour (its `\reset@color` would come between the lines' groups).
    if n.aligned && n.width.is_none() && (n.text_color.is_some() || align_lines(&n.text).is_none()) {
        return None;
    }
    // A rotated background path is `\pgf@specialrect`'s moveto and linetos: not drawn yet.
    if n.rotate && (n.draw || n.fill) {
        return None;
    }
    // In a `remember picture` picture the background's `re` would move with the picture
    // (`\pgf@shape@interpictureshift`): only unpainted nodes there yet.
    if overlay && (n.draw || n.fill || n.rotate) {
        return None;
    }
    // A circle neither turned nor rounded.
    if n.circle && (n.rotate || n.round > 0) {
        return None;
    }
    Some((n, at))
}

impl NodeCmd {
    /// Puts a path's node at `\pgftransformlineattime{t}{a}{b}` (pgfcorepoints.code.tex:445):
    /// `a` plus `t` times `b - a`, each product as TeX's `<factor><dimen>`; without `pos=`,
    /// `t` is `\tikz@time`'s `.5` (a node within a segment) or the point itself (`pos` absent
    /// after a point: `at` is that point).
    pub(super) fn put(&mut self, a: (i32, i32), b: (i32, i32), mid: bool) {
        let along = |t: i32, d: i32| (t >> 16) * d + xn_over_d(d, t & 0xffff, 65536);
        let t = match (self.time, mid) {
            (Some(t), _) => t,
            (None, true) => 32768,
            (None, false) => 65536,
        };
        self.at = Pt::At(a.0 + along(t, b.0 - a.0) + self.nudge.0, a.1 + along(t, b.1 - a.1) + self.nudge.1);
    }

    /// On a curve from `a` with supports `q[0]`, `q[1]` to `q[2]` at its time (`midway`
    /// 0.5): `\pgfpointcurveattime` (pgfcorepoints.code.tex:528), three rounds of
    /// `t·p + s·q` with `t` the printed `1pt` less `s`.
    pub(super) fn put_curve(&mut self, a: (i32, i32), q: [(i32, i32); 3], mid: bool) {
        let along = |t: i32, d: i32| (t >> 16) * d + xn_over_d(d, t & 0xffff, 65536);
        let s = match (self.time, mid) {
            (Some(t), _) => t,
            (None, true) => 32768,
            (None, false) => 65536,
        };
        let t = 65536 - s;
        let mix = |p: i32, r: i32| along(t, p) + along(s, r);
        let at = |a: i32, b: i32, c: i32, e: i32| {
            let (mut x, mut xa, xb) = (mix(a, b), mix(b, c), mix(c, e));
            x = mix(x, xa);
            xa = mix(xa, xb);
            mix(x, xa)
        };
        let x = at(a.0, q[0].0, q[1].0, q[2].0);
        let y = at(a.1, q[0].1, q[1].1, q[2].1);
        self.at = Pt::At(x + self.nudge.0, y + self.nudge.1);
    }

    /// Whether a placement key names a node (`right=of a`, the positioning library's).
    pub(super) fn placed_of(&self) -> bool {
        self.place.as_ref().is_some_and(|(_, v)| v.contains("of "))
    }

    /// Whether a placement key with a length (`above=4pt`) placed it: without the
    /// positioning library tikz's `\tikz@possibly@transform` shift takes back a pending
    /// space (probe); the library's handler does not.
    pub(super) fn place_unskip(&self) -> bool {
        self.place.as_ref().is_some_and(|(_, v)| !v.contains("of ") && !v.trim().is_empty())
    }

    /// Whether `above=<length>` (or kin) moves it from its place on the path.
    pub(super) fn nudged(&self) -> bool {
        self.nudge != (0, 0)
    }

    /// Whether `pos=` was given.
    pub(super) fn timed(&self) -> bool {
        self.time.is_some()
    }
}

/// First box register of the nodes' text boxes, one per node.
pub(super) const NODE_BOX: u32 = 32600;
/// At most this many nodes in a picture.
pub(super) const MAX_NODES: usize = 100;

/// The source that typesets node `k`'s text box into `\box(NODE_BOX + k)`
/// (`\tikz@do@fig`): `\hbox{font{\color{c}\ignorespaces text\unskip}}`; the `\unskip` is
/// tikz's `\aftergroup\unskip`, which comes before `\color`'s `\reset@color`.
pub(super) fn text_box_source(n: &NodeCmd, k: usize, paper_width: i32, out: &mut String) {
    let reg = NODE_BOX + k as u32;
    // `\paperwidth` is not the machine's: its factor is taken here, as TeX takes it, of the
    // paper width (`\pdfpagewidth`, which pdftex.def sets to it).
    let width = n.width.as_ref().map(|w| match w.strip_suffix("\\paperwidth") {
        Some(num) => {
            let f = crate::backend::backend_scaled::scan_dimen(&format!("{}pt", if num.is_empty() { "1" } else { num }), 0, 0).unwrap_or(0) as i64;
            format!("{}sp", crate::backend::backend_scaled::times_unit(f >> 16, (f & 0xffff) as i32, paper_width as i64))
        }
        None => w.clone(),
    });
    out.push_str(&format!("\\setbox{reg}\\hbox{{"));
    if n.aligned && width.is_none() {
        align_source(n, out);
        return;
    }
    // With a text width: `\begingroup\pgfutil@minipage[t]{\pgf@x}\leavevmode\hbox{}`, the
    // font, `\tikz@text@action`; `\begin{minipage}` makes the same boxes, and `\leavevmode` is
    // `\unhbox\voidb@x` (here a register never set).
    if let Some(w) = &width {
        out.push_str(&format!("\\begingroup\\begin{{minipage}}[t]{{{w}}}\\unhbox{}\\hbox{{}}{}", NODE_BOX - 1, n.font));
        out.push_str(match n.align {
            Align::Left => "\\raggedright\\rightskip0pt plus2em \\spaceskip.3333em \\xspaceskip.5em\\relax ",
            Align::Right => "\\raggedleft\\leftskip0pt plus2em \\spaceskip.3333em \\xspaceskip.5em\\relax ",
            // `\let\\=\@centercr` is `\centering`'s, whose other settings are set again or
            // already so in the minipage, but for `\finalhyphendemerits`, kept.
            Align::Center => "\\expandafter\\centering\\expandafter\\finalhyphendemerits\\the\\finalhyphendemerits\\relax\\leftskip0pt plus2em\\rightskip0pt plus2em\\spaceskip.3333em \\xspaceskip.5em\\parfillskip=0pt\\hbadness10000\\relax ",
        });
    } else {
        out.push_str(&n.font);
    }
    out.push('{');
    if let Some(c) = &n.text_color {
        out.push_str(&format!("\\color{{{c}}}"));
    }
    out.push_str("\\ignorespaces ");
    out.push_str(&n.text);
    out.push_str("\\unskip}");
    if let Some(w) = &width {
        // `\pgfutil@endminipage\endgroup`, then `\wd\pgfnodeparttextbox=\pgf@x`.
        out.push_str(&format!("\\end{{minipage}}\\endgroup}}\\wd{reg}={w}\\relax%\n"));
    } else {
        out.push_str("}%\n");
    }
}

/// The lines of an aligned node's text at its top-level `\\`, each with the skip before
/// it (`\\[<len>]`, `0pt` without); `None` for a `\\` in a group, `\par` or a skip not a
/// length.
fn align_lines(text: &str) -> Option<Vec<(&str, &str)>> {
    let b = text.as_bytes();
    let (mut depth, mut start, mut skip) = (0i32, 0, "0pt");
    let mut lines = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'\\' if b.get(i + 1) == Some(&b'\\') => {
                if depth != 0 {
                    return None;
                }
                lines.push((skip, &text[start..i]));
                i += 2;
                skip = "0pt";
                if b.get(i) == Some(&b'[') {
                    let k = text[i..].find(']')? + i;
                    skip = &text[i + 1..k];
                    length(skip)?;
                    i = k + 1;
                }
                start = i;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    lines.push((skip, &text[start..]));
    (!text.contains("\\par")).then_some(lines)
}

/// An aligned node's text box (tikz.code.tex:4222-4266, `\tikz@halign@check`): each line an
/// `\hbox` (`\tikz@start@align`: `\ignorespaces`, the line, `\unskip` from `\\`); from the
/// second on, the lines so far become `\vbox{\halign{<header>\box<lines>\cr\noalign{\vskip
/// <skip>}\unhbox<line>\unskip\cr}}`, the header `\hfil#\hfil\cr` (center), `#\hfil\cr`
/// (left), `\hfil#\cr` (right). Two registers below the text boxes hold the lines.
fn align_source(n: &NodeCmd, out: &mut String) {
    let (a, l) = (NODE_BOX - 2, NODE_BOX - 3);
    let header = match n.align {
        Align::Center => "\\hfil#\\hfil\\cr",
        Align::Left => "#\\hfil\\cr",
        Align::Right => "\\hfil#\\cr",
    };
    out.push_str(&n.font);
    out.push('{');
    for (k, (skip, line)) in align_lines(&n.text).unwrap_or_default().into_iter().enumerate() {
        if k == 0 {
            out.push_str(&format!("\\setbox{a}\\hbox{{\\ignorespaces {line}\\unskip}}"));
        } else {
            out.push_str(&format!("\\setbox{l}\\hbox{{\\ignorespaces {line}\\unskip}}\\setbox{a}\\vbox{{\\halign{{{header}\\box{a}\\cr\\noalign{{\\vskip {skip}}}\\unhbox{l}\\unskip\\cr}}}}"));
        }
    }
    out.push_str(&format!("\\box{a}}}}}%\n"));
}

/// TeX's `.5\pgf@x`: truncated toward zero.
fn half(x: i32) -> i32 {
    xn_over_d(x, 32768, 65536)
}

/// pgfmath's `.5\pgflinewidth` (`\pgfmathmultiply@`: `\pgfmath@x=0.5pt`, then
/// `\pgfmath@x=<width as printed>\pgfmath@x`), read back as a length.
fn pgfmath_half(w: i32) -> i32 {
    super::fmul(w, 32768)
}

/// A node's geometry, known once its text box is: the translation of the text (`\pgf@pt@x`,
/// `\pgf@pt@y`) and the background rectangle's corner and size.
pub(super) struct Geo {
    shift: [i32; 2],
    corner: [i32; 2],
    size: [i32; 2],
    /// A circle's background: its centre on the picture and radius (outer sep less).
    circle: Option<([i32; 2], i32)>,
    rotate: bool,
    /// Placed on the page: the text's glue and raise are set where the page ships
    /// ([`what::PGF_GLUE`]), `shift` being the page point the text goes to.
    page: bool,
}

impl NodeCmd {
    /// The saved anchors and the anchor: the node's geometry for a text box of `wd`, `ht`,
    /// `dp`, in a font of quad `em`, with `\pgflinewidth` = `lw`. Adds the background path to
    /// the picture's box (`\pgf@protocolsizes` of both corners, widened by half the line width
    /// when drawn, `\pgfusepath`).
    /// The saved anchors and the outer sep.
    fn frame(&self, [wd, ht, dp]: [i32; 3], em: i32, lw: i32) -> (Outline, i32) {
        // `\pgfmathsetlength{.3333em}`: TeX's `0.3333em`.
        let is = self.inner.map(|v| v.unwrap_or_else(|| xn_over_d(em, round_decimals(b"3333"), 65536)));
        let os = pgfmath_half(lw);
        if self.circle {
            // The circle's `\radius` (pgfmoduleshapes.code.tex:1198): the half box and inner
            // seps as a vector, its length from the normalised vector's larger coordinate
            // (`/255`, then `16` times twice), at least half each minimum size, plus the
            // outer sep.
            let ya = half(ht) + half(dp) + is[1];
            let xa = half(wd) + is[0];
            let (px, py) = normalised(xa, ya);
            let r = if px > py {
                let c = px / 255;
                if c == 0 { xa } else { 16 * (16 * xa / c) }
            } else {
                let c = py / 255;
                if c == 0 { xa } else { 16 * (16 * ya / c) }
            };
            let r = r.max(half(self.min[0])).max(half(self.min[1])) + os;
            return (Outline::Circle([half(wd), half(ht) - half(dp)], r), os);
        }
        let x = (wd + 2 * is[0]).max(self.min[0]);
        let y = (ht + dp + 2 * is[1]).max(self.min[1]);
        let ne = [half(x) + half(wd) + os, half(y) - half(dp) + half(ht) + os];
        let sw = [-half(x) + half(wd) - os, -half(y) - half(dp) + half(ht) - os];
        (Outline::Rect(sw, ne), os)
    }

    /// The translation of a node at `(px, py)` (`\pgftransformshift` of the point, of the
    /// placement's shift, the rotation, then of the anchor's negative through the matrix).
    fn shift(&self, o: Outline, (px, py): (i32, i32)) -> [i32; 2] {
        let a = o.point(self.anchor);
        if self.rotate {
            [px + a[1], py - a[0]]
        } else {
            [px - a[0], py - a[1]]
        }
    }

    /// The named node's [`Shape`] at `at` (a picture point).
    pub(super) fn shape(&self, dims: [i32; 3], em: i32, at: (i32, i32)) -> Option<Shape> {
        let (outline, _) = self.frame(dims, em, self.line_width());
        Some(Shape { name: self.name.clone()?, shift: self.shift(outline, at), outline })
    }

    pub(super) fn geometry(&self, dims: [i32; 3], em: i32, lw: i32, (px, py): (i32, i32), bbox: &mut super::Bbox) -> Geo {
        let (outline, os) = self.frame(dims, em, lw);
        let shift = self.shift(outline, (px, py));
        let w = if self.draw { super::times_frac(b"5", lw) } else { 0 };
        let (sw, ne) = match outline {
            Outline::Rect(sw, ne) => (sw, ne),
            Outline::Circle(c, r) => {
                // `\pgfpathcircle{\centerpoint}{\radius - outer sep}`: its box.
                let (c, rp) = ([c[0] + shift[0], c[1] + shift[1]], r - os);
                bbox.add(c[0] - rp - w, c[1] - rp - w);
                bbox.add(c[0] + rp + w, c[1] + rp + w);
                return Geo { shift, corner: [0; 2], size: [0; 2], circle: Some((c, rp)), rotate: false, page: matches!(self.at, Pt::Page(_)) };
            }
        };
        let c1 = [sw[0] + os, sw[1] + os];
        let c2 = [ne[0] - os, ne[1] - os];
        let size = [c2[0] - c1[0], c2[1] - c1[1]];
        let corner = [c1[0] + shift[0], c1[1] + shift[1]];
        if self.rotate {
            // `\pgf@specialrect`: each corner through the matrix (`\pgfpathmoveto`,
            // `\pgfpathlineto` protocol them). Unpainted only (`parse`).
            for (x, y) in [(c2[0], c2[1]), (c1[0], c2[1]), (c1[0], c1[1]), (c2[0], c1[1])] {
                bbox.add(shift[0] - y, shift[1] + x);
            }
        } else {
            bbox.add(corner[0] - w, corner[1] - w);
            bbox.add(corner[0] + size[0] + w, corner[1] + size[1] + w);
        }
        Geo { shift, corner, size, circle: None, rotate: self.rotate, page: matches!(self.at, Pt::Page(_)) }
    }

    /// `\pgflinewidth` in the node: its last width option, else the picture's.
    pub(super) fn line_width(&self) -> i32 {
        self.opts.iter().rev().find_map(|o| if let PathOpt::Width(w) = o { Some(*w) } else { None }).unwrap_or(self.base_width)
    }

    /// The colour specs the node's options and text use, in written order (to resolve before
    /// drawing): the options', then the text's.
    pub(super) fn colors(&self) -> impl Iterator<Item = (&str, Option<bool>)> {
        self.opts.iter().filter_map(|o| match o {
            PathOpt::Color(s) => Some((s.as_str(), None)),
            PathOpt::Stroke(s) => Some((s.as_str(), Some(true))),
            PathOpt::Fill(s) => Some((s.as_str(), Some(false))),
            _ => None,
        })
    }

    /// A path node in its path's scope (probe): its line width the path's, its text in the
    /// path's colour unless it names its own; a painted node only under widths alone.
    pub(super) fn in_path_scope(&mut self, opts: &[PathOpt], base_width: i32) -> Option<()> {
        if (self.draw || self.fill) && opts.iter().any(|o| !matches!(o, PathOpt::Width(_))) {
            return None;
        }
        self.base_width = opts.iter().rev().find_map(|o| if let PathOpt::Width(w) = o { Some(*w) } else { None }).unwrap_or(base_width);
        if self.text_color.is_none() {
            self.text_color = opts.iter().rev().find_map(|o| if let PathOpt::Color(s) = o { Some(s.clone()) } else { None });
        }
        Some(())
    }

    pub(super) fn text_color(&self) -> Option<&str> {
        self.text_color.as_deref()
    }
}

impl Machine<'_, '_> {
    /// `\tikz@figbox` of node `n` with text box register `reg`, geometry `g`, its options'
    /// literals `lits` (as [`NodeCmd::colors`]) and its text colour's `\color` literal.
    pub(super) fn tikz_node(&mut self, n: &NodeCmd, reg: u32, g: &Geo, lits: &[String], text_lit: Option<&str>, base: &[String; 2]) {
        // A path's nodes share the outer box, which the path's loop closes.
        let fig = self.cur.len();
        let inner = self.cur.len();
        // `\pgfscope`, `\tikz@options`.
        self.pdf_literal("q ");
        let pushes = self.node_options(n, lits);
        let scoped = !n.opts.is_empty();
        let painted = n.draw || n.fill;
        // The rectangle's saved anchors' code typesets two spaces, the circle's none (probe);
        // the anchor's code runs twice.
        let mut pending = if n.circle { 2 * n.anchor.circle_space() } else { 2 + 2 * n.anchor.space() };
        if !scoped && !painted {
            pending += 1;
        }
        self.space_glue(&mut pending);
        // `\tikz@finish` of the background path.
        if scoped {
            self.pdf_literal("q ");
            self.node_options(n, lits);
        }
        if let (true, Some((c, rp))) = (painted, g.circle) {
            for op in super::ellipse(c[0], c[1], rp, rp) {
                if let super::Op::Lit(l) = op {
                    self.pdf_literal(&l);
                }
            }
        } else if painted && n.round > 0 {
            // `\pgfpathrectangle` under `\pgfsetcornersarced` (pgfcorepathprocessing): the
            // rectangle from its far corner leftward, each corner rounded, the subpath's
            // moveto (the near corner) after the close.
            let (sw, ne) = ((g.corner[0], g.corner[1]), (g.corner[0] + g.size[0], g.corner[1] + g.size[1]));
            for lit in super::rounded_rect([ne, (sw.0, ne.1), sw, (ne.0, sw.1)], n.round) {
                self.pdf_literal(&lit);
            }
        } else if painted {
            let lit = format!("{} {} {} {} re ", what::pgf_bp(g.corner[0]), what::pgf_bp(g.corner[1]), what::pgf_bp(g.size[0]), what::pgf_bp(g.size[1]));
            self.pdf_literal(&lit);
        }
        if painted {
            self.pdf_literal(match (n.fill, n.draw) {
                (true, false) => "f ",
                (false, _) => "S ",
                (true, true) => "B ",
            });
        }
        if scoped || painted {
            let mut one = 1;
            self.space_glue(&mut one);
        }
        if scoped {
            for _ in 0..pushes {
                self.color_stack(None);
            }
            self.pdf_literal("Q ");
        }
        // `\pgfsys@hboxsynced` of the recoloured text box.
        self.pdf_literal("q ");
        let synced = self.cur.len();
        let g0 = if g.page {
            self.page.pgf_glue = true;
            let mut n = self.e.nodes.glue_node(Glue::default(), what::PGF_GLUE);
            n.b = g.shift[0];
            n
        } else {
            self.e.nodes.glue_node(Glue { width: g.shift[0], ..Glue::default() }, 0)
        };
        self.cur.push(g0);
        let raised = self.cur.len();
        // `\pgflowlevelsynccm`: the matrix's entries as pgf keeps them.
        self.pdf_literal(if g.rotate { "0.0 1.0 -1.0 0.0 0.0 0.0 cm " } else { "1.0 0.0 0.0 1.0 0.0 0.0 cm " });
        self.pdf_literal("q ");
        let zero = self.cur.len();
        let recolor = self.cur.len();
        // `\pgfsetcolor{.}`: the text colour's stroke and fill halves, else black's.
        match text_lit.and_then(|l| Some((super::pgf_color(l, true)?, super::pgf_color(l, false)?))) {
            Some((s, f)) => {
                self.pdf_literal(&s);
                self.pdf_literal(&f);
            }
            // The colour the picture is in (black's `0 G`, `0 g` outside any).
            None => {
                self.pdf_literal(&base[0]);
                self.pdf_literal(&base[1]);
            }
        }
        if let Some(b) = self.e.eqtb.box_reg(reg) {
            self.e.eqtb.replace_box(reg, None);
            self.cur.push(Node::new(Kind::Box, b as i32));
        }
        let mut one = 1;
        self.space_glue(&mut one);
        self.natural_box(recolor, None);
        self.natural_box(zero, Some(0));
        self.pdf_literal("Q ");
        let mut one = 1;
        self.space_glue(&mut one);
        let mut b = self.hpack(raised, 0, false);
        b.shift = if g.page { g.shift[1] } else { -g.shift[1] };
        let b = self.e.nodes.add_box(b);
        self.cur.push(Node::new(Kind::Box, b as i32));
        let fil = Glue { stretch: 65536, stretch_order: 1, shrink: 65536, shrink_order: 1, ..Glue::default() };
        let hss = self.e.nodes.glue_node(fil, 0);
        self.cur.push(hss);
        self.natural_box(synced, Some(0));
        self.pdf_literal("Q ");
        // `\pgfsys@hboxsynced`'s and the node's `\endpgfidscope`, the options' pops,
        // `\endpgfscope`.
        let mut two = 2;
        self.space_glue(&mut two);
        for _ in 0..pushes {
            self.color_stack(None);
        }
        self.pdf_literal("Q ");
        self.natural_box(inner, None);
        if n.on_path.is_none() && !n.tree {
            self.natural_box(fig, None);
        }
    }

    /// The node's `\tikz@options` literals; returns how many colour-stack pushes they made.
    fn node_options(&mut self, n: &NodeCmd, lits: &[String]) -> usize {
        let (mut k, mut pushes) = (0, 0);
        for o in &n.opts {
            match o {
                PathOpt::Width(w) => self.pdf_literal(&format!("{} w ", what::pgf_bp(*w))),
                PathOpt::Color(_) => {
                    self.color_stack(Some(&lits[k]));
                    k += 1;
                    pushes += 1;
                }
                PathOpt::Stroke(_) | PathOpt::Fill(_) => {
                    self.pdf_literal(&lits[k]);
                    k += 1;
                }
                PathOpt::Dash(_) | PathOpt::Opacity(..) => {}
            }
        }
        pushes
    }

    /// The list from `start` as an hbox of its natural size, then (`Some(0)`) its width,
    /// height and depth set to zero, as `\wd\pgf@hbox=0pt` and kin do.
    pub(super) fn natural_box(&mut self, start: usize, zero: Option<i32>) {
        let mut b = self.hpack(start, 0, false);
        if let Some(z) = zero {
            (b.width, b.height, b.depth) = (z, z, z);
        }
        let b = self.e.nodes.add_box(b);
        self.cur.push(Node::new(Kind::Box, b as i32));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Ctx<'static> {
        Ctx { dims: &|_| None, names: Vec::new(), unit: [super::super::UNIT; 2], tf: super::super::Tf::ID, boxes: &|_| [0; 3], em: 0, nodes: Vec::new(), node_count: 0, distance: [0; 2], width: 26214, arrows: [false; 2], gt: super::super::Tip::To, styles: Vec::new(), spaces: Default::default(), shape_ref: Default::default(), named: Default::default(), tree: [0, 0, -90], shorten: [0; 2], auto: false }
    }

    #[test]
    fn reads_nodes() {
        let mut c = Cursor { s: "[draw=approval,line width=1pt,minimum width=2.2cm,minimum height=2.2cm]\n  {\\sffamily\\bfseries SEAL};" };
        let n = parse(&mut c, &mut ctx(), false).expect("a seal");
        assert!(n.draw && n.min == [length("2.2cm").unwrap(); 2] && n.text == "\\sffamily\\bfseries SEAL");
        let mut c = Cursor { s: "[anchor=west,text=white,font=\\sffamily\\bfseries\\Large]\n at (.55,4.18) {BUSCARD};" };
        let n = parse(&mut c, &mut ctx(), false).expect("a card line");
        assert!(n.anchor == NodeAnchor::West && n.font == "\\sffamily\\bfseries\\Large");
        // Named and placed nodes: a shape kept, `of` a known name only.
        let mut cx = ctx();
        assert!(parse(&mut Cursor { s: "(a) {x};" }, &mut cx, false).is_some() && cx.nodes.len() == 1);
        assert!(parse(&mut Cursor { s: "[right=of a] (b) {y};" }, &mut cx, false).is_some_and(|n| n.anchor == NodeAnchor::West && n.spaces == 1));
        assert!(parse(&mut Cursor { s: "[below=of c] {z};" }, &mut cx, false).is_none());
        assert!(parse(&mut Cursor { s: "[above left=2mm] {z};" }, &mut cx, false).is_none());
        // A formula of the kernel's symbols is of the grammar, options in several brackets
        // too; not: a formula with an unknown command, an unknown command, a painted node
        // on the page.
        assert!(parse(&mut Cursor { s: "{$(6,18)$};" }, &mut ctx(), false).is_some());
        assert!(parse(&mut Cursor { s: "[draw][anchor=west] {$\\alpha_{1} \\le \\frac{1}{2}$};" }, &mut ctx(), false).is_some_and(|n| n.draw && n.anchor == NodeAnchor::West));
        assert!(parse(&mut Cursor { s: "{$\\foo$};" }, &mut ctx(), false).is_none());
        assert!(parse(&mut Cursor { s: "{$x\\label{e}$};" }, &mut ctx(), false).is_none());
        assert!(parse(&mut Cursor { s: "{\\foo};" }, &mut ctx(), false).is_none());
        assert!(parse(&mut Cursor { s: "[draw] {x};" }, &mut ctx(), true).is_none());
        assert_eq!(pgfmath_half(26214), 13107);
        assert_eq!(pgfmath_half(65536), 32768);
    }
}
