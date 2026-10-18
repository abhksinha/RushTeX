//! TeX's line breaker (tex.web §813–§890), in TeX's own integer arithmetic.
//!
//! The paragraph's words, interword glue, inline-math penalties and discretionaries are
//! turned into a compact node list in scaled points, and the Knuth–Plass algorithm runs on
//! it exactly as `line_break` does: the same badness function (§108), fitness classes,
//! demerits (`\linepenalty`, `\adjdemerits`, `\doublehyphendemerits`,
//! `\finalhyphendemerits`), passes (`\pretolerance` without hyphenation, `\tolerance` with
//! it, then `\emergencystretch`), artificial demerits on the final pass and `\looseness`.
//! Equal inputs therefore give TeX's breaks, not an approximation of them.
//!
//! Resource notes: the node list and the active/passive lists live in a [`BreakScratch`]
//! that the layouter reuses for every paragraph. Totals are kept only per active node
//! (TeX keeps deltas between them), so the working set is the active list, not a prefix
//! array over the paragraph. The word list is never modified: a hyphenated word is cut by
//! byte offset in the line descriptors and painted from slices.

use crate::backend::fonts::{Face, FontFamily};
use crate::backend::scaled::{to_pt, to_sp};
use crate::layout::layout_engine::{ParagraphSoA, WordKind};
use crate::layout::layout_hz::{calculate_line_geometry, HzLineDescriptor};

/// `inf_bad`: badness of a line that cannot be stretched enough.
const INF_BAD: i32 = 10000;
/// `inf_penalty`: penalties at or above this are not breakpoints.
const INF_PENALTY: i32 = 10000;
/// `eject_penalty`: a forced break.
const EJECT_PENALTY: i32 = -10000;
const AWFUL_BAD: i64 = 0x3FFF_FFFF;

const VERY_LOOSE: u8 = 0;
const LOOSE: u8 = 1;
const DECENT: u8 = 2;
const TIGHT: u8 = 3;

/// Glue stretch orders: normal, fil, fill, filll.
const ORDERS: usize = 4;

/// `badness(t, s)` (tex.web §108): about 100(t/s)^3, computed as TeX computes it.
#[inline]
pub fn badness(t: i32, s: i32) -> i32 {
    if t == 0 {
        return 0;
    }
    if s <= 0 {
        return INF_BAD;
    }
    let r: i64 = if t <= 7_230_584 {
        (t as i64 * 297) / s as i64
    } else if s >= 1_663_497 {
        t as i64 / (s as i64 / 297)
    } else {
        t as i64
    };
    if r > 1290 {
        INF_BAD
    } else {
        ((r * r * r + 0o400000) / 0o1000000) as i32
    }
}

/// Glue with a stretch order, in points (`order` 0 = finite, 1 = fil, 2 = fill, 3 = filll).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Skip {
    pub width: f32,
    pub stretch: f32,
    pub stretch_order: u8,
}

impl Skip {
    pub const ZERO: Skip = Skip { width: 0.0, stretch: 0.0, stretch_order: 0 };
    /// `0pt plus 1fil`.
    pub const FIL: Skip = Skip { width: 0.0, stretch: 1.0, stretch_order: 1 };
}

/// The paragraph parameters `line_break` reads, with LaTeX's defaults.
#[derive(Clone, Copy, Debug)]
pub struct BreakParams {
    pub pretolerance: i32,
    pub tolerance: i32,
    /// `\emergencystretch` in points.
    pub emergency_stretch: f32,
    pub looseness: i32,
    pub line_penalty: i32,
    pub hyphen_penalty: i32,
    pub ex_hyphen_penalty: i32,
    pub adj_demerits: i32,
    pub double_hyphen_demerits: i32,
    pub final_hyphen_demerits: i32,
    /// Stretch of `\leftskip` and `\rightskip` (their widths are already outside the measure).
    pub left_skip: Skip,
    pub right_skip: Skip,
    pub par_fill_skip: Skip,
    /// Width of the paragraph-indent box (`\parindent`), 0 when not indented.
    pub indent: f32,
    pub family: FontFamily,
}

impl Default for BreakParams {
    fn default() -> Self {
        Self {
            pretolerance: 100,
            tolerance: 200,
            emergency_stretch: 0.0,
            looseness: 0,
            line_penalty: 10,
            hyphen_penalty: 50,
            ex_hyphen_penalty: 50,
            adj_demerits: 10000,
            double_hyphen_demerits: 10000,
            final_hyphen_demerits: 5000,
            left_skip: Skip::ZERO,
            right_skip: Skip::ZERO,
            par_fill_skip: Skip::FIL,
            indent: 0.0,
            family: FontFamily::ComputerModern,
        }
    }
}

/// The line widths of the paragraph shape (`\hsize`, `\hangindent`/`\hangafter`, `\parshape`).
#[derive(Clone, Copy, Debug)]
pub struct LineShape<'s> {
    pub measure: f32,
    pub hang_indent: f32,
    pub hang_after: i32,
    pub par_shape: &'s [(f32, f32)],
}

impl LineShape<'_> {
    /// Indent and width of line `l` (0-based).
    #[inline]
    fn line(&self, l: usize) -> (f32, f32) {
        calculate_line_geometry(l, self.measure, 0.0, self.hang_indent, self.hang_after, self.par_shape)
    }

    /// TeX's `last_special_line` (§848): lines past it all have the same width.
    fn last_special_line(&self) -> usize {
        if !self.par_shape.is_empty() {
            self.par_shape.len().saturating_sub(1)
        } else if self.hang_indent == 0.0 {
            0
        } else {
            self.hang_after.unsigned_abs() as usize
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
enum NodeKind {
    Box,
    Glue,
    Penalty,
    Disc,
}

/// One item of the break list (24 bytes). `w` is the width (glue: natural width; disc:
/// pre-break width), `x`/`y` the glue stretch and shrink or, for a discretionary, the
/// post-break width correction; `item` the word-list index the node came from.
#[derive(Clone, Copy, Debug)]
struct Node {
    kind: NodeKind,
    order: u8,
    /// Byte offset of a hyphenation point inside `item` (0: the break follows the item).
    cut: u8,
    _pad: u8,
    item: u32,
    w: i32,
    x: i32,
    y: i32,
    pen: i32,
}

const _: () = assert!(core::mem::size_of::<Node>() == 24);

impl Node {
    #[inline]
    fn boxed(item: u32, w: i32) -> Self {
        Self { kind: NodeKind::Box, order: 0, cut: 0, _pad: 0, item, w, x: 0, y: 0, pen: 0 }
    }
    #[inline]
    fn glue(item: u32, w: i32, stretch: i32, order: u8, shrink: i32) -> Self {
        Self { kind: NodeKind::Glue, order, cut: 0, _pad: 0, item, w, x: stretch, y: shrink, pen: 0 }
    }
    #[inline]
    fn penalty(item: u32, pen: i32) -> Self {
        Self { kind: NodeKind::Penalty, order: 0, cut: 0, _pad: 0, item, w: 0, x: 0, y: 0, pen }
    }
    #[inline]
    fn disc(item: u32, cut: u8, pre: i32, post: i32, pen: i32) -> Self {
        Self { kind: NodeKind::Disc, order: 0, cut, _pad: 0, item, w: pre, x: post, y: 0, pen }
    }
    #[inline]
    fn discardable(&self) -> bool {
        matches!(self.kind, NodeKind::Glue | NodeKind::Penalty)
    }
}

/// Width, stretch per order and shrink of a run of nodes, in scaled points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Totals {
    w: i64,
    st: [i64; ORDERS],
    sh: i64,
}

impl Totals {
    #[inline]
    fn add(&mut self, n: &Node) {
        match n.kind {
            NodeKind::Box => self.w += n.w as i64,
            NodeKind::Glue => {
                self.w += n.w as i64;
                self.st[n.order as usize] += n.x as i64;
                self.sh += n.y as i64;
            }
            NodeKind::Penalty | NodeKind::Disc => {}
        }
    }
    #[inline]
    fn sub(&self, o: &Totals) -> Totals {
        let mut t = *self;
        t.w -= o.w;
        for k in 0..ORDERS {
            t.st[k] -= o.st[k];
        }
        t.sh -= o.sh;
        t
    }
}

#[derive(Clone, Copy, Debug)]
struct Active {
    /// Totals of the material before the line that starts at this break.
    start: Totals,
    /// Passive node of the break (`u32::MAX` for the paragraph start).
    passive: u32,
    /// Number of the line that starts here (1-based, as TeX's `line_number`).
    line: u32,
    fitness: u8,
    hyphenated: bool,
    demerits: i64,
}

#[derive(Clone, Copy, Debug)]
struct Passive {
    node: u32,
    prev: u32,
}

/// Hyphenation points of one word (32 bytes). They depend only on the letters, so one
/// entry serves the word in every face and size.
#[derive(Clone, Copy, Default)]
struct CutEntry {
    hash: u64,
    len: u16,
    count: u8,
    cuts: [u8; 21],
}

const _: () = assert!(core::mem::size_of::<CutEntry>() == 32);
const CUT_CACHE_SIZE: usize = 2048;

/// Reusable buffers of the line breaker.
#[derive(Default)]
pub struct BreakScratch {
    nodes: Vec<Node>,
    active: Vec<Active>,
    passive: Vec<Passive>,
    cuts: Vec<usize>,
    breaks: Vec<u32>,
    /// Direct-mapped cache of pattern hyphenation results (64 KB, allocated on first use).
    cut_cache: Vec<CutEntry>,
}

/// Byte offsets of the hyphenation points of `text[s..e]` (offsets into `text`), from the
/// cache or the patterns.
fn hyphenation_cuts(cache: &mut Vec<CutEntry>, text: &str, s: usize, e: usize, out: &mut Vec<usize>) {
    use crate::layout::layout_hyphen::{hash_word, hyphenate_word, DEFAULT_LEFT_HYPHEN_MIN, DEFAULT_RIGHT_HYPHEN_MIN};
    out.clear();
    let core = &text[s..e];
    let hash = hash_word(core) | 1;
    if cache.is_empty() {
        cache.resize(CUT_CACHE_SIZE, CutEntry::default());
    }
    let slot = &mut cache[(hash as usize) & (CUT_CACHE_SIZE - 1)];
    if slot.hash != hash || slot.len as usize != core.len() {
        // A miss allocates the piece list once; hits, the common case, allocate nothing.
        let mut pieces = Vec::new();
        hyphenate_word(core, DEFAULT_LEFT_HYPHEN_MIN, DEFAULT_RIGHT_HYPHEN_MIN, &mut pieces);
        let mut entry = CutEntry { hash, len: core.len() as u16, count: 0, cuts: [0; 21] };
        let mut off = 0usize;
        for p in pieces.iter().take(pieces.len().saturating_sub(1)).take(entry.cuts.len()) {
            off += p.len();
            entry.cuts[entry.count as usize] = off as u8;
            entry.count += 1;
        }
        *slot = entry;
    }
    out.extend(slot.cuts[..slot.count as usize].iter().map(|&c| s + c as usize));
}

/// Breaks `soa[range]` into lines as TeX does and appends one descriptor per line.
/// Returns the pass that found the breaks (1 `\pretolerance`, 2 `\tolerance`, 3
/// `\emergencystretch`), 0 for an empty paragraph.
pub fn tex_line_break(
    soa: &ParagraphSoA<'_>,
    range: std::ops::Range<usize>,
    params: &BreakParams,
    shape: &LineShape<'_>,
    base_size: f32,
    scratch: &mut BreakScratch,
    out: &mut Vec<HzLineDescriptor>,
) -> u8 {
    // TeX discards the paragraph's final glue (§816).
    let mut end = range.end;
    while end > range.start && soa.attrs[end - 1].is_space() {
        end -= 1;
    }
    let mut start = range.start;
    while start < end && soa.attrs[start].is_space() {
        start += 1;
    }
    if end == start {
        return 0;
    }
    let range = start..end;

    let mut pass = 0;
    if params.pretolerance >= 0 {
        build_nodes(soa, range.clone(), params, base_size, false, scratch);
        if run_pass(scratch, params, shape, params.pretolerance, false, false) {
            pass = 1;
        }
    }
    if pass == 0 {
        build_nodes(soa, range.clone(), params, base_size, true, scratch);
        let final_pass = params.emergency_stretch <= 0.0;
        pass = if run_pass(scratch, params, shape, params.tolerance, final_pass, false) {
            2
        } else {
            let found = run_pass(scratch, params, shape, params.tolerance, true, true);
            debug_assert!(found);
            3
        };
    }
    emit_lines(range, params, shape, scratch, out);
    pass
}

/// Builds the node list (§866 and the hyphenation of §891–§903 when `hyphenate`).
fn build_nodes(
    soa: &ParagraphSoA<'_>,
    range: std::ops::Range<usize>,
    params: &BreakParams,
    base_size: f32,
    hyphenate: bool,
    scratch: &mut BreakScratch,
) {
    let nodes = &mut scratch.nodes;
    nodes.clear();
    nodes.reserve(range.len() + 8);
    if params.indent != 0.0 {
        nodes.push(Node::boxed(u32::MAX, to_sp(params.indent)));
    }
    let family = params.family;
    let mut size_cursor = (base_size, if soa.size_runs.is_empty() { usize::MAX } else { 0 });
    for i in range.clone() {
        let attr = soa.attrs[i];
        let w = to_sp(soa.widths[i]);
        let item = i as u32;
        match attr.kind() {
            WordKind::Space => {
                let [st, sh] = soa.glue[i];
                nodes.push(Node::glue(item, w, to_sp(st), 0, to_sp(sh)));
            }
            // Glue inside a formula is not a breakpoint (§866: `auto_breaking` is off).
            WordKind::MathSpace => {
                nodes.push(Node::penalty(item, INF_PENALTY));
                let [st, sh] = soa.glue[i];
                nodes.push(Node::glue(item, w, to_sp(st), 0, to_sp(sh)));
            }
            WordKind::Break => {
                // `\\`: \unskip\nobreak\hfil\break.
                while nodes.last().is_some_and(|n| n.kind == NodeKind::Glue) {
                    nodes.pop();
                }
                nodes.push(Node::penalty(item, INF_PENALTY));
                nodes.push(Node::glue(item, 0, to_sp(1.0), 1, 0));
                nodes.push(Node::penalty(item, EJECT_PENALTY));
            }
            WordKind::Math | WordKind::MathBin | WordKind::MathRel => {
                nodes.push(Node::boxed(item, w));
                if let Some(p) = attr.math_penalty() {
                    nodes.push(Node::penalty(item, p));
                }
            }
            WordKind::Hyphen => {
                nodes.push(Node::boxed(item, w));
                let face = attr.face();
                // Typewriter fonts have \hyphenchar -1 in LaTeX: no discretionaries.
                if !face.is_mono() {
                    if i >= size_cursor.1 {
                        size_cursor = soa.size_span(i, base_size);
                    }
                    let text = soa.texts[i];
                    if text.ends_with('-') {
                        nodes.push(Node::disc(item, 0, 0, 0, params.ex_hyphen_penalty));
                    } else {
                        let pre = hyphen_width(face, size_cursor.0, family);
                        nodes.push(Node::disc(item, 0, pre, 0, params.hyphen_penalty));
                    }
                }
            }
            WordKind::Word => {
                let face = attr.face();
                let eligible = hyphenate
                    && !face.is_mono()
                    && i > range.start
                    && soa.attrs[i - 1].kind() == WordKind::Space;
                if eligible {
                    if i >= size_cursor.1 {
                        size_cursor = soa.size_span(i, base_size);
                    }
                    if push_hyphenated(nodes, &mut scratch.cuts, &mut scratch.cut_cache, soa.texts[i], item, w, face, size_cursor.0, family, params.hyphen_penalty) {
                        continue;
                    }
                }
                nodes.push(Node::boxed(item, w));
            }
        }
    }
    // \penalty10000 \parfillskip \penalty-10000 (§816).
    let last = range.end as u32;
    let pf = params.par_fill_skip;
    nodes.push(Node::penalty(last, INF_PENALTY));
    nodes.push(Node::glue(last, to_sp(pf.width), skip_stretch_sp(pf), pf.stretch_order, 0));
    nodes.push(Node::penalty(last, EJECT_PENALTY));
}

#[inline]
fn skip_stretch_sp(s: Skip) -> i32 {
    if s.stretch_order > 0 {
        // Infinite orders are counted in units of 1fil = 65536sp.
        (s.stretch * 65536.0).round() as i32
    } else {
        to_sp(s.stretch)
    }
}

#[inline]
fn hyphen_width(face: Face, size: f32, family: FontFamily) -> i32 {
    to_sp(crate::backend::shape::word_width_pt("-", face, size, family))
}

/// The letters TeX would hyphenate in `text` (§896–§899): leading non-letters are skipped,
/// the word is the following run of letters, and it must end the item or be followed only
/// by non-letters. Returns the byte range of the letters.
fn hyphenatable_core(text: &str) -> Option<(usize, usize)> {
    let b = text.as_bytes();
    let mut s = 0;
    while s < b.len() && !b[s].is_ascii_alphabetic() {
        if b[s] >= 0x80 {
            return None;
        }
        s += 1;
    }
    let mut e = s;
    while e < b.len() && b[e].is_ascii_alphabetic() {
        e += 1;
    }
    if e - s < crate::layout::layout_hyphen::DEFAULT_LEFT_HYPHEN_MIN + crate::layout::layout_hyphen::DEFAULT_RIGHT_HYPHEN_MIN {
        return None;
    }
    // A letter after the core (`e.g.`-style words) or a non-ASCII character in it would be
    // part of the same TeX word; such words are left alone.
    if b[e..].iter().any(|&c| c.is_ascii_alphabetic() || c >= 0x80) || e > 250 {
        return None;
    }
    Some((s, e))
}

/// Pushes `text` as boxes separated by its hyphenation discretionaries; false when the
/// word has no hyphenation point (nothing pushed).
#[allow(clippy::too_many_arguments)]
fn push_hyphenated(
    nodes: &mut Vec<Node>,
    cuts: &mut Vec<usize>,
    cache: &mut Vec<CutEntry>,
    text: &str,
    item: u32,
    whole: i32,
    face: Face,
    size: f32,
    family: FontFamily,
    pen: i32,
) -> bool {
    let Some((s, e)) = hyphenatable_core(text) else { return false };
    hyphenation_cuts(cache, text, s, e, cuts);
    if cuts.is_empty() {
        return false;
    }
    let width = |t: &str| to_sp(crate::backend::shape::word_width_pt(t, face, size, family));
    let pre = hyphen_width(face, size, family);
    let extra = whole - width(text);
    let mut prev = 0i32;
    for &c in cuts.iter() {
        // The prefix ends the line when broken; the remainder is shaped afresh there, so the
        // post-break width corrects the in-line pieces (ligatures and kerns at the cut).
        // Material after the letters (an italic correction) stays with the post-break part.
        let prefix = width(&text[..c]);
        let post = width(&text[c..]) + extra - (whole - prefix);
        nodes.push(Node::boxed(item, prefix - prev));
        nodes.push(Node::disc(item, c as u8, pre, post, pen));
        prev = prefix;
    }
    nodes.push(Node::boxed(item, whole - prev));
    true
}

/// One pass of `line_break` (§863–§873); true when a feasible set of breaks was found.
fn run_pass(
    scratch: &mut BreakScratch,
    params: &BreakParams,
    shape: &LineShape<'_>,
    threshold: i32,
    final_pass: bool,
    emergency: bool,
) -> bool {
    let BreakScratch { nodes, active, passive, breaks, .. } = scratch;
    active.clear();
    passive.clear();

    // Background: the stretch of \leftskip and \rightskip (§827), plus \emergencystretch.
    let mut background = Totals::default();
    for s in [params.left_skip, params.right_skip] {
        background.st[s.stretch_order as usize] += skip_stretch_sp(s) as i64;
    }
    if emergency {
        background.st[0] += to_sp(params.emergency_stretch) as i64;
    }

    let easy_line: u32 = if params.looseness == 0 { shape.last_special_line() as u32 } else { u32::MAX };
    active.push(Active {
        start: Totals::default(),
        passive: u32::MAX,
        line: 1,
        fitness: DECENT,
        hyphenated: false,
        demerits: 0,
    });

    let mut cur = Totals::default();
    let mut prev_is_box = false;
    let n = nodes.len();
    for p in 0..n {
        let node = nodes[p];
        match node.kind {
            NodeKind::Box => {
                cur.add(&node);
                prev_is_box = true;
            }
            NodeKind::Glue => {
                if prev_is_box {
                    try_break(nodes, active, passive, params, shape, &background, &cur, p, 0, false, threshold, final_pass, easy_line);
                }
                cur.add(&node);
                prev_is_box = false;
            }
            NodeKind::Penalty => {
                if node.pen < INF_PENALTY {
                    try_break(nodes, active, passive, params, shape, &background, &cur, p, node.pen, false, threshold, final_pass, easy_line);
                }
                prev_is_box = false;
            }
            NodeKind::Disc => {
                let mut with_pre = cur;
                with_pre.w += node.w as i64;
                try_break(nodes, active, passive, params, shape, &background, &with_pre, p, node.pen, true, threshold, final_pass, easy_line);
                prev_is_box = true;
            }
        }
        if active.is_empty() {
            return false;
        }
    }
    // The final \penalty-10000 leaves only the nodes of the paragraph's last break (§873).
    let Some((best, reached)) = choose_best(active, params) else { return false };
    if !reached && !final_pass {
        return false;
    }
    breaks.clear();
    let mut q = best;
    while q != u32::MAX {
        breaks.push(passive[q as usize].node);
        q = passive[q as usize].prev;
    }
    breaks.reverse();
    true
}

/// The active node with the fewest demerits (§874), moved toward `\looseness` lines more
/// or fewer when that is set (§875). Returns its passive break and whether the looseness
/// target was met.
fn choose_best(active: &[Active], params: &BreakParams) -> Option<(u32, bool)> {
    let mut best = *active.first()?;
    for a in &active[1..] {
        if a.demerits < best.demerits {
            best = *a;
        }
    }
    if params.looseness == 0 {
        return Some((best.passive, true));
    }
    let best_line = best.line as i64;
    let looseness = params.looseness as i64;
    let mut actual = 0i64;
    let mut fewest = best.demerits;
    for a in active {
        let diff = a.line as i64 - best_line;
        if (diff < actual && looseness <= diff) || (diff > actual && looseness >= diff) {
            best = *a;
            actual = diff;
            fewest = a.demerits;
        } else if diff == actual && a.demerits < fewest {
            best = *a;
            fewest = a.demerits;
        }
    }
    Some((best.passive, actual == looseness))
}

/// `try_break` (§829–§860) for a break at node `p` with penalty `pi`. `cur` holds the totals
/// of the material before the break, including a discretionary's pre-break text.
#[allow(clippy::too_many_arguments)]
fn try_break(
    nodes: &[Node],
    active: &mut Vec<Active>,
    passive: &mut Vec<Passive>,
    params: &BreakParams,
    shape: &LineShape<'_>,
    background: &Totals,
    cur: &Totals,
    p: usize,
    pi: i32,
    hyphenated: bool,
    threshold: i32,
    final_pass: bool,
    easy_line: u32,
) {
    let pi = pi.max(EJECT_PENALTY);
    let mut minimal = [AWFUL_BAD; 4];
    let mut best_place = [u32::MAX; 4];
    let mut best_line = [0u32; 4];
    let mut minimum = AWFUL_BAD;
    let mut old_l: u32 = 0;
    // Width of the lines numbered `l` (§850), recomputed only when `l` changes.
    let mut line_w: i64 = 0;
    let mut no_break_yet = true;
    let mut break_start = Totals::default();

    let mut r = 0usize;
    loop {
        let at_end = r >= active.len();
        let l = if at_end { u32::MAX } else { active[r].line };
        // Create new active nodes for the best feasible breaks just found (§835–§836).
        if at_end || l > old_l {
            if minimum < AWFUL_BAD && (old_l != easy_line || at_end) {
                if no_break_yet {
                    no_break_yet = false;
                    break_start = start_after_break(nodes, cur, p, hyphenated);
                }
                let limit = minimum.saturating_add(params.adj_demerits as i64);
                for fit in (VERY_LOOSE..=TIGHT).rev() {
                    let f = fit as usize;
                    if minimal[f] <= limit {
                        passive.push(Passive { node: p as u32, prev: best_place[f] });
                        let node = Active {
                            start: break_start,
                            passive: (passive.len() - 1) as u32,
                            line: best_line[f] + 1,
                            fitness: fit,
                            hyphenated,
                            demerits: minimal[f],
                        };
                        active.insert(r, node);
                        r += 1;
                    }
                    minimal[f] = AWFUL_BAD;
                }
                minimum = AWFUL_BAD;
            }
            if at_end {
                return;
            }
            // Line width for lines starting at `r` (§850).
            old_l = if l > easy_line { u32::MAX - 1 } else { l };
            line_w = to_sp(shape.line((l.min(easy_line.saturating_add(1)) - 1) as usize).1) as i64;
        }
        let a = active[r];
        let line = cur.sub(&a.start);
        let shortfall = line_w - line.w - background.w;
        let stretch = line.st[0] + background.st[0];
        let fil = line.st[1] + line.st[2] + line.st[3] + background.st[1] + background.st[2] + background.st[3];
        let shrink = line.sh + background.sh;

        let (b, fit) = if shortfall > 0 {
            if fil != 0 {
                (0, DECENT)
            } else if shortfall > 7_230_584 && stretch < 1_663_497 {
                (INF_BAD, VERY_LOOSE)
            } else {
                let b = badness(shortfall.min(i32::MAX as i64) as i32, stretch.min(i32::MAX as i64) as i32);
                let fit = if b > 12 {
                    if b > 99 { VERY_LOOSE } else { LOOSE }
                } else {
                    DECENT
                };
                (b, fit)
            }
        } else if -shortfall > shrink {
            (INF_BAD + 1, DECENT)
        } else {
            let b = badness((-shortfall).min(i32::MAX as i64) as i32, shrink.min(i32::MAX as i64) as i32);
            (b, if b > 12 { TIGHT } else { DECENT })
        };

        let mut artificial = false;
        let stays_active = if b > INF_BAD || pi == EJECT_PENALTY {
            // §854: deactivate `r` unless this is the final pass's last resort.
            if final_pass && minimum == AWFUL_BAD && r + 1 == active.len() && r == 0 {
                artificial = true;
            } else if b > threshold {
                active.remove(r);
                continue;
            }
            false
        } else {
            if b > threshold {
                r += 1;
                continue;
            }
            true
        };

        // Record a new feasible break (§855, §859).
        let d = if artificial {
            0
        } else {
            let mut d = (params.line_penalty + b) as i64;
            d = if d.abs() >= 10000 { 100_000_000 } else { d * d };
            if pi != 0 {
                if pi > 0 {
                    d += pi as i64 * pi as i64;
                } else if pi > EJECT_PENALTY {
                    d -= pi as i64 * pi as i64;
                }
            }
            // The paragraph's final break counts as hyphenated (§873), so a hyphen at the end
            // of the next-to-last line costs \finalhyphendemerits.
            let is_final = p + 1 == nodes.len();
            if (hyphenated || is_final) && a.hyphenated {
                d += if is_final {
                    params.final_hyphen_demerits as i64
                } else {
                    params.double_hyphen_demerits as i64
                };
            }
            if (fit as i32 - a.fitness as i32).abs() > 1 {
                d += params.adj_demerits as i64;
            }
            d
        };
        let d = d + a.demerits;
        let f = fit as usize;
        if d <= minimal[f] {
            minimal[f] = d;
            best_place[f] = a.passive;
            best_line[f] = a.line;
            if d < minimum {
                minimum = d;
            }
        }
        if stays_active {
            r += 1;
        } else {
            active.remove(r);
        }
    }
}

/// Totals at the start of the line after a break at node `p` (§837–§840): the discardable
/// glue and penalties after the break are dropped, a discretionary adds its post-break text.
fn start_after_break(nodes: &[Node], cur: &Totals, p: usize, hyphenated: bool) -> Totals {
    let mut t = *cur;
    let mut s = p;
    if hyphenated {
        let n = nodes[p];
        // `cur` included the pre-break text; the next line starts after the discretionary
        // with its post-break text instead.
        t.w -= n.w as i64;
        t.w -= n.x as i64;
        s = p + 1;
    }
    while s < nodes.len() && nodes[s].discardable() {
        t.add(&nodes[s]);
        s += 1;
    }
    t
}

/// `post_line_break` (§877–§890): turns the chosen breaks into line descriptors. A line
/// runs from the first box after the previous break (discardables dropped) to the box
/// before its own break; a hyphenated break cuts its word, whose remainder opens the next
/// line.
fn emit_lines(
    range: std::ops::Range<usize>,
    params: &BreakParams,
    shape: &LineShape<'_>,
    scratch: &BreakScratch,
    out: &mut Vec<HzLineDescriptor>,
) {
    let nodes = &scratch.nodes;
    let n_lines = scratch.breaks.len();
    let mut s = 0usize;
    // The discretionary that ended the previous line, when it cut a word.
    let mut carried: Option<Node> = None;
    for (line_idx, &b) in scratch.breaks.iter().enumerate() {
        let b = b as usize;
        let brk = nodes[b];
        if line_idx > 0 {
            while s < b && nodes[s].discardable() {
                s += 1;
            }
        }
        let mut w: i64 = 0;
        let mut first: Option<usize> = None;
        let mut last: Option<usize> = None;
        let mut indent = 0.0;
        let mut head_cut = 0u8;
        if let Some(d) = carried.take() {
            w += d.x as i64;
            first = Some(d.item as usize);
            head_cut = d.cut;
        }
        for nd in &nodes[s..b] {
            match nd.kind {
                NodeKind::Box if nd.item == u32::MAX => indent = params.indent,
                NodeKind::Box => {
                    w += nd.w as i64;
                    let it = nd.item as usize;
                    first.get_or_insert(it);
                    last = Some(it);
                }
                NodeKind::Glue => w += nd.w as i64,
                _ => {}
            }
        }
        let mut tail_cut = 0u8;
        if brk.kind == NodeKind::Disc {
            w += brk.w as i64;
            last = Some(brk.item as usize);
            first.get_or_insert(brk.item as usize);
            if brk.cut != 0 {
                tail_cut = brk.cut;
                carried = Some(brk);
            }
        }
        let is_last = line_idx + 1 == n_lines;
        // A line ended by `\\` closes with \hfil and keeps its natural spacing; so does the
        // last line unless \parfillskip has no stretch.
        let ends_with_hfil = brk.kind == NodeKind::Penalty
            && brk.pen == EJECT_PENALTY
            && b > 0
            && nodes[b - 1].kind == NodeKind::Glue
            && nodes[b - 1].order > 0;
        let justify = if is_last { params.par_fill_skip.stretch == 0.0 } else { !ends_with_hfil };
        let (geo_indent, _) = shape.line(line_idx);
        let (start, count) = match (first, last) {
            (Some(f), Some(l)) if f < range.end => (f, l.min(range.end - 1) + 1 - f),
            _ => (range.start, 0),
        };
        let mut d = HzLineDescriptor::new(start as u16, count as u16, justify, 0, to_pt(w), geo_indent + indent, 0.0);
        d.set_cuts(head_cut, if count > 0 && start + count - 1 == brk.item as usize { tail_cut } else { 0 });
        out.push(d);
        s = b + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badness_matches_tex() {
        assert_eq!(badness(0, 100), 0);
        assert_eq!(badness(100, 0), INF_BAD);
        // t = s: 297^3 / 2^18 rounds to 100.
        assert_eq!(badness(65536, 65536), 100);
        // r = 148: (148^3 + 2^17) / 2^18 = 12.
        assert_eq!(badness(32768, 65536), 12);
        assert_eq!(badness(2 * 65536, 65536), 800);
        assert_eq!(badness(200 * 65536, 65536), INF_BAD);
    }
}
