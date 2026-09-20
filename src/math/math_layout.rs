//! High-performance mathematical vector layout engine.
//!
//! Implements TeX Appendix G rules for math layout, including atom spacing,
//! subscripts/superscripts, vertical fractions, radicals, big operators,
//! extensible delimiters, and matrices.

use crate::backend::fonts::{self as metrics, Face, FontFamily};
use crate::layout::engine::Paint;
use crate::math::math_ast::{
    BigOpData, ColAlign, DelimitedData, FontVariantTier, MathContextFlags, MathNode, MathStyle,
    MatrixData, MatrixDelim, StyleFrameStack,
};
use crate::math::math_box::MathBox;
use crate::math::math_fonts::{
    big_op_metrics, char_height_depth, delimiter_metrics, glyph_top_accent_offset, hbar_metrics,
    italic_correction, lookup_accent_metrics, math_font_parameters, matrix_metrics, measure_char,
    radical_metrics, symbol_height_depth, symbol_italic_correction, MathFontParameters,
};
use crate::math::math_spacing::{atom_spacing, normalize_atom_kinds, MathAtomKind};

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
    pub fn font_params(&self) -> &'static MathFontParameters {
        math_font_parameters(self.font_family)
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
}

/// Resolves the italic correction in 1000-units-per-em for any mathematical nucleus.
pub fn node_italic_correction(
    node: &MathNode,
    face_override: Option<Face>,
    family: FontFamily,
) -> f32 {
    match node {
        MathNode::Variable(ch) => {
            let face = face_override.unwrap_or(Face::Italic);
            italic_correction(*ch, face, family) as f32
        }
        MathNode::Symbol { code: Some(c), .. } => symbol_italic_correction(*c) as f32,
        MathNode::Symbol { code: None, .. } => 0.0,
        MathNode::SubSup { nucleus, sup, .. } => {
            if sup.is_some() {
                0.0
            } else {
                node_italic_correction(nucleus, face_override, family)
            }
        }
        MathNode::StyleChange { face, content } => {
            node_italic_correction(content, Some(*face), family)
        }
        MathNode::StyleScope { content, .. } => {
            node_italic_correction(content, face_override, family)
        }
        MathNode::Row(nodes) => {
            for n in nodes.iter().rev() {
                if !matches!(n, MathNode::Space(_)) {
                    return node_italic_correction(n, face_override, family);
                }
            }
            0.0
        }
        MathNode::BigOp(data) => {
            if matches!(data.op, "int" | "iint" | "iiint" | "oint") {
                symbol_italic_correction(0xF2) as f32
            } else {
                0.0
            }
        }
        MathNode::Accent { nucleus, .. } => node_italic_correction(nucleus, face_override, family),
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
) -> f32 {
    if delim.is_empty() || delim == "." {
        return 0.0;
    }

    let dmetrics = delimiter_metrics(ctx.font_family);
    let delim_t = (rule_t * (dmetrics.thickness_scale as f32 * 0.001)).max(0.60);
    let bar_extra = (dmetrics.bar_extra_h as f32 * 0.001) * font_size;

    match delim {
        "|" => {
            let bar_h = (target_h + bar_extra).max(target_h);
            let bar_y = y_center - bar_h * 0.5;
            ctx.emit_rule(x, bar_y, delim_t, bar_h);
            delim_t
        }
        "\\|" | "||" => {
            let bar_h = (target_h + bar_extra).max(target_h);
            let bar_y = y_center - bar_h * 0.5;
            let sep_bars = 2.0 * delim_t;
            ctx.emit_rule(x, bar_y, delim_t, bar_h);
            ctx.emit_rule(x + delim_t + sep_bars, bar_y, delim_t, bar_h);
            2.0 * delim_t + sep_bars
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
            cap_w
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
            cap_w
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
            cap_w
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
            w
        }
        "(" | ")" => {
            let is_open = is_left || delim == "(";
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
                let glyph = if is_open { "(" } else { ")" };
                let ch = if is_open { '(' } else { ')' };
                let w = metrics::width_family(ch, Face::Symbol, delim_size, ctx.font_family);
                let glyph_y = y_center - (dmetrics.center_offset_y as f32 * 0.001) * delim_size;
                ctx.emit_glyph_run(x, glyph_y, Face::Symbol, delim_size, glyph);
                w
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

                metrics::width_family('\u{00E6}', Face::Symbol, piece_size, ctx.font_family)
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
                w
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

                metrics::width_family('\u{00EC}', Face::Symbol, piece_size, ctx.font_family)
            }
        }
        _ => {
            let delim_scale = (target_h / (1.05 * font_size)).clamp(1.0, 2.5);
            let delim_size = font_size * delim_scale;
            let ch = delim.chars().next().unwrap_or('(');
            let w = metrics::width_family(ch, Face::Roman, delim_size, ctx.font_family);
            let glyph_y = y_center - 0.28 * delim_size;
            ctx.emit_glyph_run(x, glyph_y, Face::Roman, delim_size, delim);
            w
        }
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
    let params = ctx.font_params();
    let font_size = ctx.font_tier.font_size(style);
    let scale = font_size * 0.001;
    let axis_h = (params.axis_height as f32) * scale;
    let rule_t = ((params.rule_thickness as f32) * scale).max(0.5);

    ctx.style_stack.push(style);
    let result = match node {
        MathNode::Variable(ch) => {
            let face = ctx.face_override.unwrap_or(Face::Italic);
            let w = measure_char(*ch, face, font_size, ctx.font_family);
            let (h_u, d_u) = char_height_depth(*ch, face, ctx.font_family);
            let h = (h_u as f32) * scale;
            let d = (d_u as f32) * scale;

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

            ctx.emit_glyph_run(origin_x, origin_y, face, font_size, s);
            MathBox::with_dims(w, h, d)
        }

        MathNode::Number(num_str) => {
            let face = ctx.face_override.unwrap_or(Face::Roman);
            let mut w = 0.0f32;
            for b in num_str.bytes() {
                w += metrics::width_family(b as char, face, font_size, ctx.font_family);
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
            let (w, h, d) = if let Some(c) = code {
                let w = metrics::SYMBOL_WIDTHS[*c as usize] as f32 * scale;
                let (h_u, d_u) = symbol_height_depth(*c);
                (w, (h_u as f32) * scale, (d_u as f32) * scale)
            } else {
                let mut total_w = 0.0f32;
                for ch in glyph.chars() {
                    total_w += measure_char(ch, *face, font_size, ctx.font_family);
                }
                let first = glyph.chars().next().unwrap_or('x');
                let (h_u, d_u) = char_height_depth(first, *face, ctx.font_family);
                (total_w, (h_u as f32) * scale, (d_u as f32) * scale)
            };

            ctx.emit_glyph_run(origin_x, origin_y, *face, font_size, glyph);
            MathBox::with_dims(w, h, d)
        }

        MathNode::SubSup { nucleus, sub, sup } => {
            let nuc_box = layout_node(ctx, nucleus, style, origin_x, origin_y);

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

            // TeX Appendix G Rule 17: Italic correction is applied to the horizontal
            // placement of the superscript, but not the subscript.
            let ic = node_italic_correction(nucleus, ctx.face_override, ctx.font_family) * scale;
            let sup_x = origin_x + nuc_box.w + ic;
            let sub_x = origin_x + nuc_box.w;

            let x_height = (params.accent_base_height as f32) * scale;

            // TeX Appendix G Rule 18a: Candidate shifts
            let sup_shift_std = if style.is_cramped() {
                (params.sup3 as f32) * scale
            } else if style.is_display() {
                (params.sup1 as f32) * scale
            } else {
                (params.sup2 as f32) * scale
            };
            let sup_drop = (params.sup_drop as f32) * scale;
            let mut sup_shift = sup_shift_std.max(nuc_box.h - sup_drop);

            let sub_shift_std = if sup.is_some() {
                (params.sub2 as f32) * scale
            } else {
                (params.sub1 as f32) * scale
            };
            let sub_drop = (params.sub_drop as f32) * scale;
            let mut sub_shift = sub_shift_std.max(nuc_box.d + sub_drop);

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

            let right_bound = (origin_x + nuc_box.w)
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

        MathNode::Fraction { num, den, has_rule } => {
            let num_style = style.frac_num_style();
            let den_style = style.frac_den_style();

            // 1. Measure numerator and denominator
            let num_box = {
                let mut dry_ctx = ctx.dry_context();
                layout_node(&mut dry_ctx, num, num_style, 0.0, 0.0)
            };
            let den_box = {
                let mut dry_ctx = ctx.dry_context();
                layout_node(&mut dry_ctx, den, den_style, 0.0, 0.0)
            };

            let frac_pad = if *has_rule {
                ((params.rule_thickness as f32 * 1.5) * scale).max(1.2 * scale)
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
                layout_node(&mut dry_ctx, radicand, rad_style, 0.0, 0.0)
            };

            let min_clearance = if style.is_display() {
                (params.rad_clearance_display as f32) * scale
            } else {
                (params.rad_clearance_text as f32) * scale
            };
            let rad_m = radical_metrics(ctx.font_family);
            let vspan_ratio = rad_m.vspan as f32 * 0.001;
            let bar_thickness = ((rad_m.overbar_thickness as f32 * 0.001) * font_size).max(rule_t);
            let min_needed_h = rad_box.h + rad_box.d + min_clearance + bar_thickness;

            let h_limit_single_glyph = 1.85 * font_size;
            let is_extensible = min_needed_h > h_limit_single_glyph;

            let (rad_size, rule_x, rad_content_x, bar_thickness, target_bottom_y, rule_bottom_y) =
                if !is_extensible {
                    let size = (min_needed_h / vspan_ratio).max(font_size);
                    let actual_vspan = size * vspan_ratio;
                    let actual_b_thick =
                        ((rad_m.overbar_thickness as f32 * 0.001) * size).max(rule_t);

                    // TeX Appendix G Rule 11: Distribute excess height evenly so radicand is vertically centered
                    let excess = (actual_vspan - min_needed_h).max(0.0);
                    let eff_clearance = min_clearance + excess * 0.5;

                    let r_bottom_y = origin_y + rad_box.h + eff_clearance;
                    let t_bottom_y = r_bottom_y - actual_vspan;

                    let rx = origin_x + (rad_m.rule_start_x as f32 * 0.001) * size;
                    let cx = origin_x + (rad_m.content_start_x as f32 * 0.001) * size + 1.0;

                    (size, rx, cx, actual_b_thick, t_bottom_y, r_bottom_y)
                } else {
                    let hook_size = font_size * 1.25;
                    let rx = origin_x + (rad_m.rule_start_x as f32 * 0.001) * hook_size;
                    let cx = origin_x + (rad_m.content_start_x as f32 * 0.001) * hook_size + 1.0;

                    let r_bottom_y = origin_y + rad_box.h + min_clearance;
                    let vertex_depth = (rad_m.vertex_depth as f32 * 0.001) * font_size;
                    let t_bottom_y = origin_y - rad_box.d - vertex_depth;

                    (hook_size, rx, cx, rule_t, t_bottom_y, r_bottom_y)
                };

            let rule_w = (rad_content_x + rad_box.w) - rule_x + 1.0;
            let rad_y = if !is_extensible {
                rule_bottom_y - (rad_m.junction_y as f32 * 0.001) * rad_size
            } else {
                target_bottom_y + (rad_m.vertex_depth as f32 * 0.001) * rad_size
            };

            if !ctx.is_measuring() {
                ctx.emit_glyph_run(origin_x, rad_y, Face::Symbol, rad_size, "\u{00D6}");

                if is_extensible {
                    // Vertical connecting rule from hook diagonal to overbar
                    let stem_y_start = rad_y + (rad_m.junction_y as f32 * 0.001) * rad_size;
                    let stem_h = (rule_bottom_y + bar_thickness) - stem_y_start;
                    if stem_h > 0.0 {
                        ctx.emit_rule(
                            rule_x - bar_thickness * 0.5,
                            stem_y_start,
                            bar_thickness,
                            stem_h,
                        );
                    }
                }

                if let Some(deg) = degree {
                    let deg_style = MathStyle::CrampedScriptScript;
                    let deg_x = origin_x + 0.10 * rad_size;
                    let deg_y = origin_y + (rad_box.h * 0.5);
                    layout_node(ctx, deg, deg_style, deg_x.max(origin_x), deg_y);
                }

                // Paint radicand
                layout_node(ctx, radicand, rad_style, rad_content_x, origin_y);

                // Horizontal overbar rule connects seamlessly to the glyph / vertical stem:
                ctx.emit_rule(
                    rule_x - bar_thickness * 0.5,
                    rule_bottom_y,
                    rule_w + bar_thickness * 0.5,
                    bar_thickness,
                );
            }

            let total_w = (rule_x + rule_w) - origin_x + 1.0;
            let total_h = (rule_bottom_y + bar_thickness) - origin_y;
            let total_d = origin_y - target_bottom_y;

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
            let size = if style.is_display() {
                font_size * (op_m.display_scale as f32 * 0.001)
            } else {
                font_size
            };
            let op_h = (op_m.text_height as f32 * 0.001) * size;
            let op_d = (op_m.text_depth as f32 * 0.001) * size;

            // TeX Appendix G Rule 13: shift operator vertically so center is at math axis
            let delta_y = if is_text_op {
                0.0
            } else {
                axis_h - (op_h - op_d) * 0.5
            };
            let op_y = origin_y + delta_y;
            let op_top_y = op_y + op_h;
            let op_bot_y = op_y - op_d;

            let (op_face, op_size, op_w) = if is_text_op {
                let w = metrics::string_width_family(op, Face::Roman, size, ctx.font_family);
                (Face::Roman, size, w)
            } else if is_integral {
                let base_w = metrics::SYMBOL_WIDTHS[0xF2] as f32 * (size * 0.001);
                let w = if *op == "iint" {
                    base_w * 1.45
                } else if *op == "iiint" {
                    base_w * 1.90
                } else {
                    base_w
                };
                (Face::Symbol, size, w)
            } else {
                let code = match *op {
                    "prod" | "coprod" => 0xD5,
                    "bigcap" => 0xC7,
                    "bigcup" => 0xC8,
                    "bigvee" => 0xDA,
                    "bigwedge" => 0xD9,
                    "bigoplus" => 0xC4,
                    "bigotimes" => 0xC5,
                    "bigodot" => 0xC4,
                    "biguplus" | "bigsqcup" => 0xC8,
                    _ => 0xE5,
                };
                let w = metrics::SYMBOL_WIDTHS[code as usize] as f32 * (size * 0.001);
                (Face::Symbol, size, w)
            };

            let op_ic = if is_integral {
                (symbol_italic_correction(0xF2) as f32) * (op_size * 0.001)
            } else {
                0.0
            };
            let half_ic = 0.5 * op_ic;

            let emit_op = |ctx: &mut MathLayoutContext<'a, 'p>, x: f32, y: f32| {
                if is_text_op {
                    ctx.emit_glyph_run(x, y, op_face, op_size, op);
                } else if is_integral {
                    let single_w = metrics::SYMBOL_WIDTHS[0xF2] as f32 * (op_size * 0.001);
                    if *op == "oint" {
                        ctx.emit_glyph_run(x, y, Face::Symbol, op_size, "\u{00F2}");
                        let ring_size = op_size * 0.65;
                        let ring_x = x + single_w * 0.18;
                        let ring_y = y + (op_h - op_d) * 0.45;
                        ctx.emit_glyph_run(ring_x, ring_y, Face::Symbol, ring_size, "\u{00B0}");
                    } else if *op == "iint" {
                        ctx.emit_glyph_run(x, y, Face::Symbol, op_size, "\u{00F2}");
                        ctx.emit_glyph_run(
                            x + single_w * 0.45,
                            y,
                            Face::Symbol,
                            op_size,
                            "\u{00F2}",
                        );
                    } else if *op == "iiint" {
                        ctx.emit_glyph_run(x, y, Face::Symbol, op_size, "\u{00F2}");
                        ctx.emit_glyph_run(
                            x + single_w * 0.45,
                            y,
                            Face::Symbol,
                            op_size,
                            "\u{00F2}",
                        );
                        ctx.emit_glyph_run(
                            x + single_w * 0.90,
                            y,
                            Face::Symbol,
                            op_size,
                            "\u{00F2}",
                        );
                    } else {
                        ctx.emit_glyph_run(x, y, Face::Symbol, op_size, "\u{00F2}");
                    }
                } else {
                    let glyph_str = match *op {
                        "prod" | "coprod" => "\u{00D5}",
                        "bigcap" => "\u{00C7}",
                        "bigcup" => "\u{00C8}",
                        "bigvee" => "\u{00DA}",
                        "bigwedge" => "\u{00D9}",
                        "bigoplus" => "\u{00C4}",
                        "bigotimes" => "\u{00C5}",
                        "bigodot" => "\u{00C4}",
                        "biguplus" | "bigsqcup" => "\u{00C8}",
                        _ => "\u{00E5}",
                    };
                    ctx.emit_glyph_run(x, y, Face::Symbol, op_size, glyph_str);
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

                // TeX Appendix G Rule 13a: upper and lower clearances
                let sup_clearance = ((params.big_op_spacing1 as f32) * scale).max(2.0);
                let sup_min_elev = ((params.big_op_spacing3 as f32) * scale).max(3.0);
                let sub_clearance = ((params.big_op_spacing2 as f32) * scale).max(2.0);
                let sub_min_drop = ((params.big_op_spacing4 as f32) * scale).max(3.0);

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
                    // Slanted integral limits: upper limit staggered right, lower limit tucked under
                    let sx = origin_x + op_w + op_ic + (op_m.sup_kern as f32 * 0.001) * op_size;
                    let bx = (origin_x + op_w + (op_m.sub_kern as f32 * 0.001) * op_size)
                        .max(origin_x + 0.15 * op_w);
                    let s_shift = ((op_m.sup_min_shift as f32 * 0.001) * op_size)
                        .max(0.40 * op_size + sup_box.d);
                    let b_shift = ((op_m.sub_min_shift as f32 * 0.001) * op_size)
                        .max(0.30 * op_size + sub_box.h);
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
                let total_w = right_bound - origin_x + 1.0;
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
            let open_w = {
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

            let close_w = layout_delimiter(
                ctx, close, false, close_x, y_center, target_h, font_size, rule_t,
            );

            let total_w = (close_x + close_w) - origin_x;
            let half_h = (axis_h + target_h * 0.5).max(content_box.h);
            let half_d = (target_h * 0.5 - axis_h).max(content_box.d);
            MathBox::with_dims(total_w, half_h, half_d)
        }

        MathNode::Matrix(data) => {
            let MatrixData {
                rows,
                delim,
                col_alignments,
            } = &**data;
            let delim = *delim;
            if rows.is_empty() {
                return MathBox::zero();
            }

            let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
            if num_cols == 0 {
                return MathBox::zero();
            }

            let num_rows = rows.len();
            let total_cells = num_rows * num_cols;

            let mut stack_col_widths = [0.0f32; 16];
            let mut stack_row_heights = [0.0f32; 16];
            let mut stack_row_depths = [0.0f32; 16];
            let mut stack_cell_widths = [0.0f32; 64];

            let mut heap_col_widths;
            let mut heap_row_heights;
            let mut heap_row_depths;
            let mut heap_cell_widths;

            let col_widths: &mut [f32] = if num_cols <= 16 {
                &mut stack_col_widths[..num_cols]
            } else {
                heap_col_widths = vec![0.0f32; num_cols];
                &mut heap_col_widths[..]
            };

            let row_heights: &mut [f32] = if num_rows <= 16 {
                &mut stack_row_heights[..num_rows]
            } else {
                heap_row_heights = vec![0.0f32; num_rows];
                &mut heap_row_heights[..]
            };

            let row_depths: &mut [f32] = if num_rows <= 16 {
                &mut stack_row_depths[..num_rows]
            } else {
                heap_row_depths = vec![0.0f32; num_rows];
                &mut heap_row_depths[..]
            };

            let cell_widths: &mut [f32] = if total_cells <= 64 {
                &mut stack_cell_widths[..total_cells]
            } else {
                heap_cell_widths = vec![0.0f32; total_cells];
                &mut heap_cell_widths[..]
            };

            let m_metrics = matrix_metrics(ctx.font_family);
            let min_rh = (m_metrics.min_row_height as f32 * 0.001) * font_size;
            let min_rd = (m_metrics.min_row_depth as f32 * 0.001) * font_size;
            let row_sep = (m_metrics.row_sep as f32 * 0.001) * font_size;
            let cell_style = style.matrix_cell_style();

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

            let col_gap = if delim == MatrixDelim::LeftBrace {
                ((params.matrix_col_spacing as f32) * scale * 2.0).max(12.0)
            } else {
                (params.matrix_col_spacing as f32) * scale
            };
            let matrix_w: f32 =
                col_widths.iter().sum::<f32>() + (num_cols.saturating_sub(1) as f32) * col_gap;

            // Total height from top of row 0 to bottom of row N-1:
            let total_content_h: f32 = (0..num_rows)
                .map(|i| row_heights[i] + row_depths[i])
                .sum::<f32>()
                + (num_rows.saturating_sub(1) as f32) * row_sep;

            // Center the entire matrix block symmetrically across the math axis:
            let top_y = origin_y + axis_h + (total_content_h * 0.5);
            let y_start = top_y - row_heights[0];

            let (open_str, close_str) = match delim {
                MatrixDelim::Parentheses => ("(", ")"),
                MatrixDelim::Brackets => ("[", "]"),
                MatrixDelim::Braces => ("{", "}"),
                MatrixDelim::VBar => ("|", "|"),
                MatrixDelim::DoubleVBar => ("||", "||"),
                MatrixDelim::LeftBrace => ("{", ""),
                MatrixDelim::None => ("", ""),
            };

            let has_delim = !open_str.is_empty();
            let target_h =
                total_content_h + ((m_metrics.extra_target_h as f32 * 0.001) * font_size).max(2.0);
            let y_center = origin_y + axis_h;

            let sep_ratio = match delim {
                MatrixDelim::VBar | MatrixDelim::DoubleVBar => m_metrics.vbar_sep,
                MatrixDelim::Brackets => m_metrics.bracket_sep,
                _ => m_metrics.default_sep,
            };
            let sep = ((sep_ratio as f32 * 0.001) * font_size).max(1.8);

            let open_w = if has_delim {
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
                0.0
            };

            let cells_start_x = if has_delim {
                origin_x + open_w + sep
            } else {
                origin_x
            };

            let default_align = if delim == MatrixDelim::LeftBrace {
                ColAlign::Left
            } else {
                ColAlign::Center
            };

            if !ctx.is_measuring() {
                // Second pass: paint cells aligned to each row's baseline and aligned within each column
                let mut curr_y = y_start;
                for (r_idx, r) in rows.iter().enumerate() {
                    let mut curr_x = cells_start_x;
                    for (col_idx, cell) in r.iter().enumerate() {
                        let col_w = col_widths[col_idx];
                        let cell_w = cell_widths[r_idx * num_cols + col_idx];
                        let align = col_alignments
                            .get(col_idx)
                            .copied()
                            .unwrap_or(default_align);
                        let offset_x = match align {
                            ColAlign::Left => 0.0,
                            ColAlign::Center => (col_w - cell_w) * 0.5,
                            ColAlign::Right => col_w - cell_w,
                        };
                        layout_node(ctx, cell, cell_style, curr_x + offset_x, curr_y);
                        curr_x += col_w + col_gap;
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
                let (close_x, close_w) = if has_close {
                    let cx = cells_start_x + matrix_w + sep;
                    let cw = layout_delimiter(
                        ctx, close_str, false, cx, y_center, target_h, font_size, rule_t,
                    );
                    (cx, cw)
                } else {
                    (cells_start_x + matrix_w, 0.0)
                };
                let half_h = (target_h * 0.5).max(total_content_h * 0.5);
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
            let ic_offset = match &**nucleus {
                MathNode::Variable(ch) => {
                    let face = ctx.face_override.unwrap_or(Face::Italic);
                    (glyph_top_accent_offset(*ch, face, ctx.font_family) as f32) * scale
                }
                _ => 0.5 * ic,
            };
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
                } else if *accent == "overrightarrow" {
                    let arrow_size = acc_scale * font_size;
                    let arrow_w = (metrics::SYMBOL_WIDTHS[0xAE] as f32 * 0.001) * arrow_size;
                    let arrow_y = origin_y + nuc_box.h + acc_lift;
                    let shaft_y = arrow_y + 0.13 * arrow_size;
                    if nuc_box.w > arrow_w {
                        let arrow_x = origin_x + nuc_box.w - arrow_w + ic_offset;
                        let rule_w = (arrow_x - (origin_x + ic_offset)).max(1.0);
                        ctx.emit_rule(
                            origin_x + ic_offset,
                            shaft_y,
                            rule_w + arrow_w * 0.25,
                            rule_t,
                        );
                        ctx.emit_glyph_run(arrow_x, arrow_y, Face::Symbol, arrow_size, "\u{00AE}");
                    } else {
                        let arrow_x = origin_x + (nuc_box.w - arrow_w) * 0.5 + ic_offset;
                        ctx.emit_glyph_run(arrow_x, arrow_y, Face::Symbol, arrow_size, "\u{00AE}");
                    }
                    MathBox::with_dims(nuc_box.w, nuc_box.h + acc_extra_h, nuc_box.d)
                } else if *accent == "overleftarrow" {
                    let arrow_size = acc_scale * font_size;
                    let arrow_w = (metrics::SYMBOL_WIDTHS[0xAC] as f32 * 0.001) * arrow_size;
                    let arrow_y = origin_y + nuc_box.h + acc_lift;
                    let shaft_y = arrow_y + 0.13 * arrow_size;
                    if nuc_box.w > arrow_w {
                        let arrow_x = origin_x + ic_offset;
                        let rule_start = arrow_x + arrow_w * 0.75;
                        let rule_w = (origin_x + nuc_box.w + ic_offset - rule_start).max(1.0);
                        ctx.emit_glyph_run(arrow_x, arrow_y, Face::Symbol, arrow_size, "\u{00AC}");
                        ctx.emit_rule(rule_start, shaft_y, rule_w, rule_t);
                    } else {
                        let arrow_x = origin_x + (nuc_box.w - arrow_w) * 0.5 + ic_offset;
                        ctx.emit_glyph_run(arrow_x, arrow_y, Face::Symbol, arrow_size, "\u{00AC}");
                    }
                    MathBox::with_dims(nuc_box.w, nuc_box.h + acc_extra_h, nuc_box.d)
                } else {
                    let is_wide = matches!(*accent, "widehat" | "widetilde");
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
                    let acc_x = origin_x + (nuc_box.w - eff_w) * 0.5 + ic_offset;
                    let acc_y = origin_y + nuc_box.h + acc_lift;
                    ctx.emit_glyph_run(acc_x, acc_y, am.face, eff_size, am.glyph);
                    MathBox::with_dims(nuc_box.w, nuc_box.h + eff_extra_h, nuc_box.d)
                }
            } else {
                MathBox::with_dims(nuc_box.w, nuc_box.h, nuc_box.d)
            }
        }

        MathNode::HBar => {
            let face = ctx.face_override.unwrap_or(Face::Italic);
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
                    atom_kinds[atom_count] = child.atom_kind();
                    atom_count += 1;
                }
            }

            // TeX Appendix G Rule 5 normalization over the mathematical atom sequence
            normalize_atom_kinds(&mut atom_kinds[..atom_count]);

            let mut effective_stack = [MathAtomKind::Ord; 64];
            let mut heap_effective;
            let node_effective: &mut [MathAtomKind] = if n <= 64 {
                &mut effective_stack[..n]
            } else {
                heap_effective = vec![MathAtomKind::Ord; n];
                &mut heap_effective[..]
            };

            for k in 0..atom_count {
                let node_idx = atom_node_indices[k] as usize;
                node_effective[node_idx] = atom_kinds[k];
            }

            let mut curr_x = origin_x;
            let mut max_h = 0.0f32;
            let mut max_d = 0.0f32;

            for (i, child) in nodes.iter().enumerate() {
                if i > 0 {
                    let prev_node = &nodes[i - 1];
                    if !matches!(prev_node, MathNode::Space(_))
                        && !matches!(child, MathNode::Space(_))
                    {
                        let prev_kind = node_effective[i - 1];
                        let curr_kind = node_effective[i];
                        let mut space = atom_spacing(
                            prev_kind,
                            curr_kind,
                            style.is_display(),
                            style.is_display_or_text(),
                        ) * font_size;

                        // Slanted variable / italic correction before opening delimiter (e.g. $f(x)$)
                        let is_open_delim = curr_kind == MathAtomKind::Open
                            || matches!(child, MathNode::Delimited(d) if d.open != ".")
                            || matches!(child, MathNode::Row(inner) if inner.first().map(|n| n.first_atom_kind()) == Some(MathAtomKind::Open));
                        if is_open_delim {
                            let ic = node_italic_correction(
                                prev_node,
                                ctx.face_override,
                                ctx.font_family,
                            ) * scale;
                            if ic > 0.0 {
                                space += ic + ((params.rule_thickness as f32) * scale).max(0.5);
                            }
                        }

                        curr_x += space;
                    }
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
    };
    ctx.style_stack.pop();
    result
}

/// Convenience function to measure a mathematical expression.
pub fn measure_math<'a>(
    node: &MathNode<'a>,
    base_font_size: f32,
    font_family: FontFamily,
    style: MathStyle,
) -> MathBox {
    let mut ctx = MathLayoutContext::new(base_font_size, font_family, None, None, None, style);
    layout_node(&mut ctx, node, style, 0.0, 0.0)
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
    layout_node(&mut ctx, node, style, origin_x, origin_y)
}
