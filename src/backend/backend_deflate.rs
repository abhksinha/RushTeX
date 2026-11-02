//! # High-Performance DEFLATE & zlib Compression Engine (`deflate`)
//!
//! Provides a pure-Rust, zero-dependency streaming DEFLATE (RFC 1951) and zlib (RFC 1950)
//! compression encoder optimized for CPU L1 data cache locality.
//!
//! ## Architectural Principles
//!
//! - **32 KB L1-Data-Cache Resident Table**: The [`DeflateCompressor`] maintains a single `[u32; 8192]`
//!   hash table (exactly 32,768 bytes), fitting entirely within the L1D cache of modern CPUs.
//! - **Structure-of-Arrays (SoA) Tables in `.rodata`**: RFC 1951 Fixed Huffman codes and bit-lengths
//!   are precomputed at compile-time as flat arrays (`LIT_LEN_CODES`, `LIT_LEN_BITS`, `DIST_CODES`,
//!   `DIST_BITS`), eliminating the overhead of dynamic Huffman tree construction and transmission.
//! - **Zero Intermediate Token Allocations**: Emits compressed bits directly to the target byte buffer
//!   via a 64-bit sliding register (`BitWriter`), completely bypassing intermediate token arrays.
//! - **Block-Batched Adler-32 Checksum**: Computes checksums in 5,552-byte unrolled batches, removing
//!   expensive division/modulo operations from inner byte loops.
//!
//! ## Public APIs
//!
//! - [`adler32`]: Computes the RFC 1950 Adler-32 checksum of a byte slice.
//! - [`DeflateCompressor`]: L1-resident compression engine that can be reused across pages.
//! - [`DeflateCompressor::compress_zlib`]: Compresses input data into zlib-wrapped DEFLATE format
//!   for PDF `/Filter /FlateDecode` streams.
//! - [`DeflateCompressor::compress_raw`]: Compresses input data into a raw RFC 1951 Fixed Huffman stream.

/// Precomputed compile-time Fixed Huffman codes and bit-lengths for Literal/Length (0..=287).
/// Stored in .rodata as SoA: codes array and bit lengths array.
pub static LIT_LEN_CODES: [u16; 288] = generate_lit_len_codes();
pub static LIT_LEN_BITS: [u8; 288] = generate_lit_len_bits();

/// Precomputed compile-time Fixed Huffman codes and bit-lengths for Distances (0..=31).
pub static DIST_CODES: [u8; 32] = generate_dist_codes();

const fn bit_reverse_8(mut byte_value: u8) -> u8 {
    byte_value = (byte_value & 0xF0) >> 4 | (byte_value & 0x0F) << 4;
    byte_value = (byte_value & 0xCC) >> 2 | (byte_value & 0x33) << 2;
    byte_value = (byte_value & 0xAA) >> 1 | (byte_value & 0x55) << 1;
    byte_value
}

const fn bit_reverse_16(value: u16, bit_length: u8) -> u16 {
    let reversed = ((bit_reverse_8((value & 0xFF) as u8) as u16) << 8)
        | (bit_reverse_8(((value >> 8) & 0xFF) as u8) as u16);
    reversed >> (16 - bit_length)
}

const fn generate_lit_len_codes() -> [u16; 288] {
    let mut table = [0u16; 288];
    let mut code_index = 0;
    while code_index <= 143 {
        table[code_index] = bit_reverse_16(0x30 + code_index as u16, 8);
        code_index += 1;
    }
    while code_index <= 255 {
        table[code_index] = bit_reverse_16(0x190 + (code_index - 144) as u16, 9);
        code_index += 1;
    }
    while code_index <= 279 {
        table[code_index] = bit_reverse_16((code_index - 256) as u16, 7);
        code_index += 1;
    }
    while code_index <= 287 {
        table[code_index] = bit_reverse_16(0xC0 + (code_index - 280) as u16, 8);
        code_index += 1;
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
const fn const_get_length_code(len: usize) -> (u16, u8, u16) {
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
const fn const_get_dist_code(dist: usize) -> (u8, u8, u16) {
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

const fn generate_length_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let lit_codes = generate_lit_len_codes();
    let lit_bits = generate_lit_len_bits();
    let mut len = 3;
    while len <= 258 {
        let (code, extra_bits, extra_val) = const_get_length_code(len);
        let huff_code = lit_codes[code as usize] as u32;
        let huff_bits = lit_bits[code as usize];
        let combined = huff_code | ((extra_val as u32) << huff_bits);
        let total = huff_bits + extra_bits;
        table[len - 3] = combined | ((total as u32) << 16);
        len += 1;
    }
    table
}

const fn generate_dist_table_2048() -> [u32; 2049] {
    let mut table = [0u32; 2049];
    let dist_codes = generate_dist_codes();
    let mut dist = 1;
    while dist <= 2048 {
        let (code, extra_bits, extra_val) = const_get_dist_code(dist);
        let huff_code = dist_codes[code as usize] as u32;
        let combined = huff_code | ((extra_val as u32) << 5);
        let total = 5 + extra_bits;
        table[dist] = combined | ((total as u32) << 16);
        dist += 1;
    }
    table
}

pub static LENGTH_TABLE: [u32; 256] = generate_length_table();
pub static DIST_TABLE_2048: [u32; 2049] = generate_dist_table_2048();

const fn generate_lit_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let codes = generate_lit_len_codes();
    let bits = generate_lit_len_bits();
    let mut i = 0;
    while i < 256 {
        table[i] = (codes[i] as u32) | ((bits[i] as u32) << 16);
        i += 1;
    }
    table
}

pub static LIT_TABLE: [u32; 256] = generate_lit_table();

/// Computes Adler-32 checksum (RFC 1950) with 16-byte unrolled parallel arithmetic.
pub fn adler32(data: &[u8]) -> u32 {
    adler32_from(1, data)
}

/// Adler-32 continued from `state` (the checksum of the bytes before `data`).
pub fn adler32_from(state: u32, data: &[u8]) -> u32 {
    let mut s1 = state & 0xFFFF;
    let mut s2 = state >> 16;
    const MOD_ADLER: u32 = 65521;

    for chunk in data.chunks(5552) {
        let mut slice = chunk;
        while slice.len() >= 16 {
            let b0 = slice[0] as u32;
            let b1 = slice[1] as u32;
            let b2 = slice[2] as u32;
            let b3 = slice[3] as u32;
            let b4 = slice[4] as u32;
            let b5 = slice[5] as u32;
            let b6 = slice[6] as u32;
            let b7 = slice[7] as u32;
            let b8 = slice[8] as u32;
            let b9 = slice[9] as u32;
            let b10 = slice[10] as u32;
            let b11 = slice[11] as u32;
            let b12 = slice[12] as u32;
            let b13 = slice[13] as u32;
            let b14 = slice[14] as u32;
            let b15 = slice[15] as u32;

            let sum03 = b0 + b1 + b2 + b3;
            let sum47 = b4 + b5 + b6 + b7;
            let sum8b = b8 + b9 + b10 + b11;
            let sumcf = b12 + b13 + b14 + b15;
            let sum_all = (sum03 + sum47) + (sum8b + sumcf);

            let w0 = 16 * b0 + 15 * b1 + 14 * b2 + 13 * b3;
            let w1 = 12 * b4 + 11 * b5 + 10 * b6 + 9 * b7;
            let w2 = 8 * b8 + 7 * b9 + 6 * b10 + 5 * b11;
            let w3 = 4 * b12 + 3 * b13 + 2 * b14 + b15;

            s2 += (s1 << 4) + ((w0 + w1) + (w2 + w3));
            s1 += sum_all;
            slice = &slice[16..];
        }
        for &b in slice {
            s1 += b as u32;
            s2 += s1;
        }
        s1 %= MOD_ADLER;
        s2 %= MOD_ADLER;
    }
    (s2 << 16) | s1
}

/// Bit writer: codes gather LSB-first in a 64-bit register, and each full register goes to
/// the output as 8 little-endian bytes, one store into capacity the callers reserved
/// ahead. Codes are at most 48 bits, with no bits set above their length.
struct BitWriter<'a> {
    buf: &'a mut Vec<u8>,
    bit_buf: u64,
    bit_count: u32,
}

impl<'a> BitWriter<'a> {
    #[inline(always)]
    fn new(buf: &'a mut Vec<u8>) -> Self {
        Self { buf, bit_buf: 0, bit_count: 0 }
    }

    #[inline(always)]
    fn write_bits(&mut self, val: u64, count: u8) {
        let total = self.bit_count + count as u32;
        // `bit_count` is below 64: the shift keeps the low bits of `val` that fit.
        self.bit_buf |= val << self.bit_count;
        if total < 64 {
            self.bit_count = total;
            return;
        }
        self.buf.extend_from_slice(&self.bit_buf.to_le_bytes());
        // The rest of `val` (`64 - bit_count` is 1..=48 here, as `count` is at most 48).
        self.bit_buf = val >> (64 - self.bit_count);
        self.bit_count = total - 64;
    }

    #[inline(always)]
    fn finish(self) {
        let bytes = (self.bit_count as usize).div_ceil(8);
        self.buf.extend_from_slice(&self.bit_buf.to_le_bytes()[..bytes]);
    }
}

/// zlib/DEFLATE compressor (RFC 1951): LZ77 over hash chains with deferred ("lazy")
/// matching, then each block coded with its own Huffman tables (dynamic), or the fixed
/// ones where those are smaller.
///
/// The chains are two arrays of 32-bit positions (SoA, 128 KB each, L2-resident): `head`
/// by the hash of three bytes, `prev` by position in the 32 KB window. Positions are
/// stored offset by `base`, which moves past each input, so a new input needs no clearing:
/// anything at or below `base` is from an earlier one.
pub struct DeflateCompressor {
    head: Vec<u32>,
    prev: Vec<u32>,
    base: u32,
    /// LZ77 output of the current input: a literal byte, or a match ([`match_token`]).
    tokens: Vec<u32>,
    /// Whether the block written is the stream's last (BFINAL); false only inside
    /// `compress_zlib_parts`.
    last: bool,
    /// The fast encoder (`compress_fixed`) instead: taken from [`set_fast`] when made.
    fast: bool,
    /// The fast encoder's match table, made at its first use: 4096 packed slots, `epoch <<
    /// 40 | prefix3 << 16 | pos + 1` for inputs of at most 64 KB (whose `epoch` moves past
    /// each, so they need no clearing), else `prefix3 << 32 | pos + 1`. AoS, 32 KB,
    /// L1-resident, 8 slots per cache line: a probe is one 64-bit load.
    table: Option<Box<[u64; 4096]>>,
    epoch: u32,
}

/// Which encoder the process uses, chosen once before its first compile: the strong one
/// (the default: a one-shot compile, PDFs as small as pdfLaTeX's) or the fast one (a
/// `--watch` session, where a save's latency matters more than its size; `src/main.rs`
/// `main`). One mode per process keeps a session's cached streams and its idle check (a
/// fresh compile in the same process) in the same mode.
static FAST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Makes every compressor made from now on use the fast encoder (fixed codes, one probe a
/// position): for watch sessions, before their first compile.
pub fn set_fast() {
    FAST.store(true, std::sync::atomic::Ordering::Relaxed);
}

const WSIZE: usize = 1 << 15;
const WMASK: usize = WSIZE - 1;
const HASH_BITS: u32 = 15;
/// Chain links followed per position, the length that ends a search early, and the length
/// below which the next position is tried before a match is taken (zlib level 6: 128,
/// 128, 16). Measured on the 61-page benchmark: chain 48/lazy 16 gives 291 KB for 366M
/// instructions, 8/8 with `MAX_INSERT` 8 gives 295 KB for 335M, 1/0 309 KB for 322M.
const MAX_CHAIN: usize = 8;
const NICE: usize = 128;
const LAZY: usize = 8;
const MATCH: u32 = 1 << 31;
/// Tokens per block: each block gets its own tables.
const BLOCK_TOKENS: usize = 1 << 14;
/// The order the code-length code's lengths are written in (RFC 1951 §3.2.7).
const CL_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// Inside a match longer than this, positions are not added to the chains (zlib's
/// `max_insert_length` for its faster levels): long repeats are found from their start.
const MAX_INSERT: usize = 8;

/// Each length symbol's extra bits and base (RFC 1951 §3.2.5), for `LEN_INFO`.
static LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
static LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
// Distance tables padded to 32 entries: a symbol taken from 5 bits indexes them with no
// bounds check.
static DIST_EXTRA: [u8; 32] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 0, 0];
static DIST_BASE: [u16; 32] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577, 0, 0];
/// Length 3..=258 to its symbol's index above 257 (bits 0-4), its extra bits (bits 8-11)
/// and their value (bits 16-23): one load a match. 512 entries, so a 9-bit length indexes
/// it with no bounds check.
static LEN_INFO: [u32; 512] = {
    let mut t = [0u32; 512];
    let mut l = 3;
    while l <= 258 {
        let s = (const_get_length_code(l).0 - 257) as usize;
        t[l] = s as u32 | (LEN_EXTRA[s] as u32) << 8 | ((l - LEN_BASE[s] as usize) as u32) << 16;
        l += 1;
    }
    t
};
/// Distance symbols as zlib's `_dist_code`: by `d - 1` below 256, else by `(d - 1) >> 7`.
static DIST_LO: [u8; 256] = {
    let mut t = [0u8; 256];
    let mut d = 1;
    while d <= 256 {
        t[d - 1] = const_get_dist_code(d).0;
        d += 1;
    }
    t
};
static DIST_HI: [u8; 256] = {
    let mut t = [0u8; 256];
    let mut k = 2;
    while k < 256 {
        t[k] = const_get_dist_code((k << 7) + 1).0;
        k += 1;
    }
    t
};

/// A match as a token: `MATCH`, the distance's symbol (bits 25-29, found once here for
/// both of `write_block`'s passes), the length (bits 16-24) and the distance (bits 0-15).
#[inline(always)]
fn match_token(len: usize, dist: usize) -> u32 {
    MATCH | (dist_sym(dist) as u32) << 25 | (len as u32) << 16 | dist as u32
}

#[inline(always)]
fn dist_sym(d: usize) -> usize {
    if d <= 256 { DIST_LO[d - 1] as usize } else { DIST_HI[(d - 1) >> 7] as usize }
}

#[inline(always)]
fn hash3(d: &[u8], i: usize) -> usize {
    // One 4-byte load where the input has a fourth byte, else three.
    let v = match d.get(i..i + 4) {
        Some(w) => u32::from_le_bytes([w[0], w[1], w[2], w[3]]) & 0x00FF_FFFF,
        None => d[i] as u32 | (d[i + 1] as u32) << 8 | (d[i + 2] as u32) << 16,
    };
    (v.wrapping_mul(0x1e35_a7bd) >> (32 - HASH_BITS)) as usize
}

/// Bytes `a` and `b` have in common from the start, at most `max`.
#[inline(always)]
fn common(d: &[u8], a: usize, b: usize, max: usize) -> usize {
    let mut n = 0;
    while n + 8 <= max {
        let x = u64::from_le_bytes(d[a + n..a + n + 8].try_into().unwrap());
        let y = u64::from_le_bytes(d[b + n..b + n + 8].try_into().unwrap());
        let diff = x ^ y;
        if diff != 0 {
            return n + (diff.trailing_zeros() / 8) as usize;
        }
        n += 8;
    }
    while n < max && d[a + n] == d[b + n] {
        n += 1;
    }
    n
}

/// Huffman code lengths for `freq`, none longer than `limit`, into `out` (0 for unused
/// symbols). At least two symbols get a code, so every code set is complete. Lengths
/// over the limit are cut as zlib's `gen_bitlen` cuts them, then given out again by
/// frequency, the rarest symbols longest.
fn code_lengths(freq: &[u32], limit: usize, out: &mut [u8]) {
    out.iter_mut().for_each(|l| *l = 0);
    // Fixed arrays on the stack, sized for the largest alphabet (288 literal/length
    // symbols), so a block's tables allocate nothing.
    const MAX_SYMS: usize = 288;
    // Each leaf as one sort key, its frequency above its symbol: the order of (frequency,
    // symbol) pairs, compared in one step. Frequencies are below 2^16 (a block has at most
    // `BLOCK_TOKENS` tokens).
    let mut leaves = [0u32; MAX_SYMS];
    let mut n = 0;
    for (s, &f) in freq.iter().enumerate() {
        if f > 0 {
            debug_assert!(f < 1 << 16);
            leaves[n] = f << 16 | s as u32;
            n += 1;
        }
    }
    for (s, _) in freq.iter().enumerate().filter(|(_, &f)| f == 0) {
        if n >= 2 {
            break;
        }
        leaves[n] = 1 << 16 | s as u32;
        n += 1;
    }
    let leaves = &mut leaves[..n];
    leaves.sort_unstable();
    // Two queues: the sorted leaves, and the internal nodes in the order made (their
    // weights never decrease). Weights are at most the block's token count plus one.
    let mut weight = [0u32; 2 * MAX_SYMS];
    for (w, l) in weight.iter_mut().zip(leaves.iter()) {
        *w = l >> 16;
    }
    let mut parent = [0u16; 2 * MAX_SYMS];
    let (mut li, mut qi) = (0, n);
    for k in n..2 * n - 1 {
        let mut pick = || {
            if li < n && (qi >= k || weight[li] <= weight[qi]) {
                li += 1;
                li - 1
            } else {
                qi += 1;
                qi - 1
            }
        };
        let (a, b) = (pick(), pick());
        weight[k] = weight[a] + weight[b];
        parent[a] = k as u16;
        parent[b] = k as u16;
    }
    let mut depth = [0u16; 2 * MAX_SYMS];
    for k in (0..2 * n - 2).rev() {
        depth[k] = depth[parent[k] as usize] + 1;
    }
    let mut count = [0usize; 64];
    let mut overflow = 0usize;
    for &d in &depth[..n] {
        let d = d as usize;
        if d > limit {
            overflow += 1;
            count[limit] += 1;
        } else {
            count[d] += 1;
        }
    }
    while overflow > 0 {
        let mut bits = limit - 1;
        while count[bits] == 0 {
            bits -= 1;
        }
        count[bits] -= 1;
        count[bits + 1] += 2;
        count[limit] -= 1;
        overflow = overflow.saturating_sub(2);
    }
    // The rarest leaves (first in `leaves`) take the longest codes.
    let mut i = 0;
    for len in (1..=limit).rev() {
        for _ in 0..count[len] {
            out[(leaves[i] & 0xFFFF) as usize] = len as u8;
            i += 1;
        }
    }
}

/// Canonical codes for `lens` (RFC 1951 §3.2.2), bit-reversed for LSB-first writing.
fn canonical(lens: &[u8], codes: &mut [u16]) {
    let mut count = [0u16; 16];
    for &l in lens {
        count[l as usize] += 1;
    }
    count[0] = 0;
    let mut next = [0u16; 16];
    let mut code = 0u16;
    for bits in 1..16 {
        code = (code + count[bits - 1]) << 1;
        next[bits] = code;
    }
    for (s, &l) in lens.iter().enumerate() {
        if l > 0 {
            codes[s] = bit_reverse_16(next[l as usize], l);
            next[l as usize] += 1;
        }
    }
}

impl Default for DeflateCompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl DeflateCompressor {
    pub fn new() -> Self {
        Self { head: Vec::new(), prev: Vec::new(), base: 0, tokens: Vec::new(), last: true, fast: FAST.load(std::sync::atomic::Ordering::Relaxed), table: None, epoch: 0 }
    }

    /// Compresses `input_data` into zlib-wrapped DEFLATE format (RFC 1950) appending to `destination_buffer`.
    pub fn compress_zlib(&mut self, input_data: &[u8], destination_buffer: &mut Vec<u8>) {
        // 1. Write zlib header: CMF = 0x78 (32K window, deflate), FLG = 0x9C (default)
        destination_buffer.extend_from_slice(&[0x78, 0x9C]);

        let start_len = destination_buffer.len();

        // 2. Compress payload using Fixed Huffman DEFLATE block
        self.compress_raw(input_data, destination_buffer);

        // If compression did not reduce size (e.g. tiny input < 32 bytes), fallback to stored block
        let compressed_payload_len = destination_buffer.len() - start_len;
        if compressed_payload_len >= input_data.len() && !input_data.is_empty() {
            destination_buffer.truncate(start_len);
            write_stored_block(input_data, destination_buffer);
        }

        // 3. Write Adler-32 checksum (big-endian)
        let checksum = adler32(input_data);
        destination_buffer.extend_from_slice(&checksum.to_be_bytes());
    }

    /// One zlib stream of `packed`, compressed, then `stored`, written as stored blocks:
    /// for data known not to compress (a Type 1 font's eexec-encrypted part), whose
    /// matching would be work for no gain. The compressed block ends with an empty stored
    /// block (a sync flush), so the stored ones start byte-aligned.
    pub fn compress_zlib_parts(&mut self, packed: &[u8], stored: &[u8], out: &mut Vec<u8>) {
        if packed.is_empty() {
            out.extend_from_slice(&[0x78, 0x9C]);
            write_stored_block_final(stored, out);
            out.extend_from_slice(&adler32(stored).to_be_bytes());
            return;
        }
        out.extend_from_slice(&[0x78, 0x9C]);
        self.last = false;
        self.compress_raw(packed, out);
        self.last = true;
        out.extend_from_slice(&[0x00, 0x00, 0xFF, 0xFF]);
        write_stored_block_final(stored, out);
        out.extend_from_slice(&adler32_from(adler32(packed), stored).to_be_bytes());
    }

    /// Raw DEFLATE of `input_data`: LZ77, then one block per `BLOCK_TOKENS` tokens, each
    /// with dynamic or fixed codes, whichever is shorter. The last block has BFINAL set
    /// unless the stream continues (`compress_zlib_parts`).
    pub fn compress_raw(&mut self, input_data: &[u8], destination_buffer: &mut Vec<u8>) {
        if input_data.is_empty() {
            let mut bit_writer = BitWriter::new(destination_buffer);
            bit_writer.write_bits(if self.last { 0b011 } else { 0b010 }, 3);
            bit_writer.write_bits(LIT_LEN_CODES[256] as u64, LIT_LEN_BITS[256]);
            if !self.last {
                bit_writer.write_bits(0, 3);
            }
            bit_writer.finish();
            return;
        }
        if self.fast {
            return self.compress_fixed(input_data, destination_buffer);
        }
        destination_buffer.reserve(input_data.len() / 2 + 64);
        self.lz77(input_data);
        let tokens = std::mem::take(&mut self.tokens);
        let mut bit_writer = BitWriter::new(destination_buffer);
        let blocks = tokens.len().div_ceil(BLOCK_TOKENS);
        for (k, block) in tokens.chunks(BLOCK_TOKENS).enumerate() {
            write_block(&mut bit_writer, block, self.last && k + 1 == blocks);
        }
        if !self.last {
            // A non-final stored block's header (BFINAL=0, BTYPE=00); the padding to the
            // byte boundary follows, then the caller's LEN and NLEN.
            bit_writer.write_bits(0, 3);
        }
        bit_writer.finish();
        self.tokens = tokens;
    }

    /// The fast encoder (the one before `04ece44`): one fixed-code block (BTYPE=01), each
    /// position probing one table slot, literals taken in steps after long misses. About
    /// 15% fewer instructions per compile than the strong one, PDFs 13-22% larger.
    fn compress_fixed(&mut self, input_data: &[u8], destination_buffer: &mut Vec<u8>) {
        destination_buffer.reserve(input_data.len() + 64);

        let total_input_length = input_data.len();
        if total_input_length <= 65535 {
            self.epoch = (self.epoch + 1) & 0x00FF_FFFF;
            if self.epoch == 0 {
                if let Some(t) = self.table.as_mut() {
                    t.fill(0);
                }
                self.epoch = 1;
            }
            self.compress_raw_inner::<true>(input_data, destination_buffer);
        } else {
            if let Some(t) = self.table.as_mut() {
                t.fill(0);
            }
            self.compress_raw_inner::<false>(input_data, destination_buffer);
        }
    }

    #[inline(always)]
    fn compress_raw_inner<const USE_EPOCH: bool>(&mut self, input_data: &[u8], destination_buffer: &mut Vec<u8>) {
        let last = self.last;
        let table = self.table.get_or_insert_with(|| Box::new([0u64; 4096]));
        let total_input_length = input_data.len();
        let curr_epoch = self.epoch;
        let epoch_high = (curr_epoch as u64) << 40;

        let mut bit_writer = BitWriter::new(destination_buffer);
        bit_writer.write_bits(if last { 0b011 } else { 0b010 }, 3); // BFINAL, BTYPE=01 (Fixed Huffman)

        let mut current_position = 0;
        let mut misses = 0u32;

        while current_position + 4 <= total_input_length {
            let word = u32::from_le_bytes(
                input_data[current_position..current_position + 4]
                    .try_into()
                    .unwrap(),
            );
            let prefix3 = word & 0x00FF_FFFF;
            let hash_index = (prefix3.wrapping_mul(0x1e35a7bd) >> 20) as usize & 4095;

            let entry = table[hash_index];

            let (is_valid, previous_index) = if USE_EPOCH {
                let expected = epoch_high | ((prefix3 as u64) << 16);
                let valid = ((entry ^ expected) & 0xFFFF_FFFF_FFFF_0000) == 0;
                let prev_idx = (entry as u16).wrapping_sub(1) as usize;
                table[hash_index] = expected | ((current_position as u32 + 1) as u64);
                (valid, prev_idx)
            } else {
                let expected = (prefix3 as u64) << 32;
                let valid = (entry >> 32) == (prefix3 as u64) && (entry as u32) > 0;
                let prev_idx = (entry as u32).wrapping_sub(1) as usize;
                table[hash_index] = expected | ((current_position as u32 + 1) as u64);
                (valid, prev_idx)
            };

            let mut match_length = 0;
            let mut match_distance = 0;

            // Verified match: if prefix matches in L1 cache, first 3 bytes are guaranteed!
            if is_valid {
                match_distance = current_position - previous_index;
                if match_distance > 0 && match_distance <= 32768 {
                    match_length = 3;
                    let max_match = (total_input_length - current_position).min(258);
                    while match_length + 8 <= max_match {
                        let p1 = u64::from_le_bytes(
                            input_data
                                [previous_index + match_length..previous_index + match_length + 8]
                                .try_into()
                                .unwrap(),
                        );
                        let p2 = u64::from_le_bytes(
                            input_data[current_position + match_length
                                ..current_position + match_length + 8]
                                .try_into()
                                .unwrap(),
                        );
                        let diff = p1 ^ p2;
                        if diff != 0 {
                            match_length += (diff.trailing_zeros() / 8) as usize;
                            break;
                        }
                        match_length += 8;
                    }
                    while match_length < max_match
                        && input_data[previous_index + match_length]
                            == input_data[current_position + match_length]
                    {
                        match_length += 1;
                    }
                }
            }

            if match_length >= 3 {
                let len_entry = LENGTH_TABLE[match_length - 3];
                let len_bits = (len_entry & 0xFFFF) as u64;
                let len_count = (len_entry >> 16) as u8;

                let (dist_bits, dist_count) = if match_distance <= 2048 {
                    let dist_entry = DIST_TABLE_2048[match_distance];
                    ((dist_entry & 0xFFFF) as u64, (dist_entry >> 16) as u8)
                } else {
                    let (code, extra_bits, extra_val) = const_get_dist_code(match_distance);
                    let bits = ((DIST_CODES[code as usize] as u32) | ((extra_val as u32) << 5)) as u64;
                    (bits, 5 + extra_bits)
                };

                let combined = len_bits | (dist_bits << len_count);
                bit_writer.write_bits(combined, len_count + dist_count);

                current_position += match_length;
                misses = 0;
            } else {
                let literal_byte = input_data[current_position] as usize;
                let entry = LIT_TABLE[literal_byte];
                bit_writer.write_bits((entry & 0xFFFF) as u64, (entry >> 16) as u8);
                current_position += 1;
                misses += 1;
                if misses > 32 {
                    let step = (misses >> 5).min(4) as usize;
                    let fast_literals =
                        (step - 1).min(total_input_length.saturating_sub(current_position + 4));
                    let mut k = 0;
                    while k + 1 < fast_literals {
                        let e0 = LIT_TABLE[input_data[current_position] as usize];
                        let e1 = LIT_TABLE[input_data[current_position + 1] as usize];
                        let c0 = (e0 & 0xFFFF) as u64;
                        let b0 = (e0 >> 16) as u8;
                        let c1 = (e1 & 0xFFFF) as u64;
                        let b1 = (e1 >> 16) as u8;
                        bit_writer.write_bits(c0 | (c1 << b0), b0 + b1);
                        current_position += 2;
                        k += 2;
                    }
                    if k < fast_literals {
                        let entry = LIT_TABLE[input_data[current_position] as usize];
                        bit_writer.write_bits((entry & 0xFFFF) as u64, (entry >> 16) as u8);
                        current_position += 1;
                    }
                }
            }
        }

        // Emit remaining trailing literal bytes (batched 2 at a time)
        while current_position + 1 < total_input_length {
            let e0 = LIT_TABLE[input_data[current_position] as usize];
            let e1 = LIT_TABLE[input_data[current_position + 1] as usize];
            let c0 = (e0 & 0xFFFF) as u64;
            let b0 = (e0 >> 16) as u8;
            let c1 = (e1 & 0xFFFF) as u64;
            let b1 = (e1 >> 16) as u8;
            bit_writer.write_bits(c0 | (c1 << b0), b0 + b1);
            current_position += 2;
        }
        if current_position < total_input_length {
            let entry = LIT_TABLE[input_data[current_position] as usize];
            bit_writer.write_bits((entry & 0xFFFF) as u64, (entry >> 16) as u8);
        }

        // Emit End of Block (EOB = 256)
        bit_writer.write_bits(LIT_LEN_CODES[256] as u64, LIT_LEN_BITS[256]);
        if !last {
            // A non-final stored block's header (BFINAL=0, BTYPE=00); the padding to the
            // byte boundary follows, then the caller's LEN and NLEN.
            bit_writer.write_bits(0, 3);
        }
        bit_writer.finish();
    }

    /// `d` as LZ77 tokens in `self.tokens`, with one position of deferred matching: a
    /// match shorter than `LAZY` is kept only if the next position has none longer.
    fn lz77(&mut self, d: &[u8]) {
        if self.head.is_empty() {
            self.head = vec![0; 1 << HASH_BITS];
            self.prev = vec![0; WSIZE];
        }
        // Positions of this input are stored above `base`; move it past the last input's.
        if self.base > u32::MAX - (d.len() as u32).saturating_add(2 * WSIZE as u32) - (1 << 20) {
            self.head.fill(0);
            self.prev.fill(0);
            self.base = 0;
        }
        self.tokens.clear();
        self.tokens.reserve(d.len());
        // The chains as fixed-size arrays: a hash or a masked position indexes them with no
        // bounds check.
        let mut chains = Chains { head: (&mut self.head[..]).try_into().expect("head"), prev: (&mut self.prev[..]).try_into().expect("prev"), base: self.base };
        let tokens = &mut self.tokens;
        let n = d.len();
        let mut i = 0;
        let mut pending: Option<(usize, usize)> = None;
        while i < n {
            let found = if i + 3 <= n {
                let cand = chains.insert(d, i);
                chains.longest(d, i, cand, pending.map_or(0, |p| p.0))
            } else {
                (0, 0)
            };
            if let Some((len, dist)) = pending {
                if found.0 > len {
                    // A longer match one later: the deferred position becomes a literal.
                    tokens.push(d[i - 1] as u32);
                    pending = Some(found);
                    i += 1;
                    continue;
                }
                tokens.push(match_token(len, dist));
                let end = i - 1 + len;
                if len <= MAX_INSERT {
                    for j in i + 1..end.min(n.saturating_sub(2)) {
                        chains.insert(d, j);
                    }
                }
                pending = None;
                i = end;
                continue;
            }
            if found.0 >= 3 {
                if found.0 < LAZY && i + 1 < n {
                    pending = Some(found);
                    i += 1;
                    continue;
                }
                tokens.push(match_token(found.0, found.1));
                if found.0 <= MAX_INSERT {
                    for j in i + 1..(i + found.0).min(n.saturating_sub(2)) {
                        chains.insert(d, j);
                    }
                }
                i += found.0;
                continue;
            }
            tokens.push(d[i] as u32);
            i += 1;
        }
        if let Some((len, dist)) = pending {
            tokens.push(match_token(len, dist));
        }
        self.base = self.base.wrapping_add(n as u32 + WSIZE as u32 + 1);
    }
}

/// The hash chains of one input (`DeflateCompressor::head`, `prev`, `base`), borrowed as
/// fixed-size arrays.
struct Chains<'a> {
    head: &'a mut [u32; 1 << HASH_BITS],
    prev: &'a mut [u32; WSIZE],
    base: u32,
}

impl Chains<'_> {
    /// Adds position `i` to the chains; the previous position with the same hash.
    #[inline(always)]
    fn insert(&mut self, d: &[u8], i: usize) -> u32 {
        let h = hash3(d, i);
        let old = self.head[h];
        self.head[h] = self.base + i as u32 + 1;
        self.prev[i & WMASK] = old;
        old
    }

    /// The longest match for position `i` longer than `shorter`, from chain entry `cand`:
    /// (length, distance), or (0, 0). A candidate is measured only when its first two
    /// bytes and the two ending at the length to beat agree with `i`'s (zlib's `scan_end`
    /// test): any other candidate is shorter than the best so far, so the result is the one
    /// the byte-by-byte test gives.
    #[inline(always)]
    fn longest(&self, d: &[u8], i: usize, mut cand: u32, shorter: usize) -> (usize, usize) {
        let max = (d.len() - i).min(258);
        let mut best = shorter.max(2);
        // No candidate (an empty chain or one from an earlier input): nothing to read.
        if best >= max || cand <= self.base {
            return (0, 0);
        }
        let pair = |k: usize| u16::from_le_bytes(d[k..k + 2].try_into().unwrap());
        let start = pair(i);
        let mut end = pair(i + best - 1);
        let mut dist_best = 0;
        let mut chain = if shorter >= 32 { MAX_CHAIN / 4 } else { MAX_CHAIN };
        while cand > self.base && chain > 0 {
            let p = (cand - self.base - 1) as usize;
            let dist = i - p;
            if dist >= WSIZE {
                break;
            }
            if pair(p + best - 1) == end && pair(p) == start {
                let len = common(d, p, i, max);
                if len > best {
                    best = len;
                    dist_best = dist;
                    if len >= NICE.min(max) {
                        break;
                    }
                    end = pair(i + best - 1);
                }
            }
            cand = self.prev[p & WMASK];
            chain -= 1;
        }
        if dist_best == 0 {
            (0, 0)
        } else {
            (best, dist_best)
        }
    }
}

/// One block of `tokens`: dynamic codes, or fixed ones where the block comes out shorter
/// with them (small blocks, where the tables cost more than they save).
fn write_block(w: &mut BitWriter, tokens: &[u32], last: bool) {
    let mut lf = [0u32; 286];
    // Length and distance symbols counted apart in 32-entry arrays (indexed by 5 bits),
    // the lengths folded into `lf` after.
    let mut lenf = [0u32; 32];
    let mut df32 = [0u32; 32];
    let mut extra = 0u64;
    for &t in tokens {
        if t & MATCH == 0 {
            lf[(t & 0xFF) as usize] += 1;
        } else {
            let info = LEN_INFO[((t >> 16) & 0x1FF) as usize];
            let ds = (t >> 25 & 31) as usize;
            lenf[(info & 31) as usize] += 1;
            df32[ds] += 1;
            extra += (info >> 8 & 15) as u64 + DIST_EXTRA[ds] as u64;
        }
    }
    lf[257..].copy_from_slice(&lenf[..29]);
    let mut df = [0u32; 30];
    df.copy_from_slice(&df32[..30]);
    lf[256] = 1;
    let mut ll = [0u8; 286];
    let mut dl = [0u8; 30];
    code_lengths(&lf, 15, &mut ll);
    code_lengths(&df, 15, &mut dl);
    let hlit = (257..=286).rev().find(|&n| ll[n - 1] != 0).unwrap_or(257).max(257);
    let hdist = (1..=30).rev().find(|&n| dl[n - 1] != 0).unwrap_or(1).max(1);
    // The code lengths run-length coded (16: repeat the last 3-6 times, 17: 3-10 zeros,
    // 18: 11-138 zeros), as (symbol, extra value).
    let mut seq = Vec::with_capacity(hlit + hdist);
    seq.extend_from_slice(&ll[..hlit]);
    seq.extend_from_slice(&dl[..hdist]);
    let mut rle: Vec<(u8, u8)> = Vec::with_capacity(seq.len());
    let mut i = 0;
    while i < seq.len() {
        let v = seq[i];
        let mut run = seq[i..].iter().take_while(|&&x| x == v).count();
        if v == 0 {
            while run >= 11 {
                let r = run.min(138);
                rle.push((18, (r - 11) as u8));
                run -= r;
                i += r;
            }
            if run >= 3 {
                rle.push((17, (run - 3) as u8));
                i += run;
                run = 0;
            }
        } else {
            rle.push((v, 0));
            i += 1;
            run -= 1;
            while run >= 3 {
                let r = run.min(6);
                rle.push((16, (r - 3) as u8));
                run -= r;
                i += r;
            }
        }
        for _ in 0..run {
            rle.push((v, 0));
            i += 1;
        }
    }
    let mut cf = [0u32; 19];
    for &(s, _) in &rle {
        cf[s as usize] += 1;
    }
    let mut cl = [0u8; 19];
    code_lengths(&cf, 7, &mut cl);
    let hclen = (4..=19).rev().find(|&n| cl[CL_ORDER[n - 1]] != 0).unwrap_or(4).max(4);
    let header_bits: u64 = 14 + 3 * hclen as u64 + rle.iter().map(|&(s, _)| cl[s as usize] as u64 + [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 3, 7][s as usize]).sum::<u64>();
    let dyn_bits = header_bits + lf.iter().zip(&ll).map(|(&f, &l)| f as u64 * l as u64).sum::<u64>() + df.iter().zip(&dl).map(|(&f, &l)| f as u64 * l as u64).sum::<u64>() + extra;
    let fixed_bits = lf.iter().enumerate().map(|(s, &f)| f as u64 * LIT_LEN_BITS[s] as u64).sum::<u64>() + df.iter().map(|&f| f as u64 * 5).sum::<u64>() + extra;
    let mut lc = [0u16; 286];
    let mut dc = [0u16; 30];
    let fin = last as u64;
    if fixed_bits <= dyn_bits {
        w.write_bits(fin | 0b010, 3);
        lc.copy_from_slice(&LIT_LEN_CODES[..286]);
        ll.copy_from_slice(&LIT_LEN_BITS[..286]);
        for s in 0..30 {
            dc[s] = DIST_CODES[s] as u16;
            dl[s] = 5;
        }
    } else {
        w.write_bits(fin | 0b100, 3);
        w.write_bits((hlit - 257) as u64, 5);
        w.write_bits((hdist - 1) as u64, 5);
        w.write_bits((hclen - 4) as u64, 4);
        for &s in &CL_ORDER[..hclen] {
            w.write_bits(cl[s] as u64, 3);
        }
        let mut cc = [0u16; 19];
        canonical(&cl, &mut cc);
        for &(s, x) in &rle {
            w.write_bits(cc[s as usize] as u64, cl[s as usize]);
            match s {
                16 => w.write_bits(x as u64, 2),
                17 => w.write_bits(x as u64, 3),
                18 => w.write_bits(x as u64, 7),
                _ => {}
            }
        }
        canonical(&ll, &mut lc);
        canonical(&dl, &mut dc);
    }
    // Each symbol's code and length in one word (AoS, one load a token): code in the low
    // 16 bits, length above.
    // Literals by byte, length symbols and distance symbols in 32-entry arrays.
    let mut lit = [0u32; 256];
    for s in 0..256 {
        lit[s] = lc[s] as u32 | (ll[s] as u32) << 16;
    }
    let mut lens = [0u32; 32];
    for s in 0..29 {
        lens[s] = lc[257 + s] as u32 | (ll[257 + s] as u32) << 16;
    }
    let mut dis = [0u32; 32];
    for s in 0..30 {
        dis[s] = dc[s] as u32 | (dl[s] as u32) << 16;
    }
    for &t in tokens {
        if t & MATCH == 0 {
            let e = lit[(t & 0xFF) as usize];
            w.write_bits((e & 0xFFFF) as u64, (e >> 16) as u8);
        } else {
            let info = LEN_INFO[((t >> 16) & 0x1FF) as usize];
            let dist = (t & 0xFFFF) as usize;
            let ds = (t >> 25 & 31) as usize;
            let (lx, dx) = ((info >> 16) as u64, (dist - DIST_BASE[ds] as usize) as u64);
            let (le, de) = (lens[(info & 31) as usize], dis[ds]);
            // The length's code and extra bits, then the distance's, as one write of at most
            // 15 + 5 + 15 + 13 = 48 bits.
            let lbits = (le >> 16) as u8 + (info >> 8 & 15) as u8;
            let dbits = (de >> 16) as u8 + DIST_EXTRA[ds];
            let lv = (le & 0xFFFF) as u64 | lx << (le >> 16);
            let dv = (de & 0xFFFF) as u64 | dx << (de >> 16);
            w.write_bits(lv | dv << lbits, lbits + dbits);
        }
    }
    w.write_bits(lc[256] as u64, ll[256]);
}

/// `input` as byte-aligned stored blocks, the last final; one empty final block if empty.
fn write_stored_block_final(input: &[u8], out: &mut Vec<u8>) {
    if input.is_empty() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    } else {
        write_stored_block(input, out);
    }
}

fn write_stored_block(input: &[u8], out: &mut Vec<u8>) {
    for chunk in input.chunks(65535) {
        let is_last =
            (chunk.as_ptr() as usize + chunk.len()) == (input.as_ptr() as usize + input.len());
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
    fn parts_inflate_to_both() {
        let head = b"%!PS-AdobeFont-1.0: CMR10 /Encoding 256 array dup 65 /A put readonly def ".repeat(20);
        let tail: Vec<u8> = (0..70000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
        // Both encoders: the strong one and the fast one of watch sessions.
        for fast in [false, true] {
            let mut out = Vec::new();
            let mut comp = DeflateCompressor::new();
            comp.fast = fast;
            comp.compress_zlib_parts(&head, &tail, &mut out);
            let mut back = Vec::new();
            crate::backend::backend_inflate::zlib_decompress(&out, &mut back).expect("inflates");
            assert_eq!(back.len(), head.len() + tail.len());
            assert_eq!(&back[..head.len()], &head[..]);
            assert_eq!(&back[head.len()..], &tail[..]);
            assert_eq!(adler32_from(adler32(&head), &tail), adler32(&back));
        }
    }

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

        let large_repeats =
            b"The quick brown fox jumps over the lazy dog. 1234567890\n".repeat(2000); // ~114 KB
        let cases: &[&[u8]] = &[
            b"",
            b"a",
            b"Hello world!",
            b"The quick brown fox jumps over the lazy dog. The quick brown fox jumps over the lazy dog.",
            include_bytes!("../../examples/sample.tex"),
            &large_repeats,
        ];

        let mut comp = DeflateCompressor::new();
        // Each case twice: the strong encoder, then the fast one of watch sessions.
        for (k, &input) in cases.iter().chain(cases).enumerate() {
            comp.fast = k >= cases.len();
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

            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(&compressed)
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, input);
        }
    }
}
