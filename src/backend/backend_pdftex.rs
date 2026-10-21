//! pdfTeX's page description (pdftex.web part 32c), translated point to point: how a page's
//! characters and rules become content-stream operators, in scaled points throughout.
//!
//! A PDF viewer places each glyph by adding the `/Widths` of the glyphs before it and the
//! `TJ` displacements, all rounded to the raster pdfTeX writes (`\pdfdecimaldigits` 3 for
//! positions, 1/1000 of the font size for displacements, 1/10000 for `/Widths`). pdfTeX keeps
//! `pdf_delta_h`, the viewer's idea of the current position, and emits a displacement only
//! when TeX's `cur_h` differs from it by half a unit or more. The values (raster, rounding
//! points) are pdfTeX's so the numbers written are the same; the arithmetic is ours: one
//! integer division per quantity, a few words of state, no allocation.

/// `one_hundred_bp` (§ Set init): 100bp in scaled points.
pub const ONE_HUNDRED_BP: i64 = 6578176;
/// `one_bp`: 1bp in scaled points (65781.76, rounded).
pub const ONE_BP: i64 = 65782;
/// `fixed_decimal_digits`: `\pdfdecimaldigits` as pdftex.cfg sets it.
pub const DECIMAL_DIGITS: u32 = 3;
/// `min_bp_val`: `divide_scaled(one_hundred_bp, 10^(digits+2), 0)`, the smallest move written.
pub const MIN_BP_VAL: i64 = 66;

const TEN_POW: [i64; 10] = [1, 10, 100, 1000, 10000, 100000, 1000000, 10000000, 100000000, 1000000000];

/// `(s/m) * 10^dd` rounded half up (away from zero), and `s` moved onto that raster (the
/// value pdfTeX calls `scaled_out`). One 64-bit division: |s| < 2^31 and dd <= 6 keep
/// `s * 10^dd` below 2^53.
#[inline(always)]
pub fn divide_scaled(s: i64, m: i64, dd: u32) -> (i64, i64) {
    let neg = (s < 0) != (m < 0);
    let (sa, ma) = (s.abs(), m.abs());
    let n = sa * TEN_POW[dd as usize];
    let (mut q, mut r) = (n / ma, n % ma);
    if 2 * r >= ma {
        q += 1;
        r -= ma;
    }
    let out = sa - r / TEN_POW[dd as usize];
    if neg {
        (-q, -out)
    } else {
        (q, out)
    }
}

/// `pdf_print_int`.
#[inline]
pub fn print_int(out: &mut Vec<u8>, n: i64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    let mut m = n.unsigned_abs();
    loop {
        i -= 1;
        buf[i] = b'0' + (m % 10) as u8;
        m /= 10;
        if m == 0 {
            break;
        }
    }
    if n < 0 {
        out.push(b'-');
    }
    out.extend_from_slice(&buf[i..]);
}

/// utils.c `extxnoverd`: x·n/d in doubles, rounded half away from zero.
pub fn ext_xn_over_d(x: i64, n: i64, d: i64) -> i64 {
    let r = x as f64 * n as f64 / d as f64;
    (if r > f64::EPSILON { r + 0.5 } else { r - 0.5 }) as i64
}

/// `pdf_print_real`: `m / 10^d`, without trailing zeros.
pub fn print_real(out: &mut Vec<u8>, m: i64, d: u32) {
    if m < 0 {
        out.push(b'-');
    }
    let m = m.abs();
    print_int(out, m / TEN_POW[d as usize]);
    let mut m = m % TEN_POW[d as usize];
    if m > 0 {
        out.push(b'.');
        let mut d = d - 1;
        while m < TEN_POW[d as usize] {
            out.push(b'0');
            d -= 1;
        }
        while m % 10 == 0 {
            m /= 10;
        }
        print_int(out, m);
    }
}

/// `pdf_print_bp`: `s` in big points; returns `scaled_out`.
#[inline]
pub fn print_bp(out: &mut Vec<u8>, s: i64) -> i64 {
    let (q, so) = divide_scaled(s, ONE_HUNDRED_BP, DECIMAL_DIGITS + 2);
    print_real(out, q, DECIMAL_DIGITS);
    so
}

/// `pdf_font_size[f]`: the font size moved onto the raster `Tf` is written with.
#[inline]
pub fn pdf_font_size(size_sp: i64) -> i64 {
    divide_scaled(size_sp, ONE_HUNDRED_BP, 6).1
}

/// `pdf_print_char`: octal for controls, space, `\`, parentheses and the upper half.
#[inline(always)]
pub fn print_char(out: &mut Vec<u8>, c: u8) {
    if c <= 32 || c == b'\\' || c == b'(' || c == b')' || c > 127 {
        out.extend_from_slice(&[b'\\', b'0' + (c >> 6), b'0' + ((c >> 3) & 7), b'0' + (c & 7)]);
    } else {
        out.push(c);
    }
}

/// The state pdfTeX keeps while writing a page's text: `pdf_h`, `pdf_v`, `pdf_tj_start_h`,
/// `pdf_delta_h`, `pdf_doing_text`, `pdf_doing_string` and the current font. Coordinates are
/// PDF ones (y up) in scaled points; the page origin is pdfTeX's `pdf_origin`.
#[derive(Default)]
pub struct TextState {
    v: i64,
    tj_start_h: i64,
    delta_h: i64,
    doing_text: bool,
    doing_string: bool,
    /// Current font: an identity chosen by the caller (resource) and its `font_size`.
    font: Option<(u32, i64)>,
    /// `pdf_font_size[f]` of the current font.
    font_size: i64,
    /// False after text of unknown advance: the viewer's position is then not `delta_h`.
    lost: bool,
    /// `pdf_origin_h`, and `pdf_origin_v` less the page height (down), as `pdf_set_origin`
    /// moved them: by the amounts written, so rounding carries on as in pdfTeX.
    org_h: i64,
    org_v: i64,
    /// `pdf_cur_Tm_a`: the expansion (thousandths) of the text matrix in force.
    tm_a: i64,
}

/// pdfTeX's `round_xn_over_d` on 64-bit values: `x·n/d`, the magnitude rounded half up.
#[inline]
fn rxd(x: i64, n: i64, d: i64) -> i64 {
    let p = x.abs() * n;
    let u = p / d + (2 * (p % d) >= d) as i64;
    if x >= 0 {
        u
    } else {
        -u
    }
}

impl TextState {
    /// `pdf_end_string`.
    #[inline]
    pub fn end_string(&mut self, out: &mut Vec<u8>) {
        if self.doing_string {
            out.extend_from_slice(b")]TJ");
            self.doing_string = false;
        }
    }

    /// `pdf_end_string_nl`, for operators written inside the text object (colour).
    #[inline]
    pub fn end_string_nl(&mut self, out: &mut Vec<u8>) {
        if self.doing_string {
            out.extend_from_slice(b")]TJ\n");
            self.doing_string = false;
        }
    }

    /// `pdf_end_text`.
    #[inline]
    pub fn end_text(&mut self, out: &mut Vec<u8>) {
        if self.doing_text {
            self.end_string_nl(out);
            out.extend_from_slice(b"ET\n");
            self.doing_text = false;
        }
    }

    /// `pdf_begin_text`: the origin is the page's lower left corner.
    fn begin_text(&mut self, out: &mut Vec<u8>) {
        // `pdf_set_origin(0, cur_page_height)`: text is positioned from the page's corner,
        // so an origin moved by a `\pdfliteral` or `\pdfrestore` moves back first.
        self.set_origin(out, 0, 0, 0);
        out.extend_from_slice(b"BT\n");
        self.doing_text = true;
        self.font = None;
        self.doing_string = false;
        self.v = -self.org_v;
        self.tj_start_h = self.org_h;
        self.tm_a = 0;
        // A new text object starts at the origin: the first string sets its position.
        self.lost = true;
    }

    /// `pdf_set_font`, by the caller's font identity `key`, its resource name and its size.
    #[inline]
    pub fn set_font(&mut self, out: &mut Vec<u8>, key: u32, resource: &str, size_sp: i64) {
        // pdf_begin_string begins the text object first: a new BT forgets the font.
        if !self.doing_text {
            self.begin_text(out);
        }
        if self.font == Some((key, size_sp)) {
            return;
        }
        self.end_string(out);
        out.extend_from_slice(resource.as_bytes());
        out.push(b' ');
        let (q, so) = divide_scaled(size_sp, ONE_HUNDRED_BP, 6);
        print_real(out, q, 4);
        out.extend_from_slice(b" Tf");
        self.font = Some((key, size_sp));
        self.font_size = so;
    }

    /// `pdf_set_text_pos`: `Td` from the line start to (h, v), or, for an expanded font (or
    /// back from one), a text matrix `Tm` scaled by `1000 + ea` thousandths at absolute
    /// (`h`, `y`).
    fn set_text_pos(&mut self, out: &mut Vec<u8>, h: i64, y: i64, v: i64, v_out: i64, ea: i64) {
        if ea != 0 || self.tm_a != 0 {
            out.push(b' ');
            print_real(out, 1000 + ea, 3);
            out.extend_from_slice(b" 0 0 1 ");
            self.tj_start_h = self.org_h + print_bp(out, h - self.org_h);
            out.push(b' ');
            self.v = -self.org_v + print_bp(out, y + self.org_v);
            out.extend_from_slice(b" Tm");
            self.tm_a = ea;
            self.delta_h = 0;
            self.lost = false;
            return;
        }
        out.push(b' ');
        let so = print_bp(out, h - self.tj_start_h);
        out.push(b' ');
        print_real(out, v, DECIMAL_DIGITS);
        self.v += v_out;
        out.extend_from_slice(b" Td");
        self.tj_start_h += so;
        self.delta_h = 0;
        self.lost = false;
    }

    /// `pdf_begin_string` for a character at (h, v): moves there by a `TJ` displacement or
    /// a new text position, and leaves a string open. Call [`Self::set_font`] first.
    pub fn begin_string(&mut self, out: &mut Vec<u8>, h: i64, v: i64) {
        self.begin_string_ex(out, h, v, 0);
    }

    /// [`Self::begin_string`] for a character of a font automatically expanded by `ea`
    /// thousandths (pdfTeX's `get_font_auto_expand_ratio`).
    pub fn begin_string_ex(&mut self, out: &mut Vec<u8>, h: i64, v: i64, ea: i64) {
        let mut must_set = false;
        if !self.doing_text {
            self.begin_text(out);
            must_set = true;
        }
        let dh = h - (self.tj_start_h + self.delta_h);
        let (mut s, s_out) = if self.tm_a == 0 {
            divide_scaled(dh, self.font_size, 3)
        } else {
            let a = 1000 + self.tm_a;
            let s = divide_scaled(rxd(dh, 1000, a), self.font_size, 3).0;
            let so = if s.abs() < 0o100000 { rxd(rxd(self.font_size, s.abs(), 1000), a, 1000) * s.signum() } else { 0 };
            (s, so)
        };
        let (dv, v_out) = if (v - self.v).abs() >= MIN_BP_VAL {
            divide_scaled(v - self.v, ONE_HUNDRED_BP, DECIMAL_DIGITS + 2)
        } else {
            (0, 0)
        };
        must_set |= dv != 0 || s.abs() >= 0o100000 || self.lost || ea != self.tm_a;
        if must_set {
            self.end_string(out);
            self.set_text_pos(out, h, v, dv, v_out, ea);
            s = 0;
        }
        if !self.doing_string {
            out.extend_from_slice(b" [");
            if s == 0 {
                out.push(b'(');
            }
        }
        if s != 0 {
            if self.doing_string {
                out.push(b')');
            }
            print_int(out, -s);
            out.push(b'(');
            self.delta_h += s_out;
        }
        self.doing_string = true;
    }

    /// `output_one_char`: character `c` of TeX width `w` at (h, v).
    #[inline]
    pub fn char(&mut self, out: &mut Vec<u8>, c: u8, w: i64, h: i64, v: i64) {
        self.char_ex(out, c, w, h, v, 0);
    }

    /// [`Self::char`] in a font automatically expanded by `ea` thousandths.
    #[inline]
    pub fn char_ex(&mut self, out: &mut Vec<u8>, c: u8, w: i64, h: i64, v: i64, ea: i64) {
        self.begin_string_ex(out, h, v, ea);
        print_char(out, c);
        self.advance(w);
    }

    /// `adv_char_width`: the viewer's advance over a character of TeX width `w`, through
    /// the text matrix's expansion.
    #[inline(always)]
    pub fn advance(&mut self, w: i64) {
        if self.tm_a == 0 {
            self.delta_h += divide_scaled(w, self.font_size, 4).1;
        } else {
            let a = 1000 + self.tm_a;
            let s = divide_scaled(rxd(w, 1000, a), self.font_size, 4).0;
            self.delta_h += rxd(rxd(self.font_size, s.abs(), 10000), a, 1000) * s.signum();
        }
    }

    /// Text of unknown advance was written into the open string: the next character must
    /// set a new text position.
    #[inline]
    pub fn lose_position(&mut self) {
        self.lost = true;
    }

    /// `pdf_set_origin(h, v)`: a lasting move of the origin to TeX's (h, v), `v` downward on
    /// a page of height `page_h`; the origin moves by what was written.
    pub fn set_origin(&mut self, out: &mut Vec<u8>, h: i64, v: i64, page_h: i64) {
        let origin_v = page_h + self.org_v;
        if (h - self.org_h).abs() >= MIN_BP_VAL || (v - origin_v).abs() >= MIN_BP_VAL {
            out.extend_from_slice(b"1 0 0 1 ");
            self.org_h += print_bp(out, h - self.org_h);
            out.push(b' ');
            self.org_v -= print_bp(out, origin_v - v);
            out.extend_from_slice(b" cm\n");
        }
    }

    /// pdfTeX's `literal(s, mode)` at TeX's (h, v): `set_origin` (0) ends the text and moves
    /// the origin there, `direct_page` (1) ends the text, `direct_always` (2) only the string.
    pub fn literal(&mut self, out: &mut Vec<u8>, mode: u8, s: &[u8], h: i64, v: i64, page_h: i64) {
        match mode {
            0 => {
                self.end_text(out);
                self.set_origin(out, h, v, page_h);
            }
            1 => self.end_text(out),
            _ => self.end_string_nl(out),
        }
        out.extend_from_slice(s);
        out.push(b'\n');
    }

    /// pdfTeX's `out_image` for a raster image `/Im<n>`: its box `w` wide and `hd` tall, its
    /// lower left corner at TeX's (x, v) on a page of height `origin_v`.
    pub fn image(&mut self, out: &mut Vec<u8>, n: usize, x: i64, v: i64, origin_v: i64, [w, hd]: [i64; 2]) {
        self.end_text(out);
        out.extend_from_slice(b"q\n");
        print_real(out, ext_xn_over_d(w, TEN_POW[6], ONE_HUNDRED_BP), 4);
        out.extend_from_slice(b" 0 0 ");
        print_real(out, ext_xn_over_d(hd, TEN_POW[6], ONE_HUNDRED_BP), 4);
        out.push(b' ');
        print_bp(out, x - self.org_h);
        out.push(b' ');
        print_bp(out, origin_v + self.org_v - v);
        out.extend_from_slice(b" cm\n/Im");
        print_int(out, n as i64);
        out.extend_from_slice(b" Do\nQ\n");
    }

    /// pdfTeX's `out_image` for an included PDF page `/Im<n>`: its form, `img` sp at natural
    /// size with its box's corner at `orig`, scaled to `w`×`hd` with that corner at TeX's
    /// (x, v).
    #[allow(clippy::too_many_arguments)]
    pub fn form(&mut self, out: &mut Vec<u8>, n: usize, x: i64, v: i64, origin_v: i64, [w, hd]: [i64; 2], img: [i64; 2], orig: [i64; 2]) {
        self.end_text(out);
        out.extend_from_slice(b"q\n");
        print_real(out, ext_xn_over_d(w, TEN_POW[6], img[0]), 6);
        out.extend_from_slice(b" 0 0 ");
        print_real(out, ext_xn_over_d(hd, TEN_POW[6], img[1]), 6);
        out.push(b' ');
        print_bp(out, x - self.org_h - ext_xn_over_d(w, orig[0], img[0]));
        out.push(b' ');
        print_bp(out, origin_v + self.org_v - v - ext_xn_over_d(hd, orig[1], img[1]));
        out.extend_from_slice(b" cm\n/Im");
        print_int(out, n as i64);
        out.extend_from_slice(b" Do\nQ\n");
    }

    /// `pdf_set_rule(x, y, w, h)` with (x, y) the rule's lower left corner.
    /// `pdf_set_rule(x, v, w, h)`: `v` is TeX's (downward) position of the rule's bottom and
    /// `origin_v` the page height (`pdf_origin_v`).
    pub fn rule(&mut self, out: &mut Vec<u8>, x: i64, v: i64, origin_v: i64, w: i64, h: i64) {
        let (x, origin_v) = (x - self.org_h, origin_v + self.org_v);
        self.end_text(out);
        out.extend_from_slice(b"q\n");
        // pdf_set_origin_temp prints `1 0 0 1 x y cm` only for a move of min_bp_val or more.
        let origin = |out: &mut Vec<u8>, x: i64, y: i64| {
            if x.abs() >= MIN_BP_VAL || y.abs() >= MIN_BP_VAL {
                out.extend_from_slice(b"1 0 0 1 ");
                print_bp(out, x);
                out.push(b' ');
                print_bp(out, y);
                out.extend_from_slice(b" cm\n");
            }
        };
        // `(h + 1)/2` is WEB's real division, and the real passed as `scaled` is truncated
        // toward zero by C: `y - (h+1)/2` is `(2y - h - 1) div 2` truncated, not `y - (h+1) div 2`.
        let y = origin_v - v;
        if h <= ONE_BP {
            // A thin rule is a stroke along its middle.
            origin(out, x, origin_v - (2 * v - h - 1) / 2);
            out.extend_from_slice(b"[]0 d 0 J ");
            print_bp(out, h);
            out.extend_from_slice(b" w 0 0 m ");
            print_bp(out, w);
            out.extend_from_slice(b" 0 l S\n");
        } else if w <= ONE_BP {
            origin(out, (2 * x + w + 1) / 2, y);
            out.extend_from_slice(b"[]0 d 0 J ");
            print_bp(out, w);
            out.extend_from_slice(b" w 0 0 m 0 ");
            print_bp(out, h);
            out.extend_from_slice(b" l S\n");
        } else {
            origin(out, x, y);
            out.extend_from_slice(b"0 0 ");
            print_bp(out, w);
            out.push(b' ');
            print_bp(out, h);
            out.extend_from_slice(b" re f\n");
        }
        out.extend_from_slice(b"Q\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divide_scaled_matches_pdftex() {
        // \pdfdecimaldigits 3: min_bp_val is 66sp; 10pt is 9.9626bp.
        assert_eq!(divide_scaled(ONE_HUNDRED_BP, 100000, 0).0, MIN_BP_VAL);
        let mut out = Vec::new();
        print_real(&mut out, divide_scaled(655360, ONE_HUNDRED_BP, 6).0, 4);
        assert_eq!(out, b"9.9626");
        out.clear();
        print_real(&mut out, divide_scaled(-1430226, ONE_HUNDRED_BP, 5).0, 3);
        assert_eq!(out, b"-21.742");
    }

    /// The single division equals pdftex.web's digit-by-digit `divide_scaled`.
    #[test]
    fn divide_scaled_single_division_equals_digit_loop() {
        fn reference(s: i64, m: i64, dd: u32) -> (i64, i64) {
            let sign = if (s < 0) != (m < 0) { -1 } else { 1 };
            let (s, m) = (s.abs(), m.abs());
            let (mut q, mut r) = (s / m, s % m);
            for _ in 0..dd {
                q = 10 * q + (10 * r) / m;
                r = (10 * r) % m;
            }
            if 2 * r >= m {
                q += 1;
                r -= m;
            }
            (sign * q, sign * (s - r / TEN_POW[dd as usize]))
        }
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..200_000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let s = (x as i64 >> 33) % (1 << 30);
            let m = [ONE_HUNDRED_BP, 655360, 653386, 786432, 717619, 1 + (x >> 40) as i64 % 2_000_000][(x % 6) as usize];
            for dd in [0, 3, 4, 5, 6] {
                assert_eq!(divide_scaled(s, m, dd), reference(s, m, dd), "{s} {m} {dd}");
            }
        }
    }
}
