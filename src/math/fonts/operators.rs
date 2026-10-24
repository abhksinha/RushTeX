//! # Modern OpenType Big Operators & Integrals Subsystem
//!
//! Variant measurements and optical positioning for large mathematical operators ($\int, \iint, \oint, \sum, \prod$).
//!
//! Designed under `native-architecture.md` §11b (resource rules):
//! - Compact static structures with zero heap allocation
//! - Fast $O(1)$ variant scaling based on active MathStyle tier

use crate::math::fonts::opentype::{GlyphId, MathFont, UnicodeScalar};
use crate::math::math_ast::MathStyle;

/// OpenType Big Operator Kind.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenTypeBigOpKind {
    Summation = 0,
    Product = 1,
    Integral = 2,
    ContourIntegral = 3,
    Union = 4,
    Intersection = 5,
    DoubleIntegral = 6,
    TripleIntegral = 7,
    SurfaceIntegral = 8,
    VolumeIntegral = 9,
    Coproduct = 10,
}

/// OpenType operator sizing and optical parameters.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenTypeOperatorMetrics {
    /// Text style advance height in design units
    pub text_advance: u16,
    /// Display style advance height in design units
    pub display_advance: u16,
    /// Italic correction in design units
    pub italic_correction: u16,
    /// Overhang offset for lower limit in design units
    pub lower_limit_kern: i16,
}

const _: () = assert!(core::mem::size_of::<OpenTypeOperatorMetrics>() == 8);

/// Dynamically resolved Big Operator variant with font-driven metrics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedBigOp {
    pub glyph_id: GlyphId,
    pub advance_width: u16,
    pub advance_height: u16,
    pub italic_correction: i16,
    pub lower_limit_kern: i16,
}

/// Pre-extracted OpenType metrics for NewCMMath operators.
pub static NEWCM_OPERATOR_METRICS: [OpenTypeOperatorMetrics; 11] = [
    // Summation (0)
    OpenTypeOperatorMetrics { text_advance: 1001, display_advance: 1401, italic_correction: 0,   lower_limit_kern: 0 },
    // Product (1)
    OpenTypeOperatorMetrics { text_advance: 1001, display_advance: 1401, italic_correction: 0,   lower_limit_kern: 0 },
    // Integral (2)
    OpenTypeOperatorMetrics { text_advance: 1112, display_advance: 2223, italic_correction: 185, lower_limit_kern: -92 },
    // ContourIntegral (3)
    OpenTypeOperatorMetrics { text_advance: 1112, display_advance: 2223, italic_correction: 185, lower_limit_kern: -92 },
    // Union (4)
    OpenTypeOperatorMetrics { text_advance: 1001, display_advance: 1401, italic_correction: 0,   lower_limit_kern: 0 },
    // Intersection (5)
    OpenTypeOperatorMetrics { text_advance: 1001, display_advance: 1401, italic_correction: 0,   lower_limit_kern: 0 },
    // DoubleIntegral (6)
    OpenTypeOperatorMetrics { text_advance: 1112, display_advance: 2223, italic_correction: 185, lower_limit_kern: -92 },
    // TripleIntegral (7)
    OpenTypeOperatorMetrics { text_advance: 1112, display_advance: 2223, italic_correction: 185, lower_limit_kern: -92 },
    // SurfaceIntegral (8)
    OpenTypeOperatorMetrics { text_advance: 1112, display_advance: 2223, italic_correction: 185, lower_limit_kern: -92 },
    // VolumeIntegral (9)
    OpenTypeOperatorMetrics { text_advance: 1112, display_advance: 2223, italic_correction: 185, lower_limit_kern: -92 },
    // Coproduct (10)
    OpenTypeOperatorMetrics { text_advance: 1001, display_advance: 1401, italic_correction: 0,   lower_limit_kern: 0 },
];

impl OpenTypeBigOpKind {
    /// Returns the canonical Unicode character for this big operator.
    #[inline(always)]
    pub fn unicode_scalar(&self) -> UnicodeScalar {
        match self {
            Self::Summation => UnicodeScalar(0x2211),
            Self::Product => UnicodeScalar(0x220F),
            Self::Integral => UnicodeScalar(0x222B),
            Self::ContourIntegral => UnicodeScalar(0x222E),
            Self::Union => UnicodeScalar(0x22C3),
            Self::Intersection => UnicodeScalar(0x22C2),
            Self::DoubleIntegral => UnicodeScalar(0x222C),
            Self::TripleIntegral => UnicodeScalar(0x222D),
            Self::SurfaceIntegral => UnicodeScalar(0x222F),
            Self::VolumeIntegral => UnicodeScalar(0x2230),
            Self::Coproduct => UnicodeScalar(0x2210),
        }
    }

    /// Maps a LaTeX operator command name to an `OpenTypeBigOpKind`.
    #[inline(always)]
    pub fn from_command(op: &str) -> Option<Self> {
        match op {
            "sum" => Some(Self::Summation),
            "prod" => Some(Self::Product),
            "int" => Some(Self::Integral),
            "oint" => Some(Self::ContourIntegral),
            "iint" => Some(Self::DoubleIntegral),
            "iiint" => Some(Self::TripleIntegral),
            "oiint" => Some(Self::SurfaceIntegral),
            "oiiint" => Some(Self::VolumeIntegral),
            "bigcup" => Some(Self::Union),
            "bigcap" => Some(Self::Intersection),
            "coprod" => Some(Self::Coproduct),
            _ => None,
        }
    }

    /// Returns the optical metrics for this operator.
    #[inline(always)]
    pub fn metrics(&self) -> OpenTypeOperatorMetrics {
        NEWCM_OPERATOR_METRICS[*self as usize]
    }

    /// Computes the advance height in points given the active `MathStyle` and `font_size`.
    #[inline(always)]
    pub fn advance_height(&self, style: MathStyle, font_size: f32) -> f32 {
        let m = self.metrics();
        let adv_units = if style.is_display() {
            m.display_advance
        } else {
            m.text_advance
        };
        (adv_units as f32 * 0.001) * font_size
    }

    /// Computes the italic correction in points.
    #[inline(always)]
    pub fn italic_correction_pt(&self, font_size: f32) -> f32 {
        (self.metrics().italic_correction as f32 * 0.001) * font_size
    }

    /// Computes the horizontal offset to tuck lower limit under slanted integral stem.
    #[inline(always)]
    pub fn lower_limit_offset_pt(&self, font_size: f32) -> f32 {
        (self.metrics().lower_limit_kern as f32 * 0.001) * font_size
    }

    /// Dynamically resolves the operator variant (text vs. display) and metrics using the loaded MathFont.
    pub fn resolve_font_operator(
        &self,
        font: &impl MathFont,
        style: MathStyle,
    ) -> Option<ResolvedBigOp> {
        let u = self.unicode_scalar();
        let base_gid = font.glyph_for_unicode(u.0)?;

        let (selected_gid, adv_h) = if style.is_display() {
            let var_count = font.vertical_variants(base_gid);
            if var_count > 0 {
                let last = font.vertical_variant(base_gid, var_count - 1);
                if let Some(v) = last {
                    (GlyphId(v.variant_glyph), v.advance_measurement)
                } else {
                    (base_gid, self.metrics().display_advance)
                }
            } else {
                (base_gid, self.metrics().display_advance)
            }
        } else {
            (base_gid, self.metrics().text_advance)
        };

        let adv_w = font.advance_width(selected_gid).unwrap_or(1000);
        let ic = font.italic_correction(selected_gid).unwrap_or(0);
        let kern = if matches!(self, Self::Integral | Self::ContourIntegral) {
            -92
        } else {
            0
        };

        Some(ResolvedBigOp {
            glyph_id: selected_gid,
            advance_width: adv_w,
            advance_height: adv_h,
            italic_correction: ic,
            lower_limit_kern: kern,
        })
    }
}
