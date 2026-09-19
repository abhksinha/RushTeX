//! # rushtex — Pure Rust TeX-to-PDF High-Speed Compilation Engine
//!
//! A zero-dependency, single-pass typesetting engine that compiles LaTeX-syntax documents
//! directly into standards-compliant PDF 1.4 documents at sub-10ms latency.
//!
//! ## Architectural Pipeline
//!
//! The compilation workflow bypasses the legacy TeX macro expansion loop, multi-pass `.aux`/`.log`
//! filesystem overhead, and DVI intermediaries, executing a pure in-memory pipeline:
//!
//! 1. **Parsing ([`tex`])**: Lexes TeX tokens and builds an Abstract Syntax Tree (`Document`)
//!    containing preamble configurations and structural blocks. Zero allocations for tokens
//!    (string slices borrow directly from source text).
//! 2. **Preamble Processing ([`preamble`])**: Handles `\documentclass`, package options (`geometry`,
//!    `graphicx`, `inputenc`), document metadata (`\title`, `\author`, `\date`), and page geometry.
//! 3. **Font Metrics ([`metrics`])**: Branchless Adobe Font Metrics lookups for the PDF Standard-14
//!    Helvetica typeface family with compile-time `.rodata` tables.
//! 4. **Layout Engine ([`layout`])**: Performs greedy line breaking, hanging indents, table column
//!    balancing, and page pagination into compact 24-byte `Paint` display lists.
//! 5. **PDF Generation ([`pdf`])**: Emits PDF 1.4 objects, page resource trees, WinAnsiEncoding font
//!    descriptors, and page content streams.
//! 6. **Compression ([`deflate`])**: Pure-Rust streaming RFC 1950 zlib / RFC 1951 DEFLATE encoder with
//!    a 32 KB hash table fitting inside CPU L1 data cache.
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

pub mod metrics;
pub mod tex;
pub mod layout;
pub mod pdf;
pub mod preamble;
pub mod deflate;
pub mod diagnostic;
pub mod listing;

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
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Parse(diag) => write!(f, "{diag}"),
            Error::Encode(e) => write!(f, "encode: {e}"),
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

/// Compiles a `.tex` file to a sibling `.pdf` (or `output_path` if given) with compression.
pub fn compile_file(input_path: &Path, output_path: Option<&Path>) -> Result<std::path::PathBuf, Error> {
    compile_file_opts(input_path, output_path, true)
}

/// Compiles a `.tex` file to a sibling `.pdf` (or `output_path` if given) with optional compression.
pub fn compile_file_opts(
    input_path: &Path,
    output_path: Option<&Path>,
    enable_compression: bool,
) -> Result<std::path::PathBuf, Error> {
    let source_code = std::fs::read_to_string(input_path)?;
    let mut pdf_buffer = Vec::with_capacity(4096);
    compile_into_opts(&source_code, &mut pdf_buffer, enable_compression)?;
    let destination = match output_path {
        Some(path) => path.to_path_buf(),
        None => input_path.with_extension("pdf"),
    };
    std::fs::write(&destination, &pdf_buffer)?;
    Ok(destination)
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
        assert!(output.status.success(), "pdftotext error: {:?}", String::from_utf8_lossy(&output.stderr));
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
        assert!(s.contains("/Helvetica-Bold"));
        assert!(s.contains("/Filter /FlateDecode"));
        assert!(s.contains("xref"));

        let text = extract_pdftotext(&pdf);
        assert!(text.contains("word"));
        assert!(text.contains("in bold"));

        let uncomp = compile_str_opts("This is a simple line and this \\textbf{word} is in bold", false).unwrap();
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
        assert_eq!(doc.preamble.class.as_ref().unwrap().options, vec!["12pt", "letterpaper"]);
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
        let src = include_str!("../examples/sample.tex");
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
                assert_eq!(diag.simplified_header(), "[ERROR] Line 54 from character 17");
                let rendered = diag.to_string();
                assert!(rendered.starts_with("[ERROR] Line 54 from character 17"));
                assert!(rendered.contains("54 |"));
                assert!(rendered.contains("^"));
            }
            other => panic!("expected parse error, got {other:?}"),
        }
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
        let mut src = String::from(r#"
\documentclass{article}
\begin{document}
"#);
        for i in 1..=29 {
            src.push_str(&format!("Paragraph item line number {i}.\n\n"));
        }
        src.push_str(r#"
\section{Protected Heading}
This is the text following the protected heading that belongs with it.
\end{document}
"#);
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
        assert!(has_justified_run, "Expected at least one justified GlyphRun with word_spacing > 0");

        // Check that the last GlyphRun of the paragraph has word_spacing == 0.0
        let last_glyph_run = pages[0]
            .ops
            .iter()
            .filter(|op| matches!(op, layout::Paint::GlyphRun { .. }))
            .last()
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
        assert_eq!(first_run.0, 72.0, "First paragraph after heading should not be indented");

        let second_run = glyph_runs.iter().find(|(_, t)| *t == "Second").unwrap();
        assert_eq!(second_run.0, 87.0, "Second paragraph should be indented by par_indent (15.0pt)");

        let third_run = glyph_runs.iter().find(|(_, t)| *t == "Third").unwrap();
        assert_eq!(third_run.0, 72.0, "Third paragraph with noindent should not be indented");
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
        let has_rule = pages[0].ops.iter().any(|op| matches!(op, layout::Paint::Rule { .. }));
        assert!(has_rule, "Expected horizontal rule on page 1");
    }

    #[test]
    fn test_ast_node_cache_line_fit() {
        let block_size = std::mem::size_of::<tex::Block>();
        let inline_size = std::mem::size_of::<tex::Inline>();
        assert!(
            block_size <= 32,
            "Block size must be <= 32 bytes to pack >= 2 items per 64B L1 cache line without split, got {block_size}"
        );
        assert!(
            inline_size <= 32,
            "Inline size must be <= 32 bytes to pack >= 2 items per 64B L1 cache line without split, got {inline_size}"
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
    fn test_manual_compilation() {
        let manual_source = include_str!("../docs/manual.tex");
        let pdf = compile_str(manual_source).expect("docs/manual.tex should compile cleanly");
        assert!(!pdf.is_empty());
        assert!(pdf.starts_with(b"%PDF-1.4"));
    }
}
