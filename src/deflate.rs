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

    /// Compresses `input_data` into zlib-wrapped DEFLATE format (RFC 1950) appending to `destination_buffer`.
    pub fn compress_zlib(&mut self, input_data: &[u8], destination_buffer: &mut Vec<u8>) {
        // 1. Write zlib header: CMF = 0x78 (32K window, deflate), FLG = 0x9C (default)
        destination_buffer.push(0x78);
        destination_buffer.push(0x9C);

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
            bit_writer.write_bits(LIT_LEN_CODES[256] as u32, LIT_LEN_BITS[256]); // EOB
            bit_writer.finish();
            return;
        }

        // Clear hash table (32 KB memset - instantaneous in L1 cache)
        self.head.fill(0);

        let mut bit_writer = BitWriter::new(destination_buffer);
        bit_writer.write_bits(0b011, 3); // BFINAL=1, BTYPE=01 (Fixed Huffman)

        let total_input_length = input_data.len();
        let mut current_position = 0;

        while current_position + 3 <= total_input_length {
            // 3-byte multiplicative hash
            let byte_0 = input_data[current_position] as u32;
            let byte_1 = input_data[current_position + 1] as u32;
            let byte_2 = input_data[current_position + 2] as u32;
            let hash_index = (((byte_0 << 10) ^ (byte_1 << 5) ^ byte_2).wrapping_mul(0x1e35a7bd) >> 19) as usize & 8191;

            let previous_position = self.head[hash_index];
            self.head[hash_index] = (current_position + 1) as u32;

            let mut match_length = 0;
            let mut match_distance = 0;

            // Candidate match validation (within 32K window and valid position)
            if previous_position > 0 {
                let previous_index = (previous_position - 1) as usize;
                match_distance = current_position - previous_index;
                if match_distance > 0 && match_distance <= 32768 && previous_index + 2 < total_input_length {
                    if input_data[previous_index] == input_data[current_position]
                        && input_data[previous_index + 1] == input_data[current_position + 1]
                        && input_data[previous_index + 2] == input_data[current_position + 2]
                    {
                        match_length = 3;
                        let max_match = (total_input_length - current_position).min(258);
                        while match_length < max_match && input_data[previous_index + match_length] == input_data[current_position + match_length] {
                            match_length += 1;
                        }
                    }
                }
            }

            if match_length >= 3 {
                // Emit length code
                let (length_code, length_extra_bits, length_extra_val) = get_length_code(match_length);
                bit_writer.write_bits(LIT_LEN_CODES[length_code as usize] as u32, LIT_LEN_BITS[length_code as usize]);
                if length_extra_bits > 0 {
                    bit_writer.write_bits(length_extra_val as u32, length_extra_bits);
                }

                // Emit distance code
                let (distance_code, distance_extra_bits, distance_extra_val) = get_dist_code(match_distance);
                bit_writer.write_bits(DIST_CODES[distance_code as usize] as u32, DIST_BITS[distance_code as usize]);
                if distance_extra_bits > 0 {
                    bit_writer.write_bits(distance_extra_val as u32, distance_extra_bits);
                }

                current_position += match_length;
            } else {
                let literal_byte = input_data[current_position] as usize;
                bit_writer.write_bits(LIT_LEN_CODES[literal_byte] as u32, LIT_LEN_BITS[literal_byte]);
                current_position += 1;
            }
        }

        // Emit remaining trailing literal bytes
        while current_position < total_input_length {
            let literal_byte = input_data[current_position] as usize;
            bit_writer.write_bits(LIT_LEN_CODES[literal_byte] as u32, LIT_LEN_BITS[literal_byte]);
            current_position += 1;
        }

        // Emit End of Block (EOB = 256)
        bit_writer.write_bits(LIT_LEN_CODES[256] as u32, LIT_LEN_BITS[256]);
        bit_writer.finish();
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
