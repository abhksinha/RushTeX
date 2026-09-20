//! # Modern OpenType Mathematical Fonts Architecture
//!
//! Subsystem for high-performance OpenType math font parsing, `MATH` table processing,
//! and modern ISO/IEC 14496-22 layout geometry.
//!
//! Designed under `Resource_Conscious_Software_Engineering.md`:
//! - Pure safe Rust (`#![forbid(unsafe_code)]`)
//! - Hardware L1/L2 cache residency with zero heap thrashing
//! - Fast $O(1)$ pre-computed `.rodata` tables for New Computer Modern Math and STIX Two Math
//! - Zero-allocation dynamic OpenType reader over borrowed byte slices

pub mod engine;
pub mod math_constants;
pub mod math_glyph_info;
pub mod math_variants;
pub mod newcm;
pub mod opentype;
pub mod stix2;

pub use engine::OpenTypeMathEngine;
pub use math_constants::OpenTypeMathConstants;
pub use math_glyph_info::MathGlyphProperties;
pub use math_variants::{GlyphPartRecord, MathGlyphVariantRecord};
pub use newcm::NEWCM_MATH_CONSTANTS;
pub use opentype::OpenTypeFont;
pub use stix2::STIX2_MATH_CONSTANTS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::math_ast::MathStyle;

    #[test]
    fn test_opentype_math_constants_cache_alignment() {
        assert_eq!(core::mem::size_of::<OpenTypeMathConstants>(), 128);
        assert_eq!(core::mem::align_of::<OpenTypeMathConstants>(), 64);
        assert_eq!(core::mem::size_of::<MathGlyphVariantRecord>(), 4);
        assert_eq!(core::mem::size_of::<GlyphPartRecord>(), 12);
        assert_eq!(core::mem::size_of::<MathGlyphProperties>(), 8);
    }

    #[test]
    fn test_newcm_math_constants_values() {
        let c = &NEWCM_MATH_CONSTANTS;
        assert_eq!(c.script_percent_scale_down, 70);
        assert_eq!(c.script_script_percent_scale_down, 50);
        assert_eq!(c.axis_height, 250);
        assert_eq!(c.fraction_rule_thickness, 40);
        assert_eq!(c.fraction_numerator_display_style_shift_up, 677);
        assert_eq!(c.fraction_denominator_display_style_shift_down, 686);
        assert_eq!(c.radical_vertical_gap, 50);
        assert_eq!(c.radical_display_style_vertical_gap, 148);
        assert_eq!(c.radical_degree_bottom_raise_percent, 60);
    }

    #[test]
    fn test_stix2_math_constants_values() {
        let c = &STIX2_MATH_CONSTANTS;
        assert_eq!(c.script_percent_scale_down, 70);
        assert_eq!(c.script_script_percent_scale_down, 55);
        assert_eq!(c.axis_height, 258);
        assert_eq!(c.fraction_rule_thickness, 68);
        assert_eq!(c.fraction_numerator_display_style_shift_up, 640);
    }

    #[test]
    fn test_opentype_engine_geometry() {
        let engine = OpenTypeMathEngine::new(&NEWCM_MATH_CONSTANTS, 10.0);
        assert!((engine.axis_height() - 2.50).abs() < 1e-4);
        assert!((engine.fraction_rule_thickness() - 0.40).abs() < 1e-4);
        assert!((engine.fraction_numerator_shift(true) - 6.77).abs() < 1e-4);
        assert!((engine.fraction_denominator_shift(true) - 6.86).abs() < 1e-4);
        assert!((engine.radical_vertical_gap(false) - 0.50).abs() < 1e-4);
        assert!((engine.radical_vertical_gap(true) - 1.48).abs() < 1e-4);
        assert!((engine.radical_degree_bottom_raise_percent() - 0.60).abs() < 1e-4);
        assert_eq!(engine.style_scale(MathStyle::Display), 1.0);
        assert_eq!(engine.style_scale(MathStyle::Script), 0.70);
        assert_eq!(engine.style_scale(MathStyle::ScriptScript), 0.50);
    }

    #[test]
    fn test_dynamic_otf_binary_parsing_if_available() {
        let path = "/usr/share/texlive/texmf-dist/fonts/opentype/public/newcomputermodern/NewCMMath-Regular.otf";
        if let Ok(bytes) = std::fs::read(path) {
            let font = OpenTypeFont::from_bytes(&bytes).expect("Valid OpenType font");
            assert!(font.math_table().is_some(), "Must have MATH table");
            let parsed_constants = font.parse_math_constants().expect("Must parse MathConstants");
            assert_eq!(parsed_constants.axis_height, NEWCM_MATH_CONSTANTS.axis_height);
            assert_eq!(parsed_constants.fraction_rule_thickness, NEWCM_MATH_CONSTANTS.fraction_rule_thickness);
            assert_eq!(parsed_constants.fraction_numerator_display_style_shift_up, NEWCM_MATH_CONSTANTS.fraction_numerator_display_style_shift_up);
            assert_eq!(parsed_constants.script_percent_scale_down, NEWCM_MATH_CONSTANTS.script_percent_scale_down);
        }
    }
}
