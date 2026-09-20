//! Tables subsystem for rushtex.
//!
//! Provides a dedicated, modular domain for table parsing, column specification,
//! `booktabs` professional rules, `longtable` multi-page streaming, and vector layout.

pub mod tables_layout;
pub mod tables_model;
pub mod tables_parser;
pub mod tables_spec;

// Backward-compatible module aliases:
pub use tables_layout as layout;
pub use tables_model as model;
pub use tables_parser as parser;
pub use tables_spec as spec;

pub(crate) use tables_layout::layout_table;
pub use tables_model::{TableCell, TableData, TableRow, TableRule};
pub(crate) use tables_parser::{parse_longtable, parse_table_float, parse_tabular};
pub use tables_spec::{ColAlign, ColSpec};

#[cfg(test)]
mod tests {
    use crate::{compile_str, compile_str_opts};

    #[test]
    fn test_booktabs_rules_parsing_and_painting() {
        let src = r#"
\documentclass{article}
\begin{document}
\begin{tabular}{llr}
\toprule
\textbf{Item} & \textbf{Category} & \textbf{Price} \\
\midrule
Apples & Fruit & \$1.50 \\
Oranges & Fruit & \$2.00 \\
\cmidrule(lr){2-3}
Bread & Bakery & \$3.00 \\
\bottomrule
\end{tabular}
\end{document}
"#;
        let pdf = compile_str(src).expect("Compilation of booktabs table should succeed");
        assert!(!pdf.is_empty());
        assert!(pdf.starts_with(b"%PDF-1.4"));
    }

    #[test]
    fn test_multicolumn_spanning() {
        let src = r#"
\documentclass{article}
\begin{document}
\begin{tabular}{|l|c|r|}
\hline
\multicolumn{2}{|c|}{\textbf{Span 2 Cols}} & Right \\
\hline
Cell 1 & Cell 2 & Cell 3 \\
\hline
\end{tabular}
\end{document}
"#;
        let pdf = compile_str(src).expect("Compilation of multicolumn table should succeed");
        assert!(!pdf.is_empty());
        assert!(pdf.starts_with(b"%PDF-1.4"));
    }

    #[test]
    fn test_fixed_width_paragraph_column() {
        let src = r#"
\documentclass{article}
\begin{document}
\begin{tabular}{|p{5cm}|l|}
\hline
This is a long paragraph of text designed to test automatic line wrapping within a fixed-width p column inside a table. & Short \\
\hline
Row 2 content & Second \\
\hline
\end{tabular}
\end{document}
"#;
        let pdf = compile_str(src).expect("Compilation of p{width} column table should succeed");
        assert!(!pdf.is_empty());
        assert!(pdf.starts_with(b"%PDF-1.4"));
    }

    #[test]
    fn test_cline_partial_rules() {
        let src = r#"
\documentclass{article}
\begin{document}
\begin{tabular}{|c|c|c|}
\hline
A & B & C \\
\cline{1-2}
D & E & F \\
\cline{2-3}
G & H & I \\
\hline
\end{tabular}
\end{document}
"#;
        let pdf = compile_str(src).expect("Compilation of cline table should succeed");
        assert!(!pdf.is_empty());
        assert!(pdf.starts_with(b"%PDF-1.4"));
    }

    #[test]
    fn test_longtable_multipage_streaming() {
        let mut rows = String::new();
        for i in 1..=60 {
            rows.push_str(&format!(
                "Item {i} & Description {i} & Value {i} \\\\\n\\hline\n"
            ));
        }
        let src = format!(
            r#"
\documentclass{{article}}
\begin{{document}}
\begin{{longtable}}{{|l|c|r|}}
\hline
\textbf{{Item}} & \textbf{{Description}} & \textbf{{Value}} \\
\hline
\endfirsthead
\hline
\textbf{{Item (Cont.)}} & \textbf{{Description}} & \textbf{{Value}} \\
\hline
\endhead
\hline
\multicolumn{{3}}{{|r|}}{{Continued on next page...}} \\
\hline
\endfoot
{rows}
\end{{longtable}}
\end{{document}}
"#
        );
        let pdf = compile_str(&src).expect("Compilation of multi-page longtable should succeed");
        assert!(!pdf.is_empty());
        assert!(pdf.starts_with(b"%PDF-1.4"));
        // Check that multi-page document was generated
        let pdf_str = String::from_utf8_lossy(&pdf);
        assert!(pdf_str.contains("/Type /Pages"));
    }

    #[test]
    fn test_floating_table_with_caption() {
        let src = r#"
\documentclass{article}
\begin{document}
\begin{table}[htbp]
\caption{System Performance Benchmark Results}
\begin{tabular}{|l|r|}
\hline
\textbf{Metric} & \textbf{Value} \\
\hline
Throughput & 1500 pages/sec \\
Latency & 0.65 ms \\
\hline
\end{tabular}
\end{table}

\begin{table}[h]
\begin{tabular}{cc}
A & B \\
C & D \\
\end{tabular}
\caption{Second Table Caption Below}
\end{table}
\end{document}
"#;
        let pdf = compile_str_opts(src, false)
            .expect("Compilation of floating tables with captions should succeed");
        assert!(!pdf.is_empty());
        assert!(pdf.starts_with(b"%PDF-1.4"));
        let pdf_str = String::from_utf8_lossy(&pdf);
        assert!(pdf_str.contains("Table "));
        assert!(pdf_str.contains("1: "));
        assert!(pdf_str.contains("2: "));
        assert!(pdf_str.contains("System Performance Benchmark Results"));
        assert!(pdf_str.contains("Second Table Caption Below"));
    }
}
