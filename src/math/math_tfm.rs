//! # TeX Appendix F Mathematical Font Metric Information
//!
//! Authoritative Computer Modern font metric tables (CMSY10, CMEX10, CMMI10)
//! extracted directly from canonical TeX TFM data.
//!
//! Designed with contiguous Array of Structures (AoS) packing:
//! - 8 bytes per glyph (width, height, depth, italic correction)
//! - Exactly 8 glyphs fit in a single 64-byte L1 CPU cache line
//! - 100% L1 cache residency with zero heap allocations

/// TeX Appendix F 8-byte glyph metric entry.
///
/// Hardware aligned and packed so that exactly 8 glyphs fit inside a single
/// 64-byte L1 CPU cache line.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphMetric {
    /// Advance width in 1/1000 design units
    pub w: u16,
    /// Typographic ink height above baseline in 1/1000 design units
    pub h: u16,
    /// Typographic ink depth below baseline in 1/1000 design units
    pub d: u16,
    /// Italic correction in 1/1000 design units
    pub ic: u16,
}

const _: () = assert!(core::mem::size_of::<GlyphMetric>() == 8);

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
pub static CMSY10_METRICS: [GlyphMetric; 128] = [
    GlyphMetric {
        w: 778,
        h: 583,
        d: 83,
        ic: 0,
    }, // 0x00
    GlyphMetric {
        w: 278,
        h: 444,
        d: 0,
        ic: 0,
    }, // 0x01
    GlyphMetric {
        w: 778,
        h: 583,
        d: 83,
        ic: 0,
    }, // 0x02
    GlyphMetric {
        w: 500,
        h: 465,
        d: 0,
        ic: 0,
    }, // 0x03
    GlyphMetric {
        w: 778,
        h: 583,
        d: 83,
        ic: 0,
    }, // 0x04
    GlyphMetric {
        w: 500,
        h: 444,
        d: 0,
        ic: 0,
    }, // 0x05
    GlyphMetric {
        w: 778,
        h: 583,
        d: 83,
        ic: 0,
    }, // 0x06
    GlyphMetric {
        w: 778,
        h: 583,
        d: 83,
        ic: 0,
    }, // 0x07
    GlyphMetric {
        w: 778,
        h: 583,
        d: 83,
        ic: 0,
    }, // 0x08
    GlyphMetric {
        w: 778,
        h: 583,
        d: 83,
        ic: 0,
    }, // 0x09
    GlyphMetric {
        w: 778,
        h: 583,
        d: 83,
        ic: 0,
    }, // 0x0A
    GlyphMetric {
        w: 778,
        h: 583,
        d: 83,
        ic: 0,
    }, // 0x0B
    GlyphMetric {
        w: 778,
        h: 583,
        d: 83,
        ic: 0,
    }, // 0x0C
    GlyphMetric {
        w: 1000,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x0D
    GlyphMetric {
        w: 500,
        h: 444,
        d: 0,
        ic: 0,
    }, // 0x0E
    GlyphMetric {
        w: 500,
        h: 444,
        d: 0,
        ic: 0,
    }, // 0x0F
    GlyphMetric {
        w: 778,
        h: 464,
        d: 0,
        ic: 0,
    }, // 0x10
    GlyphMetric {
        w: 778,
        h: 464,
        d: 0,
        ic: 0,
    }, // 0x11
    GlyphMetric {
        w: 778,
        h: 636,
        d: 136,
        ic: 0,
    }, // 0x12
    GlyphMetric {
        w: 778,
        h: 636,
        d: 136,
        ic: 0,
    }, // 0x13
    GlyphMetric {
        w: 778,
        h: 636,
        d: 136,
        ic: 0,
    }, // 0x14
    GlyphMetric {
        w: 778,
        h: 636,
        d: 136,
        ic: 0,
    }, // 0x15
    GlyphMetric {
        w: 778,
        h: 636,
        d: 136,
        ic: 0,
    }, // 0x16
    GlyphMetric {
        w: 778,
        h: 636,
        d: 136,
        ic: 0,
    }, // 0x17
    GlyphMetric {
        w: 778,
        h: 367,
        d: 0,
        ic: 0,
    }, // 0x18
    GlyphMetric {
        w: 778,
        h: 483,
        d: 0,
        ic: 0,
    }, // 0x19
    GlyphMetric {
        w: 778,
        h: 539,
        d: 39,
        ic: 0,
    }, // 0x1A
    GlyphMetric {
        w: 778,
        h: 539,
        d: 39,
        ic: 0,
    }, // 0x1B
    GlyphMetric {
        w: 1000,
        h: 539,
        d: 39,
        ic: 0,
    }, // 0x1C
    GlyphMetric {
        w: 1000,
        h: 539,
        d: 39,
        ic: 0,
    }, // 0x1D
    GlyphMetric {
        w: 778,
        h: 539,
        d: 39,
        ic: 0,
    }, // 0x1E
    GlyphMetric {
        w: 778,
        h: 539,
        d: 39,
        ic: 0,
    }, // 0x1F
    GlyphMetric {
        w: 1000,
        h: 367,
        d: 0,
        ic: 0,
    }, // 0x20
    GlyphMetric {
        w: 1000,
        h: 367,
        d: 0,
        ic: 0,
    }, // 0x21
    GlyphMetric {
        w: 500,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x22
    GlyphMetric {
        w: 500,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x23
    GlyphMetric {
        w: 1000,
        h: 367,
        d: 0,
        ic: 0,
    }, // 0x24
    GlyphMetric {
        w: 1000,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x25
    GlyphMetric {
        w: 1000,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x26
    GlyphMetric {
        w: 778,
        h: 464,
        d: 0,
        ic: 0,
    }, // 0x27
    GlyphMetric {
        w: 1000,
        h: 367,
        d: 0,
        ic: 0,
    }, // 0x28
    GlyphMetric {
        w: 1000,
        h: 367,
        d: 0,
        ic: 0,
    }, // 0x29
    GlyphMetric {
        w: 611,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x2A
    GlyphMetric {
        w: 611,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x2B
    GlyphMetric {
        w: 1000,
        h: 367,
        d: 0,
        ic: 0,
    }, // 0x2C
    GlyphMetric {
        w: 1000,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x2D
    GlyphMetric {
        w: 1000,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x2E
    GlyphMetric {
        w: 778,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x2F
    GlyphMetric {
        w: 275,
        h: 556,
        d: 0,
        ic: 0,
    }, // 0x30
    GlyphMetric {
        w: 1000,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x31
    GlyphMetric {
        w: 667,
        h: 539,
        d: 39,
        ic: 0,
    }, // 0x32
    GlyphMetric {
        w: 667,
        h: 539,
        d: 39,
        ic: 0,
    }, // 0x33
    GlyphMetric {
        w: 889,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x34
    GlyphMetric {
        w: 889,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x35
    GlyphMetric {
        w: 0,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x36
    GlyphMetric {
        w: 0,
        h: 367,
        d: 0,
        ic: 0,
    }, // 0x37
    GlyphMetric {
        w: 556,
        h: 694,
        d: 0,
        ic: 0,
    }, // 0x38
    GlyphMetric {
        w: 556,
        h: 694,
        d: 0,
        ic: 0,
    }, // 0x39
    GlyphMetric {
        w: 667,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x3A
    GlyphMetric {
        w: 500,
        h: 750,
        d: 56,
        ic: 0,
    }, // 0x3B
    GlyphMetric {
        w: 722,
        h: 694,
        d: 0,
        ic: 0,
    }, // 0x3C
    GlyphMetric {
        w: 722,
        h: 694,
        d: 0,
        ic: 0,
    }, // 0x3D
    GlyphMetric {
        w: 778,
        h: 694,
        d: 0,
        ic: 0,
    }, // 0x3E
    GlyphMetric {
        w: 778,
        h: 694,
        d: 0,
        ic: 0,
    }, // 0x3F
    GlyphMetric {
        w: 611,
        h: 694,
        d: 0,
        ic: 0,
    }, // 0x40
    GlyphMetric {
        w: 798,
        h: 683,
        d: 0,
        ic: 0,
    }, // 0x41
    GlyphMetric {
        w: 657,
        h: 683,
        d: 0,
        ic: 30,
    }, // 0x42
    GlyphMetric {
        w: 527,
        h: 683,
        d: 0,
        ic: 58,
    }, // 0x43
    GlyphMetric {
        w: 771,
        h: 683,
        d: 0,
        ic: 28,
    }, // 0x44
    GlyphMetric {
        w: 528,
        h: 683,
        d: 0,
        ic: 89,
    }, // 0x45
    GlyphMetric {
        w: 719,
        h: 683,
        d: 0,
        ic: 99,
    }, // 0x46
    GlyphMetric {
        w: 595,
        h: 683,
        d: 97,
        ic: 59,
    }, // 0x47
    GlyphMetric {
        w: 845,
        h: 683,
        d: 0,
        ic: 10,
    }, // 0x48
    GlyphMetric {
        w: 545,
        h: 683,
        d: 0,
        ic: 74,
    }, // 0x49
    GlyphMetric {
        w: 678,
        h: 683,
        d: 97,
        ic: 185,
    }, // 0x4A
    GlyphMetric {
        w: 762,
        h: 683,
        d: 0,
        ic: 14,
    }, // 0x4B
    GlyphMetric {
        w: 690,
        h: 683,
        d: 0,
        ic: 0,
    }, // 0x4C
    GlyphMetric {
        w: 1201,
        h: 683,
        d: 0,
        ic: 0,
    }, // 0x4D
    GlyphMetric {
        w: 820,
        h: 683,
        d: 0,
        ic: 147,
    }, // 0x4E
    GlyphMetric {
        w: 796,
        h: 683,
        d: 0,
        ic: 28,
    }, // 0x4F
    GlyphMetric {
        w: 696,
        h: 683,
        d: 0,
        ic: 82,
    }, // 0x50
    GlyphMetric {
        w: 817,
        h: 683,
        d: 97,
        ic: 0,
    }, // 0x51
    GlyphMetric {
        w: 848,
        h: 683,
        d: 0,
        ic: 0,
    }, // 0x52
    GlyphMetric {
        w: 606,
        h: 683,
        d: 0,
        ic: 75,
    }, // 0x53
    GlyphMetric {
        w: 545,
        h: 683,
        d: 0,
        ic: 254,
    }, // 0x54
    GlyphMetric {
        w: 626,
        h: 683,
        d: 0,
        ic: 99,
    }, // 0x55
    GlyphMetric {
        w: 613,
        h: 683,
        d: 0,
        ic: 82,
    }, // 0x56
    GlyphMetric {
        w: 988,
        h: 683,
        d: 0,
        ic: 82,
    }, // 0x57
    GlyphMetric {
        w: 713,
        h: 683,
        d: 0,
        ic: 146,
    }, // 0x58
    GlyphMetric {
        w: 668,
        h: 683,
        d: 97,
        ic: 82,
    }, // 0x59
    GlyphMetric {
        w: 725,
        h: 683,
        d: 0,
        ic: 79,
    }, // 0x5A
    GlyphMetric {
        w: 667,
        h: 556,
        d: 0,
        ic: 0,
    }, // 0x5B
    GlyphMetric {
        w: 667,
        h: 556,
        d: 0,
        ic: 0,
    }, // 0x5C
    GlyphMetric {
        w: 667,
        h: 556,
        d: 0,
        ic: 0,
    }, // 0x5D
    GlyphMetric {
        w: 667,
        h: 556,
        d: 0,
        ic: 0,
    }, // 0x5E
    GlyphMetric {
        w: 667,
        h: 556,
        d: 0,
        ic: 0,
    }, // 0x5F
    GlyphMetric {
        w: 611,
        h: 694,
        d: 0,
        ic: 0,
    }, // 0x60
    GlyphMetric {
        w: 611,
        h: 694,
        d: 0,
        ic: 0,
    }, // 0x61
    GlyphMetric {
        w: 444,
        h: 750,
        d: 250,
        ic: 0,
    }, // 0x62
    GlyphMetric {
        w: 444,
        h: 750,
        d: 250,
        ic: 0,
    }, // 0x63
    GlyphMetric {
        w: 444,
        h: 750,
        d: 250,
        ic: 0,
    }, // 0x64
    GlyphMetric {
        w: 444,
        h: 750,
        d: 250,
        ic: 0,
    }, // 0x65
    GlyphMetric {
        w: 500,
        h: 750,
        d: 250,
        ic: 0,
    }, // 0x66
    GlyphMetric {
        w: 500,
        h: 750,
        d: 250,
        ic: 0,
    }, // 0x67
    GlyphMetric {
        w: 389,
        h: 750,
        d: 250,
        ic: 0,
    }, // 0x68
    GlyphMetric {
        w: 389,
        h: 750,
        d: 250,
        ic: 0,
    }, // 0x69
    GlyphMetric {
        w: 278,
        h: 750,
        d: 250,
        ic: 0,
    }, // 0x6A
    GlyphMetric {
        w: 500,
        h: 750,
        d: 250,
        ic: 0,
    }, // 0x6B
    GlyphMetric {
        w: 500,
        h: 750,
        d: 250,
        ic: 0,
    }, // 0x6C
    GlyphMetric {
        w: 611,
        h: 750,
        d: 250,
        ic: 0,
    }, // 0x6D
    GlyphMetric {
        w: 500,
        h: 750,
        d: 250,
        ic: 0,
    }, // 0x6E
    GlyphMetric {
        w: 278,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x6F
    GlyphMetric {
        w: 833,
        h: 40,
        d: 960,
        ic: 0,
    }, // 0x70
    GlyphMetric {
        w: 750,
        h: 683,
        d: 0,
        ic: 0,
    }, // 0x71
    GlyphMetric {
        w: 833,
        h: 683,
        d: 0,
        ic: 0,
    }, // 0x72
    GlyphMetric {
        w: 417,
        h: 694,
        d: 194,
        ic: 111,
    }, // 0x73
    GlyphMetric {
        w: 667,
        h: 556,
        d: 0,
        ic: 0,
    }, // 0x74
    GlyphMetric {
        w: 667,
        h: 556,
        d: 0,
        ic: 0,
    }, // 0x75
    GlyphMetric {
        w: 778,
        h: 636,
        d: 136,
        ic: 0,
    }, // 0x76
    GlyphMetric {
        w: 778,
        h: 636,
        d: 136,
        ic: 0,
    }, // 0x77
    GlyphMetric {
        w: 444,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x78
    GlyphMetric {
        w: 444,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x79
    GlyphMetric {
        w: 444,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x7A
    GlyphMetric {
        w: 611,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x7B
    GlyphMetric {
        w: 778,
        h: 694,
        d: 130,
        ic: 0,
    }, // 0x7C
    GlyphMetric {
        w: 778,
        h: 694,
        d: 130,
        ic: 0,
    }, // 0x7D
    GlyphMetric {
        w: 778,
        h: 694,
        d: 130,
        ic: 0,
    }, // 0x7E
    GlyphMetric {
        w: 778,
        h: 694,
        d: 130,
        ic: 0,
    }, // 0x7F
];
/// CMEX10 (Math Extensions, Family 3) metric table
pub static CMEX10_METRICS: [GlyphMetric; 128] = [
    GlyphMetric {
        w: 458,
        h: 40,
        d: 1160,
        ic: 0,
    }, // 0x00
    GlyphMetric {
        w: 458,
        h: 40,
        d: 1160,
        ic: 0,
    }, // 0x01
    GlyphMetric {
        w: 417,
        h: 40,
        d: 1160,
        ic: 0,
    }, // 0x02
    GlyphMetric {
        w: 417,
        h: 40,
        d: 1160,
        ic: 0,
    }, // 0x03
    GlyphMetric {
        w: 472,
        h: 40,
        d: 1160,
        ic: 0,
    }, // 0x04
    GlyphMetric {
        w: 472,
        h: 40,
        d: 1160,
        ic: 0,
    }, // 0x05
    GlyphMetric {
        w: 472,
        h: 40,
        d: 1160,
        ic: 0,
    }, // 0x06
    GlyphMetric {
        w: 472,
        h: 40,
        d: 1160,
        ic: 0,
    }, // 0x07
    GlyphMetric {
        w: 583,
        h: 40,
        d: 1160,
        ic: 0,
    }, // 0x08
    GlyphMetric {
        w: 583,
        h: 40,
        d: 1160,
        ic: 0,
    }, // 0x09
    GlyphMetric {
        w: 472,
        h: 40,
        d: 1160,
        ic: 0,
    }, // 0x0A
    GlyphMetric {
        w: 472,
        h: 40,
        d: 1160,
        ic: 0,
    }, // 0x0B
    GlyphMetric {
        w: 333,
        h: 0,
        d: 600,
        ic: 0,
    }, // 0x0C
    GlyphMetric {
        w: 556,
        h: 0,
        d: 600,
        ic: 0,
    }, // 0x0D
    GlyphMetric {
        w: 578,
        h: 40,
        d: 1160,
        ic: 0,
    }, // 0x0E
    GlyphMetric {
        w: 578,
        h: 40,
        d: 1160,
        ic: 0,
    }, // 0x0F
    GlyphMetric {
        w: 597,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x10
    GlyphMetric {
        w: 597,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x11
    GlyphMetric {
        w: 736,
        h: 40,
        d: 2360,
        ic: 0,
    }, // 0x12
    GlyphMetric {
        w: 736,
        h: 40,
        d: 2360,
        ic: 0,
    }, // 0x13
    GlyphMetric {
        w: 528,
        h: 40,
        d: 2360,
        ic: 0,
    }, // 0x14
    GlyphMetric {
        w: 528,
        h: 40,
        d: 2360,
        ic: 0,
    }, // 0x15
    GlyphMetric {
        w: 583,
        h: 40,
        d: 2360,
        ic: 0,
    }, // 0x16
    GlyphMetric {
        w: 583,
        h: 40,
        d: 2360,
        ic: 0,
    }, // 0x17
    GlyphMetric {
        w: 583,
        h: 40,
        d: 2360,
        ic: 0,
    }, // 0x18
    GlyphMetric {
        w: 583,
        h: 40,
        d: 2360,
        ic: 0,
    }, // 0x19
    GlyphMetric {
        w: 750,
        h: 40,
        d: 2360,
        ic: 0,
    }, // 0x1A
    GlyphMetric {
        w: 750,
        h: 40,
        d: 2360,
        ic: 0,
    }, // 0x1B
    GlyphMetric {
        w: 750,
        h: 40,
        d: 2360,
        ic: 0,
    }, // 0x1C
    GlyphMetric {
        w: 750,
        h: 40,
        d: 2360,
        ic: 0,
    }, // 0x1D
    GlyphMetric {
        w: 1044,
        h: 40,
        d: 2360,
        ic: 0,
    }, // 0x1E
    GlyphMetric {
        w: 1044,
        h: 40,
        d: 2360,
        ic: 0,
    }, // 0x1F
    GlyphMetric {
        w: 792,
        h: 40,
        d: 2960,
        ic: 0,
    }, // 0x20
    GlyphMetric {
        w: 792,
        h: 40,
        d: 2960,
        ic: 0,
    }, // 0x21
    GlyphMetric {
        w: 583,
        h: 40,
        d: 2960,
        ic: 0,
    }, // 0x22
    GlyphMetric {
        w: 583,
        h: 40,
        d: 2960,
        ic: 0,
    }, // 0x23
    GlyphMetric {
        w: 639,
        h: 40,
        d: 2960,
        ic: 0,
    }, // 0x24
    GlyphMetric {
        w: 639,
        h: 40,
        d: 2960,
        ic: 0,
    }, // 0x25
    GlyphMetric {
        w: 639,
        h: 40,
        d: 2960,
        ic: 0,
    }, // 0x26
    GlyphMetric {
        w: 639,
        h: 40,
        d: 2960,
        ic: 0,
    }, // 0x27
    GlyphMetric {
        w: 806,
        h: 40,
        d: 2960,
        ic: 0,
    }, // 0x28
    GlyphMetric {
        w: 806,
        h: 40,
        d: 2960,
        ic: 0,
    }, // 0x29
    GlyphMetric {
        w: 806,
        h: 40,
        d: 2960,
        ic: 0,
    }, // 0x2A
    GlyphMetric {
        w: 806,
        h: 40,
        d: 2960,
        ic: 0,
    }, // 0x2B
    GlyphMetric {
        w: 1278,
        h: 40,
        d: 2960,
        ic: 0,
    }, // 0x2C
    GlyphMetric {
        w: 1278,
        h: 40,
        d: 2960,
        ic: 0,
    }, // 0x2D
    GlyphMetric {
        w: 811,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x2E
    GlyphMetric {
        w: 811,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x2F
    GlyphMetric {
        w: 875,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x30
    GlyphMetric {
        w: 875,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x31
    GlyphMetric {
        w: 667,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x32
    GlyphMetric {
        w: 667,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x33
    GlyphMetric {
        w: 667,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x34
    GlyphMetric {
        w: 667,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x35
    GlyphMetric {
        w: 667,
        h: 0,
        d: 600,
        ic: 0,
    }, // 0x36
    GlyphMetric {
        w: 667,
        h: 0,
        d: 600,
        ic: 0,
    }, // 0x37
    GlyphMetric {
        w: 889,
        h: 0,
        d: 900,
        ic: 0,
    }, // 0x38
    GlyphMetric {
        w: 889,
        h: 0,
        d: 900,
        ic: 0,
    }, // 0x39
    GlyphMetric {
        w: 889,
        h: 0,
        d: 900,
        ic: 0,
    }, // 0x3A
    GlyphMetric {
        w: 889,
        h: 0,
        d: 900,
        ic: 0,
    }, // 0x3B
    GlyphMetric {
        w: 889,
        h: 0,
        d: 1800,
        ic: 0,
    }, // 0x3C
    GlyphMetric {
        w: 889,
        h: 0,
        d: 1800,
        ic: 0,
    }, // 0x3D
    GlyphMetric {
        w: 889,
        h: 0,
        d: 300,
        ic: 0,
    }, // 0x3E
    GlyphMetric {
        w: 667,
        h: 0,
        d: 600,
        ic: 0,
    }, // 0x3F
    GlyphMetric {
        w: 875,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x40
    GlyphMetric {
        w: 875,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x41
    GlyphMetric {
        w: 875,
        h: 0,
        d: 600,
        ic: 0,
    }, // 0x42
    GlyphMetric {
        w: 875,
        h: 0,
        d: 600,
        ic: 0,
    }, // 0x43
    GlyphMetric {
        w: 611,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x44
    GlyphMetric {
        w: 611,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x45
    GlyphMetric {
        w: 833,
        h: 0,
        d: 1000,
        ic: 0,
    }, // 0x46
    GlyphMetric {
        w: 1111,
        h: 100,
        d: 1500,
        ic: 0,
    }, // 0x47
    GlyphMetric {
        w: 472,
        h: 0,
        d: 1111,
        ic: 194,
    }, // 0x48
    GlyphMetric {
        w: 556,
        h: 0,
        d: 2222,
        ic: 444,
    }, // 0x49
    GlyphMetric {
        w: 1111,
        h: 0,
        d: 1000,
        ic: 0,
    }, // 0x4A
    GlyphMetric {
        w: 1511,
        h: 100,
        d: 1500,
        ic: 0,
    }, // 0x4B
    GlyphMetric {
        w: 1111,
        h: 0,
        d: 1000,
        ic: 0,
    }, // 0x4C
    GlyphMetric {
        w: 1511,
        h: 100,
        d: 1500,
        ic: 0,
    }, // 0x4D
    GlyphMetric {
        w: 1111,
        h: 0,
        d: 1000,
        ic: 0,
    }, // 0x4E
    GlyphMetric {
        w: 1511,
        h: 100,
        d: 1500,
        ic: 0,
    }, // 0x4F
    GlyphMetric {
        w: 1056,
        h: 0,
        d: 1000,
        ic: 0,
    }, // 0x50
    GlyphMetric {
        w: 944,
        h: 0,
        d: 1000,
        ic: 0,
    }, // 0x51
    GlyphMetric {
        w: 472,
        h: 0,
        d: 1111,
        ic: 194,
    }, // 0x52
    GlyphMetric {
        w: 833,
        h: 0,
        d: 1000,
        ic: 0,
    }, // 0x53
    GlyphMetric {
        w: 833,
        h: 0,
        d: 1000,
        ic: 0,
    }, // 0x54
    GlyphMetric {
        w: 833,
        h: 0,
        d: 1000,
        ic: 0,
    }, // 0x55
    GlyphMetric {
        w: 833,
        h: 0,
        d: 1000,
        ic: 0,
    }, // 0x56
    GlyphMetric {
        w: 833,
        h: 0,
        d: 1000,
        ic: 0,
    }, // 0x57
    GlyphMetric {
        w: 1444,
        h: 100,
        d: 1500,
        ic: 0,
    }, // 0x58
    GlyphMetric {
        w: 1278,
        h: 100,
        d: 1500,
        ic: 0,
    }, // 0x59
    GlyphMetric {
        w: 556,
        h: 0,
        d: 2222,
        ic: 444,
    }, // 0x5A
    GlyphMetric {
        w: 1111,
        h: 100,
        d: 1500,
        ic: 0,
    }, // 0x5B
    GlyphMetric {
        w: 1111,
        h: 100,
        d: 1500,
        ic: 0,
    }, // 0x5C
    GlyphMetric {
        w: 1111,
        h: 100,
        d: 1500,
        ic: 0,
    }, // 0x5D
    GlyphMetric {
        w: 1111,
        h: 100,
        d: 1500,
        ic: 0,
    }, // 0x5E
    GlyphMetric {
        w: 1111,
        h: 100,
        d: 1500,
        ic: 0,
    }, // 0x5F
    GlyphMetric {
        w: 944,
        h: 0,
        d: 1000,
        ic: 0,
    }, // 0x60
    GlyphMetric {
        w: 1278,
        h: 100,
        d: 1500,
        ic: 0,
    }, // 0x61
    GlyphMetric {
        w: 556,
        h: 722,
        d: 0,
        ic: 0,
    }, // 0x62
    GlyphMetric {
        w: 1000,
        h: 750,
        d: 0,
        ic: 0,
    }, // 0x63
    GlyphMetric {
        w: 1444,
        h: 750,
        d: 0,
        ic: 0,
    }, // 0x64
    GlyphMetric {
        w: 556,
        h: 722,
        d: 0,
        ic: 0,
    }, // 0x65
    GlyphMetric {
        w: 1000,
        h: 750,
        d: 0,
        ic: 0,
    }, // 0x66
    GlyphMetric {
        w: 1444,
        h: 750,
        d: 0,
        ic: 0,
    }, // 0x67
    GlyphMetric {
        w: 472,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x68
    GlyphMetric {
        w: 472,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x69
    GlyphMetric {
        w: 528,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x6A
    GlyphMetric {
        w: 528,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x6B
    GlyphMetric {
        w: 528,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x6C
    GlyphMetric {
        w: 528,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x6D
    GlyphMetric {
        w: 667,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x6E
    GlyphMetric {
        w: 667,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x6F
    GlyphMetric {
        w: 1000,
        h: 40,
        d: 1160,
        ic: 0,
    }, // 0x70
    GlyphMetric {
        w: 1000,
        h: 40,
        d: 1760,
        ic: 0,
    }, // 0x71
    GlyphMetric {
        w: 1000,
        h: 40,
        d: 2360,
        ic: 0,
    }, // 0x72
    GlyphMetric {
        w: 1000,
        h: 40,
        d: 2960,
        ic: 0,
    }, // 0x73
    GlyphMetric {
        w: 1056,
        h: 0,
        d: 1800,
        ic: 0,
    }, // 0x74
    GlyphMetric {
        w: 1056,
        h: 0,
        d: 600,
        ic: 0,
    }, // 0x75
    GlyphMetric {
        w: 1056,
        h: 40,
        d: 560,
        ic: 0,
    }, // 0x76
    GlyphMetric {
        w: 778,
        h: 0,
        d: 600,
        ic: 0,
    }, // 0x77
    GlyphMetric {
        w: 667,
        h: 0,
        d: 600,
        ic: 0,
    }, // 0x78
    GlyphMetric {
        w: 667,
        h: 0,
        d: 600,
        ic: 0,
    }, // 0x79
    GlyphMetric {
        w: 450,
        h: 120,
        d: 0,
        ic: 0,
    }, // 0x7A
    GlyphMetric {
        w: 450,
        h: 120,
        d: 0,
        ic: 0,
    }, // 0x7B
    GlyphMetric {
        w: 450,
        h: 120,
        d: 0,
        ic: 0,
    }, // 0x7C
    GlyphMetric {
        w: 450,
        h: 120,
        d: 0,
        ic: 0,
    }, // 0x7D
    GlyphMetric {
        w: 778,
        h: 0,
        d: 600,
        ic: 0,
    }, // 0x7E
    GlyphMetric {
        w: 778,
        h: 0,
        d: 600,
        ic: 0,
    }, // 0x7F
];
/// CMMI10 (Math Italic, Family 1) metric table
pub static CMMI10_METRICS: [GlyphMetric; 128] = [
    GlyphMetric {
        w: 615,
        h: 683,
        d: 0,
        ic: 139,
    }, // 0x00
    GlyphMetric {
        w: 833,
        h: 683,
        d: 0,
        ic: 0,
    }, // 0x01
    GlyphMetric {
        w: 763,
        h: 683,
        d: 0,
        ic: 28,
    }, // 0x02
    GlyphMetric {
        w: 694,
        h: 683,
        d: 0,
        ic: 0,
    }, // 0x03
    GlyphMetric {
        w: 742,
        h: 683,
        d: 0,
        ic: 76,
    }, // 0x04
    GlyphMetric {
        w: 831,
        h: 683,
        d: 0,
        ic: 81,
    }, // 0x05
    GlyphMetric {
        w: 780,
        h: 683,
        d: 0,
        ic: 58,
    }, // 0x06
    GlyphMetric {
        w: 583,
        h: 683,
        d: 0,
        ic: 139,
    }, // 0x07
    GlyphMetric {
        w: 667,
        h: 683,
        d: 0,
        ic: 0,
    }, // 0x08
    GlyphMetric {
        w: 612,
        h: 683,
        d: 0,
        ic: 110,
    }, // 0x09
    GlyphMetric {
        w: 772,
        h: 683,
        d: 0,
        ic: 50,
    }, // 0x0A
    GlyphMetric {
        w: 640,
        h: 431,
        d: 0,
        ic: 4,
    }, // 0x0B
    GlyphMetric {
        w: 566,
        h: 694,
        d: 194,
        ic: 53,
    }, // 0x0C
    GlyphMetric {
        w: 518,
        h: 431,
        d: 194,
        ic: 56,
    }, // 0x0D
    GlyphMetric {
        w: 444,
        h: 694,
        d: 0,
        ic: 38,
    }, // 0x0E
    GlyphMetric {
        w: 406,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x0F
    GlyphMetric {
        w: 438,
        h: 694,
        d: 194,
        ic: 74,
    }, // 0x10
    GlyphMetric {
        w: 497,
        h: 431,
        d: 194,
        ic: 36,
    }, // 0x11
    GlyphMetric {
        w: 469,
        h: 694,
        d: 0,
        ic: 28,
    }, // 0x12
    GlyphMetric {
        w: 354,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x13
    GlyphMetric {
        w: 576,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x14
    GlyphMetric {
        w: 583,
        h: 694,
        d: 0,
        ic: 0,
    }, // 0x15
    GlyphMetric {
        w: 603,
        h: 431,
        d: 194,
        ic: 0,
    }, // 0x16
    GlyphMetric {
        w: 494,
        h: 431,
        d: 0,
        ic: 64,
    }, // 0x17
    GlyphMetric {
        w: 438,
        h: 694,
        d: 194,
        ic: 46,
    }, // 0x18
    GlyphMetric {
        w: 570,
        h: 431,
        d: 0,
        ic: 36,
    }, // 0x19
    GlyphMetric {
        w: 517,
        h: 431,
        d: 194,
        ic: 0,
    }, // 0x1A
    GlyphMetric {
        w: 571,
        h: 431,
        d: 0,
        ic: 36,
    }, // 0x1B
    GlyphMetric {
        w: 437,
        h: 431,
        d: 0,
        ic: 113,
    }, // 0x1C
    GlyphMetric {
        w: 540,
        h: 431,
        d: 0,
        ic: 36,
    }, // 0x1D
    GlyphMetric {
        w: 596,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x1E
    GlyphMetric {
        w: 626,
        h: 431,
        d: 194,
        ic: 0,
    }, // 0x1F
    GlyphMetric {
        w: 651,
        h: 694,
        d: 194,
        ic: 36,
    }, // 0x20
    GlyphMetric {
        w: 622,
        h: 431,
        d: 0,
        ic: 36,
    }, // 0x21
    GlyphMetric {
        w: 466,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x22
    GlyphMetric {
        w: 591,
        h: 694,
        d: 0,
        ic: 0,
    }, // 0x23
    GlyphMetric {
        w: 828,
        h: 431,
        d: 0,
        ic: 28,
    }, // 0x24
    GlyphMetric {
        w: 517,
        h: 431,
        d: 194,
        ic: 0,
    }, // 0x25
    GlyphMetric {
        w: 363,
        h: 431,
        d: 97,
        ic: 80,
    }, // 0x26
    GlyphMetric {
        w: 654,
        h: 431,
        d: 194,
        ic: 0,
    }, // 0x27
    GlyphMetric {
        w: 1000,
        h: 367,
        d: 0,
        ic: 0,
    }, // 0x28
    GlyphMetric {
        w: 1000,
        h: 367,
        d: 0,
        ic: 0,
    }, // 0x29
    GlyphMetric {
        w: 1000,
        h: 367,
        d: 0,
        ic: 0,
    }, // 0x2A
    GlyphMetric {
        w: 1000,
        h: 367,
        d: 0,
        ic: 0,
    }, // 0x2B
    GlyphMetric {
        w: 278,
        h: 464,
        d: 0,
        ic: 0,
    }, // 0x2C
    GlyphMetric {
        w: 278,
        h: 464,
        d: 0,
        ic: 0,
    }, // 0x2D
    GlyphMetric {
        w: 500,
        h: 465,
        d: 0,
        ic: 0,
    }, // 0x2E
    GlyphMetric {
        w: 500,
        h: 465,
        d: 0,
        ic: 0,
    }, // 0x2F
    GlyphMetric {
        w: 500,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x30
    GlyphMetric {
        w: 500,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x31
    GlyphMetric {
        w: 500,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x32
    GlyphMetric {
        w: 500,
        h: 431,
        d: 194,
        ic: 0,
    }, // 0x33
    GlyphMetric {
        w: 500,
        h: 431,
        d: 194,
        ic: 0,
    }, // 0x34
    GlyphMetric {
        w: 500,
        h: 431,
        d: 194,
        ic: 0,
    }, // 0x35
    GlyphMetric {
        w: 500,
        h: 644,
        d: 0,
        ic: 0,
    }, // 0x36
    GlyphMetric {
        w: 500,
        h: 431,
        d: 194,
        ic: 0,
    }, // 0x37
    GlyphMetric {
        w: 500,
        h: 644,
        d: 0,
        ic: 0,
    }, // 0x38
    GlyphMetric {
        w: 500,
        h: 431,
        d: 194,
        ic: 0,
    }, // 0x39
    GlyphMetric {
        w: 278,
        h: 106,
        d: 0,
        ic: 0,
    }, // 0x3A
    GlyphMetric {
        w: 278,
        h: 106,
        d: 194,
        ic: 0,
    }, // 0x3B
    GlyphMetric {
        w: 778,
        h: 539,
        d: 39,
        ic: 0,
    }, // 0x3C
    GlyphMetric {
        w: 500,
        h: 750,
        d: 250,
        ic: 0,
    }, // 0x3D
    GlyphMetric {
        w: 778,
        h: 539,
        d: 39,
        ic: 0,
    }, // 0x3E
    GlyphMetric {
        w: 500,
        h: 465,
        d: 0,
        ic: 0,
    }, // 0x3F
    GlyphMetric {
        w: 531,
        h: 694,
        d: 0,
        ic: 56,
    }, // 0x40
    GlyphMetric {
        w: 750,
        h: 683,
        d: 0,
        ic: 0,
    }, // 0x41
    GlyphMetric {
        w: 759,
        h: 683,
        d: 0,
        ic: 50,
    }, // 0x42
    GlyphMetric {
        w: 715,
        h: 683,
        d: 0,
        ic: 72,
    }, // 0x43
    GlyphMetric {
        w: 828,
        h: 683,
        d: 0,
        ic: 28,
    }, // 0x44
    GlyphMetric {
        w: 738,
        h: 683,
        d: 0,
        ic: 58,
    }, // 0x45
    GlyphMetric {
        w: 643,
        h: 683,
        d: 0,
        ic: 139,
    }, // 0x46
    GlyphMetric {
        w: 786,
        h: 683,
        d: 0,
        ic: 0,
    }, // 0x47
    GlyphMetric {
        w: 831,
        h: 683,
        d: 0,
        ic: 81,
    }, // 0x48
    GlyphMetric {
        w: 440,
        h: 683,
        d: 0,
        ic: 78,
    }, // 0x49
    GlyphMetric {
        w: 555,
        h: 683,
        d: 0,
        ic: 96,
    }, // 0x4A
    GlyphMetric {
        w: 849,
        h: 683,
        d: 0,
        ic: 72,
    }, // 0x4B
    GlyphMetric {
        w: 681,
        h: 683,
        d: 0,
        ic: 0,
    }, // 0x4C
    GlyphMetric {
        w: 970,
        h: 683,
        d: 0,
        ic: 109,
    }, // 0x4D
    GlyphMetric {
        w: 803,
        h: 683,
        d: 0,
        ic: 109,
    }, // 0x4E
    GlyphMetric {
        w: 763,
        h: 683,
        d: 0,
        ic: 28,
    }, // 0x4F
    GlyphMetric {
        w: 642,
        h: 683,
        d: 0,
        ic: 139,
    }, // 0x50
    GlyphMetric {
        w: 791,
        h: 683,
        d: 194,
        ic: 0,
    }, // 0x51
    GlyphMetric {
        w: 759,
        h: 683,
        d: 0,
        ic: 8,
    }, // 0x52
    GlyphMetric {
        w: 613,
        h: 683,
        d: 0,
        ic: 58,
    }, // 0x53
    GlyphMetric {
        w: 584,
        h: 683,
        d: 0,
        ic: 139,
    }, // 0x54
    GlyphMetric {
        w: 683,
        h: 683,
        d: 0,
        ic: 109,
    }, // 0x55
    GlyphMetric {
        w: 583,
        h: 683,
        d: 0,
        ic: 222,
    }, // 0x56
    GlyphMetric {
        w: 944,
        h: 683,
        d: 0,
        ic: 139,
    }, // 0x57
    GlyphMetric {
        w: 828,
        h: 683,
        d: 0,
        ic: 78,
    }, // 0x58
    GlyphMetric {
        w: 581,
        h: 683,
        d: 0,
        ic: 222,
    }, // 0x59
    GlyphMetric {
        w: 683,
        h: 683,
        d: 0,
        ic: 72,
    }, // 0x5A
    GlyphMetric {
        w: 389,
        h: 750,
        d: 0,
        ic: 0,
    }, // 0x5B
    GlyphMetric {
        w: 389,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x5C
    GlyphMetric {
        w: 389,
        h: 694,
        d: 194,
        ic: 0,
    }, // 0x5D
    GlyphMetric {
        w: 1000,
        h: 358,
        d: 0,
        ic: 0,
    }, // 0x5E
    GlyphMetric {
        w: 1000,
        h: 358,
        d: 0,
        ic: 0,
    }, // 0x5F
    GlyphMetric {
        w: 417,
        h: 694,
        d: 0,
        ic: 0,
    }, // 0x60
    GlyphMetric {
        w: 529,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x61
    GlyphMetric {
        w: 429,
        h: 694,
        d: 0,
        ic: 0,
    }, // 0x62
    GlyphMetric {
        w: 433,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x63
    GlyphMetric {
        w: 520,
        h: 694,
        d: 0,
        ic: 0,
    }, // 0x64
    GlyphMetric {
        w: 466,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x65
    GlyphMetric {
        w: 490,
        h: 694,
        d: 194,
        ic: 108,
    }, // 0x66
    GlyphMetric {
        w: 477,
        h: 431,
        d: 194,
        ic: 36,
    }, // 0x67
    GlyphMetric {
        w: 576,
        h: 694,
        d: 0,
        ic: 0,
    }, // 0x68
    GlyphMetric {
        w: 345,
        h: 660,
        d: 0,
        ic: 0,
    }, // 0x69
    GlyphMetric {
        w: 412,
        h: 660,
        d: 194,
        ic: 57,
    }, // 0x6A
    GlyphMetric {
        w: 521,
        h: 694,
        d: 0,
        ic: 31,
    }, // 0x6B
    GlyphMetric {
        w: 298,
        h: 694,
        d: 0,
        ic: 20,
    }, // 0x6C
    GlyphMetric {
        w: 878,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x6D
    GlyphMetric {
        w: 600,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x6E
    GlyphMetric {
        w: 485,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x6F
    GlyphMetric {
        w: 503,
        h: 431,
        d: 194,
        ic: 0,
    }, // 0x70
    GlyphMetric {
        w: 446,
        h: 431,
        d: 194,
        ic: 36,
    }, // 0x71
    GlyphMetric {
        w: 451,
        h: 431,
        d: 0,
        ic: 28,
    }, // 0x72
    GlyphMetric {
        w: 469,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x73
    GlyphMetric {
        w: 361,
        h: 615,
        d: 0,
        ic: 0,
    }, // 0x74
    GlyphMetric {
        w: 572,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x75
    GlyphMetric {
        w: 485,
        h: 431,
        d: 0,
        ic: 36,
    }, // 0x76
    GlyphMetric {
        w: 716,
        h: 431,
        d: 0,
        ic: 27,
    }, // 0x77
    GlyphMetric {
        w: 572,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x78
    GlyphMetric {
        w: 490,
        h: 431,
        d: 194,
        ic: 36,
    }, // 0x79
    GlyphMetric {
        w: 465,
        h: 431,
        d: 0,
        ic: 44,
    }, // 0x7A
    GlyphMetric {
        w: 322,
        h: 431,
        d: 0,
        ic: 0,
    }, // 0x7B
    GlyphMetric {
        w: 384,
        h: 431,
        d: 194,
        ic: 0,
    }, // 0x7C
    GlyphMetric {
        w: 636,
        h: 431,
        d: 194,
        ic: 0,
    }, // 0x7D
    GlyphMetric {
        w: 500,
        h: 714,
        d: 0,
        ic: 154,
    }, // 0x7E
    GlyphMetric {
        w: 278,
        h: 694,
        d: 0,
        ic: 399,
    }, // 0x7F
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
        CMSY10_METRICS[code as usize]
    } else {
        GlyphMetric {
            w: 0,
            h: 0,
            d: 0,
            ic: 0,
        }
    }
}

/// O(1) CMEX10 glyph metric lookup.
#[inline(always)]
pub fn cmex10_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMEX10_METRICS[code as usize]
    } else {
        GlyphMetric {
            w: 0,
            h: 0,
            d: 0,
            ic: 0,
        }
    }
}

/// O(1) CMMI10 glyph metric lookup.
#[inline(always)]
pub fn cmmi10_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMMI10_METRICS[code as usize]
    } else {
        GlyphMetric {
            w: 0,
            h: 0,
            d: 0,
            ic: 0,
        }
    }
}
