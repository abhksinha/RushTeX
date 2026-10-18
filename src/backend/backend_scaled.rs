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

/// Points of `sp` scaled points.
#[inline(always)]
pub fn to_pt(sp: i64) -> f32 {
    sp as f32 / SP_PER_PT
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
        Face::SmallCaps => &fix::CMCSC10_FIX,
        Face::Roman7 => &fix::CMR7_FIX,
        Face::Roman5 => &fix::CMR5_FIX,
        _ => return None,
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

/// `\fontdimen6` (the quad, 1em) of the roman font at `size`, in points; `size` for fonts
/// without TFM data.
#[inline]
pub fn quad_pt(family: FontFamily, size: f32) -> f32 {
    match fix_metrics(family, Face::Roman, size) {
        Some(m) => to_pt(Scaler::at_pt(size).scale(m.params[5]) as i64),
        None => size,
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
