//! PNG images as pdfTeX writes them (writepng.c `read_png_info`, `write_png` and its
//! `write_png_*` writers), in our own form: the file is decoded to rows (inflate, the five
//! filters, Adam7), then laid out as the PDF image pdfTeX makes of it. A colour type with
//! alpha (or a `tRNS` chunk, which libpng turns into alpha) becomes the colour samples and a
//! separate `/DeviceGray` soft mask; 16-bit samples keep their high byte unless
//! `\pdfimagehicolor` is on in PDF 1.5 or later. The decoded pixels are what pdfTeX's are;
//! the compressed bytes need not be.

use crate::backend::backend_inflate::zlib_decompress;

/// What `read_png_info` takes from the header: size, resolution in dpi (0 when the file has
/// no `pHYs`), and whether it has alpha (the page then needs a transparency group).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PngInfo {
    pub width: u32,
    pub height: u32,
    pub x_res: i32,
    pub y_res: i32,
    pub alpha: bool,
}

/// The colour space of a decoded image.
#[derive(Debug, Clone, PartialEq)]
pub enum ColorSpace {
    Gray,
    Rgb,
    /// `[/Indexed /DeviceRGB n palette]`, the palette as RGB triples.
    Indexed(Vec<u8>),
}

/// An image ready for a PDF XObject: rows of samples, and the soft mask when there is one.
#[derive(Debug, Clone)]
pub struct PdfImage {
    pub width: u32,
    pub height: u32,
    pub bits: u8,
    pub space: ColorSpace,
    pub data: Vec<u8>,
    pub smask: Option<Vec<u8>>,
}

const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

struct Chunks<'a> {
    ihdr: &'a [u8],
    plte: &'a [u8],
    trns: Option<&'a [u8]>,
    phys: Option<&'a [u8]>,
    idat: Vec<u8>,
}

fn chunks(png: &[u8]) -> Option<Chunks<'_>> {
    if png.get(..8)? != SIGNATURE {
        return None;
    }
    let mut c = Chunks { ihdr: &[], plte: &[], trns: None, phys: None, idat: Vec::new() };
    let mut at = 8;
    while at + 8 <= png.len() {
        let len = u32::from_be_bytes(png[at..at + 4].try_into().ok()?) as usize;
        let kind = &png[at + 4..at + 8];
        let body = png.get(at + 8..at + 8 + len)?;
        match kind {
            b"IHDR" => c.ihdr = body,
            b"PLTE" => c.plte = body,
            b"tRNS" => c.trns = Some(body),
            b"pHYs" => c.phys = Some(body),
            b"IDAT" => c.idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    (c.ihdr.len() == 13).then_some(c)
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// `read_png_info`: the size, `round(0.0254 · pixels per metre)` when `pHYs` is given, and
/// whether the colour type carries alpha.
pub fn png_info(png: &[u8]) -> Option<PngInfo> {
    let c = chunks(png)?;
    let (x_res, y_res) = match c.phys {
        Some(p) if p.len() >= 9 => ((0.0254 * be32(&p[0..4]) as f64).round() as i32, (0.0254 * be32(&p[4..8]) as f64).round() as i32),
        _ => (0, 0),
    };
    Some(PngInfo { width: be32(&c.ihdr[0..4]), height: be32(&c.ihdr[4..8]), x_res, y_res, alpha: c.ihdr[9] & 4 != 0 })
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i16 + b as i16 - c as i16;
    let (pa, pb, pc) = ((p - a as i16).abs(), (p - b as i16).abs(), (p - c as i16).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Undoes the filters of `rows` rows of `stride` bytes (each after its filter byte), `bpp`
/// bytes per complete pixel (at least 1), from `raw` at `*at`.
fn unfilter(raw: &[u8], at: &mut usize, rows: usize, stride: usize, bpp: usize) -> Option<Vec<u8>> {
    let mut out = vec![0u8; rows * stride];
    for y in 0..rows {
        let filter = *raw.get(*at)?;
        let line = raw.get(*at + 1..*at + 1 + stride)?;
        *at += 1 + stride;
        let (done, cur) = out.split_at_mut(y * stride);
        let prev = if y > 0 { &done[(y - 1) * stride..] } else { &[][..] };
        let cur = &mut cur[..stride];
        for x in 0..stride {
            let a = if x >= bpp { cur[x - bpp] } else { 0 };
            let b = prev.get(x).copied().unwrap_or(0);
            let c = if x >= bpp { prev.get(x - bpp).copied().unwrap_or(0) } else { 0 };
            cur[x] = line[x].wrapping_add(match filter {
                0 => 0,
                1 => a,
                2 => b,
                3 => ((a as u16 + b as u16) / 2) as u8,
                4 => paeth(a, b, c),
                _ => return None,
            });
        }
    }
    Some(out)
}

/// The value of sample `i` of a row of `bits`-bit samples.
fn sample(row: &[u8], i: usize, bits: u8) -> u16 {
    match bits {
        16 => u16::from_be_bytes([row[2 * i], row[2 * i + 1]]),
        8 => row[i] as u16,
        _ => {
            let per = 8 / bits as usize;
            let byte = row[i / per];
            let shift = 8 - bits as usize * (i % per + 1);
            ((byte >> shift) & ((1 << bits) - 1)) as u16
        }
    }
}

fn put(row: &mut [u8], i: usize, bits: u8, v: u16) {
    match bits {
        16 => row[2 * i..2 * i + 2].copy_from_slice(&v.to_be_bytes()),
        8 => row[i] = v as u8,
        _ => {
            let per = 8 / bits as usize;
            let shift = 8 - bits as usize * (i % per + 1);
            row[i / per] |= (v as u8) << shift;
        }
    }
}

/// The file as full rows of `channels` samples of `bits` bits each, Adam7 passes merged.
fn decode_rows(c: &Chunks, width: usize, height: usize, bits: u8, channels: usize, interlaced: bool) -> Option<Vec<u8>> {
    let mut raw = Vec::new();
    zlib_decompress(&c.idat, &mut raw)?;
    let row_bytes = |w: usize| (w * channels * bits as usize).div_ceil(8);
    let bpp = (channels * bits as usize).div_ceil(8).max(1);
    let mut at = 0;
    if !interlaced {
        return unfilter(&raw, &mut at, height, row_bytes(width), bpp);
    }
    let stride = row_bytes(width);
    let mut out = vec![0u8; height * stride];
    const PASSES: [(usize, usize, usize, usize); 7] = [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)];
    for (x0, y0, dx, dy) in PASSES {
        let (pw, ph) = (width.saturating_sub(x0).div_ceil(dx), height.saturating_sub(y0).div_ceil(dy));
        if pw == 0 || ph == 0 {
            continue;
        }
        let pass = unfilter(&raw, &mut at, ph, row_bytes(pw), bpp)?;
        let ps = row_bytes(pw);
        for py in 0..ph {
            let src = &pass[py * ps..(py + 1) * ps];
            let y = y0 + py * dy;
            for px in 0..pw {
                let x = x0 + px * dx;
                for ch in 0..channels {
                    let v = sample(src, px * channels + ch, bits);
                    put(&mut out[y * stride..(y + 1) * stride], x * channels + ch, bits, v);
                }
            }
        }
    }
    Some(out)
}

/// `write_png` without the `copy_png` shortcut (whose pixels are the same): the PDF image of
/// a PNG file. `hicolor` keeps 16-bit samples.
pub fn png_to_pdf(png: &[u8], hicolor: bool) -> Option<PdfImage> {
    let c = chunks(png)?;
    let (width, height) = (be32(&c.ihdr[0..4]) as usize, be32(&c.ihdr[4..8]) as usize);
    let (bits, color, interlaced) = (c.ihdr[8], c.ihdr[9], c.ihdr[12] == 1);
    let channels = match color {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => return None,
    };
    let rows = decode_rows(&c, width, height, bits, channels, interlaced)?;
    let stride = (width * channels * bits as usize).div_ceil(8);
    // libpng's transformations: `png_set_tRNS_to_alpha` (a palette is expanded to RGB, a
    // low-depth gray to 8 bits) and `png_set_strip_16` (the high byte).
    let out_bits = if bits == 16 && !hicolor { 8 } else { bits };
    let scale = |v: u16| if bits == 16 && !hicolor { v >> 8 } else { v };
    let trns = c.trns.filter(|t| !t.is_empty());
    // Samples go straight from the decoded rows into the colour and mask streams: `n`
    // colour samples per pixel, two bytes each when `wide`.
    let (mut data, mut smask) = (Vec::new(), Vec::new());
    let mut gather = |n: usize, wide: bool, f: &mut dyn FnMut(&[u8], usize) -> [u16; 4]| {
        let k = if wide { 2 } else { 1 };
        data.reserve_exact(width * height * n * k);
        smask.reserve_exact(width * height * k);
        let put = |out: &mut Vec<u8>, v: u16| {
            if wide {
                out.extend_from_slice(&v.to_be_bytes());
            } else {
                out.push(v as u8);
            }
        };
        for y in 0..height {
            let row = &rows[y * stride..(y + 1) * stride];
            for x in 0..width {
                let p = f(row, x);
                for &v in &p[..n] {
                    put(&mut data, v);
                }
                put(&mut smask, p[3]);
            }
        }
    };
    let (space, bits_out, has_alpha) = match (color, trns) {
        // Palette with transparency: RGB and alpha.
        (3, Some(t)) => {
            gather(3, false, &mut |row, x| {
                let i = sample(row, x, bits) as usize;
                let rgb = c.plte.get(3 * i..3 * i + 3).unwrap_or(&[0, 0, 0]);
                [rgb[0] as u16, rgb[1] as u16, rgb[2] as u16, t.get(i).copied().unwrap_or(255) as u16]
            });
            (ColorSpace::Rgb, 8, true)
        }
        // Gray with a transparent value: 8-bit (or 16-bit) gray and alpha.
        (0, Some(t)) if t.len() >= 2 => {
            let key = u16::from_be_bytes([t[0], t[1]]);
            let max = (1u32 << bits) - 1;
            let b = if bits == 16 { out_bits } else { 8 };
            gather(1, b == 16, &mut |row, x| {
                let v = sample(row, x, bits);
                let g = if bits < 8 { (v as u32 * 255 / max) as u16 } else { scale(v) };
                let full = if b == 16 { 65535 } else { 255 };
                [g, 0, 0, if v == key { 0 } else { full }]
            });
            (ColorSpace::Gray, b, true)
        }
        // RGB with a transparent colour.
        (2, Some(t)) if t.len() >= 6 => {
            let key = [u16::from_be_bytes([t[0], t[1]]), u16::from_be_bytes([t[2], t[3]]), u16::from_be_bytes([t[4], t[5]])];
            gather(3, out_bits == 16, &mut |row, x| {
                let s = [sample(row, 3 * x, bits), sample(row, 3 * x + 1, bits), sample(row, 3 * x + 2, bits)];
                let full = if out_bits == 16 { 65535 } else { 255 };
                [scale(s[0]), scale(s[1]), scale(s[2]), if s == key { 0 } else { full }]
            });
            (ColorSpace::Rgb, out_bits, true)
        }
        (4, _) => {
            gather(1, out_bits == 16, &mut |row, x| [scale(sample(row, 2 * x, bits)), 0, 0, scale(sample(row, 2 * x + 1, bits))]);
            (ColorSpace::Gray, out_bits, true)
        }
        (6, _) => {
            gather(3, out_bits == 16, &mut |row, x| {
                let s = |k| scale(sample(row, 4 * x + k, bits));
                [s(0), s(1), s(2), s(3)]
            });
            (ColorSpace::Rgb, out_bits, true)
        }
        // No alpha: the rows as they are (16-bit stripped), a palette as `/Indexed`.
        (0 | 2 | 3, _) => {
            let space = match color {
                0 => ColorSpace::Gray,
                2 => ColorSpace::Rgb,
                _ => ColorSpace::Indexed(c.plte.to_vec()),
            };
            let data = if bits == 16 && !hicolor { rows.chunks(2).map(|p| p[0]).collect() } else { rows };
            return Some(PdfImage { width: width as u32, height: height as u32, bits: out_bits, space, data, smask: None });
        }
        _ => return None,
    };
    Some(PdfImage { width: width as u32, height: height as u32, bits: bits_out, space, data, smask: has_alpha.then_some(smask) })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG file of `ihdr` and `rows` (each with its filter byte), compressed by our deflate.
    fn png(w: u32, h: u32, bits: u8, color: u8, interlace: u8, extra: &[(&[u8; 4], &[u8])], raw: &[u8]) -> Vec<u8> {
        let mut out = SIGNATURE.to_vec();
        let mut chunk = |kind: &[u8; 4], body: &[u8]| {
            out.extend_from_slice(&(body.len() as u32).to_be_bytes());
            out.extend_from_slice(kind);
            out.extend_from_slice(body);
            out.extend_from_slice(&[0; 4]);
        };
        let mut ihdr = w.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[bits, color, 0, 0, interlace]);
        chunk(b"IHDR", &ihdr);
        for (k, b) in extra {
            chunk(k, b);
        }
        let mut z = Vec::new();
        crate::backend::backend_deflate::DeflateCompressor::new().compress_zlib(raw, &mut z);
        chunk(b"IDAT", &z);
        chunk(b"IEND", &[]);
        out
    }

    #[test]
    fn rgba_splits_into_rgb_and_soft_mask() {
        // Two rows: filter None, then Sub (each byte adds the one a pixel to the left).
        let raw = [0, 10, 20, 30, 255, 1, 2, 3, 128, 1, 5, 5, 5, 0, 1, 1, 1, 10];
        let img = png_to_pdf(&png(2, 2, 8, 6, 0, &[], &raw), false).expect("decodes");
        assert_eq!(img.space, ColorSpace::Rgb);
        assert_eq!(img.data, [10, 20, 30, 1, 2, 3, 5, 5, 5, 6, 6, 6]);
        assert_eq!(img.smask.as_deref(), Some(&[255, 128, 0, 10][..]));
        let info = png_info(&png(2, 2, 8, 6, 0, &[(b"pHYs", &[0, 0, 11, 19, 0, 0, 11, 19, 1])], &raw)).expect("info");
        assert_eq!((info.width, info.x_res, info.alpha), (2, 72, true));
    }

    #[test]
    fn paeth_up_average_and_sixteen_bits() {
        // 1×2 gray 16-bit: rows 0x1234, then Up (+0x0101) giving 0x1335; high bytes kept.
        let raw = [0, 0x12, 0x34, 2, 0x01, 0x01];
        let img = png_to_pdf(&png(1, 2, 16, 0, 0, &[], &raw), false).expect("decodes");
        assert_eq!((img.bits, img.data.as_slice()), (8, &[0x12, 0x13][..]));
        // 2×3 gray 8-bit with Average and Paeth rows.
        let raw = [0, 100, 50, 3, 10, 10, 4, 1, 1];
        let img = png_to_pdf(&png(2, 3, 8, 0, 0, &[], &raw), false).expect("decodes");
        assert_eq!(img.data, [100, 50, 60, 65, 61, 66]);
    }

    #[test]
    fn adam7_matches_plain_rows() {
        // A 3×3 1-bit gray image, interlaced: passes 1, 2(empty width), 3(empty)… built by hand.
        let pixels = [[1u8, 0, 1], [0, 1, 0], [1, 1, 0]];
        let mut plain = Vec::new();
        for r in pixels {
            plain.push(0);
            plain.push(r[0] << 7 | r[1] << 6 | r[2] << 5);
        }
        let flat = png_to_pdf(&png(3, 3, 1, 0, 0, &[], &plain), false).expect("plain");
        let mut raw = Vec::new();
        for (x0, y0, dx, dy) in [(0usize, 0usize, 8usize, 8usize), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)] {
            let (pw, ph) = (3usize.saturating_sub(x0).div_ceil(dx), 3usize.saturating_sub(y0).div_ceil(dy));
            if pw == 0 || ph == 0 {
                continue;
            }
            for py in 0..ph {
                raw.push(0);
                let mut byte = 0u8;
                for px in 0..pw {
                    byte |= pixels[y0 + py * dy][x0 + px * dx] << (7 - px);
                }
                raw.push(byte);
            }
        }
        let laced = png_to_pdf(&png(3, 3, 1, 0, 1, &[], &raw), false).expect("interlaced");
        assert_eq!(laced.data, flat.data);
    }
}

#[cfg(test)]
mod corpus_check {
    /// `RT_PNG=file.png RT_PNG_OUT=prefix cargo test --lib png_dump -- --ignored`: the decoded
    /// samples and soft mask, to compare with the image streams of a pdfTeX PDF.
    #[test]
    #[ignore]
    fn png_dump() {
        let Ok(path) = std::env::var("RT_PNG") else { return };
        let img = super::png_to_pdf(&std::fs::read(&path).expect("file"), false).expect("decodes");
        let out = std::env::var("RT_PNG_OUT").expect("RT_PNG_OUT");
        std::fs::write(format!("{out}.data"), &img.data).expect("write");
        std::fs::write(format!("{out}.mask"), img.smask.unwrap_or_default()).expect("write");
    }
}
