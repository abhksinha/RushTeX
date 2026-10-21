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
    pub col_offsets: [f32; MATRIX_STACK_COLS],
    pub row_heights: [f32; MATRIX_STACK_ROWS],
    pub row_depths: [f32; MATRIX_STACK_ROWS],
    pub row_baseline_offsets: [f32; MATRIX_STACK_ROWS],
    pub cell_widths: [f32; MATRIX_STACK_CELLS],
}

impl Default for MatrixGridBuffer {
    #[inline(always)]
    fn default() -> Self {
        Self {
            col_widths: [0.0; MATRIX_STACK_COLS],
            col_offsets: [0.0; MATRIX_STACK_COLS],
            row_heights: [0.0; MATRIX_STACK_ROWS],
            row_depths: [0.0; MATRIX_STACK_ROWS],
            row_baseline_offsets: [0.0; MATRIX_STACK_ROWS],
            cell_widths: [0.0; MATRIX_STACK_CELLS],
        }
    }
}

impl MatrixGridBuffer {
    /// Computes column offsets, total matrix width, row baseline offsets, and total content height
    /// directly within the 64-byte aligned stack buffer in a single SIMD-friendly pass.
    #[inline(always)]
    pub fn compute_metrics(
        &mut self,
        num_cols: usize,
        num_rows: usize,
        env_kind: MatrixEnvKind,
        font_size: f32,
        row_sep: f32,
    ) -> (f32, f32) {
        let matrix_w = MatrixLayoutMetricsEngine::compute_col_offsets_and_width(
            &self.col_widths[..num_cols],
            &mut self.col_offsets[..num_cols],
            env_kind,
            font_size,
            num_rows,
            font_size,
        );
        let total_content_h = MatrixLayoutMetricsEngine::compute_row_baseline_offsets_and_total_height(
            &self.row_heights[..num_rows],
            &self.row_depths[..num_rows],
            &mut self.row_baseline_offsets[..num_rows],
            row_sep,
            MatrixLayoutMetricsEngine::openup_baselineskip(env_kind, font_size),
            &[],
        );
        (matrix_w, total_content_h)
    }
}

const _: () = assert!(core::mem::align_of::<MatrixGridBuffer>() == 64);
const _: () = assert!(core::mem::size_of::<MatrixGridBuffer>() == 576);

/// Canonical typographic metrics engine for matrices, cases, and multiline environments.
pub struct MatrixLayoutMetricsEngine;

impl MatrixLayoutMetricsEngine {
    /// Computes canonical minimum row height, minimum row depth, and inter-row separation
    /// parameterized by the active font's OpenType mathematical constants.
    #[inline(always)]
    pub fn row_parameters_font(
        env_kind: MatrixEnvKind,
        font_size: f32,
        constants: Option<&crate::math::fonts::OpenTypeMathConstants>,
    ) -> (f32, f32, f32) {
        let (base_rh, base_rd) = if let Some(c) = constants {
            let scale = font_size / 1000.0;
            let axis = c.axis_height as f32 * scale;
            let strut_h = 0.84 * font_size + (axis - 0.25 * font_size) * 0.1;
            let strut_d = 0.36 * font_size;
            (strut_h, strut_d)
        } else {
            // `\strutbox`: .7 and .3 of `\baselineskip` (1.2em), as `scan_dimen` multiplies.
            let em = crate::backend::scaled::em_mul;
            (em(0.7, 1.2 * font_size), em(0.3, 1.2 * font_size))
        };

        match env_kind {
            MatrixEnvKind::Cases => {
                // LaTeX cases uses \arraystretch = 1.2 on standard strut
                let em = crate::backend::scaled::em_mul;
                let min_rh = em(1.2, base_rh);
                let min_rd = em(1.2, base_rd);
                let row_sep = 0.0;
                (min_rh, min_rd, row_sep)
            }
            MatrixEnvKind::Aligned | MatrixEnvKind::AlignedAt | MatrixEnvKind::Gather | MatrixEnvKind::Multline => {
                // AMS multiline environments use standard strut + \jot (3pt = 0.30em)
                let row_sep = 0.30 * font_size;
                (base_rh, base_rd, row_sep)
            }
            MatrixEnvKind::Matrix | MatrixEnvKind::Array => {
                let row_sep = 0.0;
                (base_rh, base_rd, row_sep)
            }
            MatrixEnvKind::SmallMatrix | MatrixEnvKind::Subarray | MatrixEnvKind::Overlay | MatrixEnvKind::Vereq => {
                let min_rh = 0.60 * font_size;
                let min_rd = 0.25 * font_size;
                let row_sep = 0.0;
                (min_rh, min_rd, row_sep)
            }
        }
    }

    /// Backwards-compatible convenience wrapper around `row_parameters_font`.
    #[inline(always)]
    pub fn row_parameters(env_kind: MatrixEnvKind, font_size: f32) -> (f32, f32, f32) {
        Self::row_parameters_font(env_kind, font_size, None)
    }

    /// Computes canonical inter-column separation following column `col_idx`.
    #[inline(always)]
    pub fn col_gap(env_kind: MatrixEnvKind, col_idx: usize, font_size: f32, quad: f32) -> f32 {
        match env_kind {
            // Between equation pairs `aligned` puts 2\minalignsep (20pt, measured against
            // amsmath), `alignedat` none.
            MatrixEnvKind::Aligned => {
                if col_idx.is_multiple_of(2) {
                    0.0
                } else {
                    2.0 * font_size
                }
            }
            MatrixEnvKind::AlignedAt => 0.0,
            // `\quad`: 1em of the text font.
            MatrixEnvKind::Cases => quad,
            MatrixEnvKind::Gather => {
                1.0 * font_size // \quad = 1em
            }
            MatrixEnvKind::Multline => 0.0,
            MatrixEnvKind::SmallMatrix | MatrixEnvKind::Subarray | MatrixEnvKind::Overlay | MatrixEnvKind::Vereq => {
                0.50 * font_size // Compact inter-column separation
            }
            MatrixEnvKind::Matrix | MatrixEnvKind::Array => {
                1.0 * font_size // 2 * \arraycolsep = 10pt = 1em
            }
        }
    }

    /// Computes cumulative column offsets and total matrix width.
    ///
    /// Evaluates inter-column spacing in a single pass according to environment rules
    /// (e.g., Aligned equation pairs, Multline gaps, Cases quad, etc.).
    #[inline(always)]
    pub fn compute_col_offsets_and_width(
        col_widths: &[f32],
        col_offsets: &mut [f32],
        env_kind: MatrixEnvKind,
        font_size: f32,
        num_rows: usize,
        quad: f32,
    ) -> f32 {
        let num_cols = col_widths.len();
        if num_cols == 0 {
            return 0.0;
        }

        // `array` keeps the \arraycolsep (0.5em) that `matrix` removes with `@{}`.
        let outer_pad = if env_kind == MatrixEnvKind::Array { 0.5 * font_size } else { 0.0 };
        let mut curr_x = outer_pad;
        let multline_gap = 1.0 * font_size;

        for c in 0..num_cols {
            col_offsets[c] = curr_x;
            let gap = Self::col_gap(env_kind, c, font_size, quad);
            curr_x += col_widths[c] + gap;
        }

        let mut matrix_w: f32 = col_widths.iter().sum::<f32>() + 2.0 * outer_pad;
        if env_kind == MatrixEnvKind::Multline {
            if num_rows > 1 {
                matrix_w += 2.0 * multline_gap;
            }
        } else {
            for c in 0..num_cols.saturating_sub(1) {
                matrix_w += Self::col_gap(env_kind, c, font_size, quad);
            }
        }

        matrix_w
    }

    /// `\baselineskip` of the rows of an amsmath display alignment, which interline glue
    /// separates (`\openup\jot`); zero for arrays, whose rows touch.
    #[inline(always)]
    pub fn openup_baselineskip(env_kind: MatrixEnvKind, font_size: f32) -> f32 {
        match env_kind {
            MatrixEnvKind::Aligned | MatrixEnvKind::AlignedAt | MatrixEnvKind::Gather | MatrixEnvKind::Multline => {
                1.2 * font_size
            }
            _ => 0.0,
        }
    }

    /// Computes cumulative row baseline offsets from top and total content height.
    ///
    /// With `baselineskip > 0` the rows are lines of a vertical list after `\openup` by
    /// `row_sep` (`\jot`): `\baselineskip`, `\lineskip` (1pt) and `\lineskiplimit` (0pt)
    /// each grow by `row_sep` (tex.web §679). Otherwise rows are stacked depth to height
    /// with `row_sep` between them.
    #[inline(always)]
    pub fn compute_row_baseline_offsets_and_total_height(
        row_heights: &[f32],
        row_depths: &[f32],
        row_baseline_offsets: &mut [f32],
        row_sep: f32,
        baselineskip: f32,
        row_skips: &[f32],
    ) -> f32 {
        let num_rows = row_heights.len();
        if num_rows == 0 {
            return 0.0;
        }

        // In scaled points, as TeX adds the rows (§679): f32 sums drift by a sp.
        use crate::backend::scaled::{sp_pt, to_sp};
        let sp = |x: f32| to_sp(x) as i64;
        let (sep, bs, lineskip) = (sp(row_sep), sp(baselineskip), sp(baselineskip / 12.0));
        let mut off = sp(row_heights[0]);
        row_baseline_offsets[0] = row_heights[0];
        for r in 1..num_rows {
            let (pd, h) = (sp(row_depths[r - 1]), sp(row_heights[r]));
            let dist = if baselineskip > 0.0 {
                if bs - pd - h >= 0 { bs + sep } else { pd + h + lineskip + sep }
            } else {
                pd + sep + h
            };
            off += dist + row_skips.get(r - 1).map_or(0, |&s| sp(s));
            row_baseline_offsets[r] = sp_pt(off as i32);
        }

        sp_pt((off + sp(row_depths[num_rows - 1])) as i32)
    }

    /// Resolves canonical alignment for a given column index.
    #[inline(always)]
    pub fn resolve_alignment(
        env_kind: MatrixEnvKind,
        col_alignments: &[ColAlign],
        col_idx: usize,
    ) -> ColAlign {
        let fallback_align = match env_kind {
            MatrixEnvKind::Aligned | MatrixEnvKind::AlignedAt => {
                if col_idx.is_multiple_of(2) {
                    ColAlign::Right
                } else {
                    ColAlign::Left
                }
            }
            MatrixEnvKind::Cases => ColAlign::Left,
            MatrixEnvKind::Gather => ColAlign::Center,
            _ => ColAlign::Center,
        };
        col_alignments.get(col_idx).copied().unwrap_or(fallback_align)
    }

    /// Computes horizontal offset within a cell for content alignment.
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    pub fn compute_cell_offset_x(
        env_kind: MatrixEnvKind,
        align: ColAlign,
        col_w: f32,
        cell_w: f32,
        matrix_w: f32,
        r_idx: usize,
        num_rows: usize,
        multline_gap: f32,
    ) -> f32 {
        if env_kind == MatrixEnvKind::Multline {
            if num_rows == 1 {
                (matrix_w - cell_w) * 0.5
            } else if r_idx == 0 {
                multline_gap
            } else if r_idx == num_rows - 1 {
                matrix_w - multline_gap - cell_w
            } else {
                (matrix_w - cell_w) * 0.5
            }
        } else {
            match align {
                ColAlign::Left => 0.0,
                ColAlign::Center => (col_w - cell_w) * 0.5,
                ColAlign::Right => col_w - cell_w,
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
