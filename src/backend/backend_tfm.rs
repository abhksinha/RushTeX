//! TFM metrics in the file's own layout (tex.web §539–§545).
//!
//! Each character is one raw `char_info` word: width index (8 bits), height index (4),
//! depth index (4), italic index (6), tag (2) and remainder (8). The indices select from the
//! font's small tables of distinct fix_words (2^-20 of the design size), so every dimension is
//! exact and a character costs 4 bytes. [`Scaler`] turns a fix_word into scaled points as
//! TeX's `store_scaled` does (§571).

use crate::backend::scaled::Scaler;

pub use crate::backend::backend_tfm_data::*;

pub struct Tfm {
    /// Design size, a fix_word in points.
    pub design: i32,
    pub info: [u32; 128],
    pub widths: &'static [i32],
    pub heights: &'static [i32],
    pub depths: &'static [i32],
    pub italics: &'static [i32],
    /// fontdimens 1.. (`param[0]` is `\fontdimen1`).
    pub params: &'static [i32],
    /// The lig/kern program, raw four-byte words (§545), and the kerns it indexes.
    pub lig_kern: &'static [u32],
    pub kerns: &'static [i32],
    /// Extensible recipes: top, mid, bot, rep (§546).
    pub exten: &'static [[u8; 4]],
}

/// The embedded TFM of the font file `name` (`cmr10`), as `\\font` loads it (§563).
pub fn by_name(name: &str) -> Option<&'static Tfm> {
    Some(match name.strip_suffix(".tfm").unwrap_or(name) {
        "cmr5" => &CMR5_TFM,
        "cmr6" => &CMR6_TFM,
        "cmr7" => &CMR7_TFM,
        "cmr8" => &CMR8_TFM,
        "cmr9" => &CMR9_TFM,
        "cmr10" => &CMR10_TFM,
        "cmr12" => &CMR12_TFM,
        "cmr17" => &CMR17_TFM,
        "cmmi5" => &CMMI5_TFM,
        "cmmi6" => &CMMI6_TFM,
        "cmmi7" => &CMMI7_TFM,
        "cmmi8" => &CMMI8_TFM,
        "cmmi9" => &CMMI9_TFM,
        "cmmi10" => &CMMI10_TFM,
        "cmmi12" => &CMMI12_TFM,
        "cmmib5" => &CMMIB5_TFM,
        "cmmib7" => &CMMIB7_TFM,
        "cmmib10" => &CMMIB10_TFM,
        "cmsy5" => &CMSY5_TFM,
        "cmsy6" => &CMSY6_TFM,
        "cmsy7" => &CMSY7_TFM,
        "cmsy8" => &CMSY8_TFM,
        "cmsy9" => &CMSY9_TFM,
        "cmsy10" => &CMSY10_TFM,
        "cmbsy5" => &CMBSY5_TFM,
        "cmbsy7" => &CMBSY7_TFM,
        "cmbsy10" => &CMBSY10_TFM,
        "cmex7" => &CMEX7_TFM,
        "cmex8" => &CMEX8_TFM,
        "cmex9" => &CMEX9_TFM,
        "cmex10" => &CMEX10_TFM,
        "cmbx5" => &CMBX5_TFM,
        "cmbx6" => &CMBX6_TFM,
        "cmbx7" => &CMBX7_TFM,
        "cmbx8" => &CMBX8_TFM,
        "cmbx9" => &CMBX9_TFM,
        "cmbx10" => &CMBX10_TFM,
        "cmbx12" => &CMBX12_TFM,
        "cmti7" => &CMTI7_TFM,
        "cmti8" => &CMTI8_TFM,
        "cmti9" => &CMTI9_TFM,
        "cmti10" => &CMTI10_TFM,
        "cmti12" => &CMTI12_TFM,
        "cmsl10" => &CMSL10_TFM,
        "cmcsc10" => &CMCSC10_TFM,
        "cmtt8" => &CMTT8_TFM,
        "cmtt9" => &CMTT9_TFM,
        "cmtt10" => &CMTT10_TFM,
        "cmtt12" => &CMTT12_TFM,
        "cmss8" => &CMSS8_TFM,
        "cmss10" => &CMSS10_TFM,
        "cmssbx10" => &CMSSBX10_TFM,
        "msam10" => &MSAM10_TFM,
        "msbm10" => &MSBM10_TFM,
        "eufm5" => &EUFM5_TFM,
        "eufm7" => &EUFM7_TFM,
        "eufm10" => &EUFM10_TFM,
        _ => return None,
    })
}

/// `char_tag` values (§544).
pub const LIST_TAG: u8 = 2;
pub const EXT_TAG: u8 = 3;

impl Tfm {
    /// Whether `c` exists in the font (§554: a zero width index means absent).
    #[inline(always)]
    pub fn exists(&self, c: u8) -> bool {
        c < 128 && self.info[c as usize] >> 24 != 0
    }

    #[inline(always)]
    fn info(&self, c: u8) -> u32 {
        if c < 128 { self.info[c as usize] } else { 0 }
    }

    #[inline(always)]
    pub fn width(&self, c: u8) -> i32 {
        self.widths[(self.info(c) >> 24) as usize]
    }

    #[inline(always)]
    pub fn height(&self, c: u8) -> i32 {
        self.heights[(self.info(c) >> 20 & 0xF) as usize]
    }

    #[inline(always)]
    pub fn depth(&self, c: u8) -> i32 {
        self.depths[(self.info(c) >> 16 & 0xF) as usize]
    }

    #[inline(always)]
    pub fn italic(&self, c: u8) -> i32 {
        self.italics[(self.info(c) >> 10 & 0x3F) as usize]
    }

    #[inline(always)]
    pub fn tag(&self, c: u8) -> u8 {
        (self.info(c) >> 8 & 3) as u8
    }

    #[inline(always)]
    pub fn remainder(&self, c: u8) -> u8 {
        self.info(c) as u8
    }

    /// `\fontdimen n` (1-based), zero past the font's parameters as in §578.
    #[inline(always)]
    pub fn param(&self, n: usize) -> i32 {
        self.params.get(n.wrapping_sub(1)).copied().unwrap_or(0)
    }

    /// Width, height, depth and italic correction of `c` in scaled points.
    #[inline(always)]
    pub fn dims_sp(&self, c: u8, s: &Scaler) -> [i32; 4] {
        [s.scale(self.width(c)), s.scale(self.height(c)), s.scale(self.depth(c)), s.scale(self.italic(c))]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tfm_tables_are_tex_exact() {
        let s = Scaler::at_pt(10.0);
        // cmr10 `A`: 7.50002pt wide, 6.83331pt high (tftopl).
        assert_eq!(s.scale(CMR10_TFM.width(b'A')), 491521);
        assert_eq!(s.scale(CMR10_TFM.height(b'A')), 447828);
        // cmmi10 `f`: italic correction 1.07637pt, depth 1.94444pt.
        assert_eq!(s.scale(CMMI10_TFM.italic(b'f')), 70543);
        assert_eq!(s.scale(CMMI10_TFM.depth(b'f')), 127431);
        // cmsy10 fontdimen 22 (axis height) 2.5pt, cmex10 `\bigl(` lists to the next size.
        assert_eq!(s.scale(CMSY10_TFM.param(22)), 163840);
        assert_eq!(CMEX10_TFM.tag(0), LIST_TAG);
        assert_eq!(CMEX10_TFM.remainder(0), 16);
        assert!(!CMR10_TFM.exists(128) && CMR10_TFM.exists(b'A'));
    }
}
