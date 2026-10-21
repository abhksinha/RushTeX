//! TeX scaled-point arithmetic for font metrics.
//!
//! TeX keeps every dimension as an integer number of scaled points (1pt = 65536sp) and
//! converts a font's TFM fix_words to them once, at load time, with `store_scaled`
//! (tex.web §571–§575). Doing the same here makes word widths and interword glue equal
//! to TeX's to the scaled point, so line breaking compares exactly the numbers TeX
//! compares. Layout keeps its `f32` points; a scaled-point sum converts to `f32` without
//! loss below 256pt.

use crate::backend::backend_cm_fix::{self as fix, FixMetrics};
use crate::backend::backend_fonts::{Face, FontFamily};
use crate::backend::optical::OpticalMaster;

/// Scaled points per point.
pub const SP_PER_PT: f32 = 65536.0;

/// The nearest whole number of scaled points to `pt` points.
#[inline(always)]
pub fn to_sp(pt: f32) -> i32 {
    (pt * SP_PER_PT).round() as i32
}

/// `half` (tex.web §100): half of `x`, an odd value rounded up.
#[inline(always)]
pub fn half(x: i32) -> i32 {
    if x & 1 != 0 { (x + 1).div_euclid(2) } else { x / 2 }
}

/// Points of `sp`, exact for any whole number of scaled points below 256pt.
#[inline(always)]
pub fn sp_pt(sp: i32) -> f32 {
    sp as f32 / SP_PER_PT
}

/// Points of `sp` scaled points.
#[inline(always)]
pub fn to_pt(sp: i64) -> f32 {
    sp as f32 / SP_PER_PT
}

/// A page coordinate or box dimension in TeX's scaled points (1pt = 65536sp), exact where an
/// `f32` of points loses odd scaled points beyond 256pt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Sp(pub i32);

impl Sp {
    /// The nearest scaled point to `pt` points (half away from zero).
    #[inline(always)]
    pub fn pt(pt: f32) -> Sp {
        Sp((pt as f64 * 65536.0).round() as i32)
    }

    /// In points.
    #[inline(always)]
    pub fn to_pt(self) -> f32 {
        (self.0 as f64 / 65536.0) as f32
    }

    #[inline(always)]
    pub fn raw(self) -> i64 {
        self.0 as i64
    }
}

/// In points, as TeX shows a dimension (`print_scaled`, §103).
impl std::fmt::Display for Sp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}pt", self.to_pt())
    }
}

impl From<f32> for Sp {
    #[inline(always)]
    fn from(pt: f32) -> Sp {
        Sp::pt(pt)
    }
}

/// Sums saturate at the `i32` range instead of wrapping (TeX goes on with a clamped value
/// after an arithmetic error, §104); debug builds stop, since a position that far off means a
/// sentinel such as an infinite page goal leaked into arithmetic.
impl std::ops::Add for Sp {
    type Output = Sp;
    #[inline(always)]
    fn add(self, o: Sp) -> Sp {
        debug_assert!(self.0.checked_add(o.0).is_some(), "scaled point overflow");
        Sp(self.0.saturating_add(o.0))
    }
}

impl std::ops::Sub for Sp {
    type Output = Sp;
    #[inline(always)]
    fn sub(self, o: Sp) -> Sp {
        debug_assert!(self.0.checked_sub(o.0).is_some(), "scaled point overflow");
        Sp(self.0.saturating_sub(o.0))
    }
}

impl std::ops::AddAssign for Sp {
    #[inline(always)]
    fn add_assign(&mut self, o: Sp) {
        *self = *self + o;
    }
}

impl std::ops::SubAssign for Sp {
    #[inline(always)]
    fn sub_assign(&mut self, o: Sp) {
        *self = *self - o;
    }
}

/// Moves by a dimension in points: the amount is rounded to scaled points once, so a
/// position accumulated in [`Sp`] never drifts the way a sum of `f32` points does.
impl std::ops::Add<f32> for Sp {
    type Output = Sp;
    #[inline(always)]
    fn add(self, pt: f32) -> Sp {
        self + Sp::pt(pt)
    }
}

impl std::ops::Sub<f32> for Sp {
    type Output = Sp;
    #[inline(always)]
    fn sub(self, pt: f32) -> Sp {
        self - Sp::pt(pt)
    }
}

impl std::ops::AddAssign<f32> for Sp {
    #[inline(always)]
    fn add_assign(&mut self, pt: f32) {
        *self += Sp::pt(pt);
    }
}

impl std::ops::SubAssign<f32> for Sp {
    #[inline(always)]
    fn sub_assign(&mut self, pt: f32) {
        *self -= Sp::pt(pt);
    }
}

/// Compares with a dimension in points, exactly.
impl PartialEq<f32> for Sp {
    #[inline(always)]
    fn eq(&self, pt: &f32) -> bool {
        self.0 as f64 == *pt as f64 * 65536.0
    }
}

impl PartialOrd<f32> for Sp {
    #[inline(always)]
    fn partial_cmp(&self, pt: &f32) -> Option<std::cmp::Ordering> {
        (self.0 as f64).partial_cmp(&(*pt as f64 * 65536.0))
    }
}

impl std::ops::Neg for Sp {
    type Output = Sp;
    #[inline(always)]
    fn neg(self) -> Sp {
        Sp(self.0.saturating_neg())
    }
}

/// TeX's conversion of fix_words to scaled points for one font size (tex.web §572).
#[derive(Clone, Copy, Debug)]
pub struct Scaler {
    z: i64,
    alpha: i64,
    beta: i64,
}

impl Scaler {
    /// The scaler for a font loaded at `size_sp` scaled points.
    #[inline]
    pub fn new(size_sp: i32) -> Self {
        let mut z = size_sp.max(1) as i64;
        let mut alpha = 16i64;
        while z >= 0x80_0000 {
            z /= 2;
            alpha += alpha;
        }
        let beta = 256 / alpha;
        Self { z, alpha: alpha * z, beta }
    }

    #[inline]
    pub fn at_pt(size: f32) -> Self {
        Self::new(to_sp(size))
    }

    /// `store_scaled`: the fix_word `f` (2^-20 of the design size) in scaled points.
    #[inline(always)]
    pub fn scale(&self, f: i32) -> i32 {
        let u = f as u32;
        let (a, b, c, d) = ((u >> 24) as i64, ((u >> 16) & 0xFF) as i64, ((u >> 8) & 0xFF) as i64, (u & 0xFF) as i64);
        let sw = (((d * self.z) / 256 + c * self.z) / 256 + b * self.z) / self.beta;
        (if a == 0 { sw } else { sw - self.alpha }) as i32
    }
}

/// `xn_over_d` (tex.web §107): `x * n / d`, truncated toward zero.
#[inline(always)]
pub fn xn_over_d(x: i32, n: i32, d: i32) -> i32 {
    ((x as i64 * n as i64) / d as i64) as i32
}

/// pdfTeX's `round_xn_over_d`: `x * n / d` (`n`, `d` positive), its magnitude rounded half
/// up.
#[inline]
pub fn round_xn_over_d(x: i32, n: i32, d: i32) -> i32 {
    let p = (x as i64).abs() * n as i64;
    let (q, r) = (p / d as i64, p % d as i64);
    let u = (q + (2 * r >= d as i64) as i64) as i32;
    if x >= 0 {
        u
    } else {
        -u
    }
}

/// A math kern or glue of `w` mu (in units of 2^-16 mu) at a math quad of `math_quad` sp:
/// §716 `math_kern`, with `cur_mu = x_over_n(math_quad, 18)` (§732).
#[inline]
pub fn mu_sp(w: i32, math_quad: i32) -> i32 {
    let cur_mu = math_quad / 18;
    let (n, f) = (cur_mu.div_euclid(65536), cur_mu.rem_euclid(65536));
    n * w + xn_over_d(w, f, 65536)
}

/// [`mu_sp`] in points, for `mu` mu at a math quad of `math_quad` points.
#[inline]
pub fn mu_pt(mu: f32, math_quad: f32) -> f32 {
    sp_pt(mu_sp(to_sp(mu), to_sp(math_quad)))
}

/// `round_decimals` (§102): the fraction `.d1d2..dk` in scaled points (2^-16), rounded.
#[inline]
pub const fn round_decimals(digits: &[u8]) -> i32 {
    let mut a: i32 = 0;
    let mut k = if digits.len() < 17 { digits.len() } else { 17 };
    while k > 0 {
        k -= 1;
        a = (a + (digits[k] - b'0') as i32 * 0x20000) / 10;
    }
    (a + 1) / 2
}

/// `1em`, and the fractions of an em latex.ltx writes, as `scan_dimen` reads them.
pub const EM_1: i32 = 1 << 16;
pub const EM_16667: i32 = round_decimals(b"16667");
pub const EM_2222: i32 = round_decimals(b"2222");
pub const EM_2777: i32 = round_decimals(b"2777");

/// `em` (a factor in units of 2^-16, as [`EM_1`]) times `unit_sp`, as `scan_dimen` (§455)
/// multiplies a scanned factor by a unit.
#[inline]
pub fn em_times(em: i32, unit_sp: i64) -> i64 {
    let a = em.unsigned_abs() as i64;
    let v = times_unit(a >> 16, (a & 0xFFFF) as i32, unit_sp);
    if em < 0 { -v } else { v }
}

/// `nx_plus_y(n, x, y)` (§105): `n * x + y`.
#[inline(always)]
fn nx_plus_y(n: i64, x: i64, y: i64) -> i64 {
    n * x + y
}

/// `factor` (a decimal, as written in a class file: `2.5`) times the unit `unit_sp`, as TeX
/// scans `2.5em` (§455): the integer part times the unit plus `xn_over_d(unit, f, 2^16)`.
#[inline]
pub fn times_unit(int_part: i64, frac_sp: i32, unit_sp: i64) -> i64 {
    nx_plus_y(int_part, unit_sp, xn_over_d(unit_sp as i32, frac_sp, 65536) as i64)
}

/// [`times_unit`] for a factor given as `f32` (a short decimal such as `2.2`): its fraction
/// is read as TeX's `round_decimals` reads the written digits.
#[inline]
pub fn times_unit_f(factor: f32, unit_sp: i64) -> i64 {
    let neg = factor < 0.0;
    let a = factor.abs() as f64;
    let int_part = a.trunc() as i64;
    let frac = ((a - a.trunc()) * 65536.0).round() as i32;
    let v = times_unit(int_part, frac, unit_sp);
    if neg { -v } else { v }
}

/// `factor` em (or ex, or any unit) of `unit` points, as TeX scans `<factor><unit>`
/// (§455), in points: what layout's `2.5 * em` means in TeX.
#[inline]
pub fn em_mul(factor: f32, unit: f32) -> f32 {
    let v = times_unit_f(factor, (unit as f64 * 65536.0).round() as i64);
    (v as f64 / 65536.0) as f32
}

/// TeX's `scan_dimen` (§448–§458) of a written dimension (`-1.5pt`, `2.5em`, `1in`) in scaled
/// points: the decimal fraction by `round_decimals`, physical units by their exact ratio to
/// the point, `em`/`ex` from the font's quad and x-height in sp. A bare number is in pt.
pub fn scan_dimen(text: &str, em_sp: i64, ex_sp: i64) -> Option<i64> {
    let t = text.trim();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r.trim_start()),
        None => (false, t.strip_prefix('+').unwrap_or(t).trim_start()),
    };
    let digits_end = t.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',')).unwrap_or(t.len());
    let (num, unit) = (&t[..digits_end], t[digits_end..].trim());
    let unit = unit.strip_prefix("true").map_or(unit, str::trim_start);
    let (ip, fp) = match num.find(['.', ',']) {
        Some(k) => (&num[..k], &num[k + 1..]),
        None => (num, ""),
    };
    if ip.is_empty() && fp.is_empty() {
        return None;
    }
    let mut int_part: i64 = if ip.is_empty() { 0 } else { ip.parse().ok()? };
    let mut f = round_decimals(fp.as_bytes()) as i64;
    let v = match unit {
        "em" => times_unit(int_part, f as i32, em_sp),
        "ex" => times_unit(int_part, f as i32, ex_sp),
        "sp" => int_part,
        _ => {
            let (n, d): (i64, i64) = match unit {
                "pt" | "" => (1, 1),
                "in" => (7227, 100),
                "pc" => (12, 1),
                "cm" => (7227, 254),
                "mm" => (7227, 2540),
                "bp" => (7227, 7200),
                "dd" => (1238, 1157),
                "cc" => (14856, 1157),
                _ => return None,
            };
            if n != 1 || d != 1 {
                // §458: cur_val := xn_over_d(cur_val, num, denom) with its remainder carried
                // into the fraction.
                let q = int_part * n;
                let rem = q % d;
                int_part = q / d;
                f = (n * f + 65536 * rem) / d;
                int_part += f / 65536;
                f %= 65536;
            }
            int_part * 65536 + f
        }
    };
    Some(if neg { -v } else { v })
}

/// The TFM metrics behind an embedded font's `/Widths` table, found by the table's identity,
/// so the font dictionary and the text cursor agree on every advance. `None` for fonts
/// without TFM data here (the math and AMS fonts), whose integer `/Widths` stay as they are.
pub fn exact_widths(widths: &[u16]) -> Option<ExactWidths> {
    use crate::backend::backend_fonts as f;
    use crate::backend::backend_tfm as t;
    let p = widths.as_ptr();
    const PT: i32 = 65536;
    let base: [(&[u16; 128], &'static FixMetrics, i32); 6] = [
        (&f::CMR10_WIDTHS.0, &fix::CMR10_FIX, 10 * PT),
        (&f::CMBX10_WIDTHS.0, &fix::CMBX10_FIX, 10 * PT),
        (&f::CMTI10_WIDTHS.0, &fix::CMTI10_FIX, 10 * PT),
        (&f::CMTT10_WIDTHS.0, &fix::CMTT10_FIX, 10 * PT),
        (&f::CMR7_WIDTHS.0, &fix::CMR7_FIX, 7 * PT),
        (&f::CMR5_WIDTHS.0, &fix::CMR5_FIX, 5 * PT),
    ];
    if let Some(&(_, m, design)) = base.iter().find(|(t, ..)| std::ptr::eq(t.as_ptr(), p)) {
        return Some(ExactWidths::Fix(m, design));
    }
    // The math fonts, from their TFM files.
    let math: [(&[u16; 128], &'static t::Tfm); 9] = [
        (&f::CMMI10_WIDTHS.0, &t::CMMI10_TFM),
        (&f::CMSY10_WIDTHS.0, &t::CMSY10_TFM),
        (&f::CMEX10_WIDTHS.0, &t::CMEX10_TFM),
        (&f::CMMI7_WIDTHS.0, &t::CMMI7_TFM),
        (&f::CMMI5_WIDTHS.0, &t::CMMI5_TFM),
        (&f::CMSY7_WIDTHS.0, &t::CMSY7_TFM),
        (&f::CMSY5_WIDTHS.0, &t::CMSY5_TFM),
        (&f::MSAM10_WIDTHS.0, &t::MSAM10_TFM),
        (&f::MSBM10_WIDTHS.0, &t::MSBM10_TFM),
    ];
    if let Some(&(_, m)) = math.iter().find(|(t, _)| std::ptr::eq(t.as_ptr(), p)) {
        return Some(ExactWidths::Tfm(m));
    }
    if let Some(i) = f::EXTRA_FACE_WIDTHS.iter().position(|t| std::ptr::eq(t.0.as_ptr(), p)) {
        let tfm = crate::math::math_tfm::face_tfm(Face::from_u8(i as u8 + 18));
        return match f::EXTRA_FACES[i].fix {
            Some(m) => Some(ExactWidths::Fix(m, tfm.map_or(10 * PT, |t| t.design >> 4))),
            None => tfm.map(ExactWidths::Tfm),
        };
    }
    (0..OpticalMaster::COUNT as u8)
        .map(OpticalMaster::from_u8)
        .find(|m| std::ptr::eq(m.widths().as_ptr(), p))
        .map(ExactWidths::of_master)
}

/// Where a font's exact TFM widths come from: the text fonts' fix tables or a math font's TFM.
#[derive(Clone, Copy)]
pub enum ExactWidths {
    /// A text font's fix_words and its design size in sp.
    Fix(&'static FixMetrics, i32),
    Tfm(&'static crate::backend::backend_tfm::Tfm),
}

impl ExactWidths {
    /// The widths of design-size master `m`.
    pub fn of_master(m: crate::backend::optical::OpticalMaster) -> ExactWidths {
        ExactWidths::Fix(m.fix(), crate::math::math_tfm::master_tfm(m).map_or(10 * 65536, |t| t.design >> 4))
    }

    /// The design size in scaled points.
    #[inline(always)]
    pub fn design_sp(self) -> i32 {
        match self {
            ExactWidths::Fix(_, d) => d,
            ExactWidths::Tfm(t) => t.design >> 4,
        }
    }

    /// The width of `code` as a fix_word of the design size.
    #[inline(always)]
    pub fn width(self, code: u8) -> i32 {
        match self {
            ExactWidths::Fix(m, _) => m.widths[code as usize & 127] as i32,
            ExactWidths::Tfm(t) => t.width(code & 127),
        }
    }
}

/// The advance of `code` as pdfTeX writes it in `/Widths`, in 1/10000 em: the width scaled
/// to the font's size, divided by `pdf_font_size` (the size on the 10^-6 bp raster) to four
/// digits (writefont.c `create_charwidth_array`). `size_sp` is the size of the font's first
/// use; 0 means its design size.
#[inline]
pub fn width_tenths(m: ExactWidths, code: u8, size_sp: i32) -> i64 {
    let z = if size_sp != 0 { size_sp } else { m.design_sp() };
    let w = Scaler::new(z).scale(m.width(code)) as i64;
    crate::backend::backend_pdftex::divide_scaled(w, crate::backend::backend_pdftex::pdf_font_size(z as i64), 4).0
}

/// The TFM metrics of `face` at `size`: the design-size master when one applies, else the
/// face's 10pt font. `None` outside Computer Modern and for the math faces.
#[inline]
pub fn fix_metrics(family: FontFamily, face: Face, size: f32) -> Option<&'static FixMetrics> {
    if family != FontFamily::ComputerModern {
        return None;
    }
    if let Some(m) = OpticalMaster::select(family, face, size) {
        return Some(m.fix());
    }
    Some(match face {
        Face::Roman => &fix::CMR10_FIX,
        Face::Bold | Face::BoldItalic => &fix::CMBX10_FIX,
        Face::Italic => &fix::CMTI10_FIX,
        Face::Mono | Face::MonoBold => &fix::CMTT10_FIX,
        Face::Roman7 => &fix::CMR7_FIX,
        Face::Roman5 => &fix::CMR5_FIX,
        _ => return crate::backend::fonts::EXTRA_FACES.get((face as usize).checked_sub(18)?)?.fix,
    })
}

/// The italic correction (`\/`) of character `ch` of `face` at `size`, in points; 0 when the
/// font has none or no TFM data.
#[inline]
pub fn italic_correction_pt(family: FontFamily, face: Face, size: f32, ch: char) -> f32 {
    let c = ch as usize;
    match fix_metrics(family, face, size) {
        Some(m) if c < 128 && m.ics[c] != 0 => to_pt(Scaler::at_pt(size).scale(m.ics[c] as i32) as i64),
        _ => 0.0,
    }
}

/// Whether `face` at `size` slants (`\fontdimen1 > 0`), the test of latex.ltx's `\maybe@ic@`;
/// the italic faces when the font has no TFM data.
#[inline]
pub fn has_slant(family: FontFamily, face: Face, size: f32) -> bool {
    match fix_metrics(family, face, size) {
        Some(m) => m.params[0] > 0,
        None => face.is_italic(),
    }
}

/// `\fontdimen2` (the interword space) of `face` at `size`, in points, for fonts with TFM
/// data.
#[inline]
pub fn space_pt(family: FontFamily, face: Face, size: f32) -> Option<f32> {
    fix_metrics(family, face, size).map(|m| to_pt(Scaler::at_pt(size).scale(m.params[1]) as i64))
}

/// `\fontdimen6` (the quad, 1em) of the roman font at `size`, in points; `size` for fonts
/// without TFM data.
#[inline]
pub fn quad_pt(family: FontFamily, size: f32) -> f32 {
    match fix_metrics(family, Face::Roman, size) {
        Some(m) => to_pt(Scaler::at_pt(size).scale(m.params[5]) as i64),
        None => size,
    }
}

/// `\fontdimen5` (the x-height, 1ex) of the roman font at `size`, in points; half of `size`
/// for fonts without TFM data.
#[inline]
pub fn x_height_pt(family: FontFamily, size: f32) -> f32 {
    match fix_metrics(family, Face::Roman, size) {
        Some(m) => to_pt(Scaler::at_pt(size).scale(m.params[4]) as i64),
        None => 0.5 * size,
    }
}

/// `\fontdimen6` of `face` at `size`, in points; the roman quad for faces without TFM data.
#[inline]
pub fn face_quad_pt(family: FontFamily, face: Face, size: f32) -> f32 {
    to_pt(font_dimen_sp(family, face, size, 6))
}

/// `\fontdimen<n>` of `face` at `size` in scaled points. Faces without TFM data take the
/// interword glue of [`space_glue`], the roman quad, or half the size as x-height.
pub fn font_dimen_sp(family: FontFamily, face: Face, size: f32, n: u8) -> i64 {
    if let Some(m) = fix_metrics(family, face, size) {
        if let Some(&p) = m.params.get(n.wrapping_sub(1) as usize) {
            return Scaler::at_pt(size).scale(p) as i64;
        }
    }
    let g = space_glue(family, face, size);
    match n {
        2 => g.space as i64,
        3 => g.stretch as i64,
        4 => g.shrink as i64,
        7 => g.extra as i64,
        5 => to_sp(x_height_pt(family, size)) as i64,
        6 => to_sp(quad_pt(family, size)) as i64,
        _ => 0,
    }
}

/// Interword glue of a font (fontdimens 2–4 and 7), in scaled points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpaceGlue {
    pub space: i32,
    pub stretch: i32,
    pub shrink: i32,
    pub extra: i32,
}

impl SpaceGlue {
    /// The glue TeX appends for a space at space factor `sf` (tex.web §1041–§1044).
    #[inline]
    pub fn at_space_factor(self, sf: u16) -> (i32, i32, i32) {
        if sf == 1000 || sf == 0 {
            return (self.space, self.stretch, self.shrink);
        }
        let sf = sf as i32;
        let width = if sf >= 2000 { self.space + self.extra } else { self.space };
        (width, xn_over_d(self.stretch, sf, 1000), xn_over_d(self.shrink, 1000, sf))
    }
}

/// Interword glue of `face` at `size` in `family`. Computer Modern uses its TFM fontdimens;
/// the Base-14 families use the fontinst values of their TeX fonts (`ptmr8t` and kin:
/// stretch 0.6 and shrink 0.24 of the space, extra space equal to the shrink; monospaced
/// faces do not stretch and add a full space after sentences).
pub fn space_glue(family: FontFamily, face: Face, size: f32) -> SpaceGlue {
    if let Some(m) = fix_metrics(family, face, size) {
        let s = Scaler::at_pt(size);
        return SpaceGlue {
            space: s.scale(m.params[1]),
            stretch: s.scale(m.params[2]),
            shrink: s.scale(m.params[3]),
            extra: s.scale(m.params[6]),
        };
    }
    let units = crate::backend::fonts::resolve_space_units(family, face) as f32;
    let per_unit = size * 0.001;
    let space = to_sp(units * per_unit);
    if face.is_mono() {
        return SpaceGlue { space, stretch: 0, shrink: 0, extra: space };
    }
    SpaceGlue {
        space,
        stretch: to_sp(units * 0.6 * per_unit),
        shrink: to_sp(units * 0.24 * per_unit),
        extra: to_sp(units * 0.24 * per_unit),
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn sp_sums_saturate() {
        assert_eq!(Sp(5) + Sp(7), Sp(12));
        assert_eq!(Sp(i32::MIN) - Sp(0), Sp(i32::MIN));
        assert_eq!(-Sp(i32::MIN), Sp(i32::MAX));
        assert_eq!(Sp::pt(f32::INFINITY), Sp(i32::MAX));
        assert_eq!(Sp::pt(f32::NEG_INFINITY), Sp(i32::MIN));
    }

    #[test]
    fn scan_dimen_is_tex_exact() {
        // cmr10's quad is 10.00002pt (655361sp): article's \leftmargini 2.5em is 25.00003pt.
        assert_eq!(scan_dimen("2.5em", 655361, 282168), Some(1638402));
        assert_eq!(scan_dimen("1in", 0, 0), Some(4736286));
        assert_eq!(scan_dimen("0.4pt", 0, 0), Some(26214));
        assert_eq!(scan_dimen("-1.5pt", 0, 0), Some(-98304));
        assert_eq!(scan_dimen("10bp", 0, 0), Some(657817));
        assert_eq!(times_unit_f(2.2, 655361), scan_dimen("2.2em", 655361, 0).unwrap());
        assert_eq!(times_unit_f(1.87, 655361), scan_dimen("1.87em", 655361, 0).unwrap());
    }

    use super::*;

    #[test]
    fn store_scaled_matches_tex() {
        // cmr10 at 10pt: the width of `a` (0.5em) is 5pt = 327680sp, the interword space
        // 3.33333pt = 218453sp (TeX's \fontdimen2 of \tenrm).
        let s = Scaler::at_pt(10.0);
        let a = fix::CMR10_FIX.widths[b'a' as usize] as i32;
        assert_eq!(s.scale(a), 327681);
        let g = space_glue(FontFamily::ComputerModern, Face::Roman, 10.0);
        assert_eq!((g.space, g.stretch, g.shrink), (218453, 109226, 72818));
        // After a period (space factor 3000): extra space, triple stretch, a third the shrink.
        assert_eq!(g.at_space_factor(3000), (218453 + 72818, 327678, 24272));
        // cmbx12 at 14.4pt.
        let g = space_glue(FontFamily::ComputerModern, Face::Bold, 14.4);
        assert_eq!(g.space, Scaler::new(943718).scale(fix::CMBX12_FIX.params[1]));
    }
}
