//! # Backend (`backend`)
//!
//! The PDF writer the engine and the native job ship pages through (pdfTeX's backend:
//! fonts, images, compression), and the scaled arithmetic of TeX's dimensions.

pub mod backend_cache;
pub mod backend_cm_fonts;
pub mod backend_deflate;
pub mod backend_inflate;
pub mod backend_hash;
pub mod backend_pdftex;
pub mod backend_pdfread;
pub mod backend_pk;
pub mod backend_png;
pub mod backend_scaled;
pub mod backend_ship;
pub mod backend_type1;



pub use backend_scaled as scaled;
