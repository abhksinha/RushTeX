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
pub static DIST_BITS: [u8; 32] = [5u8; 32];

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
    let mut s1 = 1u32;
    let mut s2 = 0u32;
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

/// Bit writer accumulator with 256-byte L1 stack buffer to eliminate per-word heap vector churn.
struct BitWriter<'a> {
    buf: &'a mut Vec<u8>,
    bit_buf: u64,
    bit_count: u8,
    stack: [u8; 256],
    stack_len: usize,
}

impl<'a> BitWriter<'a> {
    #[inline(always)]
    fn new(buf: &'a mut Vec<u8>) -> Self {
        Self {
            buf,
            bit_buf: 0,
            bit_count: 0,
            stack: [0u8; 256],
            stack_len: 0,
        }
    }

    #[inline(always)]
    fn write_bits(&mut self, val: u64, count: u8) {
        self.bit_buf |= val << self.bit_count;
        self.bit_count += count;
        if self.bit_count >= 32 {
            let low = (self.bit_buf as u32).to_le_bytes();
            let slen = self.stack_len;
            self.stack[slen..slen + 4].copy_from_slice(&low);
            self.stack_len = slen + 4;
            self.bit_buf >>= 32;
            self.bit_count -= 32;
            if self.stack_len >= 240 {
                self.buf.extend_from_slice(&self.stack[..self.stack_len]);
                self.stack_len = 0;
            }
        }
    }

    #[inline(always)]
    fn finish(mut self) {
        let bytes = (self.bit_count as usize).div_ceil(8);
        if bytes > 0 {
            let le = self.bit_buf.to_le_bytes();
            let slen = self.stack_len;
            self.stack[slen..slen + bytes].copy_from_slice(&le[..bytes]);
            self.stack_len = slen + bytes;
        }
        if self.stack_len > 0 {
            self.buf.extend_from_slice(&self.stack[..self.stack_len]);
        }
    }
}

/// High-performance zlib/DEFLATE compressor.
///
/// Uses a Structure-of-Arrays (SoA) match table:
/// Single-pass streaming DEFLATE compressor.
///
/// Uses an AoS (Array-of-Structures) packed match table:
/// - `table`: 4096 x u64 (32 KB) table storing packed `(prefix3 << 32) | pos`.
/// - Fits 100% inside 32 KB CPU L1 data cache. Exactly 8 slots fit per 64-byte L1 cache line.
/// - Single 64-bit load/store per probe eliminates dual-array cache set conflicts and cuts
///   L1 cache line accesses in half.
pub struct DeflateCompressor {
    table: [u64; 4096],
    epoch: u32,
}

impl Default for DeflateCompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl DeflateCompressor {
    pub fn new() -> Self {
        Self {
            table: [0u64; 4096],
            epoch: 0,
        }
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

    /// Compresses raw DEFLATE stream with Fixed Huffman codes (BFINAL=1, BTYPE=01).
    pub fn compress_raw(&mut self, input_data: &[u8], destination_buffer: &mut Vec<u8>) {
        if input_data.is_empty() {
            let mut bit_writer = BitWriter::new(destination_buffer);
            bit_writer.write_bits(0b011, 3); // BFINAL=1, BTYPE=01
            bit_writer.write_bits(LIT_LEN_CODES[256] as u64, LIT_LEN_BITS[256]); // EOB
            bit_writer.finish();
            return;
        }

        destination_buffer.reserve(input_data.len() + 64);

        let total_input_length = input_data.len();
        if total_input_length <= 65535 {
            self.epoch = (self.epoch + 1) & 0x00FF_FFFF;
            if self.epoch == 0 {
                self.table.fill(0);
                self.epoch = 1;
            }
            self.compress_raw_inner::<true>(input_data, destination_buffer);
        } else {
            self.table.fill(0);
            self.compress_raw_inner::<false>(input_data, destination_buffer);
        }
    }

    #[inline(always)]
    fn compress_raw_inner<const USE_EPOCH: bool>(
        &mut self,
        input_data: &[u8],
        destination_buffer: &mut Vec<u8>,
    ) {
        let total_input_length = input_data.len();
        let curr_epoch = self.epoch;
        let epoch_high = (curr_epoch as u64) << 40;

        let mut bit_writer = BitWriter::new(destination_buffer);
        bit_writer.write_bits(0b011, 3); // BFINAL=1, BTYPE=01 (Fixed Huffman)

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

            let entry = self.table[hash_index];

            let (is_valid, previous_index) = if USE_EPOCH {
                let expected = epoch_high | ((prefix3 as u64) << 16);
                let valid = ((entry ^ expected) & 0xFFFF_FFFF_FFFF_0000) == 0;
                let prev_idx = (entry as u16).wrapping_sub(1) as usize;
                self.table[hash_index] = expected | ((current_position as u32 + 1) as u64);
                (valid, prev_idx)
            } else {
                let expected = (prefix3 as u64) << 32;
                let valid = (entry >> 32) == (prefix3 as u64) && (entry as u32) > 0;
                let prev_idx = (entry as u32).wrapping_sub(1) as usize;
                self.table[hash_index] = expected | ((current_position as u32 + 1) as u64);
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
        bit_writer.finish();
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
