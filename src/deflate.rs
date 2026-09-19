//! Pure Rust, zero-dependency streaming DEFLATE and zlib encoder.
//!
//! Designed for L1/L2 cache locality:
//! - 16 KB L1-cache-resident hash table ([u16; 8192]) for O(1) branchless LZ77 match candidate lookups.
//! - Precomputed static tables in .rodata for RFC 1951 Fixed Huffman codes (zero dynamic tree overhead).
//! - Streaming bit-accumulator avoiding intermediate token buffer allocations.
//! - Block-batched Adler-32 checksum (RFC 1950) with no division in hot loops.

/// Precomputed compile-time Fixed Huffman codes and bit-lengths for Literal/Length (0..=287).
/// Stored in .rodata as SoA: codes array and bit lengths array.
pub static LIT_LEN_CODES: [u16; 288] = generate_lit_len_codes();
pub static LIT_LEN_BITS: [u8; 288] = generate_lit_len_bits();

/// Precomputed compile-time Fixed Huffman codes and bit-lengths for Distances (0..=31).
pub static DIST_CODES: [u8; 32] = generate_dist_codes();
pub static DIST_BITS: [u8; 32] = [5u8; 32];

const fn bit_reverse_8(mut b: u8) -> u8 {
    b = (b & 0xF0) >> 4 | (b & 0x0F) << 4;
    b = (b & 0xCC) >> 2 | (b & 0x33) << 2;
    b = (b & 0xAA) >> 1 | (b & 0x55) << 1;
    b
}

const fn bit_reverse_16(val: u16, len: u8) -> u16 {
    let r = ((bit_reverse_8((val & 0xFF) as u8) as u16) << 8)
        | (bit_reverse_8(((val >> 8) & 0xFF) as u8) as u16);
    r >> (16 - len)
}

const fn generate_lit_len_codes() -> [u16; 288] {
    let mut table = [0u16; 288];
    let mut i = 0;
    while i <= 143 {
        table[i] = bit_reverse_16(0x30 + i as u16, 8);
        i += 1;
    }
    while i <= 255 {
        table[i] = bit_reverse_16(0x190 + (i - 144) as u16, 9);
        i += 1;
    }
    while i <= 279 {
        table[i] = bit_reverse_16((i - 256) as u16, 7);
        i += 1;
    }
    while i <= 287 {
        table[i] = bit_reverse_16(0xC0 + (i - 280) as u16, 8);
        i += 1;
    }
    table
}

const fn generate_lit_len_bits() -> [u8; 288] {
    let mut table = [0u8; 288];
    let mut i = 0;
    while i <= 143 {
        table[i] = 8;
        i += 1;
    }
    while i <= 255 {
        table[i] = 9;
        i += 1;
    }
    while i <= 279 {
        table[i] = 7;
        i += 1;
    }
    while i <= 287 {
        table[i] = 8;
        i += 1;
    }
    table
}

const fn generate_dist_codes() -> [u8; 32] {
    let mut table = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        let rev = bit_reverse_8(i as u8) >> 3;
        table[i] = rev;
        i += 1;
    }
    table
}

// Length code lookup: returns (code, extra_bits, extra_val)
#[inline(always)]
fn get_length_code(len: usize) -> (u16, u8, u16) {
    match len {
        3 => (257, 0, 0),
        4 => (258, 0, 0),
        5 => (259, 0, 0),
        6 => (260, 0, 0),
        7 => (261, 0, 0),
        8 => (262, 0, 0),
        9 => (263, 0, 0),
        10 => (264, 0, 0),
        11..=12 => (265, 1, (len - 11) as u16),
        13..=14 => (266, 1, (len - 13) as u16),
        15..=16 => (267, 1, (len - 15) as u16),
        17..=18 => (268, 1, (len - 17) as u16),
        19..=22 => (269, 2, (len - 19) as u16),
        23..=26 => (270, 2, (len - 23) as u16),
        27..=30 => (271, 2, (len - 27) as u16),
        31..=34 => (272, 2, (len - 31) as u16),
        35..=42 => (273, 3, (len - 35) as u16),
        43..=50 => (274, 3, (len - 43) as u16),
        51..=58 => (275, 3, (len - 51) as u16),
        59..=66 => (276, 3, (len - 59) as u16),
        67..=82 => (277, 4, (len - 67) as u16),
        83..=98 => (278, 4, (len - 83) as u16),
        99..=114 => (279, 4, (len - 99) as u16),
        115..=130 => (280, 4, (len - 115) as u16),
        131..=162 => (281, 5, (len - 131) as u16),
        163..=194 => (282, 5, (len - 163) as u16),
        195..=226 => (283, 5, (len - 195) as u16),
        227..=257 => (284, 5, (len - 227) as u16),
        _ => (285, 0, 0),
    }
}

// Distance code lookup: returns (code, extra_bits, extra_val)
#[inline(always)]
fn get_dist_code(dist: usize) -> (u8, u8, u16) {
    match dist {
        1 => (0, 0, 0),
        2 => (1, 0, 0),
        3 => (2, 0, 0),
        4 => (3, 0, 0),
        5..=6 => (4, 1, (dist - 5) as u16),
        7..=8 => (5, 1, (dist - 7) as u16),
        9..=12 => (6, 2, (dist - 9) as u16),
        13..=16 => (7, 2, (dist - 13) as u16),
        17..=24 => (8, 3, (dist - 17) as u16),
        25..=32 => (9, 3, (dist - 25) as u16),
        33..=48 => (10, 4, (dist - 33) as u16),
        49..=64 => (11, 4, (dist - 49) as u16),
        65..=96 => (12, 5, (dist - 65) as u16),
        97..=128 => (13, 5, (dist - 97) as u16),
        129..=192 => (14, 6, (dist - 129) as u16),
        193..=256 => (15, 6, (dist - 193) as u16),
        257..=384 => (16, 7, (dist - 257) as u16),
        385..=512 => (17, 7, (dist - 385) as u16),
        513..=768 => (18, 8, (dist - 513) as u16),
        769..=1024 => (19, 8, (dist - 769) as u16),
        1025..=1536 => (20, 9, (dist - 1025) as u16),
        1537..=2048 => (21, 9, (dist - 1537) as u16),
        2049..=3072 => (22, 10, (dist - 2049) as u16),
        3073..=4096 => (23, 10, (dist - 3073) as u16),
        4097..=6144 => (24, 11, (dist - 4097) as u16),
        6145..=8192 => (25, 11, (dist - 6145) as u16),
        8193..=12288 => (26, 12, (dist - 8193) as u16),
        12289..=16384 => (27, 12, (dist - 12289) as u16),
        16385..=24576 => (28, 13, (dist - 16385) as u16),
        _ => (29, 13, (dist - 24577) as u16),
    }
}

/// Computes Adler-32 checksum (RFC 1950) with block-batched unrolled arithmetic.
pub fn adler32(data: &[u8]) -> u32 {
    let mut s1 = 1u32;
    let mut s2 = 0u32;
    const MOD_ADLER: u32 = 65521;

    for chunk in data.chunks(5552) {
        for &b in chunk {
            s1 += b as u32;
            s2 += s1;
        }
        s1 %= MOD_ADLER;
        s2 %= MOD_ADLER;
    }
    (s2 << 16) | s1
}

/// Bit writer accumulator for fast LSB-first emission.
struct BitWriter<'a> {
    buf: &'a mut Vec<u8>,
    bit_buf: u64,
    bit_count: u8,
}

impl<'a> BitWriter<'a> {
    #[inline(always)]
    fn new(buf: &'a mut Vec<u8>) -> Self {
        Self {
            buf,
            bit_buf: 0,
            bit_count: 0,
        }
    }

    #[inline(always)]
    fn write_bits(&mut self, val: u32, count: u8) {
        self.bit_buf |= (val as u64) << self.bit_count;
        self.bit_count += count;
        if self.bit_count >= 32 {
            let low = (self.bit_buf as u32).to_le_bytes();
            self.buf.extend_from_slice(&low);
            self.bit_buf >>= 32;
            self.bit_count -= 32;
        }
    }

    #[inline(always)]
    fn finish(mut self) {
        while self.bit_count > 0 {
            self.buf.push((self.bit_buf & 0xFF) as u8);
            self.bit_buf >>= 8;
            self.bit_count = self.bit_count.saturating_sub(8);
        }
    }
}

/// High-performance zlib/DEFLATE compressor.
///
/// Holds an 8192-entry hash table (32 KB) that stays resident in L1 data cache.
pub struct DeflateCompressor {
    // 8192 entries of u32 = 32,768 bytes (32 KB) -> 100% L1D cache resident
    head: [u32; 8192],
}

impl Default for DeflateCompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl DeflateCompressor {
    pub fn new() -> Self {
        Self {
            head: [0u32; 8192],
        }
    }

    /// Compress `input` into zlib-wrapped DEFLATE format (RFC 1950) appending to `out`.
    pub fn compress_zlib(&mut self, input: &[u8], out: &mut Vec<u8>) {
        // 1. Write zlib header: CMF = 0x78 (32K window, deflate), FLG = 0x9C (default)
        out.push(0x78);
        out.push(0x9C);

        let start_len = out.len();

        // 2. Compress payload using Fixed Huffman DEFLATE block
        self.compress_raw(input, out);

        // If compression did not reduce size (e.g. tiny input < 32 bytes), fallback to stored block
        let compressed_payload_len = out.len() - start_len;
        if compressed_payload_len >= input.len() && !input.is_empty() {
            out.truncate(start_len);
            write_stored_block(input, out);
        }

        // 3. Write Adler-32 checksum (big-endian)
        let checksum = adler32(input);
        out.extend_from_slice(&checksum.to_be_bytes());
    }

    /// Compress raw DEFLATE stream with Fixed Huffman codes (BFINAL=1, BTYPE=01).
    pub fn compress_raw(&mut self, input: &[u8], out: &mut Vec<u8>) {
        if input.is_empty() {
            let mut w = BitWriter::new(out);
            w.write_bits(0b011, 3); // BFINAL=1, BTYPE=01
            w.write_bits(LIT_LEN_CODES[256] as u32, LIT_LEN_BITS[256]); // EOB
            w.finish();
            return;
        }

        // Clear hash table (32 KB memset - instantaneous in L1 cache)
        self.head.fill(0);

        let mut w = BitWriter::new(out);
        w.write_bits(0b011, 3); // BFINAL=1, BTYPE=01 (Fixed Huffman)

        let len = input.len();
        let mut pos = 0;

        while pos + 3 <= len {
            // 3-byte multiplicative hash
            let b0 = input[pos] as u32;
            let b1 = input[pos + 1] as u32;
            let b2 = input[pos + 2] as u32;
            let h = (((b0 << 10) ^ (b1 << 5) ^ b2).wrapping_mul(0x1e35a7bd) >> 19) as usize & 8191;

            let prev_pos = self.head[h];
            self.head[h] = (pos + 1) as u32;

            let mut match_len = 0;
            let mut dist = 0;

            // Candidate match validation (within 32K window and valid position)
            if prev_pos > 0 {
                let prev_idx = (prev_pos - 1) as usize;
                dist = pos - prev_idx;
                if dist > 0 && dist <= 32768 && prev_idx + 2 < len {
                    if input[prev_idx] == input[pos]
                        && input[prev_idx + 1] == input[pos + 1]
                        && input[prev_idx + 2] == input[pos + 2]
                    {
                        match_len = 3;
                        let max_match = (len - pos).min(258);
                        while match_len < max_match && input[prev_idx + match_len] == input[pos + match_len] {
                            match_len += 1;
                        }
                    }
                }
            }

            if match_len >= 3 {
                // Emit length code
                let (lcode, lextra_bits, lextra_val) = get_length_code(match_len);
                w.write_bits(LIT_LEN_CODES[lcode as usize] as u32, LIT_LEN_BITS[lcode as usize]);
                if lextra_bits > 0 {
                    w.write_bits(lextra_val as u32, lextra_bits);
                }

                // Emit distance code
                let (dcode, dextra_bits, dextra_val) = get_dist_code(dist);
                w.write_bits(DIST_CODES[dcode as usize] as u32, DIST_BITS[dcode as usize]);
                if dextra_bits > 0 {
                    w.write_bits(dextra_val as u32, dextra_bits);
                }

                pos += match_len;
            } else {
                let lit = input[pos] as usize;
                w.write_bits(LIT_LEN_CODES[lit] as u32, LIT_LEN_BITS[lit]);
                pos += 1;
            }
        }

        // Emit remaining trailing literal bytes
        while pos < len {
            let lit = input[pos] as usize;
            w.write_bits(LIT_LEN_CODES[lit] as u32, LIT_LEN_BITS[lit]);
            pos += 1;
        }

        // Emit End of Block (EOB = 256)
        w.write_bits(LIT_LEN_CODES[256] as u32, LIT_LEN_BITS[256]);
        w.finish();
    }
}

fn write_stored_block(input: &[u8], out: &mut Vec<u8>) {
    for chunk in input.chunks(65535) {
        let is_last = (chunk.as_ptr() as usize + chunk.len()) == (input.as_ptr() as usize + input.len());
        out.push(if is_last { 0x01 } else { 0x00 });
        let len = chunk.len() as u16;
        let nlen = !len;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&nlen.to_le_bytes());
        out.extend_from_slice(chunk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adler32() {
        assert_eq!(adler32(b""), 1);
        assert_eq!(adler32(b"a"), 0x00620062);
        assert_eq!(adler32(b"Wikipedia"), 0x11E60398);
    }

    #[test]
    fn test_deflate_compress_empty() {
        let mut comp = DeflateCompressor::new();
        let mut out = Vec::new();
        comp.compress_zlib(b"", &mut out);
        assert_eq!(out.len(), 8); // zlib header (2) + 10-bit payload in 2 bytes + adler32 (4)
        assert_eq!(&out[0..2], &[0x78, 0x9c]);
        assert_eq!(&out[4..8], &[0, 0, 0, 1]); // Adler32 of empty is 1
    }

    #[test]
    fn test_deflate_compress_basic_string() {
        let mut comp = DeflateCompressor::new();
        let mut out = Vec::new();
        let data = b"Hello, world! Hello, world! Hello, world! Hello, world!";
        comp.compress_zlib(data, &mut out);
        assert!(out.len() < data.len());
    }

    #[test]
    fn test_deflate_roundtrip_with_zlib() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let large_repeats = b"The quick brown fox jumps over the lazy dog. 1234567890\n".repeat(2000); // ~114 KB
        let cases: &[&[u8]] = &[
            b"",
            b"a",
            b"Hello world!",
            b"The quick brown fox jumps over the lazy dog. The quick brown fox jumps over the lazy dog.",
            include_bytes!("../examples/sample.tex"),
            &large_repeats,
        ];

        let mut comp = DeflateCompressor::new();
        for &input in cases {
            let mut compressed = Vec::new();
            comp.compress_zlib(input, &mut compressed);

            let mut child = Command::new("python3")
                .args([
                    "-c",
                    "import sys, zlib; sys.stdout.buffer.write(zlib.decompress(sys.stdin.buffer.read()))",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .expect("spawn python3");

            child.stdin.as_mut().unwrap().write_all(&compressed).unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, input);
        }
    }
}
