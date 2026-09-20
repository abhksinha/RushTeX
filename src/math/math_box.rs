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
    /// Converts this floating-point math box into a fixed-point scaled-point box.
    #[inline(always)]
    pub fn to_sp_box(&self) -> SpMathBox {
        SpMathBox {
            w: ScaledPoints::from_pt(self.w),
            h: ScaledPoints::from_pt(self.h),
            d: ScaledPoints::from_pt(self.d),
            shift: ScaledPoints::from_pt(self.shift),
        }
    }

    /// Reconstructs a floating-point math box from a fixed-point scaled-point box.
    #[inline(always)]
    pub fn from_sp_box(sp_b: &SpMathBox) -> Self {
        sp_b.to_math_box()
    }
}

impl Default for MathBox {
    #[inline(always)]
    fn default() -> Self {
        Self::zero()
    }
}

/// Native 32-bit fixed-point scaled point representation ($1\,\text{pt} = 65,536\,\text{sp} = 2^{16}\,\text{sp}$).
///
/// Direct algorithmic counterpart of Donald Knuth's canonical `scaled = integer` type in TeX (`tex.web`).
/// Bit-packed into 4 bytes, allowing 16 values per 64-byte L1 CPU cache line with zero floating-point drift.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
#[repr(transparent)]
pub struct ScaledPoints(pub i32);

const _: () = assert!(core::mem::size_of::<ScaledPoints>() == 4);

impl ScaledPoints {
    pub const ZERO: Self = Self(0);
    pub const ONE_PT: Self = Self(65536);
    pub const HALF_PT: Self = Self(32768);

    #[inline(always)]
    pub const fn from_sp(sp: i32) -> Self {
        Self(sp)
    }

    #[inline(always)]
    pub const fn to_sp(self) -> i32 {
        self.0
    }

    #[inline(always)]
    pub fn from_pt(pt: f32) -> Self {
        Self((pt * 65536.0).round() as i32)
    }

    #[inline(always)]
    pub fn to_pt(self) -> f32 {
        (self.0 as f32) * (1.0 / 65536.0)
    }

    #[inline(always)]
    pub fn max(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }

    #[inline(always)]
    pub fn min(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }

    #[inline(always)]
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }
}

impl core::ops::Add for ScaledPoints {
    type Output = Self;
    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }
}

impl core::ops::AddAssign for ScaledPoints {
    #[inline(always)]
    fn add_assign(&mut self, rhs: Self) {
        self.0 = self.0.wrapping_add(rhs.0);
    }
}

impl core::ops::Sub for ScaledPoints {
    type Output = Self;
    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.wrapping_sub(rhs.0))
    }
}

impl core::ops::SubAssign for ScaledPoints {
    #[inline(always)]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 = self.0.wrapping_sub(rhs.0);
    }
}

impl core::ops::Neg for ScaledPoints {
    type Output = Self;
    #[inline(always)]
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl core::ops::Mul<i32> for ScaledPoints {
    type Output = Self;
    #[inline(always)]
    fn mul(self, rhs: i32) -> Self {
        Self(self.0.wrapping_mul(rhs))
    }
}

impl core::ops::Div<i32> for ScaledPoints {
    type Output = Self;
    #[inline(always)]
    fn div(self, rhs: i32) -> Self {
        Self(self.0 / rhs)
    }
}

impl core::fmt::Display for ScaledPoints {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:.5}pt", self.to_pt())
    }
}

/// Compact 16-byte fixed-point spatial bounding box for math atoms and sub-expressions.
///
/// Uses Donald Knuth's exact 32-bit scaled points (`ScaledPoints`) to eliminate floating-point
/// rounding drift across deep sub-expression trees and matrix alignments.
/// Exactly four bounding boxes fit per 64-byte L1 CPU cache line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(C)]
pub struct SpMathBox {
    /// Typographic horizontal width in scaled points (sp).
    pub w: ScaledPoints,
    /// Height above the baseline in scaled points (sp).
    pub h: ScaledPoints,
    /// Depth below the baseline in scaled points (sp).
    pub d: ScaledPoints,
    /// Vertical baseline shift offset in scaled points (sp).
    pub shift: ScaledPoints,
}

const _: () = assert!(core::mem::size_of::<SpMathBox>() == 16);

impl SpMathBox {
    /// Creates an empty math box with zero dimensions.
    #[inline(always)]
    pub const fn zero() -> Self {
        Self {
            w: ScaledPoints::ZERO,
            h: ScaledPoints::ZERO,
            d: ScaledPoints::ZERO,
            shift: ScaledPoints::ZERO,
        }
    }

    /// Creates a math box with explicit width, height, and depth.
    #[inline(always)]
    pub const fn with_dims(w: ScaledPoints, h: ScaledPoints, d: ScaledPoints) -> Self {
        Self {
            w,
            h,
            d,
            shift: ScaledPoints::ZERO,
        }
    }

    /// Total vertical extent of the box ($h + d$).
    #[inline(always)]
    pub fn total_h(&self) -> ScaledPoints {
        self.h + self.d
    }

    /// Converts this fixed-point box into floating-point points (pt).
    #[inline(always)]
    pub fn to_math_box(&self) -> MathBox {
        MathBox {
            w: self.w.to_pt(),
            h: self.h.to_pt(),
            d: self.d.to_pt(),
            shift: self.shift.to_pt(),
        }
    }

    /// Creates a fixed-point box from a floating-point box.
    #[inline(always)]
    pub fn from_math_box(b: &MathBox) -> Self {
        b.to_sp_box()
    }

    /// Horizontally joins two boxes sequentially: accumulates width and computes bounding vertical extents.
    #[inline(always)]
    pub fn append_horiz(&mut self, next: &SpMathBox) {
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
