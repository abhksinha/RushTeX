//! Parsing logic for standard `tabular`, floating `table`, `longtable`, and `booktabs` rules.

use crate::frontend::ast::{Block, Inline, ParseError};
use crate::frontend::parser::Parser;
use crate::tables::tables_model::{TableCell, TableData, TableRow, TableRule};
use crate::tables::tables_spec::{ColAlign, ColSpec};

pub(crate) fn parse_tabular<'a>(
    p: &mut Parser<'a>,
    open_offset: usize,
) -> Result<Block<'a>, ParseError> {
    parse_table_body(p, "tabular", false, open_offset)
}

pub(crate) fn parse_longtable<'a>(
    p: &mut Parser<'a>,
    open_offset: usize,
) -> Result<Block<'a>, ParseError> {
    parse_table_body(p, "longtable", true, open_offset)
}

pub(crate) fn parse_table_float<'a>(
    p: &mut Parser<'a>,
    open_offset: usize,
) -> Result<Block<'a>, ParseError> {
    p.skip_hws();
    let _ = p.parse_optional_brackets()?;
    let mut caption = None;
    let mut caption_above = false;
    let mut table_block = None;

    loop {
        p.skip_ws();
        if p.eat_end_env("table") {
            break;
        }
        if p.peek().is_none() {
            let end_line = p.current_line_number();
            return Err(p.err_span(
                open_offset,
                "\\begin{table}".len(),
                "unclosed \\begin{table}",
                Some("this '\\begin{table}' is never closed"),
                Some(&format!("reached end of file at line {end_line} without finding matching '\\end{{table}}'")),
                Some("did you forget to add '\\end{table}'?"),
            ));
        }
        if p.cmd_ends_at("\\caption") {
            p.eat("\\caption");
            let cap_inls = p.require_group()?;
            caption = Some(cap_inls);
            if table_block.is_none() {
                caption_above = true;
            }
            continue;
        }
        if p.starts_with("\\begin{tabular}") {
            let tab_offset = p.byte_offset;
            p.eat("\\begin{tabular}");
            let block = parse_tabular(p, tab_offset)?;
            table_block = Some(block);
            continue;
        }
        if p.starts_with("\\centering") {
            p.eat("\\centering");
            continue;
        }
        // Skip unknown commands inside table float
        if p.starts_with("\\") {
            let _ = p.parse_inline()?;
            continue;
        }
        let _ = p.bump();
    }

    if let Some(Block::Table(mut tdata)) = table_block {
        tdata.caption = caption;
        tdata.caption_above = caption_above;
        Ok(Block::Table(tdata))
    } else {
        // Fallback empty table
        Ok(Block::Table(Box::new(TableData {
            cols: Vec::new(),
            col_widths: Vec::new(),
            rows: Vec::new(),
            rule_right: false,
            rule_bottom: false,
            bottom_rules: Vec::new(),
            is_longtable: false,
            header_row_count: 0,
            first_header_row_count: 0,
            footer_row_count: 0,
            caption,
            caption_above,
        })))
    }
}

fn parse_table_body<'a>(
    p: &mut Parser<'a>,
    env_name: &str,
    is_longtable: bool,
    open_offset: usize,
) -> Result<Block<'a>, ParseError> {
    p.skip_hws();
    let (cols, col_widths, rule_right) = parse_colspec(p)?;
    let col_count = cols.len();
    let mut rows = Vec::with_capacity(8);
    let mut pending_rules = Vec::with_capacity(2);
    let mut header_row_count = 0;
    let mut first_header_row_count = 0;
    let mut footer_row_count = 0;
    let mut caption = None;
    let mut caption_above = false;

    loop {
        p.skip_ws();
        if p.eat_end_env(env_name) {
            break;
        }
        if p.peek().is_none() {
            let end_line = p.current_line_number();
            return Err(p.err_span(
                open_offset,
                format!("\\begin{{{env_name}}}").len(),
                format!("unclosed \\begin{{{env_name}}}"),
                Some(&format!("this '\\begin{{{env_name}}}' is never closed")),
                Some(&format!("reached end of file at line {end_line} without finding matching '\\end{{{env_name}}}'")),
                Some(&format!("did you forget to add '\\end{{{env_name}}}'?")),
            ));
        }
        if p.starts_with("\\end{document}") {
            let doc_line = p.current_line_number();
            return Err(p.err_span(
                open_offset,
                format!("\\begin{{{env_name}}}").len(),
                format!("unclosed \\begin{{{env_name}}}"),
                Some(&format!("this '\\begin{{{env_name}}}' is never closed")),
                Some(&format!("reached '\\end{{document}}' at line {doc_line} while '\\begin{{{env_name}}}' was still open")),
                Some(&format!("add '\\end{{{env_name}}}' before '\\end{{document}}'")),
            ));
        }

        // Horizontal rules
        if p.cmd_ends_at("\\hline") {
            p.eat("\\hline");
            pending_rules.push(TableRule::HLine);
            continue;
        }
        if p.cmd_ends_at("\\toprule") {
            p.eat("\\toprule");
            pending_rules.push(TableRule::TopRule);
            continue;
        }
        if p.cmd_ends_at("\\midrule") {
            p.eat("\\midrule");
            pending_rules.push(TableRule::MidRule);
            continue;
        }
        if p.cmd_ends_at("\\bottomrule") {
            p.eat("\\bottomrule");
            pending_rules.push(TableRule::BottomRule);
            continue;
        }
        if p.cmd_ends_at("\\cline") {
            p.eat("\\cline");
            let arg = p.brace_arg()?;
            if let Some((start, end)) = parse_range(arg) {
                pending_rules.push(TableRule::CLine {
                    start_col: start,
                    end_col: end,
                });
            }
            continue;
        }
        if p.cmd_ends_at("\\cmidrule") {
            p.eat("\\cmidrule");
            let (trim_l, trim_r) = if p.starts_with("(") {
                p.bump();
                let mut tl = false;
                let mut tr = false;
                while let Some(ch) = p.peek() {
                    if ch == ')' {
                        p.bump();
                        break;
                    }
                    if ch == 'l' {
                        tl = true;
                    }
                    if ch == 'r' {
                        tr = true;
                    }
                    p.bump();
                }
                (tl, tr)
            } else {
                (false, false)
            };
            let arg = p.brace_arg()?;
            if let Some((start, end)) = parse_range(arg) {
                pending_rules.push(TableRule::CMidRule {
                    start_col: start,
                    end_col: end,
                    trim_left: trim_l,
                    trim_right: trim_r,
                });
            }
            continue;
        }

        // longtable header / footer markers
        if is_longtable {
            if p.cmd_ends_at("\\endhead") {
                p.eat("\\endhead");
                header_row_count = rows.len();
                continue;
            }
            if p.cmd_ends_at("\\endfirsthead") {
                p.eat("\\endfirsthead");
                first_header_row_count = rows.len();
                continue;
            }
            if p.cmd_ends_at("\\endfoot") {
                p.eat("\\endfoot");
                footer_row_count = rows.len();
                continue;
            }
            if p.cmd_ends_at("\\endlastfoot") {
                p.eat("\\endlastfoot");
                continue;
            }
            if p.cmd_ends_at("\\caption") {
                p.eat("\\caption");
                let cap_inls = p.require_group()?;
                caption = Some(cap_inls);
                if rows.is_empty() {
                    caption_above = true;
                }
                p.skip_hws();
                let _ = p.eat("\\\\");
                continue;
            }
        }

        // Parse row cells
        let mut cells = Vec::with_capacity(col_count);
        loop {
            p.skip_hws();
            if p.cmd_ends_at("\\multicolumn") {
                let cell = parse_multicolumn(p)?;
                cells.push(cell);
            } else {
                let cell_inls = parse_cell_inlines(p)?;
                cells.push(TableCell::simple(cell_inls));
            }
            p.skip_hws();
            if p.eat("&") {
                continue;
            }
            if p.starts_with("\\\\") {
                p.eat("\\\\");
            }
            break;
        }

        let rule_above = !pending_rules.is_empty();
        let rules_above = if pending_rules.is_empty() {
            Vec::new()
        } else {
            std::mem::replace(&mut pending_rules, Vec::with_capacity(2))
        };
        rows.push(TableRow {
            rule_above,
            rules_above,
            cells,
        });
    }

    let rule_bottom = !pending_rules.is_empty();
    Ok(Block::Table(Box::new(TableData {
        cols,
        col_widths,
        rows,
        rule_right,
        rule_bottom,
        bottom_rules: pending_rules,
        is_longtable,
        header_row_count,
        first_header_row_count,
        footer_row_count,
        caption,
        caption_above,
    })))
}

type ParsedColSpec = (Vec<ColSpec>, Vec<Option<f32>>, bool);

fn parse_colspec<'a>(p: &mut Parser<'a>) -> Result<ParsedColSpec, ParseError> {
    let open_offset = p.byte_offset;
    if !p.eat("{") {
        return Err(p.err_span(
            open_offset,
            1,
            "expected {colspec} column spec",
            Some("expected column specification '{...}' here"),
            None,
            Some("specify column alignments using '{l}', '{c}', '{r}', or '{p{width}}'"),
        ));
    }
    let mut cols = Vec::with_capacity(4);
    let mut col_widths = Vec::with_capacity(4);
    let mut rule_left = false;

    loop {
        match p.peek() {
            Some('}') => {
                p.bump();
                break;
            }
            Some('|') => {
                p.bump();
                rule_left = true;
            }
            Some('l' | 'c' | 'r') => {
                let align = match p.bump() {
                    Some('l') => ColAlign::Left,
                    Some('c') => ColAlign::Center,
                    _ => ColAlign::Right,
                };
                cols.push(ColSpec::new(align, rule_left));
                col_widths.push(None);
                rule_left = false;
            }
            Some('p') => {
                p.bump(); // 'p'
                let dim_str = p.brace_arg()?;
                let dim_pt = crate::config::parse_dimension_pt(dim_str);
                cols.push(ColSpec::new(ColAlign::Left, rule_left));
                col_widths.push(dim_pt);
                rule_left = false;
            }
            Some('*') => {
                p.bump(); // '*'
                let count_str = p.brace_arg()?;
                let repeat_count: usize = count_str.parse().unwrap_or(1);
                let inner_spec = p.brace_arg()?;
                for _ in 0..repeat_count {
                    for ch in inner_spec.chars() {
                        match ch {
                            '|' => rule_left = true,
                            'l' => {
                                cols.push(ColSpec::new(ColAlign::Left, rule_left));
                                col_widths.push(None);
                                rule_left = false;
                            }
                            'c' => {
                                cols.push(ColSpec::new(ColAlign::Center, rule_left));
                                col_widths.push(None);
                                rule_left = false;
                            }
                            'r' => {
                                cols.push(ColSpec::new(ColAlign::Right, rule_left));
                                col_widths.push(None);
                                rule_left = false;
                            }
                            _ => {}
                        }
                    }
                }
            }
            Some(' ' | '\t') => {
                p.bump();
            }
            Some(c) => {
                let off = p.byte_offset;
                return Err(p.err_span(
                    off,
                    c.len_utf8(),
                    format!("unsupported column specifier '{c}'"),
                    Some("invalid column specifier"),
                    None,
                    Some("valid column specifiers are 'l', 'c', 'r', 'p{width}', '|', and '*{n}{cols}'"),
                ));
            }
            None => {
                return Err(p.err_span(
                    open_offset,
                    1,
                    "unclosed column spec",
                    Some("column spec opened here"),
                    Some("reached end of file without closing '}'"),
                    Some("add '}' to close the column specification"),
                ));
            }
        }
    }

    if cols.is_empty() {
        return Err(p.err_span(
            open_offset,
            p.byte_offset.saturating_sub(open_offset),
            "empty column spec",
            Some("no columns specified in this column spec"),
            None,
            Some("add at least one column specifier, e.g. '{c}' or '{l r}'"),
        ));
    }
    Ok((cols, col_widths, rule_left))
}

fn parse_multicolumn<'a>(p: &mut Parser<'a>) -> Result<TableCell<'a>, ParseError> {
    p.eat("\\multicolumn");
    let count_str = p.brace_arg()?;
    let colspan: u16 = count_str.parse().unwrap_or(1);
    let align_str = p.brace_arg()?;

    let mut rule_left = false;
    let mut rule_right = false;
    let mut align = ColAlign::Left;

    for ch in align_str.chars() {
        match ch {
            '|' => {
                if !rule_left && align == ColAlign::Left {
                    rule_left = true;
                } else {
                    rule_right = true;
                }
            }
            'l' => align = ColAlign::Left,
            'c' => align = ColAlign::Center,
            'r' => align = ColAlign::Right,
            _ => {}
        }
    }

    p.skip_hws();
    let inlines = p.require_group()?;
    Ok(TableCell {
        inlines,
        colspan,
        align: Some(align),
        rule_left,
        rule_right,
    })
}

fn parse_cell_inlines<'a>(p: &mut Parser<'a>) -> Result<Vec<Inline<'a>>, ParseError> {
    let mut out = Vec::new();
    loop {
        p.skip_hws();
        match p.peek() {
            None => break,
            Some('&') => break,
            Some('%') => p.skip_comment(),
            Some('\n') if p.at_blank_line() => break,
            Some(_) if p.starts_with("\\\\") => break,
            Some(_)
                if p.starts_with("\\end")
                    || p.cmd_ends_at("\\hline")
                    || p.cmd_ends_at("\\toprule")
                    || p.cmd_ends_at("\\midrule")
                    || p.cmd_ends_at("\\bottomrule")
                    || p.cmd_ends_at("\\cline")
                    || p.cmd_ends_at("\\cmidrule")
                    || p.cmd_ends_at("\\endhead")
                    || p.cmd_ends_at("\\endfoot") =>
            {
                break;
            }
            Some(_) if p.starts_with("\\begin") => {
                return Err(p.err("nested environments are not allowed in a cell"));
            }
            Some(_) => out.push(p.parse_inline()?),
        }
    }
    Ok(out)
}

fn parse_range(s: &str) -> Option<(u16, u16)> {
    let mut parts = s.split('-');
    let start: u16 = parts.next()?.trim().parse().ok()?;
    let end: u16 = parts.next()?.trim().parse().ok()?;
    Some((start, end))
}
