//! texrs — a from-scratch TeX-subset typesetter that emits PDF.
//!
//! No crates. Layout uses hardcoded Adobe AFM widths for the PDF
//! Standard-14 Helvetica family (no font files, no embedding).

pub mod metrics;
pub mod tex;
pub mod layout;
pub mod pdf;
pub mod preamble;

use std::path::Path;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Parse(tex::ParseError),
    Encode(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Parse(e) => write!(f, "parse: {e}"),
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

impl From<tex::ParseError> for Error {
    fn from(e: tex::ParseError) -> Self {
        Error::Parse(e)
    }
}

/// Compile a TeX-subset source string into a reusable destination byte buffer.
pub fn compile_into(src: &str, out: &mut Vec<u8>) -> Result<(), Error> {
    let doc = tex::parse(src)?;
    let pages = layout::layout(&doc);
    pdf::emit_into(&pages, out)?;
    Ok(())
}

/// Compile a TeX-subset source string to an owned PDF byte buffer.
pub fn compile_str(src: &str) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(4096);
    compile_into(src, &mut out)?;
    Ok(out)
}

/// Compile a `.tex` file to a sibling `.pdf` (or `out` if given).
pub fn compile_file(input: &Path, output: Option<&Path>) -> Result<std::path::PathBuf, Error> {
    let src = std::fs::read_to_string(input)?;
    let mut pdf = Vec::with_capacity(4096);
    compile_into(&src, &mut pdf)?;
    let out = match output {
        Some(p) => p.to_path_buf(),
        None => input.with_extension("pdf"),
    };
    std::fs::write(&out, &pdf)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(s.contains("(word"));
        assert!(s.contains("xref"));
    }

    #[test]
    fn enumerate_emits_markers() {
        let src = "\\begin{enumerate}\\item Alpha\\item Bravo\\end{enumerate}";
        let pdf = compile_str(src).unwrap();
        let s = String::from_utf8_lossy(&pdf);
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
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.contains("(A"), "{s}");
        assert!(s.contains("(B)"), "{s}");
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
        assert!(s.contains("(High Speed Engine)"));
        assert!(s.contains("(Triton)"));
        assert!(s.contains("(Overview)"));
        assert!(s.contains("(Sub-topic)"));
        assert!(!s.contains("Extra garbage"));
    }

    #[test]
    fn backward_compatibility_fragments() {
        // Plain text fragments without preamble continue to compile identically
        let src = "Just a raw paragraph without any preamble or document envelope.";
        let doc = tex::parse(src).unwrap();
        assert!(doc.preamble.class.is_none());
        assert_eq!(doc.preamble.config.page_width, 595.0); // Default A4
        let pdf = compile_str(src).unwrap();
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.contains("(Just a raw paragraph"));
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
}
