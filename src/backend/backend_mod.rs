//! # Backend Output Engines (`backend`)
//!
//! Provides document serialization, PDF object emission, font metrics,
//! and RFC 1951 DEFLATE stream compression.

pub mod backend_deflate;
pub mod backend_fonts;
pub mod backend_pdf;

// Backward-compatible module aliases:
pub use backend_deflate as deflate;
pub use backend_fonts as fonts;
pub use backend_pdf as pdf;

pub use backend_pdf::{emit, emit_into, emit_into_opts};
