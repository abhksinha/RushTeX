//! # The `hz`-Algorithm (Knuth-Plass with Microtypography)
//!
//! Provides Hermann Zapf and Peter Karow's `hz`-algorithm, which enhances
//! the Knuth-Plass line-breaking model with two microtypographic degrees of freedom:
//!
//! 1. **Font Expansion (Glyph Scaling)**: Subtle horizontal scaling of characters
//!    ($\pm 1.25\%$ to $\pm 1.5\%$) to absorb slack without visibly altering typographic color.
//! 2. **Optical Margin Protrusion (Hanging Punctuation)**: Allowing low-density punctuation
//!    marks (hyphens, periods, commas, quotes) to protrude into the right margin, creating
//!    a visually razor-sharp column edge.
//!
//! ## Hardware-Level Architecture & Bit-Packing Principles
//!
//! - **16-Byte Power-of-Two [`HzLineDescriptor`]**: Bit-packs indentation, formatted width,
//!   word range, space count, expansion per-mille, justification flag, and optical protrusion
//!   into **exactly 16 bytes** with zero padding. Exactly **4 descriptors fit per 64-byte L1 CPU cache line**.
//! - **4-Byte Packed [`HzConfig`]**: Configuration flags and thresholds fit in a single 32-bit register.
//! - **64-Byte L1-Cache Aligned Protrusion LUT**: 128-byte `.rodata` lookup table gives $O(1)$ branchless
//!   protrusion factors for all printable ASCII punctuation in Cache Line 0.
//! - **Stack-Resident Dynamic Programming**: Shortest-path DAG optimization uses a 2 KB stack buffer
//!   for paragraphs $\le 256$ words, performing zero heap allocations during line breaking.
//!
//! In PDF output, font expansion is rendered natively via the standard PDF `Tz` operator
//! (Horizontal Text Scaling: e.g. `101.2 Tz`), requiring zero virtual fonts and zero extra memory.

use crate::font_metrics::Face;
use crate::layout::layout_hyphen::{
    hash_word, HyphenCacheEntry, DEFAULT_LEFT_HYPHEN_MIN, DEFAULT_RIGHT_HYPHEN_MIN,
    HYPHEN_CACHE_SIZE,
};
use crate::metrics::{self, FontFamily};
use crate::page_layout::{PackedWordAttr, ParagraphSoA, WordKind};
use std::ops::Deref;

/// Maximum font expansion in per-mille (+1.5% horizontal width).
pub const MAX_EXPANSION_PERMILLE: i8 = 15;

/// Maximum font compression in per-mille (-1.5% horizontal width).
pub const MIN_EXPANSION_PERMILLE: i8 = -15;

/// 64-byte L1-cache aligned ASCII protrusion factor table (percentage: 0..100).
#[repr(align(64))]
pub struct ProtrusionTable(pub [u8; 128]);

impl Deref for ProtrusionTable {
    type Target = [u8; 128];
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Compile-time precomputed protrusion factors for ASCII punctuation.
/// All common punctuation characters (33..63: `!`, `"`, `'`, `(`, `)`, `,`, `-`, `.`, `:`, `;`, `?`)
/// reside entirely within **L1 Cache Line 0** (bytes 0..63).
pub static PROTRUSION_ASCII_LUT: ProtrusionTable = {
    let mut table = [0u8; 128];
    table[b'.' as usize] = 70; // 70% protrusion
    table[b',' as usize] = 70; // 70% protrusion
    table[b'-' as usize] = 75; // 75% protrusion
    table[b';' as usize] = 50; // 50% protrusion
    table[b':' as usize] = 50; // 50% protrusion
    table[b'\'' as usize] = 40; // 40% protrusion
    table[b'"' as usize] = 40; // 40% protrusion
    table[b'!' as usize] = 30; // 30% protrusion
    table[b'?' as usize] = 30; // 30% protrusion
    table[b')' as usize] = 25; // 25% protrusion
    table[b']' as usize] = 25; // 25% protrusion
    ProtrusionTable(table)
};

/// 64-byte L1-cache aligned ASCII space factor code table (TeX by Topic Ch 20).
#[repr(align(64))]
pub struct SpaceFactorTable(pub [u16; 128]);

impl Deref for SpaceFactorTable {
    type Target = [u16; 128];
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// 64-byte L1-cache aligned 512-byte flat ASCII font width lookup table (256 entries * 2 bytes = 512 bytes).
/// Fits within exactly 8 contiguous 64-byte L1 data cache lines.
/// Maps any raw `u8` byte value directly to its 1000-units/em advance width, eliminating bounds checks
/// and branch instructions during hot-loop word explosion and text measurement.
#[repr(align(64))]
#[derive(Clone, Copy, Debug)]
pub struct FlatAsciiWidthTable(pub [u16; 256]);

impl Deref for FlatAsciiWidthTable {
    type Target = [u16; 256];
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FlatAsciiWidthTable {
    /// Constructs a 512-byte flat width table from a 128-entry ASCII font width table.
    /// Indices 0..128 contain authoritative glyph advance metrics; indices 128..256 are initialized to 0.
    #[inline(always)]
    pub const fn from_table(table: &[u16; 128]) -> Self {
        let mut flat = [0u16; 256];
        let mut i = 0;
        while i < 128 {
            flat[i] = table[i];
            i += 1;
        }
        FlatAsciiWidthTable(flat)
    }

    /// Branchless O(1) character advance lookup for any raw byte without bounds check.
    #[inline(always)]
    pub fn get_advance_units(&self, byte: u8) -> u16 {
        self.0[byte as usize]
    }

    /// Computes total width units for an ASCII string using branchless flat lookups.
    #[inline(always)]
    pub fn string_units(&self, text: &str) -> u32 {
        let bytes = text.as_bytes();
        let mut total = 0u32;
        for &b in bytes {
            total += self.0[b as usize] as u32;
        }
        total
    }

    /// Computes width units for a string, using fast branchless flat ASCII lookups if all bytes
    /// are ASCII (< 128) and delegating to the authoritative fallback table if any byte is non-ASCII.
    #[inline(always)]
    pub fn string_units_or(&self, text: &str, fallback_table: &[u16; 128]) -> u32 {
        let bytes = text.as_bytes();
        let mut total = 0u32;
        let mut is_ascii = true;
        for &b in bytes {
            if b >= 128 {
                is_ascii = false;
                break;
            }
            total += self.0[b as usize] as u32;
        }
        if is_ascii {
            total
        } else {
            metrics::string_units_table(text, fallback_table)
        }
    }
}

/// Compile-time precomputed space factor codes under standard TeX `\nonfrenchspacing`.
/// Standard letters are 1000, uppercase letters are 999, transparent closing delimiters are 0,
/// and sentence-ending punctuation ranges from 1250 to 3000.
pub static DEFAULT_SFCODE_LUT: SpaceFactorTable = {
    let mut table = [1000u16; 128];
    let mut b = b'A';
    while b <= b'Z' {
        table[b as usize] = 999;
        b += 1;
    }
    // Transparent characters (sfcode = 0): retain existing space factor
    table[b')' as usize] = 0;
    table[b']' as usize] = 0;
    table[b'}' as usize] = 0;
    table[b'\'' as usize] = 0;
    table[b'"' as usize] = 0;
    table[0] = 0;

    // Punctuation codes under \nonfrenchspacing (TeX by Topic Section 20.5)
    table[b'.' as usize] = 3000;
    table[b'?' as usize] = 3000;
    table[b'!' as usize] = 3000;
    table[b':' as usize] = 2000;
    table[b';' as usize] = 1500;
    table[b',' as usize] = 1250;
    SpaceFactorTable(table)
};

/// Compile-time precomputed space factor codes under `\frenchspacing`.
pub static FRENCH_SFCODE_LUT: SpaceFactorTable = {
    let mut table = [1000u16; 128];
    let mut b = b'A';
    while b <= b'Z' {
        table[b as usize] = 999;
        b += 1;
    }
    table[b')' as usize] = 0;
    table[b']' as usize] = 0;
    table[b'}' as usize] = 0;
    table[b'\'' as usize] = 0;
    table[b'"' as usize] = 0;
    table[0] = 0;
    SpaceFactorTable(table)
};

/// Branchless lookup of space factor code for an ASCII character.
#[inline(always)]
pub fn sfcode(byte: u8, french_spacing: bool) -> u16 {
    if byte < 128 {
        if french_spacing {
            FRENCH_SFCODE_LUT[byte as usize]
        } else {
            DEFAULT_SFCODE_LUT[byte as usize]
        }
    } else {
        1000
    }
}

/// Updates the current space factor after encountering character byte `b`,
/// strictly observing TeX's initials rule and transparent delimiter rule.
#[inline(always)]
pub fn update_space_factor(current_sf: u16, byte: u8, french_spacing: bool) -> u16 {
    let code = sfcode(byte, french_spacing);
    let resolved = if code > 1000 && current_sf < 1000 { 1000 } else { code };
    if code == 0 { current_sf } else { resolved }
}

/// SIMD-width 8-byte chunk classifier: returns true iff all 8 bytes are ASCII lowercase letters ('a'..='z').
/// Evaluated branchlessly using unsigned saturating subtraction.
#[inline(always)]
pub fn is_all_ascii_lowercase_8(slice: &[u8]) -> bool {
    if slice.len() < 8 {
        return false;
    }
    let mut ok = true;
    let mut i = 0;
    while i < 8 {
        ok &= slice[i].wrapping_sub(b'a') < 26;
        i += 1;
    }
    ok
}

/// SIMD-width 16-byte chunk classifier: returns true iff all 16 bytes are ASCII lowercase letters ('a'..='z').
/// Evaluated branchlessly across 16 bytes using unsigned saturating subtraction.
#[inline(always)]
pub fn is_all_ascii_lowercase_16(slice: &[u8]) -> bool {
    if slice.len() < 16 {
        return false;
    }
    let mut ok = true;
    let mut i = 0;
    while i < 16 {
        ok &= slice[i].wrapping_sub(b'a') < 26;
        i += 1;
    }
    ok
}

/// Precomputed stretch elasticity and inverse stretch lookup tables for up to 63 spaces.
/// Eliminates floating-point divisions from the Knuth-Plass inner loop.
pub static STRETCH_TABLE: [f32; 64] = {
    let mut table = [0.0f32; 64];
    table[0] = 5.0;
    let mut s = 1;
    while s < 64 {
        table[s] = s as f32 * 1.67;
        s += 1;
    }
    table
};

pub static INV_STRETCH_TABLE: [f32; 64] = {
    let mut table = [0.0f32; 64];
    table[0] = 0.2;
    let mut s = 1;
    while s < 64 {
        table[s] = 1.0 / (s as f32 * 1.67);
        s += 1;
    }
    table
};

pub static SHRINK_TABLE: [f32; 64] = {
    let mut table = [0.0f32; 64];
    table[0] = 1.0 / 0.3;
    let mut s = 1;
    while s < 64 {
        table[s] = s as f32 * 1.11;
        s += 1;
    }
    table
};

pub static INV_SHRINK_TABLE: [f32; 64] = {
    let mut table = [0.0f32; 64];
    table[0] = 0.3;
    let mut s = 1;
    while s < 64 {
        table[s] = 1.0 / (s as f32 * 1.11);
        s += 1;
    }
    table
};

/// Power-of-two rolling ring buffer capacity for active Knuth-Plass dynamic programming state.
/// Fits entirely inside 7 KB of L1 data cache and eliminates 100% of bounds-check branches via bit masking.
pub const DP_RING_CAP: usize = 256;
pub const DP_RING_MASK: usize = DP_RING_CAP - 1;
pub const DP_RING_NODES: usize = DP_RING_CAP * 4; // 1024 slots across 4 fitness classes

/// Ultra-compact 4-byte microtypography configuration.
/// Fits entirely in a single 32-bit CPU register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct HzConfig {
    /// Maximum allowed glyph expansion in per-mille (default: 15 = +1.5%).
    pub max_expansion_permille: i8,
    /// Maximum allowed glyph compression in per-mille (default: -15 = -1.5%).
    pub min_expansion_permille: i8,
    /// Expansion threshold in tenths of a point (default: 20 = 2.0 pt).
    pub expansion_threshold_tenth: u8,
    /// Bitmask flags: bit 0 = enable_protrusion, bit 1 = use_optimal_dp.
    pub flags: u8,
}

impl HzConfig {
    pub const FLAG_PROTRUSION: u8 = 0x01;
    pub const FLAG_OPTIMAL_DP: u8 = 0x02;

    #[inline(always)]
    pub fn new(
        max_expansion: i8,
        min_expansion: i8,
        enable_protrusion: bool,
        use_optimal_dp: bool,
    ) -> Self {
        let mut flags = 0u8;
        if enable_protrusion {
            flags |= Self::FLAG_PROTRUSION;
        }
        if use_optimal_dp {
            flags |= Self::FLAG_OPTIMAL_DP;
        }
        Self {
            max_expansion_permille: max_expansion.clamp(0, 30),
            min_expansion_permille: min_expansion.clamp(-30, 0),
            expansion_threshold_tenth: 20, // 2.0 pt
            flags,
        }
    }

    #[inline(always)]
    pub fn enable_protrusion(&self) -> bool {
        (self.flags & Self::FLAG_PROTRUSION) != 0
    }

    #[inline(always)]
    pub fn use_optimal_dp(&self) -> bool {
        (self.flags & Self::FLAG_OPTIMAL_DP) != 0
    }

    #[inline(always)]
    pub fn expansion_threshold_pt(&self) -> f32 {
        self.expansion_threshold_tenth as f32 * 0.1
    }
}

impl Default for HzConfig {
    fn default() -> Self {
        Self::new(MAX_EXPANSION_PERMILLE, MIN_EXPANSION_PERMILLE, true, false)
    }
}

/// Bit-packed 16-byte line descriptor resulting from `hz`-algorithm line breaking.
///
/// Designed for exact 64-byte L1 cache-line symmetry:
/// - Exactly **4 descriptors fit into one 64-byte CPU cache line** ($64 / 16 = 4$).
/// - Zero padding bytes.
///
/// ### Memory Layout (16 Bytes Total)
/// - `line_w`: `f32` (4 bytes, offset 0..4)
/// - `line_indent`: `f32` (4 bytes, offset 4..8)
/// - `start_word`: `u16` (2 bytes, offset 8..10)
/// - `word_count`: `u16` (2 bytes, offset 10..12)
/// - `num_spaces`: `u16` (2 bytes, offset 12..14)
/// - `expansion_permille`: `i8` (1 byte, offset 14..15, range $-15 \dots +15$)
/// - `packed_flags`: `u8` (1 byte, offset 15..16):
///   - Bit 7: `justify` (1 = justified, 0 = ragged)
///   - Bits 0..6: `protrusion_tenths` ($0 \dots 127$ tenths of a point, $0.0 \dots 12.7\text{ pt}$)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct HzLineDescriptor {
    pub line_w: f32,
    pub line_indent: f32,
    pub start_word: u16,
    pub word_count: u16,
    pub num_spaces: u16,
    pub expansion_permille: i8,
    pub packed_flags: u8,
}

impl HzLineDescriptor {
    pub const JUSTIFY_BIT: u8 = 0x80;
    pub const PROTRUSION_MASK: u8 = 0x7F;

    /// Constructs a bit-packed 16-byte line descriptor.
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        start_word: u16,
        word_count: u16,
        num_spaces: u16,
        justify: bool,
        expansion_permille: i8,
        line_w: f32,
        line_indent: f32,
        protrusion_w: f32,
    ) -> Self {
        let justify_bit = if justify { Self::JUSTIFY_BIT } else { 0 };
        let prot_tenths = ((protrusion_w * 10.0).round() as u8).min(Self::PROTRUSION_MASK);
        Self {
            line_w,
            line_indent,
            start_word,
            word_count,
            num_spaces,
            expansion_permille,
            packed_flags: justify_bit | prot_tenths,
        }
    }

    /// Whether this line should be justified.
    #[inline(always)]
    pub fn justify(&self) -> bool {
        (self.packed_flags & Self::JUSTIFY_BIT) != 0
    }

    /// Optical protrusion hanging past the right margin (in points).
    #[inline(always)]
    pub fn protrusion_w(&self) -> f32 {
        (self.packed_flags & Self::PROTRUSION_MASK) as f32 * 0.1
    }

    /// Computes the PDF `Tz` horizontal text scaling operand (100.0 = normal 100%).
    #[inline(always)]
    pub fn pdf_tz_operand(&self) -> f32 {
        100.0 + (self.expansion_permille as f32) * 0.1
    }
}

/// Maximum number of line descriptors cached in a single paragraph cache slot.
pub const PAR_CACHE_MAX_LINES: usize = 32;

/// A single entry in the direct-mapped paragraph content-hash cache.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ParCacheEntry {
    /// 64-bit content and layout parameter hash.
    pub hash: u64,
    /// Number of valid line descriptors in `descriptors`.
    pub line_count: u16,
    /// Line descriptors for the cached paragraph.
    pub descriptors: [HzLineDescriptor; PAR_CACHE_MAX_LINES],
}

/// Fast 64-bit hash combining helper using Wyhash / FxHash prime multiplier.
#[inline(always)]
pub fn hash_combine(mut h: u64, val: u64) -> u64 {
    h ^= val;
    h.wrapping_mul(0x517cc1b727220a95)
}

/// Computes an ultra-fast 64-bit content hash of a paragraph and its layout environment.
pub fn hash_paragraph(
    soa: &ParagraphSoA,
    measure: f32,
    first_line_indent: f32,
    custom_hang: f32,
    hang_after: i32,
    par_shape: &[(f32, f32)],
    tolerance: i32,
    pre_tolerance: i32,
    emergency_stretch: f32,
    looseness: i32,
    par_fill_width: f32,
    par_fill_stretch: f32,
    base_font_size: f32,
    font_family: FontFamily,
    math_count: usize,
) -> u64 {
    let n = soa.len();
    let mut h = (n as u64).wrapping_mul(0x517cc1b727220a95);
    for idx in 0..n {
        let w_bits = soa.widths[idx].to_bits() as u64;
        let attr_bits = soa.attrs[idx].0 as u64;
        h = hash_combine(h, (w_bits << 16) ^ attr_bits);
        let b = soa.texts[idx].as_bytes();
        let sample = if b.len() >= 4 {
            let b0 = b[0] as u64;
            let b1 = b[1] as u64;
            let b_mid = b[b.len() / 2] as u64;
            let b_end = b[b.len() - 1] as u64;
            (b0 << 24) | (b1 << 16) | (b_mid << 8) | b_end
        } else if !b.is_empty() {
            b[0] as u64
        } else {
            0
        };
        h = hash_combine(h, (sample << 8) | (b.len() as u64));
    }
    h = hash_combine(h, measure.to_bits() as u64);
    h = hash_combine(h, first_line_indent.to_bits() as u64);
    h = hash_combine(h, custom_hang.to_bits() as u64);
    h = hash_combine(h, hang_after as u64);
    h = hash_combine(h, looseness as u64);
    h = hash_combine(h, tolerance as u64);
    h = hash_combine(h, pre_tolerance as u64);
    h = hash_combine(h, emergency_stretch.to_bits() as u64);
    h = hash_combine(h, par_fill_width.to_bits() as u64);
    h = hash_combine(h, par_fill_stretch.to_bits() as u64);
    h = hash_combine(h, base_font_size.to_bits() as u64);
    h = hash_combine(h, font_family as u64);
    h = hash_combine(h, math_count as u64);
    h = hash_combine(h, par_shape.len() as u64);
    for &(indent, len) in par_shape.iter() {
        h = hash_combine(h, ((indent.to_bits() as u64) << 32) | (len.to_bits() as u64));
    }
    h
}

/// Compact 4-byte candidate line breakpoint for dynamic programming.
/// Fits 16 breakpoints per 64-byte L1 CPU cache line.
///
/// - `word_idx`: index of the word in `ParagraphSoA`
/// - `split_idx`: `255` = whole word boundary; `0..11` = syllable hyphenation break index
/// - `kind`: `WordKind` discriminant (`0..7`)
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Breakpoint {
    pub word_idx: u16,
    pub split_idx: u8,
    pub kind: u8,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<Breakpoint>() == 4);
const _: () = assert!(core::mem::align_of::<Breakpoint>() == 2);

/// Branchless $O(1)$ lookup of the right-margin optical protrusion factor.
/// Returns a fraction between `0.0` (no protrusion) and `1.0` (full width).
#[inline(always)]
pub fn right_protrusion_factor(text: &str) -> f32 {
    let bytes = text.as_bytes();
    if let Some(&last_byte) = bytes.last() {
        if last_byte < 128 {
            return PROTRUSION_ASCII_LUT[last_byte as usize] as f32 * 0.01;
        }
        protrusion_unicode_cold(text)
    } else {
        0.0
    }
}

#[cold]
fn protrusion_unicode_cold(text: &str) -> f32 {
    if text.ends_with('–') || text.ends_with('—') {
        0.75
    } else if text.ends_with('’') || text.ends_with('”') {
        0.40
    } else if text.ends_with('…') {
        0.50
    } else {
        0.0
    }
}

/// Calculates optical margin protrusion in points for a word.
#[inline(always)]
pub fn calculate_protrusion_pt(word: &str, face: Face, size: f32) -> f32 {
    let bytes = word.as_bytes();
    if let Some(&last_byte) = bytes.last() {
        if last_byte < 128 {
            let prot_pct = PROTRUSION_ASCII_LUT[last_byte as usize];
            if prot_pct == 0 {
                return 0.0;
            }
            let char_w =
                crate::font_metrics::char_units(last_byte as char, face) as f32 * (size * 0.001);
            return (prot_pct as f32 * 0.01) * char_w;
        }
        let factor = protrusion_unicode_cold(word);
        if factor > 0.0 {
            let last_char = word.chars().next_back().unwrap_or(' ');
            let char_w = crate::font_metrics::width(last_char, face, size);
            return factor * char_w;
        }
    }
    0.0
}

/// Computes combined badness under the `hz`-microtypography model:
///
/// $$\text{Badness} = 100 \cdot |r_s|^3 + 25 \cdot \left(\frac{e}{15}\right)^2$$
#[inline(always)]
pub fn hz_line_badness(space_ratio: f32, expansion_permille: i8) -> f32 {
    let abs_r = space_ratio.abs();
    let space_badness = 100.0 * (abs_r * abs_r * abs_r);
    let exp_norm = (expansion_permille.abs() as f32) * (1.0 / 15.0);
    let exp_badness = 25.0 * (exp_norm * exp_norm);
    space_badness + exp_badness
}

/// Canonical TeX fitness classes (TeX by Topic Ch 19.1.1, TeX: The Program §817):
/// - Class 0 (`FITNESS_VERY_LOOSE`): line stretched with badness >= 100 ($r > 1.0$)
/// - Class 1 (`FITNESS_LOOSE`): line stretched with badness 13..99 ($0.5 \le r \le 1.0$)
/// - Class 2 (`FITNESS_DECENT`): line badness <= 12 ($-0.5 \le r \le 0.5$)
/// - Class 3 (`FITNESS_TIGHT`): line shrunk with badness >= 13 ($r \le -0.5$)
pub const FITNESS_VERY_LOOSE: u8 = 0;
pub const FITNESS_LOOSE: u8 = 1;
pub const FITNESS_DECENT: u8 = 2;
pub const FITNESS_TIGHT: u8 = 3;

/// Default Plain TeX line penalty added to every line break (TeX by Topic Ch 19).
pub const LINE_PENALTY: f32 = 10.0;

/// Default Plain TeX demerit penalty for adjacent visually incompatible lines (\adjdemerits).
/// Incurred whenever |fitness(line_i) - fitness(line_{i-1})| > 1.
pub const ADJ_DEMERITS: f32 = 10000.0;

/// Precomputed 4x4 adjacency demerits table for adjacent lines with incompatible fitness classes.
pub const ADJ_TABLE: [[f32; 4]; 4] = [
    [0.0, 0.0, ADJ_DEMERITS, ADJ_DEMERITS],
    [0.0, 0.0, 0.0, ADJ_DEMERITS],
    [ADJ_DEMERITS, 0.0, 0.0, 0.0],
    [ADJ_DEMERITS, ADJ_DEMERITS, 0.0, 0.0],
];

/// 8-byte unified active DP ring slot. Four fitness classes of a breakpoint occupy exactly 32 bytes (half an L1 cache line).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DpRingSlot {
    pub cost: f32,
    pub line_idx: u16,
    pub flags: u8,
    pub _pad: u8,
}

impl DpRingSlot {
    pub const EMPTY: Self = Self {
        cost: f32::INFINITY,
        line_idx: 0,
        flags: 0,
        _pad: 0,
    };
}

/// Default Plain TeX demerit penalty for consecutive lines ending in a hyphen (\doublehyphendemerits).
pub const DOUBLE_HYPHEN_DEMERITS: f32 = 10000.0;

/// Default Plain TeX demerit penalty when the penultimate line ends with a hyphen (\finalhyphendemerits).
pub const FINAL_HYPHEN_DEMERITS: f32 = 5000.0;

/// Default Plain TeX penalty for breaking at a hyphen or discretionary break (\hyphenpenalty).
pub const HYPHEN_PENALTY: f32 = 50.0;

/// Default Plain TeX penalty for breaking after an explicit hyphen (\exhyphenpenalty).
pub const EXHYPHEN_PENALTY: f32 = 50.0;

/// Classifies a line into one of TeX's 4 canonical fitness classes (TeX by Topic Ch 19.1.1):
/// - Class 0 (Very Loose): line stretched with badness >= 100 (r > 1.0)
/// - Class 1 (Loose): line stretched with badness 13..99 (0.5 <= r <= 1.0)
/// - Class 2 (Decent): line badness <= 12 (-0.5 <= r <= 0.5)
/// - Class 3 (Tight): line shrunk with badness >= 13 (r <= -0.5)
#[inline(always)]
pub fn classify_fitness(slack: f32, badness: f32) -> u8 {
    if slack < 0.0 {
        if badness >= 13.0 {
            FITNESS_TIGHT
        } else {
            FITNESS_DECENT
        }
    } else if badness >= 100.0 {
        FITNESS_VERY_LOOSE
    } else if badness >= 13.0 {
        FITNESS_LOOSE
    } else {
        FITNESS_DECENT
    }
}

/// Computes line demerits according to TeX's exact quadratic formula (TeX by Topic Ch 19.1.3):
///
/// $$d = \begin{cases} (l + b)^2 + p^2 & \text{if } 0 \le p < 10000 \\ (l + b)^2 - p^2 & \text{if } -10000 < p < 0 \\ (l + b)^2 & \text{if } p \le -10000 \end{cases}$$
#[inline(always)]
pub fn calculate_line_demerits(line_penalty: f32, badness: f32, penalty: f32) -> f32 {
    let lb = line_penalty + badness.min(10000.0);
    let base = lb * lb;
    if penalty == 0.0 {
        base
    } else if penalty > 0.0 {
        base + penalty * penalty
    } else if penalty > -10000.0 {
        base - penalty * penalty
    } else {
        base
    }
}

/// Checks whether a word ends with a hyphen character or soft hyphen.
#[inline(always)]
pub fn word_ends_with_hyphen(text: &str) -> bool {
    let bytes = text.as_bytes();
    if let Some(&last) = bytes.last() {
        last == b'-' || text.ends_with('\u{2010}') || text.ends_with('\u{00AD}')
    } else {
        false
    }
}

/// Fast-path shortcut for paragraphs that naturally fit within a single line.
///
/// Bypasses candidate breakpoint building, hyphenation lookups, prefix sum calculation,
/// and dynamic programming passes entirely when a paragraph fits within the initial measure
/// without complex shaping constraints.
#[inline(always)]
pub fn try_layout_single_line_paragraph<'a>(
    soa: &ParagraphSoA<'a>,
    measure: f32,
    first_line_indent: f32,
    par_fill_width: f32,
    par_fill_stretch: f32,
    par_shape: &[(f32, f32)],
    looseness: i32,
    out_lines: &mut Vec<HzLineDescriptor>,
) -> bool {
    if !par_shape.is_empty() || looseness != 0 {
        return false;
    }
    let n = soa.len();
    if n == 0 {
        return true;
    }

    let mut eff_start = 0;
    while eff_start < n && soa.attrs[eff_start].is_space() {
        eff_start += 1;
    }
    let mut eff_end = n;
    while eff_end > eff_start && soa.attrs[eff_end - 1].is_space() {
        eff_end -= 1;
    }
    if eff_start >= eff_end {
        return true;
    }

    let avail = measure - first_line_indent;
    if avail <= 0.0 {
        return false;
    }

    let mut line_w = 0.0f32;
    let mut num_spaces = 0u16;

    for idx in eff_start..eff_end {
        let kind = soa.attrs[idx].kind();
        if kind == WordKind::Break {
            return false;
        }
        line_w += soa.widths[idx];
        if kind == WordKind::Space {
            num_spaces += 1;
        }
    }

    if line_w + par_fill_width <= avail {
        let justify = par_fill_stretch == 0.0;
        out_lines.push(HzLineDescriptor::new(
            eff_start as u16,
            (eff_end - eff_start) as u16,
            num_spaces,
            justify,
            0,
            line_w,
            first_line_indent,
            0.0,
        ));
        true
    } else {
        false
    }
}

/// Main entry point: breaks a paragraph into lines using the `hz`-algorithm.
/// Main entry point: breaks a paragraph into lines using the `hz`-algorithm with parshape support.
/// Automatically selects between the ultra-fast linear pass or full dynamic programming.
pub fn hz_break_paragraph_shape<'a>(
    soa: &ParagraphSoA<'a>,
    measure: f32,
    first_line_indent: f32,
    custom_hang_indent: f32,
    hang_after: i32,
    par_shape: &[(f32, f32)],
    config: &HzConfig,
    out_lines: &mut Vec<HzLineDescriptor>,
) {
    if config.use_optimal_dp() {
        hz_break_paragraph_optimal_shape(
            soa,
            measure,
            first_line_indent,
            custom_hang_indent,
            hang_after,
            par_shape,
            config,
            out_lines,
        );
    } else {
        hz_break_paragraph_fast_shape(
            soa,
            measure,
            first_line_indent,
            custom_hang_indent,
            hang_after,
            par_shape,
            config,
            out_lines,
        );
    }
}

/// Main entry point: breaks a paragraph into lines using the `hz`-algorithm.
/// Automatically selects between the ultra-fast linear pass or full dynamic programming.
pub fn hz_break_paragraph<'a>(
    soa: &ParagraphSoA<'a>,
    measure: f32,
    first_line_indent: f32,
    custom_hang_indent: f32,
    hang_after: i32,
    config: &HzConfig,
    out_lines: &mut Vec<HzLineDescriptor>,
) {
    hz_break_paragraph_shape(
        soa,
        measure,
        first_line_indent,
        custom_hang_indent,
        hang_after,
        &[],
        config,
        out_lines,
    );
}

/// High-speed $O(N)$ single-pass `hz` line breaker with parshape support.
pub fn hz_break_paragraph_fast_shape<'a>(
    soa: &ParagraphSoA<'a>,
    measure: f32,
    first_line_indent: f32,
    custom_hang_indent: f32,
    hang_after: i32,
    par_shape: &[(f32, f32)],
    config: &HzConfig,
    out_lines: &mut Vec<HzLineDescriptor>,
) {
    let num_words = soa.len();
    if num_words == 0 {
        return;
    }

    let mut word_idx = 0;
    let mut line_start: u32 = 0;
    let mut line_w: f32 = 0.0;
    let mut num_spaces: u32 = 0;
    let mut line_index: usize = 0;

    let (mut line_indent, mut available) = calculate_line_geometry(
        0,
        measure,
        first_line_indent,
        custom_hang_indent,
        hang_after,
        par_shape,
    );

    while word_idx < num_words {
        let attr = soa.attrs[word_idx];
        let k = attr.kind();
        let w = soa.widths[word_idx];

        if k == WordKind::Break {
            let (trimmed_end, trimmed_w, trimmed_sp) =
                trim_trailing_spaces(soa, line_start, word_idx as u32, line_w, num_spaces);
            out_lines.push(HzLineDescriptor::new(
                line_start as u16,
                (trimmed_end - line_start) as u16,
                trimmed_sp as u16,
                false,
                0,
                trimmed_w,
                line_indent,
                0.0,
            ));
            line_index += 1;
            let (new_indent, new_avail) = calculate_line_geometry(
                line_index,
                measure,
                first_line_indent,
                custom_hang_indent,
                hang_after,
                par_shape,
            );
            line_indent = new_indent;
            available = new_avail;
            word_idx += 1;
            line_start = word_idx as u32;
            line_w = 0.0;
            num_spaces = 0;
            continue;
        }

        if line_start < word_idx as u32 && !attr.is_space() && line_w + w > available {
            let current_protrusion = if config.enable_protrusion() {
                calculate_protrusion_pt(soa.texts[word_idx], attr.face(), 10.0)
            } else {
                0.0
            };

            let effective_available = available + current_protrusion;

            if line_w + w > effective_available {
                let slack = effective_available - (line_w + w);
                let can_fit_with_compression = if slack < 0.0 && config.min_expansion_permille < 0 {
                    let natural_text_w = (line_w + w) - (num_spaces as f32) * 2.5;
                    if natural_text_w > 0.0 {
                        let required_permille = ((slack / natural_text_w) * 1000.0).round() as i8;
                        required_permille >= config.min_expansion_permille
                    } else {
                        false
                    }
                } else {
                    false
                };

                if can_fit_with_compression {
                    let natural_text_w = (line_w + w) - (num_spaces as f32) * 2.5;
                    let required_permille =
                        (((effective_available - (line_w + w)) / natural_text_w) * 1000.0)
                            .clamp(config.min_expansion_permille as f32, 0.0)
                            as i8;

                    line_w += w;
                    word_idx += 1;

                    let (trimmed_end, trimmed_w, trimmed_sp) =
                        trim_trailing_spaces(soa, line_start, word_idx as u32, line_w, num_spaces);

                    out_lines.push(HzLineDescriptor::new(
                        line_start as u16,
                        (trimmed_end - line_start) as u16,
                        trimmed_sp as u16,
                        true,
                        required_permille,
                        trimmed_w,
                        line_indent,
                        current_protrusion,
                    ));

                    line_index += 1;
                    let (new_indent, new_avail) = calculate_line_geometry(
                        line_index,
                        measure,
                        first_line_indent,
                        custom_hang_indent,
                        hang_after,
                        par_shape,
                    );
                    line_indent = new_indent;
                    available = new_avail;
                    line_start = word_idx as u32;
                    line_w = 0.0;
                    num_spaces = 0;
                    continue;
                } else {
                    let last_word_idx = word_idx - 1;
                    let trail_protrusion = if config.enable_protrusion() {
                        calculate_protrusion_pt(
                            soa.texts[last_word_idx],
                            soa.attrs[last_word_idx].face(),
                            10.0,
                        )
                    } else {
                        0.0
                    };

                    let (trimmed_end, trimmed_w, trimmed_sp) =
                        trim_trailing_spaces(soa, line_start, word_idx as u32, line_w, num_spaces);

                    let line_slack = (available + trail_protrusion) - trimmed_w;
                    let expansion = if line_slack > config.expansion_threshold_pt()
                        && trimmed_sp > 0
                        && config.max_expansion_permille > 0
                    {
                        let text_w = trimmed_w.max(1.0);
                        let exp = ((line_slack * 0.25 / text_w) * 1000.0) as i8;
                        exp.clamp(0, config.max_expansion_permille)
                    } else {
                        0
                    };

                    out_lines.push(HzLineDescriptor::new(
                        line_start as u16,
                        (trimmed_end - line_start) as u16,
                        trimmed_sp as u16,
                        true,
                        expansion,
                        trimmed_w,
                        line_indent,
                        trail_protrusion,
                    ));

                    line_index += 1;
                    let (new_indent, new_avail) = calculate_line_geometry(
                        line_index,
                        measure,
                        first_line_indent,
                        custom_hang_indent,
                        hang_after,
                        par_shape,
                    );
                    line_indent = new_indent;
                    available = new_avail;

                    if k == WordKind::Space {
                        word_idx += 1;
                    }
                    line_start = word_idx as u32;
                    line_w = 0.0;
                    num_spaces = 0;
                    continue;
                }
            }
        }

        if line_start == word_idx as u32 && attr.is_space() {
            word_idx += 1;
            line_start = word_idx as u32;
            continue;
        }

        line_w += w;
        if k == WordKind::Space {
            num_spaces += 1;
        }
        word_idx += 1;
    }

    if line_start < num_words as u32 {
        let (trimmed_end, trimmed_w, trimmed_sp) =
            trim_trailing_spaces(soa, line_start, num_words as u32, line_w, num_spaces);
        if trimmed_end > line_start {
            let last_idx = (trimmed_end - 1) as usize;
            let trail_protrusion = if config.enable_protrusion() {
                calculate_protrusion_pt(soa.texts[last_idx], soa.attrs[last_idx].face(), 10.0)
            } else {
                0.0
            };

            out_lines.push(HzLineDescriptor::new(
                line_start as u16,
                (trimmed_end - line_start) as u16,
                trimmed_sp as u16,
                false,
                0,
                trimmed_w,
                line_indent,
                trail_protrusion,
            ));
        }
    }
}

/// High-speed $O(N)$ single-pass `hz` line breaker.
/// Evaluates lines sequentially while using microtypographic font compression ($\le -1.5\%$)
/// and punctuation protrusion to prevent ugly one-word overflows.
pub fn hz_break_paragraph_fast<'a>(
    soa: &ParagraphSoA<'a>,
    measure: f32,
    first_line_indent: f32,
    custom_hang_indent: f32,
    hang_after: i32,
    config: &HzConfig,
    out_lines: &mut Vec<HzLineDescriptor>,
) {
    hz_break_paragraph_fast_shape(
        soa,
        measure,
        first_line_indent,
        custom_hang_indent,
        hang_after,
        &[],
        config,
        out_lines,
    );
}

/// Full dynamic-programming global shortest-path optimizer.
/// Solves for globally optimal line breaks using the combined `hz` microtypography model
/// and TeX's exact 4-class fitness demerit invariants (TeX by Topic Ch 19).
pub fn hz_break_paragraph_optimal<'a>(
    soa: &ParagraphSoA<'a>,
    measure: f32,
    first_line_indent: f32,
    custom_hang_indent: f32,
    hang_after: i32,
    config: &HzConfig,
    out_lines: &mut Vec<HzLineDescriptor>,
) {
    hz_break_paragraph_optimal_shape(
        soa,
        measure,
        first_line_indent,
        custom_hang_indent,
        hang_after,
        &[],
        config,
        out_lines,
    );
}

/// Single-pass dynamic-programming global shortest-path optimizer with parshape support.
/// Evaluates candidate line breaks under given `allow_hyphenation`, `max_badness`, and `emergency_stretch`.
/// Returns `true` if a feasible path reaches the end of the paragraph.
pub fn hz_break_paragraph_optimal_pass<'a>(
    soa: &ParagraphSoA<'a>,
    measure: f32,
    first_line_indent: f32,
    custom_hang_indent: f32,
    hang_after: i32,
    par_shape: &[(f32, f32)],
    config: &HzConfig,
    allow_hyphenation: bool,
    pre_tolerance: f32,
    max_badness: f32,
    emergency_stretch: f32,
    looseness: i32,
    par_fill_width: f32,
    par_fill_stretch: f32,
    nodes_buf: &mut Vec<DpNode>,
    prefix_buf: &mut Vec<PrefixSum>,
    out_lines: &mut Vec<HzLineDescriptor>,
) -> bool {
    let num_words = soa.len();
    if num_words == 0 {
        return true;
    }

    let required_nodes = (num_words + 1) * 4;
    if nodes_buf.len() < required_nodes {
        nodes_buf.resize(required_nodes, DpNode::empty());
    }
    let nodes = &mut nodes_buf[..required_nodes];
    nodes[0] = DpNode::empty();
    nodes[1] = DpNode::empty();
    nodes[2] = DpNode::empty();
    nodes[3] = DpNode::empty();
    nodes[FITNESS_DECENT as usize].cost = 0.0;

    prefix_buf.clear();
    prefix_buf.reserve(num_words + 1);
    prefix_buf.push(PrefixSum {
        w: 0.0,
        sp: 0,
        prot: 0,
        _pad: 0,
    });

    for idx in 0..num_words {
        let w = soa.widths[idx];
        let attr = soa.attrs[idx];
        let k = attr.kind();
        let sp_inc = if k == WordKind::Space { 1 } else { 0 };
        let prot = if k == WordKind::Hyphen {
            let hyph_extra_w = crate::font_metrics::char_units('-', attr.face()) as f32 * 0.01;
            ((hyph_extra_w * 0.75 * 10.0).round() as u8).min(127)
        } else if config.enable_protrusion() && !attr.is_space() {
            let text = soa.texts[idx];
            let last_b = text.as_bytes().last().copied().unwrap_or(0);
            if last_b < 128 && PROTRUSION_ASCII_LUT[last_b as usize] == 0 {
                0
            } else {
                ((calculate_protrusion_pt(text, attr.face(), 10.0) * 10.0).round() as u8).min(127)
            }
        } else {
            0
        };
        let last = prefix_buf[idx];
        prefix_buf.push(PrefixSum {
            w: last.w + w,
            sp: last.sp + sp_inc,
            prot,
            _pad: 0,
        });
    }

    let nodes = &mut nodes_buf[..required_nodes];
    let prefix = &prefix_buf[..=num_words];

    // Paragraph start: TeX acts as if preceded by a Decent line (Class 2, TeX by Topic 19.1.3)
    nodes[0 * 4 + (FITNESS_DECENT as usize)].cost = 0.0;

    let has_break = soa.attrs.iter().any(|a| a.kind() == WordKind::Break);
    let is_uniform_measure = par_shape.is_empty() && custom_hang_indent == 0.0;
    let mut j_cursor = 1;

    let default_face = if soa.attrs.is_empty() { Face::Roman } else { soa.attrs[0].face() };
    let base_hyph_w = crate::font_metrics::char_units('-', default_face) as f32 * 0.01;

    let max_r = if max_badness <= 100.0 {
        1.0000001f32
    } else if max_badness <= 200.0 {
        1.2599211f32
    } else if max_badness <= 500.0 {
        1.709976f32
    } else if max_badness >= 9999.0 {
        f32::INFINITY
    } else {
        (max_badness * 0.01).cbrt() + 1e-6
    };
    let max_slack_allowable = if max_badness <= 100.0 {
        48.0
    } else if max_badness <= 200.0 {
        58.0
    } else if max_badness <= 500.0 {
        68.0
    } else {
        120.0
    };
    let slack_mult = if max_badness <= 100.0 {
        1.7
    } else if max_badness <= 200.0 {
        2.2
    } else if max_badness <= 500.0 {
        2.9
    } else {
        12.0
    };

    for i in 0..num_words {
        let base_node_idx = i * 4;
        let mut valid_pred_count = 0;
        let mut valid_preds = [(0u8, DpNode::empty()); 4];
        let mut curr_line_idx = 0;
        for c in 0..4 {
            let n = nodes[base_node_idx + c];
            if n.cost < f32::INFINITY {
                if valid_pred_count == 0 {
                    curr_line_idx = n.line_idx as usize;
                }
                valid_preds[valid_pred_count] = (c as u8, n);
                valid_pred_count += 1;
            }
        }
        if valid_pred_count == 0 {
            continue;
        }

        let mut folded_cost = [f32::INFINITY; 4];
        let mut best_c_prev = [0u8; 4];
        for t in 0..4 {
            let mut min_c = f32::INFINITY;
            let mut min_p = 0u8;
            for p_idx in 0..valid_pred_count {
                let (c_prev, pred_node) = valid_preds[p_idx];
                let adj = ADJ_TABLE[t][c_prev as usize];
                let cost = pred_node.cost + adj;
                if cost < min_c {
                    min_c = cost;
                    min_p = c_prev;
                }
            }
            folded_cost[t] = min_c;
            best_c_prev[t] = min_p;
        }
        let prev_ends_hyphen = (nodes[base_node_idx + valid_preds[0].0 as usize].flags & DpNode::FLAG_ENDS_HYPHEN) != 0;

        let (_indent, avail) = calculate_line_geometry(
            curr_line_idx,
            measure,
            first_line_indent,
            custom_hang_indent,
            hang_after,
            par_shape,
        );

        let base_w = prefix[i].w;
        let base_sp = prefix[i].sp;

        let start_j = if has_break {
            i + 1
        } else {
            let min_target = base_w + (avail - max_slack_allowable - emergency_stretch).max(0.0);
            if is_uniform_measure {
                while j_cursor < num_words && prefix[j_cursor].w < min_target {
                    j_cursor += 1;
                }
                j_cursor.max(i + 1)
            } else {
                let offset = prefix[i..num_words].partition_point(|p| p.w < min_target);
                (i + offset).max(i + 1)
            }
        };

        for j in start_j..=num_words {
            let raw_w = prefix[j].w - base_w;
            if raw_w > avail + 30.0 {
                break;
            }

            let k = soa.attrs[j - 1].kind();
            if k == WordKind::Space && j < num_words {
                continue;
            }

            if has_break && k == WordKind::Break {
                // Mandatory break point: line ends at natural width
                let min_incoming = folded_cost[FITNESS_DECENT as usize];
                if min_incoming < f32::INFINITY {
                    let c_prev = best_c_prev[FITNESS_DECENT as usize];
                    let pred_node = nodes[base_node_idx + c_prev as usize];
                    let total_cost = min_incoming + LINE_PENALTY * LINE_PENALTY;
                    let target_idx = j * 4 + (FITNESS_DECENT as usize);
                    if total_cost < nodes[target_idx].cost {
                        nodes[target_idx] = DpNode {
                            cost: total_cost,
                            prev_node: (i * 4 + c_prev as usize) as u32,
                            prev_word: i as u16,
                            exp: 0,
                            prot: 0,
                            flags: 0,
                            _pad: 0,
                            line_idx: (pred_node.line_idx + 1).min(u16::MAX - 1),
                        };
                    }
                }
                break;
            }

            let is_hyphen_break = k == WordKind::Hyphen;
            let is_discretionary = is_hyphen_break && !soa.texts[j - 1].ends_with('-');
            if is_discretionary && !allow_hyphenation {
                continue;
            }

            // Only break after a word, after a hyphenation discretionary, after a breakable math atom (Bin/Rel), or at end of paragraph
            let can_break = match k {
                WordKind::Word | WordKind::Hyphen => true,
                WordKind::MathBin | WordKind::MathRel => true,
                _ => j == num_words,
            };

            if can_break {
                let hyph_extra_w = if is_hyphen_break { base_hyph_w } else { 0.0 };
                let current_w = prefix[j].w - base_w + hyph_extra_w;
                let spaces = prefix[j].sp - base_sp;
                let prot = (prefix[j].prot as f32) * 0.1;

                let target = avail + prot;
                let slack = target - current_w;

                // Active pruning: if line is drastically overfull, subsequent words will only make it worse
                if slack < -25.0 {
                    break;
                }

                // Active pruning: prune grossly underfull non-final lines
                if j < num_words {
                    let max_slack = if spaces == 0 {
                        5.0 + emergency_stretch
                    } else {
                        (spaces as f32) * slack_mult + 6.0 + emergency_stretch
                    };
                    if slack > max_slack {
                        continue;
                    }
                }

                let is_final_line = j == num_words;
                let curr_ends_hyphen = is_hyphen_break || (j > 0 && soa.attrs[j - 1].is_hyphen());

                let (d_base, clamped_exp, c_curr) = if is_final_line {
                    let target = avail + prot - par_fill_width;
                    let slack = target - current_w;
                    if par_fill_stretch == 0.0 {
                        // Zero stretch: final line must be fully justified, evaluated like intermediate lines
                        let (badness, c) = if slack >= 0.0 {
                            let (max_allowable, inv_stretch) = if emergency_stretch == 0.0 && (spaces as usize) < 64 {
                                let sp_idx = spaces as usize;
                                (max_r * STRETCH_TABLE[sp_idx], INV_STRETCH_TABLE[sp_idx])
                            } else {
                                let total_stretch = (spaces as f32) * 1.67 + emergency_stretch;
                                if total_stretch > 0.0 {
                                    (max_r * total_stretch, 1.0 / total_stretch)
                                } else {
                                    (max_r * 5.0, 0.2)
                                }
                            };
                            if slack > max_allowable {
                                continue;
                            }
                            let r = slack * inv_stretch;
                            let b = (100.0 * (r * r * r)).min(10000.0);
                            if b > max_badness {
                                continue;
                            }
                            (b, classify_fitness(slack, b))
                        } else {
                            let (max_allowable, inv_shrink) = if (spaces as usize) < 64 {
                                let sp_idx = spaces as usize;
                                (max_r * SHRINK_TABLE[sp_idx], INV_SHRINK_TABLE[sp_idx])
                            } else {
                                let total_shrink = (spaces as f32) * 1.11;
                                if total_shrink > 0.0 {
                                    (max_r * total_shrink, 1.0 / total_shrink)
                                } else {
                                    (max_r * (1.0 / 0.3), 0.3)
                                }
                            };
                            let slack_abs = slack.abs();
                            if slack_abs > max_allowable {
                                continue;
                            }
                            let r = slack_abs * inv_shrink;
                            let b = (100.0 * (r * r * r)).min(10000.0);
                            if b > max_badness {
                                continue;
                            }
                            (b, classify_fitness(slack, b))
                        };
                        let mut dem = calculate_line_demerits(LINE_PENALTY, badness, 0.0);
                        if pre_tolerance >= 0.0 && pre_tolerance < max_badness && badness > pre_tolerance {
                            dem += 10_000_000.0;
                        }
                        (dem, 0i8, c)
                    } else {
                        // Final line: \parfillskip absorbs positive slack. Spaces set at natural width unless overfull.
                        let badness = if slack < 0.0 {
                            let r = if (spaces as usize) < 64 {
                                if spaces > 0 {
                                    slack.abs() * INV_SHRINK_TABLE[spaces as usize]
                                } else {
                                    1.0
                                }
                            } else {
                                let space_shrink = (spaces as f32) * 1.11;
                                if space_shrink > 0.0 {
                                    slack.abs() / space_shrink
                                } else {
                                    1.0
                                }
                            };
                            (100.0 * (r * r * r)).min(10000.0)
                        } else {
                            0.0
                        };
                        if badness > max_badness {
                            continue;
                        }
                        let c = if slack < 0.0 && badness >= 13.0 {
                            FITNESS_TIGHT
                        } else {
                            FITNESS_DECENT
                        };
                        let mut dem = calculate_line_demerits(LINE_PENALTY, badness, 0.0);
                        if pre_tolerance >= 0.0 && pre_tolerance < max_badness && badness > pre_tolerance {
                            dem += 10_000_000.0;
                        }
                        (dem, 0i8, c)
                    }
                } else {
                    // Evaluate hz-expansion lazily
                    let (clamped_exp, space_slack) = if slack.abs() <= 2.0 {
                        (0i8, slack)
                    } else {
                        let text_w = (current_w - (spaces as f32) * 2.5).max(1.0);
                        let exp = ((slack * 0.20 / text_w) * 1000.0) as i8;
                        let c_exp =
                            exp.clamp(config.min_expansion_permille, config.max_expansion_permille);
                        (c_exp, slack - (text_w * (c_exp as f32 * 0.001)))
                    };

                    let (b, total_b, c) = if space_slack >= 0.0 {
                        let (max_allowable, inv_stretch) = if emergency_stretch == 0.0 && (spaces as usize) < 64 {
                            let sp_idx = spaces as usize;
                            (max_r * STRETCH_TABLE[sp_idx], INV_STRETCH_TABLE[sp_idx])
                        } else {
                            let total_stretch = (spaces as f32) * 1.67 + emergency_stretch;
                            if total_stretch > 0.0 {
                                (max_r * total_stretch, 1.0 / total_stretch)
                            } else {
                                (max_r * 5.0, 0.2)
                            }
                        };
                        if space_slack > max_allowable {
                            continue;
                        }

                        let r = space_slack * inv_stretch;
                        let b = (100.0 * (r * r * r)).min(10000.0);
                        if b > max_badness {
                            continue;
                        }

                        let total_b = if clamped_exp == 0 {
                            b
                        } else {
                            let exp_norm = (clamped_exp.abs() as f32) * (1.0 / 15.0);
                            (b + 25.0 * (exp_norm * exp_norm)).min(10000.0)
                        };
                        (b, total_b, classify_fitness(space_slack, total_b))
                    } else {
                        let (max_allowable, inv_shrink) = if (spaces as usize) < 64 {
                            let sp_idx = spaces as usize;
                            (max_r * SHRINK_TABLE[sp_idx], INV_SHRINK_TABLE[sp_idx])
                        } else {
                            let total_shrink = (spaces as f32) * 1.11;
                            if total_shrink > 0.0 {
                                (max_r * total_shrink, 1.0 / total_shrink)
                            } else {
                                (max_r * (1.0 / 0.3), 0.3)
                            }
                        };
                        let slack_abs = space_slack.abs();
                        if slack_abs > max_allowable {
                            continue;
                        }

                        let r = slack_abs * inv_shrink;
                        let b = (100.0 * (r * r * r)).min(10000.0);
                        if b > max_badness {
                            continue;
                        }

                        let total_b = if clamped_exp == 0 {
                            b
                        } else {
                            let exp_norm = (clamped_exp.abs() as f32) * (1.0 / 15.0);
                            (b + 25.0 * (exp_norm * exp_norm)).min(10000.0)
                        };
                        (b, total_b, classify_fitness(space_slack, total_b))
                    };

                    let pen = match k {
                        WordKind::MathBin => crate::math::InlineMathPenaltyInserter::BIN_PENALTY as f32,
                        WordKind::MathRel => crate::math::InlineMathPenaltyInserter::REL_PENALTY as f32,
                        WordKind::Hyphen => HYPHEN_PENALTY,
                        _ => {
                            if curr_ends_hyphen {
                                EXHYPHEN_PENALTY
                            } else {
                                0.0
                            }
                        }
                    };

                    let mut dem = calculate_line_demerits(LINE_PENALTY, total_b, pen);
                    if pre_tolerance >= 0.0 && pre_tolerance < max_badness && b > pre_tolerance {
                        dem += 10_000_000.0;
                    }

                    let looseness_bias = (looseness as f32) * -50000.0;
                    (dem + looseness_bias, clamped_exp, c)
                };

                let target_idx = j * 4 + (c_curr as usize);
                let c_idx = c_curr as usize;
                let min_incoming = folded_cost[c_idx];
                if min_incoming < f32::INFINITY {
                    let c_prev = best_c_prev[c_idx];
                    let pred_node = nodes[base_node_idx + c_prev as usize];

                    let mut d = d_base;
                    if curr_ends_hyphen && prev_ends_hyphen {
                        d += DOUBLE_HYPHEN_DEMERITS;
                    }
                    if is_final_line && prev_ends_hyphen {
                        d += FINAL_HYPHEN_DEMERITS;
                    }

                    let total_cost = min_incoming + d;
                    if total_cost < nodes[target_idx].cost {
                        let target_flags = if curr_ends_hyphen { DpNode::FLAG_ENDS_HYPHEN } else { 0 };
                        let prot_val = ((prot * 10.0).round() as u8).min(0x7F);
                        nodes[target_idx] = DpNode {
                            cost: total_cost,
                            prev_node: (i * 4 + c_prev as usize) as u32,
                            prev_word: i as u16,
                            exp: clamped_exp,
                            prot: prot_val,
                            flags: target_flags,
                            _pad: 0,
                            line_idx: (pred_node.line_idx + 1).min(u16::MAX - 1),
                        };
                    }
                }
            }
        }
    }

    // Find the minimum-demerit solution among all 4 fitness classes at the paragraph end
    let mut best_cost = f32::INFINITY;
    let mut best_node_idx = 0;
    for c in 0..4 {
        let cost = nodes[num_words * 4 + c].cost;
        if cost < best_cost {
            best_cost = cost;
            best_node_idx = num_words * 4 + c;
        }
    }

    if best_cost == f32::INFINITY {
        return false;
    }

    if looseness != 0 {
        let current_lines = nodes[best_node_idx].line_idx as i32;
        let target_lines = current_lines + looseness;
        let mut best_diff = (current_lines - target_lines).abs();
        for c in 0..4 {
            let idx = num_words * 4 + c;
            let cost = nodes[idx].cost;
            if cost < f32::INFINITY {
                let lines = nodes[idx].line_idx as i32;
                let diff = (lines - target_lines).abs();
                if diff < best_diff || (diff == best_diff && cost < nodes[best_node_idx].cost) {
                    best_diff = diff;
                    best_node_idx = idx;
                }
            }
        }
    }

    // Reconstruct optimal path from back to front with zero heap allocation
    let mut stack_breaks = [(0usize, 0usize, 0i8, 0u8); 64];
    let mut break_count = 0;
    let mut heap_breaks = Vec::new();
    let mut curr_node_idx = best_node_idx;

    while curr_node_idx / 4 > 0 {
        let node = nodes[curr_node_idx];
        let end = curr_node_idx / 4;
        let start = node.prev_word as usize;
        if break_count < 64 && heap_breaks.is_empty() {
            stack_breaks[break_count] = (start, end, node.exp, node.prot);
            break_count += 1;
        } else {
            if heap_breaks.is_empty() {
                heap_breaks.reserve(break_count + 32);
                heap_breaks.extend_from_slice(&stack_breaks[..break_count]);
            }
            heap_breaks.push((start, end, node.exp, node.prot));
        }
        curr_node_idx = node.prev_node as usize;
    }

    // Emit formatted line descriptors (iterating in reverse order avoids an in-place reverse pass)
    let emit_line = |out_lines: &mut Vec<HzLineDescriptor>, line_idx: usize, start: usize, end: usize, exp: i8, prot_tenths: u8| {
        let (indent, _avail) = calculate_line_geometry(
            line_idx,
            measure,
            first_line_indent,
            custom_hang_indent,
            hang_after,
            par_shape,
        );
        let mut eff_start = start;
        while eff_start < end && soa.attrs[eff_start].is_space() {
            eff_start += 1;
        }
        let mut eff_end = end;
        while eff_end > eff_start && soa.attrs[eff_end - 1].is_space() {
            eff_end -= 1;
        }
        let is_hyphen_break = eff_end > eff_start && soa.attrs[eff_end - 1].kind() == WordKind::Hyphen;
        let hyph_extra_w = if is_hyphen_break {
            crate::font_metrics::char_units('-', soa.attrs[eff_end - 1].face()) as f32 * 0.01
        } else {
            0.0
        };
        let line_w = prefix[eff_end].w - prefix[eff_start].w + hyph_extra_w;
        let num_spaces = prefix[eff_end].sp - prefix[eff_start].sp;

        let is_last = end == num_words;
        let justify = if is_last {
            par_fill_stretch == 0.0
        } else {
            true
        };
        out_lines.push(HzLineDescriptor::new(
            eff_start as u16,
            (eff_end - eff_start) as u16,
            num_spaces,
            justify,
            exp,
            line_w,
            indent,
            prot_tenths as f32 * 0.1,
        ));
    };

    if heap_breaks.is_empty() {
        for (line_idx, &(start, end, exp, prot_tenths)) in stack_breaks[..break_count].iter().rev().enumerate() {
            emit_line(out_lines, line_idx, start, end, exp, prot_tenths);
        }
    } else {
        for (line_idx, &(start, end, exp, prot_tenths)) in heap_breaks.iter().rev().enumerate() {
            emit_line(out_lines, line_idx, start, end, exp, prot_tenths);
        }
    }

    true
}

/// Three-pass dynamic programming line breaker with looseness and parshape support (TeX by Topic Ch 19.2.1 & 19.2.2).
///
/// Executes TeX's canonical three-pass line breaking strategy:
/// - **Pass 1**: Rapid trial with hyphenation disabled under `pre_tolerance` (default 100).
///   If `pre_tolerance < 0`, Pass 1 is skipped.
/// - **Pass 2**: Hyphenation enabled under `tolerance` (default 200).
/// - **Pass 3**: If Pass 2 fails and `emergency_stretch > 0.0`, retry with emergency stretch
///   added to each line's elasticity, rescuing otherwise overfull lines.
/// - **Fallback**: If all passes fail, line breaks are constructed with relaxed badness
///   or linear pass to guarantee text is never dropped.
///
/// Returns the successful pass number (`1`, `2`, `3`, or `0` for fallback).
pub fn hz_break_paragraph_three_pass<'a>(
    soa: &ParagraphSoA<'a>,
    measure: f32,
    first_line_indent: f32,
    custom_hang_indent: f32,
    hang_after: i32,
    par_shape: &[(f32, f32)],
    config: &HzConfig,
    pre_tolerance: i32,
    tolerance: i32,
    emergency_stretch: f32,
    looseness: i32,
    par_fill_width: f32,
    par_fill_stretch: f32,
    out_lines: &mut Vec<HzLineDescriptor>,
) -> u8 {
    let num_words = soa.len();
    if num_words == 0 {
        return 0;
    }

    if try_layout_single_line_paragraph(
        soa,
        measure,
        first_line_indent,
        par_fill_width,
        par_fill_stretch,
        par_shape,
        looseness,
        out_lines,
    ) {
        return 1;
    }

    let mut nodes_buf = Vec::with_capacity((num_words + 1) * 4);
    let mut prefix_buf = Vec::with_capacity(num_words + 1);

    // Pass 1: Try without hyphenation under pretolerance (if pretolerance >= 0)
    if pre_tolerance >= 0 {
        out_lines.clear();
        if hz_break_paragraph_optimal_pass(
            soa,
            measure,
            first_line_indent,
            custom_hang_indent,
            hang_after,
            par_shape,
            config,
            false, // allow_hyphenation = false
            pre_tolerance as f32,
            pre_tolerance as f32,
            0.0,
            looseness,
            par_fill_width,
            par_fill_stretch,
            &mut nodes_buf,
            &mut prefix_buf,
            out_lines,
        ) {
            return 1;
        }
    }

    // Pass 2: Try with hyphenation under tolerance
    out_lines.clear();
    if hz_break_paragraph_optimal_pass(
        soa,
        measure,
        first_line_indent,
        custom_hang_indent,
        hang_after,
        par_shape,
        config,
        true, // allow_hyphenation = true
        -1.0,
        tolerance as f32,
        0.0,
        looseness,
        par_fill_width,
        par_fill_stretch,
        &mut nodes_buf,
        &mut prefix_buf,
        out_lines,
    ) {
        return 2;
    }

    // Pass 3: If emergency_stretch > 0, retry with emergency stretch added to elasticity
    if emergency_stretch > 0.0 {
        out_lines.clear();
        if hz_break_paragraph_optimal_pass(
            soa,
            measure,
            first_line_indent,
            custom_hang_indent,
            hang_after,
            par_shape,
            config,
            true, // allow_hyphenation = true
            -1.0,
            tolerance as f32,
            emergency_stretch,
            looseness,
            par_fill_width,
            par_fill_stretch,
            &mut nodes_buf,
            &mut prefix_buf,
            out_lines,
        ) {
            return 3;
        }
    }

    // Fallback: If all passes failed, run with relaxed badness (10000.0)
    out_lines.clear();
    if hz_break_paragraph_optimal_pass(
        soa,
        measure,
        first_line_indent,
        custom_hang_indent,
        hang_after,
        par_shape,
        config,
        true,
        -1.0,
        10000.0,
        emergency_stretch,
        looseness,
        par_fill_width,
        par_fill_stretch,
        &mut nodes_buf,
        &mut prefix_buf,
        out_lines,
    ) {
        return 0;
    }

    // Ultimate fallback: fast linear shape pass
    hz_break_paragraph_fast_shape(
        soa,
        measure,
        first_line_indent,
        custom_hang_indent,
        hang_after,
        par_shape,
        config,
        out_lines,
    );
    0
}

#[inline]
fn emit_line_from_soa<'a>(
    soa: &ParagraphSoA<'a>,
    out_lines: &mut Vec<HzLineDescriptor>,
    _line_idx: usize,
    start: usize,
    end: usize,
    exp: i8,
    prot_tenths: u8,
    indent: f32,
    is_last: bool,
    par_fill_stretch: f32,
) {
    let mut eff_start = start;
    while eff_start < end && soa.attrs[eff_start].is_space() {
        eff_start += 1;
    }
    let mut eff_end = end;
    while eff_end > eff_start && soa.attrs[eff_end - 1].is_space() {
        eff_end -= 1;
    }

    let mut line_w = 0.0f32;
    let mut num_spaces = 0u16;
    for idx in eff_start..eff_end {
        line_w += soa.widths[idx];
        if soa.attrs[idx].kind() == WordKind::Space {
            num_spaces += 1;
        }
    }
    let is_hyphen_break = eff_end > eff_start && soa.attrs[eff_end - 1].kind() == WordKind::Hyphen;
    if is_hyphen_break {
        line_w += crate::font_metrics::char_units('-', soa.attrs[eff_end - 1].face()) as f32 * 0.01;
    }

    let justify = if is_last {
        par_fill_stretch == 0.0
    } else {
        true
    };

    out_lines.push(HzLineDescriptor::new(
        eff_start as u16,
        (eff_end - eff_start) as u16,
        num_spaces,
        justify,
        exp,
        line_w,
        indent,
        prot_tenths as f32 * 0.1,
    ));
}

/// Constructs a compact 4-byte candidate breakpoint array and prefix sums directly from `words_soa`.
/// Avoids materializing a secondary `ParagraphSoA` structure.
/// Skips hyphenation hash and cache lookups for words that never fall near the right margin horizon.
pub fn build_breakpoints<'a>(
    soa: &ParagraphSoA<'a>,
    font_family: FontFamily,
    base_font_size: f32,
    measure: f32,
    custom_hang_indent: f32,
    par_shape: &[(f32, f32)],
    hyphen_cache: &mut [HyphenCacheEntry; HYPHEN_CACHE_SIZE],
    hyphen_scratch: &mut Vec<&'a str>,
    config: &HzConfig,
    breakpoints: &mut Vec<Breakpoint>,
    prefix_buf: &mut Vec<PrefixSum>,
) {
    let num_words = soa.len();
    breakpoints.clear();
    breakpoints.reserve(num_words * 2);
    prefix_buf.clear();
    prefix_buf.reserve(num_words * 2 + 1);

    prefix_buf.push(PrefixSum {
        w: 0.0,
        sp: 0,
        prot: 0,
        _pad: 0,
    });

    let scale = base_font_size * 0.001;
    let default_face = if soa.attrs.is_empty() {
        Face::Roman
    } else {
        soa.attrs[0].face()
    };
    let hyph_extra_w = crate::font_metrics::char_units('-', default_face) as f32 * 0.01;
    let hyph_prot = ((hyph_extra_w * 0.75 * 10.0).round() as u8).min(127);
    let has_custom_shape = !par_shape.is_empty() || custom_hang_indent != 0.0;

    let mut pending_w = 0.0f32;
    let mut pending_sp = 0u16;
    let mut line_target = measure;

    let mut w = 0;
    while w < num_words {
        // Fast-path: SIMD/SWAR 4-word chunk discovery when safely before the line margin horizon
        if !has_custom_shape && w + 4 <= num_words {
            let a0 = soa.attrs[w].0;
            let a1 = soa.attrs[w + 1].0;
            let a2 = soa.attrs[w + 2].0;
            let a3 = soa.attrs[w + 3].0;
            let attrs_packed = u32::from_ne_bytes([a0, a1, a2, a3]);

            // (kind & 0x06) == 0 iff kind is WordKind::Word (0) or WordKind::Space (1)
            if (attrs_packed & 0x06060606) == 0 {
                let base_prefix = *prefix_buf.last().unwrap();
                let eff_base_w = base_prefix.w + pending_w;
                while eff_base_w >= line_target {
                    line_target += measure;
                }

                let chunk_w = soa.widths[w] + soa.widths[w + 1] + soa.widths[w + 2] + soa.widths[w + 3];

                // If the entire 4-item chunk ends at least 70pt before the line margin horizon,
                // no word in the chunk can be a candidate. Vectorize prefix accumulation.
                if eff_base_w + chunk_w + 70.0 < line_target {
                    let mut curr_w = base_prefix.w;
                    let mut curr_sp = base_prefix.sp;

                    for i in 0..4 {
                        let idx = w + i;
                        let attr = soa.attrs[idx];
                        let w_width = soa.widths[idx];
                        if attr.is_space() {
                            pending_w += w_width;
                            pending_sp += 1;
                        } else {
                            let eff_w = curr_w + pending_w;
                            let eff_sp = curr_sp + pending_sp;
                            pending_w = 0.0;
                            pending_sp = 0;

                            let text = soa.texts[idx];
                            let prot = if config.enable_protrusion() {
                                let last_b = text.as_bytes().last().copied().unwrap_or(0);
                                if last_b.is_ascii_alphanumeric()
                                    || (last_b < 128 && PROTRUSION_ASCII_LUT[last_b as usize] == 0)
                                {
                                    0
                                } else {
                                    ((calculate_protrusion_pt(text, attr.face(), 10.0) * 10.0)
                                        .round() as u8)
                                        .min(127)
                                }
                            } else {
                                0
                            };

                            curr_w = eff_w + w_width;
                            curr_sp = eff_sp;

                            breakpoints.push(Breakpoint {
                                word_idx: idx as u16,
                                split_idx: 255,
                                kind: WordKind::Word as u8,
                            });
                            prefix_buf.push(PrefixSum {
                                w: curr_w,
                                sp: curr_sp,
                                prot,
                                _pad: 0,
                            });
                        }
                    }

                    w += 4;
                    continue;
                }
            }
        }

        let attr = soa.attrs[w];
        let kind = attr.kind();
        let face = attr.face();
        let text = soa.texts[w];
        let w_width = soa.widths[w];

        if kind == WordKind::Space {
            pending_w += w_width;
            pending_sp += 1;
            w += 1;
            continue;
        }

        let base_prefix = *prefix_buf.last().unwrap();
        let eff_base_w = base_prefix.w + pending_w;
        let eff_base_sp = base_prefix.sp + pending_sp;
        pending_w = 0.0;
        pending_sp = 0;

        let is_candidate = if kind == WordKind::Word && text.len() >= 5 {
            if has_custom_shape {
                true
            } else {
                while eff_base_w >= line_target {
                    line_target += measure;
                }
                let dist_to_target = line_target - eff_base_w;
                dist_to_target <= w_width + 70.0 && dist_to_target >= -30.0
            }
        } else {
            false
        };

        if is_candidate {
            let fam_face = ((font_family as u8) << 4) | ((face as u8) & 0x0F);
            let word_hash = hash_word(text);
            let set_idx = (word_hash as usize) & crate::layout::layout_hyphen::HYPHEN_CACHE_SET_MASK;
            let slot0 = set_idx * 2;
            let slot1 = slot0 + 1;
            let e0 = &hyphen_cache[slot0];
            let e1 = &hyphen_cache[slot1];

            let entry = if e0.hash == word_hash
                && e0.word_len == text.len() as u16
                && e0.family_face == fam_face
            {
                *e0
            } else if e1.hash == word_hash
                && e1.word_len == text.len() as u16
                && e1.family_face == fam_face
            {
                *e1
            } else {
                let table = metrics::resolve_width_table(font_family, face);
                let flat_table = FlatAsciiWidthTable::from_table(table);
                hyphen_scratch.clear();
                crate::layout::layout_hyphen::hyphenate_word(
                    text,
                    DEFAULT_LEFT_HYPHEN_MIN,
                    DEFAULT_RIGHT_HYPHEN_MIN,
                    hyphen_scratch,
                );

                let mut new_breaks = [0u8; 12];
                let mut piece_units = [0u16; 13];
                let mut b_count = 0;
                if hyphen_scratch.len() > 1 && hyphen_scratch.len() <= 13 {
                    let mut offset = 0;
                    for (p, piece) in hyphen_scratch[..hyphen_scratch.len() - 1].iter().enumerate() {
                        offset += piece.len();
                        if offset <= 255 && b_count < 12 {
                            new_breaks[b_count] = offset as u8;
                            piece_units[p] = (flat_table.string_units_or(piece, table).min(65535)) as u16;
                            b_count += 1;
                        }
                    }
                    let last_piece = hyphen_scratch[hyphen_scratch.len() - 1];
                    piece_units[b_count] = (flat_table.string_units_or(last_piece, table).min(65535)) as u16;
                }

                let new_entry = HyphenCacheEntry {
                    hash: word_hash,
                    word_len: text.len() as u16,
                    num_breaks: b_count as u8,
                    family_face: fam_face,
                    breaks: new_breaks,
                    piece_units,
                    total_units: (w_width / scale).round().min(65535.0) as u16,
                    _pad: [0; 12],
                };
                if e0.hash == 0 {
                    hyphen_cache[slot0] = new_entry;
                } else {
                    hyphen_cache[slot1] = new_entry;
                }
                new_entry
            };

            let num_breaks = entry.num_breaks as usize;
            let mut cum_units = 0u32;
            let mut p = 0;
            while p + 4 <= num_breaks {
                let u0 = entry.piece_units[p] as u32;
                let u1 = entry.piece_units[p + 1] as u32;
                let u2 = entry.piece_units[p + 2] as u32;
                let u3 = entry.piece_units[p + 3] as u32;

                let c0 = cum_units + u0;
                let c1 = c0 + u1;
                let c2 = c1 + u2;
                let c3 = c2 + u3;
                cum_units = c3;

                let piece_w0 = (c0 as f32) * scale;
                let piece_w1 = (c1 as f32) * scale;
                let piece_w2 = (c2 as f32) * scale;
                let piece_w3 = (c3 as f32) * scale;

                breakpoints.extend_from_slice(&[
                    Breakpoint {
                        word_idx: w as u16,
                        split_idx: p as u8,
                        kind: WordKind::Hyphen as u8,
                    },
                    Breakpoint {
                        word_idx: w as u16,
                        split_idx: (p + 1) as u8,
                        kind: WordKind::Hyphen as u8,
                    },
                    Breakpoint {
                        word_idx: w as u16,
                        split_idx: (p + 2) as u8,
                        kind: WordKind::Hyphen as u8,
                    },
                    Breakpoint {
                        word_idx: w as u16,
                        split_idx: (p + 3) as u8,
                        kind: WordKind::Hyphen as u8,
                    },
                ]);
                prefix_buf.extend_from_slice(&[
                    PrefixSum {
                        w: eff_base_w + piece_w0,
                        sp: eff_base_sp,
                        prot: hyph_prot,
                        _pad: 0,
                    },
                    PrefixSum {
                        w: eff_base_w + piece_w1,
                        sp: eff_base_sp,
                        prot: hyph_prot,
                        _pad: 0,
                    },
                    PrefixSum {
                        w: eff_base_w + piece_w2,
                        sp: eff_base_sp,
                        prot: hyph_prot,
                        _pad: 0,
                    },
                    PrefixSum {
                        w: eff_base_w + piece_w3,
                        sp: eff_base_sp,
                        prot: hyph_prot,
                        _pad: 0,
                    },
                ]);
                p += 4;
            }

            while p < num_breaks {
                cum_units += entry.piece_units[p] as u32;
                let piece_w = (cum_units as f32) * scale;
                breakpoints.push(Breakpoint {
                    word_idx: w as u16,
                    split_idx: p as u8,
                    kind: WordKind::Hyphen as u8,
                });
                prefix_buf.push(PrefixSum {
                    w: eff_base_w + piece_w,
                    sp: eff_base_sp,
                    prot: hyph_prot,
                    _pad: 0,
                });
                p += 1;
            }

            breakpoints.push(Breakpoint {
                word_idx: w as u16,
                split_idx: 255,
                kind: kind as u8,
            });
            prefix_buf.push(PrefixSum {
                w: eff_base_w + w_width,
                sp: eff_base_sp,
                prot: 0,
                _pad: 0,
            });
        } else {
            let prot = if kind == WordKind::Hyphen {
                hyph_prot
            } else if config.enable_protrusion() && !attr.is_space() {
                let last_b = text.as_bytes().last().copied().unwrap_or(0);
                if last_b.is_ascii_alphanumeric()
                    || (last_b < 128 && PROTRUSION_ASCII_LUT[last_b as usize] == 0)
                {
                    0
                } else {
                    ((calculate_protrusion_pt(text, face, 10.0) * 10.0).round() as u8).min(127)
                }
            } else {
                0
            };
            breakpoints.push(Breakpoint {
                word_idx: w as u16,
                split_idx: 255,
                kind: kind as u8,
            });
            prefix_buf.push(PrefixSum {
                w: eff_base_w + w_width,
                sp: eff_base_sp,
                prot,
                _pad: 0,
            });
        }
        w += 1;
    }
}

/// Dynamic programming line breaker executing directly on compact [`Breakpoint`] candidates.
/// On successful line breaking, in-place word splitting is performed only on selected breakpoints in `soa`.
pub fn hz_break_paragraph_breakpoints_pass<'a>(
    soa: &mut ParagraphSoA<'a>,
    breakpoints: &[Breakpoint],
    prefix: &[PrefixSum],
    hyphen_cache: &[HyphenCacheEntry],
    scale: f32,
    font_family: FontFamily,
    measure: f32,
    first_line_indent: f32,
    custom_hang_indent: f32,
    hang_after: i32,
    par_shape: &[(f32, f32)],
    config: &HzConfig,
    allow_hyphenation: bool,
    pre_tolerance: f32,
    max_badness: f32,
    emergency_stretch: f32,
    looseness: i32,
    par_fill_width: f32,
    par_fill_stretch: f32,
    nodes_buf: &mut Vec<DpNode>,
    out_lines: &mut Vec<HzLineDescriptor>,
) -> bool {
    let num_bp = breakpoints.len();
    if num_bp == 0 {
        return true;
    }

    let required_nodes = (num_bp + 1) * 4;
    if nodes_buf.len() < required_nodes {
        nodes_buf.resize(required_nodes, DpNode::empty());
    }
    let nodes = &mut nodes_buf[..required_nodes];
    nodes[0] = DpNode::empty();
    nodes[1] = DpNode::empty();
    nodes[2] = DpNode::empty();
    nodes[3] = DpNode::empty();
    nodes[FITNESS_DECENT as usize].cost = 0.0;

    let mut ring = [DpRingSlot::EMPTY; DP_RING_NODES];
    ring[FITNESS_DECENT as usize] = DpRingSlot {
        cost: 0.0,
        line_idx: 0,
        flags: 0,
        _pad: 0,
    };
    let mut highest_cleared_bp: usize = 0;

    let has_break = breakpoints.iter().any(|b| b.kind == (WordKind::Break as u8));
    let is_uniform_measure = par_shape.is_empty() && custom_hang_indent == 0.0;
    let mut j_cursor = 1;

    let default_face = if soa.attrs.is_empty() {
        Face::Roman
    } else {
        soa.attrs[0].face()
    };
    let base_hyph_w = crate::font_metrics::char_units('-', default_face) as f32 * 0.01;

    let max_r = if max_badness <= 100.0 {
        1.0000001f32
    } else if max_badness <= 200.0 {
        1.2599211f32
    } else if max_badness <= 500.0 {
        1.709976f32
    } else if max_badness >= 9999.0 {
        f32::INFINITY
    } else {
        (max_badness * 0.01).cbrt() + 1e-6
    };
    let max_slack_allowable = if max_badness <= 100.0 {
        48.0
    } else if max_badness <= 200.0 {
        58.0
    } else if max_badness <= 500.0 {
        68.0
    } else {
        120.0
    };
    let slack_mult = if max_badness <= 100.0 {
        1.7
    } else if max_badness <= 200.0 {
        2.2
    } else if max_badness <= 500.0 {
        2.9
    } else {
        12.0
    };

    for i in 0..num_bp {
        let base_ring_idx = (i * 4) & (DP_RING_NODES - 1);
        let mut valid_pred_count = 0;
        let mut valid_preds = [(0u8, f32::INFINITY, 0u16); 4];
        let mut curr_line_idx = 0;
        for c in 0..4 {
            let slot = ring[base_ring_idx + c];
            if slot.cost < f32::INFINITY {
                if valid_pred_count == 0 {
                    curr_line_idx = slot.line_idx as usize;
                }
                valid_preds[valid_pred_count] = (c as u8, slot.cost, slot.line_idx);
                valid_pred_count += 1;
            }
        }
        if valid_pred_count == 0 {
            continue;
        }

        let mut folded_cost = [f32::INFINITY; 4];
        let mut best_c_prev = [0u8; 4];
        for t in 0..4 {
            let mut min_c = f32::INFINITY;
            let mut min_p = 0u8;
            for p_idx in 0..valid_pred_count {
                let (c_prev, pred_cost, _) = valid_preds[p_idx];
                let adj = ADJ_TABLE[t][c_prev as usize];
                let cost = pred_cost + adj;
                if cost < min_c {
                    min_c = cost;
                    min_p = c_prev;
                }
            }
            folded_cost[t] = min_c;
            best_c_prev[t] = min_p;
        }
        let prev_ends_hyphen = (ring[base_ring_idx + valid_preds[0].0 as usize].flags & DpNode::FLAG_ENDS_HYPHEN) != 0;

        let (_indent, avail) = calculate_line_geometry(
            curr_line_idx,
            measure,
            first_line_indent,
            custom_hang_indent,
            hang_after,
            par_shape,
        );

        let base_w = prefix[i].w;
        let base_sp = prefix[i].sp;

        let start_j = if has_break {
            i + 1
        } else {
            let min_target = base_w + (avail - max_slack_allowable - emergency_stretch).max(0.0);
            if is_uniform_measure {
                while j_cursor < num_bp && prefix[j_cursor].w < min_target {
                    j_cursor += 1;
                }
                j_cursor.max(i + 1)
            } else {
                let offset = prefix[i..num_bp].partition_point(|p| p.w < min_target);
                (i + offset).max(i + 1)
            }
        };

        for j in start_j..=num_bp {
            let raw_w = prefix[j].w - base_w;
            if raw_w > avail + 30.0 {
                break;
            }

            while highest_cleared_bp < j {
                highest_cleared_bp += 1;
                let base = (highest_cleared_bp * 4) & (DP_RING_NODES - 1);
                ring[base] = DpRingSlot::EMPTY;
                ring[base + 1] = DpRingSlot::EMPTY;
                ring[base + 2] = DpRingSlot::EMPTY;
                ring[base + 3] = DpRingSlot::EMPTY;
            }

            let bp = breakpoints[j - 1];
            let k = WordKind::from_u8(bp.kind);

            // A line cannot start and end inside the same word
            if i > 0 && breakpoints[i - 1].word_idx == bp.word_idx {
                continue;
            }

            if has_break && k == WordKind::Break {
                let min_incoming = folded_cost[FITNESS_DECENT as usize];
                if min_incoming < f32::INFINITY {
                    let c_prev = best_c_prev[FITNESS_DECENT as usize];
                    let pred_line = ring[base_ring_idx + c_prev as usize].line_idx;
                    let total_cost = min_incoming + LINE_PENALTY * LINE_PENALTY;
                    let ring_target_idx = ((j * 4) + (FITNESS_DECENT as usize)) & (DP_RING_NODES - 1);
                    if total_cost < ring[ring_target_idx].cost {
                        let new_line = (pred_line + 1).min(u16::MAX - 1);
                        ring[ring_target_idx] = DpRingSlot {
                            cost: total_cost,
                            line_idx: new_line,
                            flags: 0,
                            _pad: 0,
                        };

                        let target_idx = j * 4 + (FITNESS_DECENT as usize);
                        nodes[target_idx] = DpNode {
                            cost: total_cost,
                            prev_node: (i * 4 + c_prev as usize) as u32,
                            prev_word: i as u16,
                            exp: 0,
                            prot: 0,
                            flags: 0,
                            _pad: 0,
                            line_idx: new_line,
                        };
                    }
                }
                break;
            }

            let is_hyphen_break = bp.kind == (WordKind::Hyphen as u8);
            if bp.split_idx != 255 && !allow_hyphenation {
                continue;
            }
            let can_break = match k {
                WordKind::Word | WordKind::Hyphen | WordKind::MathBin | WordKind::MathRel => true,
                _ => j == num_bp,
            };

            if can_break {
                let hyph_extra_w = if is_hyphen_break { base_hyph_w } else { 0.0 };
                let current_w = prefix[j].w - base_w + hyph_extra_w;
                let spaces = prefix[j].sp - base_sp;
                let prot = (prefix[j].prot as f32) * 0.1;

                let target = avail + prot;
                let slack = target - current_w;

                if slack < -25.0 {
                    break;
                }

                if j < num_bp {
                    let max_slack = if spaces == 0 {
                        5.0 + emergency_stretch
                    } else {
                        (spaces as f32) * slack_mult + 6.0 + emergency_stretch
                    };
                    if slack > max_slack {
                        continue;
                    }
                }

                let is_final_line = j == num_bp;
                let curr_ends_hyphen = is_hyphen_break;

                let (d_base, clamped_exp, c_curr) = if is_final_line {
                    let target = avail + prot - par_fill_width;
                    let slack = target - current_w;
                    if par_fill_stretch == 0.0 {
                        let (badness, c) = if slack >= 0.0 {
                            let (max_allowable, inv_stretch) = if emergency_stretch == 0.0 && (spaces as usize) < 64 {
                                let sp_idx = spaces as usize;
                                (max_r * STRETCH_TABLE[sp_idx], INV_STRETCH_TABLE[sp_idx])
                            } else {
                                let total_stretch = (spaces as f32) * 1.67 + emergency_stretch;
                                if total_stretch > 0.0 {
                                    (max_r * total_stretch, 1.0 / total_stretch)
                                } else {
                                    (max_r * 5.0, 0.2)
                                }
                            };
                            if slack > max_allowable {
                                continue;
                            }
                            let r = slack * inv_stretch;
                            let b = (100.0 * (r * r * r)).min(10000.0);
                            if b > max_badness {
                                continue;
                            }
                            (b, classify_fitness(slack, b))
                        } else {
                            let (max_allowable, inv_shrink) = if (spaces as usize) < 64 {
                                let sp_idx = spaces as usize;
                                (max_r * SHRINK_TABLE[sp_idx], INV_SHRINK_TABLE[sp_idx])
                            } else {
                                let total_shrink = (spaces as f32) * 1.11;
                                if total_shrink > 0.0 {
                                    (max_r * total_shrink, 1.0 / total_shrink)
                                } else {
                                    (max_r * (1.0 / 0.3), 0.3)
                                }
                            };
                            let slack_abs = slack.abs();
                            if slack_abs > max_allowable {
                                continue;
                            }
                            let r = slack_abs * inv_shrink;
                            let b = (100.0 * (r * r * r)).min(10000.0);
                            if b > max_badness {
                                continue;
                            }
                            (b, classify_fitness(slack, b))
                        };
                        let mut dem = calculate_line_demerits(LINE_PENALTY, badness, 0.0);
                        if pre_tolerance >= 0.0 && pre_tolerance < max_badness && badness > pre_tolerance {
                            dem += 10_000_000.0;
                        }
                        (dem, 0i8, c)
                    } else {
                        let badness = if slack < 0.0 {
                            let r = if (spaces as usize) < 64 {
                                if spaces > 0 {
                                    slack.abs() * INV_SHRINK_TABLE[spaces as usize]
                                } else {
                                    1.0
                                }
                            } else {
                                let space_shrink = (spaces as f32) * 1.11;
                                if space_shrink > 0.0 {
                                    slack.abs() / space_shrink
                                } else {
                                    1.0
                                }
                            };
                            (100.0 * (r * r * r)).min(10000.0)
                        } else {
                            0.0
                        };
                        if badness > max_badness {
                            continue;
                        }
                        let c = if slack < 0.0 && badness >= 13.0 {
                            FITNESS_TIGHT
                        } else {
                            FITNESS_DECENT
                        };
                        let mut dem = calculate_line_demerits(LINE_PENALTY, badness, 0.0);
                        if pre_tolerance >= 0.0 && pre_tolerance < max_badness && badness > pre_tolerance {
                            dem += 10_000_000.0;
                        }
                        (dem, 0i8, c)
                    }
                } else {
                    let (clamped_exp, space_slack) = if slack.abs() <= 2.0 {
                        (0i8, slack)
                    } else {
                        let text_w = (current_w - (spaces as f32) * 2.5).max(1.0);
                        let exp = ((slack * 0.20 / text_w) * 1000.0) as i8;
                        let c_exp =
                            exp.clamp(config.min_expansion_permille, config.max_expansion_permille);
                        (c_exp, slack - (text_w * (c_exp as f32 * 0.001)))
                    };

                    let (b, total_b, c) = if space_slack >= 0.0 {
                        let (max_allowable, inv_stretch) = if emergency_stretch == 0.0 && (spaces as usize) < 64 {
                            let sp_idx = spaces as usize;
                            (max_r * STRETCH_TABLE[sp_idx], INV_STRETCH_TABLE[sp_idx])
                        } else {
                            let total_stretch = (spaces as f32) * 1.67 + emergency_stretch;
                            if total_stretch > 0.0 {
                                (max_r * total_stretch, 1.0 / total_stretch)
                            } else {
                                (max_r * 5.0, 0.2)
                            }
                        };
                        if space_slack > max_allowable {
                            continue;
                        }

                        let r = space_slack * inv_stretch;
                        let b = (100.0 * (r * r * r)).min(10000.0);
                        if b > max_badness {
                            continue;
                        }

                        let total_b = if clamped_exp == 0 {
                            b
                        } else {
                            let exp_norm = (clamped_exp.abs() as f32) * (1.0 / 15.0);
                            (b + 25.0 * (exp_norm * exp_norm)).min(10000.0)
                        };
                        (b, total_b, classify_fitness(space_slack, total_b))
                    } else {
                        let (max_allowable, inv_shrink) = if (spaces as usize) < 64 {
                            let sp_idx = spaces as usize;
                            (max_r * SHRINK_TABLE[sp_idx], INV_SHRINK_TABLE[sp_idx])
                        } else {
                            let total_shrink = (spaces as f32) * 1.11;
                            if total_shrink > 0.0 {
                                (max_r * total_shrink, 1.0 / total_shrink)
                            } else {
                                (max_r * (1.0 / 0.3), 0.3)
                            }
                        };
                        let slack_abs = space_slack.abs();
                        if slack_abs > max_allowable {
                            continue;
                        }

                        let r = slack_abs * inv_shrink;
                        let b = (100.0 * (r * r * r)).min(10000.0);
                        if b > max_badness {
                            continue;
                        }

                        let total_b = if clamped_exp == 0 {
                            b
                        } else {
                            let exp_norm = (clamped_exp.abs() as f32) * (1.0 / 15.0);
                            (b + 25.0 * (exp_norm * exp_norm)).min(10000.0)
                        };
                        (b, total_b, classify_fitness(space_slack, total_b))
                    };

                    let pen = match k {
                        WordKind::MathBin => crate::math::InlineMathPenaltyInserter::BIN_PENALTY as f32,
                        WordKind::MathRel => crate::math::InlineMathPenaltyInserter::REL_PENALTY as f32,
                        WordKind::Hyphen => HYPHEN_PENALTY,
                        _ => {
                            if curr_ends_hyphen {
                                EXHYPHEN_PENALTY
                            } else {
                                0.0
                            }
                        }
                    };

                    let mut dem = calculate_line_demerits(LINE_PENALTY, total_b, pen);
                    if pre_tolerance >= 0.0 && pre_tolerance < max_badness && b > pre_tolerance {
                        dem += 10_000_000.0;
                    }

                    let looseness_bias = (looseness as f32) * -50000.0;
                    (dem + looseness_bias, clamped_exp, c)
                };

                let target_idx = j * 4 + (c_curr as usize);
                let c_idx = c_curr as usize;
                let min_incoming = folded_cost[c_idx];
                if min_incoming < f32::INFINITY {
                    let c_prev = best_c_prev[c_idx];
                    let pred_line = ring[base_ring_idx + c_prev as usize].line_idx;

                    let mut d = d_base;
                    if curr_ends_hyphen && prev_ends_hyphen {
                        d += DOUBLE_HYPHEN_DEMERITS;
                    }
                    if is_final_line && prev_ends_hyphen {
                        d += FINAL_HYPHEN_DEMERITS;
                    }

                    let total_cost = min_incoming + d;
                    let ring_target_idx = ((j * 4) + c_idx) & (DP_RING_NODES - 1);
                    if total_cost < ring[ring_target_idx].cost {
                        let target_flags = if curr_ends_hyphen { DpNode::FLAG_ENDS_HYPHEN } else { 0 };
                        let prot_val = ((prot * 10.0).round() as u8).min(0x7F);
                        let new_line = (pred_line + 1).min(u16::MAX - 1);

                        ring[ring_target_idx] = DpRingSlot {
                            cost: total_cost,
                            line_idx: new_line,
                            flags: target_flags,
                            _pad: 0,
                        };

                        nodes[target_idx] = DpNode {
                            cost: total_cost,
                            prev_node: (i * 4 + c_prev as usize) as u32,
                            prev_word: i as u16,
                            exp: clamped_exp,
                            prot: prot_val,
                            flags: target_flags,
                            _pad: 0,
                            line_idx: new_line,
                        };
                    }
                }
            }
        }
    }

    let mut best_cost = f32::INFINITY;
    let mut best_node_idx = 0;
    let final_ring_base = (num_bp * 4) & (DP_RING_NODES - 1);
    for c in 0..4 {
        let cost = ring[final_ring_base + c].cost;
        if cost < best_cost {
            best_cost = cost;
            best_node_idx = num_bp * 4 + c;
        }
    }

    if best_cost == f32::INFINITY {
        return false;
    }

    if looseness != 0 {
        let current_lines = ring[final_ring_base + (best_node_idx % 4)].line_idx as i32;
        let target_lines = current_lines + looseness;
        let mut best_diff = (current_lines - target_lines).abs();
        for c in 0..4 {
            let slot = ring[final_ring_base + c];
            if slot.cost < f32::INFINITY {
                let lines = slot.line_idx as i32;
                let diff = (lines - target_lines).abs();
                if diff < best_diff || (diff == best_diff && slot.cost < best_cost) {
                    best_diff = diff;
                    best_cost = slot.cost;
                    best_node_idx = num_bp * 4 + c;
                }
            }
        }
    }

    let mut stack_breaks = [(0usize, 0usize, 0i8, 0u8); 64];
    let mut break_count = 0;
    let mut heap_breaks = Vec::new();
    let mut curr_node_idx = best_node_idx;

    while curr_node_idx / 4 > 0 {
        let node = nodes[curr_node_idx];
        let end = curr_node_idx / 4;
        let start = node.prev_word as usize;
        if break_count < 64 && heap_breaks.is_empty() {
            stack_breaks[break_count] = (start, end, node.exp, node.prot);
            break_count += 1;
        } else {
            if heap_breaks.is_empty() {
                heap_breaks.reserve(break_count + 32);
                heap_breaks.extend_from_slice(&stack_breaks[..break_count]);
            }
            heap_breaks.push((start, end, node.exp, node.prot));
        }
        curr_node_idx = node.prev_node as usize;
    }

    let lines_slice = if heap_breaks.is_empty() {
        &mut stack_breaks[..break_count]
    } else {
        &mut heap_breaks[..]
    };
    lines_slice.reverse();

    let mut offset = 0usize;
    let mut curr_start = 0usize;

    for (line_idx, &(_start_bp_idx, end_bp_idx, exp, prot_tenths)) in lines_slice.iter().enumerate() {
        let end_bp = breakpoints[end_bp_idx - 1];
        let orig_w = end_bp.word_idx as usize;
        let w_in_soa = orig_w + offset;

        let (indent, _) = calculate_line_geometry(
            line_idx,
            measure,
            first_line_indent,
            custom_hang_indent,
            hang_after,
            par_shape,
        );

        if end_bp.split_idx != 255 {
            let split_p = end_bp.split_idx as usize;
            let text = soa.texts[w_in_soa];
            let face = soa.attrs[w_in_soa].face();
            let orig_w_val = soa.widths[w_in_soa];

            let fam_face = ((font_family as u8) << 4) | ((face as u8) & 0x0F);
            let word_hash = hash_word(text);
            let set_idx = (word_hash as usize) & crate::layout::layout_hyphen::HYPHEN_CACHE_SET_MASK;
            let slot0 = set_idx * 2;
            let slot1 = slot0 + 1;
            let e0 = hyphen_cache[slot0];
            let e1 = hyphen_cache[slot1];
            let entry = if e0.hash == word_hash
                && e0.word_len == text.len() as u16
                && e0.family_face == fam_face
            {
                e0
            } else {
                e1
            };

            let b_pos = if entry.hash == word_hash
                && entry.word_len == text.len() as u16
                && entry.family_face == fam_face
            {
                (entry.breaks[split_p] as usize).min(text.len())
            } else {
                text.len()
            };
            let prefix_slice = &text[..b_pos];
            let suffix_slice = &text[b_pos..];

            let mut cum_units = 0u32;
            for p in 0..=split_p {
                cum_units += entry.piece_units[p] as u32;
            }
            let prefix_w = (cum_units as f32) * scale;
            let suffix_w = orig_w_val - prefix_w;

            soa.texts[w_in_soa] = prefix_slice;
            soa.widths[w_in_soa] = prefix_w;
            soa.attrs[w_in_soa] = PackedWordAttr::new(WordKind::Hyphen, face);

            soa.texts.insert(w_in_soa + 1, suffix_slice);
            soa.widths.insert(w_in_soa + 1, suffix_w);
            soa.attrs.insert(w_in_soa + 1, PackedWordAttr::new(WordKind::Word, face));

            for m in &mut soa.math_indices {
                if m.word_idx > w_in_soa as u32 {
                    m.word_idx += 1;
                }
            }

            let end_in_soa = w_in_soa + 1;

            emit_line_from_soa(
                soa,
                out_lines,
                line_idx,
                curr_start,
                end_in_soa,
                exp,
                prot_tenths,
                indent,
                line_idx + 1 == lines_slice.len(),
                par_fill_stretch,
            );

            offset += 1;
            curr_start = end_in_soa;
        } else {
            let end_in_soa = w_in_soa + 1;

            emit_line_from_soa(
                soa,
                out_lines,
                line_idx,
                curr_start,
                end_in_soa,
                exp,
                prot_tenths,
                indent,
                line_idx + 1 == lines_slice.len(),
                par_fill_stretch,
            );

            curr_start = end_in_soa;
        }
    }

    true
}

/// Dynamic programming optimal line breaker with parshape support.
/// Delegates to `hz_break_paragraph_three_pass` with Plain TeX defaults (pretolerance=100, tolerance=200, emergencystretch=0.0, looseness=0).
pub fn hz_break_paragraph_optimal_shape<'a>(
    soa: &ParagraphSoA<'a>,
    measure: f32,
    first_line_indent: f32,
    custom_hang_indent: f32,
    hang_after: i32,
    par_shape: &[(f32, f32)],
    config: &HzConfig,
    out_lines: &mut Vec<HzLineDescriptor>,
) {
    hz_break_paragraph_three_pass(
        soa,
        measure,
        first_line_indent,
        custom_hang_indent,
        hang_after,
        par_shape,
        config,
        100,
        200,
        0.0,
        0,
        0.0,
        f32::INFINITY,
        out_lines,
    );
}

/// Optimal line breaker with custom looseness and parshape support (TeX by Topic Ch 19.2.2).
pub fn hz_break_paragraph_looseness<'a>(
    soa: &ParagraphSoA<'a>,
    measure: f32,
    first_line_indent: f32,
    custom_hang_indent: f32,
    hang_after: i32,
    par_shape: &[(f32, f32)],
    config: &HzConfig,
    looseness: i32,
    out_lines: &mut Vec<HzLineDescriptor>,
) -> u8 {
    hz_break_paragraph_three_pass(
        soa,
        measure,
        first_line_indent,
        custom_hang_indent,
        hang_after,
        par_shape,
        config,
        100,
        200,
        0.0,
        looseness,
        0.0,
        f32::INFINITY,
        out_lines,
    )
}

/// Optimal line breaker with parfillskip, looseness, and parshape support (TeX by Topic Ch 17.1 & 19.2.2).
pub fn hz_break_paragraph_parfill<'a>(
    soa: &ParagraphSoA<'a>,
    measure: f32,
    first_line_indent: f32,
    custom_hang_indent: f32,
    hang_after: i32,
    par_shape: &[(f32, f32)],
    config: &HzConfig,
    looseness: i32,
    par_fill_width: f32,
    par_fill_stretch: f32,
    out_lines: &mut Vec<HzLineDescriptor>,
) -> u8 {
    hz_break_paragraph_three_pass(
        soa,
        measure,
        first_line_indent,
        custom_hang_indent,
        hang_after,
        par_shape,
        config,
        100,
        200,
        0.0,
        looseness,
        par_fill_width,
        par_fill_stretch,
        out_lines,
    )
}

/// Bit-packed 16-byte DAG node representation for the Knuth-Plass line breaker.
///
/// Designed for exact 64-byte L1 cache-line symmetry:
/// - Exactly **4 fitness states for each breakpoint fit into one 64-byte CPU cache line** ($4 \times 16 = 64$).
/// - Zero line crossing when inspecting the 4 predecessor fitness classes at a candidate breakpoint.
/// - Stores total accumulated demerits, predecessor DAG index, start word, font expansion,
///   optical margin protrusion, and hyphenation flags.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct DpNode {
    /// Accumulated demerit cost along this path.
    pub cost: f32,
    /// Predecessor index in the flattened (word * 4 + fitness_class) DAG.
    pub prev_node: u32,
    /// Start word index for the line leading to this node.
    pub prev_word: u16,
    /// Font expansion per-mille (-15..+15).
    pub exp: i8,
    /// Optical margin protrusion in tenths of a point (0..127).
    pub prot: u8,
    /// Bit flags:
    /// - Bit 0: Line ends with a hyphen (for \doublehyphendemerits and \finalhyphendemerits).
    pub flags: u8,
    /// Explicit padding byte.
    pub _pad: u8,
    /// 0-indexed line number in the paragraph up to this break.
    pub line_idx: u16,
}

impl DpNode {
    pub const FLAG_ENDS_HYPHEN: u8 = 0x01;

    #[inline(always)]
    pub const fn empty() -> Self {
        Self {
            cost: f32::INFINITY,
            prev_node: 0,
            prev_word: 0,
            exp: 0,
            prot: 0,
            flags: 0,
            _pad: 0,
            line_idx: 0,
        }
    }
}

const _: () = assert!(core::mem::size_of::<DpNode>() == 16);
const _: () = assert!(core::mem::align_of::<DpNode>() == 4);

#[derive(Clone, Copy)]
#[repr(C)]
pub struct PrefixSum {
    pub w: f32,
    pub sp: u16,
    pub prot: u8,
    pub _pad: u8,
}

const _: () = assert!(core::mem::size_of::<PrefixSum>() == 8);
const _: () = assert!(core::mem::align_of::<PrefixSum>() == 4);

#[inline(always)]
fn calculate_line_indent(
    line_index: usize,
    first_line_indent: f32,
    custom_hang: f32,
    hang_after: i32,
) -> f32 {
    let mut indent = if line_index == 0 {
        first_line_indent
    } else {
        0.0
    };
    if custom_hang.abs() > 0.001 {
        let line_num = (line_index + 1) as i32;
        if hang_after >= 0 {
            if line_num > hang_after {
                indent += custom_hang;
            }
        } else if line_num <= hang_after.abs() {
            indent += custom_hang;
        }
    }
    indent
}

/// Generalized line shape query taking either parshape matrix or hanging indentation (TeX by Topic Ch 18.2.2).
#[inline(always)]
pub fn calculate_line_geometry(
    line_index: usize,
    measure: f32,
    first_line_indent: f32,
    custom_hang: f32,
    hang_after: i32,
    par_shape: &[(f32, f32)],
) -> (f32, f32) {
    if !par_shape.is_empty() {
        let m = line_index.min(par_shape.len() - 1);
        let (s_indent, s_len) = par_shape[m];
        let fli = if line_index == 0 { first_line_indent } else { 0.0 };
        let indent = s_indent + fli;
        let avail = (s_len - fli).max(1.0);
        (indent, avail)
    } else {
        let indent = calculate_line_indent(line_index, first_line_indent, custom_hang, hang_after);
        let avail = (measure - indent).max(1.0);
        (indent, avail)
    }
}

#[inline(always)]
fn trim_trailing_spaces<'a>(
    soa: &ParagraphSoA<'a>,
    start: u32,
    end: u32,
    mut line_w: f32,
    mut num_spaces: u32,
) -> (u32, f32, u32) {
    let mut trimmed_end = end;
    if trimmed_end > start && !soa.attrs[(trimmed_end - 1) as usize].is_space() {
        return (trimmed_end, line_w.max(0.0), num_spaces);
    }
    while trimmed_end > start {
        let prev = (trimmed_end - 1) as usize;
        let attr = soa.attrs[prev];
        if attr.is_space() {
            line_w -= soa.widths[prev];
            if attr.kind() == WordKind::Space {
                num_spaces = num_spaces.saturating_sub(1);
            }
            trimmed_end -= 1;
        } else {
            break;
        }
    }
    (trimmed_end, line_w.max(0.0), num_spaces)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_struct_sizes_cache_line_fit() {
        // Assert HzLineDescriptor is EXACTLY 16 bytes: exactly 4 fit in a 64B L1 cache line!
        assert_eq!(std::mem::size_of::<HzLineDescriptor>(), 16);
        assert_eq!(std::mem::align_of::<HzLineDescriptor>(), 4);

        // Assert HzConfig is EXACTLY 4 bytes: fits in a single 32-bit register!
        assert_eq!(std::mem::size_of::<HzConfig>(), 4);
        assert_eq!(std::mem::align_of::<HzConfig>(), 1);

        // Assert PROTRUSION_ASCII_LUT is 64-byte aligned and exactly 128 bytes (2 cache lines)
        assert_eq!(std::mem::size_of::<ProtrusionTable>(), 128);
        assert_eq!(std::mem::align_of::<ProtrusionTable>(), 64);

        // Assert SpaceFactorTable is 64-byte aligned and exactly 256 bytes (4 cache lines)
        assert_eq!(std::mem::size_of::<SpaceFactorTable>(), 256);
        assert_eq!(std::mem::align_of::<SpaceFactorTable>(), 64);

        // Assert FlatAsciiWidthTable is 64-byte aligned and exactly 512 bytes (8 cache lines)
        assert_eq!(std::mem::size_of::<FlatAsciiWidthTable>(), 512);
        assert_eq!(std::mem::align_of::<FlatAsciiWidthTable>(), 64);
    }

    #[test]
    fn test_flat_ascii_width_table_invariants() {
        let base_table = crate::backend::fonts::resolve_width_table(
            crate::backend::fonts::FontFamily::Times,
            Face::Roman,
        );
        let flat = FlatAsciiWidthTable::from_table(base_table);
        assert_eq!(std::mem::size_of::<FlatAsciiWidthTable>(), 512);
        assert_eq!(std::mem::align_of::<FlatAsciiWidthTable>(), 64);

        // Verify all 0..128 ASCII widths match exactly
        for b in 0..128u8 {
            assert_eq!(flat.get_advance_units(b), base_table[b as usize]);
        }
        // Verify upper half (128..256) is zero
        for b in 128..=255u8 {
            assert_eq!(flat.get_advance_units(b), 0);
        }

        // Test string measurement
        let test_str = "RushTeX Microtypography";
        let expected = metrics::string_units_table(test_str, base_table);
        assert_eq!(flat.string_units(test_str), expected);
        assert_eq!(flat.string_units_or(test_str, base_table), expected);
    }

    #[test]
    fn test_simd_lowercase_classifiers() {
        assert!(is_all_ascii_lowercase_16(b"microtypographyx"));
        assert!(!is_all_ascii_lowercase_16(b"microtypography!")); // has '!'
        assert!(!is_all_ascii_lowercase_16(b"Microtypographyx")); // uppercase
        assert!(is_all_ascii_lowercase_8(b"keyboard"));
        assert!(!is_all_ascii_lowercase_8(b"Keyboard")); // uppercase
        assert!(!is_all_ascii_lowercase_8(b"key boar")); // space
        assert!(!is_all_ascii_lowercase_8(b"key-boar")); // hyphen
        assert!(!is_all_ascii_lowercase_8(b"keyboar.")); // period at byte 7
        assert!(!is_all_ascii_lowercase_8(b"short")); // < 8 bytes
    }

    #[test]
    fn test_bit_packing_roundtrip() {
        let desc = HzLineDescriptor::new(
            12,    // start_word
            8,     // word_count
            7,     // num_spaces
            true,  // justify
            -12,   // expansion_permille (-1.2%)
            420.5, // line_w
            18.0,  // line_indent
            1.8,   // protrusion_w
        );

        assert_eq!(desc.start_word, 12);
        assert_eq!(desc.word_count, 8);
        assert_eq!(desc.num_spaces, 7);
        assert!(desc.justify());
        assert_eq!(desc.expansion_permille, -12);
        assert!((desc.line_w - 420.5).abs() < 1e-4);
        assert!((desc.line_indent - 18.0).abs() < 1e-4);
        assert!((desc.protrusion_w() - 1.8).abs() < 1e-2);
        assert!((desc.pdf_tz_operand() - 98.8).abs() < 1e-4);
    }

    #[test]
    fn test_branchless_protrusion_lut() {
        assert!((right_protrusion_factor("hello.") - 0.70).abs() < 1e-4);
        assert!((right_protrusion_factor("world,") - 0.70).abs() < 1e-4);
        assert!((right_protrusion_factor("hyphen-") - 0.75).abs() < 1e-4);
        assert!((right_protrusion_factor("semi;") - 0.50).abs() < 1e-4);
        assert!((right_protrusion_factor("colon:") - 0.50).abs() < 1e-4);
        assert!((right_protrusion_factor("quote'") - 0.40).abs() < 1e-4);
        assert!((right_protrusion_factor("excl!") - 0.30).abs() < 1e-4);
        assert!((right_protrusion_factor("quest?") - 0.30).abs() < 1e-4);
        assert!((right_protrusion_factor("paren)") - 0.25).abs() < 1e-4);
        assert_eq!(right_protrusion_factor("plain"), 0.00);
    }

    #[test]
    fn test_hz_break_paragraph_fast() {
        let mut soa = ParagraphSoA::new();
        soa.push("Introduction", Face::Roman, 55.0, WordKind::Word);
        soa.push(" ", Face::Roman, 5.0, WordKind::Space);
        soa.push("to", Face::Roman, 12.0, WordKind::Word);
        soa.push(" ", Face::Roman, 5.0, WordKind::Space);
        soa.push("Microtypography.", Face::Roman, 78.0, WordKind::Word);

        let config = HzConfig::default();
        let mut lines = Vec::new();
        hz_break_paragraph_fast(&soa, 160.0, 0.0, 0.0, 0, &config, &mut lines);

        assert_eq!(lines.len(), 1);
        assert!(lines[0].protrusion_w() > 0.0);
    }

    #[test]
    fn test_hz_break_paragraph_optimal_dp() {
        let mut soa = ParagraphSoA::new();
        for _ in 0..15 {
            soa.push("sample", Face::Roman, 30.0, WordKind::Word);
            soa.push(" ", Face::Roman, 5.0, WordKind::Space);
        }
        soa.push("final.", Face::Roman, 25.0, WordKind::Word);

        let config = HzConfig::new(15, -15, true, true);
        let mut lines = Vec::new();
        hz_break_paragraph_optimal(&soa, 120.0, 0.0, 0.0, 0, &config, &mut lines);

        assert!(!lines.is_empty());
        for line in &lines {
            assert!(line.expansion_permille >= -15 && line.expansion_permille <= 15);
        }
    }

    #[test]
    fn test_hz_break_paragraph_shape_optimal() {
        let mut soa = ParagraphSoA::new();
        for _ in 0..20 {
            soa.push("geometry", Face::Roman, 35.0, WordKind::Word);
            soa.push(" ", Face::Roman, 5.0, WordKind::Space);
        }
        soa.push("end.", Face::Roman, 20.0, WordKind::Word);

        let shape = [(20.0, 150.0), (0.0, 200.0), (10.0, 180.0)];
        let config = HzConfig::new(15, -15, true, true);
        let mut lines = Vec::new();
        hz_break_paragraph_optimal_shape(&soa, 400.0, 0.0, 0.0, 0, &shape, &config, &mut lines);

        assert!(lines.len() >= 3, "Expected at least 3 lines with shape");
        assert_eq!(lines[0].line_indent, 20.0, "Line 0 should have 20pt indent from parshape");
        assert_eq!(lines[1].line_indent, 0.0, "Line 1 should have 0pt indent from parshape");
        assert_eq!(lines[2].line_indent, 10.0, "Line 2 should have 10pt indent from parshape");
    }

    #[test]
    fn test_bench_hz_performance() {
        let mut soa = ParagraphSoA::new();
        // A realistic ~100 word paragraph
        let words = [
            "Microtypography",
            "is",
            "the",
            "art",
            "and",
            "technique",
            "of",
            "refining",
            "character",
            "spacing,",
            "kerning,",
            "optical",
            "margins,",
            "and",
            "glyph",
            "scaling",
            "to",
            "achieve",
            "visually",
            "uniform",
            "typographic",
            "color.",
            "By",
            "allowing",
            "hyphens,",
            "periods,",
            "commas,",
            "and",
            "quotation",
            "marks",
            "to",
            "protrude",
            "slightly",
            "past",
            "the",
            "right",
            "margin,",
            "the",
            "eye",
            "perceives",
            "a",
            "laser-straight",
            "column",
            "edge.",
            "Hermann",
            "Zapf",
            "and",
            "Peter",
            "Karow",
            "formalized",
            "this",
            "approach",
            "in",
            "the",
            "hz-program,",
            "demonstrating",
            "that",
            "imperceptible",
            "horizontal",
            "font",
            "scaling",
            "absorbs",
            "loose",
            "inter-word",
            "spacing",
            "without",
            "distorting",
            "the",
            "overall",
            "character",
            "proportions",
            "or",
            "page",
            "rhythm.",
        ];
        for (idx, w) in words.iter().enumerate() {
            soa.push(w, Face::Roman, 5.0 * w.len() as f32, WordKind::Word);
            if idx + 1 < words.len() {
                soa.push(" ", Face::Roman, 4.0, WordKind::Space);
            }
        }

        let fast_config = HzConfig::new(15, -15, true, false);
        let optimal_config = HzConfig::new(15, -15, true, true);

        const ITERS: usize = 100_000;
        let mut lines = Vec::with_capacity(32);

        // Warmup CPU and caches
        for _ in 0..10_000 {
            lines.clear();
            hz_break_paragraph_fast(&soa, 300.0, 18.0, 0.0, 0, &fast_config, &mut lines);
            lines.clear();
            hz_break_paragraph_optimal(&soa, 300.0, 18.0, 0.0, 0, &optimal_config, &mut lines);
        }

        // Benchmark Fast O(N)
        let t0 = std::time::Instant::now();
        for _ in 0..ITERS {
            lines.clear();
            hz_break_paragraph_fast(&soa, 300.0, 18.0, 0.0, 0, &fast_config, &mut lines);
        }
        let fast_elapsed = t0.elapsed();

        // Benchmark Optimal DP O(N*W)
        let t1 = std::time::Instant::now();
        for _ in 0..ITERS {
            lines.clear();
            hz_break_paragraph_optimal(&soa, 300.0, 18.0, 0.0, 0, &optimal_config, &mut lines);
        }
        let optimal_elapsed = t1.elapsed();

        eprintln!(
            "\n=== HZ-ALGORITHM MICROBENCHMARK ({} iterations) ===",
            ITERS
        );
        eprintln!(
            "Fast O(N) pass:      {:.2} ns/par  ({:.2} M par/sec)",
            fast_elapsed.as_nanos() as f64 / ITERS as f64,
            ITERS as f64 / fast_elapsed.as_secs_f64() / 1_000_000.0
        );
        eprintln!(
            "Optimal DP O(N*W):   {:.2} ns/par  ({:.2} M par/sec)",
            optimal_elapsed.as_nanos() as f64 / ITERS as f64,
            ITERS as f64 / optimal_elapsed.as_secs_f64() / 1_000_000.0
        );
    }

    #[test]
    fn test_space_factor_code_table_invariants() {
        use super::*;

        // Standard letters are 1000
        assert_eq!(sfcode(b'a', false), 1000);
        assert_eq!(sfcode(b'z', false), 1000);
        assert_eq!(sfcode(b'0', false), 1000);
        assert_eq!(sfcode(b'9', false), 1000);

        // Uppercase characters are 999
        assert_eq!(sfcode(b'A', false), 999);
        assert_eq!(sfcode(b'Z', false), 999);

        // Transparent closing delimiters have sfcode 0
        assert_eq!(sfcode(b')', false), 0);
        assert_eq!(sfcode(b']', false), 0);
        assert_eq!(sfcode(b'}', false), 0);
        assert_eq!(sfcode(b'\'', false), 0);
        assert_eq!(sfcode(b'"', false), 0);

        // Punctuation under default \nonfrenchspacing (TeX by Topic Section 20.5)
        assert_eq!(sfcode(b'.', false), 3000);
        assert_eq!(sfcode(b'?', false), 3000);
        assert_eq!(sfcode(b'!', false), 3000);
        assert_eq!(sfcode(b':', false), 2000);
        assert_eq!(sfcode(b';', false), 1500);
        assert_eq!(sfcode(b',', false), 1250);

        // Punctuation under \frenchspacing is uniformly 1000
        assert_eq!(sfcode(b'.', true), 1000);
        assert_eq!(sfcode(b'?', true), 1000);
        assert_eq!(sfcode(b'!', true), 1000);
        assert_eq!(sfcode(b':', true), 1000);
        assert_eq!(sfcode(b';', true), 1000);
        assert_eq!(sfcode(b',', true), 1000);
    }

    #[test]
    fn test_update_space_factor_initials_and_transparency() {
        use super::*;

        // 1. Lowercase sentence end: "word." -> sf 3000
        let mut sf = 1000u16;
        for &b in b"word." {
            sf = update_space_factor(sf, b, false);
        }
        assert_eq!(sf, 3000);

        // 2. Initials rule: "NASA." -> sf 1000 (preceding uppercase character 'A' had sf 999 < 1000)
        let mut sf_nasa = 1000u16;
        for &b in b"NASA." {
            sf_nasa = update_space_factor(sf_nasa, b, false);
        }
        assert_eq!(sf_nasa, 1000, "Uppercase letter before dot must prevent elongation");

        // 3. Transparent delimiter: "(word.)" -> closing ')' retains dot's sf 3000
        let mut sf_paren = 1000u16;
        for &b in b"(word.)" {
            sf_paren = update_space_factor(sf_paren, b, false);
        }
        assert_eq!(sf_paren, 3000, "Closing parenthesis must retain dot's space factor");

        // 4. French spacing: "word." -> sf 1000
        let mut sf_french = 1000u16;
        for &b in b"word." {
            sf_french = update_space_factor(sf_french, b, true);
        }
        assert_eq!(sf_french, 1000, "French spacing must keep space factor at 1000");
    }

    #[test]
    fn test_dp_node_size_and_cache_line_fit() {
        // Assert DpNode is EXACTLY 16 bytes: exactly 4 fitness states fit in a 64B L1 cache line!
        assert_eq!(std::mem::size_of::<DpNode>(), 16);
        assert_eq!(std::mem::align_of::<DpNode>(), 4);
        assert_eq!(std::mem::size_of::<DpNode>() * 4, 64);
    }

    #[test]
    fn test_tex_fitness_classification() {
        // Class 3 (Tight): line shrunk with badness >= 13 (r <= -0.5)
        assert_eq!(classify_fitness(-1.0, 13.0), FITNESS_TIGHT);
        assert_eq!(classify_fitness(-5.0, 100.0), FITNESS_TIGHT);

        // Class 2 (Decent): line badness <= 12 (-0.5 <= r <= 0.5)
        assert_eq!(classify_fitness(-0.5, 12.0), FITNESS_DECENT);
        assert_eq!(classify_fitness(0.0, 0.0), FITNESS_DECENT);
        assert_eq!(classify_fitness(0.5, 12.0), FITNESS_DECENT);

        // Class 1 (Loose): line stretched with badness 13..99 (0.5 <= r <= 1.0)
        assert_eq!(classify_fitness(1.0, 13.0), FITNESS_LOOSE);
        assert_eq!(classify_fitness(2.0, 99.0), FITNESS_LOOSE);

        // Class 0 (Very Loose): line stretched with badness >= 100 (r > 1.0)
        assert_eq!(classify_fitness(2.5, 100.0), FITNESS_VERY_LOOSE);
        assert_eq!(classify_fitness(5.0, 800.0), FITNESS_VERY_LOOSE);
    }

    #[test]
    fn test_tex_demerit_quadratic_equation() {
        let l = LINE_PENALTY; // 10.0
        let b = 10.0;

        // 1. Non-negative penalty p >= 0: d = (l + b)^2 + p^2
        let p_pos = 50.0;
        let d_pos = calculate_line_demerits(l, b, p_pos);
        assert_eq!(d_pos, (10.0 + 10.0) * (10.0 + 10.0) + 50.0 * 50.0); // 400 + 2500 = 2900

        // 2. Negative penalty -10000 < p < 0: d = (l + b)^2 - p^2
        let p_neg = -10.0;
        let d_neg = calculate_line_demerits(l, b, p_neg);
        assert_eq!(d_neg, (10.0 + 10.0) * (10.0 + 10.0) - 10.0 * 10.0); // 400 - 100 = 300

        // 3. Forced break p <= -10000: d = (l + b)^2
        let p_forced = -10000.0;
        let d_forced = calculate_line_demerits(l, b, p_forced);
        assert_eq!(d_forced, (10.0 + 10.0) * (10.0 + 10.0)); // 400
    }

    #[test]
    fn test_tex_adjacent_demerits_incompatibility() {
        // Adjacent classes differing by > 1 (e.g. Decent 2 vs Very Loose 0) must incur \adjdemerits
        let diff_0_2 = (FITNESS_VERY_LOOSE as i32 - FITNESS_DECENT as i32).abs();
        assert!(diff_0_2 > 1, "Very Loose (0) and Decent (2) must be visually incompatible");

        // Adjacent classes differing by <= 1 (e.g. Loose 1 vs Decent 2) are compatible
        let diff_1_2 = (FITNESS_LOOSE as i32 - FITNESS_DECENT as i32).abs();
        assert!(diff_1_2 <= 1, "Loose (1) and Decent (2) must be visually compatible");

        // Tight (3) and Loose (1) differ by 2 -> incompatible
        let diff_3_1 = (FITNESS_TIGHT as i32 - FITNESS_LOOSE as i32).abs();
        assert!(diff_3_1 > 1, "Tight (3) and Loose (1) must be visually incompatible");
    }

    #[test]
    fn test_tex_double_and_final_hyphendemerits() {
        assert_eq!(DOUBLE_HYPHEN_DEMERITS, 10000.0);
        assert_eq!(FINAL_HYPHEN_DEMERITS, 5000.0);
        assert!(word_ends_with_hyphen("well-"));
        assert!(word_ends_with_hyphen("semi\u{00AD}"));
        assert!(!word_ends_with_hyphen("normal"));
    }

    #[test]
    fn test_three_pass_line_breaking_pass1_success() {
        let mut soa = ParagraphSoA::new();
        for _ in 0..10 {
            soa.push("word", Face::Roman, 20.0, WordKind::Word);
            soa.push(" ", Face::Roman, 5.0, WordKind::Space);
        }
        soa.push("end.", Face::Roman, 15.0, WordKind::Word);

        let config = HzConfig::new(15, -15, true, true);
        let mut lines = Vec::new();
        let pass = hz_break_paragraph_three_pass(
            &soa,
            120.0,
            0.0,
            0.0,
            0,
            &[],
            &config,
            100, // pretolerance
            200, // tolerance
            0.0, // emergencystretch
            0,   // looseness
            0.0, // par_fill_width
            f32::INFINITY, // par_fill_stretch
            &mut lines,
        );

        assert_eq!(pass, 1, "Should succeed in Pass 1 without hyphenation");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_three_pass_line_breaking_pass1_skip_when_negative() {
        let mut soa = ParagraphSoA::new();
        for _ in 0..10 {
            soa.push("word", Face::Roman, 20.0, WordKind::Word);
            soa.push(" ", Face::Roman, 5.0, WordKind::Space);
        }
        soa.push("end.", Face::Roman, 15.0, WordKind::Word);

        let config = HzConfig::new(15, -15, true, true);
        let mut lines = Vec::new();
        let pass = hz_break_paragraph_three_pass(
            &soa,
            120.0,
            0.0,
            0.0,
            0,
            &[],
            &config,
            -1,  // pretolerance < 0: Pass 1 skipped
            200, // tolerance
            0.0,
            0, // looseness
            0.0,
            f32::INFINITY,
            &mut lines,
        );

        assert_eq!(pass, 2, "Should skip Pass 1 and succeed in Pass 2");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_three_pass_line_breaking_pass2_hyphenation() {
        let mut soa = ParagraphSoA::new();
        // Line 1 target: measure = 80.0 pt
        // Without hyphenation:
        // - break at "short": w = 45 pt, slack = 35 pt, sp = 1 -> r = 20.9 -> badness 10000 > 100 (fails)
        // - break at "syllable": w = 105 pt > 80 pt -> overfull (fails)
        // With hyphenation at "multi-":
        // - w = 15 + 5 + 25 + 5 + 25 + 3.3 = 78.3 pt. Spaces = 2.
        // - Target = 80.0 + 2.5 (protrusion) = 82.5 pt. Slack = 4.2 pt.
        // - total_stretch = 2 * 1.67 = 3.34 pt -> r = 1.257 -> badness = 198.6 <= 200 (Pass 2 succeeds!)
        soa.push("a", Face::Roman, 15.0, WordKind::Word);
        soa.push(" ", Face::Roman, 5.0, WordKind::Space);
        soa.push("short", Face::Roman, 25.0, WordKind::Word);
        soa.push(" ", Face::Roman, 5.0, WordKind::Space);
        soa.push("multi", Face::Roman, 25.0, WordKind::Hyphen); // Discretionary hyphen
        soa.push("syllable", Face::Roman, 30.0, WordKind::Word);
        soa.push(" ", Face::Roman, 5.0, WordKind::Space);
        soa.push("end.", Face::Roman, 20.0, WordKind::Word);

        let config = HzConfig::new(0, 0, false, true); // no hz expansion to isolate TeX badness
        let mut lines = Vec::new();
        let pass = hz_break_paragraph_three_pass(
            &soa,
            80.0,
            0.0,
            0.0,
            0,
            &[],
            &config,
            100, // pretolerance
            200, // tolerance
            0.0,
            0, // looseness
            0.0,
            f32::INFINITY,
            &mut lines,
        );

        assert_eq!(pass, 2, "Pass 1 should fail due to tight/overfull line; Pass 2 should succeed via hyphenation");
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_three_pass_line_breaking_pass3_emergencystretch() {
        let mut soa = ParagraphSoA::new();
        // A single long unbreakable word on line 1, then space, then next word
        soa.push("superlongwordhere", Face::Roman, 80.0, WordKind::Word);
        soa.push(" ", Face::Roman, 5.0, WordKind::Space);
        soa.push("nextword", Face::Roman, 40.0, WordKind::Word);

        let config = HzConfig::new(0, 0, false, true);
        let mut lines_no_em = Vec::new();
        // Measure 100 pt:
        // Line 1: word1 (80) + space (5) = 85. Slack = 15. Spaces = 1.
        // total_stretch = 1 * 1.67 = 1.67.
        // r = 15 / 1.67 = 8.98 -> badness = 100 * 8.98^3 = 72000 -> 10000.
        // Badness > 200 -> Pass 1 & Pass 2 fail!
        let pass_no_em = hz_break_paragraph_three_pass(
            &soa,
            100.0,
            0.0,
            0.0,
            0,
            &[],
            &config,
            100,
            200,
            0.0, // emergency_stretch = 0.0
            0,   // looseness
            0.0,
            f32::INFINITY,
            &mut lines_no_em,
        );
        assert_eq!(pass_no_em, 0, "Without emergencystretch, should fail passes 1-3 and use fallback");

        let mut lines_with_em = Vec::new();
        // With emergency_stretch = 30.0:
        // total_stretch = 1 * 1.67 + 30.0 = 31.67.
        // r = 15 / 31.67 = 0.47 -> badness = 100 * 0.47^3 = 10.3 <= 200!
        // Pass 3 succeeds!
        let pass_with_em = hz_break_paragraph_three_pass(
            &soa,
            100.0,
            0.0,
            0.0,
            0,
            &[],
            &config,
            100,
            200,
            30.0, // emergency_stretch = 30.0
            0,    // looseness
            0.0,
            f32::INFINITY,
            &mut lines_with_em,
        );
        assert_eq!(pass_with_em, 3, "With emergencystretch = 30.0, Pass 3 should succeed");
        assert_eq!(lines_with_em.len(), 2);
    }

    #[test]
    fn test_looseness_modifies_line_count() {
        let words = [
            "By", "allowing", "hyphens,", "periods,", "commas,", "and", "quotation", "marks",
            "to", "protrude", "slightly", "past", "the", "right", "margin,", "the", "eye",
            "perceives", "a", "laser-straight", "column", "edge.", "Hermann", "Zapf", "and",
            "Peter", "Karow", "formalized", "this", "approach", "in", "the", "hz-program,",
            "demonstrating", "that", "imperceptible", "horizontal", "font", "scaling", "absorbs",
            "loose", "inter-word", "spacing", "without", "distorting", "the", "overall",
            "character", "proportions", "or", "page", "rhythm.",
        ];
        let mut soa = ParagraphSoA::new();
        for (idx, w) in words.iter().enumerate() {
            soa.push(w, Face::Roman, 5.0 * w.len() as f32, WordKind::Word);
            if idx + 1 < words.len() {
                soa.push(" ", Face::Roman, 4.0, WordKind::Space);
            }
        }

        let config = HzConfig::new(15, -15, true, true);
        let mut lines_normal = Vec::new();
        hz_break_paragraph_three_pass(
            &soa, 250.0, 0.0, 0.0, 0, &[], &config, 500, 1000, 0.0, 0, 0.0, f32::INFINITY, &mut lines_normal,
        );

        let mut lines_loose = Vec::new();
        hz_break_paragraph_three_pass(
            &soa, 250.0, 0.0, 0.0, 0, &[], &config, 500, 1000, 0.0, 1, 0.0, f32::INFINITY, &mut lines_loose,
        );

        assert!(
            lines_loose.len() >= lines_normal.len(),
            "Looseness = 1 should produce at least as many lines as looseness = 0 (got loose={}, normal={})",
            lines_loose.len(),
            lines_normal.len()
        );
    }

    #[test]
    fn test_parfillskip_forces_full_justification_when_zero_stretch() {
        let mut soa = ParagraphSoA::new();
        soa.push("first", Face::Roman, 20.0, WordKind::Word);
        soa.push(" ", Face::Roman, 5.0, WordKind::Space);
        soa.push("second", Face::Roman, 25.0, WordKind::Word);
        soa.push(" ", Face::Roman, 5.0, WordKind::Space);
        soa.push("third", Face::Roman, 20.0, WordKind::Word);

        let config = HzConfig::new(0, 0, false, false);

        // 1. Default parfillskip (infinite stretch) -> last line is ragged-right (justify == false)
        let mut lines_ragged = Vec::new();
        hz_break_paragraph_three_pass(
            &soa, 100.0, 0.0, 0.0, 0, &[], &config, 100, 200, 0.0, 0, 0.0, f32::INFINITY, &mut lines_ragged,
        );
        assert_eq!(lines_ragged.len(), 1);
        assert!(!lines_ragged[0].justify(), "Default parfillskip should NOT justify the final line");

        // 2. Zero-stretch parfillskip (par_fill_stretch = 0.0) -> forces full justification of final line (justify == true)
        let mut lines_justified = Vec::new();
        hz_break_paragraph_three_pass(
            &soa, 100.0, 0.0, 0.0, 0, &[], &config, 100, 200, 0.0, 0, 0.0, 0.0, &mut lines_justified,
        );
        assert_eq!(lines_justified.len(), 1);
        assert!(lines_justified[0].justify(), "Zero-stretch parfillskip MUST force full justification of the final line");
    }

    #[test]
    fn test_try_layout_single_line_paragraph() {
        let mut soa = ParagraphSoA::new();
        soa.push("Single", Face::Roman, 30.0, WordKind::Word);
        soa.push(" ", Face::Roman, 5.0, WordKind::Space);
        soa.push("line", Face::Roman, 20.0, WordKind::Word);
        soa.push(" ", Face::Roman, 5.0, WordKind::Space);
        soa.push("text", Face::Roman, 20.0, WordKind::Word);

        let mut lines = Vec::new();
        // 1. Fits within measure (width = 80.0 <= 100.0) -> returns true
        let ok = try_layout_single_line_paragraph(&soa, 100.0, 0.0, 0.0, f32::INFINITY, &[], 0, &mut lines);
        assert!(ok);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].word_count, 5);
        assert_eq!(lines[0].num_spaces, 2);
        assert!(!lines[0].justify());

        // 2. Exceeds measure (width = 80.0 > 70.0) -> returns false
        lines.clear();
        let ok_exceed = try_layout_single_line_paragraph(&soa, 70.0, 0.0, 0.0, f32::INFINITY, &[], 0, &mut lines);
        assert!(!ok_exceed);
        assert!(lines.is_empty());

        // 3. Forced looseness -> returns false (must use full DP)
        lines.clear();
        let ok_loose = try_layout_single_line_paragraph(&soa, 100.0, 0.0, 0.0, f32::INFINITY, &[], 1, &mut lines);
        assert!(!ok_loose);

        // 4. Parshape present -> returns false (must use full DP)
        lines.clear();
        let ok_shape = try_layout_single_line_paragraph(&soa, 100.0, 0.0, 0.0, f32::INFINITY, &[(0.0, 100.0)], 0, &mut lines);
        assert!(!ok_shape);
    }
}
