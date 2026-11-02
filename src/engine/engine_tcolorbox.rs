//! tcolorbox 6.10.0 on the GDL path (`2026-11-01-tcolorbox-design.md`): the standard skin's
//! unbreakable box, `\begin{tcolorbox}[options]`, `\newtcolorbox{name}[n][default]{options}`
//! and `\tcbset{options}` in the preamble, with the options [`apply`] reads; anything else
//! (breakable, other skins, overlays, libraries, a lower part) goes to the engine.
//!
//! The box is tcolorbox's own steps (tcolorbox.sty):
//! - `\tcb@set@@title`: the title in `\begin{tcb@savebox}{\tcb@titlebox}{\tcb@w@title}
//!   {tcbcoltitle}`, `\tcb@lrbox`'s `\setbox\hbox{\minipage[b]{w}\color{c}\ignorespaces`,
//!   then `\kvtcb@fonttitle` and the title; then the upper part the same way
//!   (`\tcb@saveupperbox`), closed by `\end{tcolorbox}` (`\tcb@endboxanddraw`).
//! - `\tcb@drawcolorbox` (:2182): `\tcb@before@unbroken` (the default `beforeafter skip
//!   balanced=0.5\baselineskip plus 2pt`: unless `\lastnodetype` is -1, `\par`, in vertical
//!   mode `\addvspace` of the skip less `\parskip` (plus `.3\baselineskip-\prevdepth` when
//!   the depth is between 0 and that), `\nointerlineskip`; then `\lineskip\z@skip
//!   \noindent`), the picture, `\tcb@after@unbroken` (`\par`, `\prevdepth=.3\baselineskip`,
//!   `\addvspace` of the skip less `\parskip`).
//! - The picture (`tcb@drawing`, a `tikzpicture`): the frame's and the interior's filled
//!   paths (`\tcb@boundary@base` with pgf's `\pgfsetcornersarced` before each corner,
//!   `\pgfprocessround`), then each text box by `\pgftext[x=..,y=..,left,top]` in a
//!   `pgfscope` of `\pgfsetfillopacity`, recoloured by `\color@begingroup\color{..}\box..`.

use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::{Glue, Reg, Region};
use super::super::engine_node::{Kind, Node, IGNORE_DEPTH};
use super::super::engine_run::{Machine, Mode, SEMI_SIMPLE_GROUP};
use super::super::engine_token::Token;
use super::super::engine_whatsit as what;
use crate::backend::backend_scaled::{round_decimals, xn_over_d};

/// `\tcb@upperbox` and `\tcb@titlebox`, among the native job's hidden registers.
const UPPER_BOX: u32 = 32720;
const TITLE_BOX: u32 = 32721;

/// tcolorbox's state in a native document: `\tcbset`'s options, the `\newtcolorbox`
/// environments, and the box being built.
#[derive(Default, Clone, PartialEq)]
pub(super) struct TcbState {
    defaults: Vec<Vec<Token>>,
    envs: Vec<TcbEnv>,
    cur: Option<Box<Cfg>>,
}

/// `\newtcolorbox{name}[args][default]{options}`.
#[derive(Clone, PartialEq)]
struct TcbEnv {
    name: String,
    args: u8,
    default: Option<Vec<Token>>,
    opts: Vec<Token>,
}

/// A box's options after `\tcb@apply@box@options`: colours as xcolor specifications,
/// lengths in sp, the title and fonts as tokens.
#[derive(Clone, PartialEq)]
struct Cfg {
    colback: String,
    colframe: String,
    coltitle: String,
    colupper: String,
    /// `\kvtcb@top@rule@stand`, `\kvtcb@bottom@rule@stand`, `\kvtcb@left@rule`,
    /// `\kvtcb@right@rule`, `\kvtcb@title@rule`.
    rules: [i32; 5],
    arc: i32,
    boxsep: i32,
    /// `lefttitle`, `leftupper`, `righttitle`, `rightupper`.
    left: [i32; 2],
    right: [i32; 2],
    top: i32,
    bottom: i32,
    toptitle: i32,
    bottomtitle: i32,
    /// `sharp corners` (all four), else `rounded corners`.
    sharp: bool,
    title: Option<Vec<Token>>,
    fonttitle: Vec<Token>,
    fontupper: Vec<Token>,
    /// `\kvtcb@width` (`\linewidth`) when the box began.
    width: i32,
    /// `\iftcb@nobreak`: `\if@nobreak` when the options were applied (after a heading); the
    /// box then starts with `before nobreak=\noindent` alone (tcolorbox.sty:2170, :3047).
    nobreak: bool,
}

/// tcolorbox.sty:3029, the reset style: `size=normal` (:956, `boxrule=0.5mm, boxsep=1mm,
/// left=4mm, right=4mm, top=2mm, bottom=2mm, toptitle=0pt, bottomtitle=0pt, arc=1mm, auto
/// outer arc`), `colback=black!5!white, colframe=black!75!white, coltext=black,
/// coltitle=white`, `rounded corners`.
fn reset(width: i32) -> Cfg {
    let (rule, mm, side, two) = (dim("0.5mm"), dim("1mm"), dim("4mm"), dim("2mm"));
    Cfg {
        colback: "black!5!white".into(),
        colframe: "black!75!white".into(),
        coltitle: "white".into(),
        colupper: "black".into(),
        rules: [rule; 5],
        arc: mm,
        boxsep: mm,
        left: [side; 2],
        right: [side; 2],
        top: two,
        bottom: two,
        toptitle: 0,
        bottomtitle: 0,
        sharp: false,
        title: None,
        fonttitle: Vec::new(),
        fontupper: Vec::new(),
        width,
        nobreak: false,
    }
}

/// A length as TeX's `scan_dimen` reads it (§458: `mm` is `7227/2540` with the remainder
/// carried into the fraction).
fn dim(s: &str) -> i32 {
    crate::backend::backend_scaled::scan_dimen(s, 0, 0).unwrap_or(0) as i32
}

/// The options a native box takes (tcolorbox.sty's keys of the same names); the pre-scan's
/// list ([`keys_ok`]).
const KEYS: &[&str] = &["colback", "colframe", "coltitle", "coltext", "colupper", "title", "fonttitle", "fontupper", "boxrule", "arc", "boxsep", "left", "right", "top", "bottom", "lefttitle", "leftupper", "righttitle", "rightupper", "toptitle", "bottomtitle", "toprule", "bottomrule", "leftrule", "rightrule", "titlerule", "sharp corners", "rounded corners"];

/// Font commands a `fonttitle=` or `fontupper=` value may hold.
const FONTS: &[&str] = &["bfseries", "itshape", "slshape", "scshape", "upshape", "mdseries", "rmfamily", "sffamily", "ttfamily", "normalfont", "tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE", "huge", "Huge"];

/// The source's `key=value` items at the top level, values without one pair of braces.
fn items(s: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    let b = s.as_bytes();
    for (k, &c) in b.iter().enumerate().chain(std::iter::once((b.len(), &b','))) {
        match c {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b',' if depth == 0 => {
                let item = s[start..k].trim();
                start = k + 1;
                if item.is_empty() {
                    continue;
                }
                let (key, val) = item.split_once('=').map_or((item, ""), |(a, v)| (a.trim(), v.trim()));
                let val = val.strip_prefix('{').and_then(|v| v.strip_suffix('}')).unwrap_or(val);
                out.push((key, val));
            }
            _ => {}
        }
    }
    out
}

/// `\usepackage[most]{tcolorbox}` (tcolorbox.sty:3124, `many,listingsutf8,external,magazine,
/// vignette,poster`): of the packages its libraries load only amsmath (theorems) sets type;
/// listings, pdfcol, xparse, tikzfill.image and the tikz libraries define macros, keys and
/// colour stacks a standard-skin box leaves unused (probed: pdfTeX's page boxes and page
/// streams identical with and without it; vignette's fadings add only unreferenced forms).
/// The libraries' keys stay outside [`KEYS`], so a box using one declines.
pub(super) fn most(opts: &str) -> bool {
    opts.split(',').map(str::trim).filter(|o| !o.is_empty()).eq(["most"])
}

/// The pre-scan's check of an option list (`\begin{tcolorbox}[..]`, `\newtcolorbox`,
/// `\tcbset`): every key of [`KEYS`], a length of digits and a unit, a colour of letters,
/// digits, `!` and `.`, fonts of [`FONTS`]; a `#n` item or value (a `\newtcolorbox`
/// argument) only where `args` allows. The title's text is scanned on as text.
pub(super) fn keys_ok(s: &str, args: bool) -> bool {
    items(s).into_iter().all(|(key, val)| {
        if args && key.len() == 2 && key.starts_with('#') && val.is_empty() {
            return true;
        }
        if !KEYS.contains(&key) {
            return false;
        }
        match key {
            "title" => true,
            "sharp corners" | "rounded corners" => val.is_empty(),
            "fonttitle" | "fontupper" => val.split('\\').skip(1).all(|c| FONTS.contains(&c.trim())) && (val.is_empty() || val.starts_with('\\')),
            k if k.starts_with("col") => !val.is_empty() && val.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'!' || c == b'.'),
            _ => crate::backend::backend_scaled::scan_dimen(val, 0, 0).is_some() && val.trim_end().ends_with(['t', 'm', 'n', 'p']) && !val.contains("em") && !val.contains("ex"),
        }
    })
}

/// `\newtcolorbox{name}[n][default]{options}` in the source after the command: the name,
/// whether it has an optional argument, the options and the bytes read.
pub(super) fn newtcolorbox_args(after: &str) -> Option<(&str, bool, &str, usize)> {
    let t = after.trim_start();
    let skip = after.len() - t.len();
    let r = t.strip_prefix('{')?;
    let (name, mut r) = r.split_once('}')?;
    if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let mut opt = false;
    if let Some(x) = r.trim_start().strip_prefix('[') {
        let (n, x) = x.split_once(']')?;
        if !matches!(n.trim(), "0" | "1" | "2") {
            return None;
        }
        r = x;
        if let Some(x) = r.trim_start().strip_prefix('[') {
            let (d, x) = x.split_once(']')?;
            if d.contains(['[', '{', '\\']) {
                return None;
            }
            opt = true;
            r = x;
        }
    }
    let body = r.trim_start().strip_prefix('{')?;
    let k = super::balanced(body)?;
    let used = skip + (after.len() - skip) - (body.len() - k - 1);
    Some((name, opt, &body[..k], used))
}

/// The tokens' items split at top-level commas, each `(key, value)` with the value's spaces
/// at both ends and one pair of braces around it removed (pgfkeys).
fn token_items(m: &Machine, t: &[Token]) -> Vec<(String, Vec<Token>)> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut cur: Vec<Token> = Vec::new();
    let comma = Token::char(Cat::Other, ',');
    let flush = |cur: &mut Vec<Token>, out: &mut Vec<(String, Vec<Token>)>| {
        let item = std::mem::take(cur);
        let eq = item.iter().position(|&t| t == Token::char(Cat::Other, '='));
        let (k, v) = match eq {
            Some(e) => (&item[..e], &item[e + 1..]),
            None => (&item[..], &item[..0]),
        };
        let mut key = String::new();
        m.show(k, &mut key);
        let key = key.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut v: &[Token] = v;
        while v.first().is_some_and(|t| t.cat() == Some(Cat::Space)) {
            v = &v[1..];
        }
        while v.last().is_some_and(|t| t.cat() == Some(Cat::Space)) {
            v = &v[..v.len() - 1];
        }
        if v.len() >= 2 && v[0].cat() == Some(Cat::BeginGroup) && v[v.len() - 1].cat() == Some(Cat::EndGroup) && balanced_inside(&v[1..v.len() - 1]) {
            v = &v[1..v.len() - 1];
        }
        if !key.is_empty() || !v.is_empty() {
            out.push((key, v.to_vec()));
        }
    };
    for &x in t {
        match x.cat() {
            Some(Cat::BeginGroup) => depth += 1,
            Some(Cat::EndGroup) => depth -= 1,
            _ => {}
        }
        if depth == 0 && x == comma {
            flush(&mut cur, &mut out);
        } else {
            cur.push(x);
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// Whether braces stay balanced through `t` (so `{a}{b}` is not taken as one group).
fn balanced_inside(t: &[Token]) -> bool {
    let mut d = 0i32;
    for x in t {
        match x.cat() {
            Some(Cat::BeginGroup) => d += 1,
            Some(Cat::EndGroup) => {
                d -= 1;
                if d < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    d == 0
}

/// pgfcorepathprocessing.code.tex `\pgfprocessround` of `\tcb@boundary@base`'s path:
/// `moveto(xa,ya)`, then `lineto` to the NW, NE and SE corners and `closepath`, each corner
/// rounded by the radius `\pgfsetcornersarced` set before it (`r`: SW, NW, NE, SE; zero
/// skips the corner). A rounded corner `b` between `a` and `c` is a `lineto` to the point
/// `r` from `b` toward `a` (the corner before, not its cut point) and a curve to the point
/// `r` toward `c`, its supports `0.5522847` of the way to `b`. The SW corner is the close's:
/// its curve ends where the path then starts (`\pgfprocess@specialroundclose`).
fn boundary(xa: i32, ya: i32, xb: i32, yb: i32, r: [i32; 4]) -> Vec<String> {
    let lit = |op: char, x: i32, y: i32| format!("{} {} {op} ", what::pgf_bp(x), what::pgf_bp(y));
    let toward = |b: (i32, i32), to: (i32, i32), r: i32| (b.0 + r * (to.0 - b.0).signum(), b.1 + r * (to.1 - b.1).signum());
    let support = |p: (i32, i32), b: (i32, i32)| (p.0 + xn_over_d(b.0 - p.0, round_decimals(b"5522847"), 65536), p.1 + xn_over_d(b.1 - p.1, round_decimals(b"5522847"), 65536));
    let curve = |s: (i32, i32), e: (i32, i32), b: (i32, i32)| {
        let (k1, k2) = (support(s, b), support(e, b));
        let mut t = String::new();
        for (x, y) in [k1, k2, e] {
            t.push_str(&what::pgf_bp(x));
            t.push(' ');
            t.push_str(&what::pgf_bp(y));
            t.push(' ');
        }
        t.push_str("c ");
        t
    };
    let (sw, nw, ne, se) = ((xa, ya), (xa, yb), (xb, yb), (xb, ya));
    let mut out = Vec::new();
    let start = if r[0] > 0 { toward(sw, nw, r[0]) } else { sw };
    out.push(lit('m', start.0, start.1));
    for (k, (a, b, c)) in [(sw, nw, ne), (nw, ne, se), (ne, se, sw)].into_iter().enumerate() {
        let rk = r[k + 1];
        if rk > 0 {
            let (s, e) = (toward(b, a, rk), toward(b, c, rk));
            out.push(lit('l', s.0, s.1));
            out.push(curve(s, e, b));
        } else {
            out.push(lit('l', b.0, b.1));
        }
    }
    if r[0] > 0 {
        let s = toward(sw, se, r[0]);
        out.push(lit('l', s.0, s.1));
        out.push(curve(s, start, sw));
    }
    out.push("h ".into());
    out
}

/// pgfsys-common-pdf.def's fill colour from LaTeX's literal (`0.95 g 0.95 G`): the fill
/// half (`\pgfsetfillcolor`).
fn fill_half(lit: &str) -> Option<String> {
    let words: Vec<&str> = lit.split_whitespace().collect();
    let k = words.iter().position(|w| matches!(*w, "rg" | "g" | "k"))?;
    Some(format!("{} {} ", words[..k].join(" "), words[k]))
}

impl Machine<'_, '_> {
    fn tcb(&mut self) -> Option<&mut TcbState> {
        self.doc.as_mut().map(|d| &mut d.pkg.tcb)
    }

    /// The `\newtcolorbox` environment `name`, if any.
    pub(super) fn tcb_env(&self, name: &str) -> Option<usize> {
        self.doc.as_ref()?.pkg.tcb.envs.iter().position(|e| e.name == name)
    }

    /// Whether a native box is being built (its upper part is open).
    pub(super) fn tcb_open(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.pkg.tcb.cur.is_some())
    }

    /// `\begin{tcolorbox}[options]` inside `\begin`'s group.
    pub(super) fn tcolorbox_begin(&mut self) {
        let opts = if self.next_is('[') { self.bracket_arg() } else { Vec::new() };
        self.tcb_start(&[opts]);
    }

    /// `\begin{name}` of a `\newtcolorbox` environment inside `\begin`'s group: its
    /// arguments (an optional first one with its default) put in for `#1`, `#2`.
    pub(super) fn tcb_env_begin(&mut self, k: usize) {
        let Some(env) = self.doc.as_ref().map(|d| d.pkg.tcb.envs[k].clone()) else { return };
        let mut args: Vec<Vec<Token>> = Vec::new();
        for i in 0..env.args {
            if i == 0 && env.default.is_some() {
                args.push(if self.next_is('[') { self.bracket_arg() } else { env.default.clone().unwrap_or_default() });
            } else {
                let mut t = Vec::new();
                self.read_arg(&mut t);
                args.push(t);
            }
        }
        let mut opts = Vec::with_capacity(env.opts.len());
        let mut i = 0;
        while i < env.opts.len() {
            let t = env.opts[i];
            if t.cat() == Some(Cat::Param) {
                match env.opts.get(i + 1).filter(|d| d.cs_id().is_none()).and_then(|d| d.chr().to_digit(10)) {
                    Some(n) if n >= 1 && (n as usize) <= args.len() => {
                        opts.extend_from_slice(&args[n as usize - 1]);
                        i += 2;
                        continue;
                    }
                    _ => return self.not_native("a \\newtcolorbox parameter"),
                }
            }
            opts.push(t);
            i += 1;
        }
        self.tcb_start(&[opts]);
    }

    /// `\newtcolorbox{name}[n][default]{options}` (`\DeclareTColorBox` without init
    /// options), in the preamble.
    pub(super) fn new_tcolorbox(&mut self) {
        if self.doc.as_ref().is_none_or(|d| d.body) || self.next_is('[') {
            return self.not_native("\\newtcolorbox with init options or in the body");
        }
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let Some(name) = Self::plain_text(&name) else { return self.not_native("\\newtcolorbox's name") };
        let mut args = 0u8;
        let mut default = None;
        if self.next_is('[') {
            let n = self.bracket_arg();
            args = match Self::plain_text(&n).as_deref().map(str::trim) {
                Some("0") => 0,
                Some("1") => 1,
                Some("2") => 2,
                _ => return self.not_native("\\newtcolorbox's argument count"),
            };
            if self.next_is('[') {
                default = Some(self.bracket_arg());
            }
        }
        let mut opts = Vec::new();
        self.read_arg(&mut opts);
        let Some(s) = self.tcb() else { return };
        s.envs.retain(|e| e.name != name);
        s.envs.push(TcbEnv { name, args, default, opts });
    }

    /// `\tcbset{options}` in the preamble: the options every later box starts from.
    pub(super) fn tcb_set(&mut self) {
        if self.doc.as_ref().is_none_or(|d| d.body) {
            return self.not_native("\\tcbset in the body");
        }
        let mut t = Vec::new();
        self.read_arg(&mut t);
        if let Some(s) = self.tcb() {
            s.defaults.push(t);
        }
    }

    /// The box's options read, its title box typeset, its upper box opened.
    fn tcb_start(&mut self, opts: &[Vec<Token>]) {
        if self.mode == Mode::Math || self.tcb_open() {
            return self.not_native("a tcolorbox in a formula or in a tcolorbox");
        }
        // tikz's `every picture` would be in the box's pictures too.
        if self.doc.as_ref().is_some_and(|d| d.pkg.tikz_styles.iter().any(|s| s.0 == "every picture")) {
            return self.not_native("a tcolorbox after an every picture style");
        }
        if self.doc.as_ref().is_some_and(|d| d.floats.minipage || !d.lists.is_empty() || !d.pbox.is_empty()) {
            return self.not_native("a tcolorbox in a minipage, box or list");
        }
        if self.current_color() != self.default_color() {
            return self.not_native("a tcolorbox in a colour");
        }
        let Some(width) = self.length_of("\\linewidth") else { return };
        let mut c = reset(width);
        c.nobreak = self.doc.as_ref().is_some_and(|d| d.nobreak);
        let defaults = self.doc.as_ref().map(|d| d.pkg.tcb.defaults.clone()).unwrap_or_default();
        for list in defaults.iter().chain(opts) {
            if self.apply(&mut c, list).is_none() {
                return self.not_native("a tcolorbox option the native job does not draw");
            }
        }
        // The colours must be ones the native xcolor makes.
        for spec in [&c.colback, &c.colframe, &c.coltitle, &c.colupper] {
            if self.color_literal(None, spec, false).is_none() {
                return self.not_native("a tcolorbox colour the native job does not know");
            }
        }
        let [_, _, lr, rr, _] = c.rules;
        // `\tcb@set@@dimensions`: `\tcb@innerwidth`, `\tcb@w@upper`; `\tcb@set@@title`:
        // `\tcb@w@title`.
        let w_upper = c.width - lr - rr - c.boxsep * 2 - c.left[1] - c.right[1];
        let w_title = c.width - (lr + rr + c.boxsep * 2 + c.left[0] + c.right[0]);
        let mut t = Vec::new();
        if let Some(title) = c.title.clone() {
            self.tcb_savebox(TITLE_BOX, w_title, &c.coltitle.clone(), &c.fonttitle.clone(), &mut t);
            t.extend(title);
            t.extend(self.tex_tokens("\\end{minipage}}"));
        }
        self.tcb_savebox(UPPER_BOX, w_upper, &c.colupper.clone(), &c.fontupper.clone(), &mut t);
        if let Some(s) = self.tcb() {
            s.cur = Some(Box::new(c));
        }
        self.back_list(&t);
    }

    /// `\tcb@lrbox{box}{w}{colour}` and what follows it: `\setbox box\hbox{\begin{minipage}
    /// [b]{w}\color{colour}\ignorespaces <font>\ignorespaces`.
    fn tcb_savebox(&mut self, reg: u32, w: i32, color: &str, font: &[Token], t: &mut Vec<Token>) {
        t.push(self.named_cs(named!("setbox")));
        t.extend(reg.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        t.push(self.named_cs(named!("hbox")));
        t.push(Token::char(Cat::BeginGroup, '{'));
        t.extend(self.tex_tokens("\\begin{minipage}[b]{"));
        Self::sp_tokens(w, t);
        t.push(Token::char(Cat::EndGroup, '}'));
        t.extend(self.tex_tokens(&format!("\\color{{{color}}}\\ignorespaces ")));
        t.extend_from_slice(font);
        t.extend(self.tex_tokens("\\ignorespaces "));
    }

    /// One option list into the box's options; `None` for a key or value not taken.
    fn apply(&mut self, c: &mut Cfg, list: &[Token]) -> Option<()> {
        for (key, val) in token_items(self, list) {
            let mut text = String::new();
            self.show(&val, &mut text);
            let text = text.trim().to_string();
            let col = || (!text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'!' || b == b'.')).then(|| text.clone());
            match key.as_str() {
                "colback" => c.colback = col()?,
                "colframe" => c.colframe = col()?,
                "coltitle" => c.coltitle = col()?,
                "coltext" | "colupper" => c.colupper = col()?,
                "title" => c.title = Some(val),
                "fonttitle" => c.fonttitle = val,
                "fontupper" => c.fontupper = val,
                "sharp corners" if val.is_empty() => c.sharp = true,
                "rounded corners" if val.is_empty() => c.sharp = false,
                _ => {
                    if text.contains("em") || text.contains("ex") || val.is_empty() {
                        return None;
                    }
                    let v = self.enit_value(&val, false)?.width;
                    match key.as_str() {
                        "boxrule" => c.rules = [v; 5],
                        "toprule" => c.rules[0] = v,
                        "bottomrule" => c.rules[1] = v,
                        "leftrule" => c.rules[2] = v,
                        "rightrule" => c.rules[3] = v,
                        "titlerule" => c.rules[4] = v,
                        "arc" => c.arc = v,
                        "boxsep" => c.boxsep = v,
                        "left" => c.left = [v; 2],
                        "right" => c.right = [v; 2],
                        "lefttitle" => c.left[0] = v,
                        "leftupper" => c.left[1] = v,
                        "righttitle" => c.right[0] = v,
                        "rightupper" => c.right[1] = v,
                        "top" => c.top = v,
                        "bottom" => c.bottom = v,
                        "toptitle" => c.toptitle = v,
                        "bottomtitle" => c.bottomtitle = v,
                        _ => return None,
                    }
                }
            }
        }
        Some(())
    }

    /// `\end{tcolorbox}` (`\tcb@endboxanddraw`): the upper box closed (`\end{minipage}` and
    /// the `\hbox`'s brace), then the drawing.
    pub(super) fn tcolorbox_end(&mut self) {
        let mut t = self.tex_tokens("\\end{minipage}}");
        t.push(self.doc_id(doc!("rt@tcbdraw")));
        self.back_list(&t);
    }

    /// `\tcb@before@unbroken`'s start: `\ifnum\lastnodetype=-1\relax\else\par`; after a
    /// heading `\kvtcb@beforebox@nobreak`, `\noindent`.
    fn tcb_draw(&mut self) {
        let Some(nobreak) = self.doc.as_ref().and_then(|d| d.pkg.tcb.cur.as_ref()).map(|c| c.nobreak) else {
            return self.not_native("\\end{tcolorbox} without its box");
        };
        if nobreak {
            let t = [self.named_cs(named!("noindent")), self.doc_id(doc!("rt@tcbpic"))];
            return self.back_list(&t);
        }
        if self.last_node_type() == -1 {
            return self.tcb_noindent();
        }
        let t = [self.named_cs(named!("par")), self.doc_id(doc!("rt@tcbbefore"))];
        self.back_list(&t);
    }

    /// After the `\par`: in vertical mode `\addvspace` of `0.5\baselineskip plus 2pt` less
    /// `\parskip` (with `.3\baselineskip-\prevdepth` added when `\prevdepth` is from 0 to
    /// `.3\baselineskip`), `\nointerlineskip`.
    fn tcb_before(&mut self) {
        if self.mode == Mode::Vertical {
            let bs = self.glue_param(super::BASELINE_SKIP).width;
            let three = xn_over_d(bs, round_decimals(b"3"), 65536);
            let pd = self.aux;
            let mut g = self.tcb_skip();
            if pd >= 0 && pd <= three {
                g.width += three - pd;
            }
            self.add_vspace(g);
            self.aux = IGNORE_DEPTH;
        }
        self.tcb_noindent();
    }

    /// `\glueexpr 0.5\baselineskip plus 2pt-\parskip`.
    fn tcb_skip(&self) -> Glue {
        let bs = self.glue_param(super::BASELINE_SKIP).width;
        let half = Glue { width: xn_over_d(bs, round_decimals(b"5"), 65536), stretch: 2 * 65536, ..Glue::default() };
        super::glue_add(half, super::neg(self.glue_param(super::super::engine_pack::PAR_SKIP)))
    }

    /// `\lineskip\z@skip \noindent`, then the picture.
    fn tcb_noindent(&mut self) {
        self.e.eqtb.set_glue(Reg { region: Region::Glue, slot: super::super::engine_pack::LINE_SKIP }, Glue::default(), false);
        let t = [self.named_cs(named!("noindent")), self.doc_id(doc!("rt@tcbpic"))];
        self.back_list(&t);
    }

    /// The picture in the paragraph, then `\tcb@after@unbroken`'s `\par`.
    fn tcb_picture(&mut self) {
        let Some(c) = self.doc.as_mut().and_then(|d| d.pkg.tcb.cur.take()) else { return };
        if self.mode != Mode::Horizontal {
            return self.not_native("a tcolorbox outside a paragraph");
        }
        // `\tcb@parfillskip@restore` changes a `\parfillskip` with a width.
        if self.glue_param(super::super::engine_linebreak::PAR_FILL_SKIP).width > 0 {
            return self.not_native("a tcolorbox with a wide \\parfillskip");
        }
        let dims = |m: &Self, reg: u32| m.e.eqtb.box_reg(reg).map(|b| { let r = &m.e.nodes.boxes[b as usize]; [r.width, r.height, r.depth] });
        let Some(upper) = dims(self, UPPER_BOX) else { return self.not_native("a tcolorbox without its upper box") };
        let title = if c.title.is_some() { dims(self, TITLE_BOX) } else { None };
        let [tr, br, lr, rr, tir] = c.rules;
        let bs = c.boxsep;
        let w = c.width;
        // `\tcb@draw@color@box`: `\tcb@h@padtitle`, `\tcb@h@upper`; `\tcb@drawcolorbox`:
        // `\tcb@natheight`, `\tcb@innerheight`.
        let h_upper = upper[1] + upper[2];
        let pad = title.map_or(0, |t| tir + bs * 2 + c.toptitle + c.bottomtitle + t[1] + t[2]);
        let h = pad + tr + br + bs * 2 + c.top + c.bottom + h_upper;
        let inner = h - tr - br - pad;
        // `\tcb@comp@arc@auto`: the smallest rule, `+\kvtcb@arc`.
        let outer = tr.min(br).min(lr).min(rr) + c.arc;
        let (ra, ro) = if c.sharp { (0, 0) } else { (c.arc, outer) };
        let lits: Vec<String> = [&c.colframe, &c.colback, &c.coltitle, &c.colupper].iter().filter_map(|s| self.color_literal(None, s, false)).collect();
        let [frame, back, coltitle, colupper] = lits.as_slice() else { return self.not_native("a tcolorbox colour the native job does not know") };
        let (Some(frame_fill), Some(back_fill)) = (fill_half(frame), fill_half(back)) else { return self.not_native("a tcolorbox colour the native job does not know") };
        let (coltitle, colupper) = (coltitle.clone(), colupper.clone());
        let outer_start = self.cur.len();
        let g = self.e.nodes.glue_node(Glue::default(), 0);
        self.cur.push(g);
        let pic = self.cur.len();
        for l in ["q ", "0 G ", "0 g ", "0.3985 w "] {
            self.pdf_literal(l);
        }
        let layer = self.cur.len();
        self.pdf_literal("q ");
        // `\tcb@drawframe@standard`, `\tcb@drawwithouttitle@standard` or
        // `\tcb@drawwithtitle@standard`.
        let interior = if title.is_some() { boundary(lr, br, w - rr, br + inner, [ra, 0, 0, ra]) } else { boundary(lr, br, w - rr, h - tr, [ra; 4]) };
        for (fill, path) in [(frame_fill, boundary(0, 0, w, h, [ro; 4])), (back_fill, interior)] {
            self.pdf_literal("q ");
            self.pdf_literal(&fill);
            self.pgf_opacity(false, "1.0");
            for l in &path {
                self.pdf_literal(l);
            }
            self.pdf_literal("f ");
            self.pdf_literal("Q ");
        }
        // `\tcb@tdraw@title@core`: `x=left rule+boxsep+lefttitle`, `y=height-top rule-boxsep
        // -toptitle`; `\tcb@tdraw@upper@core` with `\tcb@dbox@top`: `y=` the inner height
        // + bottom rule - boxsep - top - the upper height, plus that height.
        if let Some(t) = title {
            let y = h - tr - bs - c.toptitle;
            self.tcb_text(TITLE_BOX, lr + bs + c.left[0], y - t[1], &coltitle);
            self.pdf_literal("Q ");
        }
        let y = (inner + br - bs - c.top - h_upper) + h_upper;
        self.tcb_text(UPPER_BOX, lr + bs + c.left[1], y - upper[1], &colupper);
        // The scope's `Q`, then tikz's `\endscope`: its `\endpgfidscope`'s space and `Q`.
        self.pdf_literal("Q ");
        let g = self.e.nodes.glue_node(Glue::default(), 0);
        self.cur.push(g);
        self.pdf_literal("Q ");
        let fil = Glue { stretch: 65536, stretch_order: 1, shrink: 65536, shrink_order: 1, ..Glue::default() };
        let g = self.e.nodes.glue_node(fil, 0);
        self.cur.push(g);
        self.tcb_pack(layer, true, None);
        self.pdf_literal("n ");
        self.pdf_literal("Q ");
        let g = self.e.nodes.glue_node(fil, 0);
        self.cur.push(g);
        self.tcb_pack(pic, true, None);
        let mut b = self.hpack(outer_start, 0, false);
        (b.width, b.height, b.depth) = (w, h, 0);
        let b = self.e.nodes.add_box(b);
        self.cur.push(Node::new(Kind::Box, b as i32));
        self.aux = 1000;
        let t = [self.named_cs(named!("par")), self.doc_id(doc!("rt@tcbafter"))];
        self.back_list(&t);
    }

    /// A text box by `\pgftext[x=x,y=..,left,top]` in its `pgfscope` of
    /// `\pgfsetfillopacity`: `q`, the opacity, then `\pgfsys@hboxsynced` (`q`, `\hbox to0pt
    /// {\hskip x \raise(y-ht)\hbox{cm, q, \hbox to0pt{the box recoloured}, Q, space}\hss}`,
    /// `Q`), its `\endpgfidscope`'s space; the scope's `Q` is the caller's.
    fn tcb_text(&mut self, reg: u32, x: i32, raise: i32, color: &str) {
        self.pdf_literal("q ");
        self.pgf_opacity(false, "1.0");
        self.pdf_literal("q ");
        let synced = self.cur.len();
        let g = self.e.nodes.glue_node(Glue { width: x, ..Glue::default() }, 0);
        self.cur.push(g);
        let raised = self.cur.len();
        self.pdf_literal("1.0 0.0 0.0 1.0 0.0 0.0 cm ");
        self.pdf_literal("q ");
        let zero = self.cur.len();
        let recolor = self.cur.len();
        // `\color@begingroup\color{tcbcolupper}\box..\color@endgroup` (`\tcb@upperdraw@begin@hook`).
        self.color_stack(Some(color));
        if let Some(b) = self.e.eqtb.box_reg(reg) {
            self.e.eqtb.replace_box(reg, None);
            self.cur.push(Node::new(Kind::Box, b as i32));
        }
        self.color_stack(None);
        self.tcb_pack(recolor, false, None);
        self.tcb_pack(zero, false, Some(0));
        self.pdf_literal("Q ");
        let g = self.e.nodes.glue_node(Glue::default(), 0);
        self.cur.push(g);
        let mut b = self.hpack(raised, 0, false);
        b.shift = -raise;
        let b = self.e.nodes.add_box(b);
        self.cur.push(Node::new(Kind::Box, b as i32));
        let fil = Glue { stretch: 65536, stretch_order: 1, shrink: 65536, shrink_order: 1, ..Glue::default() };
        let g = self.e.nodes.glue_node(fil, 0);
        self.cur.push(g);
        self.tcb_pack(synced, false, Some(0));
        self.pdf_literal("Q ");
        let g = self.e.nodes.glue_node(Glue::default(), 0);
        self.cur.push(g);
    }

    /// The list from `start` as an hbox, `to0pt` (`exact`) or natural, its size then set to
    /// zero (`Some(0)`).
    fn tcb_pack(&mut self, start: usize, exact: bool, zero: Option<i32>) {
        let mut b = self.hpack(start, 0, exact);
        if let Some(z) = zero {
            (b.width, b.height, b.depth) = (z, z, z);
        }
        let b = self.e.nodes.add_box(b);
        self.cur.push(Node::new(Kind::Box, b as i32));
    }

    /// `\tcb@after@unbroken` after its `\par`: in vertical mode `\prevdepth=.3\baselineskip`
    /// and `\addvspace` of the skip less `\parskip`; then `\end`'s `\endgroup`.
    fn tcb_after(&mut self) {
        if self.mode == Mode::Vertical {
            let bs = self.glue_param(super::BASELINE_SKIP).width;
            self.aux = xn_over_d(bs, round_decimals(b"3"), 65536);
            let g = self.tcb_skip();
            self.add_vspace(g);
        }
        self.end_group(SEMI_SIMPLE_GROUP);
    }
}

/// tcolorbox's natives, in `Meaning::Doc` order after the kernel's (`engine_doc::DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@newtcolorbox", |m| m.new_tcolorbox()),
    ("rt@tcbset", |m| m.tcb_set()),
    ("rt@tcbdraw", |m| m.tcb_draw()),
    ("rt@tcbbefore", |m| m.tcb_before()),
    ("rt@tcbpic", |m| m.tcb_picture()),
    ("rt@tcbafter", |m| m.tcb_after()),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_options() {
        assert!(keys_ok("colback=gray!8,colframe=gray!55,title=Reviewer comment", false));
        assert!(keys_ok("colback=commentfill,boxrule=.6pt,arc=2pt,left=8pt,right=8pt,top=6pt,bottom=6pt", false));
        assert!(keys_ok("title={Comment #1},fonttitle=\\bfseries", false));
        assert!(keys_ok("colback=black!5,title={Note},#1", true));
        assert!(!keys_ok("colback=black!5,#1", false));
        assert!(!keys_ok("breakable", false));
        assert!(!keys_ok("boxrule=1em", false));
        assert_eq!(items("a=1, b={x,y} ,c"), vec![("a", "1"), ("b", "x,y"), ("c", "")]);
    }

    #[test]
    fn computes_millimetres() {
        let p = |sp: i32| { let mut s = String::new(); crate::engine::engine_value::print_scaled(sp, &mut s); s };
        assert_eq!(p(dim("0.5mm")), "1.42262");
        assert_eq!(p(dim("1mm")), "2.84526");
        assert_eq!(p(dim("4mm")), "11.38109");
    }

    #[test]
    fn reads_newtcolorbox() {
        let (name, opt, body, used) = newtcolorbox_args("{note}[1][]{colback=black!5,#1} x").unwrap();
        assert_eq!((name, opt, body), ("note", true, "colback=black!5,#1"));
        assert_eq!(&"{note}[1][]{colback=black!5,#1} x"[used..], " x");
    }
}
