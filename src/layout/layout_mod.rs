//! # High-Speed Page Layout & Typesetting Engine (`layout`)
//!
//! Provides paragraph layout, Knuth-Plass breaking, hz-algorithm microtypography,
//! code listings, table formatting, and multi-page pagination.

pub mod layout_engine;
pub mod layout_float;
pub mod layout_hyphen;
pub mod layout_hyphen_trie;
pub mod layout_hz;
pub mod layout_linebreak;
pub mod layout_pagebuilder;
pub mod layout_listing;

// Backward-compatible module aliases:
pub use layout_engine as engine;
pub use layout_hyphen as hyphen;
pub use layout_hz as hz;
pub use layout_linebreak as linebreak;
pub use layout_listing as listing;

pub use layout_engine::{
    layout, marker_width, marker_width_family, to_roman, to_upper_roman, CellLineRange, CellSpan,
    LineDescriptor, Marker, Page, Paint, PackedWordAttr, ParagraphSoA, SparseMathIdx, WordKind, CELL_PAD, FIG_H,
    FONT_SIZE, GLYPH_RUN_COUNT_MASK, GLYPH_RUN_SHAPED, LEADING, LIST_INDENT, MARGIN, PAGE_H, PAGE_W,
    PARA_SKIP,
};
pub(crate) use layout_engine::Layouter;
