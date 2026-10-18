//! # TeX Appendix F Mathematical Font Metric Information
//!
//! Authoritative Computer Modern font metric tables (CMSY10, CMEX10, CMMI10)
//! extracted directly from canonical TeX TFM data.
//!
//! Designed with contiguous Array of Structures (AoS) packing:
//! - 8 bytes per stored glyph (width, height, depth, italic correction), so exactly 8
//!   glyphs fit in a single 64-byte L1 cache line
//! - values are stored in 1/16384 em (TFM fix_words rounded to 2^-14), which keeps the
//!   per-glyph error under 0.0004pt at 10pt so long formulas do not drift
//! - zero heap allocations; lookups unpack to [`GlyphMetric`] in 1/1000 em

/// Stored glyph metric entry in units of 1/16384 em ([`PACKED_UNITS_PER_EM`]).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackedGlyphMetric {
    pub w: u16,
    pub h: u16,
    pub d: u16,
    pub ic: u16,
}

/// Resolution of [`PackedGlyphMetric`] fields.
pub const PACKED_UNITS_PER_EM: u32 = 16384;

impl PackedGlyphMetric {
    /// The metric in 1/1000 of the design size.
    #[inline(always)]
    pub const fn unpack(self) -> GlyphMetric {
        const K: f32 = 1000.0 / PACKED_UNITS_PER_EM as f32;
        GlyphMetric { w: self.w as f32 * K, h: self.h as f32 * K, d: self.d as f32 * K, ic: self.ic as f32 * K }
    }

    /// The width rounded to whole 1/1000 em (PDF `/Widths`).
    #[inline(always)]
    pub const fn width_units(self) -> u16 {
        ((self.w as u32 * 1000 + PACKED_UNITS_PER_EM / 2) / PACKED_UNITS_PER_EM) as u16
    }
}

const _: () = assert!(core::mem::size_of::<PackedGlyphMetric>() == 8);

/// TeX Appendix F glyph metric, in 1/1000 of the design size (unpacked on lookup).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphMetric {
    /// Advance width in 1/1000 design units
    pub w: f32,
    /// Typographic ink height above baseline in 1/1000 design units
    pub h: f32,
    /// Typographic ink depth below baseline in 1/1000 design units
    pub d: f32,
    /// Italic correction in 1/1000 design units
    pub ic: f32,
}

/// TeX Appendix F extensible delimiter assembly recipe.
/// Packed into 4 bytes for zero cache pollution.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DelimiterRecipe {
    pub top: u8,
    pub mid: u8,
    pub bot: u8,
    pub rep: u8,
}

const _: () = assert!(core::mem::size_of::<DelimiterRecipe>() == 4);

/// CMSY10 (Math Symbols, Family 2) metric table
pub static CMSY10_METRICS: [PackedGlyphMetric; 128] = [
    PackedGlyphMetric { w: 12743, h: 9557, d: 1365, ic: 0 }, // 0x00
    PackedGlyphMetric { w: 4551, h: 7282, d: 0, ic: 0 }, // 0x01
    PackedGlyphMetric { w: 12743, h: 9557, d: 1365, ic: 0 }, // 0x02
    PackedGlyphMetric { w: 8192, h: 7623, d: 0, ic: 0 }, // 0x03
    PackedGlyphMetric { w: 12743, h: 9557, d: 1365, ic: 0 }, // 0x04
    PackedGlyphMetric { w: 8192, h: 7282, d: 0, ic: 0 }, // 0x05
    PackedGlyphMetric { w: 12743, h: 9557, d: 1365, ic: 0 }, // 0x06
    PackedGlyphMetric { w: 12743, h: 9557, d: 1365, ic: 0 }, // 0x07
    PackedGlyphMetric { w: 12743, h: 9557, d: 1365, ic: 0 }, // 0x08
    PackedGlyphMetric { w: 12743, h: 9557, d: 1365, ic: 0 }, // 0x09
    PackedGlyphMetric { w: 12743, h: 9557, d: 1365, ic: 0 }, // 0x0A
    PackedGlyphMetric { w: 12743, h: 9557, d: 1365, ic: 0 }, // 0x0B
    PackedGlyphMetric { w: 12743, h: 9557, d: 1365, ic: 0 }, // 0x0C
    PackedGlyphMetric { w: 16384, h: 11378, d: 3186, ic: 0 }, // 0x0D
    PackedGlyphMetric { w: 8192, h: 7282, d: 0, ic: 0 }, // 0x0E
    PackedGlyphMetric { w: 8192, h: 7282, d: 0, ic: 0 }, // 0x0F
    PackedGlyphMetric { w: 12743, h: 7598, d: 0, ic: 0 }, // 0x10
    PackedGlyphMetric { w: 12743, h: 7598, d: 0, ic: 0 }, // 0x11
    PackedGlyphMetric { w: 12743, h: 10420, d: 2228, ic: 0 }, // 0x12
    PackedGlyphMetric { w: 12743, h: 10420, d: 2228, ic: 0 }, // 0x13
    PackedGlyphMetric { w: 12743, h: 10420, d: 2228, ic: 0 }, // 0x14
    PackedGlyphMetric { w: 12743, h: 10420, d: 2228, ic: 0 }, // 0x15
    PackedGlyphMetric { w: 12743, h: 10420, d: 2228, ic: 0 }, // 0x16
    PackedGlyphMetric { w: 12743, h: 10420, d: 2228, ic: 0 }, // 0x17
    PackedGlyphMetric { w: 12743, h: 6011, d: 0, ic: 0 }, // 0x18
    PackedGlyphMetric { w: 12743, h: 7915, d: 0, ic: 0 }, // 0x19
    PackedGlyphMetric { w: 12743, h: 8833, d: 641, ic: 0 }, // 0x1A
    PackedGlyphMetric { w: 12743, h: 8833, d: 641, ic: 0 }, // 0x1B
    PackedGlyphMetric { w: 16384, h: 8833, d: 641, ic: 0 }, // 0x1C
    PackedGlyphMetric { w: 16384, h: 8833, d: 641, ic: 0 }, // 0x1D
    PackedGlyphMetric { w: 12743, h: 8833, d: 641, ic: 0 }, // 0x1E
    PackedGlyphMetric { w: 12743, h: 8833, d: 641, ic: 0 }, // 0x1F
    PackedGlyphMetric { w: 16384, h: 6011, d: 0, ic: 0 }, // 0x20
    PackedGlyphMetric { w: 16384, h: 6011, d: 0, ic: 0 }, // 0x21
    PackedGlyphMetric { w: 8192, h: 11378, d: 3186, ic: 0 }, // 0x22
    PackedGlyphMetric { w: 8192, h: 11378, d: 3186, ic: 0 }, // 0x23
    PackedGlyphMetric { w: 16384, h: 6011, d: 0, ic: 0 }, // 0x24
    PackedGlyphMetric { w: 16384, h: 11378, d: 3186, ic: 0 }, // 0x25
    PackedGlyphMetric { w: 16384, h: 11378, d: 3186, ic: 0 }, // 0x26
    PackedGlyphMetric { w: 12743, h: 7598, d: 0, ic: 0 }, // 0x27
    PackedGlyphMetric { w: 16384, h: 6011, d: 0, ic: 0 }, // 0x28
    PackedGlyphMetric { w: 16384, h: 6011, d: 0, ic: 0 }, // 0x29
    PackedGlyphMetric { w: 10012, h: 11378, d: 3186, ic: 0 }, // 0x2A
    PackedGlyphMetric { w: 10012, h: 11378, d: 3186, ic: 0 }, // 0x2B
    PackedGlyphMetric { w: 16384, h: 6011, d: 0, ic: 0 }, // 0x2C
    PackedGlyphMetric { w: 16384, h: 11378, d: 3186, ic: 0 }, // 0x2D
    PackedGlyphMetric { w: 16384, h: 11378, d: 3186, ic: 0 }, // 0x2E
    PackedGlyphMetric { w: 12743, h: 7054, d: 0, ic: 0 }, // 0x2F
    PackedGlyphMetric { w: 4506, h: 9102, d: 0, ic: 0 }, // 0x30
    PackedGlyphMetric { w: 16384, h: 7054, d: 0, ic: 0 }, // 0x31
    PackedGlyphMetric { w: 10923, h: 8833, d: 641, ic: 0 }, // 0x32
    PackedGlyphMetric { w: 10923, h: 8833, d: 641, ic: 0 }, // 0x33
    PackedGlyphMetric { w: 14564, h: 11378, d: 3186, ic: 0 }, // 0x34
    PackedGlyphMetric { w: 14564, h: 11378, d: 3186, ic: 0 }, // 0x35
    PackedGlyphMetric { w: 0, h: 11378, d: 3186, ic: 0 }, // 0x36
    PackedGlyphMetric { w: 0, h: 6011, d: 0, ic: 0 }, // 0x37
    PackedGlyphMetric { w: 9102, h: 11378, d: 0, ic: 0 }, // 0x38
    PackedGlyphMetric { w: 9102, h: 11378, d: 0, ic: 0 }, // 0x39
    PackedGlyphMetric { w: 10923, h: 7054, d: 0, ic: 0 }, // 0x3A
    PackedGlyphMetric { w: 8192, h: 12288, d: 910, ic: 0 }, // 0x3B
    PackedGlyphMetric { w: 11833, h: 11378, d: 0, ic: 0 }, // 0x3C
    PackedGlyphMetric { w: 11833, h: 11378, d: 0, ic: 0 }, // 0x3D
    PackedGlyphMetric { w: 12743, h: 11378, d: 0, ic: 0 }, // 0x3E
    PackedGlyphMetric { w: 12743, h: 11378, d: 0, ic: 0 }, // 0x3F
    PackedGlyphMetric { w: 10012, h: 11378, d: 0, ic: 0 }, // 0x40
    PackedGlyphMetric { w: 13082, h: 11196, d: 0, ic: 0 }, // 0x41
    PackedGlyphMetric { w: 10761, h: 11196, d: 0, ic: 498 }, // 0x42
    PackedGlyphMetric { w: 8627, h: 11196, d: 0, ic: 956 }, // 0x43
    PackedGlyphMetric { w: 12638, h: 11196, d: 0, ic: 455 }, // 0x44
    PackedGlyphMetric { w: 8647, h: 11196, d: 0, ic: 1465 }, // 0x45
    PackedGlyphMetric { w: 11776, h: 11196, d: 0, ic: 1627 }, // 0x46
    PackedGlyphMetric { w: 9746, h: 11196, d: 1593, ic: 972 }, // 0x47
    PackedGlyphMetric { w: 13837, h: 11196, d: 0, ic: 158 }, // 0x48
    PackedGlyphMetric { w: 8921, h: 11196, d: 0, ic: 1210 }, // 0x49
    PackedGlyphMetric { w: 11105, h: 11196, d: 1593, ic: 3026 }, // 0x4A
    PackedGlyphMetric { w: 12484, h: 11196, d: 0, ic: 237 }, // 0x4B
    PackedGlyphMetric { w: 11300, h: 11196, d: 0, ic: 0 }, // 0x4C
    PackedGlyphMetric { w: 19676, h: 11196, d: 0, ic: 0 }, // 0x4D
    PackedGlyphMetric { w: 13443, h: 11196, d: 0, ic: 2414 }, // 0x4E
    PackedGlyphMetric { w: 13044, h: 11196, d: 0, ic: 455 }, // 0x4F
    PackedGlyphMetric { w: 11396, h: 11196, d: 0, ic: 1347 }, // 0x50
    PackedGlyphMetric { w: 13380, h: 11196, d: 1593, ic: 0 }, // 0x51
    PackedGlyphMetric { w: 13886, h: 11196, d: 0, ic: 0 }, // 0x52
    PackedGlyphMetric { w: 9921, h: 11196, d: 0, ic: 1229 }, // 0x53
    PackedGlyphMetric { w: 8923, h: 11196, d: 0, ic: 4164 }, // 0x54
    PackedGlyphMetric { w: 10254, h: 11196, d: 0, ic: 1627 }, // 0x55
    PackedGlyphMetric { w: 10040, h: 11196, d: 0, ic: 1347 }, // 0x56
    PackedGlyphMetric { w: 16184, h: 11196, d: 0, ic: 1347 }, // 0x57
    PackedGlyphMetric { w: 11687, h: 11196, d: 0, ic: 2399 }, // 0x58
    PackedGlyphMetric { w: 10950, h: 11196, d: 1593, ic: 1347 }, // 0x59
    PackedGlyphMetric { w: 11874, h: 11196, d: 0, ic: 1302 }, // 0x5A
    PackedGlyphMetric { w: 10923, h: 9102, d: 0, ic: 0 }, // 0x5B
    PackedGlyphMetric { w: 10923, h: 9102, d: 0, ic: 0 }, // 0x5C
    PackedGlyphMetric { w: 10923, h: 9102, d: 0, ic: 0 }, // 0x5D
    PackedGlyphMetric { w: 10923, h: 9102, d: 0, ic: 0 }, // 0x5E
    PackedGlyphMetric { w: 10923, h: 9102, d: 0, ic: 0 }, // 0x5F
    PackedGlyphMetric { w: 10012, h: 11378, d: 0, ic: 0 }, // 0x60
    PackedGlyphMetric { w: 10012, h: 11378, d: 0, ic: 0 }, // 0x61
    PackedGlyphMetric { w: 7282, h: 12288, d: 4096, ic: 0 }, // 0x62
    PackedGlyphMetric { w: 7282, h: 12288, d: 4096, ic: 0 }, // 0x63
    PackedGlyphMetric { w: 7282, h: 12288, d: 4096, ic: 0 }, // 0x64
    PackedGlyphMetric { w: 7282, h: 12288, d: 4096, ic: 0 }, // 0x65
    PackedGlyphMetric { w: 8192, h: 12288, d: 4096, ic: 0 }, // 0x66
    PackedGlyphMetric { w: 8192, h: 12288, d: 4096, ic: 0 }, // 0x67
    PackedGlyphMetric { w: 6372, h: 12288, d: 4096, ic: 0 }, // 0x68
    PackedGlyphMetric { w: 6372, h: 12288, d: 4096, ic: 0 }, // 0x69
    PackedGlyphMetric { w: 4551, h: 12288, d: 4096, ic: 0 }, // 0x6A
    PackedGlyphMetric { w: 8192, h: 12288, d: 4096, ic: 0 }, // 0x6B
    PackedGlyphMetric { w: 8192, h: 12288, d: 4096, ic: 0 }, // 0x6C
    PackedGlyphMetric { w: 10012, h: 12288, d: 4096, ic: 0 }, // 0x6D
    PackedGlyphMetric { w: 8192, h: 12288, d: 4096, ic: 0 }, // 0x6E
    PackedGlyphMetric { w: 4551, h: 11378, d: 3186, ic: 0 }, // 0x6F
    PackedGlyphMetric { w: 13653, h: 655, d: 15729, ic: 0 }, // 0x70
    PackedGlyphMetric { w: 12288, h: 11196, d: 0, ic: 0 }, // 0x71
    PackedGlyphMetric { w: 13653, h: 11196, d: 0, ic: 0 }, // 0x72
    PackedGlyphMetric { w: 6827, h: 11378, d: 3186, ic: 1820 }, // 0x73
    PackedGlyphMetric { w: 10923, h: 9102, d: 0, ic: 0 }, // 0x74
    PackedGlyphMetric { w: 10923, h: 9102, d: 0, ic: 0 }, // 0x75
    PackedGlyphMetric { w: 12743, h: 10420, d: 2228, ic: 0 }, // 0x76
    PackedGlyphMetric { w: 12743, h: 10420, d: 2228, ic: 0 }, // 0x77
    PackedGlyphMetric { w: 7282, h: 11378, d: 3186, ic: 0 }, // 0x78
    PackedGlyphMetric { w: 7282, h: 11378, d: 3186, ic: 0 }, // 0x79
    PackedGlyphMetric { w: 7282, h: 11378, d: 3186, ic: 0 }, // 0x7A
    PackedGlyphMetric { w: 10012, h: 11378, d: 3186, ic: 0 }, // 0x7B
    PackedGlyphMetric { w: 12743, h: 11378, d: 2124, ic: 0 }, // 0x7C
    PackedGlyphMetric { w: 12743, h: 11378, d: 2124, ic: 0 }, // 0x7D
    PackedGlyphMetric { w: 12743, h: 11378, d: 2124, ic: 0 }, // 0x7E
    PackedGlyphMetric { w: 12743, h: 11378, d: 2124, ic: 0 }, // 0x7F
];
/// CMEX10 (Math Extensions, Family 3) metric table
pub static CMEX10_METRICS: [PackedGlyphMetric; 128] = [
    PackedGlyphMetric { w: 7509, h: 655, d: 19006, ic: 0 }, // 0x00
    PackedGlyphMetric { w: 7509, h: 655, d: 19006, ic: 0 }, // 0x01
    PackedGlyphMetric { w: 6827, h: 655, d: 19006, ic: 0 }, // 0x02
    PackedGlyphMetric { w: 6827, h: 655, d: 19006, ic: 0 }, // 0x03
    PackedGlyphMetric { w: 7737, h: 655, d: 19006, ic: 0 }, // 0x04
    PackedGlyphMetric { w: 7737, h: 655, d: 19006, ic: 0 }, // 0x05
    PackedGlyphMetric { w: 7737, h: 655, d: 19006, ic: 0 }, // 0x06
    PackedGlyphMetric { w: 7737, h: 655, d: 19006, ic: 0 }, // 0x07
    PackedGlyphMetric { w: 9557, h: 655, d: 19006, ic: 0 }, // 0x08
    PackedGlyphMetric { w: 9557, h: 655, d: 19006, ic: 0 }, // 0x09
    PackedGlyphMetric { w: 7737, h: 655, d: 19006, ic: 0 }, // 0x0A
    PackedGlyphMetric { w: 7737, h: 655, d: 19006, ic: 0 }, // 0x0B
    PackedGlyphMetric { w: 5461, h: 0, d: 9830, ic: 0 }, // 0x0C
    PackedGlyphMetric { w: 9102, h: 0, d: 9830, ic: 0 }, // 0x0D
    PackedGlyphMetric { w: 9466, h: 655, d: 19006, ic: 0 }, // 0x0E
    PackedGlyphMetric { w: 9466, h: 655, d: 19006, ic: 0 }, // 0x0F
    PackedGlyphMetric { w: 9785, h: 655, d: 28836, ic: 0 }, // 0x10
    PackedGlyphMetric { w: 9785, h: 655, d: 28836, ic: 0 }, // 0x11
    PackedGlyphMetric { w: 12060, h: 655, d: 38667, ic: 0 }, // 0x12
    PackedGlyphMetric { w: 12060, h: 655, d: 38667, ic: 0 }, // 0x13
    PackedGlyphMetric { w: 8647, h: 655, d: 38667, ic: 0 }, // 0x14
    PackedGlyphMetric { w: 8647, h: 655, d: 38667, ic: 0 }, // 0x15
    PackedGlyphMetric { w: 9557, h: 655, d: 38667, ic: 0 }, // 0x16
    PackedGlyphMetric { w: 9557, h: 655, d: 38667, ic: 0 }, // 0x17
    PackedGlyphMetric { w: 9557, h: 655, d: 38667, ic: 0 }, // 0x18
    PackedGlyphMetric { w: 9557, h: 655, d: 38667, ic: 0 }, // 0x19
    PackedGlyphMetric { w: 12288, h: 655, d: 38667, ic: 0 }, // 0x1A
    PackedGlyphMetric { w: 12288, h: 655, d: 38667, ic: 0 }, // 0x1B
    PackedGlyphMetric { w: 12288, h: 655, d: 38667, ic: 0 }, // 0x1C
    PackedGlyphMetric { w: 12288, h: 655, d: 38667, ic: 0 }, // 0x1D
    PackedGlyphMetric { w: 17112, h: 655, d: 38667, ic: 0 }, // 0x1E
    PackedGlyphMetric { w: 17112, h: 655, d: 38667, ic: 0 }, // 0x1F
    PackedGlyphMetric { w: 12971, h: 655, d: 48497, ic: 0 }, // 0x20
    PackedGlyphMetric { w: 12971, h: 655, d: 48497, ic: 0 }, // 0x21
    PackedGlyphMetric { w: 9557, h: 655, d: 48497, ic: 0 }, // 0x22
    PackedGlyphMetric { w: 9557, h: 655, d: 48497, ic: 0 }, // 0x23
    PackedGlyphMetric { w: 10468, h: 655, d: 48497, ic: 0 }, // 0x24
    PackedGlyphMetric { w: 10468, h: 655, d: 48497, ic: 0 }, // 0x25
    PackedGlyphMetric { w: 10468, h: 655, d: 48497, ic: 0 }, // 0x26
    PackedGlyphMetric { w: 10468, h: 655, d: 48497, ic: 0 }, // 0x27
    PackedGlyphMetric { w: 13198, h: 655, d: 48497, ic: 0 }, // 0x28
    PackedGlyphMetric { w: 13198, h: 655, d: 48497, ic: 0 }, // 0x29
    PackedGlyphMetric { w: 13198, h: 655, d: 48497, ic: 0 }, // 0x2A
    PackedGlyphMetric { w: 13198, h: 655, d: 48497, ic: 0 }, // 0x2B
    PackedGlyphMetric { w: 20935, h: 655, d: 48497, ic: 0 }, // 0x2C
    PackedGlyphMetric { w: 20935, h: 655, d: 48497, ic: 0 }, // 0x2D
    PackedGlyphMetric { w: 13289, h: 655, d: 28836, ic: 0 }, // 0x2E
    PackedGlyphMetric { w: 13289, h: 655, d: 28836, ic: 0 }, // 0x2F
    PackedGlyphMetric { w: 14336, h: 655, d: 28836, ic: 0 }, // 0x30
    PackedGlyphMetric { w: 14336, h: 655, d: 28836, ic: 0 }, // 0x31
    PackedGlyphMetric { w: 10923, h: 655, d: 28836, ic: 0 }, // 0x32
    PackedGlyphMetric { w: 10923, h: 655, d: 28836, ic: 0 }, // 0x33
    PackedGlyphMetric { w: 10923, h: 655, d: 28836, ic: 0 }, // 0x34
    PackedGlyphMetric { w: 10923, h: 655, d: 28836, ic: 0 }, // 0x35
    PackedGlyphMetric { w: 10923, h: 0, d: 9830, ic: 0 }, // 0x36
    PackedGlyphMetric { w: 10923, h: 0, d: 9830, ic: 0 }, // 0x37
    PackedGlyphMetric { w: 14564, h: 0, d: 14746, ic: 0 }, // 0x38
    PackedGlyphMetric { w: 14564, h: 0, d: 14746, ic: 0 }, // 0x39
    PackedGlyphMetric { w: 14564, h: 0, d: 14746, ic: 0 }, // 0x3A
    PackedGlyphMetric { w: 14564, h: 0, d: 14746, ic: 0 }, // 0x3B
    PackedGlyphMetric { w: 14564, h: 0, d: 29492, ic: 0 }, // 0x3C
    PackedGlyphMetric { w: 14564, h: 0, d: 29492, ic: 0 }, // 0x3D
    PackedGlyphMetric { w: 14564, h: 0, d: 4915, ic: 0 }, // 0x3E
    PackedGlyphMetric { w: 10923, h: 0, d: 9830, ic: 0 }, // 0x3F
    PackedGlyphMetric { w: 14336, h: 655, d: 28836, ic: 0 }, // 0x40
    PackedGlyphMetric { w: 14336, h: 655, d: 28836, ic: 0 }, // 0x41
    PackedGlyphMetric { w: 14336, h: 0, d: 9830, ic: 0 }, // 0x42
    PackedGlyphMetric { w: 14336, h: 0, d: 9830, ic: 0 }, // 0x43
    PackedGlyphMetric { w: 10012, h: 655, d: 28836, ic: 0 }, // 0x44
    PackedGlyphMetric { w: 10012, h: 655, d: 28836, ic: 0 }, // 0x45
    PackedGlyphMetric { w: 13653, h: 0, d: 16384, ic: 0 }, // 0x46
    PackedGlyphMetric { w: 18204, h: 1638, d: 24576, ic: 0 }, // 0x47
    PackedGlyphMetric { w: 7737, h: 0, d: 18205, ic: 3186 }, // 0x48
    PackedGlyphMetric { w: 9102, h: 0, d: 36409, ic: 7282 }, // 0x49
    PackedGlyphMetric { w: 18204, h: 0, d: 16384, ic: 0 }, // 0x4A
    PackedGlyphMetric { w: 24758, h: 1638, d: 24576, ic: 0 }, // 0x4B
    PackedGlyphMetric { w: 18204, h: 0, d: 16384, ic: 0 }, // 0x4C
    PackedGlyphMetric { w: 24758, h: 1638, d: 24576, ic: 0 }, // 0x4D
    PackedGlyphMetric { w: 18204, h: 0, d: 16384, ic: 0 }, // 0x4E
    PackedGlyphMetric { w: 24758, h: 1638, d: 24576, ic: 0 }, // 0x4F
    PackedGlyphMetric { w: 17294, h: 0, d: 16384, ic: 0 }, // 0x50
    PackedGlyphMetric { w: 15474, h: 0, d: 16384, ic: 0 }, // 0x51
    PackedGlyphMetric { w: 7737, h: 0, d: 18205, ic: 3186 }, // 0x52
    PackedGlyphMetric { w: 13653, h: 0, d: 16384, ic: 0 }, // 0x53
    PackedGlyphMetric { w: 13653, h: 0, d: 16384, ic: 0 }, // 0x54
    PackedGlyphMetric { w: 13653, h: 0, d: 16384, ic: 0 }, // 0x55
    PackedGlyphMetric { w: 13653, h: 0, d: 16384, ic: 0 }, // 0x56
    PackedGlyphMetric { w: 13653, h: 0, d: 16384, ic: 0 }, // 0x57
    PackedGlyphMetric { w: 23666, h: 1638, d: 24576, ic: 0 }, // 0x58
    PackedGlyphMetric { w: 20935, h: 1638, d: 24576, ic: 0 }, // 0x59
    PackedGlyphMetric { w: 9102, h: 0, d: 36409, ic: 7282 }, // 0x5A
    PackedGlyphMetric { w: 18204, h: 1638, d: 24576, ic: 0 }, // 0x5B
    PackedGlyphMetric { w: 18204, h: 1638, d: 24576, ic: 0 }, // 0x5C
    PackedGlyphMetric { w: 18204, h: 1638, d: 24576, ic: 0 }, // 0x5D
    PackedGlyphMetric { w: 18204, h: 1638, d: 24576, ic: 0 }, // 0x5E
    PackedGlyphMetric { w: 18204, h: 1638, d: 24576, ic: 0 }, // 0x5F
    PackedGlyphMetric { w: 15474, h: 0, d: 16384, ic: 0 }, // 0x60
    PackedGlyphMetric { w: 20935, h: 1638, d: 24576, ic: 0 }, // 0x61
    PackedGlyphMetric { w: 9102, h: 11833, d: 0, ic: 0 }, // 0x62
    PackedGlyphMetric { w: 16384, h: 12288, d: 0, ic: 0 }, // 0x63
    PackedGlyphMetric { w: 23666, h: 12288, d: 0, ic: 0 }, // 0x64
    PackedGlyphMetric { w: 9102, h: 11833, d: 0, ic: 0 }, // 0x65
    PackedGlyphMetric { w: 16384, h: 12288, d: 0, ic: 0 }, // 0x66
    PackedGlyphMetric { w: 23666, h: 12288, d: 0, ic: 0 }, // 0x67
    PackedGlyphMetric { w: 7737, h: 655, d: 28836, ic: 0 }, // 0x68
    PackedGlyphMetric { w: 7737, h: 655, d: 28836, ic: 0 }, // 0x69
    PackedGlyphMetric { w: 8647, h: 655, d: 28836, ic: 0 }, // 0x6A
    PackedGlyphMetric { w: 8647, h: 655, d: 28836, ic: 0 }, // 0x6B
    PackedGlyphMetric { w: 8647, h: 655, d: 28836, ic: 0 }, // 0x6C
    PackedGlyphMetric { w: 8647, h: 655, d: 28836, ic: 0 }, // 0x6D
    PackedGlyphMetric { w: 10923, h: 655, d: 28836, ic: 0 }, // 0x6E
    PackedGlyphMetric { w: 10923, h: 655, d: 28836, ic: 0 }, // 0x6F
    PackedGlyphMetric { w: 16384, h: 655, d: 19006, ic: 0 }, // 0x70
    PackedGlyphMetric { w: 16384, h: 655, d: 28836, ic: 0 }, // 0x71
    PackedGlyphMetric { w: 16384, h: 655, d: 38667, ic: 0 }, // 0x72
    PackedGlyphMetric { w: 16384, h: 655, d: 48497, ic: 0 }, // 0x73
    PackedGlyphMetric { w: 17294, h: 0, d: 29492, ic: 0 }, // 0x74
    PackedGlyphMetric { w: 17294, h: 0, d: 9830, ic: 0 }, // 0x75
    PackedGlyphMetric { w: 17294, h: 655, d: 9175, ic: 0 }, // 0x76
    PackedGlyphMetric { w: 12743, h: 0, d: 9830, ic: 0 }, // 0x77
    PackedGlyphMetric { w: 10923, h: 0, d: 9830, ic: 0 }, // 0x78
    PackedGlyphMetric { w: 10923, h: 0, d: 9830, ic: 0 }, // 0x79
    PackedGlyphMetric { w: 7373, h: 1966, d: 0, ic: 0 }, // 0x7A
    PackedGlyphMetric { w: 7373, h: 1966, d: 0, ic: 0 }, // 0x7B
    PackedGlyphMetric { w: 7373, h: 1966, d: 0, ic: 0 }, // 0x7C
    PackedGlyphMetric { w: 7373, h: 1966, d: 0, ic: 0 }, // 0x7D
    PackedGlyphMetric { w: 12743, h: 0, d: 9830, ic: 0 }, // 0x7E
    PackedGlyphMetric { w: 12743, h: 0, d: 9830, ic: 0 }, // 0x7F
];
/// CMMI10 (Math Italic, Family 1) metric table
pub static CMMI10_METRICS: [PackedGlyphMetric; 128] = [
    PackedGlyphMetric { w: 10081, h: 11196, d: 0, ic: 2276 }, // 0x00
    PackedGlyphMetric { w: 13653, h: 11196, d: 0, ic: 0 }, // 0x01
    PackedGlyphMetric { w: 12497, h: 11196, d: 0, ic: 455 }, // 0x02
    PackedGlyphMetric { w: 11378, h: 11196, d: 0, ic: 0 }, // 0x03
    PackedGlyphMetric { w: 12163, h: 11196, d: 0, ic: 1240 }, // 0x04
    PackedGlyphMetric { w: 13619, h: 11196, d: 0, ic: 1331 }, // 0x05
    PackedGlyphMetric { w: 12777, h: 11196, d: 0, ic: 944 }, // 0x06
    PackedGlyphMetric { w: 9557, h: 11196, d: 0, ic: 2276 }, // 0x07
    PackedGlyphMetric { w: 10923, h: 11196, d: 0, ic: 0 }, // 0x08
    PackedGlyphMetric { w: 10031, h: 11196, d: 0, ic: 1802 }, // 0x09
    PackedGlyphMetric { w: 12655, h: 11196, d: 0, ic: 822 }, // 0x0A
    PackedGlyphMetric { w: 10481, h: 7054, d: 0, ic: 61 }, // 0x0B
    PackedGlyphMetric { w: 9267, h: 11378, d: 3186, ic: 865 }, // 0x0C
    PackedGlyphMetric { w: 8482, h: 7054, d: 3186, ic: 910 }, // 0x0D
    PackedGlyphMetric { w: 7282, h: 11378, d: 0, ic: 620 }, // 0x0E
    PackedGlyphMetric { w: 6650, h: 7054, d: 0, ic: 0 }, // 0x0F
    PackedGlyphMetric { w: 7168, h: 11378, d: 3186, ic: 1209 }, // 0x10
    PackedGlyphMetric { w: 8135, h: 7054, d: 3186, ic: 588 }, // 0x11
    PackedGlyphMetric { w: 7691, h: 11378, d: 0, ic: 455 }, // 0x12
    PackedGlyphMetric { w: 5799, h: 7054, d: 0, ic: 0 }, // 0x13
    PackedGlyphMetric { w: 9440, h: 7054, d: 0, ic: 0 }, // 0x14
    PackedGlyphMetric { w: 9557, h: 11378, d: 0, ic: 0 }, // 0x15
    PackedGlyphMetric { w: 9872, h: 7054, d: 3186, ic: 0 }, // 0x16
    PackedGlyphMetric { w: 8093, h: 7054, d: 0, ic: 1043 }, // 0x17
    PackedGlyphMetric { w: 7168, h: 11378, d: 3186, ic: 754 }, // 0x18
    PackedGlyphMetric { w: 9339, h: 7054, d: 0, ic: 588 }, // 0x19
    PackedGlyphMetric { w: 8471, h: 7054, d: 3186, ic: 0 }, // 0x1A
    PackedGlyphMetric { w: 9362, h: 7054, d: 0, ic: 588 }, // 0x1B
    PackedGlyphMetric { w: 7162, h: 7054, d: 0, ic: 1855 }, // 0x1C
    PackedGlyphMetric { w: 8852, h: 7054, d: 0, ic: 588 }, // 0x1D
    PackedGlyphMetric { w: 9762, h: 11378, d: 3186, ic: 0 }, // 0x1E
    PackedGlyphMetric { w: 10251, h: 7054, d: 3186, ic: 0 }, // 0x1F
    PackedGlyphMetric { w: 10672, h: 11378, d: 3186, ic: 588 }, // 0x20
    PackedGlyphMetric { w: 10198, h: 7054, d: 0, ic: 588 }, // 0x21
    PackedGlyphMetric { w: 7640, h: 7054, d: 0, ic: 0 }, // 0x22
    PackedGlyphMetric { w: 9690, h: 11378, d: 0, ic: 0 }, // 0x23
    PackedGlyphMetric { w: 13568, h: 7054, d: 0, ic: 455 }, // 0x24
    PackedGlyphMetric { w: 8471, h: 7054, d: 3186, ic: 0 }, // 0x25
    PackedGlyphMetric { w: 5945, h: 7054, d: 1593, ic: 1308 }, // 0x26
    PackedGlyphMetric { w: 10718, h: 7054, d: 3186, ic: 0 }, // 0x27
    PackedGlyphMetric { w: 16384, h: 6011, d: 0, ic: 0 }, // 0x28
    PackedGlyphMetric { w: 16384, h: 6011, d: 0, ic: 0 }, // 0x29
    PackedGlyphMetric { w: 16384, h: 6011, d: 0, ic: 0 }, // 0x2A
    PackedGlyphMetric { w: 16384, h: 6011, d: 0, ic: 0 }, // 0x2B
    PackedGlyphMetric { w: 4551, h: 7598, d: 0, ic: 0 }, // 0x2C
    PackedGlyphMetric { w: 4551, h: 7598, d: 0, ic: 0 }, // 0x2D
    PackedGlyphMetric { w: 8192, h: 7623, d: 0, ic: 0 }, // 0x2E
    PackedGlyphMetric { w: 8192, h: 7623, d: 0, ic: 0 }, // 0x2F
    PackedGlyphMetric { w: 8192, h: 7054, d: 0, ic: 0 }, // 0x30
    PackedGlyphMetric { w: 8192, h: 7054, d: 0, ic: 0 }, // 0x31
    PackedGlyphMetric { w: 8192, h: 7054, d: 0, ic: 0 }, // 0x32
    PackedGlyphMetric { w: 8192, h: 7054, d: 3186, ic: 0 }, // 0x33
    PackedGlyphMetric { w: 8192, h: 7054, d: 3186, ic: 0 }, // 0x34
    PackedGlyphMetric { w: 8192, h: 7054, d: 3186, ic: 0 }, // 0x35
    PackedGlyphMetric { w: 8192, h: 10559, d: 0, ic: 0 }, // 0x36
    PackedGlyphMetric { w: 8192, h: 7054, d: 3186, ic: 0 }, // 0x37
    PackedGlyphMetric { w: 8192, h: 10559, d: 0, ic: 0 }, // 0x38
    PackedGlyphMetric { w: 8192, h: 7054, d: 3186, ic: 0 }, // 0x39
    PackedGlyphMetric { w: 4551, h: 1729, d: 0, ic: 0 }, // 0x3A
    PackedGlyphMetric { w: 4551, h: 1729, d: 3186, ic: 0 }, // 0x3B
    PackedGlyphMetric { w: 12743, h: 8833, d: 641, ic: 0 }, // 0x3C
    PackedGlyphMetric { w: 8192, h: 12288, d: 4096, ic: 0 }, // 0x3D
    PackedGlyphMetric { w: 12743, h: 8833, d: 641, ic: 0 }, // 0x3E
    PackedGlyphMetric { w: 8192, h: 7623, d: 0, ic: 0 }, // 0x3F
    PackedGlyphMetric { w: 8698, h: 11378, d: 0, ic: 910 }, // 0x40
    PackedGlyphMetric { w: 12288, h: 11196, d: 0, ic: 0 }, // 0x41
    PackedGlyphMetric { w: 12427, h: 11196, d: 0, ic: 822 }, // 0x42
    PackedGlyphMetric { w: 11710, h: 11196, d: 0, ic: 1172 }, // 0x43
    PackedGlyphMetric { w: 13565, h: 11196, d: 0, ic: 455 }, // 0x44
    PackedGlyphMetric { w: 12095, h: 11196, d: 0, ic: 944 }, // 0x45
    PackedGlyphMetric { w: 10536, h: 11196, d: 0, ic: 2276 }, // 0x46
    PackedGlyphMetric { w: 12882, h: 11196, d: 0, ic: 0 }, // 0x47
    PackedGlyphMetric { w: 13619, h: 11196, d: 0, ic: 1331 }, // 0x48
    PackedGlyphMetric { w: 7202, h: 11196, d: 0, ic: 1286 }, // 0x49
    PackedGlyphMetric { w: 9085, h: 11196, d: 0, ic: 1576 }, // 0x4A
    PackedGlyphMetric { w: 13915, h: 11196, d: 0, ic: 1172 }, // 0x4B
    PackedGlyphMetric { w: 11150, h: 11196, d: 0, ic: 0 }, // 0x4C
    PackedGlyphMetric { w: 15895, h: 11196, d: 0, ic: 1786 }, // 0x4D
    PackedGlyphMetric { w: 13164, h: 11196, d: 0, ic: 1786 }, // 0x4E
    PackedGlyphMetric { w: 12497, h: 11196, d: 0, ic: 455 }, // 0x4F
    PackedGlyphMetric { w: 10519, h: 11196, d: 0, ic: 2276 }, // 0x50
    PackedGlyphMetric { w: 12952, h: 11196, d: 3186, ic: 0 }, // 0x51
    PackedGlyphMetric { w: 12440, h: 11196, d: 0, ic: 127 }, // 0x52
    PackedGlyphMetric { w: 10047, h: 11196, d: 0, ic: 944 }, // 0x53
    PackedGlyphMetric { w: 9574, h: 11196, d: 0, ic: 2276 }, // 0x54
    PackedGlyphMetric { w: 11187, h: 11196, d: 0, ic: 1786 }, // 0x55
    PackedGlyphMetric { w: 9557, h: 11196, d: 0, ic: 3641 }, // 0x56
    PackedGlyphMetric { w: 15474, h: 11196, d: 0, ic: 2276 }, // 0x57
    PackedGlyphMetric { w: 13574, h: 11196, d: 0, ic: 1286 }, // 0x58
    PackedGlyphMetric { w: 9512, h: 11196, d: 0, ic: 3641 }, // 0x59
    PackedGlyphMetric { w: 11184, h: 11196, d: 0, ic: 1172 }, // 0x5A
    PackedGlyphMetric { w: 6372, h: 12288, d: 0, ic: 0 }, // 0x5B
    PackedGlyphMetric { w: 6372, h: 11378, d: 3186, ic: 0 }, // 0x5C
    PackedGlyphMetric { w: 6372, h: 11378, d: 3186, ic: 0 }, // 0x5D
    PackedGlyphMetric { w: 16384, h: 5860, d: 0, ic: 0 }, // 0x5E
    PackedGlyphMetric { w: 16384, h: 5860, d: 0, ic: 0 }, // 0x5F
    PackedGlyphMetric { w: 6827, h: 11378, d: 0, ic: 0 }, // 0x60
    PackedGlyphMetric { w: 8660, h: 7054, d: 0, ic: 0 }, // 0x61
    PackedGlyphMetric { w: 7031, h: 11378, d: 0, ic: 0 }, // 0x62
    PackedGlyphMetric { w: 7090, h: 7054, d: 0, ic: 0 }, // 0x63
    PackedGlyphMetric { w: 8528, h: 11378, d: 0, ic: 0 }, // 0x64
    PackedGlyphMetric { w: 7629, h: 7054, d: 0, ic: 0 }, // 0x65
    PackedGlyphMetric { w: 8021, h: 11378, d: 3186, ic: 1764 }, // 0x66
    PackedGlyphMetric { w: 7815, h: 7054, d: 3186, ic: 588 }, // 0x67
    PackedGlyphMetric { w: 9440, h: 11378, d: 0, ic: 0 }, // 0x68
    PackedGlyphMetric { w: 5644, h: 10806, d: 0, ic: 0 }, // 0x69
    PackedGlyphMetric { w: 6747, h: 10806, d: 3186, ic: 938 }, // 0x6A
    PackedGlyphMetric { w: 8530, h: 11378, d: 0, ic: 516 }, // 0x6B
    PackedGlyphMetric { w: 4889, h: 11378, d: 0, ic: 322 }, // 0x6C
    PackedGlyphMetric { w: 14385, h: 7054, d: 0, ic: 0 }, // 0x6D
    PackedGlyphMetric { w: 9834, h: 7054, d: 0, ic: 0 }, // 0x6E
    PackedGlyphMetric { w: 7942, h: 7054, d: 0, ic: 0 }, // 0x6F
    PackedGlyphMetric { w: 8243, h: 7054, d: 3186, ic: 0 }, // 0x70
    PackedGlyphMetric { w: 7314, h: 7054, d: 3186, ic: 588 }, // 0x71
    PackedGlyphMetric { w: 7392, h: 7054, d: 0, ic: 455 }, // 0x72
    PackedGlyphMetric { w: 7680, h: 7054, d: 0, ic: 0 }, // 0x73
    PackedGlyphMetric { w: 5916, h: 10077, d: 0, ic: 0 }, // 0x74
    PackedGlyphMetric { w: 9379, h: 7054, d: 0, ic: 0 }, // 0x75
    PackedGlyphMetric { w: 7942, h: 7054, d: 0, ic: 588 }, // 0x76
    PackedGlyphMetric { w: 11730, h: 7054, d: 0, ic: 441 }, // 0x77
    PackedGlyphMetric { w: 9364, h: 7054, d: 0, ic: 0 }, // 0x78
    PackedGlyphMetric { w: 8033, h: 7054, d: 3186, ic: 588 }, // 0x79
    PackedGlyphMetric { w: 7619, h: 7054, d: 0, ic: 721 }, // 0x7A
    PackedGlyphMetric { w: 5283, h: 7054, d: 0, ic: 0 }, // 0x7B
    PackedGlyphMetric { w: 6292, h: 7054, d: 3186, ic: 0 }, // 0x7C
    PackedGlyphMetric { w: 10428, h: 7054, d: 3186, ic: 0 }, // 0x7D
    PackedGlyphMetric { w: 8192, h: 11705, d: 0, ic: 2520 }, // 0x7E
    PackedGlyphMetric { w: 4551, h: 11378, d: 0, ic: 6545 }, // 0x7F
];

/// CMEX radical variant heights and depths in 1/1000 design units.
pub static RADICAL_VARIANTS_CMEX: [(u16, u16); 4] = [
    (40, 1160), // 0x70 (small)
    (40, 1760), // 0x71 (large 1)
    (40, 2360), // 0x72 (large 2)
    (40, 2960), // 0x73 (large 3)
];

/// CMEX extensible radical assembly recipe.
pub static RADICAL_RECIPE_CMEX: DelimiterRecipe = DelimiterRecipe {
    top: 0x76,
    mid: 0,
    bot: 0x74,
    rep: 0x75,
};

/// O(1) CMSY10 glyph metric lookup.
#[inline(always)]
pub fn cmsy10_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMSY10_METRICS[code as usize].unpack()
    } else {
        GlyphMetric { w: 0.0, h: 0.0, d: 0.0, ic: 0.0 }
    }
}

/// O(1) CMEX10 glyph metric lookup.
#[inline(always)]
pub fn cmex10_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMEX10_METRICS[code as usize].unpack()
    } else {
        GlyphMetric { w: 0.0, h: 0.0, d: 0.0, ic: 0.0 }
    }
}

/// O(1) CMMI10 glyph metric lookup.
#[inline(always)]
pub fn cmmi10_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMMI10_METRICS[code as usize].unpack()
    } else {
        GlyphMetric { w: 0.0, h: 0.0, d: 0.0, ic: 0.0 }
    }
}

/// Canonical TeX skew kerns for Family 1 (CMMI10 / MathItalic) against skewchar '177.
/// Packed into a 256-byte static array in .rodata (fits entirely in 4 L1 cache lines).
/// Values are in 1/1000 design size units.
pub static CMMI10_SKEW_KERNS: [i16; 128] = [
      83,  167,   83,  167,   83,   56,   83,   56, // 0..7
      83,   56,   83,   28,   83,    0,   56,   56, // 8..15
      83,   56,   83,   56,    0,    0,   28,   28, // 16..23
     111,    0,   83,    0,   28,   28,   83,   56, // 24..31
     111,    0,   83,   83,    0,   83,   83,   83, // 32..39
       0,    0,    0,    0,    0,    0,    0,    0, // 40..47
       0,    0,    0,    0,    0,    0,    0,    0, // 48..55
       0,    0,    0,    0,    0,    0,    0,    0, // 56..63
      83,  139,   83,   83,   56,   83,   83,   83, // 64..71
      56,  111,  167,   56,   28,   83,   83,   83, // 72..79
      83,   83,   83,   83,   83,   28,    0,    0, // 80..87
      83,    0,   83,    0,    0,    0,    0,    0, // 88..95
     111,    0,    0,   56,  167,   56,  167,   28, // 96..103
     -28,    0,    0,    0,   83,    0,    0,   56, // 104..111
      83,   83,   56,   56,   83,   28,   28,   83, // 112..119
      28,   56,   56,   28,   83,  111,    0,    0, // 120..127
];

const _: () = assert!(core::mem::size_of_val(&CMMI10_SKEW_KERNS) == 256);

/// Returns the horizontal skew offset for an italic math character in design units (scaled by font_size).
#[inline(always)]
pub fn cmmi_skew_kern(ch: char) -> f32 {
    let code = ch as u32;
    if code < 128 {
        CMMI10_SKEW_KERNS[code as usize] as f32 * 0.001
    } else {
        0.0
    }
}

/// Returns the pairwise letter kern between two CMMI characters in points.
#[inline(always)]
pub fn cmmi_pair_kern(c1: char, c2: char, tier: u8, font_size: f32) -> f32 {
    match (c1, c2) {
        ('d', 'f') => {
            let k = match tier {
                0 | 1 => -0.166667,
                2 => -0.184525,
                _ => -0.208338,
            };
            k * font_size
        }
        ('d', 'j') => {
            let k = match tier {
                0 | 1 => -0.111112,
                2 => -0.123016,
                _ => -0.138891,
            };
            k * font_size
        }
        ('d', 'Y') => {
            let k = match tier {
                0 | 1 => 0.055555,
                2 => 0.061508,
                _ => 0.069446,
            };
            k * font_size
        }
        ('d', 'Z') => {
            let k = match tier {
                0 | 1 => -0.055555,
                2 => -0.061508,
                _ => -0.069446,
            };
            k * font_size
        }
        _ => 0.0,
    }
}

pub static CMBX10_METRICS: [PackedGlyphMetric; 128] = [
    PackedGlyphMetric { w: 11332, h: 11241, d: 0, ic: 0 }, // 0x00
    PackedGlyphMetric { w: 15701, h: 11241, d: 0, ic: 0 }, // 0x01
    PackedGlyphMetric { w: 14654, h: 11241, d: 0, ic: 0 }, // 0x02
    PackedGlyphMetric { w: 13198, h: 11241, d: 0, ic: 0 }, // 0x03
    PackedGlyphMetric { w: 12561, h: 11241, d: 0, ic: 0 }, // 0x04
    PackedGlyphMetric { w: 14746, h: 11241, d: 0, ic: 0 }, // 0x05
    PackedGlyphMetric { w: 13608, h: 11241, d: 0, ic: 0 }, // 0x06
    PackedGlyphMetric { w: 14654, h: 11241, d: 0, ic: 0 }, // 0x07
    PackedGlyphMetric { w: 13608, h: 11241, d: 0, ic: 0 }, // 0x08
    PackedGlyphMetric { w: 14654, h: 11241, d: 0, ic: 0 }, // 0x09
    PackedGlyphMetric { w: 13608, h: 11241, d: 0, ic: 0 }, // 0x0A
    PackedGlyphMetric { w: 10991, h: 11378, d: 0, ic: 1786 }, // 0x0B
    PackedGlyphMetric { w: 10468, h: 11378, d: 0, ic: 0 }, // 0x0C
    PackedGlyphMetric { w: 10468, h: 11378, d: 0, ic: 0 }, // 0x0D
    PackedGlyphMetric { w: 15701, h: 11378, d: 0, ic: 0 }, // 0x0E
    PackedGlyphMetric { w: 15701, h: 11378, d: 0, ic: 0 }, // 0x0F
    PackedGlyphMetric { w: 5234, h: 7282, d: 0, ic: 0 }, // 0x10
    PackedGlyphMetric { w: 5757, h: 7282, d: 3186, ic: 0 }, // 0x11
    PackedGlyphMetric { w: 9421, h: 11378, d: 0, ic: 0 }, // 0x12
    PackedGlyphMetric { w: 9421, h: 11378, d: 0, ic: 0 }, // 0x13
    PackedGlyphMetric { w: 9421, h: 10354, d: 0, ic: 0 }, // 0x14
    PackedGlyphMetric { w: 9421, h: 11378, d: 0, ic: 0 }, // 0x15
    PackedGlyphMetric { w: 9421, h: 9767, d: 0, ic: 0 }, // 0x16
    PackedGlyphMetric { w: 14245, h: 11378, d: 0, ic: 0 }, // 0x17
    PackedGlyphMetric { w: 8374, h: 0, d: 2788, ic: 0 }, // 0x18
    PackedGlyphMetric { w: 9785, h: 11378, d: 0, ic: 0 }, // 0x19
    PackedGlyphMetric { w: 13608, h: 7282, d: 0, ic: 0 }, // 0x1A
    PackedGlyphMetric { w: 14654, h: 7282, d: 0, ic: 0 }, // 0x1B
    PackedGlyphMetric { w: 9421, h: 8875, d: 1593, ic: 0 }, // 0x1C
    PackedGlyphMetric { w: 17067, h: 11241, d: 0, ic: 0 }, // 0x1D
    PackedGlyphMetric { w: 19160, h: 11241, d: 0, ic: 0 }, // 0x1E
    PackedGlyphMetric { w: 14654, h: 12038, d: 796, ic: 0 }, // 0x1F
    PackedGlyphMetric { w: 5234, h: 7282, d: 0, ic: 0 }, // 0x20
    PackedGlyphMetric { w: 5734, h: 11378, d: 0, ic: 0 }, // 0x21
    PackedGlyphMetric { w: 9876, h: 11378, d: 0, ic: 0 }, // 0x22
    PackedGlyphMetric { w: 15701, h: 11378, d: 3186, ic: 0 }, // 0x23
    PackedGlyphMetric { w: 9421, h: 12288, d: 910, ic: 0 }, // 0x24
    PackedGlyphMetric { w: 15701, h: 12288, d: 910, ic: 0 }, // 0x25
    PackedGlyphMetric { w: 14654, h: 11378, d: 0, ic: 0 }, // 0x26
    PackedGlyphMetric { w: 5234, h: 11378, d: 0, ic: 0 }, // 0x27
    PackedGlyphMetric { w: 7327, h: 12288, d: 4096, ic: 0 }, // 0x28
    PackedGlyphMetric { w: 7327, h: 12288, d: 4096, ic: 0 }, // 0x29
    PackedGlyphMetric { w: 9421, h: 12288, d: 0, ic: 0 }, // 0x2A
    PackedGlyphMetric { w: 14654, h: 10376, d: 2184, ic: 0 }, // 0x2B
    PackedGlyphMetric { w: 5234, h: 2549, d: 3186, ic: 0 }, // 0x2C
    PackedGlyphMetric { w: 6280, h: 7282, d: 0, ic: 0 }, // 0x2D
    PackedGlyphMetric { w: 5234, h: 2549, d: 0, ic: 0 }, // 0x2E
    PackedGlyphMetric { w: 9421, h: 12288, d: 4096, ic: 0 }, // 0x2F
    PackedGlyphMetric { w: 9421, h: 10559, d: 0, ic: 0 }, // 0x30
    PackedGlyphMetric { w: 9421, h: 10559, d: 0, ic: 0 }, // 0x31
    PackedGlyphMetric { w: 9421, h: 10559, d: 0, ic: 0 }, // 0x32
    PackedGlyphMetric { w: 9421, h: 10559, d: 0, ic: 0 }, // 0x33
    PackedGlyphMetric { w: 9421, h: 10559, d: 0, ic: 0 }, // 0x34
    PackedGlyphMetric { w: 9421, h: 10559, d: 0, ic: 0 }, // 0x35
    PackedGlyphMetric { w: 9421, h: 10559, d: 0, ic: 0 }, // 0x36
    PackedGlyphMetric { w: 9421, h: 10559, d: 0, ic: 0 }, // 0x37
    PackedGlyphMetric { w: 9421, h: 10559, d: 0, ic: 0 }, // 0x38
    PackedGlyphMetric { w: 9421, h: 10559, d: 0, ic: 0 }, // 0x39
    PackedGlyphMetric { w: 5234, h: 7282, d: 0, ic: 0 }, // 0x3A
    PackedGlyphMetric { w: 5234, h: 7282, d: 3186, ic: 0 }, // 0x3B
    PackedGlyphMetric { w: 5734, h: 8192, d: 3186, ic: 0 }, // 0x3C
    PackedGlyphMetric { w: 14654, h: 6408, d: 0, ic: 0 }, // 0x3D
    PackedGlyphMetric { w: 8897, h: 8192, d: 3186, ic: 0 }, // 0x3E
    PackedGlyphMetric { w: 8897, h: 11378, d: 0, ic: 0 }, // 0x3F
    PackedGlyphMetric { w: 14654, h: 11378, d: 0, ic: 0 }, // 0x40
    PackedGlyphMetric { w: 14245, h: 11241, d: 0, ic: 0 }, // 0x41
    PackedGlyphMetric { w: 13403, h: 11241, d: 0, ic: 0 }, // 0x42
    PackedGlyphMetric { w: 13608, h: 11241, d: 0, ic: 0 }, // 0x43
    PackedGlyphMetric { w: 14450, h: 11241, d: 0, ic: 0 }, // 0x44
    PackedGlyphMetric { w: 12379, h: 11241, d: 0, ic: 0 }, // 0x45
    PackedGlyphMetric { w: 11856, h: 11241, d: 0, ic: 0 }, // 0x46
    PackedGlyphMetric { w: 14814, h: 11241, d: 0, ic: 0 }, // 0x47
    PackedGlyphMetric { w: 14746, h: 11241, d: 0, ic: 0 }, // 0x48
    PackedGlyphMetric { w: 7145, h: 11241, d: 0, ic: 0 }, // 0x49
    PackedGlyphMetric { w: 9739, h: 11241, d: 0, ic: 0 }, // 0x4A
    PackedGlyphMetric { w: 14768, h: 11241, d: 0, ic: 0 }, // 0x4B
    PackedGlyphMetric { w: 11332, h: 11241, d: 0, ic: 0 }, // 0x4C
    PackedGlyphMetric { w: 17886, h: 11241, d: 0, ic: 0 }, // 0x4D
    PackedGlyphMetric { w: 14746, h: 11241, d: 0, ic: 0 }, // 0x4E
    PackedGlyphMetric { w: 14154, h: 11241, d: 0, ic: 0 }, // 0x4F
    PackedGlyphMetric { w: 12880, h: 11241, d: 0, ic: 0 }, // 0x50
    PackedGlyphMetric { w: 14154, h: 11241, d: 3186, ic: 0 }, // 0x51
    PackedGlyphMetric { w: 14131, h: 11241, d: 0, ic: 0 }, // 0x52
    PackedGlyphMetric { w: 10468, h: 11241, d: 0, ic: 0 }, // 0x53
    PackedGlyphMetric { w: 13107, h: 11241, d: 0, ic: 0 }, // 0x54
    PackedGlyphMetric { w: 14495, h: 11241, d: 0, ic: 0 }, // 0x55
    PackedGlyphMetric { w: 14245, h: 11241, d: 0, ic: 262 }, // 0x56
    PackedGlyphMetric { w: 19479, h: 11241, d: 0, ic: 262 }, // 0x57
    PackedGlyphMetric { w: 14245, h: 11241, d: 0, ic: 0 }, // 0x58
    PackedGlyphMetric { w: 14245, h: 11241, d: 0, ic: 471 }, // 0x59
    PackedGlyphMetric { w: 11514, h: 11241, d: 0, ic: 0 }, // 0x5A
    PackedGlyphMetric { w: 5234, h: 12288, d: 4096, ic: 0 }, // 0x5B
    PackedGlyphMetric { w: 9876, h: 11378, d: 0, ic: 0 }, // 0x5C
    PackedGlyphMetric { w: 5234, h: 12288, d: 4096, ic: 0 }, // 0x5D
    PackedGlyphMetric { w: 9421, h: 11378, d: 0, ic: 0 }, // 0x5E
    PackedGlyphMetric { w: 5234, h: 11378, d: 0, ic: 0 }, // 0x5F
    PackedGlyphMetric { w: 5234, h: 11378, d: 0, ic: 0 }, // 0x60
    PackedGlyphMetric { w: 9159, h: 7282, d: 0, ic: 0 }, // 0x61
    PackedGlyphMetric { w: 10468, h: 11378, d: 0, ic: 0 }, // 0x62
    PackedGlyphMetric { w: 8374, h: 7282, d: 0, ic: 0 }, // 0x63
    PackedGlyphMetric { w: 10468, h: 11378, d: 0, ic: 0 }, // 0x64
    PackedGlyphMetric { w: 8636, h: 7282, d: 0, ic: 0 }, // 0x65
    PackedGlyphMetric { w: 5757, h: 11378, d: 0, ic: 1786 }, // 0x66
    PackedGlyphMetric { w: 9421, h: 7282, d: 3186, ic: 262 }, // 0x67
    PackedGlyphMetric { w: 10468, h: 11378, d: 0, ic: 0 }, // 0x68
    PackedGlyphMetric { w: 5234, h: 11378, d: 0, ic: 0 }, // 0x69
    PackedGlyphMetric { w: 5757, h: 11378, d: 3186, ic: 0 }, // 0x6A
    PackedGlyphMetric { w: 9944, h: 11378, d: 0, ic: 0 }, // 0x6B
    PackedGlyphMetric { w: 5234, h: 11378, d: 0, ic: 0 }, // 0x6C
    PackedGlyphMetric { w: 15701, h: 7282, d: 0, ic: 0 }, // 0x6D
    PackedGlyphMetric { w: 10468, h: 7282, d: 0, ic: 0 }, // 0x6E
    PackedGlyphMetric { w: 9421, h: 7282, d: 0, ic: 0 }, // 0x6F
    PackedGlyphMetric { w: 10468, h: 7282, d: 3186, ic: 0 }, // 0x70
    PackedGlyphMetric { w: 9944, h: 7282, d: 3186, ic: 0 }, // 0x71
    PackedGlyphMetric { w: 7760, h: 7282, d: 0, ic: 0 }, // 0x72
    PackedGlyphMetric { w: 7432, h: 7282, d: 0, ic: 0 }, // 0x73
    PackedGlyphMetric { w: 7327, h: 10403, d: 0, ic: 0 }, // 0x74
    PackedGlyphMetric { w: 10468, h: 7282, d: 0, ic: 0 }, // 0x75
    PackedGlyphMetric { w: 9944, h: 7282, d: 0, ic: 262 }, // 0x76
    PackedGlyphMetric { w: 13608, h: 7282, d: 0, ic: 262 }, // 0x77
    PackedGlyphMetric { w: 9944, h: 7282, d: 0, ic: 0 }, // 0x78
    PackedGlyphMetric { w: 9944, h: 7282, d: 3186, ic: 262 }, // 0x79
    PackedGlyphMetric { w: 8374, h: 7282, d: 0, ic: 0 }, // 0x7A
    PackedGlyphMetric { w: 9421, h: 7282, d: 0, ic: 523 }, // 0x7B
    PackedGlyphMetric { w: 18842, h: 7282, d: 0, ic: 523 }, // 0x7C
    PackedGlyphMetric { w: 9421, h: 11378, d: 0, ic: 0 }, // 0x7D
    PackedGlyphMetric { w: 9421, h: 11378, d: 0, ic: 0 }, // 0x7E
    PackedGlyphMetric { w: 9421, h: 11378, d: 0, ic: 0 }, // 0x7F
];

pub static CMTI10_METRICS: [PackedGlyphMetric; 128] = [
    PackedGlyphMetric { w: 10276, h: 11196, d: 0, ic: 2180 }, // 0x00
    PackedGlyphMetric { w: 13398, h: 11196, d: 0, ic: 0 }, // 0x01
    PackedGlyphMetric { w: 12561, h: 11196, d: 0, ic: 1541 }, // 0x02
    PackedGlyphMetric { w: 11341, h: 11196, d: 0, ic: 0 }, // 0x03
    PackedGlyphMetric { w: 10886, h: 11196, d: 0, ic: 2506 }, // 0x04
    PackedGlyphMetric { w: 12179, h: 11196, d: 0, ic: 2685 }, // 0x05
    PackedGlyphMetric { w: 11724, h: 11196, d: 0, ic: 1971 }, // 0x06
    PackedGlyphMetric { w: 12561, h: 11196, d: 0, ic: 1820 }, // 0x07
    PackedGlyphMetric { w: 11724, h: 11196, d: 0, ic: 981 }, // 0x08
    PackedGlyphMetric { w: 12561, h: 11196, d: 0, ic: 1820 }, // 0x09
    PackedGlyphMetric { w: 11724, h: 11196, d: 0, ic: 1680 }, // 0x0A
    PackedGlyphMetric { w: 10049, h: 11378, d: 3186, ic: 3472 }, // 0x0B
    PackedGlyphMetric { w: 9211, h: 11378, d: 3186, ic: 1693 }, // 0x0C
    PackedGlyphMetric { w: 9630, h: 11378, d: 3186, ic: 1693 }, // 0x0D
    PackedGlyphMetric { w: 14445, h: 11378, d: 3186, ic: 1693 }, // 0x0E
    PackedGlyphMetric { w: 14654, h: 11378, d: 3186, ic: 1693 }, // 0x0F
    PackedGlyphMetric { w: 5024, h: 7054, d: 0, ic: 1257 }, // 0x10
    PackedGlyphMetric { w: 5443, h: 7054, d: 3186, ic: 612 }, // 0x11
    PackedGlyphMetric { w: 8374, h: 11378, d: 0, ic: 0 }, // 0x12
    PackedGlyphMetric { w: 8374, h: 11378, d: 0, ic: 1588 }, // 0x13
    PackedGlyphMetric { w: 8374, h: 10297, d: 0, ic: 1359 }, // 0x14
    PackedGlyphMetric { w: 8374, h: 11378, d: 0, ic: 1770 }, // 0x15
    PackedGlyphMetric { w: 8374, h: 9202, d: 0, ic: 1693 }, // 0x16
    PackedGlyphMetric { w: 13620, h: 11378, d: 0, ic: 0 }, // 0x17
    PackedGlyphMetric { w: 7537, h: 0, d: 2788, ic: 0 }, // 0x18
    PackedGlyphMetric { w: 8793, h: 11378, d: 3186, ic: 1723 }, // 0x19
    PackedGlyphMetric { w: 11724, h: 7054, d: 0, ic: 1231 }, // 0x1A
    PackedGlyphMetric { w: 11724, h: 7054, d: 0, ic: 1231 }, // 0x1B
    PackedGlyphMetric { w: 8374, h: 8647, d: 1593, ic: 1506 }, // 0x1C
    PackedGlyphMetric { w: 14463, h: 11196, d: 0, ic: 1971 }, // 0x1D
    PackedGlyphMetric { w: 16138, h: 11196, d: 0, ic: 1971 }, // 0x1E
    PackedGlyphMetric { w: 12561, h: 11992, d: 796, ic: 1541 }, // 0x1F
    PackedGlyphMetric { w: 4187, h: 7054, d: 0, ic: 0 }, // 0x20
    PackedGlyphMetric { w: 5024, h: 11378, d: 0, ic: 2034 }, // 0x21
    PackedGlyphMetric { w: 8429, h: 11378, d: 0, ic: 1141 }, // 0x22
    PackedGlyphMetric { w: 13398, h: 11378, d: 3186, ic: 1084 }, // 0x23
    PackedGlyphMetric { w: 12601, h: 11378, d: 0, ic: 0 }, // 0x24
    PackedGlyphMetric { w: 13398, h: 12288, d: 910, ic: 2235 }, // 0x25
    PackedGlyphMetric { w: 12561, h: 11378, d: 0, ic: 1588 }, // 0x26
    PackedGlyphMetric { w: 5024, h: 11378, d: 0, ic: 2034 }, // 0x27
    PackedGlyphMetric { w: 6699, h: 12288, d: 4096, ic: 2653 }, // 0x28
    PackedGlyphMetric { w: 6699, h: 12288, d: 4096, ic: 605 }, // 0x29
    PackedGlyphMetric { w: 8374, h: 12288, d: 0, ic: 2444 }, // 0x2A
    PackedGlyphMetric { w: 12561, h: 9202, d: 928, ic: 605 }, // 0x2B
    PackedGlyphMetric { w: 5024, h: 1729, d: 3186, ic: 0 }, // 0x2C
    PackedGlyphMetric { w: 5862, h: 7054, d: 0, ic: 463 }, // 0x2D
    PackedGlyphMetric { w: 5024, h: 1729, d: 0, ic: 0 }, // 0x2E
    PackedGlyphMetric { w: 8374, h: 12288, d: 4096, ic: 2653 }, // 0x2F
    PackedGlyphMetric { w: 8374, h: 10559, d: 0, ic: 2221 }, // 0x30
    PackedGlyphMetric { w: 8374, h: 10559, d: 0, ic: 2221 }, // 0x31
    PackedGlyphMetric { w: 8374, h: 10559, d: 0, ic: 2221 }, // 0x32
    PackedGlyphMetric { w: 8374, h: 10559, d: 0, ic: 2221 }, // 0x33
    PackedGlyphMetric { w: 8374, h: 10559, d: 3186, ic: 2221 }, // 0x34
    PackedGlyphMetric { w: 8374, h: 10559, d: 0, ic: 2221 }, // 0x35
    PackedGlyphMetric { w: 8374, h: 10559, d: 0, ic: 2221 }, // 0x36
    PackedGlyphMetric { w: 8374, h: 10559, d: 3186, ic: 2221 }, // 0x37
    PackedGlyphMetric { w: 8374, h: 10559, d: 0, ic: 2221 }, // 0x38
    PackedGlyphMetric { w: 8374, h: 10559, d: 0, ic: 2221 }, // 0x39
    PackedGlyphMetric { w: 5024, h: 7054, d: 0, ic: 953 }, // 0x3A
    PackedGlyphMetric { w: 5024, h: 7054, d: 3186, ic: 953 }, // 0x3B
    PackedGlyphMetric { w: 5024, h: 8192, d: 3186, ic: 1238 }, // 0x3C
    PackedGlyphMetric { w: 12561, h: 6011, d: 0, ic: 1084 }, // 0x3D
    PackedGlyphMetric { w: 8374, h: 8192, d: 3186, ic: 0 }, // 0x3E
    PackedGlyphMetric { w: 8374, h: 11378, d: 0, ic: 2007 }, // 0x3F
    PackedGlyphMetric { w: 12561, h: 11378, d: 0, ic: 1572 }, // 0x40
    PackedGlyphMetric { w: 12179, h: 11196, d: 0, ic: 0 }, // 0x41
    PackedGlyphMetric { w: 11532, h: 11196, d: 0, ic: 1680 }, // 0x42
    PackedGlyphMetric { w: 11724, h: 11196, d: 0, ic: 2380 }, // 0x43
    PackedGlyphMetric { w: 12370, h: 11196, d: 0, ic: 1541 }, // 0x44
    PackedGlyphMetric { w: 11114, h: 11196, d: 0, ic: 1971 }, // 0x45
    PackedGlyphMetric { w: 10695, h: 11196, d: 0, ic: 2180 }, // 0x46
    PackedGlyphMetric { w: 12675, h: 11196, d: 0, ic: 1429 }, // 0x47
    PackedGlyphMetric { w: 12179, h: 11196, d: 0, ic: 2685 }, // 0x48
    PackedGlyphMetric { w: 6317, h: 11196, d: 0, ic: 2590 }, // 0x49
    PackedGlyphMetric { w: 8602, h: 11196, d: 0, ic: 2298 }, // 0x4A
    PackedGlyphMetric { w: 12597, h: 11196, d: 0, ic: 2380 }, // 0x4B
    PackedGlyphMetric { w: 10276, h: 11196, d: 0, ic: 0 }, // 0x4C
    PackedGlyphMetric { w: 14691, h: 11196, d: 0, ic: 2685 }, // 0x4D
    PackedGlyphMetric { w: 12179, h: 11196, d: 0, ic: 2685 }, // 0x4E
    PackedGlyphMetric { w: 12561, h: 11196, d: 0, ic: 1541 }, // 0x4F
    PackedGlyphMetric { w: 11114, h: 11196, d: 0, ic: 1680 }, // 0x50
    PackedGlyphMetric { w: 12561, h: 11196, d: 3186, ic: 1541 }, // 0x51
    PackedGlyphMetric { w: 11951, h: 11196, d: 0, ic: 634 }, // 0x52
    PackedGlyphMetric { w: 9211, h: 11196, d: 0, ic: 1962 }, // 0x53
    PackedGlyphMetric { w: 11724, h: 11196, d: 0, ic: 2180 }, // 0x54
    PackedGlyphMetric { w: 12179, h: 11196, d: 0, ic: 2685 }, // 0x55
    PackedGlyphMetric { w: 12179, h: 11196, d: 0, ic: 3008 }, // 0x56
    PackedGlyphMetric { w: 16366, h: 11196, d: 0, ic: 3008 }, // 0x57
    PackedGlyphMetric { w: 12179, h: 11196, d: 0, ic: 2590 }, // 0x58
    PackedGlyphMetric { w: 12179, h: 11196, d: 0, ic: 3176 }, // 0x59
    PackedGlyphMetric { w: 10049, h: 11196, d: 0, ic: 2380 }, // 0x5A
    PackedGlyphMetric { w: 5024, h: 12288, d: 4096, ic: 3072 }, // 0x5B
    PackedGlyphMetric { w: 8429, h: 11378, d: 0, ic: 2761 }, // 0x5C
    PackedGlyphMetric { w: 5024, h: 12288, d: 4096, ic: 1725 }, // 0x5D
    PackedGlyphMetric { w: 8374, h: 11378, d: 0, ic: 1089 }, // 0x5E
    PackedGlyphMetric { w: 5024, h: 10942, d: 0, ic: 1925 }, // 0x5F
    PackedGlyphMetric { w: 5024, h: 11378, d: 0, ic: 2034 }, // 0x60
    PackedGlyphMetric { w: 8374, h: 7054, d: 0, ic: 1257 }, // 0x61
    PackedGlyphMetric { w: 7537, h: 11378, d: 0, ic: 1034 }, // 0x62
    PackedGlyphMetric { w: 7537, h: 7054, d: 0, ic: 926 }, // 0x63
    PackedGlyphMetric { w: 8374, h: 11378, d: 0, ic: 1693 }, // 0x64
    PackedGlyphMetric { w: 7537, h: 7054, d: 0, ic: 1231 }, // 0x65
    PackedGlyphMetric { w: 5024, h: 11378, d: 3186, ic: 3472 }, // 0x66
    PackedGlyphMetric { w: 7537, h: 7054, d: 3186, ic: 1450 }, // 0x67
    PackedGlyphMetric { w: 8374, h: 11378, d: 0, ic: 1257 }, // 0x68
    PackedGlyphMetric { w: 5024, h: 10737, d: 0, ic: 1669 }, // 0x69
    PackedGlyphMetric { w: 5024, h: 10737, d: 3186, ic: 2370 }, // 0x6A
    PackedGlyphMetric { w: 7537, h: 11378, d: 0, ic: 1764 }, // 0x6B
    PackedGlyphMetric { w: 4187, h: 11378, d: 0, ic: 1693 }, // 0x6C
    PackedGlyphMetric { w: 13398, h: 7054, d: 0, ic: 1257 }, // 0x6D
    PackedGlyphMetric { w: 9211, h: 7054, d: 0, ic: 1257 }, // 0x6E
    PackedGlyphMetric { w: 8374, h: 7054, d: 0, ic: 1034 }, // 0x6F
    PackedGlyphMetric { w: 8374, h: 7054, d: 3186, ic: 1034 }, // 0x70
    PackedGlyphMetric { w: 7537, h: 7054, d: 3186, ic: 1450 }, // 0x71
    PackedGlyphMetric { w: 6909, h: 7054, d: 0, ic: 1764 }, // 0x72
    PackedGlyphMetric { w: 6699, h: 7054, d: 0, ic: 1345 }, // 0x73
    PackedGlyphMetric { w: 5443, h: 10077, d: 0, ic: 1554 }, // 0x74
    PackedGlyphMetric { w: 8793, h: 7054, d: 0, ic: 1257 }, // 0x75
    PackedGlyphMetric { w: 7537, h: 7054, d: 0, ic: 1764 }, // 0x76
    PackedGlyphMetric { w: 10886, h: 7054, d: 0, ic: 1764 }, // 0x77
    PackedGlyphMetric { w: 7600, h: 7054, d: 0, ic: 1973 }, // 0x78
    PackedGlyphMetric { w: 7955, h: 7054, d: 3186, ic: 1450 }, // 0x79
    PackedGlyphMetric { w: 6699, h: 7054, d: 0, ic: 2014 }, // 0x7A
    PackedGlyphMetric { w: 8374, h: 7054, d: 0, ic: 1509 }, // 0x7B
    PackedGlyphMetric { w: 16748, h: 7054, d: 0, ic: 1509 }, // 0x7C
    PackedGlyphMetric { w: 8374, h: 11378, d: 0, ic: 2007 }, // 0x7D
    PackedGlyphMetric { w: 8374, h: 10942, d: 0, ic: 1898 }, // 0x7E
    PackedGlyphMetric { w: 8374, h: 10942, d: 0, ic: 1716 }, // 0x7F
];



/// O(1) CMBX10 glyph metric lookup.
#[inline(always)]
pub fn cmbx10_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMBX10_METRICS[code as usize].unpack()
    } else {
        GlyphMetric { w: 0.0, h: 0.0, d: 0.0, ic: 0.0 }
    }
}

/// O(1) CMTI10 glyph metric lookup.
#[inline(always)]
pub fn cmti10_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMTI10_METRICS[code as usize].unpack()
    } else {
        GlyphMetric { w: 0.0, h: 0.0, d: 0.0, ic: 0.0 }
    }
}


pub static CMR10_METRICS: [PackedGlyphMetric; 128] = [
    PackedGlyphMetric { w: 10240, h: 11196, d: 0, ic: 0 }, // 0x00
    PackedGlyphMetric { w: 13653, h: 11196, d: 0, ic: 0 }, // 0x01
    PackedGlyphMetric { w: 12743, h: 11196, d: 0, ic: 0 }, // 0x02
    PackedGlyphMetric { w: 11378, h: 11196, d: 0, ic: 0 }, // 0x03
    PackedGlyphMetric { w: 10923, h: 11196, d: 0, ic: 0 }, // 0x04
    PackedGlyphMetric { w: 12288, h: 11196, d: 0, ic: 0 }, // 0x05
    PackedGlyphMetric { w: 11833, h: 11196, d: 0, ic: 0 }, // 0x06
    PackedGlyphMetric { w: 12743, h: 11196, d: 0, ic: 0 }, // 0x07
    PackedGlyphMetric { w: 11833, h: 11196, d: 0, ic: 0 }, // 0x08
    PackedGlyphMetric { w: 12743, h: 11196, d: 0, ic: 0 }, // 0x09
    PackedGlyphMetric { w: 11833, h: 11196, d: 0, ic: 0 }, // 0x0A
    PackedGlyphMetric { w: 9557, h: 11378, d: 0, ic: 1274 }, // 0x0B
    PackedGlyphMetric { w: 9102, h: 11378, d: 0, ic: 0 }, // 0x0C
    PackedGlyphMetric { w: 9102, h: 11378, d: 0, ic: 0 }, // 0x0D
    PackedGlyphMetric { w: 13653, h: 11378, d: 0, ic: 0 }, // 0x0E
    PackedGlyphMetric { w: 13653, h: 11378, d: 0, ic: 0 }, // 0x0F
    PackedGlyphMetric { w: 4551, h: 7054, d: 0, ic: 0 }, // 0x10
    PackedGlyphMetric { w: 5006, h: 7054, d: 3186, ic: 0 }, // 0x11
    PackedGlyphMetric { w: 8192, h: 11378, d: 0, ic: 0 }, // 0x12
    PackedGlyphMetric { w: 8192, h: 11378, d: 0, ic: 0 }, // 0x13
    PackedGlyphMetric { w: 8192, h: 10297, d: 0, ic: 0 }, // 0x14
    PackedGlyphMetric { w: 8192, h: 11378, d: 0, ic: 0 }, // 0x15
    PackedGlyphMetric { w: 8192, h: 9302, d: 0, ic: 0 }, // 0x16
    PackedGlyphMetric { w: 12288, h: 11378, d: 0, ic: 0 }, // 0x17
    PackedGlyphMetric { w: 7282, h: 0, d: 2788, ic: 0 }, // 0x18
    PackedGlyphMetric { w: 8192, h: 11378, d: 0, ic: 0 }, // 0x19
    PackedGlyphMetric { w: 11833, h: 7054, d: 0, ic: 0 }, // 0x1A
    PackedGlyphMetric { w: 12743, h: 7054, d: 0, ic: 0 }, // 0x1B
    PackedGlyphMetric { w: 8192, h: 8647, d: 1593, ic: 0 }, // 0x1C
    PackedGlyphMetric { w: 14791, h: 11196, d: 0, ic: 0 }, // 0x1D
    PackedGlyphMetric { w: 16612, h: 11196, d: 0, ic: 0 }, // 0x1E
    PackedGlyphMetric { w: 12743, h: 11992, d: 796, ic: 0 }, // 0x1F
    PackedGlyphMetric { w: 4551, h: 7054, d: 0, ic: 0 }, // 0x20
    PackedGlyphMetric { w: 4551, h: 11378, d: 0, ic: 0 }, // 0x21
    PackedGlyphMetric { w: 8192, h: 11378, d: 0, ic: 0 }, // 0x22
    PackedGlyphMetric { w: 13653, h: 11378, d: 3186, ic: 0 }, // 0x23
    PackedGlyphMetric { w: 8192, h: 12288, d: 910, ic: 0 }, // 0x24
    PackedGlyphMetric { w: 13653, h: 12288, d: 910, ic: 0 }, // 0x25
    PackedGlyphMetric { w: 12743, h: 11378, d: 0, ic: 0 }, // 0x26
    PackedGlyphMetric { w: 4551, h: 11378, d: 0, ic: 0 }, // 0x27
    PackedGlyphMetric { w: 6372, h: 12288, d: 4096, ic: 0 }, // 0x28
    PackedGlyphMetric { w: 6372, h: 12288, d: 4096, ic: 0 }, // 0x29
    PackedGlyphMetric { w: 8192, h: 12288, d: 0, ic: 0 }, // 0x2A
    PackedGlyphMetric { w: 12743, h: 9557, d: 1365, ic: 0 }, // 0x2B
    PackedGlyphMetric { w: 4551, h: 1729, d: 3186, ic: 0 }, // 0x2C
    PackedGlyphMetric { w: 5461, h: 7054, d: 0, ic: 0 }, // 0x2D
    PackedGlyphMetric { w: 4551, h: 1729, d: 0, ic: 0 }, // 0x2E
    PackedGlyphMetric { w: 8192, h: 12288, d: 4096, ic: 0 }, // 0x2F
    PackedGlyphMetric { w: 8192, h: 10559, d: 0, ic: 0 }, // 0x30
    PackedGlyphMetric { w: 8192, h: 10559, d: 0, ic: 0 }, // 0x31
    PackedGlyphMetric { w: 8192, h: 10559, d: 0, ic: 0 }, // 0x32
    PackedGlyphMetric { w: 8192, h: 10559, d: 0, ic: 0 }, // 0x33
    PackedGlyphMetric { w: 8192, h: 10559, d: 0, ic: 0 }, // 0x34
    PackedGlyphMetric { w: 8192, h: 10559, d: 0, ic: 0 }, // 0x35
    PackedGlyphMetric { w: 8192, h: 10559, d: 0, ic: 0 }, // 0x36
    PackedGlyphMetric { w: 8192, h: 10559, d: 0, ic: 0 }, // 0x37
    PackedGlyphMetric { w: 8192, h: 10559, d: 0, ic: 0 }, // 0x38
    PackedGlyphMetric { w: 8192, h: 10559, d: 0, ic: 0 }, // 0x39
    PackedGlyphMetric { w: 4551, h: 7054, d: 0, ic: 0 }, // 0x3A
    PackedGlyphMetric { w: 4551, h: 7054, d: 3186, ic: 0 }, // 0x3B
    PackedGlyphMetric { w: 4551, h: 8192, d: 3186, ic: 0 }, // 0x3C
    PackedGlyphMetric { w: 12743, h: 6011, d: 0, ic: 0 }, // 0x3D
    PackedGlyphMetric { w: 7737, h: 8192, d: 3186, ic: 0 }, // 0x3E
    PackedGlyphMetric { w: 7737, h: 11378, d: 0, ic: 0 }, // 0x3F
    PackedGlyphMetric { w: 12743, h: 11378, d: 0, ic: 0 }, // 0x40
    PackedGlyphMetric { w: 12288, h: 11196, d: 0, ic: 0 }, // 0x41
    PackedGlyphMetric { w: 11605, h: 11196, d: 0, ic: 0 }, // 0x42
    PackedGlyphMetric { w: 11833, h: 11196, d: 0, ic: 0 }, // 0x43
    PackedGlyphMetric { w: 12516, h: 11196, d: 0, ic: 0 }, // 0x44
    PackedGlyphMetric { w: 11150, h: 11196, d: 0, ic: 0 }, // 0x45
    PackedGlyphMetric { w: 10695, h: 11196, d: 0, ic: 0 }, // 0x46
    PackedGlyphMetric { w: 12857, h: 11196, d: 0, ic: 0 }, // 0x47
    PackedGlyphMetric { w: 12288, h: 11196, d: 0, ic: 0 }, // 0x48
    PackedGlyphMetric { w: 5916, h: 11196, d: 0, ic: 0 }, // 0x49
    PackedGlyphMetric { w: 8420, h: 11196, d: 0, ic: 0 }, // 0x4A
    PackedGlyphMetric { w: 12743, h: 11196, d: 0, ic: 0 }, // 0x4B
    PackedGlyphMetric { w: 10240, h: 11196, d: 0, ic: 0 }, // 0x4C
    PackedGlyphMetric { w: 15019, h: 11196, d: 0, ic: 0 }, // 0x4D
    PackedGlyphMetric { w: 12288, h: 11196, d: 0, ic: 0 }, // 0x4E
    PackedGlyphMetric { w: 12743, h: 11196, d: 0, ic: 0 }, // 0x4F
    PackedGlyphMetric { w: 11150, h: 11196, d: 0, ic: 0 }, // 0x50
    PackedGlyphMetric { w: 12743, h: 11196, d: 3186, ic: 0 }, // 0x51
    PackedGlyphMetric { w: 12060, h: 11196, d: 0, ic: 0 }, // 0x52
    PackedGlyphMetric { w: 9102, h: 11196, d: 0, ic: 0 }, // 0x53
    PackedGlyphMetric { w: 11833, h: 11196, d: 0, ic: 0 }, // 0x54
    PackedGlyphMetric { w: 12288, h: 11196, d: 0, ic: 0 }, // 0x55
    PackedGlyphMetric { w: 12288, h: 11196, d: 0, ic: 228 }, // 0x56
    PackedGlyphMetric { w: 16839, h: 11196, d: 0, ic: 228 }, // 0x57
    PackedGlyphMetric { w: 12288, h: 11196, d: 0, ic: 0 }, // 0x58
    PackedGlyphMetric { w: 12288, h: 11196, d: 0, ic: 410 }, // 0x59
    PackedGlyphMetric { w: 10012, h: 11196, d: 0, ic: 0 }, // 0x5A
    PackedGlyphMetric { w: 4551, h: 12288, d: 4096, ic: 0 }, // 0x5B
    PackedGlyphMetric { w: 8192, h: 11378, d: 0, ic: 0 }, // 0x5C
    PackedGlyphMetric { w: 4551, h: 12288, d: 4096, ic: 0 }, // 0x5D
    PackedGlyphMetric { w: 8192, h: 11378, d: 0, ic: 0 }, // 0x5E
    PackedGlyphMetric { w: 4551, h: 10942, d: 0, ic: 0 }, // 0x5F
    PackedGlyphMetric { w: 4551, h: 11378, d: 0, ic: 0 }, // 0x60
    PackedGlyphMetric { w: 8192, h: 7054, d: 0, ic: 0 }, // 0x61
    PackedGlyphMetric { w: 9102, h: 11378, d: 0, ic: 0 }, // 0x62
    PackedGlyphMetric { w: 7282, h: 7054, d: 0, ic: 0 }, // 0x63
    PackedGlyphMetric { w: 9102, h: 11378, d: 0, ic: 0 }, // 0x64
    PackedGlyphMetric { w: 7282, h: 7054, d: 0, ic: 0 }, // 0x65
    PackedGlyphMetric { w: 5006, h: 11378, d: 0, ic: 1274 }, // 0x66
    PackedGlyphMetric { w: 8192, h: 7054, d: 3186, ic: 228 }, // 0x67
    PackedGlyphMetric { w: 9102, h: 11378, d: 0, ic: 0 }, // 0x68
    PackedGlyphMetric { w: 4551, h: 10942, d: 0, ic: 0 }, // 0x69
    PackedGlyphMetric { w: 5006, h: 10942, d: 3186, ic: 0 }, // 0x6A
    PackedGlyphMetric { w: 8647, h: 11378, d: 0, ic: 0 }, // 0x6B
    PackedGlyphMetric { w: 4551, h: 11378, d: 0, ic: 0 }, // 0x6C
    PackedGlyphMetric { w: 13653, h: 7054, d: 0, ic: 0 }, // 0x6D
    PackedGlyphMetric { w: 9102, h: 7054, d: 0, ic: 0 }, // 0x6E
    PackedGlyphMetric { w: 8192, h: 7054, d: 0, ic: 0 }, // 0x6F
    PackedGlyphMetric { w: 9102, h: 7054, d: 3186, ic: 0 }, // 0x70
    PackedGlyphMetric { w: 8647, h: 7054, d: 3186, ic: 0 }, // 0x71
    PackedGlyphMetric { w: 6417, h: 7054, d: 0, ic: 0 }, // 0x72
    PackedGlyphMetric { w: 6463, h: 7054, d: 0, ic: 0 }, // 0x73
    PackedGlyphMetric { w: 6372, h: 10077, d: 0, ic: 0 }, // 0x74
    PackedGlyphMetric { w: 9102, h: 7054, d: 0, ic: 0 }, // 0x75
    PackedGlyphMetric { w: 8647, h: 7054, d: 0, ic: 228 }, // 0x76
    PackedGlyphMetric { w: 11833, h: 7054, d: 0, ic: 228 }, // 0x77
    PackedGlyphMetric { w: 8647, h: 7054, d: 0, ic: 0 }, // 0x78
    PackedGlyphMetric { w: 8647, h: 7054, d: 3186, ic: 228 }, // 0x79
    PackedGlyphMetric { w: 7282, h: 7054, d: 0, ic: 0 }, // 0x7A
    PackedGlyphMetric { w: 8192, h: 7054, d: 0, ic: 455 }, // 0x7B
    PackedGlyphMetric { w: 16384, h: 7054, d: 0, ic: 455 }, // 0x7C
    PackedGlyphMetric { w: 8192, h: 11378, d: 0, ic: 0 }, // 0x7D
    PackedGlyphMetric { w: 8192, h: 10942, d: 0, ic: 0 }, // 0x7E
    PackedGlyphMetric { w: 8192, h: 10942, d: 0, ic: 0 }, // 0x7F
];

pub static CMR7_METRICS: [PackedGlyphMetric; 128] = [
    PackedGlyphMetric { w: 11573, h: 11196, d: 0, ic: 0 }, // 0x00
    PackedGlyphMetric { w: 15376, h: 11196, d: 0, ic: 0 }, // 0x01
    PackedGlyphMetric { w: 14369, h: 11196, d: 0, ic: 0 }, // 0x02
    PackedGlyphMetric { w: 12808, h: 11196, d: 0, ic: 0 }, // 0x03
    PackedGlyphMetric { w: 12353, h: 11196, d: 0, ic: 0 }, // 0x04
    PackedGlyphMetric { w: 13816, h: 11196, d: 0, ic: 0 }, // 0x05
    PackedGlyphMetric { w: 13361, h: 11196, d: 0, ic: 0 }, // 0x06
    PackedGlyphMetric { w: 14369, h: 11196, d: 0, ic: 0 }, // 0x07
    PackedGlyphMetric { w: 13361, h: 11196, d: 0, ic: 0 }, // 0x08
    PackedGlyphMetric { w: 14369, h: 11196, d: 0, ic: 0 }, // 0x09
    PackedGlyphMetric { w: 13361, h: 11196, d: 0, ic: 0 }, // 0x0A
    PackedGlyphMetric { w: 11102, h: 11378, d: 0, ic: 1308 }, // 0x0B
    PackedGlyphMetric { w: 10598, h: 11378, d: 0, ic: 0 }, // 0x0C
    PackedGlyphMetric { w: 10598, h: 11378, d: 0, ic: 0 }, // 0x0D
    PackedGlyphMetric { w: 15896, h: 11378, d: 0, ic: 0 }, // 0x0E
    PackedGlyphMetric { w: 15896, h: 11378, d: 0, ic: 0 }, // 0x0F
    PackedGlyphMetric { w: 5299, h: 7054, d: 0, ic: 0 }, // 0x10
    PackedGlyphMetric { w: 5803, h: 7054, d: 3186, ic: 0 }, // 0x11
    PackedGlyphMetric { w: 9330, h: 11378, d: 0, ic: 0 }, // 0x12
    PackedGlyphMetric { w: 9330, h: 11378, d: 0, ic: 0 }, // 0x13
    PackedGlyphMetric { w: 9330, h: 10297, d: 0, ic: 0 }, // 0x14
    PackedGlyphMetric { w: 9330, h: 11378, d: 0, ic: 0 }, // 0x15
    PackedGlyphMetric { w: 9330, h: 9408, d: 0, ic: 0 }, // 0x16
    PackedGlyphMetric { w: 13816, h: 11378, d: 0, ic: 0 }, // 0x17
    PackedGlyphMetric { w: 8322, h: 0, d: 2788, ic: 0 }, // 0x18
    PackedGlyphMetric { w: 9330, h: 11378, d: 0, ic: 0 }, // 0x19
    PackedGlyphMetric { w: 13361, h: 7054, d: 0, ic: 0 }, // 0x1A
    PackedGlyphMetric { w: 14369, h: 7054, d: 0, ic: 0 }, // 0x1B
    PackedGlyphMetric { w: 9330, h: 8647, d: 1593, ic: 0 }, // 0x1C
    PackedGlyphMetric { w: 16612, h: 11196, d: 0, ic: 0 }, // 0x1D
    PackedGlyphMetric { w: 18627, h: 11196, d: 0, ic: 0 }, // 0x1E
    PackedGlyphMetric { w: 14369, h: 11992, d: 796, ic: 0 }, // 0x1F
    PackedGlyphMetric { w: 5299, h: 7054, d: 0, ic: 0 }, // 0x20
    PackedGlyphMetric { w: 5299, h: 11378, d: 0, ic: 0 }, // 0x21
    PackedGlyphMetric { w: 9330, h: 11378, d: 0, ic: 0 }, // 0x22
    PackedGlyphMetric { w: 15376, h: 11378, d: 3186, ic: 0 }, // 0x23
    PackedGlyphMetric { w: 9330, h: 12288, d: 910, ic: 0 }, // 0x24
    PackedGlyphMetric { w: 15376, h: 12288, d: 910, ic: 0 }, // 0x25
    PackedGlyphMetric { w: 14369, h: 11378, d: 0, ic: 0 }, // 0x26
    PackedGlyphMetric { w: 5299, h: 11378, d: 0, ic: 0 }, // 0x27
    PackedGlyphMetric { w: 7314, h: 12288, d: 4096, ic: 0 }, // 0x28
    PackedGlyphMetric { w: 7314, h: 12288, d: 4096, ic: 0 }, // 0x29
    PackedGlyphMetric { w: 9330, h: 12288, d: 0, ic: 0 }, // 0x2A
    PackedGlyphMetric { w: 14369, h: 10142, d: 1950, ic: 0 }, // 0x2B
    PackedGlyphMetric { w: 5299, h: 1885, d: 3186, ic: 0 }, // 0x2C
    PackedGlyphMetric { w: 6307, h: 7054, d: 0, ic: 0 }, // 0x2D
    PackedGlyphMetric { w: 5299, h: 1885, d: 0, ic: 0 }, // 0x2E
    PackedGlyphMetric { w: 9330, h: 12288, d: 4096, ic: 0 }, // 0x2F
    PackedGlyphMetric { w: 9330, h: 10559, d: 0, ic: 0 }, // 0x30
    PackedGlyphMetric { w: 9330, h: 10559, d: 0, ic: 0 }, // 0x31
    PackedGlyphMetric { w: 9330, h: 10559, d: 0, ic: 0 }, // 0x32
    PackedGlyphMetric { w: 9330, h: 10559, d: 0, ic: 0 }, // 0x33
    PackedGlyphMetric { w: 9330, h: 10559, d: 0, ic: 0 }, // 0x34
    PackedGlyphMetric { w: 9330, h: 10559, d: 0, ic: 0 }, // 0x35
    PackedGlyphMetric { w: 9330, h: 10559, d: 0, ic: 0 }, // 0x36
    PackedGlyphMetric { w: 9330, h: 10559, d: 0, ic: 0 }, // 0x37
    PackedGlyphMetric { w: 9330, h: 10559, d: 0, ic: 0 }, // 0x38
    PackedGlyphMetric { w: 9330, h: 10559, d: 0, ic: 0 }, // 0x39
    PackedGlyphMetric { w: 5299, h: 7054, d: 0, ic: 0 }, // 0x3A
    PackedGlyphMetric { w: 5299, h: 7054, d: 3186, ic: 0 }, // 0x3B
    PackedGlyphMetric { w: 5299, h: 8192, d: 3186, ic: 0 }, // 0x3C
    PackedGlyphMetric { w: 14369, h: 6293, d: 0, ic: 0 }, // 0x3D
    PackedGlyphMetric { w: 8826, h: 8192, d: 3186, ic: 0 }, // 0x3E
    PackedGlyphMetric { w: 8826, h: 11378, d: 0, ic: 0 }, // 0x3F
    PackedGlyphMetric { w: 14369, h: 11378, d: 0, ic: 0 }, // 0x40
    PackedGlyphMetric { w: 13816, h: 11196, d: 0, ic: 0 }, // 0x41
    PackedGlyphMetric { w: 13085, h: 11196, d: 0, ic: 0 }, // 0x42
    PackedGlyphMetric { w: 13361, h: 11196, d: 0, ic: 0 }, // 0x43
    PackedGlyphMetric { w: 14092, h: 11196, d: 0, ic: 0 }, // 0x44
    PackedGlyphMetric { w: 12581, h: 11196, d: 0, ic: 0 }, // 0x45
    PackedGlyphMetric { w: 12077, h: 11196, d: 0, ic: 0 }, // 0x46
    PackedGlyphMetric { w: 14482, h: 11196, d: 0, ic: 0 }, // 0x47
    PackedGlyphMetric { w: 13816, h: 11196, d: 0, ic: 0 }, // 0x48
    PackedGlyphMetric { w: 6762, h: 11196, d: 0, ic: 0 }, // 0x49
    PackedGlyphMetric { w: 9557, h: 11196, d: 0, ic: 0 }, // 0x4A
    PackedGlyphMetric { w: 14320, h: 11196, d: 0, ic: 0 }, // 0x4B
    PackedGlyphMetric { w: 11573, h: 11196, d: 0, ic: 0 }, // 0x4C
    PackedGlyphMetric { w: 16839, h: 11196, d: 0, ic: 0 }, // 0x4D
    PackedGlyphMetric { w: 13816, h: 11196, d: 0, ic: 0 }, // 0x4E
    PackedGlyphMetric { w: 14369, h: 11196, d: 0, ic: 0 }, // 0x4F
    PackedGlyphMetric { w: 12581, h: 11196, d: 0, ic: 0 }, // 0x50
    PackedGlyphMetric { w: 14369, h: 11196, d: 3186, ic: 0 }, // 0x51
    PackedGlyphMetric { w: 13588, h: 11196, d: 0, ic: 0 }, // 0x52
    PackedGlyphMetric { w: 10338, h: 11196, d: 0, ic: 0 }, // 0x53
    PackedGlyphMetric { w: 13361, h: 11196, d: 0, ic: 0 }, // 0x54
    PackedGlyphMetric { w: 13816, h: 11196, d: 0, ic: 0 }, // 0x55
    PackedGlyphMetric { w: 13816, h: 11196, d: 0, ic: 252 }, // 0x56
    PackedGlyphMetric { w: 18855, h: 11196, d: 0, ic: 252 }, // 0x57
    PackedGlyphMetric { w: 13816, h: 11196, d: 0, ic: 0 }, // 0x58
    PackedGlyphMetric { w: 13816, h: 11196, d: 0, ic: 454 }, // 0x59
    PackedGlyphMetric { w: 11345, h: 11196, d: 0, ic: 0 }, // 0x5A
    PackedGlyphMetric { w: 5299, h: 12288, d: 4096, ic: 0 }, // 0x5B
    PackedGlyphMetric { w: 9330, h: 11378, d: 0, ic: 0 }, // 0x5C
    PackedGlyphMetric { w: 5299, h: 12288, d: 4096, ic: 0 }, // 0x5D
    PackedGlyphMetric { w: 9330, h: 11378, d: 0, ic: 0 }, // 0x5E
    PackedGlyphMetric { w: 5299, h: 11020, d: 0, ic: 0 }, // 0x5F
    PackedGlyphMetric { w: 5299, h: 11378, d: 0, ic: 0 }, // 0x60
    PackedGlyphMetric { w: 9330, h: 7054, d: 0, ic: 0 }, // 0x61
    PackedGlyphMetric { w: 10338, h: 11378, d: 0, ic: 0 }, // 0x62
    PackedGlyphMetric { w: 8322, h: 7054, d: 0, ic: 0 }, // 0x63
    PackedGlyphMetric { w: 10338, h: 11378, d: 0, ic: 0 }, // 0x64
    PackedGlyphMetric { w: 8322, h: 7054, d: 0, ic: 0 }, // 0x65
    PackedGlyphMetric { w: 5803, h: 11378, d: 0, ic: 1308 }, // 0x66
    PackedGlyphMetric { w: 9330, h: 7054, d: 3186, ic: 252 }, // 0x67
    PackedGlyphMetric { w: 10338, h: 11378, d: 0, ic: 0 }, // 0x68
    PackedGlyphMetric { w: 5299, h: 11020, d: 0, ic: 0 }, // 0x69
    PackedGlyphMetric { w: 5803, h: 11020, d: 3186, ic: 0 }, // 0x6A
    PackedGlyphMetric { w: 9834, h: 11378, d: 0, ic: 0 }, // 0x6B
    PackedGlyphMetric { w: 5299, h: 11378, d: 0, ic: 0 }, // 0x6C
    PackedGlyphMetric { w: 15376, h: 7054, d: 0, ic: 0 }, // 0x6D
    PackedGlyphMetric { w: 10338, h: 7054, d: 0, ic: 0 }, // 0x6E
    PackedGlyphMetric { w: 9330, h: 7054, d: 0, ic: 0 }, // 0x6F
    PackedGlyphMetric { w: 10338, h: 7054, d: 3186, ic: 0 }, // 0x70
    PackedGlyphMetric { w: 9834, h: 7054, d: 3186, ic: 0 }, // 0x71
    PackedGlyphMetric { w: 7314, h: 7054, d: 0, ic: 0 }, // 0x72
    PackedGlyphMetric { w: 7415, h: 7054, d: 0, ic: 0 }, // 0x73
    PackedGlyphMetric { w: 7314, h: 10077, d: 0, ic: 0 }, // 0x74
    PackedGlyphMetric { w: 10338, h: 7054, d: 0, ic: 0 }, // 0x75
    PackedGlyphMetric { w: 9834, h: 7054, d: 0, ic: 252 }, // 0x76
    PackedGlyphMetric { w: 13361, h: 7054, d: 0, ic: 252 }, // 0x77
    PackedGlyphMetric { w: 9834, h: 7054, d: 0, ic: 0 }, // 0x78
    PackedGlyphMetric { w: 9834, h: 7054, d: 3186, ic: 252 }, // 0x79
    PackedGlyphMetric { w: 8322, h: 7054, d: 0, ic: 0 }, // 0x7A
    PackedGlyphMetric { w: 9330, h: 7054, d: 0, ic: 504 }, // 0x7B
    PackedGlyphMetric { w: 18660, h: 7054, d: 0, ic: 504 }, // 0x7C
    PackedGlyphMetric { w: 9330, h: 11378, d: 0, ic: 0 }, // 0x7D
    PackedGlyphMetric { w: 9330, h: 11020, d: 0, ic: 0 }, // 0x7E
    PackedGlyphMetric { w: 9330, h: 11020, d: 0, ic: 0 }, // 0x7F
];

pub static CMR5_METRICS: [PackedGlyphMetric; 128] = [
    PackedGlyphMetric { w: 13608, h: 11137, d: 0, ic: 0 }, // 0x00
    PackedGlyphMetric { w: 17977, h: 11137, d: 0, ic: 0 }, // 0x01
    PackedGlyphMetric { w: 16839, h: 11137, d: 0, ic: 0 }, // 0x02
    PackedGlyphMetric { w: 14928, h: 11137, d: 0, ic: 0 }, // 0x03
    PackedGlyphMetric { w: 14564, h: 11137, d: 0, ic: 0 }, // 0x04
    PackedGlyphMetric { w: 16066, h: 11137, d: 0, ic: 0 }, // 0x05
    PackedGlyphMetric { w: 15702, h: 11137, d: 0, ic: 0 }, // 0x06
    PackedGlyphMetric { w: 16839, h: 11137, d: 0, ic: 0 }, // 0x07
    PackedGlyphMetric { w: 15702, h: 11137, d: 0, ic: 0 }, // 0x08
    PackedGlyphMetric { w: 16839, h: 11137, d: 0, ic: 0 }, // 0x09
    PackedGlyphMetric { w: 15702, h: 11137, d: 0, ic: 0 }, // 0x0A
    PackedGlyphMetric { w: 11150, h: 11378, d: 3186, ic: 0 }, // 0x0B
    PackedGlyphMetric { w: 11150, h: 11378, d: 3186, ic: 0 }, // 0x0C
    PackedGlyphMetric { w: 6599, h: 11378, d: 0, ic: 0 }, // 0x0D
    PackedGlyphMetric { w: 6599, h: 8192, d: 3186, ic: 0 }, // 0x0E
    PackedGlyphMetric { w: 10582, h: 8192, d: 3186, ic: 0 }, // 0x0F
    PackedGlyphMetric { w: 6599, h: 7054, d: 0, ic: 0 }, // 0x10
    PackedGlyphMetric { w: 7168, h: 7054, d: 3186, ic: 0 }, // 0x11
    PackedGlyphMetric { w: 11150, h: 11378, d: 0, ic: 0 }, // 0x12
    PackedGlyphMetric { w: 11150, h: 11378, d: 0, ic: 0 }, // 0x13
    PackedGlyphMetric { w: 11150, h: 10297, d: 0, ic: 0 }, // 0x14
    PackedGlyphMetric { w: 11150, h: 11378, d: 0, ic: 0 }, // 0x15
    PackedGlyphMetric { w: 11150, h: 9512, d: 0, ic: 0 }, // 0x16
    PackedGlyphMetric { w: 16066, h: 11378, d: 0, ic: 0 }, // 0x17
    PackedGlyphMetric { w: 10013, h: 0, d: 2788, ic: 0 }, // 0x18
    PackedGlyphMetric { w: 11150, h: 11378, d: 0, ic: 0 }, // 0x19
    PackedGlyphMetric { w: 15702, h: 7054, d: 0, ic: 0 }, // 0x1A
    PackedGlyphMetric { w: 16839, h: 7054, d: 0, ic: 0 }, // 0x1B
    PackedGlyphMetric { w: 11150, h: 8647, d: 1593, ic: 0 }, // 0x1C
    PackedGlyphMetric { w: 19297, h: 11137, d: 0, ic: 0 }, // 0x1D
    PackedGlyphMetric { w: 21573, h: 11137, d: 0, ic: 0 }, // 0x1E
    PackedGlyphMetric { w: 16839, h: 11992, d: 796, ic: 0 }, // 0x1F
    PackedGlyphMetric { w: 6599, h: 7054, d: 0, ic: 0 }, // 0x20
    PackedGlyphMetric { w: 6599, h: 11378, d: 0, ic: 0 }, // 0x21
    PackedGlyphMetric { w: 11150, h: 11378, d: 0, ic: 0 }, // 0x22
    PackedGlyphMetric { w: 17977, h: 11378, d: 3186, ic: 0 }, // 0x23
    PackedGlyphMetric { w: 11150, h: 12288, d: 910, ic: 0 }, // 0x24
    PackedGlyphMetric { w: 17977, h: 12288, d: 910, ic: 0 }, // 0x25
    PackedGlyphMetric { w: 16839, h: 11378, d: 0, ic: 0 }, // 0x26
    PackedGlyphMetric { w: 6599, h: 11378, d: 0, ic: 0 }, // 0x27
    PackedGlyphMetric { w: 8875, h: 12288, d: 4096, ic: 0 }, // 0x28
    PackedGlyphMetric { w: 8875, h: 12288, d: 4096, ic: 0 }, // 0x29
    PackedGlyphMetric { w: 11150, h: 12288, d: 0, ic: 0 }, // 0x2A
    PackedGlyphMetric { w: 16839, h: 10923, d: 2731, ic: 0 }, // 0x2B
    PackedGlyphMetric { w: 6599, h: 2002, d: 3186, ic: 0 }, // 0x2C
    PackedGlyphMetric { w: 7737, h: 7054, d: 0, ic: 0 }, // 0x2D
    PackedGlyphMetric { w: 6599, h: 2002, d: 0, ic: 0 }, // 0x2E
    PackedGlyphMetric { w: 11150, h: 12288, d: 4096, ic: 0 }, // 0x2F
    PackedGlyphMetric { w: 11150, h: 10559, d: 0, ic: 0 }, // 0x30
    PackedGlyphMetric { w: 11150, h: 10559, d: 0, ic: 0 }, // 0x31
    PackedGlyphMetric { w: 11150, h: 10559, d: 0, ic: 0 }, // 0x32
    PackedGlyphMetric { w: 11150, h: 10559, d: 0, ic: 0 }, // 0x33
    PackedGlyphMetric { w: 11150, h: 10559, d: 0, ic: 0 }, // 0x34
    PackedGlyphMetric { w: 11150, h: 10559, d: 0, ic: 0 }, // 0x35
    PackedGlyphMetric { w: 11150, h: 10559, d: 0, ic: 0 }, // 0x36
    PackedGlyphMetric { w: 11150, h: 10559, d: 0, ic: 0 }, // 0x37
    PackedGlyphMetric { w: 11150, h: 10559, d: 0, ic: 0 }, // 0x38
    PackedGlyphMetric { w: 11150, h: 10559, d: 0, ic: 0 }, // 0x39
    PackedGlyphMetric { w: 6599, h: 7054, d: 0, ic: 0 }, // 0x3A
    PackedGlyphMetric { w: 6599, h: 7054, d: 3186, ic: 0 }, // 0x3B
    PackedGlyphMetric { w: 16839, h: 9845, d: 1653, ic: 0 }, // 0x3C
    PackedGlyphMetric { w: 16839, h: 6495, d: 0, ic: 0 }, // 0x3D
    PackedGlyphMetric { w: 16839, h: 9845, d: 1653, ic: 0 }, // 0x3E
    PackedGlyphMetric { w: 10582, h: 11378, d: 0, ic: 0 }, // 0x3F
    PackedGlyphMetric { w: 16839, h: 11378, d: 0, ic: 0 }, // 0x40
    PackedGlyphMetric { w: 16066, h: 11137, d: 0, ic: 0 }, // 0x41
    PackedGlyphMetric { w: 15315, h: 11137, d: 0, ic: 0 }, // 0x42
    PackedGlyphMetric { w: 15702, h: 11137, d: 0, ic: 0 }, // 0x43
    PackedGlyphMetric { w: 16453, h: 11137, d: 0, ic: 0 }, // 0x44
    PackedGlyphMetric { w: 14746, h: 11137, d: 0, ic: 0 }, // 0x45
    PackedGlyphMetric { w: 14177, h: 11137, d: 0, ic: 0 }, // 0x46
    PackedGlyphMetric { w: 16930, h: 11137, d: 0, ic: 0 }, // 0x47
    PackedGlyphMetric { w: 16066, h: 11137, d: 0, ic: 0 }, // 0x48
    PackedGlyphMetric { w: 8101, h: 11137, d: 0, ic: 0 }, // 0x49
    PackedGlyphMetric { w: 11332, h: 11137, d: 0, ic: 0 }, // 0x4A
    PackedGlyphMetric { w: 16635, h: 11137, d: 0, ic: 0 }, // 0x4B
    PackedGlyphMetric { w: 13608, h: 11137, d: 0, ic: 0 }, // 0x4C
    PackedGlyphMetric { w: 19479, h: 11137, d: 0, ic: 0 }, // 0x4D
    PackedGlyphMetric { w: 16066, h: 11137, d: 0, ic: 0 }, // 0x4E
    PackedGlyphMetric { w: 16839, h: 11137, d: 0, ic: 0 }, // 0x4F
    PackedGlyphMetric { w: 14746, h: 11137, d: 0, ic: 0 }, // 0x50
    PackedGlyphMetric { w: 16839, h: 11137, d: 3186, ic: 0 }, // 0x51
    PackedGlyphMetric { w: 15884, h: 11137, d: 0, ic: 0 }, // 0x52
    PackedGlyphMetric { w: 12288, h: 11137, d: 0, ic: 0 }, // 0x53
    PackedGlyphMetric { w: 15702, h: 11137, d: 0, ic: 0 }, // 0x54
    PackedGlyphMetric { w: 16066, h: 11137, d: 0, ic: 0 }, // 0x55
    PackedGlyphMetric { w: 16066, h: 11137, d: 0, ic: 284 }, // 0x56
    PackedGlyphMetric { w: 21755, h: 11137, d: 0, ic: 284 }, // 0x57
    PackedGlyphMetric { w: 16066, h: 11137, d: 0, ic: 0 }, // 0x58
    PackedGlyphMetric { w: 16066, h: 11137, d: 0, ic: 512 }, // 0x59
    PackedGlyphMetric { w: 13426, h: 11137, d: 0, ic: 0 }, // 0x5A
    PackedGlyphMetric { w: 6599, h: 12288, d: 4096, ic: 0 }, // 0x5B
    PackedGlyphMetric { w: 11150, h: 11378, d: 0, ic: 0 }, // 0x5C
    PackedGlyphMetric { w: 6599, h: 12288, d: 4096, ic: 0 }, // 0x5D
    PackedGlyphMetric { w: 11150, h: 11378, d: 0, ic: 0 }, // 0x5E
    PackedGlyphMetric { w: 6599, h: 11137, d: 0, ic: 0 }, // 0x5F
    PackedGlyphMetric { w: 6599, h: 11378, d: 0, ic: 0 }, // 0x60
    PackedGlyphMetric { w: 11150, h: 7054, d: 0, ic: 0 }, // 0x61
    PackedGlyphMetric { w: 12288, h: 11378, d: 0, ic: 0 }, // 0x62
    PackedGlyphMetric { w: 10013, h: 7054, d: 0, ic: 0 }, // 0x63
    PackedGlyphMetric { w: 12288, h: 11378, d: 0, ic: 0 }, // 0x64
    PackedGlyphMetric { w: 10013, h: 7054, d: 0, ic: 0 }, // 0x65
    PackedGlyphMetric { w: 7168, h: 11378, d: 0, ic: 1445 }, // 0x66
    PackedGlyphMetric { w: 11150, h: 7054, d: 3186, ic: 284 }, // 0x67
    PackedGlyphMetric { w: 12288, h: 11378, d: 0, ic: 0 }, // 0x68
    PackedGlyphMetric { w: 6599, h: 11137, d: 0, ic: 0 }, // 0x69
    PackedGlyphMetric { w: 7168, h: 11137, d: 3186, ic: 0 }, // 0x6A
    PackedGlyphMetric { w: 11719, h: 11378, d: 0, ic: 0 }, // 0x6B
    PackedGlyphMetric { w: 6599, h: 11378, d: 0, ic: 0 }, // 0x6C
    PackedGlyphMetric { w: 17977, h: 7054, d: 0, ic: 0 }, // 0x6D
    PackedGlyphMetric { w: 12288, h: 7054, d: 0, ic: 0 }, // 0x6E
    PackedGlyphMetric { w: 11150, h: 7054, d: 0, ic: 0 }, // 0x6F
    PackedGlyphMetric { w: 12288, h: 7054, d: 3186, ic: 0 }, // 0x70
    PackedGlyphMetric { w: 11719, h: 7054, d: 3186, ic: 0 }, // 0x71
    PackedGlyphMetric { w: 8875, h: 7054, d: 0, ic: 0 }, // 0x72
    PackedGlyphMetric { w: 8989, h: 7054, d: 0, ic: 0 }, // 0x73
    PackedGlyphMetric { w: 8875, h: 10077, d: 0, ic: 0 }, // 0x74
    PackedGlyphMetric { w: 12288, h: 7054, d: 0, ic: 0 }, // 0x75
    PackedGlyphMetric { w: 11719, h: 7054, d: 0, ic: 284 }, // 0x76
    PackedGlyphMetric { w: 15702, h: 7054, d: 0, ic: 284 }, // 0x77
    PackedGlyphMetric { w: 11719, h: 7054, d: 0, ic: 0 }, // 0x78
    PackedGlyphMetric { w: 11719, h: 7054, d: 3186, ic: 284 }, // 0x79
    PackedGlyphMetric { w: 10013, h: 7054, d: 0, ic: 0 }, // 0x7A
    PackedGlyphMetric { w: 11150, h: 7054, d: 0, ic: 569 }, // 0x7B
    PackedGlyphMetric { w: 22301, h: 7054, d: 0, ic: 569 }, // 0x7C
    PackedGlyphMetric { w: 11150, h: 11378, d: 0, ic: 0 }, // 0x7D
    PackedGlyphMetric { w: 11150, h: 11137, d: 0, ic: 0 }, // 0x7E
    PackedGlyphMetric { w: 11150, h: 11137, d: 0, ic: 0 }, // 0x7F
];

pub static CMMI7_METRICS: [PackedGlyphMetric; 128] = [
    PackedGlyphMetric { w: 11366, h: 11196, d: 0, ic: 2519 }, // 0x00
    PackedGlyphMetric { w: 15636, h: 11196, d: 0, ic: 0 }, // 0x01
    PackedGlyphMetric { w: 14237, h: 11196, d: 0, ic: 504 }, // 0x02
    PackedGlyphMetric { w: 13068, h: 11196, d: 0, ic: 0 }, // 0x03
    PackedGlyphMetric { w: 13836, h: 11196, d: 0, ic: 1223 }, // 0x04
    PackedGlyphMetric { w: 15329, h: 11196, d: 0, ic: 1253 }, // 0x05
    PackedGlyphMetric { w: 14521, h: 11196, d: 0, ic: 900 }, // 0x06
    PackedGlyphMetric { w: 11101, h: 11196, d: 0, ic: 2519 }, // 0x07
    PackedGlyphMetric { w: 12613, h: 11196, d: 0, ic: 0 }, // 0x08
    PackedGlyphMetric { w: 11745, h: 11196, d: 0, ic: 1875 }, // 0x09
    PackedGlyphMetric { w: 14419, h: 11196, d: 0, ic: 798 }, // 0x0A
    PackedGlyphMetric { w: 12168, h: 7054, d: 0, ic: 0 }, // 0x0B
    PackedGlyphMetric { w: 10613, h: 11378, d: 3186, ic: 842 }, // 0x0C
    PackedGlyphMetric { w: 9832, h: 7054, d: 3186, ic: 1008 }, // 0x0D
    PackedGlyphMetric { w: 8507, h: 11378, d: 0, ic: 594 }, // 0x0E
    PackedGlyphMetric { w: 7801, h: 7054, d: 0, ic: 0 }, // 0x0F
    PackedGlyphMetric { w: 8517, h: 11378, d: 3186, ic: 1063 }, // 0x10
    PackedGlyphMetric { w: 9644, h: 7054, d: 3186, ic: 588 }, // 0x11
    PackedGlyphMetric { w: 8915, h: 11378, d: 0, ic: 504 }, // 0x12
    PackedGlyphMetric { w: 6927, h: 7054, d: 0, ic: 0 }, // 0x13
    PackedGlyphMetric { w: 10958, h: 7054, d: 0, ic: 0 }, // 0x14
    PackedGlyphMetric { w: 11102, h: 11378, d: 0, ic: 0 }, // 0x15
    PackedGlyphMetric { w: 11381, h: 7054, d: 3186, ic: 0 }, // 0x16
    PackedGlyphMetric { w: 9384, h: 7054, d: 0, ic: 1092 }, // 0x17
    PackedGlyphMetric { w: 8517, h: 11378, d: 3186, ic: 559 }, // 0x18
    PackedGlyphMetric { w: 10945, h: 7054, d: 0, ic: 588 }, // 0x19
    PackedGlyphMetric { w: 9711, h: 7054, d: 3186, ic: 0 }, // 0x1A
    PackedGlyphMetric { w: 10846, h: 7054, d: 0, ic: 588 }, // 0x1B
    PackedGlyphMetric { w: 8632, h: 7054, d: 0, ic: 1894 }, // 0x1C
    PackedGlyphMetric { w: 10370, h: 7054, d: 0, ic: 588 }, // 0x1D
    PackedGlyphMetric { w: 11254, h: 11378, d: 3186, ic: 0 }, // 0x1E
    PackedGlyphMetric { w: 11695, h: 7054, d: 3186, ic: 0 }, // 0x1F
    PackedGlyphMetric { w: 12386, h: 11378, d: 3186, ic: 588 }, // 0x20
    PackedGlyphMetric { w: 11791, h: 7054, d: 0, ic: 588 }, // 0x21
    PackedGlyphMetric { w: 8843, h: 7054, d: 0, ic: 0 }, // 0x22
    PackedGlyphMetric { w: 11303, h: 11378, d: 0, ic: 0 }, // 0x23
    PackedGlyphMetric { w: 15564, h: 7054, d: 0, ic: 504 }, // 0x24
    PackedGlyphMetric { w: 9711, h: 7054, d: 3186, ic: 0 }, // 0x25
    PackedGlyphMetric { w: 7196, h: 7054, d: 1593, ic: 1260 }, // 0x26
    PackedGlyphMetric { w: 12311, h: 7054, d: 3186, ic: 0 }, // 0x27
    PackedGlyphMetric { w: 18660, h: 6293, d: 0, ic: 0 }, // 0x28
    PackedGlyphMetric { w: 18660, h: 6293, d: 0, ic: 0 }, // 0x29
    PackedGlyphMetric { w: 18660, h: 6293, d: 0, ic: 0 }, // 0x2A
    PackedGlyphMetric { w: 18660, h: 6293, d: 0, ic: 0 }, // 0x2B
    PackedGlyphMetric { w: 5559, h: 8092, d: 0, ic: 0 }, // 0x2C
    PackedGlyphMetric { w: 5559, h: 8092, d: 0, ic: 0 }, // 0x2D
    PackedGlyphMetric { w: 9590, h: 7623, d: 0, ic: 0 }, // 0x2E
    PackedGlyphMetric { w: 9590, h: 7623, d: 0, ic: 0 }, // 0x2F
    PackedGlyphMetric { w: 9590, h: 7054, d: 0, ic: 0 }, // 0x30
    PackedGlyphMetric { w: 9590, h: 7054, d: 0, ic: 0 }, // 0x31
    PackedGlyphMetric { w: 9590, h: 7054, d: 0, ic: 0 }, // 0x32
    PackedGlyphMetric { w: 9590, h: 7054, d: 3186, ic: 0 }, // 0x33
    PackedGlyphMetric { w: 9590, h: 7054, d: 3186, ic: 0 }, // 0x34
    PackedGlyphMetric { w: 9590, h: 7054, d: 3186, ic: 0 }, // 0x35
    PackedGlyphMetric { w: 9590, h: 10559, d: 0, ic: 0 }, // 0x36
    PackedGlyphMetric { w: 9590, h: 7054, d: 3186, ic: 0 }, // 0x37
    PackedGlyphMetric { w: 9590, h: 10559, d: 0, ic: 0 }, // 0x38
    PackedGlyphMetric { w: 9590, h: 7054, d: 3186, ic: 0 }, // 0x39
    PackedGlyphMetric { w: 5559, h: 1885, d: 0, ic: 0 }, // 0x3A
    PackedGlyphMetric { w: 5559, h: 1885, d: 3186, ic: 0 }, // 0x3B
    PackedGlyphMetric { w: 14629, h: 9432, d: 1240, ic: 0 }, // 0x3C
    PackedGlyphMetric { w: 9590, h: 12288, d: 4096, ic: 0 }, // 0x3D
    PackedGlyphMetric { w: 14629, h: 9432, d: 1240, ic: 0 }, // 0x3E
    PackedGlyphMetric { w: 9590, h: 7623, d: 0, ic: 0 }, // 0x3F
    PackedGlyphMetric { w: 9995, h: 11378, d: 0, ic: 1008 }, // 0x40
    PackedGlyphMetric { w: 14076, h: 11196, d: 0, ic: 0 }, // 0x41
    PackedGlyphMetric { w: 14142, h: 11196, d: 0, ic: 798 }, // 0x42
    PackedGlyphMetric { w: 13425, h: 11196, d: 0, ic: 1148 }, // 0x43
    PackedGlyphMetric { w: 15304, h: 11196, d: 0, ic: 504 }, // 0x44
    PackedGlyphMetric { w: 13741, h: 11196, d: 0, ic: 900 }, // 0x45
    PackedGlyphMetric { w: 11870, h: 11196, d: 0, ic: 2519 }, // 0x46
    PackedGlyphMetric { w: 14572, h: 11196, d: 0, ic: 0 }, // 0x47
    PackedGlyphMetric { w: 15329, h: 11196, d: 0, ic: 1253 }, // 0x48
    PackedGlyphMetric { w: 8295, h: 11196, d: 0, ic: 1274 }, // 0x49
    PackedGlyphMetric { w: 10355, h: 11196, d: 0, ic: 1546 }, // 0x4A
    PackedGlyphMetric { w: 15727, h: 11196, d: 0, ic: 1148 }, // 0x4B
    PackedGlyphMetric { w: 12841, h: 11196, d: 0, ic: 0 }, // 0x4C
    PackedGlyphMetric { w: 17849, h: 11196, d: 0, ic: 1757 }, // 0x4D
    PackedGlyphMetric { w: 14825, h: 11196, d: 0, ic: 1757 }, // 0x4E
    PackedGlyphMetric { w: 14237, h: 11196, d: 0, ic: 504 }, // 0x4F
    PackedGlyphMetric { w: 11917, h: 11196, d: 0, ic: 2519 }, // 0x50
    PackedGlyphMetric { w: 14740, h: 11196, d: 3186, ic: 0 }, // 0x51
    PackedGlyphMetric { w: 14100, h: 11196, d: 0, ic: 84 }, // 0x52
    PackedGlyphMetric { w: 11493, h: 11196, d: 0, ic: 896 }, // 0x53
    PackedGlyphMetric { w: 11055, h: 11196, d: 0, ic: 2519 }, // 0x54
    PackedGlyphMetric { w: 12750, h: 11196, d: 0, ic: 1757 }, // 0x55
    PackedGlyphMetric { w: 11053, h: 11196, d: 0, ic: 4031 }, // 0x56
    PackedGlyphMetric { w: 17603, h: 11196, d: 0, ic: 2519 }, // 0x57
    PackedGlyphMetric { w: 15350, h: 11196, d: 0, ic: 1274 }, // 0x58
    PackedGlyphMetric { w: 11002, h: 11196, d: 0, ic: 4031 }, // 0x59
    PackedGlyphMetric { w: 12753, h: 11196, d: 0, ic: 1148 }, // 0x5A
    PackedGlyphMetric { w: 7574, h: 12288, d: 0, ic: 0 }, // 0x5B
    PackedGlyphMetric { w: 7574, h: 11378, d: 3186, ic: 0 }, // 0x5C
    PackedGlyphMetric { w: 7574, h: 11378, d: 3186, ic: 0 }, // 0x5D
    PackedGlyphMetric { w: 18660, h: 5860, d: 0, ic: 0 }, // 0x5E
    PackedGlyphMetric { w: 18660, h: 5860, d: 0, ic: 0 }, // 0x5F
    PackedGlyphMetric { w: 7834, h: 11378, d: 0, ic: 0 }, // 0x60
    PackedGlyphMetric { w: 10153, h: 7054, d: 0, ic: 0 }, // 0x61
    PackedGlyphMetric { w: 8231, h: 11378, d: 0, ic: 0 }, // 0x62
    PackedGlyphMetric { w: 8365, h: 7054, d: 0, ic: 0 }, // 0x63
    PackedGlyphMetric { w: 9744, h: 11378, d: 0, ic: 0 }, // 0x64
    PackedGlyphMetric { w: 8880, h: 7054, d: 0, ic: 0 }, // 0x65
    PackedGlyphMetric { w: 9127, h: 11378, d: 3186, ic: 1837 }, // 0x66
    PackedGlyphMetric { w: 9131, h: 7054, d: 3186, ic: 588 }, // 0x67
    PackedGlyphMetric { w: 10958, h: 11378, d: 0, ic: 0 }, // 0x68
    PackedGlyphMetric { w: 6622, h: 10841, d: 0, ic: 0 }, // 0x69
    PackedGlyphMetric { w: 7745, h: 10841, d: 3186, ic: 947 }, // 0x6A
    PackedGlyphMetric { w: 9950, h: 11378, d: 0, ic: 396 }, // 0x6B
    PackedGlyphMetric { w: 5919, h: 11378, d: 0, ic: 95 }, // 0x6C
    PackedGlyphMetric { w: 16609, h: 7054, d: 0, ic: 0 }, // 0x6D
    PackedGlyphMetric { w: 11570, h: 7054, d: 0, ic: 0 }, // 0x6E
    PackedGlyphMetric { w: 9239, h: 7054, d: 0, ic: 0 }, // 0x6F
    PackedGlyphMetric { w: 9649, h: 7054, d: 3186, ic: 0 }, // 0x70
    PackedGlyphMetric { w: 8579, h: 7054, d: 3186, ic: 588 }, // 0x71
    PackedGlyphMetric { w: 8691, h: 7054, d: 0, ic: 504 }, // 0x72
    PackedGlyphMetric { w: 8834, h: 7054, d: 0, ic: 0 }, // 0x73
    PackedGlyphMetric { w: 7071, h: 10077, d: 0, ic: 0 }, // 0x74
    PackedGlyphMetric { w: 11066, h: 7054, d: 0, ic: 0 }, // 0x75
    PackedGlyphMetric { w: 9362, h: 7054, d: 0, ic: 588 }, // 0x76
    PackedGlyphMetric { w: 13540, h: 7054, d: 0, ic: 441 }, // 0x77
    PackedGlyphMetric { w: 10614, h: 7054, d: 0, ic: 0 }, // 0x78
    PackedGlyphMetric { w: 9492, h: 7054, d: 3186, ic: 588 }, // 0x79
    PackedGlyphMetric { w: 8942, h: 7054, d: 0, ic: 672 }, // 0x7A
    PackedGlyphMetric { w: 6531, h: 7054, d: 0, ic: 0 }, // 0x7B
    PackedGlyphMetric { w: 7241, h: 7054, d: 3186, ic: 0 }, // 0x7C
    PackedGlyphMetric { w: 11962, h: 7054, d: 3186, ic: 0 }, // 0x7D
    PackedGlyphMetric { w: 9590, h: 11776, d: 0, ic: 2520 }, // 0x7E
    PackedGlyphMetric { w: 5559, h: 11378, d: 0, ic: 7575 }, // 0x7F
];

pub static CMMI5_METRICS: [PackedGlyphMetric; 128] = [
    PackedGlyphMetric { w: 13642, h: 11196, d: 0, ic: 2844 }, // 0x00
    PackedGlyphMetric { w: 18887, h: 11196, d: 0, ic: 0 }, // 0x01
    PackedGlyphMetric { w: 17162, h: 11196, d: 0, ic: 569 }, // 0x02
    PackedGlyphMetric { w: 15838, h: 11196, d: 0, ic: 0 }, // 0x03
    PackedGlyphMetric { w: 16674, h: 11196, d: 0, ic: 1200 }, // 0x04
    PackedGlyphMetric { w: 18193, h: 11196, d: 0, ic: 1217 }, // 0x05
    PackedGlyphMetric { w: 17454, h: 11196, d: 0, ic: 842 }, // 0x06
    PackedGlyphMetric { w: 13767, h: 11196, d: 0, ic: 2844 }, // 0x07
    PackedGlyphMetric { w: 15474, h: 11196, d: 0, ic: 0 }, // 0x08
    PackedGlyphMetric { w: 14639, h: 11196, d: 0, ic: 1973 }, // 0x09
    PackedGlyphMetric { w: 17377, h: 11196, d: 0, ic: 765 }, // 0x0A
    PackedGlyphMetric { w: 14964, h: 7054, d: 0, ic: 0 }, // 0x0B
    PackedGlyphMetric { w: 12954, h: 11378, d: 3186, ic: 842 }, // 0x0C
    PackedGlyphMetric { w: 12237, h: 7054, d: 3186, ic: 1138 }, // 0x0D
    PackedGlyphMetric { w: 10718, h: 11378, d: 0, ic: 529 }, // 0x0E
    PackedGlyphMetric { w: 10052, h: 7054, d: 0, ic: 0 }, // 0x0F
    PackedGlyphMetric { w: 10923, h: 11378, d: 3186, ic: 868 }, // 0x10
    PackedGlyphMetric { w: 12186, h: 7054, d: 3186, ic: 588 }, // 0x11
    PackedGlyphMetric { w: 11093, h: 11378, d: 0, ic: 569 }, // 0x12
    PackedGlyphMetric { w: 9008, h: 7054, d: 0, ic: 0 }, // 0x13
    PackedGlyphMetric { w: 13559, h: 7054, d: 0, ic: 0 }, // 0x14
    PackedGlyphMetric { w: 13767, h: 11378, d: 0, ic: 0 }, // 0x15
    PackedGlyphMetric { w: 13923, h: 7054, d: 3186, ic: 0 }, // 0x16
    PackedGlyphMetric { w: 11666, h: 7054, d: 0, ic: 1157 }, // 0x17
    PackedGlyphMetric { w: 10923, h: 11378, d: 3186, ic: 299 }, // 0x18
    PackedGlyphMetric { w: 13618, h: 7054, d: 0, ic: 588 }, // 0x19
    PackedGlyphMetric { w: 11896, h: 7054, d: 3186, ic: 0 }, // 0x1A
    PackedGlyphMetric { w: 13356, h: 7054, d: 0, ic: 588 }, // 0x1B
    PackedGlyphMetric { w: 11167, h: 7054, d: 0, ic: 1900 }, // 0x1C
    PackedGlyphMetric { w: 12971, h: 7054, d: 0, ic: 588 }, // 0x1D
    PackedGlyphMetric { w: 13790, h: 11378, d: 3186, ic: 0 }, // 0x1E
    PackedGlyphMetric { w: 14166, h: 7054, d: 3186, ic: 0 }, // 0x1F
    PackedGlyphMetric { w: 15246, h: 11378, d: 3186, ic: 588 }, // 0x20
    PackedGlyphMetric { w: 14522, h: 7054, d: 0, ic: 588 }, // 0x21
    PackedGlyphMetric { w: 11054, h: 7054, d: 0, ic: 0 }, // 0x22
    PackedGlyphMetric { w: 14014, h: 11378, d: 0, ic: 0 }, // 0x23
    PackedGlyphMetric { w: 18757, h: 7054, d: 0, ic: 569 }, // 0x24
    PackedGlyphMetric { w: 11896, h: 7054, d: 3186, ic: 0 }, // 0x25
    PackedGlyphMetric { w: 9472, h: 7054, d: 1593, ic: 1195 }, // 0x26
    PackedGlyphMetric { w: 15042, h: 7054, d: 3186, ic: 0 }, // 0x27
    PackedGlyphMetric { w: 22301, h: 6495, d: 0, ic: 0 }, // 0x28
    PackedGlyphMetric { w: 22301, h: 6495, d: 0, ic: 0 }, // 0x29
    PackedGlyphMetric { w: 22301, h: 6495, d: 0, ic: 0 }, // 0x2A
    PackedGlyphMetric { w: 22301, h: 6495, d: 0, ic: 0 }, // 0x2B
    PackedGlyphMetric { w: 7509, h: 8435, d: 243, ic: 0 }, // 0x2C
    PackedGlyphMetric { w: 7509, h: 8435, d: 243, ic: 0 }, // 0x2D
    PackedGlyphMetric { w: 12061, h: 7623, d: 0, ic: 0 }, // 0x2E
    PackedGlyphMetric { w: 12061, h: 7623, d: 0, ic: 0 }, // 0x2F
    PackedGlyphMetric { w: 12061, h: 7054, d: 0, ic: 0 }, // 0x30
    PackedGlyphMetric { w: 12061, h: 7054, d: 0, ic: 0 }, // 0x31
    PackedGlyphMetric { w: 12061, h: 7054, d: 0, ic: 0 }, // 0x32
    PackedGlyphMetric { w: 12061, h: 7054, d: 3186, ic: 0 }, // 0x33
    PackedGlyphMetric { w: 12061, h: 7054, d: 3186, ic: 0 }, // 0x34
    PackedGlyphMetric { w: 12061, h: 7054, d: 3186, ic: 0 }, // 0x35
    PackedGlyphMetric { w: 12061, h: 10559, d: 0, ic: 0 }, // 0x36
    PackedGlyphMetric { w: 12061, h: 7054, d: 3186, ic: 0 }, // 0x37
    PackedGlyphMetric { w: 12061, h: 10559, d: 0, ic: 0 }, // 0x38
    PackedGlyphMetric { w: 12061, h: 7054, d: 3186, ic: 0 }, // 0x39
    PackedGlyphMetric { w: 7509, h: 2002, d: 0, ic: 0 }, // 0x3A
    PackedGlyphMetric { w: 7509, h: 2002, d: 3186, ic: 0 }, // 0x3B
    PackedGlyphMetric { w: 17750, h: 9845, d: 1653, ic: 0 }, // 0x3C
    PackedGlyphMetric { w: 12061, h: 12288, d: 4096, ic: 0 }, // 0x3D
    PackedGlyphMetric { w: 17750, h: 9845, d: 1653, ic: 0 }, // 0x3E
    PackedGlyphMetric { w: 12061, h: 7623, d: 0, ic: 0 }, // 0x3F
    PackedGlyphMetric { w: 12271, h: 11378, d: 0, ic: 1138 }, // 0x40
    PackedGlyphMetric { w: 16976, h: 11196, d: 0, ic: 0 }, // 0x41
    PackedGlyphMetric { w: 16990, h: 11196, d: 0, ic: 765 }, // 0x42
    PackedGlyphMetric { w: 16318, h: 11196, d: 0, ic: 1115 }, // 0x43
    PackedGlyphMetric { w: 18184, h: 11196, d: 0, ic: 569 }, // 0x44
    PackedGlyphMetric { w: 16498, h: 11196, d: 0, ic: 842 }, // 0x45
    PackedGlyphMetric { w: 14211, h: 11196, d: 0, ic: 2844 }, // 0x46
    PackedGlyphMetric { w: 17433, h: 11196, d: 0, ic: 0 }, // 0x47
    PackedGlyphMetric { w: 18193, h: 11196, d: 0, ic: 1217 }, // 0x48
    PackedGlyphMetric { w: 10269, h: 11196, d: 0, ic: 1257 }, // 0x49
    PackedGlyphMetric { w: 12664, h: 11196, d: 0, ic: 1559 }, // 0x4A
    PackedGlyphMetric { w: 18660, h: 11196, d: 0, ic: 1115 }, // 0x4B
    PackedGlyphMetric { w: 15656, h: 11196, d: 0, ic: 0 }, // 0x4C
    PackedGlyphMetric { w: 21038, h: 11196, d: 0, ic: 1786 }, // 0x4D
    PackedGlyphMetric { w: 17624, h: 11196, d: 0, ic: 1786 }, // 0x4E
    PackedGlyphMetric { w: 17162, h: 11196, d: 0, ic: 569 }, // 0x4F
    PackedGlyphMetric { w: 14342, h: 11196, d: 0, ic: 2844 }, // 0x50
    PackedGlyphMetric { w: 17731, h: 11196, d: 3186, ic: 0 }, // 0x51
    PackedGlyphMetric { w: 16875, h: 11196, d: 0, ic: 27 }, // 0x52
    PackedGlyphMetric { w: 14029, h: 11196, d: 0, ic: 831 }, // 0x53
    PackedGlyphMetric { w: 13636, h: 11196, d: 0, ic: 2844 }, // 0x54
    PackedGlyphMetric { w: 15465, h: 11196, d: 0, ic: 1786 }, // 0x55
    PackedGlyphMetric { w: 13562, h: 11196, d: 0, ic: 4551 }, // 0x56
    PackedGlyphMetric { w: 20958, h: 11196, d: 0, ic: 2844 }, // 0x57
    PackedGlyphMetric { w: 18233, h: 11196, d: 0, ic: 1257 }, // 0x58
    PackedGlyphMetric { w: 13506, h: 11196, d: 0, ic: 4551 }, // 0x59
    PackedGlyphMetric { w: 15451, h: 11196, d: 0, ic: 1115 }, // 0x5A
    PackedGlyphMetric { w: 9785, h: 12288, d: 0, ic: 0 }, // 0x5B
    PackedGlyphMetric { w: 9785, h: 11378, d: 3186, ic: 0 }, // 0x5C
    PackedGlyphMetric { w: 9785, h: 11378, d: 3186, ic: 0 }, // 0x5D
    PackedGlyphMetric { w: 22301, h: 5860, d: 0, ic: 0 }, // 0x5E
    PackedGlyphMetric { w: 22301, h: 5860, d: 0, ic: 0 }, // 0x5F
    PackedGlyphMetric { w: 9785, h: 11378, d: 0, ic: 0 }, // 0x60
    PackedGlyphMetric { w: 12688, h: 7054, d: 0, ic: 0 }, // 0x61
    PackedGlyphMetric { w: 10377, h: 11378, d: 0, ic: 0 }, // 0x62
    PackedGlyphMetric { w: 10640, h: 7054, d: 0, ic: 0 }, // 0x63
    PackedGlyphMetric { w: 12119, h: 11378, d: 0, ic: 0 }, // 0x64
    PackedGlyphMetric { w: 11092, h: 7054, d: 0, ic: 0 }, // 0x65
    PackedGlyphMetric { w: 11207, h: 11378, d: 3186, ic: 1934 }, // 0x66
    PackedGlyphMetric { w: 11478, h: 7054, d: 3186, ic: 588 }, // 0x67
    PackedGlyphMetric { w: 13559, h: 11378, d: 0, ic: 0 }, // 0x68
    PackedGlyphMetric { w: 8742, h: 10919, d: 0, ic: 0 }, // 0x69
    PackedGlyphMetric { w: 9637, h: 10919, d: 3186, ic: 966 }, // 0x6A
    PackedGlyphMetric { w: 12421, h: 11378, d: 0, ic: 265 }, // 0x6B
    PackedGlyphMetric { w: 7870, h: 11378, d: 0, ic: 0 }, // 0x6C
    PackedGlyphMetric { w: 20120, h: 7054, d: 0, ic: 0 }, // 0x6D
    PackedGlyphMetric { w: 14431, h: 7054, d: 0, ic: 0 }, // 0x6E
    PackedGlyphMetric { w: 11514, h: 7054, d: 0, ic: 0 }, // 0x6F
    PackedGlyphMetric { w: 12119, h: 7054, d: 3186, ic: 0 }, // 0x70
    PackedGlyphMetric { w: 10796, h: 7054, d: 3186, ic: 588 }, // 0x71
    PackedGlyphMetric { w: 10999, h: 7054, d: 0, ic: 569 }, // 0x72
    PackedGlyphMetric { w: 10980, h: 7054, d: 0, ic: 0 }, // 0x73
    PackedGlyphMetric { w: 9235, h: 10077, d: 0, ic: 0 }, // 0x74
    PackedGlyphMetric { w: 13862, h: 7054, d: 0, ic: 0 }, // 0x75
    PackedGlyphMetric { w: 11833, h: 7054, d: 0, ic: 588 }, // 0x76
    PackedGlyphMetric { w: 16531, h: 7054, d: 0, ic: 441 }, // 0x77
    PackedGlyphMetric { w: 12971, h: 7054, d: 0, ic: 0 }, // 0x78
    PackedGlyphMetric { w: 11970, h: 7054, d: 3186, ic: 588 }, // 0x79
    PackedGlyphMetric { w: 11283, h: 7054, d: 0, ic: 607 }, // 0x7A
    PackedGlyphMetric { w: 8742, h: 7054, d: 0, ic: 0 }, // 0x7B
    PackedGlyphMetric { w: 9068, h: 7054, d: 3186, ic: 0 }, // 0x7C
    PackedGlyphMetric { w: 14569, h: 7054, d: 3186, ic: 0 }, // 0x7D
    PackedGlyphMetric { w: 12061, h: 11837, d: 0, ic: 2520 }, // 0x7E
    PackedGlyphMetric { w: 7509, h: 11378, d: 0, ic: 9526 }, // 0x7F
];

pub static CMSY7_METRICS: [PackedGlyphMetric; 128] = [
    PackedGlyphMetric { w: 14629, h: 10142, d: 1950, ic: 0 }, // 0x00
    PackedGlyphMetric { w: 5559, h: 7623, d: 0, ic: 0 }, // 0x01
    PackedGlyphMetric { w: 14629, h: 10142, d: 1950, ic: 0 }, // 0x02
    PackedGlyphMetric { w: 9590, h: 7623, d: 0, ic: 0 }, // 0x03
    PackedGlyphMetric { w: 14629, h: 10142, d: 1950, ic: 0 }, // 0x04
    PackedGlyphMetric { w: 9590, h: 7623, d: 0, ic: 0 }, // 0x05
    PackedGlyphMetric { w: 14629, h: 10142, d: 1950, ic: 0 }, // 0x06
    PackedGlyphMetric { w: 14629, h: 10142, d: 1950, ic: 0 }, // 0x07
    PackedGlyphMetric { w: 14629, h: 10142, d: 1950, ic: 0 }, // 0x08
    PackedGlyphMetric { w: 14629, h: 10142, d: 1950, ic: 0 }, // 0x09
    PackedGlyphMetric { w: 14629, h: 10142, d: 1950, ic: 0 }, // 0x0A
    PackedGlyphMetric { w: 14629, h: 10142, d: 1950, ic: 0 }, // 0x0B
    PackedGlyphMetric { w: 14629, h: 10142, d: 1950, ic: 0 }, // 0x0C
    PackedGlyphMetric { w: 18660, h: 11378, d: 3186, ic: 0 }, // 0x0D
    PackedGlyphMetric { w: 9590, h: 7623, d: 0, ic: 0 }, // 0x0E
    PackedGlyphMetric { w: 9590, h: 7623, d: 0, ic: 0 }, // 0x0F
    PackedGlyphMetric { w: 14629, h: 8092, d: 0, ic: 0 }, // 0x10
    PackedGlyphMetric { w: 14629, h: 8092, d: 0, ic: 0 }, // 0x11
    PackedGlyphMetric { w: 14629, h: 11231, d: 3039, ic: 0 }, // 0x12
    PackedGlyphMetric { w: 14629, h: 11231, d: 3039, ic: 0 }, // 0x13
    PackedGlyphMetric { w: 14629, h: 11231, d: 3039, ic: 0 }, // 0x14
    PackedGlyphMetric { w: 14629, h: 11231, d: 3039, ic: 0 }, // 0x15
    PackedGlyphMetric { w: 14629, h: 11231, d: 3039, ic: 0 }, // 0x16
    PackedGlyphMetric { w: 14629, h: 11231, d: 3039, ic: 0 }, // 0x17
    PackedGlyphMetric { w: 14629, h: 6293, d: 0, ic: 0 }, // 0x18
    PackedGlyphMetric { w: 14629, h: 8451, d: 259, ic: 0 }, // 0x19
    PackedGlyphMetric { w: 14629, h: 9432, d: 1240, ic: 0 }, // 0x1A
    PackedGlyphMetric { w: 14629, h: 9432, d: 1240, ic: 0 }, // 0x1B
    PackedGlyphMetric { w: 18660, h: 9432, d: 1240, ic: 0 }, // 0x1C
    PackedGlyphMetric { w: 18660, h: 9432, d: 1240, ic: 0 }, // 0x1D
    PackedGlyphMetric { w: 14629, h: 9432, d: 1240, ic: 0 }, // 0x1E
    PackedGlyphMetric { w: 14629, h: 9432, d: 1240, ic: 0 }, // 0x1F
    PackedGlyphMetric { w: 18660, h: 6293, d: 0, ic: 0 }, // 0x20
    PackedGlyphMetric { w: 18660, h: 6293, d: 0, ic: 0 }, // 0x21
    PackedGlyphMetric { w: 9590, h: 11378, d: 3186, ic: 0 }, // 0x22
    PackedGlyphMetric { w: 9590, h: 11378, d: 3186, ic: 0 }, // 0x23
    PackedGlyphMetric { w: 18660, h: 6293, d: 0, ic: 0 }, // 0x24
    PackedGlyphMetric { w: 18660, h: 11378, d: 3186, ic: 0 }, // 0x25
    PackedGlyphMetric { w: 18660, h: 11378, d: 3186, ic: 0 }, // 0x26
    PackedGlyphMetric { w: 14629, h: 8092, d: 0, ic: 0 }, // 0x27
    PackedGlyphMetric { w: 18660, h: 6293, d: 0, ic: 0 }, // 0x28
    PackedGlyphMetric { w: 18660, h: 6293, d: 0, ic: 0 }, // 0x29
    PackedGlyphMetric { w: 11605, h: 11378, d: 3186, ic: 0 }, // 0x2A
    PackedGlyphMetric { w: 11605, h: 11378, d: 3186, ic: 0 }, // 0x2B
    PackedGlyphMetric { w: 18660, h: 6293, d: 0, ic: 0 }, // 0x2C
    PackedGlyphMetric { w: 18660, h: 11378, d: 3186, ic: 0 }, // 0x2D
    PackedGlyphMetric { w: 18660, h: 11378, d: 3186, ic: 0 }, // 0x2E
    PackedGlyphMetric { w: 14629, h: 7054, d: 0, ic: 0 }, // 0x2F
    PackedGlyphMetric { w: 5396, h: 9102, d: 0, ic: 0 }, // 0x30
    PackedGlyphMetric { w: 18660, h: 7054, d: 0, ic: 0 }, // 0x31
    PackedGlyphMetric { w: 12613, h: 9432, d: 1240, ic: 0 }, // 0x32
    PackedGlyphMetric { w: 12613, h: 9432, d: 1240, ic: 0 }, // 0x33
    PackedGlyphMetric { w: 16644, h: 11378, d: 3186, ic: 0 }, // 0x34
    PackedGlyphMetric { w: 16644, h: 11378, d: 3186, ic: 0 }, // 0x35
    PackedGlyphMetric { w: 0, h: 11378, d: 3186, ic: 0 }, // 0x36
    PackedGlyphMetric { w: 0, h: 6293, d: 0, ic: 0 }, // 0x37
    PackedGlyphMetric { w: 10598, h: 11378, d: 0, ic: 0 }, // 0x38
    PackedGlyphMetric { w: 10598, h: 11378, d: 0, ic: 0 }, // 0x39
    PackedGlyphMetric { w: 12613, h: 7054, d: 0, ic: 0 }, // 0x3A
    PackedGlyphMetric { w: 9590, h: 12288, d: 910, ic: 0 }, // 0x3B
    PackedGlyphMetric { w: 13621, h: 11378, d: 0, ic: 0 }, // 0x3C
    PackedGlyphMetric { w: 13621, h: 11378, d: 0, ic: 0 }, // 0x3D
    PackedGlyphMetric { w: 14629, h: 11378, d: 0, ic: 0 }, // 0x3E
    PackedGlyphMetric { w: 14629, h: 11378, d: 0, ic: 0 }, // 0x3F
    PackedGlyphMetric { w: 11605, h: 11378, d: 0, ic: 0 }, // 0x40
    PackedGlyphMetric { w: 15034, h: 11196, d: 0, ic: 0 }, // 0x41
    PackedGlyphMetric { w: 12344, h: 11196, d: 0, ic: 402 }, // 0x42
    PackedGlyphMetric { w: 10161, h: 11196, d: 0, ic: 818 }, // 0x43
    PackedGlyphMetric { w: 14573, h: 11196, d: 0, ic: 504 }, // 0x44
    PackedGlyphMetric { w: 10094, h: 11196, d: 0, ic: 1382 }, // 0x45
    PackedGlyphMetric { w: 13408, h: 11196, d: 0, ic: 1651 }, // 0x46
    PackedGlyphMetric { w: 11281, h: 11196, d: 1593, ic: 926 }, // 0x47
    PackedGlyphMetric { w: 16034, h: 11196, d: 0, ic: 0 }, // 0x48
    PackedGlyphMetric { w: 10592, h: 11196, d: 0, ic: 1064 }, // 0x49
    PackedGlyphMetric { w: 12815, h: 11196, d: 1593, ic: 3051 }, // 0x4A
    PackedGlyphMetric { w: 14282, h: 11196, d: 0, ic: 22 }, // 0x4B
    PackedGlyphMetric { w: 12971, h: 11196, d: 0, ic: 0 }, // 0x4C
    PackedGlyphMetric { w: 21999, h: 11196, d: 0, ic: 0 }, // 0x4D
    PackedGlyphMetric { w: 15328, h: 11196, d: 0, ic: 2463 }, // 0x4E
    PackedGlyphMetric { w: 14841, h: 11196, d: 0, ic: 504 }, // 0x4F
    PackedGlyphMetric { w: 13257, h: 11196, d: 0, ic: 1372 }, // 0x50
    PackedGlyphMetric { w: 15334, h: 11196, d: 1593, ic: 0 }, // 0x51
    PackedGlyphMetric { w: 16073, h: 11196, d: 0, ic: 0 }, // 0x52
    PackedGlyphMetric { w: 11505, h: 11196, d: 0, ic: 1180 }, // 0x53
    PackedGlyphMetric { w: 10614, h: 11196, d: 0, ic: 4311 }, // 0x54
    PackedGlyphMetric { w: 11760, h: 11196, d: 0, ic: 1651 }, // 0x55
    PackedGlyphMetric { w: 11795, h: 11196, d: 0, ic: 1372 }, // 0x56
    PackedGlyphMetric { w: 18598, h: 11196, d: 0, ic: 1372 }, // 0x57
    PackedGlyphMetric { w: 13416, h: 11196, d: 0, ic: 2399 }, // 0x58
    PackedGlyphMetric { w: 12523, h: 11196, d: 1593, ic: 1372 }, // 0x59
    PackedGlyphMetric { w: 13486, h: 11196, d: 0, ic: 1321 }, // 0x5A
    PackedGlyphMetric { w: 12613, h: 9102, d: 0, ic: 0 }, // 0x5B
    PackedGlyphMetric { w: 12613, h: 9102, d: 0, ic: 0 }, // 0x5C
    PackedGlyphMetric { w: 12613, h: 9102, d: 0, ic: 0 }, // 0x5D
    PackedGlyphMetric { w: 12613, h: 9102, d: 0, ic: 0 }, // 0x5E
    PackedGlyphMetric { w: 12613, h: 9102, d: 0, ic: 0 }, // 0x5F
    PackedGlyphMetric { w: 11605, h: 11378, d: 0, ic: 0 }, // 0x60
    PackedGlyphMetric { w: 11605, h: 11378, d: 0, ic: 0 }, // 0x61
    PackedGlyphMetric { w: 8582, h: 12288, d: 4096, ic: 0 }, // 0x62
    PackedGlyphMetric { w: 8582, h: 12288, d: 4096, ic: 0 }, // 0x63
    PackedGlyphMetric { w: 8582, h: 12288, d: 4096, ic: 0 }, // 0x64
    PackedGlyphMetric { w: 8582, h: 12288, d: 4096, ic: 0 }, // 0x65
    PackedGlyphMetric { w: 9590, h: 12288, d: 4096, ic: 0 }, // 0x66
    PackedGlyphMetric { w: 9590, h: 12288, d: 4096, ic: 0 }, // 0x67
    PackedGlyphMetric { w: 7574, h: 12288, d: 4096, ic: 0 }, // 0x68
    PackedGlyphMetric { w: 7574, h: 12288, d: 4096, ic: 0 }, // 0x69
    PackedGlyphMetric { w: 5559, h: 12288, d: 4096, ic: 0 }, // 0x6A
    PackedGlyphMetric { w: 9590, h: 12288, d: 4096, ic: 0 }, // 0x6B
    PackedGlyphMetric { w: 9590, h: 12288, d: 4096, ic: 0 }, // 0x6C
    PackedGlyphMetric { w: 11605, h: 12288, d: 4096, ic: 0 }, // 0x6D
    PackedGlyphMetric { w: 9590, h: 12288, d: 4096, ic: 0 }, // 0x6E
    PackedGlyphMetric { w: 5559, h: 11378, d: 3186, ic: 0 }, // 0x6F
    PackedGlyphMetric { w: 15376, h: 796, d: 15588, ic: 0 }, // 0x70
    PackedGlyphMetric { w: 14076, h: 11196, d: 0, ic: 0 }, // 0x71
    PackedGlyphMetric { w: 15636, h: 11196, d: 0, ic: 0 }, // 0x72
    PackedGlyphMetric { w: 8086, h: 11378, d: 3186, ic: 2016 }, // 0x73
    PackedGlyphMetric { w: 12613, h: 9102, d: 0, ic: 0 }, // 0x74
    PackedGlyphMetric { w: 12613, h: 9102, d: 0, ic: 0 }, // 0x75
    PackedGlyphMetric { w: 14629, h: 11231, d: 3039, ic: 0 }, // 0x76
    PackedGlyphMetric { w: 14629, h: 11231, d: 3039, ic: 0 }, // 0x77
    PackedGlyphMetric { w: 8582, h: 11378, d: 3186, ic: 0 }, // 0x78
    PackedGlyphMetric { w: 8582, h: 11378, d: 3186, ic: 0 }, // 0x79
    PackedGlyphMetric { w: 8582, h: 11378, d: 3186, ic: 0 }, // 0x7A
    PackedGlyphMetric { w: 11605, h: 11378, d: 3186, ic: 0 }, // 0x7B
    PackedGlyphMetric { w: 14629, h: 11378, d: 2124, ic: 0 }, // 0x7C
    PackedGlyphMetric { w: 14629, h: 11378, d: 2124, ic: 0 }, // 0x7D
    PackedGlyphMetric { w: 14629, h: 11378, d: 2124, ic: 0 }, // 0x7E
    PackedGlyphMetric { w: 14629, h: 11378, d: 2124, ic: 0 }, // 0x7F
];

pub static CMSY5_METRICS: [PackedGlyphMetric; 128] = [
    PackedGlyphMetric { w: 17750, h: 10923, d: 2731, ic: 0 }, // 0x00
    PackedGlyphMetric { w: 7509, h: 8078, d: 0, ic: 0 }, // 0x01
    PackedGlyphMetric { w: 17750, h: 10923, d: 2731, ic: 0 }, // 0x02
    PackedGlyphMetric { w: 12061, h: 7623, d: 0, ic: 0 }, // 0x03
    PackedGlyphMetric { w: 17750, h: 10923, d: 2731, ic: 0 }, // 0x04
    PackedGlyphMetric { w: 12061, h: 8078, d: 0, ic: 0 }, // 0x05
    PackedGlyphMetric { w: 17750, h: 10923, d: 2731, ic: 0 }, // 0x06
    PackedGlyphMetric { w: 17750, h: 10923, d: 2731, ic: 0 }, // 0x07
    PackedGlyphMetric { w: 17750, h: 10923, d: 2731, ic: 0 }, // 0x08
    PackedGlyphMetric { w: 17750, h: 10923, d: 2731, ic: 0 }, // 0x09
    PackedGlyphMetric { w: 17750, h: 10923, d: 2731, ic: 0 }, // 0x0A
    PackedGlyphMetric { w: 17750, h: 10923, d: 2731, ic: 0 }, // 0x0B
    PackedGlyphMetric { w: 17750, h: 10923, d: 2731, ic: 0 }, // 0x0C
    PackedGlyphMetric { w: 22301, h: 11378, d: 3186, ic: 0 }, // 0x0D
    PackedGlyphMetric { w: 12061, h: 8078, d: 0, ic: 0 }, // 0x0E
    PackedGlyphMetric { w: 12061, h: 8078, d: 0, ic: 0 }, // 0x0F
    PackedGlyphMetric { w: 17750, h: 8435, d: 243, ic: 0 }, // 0x10
    PackedGlyphMetric { w: 17750, h: 8435, d: 243, ic: 0 }, // 0x11
    PackedGlyphMetric { w: 17750, h: 11785, d: 3593, ic: 0 }, // 0x12
    PackedGlyphMetric { w: 17750, h: 11785, d: 3593, ic: 0 }, // 0x13
    PackedGlyphMetric { w: 17750, h: 11785, d: 3593, ic: 0 }, // 0x14
    PackedGlyphMetric { w: 17750, h: 11785, d: 3593, ic: 0 }, // 0x15
    PackedGlyphMetric { w: 17750, h: 11785, d: 3593, ic: 0 }, // 0x16
    PackedGlyphMetric { w: 17750, h: 11785, d: 3593, ic: 0 }, // 0x17
    PackedGlyphMetric { w: 17750, h: 6495, d: 0, ic: 0 }, // 0x18
    PackedGlyphMetric { w: 17750, h: 8823, d: 631, ic: 0 }, // 0x19
    PackedGlyphMetric { w: 17750, h: 9845, d: 1653, ic: 0 }, // 0x1A
    PackedGlyphMetric { w: 17750, h: 9845, d: 1653, ic: 0 }, // 0x1B
    PackedGlyphMetric { w: 22301, h: 9845, d: 1653, ic: 0 }, // 0x1C
    PackedGlyphMetric { w: 22301, h: 9845, d: 1653, ic: 0 }, // 0x1D
    PackedGlyphMetric { w: 17750, h: 9845, d: 1653, ic: 0 }, // 0x1E
    PackedGlyphMetric { w: 17750, h: 9845, d: 1653, ic: 0 }, // 0x1F
    PackedGlyphMetric { w: 22301, h: 6495, d: 0, ic: 0 }, // 0x20
    PackedGlyphMetric { w: 22301, h: 6495, d: 0, ic: 0 }, // 0x21
    PackedGlyphMetric { w: 12061, h: 11378, d: 3186, ic: 0 }, // 0x22
    PackedGlyphMetric { w: 12061, h: 11378, d: 3186, ic: 0 }, // 0x23
    PackedGlyphMetric { w: 22301, h: 6495, d: 0, ic: 0 }, // 0x24
    PackedGlyphMetric { w: 22301, h: 11378, d: 3186, ic: 0 }, // 0x25
    PackedGlyphMetric { w: 22301, h: 11378, d: 3186, ic: 0 }, // 0x26
    PackedGlyphMetric { w: 17750, h: 8435, d: 243, ic: 0 }, // 0x27
    PackedGlyphMetric { w: 22301, h: 6495, d: 0, ic: 0 }, // 0x28
    PackedGlyphMetric { w: 22301, h: 6495, d: 0, ic: 0 }, // 0x29
    PackedGlyphMetric { w: 14336, h: 11378, d: 3186, ic: 0 }, // 0x2A
    PackedGlyphMetric { w: 14336, h: 11378, d: 3186, ic: 0 }, // 0x2B
    PackedGlyphMetric { w: 22301, h: 6495, d: 0, ic: 0 }, // 0x2C
    PackedGlyphMetric { w: 22301, h: 11378, d: 3186, ic: 0 }, // 0x2D
    PackedGlyphMetric { w: 22301, h: 11378, d: 3186, ic: 0 }, // 0x2E
    PackedGlyphMetric { w: 17750, h: 7054, d: 0, ic: 0 }, // 0x2F
    PackedGlyphMetric { w: 7225, h: 9102, d: 0, ic: 0 }, // 0x30
    PackedGlyphMetric { w: 22301, h: 7054, d: 0, ic: 0 }, // 0x31
    PackedGlyphMetric { w: 15474, h: 9845, d: 1653, ic: 0 }, // 0x32
    PackedGlyphMetric { w: 15474, h: 9845, d: 1653, ic: 0 }, // 0x33
    PackedGlyphMetric { w: 20025, h: 11378, d: 3186, ic: 0 }, // 0x34
    PackedGlyphMetric { w: 20025, h: 11378, d: 3186, ic: 0 }, // 0x35
    PackedGlyphMetric { w: 0, h: 11378, d: 3186, ic: 0 }, // 0x36
    PackedGlyphMetric { w: 0, h: 6495, d: 0, ic: 0 }, // 0x37
    PackedGlyphMetric { w: 13198, h: 11378, d: 0, ic: 0 }, // 0x38
    PackedGlyphMetric { w: 13198, h: 11378, d: 0, ic: 0 }, // 0x39
    PackedGlyphMetric { w: 15474, h: 7054, d: 0, ic: 0 }, // 0x3A
    PackedGlyphMetric { w: 12061, h: 12288, d: 910, ic: 0 }, // 0x3B
    PackedGlyphMetric { w: 16612, h: 11378, d: 0, ic: 0 }, // 0x3C
    PackedGlyphMetric { w: 16612, h: 11378, d: 0, ic: 0 }, // 0x3D
    PackedGlyphMetric { w: 17750, h: 11378, d: 0, ic: 0 }, // 0x3E
    PackedGlyphMetric { w: 17750, h: 11378, d: 0, ic: 0 }, // 0x3F
    PackedGlyphMetric { w: 14336, h: 11378, d: 0, ic: 0 }, // 0x40
    PackedGlyphMetric { w: 18243, h: 11196, d: 0, ic: 0 }, // 0x41
    PackedGlyphMetric { w: 15062, h: 11196, d: 0, ic: 273 }, // 0x42
    PackedGlyphMetric { w: 12814, h: 11196, d: 0, ic: 635 }, // 0x43
    PackedGlyphMetric { w: 17759, h: 11196, d: 0, ic: 569 }, // 0x44
    PackedGlyphMetric { w: 12630, h: 11196, d: 0, ic: 1272 }, // 0x45
    PackedGlyphMetric { w: 16191, h: 11196, d: 0, ic: 1684 }, // 0x46
    PackedGlyphMetric { w: 13933, h: 11196, d: 1593, ic: 865 }, // 0x47
    PackedGlyphMetric { w: 19571, h: 11196, d: 0, ic: 0 }, // 0x48
    PackedGlyphMetric { w: 13427, h: 11196, d: 0, ic: 854 }, // 0x49
    PackedGlyphMetric { w: 15702, h: 11196, d: 1593, ic: 3083 }, // 0x4A
    PackedGlyphMetric { w: 17285, h: 11196, d: 0, ic: 0 }, // 0x4B
    PackedGlyphMetric { w: 15806, h: 11196, d: 0, ic: 0 }, // 0x4C
    PackedGlyphMetric { w: 25688, h: 11196, d: 0, ic: 0 }, // 0x4D
    PackedGlyphMetric { w: 18449, h: 11196, d: 0, ic: 2528 }, // 0x4E
    PackedGlyphMetric { w: 17845, h: 11196, d: 0, ic: 569 }, // 0x4F
    PackedGlyphMetric { w: 16346, h: 11196, d: 0, ic: 1404 }, // 0x50
    PackedGlyphMetric { w: 18546, h: 11196, d: 1593, ic: 0 }, // 0x51
    PackedGlyphMetric { w: 19597, h: 11196, d: 0, ic: 0 }, // 0x52
    PackedGlyphMetric { w: 14222, h: 11196, d: 0, ic: 1116 }, // 0x53
    PackedGlyphMetric { w: 13475, h: 11196, d: 0, ic: 4506 }, // 0x54
    PackedGlyphMetric { w: 14316, h: 11196, d: 0, ic: 1684 }, // 0x55
    PackedGlyphMetric { w: 14728, h: 11196, d: 0, ic: 1404 }, // 0x56
    PackedGlyphMetric { w: 22408, h: 11196, d: 0, ic: 1404 }, // 0x57
    PackedGlyphMetric { w: 16329, h: 11196, d: 0, ic: 2399 }, // 0x58
    PackedGlyphMetric { w: 15228, h: 11196, d: 1593, ic: 1404 }, // 0x59
    PackedGlyphMetric { w: 16243, h: 11196, d: 0, ic: 1347 }, // 0x5A
    PackedGlyphMetric { w: 15474, h: 9102, d: 0, ic: 0 }, // 0x5B
    PackedGlyphMetric { w: 15474, h: 9102, d: 0, ic: 0 }, // 0x5C
    PackedGlyphMetric { w: 15474, h: 9102, d: 0, ic: 0 }, // 0x5D
    PackedGlyphMetric { w: 15474, h: 9102, d: 0, ic: 0 }, // 0x5E
    PackedGlyphMetric { w: 15474, h: 9102, d: 0, ic: 0 }, // 0x5F
    PackedGlyphMetric { w: 14336, h: 11378, d: 0, ic: 0 }, // 0x60
    PackedGlyphMetric { w: 14336, h: 11378, d: 0, ic: 0 }, // 0x61
    PackedGlyphMetric { w: 10923, h: 12288, d: 4096, ic: 0 }, // 0x62
    PackedGlyphMetric { w: 10923, h: 12288, d: 4096, ic: 0 }, // 0x63
    PackedGlyphMetric { w: 10923, h: 12288, d: 4096, ic: 0 }, // 0x64
    PackedGlyphMetric { w: 10923, h: 12288, d: 4096, ic: 0 }, // 0x65
    PackedGlyphMetric { w: 12061, h: 12288, d: 4096, ic: 0 }, // 0x66
    PackedGlyphMetric { w: 12061, h: 12288, d: 4096, ic: 0 }, // 0x67
    PackedGlyphMetric { w: 9785, h: 12288, d: 4096, ic: 0 }, // 0x68
    PackedGlyphMetric { w: 9785, h: 12288, d: 4096, ic: 0 }, // 0x69
    PackedGlyphMetric { w: 7509, h: 12288, d: 4096, ic: 0 }, // 0x6A
    PackedGlyphMetric { w: 12061, h: 12288, d: 4096, ic: 0 }, // 0x6B
    PackedGlyphMetric { w: 12061, h: 12288, d: 4096, ic: 0 }, // 0x6C
    PackedGlyphMetric { w: 14336, h: 12288, d: 4096, ic: 0 }, // 0x6D
    PackedGlyphMetric { w: 12061, h: 12288, d: 4096, ic: 0 }, // 0x6E
    PackedGlyphMetric { w: 7509, h: 11378, d: 3186, ic: 0 }, // 0x6F
    PackedGlyphMetric { w: 17977, h: 918, d: 15466, ic: 0 }, // 0x70
    PackedGlyphMetric { w: 16976, h: 11196, d: 0, ic: 0 }, // 0x71
    PackedGlyphMetric { w: 18887, h: 11196, d: 0, ic: 0 }, // 0x72
    PackedGlyphMetric { w: 10354, h: 11378, d: 3186, ic: 2276 }, // 0x73
    PackedGlyphMetric { w: 15474, h: 9102, d: 0, ic: 0 }, // 0x74
    PackedGlyphMetric { w: 15474, h: 9102, d: 0, ic: 0 }, // 0x75
    PackedGlyphMetric { w: 17750, h: 11785, d: 3593, ic: 0 }, // 0x76
    PackedGlyphMetric { w: 17750, h: 11785, d: 3593, ic: 0 }, // 0x77
    PackedGlyphMetric { w: 10923, h: 11378, d: 3186, ic: 0 }, // 0x78
    PackedGlyphMetric { w: 10923, h: 11378, d: 3186, ic: 0 }, // 0x79
    PackedGlyphMetric { w: 10923, h: 11378, d: 3186, ic: 0 }, // 0x7A
    PackedGlyphMetric { w: 14336, h: 11378, d: 3186, ic: 0 }, // 0x7B
    PackedGlyphMetric { w: 17750, h: 11378, d: 2124, ic: 0 }, // 0x7C
    PackedGlyphMetric { w: 17750, h: 11378, d: 2124, ic: 0 }, // 0x7D
    PackedGlyphMetric { w: 17750, h: 11378, d: 2124, ic: 0 }, // 0x7E
    PackedGlyphMetric { w: 17750, h: 11378, d: 2124, ic: 0 }, // 0x7F
];

pub static MSAM10_METRICS: [PackedGlyphMetric; 128] = [
    PackedGlyphMetric { w: 12743, h: 11059, d: 0, ic: 0 }, // 0x00
    PackedGlyphMetric { w: 12743, h: 11059, d: 0, ic: 0 }, // 0x01
    PackedGlyphMetric { w: 12743, h: 11059, d: 0, ic: 0 }, // 0x02
    PackedGlyphMetric { w: 12743, h: 11059, d: 0, ic: 0 }, // 0x03
    PackedGlyphMetric { w: 12743, h: 11059, d: 0, ic: 0 }, // 0x04
    PackedGlyphMetric { w: 4551, h: 9009, d: 0, ic: 0 }, // 0x05
    PackedGlyphMetric { w: 10923, h: 11342, d: 1820, ic: 0 }, // 0x06
    PackedGlyphMetric { w: 10923, h: 11342, d: 1820, ic: 0 }, // 0x07
    PackedGlyphMetric { w: 12743, h: 9535, d: 1343, ic: 0 }, // 0x08
    PackedGlyphMetric { w: 12743, h: 9535, d: 1343, ic: 0 }, // 0x09
    PackedGlyphMetric { w: 16384, h: 8559, d: 222, ic: 0 }, // 0x0A
    PackedGlyphMetric { w: 16384, h: 8559, d: 222, ic: 0 }, // 0x0B
    PackedGlyphMetric { w: 12743, h: 11059, d: 0, ic: 0 }, // 0x0C
    PackedGlyphMetric { w: 11833, h: 11342, d: 0, ic: 0 }, // 0x0D
    PackedGlyphMetric { w: 14564, h: 11342, d: 0, ic: 0 }, // 0x0E
    PackedGlyphMetric { w: 10012, h: 11342, d: 0, ic: 0 }, // 0x0F
    PackedGlyphMetric { w: 16384, h: 8559, d: 222, ic: 0 }, // 0x10
    PackedGlyphMetric { w: 16384, h: 8559, d: 222, ic: 0 }, // 0x11
    PackedGlyphMetric { w: 16384, h: 11059, d: 2962, ic: 0 }, // 0x12
    PackedGlyphMetric { w: 16384, h: 11059, d: 2962, ic: 0 }, // 0x13
    PackedGlyphMetric { w: 13653, h: 11342, d: 3186, ic: 0 }, // 0x14
    PackedGlyphMetric { w: 13653, h: 11342, d: 3186, ic: 0 }, // 0x15
    PackedGlyphMetric { w: 6827, h: 11342, d: 3186, ic: 0 }, // 0x16
    PackedGlyphMetric { w: 6827, h: 11342, d: 3186, ic: 0 }, // 0x17
    PackedGlyphMetric { w: 6827, h: 11342, d: 3186, ic: 0 }, // 0x18
    PackedGlyphMetric { w: 6827, h: 11342, d: 3186, ic: 0 }, // 0x19
    PackedGlyphMetric { w: 18204, h: 8559, d: 222, ic: 0 }, // 0x1A
    PackedGlyphMetric { w: 18204, h: 8559, d: 222, ic: 0 }, // 0x1B
    PackedGlyphMetric { w: 16384, h: 11059, d: 2962, ic: 0 }, // 0x1C
    PackedGlyphMetric { w: 16384, h: 11059, d: 2962, ic: 0 }, // 0x1D
    PackedGlyphMetric { w: 8192, h: 11342, d: 0, ic: 0 }, // 0x1E
    PackedGlyphMetric { w: 8192, h: 11342, d: 0, ic: 0 }, // 0x1F
    PackedGlyphMetric { w: 16384, h: 6191, d: 0, ic: 0 }, // 0x20
    PackedGlyphMetric { w: 22756, h: 6191, d: 0, ic: 0 }, // 0x21
    PackedGlyphMetric { w: 16384, h: 9009, d: 0, ic: 0 }, // 0x22
    PackedGlyphMetric { w: 16384, h: 9009, d: 0, ic: 0 }, // 0x23
    PackedGlyphMetric { w: 12743, h: 11953, d: 3761, ic: 0 }, // 0x24
    PackedGlyphMetric { w: 12743, h: 11953, d: 3761, ic: 0 }, // 0x25
    PackedGlyphMetric { w: 12743, h: 11953, d: 3761, ic: 0 }, // 0x26
    PackedGlyphMetric { w: 12743, h: 12384, d: 4192, ic: 0 }, // 0x27
    PackedGlyphMetric { w: 18204, h: 9009, d: 0, ic: 0 }, // 0x28
    PackedGlyphMetric { w: 10923, h: 11342, d: 0, ic: 0 }, // 0x29
    PackedGlyphMetric { w: 10923, h: 11342, d: 0, ic: 0 }, // 0x2A
    PackedGlyphMetric { w: 12743, h: 9535, d: 1343, ic: 0 }, // 0x2B
    PackedGlyphMetric { w: 12743, h: 15019, d: 1343, ic: 0 }, // 0x2C
    PackedGlyphMetric { w: 12743, h: 11953, d: 3761, ic: 0 }, // 0x2D
    PackedGlyphMetric { w: 12743, h: 11953, d: 3761, ic: 0 }, // 0x2E
    PackedGlyphMetric { w: 12743, h: 12384, d: 4192, ic: 0 }, // 0x2F
    PackedGlyphMetric { w: 12743, h: 10431, d: 2239, ic: 0 }, // 0x30
    PackedGlyphMetric { w: 12743, h: 10431, d: 2239, ic: 0 }, // 0x31
    PackedGlyphMetric { w: 12743, h: 10431, d: 2239, ic: 0 }, // 0x32
    PackedGlyphMetric { w: 12743, h: 10431, d: 2239, ic: 0 }, // 0x33
    PackedGlyphMetric { w: 12743, h: 10431, d: 2239, ic: 0 }, // 0x34
    PackedGlyphMetric { w: 12743, h: 12384, d: 4192, ic: 0 }, // 0x35
    PackedGlyphMetric { w: 12743, h: 10431, d: 2239, ic: 0 }, // 0x36
    PackedGlyphMetric { w: 12743, h: 11059, d: 2962, ic: 0 }, // 0x37
    PackedGlyphMetric { w: 4506, h: 9009, d: 0, ic: 0 }, // 0x38
    PackedGlyphMetric { w: 8192, h: 7602, d: 0, ic: 0 }, // 0x39
    PackedGlyphMetric { w: 12743, h: 9535, d: 1343, ic: 0 }, // 0x3A
    PackedGlyphMetric { w: 12743, h: 9535, d: 1343, ic: 0 }, // 0x3B
    PackedGlyphMetric { w: 12743, h: 10431, d: 2239, ic: 0 }, // 0x3C
    PackedGlyphMetric { w: 12743, h: 12384, d: 4192, ic: 0 }, // 0x3D
    PackedGlyphMetric { w: 12743, h: 10431, d: 2239, ic: 0 }, // 0x3E
    PackedGlyphMetric { w: 12743, h: 11059, d: 2962, ic: 0 }, // 0x3F
    PackedGlyphMetric { w: 12743, h: 9009, d: 576, ic: 0 }, // 0x40
    PackedGlyphMetric { w: 12743, h: 9009, d: 576, ic: 0 }, // 0x41
    PackedGlyphMetric { w: 12743, h: 9009, d: 576, ic: 0 }, // 0x42
    PackedGlyphMetric { w: 12743, h: 9009, d: 576, ic: 0 }, // 0x43
    PackedGlyphMetric { w: 12743, h: 10431, d: 2239, ic: 0 }, // 0x44
    PackedGlyphMetric { w: 12743, h: 10431, d: 2239, ic: 0 }, // 0x45
    PackedGlyphMetric { w: 15474, h: 11342, d: 3186, ic: 0 }, // 0x46
    PackedGlyphMetric { w: 8192, h: 12384, d: 4192, ic: 0 }, // 0x47
    PackedGlyphMetric { w: 11833, h: 9009, d: 0, ic: 0 }, // 0x48
    PackedGlyphMetric { w: 12743, h: 9009, d: 576, ic: 0 }, // 0x49
    PackedGlyphMetric { w: 12743, h: 9009, d: 576, ic: 0 }, // 0x4A
    PackedGlyphMetric { w: 8192, h: 7602, d: 0, ic: 0 }, // 0x4B
    PackedGlyphMetric { w: 8192, h: 7602, d: 0, ic: 0 }, // 0x4C
    PackedGlyphMetric { w: 11833, h: 9009, d: 0, ic: 0 }, // 0x4D
    PackedGlyphMetric { w: 11833, h: 9009, d: 0, ic: 0 }, // 0x4E
    PackedGlyphMetric { w: 11833, h: 9009, d: 0, ic: 0 }, // 0x4F
    PackedGlyphMetric { w: 12743, h: 11342, d: 0, ic: 0 }, // 0x50
    PackedGlyphMetric { w: 12743, h: 14511, d: 6319, ic: 0 }, // 0x51
    PackedGlyphMetric { w: 12743, h: 14511, d: 6319, ic: 0 }, // 0x52
    PackedGlyphMetric { w: 12743, h: 16098, d: 7906, ic: 0 }, // 0x53
    PackedGlyphMetric { w: 12743, h: 16098, d: 7906, ic: 0 }, // 0x54
    PackedGlyphMetric { w: 12288, h: 11059, d: 0, ic: 410 }, // 0x55
    PackedGlyphMetric { w: 16384, h: 10431, d: 2239, ic: 0 }, // 0x56
    PackedGlyphMetric { w: 16384, h: 10431, d: 2239, ic: 0 }, // 0x57
    PackedGlyphMetric { w: 13653, h: 11342, d: 0, ic: 0 }, // 0x58
    PackedGlyphMetric { w: 10012, h: 11342, d: 3186, ic: 0 }, // 0x59
    PackedGlyphMetric { w: 10012, h: 11342, d: 3186, ic: 0 }, // 0x5A
    PackedGlyphMetric { w: 10012, h: 12384, d: 3186, ic: 0 }, // 0x5B
    PackedGlyphMetric { w: 11833, h: 11342, d: 0, ic: 0 }, // 0x5C
    PackedGlyphMetric { w: 11833, h: 11342, d: 0, ic: 0 }, // 0x5D
    PackedGlyphMetric { w: 11833, h: 8559, d: 576, ic: 0 }, // 0x5E
    PackedGlyphMetric { w: 12743, h: 11342, d: 0, ic: 0 }, // 0x5F
    PackedGlyphMetric { w: 12743, h: 11342, d: 3186, ic: 0 }, // 0x60
    PackedGlyphMetric { w: 12743, h: 11342, d: 3186, ic: 0 }, // 0x61
    PackedGlyphMetric { w: 12743, h: 9009, d: 576, ic: 0 }, // 0x62
    PackedGlyphMetric { w: 12743, h: 9009, d: 576, ic: 0 }, // 0x63
    PackedGlyphMetric { w: 10923, h: 9009, d: 0, ic: 0 }, // 0x64
    PackedGlyphMetric { w: 10923, h: 9009, d: 0, ic: 0 }, // 0x65
    PackedGlyphMetric { w: 12459, h: 9009, d: 0, ic: 0 }, // 0x66
    PackedGlyphMetric { w: 12459, h: 9009, d: 0, ic: 0 }, // 0x67
    PackedGlyphMetric { w: 12743, h: 11342, d: 0, ic: 0 }, // 0x68
    PackedGlyphMetric { w: 12743, h: 11342, d: 0, ic: 0 }, // 0x69
    PackedGlyphMetric { w: 12743, h: 12384, d: 4192, ic: 0 }, // 0x6A
    PackedGlyphMetric { w: 12743, h: 12384, d: 4192, ic: 0 }, // 0x6B
    PackedGlyphMetric { w: 12743, h: 9009, d: 993, ic: 0 }, // 0x6C
    PackedGlyphMetric { w: 12743, h: 9009, d: 993, ic: 0 }, // 0x6D
    PackedGlyphMetric { w: 21845, h: 9009, d: 576, ic: 0 }, // 0x6E
    PackedGlyphMetric { w: 21845, h: 9009, d: 576, ic: 0 }, // 0x6F
    PackedGlyphMetric { w: 8192, h: 11342, d: 0, ic: 0 }, // 0x70
    PackedGlyphMetric { w: 8192, h: 11342, d: 0, ic: 0 }, // 0x71
    PackedGlyphMetric { w: 15510, h: 11342, d: 2549, ic: 0 }, // 0x72
    PackedGlyphMetric { w: 14782, h: 11342, d: 2549, ic: 0 }, // 0x73
    PackedGlyphMetric { w: 10923, h: 11342, d: 0, ic: 0 }, // 0x74
    PackedGlyphMetric { w: 12743, h: 11342, d: 1343, ic: 0 }, // 0x75
    PackedGlyphMetric { w: 12743, h: 6191, d: 0, ic: 0 }, // 0x76
    PackedGlyphMetric { w: 12743, h: 7602, d: 0, ic: 0 }, // 0x77
    PackedGlyphMetric { w: 8192, h: 6191, d: 0, ic: 0 }, // 0x78
    PackedGlyphMetric { w: 8192, h: 6191, d: 0, ic: 0 }, // 0x79
    PackedGlyphMetric { w: 13653, h: 11342, d: 0, ic: 0 }, // 0x7A
    PackedGlyphMetric { w: 8192, h: 13517, d: 0, ic: 0 }, // 0x7B
    PackedGlyphMetric { w: 9102, h: 7054, d: 3186, ic: 0 }, // 0x7C
    PackedGlyphMetric { w: 12743, h: 9535, d: 1343, ic: 0 }, // 0x7D
    PackedGlyphMetric { w: 12743, h: 9535, d: 1343, ic: 0 }, // 0x7E
    PackedGlyphMetric { w: 12743, h: 9535, d: 1343, ic: 0 }, // 0x7F
];

pub static MSBM10_METRICS: [PackedGlyphMetric; 128] = [
    PackedGlyphMetric { w: 12743, h: 12407, d: 4119, ic: 0 }, // 0x00
    PackedGlyphMetric { w: 12743, h: 12407, d: 4119, ic: 0 }, // 0x01
    PackedGlyphMetric { w: 12743, h: 13006, d: 4960, ic: 0 }, // 0x02
    PackedGlyphMetric { w: 12743, h: 13006, d: 4960, ic: 0 }, // 0x03
    PackedGlyphMetric { w: 12743, h: 11563, d: 3371, ic: 0 }, // 0x04
    PackedGlyphMetric { w: 12743, h: 11563, d: 3371, ic: 0 }, // 0x05
    PackedGlyphMetric { w: 12743, h: 11563, d: 3371, ic: 0 }, // 0x06
    PackedGlyphMetric { w: 12743, h: 11563, d: 3371, ic: 0 }, // 0x07
    PackedGlyphMetric { w: 12743, h: 12407, d: 4119, ic: 0 }, // 0x08
    PackedGlyphMetric { w: 12743, h: 12407, d: 4119, ic: 0 }, // 0x09
    PackedGlyphMetric { w: 12743, h: 13006, d: 4960, ic: 0 }, // 0x0A
    PackedGlyphMetric { w: 12743, h: 13006, d: 4960, ic: 0 }, // 0x0B
    PackedGlyphMetric { w: 12743, h: 10420, d: 2228, ic: 0 }, // 0x0C
    PackedGlyphMetric { w: 12743, h: 10420, d: 2228, ic: 0 }, // 0x0D
    PackedGlyphMetric { w: 12743, h: 13006, d: 4960, ic: 0 }, // 0x0E
    PackedGlyphMetric { w: 12743, h: 13006, d: 4960, ic: 0 }, // 0x0F
    PackedGlyphMetric { w: 12743, h: 12142, d: 3805, ic: 0 }, // 0x10
    PackedGlyphMetric { w: 12743, h: 12142, d: 3805, ic: 0 }, // 0x11
    PackedGlyphMetric { w: 12743, h: 12142, d: 3805, ic: 0 }, // 0x12
    PackedGlyphMetric { w: 12743, h: 12142, d: 3805, ic: 0 }, // 0x13
    PackedGlyphMetric { w: 12743, h: 15065, d: 6873, ic: 0 }, // 0x14
    PackedGlyphMetric { w: 12743, h: 15065, d: 6873, ic: 0 }, // 0x15
    PackedGlyphMetric { w: 12743, h: 12407, d: 4119, ic: 0 }, // 0x16
    PackedGlyphMetric { w: 12743, h: 12407, d: 4119, ic: 0 }, // 0x17
    PackedGlyphMetric { w: 12743, h: 12407, d: 4287, ic: 0 }, // 0x18
    PackedGlyphMetric { w: 12743, h: 12407, d: 4287, ic: 0 }, // 0x19
    PackedGlyphMetric { w: 12743, h: 12407, d: 4287, ic: 0 }, // 0x1A
    PackedGlyphMetric { w: 12743, h: 12407, d: 4287, ic: 0 }, // 0x1B
    PackedGlyphMetric { w: 12743, h: 6011, d: 0, ic: 0 }, // 0x1C
    PackedGlyphMetric { w: 12743, h: 13006, d: 4960, ic: 0 }, // 0x1D
    PackedGlyphMetric { w: 14564, h: 11287, d: 3186, ic: 0 }, // 0x1E
    PackedGlyphMetric { w: 14564, h: 12142, d: 3186, ic: 0 }, // 0x1F
    PackedGlyphMetric { w: 12743, h: 10420, d: 2228, ic: 0 }, // 0x20
    PackedGlyphMetric { w: 12743, h: 10420, d: 2228, ic: 0 }, // 0x21
    PackedGlyphMetric { w: 12743, h: 12407, d: 4119, ic: 0 }, // 0x22
    PackedGlyphMetric { w: 12743, h: 12407, d: 4119, ic: 0 }, // 0x23
    PackedGlyphMetric { w: 12743, h: 13006, d: 4666, ic: 0 }, // 0x24
    PackedGlyphMetric { w: 12743, h: 13006, d: 4666, ic: 0 }, // 0x25
    PackedGlyphMetric { w: 12743, h: 12407, d: 4119, ic: 0 }, // 0x26
    PackedGlyphMetric { w: 12743, h: 12407, d: 4119, ic: 0 }, // 0x27
    PackedGlyphMetric { w: 12743, h: 10420, d: 2228, ic: 0 }, // 0x28
    PackedGlyphMetric { w: 12743, h: 10420, d: 2228, ic: 0 }, // 0x29
    PackedGlyphMetric { w: 12743, h: 13006, d: 4960, ic: 0 }, // 0x2A
    PackedGlyphMetric { w: 12743, h: 13006, d: 4960, ic: 0 }, // 0x2B
    PackedGlyphMetric { w: 8192, h: 12142, d: 4119, ic: 0 }, // 0x2C
    PackedGlyphMetric { w: 4551, h: 12142, d: 4119, ic: 0 }, // 0x2D
    PackedGlyphMetric { w: 3641, h: 9530, d: 1338, ic: 0 }, // 0x2E
    PackedGlyphMetric { w: 6372, h: 9530, d: 1338, ic: 0 }, // 0x2F
    PackedGlyphMetric { w: 10012, h: 11287, d: 0, ic: 0 }, // 0x30
    PackedGlyphMetric { w: 11833, h: 11287, d: 0, ic: 0 }, // 0x31
    PackedGlyphMetric { w: 10012, h: 11287, d: 0, ic: 0 }, // 0x32
    PackedGlyphMetric { w: 11833, h: 11287, d: 0, ic: 0 }, // 0x33
    PackedGlyphMetric { w: 12743, h: 13006, d: 4960, ic: 0 }, // 0x34
    PackedGlyphMetric { w: 12743, h: 13006, d: 4960, ic: 0 }, // 0x35
    PackedGlyphMetric { w: 12743, h: 11563, d: 3371, ic: 0 }, // 0x36
    PackedGlyphMetric { w: 12743, h: 11563, d: 3371, ic: 0 }, // 0x37
    PackedGlyphMetric { w: 16384, h: 6011, d: 0, ic: 0 }, // 0x38
    PackedGlyphMetric { w: 16384, h: 6011, d: 0, ic: 0 }, // 0x39
    PackedGlyphMetric { w: 16384, h: 6011, d: 0, ic: 0 }, // 0x3A
    PackedGlyphMetric { w: 16384, h: 6011, d: 0, ic: 0 }, // 0x3B
    PackedGlyphMetric { w: 16384, h: 6011, d: 0, ic: 0 }, // 0x3C
    PackedGlyphMetric { w: 16384, h: 6011, d: 0, ic: 0 }, // 0x3D
    PackedGlyphMetric { w: 12743, h: 9530, d: 1338, ic: 0 }, // 0x3E
    PackedGlyphMetric { w: 12743, h: 9530, d: 1338, ic: 0 }, // 0x3F
    PackedGlyphMetric { w: 9102, h: 11287, d: 0, ic: 0 }, // 0x40
    PackedGlyphMetric { w: 11833, h: 11287, d: 0, ic: 0 }, // 0x41
    PackedGlyphMetric { w: 10923, h: 11287, d: 0, ic: 0 }, // 0x42
    PackedGlyphMetric { w: 11833, h: 11287, d: 0, ic: 0 }, // 0x43
    PackedGlyphMetric { w: 11833, h: 11287, d: 0, ic: 0 }, // 0x44
    PackedGlyphMetric { w: 10923, h: 11287, d: 0, ic: 0 }, // 0x45
    PackedGlyphMetric { w: 10012, h: 11287, d: 0, ic: 0 }, // 0x46
    PackedGlyphMetric { w: 12743, h: 11287, d: 0, ic: 0 }, // 0x47
    PackedGlyphMetric { w: 12743, h: 11287, d: 0, ic: 0 }, // 0x48
    PackedGlyphMetric { w: 6372, h: 11287, d: 0, ic: 0 }, // 0x49
    PackedGlyphMetric { w: 8192, h: 11287, d: 2731, ic: 0 }, // 0x4A
    PackedGlyphMetric { w: 12743, h: 11287, d: 0, ic: 0 }, // 0x4B
    PackedGlyphMetric { w: 10923, h: 11287, d: 0, ic: 0 }, // 0x4C
    PackedGlyphMetric { w: 15474, h: 11287, d: 0, ic: 0 }, // 0x4D
    PackedGlyphMetric { w: 11833, h: 11287, d: 0, ic: 0 }, // 0x4E
    PackedGlyphMetric { w: 12743, h: 11287, d: 2731, ic: 0 }, // 0x4F
    PackedGlyphMetric { w: 10012, h: 11287, d: 0, ic: 0 }, // 0x50
    PackedGlyphMetric { w: 12743, h: 11287, d: 2731, ic: 0 }, // 0x51
    PackedGlyphMetric { w: 11833, h: 11287, d: 0, ic: 0 }, // 0x52
    PackedGlyphMetric { w: 9102, h: 11287, d: 0, ic: 0 }, // 0x53
    PackedGlyphMetric { w: 10923, h: 11287, d: 0, ic: 0 }, // 0x54
    PackedGlyphMetric { w: 11833, h: 11287, d: 0, ic: 0 }, // 0x55
    PackedGlyphMetric { w: 11833, h: 11287, d: 0, ic: 0 }, // 0x56
    PackedGlyphMetric { w: 16384, h: 11287, d: 0, ic: 0 }, // 0x57
    PackedGlyphMetric { w: 11833, h: 11287, d: 0, ic: 0 }, // 0x58
    PackedGlyphMetric { w: 11833, h: 11287, d: 0, ic: 0 }, // 0x59
    PackedGlyphMetric { w: 10923, h: 11287, d: 0, ic: 0 }, // 0x5A
    PackedGlyphMetric { w: 30948, h: 13517, d: 0, ic: 0 }, // 0x5B
    PackedGlyphMetric { w: 38229, h: 13517, d: 0, ic: 0 }, // 0x5C
    PackedGlyphMetric { w: 30948, h: 13517, d: 0, ic: 0 }, // 0x5D
    PackedGlyphMetric { w: 38229, h: 14746, d: 0, ic: 0 }, // 0x5E
    PackedGlyphMetric { w: 0, h: 0, d: 0, ic: 0 }, // 0x5F
    PackedGlyphMetric { w: 9102, h: 11287, d: 0, ic: 0 }, // 0x60
    PackedGlyphMetric { w: 10468, h: 11287, d: 0, ic: 0 }, // 0x61
    PackedGlyphMetric { w: 0, h: 0, d: 0, ic: 0 }, // 0x62
    PackedGlyphMetric { w: 0, h: 0, d: 0, ic: 0 }, // 0x63
    PackedGlyphMetric { w: 0, h: 0, d: 0, ic: 0 }, // 0x64
    PackedGlyphMetric { w: 0, h: 0, d: 0, ic: 0 }, // 0x65
    PackedGlyphMetric { w: 11833, h: 11287, d: 0, ic: 0 }, // 0x66
    PackedGlyphMetric { w: 9102, h: 11287, d: 0, ic: 0 }, // 0x67
    PackedGlyphMetric { w: 12743, h: 7598, d: 0, ic: 0 }, // 0x68
    PackedGlyphMetric { w: 10923, h: 11287, d: 0, ic: 0 }, // 0x69
    PackedGlyphMetric { w: 7282, h: 11287, d: 0, ic: 0 }, // 0x6A
    PackedGlyphMetric { w: 10923, h: 11287, d: 0, ic: 0 }, // 0x6B
    PackedGlyphMetric { w: 12743, h: 8833, d: 641, ic: 0 }, // 0x6C
    PackedGlyphMetric { w: 12743, h: 8833, d: 641, ic: 0 }, // 0x6D
    PackedGlyphMetric { w: 12743, h: 9530, d: 1338, ic: 0 }, // 0x6E
    PackedGlyphMetric { w: 12743, h: 9530, d: 1338, ic: 0 }, // 0x6F
    PackedGlyphMetric { w: 3641, h: 9530, d: 1338, ic: 0 }, // 0x70
    PackedGlyphMetric { w: 6372, h: 9530, d: 1338, ic: 0 }, // 0x71
    PackedGlyphMetric { w: 12743, h: 9530, d: 1338, ic: 0 }, // 0x72
    PackedGlyphMetric { w: 12743, h: 6011, d: 0, ic: 0 }, // 0x73
    PackedGlyphMetric { w: 12743, h: 7915, d: 0, ic: 0 }, // 0x74
    PackedGlyphMetric { w: 12743, h: 9530, d: 1338, ic: 0 }, // 0x75
    PackedGlyphMetric { w: 12743, h: 12407, d: 4287, ic: 0 }, // 0x76
    PackedGlyphMetric { w: 12743, h: 12407, d: 4287, ic: 0 }, // 0x77
    PackedGlyphMetric { w: 16384, h: 7054, d: 0, ic: 0 }, // 0x78
    PackedGlyphMetric { w: 16384, h: 7054, d: 0, ic: 0 }, // 0x79
    PackedGlyphMetric { w: 12743, h: 9530, d: 1338, ic: 0 }, // 0x7A
    PackedGlyphMetric { w: 10923, h: 7054, d: 0, ic: 660 }, // 0x7B
    PackedGlyphMetric { w: 9102, h: 11287, d: 0, ic: 0 }, // 0x7C
    PackedGlyphMetric { w: 8852, h: 11287, d: 0, ic: 0 }, // 0x7D
    PackedGlyphMetric { w: 8852, h: 11287, d: 0, ic: 0 }, // 0x7E
    PackedGlyphMetric { w: 7032, h: 7054, d: 0, ic: 0 }, // 0x7F
];




/// O(1) CMR10 glyph metric lookup.
#[inline(always)]
pub fn cmr10_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMR10_METRICS[code as usize].unpack()
    } else {
        GlyphMetric { w: 0.0, h: 0.0, d: 0.0, ic: 0.0 }
    }
}

/// O(1) CMR7 glyph metric lookup.
#[inline(always)]
pub fn cmr7_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMR7_METRICS[code as usize].unpack()
    } else {
        GlyphMetric { w: 0.0, h: 0.0, d: 0.0, ic: 0.0 }
    }
}

/// O(1) CMR5 glyph metric lookup.
#[inline(always)]
pub fn cmr5_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMR5_METRICS[code as usize].unpack()
    } else {
        GlyphMetric { w: 0.0, h: 0.0, d: 0.0, ic: 0.0 }
    }
}

/// O(1) CMMI7 glyph metric lookup.
#[inline(always)]
pub fn cmmi7_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMMI7_METRICS[code as usize].unpack()
    } else {
        GlyphMetric { w: 0.0, h: 0.0, d: 0.0, ic: 0.0 }
    }
}

/// O(1) CMMI5 glyph metric lookup.
#[inline(always)]
pub fn cmmi5_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMMI5_METRICS[code as usize].unpack()
    } else {
        GlyphMetric { w: 0.0, h: 0.0, d: 0.0, ic: 0.0 }
    }
}

/// O(1) CMSY7 glyph metric lookup.
#[inline(always)]
pub fn cmsy7_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMSY7_METRICS[code as usize].unpack()
    } else {
        GlyphMetric { w: 0.0, h: 0.0, d: 0.0, ic: 0.0 }
    }
}

/// O(1) CMSY5 glyph metric lookup.
#[inline(always)]
pub fn cmsy5_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMSY5_METRICS[code as usize].unpack()
    } else {
        GlyphMetric { w: 0.0, h: 0.0, d: 0.0, ic: 0.0 }
    }
}

/// O(1) MSAM10 glyph metric lookup.
#[inline(always)]
pub fn msam10_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        MSAM10_METRICS[code as usize].unpack()
    } else {
        GlyphMetric { w: 0.0, h: 0.0, d: 0.0, ic: 0.0 }
    }
}

/// O(1) MSBM10 glyph metric lookup.
#[inline(always)]
pub fn msbm10_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        MSBM10_METRICS[code as usize].unpack()
    } else {
        GlyphMetric { w: 0.0, h: 0.0, d: 0.0, ic: 0.0 }
    }
}
