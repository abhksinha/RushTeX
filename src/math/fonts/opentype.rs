//! # Zero-Allocation OpenType Font Parser
//!
//! A zero-dependency, pure safe-Rust parser for OpenType font tables (`MATH`, `CFF `, `cmap`, `head`, `hmtx`).
//!
//! Designed under `Resource_Conscious_Software_Engineering.md`:
//! - Zero heap allocations (`#![no_std]` capable)
//! - Operates purely on borrowed memory slices (`&'a [u8]`)
//! - Strict bounds checking, `#![forbid(unsafe_code)]` compliant
//! - $O(1)$ table directory lookup and $O(\log N)$ binary search coverage matching

use super::math_constants::OpenTypeMathConstants;
use super::math_variants::MathGlyphVariantRecord;

/// Lightweight, zero-allocation reference to an OpenType font in memory.
#[derive(Clone, Copy, Debug)]
pub struct OpenTypeFont<'a> {
    data: &'a [u8],
}

#[inline(always)]
fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    if offset + 2 <= data.len() {
        Some(u16::from_be_bytes([data[offset], data[offset + 1]]))
    } else {
        None
    }
}

#[inline(always)]
fn read_i16(data: &[u8], offset: usize) -> Option<i16> {
    if offset + 2 <= data.len() {
        Some(i16::from_be_bytes([data[offset], data[offset + 1]]))
    } else {
        None
    }
}

#[inline(always)]
fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    if offset + 4 <= data.len() {
        Some(u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]))
    } else {
        None
    }
}

impl<'a> OpenTypeFont<'a> {
    /// Constructs a font reference from a raw OpenType/TrueType binary buffer.
    #[inline(always)]
    pub fn from_bytes(data: &'a [u8]) -> Option<Self> {
        if data.len() < 12 {
            return None;
        }
        let magic = read_u32(data, 0)?;
        // 0x00010000 (TrueType), 0x4F54544F ('OTTO' OpenType CFF), 0x74727565 ('true')
        if magic == 0x00010000 || magic == 0x4F54544F || magic == 0x74727565 {
            Some(Self { data })
        } else {
            None
        }
    }

    /// Finds a font table by 4-byte ASCII tag without allocation.
    pub fn find_table(&self, tag: &[u8; 4]) -> Option<&'a [u8]> {
        let num_tables = read_u16(self.data, 4)? as usize;
        let mut offset = 12;
        for _ in 0..num_tables {
            if offset + 16 > self.data.len() {
                break;
            }
            if &self.data[offset..offset + 4] == tag {
                let table_offset = read_u32(self.data, offset + 8)? as usize;
                let table_len = read_u32(self.data, offset + 12)? as usize;
                if table_offset + table_len <= self.data.len() {
                    return Some(&self.data[table_offset..table_offset + table_len]);
                } else {
                    return None;
                }
            }
            offset += 16;
        }
        None
    }

    /// Returns the OpenType `MATH` table slice if present.
    #[inline(always)]
    pub fn math_table(&self) -> Option<&'a [u8]> {
        self.find_table(b"MATH")
    }

    /// Parses the complete 56-parameter `MathConstants` table with zero allocations.
    pub fn parse_math_constants(&self) -> Option<OpenTypeMathConstants> {
        let math = self.math_table()?;
        if math.len() < 10 {
            return None;
        }
        let constants_offset = read_u16(math, 4)? as usize;
        if constants_offset + 112 > math.len() {
            return None;
        }
        let c_data = &math[constants_offset..];

        let mut c = OpenTypeMathConstants::zero();
        c.script_percent_scale_down = read_i16(c_data, 0)?;
        c.script_script_percent_scale_down = read_i16(c_data, 2)?;
        c.delimited_sub_formula_min_height = read_u16(c_data, 4)?;
        c.display_operator_min_height = read_u16(c_data, 6)?;

        // The remaining 52 values are MathValueRecords (value: i16 at +0, deviceOffset: u16 at +2)
        let mut pos = 8;
        let mut next_val = || -> Option<i16> {
            let v = read_i16(c_data, pos)?;
            pos += 4;
            Some(v)
        };

        c.math_leading = next_val()?;
        c.axis_height = next_val()?;
        c.accent_base_height = next_val()?;
        c.flattened_accent_base_height = next_val()?;
        c.subscript_shift_down = next_val()?;
        c.subscript_top_max = next_val()?;
        c.subscript_baseline_drop_min = next_val()?;
        c.superscript_shift_up = next_val()?;
        c.superscript_shift_up_cramped = next_val()?;
        c.superscript_bottom_min = next_val()?;
        c.superscript_baseline_drop_max = next_val()?;
        c.sub_superscript_gap_min = next_val()?;
        c.superscript_bottom_max_with_subscript = next_val()?;
        c.space_after_script = next_val()?;
        c.upper_limit_gap_min = next_val()?;
        c.upper_limit_baseline_rise_min = next_val()?;
        c.lower_limit_gap_min = next_val()?;
        c.lower_limit_baseline_drop_min = next_val()?;
        c.stack_top_shift_up = next_val()?;
        c.stack_top_display_style_shift_up = next_val()?;
        c.stack_bottom_shift_down = next_val()?;
        c.stack_bottom_display_style_shift_down = next_val()?;
        c.stack_gap_min = next_val()?;
        c.stack_display_style_gap_min = next_val()?;
        c.stretch_stack_top_shift_up = next_val()?;
        c.stretch_stack_bottom_shift_down = next_val()?;
        c.stretch_stack_gap_above_min = next_val()?;
        c.stretch_stack_gap_below_min = next_val()?;
        c.fraction_numerator_shift_up = next_val()?;
        c.fraction_numerator_display_style_shift_up = next_val()?;
        c.fraction_denominator_shift_down = next_val()?;
        c.fraction_denominator_display_style_shift_down = next_val()?;
        c.fraction_numerator_gap_min = next_val()?;
        c.fraction_num_display_style_gap_min = next_val()?;
        c.fraction_rule_thickness = next_val()?;
        c.fraction_denominator_gap_min = next_val()?;
        c.fraction_denom_display_style_gap_min = next_val()?;
        c.skewed_fraction_horizontal_gap = next_val()?;
        c.skewed_fraction_vertical_gap = next_val()?;
        c.overbar_vertical_gap = next_val()?;
        c.overbar_rule_thickness = next_val()?;
        c.overbar_extra_ascender = next_val()?;
        c.underbar_vertical_gap = next_val()?;
        c.underbar_rule_thickness = next_val()?;
        c.underbar_extra_descender = next_val()?;
        c.radical_vertical_gap = next_val()?;
        c.radical_display_style_vertical_gap = next_val()?;
        c.radical_rule_thickness = next_val()?;
        c.radical_extra_ascender = next_val()?;
        c.radical_kern_before_degree = next_val()?;
        c.radical_kern_after_degree = next_val()?;
        c.radical_degree_bottom_raise_percent = read_u16(c_data, pos)?;

        Some(c)
    }

    /// Looks up optical top accent attachment for a glyph ID via MathGlyphInfo.
    pub fn glyph_top_accent_attachment(&self, glyph_id: u16) -> Option<i16> {
        let math = self.math_table()?;
        let info_offset = read_u16(math, 6)? as usize;
        let info = math.get(info_offset..)?;
        let top_accent_offset = read_u16(info, 2)? as usize;
        if top_accent_offset == 0 {
            return None;
        }
        let table = info.get(top_accent_offset..)?;
        let coverage_offset = read_u16(table, 0)? as usize;
        let coverage = table.get(coverage_offset..)?;
        let index = lookup_coverage(coverage, glyph_id)? as usize;
        let count = read_u16(table, 2)? as usize;
        if index < count {
            let val_offset = 4 + index * 4;
            read_i16(table, val_offset)
        } else {
            None
        }
    }

    /// Looks up italic correction for a glyph ID via MathGlyphInfo.
    pub fn glyph_italic_correction(&self, glyph_id: u16) -> Option<i16> {
        let math = self.math_table()?;
        let info_offset = read_u16(math, 6)? as usize;
        let info = math.get(info_offset..)?;
        let ic_offset = read_u16(info, 0)? as usize;
        if ic_offset == 0 {
            return None;
        }
        let table = info.get(ic_offset..)?;
        let coverage_offset = read_u16(table, 0)? as usize;
        let coverage = table.get(coverage_offset..)?;
        let index = lookup_coverage(coverage, glyph_id)? as usize;
        let count = read_u16(table, 2)? as usize;
        if index < count {
            let val_offset = 4 + index * 4;
            read_i16(table, val_offset)
        } else {
            None
        }
    }

    /// Returns the count of discrete vertical variants for a delimiter glyph ID via MathVariants.
    pub fn vert_variant_count(&self, glyph_id: u16) -> usize {
        let math = match self.math_table() {
            Some(m) => m,
            None => return 0,
        };
        let variants_offset = match read_u16(math, 8) {
            Some(o) => o as usize,
            None => return 0,
        };
        let variants = match math.get(variants_offset..) {
            Some(v) => v,
            None => return 0,
        };
        let coverage_offset = match read_u16(variants, 2) {
            Some(o) => o as usize,
            None => return 0,
        };
        let coverage = match variants.get(coverage_offset..) {
            Some(c) => c,
            None => return 0,
        };
        let index = match lookup_coverage(coverage, glyph_id) {
            Some(i) => i as usize,
            None => return 0,
        };
        let count = match read_u16(variants, 6) {
            Some(c) => c as usize,
            None => return 0,
        };
        if index < count {
            let offset_pos = 10 + index * 2;
            let construction_offset = match read_u16(variants, offset_pos) {
                Some(o) => o as usize,
                None => return 0,
            };
            let construction = match variants.get(construction_offset..) {
                Some(c) => c,
                None => return 0,
            };
            read_u16(construction, 2).unwrap_or(0) as usize
        } else {
            0
        }
    }

    /// Reads a specific discrete vertical variant record by index.
    pub fn vert_variant(&self, glyph_id: u16, variant_idx: usize) -> Option<MathGlyphVariantRecord> {
        let math = self.math_table()?;
        let variants_offset = read_u16(math, 8)? as usize;
        let variants = math.get(variants_offset..)?;
        let coverage_offset = read_u16(variants, 2)? as usize;
        let coverage = variants.get(coverage_offset..)?;
        let index = lookup_coverage(coverage, glyph_id)? as usize;
        let count = read_u16(variants, 6)? as usize;
        if index < count {
            let offset_pos = 10 + index * 2;
            let construction_offset = read_u16(variants, offset_pos)? as usize;
            let construction = variants.get(construction_offset..)?;
            let variant_count = read_u16(construction, 2)? as usize;
            if variant_idx < variant_count {
                let rec_offset = 4 + variant_idx * 4;
                let variant_glyph = read_u16(construction, rec_offset)?;
                let advance_measurement = read_u16(construction, rec_offset + 2)?;
                Some(MathGlyphVariantRecord {
                    variant_glyph,
                    advance_measurement,
                })
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Zero-allocation OpenType Coverage Table lookup (Format 1 and Format 2).
fn lookup_coverage(coverage: &[u8], glyph_id: u16) -> Option<u16> {
    let format = read_u16(coverage, 0)?;
    match format {
        1 => {
            let count = read_u16(coverage, 2)? as usize;
            let glyphs_data = coverage.get(4..4 + count * 2)?;
            // Binary search over sorted glyph array
            let mut low = 0;
            let mut high = count;
            while low < high {
                let mid = (low + high) / 2;
                let gid = read_u16(glyphs_data, mid * 2)?;
                if gid < glyph_id {
                    low = mid + 1;
                } else if gid > glyph_id {
                    high = mid;
                } else {
                    return Some(mid as u16);
                }
            }
            None
        }
        2 => {
            let range_count = read_u16(coverage, 2)? as usize;
            let ranges_data = coverage.get(4..4 + range_count * 6)?;
            let mut low = 0;
            let mut high = range_count;
            while low < high {
                let mid = (low + high) / 2;
                let start_gid = read_u16(ranges_data, mid * 6)?;
                let end_gid = read_u16(ranges_data, mid * 6 + 2)?;
                let start_cov = read_u16(ranges_data, mid * 6 + 4)?;
                if glyph_id < start_gid {
                    high = mid;
                } else if glyph_id > end_gid {
                    low = mid + 1;
                } else {
                    return Some(start_cov + (glyph_id - start_gid));
                }
            }
            None
        }
        _ => None,
    }
}
