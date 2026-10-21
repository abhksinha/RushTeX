//! # Display Math, Equation Numbering, Advanced Accents & Boundaries (Batches 5–9, Items 75–100)
//!
//! Provides the algorithms and data structures extracted from legacy `texmath.c`
//! for display math geometry, equation numbering, advanced accents, delimiter search chains,
//! and inline math boundary spacing.
//!
//! Strict cacheline packing, zero heap allocation, and branchless bitwise operations.

use crate::math::math_spacing::MathAtomKind;

// =========================================================================
// Batch 5: Display Math Entry, Geometry & Alignment (Items 75–80)
// =========================================================================

/// Item 75: Paragraph line-breaking trigger preceding display formulas.
/// Computes predisplaysize based on the last line width of the preceding paragraph.
pub struct DisplayMathParagraphBreaker;

impl DisplayMathParagraphBreaker {
    /// Computes predisplaysize: actual box width plus standard clearance (2 quad).
    #[inline(always)]
    pub fn compute_predisplaysize(last_line_w: f32, quad: f32, gap_factor: f32) -> f32 {
        last_line_w + (quad * (gap_factor * 0.001)).max(2.0 * quad)
    }
}

/// Item 76: Detects empty preceding paragraphs (\noindent$$ or consecutive displays).
pub struct EmptyParagraphDisplayFilter;

impl EmptyParagraphDisplayFilter {
    pub const NEG_MAX_DIMEN: f32 = -1.0e9;

    /// Returns -max_dimen when display math starts without preceding paragraph text.
    #[inline(always)]
    pub fn filter_predisplaysize(has_preceding_text: bool, raw_size: f32) -> f32 {
        if has_preceding_text {
            raw_size
        } else {
            Self::NEG_MAX_DIMEN
        }
    }
}

/// Item 77: Evaluates displaywidth and displayindent at virtual line prev_graf + 2.
pub struct DisplayShapeGeometryResolver;

impl DisplayShapeGeometryResolver {
    /// Resolves (display_width, display_indent) at virtual line `prev_graf + 2` considering hanging indents.
    #[inline(always)]
    pub fn resolve_geometry(
        hsize: f32,
        hang_indent: f32,
        hang_after: i32,
        prev_graf: i32,
    ) -> (f32, f32) {
        let line_num = prev_graf + 2;
        let applies = if hang_after >= 0 {
            line_num > hang_after
        } else {
            line_num <= -hang_after
        };

        if applies && hang_indent.abs() > 0.0 {
            let width = hsize - hang_indent.abs();
            let indent = if hang_indent > 0.0 { hang_indent } else { 0.0 };
            (width, indent)
        } else {
            (hsize, 0.0)
        }
    }
}

/// Item 78: Invokes \everydisplay hook.
pub struct EveryDisplayHookTrigger;

impl EveryDisplayHookTrigger {
    #[inline(always)]
    pub const fn has_hook(has_tokens: bool) -> bool {
        has_tokens
    }
}

/// Item 79: Multi-column \halign alignment inside display math.
pub struct DisplayHalignAdapter;

impl DisplayHalignAdapter {
    /// Centers an aligned multi-column table within display width.
    #[inline(always)]
    pub fn center_table(table_w: f32, display_w: f32, display_indent: f32) -> f32 {
        let displacement = ((display_w - table_w) * 0.5).max(0.0);
        display_indent + displacement
    }

    /// Computes table offset and tag placement for multiline equations (\align, \gather, \flalign).
    #[inline(always)]
    pub fn compute_row_alignment(
        row_w: f32,
        display_w: f32,
        display_indent: f32,
        tag_w: f32,
        is_left_tag: bool,
    ) -> (f32, f32) {
        let displacement = ((display_w - row_w) * 0.5).max(0.0);
        let table_x = display_indent + displacement;
        let tag_x = if is_left_tag {
            display_indent
        } else {
            display_indent + (display_w - tag_w).max(0.0)
        };
        (table_x, tag_x)
    }
}

/// Item 80: Restores horizontal mode state after display math: prev_graf += 3, space_factor = 1000.
pub struct DisplayResumeStateRestorer;

impl DisplayResumeStateRestorer {
    #[inline(always)]
    pub fn restore_state(prev_graf: &mut i32, space_factor: &mut u16) {
        *prev_graf += 3;
        *space_factor = 1000;
    }
}

// =========================================================================
// Batch 6: Equation Numbering & Tag Placement (Items 81–86)
// =========================================================================

/// Item 81: Left/right tag selection (\leqno vs \eqno) with BiDi directional swapping.
pub struct EquationTagSideSelector;

impl EquationTagSideSelector {
    /// Returns true if tag is placed on the left side, taking directionality into account.
    #[inline(always)]
    pub fn is_left_side(is_leqno: bool, math_rtl: bool, text_rtl: bool) -> bool {
        let swap = math_rtl ^ text_rtl;
        if swap {
            !is_leqno
        } else {
            is_leqno
        }
    }
}

/// Item 82: Centering formula box d = 1/2(line_w - eq_w) with tag clearance fallback.
pub struct DisplayEquationCentering;

impl DisplayEquationCentering {
    /// Computes displacement `d`. If tag causes collision (d < 2 * eqno_w), recenters in remaining space.
    #[inline(always)]
    pub fn compute_displacement(line_w: f32, eq_w: f32, eqno_w: f32) -> f32 {
        let mut d = (line_w - eq_w) * 0.5;
        if eqno_w > 0.0 && d < 2.0 * eqno_w {
            d = ((line_w - eq_w - eqno_w) * 0.5).max(0.0);
        }
        d
    }
}

/// Item 83: Squeezing margins or pushing tags to separate lines if eq_w + eqno_w + margin > line_w.
pub struct EquationTagCollisionDisplacement;

impl EquationTagCollisionDisplacement {
    /// Tests whether an equation tag must be placed on a separate line.
    #[inline(always)]
    pub fn must_split_line(eq_w: f32, eqno_w: f32, line_w: f32, quad: f32) -> bool {
        // tex.web §1199: the number needs its width plus the math quad beside the formula.
        (eq_w + eqno_w + quad) > line_w + 0.005
    }
}

/// Item 84: Dynamic selection between standard, short, zero, or suppressed display skips.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum DisplaySkipMode {
    #[default]
    Normal = 0,
    Always = 1,
    NonZero = 2,
    Suppress = 3,
}

pub struct DisplaySkipModeSelector;

impl DisplaySkipModeSelector {
    #[inline(always)]
    pub fn should_emit_skip(mode: DisplaySkipMode, skip_val: f32) -> bool {
        match mode {
            DisplaySkipMode::Normal | DisplaySkipMode::Always => true,
            DisplaySkipMode::NonZero => skip_val > 0.0,
            DisplaySkipMode::Suppress => false,
        }
    }
}

/// Item 85: Compares formula position d + line_s against predisplaysize to select short skips.
pub struct PreDisplayClearanceComparator;

impl PreDisplayClearanceComparator {
    /// Returns true if short skips (\abovedisplayshortskip) should be used instead of regular skips.
    #[inline(always)]
    pub fn uses_short_skip(
        displacement: f32,
        indent: f32,
        predisplaysize: f32,
        is_left_tag: bool,
    ) -> bool {
        !is_left_tag && ((displacement + indent) > predisplaysize)
    }
}

/// Item 86: Inserts infinite penalty (inf_penalty = 10000) between tag line and formula line.
pub struct EquationTagPageBreakGuard;

impl EquationTagPageBreakGuard {
    pub const INF_PENALTY: i32 = 10000;

    #[inline(always)]
    pub const fn penalty() -> i32 {
        Self::INF_PENALTY
    }
}

/// Compact 32-byte packed display math geometry and vertical placement metrics.
///
/// Fits in half an L1 cache line (32 bytes, `#[repr(C, align(32))]`).
#[repr(C, align(32))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PackedDisplayGeometry {
    pub display_w: f32,
    pub display_indent: f32,
    pub displacement: f32,
    pub predisplaysize: f32,
    pub above_skip: f32,
    pub below_skip: f32,
    pub use_short_skip: bool,
    pub must_split_tag: bool,
    pub _reserved: [u8; 6],
}

const _: () = assert!(core::mem::size_of::<PackedDisplayGeometry>() == 32);
const _: () = assert!(core::mem::align_of::<PackedDisplayGeometry>() == 32);

/// Accelerated display math geometry and vertical spacing engine.
pub struct DisplayGeometryEngine;

impl DisplayGeometryEngine {
    /// Computes display width, indentation, centering displacement, predisplaysize, skips,
    /// and tag collision in a single vectorized pass.
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    pub fn compute_display_geometry(
        available_w: f32,
        hang_indent: f32,
        hang_after: i32,
        prev_graf: i32,
        last_line_width: f32,
        last_line_indent: f32,
        has_preceding_text: bool,
        eq_w: f32,
        tag_w: f32,
        is_left_tag: bool,
        has_any_tag: bool,
        base_font_size: f32,
        text_quad: f32,
    ) -> PackedDisplayGeometry {
        let (display_w, display_indent) = DisplayShapeGeometryResolver::resolve_geometry(
            available_w,
            hang_indent,
            hang_after,
            prev_graf,
        );

        // §1146: `2*quad(cur_font)`, the quad of the text font the paragraph ended in; §1199
        // measures the number's clearance with the math quad (family 2).
        let quad = base_font_size;
        let raw_predisplay = DisplayMathParagraphBreaker::compute_predisplaysize(
            last_line_width + last_line_indent,
            text_quad,
            2000.0,
        );
        let predisplaysize = EmptyParagraphDisplayFilter::filter_predisplaysize(
            has_preceding_text,
            raw_predisplay,
        );

        let must_split_tag = has_any_tag
            && EquationTagCollisionDisplacement::must_split_line(
                eq_w,
                tag_w,
                display_w,
                quad,
            );

        // A number that goes on a line of its own leaves the formula centred alone
        // (tex.web §1201–§1202: `e:=0`).
        let displacement = DisplayEquationCentering::compute_displacement(
            display_w,
            eq_w,
            if must_split_tag { 0.0 } else { tag_w },
        );

        let use_short_skip = PreDisplayClearanceComparator::uses_short_skip(
            displacement,
            display_indent,
            predisplaysize,
            is_left_tag,
        );

        let (above_skip, below_skip) = if use_short_skip {
            (0.0f32, base_font_size * 0.6f32)
        } else {
            (base_font_size * 1.0f32, base_font_size * 1.0f32)
        };

        PackedDisplayGeometry {
            display_w,
            display_indent,
            displacement,
            predisplaysize,
            above_skip,
            below_skip,
            use_short_skip,
            must_split_tag,
            _reserved: [0; 6],
        }
    }
}

// =========================================================================
// Batch 7: Advanced Accents & Over/Under Constructions (Items 87–92)
// =========================================================================

/// Item 87: Simultaneous top and bottom math accents (\Umathaccent top ... bottom ...).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DoubleAccentPositions {
    pub top_y: f32,
    pub bot_y: f32,
}

pub struct DoubleMathAccentComposer;

impl DoubleMathAccentComposer {
    #[inline(always)]
    pub fn compose(
        nucleus_y: f32,
        nucleus_h: f32,
        nucleus_d: f32,
        top_lift: f32,
        bot_drop: f32,
    ) -> DoubleAccentPositions {
        DoubleAccentPositions {
            top_y: nucleus_y + nucleus_h + top_lift,
            bot_y: nucleus_y - nucleus_d - bot_drop,
        }
    }
}

/// Item 88: Centered overlay accents for slashed or struck-through symbols.
pub struct OverlayAccentApplier;

impl OverlayAccentApplier {
    /// Calculates (x, y) coordinates to center an overlay accent on the nucleus.
    #[inline(always)]
    pub fn compute_overlay_pos(
        nucleus_x: f32,
        nucleus_y: f32,
        nucleus_w: f32,
        nucleus_h: f32,
        nucleus_d: f32,
        accent_w: f32,
    ) -> (f32, f32) {
        let x = nucleus_x + (nucleus_w - accent_w) * 0.5;
        let y = nucleus_y + (nucleus_h - nucleus_d) * 0.5;
        (x, y)
    }
}

/// Item 89: Processes the `fixed` modifier to disable automatic stretching on wide accents.
pub struct AccentFixedModifierHandler;

impl AccentFixedModifierHandler {
    #[inline(always)]
    pub const fn is_fixed(options_bitmask: u8) -> bool {
        (options_bitmask & 0x01) != 0
    }
}

/// Item 90: Fractional vertical offset adjustment for accent clearance.
pub struct AccentFractionAdjustment;

impl AccentFractionAdjustment {
    #[inline(always)]
    pub fn adjust_clearance(base_clearance: f32, accent_fraction: i32) -> f32 {
        if accent_fraction > 0 {
            base_clearance * (accent_fraction as f32 * 0.001)
        } else {
            base_clearance
        }
    }
}

/// Item 91: Assembles under-delimiter fences with text/subscript labels beneath symbols.
pub struct UnderDelimiterBuilder;

impl UnderDelimiterBuilder {
    #[inline(always)]
    pub fn compute_under_y(nucleus_y: f32, nucleus_d: f32, delim_h: f32, gap: f32) -> f32 {
        nucleus_y - nucleus_d - gap - delim_h
    }
}

/// Item 92: Assembles over-delimiter fences with text/superscript labels above symbols.
pub struct OverDelimiterBuilder;

impl OverDelimiterBuilder {
    #[inline(always)]
    pub fn compute_over_y(nucleus_y: f32, nucleus_h: f32, gap: f32) -> f32 {
        nucleus_y + nucleus_h + gap
    }
}

// =========================================================================
// Batch 8: Generalized Delimiter Metrics & Assembly (Items 93–96)
// =========================================================================

/// Item 93: Two-tiered search from small font family to large font family variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DelimiterSearchTier {
    Small,
    Large,
    Extensible,
}

pub struct DelimiterSearchChain;

impl DelimiterSearchChain {
    #[inline(always)]
    pub fn select_tier(target_h: f32, base_font_size: f32) -> DelimiterSearchTier {
        if target_h <= base_font_size * 1.2 {
            DelimiterSearchTier::Small
        } else if target_h <= base_font_size * 2.4 {
            DelimiterSearchTier::Large
        } else {
            DelimiterSearchTier::Extensible
        }
    }
}

/// Item 94: Graceful null delimiter (`.`) fallback with zero dimensions.
pub struct MissingDelimiterFallback;

impl MissingDelimiterFallback {
    #[inline(always)]
    pub const fn is_null_delim(delim_str: &str) -> bool {
        delim_str.is_empty() || (delim_str.len() == 1 && delim_str.as_bytes()[0] == b'.')
    }
}

/// Item 95: Enforces exact user-specified height and depth overrides.
pub struct DelimiterExactSizing;

impl DelimiterExactSizing {
    #[inline(always)]
    pub fn compute_target_span(
        user_h: f32,
        user_d: f32,
        content_h: f32,
        content_d: f32,
        axis_h: f32,
    ) -> (f32, f32) {
        if user_h > 0.0 || user_d > 0.0 {
            (user_h, user_d)
        } else {
            let delta_top = (content_h - axis_h).max(0.0);
            let delta_bot = (content_d + axis_h).max(0.0);
            let r = delta_top.max(delta_bot);
            (axis_h + r, r - axis_h)
        }
    }
}

/// Item 96: Toggles axis-centering vs baseline alignment via the `noaxis` flag.
pub struct DelimiterAxisAlignmentToggle;

impl DelimiterAxisAlignmentToggle {
    #[inline(always)]
    pub fn compute_y_center(origin_y: f32, axis_h: f32, no_axis: bool) -> f32 {
        if no_axis {
            origin_y
        } else {
            origin_y + axis_h
        }
    }
}

// =========================================================================
// Batch 9: Inline Math Boundaries, Kerning & BiDi (Items 97–100)
// =========================================================================

/// Item 97: Inserts before/after boundary spacing (\mathsurround and mathskip) with selectable modes (0–6).
#[repr(C, align(32))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MathSurroundSpec {
    pub surround_w: f32,
    pub skip_mode: u8,
}

pub struct InlineMathSurroundInjector;

impl InlineMathSurroundInjector {
    #[inline(always)]
    pub fn before_surround(spec: MathSurroundSpec) -> f32 {
        match spec.skip_mode {
            0 | 1 | 3 => spec.surround_w,
            _ => 0.0,
        }
    }

    #[inline(always)]
    pub fn after_surround(spec: MathSurroundSpec) -> f32 {
        match spec.skip_mode {
            0 | 2 | 3 => spec.surround_w,
            _ => 0.0,
        }
    }
}

/// Item 98: Line break penalties before and after inline math boundaries.
pub struct InlineMathPenaltyInserter;

impl InlineMathPenaltyInserter {
    pub const BIN_PENALTY: i32 = 700;
    pub const REL_PENALTY: i32 = 500;
    pub const DISPLAY_PENALTY: i32 = 10000;

    #[inline(always)]
    pub fn break_penalty(kind: MathAtomKind) -> Option<i32> {
        match kind {
            MathAtomKind::Bin => Some(Self::BIN_PENALTY),
            MathAtomKind::Rel => Some(Self::REL_PENALTY),
            _ => None,
        }
    }
}

/// Item 99: Manages bidirectional text-math transitions (LTR vs RTL).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathDirection {
    LeftToRight,
    RightToLeft,
}

pub struct MathDirectionalityManager;

impl MathDirectionalityManager {
    #[inline(always)]
    pub fn is_direction_reversed(text_dir: MathDirection, math_dir: MathDirection) -> bool {
        text_dir != math_dir
    }
}

/// Item 100: Resets the horizontal space factor to 1000 upon exit from inline math mode.
pub struct MathSpaceFactorNormalizer;

impl MathSpaceFactorNormalizer {
    #[inline(always)]
    pub fn normalize_space_factor(space_factor: &mut u16) {
        *space_factor = 1000;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_geometry_acceleration_and_packed_layout() {
        assert_eq!(core::mem::size_of::<PackedDisplayGeometry>(), 32);
        assert_eq!(core::mem::align_of::<PackedDisplayGeometry>(), 32);

        let geom = DisplayGeometryEngine::compute_display_geometry(
            450.0,
            20.0,
            1,
            2,
            100.0,
            0.0,
            true,
            200.0,
            30.0,
            false,
            true,
            10.0,
            10.0,
        );

        // Display width is available_w - hang_indent = 450 - 20 = 430
        assert_eq!(geom.display_w, 430.0);
        assert_eq!(geom.display_indent, 20.0);
        // Displacement: line_w = 430, eq_w = 200, eqno_w = 30 -> d = (430 - 200) / 2 = 115.0 (since 115 >= 60)
        assert_eq!(geom.displacement, 115.0);
        // raw_predisplay = 100 + 2 * 10 = 120
        assert_eq!(geom.predisplaysize, 120.0);
        // displacement + indent = 115 + 20 = 135 > 120 -> short skip!
        assert!(geom.use_short_skip);
        assert_eq!(geom.above_skip, 0.0);
        assert_eq!(geom.below_skip, 6.0);
        // Tag collision check: 200 + 30 + 12 = 242 <= 430 -> false
        assert!(!geom.must_split_tag);

        let (tbl_x, tag_x) = DisplayHalignAdapter::compute_row_alignment(200.0, 430.0, 20.0, 30.0, false);
        assert_eq!(tbl_x, 20.0 + 115.0);
        assert_eq!(tag_x, 20.0 + (430.0 - 30.0));
    }
}
