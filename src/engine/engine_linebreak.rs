//! Breaking paragraphs into lines: tex.web §813–§890 (`line_break`, `try_break`,
//! `post_line_break`), in our own form.
//!
//! TeX keeps the active breakpoints in a linked list with "delta nodes" between them that
//! hold width differences. Here the active list is a vector in TeX's order, and each active
//! node keeps its origin instead: the running total of the paragraph's widths at its break
//! minus TeX's `break_width` there. The width of a line from an active node to the current
//! position is then `total - origin`, the same integer sums TeX's deltas add up to. Every
//! decision (feasibility, fitness classes, demerits and their ties, the passes, looseness)
//! follows tex.web exactly.

use super::engine_eqtb::{Glue, Reg, Region};
use super::engine_font::Fonts;
use super::engine_microtype::{char_pw, protchar_left, protchar_right};
use super::engine_node::{Kind, Node, Nodes};
use super::engine_pack::badness;
use super::engine_run::Machine;
use super::engine_whatsit::sized;

const INF_BAD: i32 = 10_000;
const INF_PENALTY: i32 = 10_000;
const EJECT_PENALTY: i32 = -10_000;
/// §833 `awful_bad`.
const AWFUL_BAD: i32 = 0o7777777777;

// Parameter slots (tex.web §236, §247, §224).
const PRETOLERANCE: u32 = 0;
const TOLERANCE: u32 = 1;
const LINE_PENALTY: u32 = 2;
const HYPHEN_PENALTY: u32 = 3;
const EX_HYPHEN_PENALTY: u32 = 4;
const CLUB_PENALTY: u32 = 5;
const WIDOW_PENALTY: u32 = 6;
const DISPLAY_WIDOW_PENALTY: u32 = 7;
const BROKEN_PENALTY: u32 = 8;
const INTER_LINE_PENALTY: u32 = 13;
const DOUBLE_HYPHEN_DEMERITS: u32 = 14;
const FINAL_HYPHEN_DEMERITS: u32 = 15;
const ADJ_DEMERITS: u32 = 16;
const TRACING_PARAGRAPHS: u32 = 32;
const PDF_PROTRUDE_CHARS: u32 = 76;
const PDF_ADJUST_SPACING: u32 = 75;
const LOOSENESS: u32 = 19;
const HANG_AFTER: u32 = 41;
const HSIZE: u32 = 3;
const HANG_INDENT: u32 = 17;
const EMERGENCY_STRETCH: u32 = 20;
const LEFT_SKIP: u32 = 7;
const RIGHT_SKIP: u32 = 8;
const PAR_FILL_SKIP: u32 = 14;

/// Widths in TeX's six components (§823): natural width, stretch of order normal, fil,
/// fill, filll, and shrink; then pdfTeX's font stretch and shrink (`\pdfadjustspacing > 1`).
type Widths = [i32; 8];

/// `a - b`, component by component.
fn diff(a: Widths, b: Widths) -> Widths {
    std::array::from_fn(|k| a[k] - b[k])
}

fn sum(a: Widths, b: Widths) -> Widths {
    std::array::from_fn(|k| a[k] + b[k])
}

fn add(a: &mut Widths, g: Glue) {
    a[0] += g.width;
    a[1 + g.stretch_order as usize] += g.stretch;
    a[5] += g.shrink;
}

/// §819: an active breakpoint.
#[derive(Clone, Copy, Debug)]
struct Active {
    /// The passive node of its break, `None` for the paragraph's start.
    passive: Option<u32>,
    line: i32,
    fitness: u8,
    hyphenated: bool,
    total: i32,
    origin: Widths,
}

/// §821: a breakpoint some active node or chosen path went through.
#[derive(Clone, Copy, Debug)]
struct Passive {
    /// The index of the break node in the paragraph, `None` at its end.
    at: Option<u32>,
    prev: Option<u32>,
}

/// The paragraph's line widths and indents (§847–§849).
#[derive(Clone)]
struct Shape {
    easy_line: i32,
    last_special_line: i32,
    first_width: i32,
    first_indent: i32,
    second_width: i32,
    second_indent: i32,
    par_shape: Vec<[i32; 2]>,
}

impl Shape {
    /// (indent, width) of line `l`.
    fn line(&self, l: i32) -> (i32, i32) {
        if l > self.last_special_line {
            (self.second_indent, self.second_width)
        } else if self.par_shape.is_empty() {
            (self.first_indent, self.first_width)
        } else {
            let [i, w] = self.par_shape[l as usize - 1];
            (i, w)
        }
    }
}

/// The state of one `line_break` call.
struct Breaker<'p> {
    par: &'p [Node],
    nodes: &'p Nodes,
    fonts: &'p Fonts,
    /// `\pdfprotrudechars > 1`: protrusion counts in the line widths (pdfTeX).
    protrude: bool,
    /// pdfTeX's paragraph-wide expansion limits (`check_expand_pars`): `cur_font_step`,
    /// `max_stretch_ratio`, `max_shrink_ratio`; `None` without `\pdfadjustspacing > 1`.
    expand: Option<[i32; 3]>,
    /// §839 `disc_width`: the pre-break text of the discretionary being tried.
    disc_width: Widths,
    actives: Vec<Active>,
    passives: Vec<Passive>,
    /// §823 `background` and the running total of widths before the current node.
    background: Widths,
    total: Widths,
    threshold: i32,
    final_pass: bool,
    minimal: [i32; 4],
    minimum: i32,
    best_place: [Option<u32>; 4],
    best_line: [i32; 4],
    shape: Shape,
    line_penalty: i32,
    adj_demerits: i32,
    double_hyphen: i32,
    final_hyphen: i32,
    /// `\tracingparagraphs > 0`: feasible breaks and new active nodes go to stderr in
    /// pdfTeX's `@ via @@n` / `@@n: line` form (§846, §856), for diffing against its log.
    trace: bool,
}

impl Breaker<'_> {
    /// §829 `try_break` at node `at` (`None` for the paragraph's end).
    fn try_break(&mut self, widths: &impl Fn(&[Node], usize) -> Widths, mut pi: i32, hyphenated: bool, at: Option<usize>) {
        if pi.abs() >= INF_PENALTY {
            if pi > 0 {
                return;
            }
            pi = EJECT_PENALTY;
        }
        let mut break_width: Option<Widths> = None;
        let mut old_l = 0;
        let mut line_width = 0;
        let mut i = 0;
        loop {
            let len = self.actives.len();
            let l = if i < len { self.actives[i].line } else { i32::MAX };
            // §835: a line-number class has ended.
            if l > old_l {
                if self.minimum < AWFUL_BAD && (old_l != self.shape.easy_line || i == len) {
                    let bw = *break_width.get_or_insert_with(|| self.break_width(widths, hyphenated, at));
                    let origin = diff(self.total, bw);
                    // §836
                    if self.adj_demerits.abs() >= AWFUL_BAD - self.minimum {
                        self.minimum = AWFUL_BAD - 1;
                    } else {
                        self.minimum += self.adj_demerits.abs();
                    }
                    for fit in 0..4 {
                        if self.minimal[fit] <= self.minimum {
                            self.passives.push(Passive { at: at.map(|a| a as u32), prev: self.best_place[fit] });
                            if self.trace {
                                // §846
                                let prev = self.best_place[fit].map_or(0, |p| p + 1);
                                eprintln!("@@{}: line {}.{}{} t={} -> @@{prev}", self.passives.len(), self.best_line[fit], fit, if hyphenated { "-" } else { "" }, self.minimal[fit]);
                            }
                            let a = Active { passive: Some(self.passives.len() as u32 - 1), line: self.best_line[fit] + 1, fitness: fit as u8, hyphenated, total: self.minimal[fit], origin };
                            self.actives.insert(i, a);
                            i += 1;
                        }
                        self.minimal[fit] = AWFUL_BAD;
                    }
                    self.minimum = AWFUL_BAD;
                }
                if i == self.actives.len() {
                    return;
                }
                // §850
                if l > self.shape.easy_line {
                    line_width = self.shape.second_width;
                    old_l = i32::MAX - 1;
                } else {
                    old_l = l;
                    line_width = self.shape.line(l).1;
                }
            }
            // §851: the badness and fitness class of a line from node `i` to here.
            let r = self.actives[i];
            let w = diff(self.total, r.origin);
            let mut shortfall = line_width - w[0];
            if self.protrude {
                shortfall += self.total_pw(r.passive, at);
            }
            // pdfTeX: the fonts' stretch or shrink absorbs the shortfall, to half a step
            // when it can absorb all of it. (A margin kern's character in an automatically
            // expanded font keeps its base's quad, so margin kerns add no variation.)
            if let Some([step, max_stretch, max_shrink]) = self.expand.filter(|_| shortfall != 0) {
                if shortfall > 0 && w[6] > 0 {
                    shortfall = if w[6] > shortfall { (w[6] / (max_stretch / step)) / 2 } else { shortfall - w[6] };
                } else if shortfall < 0 && w[7] > 0 {
                    shortfall = if w[7] > -shortfall { -((w[7] / (max_shrink / step)) / 2) } else { shortfall + w[7] };
                }
            }
            let (b, fit) = if shortfall > 0 {
                if w[2] != 0 || w[3] != 0 || w[4] != 0 {
                    (0, 2)
                } else if shortfall > 7_230_584 && w[1] < 1_663_497 {
                    (INF_BAD, 0)
                } else {
                    let b = badness(shortfall, w[1]);
                    (b, if b > 12 { if b > 99 { 0 } else { 1 } } else { 2 })
                }
            } else {
                let b = if -shortfall > w[5] { INF_BAD + 1 } else { badness(-shortfall, w[5]) };
                (b, if b > 12 { 3 } else { 2 })
            };
            let mut artificial = false;
            let stays = if b > INF_BAD || pi == EJECT_PENALTY {
                // §854: the last resort of the final pass keeps one break alive.
                if self.final_pass && self.minimum == AWFUL_BAD && i + 1 == self.actives.len() && i == 0 {
                    artificial = true;
                } else if b > self.threshold {
                    self.actives.remove(i);
                    continue;
                }
                false
            } else {
                if b > self.threshold {
                    i += 1;
                    continue;
                }
                true
            };
            // §855, §859: the demerits.
            let mut d = if artificial {
                0
            } else {
                let mut d = self.line_penalty + b;
                d = if d.abs() >= 10_000 { 100_000_000 } else { d * d };
                if pi != 0 {
                    if pi > 0 {
                        d += pi * pi;
                    } else if pi > EJECT_PENALTY {
                        d -= pi * pi;
                    }
                }
                if hyphenated && r.hyphenated {
                    d += if at.is_some() { self.double_hyphen } else { self.final_hyphen };
                }
                if (fit as i32 - r.fitness as i32).abs() > 1 {
                    d += self.adj_demerits;
                }
                d
            };
            if self.trace {
                // §856
                let kind = match at.map(|a| self.par[a].kind) {
                    None => "\\par",
                    Some(Kind::Glue) => "",
                    Some(Kind::Penalty) => "\\penalty",
                    Some(Kind::Disc) => "\\discretionary",
                    Some(Kind::Kern) => "\\kern",
                    Some(_) => "\\math",
                };
                let b = if b > INF_BAD { "*".to_string() } else { b.to_string() };
                let d = if artificial { "*".to_string() } else { d.to_string() };
                eprintln!("@{kind} via @@{} b={b} p={pi} d={d}", r.passive.map_or(0, |p| p + 1));
            }
            d += r.total;
            if d <= self.minimal[fit] {
                self.minimal[fit] = d;
                self.best_place[fit] = r.passive;
                self.best_line[fit] = l;
                if d < self.minimum {
                    self.minimum = d;
                }
            }
            if stays {
                i += 1;
            } else {
                self.actives.remove(i);
            }
        }
    }

    /// pdftex.web `total_pw`: the protrusion at both ends of the line from active node
    /// `passive`'s break to a break at `at`.
    fn total_pw(&self, passive: Option<u32>, at: Option<usize>) -> i32 {
        let (par, nodes) = (self.par, self.nodes);
        let l = passive.map_or(0, |p| self.passives[p as usize].at.map_or(par.len(), |a| a as usize));
        let end = at.unwrap_or(par.len());
        // The right margin: a discretionary's pre-break text ends in its last character.
        let right = match at.map(|a| par[a]) {
            Some(n) if n.kind == Kind::Disc && !nodes.get(disc_lists(nodes, &n)[0]).is_empty() => nodes.get(disc_lists(nodes, &n)[0]).last().copied(),
            _ => protchar_right(nodes, &par[l.min(end)..end]),
        };
        // The left margin: after a discretionary, its post-break text, else what follows
        // the text it replaced.
        let left = match par.get(l) {
            Some(n) if passive.is_some() && n.kind == Kind::Disc => {
                let post = nodes.get(disc_lists(nodes, n)[1]);
                match post.first() {
                    Some(&first) => Some(first),
                    None => protchar_left(nodes, &par[(l + 1 + n.sub as usize).min(par.len() - 1)..], true),
                }
            }
            Some(_) => protchar_left(nodes, &par[l..], true),
            None => None,
        };
        char_pw(self.fonts, left, false) + char_pw(self.fonts, right, true)
    }

    /// §837–§840 `break_width`: the background less what disappears after a break at `at`.
    fn break_width(&self, widths: &impl Fn(&[Node], usize) -> Widths, hyphenated: bool, at: Option<usize>) -> Widths {
        let mut bw = self.background;
        let Some(mut s) = at else { return bw };
        // §840: at a discretionary, the replaced text goes and the post-break text comes;
        // only an empty post-break text lets what follows be discarded too.
        if hyphenated && self.par[s].kind == Kind::Disc {
            let d = self.par[s];
            let t = d.sub as usize;
            for i in s + 1..s + 1 + t {
                bw = diff(bw, widths(self.par, i));
            }
            let [_, post] = disc_lists(self.nodes, &d);
            let post_list = self.nodes.get(post);
            for i in 0..post_list.len() {
                bw = sum(bw, widths(post_list, i));
            }
            bw = sum(bw, self.disc_width);
            if post.0 != post.1 {
                return bw;
            }
            s += 1 + t;
        }
        while let Some(n) = self.par.get(s) {
            match n.kind {
                Kind::Glue => bw = diff(bw, widths(self.par, s)),
                Kind::Penalty => {}
                Kind::Kern if n.sub == 1 => bw[0] -= n.a,
                Kind::Math => bw[0] -= n.a,
                _ => break,
            }
            s += 1;
        }
        bw
    }
}

/// The pre-break and post-break lists of a discretionary.
fn disc_lists(nodes: &Nodes, n: &Node) -> [(u32, u32); 2] {
    if n.a < 0 {
        [(0, 0); 2]
    } else {
        nodes.discs[n.a as usize]
    }
}

impl Machine<'_, '_> {
    pub(super) fn ip(&self, slot: u32) -> i32 {
        self.e.eqtb.word(Reg { region: Region::Int, slot })
    }

    fn dp(&self, slot: u32) -> i32 {
        self.e.eqtb.word(Reg { region: Region::Dimen, slot })
    }

    fn gp(&self, slot: u32) -> Glue {
        self.e.eqtb.glue(Reg { region: Region::Glue, slot })
    }

    /// §1096 `end_graf` and §815 `line_break`: the paragraph from `start` in the build buffer
    /// (the current horizontal level) becomes lines on the enclosing vertical list.
    /// `display`: e-TeX's `line_break(d)`, for the lines before a display, which end with
    /// `\displaywidowpenalty`.
    pub(super) fn line_break(&mut self, display: bool) {
        let start = self.start as usize;
        // §816: the paragraph ends with an infinite penalty and `\parfillskip` (a final
        // glue becomes the penalty).
        if self.cur.last().is_some_and(|n| n.kind == Kind::Glue) && self.cur.len() > start {
            if let Some(n) = self.cur.pop() {
                self.e.nodes.flush_node(n);
            }
        }
        self.cur.push(Node::new(Kind::Penalty, INF_PENALTY));
        let fill = self.e.nodes.glue_node(self.gp(PAR_FILL_SKIP), PAR_FILL_SKIP as u8 + 1);
        self.cur.push(fill);
        let mut par: Vec<Node> = self.cur.drain(start..).collect();
        // §891: the paragraph's language and hyphen minima, which `new_graf` packed into
        // the horizontal list's `prev_graf` (§1091).
        let par_hyph = self.prev_graf;
        self.pop_nest();

        // §827, §848: background, line widths.
        let check = |m: &mut Self, mut g: Glue| {
            if g.shrink_order != 0 && g.shrink != 0 {
                m.error("Infinite glue shrinkage found in a paragraph");
                g.shrink_order = 0;
            }
            g
        };
        let (left, right) = (self.gp(LEFT_SKIP), self.gp(RIGHT_SKIP));
        let (left, right) = (check(self, left), check(self, right));
        let mut background = [0; 8];
        add(&mut background, left);
        add(&mut background, right);
        let hsize = self.dp(HSIZE);
        let par_shape = self.e.eqtb.par_shape().to_vec();
        let (hang_indent, hang_after) = (self.dp(HANG_INDENT), self.ip(HANG_AFTER));
        let mut shape = Shape { easy_line: 0, last_special_line: 0, first_width: 0, first_indent: 0, second_width: hsize, second_indent: 0, par_shape };
        if shape.par_shape.is_empty() {
            if hang_indent != 0 {
                shape.last_special_line = hang_after.abs();
                let narrow = hsize - hang_indent.abs();
                let indent = hang_indent.max(0);
                if hang_after < 0 {
                    (shape.first_width, shape.first_indent) = (narrow, indent);
                } else {
                    (shape.first_width, shape.first_indent) = (hsize, 0);
                    (shape.second_width, shape.second_indent) = (narrow, indent);
                }
            }
        } else {
            let n = shape.par_shape.len();
            shape.last_special_line = n as i32 - 1;
            [shape.second_indent, shape.second_width] = shape.par_shape[n - 1];
        }
        let looseness = self.ip(LOOSENESS);
        shape.easy_line = if looseness == 0 { shape.last_special_line } else { i32::MAX };

        let nodes = &self.e.nodes;
        if par.iter().any(|n| n.kind == Kind::Glue && nodes.glue(n).shrink_order != 0 && nodes.glue(n).shrink != 0) {
            self.errors.push(format!("l.{} [{}] Infinite glue shrinkage found in a paragraph", self.input.line(), self.input.file_depth()));
        }
        let prev_graf = self.prev_graf;
        // §863: the first pass, without hyphenation; then (§891) every word is hyphenated
        // before the second, which is what TeX's word-at-a-time hyphenation in that pass
        // amounts to, since each word depends only on its own nodes.
        let first = if self.ip(PRETOLERANCE) >= 0 { self.passes(&par, &shape, background, prev_graf, false) } else { None };
        let (breaks, last_line) = match first {
            Some(b) => b,
            None => {
                self.e.hyph.ready = true;
                self.hyphenate_par(&mut par, par_hyph);
                self.passes(&par, &shape, background, prev_graf, true).expect("the final pass always succeeds")
            }
        };
        self.post_line_break(&par, &breaks, &shape, left, prev_graf, display);
        self.prev_graf = last_line - 1;
    }

    /// §863–§876: the breaks of one pass (`second`: the second and, if need be, the final
    /// pass), with the best active node's line number; `None` when the first pass fails.
    fn passes(&self, par: &[Node], shape: &Shape, background: Widths, prev_graf: i32, second: bool) -> Option<(Vec<Option<u32>>, i32)> {
        let looseness = self.ip(LOOSENESS);
        // Every node's six widths: characters from their fonts, glue (with §826 finite
        // shrinkage), boxes, rules, kerns.
        let nodes = &self.e.nodes;
        let fonts = &self.e.fonts;
        let expand = (self.ip(PDF_ADJUST_SPACING) > 1).then(|| self.expand_pars(par)).flatten();
        // Node `i` of `list` (the paragraph or a discretionary's text): its widths, with
        // pdfTeX's font stretch and shrink of a character or of a font kern between two.
        let widths = |list: &[Node], i: usize| -> Widths {
            let n = &list[i];
            let mut w = [0; 8];
            match n.kind {
                Kind::Char | Kind::Lig => {
                    let f = &fonts.list[n.font as usize];
                    let c = n.a as u8;
                    w[0] = u8::try_from(n.a).ok().and_then(|c| f.dims(c)).map_or(0, |d| d[0]);
                    if expand.is_some() && fonts.micro.expand(n.font).step != 0 {
                        w[6] = self.char_flex(n.font, c, false);
                        w[7] = self.char_flex(n.font, c, true);
                    }
                }
                Kind::Kern if n.sub == 0 && expand.is_some() => {
                    w[0] = n.a;
                    if let (Some(l), Some(&r)) = (i.checked_sub(1).map(|j| list[j]), list.get(i + 1)) {
                        w[6] = self.kern_flex(l, n.a, r, false);
                        w[7] = self.kern_flex(l, n.a, r, true);
                    }
                }
                Kind::Glue => {
                    let mut g = nodes.glue(n);
                    if g.shrink_order != 0 && g.shrink != 0 {
                        g.shrink_order = 0;
                    }
                    add(&mut w, g);
                }
                Kind::Box => w[0] = nodes.boxes[n.a as usize].width,
                Kind::Rule => w[0] = nodes.rules[n.a as usize][0],
                Kind::Kern | Kind::Math => w[0] = n.a,
                Kind::Whatsit if sized(n.sub) => w[0] = nodes.rules[n.a as usize][0],
                Kind::Penalty | Kind::Disc | Kind::Ins | Kind::Mark | Kind::Adjust | Kind::Whatsit | Kind::Noad => {}
            }
            w
        };

        let mut br = Breaker {
            par,
            nodes,
            fonts,
            protrude: self.ip(PDF_PROTRUDE_CHARS) > 1,
            expand,
            disc_width: [0; 8],
            actives: Vec::new(),
            passives: Vec::new(),
            background,
            total: [0; 8],
            threshold: 0,
            final_pass: false,
            minimal: [AWFUL_BAD; 4],
            minimum: AWFUL_BAD,
            best_place: [None; 4],
            best_line: [0; 4],
            shape: shape.clone(),
            line_penalty: self.ip(LINE_PENALTY),
            adj_demerits: self.ip(ADJ_DEMERITS),
            double_hyphen: self.ip(DOUBLE_HYPHEN_DEMERITS),
            final_hyphen: self.ip(FINAL_HYPHEN_DEMERITS),
            trace: self.ip(TRACING_PARAGRAPHS) > 0,
        };
        // §863: the passes.
        let (hyphen_penalty, ex_hyphen_penalty) = (self.ip(HYPHEN_PENALTY), self.ip(EX_HYPHEN_PENALTY));
        let pretolerance = self.ip(PRETOLERANCE);
        let tolerance = self.ip(TOLERANCE);
        let emergency = self.dp(EMERGENCY_STRETCH);
        if second {
            br.threshold = tolerance;
            br.final_pass = emergency <= 0;
        } else {
            br.threshold = pretolerance;
            br.final_pass = false;
        }
        let best = loop {
            br.threshold = br.threshold.min(INF_BAD);
            if br.trace {
                eprintln!("{}", if br.threshold == pretolerance && !second { "@firstpass" } else { "@secondpass" });
            }
            // §864: the paragraph's start.
            br.passives.clear();
            br.actives.clear();
            br.actives.push(Active { passive: None, line: prev_graf + 1, fitness: 2, hyphenated: false, total: 0, origin: br.background.map(|v| -v) });
            br.total = [0; 8];
            br.minimal = [AWFUL_BAD; 4];
            br.minimum = AWFUL_BAD;
            // §866: false inside math, which the engine's lists do not hold yet.
            let mut auto_breaking = true;
            let mut prev: Option<Kind> = None;
            let mut prev_explicit_kern = false;
            let mut k = 0;
            // §866
            while k < par.len() && !br.actives.is_empty() {
                let n = par[k];
                match n.kind {
                    Kind::Char => {
                        // §867: a run of characters.
                        while k < par.len() && par[k].kind == Kind::Char {
                            br.total = sum(br.total, widths(par, k));
                            k += 1;
                        }
                        prev = Some(Kind::Char);
                        prev_explicit_kern = false;
                        continue;
                    }
                    Kind::Box | Kind::Rule | Kind::Lig => br.total = sum(br.total, widths(par, k)),
                    // §869: a break after the pre-break text, then past the replaced text.
                    Kind::Disc => {
                        let [pre, _] = disc_lists(nodes, &n);
                        let pre_list = nodes.get(pre);
                        br.disc_width = (0..pre_list.len()).fold([0; 8], |a, i| sum(a, widths(pre_list, i)));
                        if pre.0 == pre.1 {
                            br.try_break(&widths, ex_hyphen_penalty, true, Some(k));
                        } else {
                            let before = br.total;
                            br.total = sum(br.total, br.disc_width);
                            br.try_break(&widths, hyphen_penalty, true, Some(k));
                            br.total = before;
                        }
                        let r = n.sub as usize;
                        for s in k + 1..(k + 1 + r).min(par.len()) {
                            br.total = sum(br.total, widths(par, s));
                        }
                        prev = Some(Kind::Disc);
                        prev_explicit_kern = false;
                        k += 1 + r;
                        continue;
                    }
                    Kind::Glue => {
                        // §868: glue is a legal break after a character, a box, a rule or a
                        // kern that is not explicit.
                        if auto_breaking {
                            let legal = match prev {
                                // §148 `precedes_break`: also insertions, marks and adjustments.
                                Some(Kind::Char | Kind::Box | Kind::Rule | Kind::Lig | Kind::Disc | Kind::Ins | Kind::Mark | Kind::Adjust | Kind::Whatsit) => true,
                                Some(Kind::Kern) => !prev_explicit_kern,
                                _ => false,
                            };
                            if legal {
                                br.try_break(&widths, 0, false, Some(k));
                            }
                        }
                        br.total = sum(br.total, widths(par, k));
                    }
                    Kind::Kern => {
                        // `kern_break`: an explicit kern before glue is a legal break.
                        if n.sub == 1 && auto_breaking && par.get(k + 1).is_some_and(|m| m.kind == Kind::Glue) {
                            br.try_break(&widths, 0, false, Some(k));
                        }
                        br.total = sum(br.total, widths(par, k));
                    }
                    // §866: a math node switches automatic breaking off (before) or on
                    // (after), and is a legal break before glue (`kern_break`).
                    Kind::Math => {
                        auto_breaking = n.sub == 1;
                        if auto_breaking && par.get(k + 1).is_some_and(|m| m.kind == Kind::Glue) {
                            br.try_break(&widths, 0, false, Some(k));
                        }
                        br.total[0] += n.a;
                    }
                    Kind::Penalty => br.try_break(&widths, n.a, false, Some(k)),
                    // pdfTeX: an image whatsit adds its width (`act_width`).
                    Kind::Whatsit if sized(n.sub) => br.total[0] += nodes.rules[n.a as usize][0],
                    // §866: `mark_node, ins_node, adjust_node: do_nothing`.
                    Kind::Ins | Kind::Mark | Kind::Adjust | Kind::Whatsit | Kind::Noad => {}
                }
                prev = Some(n.kind);
                prev_explicit_kern = n.kind == Kind::Kern && n.sub == 1;
                k += 1;
            }
            if k >= par.len() {
                // §873: the final break.
                br.try_break(&widths, EJECT_PENALTY, true, None);
                if !br.actives.is_empty() {
                    // §874: the fewest demerits (the first of equals).
                    let mut best = 0;
                    for (j, a) in br.actives.iter().enumerate() {
                        if a.total < br.actives[best].total {
                            best = j;
                        }
                    }
                    if looseness == 0 {
                        break best;
                    }
                    // §875: the best node for the looseness asked for.
                    let best_line = br.actives[best].line;
                    let mut actual = 0;
                    let mut fewest = br.actives[best].total;
                    for (j, a) in br.actives.iter().enumerate() {
                        let diff = a.line - best_line;
                        if (diff < actual && looseness <= diff) || (diff > actual && looseness >= diff) {
                            best = j;
                            actual = diff;
                            fewest = a.total;
                        } else if diff == actual && a.total < fewest {
                            best = j;
                            fewest = a.total;
                        }
                    }
                    if actual == looseness || br.final_pass {
                        break best;
                    }
                }
            }
            // §873: if at first you don't succeed ...
            if !second {
                return None;
            } else {
                br.background[1] += emergency;
                br.final_pass = true;
            }
        };
        let best = br.actives[best];
        // §878: the chosen breaks, first to last.
        let mut breaks = Vec::new();
        let mut p = best.passive;
        while let Some(i) = p {
            breaks.push(br.passives[i as usize].at);
            p = br.passives[i as usize].prev;
        }
        breaks.reverse();
        Some((breaks, best.line))
    }

    /// §877–§890 `post_line_break`: each line packaged to its width, with `\leftskip`,
    /// `\rightskip`, and the interline penalties.
    fn post_line_break(&mut self, par: &[Node], breaks: &[Option<u32>], shape: &Shape, left: Glue, prev_graf: i32, display: bool) {
        let right = self.gp(RIGHT_SKIP);
        let protrude = self.ip(PDF_PROTRUDE_CHARS) > 0;
        let best_line = prev_graf + 1 + breaks.len() as i32;
        let mut pos = 0usize;
        // §882: a discretionary's post-break text, which begins the next line.
        let mut carry: Vec<Node> = Vec::new();
        for (j, &at) in breaks.iter().enumerate() {
            let cur_line = prev_graf + 1 + j as i32;
            let line_start = self.cur.len();
            // §887: `\leftskip`, unless zero.
            if left != Glue::default() {
                let g = self.e.nodes.glue_node(left, LEFT_SKIP as u8 + 1);
                self.cur.push(g);
            }
            let content_start = self.cur.len();
            self.cur.append(&mut carry);
            let mut disc_break = false;
            // pdfTeX: where the right margin's search ends (the node before the break),
            // or the last node of a discretionary's pre-break text, taken as it is.
            let mut right_end: usize;
            let mut pre_last = false;
            // §881: the break node: glue becomes `\rightskip`; a kern loses its width; a
            // discretionary becomes compulsory (§882): the text it replaces goes, its
            // pre-break text ends this line and its post-break text starts the next.
            let next = match at {
                Some(q) => {
                    let q = q as usize;
                    let n = par[q];
                    self.cur.extend_from_slice(&par[pos..q]);
                    match n.kind {
                        Kind::Glue => {
                            right_end = self.cur.len();
                            self.e.nodes.flush_node(n);
                            q + 1
                        }
                        Kind::Kern | Kind::Math => {
                            right_end = self.cur.len();
                            self.cur.push(Node { a: 0, ..n });
                            q + 1
                        }
                        Kind::Disc => {
                            let [pre, post] = disc_lists(&self.e.nodes, &n);
                            right_end = self.cur.len();
                            self.cur.push(Node { sub: 0, a: -1, ..n });
                            let nodes = &self.e.nodes;
                            let pre = nodes.get(pre);
                            if !pre.is_empty() {
                                pre_last = true;
                                self.cur.extend_from_slice(pre);
                                right_end = self.cur.len();
                            }
                            carry.extend_from_slice(nodes.get(post));
                            disc_break = true;
                            // Both texts moved into the lines; the replaced nodes go.
                            let [pre, post] = disc_lists(&self.e.nodes, &n);
                            self.e.nodes.free_block(pre);
                            self.e.nodes.free_block(post);
                            if n.a >= 0 {
                                self.e.nodes.free_disc(n.a as u32);
                            }
                            for &r in &par[q + 1..(q + 1 + n.sub as usize).min(par.len())] {
                                self.e.nodes.flush_node(r);
                            }
                            q + 1 + n.sub as usize
                        }
                        _ => {
                            right_end = self.cur.len();
                            self.cur.push(n);
                            q + 1
                        }
                    }
                }
                None => {
                    self.cur.extend_from_slice(&par[pos..]);
                    right_end = self.cur.len().saturating_sub(1).max(content_start);
                    par.len()
                }
            };
            // pdfTeX's character protrusion (post_line_break): a margin kern after the
            // rightmost and before the leftmost protruding character.
            if protrude {
                let end = right_end;
                let ch = if pre_last { self.cur.get(end - 1).copied() } else { protchar_right(&self.e.nodes, &self.cur[content_start..end]) };
                let w = char_pw(&self.e.fonts, ch, true);
                if let (true, Some(ch)) = (w != 0, ch) {
                    self.cur.insert(end, Self::margin_kern(w, ch, true));
                }
                let ch = protchar_left(&self.e.nodes, &self.cur[content_start..], false);
                let w = char_pw(&self.e.fonts, ch, false);
                if let (true, Some(ch)) = (w != 0, ch) {
                    self.cur.insert(content_start, Self::margin_kern(w, ch, false));
                }
            }
            let g = self.e.nodes.glue_node(right, RIGHT_SKIP as u8 + 1);
            self.cur.push(g);
            // §889: packaged to the line's width, shifted by its indent.
            let (indent, width) = shape.line(cur_line);
            // pdfTeX's font expansion of the line (`hpack` with `cal_expand_ratio`).
            if self.ip(PDF_ADJUST_SPACING) > 0 {
                self.expand_line(line_start, width);
            }
            self.adjust = Some(Vec::new());
            let mut b = self.hpack(line_start, width, true);
            b.shift = indent;
            let idx = self.e.nodes.add_box(b);
            self.just_box = idx;
            self.append_to_vlist(idx);
            if let Some(mut adj) = self.adjust.take() {
                self.cur.append(&mut adj);
            }
            // §890: the penalty between this line and the next.
            if cur_line + 1 != best_line {
                let mut pen = self.ip(INTER_LINE_PENALTY);
                if cur_line == prev_graf + 1 {
                    pen += self.ip(CLUB_PENALTY);
                }
                if cur_line + 2 == best_line {
                    pen += self.ip(if display { DISPLAY_WIDOW_PENALTY } else { WIDOW_PENALTY });
                }
                if disc_break {
                    pen += self.ip(BROKEN_PENALTY);
                }
                if pen != 0 {
                    self.cur.push(Node::new(Kind::Penalty, pen));
                }
            }
            // §879: discardable nodes after a break, up to the next breakpoint, go (not
            // after a post-break text).
            pos = next;
            if !carry.is_empty() {
                continue;
            }
            let stop = breaks.get(j + 1).copied().flatten().map(|q| q as usize);
            while pos < par.len() && Some(pos) != stop {
                let n = par[pos];
                let discardable = matches!(n.kind, Kind::Glue | Kind::Penalty | Kind::Math) || (n.kind == Kind::Kern && n.sub == 1);
                if !discardable {
                    break;
                }
                self.e.nodes.flush_node(n);
                pos += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::engine_names::param;
    use super::*;

    #[test]
    fn slots_match_the_table() {
        for (name, region, slot) in [
            ("pretolerance", Region::Int, PRETOLERANCE),
            ("tolerance", Region::Int, TOLERANCE),
            ("linepenalty", Region::Int, LINE_PENALTY),
            ("clubpenalty", Region::Int, CLUB_PENALTY),
            ("widowpenalty", Region::Int, WIDOW_PENALTY),
            ("displaywidowpenalty", Region::Int, DISPLAY_WIDOW_PENALTY),
            ("interlinepenalty", Region::Int, INTER_LINE_PENALTY),
            ("doublehyphendemerits", Region::Int, DOUBLE_HYPHEN_DEMERITS),
            ("finalhyphendemerits", Region::Int, FINAL_HYPHEN_DEMERITS),
            ("adjdemerits", Region::Int, ADJ_DEMERITS),
            ("looseness", Region::Int, LOOSENESS),
            ("hangafter", Region::Int, HANG_AFTER),
            ("hsize", Region::Dimen, HSIZE),
            ("hangindent", Region::Dimen, HANG_INDENT),
            ("emergencystretch", Region::Dimen, EMERGENCY_STRETCH),
            ("leftskip", Region::Glue, LEFT_SKIP),
            ("rightskip", Region::Glue, RIGHT_SKIP),
            ("parfillskip", Region::Glue, PAR_FILL_SKIP),
        ] {
            assert_eq!(param(name), Some(Reg { region, slot }), "{name}");
        }
    }
}
