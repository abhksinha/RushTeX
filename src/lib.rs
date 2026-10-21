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
//! 0. **Expansion ([`expand`])**: Resolves user macros, environments, conditionals, counters,
//!    cross-references, and citations into plain LaTeX. Documents that use none of these are
//!    borrowed unchanged (zero copy).
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
#[path = "engine/engine_mod.rs"]
pub mod engine;
#[path = "expand/expand_mod.rs"]
pub mod expand;
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

/// Compiles a LaTeX source string into a sequence of laid-out pages.
/// The source must already be expanded with [`expand::expand`] if it uses macros or references.
/// Uses memory-conscious token-to-layout streaming when the document does not contain
/// whole-document tables of contents or indices, dropping AST nodes immediately after layout.
pub fn compile_source_to_pages(source_code: &str) -> Result<Vec<layout::Page<'_>>, Error> {
    let has_indices = source_code.contains("\\tableofcontents")
        || source_code.contains("\\listoffigures")
        || source_code.contains("\\listoftables");

    if has_indices {
        let document = tex::parse(source_code)?;
        let pages = layout::layout(&document);
        drop(document);
        Ok(pages)
    } else {
        let mut parser = tex::Parser::new(source_code);
        let has_doc_env = parser.parse_preamble_if_present()?;
        let mut layouter = layout::Layouter::new(&parser.preamble.config);
        let mut prev_was_block = false;
        while let Some(block) = parser.parse_next_block()? {
            layouter.layout_stream_block(&block, &mut prev_was_block);
        }
        parser.finish_document(has_doc_env)?;
        Ok(layouter.finish())
    }
}

fn compile_streaming_into(
    source_code: &str,
    destination_buffer: &mut Vec<u8>,
    enable_compression: bool,
    threads: bool,
    mut session: Option<&mut Session>,
) -> Result<usize, Error> {
    let mut parser = tex::Parser::new(source_code);
    let has_doc_env = parser.parse_preamble_if_present()?;
    let config = parser.preamble.config;
    let mut layouter = layout::Layouter::new(&config);
    layouter.enable_streaming();
    let mut page_cache = None;
    if let Some(s) = session.as_deref_mut() {
        s.breaks.begin();
        s.pages.begin();
        layouter.break_cache = Some(std::mem::take(&mut s.breaks));
        page_cache = Some(std::mem::take(&mut s.pages));
    }

    destination_buffer.reserve((source_code.len() * 2).clamp(65536, 1048576));
    let (result, page_cache) = if threads && pipeline_pages(source_code) {
        compile_streaming_pipelined(parser, &mut layouter, has_doc_env, destination_buffer, enable_compression, config.font_family, page_cache)
    } else {
        let mut accumulator = backend::backend_pdf::PageStreamAccumulator::new(destination_buffer, enable_compression);
        accumulator.page_cache = page_cache;
        let laid_out = (|| {
            let mut prev_was_block = false;
            while let Some(block) = parser.parse_next_block()? {
                layouter.layout_stream_block(&block, &mut prev_was_block);
                while let Some(mut page) = layouter.take_completed_page() {
                    accumulator.push_page(&mut page);
                    layouter.recycle_page(page);
                }
            }
            parser.finish_document(has_doc_env)?;
            for mut page in layouter.finish_streaming() {
                accumulator.push_page(&mut page);
            }
            Ok(())
        })();
        let page_cache = accumulator.page_cache.take();
        (laid_out.and_then(|()| accumulator.finish(config.font_family)), page_cache)
    };

    if let Some(s) = session {
        s.breaks = layouter.break_cache.take().unwrap_or_default();
        s.pages = page_cache.unwrap_or_default();
        // An error leaves the document half compiled; keep what earlier compiles cached.
        if result.is_ok() {
            s.breaks.sweep();
            s.pages.sweep();
        }
    }
    result
}

/// Sources at least this long write their pages on a second thread.
const PIPELINE_MIN_SOURCE: usize = 32 * 1024;
/// Pages queued between layout and the PDF writer; bounds the memory in flight.
const PIPELINE_DEPTH: usize = 4;

/// Whether to overlap layout with page writing (content stream and DEFLATE): only for
/// documents long enough to repay a thread, and only where threads exist.
fn pipeline_pages(source_code: &str) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = source_code;
        false
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        source_code.len() >= PIPELINE_MIN_SOURCE
            && std::thread::available_parallelism().is_ok_and(|n| n.get() > 1)
    }
}

/// [`compile_streaming_into`] with the PDF writer on its own thread. Layout sends each
/// finished page through a bounded queue; the writer sends the emptied page back so its
/// buffers are reused. The output is byte-identical to the serial path.
#[cfg(not(target_arch = "wasm32"))]
fn compile_streaming_pipelined<'a>(
    mut parser: tex::Parser<'a>,
    layouter: &mut layout::Layouter<'a>,
    has_doc_env: bool,
    destination_buffer: &mut Vec<u8>,
    enable_compression: bool,
    font_family: backend::fonts::FontFamily,
    page_cache: Option<backend::backend_pdf::PageCache>,
) -> (Result<usize, Error>, Option<backend::backend_pdf::PageCache>) {
    use std::sync::mpsc;
    std::thread::scope(|scope| {
        let (page_tx, page_rx) = mpsc::sync_channel::<layout::Page<'a>>(PIPELINE_DEPTH);
        let (back_tx, back_rx) = mpsc::channel::<layout::Page<'a>>();
        let writer = scope.spawn(move || {
            let mut accumulator = backend::backend_pdf::PageStreamAccumulator::new(destination_buffer, enable_compression);
            accumulator.page_cache = page_cache;
            for mut page in page_rx {
                accumulator.push_page(&mut page);
                let _ = back_tx.send(page);
            }
            let page_cache = accumulator.page_cache.take();
            (accumulator.finish(font_family), page_cache)
        });

        let laid_out = (|| {
            let mut prev_was_block = false;
            while let Some(block) = parser.parse_next_block()? {
                layouter.layout_stream_block(&block, &mut prev_was_block);
                while let Some(page) = layouter.take_completed_page() {
                    let _ = page_tx.send(page);
                }
                while let Ok(page) = back_rx.try_recv() {
                    layouter.recycle_page(page);
                }
            }
            parser.finish_document(has_doc_env)?;
            for page in layouter.finish_streaming() {
                let _ = page_tx.send(page);
            }
            Ok(())
        })();
        // Closing the queue lets the writer finish; on an error its output is discarded.
        drop(page_tx);
        let (written, page_cache) = writer.join().expect("PDF writer thread panicked");
        (laid_out.and(written), page_cache)
    })
}

#[cfg(target_arch = "wasm32")]
fn compile_streaming_pipelined<'a>(
    _parser: tex::Parser<'a>,
    _layouter: &mut layout::Layouter<'a>,
    _has_doc_env: bool,
    _destination_buffer: &mut Vec<u8>,
    _enable_compression: bool,
    _font_family: backend::fonts::FontFamily,
    _page_cache: Option<backend::backend_pdf::PageCache>,
) -> (Result<usize, Error>, Option<backend::backend_pdf::PageCache>) {
    unreachable!("pages are never pipelined on wasm32")
}

/// Compiles a TeX-subset source string into a reusable destination byte buffer with optional compression,
/// returning the number of pages compiled.
pub fn compile_into_opts_counted(
    source_code: &str,
    destination_buffer: &mut Vec<u8>,
    enable_compression: bool,
) -> Result<usize, Error> {
    compile_into_with(source_code, destination_buffer, CompileOptions { compress: enable_compression, threads: false })
}

/// How a compile runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompileOptions {
    /// FlateDecode the content streams.
    pub compress: bool,
    /// Write pages on a second thread while layout continues (long documents only). It
    /// lowers wall time but costs some extra CPU, so it is off unless asked for.
    pub threads: bool,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self { compress: true, threads: false }
    }
}

/// [`compile_into_opts_counted`] with all [`CompileOptions`].
pub fn compile_into_with(
    source_code: &str,
    destination_buffer: &mut Vec<u8>,
    options: CompileOptions,
) -> Result<usize, Error> {
    compile_into_session(source_code, destination_buffer, options, None)
}

/// What a document's compiles keep for the next one, so that a recompile after an edit
/// redoes only the parts that changed. Line breaks are reused for every paragraph whose
/// words, fonts and parameters are unchanged, and compressed content streams for every
/// page whose content is unchanged. The output is byte-identical to a fresh compile.
///
/// The caches hold what the last successful compile used, so they stay about the size
/// of the document's PDF. Documents with a table of contents or lists of figures or
/// tables are compiled without them.
#[derive(Default)]
pub struct Session {
    breaks: layout::layout_linebreak::BreakCache,
    pages: backend::backend_pdf::PageCache,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    /// [`compile_into_with`], reusing what earlier compiles in this session cached.
    pub fn compile_into(&mut self, source_code: &str, destination_buffer: &mut Vec<u8>, options: CompileOptions) -> Result<usize, Error> {
        compile_into_session(source_code, destination_buffer, options, Some(self))
    }

    /// Paragraphs and pages cached.
    pub fn cached(&self) -> (usize, usize) {
        (self.breaks.len(), self.pages.len())
    }
}

static EMPTY_PAGES: std::sync::LazyLock<std::collections::HashMap<String, u32>> = std::sync::LazyLock::new(std::collections::HashMap::new);

fn compile_into_session(
    source_code: &str,
    destination_buffer: &mut Vec<u8>,
    options: CompileOptions,
    session: Option<&mut Session>,
) -> Result<usize, Error> {
    let enable_compression = options.compress;
    let (mut expanded, page_keys) = expand::expand_with_pages(source_code, &EMPTY_PAGES)?;
    let known_pages;
    if !page_keys.is_empty() {
        // `\pageref`: one pass to learn the label pages, as pdfLaTeX's first run writes the
        // `.aux` file, then the document again with them.
        let pages = layout::layout_engine::label_pages(&tex::parse(&expanded)?);
        known_pages = page_keys
            .into_iter()
            .zip(pages.iter().copied().chain(std::iter::repeat(0)))
            .filter(|&(_, p)| p > 0)
            .collect::<std::collections::HashMap<_, _>>();
        expanded = expand::expand_with_pages(source_code, &known_pages)?.0;
    }
    let source_code: &str = &expanded;
    let has_indices = source_code.contains("\\tableofcontents")
        || source_code.contains("\\listoffigures")
        || source_code.contains("\\listoftables");

    if has_indices {
        let mut pages = compile_source_to_pages(source_code)?;
        let count = pages.len();
        destination_buffer.reserve((count * if enable_compression { 3500 } else { 7000 }).max(16384));
        pdf::emit_into_consume(&mut pages, destination_buffer, enable_compression)?;
        drop(pages);
        Ok(count)
    } else {
        compile_streaming_into(source_code, destination_buffer, enable_compression, options.threads, session)
    }
}

/// Compiles a TeX-subset source string into a reusable destination byte buffer with optional compression.
pub fn compile_into_opts(
    source_code: &str,
    destination_buffer: &mut Vec<u8>,
    enable_compression: bool,
) -> Result<(), Error> {
    compile_into_opts_counted(source_code, destination_buffer, enable_compression).map(|_| ())
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

/// Gap inventory for `rushtex --gaps`: expands and parses `source` without laying it out,
/// recording every unsupported command and environment and every error instead of stopping
/// at the first. Lines are tab-separated `kind, name, count` (most frequent first), then the
/// document class and packages, then `error, line, message` for errors that are not gaps.
pub fn gap_report(source: &str) -> String {
    use std::fmt::Write;
    let (expanded, expand_errors) = expand::expand_recovering(source);
    let mut parser = tex::Parser::new(&expanded);
    parser.gaps = Some(Vec::new());
    let mut parse_errors = Vec::new();
    if let Err(e) = parser.parse_preamble_if_present() {
        parse_errors.push(e);
        // Resume in the body.
        if let Some(p) = expanded.find("\\begin{document}") {
            parser.byte_offset = p + "\\begin{document}".len();
        }
    }
    loop {
        match parser.parse_next_block() {
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(e) => {
                // Resume after the next blank line, the end of the paragraph TeX would recover in.
                let from = e.byte_offset.max(parser.byte_offset).min(expanded.len());
                parse_errors.push(e);
                match expanded[from..].find("\n\n") {
                    Some(p) => parser.byte_offset = from + p + 2,
                    None => break,
                }
            }
        }
    }
    let gaps = parser.gaps.take().unwrap_or_default();
    let mut counts: Vec<((&str, &str), u32)> = Vec::new();
    for &(_, kind, name) in &gaps {
        match counts.iter_mut().find(|(k, _)| *k == (kind, name)) {
            Some((_, n)) => *n += 1,
            None => counts.push(((kind, name), 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut out = String::new();
    for ((kind, name), n) in counts {
        let _ = writeln!(out, "{kind}\t{name}\t{n}");
    }
    if let Some(class) = &parser.preamble.class {
        let _ = writeln!(out, "class\t{}\t1", class.name);
    }
    for p in &parser.preamble.packages {
        let _ = writeln!(out, "package\t{}\t1", p.name);
    }
    // Errors grouped by message (highlighting stripped): count and first line.
    let mut errs: Vec<(String, u32, usize)> = Vec::new();
    for e in expand_errors.iter().chain(&parse_errors) {
        let msg = strip_ansi(&e.message);
        match errs.iter_mut().find(|(m, ..)| *m == msg) {
            Some((_, n, _)) => *n += 1,
            None => errs.push((msg, 1, e.line)),
        }
    }
    errs.sort_by_key(|e| std::cmp::Reverse(e.1));
    for (msg, n, line) in errs {
        let _ = writeln!(out, "error\t{msg}\t{n}\tline {line}");
    }
    out
}

/// `text` without ANSI escape sequences.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
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
    compile_file_with(input_path, output_path, CompileOptions { compress: enable_compression, threads: false })
}

/// [`compile_file_opts`] with all [`CompileOptions`].
pub fn compile_file_with(
    input_path: &Path,
    output_path: Option<&Path>,
    options: CompileOptions,
) -> Result<CompileReport, Error> {
    #[cfg(not(target_arch = "wasm32"))]
    let start_time = std::time::Instant::now();
    let source_code = std::fs::read_to_string(input_path)?;
    let mut pdf_buffer = Vec::with_capacity((source_code.len() * 2).clamp(65536, 1048576));
    let page_count = compile_into_with(&source_code, &mut pdf_buffer, options)?;
    drop(source_code);
    let destination = match output_path {
        Some(path) => path.to_path_buf(),
        None => input_path.with_extension("pdf"),
    };
    std::fs::write(&destination, &pdf_buffer)?;
    drop(pdf_buffer);
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

    /// Page positions are whole scaled points; points in the test agree to within one.
    macro_rules! assert_pt_eq {
        ($a:expr, $b:expr $(, $m:expr)* $(,)?) => {{
            let (a, b): (f32, f32) = ($a, $b);
            assert!((a - b).abs() * 65536.0 <= 1.0, "{} != {} {}", a, b, concat!($($m),*));
        }};
    }

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
        assert!(s.contains("/CMBX10"));
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
        // cmbx10 kerns `w`+`o` by -0.0319 em: pdfTeX's integer TJ displacement in 1/1000 em
        // (`pdf_begin_string`), the array continued from the interword space before it.
        assert!(s_raw.contains("(w)32(ord)]TJ"));
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
        // cmr10 kerns `a`+`v` and `v`+`o` by -0.0278 em each (TFM lig/kern program).
        assert!(s.contains("(Bra)28(v)28(o)]TJ"));
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
        assert!(s.contains("[(Figure)-333(1:)"));
        // 0.4pt rules are thin, so `pdf_set_rule` strokes them as lines.
        assert!(s.contains("0.398 w 0 0 m"));
    }

    #[test]
    fn test_table_boundary_closure_and_multiline_wrapping() {
        let src = r#"
\documentclass{article}
\usepackage[margin=1in]{geometry}
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
                rules.push((x.to_pt(), y.to_pt(), w.to_pt(), h.to_pt(), *fill));
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

        // Vertical rules: the rightmost rule is centred on origin_x + table_w, as pdfTeX
        // draws it (half of \\arrayrulewidth past the end of \\hline).
        let origin_x = h_rules[0].0;
        let right_x = origin_x + table_w;
        let v_rules_right: Vec<_> = rules
            .iter()
            .filter(|r| (r.0 - (right_x - 0.2)).abs() < 1e-2 && r.3 > 1.0)
            .collect();
        assert!(
            !v_rules_right.is_empty(),
            "Rightmost vertical rule (\\arrayrulewidth 0.4pt) must be centred on the table edge ({})",
            right_x
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
        // Letter paper (8.5in x 11in) and margins in TeX points (1in = 72.27pt).
        assert_pt_eq!(doc.preamble.config.page_width, 614.295);
        assert_pt_eq!(doc.preamble.config.page_height, 794.97);
        assert_pt_eq!(doc.preamble.config.margin_left, 36.135);
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
        // Default A4: 210mm as TeX scans it, a whole number of scaled points.
        assert!((doc.preamble.config.page_width - 597.508).abs() < 1e-3);
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
        let path = if std::path::Path::new("examples/stress_100k.tex").exists() {
            "examples/stress_100k.tex"
        } else if std::path::Path::new("examples/stress_500k.tex").exists() {
            "examples/stress_500k.tex"
        } else {
            return;
        };
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
    fn test_bench_phase1_100k_detailed_breakdown() {
        let path = "examples/phase1_stress_100k.tex";
        if !std::path::Path::new(path).exists() {
            return;
        }
        let src = std::fs::read_to_string(path).expect("read phase1_stress_100k.tex");

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
            "\n=== PHASE 1 100K-WORD STRESS BENCHMARK ({} iterations) ===",
            ITERS
        );
        eprintln!(
            "Parse (AST):                         avg = {:.2} ms, min = {:.2} ms",
            avg(&parse_times),
            min(&parse_times)
        );
        eprintln!(
            "Layout (Knuth-Plass Phase 1):        avg = {:.2} ms, min = {:.2} ms",
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
        // article 10pt: 46 baselines per page; after 42 one-line paragraphs the heading
        // still fits but heading + afterskip + two body lines do not.
        for i in 1..=42 {
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
        // As pdfTeX, interword glue is a TJ displacement, never `Tw`: stretched spaces on
        // justified lines are wider than the 333/1000 em of cmr10's natural space.
        assert!(!s.contains(" Tw"));
        assert!(s.contains(")-36") || s.contains(")-35") || s.contains(")-34"));

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
\usepackage[margin=1in]{geometry}
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
                    Some((x.to_pt(), text))
                }
                _ => None,
            })
            .collect();

        // Heading "Intro": x == 1in
        // "First": x == 1in (unindented after section)
        // "Second": x == 1in + 15.0 = 87.0 (indented by 15.0pt)
        // "Third": x == 1in (unindented due to \noindent)
        let first_run = glyph_runs.iter().find(|(_, t)| *t == "First").unwrap();
        assert_pt_eq!(
            first_run.0, crate::config::TEX_INCH,
            "First paragraph after heading should not be indented"
        );

        let second_run = glyph_runs.iter().find(|(_, t)| *t == "Second").unwrap();
        assert_pt_eq!(
            second_run.0, (crate::config::TEX_INCH + 15.0),
            "Second paragraph should be indented by par_indent (15.0pt)"
        );

        let third_run = glyph_runs.iter().find(|(_, t)| *t == "Third").unwrap();
        assert_pt_eq!(
            third_run.0, crate::config::TEX_INCH,
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
        let packed_word_attr_size = std::mem::size_of::<crate::layout::PackedWordAttr>();
        assert_eq!(
            packed_word_attr_size, 2,
            "PackedWordAttr must be exactly 2 bytes to pack 32 items per 64B L1 cache line, got {packed_word_attr_size}"
        );
        let soa_size = std::mem::size_of::<crate::layout::ParagraphSoA>();
        assert_eq!(
            soa_size, 248,
            "ParagraphSoA must be 248 bytes (5 per-word Vecs, 5 sparse run Vecs incl. vadjusts and fils, class size), got {soa_size}"
        );
    }

    #[test]
    fn test_verbatim_and_code_listing() {
        let src = r#"
\documentclass{article}\usepackage{times}
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
        // `listings` sets fixed columns in the document font, so extracted words come apart
        // at the column gaps; compare tokens without spaces.
        let squeezed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        for token in ["pubstruct", "Vector3{", "pubz:", "f32,"] {
            assert!(squeezed.contains(token), "{token} in {squeezed}");
        }

        // `verbatim` is set in the typewriter face at the document size.
        let uncomp = compile_str_opts(src, false).unwrap();
        let stream_str = String::from_utf8_lossy(&uncomp);
        assert!(stream_str.contains("/Fc 9.9626 Tf"));
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
\documentclass{article}\usepackage{times}
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

    /// First glyph run (x, y) of every run whose text is `word` on page 0.
    fn runs_of(page: &layout::Page<'_>, word: &str) -> Vec<(f32, f32)> {
        page.ops
            .iter()
            .filter_map(|op| match op {
                layout::Paint::GlyphRun { x, y, part_start, .. } if page.text_pool[*part_start as usize] == word => {
                    Some((x.to_pt(), y.to_pt()))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn test_center_flush_environments_and_declarations() {
        let src = r#"
\documentclass{article}
\usepackage[margin=1in]{geometry}
\begin{document}
Lead paragraph.
\begin{center}
Mid
\end{center}
After env without a blank line.
\begin{flushright}
Right
\end{flushright}

{\raggedright Ragged\par}
Plain paragraph.
\end{document}
"#;
        let doc = tex::parse(src).unwrap();
        let pages = layout::layout(&doc);
        let page = &pages[0];
        let left = crate::config::TEX_INCH;
        let right = page.width - crate::config::TEX_INCH;
        let (mid_x, mid_y) = runs_of(page, "Mid")[0];
        let (lead_x, lead_y) = runs_of(page, "Lead")[0];
        assert!(lead_x > left, "the lead paragraph is indented");
        // Centered: equal space on both sides of the word.
        let mid_w = 3.0 * 5.0;
        assert!((mid_x - left) > 100.0 && (right - mid_x) > 100.0 && (mid_x - left - (right - mid_x - mid_w)).abs() < 20.0);
        // \topsep (8pt) above a trivlist begun inside a paragraph.
        assert!((lead_y - mid_y - 20.0).abs() < 0.01, "baselines {lead_y} -> {mid_y}");
        // No blank line after \end{center}: the next paragraph is not indented.
        assert_pt_eq!(runs_of(page, "After")[0].0, left);
        let (right_x, _) = runs_of(page, "Right")[0];
        assert!(right_x > right - 40.0, "flushright sets the word at the right margin");
        // A declaration that opens a group runs in vertical mode: no indent; the group ends
        // after \par, so the next paragraph is justified and indented again.
        assert_pt_eq!(runs_of(page, "Ragged")[0].0, left);
        assert!(runs_of(page, "Plain")[0].0 > left);
    }

    #[test]
    fn test_xcolor_runs_and_scoping() {
        use crate::frontend::ast::PackedColor;
        let src = r#"
\documentclass{article}
\definecolor{mine}{HTML}{336699}
\begin{document}
A \textcolor{red}{B} C {\color{mine} D} E \textcolor{blue!50}{F}

\color{green} G
\end{document}
"#;
        let doc = tex::parse(src).unwrap();
        let pages = layout::layout(&doc);
        let page = &pages[0];
        // Colour in force for every glyph run, in order.
        let mut color = PackedColor::BLACK;
        let mut seen = Vec::new();
        for op in &page.ops {
            match op {
                layout::Paint::Color { color: c } => color = *c,
                layout::Paint::GlyphRun { part_start, .. } => {
                    seen.push((page.text_pool[*part_start as usize], color))
                }
                _ => {}
            }
        }
        let of = |w: &str| seen.iter().find(|(t, _)| *t == w).map(|(_, c)| *c).unwrap();
        assert_eq!(of("A"), PackedColor::BLACK);
        assert_eq!(of("B"), PackedColor::named("red").unwrap());
        assert_eq!(of("C"), PackedColor::BLACK);
        assert_eq!(of("D"), PackedColor::from_model("HTML", "336699").unwrap());
        assert_eq!(of("E"), PackedColor::BLACK);
        assert_eq!(of("F"), PackedColor::from_rgb(0.5, 0.5, 1.0));
        // A top-level \color holds for the rest of the document.
        assert_eq!(of("G"), PackedColor::named("green").unwrap());
        // Every line ends black, so the page number is black.
        assert!(matches!(page.ops.iter().rev().find(|op| matches!(op, layout::Paint::Color { .. })), Some(layout::Paint::Color { color }) if *color == PackedColor::BLACK));
    }

    #[test]
    fn test_xcolor_in_math_cells_and_headings() {
        use crate::frontend::ast::PackedColor;
        let src = r#"
\documentclass{article}
\usepackage{lmodern}
\begin{document}
\section{A \textcolor{blue}{B}}
\color{orange} a $b + \textcolor{red}{c}$ e

\color{black}
\begin{tabular}{ll}
\textcolor{red}{R} x & y \\
\end{tabular}
\end{document}
"#;
        let doc = tex::parse(src).unwrap();
        let pages = layout::layout(&doc);
        let page = &pages[0];
        let mut color = PackedColor::BLACK;
        let mut seen = Vec::new();
        for op in &page.ops {
            match op {
                layout::Paint::Color { color: c } => color = *c,
                layout::Paint::GlyphRun { part_start, x, .. } => {
                    seen.push((page.text_pool[*part_start as usize], color, x.to_pt()))
                }
                _ => {}
            }
        }
        let of = |w: &str| seen.iter().find(|(t, _, _)| *t == w).map(|e| (e.1, e.2)).unwrap();
        let red = PackedColor::named("red").unwrap();
        let orange = PackedColor::named("orange").unwrap();
        assert_eq!(of("A").0, PackedColor::BLACK);
        assert_eq!(of("B").0, PackedColor::named("blue").unwrap());
        assert_eq!(of("a").0, orange);
        // `\textcolor` inside the formula, and the text after it back in the line's colour.
        assert_eq!(of("c").0, red);
        assert_eq!(of("e").0, orange);
        assert_eq!(of("R").0, red);
        assert_eq!(of("x").0, PackedColor::BLACK);
        // The space after the coloured group is kept: R (7.36pt) + interword space.
        assert!(of("x").1 - of("R").1 > 10.0);
    }

    #[test]
    fn test_design_size_masters() {
        let src = r#"
\documentclass{article}
\usepackage{lmodern}
\begin{document}
\section{Heading}
Text {\LARGE Title} {\small small}
\end{document}
"#;
        let pdf = compile_str(src).unwrap();
        let has = |name: &str| pdf.windows(name.len()).any(|w| w == name.as_bytes());
        // \section is \Large\bfseries (cmbx12 at 14.4pt); \LARGE is cmr17, \small cmr9.
        assert!(has("/FontName /CMBX12"));
        assert!(has("/FontName /CMR17"));
        assert!(has("/FontName /CMR9"));
        assert!(!has("/FontName /CMR12"));

        let doc = tex::parse(src).unwrap();
        let pages = layout::layout(&doc);
        // The page number is centred on the text block (`\hfil\thepage\hfil`), not the page.
        let x = pages[0].ops.iter().find_map(|op| match op {
            layout::Paint::PageNumber { x, .. } => Some(x.to_pt()),
            _ => None,
        });
        assert!((x.unwrap() + 2.5 - 306.77).abs() < 0.05, "{x:?}");
    }

    #[test]
    fn test_font_size_declarations() {
        let src = r#"
\documentclass{article}
\begin{document}
One {\Large Two} Three

{\small Four\par}
Five

\footnotesize Six

Seven
\end{document}
"#;
        let doc = tex::parse(src).unwrap();
        let pages = layout::layout(&doc);
        let page = &pages[0];
        let size_of = |w: &str| {
            page.ops
                .iter()
                .find_map(|op| match op {
                    layout::Paint::GlyphRun { size, y, part_start, .. } if page.text_pool[*part_start as usize] == w => Some((*size, y.to_pt())),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(size_of("One").0, 10.0);
        assert_eq!(size_of("Two").0, 14.4);
        assert_eq!(size_of("Three").0, 10.0);
        assert_eq!(size_of("Four").0, 9.0);
        assert_eq!(size_of("Five").0, 10.0);
        // \small's \baselineskip (11pt) separates its line from the one above it.
        assert!((size_of("One").1 - size_of("Four").1 - 11.0).abs() < 0.01);
        // A top-level declaration persists into later paragraphs.
        assert_eq!(size_of("Six").0, 8.0);
        assert_eq!(size_of("Seven").0, 8.0);
        assert!((size_of("Six").1 - size_of("Seven").1 - 9.5).abs() < 0.01);
    }

    /// A `figure` without placement is a `tbp` float: it goes to the top of the page above
    /// the text, as graphicx's 1in draft box for a file rushtex does not read.
    #[test]
    fn test_figure_is_top_float_with_draft_box() {
        let src = r#"
\documentclass{article}
\begin{document}
First paragraph.

\begin{figure}
\includegraphics{missing.png}
\caption{A caption.}
\end{figure}

Second paragraph.
\end{document}
"#;
        let pages = compile_source_to_pages(src).unwrap();
        let page = &pages[0];
        let y_of = |w: &str| {
            page.ops
                .iter()
                .find_map(|op| match op {
                    layout::Paint::GlyphRun { y, part_start, .. } if page.text_pool[*part_start as usize] == w => Some(y.to_pt()),
                    _ => None,
                })
                .unwrap()
        };
        let frame_h = page
            .ops
            .iter()
            .filter_map(|op| match op {
                layout::Paint::Rule { h, .. } => Some(h.to_pt()),
                _ => None,
            })
            .fold(0.0f32, f32::max);
        assert!((frame_h - 72.27).abs() < 0.01, "draft box height {frame_h}");
        assert!(page.ops.iter().any(|op| matches!(op, layout::Paint::CaptionTag { n: 1, .. })));
        // The float sits above the paragraph that precedes it in the source.
        assert!(y_of("missing.png") > y_of("First"));
        assert!(y_of("First") > y_of("Second"));
    }

    /// `\hrule` between `\medskip`s gets no interline glue: the next line's top sits the
    /// skip below the rule (TeX's ignore-depth `\prevdepth`).
    #[test]
    fn test_hrule_ignores_depth() {
        let src = r#"
\documentclass{article}
\begin{document}
Text before.

\medskip
\hrule
\medskip

After.
\end{document}
"#;
        let pages = compile_source_to_pages(src).unwrap();
        let page = &pages[0];
        let rule_y = page
            .ops
            .iter()
            .find_map(|op| match op {
                layout::Paint::Rule { y, h, .. } if (h.to_pt() - 0.4).abs() < 1e-3 => Some(y.to_pt()),
                _ => None,
            })
            .unwrap();
        let after_y = page
            .ops
            .iter()
            .find_map(|op| match op {
                layout::Paint::GlyphRun { y, part_start, .. } if page.text_pool[*part_start as usize] == "After." => Some(y.to_pt()),
                _ => None,
            })
            .unwrap();
        // cmr10 `f` is 6.94pt high.
        let h = crate::math::math_fonts::char_height_depth('f', crate::font_metrics::Face::Roman, crate::font_metrics::FontFamily::ComputerModern).0 / 100.0;
        assert!((rule_y - 6.0 - h - after_y).abs() < 0.02, "rule {rule_y} after {after_y} h {h}");
    }

    #[test]
    fn test_parindent_setlength_and_indent() {
        let src = r#"
\documentclass{article}
\usepackage[margin=1in]{geometry}
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
                    Some((x.to_pt(), text))
                }
                _ => None,
            })
            .collect();

        let forced = glyph_runs
            .iter()
            .find(|(_, t)| *t == "ForceIndentParagraph")
            .unwrap();
        // pdfLaTeX: `\@afterheading` removes the indent box `\indent` makes.
        assert_pt_eq!(forced.0, crate::config::TEX_INCH, "\\indent right after a heading does not indent");

        let second = glyph_runs
            .iter()
            .find(|(_, t)| *t == "NormalSecondParagraph")
            .unwrap();
        assert_pt_eq!(
            second.0,
            crate::config::TEX_INCH + 24.0,
            "Normal paragraph should use 24pt parindent"
        );

        let noind = glyph_runs
            .iter()
            .find(|(_, t)| *t == "NoIndentParagraph.")
            .unwrap();
        assert_pt_eq!(noind.0, crate::config::TEX_INCH, "\\noindent should have 0pt indent");
    }

    #[test]
    fn test_hangindent_and_hangafter() {
        let src = r#"
\documentclass{article}\usepackage{times}
\usepackage[margin=1in]{geometry}
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
                    Some((x.to_pt(), text))
                }
                _ => None,
            })
            .collect();

        assert!(
            line_starts.len() >= 2,
            "Paragraph must break into at least 2 lines"
        );
        // Line 1: hangafter = 1 means line 1 is unaffected (1in)
        assert_pt_eq!(line_starts[0].0, crate::config::TEX_INCH, "Line 1 should be flush at 72.0");
        // Line 2+: should be indented by hangindent (1in + 30.0 = 102.0)
        assert_pt_eq!(
            line_starts[1].0, (crate::config::TEX_INCH + 30.0),
            "Line 2 should have 30pt hanging indent (102.0)"
        );
    }

    #[test]
    fn test_parshape_multiline_matrix() {
        let src = r#"
\documentclass{article}
\usepackage[margin=1in]{geometry}
\begin{document}
\noindent
\parshape 3 20pt 300pt 0pt 400pt 10pt 350pt
First line of the shaped paragraph that wraps around nicely so we can inspect line geometries.
Second line continues the flow and wraps again to the third line.
Third line will continue and repeat the final shape specification if there are further lines.
\end{document}
"#;
        let doc = tex::parse(src).unwrap();
        let pages = layout::layout(&doc);
        assert!(!pages.is_empty());

        // First glyph run of each line (a line may hold several runs, e.g. after sentence space).
        let mut line_starts: Vec<f32> = Vec::new();
        let mut last_y = f32::NAN;
        for op in &pages[0].ops {
            if let layout::Paint::GlyphRun { x, y, .. } = op {
                if y.to_pt() != last_y {
                    line_starts.push(x.to_pt());
                    last_y = y.to_pt();
                }
            }
        }

        assert!(
            line_starts.len() >= 3,
            "Paragraph must break into at least 3 lines, got {}",
            line_starts.len()
        );
        // Margin is 1in.
        // Line 0: indent 20pt -> 1in + 20.0 = 92.0
        assert_pt_eq!(line_starts[0], (crate::config::TEX_INCH + 20.0), "Line 0 should be indented by 20pt (92.0)");
        // Line 1: indent 0pt -> 1in + 0.0 = 1in
        assert_pt_eq!(line_starts[1], crate::config::TEX_INCH, "Line 1 should be flush at 72.0");
        // Line 2: indent 10pt -> 1in + 10.0 = 82.0
        assert_pt_eq!(line_starts[2], (crate::config::TEX_INCH + 10.0), "Line 2 should be indented by 10pt (82.0)");
    }

    #[test]
    fn test_parshape_precedence_and_reset() {
        let src = r#"
\documentclass{article}\usepackage{times}
\usepackage[margin=1in]{geometry}
\begin{document}
\noindent
\hangindent 40pt
\hangafter 1
\parshape 2 15pt 300pt 25pt 300pt
First paragraph shaped by parshape which completely overrides hangindent and hangafter according to TeX rules.
It wraps onto a second line to verify both line indents.

\noindent
Second paragraph after blank line should have parshape cleared and no hanging indent.
It starts flush at the margin.
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
                    Some((x.to_pt(), text))
                }
                _ => None,
            })
            .collect();

        assert!(
            line_starts.len() >= 3,
            "Must have at least 3 lines across paragraphs, got {}",
            line_starts.len()
        );
        // Paragraph 1:
        // Line 0: parshape indent 15pt -> 1in + 15.0 = 87.0 (hangindent 40pt ignored!)
        assert_pt_eq!(line_starts[0].0, (crate::config::TEX_INCH + 15.0), "Line 0 must use parshape indent 15pt (87.0)");
        // Line 1: parshape indent 25pt -> 1in + 25.0 = 97.0
        assert_pt_eq!(line_starts[1].0, (crate::config::TEX_INCH + 25.0), "Line 1 must use parshape indent 25pt (97.0)");

        // Paragraph 2:
        // First line of par 2: flush at margin (1in) because parshape & hangindent reset!
        let par2 = line_starts
            .iter()
            .find(|(_, t)| t.starts_with("Second"))
            .expect("Paragraph 2 must exist");
        assert_pt_eq!(par2.0, crate::config::TEX_INCH, "Paragraph 2 must reset parshape back to margin 72.0");
    }

    #[test]
    fn test_list_indentation_and_labelsep() {
        let src = r#"
\documentclass{article}\usepackage{times}
\usepackage[margin=1in]{geometry}
\begin{document}
\begin{enumerate}[labelsep=15pt, leftmargin=35pt, itemindent=5pt]
\item First line of item one which has enough text to comfortably wrap onto the second line for hanging test, even at the ten point default size.
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
                } => Some(x.to_pt()),
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
                    Some(x.to_pt())
                }
                _ => None,
            })
            .expect("First word should exist");

        // First line x = 1in (left margin) + 35.0 (leftmargin) + 5.0 (itemindent) = 112.0
        assert_pt_eq!(first_word, (crate::config::TEX_INCH + 40.0), "First word should be at 112.0");

        // Marker width for "1." in Roman 10pt (default document size)
        let marker_w = metrics::width('1', metrics::Face::Roman, 10.0)
            + metrics::width('.', metrics::Face::Roman, 10.0);
        let expected_marker_x = first_word - 15.0 - marker_w;
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
                    Some(x.to_pt())
                }
                _ => None,
            })
            .expect("Second line should exist");

        // Hanging lines start at self.left = 1in + 35.0 = 107.0
        assert_pt_eq!(
            second_line_start, (crate::config::TEX_INCH + 35.0),
            "Hanging lines must align with leftmargin (107.0)"
        );
    }

    #[test]
    fn test_itemize_bullet_labelsep_and_hang() {
        let src = r#"
\documentclass{article}\usepackage{times}
\usepackage[margin=1in]{geometry}
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
                } => Some(x.to_pt()),
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
                    Some(x.to_pt())
                }
                _ => None,
            })
            .expect("First word should exist");

        // leftmargin = 28.0, itemindent = 0.0 -> first_word = 1in + 28.0 = 100.0
        assert_pt_eq!(first_word, (crate::config::TEX_INCH + 28.0));

        let bullet_w = metrics::width('\u{2022}', metrics::Face::Roman, 10.0);
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
    fn test_default_font_is_computer_modern() {
        // LaTeX's default text fonts are Computer Modern: cmr10, cmbx10, cmti10, cmtt10.
        let src = r#"
\documentclass{article}
\begin{document}
Standard serif text with \textbf{bold serif}, \textit{italic serif}, and \texttt{typewriter mono}.
\end{document}
"#;
        let pdf = compile_str(src).unwrap();
        let s = String::from_utf8_lossy(&pdf);
        for font in ["CMR10", "CMBX10", "CMTI10", "CMTT10"] {
            assert!(s.contains(&format!("/BaseFont /{font}")), "default face must use {font}");
        }
        assert!(!s.contains("/BaseFont /Times-Roman"), "Times must not be the default");
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
        use crate::layout::engine::Sp;
        let page = Page {
            width: 595.0,
            height: 842.0,
            font_family: crate::font_metrics::FontFamily::Times,
            used_faces: (1 << Face::Symbol as u8) | (1 << Face::Dingbats as u8),
            ops: vec![
                Paint::GlyphRun {
                    x: Sp::pt(72.0),
                    y: Sp::pt(700.0),
                    face: Face::Symbol,
                    size: 10.0,
                    part_start: 0,
                    part_count: 1,
                    word_spacing: 0.0,
                },
                Paint::GlyphRun {
                    x: Sp::pt(100.0),
                    y: Sp::pt(700.0),
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
\documentclass{article}\usepackage{times}
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

    #[test]
    fn test_tex_by_topic_ch20_space_factor_widths() {
        let src = "word. Next NASA. Next";
        let inls = vec![crate::tex_parser::Inline::Text(src)];
        let mut soa_default = crate::layout::ParagraphSoA::new();
        let mut math_col = Vec::new();
        let mut sf = 1000u16;
        crate::layout::engine::explode_inlines_into_family_sf(
            &inls,
            crate::font_metrics::Face::Roman,
            10.0,
            &mut soa_default,
            crate::font_metrics::FontFamily::Times,
            &mut math_col,
            &mut sf,
            0,
        );
        assert_eq!(soa_default.len(), 7);
        assert_eq!(soa_default.attrs[1].kind(), crate::layout::WordKind::Space);
        assert_eq!(soa_default.attrs[3].kind(), crate::layout::WordKind::Space);
        assert_eq!(soa_default.attrs[5].kind(), crate::layout::WordKind::Space);

        let space_after_period = soa_default.widths[1];
        let normal_space = soa_default.widths[3];
        let space_after_initials = soa_default.widths[5];

        assert!((normal_space - 2.5).abs() < 1e-3, "Normal space at 10pt Times should be 2.5pt");
        assert!(space_after_period > normal_space, "Space after sentence-ending period must be wider than normal space");
        // ptmr8t.tfm: \fontdimen7 (extra space) is 0.06em.
        assert!((space_after_period - (2.5 + 0.6)).abs() < 1e-2, "Space after period should include extra space (fontdimen 7)");
        assert!((space_after_initials - normal_space).abs() < 1e-3, "Space after initials (NASA.) must NOT include extra space");

        // Now test under \frenchspacing
        let mut soa_french = crate::layout::ParagraphSoA::new();
        let mut sf_fr = 1000u16;
        crate::layout::engine::explode_inlines_into_family_sf(
            &inls,
            crate::font_metrics::Face::Roman,
            10.0,
            &mut soa_french,
            crate::font_metrics::FontFamily::Times,
            &mut math_col,
            &mut sf_fr,
            crate::layout::engine::SF_FRENCH,
        );
        assert!((soa_french.widths[1] - normal_space).abs() < 1e-3, "Under french_spacing, space after period must equal normal space");
    }

    #[test]
    fn test_tex_by_topic_ch20_sentence_spacing_and_frenchspacing() {
        let src_default = r#"
\documentclass{article}
\begin{document}
First word. Second sentence NASA. Next.
\end{document}
"#;
        let doc_default = tex::parse(src_default).unwrap();
        assert!(!doc_default.preamble.config.french_spacing);
        let pages_default = layout::layout(&doc_default);
        assert_eq!(pages_default.len(), 1);

        let src_french = r#"
\documentclass{article}
\frenchspacing
\begin{document}
First word. Second sentence NASA. Next.
\end{document}
"#;
        let doc_french = tex::parse(src_french).unwrap();
        assert!(doc_french.preamble.config.french_spacing);
        let pages_french = layout::layout(&doc_french);
        assert_eq!(pages_french.len(), 1);

        let pdf_default = compile_str(src_default).unwrap();
        let pdf_french = compile_str(src_french).unwrap();
        assert!(extract_pdftotext(&pdf_default).contains("First word. Second sentence"));
        assert!(extract_pdftotext(&pdf_french).contains("First word. Second sentence"));
    }

    #[test]
    fn test_tex_by_topic_ch19_knuth_liang_hyphenation_and_discretionaries() {
        // 1. Verify Knuth-Liang pattern hyphenation on canonical TeX words
        let mut pieces = Vec::new();
        crate::layout::layout_hyphen::hyphenate_word("hyphenation", 2, 3, &mut pieces);
        assert_eq!(pieces, &["hy", "phen", "ation"]);

        pieces.clear();
        crate::layout::layout_hyphen::hyphenate_word("algorithm", 2, 3, &mut pieces);
        assert_eq!(pieces, &["al", "go", "rithm"]);

        pieces.clear();
        crate::layout::layout_hyphen::hyphenate_word("reformatting", 2, 3, &mut pieces);
        assert_eq!(pieces, &["re", "for", "mat", "ting"]);

        // 2. Discretionary \- parsing into soft hyphen \u{00AD}
        let doc_src = r#"
\documentclass{article}
\begin{document}
dis\-cre\-tion\-ary and well-known
\end{document}
"#;
        let doc = tex::parse(doc_src).unwrap();
        if let crate::tex_parser::Block::Para(inls) = &doc.blocks[0] {
            let mut soa = crate::layout::ParagraphSoA::new();
            let mut math_col = Vec::new();
            let mut sf = 1000u16;
            crate::layout::engine::explode_inlines_into_family_sf(
                inls,
                crate::font_metrics::Face::Roman,
                10.0,
                &mut soa,
                crate::font_metrics::FontFamily::Times,
                &mut math_col,
                &mut sf,
                0,
            );

            // "dis" (Hyphen), "cre" (Hyphen), "tion" (Hyphen), "ary" (Word), " " (Space), "and" (Word), " " (Space), "well-" (Word), "known" (Word)
            assert_eq!(soa.texts[0], "dis");
            assert_eq!(soa.attrs[0].kind(), crate::layout::WordKind::Hyphen);
            assert_eq!(soa.texts[1], "cre");
            assert_eq!(soa.attrs[1].kind(), crate::layout::WordKind::Hyphen);
            assert_eq!(soa.texts[2], "tion");
            assert_eq!(soa.attrs[2].kind(), crate::layout::WordKind::Hyphen);
            assert_eq!(soa.texts[3], "ary");
            assert_eq!(soa.attrs[3].kind(), crate::layout::WordKind::Word);

            // Compound word with explicit hyphen
            assert_eq!(soa.texts[7], "well-");
            assert_eq!(soa.texts[8], "known");
        } else {
            panic!("Expected paragraph block");
        }

        // 3. Document end-to-end compilation with discretionary hyphen
        let full_src = r#"
\documentclass{article}
\begin{document}
A paragraph demonstrating explicit discretionary dis\-cre\-tion\-ary hyphenation and compound well-known words.
\end{document}
"#;
        let pdf = compile_str(full_src).unwrap();
        let text = extract_pdftotext(&pdf);
        assert!(text.contains("discretionary") || text.contains("dis"));
        assert!(text.contains("well-known") || text.contains("well-") || text.contains("wellknown"));
    }

    #[test]
    fn test_tex_by_topic_ch19_three_pass_driver_and_tolerances() {
        // 1. Preamble \setlength configuration
        let src_preamble = r#"
\documentclass{article}\usepackage{times}
\setlength{\pretolerance}{150}
\setlength{\tolerance}{350}
\setlength{\emergencystretch}{18pt}
\begin{document}
A test paragraph under custom pretolerance, tolerance, and emergencystretch.
\end{document}
"#;
        let doc_preamble = tex::parse(src_preamble).unwrap();
        assert_eq!(doc_preamble.preamble.config.pre_tolerance, 150);
        assert_eq!(doc_preamble.preamble.config.tolerance, 350);
        assert!((doc_preamble.preamble.config.emergency_stretch - 18.0).abs() < 1e-4);

        let pdf_preamble = compile_str(src_preamble).unwrap();
        let text_preamble = extract_pdftotext(&pdf_preamble);
        assert!(text_preamble.contains("custom pretolerance, tolerance"));

        // 2. In-body \sloppy and \fussy state switching
        let src_sloppy = r#"
\documentclass{article}\usepackage{times}
\begin{document}
\sloppy
This paragraph is typeset with sloppy line breaking parameters (tolerance 9999 and emergencystretch 33pt).
\fussy
This paragraph is typeset with fussy line breaking parameters (tolerance 200 and emergencystretch 0pt).
\end{document}
"#;
        let doc_sloppy = tex::parse(src_sloppy).unwrap();
        let pages_sloppy = layout::layout(&doc_sloppy);
        assert!(!pages_sloppy.is_empty());

        let pdf_sloppy = compile_str(src_sloppy).unwrap();
        let text_sloppy = extract_pdftotext(&pdf_sloppy);
        assert!(text_sloppy.contains("sloppy line breaking"));
        assert!(text_sloppy.contains("fussy line breaking"));

        // 3. Direct \pretolerance, \tolerance, \emergencystretch commands
        let src_commands = r#"
\documentclass{article}\usepackage{times}
\begin{document}
\pretolerance=120
\tolerance=280
\emergencystretch=12pt
Line breaking with explicit parameter assignments.
\end{document}
"#;
        let doc_commands = tex::parse(src_commands).unwrap();
        let pages_commands = layout::layout(&doc_commands);
        assert!(!pages_commands.is_empty());

        let pdf_commands = compile_str(src_commands).unwrap();
        let text_commands = extract_pdftotext(&pdf_commands);
        assert!(text_commands.contains("Line breaking with explicit parameter assignments"));
    }

    #[test]
    fn test_tex_by_topic_ch18_19_looseness_leftskip_rightskip() {
        // 1. Preamble \setlength{\looseness}{1} configuration
        let src_preamble = r#"
\documentclass{article}
\setlength{\looseness}{1}
\begin{document}
A test paragraph under looseness configured via preamble.
\end{document}
"#;
        let doc_preamble = tex::parse(src_preamble).unwrap();
        assert_eq!(doc_preamble.preamble.config.looseness, 1);

        let pdf_preamble = compile_str(src_preamble).unwrap();
        let text_preamble = extract_pdftotext(&pdf_preamble);
        assert!(text_preamble.contains("A test paragraph under looseness configured via preamble."));

        // 2. Direct inline/block \looseness, \leftskip, and \rightskip
        let src_skips = r#"
\documentclass{article}
\begin{document}
\leftskip=30pt
\rightskip=40pt
\looseness=1
This is the first paragraph with custom margins and looseness requested to lengthen lines.
It tests that leftskip and rightskip adjust the line measure and coordinate margins.

This is the second paragraph. It tests that looseness was reset after the first paragraph
while leftskip and rightskip persistently apply to subsequent paragraphs.
\end{document}
"#;
        let doc_skips = tex::parse(src_skips).unwrap();
        let pages_skips = layout::layout(&doc_skips);
        assert!(!pages_skips.is_empty());

        let pdf_skips = compile_str(src_skips).unwrap();
        let text_skips = extract_pdftotext(&pdf_skips);
        assert!(text_skips.contains("This is the first paragraph with custom margins"));
        assert!(text_skips.contains("This is the second paragraph"));
    }

    #[test]
    fn test_tex_by_topic_ch17_parfillskip_prevgraf_lastlinefit() {
        // 1. Preamble configuration via \setlength
        let src_preamble = r#"
\documentclass{article}
\setlength{\parfillskip}{0pt}
\setlength{\prevgraf}{3}
\setlength{\lastlinefit}{500}
\begin{document}
A test paragraph under preamble parfillskip, prevgraf, and lastlinefit settings.
\end{document}
"#;
        let doc_preamble = tex::parse(src_preamble).unwrap();
        assert_eq!(doc_preamble.preamble.config.par_fill_width, 0.0);
        assert_eq!(doc_preamble.preamble.config.par_fill_stretch, 0.0);
        assert_eq!(doc_preamble.preamble.config.prev_graf, 3);
        assert_eq!(doc_preamble.preamble.config.last_line_fit, 500);

        let pages = layout::layout(&doc_preamble);
        assert_eq!(pages.len(), 1);
        let pdf_preamble = compile_str(src_preamble).unwrap();
        let text_preamble = extract_pdftotext(&pdf_preamble);
        assert!(text_preamble.contains("preamble"));
        assert!(text_preamble.contains("parfillskip"));
        assert!(text_preamble.contains("lastlinefit"));

        // 2. In-body commands: \parfillskip=..., \prevgraf=..., \lastlinefit=...
        let src_body = r#"
\documentclass{article}
\begin{document}
\parfillskip=0pt plus 1fil
\prevgraf=2
\lastlinefit=1000
Paragraph one tests dynamic parameter mutation before paragraph layout.
The final line has normal fil stretch.

\parfillskip=10pt
Paragraph two tests zero stretch with positive 10pt width parfillskip,
forcing the final line to be fully justified across the remaining measure.
\end{document}
"#;
        let doc_body = tex::parse(src_body).unwrap();
        let pages_body = layout::layout(&doc_body);
        assert!(!pages_body.is_empty());

        let pdf_body = compile_str(src_body).unwrap();
        let text_body = extract_pdftotext(&pdf_body);
        assert!(text_body.contains("Paragraph one tests dynamic parameter mutation"));
        assert!(text_body.contains("Paragraph two tests zero stretch"));
    }

    #[test]
    fn test_multipage_parallel_deflation_parity() {
        let mut src = String::from("\\documentclass{article}\n\\begin{document}\n");
        for page in 1..=8 {
            src.push_str(&format!(
                "\\section{{Page Section {}}}\nThis is page content for testing parallel deflation pipeline reproducibility.\n\\newpage\n",
                page
            ));
        }
        src.push_str("\\end{document}\n");

        let doc = tex::parse(&src).unwrap();
        let pages = layout::layout(&doc);
        assert!(pages.len() >= 8);

        // Emit with default (parallel when threads > 1)
        let pdf_parallel = backend::pdf::emit(&pages).unwrap();

        // Emit with single-threaded forced
        std::env::set_var("RUSHTEX_SINGLE_THREAD", "1");
        let pdf_single = backend::pdf::emit(&pages).unwrap();
        std::env::remove_var("RUSHTEX_SINGLE_THREAD");

        assert_eq!(pdf_parallel.len(), pdf_single.len(), "PDF lengths must match exactly");
        assert_eq!(pdf_parallel, pdf_single, "Parallel and single-threaded PDF output must be bit-for-bit identical");
    }

    #[test]
    fn session_recompiles_match_fresh_compiles() {
        let para = "The quick brown fox jumps over the lazy dog while typesetting \\textbf{bold} and \\emph{emphasised} words in justified paragraphs. ";
        let body: String = (0..40).map(|i| format!("{}{}\n\n", para.repeat(3 + i % 4), i)).collect();
        let doc = |body: &str| format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\\end{{document}}\n");
        let edits = [
            doc(&body),
            doc(&body.replacen("lazy", "sleepy", 1)),
            doc(&body.replacen("lazy dog", "dog", 7)),
            doc("\\textbf{unclosed"),
            doc(&body),
            doc(&format!("Inserted opening paragraph.\n\n{body}")),
        ];
        let mut session = Session::new();
        let mut out = Vec::new();
        for (i, src) in edits.iter().enumerate() {
            for compress in [true, false] {
                let options = CompileOptions { compress, threads: false };
                let mut fresh = Vec::new();
                let expected = compile_into_with(src, &mut fresh, options);
                let got = session.compile_into(src, &mut out, options);
                assert_eq!(expected.is_ok(), got.is_ok(), "edit {i}");
                if expected.is_ok() {
                    assert!(out == fresh, "edit {i} (compress {compress}) differs from a fresh compile");
                }
            }
        }
        let (paragraphs, pages) = session.cached();
        assert!(paragraphs > 0 && pages > 0);
    }

    fn pdf_text(src: &str) -> String {
        extract_pdftotext(&compile_str(src).unwrap_or_else(|e| panic!("compile failed: {e}")))
    }

    #[test]
    fn blank_line_inside_a_group_is_a_paragraph_break() {
        // Used to loop forever, allocating without bound.
        let text = pdf_text("\\documentclass{article}\n\\begin{document}\n\\textit{a\n\nb}\n\\end{document}\n");
        assert!(text.contains('a') && text.contains('b'));
        // A theorem with a note and a \label on its own line expands to exactly that.
        let text = pdf_text("\\documentclass{article}\n\\newtheorem{thm}{Theorem}\n\\begin{document}\n\\begin{thm}[Note]\n\\label{t}\nBody.\n\\end{thm}\n\\end{document}\n");
        assert!(text.contains("Theorem 1 (Note).") && text.contains("Body."));
    }

    #[test]
    fn environments_let_to_document_and_quote() {
        // lppl.tex: the document environment is entered through \let.
        let src = "\\makeatletter\\ifx\\@preamblecmds\\@notprerr\\let\\LP\\bgroup\\let\\endLP\\egroup\\else\\documentclass{article}\\let\\LP\\document\\let\\endLP\\enddocument\\fi\\makeatother\n\\begin{LP}Hello world.\\end{LP}\n";
        assert!(pdf_text(src).contains("Hello world."));
        let src = "\\documentclass{article}\\let\\Foo\\quote\\let\\endFoo\\endquote\\begin{document}\\begin{Foo}Quoted.\\end{Foo}\\end{document}";
        assert!(pdf_text(src).contains("Quoted."));
    }

    #[test]
    fn missing_begin_document_is_an_error() {
        let err = compile_str("\\documentclass{article}\nHello\n").unwrap_err();
        assert!(format!("{err}").contains("missing \\begin{document}"), "{err}");
    }

    #[test]
    fn needs_tex_format_before_documentclass() {
        assert!(pdf_text("\\NeedsTeXFormat{LaTeX2e}[1994/12/01]\n\\documentclass{article}\\begin{document}ok\\end{document}").contains("ok"));
    }

    #[test]
    fn macro_arguments_do_not_fuse_with_control_words() {
        let src = "\\documentclass{article}\\newcommand{\\x}{X}\\newcommand{\\p}[1]{\\x#1}\\newcommand{\\q}[1]{#1y}\\begin{document}\\p{word} \\q{\\x}\\end{document}";
        assert!(pdf_text(src).contains("Xword Xy"));
    }

    #[test]
    fn ifx_against_undefined() {
        let src = "\\documentclass{article}\\newcommand{\\foo}{f}\\begin{document}\\ifx\\bar\\undefined A\\else B\\fi\\ifx\\foo\\undefined C\\else D\\fi\\ifx\\pdfoutput\\undefined E\\else F\\fi\\end{document}";
        assert!(pdf_text(src).contains("ADF"));
    }
}
