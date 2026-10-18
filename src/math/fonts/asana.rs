//! # Asana Math Static Tables
//!
//! Authoritative OpenType MathConstants extracted directly from `Asana-Math.otf`
//! (OFL-1.1 licensed Palatino/serif mathematical font).
//!
//! Stored in static `.rodata` for $O(1)$ zero-allocation L1 cache residency.

use super::math_constants::OpenTypeMathConstants;

/// Pre-extracted OpenType MathConstants for Asana Math (Palatino serif pairing).
pub static ASANA_MATH_CONSTANTS: OpenTypeMathConstants = OpenTypeMathConstants {
    script_percent_scale_down: 73,
    script_script_percent_scale_down: 60,
    delimited_sub_formula_min_height: 1500,
    display_operator_min_height: 1250,
    math_leading: 150,
    axis_height: 271,
    accent_base_height: 469,
    flattened_accent_base_height: 692,
    subscript_shift_down: 210,
    subscript_top_max: 366,
    subscript_baseline_drop_min: 222,
    superscript_shift_up: 361,
    superscript_shift_up_cramped: 296,
    superscript_bottom_min: 154,
    superscript_baseline_drop_max: 230,
    sub_superscript_gap_min: 145,
    superscript_bottom_max_with_subscript: 379,
    space_after_script: 41,
    upper_limit_gap_min: 100,
    upper_limit_baseline_rise_min: 250,
    lower_limit_gap_min: 120,
    lower_limit_baseline_drop_min: 620,
    stack_top_shift_up: 50,
    stack_top_display_style_shift_up: 50,
    stack_bottom_shift_down: 50,
    stack_bottom_display_style_shift_down: 50,
    stack_gap_min: 348,
    stack_display_style_gap_min: 812,
    stretch_stack_top_shift_up: 50,
    stretch_stack_bottom_shift_down: 80,
    stretch_stack_gap_above_min: 80,
    stretch_stack_gap_below_min: 40,
    fraction_numerator_shift_up: 600,
    fraction_numerator_display_style_shift_up: 725,
    fraction_denominator_shift_down: 500,
    fraction_denominator_display_style_shift_down: 710,
    fraction_numerator_gap_min: 116,
    fraction_num_display_style_gap_min: 130,
    fraction_rule_thickness: 70,
    fraction_denominator_gap_min: 116,
    fraction_denom_display_style_gap_min: 120,
    skewed_fraction_horizontal_gap: 400,
    skewed_fraction_vertical_gap: 60,
    overbar_vertical_gap: 175,
    overbar_rule_thickness: 75,
    overbar_extra_ascender: 75,
    underbar_vertical_gap: 175,
    underbar_rule_thickness: 75,
    underbar_extra_descender: 75,
    radical_vertical_gap: 150,
    radical_display_style_vertical_gap: 161,
    radical_rule_thickness: 59,
    radical_extra_ascender: 59,
    radical_kern_before_degree: 577,
    radical_kern_after_degree: -320,
    radical_degree_bottom_raise_percent: 65,
    _cache_padding: [0; 16],
};

const _: () = assert!(core::mem::size_of::<OpenTypeMathConstants>() == 128);
const _: () = assert!(core::mem::align_of::<OpenTypeMathConstants>() == 64);
