//! # Modern OpenType Math Layout Engine
//!
//! Converts OpenType MathConstants and font metrics into exact geometric dimensions
//! conforming to ISO/IEC 14496-22 and Microsoft OpenType mathematical layout specifications.
//!
//! Designed under `native-architecture.md` §11b (resource rules):
//! - Pure math calculations, zero memory allocations
//! - Hardware L1 cache resident via 64-byte aligned constants
//! - Branchless min/max arithmetic

use super::math_constants::OpenTypeMathConstants;
use crate::math::math_ast::MathStyle;

/// High-speed geometry engine driving modern OpenType mathematical layout.
#[derive(Clone, Copy, Debug)]
pub struct OpenTypeMathEngine<'a> {
    constants: &'a OpenTypeMathConstants,
    font_size: f32,
}

impl<'a> OpenTypeMathEngine<'a> {
    /// Creates a new layout engine configured for the specified constants and font size.
    #[inline(always)]
    pub fn new(constants: &'a OpenTypeMathConstants, font_size: f32) -> Self {
        Self {
            constants,
            font_size,
        }
    }

    /// Returns the underlying OpenTypeMathConstants table reference.
    #[inline(always)]
    pub fn constants(&self) -> &'a OpenTypeMathConstants {
        self.constants
    }

    /// Scale factor for design units to points at the base font size.
    #[inline(always)]
    fn scale(&self, val: i16) -> f32 {
        (val as f32 * 0.001) * self.font_size
    }

    /// Distance from the baseline to the mathematical axis.
    #[inline(always)]
    pub fn axis_height(&self) -> f32 {
        self.scale(self.constants.axis_height)
    }

    /// Default rule thickness for fraction bars and radical roofs.
    #[inline(always)]
    pub fn fraction_rule_thickness(&self) -> f32 {
        self.scale(self.constants.fraction_rule_thickness).max(0.4)
    }

    /// Numerator baseline shift above formula origin.
    #[inline(always)]
    pub fn fraction_numerator_shift(&self, is_display: bool) -> f32 {
        if is_display {
            self.scale(self.constants.fraction_numerator_display_style_shift_up)
        } else {
            self.scale(self.constants.fraction_numerator_shift_up)
        }
    }

    /// Denominator baseline shift below formula origin.
    #[inline(always)]
    pub fn fraction_denominator_shift(&self, is_display: bool) -> f32 {
        if is_display {
            self.scale(self.constants.fraction_denominator_display_style_shift_down)
        } else {
            self.scale(self.constants.fraction_denominator_shift_down)
        }
    }

    /// Minimum clearance gap between numerator bottom and fraction bar.
    #[inline(always)]
    pub fn fraction_numerator_gap_min(&self, is_display: bool) -> f32 {
        if is_display {
            self.scale(self.constants.fraction_num_display_style_gap_min)
        } else {
            self.scale(self.constants.fraction_numerator_gap_min)
        }
    }

    /// Minimum clearance gap between denominator top and fraction bar.
    #[inline(always)]
    pub fn fraction_denominator_gap_min(&self, is_display: bool) -> f32 {
        if is_display {
            self.scale(self.constants.fraction_denom_display_style_gap_min)
        } else {
            self.scale(self.constants.fraction_denominator_gap_min)
        }
    }

    /// Thickness of radical rule.
    #[inline(always)]
    pub fn radical_rule_thickness(&self) -> f32 {
        self.scale(self.constants.radical_rule_thickness).max(0.4)
    }

    /// Vertical clearance gap between radicand top and radical roof.
    #[inline(always)]
    pub fn radical_vertical_gap(&self, is_display: bool) -> f32 {
        if is_display {
            self.scale(self.constants.radical_display_style_vertical_gap)
        } else {
            self.scale(self.constants.radical_vertical_gap)
        }
    }

    /// Percentage to raise root degree index relative to radical bounding height.
    #[inline(always)]
    pub fn radical_degree_bottom_raise_percent(&self) -> f32 {
        self.constants.radical_degree_bottom_raise_percent as f32 * 0.01
    }

    /// Default subscript baseline shift down.
    #[inline(always)]
    pub fn subscript_shift_down(&self) -> f32 {
        self.scale(self.constants.subscript_shift_down)
    }

    /// Default superscript baseline shift up.
    #[inline(always)]
    pub fn superscript_shift_up(&self, is_cramped: bool) -> f32 {
        if is_cramped {
            self.scale(self.constants.superscript_shift_up_cramped)
        } else {
            self.scale(self.constants.superscript_shift_up)
        }
    }

    /// Minimum clearance gap between simultaneous sub and superscripts.
    #[inline(always)]
    pub fn sub_superscript_gap_min(&self) -> f32 {
        self.scale(self.constants.sub_superscript_gap_min)
    }

    /// Minimum gap above a big operator to its upper limit.
    #[inline(always)]
    pub fn upper_limit_gap_min(&self) -> f32 {
        self.scale(self.constants.upper_limit_gap_min).max(1.5)
    }

    /// Minimum gap below a big operator to its lower limit.
    #[inline(always)]
    pub fn lower_limit_gap_min(&self) -> f32 {
        self.scale(self.constants.lower_limit_gap_min).max(1.5)
    }

    /// Scaling factor for child style relative to current base font size.
    #[inline(always)]
    pub fn style_scale(&self, style: MathStyle) -> f32 {
        match style.tier() {
            0 => 1.0,
            1 => 1.0,
            2 => (self.constants.script_percent_scale_down as f32) * 0.01,
            _ => (self.constants.script_script_percent_scale_down as f32) * 0.01,
        }
    }
}
