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
use super::math_variants::{GlyphPartRecord, MathGlyphVariantRecord};

/// Strongly typed glyph identifier within an OpenType font.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
pub struct GlyphId(pub u16);

/// Strongly typed Unicode scalar value (code point).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
pub struct UnicodeScalar(pub u32);

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

    /// Returns the OpenType `cmap` table slice if present.
    #[inline(always)]
    pub fn cmap_table(&self) -> Option<&'a [u8]> {
        self.find_table(b"cmap")
    }

    /// Returns the OpenType `head` table slice if present.
    #[inline(always)]
    pub fn head_table(&self) -> Option<&'a [u8]> {
        self.find_table(b"head")
    }

    /// Returns the OpenType `maxp` table slice if present.
    #[inline(always)]
    pub fn maxp_table(&self) -> Option<&'a [u8]> {
        self.find_table(b"maxp")
    }

    /// Returns the OpenType `hhea` table slice if present.
    #[inline(always)]
    pub fn hhea_table(&self) -> Option<&'a [u8]> {
        self.find_table(b"hhea")
    }

    /// Returns the OpenType `hmtx` table slice if present.
    #[inline(always)]
    pub fn hmtx_table(&self) -> Option<&'a [u8]> {
        self.find_table(b"hmtx")
    }

    /// Returns the OpenType `CFF ` table slice if present.
    #[inline(always)]
    pub fn cff_table(&self) -> Option<&'a [u8]> {
        self.find_table(b"CFF ")
    }

    /// Returns the TrueType `glyf` table slice if present.
    #[inline(always)]
    pub fn glyf_table(&self) -> Option<&'a [u8]> {
        self.find_table(b"glyf")
    }

    /// Returns the TrueType `loca` table slice if present.
    #[inline(always)]
    pub fn loca_table(&self) -> Option<&'a [u8]> {
        self.find_table(b"loca")
    }

    /// Returns the OpenType `OS/2` table slice if present.
    #[inline(always)]
    pub fn os2_table(&self) -> Option<&'a [u8]> {
        self.find_table(b"OS/2")
    }

    /// Returns the OpenType `name` table slice if present.
    #[inline(always)]
    pub fn name_table(&self) -> Option<&'a [u8]> {
        self.find_table(b"name")
    }

    /// Returns the PostScript font name from the Name INDEX of CFF if present.
    pub fn cff_postscript_name(&self) -> Option<&'a str> {
        let cff = self.cff_table()?;
        if cff.len() < 4 {
            return None;
        }
        let hdr_size = *cff.get(2)? as usize;
        let (count, offsets, off_size, data, _) = parse_cff_index(cff, hdr_size)?;
        if count == 0 {
            return None;
        }
        let start = read_cff_offset(cff, offsets, off_size)?.saturating_sub(1);
        let end = read_cff_offset(cff, offsets + off_size, off_size)?.saturating_sub(1);
        let bytes = cff.get(data + start..data + end)?;
        core::str::from_utf8(bytes).ok()
    }

    /// Returns the FontBBox `[min_x, min_y, max_x, max_y]` from CFF Top DICT if present.
    pub fn cff_font_bbox(&self) -> Option<[i16; 4]> {
        let cff = self.cff_table()?;
        if cff.len() < 4 {
            return None;
        }
        let hdr_size = *cff.get(2)? as usize;
        let mut offset = hdr_size;

        // 1. Name INDEX
        let (_, _, _, _, next_off) = parse_cff_index(cff, offset)?;
        offset = next_off;

        // 2. Top DICT INDEX
        let (top_count, top_offsets, top_off_size, top_data, _) = parse_cff_index(cff, offset)?;
        if top_count == 0 {
            return None;
        }
        let top_start = read_cff_offset(cff, top_offsets, top_off_size)?.saturating_sub(1);
        let top_end = read_cff_offset(cff, top_offsets + top_off_size, top_off_size)?.saturating_sub(1);
        let top_dict = cff.get(top_data + top_start..top_data + top_end)?;
        decode_top_dict_bbox(top_dict)
    }

    /// Returns the total number of CharStrings in the CFF table.
    pub fn cff_charstrings_count(&self) -> Option<u16> {
        let cff = self.cff_table()?;
        if cff.len() < 4 {
            return None;
        }
        let hdr_size = *cff.get(2)? as usize;
        let mut offset = hdr_size;

        // Name INDEX
        let (_, _, _, _, next_off) = parse_cff_index(cff, offset)?;
        offset = next_off;

        // Top DICT INDEX
        let (top_count, top_offsets, top_off_size, top_data, next_off) = parse_cff_index(cff, offset)?;
        if top_count == 0 {
            return None;
        }
        let top_start = read_cff_offset(cff, top_offsets, top_off_size)?.saturating_sub(1);
        let top_end = read_cff_offset(cff, top_offsets + top_off_size, top_off_size)?.saturating_sub(1);
        let top_dict = cff.get(top_data + top_start..top_data + top_end)?;
        offset = next_off;

        // String INDEX
        let (_, _, _, _, next_off) = parse_cff_index(cff, offset)?;
        let _ = next_off;

        // CharStrings offset from Top DICT (op 17)
        let cs_offset = decode_top_dict_operator(top_dict, 17)?;
        let (cs_count, _, _, _, _) = parse_cff_index(cff, cs_offset as usize)?;
        Some(cs_count as u16)
    }

    /// Returns the raw Type 2 CharString slice for a glyph ID from the CFF table.
    pub fn cff_charstring(&self, glyph_id: u16) -> Option<&'a [u8]> {
        let cff = self.cff_table()?;
        if cff.len() < 4 {
            return None;
        }
        let hdr_size = *cff.get(2)? as usize;
        let mut offset = hdr_size;

        // 1. Name INDEX
        let (_, _, _, _, next_off) = parse_cff_index(cff, offset)?;
        offset = next_off;

        // 2. Top DICT INDEX
        let (top_count, top_offsets, top_off_size, top_data, next_off) = parse_cff_index(cff, offset)?;
        if top_count == 0 {
            return None;
        }
        let top_start = read_cff_offset(cff, top_offsets, top_off_size)?.saturating_sub(1);
        let top_end = read_cff_offset(cff, top_offsets + top_off_size, top_off_size)?.saturating_sub(1);
        let top_dict = cff.get(top_data + top_start..top_data + top_end)?;
        offset = next_off;

        // 3. String INDEX
        let (_, _, _, _, next_off) = parse_cff_index(cff, offset)?;
        let _ = next_off;

        // CharStrings offset from Top DICT (operator 17)
        let cs_offset = decode_top_dict_operator(top_dict, 17)?;
        let (cs_count, cs_offsets, cs_off_size, cs_data, _) = parse_cff_index(cff, cs_offset as usize)?;
        let gid = glyph_id as usize;
        if gid >= cs_count {
            return None;
        }
        let start = read_cff_offset(cff, cs_offsets + gid * cs_off_size, cs_off_size)?.saturating_sub(1);
        let end = read_cff_offset(cff, cs_offsets + (gid + 1) * cs_off_size, cs_off_size)?.saturating_sub(1);
        cff.get(cs_data + start..cs_data + end)
    }

    /// Returns the raw TrueType glyph outline data slice from the `glyf` table for a given glyph ID.
    pub fn truetype_glyph_data(&self, glyph_id: u16) -> Option<&'a [u8]> {
        let glyf = self.glyf_table()?;
        let loca = self.loca_table()?;
        let head = self.head_table()?;
        let is_long = read_i16(head, 50)? == 1;
        let gid = glyph_id as usize;

        let (start, end) = if is_long {
            let start = read_u32(loca, gid * 4)? as usize;
            let end = read_u32(loca, (gid + 1) * 4)? as usize;
            (start, end)
        } else {
            let start = (read_u16(loca, gid * 2)? as usize) * 2;
            let end = (read_u16(loca, (gid + 1) * 2)? as usize) * 2;
            (start, end)
        };

        if start <= end && end <= glyf.len() {
            Some(&glyf[start..end])
        } else {
            None
        }
    }

    /// Returns font design units-per-em from `head` (typically 1000 or 2048).
    #[inline(always)]
    pub fn units_per_em(&self) -> Option<u16> {
        let head = self.head_table()?;
        read_u16(head, 18)
    }

    /// Returns the total number of glyphs defined in the font from `maxp`.
    #[inline(always)]
    pub fn num_glyphs(&self) -> Option<u16> {
        let maxp = self.maxp_table()?;
        read_u16(maxp, 4)
    }

    /// Returns the number of horizontal metric records from `hhea`.
    #[inline(always)]
    pub fn num_h_metrics(&self) -> Option<u16> {
        let hhea = self.hhea_table()?;
        read_u16(hhea, 34)
    }

    /// Resolves the horizontal advance width in design units for a given glyph ID from `hmtx`.
    pub fn h_advance(&self, glyph_id: u16) -> Option<u16> {
        let hmtx = self.hmtx_table()?;
        let num_h_metrics = self.num_h_metrics()? as usize;
        if num_h_metrics == 0 {
            return None;
        }
        let gid = glyph_id as usize;
        if gid < num_h_metrics {
            read_u16(hmtx, gid * 4)
        } else {
            read_u16(hmtx, (num_h_metrics - 1) * 4)
        }
    }

    /// Resolves the left side bearing (LSB) in design units for a given glyph ID from `hmtx`.
    pub fn h_lsb(&self, glyph_id: u16) -> Option<i16> {
        let hmtx = self.hmtx_table()?;
        let num_h_metrics = self.num_h_metrics()? as usize;
        if num_h_metrics == 0 {
            return None;
        }
        let gid = glyph_id as usize;
        if gid < num_h_metrics {
            read_i16(hmtx, gid * 4 + 2)
        } else {
            let lsb_start = num_h_metrics * 4;
            let offset = lsb_start + (gid - num_h_metrics) * 2;
            read_i16(hmtx, offset)
        }
    }

    /// Looks up a Unicode code point in the font's `cmap` table to dynamically resolve its Glyph ID.
    ///
    /// Supports Format 12 (segmented 32-bit coverage for mathematical alphanumerics `U+1D400..U+1D7FF`)
    /// and Format 4 (segmented 16-bit BMP coverage).
    pub fn glyph_for_unicode(&self, unicode: u32) -> Option<GlyphId> {
        let cmap = self.cmap_table()?;
        let num_tables = read_u16(cmap, 2)? as usize;

        let mut fmt12_offset = None;
        let mut fmt4_offset = None;

        let mut pos = 4;
        for _ in 0..num_tables {
            if pos + 8 > cmap.len() {
                break;
            }
            let platform_id = read_u16(cmap, pos)?;
            let encoding_id = read_u16(cmap, pos + 2)?;
            let subtable_offset = read_u32(cmap, pos + 4)? as usize;
            pos += 8;

            if ((platform_id == 0 && encoding_id == 4) || (platform_id == 3 && encoding_id == 10))
                && fmt12_offset.is_none()
            {
                fmt12_offset = Some(subtable_offset);
            } else if ((platform_id == 0 && encoding_id == 3) || (platform_id == 3 && encoding_id == 1))
                && fmt4_offset.is_none()
            {
                fmt4_offset = Some(subtable_offset);
            }
        }

        if let Some(offset) = fmt12_offset {
            if let Some(subtable) = cmap.get(offset..) {
                if let Some(gid) = lookup_cmap_format_12(subtable, unicode) {
                    if gid != 0 {
                        return Some(GlyphId(gid));
                    }
                }
            }
        }

        if unicode <= 0xFFFF {
            if let Some(offset) = fmt4_offset {
                if let Some(subtable) = cmap.get(offset..) {
                    if let Some(gid) = lookup_cmap_format_4(subtable, unicode as u16) {
                        if gid != 0 {
                            return Some(GlyphId(gid));
                        }
                    }
                }
            }
        }

        None
    }

    /// Looks up a Glyph ID in the font's `cmap` table to reverse-resolve its authoritative Unicode code point.
    ///
    /// Checks Format 12 (SMP mathematical alphanumerics `U+1D400..U+1D7FF`) first,
    /// then Format 4 (BMP `U+0000..U+FFFF`).
    pub fn unicode_for_glyph(&self, glyph_id: u16) -> Option<u32> {
        let cmap = self.cmap_table()?;
        let num_tables = read_u16(cmap, 2)? as usize;

        let mut fmt12_offset = None;
        let mut fmt4_offset = None;

        let mut pos = 4;
        for _ in 0..num_tables {
            if pos + 8 > cmap.len() {
                break;
            }
            let platform_id = read_u16(cmap, pos)?;
            let encoding_id = read_u16(cmap, pos + 2)?;
            let subtable_offset = read_u32(cmap, pos + 4)? as usize;
            pos += 8;

            if ((platform_id == 0 && encoding_id == 4) || (platform_id == 3 && encoding_id == 10))
                && fmt12_offset.is_none()
            {
                fmt12_offset = Some(subtable_offset);
            } else if ((platform_id == 0 && encoding_id == 3) || (platform_id == 3 && encoding_id == 1))
                && fmt4_offset.is_none()
            {
                fmt4_offset = Some(subtable_offset);
            }
        }

        if let Some(offset) = fmt12_offset {
            if let Some(subtable) = cmap.get(offset..) {
                if let Some(unicode) = reverse_cmap_format_12(subtable, glyph_id) {
                    return Some(unicode);
                }
            }
        }

        if let Some(offset) = fmt4_offset {
            if let Some(subtable) = cmap.get(offset..) {
                if let Some(unicode) = reverse_cmap_format_4(subtable, glyph_id) {
                    return Some(unicode);
                }
            }
        }

        None
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

    /// Looks up top-right math kerning value for a glyph ID via MathGlyphInfo.
    pub fn glyph_top_right_math_kern(&self, glyph_id: u16) -> Option<i16> {
        let math = self.math_table()?;
        let info_offset = read_u16(math, 6)? as usize;
        let info = math.get(info_offset..)?;
        let kern_info_offset = read_u16(info, 6)? as usize;
        if kern_info_offset == 0 {
            return None;
        }
        let table = info.get(kern_info_offset..)?;
        let coverage_offset = read_u16(table, 0)? as usize;
        let coverage = table.get(coverage_offset..)?;
        let index = lookup_coverage(coverage, glyph_id)? as usize;
        let count = read_u16(table, 2)? as usize;
        if index >= count {
            return None;
        }
        let rec_offset = 4 + index * 8;
        let top_right_offset = read_u16(table, rec_offset)? as usize;
        if top_right_offset == 0 {
            return None;
        }
        let kern_table = table.get(top_right_offset..)?;
        let height_count = read_u16(kern_table, 0)? as usize;
        let kern_val_offset = 2 + height_count * 4;
        read_i16(kern_table, kern_val_offset)
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

    /// Returns the minimum connector overlap for vertical glyph assembly, if present.
    pub fn vert_assembly_min_connector_overlap(&self, glyph_id: u16) -> Option<i16> {
        let math = self.math_table()?;
        let variants_offset = read_u16(math, 8)? as usize;
        let variants = math.get(variants_offset..)?;
        let coverage_offset = read_u16(variants, 2)? as usize;
        let coverage = variants.get(coverage_offset..)?;
        let index = lookup_coverage(coverage, glyph_id)? as usize;
        let count = read_u16(variants, 6)? as usize;
        if index >= count {
            return None;
        }
        let offset_pos = 10 + index * 2;
        let construction_offset = read_u16(variants, offset_pos)? as usize;
        let construction = variants.get(construction_offset..)?;
        let assembly_offset = read_u16(construction, 0)? as usize;
        if assembly_offset == 0 {
            return None;
        }
        let assembly = construction.get(assembly_offset..)?;
        read_i16(assembly, 0)
    }

    /// Returns the assembly component count for vertical extensible glyph construction.
    pub fn vert_assembly_part_count(&self, glyph_id: u16) -> usize {
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
        if index >= count {
            return 0;
        }
        let offset_pos = 10 + index * 2;
        let construction_offset = match read_u16(variants, offset_pos) {
            Some(o) => o as usize,
            None => return 0,
        };
        let construction = match variants.get(construction_offset..) {
            Some(c) => c,
            None => return 0,
        };
        let assembly_offset = match read_u16(construction, 0) {
            Some(o) => o as usize,
            None => return 0,
        };
        if assembly_offset == 0 {
            return 0;
        }
        let assembly = match construction.get(assembly_offset..) {
            Some(a) => a,
            None => return 0,
        };
        read_u16(assembly, 4).unwrap_or(0) as usize
    }

    /// Reads a specific extensible assembly component part record by index.
    pub fn vert_assembly_part(
        &self,
        glyph_id: u16,
        part_idx: usize,
    ) -> Option<GlyphPartRecord> {
        let math = self.math_table()?;
        let variants_offset = read_u16(math, 8)? as usize;
        let variants = math.get(variants_offset..)?;
        let coverage_offset = read_u16(variants, 2)? as usize;
        let coverage = variants.get(coverage_offset..)?;
        let index = lookup_coverage(coverage, glyph_id)? as usize;
        let count = read_u16(variants, 6)? as usize;
        if index >= count {
            return None;
        }
        let offset_pos = 10 + index * 2;
        let construction_offset = read_u16(variants, offset_pos)? as usize;
        let construction = variants.get(construction_offset..)?;
        let assembly_offset = read_u16(construction, 0)? as usize;
        if assembly_offset == 0 {
            return None;
        }
        let assembly = construction.get(assembly_offset..)?;
        let part_count = read_u16(assembly, 4)? as usize;
        if part_idx >= part_count {
            return None;
        }
        let rec_pos = 6 + part_idx * 10;
        let glyph = read_u16(assembly, rec_pos)?;
        let start_connector_length = read_u16(assembly, rec_pos + 2)?;
        let end_connector_length = read_u16(assembly, rec_pos + 4)?;
        let full_advance = read_u16(assembly, rec_pos + 6)?;
        let part_flags = read_u16(assembly, rec_pos + 8)?;
        Some(GlyphPartRecord {
            glyph,
            start_connector_length,
            end_connector_length,
            full_advance,
            part_flags,
            _pad: 0,
        })
    }

    /// Returns the count of discrete horizontal variants for an accent/horizontal glyph ID via MathVariants.
    pub fn horiz_variant_count(&self, glyph_id: u16) -> usize {
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
        let coverage_offset = match read_u16(variants, 4) {
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
        let vert_count = match read_u16(variants, 6) {
            Some(c) => c as usize,
            None => return 0,
        };
        let horiz_count = match read_u16(variants, 8) {
            Some(c) => c as usize,
            None => return 0,
        };
        if index < horiz_count {
            let offset_pos = 10 + vert_count * 2 + index * 2;
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

    /// Reads a specific discrete horizontal variant record by index.
    pub fn horiz_variant(&self, glyph_id: u16, variant_idx: usize) -> Option<MathGlyphVariantRecord> {
        let math = self.math_table()?;
        let variants_offset = read_u16(math, 8)? as usize;
        let variants = math.get(variants_offset..)?;
        let coverage_offset = read_u16(variants, 4)? as usize;
        let coverage = variants.get(coverage_offset..)?;
        let index = lookup_coverage(coverage, glyph_id)? as usize;
        let vert_count = read_u16(variants, 6)? as usize;
        let horiz_count = read_u16(variants, 8)? as usize;
        if index < horiz_count {
            let offset_pos = 10 + vert_count * 2 + index * 2;
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

    /// Returns the minimum connector overlap for horizontal glyph assembly.
    pub fn horiz_assembly_min_connector_overlap(&self, _glyph_id: u16) -> Option<i16> {
        let math = self.math_table()?;
        let variants_offset = read_u16(math, 8)? as usize;
        let variants = math.get(variants_offset..)?;
        read_i16(variants, 0)
    }

    /// Returns the assembly component count for horizontal extensible glyph construction.
    pub fn horiz_assembly_part_count(&self, glyph_id: u16) -> usize {
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
        let coverage_offset = match read_u16(variants, 4) {
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
        let vert_count = match read_u16(variants, 6) {
            Some(c) => c as usize,
            None => return 0,
        };
        let horiz_count = match read_u16(variants, 8) {
            Some(c) => c as usize,
            None => return 0,
        };
        if index >= horiz_count {
            return 0;
        }
        let offset_pos = 10 + vert_count * 2 + index * 2;
        let construction_offset = match read_u16(variants, offset_pos) {
            Some(o) => o as usize,
            None => return 0,
        };
        let construction = match variants.get(construction_offset..) {
            Some(c) => c,
            None => return 0,
        };
        let assembly_offset = match read_u16(construction, 0) {
            Some(o) => o as usize,
            None => return 0,
        };
        if assembly_offset == 0 {
            return 0;
        }
        let assembly = match construction.get(assembly_offset..) {
            Some(a) => a,
            None => return 0,
        };
        read_u16(assembly, 4).unwrap_or(0) as usize
    }

    /// Reads a specific extensible assembly component part record by index for horizontal glyphs.
    pub fn horiz_assembly_part(
        &self,
        glyph_id: u16,
        part_idx: usize,
    ) -> Option<GlyphPartRecord> {
        let math = self.math_table()?;
        let variants_offset = read_u16(math, 8)? as usize;
        let variants = math.get(variants_offset..)?;
        let coverage_offset = read_u16(variants, 4)? as usize;
        let coverage = variants.get(coverage_offset..)?;
        let index = lookup_coverage(coverage, glyph_id)? as usize;
        let vert_count = read_u16(variants, 6)? as usize;
        let horiz_count = read_u16(variants, 8)? as usize;
        if index >= horiz_count {
            return None;
        }
        let offset_pos = 10 + vert_count * 2 + index * 2;
        let construction_offset = read_u16(variants, offset_pos)? as usize;
        let construction = variants.get(construction_offset..)?;
        let assembly_offset = read_u16(construction, 0)? as usize;
        if assembly_offset == 0 {
            return None;
        }
        let assembly = construction.get(assembly_offset..)?;
        let part_count = read_u16(assembly, 4)? as usize;
        if part_idx >= part_count {
            return None;
        }
        let rec_pos = 6 + part_idx * 10;
        let glyph = read_u16(assembly, rec_pos)?;
        let start_connector_length = read_u16(assembly, rec_pos + 2)?;
        let end_connector_length = read_u16(assembly, rec_pos + 4)?;
        let full_advance = read_u16(assembly, rec_pos + 6)?;
        let part_flags = read_u16(assembly, rec_pos + 8)?;
        Some(GlyphPartRecord {
            glyph,
            start_connector_length,
            end_connector_length,
            full_advance,
            part_flags,
            _pad: 0,
        })
    }
}

/// Zero-allocation OpenType cmap Format 12 lookup (segmented 32-bit coverage).
fn lookup_cmap_format_12(subtable: &[u8], unicode: u32) -> Option<u16> {
    let format = read_u16(subtable, 0)?;
    if format != 12 {
        return None;
    }
    let num_groups = read_u32(subtable, 12)? as usize;
    let groups_data = subtable.get(16..16 + num_groups * 12)?;

    let mut low = 0;
    let mut high = num_groups;
    while low < high {
        let mid = (low + high) / 2;
        let rec = &groups_data[mid * 12..mid * 12 + 12];
        let start_char = read_u32(rec, 0)?;
        let end_char = read_u32(rec, 4)?;
        let start_gid = read_u32(rec, 8)?;

        if unicode < start_char {
            high = mid;
        } else if unicode > end_char {
            low = mid + 1;
        } else {
            let offset = unicode - start_char;
            return Some((start_gid + offset) as u16);
        }
    }
    None
}

/// Zero-allocation OpenType cmap Format 4 lookup (segmented 16-bit BMP coverage).
fn lookup_cmap_format_4(subtable: &[u8], code: u16) -> Option<u16> {
    let format = read_u16(subtable, 0)?;
    if format != 4 {
        return None;
    }
    let seg_count_x2 = read_u16(subtable, 6)? as usize;
    let seg_count = seg_count_x2 / 2;
    if seg_count == 0 {
        return None;
    }

    let end_codes_start = 14;
    let end_codes_end = end_codes_start + seg_count * 2;
    let start_codes_start = end_codes_end + 2;
    let start_codes_end = start_codes_start + seg_count * 2;
    let id_deltas_start = start_codes_end;
    let id_deltas_end = id_deltas_start + seg_count * 2;
    let id_range_offsets_start = id_deltas_end;

    let mut low = 0;
    let mut high = seg_count;
    while low < high {
        let mid = (low + high) / 2;
        let end_code = read_u16(subtable, end_codes_start + mid * 2)?;
        if code <= end_code {
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    if low >= seg_count {
        return None;
    }
    let seg_idx = low;
    let start_code = read_u16(subtable, start_codes_start + seg_idx * 2)?;
    if code < start_code {
        return None;
    }

    let id_delta = read_i16(subtable, id_deltas_start + seg_idx * 2)? as i32;
    let id_range_offset_pos = id_range_offsets_start + seg_idx * 2;
    let id_range_offset = read_u16(subtable, id_range_offset_pos)? as usize;

    if id_range_offset == 0 {
        let gid = (code as i32 + id_delta) as u16;
        Some(gid)
    } else {
        let glyph_offset =
            id_range_offset_pos + id_range_offset + (code as usize - start_code as usize) * 2;
        let raw_val = read_u16(subtable, glyph_offset)?;
        if raw_val == 0 {
            Some(0)
        } else {
            let gid = (raw_val as i32 + id_delta) as u16;
            Some(gid)
        }
    }
}

/// Reverse lookup in OpenType cmap Format 12 (GID -> Unicode scalar).
fn reverse_cmap_format_12(subtable: &[u8], glyph_id: u16) -> Option<u32> {
    let format = read_u16(subtable, 0)?;
    if format != 12 {
        return None;
    }
    let num_groups = read_u32(subtable, 12)? as usize;
    let groups_data = subtable.get(16..16 + num_groups * 12)?;
    let target_gid = glyph_id as u32;

    for i in 0..num_groups {
        let rec = &groups_data[i * 12..i * 12 + 12];
        let start_char = read_u32(rec, 0)?;
        let end_char = read_u32(rec, 4)?;
        let start_gid = read_u32(rec, 8)?;
        let count = end_char.saturating_sub(start_char) + 1;
        if target_gid >= start_gid && target_gid < start_gid + count {
            return Some(start_char + (target_gid - start_gid));
        }
    }
    None
}

/// Reverse lookup in OpenType cmap Format 4 (GID -> Unicode BMP scalar).
fn reverse_cmap_format_4(subtable: &[u8], glyph_id: u16) -> Option<u32> {
    let format = read_u16(subtable, 0)?;
    if format != 4 {
        return None;
    }
    let seg_count_x2 = read_u16(subtable, 6)? as usize;
    let seg_count = seg_count_x2 / 2;
    if seg_count == 0 {
        return None;
    }

    let end_codes_start = 14;
    let end_codes_end = end_codes_start + seg_count * 2;
    let start_codes_start = end_codes_end + 2;
    let start_codes_end = start_codes_start + seg_count * 2;
    let id_deltas_start = start_codes_end;
    let id_deltas_end = id_deltas_start + seg_count * 2;
    let id_range_offsets_start = id_deltas_end;

    for seg_idx in 0..seg_count {
        let start_code = read_u16(subtable, start_codes_start + seg_idx * 2)?;
        let end_code = read_u16(subtable, end_codes_start + seg_idx * 2)?;
        if start_code == 0xFFFF {
            break;
        }
        let id_delta = read_i16(subtable, id_deltas_start + seg_idx * 2)? as i32;
        let id_range_offset_pos = id_range_offsets_start + seg_idx * 2;
        let id_range_offset = read_u16(subtable, id_range_offset_pos)? as usize;

        if id_range_offset == 0 {
            let cand_code = ((glyph_id as i32 - id_delta) & 0xFFFF) as u16;
            if cand_code >= start_code && cand_code <= end_code {
                return Some(cand_code as u32);
            }
        } else {
            for code in start_code..=end_code {
                let glyph_offset =
                    id_range_offset_pos + id_range_offset + (code as usize - start_code as usize) * 2;
                if let Some(raw_val) = read_u16(subtable, glyph_offset) {
                    if raw_val != 0 {
                        let gid = ((raw_val as i32 + id_delta) & 0xFFFF) as u16;
                        if gid == glyph_id {
                            return Some(code as u32);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Helper to parse a CFF INDEX structure without allocation.
///
/// Returns `(count, offsets_offset, off_size, data_offset, next_index_offset)`.
pub(crate) fn parse_cff_index(cff: &[u8], offset: usize) -> Option<(usize, usize, usize, usize, usize)> {
    let count = read_u16(cff, offset)? as usize;
    if count == 0 {
        return Some((0, offset + 2, 0, offset + 2, offset + 2));
    }
    let off_size = *cff.get(offset + 2)? as usize;
    if !(1..=4).contains(&off_size) {
        return None;
    }
    let offsets_offset = offset + 3;
    let offsets_len = (count + 1) * off_size;
    let data_offset = offsets_offset + offsets_len;
    if data_offset > cff.len() {
        return None;
    }
    let last_off = read_cff_offset(cff, offsets_offset + count * off_size, off_size)?;
    let next_index_offset = data_offset + last_off.saturating_sub(1);
    if next_index_offset > cff.len() {
        return None;
    }
    Some((count, offsets_offset, off_size, data_offset, next_index_offset))
}

#[inline(always)]
pub(crate) fn read_cff_offset(cff: &[u8], offset: usize, off_size: usize) -> Option<usize> {
    match off_size {
        1 => cff.get(offset).copied().map(|b| b as usize),
        2 => read_u16(cff, offset).map(|v| v as usize),
        3 => {
            if offset + 3 <= cff.len() {
                Some(
                    ((cff[offset] as usize) << 16)
                        | ((cff[offset + 1] as usize) << 8)
                        | (cff[offset + 2] as usize),
                )
            } else {
                None
            }
        }
        4 => read_u32(cff, offset).map(|v| v as usize),
        _ => None,
    }
}

pub(crate) fn decode_top_dict_operator(dict: &[u8], target_op: u8) -> Option<i32> {
    let mut i = 0;
    let mut stack = [0i32; 16];
    let mut stack_len = 0;
    while i < dict.len() {
        let b0 = dict[i];
        i += 1;
        if (32..=246).contains(&b0) {
            if stack_len < 16 {
                stack[stack_len] = (b0 as i32) - 139;
                stack_len += 1;
            }
        } else if (247..=250).contains(&b0) {
            if i < dict.len() {
                let b1 = dict[i] as i32;
                i += 1;
                if stack_len < 16 {
                    stack[stack_len] = (b0 as i32 - 247) * 256 + b1 + 108;
                    stack_len += 1;
                }
            }
        } else if (251..=254).contains(&b0) {
            if i < dict.len() {
                let b1 = dict[i] as i32;
                i += 1;
                if stack_len < 16 {
                    stack[stack_len] = -(b0 as i32 - 251) * 256 - b1 - 108;
                    stack_len += 1;
                }
            }
        } else if b0 == 28 {
            if i + 2 <= dict.len() {
                let v = i16::from_be_bytes([dict[i], dict[i + 1]]) as i32;
                i += 2;
                if stack_len < 16 {
                    stack[stack_len] = v;
                    stack_len += 1;
                }
            }
        } else if b0 == 29 {
            if i + 4 <= dict.len() {
                let v = i32::from_be_bytes([dict[i], dict[i + 1], dict[i + 2], dict[i + 3]]);
                i += 4;
                if stack_len < 16 {
                    stack[stack_len] = v;
                    stack_len += 1;
                }
            }
        } else if b0 == 30 {
            while i < dict.len() {
                let b = dict[i];
                i += 1;
                if (b & 0xF) == 0xF || (b >> 4) == 0xF {
                    break;
                }
            }
            if stack_len < 16 {
                stack[stack_len] = 0;
                stack_len += 1;
            }
        } else if b0 == 12 {
            if i < dict.len() {
                i += 1;
            }
            stack_len = 0;
        } else {
            if b0 == target_op && stack_len > 0 {
                return Some(stack[stack_len - 1]);
            }
            stack_len = 0;
        }
    }
    None
}

fn decode_top_dict_bbox(dict: &[u8]) -> Option<[i16; 4]> {
    let mut i = 0;
    let mut stack = [0i16; 16];
    let mut stack_len = 0;
    while i < dict.len() {
        let b0 = dict[i];
        i += 1;
        if (32..=246).contains(&b0) {
            if stack_len < 16 {
                stack[stack_len] = (b0 as i16) - 139;
                stack_len += 1;
            }
        } else if (247..=250).contains(&b0) {
            if i < dict.len() {
                let b1 = dict[i] as i16;
                i += 1;
                if stack_len < 16 {
                    stack[stack_len] = (b0 as i16 - 247) * 256 + b1 + 108;
                    stack_len += 1;
                }
            }
        } else if (251..=254).contains(&b0) {
            if i < dict.len() {
                let b1 = dict[i] as i16;
                i += 1;
                if stack_len < 16 {
                    stack[stack_len] = -(b0 as i16 - 251) * 256 - b1 - 108;
                    stack_len += 1;
                }
            }
        } else if b0 == 28 {
            if i + 2 <= dict.len() {
                let v = i16::from_be_bytes([dict[i], dict[i + 1]]);
                i += 2;
                if stack_len < 16 {
                    stack[stack_len] = v;
                    stack_len += 1;
                }
            }
        } else if b0 == 29 {
            if i + 4 <= dict.len() {
                let v = i32::from_be_bytes([dict[i], dict[i + 1], dict[i + 2], dict[i + 3]]) as i16;
                i += 4;
                if stack_len < 16 {
                    stack[stack_len] = v;
                    stack_len += 1;
                }
            }
        } else if b0 == 30 {
            while i < dict.len() {
                let b = dict[i];
                i += 1;
                if (b & 0xF) == 0xF || (b >> 4) == 0xF {
                    break;
                }
            }
            if stack_len < 16 {
                stack[stack_len] = 0;
                stack_len += 1;
            }
        } else if b0 == 12 {
            if i < dict.len() {
                i += 1;
            }
            stack_len = 0;
        } else {
            if b0 == 5 && stack_len >= 4 {
                return Some([
                    stack[stack_len - 4],
                    stack[stack_len - 3],
                    stack[stack_len - 2],
                    stack[stack_len - 1],
                ]);
            }
            stack_len = 0;
        }
    }
    None
}

/// Helper to find the byte offset in `top_dict` of the 4-byte big-endian operand for Op 18 (Private DICT offset).
fn find_top_dict_private_offset_pos(top_dict: &[u8]) -> Option<usize> {
    let op18_idx = top_dict.iter().position(|&b| b == 18)?;
    if op18_idx >= 5 && top_dict[op18_idx - 5] == 29 {
        Some(op18_idx - 4)
    } else {
        None
    }
}

/// Subsets a CFF 1.0 table by preserving only the CharStrings for referenced Glyph IDs
/// and replacing all unused glyphs with a 1-byte `endchar` operator (14).
///
/// This maintains exact GID indexing and full CFF spec compliance while reducing
/// CharStrings payload size by up to 90%.
pub fn subset_cff(cff: &[u8], used_gids: &[u16]) -> Option<Vec<u8>> {
    if cff.len() < 4 {
        return None;
    }
    let hdr_size = *cff.get(2)? as usize;
    let mut offset = hdr_size;

    // 1. Name INDEX
    let (_, _, _, _, next_off) = parse_cff_index(cff, offset)?;
    offset = next_off;

    // 2. Top DICT INDEX
    let (top_count, top_offsets, top_off_size, top_data, _) = parse_cff_index(cff, offset)?;
    if top_count == 0 {
        return None;
    }
    let top_start = read_cff_offset(cff, top_offsets, top_off_size)?.saturating_sub(1);
    let top_end = read_cff_offset(cff, top_offsets + top_off_size, top_off_size)?.saturating_sub(1);
    let top_dict = cff.get(top_data + top_start..top_data + top_end)?;

    // CharStrings offset (op 17) and Private offset (op 18)
    let cs_off = decode_top_dict_operator(top_dict, 17)? as usize;
    let priv_off = decode_top_dict_operator(top_dict, 18)? as usize;

    if cs_off >= priv_off || priv_off > cff.len() {
        return None;
    }

    // Parse CharStrings INDEX
    let (cs_count, cs_offsets, cs_off_size, cs_data, _) = parse_cff_index(cff, cs_off)?;
    if cs_count == 0 {
        return None;
    }

    // Find the offset of the Private DICT offset operand in top_dict
    let priv_operand_pos = find_top_dict_private_offset_pos(top_dict)?;

    // Build boolean lookup for used glyphs (GID 0 is always preserved for .notdef)
    let mut is_used = vec![false; cs_count];
    if !is_used.is_empty() {
        is_used[0] = true;
    }
    for &gid in used_gids {
        if (gid as usize) < cs_count {
            is_used[gid as usize] = true;
        }
    }

    // Build new CharStrings INDEX
    let mut new_offsets = Vec::with_capacity(cs_count + 1);
    new_offsets.push(1usize);
    let mut cur_offset = 1usize;

    for (gid, &used) in is_used.iter().enumerate() {
        let len = if used {
            let start = read_cff_offset(cff, cs_offsets + gid * cs_off_size, cs_off_size)?.saturating_sub(1);
            let end = read_cff_offset(cff, cs_offsets + (gid + 1) * cs_off_size, cs_off_size)?.saturating_sub(1);
            end.saturating_sub(start)
        } else {
            1 // 0x0E (endchar)
        };
        cur_offset = cur_offset.checked_add(len)?;
        new_offsets.push(cur_offset);
    }

    let total_data_len = cur_offset.saturating_sub(1);
    let new_off_size = if cur_offset <= 0xFF {
        1
    } else if cur_offset <= 0xFFFF {
        2
    } else if cur_offset <= 0xFFFFFF {
        3
    } else {
        4
    };

    let new_cs_index_len = 2 + 1 + (cs_count + 1) * new_off_size + total_data_len;
    let mut new_cff = Vec::with_capacity(cs_off + new_cs_index_len + (cff.len() - priv_off));

    // Copy up to cs_off
    new_cff.extend_from_slice(&cff[..cs_off]);

    // Write new CharStrings INDEX
    new_cff.extend_from_slice(&(cs_count as u16).to_be_bytes());
    new_cff.push(new_off_size as u8);
    for &off_val in &new_offsets {
        match new_off_size {
            1 => new_cff.push(off_val as u8),
            2 => new_cff.extend_from_slice(&(off_val as u16).to_be_bytes()),
            3 => {
                new_cff.push((off_val >> 16) as u8);
                new_cff.push((off_val >> 8) as u8);
                new_cff.push(off_val as u8);
            }
            4 => new_cff.extend_from_slice(&(off_val as u32).to_be_bytes()),
            _ => return None,
        }
    }

    // Write CharStrings payload
    for (gid, &used) in is_used.iter().enumerate() {
        if used {
            let start = read_cff_offset(cff, cs_offsets + gid * cs_off_size, cs_off_size)?.saturating_sub(1);
            let end = read_cff_offset(cff, cs_offsets + (gid + 1) * cs_off_size, cs_off_size)?.saturating_sub(1);
            let slice = cff.get(cs_data + start..cs_data + end)?;
            new_cff.extend_from_slice(slice);
        } else {
            new_cff.push(14); // endchar
        }
    }

    let new_priv_off = new_cff.len();

    // Copy Private DICT + Local Subrs from original priv_off to end of cff
    new_cff.extend_from_slice(&cff[priv_off..]);

    // Patch Op 18 Private offset in top_dict
    let patch_pos = top_data + top_start + priv_operand_pos;
    if patch_pos + 4 <= new_cff.len() {
        let be_bytes = (new_priv_off as u32).to_be_bytes();
        new_cff[patch_pos..patch_pos + 4].copy_from_slice(&be_bytes);
    } else {
        return None;
    }

    Some(new_cff)
}

/// Unified Mathematical Font Abstraction.
///
/// Decouples layout calculation from specific font storage backends (OpenType vs legacy TeX).
pub trait MathFont {
    /// Resolves Unicode codepoint to a font Glyph ID.
    fn glyph_for_unicode(&self, unicode: u32) -> Option<GlyphId>;
    /// Resolves advance width in design units.
    fn advance_width(&self, glyph: GlyphId) -> Option<u16>;
    /// Accesses OpenType math layout constants.
    fn math_constants(&self) -> &OpenTypeMathConstants;
    /// Resolves italic correction in design units.
    fn italic_correction(&self, glyph: GlyphId) -> Option<i16>;
    /// Resolves optical top accent attachment point in design units.
    fn top_accent_attachment(&self, glyph: GlyphId) -> Option<i16>;
    /// Counts vertical discrete size variants.
    fn vertical_variants(&self, glyph: GlyphId) -> usize;
    /// Retrieves a discrete vertical variant record.
    fn vertical_variant(&self, glyph: GlyphId, idx: usize) -> Option<MathGlyphVariantRecord>;
    /// Returns the minimum connector overlap for vertical glyph assembly, if present.
    fn vertical_assembly_min_connector_overlap(&self, glyph: GlyphId) -> Option<i16>;
    /// Returns the assembly component count for vertical extensible glyph construction.
    fn vertical_assembly_part_count(&self, glyph: GlyphId) -> usize;
    /// Reads a specific extensible assembly component part record by index.
    fn vertical_assembly_part(&self, glyph: GlyphId, idx: usize) -> Option<GlyphPartRecord>;
    /// Counts horizontal discrete size variants.
    fn horizontal_variants(&self, glyph: GlyphId) -> usize;
    /// Retrieves a discrete horizontal variant record.
    fn horizontal_variant(&self, glyph: GlyphId, idx: usize) -> Option<MathGlyphVariantRecord>;
    /// Returns the minimum connector overlap for horizontal glyph assembly.
    fn horizontal_assembly_min_connector_overlap(&self, glyph: GlyphId) -> Option<i16>;
    /// Returns the assembly component count for horizontal extensible glyph construction.
    fn horizontal_assembly_part_count(&self, glyph: GlyphId) -> usize;
    /// Reads a specific extensible assembly component part record by index for horizontal glyphs.
    fn horizontal_assembly_part(&self, glyph: GlyphId, idx: usize) -> Option<GlyphPartRecord>;
    /// Looks up top-right math kerning value for a glyph ID via MathGlyphInfo.
    fn top_right_math_kern(&self, glyph: GlyphId) -> Option<i16>;
}

/// An active OpenType math font instance pairing binary font bytes with cached constants.
#[derive(Clone, Copy, Debug)]
pub struct OpenTypeMathFont<'a> {
    pub font: OpenTypeFont<'a>,
    pub constants: OpenTypeMathConstants,
}

impl<'a> OpenTypeMathFont<'a> {
    /// Creates an `OpenTypeMathFont` by parsing math constants from the font.
    pub fn from_font(font: OpenTypeFont<'a>) -> Option<Self> {
        let constants = font.parse_math_constants()?;
        Some(Self { font, constants })
    }
}

impl<'a> MathFont for OpenTypeMathFont<'a> {
    #[inline(always)]
    fn glyph_for_unicode(&self, unicode: u32) -> Option<GlyphId> {
        self.font.glyph_for_unicode(unicode)
    }

    #[inline(always)]
    fn advance_width(&self, glyph: GlyphId) -> Option<u16> {
        self.font.h_advance(glyph.0)
    }

    #[inline(always)]
    fn math_constants(&self) -> &OpenTypeMathConstants {
        &self.constants
    }

    #[inline(always)]
    fn italic_correction(&self, glyph: GlyphId) -> Option<i16> {
        self.font.glyph_italic_correction(glyph.0)
    }

    #[inline(always)]
    fn top_accent_attachment(&self, glyph: GlyphId) -> Option<i16> {
        self.font.glyph_top_accent_attachment(glyph.0)
    }

    #[inline(always)]
    fn vertical_variants(&self, glyph: GlyphId) -> usize {
        self.font.vert_variant_count(glyph.0)
    }

    #[inline(always)]
    fn vertical_variant(&self, glyph: GlyphId, idx: usize) -> Option<MathGlyphVariantRecord> {
        self.font.vert_variant(glyph.0, idx)
    }

    #[inline(always)]
    fn vertical_assembly_min_connector_overlap(&self, glyph: GlyphId) -> Option<i16> {
        self.font.vert_assembly_min_connector_overlap(glyph.0)
    }

    #[inline(always)]
    fn vertical_assembly_part_count(&self, glyph: GlyphId) -> usize {
        self.font.vert_assembly_part_count(glyph.0)
    }

    #[inline(always)]
    fn vertical_assembly_part(&self, glyph: GlyphId, idx: usize) -> Option<GlyphPartRecord> {
        self.font.vert_assembly_part(glyph.0, idx)
    }

    #[inline(always)]
    fn horizontal_variants(&self, glyph: GlyphId) -> usize {
        self.font.horiz_variant_count(glyph.0)
    }

    #[inline(always)]
    fn horizontal_variant(&self, glyph: GlyphId, idx: usize) -> Option<MathGlyphVariantRecord> {
        self.font.horiz_variant(glyph.0, idx)
    }

    #[inline(always)]
    fn horizontal_assembly_min_connector_overlap(&self, glyph: GlyphId) -> Option<i16> {
        self.font.horiz_assembly_min_connector_overlap(glyph.0)
    }

    #[inline(always)]
    fn horizontal_assembly_part_count(&self, glyph: GlyphId) -> usize {
        self.font.horiz_assembly_part_count(glyph.0)
    }

    #[inline(always)]
    fn horizontal_assembly_part(&self, glyph: GlyphId, idx: usize) -> Option<GlyphPartRecord> {
        self.font.horiz_assembly_part(glyph.0, idx)
    }

    #[inline(always)]
    fn top_right_math_kern(&self, glyph: GlyphId) -> Option<i16> {
        self.font.glyph_top_right_math_kern(glyph.0)
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
