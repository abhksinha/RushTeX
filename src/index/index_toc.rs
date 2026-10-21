//! Layout calculation, hierarchy indentation, dotted leader generation,
//! and page number alignment for `\tableofcontents`.

use crate::backend::fonts::Face;
use crate::index::index_model::IndexRegistry;
use crate::layout::engine::{count_digits, Layouter, Paint, Sp};

/// Lays out `\tableofcontents` as article.cls does: a `\section*{Contents}` heading, then
/// `\l@section` entries (bold, `\addvspace{1em}` before each, no leaders) and
/// `\@dottedtocline` entries for deeper levels (aligned dot leaders), each with its page
/// number right-aligned in a `\@pnumwidth` box at the right margin.
pub(crate) fn layout_toc<'a>(
    layouter: &mut Layouter<'a>,
    registry: &IndexRegistry<'a>,
) {
    let avail_w = (layouter.right - layouter.right_skip) - (layouter.left + layouter.left_skip);
    if avail_w <= 0.0 {
        return;
    }
    let heading = [crate::frontend::ast::Inline::Text("Contents")];
    let merged = std::mem::take(&mut layouter.last_addvspace);
    layouter.layout_section(1, true, false, &heading, merged);

    let size = layouter.base_font_size;
    let family = layouter.font_family;
    // The font's quad and x-height (cmr10: 10.00002pt, 4.30554pt).
    let em = crate::backend::scaled::quad_pt(family, size);
    let ex = crate::backend::scaled::x_height_pt(family, size);
    let left_margin = layouter.left + layouter.left_skip;
    let right_margin = layouter.right - layouter.right_skip;
    // `\leaders\hbox{$\mkern\@dotsep mu\hbox{.}\mkern\@dotsep mu$}`, \@dotsep = 4.5. §716
    // `math_kern`: 1mu is `x_over_n(math_quad, 18)` sp, and the kern `xn_over_d(4.5mu, that, 2^16)`.
    let mu = (em * 65536.0).round() as i32 / 18;
    let dot_kern = crate::backend::scaled::xn_over_d(4 * 65536 + 32768, mu, 65536) as f32 / 65536.0;
    let dot_w = crate::backend::scaled::fix_metrics(family, Face::Roman, size)
        .map_or_else(|| crate::font_metrics::width_family('.', Face::Roman, size, family), |m| m.widths[b'.' as usize] as f32 / 1048576.0 * size);
    let leader_w = 2.0 * dot_kern + dot_w;
    // The heading's 2.3ex afterskip is the last skip that the first \addvspace meets.
    let mut last_skip = crate::backend::scaled::em_mul(2.3, ex);

    for entry in &registry.toc_entries {
        if entry.level > registry.tocdepth {
            continue;
        }
        // (indent, number width) of \l@section and \@dottedtocline{2}{1.5em}{2.3em},
        // {3}{3.8em}{3.2em}.
        let (indent, num_w, face, dotted) = match entry.level {
            1 => (0.0, crate::backend::scaled::em_mul(1.5, em), Face::Bold, false),
            2 => (crate::backend::scaled::em_mul(1.5, em), crate::backend::scaled::em_mul(2.3, em), Face::Roman, true),
            _ => (crate::backend::scaled::em_mul(3.8, em), crate::backend::scaled::em_mul(3.2, em), Face::Roman, true),
        };
        // Each entry is a paragraph; \l@section adds \@secpenalty before it (\addpenalty
        // does nothing right after the heading).
        if layouter.after_heading {
            layouter.after_heading = false;
        } else {
            layouter.page_breakpoint(if entry.level == 1 { crate::layout::layout_pagebuilder::SEC_PENALTY } else { 0 });
        }
        if entry.level == 1 {
            // \addvspace{1em plus 1pt}.
            layouter.vglue((em - last_skip).max(0.0), 1.0, 0.0);
        }
        last_skip = 0.0;
        // The entry's line box: its title's letters and the page number's digits (the
        // number box adds nothing taller); its depth goes to `\prevdepth`, and into the page's
        // natural height when the page ends here.
        // The title goes through the paragraph path (fonts, kerns, `\ `, logos), set as one
        // line at its natural width.
        layouter.cell_words_soa.clear();
        let mut math = Vec::new();
        let title_w = crate::layout::layout_engine::explode_inlines_into_family(&entry.title, face, size, &mut layouter.cell_words_soa, family, &mut math);
        let n_words = layouter.cell_words_soa.len();
        let (line_h, line_d) = {
            let (h, d) = layouter.cell_words_soa.line_extent(0, n_words, size, family);
            let digit_h = crate::math::math_fonts::char_height_depth('0', face, family).0 * size / 1000.0;
            (h.max(digit_h), d)
        };
        layouter.place_baseline(line_h);

        if !layouter.is_dry_run {
            // An unnumbered heading's entry has no `\numberline` box.
            if entry.numbered {
                layouter.push_op(Paint::SectionNumber {
                    x: (left_margin + indent).into(),
                    y: layouter.y,
                    face,
                    size,
                    number: entry.number,
                    level: entry.level,
                });
            }
            let title_x = left_margin + indent + if entry.numbered { num_w } else { 0.0 };
            let line = crate::layout::layout_engine::CellLineRange::natural(0, n_words as u16, title_w);
            layouter.paint_cell_line_at(line, title_x, layouter.y, size, &math);

            if dotted {
                // Aligned leaders: boxes at multiples of their width from the line's left
                // edge, between the title and the \@pnumwidth box.
                let glue_start = title_x + title_w;
                let glue_end = right_margin - crate::backend::scaled::em_mul(1.55, em);
                let mut box_x = left_margin + ((glue_start - left_margin) / leader_w).ceil() * leader_w;
                let dot_pool_start = layouter.current_page.text_pool.len() as u32;
                layouter.current_page.text_pool.push(".");
                while box_x + leader_w <= glue_end + 0.001 {
                    layouter.push_op(Paint::GlyphRun {
                        x: (box_x + dot_kern).into(),
                        y: layouter.y,
                        face: Face::Roman,
                        size,
                        part_start: dot_pool_start,
                        part_count: 1,
                        word_spacing: 0.0,
                    });
                    box_x += leader_w;
                }
            }
            // The `\@pnumwidth` box after the leaders, its digits flush right.
            let digit_w = crate::backend::scaled::fix_metrics(family, face, size).map_or_else(
                || crate::font_metrics::resolve_digit_units_at(family, face, size) as f32 * size * 0.001,
                |m| crate::backend::scaled::Scaler::at_pt(size).scale(m.widths[b'0' as usize] as i32) as f32 / 65536.0,
            );
            let page_x = Sp::pt(right_margin) - Sp::pt(digit_w * count_digits(entry.page) as f32);
            layouter.push_op(Paint::PageNumber {
                x: page_x,
                y: layouter.y,
                face,
                size,
                page_number: entry.page,
            });
        }

        layouter.prev_depth = line_d;
        layouter.y -= layouter.base_leading;
    }
}
