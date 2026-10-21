//! Metafont's packed bitmap fonts (PK) as pdfTeX embeds them: the character rasters
//! (pdftex `pkin.c` `readchar` and `unpack`), the resolution of a font's PK file (kpathsea
//! `magstep_fix`), and the scaling of a Type 3 font made from them (`writet3.c`).
//!
//! Only the characters asked for are unpacked; the others are skipped by their packet
//! length. Each raster row is a run of 16-bit words, high bit first, as pkin.c keeps it.

use super::backend_pdftex::divide_scaled;

/// One character's bitmap: its code, size in pixels, offsets (`hoff`, `voff`), and rows.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PkChar {
    pub code: u8,
    pub width: i32,
    pub height: i32,
    pub xoff: i32,
    pub yoff: i32,
    pub raster: Vec<u16>,
}

/// pkin.c's bit and nybble reader over a packet.
struct Bits<'a> {
    s: &'a [u8],
    i: usize,
    input: u16,
    weight: u16,
    dynf: u16,
    repeat: u32,
    /// `pk_remainder` while a huge count is handed out in pieces.
    remainder: i64,
}

impl Bits<'_> {
    fn byte(&mut self) -> u16 {
        let b = self.s.get(self.i).copied().unwrap_or(0);
        self.i += 1;
        b as u16
    }

    fn nyb(&mut self) -> u32 {
        if self.weight == 0 {
            self.weight = 16;
            self.input = self.byte();
            (self.input >> 4) as u32
        } else {
            self.weight = 0;
            (self.input & 15) as u32
        }
    }

    fn bit(&mut self) -> bool {
        self.weight >>= 1;
        if self.weight == 0 {
            self.input = self.byte();
            self.weight = 128;
        }
        self.input & self.weight != 0
    }

    /// `pkpackednum` (and `rest` while a huge count lasts).
    fn packed(&mut self) -> u32 {
        if self.remainder != 0 {
            return self.rest();
        }
        let dynf = self.dynf as u32;
        let mut i = self.nyb();
        if i == 0 {
            let mut j;
            loop {
                j = self.nyb();
                i += 1;
                if j != 0 {
                    break;
                }
            }
            if i > 3 {
                // `handlehuge`
                let mut j = j as i64;
                while i > 0 {
                    j = (j << 4) + self.nyb() as i64;
                    i -= 1;
                }
                self.remainder = j - 15 + (13 - dynf as i64) * 16 + dynf as i64;
                return self.rest();
            }
            while i > 0 {
                j = j * 16 + self.nyb();
                i -= 1;
            }
            j + (13 - dynf) * 16 + dynf - 15
        } else if i <= dynf {
            i
        } else if i < 14 {
            (i - dynf - 1) * 16 + self.nyb() + dynf + 1
        } else {
            self.repeat = if i == 14 { self.packed() } else { 1 };
            self.packed()
        }
    }

    fn rest(&mut self) -> u32 {
        if self.remainder < 0 {
            self.remainder = -self.remainder;
            0
        } else if self.remainder > 4000 {
            self.remainder = 4000 - self.remainder;
            4000
        } else {
            let i = self.remainder;
            self.remainder = 0;
            i as u32
        }
    }
}

/// pkin.c `gpower`: `2^k - 1`.
fn gpower(k: u32) -> u32 {
    (1u32 << k) - 1
}

/// pkin.c `unpack`: the raster of a `w`×`h` character whose packed data starts at `s`.
fn unpack(s: &[u8], flag: u8, w: i32, h: i32) -> Vec<u16> {
    let words = ((w + 15) / 16) as usize;
    let mut raster = Vec::with_capacity(words * h.max(0) as usize);
    let mut b = Bits { s, i: 0, input: 0, weight: 0, dynf: (flag >> 4) as u16, repeat: 0, remainder: 0 };
    let mut turn_on = flag & 8 != 0;
    if b.dynf == 14 {
        for _ in 0..h {
            let (mut word, mut ww) = (0u16, 0x8000u16);
            for _ in 0..w {
                if b.bit() {
                    word |= ww;
                }
                ww >>= 1;
                if ww == 0 {
                    raster.push(word);
                    (word, ww) = (0, 0x8000);
                }
            }
            if ww != 0x8000 {
                raster.push(word);
            }
        }
        return raster;
    }
    let (mut rows_left, mut hbit) = (h, w as u32);
    let (mut word, mut ww) = (0u32, 16u32);
    while rows_left > 0 {
        let mut count = b.packed();
        while count != 0 {
            if count < ww && count < hbit {
                if turn_on {
                    word += gpower(ww) - gpower(ww - count);
                }
                hbit -= count;
                ww -= count;
                count = 0;
            } else if count >= hbit && hbit <= ww {
                if turn_on {
                    word += gpower(ww) - gpower(ww - hbit);
                }
                raster.push(word as u16);
                // The finished row, repeated.
                let row = raster.len() - words;
                for _ in 0..b.repeat {
                    raster.extend_from_within(row..row + words);
                }
                rows_left -= b.repeat as i32 + 1;
                b.repeat = 0;
                (word, ww) = (0, 16);
                count -= hbit;
                hbit = w as u32;
            } else {
                if turn_on {
                    word += gpower(ww);
                }
                raster.push(word as u16);
                word = 0;
                count -= ww;
                hbit -= ww;
                ww = 16;
            }
        }
        turn_on = !turn_on;
    }
    raster
}

fn be(s: &[u8], at: usize, n: usize, signed: bool) -> i64 {
    let mut v = 0i64;
    for k in 0..n {
        v = v << 8 | *s.get(at + k).unwrap_or(&0) as i64;
    }
    if signed && n < 8 && v >> (8 * n - 1) & 1 == 1 {
        v -= 1 << (8 * n);
    }
    v
}

/// The characters of PK file `s` whose codes are set in `wanted` (a 256-bit set), in file
/// order (pkin.c `readchar` until `post`).
pub fn pk_chars(s: &[u8], wanted: &[u64; 4]) -> Option<Vec<PkChar>> {
    if s.first() != Some(&247) || s.get(1) != Some(&89) {
        return None;
    }
    let mut i = 3 + *s.get(2)? as usize + 16;
    let mut out = Vec::new();
    loop {
        let flag = *s.get(i)?;
        i += 1;
        match flag {
            245 => return Some(out),
            240..=243 => {
                let n = (flag - 239) as usize;
                i += n + be(s, i, n, false) as usize;
            }
            244 => i += 4,
            246 => {}
            247..=255 => return None,
            _ => {
                // The packet: its length counts the bytes from the `tfm` width on (pktype.web).
                let (len_n, head) = match flag & 7 {
                    0..=3 => (1, [1, 3, 1, 1, 1, 1, 1]),
                    4..=6 => (2, [1, 3, 2, 2, 2, 2, 2]),
                    _ => (4, [4, 4, 4, 4, 4, 4, 4]),
                };
                let pl = if len_n == 4 { be(s, i, 4, false) } else { ((flag & 3) as i64) << (8 * len_n) | be(s, i, len_n, false) } as usize;
                i += len_n;
                let end = i + head[0] + pl;
                // cc, tfm, dx (and dy in the long form), w, h, hoff, voff.
                let mut at = i;
                let mut v = [0i64; 7];
                for (k, n) in head.iter().enumerate() {
                    v[k] = be(s, at, *n, (len_n != 1 && k >= 2) || k >= 5);
                    at += n;
                    if k == 2 && len_n == 4 {
                        at += 4;
                    }
                }
                let (code, w, h, xoff, yoff) = (v[0], v[3] as i32, v[4] as i32, v[5] as i32, v[6] as i32);
                if (0..256).contains(&code) && wanted[code as usize / 64] >> (code % 64) & 1 == 1 {
                    let raster = if w > 0 && h > 0 { unpack(s.get(at..end)?, flag, w, h) } else { Vec::new() };
                    out.push(PkChar { code: code as u8, width: w, height: h, xoff, yoff, raster });
                }
                i = end;
            }
        }
    }
}

/// kpathsea `magstep`.
fn magstep(n: i32, bdpi: i32) -> i32 {
    let (neg, mut n) = (n < 0, n.abs());
    let mut t = if n & 1 == 1 {
        n &= !1;
        1.095445115
    } else {
        1.0
    };
    while n > 8 {
        n -= 8;
        t *= 2.0736;
    }
    while n > 0 {
        n -= 2;
        t *= 1.2;
    }
    (0.5 + if neg { bdpi as f64 / t } else { bdpi as f64 * t }) as i32
}

/// writet3.c `writepk`: the resolution of the PK file for a font at `size` designed at
/// `dsize`, with `\pdfpkresolution` `res` (kpathsea `magstep_fix`).
pub fn pk_dpi(size: i64, dsize: i64, res: i32) -> i32 {
    let dpi = (res as f32 * (size as f32 / dsize as f32)).round() as i32;
    let sign = if dpi < res { -1 } else { 1 };
    for m in 0..40 {
        let mdpi = magstep(m * sign, res);
        if (mdpi - dpi).abs() <= 1 {
            return mdpi;
        }
        if (mdpi - dpi) * sign > 0 {
            return dpi;
        }
    }
    dpi
}

/// writet3.c `get_pk_font_scale` of a font at `size` sp: the font matrix's scale in
/// 10^-5, with `\pdfpkresolution` `res` and `\pdfdecimaldigits` `dd`.
pub fn pk_font_scale(size: i64, res: i64, dd: u32) -> i64 {
    let factor = divide_scaled(72, res, 5 + dd).0;
    // A font too small for the raster (only ever loaded, never shown) gets none.
    match divide_scaled(size, 6_578_176, dd + 2).0 {
        0 => 0,
        m => divide_scaled(factor, m, 0).0,
    }
}

/// writet3.c `pk_char_width`: a character's width `w` in glyph space, in hundredths.
pub fn pk_char_width(w: i64, size: i64, scale: i64) -> i64 {
    divide_scaled(divide_scaled(w, size, 7).0, scale, 0).0
}

/// writet3.c `getpkcharwidth`: the viewer's advance in sp over a character of width `w`
/// (a C `double` truncated to `scaled`).
pub fn pk_advance(w: i64, size: i64, scale: i64) -> i64 {
    ((scale as f64 / 100000.0) * (pk_char_width(w, size, scale) as f64 / 100.0) * size as f64) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `RT_PK=file.pk cargo test --release --lib pk_file -- --ignored --nocapture`: every
    /// character of a PK file unpacked, with its size.
    #[test]
    #[ignore]
    fn pk_file() {
        let data = std::fs::read(std::env::var("RT_PK").unwrap()).unwrap();
        let chars = pk_chars(&data, &[u64::MAX; 4]).unwrap();
        for c in &chars {
            let words = ((c.width + 15) / 16).max(0) as usize;
            assert_eq!(c.raster.len(), words * c.height.max(0) as usize, "char {}", c.code);
            println!("{} {}x{} {} {}", c.code, c.width, c.height, c.xoff, c.yoff);
        }
    }

    #[test]
    fn magsteps() {
        assert_eq!(pk_dpi(655360, 655360, 600), 600);
        // magstep 1 at 600dpi is 720.
        assert_eq!(pk_dpi(786432, 655360, 600), 720);
        assert_eq!(pk_dpi(589824, 655360, 600), 540);
    }

    #[test]
    fn rasters() {
        // A 3×2 character, dynf 14 (bitmap): rows 101 and 010.
        assert_eq!(unpack(&[0xA8], 14 << 4, 3, 2), vec![0xA000, 0x4000]);
        // The same as runs of one (dynf 13, black first).
        assert_eq!(unpack(&[0x11, 0x11, 0x11], (13 << 4) | 8, 3, 2), vec![0xA000, 0x4000]);
    }
}
