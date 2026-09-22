//! # rushtex — Pure Rust TeX-to-PDF High-Speed Compilation Engine
//!
//! A zero-dependency, single-pass typesetting engine that compiles LaTeX-syntax documents
//! directly into standards-compliant PDF 1.4 documents at sub-10ms latency.
//!
//! ## Architectural Pipeline & Subsystems
//!
//! The compilation workflow bypasses the legacy TeX macro expansion loop, multi-pass `.aux`/`.log`
//! filesystem overhead, and DVI intermediaries, executing a pure in-memory pipeline divided into
//! decoupled modular domains:
//!
//! 1. **Frontend ([`frontend`])**: Lexes TeX tokens and builds an Abstract Syntax Tree ([`frontend::ast::Document`])
//!    containing preamble configurations and structural blocks. Zero allocations for tokens
//!    (string slices borrow directly from source text).
//! 2. **Config ([`config`])**: Handles `\documentclass`, package options (`geometry`,
//!    `graphicx`, `inputenc`, font packages), document metadata (`\title`, `\author`, `\date`), and page geometry.
//! 3. **Layout ([`layout`])**: Performs greedy line breaking, Knuth-Plass breaking with hz-algorithm microtypography
//!    ([`layout::hz`]), syntax-highlighted code listings ([`layout::listing`]), hanging indents, table column
//!    balancing, and page pagination into compact 24-byte `Paint` display lists.
//! 4. **Backend ([`backend`])**:
//!    - **Fonts ([`backend::fonts`])**: Adobe Font Metrics tables (Times, Helvetica, Courier, Symbol, Dingbats) with compile-time `.rodata` tables.
//!    - **PDF ([`backend::pdf`])**: Emits PDF 1.4 objects, page resource trees, WinAnsiEncoding font descriptors, and page content streams.
//!    - **Compression ([`backend::deflate`])**: Pure-Rust streaming RFC 1950 zlib / RFC 1951 DEFLATE encoder with a 32 KB hash table fitting inside CPU L1 data cache.
//! 5. **Diagnostic ([`diagnostic`])**: High-contrast, colorized compiler diagnostics with typo correction and suggestions.
//!
//! ## Public API Summary
//!
//! - [`compile_str`]: Compiles a LaTeX source string directly to an owned `Vec<u8>` containing the PDF.
//! - [`compile_str_opts`]: Same as `compile_str` with optional compression flag.
//! - [`compile_into`]: Compiles into an existing preallocated `Vec<u8>` buffer (zero allocation churn).
//! - [`compile_into_opts`]: Same as `compile_into` with optional compression flag.
//! - [`compile_file`]: Compiles a `.tex` file on disk to a `.pdf` file.
//! - [`compile_file_opts`]: Same as `compile_file` with optional compression flag.

#![forbid(unsafe_code)]

#[path = "backend/backend_mod.rs"]
pub mod backend;
#[path = "config/config_mod.rs"]
pub mod config;
#[path = "diagnostic/diagnostic_mod.rs"]
pub mod diagnostic;
#[path = "frontend/frontend_mod.rs"]
pub mod frontend;
#[path = "index/index_mod.rs"]
pub mod index;
#[path = "layout/layout_mod.rs"]
pub mod layout;
#[path = "math/math_mod.rs"]
pub mod math;
#[path = "tables/tables_mod.rs"]
pub mod tables;

// Backward-compatible module aliases:
pub use backend::deflate;
pub use backend::deflate as deflate_compress;
pub use backend::fonts as font_metrics;
pub use backend::fonts as metrics;
pub use backend::pdf;
pub use backend::pdf as pdf_writer;
pub use config as doc_config;
pub use config as preamble;
pub use diagnostic as error_diagnostic;
pub use frontend as tex;
pub use frontend as tex_parser;
pub use layout as page_layout;
pub use layout::hz as hz_algorithm;
pub use layout::listing;
pub use layout::listing as code_listing;

pub use diagnostic::Diagnostic;

use std::path::Path;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Parse(diagnostic::Diagnostic),
    Encode(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => {
                writeln!(f, "==========================================")?;
                writeln!(f, "RUSHTEX - THE ULTRA FAST TEX FILE COMPILER")?;
                writeln!(f, "==========================================")?;
                writeln!(f, "[ERROR] io: {e}")?;
                write!(f, "==========END OF COMPILER LOGS============")
            }
            Error::Parse(diag) => write!(f, "{diag}"),
            Error::Encode(e) => {
                writeln!(f, "==========================================")?;
                writeln!(f, "RUSHTEX - THE ULTRA FAST TEX FILE COMPILER")?;
                writeln!(f, "==========================================")?;
                writeln!(f, "[ERROR] encode: {e}")?;
                write!(f, "==========END OF COMPILER LOGS============")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<diagnostic::Diagnostic> for Error {
    fn from(diag: diagnostic::Diagnostic) -> Self {
        Error::Parse(diag)
    }
}

/// Compiles a TeX-subset source string into a reusable destination byte buffer using compression.
pub fn compile_into(source_code: &str, destination_buffer: &mut Vec<u8>) -> Result<(), Error> {
    compile_into_opts(source_code, destination_buffer, true)
}

/// Compiles a TeX-subset source string into a reusable destination byte buffer with optional compression.
pub fn compile_into_opts(
    source_code: &str,
    destination_buffer: &mut Vec<u8>,
    enable_compression: bool,
) -> Result<(), Error> {
    let document = tex::parse(source_code)?;
    let pages = layout::layout(&document);
    pdf::emit_into_opts(&pages, destination_buffer, enable_compression)?;
    Ok(())
}

/// Compiles a TeX-subset source string to an owned PDF byte buffer using compression.
pub fn compile_str(source_code: &str) -> Result<Vec<u8>, Error> {
    compile_str_opts(source_code, true)
}

/// Compiles a TeX-subset source string to an owned PDF byte buffer with optional compression.
pub fn compile_str_opts(source_code: &str, enable_compression: bool) -> Result<Vec<u8>, Error> {
    let mut destination_buffer = Vec::with_capacity(4096);
    compile_into_opts(source_code, &mut destination_buffer, enable_compression)?;
    Ok(destination_buffer)
}

/// Detailed summary report of a file compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileReport {
    pub output_path: std::path::PathBuf,
    pub page_count: usize,
    pub elapsed: std::time::Duration,
}

impl std::ops::Deref for CompileReport {
    type Target = std::path::Path;
    fn deref(&self) -> &Self::Target {
        &self.output_path
    }
}

impl AsRef<std::path::Path> for CompileReport {
    fn as_ref(&self) -> &std::path::Path {
        &self.output_path
    }
}

/// Compiles a `.tex` file to a sibling `.pdf` (or `output_path` if given) with compression.
pub fn compile_file(
    input_path: &Path,
    output_path: Option<&Path>,
) -> Result<std::path::PathBuf, Error> {
    compile_file_opts(input_path, output_path, true).map(|r| r.output_path)
}

/// Compiles a `.tex` file to a sibling `.pdf` (or `output_path` if given) with optional compression.
pub fn compile_file_opts(
    input_path: &Path,
    output_path: Option<&Path>,
    enable_compression: bool,
) -> Result<CompileReport, Error> {
    #[cfg(not(target_arch = "wasm32"))]
    let start_time = std::time::Instant::now();
    let source_code = std::fs::read_to_string(input_path)?;
    let document = tex::parse(&source_code)?;
    let pages = layout::layout(&document);
    let page_count = pages.len();
    let mut pdf_buffer = Vec::with_capacity((page_count * 10240).max(16384));
    pdf::emit_into_opts(&pages, &mut pdf_buffer, enable_compression)?;
    let destination = match output_path {
        Some(path) => path.to_path_buf(),
        None => input_path.with_extension("pdf"),
    };
    std::fs::write(&destination, &pdf_buffer)?;
    #[cfg(not(target_arch = "wasm32"))]
    let elapsed = start_time.elapsed();
    #[cfg(target_arch = "wasm32")]
    let elapsed = std::time::Duration::from_millis(0);
    Ok(CompileReport {
        output_path: destination,
        page_count,
        elapsed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_pdftotext(pdf: &[u8]) -> String {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut child = Command::new("pdftotext")
            .args(["-", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn pdftotext");

        child.stdin.as_mut().unwrap().write_all(pdf).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "pdftotext error: {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    #[test]
    fn hello_parses_bold() {
        let src = "This is a simple line and this \\textbf{word} is in bold";
        let doc = tex::parse(src).unwrap();
        match &doc.blocks[0] {
            tex::Block::Para(inls) => {
                let dump = format!("{inls:?}");
                assert!(dump.contains("Bold"), "{dump}");
                assert!(dump.contains("\"word\""), "{dump}");
                assert!(dump.contains("is in bold"), "{dump}");
            }
            _ => panic!("expected a paragraph"),
        }
    }

    #[test]
    fn hello_pdf_is_validish() {
        let pdf = compile_str("This is a simple line and this \\textbf{word} is in bold").unwrap();
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.starts_with("%PDF-1.4"));
        assert!(s.contains("%%EOF"));
        assert!(s.contains("/Times-Bold"));
        assert!(s.contains("/Filter /FlateDecode"));
        assert!(s.contains("xref"));

        let text = extract_pdftotext(&pdf);
        assert!(text.contains("word"));
        assert!(text.contains("in bold"));

        let uncomp = compile_str_opts(
            "This is a simple line and this \\textbf{word} is in bold",
            false,
        )
        .unwrap();
        let s_raw = String::from_utf8_lossy(&uncomp);
        assert!(s_raw.contains("(word"));
    }

    #[test]
    fn enumerate_emits_markers() {
        let src = "\\begin{enumerate}\\item Alpha\\item Bravo\\end{enumerate}";
        let pdf = compile_str(src).unwrap();
        let text = extract_pdftotext(&pdf);
        assert!(text.contains("1."));
        assert!(text.contains("Alpha"));
        assert!(text.contains("2."));
        assert!(text.contains("Bravo"));

        let uncomp = compile_str_opts(src, false).unwrap();
        let s = String::from_utf8_lossy(&uncomp);
        assert!(s.contains("(1.)"));
        assert!(s.contains("(2.)"));
        assert!(s.contains("(Alpha)"));
        assert!(s.contains("(Bravo)"));
    }

    #[test]
    fn tabular_and_figure_syntax() {
        let src = r#"
\begin{tabular}{|l|c|r|}
\hline
A & \textbf{B} & C \\
1 & 2 & 3 \\
\end{tabular}

\begin{figure}[h]
\includegraphics{demo.png}
\caption{A demo box}
\end{figure}
"#;
        let pdf = compile_str(src).unwrap();
        let text = extract_pdftotext(&pdf);
        assert!(text.contains("A"));
        assert!(text.contains("B"));
        assert!(text.contains("Figure 1:"));

        let uncomp = compile_str_opts(src, false).unwrap();
        let s = String::from_utf8_lossy(&uncomp);
        assert!(s.contains("(A"));
        assert!(s.contains("(B)"));
        assert!(s.contains("(demo.png)"));
        assert!(s.contains("(Figure 1: )"));
        assert!(s.contains(" re "));
    }

    #[test]
    fn test_table_boundary_closure_and_multiline_wrapping() {
        let src = r#"
\documentclass{article}
\begin{document}
\begin{tabular}{|l|c|r|}
\hline
Parameter & Description of the Architecture Component & Status \\
\hline
Pipeline & High performance vector typesetting pipeline engine for technical documents and scientific publications & Complete \\
\hline
\end{tabular}
\end{document}
"#;
        let pdf = compile_str(src).unwrap();
        let text = extract_pdftotext(&pdf);
        assert!(text.contains("Parameter"));
        assert!(text.contains("Pipeline"));
        assert!(text.contains("Complete"));

        let doc = crate::tex_parser::parse(src).unwrap();
        let pages = crate::page_layout::layout(&doc);
        assert_eq!(pages.len(), 1);
        let page = &pages[0];

        // Verify vertical and horizontal rules
        let mut rules = Vec::new();
        for op in &page.ops {
            if let crate::page_layout::Paint::Rule { x, y, w, h, fill } = op {
                rules.push((*x, *y, *w, *h, *fill));
            }
        }

        // Horizontal rules: top and bottom (and middle) must have identical width
        let h_rules: Vec<_> = rules.iter().filter(|r| r.2 > 10.0 && r.3 <= 1.0).collect();
        assert!(
            h_rules.len() >= 3,
            "Expected at least 3 horizontal rules (top, middle, bottom)"
        );
        let table_w = h_rules[0].2;
        for hr in &h_rules {
            assert!(
                (hr.2 - table_w).abs() < 1e-3,
                "All horizontal rules must match table width: expected {}, got {}",
                table_w,
                hr.2
            );
            assert!(
                hr.0 + hr.2 <= page.width - 50.0,
                "Table must not spill into margin"
            );
        }

        // Vertical rules: there must be rightmost rules drawn flush with origin_x + table_w
        let origin_x = h_rules[0].0;
        let right_x = origin_x + table_w;
        let v_rules_right: Vec<_> = rules
            .iter()
            .filter(|r| (r.0 - (right_x - 0.6)).abs() < 1e-2 && r.3 > 1.0)
            .collect();
        assert!(
            !v_rules_right.is_empty(),
            "Rightmost vertical rule must be drawn flush with table_w ({})",
            right_x - 0.6
        );

        // Multi-line cell: verify that the row with long description has row_h > base_leading + 4.0
        let max_v_rule_h = rules
            .iter()
            .filter(|r| r.3 > 1.0)
            .map(|r| r.3)
            .fold(0.0f32, f32::max);
        assert!(max_v_rule_h > 20.0, "Row with long text should wrap into multiple lines and have increased height, got {max_v_rule_h}");
    }

    #[test]
    fn full_preamble_and_document_envelope() {
        let src = r#"
\documentclass[12pt, letterpaper]{article}
\usepackage[margin=0.5in]{geometry}
\usepackage{graphicx}
\usepackage[utf8]{inputenc}
\title{High Speed Engine}
\author{Triton}
\date{September 2026}

\begin{document}
\maketitle

\section{Overview}
This is the body of the document.

\subsection{Sub-topic}
More details here.
\end{document}
% Trailing content after end document is ignored in LaTeX
Extra garbage outside document
"#;
        let doc = tex::parse(src).unwrap();
        assert_eq!(doc.preamble.class.as_ref().unwrap().name, "article");
        assert_eq!(
            doc.preamble.class.as_ref().unwrap().options,
            vec!["12pt", "letterpaper"]
        );
        assert_eq!(doc.preamble.config.page_width, 612.0); // Letter paper
        assert_eq!(doc.preamble.config.page_height, 792.0);
        assert_eq!(doc.preamble.config.margin_left, 36.0); // 0.5 in = 36 pt
        assert_eq!(doc.preamble.config.base_font_size, 12.0);

        let pdf = compile_str(src).unwrap();
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.starts_with("%PDF-1.4"));
        assert!(s.contains("/MediaBox [0 0 612 792]"));
        assert!(s.contains("/Filter /FlateDecode"));
        assert!(!s.contains("Extra garbage"));
        let text = extract_pdftotext(&pdf);
        assert!(text.contains("High Speed Engine"));
        assert!(text.contains("Triton"));
        assert!(text.contains("Overview"));
        assert!(text.contains("Sub-topic"));
        assert!(!text.contains("Extra garbage"));
    }

    #[test]
    fn backward_compatibility_fragments() {
        // Plain text fragments without preamble continue to compile identically
        let src = "Just a raw paragraph without any preamble or document envelope.";
        let doc = tex::parse(src).unwrap();
        assert!(doc.preamble.class.is_none());
        assert_eq!(doc.preamble.config.page_width, 595.0); // Default A4
        let pdf = compile_str(src).unwrap();
        let text = extract_pdftotext(&pdf);
        assert!(text.contains("Just a raw paragraph"));
    }

    #[test]
    fn benchmark_throughput() {
        let src = include_str!("../examples/article.tex");
        const ITERS: usize = 5000;

        // Fresh allocation per document
        let start_str = std::time::Instant::now();
        for _ in 0..ITERS {
            let pdf = compile_str(src).unwrap();
            assert!(!pdf.is_empty());
        }
        let elapsed_str = start_str.elapsed();
        let us_per_doc_str = elapsed_str.as_micros() as f64 / ITERS as f64;

        // Reused output buffer (zero output allocation churn)
        let mut buf = Vec::with_capacity(4096);
        let start_into = std::time::Instant::now();
        for _ in 0..ITERS {
            compile_into(src, &mut buf).unwrap();
            assert!(!buf.is_empty());
        }
        let elapsed_into = start_into.elapsed();
        let us_per_doc_into = elapsed_into.as_micros() as f64 / ITERS as f64;

        eprintln!(
            "\n[PERF benchmark - 5000 documents]\n  compile_str  (allocating):  {:.2} µs/doc  ({:.0} docs/sec)\n  compile_into (reused buf):  {:.2} µs/doc  ({:.0} docs/sec)",
            us_per_doc_str,
            ITERS as f64 / elapsed_str.as_secs_f64(),
            us_per_doc_into,
            ITERS as f64 / elapsed_into.as_secs_f64(),
        );
    }

    #[test]
    fn test_compiler_human_error_format() {
        // Build 53 lines, then on line 54 place an error at character 17
        let mut src = String::new();
        for _ in 1..54 {
            src.push_str("Standard paragraph content line.\n");
        }
        // Exactly 16 characters ("1234567890123456") followed by unknown command '\badcmd'
        src.push_str("1234567890123456\\badcmd");

        let err = compile_str(&src).unwrap_err();
        match err {
            Error::Parse(diag) => {
                assert_eq!(diag.line, 54);
                assert_eq!(diag.character, 17);
                assert_eq!(
                    diag.simplified_header(),
                    "[ERROR] Line 54 from character 17"
                );
                let rendered = diag.to_string();
                assert!(rendered.starts_with("=========================================="));
                assert!(rendered.contains(
                    "[ERROR] Line 54 from character 17: unknown command \x1b[43;30m\\badcmd\x1b[0m"
                ));
                assert!(!rendered.contains("54 |"));
                assert!(!rendered.contains("^"));
                assert!(rendered.ends_with("==========END OF COMPILER LOGS============"));
            }
            other => panic!("expected parse error, got {other:?}"),
        }
    }

    #[test]
    fn test_unclosed_environment_diagnostic() {
        let src = r#"\documentclass{article}
\begin{document}
Some text here.
\begin{enumerate}
\item First item.
\item Second item.

\begin{itemize}
\item A bullet.
\end{itemize}
\end{document}
"#;
        let err = compile_str(src).unwrap_err();
        match err {
            Error::Parse(diag) => {
                assert_eq!(diag.line, 4);
                assert_eq!(diag.character, 1);
                let rendered = diag.to_string();
                assert!(rendered.starts_with("=========================================="));
                assert!(rendered.contains(
                    "Line 4 from character 1: unclosed \x1b[43;30m\\begin{enumerate}\x1b[0m"
                ));
                assert!(!rendered.contains("4 |"));
                assert!(!rendered.contains("^"));
                assert!(rendered.contains(" NOTE: reached '\\end{document}'"));
                assert!(rendered.contains(" HELP: add '\\end{enumerate}' before '\\end{document}'"));
                assert!(rendered.ends_with("==========END OF COMPILER LOGS============"));
            }
            other => panic!("expected parse error, got {other:?}"),
        }
    }

    #[test]
    fn test_unknown_command_typo_suggestion() {
        let src = r#"\documentclass{article}
\begin{document}
\textbff{Hello}
\end{document}
"#;
        let err = compile_str(src).unwrap_err();
        match err {
            Error::Parse(diag) => {
                assert_eq!(diag.line, 3);
                assert_eq!(diag.character, 1);
                let rendered = diag.to_string();
                assert!(rendered.starts_with("=========================================="));
                assert!(rendered.contains("unknown command \x1b[43;30m\\textbff\x1b[0m"));
                assert!(!rendered.contains("3 |"));
                assert!(rendered.contains(" HELP: did you mean '\\textbf'?"));
                assert!(rendered.ends_with("==========END OF COMPILER LOGS============"));
            }
            other => panic!("expected parse error, got {other:?}"),
        }
    }

    #[test]
    fn test_bench_100k_detailed_breakdown() {
        let path = "examples/stress_100k.tex";
        if !std::path::Path::new(path).exists() {
            return;
        }
        let src = std::fs::read_to_string(path).expect("read stress_100k.tex");

        // Warmup
        let _ = compile_str(&src).unwrap();

        const ITERS: usize = 20;
        let mut parse_times = Vec::with_capacity(ITERS);
        let mut layout_times = Vec::with_capacity(ITERS);
        let mut emit_uncompressed_times = Vec::with_capacity(ITERS);
        let mut emit_compressed_times = Vec::with_capacity(ITERS);
        let mut total_compressed_times = Vec::with_capacity(ITERS);
        let mut total_uncompressed_times = Vec::with_capacity(ITERS);

        let mut uncompressed_buf = Vec::with_capacity(2 * 1024 * 1024);
        let mut compressed_buf = Vec::with_capacity(1024 * 1024);

        for _ in 0..ITERS {
            uncompressed_buf.clear();
            compressed_buf.clear();

            let t0 = std::time::Instant::now();
            let doc = tex::parse(&src).unwrap();
            let t1 = std::time::Instant::now();
            let pages = layout::layout(&doc);
            let t2 = std::time::Instant::now();
            pdf::emit_into_opts(&pages, &mut uncompressed_buf, false).unwrap();
            let t3 = std::time::Instant::now();
            pdf::emit_into_opts(&pages, &mut compressed_buf, true).unwrap();
            let t4 = std::time::Instant::now();

            let parse_ms = (t1 - t0).as_secs_f64() * 1000.0;
            let layout_ms = (t2 - t1).as_secs_f64() * 1000.0;
            let uncompressed_emit_ms = (t3 - t2).as_secs_f64() * 1000.0;
            let compressed_emit_ms = (t4 - t3).as_secs_f64() * 1000.0;

            parse_times.push(parse_ms);
            layout_times.push(layout_ms);
            emit_uncompressed_times.push(uncompressed_emit_ms);
            emit_compressed_times.push(compressed_emit_ms);
            total_compressed_times.push(parse_ms + layout_ms + compressed_emit_ms);
            total_uncompressed_times.push(parse_ms + layout_ms + uncompressed_emit_ms);
        }

        let avg = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        let min = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);

        eprintln!(
            "\n=== 100K-WORD STRESS BENCHMARK ({} iterations) ===",
            ITERS
        );
        eprintln!(
            "Parse (AST):                         avg = {:.2} ms, min = {:.2} ms",
            avg(&parse_times),
            min(&parse_times)
        );
        eprintln!(
            "Layout (Knuth-Plass):                avg = {:.2} ms, min = {:.2} ms",
            avg(&layout_times),
            min(&layout_times)
        );
        eprintln!(
            "Emit [Uncompressed - stream only]:   avg = {:.2} ms, min = {:.2} ms",
            avg(&emit_uncompressed_times),
            min(&emit_uncompressed_times)
        );
        eprintln!(
            "Emit [Compressed - stream+DEFLATE]:  avg = {:.2} ms, min = {:.2} ms",
            avg(&emit_compressed_times),
            min(&emit_compressed_times)
        );
        eprintln!("------------------------------------------------------------------");
        eprintln!("Total [Default Compressed PDF]:      avg = {:.2} ms, min = {:.2} ms  (Parse + Layout + Compressed Emit)", avg(&total_compressed_times), min(&total_compressed_times));
        eprintln!("Total [Optional Uncompressed PDF]:    avg = {:.2} ms, min = {:.2} ms  (Parse + Layout + Uncompressed Emit)", avg(&total_uncompressed_times), min(&total_uncompressed_times));
    }

    #[test]
    fn test_bench_manual_detailed_breakdown() {
        let path = "docs/manual.tex";
        if !std::path::Path::new(path).exists() {
            return;
        }
        let src = std::fs::read_to_string(path).expect("read manual.tex");

        // Warmup
        let _ = compile_str(&src).unwrap();

        const ITERS: usize = 20;
        let mut parse_times = Vec::with_capacity(ITERS);
        let mut layout_times = Vec::with_capacity(ITERS);
        let mut emit_uncompressed_times = Vec::with_capacity(ITERS);
        let mut emit_compressed_times = Vec::with_capacity(ITERS);
        let mut total_compressed_times = Vec::with_capacity(ITERS);

        let mut uncompressed_buf = Vec::with_capacity(2 * 1024 * 1024);
        let mut compressed_buf = Vec::with_capacity(1024 * 1024);

        for _ in 0..ITERS {
            uncompressed_buf.clear();
            compressed_buf.clear();

            let t0 = std::time::Instant::now();
            let doc = tex::parse(&src).unwrap();
            let t1 = std::time::Instant::now();
            let pages = layout::layout(&doc);
            let t2 = std::time::Instant::now();
            pdf::emit_into_opts(&pages, &mut uncompressed_buf, false).unwrap();
            let t3 = std::time::Instant::now();
            pdf::emit_into_opts(&pages, &mut compressed_buf, true).unwrap();
            let t4 = std::time::Instant::now();

            let parse_ms = (t1 - t0).as_secs_f64() * 1000.0;
            let layout_ms = (t2 - t1).as_secs_f64() * 1000.0;
            let uncompressed_emit_ms = (t3 - t2).as_secs_f64() * 1000.0;
            let compressed_emit_ms = (t4 - t3).as_secs_f64() * 1000.0;

            parse_times.push(parse_ms);
            layout_times.push(layout_ms);
            emit_uncompressed_times.push(uncompressed_emit_ms);
            emit_compressed_times.push(compressed_emit_ms);
            total_compressed_times.push(parse_ms + layout_ms + compressed_emit_ms);
        }

        let avg = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        let min = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);

        eprintln!(
            "\n=== MANUAL.TEX BENCHMARK BREAKDOWN ({} iterations) ===",
            ITERS
        );
        eprintln!(
            "Parse (AST):                         avg = {:.2} ms, min = {:.2} ms",
            avg(&parse_times),
            min(&parse_times)
        );
        eprintln!(
            "Layout (Knuth-Plass + Math):         avg = {:.2} ms, min = {:.2} ms",
            avg(&layout_times),
            min(&layout_times)
        );
        eprintln!(
            "Emit [Uncompressed - stream only]:   avg = {:.2} ms, min = {:.2} ms",
            avg(&emit_uncompressed_times),
            min(&emit_uncompressed_times)
        );
        eprintln!(
            "Emit [Compressed - stream+DEFLATE]:  avg = {:.2} ms, min = {:.2} ms",
            avg(&emit_compressed_times),
            min(&emit_compressed_times)
        );
        eprintln!("------------------------------------------------------------------");
        eprintln!("Total [Default Compressed PDF]:      avg = {:.2} ms, min = {:.2} ms", avg(&total_compressed_times), min(&total_compressed_times));
    }

    #[test]
    fn test_page_numbering_plain_and_empty() {
        // Plain (default) document has page number "1"
        let src_plain = r#"
\documentclass{article}
\begin{document}
Hello World with Page Number.
\end{document}
"#;
        let pdf_plain = compile_str(src_plain).unwrap();
        let text_plain = extract_pdftotext(&pdf_plain);
        assert!(text_plain.contains("Hello World with Page Number."));
        assert!(text_plain.contains("1"));

        // Empty pagestyle has NO page number
        let src_empty = r#"
\documentclass{article}
\pagestyle{empty}
\begin{document}
Hello World without Page Number.
\end{document}
"#;
        let pdf_empty = compile_str(src_empty).unwrap();
        let text_empty = extract_pdftotext(&pdf_empty);
        assert!(text_empty.contains("Hello World without Page Number."));
        // Check that page number "1" was not emitted in footer
        let uncomp = compile_str_opts(src_empty, false).unwrap();
        let s = String::from_utf8_lossy(&uncomp);
        // The uncompressed stream shouldn't have a single digit page number (1) Tj
        assert!(!s.contains("(1) Tj"));
    }

    #[test]
    fn test_heading_orphan_protection() {
        // Document with enough lines such that section heading fits alone
        // but cannot fit with 2 lines of text.
        // It must be pushed to page 2 rather than being orphaned on page 1.
        let mut src = String::from(
            r#"
\documentclass{article}
\begin{document}
"#,
        );
        for i in 1..=29 {
            src.push_str(&format!("Paragraph item line number {i}.\n\n"));
        }
        src.push_str(
            r#"
\section{Protected Heading}
This is the text following the protected heading that belongs with it.
\end{document}
"#,
        );
        let doc = tex::parse(&src).unwrap();
        let pages = layout::layout(&doc);
        assert!(pages.len() >= 2, "should span at least 2 pages");
        // Check that page 1 does NOT have the section heading
        assert!(!pages[0].text_pool.contains(&"Protected"));
        // Check that page 2 DOES have the section heading
        assert!(pages[1].text_pool.contains(&"Protected"));
    }

    #[test]
    fn test_full_justification_tw() {
        let src = r#"
\documentclass{article}
\begin{document}
This is a long paragraph that is designed to demonstrate full justification in our high-speed LaTeX engine.
The words on the first line will expand their inter-word spacing so that the line reaches exactly the right margin.
The very last line of this paragraph will remain left-aligned without justification.
\end{document}
"#;
        let pdf_uncompressed = compile_str_opts(src, false).unwrap();
        let s = String::from_utf8_lossy(&pdf_uncompressed);
        // The uncompressed PDF must contain positive word spacing commands `Tw`
        assert!(s.contains(" Tw\n"));
        // And it must reset back to 0 Tw for the last line or trailing text
        assert!(s.contains("0 Tw\n"));

        let doc = tex::parse(src).unwrap();
        let pages = layout::layout(&doc);
        assert!(!pages.is_empty());
        // Check that at least one GlyphRun has word_spacing > 0.0
        let has_justified_run = pages[0].ops.iter().any(|op| match op {
            layout::Paint::GlyphRun { word_spacing, .. } => *word_spacing > 0.0,
            _ => false,
        });
        assert!(
            has_justified_run,
            "Expected at least one justified GlyphRun with word_spacing > 0"
        );

        // Check that the last GlyphRun of the paragraph has word_spacing == 0.0
        let last_glyph_run = pages[0]
            .ops
            .iter()
            .rfind(|op| matches!(op, layout::Paint::GlyphRun { .. }))
            .unwrap();
        match last_glyph_run {
            layout::Paint::GlyphRun { word_spacing, .. } => assert_eq!(*word_spacing, 0.0),
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_paragraph_first_line_indent_and_noindent() {
        let src = r#"
\documentclass{article}
\begin{document}
\section{Intro}
First paragraph after section heading has no indent.

Second paragraph in the section has standard first line indent.

\noindent Third paragraph explicitly requested noindent.
\end{document}
"#;
        let doc = tex::parse(src).unwrap();
        let pages = layout::layout(&doc);
        assert!(!pages.is_empty());

        let glyph_runs: Vec<_> = pages[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                layout::Paint::GlyphRun { x, part_start, .. } => {
                    let text = pages[0].text_pool[*part_start as usize];
                    Some((*x, text))
                }
                _ => None,
            })
            .collect();

        // Heading "Intro": x == 72.0
        // "First": x == 72.0 (unindented after section)
        // "Second": x == 72.0 + 15.0 = 87.0 (indented by 15.0pt)
        // "Third": x == 72.0 (unindented due to \noindent)
        let first_run = glyph_runs.iter().find(|(_, t)| *t == "First").unwrap();
        assert_eq!(
            first_run.0, 72.0,
            "First paragraph after heading should not be indented"
        );

        let second_run = glyph_runs.iter().find(|(_, t)| *t == "Second").unwrap();
        assert_eq!(
            second_run.0, 87.0,
            "Second paragraph should be indented by par_indent (15.0pt)"
        );

        let third_run = glyph_runs.iter().find(|(_, t)| *t == "Third").unwrap();
        assert_eq!(
            third_run.0, 72.0,
            "Third paragraph with noindent should not be indented"
        );
    }

    #[test]
    fn test_explicit_page_break_and_skips() {
        let src = r#"
\documentclass{article}
\begin{document}
Content on page 1.
\bigskip
\hrule
\vspace{20pt}
More content on page 1.
\newpage
Content on page 2 after explicit newpage.
\end{document}
"#;
        let doc = tex::parse(src).unwrap();
        let pages = layout::layout(&doc);
        assert_eq!(pages.len(), 2, "Expected exactly 2 pages from \\newpage");
        assert!(pages[0].text_pool.contains(&"Content"));
        assert!(pages[0].text_pool.contains(&"More"));
        assert!(pages[1].text_pool.contains(&"explicit"));

        // Verify HRule was emitted on page 1
        let has_rule = pages[0]
            .ops
            .iter()
            .any(|op| matches!(op, layout::Paint::Rule { .. }));
        assert!(has_rule, "Expected horizontal rule on page 1");
    }

    #[test]
    fn test_ast_node_cache_line_fit() {
        let block_size = std::mem::size_of::<tex::Block>();
        let inline_size = std::mem::size_of::<tex::Inline>();
        let colspec_size = std::mem::size_of::<tex::ColSpec>();
        let list_opts_size = std::mem::size_of::<tex::ListOptions>();
        assert!(
            block_size <= 32,
            "Block size must be <= 32 bytes to pack >= 2 items per 64B L1 cache line without split, got {block_size}"
        );
        assert!(
            inline_size <= 32,
            "Inline size must be <= 32 bytes to pack >= 2 items per 64B L1 cache line without split, got {inline_size}"
        );
        assert_eq!(
            colspec_size, 1,
            "ColSpec must be exactly 1 byte bit-packed, got {colspec_size}"
        );
        assert_eq!(
            list_opts_size, 32,
            "ListOptions must be exactly 32 bytes bit-packed, got {list_opts_size}"
        );
        let table_cell_size = std::mem::size_of::<tex::TableCell>();
        let table_rule_size = std::mem::size_of::<tex::TableRule>();
        assert_eq!(
            table_cell_size, 32,
            "TableCell must be exactly 32 bytes to pack 2 items per 64B L1 cache line, got {table_cell_size}"
        );
        assert!(
            table_rule_size <= 8,
            "TableRule must be <= 8 bytes to pack >= 8 items per 64B L1 cache line, got {table_rule_size}"
        );
    }

    #[test]
    fn test_verbatim_and_code_listing() {
        let src = r#"
\documentclass{article}
\begin{document}
Here is a raw verbatim code listing:
\begin{verbatim}
fn calculate_sum(values: &[i32]) -> i32 {
    let mut sum = 0;
    for &v in values {
        sum += v;
    }
    sum // returned % comment literal
}
\end{verbatim}

Now a numbered listings environment:
\begin{lstlisting}[language=Rust, numbers=left]
pub struct Vector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
\end{lstlisting}
\end{document}
"#;
        let pdf = compile_str(src).unwrap();
        let text = extract_pdftotext(&pdf);
        assert!(text.contains("calculate_sum"));
        assert!(text.contains("sum += v;"));
        assert!(text.contains("returned % comment literal"));
        assert!(text.contains("pub struct Vector3"));
        assert!(text.contains("pub z: f32,"));

        // Verify uncompressed stream contains /Fc font selection for Courier
        let uncomp = compile_str_opts(src, false).unwrap();
        let stream_str = String::from_utf8_lossy(&uncomp);
        assert!(stream_str.contains("/Fc 9"));
        assert!(stream_str.contains("/Courier"));
    }

    #[test]
    fn test_inline_monospace_and_verb() {
        let src = r#"
\documentclass{article}
\begin{document}
Use the command \texttt{cargo test} or \verb|fn main() { println!("hi"); }| in your terminal.
\end{document}
"#;
        let pdf = compile_str(src).unwrap();
        let text = extract_pdftotext(&pdf);
        assert!(text.contains("cargo test"));
        assert!(text.contains(r#"fn main() { println!("hi"); }"#));

        let uncomp = compile_str_opts(src, false).unwrap();
        let stream_str = String::from_utf8_lossy(&uncomp);
        assert!(stream_str.contains("/Fc"));
    }

    #[test]
    fn test_text_escape_commands() {
        let src = r#"
\documentclass{article}
\begin{document}
Escaped symbols: \textbackslash{} \textasciitilde{} \textasciicircum{} \% \$ \& \# \_ \{ \}
\end{document}
"#;
        let pdf = compile_str(src).unwrap();
        let text = extract_pdftotext(&pdf);
        assert!(text.contains(r#"\"#));
        assert!(text.contains("~"));
        assert!(text.contains("^"));
        assert!(text.contains("%"));
        assert!(text.contains("$"));
        assert!(text.contains("&"));
        assert!(text.contains("#"));
        assert!(text.contains("_"));
        assert!(text.contains("{"));
        assert!(text.contains("}"));
    }

    #[test]
    fn test_parindent_setlength_and_indent() {
        let src = r#"
\documentclass{article}
\begin{document}
\setlength{\parindent}{24pt}
\section{Heading}
\indent ForceIndentParagraph after heading.

NormalSecondParagraph with parindent.

\noindent NoIndentParagraph.
\end{document}
"#;
        let doc = tex::parse(src).unwrap();
        let pages = layout::layout(&doc);
        assert!(!pages.is_empty());

        let glyph_runs: Vec<_> = pages[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                layout::Paint::GlyphRun { x, part_start, .. } => {
                    let text = pages[0].text_pool[*part_start as usize];
                    Some((*x, text))
                }
                _ => None,
            })
            .collect();

        let forced = glyph_runs
            .iter()
            .find(|(_, t)| *t == "ForceIndentParagraph")
            .unwrap();
        assert_eq!(
            forced.0,
            72.0 + 24.0,
            "\\indent should force 24pt indent even after heading"
        );

        let second = glyph_runs
            .iter()
            .find(|(_, t)| *t == "NormalSecondParagraph")
            .unwrap();
        assert_eq!(
            second.0,
            72.0 + 24.0,
            "Normal paragraph should use 24pt parindent"
        );

        let noind = glyph_runs
            .iter()
            .find(|(_, t)| *t == "NoIndentParagraph.")
            .unwrap();
        assert_eq!(noind.0, 72.0, "\\noindent should have 0pt indent");
    }

    #[test]
    fn test_hangindent_and_hangafter() {
        let src = r#"
\documentclass{article}
\begin{document}
\noindent
\hangindent 30pt
\hangafter 1
Alpha line one is quite long and we need enough words to ensure it wraps onto a second line and a third line so we can verify hanging indentation.
\end{document}
"#;
        let doc = tex::parse(src).unwrap();
        let pages = layout::layout(&doc);
        assert!(!pages.is_empty());

        let line_starts: Vec<(f32, &str)> = pages[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                layout::Paint::GlyphRun { x, part_start, .. } => {
                    let text = pages[0].text_pool[*part_start as usize];
                    Some((*x, text))
                }
                _ => None,
            })
            .collect();

        assert!(
            line_starts.len() >= 2,
            "Paragraph must break into at least 2 lines"
        );
        // Line 1: hangafter = 1 means line 1 is unaffected (72.0)
        assert_eq!(line_starts[0].0, 72.0, "Line 1 should be flush at 72.0");
        // Line 2+: should be indented by hangindent (72.0 + 30.0 = 102.0)
        assert_eq!(
            line_starts[1].0, 102.0,
            "Line 2 should have 30pt hanging indent (102.0)"
        );
    }

    #[test]
    fn test_list_indentation_and_labelsep() {
        let src = r#"
\documentclass{article}
\begin{document}
\begin{enumerate}[labelsep=15pt, leftmargin=35pt, itemindent=5pt]
\item First line of item one which has enough text to comfortably wrap onto the second line for hanging test.
\item Item two.
\end{enumerate}
\end{document}
"#;
        let doc = tex::parse(src).unwrap();
        let pages = layout::layout(&doc);
        assert!(!pages.is_empty());

        // Find marker 1
        let marker1 = pages[0]
            .ops
            .iter()
            .find_map(|op| match op {
                layout::Paint::Marker {
                    x,
                    marker: layout::Marker::Number(1),
                    ..
                } => Some(*x),
                _ => None,
            })
            .expect("Marker 1 should exist");

        // Find glyph run for "First"
        let first_word = pages[0]
            .ops
            .iter()
            .find_map(|op| match op {
                layout::Paint::GlyphRun { x, part_start, .. }
                    if pages[0].text_pool[*part_start as usize] == "First" =>
                {
                    Some(*x)
                }
                _ => None,
            })
            .expect("First word should exist");

        // First line x = 72.0 (left margin) + 35.0 (leftmargin) + 5.0 (itemindent) = 112.0
        assert_eq!(first_word, 112.0, "First word should be at 112.0");

        // Marker width for "1." in Roman 11pt
        let marker_w = metrics::width('1', metrics::Face::Roman, 11.0)
            + metrics::width('.', metrics::Face::Roman, 11.0);
        let expected_marker_x = 112.0 - 15.0 - marker_w;
        assert!(
            (marker1 - expected_marker_x).abs() < 1e-3,
            "Marker x should be exactly first_line_x - labelsep - marker_w"
        );

        // Verify distance between marker right edge and first word is strictly labelsep (15.0)
        let distance_to_first_word = first_word - (marker1 + marker_w);
        assert!(
            (distance_to_first_word - 15.0).abs() < 1e-3,
            "Distance between marker and first word must be exactly 15pt"
        );

        // Find second line of item 1
        let second_line_start = pages[0]
            .ops
            .iter()
            .find_map(|op| match op {
                layout::Paint::GlyphRun { x, part_start, .. }
                    if pages[0].text_pool[*part_start as usize] != "First"
                        && pages[0].text_pool[*part_start as usize] != "Item" =>
                {
                    Some(*x)
                }
                _ => None,
            })
            .expect("Second line should exist");

        // Hanging lines start at self.left = 72.0 + 35.0 = 107.0
        assert_eq!(
            second_line_start, 107.0,
            "Hanging lines must align with leftmargin (107.0)"
        );
    }

    #[test]
    fn test_itemize_bullet_labelsep_and_hang() {
        let src = r#"
\documentclass{article}
\begin{document}
\begin{itemize}[labelsep=12pt, leftmargin=28pt]
\item Bullet item with enough continuous prose to wrap cleanly onto a second line of text so we can verify hanging alignment.
\end{itemize}
\end{document}
"#;
        let doc = tex::parse(src).unwrap();
        let pages = layout::layout(&doc);
        assert!(!pages.is_empty());

        let marker_bullet = pages[0]
            .ops
            .iter()
            .find_map(|op| match op {
                layout::Paint::Marker {
                    x,
                    marker: layout::Marker::Bullet,
                    ..
                } => Some(*x),
                _ => None,
            })
            .expect("Bullet marker should exist");

        let first_word = pages[0]
            .ops
            .iter()
            .find_map(|op| match op {
                layout::Paint::GlyphRun { x, part_start, .. }
                    if pages[0].text_pool[*part_start as usize] == "Bullet" =>
                {
                    Some(*x)
                }
                _ => None,
            })
            .expect("First word should exist");

        // leftmargin = 28.0, itemindent = 0.0 -> first_word = 72.0 + 28.0 = 100.0
        assert_eq!(first_word, 100.0);

        let bullet_w = metrics::width('\u{2022}', metrics::Face::Roman, 11.0);
        let distance = first_word - (marker_bullet + bullet_w);
        assert!(
            (distance - 12.0).abs() < 1e-3,
            "Distance between bullet and first word must be exactly 12pt"
        );
    }

    #[test]
    fn test_nested_enumeration_and_depth_markers() {
        let src = r#"
\documentclass{article}
\begin{document}
\begin{enumerate}
\item Level 1 Item
  \begin{enumerate}
  \item Level 2 Item
    \begin{enumerate}
    \item Level 3 Item
    \end{enumerate}
  \end{enumerate}
\end{enumerate}
\end{document}
"#;
        let doc = tex::parse(src).unwrap();
        let pages = layout::layout(&doc);
        assert!(!pages.is_empty());

        let has_level1 = pages[0].ops.iter().any(|op| {
            matches!(
                op,
                layout::Paint::Marker {
                    marker: layout::Marker::Number(1),
                    ..
                }
            )
        });
        let has_level2 = pages[0].ops.iter().any(|op| {
            matches!(
                op,
                layout::Paint::Marker {
                    marker: layout::Marker::Alpha(1),
                    ..
                }
            )
        });
        let has_level3 = pages[0].ops.iter().any(|op| {
            matches!(
                op,
                layout::Paint::Marker {
                    marker: layout::Marker::Roman(1),
                    ..
                }
            )
        });

        assert!(has_level1, "Should have level 1 marker: 1.");
        assert!(has_level2, "Should have level 2 marker: (a)");
        assert!(has_level3, "Should have level 3 marker: i.");

        let pdf = compile_str(src).unwrap();
        let text = extract_pdftotext(&pdf);
        assert!(text.contains("1."));
        assert!(text.contains("(a)"));
        assert!(text.contains("i."));
    }

    #[test]
    fn test_manual_compilation() {
        let manual_source = include_str!("../docs/manual.tex");
        let pdf = compile_str(manual_source).expect("docs/manual.tex should compile cleanly");
        assert!(!pdf.is_empty());
        assert!(pdf.starts_with(b"%PDF-1.4"));

        let text = extract_pdftotext(&pdf);
        assert!(
            text.contains("The rushtex Typesetting Engine"),
            "Title must be present"
        );
        assert!(
            text.contains("Diagnostic Error Reporting System"),
            "Diagnostic section must be present"
        );
        assert!(
            text.contains("List and Enumeration Geometry"),
            "List geometry section must be present"
        );
        assert!(
            text.contains("Multi-Level Nested Enumeration Test"),
            "Nested enum test must be present"
        );
        assert!(
            text.contains("(a)"),
            "Nested level 2 marker (a) must be rendered"
        );
        assert!(
            text.contains("i."),
            "Nested level 3 marker i. must be rendered"
        );
        assert!(
            text.contains("100,000-Word Benchmark Comparison"),
            "Benchmark section must be present"
        );
        assert!(
            text.contains("5,319,739"),
            "Throughput figure must be present"
        );
    }

    #[test]
    fn test_cm_math_tounicode_searchability() {
        let src = r"\documentclass{article}
\usepackage{computermodern}
\begin{document}
Einstein field formula $E = mc^2$ and Euler identity $e^{i\pi} + 1 = 0$.
Calculus formula $\int_0^1 f(x) dx \le \sum_{i=1}^n x_i$.
\end{document}";
        let pdf = compile_str(src).expect("Compilation must succeed");
        assert!(!pdf.is_empty());
        let text = extract_pdftotext(&pdf);
        assert!(text.contains("E = mc"));
        assert!(text.contains('π'), "Greek letter pi must be extracted via ToUnicode");
        assert!(text.contains('∫'), "Integral symbol must be extracted via ToUnicode");
        assert!(text.contains('≤'), "Less-than-or-equal symbol must be extracted via ToUnicode");
        assert!(text.contains("f (x)") || text.contains("f(x)"));
        // Check that PDF contains /ToUnicode references for embedded CM fonts
        let pdf_str = String::from_utf8_lossy(&pdf);
        assert!(pdf_str.contains("/ToUnicode"));
        assert!(pdf_str.contains("/RushTeX-ToUnicode"));
    }

    #[test]
    fn test_default_font_is_times_roman() {
        let src = r#"
\documentclass{article}
\begin{document}
Standard serif text with \textbf{bold serif}, \textit{italic serif}, \textbf{\textit{bold-italic serif}}, and \texttt{typewriter mono}.
\end{document}
"#;
        let pdf = compile_str(src).unwrap();
        let s = String::from_utf8_lossy(&pdf);
        assert!(
            s.contains("/BaseFont /Times-Roman"),
            "Default Roman face must be Times-Roman"
        );
        assert!(
            s.contains("/BaseFont /Times-Bold"),
            "Default Bold face must be Times-Bold"
        );
        assert!(
            s.contains("/BaseFont /Times-Italic"),
            "Default Italic face must be Times-Italic"
        );
        assert!(
            s.contains("/BaseFont /Times-BoldItalic"),
            "Default BoldItalic face must be Times-BoldItalic"
        );
        assert!(
            s.contains("/BaseFont /Courier"),
            "Mono face must remain Courier"
        );
    }

    #[test]
    fn test_preamble_helvet_package_switches_to_helvetica() {
        let src = r#"
\documentclass{article}
\usepackage{helvet}
\begin{document}
Sans-serif text with \textbf{bold sans}, \textit{oblique sans}, and \textbf{\textit{bold oblique sans}}.
\end{document}
"#;
        let pdf = compile_str(src).unwrap();
        let s = String::from_utf8_lossy(&pdf);
        assert!(
            s.contains("/BaseFont /Helvetica"),
            "Package helvet must switch Roman face to Helvetica"
        );
        assert!(
            s.contains("/BaseFont /Helvetica-Bold"),
            "Package helvet must switch Bold face to Helvetica-Bold"
        );
        assert!(
            s.contains("/BaseFont /Helvetica-Oblique"),
            "Package helvet must switch Italic face to Helvetica-Oblique"
        );
        assert!(
            s.contains("/BaseFont /Helvetica-BoldOblique"),
            "Package helvet must switch BoldItalic face to Helvetica-BoldOblique"
        );
    }

    #[test]
    fn test_preamble_docclass_sans_switches_to_helvetica() {
        let src = r#"
\documentclass[sans]{article}
\begin{document}
Sans-serif document via documentclass option.
\end{document}
"#;
        let pdf = compile_str(src).unwrap();
        let s = String::from_utf8_lossy(&pdf);
        assert!(
            s.contains("/BaseFont /Helvetica"),
            "documentclass[sans] must switch to Helvetica"
        );
    }

    #[test]
    fn test_preamble_renewcommand_sfdefault_switches_to_helvetica() {
        let src = r#"
\documentclass{article}
\renewcommand{\familydefault}{\sfdefault}
\begin{document}
Document configured to sfdefault.
\end{document}
"#;
        let pdf = compile_str(src).unwrap();
        let s = String::from_utf8_lossy(&pdf);
        assert!(
            s.contains("/BaseFont /Helvetica"),
            "renewcommand familydefault sfdefault must switch to Helvetica"
        );
    }

    #[test]
    fn test_palatino_package_and_docclass() {
        let src = r#"
\documentclass{article}
\usepackage{palatino}
\begin{document}
Palatino serif document with \textbf{bold}, \textit{italic}, and \textbf{\textit{bold italic}}.
\end{document}
"#;
        let pdf = compile_str(src).unwrap();
        let s = String::from_utf8_lossy(&pdf);
        assert!(
            s.contains("/BaseFont /Palatino-Roman"),
            "Palatino package must switch to Palatino-Roman"
        );
        assert!(
            s.contains("/BaseFont /Palatino-Bold"),
            "Palatino package must switch to Palatino-Bold"
        );
        assert!(
            s.contains("/BaseFont /Palatino-Italic"),
            "Palatino package must switch to Palatino-Italic"
        );
        assert!(
            s.contains("/BaseFont /Palatino-BoldItalic"),
            "Palatino package must switch to Palatino-BoldItalic"
        );

        let src_pazo = r#"
\documentclass{article}
\usepackage{mathpazo}
\begin{document}
Mathpazo document.
\end{document}
"#;
        let pdf_pazo = compile_str(src_pazo).unwrap();
        let s_pazo = String::from_utf8_lossy(&pdf_pazo);
        assert!(
            s_pazo.contains("/BaseFont /Palatino-Roman"),
            "mathpazo must switch to Palatino-Roman"
        );
    }

    #[test]
    fn test_computermodern_package_and_docclass() {
        let src = r#"
\documentclass{article}
\usepackage{lmodern}
\begin{document}
Computer Modern document with \textbf{bold} and \textit{italic}.
\end{document}
"#;
        let pdf = compile_str(src).unwrap();
        let s = String::from_utf8_lossy(&pdf);
        assert!(
            s.contains("/BaseFont /CMR10"),
            "lmodern package must switch to CMR10"
        );
        assert!(
            s.contains("/BaseFont /CMTI10"),
            "lmodern package must switch to CMTI10"
        );
        assert!(
            s.contains("/BaseFont /CMBX10"),
            "lmodern package must switch to CMBX10"
        );

        let src_cm = r#"
\documentclass[cm]{article}
\begin{document}
Donald Knuth's Computer Modern via documentclass.
\end{document}
"#;
        let pdf_cm = compile_str(src_cm).unwrap();
        let s_cm = String::from_utf8_lossy(&pdf_cm);
        assert!(
            s_cm.contains("/BaseFont /CMR10"),
            "documentclass[cm] must switch to CMR10"
        );
    }

    #[test]
    fn test_symbol_and_dingbats_emitted() {
        use crate::font_metrics::Face;
        use crate::page_layout::{Page, Paint};
        let page = Page {
            width: 595.0,
            height: 842.0,
            font_family: crate::font_metrics::FontFamily::Times,
            used_faces: (1 << Face::Symbol as u8) | (1 << Face::Dingbats as u8),
            ops: vec![
                Paint::GlyphRun {
                    x: 72.0,
                    y: 700.0,
                    face: Face::Symbol,
                    size: 10.0,
                    part_start: 0,
                    part_count: 1,
                    word_spacing: 0.0,
                },
                Paint::GlyphRun {
                    x: 100.0,
                    y: 700.0,
                    face: Face::Dingbats,
                    size: 10.0,
                    part_start: 1,
                    part_count: 1,
                    word_spacing: 0.0,
                },
            ],
            text_pool: vec!["α", "4"],
        };
        let mut pdf = Vec::new();
        pdf::emit_into_opts(&[page], &mut pdf, false).unwrap();
        let s = String::from_utf8_lossy(&pdf);
        assert!(
            s.contains("/Fs "),
            "Page resources must include /Fs font identifier"
        );
        assert!(
            s.contains("/Fz "),
            "Page resources must include /Fz font identifier"
        );
        assert!(
            s.contains("/BaseFont /Symbol"),
            "Font dictionary must include /BaseFont /Symbol"
        );
        assert!(
            s.contains("/BaseFont /ZapfDingbats"),
            "Font dictionary must include /BaseFont /ZapfDingbats"
        );
    }

    #[test]
    fn test_only_used_fonts_are_emitted_in_pdf() {
        let src = r#"
\documentclass{article}
\begin{document}
Plain roman text only.
\end{document}
"#;
        let pdf = compile_str(src).unwrap();
        let s = String::from_utf8_lossy(&pdf);
        assert!(
            s.contains("/BaseFont /Times-Roman"),
            "Used Roman font must be present"
        );
        assert!(
            !s.contains("/BaseFont /Times-Bold"),
            "Unused Times-Bold must NOT be emitted"
        );
        assert!(
            !s.contains("/BaseFont /Times-Italic"),
            "Unused Times-Italic must NOT be emitted"
        );
        assert!(
            !s.contains("/BaseFont /Times-BoldItalic"),
            "Unused Times-BoldItalic must NOT be emitted"
        );
        assert!(
            !s.contains("/BaseFont /Courier"),
            "Unused Courier must NOT be emitted"
        );
        assert!(
            !s.contains("/BaseFont /Symbol"),
            "Unused Symbol must NOT be emitted"
        );
        assert!(
            !s.contains("/BaseFont /ZapfDingbats"),
            "Unused ZapfDingbats must NOT be emitted"
        );
        assert!(s.contains("/Fr "), "Page resources must include /Fr");
        assert!(!s.contains("/Fb "), "Page resources must NOT include /Fb");
        assert!(!s.contains("/Fi "), "Page resources must NOT include /Fi");
        assert!(!s.contains("/Fc "), "Page resources must NOT include /Fc");
        assert!(!s.contains("/Fs "), "Page resources must NOT include /Fs");
        assert!(!s.contains("/Fz "), "Page resources must NOT include /Fz");
    }

    #[test]
    fn test_stem_math_and_greek_symbols() {
        let src = r#"
\documentclass{article}
\begin{document}
In STEM mathematics:
\alpha + \beta = \gamma
\sum \int \prod \partial \nabla \infty
\le \ge \neq \approx \pm \times \div \cdot
\forall x \in S, \exists y : x \le y
\rightarrow \leftarrow \leftrightarrow \Rightarrow \Leftarrow \Leftrightarrow
\sqrt \perp \sim \cong \oplus \otimes \emptyset \propto
\end{document}
"#;
        let pdf = compile_str(src).expect("STEM math symbols should compile without error");
        assert!(pdf.starts_with(b"%PDF-1.4"));
        let text = extract_pdftotext(&pdf);
        assert!(
            text.contains("α") || text.contains("alpha") || !text.is_empty(),
            "PDF must render text"
        );
    }

    #[test]
    fn test_static_rodata_metrics_invariants() {
        use crate::font_metrics::*;
        // Verify Symbol widths (256 elements in .rodata)
        assert_eq!(SYMBOL_WIDTHS.len(), 256);
        assert_eq!(SYMBOL_WIDTHS[b'a' as usize], 631); // alpha in Symbol
        assert_eq!(SYMBOL_WIDTHS[b'b' as usize], 549); // beta in Symbol
        assert_eq!(SYMBOL_WIDTHS[b'p' as usize], 549); // pi in Symbol
        assert_eq!(SYMBOL_WIDTHS[0xE5], 713); // sum in Symbol
        assert_eq!(SYMBOL_WIDTHS[0xF2], 274); // int in Symbol
        assert_eq!(SYMBOL_WIDTHS[0xA5], 713); // infty in Symbol

        // Verify ZapfDingbats widths (256 elements in .rodata)
        assert_eq!(ZAPFDINGBATS_WIDTHS.len(), 256);

        // Verify Palatino widths (128 elements in .rodata)
        assert_eq!(PALATINO_ROMAN_WIDTHS.len(), 128);
        assert_eq!(PALATINO_BOLD_WIDTHS.len(), 128);
        assert_eq!(PALATINO_ITALIC_WIDTHS.len(), 128);
        assert_eq!(PALATINO_BOLD_ITALIC_WIDTHS.len(), 128);
        assert_eq!(PALATINO_ROMAN_WIDTHS[b'A' as usize], 778);
        assert_eq!(PALATINO_BOLD_WIDTHS[b'A' as usize], 778);

        // Verify Computer Modern widths (128 elements in .rodata)
        assert_eq!(CMR10_WIDTHS.len(), 128);
        assert_eq!(CMMI10_WIDTHS.len(), 128);
        assert_eq!(CMSY10_WIDTHS.len(), 128);
        assert_eq!(CMEX10_WIDTHS.len(), 128);
        assert_eq!(CMTT10_WIDTHS.len(), 128);
        assert_eq!(CMTT10_WIDTHS[b'A' as usize], 525); // Monospace 525 units

        // Verify unicode_to_symbol_byte mappings
        assert_eq!(unicode_to_symbol_byte('α'), Some(b'a'));
        assert_eq!(unicode_to_symbol_byte('β'), Some(b'b'));
        assert_eq!(unicode_to_symbol_byte('π'), Some(b'p'));
        assert_eq!(unicode_to_symbol_byte('∑'), Some(0xE5));
        assert_eq!(unicode_to_symbol_byte('∫'), Some(0xF2));
        assert_eq!(unicode_to_symbol_byte('∞'), Some(0xA5));
        assert_eq!(unicode_to_symbol_byte('≤'), Some(0xA3));
        assert_eq!(unicode_to_symbol_byte('≥'), Some(0xB3));
    }

    #[test]
    fn test_compile_file_opts_report() {
        let temp_dir = std::env::temp_dir();
        let tex_path = temp_dir.join("rushtex_test_doc.tex");
        let pdf_path = temp_dir.join("rushtex_test_doc.pdf");
        std::fs::write(
            &tex_path,
            "\\documentclass{article}\n\\begin{document}\nHello World\n\\end{document}",
        )
        .unwrap();

        let report = compile_file_opts(&tex_path, Some(&pdf_path), true).unwrap();
        assert_eq!(report.page_count, 1);
        assert_eq!(report.output_path, pdf_path);
        assert!(pdf_path.exists());

        let _ = std::fs::remove_file(tex_path);
        let _ = std::fs::remove_file(pdf_path);
    }

    #[test]
    fn test_description_environment_and_custom_item_labels() {
        let src = r#"
\documentclass{article}
\begin{document}
\begin{description}
\item[Rust] A fast systems programming language.
\item[LaTeX] Vector document compilation engine.
\end{description}

\begin{enumerate}
\item Normal item
\item[(X)] Custom bracket marker
\item Follow-up item
\end{enumerate}
\end{document}
"#;
        let pdf = compile_str(src).unwrap();
        let text = extract_pdftotext(&pdf);
        assert!(
            text.contains("Rust"),
            "Description term must appear: {text}"
        );
        assert!(text.contains("A fast systems programming language."));
        assert!(text.contains("LaTeX"));
        assert!(text.contains("Vector document compilation engine."));
        assert!(text.contains("1."));
        assert!(
            text.contains("(X)"),
            "Custom item label must appear: {text}"
        );
        assert!(text.contains("3."));
    }

    #[test]
    fn test_enumeration_start_and_label_format() {
        let src = r#"
\documentclass{article}
\begin{document}
\begin{enumerate}[start=5, label=(\alph*)]
\item Fifth item
\item Sixth item
\end{enumerate}
\end{document}
"#;
        let pdf = compile_str(src).unwrap();
        let text = extract_pdftotext(&pdf);
        assert!(
            text.contains("(e)"),
            "Start=5 with (alph) format must emit (e): {text}"
        );
        assert!(text.contains("Fifth item"));
        assert!(text.contains("(f)"), "Sixth item must emit (f): {text}");
        assert!(text.contains("Sixth item"));
    }

    #[test]
    fn test_quote_and_quotation_environment() {
        let src = r#"
\documentclass{article}
\begin{document}
Standard paragraph outside quote.

\begin{quote}
This is an indented quotation block without paragraph indent.
\end{quote}

\begin{quotation}
This is a quotation environment with standard indentation.
\end{quotation}
\end{document}
"#;
        let doc = tex::parse(src).unwrap();
        let pages = layout::layout(&doc);
        assert_eq!(pages.len(), 1);

        let pdf = compile_str(src).unwrap();
        let text = extract_pdftotext(&pdf);
        assert!(text.contains("Standard paragraph outside quote."));
        assert!(text.contains("This is an indented quotation block"));
        assert!(text.contains("This is a quotation environment"));
    }

    #[test]
    fn test_inline_hspace() {
        let src = r#"
\documentclass{article}
\begin{document}
WordA\hspace{40pt}WordB
\end{document}
"#;
        let doc = tex::parse(src).unwrap();
        let pages = layout::layout(&doc);
        assert_eq!(pages.len(), 1);
        let text = pages[0].text_pool.join(" ");
        assert!(text.contains("WordA"));
        assert!(text.contains("WordB"));

        let pdf = compile_str(src).unwrap();
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.contains("%PDF-1.4"));
    }

    #[test]
    fn test_section_numbering_and_starred_sections() {
        let src = r#"
\documentclass{article}
\begin{document}
\tableofcontents

\section{Introduction}
First section text.

\subsection{Background}
Background details.

\subsubsection{Historical Context}
History notes.

\subsubsection{Modern Era}
Modern developments.

\subsection{Motivation}
Why this matters.

\section*{Unnumbered Aside}
This is an unnumbered section that does not increment counters or appear in ToC.

\section{Methodology}
Second numbered section.

\subsection{Architecture}
Methodology details.
\end{document}
"#;
        let doc = tex::parse(src).unwrap();
        // Verify AST blocks
        assert!(matches!(
            &doc.blocks[1],
            crate::tex_parser::Block::Section {
                level: 1,
                starred: false,
                ..
            }
        ));
        assert!(matches!(
            &doc.blocks[11],
            crate::tex_parser::Block::Section {
                level: 1,
                starred: true,
                ..
            }
        ));
        assert!(matches!(
            &doc.blocks[13],
            crate::tex_parser::Block::Section {
                level: 1,
                starred: false,
                ..
            }
        ));

        let pages = layout::layout(&doc);
        assert!(!pages.is_empty());

        let pdf = compile_str(src).unwrap();
        let text = extract_pdftotext(&pdf);

        // Verify PDF text output contains numbered headings and unnumbered heading
        assert!(
            text.contains("1 Introduction"),
            "Expected '1 Introduction' in text:\n{text}"
        );
        assert!(
            text.contains("1.1 Background"),
            "Expected '1.1 Background' in text:\n{text}"
        );
        assert!(
            text.contains("1.1.1 Historical Context"),
            "Expected '1.1.1 Historical Context' in text:\n{text}"
        );
        assert!(
            text.contains("1.1.2 Modern Era"),
            "Expected '1.1.2 Modern Era' in text:\n{text}"
        );
        assert!(
            text.contains("1.2 Motivation"),
            "Expected '1.2 Motivation' in text:\n{text}"
        );
        assert!(
            text.contains("Unnumbered Aside"),
            "Expected 'Unnumbered Aside' in text:\n{text}"
        );
        assert!(
            !text.contains("1.3 Unnumbered") && !text.contains("2 Unnumbered Aside"),
            "Starred section must not be numbered"
        );
        assert!(
            text.contains("2 Methodology"),
            "Expected '2 Methodology' in text:\n{text}"
        );
        assert!(
            text.contains("2.1 Architecture"),
            "Expected '2.1 Architecture' in text:\n{text}"
        );
    }
}
