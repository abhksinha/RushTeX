//! # Fira Math Static Tables
//!
//! Authoritative OpenType MathConstants extracted directly from `FiraMath-Regular.otf`
//! (OFL-1.1 licensed sans-serif mathematical font).
//!
//! Stored in static `.rodata` for $O(1)$ zero-allocation L1 cache residency.

use super::math_constants::OpenTypeMathConstants;

/// Pre-extracted OpenType MathConstants for Fira Math (sans-serif pairing for Helvetica).
pub static FIRAMATH_CONSTANTS: OpenTypeMathConstants = OpenTypeMathConstants {
    script_percent_scale_down: 72,
    script_script_percent_scale_down: 58,
    delimited_sub_formula_min_height: 1500,
    display_operator_min_height: 1500,
    math_leading: 150,
    axis_height: 280,
    accent_base_height: 527,
    flattened_accent_base_height: 689,
    subscript_shift_down: 350,
    subscript_top_max: 527,
    subscript_baseline_drop_min: 250,
    superscript_shift_up: 400,
    superscript_shift_up_cramped: 270,
    superscript_bottom_min: 130,
    superscript_baseline_drop_max: 360,
    sub_superscript_gap_min: 200,
    superscript_bottom_max_with_subscript: 527,
    space_after_script: 41,
    upper_limit_gap_min: 150,
    upper_limit_baseline_rise_min: 150,
    lower_limit_gap_min: 150,
    lower_limit_baseline_drop_min: 600,
    stack_top_shift_up: 450,
    stack_top_display_style_shift_up: 580,
    stack_bottom_shift_down: 480,
    stack_bottom_display_style_shift_down: 700,
    stack_gap_min: 200,
    stack_display_style_gap_min: 500,
    stretch_stack_top_shift_up: 300,
    stretch_stack_bottom_shift_down: 600,
    stretch_stack_gap_above_min: 150,
    stretch_stack_gap_below_min: 150,
    fraction_numerator_shift_up: 450,
    fraction_numerator_display_style_shift_up: 580,
    fraction_denominator_shift_down: 480,
    fraction_denominator_display_style_shift_down: 700,
    fraction_numerator_gap_min: 80,
    fraction_num_display_style_gap_min: 200,
    fraction_rule_thickness: 76,
    fraction_denominator_gap_min: 80,
    fraction_denom_display_style_gap_min: 200,
    skewed_fraction_horizontal_gap: 0,
    skewed_fraction_vertical_gap: 0,
    overbar_vertical_gap: 150,
    overbar_rule_thickness: 66,
    overbar_extra_ascender: 50,
    underbar_vertical_gap: 150,
    underbar_rule_thickness: 66,
    underbar_extra_descender: 50,
    radical_vertical_gap: 96,
    radical_display_style_vertical_gap: 142,
    radical_rule_thickness: 76,
    radical_extra_ascender: 76,
    radical_kern_before_degree: 276,
    radical_kern_after_degree: -400,
    radical_degree_bottom_raise_percent: 64,
    _cache_padding: [0; 16],
};

const _: () = assert!(core::mem::size_of::<OpenTypeMathConstants>() == 128);
const _: () = assert!(core::mem::align_of::<OpenTypeMathConstants>() == 64);
