//! geometry.sty's page layout for the native job: its keys (`\setkeys{Gm}`) and
//! `\Gm@@process`, which turns them into `\textwidth`, `\textheight`, `\oddsidemargin`,
//! `\evensidemargin` and `\topmargin`. Lengths are read by the caller (the engine's own
//! `\setlength`); everything here is TeX's integer arithmetic on scaled points, as the
//! package does it.

/// The slots of [`Geometry::len`]: geometry's `\Gm@lmargin`, `\Gm@rmargin`, `\Gm@tmargin`,
/// `\Gm@bmargin`, `\Gm@width`, `\Gm@height`, `\Gm@textwidth`, `\Gm@textheight`.
const L: usize = 0;
const R: usize = 1;
const T: usize = 2;
const B: usize = 3;
const W: usize = 4;
const H: usize = 5;
const TW: usize = 6;
const TH: usize = 7;

/// `1in` in scaled points.
const INCH: i32 = 4736286;

/// The package's state between `\usepackage{geometry}` and the `\geometry` calls after it.
#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub struct Geometry {
    /// The lengths given (or computed by the last process; `clean` drops those), by slot.
    pub len: [Option<i32>; 8],
    /// `\ifGm@hbody` and `\ifGm@vbody`: a width or height was given.
    pub body: [bool; 2],
    /// `\ifGm@includehead`, `\ifGm@includefoot`.
    pub include: [bool; 2],
    /// `\paperwidth` and `\paperheight` as `\Gm@dimlist` sets them.
    pub paper: [i32; 2],
    /// `\headheight`, `\headsep` and `\footskip` set by keys (`\Gm@dimlist`).
    pub head: [Option<i32>; 3],
    /// `\skip\footins` set by `footnotesep` (a length, no stretch or shrink).
    pub footins: Option<i32>,
    /// `\columnsep` set by `columnsep` (`\Gm@setlength\columnsep`), until the caller applies it.
    pub columnsep: Option<i32>,
    /// `\Gm@cnth` and `\Gm@cntv`: which of margin, body, margin were given, as bits 4, 2, 1.
    pub cnt: [u8; 2],
    /// `\Gm@hscale` and `\Gm@vscale` as the factor `\Gm@defbylen` scans, in units of 2^-16
    /// (the value read as `<v>pt`).
    pub scale: [Option<i32>; 2],
    /// `\ifGm@landscape` and `\ifGm@swap@papersize`: `\Gm@adjustpaper` swaps the paper
    /// (`paper` holds it as `\Gm@dimlist` sets it) at each process when the second is true.
    pub landscape: bool,
    pub swap: bool,
    /// The class swapped its paper (`landscape`): `paperwidth` and the like are refused,
    /// since the native job does not keep whether the class named a paper (with none,
    /// `\Gm@setdefaultpaper` takes the swapped one and clears the swap).
    pub class_landscape: bool,
}

/// What a key does.
enum Key {
    /// One length (`lmargin=…`); the body slots also set `\ifGm@hbody` or `\ifGm@vbody`.
    Len(usize),
    /// `\Gm@branch`: one value sets both slots, two values one each.
    Branch(usize, usize),
    /// `margin`: `\Gm@branch{#1}{lmargin}{tmargin}\Gm@branch{#1}{rmargin}{bmargin}`.
    Margin,
    Paper(usize),
    /// `paperwidth`, `paperheight`: `\Gm@setlength\paperwidth{#1}` (`\Gm@paper` becomes
    /// `custom`, which only `\Gm@setdefaultpaper` reads, and that ran after the class's
    /// options already).
    PaperLen(usize),
    /// `papersize`: `\Gm@branch{#1}{paperwidth}{paperheight}`.
    PaperSize,
    /// `hscale`, `vscale`: `\Gm@hbodytrue\edef\Gm@hscale{#1}`.
    Scale(usize),
    /// `scale`: `\Gm@branch{#1}{hscale}{vscale}`.
    ScaleBoth,
    /// `landscape` (true) and `portrait` (false, the reverse): a change of
    /// `\ifGm@landscape` reverses `\ifGm@swap@papersize`.
    Landscape(bool),
    /// `includehead`, `includefoot`, `includeheadfoot` as bits 1 and 2.
    Include(u8),
    /// `nohead`, `nofoot`, `noheadfoot`: `\Gm@setlength` of `\headheight` and `\headsep`
    /// (bit 1), `\footskip` (bit 2) to `\z@`.
    No(u8),
    /// `headheight`, `headsep`, `footskip`.
    Head(usize),
    /// `footnotesep`: `\Gm@setlength{\skip\footins}`.
    Footins,
    /// `columnsep`: `\Gm@setlength\columnsep{#1}`.
    ColumnSep,
    /// A key with no effect on the pages: `verbose` writes to the log only, and the
    /// margin-note lengths matter only to `\marginpar` and `\the\marginparwidth`, which the
    /// pre-scan refuses.
    None,
}

fn key(name: &str) -> Option<Key> {
    let papers = crate::data::data_class::PAPERS;
    Some(match name {
        "lmargin" | "left" | "inner" | "innermargin" => Key::Len(L),
        "rmargin" | "right" | "outer" | "outermargin" => Key::Len(R),
        "tmargin" | "top" => Key::Len(T),
        "bmargin" | "bottom" => Key::Len(B),
        "width" | "totalwidth" => Key::Len(W),
        "height" | "totalheight" => Key::Len(H),
        "textwidth" => Key::Len(TW),
        "textheight" => Key::Len(TH),
        "hmargin" => Key::Branch(L, R),
        "vmargin" => Key::Branch(T, B),
        "total" => Key::Branch(W, H),
        "text" | "body" => Key::Branch(TW, TH),
        "margin" => Key::Margin,
        "includehead" => Key::Include(1),
        "includefoot" => Key::Include(2),
        "includeheadfoot" => Key::Include(3),
        "nohead" => Key::No(1),
        "nofoot" => Key::No(2),
        "noheadfoot" => Key::No(3),
        "paperwidth" => Key::PaperLen(0),
        "paperheight" => Key::PaperLen(1),
        "papersize" => Key::PaperSize,
        "hscale" => Key::Scale(0),
        "vscale" => Key::Scale(1),
        "scale" => Key::ScaleBoth,
        "headheight" | "head" => Key::Head(0),
        "headsep" => Key::Head(1),
        "footskip" | "foot" => Key::Head(2),
        "footnotesep" => Key::Footins,
        "columnsep" => Key::ColumnSep,
        "landscape" => Key::Landscape(true),
        "portrait" => Key::Landscape(false),
        "verbose" | "pdftex" | "marginparwidth" | "marginpar" | "marginparsep" => Key::None,
        _ => Key::Paper(papers.iter().position(|p| *p == name)?),
    })
}

/// `key=value` items of a keyval list at brace depth 0, spaces around both trimmed, one
/// brace group around the value removed (keyval's `\KV@@sp@def` and `\KV@split`).
fn items(list: &str) -> impl Iterator<Item = (&str, Option<&str>)> {
    split_top(list).filter(|s| !s.is_empty()).map(|s| match s.split_once('=') {
        Some((k, v)) => (k.trim(), Some(unbrace(v.trim()))),
        None => (s, None),
    })
}

/// The comma-separated parts of `s` outside braces, trimmed.
fn split_top(s: &str) -> impl Iterator<Item = &str> {
    let mut depth = 0i32;
    let mut start = 0;
    let mut parts = Vec::new();
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(s[start..].trim());
    parts.into_iter()
}

fn unbrace(v: &str) -> &str {
    match v.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        Some(inner) if inner.chars().scan(0i32, |d, c| { *d += (c == '{') as i32 - (c == '}') as i32; Some(*d) }).all(|d| d >= 0) => inner.trim(),
        _ => v,
    }
}

/// `\Gm@doif`: an empty value or `true` is true, `false` false (case ignored).
fn boolean(v: Option<&str>) -> Option<bool> {
    match v.map(str::to_ascii_lowercase).as_deref() {
        None | Some("") | Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    }
}

impl Geometry {
    /// The state after `\Gm@initall` with the class's paper (`\Gm@setdefaultpaper`).
    pub fn new(paper: [i32; 2]) -> Geometry {
        Geometry { paper, ..Default::default() }
    }

    /// The state after `\ProcessOptionsKV[c]` and `\Gm@setdefaultpaper` for a class with
    /// `landscape` and the portrait `paper`: geometry's own `landscape` key sets
    /// `\ifGm@landscape` and the swap; a class paper option (`\Gm@setsize`) keeps the swap
    /// in either order. With no paper option `\Gm@setdefaultpaper` takes the class's
    /// swapped paper and clears the swap instead: the same page, until a `paperwidth`.
    pub fn landscape_class(paper: [i32; 2]) -> Geometry {
        Geometry { paper, landscape: true, swap: true, class_landscape: true, ..Default::default() }
    }

    /// `\paperwidth` and `\paperheight` after `\Gm@adjustpaper`.
    pub fn page(&self) -> [i32; 2] {
        let [w, h] = self.paper;
        if self.swap { [h, w] } else { [w, h] }
    }

    /// `\setkeys{Gm}{list}`, each length read by `len` (`None` for a value it cannot
    /// read). `Err` names what the native job does not do.
    pub fn keys(&mut self, list: &str, len: &mut dyn FnMut(&str) -> Option<i32>) -> Result<(), &'static str> {
        for (name, value) in items(list) {
            let Some(k) = key(name) else { return Err("a geometry option") };
            let mut set = |g: &mut Geometry, slot: usize, v: &str| -> Result<(), &'static str> {
                g.len[slot] = Some(len(v).ok_or("a geometry length")?);
                if slot >= W {
                    g.body[(slot - W) % 2] = true;
                }
                Ok(())
            };
            // `\Gm@hscale\Gm@layoutwidth` scans the text as a factor: plain decimal digits only.
            let factor = |v: &str, len: &mut dyn FnMut(&str) -> Option<i32>| -> Result<i32, &'static str> {
                if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit() || b == b'.') || v.bytes().filter(|&b| b == b'.').count() > 1 {
                    return Err("a geometry option");
                }
                len(&format!("{v}pt")).ok_or("a geometry length")
            };
            match k {
                Key::Len(s) => set(self, s, value.ok_or("a geometry option")?)?,
                Key::Branch(..) | Key::Margin | Key::PaperSize | Key::ScaleBoth if value.is_none() => return Err("a geometry option"),
                Key::PaperLen(..) | Key::PaperSize if self.class_landscape => return Err("a geometry paper size with a landscape class"),
                Key::PaperLen(i) => self.paper[i] = len(value.ok_or("a geometry option")?).ok_or("a geometry length")?,
                Key::Landscape(to) => {
                    let on = boolean(value).ok_or("a geometry option")? == to;
                    if on != self.landscape {
                        self.landscape = on;
                        self.swap = !self.swap;
                    }
                }
                Key::PaperSize => {
                    let v: Vec<&str> = split_top(value.unwrap_or("")).collect();
                    self.paper[0] = len(v[0]).ok_or("a geometry length")?;
                    self.paper[1] = len(v.get(1).copied().unwrap_or(v[0])).ok_or("a geometry length")?;
                }
                Key::Scale(i) => {
                    self.scale[i] = Some(factor(value.ok_or("a geometry option")?, len)?);
                    self.body[i] = true;
                }
                Key::ScaleBoth => {
                    let v: Vec<&str> = split_top(value.unwrap_or("")).collect();
                    for (i, s) in [v[0], v.get(1).copied().unwrap_or(v[0])].into_iter().enumerate() {
                        self.scale[i] = Some(factor(s, len)?);
                        self.body[i] = true;
                    }
                }
                Key::No(bits) => {
                    if boolean(value).ok_or("a geometry option")? {
                        if bits & 1 != 0 {
                            self.head[0] = Some(0);
                            self.head[1] = Some(0);
                        }
                        if bits & 2 != 0 {
                            self.head[2] = Some(0);
                        }
                    }
                }
                Key::Branch(a, b) => {
                    let v: Vec<&str> = split_top(value.unwrap_or("")).collect();
                    set(self, a, v[0])?;
                    set(self, b, v.get(1).copied().unwrap_or(v[0]))?;
                }
                Key::Margin => {
                    let v: Vec<&str> = split_top(value.unwrap_or("")).collect();
                    let (x, y) = (v[0], v.get(1).copied().unwrap_or(v[0]));
                    set(self, L, x)?;
                    set(self, T, y)?;
                    set(self, R, x)?;
                    set(self, B, y)?;
                }
                Key::Paper(i) => {
                    if boolean(value) != Some(true) {
                        return Err("a geometry option");
                    }
                    // `\Gm@setsize`: the paper, and `\ifGm@swap@papersize` from `\ifGm@landscape`.
                    self.paper = super::engine_doc::PAPER_SIZES[i];
                    self.swap = self.landscape;
                }
                Key::Include(bits) => {
                    let on = boolean(value).ok_or("a geometry option")?;
                    for i in 0..2 {
                        if bits >> i & 1 != 0 {
                            self.include[i] = on;
                        }
                    }
                }
                Key::Head(i) => self.head[i] = Some(len(value.ok_or("a geometry option")?).ok_or("a geometry length")?),
                Key::Footins => self.footins = Some(len(value.ok_or("a geometry option")?).ok_or("a geometry length")?),
                Key::ColumnSep => self.columnsep = Some(len(value.ok_or("a geometry option")?).ok_or("a geometry length")?),
                Key::None if value.is_some_and(|v| len(v).is_none()) => return Err("a geometry length"),
                Key::None => {}
            }
        }
        Ok(())
    }

    /// `\Gm@clean`, before a `\geometry`'s keys: the lengths the last process computed go.
    pub fn clean(&mut self) {
        let [h, v] = self.cnt;
        for (slot, given) in [(L, h >= 4), (R, h & 1 != 0), (T, v >= 4), (B, v & 1 != 0)] {
            if !given {
                self.len[slot] = None;
            }
        }
        for (i, (body, text)) in [(W, TW), (H, TH)].into_iter().enumerate() {
            if !self.body[i] {
                self.scale[i] = None;
                self.len[body] = None;
                self.len[text] = None;
            }
        }
    }

    /// `\Gm@@process` with the current `\headheight`, `\headsep`, `\footskip` (`head`,
    /// replaced by the keys') and `\if@twoside` (also `\if@mparswitch` in the standard
    /// classes): `[\textwidth, \textheight, \oddsidemargin, \evensidemargin, \topmargin]`
    /// and the three head lengths.
    pub fn process(&mut self, head: [i32; 3], twoside: bool) -> ([i32; 5], [i32; 3]) {
        let [hh, hs, fs] = [0, 1, 2].map(|i| self.head[i].unwrap_or(head[i]));
        let [pw, ph] = self.page();
        // `\Gm@adjustbody`.
        if self.body[0] {
            self.len[W] = self.len[TW].or(self.len[W]).or(Some(scale(pw, self.scale[0])));
        }
        if self.body[1] {
            if let Some(th) = self.len[TH] {
                self.len[H] = Some(th + if self.include[0] { hh + hs } else { 0 } + if self.include[1] { fs } else { 0 });
            } else if self.len[H].is_none() {
                self.len[H] = Some(scale(ph, self.scale[1]));
            }
        }
        self.cnt[0] = self.detall([W, L, R], pw, if twoside { (2, 3) } else { (1, 1) }, self.body[0]);
        self.cnt[1] = self.detall([H, T, B], ph, (2, 3), self.body[1]);
        let at = |s: usize| self.len[s].unwrap_or(0);
        let odd = at(L) - INCH;
        let even = if twoside { at(R) - INCH } else { odd };
        let (mut height, mut top) = (at(H), at(T) - INCH);
        if self.include[0] {
            height -= hh + hs;
        } else {
            top -= hh + hs;
        }
        if self.include[1] {
            height -= fs;
        }
        ([at(W), height, odd, even, top], [hh, hs, fs])
    }

    /// `\Gm@detall` in one direction: the body slot, its two margins, the layout length and
    /// the default margin ratio. Returns `\Gm@cnth` (or `\Gm@cntv`).
    fn detall(&mut self, [w, a, b]: [usize; 3], layout: i32, ratio: (i32, i32), body: bool) -> u8 {
        let c = (self.len[a].is_some() as u8) << 2 | (body as u8) << 1 | self.len[b].is_some() as u8;
        let get = |g: &Geometry, s: usize| g.len[s].unwrap_or(0);
        // `\Gm@detiiandiii{body}{x}{y}`: the rest of the layout split by the ratio, `x` first.
        let split = |g: &mut Geometry, x: usize, y: usize| {
            let rest = layout - get(g, w);
            let first = rest / (ratio.0 + ratio.1) * ratio.0;
            g.len[x] = Some(first);
            g.len[y] = Some(rest - first);
        };
        // `\Gm@detiv`: `out` is the layout less the other two.
        let fill = |g: &mut Geometry, x: usize, y: usize, out: usize| g.len[out] = Some(layout - get(g, x) - get(g, y));
        match c {
            0 => {
                self.len[w] = Some(scale(layout, None));
                split(self, a, b);
            }
            1 | 4 => {
                // One margin: the other from the default body's split, then the body.
                let (given, other) = if c == 1 { (b, a) } else { (a, b) };
                let keep = self.len[given];
                self.len[w] = Some(scale(layout, None));
                split(self, other, given);
                self.len[w] = None;
                self.len[given] = keep;
                fill(self, a, b, w);
            }
            2 => split(self, a, b),
            3 => fill(self, w, b, a),
            6 => fill(self, w, a, b),
            _ => fill(self, a, b, w),
        }
        c
    }
}

/// `0.7\Gm@layoutwidth` (`\Gm@Dhscale`, `\Gm@Dvscale`), or `\Gm@hscale\Gm@layoutwidth`
/// with `factor`: §453's coefficient (`0.7` is 45875/65536), times the length, truncated.
/// For a factor with an integer part `i`, §453's `nx_plus_y(i, v, xn_over_d(v, f, 2^16))`
/// is the same number for the positive lengths of a page.
fn scale(v: i32, factor: Option<i32>) -> i32 {
    let p = v as i64 * factor.unwrap_or(45875) as i64;
    (if p < 0 { -((-p) >> 16) } else { p >> 16 }) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sp(pt: f64) -> i32 {
        (pt * 65536.0).round() as i32
    }

    /// Article's head lengths (12pt, 25pt, 30pt) on letter paper, against pdfLaTeX's
    /// `\the\textwidth`, `\the\textheight`, `\the\oddsidemargin`, `\the\evensidemargin`,
    /// `\the\topmargin`.
    #[test]
    fn layouts_match_pdflatex() {
        let cases: [(&str, bool, [f64; 5]); 4] = [
            ("margin=1in", false, [469.75502, 650.43001, 0.0, 0.0, -37.0]),
            ("", false, [430.00462, 556.47656, 19.8752, 19.8752, -13.87262]),
            ("lmargin=1in", false, [449.87982, 556.47656, 0.0, 0.0, -13.87262]),
            ("left=1in,top=1in", true, [468.30888, 627.30263, 0.0, 1.44614, -37.0]),
        ];
        for (opts, twoside, want) in cases {
            let mut g = Geometry::new([40258437, 52099153]);
            g.keys(opts, &mut |v| (v == "1in").then_some(INCH)).unwrap();
            let (dims, _) = g.process([sp(12.0), sp(25.0), sp(30.0)], twoside);
            assert_eq!(dims, want.map(sp), "{opts}");
        }
        // `scale=0.8`: `0.8pt` is 52429sp, the factor TeX scans.
        let mut g = Geometry::new([40258437, 52099153]);
        g.keys("scale=0.8", &mut |v| (v == "0.8pt").then_some(52429)).unwrap();
        let (dims, _) = g.process([sp(12.0), sp(25.0), sp(30.0)], false);
        assert_eq!(dims, [491.43787, 635.97841, -10.84143, -10.84143, -45.67337].map(sp));
        // `noheadfoot`, then a head height after it: the later key wins (`\Gm@dimlist` in order).
        let mut g = Geometry::new([40258437, 52099153]);
        g.keys("noheadfoot,headheight=1in,paperwidth=1in,papersize={1in}", &mut |v| (v == "1in").then_some(INCH)).unwrap();
        assert_eq!((g.head, g.paper), ([Some(INCH), Some(0), Some(0)], [INCH, INCH]));
        assert!(g.keys("scale=0.8cm", &mut |_| Some(0)).is_err());
    }
}
