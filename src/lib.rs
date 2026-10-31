//! # rushtex: LaTeX to PDF, identical to pdfLaTeX
//!
//! Two paths compile a document (`docs/architecture/`):
//!
//! 1. **The native job ([`engine::engine_doc`])**: guarded direct lowering. Where its
//!    pre-scan accepts a document, every construct runs as ported Rust: no format, no TeX
//!    files, one pass.
//! 2. **The TeX engine ([`engine`], feature `oracle`)**: tex.web and pdfTeX ported, run on
//!    the LaTeX format as `pdflatex` runs it, for every document the native job declines.
//!
//! [`data`] holds the tables both read (fonts, hyphenation, the kernel's macros);
//! [`backend`] writes the PDF.

#![forbid(unsafe_code)]

#[path = "backend/backend_mod.rs"]
pub mod backend;
#[path = "data/data_mod.rs"]
pub mod data;
#[path = "engine/engine_mod.rs"]
pub mod engine;
