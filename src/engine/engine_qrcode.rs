//! qrcode.sty 1.51 (2015/01/08) on the GDL path: `\qrcode[height=..]{text}`.
//!
//! The package encodes in TeX and prints the matrix as rules; this is its encoder, step for
//! step, quirks included, and its printing as the same boxes:
//! - `\qr@choose@best@version`: the smallest version from `version` (1 when 0) where a level
//!   at least the one asked fits, then the best level that fits at that version, the levels
//!   tried in the order L, M, Q, H (the loop maps 0..3 to 1, 0, 3, 2);
//! - `\qr@calculate@capacity`, `\qr@setversion`, `\qr@settableix` (the package's block
//!   table, [`BLOCKS`]);
//! - `\qr@encode@binary`: byte mode only, `4`, the count, the bytes, a `0` nibble, then the
//!   padding codewords `ec11`;
//! - `\FX@generate@errorbytes` (Reed–Solomon over GF(256) by 0x11d), `\qr@interleave`;
//! - the matrix: `\qr@placefinderpatterns`, `\qr@placetimingpatterns`,
//!   `\qr@placealignmentpatterns`, `\qr@placedummyformatpatterns`,
//!   `\qr@placedummyversionpatterns`, `\qr@writedata@hex`, `\qr@writeremainderbits`;
//! - `\qr@chooseandapplybestmask` with `\qr@evaluatemaskpenalty` as the package computes it:
//!   `\qr@fivezeros` is `11111`, so runs of light modules cost nothing; a 1:1:3:1:1 pattern
//!   counts only as `0000 1011101` inside a row or column (`\qr@lastfive` is compared with
//!   the eleven-bit `\qr@finderB`, never equal), and at the last module as one of the five
//!   `\qr@finderB` endings, not as `\qr@finderA`; the format modules count as light;
//! - `\qr@writeformatstring` (`\qr@BCHcode`, masked by `\qr@formatmask`),
//!   `\qr@writeversionstring` (`\qr@Golaycode`).
//!
//! `\qr@printmatrix` then sets, in `\begin{minipage}{n m}` with `\baselineskip=m`, each row
//! as a paragraph of `\rule{m}{m}` (dark) and `\rule{m}{0pt}` (light), `m` the height
//! divided by `n` (`\divide`), then `\par`.

use super::super::engine_run::Machine;

/// `\qr@settableix`: per level (0 M, 1 L, 2 H, 3 Q) and version 1..40, the number of blocks
/// and of error-correction codewords per block, as the package's table has them.
const BLOCKS: [[(u8, u8); 40]; 4] = [
    [(1, 10), (1, 16), (1, 26), (2, 18), (2, 24), (4, 16), (4, 18), (4, 22), (5, 22), (5, 26), (5, 30), (8, 22), (9, 22), (9, 24), (10, 24), (10, 28), (11, 28), (13, 26), (14, 26), (16, 26), (17, 26), (17, 28), (18, 28), (20, 28), (21, 28), (23, 28), (25, 28), (26, 28), (28, 28), (29, 28), (31, 28), (33, 28), (35, 28), (37, 28), (38, 28), (40, 28), (43, 28), (45, 28), (47, 28), (49, 28)],
    [(1, 7), (1, 10), (1, 15), (1, 20), (1, 26), (2, 18), (2, 20), (2, 24), (2, 30), (4, 18), (4, 20), (4, 24), (4, 26), (4, 30), (6, 22), (6, 24), (6, 28), (6, 30), (7, 28), (8, 28), (8, 28), (9, 28), (9, 30), (10, 30), (12, 26), (12, 28), (12, 30), (13, 30), (14, 30), (15, 30), (16, 30), (17, 30), (18, 30), (19, 30), (19, 30), (20, 30), (21, 30), (22, 30), (24, 30), (25, 30)],
    [(1, 17), (1, 28), (2, 22), (4, 16), (4, 22), (4, 28), (5, 26), (6, 26), (8, 24), (8, 28), (11, 24), (11, 28), (16, 22), (16, 24), (18, 24), (16, 30), (19, 28), (21, 28), (25, 26), (25, 28), (25, 30), (34, 24), (30, 30), (32, 30), (35, 30), (37, 30), (40, 30), (42, 30), (45, 30), (48, 30), (51, 30), (54, 30), (57, 30), (60, 30), (63, 30), (66, 30), (70, 30), (74, 30), (77, 30), (81, 30)],
    [(1, 13), (1, 22), (2, 18), (2, 26), (4, 18), (4, 24), (6, 18), (6, 22), (8, 20), (8, 24), (8, 28), (10, 26), (12, 24), (16, 20), (12, 30), (17, 24), (16, 28), (18, 28), (21, 26), (20, 30), (23, 28), (23, 30), (25, 30), (27, 30), (29, 30), (34, 28), (34, 30), (35, 30), (38, 30), (40, 30), (43, 30), (45, 30), (48, 30), (51, 30), (53, 30), (56, 30), (59, 30), (62, 30), (65, 30), (68, 30)],
];

/// A module of the matrix while it is built, as the package's entries are (`\qr@blank`,
/// `\qr@black@fixed`/`\qr@white@fixed`, `\qr@format@square`, data `\qr@black`/`\qr@white`
/// and their masked flips, and the written format and version bits).
const BLANK: u8 = 0;
const FIXED: u8 = 2;
const FORMAT_SQUARE: u8 = 4;
const DATA: u8 = 6;
const INFO: u8 = 8;

/// What `\qr@calculate@capacity` and `\qr@setversion` derive for a version and level.
struct Layout {
    size: usize,
    k: i32,
    remainder: usize,
    count_bits: u32,
    blocks: usize,
    ec: usize,
    total_data: usize,
    capacity: i32,
}

fn layout(version: u32, level: usize) -> Layout {
    let v = version as i32;
    let n = 4 * v + 17;
    let k = if v == 1 { 0 } else { v / 7 + 2 };
    let mut function = 2 * n + 160;
    if k != 0 {
        function += (k * k - 3) * 25 - (k - 2) * 10;
    }
    let version_modules = if v > 6 { 36 } else { 0 };
    let avail = n * n - function - 31 - version_modules;
    let (blocks, ec) = BLOCKS[level][version as usize - 1];
    let total_data = avail / 8 - blocks as i32 * ec as i32;
    let count_bytes = if v < 10 { 1 } else { 2 };
    Layout { size: n as usize, k, remainder: (avail % 8) as usize, count_bits: count_bytes * 8, blocks: blocks as usize, ec: ec as usize, total_data: total_data as usize, capacity: total_data - count_bytes as i32 - 1 }
}

/// `\qr@choose@best@version`: the version and level the package uses for `len` bytes, or
/// `None` when nothing up to version 40 holds them.
fn choose(len: usize, desired_version: u32, desired_level: usize) -> Option<(u32, usize)> {
    let mut best = None;
    for v in desired_version.max(1)..=40 {
        let mut good = false;
        for j in 0..4usize {
            let level = (j / 2) * 4 + 1 - j;
            good |= level == desired_level;
            if good && layout(v, level).capacity >= len as i32 {
                best = Some((v, level));
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

/// GF(256) by 0x11d: the package's `\F@twotothe` and `\F@logtwo` tables.
fn gf_tables() -> ([u8; 256], [u8; 256]) {
    let (mut exp, mut log) = ([0u8; 256], [0u8; 256]);
    let mut x = 1u32;
    for (i, e) in exp.iter_mut().take(255).enumerate() {
        *e = x as u8;
        log[x as usize] = i as u8;
        x <<= 1;
        if x > 255 {
            x ^= 0x11d;
        }
    }
    (exp, log)
}

/// `\FX@generate@errorbytes`: the remainder of `data * x^ec` by `\prod_{j<ec} (x - 2^j)`.
fn error_bytes(data: &[u8], ec: usize, exp: &[u8; 256], log: &[u8; 256]) -> Vec<u8> {
    let mul = |a: u8, b: u8| if a == 0 || b == 0 { 0 } else { exp[(log[a as usize] as usize + log[b as usize] as usize) % 255] };
    let mut g = vec![1u8];
    for &root in exp.iter().take(ec) {
        let mut next = vec![0u8; g.len() + 1];
        for (t, &c) in g.iter().enumerate() {
            next[t] ^= c;
            next[t + 1] ^= mul(c, root);
        }
        g = next;
    }
    let mut r = data.to_vec();
    r.resize(data.len() + ec, 0);
    for t in 0..data.len() {
        let lead = r[t];
        if lead != 0 {
            for (s, &c) in g.iter().enumerate() {
                r[t + s] ^= mul(c, lead);
            }
        }
    }
    r.split_off(data.len())
}

/// `\qr@polynomialdivide` of `bits` (most significant first, `len` of them) by `divisor`
/// (`width` bits): the remainder.
fn bch(value: u32, data_bits: u32, divisor: u32, width: u32) -> u32 {
    let mut r = value << (width - 1);
    for b in (width - 1..data_bits + width - 1).rev() {
        if r >> b & 1 == 1 {
            r ^= divisor << (b + 1 - width);
        }
    }
    value << (width - 1) | r
}

/// The encoded matrix: its side and its modules row by row (`true` dark), with the version
/// and level used (for `\qr@savedbinarymatrix@text@version@level`).
pub(super) struct Qr {
    pub(super) size: usize,
    pub(super) dark: Vec<bool>,
    pub(super) version: u32,
    pub(super) level: usize,
}

/// The package's encoding of `text` (bytes taken as their codes); `None` when it does not
/// fit or would leave a module blank.
pub(super) fn encode(text: &[u8], desired_version: u32, desired_level: usize) -> Option<Qr> {
    let (version, level) = choose(text.len(), desired_version, desired_level)?;
    let l = layout(version, level);
    let n = l.size;
    // `\qr@encode@binary`, in nibbles.
    let mut nib: Vec<u8> = vec![4];
    for s in (0..l.count_bits / 4).rev() {
        nib.push((text.len() >> (4 * s) & 15) as u8);
    }
    for &b in text {
        nib.extend([b >> 4, b & 15]);
    }
    nib.push(0);
    let mut code: Vec<u8> = nib.chunks(2).map(|p| p[0] << 4 | p.get(1).copied().unwrap_or(0)).collect();
    if code.len() > l.total_data {
        return None;
    }
    let pad = l.total_data - code.len();
    for k in 0..pad {
        code.push(if k % 2 == 0 { 0xec } else { 0x11 });
    }
    // `\qr@splitcodetextintoblocks`, `\qr@createerrorblocks`, `\qr@interleave`.
    let short = l.total_data / l.blocks;
    let long_n = l.total_data % l.blocks;
    let short_n = l.blocks - long_n;
    let (exp, log) = gf_tables();
    let mut blocks: Vec<&[u8]> = Vec::with_capacity(l.blocks);
    let mut at = 0;
    for b in 0..l.blocks {
        let len = if b < short_n { short } else { short + 1 };
        blocks.push(&code[at..at + len]);
        at += len;
    }
    let ecc: Vec<Vec<u8>> = blocks.iter().map(|d| error_bytes(d, l.ec, &exp, &log)).collect();
    let mut stream = Vec::with_capacity(l.total_data + l.blocks * l.ec);
    for i in 0..short {
        stream.extend(blocks.iter().map(|d| d[i]));
    }
    stream.extend(blocks[short_n..].iter().map(|d| d[short]));
    for i in 0..l.ec {
        stream.extend(ecc.iter().map(|e| e[i]));
    }
    // The matrix, 0-based here (the package counts from 1).
    let mut m = vec![BLANK; n * n];
    let set = |m: &mut Vec<u8>, i: usize, j: usize, v: u8| m[i * n + j] = v;
    // `\qr@placefinderpatterns`: the 8x8 `finderpattern` with its separator.
    for &(i0, j0, di, dj) in &[(0i32, 0i32, 1i32, 1i32), (n as i32 - 1, 0, -1, 1), (0, n as i32 - 1, 1, -1)] {
        for si in 0..8i32 {
            for sj in 0..8i32 {
                let dark = si < 7 && sj < 7 && (si == 0 || si == 6 || sj == 0 || sj == 6 || (2..=4).contains(&si) && (2..=4).contains(&sj));
                set(&mut m, (i0 + di * si) as usize, (j0 + dj * sj) as usize, FIXED | dark as u8);
            }
        }
    }
    // `\qr@placetimingpatterns`: row and column 7, modules 9..n-8 (1-based), dark when odd.
    for j in 8..n - 8 {
        let v = FIXED | (j % 2 == 0) as u8;
        set(&mut m, 6, j, v);
        set(&mut m, j, 6, v);
    }
    // `\qr@placealignmentpatterns`.
    if l.k > 0 {
        let general = ((2 * (n as i32 - 13)) / (l.k - 1) + 3) / 4 * 2;
        let first = n as i32 - 13 - (l.k - 2) * general;
        let align = |m: &mut Vec<u8>, ci: i32, cj: i32| {
            for a in 0..5i32 {
                for b in 0..5i32 {
                    let dark = a == 0 || a == 4 || b == 0 || b == 4 || a == 2 && b == 2;
                    set(m, (ci - 3 + a) as usize, (cj - 3 + b) as usize, FIXED | dark as u8);
                }
            }
        };
        if l.k == 2 {
            align(&mut m, 7 + first, 7 + first);
        } else {
            let at = |t: i32| if t == 1 { 7 } else { 7 + first + (t - 2) * general };
            for ii in 1..=l.k {
                for jj in 1..=l.k {
                    let corner = ii == 1 && (jj == 1 || jj == l.k) || ii == l.k && jj == 1;
                    if !corner {
                        align(&mut m, at(ii), at(jj));
                    }
                }
            }
        }
    }
    // `\qr@placedummyformatpatterns`.
    for j in (0..9).filter(|&j| j != 6) {
        set(&mut m, 8, j, FORMAT_SQUARE);
        set(&mut m, j, 8, FORMAT_SQUARE);
    }
    for j in n - 8..n {
        set(&mut m, 8, j, FORMAT_SQUARE);
        set(&mut m, j, 8, FORMAT_SQUARE);
    }
    set(&mut m, n - 8, 8, FIXED | 1);
    // `\qr@placedummyversionpatterns`.
    if version > 6 {
        for i in n - 11..n - 8 {
            for j in 0..6 {
                set(&mut m, i, j, FORMAT_SQUARE);
                set(&mut m, j, i, FORMAT_SQUARE);
            }
        }
    }
    // `\qr@writedata@hex`, then `\qr@writeremainderbits` from where it stopped.
    let mut bits: Vec<u8> = Vec::with_capacity(stream.len() * 8 + l.remainder);
    for &b in &stream {
        bits.extend((0..8).rev().map(|s| b >> s & 1));
    }
    let (mut i, mut j, mut right, mut up) = (n as i32 - 1, n as i32 - 1, true, true);
    let mut write = |m: &mut Vec<u8>, bits: &[u8]| -> Option<()> {
        let mut next = 0;
        while next < bits.len() {
            if !(0..n as i32).contains(&i) || !(0..n as i32).contains(&j) {
                return None;
            }
            let c = &mut m[i as usize * n + j as usize];
            if *c == BLANK {
                *c = DATA | bits[next];
                next += 1;
            }
            if right {
                j -= 1;
                right = false;
            } else {
                if up {
                    if i > 0 {
                        j += 1;
                        i -= 1;
                    } else {
                        j -= 1;
                        up = false;
                    }
                } else if (i as usize) < n - 1 {
                    j += 1;
                    i += 1;
                } else {
                    j -= 1;
                    up = true;
                }
                right = true;
                if j == 6 {
                    j = 5;
                }
            }
        }
        Some(())
    };
    write(&mut m, &bits)?;
    if l.remainder > 0 {
        write(&mut m, &vec![0; l.remainder])?;
    }
    if m.contains(&BLANK) {
        return None;
    }
    // `\qr@chooseandapplybestmask`: mask 0, then each of 1..7 that costs strictly less.
    let masked = |mask: u32| -> Vec<u8> {
        let mut out = m.clone();
        for i in 0..n {
            for j in 0..n {
                let c = &mut out[i * n + j];
                if *c & !1 == DATA && in_mask(mask, i as i32, j as i32) {
                    *c ^= 1;
                }
            }
        }
        out
    };
    let mut best = masked(0);
    let mut best_pen = penalty(&best, n);
    let mut mask = 0;
    for k in 1..8 {
        let cand = masked(k);
        let p = penalty(&cand, n);
        if p < best_pen {
            (best, best_pen, mask) = (cand, p, k);
        }
    }
    let mut m = best;
    // `\qr@writeformatstring`: level (2 bits) and mask (3 bits), `\qr@BCHcode`, xor
    // `\qr@formatmask`; A along row 9 then up column 9, B up column 9 then along row 9.
    let format = bch((level as u32) << 3 | mask, 5, 0b10100110111, 11) ^ 0b101010000010010;
    let fbit = |t: usize| (format >> (14 - t) & 1) as u8;
    let mut a = Vec::with_capacity(15);
    a.extend((0..9).filter(|&j| j != 6).map(|j| (8, j)));
    a.extend((0..8).rev().filter(|&i| i != 6).map(|i| (i, 8)));
    let mut b = Vec::with_capacity(15);
    b.extend((n - 7..n).rev().map(|i| (i, 8)));
    b.extend((n - 8..n).map(|j| (8, j)));
    for (t, &(i, j)) in a.iter().enumerate() {
        set(&mut m, i, j, INFO | fbit(t));
    }
    for (t, &(i, j)) in b.iter().enumerate() {
        set(&mut m, i, j, INFO | fbit(t));
    }
    // `\qr@writeversionstring`: from (n-8, 6) up three, then a column left, mirrored.
    if version > 6 {
        let golay = bch(version, 6, 0b1111100100101, 13);
        let (mut i, mut j) = (n - 9, 5usize);
        for t in 0..18 {
            let v = INFO | (golay >> (17 - t) & 1) as u8;
            set(&mut m, i, j, v);
            set(&mut m, j, i, v);
            i -= 1;
            if i < n - 11 {
                if j == 0 {
                    break;
                }
                j -= 1;
                i += 3;
            }
        }
    }
    if m.iter().any(|&c| c == BLANK || c == FORMAT_SQUARE) {
        return None;
    }
    Some(Qr { size: n, dark: m.iter().map(|&c| c & 1 == 1).collect(), version, level })
}

/// `\qr@setmaskingfunction`: whether the module at 0-based (`i`, `j`) is flipped.
fn in_mask(mask: u32, i: i32, j: i32) -> bool {
    match mask {
        0 => (i + j) % 2 == 0,
        1 => i % 2 == 0,
        2 => j % 3 == 0,
        3 => (i + j) % 3 == 0,
        4 => (i / 2 + j / 3) % 2 == 0,
        5 => (i * j) % 2 + (i * j) % 3 == 0,
        6 => ((i * j) % 2 + (i * j) % 3) % 2 == 0,
        _ => ((i + j) % 2 + (i * j) % 3) % 2 == 0,
    }
}

/// `\qr@evaluatemaskpenalty` as the package computes it (see the module comment).
fn penalty(m: &[u8], n: usize) -> i32 {
    // Format modules (`\qr@format@square`) are light; everything else by its bit.
    let bit = |i: usize, j: usize| -> u32 {
        let c = m[i * n + j];
        if c == FORMAT_SQUARE { 0 } else { (c & 1) as u32 }
    };
    let (mut p1, mut p2, mut p3, mut ones) = (0i32, 0i32, 0i32, 0i64);
    // One line (a row, or a column with `col`): penalties 1 and 3.
    let line = |get: &dyn Fn(usize) -> u32, p1: &mut i32, p3: &mut i32| {
        let mut run = false;
        // The last eleven bits of `0000` and the line so far.
        let mut last11: u32 = 0;
        for j in 0..n {
            let b = get(j);
            last11 = (last11 << 1 | b) & 0x7ff;
            if j >= 4 {
                if last11 & 0x1f == 0x1f {
                    *p1 += if run { 1 } else { 3 };
                    run = true;
                } else {
                    run = false;
                }
            }
            if j >= 6 {
                if j == n - 1 {
                    let hit = last11 == 0b10111010000 || last11 & 0x3ff == 0b1011101000 || last11 & 0x1ff == 0b101110100 || last11 & 0xff == 0b10111010 || last11 & 0x7f == 0b1011101;
                    if hit {
                        *p3 += 40;
                    }
                } else if last11 == 0b00001011101 {
                    *p3 += 40;
                }
            }
        }
    };
    for i in 0..n {
        line(&|j| bit(i, j), &mut p1, &mut p3);
        for j in 0..n {
            let b = bit(i, j);
            ones += b as i64;
            if i + 1 < n && j >= 1 {
                let (a0, a1, c0, c1) = (bit(i, j - 1), b, bit(i + 1, j - 1), bit(i + 1, j));
                if a0 & a1 & c0 & c1 == 1 || (a0 | a1 | c0 | c1) == 0 {
                    p2 += 3;
                }
            }
        }
    }
    for j in 0..n {
        line(&|i| bit(i, j), &mut p1, &mut p3);
    }
    let sq = (n * n) as i64;
    let p4 = ((20 * ones - 10 * sq).abs() / sq) as i32;
    p1 + p2 + p3 + p4
}

/// The package's state in a native document: the codes already written to the `.aux` file
/// (`\qr@savedflag@text@version@level`), and the rows of the matrix being printed.
#[derive(PartialEq, Clone, Default)]
pub(super) struct QrState {
    saved: Vec<(Vec<u8>, u32, usize)>,
    rows: Vec<bool>,
    size: usize,
    module: i32,
    row: usize,
}

/// Characters a code's text may hold: those whose catcode and meaning are the same under
/// `\qr@verbatimcatcodes` and the document's (letters, digits, punctuation), so the text
/// reads the same from the file and from a macro's body. No space (other there, skipped
/// here), `\`, braces, `%`, `#`, `$`, `&`, `^`, `_`, `~` or line end.
pub(super) fn text_ok(c: u8) -> bool {
    c.is_ascii_graphic() && !b"\\{}%#$&^_~".contains(&c)
}

/// After `\qrcode` in the source: the length of `*[options]{text}` when the native job
/// takes it (`Machine::qrcode`), the options among `height=<length>`, `level=`, `version=`,
/// `tight`, `nolink` and the text of [`text_ok`] characters.
pub(super) fn call_len(after: &str) -> Option<usize> {
    let mut r = after.strip_prefix('*').unwrap_or(after).trim_start();
    if let Some(o) = r.strip_prefix('[') {
        let (opts, rest) = o.split_once(']')?;
        options(opts)?;
        r = rest.trim_start();
    }
    let body = r.strip_prefix('{')?;
    let k = body.find('}')?;
    let text = &body[..k];
    // `#1` and kin: a macro's argument, whose tokens `Machine::qrcode` checks.
    if text.is_empty() || !text.bytes().all(|c| text_ok(c) || c == b'#') {
        return None;
    }
    Some(after.len() - body.len() + k + 1)
}

/// The keys of `\qrcode[options]` the native job takes: `(height text, version, level)`,
/// each `None` when absent; `None` for anything else (`padding`, the quiet zone, is not
/// ported).
fn options(opts: &str) -> Option<(Option<&str>, Option<u32>, Option<usize>)> {
    let (mut h, mut v, mut l) = (None, None, None);
    for kv in opts.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (k, val) = kv.split_once('=').map_or((kv, None), |(k, v)| (k.trim(), Some(v.trim())));
        match (k, val) {
            ("height", Some(x)) if !x.is_empty() && x.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'.') => h = Some(x),
            // `\qr@setlevel`: an unknown value leaves the level as it was.
            ("level", Some(x)) => l = Some(match x { "M" | "0" => 0, "L" | "1" => 1, "H" | "2" => 2, "Q" | "3" => 3, _ => return None }),
            ("version", Some(x)) => v = Some(x.parse::<u32>().ok().filter(|&n| n <= 40)?),
            ("tight", None | Some("true")) | ("padding", Some("false")) => {}
            ("nolink", None | Some("true")) => {}
            _ => return None,
        }
    }
    Some((h, v, l))
}

impl Machine<'_, '_> {
    /// `\qrcode*[options]{text}` (`\qrcode@i`, `\qrcode@int`) as the last of LaTeX's runs
    /// sets it, with every code in the `.aux` file: `\qrcode@int@remember`, the code printed
    /// (`\qr@printsavedbinarymatrix`, the same boxes as `\qr@printmatrix`), then, the first
    /// time this text is used at this version and level, `\write\@auxout{\string
    /// \qr@savematrix..}` (`\qr@savedflag@..`). The print is
    /// `\begin{minipage}{\qr@minipagewidth}\baselineskip=\qr@modulesize`, each row's rules
    /// and `\par`, `\end{minipage}`. The groups `\qrcode@i` and `\qrcode@wrapper{..}`
    /// (`\relax{..}` without hyperref) open hold no assignment the document can see.
    pub(super) fn qrcode(&mut self) {
        if self.mode == super::super::engine_run::Mode::Math {
            return self.not_native("\\qrcode in a formula");
        }
        let _star = self.next_is('*');
        let opts = if self.next_is('[') {
            let arg = self.bracket_arg();
            match arg.iter().map(|t| t.cs_id().is_none().then(|| t.chr())).collect::<Option<String>>() {
                Some(s) => s,
                None => return self.not_native("\\qrcode options with commands"),
            }
        } else {
            String::new()
        };
        let Some((height, version, level)) = options(&opts) else { return self.not_native("a \\qrcode option") };
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let text: Option<Vec<u8>> = arg.iter().map(|t| (t.cs_id().is_none() && matches!(t.cat(), Some(super::super::engine_catcode::Cat::Letter | super::super::engine_catcode::Cat::Other)) && (t.chr() as u32) < 128 && text_ok(t.chr() as u8)).then_some(t.chr() as u8)).collect();
        let Some(text) = text.filter(|t| !t.is_empty()) else { return self.not_native("a \\qrcode text of other characters") };
        // `\qrset{version=0, level=0, tight}` and `\setlength\qr@desiredheight{2cm}`.
        let toks: Vec<_> = height.unwrap_or("2cm").chars().map(|c| super::super::engine_token::Token::char(if c.is_ascii_alphabetic() { super::super::engine_catcode::Cat::Letter } else { super::super::engine_catcode::Cat::Other }, c)).collect();
        let height = self.dimen_arg(toks);
        let Some(q) = encode(&text, version.unwrap_or(0), level.unwrap_or(0)) else { return self.not_native("a \\qrcode text too long for version 40") };
        // `\setlength{\qr@modulesize}{\qr@desiredheight}\divide\qr@modulesize by \qr@size`,
        // `\multiply\qr@minipagewidth by \qr@size` (`tight`: no quiet zone).
        let n = q.size as i32;
        let module = height / n;
        let width = module.wrapping_mul(n);
        let Some(doc) = self.doc.as_mut() else { return };
        let st = doc.pkg.qr.get_or_insert_with(Default::default);
        let key = (text, q.version, q.level);
        let first = !st.saved.contains(&key);
        if first {
            st.saved.push(key);
        }
        (st.rows, st.size, st.module, st.row) = (q.dark, q.size, module, 0);
        use super::super::engine_catcode::Cat;
        use super::super::engine_token::Token;
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = Vec::with_capacity(32 + 2 * q.size);
        t.extend([self.named_cs(named!("begin")), open]);
        self.word_tokens("minipage", &mut t);
        t.extend([close, open]);
        Self::sp_tokens(width, &mut t);
        t.extend([close, self.named_cs(named!("baselineskip"))]);
        Self::sp_tokens(module, &mut t);
        t.push(Token::char(Cat::Space, ' '));
        let (row, par) = (self.doc_id(doc!("rt@qrrow")), self.named_cs(named!("par")));
        for _ in 0..q.size {
            t.extend([row, par]);
        }
        t.extend([self.named_cs(named!("end")), open]);
        self.word_tokens("minipage", &mut t);
        t.push(close);
        if first {
            t.push(self.doc_id(doc!("rt@qrwrite")));
        }
        self.back_list(&t);
    }

    /// qrcode's `\write\@auxout{..}` (not `\protected@write`): an inert whatsit where it is
    /// made. The package makes one in the preamble (`\qr@dummyqrsavedefinition`), after
    /// xcolor's push, which `document` puts on the main list.
    pub(super) fn qr_write(&mut self) {
        self.cur.push(super::super::engine_node::Node { kind: super::super::engine_node::Kind::Whatsit, sub: super::super::engine_whatsit::WRITE, font: super::super::engine_whatsit::NULL_STREAM, a: 0, b: 0 });
    }

    /// One row of `\qr@printmatrix`: `\rule{m}{m}` for a dark module, `\rule{m}{0pt}` for a
    /// light one (`\leavevmode\hbox{\vrule width m height h depth -0pt}`).
    pub(super) fn qr_row(&mut self) {
        if self.leave_vmode() {
            return;
        }
        let Some(st) = self.doc.as_mut().and_then(|d| d.pkg.qr.as_mut()) else { return };
        let (n, m, r) = (st.size, st.module, st.row);
        st.row += 1;
        let row: Vec<bool> = st.rows[r * n..(r + 1) * n].to_vec();
        for dark in row {
            self.build_hbox(|mm| mm.build_rule([m, if dark { m } else { 0 }, 0]));
        }
    }
}


/// qrcode.sty's natives, in `Meaning::Doc` order after the kernel's (`engine_doc::DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@qrcode", |m| m.qrcode()),
    ("rt@qrrow", |m| m.qr_row()),
    ("rt@qrwrite", |m| m.qr_write()),
];

#[cfg(test)]
mod tests {
    use super::encode;

    /// Matrices qrcode.sty wrote to its `.aux` (`\qr@savematrix{text}{version}{level}{bits}`).
    #[test]
    fn matches_qrcode_sty() {
        for (text, version, level, bits) in FIXTURES {
            let q = encode(text.as_bytes(), 0, 0).expect("encodes");
            assert_eq!((q.version, q.level), (*version, *level), "{text}");
            let got: String = q.dark.iter().map(|&d| if d { '1' } else { '0' }).collect();
            assert_eq!(&got, bits, "{text}");
        }
    }

    /// Every `\qrcode[level=..,height=..]{text}` of the document `QR_TEX` against the matrices
    /// pdfLaTeX wrote to its `.aux` (`QR_TEX` with `.aux`); run by hand.
    #[test]
    #[ignore]
    fn matches_aux_file() {
        let Ok(tex) = std::env::var("QR_TEX") else { return };
        let src = std::fs::read_to_string(&tex).unwrap();
        let aux = std::fs::read_to_string(tex.replace(".tex", ".aux")).unwrap();
        let mut saved = Vec::new();
        for line in aux.lines().filter_map(|l| l.strip_prefix("\\qr@savematrix")) {
            let f: Vec<&str> = line.split(['{', '}']).filter(|s| !s.is_empty()).collect();
            saved.push((f[0].to_string(), f[1].parse::<u32>().unwrap(), f[2].parse::<usize>().unwrap(), f[3].to_string()));
        }
        let (mut n, mut missing) = (0, 0);
        for call in src.split("\\qrcode[").skip(1) {
            let (opts, rest) = call.split_once("]{").unwrap();
            let text = &rest[..rest.find('}').unwrap()];
            let mut level = 0;
            let mut version = 0;
            for kv in opts.split(',') {
                match kv.split_once('=') {
                    Some(("level", v)) => level = ["M", "L", "H", "Q"].iter().position(|l| *l == v).unwrap(),
                    Some(("version", v)) => version = v.parse().unwrap(),
                    _ => {}
                }
            }
            let q = encode(text.as_bytes(), version, level).expect("encodes");
            let got: String = q.dark.iter().map(|&d| if d { '1' } else { '0' }).collect();
            // pdfLaTeX may have stopped (main memory) before the largest codes.
            let Some(want) = saved.iter().find(|s| s.0 == text && s.1 == q.version && s.2 == q.level) else {
                missing += 1;
                continue;
            };
            assert_eq!(got, want.3, "{text} {}-{}", q.version, q.level);
            n += 1;
        }
        eprintln!("{n} codes match, {missing} not in the aux");
        assert!(n > 0);
    }

    /// `\qrcode{A}` (level M asked, H given at no cost) and the first template's code.
    const FIXTURES: &[(&str, u32, usize, &str)] = &[
        ("A", 1, 2, "111111101010101111111100000100010101000001101110100100001011101101110101110101011101101110100000001011101100000100001001000001111111101010101111111000000000101100000000001011101011110001001001001011100011000110100100100110000010001101000011111111000100011110111100000010101000000001110101101010111111100000011101111100000101011111111000101110101011011101101101110100100011000110101110101010000010001100000100110101000110111111100001000010111"),
        ("https://example.edu/mira", 2, 0, "1111111010001110001111111100000100111101010100000110111010111011110010111011011101001101110001011101101110100001010000101110110000010110110100010000011111111010101010101111111000000000001001110000000010100011001010101001001011001010111011101011101011101101100110100101001110111101100001100101001010001010001001110001101100001000100000010010110110001111000110100101011110011010011100011000010110111000111001110101100011111001000000000111011001000100011111111010001110101010001100000100011101110001000010111010010010011111100101011101000100010010010110101110101011001111011101110000010010000011111100001111111010111000101001001"),
    ];
}
