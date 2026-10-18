//! # OpenType MathConstants Specification
//!
//! Authoritative 56-parameter mathematical constants table defined in ISO/IEC 14496-22
//! and Microsoft OpenType specification.
//!
//! Designed with strict hardware alignment:
//! - Exactly 56 parameters (112 bytes) + 16 bytes cache padding = 128 bytes
//! - Aligned to 64 bytes (occupies exactly 2 L1 CPU cache lines with zero split overhead)
//! - 100% stack / rodata resident, zero heap thrashing

/// 56 OpenType MathConstants with hardware L1 cache alignment.
#[repr(C, align(64))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenTypeMathConstants {
    /// Percentage of the parent font size used for Script style (e.g. 70%)
    pub script_percent_scale_down: i16,
    /// Percentage of the parent font size used for ScriptScript style (e.g. 50%)
    pub script_script_percent_scale_down: i16,
    /// Minimum height for delimited sub-formulas
    pub delimited_sub_formula_min_height: u16,
    /// Minimum height for display-style big operators
    pub display_operator_min_height: u16,
    /// Math leading clearance
    pub math_leading: i16,
    /// Distance from the baseline to the mathematical axis
    pub axis_height: i16,
    /// Height of math accents above the baseline
    pub accent_base_height: i16,
    /// Height of flattened math accents
    pub flattened_accent_base_height: i16,
    /// Default subscript drop below baseline
    pub subscript_shift_down: i16,
    /// Maximum distance that subscript top can extend above the baseline
    pub subscript_top_max: i16,
    /// Minimum baseline drop for subscripts
    pub subscript_baseline_drop_min: i16,
    /// Default superscript rise above baseline
    pub superscript_shift_up: i16,
    /// Default superscript rise in cramped styles
    pub superscript_shift_up_cramped: i16,
    /// Minimum distance that superscript bottom must stay above baseline
    pub superscript_bottom_min: i16,
    /// Maximum baseline drop for superscripts
    pub superscript_baseline_drop_max: i16,
    /// Minimum clearance between sub and superscript bounding boxes
    pub sub_superscript_gap_min: i16,
    /// Maximum superscript bottom elevation when a subscript is present
    pub superscript_bottom_max_with_subscript: i16,
    /// Space inserted after scripts
    pub space_after_script: i16,
    /// Minimum gap above a big operator to its upper limit
    pub upper_limit_gap_min: i16,
    /// Minimum rise of upper limit baseline above operator top
    pub upper_limit_baseline_rise_min: i16,
    /// Minimum gap below a big operator to its lower limit
    pub lower_limit_gap_min: i16,
    /// Minimum drop of lower limit baseline below operator bottom
    pub lower_limit_baseline_drop_min: i16,
    /// Default shift of top element in non-display stack
    pub stack_top_shift_up: i16,
    /// Default shift of top element in display stack
    pub stack_top_display_style_shift_up: i16,
    /// Default shift of bottom element in non-display stack
    pub stack_bottom_shift_down: i16,
    /// Default shift of bottom element in display stack
    pub stack_bottom_display_style_shift_down: i16,
    /// Minimum gap between elements in a non-display stack
    pub stack_gap_min: i16,
    /// Minimum gap between elements in a display stack
    pub stack_display_style_gap_min: i16,
    /// Shift of top element in stretch stack
    pub stretch_stack_top_shift_up: i16,
    /// Shift of bottom element in stretch stack
    pub stretch_stack_bottom_shift_down: i16,
    /// Minimum gap above stretch stack base
    pub stretch_stack_gap_above_min: i16,
    /// Minimum gap below stretch stack base
    pub stretch_stack_gap_below_min: i16,
    /// Numerator baseline shift in non-display fractions
    pub fraction_numerator_shift_up: i16,
    /// Numerator baseline shift in display fractions
    pub fraction_numerator_display_style_shift_up: i16,
    /// Denominator baseline shift in non-display fractions
    pub fraction_denominator_shift_down: i16,
    /// Denominator baseline shift in display fractions
    pub fraction_denominator_display_style_shift_down: i16,
    /// Minimum gap between numerator bottom and fraction bar in non-display style
    pub fraction_numerator_gap_min: i16,
    /// Minimum gap between numerator bottom and fraction bar in display style
    pub fraction_num_display_style_gap_min: i16,
    /// Thickness of the fraction rule bar
    pub fraction_rule_thickness: i16,
    /// Minimum gap between denominator top and fraction bar in non-display style
    pub fraction_denominator_gap_min: i16,
    /// Minimum gap between denominator top and fraction bar in display style
    pub fraction_denom_display_style_gap_min: i16,
    /// Horizontal gap for skewed fractions
    pub skewed_fraction_horizontal_gap: i16,
    /// Vertical gap for skewed fractions
    pub skewed_fraction_vertical_gap: i16,
    /// Clearance gap between formula and overbar
    pub overbar_vertical_gap: i16,
    /// Thickness of the overbar rule
    pub overbar_rule_thickness: i16,
    /// Extra ascent added above overbar
    pub overbar_extra_ascender: i16,
    /// Clearance gap between formula and underbar
    pub underbar_vertical_gap: i16,
    /// Thickness of the underbar rule
    pub underbar_rule_thickness: i16,
    /// Extra descent added below underbar
    pub underbar_extra_descender: i16,
    /// Vertical gap between radicand top and radical roof in non-display style
    pub radical_vertical_gap: i16,
    /// Vertical gap between radicand top and radical roof in display style
    pub radical_display_style_vertical_gap: i16,
    /// Thickness of the radical rule
    pub radical_rule_thickness: i16,
    /// Extra ascent added above radical rule
    pub radical_extra_ascender: i16,
    /// Horizontal kerning inserted before root degree index
    pub radical_kern_before_degree: i16,
    /// Horizontal kerning inserted after root degree index
    pub radical_kern_after_degree: i16,
    /// Percentage of radical height to raise degree index bottom
    pub radical_degree_bottom_raise_percent: u16,
    /// Explicit padding to ensure 128-byte alignment (exactly 2 cache lines)
    pub _cache_padding: [u8; 16],
}

const _: () = assert!(core::mem::size_of::<OpenTypeMathConstants>() == 128);
const _: () = assert!(core::mem::align_of::<OpenTypeMathConstants>() == 64);

impl OpenTypeMathConstants {
    /// Constructs constants with zeroed values.
    #[inline(always)]
    pub const fn zero() -> Self {
        Self {
            script_percent_scale_down: 0,
            script_script_percent_scale_down: 0,
            delimited_sub_formula_min_height: 0,
            display_operator_min_height: 0,
            math_leading: 0,
            axis_height: 0,
            accent_base_height: 0,
            flattened_accent_base_height: 0,
            subscript_shift_down: 0,
            subscript_top_max: 0,
            subscript_baseline_drop_min: 0,
            superscript_shift_up: 0,
            superscript_shift_up_cramped: 0,
            superscript_bottom_min: 0,
            superscript_baseline_drop_max: 0,
            sub_superscript_gap_min: 0,
            superscript_bottom_max_with_subscript: 0,
            space_after_script: 0,
            upper_limit_gap_min: 0,
            upper_limit_baseline_rise_min: 0,
            lower_limit_gap_min: 0,
            lower_limit_baseline_drop_min: 0,
            stack_top_shift_up: 0,
            stack_top_display_style_shift_up: 0,
            stack_bottom_shift_down: 0,
            stack_bottom_display_style_shift_down: 0,
            stack_gap_min: 0,
            stack_display_style_gap_min: 0,
            stretch_stack_top_shift_up: 0,
            stretch_stack_bottom_shift_down: 0,
            stretch_stack_gap_above_min: 0,
            stretch_stack_gap_below_min: 0,
            fraction_numerator_shift_up: 0,
            fraction_numerator_display_style_shift_up: 0,
            fraction_denominator_shift_down: 0,
            fraction_denominator_display_style_shift_down: 0,
            fraction_numerator_gap_min: 0,
            fraction_num_display_style_gap_min: 0,
            fraction_rule_thickness: 0,
            fraction_denominator_gap_min: 0,
            fraction_denom_display_style_gap_min: 0,
            skewed_fraction_horizontal_gap: 0,
            skewed_fraction_vertical_gap: 0,
            overbar_vertical_gap: 0,
            overbar_rule_thickness: 0,
            overbar_extra_ascender: 0,
            underbar_vertical_gap: 0,
            underbar_rule_thickness: 0,
            underbar_extra_descender: 0,
            radical_vertical_gap: 0,
            radical_display_style_vertical_gap: 0,
            radical_rule_thickness: 0,
            radical_extra_ascender: 0,
            radical_kern_before_degree: 0,
            radical_kern_after_degree: 0,
            radical_degree_bottom_raise_percent: 0,
            _cache_padding: [0; 16],
        }
    }

    /// Converts a design unit constant to fractional points given font size.
    #[inline(always)]
    pub fn scale(&self, val: i16, font_size: f32) -> f32 {
        (val as f32 * 0.001) * font_size
    }

    /// Converts an unsigned design unit constant to fractional points.
    #[inline(always)]
    pub fn scale_u(&self, val: u16, font_size: f32) -> f32 {
        (val as f32 * 0.001) * font_size
    }
}
