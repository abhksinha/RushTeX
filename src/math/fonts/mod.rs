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

pub mod asana;
pub mod calligraphy;
pub mod delimiters;
pub mod engine;
pub mod firamath;
pub mod math_constants;
pub mod math_glyph_info;
pub mod math_variants;
pub mod newcm;
pub mod opentype;
pub mod operators;
pub mod stix2;
pub mod strokes;
pub mod vector_paths;

pub use asana::ASANA_MATH_CONSTANTS;
pub use calligraphy::{
    script_glyph, symbol_to_unicode_greek, to_unicode_blackboard, to_unicode_bold, to_unicode_bold_italic,
    to_unicode_fraktur, to_unicode_script, NEWCM_SCRIPT_GLYPHS, STIX2_SCRIPT_GLYPHS, OpenTypeScriptGlyph,
    static_cff_compressed, static_cff_raw, static_script_advance,
};
pub use vector_paths::{get_vector_glyph_path, vector_glyph_for_unicode};
pub use delimiters::{
    OpenTypeAssemblyRecipe, OpenTypeDelimKind, OpenTypeHorizAssemblyRecipe, OpenTypeHorizKind,
    ResolvedDelimiter, ResolvedHorizontal, NEWCM_BAR_VARIANTS, NEWCM_BRACE_VARIANTS,
    NEWCM_BRACKET_VARIANTS, NEWCM_PAREN_VARIANTS, NEWCM_RADICAL_VARIANTS,
};
pub use engine::OpenTypeMathEngine;
pub use firamath::FIRAMATH_CONSTANTS;
pub use math_constants::OpenTypeMathConstants;
pub use math_glyph_info::MathGlyphProperties;
pub use math_variants::{GlyphPartRecord, MathGlyphVariantRecord};
pub use newcm::NEWCM_MATH_CONSTANTS;
pub use opentype::{GlyphId, MathFont, OpenTypeFont, OpenTypeMathFont, UnicodeScalar};
pub use operators::{OpenTypeBigOpKind, OpenTypeOperatorMetrics, ResolvedBigOp, NEWCM_OPERATOR_METRICS};
pub use stix2::STIX2_MATH_CONSTANTS;
pub use strokes::{CalligraphicNib, CalligraphicStrokeBuffer, PointKind, StrokePoint};

use std::sync::OnceLock;

static NEWCM_MATH_FONT_BYTES: OnceLock<Option<Vec<u8>>> = OnceLock::new();
static STIX2_MATH_FONT_BYTES: OnceLock<Option<Vec<u8>>> = OnceLock::new();
static FIRAMATH_FONT_BYTES: OnceLock<Option<Vec<u8>>> = OnceLock::new();
static ASANA_MATH_FONT_BYTES: OnceLock<Option<Vec<u8>>> = OnceLock::new();

/// Resolves the New Computer Modern Math binary font, cached globally across invocations.
pub fn get_newcm_math_font() -> Option<OpenTypeFont<'static>> {
    let bytes = NEWCM_MATH_FONT_BYTES.get_or_init(|| {
        let candidates = [
            "/usr/share/texlive/texmf-dist/fonts/opentype/public/newcomputermodern/NewCMMath-Regular.otf",
            "/usr/share/fonts/opentype/newcomputermodern/NewCMMath-Regular.otf",
        ];
        for path in candidates {
            if let Ok(b) = std::fs::read(path) {
                return Some(b);
            }
        }
        None
    });
    bytes.as_ref().and_then(|b| OpenTypeFont::from_bytes(b))
}

/// Resolves the STIX Two Math binary font, cached globally across invocations.
pub fn get_stix2_math_font() -> Option<OpenTypeFont<'static>> {
    let bytes = STIX2_MATH_FONT_BYTES.get_or_init(|| {
        let candidates = [
            "/usr/share/texlive/texmf-dist/fonts/opentype/public/stix2-otf/STIXTwoMath-Regular.otf",
            "/usr/share/fonts/opentype/stix/STIXTwoMath-Regular.otf",
            "/usr/share/fonts/opentype/stix-word/STIXMath-Regular.otf",
        ];
        for path in candidates {
            if let Ok(b) = std::fs::read(path) {
                return Some(b);
            }
        }
        None
    });
    bytes.as_ref().and_then(|b| OpenTypeFont::from_bytes(b))
}

/// Resolves the Fira Math binary font, cached globally across invocations.
pub fn get_firamath_font() -> Option<OpenTypeFont<'static>> {
    let bytes = FIRAMATH_FONT_BYTES.get_or_init(|| {
        let candidates = [
            "/usr/share/texlive/texmf-dist/fonts/opentype/public/firamath/FiraMath-Regular.otf",
            "/usr/share/fonts/opentype/firamath/FiraMath-Regular.otf",
        ];
        for path in candidates {
            if let Ok(b) = std::fs::read(path) {
                return Some(b);
            }
        }
        None
    });
    bytes.as_ref().and_then(|b| OpenTypeFont::from_bytes(b))
}

/// Resolves the Asana Math binary font, cached globally across invocations.
pub fn get_asana_math_font() -> Option<OpenTypeFont<'static>> {
    let bytes = ASANA_MATH_FONT_BYTES.get_or_init(|| {
        let candidates = [
            "/usr/share/texlive/texmf-dist/fonts/opentype/public/Asana-Math/Asana-Math.otf",
            "/usr/share/fonts/opentype/asana-math/Asana-Math.otf",
        ];
        for path in candidates {
            if let Ok(b) = std::fs::read(path) {
                return Some(b);
            }
        }
        None
    });
    bytes.as_ref().and_then(|b| OpenTypeFont::from_bytes(b))
}

/// Dynamically resolves the active OpenType math font instance for the specified font family.
pub fn active_math_font(family: crate::backend::fonts::FontFamily) -> Option<OpenTypeFont<'static>> {
    match family {
        crate::backend::fonts::FontFamily::Times => {
            get_stix2_math_font().or_else(get_newcm_math_font)
        }
        crate::backend::fonts::FontFamily::Helvetica => {
            get_firamath_font().or_else(get_newcm_math_font)
        }
        crate::backend::fonts::FontFamily::Palatino => {
            get_asana_math_font().or_else(get_newcm_math_font)
        }
        _ => get_newcm_math_font(),
    }
}

static NEWCM_MATH_INSTANCE: OnceLock<Option<OpenTypeMathFont<'static>>> = OnceLock::new();
static STIX2_MATH_INSTANCE: OnceLock<Option<OpenTypeMathFont<'static>>> = OnceLock::new();
static FIRAMATH_INSTANCE: OnceLock<Option<OpenTypeMathFont<'static>>> = OnceLock::new();
static ASANA_MATH_INSTANCE: OnceLock<Option<OpenTypeMathFont<'static>>> = OnceLock::new();

/// Returns the statically precomputed OpenType math constants for the given font family directly from .rodata.
#[inline(always)]
pub fn math_constants_for_family(family: crate::backend::fonts::FontFamily) -> &'static OpenTypeMathConstants {
    match family {
        crate::backend::fonts::FontFamily::Times => &STIX2_MATH_CONSTANTS,
        crate::backend::fonts::FontFamily::Helvetica => &FIRAMATH_CONSTANTS,
        crate::backend::fonts::FontFamily::Palatino => &ASANA_MATH_CONSTANTS,
        _ => &NEWCM_MATH_CONSTANTS,
    }
}

/// Resolves the cached `OpenTypeMathFont` instance with pre-parsed constants.
pub fn active_opentype_math_font(family: crate::backend::fonts::FontFamily) -> Option<&'static OpenTypeMathFont<'static>> {
    match family {
        crate::backend::fonts::FontFamily::Times => {
            let stix = STIX2_MATH_INSTANCE.get_or_init(|| {
                get_stix2_math_font().and_then(OpenTypeMathFont::from_font)
            });
            if stix.is_some() {
                return stix.as_ref();
            }
            let newcm = NEWCM_MATH_INSTANCE.get_or_init(|| {
                get_newcm_math_font().and_then(OpenTypeMathFont::from_font)
            });
            newcm.as_ref()
        }
        crate::backend::fonts::FontFamily::Helvetica => {
            let fira = FIRAMATH_INSTANCE.get_or_init(|| {
                get_firamath_font().and_then(OpenTypeMathFont::from_font)
            });
            if fira.is_some() {
                return fira.as_ref();
            }
            let newcm = NEWCM_MATH_INSTANCE.get_or_init(|| {
                get_newcm_math_font().and_then(OpenTypeMathFont::from_font)
            });
            newcm.as_ref()
        }
        crate::backend::fonts::FontFamily::Palatino => {
            let asana = ASANA_MATH_INSTANCE.get_or_init(|| {
                get_asana_math_font().and_then(OpenTypeMathFont::from_font)
            });
            if asana.is_some() {
                return asana.as_ref();
            }
            let newcm = NEWCM_MATH_INSTANCE.get_or_init(|| {
                get_newcm_math_font().and_then(OpenTypeMathFont::from_font)
            });
            newcm.as_ref()
        }
        _ => {
            let newcm = NEWCM_MATH_INSTANCE.get_or_init(|| {
                get_newcm_math_font().and_then(OpenTypeMathFont::from_font)
            });
            newcm.as_ref()
        }
    }
}

static NEWCM_COMPRESSED_CFF: OnceLock<Option<Vec<u8>>> = OnceLock::new();
static STIX2_COMPRESSED_CFF: OnceLock<Option<Vec<u8>>> = OnceLock::new();
static FIRAMATH_COMPRESSED_CFF: OnceLock<Option<Vec<u8>>> = OnceLock::new();
static ASANA_COMPRESSED_CFF: OnceLock<Option<Vec<u8>>> = OnceLock::new();

/// Returns the zlib-compressed CFF table bytes for the specified font family, cached globally with `OnceLock`.
pub fn get_compressed_cff(family: crate::backend::fonts::FontFamily) -> Option<&'static [u8]> {
    let lock = match family {
        crate::backend::fonts::FontFamily::Times => &STIX2_COMPRESSED_CFF,
        crate::backend::fonts::FontFamily::Helvetica => &FIRAMATH_COMPRESSED_CFF,
        crate::backend::fonts::FontFamily::Palatino => &ASANA_COMPRESSED_CFF,
        _ => &NEWCM_COMPRESSED_CFF,
    };
    let opt = lock.get_or_init(|| {
        let math_font = active_opentype_math_font(family)?;
        let cff = math_font.font.cff_table()?;
        let mut comp = Vec::with_capacity(cff.len() / 2);
        let mut compressor = crate::deflate_compress::DeflateCompressor::new();
        compressor.compress_zlib(cff, &mut comp);
        Some(comp)
    });
    opt.as_deref()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::fonts::FontFamily;
    use crate::math::math_ast::MathStyle;

    #[test]
    fn test_opentype_math_constants_cache_alignment() {
        assert_eq!(core::mem::size_of::<OpenTypeMathConstants>(), 128);
        assert_eq!(core::mem::align_of::<OpenTypeMathConstants>(), 64);
        assert_eq!(core::mem::size_of::<MathGlyphVariantRecord>(), 4);
        assert_eq!(core::mem::size_of::<GlyphPartRecord>(), 12);
        assert_eq!(core::mem::size_of::<MathGlyphProperties>(), 8);
        assert_eq!(core::mem::size_of::<OpenTypeScriptGlyph>(), 16);
        assert_eq!(core::mem::align_of::<OpenTypeScriptGlyph>(), 4);
        assert_eq!(core::mem::size_of::<StrokePoint>(), 8);
        assert_eq!(core::mem::size_of::<OpenTypeAssemblyRecipe>(), 8);
        assert_eq!(core::mem::size_of::<OpenTypeOperatorMetrics>(), 8);
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
    fn test_firamath_constants_values() {
        let c = &FIRAMATH_CONSTANTS;
        assert_eq!(c.script_percent_scale_down, 72);
        assert_eq!(c.script_script_percent_scale_down, 58);
        assert_eq!(c.axis_height, 280);
        assert_eq!(c.fraction_rule_thickness, 76);
        assert_eq!(c.fraction_numerator_display_style_shift_up, 580);
        assert_eq!(c.radical_degree_bottom_raise_percent, 64);
    }

    #[test]
    fn test_asana_math_constants_values() {
        let c = &ASANA_MATH_CONSTANTS;
        assert_eq!(c.script_percent_scale_down, 73);
        assert_eq!(c.script_script_percent_scale_down, 60);
        assert_eq!(c.axis_height, 271);
        assert_eq!(c.fraction_rule_thickness, 70);
        assert_eq!(c.fraction_numerator_display_style_shift_up, 725);
        assert_eq!(c.radical_degree_bottom_raise_percent, 65);
    }

    #[test]
    fn test_multi_family_font_resolution_and_geometry() {
        // ComputerModern -> NewCM
        let cm_font = active_opentype_math_font(FontFamily::ComputerModern);
        assert!(cm_font.is_some());
        if let Some(f) = cm_font {
            assert_eq!(f.constants.axis_height, 250);
        }

        // Times -> STIX2
        let times_font = active_opentype_math_font(FontFamily::Times);
        assert!(times_font.is_some());
        if let Some(f) = times_font {
            assert_eq!(f.constants.axis_height, 258);
        }

        // Helvetica -> FiraMath
        let helv_font = active_opentype_math_font(FontFamily::Helvetica);
        assert!(helv_font.is_some());
        if let Some(f) = helv_font {
            assert_eq!(f.constants.axis_height, 280);
        }

        // Palatino -> AsanaMath
        let pal_font = active_opentype_math_font(FontFamily::Palatino);
        assert!(pal_font.is_some());
        if let Some(f) = pal_font {
            assert_eq!(f.constants.axis_height, 271);
        }
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
    fn test_calligraphic_script_alphabets() {
        assert_eq!(NEWCM_SCRIPT_GLYPHS.len(), 26);
        assert_eq!(STIX2_SCRIPT_GLYPHS.len(), 26);

        // Check A and J (calligraphic J has descender)
        let newcm_a = script_glyph('A', FontFamily::ComputerModern).unwrap();
        assert_eq!(newcm_a.w, 857);
        assert_eq!(newcm_a.top_accent, 702);

        let newcm_j = script_glyph('J', FontFamily::ComputerModern).unwrap();
        assert_eq!(newcm_j.w, 698);
        assert_eq!(newcm_j.d, 129); // Descender!

        let stix2_j = script_glyph('J', FontFamily::Times).unwrap();
        assert_eq!(stix2_j.w, 551);
        assert_eq!(stix2_j.d, 127); // Descender!

        // Unicode mappings
        assert_eq!(to_unicode_script('A'), Some('\u{1D49C}'));
        assert_eq!(to_unicode_script('B'), Some('\u{212C}')); // Exception
        assert_eq!(to_unicode_fraktur('A'), Some('\u{1D504}'));
        assert_eq!(to_unicode_blackboard('R'), Some('\u{211D}')); // Real numbers
        assert_eq!(to_unicode_blackboard('N'), Some('\u{2115}')); // Natural numbers
        assert_eq!(to_unicode_blackboard('Z'), Some('\u{2124}')); // Integers
    }

    #[test]
    fn test_delimiter_variant_selection() {
        let paren = OpenTypeDelimKind::ParenLeft;
        // At 10pt: target 10.5pt = 1050 units -> selects variant 1 (adv=1095)
        assert_eq!(paren.select_variant_tier(10.5, 10.0), Some(1));
        // Target 25pt = 2500 units -> selects variant 7 (adv=2991)
        assert_eq!(paren.select_variant_tier(25.0, 10.0), Some(7));
        // Target 40pt = 4000 units -> exceeds discrete variants, returns None (assembly needed)
        assert_eq!(paren.select_variant_tier(40.0, 10.0), None);
        assert!(paren.assembly_recipe().is_some());
    }

    #[test]
    fn test_operator_variants_and_metrics() {
        let int_op = OpenTypeBigOpKind::Integral;
        assert!((int_op.advance_height(MathStyle::Text, 10.0) - 11.12).abs() < 1e-4);
        assert!((int_op.advance_height(MathStyle::Display, 10.0) - 22.23).abs() < 1e-4);
        assert!((int_op.italic_correction_pt(10.0) - 1.85).abs() < 1e-4);
        assert!((int_op.lower_limit_offset_pt(10.0) - (-0.92)).abs() < 1e-4);

        let newcm_path = "/usr/share/texlive/texmf-dist/fonts/opentype/public/newcomputermodern/NewCMMath-Regular.otf";
        if let Ok(bytes) = std::fs::read(newcm_path) {
            let font = OpenTypeFont::from_bytes(&bytes).expect("Valid NewCM");
            let math_font = OpenTypeMathFont::from_font(font).expect("Math font");

            // Dynamic display variant for Summation
            let sum_disp = OpenTypeBigOpKind::Summation.resolve_font_operator(&math_font, MathStyle::Display)
                .expect("Dynamic sum display");
            assert_eq!(sum_disp.glyph_id, GlyphId(5004));
            assert!(sum_disp.advance_width > 0);

            // Dynamic text variant for Summation
            let sum_text = OpenTypeBigOpKind::Summation.resolve_font_operator(&math_font, MathStyle::Text)
                .expect("Dynamic sum text");
            assert_eq!(sum_text.glyph_id, GlyphId(806));

            // Dynamic display variant for Integral
            let int_disp = OpenTypeBigOpKind::Integral.resolve_font_operator(&math_font, MathStyle::Display)
                .expect("Dynamic int display");
            assert_eq!(int_disp.glyph_id, GlyphId(4993));

            // Dynamic text variant for Integral
            let int_text = OpenTypeBigOpKind::Integral.resolve_font_operator(&math_font, MathStyle::Text)
                .expect("Dynamic int text");
            assert_eq!(int_text.glyph_id, GlyphId(832));

            // Dynamic display variant for Product
            let prod_disp = OpenTypeBigOpKind::Product.resolve_font_operator(&math_font, MathStyle::Display)
                .expect("Dynamic prod display");
            assert_eq!(prod_disp.glyph_id, GlyphId(5005));

            // Dynamic text variant for Product
            let prod_text = OpenTypeBigOpKind::Product.resolve_font_operator(&math_font, MathStyle::Text)
                .expect("Dynamic prod text");
            assert_eq!(prod_text.glyph_id, GlyphId(804));
        }

        let stix_path = "/usr/share/texlive/texmf-dist/fonts/opentype/public/stix2-otf/STIXTwoMath-Regular.otf";
        if let Ok(bytes) = std::fs::read(stix_path) {
            let font = OpenTypeFont::from_bytes(&bytes).expect("Valid STIX2");
            let math_font = OpenTypeMathFont::from_font(font).expect("Math font");

            // Dynamic display variant for Summation in STIX2
            let sum_disp = OpenTypeBigOpKind::Summation.resolve_font_operator(&math_font, MathStyle::Display)
                .expect("Dynamic sum display in STIX2");
            assert!(sum_disp.advance_width > 0);
            assert_ne!(sum_disp.glyph_id.0, 0);

            // Dynamic text variant for Summation in STIX2
            let sum_text = OpenTypeBigOpKind::Summation.resolve_font_operator(&math_font, MathStyle::Text)
                .expect("Dynamic sum text in STIX2");
            assert_eq!(sum_text.glyph_id, GlyphId(1646));
        }
    }

    #[test]
    fn test_strokes_and_bezier_evaluation() {
        let p0 = (0.0, 0.0);
        let p1 = (0.0, 10.0);
        let p2 = (10.0, 10.0);
        let p3 = (10.0, 0.0);

        let mid = CalligraphicStrokeBuffer::eval_cubic_bezier(p0, p1, p2, p3, 0.5);
        assert!((mid.0 - 5.0).abs() < 1e-4);
        assert!((mid.1 - 7.5).abs() < 1e-4);

        let nib = CalligraphicNib::default_copperplate(10.0);
        let (dx, dy) = nib.offset();
        assert!(dx > 0.0 && dy > 0.0);
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

            // 1. Dynamic Unicode -> GID resolution via cmap Format 4 & Format 12
            assert_eq!(font.glyph_for_unicode(0x41), Some(GlyphId(34))); // Latin A
            assert_eq!(font.glyph_for_unicode(0x222B), Some(GlyphId(832))); // \int
            assert_eq!(font.glyph_for_unicode(0x2211), Some(GlyphId(806))); // \sum
            assert_eq!(font.glyph_for_unicode(0x1D49C), Some(GlyphId(2229))); // \mathcal{A} (Format 12!)
            assert_eq!(font.glyph_for_unicode(0x1D4A5), Some(GlyphId(2233))); // \mathcal{J} (Format 12!)

            // 2. Dynamic horizontal metrics via hmtx
            assert_eq!(font.h_advance(2233), Some(698)); // Script J advance
            assert_eq!(font.h_advance(832), Some(665)); // Integral advance
            assert_eq!(font.h_advance(9), Some(389)); // parenleft advance

            // 3. Dynamic extensible vertical delimiter assembly via MATH table
            assert_eq!(font.vert_assembly_part_count(9), 3);
            let bot = font.vert_assembly_part(9, 0).expect("bot piece");
            let ext = font.vert_assembly_part(9, 1).expect("extender");
            let top = font.vert_assembly_part(9, 2).expect("top piece");
            assert_eq!(bot.glyph, 1088);
            assert_eq!(ext.glyph, 1087);
            assert!(ext.is_extender());
            assert_eq!(top.glyph, 1086);
            assert!(!top.is_extender());

            // 4. Unified MathFont trait abstraction
            let math_font = OpenTypeMathFont::from_font(font).expect("OpenTypeMathFont init");
            assert_eq!(math_font.glyph_for_unicode(0x1D4A5), Some(GlyphId(2233)));
            assert_eq!(math_font.advance_width(GlyphId(2233)), Some(698));

            let sum_gid = font.glyph_for_unicode(0x2211).expect("sum");
            assert_eq!(sum_gid, GlyphId(806));
            let sum_variants = font.vert_variant_count(806);
            assert!(sum_variants > 0);
            let sum_display = font.vert_variant(806, sum_variants - 1).expect("display sum");
            assert_eq!(sum_display.variant_glyph, 5004);

            let a_gid = font.glyph_for_unicode('A' as u32).expect("A");
            let a_attach = math_font.top_accent_attachment(a_gid);
            assert!(a_attach.is_some(), "NewCM must provide top accent attachment for A");

            let rad_gid = font.glyph_for_unicode(0x221A).expect("radical");
            assert!(font.h_advance(rad_gid.0).is_some());

            let bold_v = font.glyph_for_unicode(0x1D42F).expect("bold v");
            let bold_a = font.glyph_for_unicode(0x1D400).expect("bold A");
            assert!(font.h_advance(bold_v.0).is_some());
            assert!(font.h_advance(bold_a.0).is_some());
        }

        // 5. Test interchangeable font: STIX Two Math (zero code changes!)
        let stix_path = "/usr/share/texlive/texmf-dist/fonts/opentype/public/stix2-otf/STIXTwoMath-Regular.otf";
        if let Ok(stix_bytes) = std::fs::read(stix_path) {
            let stix_font = OpenTypeFont::from_bytes(&stix_bytes).expect("Valid STIX2 font");
            assert_eq!(stix_font.glyph_for_unicode(0x1D4A5), Some(GlyphId(3416))); // Script J
            assert_eq!(stix_font.glyph_for_unicode(0x222B), Some(GlyphId(1698))); // Integral
            assert_eq!(stix_font.glyph_for_unicode(0x2211), Some(GlyphId(1646))); // Summation
            assert!(stix_font.h_advance(3416).is_some());

            let paren_gid = stix_font.glyph_for_unicode('(' as u32).expect("paren");
            let var_count = stix_font.vert_variant_count(paren_gid.0);
            assert!(var_count > 0, "STIX2 must have discrete variants for paren");
            let part_count = stix_font.vert_assembly_part_count(paren_gid.0);
            assert!(part_count >= 3, "STIX2 must have assembly parts for paren");
            let stix_math_font = OpenTypeMathFont::from_font(stix_font).expect("OpenTypeMathFont init");
            assert_eq!(stix_math_font.vertical_variants(paren_gid), var_count);
            assert_eq!(stix_math_font.vertical_assembly_part_count(paren_gid), part_count);

            let stix_sum_gid = GlyphId(1646);
            let stix_sum_vars = stix_font.vert_variant_count(stix_sum_gid.0);
            assert!(stix_sum_vars > 0);
            let stix_sum_display = stix_font.vert_variant(stix_sum_gid.0, stix_sum_vars - 1).expect("stix display sum");
            assert_ne!(stix_sum_display.variant_glyph, 0);
        }
    }

    #[test]
    fn test_cff_and_truetype_outline_access() {
        if let Some(font) = get_newcm_math_font() {
            // 1. PostScript Name from CFF Name INDEX
            let ps_name = font.cff_postscript_name().expect("NewCM PostScript name");
            assert_eq!(ps_name, "NewCMMath-Regular");

            // 2. FontBBox from CFF Top DICT
            let bbox = font.cff_font_bbox().expect("NewCM FontBBox");
            assert_eq!(bbox, [-1042, -3060, 4082, 3560]);

            // 3. CharStrings count
            let cs_count = font.cff_charstrings_count().expect("NewCM CharStrings count");
            assert!(cs_count > 7000);

            // 4. Raw Type 2 CharString slice retrieval
            let gid_a = font.glyph_for_unicode('A' as u32).expect("GID for 'A'");
            let cs_a = font.cff_charstring(gid_a.0).expect("CharString for 'A'");
            assert!(!cs_a.is_empty());

            let gid_int = font.glyph_for_unicode(0x222B).expect("GID for integral");
            let cs_int = font.cff_charstring(gid_int.0).expect("CharString for integral");
            assert!(!cs_int.is_empty());

            // 5. Reverse cmap lookup (unicode_for_glyph) for BMP and SMP
            assert_eq!(font.unicode_for_glyph(gid_a.0), Some('A' as u32));
            assert_eq!(font.unicode_for_glyph(gid_int.0), Some(0x222B));

            if let Some(gid_bold_v) = font.glyph_for_unicode(0x1D42F) {
                assert_eq!(font.unicode_for_glyph(gid_bold_v.0), Some(0x1D42F));
            }
        }

        if let Some(stix) = get_stix2_math_font() {
            let ps_name = stix.cff_postscript_name().expect("STIX2 PostScript name");
            assert!(ps_name.contains("STIXTwoMath"));

            let cs_count = stix.cff_charstrings_count().expect("STIX2 CharStrings count");
            assert!(cs_count > 6000);

            let gid_sum = stix.glyph_for_unicode(0x2211).expect("GID for sum");
            let cs_sum = stix.cff_charstring(gid_sum.0).expect("CharString for sum");
            assert!(!cs_sum.is_empty());
        }

        // 6. Validate Dynamic CFF CharStrings Subsetter
        if let Some(font) = get_newcm_math_font() {
            let orig_cff = font.cff_table().expect("NewCM CFF table");
            let gid_a = font.glyph_for_unicode('A' as u32).expect("GID for 'A'").0;
            let gid_int = font.glyph_for_unicode(0x222B).expect("GID for integral").0;
            let orig_cs_a = font.cff_charstring(gid_a).expect("original CharString for A");
            let orig_cs_int = font.cff_charstring(gid_int).expect("original CharString for int");

            let used_gids = [gid_a, gid_int];
            let sub_cff = crate::math::fonts::opentype::subset_cff(orig_cff, &used_gids)
                .expect("subset_cff must succeed");

            assert!(sub_cff.len() < orig_cff.len() / 4, "Subsetted CFF must be < 25% of original size: got {} vs {}", sub_cff.len(), orig_cff.len());

            use crate::math::fonts::opentype::{parse_cff_index, read_cff_offset, decode_top_dict_operator};
            let hdr_size = sub_cff[2] as usize;
            let (_, _, _, _, next_off) = parse_cff_index(&sub_cff, hdr_size).unwrap();
            let (_, top_offsets, top_off_size, top_data, _) = parse_cff_index(&sub_cff, next_off).unwrap();
            let top_start = read_cff_offset(&sub_cff, top_offsets, top_off_size).unwrap().saturating_sub(1);
            let top_end = read_cff_offset(&sub_cff, top_offsets + top_off_size, top_off_size).unwrap().saturating_sub(1);
            let top_dict = &sub_cff[top_data + top_start..top_data + top_end];

            let cs_off = decode_top_dict_operator(top_dict, 17).unwrap() as usize;
            let (cs_count, cs_offsets, cs_off_size, cs_data, _) = parse_cff_index(&sub_cff, cs_off).unwrap();
            assert_eq!(cs_count, font.cff_charstrings_count().unwrap() as usize);

            // Verify 'A' charstring is exact match
            let start_a = read_cff_offset(&sub_cff, cs_offsets + (gid_a as usize) * cs_off_size, cs_off_size).unwrap().saturating_sub(1);
            let end_a = read_cff_offset(&sub_cff, cs_offsets + (gid_a as usize + 1) * cs_off_size, cs_off_size).unwrap().saturating_sub(1);
            assert_eq!(&sub_cff[cs_data + start_a..cs_data + end_a], orig_cs_a);

            // Verify integral charstring is exact match
            let start_int = read_cff_offset(&sub_cff, cs_offsets + (gid_int as usize) * cs_off_size, cs_off_size).unwrap().saturating_sub(1);
            let end_int = read_cff_offset(&sub_cff, cs_offsets + (gid_int as usize + 1) * cs_off_size, cs_off_size).unwrap().saturating_sub(1);
            assert_eq!(&sub_cff[cs_data + start_int..cs_data + end_int], orig_cs_int);

            // Verify unused glyph (e.g. GID 100) is pruned to 1-byte endchar (14)
            let unused_gid = 100usize;
            let start_unused = read_cff_offset(&sub_cff, cs_offsets + unused_gid * cs_off_size, cs_off_size).unwrap().saturating_sub(1);
            let end_unused = read_cff_offset(&sub_cff, cs_offsets + (unused_gid + 1) * cs_off_size, cs_off_size).unwrap().saturating_sub(1);
            assert_eq!(&sub_cff[cs_data + start_unused..cs_data + end_unused], &[14]);
        }
    }

    #[test]
    fn test_modern_vector_glyph_emission_and_pdf_stream() {
        use crate::math::fonts::get_vector_glyph_path;

        // 1. Verify vector path retrieval for calligraphic J (glyph 1129)
        let path_j = get_vector_glyph_path(1129).expect("Must have vector path for Script J");
        assert!(path_j.ends_with(b"h"));

        // 2. Typeset \mathcal{J} and verify Paint::VectorGlyph emission in uncompressed PDF
        let doc_src = "\\documentclass{article}\n\\begin{document}\n$\\mathcal{J}$\n\\end{document}";
        let parsed = crate::tex::parse(doc_src).expect("parse");
        let pages = crate::layout::layout(&parsed);
        assert!(pages[0].ops.iter().any(|op| matches!(op, crate::layout::layout_engine::Paint::VectorGlyph { glyph_id: 3416 | 2233 | 1249, .. })));
        let pdf_bytes = crate::compile_str_opts(doc_src, false).expect("Must compile document");
        let pdf_str = String::from_utf8_lossy(&pdf_bytes);
        assert!(pdf_str.contains("cm\n"), "PDF stream must contain coordinate transformation for vector glyph");
        assert!(pdf_str.contains("658 692 m"), "PDF stream must contain Script J vector path");
        assert!(pdf_str.contains("\nf\nQ\n"), "PDF stream must close and fill vector path");
    }

    #[test]
    fn test_dynamic_font_delimiter_resolution() {
        use crate::math::fonts::delimiters::{OpenTypeDelimKind, ResolvedDelimiter};

        let newcm_path = "/usr/share/texlive/texmf-dist/fonts/opentype/public/newcomputermodern/NewCMMath-Regular.otf";
        if let Ok(bytes) = std::fs::read(newcm_path) {
            let font = OpenTypeFont::from_bytes(&bytes).expect("Valid NewCM");
            let math_font = OpenTypeMathFont::from_font(font).expect("OpenTypeMathFont init");

            // 1. Discrete variant selection for small height (15pt at 10pt font)
            let delim_res = OpenTypeDelimKind::ParenLeft.resolve_font_delimiter(&math_font, 15.0, 10.0)
                .expect("Must resolve paren delimiter");
            match delim_res {
                ResolvedDelimiter::Variant { glyph_id, advance } => {
                    assert!(advance >= 1200, "Variant advance must cover target units: got {}", advance);
                    assert_ne!(glyph_id.0, 0);
                }
                ResolvedDelimiter::Assembly { .. } => panic!("15pt height should use discrete variant, not assembly"),
            }

            // 2. Extensible assembly for large height (50pt at 10pt font)
            let large_res = OpenTypeDelimKind::ParenLeft.resolve_font_delimiter(&math_font, 50.0, 10.0)
                .expect("Must resolve large paren delimiter");
            match large_res {
                ResolvedDelimiter::Assembly { recipe, min_overlap, .. } => {
                    assert_eq!(recipe.top_glyph, GlyphId(1086));
                    assert_eq!(recipe.ext_glyph, GlyphId(1087));
                    assert_eq!(recipe.bot_glyph, GlyphId(1088));
                    assert!(min_overlap >= 0);
                }
                ResolvedDelimiter::Variant { .. } => panic!("50pt height should trigger extensible assembly"),
            }

            // 3. Dynamic Radical resolution
            let rad_small = OpenTypeDelimKind::Radical.resolve_font_delimiter(&math_font, 15.0, 10.0)
                .expect("Must resolve radical delimiter");
            match rad_small {
                ResolvedDelimiter::Variant { glyph_id, advance } => {
                    assert_eq!(glyph_id, GlyphId(5010));
                    assert_eq!(advance, 1801);
                }
                ResolvedDelimiter::Assembly { .. } => panic!("15pt should be variant"),
            }

            let rad_large = OpenTypeDelimKind::Radical.resolve_font_delimiter(&math_font, 50.0, 10.0)
                .expect("Must resolve large radical delimiter");
            match rad_large {
                ResolvedDelimiter::Assembly { recipe, .. } => {
                    assert_eq!(recipe.top_glyph, GlyphId(5008));
                    assert_eq!(recipe.ext_glyph, GlyphId(5007));
                    assert_eq!(recipe.bot_glyph, GlyphId(1114));
                }
                ResolvedDelimiter::Variant { .. } => panic!("50pt should trigger assembly"),
            }
        }

        // Test STIX Two Math if available
        if let Some(stix_font) = get_stix2_math_font() {
            let math_font = OpenTypeMathFont::from_font(stix_font).expect("STIX2 Math font init");
            let delim_res = OpenTypeDelimKind::ParenLeft.resolve_font_delimiter(&math_font, 15.0, 10.0)
                .expect("STIX2 paren resolve");
            assert!(matches!(delim_res, ResolvedDelimiter::Variant { .. }));

            let rad_res = OpenTypeDelimKind::Radical.resolve_font_delimiter(&math_font, 15.0, 10.0)
                .expect("STIX2 radical resolve");
            assert!(matches!(rad_res, ResolvedDelimiter::Variant { .. }));
        }
    }

    #[test]
    fn test_bold_vectors_radicals_and_integrals() {
        // 1. Bold vector \mathbf{v}, \vec{\mathbf{v}}, \boldsymbol{\alpha}, \bm{v}
        let bold_doc = "\\documentclass{article}\n\\begin{document}\n$\\vec{\\mathbf{v}} + \\mathbf{A} + \\boldsymbol{\\alpha} + \\bm{w} + \\mathbf{\\Gamma}$\n\\end{document}";
        let parsed = crate::tex::parse(bold_doc).expect("parse bold");
        let pages = crate::layout::layout(&parsed);
        assert!(!pages.is_empty());
        assert!(!pages[0].ops.is_empty());

        // 2. Radical \sqrt{x} and \sqrt[3]{x^2 + 1}
        let rad_doc = "\\documentclass{article}\n\\begin{document}\n$\\sqrt{x} + \\sqrt[3]{x^2 + 1}$\n\\end{document}";
        let parsed_rad = crate::tex::parse(rad_doc).expect("parse rad");
        let pages_rad = crate::layout::layout(&parsed_rad);
        assert!(!pages_rad.is_empty());
        assert!(!pages_rad[0].ops.is_empty());

        // 3. Integrals: \iint, \iiint, \oint, \oiint
        let int_doc = "\\documentclass{article}\n\\begin{document}\n$\\iint_D f(x,y)\\,dx\\,dy = \\oint_C P\\,dx + Q\\,dy + \\oiint_S \\vec{F}\\cdot d\\vec{A}$\n\\end{document}";
        let parsed_int = crate::tex::parse(int_doc).expect("parse int");
        let pages_int = crate::layout::layout(&parsed_int);
        assert!(!pages_int.is_empty());
        assert!(!pages_int[0].ops.is_empty());

        // 4. Dynamic OpenType BigOp multi-integral verification on loaded font
        if let Some(font) = get_newcm_math_font() {
            let math_font = OpenTypeMathFont::from_font(font).expect("OpenTypeMathFont init");
            let dbl_int = OpenTypeBigOpKind::DoubleIntegral.resolve_font_operator(&math_font, MathStyle::Display)
                .expect("Must resolve DoubleIntegral in NewCM");
            assert_ne!(dbl_int.glyph_id.0, 0);
            assert_eq!(dbl_int.advance_height, 2223);

            let trp_int = OpenTypeBigOpKind::TripleIntegral.resolve_font_operator(&math_font, MathStyle::Display)
                .expect("Must resolve TripleIntegral in NewCM");
            assert_ne!(trp_int.glyph_id.0, 0);
            assert_eq!(trp_int.advance_height, 2223);
        }
    }

    #[test]
    fn test_opentype_driven_layout_integration() {
        let cm_otf = active_opentype_math_font(FontFamily::ComputerModern);
        assert!(cm_otf.is_some(), "Must resolve NewCMMath on systems with texlive");
        if let Some(otf) = cm_otf {
            assert_eq!(otf.constants.axis_height, 250);
            assert_eq!(otf.constants.radical_degree_bottom_raise_percent, 60);
            assert!(otf.constants.radical_rule_thickness > 0);

            // Delimiter mapping & resolution
            let paren = OpenTypeDelimKind::from_delim("(").expect("ParenLeft");
            assert_eq!(paren, OpenTypeDelimKind::ParenLeft);
            let delim_res = paren.resolve_font_delimiter(otf, 20.0, 10.0);
            assert!(delim_res.is_some());

            // BigOp resolution
            let int_op = OpenTypeBigOpKind::Integral.resolve_font_operator(otf, MathStyle::Display);
            assert!(int_op.is_some());
            let int_resolved = int_op.unwrap();
            assert_eq!(int_resolved.lower_limit_kern, -92);
        }

        // Layout verification
        let doc = "\\documentclass{article}\n\\begin{document}\n$\\sqrt{\\frac{x^2+1}{x-1}} + \\int_0^1 f(x)\\,dx + \\oint_C g(z)\\,dz$\n\\end{document}";
        let parsed = crate::tex::parse(doc).expect("parse math document");
        let pages = crate::layout::layout(&parsed);
        assert_eq!(pages.len(), 1);
        assert!(!pages[0].ops.is_empty());
    }

    #[test]
    fn test_horizontal_variants_and_extensible_assemblies() {
        use crate::math::fonts::delimiters::{OpenTypeHorizKind, ResolvedHorizontal};

        if let Some(otf) = active_opentype_math_font(FontFamily::ComputerModern) {
            // 1. WideHat discrete variants: small vs large
            let hat_small = OpenTypeHorizKind::WideHat.resolve_font_horizontal(otf, 6.0, 10.0)
                .expect("Resolve small wide hat");
            match hat_small {
                ResolvedHorizontal::Variant { advance, .. } => {
                    assert!(advance >= 600, "WideHat variant advance must cover width: got {}", advance);
                }
                ResolvedHorizontal::Assembly { .. } => panic!("Small wide hat should be discrete variant"),
            }

            let hat_large = OpenTypeHorizKind::WideHat.resolve_font_horizontal(otf, 18.0, 10.0)
                .expect("Resolve large wide hat");
            match hat_large {
                ResolvedHorizontal::Variant { advance, .. } => {
                    assert!(advance >= 1800, "Large WideHat advance must cover 18pt: got {}", advance);
                }
                ResolvedHorizontal::Assembly { .. } => panic!("18pt wide hat should be largest variant"),
            }

            // 2. OverBrace discrete variant (small formula) vs extensible assembly (wide formula)
            let over_small = OpenTypeHorizKind::OverBrace.resolve_font_horizontal(otf, 15.0, 10.0)
                .expect("Resolve small overbrace");
            assert!(matches!(over_small, ResolvedHorizontal::Variant { .. }));

            let over_large = OpenTypeHorizKind::OverBrace.resolve_font_horizontal(otf, 60.0, 10.0)
                .expect("Resolve large overbrace");
            match over_large {
                ResolvedHorizontal::Assembly { recipe, min_overlap } => {
                    assert_ne!(recipe.left_glyph.0, 0, "Overbrace left piece must be valid");
                    assert_ne!(recipe.ext_glyph.0, 0, "Overbrace extender must be valid");
                    assert_ne!(recipe.mid_glyph.0, 0, "Overbrace mid piece must be valid");
                    assert_ne!(recipe.right_glyph.0, 0, "Overbrace right piece must be valid");
                    assert!(min_overlap >= 0);
                }
                ResolvedHorizontal::Variant { .. } => panic!("60pt overbrace must use extensible assembly"),
            }

            // 3. UnderBrace extensible assembly
            let under_large = OpenTypeHorizKind::UnderBrace.resolve_font_horizontal(otf, 60.0, 10.0)
                .expect("Resolve large underbrace");
            match under_large {
                ResolvedHorizontal::Assembly { recipe, .. } => {
                    assert_ne!(recipe.left_glyph.0, 0);
                    assert_ne!(recipe.ext_glyph.0, 0);
                    assert_ne!(recipe.mid_glyph.0, 0);
                    assert_ne!(recipe.right_glyph.0, 0);
                }
                ResolvedHorizontal::Variant { .. } => panic!("60pt underbrace must use assembly"),
            }
        }

        // 4. Test full document layout and Paint::VectorGlyph emission for wide accents & braces
        let doc = "\\documentclass{article}\n\\begin{document}\n$\\widehat{x^2+y^2+z^2} + \\widetilde{A+B+C} + \\overbrace{a_1 + a_2 + a_3 + a_4}^N + \\underbrace{b_1 + \\dots + b_k}_K$\n\\end{document}";
        let parsed = crate::tex::parse(doc).expect("parse wide math");
        let pages = crate::layout::layout(&parsed);
        assert!(!pages.is_empty());
        assert!(pages[0].ops.iter().any(|op| matches!(op, crate::layout::layout_engine::Paint::VectorGlyph { .. })),
            "Pages must contain Paint::VectorGlyph operations for OpenType horizontal math");
    }

    #[test]
    fn test_alphanumeric_alphabets_and_bold_vector_optics() {
        use super::calligraphy::*;

        // 1. Fraktur Unicode mapping parity (uppercase, lowercase, and Unicode 3.1 exceptions)
        assert_eq!(to_unicode_fraktur('C'), Some('\u{212D}'));
        assert_eq!(to_unicode_fraktur('H'), Some('\u{210C}'));
        assert_eq!(to_unicode_fraktur('I'), Some('\u{2111}'));
        assert_eq!(to_unicode_fraktur('R'), Some('\u{211C}'));
        assert_eq!(to_unicode_fraktur('Z'), Some('\u{2128}'));
        assert_eq!(to_unicode_fraktur('A'), Some('\u{1D504}'));
        assert_eq!(to_unicode_fraktur('g'), Some('\u{1D524}'));
        assert_eq!(to_unicode_fraktur('z'), Some('\u{1D537}'));

        // 2. Blackboard Bold Unicode mapping parity (uppercase exceptions, lowercase, and digits)
        assert_eq!(to_unicode_blackboard('R'), Some('\u{211D}'));
        assert_eq!(to_unicode_blackboard('C'), Some('\u{2102}'));
        assert_eq!(to_unicode_blackboard('N'), Some('\u{2115}'));
        assert_eq!(to_unicode_blackboard('Z'), Some('\u{2124}'));
        assert_eq!(to_unicode_blackboard('Q'), Some('\u{211A}'));
        assert_eq!(to_unicode_blackboard('P'), Some('\u{2119}'));
        assert_eq!(to_unicode_blackboard('A'), Some('\u{1D538}'));
        assert_eq!(to_unicode_blackboard('a'), Some('\u{1D552}'));
        assert_eq!(to_unicode_blackboard('z'), Some('\u{1D56B}'));
        assert_eq!(to_unicode_blackboard('0'), Some('\u{1D7D8}'));
        assert_eq!(to_unicode_blackboard('1'), Some('\u{1D7D9}'));

        // 3. Parser verification for \mathfrak and \frak
        let mathfrak_parsed = crate::math::parse_math_str(r"\mathfrak{g}");
        assert!(matches!(mathfrak_parsed, crate::math::MathNode::StyleChange { face: crate::backend::fonts::Face::AmsA, .. }));
        let frak_parsed = crate::math::parse_math_str(r"\frak{H}");
        assert!(matches!(frak_parsed, crate::math::MathNode::StyleChange { face: crate::backend::fonts::Face::AmsA, .. }));

        // 4. Full document layout with bold vectors, blackboard bold, and fraktur
        let doc = "\\documentclass{article}\n\\begin{document}\n$\\mathbf{v} + \\boldsymbol{\\alpha} + \\mathbb{R} + \\mathfrak{g} + \\vec{\\mathbf{v}} + \\widehat{\\mathbf{v}}$\n\\end{document}";
        let parsed = crate::tex::parse(doc).expect("parse alphanumeric math");
        let pages = crate::layout::layout(&parsed);
        assert!(!pages.is_empty());

        let mut found_bold_v = false;
        let mut found_bb_r = false;
        let mut found_bold_alpha = false;
        let mut found_frak_g = false;
        for op in &pages[0].ops {
            if let crate::layout::layout_engine::Paint::VectorGlyph { glyph_id, .. } = op {
                if *glyph_id == 2121 || *glyph_id == 2224 || *glyph_id == 3295 || *glyph_id == 3407 {
                    found_bold_v = true;
                }
                if *glyph_id == 636 || *glyph_id == 1236 {
                    found_bb_r = true;
                }
                if *glyph_id == 2754 || *glyph_id == 2870 || *glyph_id == 3957 || *glyph_id == 4073 {
                    found_bold_alpha = true;
                }
                if *glyph_id == 2349 || *glyph_id == 3536 {
                    found_frak_g = true;
                }
            }
        }
        assert!(found_bold_v, "Must emit Paint::VectorGlyph for bold vector v");
        assert!(found_bb_r, "Must emit Paint::VectorGlyph for blackboard bold R");
        assert!(found_bold_alpha, "Must emit Paint::VectorGlyph for bold alpha");
        assert!(found_frak_g, "Must emit Paint::VectorGlyph for fraktur g");

        // 5. Verify Computer Modern layout with bold vectors and apex accent positioning
        let doc_cm = "\\documentclass{article}\n\\usepackage{computermodern}\n\\begin{document}\n$\\mathbf{v} + \\boldsymbol{\\alpha} + \\mathbb{R} + \\mathfrak{g} + \\vec{\\mathbf{v}}$\n\\end{document}";
        let parsed_cm = crate::tex::parse(doc_cm).expect("parse cm alphanumeric math");
        let pages_cm = crate::layout::layout(&parsed_cm);
        assert!(!pages_cm.is_empty());
        let found_cm_bold_v = pages_cm[0].ops.iter().any(|op| {
            matches!(op, crate::layout::layout_engine::Paint::VectorGlyph { glyph_id: 2121, .. })
        });
        assert!(found_cm_bold_v, "Computer Modern must emit NewCM GID 2121 for bold vector v");
    }
}
