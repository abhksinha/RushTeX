//! # OpenType MathGlyphInfo Specification
//!
//! Direct-mapped structures for per-glyph italic corrections, optical top-accent
//! attachment offsets, and extended shape flags.

/// Per-glyph optical metadata from OpenType MathGlyphInfo.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MathGlyphProperties {
    /// Glyph ID
    pub glyph_id: u16,
    /// Italic correction in design units
    pub italic_correction: i16,
    /// Optical top accent attachment horizontal position in design units
    pub top_accent_attachment: i16,
    /// Flags: bit 0 = extended shape (radical/delimiter)
    pub flags: u16,
}

const _: () = assert!(core::mem::size_of::<MathGlyphProperties>() == 8);

impl MathGlyphProperties {
    /// True if the glyph is marked as an extended shape in OpenType MATH.
    #[inline(always)]
    pub fn is_extended_shape(&self) -> bool {
        (self.flags & 0x0001) != 0
    }
}
