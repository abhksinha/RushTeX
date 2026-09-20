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
        ic: 195,
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

pub static CMBX10_METRICS: [GlyphMetric; 128] = [
    GlyphMetric { w: 692, h: 686, d: 0, ic: 0 }, // 0x00
    GlyphMetric { w: 958, h: 686, d: 0, ic: 0 }, // 0x01
    GlyphMetric { w: 894, h: 686, d: 0, ic: 0 }, // 0x02
    GlyphMetric { w: 806, h: 686, d: 0, ic: 0 }, // 0x03
    GlyphMetric { w: 767, h: 686, d: 0, ic: 0 }, // 0x04
    GlyphMetric { w: 900, h: 686, d: 0, ic: 0 }, // 0x05
    GlyphMetric { w: 831, h: 686, d: 0, ic: 0 }, // 0x06
    GlyphMetric { w: 894, h: 686, d: 0, ic: 0 }, // 0x07
    GlyphMetric { w: 831, h: 686, d: 0, ic: 0 }, // 0x08
    GlyphMetric { w: 894, h: 686, d: 0, ic: 0 }, // 0x09
    GlyphMetric { w: 831, h: 686, d: 0, ic: 0 }, // 0x0A
    GlyphMetric { w: 671, h: 694, d: 0, ic: 109 }, // 0x0B
    GlyphMetric { w: 639, h: 694, d: 0, ic: 0 }, // 0x0C
    GlyphMetric { w: 639, h: 694, d: 0, ic: 0 }, // 0x0D
    GlyphMetric { w: 958, h: 694, d: 0, ic: 0 }, // 0x0E
    GlyphMetric { w: 958, h: 694, d: 0, ic: 0 }, // 0x0F
    GlyphMetric { w: 319, h: 444, d: 0, ic: 0 }, // 0x10
    GlyphMetric { w: 351, h: 444, d: 194, ic: 0 }, // 0x11
    GlyphMetric { w: 575, h: 694, d: 0, ic: 0 }, // 0x12
    GlyphMetric { w: 575, h: 694, d: 0, ic: 0 }, // 0x13
    GlyphMetric { w: 575, h: 632, d: 0, ic: 0 }, // 0x14
    GlyphMetric { w: 575, h: 694, d: 0, ic: 0 }, // 0x15
    GlyphMetric { w: 575, h: 596, d: 0, ic: 0 }, // 0x16
    GlyphMetric { w: 869, h: 694, d: 0, ic: 0 }, // 0x17
    GlyphMetric { w: 511, h: 0, d: 170, ic: 0 }, // 0x18
    GlyphMetric { w: 597, h: 694, d: 0, ic: 0 }, // 0x19
    GlyphMetric { w: 831, h: 444, d: 0, ic: 0 }, // 0x1A
    GlyphMetric { w: 894, h: 444, d: 0, ic: 0 }, // 0x1B
    GlyphMetric { w: 575, h: 542, d: 97, ic: 0 }, // 0x1C
    GlyphMetric { w: 1042, h: 686, d: 0, ic: 0 }, // 0x1D
    GlyphMetric { w: 1169, h: 686, d: 0, ic: 0 }, // 0x1E
    GlyphMetric { w: 894, h: 735, d: 49, ic: 0 }, // 0x1F
    GlyphMetric { w: 319, h: 444, d: 0, ic: 0 }, // 0x20
    GlyphMetric { w: 350, h: 694, d: 0, ic: 0 }, // 0x21
    GlyphMetric { w: 603, h: 694, d: 0, ic: 0 }, // 0x22
    GlyphMetric { w: 958, h: 694, d: 194, ic: 0 }, // 0x23
    GlyphMetric { w: 575, h: 750, d: 56, ic: 0 }, // 0x24
    GlyphMetric { w: 958, h: 750, d: 56, ic: 0 }, // 0x25
    GlyphMetric { w: 894, h: 694, d: 0, ic: 0 }, // 0x26
    GlyphMetric { w: 319, h: 694, d: 0, ic: 0 }, // 0x27
    GlyphMetric { w: 447, h: 750, d: 250, ic: 0 }, // 0x28
    GlyphMetric { w: 447, h: 750, d: 250, ic: 0 }, // 0x29
    GlyphMetric { w: 575, h: 750, d: 0, ic: 0 }, // 0x2A
    GlyphMetric { w: 894, h: 633, d: 133, ic: 0 }, // 0x2B
    GlyphMetric { w: 319, h: 156, d: 194, ic: 0 }, // 0x2C
    GlyphMetric { w: 383, h: 444, d: 0, ic: 0 }, // 0x2D
    GlyphMetric { w: 319, h: 156, d: 0, ic: 0 }, // 0x2E
    GlyphMetric { w: 575, h: 750, d: 250, ic: 0 }, // 0x2F
    GlyphMetric { w: 575, h: 644, d: 0, ic: 0 }, // 0x30
    GlyphMetric { w: 575, h: 644, d: 0, ic: 0 }, // 0x31
    GlyphMetric { w: 575, h: 644, d: 0, ic: 0 }, // 0x32
    GlyphMetric { w: 575, h: 644, d: 0, ic: 0 }, // 0x33
    GlyphMetric { w: 575, h: 644, d: 0, ic: 0 }, // 0x34
    GlyphMetric { w: 575, h: 644, d: 0, ic: 0 }, // 0x35
    GlyphMetric { w: 575, h: 644, d: 0, ic: 0 }, // 0x36
    GlyphMetric { w: 575, h: 644, d: 0, ic: 0 }, // 0x37
    GlyphMetric { w: 575, h: 644, d: 0, ic: 0 }, // 0x38
    GlyphMetric { w: 575, h: 644, d: 0, ic: 0 }, // 0x39
    GlyphMetric { w: 319, h: 444, d: 0, ic: 0 }, // 0x3A
    GlyphMetric { w: 319, h: 444, d: 194, ic: 0 }, // 0x3B
    GlyphMetric { w: 350, h: 500, d: 194, ic: 0 }, // 0x3C
    GlyphMetric { w: 894, h: 391, d: 0, ic: 0 }, // 0x3D
    GlyphMetric { w: 543, h: 500, d: 194, ic: 0 }, // 0x3E
    GlyphMetric { w: 543, h: 694, d: 0, ic: 0 }, // 0x3F
    GlyphMetric { w: 894, h: 694, d: 0, ic: 0 }, // 0x40
    GlyphMetric { w: 869, h: 686, d: 0, ic: 0 }, // 0x41
    GlyphMetric { w: 818, h: 686, d: 0, ic: 0 }, // 0x42
    GlyphMetric { w: 831, h: 686, d: 0, ic: 0 }, // 0x43
    GlyphMetric { w: 882, h: 686, d: 0, ic: 0 }, // 0x44
    GlyphMetric { w: 756, h: 686, d: 0, ic: 0 }, // 0x45
    GlyphMetric { w: 724, h: 686, d: 0, ic: 0 }, // 0x46
    GlyphMetric { w: 904, h: 686, d: 0, ic: 0 }, // 0x47
    GlyphMetric { w: 900, h: 686, d: 0, ic: 0 }, // 0x48
    GlyphMetric { w: 436, h: 686, d: 0, ic: 0 }, // 0x49
    GlyphMetric { w: 594, h: 686, d: 0, ic: 0 }, // 0x4A
    GlyphMetric { w: 901, h: 686, d: 0, ic: 0 }, // 0x4B
    GlyphMetric { w: 692, h: 686, d: 0, ic: 0 }, // 0x4C
    GlyphMetric { w: 1092, h: 686, d: 0, ic: 0 }, // 0x4D
    GlyphMetric { w: 900, h: 686, d: 0, ic: 0 }, // 0x4E
    GlyphMetric { w: 864, h: 686, d: 0, ic: 0 }, // 0x4F
    GlyphMetric { w: 786, h: 686, d: 0, ic: 0 }, // 0x50
    GlyphMetric { w: 864, h: 686, d: 194, ic: 0 }, // 0x51
    GlyphMetric { w: 862, h: 686, d: 0, ic: 0 }, // 0x52
    GlyphMetric { w: 639, h: 686, d: 0, ic: 0 }, // 0x53
    GlyphMetric { w: 800, h: 686, d: 0, ic: 0 }, // 0x54
    GlyphMetric { w: 885, h: 686, d: 0, ic: 0 }, // 0x55
    GlyphMetric { w: 869, h: 686, d: 0, ic: 16 }, // 0x56
    GlyphMetric { w: 1189, h: 686, d: 0, ic: 16 }, // 0x57
    GlyphMetric { w: 869, h: 686, d: 0, ic: 0 }, // 0x58
    GlyphMetric { w: 869, h: 686, d: 0, ic: 29 }, // 0x59
    GlyphMetric { w: 703, h: 686, d: 0, ic: 0 }, // 0x5A
    GlyphMetric { w: 319, h: 750, d: 250, ic: 0 }, // 0x5B
    GlyphMetric { w: 603, h: 694, d: 0, ic: 0 }, // 0x5C
    GlyphMetric { w: 319, h: 750, d: 250, ic: 0 }, // 0x5D
    GlyphMetric { w: 575, h: 694, d: 0, ic: 0 }, // 0x5E
    GlyphMetric { w: 319, h: 694, d: 0, ic: 0 }, // 0x5F
    GlyphMetric { w: 319, h: 694, d: 0, ic: 0 }, // 0x60
    GlyphMetric { w: 559, h: 444, d: 0, ic: 0 }, // 0x61
    GlyphMetric { w: 639, h: 694, d: 0, ic: 0 }, // 0x62
    GlyphMetric { w: 511, h: 444, d: 0, ic: 0 }, // 0x63
    GlyphMetric { w: 639, h: 694, d: 0, ic: 0 }, // 0x64
    GlyphMetric { w: 527, h: 444, d: 0, ic: 0 }, // 0x65
    GlyphMetric { w: 351, h: 694, d: 0, ic: 109 }, // 0x66
    GlyphMetric { w: 575, h: 444, d: 194, ic: 16 }, // 0x67
    GlyphMetric { w: 639, h: 694, d: 0, ic: 0 }, // 0x68
    GlyphMetric { w: 319, h: 694, d: 0, ic: 0 }, // 0x69
    GlyphMetric { w: 351, h: 694, d: 194, ic: 0 }, // 0x6A
    GlyphMetric { w: 607, h: 694, d: 0, ic: 0 }, // 0x6B
    GlyphMetric { w: 319, h: 694, d: 0, ic: 0 }, // 0x6C
    GlyphMetric { w: 958, h: 444, d: 0, ic: 0 }, // 0x6D
    GlyphMetric { w: 639, h: 444, d: 0, ic: 0 }, // 0x6E
    GlyphMetric { w: 575, h: 444, d: 0, ic: 0 }, // 0x6F
    GlyphMetric { w: 639, h: 444, d: 194, ic: 0 }, // 0x70
    GlyphMetric { w: 607, h: 444, d: 194, ic: 0 }, // 0x71
    GlyphMetric { w: 474, h: 444, d: 0, ic: 0 }, // 0x72
    GlyphMetric { w: 454, h: 444, d: 0, ic: 0 }, // 0x73
    GlyphMetric { w: 447, h: 635, d: 0, ic: 0 }, // 0x74
    GlyphMetric { w: 639, h: 444, d: 0, ic: 0 }, // 0x75
    GlyphMetric { w: 607, h: 444, d: 0, ic: 16 }, // 0x76
    GlyphMetric { w: 831, h: 444, d: 0, ic: 16 }, // 0x77
    GlyphMetric { w: 607, h: 444, d: 0, ic: 0 }, // 0x78
    GlyphMetric { w: 607, h: 444, d: 194, ic: 16 }, // 0x79
    GlyphMetric { w: 511, h: 444, d: 0, ic: 0 }, // 0x7A
    GlyphMetric { w: 575, h: 444, d: 0, ic: 32 }, // 0x7B
    GlyphMetric { w: 1150, h: 444, d: 0, ic: 32 }, // 0x7C
    GlyphMetric { w: 575, h: 694, d: 0, ic: 0 }, // 0x7D
    GlyphMetric { w: 575, h: 694, d: 0, ic: 0 }, // 0x7E
    GlyphMetric { w: 575, h: 694, d: 0, ic: 0 }, // 0x7F
];

pub static CMTI10_METRICS: [GlyphMetric; 128] = [
    GlyphMetric { w: 627, h: 683, d: 0, ic: 133 }, // 0x00
    GlyphMetric { w: 818, h: 683, d: 0, ic: 0 }, // 0x01
    GlyphMetric { w: 767, h: 683, d: 0, ic: 94 }, // 0x02
    GlyphMetric { w: 692, h: 683, d: 0, ic: 0 }, // 0x03
    GlyphMetric { w: 664, h: 683, d: 0, ic: 153 }, // 0x04
    GlyphMetric { w: 743, h: 683, d: 0, ic: 164 }, // 0x05
    GlyphMetric { w: 716, h: 683, d: 0, ic: 120 }, // 0x06
    GlyphMetric { w: 767, h: 683, d: 0, ic: 111 }, // 0x07
    GlyphMetric { w: 716, h: 683, d: 0, ic: 60 }, // 0x08
    GlyphMetric { w: 767, h: 683, d: 0, ic: 111 }, // 0x09
    GlyphMetric { w: 716, h: 683, d: 0, ic: 103 }, // 0x0A
    GlyphMetric { w: 613, h: 694, d: 194, ic: 212 }, // 0x0B
    GlyphMetric { w: 562, h: 694, d: 194, ic: 103 }, // 0x0C
    GlyphMetric { w: 588, h: 694, d: 194, ic: 103 }, // 0x0D
    GlyphMetric { w: 882, h: 694, d: 194, ic: 103 }, // 0x0E
    GlyphMetric { w: 894, h: 694, d: 194, ic: 103 }, // 0x0F
    GlyphMetric { w: 307, h: 431, d: 0, ic: 77 }, // 0x10
    GlyphMetric { w: 332, h: 431, d: 194, ic: 37 }, // 0x11
    GlyphMetric { w: 511, h: 694, d: 0, ic: 0 }, // 0x12
    GlyphMetric { w: 511, h: 694, d: 0, ic: 97 }, // 0x13
    GlyphMetric { w: 511, h: 628, d: 0, ic: 83 }, // 0x14
    GlyphMetric { w: 511, h: 694, d: 0, ic: 108 }, // 0x15
    GlyphMetric { w: 511, h: 562, d: 0, ic: 103 }, // 0x16
    GlyphMetric { w: 831, h: 694, d: 0, ic: 0 }, // 0x17
    GlyphMetric { w: 460, h: 0, d: 170, ic: 0 }, // 0x18
    GlyphMetric { w: 537, h: 694, d: 194, ic: 105 }, // 0x19
    GlyphMetric { w: 716, h: 431, d: 0, ic: 75 }, // 0x1A
    GlyphMetric { w: 716, h: 431, d: 0, ic: 75 }, // 0x1B
    GlyphMetric { w: 511, h: 528, d: 97, ic: 92 }, // 0x1C
    GlyphMetric { w: 883, h: 683, d: 0, ic: 120 }, // 0x1D
    GlyphMetric { w: 985, h: 683, d: 0, ic: 120 }, // 0x1E
    GlyphMetric { w: 767, h: 732, d: 49, ic: 94 }, // 0x1F
    GlyphMetric { w: 256, h: 431, d: 0, ic: 0 }, // 0x20
    GlyphMetric { w: 307, h: 694, d: 0, ic: 124 }, // 0x21
    GlyphMetric { w: 514, h: 694, d: 0, ic: 70 }, // 0x22
    GlyphMetric { w: 818, h: 694, d: 194, ic: 66 }, // 0x23
    GlyphMetric { w: 769, h: 694, d: 0, ic: 0 }, // 0x24
    GlyphMetric { w: 818, h: 750, d: 56, ic: 136 }, // 0x25
    GlyphMetric { w: 767, h: 694, d: 0, ic: 97 }, // 0x26
    GlyphMetric { w: 307, h: 694, d: 0, ic: 124 }, // 0x27
    GlyphMetric { w: 409, h: 750, d: 250, ic: 162 }, // 0x28
    GlyphMetric { w: 409, h: 750, d: 250, ic: 37 }, // 0x29
    GlyphMetric { w: 511, h: 750, d: 0, ic: 149 }, // 0x2A
    GlyphMetric { w: 767, h: 562, d: 57, ic: 37 }, // 0x2B
    GlyphMetric { w: 307, h: 106, d: 194, ic: 0 }, // 0x2C
    GlyphMetric { w: 358, h: 431, d: 0, ic: 28 }, // 0x2D
    GlyphMetric { w: 307, h: 106, d: 0, ic: 0 }, // 0x2E
    GlyphMetric { w: 511, h: 750, d: 250, ic: 162 }, // 0x2F
    GlyphMetric { w: 511, h: 644, d: 0, ic: 136 }, // 0x30
    GlyphMetric { w: 511, h: 644, d: 0, ic: 136 }, // 0x31
    GlyphMetric { w: 511, h: 644, d: 0, ic: 136 }, // 0x32
    GlyphMetric { w: 511, h: 644, d: 0, ic: 136 }, // 0x33
    GlyphMetric { w: 511, h: 644, d: 194, ic: 136 }, // 0x34
    GlyphMetric { w: 511, h: 644, d: 0, ic: 136 }, // 0x35
    GlyphMetric { w: 511, h: 644, d: 0, ic: 136 }, // 0x36
    GlyphMetric { w: 511, h: 644, d: 194, ic: 136 }, // 0x37
    GlyphMetric { w: 511, h: 644, d: 0, ic: 136 }, // 0x38
    GlyphMetric { w: 511, h: 644, d: 0, ic: 136 }, // 0x39
    GlyphMetric { w: 307, h: 431, d: 0, ic: 58 }, // 0x3A
    GlyphMetric { w: 307, h: 431, d: 194, ic: 58 }, // 0x3B
    GlyphMetric { w: 307, h: 500, d: 194, ic: 76 }, // 0x3C
    GlyphMetric { w: 767, h: 367, d: 0, ic: 66 }, // 0x3D
    GlyphMetric { w: 511, h: 500, d: 194, ic: 0 }, // 0x3E
    GlyphMetric { w: 511, h: 694, d: 0, ic: 123 }, // 0x3F
    GlyphMetric { w: 767, h: 694, d: 0, ic: 96 }, // 0x40
    GlyphMetric { w: 743, h: 683, d: 0, ic: 0 }, // 0x41
    GlyphMetric { w: 704, h: 683, d: 0, ic: 103 }, // 0x42
    GlyphMetric { w: 716, h: 683, d: 0, ic: 145 }, // 0x43
    GlyphMetric { w: 755, h: 683, d: 0, ic: 94 }, // 0x44
    GlyphMetric { w: 678, h: 683, d: 0, ic: 120 }, // 0x45
    GlyphMetric { w: 653, h: 683, d: 0, ic: 133 }, // 0x46
    GlyphMetric { w: 774, h: 683, d: 0, ic: 87 }, // 0x47
    GlyphMetric { w: 743, h: 683, d: 0, ic: 164 }, // 0x48
    GlyphMetric { w: 386, h: 683, d: 0, ic: 158 }, // 0x49
    GlyphMetric { w: 525, h: 683, d: 0, ic: 140 }, // 0x4A
    GlyphMetric { w: 769, h: 683, d: 0, ic: 145 }, // 0x4B
    GlyphMetric { w: 627, h: 683, d: 0, ic: 0 }, // 0x4C
    GlyphMetric { w: 897, h: 683, d: 0, ic: 164 }, // 0x4D
    GlyphMetric { w: 743, h: 683, d: 0, ic: 164 }, // 0x4E
    GlyphMetric { w: 767, h: 683, d: 0, ic: 94 }, // 0x4F
    GlyphMetric { w: 678, h: 683, d: 0, ic: 103 }, // 0x50
    GlyphMetric { w: 767, h: 683, d: 194, ic: 94 }, // 0x51
    GlyphMetric { w: 729, h: 683, d: 0, ic: 39 }, // 0x52
    GlyphMetric { w: 562, h: 683, d: 0, ic: 120 }, // 0x53
    GlyphMetric { w: 716, h: 683, d: 0, ic: 133 }, // 0x54
    GlyphMetric { w: 743, h: 683, d: 0, ic: 164 }, // 0x55
    GlyphMetric { w: 743, h: 683, d: 0, ic: 184 }, // 0x56
    GlyphMetric { w: 999, h: 683, d: 0, ic: 184 }, // 0x57
    GlyphMetric { w: 743, h: 683, d: 0, ic: 158 }, // 0x58
    GlyphMetric { w: 743, h: 683, d: 0, ic: 194 }, // 0x59
    GlyphMetric { w: 613, h: 683, d: 0, ic: 145 }, // 0x5A
    GlyphMetric { w: 307, h: 750, d: 250, ic: 188 }, // 0x5B
    GlyphMetric { w: 514, h: 694, d: 0, ic: 168 }, // 0x5C
    GlyphMetric { w: 307, h: 750, d: 250, ic: 105 }, // 0x5D
    GlyphMetric { w: 511, h: 694, d: 0, ic: 66 }, // 0x5E
    GlyphMetric { w: 307, h: 668, d: 0, ic: 118 }, // 0x5F
    GlyphMetric { w: 307, h: 694, d: 0, ic: 124 }, // 0x60
    GlyphMetric { w: 511, h: 431, d: 0, ic: 77 }, // 0x61
    GlyphMetric { w: 460, h: 694, d: 0, ic: 63 }, // 0x62
    GlyphMetric { w: 460, h: 431, d: 0, ic: 57 }, // 0x63
    GlyphMetric { w: 511, h: 694, d: 0, ic: 103 }, // 0x64
    GlyphMetric { w: 460, h: 431, d: 0, ic: 75 }, // 0x65
    GlyphMetric { w: 307, h: 694, d: 194, ic: 212 }, // 0x66
    GlyphMetric { w: 460, h: 431, d: 194, ic: 88 }, // 0x67
    GlyphMetric { w: 511, h: 694, d: 0, ic: 77 }, // 0x68
    GlyphMetric { w: 307, h: 655, d: 0, ic: 102 }, // 0x69
    GlyphMetric { w: 307, h: 655, d: 194, ic: 145 }, // 0x6A
    GlyphMetric { w: 460, h: 694, d: 0, ic: 108 }, // 0x6B
    GlyphMetric { w: 256, h: 694, d: 0, ic: 103 }, // 0x6C
    GlyphMetric { w: 818, h: 431, d: 0, ic: 77 }, // 0x6D
    GlyphMetric { w: 562, h: 431, d: 0, ic: 77 }, // 0x6E
    GlyphMetric { w: 511, h: 431, d: 0, ic: 63 }, // 0x6F
    GlyphMetric { w: 511, h: 431, d: 194, ic: 63 }, // 0x70
    GlyphMetric { w: 460, h: 431, d: 194, ic: 88 }, // 0x71
    GlyphMetric { w: 422, h: 431, d: 0, ic: 108 }, // 0x72
    GlyphMetric { w: 409, h: 431, d: 0, ic: 82 }, // 0x73
    GlyphMetric { w: 332, h: 615, d: 0, ic: 95 }, // 0x74
    GlyphMetric { w: 537, h: 431, d: 0, ic: 77 }, // 0x75
    GlyphMetric { w: 460, h: 431, d: 0, ic: 108 }, // 0x76
    GlyphMetric { w: 664, h: 431, d: 0, ic: 108 }, // 0x77
    GlyphMetric { w: 464, h: 431, d: 0, ic: 120 }, // 0x78
    GlyphMetric { w: 486, h: 431, d: 194, ic: 88 }, // 0x79
    GlyphMetric { w: 409, h: 431, d: 0, ic: 123 }, // 0x7A
    GlyphMetric { w: 511, h: 431, d: 0, ic: 92 }, // 0x7B
    GlyphMetric { w: 1022, h: 431, d: 0, ic: 92 }, // 0x7C
    GlyphMetric { w: 511, h: 694, d: 0, ic: 123 }, // 0x7D
    GlyphMetric { w: 511, h: 668, d: 0, ic: 116 }, // 0x7E
    GlyphMetric { w: 511, h: 668, d: 0, ic: 105 }, // 0x7F
];



/// O(1) CMBX10 glyph metric lookup.
#[inline(always)]
pub fn cmbx10_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMBX10_METRICS[code as usize]
    } else {
        GlyphMetric {
            w: 0,
            h: 0,
            d: 0,
            ic: 0,
        }
    }
}

/// O(1) CMTI10 glyph metric lookup.
#[inline(always)]
pub fn cmti10_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMTI10_METRICS[code as usize]
    } else {
        GlyphMetric {
            w: 0,
            h: 0,
            d: 0,
            ic: 0,
        }
    }
}


pub static CMR10_METRICS: [GlyphMetric; 128] = [
    GlyphMetric { w: 625, h: 683, d: 0, ic: 0 }, // 0x00
    GlyphMetric { w: 833, h: 683, d: 0, ic: 0 }, // 0x01
    GlyphMetric { w: 778, h: 683, d: 0, ic: 0 }, // 0x02
    GlyphMetric { w: 694, h: 683, d: 0, ic: 0 }, // 0x03
    GlyphMetric { w: 667, h: 683, d: 0, ic: 0 }, // 0x04
    GlyphMetric { w: 750, h: 683, d: 0, ic: 0 }, // 0x05
    GlyphMetric { w: 722, h: 683, d: 0, ic: 0 }, // 0x06
    GlyphMetric { w: 778, h: 683, d: 0, ic: 0 }, // 0x07
    GlyphMetric { w: 722, h: 683, d: 0, ic: 0 }, // 0x08
    GlyphMetric { w: 778, h: 683, d: 0, ic: 0 }, // 0x09
    GlyphMetric { w: 722, h: 683, d: 0, ic: 0 }, // 0x0A
    GlyphMetric { w: 583, h: 694, d: 0, ic: 78 }, // 0x0B
    GlyphMetric { w: 556, h: 694, d: 0, ic: 0 }, // 0x0C
    GlyphMetric { w: 556, h: 694, d: 0, ic: 0 }, // 0x0D
    GlyphMetric { w: 833, h: 694, d: 0, ic: 0 }, // 0x0E
    GlyphMetric { w: 833, h: 694, d: 0, ic: 0 }, // 0x0F
    GlyphMetric { w: 278, h: 431, d: 0, ic: 0 }, // 0x10
    GlyphMetric { w: 306, h: 431, d: 194, ic: 0 }, // 0x11
    GlyphMetric { w: 500, h: 694, d: 0, ic: 0 }, // 0x12
    GlyphMetric { w: 500, h: 694, d: 0, ic: 0 }, // 0x13
    GlyphMetric { w: 500, h: 628, d: 0, ic: 0 }, // 0x14
    GlyphMetric { w: 500, h: 694, d: 0, ic: 0 }, // 0x15
    GlyphMetric { w: 500, h: 568, d: 0, ic: 0 }, // 0x16
    GlyphMetric { w: 750, h: 694, d: 0, ic: 0 }, // 0x17
    GlyphMetric { w: 444, h: 0, d: 170, ic: 0 }, // 0x18
    GlyphMetric { w: 500, h: 694, d: 0, ic: 0 }, // 0x19
    GlyphMetric { w: 722, h: 431, d: 0, ic: 0 }, // 0x1A
    GlyphMetric { w: 778, h: 431, d: 0, ic: 0 }, // 0x1B
    GlyphMetric { w: 500, h: 528, d: 97, ic: 0 }, // 0x1C
    GlyphMetric { w: 903, h: 683, d: 0, ic: 0 }, // 0x1D
    GlyphMetric { w: 1014, h: 683, d: 0, ic: 0 }, // 0x1E
    GlyphMetric { w: 778, h: 732, d: 49, ic: 0 }, // 0x1F
    GlyphMetric { w: 278, h: 431, d: 0, ic: 0 }, // 0x20
    GlyphMetric { w: 278, h: 694, d: 0, ic: 0 }, // 0x21
    GlyphMetric { w: 500, h: 694, d: 0, ic: 0 }, // 0x22
    GlyphMetric { w: 833, h: 694, d: 194, ic: 0 }, // 0x23
    GlyphMetric { w: 500, h: 750, d: 56, ic: 0 }, // 0x24
    GlyphMetric { w: 833, h: 750, d: 56, ic: 0 }, // 0x25
    GlyphMetric { w: 778, h: 694, d: 0, ic: 0 }, // 0x26
    GlyphMetric { w: 278, h: 694, d: 0, ic: 0 }, // 0x27
    GlyphMetric { w: 389, h: 750, d: 250, ic: 0 }, // 0x28
    GlyphMetric { w: 389, h: 750, d: 250, ic: 0 }, // 0x29
    GlyphMetric { w: 500, h: 750, d: 0, ic: 0 }, // 0x2A
    GlyphMetric { w: 778, h: 583, d: 83, ic: 0 }, // 0x2B
    GlyphMetric { w: 278, h: 106, d: 194, ic: 0 }, // 0x2C
    GlyphMetric { w: 333, h: 431, d: 0, ic: 0 }, // 0x2D
    GlyphMetric { w: 278, h: 106, d: 0, ic: 0 }, // 0x2E
    GlyphMetric { w: 500, h: 750, d: 250, ic: 0 }, // 0x2F
    GlyphMetric { w: 500, h: 644, d: 0, ic: 0 }, // 0x30
    GlyphMetric { w: 500, h: 644, d: 0, ic: 0 }, // 0x31
    GlyphMetric { w: 500, h: 644, d: 0, ic: 0 }, // 0x32
    GlyphMetric { w: 500, h: 644, d: 0, ic: 0 }, // 0x33
    GlyphMetric { w: 500, h: 644, d: 0, ic: 0 }, // 0x34
    GlyphMetric { w: 500, h: 644, d: 0, ic: 0 }, // 0x35
    GlyphMetric { w: 500, h: 644, d: 0, ic: 0 }, // 0x36
    GlyphMetric { w: 500, h: 644, d: 0, ic: 0 }, // 0x37
    GlyphMetric { w: 500, h: 644, d: 0, ic: 0 }, // 0x38
    GlyphMetric { w: 500, h: 644, d: 0, ic: 0 }, // 0x39
    GlyphMetric { w: 278, h: 431, d: 0, ic: 0 }, // 0x3A
    GlyphMetric { w: 278, h: 431, d: 194, ic: 0 }, // 0x3B
    GlyphMetric { w: 278, h: 500, d: 194, ic: 0 }, // 0x3C
    GlyphMetric { w: 778, h: 367, d: 0, ic: 0 }, // 0x3D
    GlyphMetric { w: 472, h: 500, d: 194, ic: 0 }, // 0x3E
    GlyphMetric { w: 472, h: 694, d: 0, ic: 0 }, // 0x3F
    GlyphMetric { w: 778, h: 694, d: 0, ic: 0 }, // 0x40
    GlyphMetric { w: 750, h: 683, d: 0, ic: 0 }, // 0x41
    GlyphMetric { w: 708, h: 683, d: 0, ic: 0 }, // 0x42
    GlyphMetric { w: 722, h: 683, d: 0, ic: 0 }, // 0x43
    GlyphMetric { w: 764, h: 683, d: 0, ic: 0 }, // 0x44
    GlyphMetric { w: 681, h: 683, d: 0, ic: 0 }, // 0x45
    GlyphMetric { w: 653, h: 683, d: 0, ic: 0 }, // 0x46
    GlyphMetric { w: 785, h: 683, d: 0, ic: 0 }, // 0x47
    GlyphMetric { w: 750, h: 683, d: 0, ic: 0 }, // 0x48
    GlyphMetric { w: 361, h: 683, d: 0, ic: 0 }, // 0x49
    GlyphMetric { w: 514, h: 683, d: 0, ic: 0 }, // 0x4A
    GlyphMetric { w: 778, h: 683, d: 0, ic: 0 }, // 0x4B
    GlyphMetric { w: 625, h: 683, d: 0, ic: 0 }, // 0x4C
    GlyphMetric { w: 917, h: 683, d: 0, ic: 0 }, // 0x4D
    GlyphMetric { w: 750, h: 683, d: 0, ic: 0 }, // 0x4E
    GlyphMetric { w: 778, h: 683, d: 0, ic: 0 }, // 0x4F
    GlyphMetric { w: 681, h: 683, d: 0, ic: 0 }, // 0x50
    GlyphMetric { w: 778, h: 683, d: 194, ic: 0 }, // 0x51
    GlyphMetric { w: 736, h: 683, d: 0, ic: 0 }, // 0x52
    GlyphMetric { w: 556, h: 683, d: 0, ic: 0 }, // 0x53
    GlyphMetric { w: 722, h: 683, d: 0, ic: 0 }, // 0x54
    GlyphMetric { w: 750, h: 683, d: 0, ic: 0 }, // 0x55
    GlyphMetric { w: 750, h: 683, d: 0, ic: 14 }, // 0x56
    GlyphMetric { w: 1028, h: 683, d: 0, ic: 14 }, // 0x57
    GlyphMetric { w: 750, h: 683, d: 0, ic: 0 }, // 0x58
    GlyphMetric { w: 750, h: 683, d: 0, ic: 25 }, // 0x59
    GlyphMetric { w: 611, h: 683, d: 0, ic: 0 }, // 0x5A
    GlyphMetric { w: 278, h: 750, d: 250, ic: 0 }, // 0x5B
    GlyphMetric { w: 500, h: 694, d: 0, ic: 0 }, // 0x5C
    GlyphMetric { w: 278, h: 750, d: 250, ic: 0 }, // 0x5D
    GlyphMetric { w: 500, h: 694, d: 0, ic: 0 }, // 0x5E
    GlyphMetric { w: 278, h: 668, d: 0, ic: 0 }, // 0x5F
    GlyphMetric { w: 278, h: 694, d: 0, ic: 0 }, // 0x60
    GlyphMetric { w: 500, h: 431, d: 0, ic: 0 }, // 0x61
    GlyphMetric { w: 556, h: 694, d: 0, ic: 0 }, // 0x62
    GlyphMetric { w: 444, h: 431, d: 0, ic: 0 }, // 0x63
    GlyphMetric { w: 556, h: 694, d: 0, ic: 0 }, // 0x64
    GlyphMetric { w: 444, h: 431, d: 0, ic: 0 }, // 0x65
    GlyphMetric { w: 306, h: 694, d: 0, ic: 78 }, // 0x66
    GlyphMetric { w: 500, h: 431, d: 194, ic: 14 }, // 0x67
    GlyphMetric { w: 556, h: 694, d: 0, ic: 0 }, // 0x68
    GlyphMetric { w: 278, h: 668, d: 0, ic: 0 }, // 0x69
    GlyphMetric { w: 306, h: 668, d: 194, ic: 0 }, // 0x6A
    GlyphMetric { w: 528, h: 694, d: 0, ic: 0 }, // 0x6B
    GlyphMetric { w: 278, h: 694, d: 0, ic: 0 }, // 0x6C
    GlyphMetric { w: 833, h: 431, d: 0, ic: 0 }, // 0x6D
    GlyphMetric { w: 556, h: 431, d: 0, ic: 0 }, // 0x6E
    GlyphMetric { w: 500, h: 431, d: 0, ic: 0 }, // 0x6F
    GlyphMetric { w: 556, h: 431, d: 194, ic: 0 }, // 0x70
    GlyphMetric { w: 528, h: 431, d: 194, ic: 0 }, // 0x71
    GlyphMetric { w: 392, h: 431, d: 0, ic: 0 }, // 0x72
    GlyphMetric { w: 394, h: 431, d: 0, ic: 0 }, // 0x73
    GlyphMetric { w: 389, h: 615, d: 0, ic: 0 }, // 0x74
    GlyphMetric { w: 556, h: 431, d: 0, ic: 0 }, // 0x75
    GlyphMetric { w: 528, h: 431, d: 0, ic: 14 }, // 0x76
    GlyphMetric { w: 722, h: 431, d: 0, ic: 14 }, // 0x77
    GlyphMetric { w: 528, h: 431, d: 0, ic: 0 }, // 0x78
    GlyphMetric { w: 528, h: 431, d: 194, ic: 14 }, // 0x79
    GlyphMetric { w: 444, h: 431, d: 0, ic: 0 }, // 0x7A
    GlyphMetric { w: 500, h: 431, d: 0, ic: 28 }, // 0x7B
    GlyphMetric { w: 1000, h: 431, d: 0, ic: 28 }, // 0x7C
    GlyphMetric { w: 500, h: 694, d: 0, ic: 0 }, // 0x7D
    GlyphMetric { w: 500, h: 668, d: 0, ic: 0 }, // 0x7E
    GlyphMetric { w: 500, h: 668, d: 0, ic: 0 }, // 0x7F
];

pub static CMR7_METRICS: [GlyphMetric; 128] = [
    GlyphMetric { w: 706, h: 683, d: 0, ic: 0 }, // 0x00
    GlyphMetric { w: 938, h: 683, d: 0, ic: 0 }, // 0x01
    GlyphMetric { w: 877, h: 683, d: 0, ic: 0 }, // 0x02
    GlyphMetric { w: 782, h: 683, d: 0, ic: 0 }, // 0x03
    GlyphMetric { w: 754, h: 683, d: 0, ic: 0 }, // 0x04
    GlyphMetric { w: 843, h: 683, d: 0, ic: 0 }, // 0x05
    GlyphMetric { w: 815, h: 683, d: 0, ic: 0 }, // 0x06
    GlyphMetric { w: 877, h: 683, d: 0, ic: 0 }, // 0x07
    GlyphMetric { w: 815, h: 683, d: 0, ic: 0 }, // 0x08
    GlyphMetric { w: 877, h: 683, d: 0, ic: 0 }, // 0x09
    GlyphMetric { w: 815, h: 683, d: 0, ic: 0 }, // 0x0A
    GlyphMetric { w: 678, h: 694, d: 0, ic: 80 }, // 0x0B
    GlyphMetric { w: 647, h: 694, d: 0, ic: 0 }, // 0x0C
    GlyphMetric { w: 647, h: 694, d: 0, ic: 0 }, // 0x0D
    GlyphMetric { w: 970, h: 694, d: 0, ic: 0 }, // 0x0E
    GlyphMetric { w: 970, h: 694, d: 0, ic: 0 }, // 0x0F
    GlyphMetric { w: 323, h: 431, d: 0, ic: 0 }, // 0x10
    GlyphMetric { w: 354, h: 431, d: 194, ic: 0 }, // 0x11
    GlyphMetric { w: 569, h: 694, d: 0, ic: 0 }, // 0x12
    GlyphMetric { w: 569, h: 694, d: 0, ic: 0 }, // 0x13
    GlyphMetric { w: 569, h: 628, d: 0, ic: 0 }, // 0x14
    GlyphMetric { w: 569, h: 694, d: 0, ic: 0 }, // 0x15
    GlyphMetric { w: 569, h: 574, d: 0, ic: 0 }, // 0x16
    GlyphMetric { w: 843, h: 694, d: 0, ic: 0 }, // 0x17
    GlyphMetric { w: 508, h: 0, d: 170, ic: 0 }, // 0x18
    GlyphMetric { w: 569, h: 694, d: 0, ic: 0 }, // 0x19
    GlyphMetric { w: 815, h: 431, d: 0, ic: 0 }, // 0x1A
    GlyphMetric { w: 877, h: 431, d: 0, ic: 0 }, // 0x1B
    GlyphMetric { w: 569, h: 528, d: 97, ic: 0 }, // 0x1C
    GlyphMetric { w: 1014, h: 683, d: 0, ic: 0 }, // 0x1D
    GlyphMetric { w: 1137, h: 683, d: 0, ic: 0 }, // 0x1E
    GlyphMetric { w: 877, h: 732, d: 49, ic: 0 }, // 0x1F
    GlyphMetric { w: 323, h: 431, d: 0, ic: 0 }, // 0x20
    GlyphMetric { w: 323, h: 694, d: 0, ic: 0 }, // 0x21
    GlyphMetric { w: 569, h: 694, d: 0, ic: 0 }, // 0x22
    GlyphMetric { w: 938, h: 694, d: 194, ic: 0 }, // 0x23
    GlyphMetric { w: 569, h: 750, d: 56, ic: 0 }, // 0x24
    GlyphMetric { w: 938, h: 750, d: 56, ic: 0 }, // 0x25
    GlyphMetric { w: 877, h: 694, d: 0, ic: 0 }, // 0x26
    GlyphMetric { w: 323, h: 694, d: 0, ic: 0 }, // 0x27
    GlyphMetric { w: 446, h: 750, d: 250, ic: 0 }, // 0x28
    GlyphMetric { w: 446, h: 750, d: 250, ic: 0 }, // 0x29
    GlyphMetric { w: 569, h: 750, d: 0, ic: 0 }, // 0x2A
    GlyphMetric { w: 877, h: 619, d: 119, ic: 0 }, // 0x2B
    GlyphMetric { w: 323, h: 115, d: 194, ic: 0 }, // 0x2C
    GlyphMetric { w: 385, h: 431, d: 0, ic: 0 }, // 0x2D
    GlyphMetric { w: 323, h: 115, d: 0, ic: 0 }, // 0x2E
    GlyphMetric { w: 569, h: 750, d: 250, ic: 0 }, // 0x2F
    GlyphMetric { w: 569, h: 644, d: 0, ic: 0 }, // 0x30
    GlyphMetric { w: 569, h: 644, d: 0, ic: 0 }, // 0x31
    GlyphMetric { w: 569, h: 644, d: 0, ic: 0 }, // 0x32
    GlyphMetric { w: 569, h: 644, d: 0, ic: 0 }, // 0x33
    GlyphMetric { w: 569, h: 644, d: 0, ic: 0 }, // 0x34
    GlyphMetric { w: 569, h: 644, d: 0, ic: 0 }, // 0x35
    GlyphMetric { w: 569, h: 644, d: 0, ic: 0 }, // 0x36
    GlyphMetric { w: 569, h: 644, d: 0, ic: 0 }, // 0x37
    GlyphMetric { w: 569, h: 644, d: 0, ic: 0 }, // 0x38
    GlyphMetric { w: 569, h: 644, d: 0, ic: 0 }, // 0x39
    GlyphMetric { w: 323, h: 431, d: 0, ic: 0 }, // 0x3A
    GlyphMetric { w: 323, h: 431, d: 194, ic: 0 }, // 0x3B
    GlyphMetric { w: 323, h: 500, d: 194, ic: 0 }, // 0x3C
    GlyphMetric { w: 877, h: 384, d: 0, ic: 0 }, // 0x3D
    GlyphMetric { w: 539, h: 500, d: 194, ic: 0 }, // 0x3E
    GlyphMetric { w: 539, h: 694, d: 0, ic: 0 }, // 0x3F
    GlyphMetric { w: 877, h: 694, d: 0, ic: 0 }, // 0x40
    GlyphMetric { w: 843, h: 683, d: 0, ic: 0 }, // 0x41
    GlyphMetric { w: 799, h: 683, d: 0, ic: 0 }, // 0x42
    GlyphMetric { w: 815, h: 683, d: 0, ic: 0 }, // 0x43
    GlyphMetric { w: 860, h: 683, d: 0, ic: 0 }, // 0x44
    GlyphMetric { w: 768, h: 683, d: 0, ic: 0 }, // 0x45
    GlyphMetric { w: 737, h: 683, d: 0, ic: 0 }, // 0x46
    GlyphMetric { w: 884, h: 683, d: 0, ic: 0 }, // 0x47
    GlyphMetric { w: 843, h: 683, d: 0, ic: 0 }, // 0x48
    GlyphMetric { w: 413, h: 683, d: 0, ic: 0 }, // 0x49
    GlyphMetric { w: 583, h: 683, d: 0, ic: 0 }, // 0x4A
    GlyphMetric { w: 874, h: 683, d: 0, ic: 0 }, // 0x4B
    GlyphMetric { w: 706, h: 683, d: 0, ic: 0 }, // 0x4C
    GlyphMetric { w: 1028, h: 683, d: 0, ic: 0 }, // 0x4D
    GlyphMetric { w: 843, h: 683, d: 0, ic: 0 }, // 0x4E
    GlyphMetric { w: 877, h: 683, d: 0, ic: 0 }, // 0x4F
    GlyphMetric { w: 768, h: 683, d: 0, ic: 0 }, // 0x50
    GlyphMetric { w: 877, h: 683, d: 194, ic: 0 }, // 0x51
    GlyphMetric { w: 829, h: 683, d: 0, ic: 0 }, // 0x52
    GlyphMetric { w: 631, h: 683, d: 0, ic: 0 }, // 0x53
    GlyphMetric { w: 815, h: 683, d: 0, ic: 0 }, // 0x54
    GlyphMetric { w: 843, h: 683, d: 0, ic: 0 }, // 0x55
    GlyphMetric { w: 843, h: 683, d: 0, ic: 15 }, // 0x56
    GlyphMetric { w: 1151, h: 683, d: 0, ic: 15 }, // 0x57
    GlyphMetric { w: 843, h: 683, d: 0, ic: 0 }, // 0x58
    GlyphMetric { w: 843, h: 683, d: 0, ic: 28 }, // 0x59
    GlyphMetric { w: 692, h: 683, d: 0, ic: 0 }, // 0x5A
    GlyphMetric { w: 323, h: 750, d: 250, ic: 0 }, // 0x5B
    GlyphMetric { w: 569, h: 694, d: 0, ic: 0 }, // 0x5C
    GlyphMetric { w: 323, h: 750, d: 250, ic: 0 }, // 0x5D
    GlyphMetric { w: 569, h: 694, d: 0, ic: 0 }, // 0x5E
    GlyphMetric { w: 323, h: 673, d: 0, ic: 0 }, // 0x5F
    GlyphMetric { w: 323, h: 694, d: 0, ic: 0 }, // 0x60
    GlyphMetric { w: 569, h: 431, d: 0, ic: 0 }, // 0x61
    GlyphMetric { w: 631, h: 694, d: 0, ic: 0 }, // 0x62
    GlyphMetric { w: 508, h: 431, d: 0, ic: 0 }, // 0x63
    GlyphMetric { w: 631, h: 694, d: 0, ic: 0 }, // 0x64
    GlyphMetric { w: 508, h: 431, d: 0, ic: 0 }, // 0x65
    GlyphMetric { w: 354, h: 694, d: 0, ic: 80 }, // 0x66
    GlyphMetric { w: 569, h: 431, d: 194, ic: 15 }, // 0x67
    GlyphMetric { w: 631, h: 694, d: 0, ic: 0 }, // 0x68
    GlyphMetric { w: 323, h: 673, d: 0, ic: 0 }, // 0x69
    GlyphMetric { w: 354, h: 673, d: 194, ic: 0 }, // 0x6A
    GlyphMetric { w: 600, h: 694, d: 0, ic: 0 }, // 0x6B
    GlyphMetric { w: 323, h: 694, d: 0, ic: 0 }, // 0x6C
    GlyphMetric { w: 938, h: 431, d: 0, ic: 0 }, // 0x6D
    GlyphMetric { w: 631, h: 431, d: 0, ic: 0 }, // 0x6E
    GlyphMetric { w: 569, h: 431, d: 0, ic: 0 }, // 0x6F
    GlyphMetric { w: 631, h: 431, d: 194, ic: 0 }, // 0x70
    GlyphMetric { w: 600, h: 431, d: 194, ic: 0 }, // 0x71
    GlyphMetric { w: 446, h: 431, d: 0, ic: 0 }, // 0x72
    GlyphMetric { w: 453, h: 431, d: 0, ic: 0 }, // 0x73
    GlyphMetric { w: 446, h: 615, d: 0, ic: 0 }, // 0x74
    GlyphMetric { w: 631, h: 431, d: 0, ic: 0 }, // 0x75
    GlyphMetric { w: 600, h: 431, d: 0, ic: 15 }, // 0x76
    GlyphMetric { w: 815, h: 431, d: 0, ic: 15 }, // 0x77
    GlyphMetric { w: 600, h: 431, d: 0, ic: 0 }, // 0x78
    GlyphMetric { w: 600, h: 431, d: 194, ic: 15 }, // 0x79
    GlyphMetric { w: 508, h: 431, d: 0, ic: 0 }, // 0x7A
    GlyphMetric { w: 569, h: 431, d: 0, ic: 31 }, // 0x7B
    GlyphMetric { w: 1139, h: 431, d: 0, ic: 31 }, // 0x7C
    GlyphMetric { w: 569, h: 694, d: 0, ic: 0 }, // 0x7D
    GlyphMetric { w: 569, h: 673, d: 0, ic: 0 }, // 0x7E
    GlyphMetric { w: 569, h: 673, d: 0, ic: 0 }, // 0x7F
];

pub static CMR5_METRICS: [GlyphMetric; 128] = [
    GlyphMetric { w: 831, h: 680, d: 0, ic: 0 }, // 0x00
    GlyphMetric { w: 1097, h: 680, d: 0, ic: 0 }, // 0x01
    GlyphMetric { w: 1028, h: 680, d: 0, ic: 0 }, // 0x02
    GlyphMetric { w: 911, h: 680, d: 0, ic: 0 }, // 0x03
    GlyphMetric { w: 889, h: 680, d: 0, ic: 0 }, // 0x04
    GlyphMetric { w: 981, h: 680, d: 0, ic: 0 }, // 0x05
    GlyphMetric { w: 958, h: 680, d: 0, ic: 0 }, // 0x06
    GlyphMetric { w: 1028, h: 680, d: 0, ic: 0 }, // 0x07
    GlyphMetric { w: 958, h: 680, d: 0, ic: 0 }, // 0x08
    GlyphMetric { w: 1028, h: 680, d: 0, ic: 0 }, // 0x09
    GlyphMetric { w: 958, h: 680, d: 0, ic: 0 }, // 0x0A
    GlyphMetric { w: 681, h: 694, d: 194, ic: 0 }, // 0x0B
    GlyphMetric { w: 681, h: 694, d: 194, ic: 0 }, // 0x0C
    GlyphMetric { w: 403, h: 694, d: 0, ic: 0 }, // 0x0D
    GlyphMetric { w: 403, h: 500, d: 194, ic: 0 }, // 0x0E
    GlyphMetric { w: 646, h: 500, d: 194, ic: 0 }, // 0x0F
    GlyphMetric { w: 403, h: 431, d: 0, ic: 0 }, // 0x10
    GlyphMetric { w: 438, h: 431, d: 194, ic: 0 }, // 0x11
    GlyphMetric { w: 681, h: 694, d: 0, ic: 0 }, // 0x12
    GlyphMetric { w: 681, h: 694, d: 0, ic: 0 }, // 0x13
    GlyphMetric { w: 681, h: 628, d: 0, ic: 0 }, // 0x14
    GlyphMetric { w: 681, h: 694, d: 0, ic: 0 }, // 0x15
    GlyphMetric { w: 681, h: 581, d: 0, ic: 0 }, // 0x16
    GlyphMetric { w: 981, h: 694, d: 0, ic: 0 }, // 0x17
    GlyphMetric { w: 611, h: 0, d: 170, ic: 0 }, // 0x18
    GlyphMetric { w: 681, h: 694, d: 0, ic: 0 }, // 0x19
    GlyphMetric { w: 958, h: 431, d: 0, ic: 0 }, // 0x1A
    GlyphMetric { w: 1028, h: 431, d: 0, ic: 0 }, // 0x1B
    GlyphMetric { w: 681, h: 528, d: 97, ic: 0 }, // 0x1C
    GlyphMetric { w: 1178, h: 680, d: 0, ic: 0 }, // 0x1D
    GlyphMetric { w: 1317, h: 680, d: 0, ic: 0 }, // 0x1E
    GlyphMetric { w: 1028, h: 732, d: 49, ic: 0 }, // 0x1F
    GlyphMetric { w: 403, h: 431, d: 0, ic: 0 }, // 0x20
    GlyphMetric { w: 403, h: 694, d: 0, ic: 0 }, // 0x21
    GlyphMetric { w: 681, h: 694, d: 0, ic: 0 }, // 0x22
    GlyphMetric { w: 1097, h: 694, d: 194, ic: 0 }, // 0x23
    GlyphMetric { w: 681, h: 750, d: 56, ic: 0 }, // 0x24
    GlyphMetric { w: 1097, h: 750, d: 56, ic: 0 }, // 0x25
    GlyphMetric { w: 1028, h: 694, d: 0, ic: 0 }, // 0x26
    GlyphMetric { w: 403, h: 694, d: 0, ic: 0 }, // 0x27
    GlyphMetric { w: 542, h: 750, d: 250, ic: 0 }, // 0x28
    GlyphMetric { w: 542, h: 750, d: 250, ic: 0 }, // 0x29
    GlyphMetric { w: 681, h: 750, d: 0, ic: 0 }, // 0x2A
    GlyphMetric { w: 1028, h: 667, d: 167, ic: 0 }, // 0x2B
    GlyphMetric { w: 403, h: 122, d: 194, ic: 0 }, // 0x2C
    GlyphMetric { w: 472, h: 431, d: 0, ic: 0 }, // 0x2D
    GlyphMetric { w: 403, h: 122, d: 0, ic: 0 }, // 0x2E
    GlyphMetric { w: 681, h: 750, d: 250, ic: 0 }, // 0x2F
    GlyphMetric { w: 681, h: 644, d: 0, ic: 0 }, // 0x30
    GlyphMetric { w: 681, h: 644, d: 0, ic: 0 }, // 0x31
    GlyphMetric { w: 681, h: 644, d: 0, ic: 0 }, // 0x32
    GlyphMetric { w: 681, h: 644, d: 0, ic: 0 }, // 0x33
    GlyphMetric { w: 681, h: 644, d: 0, ic: 0 }, // 0x34
    GlyphMetric { w: 681, h: 644, d: 0, ic: 0 }, // 0x35
    GlyphMetric { w: 681, h: 644, d: 0, ic: 0 }, // 0x36
    GlyphMetric { w: 681, h: 644, d: 0, ic: 0 }, // 0x37
    GlyphMetric { w: 681, h: 644, d: 0, ic: 0 }, // 0x38
    GlyphMetric { w: 681, h: 644, d: 0, ic: 0 }, // 0x39
    GlyphMetric { w: 403, h: 431, d: 0, ic: 0 }, // 0x3A
    GlyphMetric { w: 403, h: 431, d: 194, ic: 0 }, // 0x3B
    GlyphMetric { w: 1028, h: 601, d: 101, ic: 0 }, // 0x3C
    GlyphMetric { w: 1028, h: 396, d: 0, ic: 0 }, // 0x3D
    GlyphMetric { w: 1028, h: 601, d: 101, ic: 0 }, // 0x3E
    GlyphMetric { w: 646, h: 694, d: 0, ic: 0 }, // 0x3F
    GlyphMetric { w: 1028, h: 694, d: 0, ic: 0 }, // 0x40
    GlyphMetric { w: 981, h: 680, d: 0, ic: 0 }, // 0x41
    GlyphMetric { w: 935, h: 680, d: 0, ic: 0 }, // 0x42
    GlyphMetric { w: 958, h: 680, d: 0, ic: 0 }, // 0x43
    GlyphMetric { w: 1004, h: 680, d: 0, ic: 0 }, // 0x44
    GlyphMetric { w: 900, h: 680, d: 0, ic: 0 }, // 0x45
    GlyphMetric { w: 865, h: 680, d: 0, ic: 0 }, // 0x46
    GlyphMetric { w: 1033, h: 680, d: 0, ic: 0 }, // 0x47
    GlyphMetric { w: 981, h: 680, d: 0, ic: 0 }, // 0x48
    GlyphMetric { w: 494, h: 680, d: 0, ic: 0 }, // 0x49
    GlyphMetric { w: 692, h: 680, d: 0, ic: 0 }, // 0x4A
    GlyphMetric { w: 1015, h: 680, d: 0, ic: 0 }, // 0x4B
    GlyphMetric { w: 831, h: 680, d: 0, ic: 0 }, // 0x4C
    GlyphMetric { w: 1189, h: 680, d: 0, ic: 0 }, // 0x4D
    GlyphMetric { w: 981, h: 680, d: 0, ic: 0 }, // 0x4E
    GlyphMetric { w: 1028, h: 680, d: 0, ic: 0 }, // 0x4F
    GlyphMetric { w: 900, h: 680, d: 0, ic: 0 }, // 0x50
    GlyphMetric { w: 1028, h: 680, d: 194, ic: 0 }, // 0x51
    GlyphMetric { w: 969, h: 680, d: 0, ic: 0 }, // 0x52
    GlyphMetric { w: 750, h: 680, d: 0, ic: 0 }, // 0x53
    GlyphMetric { w: 958, h: 680, d: 0, ic: 0 }, // 0x54
    GlyphMetric { w: 981, h: 680, d: 0, ic: 0 }, // 0x55
    GlyphMetric { w: 981, h: 680, d: 0, ic: 17 }, // 0x56
    GlyphMetric { w: 1328, h: 680, d: 0, ic: 17 }, // 0x57
    GlyphMetric { w: 981, h: 680, d: 0, ic: 0 }, // 0x58
    GlyphMetric { w: 981, h: 680, d: 0, ic: 31 }, // 0x59
    GlyphMetric { w: 819, h: 680, d: 0, ic: 0 }, // 0x5A
    GlyphMetric { w: 403, h: 750, d: 250, ic: 0 }, // 0x5B
    GlyphMetric { w: 681, h: 694, d: 0, ic: 0 }, // 0x5C
    GlyphMetric { w: 403, h: 750, d: 250, ic: 0 }, // 0x5D
    GlyphMetric { w: 681, h: 694, d: 0, ic: 0 }, // 0x5E
    GlyphMetric { w: 403, h: 680, d: 0, ic: 0 }, // 0x5F
    GlyphMetric { w: 403, h: 694, d: 0, ic: 0 }, // 0x60
    GlyphMetric { w: 681, h: 431, d: 0, ic: 0 }, // 0x61
    GlyphMetric { w: 750, h: 694, d: 0, ic: 0 }, // 0x62
    GlyphMetric { w: 611, h: 431, d: 0, ic: 0 }, // 0x63
    GlyphMetric { w: 750, h: 694, d: 0, ic: 0 }, // 0x64
    GlyphMetric { w: 611, h: 431, d: 0, ic: 0 }, // 0x65
    GlyphMetric { w: 438, h: 694, d: 0, ic: 88 }, // 0x66
    GlyphMetric { w: 681, h: 431, d: 194, ic: 17 }, // 0x67
    GlyphMetric { w: 750, h: 694, d: 0, ic: 0 }, // 0x68
    GlyphMetric { w: 403, h: 680, d: 0, ic: 0 }, // 0x69
    GlyphMetric { w: 438, h: 680, d: 194, ic: 0 }, // 0x6A
    GlyphMetric { w: 715, h: 694, d: 0, ic: 0 }, // 0x6B
    GlyphMetric { w: 403, h: 694, d: 0, ic: 0 }, // 0x6C
    GlyphMetric { w: 1097, h: 431, d: 0, ic: 0 }, // 0x6D
    GlyphMetric { w: 750, h: 431, d: 0, ic: 0 }, // 0x6E
    GlyphMetric { w: 681, h: 431, d: 0, ic: 0 }, // 0x6F
    GlyphMetric { w: 750, h: 431, d: 194, ic: 0 }, // 0x70
    GlyphMetric { w: 715, h: 431, d: 194, ic: 0 }, // 0x71
    GlyphMetric { w: 542, h: 431, d: 0, ic: 0 }, // 0x72
    GlyphMetric { w: 549, h: 431, d: 0, ic: 0 }, // 0x73
    GlyphMetric { w: 542, h: 615, d: 0, ic: 0 }, // 0x74
    GlyphMetric { w: 750, h: 431, d: 0, ic: 0 }, // 0x75
    GlyphMetric { w: 715, h: 431, d: 0, ic: 17 }, // 0x76
    GlyphMetric { w: 958, h: 431, d: 0, ic: 17 }, // 0x77
    GlyphMetric { w: 715, h: 431, d: 0, ic: 0 }, // 0x78
    GlyphMetric { w: 715, h: 431, d: 194, ic: 17 }, // 0x79
    GlyphMetric { w: 611, h: 431, d: 0, ic: 0 }, // 0x7A
    GlyphMetric { w: 681, h: 431, d: 0, ic: 35 }, // 0x7B
    GlyphMetric { w: 1361, h: 431, d: 0, ic: 35 }, // 0x7C
    GlyphMetric { w: 681, h: 694, d: 0, ic: 0 }, // 0x7D
    GlyphMetric { w: 681, h: 680, d: 0, ic: 0 }, // 0x7E
    GlyphMetric { w: 681, h: 680, d: 0, ic: 0 }, // 0x7F
];

pub static CMMI7_METRICS: [GlyphMetric; 128] = [
    GlyphMetric { w: 694, h: 683, d: 0, ic: 154 }, // 0x00
    GlyphMetric { w: 954, h: 683, d: 0, ic: 0 }, // 0x01
    GlyphMetric { w: 869, h: 683, d: 0, ic: 31 }, // 0x02
    GlyphMetric { w: 798, h: 683, d: 0, ic: 0 }, // 0x03
    GlyphMetric { w: 844, h: 683, d: 0, ic: 75 }, // 0x04
    GlyphMetric { w: 936, h: 683, d: 0, ic: 76 }, // 0x05
    GlyphMetric { w: 886, h: 683, d: 0, ic: 55 }, // 0x06
    GlyphMetric { w: 678, h: 683, d: 0, ic: 154 }, // 0x07
    GlyphMetric { w: 770, h: 683, d: 0, ic: 0 }, // 0x08
    GlyphMetric { w: 717, h: 683, d: 0, ic: 114 }, // 0x09
    GlyphMetric { w: 880, h: 683, d: 0, ic: 49 }, // 0x0A
    GlyphMetric { w: 743, h: 431, d: 0, ic: 0 }, // 0x0B
    GlyphMetric { w: 648, h: 694, d: 194, ic: 51 }, // 0x0C
    GlyphMetric { w: 600, h: 431, d: 194, ic: 62 }, // 0x0D
    GlyphMetric { w: 519, h: 694, d: 0, ic: 36 }, // 0x0E
    GlyphMetric { w: 476, h: 431, d: 0, ic: 0 }, // 0x0F
    GlyphMetric { w: 520, h: 694, d: 194, ic: 65 }, // 0x10
    GlyphMetric { w: 589, h: 431, d: 194, ic: 36 }, // 0x11
    GlyphMetric { w: 544, h: 694, d: 0, ic: 31 }, // 0x12
    GlyphMetric { w: 423, h: 431, d: 0, ic: 0 }, // 0x13
    GlyphMetric { w: 669, h: 431, d: 0, ic: 0 }, // 0x14
    GlyphMetric { w: 678, h: 694, d: 0, ic: 0 }, // 0x15
    GlyphMetric { w: 695, h: 431, d: 194, ic: 0 }, // 0x16
    GlyphMetric { w: 573, h: 431, d: 0, ic: 67 }, // 0x17
    GlyphMetric { w: 520, h: 694, d: 194, ic: 34 }, // 0x18
    GlyphMetric { w: 668, h: 431, d: 0, ic: 36 }, // 0x19
    GlyphMetric { w: 593, h: 431, d: 194, ic: 0 }, // 0x1A
    GlyphMetric { w: 662, h: 431, d: 0, ic: 36 }, // 0x1B
    GlyphMetric { w: 527, h: 431, d: 0, ic: 116 }, // 0x1C
    GlyphMetric { w: 633, h: 431, d: 0, ic: 36 }, // 0x1D
    GlyphMetric { w: 687, h: 694, d: 194, ic: 0 }, // 0x1E
    GlyphMetric { w: 714, h: 431, d: 194, ic: 0 }, // 0x1F
    GlyphMetric { w: 756, h: 694, d: 194, ic: 36 }, // 0x20
    GlyphMetric { w: 720, h: 431, d: 0, ic: 36 }, // 0x21
    GlyphMetric { w: 540, h: 431, d: 0, ic: 0 }, // 0x22
    GlyphMetric { w: 690, h: 694, d: 0, ic: 0 }, // 0x23
    GlyphMetric { w: 950, h: 431, d: 0, ic: 31 }, // 0x24
    GlyphMetric { w: 593, h: 431, d: 194, ic: 0 }, // 0x25
    GlyphMetric { w: 439, h: 431, d: 97, ic: 77 }, // 0x26
    GlyphMetric { w: 751, h: 431, d: 194, ic: 0 }, // 0x27
    GlyphMetric { w: 1139, h: 384, d: 0, ic: 0 }, // 0x28
    GlyphMetric { w: 1139, h: 384, d: 0, ic: 0 }, // 0x29
    GlyphMetric { w: 1139, h: 384, d: 0, ic: 0 }, // 0x2A
    GlyphMetric { w: 1139, h: 384, d: 0, ic: 0 }, // 0x2B
    GlyphMetric { w: 339, h: 494, d: 0, ic: 0 }, // 0x2C
    GlyphMetric { w: 339, h: 494, d: 0, ic: 0 }, // 0x2D
    GlyphMetric { w: 585, h: 465, d: 0, ic: 0 }, // 0x2E
    GlyphMetric { w: 585, h: 465, d: 0, ic: 0 }, // 0x2F
    GlyphMetric { w: 585, h: 431, d: 0, ic: 0 }, // 0x30
    GlyphMetric { w: 585, h: 431, d: 0, ic: 0 }, // 0x31
    GlyphMetric { w: 585, h: 431, d: 0, ic: 0 }, // 0x32
    GlyphMetric { w: 585, h: 431, d: 194, ic: 0 }, // 0x33
    GlyphMetric { w: 585, h: 431, d: 194, ic: 0 }, // 0x34
    GlyphMetric { w: 585, h: 431, d: 194, ic: 0 }, // 0x35
    GlyphMetric { w: 585, h: 644, d: 0, ic: 0 }, // 0x36
    GlyphMetric { w: 585, h: 431, d: 194, ic: 0 }, // 0x37
    GlyphMetric { w: 585, h: 644, d: 0, ic: 0 }, // 0x38
    GlyphMetric { w: 585, h: 431, d: 194, ic: 0 }, // 0x39
    GlyphMetric { w: 339, h: 115, d: 0, ic: 0 }, // 0x3A
    GlyphMetric { w: 339, h: 115, d: 194, ic: 0 }, // 0x3B
    GlyphMetric { w: 893, h: 576, d: 76, ic: 0 }, // 0x3C
    GlyphMetric { w: 585, h: 750, d: 250, ic: 0 }, // 0x3D
    GlyphMetric { w: 893, h: 576, d: 76, ic: 0 }, // 0x3E
    GlyphMetric { w: 585, h: 465, d: 0, ic: 0 }, // 0x3F
    GlyphMetric { w: 610, h: 694, d: 0, ic: 62 }, // 0x40
    GlyphMetric { w: 859, h: 683, d: 0, ic: 0 }, // 0x41
    GlyphMetric { w: 863, h: 683, d: 0, ic: 49 }, // 0x42
    GlyphMetric { w: 819, h: 683, d: 0, ic: 70 }, // 0x43
    GlyphMetric { w: 934, h: 683, d: 0, ic: 31 }, // 0x44
    GlyphMetric { w: 839, h: 683, d: 0, ic: 55 }, // 0x45
    GlyphMetric { w: 725, h: 683, d: 0, ic: 154 }, // 0x46
    GlyphMetric { w: 889, h: 683, d: 0, ic: 0 }, // 0x47
    GlyphMetric { w: 936, h: 683, d: 0, ic: 76 }, // 0x48
    GlyphMetric { w: 506, h: 683, d: 0, ic: 78 }, // 0x49
    GlyphMetric { w: 632, h: 683, d: 0, ic: 94 }, // 0x4A
    GlyphMetric { w: 960, h: 683, d: 0, ic: 70 }, // 0x4B
    GlyphMetric { w: 784, h: 683, d: 0, ic: 0 }, // 0x4C
    GlyphMetric { w: 1089, h: 683, d: 0, ic: 107 }, // 0x4D
    GlyphMetric { w: 905, h: 683, d: 0, ic: 107 }, // 0x4E
    GlyphMetric { w: 869, h: 683, d: 0, ic: 31 }, // 0x4F
    GlyphMetric { w: 727, h: 683, d: 0, ic: 154 }, // 0x50
    GlyphMetric { w: 900, h: 683, d: 194, ic: 0 }, // 0x51
    GlyphMetric { w: 861, h: 683, d: 0, ic: 5 }, // 0x52
    GlyphMetric { w: 701, h: 683, d: 0, ic: 55 }, // 0x53
    GlyphMetric { w: 675, h: 683, d: 0, ic: 154 }, // 0x54
    GlyphMetric { w: 778, h: 683, d: 0, ic: 107 }, // 0x55
    GlyphMetric { w: 675, h: 683, d: 0, ic: 246 }, // 0x56
    GlyphMetric { w: 1074, h: 683, d: 0, ic: 154 }, // 0x57
    GlyphMetric { w: 937, h: 683, d: 0, ic: 78 }, // 0x58
    GlyphMetric { w: 672, h: 683, d: 0, ic: 246 }, // 0x59
    GlyphMetric { w: 778, h: 683, d: 0, ic: 70 }, // 0x5A
    GlyphMetric { w: 462, h: 750, d: 0, ic: 0 }, // 0x5B
    GlyphMetric { w: 462, h: 694, d: 194, ic: 0 }, // 0x5C
    GlyphMetric { w: 462, h: 694, d: 194, ic: 0 }, // 0x5D
    GlyphMetric { w: 1139, h: 358, d: 0, ic: 0 }, // 0x5E
    GlyphMetric { w: 1139, h: 358, d: 0, ic: 0 }, // 0x5F
    GlyphMetric { w: 478, h: 694, d: 0, ic: 0 }, // 0x60
    GlyphMetric { w: 620, h: 431, d: 0, ic: 0 }, // 0x61
    GlyphMetric { w: 502, h: 694, d: 0, ic: 0 }, // 0x62
    GlyphMetric { w: 511, h: 431, d: 0, ic: 0 }, // 0x63
    GlyphMetric { w: 595, h: 694, d: 0, ic: 0 }, // 0x64
    GlyphMetric { w: 542, h: 431, d: 0, ic: 0 }, // 0x65
    GlyphMetric { w: 557, h: 694, d: 194, ic: 112 }, // 0x66
    GlyphMetric { w: 557, h: 431, d: 194, ic: 36 }, // 0x67
    GlyphMetric { w: 669, h: 694, d: 0, ic: 0 }, // 0x68
    GlyphMetric { w: 404, h: 662, d: 0, ic: 0 }, // 0x69
    GlyphMetric { w: 473, h: 662, d: 194, ic: 58 }, // 0x6A
    GlyphMetric { w: 607, h: 694, d: 0, ic: 24 }, // 0x6B
    GlyphMetric { w: 361, h: 694, d: 0, ic: 6 }, // 0x6C
    GlyphMetric { w: 1014, h: 431, d: 0, ic: 0 }, // 0x6D
    GlyphMetric { w: 706, h: 431, d: 0, ic: 0 }, // 0x6E
    GlyphMetric { w: 564, h: 431, d: 0, ic: 0 }, // 0x6F
    GlyphMetric { w: 589, h: 431, d: 194, ic: 0 }, // 0x70
    GlyphMetric { w: 524, h: 431, d: 194, ic: 36 }, // 0x71
    GlyphMetric { w: 530, h: 431, d: 0, ic: 31 }, // 0x72
    GlyphMetric { w: 539, h: 431, d: 0, ic: 0 }, // 0x73
    GlyphMetric { w: 432, h: 615, d: 0, ic: 0 }, // 0x74
    GlyphMetric { w: 675, h: 431, d: 0, ic: 0 }, // 0x75
    GlyphMetric { w: 571, h: 431, d: 0, ic: 36 }, // 0x76
    GlyphMetric { w: 826, h: 431, d: 0, ic: 27 }, // 0x77
    GlyphMetric { w: 648, h: 431, d: 0, ic: 0 }, // 0x78
    GlyphMetric { w: 579, h: 431, d: 194, ic: 36 }, // 0x79
    GlyphMetric { w: 546, h: 431, d: 0, ic: 41 }, // 0x7A
    GlyphMetric { w: 399, h: 431, d: 0, ic: 0 }, // 0x7B
    GlyphMetric { w: 442, h: 431, d: 194, ic: 0 }, // 0x7C
    GlyphMetric { w: 730, h: 431, d: 194, ic: 0 }, // 0x7D
    GlyphMetric { w: 585, h: 719, d: 0, ic: 154 }, // 0x7E
    GlyphMetric { w: 339, h: 694, d: 0, ic: 462 }, // 0x7F
];

pub static CMMI5_METRICS: [GlyphMetric; 128] = [
    GlyphMetric { w: 833, h: 683, d: 0, ic: 174 }, // 0x00
    GlyphMetric { w: 1153, h: 683, d: 0, ic: 0 }, // 0x01
    GlyphMetric { w: 1048, h: 683, d: 0, ic: 35 }, // 0x02
    GlyphMetric { w: 967, h: 683, d: 0, ic: 0 }, // 0x03
    GlyphMetric { w: 1018, h: 683, d: 0, ic: 73 }, // 0x04
    GlyphMetric { w: 1110, h: 683, d: 0, ic: 74 }, // 0x05
    GlyphMetric { w: 1065, h: 683, d: 0, ic: 51 }, // 0x06
    GlyphMetric { w: 840, h: 683, d: 0, ic: 174 }, // 0x07
    GlyphMetric { w: 944, h: 683, d: 0, ic: 0 }, // 0x08
    GlyphMetric { w: 893, h: 683, d: 0, ic: 120 }, // 0x09
    GlyphMetric { w: 1061, h: 683, d: 0, ic: 47 }, // 0x0A
    GlyphMetric { w: 913, h: 431, d: 0, ic: 0 }, // 0x0B
    GlyphMetric { w: 791, h: 694, d: 194, ic: 51 }, // 0x0C
    GlyphMetric { w: 747, h: 431, d: 194, ic: 69 }, // 0x0D
    GlyphMetric { w: 654, h: 694, d: 0, ic: 32 }, // 0x0E
    GlyphMetric { w: 614, h: 431, d: 0, ic: 0 }, // 0x0F
    GlyphMetric { w: 667, h: 694, d: 194, ic: 53 }, // 0x10
    GlyphMetric { w: 744, h: 431, d: 194, ic: 36 }, // 0x11
    GlyphMetric { w: 677, h: 694, d: 0, ic: 35 }, // 0x12
    GlyphMetric { w: 550, h: 431, d: 0, ic: 0 }, // 0x13
    GlyphMetric { w: 828, h: 431, d: 0, ic: 0 }, // 0x14
    GlyphMetric { w: 840, h: 694, d: 0, ic: 0 }, // 0x15
    GlyphMetric { w: 850, h: 431, d: 194, ic: 0 }, // 0x16
    GlyphMetric { w: 712, h: 431, d: 0, ic: 71 }, // 0x17
    GlyphMetric { w: 667, h: 694, d: 194, ic: 18 }, // 0x18
    GlyphMetric { w: 831, h: 431, d: 0, ic: 36 }, // 0x19
    GlyphMetric { w: 726, h: 431, d: 194, ic: 0 }, // 0x1A
    GlyphMetric { w: 815, h: 431, d: 0, ic: 36 }, // 0x1B
    GlyphMetric { w: 682, h: 431, d: 0, ic: 116 }, // 0x1C
    GlyphMetric { w: 792, h: 431, d: 0, ic: 36 }, // 0x1D
    GlyphMetric { w: 842, h: 694, d: 194, ic: 0 }, // 0x1E
    GlyphMetric { w: 865, h: 431, d: 194, ic: 0 }, // 0x1F
    GlyphMetric { w: 931, h: 694, d: 194, ic: 36 }, // 0x20
    GlyphMetric { w: 886, h: 431, d: 0, ic: 36 }, // 0x21
    GlyphMetric { w: 675, h: 431, d: 0, ic: 0 }, // 0x22
    GlyphMetric { w: 855, h: 694, d: 0, ic: 0 }, // 0x23
    GlyphMetric { w: 1145, h: 431, d: 0, ic: 35 }, // 0x24
    GlyphMetric { w: 726, h: 431, d: 194, ic: 0 }, // 0x25
    GlyphMetric { w: 578, h: 431, d: 97, ic: 73 }, // 0x26
    GlyphMetric { w: 918, h: 431, d: 194, ic: 0 }, // 0x27
    GlyphMetric { w: 1361, h: 396, d: 0, ic: 0 }, // 0x28
    GlyphMetric { w: 1361, h: 396, d: 0, ic: 0 }, // 0x29
    GlyphMetric { w: 1361, h: 396, d: 0, ic: 0 }, // 0x2A
    GlyphMetric { w: 1361, h: 396, d: 0, ic: 0 }, // 0x2B
    GlyphMetric { w: 458, h: 515, d: 15, ic: 0 }, // 0x2C
    GlyphMetric { w: 458, h: 515, d: 15, ic: 0 }, // 0x2D
    GlyphMetric { w: 736, h: 465, d: 0, ic: 0 }, // 0x2E
    GlyphMetric { w: 736, h: 465, d: 0, ic: 0 }, // 0x2F
    GlyphMetric { w: 736, h: 431, d: 0, ic: 0 }, // 0x30
    GlyphMetric { w: 736, h: 431, d: 0, ic: 0 }, // 0x31
    GlyphMetric { w: 736, h: 431, d: 0, ic: 0 }, // 0x32
    GlyphMetric { w: 736, h: 431, d: 194, ic: 0 }, // 0x33
    GlyphMetric { w: 736, h: 431, d: 194, ic: 0 }, // 0x34
    GlyphMetric { w: 736, h: 431, d: 194, ic: 0 }, // 0x35
    GlyphMetric { w: 736, h: 644, d: 0, ic: 0 }, // 0x36
    GlyphMetric { w: 736, h: 431, d: 194, ic: 0 }, // 0x37
    GlyphMetric { w: 736, h: 644, d: 0, ic: 0 }, // 0x38
    GlyphMetric { w: 736, h: 431, d: 194, ic: 0 }, // 0x39
    GlyphMetric { w: 458, h: 122, d: 0, ic: 0 }, // 0x3A
    GlyphMetric { w: 458, h: 122, d: 194, ic: 0 }, // 0x3B
    GlyphMetric { w: 1083, h: 601, d: 101, ic: 0 }, // 0x3C
    GlyphMetric { w: 736, h: 750, d: 250, ic: 0 }, // 0x3D
    GlyphMetric { w: 1083, h: 601, d: 101, ic: 0 }, // 0x3E
    GlyphMetric { w: 736, h: 465, d: 0, ic: 0 }, // 0x3F
    GlyphMetric { w: 749, h: 694, d: 0, ic: 69 }, // 0x40
    GlyphMetric { w: 1036, h: 683, d: 0, ic: 0 }, // 0x41
    GlyphMetric { w: 1037, h: 683, d: 0, ic: 47 }, // 0x42
    GlyphMetric { w: 996, h: 683, d: 0, ic: 68 }, // 0x43
    GlyphMetric { w: 1110, h: 683, d: 0, ic: 35 }, // 0x44
    GlyphMetric { w: 1007, h: 683, d: 0, ic: 51 }, // 0x45
    GlyphMetric { w: 867, h: 683, d: 0, ic: 174 }, // 0x46
    GlyphMetric { w: 1064, h: 683, d: 0, ic: 0 }, // 0x47
    GlyphMetric { w: 1110, h: 683, d: 0, ic: 74 }, // 0x48
    GlyphMetric { w: 627, h: 683, d: 0, ic: 77 }, // 0x49
    GlyphMetric { w: 773, h: 683, d: 0, ic: 95 }, // 0x4A
    GlyphMetric { w: 1139, h: 683, d: 0, ic: 68 }, // 0x4B
    GlyphMetric { w: 956, h: 683, d: 0, ic: 0 }, // 0x4C
    GlyphMetric { w: 1284, h: 683, d: 0, ic: 109 }, // 0x4D
    GlyphMetric { w: 1076, h: 683, d: 0, ic: 109 }, // 0x4E
    GlyphMetric { w: 1048, h: 683, d: 0, ic: 35 }, // 0x4F
    GlyphMetric { w: 875, h: 683, d: 0, ic: 174 }, // 0x50
    GlyphMetric { w: 1082, h: 683, d: 194, ic: 0 }, // 0x51
    GlyphMetric { w: 1030, h: 683, d: 0, ic: 2 }, // 0x52
    GlyphMetric { w: 856, h: 683, d: 0, ic: 51 }, // 0x53
    GlyphMetric { w: 832, h: 683, d: 0, ic: 174 }, // 0x54
    GlyphMetric { w: 944, h: 683, d: 0, ic: 109 }, // 0x55
    GlyphMetric { w: 828, h: 683, d: 0, ic: 278 }, // 0x56
    GlyphMetric { w: 1279, h: 683, d: 0, ic: 174 }, // 0x57
    GlyphMetric { w: 1113, h: 683, d: 0, ic: 77 }, // 0x58
    GlyphMetric { w: 824, h: 683, d: 0, ic: 278 }, // 0x59
    GlyphMetric { w: 943, h: 683, d: 0, ic: 68 }, // 0x5A
    GlyphMetric { w: 597, h: 750, d: 0, ic: 0 }, // 0x5B
    GlyphMetric { w: 597, h: 694, d: 194, ic: 0 }, // 0x5C
    GlyphMetric { w: 597, h: 694, d: 194, ic: 0 }, // 0x5D
    GlyphMetric { w: 1361, h: 358, d: 0, ic: 0 }, // 0x5E
    GlyphMetric { w: 1361, h: 358, d: 0, ic: 0 }, // 0x5F
    GlyphMetric { w: 597, h: 694, d: 0, ic: 0 }, // 0x60
    GlyphMetric { w: 774, h: 431, d: 0, ic: 0 }, // 0x61
    GlyphMetric { w: 633, h: 694, d: 0, ic: 0 }, // 0x62
    GlyphMetric { w: 649, h: 431, d: 0, ic: 0 }, // 0x63
    GlyphMetric { w: 740, h: 694, d: 0, ic: 0 }, // 0x64
    GlyphMetric { w: 677, h: 431, d: 0, ic: 0 }, // 0x65
    GlyphMetric { w: 684, h: 694, d: 194, ic: 118 }, // 0x66
    GlyphMetric { w: 701, h: 431, d: 194, ic: 36 }, // 0x67
    GlyphMetric { w: 828, h: 694, d: 0, ic: 0 }, // 0x68
    GlyphMetric { w: 534, h: 666, d: 0, ic: 0 }, // 0x69
    GlyphMetric { w: 588, h: 666, d: 194, ic: 59 }, // 0x6A
    GlyphMetric { w: 758, h: 694, d: 0, ic: 16 }, // 0x6B
    GlyphMetric { w: 480, h: 694, d: 0, ic: 0 }, // 0x6C
    GlyphMetric { w: 1228, h: 431, d: 0, ic: 0 }, // 0x6D
    GlyphMetric { w: 881, h: 431, d: 0, ic: 0 }, // 0x6E
    GlyphMetric { w: 703, h: 431, d: 0, ic: 0 }, // 0x6F
    GlyphMetric { w: 740, h: 431, d: 194, ic: 0 }, // 0x70
    GlyphMetric { w: 659, h: 431, d: 194, ic: 36 }, // 0x71
    GlyphMetric { w: 671, h: 431, d: 0, ic: 35 }, // 0x72
    GlyphMetric { w: 670, h: 431, d: 0, ic: 0 }, // 0x73
    GlyphMetric { w: 564, h: 615, d: 0, ic: 0 }, // 0x74
    GlyphMetric { w: 846, h: 431, d: 0, ic: 0 }, // 0x75
    GlyphMetric { w: 722, h: 431, d: 0, ic: 36 }, // 0x76
    GlyphMetric { w: 1009, h: 431, d: 0, ic: 27 }, // 0x77
    GlyphMetric { w: 792, h: 431, d: 0, ic: 0 }, // 0x78
    GlyphMetric { w: 731, h: 431, d: 194, ic: 36 }, // 0x79
    GlyphMetric { w: 689, h: 431, d: 0, ic: 37 }, // 0x7A
    GlyphMetric { w: 534, h: 431, d: 0, ic: 0 }, // 0x7B
    GlyphMetric { w: 553, h: 431, d: 194, ic: 0 }, // 0x7C
    GlyphMetric { w: 889, h: 431, d: 194, ic: 0 }, // 0x7D
    GlyphMetric { w: 736, h: 722, d: 0, ic: 154 }, // 0x7E
    GlyphMetric { w: 458, h: 694, d: 0, ic: 581 }, // 0x7F
];

pub static CMSY7_METRICS: [GlyphMetric; 128] = [
    GlyphMetric { w: 893, h: 619, d: 119, ic: 0 }, // 0x00
    GlyphMetric { w: 339, h: 465, d: 0, ic: 0 }, // 0x01
    GlyphMetric { w: 893, h: 619, d: 119, ic: 0 }, // 0x02
    GlyphMetric { w: 585, h: 465, d: 0, ic: 0 }, // 0x03
    GlyphMetric { w: 893, h: 619, d: 119, ic: 0 }, // 0x04
    GlyphMetric { w: 585, h: 465, d: 0, ic: 0 }, // 0x05
    GlyphMetric { w: 893, h: 619, d: 119, ic: 0 }, // 0x06
    GlyphMetric { w: 893, h: 619, d: 119, ic: 0 }, // 0x07
    GlyphMetric { w: 893, h: 619, d: 119, ic: 0 }, // 0x08
    GlyphMetric { w: 893, h: 619, d: 119, ic: 0 }, // 0x09
    GlyphMetric { w: 893, h: 619, d: 119, ic: 0 }, // 0x0A
    GlyphMetric { w: 893, h: 619, d: 119, ic: 0 }, // 0x0B
    GlyphMetric { w: 893, h: 619, d: 119, ic: 0 }, // 0x0C
    GlyphMetric { w: 1139, h: 694, d: 194, ic: 0 }, // 0x0D
    GlyphMetric { w: 585, h: 465, d: 0, ic: 0 }, // 0x0E
    GlyphMetric { w: 585, h: 465, d: 0, ic: 0 }, // 0x0F
    GlyphMetric { w: 893, h: 494, d: 0, ic: 0 }, // 0x10
    GlyphMetric { w: 893, h: 494, d: 0, ic: 0 }, // 0x11
    GlyphMetric { w: 893, h: 685, d: 185, ic: 0 }, // 0x12
    GlyphMetric { w: 893, h: 685, d: 185, ic: 0 }, // 0x13
    GlyphMetric { w: 893, h: 685, d: 185, ic: 0 }, // 0x14
    GlyphMetric { w: 893, h: 685, d: 185, ic: 0 }, // 0x15
    GlyphMetric { w: 893, h: 685, d: 185, ic: 0 }, // 0x16
    GlyphMetric { w: 893, h: 685, d: 185, ic: 0 }, // 0x17
    GlyphMetric { w: 893, h: 384, d: 0, ic: 0 }, // 0x18
    GlyphMetric { w: 893, h: 516, d: 16, ic: 0 }, // 0x19
    GlyphMetric { w: 893, h: 576, d: 76, ic: 0 }, // 0x1A
    GlyphMetric { w: 893, h: 576, d: 76, ic: 0 }, // 0x1B
    GlyphMetric { w: 1139, h: 576, d: 76, ic: 0 }, // 0x1C
    GlyphMetric { w: 1139, h: 576, d: 76, ic: 0 }, // 0x1D
    GlyphMetric { w: 893, h: 576, d: 76, ic: 0 }, // 0x1E
    GlyphMetric { w: 893, h: 576, d: 76, ic: 0 }, // 0x1F
    GlyphMetric { w: 1139, h: 384, d: 0, ic: 0 }, // 0x20
    GlyphMetric { w: 1139, h: 384, d: 0, ic: 0 }, // 0x21
    GlyphMetric { w: 585, h: 694, d: 194, ic: 0 }, // 0x22
    GlyphMetric { w: 585, h: 694, d: 194, ic: 0 }, // 0x23
    GlyphMetric { w: 1139, h: 384, d: 0, ic: 0 }, // 0x24
    GlyphMetric { w: 1139, h: 694, d: 194, ic: 0 }, // 0x25
    GlyphMetric { w: 1139, h: 694, d: 194, ic: 0 }, // 0x26
    GlyphMetric { w: 893, h: 494, d: 0, ic: 0 }, // 0x27
    GlyphMetric { w: 1139, h: 384, d: 0, ic: 0 }, // 0x28
    GlyphMetric { w: 1139, h: 384, d: 0, ic: 0 }, // 0x29
    GlyphMetric { w: 708, h: 694, d: 194, ic: 0 }, // 0x2A
    GlyphMetric { w: 708, h: 694, d: 194, ic: 0 }, // 0x2B
    GlyphMetric { w: 1139, h: 384, d: 0, ic: 0 }, // 0x2C
    GlyphMetric { w: 1139, h: 694, d: 194, ic: 0 }, // 0x2D
    GlyphMetric { w: 1139, h: 694, d: 194, ic: 0 }, // 0x2E
    GlyphMetric { w: 893, h: 431, d: 0, ic: 0 }, // 0x2F
    GlyphMetric { w: 329, h: 556, d: 0, ic: 0 }, // 0x30
    GlyphMetric { w: 1139, h: 431, d: 0, ic: 0 }, // 0x31
    GlyphMetric { w: 770, h: 576, d: 76, ic: 0 }, // 0x32
    GlyphMetric { w: 770, h: 576, d: 76, ic: 0 }, // 0x33
    GlyphMetric { w: 1016, h: 694, d: 194, ic: 0 }, // 0x34
    GlyphMetric { w: 1016, h: 694, d: 194, ic: 0 }, // 0x35
    GlyphMetric { w: 0, h: 694, d: 194, ic: 0 }, // 0x36
    GlyphMetric { w: 0, h: 384, d: 0, ic: 0 }, // 0x37
    GlyphMetric { w: 647, h: 694, d: 0, ic: 0 }, // 0x38
    GlyphMetric { w: 647, h: 694, d: 0, ic: 0 }, // 0x39
    GlyphMetric { w: 770, h: 431, d: 0, ic: 0 }, // 0x3A
    GlyphMetric { w: 585, h: 750, d: 56, ic: 0 }, // 0x3B
    GlyphMetric { w: 831, h: 694, d: 0, ic: 0 }, // 0x3C
    GlyphMetric { w: 831, h: 694, d: 0, ic: 0 }, // 0x3D
    GlyphMetric { w: 893, h: 694, d: 0, ic: 0 }, // 0x3E
    GlyphMetric { w: 893, h: 694, d: 0, ic: 0 }, // 0x3F
    GlyphMetric { w: 708, h: 694, d: 0, ic: 0 }, // 0x40
    GlyphMetric { w: 918, h: 683, d: 0, ic: 0 }, // 0x41
    GlyphMetric { w: 753, h: 683, d: 0, ic: 25 }, // 0x42
    GlyphMetric { w: 620, h: 683, d: 0, ic: 50 }, // 0x43
    GlyphMetric { w: 889, h: 683, d: 0, ic: 31 }, // 0x44
    GlyphMetric { w: 616, h: 683, d: 0, ic: 84 }, // 0x45
    GlyphMetric { w: 818, h: 683, d: 0, ic: 101 }, // 0x46
    GlyphMetric { w: 689, h: 683, d: 97, ic: 57 }, // 0x47
    GlyphMetric { w: 979, h: 683, d: 0, ic: 0 }, // 0x48
    GlyphMetric { w: 647, h: 683, d: 0, ic: 65 }, // 0x49
    GlyphMetric { w: 782, h: 683, d: 97, ic: 186 }, // 0x4A
    GlyphMetric { w: 872, h: 683, d: 0, ic: 1 }, // 0x4B
    GlyphMetric { w: 792, h: 683, d: 0, ic: 0 }, // 0x4C
    GlyphMetric { w: 1343, h: 683, d: 0, ic: 0 }, // 0x4D
    GlyphMetric { w: 936, h: 683, d: 0, ic: 150 }, // 0x4E
    GlyphMetric { w: 906, h: 683, d: 0, ic: 31 }, // 0x4F
    GlyphMetric { w: 809, h: 683, d: 0, ic: 84 }, // 0x50
    GlyphMetric { w: 936, h: 683, d: 97, ic: 0 }, // 0x51
    GlyphMetric { w: 981, h: 683, d: 0, ic: 0 }, // 0x52
    GlyphMetric { w: 702, h: 683, d: 0, ic: 72 }, // 0x53
    GlyphMetric { w: 648, h: 683, d: 0, ic: 263 }, // 0x54
    GlyphMetric { w: 718, h: 683, d: 0, ic: 101 }, // 0x55
    GlyphMetric { w: 720, h: 683, d: 0, ic: 84 }, // 0x56
    GlyphMetric { w: 1135, h: 683, d: 0, ic: 84 }, // 0x57
    GlyphMetric { w: 819, h: 683, d: 0, ic: 146 }, // 0x58
    GlyphMetric { w: 764, h: 683, d: 97, ic: 84 }, // 0x59
    GlyphMetric { w: 823, h: 683, d: 0, ic: 81 }, // 0x5A
    GlyphMetric { w: 770, h: 556, d: 0, ic: 0 }, // 0x5B
    GlyphMetric { w: 770, h: 556, d: 0, ic: 0 }, // 0x5C
    GlyphMetric { w: 770, h: 556, d: 0, ic: 0 }, // 0x5D
    GlyphMetric { w: 770, h: 556, d: 0, ic: 0 }, // 0x5E
    GlyphMetric { w: 770, h: 556, d: 0, ic: 0 }, // 0x5F
    GlyphMetric { w: 708, h: 694, d: 0, ic: 0 }, // 0x60
    GlyphMetric { w: 708, h: 694, d: 0, ic: 0 }, // 0x61
    GlyphMetric { w: 524, h: 750, d: 250, ic: 0 }, // 0x62
    GlyphMetric { w: 524, h: 750, d: 250, ic: 0 }, // 0x63
    GlyphMetric { w: 524, h: 750, d: 250, ic: 0 }, // 0x64
    GlyphMetric { w: 524, h: 750, d: 250, ic: 0 }, // 0x65
    GlyphMetric { w: 585, h: 750, d: 250, ic: 0 }, // 0x66
    GlyphMetric { w: 585, h: 750, d: 250, ic: 0 }, // 0x67
    GlyphMetric { w: 462, h: 750, d: 250, ic: 0 }, // 0x68
    GlyphMetric { w: 462, h: 750, d: 250, ic: 0 }, // 0x69
    GlyphMetric { w: 339, h: 750, d: 250, ic: 0 }, // 0x6A
    GlyphMetric { w: 585, h: 750, d: 250, ic: 0 }, // 0x6B
    GlyphMetric { w: 585, h: 750, d: 250, ic: 0 }, // 0x6C
    GlyphMetric { w: 708, h: 750, d: 250, ic: 0 }, // 0x6D
    GlyphMetric { w: 585, h: 750, d: 250, ic: 0 }, // 0x6E
    GlyphMetric { w: 339, h: 694, d: 194, ic: 0 }, // 0x6F
    GlyphMetric { w: 938, h: 49, d: 951, ic: 0 }, // 0x70
    GlyphMetric { w: 859, h: 683, d: 0, ic: 0 }, // 0x71
    GlyphMetric { w: 954, h: 683, d: 0, ic: 0 }, // 0x72
    GlyphMetric { w: 494, h: 694, d: 194, ic: 123 }, // 0x73
    GlyphMetric { w: 770, h: 556, d: 0, ic: 0 }, // 0x74
    GlyphMetric { w: 770, h: 556, d: 0, ic: 0 }, // 0x75
    GlyphMetric { w: 893, h: 685, d: 185, ic: 0 }, // 0x76
    GlyphMetric { w: 893, h: 685, d: 185, ic: 0 }, // 0x77
    GlyphMetric { w: 524, h: 694, d: 194, ic: 0 }, // 0x78
    GlyphMetric { w: 524, h: 694, d: 194, ic: 0 }, // 0x79
    GlyphMetric { w: 524, h: 694, d: 194, ic: 0 }, // 0x7A
    GlyphMetric { w: 708, h: 694, d: 194, ic: 0 }, // 0x7B
    GlyphMetric { w: 893, h: 694, d: 130, ic: 0 }, // 0x7C
    GlyphMetric { w: 893, h: 694, d: 130, ic: 0 }, // 0x7D
    GlyphMetric { w: 893, h: 694, d: 130, ic: 0 }, // 0x7E
    GlyphMetric { w: 893, h: 694, d: 130, ic: 0 }, // 0x7F
];

pub static CMSY5_METRICS: [GlyphMetric; 128] = [
    GlyphMetric { w: 1083, h: 667, d: 167, ic: 0 }, // 0x00
    GlyphMetric { w: 458, h: 493, d: 0, ic: 0 }, // 0x01
    GlyphMetric { w: 1083, h: 667, d: 167, ic: 0 }, // 0x02
    GlyphMetric { w: 736, h: 465, d: 0, ic: 0 }, // 0x03
    GlyphMetric { w: 1083, h: 667, d: 167, ic: 0 }, // 0x04
    GlyphMetric { w: 736, h: 493, d: 0, ic: 0 }, // 0x05
    GlyphMetric { w: 1083, h: 667, d: 167, ic: 0 }, // 0x06
    GlyphMetric { w: 1083, h: 667, d: 167, ic: 0 }, // 0x07
    GlyphMetric { w: 1083, h: 667, d: 167, ic: 0 }, // 0x08
    GlyphMetric { w: 1083, h: 667, d: 167, ic: 0 }, // 0x09
    GlyphMetric { w: 1083, h: 667, d: 167, ic: 0 }, // 0x0A
    GlyphMetric { w: 1083, h: 667, d: 167, ic: 0 }, // 0x0B
    GlyphMetric { w: 1083, h: 667, d: 167, ic: 0 }, // 0x0C
    GlyphMetric { w: 1361, h: 694, d: 194, ic: 0 }, // 0x0D
    GlyphMetric { w: 736, h: 493, d: 0, ic: 0 }, // 0x0E
    GlyphMetric { w: 736, h: 493, d: 0, ic: 0 }, // 0x0F
    GlyphMetric { w: 1083, h: 515, d: 15, ic: 0 }, // 0x10
    GlyphMetric { w: 1083, h: 515, d: 15, ic: 0 }, // 0x11
    GlyphMetric { w: 1083, h: 719, d: 219, ic: 0 }, // 0x12
    GlyphMetric { w: 1083, h: 719, d: 219, ic: 0 }, // 0x13
    GlyphMetric { w: 1083, h: 719, d: 219, ic: 0 }, // 0x14
    GlyphMetric { w: 1083, h: 719, d: 219, ic: 0 }, // 0x15
    GlyphMetric { w: 1083, h: 719, d: 219, ic: 0 }, // 0x16
    GlyphMetric { w: 1083, h: 719, d: 219, ic: 0 }, // 0x17
    GlyphMetric { w: 1083, h: 396, d: 0, ic: 0 }, // 0x18
    GlyphMetric { w: 1083, h: 538, d: 38, ic: 0 }, // 0x19
    GlyphMetric { w: 1083, h: 601, d: 101, ic: 0 }, // 0x1A
    GlyphMetric { w: 1083, h: 601, d: 101, ic: 0 }, // 0x1B
    GlyphMetric { w: 1361, h: 601, d: 101, ic: 0 }, // 0x1C
    GlyphMetric { w: 1361, h: 601, d: 101, ic: 0 }, // 0x1D
    GlyphMetric { w: 1083, h: 601, d: 101, ic: 0 }, // 0x1E
    GlyphMetric { w: 1083, h: 601, d: 101, ic: 0 }, // 0x1F
    GlyphMetric { w: 1361, h: 396, d: 0, ic: 0 }, // 0x20
    GlyphMetric { w: 1361, h: 396, d: 0, ic: 0 }, // 0x21
    GlyphMetric { w: 736, h: 694, d: 194, ic: 0 }, // 0x22
    GlyphMetric { w: 736, h: 694, d: 194, ic: 0 }, // 0x23
    GlyphMetric { w: 1361, h: 396, d: 0, ic: 0 }, // 0x24
    GlyphMetric { w: 1361, h: 694, d: 194, ic: 0 }, // 0x25
    GlyphMetric { w: 1361, h: 694, d: 194, ic: 0 }, // 0x26
    GlyphMetric { w: 1083, h: 515, d: 15, ic: 0 }, // 0x27
    GlyphMetric { w: 1361, h: 396, d: 0, ic: 0 }, // 0x28
    GlyphMetric { w: 1361, h: 396, d: 0, ic: 0 }, // 0x29
    GlyphMetric { w: 875, h: 694, d: 194, ic: 0 }, // 0x2A
    GlyphMetric { w: 875, h: 694, d: 194, ic: 0 }, // 0x2B
    GlyphMetric { w: 1361, h: 396, d: 0, ic: 0 }, // 0x2C
    GlyphMetric { w: 1361, h: 694, d: 194, ic: 0 }, // 0x2D
    GlyphMetric { w: 1361, h: 694, d: 194, ic: 0 }, // 0x2E
    GlyphMetric { w: 1083, h: 431, d: 0, ic: 0 }, // 0x2F
    GlyphMetric { w: 441, h: 556, d: 0, ic: 0 }, // 0x30
    GlyphMetric { w: 1361, h: 431, d: 0, ic: 0 }, // 0x31
    GlyphMetric { w: 944, h: 601, d: 101, ic: 0 }, // 0x32
    GlyphMetric { w: 944, h: 601, d: 101, ic: 0 }, // 0x33
    GlyphMetric { w: 1222, h: 694, d: 194, ic: 0 }, // 0x34
    GlyphMetric { w: 1222, h: 694, d: 194, ic: 0 }, // 0x35
    GlyphMetric { w: 0, h: 694, d: 194, ic: 0 }, // 0x36
    GlyphMetric { w: 0, h: 396, d: 0, ic: 0 }, // 0x37
    GlyphMetric { w: 806, h: 694, d: 0, ic: 0 }, // 0x38
    GlyphMetric { w: 806, h: 694, d: 0, ic: 0 }, // 0x39
    GlyphMetric { w: 944, h: 431, d: 0, ic: 0 }, // 0x3A
    GlyphMetric { w: 736, h: 750, d: 56, ic: 0 }, // 0x3B
    GlyphMetric { w: 1014, h: 694, d: 0, ic: 0 }, // 0x3C
    GlyphMetric { w: 1014, h: 694, d: 0, ic: 0 }, // 0x3D
    GlyphMetric { w: 1083, h: 694, d: 0, ic: 0 }, // 0x3E
    GlyphMetric { w: 1083, h: 694, d: 0, ic: 0 }, // 0x3F
    GlyphMetric { w: 875, h: 694, d: 0, ic: 0 }, // 0x40
    GlyphMetric { w: 1113, h: 683, d: 0, ic: 0 }, // 0x41
    GlyphMetric { w: 919, h: 683, d: 0, ic: 17 }, // 0x42
    GlyphMetric { w: 782, h: 683, d: 0, ic: 39 }, // 0x43
    GlyphMetric { w: 1084, h: 683, d: 0, ic: 35 }, // 0x44
    GlyphMetric { w: 771, h: 683, d: 0, ic: 78 }, // 0x45
    GlyphMetric { w: 988, h: 683, d: 0, ic: 103 }, // 0x46
    GlyphMetric { w: 850, h: 683, d: 97, ic: 53 }, // 0x47
    GlyphMetric { w: 1195, h: 683, d: 0, ic: 0 }, // 0x48
    GlyphMetric { w: 820, h: 683, d: 0, ic: 52 }, // 0x49
    GlyphMetric { w: 958, h: 683, d: 97, ic: 188 }, // 0x4A
    GlyphMetric { w: 1055, h: 683, d: 0, ic: 0 }, // 0x4B
    GlyphMetric { w: 965, h: 683, d: 0, ic: 0 }, // 0x4C
    GlyphMetric { w: 1568, h: 683, d: 0, ic: 0 }, // 0x4D
    GlyphMetric { w: 1126, h: 683, d: 0, ic: 154 }, // 0x4E
    GlyphMetric { w: 1089, h: 683, d: 0, ic: 35 }, // 0x4F
    GlyphMetric { w: 998, h: 683, d: 0, ic: 86 }, // 0x50
    GlyphMetric { w: 1132, h: 683, d: 97, ic: 0 }, // 0x51
    GlyphMetric { w: 1196, h: 683, d: 0, ic: 0 }, // 0x52
    GlyphMetric { w: 868, h: 683, d: 0, ic: 68 }, // 0x53
    GlyphMetric { w: 822, h: 683, d: 0, ic: 275 }, // 0x54
    GlyphMetric { w: 874, h: 683, d: 0, ic: 103 }, // 0x55
    GlyphMetric { w: 899, h: 683, d: 0, ic: 86 }, // 0x56
    GlyphMetric { w: 1368, h: 683, d: 0, ic: 86 }, // 0x57
    GlyphMetric { w: 997, h: 683, d: 0, ic: 146 }, // 0x58
    GlyphMetric { w: 929, h: 683, d: 97, ic: 86 }, // 0x59
    GlyphMetric { w: 991, h: 683, d: 0, ic: 82 }, // 0x5A
    GlyphMetric { w: 944, h: 556, d: 0, ic: 0 }, // 0x5B
    GlyphMetric { w: 944, h: 556, d: 0, ic: 0 }, // 0x5C
    GlyphMetric { w: 944, h: 556, d: 0, ic: 0 }, // 0x5D
    GlyphMetric { w: 944, h: 556, d: 0, ic: 0 }, // 0x5E
    GlyphMetric { w: 944, h: 556, d: 0, ic: 0 }, // 0x5F
    GlyphMetric { w: 875, h: 694, d: 0, ic: 0 }, // 0x60
    GlyphMetric { w: 875, h: 694, d: 0, ic: 0 }, // 0x61
    GlyphMetric { w: 667, h: 750, d: 250, ic: 0 }, // 0x62
    GlyphMetric { w: 667, h: 750, d: 250, ic: 0 }, // 0x63
    GlyphMetric { w: 667, h: 750, d: 250, ic: 0 }, // 0x64
    GlyphMetric { w: 667, h: 750, d: 250, ic: 0 }, // 0x65
    GlyphMetric { w: 736, h: 750, d: 250, ic: 0 }, // 0x66
    GlyphMetric { w: 736, h: 750, d: 250, ic: 0 }, // 0x67
    GlyphMetric { w: 597, h: 750, d: 250, ic: 0 }, // 0x68
    GlyphMetric { w: 597, h: 750, d: 250, ic: 0 }, // 0x69
    GlyphMetric { w: 458, h: 750, d: 250, ic: 0 }, // 0x6A
    GlyphMetric { w: 736, h: 750, d: 250, ic: 0 }, // 0x6B
    GlyphMetric { w: 736, h: 750, d: 250, ic: 0 }, // 0x6C
    GlyphMetric { w: 875, h: 750, d: 250, ic: 0 }, // 0x6D
    GlyphMetric { w: 736, h: 750, d: 250, ic: 0 }, // 0x6E
    GlyphMetric { w: 458, h: 694, d: 194, ic: 0 }, // 0x6F
    GlyphMetric { w: 1097, h: 56, d: 944, ic: 0 }, // 0x70
    GlyphMetric { w: 1036, h: 683, d: 0, ic: 0 }, // 0x71
    GlyphMetric { w: 1153, h: 683, d: 0, ic: 0 }, // 0x72
    GlyphMetric { w: 632, h: 694, d: 194, ic: 139 }, // 0x73
    GlyphMetric { w: 944, h: 556, d: 0, ic: 0 }, // 0x74
    GlyphMetric { w: 944, h: 556, d: 0, ic: 0 }, // 0x75
    GlyphMetric { w: 1083, h: 719, d: 219, ic: 0 }, // 0x76
    GlyphMetric { w: 1083, h: 719, d: 219, ic: 0 }, // 0x77
    GlyphMetric { w: 667, h: 694, d: 194, ic: 0 }, // 0x78
    GlyphMetric { w: 667, h: 694, d: 194, ic: 0 }, // 0x79
    GlyphMetric { w: 667, h: 694, d: 194, ic: 0 }, // 0x7A
    GlyphMetric { w: 875, h: 694, d: 194, ic: 0 }, // 0x7B
    GlyphMetric { w: 1083, h: 694, d: 130, ic: 0 }, // 0x7C
    GlyphMetric { w: 1083, h: 694, d: 130, ic: 0 }, // 0x7D
    GlyphMetric { w: 1083, h: 694, d: 130, ic: 0 }, // 0x7E
    GlyphMetric { w: 1083, h: 694, d: 130, ic: 0 }, // 0x7F
];

pub static MSAM10_METRICS: [GlyphMetric; 128] = [
    GlyphMetric { w: 778, h: 675, d: 0, ic: 0 }, // 0x00
    GlyphMetric { w: 778, h: 675, d: 0, ic: 0 }, // 0x01
    GlyphMetric { w: 778, h: 675, d: 0, ic: 0 }, // 0x02
    GlyphMetric { w: 778, h: 675, d: 0, ic: 0 }, // 0x03
    GlyphMetric { w: 778, h: 675, d: 0, ic: 0 }, // 0x04
    GlyphMetric { w: 278, h: 550, d: 0, ic: 0 }, // 0x05
    GlyphMetric { w: 667, h: 692, d: 111, ic: 0 }, // 0x06
    GlyphMetric { w: 667, h: 692, d: 111, ic: 0 }, // 0x07
    GlyphMetric { w: 778, h: 582, d: 82, ic: 0 }, // 0x08
    GlyphMetric { w: 778, h: 582, d: 82, ic: 0 }, // 0x09
    GlyphMetric { w: 1000, h: 522, d: 14, ic: 0 }, // 0x0A
    GlyphMetric { w: 1000, h: 522, d: 14, ic: 0 }, // 0x0B
    GlyphMetric { w: 778, h: 675, d: 0, ic: 0 }, // 0x0C
    GlyphMetric { w: 722, h: 692, d: 0, ic: 0 }, // 0x0D
    GlyphMetric { w: 889, h: 692, d: 0, ic: 0 }, // 0x0E
    GlyphMetric { w: 611, h: 692, d: 0, ic: 0 }, // 0x0F
    GlyphMetric { w: 1000, h: 522, d: 14, ic: 0 }, // 0x10
    GlyphMetric { w: 1000, h: 522, d: 14, ic: 0 }, // 0x11
    GlyphMetric { w: 1000, h: 675, d: 181, ic: 0 }, // 0x12
    GlyphMetric { w: 1000, h: 675, d: 181, ic: 0 }, // 0x13
    GlyphMetric { w: 833, h: 692, d: 194, ic: 0 }, // 0x14
    GlyphMetric { w: 833, h: 692, d: 194, ic: 0 }, // 0x15
    GlyphMetric { w: 417, h: 692, d: 194, ic: 0 }, // 0x16
    GlyphMetric { w: 417, h: 692, d: 194, ic: 0 }, // 0x17
    GlyphMetric { w: 417, h: 692, d: 194, ic: 0 }, // 0x18
    GlyphMetric { w: 417, h: 692, d: 194, ic: 0 }, // 0x19
    GlyphMetric { w: 1111, h: 522, d: 14, ic: 0 }, // 0x1A
    GlyphMetric { w: 1111, h: 522, d: 14, ic: 0 }, // 0x1B
    GlyphMetric { w: 1000, h: 675, d: 181, ic: 0 }, // 0x1C
    GlyphMetric { w: 1000, h: 675, d: 181, ic: 0 }, // 0x1D
    GlyphMetric { w: 500, h: 692, d: 0, ic: 0 }, // 0x1E
    GlyphMetric { w: 500, h: 692, d: 0, ic: 0 }, // 0x1F
    GlyphMetric { w: 1000, h: 378, d: 0, ic: 0 }, // 0x20
    GlyphMetric { w: 1389, h: 378, d: 0, ic: 0 }, // 0x21
    GlyphMetric { w: 1000, h: 550, d: 0, ic: 0 }, // 0x22
    GlyphMetric { w: 1000, h: 550, d: 0, ic: 0 }, // 0x23
    GlyphMetric { w: 778, h: 730, d: 230, ic: 0 }, // 0x24
    GlyphMetric { w: 778, h: 730, d: 230, ic: 0 }, // 0x25
    GlyphMetric { w: 778, h: 730, d: 230, ic: 0 }, // 0x26
    GlyphMetric { w: 778, h: 756, d: 256, ic: 0 }, // 0x27
    GlyphMetric { w: 1111, h: 550, d: 0, ic: 0 }, // 0x28
    GlyphMetric { w: 667, h: 692, d: 0, ic: 0 }, // 0x29
    GlyphMetric { w: 667, h: 692, d: 0, ic: 0 }, // 0x2A
    GlyphMetric { w: 778, h: 582, d: 82, ic: 0 }, // 0x2B
    GlyphMetric { w: 778, h: 917, d: 82, ic: 0 }, // 0x2C
    GlyphMetric { w: 778, h: 730, d: 230, ic: 0 }, // 0x2D
    GlyphMetric { w: 778, h: 730, d: 230, ic: 0 }, // 0x2E
    GlyphMetric { w: 778, h: 756, d: 256, ic: 0 }, // 0x2F
    GlyphMetric { w: 778, h: 637, d: 137, ic: 0 }, // 0x30
    GlyphMetric { w: 778, h: 637, d: 137, ic: 0 }, // 0x31
    GlyphMetric { w: 778, h: 637, d: 137, ic: 0 }, // 0x32
    GlyphMetric { w: 778, h: 637, d: 137, ic: 0 }, // 0x33
    GlyphMetric { w: 778, h: 637, d: 137, ic: 0 }, // 0x34
    GlyphMetric { w: 778, h: 756, d: 256, ic: 0 }, // 0x35
    GlyphMetric { w: 778, h: 637, d: 137, ic: 0 }, // 0x36
    GlyphMetric { w: 778, h: 675, d: 181, ic: 0 }, // 0x37
    GlyphMetric { w: 275, h: 550, d: 0, ic: 0 }, // 0x38
    GlyphMetric { w: 500, h: 464, d: 0, ic: 0 }, // 0x39
    GlyphMetric { w: 778, h: 582, d: 82, ic: 0 }, // 0x3A
    GlyphMetric { w: 778, h: 582, d: 82, ic: 0 }, // 0x3B
    GlyphMetric { w: 778, h: 637, d: 137, ic: 0 }, // 0x3C
    GlyphMetric { w: 778, h: 756, d: 256, ic: 0 }, // 0x3D
    GlyphMetric { w: 778, h: 637, d: 137, ic: 0 }, // 0x3E
    GlyphMetric { w: 778, h: 675, d: 181, ic: 0 }, // 0x3F
    GlyphMetric { w: 778, h: 550, d: 35, ic: 0 }, // 0x40
    GlyphMetric { w: 778, h: 550, d: 35, ic: 0 }, // 0x41
    GlyphMetric { w: 778, h: 550, d: 35, ic: 0 }, // 0x42
    GlyphMetric { w: 778, h: 550, d: 35, ic: 0 }, // 0x43
    GlyphMetric { w: 778, h: 637, d: 137, ic: 0 }, // 0x44
    GlyphMetric { w: 778, h: 637, d: 137, ic: 0 }, // 0x45
    GlyphMetric { w: 944, h: 692, d: 194, ic: 0 }, // 0x46
    GlyphMetric { w: 500, h: 756, d: 256, ic: 0 }, // 0x47
    GlyphMetric { w: 722, h: 550, d: 0, ic: 0 }, // 0x48
    GlyphMetric { w: 778, h: 550, d: 35, ic: 0 }, // 0x49
    GlyphMetric { w: 778, h: 550, d: 35, ic: 0 }, // 0x4A
    GlyphMetric { w: 500, h: 464, d: 0, ic: 0 }, // 0x4B
    GlyphMetric { w: 500, h: 464, d: 0, ic: 0 }, // 0x4C
    GlyphMetric { w: 722, h: 550, d: 0, ic: 0 }, // 0x4D
    GlyphMetric { w: 722, h: 550, d: 0, ic: 0 }, // 0x4E
    GlyphMetric { w: 722, h: 550, d: 0, ic: 0 }, // 0x4F
    GlyphMetric { w: 778, h: 692, d: 0, ic: 0 }, // 0x50
    GlyphMetric { w: 778, h: 886, d: 386, ic: 0 }, // 0x51
    GlyphMetric { w: 778, h: 886, d: 386, ic: 0 }, // 0x52
    GlyphMetric { w: 778, h: 983, d: 483, ic: 0 }, // 0x53
    GlyphMetric { w: 778, h: 983, d: 483, ic: 0 }, // 0x54
    GlyphMetric { w: 750, h: 675, d: 0, ic: 25 }, // 0x55
    GlyphMetric { w: 1000, h: 637, d: 137, ic: 0 }, // 0x56
    GlyphMetric { w: 1000, h: 637, d: 137, ic: 0 }, // 0x57
    GlyphMetric { w: 833, h: 692, d: 0, ic: 0 }, // 0x58
    GlyphMetric { w: 611, h: 692, d: 194, ic: 0 }, // 0x59
    GlyphMetric { w: 611, h: 692, d: 194, ic: 0 }, // 0x5A
    GlyphMetric { w: 611, h: 756, d: 194, ic: 0 }, // 0x5B
    GlyphMetric { w: 722, h: 692, d: 0, ic: 0 }, // 0x5C
    GlyphMetric { w: 722, h: 692, d: 0, ic: 0 }, // 0x5D
    GlyphMetric { w: 722, h: 522, d: 35, ic: 0 }, // 0x5E
    GlyphMetric { w: 778, h: 692, d: 0, ic: 0 }, // 0x5F
    GlyphMetric { w: 778, h: 692, d: 194, ic: 0 }, // 0x60
    GlyphMetric { w: 778, h: 692, d: 194, ic: 0 }, // 0x61
    GlyphMetric { w: 778, h: 550, d: 35, ic: 0 }, // 0x62
    GlyphMetric { w: 778, h: 550, d: 35, ic: 0 }, // 0x63
    GlyphMetric { w: 667, h: 550, d: 0, ic: 0 }, // 0x64
    GlyphMetric { w: 667, h: 550, d: 0, ic: 0 }, // 0x65
    GlyphMetric { w: 760, h: 550, d: 0, ic: 0 }, // 0x66
    GlyphMetric { w: 760, h: 550, d: 0, ic: 0 }, // 0x67
    GlyphMetric { w: 778, h: 692, d: 0, ic: 0 }, // 0x68
    GlyphMetric { w: 778, h: 692, d: 0, ic: 0 }, // 0x69
    GlyphMetric { w: 778, h: 756, d: 256, ic: 0 }, // 0x6A
    GlyphMetric { w: 778, h: 756, d: 256, ic: 0 }, // 0x6B
    GlyphMetric { w: 778, h: 550, d: 61, ic: 0 }, // 0x6C
    GlyphMetric { w: 778, h: 550, d: 61, ic: 0 }, // 0x6D
    GlyphMetric { w: 1333, h: 550, d: 35, ic: 0 }, // 0x6E
    GlyphMetric { w: 1333, h: 550, d: 35, ic: 0 }, // 0x6F
    GlyphMetric { w: 500, h: 692, d: 0, ic: 0 }, // 0x70
    GlyphMetric { w: 500, h: 692, d: 0, ic: 0 }, // 0x71
    GlyphMetric { w: 947, h: 692, d: 156, ic: 0 }, // 0x72
    GlyphMetric { w: 902, h: 692, d: 156, ic: 0 }, // 0x73
    GlyphMetric { w: 667, h: 692, d: 0, ic: 0 }, // 0x74
    GlyphMetric { w: 778, h: 692, d: 82, ic: 0 }, // 0x75
    GlyphMetric { w: 778, h: 378, d: 0, ic: 0 }, // 0x76
    GlyphMetric { w: 778, h: 464, d: 0, ic: 0 }, // 0x77
    GlyphMetric { w: 500, h: 378, d: 0, ic: 0 }, // 0x78
    GlyphMetric { w: 500, h: 378, d: 0, ic: 0 }, // 0x79
    GlyphMetric { w: 833, h: 692, d: 0, ic: 0 }, // 0x7A
    GlyphMetric { w: 500, h: 825, d: 0, ic: 0 }, // 0x7B
    GlyphMetric { w: 556, h: 431, d: 194, ic: 0 }, // 0x7C
    GlyphMetric { w: 778, h: 582, d: 82, ic: 0 }, // 0x7D
    GlyphMetric { w: 778, h: 582, d: 82, ic: 0 }, // 0x7E
    GlyphMetric { w: 778, h: 582, d: 82, ic: 0 }, // 0x7F
];

pub static MSBM10_METRICS: [GlyphMetric; 128] = [
    GlyphMetric { w: 778, h: 757, d: 251, ic: 0 }, // 0x00
    GlyphMetric { w: 778, h: 757, d: 251, ic: 0 }, // 0x01
    GlyphMetric { w: 778, h: 794, d: 303, ic: 0 }, // 0x02
    GlyphMetric { w: 778, h: 794, d: 303, ic: 0 }, // 0x03
    GlyphMetric { w: 778, h: 706, d: 206, ic: 0 }, // 0x04
    GlyphMetric { w: 778, h: 706, d: 206, ic: 0 }, // 0x05
    GlyphMetric { w: 778, h: 706, d: 206, ic: 0 }, // 0x06
    GlyphMetric { w: 778, h: 706, d: 206, ic: 0 }, // 0x07
    GlyphMetric { w: 778, h: 757, d: 251, ic: 0 }, // 0x08
    GlyphMetric { w: 778, h: 757, d: 251, ic: 0 }, // 0x09
    GlyphMetric { w: 778, h: 794, d: 303, ic: 0 }, // 0x0A
    GlyphMetric { w: 778, h: 794, d: 303, ic: 0 }, // 0x0B
    GlyphMetric { w: 778, h: 636, d: 136, ic: 0 }, // 0x0C
    GlyphMetric { w: 778, h: 636, d: 136, ic: 0 }, // 0x0D
    GlyphMetric { w: 778, h: 794, d: 303, ic: 0 }, // 0x0E
    GlyphMetric { w: 778, h: 794, d: 303, ic: 0 }, // 0x0F
    GlyphMetric { w: 778, h: 741, d: 232, ic: 0 }, // 0x10
    GlyphMetric { w: 778, h: 741, d: 232, ic: 0 }, // 0x11
    GlyphMetric { w: 778, h: 741, d: 232, ic: 0 }, // 0x12
    GlyphMetric { w: 778, h: 741, d: 232, ic: 0 }, // 0x13
    GlyphMetric { w: 778, h: 920, d: 420, ic: 0 }, // 0x14
    GlyphMetric { w: 778, h: 920, d: 420, ic: 0 }, // 0x15
    GlyphMetric { w: 778, h: 757, d: 251, ic: 0 }, // 0x16
    GlyphMetric { w: 778, h: 757, d: 251, ic: 0 }, // 0x17
    GlyphMetric { w: 778, h: 757, d: 262, ic: 0 }, // 0x18
    GlyphMetric { w: 778, h: 757, d: 262, ic: 0 }, // 0x19
    GlyphMetric { w: 778, h: 757, d: 262, ic: 0 }, // 0x1A
    GlyphMetric { w: 778, h: 757, d: 262, ic: 0 }, // 0x1B
    GlyphMetric { w: 778, h: 367, d: 0, ic: 0 }, // 0x1C
    GlyphMetric { w: 778, h: 794, d: 303, ic: 0 }, // 0x1D
    GlyphMetric { w: 889, h: 689, d: 194, ic: 0 }, // 0x1E
    GlyphMetric { w: 889, h: 741, d: 194, ic: 0 }, // 0x1F
    GlyphMetric { w: 778, h: 636, d: 136, ic: 0 }, // 0x20
    GlyphMetric { w: 778, h: 636, d: 136, ic: 0 }, // 0x21
    GlyphMetric { w: 778, h: 757, d: 251, ic: 0 }, // 0x22
    GlyphMetric { w: 778, h: 757, d: 251, ic: 0 }, // 0x23
    GlyphMetric { w: 778, h: 794, d: 285, ic: 0 }, // 0x24
    GlyphMetric { w: 778, h: 794, d: 285, ic: 0 }, // 0x25
    GlyphMetric { w: 778, h: 757, d: 251, ic: 0 }, // 0x26
    GlyphMetric { w: 778, h: 757, d: 251, ic: 0 }, // 0x27
    GlyphMetric { w: 778, h: 636, d: 136, ic: 0 }, // 0x28
    GlyphMetric { w: 778, h: 636, d: 136, ic: 0 }, // 0x29
    GlyphMetric { w: 778, h: 794, d: 303, ic: 0 }, // 0x2A
    GlyphMetric { w: 778, h: 794, d: 303, ic: 0 }, // 0x2B
    GlyphMetric { w: 500, h: 741, d: 251, ic: 0 }, // 0x2C
    GlyphMetric { w: 278, h: 741, d: 251, ic: 0 }, // 0x2D
    GlyphMetric { w: 222, h: 582, d: 82, ic: 0 }, // 0x2E
    GlyphMetric { w: 389, h: 582, d: 82, ic: 0 }, // 0x2F
    GlyphMetric { w: 611, h: 689, d: 0, ic: 0 }, // 0x30
    GlyphMetric { w: 722, h: 689, d: 0, ic: 0 }, // 0x31
    GlyphMetric { w: 611, h: 689, d: 0, ic: 0 }, // 0x32
    GlyphMetric { w: 722, h: 689, d: 0, ic: 0 }, // 0x33
    GlyphMetric { w: 778, h: 794, d: 303, ic: 0 }, // 0x34
    GlyphMetric { w: 778, h: 794, d: 303, ic: 0 }, // 0x35
    GlyphMetric { w: 778, h: 706, d: 206, ic: 0 }, // 0x36
    GlyphMetric { w: 778, h: 706, d: 206, ic: 0 }, // 0x37
    GlyphMetric { w: 1000, h: 367, d: 0, ic: 0 }, // 0x38
    GlyphMetric { w: 1000, h: 367, d: 0, ic: 0 }, // 0x39
    GlyphMetric { w: 1000, h: 367, d: 0, ic: 0 }, // 0x3A
    GlyphMetric { w: 1000, h: 367, d: 0, ic: 0 }, // 0x3B
    GlyphMetric { w: 1000, h: 367, d: 0, ic: 0 }, // 0x3C
    GlyphMetric { w: 1000, h: 367, d: 0, ic: 0 }, // 0x3D
    GlyphMetric { w: 778, h: 582, d: 82, ic: 0 }, // 0x3E
    GlyphMetric { w: 778, h: 582, d: 82, ic: 0 }, // 0x3F
    GlyphMetric { w: 556, h: 689, d: 0, ic: 0 }, // 0x40
    GlyphMetric { w: 722, h: 689, d: 0, ic: 0 }, // 0x41
    GlyphMetric { w: 667, h: 689, d: 0, ic: 0 }, // 0x42
    GlyphMetric { w: 722, h: 689, d: 0, ic: 0 }, // 0x43
    GlyphMetric { w: 722, h: 689, d: 0, ic: 0 }, // 0x44
    GlyphMetric { w: 667, h: 689, d: 0, ic: 0 }, // 0x45
    GlyphMetric { w: 611, h: 689, d: 0, ic: 0 }, // 0x46
    GlyphMetric { w: 778, h: 689, d: 0, ic: 0 }, // 0x47
    GlyphMetric { w: 778, h: 689, d: 0, ic: 0 }, // 0x48
    GlyphMetric { w: 389, h: 689, d: 0, ic: 0 }, // 0x49
    GlyphMetric { w: 500, h: 689, d: 167, ic: 0 }, // 0x4A
    GlyphMetric { w: 778, h: 689, d: 0, ic: 0 }, // 0x4B
    GlyphMetric { w: 667, h: 689, d: 0, ic: 0 }, // 0x4C
    GlyphMetric { w: 944, h: 689, d: 0, ic: 0 }, // 0x4D
    GlyphMetric { w: 722, h: 689, d: 0, ic: 0 }, // 0x4E
    GlyphMetric { w: 778, h: 689, d: 167, ic: 0 }, // 0x4F
    GlyphMetric { w: 611, h: 689, d: 0, ic: 0 }, // 0x50
    GlyphMetric { w: 778, h: 689, d: 167, ic: 0 }, // 0x51
    GlyphMetric { w: 722, h: 689, d: 0, ic: 0 }, // 0x52
    GlyphMetric { w: 556, h: 689, d: 0, ic: 0 }, // 0x53
    GlyphMetric { w: 667, h: 689, d: 0, ic: 0 }, // 0x54
    GlyphMetric { w: 722, h: 689, d: 0, ic: 0 }, // 0x55
    GlyphMetric { w: 722, h: 689, d: 0, ic: 0 }, // 0x56
    GlyphMetric { w: 1000, h: 689, d: 0, ic: 0 }, // 0x57
    GlyphMetric { w: 722, h: 689, d: 0, ic: 0 }, // 0x58
    GlyphMetric { w: 722, h: 689, d: 0, ic: 0 }, // 0x59
    GlyphMetric { w: 667, h: 689, d: 0, ic: 0 }, // 0x5A
    GlyphMetric { w: 1889, h: 825, d: 0, ic: 0 }, // 0x5B
    GlyphMetric { w: 2333, h: 825, d: 0, ic: 0 }, // 0x5C
    GlyphMetric { w: 1889, h: 825, d: 0, ic: 0 }, // 0x5D
    GlyphMetric { w: 2333, h: 900, d: 0, ic: 0 }, // 0x5E
    GlyphMetric { w: 0, h: 0, d: 0, ic: 0 }, // 0x5F
    GlyphMetric { w: 556, h: 689, d: 0, ic: 0 }, // 0x60
    GlyphMetric { w: 639, h: 689, d: 0, ic: 0 }, // 0x61
    GlyphMetric { w: 0, h: 0, d: 0, ic: 0 }, // 0x62
    GlyphMetric { w: 0, h: 0, d: 0, ic: 0 }, // 0x63
    GlyphMetric { w: 0, h: 0, d: 0, ic: 0 }, // 0x64
    GlyphMetric { w: 0, h: 0, d: 0, ic: 0 }, // 0x65
    GlyphMetric { w: 722, h: 689, d: 0, ic: 0 }, // 0x66
    GlyphMetric { w: 556, h: 689, d: 0, ic: 0 }, // 0x67
    GlyphMetric { w: 778, h: 464, d: 0, ic: 0 }, // 0x68
    GlyphMetric { w: 667, h: 689, d: 0, ic: 0 }, // 0x69
    GlyphMetric { w: 444, h: 689, d: 0, ic: 0 }, // 0x6A
    GlyphMetric { w: 667, h: 689, d: 0, ic: 0 }, // 0x6B
    GlyphMetric { w: 778, h: 539, d: 39, ic: 0 }, // 0x6C
    GlyphMetric { w: 778, h: 539, d: 39, ic: 0 }, // 0x6D
    GlyphMetric { w: 778, h: 582, d: 82, ic: 0 }, // 0x6E
    GlyphMetric { w: 778, h: 582, d: 82, ic: 0 }, // 0x6F
    GlyphMetric { w: 222, h: 582, d: 82, ic: 0 }, // 0x70
    GlyphMetric { w: 389, h: 582, d: 82, ic: 0 }, // 0x71
    GlyphMetric { w: 778, h: 582, d: 82, ic: 0 }, // 0x72
    GlyphMetric { w: 778, h: 367, d: 0, ic: 0 }, // 0x73
    GlyphMetric { w: 778, h: 483, d: 0, ic: 0 }, // 0x74
    GlyphMetric { w: 778, h: 582, d: 82, ic: 0 }, // 0x75
    GlyphMetric { w: 778, h: 757, d: 262, ic: 0 }, // 0x76
    GlyphMetric { w: 778, h: 757, d: 262, ic: 0 }, // 0x77
    GlyphMetric { w: 1000, h: 431, d: 0, ic: 0 }, // 0x78
    GlyphMetric { w: 1000, h: 431, d: 0, ic: 0 }, // 0x79
    GlyphMetric { w: 778, h: 582, d: 82, ic: 0 }, // 0x7A
    GlyphMetric { w: 667, h: 431, d: 0, ic: 40 }, // 0x7B
    GlyphMetric { w: 556, h: 689, d: 0, ic: 0 }, // 0x7C
    GlyphMetric { w: 540, h: 689, d: 0, ic: 0 }, // 0x7D
    GlyphMetric { w: 540, h: 689, d: 0, ic: 0 }, // 0x7E
    GlyphMetric { w: 429, h: 431, d: 0, ic: 0 }, // 0x7F
];




/// O(1) CMR10 glyph metric lookup.
#[inline(always)]
pub fn cmr10_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMR10_METRICS[code as usize]
    } else {
        GlyphMetric { w: 0, h: 0, d: 0, ic: 0 }
    }
}

/// O(1) CMR7 glyph metric lookup.
#[inline(always)]
pub fn cmr7_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMR7_METRICS[code as usize]
    } else {
        GlyphMetric { w: 0, h: 0, d: 0, ic: 0 }
    }
}

/// O(1) CMR5 glyph metric lookup.
#[inline(always)]
pub fn cmr5_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMR5_METRICS[code as usize]
    } else {
        GlyphMetric { w: 0, h: 0, d: 0, ic: 0 }
    }
}

/// O(1) CMMI7 glyph metric lookup.
#[inline(always)]
pub fn cmmi7_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMMI7_METRICS[code as usize]
    } else {
        GlyphMetric { w: 0, h: 0, d: 0, ic: 0 }
    }
}

/// O(1) CMMI5 glyph metric lookup.
#[inline(always)]
pub fn cmmi5_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMMI5_METRICS[code as usize]
    } else {
        GlyphMetric { w: 0, h: 0, d: 0, ic: 0 }
    }
}

/// O(1) CMSY7 glyph metric lookup.
#[inline(always)]
pub fn cmsy7_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMSY7_METRICS[code as usize]
    } else {
        GlyphMetric { w: 0, h: 0, d: 0, ic: 0 }
    }
}

/// O(1) CMSY5 glyph metric lookup.
#[inline(always)]
pub fn cmsy5_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        CMSY5_METRICS[code as usize]
    } else {
        GlyphMetric { w: 0, h: 0, d: 0, ic: 0 }
    }
}

/// O(1) MSAM10 glyph metric lookup.
#[inline(always)]
pub fn msam10_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        MSAM10_METRICS[code as usize]
    } else {
        GlyphMetric { w: 0, h: 0, d: 0, ic: 0 }
    }
}

/// O(1) MSBM10 glyph metric lookup.
#[inline(always)]
pub fn msbm10_metric(code: u8) -> GlyphMetric {
    if code < 128 {
        MSBM10_METRICS[code as usize]
    } else {
        GlyphMetric { w: 0, h: 0, d: 0, ic: 0 }
    }
}
