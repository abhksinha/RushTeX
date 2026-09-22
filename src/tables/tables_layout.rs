//! Layout calculation, multi-page streaming, and vector painting for tables.

use crate::backend::fonts::Face;
use crate::layout::engine::{explode_inlines_into_family, CellSpan, Layouter, Paint, CELL_PAD};
use crate::tables::tables_model::{TableData, TableRow, TableRule};
use crate::tables::tables_spec::{ColAlign, ColSpec};

pub(crate) fn layout_table<'a>(layouter: &mut Layouter<'a>, table_data: &'a TableData<'a>) {
    let cols = &table_data.cols;
    let rows = &table_data.rows;
    if cols.is_empty() {
        return;
    }
    let n = cols.len();

    // 1. Compute natural column widths (stack buffer sized for up to 64 columns)
    let mut widths_buf = [0.0f32; 64];
    let mut col_w_buf = [0.0f32; 64];
    let (mut widths_heap, mut col_w_heap);
    let (widths, col_w) = if n <= 64 {
        (&mut widths_buf[..n], &mut col_w_buf[..n])
    } else {
        widths_heap = vec![0.0f32; n];
        col_w_heap = vec![0.0f32; n];
        (widths_heap.as_mut_slice(), col_w_heap.as_mut_slice())
    };

    // Initialize with fixed widths if specified via p{width}
    for (i, &fw) in table_data.col_widths.iter().enumerate().take(n) {
        if let Some(w) = fw {
            widths[i] = (w - 2.0 * CELL_PAD).max(1.0);
        }
    }

    // Measure cell contents
    for row in rows {
        let mut c_idx = 0;
        for cell in &row.cells {
            if c_idx >= n {
                break;
            }
            let colspan = cell.colspan.max(1) as usize;
            let cell_w = layouter.measure_inlines_face_size(
                &cell.inlines,
                Face::Roman,
                layouter.base_font_size,
            );
            if colspan == 1 {
                if table_data
                    .col_widths
                    .get(c_idx)
                    .copied()
                    .flatten()
                    .is_none()
                {
                    widths[c_idx] = widths[c_idx].max(cell_w);
                }
            } else {
                // Distribute multicolumn excess across spanned auto columns
                let span_end = (c_idx + colspan).min(n);
                let current_span_w: f32 = widths[c_idx..span_end].iter().sum();
                if cell_w > current_span_w {
                    let excess = (cell_w - current_span_w) / (span_end - c_idx) as f32;
                    for w in &mut widths[c_idx..span_end] {
                        *w += excess;
                    }
                }
            }
            c_idx += colspan;
        }
    }

    // 2. Add cell horizontal padding
    let avail_w = (layouter.right - layouter.right_skip) - (layouter.left + layouter.left_skip);
    let mut raw_table_w = 0.0f32;
    for i in 0..n {
        col_w[i] = if let Some(fw) = table_data.col_widths.get(i).copied().flatten() {
            fw
        } else {
            widths[i] + 2.0 * CELL_PAD
        };
        raw_table_w += col_w[i];
    }

    // 3. Auto-scale auto columns if table exceeds available width
    let table_w = if raw_table_w > avail_w && raw_table_w > 0.0 {
        let scale = avail_w / raw_table_w;
        let mut sum_assigned = 0.0f32;
        for (i, w) in col_w.iter_mut().enumerate().take(n) {
            if table_data.col_widths.get(i).copied().flatten().is_some() {
                sum_assigned += *w;
            } else if i == n - 1 {
                *w = (avail_w - sum_assigned).max(2.0 * CELL_PAD);
            } else {
                *w = (*w * scale).max(2.0 * CELL_PAD);
                sum_assigned += *w;
            }
        }
        avail_w
    } else {
        raw_table_w
    };

    // Prefix sums of column offsets for O(1) cell span and rule width calculation
    let mut col_offsets_buf = [0.0f32; 65];
    let mut col_offsets_heap;
    let col_offsets = if n <= 64 {
        &mut col_offsets_buf[..=n]
    } else {
        col_offsets_heap = vec![0.0f32; n + 1];
        col_offsets_heap.as_mut_slice()
    };
    col_offsets[0] = 0.0;
    for i in 0..n {
        col_offsets[i + 1] = col_offsets[i] + col_w[i];
    }

    let origin_x = layouter.left + layouter.left_skip;

    // 4. Render Caption Above (if present)
    if table_data.caption_above {
        if let Some(caption_inls) = table_data.caption.as_deref() {
            render_caption(layouter, caption_inls, origin_x, table_w);
        }
    }

    let mut y_top = layouter.y;
    let rule_right = table_data.rule_right;
    let rule_bottom = table_data.rule_bottom
        || (rows.first().is_some_and(|r| r.rule_above)
            && (cols.first().is_some_and(|c| c.rule_left()) || rule_right));

    // 5. Render rows with row-level pagination and longtable header repetition
    let mut row_idx = 0;
    while row_idx < rows.len() {
        let row = &rows[row_idx];
        let row_h = prepare_row_cells(layouter, row, n, col_w, col_offsets);

        let broken = layouter.ensure_room(row_h);
        if broken {
            y_top = layouter.y;
            // Longtable: Repeat headers on new page!
            if table_data.is_longtable
                && table_data.header_row_count > 0
                && row_idx >= table_data.header_row_count
            {
                for h_row in rows.iter().take(table_data.header_row_count) {
                    let h_row_h = prepare_row_cells(layouter, h_row, n, col_w, col_offsets);
                    paint_prepared_row(
                        layouter, h_row, cols, col_w, col_offsets, origin_x, table_w, rule_right, &mut y_top,
                        h_row_h,
                    );
                }
                // Re-prepare the current row since header rendering overwrote layouter's cell buffers
                prepare_row_cells(layouter, row, n, col_w, col_offsets);
            }
        }

        paint_prepared_row(
            layouter, row, cols, col_w, col_offsets, origin_x, table_w, rule_right, &mut y_top, row_h,
        );

        row_idx += 1;
    }

    // 6. Emit Bottom Rules
    for rule in &table_data.bottom_rules {
        emit_table_rule(layouter, rule, origin_x, col_offsets, table_w, &mut y_top);
    }
    if rule_bottom && table_data.bottom_rules.is_empty() {
        let rule_t = 0.6f32;
        if !layouter.is_dry_run {
            layouter.push_op(Paint::Rule {
                x: origin_x,
                y: y_top - rule_t,
                w: table_w,
                h: rule_t,
                fill: true,
            });
        }
        y_top -= rule_t;
    }

    // 7. Render Caption Below (if present)
    if !table_data.caption_above {
        if let Some(caption_inls) = table_data.caption.as_deref() {
            y_top -= 6.0;
            layouter.y = y_top;
            render_caption(layouter, caption_inls, origin_x, table_w);
            y_top = layouter.y;
        }
    }

    layouter.y = y_top - layouter.par_skip;
}

fn prepare_row_cells<'a>(
    layouter: &mut Layouter<'a>,
    row: &'a TableRow<'a>,
    n: usize,
    col_w: &[f32],
    col_offsets: &[f32],
) -> f32 {
    layouter.cell_words_soa.clear();
    layouter.cell_lines.clear();
    layouter.cell_line_spans.clear();

    let mut max_lines = 1u16;
    let mut c_idx = 0;
    for cell in &row.cells {
        if c_idx >= n {
            break;
        }
        let colspan = cell.colspan.max(1) as usize;
        let span_end = (c_idx + colspan).min(n);
        let item_w = if colspan == 1 {
            col_w[c_idx]
        } else {
            col_offsets[span_end] - col_offsets[c_idx]
        };
        let inner_w = (item_w - 2.0 * CELL_PAD).max(1.0);

        let start_idx = layouter.cell_words_soa.len();
        explode_inlines_into_family(
            &cell.inlines,
            Face::Roman,
            layouter.base_font_size,
            &mut layouter.cell_words_soa,
            layouter.font_family,
        );
        let end_idx = layouter.cell_words_soa.len();

        if start_idx == end_idx {
            layouter.cell_line_spans.push(CellSpan {
                start: layouter.cell_lines.len() as u16,
                count: 0,
            });
        } else {
            let lines_start = layouter.cell_lines.len();
            layouter.break_cell_words_into_lines(start_idx, end_idx, inner_w);
            let lines_count = (layouter.cell_lines.len() - lines_start) as u16;
            layouter.cell_line_spans.push(CellSpan {
                start: lines_start as u16,
                count: lines_count,
            });
            max_lines = max_lines.max(lines_count);
        }
        c_idx += colspan;
    }

    (max_lines as f32) * layouter.base_leading + 4.0
}

#[allow(clippy::too_many_arguments)]
fn paint_prepared_row<'a>(
    layouter: &mut Layouter<'a>,
    row: &'a TableRow<'a>,
    cols: &[ColSpec],
    col_w: &[f32],
    col_offsets: &[f32],
    origin_x: f32,
    table_w: f32,
    rule_right: bool,
    y_top: &mut f32,
    row_h: f32,
) {
    let n = cols.len();

    // 1. Draw rules above row
    for rule in &row.rules_above {
        emit_table_rule(layouter, rule, origin_x, col_offsets, table_w, y_top);
    }
    if row.rule_above && row.rules_above.is_empty() {
        let rule_t = 0.6f32;
        if !layouter.is_dry_run {
            layouter.push_op(Paint::Rule {
                x: origin_x,
                y: *y_top - rule_t,
                w: table_w,
                h: rule_t,
                fill: true,
            });
        }
        *y_top -= rule_t;
    }

    let row_bottom = *y_top - row_h;
    let rule_t = 0.6f32;

    // 2. Paint vertical rules and cell contents
    let mut cell_idx = 0;
    let mut c_idx = 0;

    while c_idx < n && cell_idx < row.cells.len() {
        let cell = &row.cells[cell_idx];
        let colspan = cell.colspan.max(1) as usize;
        let span_end = (c_idx + colspan).min(n);
        let x = origin_x + col_offsets[c_idx];
        let item_w = if colspan == 1 {
            col_w[c_idx]
        } else {
            col_offsets[span_end] - col_offsets[c_idx]
        };
        let inner_w = (item_w - 2.0 * CELL_PAD).max(1.0);

        // Draw left vertical rule
        if cols[c_idx].rule_left() || cell.rule_left {
            let rule_x = if c_idx == 0 { x } else { x - rule_t * 0.5 };
            if !layouter.is_dry_run {
                layouter.push_op(Paint::Rule {
                    x: rule_x,
                    y: row_bottom,
                    w: rule_t,
                    h: row_h,
                    fill: true,
                });
            }
        }

        // Draw cell lines
        let span = layouter.cell_line_spans[cell_idx];
        let span_start = span.start as usize;
        let span_count = span.count as usize;
        let align = cell.align.unwrap_or_else(|| cols[c_idx].align());

        for l in 0..span_count {
            let line_desc = layouter.cell_lines[span_start + l];
            let baseline =
                (*y_top - layouter.base_leading + 1.5) - (l as f32) * layouter.base_leading;
            let inner_x = match align {
                ColAlign::Left => x + CELL_PAD,
                ColAlign::Right => x + item_w - CELL_PAD - line_desc.line_w,
                ColAlign::Center => x + CELL_PAD + (inner_w - line_desc.line_w) * 0.5,
            };
            layouter.paint_cell_line(
                line_desc.start_word as usize,
                (line_desc.start_word + line_desc.word_count) as usize,
                inner_x.max(x + 1.0),
                baseline,
            );
        }

        // Draw right vertical rule for multicolumn
        if cell.rule_right && !layouter.is_dry_run {
            layouter.push_op(Paint::Rule {
                x: x + item_w - rule_t * 0.5,
                y: row_bottom,
                w: rule_t,
                h: row_h,
                fill: true,
            });
        }

        c_idx += colspan;
        cell_idx += 1;
    }

    if rule_right && !layouter.is_dry_run {
        layouter.push_op(Paint::Rule {
            x: origin_x + table_w - rule_t,
            y: row_bottom,
            w: rule_t,
            h: row_h,
            fill: true,
        });
    }

    *y_top = row_bottom;
}

fn emit_table_rule<'a>(
    layouter: &mut Layouter<'a>,
    rule: &TableRule,
    origin_x: f32,
    col_offsets: &[f32],
    table_w: f32,
    y_top: &mut f32,
) {
    match rule {
        TableRule::HLine => {
            let h = 0.6f32;
            if !layouter.is_dry_run {
                layouter.push_op(Paint::Rule {
                    x: origin_x,
                    y: *y_top - h,
                    w: table_w,
                    h,
                    fill: true,
                });
            }
            *y_top -= h;
        }
        TableRule::TopRule => {
            let h = 1.2f32;
            if !layouter.is_dry_run {
                layouter.push_op(Paint::Rule {
                    x: origin_x,
                    y: *y_top - h,
                    w: table_w,
                    h,
                    fill: true,
                });
            }
            *y_top -= h + 2.0;
        }
        TableRule::MidRule => {
            let h = 0.6f32;
            *y_top -= 1.5;
            if !layouter.is_dry_run {
                layouter.push_op(Paint::Rule {
                    x: origin_x,
                    y: *y_top - h,
                    w: table_w,
                    h,
                    fill: true,
                });
            }
            *y_top -= h + 1.5;
        }
        TableRule::BottomRule => {
            let h = 1.2f32;
            *y_top -= 2.0;
            if !layouter.is_dry_run {
                layouter.push_op(Paint::Rule {
                    x: origin_x,
                    y: *y_top - h,
                    w: table_w,
                    h,
                    fill: true,
                });
            }
            *y_top -= h;
        }
        TableRule::CLine { start_col, end_col } => {
            let h = 0.4f32;
            if !layouter.is_dry_run {
                let (rx, rw) =
                    col_range_extent(origin_x, col_offsets, *start_col as usize, *end_col as usize);
                layouter.push_op(Paint::Rule {
                    x: rx,
                    y: *y_top - h,
                    w: rw,
                    h,
                    fill: true,
                });
            }
            *y_top -= h;
        }
        TableRule::CMidRule {
            start_col,
            end_col,
            trim_left,
            trim_right,
        } => {
            let h = 0.5f32;
            if !layouter.is_dry_run {
                let (mut rx, mut rw) =
                    col_range_extent(origin_x, col_offsets, *start_col as usize, *end_col as usize);
                if *trim_left {
                    rx += 2.0;
                    rw = (rw - 2.0).max(1.0);
                }
                if *trim_right {
                    rw = (rw - 2.0).max(1.0);
                }
                layouter.push_op(Paint::Rule {
                    x: rx,
                    y: *y_top - h,
                    w: rw,
                    h,
                    fill: true,
                });
            }
            *y_top -= h;
        }
    }
}

#[inline]
fn col_range_extent(origin_x: f32, col_offsets: &[f32], start_col: usize, end_col: usize) -> (f32, f32) {
    let n = col_offsets.len().saturating_sub(1);
    let s = (start_col.max(1) - 1).min(n);
    let e = end_col.max(s).min(n);

    let offset_x = col_offsets[s];
    let span_w = col_offsets[e] - offset_x;
    (origin_x + offset_x, span_w)
}

fn render_caption<'a>(
    layouter: &mut Layouter<'a>,
    caption_inls: &'a [crate::frontend::ast::Inline<'a>],
    origin_x: f32,
    table_w: f32,
) {
    layouter.table_counter += 1;
    let n = layouter.table_counter;
    layouter.ensure_room(layouter.base_leading * 2.0);

    if layouter.current_tbl_idx < layouter.registry.table_entries.len() {
        layouter.registry.table_entries[layouter.current_tbl_idx].page =
            (layouter.pages.len() + 1) as u32;
        layouter.current_tbl_idx += 1;
    }

    let tag_w = 40.0;
    let inls_w =
        layouter.measure_inlines_face_size(caption_inls, Face::Roman, layouter.base_font_size);
    let total_cap_w = tag_w + inls_w;
    let cap_x = origin_x + (table_w - total_cap_w).max(0.0) * 0.5;

    if !layouter.is_dry_run {
        layouter.push_op(Paint::TableCaptionTag {
            x: cap_x,
            y: layouter.y,
            n,
        });
        layouter.paint_inlines_at_size(
            caption_inls,
            Face::Roman,
            layouter.base_font_size,
            cap_x + tag_w,
            layouter.y,
        );
    }
    layouter.y -= layouter.base_leading + 4.0;
}
