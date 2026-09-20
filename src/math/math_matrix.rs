//! Matrix, grid layout, and vertical centering engine (TeX Appendix G Rule 12 & 20).
//!
//! Provides Structure of Arrays column buffers, vectorized row alignment,
//! Rule 12 \vcenter math axis centering, and delimiter binders.

use crate::math::math_ast::{ColAlign, MatrixDelim};
use crate::math::math_box::MathBox;

/// Structure of Arrays (SoA) layout for matrix column metrics.
///
/// Enables SIMD-friendly vectorized column alignment and accumulation.
#[derive(Clone, Debug, Default)]
pub struct MatrixColumnSoA {
    /// Maximum width of each column in points.
    pub widths: Vec<f32>,
    /// Alignment of each column (Left, Center, Right).
    pub aligns: Vec<ColAlign>,
    /// Precomputed horizontal origin offset for each column.
    pub offsets: Vec<f32>,
}

impl MatrixColumnSoA {
    #[inline(always)]
    pub fn with_capacity(cols: usize) -> Self {
        Self {
            widths: vec![0.0; cols],
            aligns: vec![ColAlign::Center; cols],
            offsets: vec![0.0; cols],
        }
    }

    /// Precomputes cumulative horizontal column offsets with column spacing.
    #[inline(always)]
    pub fn compute_offsets(&mut self, col_spacing: f32) -> f32 {
        let mut curr_x = 0.0f32;
        for i in 0..self.widths.len() {
            self.offsets[i] = curr_x;
            curr_x += self.widths[i] + col_spacing;
        }
        if !self.widths.is_empty() {
            curr_x - col_spacing
        } else {
            0.0
        }
    }
}

/// Vectorized row baseline alignment engine for matrix rows.
pub struct RowBaselineAligner;

impl RowBaselineAligner {
    /// Computes max ink height and max ink depth across cells in a matrix row.
    #[inline(always)]
    pub fn compute_row_extents(cell_boxes: &[MathBox]) -> (f32, f32) {
        let mut max_h = 0.0f32;
        let mut max_d = 0.0f32;
        for b in cell_boxes {
            if b.h > max_h {
                max_h = b.h;
            }
            if b.d > max_d {
                max_d = b.d;
            }
        }
        (max_h, max_d)
    }
}

/// TeX Appendix G Rule 12 engine centering vertical material on the math axis.
pub struct VcenterGeometryEngine;

impl VcenterGeometryEngine {
    /// Calculates vertical shift $\Delta$ to center a box on the math axis:
    /// $\Delta = \sigma_{22} - \frac{h - d}{2}$
    #[inline(always)]
    pub fn compute_centering_shift(axis_h: f32, box_h: f32, box_d: f32) -> (f32, f32, f32) {
        let delta = axis_h - 0.5 * (box_h - box_d);
        let total_extent = 0.5 * (box_h + box_d);
        let new_h = axis_h + total_extent;
        let new_d = total_extent - axis_h;
        (delta, new_h, new_d)
    }
}

/// Binder resolving delimiters for standard matrix environments.
pub struct MatrixDelimBinder;

impl MatrixDelimBinder {
    /// Resolves opening and closing delimiter characters for a `MatrixDelim`.
    #[inline(always)]
    pub fn resolve_delims(delim: MatrixDelim) -> (&'static str, &'static str) {
        match delim {
            MatrixDelim::None => ("", ""),
            MatrixDelim::Parentheses => ("(", ")"),
            MatrixDelim::Brackets => ("[", "]"),
            MatrixDelim::Braces => ("{", "}"),
            MatrixDelim::VBar => ("|", "|"),
            MatrixDelim::DoubleVBar => ("\u{2016}", "\u{2016}"),
            MatrixDelim::LeftBrace => ("{", "."),
        }
    }
}

/// Compressor for sparse matrix grids, avoiding memory allocations for empty cells.
pub struct SparseGridCompressor;

impl SparseGridCompressor {
    /// Returns true if a cell box is zero-sized and has no visual paint content.
    #[inline(always)]
    pub fn is_empty_cell(b: &MathBox) -> bool {
        b.w == 0.0 && b.h == 0.0 && b.d == 0.0
    }
}
