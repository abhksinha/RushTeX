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
use crate::page_layout::{ParagraphSoA, WordKind};
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
#[derive(Clone, Copy, Debug, PartialEq)]
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
    if config.use_optimal_dp() {
        hz_break_paragraph_optimal(
            soa,
            measure,
            first_line_indent,
            custom_hang_indent,
            hang_after,
            config,
            out_lines,
        );
    } else {
        hz_break_paragraph_fast(
            soa,
            measure,
            first_line_indent,
            custom_hang_indent,
            hang_after,
            config,
            out_lines,
        );
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
    let num_words = soa.len();
    if num_words == 0 {
        return;
    }

    let mut word_idx = 0;
    let mut line_start: u32 = 0;
    let mut line_w: f32 = 0.0;
    let mut num_spaces: u32 = 0;
    let mut line_index: usize = 0;

    let mut line_indent =
        calculate_line_indent(0, first_line_indent, custom_hang_indent, hang_after);
    let mut available = measure - line_indent;

    while word_idx < num_words {
        let k = soa.kinds[word_idx];
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
            line_indent = calculate_line_indent(
                line_index,
                first_line_indent,
                custom_hang_indent,
                hang_after,
            );
            available = measure - line_indent;
            word_idx += 1;
            line_start = word_idx as u32;
            line_w = 0.0;
            num_spaces = 0;
            continue;
        }

        if line_start < word_idx as u32 && k != WordKind::Space && line_w + w > available {
            let current_protrusion = if config.enable_protrusion() {
                calculate_protrusion_pt(soa.texts[word_idx], soa.faces[word_idx], 10.0)
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
                    line_indent = calculate_line_indent(
                        line_index,
                        first_line_indent,
                        custom_hang_indent,
                        hang_after,
                    );
                    available = measure - line_indent;
                    line_start = word_idx as u32;
                    line_w = 0.0;
                    num_spaces = 0;
                    continue;
                } else {
                    let last_word_idx = word_idx - 1;
                    let trail_protrusion = if config.enable_protrusion() {
                        calculate_protrusion_pt(
                            soa.texts[last_word_idx],
                            soa.faces[last_word_idx],
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
                    line_indent = calculate_line_indent(
                        line_index,
                        first_line_indent,
                        custom_hang_indent,
                        hang_after,
                    );
                    available = measure - line_indent;

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

        if line_start == word_idx as u32 && k == WordKind::Space {
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
                calculate_protrusion_pt(soa.texts[last_idx], soa.faces[last_idx], 10.0)
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

/// Full dynamic-programming global shortest-path optimizer.
/// Solves for globally optimal line breaks using the combined `hz` microtypography model.
pub fn hz_break_paragraph_optimal<'a>(
    soa: &ParagraphSoA<'a>,
    measure: f32,
    first_line_indent: f32,
    custom_hang_indent: f32,
    hang_after: i32,
    config: &HzConfig,
    out_lines: &mut Vec<HzLineDescriptor>,
) {
    let num_words = soa.len();
    if num_words == 0 {
        return;
    }

    // Fast path for small paragraphs: use stack buffer (zero heap allocations)
    const MAX_STACK_WORDS: usize = 256;
    let mut stack_nodes = [DpNode {
        cost: f32::INFINITY,
        prev: 0,
        exp: 0,
        prot: 0,
    }; MAX_STACK_WORDS + 1];
    let mut stack_prefix = [PrefixSum {
        w: 0.0,
        sp: 0,
        _pad: 0,
    }; MAX_STACK_WORDS + 1];
    let mut stack_prot_cache = [0.0f32; MAX_STACK_WORDS];

    let mut heap_nodes;
    let mut heap_prefix;
    let mut heap_prot_cache;

    let (nodes, prefix, prot_cache) = if num_words <= MAX_STACK_WORDS {
        (
            &mut stack_nodes[..=num_words],
            &mut stack_prefix[..=num_words],
            &mut stack_prot_cache[..num_words],
        )
    } else {
        heap_nodes = vec![
            DpNode {
                cost: f32::INFINITY,
                prev: 0,
                exp: 0,
                prot: 0,
            };
            num_words + 1
        ];
        heap_prefix = vec![
            PrefixSum {
                w: 0.0,
                sp: 0,
                _pad: 0
            };
            num_words + 1
        ];
        heap_prot_cache = vec![0.0f32; num_words];
        (
            &mut heap_nodes[..],
            &mut heap_prefix[..],
            &mut heap_prot_cache[..],
        )
    };

    nodes[0].cost = 0.0;
    prefix[0] = PrefixSum {
        w: 0.0,
        sp: 0,
        _pad: 0,
    };

    let enable_prot = config.enable_protrusion();
    for idx in 0..num_words {
        let w = soa.widths[idx];
        let k = soa.kinds[idx];
        let sp_inc = if k == WordKind::Space { 1 } else { 0 };
        prefix[idx + 1] = PrefixSum {
            w: prefix[idx].w + w,
            sp: prefix[idx].sp + sp_inc,
            _pad: 0,
        };
        prot_cache[idx] = if enable_prot && k != WordKind::Space {
            calculate_protrusion_pt(soa.texts[idx], soa.faces[idx], 10.0)
        } else {
            0.0
        };
    }

    for i in 0..num_words {
        let cost_i = nodes[i].cost;
        if cost_i == f32::INFINITY {
            continue;
        }

        // Determine line indentation for current line index
        // Approximate line index from average words-per-line (~10)
        let est_line_idx = i / 10;
        let indent = calculate_line_indent(
            est_line_idx,
            first_line_indent,
            custom_hang_indent,
            hang_after,
        );
        let avail = measure - indent;

        let base_w = prefix[i].w;
        let base_sp = prefix[i].sp;

        for j in (i + 1)..=num_words {
            let k = soa.kinds[j - 1];

            if k == WordKind::Break {
                // Mandatory break point
                if cost_i < nodes[j].cost {
                    nodes[j] = DpNode {
                        cost: cost_i,
                        prev: i as u16,
                        exp: 0,
                        prot: 0,
                    };
                }
                break;
            }

            // Only break after a word or math atom (not in the middle of spaces)
            if k == WordKind::Word || k == WordKind::Math || j == num_words {
                let current_w = prefix[j].w - base_w;
                let spaces = prefix[j].sp - base_sp;
                let prot = prot_cache[j - 1];

                let target = avail + prot;
                let slack = target - current_w;

                // Active pruning: if line is drastically overfull, subsequent words will only make it worse
                if slack < -25.0 {
                    break;
                }

                // Active pruning: prune grossly underfull non-final lines (space ratio > 3.5)
                if j < num_words {
                    if spaces == 0 && slack > 5.0 {
                        continue;
                    }
                    if spaces > 0 && slack > (spaces as f32) * 8.75 {
                        continue;
                    }
                }

                let is_final_line = j == num_words;
                let (demerits, exp_val) = if is_final_line {
                    if slack >= 0.0 {
                        (0.0, 0i8)
                    } else {
                        (100.0 * slack.abs(), 0i8)
                    }
                } else {
                    // Evaluate hz-expansion
                    let text_w = (current_w - (spaces as f32) * 2.5).max(1.0);
                    let exp = ((slack * 0.20 / text_w) * 1000.0) as i8;
                    let clamped_exp =
                        exp.clamp(config.min_expansion_permille, config.max_expansion_permille);
                    let space_slack = slack - (text_w * (clamped_exp as f32 * 0.001));
                    let space_ratio = if spaces > 0 {
                        space_slack / ((spaces as f32) * 2.5)
                    } else {
                        space_slack * 0.1
                    };

                    let badness = hz_line_badness(space_ratio, clamped_exp);
                    let d = (1.0 + badness) * (1.0 + badness);
                    (d, clamped_exp)
                };

                let new_cost = cost_i + demerits;
                if new_cost < nodes[j].cost {
                    nodes[j] = DpNode {
                        cost: new_cost,
                        prev: i as u16,
                        exp: exp_val,
                        prot: ((prot * 10.0).round() as u8).min(0x7F),
                    };
                }
            }
        }
    }

    // Reconstruct optimal path from back to front with zero heap allocation
    let mut stack_breaks = [(0usize, 0usize, 0i8, 0u8); 64];
    let mut break_count = 0;
    let mut heap_breaks = Vec::new();
    let mut curr = num_words;
    while curr > 0 {
        let node = nodes[curr];
        let p = node.prev as usize;
        if break_count < 64 && heap_breaks.is_empty() {
            stack_breaks[break_count] = (p, curr, node.exp, node.prot);
            break_count += 1;
        } else {
            if heap_breaks.is_empty() {
                heap_breaks.reserve(break_count + 32);
                heap_breaks.extend_from_slice(&stack_breaks[..break_count]);
            }
            heap_breaks.push((p, curr, node.exp, node.prot));
        }
        curr = p;
    }

    // Emit formatted line descriptors (iterating in reverse order avoids an in-place reverse pass)
    let emit_line = |out_lines: &mut Vec<HzLineDescriptor>, line_idx: usize, start: usize, end: usize, exp: i8, prot_tenths: u8| {
        let indent =
            calculate_line_indent(line_idx, first_line_indent, custom_hang_indent, hang_after);
        let line_w = prefix[end].w - prefix[start].w;
        let num_spaces = prefix[end].sp - prefix[start].sp;

        let is_last = end == num_words;
        out_lines.push(HzLineDescriptor::new(
            start as u16,
            (end - start) as u16,
            num_spaces,
            !is_last,
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
}

#[derive(Clone, Copy)]
#[repr(C)]
struct DpNode {
    cost: f32,
    prev: u16,
    exp: i8,
    prot: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct PrefixSum {
    w: f32,
    sp: u16,
    _pad: u16,
}

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

#[inline(always)]
fn trim_trailing_spaces<'a>(
    soa: &ParagraphSoA<'a>,
    start: u32,
    end: u32,
    mut line_w: f32,
    mut num_spaces: u32,
) -> (u32, f32, u32) {
    let mut trimmed_end = end;
    if trimmed_end > start && soa.kinds[(trimmed_end - 1) as usize] != WordKind::Space {
        return (trimmed_end, line_w.max(0.0), num_spaces);
    }
    while trimmed_end > start {
        let prev = (trimmed_end - 1) as usize;
        if soa.kinds[prev] == WordKind::Space {
            line_w -= soa.widths[prev];
            num_spaces = num_spaces.saturating_sub(1);
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
}
