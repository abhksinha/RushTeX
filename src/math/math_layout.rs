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
    MathFontParameters,
};
use crate::math::math_spacing::{atom_spacing, atom_spacing_sp, normalize_atom_kinds, MathAtomKind};

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
        MathNode::Symbol { code: Some(c), face, .. } => {
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
                symbol_italic_correction(*c) as f32
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
        MathNode::Overbrace(_) | MathNode::Underbrace(_) => 0.0,
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
    if delim.is_empty() || delim == "." {
        return (0.0, 0.0);
    }

    let dmetrics = delimiter_metrics(ctx.font_family);
    let delim_t = (rule_t * (dmetrics.thickness_scale as f32 * 0.001)).max(0.60);
    let bar_extra = (dmetrics.bar_extra_h as f32 * 0.001) * font_size;

    if delim == "|" {
        let bar_h = (target_h + bar_extra).max(target_h);
        let bar_y = y_center - bar_h * 0.5;
        let bar_t = (0.040 * font_size).max(0.40);
        ctx.emit_rule(x, bar_y, bar_t, bar_h);
        return (bar_t, bar_h);
    }
    if delim == "\\|" || delim == "||" {
        let bar_h = (target_h + bar_extra).max(target_h);
        let bar_y = y_center - bar_h * 0.5;
        let bar_t = (0.040 * font_size).max(0.40);
        let sep_bars = 2.0 * bar_t;
        ctx.emit_rule(x, bar_y, bar_t, bar_h);
        ctx.emit_rule(x + bar_t + sep_bars, bar_y, bar_t, bar_h);
        return (2.0 * bar_t + sep_bars, bar_h);
    }

    if target_h <= 1.15 * font_size {
        match delim {
            "(" | ")" | "[" | "]" => {
                let ch = delim.chars().next().unwrap();
                let w = metrics::width_family(ch, Face::Roman, font_size, ctx.font_family);
                let glyph_y = y_center - (dmetrics.center_offset_y as f32 * 0.001) * font_size;
                ctx.emit_glyph_run(x, glyph_y, Face::Roman, font_size, delim);
                return (w, font_size);
            }
            _ => {}
        }
    }

    if let Some((chain, recipe_opt)) = crate::math::math_delim::resolve_cmex_delimiter(delim, is_left) {
        let scale = font_size * 0.1;
        let tier = if target_h <= 14.0 * scale {
            0
        } else if target_h <= 18.0 * scale {
            1
        } else if target_h <= 24.0 * scale {
            2
        } else if target_h <= 30.0 * scale {
            3
        } else if recipe_opt.is_none() {
            chain.len() - 1
        } else {
            4
        };

            if tier < chain.len() {
                let code = chain[tier];
                let m = crate::math::math_tfm::cmex10_metric(code);
                let w = (m.w as f32 * 0.001) * font_size;
                let h = (m.h as f32 * 0.001) * font_size;
                let d = (m.d as f32 * 0.001) * font_size;
                let glyph_y = y_center + (d - h) * 0.5;
                let s = crate::math::math_delim::byte_str(code);
                ctx.emit_glyph_run(x, glyph_y, Face::Extension, font_size, s);
                return (w, h + d);
            }

            if let Some(recipe) = recipe_opt {
                let m_rep = crate::math::math_tfm::cmex10_metric(recipe.rep);
                let w = (m_rep.w as f32 * 0.001) * font_size;
                let u = ((m_rep.h + m_rep.d) as f32 * 0.001) * font_size;

                let h_top = if recipe.top != 0 {
                    let m = crate::math::math_tfm::cmex10_metric(recipe.top);
                    ((m.h + m.d) as f32 * 0.001) * font_size
                } else {
                    0.0
                };
                let h_bot = if recipe.bot != 0 {
                    let m = crate::math::math_tfm::cmex10_metric(recipe.bot);
                    ((m.h + m.d) as f32 * 0.001) * font_size
                } else {
                    0.0
                };
                let h_mid = if recipe.mid != 0 {
                    let m = crate::math::math_tfm::cmex10_metric(recipe.mid);
                    ((m.h + m.d) as f32 * 0.001) * font_size
                } else {
                    0.0
                };

                let fixed_h = h_top + h_bot + h_mid;
                let has_mid = recipe.mid != 0;
                let n_rep = if u > 0.0 && target_h > fixed_h {
                    if has_mid {
                        ((target_h - fixed_h) / (2.0 * u)).ceil() as usize
                    } else {
                        ((target_h - fixed_h) / u).ceil() as usize
                    }
                } else {
                    0
                };

                let total_h = if has_mid {
                    fixed_h + (2 * n_rep) as f32 * u
                } else {
                    fixed_h + (n_rep as f32) * u
                };

                let min_overlap_pt = 0.0;

                let mut curr_top_y = y_center + total_h * 0.5;

                let mut emit_piece = |code: u8, top_y: &mut f32| {
                    if code != 0 {
                        let m = crate::math::math_tfm::cmex10_metric(code);
                        let baseline_y = *top_y - (m.h as f32 * 0.001) * font_size;
                        let s = crate::math::math_delim::byte_str(code);
                        ctx.emit_glyph_run(x, baseline_y, Face::Extension, font_size, s);
                        *top_y = baseline_y - (m.d as f32 * 0.001) * font_size + min_overlap_pt;
                    }
                };

                // TeX Appendix G Rule 19 vertical stack:
                // 1. Top piece
                emit_piece(recipe.top, &mut curr_top_y);

                // 2. Upper repeaters
                for _ in 0..n_rep {
                    emit_piece(recipe.rep, &mut curr_top_y);
                }

                // 3. Middle piece (if any)
                emit_piece(recipe.mid, &mut curr_top_y);

                // 4. Lower repeaters (if middle piece exists)
                if has_mid {
                    for _ in 0..n_rep {
                        emit_piece(recipe.rep, &mut curr_top_y);
                    }
                }

                // 5. Bottom piece
                emit_piece(recipe.bot, &mut curr_top_y);

                return (w, total_h);
            }
        }

    match delim {
        "|" => {
            let bar_h = (target_h + bar_extra).max(target_h);
            let bar_y = y_center - bar_h * 0.5;
            ctx.emit_rule(x, bar_y, delim_t, bar_h);
            (delim_t, bar_h)
        }
        "\\|" | "||" => {
            let bar_h = (target_h + bar_extra).max(target_h);
            let bar_y = y_center - bar_h * 0.5;
            let sep_bars = 2.0 * delim_t;
            ctx.emit_rule(x, bar_y, delim_t, bar_h);
            ctx.emit_rule(x + delim_t + sep_bars, bar_y, delim_t, bar_h);
            (2.0 * delim_t + sep_bars, bar_h)
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
                    } else {
                        (*face, *glyph, *code)
                    }
                } else {
                    (*face, *glyph, *code)
                }
            } else if *glyph == "†" || *glyph == "‡" || *glyph == "|" {
                (Face::Roman, *glyph, *code)
            } else {
                (*face, *glyph, *code)
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
                        Face::Symbol => crate::math::math_tfm::cmsy10_metric(c).w as f32 * scale,
                        Face::Symbol7 => crate::math::math_tfm::cmsy7_metric(c).w as f32 * scale,
                        Face::Symbol5 => crate::math::math_tfm::cmsy5_metric(c).w as f32 * scale,
                        _ => crate::math::math_tfm::cmsy10_metric(c).w as f32 * scale,
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
                } else if eff_face == Face::Symbol {
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
                        Face::Symbol => {
                            let m = crate::math::math_tfm::cmsy10_metric(c);
                            (m.h, m.d)
                        }
                        Face::Symbol7 => {
                            let m = crate::math::math_tfm::cmsy7_metric(c);
                            (m.h, m.d)
                        }
                        Face::Symbol5 => {
                            let m = crate::math::math_tfm::cmsy5_metric(c);
                            (m.h, m.d)
                        }
                        _ => {
                            let m = crate::math::math_tfm::cmsy10_metric(c);
                            (m.h, m.d)
                        }
                    }
                } else if eff_face == Face::Symbol {
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

            ctx.emit_glyph_run(origin_x, origin_y, eff_face, font_size, eff_glyph);
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

            let nuc_box = layout_node(ctx, nucleus, style, origin_x, origin_y);

            let mut sup_box = MathBox::zero();
            let mut sub_box = MathBox::zero();

            if let Some(sp) = sup {
                let mut dry_ctx = ctx.dry_context();
                let sup_style = style.sup_style();
                let mut mb = layout_node(&mut dry_ctx, sp, sup_style, 0.0, 0.0);
                let ic = node_italic_correction_styled(sp, dry_ctx.face_override, dry_ctx.font_family, sup_style)
                    * (dry_ctx.font_tier.font_size(sup_style) * 0.001);
                mb.w += ic;
                sup_box = mb;
            }
            if let Some(sb) = sub {
                let mut dry_ctx = ctx.dry_context();
                let sub_style = style.sub_style();
                let mut mb = layout_node(&mut dry_ctx, sb, sub_style, 0.0, 0.0);
                let ic = node_italic_correction_styled(sb, dry_ctx.face_override, dry_ctx.font_family, sub_style)
                    * (dry_ctx.font_tier.font_size(sub_style) * 0.001);
                mb.w += ic;
                sub_box = mb;
            }

            // TeX Appendix G Rule 17: Italic correction is applied to the horizontal
            // placement of the superscript, but not the subscript.
            let ic = node_italic_correction(nucleus, ctx.face_override, ctx.font_family) * scale;
            let math_kern = if let MathNode::Variable(ch) = &**nucleus {
                if ctx.font_family != FontFamily::ComputerModern && ctx.font_family != FontFamily::Times {
                    if let Some(font) = crate::math::fonts::active_math_font(ctx.font_family) {
                        let u = *ch as u32;
                        font.glyph_for_unicode(u)
                            .and_then(|gid| font.glyph_top_right_math_kern(gid.0))
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

            let is_char = match &**nucleus {
                MathNode::Variable(_) | MathNode::Symbol { .. } | MathNode::Number(_) => true,
                MathNode::Row(children) if children.len() == 1 => {
                    matches!(&children[0], MathNode::Variable(_) | MathNode::Symbol { .. } | MathNode::Number(_))
                }
                MathNode::StyleScope { content, .. } | MathNode::StyleChange { content, .. } => {
                    matches!(&**content, MathNode::Variable(_) | MathNode::Symbol { .. } | MathNode::Number(_))
                }
                _ => false,
            };

            // TeX Appendix G Rule 18a: Candidate shifts
            let sup_shift_std = if style.is_cramped() {
                (params.sup3 as f32) * scale
            } else if style.is_display() {
                (params.sup1 as f32) * scale
            } else {
                (params.sup2 as f32) * scale
            };
            let sup_drop = (params.sup_drop as f32) * scale;
            let mut sup_shift = if is_char {
                sup_shift_std
            } else {
                sup_shift_std.max(nuc_box.h - sup_drop)
            };

            let sub_shift_std = if sup.is_some() {
                (params.sub2 as f32) * scale
            } else {
                (params.sub1 as f32) * scale
            };
            let sub_drop = (params.sub_drop as f32) * scale;
            let mut sub_shift = if is_char {
                sub_shift_std
            } else {
                sub_shift_std.max(nuc_box.d + sub_drop)
            };

            // TeX Rule 18c: Superscript elevation constraint (bottom at least 1/4 x-height above baseline)
            if sup.is_some() {
                let min_sup_elevation = 0.25 * x_height + sup_box.d;
                sup_shift = sup_shift.max(min_sup_elevation);
            }

            // TeX Rule 18d: Subscript drop constraint (top cannot exceed 4/5 x-height below baseline)
            if sub.is_some() {
                let min_sub_drop = sub_box.h - 0.8 * x_height;
                if min_sub_drop > 0.0 {
                    sub_shift = sub_shift.max(min_sub_drop);
                }
            }

            // TeX Rule 18e: Clearance constraint between superscript bottom and subscript top
            if sup.is_some() && sub.is_some() {
                let gap = (sup_shift - sup_box.d) - (-sub_shift + sub_box.h);
                let min_clearance = (params.sub_sup_gap as f32) * scale;
                if gap < min_clearance {
                    let shortfall = min_clearance - gap;
                    sub_shift += shortfall;
                    let min_sup_h = 0.8 * x_height + sup_box.d;
                    if sup_shift < min_sup_h {
                        let push = (min_sup_h - sup_shift).min(shortfall);
                        sup_shift += push;
                        sub_shift -= push;
                    }
                }
            }

            if !ctx.is_measuring() {
                if let Some(sp) = sup {
                    layout_node(ctx, sp, style.sup_style(), sup_x, origin_y + sup_shift);
                }
                if let Some(sb) = sub {
                    layout_node(ctx, sb, style.sub_style(), sub_x, origin_y - sub_shift);
                }
            }

            let script_space = if sup.is_some() || sub.is_some() {
                0.5 * (ctx.base_font_size / 10.0)
            } else {
                0.0
            };
            let right_bound = (origin_x + nuc_box.w)
                .max(if sup.is_some() {
                    sup_x + sup_box.w + script_space
                } else {
                    0.0
                })
                .max(if sub.is_some() {
                    sub_x + sub_box.w + script_space
                } else {
                    0.0
                });
            let total_w = right_bound - origin_x;
            let total_h = nuc_box.h.max(if sup.is_some() {
                sup_shift + sup_box.h
            } else {
                0.0
            });
            let total_d = nuc_box.d.max(if sub.is_some() {
                sub_shift + sub_box.d
            } else {
                0.0
            });

            MathBox::with_dims(total_w, total_h, total_d)
        }

        MathNode::Fraction {
            num,
            den,
            has_rule,
            options: _,
        } => {
            let num_style = style.frac_num_style();
            let den_style = style.frac_den_style();

            // 1. Measure numerator and denominator
            let num_box = {
                let mut dry_ctx = ctx.dry_context();
                let mut mb = layout_node(&mut dry_ctx, num, num_style, 0.0, 0.0);
                let ic = node_italic_correction_styled(num, dry_ctx.face_override, dry_ctx.font_family, num_style)
                    * (dry_ctx.font_tier.font_size(num_style) * 0.001);
                mb.w += ic;
                mb
            };
            let den_box = {
                let mut dry_ctx = ctx.dry_context();
                let mut mb = layout_node(&mut dry_ctx, den, den_style, 0.0, 0.0);
                let ic = node_italic_correction_styled(den, dry_ctx.face_override, dry_ctx.font_family, den_style)
                    * (dry_ctx.font_tier.font_size(den_style) * 0.001);
                mb.w += ic;
                mb
            };

            let frac_pad = if *has_rule {
                0.12 * font_size
            } else {
                0.0
            };
            let frac_w = num_box.w.max(den_box.w) + 2.0 * frac_pad;
            let num_x = origin_x + (frac_w - num_box.w) * 0.5;
            let den_x = origin_x + (frac_w - den_box.w) * 0.5;

            let (num_y, den_y) = if *has_rule {
                // TeX Appendix G Rule 10: Fraction with rule
                let num_shift = if style.is_display() {
                    (params.num1 as f32) * scale
                } else {
                    (params.num2 as f32) * scale
                };
                let den_shift = if style.is_display() {
                    (params.denom1 as f32) * scale
                } else {
                    (params.denom2 as f32) * scale
                };
                let min_clr = if style.is_display() {
                    (params.frac_clearance_display as f32) * scale
                } else {
                    (params.frac_clearance_text as f32) * scale
                };

                let actual_num_shift = num_shift.max(axis_h + 0.5 * rule_t + min_clr + num_box.d);
                let actual_den_shift = den_shift.max(0.5 * rule_t + min_clr + den_box.h - axis_h);
                (origin_y + actual_num_shift, origin_y - actual_den_shift)
            } else {
                // TeX Appendix G Rule 10: Generalized fraction without rule (\binom, \atop)
                let num_shift = if style.is_display() {
                    (params.num1 as f32) * scale
                } else {
                    (params.num3 as f32) * scale
                };
                let den_shift = if style.is_display() {
                    (params.denom1 as f32) * scale
                } else {
                    (params.denom2 as f32) * scale
                };
                let min_gap = if style.is_display() {
                    (params.rule_thickness as f32 * 7.0) * scale
                } else {
                    (params.rule_thickness as f32 * 3.0) * scale
                };

                let mut actual_num_shift = num_shift.max(axis_h + num_box.d);
                let mut actual_den_shift = den_shift.max(den_box.h - axis_h);

                let gap = (actual_num_shift - num_box.d) - (-actual_den_shift + den_box.h);
                if gap < min_gap {
                    let shortfall = min_gap - gap;
                    actual_num_shift += shortfall * 0.5;
                    actual_den_shift += shortfall * 0.5;
                }
                (origin_y + actual_num_shift, origin_y - actual_den_shift)
            };

            if !ctx.is_measuring() {
                // Paint numerator
                layout_node(ctx, num, num_style, num_x, num_y);
                // Paint fraction rule if present
                if *has_rule {
                    let rule_y = origin_y + axis_h - 0.5 * rule_t;
                    ctx.emit_rule(origin_x, rule_y, frac_w, rule_t);
                }
                // Paint denominator
                layout_node(ctx, den, den_style, den_x, den_y);
            }

            let total_h = (num_y + num_box.h) - origin_y;
            let total_d = origin_y - (den_y - den_box.d);

            MathBox::with_dims(frac_w, total_h, total_d)
        }

        MathNode::Radical { degree, radicand } => {
            let rad_style = style.cramped();

            // Measure radicand in cramped style (TeX Appendix G Rule 11)
            let rad_box = {
                let mut dry_ctx = ctx.dry_context();
                let mut mb = layout_node(&mut dry_ctx, radicand, rad_style, 0.0, 0.0);
                let ic = node_italic_correction_styled(radicand, dry_ctx.face_override, dry_ctx.font_family, rad_style)
                    * (dry_ctx.font_tier.font_size(rad_style) * 0.001);
                mb.w += ic;
                mb
            };

            // Measure degree in CrampedScriptScript if present (LuaTeX / TeX Appendix G)
            let deg_box = if let Some(deg) = degree {
                let mut dry_ctx = ctx.dry_context();
                layout_node(&mut dry_ctx, deg, MathStyle::CrampedScriptScript, 0.0, 0.0)
            } else {
                MathBox::zero()
            };


            let (_deg_advance, deg_x_offset, rad_origin_x) = if degree.is_some() {
                let c = crate::math::fonts::math_constants_for_family(ctx.font_family);
                let kern_before = (c.radical_kern_before_degree as f32 * 0.001) * font_size;
                let kern_after = (c.radical_kern_after_degree as f32 * 0.001) * font_size;
                let advance = (kern_before + deg_box.w + kern_after).max(0.0);
                (advance, kern_before, origin_x + advance)
            } else {
                (0.0, 0.0, origin_x)
            };

            let c = crate::math::fonts::math_constants_for_family(ctx.font_family);
            let rule_t_cm = (0.040 * font_size).max(0.398);
            let min_clr = if style.is_display() {
                (c.radical_display_style_vertical_gap as f32 * 0.001) * font_size
            } else {
                (c.radical_vertical_gap as f32 * 0.001) * font_size
            };
            let deg_raise_percent = c.radical_degree_bottom_raise_percent;
            let v = rad_box.h + rad_box.d + min_clr + rule_t_cm;

            // 1. Check small radical in cmsy10 first (Family 2, char 0x70)
            let small_m = crate::math::math_tfm::cmsy10_metric(0x70);

            let (rad_w, rad_h, rad_d, is_ext, rad_code, n_rep) = {
                let kind = crate::math::fonts::OpenTypeDelimKind::Radical;
                match kind.select_variant_tier(v, font_size) {
                    Some(0) => {
                        let w = (small_m.w as f32 * 0.001) * font_size;
                        let h = (small_m.h as f32 * 0.001) * font_size;
                        let d = (small_m.d as f32 * 0.001) * font_size;
                        (w, h, d, false, 0x70u8, 0usize)
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
                        (w, h, d, false, code, 0usize)
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
                        (w, h, d, true, 0x74u8, n)
                    }
                }
            };

                // TeX Appendix G Rule 11: Distribute excess height evenly
                let delta = rad_d - (rad_box.h + rad_box.d + min_clr);
                let actual_clr = if delta > 0.0 {
                    min_clr + delta * 0.5
                } else {
                    min_clr
                };

                let rule_bottom_y = origin_y + rad_box.h + actual_clr;
                let rad_y = rule_bottom_y;
                let rad_content_x = rad_origin_x + rad_w;

                if !ctx.is_measuring() {
                    if !is_ext {
                        let s = crate::math::math_delim::byte_str(rad_code);
                        ctx.emit_glyph_run(rad_origin_x, rad_y, Face::Extension, font_size, s);
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

                    if let Some(deg) = degree {
                        let deg_style = MathStyle::CrampedScriptScript;
                        let deg_x = origin_x + deg_x_offset;
                        let rad_bottom_y = rad_y - rad_d;
                        let deg_y = rad_bottom_y + (deg_raise_percent as f32 * 0.01) * (rad_h + rad_d);
                        layout_node(ctx, deg, deg_style, deg_x, deg_y);
                    }

                    // Paint radicand
                    layout_node(ctx, radicand, rad_style, rad_content_x, origin_y);

                    // Overbar rule connecting seamlessly to glyph ledge:
                    let overlap = 0.20;
                    let rule_start = rad_content_x - overlap;
                    let rule_w = (rad_content_x + rad_box.w) - rule_start;
                    ctx.emit_rule(
                        rule_start,
                        rule_bottom_y,
                        rule_w,
                        rule_t_cm,
                    );
                }

                let total_w = rad_content_x + rad_box.w - origin_x;
                let total_h = (rule_bottom_y + rule_t_cm + rule_t_cm) - origin_y;
                let total_d = rad_box.d.max(rad_d - rad_box.h - actual_clr);
                MathBox::with_dims(total_w, total_h, total_d)
        }

        MathNode::BigOp(data) => {
            let BigOpData {
                op,
                sub,
                sup,
                limits,
            } = &**data;
            let is_text_op = matches!(
                *op,
                "lim" | "max" | "min" | "sup" | "inf" | "gcd" | "det" | "liminf" | "limsup" | "Pr"
            );
            let is_integral = matches!(*op, "int" | "iint" | "iiint" | "oint");
            let is_display_limits = *limits && style.is_display();
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
                let code = if *op == "oint" || *op == "oiint" || *op == "oiiint" {
                    if style.is_display() { 0x49 } else { 0x48 }
                } else if style.is_display() {
                    0x5A
                } else {
                    0x52
                };
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
                let offset = if style.is_display() {
                    single_w - (6.0 / 18.0) * font_size
                } else {
                    single_w - (3.0 / 18.0) * font_size
                };
                let w = match *op {
                    "iint" | "oiint" => single_w + offset,
                    "iiint" | "oiiint" => single_w + 2.0 * offset,
                    _ => single_w,
                };
                (Face::Extension, font_size, w, h, d, ic)
            } else {
                let cmex_code = if style.is_display() {
                    match *op {
                        "prod" => 0x59,
                        "coprod" => 0x61,
                        "bigcap" => 0x5C,
                        "bigcup" => 0x5B,
                        "biguplus" => 0x5D,
                        "bigsqcup" => 0x47,
                        "bigodot" => 0x4B,
                        "bigoplus" => 0x4D,
                        "bigotimes" => 0x4F,
                        "bigwedge" => 0x5E,
                        "bigvee" => 0x5F,
                        _ => 0x58, // sum
                    }
                } else {
                    match *op {
                        "prod" => 0x51,
                        "coprod" => 0x60,
                        "bigcap" => 0x54,
                        "bigcup" => 0x53,
                        "biguplus" => 0x55,
                        "bigsqcup" => 0x46,
                        "bigodot" => 0x4A,
                        "bigoplus" => 0x4C,
                        "bigotimes" => 0x4E,
                        "bigwedge" => 0x56,
                        "bigvee" => 0x57,
                        _ => 0x50, // sum
                    }
                };
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
            let delta_y = if is_text_op {
                0.0
            } else {
                axis_h - (op_h - op_d) * 0.5
            };
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
                    let offset = if style.is_display() {
                        single_w - (6.0 / 18.0) * op_size
                    } else {
                        single_w - (3.0 / 18.0) * op_size
                    };
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
                let sub_box = sub
                    .as_ref()
                    .map(|s| {
                        let mut dry_ctx = ctx.dry_context();
                        layout_node(&mut dry_ctx, s, style.sub_style(), 0.0, 0.0)
                    })
                    .unwrap_or_default();

                let sup_box = sup
                    .as_ref()
                    .map(|s| {
                        let mut dry_ctx = ctx.dry_context();
                        layout_node(&mut dry_ctx, s, style.sup_style(), 0.0, 0.0)
                    })
                    .unwrap_or_default();

                let max_w = op_w.max(sub_box.w + half_ic).max(sup_box.w + half_ic);
                let op_x = origin_x + (max_w - op_w) * 0.5;

                let c = crate::math::fonts::math_constants_for_family(ctx.font_family);
                let (sup_clearance, sup_min_elev, sub_clearance, sub_min_drop) = (
                    ((c.upper_limit_gap_min as f32) * 0.001 * font_size).max(2.0),
                    ((c.upper_limit_baseline_rise_min as f32) * 0.001 * font_size).max(3.0),
                    ((c.lower_limit_gap_min as f32) * 0.001 * font_size).max(2.0),
                    ((c.lower_limit_baseline_drop_min as f32) * 0.001 * font_size).max(3.0),
                );

                let sup_bot_y = (op_top_y + sup_clearance).max(origin_y + axis_h + sup_min_elev);
                let sup_y = sup_bot_y + sup_box.d;
                let sup_x = origin_x + (max_w - sup_box.w) * 0.5 + half_ic;

                let sub_top_y = (op_bot_y - sub_clearance).min(origin_y + axis_h - sub_min_drop);
                let sub_y = sub_top_y - sub_box.h;
                let sub_x = origin_x + (max_w - sub_box.w) * 0.5 - half_ic;

                if !ctx.is_measuring() {
                    emit_op(ctx, op_x, op_y);

                    if let Some(sp) = sup {
                        layout_node(ctx, sp, style.sup_style(), sup_x, sup_y);
                    }

                    if let Some(sb) = sub {
                        layout_node(ctx, sb, style.sub_style(), sub_x, sub_y);
                    }
                }

                let op_extra_pad = (params.big_op_spacing5 as f32) * scale;
                let total_h = if sup.is_some() {
                    (sup_y + sup_box.h) - origin_y + op_extra_pad
                } else {
                    op_top_y - origin_y
                };
                let total_d = if sub.is_some() {
                    origin_y - (sub_y - sub_box.d) + op_extra_pad
                } else {
                    origin_y - op_bot_y
                };
                let op_padding = (0.35 * font_size).max(3.5);
                MathBox::with_dims(max_w + op_padding, total_h, total_d)
            } else {
                let mut sup_box = MathBox::zero();
                let mut sub_box = MathBox::zero();

                if let Some(sp) = sup {
                    let mut dry_ctx = ctx.dry_context();
                    sup_box = layout_node(&mut dry_ctx, sp, style.sup_style(), 0.0, 0.0);
                }
                if let Some(sb) = sub {
                    let mut dry_ctx = ctx.dry_context();
                    sub_box = layout_node(&mut dry_ctx, sb, style.sub_style(), 0.0, 0.0);
                }

                let (sup_x, sub_x, mut sup_shift, mut sub_shift) = if is_integral {
                    let (s_shift, b_shift) = if style.is_display() {
                        (
                            (0.95 * font_size).max((params.sup1 as f32) * scale),
                            (0.75 * font_size).max((params.sub1 as f32) * scale),
                        )
                    } else {
                        (
                            (0.55 * font_size).max((params.sup1 as f32) * scale),
                            (0.35 * font_size).max((params.sub1 as f32) * scale),
                        )
                    };

                    let sx = origin_x + op_w + (0.05 * font_size);
                    let bx = if *op == "oint" || *op == "oiint" || *op == "oiiint" {
                        origin_x + op_w + (0.02 * font_size)
                    } else if *op == "iint" || *op == "iiint" {
                        origin_x + op_w - (0.12 * font_size)
                    } else {
                        origin_x + op_w - (0.22 * font_size)
                    };
                    (sx, bx, s_shift, b_shift)
                } else {
                    let sx = origin_x + op_w + op_ic + 1.0;
                    let bx = origin_x + op_w + 1.0;
                    let s_shift = ((params.sup1 as f32) * scale)
                        .max(op_top_y - origin_y - (params.sup_drop as f32) * scale);
                    let b_shift = ((params.sub1 as f32) * scale)
                        .max(origin_y - op_bot_y + (params.sub_drop as f32) * scale);
                    (sx, bx, s_shift, b_shift)
                };

                // TeX Rule 18 clearance check
                if sup.is_some() && sub.is_some() {
                    let gap = (sup_shift - sup_box.d) - (-sub_shift + sub_box.h);
                    let min_clearance = (params.sub_sup_gap as f32) * scale;
                    if gap < min_clearance {
                        let shortfall = min_clearance - gap;
                        sub_shift += shortfall;
                        let x_height = (params.accent_base_height as f32) * scale;
                        let min_sup_h = 0.8 * x_height + sup_box.d;
                        if sup_shift < min_sup_h {
                            let push = (min_sup_h - sup_shift).min(shortfall);
                            sup_shift += push;
                            sub_shift -= push;
                        }
                    }
                }

                if !ctx.is_measuring() {
                    emit_op(ctx, origin_x, op_y);
                    if let Some(sp) = sup {
                        layout_node(ctx, sp, style.sup_style(), sup_x, origin_y + sup_shift);
                    }
                    if let Some(sb) = sub {
                        layout_node(ctx, sb, style.sub_style(), sub_x, origin_y - sub_shift);
                    }
                }

                let right_bound = (origin_x + op_w)
                    .max(if sup.is_some() {
                        sup_x + sup_box.w
                    } else {
                        0.0
                    })
                    .max(if sub.is_some() {
                        sub_x + sub_box.w
                    } else {
                        0.0
                    });
                let total_w = right_bound - origin_x;
                let total_h = (op_top_y - origin_y).max(if sup.is_some() {
                    sup_shift + sup_box.h
                } else {
                    0.0
                });
                let total_d = (origin_y - op_bot_y).max(if sub.is_some() {
                    sub_shift + sub_box.d
                } else {
                    0.0
                });

                MathBox::with_dims(total_w, total_h, total_d)
            }
        }

        MathNode::Delimited(data) => {
            let DelimitedData {
                open,
                close,
                content,
                ..
            } = &**data;
            // Measure content
            let content_box = {
                let mut dry_ctx = ctx.dry_context();
                layout_node(&mut dry_ctx, content, style, 0.0, 0.0)
            };

            let dmetrics = delimiter_metrics(ctx.font_family);
            let bar_extra = (dmetrics.bar_extra_h as f32 * 0.001) * font_size;

            // TeX Appendix G Rule 19: symmetric height about the mathematical axis
            let delta_top = (content_box.h - axis_h).max(0.0);
            let delta_bot = (content_box.d + axis_h).max(0.0);
            let r = delta_top.max(delta_bot);
            let target_h = 2.0 * r + bar_extra.max(1.5);
            let y_center = origin_y + axis_h;

            let sep = ((dmetrics.bracket_cap_w as f32 * 0.001 * 0.65) * font_size).max(1.8);
            let (open_w, open_h) = {
                let mut dry_ctx = ctx.dry_context();
                layout_delimiter(
                    &mut dry_ctx,
                    open,
                    true,
                    0.0,
                    y_center,
                    target_h,
                    font_size,
                    rule_t,
                )
            };
            let open_pad = if open_w > 0.0 { sep } else { 0.0 };

            let content_x = origin_x + open_w + open_pad;
            let close_x = content_x + content_box.w + open_pad;

            if !ctx.is_measuring() {
                if open_w > 0.0 {
                    layout_delimiter(
                        ctx, open, true, origin_x, y_center, target_h, font_size, rule_t,
                    );
                }
                layout_node(ctx, content, style, content_x, origin_y);
            }

            let (close_w, close_h) = layout_delimiter(
                ctx, close, false, close_x, y_center, target_h, font_size, rule_t,
            );

            let total_w = (close_x + close_w) - origin_x;
            let actual_delim_h = open_h.max(close_h).max(target_h);
            let half_h = (axis_h + actual_delim_h * 0.5).max(content_box.h);
            let half_d = (actual_delim_h * 0.5 - axis_h).max(content_box.d);
            MathBox::with_dims(total_w, half_h, half_d)
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
            let mut heap_row_heights;
            let mut heap_row_depths;
            let mut heap_cell_widths;

            let col_widths: &mut [f32] =
                if num_cols <= crate::math::math_matrix::MATRIX_STACK_COLS {
                    &mut grid_buf.col_widths[..num_cols]
                } else {
                    heap_col_widths = vec![0.0f32; num_cols];
                    &mut heap_col_widths[..]
                };

            let row_heights: &mut [f32] =
                if num_rows <= crate::math::math_matrix::MATRIX_STACK_ROWS {
                    &mut grid_buf.row_heights[..num_rows]
                } else {
                    heap_row_heights = vec![0.0f32; num_rows];
                    &mut heap_row_heights[..]
                };

            let row_depths: &mut [f32] =
                if num_rows <= crate::math::math_matrix::MATRIX_STACK_ROWS {
                    &mut grid_buf.row_depths[..num_rows]
                } else {
                    heap_row_depths = vec![0.0f32; num_rows];
                    &mut heap_row_depths[..]
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
            for (r_idx, r) in rows.iter().enumerate() {
                for (col_idx, cell) in r.iter().enumerate() {
                    let mut dry_ctx = ctx.dry_context();
                    let b = layout_node(&mut dry_ctx, cell, cell_style, 0.0, 0.0);
                    cell_widths[r_idx * num_cols + col_idx] = b.w;
                    if b.w > col_widths[col_idx] {
                        col_widths[col_idx] = b.w;
                    }
                    if b.h > row_heights[r_idx] {
                        row_heights[r_idx] = b.h;
                    }
                    if b.d > row_depths[r_idx] {
                        row_depths[r_idx] = b.d;
                    }
                }
                // Enforce minimum row typographic clearance:
                row_heights[r_idx] = row_heights[r_idx].max(min_rh);
                row_depths[r_idx] = row_depths[r_idx].max(min_rd);
            }

            let gap_after_col = |c: usize| -> f32 {
                crate::math::math_matrix::MatrixLayoutMetricsEngine::col_gap(
                    env_kind, c, font_size,
                )
            };

            let multline_gap = 1.0 * font_size; // \multlinegap = 1em = 10pt
            let mut matrix_w: f32 = col_widths.iter().sum::<f32>();
            if env_kind == MatrixEnvKind::Multline {
                if num_rows > 1 {
                    matrix_w += 2.0 * multline_gap;
                }
            } else if env_kind == MatrixEnvKind::Aligned {
                // In TeX aligned, each pair (e.g. col 0+1, 2+3) has \tabskip (1.0em = 10pt) after it
                for c in 0..num_cols {
                    if c % 2 == 1 {
                        matrix_w += 1.0 * font_size;
                    }
                }
            } else {
                for c in 0..num_cols.saturating_sub(1) {
                    matrix_w += gap_after_col(c);
                }
            }

            // Total height from top of row 0 to bottom of row N-1:
            let total_content_h: f32 = (0..num_rows)
                .map(|i| row_heights[i] + row_depths[i])
                .sum::<f32>()
                + (num_rows.saturating_sub(1) as f32) * row_sep;

            // Center the entire matrix block symmetrically across the math axis:
            let top_y = origin_y + axis_h + (total_content_h * 0.5);
            let y_start = top_y - row_heights[0];

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
                let mut dry_ctx = ctx.dry_context();
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
                // Second pass: paint cells aligned to each row's baseline and aligned within each column
                let mut curr_y = y_start;
                for (r_idx, r) in rows.iter().enumerate() {
                    let mut curr_x = cells_start_x;
                    for (col_idx, cell) in r.iter().enumerate() {
                        let col_w = col_widths[col_idx];
                        let cell_w = cell_widths[r_idx * num_cols + col_idx];

                        let offset_x = if env_kind == MatrixEnvKind::Multline {
                            if num_rows == 1 {
                                (matrix_w - cell_w) * 0.5
                            } else if r_idx == 0 {
                                multline_gap
                            } else if r_idx == num_rows - 1 {
                                matrix_w - multline_gap - cell_w
                            } else {
                                (matrix_w - cell_w) * 0.5
                            }
                        } else {
                            let fallback_align = match env_kind {
                                MatrixEnvKind::Aligned => {
                                    if col_idx.is_multiple_of(2) {
                                        ColAlign::Right
                                    } else {
                                        ColAlign::Left
                                    }
                                }
                                MatrixEnvKind::Cases => ColAlign::Left,
                                MatrixEnvKind::Gather => ColAlign::Center,
                                _ => ColAlign::Center,
                            };
                            let align = col_alignments
                                .get(col_idx)
                                .copied()
                                .unwrap_or(fallback_align);
                            match align {
                                ColAlign::Left => 0.0,
                                ColAlign::Center => (col_w - cell_w) * 0.5,
                                ColAlign::Right => col_w - cell_w,
                            }
                        };

                        layout_node(ctx, cell, cell_style, curr_x + offset_x, curr_y);
                        curr_x += col_w + gap_after_col(col_idx);
                    }
                    if r_idx + 1 < num_rows {
                        curr_y -= row_depths[r_idx] + row_sep + row_heights[r_idx + 1];
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
                let rule_y = origin_y - nuc_box.d - 3.0 * rule_t - rule_t;
                ctx.emit_rule(origin_x, rule_y, rule_w, rule_t);
                return MathBox::with_dims(
                    nuc_box.w,
                    nuc_box.h,
                    nuc_box.d + 5.0 * rule_t,
                );
            }

            // TeX Appendix G Rule 12 for Computer Modern
            if ctx.font_family == metrics::FontFamily::ComputerModern {
                let is_wide = matches!(*accent, "widehat" | "widetilde");
                let x_height = (params.accent_base_height as f32) * scale;
                let delta = nuc_box.h.min(x_height);

                if is_wide {
                    let (v0, v1, v2) = if *accent == "widehat" {
                        (0x62u8, 0x63u8, 0x64u8)
                    } else {
                        (0x65u8, 0x66u8, 0x67u8)
                    };
                    let m_v1 = crate::math::math_tfm::cmex10_metric(v1);
                    let w_v1 = (m_v1.w as f32) * (font_size * 0.001);
                    let (code, m) = if nuc_box.w >= w_v1 {
                        let m_v2 = crate::math::math_tfm::cmex10_metric(v2);
                        let w_v2 = (m_v2.w as f32) * (font_size * 0.001);
                        if nuc_box.w >= w_v2 {
                            (v2, m_v2)
                        } else {
                            (v1, m_v1)
                        }
                    } else {
                        (v0, crate::math::math_tfm::cmex10_metric(v0))
                    };

                    let acc_w = (m.w as f32) * (font_size * 0.001);
                    let acc_h = (m.h as f32) * (font_size * 0.001);
                    let acc_y = origin_y + nuc_box.h - delta;
                    let acc_x = origin_x + (nuc_box.w - acc_w) * 0.5 + skew_offset;

                    ctx.emit_glyph_run(
                        acc_x,
                        acc_y,
                        Face::Extension,
                        font_size,
                        crate::math::math_delim::byte_str(code),
                    );

                    return MathBox::with_dims(
                        nuc_box.w,
                        (acc_y + acc_h - origin_y).max(nuc_box.h),
                        nuc_box.d,
                    );
                }

                let cm_accent_info: Option<(Face, u8, f32, f32)> = match *accent {
                    "hat" => {
                        let m = crate::math::math_tfm::cmr10_metric(0x5E);
                        Some((Face::Roman, 0x5E, (m.w as f32) * (font_size * 0.001), (m.h as f32) * (font_size * 0.001)))
                    }
                    "tilde" => {
                        let m = crate::math::math_tfm::cmr10_metric(0x7E);
                        Some((Face::Roman, 0x7E, (m.w as f32) * (font_size * 0.001), (m.h as f32) * (font_size * 0.001)))
                    }
                    "vec" => {
                        let m = crate::math::math_tfm::cmmi10_metric(0x7E);
                        Some((Face::MathItalic, 0x7E, (m.w as f32) * (font_size * 0.001), (m.h as f32) * (font_size * 0.001)))
                    }
                    "acute" => {
                        let m = crate::math::math_tfm::cmr10_metric(0x13);
                        Some((Face::Roman, 0x13, (m.w as f32) * (font_size * 0.001), (m.h as f32) * (font_size * 0.001)))
                    }
                    "grave" => {
                        let m = crate::math::math_tfm::cmr10_metric(0x12);
                        Some((Face::Roman, 0x12, (m.w as f32) * (font_size * 0.001), (m.h as f32) * (font_size * 0.001)))
                    }
                    "check" => {
                        let m = crate::math::math_tfm::cmr10_metric(0x14);
                        Some((Face::Roman, 0x14, (m.w as f32) * (font_size * 0.001), (m.h as f32) * (font_size * 0.001)))
                    }
                    "breve" => {
                        let m = crate::math::math_tfm::cmr10_metric(0x15);
                        Some((Face::Roman, 0x15, (m.w as f32) * (font_size * 0.001), (m.h as f32) * (font_size * 0.001)))
                    }
                    "bar" => {
                        let m = crate::math::math_tfm::cmr10_metric(0x16);
                        Some((Face::Roman, 0x16, (m.w as f32) * (font_size * 0.001), (m.h as f32) * (font_size * 0.001)))
                    }
                    "dot" => {
                        let m = crate::math::math_tfm::cmr10_metric(0x5F);
                        Some((Face::Roman, 0x5F, (m.w as f32) * (font_size * 0.001), (m.h as f32) * (font_size * 0.001)))
                    }
                    "ddot" => {
                        let m = crate::math::math_tfm::cmr10_metric(0x7F);
                        Some((Face::Roman, 0x7F, (m.w as f32) * (font_size * 0.001), (m.h as f32) * (font_size * 0.001)))
                    }
                    "mathring" => {
                        let m = crate::math::math_tfm::cmr10_metric(0x17);
                        Some((Face::Roman, 0x17, (m.w as f32) * (font_size * 0.001), (m.h as f32) * (font_size * 0.001)))
                    }
                    _ => None,
                };

                if let Some((face, code, acc_w, acc_h)) = cm_accent_info {
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
                            let bar_w = (nuc_box.w * (am.width_ratio as f32 * 0.001)).max(3.0);
                            let bar_x = origin_x + (nuc_box.w - bar_w) * 0.5 + ic_offset;
                            let bar_y = origin_y + nuc_box.h + acc_lift;
                            ctx.emit_rule(bar_x, bar_y, bar_w, rule_t);
                            MathBox::with_dims(nuc_box.w, nuc_box.h + acc_extra_h, nuc_box.d)
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
                                        let eff_w = (advance as f32) * (font_size * 0.001);
                                        let acc_x = origin_x + (nuc_box.w - eff_w) * 0.5 + ic_offset;
                                        let acc_y = origin_y + nuc_box.h + acc_lift;
                                        ctx.emit_vector_glyph(acc_x, acc_y, font_size, glyph_id.0);
                                        drew_otf = true;
                                    }
                                    crate::math::fonts::ResolvedHorizontal::Assembly { recipe, min_overlap } => {
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
                    if is_wide {
                        if let Some(kind) = crate::math::fonts::OpenTypeHorizKind::from_name(accent) {
                            if let Some(otf) = crate::math::fonts::active_opentype_math_font(ctx.font_family) {
                                if let Some(crate::math::fonts::ResolvedHorizontal::Variant { glyph_id, advance }) = kind.resolve_font_horizontal(otf, nuc_box.w, font_size) {
                                    let eff_w = (advance as f32) * (font_size * 0.001);
                                    let acc_x = origin_x + (nuc_box.w - eff_w) * 0.5 + ic_offset;
                                    let acc_y = origin_y + nuc_box.h + acc_lift;
                                    ctx.emit_vector_glyph(acc_x, acc_y, font_size, glyph_id.0);
                                    drew_otf = true;
                                }
                            }
                        }
                    }
                    let mut final_extra_h = acc_extra_h;
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
                            (
                                acc_scale * font_size,
                                base_w.min(nuc_box.w + 1.0),
                                acc_extra_h,
                            )
                        };
                        final_extra_h = eff_extra_h;
                        let acc_x = origin_x + (nuc_box.w - eff_w) * 0.5 + ic_offset;
                        let acc_y = origin_y + nuc_box.h + acc_lift;
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

            // Sub-cache-line / 64-byte stack buffers (exactly 1 L1 cache line each)
            let mut stack_kinds = [MathAtomKind::Ord; 64];
            let mut stack_indices = [0u8; 64];
            let mut heap_kinds;
            let mut heap_indices;

            let (atom_kinds, atom_node_indices): (&mut [MathAtomKind], &mut [u8]) = if n <= 64 {
                (&mut stack_kinds[..n], &mut stack_indices[..n])
            } else {
                heap_kinds = vec![MathAtomKind::Ord; n];
                heap_indices = vec![0u8; n];
                (&mut heap_kinds[..], &mut heap_indices[..])
            };

            let mut atom_count = 0usize;
            for (i, child) in nodes.iter().enumerate() {
                if !matches!(child, MathNode::Space(_)) {
                    atom_node_indices[atom_count] = i as u8;
                    atom_kinds[atom_count] = child.atom_kind_in_style(style);
                    atom_count += 1;
                }
            }

            // TeX Appendix G Rule 5 normalization over the mathematical atom sequence
            normalize_atom_kinds(&mut atom_kinds[..atom_count]);

            let mut curr_x = origin_x;
            let mut max_h = 0.0f32;
            let mut max_d = 0.0f32;
            let mut curr_atom_idx = 0usize;

            for child in nodes.iter() {
                if !matches!(child, MathNode::Space(_)) {
                    if curr_atom_idx > 0 {
                        let prev_k = curr_atom_idx - 1;
                        let prev_kind = atom_kinds[prev_k];
                        let curr_kind = atom_kinds[curr_atom_idx];
                        let mut space = if ctx.font_family == metrics::FontFamily::ComputerModern {
                            let quad_sp = crate::math::math_box::ScaledPoints::from_pt(
                                (params.quad as f32) * scale,
                            );
                            atom_spacing_sp(
                                prev_kind,
                                curr_kind,
                                style.is_display_or_text(),
                                quad_sp,
                            )
                            .to_pt()
                        } else {
                            atom_spacing(
                                prev_kind,
                                curr_kind,
                                style.is_display(),
                                style.is_display_or_text(),
                            ) * font_size
                        };

                        let prev_node_idx = atom_node_indices[prev_k] as usize;
                        let prev_node = &nodes[prev_node_idx];
                        let ic = node_italic_correction_styled(
                            prev_node,
                            ctx.face_override,
                            ctx.font_family,
                            style,
                        ) * scale;
                        space += ic;

                        if ctx.font_family == metrics::FontFamily::ComputerModern {
                            if let (MathNode::Variable(c1), MathNode::Variable(c2)) = (prev_node, child) {
                                space += crate::math::math_tfm::cmmi_pair_kern(*c1, *c2, style.tier(), font_size);
                            }
                        }

                        curr_x += space;
                    }
                    curr_atom_idx += 1;
                }

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
            let content_box = {
                let mut dry_ctx = ctx.dry_context();
                layout_node(&mut dry_ctx, content, style, 0.0, 0.0)
            };
            let delta = axis_h - 0.5 * (content_box.h - content_box.d);
            if !ctx.is_measuring() {
                layout_node(ctx, content, style, origin_x, origin_y + delta);
            }
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
            let target_h = (params.axis_height as f32 * scale * 2.0).max(font_size);
            let y_center = origin_y + (params.axis_height as f32 * scale);
            let (w, actual_h) = layout_delimiter(
                ctx, delim, false, origin_x, y_center, target_h, font_size, rule_t,
            );
            MathBox::with_dims(w, actual_h * 0.5, actual_h * 0.5)
        }

        MathNode::Overbrace(inner) => {
            layout_overbrace(ctx, inner, None, None, style, origin_x, origin_y)
        }

        MathNode::Underbrace(inner) => {
            layout_underbrace(ctx, inner, None, None, style, origin_x, origin_y)
        }

        MathNode::Coerce { content, .. } => {
            layout_node(ctx, content, style, origin_x, origin_y)
        }
    };
    ctx.style_stack.pop();
    result
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
    let nuc_style = MathStyle::Display;

    let nuc_box = {
        let mut dry_ctx = ctx.dry_context();
        layout_node(&mut dry_ctx, nucleus, nuc_style, 0.0, 0.0)
    };

    let corner_w = if ctx.font_family == metrics::FontFamily::ComputerModern {
        (crate::math::math_tfm::cmex10_metric(0x7A).w as f32) * (font_size * 0.001)
    } else {
        0.45 * font_size
    };
    let brace_h = if ctx.font_family == metrics::FontFamily::ComputerModern {
        (crate::math::math_tfm::cmex10_metric(0x7A).h as f32) * (font_size * 0.001)
    } else {
        0.12 * font_size
    };
    let min_brace_w = 4.0 * corner_w;
    let target_w = nuc_box.w.max(min_brace_w);
    let nuc_offset_x = (target_w - nuc_box.w) * 0.5;

    let kern3 = 3.0 * size_ratio;
    let brace_y = origin_y + nuc_box.h + kern3;
    let rule_seg_w = ((target_w - min_brace_w) * 0.5).max(0.0);

    let inner_top_h = nuc_box.h + kern3 + brace_h + kern3;
    let inner_bot_d = nuc_box.d;

    let sup_box = sup.map(|s| {
        let mut dry = ctx.dry_context();
        layout_node(&mut dry, s, style.sup_style(), 0.0, 0.0)
    }).unwrap_or_default();
    let sub_box = sub.map(|s| {
        let mut dry = ctx.dry_context();
        layout_node(&mut dry, s, style.sub_style(), 0.0, 0.0)
    }).unwrap_or_default();

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
        layout_node(ctx, nucleus, nuc_style, actual_nuc_x, origin_y);

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
                    ctx.emit_rule(x0 + corner_w, brace_y, rule_seg_w, brace_h);
                }
                let x_mid1 = x0 + corner_w + rule_seg_w;
                ctx.emit_glyph_run(x_mid1, brace_y, Face::Extension, font_size, crate::math::math_delim::byte_str(0x7D));
                let x_mid2 = x_mid1 + corner_w;
                ctx.emit_glyph_run(x_mid2, brace_y, Face::Extension, font_size, crate::math::math_delim::byte_str(0x7C));
                if rule_seg_w > 0.0 {
                    ctx.emit_rule(x_mid2 + corner_w, brace_y, rule_seg_w, brace_h);
                }
                let x_end = x_mid2 + corner_w + rule_seg_w;
                ctx.emit_glyph_run(x_end, brace_y, Face::Extension, font_size, crate::math::math_delim::byte_str(0x7B));
            } else {
                ctx.emit_rule(base_brace_x, brace_y, target_w, brace_h);
            }
        }

        if let Some(s) = sup {
            layout_node(ctx, s, style.sup_style(), sup_x, sup_y);
        }
        if let Some(s) = sub {
            layout_node(ctx, s, style.sub_style(), sub_x, sub_y);
        }
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
    let nuc_style = MathStyle::Display;

    let nuc_box = {
        let mut dry_ctx = ctx.dry_context();
        layout_node(&mut dry_ctx, nucleus, nuc_style, 0.0, 0.0)
    };

    let corner_w = if ctx.font_family == metrics::FontFamily::ComputerModern {
        (crate::math::math_tfm::cmex10_metric(0x7A).w as f32) * (font_size * 0.001)
    } else {
        0.45 * font_size
    };
    let brace_h = if ctx.font_family == metrics::FontFamily::ComputerModern {
        (crate::math::math_tfm::cmex10_metric(0x7A).h as f32) * (font_size * 0.001)
    } else {
        0.12 * font_size
    };
    let min_brace_w = 4.0 * corner_w;
    let target_w = nuc_box.w.max(min_brace_w);
    let nuc_offset_x = (target_w - nuc_box.w) * 0.5;

    let kern3 = 3.0 * size_ratio;
    let brace_y = origin_y - nuc_box.d - kern3 - brace_h;
    let rule_seg_w = ((target_w - min_brace_w) * 0.5).max(0.0);

    let inner_top_h = nuc_box.h;
    let inner_bot_d = nuc_box.d + kern3 + brace_h + kern3;

    let sup_box = sup.map(|s| {
        let mut dry = ctx.dry_context();
        layout_node(&mut dry, s, style.sup_style(), 0.0, 0.0)
    }).unwrap_or_default();
    let sub_box = sub.map(|s| {
        let mut dry = ctx.dry_context();
        layout_node(&mut dry, s, style.sub_style(), 0.0, 0.0)
    }).unwrap_or_default();

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
        layout_node(ctx, nucleus, nuc_style, actual_nuc_x, origin_y);

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
                    ctx.emit_rule(x0 + corner_w, brace_y, rule_seg_w, brace_h);
                }
                let x_mid1 = x0 + corner_w + rule_seg_w;
                ctx.emit_glyph_run(x_mid1, brace_y, Face::Extension, font_size, crate::math::math_delim::byte_str(0x7B));
                let x_mid2 = x_mid1 + corner_w;
                ctx.emit_glyph_run(x_mid2, brace_y, Face::Extension, font_size, crate::math::math_delim::byte_str(0x7A));
                if rule_seg_w > 0.0 {
                    ctx.emit_rule(x_mid2 + corner_w, brace_y, rule_seg_w, brace_h);
                }
                let x_end = x_mid2 + corner_w + rule_seg_w;
                ctx.emit_glyph_run(x_end, brace_y, Face::Extension, font_size, crate::math::math_delim::byte_str(0x7D));
            } else {
                ctx.emit_rule(base_brace_x, brace_y, target_w, brace_h);
            }
        }

        if let Some(s) = sup {
            layout_node(ctx, s, style.sup_style(), sup_x, sup_y);
        }
        if let Some(s) = sub {
            layout_node(ctx, s, style.sub_style(), sub_x, sub_y);
        }
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
