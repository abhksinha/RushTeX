//! Mathematical Layout Geometry Verification Oracle.
//!
//! Provides machine-readable bounding box, baseline, and optical geometry extraction
//! for automated regression verification against authoritative TeX Live reference metrics
//! (as specified in RushTeX Math Architecture Blueprint Section 33 & 35).

use crate::backend::fonts::FontFamily;
use crate::math::math_ast::MathStyle;
use crate::math::math_layout::{layout_node, MathLayoutContext};
use crate::Error;

/// Machine-readable bounding box, baseline, and optical geometry metrics for a mathematical formula.
///
/// Packed into 24 bytes (4-byte aligned) for optimal L1 cache line density.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct MathFormulaGeometry {
    /// Total layout advance width in points.
    pub width: f32,
    /// Total ascender height above the math baseline in points.
    pub height: f32,
    /// Total descender depth below the math baseline in points.
    pub depth: f32,
    /// Typographic mathematical axis height above baseline in points.
    pub axis_height: f32,
    /// Fraction and radical rule stroke thickness in points.
    pub rule_thickness: f32,
    /// Number of top-level structural math nodes in the formula.
    pub child_count: u32,
}

const _: () = assert!(core::mem::size_of::<MathFormulaGeometry>() == 24);

/// Typographic geometry verification error with discrepancy details.
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryDiscrepancy {
    pub dimension: &'static str,
    pub measured: f32,
    pub expected: f32,
    pub delta: f32,
    pub tolerance: f32,
}

impl core::fmt::Display for GeometryDiscrepancy {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} discrepancy: measured {:.4} pt, expected {:.4} pt (delta {:.4} > tolerance {:.4} pt)",
            self.dimension, self.measured, self.expected, self.delta, self.tolerance
        )
    }
}

/// Measures the authoritative mathematical geometry of a LaTeX mathematical formula.
pub fn measure_formula_geometry(
    latex_math: &str,
    family: FontFamily,
    font_size: f32,
    style: MathStyle,
) -> Result<MathFormulaGeometry, Error> {
    let node = crate::math::math_parser::parse_math_str(latex_math);

    let mut ctx = MathLayoutContext::new(font_size, family, None, None, None, style);
    let engine = ctx.opentype_engine();
    let b = layout_node(&mut ctx, &node, style, 0.0, 0.0);

    let axis_height = engine.axis_height();
    let rule_thickness = engine.fraction_rule_thickness();

    Ok(MathFormulaGeometry {
        width: b.w,
        height: b.h,
        depth: b.d,
        axis_height,
        rule_thickness,
        child_count: 1,
    })
}

/// Verifies whether the measured formula geometry matches expected reference values within `tolerance_pt`.
pub fn verify_geometry_tolerance(
    measured: &MathFormulaGeometry,
    expected: &MathFormulaGeometry,
    tolerance_pt: f32,
) -> Result<(), GeometryDiscrepancy> {
    let w_diff = (measured.width - expected.width).abs();
    if w_diff > tolerance_pt {
        return Err(GeometryDiscrepancy {
            dimension: "Width",
            measured: measured.width,
            expected: expected.width,
            delta: w_diff,
            tolerance: tolerance_pt,
        });
    }

    let h_diff = (measured.height - expected.height).abs();
    if h_diff > tolerance_pt {
        return Err(GeometryDiscrepancy {
            dimension: "Height",
            measured: measured.height,
            expected: expected.height,
            delta: h_diff,
            tolerance: tolerance_pt,
        });
    }

    let d_diff = (measured.depth - expected.depth).abs();
    if d_diff > tolerance_pt {
        return Err(GeometryDiscrepancy {
            dimension: "Depth",
            measured: measured.depth,
            expected: expected.depth,
            delta: d_diff,
            tolerance: tolerance_pt,
        });
    }

    let axis_diff = (measured.axis_height - expected.axis_height).abs();
    if axis_diff > tolerance_pt {
        return Err(GeometryDiscrepancy {
            dimension: "Axis height",
            measured: measured.axis_height,
            expected: expected.axis_height,
            delta: axis_diff,
            tolerance: tolerance_pt,
        });
    }

    let rule_diff = (measured.rule_thickness - expected.rule_thickness).abs();
    if rule_diff > tolerance_pt {
        return Err(GeometryDiscrepancy {
            dimension: "Rule thickness",
            measured: measured.rule_thickness,
            expected: expected.rule_thickness,
            delta: rule_diff,
            tolerance: tolerance_pt,
        });
    }

    Ok(())
}
