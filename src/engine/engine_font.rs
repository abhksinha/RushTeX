//! Fonts as the engine sees them: tex.web §549–§582 (the font arrays, `read_font_info`,
//! `find_font_dimen`) and §1257–§1261 (`\font`), in our own form.
//!
//! A font is its file name, its size, and its parameters (`\fontdimen`), scaled as TeX scales
//! them; character metrics stay in the static TFM tables, so a loaded font costs one small
//! vector. Fonts are never freed, as in TeX. Font 0 is `\nullfont`.

use super::engine_names::param;
use super::engine_prim::{Meaning, Prim};
use super::engine_run::Machine;
use crate::backend::backend_tfm::{by_name, Tfm};
use crate::backend::scaled::{xn_over_d, Scaler};

/// One loaded font (§549's per-font arrays).
pub struct Font {
    pub name: String,
    /// `font_size` and `font_dsize` in scaled points.
    pub size: i32,
    pub design: i32,
    /// `\fontdimen1..`, scaled (`\fontdimen1`, the slant, is a plain fraction).
    pub params: Vec<i32>,
    pub hyphen_char: i32,
    pub skew_char: i32,
    pub metrics: Metrics,
    /// §1257 `font_id_text`: the control sequence that last named the font (`\the\font`
    /// gives it back); `u32::MAX` for none.
    pub ident: u32,
}

/// Where a font's character metrics come from: an embedded TFM, or one read from a file.
pub enum Metrics {
    None,
    Static(&'static Tfm),
    File(Box<FileTfm>),
}

/// The parts of a TFM file (§539–§545) the engine reads: each character's `char_info` word
/// and the dimension tables, all fix_words.
pub struct FileTfm {
    bc: u8,
    info: Vec<u32>,
    widths: Vec<i32>,
    heights: Vec<i32>,
    depths: Vec<i32>,
    italics: Vec<i32>,
    lig_kern: Vec<u32>,
    kerns: Vec<i32>,
    /// §546 extensible recipes: top, mid, bot, rep.
    exten: Vec<u32>,
}

/// §545: `char_tag` of a character with a lig/kern program.
const LIG_TAG: u32 = 1;
/// §545 `stop_flag` and `kern_flag`.
pub const STOP_FLAG: u8 = 128;
/// §549 `non_char`: no boundary character.
pub const NON_CHAR: u16 = 256;

impl Metrics {
    /// The `char_info` word of `c` (zero when absent).
    fn info(&self, c: u8) -> u32 {
        match self {
            Metrics::None => 0,
            Metrics::Static(t) => {
                if c < 128 {
                    t.info[c as usize]
                } else {
                    0
                }
            }
            Metrics::File(f) => c.checked_sub(f.bc).and_then(|i| f.info.get(i as usize)).copied().unwrap_or(0),
        }
    }

    /// §544 `char_tag` and `rem_byte` of `c`.
    pub fn tag(&self, c: u8) -> (u32, u8) {
        let info = self.info(c);
        (info >> 8 & 3, info as u8)
    }

    /// §546 extensible recipe `r`: top, mid, bot, rep.
    pub fn exten(&self, r: u8) -> [u8; 4] {
        match self {
            Metrics::None => [0; 4],
            Metrics::Static(t) => t.exten.get(r as usize).copied().unwrap_or([0; 4]),
            Metrics::File(f) => f.exten.get(r as usize).map_or([0; 4], |w| w.to_be_bytes()),
        }
    }

    /// Whether `c` exists (§554: its width index is not zero).
    pub fn exists(&self, c: u8) -> bool {
        self.info(c) >> 24 != 0
    }

    /// The lig/kern program and its kerns (unscaled fix_words).
    pub fn lig_kern(&self) -> (&[u32], &[i32]) {
        match self {
            Metrics::None => (&[], &[]),
            Metrics::Static(t) => (t.lig_kern, t.kerns),
            Metrics::File(f) => (&f.lig_kern, &f.kerns),
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
        let j = *prog.get(k)?;
        Some(if (j >> 24) as u8 > STOP_FLAG { restart(j) } else { k })
    }

    /// §573 `bchar` and §576 `bchar_label`: the right boundary character (`NON_CHAR` for
    /// none) and where the left boundary's program begins.
    pub fn boundary(&self) -> (u16, Option<usize>) {
        let (prog, _) = self.lig_kern();
        let bchar = match prog.first() {
            Some(&j) if j >> 24 == 255 => (j >> 16 & 0xFF) as u16,
            _ => NON_CHAR,
        };
        let label = match prog.last() {
            Some(&j) if j >> 24 == 255 => Some(restart(j)).filter(|&l| l < prog.len()),
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

impl Metrics {
    /// Width, height, depth and italic correction of `c` at `size`, if `c` exists (§554).
    pub fn dims(&self, c: u8, size: i32) -> Option<[i32; 4]> {
        match self {
            Metrics::None => None,
            Metrics::Static(tfm) => tfm.exists(c).then(|| tfm.dims_sp(c, &Scaler::new(size))),
            Metrics::File(f) => {
                let info = *f.info.get(c.checked_sub(f.bc)? as usize)?;
                let w = (info >> 24) as usize;
                if w == 0 {
                    return None;
                }
                let s = Scaler::new(size);
                let at = |t: &[i32], i: usize| s.scale(t.get(i).copied().unwrap_or(0));
                Some([at(&f.widths, w), at(&f.heights, (info >> 20 & 15) as usize), at(&f.depths, (info >> 16 & 15) as usize), at(&f.italics, (info >> 10 & 63) as usize)])
            }
        }
    }
}

/// §560 `read_font_info`: the design size, the parameters and the metrics of a TFM file;
/// `None` when the file is not a well-formed TFM.
pub fn parse_tfm(b: &[u8]) -> Option<(i32, Vec<i32>, FileTfm)> {
    let half = |i: usize| -> Option<usize> { Some(u16::from_be_bytes([*b.get(2 * i)?, *b.get(2 * i + 1)?]) as usize) };
    let word = |i: usize| -> Option<i32> { Some(i32::from_be_bytes(b.get(4 * i..4 * i + 4)?.try_into().ok()?)) };
    let (lf, lh, bc, ec) = (half(0)?, half(1)?, half(2)?, half(3)?);
    let (nw, nh, nd, ni, nl, nk, ne, np) = (half(4)?, half(5)?, half(6)?, half(7)?, half(8)?, half(9)?, half(10)?, half(11)?);
    let chars = if ec + 1 >= bc { ec + 1 - bc } else { return None };
    if b.len() < 4 * lf || lf != 6 + lh + chars + nw + nh + nd + ni + nl + nk + ne + np || lh < 2 || ec > 255 {
        return None;
    }
    let design = word(7)? >> 4;
    let mut at = 6 + lh;
    let mut table = |n: usize| -> Option<Vec<i32>> {
        let v = (at..at + n).map(word).collect::<Option<Vec<i32>>>();
        at += n;
        v
    };
    let info: Vec<u32> = table(chars)?.into_iter().map(|w| w as u32).collect();
    let (widths, heights, depths, italics) = (table(nw)?, table(nh)?, table(nd)?, table(ni)?);
    let lig_kern: Vec<u32> = table(nl)?.into_iter().map(|w| w as u32).collect();
    let kerns = table(nk)?;
    let exten: Vec<u32> = table(ne)?.into_iter().map(|w| w as u32).collect();
    let params = table(np)?;
    Some((design, params, FileTfm { bc: bc as u8, info, widths, heights, depths, italics, lig_kern, kerns, exten }))
}

impl Fonts {
    /// The fonts of a format: each one's scaled parameters, and its metrics as the name of an
    /// embedded TFM or the parts of the file read.
    pub(super) fn dump(&self, w: &mut super::engine_snapshot::Writer) {
        w.count(self.list.len());
        for f in &self.list {
            w.str(&f.name);
            w.i32(f.size);
            w.i32(f.design);
            w.all(&f.params);
            w.i32(f.hyphen_char);
            w.i32(f.skew_char);
            match &f.metrics {
                Metrics::None => w.u8(0),
                Metrics::Static(_) => w.u8(1),
                Metrics::File(t) => {
                    w.u8(2);
                    w.u8(t.bc);
                    w.all(&t.info);
                    for v in [&t.widths, &t.heights, &t.depths, &t.italics] {
                        w.all(v);
                    }
                    w.all(&t.lig_kern);
                    w.all(&t.kerns);
                    w.all(&t.exten);
                }
            }
            w.u32(f.ident);
        }
    }

    pub(super) fn undump(r: &mut super::engine_snapshot::Reader) -> Option<Fonts> {
        let n = r.count()?;
        let mut list = Vec::with_capacity(n);
        for _ in 0..n {
            let (name, size, design, params, hyphen_char, skew_char) = (r.str()?, r.i32()?, r.i32()?, r.all()?, r.i32()?, r.i32()?);
            let metrics = match r.u8()? {
                0 => Metrics::None,
                1 => Metrics::Static(by_name(&name)?),
                2 => Metrics::File(Box::new(FileTfm { bc: r.u8()?, info: r.all()?, widths: r.all()?, heights: r.all()?, depths: r.all()?, italics: r.all()?, lig_kern: r.all()?, kerns: r.all()?, exten: r.all()? })),
                _ => return None,
            };
            let ident = r.u32()?;
            list.push(Font { name, size, design, params, hyphen_char, skew_char, metrics, ident });
        }
        (!list.is_empty()).then_some(Fonts { list })
    }
}

/// Every font loaded so far.
pub struct Fonts {
    pub list: Vec<Font>,
}

impl Default for Fonts {
    /// §552: `\nullfont` has seven zero parameters.
    fn default() -> Self {
        let null = Font { name: String::from("nullfont"), size: 0, design: 0, params: vec![0; 7], hyphen_char: '-' as i32, skew_char: -1, metrics: Metrics::None, ident: u32::MAX };
        Fonts { list: vec![null] }
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
        for (f, font) in self.e.fonts.list.iter().enumerate().skip(1) {
            if font.name == name {
                let size = if s > 0 { s } else { xn_over_d(font.design, -s, 1000) };
                if size == font.size {
                    return f as u16;
                }
            }
        }
        // The embedded TFMs first, then the file `<name>.tfm` from the resolver.
        let (design, raw, metrics) = match by_name(name) {
            Some(tfm) => (tfm.design >> 4, tfm.params.to_vec(), Metrics::Static(tfm)),
            None => match self.read_file.as_ref().and_then(|read| read(&format!("{name}.tfm"))).as_deref().and_then(parse_tfm) {
                Some((design, params, file)) => (design, params, Metrics::File(Box::new(file))),
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
        let scaler = Scaler::new(size);
        // §575: the slant is a plain fraction; the rest scale with the font; at least seven.
        let mut params: Vec<i32> = raw.iter().enumerate().map(|(k, &p)| if k == 0 { p >> 4 } else { scaler.scale(p) }).collect();
        if params.len() < 7 {
            params.resize(7, 0);
        }
        let hyphen_char = self.int_param("defaulthyphenchar");
        let skew_char = self.int_param("defaultskewchar");
        self.e.fonts.list.push(Font { name: name.to_string(), size, design, params, hyphen_char, skew_char, metrics, ident: u32::MAX });
        (self.e.fonts.list.len() - 1) as u16
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
        let font = &mut self.e.fonts.list[f as usize];
        let have = font.params.len();
        let fits = n > 0 && (n as usize <= have || (f as usize == last && f != 0));
        if !fits {
            let msg = format!("Font {} has only {have} fontdimen parameters", font.name);
            self.error(msg);
            return None;
        }
        if n as usize > have {
            font.params.resize(n as usize, 0);
        }
        Some((f, n as usize - 1))
    }

    /// `\fontdimen<n><font>[=]<dimen>` (§1253), global as fonts are.
    pub(super) fn assign_font_dimen(&mut self) {
        let at = self.find_font_dimen();
        self.scan_optional_equals();
        let v = self.scan_dimen(false).0;
        if let Some((f, k)) = at {
            self.e.fonts.list[f as usize].params[k] = v;
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
        let mut s = font.name.clone();
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
}
