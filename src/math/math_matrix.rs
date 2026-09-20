//! Matrix, grid layout, and vertical centering engine (TeX Appendix G Rule 12 & 20).
//!
//! Provides Structure of Arrays column buffers, vectorized row alignment,
//! Rule 12 \vcenter math axis centering, and delimiter binders.

use crate::math::math_ast::{ColAlign, MatrixDelim, MatrixEnvKind};
use crate::math::math_box::MathBox;

/// Maximum stack columns before falling back to heap allocation.
pub const MATRIX_STACK_COLS: usize = 16;
/// Maximum stack rows before falling back to heap allocation.
pub const MATRIX_STACK_ROWS: usize = 16;
/// Maximum stack cells before falling back to heap allocation.
pub const MATRIX_STACK_CELLS: usize = 64;

/// Structure of Arrays (SoA) buffer for matrix column widths, row heights, and row depths.
///
/// 64-byte aligned to maximize L1 CPU cache line packing and eliminate heap allocations
/// across standard mathematical matrices and multiline environments.
#[repr(C, align(64))]
pub struct MatrixGridBuffer {
    pub col_widths: [f32; MATRIX_STACK_COLS],
    pub row_heights: [f32; MATRIX_STACK_ROWS],
    pub row_depths: [f32; MATRIX_STACK_ROWS],
    pub cell_widths: [f32; MATRIX_STACK_CELLS],
}

impl Default for MatrixGridBuffer {
    #[inline(always)]
    fn default() -> Self {
        Self {
            col_widths: [0.0; MATRIX_STACK_COLS],
            row_heights: [0.0; MATRIX_STACK_ROWS],
            row_depths: [0.0; MATRIX_STACK_ROWS],
            cell_widths: [0.0; MATRIX_STACK_CELLS],
        }
    }
}

const _: () = assert!(core::mem::align_of::<MatrixGridBuffer>() == 64);
const _: () = assert!(core::mem::size_of::<MatrixGridBuffer>() == 448);

/// Canonical typographic metrics engine for matrices, cases, and multiline environments.
pub struct MatrixLayoutMetricsEngine;

impl MatrixLayoutMetricsEngine {
    /// Computes canonical minimum row height, minimum row depth, and inter-row separation
    /// according to TeX Live / AMS-LaTeX mathematical layout rules.
    #[inline(always)]
    pub fn row_parameters(env_kind: MatrixEnvKind, font_size: f32) -> (f32, f32, f32) {
        match env_kind {
            MatrixEnvKind::Cases => {
                // LaTeX cases uses \arraystretch = 1.2 on standard strut
                let min_rh = 1.2 * 0.84 * font_size; // ~10.08pt at 10pt
                let min_rd = 1.2 * 0.36 * font_size; // ~4.32pt at 10pt
                let row_sep = 0.0;
                (min_rh, min_rd, row_sep)
            }
            MatrixEnvKind::Aligned | MatrixEnvKind::Gather | MatrixEnvKind::Multline => {
                // AMS multiline environments use standard strut + \jot (3pt = 0.30em)
                let min_rh = 0.84 * font_size; // ~8.40pt at 10pt
                let min_rd = 0.36 * font_size; // ~3.60pt at 10pt
                let row_sep = 0.30 * font_size; // ~3.00pt (\jot) at 10pt
                (min_rh, min_rd, row_sep)
            }
            MatrixEnvKind::Matrix => {
                // Standard TeX matrix uses \strutbox (8.4pt + 3.6pt = 12pt baselineskip)
                let min_rh = 0.84 * font_size;
                let min_rd = 0.36 * font_size;
                let row_sep = 0.0;
                (min_rh, min_rd, row_sep)
            }
            MatrixEnvKind::SmallMatrix => {
                // AMS-LaTeX smallmatrix uses compact script-size metrics
                let min_rh = 0.60 * font_size;
                let min_rd = 0.25 * font_size;
                let row_sep = 0.0;
                (min_rh, min_rd, row_sep)
            }
        }
    }

    /// Computes canonical inter-column separation following column `col_idx`.
    #[inline(always)]
    pub fn col_gap(env_kind: MatrixEnvKind, col_idx: usize, font_size: f32) -> f32 {
        match env_kind {
            MatrixEnvKind::Aligned => {
                if col_idx.is_multiple_of(2) {
                    0.0 // Exactly 0 between relation/right and left
                } else {
                    1.0 * font_size // 1em (\alignedspace) between equation pairs
                }
            }
            MatrixEnvKind::Cases => {
                1.0 * font_size // \quad = 1em
            }
            MatrixEnvKind::Gather => {
                1.0 * font_size // \quad = 1em
            }
            MatrixEnvKind::Multline => 0.0,
            MatrixEnvKind::SmallMatrix => {
                0.50 * font_size // Compact inter-column separation
            }
            MatrixEnvKind::Matrix => {
                1.0 * font_size // 2 * \arraycolsep = 10pt = 1em
            }
        }
    }
}

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
            MatrixDelim::DoubleVBar => ("||", "||"),
            MatrixDelim::LeftBrace => ("{", ""),
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
