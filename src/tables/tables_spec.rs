//! Column specification and alignment parsing for tables.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ColAlign {
    Left = 0,
    Center = 1,
    Right = 2,
}

/// Bit-packed 1-byte column specification struct.
/// Fits exactly 64 column specs per 64-byte L1 CPU cache line.
///
/// Bit layout:
/// - Bits 0..2: `ColAlign` (Left = 0, Center = 1, Right = 2)
/// - Bit 2: `rule_left` (1 = vertical rule to the left of this column)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColSpec {
    bits: u8,
}

impl ColSpec {
    const ALIGN_MASK: u8 = 0b0000_0011;
    const RULE_LEFT_BIT: u8 = 0b0000_0100;

    #[inline(always)]
    pub fn new(align: ColAlign, rule_left: bool) -> Self {
        let mut bits = align as u8;
        if rule_left {
            bits |= Self::RULE_LEFT_BIT;
        }
        Self { bits }
    }

    #[inline(always)]
    pub fn align(self) -> ColAlign {
        match self.bits & Self::ALIGN_MASK {
            0 => ColAlign::Left,
            1 => ColAlign::Center,
            _ => ColAlign::Right,
        }
    }

    #[inline(always)]
    pub fn rule_left(self) -> bool {
        (self.bits & Self::RULE_LEFT_BIT) != 0
    }
}
