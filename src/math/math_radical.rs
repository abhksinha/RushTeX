//! Radical and root geometric construction engine (TeX Appendix G Rule 11).
//!
//! Provides packed radical metrics, branchless clearance evaluation,
//! CMEX10 variant selection, symmetrical excess distribution, and root degree positioning.

use crate::math::math_box::MathBox;

/// 16-byte packed structure encoding radical geometry parameters.
///
/// Designed to pack exactly four instances per 64-byte L1 cache line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct PackedRadicalMetrics {
    /// Thickness of the radical horizontal overbar ($\theta$).
    pub rule_thickness: f32,
    /// Minimum clearance between radicand top and radical overbar ($\psi$).
    pub min_clearance: f32,
    /// Symmetrical clearance excess distributed above/below radicand.
    pub excess: f32,
    /// Vertical shift applied to the radical glyph to align with overbar.
    pub radical_shift: f32,
}

const _: () = assert!(core::mem::size_of::<PackedRadicalMetrics>() == 16);

impl PackedRadicalMetrics {
    #[inline(always)]
    pub const fn new(
        rule_thickness: f32,
        min_clearance: f32,
        excess: f32,
        radical_shift: f32,
    ) -> Self {
        Self {
            rule_thickness,
            min_clearance,
            excess,
            radical_shift,
        }
    }
}

/// Branchless governor evaluating TeX Appendix G Rule 11 clearances.
pub struct RadicalClearanceGovernor;

impl RadicalClearanceGovernor {
    /// Computes minimum clearance $\psi$ for Rule 11:
    /// - Display style: $\xi_8 + \theta / 4$
    /// - Non-display styles: $\theta + \theta / 4$
    #[inline(always)]
    pub fn min_clearance(
        rule_thickness: f32,
        default_rule_thickness: f32,
        is_display: bool,
    ) -> f32 {
        let base = if is_display {
            default_rule_thickness
        } else {
            rule_thickness
        };
        base + 0.25 * rule_thickness
    }
}

/// 64-byte aligned static lookup table mapping radicand height directly to radical glyph variants.
#[repr(C, align(64))]
pub struct RadicalVariantSelector {
    /// Height thresholds for CMEX10 radical sizes in points.
    pub height_thresholds: [f32; 4],
    /// Glyph codes in CMEX10: 0x70 (small), 0x71 (medium), 0x72 (large), 0x73 (xlarge).
    pub glyph_codes: [u8; 4],
    /// Extensible piece glyph codes: top, bottom, rep.
    pub extensible_pieces: [u8; 4],
    pub _pad: [u8; 40],
}

pub static RADICAL_VARIANT_SELECTOR: RadicalVariantSelector = RadicalVariantSelector {
    height_thresholds: [10.0, 15.0, 22.0, 30.0],
    glyph_codes: [0x70, 0x71, 0x72, 0x73],
    extensible_pieces: [0x74, 0x75, 0x76, 0x00],
    _pad: [0; 40],
};

const _: () = assert!(core::mem::size_of::<RadicalVariantSelector>() == 64);
const _: () = assert!(core::mem::align_of::<RadicalVariantSelector>() == 64);

impl RadicalVariantSelector {
    /// Resolves the radical variant glyph code for a given target inner height.
    #[inline(always)]
    pub fn select_variant(&self, target_h: f32) -> (u8, bool) {
        if target_h <= self.height_thresholds[0] {
            (self.glyph_codes[0], false)
        } else if target_h <= self.height_thresholds[1] {
            (self.glyph_codes[1], false)
        } else if target_h <= self.height_thresholds[2] {
            (self.glyph_codes[2], false)
        } else if target_h <= self.height_thresholds[3] {
            (self.glyph_codes[3], false)
        } else {
            (self.extensible_pieces[0], true) // Extensible assembly required
        }
    }
}

/// Symmetrical clearance excess distributor for radicals (eliminates $\sqrt{\pi}$ shift defect).
pub struct RadicalExcessDistributor;

impl RadicalExcessDistributor {
    /// Symmetrically divides any excess clearance between top overbar and bottom clearance.
    #[inline(always)]
    pub fn distribute_excess(actual_h: f32, needed_h: f32) -> (f32, f32) {
        if actual_h > needed_h {
            let excess = actual_h - needed_h;
            let half_excess = 0.5 * excess;
            (half_excess, half_excess)
        } else {
            (0.0, 0.0)
        }
    }
}

/// Precomputed positioner for radical degree indices (e.g. $\sqrt[n]{x}$).
pub struct RootDegreePositioner;

impl RootDegreePositioner {
    /// Computes $(x, y)$ coordinate offsets for the degree box relative to radical origin.
    #[inline(always)]
    pub fn degree_offset(rad_w: f32, rad_h: f32, deg_box: &MathBox) -> (f32, f32) {
        let deg_x = 0.2 * rad_w - deg_box.w * 0.4;
        let deg_y = 0.55 * rad_h - deg_box.d;
        (deg_x, deg_y)
    }
}

/// Zero-heap radical assembler combining radical symbol and horizontal overbar.
pub struct ExtensibleRadicalAssembler;

impl ExtensibleRadicalAssembler {
    /// Computes overall radical bounding box including radicand, overbar, and padding.
    #[inline(always)]
    pub fn compute_box(
        radicand_box: &MathBox,
        radical_w: f32,
        rule_t: f32,
        clearance: f32,
    ) -> MathBox {
        let total_w = radical_w + radicand_box.w + 0.15 * radical_w;
        let total_h = radicand_box.h + clearance + rule_t;
        let total_d = radicand_box.d;
        MathBox::with_dims(total_w, total_h, total_d)
    }
}
