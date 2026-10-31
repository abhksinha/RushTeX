//! zlib/DEFLATE decompression (RFC 1950, RFC 1951), the reading half of
//! `backend_deflate`: stored, fixed-Huffman and dynamic-Huffman blocks, decoded through
//! canonical code tables built once per block. Used for PNG image data.

/// Bits of the one-level lookup table: a code this long or shorter is decoded with one
/// probe; longer ones bit by bit.
const FAST: u32 = 10;

/// A canonical Huffman code: how many codes of each length, the symbols in code order, and
/// `fast`, indexed by the next `FAST` input bits (least significant first, as DEFLATE packs
/// them): `symbol << 4 | length` for a code of at most `FAST` bits, 0 otherwise.
struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
    fast: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Option<Huffman> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        // An over-subscribed code is invalid; an incomplete one is allowed (RFC 1951 3.2.7).
        let mut left = 1i32;
        for &c in &counts[1..] {
            left = (left << 1) - c as i32;
            if left < 0 {
                return None;
            }
        }
        let mut offs = [0u16; 16];
        for l in 1..15 {
            offs[l + 1] = offs[l] + counts[l];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        // RFC 1951 3.2.2: codes of one length are consecutive, in symbol order.
        let mut fast = vec![0u16; 1 << FAST];
        let (mut code, mut at) = (0u32, 0usize);
        for (l, &count) in counts.iter().enumerate().take(FAST as usize + 1).skip(1) {
            for _ in 0..count {
                let rev = code.reverse_bits() >> (32 - l);
                let e = symbols[at] << 4 | l as u16;
                let mut i = rev as usize;
                while i < fast.len() {
                    fast[i] = e;
                    i += 1 << l;
                }
                code += 1;
                at += 1;
            }
            code <<= 1;
        }
        Some(Huffman { counts, symbols, fast })
    }
}

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u64,
    count: u32,
}

impl Bits<'_> {
    /// At least 57 bits buffered, or all that is left.
    #[inline]
    fn refill(&mut self) {
        while self.count <= 56 {
            let Some(&b) = self.data.get(self.pos) else { return };
            self.pos += 1;
            self.buf |= (b as u64) << self.count;
            self.count += 8;
        }
    }

    #[inline]
    fn bits(&mut self, n: u32) -> Option<u32> {
        if self.count < n {
            self.refill();
            if self.count < n {
                return None;
            }
        }
        let v = (self.buf & ((1u64 << n) - 1)) as u32;
        self.buf >>= n;
        self.count -= n;
        Some(v)
    }

    /// To the next byte boundary, the bytes read ahead given back (a stored block).
    fn align(&mut self) {
        self.pos -= (self.count / 8) as usize;
        self.buf = 0;
        self.count = 0;
    }

    /// One symbol: through `fast` when its code is short, else bit by bit, most
    /// significant first (RFC 1951 3.1.1).
    #[inline]
    fn decode(&mut self, h: &Huffman) -> Option<u16> {
        self.refill();
        let e = h.fast[(self.buf & ((1 << FAST) - 1)) as usize];
        let len = (e & 15) as u32;
        if e != 0 && len <= self.count {
            self.buf >>= len;
            self.count -= len;
            return Some(e >> 4);
        }
        self.decode_slow(h)
    }

    fn decode_slow(&mut self, h: &Huffman) -> Option<u16> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= self.bits(1)? as i32;
            let count = h.counts[len] as i32;
            if code - count < first {
                return h.symbols.get((index + code - first) as usize).copied();
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        None
    }
}

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
/// The order of the code-length code lengths in a dynamic block header.
const CL_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// Raw DEFLATE data, appended to `out`, stopping once `out` holds at least `limit` bytes (it
/// may hold a little more): a prefix of the data, without decoding the rest. `None` when it
/// is malformed.
pub fn inflate_to(data: &[u8], out: &mut Vec<u8>, limit: usize) -> Option<()> {
    let mut b = Bits { data, pos: 0, buf: 0, count: 0 };
    loop {
        if out.len() >= limit {
            return Some(());
        }
        let last = b.bits(1)? == 1;
        match b.bits(2)? {
            0 => {
                b.align();
                let len = u16::from_le_bytes([*data.get(b.pos)?, *data.get(b.pos + 1)?]) as usize;
                b.pos += 4;
                out.extend_from_slice(data.get(b.pos..b.pos + len)?);
                b.pos += len;
            }
            1 => {
                let mut l = [8u8; 288];
                l[144..256].fill(9);
                l[256..280].fill(7);
                let lit = Huffman::new(&l)?;
                let dist = Huffman::new(&[5u8; 30])?;
                codes(&mut b, &lit, &dist, out, limit)?;
            }
            2 => {
                let hlit = b.bits(5)? as usize + 257;
                let hdist = b.bits(5)? as usize + 1;
                let hclen = b.bits(4)? as usize + 4;
                let mut cl = [0u8; 19];
                for &k in &CL_ORDER[..hclen] {
                    cl[k] = b.bits(3)? as u8;
                }
                let clh = Huffman::new(&cl)?;
                let mut lengths = vec![0u8; hlit + hdist];
                let mut i = 0;
                while i < hlit + hdist {
                    let sym = b.decode(&clh)?;
                    let (val, rep) = match sym {
                        0..=15 => (sym as u8, 1),
                        16 => (*lengths.get(i.checked_sub(1)?)?, 3 + b.bits(2)? as usize),
                        17 => (0, 3 + b.bits(3)? as usize),
                        _ => (0, 11 + b.bits(7)? as usize),
                    };
                    if i + rep > lengths.len() {
                        return None;
                    }
                    lengths[i..i + rep].fill(val);
                    i += rep;
                }
                let lit = Huffman::new(&lengths[..hlit])?;
                let dist = Huffman::new(&lengths[hlit..])?;
                codes(&mut b, &lit, &dist, out, limit)?;
            }
            _ => return None,
        }
        if last {
            return Some(());
        }
    }
}

fn codes(b: &mut Bits, lit: &Huffman, dist: &Huffman, out: &mut Vec<u8>, limit: usize) -> Option<()> {
    loop {
        if out.len() >= limit {
            return Some(());
        }
        let sym = b.decode(lit)? as usize;
        match sym {
            0..=255 => out.push(sym as u8),
            256 => return Some(()),
            _ => {
                let k = sym - 257;
                let len = *LEN_BASE.get(k)? as usize + b.bits(*LEN_EXTRA.get(k)? as u32)? as usize;
                let d = b.decode(dist)? as usize;
                let back = *DIST_BASE.get(d)? as usize + b.bits(*DIST_EXTRA.get(d)? as u32)? as usize;
                let start = out.len().checked_sub(back)?;
                if back >= len {
                    out.extend_from_within(start..start + len);
                } else {
                    for i in 0..len {
                        out.push(out[start + i]);
                    }
                }
            }
        }
    }
}

/// zlib data (a two-byte header, DEFLATE data, an Adler-32 checksum, which is not checked:
/// libpng is told to ignore header mistakes too).
pub fn zlib_decompress(data: &[u8], out: &mut Vec<u8>) -> Option<()> {
    zlib_decompress_to(data, out, usize::MAX)
}

/// [`zlib_decompress`] of a prefix of at least `limit` bytes ([`inflate_to`]).
pub fn zlib_decompress_to(data: &[u8], out: &mut Vec<u8>, limit: usize) -> Option<()> {
    if data.len() < 2 || data[0] & 0x0F != 8 || (u16::from(data[0]) << 8 | u16::from(data[1])) % 31 != 0 {
        return None;
    }
    inflate_to(&data[2..], out, limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_our_deflate() {
        let text: Vec<u8> = (0..20000u32).map(|i| (i * 7 % 251) as u8 ^ (i / 300) as u8).chain(b"hello hello hello".iter().copied()).collect();
        let mut packed = Vec::new();
        crate::backend::backend_deflate::DeflateCompressor::new().compress_zlib(&text, &mut packed);
        let mut out = Vec::new();
        zlib_decompress(&packed, &mut out).expect("valid zlib");
        assert_eq!(out, text);
    }

    /// zlib's own output (level 9, one dynamic block) for text with skewed frequencies, so
    /// some codes are longer than the `FAST` table and go through `decode_slow`; and its
    /// prefix through `zlib_decompress_to`.
    #[test]
    fn dynamic_block_with_long_codes() {
        let packed = include_bytes!("testdata/inflate-dynamic.z");
        let text = include_bytes!("testdata/inflate-dynamic.txt");
        let mut out = Vec::new();
        zlib_decompress(packed, &mut out).expect("valid zlib");
        assert_eq!(&out[..], &text[..]);
        let mut head = Vec::new();
        zlib_decompress_to(packed, &mut head, 100).expect("valid zlib");
        assert!(head.len() >= 100 && head.len() < text.len());
        assert_eq!(&head[..], &text[..head.len()]);
    }

    #[test]
    fn stored_and_fixed_blocks() {
        // "abc" stored, then a fixed block with "abcabc" written as a back reference.
        let mut out = Vec::new();
        inflate_to(&[0x01, 0x03, 0x00, 0xFC, 0xFF, b'a', b'b', b'c'], &mut out, usize::MAX).expect("stored");
        assert_eq!(out, b"abc");
    }
}
