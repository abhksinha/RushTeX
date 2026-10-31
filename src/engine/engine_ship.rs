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
use super::engine_microtype::Micro;
use super::engine_node::{BoxRec, Kind, Nodes, Sign, RUNNING};
use super::engine_run::Machine;
use super::engine_color::{ColorStacks, SET_ORIGIN};
use super::engine_node::Node;
use super::engine_pdfdoc::{self as doc, PdfDoc};
use super::engine_vf::{PacketOut, Vf, VfTable};
use super::engine_whatsit as what;
use crate::backend::backend_pdftex::TextState;
pub use crate::backend::backend_ship::{ShippedForm, ShippedPage};

const H_OFFSET: u32 = 18;
const V_OFFSET: u32 = 19;
const PDF_H_ORIGIN: u32 = 21;
const PDF_V_ORIGIN: u32 = 22;
const PDF_PAGE_WIDTH: u32 = 23;
const PDF_PAGE_HEIGHT: u32 = 24;
const PDF_GEN_TO_UNICODE: u32 = 81;
const PDF_MINOR_VERSION: u32 = 67;
const PDF_OBJ_COMPRESS_LEVEL: u32 = 57;
const PDF_LINK_MARGIN: u32 = 25;
const PDF_DEST_MARGIN: u32 = 26;
/// `\pdfdecimaldigits` and `\pdfpkresolution` (integer slots).
const PDF_DECIMAL_DIGITS: u32 = 58;
const PDF_PK_RESOLUTION: u32 = 61;

/// `pdf_font_num` of a font not yet shipped.
const NOT_USED: u16 = u16::MAX;

/// The font whose PDF object `f` shares, fixed at `f`'s first use (pdftex.web
/// `pdf_init_font`): the earlier-used font of the same file name at another size, else `f`
/// itself; an expanded copy shares its base's (which is set up first). That font's size
/// gives the shared `/Widths`. Searched once per font.
fn font_base(base: &mut Vec<u16>, fonts: &[Font], micro: &Micro, pk: &[i64], f: u16) -> u16 {
    if base.len() < fonts.len() {
        base.resize(fonts.len(), NOT_USED);
    }
    if base[f as usize] == NOT_USED && fonts[f as usize].expand != 0 {
        let b = font_base(base, fonts, micro, pk, micro.expand(f).blink);
        base[f as usize] = b;
    }
    // A bitmap font shares with no other size (pdftex.web `pdf_init_font`: `isscalable`).
    if base[f as usize] == NOT_USED && pk.get(f as usize).is_some_and(|&s| s != 0) {
        base[f as usize] = f;
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
    /// `Fonts::info`: the fonts' scaled values.
    info: &'a [i32],
    micro: &'a Micro,
    vf: &'a VfTable,
    text: TextState,
    out: Vec<u8>,
    page_height: i64,
    /// The font each font shares its PDF object with (`NOT_USED` before its first use), the
    /// characters used of each, and each font's PK scale (0 for a scalable font).
    base: &'a mut Vec<u16>,
    used: &'a mut Vec<[u64; 4]>,
    pk: &'a [i64],
    word_ship: &'a mut WordShip,
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
    /// The forms it shows (`pdf_xform_list`), by resource number and object.
    forms: Vec<(u32, i32)>,
    /// Where each `\pdfsavepos` of a page is, from its lower left corner (none in forms).
    positions: Vec<(i32, i32)>,
    /// utils.c `matrix_stack` (each `\pdfsetmatrix` composed with those before it, as
    /// a b c d e f), the matrix depth at each open `\pdfsave` (`pos_stack`), and the last
    /// rectangle transformed (`last_llx` ...), all for pages only.
    matrices: Vec<[f64; 6]>,
    saves: Vec<usize>,
    last_rect: [i32; 4],
}

/// Each word entry the ship has written, as its runs of characters: a run is the shift
/// (font kerns) before its first character, that character, and the characters after it
/// up to the next kern. A run's first character goes through `TextState::char_ex`, which
/// moves the viewer after a space; the rest are bytes ready to copy when
/// `TextState::run` finds they would all take `char_ex`'s fast path. Made the first time
/// an entry is shipped in a font at a size; entries never change, so it holds.
#[derive(PartialEq, Clone, Debug, Default)]
pub struct WordShip {
    /// Per entry id: its record, or none yet.
    recs: Vec<ShipRec>,
    runs: Vec<Run>,
    /// The runs' characters, raw (for a run `TextState::run` refuses) and as written.
    chars: Vec<u8>,
    bytes: Vec<u8>,
    /// Per entry id: the virtual font it was asked for and [`Walk::direct_font`]'s answer
    /// (0 not asked yet, 1 none, else the real font plus 2).
    direct: Vec<(u16, u16)>,
    /// Per entry id: the hash of its items for a session's shipped-page keys
    /// (`Machine::ship_key`), 0 not made yet.
    keys: Vec<u128>,
}

/// An entry's runs, `runs[start..end]`, made for font `font` at `pdf_font_size` `size`,
/// and the characters it uses (`pdf_char_marked`).
#[derive(PartialEq, Debug, Clone, Copy, Default)]
struct ShipRec {
    font: u16,
    made: bool,
    start: u32,
    end: u32,
    size: i64,
    used: [u64; 4],
}

/// One run, read whole.
#[derive(PartialEq, Debug, Clone, Copy)]
struct Run {
    shift: i32,
    first: u8,
    first_w: i32,
    /// The characters after the first: TeX's width, the viewer's advance, the drift range,
    /// and their ranges in `chars` and `bytes`.
    width: i32,
    adv: i64,
    drift: [i64; 2],
    chars: (u32, u32),
    bytes: (u32, u32),
}

impl WordShip {
    /// Entry `id`'s record for font `f` at `size`, made from `items` (their codes through
    /// virtual font `map`'s packets, when it has one) if it has none.
    /// `None` when it was made for another font or size (the ship then writes the word
    /// character by character).
    #[allow(clippy::too_many_arguments)]
    fn get(&mut self, id: u32, f: u16, size: i64, items: &[Node], font: &Font, info: &[i32], map: Option<&Vf>) -> Option<ShipRec> {
        let id = id as usize;
        if self.recs.len() <= id {
            self.recs.resize(id + 1, ShipRec::default());
        }
        let r = self.recs[id];
        if r.made {
            return (r.font == f && r.size == size).then_some(r);
        }
        let start = self.runs.len() as u32;
        let mut used = [0u64; 4];
        let mut shift = 0;
        let mut open = false;
        for m in items {
            match m.kind {
                Kind::Char | Kind::Lig => {
                    let c = remap(map, m.a as u8);
                    let Some([w, _, _, _]) = font.dims(c, info) else { continue };
                    used[c as usize / 64] |= 1 << (c % 64);
                    if !open {
                        let (cs, bs) = (self.chars.len() as u32, self.bytes.len() as u32);
                        self.runs.push(Run { shift, first: c, first_w: w, width: 0, adv: 0, drift: [0, 0], chars: (cs, cs), bytes: (bs, bs) });
                        shift = 0;
                        open = true;
                        continue;
                    }
                    let run = self.runs.last_mut().expect("an open run");
                    // The drift before this character, then its own width less its advance.
                    let d = run.width as i64 - run.adv;
                    run.drift = [run.drift[0].min(d), run.drift[1].max(d)];
                    run.width += w;
                    run.adv += crate::backend::backend_pdftex::divide_scaled(w as i64, size, 4).1;
                    self.chars.push(c);
                    crate::backend::backend_pdftex::print_char(&mut self.bytes, c);
                    run.chars.1 = self.chars.len() as u32;
                    run.bytes.1 = self.bytes.len() as u32;
                }
                Kind::Kern => {
                    shift += m.a;
                    open = false;
                }
                _ => {}
            }
        }
        // A trailing shift has no character to carry it: a run without one (`first_w` is
        // `i32::MIN`) keeps it.
        if shift != 0 {
            let (cs, bs) = (self.chars.len() as u32, self.bytes.len() as u32);
            self.runs.push(Run { shift, first: 0, first_w: i32::MIN, width: 0, adv: 0, drift: [0, 0], chars: (cs, cs), bytes: (bs, bs) });
        }
        let r = ShipRec { font: f, made: true, start, end: self.runs.len() as u32, size, used };
        self.recs[id] = r;
        Some(r)
    }
}

/// Character `c` of a word, through virtual font `map`'s packets when it has one.
fn remap(map: Option<&Vf>, c: u8) -> u8 {
    map.and_then(|vf| vf.direct(c)).map_or(c, |d| d.1)
}

/// What a walk of a page or form box leaves: its content, link annotations, transparency
/// group, images and forms.
struct Walked {
    content: Vec<u8>,
    annots: Vec<(i32, usize, [i32; 4])>,
    group: i32,
    images: Vec<(u32, i32)>,
    forms: Vec<(u32, i32)>,
    positions: Vec<(i32, i32)>,
}

/// `set_rect_dimens`: the rectangle of an annotation or destination with dimensions `dims`
/// (running ones from the parent box at (x, y)) begun at (`cur_h`, `cur_v`).
fn rect_dimens(dims: [Option<i32>; 3], parent: &BoxRec, x: i32, y: i32, cur_h: i32, cur_v: i32) -> [i32; 4] {
    let right = dims[0].map_or(x + parent.width, |w| cur_h + w);
    let top = dims[1].map_or(y - parent.height, |h| cur_v - h);
    let bottom = dims[2].map_or(y + parent.depth, |d| cur_v + d);
    [cur_h, top, right, bottom]
}

/// utils.c `do_matrixtransform`: point (x, y) through matrix `m`, rounded half away from 0.
fn transform(m: &[f64; 6], x: i32, y: i32) -> (i32, i32) {
    let round = |v: f64| if v > 0.0 { (v + 0.5) as i32 } else { (v - 0.5) as i32 };
    let (x, y) = (x as f64, y as f64);
    (round(x * m[0] + y * m[2] + m[4]), round(x * m[1] + y * m[3] + m[5]))
}

impl Walk<'_> {
    /// `set_rect_dimens`' end: `raw` (left, top, right, bottom) through the current
    /// `\pdfsetmatrix` on a page, then widened by `margin`.
    fn rect(&mut self, raw: [i32; 4], margin: i32) -> [i32; 4] {
        let [l, t, r, b] = if self.matrices.is_empty() { raw } else { self.transform_rect(raw) };
        [l - margin, t - margin, r + margin, b + margin]
    }

    /// utils.c `matrixtransformrect` in TeX's page coordinates (down from the top): the
    /// box around the four corners, the PDF corners kept for `matrixrecalculate`.
    fn transform_rect(&mut self, [l, t, r, b]: [i32; 4]) -> [i32; 4] {
        let ph = self.page_height as i32;
        let (llx, lly, urx, ury) = (l, ph - b, r, ph - t);
        self.last_rect = [llx, lly, urx, ury];
        let m = self.matrices[self.matrices.len() - 1];
        let p = [transform(&m, llx, lly), transform(&m, llx, ury), transform(&m, urx, lly), transform(&m, urx, ury)];
        let min_x = p.iter().map(|q| q.0).min().unwrap_or(0);
        let min_y = p.iter().map(|q| q.1).min().unwrap_or(0);
        let max_x = p.iter().map(|q| q.0).max().unwrap_or(0);
        let max_y = p.iter().map(|q| q.1).max().unwrap_or(0);
        [min_x, ph - max_y, max_x, ph - min_y]
    }

    /// `Save current position to pdf_last_x_pos, pdf_last_y_pos`, on a page and outside
    /// leaders (whose `\write`s TeX skips, so the positions stay in step with them).
    fn save_pos(&mut self, h: i32, v: i32) {
        if self.page_obj != 0 && !self.leaders {
            self.positions.push((h, (self.page_height - v as i64) as i32));
        }
    }

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
            what::PGF_POINT => Some((SET_ORIGIN, what::pgf_point_text(n, h, (self.page_height - v as i64) as i32))),
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
                let mut k = [0f64; 4];
                let mut words = s.split_whitespace();
                let ok = k.iter_mut().all(|x| words.next().and_then(|w| w.parse().ok()).map(|v| *x = v).is_some()) && words.next().is_none();
                if ok && self.page_obj != 0 {
                    // utils.c `pdfsetmatrix`: about the current point, after the matrix in force.
                    let (x, y) = (h as f64, (self.page_height - v as i64) as f64);
                    let [a, b, c, d] = k;
                    let (e, f) = (x * (1.0 - a) - y * c, y * (1.0 - d) - x * b);
                    let m = match self.matrices.last() {
                        Some(t) => [a * t[0] + b * t[2], a * t[1] + b * t[3], c * t[0] + d * t[2], c * t[1] + d * t[3], e * t[0] + f * t[2] + t[4], e * t[1] + f * t[3] + t[5]],
                        None => [a, b, c, d, e, f],
                    };
                    self.matrices.push(m);
                }
                ok.then(|| (SET_ORIGIN, format!("{s} 0 0 cm")))
            }
            // `checkpdfsave`, `checkpdfrestore`: a restore drops the matrices set since.
            what::SAVE => {
                if self.page_obj != 0 {
                    self.saves.push(self.matrices.len());
                }
                Some((SET_ORIGIN, "q".to_string()))
            }
            what::RESTORE => {
                if let Some(k) = self.saves.pop().filter(|_| self.page_obj != 0) {
                    self.matrices.truncate(k);
                }
                Some((SET_ORIGIN, "Q".to_string()))
            }
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
        let ea = self.real_font(f);
        let base = self.base[f as usize];
        self.used[base as usize][c as usize / 64] |= 1 << (c % 64);
        self.text.char_ex(&mut self.out, c, w as i64, h as i64, self.page_height - v as i64, ea);
    }

    /// A word node's characters and font kerns from `h` on the baseline `v`, all of real
    /// font `f`: the font is made current once. Returns the width written. A word of a
    /// virtual font whose packets each set one character of `f` as wide (`direct_font`)
    /// comes with `map`, its codes then taken through the packets.
    fn real_word(&mut self, f: u16, id: u32, h: i32, v: i32, map: Option<&Vf>) -> i32 {
        let ea = self.real_font(f);
        let base = self.base[f as usize] as usize;
        let font = &self.fonts[f as usize];
        let items = self.nodes.words.items(id);
        let y = self.page_height - v as i64;
        let mut x = h;
        // Runs of an entry already written: each run's first character as `char_ex`
        // writes it, the rest copied when they all take its fast path.
        if ea == 0 && self.text.pk_scale == 0 {
            if let Some(r) = self.word_ship.get(id, f, self.text.font_size(), items, font, self.info, map) {
                for (u, m) in self.used[base].iter_mut().zip(r.used) {
                    *u |= m;
                }
                let ws = &*self.word_ship;
                for run in &ws.runs[r.start as usize..r.end as usize] {
                    x += run.shift;
                    if run.first_w == i32::MIN {
                        continue;
                    }
                    self.text.char_ex(&mut self.out, run.first, run.first_w as i64, x as i64, y, 0);
                    x += run.first_w;
                    let bytes = &ws.bytes[run.bytes.0 as usize..run.bytes.1 as usize];
                    if bytes.is_empty() || self.text.run(&mut self.out, bytes, x as i64, y, run.drift, run.adv) {
                        x += run.width;
                        continue;
                    }
                    for &c in &ws.chars[run.chars.0 as usize..run.chars.1 as usize] {
                        let w = font.dims(c, self.info).map_or(0, |d| d[0]);
                        self.text.char_ex(&mut self.out, c, w as i64, x as i64, y, 0);
                        x += w;
                    }
                }
                return x - h;
            }
        }
        for m in items {
            match m.kind {
                Kind::Char | Kind::Lig => {
                    let c = remap(map, m.a as u8);
                    let Some([w, _, _, _]) = font.dims(c, self.info) else { continue };
                    self.used[base][c as usize / 64] |= 1 << (c % 64);
                    self.text.char_ex(&mut self.out, c, w as i64, x as i64, y, ea);
                    x += w;
                }
                Kind::Kern => x += m.a,
                _ => {}
            }
        }
        x - h
    }

    /// The real font every character of word `items` (of virtual font `f`) is, when each
    /// packet only sets one character of that font (`Vf::direct`) as wide as the virtual
    /// character: `typeset` then writes exactly that character at the reference point and
    /// the advance is the same width, so the word ships as a word of the real font.
    fn direct_font(&self, f: u16, items: &[Node]) -> Option<u16> {
        let vf = self.vf.get(f)?;
        let font = &self.fonts[f as usize];
        if font.expand != 0 {
            return None;
        }
        let mut real = None;
        for m in items {
            if !matches!(m.kind, Kind::Char | Kind::Lig) {
                continue;
            }
            let c = m.a as u8;
            let (g, d) = vf.direct(c)?;
            if m.font != f || g == 0 || *real.get_or_insert(g) != g || self.vf.get(g).is_some() {
                return None;
            }
            if self.fonts[g as usize].dims(d, self.info)?[0] != font.dims(c, self.info)?[0] {
                return None;
            }
        }
        real
    }

    /// pdfTeX's `pdf_set_font` for real font `f`, and its automatic expansion (thousandths)
    /// for the text matrix.
    fn real_font(&mut self, f: u16) -> i64 {
        let font = &self.fonts[f as usize];
        if self.names.len() <= f as usize {
            self.names.resize(f as usize + 1, String::new());
        }
        let base = font_base(self.base, self.fonts, self.micro, self.pk, f);
        if self.used.len() <= base as usize {
            self.used.resize(base as usize + 1, [0; 4]);
        }
        if self.names[f as usize].is_empty() {
            self.names[f as usize] = format!("/F{base}");
        }
        // pdf_set_font: `Tf` only when the shared font or the size changes.
        self.text.set_font(&mut self.out, base as u32, &self.names[f as usize], font.size as i64);
        // An expanded copy is scalable as its base is (the copy's own name has no map entry).
        self.text.pk_scale = self.pk.get(base as usize).copied().unwrap_or(0);
        // An automatically expanded copy is its base drawn through a scaled text matrix.
        if font.expand != 0 && self.micro.expand(f).auto { font.expand as i64 } else { 0 }
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
        match &im.pdf {
            // `out_image` of a PDF page: its form scaled from the box's size, placed by the
            // box's corner.
            Some(p) => {
                let [iw, ih] = if p.rotate == 90 || p.rotate == 270 { [p.size[1], p.size[0]] } else { p.size };
                let (orig, k) = (p.orig, n.b as usize + 1);
                self.text.form(&mut self.out, k, h as i64, v as i64, self.page_height, [w as i64, ht as i64 + dp as i64], [iw as i64, ih as i64], [orig[0] as i64, orig[1] as i64]);
            }
            None => self.text.image(&mut self.out, n.b as usize + 1, h as i64, v as i64, self.page_height, [w as i64, ht as i64 + dp as i64]),
        }
    }

    /// `out_form` with the form's lower left corner at TeX's (h, v).
    fn form(&mut self, n: &Node, h: i32, v: i32) {
        let entry = (n.b as u32 + 1, self.doc.forms[n.b as usize].obj);
        if !self.forms.contains(&entry) {
            self.forms.push(entry);
        }
        self.text.form_ref(&mut self.out, n.b as usize + 1, h as i64, v as i64, self.page_height);
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
                let rect = self.rect(rect_dimens(self.doc.link_dims(link), b, left_edge, base_line, left_edge, base_line), self.link_margin);
                self.link_stack[i].2 = self.annots.len();
                self.annots.push((obj, link, rect));
            }
        }
        let (mut cur_g, mut cur_glue) = (0i32, 0f64);
        for n in self.nodes.get(b.list) {
            match n.kind {
                Kind::Char => cur_h += self.char(n.font, n.a as u8, cur_h, base_line),
                Kind::Lig => cur_h += self.char(n.font, n.a as u8, cur_h, base_line),
                // A word: its characters and font kerns.
                Kind::Word if self.vf.get(n.font).is_none() => cur_h += self.real_word(n.font, n.a as u32, cur_h, base_line, None),
                Kind::Word => {
                    let nodes = self.nodes;
                    // Entries never change, so the answer for an entry is kept.
                    let id = n.a as usize;
                    if self.word_ship.direct.len() <= id {
                        self.word_ship.direct.resize(id + 1, (0, 0));
                    }
                    let direct = match self.word_ship.direct[id] {
                        (f, 1) if f == n.font => None,
                        (f, k) if f == n.font && k >= 2 => Some(k - 2),
                        _ => {
                            let g = self.direct_font(n.font, nodes.word_items(n));
                            self.word_ship.direct[id] = (n.font, g.map_or(1, |g| g + 2));
                            g
                        }
                    };
                    if let Some(g) = direct {
                        let vf = self.vf;
                        cur_h += self.real_word(g, n.a as u32, cur_h, base_line, vf.get(n.font));
                    } else {
                        for m in nodes.word_items(n) {
                            match m.kind {
                                Kind::Char | Kind::Lig => cur_h += self.char(m.font, m.a as u8, cur_h, base_line),
                                Kind::Kern => cur_h += m.a,
                                _ => {}
                            }
                        }
                    }
                }
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
                    // pdfTeX's `Output a Image node in a hlist`, and a Form node.
                    what::XIMAGE => {
                        self.image(n, cur_h, base_line + self.nodes.rules[n.a as usize][2]);
                        cur_h += self.nodes.rules[n.a as usize][0];
                    }
                    what::XFORM => {
                        self.form(n, cur_h, base_line + self.nodes.rules[n.a as usize][2]);
                        cur_h += self.nodes.rules[n.a as usize][0];
                    }
                    doc::START_LINK => self.do_link(n.a as usize, b, left_edge, base_line, cur_h),
                    doc::END_LINK => self.end_link(cur_h),
                    doc::DEST => self.do_dest(n.a as usize, b, left_edge, base_line, cur_h, base_line),
                    doc::REF_OBJ => self.doc.want_obj(n.a),
                    what::SAVE_POS => self.save_pos(cur_h, base_line),
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
        let rect = self.rect(rect_dimens(self.doc.link_dims(link), b, x, y, cur_h, y), self.link_margin);
        self.link_stack.push((self.cur_s, link, self.annots.len()));
        self.annots.push((obj, link, rect));
    }

    fn end_link(&mut self, cur_h: i32) {
        let Some((_, link, annot)) = self.link_stack.pop() else { return };
        if self.doc.link_dims(link)[0].is_none() {
            if self.matrices.is_empty() {
                self.annots[annot].2[2] = cur_h + self.link_margin;
            } else {
                // `matrixrecalculate`: the last rectangle transformed, with this right edge.
                let ph = self.page_height as i32;
                let [l, b, _, t] = self.last_rect;
                let [l, t, r, b] = self.transform_rect([l, ph - t, cur_h + self.link_margin, ph - b]);
                let m = self.link_margin;
                self.annots[annot].2 = [l - m, t - m, r + m, b + m];
            }
        }
    }

    /// `do_dest` at (`cur_h`, `cur_v`) in box `b` whose reference point is (x, y).
    fn do_dest(&mut self, dest: usize, b: &BoxRec, x: i32, y: i32, cur_h: i32, cur_v: i32) {
        if self.leaders {
            return;
        }
        // Under `\pdfsetmatrix` every view takes its place from the transformed rectangle
        // (`pdf_left`, `pdf_top`); otherwise only `/FitR` has one.
        if !self.matrices.is_empty() {
            let rect = self.rect(rect_dimens(self.doc.dest_dims(dest), b, x, y, cur_h, cur_v), self.dest_margin);
            return self.doc.place_dest(dest, self.page_obj, rect[0], rect[1], rect, self.page_height as i32);
        }
        let rect = if self.doc.dest_is_fitr(dest) { self.rect(rect_dimens(self.doc.dest_dims(dest), b, x, y, cur_h, cur_v), self.dest_margin) } else { [0; 4] };
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
                    what::XFORM => {
                        let [_, ht, dp] = self.nodes.rules[n.a as usize];
                        cur_v += ht + dp;
                        self.form(n, left_edge, cur_v);
                    }
                    doc::DEST => self.do_dest(n.a as usize, b, left_edge, top_edge + b.height, left_edge, cur_v),
                    doc::START_LINK | doc::END_LINK => {}
                    doc::REF_OBJ => self.doc.want_obj(n.a),
                    what::SAVE_POS => self.save_pos(left_edge, cur_v),
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
        let Some([w, _, _, _]) = font.dims(c, self.info) else { return 0 };
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
    /// Returns where its `\pdfsavepos`s are.
    pub(super) fn ship_page(&mut self, b: u32) -> Vec<(i32, i32)> {
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
        // A session's cache (`backend_cache`) keys the page by everything its walk reads; a
        // page whose walk would touch the document's objects (links, destinations, images,
        // forms, `\pdfrefobj`, `\pdfsavepos`) has no key.
        let key = self.pdf_cache.and_then(|_| {
            self.classify_fonts();
            self.ship_key(&bx, [width, height, h_offset, v_offset])
        });
        // `pdf_last_page`: the page's object, made now unless a `goto page` asked for it.
        let page_obj = self.pdf_doc.page_obj(self.page.pdf.len() as i32 + 1);
        if let (Some(c), Some(k)) = (self.pdf_cache, key) {
            if let Some(d) = c.page(k) {
                let used = &mut self.page.font_used;
                if used.len() < d.used.len() {
                    used.resize(d.used.len(), [0; 4]);
                }
                for (u, o) in used.iter_mut().zip(&d.used) {
                    u.iter_mut().zip(o).for_each(|(u, o)| *u |= o);
                }
                self.page.font_base.clone_from(&d.base);
                self.colors = d.colors.clone();
                self.page.pdf.push(ShippedPage { id: page_obj, width, height, content: d.content.clone(), ..Default::default() });
                return Vec::new();
            }
        }
        // With a key, the walk marks the characters it uses in an empty table, kept as the
        // page's own and added to the document's.
        let live = key.map(|_| {
            let n = self.page.font_used.len();
            std::mem::replace(&mut self.page.font_used, vec![[0; 4]; n])
        });
        let w = self.walk_box(bx, Some(page_obj), height, h_offset, bx.height + v_offset);
        if let (Some(mut live), Some(c), Some(k)) = (live, self.pdf_cache, key) {
            let own = std::mem::take(&mut self.page.font_used);
            if live.len() < own.len() {
                live.resize(own.len(), [0; 4]);
            }
            for (u, o) in live.iter_mut().zip(&own) {
                u.iter_mut().zip(o).for_each(|(u, o)| *u |= o);
            }
            self.page.font_used = live;
            c.put_page(k, crate::backend::backend_cache::ShippedDone { content: w.content.clone(), used: own, base: self.page.font_base.clone(), colors: self.colors.clone() });
        }
        for &(obj, link, rect) in &w.annots {
            self.pdf_doc.write_link(link, obj, rect, height);
        }
        self.ship_forms(&w.forms);
        let annots = w.annots.iter().map(|a| a.0).collect();
        self.page.pdf.push(ShippedPage { id: page_obj, width, height, content: w.content, annots, group: w.group, images: w.images, forms: w.forms });
        w.positions
    }

    /// A session's key of shipping box `bx` as a page `dims` (width, height, and the two
    /// offsets): a hash of everything the walk reads, by content, never by arena index. The
    /// fonts (name, sizes, expansion, every character's dimensions, once per font), the
    /// sharing of PDF fonts so far, PK scales, virtual fonts, the colour stacks, and the
    /// box's whole tree with its whatsits' texts. `None` when the walk would touch the
    /// document's objects or positions.
    fn ship_key(&mut self, bx: &BoxRec, dims: [i32; 4]) -> Option<u128> {
        use crate::backend::backend_hash::ContentHasher;
        let fonts = &self.e.fonts;
        let (mut n, mut fp) = self.page.ship_fonts;
        while n < fonts.list.len() {
            let f = &fonts.list[n];
            let mut h = ContentHasher::new();
            h.u64(fp as u64);
            h.u64((fp >> 64) as u64);
            h.bytes(f.name.as_bytes());
            h.u64(((f.size as u32 as u64) << 32) | f.design as u32 as u64);
            h.u64(f.expand as u16 as u64);
            for c in 0..=255u8 {
                match f.dims(c, &fonts.info) {
                    Some([w, ht, d, i]) => {
                        h.u64(((w as u32 as u64) << 32) | ht as u32 as u64);
                        h.u64(((d as u32 as u64) << 32) | i as u32 as u64);
                    }
                    None => h.u64(u64::MAX),
                }
            }
            fp = h.finish();
            n += 1;
        }
        self.page.ship_fonts = (n, fp);
        let mut h = ContentHasher::new();
        h.u64(5);
        h.u64(fp as u64);
        h.u64((fp >> 64) as u64);
        h.u64(n as u64);
        for i in 0..n {
            let e = fonts.micro.expand(i as u16);
            let base = self.page.font_base.get(i).copied().unwrap_or(NOT_USED);
            h.u64(((base as u64) << 32) | ((e.blink as u64) << 1) | e.auto as u64);
            h.u64(self.page.font_pk.get(i).copied().unwrap_or(-1) as u64);
            match self.vf.get(i as u16) {
                Some(v) => {
                    h.u64(v.digest as u64);
                    h.u64((v.digest >> 64) as u64);
                }
                None => h.u64(0),
            }
        }
        h.u64(self.page.font_base.len() as u64);
        h.u64(self.page.font_used.len() as u64);
        self.colors.digest(&mut h);
        dims.iter().for_each(|&d| h.u64(d as u32 as u64));
        let mut keys = std::mem::take(&mut self.page.word_ship.keys);
        let ok = self.hash_box(bx, &mut h, &mut keys);
        self.page.word_ship.keys = keys;
        ok.then(|| h.finish())
    }

    /// Box `bx`'s tree into `h` as the walk reads it (`ship_key`); false at a whatsit that
    /// touches the document's objects or positions.
    /// A word entry's items are hashed once (`keys`, by entry id; entries never change).
    fn hash_box(&self, bx: &BoxRec, h: &mut crate::backend::backend_hash::ContentHasher, keys: &mut Vec<u128>) -> bool {
        let nodes = &self.e.nodes;
        h.u64(((bx.vertical as u64) << 16) | ((bx.glue_sign as u64) << 8) | bx.glue_order as u64);
        h.u64(((bx.width as u32 as u64) << 32) | bx.height as u32 as u64);
        h.u64(((bx.depth as u32 as u64) << 32) | bx.shift as u32 as u64);
        h.u64(bx.glue_set.to_bits());
        let list = nodes.get(bx.list);
        h.u64(list.len() as u64);
        let mut text = String::new();
        for n in list {
            let head = ((n.kind as u64) << 56) | ((n.sub as u64) << 48) | ((n.font as u64) << 32);
            match n.kind {
                Kind::Char | Kind::Lig | Kind::Kern | Kind::Math => h.u64(head | n.a as u32 as u64),
                Kind::Box => {
                    h.u64(head);
                    if !self.hash_box(&nodes.boxes[n.a as usize], h, keys) {
                        return false;
                    }
                }
                Kind::Rule => {
                    let r = nodes.rules[n.a as usize];
                    h.u64(head | r[0] as u32 as u64);
                    h.u64(((r[1] as u32 as u64) << 32) | r[2] as u32 as u64);
                }
                Kind::Glue => {
                    let g = nodes.glue(n);
                    h.u64(head | g.width as u32 as u64);
                    h.u64(((g.stretch as u32 as u64) << 32) | g.shrink as u32 as u64);
                    h.u64(((g.stretch_order as u64) << 8) | g.shrink_order as u64);
                    if n.sub >= 110 {
                        let r = nodes.rules[n.b as usize];
                        h.u64(((r[0] as u32 as u64) << 32) | r[1] as u32 as u64);
                        h.u64(r[2] as u32 as u64);
                    } else if n.sub >= 100 && !self.hash_box(&nodes.boxes[n.b as usize], h, keys) {
                        return false;
                    }
                }
                Kind::Word => {
                    let id = n.a as usize;
                    if keys.len() <= id {
                        keys.resize(id + 1, 0);
                    }
                    if keys[id] == 0 {
                        let items = nodes.words.items(id as u32);
                        let mut w = crate::backend::backend_hash::ContentHasher::new();
                        w.u64(items.len() as u64);
                        for m in items {
                            w.u64(((m.kind as u64) << 56) | ((m.font as u64) << 32) | m.a as u32 as u64);
                        }
                        keys[id] = w.finish() | 1;
                    }
                    h.u64(head);
                    h.u64(keys[id] as u64);
                    h.u64((keys[id] >> 64) as u64);
                }
                Kind::Whatsit => {
                    if matches!(n.sub, what::XIMAGE | what::XFORM | doc::START_LINK | doc::END_LINK | doc::DEST | doc::REF_OBJ | what::SAVE_POS) {
                        return false;
                    }
                    h.u64(head);
                    // The whatsits whose text the walk writes (`Walk::whatsit`).
                    if matches!(n.sub, what::LITERAL | what::LATE_LITERAL | what::SPECIAL | what::COLOR_SET | what::COLOR_PUSH | what::SET_MATRIX) {
                        text.clear();
                        self.show(self.e.toks.get(super::engine_token::Span { start: n.a as u32, end: n.b as u32 }), &mut text);
                        h.bytes(text.as_bytes());
                    }
                }
                // Penalties, discretionaries, marks, insertions and adjustments are passed
                // over by the walk.
                _ => h.u64(head & (0xff << 56)),
            }
        }
        true
    }

    /// The forms a page or form showed that are not shipped yet (pdfTeX's `Write out
    /// pending forms`).
    fn ship_forms(&mut self, forms: &[(u32, i32)]) {
        for &(n, _) in forms {
            if !self.pdf_doc.forms[n as usize - 1].shipped {
                self.ship_form(n as usize - 1);
            }
        }
    }

    /// `pdf_ship_out` of form `k`: its box walked from (0, height), the page height its
    /// height plus depth; then the forms it shows.
    pub(super) fn ship_form(&mut self, k: usize) {
        let f = &mut self.pdf_doc.forms[k];
        f.shipped = true;
        let (b, [width, height, depth]) = (f.box_rec, f.dims);
        self.check_virtual_fonts(b);
        let bx = self.e.nodes.boxes[b as usize];
        self.colors.swap_form();
        let w = self.walk_box(bx, None, height + depth, 0, height);
        self.colors.swap_form();
        let f = &self.pdf_doc.forms[k];
        let latin1 = |s: &str| s.chars().map(|c| c as u32 as u8).collect::<Vec<u8>>();
        let shipped = ShippedForm { id: f.obj, width, height: height + depth, content: w.content, images: w.images, forms: w.forms, attr: latin1(&f.attr), resources: latin1(&f.resources) };
        self.ship_forms(&shipped.forms);
        self.pdf_doc.shipped_forms.push(shipped);
    }

    /// pdfTeX's `pdf_hlist_out`/`pdf_vlist_out` of box `bx` with its reference point at
    /// (h, v), on a page (whose object is given; its colour stacks start it) or a form
    /// `height` tall.
    fn walk_box(&mut self, bx: BoxRec, page: Option<i32>, height: i32, h: i32, v: i32) -> Walked {
        let dimen = |m: &Self, slot| m.dimen_param(slot);
        self.classify_fonts();
        let mut base = std::mem::take(&mut self.page.font_base);
        let mut used = std::mem::take(&mut self.page.font_used);
        let pk = std::mem::take(&mut self.page.font_pk);
        let mut word_ship = std::mem::take(&mut self.page.word_ship);
        let mut colors = std::mem::take(&mut self.colors);
        let mut pdf_doc = std::mem::take(&mut self.pdf_doc);
        let (link_margin, dest_margin) = (dimen(self, PDF_LINK_MARGIN), dimen(self, PDF_DEST_MARGIN));
        // The content buffer starts at the last page's size, not empty and doubling.
        let out = Vec::with_capacity(self.page.pdf.last().map_or(4096, |p| p.content.len() + p.content.len() / 4));
        let me = &*self;
        let text_of = move |a: i32, b: i32| {
            let mut s = String::new();
            me.show(me.e.toks.get(super::engine_token::Span { start: a as u32, end: b as u32 }), &mut s);
            s
        };
        let mut walk = Walk { nodes: &self.e.nodes, fonts: &self.e.fonts.list, info: &self.e.fonts.info, micro: &self.e.fonts.micro, vf: &self.vf, text: TextState::default(), out, page_height: height as i64, base: &mut base, used: &mut used, pk: &pk, word_ship: &mut word_ship, names: Vec::new(), colors: &mut colors, text_of: &text_of, doc: &mut pdf_doc, page_obj: page.unwrap_or(0), cur_s: -1, leaders: false, link_margin, dest_margin, link_stack: Vec::new(), annots: Vec::new(), group: 0, images: Vec::new(), forms: Vec::new(), positions: Vec::new(), matrices: Vec::new(), saves: Vec::new(), last_rect: [0; 4] };
        // `pdf_out_colorstack_startpage`, at the page's origin.
        if page.is_some() {
            for (mode, s) in walk.colors.page_start() {
                walk.literal(mode, &s, 0, height);
            }
        }
        walk.box_out(&bx, h, v);
        walk.text.end_text(&mut walk.out);
        let walked = Walked { content: walk.out, annots: walk.annots, group: walk.group, images: walk.images, forms: walk.forms, positions: walk.positions };
        self.page.font_base = base;
        self.page.font_used = used;
        self.page.font_pk = pk;
        self.page.word_ship = word_ship;
        self.colors = colors;
        self.pdf_doc = pdf_doc;
        walked
    }

    /// mapfile.c `isscalable` for the fonts loaded since the last shipout: a font with a map
    /// entry (or one of the built-in programs) is scalable; another is drawn from PK bitmaps
    /// at `\pdfpkresolution`, its scale fixed now.
    fn classify_fonts(&mut self) {
        let n = self.e.fonts.list.len();
        if self.page.font_pk.len() >= n {
            return;
        }
        if self.page.map_names.is_none() {
            let map = self.read_file.as_ref().and_then(|r| r("pdftex.map")).unwrap_or_default();
            self.page.map_names = Some(crate::backend::backend_ship::map_names(&map));
        }
        let int = |m: &Self, slot| m.e.eqtb.word(super::engine_eqtb::Reg { region: super::engine_eqtb::Region::Int, slot });
        let res = match int(self, PDF_PK_RESOLUTION) {
            0 => 600,
            r => r.clamp(72, 8000),
        } as i64;
        let dd = int(self, PDF_DECIMAL_DIGITS).clamp(0, 4) as u32;
        let names = self.page.map_names.as_deref().unwrap_or(&[]);
        for (i, f) in self.e.fonts.list.iter().enumerate().skip(self.page.font_pk.len()) {
            // An expanded copy (`name+e`) is drawn from its base font's program or bitmaps. A
            // copy found again for a later base of the same name (`tfm_lookup`) links to it.
            let blink = self.e.fonts.micro.expand(i as u16).blink;
            if f.expand != 0 && blink != 0 && (blink as usize) < i {
                let base = self.page.font_pk[blink as usize];
                self.page.font_pk.push(base);
                continue;
            }
            let f = if f.expand != 0 && blink != 0 { &self.e.fonts.list[blink as usize] } else { f };
            let mapped = names.binary_search(&crate::backend::backend_ship::fnv(f.name.as_bytes())).is_ok() || crate::backend::backend_cm_fonts::program_by_name(&f.name).is_some();
            let size = crate::backend::backend_pdftex::pdf_font_size(f.size as i64);
            self.page.font_pk.push(if mapped || f.name.is_empty() { 0 } else { crate::backend::backend_pk::pk_font_scale(size, res, dd) });
        }
    }

    fn pk_resolution(&self) -> i32 {
        match self.e.eqtb.word(super::engine_eqtb::Reg { region: super::engine_eqtb::Region::Int, slot: PDF_PK_RESOLUTION }) {
            0 => 600,
            r => r.clamp(72, 8000),
        }
    }

    /// The document so far as a PDF: the shipped pages and the fonts they use.
    pub fn pdf(&self) -> Vec<u8> {
        self.pdf_cached(None)
    }

    /// [`pdf`](Self::pdf) through a `--watch` session's caches of fonts, streams and images
    /// (`backend_cache`): the same bytes, with the work an earlier recompile did from the
    /// same inputs taken from the cache.
    pub fn pdf_cached(&self, cache: Option<&crate::backend::backend_cache::PdfCache>) -> Vec<u8> {
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
                    name: f.name.to_string(),
                    size: f.size as i64,
                    widths: std::array::from_fn(|c| f.dims(c as u8, &self.e.fonts.info).map_or(0, |d| d[0])),
                    used: self.page.font_used.get(i).copied().unwrap_or_default(),
                    pk: self.page.font_pk.get(i).filter(|&&s| s != 0).map(|&scale| crate::backend::backend_ship::PkFont {
                        dpi: crate::backend::backend_pk::pk_dpi(f.size as i64, f.design as i64, self.pk_resolution()),
                        scale,
                        used: self.page.font_used.get(i).copied().unwrap_or_default(),
                    }),
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
        // tounicode.c: with no entry defined, no ToUnicode map is written.
        let to_unicode: crate::backend::backend_ship::ToUnicode = if gen && !self.e.glyph_unicode.is_empty() { Some(&self.e.glyph_unicode) } else { None };
        let minor = self.e.eqtb.word(super::engine_eqtb::Reg { region: super::engine_eqtb::Region::Int, slot: PDF_MINOR_VERSION });
        let first_page = self.page.pdf.first().map_or(0, |p| p.id);
        let (objects, catalog, base) = self.pdf_doc.finish(first_page, files, self.image_hicolor(), cache);
        let objcompress = self.e.eqtb.word(super::engine_eqtb::Reg { region: super::engine_eqtb::Region::Int, slot: PDF_OBJ_COMPRESS_LEVEL });
        let meta = crate::backend::backend_ship::DocMeta { minor, objcompress, info: &self.pdf_doc.info, catalog: &catalog, objects: &objects, base: base as usize, forms: &self.pdf_doc.shipped_forms };
        crate::backend::backend_ship::write(&self.page.pdf, &fonts, files, to_unicode, &meta, cache)
    }
}
