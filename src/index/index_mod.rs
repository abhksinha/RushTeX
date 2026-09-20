//! Document indexing and catalog subsystem for rushtex.
//!
//! Provides a dedicated, modular domain for tracking document headings,
//! figure captions, table captions, and rendering Table of Contents (`\tableofcontents`),
//! List of Figures (`\listoffigures`), and List of Tables (`\listoftables`).

pub mod index_lof_lot;
pub mod index_model;
pub mod index_toc;

// Backward-compatible module aliases:
pub use index_lof_lot as lof_lot;
pub use index_model as model;
pub use index_toc as toc;

pub(crate) use index_lof_lot::{layout_lof, layout_lot};
pub use index_model::{FigureEntry, IndexRegistry, TableEntry, TocEntry};
pub(crate) use index_toc::layout_toc;

use crate::frontend::ast::Block;

/// Scans the document block tree and populates the registry with all headings,
/// figure captions, and table captions in document order prior to layout.
pub fn pre_scan_entries<'a>(blocks: &'a [Block<'a>], registry: &mut IndexRegistry<'a>) {
    let mut section_counters = [0u16; 4];
    let mut fig_counter = 0u32;
    let mut table_counter = 0u32;

    scan_inner(
        blocks,
        registry,
        &mut section_counters,
        &mut fig_counter,
        &mut table_counter,
    );
}

fn scan_inner<'a>(
    blocks: &'a [Block<'a>],
    registry: &mut IndexRegistry<'a>,
    section_counters: &mut [u16; 4],
    fig_counter: &mut u32,
    table_counter: &mut u32,
) {
    for b in blocks {
        match b {
            Block::Section {
                level,
                starred,
                title,
            } => {
                if !starred {
                    let lvl = (*level as usize).clamp(1, 4);
                    section_counters[lvl - 1] += 1;
                    section_counters[lvl..4].fill(0);
                    registry.toc_entries.push(TocEntry {
                        level: *level,
                        number: *section_counters,
                        title,
                        page: 1,
                    });
                }
            }
            Block::Figure(f) => {
                *fig_counter += 1;
                registry.figure_entries.push(FigureEntry {
                    number: *fig_counter,
                    caption: &f.caption,
                    page: 1,
                });
            }
            Block::Table(t) => {
                if let Some(ref cap) = t.caption {
                    *table_counter += 1;
                    registry.table_entries.push(TableEntry {
                        number: *table_counter,
                        caption: cap,
                        page: 1,
                    });
                }
            }
            Block::List(list_data) => {
                for item in &list_data.items {
                    scan_inner(
                        &item.blocks,
                        registry,
                        section_counters,
                        fig_counter,
                        table_counter,
                    );
                }
            }
            Block::Quote(quote_data) => {
                scan_inner(
                    &quote_data.blocks,
                    registry,
                    section_counters,
                    fig_counter,
                    table_counter,
                );
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::parse;
    use crate::layout::engine::{layout, Paint};

    #[test]
    fn test_index_struct_cache_line_fit() {
        let toc_size = std::mem::size_of::<TocEntry>();
        let fig_size = std::mem::size_of::<FigureEntry>();
        let tbl_size = std::mem::size_of::<TableEntry>();
        let paint_size = std::mem::size_of::<Paint>();

        assert!(
            toc_size <= 64,
            "TocEntry must fit in a 64B L1 cache line, got {toc_size}"
        );
        assert!(
            fig_size <= 32,
            "FigureEntry must be <= 32 bytes, got {fig_size}"
        );
        assert!(
            tbl_size <= 32,
            "TableEntry must be <= 32 bytes, got {tbl_size}"
        );
        assert!(
            paint_size <= 32,
            "Paint must be <= 32 bytes, got {paint_size}"
        );
    }

    #[test]
    fn test_table_of_contents_generation() {
        let tex = r#"
\documentclass{article}
\begin{document}
\tableofcontents

\section{Introduction}
This is the intro.

\subsection{Background}
Some background info.

\subsubsection{Details}
Deep details.

\section{Methodology}
Our awesome methods.

\subsection{Evaluation}
Numbers and metrics.
\end{document}
"#;
        let doc = parse(tex).expect("parse failed");
        let pages = layout(&doc);
        assert!(!pages.is_empty());

        let page1 = &pages[0];
        // Check "Contents" glyph run
        let has_contents_header = page1.ops.iter().any(|op| match op {
            Paint::GlyphRun {
                part_start,
                part_count,
                ..
            } => {
                let s =
                    &page1.text_pool[*part_start as usize..(*part_start + *part_count) as usize];
                s.contains(&"Contents")
            }
            _ => false,
        });
        assert!(has_contents_header, "Expected 'Contents' heading in ToC");

        // Check section numbers in ToC and body headings (5 in ToC + 5 in body)
        let sec_numbers: Vec<([u16; 4], u8)> = page1
            .ops
            .iter()
            .filter_map(|op| match op {
                Paint::SectionNumber { number, level, .. } => Some((*number, *level)),
                _ => None,
            })
            .collect();
        assert_eq!(sec_numbers.len(), 10);
        assert_eq!(sec_numbers[0], ([1, 0, 0, 0], 1)); // ToC Section 1
        assert_eq!(sec_numbers[1], ([1, 1, 0, 0], 2)); // ToC Subsection 1.1
        assert_eq!(sec_numbers[2], ([1, 1, 1, 0], 3)); // ToC Subsubsection 1.1.1
        assert_eq!(sec_numbers[3], ([2, 0, 0, 0], 1)); // ToC Section 2
        assert_eq!(sec_numbers[4], ([2, 1, 0, 0], 2)); // ToC Subsection 2.1
        assert_eq!(sec_numbers[5], ([1, 0, 0, 0], 1)); // Body Section 1
        assert_eq!(sec_numbers[6], ([1, 1, 0, 0], 2)); // Body Subsection 1.1
        assert_eq!(sec_numbers[7], ([1, 1, 1, 0], 3)); // Body Subsubsection 1.1.1
        assert_eq!(sec_numbers[8], ([2, 0, 0, 0], 1)); // Body Section 2
        assert_eq!(sec_numbers[9], ([2, 1, 0, 0], 2)); // Body Subsection 2.1

        // Check dotted leaders
        let dot_count = page1
            .ops
            .iter()
            .filter(|op| match op {
                Paint::GlyphRun {
                    part_start,
                    part_count,
                    ..
                } => {
                    let s = &page1.text_pool
                        [*part_start as usize..(*part_start + *part_count) as usize];
                    s.contains(&".")
                }
                _ => false,
            })
            .count();
        assert!(
            dot_count >= 5,
            "Expected dotted leaders for each section entry"
        );

        // Check page numbers in ToC
        let page_nums: Vec<u32> = page1
            .ops
            .iter()
            .filter_map(|op| match op {
                Paint::PageNumber { page_number, .. } => Some(*page_number),
                _ => None,
            })
            .collect();
        // There should be 5 page numbers in ToC plus 1 footer page number
        assert!(page_nums.len() >= 5);
    }

    #[test]
    fn test_list_of_figures_and_tables() {
        let tex = r#"
\documentclass{article}
\begin{document}
\listoffigures
\listoftables

\begin{figure}
\caption{System Architecture Overview}
\end{figure}

\begin{table}
\caption{Benchmark Performance Results}
\begin{tabular}{|l|c|}
\hline
Name & Score \\
\hline
rushtex & 100 \\
\hline
\end{tabular}
\end{table}
\end{document}
"#;
        let doc = parse(tex).expect("parse failed");
        let pages = layout(&doc);
        assert!(!pages.is_empty());

        let page1 = &pages[0];
        let has_lof_header = page1.ops.iter().any(|op| match op {
            Paint::GlyphRun {
                part_start,
                part_count,
                ..
            } => {
                let s =
                    &page1.text_pool[*part_start as usize..(*part_start + *part_count) as usize];
                s.contains(&"List of Figures")
            }
            _ => false,
        });
        assert!(has_lof_header, "Expected 'List of Figures' heading");

        let has_lot_header = page1.ops.iter().any(|op| match op {
            Paint::GlyphRun {
                part_start,
                part_count,
                ..
            } => {
                let s =
                    &page1.text_pool[*part_start as usize..(*part_start + *part_count) as usize];
                s.contains(&"List of Tables")
            }
            _ => false,
        });
        assert!(has_lot_header, "Expected 'List of Tables' heading");

        // Verify FigureTag and TableTag are rendered
        let has_fig_tag = page1
            .ops
            .iter()
            .any(|op| matches!(op, Paint::FigureTag { n: 1, .. }));
        let has_tbl_tag = page1
            .ops
            .iter()
            .any(|op| matches!(op, Paint::TableTag { n: 1, .. }));
        assert!(has_fig_tag, "Expected FigureTag in LoF");
        assert!(has_tbl_tag, "Expected TableTag in LoT");
    }

    #[test]
    fn test_toc_multipage_pagination() {
        let tex = r#"
\documentclass{article}
\begin{document}
\tableofcontents

\newpage
\section{First Page After ToC}
Content on page 2.

\newpage
\section{Second Page After ToC}
Content on page 3.
\end{document}
"#;
        let doc = parse(tex).expect("parse failed");
        let pages = layout(&doc);
        assert_eq!(pages.len(), 3);

        // On page 1, check that the two sections have page targets 2 and 3!
        let page1 = &pages[0];
        let toc_page_targets: Vec<u32> = page1
            .ops
            .iter()
            .filter_map(|op| match op {
                Paint::PageNumber { page_number, y, .. } if *y > 100.0 => Some(*page_number),
                _ => None,
            })
            .collect();

        assert_eq!(
            toc_page_targets,
            vec![2, 3],
            "ToC must resolve target page numbers accurately across page boundaries"
        );
    }
}
