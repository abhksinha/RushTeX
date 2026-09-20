//! Spatial bounding box representations for mathematical layout.
//!
//! Provides compact 16-byte AoS geometric boxes (`MathBox`) representing width,
//! height above baseline, depth below baseline, and vertical axis shifts.

/// Compact 16-byte spatial bounding box for math atoms and sub-expressions.
///
/// Designed to fit exactly four bounding boxes per 64-byte L1 CPU cache line.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct MathBox {
    /// Typographic horizontal width in points (pt).
    pub w: f32,
    /// Height above the baseline in points (pt).
    pub h: f32,
    /// Depth below the baseline in points (pt).
    pub d: f32,
    /// Vertical baseline shift offset in points (pt).
    pub shift: f32,
}

const _: () = assert!(core::mem::size_of::<MathBox>() == 16);

impl MathBox {
    /// Creates an empty math box with zero dimensions.
    #[inline(always)]
    pub const fn zero() -> Self {
        Self {
            w: 0.0,
            h: 0.0,
            d: 0.0,
            shift: 0.0,
        }
    }

    /// Creates a math box with explicit width, height, and depth.
    #[inline(always)]
    pub const fn with_dims(w: f32, h: f32, d: f32) -> Self {
        Self {
            w,
            h,
            d,
            shift: 0.0,
        }
    }

    /// Total vertical extent of the box ($h + d$).
    #[inline(always)]
    pub fn total_h(&self) -> f32 {
        self.h + self.d
    }

    /// Horizontally joins two boxes sequentially: accumulates width and computes bounding vertical extents.
    #[inline(always)]
    pub fn append_horiz(&mut self, next: &MathBox) {
        self.w += next.w;
        let next_top = next.h + next.shift;
        let next_bottom = next.d - next.shift;
        if next_top > self.h {
            self.h = next_top;
        }
        if next_bottom > self.d {
            self.d = next_bottom;
        }
    }
}

impl Default for MathBox {
    #[inline(always)]
    fn default() -> Self {
        Self::zero()
    }
}
