//! # Backend Output Engines (`backend`)
//!
//! Provides document serialization, PDF object emission, font metrics,
//! and RFC 1951 DEFLATE stream compression.

pub mod backend_afm_ligkern;
pub mod backend_cm_fix;
pub mod backend_cm_fonts;
pub mod backend_cm_optical;
pub mod backend_deflate;
pub mod backend_inflate;
pub mod backend_fonts;
pub mod backend_hash;
pub mod backend_ligkern;
pub mod backend_optical;
pub mod backend_pdf;
pub mod backend_pdftex;
pub mod backend_png;
pub mod backend_scaled;
pub mod backend_tfm;
pub mod backend_tfm_data;
pub mod backend_shape;
pub mod backend_ship;
pub mod backend_ts1;

// Backward-compatible module aliases:
pub use backend_cm_fonts as cm_fonts;
pub use backend_deflate as deflate;
pub use backend_optical as optical;
pub use backend_fonts as fonts;
pub use backend_pdf as pdf;
pub use backend_scaled as scaled;
pub use backend_shape as shape;

pub use backend_pdf::{emit, emit_into, emit_into_opts};
