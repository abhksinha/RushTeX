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
    char_height_depth, delimiter_metrics, glyph_top_accent_offset, hbar_metrics,
    italic_correction, lookup_accent_metrics, math_font_parameters_styled,
    matrix_metrics, measure_char, resolve_cm_symbol, resolve_optical_face, cm_alphabet_face,
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
    /// Colour of the `\color` scope being laid out (black outside any).
    pub color: crate::frontend::ast::PackedColor,
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
            color: crate::frontend::ast::PackedColor::BLACK,
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
            color: self.color,
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
                        Paint::Color { .. } => {}
                    }
                }
            }
        }
    }

    #[inline(always)]
    pub fn font_params(&self, style: MathStyle) -> &'static MathFontParameters {
        math_font_parameters_styled(self.font_family, style)
    }

    /// `(sup_drop, sub_drop)` in points from the script-size font of `style` (TeX §756).
    #[inline(always)]
    /// Width in points of a math skip in `style` (TeX §716: 1mu is 1/18 of the math quad).
    pub fn skip_width(&self, skip: crate::math::math_ast::MathSkip, style: MathStyle) -> f32 {
        use crate::math::math_ast::MathSkip;
        match skip {
            MathSkip::TextEm(em) => em * self.base_font_size,
            MathSkip::Pt(pt) => pt,
            MathSkip::Mu(mu) => {
                let quad = (self.font_params(style).quad as f32) * self.font_tier.font_size(style) * 0.001;
                mu * quad / 18.0
            }
            MathSkip::TextSpace if self.font_family == FontFamily::ComputerModern => {
                // fontdimen 2 of cmr10, cmr7 and cmr5, in em of each font.
                const CMR_SPACE: [f32; 3] = [0.333_334, 0.384_923, 0.472_23];
                let tier = (style.tier() as usize).saturating_sub(1).min(2);
                CMR_SPACE[tier] * self.font_tier.font_size(style)
            }
            MathSkip::TextSpace => self.skip_width(MathSkip::Mu(6.0), style),
        }
    }

    pub fn script_drops(&self, style: MathStyle) -> (f32, f32) {
        let script = style.sup_style();
        let p = self.font_params(script);
        let scale = self.font_tier.font_size(script) * 0.001;
        ((p.sup_drop as f32) * scale, (p.sub_drop as f32) * scale)
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

/// Lays out a script at the origin and appends its trailing italic correction, as TeX's
/// `clean_box` does for a character noad without its own subscript (Rule 17).
fn layout_script_box<'a, 'p>(ctx: &mut MathLayoutContext<'a, 'p>, node: &MathNode<'a>, style: MathStyle) -> MathBox {
    let mut mb = layout_node(ctx, node, style, 0.0, 0.0);
    mb.w += node_italic_correction_styled(node, ctx.face_override, ctx.font_family, style)
        * (ctx.font_tier.font_size(style) * 0.001);
    mb
}

/// TFM lig/kern program of the Computer Modern text font drawn for `face`.
fn cm_ligkern_table(face: Face) -> Option<&'static crate::backend::backend_ligkern::LigKernTable> {
    use crate::backend::backend_ligkern as lk;
    match face {
        Face::Roman => Some(&lk::CMR10),
        Face::Roman7 => Some(&lk::CMR7),
        Face::Roman5 => Some(&lk::CMR5),
        Face::Bold => Some(&lk::CMBX10),
        Face::Italic => Some(&lk::CMTI10),
        _ => None,
    }
}

/// A word after its font's ligature program has run, ready for kerning.
struct CmTextWord {
    glyphs: [u8; 64],
    len: usize,
    table: &'static crate::backend::backend_ligkern::LigKernTable,
}

/// Applies TeX's `=:` ligatures (§1035) to an ASCII word set in a Computer Modern text font.
/// Returns `None` when the word is not plain ASCII, too long, or the face has no TFM program.
fn cm_text_word(word: &str, face: Face, family: FontFamily) -> Option<CmTextWord> {
    if family != FontFamily::ComputerModern || word.len() < 2 || word.len() > 64 || !word.is_ascii() {
        return None;
    }
    let table = cm_ligkern_table(face)?;
    let mut glyphs = [0u8; 64];
    let mut len = 0usize;
    for &b in word.as_bytes() {
        glyphs[len] = b;
        len += 1;
        while len >= 2 {
            match table.lig(glyphs[len - 2], glyphs[len - 1]) {
                Some((0, lig)) => {
                    glyphs[len - 2] = lig;
                    len -= 1;
                }
                _ => break,
            }
        }
    }
    Some(CmTextWord { glyphs, len, table })
}

/// Width of a word in a text face, including its TFM ligatures and kerns under CM.
fn text_word_width(word: &str, face: Face, font_size: f32, family: FontFamily) -> f32 {
    match cm_text_word(word, face, family) {
        Some(w) => (0..w.len)
            .map(|i| {
                let c = w.glyphs[i];
                let kern = if i + 1 < w.len { w.table.kern(c, w.glyphs[i + 1]) } else { 0.0 };
                measure_char(c as char, face, font_size, family) + kern * font_size
            })
            .sum(),
        None => word.chars().map(|ch| measure_char(ch, face, font_size, family)).sum(),
    }
}

/// Emits a word in a text face at `(x, y)`, glyph by glyph when ligatures or kerns apply.
fn emit_text_word<'a, 'p>(ctx: &mut MathLayoutContext<'a, 'p>, word: &'a str, face: Face, font_size: f32, x: f32, y: f32) {
    let Some(w) = cm_text_word(word, face, ctx.font_family) else {
        ctx.emit_glyph_run(x, y, face, font_size, word);
        return;
    };
    let mut gx = x;
    for i in 0..w.len {
        let c = w.glyphs[i];
        ctx.emit_glyph_run(gx, y, face, font_size, ASCII_GLYPH_STRS[c as usize]);
        gx += measure_char(c as char, face, font_size, ctx.font_family);
        if i + 1 < w.len {
            gx += w.table.kern(c, w.glyphs[i + 1]) * font_size;
        }
    }
}

/// TFM italic correction (1000 units per em) of a Computer Modern glyph in a sized face.
fn cm_glyph_italic_correction(face: Face, c: u8) -> f32 {
    use crate::math::math_tfm as t;
    match face {
        Face::Roman => t::cmr10_metric(c).ic,
        Face::Roman7 => t::cmr7_metric(c).ic,
        Face::Roman5 => t::cmr5_metric(c).ic,
        Face::MathItalic => t::cmmi10_metric(c).ic,
        Face::MathItalic7 => t::cmmi7_metric(c).ic,
        Face::MathItalic5 => t::cmmi5_metric(c).ic,
        Face::Symbol => t::cmsy10_metric(c).ic,
        Face::Symbol7 => t::cmsy7_metric(c).ic,
        Face::Symbol5 => t::cmsy5_metric(c).ic,
        Face::Extension => t::cmex10_metric(c).ic,
        Face::AmsA => t::msam10_metric(c).ic,
        Face::AmsB => t::msbm10_metric(c).ic,
        Face::Bold => t::cmbx10_metric(c).ic,
        f => crate::math::math_tfm_faces::cm_face_metric(f, c).map_or(0.0, |m| m.ic),
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
        MathNode::FamilyChoice(choice) => {
            let branch = if family == FontFamily::ComputerModern { &choice.0 } else { &choice.1 };
            node_italic_correction_styled(branch, face_override, family, style)
        }
        MathNode::Variable(ch) => {
            let default_face = if family == FontFamily::ComputerModern {
                Face::MathItalic
            } else {
                Face::Italic
            };
            let face = face_override.unwrap_or(default_face);
            if family == FontFamily::ComputerModern {
                if let Some((alpha_face, code)) = cm_alphabet_face(face, *ch) {
                    // TeX: the italic correction of the alphabet's font at the current size.
                    let sized = crate::math::math_fonts::resolve_optical_face(alpha_face, style, family);
                    return crate::math::math_tfm_faces::cm_face_metric(sized, code).map_or(0.0, |m| m.ic);
                }
            }
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
            // Italic correction comes from the font of the current size (cmmi7 in script style).
            let face = crate::math::math_fonts::resolve_optical_face(face, style, family);
            italic_correction(*ch, face, family)
        }
        MathNode::Symbol { mathcode, .. }
            if family == FontFamily::ComputerModern && *mathcode != 0 =>
        {
            let Some((f, _, c)) = crate::math::math_fonts::resolve_cm_mathcode(*mathcode) else {
                return 0.0;
            };
            let ov = if crate::math::math_fonts::mathcode_follows_family(*mathcode) { face_override } else { None };
            let f = match ov {
                // \boldsymbol switches the math version: every family takes its bold font.
                _ if face_override == Some(Face::BoldItalic) => crate::math::math_fonts::bold_math_version(f),
                Some(o) => o,
                None => f,
            };
            cm_glyph_italic_correction(crate::math::math_fonts::resolve_optical_face(f, style, family), c)
        }
        MathNode::Symbol { code: Some(c), face, glyph, .. } => {
            if family == FontFamily::ComputerModern {
                let eff = resolve_cm_symbol(*c).map(|(f, _, code)| (f, code)).unwrap_or((*face, *c));
                let eff_face = crate::math::math_fonts::resolve_optical_face(
                    face_override.unwrap_or(eff.0),
                    style,
                    family,
                );
                match eff_face {
                    Face::MathItalic => crate::math::math_tfm::cmmi10_metric(eff.1).ic,
                    Face::MathItalic7 => crate::math::math_tfm::cmmi7_metric(eff.1).ic,
                    Face::MathItalic5 => crate::math::math_tfm::cmmi5_metric(eff.1).ic,
                    Face::Symbol => crate::math::math_tfm::cmsy10_metric(eff.1).ic,
                    Face::Symbol7 => crate::math::math_tfm::cmsy7_metric(eff.1).ic,
                    Face::Symbol5 => crate::math::math_tfm::cmsy5_metric(eff.1).ic,
                    Face::Extension => {
                        crate::math::math_tfm::cmex10_metric(eff.1).ic
                    }
                    _ => 0.0,
                }
            } else {
                let sym_b = glyph
                    .chars()
                    .next()
                    .and_then(crate::font_metrics::unicode_to_symbol_byte)
                    .unwrap_or(*c);
                symbol_italic_correction(sym_b)
            }
        }
        MathNode::Symbol { code: None, .. } => 0.0,
        MathNode::SubSup { .. } => 0.0,
        MathNode::StyleChange { face, content } => {
            node_italic_correction_styled(content, Some(*face), family, style)
        }
        MathNode::Colored { content, .. } => node_italic_correction_styled(content, face_override, family, style),
        MathNode::StyleScope { content, style: inner_style } => {
            // Convert from the inner style's units to the caller's, which scale by the outer size.
            node_italic_correction_styled(content, face_override, family, *inner_style)
                * (inner_style.scale() / style.scale())
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
        MathNode::Accent { accent, .. }
            if family == FontFamily::ComputerModern
                && (amsmath_arrow_fill(accent).is_some() || matches!(*accent, "dddot" | "ddddot")) =>
        {
            0.0
        }
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
        MathNode::DimensionBox { content, zero_width, .. }
            if !*zero_width => {
                node_italic_correction_styled(content, face_override, family, style)
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
/// `\nulldelimiterspace` from plain.tex and the LaTeX kernel.
const NULL_DELIMITER_SPACE: f32 = 1.2;

/// Small-variant face and code of a Computer Modern delimiter (the `\delcode`/`\delimiter`
/// small fields from plain.tex), for delimiters whose small form is not in cmr10.
fn cm_small_delimiter(delim: &str) -> Option<(Face, u8)> {
    let code = match delim {
        "{" | "\\{" | "\\lbrace" => 0x66,
        "}" | "\\}" | "\\rbrace" => 0x67,
        "<" | "\\langle" => 0x68,
        ">" | "\\rangle" => 0x69,
        "\\lfloor" => 0x62,
        "\\rfloor" => 0x63,
        "\\lceil" => 0x64,
        "\\rceil" => 0x65,
        "|" | "\\vert" | "\\mid" => 0x6A,
        "\\|" | "\\Vert" | "||" => 0x6B,
        "\\backslash" | "\\" => 0x6E,
        "/" => return Some((Face::MathItalic, 0x3D)),
        "\\surd" => 0x70,
        "(" | ")" | "[" | "]" => return Some((Face::Roman, delim.as_bytes()[0])),
        _ => return None,
    };
    Some((Face::Symbol, code))
}

/// Computer Modern metric of `code` in `face`, falling back to the 10pt table of its family.
fn cm_delim_metric(face: Face, code: u8) -> crate::math::math_tfm::GlyphMetric {
    if let Some(m) = crate::math::math_tfm_faces::cm_face_metric(face, code) {
        return m;
    }
    match face {
        Face::MathItalic | Face::MathItalic7 | Face::MathItalic5 => crate::math::math_tfm::cmmi10_metric(code),
        Face::Roman | Face::Roman7 | Face::Roman5 => crate::math::math_tfm::cmr10_metric(code),
        Face::Extension | Face::Extension7 => crate::math::math_tfm::cmex10_metric(code),
        _ => crate::math::math_tfm::cmsy10_metric(code),
    }
}

/// TeX's `var_delimiter` (§707) for Computer Modern: the small variant is tried in the
/// current size and then each larger size up to text size, then the large chain the same
/// way; the first glyph at least `target_h` tall wins, else the tallest seen. Returns the
/// face, code and size of a glyph, or `None` when an extensible recipe is reached.
fn cm_var_delimiter(ctx: &MathLayoutContext, delim: &str, is_left: bool, target_h: f32, font_size: f32) -> Option<(Face, u8, f32)> {
    let text = ctx.font_tier.font_size(MathStyle::Text);
    let script = ctx.font_tier.font_size(MathStyle::Script);
    let mut sizes = [font_size, 0.0, 0.0];
    let mut n = 1;
    for s in [script, text] {
        if s > sizes[n - 1] + 0.01 {
            sizes[n] = s;
            n += 1;
        }
    }
    let style_of = |s: f32| {
        if s >= text - 0.01 {
            MathStyle::Text
        } else if s >= script - 0.01 {
            MathStyle::Script
        } else {
            MathStyle::ScriptScript
        }
    };
    let mut best: Option<(Face, u8, f32)> = None;
    let mut best_hd = f32::MIN;
    if let Some((face, code)) = cm_small_delimiter(delim) {
        for &s in &sizes[..n] {
            let f = resolve_optical_face(face, style_of(s), ctx.font_family);
            let m = cm_delim_metric(f, code);
            let hd = (m.h + m.d) * 0.001 * s;
            if hd > best_hd {
                best = Some((f, code, s));
                best_hd = hd;
            }
            if hd >= target_h - crate::math::math_delim::DELIM_FIT_EPS {
                return best;
            }
        }
    }
    // The radical's large variant (`\radical"270370`) starts at cmex "70.
    let large = if delim == "\\surd" { Some(0x70) } else { resolve_cmex_delimiter(delim, is_left).map(|(c, _)| c[0]) };
    if let Some(large) = large {
        use crate::math::math_delim::{cmex_var_delimiter, CmexDelimiterChoice};
        for &s in &sizes[..n] {
            let ext = if s < 0.85 * text { Face::Extension7 } else { Face::Extension };
            match cmex_var_delimiter(large, target_h, s, ext) {
                CmexDelimiterChoice::Extensible(..) => return None,
                CmexDelimiterChoice::Glyph(code) => {
                    let m = crate::math::math_delim::cmex_face_metric(ext, code);
                    let hd = (m.h + m.d) * 0.001 * s;
                    if hd > best_hd {
                        best = Some((ext, code, s));
                        best_hd = hd;
                    }
                    if hd >= target_h - crate::math::math_delim::DELIM_FIT_EPS {
                        return best;
                    }
                }
            }
        }
    }
    best
}

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
        // TeX §762: a null delimiter is an empty box `\nulldelimiterspace` (1.2pt) wide.
        return (NULL_DELIMITER_SPACE, 0.0);
    }

    if ctx.font_family == metrics::FontFamily::ComputerModern
        && (cm_small_delimiter(delim).is_some() || resolve_cmex_delimiter(delim, is_left).is_some())
    {
        if let Some((face, code, size)) = cm_var_delimiter(ctx, delim, is_left, target_h, font_size) {
            let m = cm_delim_metric(face, code);
            let w = (m.w * 0.001) * size;
            let h = (m.h * 0.001) * size;
            let d = (m.d * 0.001) * size;
            if !ctx.is_measuring() {
                ctx.emit_glyph_run(x, y_center + (d - h) * 0.5, face, size, byte_str(code));
            }
            return (w, h + d);
        }
    }

    // Computer Modern bars are cmex10 extensibles ("0C/"0D repeated), handled below.
    if ctx.font_family != metrics::FontFamily::ComputerModern {
        if let Some((w, h)) = DelimiterBarRenderer::render_bar(
            ctx, delim, x, y_center, target_h, font_size, rule_t,
        ) {
            return (w, h);
        }
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

    if ctx.font_family == metrics::FontFamily::ComputerModern {
        if let Some((chain, _)) = resolve_cmex_delimiter(delim, is_left) {
            use crate::math::math_delim::{cmex_var_delimiter, CmexDelimiterChoice};
            // amsmath's `<-8>cmex7`: delimiters below text size come from cmex7.
            let ext_face = if font_size < 0.85 * ctx.base_font_size { Face::Extension7 } else { Face::Extension };
            match cmex_var_delimiter(chain[0], target_h, font_size, ext_face) {
                CmexDelimiterChoice::Glyph(code) => {
                    let m = crate::math::math_delim::cmex_face_metric(ext_face, code);
                    let w = (m.w * 0.001) * font_size;
                    let h = (m.h * 0.001) * font_size;
                    let d = (m.d * 0.001) * font_size;
                    if !ctx.is_measuring() {
                        ctx.emit_glyph_run(x, y_center + (d - h) * 0.5, ext_face, font_size, byte_str(code));
                    }
                    return (w, h + d);
                }
                CmexDelimiterChoice::Extensible(carrier, recipe) => {
                    let mut geom = ExtensibleAssemblyEngine::compute_assembly_geometry(recipe, target_h, font_size);
                    let cm = crate::math::math_tfm::cmex10_metric(carrier);
                    geom.width = (cm.w + cm.ic) * 0.001 * font_size;
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
            let w = (m.w * 0.001) * font_size;
            let h = (m.h * 0.001) * font_size;
            let d = (m.d * 0.001) * font_size;
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
        if let MathNode::Symbol { mathcode, .. } = target_node {
            if let Some((Face::MathItalic, _, glyph_code)) = crate::math::math_fonts::resolve_cm_mathcode(*mathcode) {
                return crate::math::math_tfm::CMMI10_SKEW_KERNS[glyph_code as usize] as f32 * 0.001 * font_size;
            }
        }
        if let MathNode::Symbol { code: Some(code), mathcode: 0, .. } = target_node {
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
    // TeX metrics are exact for Computer Modern; elsewhere keep hairlines visible.
    let rule_t = if ctx.font_family == metrics::FontFamily::ComputerModern {
        (params.rule_thickness as f32) * scale
    } else {
        ((params.rule_thickness as f32) * scale).max(0.35)
    };

    ctx.style_stack.push(style);
    let result = match node {
        // Computer Modern math alphabets (\mathsf, \mathtt, \mathcal, \mathfrak, \boldsymbol):
        // the glyph and its TFM box come from the alphabet's font at the current size.
        MathNode::Variable(ch)
            if ctx.font_family == metrics::FontFamily::ComputerModern
                && cm_alphabet_face(ctx.face_override.unwrap_or(Face::MathItalic), *ch).is_some() =>
        {
            let (alpha_face, code) = cm_alphabet_face(ctx.face_override.unwrap_or(Face::MathItalic), *ch)
                .unwrap_or((Face::MathItalic, b'x'));
            let face = resolve_optical_face(alpha_face, style, ctx.font_family);
            let (w, h, d) = crate::math::math_tfm_faces::cm_face_metric(face, code)
                .map_or((0.0, 0.0, 0.0), |m| (m.w * scale, m.h * scale, m.d * scale));
            ctx.emit_glyph_run(origin_x, origin_y, face, font_size, ASCII_GLYPH_STRS[code as usize]);
            MathBox::with_dims(w, h, d)
        }
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
                        (w, h_u * scale, d_u * scale, None)
                    }
                } else {
                    let w = measure_char(*ch, face, font_size, ctx.font_family);
                    let (h_u, d_u) = char_height_depth(*ch, face, ctx.font_family);
                    (w, h_u * scale, d_u * scale, None)
                }
            } else if base_face == Face::AmsB {
                let u_char = crate::math::fonts::to_unicode_blackboard(*ch);
                let gid = u_char.and_then(|u| crate::math::fonts::vector_glyph_for_unicode(u as u32, ctx.font_family));
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_height_depth(*ch, face, ctx.font_family);
                (w, h_u * scale, d_u * scale, gid)
            } else if base_face == Face::AmsA {
                let u_char = crate::math::fonts::to_unicode_fraktur(*ch);
                let gid = u_char.and_then(|u| crate::math::fonts::vector_glyph_for_unicode(u as u32, ctx.font_family));
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_height_depth(*ch, face, ctx.font_family);
                (w, h_u * scale, d_u * scale, gid)
            } else if base_face == Face::Mono {
                let u_char = crate::math::fonts::to_unicode_monospace(*ch);
                let gid = u_char.and_then(|u| {
                    crate::math::fonts::vector_glyph_for_unicode(u as u32, ctx.font_family)
                        .or_else(|| crate::math::fonts::active_math_font(ctx.font_family).and_then(|f| f.glyph_for_unicode(u as u32).map(|g| g.0)))
                });
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_height_depth(*ch, face, ctx.font_family);
                (w, h_u * scale, d_u * scale, gid)
            } else if base_face == Face::Italic {
                let u_char = crate::math::fonts::to_unicode_math_italic(*ch);
                let gid = u_char.and_then(|u| {
                    crate::math::fonts::vector_glyph_for_unicode(u as u32, ctx.font_family)
                        .or_else(|| crate::math::fonts::active_math_font(ctx.font_family).and_then(|f| f.glyph_for_unicode(u as u32).map(|g| g.0)))
                });
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_height_depth(*ch, face, ctx.font_family);
                (w, h_u * scale, d_u * scale, gid)
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
                (w, h_u * scale, d_u * scale, gid)
            } else {
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_height_depth(*ch, face, ctx.font_family);
                (w, h_u * scale, d_u * scale, None)
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
            let base_face = if ctx.font_family == metrics::FontFamily::ComputerModern {
                crate::math::math_fonts::cm_number_face(base_face)
            } else {
                base_face
            };
            let face = resolve_optical_face(base_face, style, ctx.font_family);
            let mut w = 0.0f32;
            for b in num_str.bytes() {
                w += measure_char(b as char, face, font_size, ctx.font_family);
            }
            let (h_u, d_u) = char_height_depth('0', face, ctx.font_family);
            let h = h_u * scale;
            let d = d_u * scale;

            ctx.emit_glyph_run(origin_x, origin_y, face, font_size, num_str);
            MathBox::with_dims(w, h, d)
        }

        MathNode::Symbol {
            kind, face, glyph, code, mathcode,
        } => {
            let cm_target = if ctx.font_family == metrics::FontFamily::ComputerModern {
                crate::math::math_fonts::resolve_cm_mathcode(*mathcode)
            } else {
                None
            };
            let (raw_face, eff_glyph, eff_code) = if let Some((cm_face, cm_glyph, cm_c)) = cm_target {
                (cm_face, cm_glyph, Some(cm_c))
            } else if ctx.font_family == metrics::FontFamily::ComputerModern {
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
            // `\boldsymbol` (BoldItalic) switches the math version, so it reaches every
            // class; alphabet switches such as `\mathbf` only re-map class-7 characters.
            let override_face = if ctx.face_override == Some(Face::BoldItalic)
                || crate::math::math_fonts::mathcode_follows_family(*mathcode)
            {
                ctx.face_override
            } else {
                None
            };
            // \boldsymbol (Computer Modern) switches the math version: each family's bold font.
            let cm_bold_version = ctx.font_family == metrics::FontFamily::ComputerModern
                && ctx.face_override == Some(Face::BoldItalic);
            let eff_face = if cm_bold_version {
                resolve_optical_face(crate::math::math_fonts::bold_math_version(raw_face), style, ctx.font_family)
            } else if let Some(ov) = override_face {
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
                (adv, h_u * scale, d_u * scale)
            } else if let Some(c) = eff_code {
                let w = if ctx.font_family == metrics::FontFamily::ComputerModern {
                    match eff_face {
                        Face::MathItalic => crate::math::math_tfm::cmmi10_metric(c).w * scale,
                        Face::MathItalic7 => crate::math::math_tfm::cmmi7_metric(c).w * scale,
                        Face::MathItalic5 => crate::math::math_tfm::cmmi5_metric(c).w * scale,
                        Face::Italic => crate::math::math_tfm::cmti10_metric(c).w * scale,
                        Face::Bold => crate::math::math_tfm::cmbx10_metric(c).w * scale,
                        Face::Roman => crate::math::math_tfm::cmr10_metric(c).w * scale,
                        Face::Roman7 => crate::math::math_tfm::cmr7_metric(c).w * scale,
                        Face::Roman5 => crate::math::math_tfm::cmr5_metric(c).w * scale,
                        Face::AmsA => crate::math::math_tfm::msam10_metric(c).w * scale,
                        Face::AmsB => crate::math::math_tfm::msbm10_metric(c).w * scale,
                        Face::Extension => crate::math::math_tfm::cmex10_metric(c).w * scale,
                        f if crate::math::math_tfm_faces::is_alphabet_face(f) => {
                            crate::math::math_tfm_faces::cm_face_metric(f, c).map_or(0.0, |m| m.w) * scale
                        }
                        Face::Symbol | Face::Symbol7 | Face::Symbol5 => {
                            let mw = match eff_face {
                                Face::Symbol7 => crate::math::math_tfm::cmsy7_metric(c).w * scale,
                                Face::Symbol5 => crate::math::math_tfm::cmsy5_metric(c).w * scale,
                                _ => crate::math::math_tfm::cmsy10_metric(c).w * scale,
                            };
                            if mw == 0.0 && c >= 128 {
                                metrics::SYMBOL_WIDTHS[c as usize] as f32 * scale
                            } else {
                                mw
                            }
                        }
                        _ => {
                            let mw = crate::math::math_tfm::cmsy10_metric(c).w * scale;
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
                        Face::Extension => {
                            let m = crate::math::math_tfm::cmex10_metric(c);
                            (m.h, m.d)
                        }
                        f if crate::math::math_tfm_faces::is_alphabet_face(f) => {
                            crate::math::math_tfm_faces::cm_face_metric(f, c).map_or((0.0, 0.0), |m| (m.h, m.d))
                        }
                        Face::Symbol | Face::Symbol7 | Face::Symbol5 => {
                            let m = match eff_face {
                                Face::Symbol7 => crate::math::math_tfm::cmsy7_metric(c),
                                Face::Symbol5 => crate::math::math_tfm::cmsy5_metric(c),
                                _ => crate::math::math_tfm::cmsy10_metric(c),
                            };
                            if m.h == 0.0 && m.d == 0.0 && c >= 128 {
                                symbol_height_depth_family(c, ctx.font_family)
                            } else {
                                (m.h, m.d)
                            }
                        }
                        _ => {
                            let m = crate::math::math_tfm::cmsy10_metric(c);
                            if m.h == 0.0 && m.d == 0.0 && c >= 128 {
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
                (w, h_u * scale, d_u * scale)
            } else if let Some(word) = cm_text_word(eff_glyph, eff_face, ctx.font_family) {
                // `\text` words: an hbox in the text font, with its ligatures and kerns and
                // no trailing italic correction.
                let mut total_w = 0.0f32;
                let (mut max_h_u, mut max_d_u) = (0.0f32, 0.0f32);
                for (i, &c) in word.glyphs[..word.len].iter().enumerate() {
                    let gx = origin_x + total_w;
                    ctx.emit_glyph_run(gx, origin_y, eff_face, font_size, ASCII_GLYPH_STRS[c as usize]);
                    total_w += measure_char(c as char, eff_face, font_size, ctx.font_family);
                    if i + 1 < word.len {
                        total_w += word.table.kern(c, word.glyphs[i + 1]) * font_size;
                    }
                    let (ch_h, ch_d) = char_height_depth(c as char, eff_face, ctx.font_family);
                    max_h_u = max_h_u.max(ch_h);
                    max_d_u = max_d_u.max(ch_d);
                }
                // Operator names and joined math letters are math characters: the last one
                // keeps its italic correction.
                if *kind == MathAtomKind::Op || crate::math::math_ast::is_math_ligature_word(eff_glyph) {
                    total_w += cm_glyph_italic_correction(eff_face, word.glyphs[word.len - 1]) * scale;
                }
                return MathBox::with_dims(total_w, max_h_u * scale, max_d_u * scale);
            } else {
                let mut total_w = 0.0f32;
                let mut max_h_u = 0.0f32;
                let mut max_d_u = 0.0f32;
                for ch in eff_glyph.chars() {
                    total_w += measure_char(ch, eff_face, font_size, ctx.font_family);
                    let (ch_h, ch_d) = char_height_depth(ch, eff_face, ctx.font_family);
                    max_h_u = max_h_u.max(ch_h);
                    max_d_u = max_d_u.max(ch_d);
                }
                // Text words are hboxes; operator names keep the last character's italic correction.
                if *kind == MathAtomKind::Op {
                    if let Some(last_ch) = eff_glyph.chars().last() {
                        total_w += italic_correction(last_ch, eff_face, ctx.font_family) * scale;
                    }
                }
                (total_w, max_h_u * scale, max_d_u * scale)
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
            if let MathNode::OpLimits { content, mode } = &**nucleus {
                return layout_op_limits(ctx, content, *mode, sub.as_deref(), sup.as_deref(), style, origin_x, origin_y);
            }
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
            let sup_box = match sup {
                Some(sp) => layout_script_box(ctx, sp, style.sup_style()),
                None => MathBox::zero(),
            };
            let sup_end = ctx.out_ops_len();

            let sub_start = ctx.out_ops_len();
            let sub_box = match sub {
                Some(sb) => layout_script_box(ctx, sb, style.sub_style()),
                None => MathBox::zero(),
            };
            let sub_end = ctx.out_ops_len();

            // TeX Appendix G Rule 17: Italic correction is applied to the horizontal
            // placement of the superscript, but not the subscript.
            let ic = node_italic_correction_styled(effective_nuc, ctx.face_override, ctx.font_family, style) * scale;
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
            // amsbsy sets `\boldsymbol{..}` in an \hbox (\mathchoice of boxed formulas), so the
            // nucleus is a box: its width already carries the italic correction (Rule 18a/b
            // treat it as a non-character).
            let boxed_bold = matches!(effective_nuc, MathNode::StyleChange { face: Face::BoldItalic, .. });
            let sup_x = origin_x + nuc_box.w + ic + math_kern;
            let sub_x = origin_x + nuc_box.w + if boxed_bold { ic } else { 0.0 };

            let x_height = (params.accent_base_height as f32) * scale;

            let is_char = match effective_nuc {
                MathNode::Variable(_) | MathNode::Symbol { .. } | MathNode::Number(_) => true,
                MathNode::Row(children) if children.len() == 1 => {
                    matches!(&children[0], MathNode::Variable(_) | MathNode::Symbol { .. } | MathNode::Number(_))
                }
                _ if boxed_bold => false,
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
                ctx.script_drops(style),
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
            let total_h = scripts.total_height(sup.is_some(), sub.is_some(), nuc_box.h, sup_shift, sub_shift);
            let total_d = scripts.total_depth(sup.is_some(), sub.is_some(), nuc_box.d, sup_shift, sub_shift);

            MathBox::with_dims(total_w, total_h, total_d)
        }

        MathNode::Fraction {
            num,
            den,
            has_rule,
            options,
            thickness,
        } => {
            // `\above` / `\genfrac` give the rule thickness that Rule 15 uses (θ).
            let rule_t = if *thickness >= 0.0 { *thickness } else { rule_t };
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

            // TeX §748: `\nulldelimiterspace` (1.2pt) on both sides unless the fraction
            // carries its own delimiters.
            let frac_pad = if *options & crate::math::math_ast::FRACTION_OPT_DELIMITED != 0 {
                0.0
            } else {
                1.2 * (ctx.base_font_size / 10.0)
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
                // plain/latex `\r@@t`: `\mkern5mu` before and `\mkern-10mu` after the degree.
                let kern_before = if ctx.font_family == metrics::FontFamily::ComputerModern {
                    ctx.skip_width(crate::math::math_ast::MathSkip::Mu(5.0), style)
                } else {
                    (c.radical_kern_before_degree as f32 * 0.001) * font_size
                };
                let mut kern_after = if ctx.font_family == metrics::FontFamily::ComputerModern {
                    ctx.skip_width(crate::math::math_ast::MathSkip::Mu(-10.0), style)
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

            let cm_choice = if ctx.font_family == metrics::FontFamily::ComputerModern {
                cm_var_delimiter(ctx, "\\surd", false, v, font_size)
            } else {
                None
            };
            let mut rad_size = font_size;
            let (rad_w, rad_h, rad_d, is_ext, face, rad_code, n_rep) = if let Some((f, code, size)) = cm_choice {
                let m = cm_delim_metric(f, code);
                rad_size = size;
                let em = |v: f32| (v * 0.001) * size;
                (em(m.w), em(m.h), em(m.d), false, f, code, 0usize)
            } else {
                let kind = crate::math::fonts::OpenTypeDelimKind::Radical;
                match kind.select_variant_tier(v, font_size) {
                    Some(0) => {
                        let w = (small_m.w * 0.001) * font_size;
                        let h = (small_m.h * 0.001) * font_size;
                        let d = (small_m.d * 0.001) * font_size;
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
                        let w = (m.w * 0.001) * font_size;
                        let h = (m.h * 0.001) * font_size;
                        let d = (m.d * 0.001) * font_size;
                        (w, h, d, false, Face::Extension, code, 0usize)
                    }
                    None => {
                        // Extensible radical in cmex10 (top 0x76, rep 0x75, bot 0x74)
                        let m_top = crate::math::math_tfm::cmex10_metric(0x76);
                        let m_bot = crate::math::math_tfm::cmex10_metric(0x74);
                        let m_rep = crate::math::math_tfm::cmex10_metric(0x75);
                        let h_fixed = ((m_top.h + m_top.d + m_bot.h + m_bot.d) * 0.001) * font_size;
                        let u = ((m_rep.h + m_rep.d) * 0.001) * font_size;
                        let eps = crate::math::math_delim::DELIM_FIT_EPS;
                        let n = if u > 0.0 && v > h_fixed + eps {
                            ((v - h_fixed - eps) / u).ceil() as usize
                        } else {
                            0
                        };
                        let total_hd = h_fixed + (n as f32) * u;
                        let w = (m_top.w * 0.001) * font_size;
                        let h = (m_top.h * 0.001) * font_size;
                        let d = total_hd - h;
                        (w, h, d, true, Face::Extension, 0x74u8, n)
                    }
                }
            };

            // TeX Appendix G Rule 11 (§737): the excess depth of the surd over the radicand
            // and clearance is split evenly, and the overbar is as thick as the surd is tall.
            let is_cm_surd = ctx.font_family == metrics::FontFamily::ComputerModern;
            let bar_t = if is_cm_surd { rad_h } else { rule_t_cm };
            let delta = if is_cm_surd {
                rad_d - (rad_box.h + rad_box.d + min_clr)
            } else {
                rad_h + rad_d - (rad_box.h + rad_box.d + min_clr + rule_t_cm)
            };
            let actual_clr = if delta > 0.0 {
                min_clr + delta * 0.5
            } else {
                min_clr
            };

            let rule_bottom_y = origin_y + rad_box.h + actual_clr;
            let rad_y = rule_bottom_y + bar_t - rad_h;
            let rad_content_x = rad_origin_x + rad_w;

            if !ctx.is_measuring() {
                if !is_ext {
                    let s = crate::math::math_delim::byte_str(rad_code);
                    ctx.emit_glyph_run(rad_origin_x, rad_y, face, rad_size, s);
                } else {
                    // Extensible radical from cmex10
                    // Top piece (0x76): its upper horizontal ledge sits at height m_top.h above baseline.
                    // We align this ledge with rule_bottom_y + bar_t so the overbar connects seamlessly.
                    let m_top = crate::math::math_tfm::cmex10_metric(0x76);
                    let m_rep = crate::math::math_tfm::cmex10_metric(0x75);
                    let m_bot = crate::math::math_tfm::cmex10_metric(0x74);

                    let top_glyph_h = (m_top.h * 0.001) * font_size;
                    let top_baseline_y = rule_bottom_y + bar_t - top_glyph_h;
                    let s_top = crate::math::math_delim::byte_str(0x76);
                    ctx.emit_glyph_run(rad_origin_x, top_baseline_y, Face::Extension, font_size, s_top);

                    let mut curr_y = top_baseline_y - (m_top.d * 0.001) * font_size;
                    let s_rep = crate::math::math_delim::byte_str(0x75);
                    let rep_d = (m_rep.d * 0.001) * font_size;
                    let rep_h = (m_rep.h * 0.001) * font_size;
                    for _ in 0..n_rep {
                        let rep_baseline = curr_y - rep_h;
                        ctx.emit_glyph_run(rad_origin_x, rep_baseline, Face::Extension, font_size, s_rep);
                        curr_y = rep_baseline - rep_d;
                    }

                    let bot_h = (m_bot.h * 0.001) * font_size;
                    let bot_baseline = curr_y - bot_h;
                    let s_bot = crate::math::math_delim::byte_str(0x74);
                    ctx.emit_glyph_run(rad_origin_x, bot_baseline, Face::Extension, font_size, s_bot);
                }

                // Overbar rule connecting seamlessly to glyph ledge:
                let overlap = (0.5 * bar_t).max(0.20);
                let rule_start = rad_content_x - overlap;
                let rule_w = (rad_content_x + rad_box.w) - rule_start;
                ctx.emit_rule(
                    rule_start,
                    rule_bottom_y,
                    rule_w,
                    bar_t,
                );
            }

                ctx.shift_ops_range(rad_start, rad_end, rad_content_x, origin_y);

                let mut total_d = rad_box.d.max(rad_d - rad_box.h - actual_clr);
                let mut total_h = (rule_bottom_y + bar_t + bar_t) - origin_y;

                if degree.is_some() {
                    let deg_x = origin_x + deg_x_offset;
                    let deg_y = if ctx.font_family == metrics::FontFamily::ComputerModern {
                        origin_y + 0.60 * ((rule_bottom_y + bar_t - origin_y) - total_d)
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
            let is_display_limits = OperatorLimitSwitch::takes_limits(*limits, style);
            let mut cm_op_code: u8 = 0;
            // amsmath's OMX font definition `<-8>cmex7`: script and scriptscript operators
            // come from cmex7 (scaled to the style size), with its own TFM box.
            let op_ext_face = if ctx.font_family == metrics::FontFamily::ComputerModern && style.tier() >= 2 {
                Face::Extension7
            } else {
                Face::Extension
            };
            let op_metric = |c: u8| {
                crate::math::math_tfm_faces::cm_face_metric(op_ext_face, c)
                    .unwrap_or_else(|| crate::math::math_tfm::cmex10_metric(c))
            };
            // `\limsup`/`\liminf` are `lim\,sup`/`lim\,inf` (latex.ltx); other operator
            // names are a single word set in the operator font at the current size.
            let (text_head, text_tail) = match *op {
                "limsup" => ("lim", "sup"),
                "liminf" => ("lim", "inf"),
                _ => (*op, ""),
            };
            let text_face = resolve_optical_face(Face::Roman, style, ctx.font_family);
            let text_gap = if text_tail.is_empty() {
                0.0
            } else {
                3.0 * (params.quad as f32) * scale / 18.0
            };
            let text_head_w = text_word_width(text_head, text_face, font_size, ctx.font_family);
            let (op_face, op_size, op_w, op_h, op_d, op_ic) = if is_text_op {
                let tail_w = text_word_width(text_tail, text_face, font_size, ctx.font_family);
                let (mut h_u, mut d_u) = (0.0f32, 0.0f32);
                for ch in text_head.chars().chain(text_tail.chars()) {
                    let (ch_h, ch_d) = char_height_depth(ch, text_face, ctx.font_family);
                    h_u = h_u.max(ch_h);
                    d_u = d_u.max(ch_d);
                }
                (
                    text_face,
                    font_size,
                    text_head_w + text_gap + tail_w,
                    h_u * scale,
                    d_u * scale,
                    0.0,
                )
            } else if is_integral {
                let code = BigOpVariantTier::resolve_integral_cmex_code(op, style.is_display());
                cm_op_code = code;
                let m = op_metric(code);
                let single_w = (m.w + m.ic) * (font_size * 0.001);
                let h = m.h * (font_size * 0.001);
                let d = m.d * (font_size * 0.001);
                // The emitted glyph is the cmex10 one, so its TFM italic correction applies.
                let ic = m.ic * (font_size * 0.001);
                let (w, _) = IntegralOverhangCorrector::total_integral_width(
                    op,
                    single_w,
                    font_size,
                    style.is_display(),
                );
                (op_ext_face, font_size, w, h, d, ic)
            } else {
                let cmex_code = BigOpVariantTier::resolve_cmex_code(op, style.is_display());
                cm_op_code = cmex_code;
                let m = op_metric(cmex_code);
                let w = (m.w + m.ic) * (font_size * 0.001);
                let h = m.h * (font_size * 0.001);
                let d = m.d * (font_size * 0.001);
                // The emitted glyph is the cmex10 one, so its TFM italic correction applies.
                let ic = m.ic * (font_size * 0.001);
                (op_ext_face, font_size, w, h, d, ic)
            };

            // TeX Appendix G Rule 13: shift operator vertically so center is at math axis
            let delta_y = Rule13OperatorAxis::compute_delta_y(is_text_op, axis_h, op_h, op_d);
            let op_y = origin_y + delta_y;
            let op_top_y = op_y + op_h;
            let op_bot_y = op_y - op_d;
            let half_ic = 0.5 * op_ic;

            let emit_op = |ctx: &mut MathLayoutContext<'a, 'p>, x: f32, y: f32| {
                if is_text_op {
                    emit_text_word(ctx, text_head, op_face, op_size, x, y);
                    if !text_tail.is_empty() {
                        emit_text_word(ctx, text_tail, op_face, op_size, x + text_head_w + text_gap, y);
                    }
                } else if is_integral {
                    let int_glyph = crate::math::math_delim::byte_str(cm_op_code);
                    let m = op_metric(cm_op_code);
                    let single_w = (m.w + m.ic) * (op_size * 0.001);
                    let offset = IntegralOverhangCorrector::integral_offset(
                        single_w,
                        op_size,
                        style.is_display(),
                    );
                    if *op == "iint" || *op == "oiint" {
                        ctx.emit_glyph_run(x, y, op_ext_face, op_size, int_glyph);
                        ctx.emit_glyph_run(
                            x + offset,
                            y,
                            op_ext_face,
                            op_size,
                            int_glyph,
                        );
                    } else if *op == "iiint" || *op == "oiiint" {
                        ctx.emit_glyph_run(x, y, op_ext_face, op_size, int_glyph);
                        ctx.emit_glyph_run(
                            x + offset,
                            y,
                            op_ext_face,
                            op_size,
                            int_glyph,
                        );
                        ctx.emit_glyph_run(
                            x + 2.0 * offset,
                            y,
                            op_ext_face,
                            op_size,
                            int_glyph,
                        );
                    } else {
                        ctx.emit_glyph_run(x, y, op_ext_face, op_size, int_glyph);
                    }
                } else {
                    let s = crate::math::math_delim::byte_str(cm_op_code);
                    ctx.emit_glyph_run(x, y, op_ext_face, op_size, s);
                }
            };

            if is_display_limits {
                let sub_start = ctx.out_ops_len();
                let sub_box = match sub {
                    Some(s) => layout_script_box(ctx, s, style.sub_style()),
                    None => MathBox::zero(),
                };
                let sub_end = ctx.out_ops_len();

                let sup_start = ctx.out_ops_len();
                let sup_box = match sup {
                    Some(s) => layout_script_box(ctx, s, style.sup_style()),
                    None => MathBox::zero(),
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
                )
            } else {
                let sup_start = ctx.out_ops_len();
                let sup_box = match sup {
                    Some(sp) => layout_script_box(ctx, sp, style.sup_style()),
                    None => MathBox::zero(),
                };
                let sup_end = ctx.out_ops_len();

                let sub_start = ctx.out_ops_len();
                let sub_box = match sub {
                    Some(sb) => layout_script_box(ctx, sb, style.sub_style()),
                    None => MathBox::zero(),
                };
                let sub_end = ctx.out_ops_len();

                let (sup_x_rel, sub_x_rel, sup_shift, sub_shift) =
                    BigOpShiftResolver::resolve_side_scripts(
                        op_w,
                        op_ic,
                        delta_y + op_h,
                        op_d - delta_y,
                        sup.is_some(),
                        sub.is_some(),
                        style,
                        sup_box.d,
                        sub_box.h,
                        scale,
                        params,
                        ctx.script_drops(style),
                    );

                let sup_x = origin_x + sup_x_rel;
                let sub_x = origin_x + sub_x_rel;
                // \scriptspace follows the scripts (TeX §756).
                let script_space = if sup.is_some() || sub.is_some() {
                    0.5 * (ctx.base_font_size / 10.0)
                } else {
                    0.0
                };

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
                    sup_box.w + script_space,
                    sup_shift,
                    sup_box.h,
                    sub.is_some(),
                    sub_x_rel,
                    sub_box.w + script_space,
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

            // A generalized fraction's own delimiters (`\atopwithdelims`, `\binom`, `\genfrac`).
            let frac_options = match &**content {
                MathNode::Fraction { options, .. } => Some(*options),
                MathNode::StyleScope { content: inner, .. } => match &**inner {
                    MathNode::Fraction { options, .. } => Some(*options),
                    _ => None,
                },
                _ => None,
            };
            let is_genfrac = frac_options.is_some_and(|o| o & crate::math::math_ast::FRACTION_OPT_DELIMITED != 0);

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
            // The inner list ends with its last character's italic-correction kern (TeX §755).
            let mut content_box = content_box;
            content_box.w += node_italic_correction_styled(content, ctx.face_override, ctx.font_family, style) * scale;

            let mut target_h = crate::math::math_delim::DelimiterSizingGovernor::target_height_with_shortfall(
                content_box.h,
                content_box.d,
                axis_h,
                font_size,
                shortfall,
            );

            // TeX Appendix G Rule 15e (mlist.c 3131-3138): a generalized fraction's delimiters
            // are var_delimiter'd to σ20 (delim1) in display style and σ21 (delim2) otherwise,
            // whatever the fraction's height.
            if is_genfrac {
                let delim = if style.is_display() { params.delim1 } else { params.delim2 };
                target_h = (delim as f32) * scale;
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
            if matches!(
                env_kind,
                MatrixEnvKind::SmallMatrix | MatrixEnvKind::Subarray | MatrixEnvKind::Overlay | MatrixEnvKind::Vereq
            ) {
                return layout_script_array(ctx, rows, col_alignments, env_kind, style, origin_x, origin_y);
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
                    // Each cell is its own formula (`$...$` in an `\halign`), so it ends with
                    // its last character's italic correction.
                    let mut b = layout_node(&mut dry_ctx, cell, cell_style, 0.0, 0.0);
                    b.w += node_italic_correction_styled(cell, dry_ctx.face_override, dry_ctx.font_family, cell_style)
                        * (dry_ctx.font_tier.font_size(cell_style) * 0.001);
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
                    #[allow(clippy::needless_range_loop)]
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
                    (cells_start_x + matrix_w, 1.2 * (ctx.base_font_size / 10.0), 0.0)
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
            let nuc_start = ctx.out_ops_len();
            let nuc_box = layout_node(ctx, nucleus, nuc_style, origin_x, origin_y);
            let nuc_end = ctx.out_ops_len();
            let ic = node_italic_correction_styled(nucleus, ctx.face_override, ctx.font_family, style) * scale;
            let skew_offset = compute_top_accent_skew_offset(nucleus, ctx, font_size, scale, ic);

            if ctx.font_family == metrics::FontFamily::ComputerModern {
                if let Some((left, right, below)) = amsmath_arrow_fill(accent) {
                    return layout_arrow_fill_accent(
                        ctx, nuc_box, nuc_start, nuc_end, ic, left, right, below, style, origin_x, origin_y,
                    );
                }
                if matches!(*accent, "dddot" | "ddddot") {
                    let n_dots = if *accent == "dddot" { 3 } else { 4 };
                    let mut nuc = nuc_box;
                    nuc.w += ic;
                    return layout_amsmath_dots_accent(ctx, nuc, nuc_start, nuc_end, n_dots, style, origin_x, origin_y);
                }
            }

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
                        crate::math::math_fonts::AccentPosition::Overlay if *accent == "not" => {
                            // plain.tex/fontmath.ltx: `\not` is the zero-width relation cmsy10 "36
                            // set on the baseline at the left edge of the following relation.
                            let m = crate::math::math_tfm::cmsy10_metric(code);
                            let (gh, gd) = (m.h * 0.001 * font_size, m.d * 0.001 * font_size);
                            ctx.emit_glyph_run(
                                origin_x,
                                origin_y,
                                face,
                                font_size,
                                crate::math::math_delim::byte_str(code),
                            );
                            return MathBox::with_dims(nuc_box.w, nuc_box.h.max(gh), nuc_box.d.max(gd));
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

                            // cancel.sty strikes through in a picture overlay that leaves the
                            // box of its argument unchanged.
                            if *accent == "cancel" {
                                return nuc_box;
                            }
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

        MathNode::HBar if ctx.font_family == metrics::FontFamily::ComputerModern => {
            // amsfonts declares `\hbar` as msbm10 "7E (`\mathord`, AMSb family).
            let sym = MathNode::Symbol {
                kind: MathAtomKind::Ord,
                face: Face::AmsB,
                glyph: "~",
                code: None,
                mathcode: 0x057E,
            };
            layout_node(ctx, &sym, style, origin_x, origin_y)
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
            let h = h_u * scale;
            let d = d_u * scale;

            ctx.emit_glyph_run(origin_x, origin_y, face, font_size, "h");
            let hm = hbar_metrics(ctx.font_family);
            let bar_w = (hm.bar_w as f32 * 0.001) * font_size;
            let bar_x = origin_x + (hm.bar_x as f32 * 0.001) * font_size;
            let bar_y = origin_y + (hm.bar_y as f32 * 0.001) * font_size;
            ctx.emit_rule(bar_x, bar_y, bar_w, rule_t);

            MathBox::with_dims(w, h, d)
        }

        MathNode::Space(skip) => MathBox::with_dims(ctx.skip_width(*skip, style), 0.0, 0.0),

        MathNode::Nonscript => MathBox::zero(),

        MathNode::StyleChange { face, content } => {
            let prev = ctx.face_override;
            ctx.face_override = Some(*face);
            let b = layout_node(ctx, content, style, origin_x, origin_y);
            ctx.face_override = prev;
            b
        }

        MathNode::Colored { color, content } => {
            let prev = ctx.color;
            ctx.color = *color;
            if let Some(ops) = ctx.out_ops.as_deref_mut() {
                ops.push(Paint::Color { color: *color });
            }
            let b = layout_node(ctx, content, style, origin_x, origin_y);
            ctx.color = prev;
            if let Some(ops) = ctx.out_ops.as_deref_mut() {
                ops.push(Paint::Color { color: prev });
            }
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
                    atom_kinds[atom_count] = if matches!(child, MathNode::MiddleDelim { .. }) {
                        MathAtomKind::Close
                    } else {
                        child.atom_kind_in_style(style)
                    };
                    atom_count += 1;
                }
            }

            // TeX Appendix G Rule 5 normalization over the mathematical atom sequence
            normalize_atom_kinds(&mut atom_kinds[..atom_count]);
            // A `\middle` reads as Close to its left (above) and as Open to its right, so a Bin
            // right after it becomes Ord (Rule 5).
            for (middle_idx, child) in
                nodes.iter().filter(|c| !matches!(c, MathNode::Space(_) | MathNode::Nonscript)).enumerate()
            {
                if matches!(child, MathNode::MiddleDelim { .. })
                    && middle_idx + 1 < atom_count
                    && atom_kinds[middle_idx + 1] == MathAtomKind::Bin
                {
                    atom_kinds[middle_idx + 1] = MathAtomKind::Ord;
                }
            }

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
                if let MathNode::Space(skip) = child {
                    let eff_amt = if pending_nonscript && !is_disp_or_text {
                        0.0
                    } else {
                        ctx.skip_width(*skip, style)
                    };
                    pending_nonscript = false;
                    curr_x += eff_amt;
                    continue;
                }
                pending_nonscript = false;

                if curr_atom_idx > 0 {
                    let prev_k = curr_atom_idx - 1;
                    let prev_kind = if matches!(prev_node, Some(MathNode::MiddleDelim { .. })) {
                        MathAtomKind::Open
                    } else {
                        atom_kinds[prev_k]
                    };
                    let curr_kind = atom_kinds[curr_atom_idx];
                    let mut space = if is_cm {
                        let mu = atom_spacing_mu(prev_kind, curr_kind, is_disp_or_text);
                        mu_table[mu as usize]
                    } else {
                        let idx = ((prev_kind as usize & 7) << 3) | (curr_kind as usize & 7);
                        lut_row[idx] * font_size
                    };

                    // Letters of one CM math alphabet: the font's kern, and no italic
                    // correction inside a word of a text font (TeX make_ord).
                    let alpha_pair = match (prev_node, child) {
                        (Some(MathNode::Variable(c1)), MathNode::Variable(c2)) if is_cm => {
                            crate::math::math_fonts::cm_alphabet_pair(ctx.face_override, *c1, *c2, style, font_size)
                        }
                        _ => None,
                    };
                    space += if alpha_pair.is_some_and(|p| !p.0) { 0.0 } else { prev_node_ic };

                    if let Some((_, kern)) = alpha_pair {
                        space += kern;
                    } else if is_cm {
                        if let (Some(MathNode::Variable(c1)), MathNode::Variable(c2)) = (prev_node, child) {
                            // Adjacent characters of one font take its TFM kern (TeX §752); in a
                            // text font (nonzero interword space) the italic correction between
                            // them is dropped as well.
                            let face = resolve_optical_face(
                                ctx.face_override.unwrap_or(Face::MathItalic),
                                style,
                                ctx.font_family,
                            );
                            match cm_ligkern_table(face) {
                                Some(table) if c1.is_ascii() && c2.is_ascii() => {
                                    space -= prev_node_ic;
                                    space += table.kern(*c1 as u8, *c2 as u8) * font_size;
                                }
                                Some(_) => {}
                                None => space += crate::math::math_tfm::cmmi_pair_kern(*c1, *c2, tier, font_size),
                            }
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

        MathNode::FamilyChoice(choice) => {
            let branch = if ctx.font_family == metrics::FontFamily::ComputerModern {
                &choice.0
            } else {
                &choice.1
            };
            layout_node(ctx, branch, style, origin_x, origin_y)
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
            // amsmath `\bBigg@`: `\left` around an empty box 1, 1.5, 2 or 2.5 times
            // `\big@size` (1.2em of the text font) tall, centred on the axis, so the
            // delimiter must reach Rule 19's max(0.901 z, z - 5pt).
            let factor = [1.0f32, 1.5, 2.0, 2.5][(*size_tier as usize).min(3)];
            let z = factor * 1.2 * ctx.base_font_size;
            let target_h = (z * 0.901).max(z - 5.0 * ctx.base_font_size / 10.0);
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
            // LaTeX's `\strutbox`: 0.7 and 0.3 of the normal-size `\baselineskip` (1.2em).
            let h = 0.84 * ctx.base_font_size;
            let d = 0.36 * ctx.base_font_size;
            MathBox::with_dims(0.0, h, d)
        }

        MathNode::Phantom { content, width, height_depth } => {
            let inner_box = measure_math(content, font_size, ctx.font_family, style);
            let w = if *width { inner_box.w } else { 0.0 };
            let (h, d) = if *height_depth { (inner_box.h, inner_box.d) } else { (0.0, 0.0) };
            MathBox::with_dims(w, h, d)
        }

        MathNode::OpLimits { content, mode } => {
            layout_op_limits(ctx, content, *mode, None, None, style, origin_x, origin_y)
        }

        MathNode::SideSet(parts) => {
            // amsmath: `\hbox to\dimen@{}\mathop{\kern-\dimen@\box4\box6}` with every part
            // measured in display style and \dimen@ = wd4 + wd0 - wd6. The empty Ord box
            // precedes this node in the list; the Op draws the pre-scripted and the
            // post-scripted boxes side by side.
            let (_, pre, post) = &**parts;
            let pre_box = layout_node(ctx, pre, MathStyle::Display, origin_x, origin_y);
            let post_box = layout_node(ctx, post, MathStyle::Display, origin_x + pre_box.w, origin_y);
            MathBox::with_dims(pre_box.w + post_box.w, pre_box.h.max(post_box.h), pre_box.d.max(post_box.d))
        }

        MathNode::Boxed(content) => {
            // `\fbox`: `\fboxsep` 3pt of padding inside a `\fboxrule` 0.4pt frame.
            let unit = ctx.base_font_size / 10.0;
            let (sep, rule) = (3.0 * unit, 0.4 * unit);
            let pad = sep + rule;
            let inner = layout_node(ctx, content, MathStyle::Display, origin_x + pad, origin_y);
            let w = inner.w + 2.0 * pad;
            let (top, bot) = (origin_y + inner.h + sep, origin_y - inner.d - pad);
            if !ctx.is_measuring() {
                ctx.emit_rule(origin_x, top, w, rule);
                ctx.emit_rule(origin_x, bot, w, rule);
                ctx.emit_rule(origin_x, bot, rule, top + rule - bot);
                ctx.emit_rule(origin_x + w - rule, bot, rule, top + rule - bot);
            }
            MathBox::with_dims(w, inner.h + pad, inner.d + pad)
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

/// Tight `\ialign`s whose interline glue TeX decides by `\baselineskip`, `\lineskip` and
/// `\lineskiplimit` (§679):
/// - `subarray` (`\substack`): script style, `\baselineskip` = fontdimens 10 + 12 of
///   `\scriptfont3`, `\lineskip` = `\lineskiplimit` = 3 x its fontdimen 8, `\vcenter`ed;
/// - `smallmatrix`: script style, `\baselineskip 6\ex@`, `\lineskip` = `\lineskiplimit` =
///   `1.5\ex@` (`\ex@` = .2326ex), `\thickspace` between columns, `\,` on both sides,
///   `\vcenter`ed;
/// - `\ooalign`: current style, `\baselineskip 0` with no lineskip, so the rows share one
///   baseline, in a `\vtop`;
/// - `\@vereq`: current style, always `\lineskip -.5pt`, in a `\vbox` lowered .5pt.
#[allow(clippy::too_many_arguments)]
fn layout_script_array<'a, 'p>(
    ctx: &mut MathLayoutContext<'a, 'p>,
    rows: &[Vec<MathNode<'a>>],
    col_alignments: &[ColAlign],
    env_kind: MatrixEnvKind,
    style: MathStyle,
    origin_x: f32,
    origin_y: f32,
) -> MathBox {
    let params = ctx.font_params(style);
    let axis_h = (params.axis_height as f32) * ctx.font_tier.font_size(style) * 0.001;
    let unit = ctx.base_font_size / 10.0;
    let (cell_style, baselineskip, lineskip, lineskiplimit, col_sep, outer) = match env_kind {
        MatrixEnvKind::Subarray => {
            // amsmath: \baselineskip is fontdimens 10 + 12 of \scriptfont2 (cmsy7's num3 and
            // denom2); \lineskip and \lineskiplimit are 3 × fontdimen 8 of \scriptfont3 (cmex7).
            let s = ctx.font_tier.font_size(MathStyle::Script);
            let sy = ctx.font_params(MathStyle::Script);
            let ls = 3.0 * 0.04857 * s;
            (MathStyle::Script, ((sy.num3 + sy.denom2) as f32) * 0.001 * s, ls, ls, 0.0, 0.0)
        }
        MatrixEnvKind::Overlay => (style, 0.0, 0.0, f32::NEG_INFINITY, 0.0, 0.0),
        MatrixEnvKind::Vereq => (style, 0.0, -0.5 * unit, f32::INFINITY, 0.0, 0.0),
        _ => {
            let text = ctx.font_params(MathStyle::Text);
            let ex = (text.accent_base_height as f32) * ctx.font_tier.font_size(MathStyle::Text) * 0.001;
            let ex_at = 0.2326 * ex;
            let thin = ctx.skip_width(crate::math::math_ast::MathSkip::Mu(3.0), style);
            (MathStyle::Script, 6.0 * ex_at, 1.5 * ex_at, 1.5 * ex_at, 0.2777 * ctx.base_font_size, thin)
        }
    };

    let n_cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut col_w = vec![0.0f32; n_cols];
    let mut row_hd = vec![(0.0f32, 0.0f32); rows.len()];
    let mut cells: Vec<(usize, usize, f32)> = Vec::with_capacity(rows.len() * n_cols);
    let size = ctx.font_tier.font_size(cell_style) * 0.001;
    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            let start = ctx.out_ops_len();
            let mut b = layout_node(ctx, cell, cell_style, 0.0, 0.0);
            b.w += node_italic_correction_styled(cell, ctx.face_override, ctx.font_family, cell_style) * size;
            cells.push((start, ctx.out_ops_len(), b.w));
            col_w[c] = col_w[c].max(b.w);
            row_hd[r].0 = row_hd[r].0.max(b.h);
            row_hd[r].1 = row_hd[r].1.max(b.d);
        }
    }

    // Baselines measured down from the top of the first row.
    let mut baselines = Vec::with_capacity(rows.len());
    let mut y = row_hd[0].0;
    baselines.push(y);
    for r in 1..rows.len() {
        let gap = baselineskip - row_hd[r - 1].1 - row_hd[r].0;
        y += row_hd[r - 1].1 + if gap < lineskiplimit { lineskip } else { gap } + row_hd[r].0;
        baselines.push(y);
    }
    let last_d = row_hd[rows.len() - 1].1;
    let total = y + last_d;
    let top = origin_y
        + match env_kind {
            MatrixEnvKind::Overlay => row_hd[0].0,
            MatrixEnvKind::Vereq => total - last_d - 0.5 * unit,
            _ => 0.5 * total + axis_h,
        };

    let mut idx = 0;
    for (r, row) in rows.iter().enumerate() {
        let mut x = origin_x + outer;
        #[allow(clippy::needless_range_loop)]
        for c in 0..row.len() {
            let (start, end, w) = cells[idx];
            idx += 1;
            let align = col_alignments.get(c).or(col_alignments.last()).copied().unwrap_or(ColAlign::Center);
            let dx = match align {
                ColAlign::Left => 0.0,
                ColAlign::Right => col_w[c] - w,
                ColAlign::Center => 0.5 * (col_w[c] - w),
            };
            ctx.shift_ops_range(start, end, x + dx, top - baselines[r]);
            x += col_w[c] + col_sep;
        }
    }
    let width = col_w.iter().sum::<f32>() + col_sep * n_cols.saturating_sub(1) as f32 + 2.0 * outer;
    let h = top - origin_y;
    MathBox::with_dims(width, h, total - h)
}

/// amsmath `\dddot` / `\ddddot`: `\mathop{\kern0pt#1}\limits^{S}` where S is
/// `\vbox to-1.4\ex@{\kern-2\ex@\hbox{\,\normalfont...}\vss}`, text-size periods in a box of
/// height -1.4\ex@ (\ex@ = .2326ex), placed by Rule 13a.
#[allow(clippy::too_many_arguments)]
fn layout_amsmath_dots_accent<'a, 'p>(
    ctx: &mut MathLayoutContext<'a, 'p>,
    nuc_box: MathBox,
    nuc_start: usize,
    nuc_end: usize,
    n_dots: usize,
    style: MathStyle,
    origin_x: f32,
    origin_y: f32,
) -> MathBox {
    let params = ctx.font_params(style);
    let scale = ctx.font_tier.font_size(style) * 0.001;
    let text = ctx.base_font_size;
    let ex_at = 0.2326 * (ctx.font_params(MathStyle::Text).accent_base_height as f32) * ctx.font_tier.font_size(MathStyle::Text) * 0.001;
    let dot = crate::math::math_tfm::cmr10_metric(b'.');
    let (dot_w, dot_h) = (dot.w * 0.001 * text, dot.h * 0.001 * text);
    let thin = text / 6.0;
    let sup_w = thin + n_dots as f32 * dot_w;
    let width = nuc_box.w.max(sup_w);
    // A narrower limit is reboxed (§715), which hpacks the vbox and clamps its height to 0.
    let sup_h = if sup_w < width { 0.0 } else { -1.4 * ex_at };
    ctx.shift_ops_range(nuc_start, nuc_end, 0.5 * (width - nuc_box.w), 0.0);
    let shift_up = ((params.big_op_spacing1 as f32) * scale).max((params.big_op_spacing3 as f32) * scale);
    let sup_y = origin_y + nuc_box.h + shift_up;
    if !ctx.is_measuring() {
        // The hbox's top sits 2\ex@ below the box's (negative) top.
        let dots_y = sup_y + 0.6 * ex_at - dot_h;
        let mut x = origin_x + 0.5 * (width - sup_w) + thin;
        for _ in 0..n_dots {
            ctx.emit_glyph_run(x, dots_y, Face::Roman, text, ".");
            x += dot_w;
        }
    }
    let bos5 = (params.big_op_spacing5 as f32) * scale;
    MathBox::with_dims(width, (sup_y + sup_h + bos5 - origin_y).max(nuc_box.h), nuc_box.d)
}

/// amsmath's arrow accents: the cmsy end pieces of the `\arrowfill@` row (`\relbar` is
/// the minus sign "00) and whether it goes below the content.
fn amsmath_arrow_fill(accent: &str) -> Option<(u8, u8, bool)> {
    Some(match accent {
        "overrightarrow" => (0x00, 0x21, false),
        "overleftarrow" => (0x20, 0x00, false),
        "overleftrightarrow" => (0x20, 0x21, false),
        "underrightarrow" => (0x00, 0x21, true),
        "underleftarrow" => (0x20, 0x00, true),
        "underleftrightarrow" => (0x20, 0x21, true),
        _ => return None,
    })
}

/// amsmath `\overarrow@` / `\underarrow@`: the content `$\hfil#2#3\hfil$` (with its italic
/// correction) and an `\arrowfill@` row in the same style, stacked with no interline glue
/// (a `\vbox` above, a `\vtop` 1.3pt below). The row is `#1\mkern-7mu`, leaders of
/// `\mkern-2mu\relbar\mkern-2mu`, then `\mkern-7mu#3`; the smashed `\relbar` and the arrows'
/// negative depth leave it the arrow's height and no depth.
#[allow(clippy::too_many_arguments)]
fn layout_arrow_fill_accent<'a, 'p>(
    ctx: &mut MathLayoutContext<'a, 'p>,
    nuc_box: MathBox,
    nuc_start: usize,
    nuc_end: usize,
    ic: f32,
    left: u8,
    right: u8,
    below: bool,
    style: MathStyle,
    origin_x: f32,
    origin_y: f32,
) -> MathBox {
    let font_size = ctx.font_tier.font_size(style);
    let face = resolve_optical_face(Face::Symbol, style, ctx.font_family);
    let metric = |c: u8| {
        crate::math::math_tfm_faces::cm_face_metric(face, c).unwrap_or_else(|| crate::math::math_tfm::cmsy10_metric(c))
    };
    let em = |v: f32| v * 0.001 * font_size;
    let mu = ctx.skip_width(crate::math::math_ast::MathSkip::Mu(1.0), style);
    let (lw, rw, bar_w) = (em(metric(left).w), em(metric(right).w), em(metric(0x00).w));
    let row_h = em(metric(0x21).h);
    let content_w = nuc_box.w + ic;
    let width = content_w.max(lw + rw - 14.0 * mu);
    ctx.shift_ops_range(nuc_start, nuc_end, 0.5 * (width - content_w), 0.0);

    let below_gap = 1.3 * (ctx.base_font_size / 10.0);
    let row_y = if below { origin_y - nuc_box.d - below_gap - row_h } else { origin_y + nuc_box.h };
    if !ctx.is_measuring() {
        // Leaders: whole `\mkern-2mu\relbar\mkern-2mu` boxes fill the middle.
        let (fill_start, fill_end) = (origin_x + lw - 7.0 * mu, origin_x + width - rw + 7.0 * mu);
        let step = bar_w - 4.0 * mu;
        if step > 0.0 {
            let mut x = fill_start;
            while x + step <= fill_end + 0.01 {
                ctx.emit_glyph_run(x - 2.0 * mu, row_y, face, font_size, byte_str(0x00));
                x += step;
            }
        }
        ctx.emit_glyph_run(origin_x, row_y, face, font_size, byte_str(left));
        ctx.emit_glyph_run(origin_x + width - rw, row_y, face, font_size, byte_str(right));
    }
    if below {
        MathBox::with_dims(width, nuc_box.h, nuc_box.d + below_gap + row_h)
    } else {
        MathBox::with_dims(width, nuc_box.h + row_h, nuc_box.d)
    }
}

/// True when `node` is the single ordinary character TeX keeps as a `math_char` nucleus
/// (§1186), as opposed to a sub-formula box.
fn is_single_ord_char(node: &MathNode) -> bool {
    match node {
        MathNode::Variable(_) => true,
        MathNode::Number(s) => s.chars().count() == 1,
        MathNode::Symbol { kind, .. } => *kind == MathAtomKind::Ord,
        MathNode::Row(items) => items.len() == 1 && is_single_ord_char(&items[0]),
        _ => false,
    }
}

/// An Op atom with an arbitrary nucleus (TeX §749-§750, Rule 13): a character nucleus is
/// centred on the axis, then the scripts become limits (Rule 13a) under `\limits`, or under
/// `\displaylimits` in display style, and are attached beside it otherwise.
#[allow(clippy::too_many_arguments)]
fn layout_op_limits<'a, 'p>(
    ctx: &mut MathLayoutContext<'a, 'p>,
    content: &MathNode<'a>,
    mode: u8,
    sub: Option<&MathNode<'a>>,
    sup: Option<&MathNode<'a>>,
    style: MathStyle,
    origin_x: f32,
    origin_y: f32,
) -> MathBox {
    use crate::math::math_ast::{OP_LIMITS_BOXED_NUCLEUS, OP_LIMITS_LIMITS, OP_LIMITS_MODE_MASK, OP_LIMITS_NOLIMITS};
    let params = ctx.font_params(style);
    let font_size = ctx.font_tier.font_size(style);
    let scale = font_size * 0.001;
    let axis_h = (params.axis_height as f32) * scale;

    let is_char = mode & OP_LIMITS_BOXED_NUCLEUS == 0 && is_single_ord_char(content);
    // Rule 13: in display style a cmex character is replaced by its next larger variant.
    let larger;
    let content = match content {
        MathNode::Symbol { kind, face, glyph, code, mathcode }
            if style.is_display() && (mathcode >> 8) & 0xF == 3 && ctx.font_family == metrics::FontFamily::ComputerModern =>
        {
            match crate::math::math_delim::cmex10_next_larger((mathcode & 0xFF) as u8) {
                Some(next) => {
                    larger = MathNode::Symbol {
                        kind: *kind,
                        face: *face,
                        glyph,
                        code: *code,
                        mathcode: (mathcode & 0xFF00) | next as u16,
                    };
                    &larger
                }
                None => content,
            }
        }
        _ => content,
    };
    let nuc_start = ctx.out_ops_len();
    let mut nuc = layout_node(ctx, content, style, 0.0, 0.0);
    let nuc_end = ctx.out_ops_len();
    // clean_box keeps the last character's italic correction; for a character nucleus it
    // is also the `delta` that offsets the limits (§749-§750).
    let ic = node_italic_correction_styled(content, ctx.face_override, ctx.font_family, style) * scale;
    let mut dy = 0.0;
    let delta = if is_char { ic } else { 0.0 };
    if is_char {
        let v = 0.5 * (nuc.h - nuc.d) - axis_h;
        dy = -v;
        nuc.h -= v;
        nuc.d += v;
    }

    let limits = match mode & OP_LIMITS_MODE_MASK {
        OP_LIMITS_LIMITS => true,
        OP_LIMITS_NOLIMITS => false,
        _ => style.is_display(),
    };
    if sub.is_none() && sup.is_none() {
        ctx.shift_ops_range(nuc_start, nuc_end, origin_x, origin_y + dy);
        return MathBox::with_dims(nuc.w + ic, nuc.h, nuc.d);
    }

    let sup_start = ctx.out_ops_len();
    let sup_box = sup.map_or(MathBox::zero(), |s| layout_script_box(ctx, s, style.sup_style()));
    let sup_end = ctx.out_ops_len();
    let sub_start = ctx.out_ops_len();
    let sub_box = sub.map_or(MathBox::zero(), |s| layout_script_box(ctx, s, style.sub_style()));
    let sub_end = ctx.out_ops_len();

    if !limits {
        let op_w = nuc.w + ic;
        let (sup_x_rel, sub_x_rel, sup_shift, sub_shift) = BigOpShiftResolver::resolve_side_scripts(
            op_w,
            delta,
            nuc.h,
            nuc.d,
            sup.is_some(),
            sub.is_some(),
            style,
            sup_box.d,
            sub_box.h,
            scale,
            params,
            ctx.script_drops(style),
        );
        let script_space = 0.5 * (ctx.base_font_size / 10.0);
        ctx.shift_ops_range(nuc_start, nuc_end, origin_x, origin_y + dy);
        ctx.shift_ops_range(sup_start, sup_end, origin_x + sup_x_rel, origin_y + sup_shift);
        ctx.shift_ops_range(sub_start, sub_end, origin_x + sub_x_rel, origin_y - sub_shift);
        return BigOpShiftResolver::compute_side_scripts_box(
            op_w,
            0.0,
            nuc.h,
            nuc.d,
            sup.is_some(),
            sup_x_rel,
            sup_box.w + script_space,
            sup_shift,
            sup_box.h,
            sub.is_some(),
            sub_x_rel,
            sub_box.w + script_space,
            sub_shift,
            sub_box.d,
        );
    }

    // The shifted nucleus is re-packed by clean_box, so its depth (or height) cannot
    // go below zero.
    nuc.h = nuc.h.max(0.0);
    nuc.d = nuc.d.max(0.0);
    let half_ic = 0.5 * delta;
    let (max_w, op_x, sup_x, sub_x) = OperatorCenteringEngine::center_limits_positioned(
        origin_x,
        nuc.w + ic,
        sup_box.w,
        sub_box.w,
        sup.is_some(),
        sub.is_some(),
        half_ic,
    );
    let metrics = BigOpShiftResolver::resolve_limit_metrics(ctx.font_family, font_size, scale, params);
    let (op_top_y, op_bot_y) = (origin_y + nuc.h, origin_y - nuc.d);
    let (sup_y, sub_y) =
        LimitClearanceGovernor::compute_limit_positions(op_top_y, op_bot_y, sup_box.d, sub_box.h, &metrics);
    ctx.shift_ops_range(nuc_start, nuc_end, op_x, origin_y + dy);
    if sup.is_some() {
        ctx.shift_ops_range(sup_start, sup_end, sup_x, sup_y);
    }
    if sub.is_some() {
        ctx.shift_ops_range(sub_start, sub_end, sub_x, sub_y);
    }
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
        (params.big_op_spacing5 as f32) * scale,
    )
}

/// A text character's width: the exact TFM width for Computer Modern, the AFM width otherwise.
#[inline]
fn cm_or_base14_width(ch: char, face: Face, font_size: f32, family: metrics::FontFamily) -> f32 {
    if family == metrics::FontFamily::ComputerModern {
        measure_char(ch, face, font_size, family)
    } else {
        metrics::width_family(ch, face, font_size, family)
    }
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
            let w_dot = cm_or_base14_width('.', Face::Roman, font_size, ctx.font_family);
            let (h_u, d_u) = char_height_depth('.', Face::Roman, ctx.font_family);
            let h_dot = h_u * 0.001 * font_size;
            let d_dot = d_u * 0.001 * font_size;
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
                (Face::Symbol, crate::math::math_delim::byte_str(0x01), crate::math::math_tfm::cmsy10_metric(0x01).w * 0.001 * font_size)
            } else {
                (Face::Symbol, "·", 0.278 * font_size)
            };
            let thin = (3.0 / 18.0) * font_size; // TeX thinmuskip = 3 mu
            let step = w_dot + thin;
            ctx.emit_glyph_run(origin_x, origin_y, face, font_size, dot_str);
            ctx.emit_glyph_run(origin_x + step, origin_y, face, font_size, dot_str);
            ctx.emit_glyph_run(origin_x + 2.0 * step, origin_y, face, font_size, dot_str);
            let (h, d) = if is_cm {
                let m = crate::math::math_tfm::cmsy10_metric(0x01);
                (m.h * 0.001 * font_size, m.d * 0.001 * font_size)
            } else {
                (axis_h + 0.15 * font_size, (0.15 * font_size - axis_h).max(0.0))
            };
            MathBox::with_dims(3.0 * w_dot + 2.0 * thin, h, d)
        }

        EllipsisKind::Vertical => {
            let w_dot = cm_or_base14_width('.', Face::Roman, font_size, ctx.font_family);
            let (h_u, d_u) = char_height_depth('.', Face::Roman, ctx.font_family);
            let h_dot = h_u * 0.001 * font_size;
            let d_dot = d_u * 0.001 * font_size;
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
            let w_dot = cm_or_base14_width('.', Face::Roman, font_size, ctx.font_family);
            let (h_u, _) = char_height_depth('.', Face::Roman, ctx.font_family);
            let h_dot = h_u * 0.001 * font_size;
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
        crate::math::math_tfm::cmex10_metric(0x7A).w * (font_size * 0.001)
    } else {
        0.45 * font_size
    };
    let (brace_h, gap) = if ctx.font_family == metrics::FontFamily::ComputerModern {
        (
            crate::math::math_tfm::cmex10_metric(0x7A).h * (font_size * 0.001),
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

    // Rule 13a: the limits clear the braced box by max(xi9, xi11 - d) above and
    // max(xi10, xi12 - h) below.
    let sup_clearance = ((params.big_op_spacing1 as f32) * scale)
        .max((params.big_op_spacing3 as f32) * scale - sup_box.d)
        .max(2.0 * size_ratio - sup_box.d);
    let extra_pad = (params.big_op_spacing5 as f32) * scale;
    let sup_y = origin_y + inner_top_h + sup_clearance + sup_box.d;
    let sup_x = origin_x + (total_w - sup_box.w) * 0.5;

    let sub_clearance = ((params.big_op_spacing2 as f32) * scale)
        .max((params.big_op_spacing4 as f32) * scale - sub_box.h)
        .max(1.667 * size_ratio);
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
        (origin_y - sub_y + sub_box.d + extra_pad).max(inner_bot_d)
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
        crate::math::math_tfm::cmex10_metric(0x7A).w * (font_size * 0.001)
    } else {
        0.45 * font_size
    };
    let (brace_h, gap) = if ctx.font_family == metrics::FontFamily::ComputerModern {
        (
            crate::math::math_tfm::cmex10_metric(0x7A).h * (font_size * 0.001),
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

    // Rule 13a: the limits clear the braced box by max(xi9, xi11 - d) above and
    // max(xi10, xi12 - h) below.
    let sup_clearance = ((params.big_op_spacing1 as f32) * scale)
        .max((params.big_op_spacing3 as f32) * scale - sup_box.d)
        .max(2.0 * size_ratio - sup_box.d);
    let extra_pad = (params.big_op_spacing5 as f32) * scale;
    let sup_y = origin_y + inner_top_h + sup_clearance + sup_box.d;
    let sup_x = origin_x + (total_w - sup_box.w) * 0.5;

    let sub_clearance = ((params.big_op_spacing2 as f32) * scale)
        .max((params.big_op_spacing4 as f32) * scale - sub_box.h)
        .max(1.667 * size_ratio);
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
        (origin_y - sub_y + sub_box.d + extra_pad).max(inner_bot_d)
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
    // The formula is an hbox, whose height and depth start at zero (TeX §653).
    b.h = b.h.max(0.0);
    b.d = b.d.max(0.0);
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
    let is_left = data.arrow == "leftarrow";
    let is_both = data.arrow == "leftrightarrow";

    // amsmath `\ext@arrow`: mu kerns (before limit, after limit, before label, after label)
    // are 0359 for `\xrightarrow`, 3095 for `\xleftarrow`, 3399 for `\xleftrightarrow`.
    let (lim_pre, lim_post, lab_pre, lab_post) = if is_both {
        (3.0, 3.0, 9.0, 9.0)
    } else if is_left {
        (3.0, 0.0, 9.0, 5.0)
    } else {
        (0.0, 3.0, 5.0, 9.0)
    };
    let mu = |ctx: &MathLayoutContext<'a, 'p>, st: MathStyle| {
        (ctx.font_params(st).quad as f32) * ctx.font_tier.font_size(st) * (0.001 / 18.0)
    };

    let mut sup_box = MathBox::zero();
    let sup_start = ctx.out_ops_len();
    if let Some(ref sup_node) = data.sup {
        sup_box = layout_script_box(ctx, sup_node, sup_style);
    }
    let sup_end = ctx.out_ops_len();

    let mut sub_box = MathBox::zero();
    let sub_start = ctx.out_ops_len();
    if let Some(ref sub_node) = data.sub {
        sub_box = layout_script_box(ctx, sub_node, sub_style);
    }
    let sub_end = ctx.out_ops_len();

    let arrow_size = ctx.font_tier.font_size(MathStyle::Display);

    let (arrow_w, arrow_y, is_cm) = if ctx.font_family == metrics::FontFamily::ComputerModern {
        let code = if is_left { 0x20u8 } else { 0x21u8 };
        let w = (crate::math::math_tfm::cmsy10_metric(code).w * 0.001) * arrow_size;
        (w, origin_y, true)
    } else {
        let sym_idx = if is_left { 0xAC } else { 0xAE };
        let w = (metrics::SYMBOL_WIDTHS[sym_idx] as f32 * 0.001) * arrow_size;
        let shaft_y = origin_y + axis_h;
        (w, shaft_y - 0.13 * arrow_size, false)
    };

    // `\arrowfill@` in display style: arrow head(s) and one relbar overlapped by -7mu each side.
    let bar_w = if is_cm {
        (crate::math::math_tfm::cmsy10_metric(0x00).w * 0.001) * arrow_size
    } else {
        arrow_w
    };
    let head_w = if is_both { 2.0 * arrow_w } else { arrow_w + bar_w };
    let fill_w = head_w - 14.0 * mu(ctx, MathStyle::Display);

    // The arrow is stretched to the widest of itself and both `\scriptstyle` labels
    // (each padded by the label kerns, present even when the label is empty).
    let label_pad = (lab_pre + lab_post) * mu(ctx, MathStyle::Script);
    let label_w = |ctx: &mut MathLayoutContext<'a, 'p>, node: &Option<MathNode<'a>>, laid: MathBox, st: MathStyle| {
        match node {
            Some(_) if st == MathStyle::Script || st == MathStyle::CrampedScript => laid.w,
            Some(n) => {
                let mut dry = ctx.dry_context();
                layout_script_box(&mut dry, n, MathStyle::Script).w
            }
            None => 0.0,
        }
    };
    let sup_lab_w = label_w(ctx, &data.sup, sup_box, sup_style);
    let sub_lab_w = label_w(ctx, &data.sub, sub_box, sub_style);
    let target_w = fill_w.max(sup_lab_w + label_pad).max(sub_lab_w + label_pad);

    let (arrow_geom, left_x, right_x) = crate::math::math_delim::ExtensibleArrowEngine::compute_arrow_geometry(
        origin_x,
        origin_y,
        axis_h,
        target_w,
        arrow_w,
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

    // Limits of a `\mathop` box (Rule 13a): the nucleus is the stretched arrow box, whose
    // depth is zero because the relbars are smashed and hpack starts from zero.
    let nuc_h = if is_cm {
        (crate::math::math_tfm::cmsy10_metric(0x21).h * 0.001) * arrow_size
    } else {
        axis_h + rule_t * 0.5 + 2.0
    };
    let nuc_d = 0.0f32;
    let mut total_h = nuc_h;
    let mut total_d = nuc_d;

    if data.sup.is_some() {
        let m = mu(ctx, sup_style);
        let lim_w = (lim_pre + lim_post) * m + sup_box.w;
        let sup_x = origin_x + (target_w - lim_w) * 0.5 + lim_pre * m;
        let clr = ((params.big_op_spacing1 as f32) * scale)
            .max((params.big_op_spacing3 as f32) * scale - sup_box.d);
        let sup_y = origin_y + nuc_h + clr + sup_box.d;
        ctx.shift_ops_range(sup_start, sup_end, sup_x, sup_y);
        total_h = sup_y - origin_y + sup_box.h + (params.big_op_spacing5 as f32) * scale;
    }

    if data.sub.is_some() {
        let m = mu(ctx, sub_style);
        let lim_w = (lim_pre + lim_post) * m + sub_box.w;
        let sub_x = origin_x + (target_w - lim_w) * 0.5 + lim_pre * m;
        let clr = ((params.big_op_spacing2 as f32) * scale)
            .max((params.big_op_spacing4 as f32) * scale - sub_box.h);
        let sub_y = origin_y - nuc_d - clr - sub_box.h;
        ctx.shift_ops_range(sub_start, sub_end, sub_x, sub_y);
        total_d = origin_y - sub_y + sub_box.d + (params.big_op_spacing5 as f32) * scale;
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
