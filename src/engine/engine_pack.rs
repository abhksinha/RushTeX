//! Packaging lists into boxes: tex.web §644–§679 (`hpack`, `vpackage`, `append_to_vlist`)
//! and §108 (`badness`).
//!
//! Packing reads the list being built (the top segment of the build buffer), totals its
//! widths, heights, depths and glue by order, sets the glue the way TeX does, and stores the
//! list in the node arena as one span.

use super::engine_eqtb::{Glue, Reg, Region};
use super::engine_node::{BoxRec, Kind, Node, Sign, IGNORE_DEPTH, RUNNING};
use super::engine_run::Machine;
use super::engine_whatsit::sized;

// Parameter slots (tex.web §236, §247, §224 numbering; checked against the table in tests).
pub(super) const LOOSENESS: u32 = 19;
pub(super) const HBADNESS: u32 = 26;
pub(super) const VBADNESS: u32 = 27;
pub(super) const HANG_AFTER: u32 = 41;
pub(super) const PAR_INDENT: u32 = 0;
pub(super) const LINE_SKIP_LIMIT: u32 = 2;
pub(super) const BOX_MAX_DEPTH: u32 = 7;
pub(super) const HSIZE: u32 = 3;
pub(super) const HFUZZ: u32 = 8;
pub(super) const VFUZZ: u32 = 9;
pub(super) const OVERFULL_RULE: u32 = 16;
pub(super) const HANG_INDENT: u32 = 17;
pub(super) const LINE_SKIP: u32 = 0;
pub(super) const BASELINE_SKIP: u32 = 1;
pub(super) const PAR_SKIP: u32 = 2;
pub(super) const SPACE_SKIP: u32 = 12;
pub(super) const XSPACE_SKIP: u32 = 13;
pub(super) const EVERY_PAR: u32 = 1;
pub(super) const EVERY_HBOX: u32 = 4;
pub(super) const EVERY_VBOX: u32 = 5;

/// §108: how bad it is to stretch or shrink by `t` when the total is `s`, about
/// 100·(t/s)³, capped at 10000.
pub fn badness(t: i32, s: i32) -> i32 {
    if t == 0 {
        return 0;
    }
    if s <= 0 {
        return 10_000;
    }
    let r = if t <= 7_230_584 {
        (t as i64 * 297 / s as i64) as i32
    } else if s >= 1_663_497 {
        t / (s / 297)
    } else {
        t
    };
    if r > 1290 {
        10_000
    } else {
        (r * r * r + 0o400000) / 0o1000000
    }
}

/// The highest order with a nonzero total (§659, §665).
pub(super) fn order(totals: &[i32; 4]) -> u8 {
    (0..4).rev().find(|&o| totals[o] != 0).unwrap_or(0) as u8
}

impl Machine<'_, '_> {
    fn int(&self, slot: u32) -> i32 {
        self.e.eqtb.word(Reg { region: Region::Int, slot })
    }

    pub(super) fn dimen_param(&self, slot: u32) -> i32 {
        self.e.eqtb.word(Reg { region: Region::Dimen, slot })
    }

    pub(super) fn glue_param(&self, slot: u32) -> Glue {
        self.e.eqtb.glue(Reg { region: Region::Glue, slot })
    }

    /// §651–§656: the natural width, height and depth of horizontal list `list`, and its
    /// stretch and shrink by order.
    pub(super) fn measure_h(&self, list: &[Node]) -> (i32, i32, i32, [i32; 4], [i32; 4]) {
        let (mut x, mut h, mut d) = (0i32, 0i32, 0i32);
        let (mut stretch, mut shrink) = ([0i32; 4], [0i32; 4]);
        let nodes = &self.e.nodes;
        for n in list {
            match n.kind {
                // §654: a ligature measures as its character.
                Kind::Char | Kind::Lig => {
                    let f = &self.e.fonts.list[n.font as usize];
                    if let Some([wd, ht, dp, _]) = u8::try_from(n.a).ok().and_then(|c| f.dims(c, &self.e.fonts.info)) {
                        x += wd;
                        h = h.max(ht);
                        d = d.max(dp);
                    }
                }
                // A word: its entry's size, summed once.
                Kind::Word => {
                    let r = nodes.words.rec(n.a as u32);
                    x += r.width;
                    h = h.max(r.height);
                    d = d.max(r.depth);
                }
                Kind::Box => {
                    let b = &nodes.boxes[n.a as usize];
                    x += b.width;
                    h = h.max(b.height - b.shift);
                    d = d.max(b.depth + b.shift);
                }
                Kind::Rule => {
                    let [rw, rh, rd] = nodes.rules[n.a as usize];
                    x += rw;
                    h = h.max(rh);
                    d = d.max(rd);
                }
                Kind::Glue => {
                    let g = nodes.glue(n);
                    x += g.width;
                    stretch[g.stretch_order as usize] += g.stretch;
                    shrink[g.shrink_order as usize] += g.shrink;
                    // §656: leaders count their box's (or rule's) height and depth.
                    if n.sub >= 110 {
                        let [_, rh, rd] = nodes.rules[n.b as usize];
                        h = h.max(rh);
                        d = d.max(rd);
                    } else if n.sub >= 100 {
                        let b = &nodes.boxes[n.b as usize];
                        h = h.max(b.height);
                        d = d.max(b.depth);
                    }
                }
                Kind::Kern | Kind::Math => x += n.a,
                // pdfTeX: an image whatsit counts like a rule.
                Kind::Whatsit if sized(n.sub) => {
                    let [w, ih, id] = nodes.rules[n.a as usize];
                    x += w;
                    h = h.max(ih);
                    d = d.max(id);
                }
                Kind::Penalty | Kind::Disc | Kind::Ins | Kind::Mark | Kind::Adjust | Kind::Whatsit | Kind::Noad => {}
            }
        }
        (x, h, d, stretch, shrink)
    }

    /// §649 `hpack` of the list from `start` in the build buffer: to width `w` when `exact`,
    /// else its natural width plus `w`. The list moves to the arena.
    #[cfg_attr(feature = "ownership-check", track_caller)]
    pub(super) fn hpack(&mut self, start: usize, w: i32, exact: bool) -> BoxRec {
        let (x, h, d, stretch, shrink) = self.measure_h(&self.cur[start..]);
        let width = if exact { w } else { x + w };
        let mut b = BoxRec { vertical: false, width, height: h, depth: d, ..BoxRec::default() };
        let excess = width - x;
        let nonempty = self.cur.len() > start;
        let (hbadness, hfuzz) = (self.int(HBADNESS), self.dimen_param(HFUZZ));
        self.last_badness = 0;
        let overfull = self.set_glue(&mut b, excess, &stretch, &shrink, nonempty, hbadness, hfuzz);
        // §666: an overfull box gets a rule `\overfullrule` wide at its end.
        let rule = self.dimen_param(OVERFULL_RULE);
        if overfull && rule > 0 {
            let r = self.e.nodes.rule_node([rule, RUNNING, RUNNING]);
            self.cur.push(r);
        }
        // §655: with an adjustment list open, insertions, marks and `\vadjust` material
        // move out of the box to it.
        if let Some(adj) = self.adjust.as_mut() {
            let nodes = &mut self.e.nodes;
            let mut k = start;
            for i in start..self.cur.len() {
                let n = self.cur[i];
                match n.kind {
                    Kind::Ins | Kind::Mark => adj.push(n),
                    Kind::Adjust => {
                        // The material moves out and the adjust node is freed (§655).
                        let l = (n.a as u32, n.b as u32);
                        adj.extend_from_slice(nodes.get(l));
                        nodes.free_block(l);
                    }
                    _ => {
                        self.cur[k] = n;
                        k += 1;
                    }
                }
            }
            self.cur.truncate(k);
        }
        b.list = self.e.nodes.store(&self.cur[start..]);
        self.cur.truncate(start);
        b
    }

    /// §668 `vpackage` of the list from `start`: to height `h` when `exact`, else natural
    /// plus `h`, with depth at most `max_depth`.
    #[cfg_attr(feature = "ownership-check", track_caller)]
    pub(super) fn vpack(&mut self, start: usize, h: i32, exact: bool, max_depth: i32) -> BoxRec {
        let (mut w, mut d, mut x) = (0i32, 0i32, 0i32);
        let (mut stretch, mut shrink) = ([0i32; 4], [0i32; 4]);
        let nodes = &self.e.nodes;
        for n in &self.cur[start..] {
            match n.kind {
                Kind::Box => {
                    let b = &nodes.boxes[n.a as usize];
                    x += d + b.height;
                    d = b.depth;
                    w = w.max(b.width + b.shift);
                }
                Kind::Rule => {
                    let [rw, rh, rd] = nodes.rules[n.a as usize];
                    x += d + rh;
                    d = rd;
                    w = w.max(rw);
                }
                Kind::Glue => {
                    let g = nodes.glue(n);
                    x += d + g.width;
                    d = 0;
                    stretch[g.stretch_order as usize] += g.stretch;
                    shrink[g.shrink_order as usize] += g.shrink;
                    if n.sub >= 110 {
                        w = w.max(nodes.rules[n.b as usize][0]);
                    } else if n.sub >= 100 {
                        w = w.max(nodes.boxes[n.b as usize].width);
                    }
                }
                Kind::Kern => {
                    x += d + n.a;
                    d = 0;
                }
                Kind::Whatsit if sized(n.sub) => {
                    let [iw, ih, id] = nodes.rules[n.a as usize];
                    x += d + ih;
                    d = id;
                    w = w.max(iw);
                }
                Kind::Char | Kind::Lig | Kind::Word | Kind::Disc | Kind::Penalty | Kind::Ins | Kind::Mark | Kind::Adjust | Kind::Whatsit | Kind::Math | Kind::Noad => {}
            }
        }
        // §669: a depth beyond `max_depth` moves into the height.
        if d > max_depth {
            x += d - max_depth;
            d = max_depth;
        }
        let height = if exact { h } else { x + h };
        let mut b = BoxRec { vertical: true, width: w, height, depth: d, ..BoxRec::default() };
        let nonempty = self.cur.len() > start;
        let (vbadness, vfuzz) = (self.int(VBADNESS), self.dimen_param(VFUZZ));
        self.last_badness = 0;
        self.set_glue(&mut b, height - x, &stretch, &shrink, nonempty, vbadness, vfuzz);
        b.list = self.e.nodes.store(&self.cur[start..]);
        self.cur.truncate(start);
        b
    }

    /// §657–§667: sets the glue of `b` to take up `excess`, and `\badness`; `true` when the
    /// box is overfull beyond the fuzz (or `badness_limit` is below 100), as §666 tests.
    #[allow(clippy::too_many_arguments)]
    fn set_glue(&mut self, b: &mut BoxRec, excess: i32, stretch: &[i32; 4], shrink: &[i32; 4], nonempty: bool, badness_limit: i32, fuzz: i32) -> bool {
        if excess > 0 {
            let o = order(stretch);
            b.glue_order = o;
            b.glue_sign = Sign::Stretching;
            if stretch[o as usize] != 0 {
                b.glue_set = excess as f64 / stretch[o as usize] as f64;
            } else {
                b.glue_sign = Sign::Normal;
            }
            if o == 0 && nonempty {
                self.last_badness = badness(excess, stretch[0]);
            }
        } else if excess < 0 {
            let o = order(shrink);
            b.glue_order = o;
            b.glue_sign = Sign::Shrinking;
            if shrink[o as usize] != 0 {
                b.glue_set = (-excess) as f64 / shrink[o as usize] as f64;
            } else {
                b.glue_sign = Sign::Normal;
            }
            if shrink[o as usize] < -excess && o == 0 && nonempty {
                // §664: overfull: the glue shrinks all it can.
                self.last_badness = 1_000_000;
                b.glue_set = 1.0;
                return -excess - shrink[0] > fuzz || badness_limit < 100;
            } else if o == 0 && nonempty {
                self.last_badness = badness(-excess, shrink[0]);
            }
        }
        false
    }

    /// §679 `append_to_vlist`: box `b` goes on the vertical list being built, after the
    /// interline glue `\baselineskip` asks for (or `\lineskip` when that would bring the
    /// boxes closer than `\lineskiplimit`).
    pub(super) fn append_to_vlist(&mut self, b: u32) {
        let (height, depth) = {
            let r = &self.e.nodes.boxes[b as usize];
            (r.height, r.depth)
        };
        if self.aux > IGNORE_DEPTH {
            let base = self.glue_param(BASELINE_SKIP);
            let d = base.width - self.aux - height;
            let g = if d < self.dimen_param(LINE_SKIP_LIMIT) {
                self.e.nodes.glue_node(self.glue_param(LINE_SKIP), LINE_SKIP as u8 + 1)
            } else {
                self.e.nodes.glue_node(Glue { width: d, ..base }, BASELINE_SKIP as u8 + 1)
            };
            self.cur.push(g);
        }
        self.cur.push(Node::new(Kind::Box, b as i32));
        self.aux = depth;
    }
}

#[cfg(test)]
mod tests {
    use super::super::engine_names::param;
    use super::*;

    #[test]
    fn slots_match_the_table() {
        for (name, region, slot) in [
            ("looseness", Region::Int, LOOSENESS),
            ("hbadness", Region::Int, HBADNESS),
            ("vbadness", Region::Int, VBADNESS),
            ("hangafter", Region::Int, HANG_AFTER),
            ("parindent", Region::Dimen, PAR_INDENT),
            ("lineskiplimit", Region::Dimen, LINE_SKIP_LIMIT),
            ("boxmaxdepth", Region::Dimen, BOX_MAX_DEPTH),
            ("hfuzz", Region::Dimen, HFUZZ),
            ("vfuzz", Region::Dimen, VFUZZ),
            ("overfullrule", Region::Dimen, OVERFULL_RULE),
            ("hangindent", Region::Dimen, HANG_INDENT),
            ("lineskip", Region::Glue, LINE_SKIP),
            ("baselineskip", Region::Glue, BASELINE_SKIP),
            ("parskip", Region::Glue, PAR_SKIP),
            ("spaceskip", Region::Glue, SPACE_SKIP),
            ("xspaceskip", Region::Glue, XSPACE_SKIP),
            ("everypar", Region::Toks, EVERY_PAR),
            ("everyhbox", Region::Toks, EVERY_HBOX),
            ("everyvbox", Region::Toks, EVERY_VBOX),
        ] {
            assert_eq!(param(name), Some(Reg { region, slot }), "{name}");
        }
    }

    #[test]
    fn badness_follows_section_108() {
        assert_eq!(badness(0, 0), 0);
        assert_eq!(badness(10, 0), 10_000);
        assert_eq!(badness(100, 100), 100);
        assert_eq!(badness(50, 100), 12);
        assert_eq!(badness(200, 100), 800);
    }
}
