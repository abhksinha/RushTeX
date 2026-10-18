//! # OpenType MathVariants Specification
//!
//! Structured representations for multi-tier discrete sizing chains and extensible
//! glyph assemblies conforming to the OpenType MATH table specification.

/// A single variant step in a discrete delimiter or radical sizing chain.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MathGlyphVariantRecord {
    /// Glyph ID of the variant
    pub variant_glyph: u16,
    /// Advance width or height measurement in design units
    pub advance_measurement: u16,
}

const _: () = assert!(core::mem::size_of::<MathGlyphVariantRecord>() == 4);

/// An extensible part specification for multi-piece glyph assembly (top, mid, bot, extender).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphPartRecord {
    /// Glyph ID of the component part
    pub glyph: u16,
    /// Maximum overlap length at the start connection point in design units
    pub start_connector_length: u16,
    /// Maximum overlap length at the end connection point in design units
    pub end_connector_length: u16,
    /// Full unclipped advance dimension of the part in design units
    pub full_advance: u16,
    /// Part flags: bit 0 = is_extender (repeating part)
    pub part_flags: u16,
    /// Explicit padding for 12-byte struct alignment
    pub _pad: u16,
}

const _: () = assert!(core::mem::size_of::<GlyphPartRecord>() == 12);

impl GlyphPartRecord {
    /// True if this piece is an extensible repeating connector.
    #[inline(always)]
    pub fn is_extender(&self) -> bool {
        (self.part_flags & 0x0001) != 0
    }
}
