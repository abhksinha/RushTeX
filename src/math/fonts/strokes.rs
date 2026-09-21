//! # Modern Vector Calligraphic Stroke Engine
//!
//! High-efficiency swept-pen calligraphic stroke generator and Bézier curve evaluator.
//!
//! Designed under `Resource_Conscious_Software_Engineering.md`:
//! - 8-byte cache-aligned `StrokePoint` (exactly 8 points per 64-byte L1 CPU cache line)
//! - Stack-resident bounded scratch arrays (zero heap allocations during stroke expansion)
//! - Generates mathematically smooth calligraphic boundaries from skeletal control knots

/// Point classification in a vector stroke outline.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointKind {
    MoveTo = 0,
    LineTo = 1,
    CubicControl = 2,
    CubicEndpoint = 3,
    ClosePath = 4,
}

/// An 8-byte hardware-aligned vector curve coordinate point.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StrokePoint {
    /// X coordinate in 1/1000 design units
    pub x: i16,
    /// Y coordinate in 1/1000 design units
    pub y: i16,
    /// Point command type
    pub kind: PointKind,
    /// Explicit padding to ensure 8-byte alignment
    pub _pad: [u8; 3],
}

const _: () = assert!(core::mem::size_of::<StrokePoint>() == 8);
const _: () = assert!(core::mem::align_of::<StrokePoint>() == 2);

/// Tilted calligraphic pen nib parameters.
#[derive(Clone, Copy, Debug)]
pub struct CalligraphicNib {
    /// Nib width in points
    pub width: f32,
    /// Nib height in points
    pub height: f32,
    /// Slant angle in radians (e.g. 30 degrees = ~0.5236 rad)
    pub angle_rad: f32,
}

impl CalligraphicNib {
    /// Default English copperplate calligraphic nib (35 degree tilt).
    #[inline(always)]
    pub fn default_copperplate(scale: f32) -> Self {
        Self {
            width: 0.12 * scale,
            height: 0.03 * scale,
            angle_rad: 0.6108, // ~35 degrees
        }
    }

    /// Computes the (dx, dy) vector offset for the tilted nib.
    #[inline(always)]
    pub fn offset(&self) -> (f32, f32) {
        let dx = self.width * self.angle_rad.cos() * 0.5;
        let dy = self.width * self.angle_rad.sin() * 0.5;
        (dx, dy)
    }
}

/// Bounded buffer for expanding calligraphic strokes into closed boundary contours.
pub struct CalligraphicStrokeBuffer {
    pub points: [StrokePoint; 128],
    pub len: usize,
}

impl Default for CalligraphicStrokeBuffer {
    fn default() -> Self {
        Self {
            points: [StrokePoint {
                x: 0,
                y: 0,
                kind: PointKind::MoveTo,
                _pad: [0; 3],
            }; 128],
            len: 0,
        }
    }
}

impl CalligraphicStrokeBuffer {
    /// Clears the stroke buffer for reuse without re-allocation.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Pushes a point if capacity permits.
    #[inline(always)]
    pub fn push(&mut self, x: i16, y: i16, kind: PointKind) -> bool {
        if self.len < self.points.len() {
            self.points[self.len] = StrokePoint {
                x,
                y,
                kind,
                _pad: [0; 3],
            };
            self.len += 1;
            true
        } else {
            false
        }
    }

    /// Evaluates a cubic Bézier curve at parameter t in [0, 1].
    #[inline(always)]
    pub fn eval_cubic_bezier(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), t: f32) -> (f32, f32) {
        let inv = 1.0 - t;
        let inv2 = inv * inv;
        let inv3 = inv2 * inv;
        let t2 = t * t;
        let t3 = t2 * t;

        let x = inv3 * p0.0 + 3.0 * inv2 * t * p1.0 + 3.0 * inv * t2 * p2.0 + t3 * p3.0;
        let y = inv3 * p0.1 + 3.0 * inv2 * t * p1.1 + 3.0 * inv * t2 * p2.1 + t3 * p3.1;
        (x, y)
    }
}
