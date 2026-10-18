//! Layout calculation, multi-page streaming, and vector painting for tables.

use crate::backend::fonts::Face;
use crate::layout::engine::{explode_inlines_into_family, CellLineRange, CellSpan, Layouter, Paint, CELL_PAD};
use crate::tables::tables_model::{LtSections, TableData, TableRow, TableRule};
use crate::tables::tables_spec::{ColAlign, ColSpec};

/// LaTeX `\arrayrulewidth` (0.4pt): thickness of `\hline` and `|` rules.
const ARRAY_RULE_WIDTH: f32 = 0.4;
/// longtable.sty `\LTpre` / `\LTpost` (`\bigskipamount`, natural width).
const LT_PRE: f32 = 12.0;
const LT_POST: f32 = 12.0;

/// Column geometry shared by every row painter of one table.
struct TableGeom<'g> {
    cols: &'g [ColSpec],
    col_w: &'g [f32],
    col_offsets: &'g [f32],
    origin_x: f32,
    table_w: f32,
    rule_right: bool,
}

pub(crate) fn layout_table<'a>(
    layouter: &mut Layouter<'a>,
    table_data: &TableData<'a>,
) {
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

    // Pre-mask fixed columns up to 64 columns in a single register
    let mut fixed_cols_mask: u64 = 0;
    for (i, &fw) in table_data.col_widths.iter().enumerate().take(n) {
        if let Some(w) = fw {
            // `p{w}` sets the cell's \hsize to w; \tabcolsep is added outside it.
            widths[i] = w.max(1.0);
            if i < 64 {
                fixed_cols_mask |= 1 << i;
            }
        }
    }

    // Two-pass measurement:
    // Pass 1: Measure all single-column cells (colspan <= 1) to establish natural column widths.
    // Multicolumn cells (colspan > 1) are buffered in a stack array (or small heap vector if > 32).
    let mut stack_mc = [(0u16, 0u16, 0.0f32); 32];
    let mut mc_count = 0;
    let mut heap_mc: Option<Vec<(u16, u16, f32)>> = None;

    for row in rows {
        let mut c_idx = 0;
        for cell in &row.cells {
            if c_idx >= n {
                break;
            }
            let colspan = cell.colspan.max(1) as usize;
            let is_fixed = if c_idx < 64 {
                (fixed_cols_mask & (1 << c_idx)) != 0
            } else {
                table_data.col_widths.get(c_idx).copied().flatten().is_some()
            };
            if colspan == 1 && is_fixed {
                c_idx += 1;
                continue;
            }
            let cell_w = layouter.measure_inlines_face_size(
                &cell.inlines,
                Face::Roman,
                layouter.base_font_size,
            );
            if colspan == 1 {
                widths[c_idx] = widths[c_idx].max(cell_w);
            } else {
                if mc_count < 32 && heap_mc.is_none() {
                    stack_mc[mc_count] = (c_idx as u16, colspan as u16, cell_w);
                    mc_count += 1;
                } else {
                    let v = heap_mc.get_or_insert_with(|| {
                        let mut vec = Vec::with_capacity(mc_count + 16);
                        vec.extend_from_slice(&stack_mc[..mc_count]);
                        vec
                    });
                    v.push((c_idx as u16, colspan as u16, cell_w));
                }
            }
            c_idx += colspan;
        }
    }

    // Pass 2: TeX §801. Columns are finished left to right; a span ending at column j
    // widens only column j, by whatever its width exceeds the columns it covers.
    let mc_slice: &[(u16, u16, f32)] = if let Some(ref v) = heap_mc {
        v.as_slice()
    } else {
        &stack_mc[..mc_count]
    };

    for last in 0..n {
        for &(start_col, span, cell_w) in mc_slice {
            let start = start_col as usize;
            let span_end = (start + span as usize).min(n);
            if span_end == 0 || span_end - 1 != last {
                continue;
            }
            // The span also covers the \tabcolsep padding between its columns.
            let current_span_w: f32 =
                widths[start..span_end].iter().sum::<f32>() + (span_end - start - 1) as f32 * 2.0 * CELL_PAD;
            if cell_w > current_span_w {
                widths[last] += cell_w - current_span_w;
            }
        }
    }

    // 2. Add cell horizontal padding
    let avail_w = (layouter.right - layouter.right_skip) - (layouter.left + layouter.left_skip);
    let mut raw_table_w = 0.0f32;
    for i in 0..n {
        col_w[i] = if let Some(fw) = table_data.col_widths.get(i).copied().flatten() {
            fw + 2.0 * CELL_PAD
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

    // Inside a float \@parboxrestore sets \parindent = 0pt, so the tabular starts
    // at the left margin unless \centering is in force.
    let origin_x = if table_data.centered {
        layouter.left + layouter.left_skip + (avail_w - table_w).max(0.0) * 0.5
    } else {
        layouter.left + layouter.left_skip
    };
    // A `[c]` tabular is vertically centred on the math axis; with every row at
    // least one strut (\baselineskip) tall this bounds its height from below.
    let axis = AXIS_HEIGHT_EM * layouter.base_font_size;
    let est_tab_h = rows.len().max(1) as f32 * layouter.base_leading * 0.5 + axis;

    // 4. Render Caption Above (if present)
    if table_data.caption_above {
        if let Some(caption_inls) = table_data.caption.as_deref() {
            // Top of the float box: where a strut-high line above the current
            // baseline would start.
            let float_top = layouter.y + 0.7 * layouter.base_leading;
            let d_cap = render_caption(layouter, caption_inls, float_top, None);
            // \belowcaptionskip is followed by the tabular's paragraph line.
            layouter.y -= d_cap + interline_to_baseline(Some(d_cap), est_tab_h, layouter.base_leading) - est_tab_h;
        }
    }

    if table_data.is_longtable {
        // \LTleft = \LTright = \fill: the longtable is centred in the measure.
        let centred_x = origin_x + (avail_w - table_w).max(0.0) * 0.5;
        let geom = TableGeom {
            cols,
            col_w,
            col_offsets,
            origin_x: centred_x,
            table_w,
            rule_right: table_data.rule_right,
        };
        layout_longtable(layouter, table_data, &geom);
        return;
    }

    let mut y_top = layouter.y;
    let table_top = y_top;
    let rule_right = table_data.rule_right;
    let rule_bottom = table_data.rule_bottom
        || (rows.first().is_some_and(|r| r.rule_above)
            && (cols.first().is_some_and(|c| c.rule_left()) || rule_right));

    // 5. Render rows with row-level pagination and longtable header repetition
    let mut row_math_collector = Vec::new();
    let mut row_idx = 0;
    while row_idx < rows.len() {
        let row = &rows[row_idx];
        let row_h = prepare_row_cells(layouter, row, n, col_w, col_offsets, &mut row_math_collector);
        paint_prepared_row(
            layouter, row, cols, col_w, col_offsets, origin_x, table_w, rule_right, &mut y_top, row_h,
            &row_math_collector, true,
        );
        layouter.y = y_top;

        row_idx += 1;
    }

    // 6. Emit Bottom Rules
    emit_table_rules(layouter, &table_data.bottom_rules, origin_x, col_offsets, table_w, &mut y_top);
    if rule_bottom && table_data.bottom_rules.is_empty() {
        let rule_t = ARRAY_RULE_WIDTH;
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
            // Depth of the tabular's \vcenter box below its baseline.
            let d_tab = ((table_top - y_top) * 0.5 - axis).max(0.0);
            let d_cap = render_caption(layouter, caption_inls, y_top, Some(d_tab));
            y_top = layouter.y - d_cap;
        }
    }

    layouter.y = y_top;
}

fn prepare_row_cells<'a, 'doc>(
    layouter: &mut Layouter<'a>,
    row: &'doc TableRow<'a>,
    n: usize,
    col_w: &[f32],
    col_offsets: &[f32],
    row_math_collector: &mut Vec<&'doc crate::math::MathNode<'a>>,
) -> f32 {
    layouter.cell_words_soa.clear();
    layouter.cell_lines.clear();
    layouter.cell_line_spans.clear();
    row_math_collector.clear();
    if row.cells.is_empty() {
        // Rule-only row (trailing rules of a longtable head/foot box): no height.
        return 0.0;
    }

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
        // Each cell is a group: it starts with the colour and size outside the tabular.
        layouter.cell_words_soa.class_size = layouter.class_font_size;
        layouter.cell_words_soa.set_color(layouter.text_color);
        layouter.cell_words_soa.set_size(layouter.base_font_size, layouter.base_font_size);
        let total_w = explode_inlines_into_family(
            &cell.inlines,
            Face::Roman,
            layouter.base_font_size,
            &mut layouter.cell_words_soa,
            layouter.font_family,
            row_math_collector,
        );
        let end_idx = layouter.cell_words_soa.len();

        if start_idx == end_idx {
            layouter.cell_line_spans.push(CellSpan {
                start: layouter.cell_lines.len() as u16,
                count: 0,
            });
        } else if total_w <= inner_w && !crate::layout::layout_engine::inlines_have_break(&cell.inlines) {
            let lines_start = layouter.cell_lines.len();
            let (trimmed_end, trimmed_w) = layouter.trim_cell_line(start_idx, end_idx, total_w);
            layouter.cell_lines.push(CellLineRange::natural(start_idx as u16, (trimmed_end - start_idx) as u16, trimmed_w));
            layouter.cell_line_spans.push(CellSpan {
                start: lines_start as u16,
                count: 1,
            });
        } else {
            let lines_start = layouter.cell_lines.len();
            let base = layouter.base_font_size;
            layouter.break_cell_words_into_lines(start_idx, end_idx, inner_w, base, true, false);
            let lines_count = (layouter.cell_lines.len() - lines_start) as u16;
            layouter.cell_line_spans.push(CellSpan {
                start: lines_start as u16,
                count: lines_count,
            });
            max_lines = max_lines.max(lines_count);
        }
        c_idx += colspan;
    }

    // Row = \@arstrut height + (lines - 1) \baselineskip + \@arstrut depth.
    max_lines as f32 * layouter.base_leading
}

#[allow(clippy::too_many_arguments)]
fn paint_prepared_row<'a, 'doc>(
    layouter: &mut Layouter<'a>,
    row: &'doc TableRow<'a>,
    cols: &[ColSpec],
    col_w: &[f32],
    col_offsets: &[f32],
    origin_x: f32,
    table_w: f32,
    rule_right: bool,
    y_top: &mut f32,
    row_h: f32,
    row_math_collector: &[&'doc crate::math::MathNode<'a>],
    with_rules: bool,
) {
    let n = cols.len();

    // 1. Draw rules above row
    if with_rules {
        emit_table_rules(layouter, &row.rules_above, origin_x, col_offsets, table_w, y_top);
    }
    if with_rules && row.rule_above && row.rules_above.is_empty() {
        let rule_t = ARRAY_RULE_WIDTH;
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

    if row.cells.is_empty() {
        return;
    }
    let row_bottom = *y_top - row_h;
    let rule_t = ARRAY_RULE_WIDTH;

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
            // Every \vline is centred on its column boundary, the outer ones too: pdfTeX's
            // rules there reach half a rule width past the ends of \hline.
            let rule_x = x - rule_t * 0.5;
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
            // Each row carries \@arstrut (height 0.7\baselineskip): the first
            // baseline sits that far below the row top, later ones \baselineskip apart.
            let baseline =
                (*y_top - STRUT_HT_FRAC * layouter.base_leading) - (l as f32) * layouter.base_leading;
            let inner_x = match align {
                ColAlign::Left => x + CELL_PAD,
                ColAlign::Right => x + item_w - CELL_PAD - line_desc.line_w,
                ColAlign::Center => x + CELL_PAD + (inner_w - line_desc.line_w) * 0.5,
            };
            layouter.paint_cell_line(line_desc, inner_x.max(x + 1.0), baseline, row_math_collector);
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
            x: origin_x + table_w - rule_t * 0.5,
            y: row_bottom,
            w: rule_t,
            h: row_h,
            fill: true,
        });
    }

    *y_top = row_bottom;
}

/// Font-relative lengths of `booktabs.sty` for the current text font.
#[derive(Clone, Copy)]
struct RuleMetrics {
    em: f32,
    ex: f32,
}

impl RuleMetrics {
    fn of(layouter: &Layouter<'_>) -> Self {
        let size = layouter.base_font_size;
        let ex = crate::font_metrics::resolve_x_height_units(layouter.font_family, Face::Roman) * size * 0.001;
        Self { em: size, ex }
    }
}

/// `\doublerulesep` (latex.ltx), the gap booktabs puts between two consecutive rules.
const DOUBLE_RULE_SEP: f32 = 2.0;

#[inline]
fn is_booktabs_rule(rule: &TableRule) -> bool {
    matches!(rule, TableRule::TopRule | TableRule::MidRule | TableRule::BottomRule)
}

/// Space above, rule thickness and space below of `rules[i]` in a run of consecutive rules,
/// following booktabs.sty:
///
/// - `\toprule`/`\midrule`/`\bottomrule` (`\@BTrule`): `\@aboverulesep` above (`\abovetopsep`
///   = 0 for `\toprule`, `\aboverulesep` = .4ex otherwise), or `\doublerulesep` when another
///   booktabs rule came just before; `\@belowrulesep` below (`\belowrulesep` = .65ex,
///   `\belowbottomsep` = 0 for `\bottomrule`) unless a booktabs rule or `\cmidrule` follows
///   (`\@BTendrule`). Widths: `\heavyrulewidth` .08em, `\lightrulewidth` .05em.
/// - `\cmidrule` (`\cmidrulewidth` .03em): `\aboverulesep` above only after non-rule material;
///   a following `\cmidrule` shares its line (`\vskip-\@thisrulewidth`), anything else gets
///   `\belowrulesep`.
/// - `\hline` is `\arrayrulewidth` (0.4pt) with no space. `\cline` is a row of that height
///   followed by `\noalign{\vskip-\arrayrulewidth}`: it draws over the top of the next row
///   and takes no height.
fn rule_geom(rules: &[TableRule], i: usize, m: RuleMetrics) -> (f32, f32, f32) {
    let rule = &rules[i];
    let prev = i.checked_sub(1).map(|k| &rules[k]);
    let next = rules.get(i + 1);
    // \@lastruleclass = 1: the previous booktabs rule saw this rule coming.
    let prev_is_bt = prev.is_some_and(is_booktabs_rule);
    let next_is_bt = next.is_some_and(|n| is_booktabs_rule(n) || matches!(n, TableRule::CMidRule { .. }));
    let (above_sep, h, below_sep) = match rule {
        TableRule::HLine => return (0.0, ARRAY_RULE_WIDTH, 0.0),
        TableRule::CLine { .. } => return (0.0, ARRAY_RULE_WIDTH, -ARRAY_RULE_WIDTH),
        TableRule::TopRule => (0.0, 0.08 * m.em, 0.65 * m.ex),
        TableRule::MidRule => (0.4 * m.ex, 0.05 * m.em, 0.65 * m.ex),
        TableRule::BottomRule => (0.4 * m.ex, 0.08 * m.em, 0.0),
        TableRule::CMidRule { .. } => {
            let h = 0.03 * m.em;
            let after_rule = prev_is_bt || matches!(prev, Some(TableRule::CMidRule { .. }));
            let above = if after_rule { 0.0 } else { 0.4 * m.ex };
            let below = if matches!(next, Some(TableRule::CMidRule { .. })) { -h } else { 0.65 * m.ex };
            return (above, h, below);
        }
    };
    let above = if prev_is_bt { DOUBLE_RULE_SEP } else { above_sep };
    let below = if next_is_bt { 0.0 } else { below_sep };
    (above, h, below)
}

/// Vertical advance of `rules[i]` exactly as [`emit_table_rules`] applies it.
#[inline]
fn rule_advance_at(rules: &[TableRule], i: usize, m: RuleMetrics) -> f32 {
    let (a, h, b) = rule_geom(rules, i, m);
    a + h + b
}

/// Vertical advance of a run of consecutive rules.
#[inline]
fn rules_list_advance(rules: &[TableRule], m: RuleMetrics) -> f32 {
    (0..rules.len()).map(|i| rule_advance_at(rules, i, m)).sum()
}

/// Emits a run of consecutive rules (the rules between two rows) from `*y_top` down.
fn emit_table_rules<'a>(
    layouter: &mut Layouter<'a>,
    rules: &[TableRule],
    origin_x: f32,
    col_offsets: &[f32],
    table_w: f32,
    y_top: &mut f32,
) {
    let m = RuleMetrics::of(layouter);
    for (i, rule) in rules.iter().enumerate() {
        let (above, h, below) = rule_geom(rules, i, m);
        *y_top -= above;
        if !layouter.is_dry_run {
            let (x, w) = match *rule {
                TableRule::CLine { start_col, end_col } => {
                    col_range_extent(origin_x, col_offsets, start_col as usize, end_col as usize)
                }
                TableRule::CMidRule { start_col, end_col, trim_left, trim_right } => {
                    // (l)/(r) trims: \cmidrulekern = .5em.
                    let (mut x, mut w) = col_range_extent(origin_x, col_offsets, start_col as usize, end_col as usize);
                    let kern = 0.5 * m.em;
                    if trim_left {
                        x += kern;
                        w -= kern;
                    }
                    if trim_right {
                        w -= kern;
                    }
                    (x, w.max(0.0))
                }
                _ => (origin_x, table_w),
            };
            layouter.push_op(Paint::Rule { x, y: *y_top - h, w, h, fill: true });
        }
        *y_top -= h + below;
    }
}

#[inline]
fn rules_advance(row: &TableRow<'_>, m: RuleMetrics) -> f32 {
    if row.rule_above && row.rules_above.is_empty() {
        return ARRAY_RULE_WIDTH;
    }
    rules_list_advance(&row.rules_above, m)
}

/// Height of a longtable head/foot box and the depth of its last item
/// (a trailing rule has no depth; a trailing row keeps its strut depth).
fn lt_section_extent<'a, 'doc>(
    layouter: &mut Layouter<'a>,
    rows: &'doc [TableRow<'a>],
    range: (u16, u16),
    geom: &TableGeom<'_>,
    collector: &mut Vec<&'doc crate::math::MathNode<'a>>,
    strut_dp: f32,
) -> (f32, f32) {
    let mut total = 0.0f32;
    let mut last_dp = 0.0f32;
    let m = RuleMetrics::of(layouter);
    for row in &rows[range.0 as usize..range.1 as usize] {
        let n = geom.cols.len();
        let h = prepare_row_cells(layouter, row, n, geom.col_w, geom.col_offsets, collector);
        total += rules_advance(row, m) + h;
        last_dp = if row.cells.is_empty() { 0.0 } else { strut_dp };
    }
    (total, last_dp)
}

fn paint_lt_section<'a, 'doc>(
    layouter: &mut Layouter<'a>,
    rows: &'doc [TableRow<'a>],
    range: (u16, u16),
    geom: &TableGeom<'_>,
    y_top: &mut f32,
    collector: &mut Vec<&'doc crate::math::MathNode<'a>>,
) {
    let n = geom.cols.len();
    for row in &rows[range.0 as usize..range.1 as usize] {
        let h = prepare_row_cells(layouter, row, n, geom.col_w, geom.col_offsets, collector);
        paint_prepared_row(
            layouter, row, geom.cols, geom.col_w, geom.col_offsets, geom.origin_x, geom.table_w,
            geom.rule_right, y_top, h, collector, true,
        );
    }
}

/// `longtable` pagination following longtable.sty:
///
/// - the first page starts with `\endfirsthead` (or `\endhead`), later pages with `\endhead`;
/// - `\endfoot` is set at the bottom of every page and its height is withheld from `\vsize`
///   (with `\maxdepth = 0`); the last page ends with `\endlastfoot` (or `\endfoot`);
/// - rows are unbreakable; an `\hline` is emitted as `rule \penalty-\@lowpenalty
///   \vskip-\arrayrulewidth rule`, so a page break at a rule leaves one copy at the page
///   bottom and repeats it, overlapping the head, at the top of the next page;
/// - like TeX's page builder with no stretch available (every break is `deplorable`),
///   the last row that still fits is kept on the page.
fn layout_longtable<'a>(layouter: &mut Layouter<'a>, table_data: &TableData<'a>, geom: &TableGeom<'_>) {
    let rows = &table_data.rows;
    let lt = table_data.lt_sections;
    let n = geom.cols.len();
    let mut collector: Vec<&crate::math::MathNode<'a>> = Vec::new();
    let strut_dp = 0.3 * layouter.base_leading;
    let m = RuleMetrics::of(layouter);

    let head = lt.head;
    let first_head = if LtSections::is_empty_range(lt.first_head) { head } else { lt.first_head };
    let foot = lt.foot;
    let last_foot = if LtSections::is_empty_range(lt.last_foot) { foot } else { lt.last_foot };

    let (head_h, head_dp) = lt_section_extent(layouter, rows, head, geom, &mut collector, strut_dp);
    let (first_head_h, first_head_dp) =
        lt_section_extent(layouter, rows, first_head, geom, &mut collector, strut_dp);
    let (foot_h, _) = lt_section_extent(layouter, rows, foot, geom, &mut collector, strut_dp);
    let (last_foot_h, _) = lt_section_extent(layouter, rows, last_foot, geom, &mut collector, strut_dp);
    let has_foot = !LtSections::is_empty_range(foot);
    let max_depth = if has_foot { 0.0 } else { 0.5 * layouter.top_skip };
    let body = (lt.body_start as usize).min(rows.len())..rows.len();

    // Height of the first item of a fresh page: \topskip is measured against it (tex.web §1001).
    let first_item_ht = |sect_h: f32, sect_dp: f32, first_body: Option<&TableRow<'_>>, body_h: f32| -> f32 {
        if sect_h > 0.0 {
            sect_h - sect_dp
        } else if let Some(r) = first_body {
            if r.rules_above.is_empty() {
                (body_h - strut_dp).max(0.0)
            } else {
                rule_advance_at(&r.rules_above, 0, m)
            }
        } else {
            0.0
        }
    };
    let top_of_fresh_page = |l: &Layouter<'a>, item_ht: f32| l.text_top() - (l.top_skip - item_ht).max(0.0);

    let first_body = rows.get(body.start);
    let first_body_h = first_body
        .map(|r| rules_advance(r, m) + prepare_row_cells(layouter, r, n, geom.col_w, geom.col_offsets, &mut collector))
        .unwrap_or(0.0);

    // 4. Caption above (longtable \caption inside the first head is rendered first).
    if table_data.caption_above {
        if let Some(caption_inls) = table_data.caption.as_deref() {
            let float_top = layouter.y + STRUT_HT_FRAC * layouter.base_leading;
            let d_cap = render_caption(layouter, caption_inls, float_top, None);
            layouter.y -= d_cap;
        }
    }

    // \LT@start: \par\penalty0\vskip\LTpre, then break first if head + foot + first row overflow.
    let mut y_top = if layouter.at_page_top() {
        top_of_fresh_page(layouter, first_item_ht(first_head_h, first_head_dp, first_body, first_body_h))
    } else {
        layouter.y + layouter.base_leading - LT_PRE
    };
    if !layouter.at_page_top()
        && y_top - first_head_h - foot_h - first_body_h + strut_dp.min(max_depth) < layouter.margin_bottom - 0.01
    {
        layouter.new_page();
        y_top = top_of_fresh_page(layouter, first_item_ht(first_head_h, first_head_dp, first_body, first_body_h));
    }
    paint_lt_section(layouter, rows, first_head, geom, &mut y_top, &mut collector);

    let mut rows_on_page = 0usize;
    for idx in body.clone() {
        let row = &rows[idx];
        let bottom_goal = layouter.margin_bottom + foot_h;
        let row_h = prepare_row_cells(layouter, row, n, geom.col_w, geom.col_offsets, &mut collector);
        let row_dp = if row.cells.is_empty() { 0.0 } else { strut_dp };
        let after = y_top - rules_advance(row, m) - row_h;
        // Legal break after this row: at the next row's first rule (the rule stays here),
        // or at the interline glue / table end (the row depth may hang into \maxdepth).
        let next_rule = if idx + 1 < body.end {
            let r = &rows[idx + 1].rules_above;
            (!r.is_empty()).then(|| rule_advance_at(r, 0, m))
        } else {
            let r = &table_data.bottom_rules;
            (!r.is_empty()).then(|| rule_advance_at(r, 0, m))
        };
        let lowest = match next_rule {
            Some(r) => after - r,
            None => after + row_dp.min(max_depth),
        };
        if lowest < bottom_goal - 0.01 && rows_on_page > 0 {
            // First copy of the separating rules ends this page, then the foot.
            emit_table_rules(layouter, &row.rules_above, geom.origin_x, geom.col_offsets, geom.table_w, &mut y_top);
            paint_lt_section(layouter, rows, foot, geom, &mut y_top, &mut collector);
            layouter.new_page();
            y_top = top_of_fresh_page(layouter, first_item_ht(head_h, head_dp, Some(row), row_h));
            paint_lt_section(layouter, rows, head, geom, &mut y_top, &mut collector);
            // Second copy after \vskip-\arrayrulewidth (kept: the head already started the page).
            if !row.rules_above.is_empty() {
                if head_h > 0.0 {
                    y_top += ARRAY_RULE_WIDTH;
                }
                emit_table_rules(layouter, &row.rules_above, geom.origin_x, geom.col_offsets, geom.table_w, &mut y_top);
            }
            rows_on_page = 0;
            let row_h = prepare_row_cells(layouter, row, n, geom.col_w, geom.col_offsets, &mut collector);
            paint_prepared_row(
                layouter, row, geom.cols, geom.col_w, geom.col_offsets, geom.origin_x, geom.table_w,
                geom.rule_right, &mut y_top, row_h, &collector, false,
            );
        } else {
            paint_prepared_row(
                layouter, row, geom.cols, geom.col_w, geom.col_offsets, geom.origin_x, geom.table_w,
                geom.rule_right, &mut y_top, row_h, &collector, true,
            );
        }
        if !row.cells.is_empty() {
            rows_on_page += 1;
        }
    }

    // Closing rules (\hline before \end{longtable}).
    emit_table_rules(layouter, &table_data.bottom_rules, geom.origin_x, geom.col_offsets, geom.table_w, &mut y_top);

    // \LT@output at \endlongtable: a taller \endlastfoot that no longer fits forces one more
    // page carrying the head and the last foot.
    if last_foot_h > foot_h && y_top - last_foot_h < layouter.margin_bottom - 0.01 {
        paint_lt_section(layouter, rows, foot, geom, &mut y_top, &mut collector);
        layouter.new_page();
        y_top = top_of_fresh_page(layouter, first_item_ht(head_h, head_dp, None, 0.0));
        paint_lt_section(layouter, rows, head, geom, &mut y_top, &mut collector);
    }
    paint_lt_section(layouter, rows, last_foot, geom, &mut y_top, &mut collector);

    // 7. Caption below.
    if !table_data.caption_above {
        if let Some(caption_inls) = table_data.caption.as_deref() {
            let d_cap = render_caption(layouter, caption_inls, y_top, Some(0.0));
            y_top = layouter.y - d_cap;
        }
    }

    // \endgraf\penalty0\addvspace\LTpost; the next baseline follows at \baselineskip.
    layouter.y = y_top - LT_POST - layouter.base_leading;
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

/// `\@arstrut` height as a fraction of `\baselineskip` (latex.ltx: `.7\baselineskip`).
const STRUT_HT_FRAC: f32 = 0.7;
/// `\abovecaptionskip` (article.cls / latex.ltx default: 10pt).
const ABOVE_CAPTION_SKIP: f32 = 10.0;
/// `\belowcaptionskip` (latex.ltx default: 0pt).
const BELOW_CAPTION_SKIP: f32 = 0.0;
/// `\lineskip` (plain/LaTeX default: 1pt).
const LINESKIP: f32 = 1.0;
/// Math axis height of cmsy10 (fontdimen 22) in em; a `[c]` tabular is a `\vcenter`ed box.
const AXIS_HEIGHT_EM: f32 = 0.25;

/// TFM box (height, depth) in 1/1000 em of the characters in `bytes`
/// (cmr10, or cmbx10 when `bold`): the dimensions TeX gives an `\hbox` of them.
#[inline]
fn bytes_extent_units(bytes: &[u8], bold: bool) -> (f32, f32) {
    let (mut h, mut d) = (0.0f32, 0.0f32);
    for &b in bytes {
        let m = if bold { crate::math::math_tfm::cmbx10_metric(b) } else { crate::math::math_tfm::cmr10_metric(b) };
        h = h.max(m.h);
        d = d.max(m.d);
    }
    (h, d)
}

/// Vertical extent (height, depth) in points of the words in `soa[start..end]`.
fn soa_ink_extent(soa: &crate::layout::engine::ParagraphSoA<'_>, start: usize, end: usize, size: f32) -> (f32, f32) {
    let (mut h, mut d) = (0.0f32, 0.0f32);
    for i in start..end {
        if !soa.attrs[i].is_word_like() {
            continue;
        }
        let (wh, wd) = bytes_extent_units(soa.texts[i].as_bytes(), soa.attrs[i].face().is_bold());
        h = h.max(wh);
        d = d.max(wd);
    }
    (h * size * 0.001, d * size * 0.001)
}

/// Distance from the bottom of a box of depth `prev_depth` (or from the top of
/// the enclosing vbox when `None`) down to the baseline of a following box of
/// height `h`: TeX's interline glue with `\baselineskip`, `\lineskiplimit = 0pt`
/// and `\lineskip = 1pt`.
#[inline]
fn interline_to_baseline(prev_depth: Option<f32>, h: f32, baselineskip: f32) -> f32 {
    match prev_depth {
        None => h,
        Some(pd) => {
            let glue = baselineskip - pd - h;
            if glue < 0.0 { LINESKIP + h } else { glue + h }
        }
    }
}

/// Width of the caption label `\fnum@table` = `\tablename~\thetable` followed by
/// the `:` that `\@makecaption` appends.
fn caption_label_width(layouter: &Layouter<'_>, n: u32, size: f32) -> f32 {
    let fam = layouter.font_family;
    let scale = size * 0.001;
    let mut digits = 1u32;
    let mut v = n / 10;
    while v > 0 {
        digits += 1;
        v /= 10;
    }
    let units = crate::font_metrics::string_units_family("Table:", Face::Roman, fam)
        + crate::font_metrics::resolve_space_units(fam, Face::Roman)
        + digits * crate::font_metrics::resolve_digit_units(fam, Face::Roman);
    units as f32 * scale
}

/// Typesets a table caption following latex.ltx `\@makecaption`:
/// `\vskip\abovecaptionskip`, then the box `{#1: #2}` centred in `\hsize` when it
/// fits (`\hb@xt@\hsize{\hfil\box\@tempboxa\hfil}`), else set as a justified
/// paragraph of width `\hsize`; finally `\vskip\belowcaptionskip`.
///
/// `top` is the bottom edge of the material above (or the top of the float box)
/// and `prev_depth` that material's depth (TeX's `\prevdepth`; `None` at the top
/// of the float box). On exit `layouter.y` is the caption's last baseline and the
/// returned value is that line's depth.
fn render_caption<'a, 'doc>(
    layouter: &mut Layouter<'a>,
    caption_inls: &'doc [crate::frontend::ast::Inline<'a>],
    top: f32,
    prev_depth: Option<f32>,
) -> f32 {
    layouter.table_counter += 1;
    let n = layouter.table_counter;
    let size = layouter.base_font_size;
    let leading = layouter.base_leading;
    // After a page break the caption starts the float box at the top of the new page;
    // `top` still points at the previous page.
    let top = if layouter.ensure_room(leading * 2.0) { layouter.y + 0.7 * leading } else { top };

    if layouter.current_tbl_idx < layouter.registry.table_entries.len() {
        layouter.registry.table_entries[layouter.current_tbl_idx].page =
            layouter.current_page_number();
        layouter.current_tbl_idx += 1;
    }

    let fam = layouter.font_family;
    let hsize = (layouter.right - layouter.right_skip) - (layouter.left + layouter.left_skip);
    let origin_x = layouter.left + layouter.left_skip;
    let label_w = caption_label_width(layouter, n, size);
    // The space after ':' carries space factor 2000 (\nonfrenchspacing), so it
    // gets the font's extra space (fontdimen 7) on top of the interword space.
    let mut gap_units = crate::font_metrics::resolve_space_units(fam, Face::Roman);
    if !layouter.french_spacing {
        gap_units += crate::font_metrics::resolve_extra_space_units(fam, Face::Roman);
    }
    let lead_w = label_w + gap_units as f32 * size * 0.001;

    // Word 0 is a placeholder reserving the label's width so that line breaking
    // sees the label as the start of the caption paragraph.
    layouter.cell_words_soa.clear();
    layouter.cell_lines.clear();
    layouter.cell_words_soa.push("", Face::Roman, lead_w, crate::layout::engine::WordKind::Word);
    let mut cap_math: Vec<&'doc crate::math::MathNode<'a>> = Vec::new();
    let text_w = explode_inlines_into_family(
        caption_inls,
        Face::Roman,
        size,
        &mut layouter.cell_words_soa,
        fam,
        &mut cap_math,
    );
    let end = layouter.cell_words_soa.len();
    let total_w = lead_w + text_w;
    let single = total_w <= hsize && !crate::layout::layout_engine::inlines_have_break(caption_inls);
    if single {
        let (trimmed_end, trimmed_w) = layouter.trim_cell_line(0, end, total_w);
        layouter.cell_lines.push(CellLineRange::natural(0, trimmed_end as u16, trimmed_w));
    } else {
        layouter.break_cell_words_into_lines(0, end, hsize, size, true, false);
    }

    // First caption baseline: \vskip\abovecaptionskip, then interline glue
    // against the material above.
    let first = layouter.cell_lines[0];
    let (h_first, _) = soa_ink_extent(
        &layouter.cell_words_soa,
        first.start_word as usize,
        (first.start_word + first.word_count) as usize,
        size,
    );
    let h_label = bytes_extent_units(b"Table0:", false).0 * size * 0.001;
    let h_first = h_first.max(h_label);
    layouter.y = top - ABOVE_CAPTION_SKIP - interline_to_baseline(prev_depth, h_first, leading);

    let line_count = layouter.cell_lines.len();
    let mut last_depth = 0.0;
    for idx in 0..line_count {
        let line = layouter.cell_lines[idx];
        let start = line.start_word as usize;
        let stop = start + line.word_count as usize;
        let (_, d) = soa_ink_extent(&layouter.cell_words_soa, start, stop, size);
        last_depth = d;
        if idx > 0 {
            layouter.y -= leading;
        }
        let y = layouter.y;
        let x = if single { origin_x + (hsize - line.line_w).max(0.0) * 0.5 } else { origin_x };
        if layouter.is_dry_run {
            continue;
        }
        let mut word_x = x;
        let mut painted = line;
        if start == 0 {
            layouter.push_op(Paint::TableCaptionTag { x, y, size, n });
            word_x += lead_w;
            painted.start_word = 1;
            painted.word_count = (stop - 1) as u16;
        }
        layouter.paint_cell_line(painted, word_x, y, &cap_math);
    }
    layouter.y -= BELOW_CAPTION_SKIP;
    last_depth
}
