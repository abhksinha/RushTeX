//! Layout calculation, hierarchy indentation, dotted leader generation,
//! and page number alignment for `\tableofcontents`.

use crate::backend::fonts::Face;
use crate::index::index_model::IndexRegistry;
use crate::layout::engine::{count_digits, Layouter, Paint};

/// Lays out the complete Table of Contents with section numbers, hierarchical indentation,
/// dotted leaders, and right-aligned page numbers.
pub(crate) fn layout_toc<'a>(
    layouter: &mut Layouter<'a>,
    registry: &IndexRegistry<'a>,
) {
    let avail_w = (layouter.right - layouter.right_skip) - (layouter.left + layouter.left_skip);
    if avail_w <= 0.0 {
        return;
    }

    // 1. Render "Contents" Header
    layouter.ensure_room(36.0);
    let heading_size = 18.0f32;
    let heading_leading = heading_size * 1.3;
    let heading_text = "Contents";

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

    for entry in &registry.toc_entries {
        if entry.level > registry.tocdepth {
            continue;
        }

        let (indent, num_box_w, skip_before, face) = match entry.level {
            1 => (0.0f32, 20.0f32, 10.0f32, Face::Bold),
            2 => (16.0f32, 28.0f32, 4.0f32, Face::Roman),
            _ => (32.0f32, 36.0f32, 2.0f32, Face::Roman),
        };

        let row_h = layouter.base_leading;
        layouter.ensure_room(skip_before + row_h);
        layouter.y -= skip_before;

        if !layouter.is_dry_run {
            // A. Draw Section Number
            layouter.push_op(Paint::SectionNumber {
                x: left_margin + indent,
                y: layouter.y,
                face,
                size: layouter.base_font_size,
                number: entry.number,
                level: entry.level,
            });

            // B. Draw Title Inlines
            let title_x = left_margin + indent + num_box_w;
            layouter.paint_inlines_at_size(
                &entry.title,
                face,
                layouter.base_font_size,
                title_x,
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

            // D. Draw Dotted Leader (. . . . . . .)
            let title_w =
                layouter.measure_inlines_face_size(&entry.title, face, layouter.base_font_size);
            let leader_start = title_x + title_w + 10.0;
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
