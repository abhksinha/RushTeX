//! Layout calculations and formatting for `\listoffigures` and `\listoftables`.

use crate::backend::fonts::Face;
use crate::index::index_model::IndexRegistry;
use crate::layout::engine::{count_digits, Layouter, Paint};

/// Lays out the complete List of Figures with figure captions, dotted leaders, and page numbers.
pub(crate) fn layout_lof<'a>(layouter: &mut Layouter<'a>, registry: &IndexRegistry<'a>) {
    let avail_w = (layouter.right - layouter.right_skip) - (layouter.left + layouter.left_skip);
    if avail_w <= 0.0 {
        return;
    }

    // 1. Render "List of Figures" Header
    layouter.ensure_room(36.0);
    let heading_size = 18.0f32;
    let heading_leading = heading_size * 1.3;
    let heading_text = "List of Figures";

    if !layouter.is_dry_run {
        let text_pool_start = layouter.current_page.text_pool.len() as u32;
        layouter.current_page.text_pool.push(heading_text);
        layouter.push_op(Paint::GlyphRun {
            x: layouter.left + layouter.left_skip,
            y: layouter.y,
            face: Face::Bold,
            size: heading_size,
            part_start: text_pool_start,
            part_count: 1,
            word_spacing: 0.0,
        });
    }

    layouter.y -= heading_leading + 12.0;

    // 2. Render Entries
    let left_margin = layouter.left + layouter.left_skip;
    let right_margin = layouter.right - layouter.right_skip;
    let dot_spacing = 8.0f32;
    let row_h = layouter.base_leading;

    for entry in &registry.figure_entries {
        layouter.ensure_room(row_h + 2.0);
        layouter.y -= 2.0;

        if !layouter.is_dry_run {
            // A. Draw Figure Tag (e.g. "Figure 1")
            layouter.push_op(Paint::FigureTag {
                x: left_margin,
                y: layouter.y,
                face: Face::Roman,
                size: layouter.base_font_size,
                n: entry.number,
            });

            // B. Draw Caption
            let caption_x = left_margin + 60.0;
            layouter.paint_inlines_at_size(
                entry.caption,
                Face::Roman,
                layouter.base_font_size,
                caption_x,
                layouter.y,
            );

            // C. Draw Page Number (Right-Aligned)
            let page_digits = count_digits(entry.page);
            let page_w = (page_digits as f32) * (layouter.base_font_size * 0.5);
            let page_x = right_margin - page_w;

            layouter.push_op(Paint::PageNumber {
                x: page_x,
                y: layouter.y,
                face: Face::Roman,
                size: layouter.base_font_size,
                page_number: entry.page,
            });

            // D. Draw Dotted Leader
            let cap_w = layouter.measure_inlines_face_size(
                entry.caption,
                Face::Roman,
                layouter.base_font_size,
            );
            let leader_start = caption_x + cap_w + 10.0;
            let leader_end = page_x - 10.0;

            if leader_end > leader_start + dot_spacing {
                let mut dot_x = leader_start;
                let dot_pool_start = layouter.current_page.text_pool.len() as u32;
                layouter.current_page.text_pool.push(".");
                while dot_x < leader_end {
                    layouter.push_op(Paint::GlyphRun {
                        x: dot_x,
                        y: layouter.y,
                        face: Face::Roman,
                        size: layouter.base_font_size,
                        part_start: dot_pool_start,
                        part_count: 1,
                        word_spacing: 0.0,
                    });
                    dot_x += dot_spacing;
                }
            }
        }

        layouter.y -= row_h;
    }

    layouter.y -= 16.0;
}

/// Lays out the complete List of Tables with table captions, dotted leaders, and page numbers.
pub(crate) fn layout_lot<'a>(layouter: &mut Layouter<'a>, registry: &IndexRegistry<'a>) {
    let avail_w = (layouter.right - layouter.right_skip) - (layouter.left + layouter.left_skip);
    if avail_w <= 0.0 {
        return;
    }

    // 1. Render "List of Tables" Header
    layouter.ensure_room(36.0);
    let heading_size = 18.0f32;
    let heading_leading = heading_size * 1.3;
    let heading_text = "List of Tables";

    if !layouter.is_dry_run {
        let text_pool_start = layouter.current_page.text_pool.len() as u32;
        layouter.current_page.text_pool.push(heading_text);
        layouter.push_op(Paint::GlyphRun {
            x: layouter.left + layouter.left_skip,
            y: layouter.y,
            face: Face::Bold,
            size: heading_size,
            part_start: text_pool_start,
            part_count: 1,
            word_spacing: 0.0,
        });
    }

    layouter.y -= heading_leading + 12.0;

    // 2. Render Entries
    let left_margin = layouter.left + layouter.left_skip;
    let right_margin = layouter.right - layouter.right_skip;
    let dot_spacing = 8.0f32;
    let row_h = layouter.base_leading;

    for entry in &registry.table_entries {
        layouter.ensure_room(row_h + 2.0);
        layouter.y -= 2.0;

        if !layouter.is_dry_run {
            // A. Draw Table Tag (e.g. "Table 1")
            layouter.push_op(Paint::TableTag {
                x: left_margin,
                y: layouter.y,
                face: Face::Roman,
                size: layouter.base_font_size,
                n: entry.number,
            });

            // B. Draw Caption
            let caption_x = left_margin + 60.0;
            layouter.paint_inlines_at_size(
                entry.caption,
                Face::Roman,
                layouter.base_font_size,
                caption_x,
                layouter.y,
            );

            // C. Draw Page Number (Right-Aligned)
            let page_digits = count_digits(entry.page);
            let page_w = (page_digits as f32) * (layouter.base_font_size * 0.5);
            let page_x = right_margin - page_w;

            layouter.push_op(Paint::PageNumber {
                x: page_x,
                y: layouter.y,
                face: Face::Roman,
                size: layouter.base_font_size,
                page_number: entry.page,
            });

            // D. Draw Dotted Leader
            let cap_w = layouter.measure_inlines_face_size(
                entry.caption,
                Face::Roman,
                layouter.base_font_size,
            );
            let leader_start = caption_x + cap_w + 10.0;
            let leader_end = page_x - 10.0;

            if leader_end > leader_start + dot_spacing {
                let mut dot_x = leader_start;
                let dot_pool_start = layouter.current_page.text_pool.len() as u32;
                layouter.current_page.text_pool.push(".");
                while dot_x < leader_end {
                    layouter.push_op(Paint::GlyphRun {
                        x: dot_x,
                        y: layouter.y,
                        face: Face::Roman,
                        size: layouter.base_font_size,
                        part_start: dot_pool_start,
                        part_count: 1,
                        word_spacing: 0.0,
                    });
                    dot_x += dot_spacing;
                }
            }
        }

        layouter.y -= row_h;
    }

    layouter.y -= 16.0;
}
