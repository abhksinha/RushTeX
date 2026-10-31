//! TeX scaled-point arithmetic for font metrics.
//!
//! TeX keeps every dimension as an integer number of scaled points (1pt = 65536sp) and
//! converts a font's TFM fix_words to them once, at load time, with `store_scaled`
//! (tex.web §571–§575). Doing the same here makes word widths and interword glue equal
//! to TeX's to the scaled point, so line breaking compares exactly the numbers TeX
//! compares. Layout keeps its `f32` points; a scaled-point sum converts to `f32` without
//! loss below 256pt.


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

#[cfg(test)]
mod tests {
    use super::*;

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
}
