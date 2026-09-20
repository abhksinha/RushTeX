//! AST data models for tables, rows, cells, and professional rules.

use crate::frontend::ast::Inline;
use crate::tables::tables_spec::{ColAlign, ColSpec};

/// Kinds of horizontal rules within a table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TableRule {
    /// Standard LaTeX `\hline` (0.6 pt).
    HLine,
    /// `booktabs` top boundary rule (`\toprule`, heavy 1.2 pt).
    TopRule,
    /// `booktabs` separator rule (`\midrule`, medium 0.6 pt).
    MidRule,
    /// `booktabs` bottom boundary rule (`\bottomrule`, heavy 1.2 pt).
    BottomRule,
    /// Partial rule across columns `[start_col..=end_col]` (`\cline{i-j}`, 1-based indices).
    CLine { start_col: u16, end_col: u16 },
    /// `booktabs` partial trimmed rule (`\cmidrule(lr){i-j}`).
    CMidRule {
        start_col: u16,
        end_col: u16,
        trim_left: bool,
        trim_right: bool,
    },
}

const _: () = assert!(core::mem::size_of::<TableRule>() <= 8);

/// A single cell within a table row, with support for `\multicolumn` spanning.
///
/// Exactly 32 bytes in size to allow precisely two cells to occupy a single
/// 64-byte L1 CPU cache line with 100% packing density and zero line splits.
#[derive(Debug, Clone)]
pub struct TableCell<'a> {
    pub inlines: Vec<Inline<'a>>,
    pub colspan: u16,
    pub align: Option<ColAlign>,
    pub rule_left: bool,
    pub rule_right: bool,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<TableCell>() == 32);

#[cfg(target_pointer_width = "32")]
const _: () = assert!(core::mem::size_of::<TableCell>() <= 32);

impl<'a> TableCell<'a> {
    #[inline]
    pub fn simple(inlines: Vec<Inline<'a>>) -> Self {
        Self {
            inlines,
            colspan: 1,
            align: None,
            rule_left: false,
            rule_right: false,
        }
    }
}

/// A row within a table.
#[derive(Debug, Clone)]
pub struct TableRow<'a> {
    /// Backward-compatible flag indicating whether any rule precedes this row.
    pub rule_above: bool,
    /// Specific rule types preceding this row (e.g. `\hline`, `\toprule`, `\cline`).
    pub rules_above: Vec<TableRule>,
    /// Cells in this row.
    pub cells: Vec<TableCell<'a>>,
}

/// Complete table data model supporting standard `tabular`, floating `table` with
/// captions, professional `booktabs`, and multi-page streaming `longtable`.
#[derive(Debug, Clone)]
pub struct TableData<'a> {
    pub cols: Vec<ColSpec>,
    pub col_widths: Vec<Option<f32>>,
    pub rows: Vec<TableRow<'a>>,
    pub rule_right: bool,
    pub rule_bottom: bool,
    pub bottom_rules: Vec<TableRule>,
    pub is_longtable: bool,
    pub header_row_count: usize,
    pub first_header_row_count: usize,
    pub footer_row_count: usize,
    pub caption: Option<Vec<Inline<'a>>>,
    pub caption_above: bool,
}
