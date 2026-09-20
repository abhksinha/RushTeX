//! Automated mathematical geometric differential testing and regression tracing.
//!
//! Captures machine-readable geometric traces of mathematical formulas (box dimensions,
//! glyph origins, rule coordinates, baselines, and element counts) and provides
//! differential comparison against reference geometry tolerances.

use crate::backend::fonts::{Face, FontFamily};
use crate::layout::engine::Paint;
use crate::math::math_ast::{MathNode, MathStyle};
use crate::math::math_box::MathBox;
use crate::math::math_layout::render_math;

/// Positioned glyph run record in a mathematical geometry trace.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphPlacement {
    pub x: f32,
    pub y: f32,
    pub face: Face,
    pub size: f32,
    pub text: String,
}

/// Positioned rule (horizontal/vertical bar) record in a mathematical geometry trace.
#[derive(Clone, Debug, PartialEq)]
pub struct RulePlacement {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Machine-readable mathematical geometry snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct MathGeometryTrace {
    pub bounding_box: MathBox,
    pub glyphs: Vec<GlyphPlacement>,
    pub rules: Vec<RulePlacement>,
}

impl MathGeometryTrace {
    /// Total ink width.
    #[inline(always)]
    pub fn width(&self) -> f32 {
        self.bounding_box.w
    }

    /// Total height above baseline.
    #[inline(always)]
    pub fn height(&self) -> f32 {
        self.bounding_box.h
    }

    /// Total depth below baseline.
    #[inline(always)]
    pub fn depth(&self) -> f32 {
        self.bounding_box.d
    }

    /// Total vertical span.
    #[inline(always)]
    pub fn total_height(&self) -> f32 {
        self.bounding_box.total_h()
    }
}

/// Tolerances for geometric differential assertions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeometryTolerance {
    /// Bounding box dimension tolerance in points.
    pub dim_tolerance: f32,
    /// Coordinate placement tolerance in points.
    pub coord_tolerance: f32,
}

impl Default for GeometryTolerance {
    fn default() -> Self {
        Self {
            dim_tolerance: 0.05,
            coord_tolerance: 0.05,
        }
    }
}

impl GeometryTolerance {
    pub const STRICT: Self = Self {
        dim_tolerance: 0.01,
        coord_tolerance: 0.01,
    };

    pub const RELAXED: Self = Self {
        dim_tolerance: 0.20,
        coord_tolerance: 0.20,
    };
}

/// Result of geometric differential comparison.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeometryDiff {
    pub passed: bool,
    pub width_diff: f32,
    pub height_diff: f32,
    pub depth_diff: f32,
    pub glyph_mismatches: usize,
    pub rule_mismatches: usize,
    pub diagnostics: Vec<String>,
}

/// Traces the complete mathematical geometry of an AST node.
pub fn trace_math_geometry(
    node: &MathNode,
    font_size: f32,
    family: FontFamily,
    style: MathStyle,
) -> MathGeometryTrace {
    let mut ops = Vec::with_capacity(32);
    let mut pool = Vec::with_capacity(32);
    let bounding_box = render_math(
        node, font_size, family, style, 0.0, 0.0, &mut ops, &mut pool,
    );

    let mut glyphs = Vec::with_capacity(ops.len());
    let mut rules = Vec::with_capacity(ops.len());

    for op in ops {
        match op {
            Paint::GlyphRun {
                x,
                y,
                face,
                size,
                part_start,
                part_count,
                ..
            } => {
                let start = part_start as usize;
                let end = start + part_count as usize;
                let text = if end <= pool.len() {
                    pool[start..end].concat()
                } else {
                    String::new()
                };
                glyphs.push(GlyphPlacement {
                    x,
                    y,
                    face,
                    size,
                    text,
                });
            }
            Paint::Rule { x, y, w, h, .. } => {
                rules.push(RulePlacement { x, y, w, h });
            }
            _ => {}
        }
    }

    MathGeometryTrace {
        bounding_box,
        glyphs,
        rules,
    }
}

/// Compares an actual mathematical geometry trace against a reference trace.
pub fn compare_geometry(
    actual: &MathGeometryTrace,
    reference: &MathGeometryTrace,
    tolerance: GeometryTolerance,
) -> Result<(), GeometryDiff> {
    let mut diff = GeometryDiff {
        passed: true,
        ..Default::default()
    };

    diff.width_diff = (actual.width() - reference.width()).abs();
    diff.height_diff = (actual.height() - reference.height()).abs();
    diff.depth_diff = (actual.depth() - reference.depth()).abs();

    if diff.width_diff > tolerance.dim_tolerance {
        diff.passed = false;
        diff.diagnostics.push(format!(
            "Width mismatch: actual={:.3}, expected={:.3}, delta={:.3} > tol={:.3}",
            actual.width(),
            reference.width(),
            diff.width_diff,
            tolerance.dim_tolerance
        ));
    }

    if diff.height_diff > tolerance.dim_tolerance {
        diff.passed = false;
        diff.diagnostics.push(format!(
            "Height mismatch: actual={:.3}, expected={:.3}, delta={:.3} > tol={:.3}",
            actual.height(),
            reference.height(),
            diff.height_diff,
            tolerance.dim_tolerance
        ));
    }

    if diff.depth_diff > tolerance.dim_tolerance {
        diff.passed = false;
        diff.diagnostics.push(format!(
            "Depth mismatch: actual={:.3}, expected={:.3}, delta={:.3} > tol={:.3}",
            actual.depth(),
            reference.depth(),
            diff.depth_diff,
            tolerance.dim_tolerance
        ));
    }

    if actual.glyphs.len() != reference.glyphs.len() {
        diff.passed = false;
        diff.glyph_mismatches += actual.glyphs.len().abs_diff(reference.glyphs.len());
        diff.diagnostics.push(format!(
            "Glyph count mismatch: actual={}, expected={}",
            actual.glyphs.len(),
            reference.glyphs.len()
        ));
    } else {
        for (i, (ag, rg)) in actual
            .glyphs
            .iter()
            .zip(reference.glyphs.iter())
            .enumerate()
        {
            let dx = (ag.x - rg.x).abs();
            let dy = (ag.y - rg.y).abs();
            if dx > tolerance.coord_tolerance
                || dy > tolerance.coord_tolerance
                || ag.text != rg.text
            {
                diff.passed = false;
                diff.glyph_mismatches += 1;
                diff.diagnostics.push(format!(
                    "Glyph[{}] mismatch: actual='{}' at ({:.3}, {:.3}), expected='{}' at ({:.3}, {:.3})",
                    i, ag.text, ag.x, ag.y, rg.text, rg.x, rg.y
                ));
            }
        }
    }

    if actual.rules.len() != reference.rules.len() {
        diff.passed = false;
        diff.rule_mismatches += actual.rules.len().abs_diff(reference.rules.len());
        diff.diagnostics.push(format!(
            "Rule count mismatch: actual={}, expected={}",
            actual.rules.len(),
            reference.rules.len()
        ));
    } else {
        for (i, (ar, rr)) in actual.rules.iter().zip(reference.rules.iter()).enumerate() {
            let dx = (ar.x - rr.x).abs();
            let dy = (ar.y - rr.y).abs();
            let dw = (ar.w - rr.w).abs();
            let dh = (ar.h - rr.h).abs();
            if dx > tolerance.coord_tolerance
                || dy > tolerance.coord_tolerance
                || dw > tolerance.dim_tolerance
                || dh > tolerance.dim_tolerance
            {
                diff.passed = false;
                diff.rule_mismatches += 1;
                diff.diagnostics.push(format!(
                    "Rule[{}] mismatch: actual=({:.3}, {:.3}, w={:.3}, h={:.3}), expected=({:.3}, {:.3}, w={:.3}, h={:.3})",
                    i, ar.x, ar.y, ar.w, ar.h, rr.x, rr.y, rr.w, rr.h
                ));
            }
        }
    }

    if diff.passed {
        Ok(())
    } else {
        Err(diff)
    }
}
