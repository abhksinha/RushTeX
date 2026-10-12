//! High-performance mathematical vector layout engine.
//!
//! Implements TeX Appendix G rules for math layout, including atom spacing,
//! subscripts/superscripts, vertical fractions, radicals, big operators,
//! extensible delimiters, and matrices.

use crate::backend::fonts::{self as metrics, Face, FontFamily};
use crate::layout::engine::Paint;
use crate::math::fonts::MathFont;
use crate::math::math_ast::{
    BigOpData, ColAlign, DelimitedData, FontVariantTier, MathContextFlags, MathNode, MathStyle,
    MatrixData, MatrixDelim, MatrixEnvKind, StyleFrameStack,
};
use crate::math::math_box::MathBox;
use crate::math::math_fonts::{
    big_op_metrics, char_height_depth, delimiter_metrics, glyph_top_accent_offset, hbar_metrics,
    italic_correction, lookup_accent_metrics, math_font_parameters_styled,
    matrix_metrics, measure_char, resolve_cm_symbol, resolve_optical_face,
    symbol_height_depth_family, symbol_italic_correction,
    MathFontParameters, ASCII_GLYPH_STRS,
};
use crate::math::math_bigop::{
    BigOpShiftResolver, BigOpVariantTier, IntegralOverhangCorrector, LimitClearanceGovernor,
    OperatorCenteringEngine, OperatorLimitSwitch, Rule13OperatorAxis,
};
use crate::math::math_delim::{
    byte_str, resolve_cmex_delimiter, DelimiterBarRenderer, DelimiterSizingGovernor,
    ExtensibleAssemblyEngine, NullDelimiterEliminator, OverUnderDelimiterEngine,
    DELIMITER_VARIANT_RESOLVER,
};
use crate::math::math_fraction::{FractionShiftResolver, NumeratorDenominatorAligner};
use crate::math::math_scripts::{ScriptShiftResolver, TransientScriptPair};
use crate::math::math_spacing::{atom_spacing_mu, normalize_atom_kinds, MathAtomKind};

/// Context for laying out a mathematical formula.
pub struct MathLayoutContext<'a, 'p> {
    pub base_font_size: f32,
    pub font_family: FontFamily,
    pub face_override: Option<Face>,
    pub out_ops: Option<&'p mut Vec<Paint>>,
    pub text_pool: Option<&'p mut Vec<&'a str>>,
    pub font_tier: FontVariantTier,
    pub style_stack: StyleFrameStack,
    pub flags: MathContextFlags,
    pub target_middle_delim_h: Option<f32>,
}

impl<'a, 'p> MathLayoutContext<'a, 'p> {
    /// Creates a new layout context initialized with a root style.
    #[inline(always)]
    pub fn new(
        base_font_size: f32,
        font_family: FontFamily,
        face_override: Option<Face>,
        out_ops: Option<&'p mut Vec<Paint>>,
        text_pool: Option<&'p mut Vec<&'a str>>,
        root_style: MathStyle,
    ) -> Self {
        let mut flags = MathContextFlags::empty();
        if root_style.is_display() {
            flags.set(MathContextFlags::IN_DISPLAY);
        }
        Self {
            base_font_size,
            font_family,
            face_override,
            out_ops,
            text_pool,
            font_tier: FontVariantTier::new(base_font_size),
            style_stack: StyleFrameStack::new(root_style),
            flags,
            target_middle_delim_h: None,
        }
    }

    #[inline(always)]
    pub fn dry_context<'d>(&self) -> MathLayoutContext<'a, 'd> {
        MathLayoutContext {
            base_font_size: self.base_font_size,
            font_family: self.font_family,
            face_override: self.face_override,
            out_ops: None,
            text_pool: None,
            font_tier: self.font_tier,
            style_stack: self.style_stack,
            flags: self.flags,
            target_middle_delim_h: self.target_middle_delim_h,
        }
    }

    #[inline(always)]
    pub fn out_ops_len(&self) -> usize {
        self.out_ops.as_ref().map(|o| o.len()).unwrap_or(0)
    }

    #[inline(always)]
    pub fn shift_ops_range(&mut self, start: usize, end: usize, dx: f32, dy: f32) {
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        if let Some(ref mut ops) = self.out_ops {
            let actual_end = end.min(ops.len());
            if start < actual_end {
                for op in &mut ops[start..actual_end] {
                    match op {
                        Paint::GlyphRun { x, y, .. }
                        | Paint::Rule { x, y, .. }
                        | Paint::VectorGlyph { x, y, .. }
                        | Paint::CidGlyphRun { x, y, .. }
                        | Paint::Marker { x, y, .. }
                        | Paint::CaptionTag { x, y, .. }
                        | Paint::TableCaptionTag { x, y, .. }
                        | Paint::PageNumber { x, y, .. }
                        | Paint::SectionNumber { x, y, .. }
                        | Paint::FigureTag { x, y, .. }
                        | Paint::TableTag { x, y, .. }
                        | Paint::EquationNumber { x, y, .. } => {
                            *x += dx;
                            *y += dy;
                        }
                    }
                }
            }
        }
    }

    #[inline(always)]
    pub fn font_params(&self, style: MathStyle) -> &'static MathFontParameters {
        math_font_parameters_styled(self.font_family, style)
    }

    #[inline(always)]
    pub fn opentype_engine(&self) -> crate::math::fonts::OpenTypeMathEngine<'static> {
        let constants = match self.font_family {
            FontFamily::Times => &crate::math::fonts::STIX2_MATH_CONSTANTS,
            FontFamily::Helvetica => &crate::math::fonts::FIRAMATH_CONSTANTS,
            FontFamily::Palatino => &crate::math::fonts::ASANA_MATH_CONSTANTS,
            _ => &crate::math::fonts::NEWCM_MATH_CONSTANTS,
        };
        crate::math::fonts::OpenTypeMathEngine::new(constants, self.base_font_size)
    }

    #[inline(always)]
    pub fn is_measuring(&self) -> bool {
        self.out_ops.is_none()
    }

    #[inline(always)]
    pub fn emit_glyph_run(&mut self, x: f32, y: f32, face: Face, size: f32, text: &'a str) {
        if let (Some(ops), Some(pool)) =
            (self.out_ops.as_deref_mut(), self.text_pool.as_deref_mut())
        {
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
    }

    #[inline(always)]
    pub fn emit_rule(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if let Some(ops) = self.out_ops.as_deref_mut() {
            ops.push(Paint::Rule {
                x,
                y,
                w,
                h,
                fill: true,
            });
        }
    }

    #[inline(always)]
    pub fn emit_vector_glyph(&mut self, x: f32, y: f32, size: f32, glyph_id: u16) {
        if let Some(ops) = self.out_ops.as_deref_mut() {
            ops.push(Paint::VectorGlyph {
                x,
                y,
                size,
                glyph_id,
            });
        }
    }

    #[inline(always)]
    pub fn emit_cid_glyph(&mut self, x: f32, y: f32, size: f32, glyph_id: u16) {
        if let Some(ops) = self.out_ops.as_deref_mut() {
            ops.push(Paint::CidGlyphRun {
                x,
                y,
                size,
                glyph_id,
            });
        }
    }
}

/// Resolves the italic correction in 1000-units-per-em for any mathematical nucleus.
pub fn node_italic_correction(
    node: &MathNode,
    face_override: Option<Face>,
    family: FontFamily,
) -> f32 {
    node_italic_correction_styled(node, face_override, family, MathStyle::Display)
}

pub fn node_italic_correction_styled(
    node: &MathNode,
    face_override: Option<Face>,
    family: FontFamily,
    style: MathStyle,
) -> f32 {
    match node {
        MathNode::MathChoice(choice) => {
            let branch = match style.tier() {
                0 => &choice.display,
                1 => &choice.text,
                2 => &choice.script,
                _ => &choice.script_script,
            };
            node_italic_correction_styled(branch, face_override, family, style)
        }
        MathNode::Variable(ch) => {
            let default_face = if family == FontFamily::ComputerModern {
                Face::MathItalic
            } else {
                Face::Italic
            };
            let face = face_override.unwrap_or(default_face);
            if face == Face::Symbol && ch.is_ascii_uppercase() {
                if let Some(sg) = crate::math::fonts::script_glyph(*ch, family) {
                    return sg.ic as f32;
                }
            }
            if family != FontFamily::ComputerModern {
                if let Some(font) = crate::math::fonts::active_math_font(family) {
                    let u = if face == Face::Symbol && ch.is_ascii_uppercase() {
                        crate::math::fonts::to_unicode_script(*ch)
                            .map(|c| c as u32)
                            .unwrap_or(*ch as u32)
                    } else if face == Face::Bold {
                        crate::math::fonts::to_unicode_bold(*ch).map(|c| c as u32).unwrap_or(*ch as u32)
                    } else if face == Face::BoldItalic {
                        crate::math::fonts::to_unicode_bold_italic(*ch)
                            .or_else(|| crate::math::fonts::to_unicode_bold(*ch))
                            .map(|c| c as u32)
                            .unwrap_or(*ch as u32)
                    } else if face == Face::AmsB {
                        crate::math::fonts::to_unicode_blackboard(*ch).map(|c| c as u32).unwrap_or(*ch as u32)
                    } else if face == Face::AmsA {
                        crate::math::fonts::to_unicode_fraktur(*ch).map(|c| c as u32).unwrap_or(*ch as u32)
                    } else {
                        *ch as u32
                    };
                    if let Some(gid) = font.glyph_for_unicode(u) {
                        if let Some(ic) = font.glyph_italic_correction(gid.0) {
                            return ic as f32;
                        }
                    }
                }
            }
            italic_correction(*ch, face, family) as f32
        }
        MathNode::Symbol { code: Some(c), face, glyph, .. } => {
            if family == FontFamily::ComputerModern {
                let eff = resolve_cm_symbol(*c).map(|(f, _, code)| (f, code)).unwrap_or((*face, *c));
                let eff_face = face_override.unwrap_or(eff.0);
                match eff_face {
                    Face::MathItalic | Face::MathItalic7 | Face::MathItalic5 => {
                        crate::math::math_tfm::cmmi10_metric(eff.1).ic as f32
                    }
                    Face::Symbol | Face::Symbol7 | Face::Symbol5 => {
                        crate::math::math_tfm::cmsy10_metric(eff.1).ic as f32
                    }
                    Face::Extension => {
                        crate::math::math_tfm::cmex10_metric(eff.1).ic as f32
                    }
                    _ => 0.0,
                }
            } else {
                let sym_b = glyph
                    .chars()
                    .next()
                    .and_then(crate::font_metrics::unicode_to_symbol_byte)
                    .unwrap_or(*c);
                symbol_italic_correction(sym_b) as f32
            }
        }
        MathNode::Symbol { code: None, .. } => 0.0,
        MathNode::SubSup { .. } => 0.0,
        MathNode::StyleChange { face, content } => {
            node_italic_correction_styled(content, Some(*face), family, style)
        }
        MathNode::StyleScope { content, style: inner_style } => {
            node_italic_correction_styled(content, face_override, family, *inner_style)
        }
        MathNode::Row(nodes) => {
            for n in nodes.iter().rev() {
                if !matches!(n, MathNode::Space(_)) {
                    return node_italic_correction_styled(n, face_override, family, style);
                }
            }
            0.0
        }
        MathNode::BigOp(_) => 0.0,
        MathNode::Accent { nucleus, .. } => {
            node_italic_correction_styled(nucleus, face_override, family, style)
        }
        MathNode::Overbrace(_) | MathNode::Underbrace(_)
        | MathNode::Overbracket(_) | MathNode::Underbracket(_)
        | MathNode::ExtensibleArrow(_) => 0.0,
        MathNode::Phantom { content, width, .. } => {
            if *width {
                node_italic_correction_styled(content, face_override, family, style)
            } else {
                0.0
            }
        }
        MathNode::DimensionBox { content, zero_width, .. } => {
            if !*zero_width {
                node_italic_correction_styled(content, face_override, family, style)
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

/// Lays out or measures a mathematical delimiter (parenthesis, bracket, brace, vertical bar, angle, floor, ceil),
/// using TeX's two-tier model:
/// 1. Discrete variant selection for small to moderate heights (\big, \Big, \bigg, \Bigg).
/// 2. Multi-piece extensible assembly for large extents beyond discrete variants.
///
/// Returns the typographic advance width of the delimiter.
#[allow(clippy::too_many_arguments)]
pub fn layout_delimiter<'a, 'p>(
    ctx: &mut MathLayoutContext<'a, 'p>,
    delim: &'a str,
    is_left: bool,
    x: f32,
    y_center: f32,
    target_h: f32,
    font_size: f32,
    rule_t: f32,
) -> (f32, f32) {
    if NullDelimiterEliminator::is_null(delim) {
        return (0.0, 0.0);
    }

    if let Some((w, h)) = DelimiterBarRenderer::render_bar(
        ctx, delim, x, y_center, target_h, font_size, rule_t,
    ) {
        return (w, h);
    }

    let dmetrics = delimiter_metrics(ctx.font_family);

    if target_h <= 1.00 * font_size {
        match delim {
            "(" | ")" | "[" | "]" => {
                let ch = delim.chars().next().unwrap();
                let w = metrics::width_family(ch, Face::Roman, font_size, ctx.font_family);
                if !ctx.is_measuring() {
                    let glyph_y = y_center - (dmetrics.center_offset_y as f32 * 0.001) * font_size;
                    ctx.emit_glyph_run(x, glyph_y, Face::Roman, font_size, delim);
                }
                return (w, font_size);
            }
            _ => {}
        }
    }

    if let Some((chain, recipe_opt)) = resolve_cmex_delimiter(delim, is_left) {
        let scale = font_size * 0.1;
        let tier = DELIMITER_VARIANT_RESOLVER.resolve_tier_scaled(
            target_h,
            scale,
            recipe_opt.is_some(),
            chain.len(),
        );

        if tier < chain.len() {
            let code = chain[tier];
            let m = crate::math::math_tfm::cmex10_metric(code);
            let w = (m.w as f32 * 0.001) * font_size;
            let h = (m.h as f32 * 0.001) * font_size;
            let d = (m.d as f32 * 0.001) * font_size;
            if !ctx.is_measuring() {
                let glyph_y = y_center + (d - h) * 0.5;
                let s = byte_str(code);
                ctx.emit_glyph_run(x, glyph_y, Face::Extension, font_size, s);
            }
            return (w, h + d);
        }

        if let Some(recipe) = recipe_opt {
            let geom = ExtensibleAssemblyEngine::compute_assembly_geometry(
                recipe,
                target_h,
                font_size,
            );
            ExtensibleAssemblyEngine::emit_assembly(
                ctx,
                recipe,
                geom.n_rep as usize,
                x,
                y_center,
                geom.total_h,
                font_size,
            );
            return (geom.width, geom.total_h);
        }
    }

    let delim_t = (rule_t * (dmetrics.thickness_scale as f32 * 0.001)).max(0.60);
    let bar_extra = (dmetrics.bar_extra_h as f32 * 0.001) * font_size;

    match delim {
        "|" => {
            let bar_h = (target_h + bar_extra).max(target_h);
            let bar_y = y_center - bar_h * 0.5;
            let bar_w = 0.278 * font_size;
            let bar_x = x + (bar_w - delim_t).max(0.0) * 0.5;
            ctx.emit_rule(bar_x, bar_y, delim_t, bar_h);
            (bar_w, bar_h)
        }
        "\\|" | "||" => {
            let bar_h = (target_h + bar_extra).max(target_h);
            let bar_y = y_center - bar_h * 0.5;
            let sep_bars = 2.0 * delim_t;
            let bar_w = 0.398 * font_size;
            let bar_x = x + (bar_w - (2.0 * delim_t + sep_bars)).max(0.0) * 0.5;
            ctx.emit_rule(bar_x, bar_y, delim_t, bar_h);
            ctx.emit_rule(bar_x + delim_t + sep_bars, bar_y, delim_t, bar_h);
            (bar_w, bar_h)
        }
        "[" | "]" => {
            let cap_w = ((dmetrics.bracket_cap_w as f32 * 0.001) * font_size).max(2.8);
            let brk_h = (target_h + bar_extra).max(target_h);
            let brk_y = y_center - brk_h * 0.5;
            let y_top = brk_y + brk_h;
            if is_left {
                // Left bracket `[`: stem on left, caps pointing right
                ctx.emit_rule(x, brk_y, delim_t, brk_h);
                ctx.emit_rule(x, y_top - delim_t, cap_w, delim_t);
                ctx.emit_rule(x, brk_y, cap_w, delim_t);
            } else {
                // Right bracket `]`: stem on right, caps pointing left
                ctx.emit_rule(x + cap_w - delim_t, brk_y, delim_t, brk_h);
                ctx.emit_rule(x, y_top - delim_t, cap_w, delim_t);
                ctx.emit_rule(x, brk_y, cap_w, delim_t);
            }
            (cap_w, brk_h)
        }
        "\\lfloor" | "\\rfloor" => {
            let cap_w = ((dmetrics.bracket_cap_w as f32 * 0.001) * font_size).max(2.8);
            let brk_h = (target_h + bar_extra).max(target_h);
            let brk_y = y_center - brk_h * 0.5;
            let is_open = is_left || delim == "\\lfloor";
            if is_open {
                ctx.emit_rule(x, brk_y, delim_t, brk_h);
                ctx.emit_rule(x, brk_y, cap_w, delim_t);
            } else {
                ctx.emit_rule(x + cap_w - delim_t, brk_y, delim_t, brk_h);
                ctx.emit_rule(x, brk_y, cap_w, delim_t);
            }
            (cap_w, brk_h)
        }
        "\\lceil" | "\\rceil" => {
            let cap_w = ((dmetrics.bracket_cap_w as f32 * 0.001) * font_size).max(2.8);
            let brk_h = (target_h + bar_extra).max(target_h);
            let brk_y = y_center - brk_h * 0.5;
            let y_top = brk_y + brk_h;
            let is_open = is_left || delim == "\\lceil";
            if is_open {
                ctx.emit_rule(x, brk_y, delim_t, brk_h);
                ctx.emit_rule(x, y_top - delim_t, cap_w, delim_t);
            } else {
                ctx.emit_rule(x + cap_w - delim_t, brk_y, delim_t, brk_h);
                ctx.emit_rule(x, y_top - delim_t, cap_w, delim_t);
            }
            (cap_w, brk_h)
        }
        "\\langle" | "\\rangle" | "<" | ">" => {
            let is_open = is_left || delim == "\\langle" || delim == "<";
            let glyph = if is_open { "\u{00E1}" } else { "\u{00F1}" };
            let ch = if is_open { '\u{00E1}' } else { '\u{00F1}' };

            let scale_factor = if target_h <= 1.15 * font_size {
                1.00f32
            } else if target_h <= 1.42 * font_size {
                1.25
            } else if target_h <= 1.82 * font_size {
                1.60
            } else if target_h <= 2.30 * font_size {
                2.00
            } else if target_h <= 2.80 * font_size {
                2.50
            } else {
                (target_h / (1.10 * font_size)).min(3.8)
            };
            let delim_size = font_size * scale_factor;
            let w = metrics::width_family(ch, Face::Symbol, delim_size, ctx.font_family);
            let glyph_y = y_center - (dmetrics.center_offset_y as f32 * 0.001) * delim_size;
            ctx.emit_glyph_run(x, glyph_y, Face::Symbol, delim_size, glyph);
            (w, delim_size)
        }
        "(" | ")" => {
            let is_open = is_left || delim == "(";
            let h_limit_variants = 3.80 * font_size;

            if target_h <= h_limit_variants {
                // Tier 1: Discrete variants selection
                let scale_factor = if target_h <= 1.15 * font_size {
                    1.00f32
                } else if target_h <= 1.42 * font_size {
                    1.25
                } else if target_h <= 1.82 * font_size {
                    1.60
                } else if target_h <= 2.30 * font_size {
                    2.00
                } else if target_h <= 2.80 * font_size {
                    2.50
                } else {
                    (target_h / (1.10 * font_size)).min(3.5)
                };
                let delim_size = font_size * scale_factor;
                let glyph = if is_open { "(" } else { ")" };
                let ch = if is_open { '(' } else { ')' };
                let w = metrics::width_family(ch, Face::Symbol, delim_size, ctx.font_family);
                let glyph_y = y_center - (dmetrics.center_offset_y as f32 * 0.001) * delim_size;
                ctx.emit_glyph_run(x, glyph_y, Face::Symbol, delim_size, glyph);
                (w, delim_size)
            } else {
                // Tier 2: Multi-piece extensible assembly
                let piece_size = font_size * 1.25;
                let step_target = 0.70 * piece_size;
                let needed = target_h - piece_size;
                let n_extenders = ((needed / step_target).round() as usize).max(1);
                let delta_y = needed / ((n_extenders + 1) as f32);

                let top_glyph = if is_open { "\u{00E6}" } else { "\u{00F6}" };
                let ext_glyph = if is_open { "\u{00E7}" } else { "\u{00F7}" };
                let bot_glyph = if is_open { "\u{00E8}" } else { "\u{00F8}" };

                let top_y = y_center + ((n_extenders + 1) as f32) * 0.5 * delta_y;
                let bot_y = y_center - ((n_extenders + 1) as f32) * 0.5 * delta_y;

                ctx.emit_glyph_run(x, top_y, Face::Symbol, piece_size, top_glyph);
                for i in 0..n_extenders {
                    let curr_y =
                        y_center + (((n_extenders - 1) as f32) * 0.5 - (i as f32)) * delta_y;
                    ctx.emit_glyph_run(x, curr_y, Face::Symbol, piece_size, ext_glyph);
                }
                ctx.emit_glyph_run(x, bot_y, Face::Symbol, piece_size, bot_glyph);

                (
                    metrics::width_family('\u{00E6}', Face::Symbol, piece_size, ctx.font_family),
                    target_h,
                )
            }
        }
        "{" | "}" => {
            let is_open = is_left || delim == "{";
            let h_limit_variants = 2.80 * font_size;

            if target_h <= h_limit_variants {
                // Tier 1: Discrete variants selection
                let scale_factor = if target_h <= 1.15 * font_size {
                    1.00f32
                } else if target_h <= 1.42 * font_size {
                    1.25
                } else if target_h <= 1.82 * font_size {
                    1.60
                } else if target_h <= 2.30 * font_size {
                    2.00
                } else {
                    2.50
                };
                let delim_size = font_size * scale_factor;
                let glyph = if is_open { "{" } else { "}" };
                let ch = if is_open { '{' } else { '}' };
                let w = metrics::width_family(ch, Face::Symbol, delim_size, ctx.font_family);
                let glyph_y = y_center - (dmetrics.center_offset_y as f32 * 0.001) * delim_size;
                ctx.emit_glyph_run(x, glyph_y, Face::Symbol, delim_size, glyph);
                (w, delim_size)
            } else {
                // Tier 2: Multi-piece extensible assembly
                let piece_size = font_size * 1.25;
                let half_h = target_h * 0.5;
                let needed_half = half_h - 0.5 * piece_size;
                let step_target = 0.70 * piece_size;
                let k_extenders = ((needed_half / step_target).round() as usize).max(1);
                let delta_y = needed_half / ((k_extenders + 1) as f32);

                let top_glyph = if is_open { "\u{00EC}" } else { "\u{00FC}" };
                let mid_glyph = if is_open { "\u{00ED}" } else { "\u{00FD}" };
                let bot_glyph = if is_open { "\u{00EE}" } else { "\u{00FE}" };
                let ext_glyph = "\u{00EF}";

                // Top hook
                let top_y = y_center + ((k_extenders + 1) as f32) * delta_y;
                ctx.emit_glyph_run(x, top_y, Face::Symbol, piece_size, top_glyph);

                // Upper extenders
                for i in 0..k_extenders {
                    let curr_y = y_center + ((k_extenders - i) as f32) * delta_y;
                    ctx.emit_glyph_run(x, curr_y, Face::Symbol, piece_size, ext_glyph);
                }

                // Mid tip
                ctx.emit_glyph_run(x, y_center, Face::Symbol, piece_size, mid_glyph);

                // Lower extenders
                for i in 1..=k_extenders {
                    let curr_y = y_center - (i as f32) * delta_y;
                    ctx.emit_glyph_run(x, curr_y, Face::Symbol, piece_size, ext_glyph);
                }

                // Bottom hook
                let bot_y = y_center - ((k_extenders + 1) as f32) * delta_y;
                ctx.emit_glyph_run(x, bot_y, Face::Symbol, piece_size, bot_glyph);

                (
                    metrics::width_family('\u{00EC}', Face::Symbol, piece_size, ctx.font_family),
                    target_h,
                )
            }
        }
        _ => {
            let delim_scale = (target_h / (1.05 * font_size)).clamp(1.0, 2.5);
            let delim_size = font_size * delim_scale;
            let ch = delim.chars().next().unwrap_or('(');
            let w = metrics::width_family(ch, Face::Roman, delim_size, ctx.font_family);
            let glyph_y = y_center - 0.28 * delim_size;
            ctx.emit_glyph_run(x, glyph_y, Face::Roman, delim_size, delim);
            (w, delim_size)
        }
    }
}

/// Computes the optical top accent attachment skew offset for any mathematical nucleus.
/// Adheres to OpenType MATH `TopAccentAttachment` rules for styled alphanumerics and Greek symbols,
/// falling back to classic TeX CMMI skewchar kerning for Computer Modern unstyled variables.
fn compute_top_accent_skew_offset<'a, 'p>(
    nucleus: &MathNode<'a>,
    ctx: &MathLayoutContext<'a, 'p>,
    font_size: f32,
    scale: f32,
    ic: f32,
) -> f32 {
    let (target_node, effective_face) = match nucleus {
        MathNode::StyleChange { face, content } => (&**content, *face),
        _ => {
            let default_face = if ctx.font_family == metrics::FontFamily::ComputerModern {
                Face::MathItalic
            } else {
                Face::Italic
            };
            (nucleus, ctx.face_override.unwrap_or(default_face))
        }
    };

    let unicode_target = match target_node {
        MathNode::Variable(ch) => match effective_face {
            Face::Bold => crate::math::fonts::to_unicode_bold(*ch).map(|c| c as u32),
            Face::BoldItalic => crate::math::fonts::to_unicode_bold_italic(*ch)
                .or_else(|| crate::math::fonts::to_unicode_bold(*ch))
                .map(|c| c as u32),
            Face::Symbol if ch.is_ascii_uppercase() => {
                crate::math::fonts::to_unicode_script(*ch).map(|c| c as u32)
            }
            Face::AmsB => crate::math::fonts::to_unicode_blackboard(*ch).map(|c| c as u32),
            Face::AmsA => crate::math::fonts::to_unicode_fraktur(*ch).map(|c| c as u32),
            Face::Mono => crate::math::fonts::to_unicode_monospace(*ch).map(|c| c as u32),
            Face::Italic => crate::math::fonts::to_unicode_math_italic(*ch).map(|c| c as u32),
            _ => Some(*ch as u32),
        },
        MathNode::Symbol { glyph, code, face: sym_face, .. } => {
            let face = if matches!(nucleus, MathNode::StyleChange { .. }) {
                effective_face
            } else {
                ctx.face_override.unwrap_or(*sym_face)
            };
            let greek = code.and_then(crate::math::fonts::symbol_to_unicode_greek)
                .or_else(|| glyph.chars().next().filter(|ch| ('\u{0391}'..='\u{03C9}').contains(ch)));
            if let Some(g) = greek {
                match face {
                    Face::Bold => crate::math::fonts::to_unicode_bold(g).map(|c| c as u32),
                    Face::BoldItalic => crate::math::fonts::to_unicode_bold_italic(g)
                        .or_else(|| crate::math::fonts::to_unicode_bold(g))
                        .map(|c| c as u32),
                    _ => Some(g as u32),
                }
            } else if face == Face::AmsB {
                glyph.chars().next().and_then(crate::math::fonts::to_unicode_blackboard).map(|c| c as u32)
            } else if face == Face::AmsA {
                glyph.chars().next().and_then(crate::math::fonts::to_unicode_fraktur).map(|c| c as u32)
            } else {
                glyph.chars().next().map(|c| c as u32)
            }
        }
        _ => None,
    };

    // For classic unstyled Computer Modern MathItalic: adhere to classic TeX cmmi_skew_kern
    if ctx.font_family == metrics::FontFamily::ComputerModern && !matches!(nucleus, MathNode::StyleChange { .. }) {
        if let MathNode::Variable(ch) = target_node {
            if effective_face == Face::MathItalic {
                return crate::math::math_tfm::cmmi_skew_kern(*ch) * font_size;
            }
        }
        if let MathNode::Symbol { code: Some(code), .. } = target_node {
            if let Some((Face::MathItalic, _, glyph_code)) = crate::math::math_fonts::resolve_cm_symbol(*code) {
                return crate::math::math_tfm::CMMI10_SKEW_KERNS[glyph_code as usize] as f32 * 0.001 * font_size;
            }
        }
    }

    if let MathNode::Variable(ch) = target_node {
        if effective_face == Face::Symbol && ch.is_ascii_uppercase() {
            if let Some(sg) = crate::math::fonts::script_glyph(*ch, ctx.font_family) {
                return (sg.top_accent as f32) * scale;
            }
        }
        return (glyph_top_accent_offset(*ch, effective_face, ctx.font_family) as f32) * scale;
    }

    if let Some(u) = unicode_target {
        if let Some(font) = crate::math::fonts::active_math_font(ctx.font_family) {
            if let Some(gid) = font.glyph_for_unicode(u) {
                if let Some(attach) = font.glyph_top_accent_attachment(gid.0) {
                    let adv = font.h_advance(gid.0).unwrap_or(1000);
                    return (attach as f32 - 0.5 * adv as f32) * scale;
                }
            }
        }
    }

    0.5 * ic
}

#[inline]
fn content_has_middle(node: &MathNode) -> bool {
    match node {
        MathNode::MiddleDelim { .. } => true,
        MathNode::Row(nodes) => nodes.iter().any(content_has_middle),
        MathNode::StyleScope { content, .. }
        | MathNode::Vcenter(content)
        | MathNode::Tagged { formula: content, .. }
        | MathNode::Phantom { content, .. }
        | MathNode::DimensionBox { content, .. } => content_has_middle(content),
        MathNode::SubSup { nucleus, .. } => content_has_middle(nucleus),
        _ => false,
    }
}

/// Lays out a `MathNode` at a given `(origin_x, origin_y)` with `style` and returns its bounding box.
pub fn layout_node<'a, 'p>(
    ctx: &mut MathLayoutContext<'a, 'p>,
    node: &MathNode<'a>,
    style: MathStyle,
    origin_x: f32,
    origin_y: f32,
) -> MathBox {
    let params = ctx.font_params(style);
    let font_size = ctx.font_tier.font_size(style);
    let scale = font_size * 0.001;
    let axis_h = (params.axis_height as f32) * scale;
    let rule_t = ((params.rule_thickness as f32) * scale).max(0.35);

    ctx.style_stack.push(style);
    let result = match node {
        MathNode::Variable(ch) => {
            let default_face = if ctx.font_family == metrics::FontFamily::ComputerModern {
                Face::MathItalic
            } else {
                Face::Italic
            };
            let base_face = ctx.face_override.unwrap_or(default_face);
            let face = resolve_optical_face(base_face, style, ctx.font_family);

            let (w, h, d, dynamic_gid) = if base_face == Face::Symbol && ch.is_ascii_uppercase() {
                if let Some(sg) = crate::math::fonts::script_glyph(*ch, ctx.font_family) {
                    (
                        (sg.w as f32) * scale,
                        (sg.h as f32) * scale,
                        (sg.d as f32) * scale,
                        Some(sg.glyph_id),
                    )
                } else if let Some(font) = crate::math::fonts::active_math_font(ctx.font_family) {
                    let u = crate::math::fonts::to_unicode_script(*ch)
                        .map(|c| c as u32)
                        .unwrap_or(*ch as u32);
                    if let Some(gid) = font.glyph_for_unicode(u) {
                        let adv = font.h_advance(gid.0).unwrap_or(650) as f32 * scale;
                        let h = 0.69 * font_size;
                        let d = if matches!(*ch, 'G' | 'J' | 'Y') {
                            0.13 * font_size
                        } else {
                            0.0
                        };
                        (adv, h, d, Some(gid.0))
                    } else {
                        let w = measure_char(*ch, face, font_size, ctx.font_family);
                        let (h_u, d_u) = char_height_depth(*ch, face, ctx.font_family);
                        (w, (h_u as f32) * scale, (d_u as f32) * scale, None)
                    }
                } else {
                    let w = measure_char(*ch, face, font_size, ctx.font_family);
                    let (h_u, d_u) = char_height_depth(*ch, face, ctx.font_family);
                    (w, (h_u as f32) * scale, (d_u as f32) * scale, None)
                }
            } else if base_face == Face::AmsB {
                let u_char = crate::math::fonts::to_unicode_blackboard(*ch);
                let gid = u_char.and_then(|u| crate::math::fonts::vector_glyph_for_unicode(u as u32, ctx.font_family));
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_height_depth(*ch, face, ctx.font_family);
                (w, (h_u as f32) * scale, (d_u as f32) * scale, gid)
            } else if base_face == Face::AmsA {
                let u_char = crate::math::fonts::to_unicode_fraktur(*ch);
                let gid = u_char.and_then(|u| crate::math::fonts::vector_glyph_for_unicode(u as u32, ctx.font_family));
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_height_depth(*ch, face, ctx.font_family);
                (w, (h_u as f32) * scale, (d_u as f32) * scale, gid)
            } else if base_face == Face::Mono {
                let u_char = crate::math::fonts::to_unicode_monospace(*ch);
                let gid = u_char.and_then(|u| {
                    crate::math::fonts::vector_glyph_for_unicode(u as u32, ctx.font_family)
                        .or_else(|| crate::math::fonts::active_math_font(ctx.font_family).and_then(|f| f.glyph_for_unicode(u as u32).map(|g| g.0)))
                });
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_height_depth(*ch, face, ctx.font_family);
                (w, (h_u as f32) * scale, (d_u as f32) * scale, gid)
            } else if base_face == Face::Italic {
                let u_char = crate::math::fonts::to_unicode_math_italic(*ch);
                let gid = u_char.and_then(|u| {
                    crate::math::fonts::vector_glyph_for_unicode(u as u32, ctx.font_family)
                        .or_else(|| crate::math::fonts::active_math_font(ctx.font_family).and_then(|f| f.glyph_for_unicode(u as u32).map(|g| g.0)))
                });
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_height_depth(*ch, face, ctx.font_family);
                (w, (h_u as f32) * scale, (d_u as f32) * scale, gid)
            } else if base_face == Face::Bold || base_face == Face::BoldItalic {
                let bold_u = if base_face == Face::BoldItalic {
                    crate::math::fonts::to_unicode_bold_italic(*ch)
                        .or_else(|| crate::math::fonts::to_unicode_bold(*ch))
                } else {
                    crate::math::fonts::to_unicode_bold(*ch)
                };
                let gid = bold_u.and_then(|u| crate::math::fonts::vector_glyph_for_unicode(u as u32, ctx.font_family));
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_height_depth(*ch, face, ctx.font_family);
                (w, (h_u as f32) * scale, (d_u as f32) * scale, gid)
            } else {
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_height_depth(*ch, face, ctx.font_family);
                (w, (h_u as f32) * scale, (d_u as f32) * scale, None)
            };

            let s = match *ch {
                'a' => "a",
                'b' => "b",
                'c' => "c",
                'd' => "d",
                'e' => "e",
                'f' => "f",
                'g' => "g",
                'h' => "h",
                'i' => "i",
                'j' => "j",
                'k' => "k",
                'l' => "l",
                'm' => "m",
                'n' => "n",
                'o' => "o",
                'p' => "p",
                'q' => "q",
                'r' => "r",
                's' => "s",
                't' => "t",
                'u' => "u",
                'v' => "v",
                'w' => "w",
                'x' => "x",
                'y' => "y",
                'z' => "z",
                'A' => "A",
                'B' => "B",
                'C' => "C",
                'D' => "D",
                'E' => "E",
                'F' => "F",
                'G' => "G",
                'H' => "H",
                'I' => "I",
                'J' => "J",
                'K' => "K",
                'L' => "L",
                'M' => "M",
                'N' => "N",
                'O' => "O",
                'P' => "P",
                'Q' => "Q",
                'R' => "R",
                'S' => "S",
                'T' => "T",
                'U' => "U",
                'V' => "V",
                'W' => "W",
                'X' => "X",
                'Y' => "Y",
                'Z' => "Z",
                _ => "x",
            };

            if let Some(gid) = dynamic_gid {
                if crate::math::fonts::get_vector_glyph_path(gid).is_some() {
                    ctx.emit_vector_glyph(origin_x, origin_y, font_size, gid);
                    return MathBox::with_dims(w, h, d);
                }
            }

            ctx.emit_glyph_run(origin_x, origin_y, face, font_size, s);
            MathBox::with_dims(w, h, d)
        }

        MathNode::Number(num_str) => {
            let base_face = ctx.face_override.unwrap_or(Face::Roman);
            let face = resolve_optical_face(base_face, style, ctx.font_family);
            let mut w = 0.0f32;
            for b in num_str.bytes() {
                w += measure_char(b as char, face, font_size, ctx.font_family);
            }
            let (h_u, d_u) = char_height_depth('0', face, ctx.font_family);
            let h = (h_u as f32) * scale;
            let d = (d_u as f32) * scale;

            ctx.emit_glyph_run(origin_x, origin_y, face, font_size, num_str);
            MathBox::with_dims(w, h, d)
        }

        MathNode::Symbol {
            face, glyph, code, ..
        } => {
            let (raw_face, eff_glyph, eff_code) = if ctx.font_family == metrics::FontFamily::ComputerModern {
                if *glyph == "<" {
                    (Face::MathItalic, "<", Some(0x3C))
                } else if *glyph == ">" {
                    (Face::MathItalic, ">", Some(0x3E))
                } else if let Some(c) = code {
                    if let Some((cm_face, cm_glyph, cm_c)) = resolve_cm_symbol(*c) {
                        (cm_face, cm_glyph, Some(cm_c))
                    } else if (*c as usize) < ASCII_GLYPH_STRS.len() {
                        (*face, ASCII_GLYPH_STRS[*c as usize], Some(*c))
                    } else {
                        (*face, *glyph, *code)
                    }
                } else {
                    (*face, *glyph, *code)
                }
            } else if *glyph == "†" || *glyph == "‡" || *glyph == "|" || *glyph == "||" {
                (Face::Roman, *glyph, *code)
            } else {
                let resolved_code = if matches!(*face, Face::Symbol | Face::Symbol7 | Face::Symbol5) {
                    glyph
                        .chars()
                        .next()
                        .and_then(crate::font_metrics::unicode_to_symbol_byte)
                        .or(*code)
                } else {
                    *code
                };
                (*face, *glyph, resolved_code)
            };
            let override_face = ctx.face_override;
            let eff_face = if let Some(ov) = override_face {
                if ctx.font_family == metrics::FontFamily::ComputerModern {
                    if ov == Face::Bold || ov == Face::BoldItalic {
                        Face::Bold
                    } else {
                        resolve_optical_face(ov, style, ctx.font_family)
                    }
                } else {
                    resolve_optical_face(ov, style, ctx.font_family)
                }
            } else {
                resolve_optical_face(raw_face, style, ctx.font_family)
            };

            let mut dynamic_gid: Option<u16> = None;
            if eff_face == Face::Bold || eff_face == Face::BoldItalic {
                let greek = eff_code.and_then(crate::math::fonts::symbol_to_unicode_greek)
                    .or_else(|| eff_glyph.chars().next().filter(|ch| ('\u{0391}'..='\u{03C9}').contains(ch)));
                let bold_u = greek.and_then(|g| {
                    if eff_face == Face::BoldItalic {
                        crate::math::fonts::to_unicode_bold_italic(g)
                            .or_else(|| crate::math::fonts::to_unicode_bold(g))
                    } else {
                        crate::math::fonts::to_unicode_bold(g)
                    }
                });
                if let Some(u_char) = bold_u {
                    if let Some(gid) = crate::math::fonts::vector_glyph_for_unicode(u_char as u32, ctx.font_family) {
                        dynamic_gid = Some(gid);
                    } else if let Some(font) = crate::math::fonts::active_math_font(ctx.font_family) {
                        if let Some(gid) = font.glyph_for_unicode(u_char as u32) {
                            dynamic_gid = Some(gid.0);
                        }
                    }
                }
            } else if eff_face == Face::AmsB {
                if let Some(ch) = eff_glyph.chars().next() {
                    if let Some(u_char) = crate::math::fonts::to_unicode_blackboard(ch) {
                        if let Some(gid) = crate::math::fonts::vector_glyph_for_unicode(u_char as u32, ctx.font_family) {
                            dynamic_gid = Some(gid);
                        } else if let Some(font) = crate::math::fonts::active_math_font(ctx.font_family) {
                            if let Some(gid) = font.glyph_for_unicode(u_char as u32) {
                                dynamic_gid = Some(gid.0);
                            }
                        }
                    }
                }
            } else if eff_face == Face::AmsA {
                if let Some(ch) = eff_glyph.chars().next() {
                    if let Some(u_char) = crate::math::fonts::to_unicode_fraktur(ch) {
                        if let Some(gid) = crate::math::fonts::vector_glyph_for_unicode(u_char as u32, ctx.font_family) {
                            dynamic_gid = Some(gid);
                        } else if let Some(font) = crate::math::fonts::active_math_font(ctx.font_family) {
                            if let Some(gid) = font.glyph_for_unicode(u_char as u32) {
                                dynamic_gid = Some(gid.0);
                            }
                        }
                    }
                }
            }

            let (w, h, d) = if let Some(gid) = dynamic_gid {
                let adv = if let Some(font) = crate::math::fonts::active_math_font(ctx.font_family) {
                    font.h_advance(gid).unwrap_or(600) as f32 * scale
                } else {
                    600.0 * scale
                };
                let (h_u, d_u) = if let Some(c) = eff_code {
                    symbol_height_depth_family(c, ctx.font_family)
                } else {
                    let first = eff_glyph.chars().next().unwrap_or('x');
                    char_height_depth(first, eff_face, ctx.font_family)
                };
                (adv, (h_u as f32) * scale, (d_u as f32) * scale)
            } else if let Some(c) = eff_code {
                let w = if ctx.font_family == metrics::FontFamily::ComputerModern {
                    match eff_face {
                        Face::MathItalic => crate::math::math_tfm::cmmi10_metric(c).w as f32 * scale,
                        Face::MathItalic7 => crate::math::math_tfm::cmmi7_metric(c).w as f32 * scale,
                        Face::MathItalic5 => crate::math::math_tfm::cmmi5_metric(c).w as f32 * scale,
                        Face::Italic => crate::math::math_tfm::cmti10_metric(c).w as f32 * scale,
                        Face::Bold => crate::math::math_tfm::cmbx10_metric(c).w as f32 * scale,
                        Face::Roman => crate::math::math_tfm::cmr10_metric(c).w as f32 * scale,
                        Face::Roman7 => crate::math::math_tfm::cmr7_metric(c).w as f32 * scale,
                        Face::Roman5 => crate::math::math_tfm::cmr5_metric(c).w as f32 * scale,
                        Face::AmsA => crate::math::math_tfm::msam10_metric(c).w as f32 * scale,
                        Face::AmsB => crate::math::math_tfm::msbm10_metric(c).w as f32 * scale,
                        Face::Symbol | Face::Symbol7 | Face::Symbol5 => {
                            let mw = match eff_face {
                                Face::Symbol7 => crate::math::math_tfm::cmsy7_metric(c).w as f32 * scale,
                                Face::Symbol5 => crate::math::math_tfm::cmsy5_metric(c).w as f32 * scale,
                                _ => crate::math::math_tfm::cmsy10_metric(c).w as f32 * scale,
                            };
                            if mw == 0.0 && c >= 128 {
                                metrics::SYMBOL_WIDTHS[c as usize] as f32 * scale
                            } else {
                                mw
                            }
                        }
                        _ => {
                            let mw = crate::math::math_tfm::cmsy10_metric(c).w as f32 * scale;
                            if mw == 0.0 && c >= 128 {
                                metrics::SYMBOL_WIDTHS[c as usize] as f32 * scale
                            } else {
                                mw
                            }
                        }
                    }
                } else if (eff_face == Face::Bold || eff_face == Face::BoldItalic) && ctx.font_family != metrics::FontFamily::ComputerModern {
                    let greek = eff_code.and_then(crate::math::fonts::symbol_to_unicode_greek)
                        .or_else(|| eff_glyph.chars().next().filter(|ch| ('\u{0391}'..='\u{03C9}').contains(ch)));
                    let bold_u = greek.and_then(|g| {
                        if eff_face == Face::BoldItalic {
                            crate::math::fonts::to_unicode_bold_italic(g)
                                .or_else(|| crate::math::fonts::to_unicode_bold(g))
                        } else {
                            crate::math::fonts::to_unicode_bold(g)
                        }
                    });
                    if let Some(u_char) = bold_u {
                        if let Some(font) = crate::math::fonts::active_math_font(ctx.font_family) {
                            if let Some(gid) = font.glyph_for_unicode(u_char as u32) {
                                font.h_advance(gid.0).unwrap_or(600) as f32 * scale
                            } else {
                                metrics::SYMBOL_WIDTHS[c as usize] as f32 * scale
                            }
                        } else {
                            metrics::SYMBOL_WIDTHS[c as usize] as f32 * scale
                        }
                    } else {
                        metrics::SYMBOL_WIDTHS[c as usize] as f32 * scale
                    }
                } else if matches!(eff_face, Face::Symbol | Face::Symbol7 | Face::Symbol5) {
                    metrics::SYMBOL_WIDTHS[c as usize] as f32 * scale
                } else {
                    let mut total_w = 0.0f32;
                    for ch in eff_glyph.chars() {
                        total_w += measure_char(ch, eff_face, font_size, ctx.font_family);
                    }
                    total_w
                };
                let (h_u, d_u) = if ctx.font_family == metrics::FontFamily::ComputerModern {
                    match eff_face {
                        Face::MathItalic => {
                            let m = crate::math::math_tfm::cmmi10_metric(c);
                            (m.h, m.d)
                        }
                        Face::MathItalic7 => {
                            let m = crate::math::math_tfm::cmmi7_metric(c);
                            (m.h, m.d)
                        }
                        Face::MathItalic5 => {
                            let m = crate::math::math_tfm::cmmi5_metric(c);
                            (m.h, m.d)
                        }
                        Face::Italic => {
                            let m = crate::math::math_tfm::cmti10_metric(c);
                            (m.h, m.d)
                        }
                        Face::Bold => {
                            let m = crate::math::math_tfm::cmbx10_metric(c);
                            (m.h, m.d)
                        }
                        Face::Roman => {
                            let m = crate::math::math_tfm::cmr10_metric(c);
                            (m.h, m.d)
                        }
                        Face::Roman7 => {
                            let m = crate::math::math_tfm::cmr7_metric(c);
                            (m.h, m.d)
                        }
                        Face::Roman5 => {
                            let m = crate::math::math_tfm::cmr5_metric(c);
                            (m.h, m.d)
                        }
                        Face::AmsA => {
                            let m = crate::math::math_tfm::msam10_metric(c);
                            (m.h, m.d)
                        }
                        Face::AmsB => {
                            let m = crate::math::math_tfm::msbm10_metric(c);
                            (m.h, m.d)
                        }
                        Face::Symbol | Face::Symbol7 | Face::Symbol5 => {
                            let m = match eff_face {
                                Face::Symbol7 => crate::math::math_tfm::cmsy7_metric(c),
                                Face::Symbol5 => crate::math::math_tfm::cmsy5_metric(c),
                                _ => crate::math::math_tfm::cmsy10_metric(c),
                            };
                            if m.h == 0 && m.d == 0 && c >= 128 {
                                symbol_height_depth_family(c, ctx.font_family)
                            } else {
                                (m.h, m.d)
                            }
                        }
                        _ => {
                            let m = crate::math::math_tfm::cmsy10_metric(c);
                            if m.h == 0 && m.d == 0 && c >= 128 {
                                symbol_height_depth_family(c, ctx.font_family)
                            } else {
                                (m.h, m.d)
                            }
                        }
                    }
                } else if matches!(eff_face, Face::Symbol | Face::Symbol7 | Face::Symbol5) {
                    symbol_height_depth_family(c, ctx.font_family)
                } else {
                    let first = eff_glyph.chars().next().unwrap_or('x');
                    char_height_depth(first, eff_face, ctx.font_family)
                };
                (w, (h_u as f32) * scale, (d_u as f32) * scale)
            } else {
                let mut total_w = 0.0f32;
                let mut max_h_u = 0u16;
                let mut max_d_u = 0u16;
                for ch in eff_glyph.chars() {
                    total_w += measure_char(ch, eff_face, font_size, ctx.font_family);
                    let (ch_h, ch_d) = char_height_depth(ch, eff_face, ctx.font_family);
                    max_h_u = max_h_u.max(ch_h);
                    max_d_u = max_d_u.max(ch_d);
                }
                if let Some(last_ch) = eff_glyph.chars().last() {
                    total_w += (italic_correction(last_ch, eff_face, ctx.font_family) as f32) * scale;
                }
                (total_w, (max_h_u as f32) * scale, (max_d_u as f32) * scale)
            };

            if let Some(gid) = dynamic_gid {
                if crate::math::fonts::get_vector_glyph_path(gid).is_some() {
                    ctx.emit_vector_glyph(origin_x, origin_y, font_size, gid);
                    return MathBox::with_dims(w, h, d);
                }
            }

            let emit_x = if eff_glyph == "|" && ctx.font_family != metrics::FontFamily::ComputerModern {
                origin_x + 0.039 * font_size
            } else {
                origin_x
            };
            ctx.emit_glyph_run(emit_x, origin_y, eff_face, font_size, eff_glyph);
            MathBox::with_dims(w, h, d)
        }

        MathNode::SubSup { nucleus, sub, sup } => {
            if let MathNode::Overbrace(inner) = &**nucleus {
                return layout_overbrace(
                    ctx,
                    inner,
                    sub.as_deref(),
                    sup.as_deref(),
                    style,
                    origin_x,
                    origin_y,
                );
            }
            if let MathNode::Underbrace(inner) = &**nucleus {
                return layout_underbrace(
                    ctx,
                    inner,
                    sub.as_deref(),
                    sup.as_deref(),
                    style,
                    origin_x,
                    origin_y,
                );
            }
            if let MathNode::Overbracket(inner) = &**nucleus {
                return layout_overbracket(
                    ctx,
                    inner,
                    sub.as_deref(),
                    sup.as_deref(),
                    style,
                    origin_x,
                    origin_y,
                );
            }
            if let MathNode::Underbracket(inner) = &**nucleus {
                return layout_underbracket(
                    ctx,
                    inner,
                    sub.as_deref(),
                    sup.as_deref(),
                    style,
                    origin_x,
                    origin_y,
                );
            }

            let nuc_box = layout_node(ctx, nucleus, style, origin_x, origin_y);

            // TeX Rule 12 & mlist.c lines 2796-2812:
            // When nucleus is an Accent, scripts attach to the base nucleus character
            let (effective_nuc, effective_h, effective_d) = match &**nucleus {
                MathNode::Accent { nucleus: inner, .. } => {
                    let inner_b = measure_math(inner, font_size, ctx.font_family, style);
                    (&**inner, inner_b.h, inner_b.d)
                }
                _ => (&**nucleus, nuc_box.h, nuc_box.d),
            };

            let sup_start = ctx.out_ops_len();
            let sup_box = if let Some(sp) = sup {
                let sup_style = style.sup_style();
                let mut mb = layout_node(ctx, sp, sup_style, 0.0, 0.0);
                let ic = node_italic_correction_styled(sp, ctx.face_override, ctx.font_family, sup_style)
                    * (ctx.font_tier.font_size(sup_style) * 0.001);
                mb.w += ic;
                mb
            } else {
                MathBox::zero()
            };
            let sup_end = ctx.out_ops_len();

            let sub_start = ctx.out_ops_len();
            let sub_box = if let Some(sb) = sub {
                let sub_style = style.sub_style();
                let mut mb = layout_node(ctx, sb, sub_style, 0.0, 0.0);
                let ic = node_italic_correction_styled(sb, ctx.face_override, ctx.font_family, sub_style)
                    * (ctx.font_tier.font_size(sub_style) * 0.001);
                mb.w += ic;
                mb
            } else {
                MathBox::zero()
            };
            let sub_end = ctx.out_ops_len();

            // TeX Appendix G Rule 17: Italic correction is applied to the horizontal
            // placement of the superscript, but not the subscript.
            let ic = node_italic_correction(effective_nuc, ctx.face_override, ctx.font_family) * scale;
            let math_kern = if let MathNode::Variable(ch) = effective_nuc {
                if ctx.font_family != FontFamily::ComputerModern && ctx.font_family != FontFamily::Times {
                    if let Some(font) = crate::math::fonts::active_math_font(ctx.font_family) {
                        let u = *ch as u32;
                        let corr_h = (effective_h / scale.max(0.0001)) as i16;
                        font.glyph_for_unicode(u)
                            .and_then(|gid| {
                                font.glyph_top_right_math_kern_at(gid.0, corr_h)
                                    .or_else(|| font.glyph_top_right_math_kern(gid.0))
                            })
                            .unwrap_or(0) as f32 * scale
                    } else {
                        0.0
                    }
                } else {
                    0.0
                }
            } else {
                0.0
            };
            let sup_x = origin_x + nuc_box.w + ic + math_kern;
            let sub_x = origin_x + nuc_box.w;

            let x_height = (params.accent_base_height as f32) * scale;

            let is_char = match effective_nuc {
                MathNode::Variable(_) | MathNode::Symbol { .. } | MathNode::Number(_) => true,
                MathNode::Row(children) if children.len() == 1 => {
                    matches!(&children[0], MathNode::Variable(_) | MathNode::Symbol { .. } | MathNode::Number(_))
                }
                MathNode::StyleScope { content, .. } | MathNode::StyleChange { content, .. } => {
                    matches!(&**content, MathNode::Variable(_) | MathNode::Symbol { .. } | MathNode::Number(_))
                }
                _ => false,
            };

            let script_metrics = ScriptShiftResolver::resolve(
                sup.is_some(),
                sub.is_some(),
                style,
                is_char,
                effective_h,
                effective_d,
                sup_box.d,
                sub_box.h,
                x_height,
                scale,
                params,
            );
            let sup_shift = script_metrics.sup_shift;
            let sub_shift = script_metrics.sub_shift;

            if sup.is_some() {
                ctx.shift_ops_range(sup_start, sup_end, sup_x, origin_y + sup_shift);
            }
            if sub.is_some() {
                ctx.shift_ops_range(sub_start, sub_end, sub_x, origin_y - sub_shift);
            }

            let script_space = if sup.is_some() || sub.is_some() {
                0.5 * (ctx.base_font_size / 10.0)
            } else {
                0.0
            };
            let scripts = TransientScriptPair::new(sup_box, sub_box);
            let right_bound = scripts.right_bound(
                sup.is_some(),
                sub.is_some(),
                origin_x,
                nuc_box.w,
                sup_x,
                sub_x,
                script_space,
            );
            let total_w = right_bound - origin_x;
            let total_h = scripts.total_height(sup.is_some(), nuc_box.h, sup_shift);
            let total_d = scripts.total_depth(sub.is_some(), nuc_box.d, sub_shift);

            MathBox::with_dims(total_w, total_h, total_d)
        }

        MathNode::Fraction {
            num,
            den,
            has_rule,
            options,
        } => {
            let is_skewed = *options & crate::math::math_ast::FRACTION_OPT_SKEWED != 0;
            let (num_style, den_style) = if is_skewed {
                (style.sup_style(), style.sub_style().cramped())
            } else {
                (style.frac_num_style(), style.frac_den_style())
            };

            let num_start = ctx.out_ops_len();
            let mut num_box = layout_node(ctx, num, num_style, 0.0, 0.0);
            let ic = node_italic_correction_styled(num, ctx.face_override, ctx.font_family, num_style)
                * (ctx.font_tier.font_size(num_style) * 0.001);
            num_box.w += ic;
            let num_end = ctx.out_ops_len();

            let den_start = ctx.out_ops_len();
            let mut den_box = layout_node(ctx, den, den_style, 0.0, 0.0);
            let ic = node_italic_correction_styled(den, ctx.face_override, ctx.font_family, den_style)
                * (ctx.font_tier.font_size(den_style) * 0.001);
            den_box.w += ic;
            let den_end = ctx.out_ops_len();

            // Skewed / slashed fraction layout (mlist.c lines 3023-3094)
            if *options & crate::math::math_ast::FRACTION_OPT_SKEWED != 0 {
                let slash_w = measure_char('/', Face::Roman, font_size, ctx.font_family);
                let (shift_up, shift_down, slash_x_rel, den_x_rel, total_w) =
                    crate::math::math_fraction::SkewedFractionGovernor::compute_skewed_geometry(
                        num_box.w,
                        den_box.w,
                        slash_w,
                        axis_h,
                        font_size,
                    );
                let num_x = origin_x;
                let num_y = origin_y + shift_up;
                let den_x = origin_x + den_x_rel;
                let den_y = origin_y - shift_down;
                let slash_x = origin_x + slash_x_rel;
                let slash_y = origin_y;

                ctx.shift_ops_range(num_start, num_end, num_x, num_y);
                ctx.shift_ops_range(den_start, den_end, den_x, den_y);

                ctx.emit_glyph_run(slash_x, slash_y, Face::Roman, font_size, "/");

                let max_h = (num_y + num_box.h - origin_y).max(0.7 * font_size);
                let max_d = (origin_y - (den_y - den_box.d)).max(0.3 * font_size);
                return MathBox::with_dims(total_w, max_h, max_d);
            }

            let frac_pad = if *has_rule {
                0.12 * font_size
            } else {
                0.0
            };
            let (frac_w, num_offset, den_offset) =
                NumeratorDenominatorAligner::align_centered_padded(num_box.w, den_box.w, frac_pad);
            let num_x = origin_x + num_offset;
            let den_x = origin_x + den_offset;

            let metrics = FractionShiftResolver::resolve(
                style,
                *has_rule,
                axis_h,
                rule_t,
                num_box.d,
                den_box.h,
                scale,
                params,
            );
            let num_y = origin_y + metrics.num_shift;
            let den_y = origin_y - metrics.den_shift;

            ctx.shift_ops_range(num_start, num_end, num_x, num_y);
            if *has_rule {
                let rule_y = origin_y + axis_h - 0.5 * rule_t;
                ctx.emit_rule(origin_x, rule_y, frac_w, rule_t);
            }
            ctx.shift_ops_range(den_start, den_end, den_x, den_y);

            let total_h = (num_y + num_box.h) - origin_y;
            let total_d = origin_y - (den_y - den_box.d);

            MathBox::with_dims(frac_w, total_h, total_d)
        }

        MathNode::Radical { degree, radicand } => {
            let rad_style = style.cramped();

            let rad_start = ctx.out_ops_len();
            let mut rad_box = layout_node(ctx, radicand, rad_style, 0.0, 0.0);
            let ic = node_italic_correction_styled(radicand, ctx.face_override, ctx.font_family, rad_style)
                * (ctx.font_tier.font_size(rad_style) * 0.001);
            rad_box.w += ic;
            let rad_end = ctx.out_ops_len();

            let deg_start = ctx.out_ops_len();
            let deg_box = if let Some(deg) = degree {
                layout_node(ctx, deg, MathStyle::CrampedScriptScript, 0.0, 0.0)
            } else {
                MathBox::zero()
            };
            let deg_end = ctx.out_ops_len();


            let (_deg_advance, deg_x_offset, rad_origin_x) = if degree.is_some() {
                let c = crate::math::fonts::math_constants_for_family(ctx.font_family);
                let kern_before = if ctx.font_family == metrics::FontFamily::ComputerModern {
                    (5.0 / 18.0) * font_size
                } else {
                    (c.radical_kern_before_degree as f32 * 0.001) * font_size
                };
                let mut kern_after = if ctx.font_family == metrics::FontFamily::ComputerModern {
                    (-10.0 / 18.0) * font_size
                } else {
                    (c.radical_kern_after_degree as f32 * 0.001) * font_size
                };
                // LuaTeX mlist.c:2389-2390 clamp: if -ar > (wr + br) { ar = -(wr + br); }
                if -kern_after > deg_box.w + kern_before {
                    kern_after = -(deg_box.w + kern_before);
                }
                let advance = (kern_before + deg_box.w + kern_after).max(0.0);
                (advance, kern_before, origin_x + advance)
            } else {
                (0.0, 0.0, origin_x)
            };

            let c = crate::math::fonts::math_constants_for_family(ctx.font_family);
            let (rule_t_cm, min_clr) = if ctx.font_family == metrics::FontFamily::ComputerModern {
                let r_t = (params.rule_thickness as f32) * scale;
                let clr = if style.is_display() {
                    (params.rad_clearance_display as f32) * scale
                } else {
                    (params.rad_clearance_text as f32) * scale
                };
                (r_t, clr)
            } else {
                let r_t = (0.040 * font_size).max(0.398);
                let clr = if style.is_display() {
                    (c.radical_display_style_vertical_gap as f32 * 0.001) * font_size
                } else {
                    (c.radical_vertical_gap as f32 * 0.001) * font_size
                };
                (r_t, clr)
            };
            let deg_raise_percent = c.radical_degree_bottom_raise_percent;
            let v = rad_box.h + rad_box.d + min_clr + rule_t_cm;

            if let Some(otf) = crate::math::fonts::active_opentype_math_font(ctx.font_family) {
                if let Some(crate::math::fonts::ResolvedDelimiter::Variant { glyph_id, advance }) =
                    crate::math::fonts::OpenTypeDelimKind::Radical.resolve_font_delimiter(otf, v, font_size)
                {
                    if crate::math::fonts::get_vector_glyph_path(glyph_id.0).is_some() {
                        let rad_w = (otf.advance_width(glyph_id).unwrap_or(833) as f32 * 0.001) * font_size;
                        let rad_total_hd = (advance as f32 * 0.001) * font_size;
                        let delta = rad_total_hd - (rad_box.h + rad_box.d + min_clr + rule_t_cm);
                        let actual_clr = if delta > 0.0 {
                            min_clr + delta * 0.5
                        } else {
                            min_clr
                        };
                        let rule_bottom_y = origin_y + rad_box.h + actual_clr;
                        let rad_content_x = rad_origin_x + rad_w;
                        let overlap = (0.5 * rule_t_cm).max(0.25);
                        let rule_start = rad_content_x - overlap;
                        let rule_w = (rad_content_x + rad_box.w) - rule_start;

                        if !ctx.is_measuring() {
                            let hook_y = crate::math::fonts::vector_radical_hook_y(glyph_id.0);
                            let rad_glyph_y = rule_bottom_y + rule_t_cm - (hook_y as f32 * 0.001) * font_size;
                            ctx.emit_vector_glyph(rad_origin_x, rad_glyph_y, font_size, glyph_id.0);
                            ctx.emit_rule(rule_start, rule_bottom_y, rule_w, rule_t_cm);
                        }
                        ctx.shift_ops_range(rad_start, rad_end, rad_content_x, origin_y);

                        let rad_d = rad_total_hd - rule_t_cm;
                        let mut total_d = rad_box.d.max(rad_d - rad_box.h - actual_clr);
                        let mut total_h = (rule_bottom_y + rule_t_cm + rule_t_cm) - origin_y;

                        if degree.is_some() {
                            let deg_x = origin_x + deg_x_offset;
                            let deg_y = if ctx.font_family == metrics::FontFamily::ComputerModern {
                                origin_y + 0.60 * ((rule_bottom_y + rule_t_cm - origin_y) - total_d)
                            } else {
                                let rad_bottom_y = rule_bottom_y - rad_d;
                                rad_bottom_y + (deg_raise_percent as f32 * 0.01) * rad_total_hd
                            };
                            ctx.shift_ops_range(deg_start, deg_end, deg_x, deg_y);
                            total_h = total_h.max(deg_y + deg_box.h - origin_y);
                            total_d = total_d.max(origin_y - deg_y + deg_box.d);
                        }

                        let trailing_clr = if ctx.font_family == metrics::FontFamily::ComputerModern {
                            0.0
                        } else {
                            0.20 * font_size
                        };
                        let total_w = rad_content_x + rad_box.w + trailing_clr - origin_x;
                        return MathBox::with_dims(total_w, total_h, total_d);
                    }
                }
            }

            // 1. Check small radical in cmsy10 first (Family 2, char 0x70)
            let small_m = crate::math::math_tfm::cmsy10_metric(0x70);

            let (rad_w, rad_h, rad_d, is_ext, face, rad_code, n_rep) = {
                let kind = crate::math::fonts::OpenTypeDelimKind::Radical;
                match kind.select_variant_tier(v, font_size) {
                    Some(0) => {
                        let w = (small_m.w as f32 * 0.001) * font_size;
                        let h = (small_m.h as f32 * 0.001) * font_size;
                        let d = (small_m.d as f32 * 0.001) * font_size;
                        (w, h, d, false, Face::Symbol, 0x70u8, 0usize)
                    }
                    Some(tier) => {
                        let code = match tier {
                            1 => 0x70,
                            2 => 0x71,
                            3 => 0x72,
                            _ => 0x73,
                        };
                        let m = crate::math::math_tfm::cmex10_metric(code);
                        let w = (m.w as f32 * 0.001) * font_size;
                        let h = (m.h as f32 * 0.001) * font_size;
                        let d = (m.d as f32 * 0.001) * font_size;
                        (w, h, d, false, Face::Extension, code, 0usize)
                    }
                    None => {
                        // Extensible radical in cmex10 (top 0x76, rep 0x75, bot 0x74)
                        let m_top = crate::math::math_tfm::cmex10_metric(0x76);
                        let m_bot = crate::math::math_tfm::cmex10_metric(0x74);
                        let m_rep = crate::math::math_tfm::cmex10_metric(0x75);
                        let h_fixed = ((m_top.h + m_top.d + m_bot.h + m_bot.d) as f32 * 0.001) * font_size;
                        let u = ((m_rep.h + m_rep.d) as f32 * 0.001) * font_size;
                        let n = if u > 0.0 && v > h_fixed {
                            ((v - h_fixed) / u).ceil() as usize
                        } else {
                            0
                        };
                        let total_hd = h_fixed + (n as f32) * u;
                        let w = (m_top.w as f32 * 0.001) * font_size;
                        let h = (m_top.h as f32 * 0.001) * font_size;
                        let d = total_hd - h;
                        (w, h, d, true, Face::Extension, 0x74u8, n)
                    }
                }
            };

            // TeX Appendix G Rule 11: Distribute excess height evenly
            let rad_total_hd = rad_h + rad_d;
            let delta = rad_total_hd - (rad_box.h + rad_box.d + min_clr + rule_t_cm);
            let actual_clr = if delta > 0.0 {
                min_clr + delta * 0.5
            } else {
                min_clr
            };

            let rule_bottom_y = origin_y + rad_box.h + actual_clr;
            let rad_y = rule_bottom_y + rule_t_cm - rad_h;
            let rad_content_x = rad_origin_x + rad_w;

            if !ctx.is_measuring() {
                if !is_ext {
                    let s = crate::math::math_delim::byte_str(rad_code);
                    ctx.emit_glyph_run(rad_origin_x, rad_y, face, font_size, s);
                } else {
                    // Extensible radical from cmex10
                    // Top piece (0x76): its upper horizontal ledge sits at height m_top.h above baseline.
                    // We align this ledge with rule_bottom_y + rule_t_cm so the overbar connects seamlessly.
                    let m_top = crate::math::math_tfm::cmex10_metric(0x76);
                    let m_rep = crate::math::math_tfm::cmex10_metric(0x75);
                    let m_bot = crate::math::math_tfm::cmex10_metric(0x74);

                    let top_glyph_h = (m_top.h as f32 * 0.001) * font_size;
                    let top_baseline_y = rule_bottom_y + rule_t_cm - top_glyph_h;
                    let s_top = crate::math::math_delim::byte_str(0x76);
                    ctx.emit_glyph_run(rad_origin_x, top_baseline_y, Face::Extension, font_size, s_top);

                    let mut curr_y = top_baseline_y - (m_top.d as f32 * 0.001) * font_size;
                    let s_rep = crate::math::math_delim::byte_str(0x75);
                    let rep_d = (m_rep.d as f32 * 0.001) * font_size;
                    let rep_h = (m_rep.h as f32 * 0.001) * font_size;
                    for _ in 0..n_rep {
                        let rep_baseline = curr_y - rep_h;
                        ctx.emit_glyph_run(rad_origin_x, rep_baseline, Face::Extension, font_size, s_rep);
                        curr_y = rep_baseline - rep_d;
                    }

                    let bot_h = (m_bot.h as f32 * 0.001) * font_size;
                    let bot_baseline = curr_y - bot_h;
                    let s_bot = crate::math::math_delim::byte_str(0x74);
                    ctx.emit_glyph_run(rad_origin_x, bot_baseline, Face::Extension, font_size, s_bot);
                }

                // Overbar rule connecting seamlessly to glyph ledge:
                let overlap = (0.5 * rule_t_cm).max(0.20);
                let rule_start = rad_content_x - overlap;
                let rule_w = (rad_content_x + rad_box.w) - rule_start;
                ctx.emit_rule(
                    rule_start,
                    rule_bottom_y,
                    rule_w,
                    rule_t_cm,
                );
            }

                ctx.shift_ops_range(rad_start, rad_end, rad_content_x, origin_y);

                let mut total_d = rad_box.d.max(rad_d - rad_box.h - actual_clr);
                let mut total_h = (rule_bottom_y + rule_t_cm + rule_t_cm) - origin_y;

                if degree.is_some() {
                    let deg_x = origin_x + deg_x_offset;
                    let deg_y = if ctx.font_family == metrics::FontFamily::ComputerModern {
                        origin_y + 0.60 * ((rule_bottom_y + rule_t_cm - origin_y) - total_d)
                    } else {
                        let rad_bottom_y = rad_y - rad_d;
                        rad_bottom_y + (deg_raise_percent as f32 * 0.01) * (rad_h + rad_d)
                    };
                    ctx.shift_ops_range(deg_start, deg_end, deg_x, deg_y);
                    total_h = total_h.max(deg_y + deg_box.h - origin_y);
                    total_d = total_d.max(origin_y - deg_y + deg_box.d);
                }

                let trailing_clr = if ctx.font_family == metrics::FontFamily::ComputerModern {
                    0.0
                } else {
                    0.20 * font_size
                };
                let total_w = rad_content_x + rad_box.w + trailing_clr - origin_x;
                MathBox::with_dims(total_w, total_h, total_d)
        }

        MathNode::BigOp(data) => {
            let BigOpData {
                op,
                sub,
                sup,
                limits,
            } = &**data;
            let is_text_op = OperatorLimitSwitch::is_text_operator(op);
            let is_integral = OperatorLimitSwitch::is_integral_operator(op);
            let is_display_limits = OperatorLimitSwitch::is_display_limits(*limits, style);
            let op_m = big_op_metrics(op, ctx.font_family);
            let mut cm_op_code: u8 = 0;
            let otf_resolved = if !is_text_op {
                crate::math::fonts::OpenTypeBigOpKind::from_command(op).and_then(|kind| {
                    if let Some(m) = crate::math::fonts::operators::NEWCM_OPERATOR_METRICS.get(kind as usize) {
                        Some(crate::math::fonts::ResolvedBigOp {
                            glyph_id: crate::math::fonts::GlyphId(0),
                            advance_width: 1000,
                            advance_height: if style.is_display() { m.display_advance } else { m.text_advance },
                            italic_correction: m.italic_correction as i16,
                            lower_limit_kern: m.lower_limit_kern,
                        })
                    } else {
                        crate::math::fonts::active_opentype_math_font(ctx.font_family)
                            .and_then(|otf| kind.resolve_font_operator(otf, style))
                    }
                })
            } else {
                None
            };
            let (op_face, op_size, op_w, op_h, op_d, op_ic) = if is_text_op {
                let w = metrics::string_width_family(op, Face::Roman, font_size, ctx.font_family);
                (Face::Roman, font_size, w, (op_m.text_height as f32 * 0.001) * font_size, 0.0, 0.0)
            } else if is_integral {
                let code = BigOpVariantTier::resolve_integral_cmex_code(op, style.is_display());
                cm_op_code = code;
                let m = crate::math::math_tfm::cmex10_metric(code);
                let single_w = ((m.w + m.ic) as f32) * (font_size * 0.001);
                let h = (m.h as f32) * (font_size * 0.001);
                let d = (m.d as f32) * (font_size * 0.001);
                let ic = if let Some(res) = otf_resolved {
                    if res.italic_correction > 0 {
                        (res.italic_correction as f32 * 0.001) * font_size
                    } else {
                        (m.ic as f32) * (font_size * 0.001)
                    }
                } else {
                    (m.ic as f32) * (font_size * 0.001)
                };
                let (w, _) = IntegralOverhangCorrector::total_integral_width(
                    op,
                    single_w,
                    font_size,
                    style.is_display(),
                );
                (Face::Extension, font_size, w, h, d, ic)
            } else {
                let cmex_code = BigOpVariantTier::resolve_cmex_code(op, style.is_display());
                cm_op_code = cmex_code;
                let m = crate::math::math_tfm::cmex10_metric(cmex_code);
                let w = ((m.w + m.ic) as f32) * (font_size * 0.001);
                let h = (m.h as f32) * (font_size * 0.001);
                let d = (m.d as f32) * (font_size * 0.001);
                let ic = if let Some(res) = otf_resolved {
                    if res.italic_correction > 0 {
                        (res.italic_correction as f32 * 0.001) * font_size
                    } else {
                        (m.ic as f32) * (font_size * 0.001)
                    }
                } else {
                    (m.ic as f32) * (font_size * 0.001)
                };
                (Face::Extension, font_size, w, h, d, ic)
            };

            // TeX Appendix G Rule 13: shift operator vertically so center is at math axis
            let delta_y = Rule13OperatorAxis::compute_delta_y(is_text_op, axis_h, op_h, op_d);
            let op_y = origin_y + delta_y;
            let op_top_y = op_y + op_h;
            let op_bot_y = op_y - op_d;
            let half_ic = 0.5 * op_ic;

            let emit_op = |ctx: &mut MathLayoutContext<'a, 'p>, x: f32, y: f32| {
                if is_text_op {
                    ctx.emit_glyph_run(x, y, op_face, op_size, op);
                } else if is_integral {
                    let int_glyph = crate::math::math_delim::byte_str(cm_op_code);
                    let m = crate::math::math_tfm::cmex10_metric(cm_op_code);
                    let single_w = ((m.w + m.ic) as f32) * (op_size * 0.001);
                    let offset = IntegralOverhangCorrector::integral_offset(
                        single_w,
                        op_size,
                        style.is_display(),
                    );
                    if *op == "iint" || *op == "oiint" {
                        ctx.emit_glyph_run(x, y, Face::Extension, op_size, int_glyph);
                        ctx.emit_glyph_run(
                            x + offset,
                            y,
                            Face::Extension,
                            op_size,
                            int_glyph,
                        );
                    } else if *op == "iiint" || *op == "oiiint" {
                        ctx.emit_glyph_run(x, y, Face::Extension, op_size, int_glyph);
                        ctx.emit_glyph_run(
                            x + offset,
                            y,
                            Face::Extension,
                            op_size,
                            int_glyph,
                        );
                        ctx.emit_glyph_run(
                            x + 2.0 * offset,
                            y,
                            Face::Extension,
                            op_size,
                            int_glyph,
                        );
                    } else {
                        ctx.emit_glyph_run(x, y, Face::Extension, op_size, int_glyph);
                    }
                } else {
                    let s = crate::math::math_delim::byte_str(cm_op_code);
                    ctx.emit_glyph_run(x, y, Face::Extension, op_size, s);
                }
            };

            if is_display_limits {
                let sub_start = ctx.out_ops_len();
                let sub_box = if let Some(s) = sub {
                    layout_node(ctx, s, style.sub_style(), 0.0, 0.0)
                } else {
                    MathBox::zero()
                };
                let sub_end = ctx.out_ops_len();

                let sup_start = ctx.out_ops_len();
                let sup_box = if let Some(s) = sup {
                    layout_node(ctx, s, style.sup_style(), 0.0, 0.0)
                } else {
                    MathBox::zero()
                };
                let sup_end = ctx.out_ops_len();

                let (max_w, op_x, sup_x, sub_x) = OperatorCenteringEngine::center_limits_positioned(
                    origin_x,
                    op_w,
                    sup_box.w,
                    sub_box.w,
                    sup.is_some(),
                    sub.is_some(),
                    half_ic,
                );

                let big_metrics = BigOpShiftResolver::resolve_limit_metrics(
                    ctx.font_family,
                    font_size,
                    scale,
                    params,
                );

                let (sup_y, sub_y) = LimitClearanceGovernor::compute_limit_positions(
                    op_top_y,
                    op_bot_y,
                    sup_box.d,
                    sub_box.h,
                    &big_metrics,
                );

                if !ctx.is_measuring() {
                    emit_op(ctx, op_x, op_y);
                }

                if sup.is_some() {
                    ctx.shift_ops_range(sup_start, sup_end, sup_x, sup_y);
                }

                if sub.is_some() {
                    ctx.shift_ops_range(sub_start, sub_end, sub_x, sub_y);
                }

                let op_extra_pad = (params.big_op_spacing5 as f32) * scale;
                LimitClearanceGovernor::compute_limits_box(
                    max_w,
                    origin_y,
                    op_top_y,
                    op_bot_y,
                    sup.is_some(),
                    sup_y,
                    sup_box.h,
                    sub.is_some(),
                    sub_y,
                    sub_box.d,
                    op_extra_pad,
                    font_size,
                )
            } else {
                let sup_start = ctx.out_ops_len();
                let sup_box = if let Some(sp) = sup {
                    layout_node(ctx, sp, style.sup_style(), 0.0, 0.0)
                } else {
                    MathBox::zero()
                };
                let sup_end = ctx.out_ops_len();

                let sub_start = ctx.out_ops_len();
                let sub_box = if let Some(sb) = sub {
                    layout_node(ctx, sb, style.sub_style(), 0.0, 0.0)
                } else {
                    MathBox::zero()
                };
                let sub_end = ctx.out_ops_len();

                let (sup_x_rel, sub_x_rel, mut sup_shift, mut sub_shift) =
                    BigOpShiftResolver::resolve_side_scripts(
                        op,
                        is_integral,
                        style.is_display(),
                        op_w,
                        op_h,
                        op_d,
                        op_ic,
                        delta_y,
                        font_size,
                        scale,
                        ctx.font_family == metrics::FontFamily::ComputerModern,
                        sup.is_some(),
                        params,
                    );

                let sup_x = origin_x + sup_x_rel;
                let sub_x = origin_x + sub_x_rel;

                // TeX Rule 18 clearance check
                if sup.is_some() && sub.is_some() {
                    let min_clearance = (params.sub_sup_gap as f32) * scale;
                    let x_height = (params.accent_base_height as f32) * scale;
                    let (s_adj, b_adj) = BigOpShiftResolver::enforce_side_script_clearance(
                        sup_shift,
                        sub_shift,
                        sup_box.d,
                        sub_box.h,
                        min_clearance,
                        x_height,
                    );
                    sup_shift = s_adj;
                    sub_shift = b_adj;
                }

                if !ctx.is_measuring() {
                    emit_op(ctx, origin_x, op_y);
                }

                if sup.is_some() {
                    ctx.shift_ops_range(sup_start, sup_end, sup_x, origin_y + sup_shift);
                }
                if sub.is_some() {
                    ctx.shift_ops_range(sub_start, sub_end, sub_x, origin_y - sub_shift);
                }

                BigOpShiftResolver::compute_side_scripts_box(
                    op_w,
                    delta_y,
                    op_h,
                    op_d,
                    sup.is_some(),
                    sup_x_rel,
                    sup_box.w,
                    sup_shift,
                    sup_box.h,
                    sub.is_some(),
                    sub_x_rel,
                    sub_box.w,
                    sub_shift,
                    sub_box.d,
                )
            }
        }

        MathNode::Delimited(data) => {
            let DelimitedData {
                open,
                close,
                content,
                ..
            } = &**data;

            let is_atop = match &**content {
                MathNode::Fraction { has_rule: false, .. } => true,
                MathNode::StyleScope { content: inner, .. } => matches!(&**inner, MathNode::Fraction { has_rule: false, .. }),
                _ => false,
            };

            let shortfall = if ctx.font_family == metrics::FontFamily::ComputerModern {
                5.0
            } else {
                5.0 * (font_size / 10.0)
            };

            let has_middle = content_has_middle(content);
            let (content_start, content_box, content_end) = if has_middle {
                let mut dry = ctx.dry_context();
                let initial_box = layout_node(&mut dry, content, style, 0.0, 0.0);
                let initial_target_h = crate::math::math_delim::DelimiterSizingGovernor::target_height_with_shortfall(
                    initial_box.h,
                    initial_box.d,
                    axis_h,
                    font_size,
                    shortfall,
                );
                let prev_target = ctx.target_middle_delim_h;
                ctx.target_middle_delim_h = Some(initial_target_h);
                let start = ctx.out_ops_len();
                let b = layout_node(ctx, content, style, 0.0, 0.0);
                let end = ctx.out_ops_len();
                ctx.target_middle_delim_h = prev_target;
                (start, b, end)
            } else {
                let start = ctx.out_ops_len();
                let b = layout_node(ctx, content, style, 0.0, 0.0);
                let end = ctx.out_ops_len();
                (start, b, end)
            };

            let mut target_h = crate::math::math_delim::DelimiterSizingGovernor::target_height_with_shortfall(
                content_box.h,
                content_box.d,
                axis_h,
                font_size,
                shortfall,
            );

            // Canonical TeX Appendix G Rule 15e & mlist.c lines 3131-3138:
            // Generalized fractions (\atopwithdelims, \binom) enforce minimum delimiter extent from font params (delim1/delim2)
            if is_atop {
                target_h = target_h.max(DelimiterSizingGovernor::atop_min_delimiter(
                    style.is_display(),
                    params.delim1,
                    scale,
                    font_size,
                ));
            }
            let y_center = origin_y + axis_h;

            let (open_w, open_h) = layout_delimiter(
                ctx,
                open,
                true,
                origin_x,
                y_center,
                target_h,
                font_size,
                rule_t,
            );

            let content_x = origin_x + open_w;
            let close_x = content_x + content_box.w;

            ctx.shift_ops_range(content_start, content_end, content_x, origin_y);

            let (close_w, close_h) = layout_delimiter(
                ctx, close, false, close_x, y_center, target_h, font_size, rule_t,
            );

            DelimiterSizingGovernor::compute_delimited_box(
                open_w,
                open_h,
                content_box.w,
                content_box.h,
                content_box.d,
                close_w,
                close_h,
                target_h,
                axis_h,
            )
        }

        MathNode::Matrix(data) => {
            let MatrixData {
                rows,
                delim,
                env_kind,
                col_alignments,
            } = &**data;
            let delim = *delim;
            let env_kind = *env_kind;
            if rows.is_empty() {
                return MathBox::zero();
            }

            let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
            if num_cols == 0 {
                return MathBox::zero();
            }

            let num_rows = rows.len();
            let total_cells = num_rows * num_cols;

            let mut grid_buf = crate::math::math_matrix::MatrixGridBuffer::default();

            let mut heap_col_widths;
            let mut heap_col_offsets;
            let mut heap_row_heights;
            let mut heap_row_depths;
            let mut heap_row_baseline_offsets;
            let mut heap_cell_widths;

            let (col_widths, col_offsets): (&mut [f32], &mut [f32]) =
                if num_cols <= crate::math::math_matrix::MATRIX_STACK_COLS {
                    (
                        &mut grid_buf.col_widths[..num_cols],
                        &mut grid_buf.col_offsets[..num_cols],
                    )
                } else {
                    heap_col_widths = vec![0.0f32; num_cols];
                    heap_col_offsets = vec![0.0f32; num_cols];
                    (&mut heap_col_widths[..], &mut heap_col_offsets[..])
                };

            let (row_heights, row_depths, row_baseline_offsets): (&mut [f32], &mut [f32], &mut [f32]) =
                if num_rows <= crate::math::math_matrix::MATRIX_STACK_ROWS {
                    (
                        &mut grid_buf.row_heights[..num_rows],
                        &mut grid_buf.row_depths[..num_rows],
                        &mut grid_buf.row_baseline_offsets[..num_rows],
                    )
                } else {
                    heap_row_heights = vec![0.0f32; num_rows];
                    heap_row_depths = vec![0.0f32; num_rows];
                    heap_row_baseline_offsets = vec![0.0f32; num_rows];
                    (
                        &mut heap_row_heights[..],
                        &mut heap_row_depths[..],
                        &mut heap_row_baseline_offsets[..],
                    )
                };

            let cell_widths: &mut [f32] =
                if total_cells <= crate::math::math_matrix::MATRIX_STACK_CELLS {
                    &mut grid_buf.cell_widths[..total_cells]
                } else {
                    heap_cell_widths = vec![0.0f32; total_cells];
                    &mut heap_cell_widths[..]
                };

            let (min_rh, min_rd, row_sep) =
                crate::math::math_matrix::MatrixLayoutMetricsEngine::row_parameters_font(
                    env_kind, font_size, Some(ctx.opentype_engine().constants()),
                );
            let cell_style = if env_kind == MatrixEnvKind::SmallMatrix {
                MathStyle::Script
            } else {
                style.matrix_cell_style()
            };

            // First pass: content-sensitive measurement of cell widths, row heights, and row depths
            let mut dry_ctx = ctx.dry_context();
            for (r_idx, r) in rows.iter().enumerate() {
                let row_cell_base = r_idx * num_cols;
                let rh = &mut row_heights[r_idx];
                let rd = &mut row_depths[r_idx];
                for (col_idx, cell) in r.iter().enumerate() {
                    let b = layout_node(&mut dry_ctx, cell, cell_style, 0.0, 0.0);
                    cell_widths[row_cell_base + col_idx] = b.w;
                    if b.w > col_widths[col_idx] {
                        col_widths[col_idx] = b.w;
                    }
                    if b.h > *rh {
                        *rh = b.h;
                    }
                    if b.d > *rd {
                        *rd = b.d;
                    }
                }
                // Enforce minimum row typographic clearance:
                *rh = (*rh).max(min_rh);
                *rd = (*rd).max(min_rd);
            }

            let matrix_w = crate::math::math_matrix::MatrixLayoutMetricsEngine::compute_col_offsets_and_width(
                col_widths, col_offsets, env_kind, font_size, num_rows,
            );
            let total_content_h = crate::math::math_matrix::MatrixLayoutMetricsEngine::compute_row_baseline_offsets_and_total_height(
                row_heights, row_depths, row_baseline_offsets, row_sep,
            );

            // Center the entire matrix block symmetrically across the math axis:
            let top_y = origin_y + axis_h + (total_content_h * 0.5);

            let (open_str, close_str) =
                crate::math::math_matrix::MatrixDelimBinder::resolve_delims(delim);
            let has_delim = !open_str.is_empty();
            let y_center = origin_y + axis_h;
            let m_metrics = matrix_metrics(ctx.font_family);
            let (target_h, sep) = if ctx.font_family == metrics::FontFamily::ComputerModern {
                (total_content_h, 0.0)
            } else {
                let th = total_content_h
                    + ((m_metrics.extra_target_h as f32 * 0.001) * font_size).max(2.0);
                let sep_ratio = match delim {
                    MatrixDelim::VBar | MatrixDelim::DoubleVBar => m_metrics.vbar_sep,
                    MatrixDelim::Brackets => m_metrics.bracket_sep,
                    _ => m_metrics.default_sep,
                };
                let s = ((sep_ratio as f32 * 0.001) * font_size).max(1.8);
                (th, s)
            };

            let (open_w, open_h) = if has_delim {
                layout_delimiter(
                    &mut dry_ctx,
                    open_str,
                    true,
                    0.0,
                    y_center,
                    target_h,
                    font_size,
                    rule_t,
                )
            } else {
                (0.0, 0.0)
            };

            let cells_start_x = if has_delim {
                origin_x + open_w + sep
            } else {
                origin_x
            };

            if !ctx.is_measuring() {
                // Precompute column alignments to avoid repeated match branches in inner row loop
                let mut stack_aligns = [ColAlign::Center; crate::math::math_matrix::MATRIX_STACK_COLS];
                let mut heap_aligns;
                let col_aligns: &[ColAlign] = if num_cols <= crate::math::math_matrix::MATRIX_STACK_COLS {
                    for c in 0..num_cols {
                        stack_aligns[c] = crate::math::math_matrix::MatrixLayoutMetricsEngine::resolve_alignment(
                            env_kind, col_alignments, c,
                        );
                    }
                    &stack_aligns[..num_cols]
                } else {
                    heap_aligns = Vec::with_capacity(num_cols);
                    for c in 0..num_cols {
                        heap_aligns.push(crate::math::math_matrix::MatrixLayoutMetricsEngine::resolve_alignment(
                            env_kind, col_alignments, c,
                        ));
                    }
                    &heap_aligns[..]
                };

                let multline_gap = 1.0 * font_size;
                // Second pass: paint cells aligned to each row's baseline and aligned within each column
                for (r_idx, r) in rows.iter().enumerate() {
                    let curr_y = top_y - row_baseline_offsets[r_idx];
                    let row_cell_base = r_idx * num_cols;
                    for (col_idx, cell) in r.iter().enumerate() {
                        let col_w = col_widths[col_idx];
                        let cell_w = cell_widths[row_cell_base + col_idx];
                        let align = col_aligns[col_idx];

                        let offset_x = crate::math::math_matrix::MatrixLayoutMetricsEngine::compute_cell_offset_x(
                            env_kind, align, col_w, cell_w, matrix_w, r_idx, num_rows, multline_gap,
                        );

                        layout_node(
                            ctx,
                            cell,
                            cell_style,
                            cells_start_x + col_offsets[col_idx] + offset_x,
                            curr_y,
                        );
                    }
                }

                if has_delim {
                    layout_delimiter(
                        ctx, open_str, true, origin_x, y_center, target_h, font_size, rule_t,
                    );
                }
            }

            if has_delim {
                let has_close = !close_str.is_empty();
                let (close_x, close_w, close_h) = if has_close {
                    let cx = cells_start_x + matrix_w + sep;
                    let (cw, ch) = layout_delimiter(
                        ctx, close_str, false, cx, y_center, target_h, font_size, rule_t,
                    );
                    (cx, cw, ch)
                } else if delim == MatrixDelim::LeftBrace {
                    // Canonical \nulldelimiterspace margin (1.2pt = 0.12 * font_size)
                    (cells_start_x + matrix_w, 0.12 * font_size, 0.0)
                } else {
                    (cells_start_x + matrix_w, 0.0, 0.0)
                };
                let delim_extent = open_h.max(close_h);
                let half_h = (delim_extent * 0.5).max(total_content_h * 0.5);
                MathBox::with_dims(
                    (close_x + close_w) - origin_x,
                    axis_h + half_h,
                    half_h - axis_h,
                )
            } else {
                let half_h = total_content_h * 0.5;
                MathBox::with_dims(matrix_w, axis_h + half_h, half_h - axis_h)
            }
        }

        MathNode::Accent { accent, nucleus } => {
            let nuc_style = if *accent == "overline" {
                style.cramped()
            } else {
                style
            };
            let nuc_box = layout_node(ctx, nucleus, nuc_style, origin_x, origin_y);
            let ic = node_italic_correction(nucleus, ctx.face_override, ctx.font_family) * scale;
            let skew_offset = compute_top_accent_skew_offset(nucleus, ctx, font_size, scale, ic);

            // TeX Appendix G Rule 8 & 9: overline and underline
            if *accent == "overline" {
                let rule_w = nuc_box.w;
                let rule_y = origin_y + nuc_box.h + 3.0 * rule_t;
                ctx.emit_rule(origin_x, rule_y, rule_w, rule_t);
                return MathBox::with_dims(
                    nuc_box.w,
                    nuc_box.h + 5.0 * rule_t,
                    nuc_box.d,
                );
            }
            if *accent == "underline" {
                let rule_w = nuc_box.w;
                let under_gap = if ctx.font_family == metrics::FontFamily::ComputerModern {
                    3.0 * rule_t
                } else {
                    1.5 * (font_size / 10.0)
                };
                let rule_y = origin_y - nuc_box.d - under_gap - rule_t;
                ctx.emit_rule(origin_x, rule_y, rule_w, rule_t);
                return MathBox::with_dims(
                    nuc_box.w,
                    nuc_box.h,
                    nuc_box.d + under_gap + 2.0 * rule_t,
                );
            }

            // TeX Appendix G Rule 12 & mlist.c for Computer Modern
            if ctx.font_family == metrics::FontFamily::ComputerModern {
                if let Some(record) = crate::math::math_fonts::CmAccentRecord::lookup(accent) {
                    let (face, code, acc_w, acc_h) = record.resolve_metric(nuc_box.w, font_size);
                    match record.position {
                        crate::math::math_fonts::AccentPosition::Top => {
                            let x_height = (params.accent_base_height as f32) * scale;
                            let delta = nuc_box.h.min(x_height);
                            let acc_y = origin_y + nuc_box.h - delta;
                            let acc_x = origin_x + (nuc_box.w - acc_w) * 0.5 + skew_offset;

                            ctx.emit_glyph_run(
                                acc_x,
                                acc_y,
                                face,
                                font_size,
                                crate::math::math_delim::byte_str(code),
                            );

                            return MathBox::with_dims(
                                nuc_box.w,
                                (acc_y + acc_h - origin_y).max(nuc_box.h),
                                nuc_box.d,
                            );
                        }
                        crate::math::math_fonts::AccentPosition::Bottom => {
                            // mlist.c lines 2880, 2903: bottom accent placement
                            let under_gap = 0.20 * font_size;
                            let acc_y = origin_y - nuc_box.d - acc_h - under_gap;
                            let acc_x = origin_x + (nuc_box.w - acc_w) * 0.5;

                            ctx.emit_glyph_run(
                                acc_x,
                                acc_y,
                                face,
                                font_size,
                                crate::math::math_delim::byte_str(code),
                            );

                            return MathBox::with_dims(
                                nuc_box.w,
                                nuc_box.h,
                                (origin_y - acc_y).max(nuc_box.d),
                            );
                        }
                        crate::math::math_fonts::AccentPosition::Overlay => {
                            // mlist.c lines 2791, 2911: overlay accent centered over nucleus
                            let center_y = origin_y + 0.5 * (nuc_box.h - nuc_box.d);
                            let acc_y = center_y - 0.5 * acc_h;
                            let acc_x = if acc_w == 0.0 {
                                origin_x
                            } else {
                                origin_x + (nuc_box.w - acc_w) * 0.5
                            };

                            ctx.emit_glyph_run(
                                acc_x,
                                acc_y,
                                face,
                                font_size,
                                crate::math::math_delim::byte_str(code),
                            );

                            return MathBox::with_dims(
                                nuc_box.w,
                                nuc_box.h.max(acc_y + acc_h - origin_y),
                                nuc_box.d.max(origin_y - acc_y),
                            );
                        }
                    }
                }
            }

            let ic_offset = skew_offset;
            if let Some(am) = lookup_accent_metrics(accent, ctx.font_family) {
                let acc_lift = (am.lift as f32 * 0.001) * font_size;
                let acc_extra_h = (am.extra_height as f32 * 0.001) * font_size;
                let acc_scale = am.scale as f32 * 0.001;

                if am.is_rule {
                    match *accent {
                        "overline" => {
                            let rule_w = nuc_box.w;
                            let rule_y = origin_y + nuc_box.h + acc_lift;
                            ctx.emit_rule(origin_x, rule_y, rule_w, rule_t);
                            MathBox::with_dims(
                                nuc_box.w,
                                nuc_box.h + acc_extra_h + rule_t,
                                nuc_box.d,
                            )
                        }
                        "underline" => {
                            let rule_w = nuc_box.w;
                            let rule_y = origin_y - nuc_box.d - acc_lift;
                            ctx.emit_rule(origin_x, rule_y, rule_w, rule_t);
                            MathBox::with_dims(
                                nuc_box.w,
                                nuc_box.h,
                                nuc_box.d + acc_extra_h + rule_t,
                            )
                        }
                        "bar" => {
                            let x_height = (params.accent_base_height as f32) * scale;
                            let base_h = nuc_box.h.max(x_height);
                            let bar_w = (nuc_box.w * (am.width_ratio as f32 * 0.001)).max(3.0);
                            let bar_x = origin_x + (nuc_box.w - bar_w) * 0.5 + ic_offset;
                            let bar_y = origin_y + base_h + acc_lift;
                            ctx.emit_rule(bar_x, bar_y, bar_w, rule_t);
                            MathBox::with_dims(nuc_box.w, base_h + acc_extra_h, nuc_box.d)
                        }
                        "dot" => {
                            let dot_w = (rule_t * acc_scale).max(1.0);
                            let dot_x = origin_x + (nuc_box.w - dot_w) * 0.5 + ic_offset;
                            let dot_y = origin_y + nuc_box.h + acc_lift;
                            ctx.emit_rule(dot_x, dot_y, dot_w, dot_w);
                            MathBox::with_dims(nuc_box.w, nuc_box.h + acc_extra_h, nuc_box.d)
                        }
                        "ddot" => {
                            let dot_w = (rule_t * acc_scale).max(1.0);
                            let sep = 2.0 * rule_t;
                            let ddot_x1 =
                                origin_x + (nuc_box.w - (2.0 * dot_w + sep)) * 0.5 + ic_offset;
                            let ddot_x2 = ddot_x1 + dot_w + sep;
                            let dot_y = origin_y + nuc_box.h + acc_lift;
                            ctx.emit_rule(ddot_x1, dot_y, dot_w, dot_w);
                            ctx.emit_rule(ddot_x2, dot_y, dot_w, dot_w);
                            MathBox::with_dims(nuc_box.w, nuc_box.h + acc_extra_h, nuc_box.d)
                        }
                        "dddot" => {
                            let dot_w = (rule_t * acc_scale).max(1.0);
                            let sep = 1.8 * rule_t;
                            let total_dots_w = 3.0 * dot_w + 2.0 * sep;
                            let dot1_x = origin_x + (nuc_box.w - total_dots_w) * 0.5 + ic_offset;
                            let dot2_x = dot1_x + dot_w + sep;
                            let dot3_x = dot2_x + dot_w + sep;
                            let dot_y = origin_y + nuc_box.h + acc_lift;
                            ctx.emit_rule(dot1_x, dot_y, dot_w, dot_w);
                            ctx.emit_rule(dot2_x, dot_y, dot_w, dot_w);
                            ctx.emit_rule(dot3_x, dot_y, dot_w, dot_w);
                            MathBox::with_dims(nuc_box.w, nuc_box.h + acc_extra_h, nuc_box.d)
                        }
                        "ddddot" => {
                            let dot_w = (rule_t * acc_scale).max(1.0);
                            let sep = 1.5 * rule_t;
                            let total_dots_w = 4.0 * dot_w + 3.0 * sep;
                            let dot1_x = origin_x + (nuc_box.w - total_dots_w) * 0.5 + ic_offset;
                            let dot2_x = dot1_x + dot_w + sep;
                            let dot3_x = dot2_x + dot_w + sep;
                            let dot4_x = dot3_x + dot_w + sep;
                            let dot_y = origin_y + nuc_box.h + acc_lift;
                            ctx.emit_rule(dot1_x, dot_y, dot_w, dot_w);
                            ctx.emit_rule(dot2_x, dot_y, dot_w, dot_w);
                            ctx.emit_rule(dot3_x, dot_y, dot_w, dot_w);
                            ctx.emit_rule(dot4_x, dot_y, dot_w, dot_w);
                            MathBox::with_dims(nuc_box.w, nuc_box.h + acc_extra_h, nuc_box.d)
                        }
                        _ => MathBox::with_dims(nuc_box.w, nuc_box.h, nuc_box.d),
                    }
                } else if *accent == "overrightarrow" || *accent == "overleftarrow" {
                    let mut drew_otf = false;
                    if let Some(kind) = crate::math::fonts::OpenTypeHorizKind::from_name(accent) {
                        if let Some(otf) = crate::math::fonts::active_opentype_math_font(ctx.font_family) {
                            if let Some(res) = kind.resolve_font_horizontal(otf, nuc_box.w, font_size) {
                                match res {
                                    crate::math::fonts::ResolvedHorizontal::Variant { glyph_id, advance } => {
                                        if crate::math::fonts::get_vector_glyph_path(glyph_id.0).is_some() {
                                            let eff_w = (advance as f32) * (font_size * 0.001);
                                            let acc_x = origin_x + (nuc_box.w - eff_w) * 0.5 + ic_offset;
                                            let acc_y = origin_y + nuc_box.h + acc_lift;
                                            ctx.emit_vector_glyph(acc_x, acc_y, font_size, glyph_id.0);
                                            drew_otf = true;
                                        }
                                    }
                                    crate::math::fonts::ResolvedHorizontal::Assembly { recipe, min_overlap } => {
                                        if crate::math::fonts::get_vector_glyph_path(recipe.left_glyph.0).is_some()
                                            && crate::math::fonts::get_vector_glyph_path(recipe.right_glyph.0).is_some()
                                            && crate::math::fonts::get_vector_glyph_path(recipe.ext_glyph.0).is_some()
                                        {
                                            let left_w = (otf.advance_width(recipe.left_glyph).unwrap_or(500) as f32) * (font_size * 0.001);
                                            let right_w = (otf.advance_width(recipe.right_glyph).unwrap_or(500) as f32) * (font_size * 0.001);
                                            let ext_w = (otf.advance_width(recipe.ext_glyph).unwrap_or(337) as f32) * (font_size * 0.001);
                                            let overlap_pt = (min_overlap as f32) * (font_size * 0.001);
                                            let acc_y = origin_y + nuc_box.h + acc_lift;
                                            let acc_x = origin_x + ic_offset;

                                            ctx.emit_vector_glyph(acc_x, acc_y, font_size, recipe.left_glyph.0);
                                            let right_x = acc_x + nuc_box.w - right_w;
                                            ctx.emit_vector_glyph(right_x, acc_y, font_size, recipe.right_glyph.0);
                                            let span = (right_x + overlap_pt) - (acc_x + left_w - overlap_pt);
                                            if span > 0.0 && ext_w > overlap_pt {
                                                let step = ext_w - overlap_pt;
                                                let n_ext = (span / step).ceil().max(1.0) as usize;
                                                let actual_step = span / n_ext as f32;
                                                let start_x = acc_x + left_w - overlap_pt;
                                                for i in 0..n_ext {
                                                    ctx.emit_vector_glyph(start_x + (i as f32) * actual_step, acc_y, font_size, recipe.ext_glyph.0);
                                                }
                                            }
                                            drew_otf = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !drew_otf {
                        let arrow_size = acc_scale * font_size;
                        let is_right = *accent == "overrightarrow";
                        let sym_idx = if is_right { 0xAE } else { 0xAC };
                        let arrow_w = (metrics::SYMBOL_WIDTHS[sym_idx] as f32 * 0.001) * arrow_size;
                        let arrow_y = origin_y + nuc_box.h + acc_lift;
                        let shaft_y = arrow_y + 0.13 * arrow_size;
                        let sym_str = if is_right { "\u{00AE}" } else { "\u{00AC}" };
                        if nuc_box.w > arrow_w {
                            if is_right {
                                let arrow_x = origin_x + nuc_box.w - arrow_w + ic_offset;
                                let rule_w = (arrow_x - (origin_x + ic_offset)).max(1.0);
                                ctx.emit_rule(origin_x + ic_offset, shaft_y, rule_w + arrow_w * 0.25, rule_t);
                                ctx.emit_glyph_run(arrow_x, arrow_y, Face::Symbol, arrow_size, sym_str);
                            } else {
                                let arrow_x = origin_x + ic_offset;
                                let rule_start = arrow_x + arrow_w * 0.75;
                                let rule_w = (origin_x + nuc_box.w + ic_offset - rule_start).max(1.0);
                                ctx.emit_glyph_run(arrow_x, arrow_y, Face::Symbol, arrow_size, sym_str);
                                ctx.emit_rule(rule_start, shaft_y, rule_w, rule_t);
                            }
                        } else {
                            let arrow_x = origin_x + (nuc_box.w - arrow_w) * 0.5 + ic_offset;
                            ctx.emit_glyph_run(arrow_x, arrow_y, Face::Symbol, arrow_size, sym_str);
                        }
                    }
                    MathBox::with_dims(nuc_box.w, nuc_box.h + acc_extra_h, nuc_box.d)
                } else {
                    let is_wide = matches!(*accent, "widehat" | "widetilde");
                    let mut drew_otf = false;
                    let mut final_extra_h = acc_extra_h;
                    let x_height = (params.accent_base_height as f32) * scale;
                    if let Some(kind) = crate::math::fonts::OpenTypeHorizKind::from_name(accent) {
                        if let Some(otf) = crate::math::fonts::active_opentype_math_font(ctx.font_family) {
                            let res = if !is_wide && (*accent == "hat" || *accent == "tilde") {
                                let u = if *accent == "hat" { 0x0302 } else { 0x0303 };
                                otf.glyph_for_unicode(u).map(|gid| {
                                    let adv = otf.advance_width(gid).unwrap_or(500);
                                    crate::math::fonts::ResolvedHorizontal::Variant { glyph_id: gid, advance: adv }
                                })
                            } else {
                                kind.resolve_font_horizontal(otf, nuc_box.w, font_size)
                            };
                            if let Some(crate::math::fonts::ResolvedHorizontal::Variant { glyph_id, advance }) = res {
                                if crate::math::fonts::get_vector_glyph_path(glyph_id.0).is_some() {
                                    let eff_w = (advance as f32) * (font_size * 0.001);
                                    let acc_x = origin_x + (nuc_box.w - eff_w) * 0.5 + ic_offset;
                                    let acc_y = origin_y + (nuc_box.h - x_height).max(0.0);
                                    ctx.emit_vector_glyph(acc_x, acc_y, font_size, glyph_id.0);
                                    drew_otf = true;
                                    final_extra_h = ((nuc_box.h - x_height).max(0.0) + 0.25 * font_size).max(acc_extra_h);
                                }
                            }
                        }
                    }
                    if !drew_otf {
                        let base_w = (am.width_ratio as f32 * 0.001) * font_size;
                        let (eff_size, eff_w, eff_extra_h) = if is_wide && nuc_box.w > base_w * 1.1 {
                            let scale_factor = (nuc_box.w / base_w).clamp(1.0, 3.5);
                            (
                                acc_scale * font_size * scale_factor,
                                base_w * scale_factor,
                                acc_extra_h * scale_factor,
                            )
                        } else {
                            let eff_w = if *accent == "hat" {
                                measure_char('^', am.face, acc_scale * font_size, ctx.font_family)
                            } else {
                                base_w.min(nuc_box.w + 1.0)
                            };
                            (
                                acc_scale * font_size,
                                eff_w,
                                acc_extra_h,
                            )
                        };
                        final_extra_h = eff_extra_h;
                        let acc_x = origin_x + (nuc_box.w - eff_w) * 0.5 + ic_offset;
                        let acc_y = if *accent == "hat" {
                            origin_y + nuc_box.h - 0.22 * font_size
                        } else {
                            origin_y + nuc_box.h + acc_lift
                        };
                        ctx.emit_glyph_run(acc_x, acc_y, am.face, eff_size, am.glyph);
                    }
                    MathBox::with_dims(nuc_box.w, nuc_box.h + final_extra_h, nuc_box.d)
                }
            } else {
                MathBox::with_dims(nuc_box.w, nuc_box.h, nuc_box.d)
            }
        }

        MathNode::HBar => {
            let default_face = if ctx.font_family == metrics::FontFamily::ComputerModern {
                Face::MathItalic
            } else {
                Face::Italic
            };
            let face = ctx.face_override.unwrap_or(default_face);
            let w = measure_char('h', face, font_size, ctx.font_family);
            let (h_u, d_u) = char_height_depth('h', face, ctx.font_family);
            let h = (h_u as f32) * scale;
            let d = (d_u as f32) * scale;

            ctx.emit_glyph_run(origin_x, origin_y, face, font_size, "h");
            let hm = hbar_metrics(ctx.font_family);
            let bar_w = (hm.bar_w as f32 * 0.001) * font_size;
            let bar_x = origin_x + (hm.bar_x as f32 * 0.001) * font_size;
            let bar_y = origin_y + (hm.bar_y as f32 * 0.001) * font_size;
            ctx.emit_rule(bar_x, bar_y, bar_w, rule_t);

            MathBox::with_dims(w, h, d)
        }

        MathNode::Space(amount) => MathBox::with_dims(*amount, 0.0, 0.0),

        MathNode::Nonscript => MathBox::zero(),

        MathNode::StyleChange { face, content } => {
            let prev = ctx.face_override;
            ctx.face_override = Some(*face);
            let b = layout_node(ctx, content, style, origin_x, origin_y);
            ctx.face_override = prev;
            b
        }

        MathNode::Row(nodes) => {
            let n = nodes.len();
            if n == 0 {
                return MathBox::default();
            }
            if n == 1 {
                return layout_node(ctx, &nodes[0], style, origin_x, origin_y);
            }

            // Hardware L1 cache-resident stack buffer (contiguous SoA layout, zero heap alloc)
            let mut stack_kinds = [MathAtomKind::Ord; 128];
            let mut heap_kinds;

            let atom_kinds: &mut [MathAtomKind] = if n <= 128 {
                &mut stack_kinds[..n]
            } else {
                heap_kinds = vec![MathAtomKind::Ord; n];
                &mut heap_kinds[..]
            };

            let mut atom_count = 0usize;
            for child in nodes.iter() {
                if !matches!(child, MathNode::Space(_) | MathNode::Nonscript) {
                    atom_kinds[atom_count] = child.atom_kind_in_style(style);
                    atom_count += 1;
                }
            }

            // TeX Appendix G Rule 5 normalization over the mathematical atom sequence
            normalize_atom_kinds(&mut atom_kinds[..atom_count]);

            let is_cm = ctx.font_family == metrics::FontFamily::ComputerModern;
            let is_disp_or_text = style.is_display_or_text();
            let is_disp = style.is_display();
            let tier = style.tier();
            let mu_table = if is_cm {
                let quad_sp = crate::math::math_box::ScaledPoints::from_pt(
                    (params.quad as f32) * scale,
                );
                let mu_1 = quad_sp / 18;
                [
                    0.0f32,
                    (mu_1 * 1).to_pt(),
                    (mu_1 * 2).to_pt(),
                    (mu_1 * 3).to_pt(),
                    (mu_1 * 4).to_pt(),
                    (mu_1 * 5).to_pt(),
                ]
            } else {
                [0.0; 6]
            };
            let lut_mode = (2 - (is_disp as usize) - (is_disp_or_text as usize)) & 3;
            let lut_row = &crate::math::math_spacing::ATOM_SPACING_LUT.0[lut_mode];

            let mut curr_x = origin_x;
            let mut max_h = 0.0f32;
            let mut max_d = 0.0f32;
            let mut curr_atom_idx = 0usize;
            let mut prev_node_ic = 0.0f32;
            let mut prev_node: Option<&MathNode> = None;
            let mut pending_nonscript = false;

            for child in nodes.iter() {
                if matches!(child, MathNode::Nonscript) {
                    pending_nonscript = true;
                    continue;
                }
                if let MathNode::Space(amt) = child {
                    let eff_amt = if pending_nonscript && !is_disp_or_text {
                        0.0
                    } else {
                        *amt
                    };
                    pending_nonscript = false;
                    curr_x += eff_amt;
                    continue;
                }
                pending_nonscript = false;

                if curr_atom_idx > 0 {
                    let prev_k = curr_atom_idx - 1;
                    let prev_kind = atom_kinds[prev_k];
                    let curr_kind = atom_kinds[curr_atom_idx];
                    let mut space = if is_cm {
                        let mu = atom_spacing_mu(prev_kind, curr_kind, is_disp_or_text);
                        mu_table[mu as usize]
                    } else {
                        let idx = ((prev_kind as usize & 7) << 3) | (curr_kind as usize & 7);
                        lut_row[idx] * font_size
                    };

                    space += prev_node_ic;

                    if is_cm {
                        if let (Some(MathNode::Variable(c1)), MathNode::Variable(c2)) = (prev_node, child) {
                            space += crate::math::math_tfm::cmmi_pair_kern(*c1, *c2, tier, font_size);
                        }
                    }

                    curr_x += space;
                }

                prev_node = Some(child);
                prev_node_ic = node_italic_correction_styled(
                    child,
                    ctx.face_override,
                    ctx.font_family,
                    style,
                ) * scale;
                curr_atom_idx += 1;

                let b = layout_node(ctx, child, style, curr_x, origin_y);
                curr_x += b.w;
                if b.h > max_h {
                    max_h = b.h;
                }
                if b.d > max_d {
                    max_d = b.d;
                }
            }

            MathBox::with_dims(curr_x - origin_x, max_h, max_d)
        }

        MathNode::StyleScope {
            style: target_style,
            content,
        } => layout_node(ctx, content, *target_style, origin_x, origin_y),

        MathNode::Vcenter(content) => {
            // TeX Appendix G Rule 12: \vcenter centers the box vertically on the math axis
            let start = ctx.out_ops_len();
            let content_box = layout_node(ctx, content, style, 0.0, 0.0);
            let end = ctx.out_ops_len();
            let delta = axis_h - 0.5 * (content_box.h - content_box.d);
            ctx.shift_ops_range(start, end, origin_x, origin_y + delta);
            let total_h = axis_h + 0.5 * (content_box.h + content_box.d);
            let total_d = 0.5 * (content_box.h + content_box.d) - axis_h;
            MathBox::with_dims(content_box.w, total_h, total_d)
        }

        MathNode::MathChoice(choice) => {
            // Batch 1: Item 54 - MathChoiceQuadBranches
            // Selectively render ONLY the branch corresponding to the active style tier
            let branch = match style.tier() {
                0 => &choice.display,
                1 => &choice.text,
                2 => &choice.script,
                _ => &choice.script_script,
            };
            layout_node(ctx, branch, style, origin_x, origin_y)
        }

        MathNode::MiddleDelim { delim, .. } => {
            // Batch 1: Item 56 - MiddleDelimiterSupport
            let target_h = ctx.target_middle_delim_h.unwrap_or_else(|| {
                (params.axis_height as f32 * scale * 2.0).max(font_size)
            });
            let y_center = origin_y + axis_h;
            let (w, actual_h) = layout_delimiter(
                ctx, delim, false, origin_x, y_center, target_h, font_size, rule_t,
            );
            let half_h = (axis_h + actual_h * 0.5).max(0.0);
            let half_d = (actual_h * 0.5 - axis_h).max(0.0);
            MathBox::with_dims(w, half_h, half_d)
        }

        MathNode::SizedDelim { delim, size_tier, is_left, .. } => {
            // The TeXbook Chapter 17: Delimiter sizing hierarchy (\big, \Big, \bigg, \Bigg)
            let target_h = match size_tier {
                0 => 1.20 * font_size, // \big: tier 0 (12.0 pt for 10pt base)
                1 => 1.60 * font_size, // \Big: tier 1 (16.0 pt for 10pt base)
                2 => 2.10 * font_size, // \bigg: tier 2 (21.0 pt for 10pt base)
                _ => 2.70 * font_size, // \Bigg: tier 3 (27.0 pt for 10pt base)
            };
            let y_center = origin_y + axis_h;
            let (w, actual_h) = layout_delimiter(
                ctx, delim, *is_left, origin_x, y_center, target_h, font_size, rule_t,
            );
            let half_h = (axis_h + actual_h * 0.5).max(0.0);
            let half_d = (actual_h * 0.5 - axis_h).max(0.0);
            MathBox::with_dims(w, half_h, half_d)
        }

        MathNode::Overbrace(inner) => {
            layout_overbrace(ctx, inner, None, None, style, origin_x, origin_y)
        }

        MathNode::Underbrace(inner) => {
            layout_underbrace(ctx, inner, None, None, style, origin_x, origin_y)
        }

        MathNode::Overbracket(inner) => {
            layout_overbracket(ctx, inner, None, None, style, origin_x, origin_y)
        }

        MathNode::Underbracket(inner) => {
            layout_underbracket(ctx, inner, None, None, style, origin_x, origin_y)
        }

        MathNode::ExtensibleArrow(data) => {
            layout_extensible_arrow(ctx, data, style, origin_x, origin_y)
        }

        MathNode::Coerce { content, .. } => {
            layout_node(ctx, content, style, origin_x, origin_y)
        }

        MathNode::Ellipsis(kind) => {
            layout_ellipsis(ctx, *kind, origin_x, origin_y, font_size, axis_h)
        }

        MathNode::Tagged { formula, .. } => {
            layout_node(ctx, formula, style, origin_x, origin_y)
        }

        MathNode::Strut => {
            let h = 0.85 * font_size;
            let d = 0.35 * font_size;
            MathBox::with_dims(0.0, h, d)
        }

        MathNode::Phantom { content, width, height_depth } => {
            let inner_box = measure_math(content, font_size, ctx.font_family, style);
            let w = if *width { inner_box.w } else { 0.0 };
            let (h, d) = if *height_depth { (inner_box.h, inner_box.d) } else { (0.0, 0.0) };
            MathBox::with_dims(w, h, d)
        }

        MathNode::DimensionBox { content, zero_width, shift_left, zero_height_depth } => {
            let inner_box = if *shift_left {
                measure_math(content, font_size, ctx.font_family, style)
            } else {
                MathBox::zero()
            };
            let render_x = if *shift_left {
                origin_x - inner_box.w
            } else {
                origin_x
            };
            let painted_box = layout_node(ctx, content, style, render_x, origin_y);
            let w = if *zero_width { 0.0 } else { painted_box.w };
            let (h, d) = if *zero_height_depth { (0.0, 0.0) } else { (painted_box.h, painted_box.d) };
            MathBox::with_dims(w, h, d)
        }
    };
    ctx.style_stack.pop();
    result
}

fn layout_ellipsis<'a, 'p>(
    ctx: &mut MathLayoutContext<'a, 'p>,
    kind: crate::math::math_ast::EllipsisKind,
    origin_x: f32,
    origin_y: f32,
    font_size: f32,
    axis_h: f32,
) -> MathBox {
    use crate::math::math_ast::EllipsisKind;

    match kind {
        EllipsisKind::Low => {
            let w_dot = metrics::width_family('.', Face::Roman, font_size, ctx.font_family);
            let (h_u, d_u) = char_height_depth('.', Face::Roman, ctx.font_family);
            let h_dot = (h_u as f32) * 0.001 * font_size;
            let d_dot = (d_u as f32) * 0.001 * font_size;
            let thin = (3.0 / 18.0) * font_size; // TeX thinmuskip = 3 mu
            let step = w_dot + thin;
            ctx.emit_glyph_run(origin_x, origin_y, Face::Roman, font_size, ".");
            ctx.emit_glyph_run(origin_x + step, origin_y, Face::Roman, font_size, ".");
            ctx.emit_glyph_run(origin_x + 2.0 * step, origin_y, Face::Roman, font_size, ".");
            MathBox::with_dims(3.0 * w_dot + 2.0 * thin, h_dot, d_dot)
        }

        EllipsisKind::Centered => {
            let is_cm = ctx.font_family == metrics::FontFamily::ComputerModern;
            let (face, dot_str, w_dot) = if is_cm {
                (Face::Symbol, crate::math::math_delim::byte_str(0x01), 0.278 * font_size)
            } else {
                (Face::Symbol, "·", 0.278 * font_size)
            };
            let thin = (3.0 / 18.0) * font_size; // TeX thinmuskip = 3 mu
            let step = w_dot + thin;
            ctx.emit_glyph_run(origin_x, origin_y, face, font_size, dot_str);
            ctx.emit_glyph_run(origin_x + step, origin_y, face, font_size, dot_str);
            ctx.emit_glyph_run(origin_x + 2.0 * step, origin_y, face, font_size, dot_str);
            let h = axis_h + 0.15 * font_size;
            let d = (0.15 * font_size - axis_h).max(0.0);
            MathBox::with_dims(3.0 * w_dot + 2.0 * thin, h, d)
        }

        EllipsisKind::Vertical => {
            let w_dot = metrics::width_family('.', Face::Roman, font_size, ctx.font_family);
            let (h_u, d_u) = char_height_depth('.', Face::Roman, ctx.font_family);
            let h_dot = (h_u as f32) * 0.001 * font_size;
            let d_dot = (d_u as f32) * 0.001 * font_size;
            let scale_pt = font_size * 0.1;
            let dy = 4.0 * scale_pt; // Knuth baselineskip = 4pt
            let y_mid = origin_y + axis_h;
            ctx.emit_glyph_run(origin_x, y_mid, Face::Roman, font_size, ".");
            ctx.emit_glyph_run(origin_x, y_mid + dy, Face::Roman, font_size, ".");
            ctx.emit_glyph_run(origin_x, y_mid - dy, Face::Roman, font_size, ".");
            let total_h = axis_h + dy + h_dot;
            let total_d = (dy - axis_h + d_dot).max(0.0);
            MathBox::with_dims(w_dot, total_h, total_d)
        }

        EllipsisKind::Diagonal => {
            let w_dot = metrics::width_family('.', Face::Roman, font_size, ctx.font_family);
            let (h_u, _) = char_height_depth('.', Face::Roman, ctx.font_family);
            let h_dot = (h_u as f32) * 0.001 * font_size;
            let scale_pt = font_size * 0.1;
            let mu = font_size / 18.0;
            let x0 = origin_x + 1.0 * mu;
            let y0 = origin_y + 7.0 * scale_pt;
            let x1 = x0 + w_dot + 2.0 * mu;
            let y1 = origin_y + 4.0 * scale_pt;
            let x2 = x1 + w_dot + 2.0 * mu;
            let y2 = origin_y + 1.0 * scale_pt;
            ctx.emit_glyph_run(x0, y0, Face::Roman, font_size, ".");
            ctx.emit_glyph_run(x1, y1, Face::Roman, font_size, ".");
            ctx.emit_glyph_run(x2, y2, Face::Roman, font_size, ".");
            let total_w = 3.0 * w_dot + 6.0 * mu;
            let total_h = 7.0 * scale_pt + h_dot;
            MathBox::with_dims(total_w, total_h, 0.0)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn layout_overbrace<'a, 'p>(
    ctx: &mut MathLayoutContext<'a, 'p>,
    nucleus: &MathNode<'a>,
    sub: Option<&MathNode<'a>>,
    sup: Option<&MathNode<'a>>,
    style: MathStyle,
    origin_x: f32,
    origin_y: f32,
) -> MathBox {
    let font_size = ctx.font_tier.font_size(style);
    let scale = font_size * 0.001;
    let size_ratio = font_size / 10.0;
    let params = ctx.font_params(style);
    let rule_t = (params.rule_thickness as f32) * scale;
    let nuc_style = MathStyle::Display;

    let nuc_start = ctx.out_ops_len();
    let nuc_box = layout_node(ctx, nucleus, nuc_style, 0.0, 0.0);
    let nuc_end = ctx.out_ops_len();

    let corner_w = if ctx.font_family == metrics::FontFamily::ComputerModern {
        (crate::math::math_tfm::cmex10_metric(0x7A).w as f32) * (font_size * 0.001)
    } else {
        0.45 * font_size
    };
    let (brace_h, gap) = if ctx.font_family == metrics::FontFamily::ComputerModern {
        (
            (crate::math::math_tfm::cmex10_metric(0x7A).h as f32) * (font_size * 0.001),
            3.0 * size_ratio,
        )
    } else {
        (
            0.60 * font_size,
            2.0 * size_ratio,
        )
    };
    let b_geom = crate::math::math_delim::HorizontalBraceEngine::compute_overbrace_metrics(
        nuc_box.w,
        nuc_box.h,
        origin_y,
        corner_w,
        brace_h,
        gap,
    );
    let target_w = b_geom.target_w;
    let nuc_offset_x = b_geom.nuc_offset_x;
    let brace_y = b_geom.brace_y;
    let rule_seg_w = b_geom.rule_seg_w;

    let inner_top_h = nuc_box.h + gap + brace_h + gap;
    let inner_bot_d = nuc_box.d;

    let sup_start = ctx.out_ops_len();
    let sup_box = if let Some(s) = sup {
        layout_node(ctx, s, style.sup_style(), 0.0, 0.0)
    } else {
        MathBox::zero()
    };
    let sup_end = ctx.out_ops_len();

    let sub_start = ctx.out_ops_len();
    let sub_box = if let Some(s) = sub {
        layout_node(ctx, s, style.sub_style(), 0.0, 0.0)
    } else {
        MathBox::zero()
    };
    let sub_end = ctx.out_ops_len();

    let total_w = target_w.max(sup_box.w).max(sub_box.w);
    let brace_shift_x = (total_w - target_w) * 0.5;
    let base_brace_x = origin_x + brace_shift_x;
    let actual_nuc_x = base_brace_x + nuc_offset_x;

    let sup_clearance = ((params.big_op_spacing1 as f32) * scale).max(2.0 * size_ratio);
    let extra_pad = (params.big_op_spacing5 as f32) * scale;
    let sup_y = origin_y + inner_top_h + sup_clearance + sup_box.d;
    let sup_x = origin_x + (total_w - sup_box.w) * 0.5;

    let sub_clearance = ((params.big_op_spacing2 as f32) * scale).max(1.667 * size_ratio);
    let sub_y = origin_y - inner_bot_d - sub_clearance - sub_box.h;
    let sub_x = origin_x + (total_w - sub_box.w) * 0.5;

    if !ctx.is_measuring() {
        let mut drew_otf = false;
        if let Some(otf) = crate::math::fonts::active_opentype_math_font(ctx.font_family) {
            if let Some(res) = crate::math::fonts::OpenTypeHorizKind::OverBrace.resolve_font_horizontal(otf, target_w, font_size) {
                match res {
                    crate::math::fonts::ResolvedHorizontal::Variant { glyph_id, advance } => {
                        let var_w = (advance as f32) * (font_size * 0.001);
                        let brace_x = base_brace_x + (target_w - var_w) * 0.5;
                        ctx.emit_vector_glyph(brace_x, brace_y, font_size, glyph_id.0);
                        drew_otf = true;
                    }
                    crate::math::fonts::ResolvedHorizontal::Assembly { recipe, min_overlap } => {
                        let left_w = (otf.advance_width(recipe.left_glyph).unwrap_or(1002) as f32) * (font_size * 0.001);
                        let mid_w = (otf.advance_width(recipe.mid_glyph).unwrap_or(2003) as f32) * (font_size * 0.001);
                        let right_w = (otf.advance_width(recipe.right_glyph).unwrap_or(1001) as f32) * (font_size * 0.001);
                        let ext_w = (otf.advance_width(recipe.ext_glyph).unwrap_or(994) as f32) * (font_size * 0.001);
                        let overlap_pt = (min_overlap as f32) * (font_size * 0.001);

                        ctx.emit_vector_glyph(base_brace_x, brace_y, font_size, recipe.left_glyph.0);
                        let mid_x = base_brace_x + (target_w - mid_w) * 0.5;
                        ctx.emit_vector_glyph(mid_x, brace_y, font_size, recipe.mid_glyph.0);
                        let right_x = base_brace_x + target_w - right_w;
                        ctx.emit_vector_glyph(right_x, brace_y, font_size, recipe.right_glyph.0);

                        let span_left = (mid_x + overlap_pt) - (base_brace_x + left_w - overlap_pt);
                        if span_left > 0.0 && ext_w > overlap_pt {
                            let step = ext_w - overlap_pt;
                            let n_ext = (span_left / step).ceil().max(1.0) as usize;
                            let actual_step = span_left / n_ext as f32;
                            let start_x = base_brace_x + left_w - overlap_pt;
                            for i in 0..n_ext {
                                ctx.emit_vector_glyph(start_x + (i as f32) * actual_step, brace_y, font_size, recipe.ext_glyph.0);
                            }
                        }

                        let span_right = (right_x + overlap_pt) - (mid_x + mid_w - overlap_pt);
                        if span_right > 0.0 && ext_w > overlap_pt {
                            let step = ext_w - overlap_pt;
                            let n_ext = (span_right / step).ceil().max(1.0) as usize;
                            let actual_step = span_right / n_ext as f32;
                            let start_x = mid_x + mid_w - overlap_pt;
                            for i in 0..n_ext {
                                ctx.emit_vector_glyph(start_x + (i as f32) * actual_step, brace_y, font_size, recipe.ext_glyph.0);
                            }
                        }
                        drew_otf = true;
                    }
                }
            }
        }
        if !drew_otf {
            if ctx.font_family == metrics::FontFamily::ComputerModern {
                let x0 = base_brace_x;
                ctx.emit_glyph_run(x0, brace_y, Face::Extension, font_size, crate::math::math_delim::byte_str(0x7A));
                if rule_seg_w > 0.0 {
                    ctx.emit_rule(x0 + corner_w, brace_y + brace_h - rule_t, rule_seg_w, rule_t);
                }
                let x_mid1 = x0 + corner_w + rule_seg_w;
                ctx.emit_glyph_run(x_mid1, brace_y, Face::Extension, font_size, crate::math::math_delim::byte_str(0x7D));
                let x_mid2 = x_mid1 + corner_w;
                ctx.emit_glyph_run(x_mid2, brace_y, Face::Extension, font_size, crate::math::math_delim::byte_str(0x7C));
                if rule_seg_w > 0.0 {
                    ctx.emit_rule(x_mid2 + corner_w, brace_y + brace_h - rule_t, rule_seg_w, rule_t);
                }
                let x_end = x_mid2 + corner_w + rule_seg_w;
                ctx.emit_glyph_run(x_end, brace_y, Face::Extension, font_size, crate::math::math_delim::byte_str(0x7B));
            } else {
                ctx.emit_rule(base_brace_x, brace_y + brace_h - rule_t, target_w, rule_t);
            }
        }
    }

    ctx.shift_ops_range(nuc_start, nuc_end, actual_nuc_x, origin_y);
    if sup.is_some() {
        ctx.shift_ops_range(sup_start, sup_end, sup_x, sup_y);
    }
    if sub.is_some() {
        ctx.shift_ops_range(sub_start, sub_end, sub_x, sub_y);
    }

    let final_h = if sup.is_some() {
        (sup_y + sup_box.h + extra_pad) - origin_y
    } else {
        inner_top_h
    };
    let final_d = if sub.is_some() {
        (origin_y - sub_y + extra_pad).max(inner_bot_d)
    } else {
        inner_bot_d
    };

    MathBox::with_dims(total_w, final_h, final_d)
}

#[allow(clippy::too_many_arguments)]
fn layout_underbrace<'a, 'p>(
    ctx: &mut MathLayoutContext<'a, 'p>,
    nucleus: &MathNode<'a>,
    sub: Option<&MathNode<'a>>,
    sup: Option<&MathNode<'a>>,
    style: MathStyle,
    origin_x: f32,
    origin_y: f32,
) -> MathBox {
    let font_size = ctx.font_tier.font_size(style);
    let scale = font_size * 0.001;
    let size_ratio = font_size / 10.0;
    let params = ctx.font_params(style);
    let rule_t = (params.rule_thickness as f32) * scale;
    let nuc_style = MathStyle::Display;

    let nuc_start = ctx.out_ops_len();
    let nuc_box = layout_node(ctx, nucleus, nuc_style, 0.0, 0.0);
    let nuc_end = ctx.out_ops_len();

    let corner_w = if ctx.font_family == metrics::FontFamily::ComputerModern {
        (crate::math::math_tfm::cmex10_metric(0x7A).w as f32) * (font_size * 0.001)
    } else {
        0.45 * font_size
    };
    let (brace_h, gap) = if ctx.font_family == metrics::FontFamily::ComputerModern {
        (
            (crate::math::math_tfm::cmex10_metric(0x7A).h as f32) * (font_size * 0.001),
            3.0 * size_ratio,
        )
    } else {
        (
            0.60 * font_size,
            2.0 * size_ratio,
        )
    };
    let b_geom = crate::math::math_delim::HorizontalBraceEngine::compute_underbrace_metrics(
        nuc_box.w,
        nuc_box.d,
        origin_y,
        corner_w,
        brace_h,
        gap,
    );
    let target_w = b_geom.target_w;
    let nuc_offset_x = b_geom.nuc_offset_x;
    let brace_y = b_geom.brace_y;
    let rule_seg_w = b_geom.rule_seg_w;

    let inner_top_h = nuc_box.h;
    let inner_bot_d = nuc_box.d + gap + brace_h + gap;

    let sup_start = ctx.out_ops_len();
    let sup_box = if let Some(s) = sup {
        layout_node(ctx, s, style.sup_style(), 0.0, 0.0)
    } else {
        MathBox::zero()
    };
    let sup_end = ctx.out_ops_len();

    let sub_start = ctx.out_ops_len();
    let sub_box = if let Some(s) = sub {
        layout_node(ctx, s, style.sub_style(), 0.0, 0.0)
    } else {
        MathBox::zero()
    };
    let sub_end = ctx.out_ops_len();

    let total_w = target_w.max(sup_box.w).max(sub_box.w);
    let brace_shift_x = (total_w - target_w) * 0.5;
    let base_brace_x = origin_x + brace_shift_x;
    let actual_nuc_x = base_brace_x + nuc_offset_x;

    let sup_clearance = ((params.big_op_spacing1 as f32) * scale).max(2.0 * size_ratio);
    let extra_pad = (params.big_op_spacing5 as f32) * scale;
    let sup_y = origin_y + inner_top_h + sup_clearance + sup_box.d;
    let sup_x = origin_x + (total_w - sup_box.w) * 0.5;

    let sub_clearance = ((params.big_op_spacing2 as f32) * scale).max(1.667 * size_ratio);
    let sub_y = origin_y - inner_bot_d - sub_clearance - sub_box.h;
    let sub_x = origin_x + (total_w - sub_box.w) * 0.5;

    if !ctx.is_measuring() {
        let mut drew_otf = false;
        if let Some(otf) = crate::math::fonts::active_opentype_math_font(ctx.font_family) {
            if let Some(res) = crate::math::fonts::OpenTypeHorizKind::UnderBrace.resolve_font_horizontal(otf, target_w, font_size) {
                match res {
                    crate::math::fonts::ResolvedHorizontal::Variant { glyph_id, advance } => {
                        let var_w = (advance as f32) * (font_size * 0.001);
                        let brace_x = base_brace_x + (target_w - var_w) * 0.5;
                        ctx.emit_vector_glyph(brace_x, brace_y, font_size, glyph_id.0);
                        drew_otf = true;
                    }
                    crate::math::fonts::ResolvedHorizontal::Assembly { recipe, min_overlap } => {
                        let left_w = (otf.advance_width(recipe.left_glyph).unwrap_or(1002) as f32) * (font_size * 0.001);
                        let mid_w = (otf.advance_width(recipe.mid_glyph).unwrap_or(2003) as f32) * (font_size * 0.001);
                        let right_w = (otf.advance_width(recipe.right_glyph).unwrap_or(1001) as f32) * (font_size * 0.001);
                        let ext_w = (otf.advance_width(recipe.ext_glyph).unwrap_or(994) as f32) * (font_size * 0.001);
                        let overlap_pt = (min_overlap as f32) * (font_size * 0.001);

                        ctx.emit_vector_glyph(base_brace_x, brace_y, font_size, recipe.left_glyph.0);
                        let mid_x = base_brace_x + (target_w - mid_w) * 0.5;
                        ctx.emit_vector_glyph(mid_x, brace_y, font_size, recipe.mid_glyph.0);
                        let right_x = base_brace_x + target_w - right_w;
                        ctx.emit_vector_glyph(right_x, brace_y, font_size, recipe.right_glyph.0);

                        let span_left = (mid_x + overlap_pt) - (base_brace_x + left_w - overlap_pt);
                        if span_left > 0.0 && ext_w > overlap_pt {
                            let step = ext_w - overlap_pt;
                            let n_ext = (span_left / step).ceil().max(1.0) as usize;
                            let actual_step = span_left / n_ext as f32;
                            let start_x = base_brace_x + left_w - overlap_pt;
                            for i in 0..n_ext {
                                ctx.emit_vector_glyph(start_x + (i as f32) * actual_step, brace_y, font_size, recipe.ext_glyph.0);
                            }
                        }

                        let span_right = (right_x + overlap_pt) - (mid_x + mid_w - overlap_pt);
                        if span_right > 0.0 && ext_w > overlap_pt {
                            let step = ext_w - overlap_pt;
                            let n_ext = (span_right / step).ceil().max(1.0) as usize;
                            let actual_step = span_right / n_ext as f32;
                            let start_x = mid_x + mid_w - overlap_pt;
                            for i in 0..n_ext {
                                ctx.emit_vector_glyph(start_x + (i as f32) * actual_step, brace_y, font_size, recipe.ext_glyph.0);
                            }
                        }
                        drew_otf = true;
                    }
                }
            }
        }
        if !drew_otf {
            if ctx.font_family == metrics::FontFamily::ComputerModern {
                let x0 = base_brace_x;
                ctx.emit_glyph_run(x0, brace_y, Face::Extension, font_size, crate::math::math_delim::byte_str(0x7C));
                if rule_seg_w > 0.0 {
                    ctx.emit_rule(x0 + corner_w, brace_y, rule_seg_w, rule_t);
                }
                let x_mid1 = x0 + corner_w + rule_seg_w;
                ctx.emit_glyph_run(x_mid1, brace_y, Face::Extension, font_size, crate::math::math_delim::byte_str(0x7B));
                let x_mid2 = x_mid1 + corner_w;
                ctx.emit_glyph_run(x_mid2, brace_y, Face::Extension, font_size, crate::math::math_delim::byte_str(0x7A));
                if rule_seg_w > 0.0 {
                    ctx.emit_rule(x_mid2 + corner_w, brace_y, rule_seg_w, rule_t);
                }
                let x_end = x_mid2 + corner_w + rule_seg_w;
                ctx.emit_glyph_run(x_end, brace_y, Face::Extension, font_size, crate::math::math_delim::byte_str(0x7D));
            } else {
                ctx.emit_rule(base_brace_x, brace_y, target_w, rule_t);
            }
        }
    }

    ctx.shift_ops_range(nuc_start, nuc_end, actual_nuc_x, origin_y);
    if sup.is_some() {
        ctx.shift_ops_range(sup_start, sup_end, sup_x, sup_y);
    }
    if sub.is_some() {
        ctx.shift_ops_range(sub_start, sub_end, sub_x, sub_y);
    }

    let final_h = if sup.is_some() {
        (sup_y + sup_box.h + extra_pad) - origin_y
    } else {
        inner_top_h
    };
    let final_d = if sub.is_some() {
        (origin_y - sub_y + extra_pad).max(inner_bot_d)
    } else {
        inner_bot_d
    };

    MathBox::with_dims(total_w, final_h, final_d)
}

/// Convenience function to measure a mathematical expression.
pub fn measure_math<'a>(
    node: &MathNode<'a>,
    base_font_size: f32,
    font_family: FontFamily,
    style: MathStyle,
) -> MathBox {
    let mut ctx = MathLayoutContext::new(base_font_size, font_family, None, None, None, style);
    let mut b = layout_node(&mut ctx, node, style, 0.0, 0.0);
    let trailing_ic = node_italic_correction_styled(node, ctx.face_override, ctx.font_family, style)
        * (ctx.font_tier.font_size(style) * 0.001);
    b.w += trailing_ic;
    b
}

/// Convenience function to render a mathematical expression into operations.
#[allow(clippy::too_many_arguments)]
pub fn render_math<'a, 'p>(
    node: &MathNode<'a>,
    base_font_size: f32,
    font_family: FontFamily,
    style: MathStyle,
    origin_x: f32,
    origin_y: f32,
    ops: &'p mut Vec<Paint>,
    pool: &'p mut Vec<&'a str>,
) -> MathBox {
    let mut ctx = MathLayoutContext::new(
        base_font_size,
        font_family,
        None,
        Some(ops),
        Some(pool),
        style,
    );
    let mut b = layout_node(&mut ctx, node, style, origin_x, origin_y);
    let trailing_ic = node_italic_correction_styled(node, ctx.face_override, ctx.font_family, style)
        * (ctx.font_tier.font_size(style) * 0.001);
    b.w += trailing_ic;
    b
}

fn layout_extensible_arrow<'a, 'p>(
    ctx: &mut MathLayoutContext<'a, 'p>,
    data: &crate::math::math_ast::ExtensibleArrowData<'a>,
    style: MathStyle,
    origin_x: f32,
    origin_y: f32,
) -> MathBox {
    let font_size = ctx.font_tier.font_size(style);
    let scale = font_size * 0.001;
    let params = ctx.font_params(style);
    let axis_h = (params.axis_height as f32) * scale;
    let rule_t = (params.rule_thickness as f32) * scale;

    let sub_style = style.sub();
    let sup_style = style.sup();

    let mut sup_box = MathBox::zero();
    let sup_start = ctx.out_ops_len();
    if let Some(ref sup_node) = data.sup {
        sup_box = layout_node(ctx, sup_node, sup_style, 0.0, 0.0);
    }
    let sup_end = ctx.out_ops_len();

    let mut sub_box = MathBox::zero();
    let sub_start = ctx.out_ops_len();
    if let Some(ref sub_node) = data.sub {
        sub_box = layout_node(ctx, sub_node, sub_style, 0.0, 0.0);
    }
    let sub_end = ctx.out_ops_len();

    let arrow_size = font_size;
    let is_left = data.arrow == "leftarrow";
    let is_both = data.arrow == "leftrightarrow";

    let (arrow_w, arrow_y, is_cm) = if ctx.font_family == metrics::FontFamily::ComputerModern {
        let code = if is_left { 0x20u8 } else { 0x21u8 };
        let w = (crate::math::math_tfm::cmsy10_metric(code).w as f32 * 0.001) * arrow_size;
        (w, origin_y, true)
    } else {
        let sym_idx = if is_left { 0xAC } else { 0xAE };
        let w = (metrics::SYMBOL_WIDTHS[sym_idx] as f32 * 0.001) * arrow_size;
        let shaft_y = origin_y + axis_h;
        (w, shaft_y - 0.13 * arrow_size, false)
    };

    let (arrow_geom, left_x, right_x) = crate::math::math_delim::ExtensibleArrowEngine::compute_arrow_geometry(
        origin_x,
        origin_y,
        axis_h,
        sup_box.w,
        sub_box.w,
        arrow_w,
        font_size,
        is_left,
        is_both,
    );
    let target_w = arrow_geom.target_w;
    let shaft_y = arrow_geom.shaft_y;

    if is_cm {
        let left_code = crate::math::math_delim::byte_str(0x20);
        let right_code = crate::math::math_delim::byte_str(0x21);
        if is_both {
            ctx.emit_glyph_run(left_x, arrow_y, Face::Symbol, arrow_size, left_code);
            ctx.emit_glyph_run(right_x, arrow_y, Face::Symbol, arrow_size, right_code);
        } else if is_left {
            ctx.emit_glyph_run(left_x, arrow_y, Face::Symbol, arrow_size, left_code);
        } else {
            ctx.emit_glyph_run(right_x, arrow_y, Face::Symbol, arrow_size, right_code);
        }
        ctx.emit_rule(arrow_geom.rule_start_x, shaft_y, arrow_geom.rule_w, rule_t);
    } else {
        if is_both {
            ctx.emit_glyph_run(left_x, arrow_y, Face::Symbol, arrow_size, "\u{00AC}");
            ctx.emit_glyph_run(right_x, arrow_y, Face::Symbol, arrow_size, "\u{00AE}");
            let rule_start = left_x + arrow_w * 0.75;
            let rule_w = (right_x + arrow_w * 0.25 - rule_start).max(1.0);
            ctx.emit_rule(rule_start, shaft_y, rule_w, rule_t);
        } else if is_left {
            ctx.emit_glyph_run(left_x, arrow_y, Face::Symbol, arrow_size, "\u{00AC}");
            let rule_start = left_x + arrow_w * 0.75;
            let rule_w = (origin_x + target_w - rule_start).max(1.0);
            ctx.emit_rule(rule_start, shaft_y, rule_w, rule_t);
        } else {
            let rule_w = (right_x + arrow_w * 0.25 - origin_x).max(1.0);
            ctx.emit_rule(origin_x, shaft_y, rule_w, rule_t);
            ctx.emit_glyph_run(right_x, arrow_y, Face::Symbol, arrow_size, "\u{00AE}");
        }
    }

    let mut total_h = axis_h + rule_t * 0.5 + 2.0;
    let mut total_d = (2.0 - (axis_h - rule_t * 0.5)).max(0.0);
    let label_clearance = (0.15 * font_size).max(1.5);

    if data.sup.is_some() {
        let sup_x = origin_x + (target_w - sup_box.w) * 0.5;
        let sup_y = shaft_y + rule_t + sup_box.d + label_clearance;
        ctx.shift_ops_range(sup_start, sup_end, sup_x, sup_y);
        total_h = (sup_y + sup_box.h - origin_y).max(total_h);
    }

    if data.sub.is_some() {
        let sub_x = origin_x + (target_w - sub_box.w) * 0.5;
        let sub_y = shaft_y - rule_t - sub_box.h - label_clearance;
        ctx.shift_ops_range(sub_start, sub_end, sub_x, sub_y);
        total_d = (origin_y - (sub_y - sub_box.d)).max(total_d);
    }

    MathBox::with_dims(target_w, total_h, total_d)
}

#[allow(clippy::too_many_arguments)]
fn layout_overbracket<'a, 'p>(
    ctx: &mut MathLayoutContext<'a, 'p>,
    nucleus: &MathNode<'a>,
    sub: Option<&MathNode<'a>>,
    sup: Option<&MathNode<'a>>,
    style: MathStyle,
    origin_x: f32,
    origin_y: f32,
) -> MathBox {
    let font_size = ctx.font_tier.font_size(style);
    let scale = font_size * 0.001;
    let params = ctx.font_params(style);
    let rule_t = (params.rule_thickness as f32) * scale;
    let leg_h = 3.0 * rule_t;

    let nuc_start = ctx.out_ops_len();
    let nuc_box = layout_node(ctx, nucleus, MathStyle::Display, 0.0, 0.0);
    let nuc_end = ctx.out_ops_len();

    let mut sup_box = MathBox::zero();
    let sup_start = ctx.out_ops_len();
    if let Some(sup_node) = sup {
        sup_box = layout_node(ctx, sup_node, style.sup(), 0.0, 0.0);
    }
    let sup_end = ctx.out_ops_len();

    let mut sub_box = MathBox::zero();
    let sub_start = ctx.out_ops_len();
    if let Some(sub_node) = sub {
        sub_box = layout_node(ctx, sub_node, style.sub(), 0.0, 0.0);
    }
    let sub_end = ctx.out_ops_len();

    let target_w = nuc_box.w.max(sup_box.w).max(sub_box.w);
    let nuc_x = origin_x + (target_w - nuc_box.w) * 0.5;
    ctx.shift_ops_range(nuc_start, nuc_end, nuc_x, origin_y);

    let vgap = OverUnderDelimiterEngine::compute_over_shift(
        rule_t,
        leg_h,
        nuc_box.h,
        scale,
        style,
    );
    let bracket_y = origin_y + nuc_box.h + vgap;
    let bracket_x = origin_x;

    ctx.emit_rule(bracket_x, bracket_y, target_w, rule_t);
    ctx.emit_rule(bracket_x, bracket_y - leg_h, rule_t, leg_h);
    ctx.emit_rule(bracket_x + target_w - rule_t, bracket_y - leg_h, rule_t, leg_h);

    let mut total_h = (bracket_y + rule_t - origin_y).max(nuc_box.h);
    let total_d = nuc_box.d;

    if sup.is_some() {
        let sup_x = origin_x + (target_w - sup_box.w) * 0.5;
        let sup_y = bracket_y + rule_t + 1.5;
        ctx.shift_ops_range(sup_start, sup_end, sup_x, sup_y);
        total_h = (sup_y + sup_box.h - origin_y).max(total_h);
    }
    if sub.is_some() {
        let sub_x = origin_x + (target_w - sub_box.w) * 0.5;
        let sub_y = origin_y - nuc_box.d - sub_box.h - 1.5;
        ctx.shift_ops_range(sub_start, sub_end, sub_x, sub_y);
    }

    MathBox::with_dims(target_w, total_h, total_d)
}

#[allow(clippy::too_many_arguments)]
fn layout_underbracket<'a, 'p>(
    ctx: &mut MathLayoutContext<'a, 'p>,
    nucleus: &MathNode<'a>,
    sub: Option<&MathNode<'a>>,
    sup: Option<&MathNode<'a>>,
    style: MathStyle,
    origin_x: f32,
    origin_y: f32,
) -> MathBox {
    let font_size = ctx.font_tier.font_size(style);
    let scale = font_size * 0.001;
    let params = ctx.font_params(style);
    let rule_t = (params.rule_thickness as f32) * scale;
    let leg_h = 3.0 * rule_t;

    let nuc_start = ctx.out_ops_len();
    let nuc_box = layout_node(ctx, nucleus, MathStyle::Display, 0.0, 0.0);
    let nuc_end = ctx.out_ops_len();

    let mut sup_box = MathBox::zero();
    let sup_start = ctx.out_ops_len();
    if let Some(sup_node) = sup {
        sup_box = layout_node(ctx, sup_node, style.sup(), 0.0, 0.0);
    }
    let sup_end = ctx.out_ops_len();

    let mut sub_box = MathBox::zero();
    let sub_start = ctx.out_ops_len();
    if let Some(sub_node) = sub {
        sub_box = layout_node(ctx, sub_node, style.sub(), 0.0, 0.0);
    }
    let sub_end = ctx.out_ops_len();

    let target_w = nuc_box.w.max(sup_box.w).max(sub_box.w);
    let nuc_x = origin_x + (target_w - nuc_box.w) * 0.5;
    ctx.shift_ops_range(nuc_start, nuc_end, nuc_x, origin_y);

    let vgap = OverUnderDelimiterEngine::compute_under_shift(
        rule_t,
        leg_h,
        nuc_box.d,
        scale,
        style,
    );
    let bracket_y = origin_y - nuc_box.d - vgap - rule_t;
    let bracket_x = origin_x;

    ctx.emit_rule(bracket_x, bracket_y, target_w, rule_t);
    ctx.emit_rule(bracket_x, bracket_y, rule_t, leg_h);
    ctx.emit_rule(bracket_x + target_w - rule_t, bracket_y, rule_t, leg_h);

    let total_h = nuc_box.h;
    let mut total_d = (origin_y - bracket_y).max(nuc_box.d);

    if sub.is_some() {
        let sub_x = origin_x + (target_w - sub_box.w) * 0.5;
        let sub_y = bracket_y - sub_box.h - 1.5;
        ctx.shift_ops_range(sub_start, sub_end, sub_x, sub_y);
        total_d = (origin_y - sub_y).max(total_d);
    }
    if sup.is_some() {
        let sup_x = origin_x + (target_w - sup_box.w) * 0.5;
        let sup_y = origin_y + nuc_box.h + 1.5;
        ctx.shift_ops_range(sup_start, sup_end, sup_x, sup_y);
    }

    MathBox::with_dims(target_w, total_h, total_d)
}
