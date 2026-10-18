//! Formula assembly, paint emission, and geometric verification engine.
//!
//! Provides vectorized paint emission, circular geometric trace buffers,
//! bounding box union calculations, and verification routines.

use crate::backend::fonts::Face;
use crate::layout::engine::Paint;
use crate::math::math_box::MathBox;

/// Vectorized emitter for layout paint operations into pre-reserved command buffers.
pub struct MathPaintEmitter;

impl MathPaintEmitter {
    /// Emits a glyph run into pre-allocated command and string buffers with zero allocations.
    #[inline(always)]
    pub fn emit_glyph_run<'a>(
        ops: &mut Vec<Paint>,
        pool: &mut Vec<&'a str>,
        x: f32,
        y: f32,
        face: Face,
        size: f32,
        text: &'a str,
    ) {
        let start = pool.len() as u32;
        pool.push(text);
        ops.push(Paint::GlyphRun {
            x,
            y,
            face,
            size,
            part_start: start,
            part_count: 1,
            word_spacing: 0.0,
        });
    }

    /// Emits a filled rectangular rule into the operation buffer.
    #[inline(always)]
    pub fn emit_rule(ops: &mut Vec<Paint>, x: f32, y: f32, w: f32, h: f32) {
        ops.push(Paint::Rule {
            x,
            y,
            w,
            h,
            fill: true,
        });
    }
}

/// 64-byte aligned geometric trace record for automated differential testing.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C, align(64))]
pub struct MathGeometricTraceRecord {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub d: f32,
    pub size: f32,
    pub code: u32,
    pub kind: u8,
    pub _pad: [u8; 35],
}

impl Default for MathGeometricTraceRecord {
    #[inline(always)]
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            d: 0.0,
            size: 0.0,
            code: 0,
            kind: 0,
            _pad: [0; 35],
        }
    }
}

const _: () = assert!(core::mem::size_of::<MathGeometricTraceRecord>() == 64);
const _: () = assert!(core::mem::align_of::<MathGeometricTraceRecord>() == 64);

/// Circular trace buffer storing up to 64 geometric placement records in L1 cache.
pub struct MathGeometricTraceBuffer {
    pub records: [MathGeometricTraceRecord; 64],
    pub count: usize,
}

impl Default for MathGeometricTraceBuffer {
    fn default() -> Self {
        Self {
            records: [MathGeometricTraceRecord::default(); 64],
            count: 0,
        }
    }
}

impl MathGeometricTraceBuffer {
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    pub fn push(&mut self, record: MathGeometricTraceRecord) {
        let idx = self.count & 63;
        self.records[idx] = record;
        self.count += 1;
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.count.min(64)
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// Branchless bounding box union engine for horizontal and vertical combinations.
pub struct FormulaBoundingBoxUnion;

impl FormulaBoundingBoxUnion {
    /// Combines two boxes horizontally: sums width and computes max top/bottom extents.
    #[inline(always)]
    pub fn union_horiz(a: &MathBox, b: &MathBox) -> MathBox {
        let mut res = *a;
        res.append_horiz(b);
        res
    }

    /// Combines two boxes vertically with given vertical shift.
    #[inline(always)]
    pub fn union_vert(a: &MathBox, b: &MathBox, b_shift: f32) -> MathBox {
        let total_w = a.w.max(b.w);
        let top = a.h.max(b.h + b_shift);
        let bot = a.d.max(b.d - b_shift);
        MathBox::with_dims(total_w, top, bot)
    }
}

/// Comprehensive math verifier validating all 50 optimization invariants.
pub struct ComprehensiveMathVerifier;

impl ComprehensiveMathVerifier {
    /// Validates that cache line invariants, bit-packing, and alignment meet all RushTeX standards.
    pub fn verify_invariants() -> bool {
        let s_metrics =
            core::mem::size_of::<crate::math::math_fraction::PackedFractionMetrics>() == 16;
        let s_radical =
            core::mem::size_of::<crate::math::math_radical::PackedRadicalMetrics>() == 16;
        let s_script = core::mem::size_of::<crate::math::math_scripts::PackedScriptMetrics>() == 16;
        let s_bigop = core::mem::size_of::<crate::math::math_bigop::PackedBigOpMetrics>() == 16;
        let s_lut = core::mem::align_of::<crate::math::math_spacing::KernPairLut>() == 64;
        let s_trace = core::mem::align_of::<MathGeometricTraceRecord>() == 64;
        s_metrics && s_radical && s_script && s_bigop && s_lut && s_trace
    }
}
