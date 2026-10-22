//! Typesetting math formulas: tex.web §680–§767 (Appendix G of The TeXbook), in our own
//! form.
//!
//! A math list is a list of ordinary nodes and [`Kind::Noad`] nodes; a noad's fields live
//! in the machine's noad table, and a subformula is a finished list in the node arena. The
//! first pass of `mlist_to_hlist` turns each noad into its own horizontal list, kept back
//! to back in one local buffer; the second pass joins them with TeX's spacing and
//! penalties. Boxes made here are fresh records, so they are adjusted in place.

use super::engine_eqtb::{Glue, Reg, Region};
use super::engine_font::{restart, STOP_FLAG};
use super::engine_node::{BoxRec, Kind, Node, RUNNING};
use super::engine_prim::Meaning;
use super::engine_run::Machine;
use crate::backend::scaled::Scaler;

/// §682–§688 noad kinds, in TeX's order (`ord_noad` .. `right_noad`), then §688's style
/// and choice nodes.
pub(super) const ORD: u8 = 0;
pub(super) const OP: u8 = 1;
pub(super) const BIN: u8 = 2;
pub(super) const REL: u8 = 3;
pub(super) const OPEN: u8 = 4;
pub(super) const CLOSE: u8 = 5;
pub(super) const PUNCT: u8 = 6;
pub(super) const INNER: u8 = 7;
pub(super) const RADICAL: u8 = 8;
pub(super) const FRACTION: u8 = 9;
pub(super) const UNDER: u8 = 10;
pub(super) const OVER: u8 = 11;
pub(super) const ACCENT: u8 = 12;
pub(super) const VCENTER: u8 = 13;
pub(super) const LEFT: u8 = 14;
pub(super) const RIGHT: u8 = 15;
pub(super) const STYLE: u8 = 16;
pub(super) const CHOICE: u8 = 17;

/// §682 `subtype` of an op noad, and e-TeX's `middle_noad` subtype of a right noad.
pub(super) const LIMITS: u8 = 1;
#[cfg(test)]
pub(super) const NO_LIMITS: u8 = 2;
pub(super) const MIDDLE: u8 = 1;
/// §683 `default_code`: the thickness of `\over`.
pub(super) const DEFAULT_CODE: i32 = 0o10000000000;
/// §149 glue subtypes: `\nonscript` and `\mskip`; §155: `\mkern`.
pub(super) const COND_MATH_GLUE: u8 = 98;
pub(super) const MU_GLUE: u8 = 99;

/// §688 styles.
pub(super) const DISPLAY: u8 = 0;
pub(super) const TEXT: u8 = 2;
pub(super) const SCRIPT: u8 = 4;
/// §699 sizes: the offsets of `\scriptfont` and `\scriptscriptfont` from `\textfont`.
pub(super) const TEXT_SIZE: u32 = 0;
const SCRIPT_SIZE: u32 = 16;
const SCRIPT_SCRIPT_SIZE: u32 = 32;

const INF_PENALTY: i32 = 10_000;
const MAX_DIMEN: i32 = 0o7777777777;
const LIST_TAG: u32 = 2;
const EXT_TAG: u32 = 3;
const LIG_TAG: u32 = 1;

// §236, §247, §224 parameter slots.
const BIN_OP_PENALTY: u32 = 9;
const REL_PENALTY: u32 = 10;
const DELIMITER_FACTOR: u32 = 18;
const DELIMITER_SHORTFALL: u32 = 10;
const NULL_DELIMITER_SPACE: u32 = 11;
const SCRIPT_SPACE: u32 = 12;
const THIN_MU_SKIP: u32 = 15;
const MED_MU_SKIP: u32 = 16;
const THICK_MU_SKIP: u32 = 17;

/// §764: the spacing between noad types, row the left type, column the right one
/// (`ord` .. `inner`): 0 none, 1 thin unless in a script, 2 thin, 3 medium unless in a
/// script, 4 thick unless in a script.
const MATH_SPACING: &[u8; 64] = b"0234000122*4000133**3**344*0400400*000000234000111*1111112341011";

/// §681: what a noad field holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Field {
    #[default]
    Empty,
    Char { fam: u8, c: u8 },
    /// §752 `math_text_char`: a character in a word of a text font (no italic correction).
    TextChar { fam: u8, c: u8 },
    Box(u32),
    Mlist(u32, u32),
}

/// §681–§688: one noad. A fraction keeps its numerator in `supscr` and denominator in
/// `subscr` (as TeX does); a choice keeps its four lists in `nucleus`, `supscr`, `subscr`
/// and `extra`; an accent keeps its accent character in `extra`.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Noad {
    pub kind: u8,
    pub sub: u8,
    pub nucleus: Field,
    pub supscr: Field,
    pub subscr: Field,
    pub extra: Field,
    /// §683 delimiter fields as TeX's 24-bit code (small family, small character, large
    /// family, large character): a radical's or `\left`'s in `[0]`, a fraction's left and
    /// right.
    pub delims: [u32; 2],
    pub thickness: i32,
    /// `new_hlist`: a span of the converting call's buffer.
    pub hlist: (u32, u32),
}

impl Noad {
    pub(super) fn new(kind: u8) -> Noad {
        Noad { kind, ..Noad::default() }
    }
}

/// §703 `cur_style`, `cur_size` and `cur_mu`.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct MathState {
    pub style: u8,
    pub size: u32,
    pub mu: i32,
}

/// §702 style changes.
pub(super) fn cramped(s: u8) -> u8 {
    2 * (s / 2) + 1
}
pub(super) fn sub_style(s: u8) -> u8 {
    2 * (s / 4) + SCRIPT + 1
}
pub(super) fn sup_style(s: u8) -> u8 {
    2 * (s / 4) + SCRIPT + s % 2
}
fn num_style(s: u8) -> u8 {
    s + 2 - 2 * (s / 6)
}
fn denom_style(s: u8) -> u8 {
    2 * (s / 2) + 1 + 2 - 2 * (s / 6)
}

/// §100 `half`.
pub(super) fn half(x: i32) -> i32 {
    if x & 1 != 0 {
        (x + 1) / 2
    } else {
        x / 2
    }
}

/// §716 `mu_mult`: `x` times `n + f/65536`, with §105's overflow check.
fn mu_mult(x: i32, n: i32, f: i32) -> i32 {
    let v = n as i64 * x as i64 + x as i64 * f as i64 / 65536;
    if v.abs() > MAX_DIMEN as i64 {
        0
    } else {
        v as i32
    }
}

/// §716: `cur_mu` split into its integer and fractional parts.
fn mu_parts(m: i32) -> (i32, i32) {
    let (mut n, mut f) = (m / 65536, m % 65536);
    if f < 0 {
        n -= 1;
        f += 65536;
    }
    (n, f)
}

/// §716 `math_glue`.
fn math_glue(g: Glue, m: i32) -> Glue {
    let (n, f) = mu_parts(m);
    Glue {
        width: mu_mult(g.width, n, f),
        stretch: if g.stretch_order == 0 { mu_mult(g.stretch, n, f) } else { g.stretch },
        shrink: if g.shrink_order == 0 { mu_mult(g.shrink, n, f) } else { g.shrink },
        ..g
    }
}

/// §1151 the fields of a noad, by index: nucleus, superscript, subscript.
pub(super) fn field_mut(q: &mut Noad, k: u32) -> &mut Field {
    match k {
        0 => &mut q.nucleus,
        1 => &mut q.supscr,
        2 => &mut q.subscr,
        _ => &mut q.extra,
    }
}

impl Machine<'_, '_> {
    /// §703: `cur_size` and `cur_mu` for `style`.
    pub(super) fn set_style(&mut self, style: u8) {
        let size = if style < SCRIPT { TEXT_SIZE } else { 16 * ((style as u32 - TEXT as u32) / 2) };
        self.math = MathState { style, size, mu: 0 };
        self.math.mu = self.mathsy(6, size) / 18;
    }

    /// §230 `fam_fnt`: the font of family `fam` in `size`, `\nullfont` when unset.
    pub(super) fn fam_fnt(&self, fam: u8, size: u32) -> u16 {
        match self.e.eqtb.meaning(super::FAM_BASE_ID + size + fam as u32) {
            Meaning::Font(f) => f,
            _ => 0,
        }
    }

    fn param_of(&self, f: u16, k: usize) -> i32 {
        self.e.fonts.list[f as usize].params.get(k - 1).copied().unwrap_or(0)
    }

    /// §700 `mathsy`: parameter `k` of family 2 in `size`.
    pub(super) fn mathsy(&self, k: usize, size: u32) -> i32 {
        self.param_of(self.fam_fnt(2, size), k)
    }

    /// §701 `mathex`: parameter `k` of family 3 in the current size.
    fn mathex(&self, k: usize) -> i32 {
        self.param_of(self.fam_fnt(3, self.math.size), k)
    }

    fn axis_height(&self, size: u32) -> i32 {
        self.mathsy(22, size)
    }

    fn rule_thickness(&self) -> i32 {
        self.mathex(8)
    }

    /// Width, height, depth and italic correction of `c` in `f`; zero when it is absent.
    fn char_dims(&self, f: u16, c: u8) -> [i32; 4] {
        let font = &self.e.fonts.list[f as usize];
        font.dims(c).unwrap_or([0; 4])
    }

    fn char_exists(&self, f: u16, c: u8) -> bool {
        self.e.fonts.list[f as usize].metrics.exists(c)
    }

    fn char_tag(&self, f: u16, c: u8) -> (u32, u8) {
        self.e.fonts.list[f as usize].metrics.tag(c)
    }

    #[cfg_attr(feature = "ownership-check", track_caller)]
    fn new_box(&mut self, b: BoxRec) -> u32 {
        self.e.nodes.add_box(b)
    }

    fn bx(&self, b: u32) -> BoxRec {
        self.e.nodes.boxes[b as usize]
    }

    fn bx_mut(&mut self, b: u32) -> &mut BoxRec {
        &mut self.e.nodes.boxes[b as usize]
    }

    /// `hpack(list, natural)` of nodes not in the build buffer.
    #[cfg_attr(feature = "ownership-check", track_caller)]
    pub(super) fn hpack_nodes(&mut self, list: &[Node], w: i32, exact: bool) -> u32 {
        let start = self.cur.len();
        self.cur.extend_from_slice(list);
        let b = self.hpack(start, w, exact);
        self.new_box(b)
    }

    /// `vpack(list, natural)`: `vpackage` with no depth limit.
    #[cfg_attr(feature = "ownership-check", track_caller)]
    fn vpack_nodes(&mut self, list: &[Node]) -> u32 {
        let start = self.cur.len();
        self.cur.extend_from_slice(list);
        let b = self.vpack(start, 0, false, MAX_DIMEN);
        self.new_box(b)
    }

    /// A vertical box of the given size around `list`, as TeX builds one by hand.
    fn vlist_box(&mut self, list: &[Node], width: i32, height: i32, depth: i32) -> u32 {
        let list = self.e.nodes.store(list);
        self.new_box(BoxRec { vertical: true, width, height, depth, list, ..BoxRec::default() })
    }

    /// §704 `fraction_rule`.
    fn fraction_rule(&mut self, t: i32) -> Node {
        self.e.nodes.rule_node([RUNNING, t, 0])
    }

    /// §705 `overbar`: box `b` with a rule `t` thick, `k` above it, and `t` above that.
    fn overbar(&mut self, b: u32, k: i32, t: i32) -> u32 {
        let r = self.fraction_rule(t);
        self.vpack_nodes(&[kern(t), r, kern(k), boxed(b)])
    }

    /// §709 `char_box`: a box holding `c` alone, its width including the italic correction.
    fn char_box(&mut self, f: u16, c: u8) -> u32 {
        let [w, h, d, ic] = self.char_dims(f, c);
        let list = self.e.nodes.store(&[Node { kind: Kind::Char, sub: 0, font: f, a: c as i32, b: 0 }]);
        self.new_box(BoxRec { width: w + ic, height: h, depth: d, list, ..BoxRec::default() })
    }

    fn height_plus_depth(&self, f: u16, c: u8) -> i32 {
        let [_, h, d, _] = self.char_dims(f, c);
        h + d
    }

    /// §706 `var_delimiter`: delimiter `d` at least `v` tall in `size`, the small variant's
    /// successors tried first, then the large one's (§707, §708); an extensible character
    /// is built when one is reached (§713); centred on the axis.
    fn var_delimiter(&mut self, d: u32, size: u32, v: i32) -> u32 {
        let mut best: Option<(u16, u8)> = None;
        let mut w = 0;
        let (mut z, mut x) = ((d >> 20 & 15) as u8, (d >> 12 & 0xFF) as u8);
        let mut large_attempt = false;
        'found: loop {
            if z != 0 || x != 0 {
                let mut zz = z as u32 + size + 16;
                loop {
                    zz -= 16;
                    let g = self.fam_fnt(z, zz - z as u32);
                    if g != 0 {
                        let mut y = x;
                        while self.char_exists(g, y) {
                            let (tag, rem) = self.char_tag(g, y);
                            if tag == EXT_TAG {
                                best = Some((g, y));
                                break 'found;
                            }
                            let u = self.height_plus_depth(g, y);
                            if u > w {
                                best = Some((g, y));
                                w = u;
                                if u >= v {
                                    break 'found;
                                }
                            }
                            if tag != LIST_TAG {
                                break;
                            }
                            y = rem;
                        }
                    }
                    if zz < 16 {
                        break;
                    }
                }
            }
            if large_attempt {
                break;
            }
            large_attempt = true;
            (z, x) = ((d >> 8 & 15) as u8, d as u8);
        }
        let b = match best {
            Some((f, c)) => match self.char_tag(f, c) {
                (EXT_TAG, r) => self.extensible(f, r, v),
                _ => self.char_box(f, c),
            },
            None => {
                let width = self.e.eqtb.word(Reg { region: Region::Dimen, slot: NULL_DELIMITER_SPACE });
                self.new_box(BoxRec { width, ..BoxRec::default() })
            }
        };
        let r = self.bx(b);
        let axis = self.axis_height(size);
        self.bx_mut(b).shift = half(r.height - r.depth) - axis;
        b
    }

    /// §713, §714: an extensible delimiter from recipe `r` of font `f`, at least `v` tall.
    fn extensible(&mut self, f: u16, r: u8, v: i32) -> u32 {
        let [top, mid, bot, rep] = self.e.fonts.list[f as usize].metrics.exten(r);
        let u = self.height_plus_depth(f, rep);
        let [rw, _, _, ric] = self.char_dims(f, rep);
        let mut w = 0;
        for c in [bot, mid, top] {
            if c != 0 {
                w += self.height_plus_depth(f, c);
            }
        }
        let mut n = 0;
        if u > 0 {
            while w < v {
                w += u;
                n += 1;
                if mid != 0 {
                    w += u;
                }
            }
        }
        // `stack_into_box` puts each piece on top of the last: bottom first.
        let mut pieces: Vec<u8> = Vec::with_capacity(2 * n + 3);
        if bot != 0 {
            pieces.push(bot);
        }
        pieces.extend(std::iter::repeat_n(rep, n));
        if mid != 0 {
            pieces.push(mid);
            pieces.extend(std::iter::repeat_n(rep, n));
        }
        if top != 0 {
            pieces.push(top);
        }
        let mut list: Vec<Node> = Vec::with_capacity(pieces.len());
        let mut height = 0;
        for &c in &pieces {
            let b = self.char_box(f, c);
            height = self.bx(b).height;
            list.push(boxed(b));
        }
        list.reverse();
        self.vlist_box(&list, rw + ric, height, w - height)
    }

    /// §715 `rebox`: box `b` centred in a box `w` wide.
    fn rebox(&mut self, b: u32, w: i32) -> u32 {
        let r = self.bx(b);
        if r.width == w || r.list.0 == r.list.1 {
            self.bx_mut(b).width = w;
            return b;
        }
        let mut inner: Vec<Node> = if r.vertical { vec![boxed(b)] } else { self.e.nodes.get(r.list).to_vec() };
        if let [p] = inner[..] {
            if p.kind == Kind::Char {
                let v = self.char_dims(p.font, p.a as u8)[0];
                if v != r.width {
                    inner.push(kern(r.width - v));
                }
            }
        }
        let ss = Glue { width: 0, stretch: 65536, shrink: 65536, stretch_order: 1, shrink_order: 1 };
        let mut list = Vec::with_capacity(inner.len() + 2);
        list.push(self.e.nodes.glue_node(ss, 0));
        list.extend_from_slice(&inner);
        list.push(self.e.nodes.glue_node(ss, 0));
        // §715: a horizontal box's nodes moved into the new one, and it is freed.
        if !r.vertical {
            self.e.nodes.free_block(r.list);
            self.e.nodes.free_box(b);
        }
        self.hpack_nodes(&list, w, true)
    }

    /// §698 `flush_node_list` of a math list: noads free the boxes and lists in their
    /// fields, other nodes what they own.
    pub(super) fn flush_mlist(&mut self, l: (u32, u32)) {
        for i in l.0..l.1 {
            let n = self.e.nodes.list[i as usize];
            self.flush_math_node(n);
        }
        self.e.nodes.free_block(l);
    }

    /// §698 for one node of a math list.
    pub(super) fn flush_math_node(&mut self, n: Node) {
        if n.kind == Kind::Noad {
            let q = self.noads[n.a as usize];
            for f in [q.nucleus, q.supscr, q.subscr, q.extra] {
                match f {
                    Field::Box(b) => self.e.nodes.flush_box(b),
                    Field::Mlist(a, b) => self.flush_mlist((a, b)),
                    _ => {}
                }
            }
        } else {
            self.e.nodes.flush_node(n);
        }
    }

    /// §720 `clean_box`: field `p` as a box in style `s`.
    #[cfg_attr(feature = "ownership-check", track_caller)]
    fn clean_box(&mut self, p: Field, s: u8) -> u32 {
        let save = self.math.style;
        let q: Vec<Node> = match p {
            Field::Char { .. } => {
                let i = self.add_noad(Noad { nucleus: p, ..Noad::new(ORD) });
                let l = self.mlist_to_hlist(vec![noad_node(i)], s, false);
                self.set_style(save);
                l
            }
            Field::Box(b) => vec![boxed(b)],
            Field::Mlist(a, b) => {
                let list = self.e.nodes.get((a, b)).to_vec();
                // §720: the list is converted, its nodes taken.
                self.e.nodes.free_block((a, b));
                let l = self.mlist_to_hlist(list, s, false);
                self.set_style(save);
                l
            }
            _ => {
                let b = self.new_box(BoxRec::default());
                vec![boxed(b)]
            }
        };
        let x = match q[..] {
            // §720: a lone unshifted box is clean already.
            [n] if n.kind == Kind::Box && self.bx(n.a as u32).shift == 0 => n.a as u32,
            _ => self.hpack_nodes(&q, 0, false),
        };
        // §721: an italic correction after a lone character is not needed.
        let r = self.bx(x);
        if let &[c, k] = self.e.nodes.get(r.list) {
            if c.kind == Kind::Char && k.kind == Kind::Kern && !r.vertical {
                // The kern is dropped: the character alone gets a block of its class.
                self.e.nodes.free_block(r.list);
                self.bx_mut(x).list = self.e.nodes.store(&[c]);
            }
        }
        x
    }

    /// §722 `fetch`: the font and character of a character field, if it exists; the field
    /// becomes empty otherwise.
    fn fetch(&mut self, a: &mut Field) -> Option<(u16, u8)> {
        let (Field::Char { fam, c } | Field::TextChar { fam, c }) = *a else { return None };
        let f = self.fam_fnt(fam, self.math.size);
        if f == 0 {
            let size = match self.math.size {
                TEXT_SIZE => "textfont",
                SCRIPT_SIZE => "scriptfont",
                _ => "scriptscriptfont",
            };
            self.error(format!("\\{size} {fam} is undefined (character {})", c as char));
            *a = Field::Empty;
            return None;
        }
        if !self.char_exists(f, c) {
            // §581 `char_warning`: only logged.
            *a = Field::Empty;
            return None;
        }
        Some((f, c))
    }

    pub(super) fn add_noad(&mut self, n: Noad) -> u32 {
        self.noads.push(n);
        self.noads.len() as u32 - 1
    }

    /// §726 `mlist_to_hlist`: math list `ml` in `style` as a horizontal list; `penalties`
    /// says whether breaks are allowed after binary operations and relations.
    pub(super) fn mlist_to_hlist(&mut self, mut ml: Vec<Node>, style: u8, penalties: bool) -> Vec<Node> {
        self.set_style(style);
        let mut buf: Vec<Node> = Vec::new();
        let (mut r, mut r_type) = (None::<usize>, OP);
        let (mut max_h, mut max_d) = (0, 0);
        let mut i = 0;
        while i < ml.len() {
            let n = ml[i];
            if n.kind != Kind::Noad {
                // §730: nodes that can appear in an mlist.
                match n.kind {
                    Kind::Rule => {
                        let [_, h, d] = self.e.nodes.rules[n.a as usize];
                        max_h = max_h.max(h);
                        max_d = max_d.max(d);
                    }
                    // §732
                    Kind::Glue if n.sub == MU_GLUE => {
                        let g = math_glue(self.e.nodes.glue(&n), self.math.mu);
                        ml[i] = self.e.nodes.glue_node(g, 0);
                        // §732: the mu glue gives way to its converted copy.
                        self.e.nodes.flush_node(n);
                    }
                    Kind::Glue if n.sub == COND_MATH_GLUE && self.math.size != TEXT_SIZE => {
                        if ml.get(i + 1).is_some_and(|p| matches!(p.kind, Kind::Glue | Kind::Kern)) {
                            let p = ml.remove(i + 1);
                            self.e.nodes.flush_node(p);
                        }
                    }
                    // §717 `math_kern`
                    Kind::Kern if n.sub == MU_GLUE => {
                        let (k, f) = mu_parts(self.math.mu);
                        ml[i] = Node { sub: 1, a: mu_mult(n.a, k, f), ..n };
                    }
                    _ => {}
                }
                i += 1;
                continue;
            }
            let q = n.a as usize;
            let mut delta = 0;
            // §728: the kind decides; `false` when the noad needs no hlist from its nucleus.
            let mut convert = true;
            let mut measure = true;
            loop {
                match self.noads[q].kind {
                    BIN => {
                        if matches!(r_type, BIN | OP | REL | OPEN | PUNCT | LEFT) {
                            self.noads[q].kind = ORD;
                            continue;
                        }
                    }
                    REL | CLOSE | PUNCT | RIGHT => {
                        if r_type == BIN {
                            if let Some(r) = r {
                                self.noads[r].kind = ORD;
                            }
                        }
                        if self.noads[q].kind == RIGHT {
                            (convert, measure) = (false, false);
                        }
                    }
                    LEFT => (convert, measure) = (false, false),
                    FRACTION => {
                        self.make_fraction(q, &mut buf);
                        convert = false;
                    }
                    OP => {
                        delta = self.make_op(q, &mut buf);
                        if self.noads[q].sub == LIMITS {
                            convert = false;
                        }
                    }
                    ORD => self.make_ord(&mut ml, i),
                    RADICAL => self.make_radical(q),
                    OVER => self.make_over(q),
                    UNDER => self.make_under(q),
                    ACCENT => self.make_math_accent(q),
                    VCENTER => self.make_vcenter(q),
                    STYLE => {
                        self.set_style(self.noads[q].sub);
                        (convert, measure) = (false, false);
                    }
                    // §731: a style node and the list for the current style.
                    CHOICE => {
                        let c = self.noads[q];
                        let chosen = match self.math.style / 2 {
                            0 => c.nucleus,
                            1 => c.supscr,
                            2 => c.subscr,
                            _ => c.extra,
                        };
                        self.noads[q] = Noad { sub: self.math.style, ..Noad::new(STYLE) };
                        if let Field::Mlist(a, b) = chosen {
                            let list = self.e.nodes.get((a, b)).to_vec();
                            self.e.nodes.free_block((a, b));
                            ml.splice(i + 1..i + 1, list);
                        }
                        // §731: the other three lists are flushed.
                        for f in [c.nucleus, c.supscr, c.subscr, c.extra] {
                            if f != chosen {
                                if let Field::Mlist(a, b) = f {
                                    self.flush_mlist((a, b));
                                }
                            }
                        }
                        (convert, measure) = (false, false);
                    }
                    _ => {}
                }
                break;
            }
            let kind = self.noads[q].kind;
            if kind == STYLE {
                i += 1;
                continue;
            }
            if convert {
                self.convert_nucleus(q, delta, &mut buf);
            }
            if measure {
                // §727 `check_dimensions`.
                let (s, e) = self.noads[q].hlist;
                let (_, h, d, _, _) = self.measure_h(&buf[s as usize..e as usize]);
                max_h = max_h.max(h);
                max_d = max_d.max(d);
            }
            // `done_with_noad` (e-TeX: after `\middle` or `\right`, the style is reset).
            r = Some(q);
            r_type = kind;
            if r_type == RIGHT {
                r_type = LEFT;
                self.set_style(style);
            }
            i += 1;
        }
        // §729
        if r_type == BIN {
            if let Some(r) = r {
                self.noads[r].kind = ORD;
            }
        }
        self.second_pass(&ml, &buf, style, penalties, max_h, max_d)
    }

    /// §754: the nucleus of noad `q` as its hlist, with its scripts (§756).
    fn convert_nucleus(&mut self, q: usize, mut delta: i32, buf: &mut Vec<Node>) {
        let start = buf.len() as u32;
        let mut nucleus = self.noads[q].nucleus;
        match nucleus {
            Field::Char { .. } | Field::TextChar { .. } => {
                if let Some((f, c)) = self.fetch(&mut nucleus) {
                    // §755
                    delta = self.char_dims(f, c)[3];
                    buf.push(Node { kind: Kind::Char, sub: 0, font: f, a: c as i32, b: 0 });
                    if matches!(nucleus, Field::TextChar { .. }) && self.param_of(f, 2) != 0 {
                        delta = 0;
                    }
                    if self.noads[q].subscr == Field::Empty && delta != 0 {
                        buf.push(kern(delta));
                        delta = 0;
                    }
                }
                self.noads[q].nucleus = nucleus;
            }
            Field::Empty => {}
            Field::Box(b) => buf.push(boxed(b)),
            Field::Mlist(a, b) => {
                let save = self.math.style;
                let list = self.e.nodes.get((a, b)).to_vec();
                self.e.nodes.free_block((a, b));
                let h = self.mlist_to_hlist(list, save, false);
                self.set_style(save);
                let b = self.hpack_nodes(&h, 0, false);
                buf.push(boxed(b));
            }
        }
        self.noads[q].hlist = (start, buf.len() as u32);
        if self.noads[q].subscr != Field::Empty || self.noads[q].supscr != Field::Empty {
            self.make_scripts(q, delta, buf);
        }
    }

    /// §734 `make_over`.
    fn make_over(&mut self, q: usize) {
        let x = self.clean_box(self.noads[q].nucleus, cramped(self.math.style));
        let t = self.rule_thickness();
        let b = self.overbar(x, 3 * t, t);
        self.noads[q].nucleus = Field::Box(b);
    }

    /// §735 `make_under`.
    fn make_under(&mut self, q: usize) {
        let x = self.clean_box(self.noads[q].nucleus, self.math.style);
        let t = self.rule_thickness();
        let rule = self.fraction_rule(t);
        let y = self.vpack_nodes(&[boxed(x), kern(3 * t), rule]);
        let (xh, yr) = (self.bx(x).height, self.bx(y));
        let delta = yr.height + yr.depth + t;
        let b = self.bx_mut(y);
        b.height = xh;
        b.depth = delta - xh;
        self.noads[q].nucleus = Field::Box(y);
    }

    /// §736 `make_vcenter`.
    fn make_vcenter(&mut self, q: usize) {
        let Field::Box(v) = self.noads[q].nucleus else { return };
        let axis = self.axis_height(self.math.size);
        let r = self.bx(v);
        let delta = r.height + r.depth;
        let b = self.bx_mut(v);
        b.height = axis + half(delta);
        b.depth = delta - b.height;
    }

    /// §737 `make_radical`.
    fn make_radical(&mut self, q: usize) {
        let x = self.clean_box(self.noads[q].nucleus, cramped(self.math.style));
        let t = self.rule_thickness();
        let mut clr = if self.math.style < TEXT { t + self.mathsy(5, self.math.size).abs() / 4 } else { t + t.abs() / 4 };
        let xr = self.bx(x);
        let y = self.var_delimiter(self.noads[q].delims[0], self.math.size, xr.height + xr.depth + clr + t);
        let yr = self.bx(y);
        let delta = yr.depth - (xr.height + xr.depth + clr);
        if delta > 0 {
            clr += half(delta);
        }
        self.bx_mut(y).shift = -(xr.height + clr);
        let bar = self.overbar(x, clr, yr.height);
        let b = self.hpack_nodes(&[boxed(y), boxed(bar)], 0, false);
        self.noads[q].nucleus = Field::Box(b);
    }

    /// The kern for `next` in the lig/kern program of `c` in `f`, or its ligature step:
    /// `Ok(Some(kern))`, `Err((op, rem))`, or `Ok(None)` when there is neither (§545).
    fn lig_kern_step(&self, f: u16, c: u8, next: u8) -> Result<Option<i32>, (u8, u8)> {
        let font = &self.e.fonts.list[f as usize];
        let Some(mut a) = font.metrics.lig_kern_start(c) else { return Ok(None) };
        let (prog, kerns) = font.metrics.lig_kern();
        loop {
            let Some(&j) = prog.get(a) else { return Ok(None) };
            let (skip, nc, op, rem) = ((j >> 24) as u8, (j >> 16 & 0xFF) as u8, (j >> 8 & 0xFF) as u8, j as u8);
            if nc == next && skip <= STOP_FLAG {
                if op >= STOP_FLAG {
                    let k = kerns.get(restart(j) - 256 * STOP_FLAG as usize).copied().unwrap_or(0);
                    return Ok(Some(Scaler::new(font.size).scale(k)));
                }
                return Err((op, rem));
            }
            if skip >= STOP_FLAG {
                return Ok(None);
            }
            a += skip as usize + 1;
        }
    }

    /// §738 `make_math_accent`.
    fn make_math_accent(&mut self, q: usize) {
        let mut acc = self.noads[q].extra;
        let Some((f, mut c)) = self.fetch(&mut acc) else {
            self.noads[q].extra = acc;
            return;
        };
        // §741: the skew, from the kern with the font's skew character.
        let mut s = 0;
        let mut nucleus = self.noads[q].nucleus;
        if matches!(nucleus, Field::Char { .. }) {
            if let Some((nf, nc)) = self.fetch(&mut nucleus) {
                let skew = self.e.fonts.list[nf as usize].skew_char;
                if self.char_tag(nf, nc).0 == LIG_TAG && (0..256).contains(&skew) {
                    if let Ok(Some(k)) = self.lig_kern_step(nf, nc, skew as u8) {
                        s = k;
                    }
                }
            }
            self.noads[q].nucleus = nucleus;
        }
        let mut x = self.clean_box(self.noads[q].nucleus, cramped(self.math.style));
        let (w, mut h) = (self.bx(x).width, self.bx(x).height);
        // §740: a larger accent, while one fits.
        loop {
            let (tag, y) = self.char_tag(f, c);
            if tag != LIST_TAG || !self.char_exists(f, y) || self.char_dims(f, y)[0] > w {
                break;
            }
            c = y;
        }
        let mut delta = h.min(self.param_of(f, 5));
        let n = self.noads[q];
        if (n.supscr != Field::Empty || n.subscr != Field::Empty) && matches!(n.nucleus, Field::Char { .. }) {
            // §742: the scripts go with the accentee.
            let inner = self.add_noad(Noad { nucleus: n.nucleus, supscr: n.supscr, subscr: n.subscr, ..Noad::new(ORD) });
            let span = self.e.nodes.store(&[noad_node(inner)]);
            let m = &mut self.noads[q];
            m.supscr = Field::Empty;
            m.subscr = Field::Empty;
            m.nucleus = Field::Mlist(span.0, span.1);
            // §742: the first box is flushed.
            self.e.nodes.flush_box(x);
            x = self.clean_box(self.noads[q].nucleus, self.math.style);
            delta += self.bx(x).height - h;
            h = self.bx(x).height;
        }
        let y = self.char_box(f, c);
        let yw = self.bx(y).width;
        let yb = self.bx_mut(y);
        yb.shift = s + half(w - yw);
        yb.width = 0;
        let v = self.vpack_nodes(&[boxed(y), kern(-delta), boxed(x)]);
        self.bx_mut(v).width = self.bx(x).width;
        let vr = self.bx(v);
        if vr.height < h {
            // §739
            let mut list = vec![kern(h - vr.height)];
            list.extend_from_slice(self.e.nodes.get(vr.list));
            self.e.nodes.free_block(vr.list);
            let list = self.e.nodes.store(&list);
            let b = self.bx_mut(v);
            b.list = list;
            b.height = h;
        }
        self.noads[q].nucleus = Field::Box(v);
    }

    /// §743 `make_fraction`.
    fn make_fraction(&mut self, q: usize, buf: &mut Vec<Node>) {
        let style = self.math.style;
        let size = self.math.size;
        if self.noads[q].thickness == DEFAULT_CODE {
            self.noads[q].thickness = self.rule_thickness();
        }
        let n = self.noads[q];
        let thickness = n.thickness;
        // §744
        let mut x = self.clean_box(n.supscr, num_style(style));
        let mut z = self.clean_box(n.subscr, denom_style(style));
        if self.bx(x).width < self.bx(z).width {
            x = self.rebox(x, self.bx(z).width);
        } else {
            z = self.rebox(z, self.bx(x).width);
        }
        let (mut shift_up, mut shift_down) = if style < TEXT {
            (self.mathsy(8, size), self.mathsy(11, size))
        } else {
            (if thickness != 0 { self.mathsy(9, size) } else { self.mathsy(10, size) }, self.mathsy(12, size))
        };
        let (xr, zr) = (self.bx(x), self.bx(z));
        let axis = self.axis_height(size);
        let t = self.rule_thickness();
        let delta = half(thickness);
        if thickness == 0 {
            // §745
            let clr = if style < TEXT { 7 * t } else { 3 * t };
            let delta = half(clr - ((shift_up - xr.depth) - (zr.height - shift_down)));
            if delta > 0 {
                shift_up += delta;
                shift_down += delta;
            }
        } else {
            // §746
            let clr = if style < TEXT { 3 * thickness } else { thickness };
            let delta1 = clr - ((shift_up - xr.depth) - (axis + delta));
            let delta2 = clr - ((axis - delta) - (zr.height - shift_down));
            if delta1 > 0 {
                shift_up += delta1;
            }
            if delta2 > 0 {
                shift_down += delta2;
            }
        }
        // §747
        let list = if thickness == 0 {
            vec![boxed(x), kern((shift_up - xr.depth) - (zr.height - shift_down)), boxed(z)]
        } else {
            let y = self.fraction_rule(thickness);
            vec![boxed(x), kern((shift_up - xr.depth) - (axis + delta)), y, kern((axis - delta) - (zr.height - shift_down)), boxed(z)]
        };
        let v = self.vlist_box(&list, xr.width, shift_up + xr.height, zr.depth + shift_down);
        // §748
        let delta = if style < TEXT { self.mathsy(20, size) } else { self.mathsy(21, size) };
        let l = self.var_delimiter(n.delims[0], size, delta);
        let r = self.var_delimiter(n.delims[1], size, delta);
        let b = self.hpack_nodes(&[boxed(l), boxed(v), boxed(r)], 0, false);
        let start = buf.len() as u32;
        buf.push(boxed(b));
        self.noads[q].hlist = (start, buf.len() as u32);
    }

    /// §749 `make_op`: the offset between subscript and superscript; with limits the noad's
    /// hlist is made here (§750).
    fn make_op(&mut self, q: usize, buf: &mut Vec<Node>) -> i32 {
        let style = self.math.style;
        if self.noads[q].sub == 0 && style < TEXT {
            self.noads[q].sub = LIMITS;
        }
        let mut delta = 0;
        let mut nucleus = self.noads[q].nucleus;
        if let Field::Char { fam, .. } = nucleus {
            let found = self.fetch(&mut nucleus);
            if let Some((f, mut c)) = found {
                let (tag, rem) = self.char_tag(f, c);
                if style < TEXT && tag == LIST_TAG && self.char_exists(f, rem) {
                    c = rem;
                    nucleus = Field::Char { fam, c };
                }
                delta = self.char_dims(f, c)[3];
            }
            let x = self.clean_box(nucleus, style);
            if self.noads[q].subscr != Field::Empty && self.noads[q].sub != LIMITS {
                self.bx_mut(x).width -= delta;
            }
            let r = self.bx(x);
            let axis = self.axis_height(self.math.size);
            self.bx_mut(x).shift = half(r.height - r.depth) - axis;
            nucleus = Field::Box(x);
        }
        self.noads[q].nucleus = nucleus;
        if self.noads[q].sub == LIMITS {
            // §750
            let n = self.noads[q];
            let x = self.clean_box(n.supscr, sup_style(style));
            let y = self.clean_box(n.nucleus, style);
            let z = self.clean_box(n.subscr, sub_style(style));
            let width = self.bx(y).width.max(self.bx(x).width).max(self.bx(z).width);
            let x = self.rebox(x, width);
            let y = self.rebox(y, width);
            let z = self.rebox(z, width);
            self.bx_mut(x).shift = half(delta);
            self.bx_mut(z).shift = -half(delta);
            let yr = self.bx(y);
            let (mut height, mut depth) = (yr.height, yr.depth);
            // §751
            let mut list = Vec::with_capacity(7);
            if n.supscr != Field::Empty {
                let xr = self.bx(x);
                let shift_up = (self.mathex(11) - xr.depth).max(self.mathex(9));
                list.extend([kern(self.mathex(13)), boxed(x), kern(shift_up)]);
                height += self.mathex(13) + xr.height + xr.depth + shift_up;
            } else {
                // §751: an empty superscript's box is freed.
                self.e.nodes.flush_box(x);
            }
            list.push(boxed(y));
            if n.subscr != Field::Empty {
                let zr = self.bx(z);
                let shift_down = (self.mathex(12) - zr.height).max(self.mathex(10));
                list.extend([kern(shift_down), boxed(z), kern(self.mathex(13))]);
                depth += self.mathex(13) + zr.height + zr.depth + shift_down;
            } else {
                self.e.nodes.flush_box(z);
            }
            let v = self.vlist_box(&list, width, height, depth);
            let start = buf.len() as u32;
            buf.push(boxed(v));
            self.noads[q].hlist = (start, buf.len() as u32);
        }
        delta
    }

    /// §752 `make_ord`: a kern or ligature between this character and the next ordinary
    /// character of the same family (§753).
    fn make_ord(&mut self, ml: &mut Vec<Node>, i: usize) {
        let q = ml[i].a as usize;
        loop {
            let n = self.noads[q];
            let (Field::Char { fam, c: _ }, Field::Empty, Field::Empty) = (n.nucleus, n.subscr, n.supscr) else { return };
            let Some(&pn) = ml.get(i + 1) else { return };
            if pn.kind != Kind::Noad {
                return;
            }
            let p = pn.a as usize;
            let pnoad = self.noads[p];
            let Field::Char { fam: pfam, c: pc } = pnoad.nucleus else { return };
            if !(ORD..=PUNCT).contains(&pnoad.kind) || pfam != fam {
                return;
            }
            let mut nucleus = match n.nucleus {
                Field::Char { fam, c } => Field::TextChar { fam, c },
                other => other,
            };
            let fetched = self.fetch(&mut nucleus);
            self.noads[q].nucleus = nucleus;
            let Some((f, c)) = fetched else { return };
            match self.lig_kern_step(f, c, pc) {
                Ok(None) => return,
                Ok(Some(k)) => {
                    ml.insert(i + 1, kern(k));
                    return;
                }
                Err((op, rem)) => {
                    match op {
                        1 | 5 => self.noads[q].nucleus = Field::TextChar { fam, c: rem },
                        2 | 6 => self.noads[p].nucleus = Field::Char { fam, c: rem },
                        3 | 7 | 11 => {
                            let field = if op < 11 { Field::Char { fam, c: rem } } else { Field::TextChar { fam, c: rem } };
                            let r = self.add_noad(Noad { nucleus: field, ..Noad::new(ORD) });
                            ml.insert(i + 1, noad_node(r));
                        }
                        _ => {
                            ml.remove(i + 1);
                            let m = &mut self.noads[q];
                            m.nucleus = Field::TextChar { fam, c: rem };
                            m.subscr = pnoad.subscr;
                            m.supscr = pnoad.supscr;
                        }
                    }
                    if op > 3 {
                        return;
                    }
                    if let Field::TextChar { fam, c } = self.noads[q].nucleus {
                        self.noads[q].nucleus = Field::Char { fam, c };
                    }
                }
            }
        }
    }

    /// §756 `make_scripts`: the scripts of noad `q` after its hlist, the superscript `delta`
    /// to the right of the subscript.
    fn make_scripts(&mut self, q: usize, delta: i32, buf: &mut Vec<Node>) {
        let style = self.math.style;
        let size = self.math.size;
        let (s, e) = self.noads[q].hlist;
        let (mut shift_up, mut shift_down) = match buf.get(s as usize) {
            Some(n) if n.kind == Kind::Char && s < e => (0, 0),
            _ => {
                let (_, h, d, _, _) = self.measure_h(&buf[s as usize..e as usize]);
                let t = if style < SCRIPT { SCRIPT_SIZE } else { SCRIPT_SCRIPT_SIZE };
                (h - self.mathsy(18, t), d + self.mathsy(19, t))
            }
        };
        let n = self.noads[q];
        let script_space = self.e.eqtb.word(Reg { region: Region::Dimen, slot: SCRIPT_SPACE });
        let x_height = self.mathsy(5, size);
        let x = if n.supscr == Field::Empty {
            // §757
            let x = self.clean_box(n.subscr, sub_style(style));
            self.bx_mut(x).width += script_space;
            let xr = self.bx(x);
            shift_down = shift_down.max(self.mathsy(16, size)).max(xr.height - (x_height * 4).abs() / 5);
            self.bx_mut(x).shift = shift_down;
            x
        } else {
            // §758
            let x = self.clean_box(n.supscr, sup_style(style));
            self.bx_mut(x).width += script_space;
            let clr = if style & 1 != 0 {
                self.mathsy(15, size)
            } else if style < TEXT {
                self.mathsy(13, size)
            } else {
                self.mathsy(14, size)
            };
            let xr = self.bx(x);
            shift_up = shift_up.max(clr).max(xr.depth + x_height.abs() / 4);
            if n.subscr == Field::Empty {
                self.bx_mut(x).shift = -shift_up;
                x
            } else {
                // §759
                let y = self.clean_box(n.subscr, sub_style(style));
                self.bx_mut(y).width += script_space;
                let yr = self.bx(y);
                shift_down = shift_down.max(self.mathsy(17, size));
                let clr = 4 * self.rule_thickness() - ((shift_up - xr.depth) - (yr.height - shift_down));
                if clr > 0 {
                    shift_down += clr;
                    let clr = (x_height * 4).abs() / 5 - (shift_up - xr.depth);
                    if clr > 0 {
                        shift_up += clr;
                        shift_down -= clr;
                    }
                }
                self.bx_mut(x).shift = delta;
                let v = self.vpack_nodes(&[boxed(x), kern((shift_up - xr.depth) - (yr.height - shift_down)), boxed(y)]);
                self.bx_mut(v).shift = shift_down;
                v
            }
        };
        buf.push(boxed(x));
        self.noads[q].hlist.1 = buf.len() as u32;
    }

    /// §762 `make_left_right`: the delimiter of a `\left`, `\middle` or `\right` noad, big
    /// enough for the formula; the noad then counts as an open or close.
    fn make_left_right(&mut self, q: usize, style: u8, max_d: i32, max_h: i32) -> (u8, u32) {
        self.set_style(style);
        let size = self.math.size;
        let delta2 = max_d + self.axis_height(size);
        let delta1 = (max_h + max_d - delta2).max(delta2);
        let factor = self.ip(DELIMITER_FACTOR);
        let shortfall = self.e.eqtb.word(Reg { region: Region::Dimen, slot: DELIMITER_SHORTFALL });
        let delta = ((delta1 / 500) * factor).max(delta1 + delta1 - shortfall);
        let b = self.var_delimiter(self.noads[q].delims[0], size, delta);
        let kind = if self.noads[q].kind == LEFT { OPEN } else { CLOSE };
        (kind, b)
    }

    /// §760–§767: the noads' lists joined with the spacing between noad types and the
    /// penalties after binary operations and relations.
    fn second_pass(&mut self, ml: &[Node], buf: &[Node], style: u8, penalties: bool, max_h: i32, max_d: i32) -> Vec<Node> {
        let mut out: Vec<Node> = Vec::with_capacity(buf.len() + ml.len());
        let mut r_type: Option<u8> = None;
        self.set_style(style);
        for (i, n) in ml.iter().enumerate() {
            if n.kind != Kind::Noad {
                out.push(*n);
                continue;
            }
            let q = n.a as usize;
            let kind = self.noads[q].kind;
            let mut t = ORD;
            let mut pen = INF_PENALTY;
            let mut delim = None;
            match kind {
                OP | OPEN | CLOSE | PUNCT | INNER => t = kind,
                BIN => {
                    t = BIN;
                    pen = self.ip(BIN_OP_PENALTY);
                }
                REL => {
                    t = REL;
                    pen = self.ip(REL_PENALTY);
                }
                LEFT | RIGHT => {
                    let (k, b) = self.make_left_right(q, style, max_d, max_h);
                    t = k;
                    delim = Some(b);
                }
                STYLE => {
                    self.set_style(self.noads[q].sub);
                    continue;
                }
                _ => {}
            }
            // §766
            if let Some(rt) = r_type {
                let x = match MATH_SPACING[rt as usize * 8 + t as usize] {
                    b'1' | b'3' | b'4' if self.math.style >= SCRIPT => None,
                    b'1' | b'2' => Some(THIN_MU_SKIP),
                    b'3' => Some(MED_MU_SKIP),
                    b'4' => Some(THICK_MU_SKIP),
                    _ => None,
                };
                if let Some(x) = x {
                    let g = math_glue(self.e.eqtb.glue(Reg { region: Region::Glue, slot: x }), self.math.mu);
                    out.push(self.e.nodes.glue_node(g, x as u8 + 1));
                }
            }
            // §767
            match delim {
                Some(b) => out.push(boxed(b)),
                None => {
                    let (s, e) = self.noads[q].hlist;
                    out.extend_from_slice(&buf[s as usize..e as usize]);
                }
            }
            if penalties && pen < INF_PENALTY {
                if let Some(next) = ml.get(i + 1) {
                    let rel = next.kind == Kind::Noad && self.noads[next.a as usize].kind == REL;
                    if next.kind != Kind::Penalty && !rel {
                        out.push(Node::new(Kind::Penalty, pen));
                    }
                }
            }
            // e-TeX: a `\middle` counts as an open noad for what follows.
            if kind == RIGHT {
                t = OPEN;
            }
            r_type = Some(t);
        }
        out
    }
}

pub(super) fn kern(w: i32) -> Node {
    Node::new(Kind::Kern, w)
}

pub(super) fn boxed(b: u32) -> Node {
    Node::new(Kind::Box, b as i32)
}

pub(super) fn noad_node(i: u32) -> Node {
    Node::new(Kind::Noad, i as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_match_the_table() {
        use super::super::engine_names::param;
        for (name, region, slot) in [
            ("binoppenalty", Region::Int, BIN_OP_PENALTY),
            ("relpenalty", Region::Int, REL_PENALTY),
            ("delimiterfactor", Region::Int, DELIMITER_FACTOR),
            ("delimitershortfall", Region::Dimen, DELIMITER_SHORTFALL),
            ("nulldelimiterspace", Region::Dimen, NULL_DELIMITER_SPACE),
            ("scriptspace", Region::Dimen, SCRIPT_SPACE),
            ("thinmuskip", Region::Glue, THIN_MU_SKIP),
            ("medmuskip", Region::Glue, MED_MU_SKIP),
            ("thickmuskip", Region::Glue, THICK_MU_SKIP),
        ] {
            assert_eq!(param(name), Some(Reg { region, slot }), "{name}");
        }
    }

    #[test]
    fn styles_follow_tex() {
        // §702: D=0 D'=1 T=2 T'=3 S=4 S'=5 SS=6 SS'=7.
        assert_eq!((0..8).map(sup_style).collect::<Vec<_>>(), [4, 5, 4, 5, 6, 7, 6, 7]);
        assert_eq!((0..8).map(sub_style).collect::<Vec<_>>(), [5, 5, 5, 5, 7, 7, 7, 7]);
        assert_eq!((0..8).map(num_style).collect::<Vec<_>>(), [2, 3, 4, 5, 6, 7, 6, 7]);
        assert_eq!((0..8).map(denom_style).collect::<Vec<_>>(), [3, 3, 5, 5, 7, 7, 7, 7]);
        assert_eq!(half(-3), -1);
        assert_eq!(half(5), 3);
    }
}
