//! Shipping a page out to PDF: tex.web §619–§640 (`hlist_out`, `vlist_out`, `ship_out`) as
//! pdfTeX does them (`pdf_hlist_out`, `pdf_vlist_out`, `pdf_ship_out`), in our own form.
//!
//! The walk places every character and rule of the shipped box exactly where TeX's `cur_h`
//! and `cur_v` put them, glue rounded as §625 rounds it, and writes the page's content stream
//! straight away through `backend_pdftex::TextState` (pdfTeX's own operator choice), so no
//! intermediate list of positioned glyphs is kept. The document around the pages (fonts,
//! page tree) is `backend_ship`'s.

use super::engine_eqtb::Glue;
use super::engine_font::Font;
use super::engine_node::{BoxRec, Kind, Nodes, Sign, RUNNING};
use super::engine_run::Machine;
use super::engine_color::{ColorStacks, SET_ORIGIN};
use super::engine_node::Node;
use super::engine_pdfdoc::{self as doc, PdfDoc};
use super::engine_vf::{PacketOut, VfTable};
use super::engine_whatsit as what;
use crate::backend::backend_pdftex::TextState;
pub use crate::backend::backend_ship::ShippedPage;

const H_OFFSET: u32 = 18;
const V_OFFSET: u32 = 19;
const PDF_H_ORIGIN: u32 = 21;
const PDF_V_ORIGIN: u32 = 22;
const PDF_PAGE_WIDTH: u32 = 23;
const PDF_PAGE_HEIGHT: u32 = 24;
const PDF_GEN_TO_UNICODE: u32 = 81;
const PDF_MINOR_VERSION: u32 = 67;
const PDF_LINK_MARGIN: u32 = 25;
const PDF_DEST_MARGIN: u32 = 26;

/// `pdf_font_num` of a font not yet shipped.
const NOT_USED: u16 = u16::MAX;

/// The font whose PDF object `f` shares, fixed at `f`'s first use (pdftex.web
/// `pdf_init_font`): the earlier-used font of the same file name at another size, else `f`
/// itself. That font's size gives the shared `/Widths`. Searched once per font.
fn font_base(base: &mut Vec<u16>, fonts: &[Font], f: u16) -> u16 {
    if base.len() < fonts.len() {
        base.resize(fonts.len(), NOT_USED);
    }
    if base[f as usize] == NOT_USED {
        let name = &fonts[f as usize].name;
        base[f as usize] = (0..base.len()).map(|k| base[k]).find(|&b| b != NOT_USED && fonts[b as usize].name == *name).unwrap_or(f);
    }
    base[f as usize]
}

/// Pascal's `round` as web2c implements it (`zround`): `r + 0.5` truncated, so a value just
/// under one half rounds up when the addition does. `f64::round` would differ there by 1sp.
#[inline]
pub(super) fn zround(r: f64) -> i32 {
    if r > 2147483647.0 {
        i32::MAX
    } else if r < -2147483647.0 {
        -i32::MAX
    } else if r >= 0.0 {
        (r + 0.5) as i32
    } else {
        (r - 0.5) as i32
    }
}

/// §625 `vet_glue`: glue is kept within a billion scaled points.
#[inline]
fn vet_glue(g: f64) -> f64 {
    g.clamp(-1e9, 1e9)
}

/// The state of one page's walk: pdfTeX's `cur_h`/`cur_v` are passed down the recursion, the
/// page height turns them into PDF coordinates (y up).
struct Walk<'a> {
    nodes: &'a Nodes,
    fonts: &'a [Font],
    vf: &'a VfTable,
    text: TextState,
    out: Vec<u8>,
    page_height: i64,
    /// The font each font shares its PDF object with (`NOT_USED` before its first use).
    base: &'a mut Vec<u16>,
    /// Resource names (`/F<base>`), made when a font is first used on the page.
    names: Vec<String>,
    colors: &'a mut ColorStacks,
    /// A whatsit's token text `a..b` as pdfTeX's `show_token_list` prints it.
    text_of: &'a dyn Fn(i32, i32) -> String,
    doc: &'a mut PdfDoc,
    /// This page's object, the box nesting level (`cur_s`), whether leaders are being
    /// output, and the margins of links and destinations.
    page_obj: i32,
    cur_s: i32,
    leaders: bool,
    link_margin: i32,
    dest_margin: i32,
    /// pdfTeX's `pdf_link_stack`: each open link's nesting level, its `\pdfstartlink`, and
    /// the annotation (in `annots`) its latest piece is.
    link_stack: Vec<(i32, usize, usize)>,
    /// The page's link annotations: object, link, and rectangle (left, top, right, bottom).
    annots: Vec<(i32, usize, [i32; 4])>,
    /// `pdf_page_group_val`: the page's transparency group (0 for none); the images it
    /// shows (`pdf_ximage_list`), by resource number and object.
    group: i32,
    images: Vec<(u32, i32)>,
}

/// `set_rect_dimens`: the rectangle of an annotation or destination with dimensions `dims`
/// (running ones from the parent box at (x, y)) begun at (`cur_h`, `cur_v`).
fn rect_dimens(dims: [Option<i32>; 3], parent: &BoxRec, x: i32, y: i32, cur_h: i32, cur_v: i32, margin: i32) -> [i32; 4] {
    let right = dims[0].map_or(x + parent.width, |w| cur_h + w);
    let top = dims[1].map_or(y - parent.height, |h| cur_v - h);
    let bottom = dims[2].map_or(y + parent.depth, |d| cur_v + d);
    [cur_h - margin, top - margin, right + margin, bottom + margin]
}

impl Walk<'_> {
    fn char(&mut self, f: u16, c: u8, h: i32, v: i32) -> i32 {
        self.packet_char(f, c, h, v, 0)
    }

    fn literal(&mut self, mode: u8, s: &str, h: i32, v: i32) {
        let bytes: Vec<u8> = s.chars().map(|c| c as u32 as u8).collect();
        self.text.literal(&mut self.out, mode, &bytes, h as i64, v as i64, self.page_height);
    }

    /// pdfTeX's whatsits in `pdf_hlist_out`/`pdf_vlist_out` that write to the page, at TeX's
    /// (h, v): literals, `\special`, colour stacks, `\pdfsetmatrix`, `\pdfsave`, `\pdfrestore`.
    fn whatsit(&mut self, n: &Node, h: i32, v: i32) {
        let text = |w: &Self| (w.text_of)(n.a, n.b);
        let stack = n.font as usize;
        let lit = match n.sub {
            // `pdf_out_literal`; a `shipout` literal's text is written as stored.
            what::LITERAL | what::LATE_LITERAL => Some((n.font as u8, text(self))),
            // `literal(s, scan_special)`: only `PDF:` (or `pdf:`) specials, `direct:` and
            // `page:` choosing the mode.
            what::SPECIAL => {
                let s = text(self);
                let Some(rest) = s.strip_prefix("PDF:").or_else(|| s.strip_prefix("pdf:")) else { return };
                Some(match (rest.strip_prefix("direct:"), rest.strip_prefix("page:")) {
                    (Some(r), _) => (2, r.to_string()),
                    (_, Some(r)) => (1, r.to_string()),
                    _ => (SET_ORIGIN, rest.to_string()),
                })
            }
            what::COLOR_SET => self.colors.set(stack, text(self)),
            what::COLOR_PUSH => self.colors.push(stack, text(self)),
            what::COLOR_POP => self.colors.pop(stack),
            what::COLOR_CURRENT => self.colors.current(stack),
            // `pdf_out_setmatrix`: four numbers, written with ` 0 0 cm` at the origin.
            what::SET_MATRIX => {
                let s = text(self);
                let ok = s.split_whitespace().count() == 4 && s.split_whitespace().all(|t| t.parse::<f64>().is_ok());
                ok.then(|| (SET_ORIGIN, format!("{s} 0 0 cm")))
            }
            what::SAVE => Some((SET_ORIGIN, "q".to_string())),
            what::RESTORE => Some((SET_ORIGIN, "Q".to_string())),
            _ => None,
        };
        if let Some((mode, s)) = lit {
            // A colour stack writes nothing for an empty literal.
            if !(s.is_empty() && (what::COLOR_SET..=what::COLOR_CURRENT).contains(&n.sub)) {
                self.literal(mode, &s, h, v);
            }
        }
    }

    /// A character of a real font, written to the page.
    fn real_char(&mut self, f: u16, c: u8, w: i32, h: i32, v: i32) {
        let font = &self.fonts[f as usize];
        if self.names.len() <= f as usize {
            self.names.resize(f as usize + 1, String::new());
        }
        if self.names[f as usize].is_empty() {
            let base = font_base(self.base, self.fonts, f);
            self.names[f as usize] = format!("/F{base}");
        }
        self.text.set_font(&mut self.out, f as u32, &self.names[f as usize], font.size as i64);
        self.text.char(&mut self.out, c, w as i64, h as i64, self.page_height - v as i64);
    }

    /// A rule with its lower left corner at (h, v).
    fn rule(&mut self, h: i32, v: i32, w: i32, ht: i32) {
        self.text.rule(&mut self.out, h as i64, v as i64, self.page_height, w as i64, ht as i64);
    }

    /// `out_image` with the image's lower left corner at TeX's (h, v): the image is written
    /// at the end, and the page takes its transparency group.
    fn image(&mut self, n: &Node, h: i32, v: i32) {
        let [w, ht, dp] = self.nodes.rules[n.a as usize];
        let im = &mut self.doc.images[n.b as usize];
        im.wanted = true;
        let entry = (n.b as u32 + 1, im.obj);
        if !self.images.contains(&entry) {
            self.images.push(entry);
        }
        if self.group == 0 {
            self.group = im.group;
        }
        self.text.image(&mut self.out, n.b as usize + 1, h as i64, v as i64, self.page_height, [w as i64, ht as i64 + dp as i64]);
    }

    /// §621 `hlist_out` of box `b` with its reference point at (`cur_h`, `cur_v`).
    fn hlist(&mut self, b: &BoxRec, mut cur_h: i32, cur_v: i32) {
        let (g_order, g_sign) = (b.glue_order, b.glue_sign);
        let base_line = cur_v;
        let left_edge = cur_h;
        self.cur_s += 1;
        // "Create link annotations for the current hbox if needed": a link still open at this
        // nesting level continues in this box.
        for i in 0..self.link_stack.len() {
            let (level, link, _) = self.link_stack[i];
            if level == self.cur_s {
                let obj = self.doc.new_link_piece();
                let rect = rect_dimens(self.doc.link_dims(link), b, left_edge, base_line, left_edge, base_line, self.link_margin);
                self.link_stack[i].2 = self.annots.len();
                self.annots.push((obj, link, rect));
            }
        }
        let (mut cur_g, mut cur_glue) = (0i32, 0f64);
        for n in self.nodes.get(b.list) {
            match n.kind {
                Kind::Char => cur_h += self.char(n.font, n.a as u8, cur_h, base_line),
                Kind::Lig => cur_h += self.char(n.font, n.a as u8, cur_h, base_line),
                // §623
                Kind::Box => {
                    let inner = self.nodes.boxes[n.a as usize];
                    if inner.list.0 != inner.list.1 {
                        self.box_out(&inner, cur_h, base_line + inner.shift);
                    }
                    cur_h += inner.width;
                }
                // §624
                Kind::Rule => {
                    let [w, h, d] = self.nodes.rules[n.a as usize];
                    self.h_rule(b, w, h, d, cur_h, base_line);
                    cur_h += w;
                }
                // §625
                Kind::Glue => {
                    let g = self.nodes.glue(n);
                    let mut rule_wd = g.width - cur_g;
                    stretched(&g, g_sign, g_order, b.glue_set, &mut cur_g, &mut cur_glue);
                    rule_wd += cur_g;
                    if n.sub >= 110 {
                        let [_, h, d] = self.nodes.rules[n.b as usize];
                        self.h_rule(b, rule_wd, h, d, cur_h, base_line);
                    } else if n.sub >= 100 {
                        cur_h = self.h_leaders(n.sub - 100, n.b as u32, rule_wd, cur_h, left_edge, base_line);
                        continue;
                    }
                    cur_h += rule_wd;
                }
                Kind::Kern | Kind::Math => cur_h += n.a,
                Kind::Whatsit => match n.sub {
                    // pdfTeX's `Output a Image node in a hlist`.
                    what::XIMAGE => {
                        self.image(n, cur_h, base_line + self.nodes.rules[n.a as usize][2]);
                        cur_h += self.nodes.rules[n.a as usize][0];
                    }
                    doc::START_LINK => self.do_link(n.a as usize, b, left_edge, base_line, cur_h),
                    doc::END_LINK => self.end_link(cur_h),
                    doc::DEST => self.do_dest(n.a as usize, b, left_edge, base_line, cur_h, base_line),
                    doc::REF_OBJ => self.doc.want_obj(n.a),
                    _ => self.whatsit(n, cur_h, base_line),
                },
                _ => {}
            }
        }
        self.cur_s -= 1;
    }

    /// `do_link`: an annotation from (`cur_h`, `base_line`), its right edge set by
    /// `end_link` when its width is running.
    fn do_link(&mut self, link: usize, b: &BoxRec, x: i32, y: i32, cur_h: i32) {
        let obj = self.doc.link_obj(link);
        let rect = rect_dimens(self.doc.link_dims(link), b, x, y, cur_h, y, self.link_margin);
        self.link_stack.push((self.cur_s, link, self.annots.len()));
        self.annots.push((obj, link, rect));
    }

    fn end_link(&mut self, cur_h: i32) {
        let Some((_, link, annot)) = self.link_stack.pop() else { return };
        if self.doc.link_dims(link)[0].is_none() {
            self.annots[annot].2[2] = cur_h + self.link_margin;
        }
    }

    /// `do_dest` at (`cur_h`, `cur_v`) in box `b` whose reference point is (x, y).
    fn do_dest(&mut self, dest: usize, b: &BoxRec, x: i32, y: i32, cur_h: i32, cur_v: i32) {
        if self.leaders {
            return;
        }
        let rect = if self.doc.dest_is_fitr(dest) { rect_dimens(self.doc.dest_dims(dest), b, x, y, cur_h, cur_v, self.dest_margin) } else { [0; 4] };
        self.doc.place_dest(dest, self.page_obj, cur_h, cur_v, rect, self.page_height as i32);
    }

    /// §624 `fin_rule`: running dimensions take the box's.
    fn h_rule(&mut self, b: &BoxRec, w: i32, h: i32, d: i32, cur_h: i32, base_line: i32) {
        let h = if h == RUNNING { b.height } else { h };
        let d = if d == RUNNING { b.depth } else { d };
        let ht = h + d;
        if ht > 0 && w > 0 {
            self.rule(cur_h, base_line + d, w, ht);
        }
    }

    /// §626: leaders of box `lb` (kind 0 aligned, 1 centred, 2 expanded) over `rule_wd`;
    /// returns `cur_h` after them.
    fn h_leaders(&mut self, kind: u8, lb: u32, mut rule_wd: i32, mut cur_h: i32, left_edge: i32, base_line: i32) -> i32 {
        let leader = self.nodes.boxes[lb as usize];
        let leader_wd = leader.width;
        if leader_wd <= 0 || rule_wd <= 0 {
            return cur_h + rule_wd;
        }
        // Compensates for floating-point rounding.
        rule_wd += 10;
        let edge = cur_h + rule_wd;
        let mut lx = 0;
        if kind == 0 {
            let save_h = cur_h;
            cur_h = left_edge + leader_wd * ((cur_h - left_edge) / leader_wd);
            if cur_h < save_h {
                cur_h += leader_wd;
            }
        } else {
            let lq = rule_wd / leader_wd;
            let lr = rule_wd % leader_wd;
            if kind == 1 {
                cur_h += lr / 2;
            } else {
                lx = lr / (lq + 1);
                cur_h += (lr - (lq - 1) * lx) / 2;
            }
        }
        let outer = std::mem::replace(&mut self.leaders, true);
        while cur_h + leader_wd <= edge {
            if leader.list.0 != leader.list.1 {
                self.box_out(&leader, cur_h, base_line + leader.shift);
            }
            cur_h += leader_wd + lx;
        }
        self.leaders = outer;
        edge - 10
    }

    /// §630 `vlist_out` of box `b` with its reference point at (`cur_h`, `cur_v`).
    fn vlist(&mut self, b: &BoxRec, cur_h: i32, cur_v: i32) {
        let (g_order, g_sign) = (b.glue_order, b.glue_sign);
        let left_edge = cur_h;
        let mut cur_v = cur_v - b.height;
        let top_edge = cur_v;
        self.cur_s += 1;
        let (mut cur_g, mut cur_glue) = (0i32, 0f64);
        for n in self.nodes.get(b.list) {
            match n.kind {
                // §632
                Kind::Box => {
                    let inner = self.nodes.boxes[n.a as usize];
                    if inner.list.0 != inner.list.1 {
                        self.box_out(&inner, left_edge + inner.shift, cur_v + inner.height);
                    }
                    cur_v += inner.height + inner.depth;
                }
                // §633
                Kind::Rule => {
                    let [w, h, d] = self.nodes.rules[n.a as usize];
                    let ht = h + d;
                    let w = if w == RUNNING { b.width } else { w };
                    cur_v += ht;
                    if ht > 0 && w > 0 {
                        self.rule(left_edge, cur_v, w, ht);
                    }
                }
                // §634
                Kind::Glue => {
                    let g = self.nodes.glue(n);
                    let mut rule_ht = g.width - cur_g;
                    stretched(&g, g_sign, g_order, b.glue_set, &mut cur_g, &mut cur_glue);
                    rule_ht += cur_g;
                    if n.sub >= 110 {
                        let [w, _, _] = self.nodes.rules[n.b as usize];
                        let w = if w == RUNNING { b.width } else { w };
                        cur_v += rule_ht;
                        if rule_ht > 0 && w > 0 {
                            self.rule(left_edge, cur_v, w, rule_ht);
                        }
                        continue;
                    } else if n.sub >= 100 {
                        cur_v = self.v_leaders(n.sub - 100, n.b as u32, rule_ht, cur_v, left_edge, top_edge);
                        continue;
                    }
                    cur_v += rule_ht;
                }
                Kind::Kern => cur_v += n.a,
                Kind::Whatsit => match n.sub {
                    // `Output a Image node in a vlist`.
                    what::XIMAGE => {
                        let [_, ht, dp] = self.nodes.rules[n.a as usize];
                        cur_v += ht + dp;
                        self.image(n, left_edge, cur_v);
                    }
                    doc::DEST => self.do_dest(n.a as usize, b, left_edge, top_edge + b.height, left_edge, cur_v),
                    doc::START_LINK | doc::END_LINK => {}
                    doc::REF_OBJ => self.doc.want_obj(n.a),
                    _ => self.whatsit(n, left_edge, cur_v),
                },
                _ => {}
            }
        }
        self.cur_s -= 1;
    }

    /// §635: vertical leaders; returns `cur_v` after them.
    fn v_leaders(&mut self, kind: u8, lb: u32, mut rule_ht: i32, mut cur_v: i32, left_edge: i32, top_edge: i32) -> i32 {
        let leader = self.nodes.boxes[lb as usize];
        let leader_ht = leader.height + leader.depth;
        if leader_ht <= 0 || rule_ht <= 0 {
            return cur_v + rule_ht;
        }
        rule_ht += 10;
        let edge = cur_v + rule_ht;
        let mut lx = 0;
        if kind == 0 {
            let save_v = cur_v;
            cur_v = top_edge + leader_ht * ((cur_v - top_edge) / leader_ht);
            if cur_v < save_v {
                cur_v += leader_ht;
            }
        } else {
            let lq = rule_ht / leader_ht;
            let lr = rule_ht % leader_ht;
            if kind == 1 {
                cur_v += lr / 2;
            } else {
                lx = lr / (lq + 1);
                cur_v += (lr - (lq - 1) * lx) / 2;
            }
        }
        let outer = std::mem::replace(&mut self.leaders, true);
        while cur_v + leader_ht <= edge {
            if leader.list.0 != leader.list.1 {
                self.box_out(&leader, left_edge + leader.shift, cur_v + leader.height);
            }
            cur_v += leader_ht + lx;
        }
        self.leaders = outer;
        edge - 10
    }

    fn box_out(&mut self, b: &BoxRec, h: i32, v: i32) {
        if b.vertical {
            self.vlist(b, h, v);
        } else {
            self.hlist(b, h, v);
        }
    }
}

impl PacketOut for Walk<'_> {
    /// pdfTeX's `output_one_char`: a virtual font's character typesets its packet; either
    /// way the character's own width is the advance.
    fn packet_char(&mut self, f: u16, c: u8, h: i32, v: i32, depth: u8) -> i32 {
        let font = &self.fonts[f as usize];
        let Some([w, _, _, _]) = font.metrics.dims(c, font.size) else { return 0 };
        let vf = self.vf;
        match vf.get(f) {
            Some(packets) => packets.typeset(font.size, c, h, v, depth, self),
            None => self.real_char(f, c, w, h, v),
        }
        w
    }

    fn packet_rule(&mut self, h: i32, v: i32, w: i32, ht: i32) {
        self.rule(h, v, w, ht);
    }
}

/// §625/§634: the glue set so far, rounded, after glue `g` (`cur_g`, from the running total
/// `cur_glue` so that rounding errors do not accumulate).
#[inline]
fn stretched(g: &Glue, sign: Sign, order: u8, set: f64, cur_g: &mut i32, cur_glue: &mut f64) {
    match sign {
        Sign::Stretching if g.stretch_order == order => {
            *cur_glue += g.stretch as f64;
            *cur_g = zround(vet_glue(set * *cur_glue));
        }
        Sign::Shrinking if g.shrink_order == order => {
            *cur_glue -= g.shrink as f64;
            *cur_g = zround(vet_glue(set * *cur_glue));
        }
        _ => {}
    }
}

impl Machine<'_, '_> {
    /// pdfTeX's `pdf_ship_out` of box `b` as a page: its size, origin and content stream.
    pub(super) fn ship_page(&mut self, b: u32) {
        let dimen = |m: &Self, slot| m.dimen_param(slot);
        let h_offset = dimen(self, PDF_H_ORIGIN) + dimen(self, H_OFFSET);
        let v_offset = dimen(self, PDF_V_ORIGIN) + dimen(self, V_OFFSET);
        let bx = self.e.nodes.boxes[b as usize];
        let width = match dimen(self, PDF_PAGE_WIDTH) {
            0 => bx.width + 2 * h_offset,
            w => w,
        };
        let height = match dimen(self, PDF_PAGE_HEIGHT) {
            0 => bx.height + bx.depth + 2 * v_offset,
            h => h,
        };
        self.check_virtual_fonts(b);
        let mut base = std::mem::take(&mut self.page.font_base);
        let mut colors = std::mem::take(&mut self.colors);
        let mut pdf_doc = std::mem::take(&mut self.pdf_doc);
        // `pdf_last_page`: the page's object, made now unless a `goto page` asked for it.
        let page_obj = pdf_doc.page_obj(self.page.pdf.len() as i32 + 1);
        let (link_margin, dest_margin) = (dimen(self, PDF_LINK_MARGIN), dimen(self, PDF_DEST_MARGIN));
        let me = &*self;
        let text_of = move |a: i32, b: i32| {
            let mut s = String::new();
            me.show(me.e.toks.get(super::engine_token::Span { start: a as u32, end: b as u32 }), &mut s);
            s
        };
        let mut walk = Walk { nodes: &self.e.nodes, fonts: &self.e.fonts.list, vf: &self.vf, text: TextState::default(), out: Vec::new(), page_height: height as i64, base: &mut base, names: Vec::new(), colors: &mut colors, text_of: &text_of, doc: &mut pdf_doc, page_obj, cur_s: -1, leaders: false, link_margin, dest_margin, link_stack: Vec::new(), annots: Vec::new(), group: 0, images: Vec::new() };
        // `pdf_out_colorstack_startpage`, at the page's origin.
        for (mode, s) in walk.colors.page_start() {
            walk.literal(mode, &s, 0, height);
        }
        walk.box_out(&bx, h_offset, bx.height + v_offset);
        walk.text.end_text(&mut walk.out);
        let content = walk.out;
        let annots = std::mem::take(&mut walk.annots);
        let group = walk.group;
        let images = std::mem::take(&mut walk.images);
        for &(obj, link, rect) in &annots {
            pdf_doc.write_link(link, obj, rect, height);
        }
        self.page.font_base = base;
        self.colors = colors;
        self.pdf_doc = pdf_doc;
        self.page.pdf.push(ShippedPage { id: page_obj, width, height, content, annots: annots.iter().map(|a| a.0).collect(), group, images });
    }

    /// The document so far as a PDF: the shipped pages and the fonts they use.
    pub fn pdf(&self) -> Vec<u8> {
        let fonts: Vec<Option<crate::backend::backend_ship::DocFont>> = self
            .e
            .fonts
            .list
            .iter()
            .enumerate()
            .map(|(i, f)| {
                // Only a font with its own PDF object is written; the others name it.
                let own = self.page.font_base.get(i) == Some(&(i as u16));
                own.then(|| crate::backend::backend_ship::DocFont {
                    name: f.name.clone(),
                    size: f.size as i64,
                    widths: std::array::from_fn(|c| f.metrics.dims(c as u8, f.size).map_or(0, |d| d[0])),
                })
            })
            .collect();
        let none = |_: &str| None;
        let files: &dyn Fn(&str) -> Option<Vec<u8>> = match &self.read_file {
            Some(f) => f.as_ref(),
            None => &none,
        };
        // `\pdfgentounicode`: ToUnicode maps from glyph names, through `\pdfglyphtounicode`.
        let gen = self.e.eqtb.word(super::engine_eqtb::Reg { region: super::engine_eqtb::Region::Int, slot: PDF_GEN_TO_UNICODE }) > 0;
        let lookup = |n: &str| self.e.glyph_unicode.unicode_of(n);
        let to_unicode: crate::backend::backend_ship::ToUnicode = if gen { Some(&lookup) } else { None };
        let minor = self.e.eqtb.word(super::engine_eqtb::Reg { region: super::engine_eqtb::Region::Int, slot: PDF_MINOR_VERSION });
        let first_page = self.page.pdf.first().map_or(0, |p| p.id);
        let (objects, catalog, base) = self.pdf_doc.finish(first_page, files, self.image_hicolor());
        let meta = crate::backend::backend_ship::DocMeta { minor, info: &self.pdf_doc.info, catalog: &catalog, objects: &objects, base: base as usize };
        crate::backend::backend_ship::write(&self.page.pdf, &fonts, files, to_unicode, &meta)
    }
}
