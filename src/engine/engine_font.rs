//! Fonts as the engine sees them: tex.web §549–§582 (the font arrays, `read_font_info`,
//! `find_font_dimen`) and §1257–§1261 (`\font`), in our own form.
//!
//! A font is its file name, its size, and its parameters (`\fontdimen`), scaled as TeX scales
//! them; character metrics stay in the static TFM tables, so a loaded font costs one small
//! vector. Fonts are never freed, as in TeX. Font 0 is `\nullfont`.

use super::engine_names::param;
use super::engine_prim::{Meaning, Prim};
use super::engine_run::Machine;
use crate::data::data_blob::{quads, U32s};
use crate::data::data_tfm::{self, TfmParts, TfmView};
use crate::backend::scaled::{xn_over_d, Scaler};
use std::borrow::Cow;

/// One loaded font (§549's per-font arrays). Its scaled values live in `Fonts::info`.
#[derive(PartialEq, Clone)]
pub struct Font {
    /// The TFM name: borrowed from the font table for a pinned font, owned otherwise.
    pub name: Cow<'static, str>,
    /// `font_size` and `font_dsize` in scaled points.
    pub size: i32,
    pub design: i32,
    /// Where this font's scaled arrays start in `Fonts::info` (§550 `width_base` and kin):
    /// widths, heights, depths, italic corrections, kerns, then the parameters.
    at: FontAt,
    pub hyphen_char: i32,
    pub skew_char: i32,
    pub metrics: Metrics,
    /// §1257 `font_id_text`: the control sequence that last named the font (`\the\font`
    /// gives it back); `u32::MAX` for none.
    pub ident: u32,
    /// pdfTeX's `pdf_font_expand_ratio` of an expanded copy (thousandths; 0 for a font as
    /// loaded): its widths, italic corrections and kerns are the base's times
    /// `1000 + expand` over 1000 (`auto_expand_font`).
    pub expand: i16,
    /// Bit `c` for a code the font has; its width, height, depth and italic correction
    /// (§554) are resolved once at load into four words of the arena (`FontAt::dims`), so a
    /// list, a pass, packing or shipping reads one record, not the `char_info` word and four
    /// arrays.
    present: [u64; 4],
}

/// A font's bases in `Fonts::info` and its parameter count.
#[derive(PartialEq, Clone, Copy, Default)]
struct FontAt {
    width: u32,
    height: u32,
    depth: u32,
    italic: u32,
    kern: u32,
    param: u32,
    params: u32,
    /// [`Font::dims`]' 256 records of four words.
    dims: u32,
}

impl Font {
    /// Width, height, depth and italic correction of `c`, if it exists (§554), expanded as
    /// pdfTeX's `auto_expand_font` scales its copy: the `char_info` word, then four loads of
    /// values scaled at load (§571). `info` is `Fonts::info`.
    #[inline]
    pub fn dims(&self, c: u8, info: &[i32]) -> Option<[i32; 4]> {
        if self.present[c as usize >> 6] >> (c & 63) & 1 == 0 {
            return None;
        }
        let at = self.at.dims as usize + 4 * c as usize;
        info.get(at..at + 4).map(|d| [d[0], d[1], d[2], d[3]])
    }

    /// [`Font::dims`] worked out from the `char_info` word (§554), for the table at load.
    fn resolve_dims(&self, c: u8, info: &[i32]) -> Option<[i32; 4]> {
        let ci = self.metrics.info(c);
        if ci >> 24 == 0 {
            return None;
        }
        let a = &self.at;
        let at = |base: u32, i: u32| info.get((base + i) as usize).copied().unwrap_or(0);
        let d = [at(a.width, ci >> 24), at(a.height, ci >> 20 & 15), at(a.depth, ci >> 16 & 15), at(a.italic, ci >> 10 & 63)];
        if self.expand == 0 {
            return Some(d);
        }
        Some([self.expanded(d[0]), d[1], d[2], self.expanded(d[3])])
    }

    /// Kern `k` of the lig/kern program (§557's kern index), scaled at load.
    #[inline]
    pub fn kern(&self, k: usize, info: &[i32]) -> i32 {
        // The kerns end where the parameters begin; an index past them is zero, never the
        // next array.
        let (base, end) = (self.at.kern as usize, self.at.param as usize);
        if k < end - base { info[base + k] } else { 0 }
    }

    /// The parameters (`\fontdimen1..`, scaled; `\fontdimen1`, the slant, a plain fraction).
    #[inline]
    pub fn params<'i>(&self, info: &'i [i32]) -> &'i [i32] {
        &info[self.at.param as usize..(self.at.param + self.at.params) as usize]
    }

    /// A width or kern of this font's base, scaled to this expanded copy.
    #[inline]
    /// [`Font::dims`]' records, appended to the arena from its scaled arrays.
    fn resolve_all(&mut self, info: &mut super::engine_chunk::Chunked<i32>) {
        self.at.dims = info.len() as u32;
        info.reserve(4 * 256);
        for c in 0..=255u8 {
            let d = self.resolve_dims(c, info);
            if d.is_some() {
                self.present[c as usize >> 6] |= 1 << (c & 63);
            }
            info.extend(d.unwrap_or_default());
        }
    }

    pub fn expanded(&self, v: i32) -> i32 {
        if self.expand == 0 {
            v
        } else {
            crate::backend::scaled::round_xn_over_d(v, 1000 + self.expand as i32, 1000)
        }
    }
}

/// Where a font's character metrics come from. Both kinds hold the same layout: §539's
/// word arrays as little-endian quads (`data_tfm::TfmView`), so the per-character paths
/// below read one representation.
#[derive(PartialEq, Clone)]
pub enum Metrics {
    None,
    /// A pinned font: its rows in the static table (`data_tfm::fonts`), resolved once at
    /// `\font`, and its table ID (what the format stores).
    Table(TfmView<'static>, u32),
    /// A TFM outside the table, read from its file.
    File(std::rc::Rc<FileTfm>),
}

/// The parts of a TFM file read at run time (§539–§546), in the table's quad layout.
#[derive(PartialEq)]
pub struct FileTfm {
    bc: u8,
    design: i32,
    /// In `data_tfm::POOLS` order.
    pools: [Vec<[u8; 4]>; 9],
}

impl FileTfm {
    fn new(p: &TfmParts) -> FileTfm {
        FileTfm { bc: p.bc, design: p.design, pools: std::array::from_fn(|k| quads(p.pools[k].iter().copied())) }
    }

    fn view(&self) -> TfmView<'_> {
        TfmView::from_pools(self.bc, self.design, std::array::from_fn(|k| U32s::new(&self.pools[k])))
    }
}

/// §545: `char_tag` of a character with a lig/kern program.
const LIG_TAG: u32 = 1;
/// §545 `stop_flag` and `kern_flag`.
pub const STOP_FLAG: u8 = 128;
/// §549 `non_char`: no boundary character.
pub const NON_CHAR: u16 = 256;

/// The `char_info` word of `c` (zero when absent).
#[inline]
fn info_of(v: &TfmView, c: u8) -> u32 {
    c.checked_sub(v.bc).and_then(|i| v.info.try_get(i as usize)).unwrap_or(0)
}

impl Metrics {
    /// The `char_info` word of `c` (zero when absent).
    #[inline]
    pub fn info(&self, c: u8) -> u32 {
        match self {
            Metrics::None => 0,
            Metrics::Table(v, _) => info_of(v, c),
            Metrics::File(f) => info_of(&f.view(), c),
        }
    }

    /// The font's arrays, borrowed (at load; not on a per-character path).
    fn view(&self) -> TfmView<'_> {
        match self {
            Metrics::None => TfmView::default(),
            Metrics::Table(v, _) => *v,
            Metrics::File(f) => f.view(),
        }
    }

    /// §544 `char_tag` and `rem_byte` of `c`.
    pub fn tag(&self, c: u8) -> (u32, u8) {
        let info = self.info(c);
        (info >> 8 & 3, info as u8)
    }

    /// §546 extensible recipe `r`: top, mid, bot, rep (the word's bytes, high first).
    pub fn exten(&self, r: u8) -> [u8; 4] {
        let get = |v: &TfmView| v.exten.try_get(r as usize).map_or([0; 4], u32::to_be_bytes);
        match self {
            Metrics::None => [0; 4],
            Metrics::Table(v, _) => get(v),
            Metrics::File(f) => get(&f.view()),
        }
    }

    /// Whether `c` exists (§554: its width index is not zero).
    pub fn exists(&self, c: u8) -> bool {
        self.info(c) >> 24 != 0
    }

    /// The lig/kern program and its kerns (unscaled fix_words, read `as i32`). One match
    /// here, then the walkers index the two columns with no branch per word.
    pub fn lig_kern(&self) -> (U32s<'_>, U32s<'_>) {
        match self {
            Metrics::None => Default::default(),
            Metrics::Table(v, _) => (v.lig_kern, v.kerns),
            Metrics::File(f) => (U32s::new(&f.pools[5]), U32s::new(&f.pools[6])),
        }
    }

    /// §557 `lig_kern_start`, with §557's restart when the first word says so: where the
    /// program of `c` begins, if it has one.
    pub fn lig_kern_start(&self, c: u8) -> Option<usize> {
        let info = self.info(c);
        if info >> 8 & 3 != LIG_TAG {
            return None;
        }
        let (prog, _) = self.lig_kern();
        let k = (info & 0xFF) as usize;
        let j = prog.try_get(k)?;
        Some(if (j >> 24) as u8 > STOP_FLAG { restart(j) } else { k })
    }

    /// §573 `bchar` and §576 `bchar_label`: the right boundary character (`NON_CHAR` for
    /// none) and where the left boundary's program begins.
    pub fn boundary(&self) -> (u16, Option<usize>) {
        let (prog, _) = self.lig_kern();
        let bchar = match prog.try_get(0) {
            Some(j) if j >> 24 == 255 => (j >> 16 & 0xFF) as u16,
            _ => NON_CHAR,
        };
        let label = match prog.len().checked_sub(1).and_then(|l| prog.try_get(l)) {
            Some(j) if j >> 24 == 255 => Some(restart(j)).filter(|&l| l < prog.len()),
            _ => None,
        };
        (bchar, label)
    }
}

/// §557 `lig_kern_restart`: the program index in the op and remainder bytes of `j`.
#[inline]
pub fn restart(j: u32) -> usize {
    (j >> 8 & 0xFF) as usize * 256 + (j & 0xFF) as usize
}

impl Fonts {
    /// A checkpoint's copy, the parameter arena sharing what did not change since the last
    /// one, and a live copy of one (`engine_chunk`).
    pub fn freeze(&mut self) -> Fonts {
        Fonts { list: self.list.freeze(), info: self.info.freeze(), by_key: self.by_key.clone(), micro: self.micro.clone() }
    }

    pub fn thaw(&self) -> Fonts {
        Fonts { list: self.list.thaw(), info: self.info.thaw(), by_key: self.by_key.clone(), micro: self.micro.clone() }
    }
}

impl Fonts {
    /// The fonts of a format: each one's scaled parameters, and its metrics as the name of an
    /// embedded TFM or the parts of the file read.
    #[cfg(feature = "oracle")]
    pub(super) fn dump(&self, w: &mut super::engine_snapshot::Writer) {
        w.count(self.list.len());
        for f in &self.list {
            w.str(&f.name);
            w.i32(f.size);
            w.i32(f.design);
            w.all(f.params(&self.info));
            w.i32(f.hyphen_char);
            w.i32(f.skew_char);
            match &f.metrics {
                Metrics::None => w.u8(0),
                Metrics::Table(_, id) => {
                    w.u8(1);
                    w.u32(*id);
                }
                Metrics::File(t) => {
                    w.u8(2);
                    w.u8(t.bc);
                    w.i32(t.design);
                    for pool in &t.pools {
                        w.all(&pool.iter().map(|&q| u32::from_le_bytes(q)).collect::<Vec<u32>>());
                    }
                }
            }
            w.u32(f.ident);
        }
    }

    /// The fonts of a format: each is loaded again from its metrics (the scaled arrays are
    /// recomputed, never stored), then given its stored parameters.
    #[cfg(feature = "oracle")]
    pub(super) fn undump(r: &mut super::engine_snapshot::Reader) -> Option<Fonts> {
        let n = r.count()?;
        let mut fonts = Fonts { list: super::engine_chunk::Chunked::with_capacity(n), info: Default::default(), by_key: Default::default(), micro: Default::default() };
        for _ in 0..n {
            let (name, size, design, params, hyphen_char, skew_char): (String, i32, i32, Vec<i32>, i32, i32) = (r.str()?, r.i32()?, r.i32()?, r.all()?, r.i32()?, r.i32()?);
            let metrics = match r.u8()? {
                0 => Metrics::None,
                1 => {
                    let (table, id) = (data_tfm::fonts()?, r.u32()?);
                    (id < table.len() as u32).then(|| Metrics::Table(table.view(id), id))?
                }
                2 => {
                    let (bc, design) = (r.u8()?, r.i32()?);
                    let mut pools: [Vec<[u8; 4]>; 9] = Default::default();
                    for pool in &mut pools {
                        *pool = quads(r.all::<u32>()?);
                    }
                    Metrics::File(std::rc::Rc::new(FileTfm { bc, design, pools }))
                }
                _ => return None,
            };
            let ident = r.u32()?;
            let name = match &metrics {
                Metrics::Table(_, id) => table_name(*id).filter(|n| *n == name).map_or(Cow::Owned(name), Cow::Borrowed),
                _ => Cow::Owned(name),
            };
            let f = fonts.push(name, size, design, metrics, params.iter().copied(), false, hyphen_char, skew_char);
            fonts.list[f as usize].ident = ident;
        }
        // The format's fonts fit exactly (one font's parameters can be large: expl3 keeps
        // data in `\fontdimen`s), with fixed room for a document's own fonts.
        let mut info = fonts.info.take();
        info.shrink_to_fit();
        info.reserve_exact(ARENA_HEADROOM);
        fonts.info = info.into();
        (!fonts.list.is_empty()).then_some(fonts)
    }
}

/// The name of pinned font `id`, borrowed from the table.
fn table_name(id: u32) -> Option<&'static str> {
    std::str::from_utf8(data_tfm::fonts()?.name(id)).ok()
}

/// Every font loaded so far, and their scaled values in one append-only arena (TeX's
/// `font_info`, §549). Fonts are never freed, as in TeX; a checkpoint is the two lengths.
#[derive(PartialEq, Clone)]
pub struct Fonts {
    pub list: super::engine_chunk::Chunked<Font>,
    /// Scaled widths, heights, depths, italic corrections and kerns of every font, then its
    /// parameters last, so §579's growth of the last font's parameters appends in place.
    pub info: super::engine_chunk::Chunked<i32>,
    /// (hash of the name, size) to the first font loaded with them (§1260's search).
    by_key: std::collections::HashMap<(u64, i32), u16>,
    /// Protrusion and expansion codes and links (`engine_microtype`); never in a format.
    pub micro: super::engine_microtype::Micro,
}

/// Arena words reserved past a format's fonts: about 300 fonts of the corpus's average size.
#[cfg(feature = "oracle")]
const ARENA_HEADROOM: usize = 1 << 16;

#[inline]
fn name_key(name: &str) -> u64 {
    crate::backend::backend_hash::hash64(0, name.as_bytes())
}

impl Default for Fonts {
    /// §552: `\nullfont` has seven zero parameters.
    fn default() -> Self {
        let mut f = Fonts { list: Default::default(), info: Default::default(), by_key: Default::default(), micro: Default::default() };
        f.push(Cow::Borrowed("nullfont"), 0, 0, Metrics::None, [0; 7].into_iter(), false, '-' as i32, -1);
        f
    }
}

impl Fonts {
    /// Appends a font: its metric arrays scaled once to `size` (§571–§575) and its
    /// parameters (`scale_params`: raw fix_words, the slant a plain fraction; otherwise
    /// already scaled), at least seven. No allocation beyond the arena's amortized growth
    /// and, for a font outside the table, its name.
    #[allow(clippy::too_many_arguments)]
    fn push(&mut self, name: Cow<'static, str>, size: i32, design: i32, metrics: Metrics, params: impl Iterator<Item = i32>, scale_params: bool, hyphen_char: i32, skew_char: i32) -> u16 {
        let s = Scaler::new(size);
        let v = metrics.view();
        let base = |info: &mut super::engine_chunk::Chunked<i32>, col: U32s| {
            let b = info.len() as u32;
            info.extend(col.iter().map(|x| s.scale(x as i32)));
            b
        };
        let (width, height, depth, italic, kern) = (base(&mut self.info, v.widths), base(&mut self.info, v.heights), base(&mut self.info, v.depths), base(&mut self.info, v.italics), base(&mut self.info, v.kerns));
        // The resolved records come before the parameters, which end the arena (§579).
        let mut font = Font { name, size, design, at: FontAt { width, height, depth, italic, kern, ..FontAt::default() }, hyphen_char, skew_char, metrics, ident: u32::MAX, expand: 0, present: [0; 4] };
        font.resolve_all(&mut self.info);
        let param = self.info.len() as u32;
        self.info.extend(params.enumerate().map(|(k, p)| if !scale_params { p } else if k == 0 { p >> 4 } else { s.scale(p) }));
        let params = (self.info.len() as u32 - param).max(7);
        self.info.resize((param + params) as usize, 0);
        let f = self.list.len() as u16;
        (font.at.param, font.at.params) = (param, params);
        self.by_key.entry((name_key(&font.name), size)).or_insert(f);
        self.list.push(font);
        f
    }

    /// The first font loaded as `name` at `size` (§1260).
    pub(super) fn find(&self, name: &str, size: i32) -> Option<u16> {
        self.by_key.get(&(name_key(name), size)).copied().filter(|&f| self.list[f as usize].name == name)
    }

    /// pdftex.ch's §1260: the first font loaded as `name` at `size` that `\pdffontexpand` has
    /// not set up; an expandable font is never loaded again under a new name.
    pub(super) fn find_reusable(&self, name: &str, size: i32) -> Option<u16> {
        let f = self.find(name, size)?;
        if self.micro.expand(f).step == 0 {
            return Some(f);
        }
        (f as usize + 1..self.list.len()).find(|&k| self.list[k].name == name && self.list[k].size == size && self.micro.expand(k as u16).step == 0).map(|k| k as u16)
    }

    /// Font `f`'s parameters.
    #[inline]
    pub fn params(&self, f: u16) -> &[i32] {
        self.list[f as usize].params(&self.info)
    }

    #[inline]
    pub fn params_mut(&mut self, f: u16) -> &mut [i32] {
        let a = self.list[f as usize].at;
        self.info.slice_mut(a.param as usize..(a.param + a.params) as usize)
    }

    /// §579: the last-loaded font's parameters grown to `n`; its parameters end the arena.
    fn grow_params(&mut self, f: u16, n: u32) {
        let a = &mut self.list[f as usize].at;
        debug_assert_eq!((a.param + a.params) as usize, self.info.len(), "only the last font's parameters end the arena");
        a.params = n;
        self.info.resize((a.param + n) as usize, 0);
    }

    /// Width, height, depth and italic correction of `c` in font `f` (§554).
    #[inline]
    pub fn dims(&self, f: u16, c: u8) -> Option<[i32; 4]> {
        self.list[f as usize].dims(c, &self.info)
    }

    /// An expanded copy of font `base` (pdfTeX's `auto_expand_font`): the base's scaled
    /// arrays shared, its own parameters, `expand` thousandths.
    pub fn push_expanded(&mut self, base: u16, name: Cow<'static, str>, expand: i16) -> u16 {
        let b = &self.list[base as usize];
        let (size, design, hyphen_char, skew_char, metrics, ident, at) = (b.size, b.design, b.hyphen_char, b.skew_char, b.metrics.clone(), b.ident, b.at);
        let (from, n) = (at.param as usize, at.params as usize);
        let mut font = Font { name, size, design, at, hyphen_char, skew_char, metrics, ident, expand, present: [0; 4] };
        font.resolve_all(&mut self.info);
        font.at.param = self.info.len() as u32;
        self.info.extend_from_within(from..from + n);
        let f = self.list.len() as u16;
        self.by_key.entry((name_key(&font.name), size)).or_insert(f);
        self.list.push(font);
        f
    }
}

/// The name under which the current font is kept: a meaning, so it is grouped like one
/// (§230 `cur_font_loc`).
pub(super) const CUR_FONT: &str = "\0font";

impl Machine<'_, '_> {
    /// §1257 `new_font`: `\font<cs>[=]<file name>[ at <dimen> | scaled <n>]`.
    pub(super) fn new_font(&mut self, global: bool) {
        let Some(id) = self.get_r_token() else { return };
        // While the name and size are read, the control sequence means `\nullfont`.
        self.e.eqtb.set_meaning(id, Meaning::Font(0), global);
        self.scan_optional_equals();
        let name = self.scan_file_name();
        // §1258: `s > 0` is an `at` size, `s < 0` a scale factor times -1000.
        let mut s = -1000;
        if self.scan_keyword("at") {
            s = self.scan_dimen(false).0;
            if s <= 0 || s >= 2048 << 16 {
                self.error("Improper `at' size, replaced by 10pt");
                s = 10 << 16;
            }
        } else if self.scan_keyword("scaled") {
            let n = self.scan_int();
            s = -n;
            if n <= 0 || n > 32768 {
                self.error("Illegal magnification has been changed to 1000");
                s = -1000;
            }
        }
        let f = self.find_font(&name, s, Some(id));
        self.e.eqtb.set_meaning(id, Meaning::Font(f), global);
        self.e.fonts.list[f as usize].ident = id;
    }

    /// §1260: a font already loaded with this name and size, else §560 `read_font_info`;
    /// `cs` is the control sequence being defined, for §561's message.
    pub(super) fn find_font(&mut self, name: &str, s: i32, cs: Option<u32>) -> u16 {
        // The static table first (no file read, arrays borrowed in place), then the file
        // `<name>.tfm` from the resolver.
        let key = name.strip_suffix(".tfm").unwrap_or(name);
        let pinned = data_tfm::fonts().and_then(|t| t.lookup(key).map(|id| (t.view(id), id)));
        let design = pinned.as_ref().map(|(v, _)| v.design);
        // §1260: a font already loaded with this name and size. The size of a `scaled` font
        // depends on the design size, known for a pinned font without loading it. A font
        // `\pdffontexpand` has set up is passed over (pdftex.ch).
        if let Some(d) = design {
            let size = if s > 0 { s } else { xn_over_d(d, -s, 1000) };
            if let Some(f) = self.e.fonts.find_reusable(name, size) {
                return f;
            }
        } else if let Some(f) = self.e.fonts.list.iter().enumerate().skip(1).find(|&(k, font)| font.name == name && font.size == if s > 0 { s } else { xn_over_d(font.design, -s, 1000) } && self.e.fonts.micro.expand(k as u16).step == 0).map(|(f, _)| f as u16) {
            return f;
        }
        let (design, metrics) = match pinned {
            Some((v, id)) => (v.design, Metrics::Table(v, id)),
            None => match self.read_file.as_ref().and_then(|read| read(&format!("{name}.tfm"))).as_deref().and_then(data_tfm::parse) {
                Some(p) => (p.design, Metrics::File(std::rc::Rc::new(FileTfm::new(&p)))),
                None => {
                    // §561: `Font \x=name at 12.0pt not loadable: …`.
                    let mut msg = String::from("Font ");
                    if let Some(cs) = cs {
                        msg.push('\\');
                        msg.push_str(self.e.cs.name(cs));
                        msg.push('=');
                    }
                    msg.push_str(name);
                    if s >= 0 {
                        msg.push_str(" at ");
                        super::engine_value::print_scaled(s, &mut msg);
                        msg.push_str("pt");
                    } else if s != -1000 {
                        msg.push_str(&format!(" scaled {}", -s));
                    }
                    msg.push_str(" not loadable: Metric (TFM) file not found");
                    self.error(msg);
                    return 0;
                }
            },
        };
        let size = if s > 0 { s } else if s == -1000 { design } else { xn_over_d(design, -s, 1000) };
        let name = match &metrics {
            Metrics::Table(_, id) => table_name(*id).filter(|n| *n == name).map_or_else(|| Cow::Owned(name.to_string()), Cow::Borrowed),
            _ => Cow::Owned(name.to_string()),
        };
        let (hyphen_char, skew_char) = (self.int_param("defaulthyphenchar"), self.int_param("defaultskewchar"));
        let raw = metrics.view().params;
        self.e.fonts.push(name, size, design, metrics.clone(), raw.iter().map(|p| p as i32), true, hyphen_char, skew_char)
    }

    fn int_param(&self, name: &str) -> i32 {
        param(name).map_or(0, |r| self.e.eqtb.word(r))
    }

    /// The current font (§230 `cur_font`).
    pub(super) fn cur_font(&self) -> u16 {
        match self.e.eqtb.meaning(super::CUR_FONT_ID) {
            Meaning::Font(f) => f,
            _ => 0,
        }
    }

    /// §1217: selecting font `f` is an assignment of the current font.
    pub(super) fn select_font(&mut self, f: u16, global: bool) {
        let id = super::CUR_FONT_ID;
        self.e.eqtb.set_meaning(id, Meaning::Font(f), global);
    }

    /// §577 `scan_font_ident`: a font identifier, `\font` (the current font) or `\nullfont`.
    pub(super) fn scan_font_ident(&mut self) -> u16 {
        let Some(x) = self.next_non_blank() else { return 0 };
        match x.m {
            Meaning::Font(f) => f,
            Meaning::Prim(Prim::Font) => self.cur_font(),
            Meaning::Prim(Prim::NullFont) => 0,
            Meaning::Prim(p @ (Prim::TextFont | Prim::ScriptFont | Prim::ScriptScriptFont)) => {
                let n = self.scan_int();
                self.fam_font(p, n)
            }
            _ => {
                self.error("Missing font identifier");
                self.input.back(x.t);
                0
            }
        }
    }

    /// `\textfont n` and its kin: the id of the family's font equivalent.
    fn fam_id(p: Prim, n: i32) -> u32 {
        let size = match p {
            Prim::TextFont => 0,
            Prim::ScriptFont => 16,
            _ => 32,
        };
        super::FAM_BASE_ID + size + n.clamp(0, 15) as u32
    }

    /// The font of family `n` in `p`'s size (§230), `\nullfont` when unset.
    pub(super) fn fam_font(&self, p: Prim, n: i32) -> u16 {
        match self.e.eqtb.meaning(Self::fam_id(p, n)) {
            Meaning::Font(f) => f,
            _ => 0,
        }
    }

    /// §1234 `def_family`: `\textfont n = <font>`.
    pub(super) fn def_family(&mut self, p: Prim, global: bool) {
        let n = self.scan_int();
        if !(0..16).contains(&n) {
            self.error(format!("Bad number ({n})"));
        }
        self.scan_optional_equals();
        let f = self.scan_font_ident();
        self.e.eqtb.set_meaning(Self::fam_id(p, n), Meaning::Font(f), global);
    }

    /// §578 `find_font_dimen`: `<n><font>`; the parameter's index, growing the most recently
    /// loaded font's parameters when `n` is past its end (only that font can grow).
    pub(super) fn find_font_dimen(&mut self) -> Option<(u16, usize)> {
        let n = self.scan_int();
        let f = self.scan_font_ident();
        let last = self.e.fonts.list.len() - 1;
        let have = self.e.fonts.params(f).len();
        let fits = n > 0 && (n as usize <= have || (f as usize == last && f != 0));
        if !fits {
            let msg = format!("Font {} has only {have} fontdimen parameters", self.e.fonts.list[f as usize].name);
            self.error(msg);
            return None;
        }
        if n as usize > have {
            self.e.fonts.grow_params(f, n as u32);
        }
        Some((f, n as usize - 1))
    }

    /// `\fontdimen<n><font>[=]<dimen>` (§1253), global as fonts are.
    pub(super) fn assign_font_dimen(&mut self) {
        let at = self.find_font_dimen();
        self.scan_optional_equals();
        let v = self.scan_dimen(false).0;
        if let Some((f, k)) = at {
            self.e.fonts.params_mut(f)[k] = v;
        }
    }

    /// `\hyphenchar` and `\skewchar` `<font>[=]<int>` (§1253).
    pub(super) fn assign_font_int(&mut self, skew: bool) {
        let f = self.scan_font_ident() as usize;
        self.scan_optional_equals();
        let v = self.scan_int();
        let font = &mut self.e.fonts.list[f];
        if skew {
            font.skew_char = v;
        } else {
            font.hyphen_char = v;
        }
    }

    /// §1261 and §472 `\fontname`: the file name, and `at <size>` when it is not the
    /// design size.
    pub(super) fn font_name(&mut self) -> String {
        let f = self.scan_font_ident() as usize;
        let font = &self.e.fonts.list[f];
        let mut s = font.name.to_string();
        if font.size != font.design {
            s.push_str(" at ");
            super::engine_value::print_scaled(font.size, &mut s);
            s.push_str("pt");
        }
        s
    }
}

impl std::fmt::Debug for Fonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fonts({})", self.list.len())
    }
}

#[cfg(test)]
mod tests {
    use super::super::Engine;
    use super::*;

    fn ok(src: &str) -> String {
        let mut e = Engine::default();
        let mut m = Machine::new(&mut e, src);
        m.run();
        assert!(m.errors.is_empty(), "{:?}", m.errors);
        m.out_text()
    }

    #[test]
    fn fonts_load_scale_and_grow() {
        // cmr10's quad is 1048579 fix_words: 10.00002pt (pdfTeX's `\the\fontdimen6`).
        assert_eq!(ok("\\font\\x=cmr10\\relax\\the\\fontdimen6\\x"), "\\relax 10.00002pt");
        // The keyword scan expands `\the` while `\x` still means `\nullfont` (§1257).
        assert_eq!(ok("\\font\\x=cmr10 \\the\\fontdimen6\\x"), "0.0pt");
        assert_eq!(ok("\\font\\y=cmr10 at 5pt \\fontname\\y"), "cmr10 at 5.0pt");
        assert_eq!(ok("\\font\\y=cmr10 scaled 2000 \\meaning\\y"), "select font cmr10 at 20.0pt");
        assert_eq!(ok("\\font\\z=cmr10 at 1sp \\fontdimen9\\z=3sp \\the\\fontdimen9\\z"), "0.00005pt");
        assert_eq!(ok("\\font\\x=cmr10 \\hyphenchar\\x=-1 \\the\\hyphenchar\\x"), "-1");
        assert_eq!(ok("\\font\\x=cmr10 {\\x\\fontname\\font}\\fontname\\font"), "{cmr10}nullfont");
    }

    #[test]
    fn only_the_last_font_grows() {
        let mut e = Engine::default();
        let mut m = Machine::new(&mut e, "\\font\\a=cmr10 \\font\\b=cmr7 \\fontdimen9\\a=1pt");
        m.run();
        assert_eq!(m.errors.len(), 1, "{:?}", m.errors);
    }

    /// §1260 finds a loaded font by name and size: the arena does not grow, the name is
    /// borrowed from the table, and the arena's values equal per-use scaling (§571).
    #[test]
    fn arena_reuses_and_scales_once() {
        let mut e = Engine::default();
        let mut m = Machine::new(&mut e, "\\font\\a=cmr10 \\font\\b=cmr10 scaled 1000 \\font\\c=cmr10 at 10pt \\font\\d=cmr10 at 5pt ");
        m.run();
        assert!(m.errors.is_empty(), "{:?}", m.errors);
        let fonts = &m.e.fonts;
        assert_eq!(fonts.list.len(), 3, "nullfont, cmr10, cmr10 at 5pt");
        assert!(matches!(fonts.list[1].name, Cow::Borrowed("cmr10")));
        let (f, half) = (&fonts.list[1], &fonts.list[2]);
        let s = Scaler::new(f.size);
        let v = f.metrics.view();
        let ci = f.metrics.info(b'A');
        let wd = s.scale(v.widths.get((ci >> 24) as usize) as i32);
        assert_eq!(fonts.dims(1, b'A').map(|d| d[0]), Some(wd));
        assert_eq!(fonts.dims(2, b'A').map(|d| d[0]), Some(Scaler::new(half.size).scale(v.widths.get((ci >> 24) as usize) as i32)));
        assert_eq!(f.kern(usize::MAX / 2, &fonts.info), 0);
        assert_eq!(fonts.params(1).len(), 7);
    }
}
