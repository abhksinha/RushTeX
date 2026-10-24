//! High-performance mathematical vector layout engine.
//!
//! Implements TeX Appendix G rules for math layout, including atom spacing,
//! subscripts/superscripts, vertical fractions, radicals, big operators,
//! extensible delimiters, and matrices.

use crate::backend::fonts::{self as metrics, Face, FontFamily};
use crate::layout::engine::{Paint, Sp};
use crate::math::fonts::MathFont;
use crate::math::math_ast::{
    BigOpData, ColAlign, DelimitedData, FontVariantTier, MathContextFlags, MathNode, MathStyle,
    MatrixData, MatrixDelim, MatrixEnvKind, StyleFrameStack,
};
use crate::math::math_box::MathBox;
use crate::math::math_fonts::{
    char_height_depth, delimiter_metrics, glyph_top_accent_offset, hbar_metrics,
    lookup_accent_metrics,
    matrix_metrics, measure_char, resolve_cm_symbol, resolve_optical_face, cm_alphabet_face,
    symbol_height_depth_family, symbol_italic_correction, char_hd, char_ic,
    ASCII_GLYPH_STRS,
};
use crate::math::math_bigop::{
    BigOpShiftResolver, BigOpVariantTier, IntegralOverhangCorrector, LimitClearanceGovernor,
    OperatorCenteringEngine, OperatorLimitSwitch, Rule13OperatorAxis,
};
use crate::math::math_delim::{
    byte_str, resolve_cmex_delimiter, DelimiterBarRenderer,
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
    /// The text font around the formula, which amsmath's `\text` keeps.
    pub text_face: Face,
    pub out_ops: Option<&'p mut Vec<Paint>>,
    pub text_pool: Option<&'p mut Vec<&'a str>>,
    pub font_tier: FontVariantTier,
    /// TeX's math font parameters at the text, script and scriptscript sizes, computed once
    /// per formula: every node asks for them, and `MathParams::at` is thirty exact scalings.
    pub font_params_by_tier: [crate::math::math_fonts::MathParams; 3],
    pub style_stack: StyleFrameStack,
    pub flags: MathContextFlags,
    pub target_middle_delim_h: Option<f32>,
    /// Colour of the `\color` scope being laid out (black outside any).
    pub color: crate::frontend::ast::PackedColor,
    /// How much each `\medmuskip` of the outermost row shrinks: a display packed to a
    /// width below its natural one (tex.web §1201). Taken by the first row laid out.
    pub top_med_shrink: f32,
    /// `\displaywidth` of a display being laid out (0 elsewhere): amsmath's `align` spreads
    /// its column pairs over it.
    pub display_width: f32,
    /// The number the display's first numbered row gets (amsmath measures the numbers).
    pub first_equation: u32,
    /// Laying out an alignment nested in a display alignment's cell.
    pub inner_align: bool,
    /// Laying out an hbox of text (`MathNode::Raise`: logos, `\raisebox`): its letters are
    /// text characters, so no math italic correction follows them (tex.web §752 is math only).
    pub text_box: bool,
    /// Op count after each row of the display's outermost alignment, when the caller asks.
    pub row_ends: Option<&'p mut Vec<usize>>,
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
        let font_tier = FontVariantTier::new(base_font_size);
        let font_params_by_tier = [
            crate::math::math_fonts::MathParams::at(font_family, font_tier.font_size(MathStyle::Text)),
            crate::math::math_fonts::MathParams::at(font_family, font_tier.font_size(MathStyle::Script)),
            crate::math::math_fonts::MathParams::at(font_family, font_tier.font_size(MathStyle::ScriptScript)),
        ];
        Self {
            base_font_size,
            font_family,
            face_override,
            text_face: Face::Roman,
            out_ops,
            text_pool,
            font_tier,
            font_params_by_tier,
            style_stack: StyleFrameStack::new(root_style),
            flags,
            target_middle_delim_h: None,
            color: crate::frontend::ast::PackedColor::BLACK,
            top_med_shrink: 0.0,
            display_width: 0.0,
            first_equation: 1,
            inner_align: false,
            text_box: false,
            row_ends: None,
        }
    }

    #[inline(always)]
    pub fn dry_context<'d>(&self) -> MathLayoutContext<'a, 'd> {
        MathLayoutContext {
            base_font_size: self.base_font_size,
            font_family: self.font_family,
            face_override: self.face_override,
            text_face: self.text_face,
            out_ops: None,
            text_pool: None,
            font_tier: self.font_tier,
            font_params_by_tier: self.font_params_by_tier,
            style_stack: self.style_stack,
            flags: self.flags,
            target_middle_delim_h: self.target_middle_delim_h,
            color: self.color,
            top_med_shrink: 0.0,
            display_width: self.display_width,
            first_equation: self.first_equation,
            inner_align: self.inner_align,
            text_box: self.text_box,
            row_ends: None,
        }
    }

    /// Moves the ops from `first` on, set about the formula's own reference point, to the
    /// page position (`x`, `y`) in scaled points: offsets in points are exact in `f32`,
    /// page positions are not.
    fn place_ops(&mut self, first: usize, x: Sp, y: Sp) {
        if let Some(ops) = self.out_ops.as_mut() {
            for op in &mut ops[first..] {
                op.shift_x(x);
                op.shift_y(y);
            }
        }
    }

    #[inline(always)]
    pub fn out_ops_len(&self) -> usize {
        self.out_ops.as_ref().map(|o| o.len()).unwrap_or(0)
    }

    /// Moves the ops from `start` on behind the last one (emitted later, painted first).
    pub fn move_last_op_to(&mut self, start: usize) {
        self.move_last_ops_to(start, 1);
    }

    /// Swaps the adjacent op ranges `start..mid` and `mid..end`.
    #[inline]
    pub fn swap_op_ranges(&mut self, start: usize, mid: usize, end: usize) {
        if let Some(ops) = self.out_ops.as_mut() {
            if start <= mid && mid <= end && end <= ops.len() {
                ops[start..end].rotate_left(mid - start);
            }
        }
    }

    /// Moves the last `n` ops to index `start`, so they are drawn in TeX's list order.
    #[inline]
    pub fn move_last_ops_to(&mut self, start: usize, n: usize) {
        if let Some(ops) = self.out_ops.as_mut() {
            if start + n <= ops.len() {
                ops[start..].rotate_right(n);
            }
        }
    }

    #[inline(always)]
    pub fn shift_ops_range(&mut self, start: usize, end: usize, dx: f32, dy: f32) {
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        let (dx, dy) = (Sp::pt(dx), Sp::pt(dy));
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
    pub fn font_params(&self, style: MathStyle) -> crate::math::math_fonts::MathParams {
        // Display/Text and their cramped forms share the text size (styles 0–3), Script and
        // CrampedScript the script size (4–5), ScriptScript and its cramped form (6–7) theirs.
        self.font_params_by_tier[((style as usize) & 0x07).saturating_sub(2) >> 1]
    }

    /// `(sup_drop, sub_drop)` in points from the script-size font of `style` (TeX §756).
    #[inline(always)]
    /// Width in points of a math skip in `style` (TeX §716: 1mu is 1/18 of the math quad).
    pub fn skip_width(&self, skip: crate::math::math_ast::MathSkip, style: MathStyle) -> f32 {
        use crate::math::math_ast::MathSkip;
        match skip {
            // `\hskip<em>em`: the quad of the current (text) font, as `scan_dimen` scales it.
            MathSkip::TextEm(em) if self.font_family == FontFamily::ComputerModern => {
                crate::backend::scaled::em_mul(em, crate::backend::scaled::face_quad_pt(self.font_family, self.text_face, self.base_font_size))
            }
            MathSkip::TextEm(em) => em * self.base_font_size,
            MathSkip::Pt(pt) => pt,
            MathSkip::Mu(mu) | MathSkip::MuGlue(mu, _) => {
                crate::backend::scaled::mu_pt(mu, self.font_params(style).quad)
            }
            MathSkip::TextSpace
                if self.font_family == FontFamily::ComputerModern
                    && matches!(
                        self.face_override,
                        Some(Face::Italic | Face::Bold | Face::BoldItalic | Face::Slanted | Face::SmallCaps | Face::Mono)
                    ) =>
            {
                let size = self.font_tier.font_size(style);
                let face = self.face_override.unwrap_or(Face::Roman);
                crate::backend::scaled::space_pt(self.font_family, face, size).unwrap_or(0.333_334 * size)
            }
            MathSkip::TextSpace if self.font_family == FontFamily::ComputerModern => {
                // fontdimen 2 of the roman font at the style's size.
                let size = self.font_tier.font_size(style);
                crate::backend::scaled::space_pt(self.font_family, Face::Roman, size).unwrap_or(0.333_334 * size)
            }
            MathSkip::TextSpace => self.skip_width(MathSkip::Mu(6.0), style),
            MathSkip::TextSpaceExtra => {
                let size = self.font_tier.font_size(style);
                let face = self.face_override.unwrap_or(Face::Roman);
                let extra = crate::backend::scaled::space_glue(self.font_family, face, size).extra;
                self.skip_width(MathSkip::TextSpace, style) + crate::backend::scaled::to_pt(extra as i64)
            }
        }
    }

    pub fn script_drops(&self, style: MathStyle) -> (f32, f32) {
        let script = style.sup_style();
        let p = self.font_params(script);
        (p.sup_drop, p.sub_drop)
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
                x: x.into(),
                y: y.into(),
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
                x: x.into(),
                y: y.into(),
                w: w.into(),
                h: h.into(),
                fill: true,
            });
        }
    }

    #[inline(always)]
    pub fn emit_vector_glyph(&mut self, x: f32, y: f32, size: f32, glyph_id: u16) {
        if let Some(ops) = self.out_ops.as_deref_mut() {
            ops.push(Paint::VectorGlyph {
                x: x.into(),
                y: y.into(),
                size,
                glyph_id,
            });
        }
    }

    #[inline(always)]
    pub fn emit_cid_glyph(&mut self, x: f32, y: f32, size: f32, glyph_id: u16) {
        if let Some(ops) = self.out_ops.as_deref_mut() {
            ops.push(Paint::CidGlyphRun {
                x: x.into(),
                y: y.into(),
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
    mb.w += node_italic_correction_styled(node, ctx.face_override, ctx.font_family, style, &ctx.font_tier);
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

/// TFM lig/kern program of the Computer Modern math italic font drawn for `face`.
fn cm_math_kern_table(face: Face) -> Option<&'static crate::backend::backend_ligkern::LigKernTable> {
    use crate::backend::backend_ligkern as lk;
    match face {
        Face::MathItalic => Some(&lk::CMMI10),
        Face::MathItalic7 => Some(&lk::CMMI7),
        Face::MathItalic5 => Some(&lk::CMMI5),
        Face::MathItalic6 => Some(&lk::CMMI6),
        Face::MathItalic8 => Some(&lk::CMMI8),
        Face::MathItalic9 => Some(&lk::CMMI9),
        Face::MathItalic12 => Some(&lk::CMMI12),
        _ => None,
    }
}

/// The font and position of a character noad without scripts, as `make_ord` sees it.
fn cm_math_char(ctx: &MathLayoutContext<'_, '_>, node: &MathNode<'_>, style: MathStyle) -> Option<(Face, u8)> {
    match node {
        MathNode::Variable(c) if c.is_ascii() => {
            Some((resolve_optical_face(ctx.face_override.unwrap_or(Face::MathItalic), ctx.font_tier.font_size(style), ctx.font_family), *c as u8))
        }
        MathNode::Symbol { face, code: Some(code), .. } => {
            Some((resolve_optical_face(*face, ctx.font_tier.font_size(style), ctx.font_family), *code))
        }
        _ => None,
    }
}

/// What TeX's `make_ord` (§752) changes between two adjacent atoms beyond the italic
/// correction `prev_ic` after the first: letters of one CM math alphabet take its kern and,
/// in a text font, lose the correction; an Ord character followed by a character of the
/// same font takes the font's TFM kern (and, in a text font with nonzero interword space,
/// loses the correction too).
fn char_pair_adjust(
    ctx: &MathLayoutContext<'_, '_>,
    prev: &MathNode<'_>,
    child: &MathNode<'_>,
    style: MathStyle,
    font_size: f32,
    prev_ic: f32,
) -> f32 {
    if ctx.font_family != FontFamily::ComputerModern {
        return 0.0;
    }
    if let (MathNode::Variable(c1), MathNode::Variable(c2)) = (prev, child) {
        if let Some((keep_ic, kern)) = crate::math::math_fonts::cm_alphabet_pair(ctx.face_override, *c1, *c2, font_size) {
            return kern - if keep_ic { 0.0 } else { prev_ic };
        }
    }
    if prev.atom_kind() != MathAtomKind::Ord {
        return 0.0;
    }
    match (cm_math_char(ctx, prev, style), cm_math_char(ctx, child, style)) {
        (Some((f1, k1)), Some((f2, k2))) if f1 == f2 => {
            if let Some(table) = cm_ligkern_table(f1) {
                kern_pt(table, k1, k2, font_size) - prev_ic
            } else if let Some(table) = cm_math_kern_table(f1) {
                kern_pt(table, k1, k2, font_size)
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

/// [`char_pair_adjust`] for two atoms of an inline formula that the line breaker holds as
/// separate items, each measured with its trailing italic correction.
pub fn inline_pair_adjust(
    prev: &MathNode<'_>,
    child: &MathNode<'_>,
    base_font_size: f32,
    font_family: FontFamily,
    text_face: Face,
) -> f32 {
    let char_like = |n: &MathNode<'_>| matches!(n, MathNode::Variable(_) | MathNode::Symbol { .. });
    if font_family != FontFamily::ComputerModern || !char_like(prev) || !char_like(child) {
        return 0.0;
    }
    let style = MathStyle::Text;
    let mut ctx = MathLayoutContext::new(base_font_size, font_family, None, None, None, style);
    ctx.text_face = text_face;
    let font_size = ctx.font_tier.font_size(style);
    let prev_ic = node_italic_correction_styled(prev, ctx.face_override, ctx.font_family, style, &ctx.font_tier);
    char_pair_adjust(&ctx, prev, child, style, font_size, prev_ic)
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
                let kern = if i + 1 < w.len { kern_pt(w.table, c, w.glyphs[i + 1], font_size) } else { 0.0 };
                measure_char(c as char, face, font_size, family) + kern
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
            gx += kern_pt(w.table, c, w.glyphs[i + 1], font_size);
        }
    }
}

/// TFM italic correction (1000 units per em) of a Computer Modern glyph in a sized face.
/// A TFM kern between two characters in points, exact (`store_scaled`, §571).
#[inline]
fn kern_pt(table: &crate::backend::backend_ligkern::LigKernTable, l: u8, r: u8, size: f32) -> f32 {
    crate::backend::scaled::Scaler::at_pt(size).scale(table.kern_fix(l, r)) as f32 / 65536.0
}

/// Italic correction in points of code `c` in `face` at `size`, exact from the face's TFM.
fn cm_ic_pt(face: Face, c: u8, size: f32) -> f32 {
    crate::math::math_tfm::face_tfm(face).map_or(0.0, |t| crate::math::math_tfm::GlyphDims::of(t, c, size).ic)
}

/// The italic correction in points of any mathematical nucleus, from the fonts of `style`
/// at the sizes of `tier`; exact (TFM) under Computer Modern.
pub fn node_italic_correction_styled(
    node: &MathNode,
    face_override: Option<Face>,
    family: FontFamily,
    style: MathStyle,
    tier: &FontVariantTier,
) -> f32 {
    let size = tier.font_size(style);
    let k = size * 0.001;
    match node {
        MathNode::MathChoice(choice) => {
            let branch = match style.tier() {
                0 => &choice.display,
                1 => &choice.text,
                2 => &choice.script,
                _ => &choice.script_script,
            };
            node_italic_correction_styled(branch, face_override, family, style, tier)
        }
        MathNode::FamilyChoice(choice) => {
            let branch = if family == FontFamily::ComputerModern { &choice.0 } else { &choice.1 };
            node_italic_correction_styled(branch, face_override, family, style, tier)
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
                    let sized = crate::math::math_fonts::resolve_optical_face(alpha_face, size, family);
                    return cm_ic_pt(sized, code, size);
                }
            }
            if face == Face::Symbol && ch.is_ascii_uppercase() && family != FontFamily::ComputerModern {
                if let Some(sg) = crate::math::fonts::script_glyph(*ch, family) {
                    return sg.ic as f32 * k;
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
                            return ic as f32 * k;
                        }
                    }
                }
            }
            // Italic correction comes from the font of the current size (cmmi7 in script style).
            let face = crate::math::math_fonts::resolve_optical_face(face, size, family);
            char_ic(*ch, face, size, family)
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
            cm_ic_pt(crate::math::math_fonts::resolve_optical_face(f, size, family), c, size)
        }
        MathNode::Symbol { code: Some(c), face, glyph, .. } => {
            if family == FontFamily::ComputerModern {
                let eff = resolve_cm_symbol(*c).map(|(f, _, code)| (f, code)).unwrap_or((*face, *c));
                let eff_face = crate::math::math_fonts::resolve_optical_face(
                    face_override.unwrap_or(eff.0),
                    size,
                    family,
                );
                if eff_face.is_symbol() || eff_face.is_math_italic() || eff_face == Face::Extension {
                    cm_ic_pt(eff_face, eff.1, size)
                } else {
                    0.0
                }
            } else {
                let sym_b = glyph
                    .chars()
                    .next()
                    .and_then(crate::font_metrics::unicode_to_symbol_byte)
                    .unwrap_or(*c);
                symbol_italic_correction(sym_b) * k
            }
        }
        MathNode::Symbol { code: None, .. } => 0.0,
        MathNode::SubSup { .. } => 0.0,
        MathNode::StyleChange { face, content } => {
            node_italic_correction_styled(content, Some(*face), family, style, tier)
        }
        MathNode::Colored { content, .. } => node_italic_correction_styled(content, face_override, family, style, tier),
        MathNode::StyleScope { content, style: inner_style } => {
            node_italic_correction_styled(content, face_override, family, *inner_style, tier)
        }
        MathNode::Row(nodes) => {
            for n in nodes.iter().rev() {
                if !matches!(n, MathNode::Space(_)) {
                    return node_italic_correction_styled(n, face_override, family, style, tier);
                }
            }
            0.0
        }
        MathNode::BigOp(_) => 0.0,
        MathNode::Accent { accent, .. }
            if family == FontFamily::ComputerModern
                && (amsmath_arrow_fill(accent).is_some() || matches!(*accent, "dddot" | "ddddot" | "overline" | "underline")) =>
        {
            0.0
        }
        MathNode::Accent { nucleus, .. } => {
            node_italic_correction_styled(nucleus, face_override, family, style, tier)
        }
        MathNode::Overbrace(_) | MathNode::Underbrace(_)
        | MathNode::Overbracket(_) | MathNode::Underbracket(_)
        | MathNode::ExtensibleArrow(_) => 0.0,
        MathNode::Phantom { content, width, .. } => {
            if *width {
                node_italic_correction_styled(content, face_override, family, style, tier)
            } else {
                0.0
            }
        }
        MathNode::DimensionBox { content, zero_width, .. }
            if !*zero_width => {
                node_italic_correction_styled(content, face_override, family, style, tier)
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
const NULL_DELIMITER_SPACE: f32 = 78643.0 / 65536.0; // 1.2pt as `scan_dimen` reads it
/// `\scriptspace`, 0.5pt in latex.ltx at every size.
const SCRIPT_SPACE_PT: f32 = 0.5;

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
    let mut best: Option<(Face, u8, f32)> = None;
    let mut best_hd = f32::MIN;
    if let Some((face, code)) = cm_small_delimiter(delim) {
        for &s in &sizes[..n] {
            let f = resolve_optical_face(face, s, ctx.font_family);
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

/// A Computer Modern delimiter as `var_delimiter` (§706–§713) builds it, in scaled points:
/// its box before `shift_amount` (width with italic correction, height, depth) and how to
/// draw it.
struct CmDelimBox {
    face: Face,
    size: f32,
    code: u8,
    /// An extensible's recipe (top, mid, bot, rep) and its repeat count `n`.
    ext: Option<([u8; 4], u32)>,
    w: i32,
    h: i32,
    d: i32,
}

/// §706 `var_delimiter` for Computer Modern: the small variant, then the large, each tried
/// at the current size and then each larger size up to text size, following the fonts'
/// successor lists; the first character `v` tall or more wins, an extensible recipe wins
/// at once, else the tallest seen. `None` when the delimiter has no Computer Modern form.
fn cm_var_delimiter_sp(ctx: &MathLayoutContext, delim: &str, is_left: bool, v: i32, font_size: f32) -> Option<CmDelimBox> {
    use crate::backend::backend_tfm::{EXT_TAG, LIST_TAG};
    use crate::backend::scaled::Scaler;
    use crate::math::math_tfm::face_tfm;
    let text = ctx.font_tier.font_size(MathStyle::Text);
    let script = ctx.font_tier.font_size(MathStyle::Script);
    let mut sizes = [font_size, 0.0, 0.0];
    let mut n = 1;
    for sz in [script, text] {
        if sz > sizes[n - 1] + 0.01 {
            sizes[n] = sz;
            n += 1;
        }
    }
    let small = cm_small_delimiter(delim).map(|(f, c)| (f, c, false));
    // The radical's large variant (`\radical"270370`) starts at cmex "70.
    let large = if delim == "\\surd" { Some(0x70) } else { resolve_cmex_delimiter(delim, is_left).map(|(c, _)| c[0]) };
    let large = large.map(|c| (Face::Extension, c, true));
    let mut best: Option<(Face, f32, u8)> = None;
    let mut is_ext = false;
    let mut w = 0i32;
    'search: for (base, x, is_large) in [small, large].into_iter().flatten() {
        for &sz in &sizes[..n] {
            let face = if is_large {
                if sz < 0.85 * text { Face::Extension7 } else { Face::Extension }
            } else {
                resolve_optical_face(base, sz, ctx.font_family)
            };
            let Some(t) = face_tfm(face) else { continue };
            let sc = Scaler::at_pt(sz);
            let mut y = x;
            // §708: the list of characters starting with `x` in this font.
            while t.exists(y) {
                if t.tag(y) == EXT_TAG {
                    best = Some((face, sz, y));
                    is_ext = true;
                    break 'search;
                }
                let u = sc.scale(t.height(y)) + sc.scale(t.depth(y));
                if u > w {
                    best = Some((face, sz, y));
                    w = u;
                    if u >= v {
                        break 'search;
                    }
                }
                if t.tag(y) != LIST_TAG {
                    break;
                }
                y = t.remainder(y);
            }
        }
    }
    let (face, size, code) = best?;
    let t = face_tfm(face)?;
    let sc = Scaler::at_pt(size);
    if !is_ext {
        // §709 `char_box`: the width includes the italic correction; h and d unclamped.
        let [cw, ch, cd, ci] = t.dims_sp(code, &sc);
        return Some(CmDelimBox { face, size, code, ext: None, w: cw + ci, h: ch, d: cd });
    }
    // §713–§714: bot, n reps, mid, n reps, top, stacked upward; as few reps as reach `v`.
    let r = t.exten[t.remainder(code) as usize];
    let (top, mid, bot, rep) = (r[0], r[1], r[2], r[3]);
    let hpd = |c: u8| sc.scale(t.height(c)) + sc.scale(t.depth(c));
    let u = hpd(rep);
    let width = sc.scale(t.width(rep)) + sc.scale(t.italic(rep));
    let mut total = [bot, mid, top].iter().filter(|&&c| c != 0).map(|&c| hpd(c)).sum::<i32>();
    let mut reps = 0u32;
    if u > 0 {
        while total < v {
            total += u;
            reps += 1;
            if mid != 0 {
                total += u;
            }
        }
    }
    // The box's height is that of the last piece stacked.
    let last = if top != 0 {
        top
    } else if reps > 0 {
        rep
    } else if mid != 0 {
        mid
    } else {
        bot
    };
    let h = sc.scale(t.height(last));
    Some(CmDelimBox { face, size, code, ext: Some((r, reps)), w: width, h, d: total - h })
}

/// Height and depth of code `c` of `face` at `size`, exact from the TFM of the face's font at
/// that size (h, d >= 0 as a character box in a math list).
fn char_hd_code(face: Face, c: u8, size: f32, family: FontFamily) -> (f32, f32) {
    let f = resolve_optical_face(face, size, family);
    crate::math::math_tfm::face_tfm(f).map_or((0.0, 0.0), |t| {
        let g = crate::math::math_tfm::GlyphDims::of(t, c, size);
        (g.h, g.d)
    })
}

/// Height and depth of a `\vcenter` of total height `v` (§736): half(v) above the axis,
/// the rest below, in scaled points.
fn vcenter_hd(v: f32, axis: f32) -> (f32, f32) {
    use crate::backend::scaled::{half, sp_pt, to_sp};
    let (v, axis) = (to_sp(v), to_sp(axis));
    let h = half(v) + axis;
    (sp_pt(h), sp_pt(v - h))
}

/// Whether `node` is one character (TeX simplifies a group of one to its math_char, §1186).
fn is_char_like(node: &MathNode<'_>) -> bool {
    match node {
        MathNode::Variable(_) | MathNode::Symbol { .. } => true,
        MathNode::StyleChange { content, .. } => is_char_like(content),
        MathNode::Row(nodes) => nodes.len() == 1 && is_char_like(&nodes[0]),
        _ => false,
    }
}

/// Draws a [`CmDelimBox`] with its baseline at `base_y`: the character, or the extensible's
/// pieces stacked down from the box's top (§713).
fn emit_cm_delimiter(ctx: &mut MathLayoutContext<'_, '_>, b: &CmDelimBox, x: f32, base_y: f32) {
    use crate::backend::scaled::{sp_pt, to_sp};
    if ctx.is_measuring() {
        return;
    }
    let Some(([top, mid, bot, rep], reps)) = b.ext else {
        ctx.emit_glyph_run(x, base_y, b.face, b.size, byte_str(b.code));
        return;
    };
    let Some(t) = crate::math::math_tfm::face_tfm(b.face) else { return };
    let sc = crate::backend::scaled::Scaler::at_pt(b.size);
    let mut top_y = to_sp(base_y) + b.h;
    let mut piece = |c: u8| {
        if c != 0 {
            let base = top_y - sc.scale(t.height(c));
            ctx.emit_glyph_run(x, sp_pt(base), b.face, b.size, byte_str(c));
            top_y = base - sc.scale(t.depth(c));
        }
    };
    piece(top);
    if mid != 0 {
        (0..reps).for_each(|_| piece(rep));
        piece(mid);
    }
    (0..reps).for_each(|_| piece(rep));
    piece(bot);
}

/// §762 `make_left_right`: the size a `\left`/`\right` delimiter must reach around a list
/// `h` high and `d` deep, in scaled points.
pub fn left_right_target_sp(h: i32, d: i32, axis: i32, factor: i32, shortfall: i32) -> i32 {
    let mut delta1 = h - axis;
    let delta2 = d + axis;
    if delta2 > delta1 {
        delta1 = delta2;
    }
    let delta = (delta1 / 500) * factor;
    delta.max(delta1 + delta1 - shortfall)
}

/// [`layout_delimiter_legacy`] as a box around `y_center`: its width, its extent above
/// `y_center` and below it. Computer Modern delimiters are TeX's exactly (§706, §762):
/// the box `var_delimiter` builds, shifted to centre on the axis of `font_size`.
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
) -> MathBox {
    use crate::backend::scaled::{half, sp_pt, to_sp};
    if ctx.font_family == metrics::FontFamily::ComputerModern && !NullDelimiterEliminator::is_null(delim) {
        if let Some(b) = cm_var_delimiter_sp(ctx, delim, is_left, to_sp(target_h), font_size) {
            let axis = to_sp(crate::math::math_fonts::MathParams::at(ctx.font_family, font_size).axis_height);
            let shift = half(b.h - b.d) - axis;
            emit_cm_delimiter(ctx, &b, x, y_center - sp_pt(axis + shift));
            return MathBox::with_dims(sp_pt(b.w), sp_pt(b.h - shift - axis), sp_pt(b.d + shift + axis));
        }
    }
    let (w, total) = layout_delimiter_legacy(ctx, delim, is_left, x, y_center, target_h, font_size, rule_t);
    MathBox::with_dims(w, 0.5 * total, 0.5 * total)
}

#[allow(clippy::too_many_arguments)]
fn layout_delimiter_legacy<'a, 'p>(
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
/// amsmath `\r@@t`: the degree is raised .6(ht − dp + 1.667·`\uproot` mu) of the radical box,
/// whose height includes the kern above the rule.
fn root_degree_raise(ctx: &MathLayoutContext<'_, '_>, origin_y: f32, ht: f32, dp: f32, up: i8, style: MathStyle) -> f32 {
    // amsmath `\r@@t`: `\raise.6\dimen@`, \dimen@ the root's height less its depth plus
    // 1.667 times `\uproot` mu, each product as `scan_dimen` forms it.
    use crate::backend::scaled::{em_mul, sp_pt, to_sp};
    let up_w = ctx.skip_width(crate::math::math_ast::MathSkip::Mu(up as f32), style);
    let dimen = to_sp(ht) - to_sp(dp) + to_sp(em_mul(1.667, up_w));
    origin_y + em_mul(0.6, sp_pt(dimen))
}

/// falling back to classic TeX CMMI skewchar kerning for Computer Modern unstyled variables.
fn compute_top_accent_skew_offset<'a, 'p>(
    nucleus: &MathNode<'a>,
    ctx: &MathLayoutContext<'a, 'p>,
    font_size: f32,
    scale: f32,
    ic: f32,
) -> f32 {
    // amsmath `\macc@nested`: an accent over an accented character keeps that character's
    // skew, so stacked accents line up.
    if let MathNode::Accent { nucleus: inner, .. } = nucleus {
        return compute_top_accent_skew_offset(inner, ctx, font_size, scale, ic);
    }
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

    // Computer Modern math italic: the skew is the kern between the character and the
    // `\skewchar` (127) in the cmmi design of the current size (tex.web §741).
    if ctx.font_family == metrics::FontFamily::ComputerModern && !matches!(nucleus, MathNode::StyleChange { .. }) {
        let skew = |code: u8| {
            let face = resolve_optical_face(Face::MathItalic, font_size, ctx.font_family);
            cm_math_kern_table(face).map_or(0.0, |t| kern_pt(t, code, 127, font_size))
        };
        if let MathNode::Variable(ch) = target_node {
            if effective_face == Face::MathItalic {
                return if ch.is_ascii() { skew(*ch as u8) } else { 0.0 };
            }
        }
        if let MathNode::Symbol { mathcode, .. } = target_node {
            if let Some((Face::MathItalic, _, glyph_code)) = crate::math::math_fonts::resolve_cm_mathcode(*mathcode) {
                return skew(glyph_code);
            }
        }
        if let MathNode::Symbol { code: Some(code), mathcode: 0, .. } = target_node {
            if let Some((Face::MathItalic, _, glyph_code)) = crate::math::math_fonts::resolve_cm_symbol(*code) {
                return skew(glyph_code);
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
    let axis_h = params.axis_height;
    // TeX metrics are exact for Computer Modern; elsewhere keep hairlines visible.
    let rule_t = if ctx.font_family == metrics::FontFamily::ComputerModern {
        params.rule_thickness
    } else {
        (params.rule_thickness).max(0.35)
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
            let face = resolve_optical_face(alpha_face, ctx.font_tier.font_size(style), ctx.font_family);
            let (mut w, h, d) = match crate::math::math_tfm::glyph_dims(face, code, font_size, ctx.font_family) {
                Some(g) => (g.w, g.h, g.d),
                None => crate::math::math_tfm_faces::cm_face_metric(face, code)
                    .map_or((0.0, 0.0, 0.0), |m| (m.w * scale, m.h * scale, m.d * scale)),
            };
            // TS1 has a font per design size (`ts1cmr.fd`): its exact width at this size.
            if face == Face::TextComp {
                if let Some(fx) = crate::backend::scaled::fix_metrics(ctx.font_family, face, font_size) {
                    w = fx.widths[code as usize & 127] as f32 / 1048576.0 * font_size;
                }
            }
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
            let face = resolve_optical_face(base_face, ctx.font_tier.font_size(style), ctx.font_family);

            // Computer Modern's calligraphic capitals are cmsy's own codes, measured from its TFM below.
            let (w, h, d, dynamic_gid) = if base_face == Face::Symbol && ch.is_ascii_uppercase() && ctx.font_family != metrics::FontFamily::ComputerModern {
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
                        let (h_u, d_u) = char_hd(*ch, face, font_size, ctx.font_family);
                        (w, h_u, d_u, None)
                    }
                } else {
                    let w = measure_char(*ch, face, font_size, ctx.font_family);
                    let (h_u, d_u) = char_hd(*ch, face, font_size, ctx.font_family);
                    (w, h_u, d_u, None)
                }
            } else if base_face == Face::AmsB {
                let u_char = crate::math::fonts::to_unicode_blackboard(*ch);
                let gid = u_char.and_then(|u| crate::math::fonts::vector_glyph_for_unicode(u as u32, ctx.font_family));
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_hd(*ch, face, font_size, ctx.font_family);
                (w, h_u, d_u, gid)
            } else if base_face == Face::AmsA {
                let u_char = crate::math::fonts::to_unicode_fraktur(*ch);
                let gid = u_char.and_then(|u| crate::math::fonts::vector_glyph_for_unicode(u as u32, ctx.font_family));
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_hd(*ch, face, font_size, ctx.font_family);
                (w, h_u, d_u, gid)
            } else if base_face == Face::Mono {
                let u_char = crate::math::fonts::to_unicode_monospace(*ch);
                let gid = u_char.and_then(|u| {
                    crate::math::fonts::vector_glyph_for_unicode(u as u32, ctx.font_family)
                        .or_else(|| crate::math::fonts::active_math_font(ctx.font_family).and_then(|f| f.glyph_for_unicode(u as u32).map(|g| g.0)))
                });
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_hd(*ch, face, font_size, ctx.font_family);
                (w, h_u, d_u, gid)
            } else if base_face == Face::Italic {
                let u_char = crate::math::fonts::to_unicode_math_italic(*ch);
                let gid = u_char.and_then(|u| {
                    crate::math::fonts::vector_glyph_for_unicode(u as u32, ctx.font_family)
                        .or_else(|| crate::math::fonts::active_math_font(ctx.font_family).and_then(|f| f.glyph_for_unicode(u as u32).map(|g| g.0)))
                });
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_hd(*ch, face, font_size, ctx.font_family);
                (w, h_u, d_u, gid)
            } else if base_face == Face::Bold || base_face == Face::BoldItalic {
                let bold_u = if base_face == Face::BoldItalic {
                    crate::math::fonts::to_unicode_bold_italic(*ch)
                        .or_else(|| crate::math::fonts::to_unicode_bold(*ch))
                } else {
                    crate::math::fonts::to_unicode_bold(*ch)
                };
                let gid = bold_u.and_then(|u| crate::math::fonts::vector_glyph_for_unicode(u as u32, ctx.font_family));
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_hd(*ch, face, font_size, ctx.font_family);
                (w, h_u, d_u, gid)
            } else {
                let w = measure_char(*ch, face, font_size, ctx.font_family);
                let (h_u, d_u) = char_hd(*ch, face, font_size, ctx.font_family);
                (w, h_u, d_u, None)
            };

            let s = if ch.is_ascii() { crate::math::math_fonts::ASCII_GLYPH_STRS[*ch as usize] } else { "x" };

            // Computer Modern has a TFM font for every alphabet (msbm, eufm, cmbx, cmmib,
            // cmsy, cmtt), so it never draws the OpenType outlines.
            if let Some(gid) = dynamic_gid.filter(|_| ctx.font_family != metrics::FontFamily::ComputerModern) {
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
            let face = resolve_optical_face(base_face, ctx.font_tier.font_size(style), ctx.font_family);
            let mut w = 0.0f32;
            for b in num_str.bytes() {
                w += measure_char(b as char, face, font_size, ctx.font_family);
            }
            let (h_u, d_u) = char_hd('0', face, font_size, ctx.font_family);
            let h = h_u;
            let d = d_u;

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
                let resolved_code = if face.is_symbol() {
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
                resolve_optical_face(crate::math::math_fonts::bold_math_version(raw_face), ctx.font_tier.font_size(style), ctx.font_family)
            } else if let Some(ov) = override_face {
                if ctx.font_family == metrics::FontFamily::ComputerModern {
                    if ov == Face::Bold || ov == Face::BoldItalic {
                        Face::Bold
                    } else {
                        resolve_optical_face(ov, ctx.font_tier.font_size(style), ctx.font_family)
                    }
                } else {
                    resolve_optical_face(ov, ctx.font_tier.font_size(style), ctx.font_family)
                }
            } else {
                resolve_optical_face(raw_face, ctx.font_tier.font_size(style), ctx.font_family)
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
                // Computer Modern: the TFM box of the character, exact at this size.
                let tfm = (ctx.font_family == metrics::FontFamily::ComputerModern && c < 128)
                    .then(|| crate::math::math_tfm::glyph_dims(eff_face, c, font_size, ctx.font_family))
                    .flatten();
                if let Some(g) = tfm {
                    (g.w, g.h, g.d)
                } else {
                let w = if (eff_face == Face::Bold || eff_face == Face::BoldItalic) && ctx.font_family != metrics::FontFamily::ComputerModern {
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
                } else if eff_face.is_symbol() {
                    metrics::SYMBOL_WIDTHS[c as usize] as f32 * scale
                } else {
                    let mut total_w = 0.0f32;
                    for ch in eff_glyph.chars() {
                        total_w += measure_char(ch, eff_face, font_size, ctx.font_family);
                    }
                    total_w
                };
                let (h_u, d_u) = if eff_face.is_symbol() {
                    symbol_height_depth_family(c, ctx.font_family)
                } else {
                    let first = eff_glyph.chars().next().unwrap_or('x');
                    char_height_depth(first, eff_face, ctx.font_family)
                };
                (w, h_u * scale, d_u * scale)
                }
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
                        total_w += kern_pt(word.table, c, word.glyphs[i + 1], font_size);
                    }
                    let (ch_h, ch_d) = char_hd(c as char, eff_face, font_size, ctx.font_family);
                    max_h_u = max_h_u.max(ch_h);
                    max_d_u = max_d_u.max(ch_d);
                }
                // Operator names and joined math letters are math characters: the last one
                // keeps its italic correction.
                if *kind == MathAtomKind::Op || crate::math::math_ast::is_math_ligature_word(eff_glyph) {
                    total_w += char_ic(word.glyphs[word.len - 1] as char, eff_face, font_size, ctx.font_family);
                }
                return MathBox::with_dims(total_w, max_h_u, max_d_u);
            } else {
                let mut total_w = 0.0f32;
                let mut max_h_u = 0.0f32;
                let mut max_d_u = 0.0f32;
                for ch in eff_glyph.chars() {
                    total_w += measure_char(ch, eff_face, font_size, ctx.font_family);
                    let (ch_h, ch_d) = char_hd(ch, eff_face, font_size, ctx.font_family);
                    max_h_u = max_h_u.max(ch_h);
                    max_d_u = max_d_u.max(ch_d);
                }
                // Text words are hboxes; operator names keep the last character's italic correction.
                if *kind == MathAtomKind::Op {
                    if let Some(last_ch) = eff_glyph.chars().last() {
                        total_w += char_ic(last_ch, eff_face, font_size, ctx.font_family);
                    }
                }
                (total_w, max_h_u, max_d_u)
            };

            // Computer Modern has a TFM font for every alphabet (msbm, eufm, cmbx, cmmib,
            // cmsy, cmtt), so it never draws the OpenType outlines.
            if let Some(gid) = dynamic_gid.filter(|_| ctx.font_family != metrics::FontFamily::ComputerModern) {
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
            // (§738); an overline or underline (Rules 8, 9) leaves a box.
            let (effective_nuc, effective_h, effective_d) = match &**nucleus {
                MathNode::Accent { nucleus: inner, accent } if !matches!(*accent, "overline" | "underline") => {
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
            let ic = node_italic_correction_styled(effective_nuc, ctx.face_override, ctx.font_family, style, &ctx.font_tier);
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
            // treat it as a non-character). So is an overlined or underlined nucleus
            // (`clean_box` keeps the correction's width).
            let boxed_bold = matches!(
                effective_nuc,
                MathNode::StyleChange { face: Face::BoldItalic, .. } | MathNode::Accent { .. }
            );
            let sup_x = origin_x + nuc_box.w + ic + math_kern;
            let sub_x = origin_x + nuc_box.w + if boxed_bold { ic } else { 0.0 };

            let x_height = params.accent_base_height;

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
                &params,
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
                SCRIPT_SPACE_PT
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
            let ic = node_italic_correction_styled(num, ctx.face_override, ctx.font_family, num_style, &ctx.font_tier);
            num_box.w += ic;
            let num_end = ctx.out_ops_len();

            let den_start = ctx.out_ops_len();
            let mut den_box = layout_node(ctx, den, den_style, 0.0, 0.0);
            let ic = node_italic_correction_styled(den, ctx.face_override, ctx.font_family, den_style, &ctx.font_tier);
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
                NULL_DELIMITER_SPACE
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
                &params,
            );
            let num_y = origin_y + metrics.num_shift;
            let den_y = origin_y - metrics.den_shift;

            ctx.shift_ops_range(num_start, num_end, num_x, num_y);
            ctx.shift_ops_range(den_start, den_end, den_x, den_y);
            if *has_rule {
                // §747–§748: the rule spans the vlist of numerator and denominator, between
                // the delimiters, and is shipped out between them (numerator, rule, denominator).
                let rule_y = origin_y + axis_h - 0.5 * rule_t;
                let before = ctx.out_ops_len();
                ctx.emit_rule(origin_x + frac_pad, rule_y, frac_w - 2.0 * frac_pad, rule_t);
                let n = ctx.out_ops_len() - before;
                ctx.move_last_ops_to(num_end, n);
            }

            let total_h = (num_y + num_box.h) - origin_y;
            let total_d = origin_y - (den_y - den_box.d);

            MathBox::with_dims(frac_w, total_h, total_d)
        }

        MathNode::Radical { degree, radicand, up, left } => {
            let rad_style = style.cramped();

            let rad_start = ctx.out_ops_len();
            let mut rad_box = layout_node(ctx, radicand, rad_style, 0.0, 0.0);
            let ic = node_italic_correction_styled(radicand, ctx.face_override, ctx.font_family, rad_style, &ctx.font_tier);
            rad_box.w += ic;
            let rad_end = ctx.out_ops_len();

            let deg_start = ctx.out_ops_len();
            let deg_box = if let Some(deg) = degree {
                // §755: a final character's italic correction is part of the degree's box.
                let deg_style = MathStyle::CrampedScriptScript;
                let mut b = layout_node(ctx, deg, deg_style, 0.0, 0.0);
                b.w += node_italic_correction_styled(deg, ctx.face_override, ctx.font_family, deg_style, &ctx.font_tier);
                b
            } else {
                MathBox::zero()
            };
            let deg_end = ctx.out_ops_len();


            let (_deg_advance, deg_x_offset, rad_origin_x) = if degree.is_some() {
                let c = crate::math::fonts::math_constants_for_family(ctx.font_family);
                // plain/latex `\r@@t`: `\mkern5mu` before and `\mkern-10mu` after the degree;
                // amsmath moves the degree left by `\leftroot` mu, keeping the total width.
                let left_mu = *left as f32;
                let kern_before = if ctx.font_family == metrics::FontFamily::ComputerModern {
                    ctx.skip_width(crate::math::math_ast::MathSkip::Mu(5.0 - left_mu), style)
                } else {
                    (c.radical_kern_before_degree as f32 * 0.001) * font_size
                };
                let mut kern_after = if ctx.font_family == metrics::FontFamily::ComputerModern {
                    ctx.skip_width(crate::math::math_ast::MathSkip::Mu(-10.0 + left_mu), style)
                } else {
                    (c.radical_kern_after_degree as f32 * 0.001) * font_size
                };
                // LuaTeX mlist.c:2389-2390 clamp: if -ar > (wr + br) { ar = -(wr + br); }
                // Computer Modern follows the `\r@@t` macro, which has no clamp.
                let cm = ctx.font_family == metrics::FontFamily::ComputerModern;
                if !cm && -kern_after > deg_box.w + kern_before {
                    kern_after = -(deg_box.w + kern_before);
                }
                let advance = kern_before + deg_box.w + kern_after;
                let advance = if cm { advance } else { advance.max(0.0) };
                (advance, kern_before, origin_x + advance)
            } else {
                (0.0, 0.0, origin_x)
            };

            let c = crate::math::fonts::math_constants_for_family(ctx.font_family);
            let (rule_t_cm, min_clr) = if ctx.font_family == metrics::FontFamily::ComputerModern {
                let r_t = params.rule_thickness;
                let clr = if style.is_display() {
                    params.rad_clearance_display
                } else {
                    params.rad_clearance_text
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
                                root_degree_raise(ctx, origin_y, total_h, total_d, *up, style)
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

            // §737 `make_radical` for Computer Modern, in scaled points: the surd is
            // `var_delimiter`'d to the radicand's height plus depth, clearance and rule; its
            // excess depth widens the clearance by half; the overbar is as thick as the surd
            // is high and sits on the clearance above the radicand.
            if ctx.font_family == metrics::FontFamily::ComputerModern {
                use crate::backend::scaled::{half, sp_pt, to_sp};
                let (hx, dx, t) = (to_sp(rad_box.h), to_sp(rad_box.d), to_sp(params.rule_thickness));
                let mut clr = to_sp(min_clr);
                if let Some(y) = cm_var_delimiter_sp(ctx, "\\surd", false, hx + dx + clr + t, font_size) {
                    let delta = y.d - (hx + dx + clr);
                    if delta > 0 {
                        clr += half(delta);
                    }
                    let rad_content_x = rad_origin_x + sp_pt(y.w);
                    ctx.shift_ops_range(rad_start, rad_end, rad_content_x, origin_y);
                    let mark = ctx.out_ops_len();
                    if !ctx.is_measuring() {
                        emit_cm_delimiter(ctx, &y, rad_origin_x, origin_y + sp_pt(hx + clr));
                        ctx.emit_rule(rad_content_x, origin_y + sp_pt(hx + clr), rad_box.w, sp_pt(y.h));
                    }
                    let mut total_h = sp_pt(hx + clr + 2 * y.h);
                    let mut total_d = sp_pt(dx.max(y.d - hx - clr));
                    if degree.is_some() {
                        let deg_y = root_degree_raise(ctx, origin_y, total_h, total_d, *up, style);
                        ctx.shift_ops_range(deg_start, deg_end, origin_x + deg_x_offset, deg_y);
                        total_h = total_h.max(deg_y + deg_box.h - origin_y);
                        total_d = total_d.max(origin_y - deg_y + deg_box.d);
                    }
                    // Drawn in list order (`\r@@t`, §737): degree, surd, overbar, radicand.
                    debug_assert!(deg_start == rad_end && deg_end <= mark);
                    ctx.move_last_ops_to(rad_start, ctx.out_ops_len().saturating_sub(rad_end));
                    return MathBox::with_dims(rad_content_x + rad_box.w - origin_x, total_h, total_d);
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
                        root_degree_raise(ctx, origin_y, total_h, total_d, *up, style)
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
            // Computer Modern here is Latin Modern (`lmodern`), whose OMX shape is
            // `<->sfixed*lmex10`: script and scriptscript operators are the 10pt glyphs, not
            // amsmath's cmex7 (which only applies to the cmex family).
            let op_ext_face = Face::Extension;
            let ext_size = if ctx.font_family == metrics::FontFamily::ComputerModern && style.tier() >= 2 {
                LMEX_FIXED_SIZE
            } else {
                font_size
            };
            // Exact cmex10 box of an operator glyph at its size (in points).
            let op_metric = |c: u8, size: f32| crate::math::math_tfm::GlyphDims::of(&crate::backend::backend_tfm::CMEX10_TFM, c, size);
            // `\limsup`/`\liminf` are `lim\,sup`/`lim\,inf` (latex.ltx); other operator
            // names are a single word set in the operator font at the current size.
            let (text_head, text_tail) = match *op {
                "limsup" => ("lim", "sup"),
                "liminf" => ("lim", "inf"),
                _ => (*op, ""),
            };
            let text_face = resolve_optical_face(Face::Roman, ctx.font_tier.font_size(style), ctx.font_family);
            let text_gap = if text_tail.is_empty() {
                0.0
            } else {
                3.0 * params.quad / 18.0
            };
            let text_head_w = text_word_width(text_head, text_face, font_size, ctx.font_family);
            let (op_face, op_size, op_w, op_h, op_d, op_ic) = if is_text_op {
                let tail_w = text_word_width(text_tail, text_face, font_size, ctx.font_family);
                let (mut h_u, mut d_u) = (0.0f32, 0.0f32);
                for ch in text_head.chars().chain(text_tail.chars()) {
                    let (ch_h, ch_d) = char_hd(ch, text_face, font_size, ctx.font_family);
                    h_u = h_u.max(ch_h);
                    d_u = d_u.max(ch_d);
                }
                // §755: the name's last character is a `math_char`, so its italic
                // correction is kerned in (`inf` ends `f\/`).
                let last = text_tail.chars().last().or_else(|| text_head.chars().last());
                let ic = last.map_or(0.0, |c| {
                    crate::backend::scaled::italic_correction_pt(ctx.font_family, text_face, font_size, c)
                });
                (
                    text_face,
                    font_size,
                    text_head_w + text_gap + tail_w + ic,
                    h_u,
                    d_u,
                    0.0,
                )
            } else if is_integral {
                let code = BigOpVariantTier::resolve_integral_cmex_code(op, style.is_display());
                cm_op_code = code;
                let m = op_metric(code, ext_size);
                let single_w = m.w + m.ic;
                let (h, d) = (m.h, m.d);
                // The emitted glyph is the cmex10 one, so its TFM italic correction applies.
                let ic = m.ic;
                let (w, _) = IntegralOverhangCorrector::total_integral_width(
                    op,
                    single_w,
                    ext_size,
                    style.is_display(),
                );
                (op_ext_face, ext_size, w, h, d, ic)
            } else {
                let cmex_code = BigOpVariantTier::resolve_cmex_code(op, style.is_display());
                cm_op_code = cmex_code;
                let m = op_metric(cmex_code, ext_size);
                let w = m.w + m.ic;
                let (h, d) = (m.h, m.d);
                // The emitted glyph is the cmex10 one, so its TFM italic correction applies.
                let ic = m.ic;
                (op_ext_face, ext_size, w, h, d, ic)
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
                    let m = op_metric(cm_op_code, op_size);
                    let single_w = m.w + m.ic;
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
                    } else if matches!(*op, "iiint" | "oiiint" | "iiiint") {
                        let n = if *op == "iiiint" { 4 } else { 3 };
                        for k in 0..n {
                            ctx.emit_glyph_run(x + k as f32 * offset, y, op_ext_face, op_size, int_glyph);
                        }
                    } else if *op == "idotsint" {
                        // `\intop`, a thin space, `\intdots@` on the baseline, a thin space,
                        // `\intop`.
                        let mu = op_size / 18.0;
                        let display = style.is_display();
                        ctx.emit_glyph_run(x, y, op_ext_face, op_size, int_glyph);
                        let gap = if display { 3.0 * mu } else { 1.5 * mu };
                        let cdot_w = 0.277_779 * op_size;
                        let mut dx = x + single_w + 3.0 * mu;
                        for _ in 0..3 {
                            ctx.emit_glyph_run(dx, y - delta_y, Face::Symbol, op_size, crate::math::math_delim::byte_str(1));
                            dx += cdot_w + gap;
                        }
                        dx += 3.0 * mu - gap;
                        ctx.emit_glyph_run(dx, y, op_ext_face, op_size, int_glyph);
                    } else {
                        ctx.emit_glyph_run(x, y, op_ext_face, op_size, int_glyph);
                    }
                } else {
                    let s = crate::math::math_delim::byte_str(cm_op_code);
                    ctx.emit_glyph_run(x, y, op_ext_face, op_size, s);
                }
            };

            if is_display_limits {
                // §750 stacks superscript, operator, subscript: drawn in that order.
                let sup_start = ctx.out_ops_len();
                let sup_box = match sup {
                    Some(s) => layout_script_box(ctx, s, style.sup_style()),
                    None => MathBox::zero(),
                };
                let sup_end = ctx.out_ops_len();

                let sub_start = ctx.out_ops_len();
                let sub_box = match sub {
                    Some(s) => layout_script_box(ctx, s, style.sub_style()),
                    None => MathBox::zero(),
                };
                let sub_end = ctx.out_ops_len();

                // amsmath `\ints@b`: a multiple integral with limits is `\mkern-7mu` (-9mu in
                // display) and a `\mathop` group opening with `\mkern7mu` (9mu): the limits
                // centre on that wider box, which has no italic correction of its own.
                let lead = if matches!(*op, "iint" | "iiint" | "iiiint" | "idotsint") {
                    (if style.is_display() { 9.0 } else { 7.0 }) * font_size / 18.0
                } else {
                    0.0
                };
                let (max_w, op_x, sup_x, sub_x) = OperatorCenteringEngine::center_limits_positioned(
                    origin_x - lead,
                    op_w + lead,
                    sup_box.w,
                    sub_box.w,
                    sup.is_some(),
                    sub.is_some(),
                    if lead > 0.0 { 0.0 } else { half_ic },
                );
                let (max_w, op_x) = (max_w - lead, op_x + lead);

                let big_metrics = BigOpShiftResolver::resolve_limit_metrics(
                    ctx.font_family,
                    font_size,
                    &params,
                );

                let (sup_y, sub_y) = LimitClearanceGovernor::compute_limit_positions(
                    op_top_y,
                    op_bot_y,
                    sup_box.d,
                    sub_box.h,
                    &big_metrics,
                );

                if sup.is_some() {
                    ctx.shift_ops_range(sup_start, sup_end, sup_x, sup_y);
                }

                if sub.is_some() {
                    ctx.shift_ops_range(sub_start, sub_end, sub_x, sub_y);
                }

                if !ctx.is_measuring() {
                    let mark = ctx.out_ops_len();
                    emit_op(ctx, op_x, op_y);
                    ctx.move_last_ops_to(sub_start, ctx.out_ops_len() - mark);
                }

                let op_extra_pad = params.big_op_spacing5;
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
                        &params,
                        ctx.script_drops(style),
                    );

                let sup_x = origin_x + sup_x_rel;
                let sub_x = origin_x + sub_x_rel;
                // \scriptspace follows the scripts (TeX §756).
                let script_space = if sup.is_some() || sub.is_some() {
                    SCRIPT_SPACE_PT
                } else {
                    0.0
                };

                if !ctx.is_measuring() {
                    // The nucleus comes before its scripts in the list (§756).
                    let mark = ctx.out_ops_len();
                    emit_op(ctx, origin_x, op_y);
                    ctx.move_last_ops_to(sup_start, ctx.out_ops_len() - mark);
                }
                let op_n = ctx.out_ops_len() - sub_end;
                let (sup_start, sup_end, sub_start, sub_end) = (sup_start + op_n, sup_end + op_n, sub_start + op_n, sub_end + op_n);

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
                factor,
                shortfall: declared_shortfall,
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

            let shortfall = if !declared_shortfall.is_nan() {
                *declared_shortfall
            } else if ctx.font_family == metrics::FontFamily::ComputerModern {
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
                    *factor,
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
            content_box.w += node_italic_correction_styled(content, ctx.face_override, ctx.font_family, style, &ctx.font_tier);

            let mut target_h = if ctx.font_family == metrics::FontFamily::ComputerModern {
                use crate::backend::scaled::{sp_pt, to_sp};
                sp_pt(left_right_target_sp(to_sp(content_box.h), to_sp(content_box.d), to_sp(axis_h), i32::from(*factor), to_sp(shortfall)))
            } else {
                crate::math::math_delim::DelimiterSizingGovernor::target_height_with_shortfall(
                    content_box.h,
                    content_box.d,
                    axis_h,
                    font_size,
                    shortfall,
                    *factor,
                )
            };

            // TeX Appendix G Rule 15e (mlist.c 3131-3138): a generalized fraction's delimiters
            // are var_delimiter'd to σ20 (delim1) in display style and σ21 (delim2) otherwise,
            // whatever the fraction's height.
            if is_genfrac {
                let delim = if style.is_display() { params.delim1 } else { params.delim2 };
                target_h = delim;
            }
            let y_center = origin_y + axis_h;

            let open_start = ctx.out_ops_len();
            let open_box = layout_delimiter(
                ctx,
                open,
                true,
                origin_x,
                y_center,
                target_h,
                font_size,
                rule_t,
            );
            // The left delimiter precedes the inner list (§762).
            let open_n = ctx.out_ops_len() - open_start;
            ctx.move_last_ops_to(content_start, open_n);
            let (content_start, content_end) = (content_start + open_n, content_end + open_n);

            let content_x = origin_x + open_box.w;
            let close_x = content_x + content_box.w;

            ctx.shift_ops_range(content_start, content_end, content_x, origin_y);

            let close_box = layout_delimiter(
                ctx, close, false, close_x, y_center, target_h, font_size, rule_t,
            );

            // `hpack` of the delimiters (shifted onto the axis) and the inner list.
            MathBox::with_dims(
                open_box.w + content_box.w + close_box.w,
                content_box.h.max(axis_h + open_box.h.max(close_box.h)),
                content_box.d.max(open_box.d.max(close_box.d) - axis_h),
            )
        }

        MathNode::Matrix(data) => {
            let MatrixData {
                rows,
                delim,
                env_kind,
                col_alignments,
                split,
                ..
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
                    env_kind,
                    font_size,
                    (ctx.font_family != FontFamily::ComputerModern).then(|| ctx.opentype_engine().constants()),
                );
            let cell_style = matrix_cell_style(env_kind, style);

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
                    b.w += node_italic_correction_styled(cell, dry_ctx.face_override, dry_ctx.font_family, cell_style, &dry_ctx.font_tier);
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

            let mut matrix_w = crate::math::math_matrix::MatrixLayoutMetricsEngine::compute_col_offsets_and_width(
                col_widths, col_offsets, env_kind, font_size, num_rows,
                crate::backend::scaled::face_quad_pt(ctx.font_family, ctx.text_face, ctx.base_font_size),
            );
            // amsmath `\measure@` for a display `align`: the box starts with \eqnshift@ and its n
            // column pairs are \alignsep@ apart, both (\displaywidth - \totwidth@)/(n + 1),
            // the separation at least \minalignsep (10pt); `\calc@shift@align` then narrows
            // them for rows whose number would not fit.
            if data.display_env && env_kind == MatrixEnvKind::Aligned && ctx.display_width > 0.0 {
                let (shift, sep) = align_shift_and_sep(ctx, data, col_widths, cell_widths, num_cols);
                let mut x = shift;
                for c in 0..num_cols {
                    col_offsets[c] = x;
                    x += col_widths[c] + if c % 2 == 1 { sep } else { 0.0 };
                }
                let pairs = num_cols.div_ceil(2);
                let content = col_widths.iter().sum::<f32>() + (pairs - 1) as f32 * sep;
                // The display's alignment is `\displaywidth` wide; a nested one is its
                // `\halign` at natural width.
                matrix_w = if ctx.inner_align { shift + content } else { ctx.display_width.max(shift + content) };
            }
            let total_content_h = crate::math::math_matrix::MatrixLayoutMetricsEngine::compute_row_baseline_offsets_and_total_height(
                row_heights, row_depths, row_baseline_offsets, row_sep,
                crate::math::math_matrix::MatrixLayoutMetricsEngine::openup_baselineskip(env_kind, font_size),
                data.row_skips(),
            );

            // amsmath's `split` is `\gather@` in a `\vcenter`; the `\cr` ending its preamble
            // puts `\vskip-\lineskiplimit` (`-\jot` after `\openup`) above the first row.
            let top_kern = if *split { 0.3 * font_size } else { 0.0 };
            let total_content_h = total_content_h - top_kern;
            // Center the entire matrix block symmetrically across the math axis:
            // §736: a `\vcenter`ed box's height is half(h + d) above the axis.
            let half_v = crate::backend::scaled::sp_pt(crate::backend::scaled::half(crate::backend::scaled::to_sp(total_content_h)));
            let top_y = origin_y + axis_h + half_v + top_kern;

            let (open_str, close_str) =
                crate::math::math_matrix::MatrixDelimBinder::resolve_delims(delim);
            let has_delim = !open_str.is_empty();
            let y_center = origin_y + axis_h;
            let m_metrics = matrix_metrics(ctx.font_family);
            let (target_h, sep) = if ctx.font_family == metrics::FontFamily::ComputerModern {
                // amsmath wraps the array in \left…\right: Rule 19's max(0.901 z, z - 5pt)
                // with z the array's height plus depth (it is centred on the axis).
                // §762 on the `\vcenter`ed array.
                use crate::backend::scaled::{sp_pt, to_sp};
                let (vh, vd) = vcenter_hd(total_content_h, axis_h);
                (sp_pt(left_right_target_sp(to_sp(vh), to_sp(vd), to_sp(axis_h), 901, to_sp(5.0))), 0.0)
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
                let b = layout_delimiter(
                    &mut dry_ctx,
                    open_str,
                    true,
                    0.0,
                    y_center,
                    target_h,
                    font_size,
                    rule_t,
                );
                (b.w, (b.h, b.d))
            } else {
                (0.0, (0.0, 0.0))
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
                // The outermost alignment records where each row's ops end; nested ones do not,
                // except a numbered one (amsmath `\ingather@`), whose rows are the display's.
                let mut row_ends = ctx.row_ends.take();
                let cells_start = ctx.out_ops_len();
                // Second pass: paint cells aligned to each row's baseline and aligned within each column
                for (r_idx, r) in rows.iter().enumerate() {
                    let curr_y = top_y - row_baseline_offsets[r_idx];
                    let inner_numbered = row_ends.is_some() && numbered_inner_align(r).is_some();
                    if inner_numbered {
                        ctx.row_ends = row_ends.take();
                    }
                    let row_cell_base = r_idx * num_cols;
                    for (col_idx, cell) in r.iter().enumerate() {
                        if let Some((cols, space)) = hdotsfor_cell(cell) {
                            let last = (col_idx + cols as usize - 1).min(num_cols - 1);
                            let x0 = cells_start_x + col_offsets[col_idx];
                            let x1 = cells_start_x + col_offsets[last] + col_widths[last];
                            paint_hdotsfor(ctx, space, x0, x1, curr_y, cell_style);
                            continue;
                        }
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
                    if inner_numbered {
                        row_ends = ctx.row_ends.take();
                    } else if let Some(ends) = row_ends.as_deref_mut() {
                        ends.push(ctx.out_ops_len());
                    }
                }
                ctx.row_ends = row_ends;

                if has_delim {
                    // `\left` is shipped out before the array it opens.
                    let before = ctx.out_ops_len();
                    layout_delimiter(
                        ctx, open_str, true, origin_x, y_center, target_h, font_size, rule_t,
                    );
                    let n = ctx.out_ops_len() - before;
                    ctx.move_last_ops_to(cells_start, n);
                    if let Some(ends) = ctx.row_ends.as_deref_mut() {
                        ends.iter_mut().filter(|e| **e > cells_start).for_each(|e| *e += n);
                    }
                }
            }

            if has_delim {
                let has_close = !close_str.is_empty();
                let (close_x, close_w, close_h) = if has_close {
                    let cx = cells_start_x + matrix_w + sep;
                    let b = layout_delimiter(
                        ctx, close_str, false, cx, y_center, target_h, font_size, rule_t,
                    );
                    (cx, b.w, (b.h, b.d))
                } else if delim == MatrixDelim::LeftBrace {
                    // Canonical \nulldelimiterspace margin (1.2pt = 0.12 * font_size)
                    (cells_start_x + matrix_w, NULL_DELIMITER_SPACE, (0.0, 0.0))
                } else {
                    (cells_start_x + matrix_w, 0.0, (0.0, 0.0))
                };
                // The delimiters' extents above and below the axis (`layout_delimiter`'s h, d).
                let (vh, vd) = vcenter_hd(total_content_h, axis_h);
                MathBox::with_dims(
                    (close_x + close_w) - origin_x,
                    vh.max(axis_h + open_h.0.max(close_h.0)),
                    vd.max(open_h.1.max(close_h.1) - axis_h),
                )
            } else {
                let (vh, vd) = vcenter_hd(total_content_h, axis_h);
                MathBox::with_dims(matrix_w, vh, vd)
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
            let ic = node_italic_correction_styled(nucleus, ctx.face_override, ctx.font_family, style, &ctx.font_tier);
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
            // §734–§735: the nucleus is a `clean_box`, its italic correction inside, so the
            // rule spans it; `overbar` stacks the rule above the box (drawn first).
            let cm = ctx.font_family == metrics::FontFamily::ComputerModern;
            let bar_w = if cm { nuc_box.w + ic } else { nuc_box.w };
            if *accent == "overline" {
                let rule_y = origin_y + nuc_box.h + 3.0 * rule_t;
                ctx.emit_rule(origin_x, rule_y, bar_w, rule_t);
                if ctx.out_ops_len() == nuc_end + 1 {
                    ctx.move_last_op_to(nuc_start);
                }
                return MathBox::with_dims(
                    bar_w,
                    nuc_box.h + 5.0 * rule_t,
                    nuc_box.d,
                );
            }
            if *accent == "underline" {
                let rule_w = bar_w;
                let under_gap = if ctx.font_family == metrics::FontFamily::ComputerModern {
                    3.0 * rule_t
                } else {
                    1.5 * (font_size / 10.0)
                };
                let rule_y = origin_y - nuc_box.d - under_gap - rule_t;
                ctx.emit_rule(origin_x, rule_y, rule_w, rule_t);
                return MathBox::with_dims(
                    bar_w,
                    nuc_box.h,
                    nuc_box.d + under_gap + 2.0 * rule_t,
                );
            }

            // TeX Appendix G Rule 12 & mlist.c for Computer Modern
            if ctx.font_family == metrics::FontFamily::ComputerModern {
                if let Some(record) = crate::math::math_fonts::CmAccentRecord::lookup(accent) {
                    let ext = if font_size < 0.85 * ctx.base_font_size { Face::Extension7 } else { Face::Extension };
                    // §738: the variant search compares with `w`, the width of
                    // `clean_box(nucleus)`, which includes its italic correction.
                    let (face, code, acc_w, acc_h, acc_ic) = record.resolve_metric(nuc_box.w + ic, font_size, ext);
                    match record.position {
                        crate::math::math_fonts::AccentPosition::Top => {
                            let x_height = params.accent_base_height;
                            let delta = nuc_box.h.min(x_height);
                            let acc_y = origin_y + nuc_box.h - delta;
                            // §738: `shift_amount(y) := s + half(w - width(y))`. `w` is the
                            // width of `clean_box(nucleus)`, with a character's italic
                            // correction (§755); `y` is `char_box(f, c)`, as wide as the
                            // accent's width plus its italic correction (§709).
                            let acc_x = origin_x + (nuc_box.w + ic - (acc_w + acc_ic)) * 0.5 + skew_offset;

                            ctx.emit_glyph_run(
                                acc_x,
                                acc_y,
                                face,
                                font_size,
                                crate::math::math_delim::byte_str(code),
                            );
                            // §738 packs the accent above the nucleus (`link(y):=p; link(p):=x`),
                            // so it is drawn first.
                            if ctx.out_ops_len() == nuc_end + 1 {
                                ctx.move_last_op_to(nuc_start);
                            }

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
                            let (gh, gd) = char_hd_code(Face::Symbol, code, font_size, ctx.font_family);
                            ctx.emit_glyph_run(
                                origin_x,
                                origin_y,
                                face,
                                font_size,
                                crate::math::math_delim::byte_str(code),
                            );
                            // It precedes the relation in the list.
                            if ctx.out_ops_len() == nuc_end + 1 {
                                ctx.move_last_op_to(nuc_start);
                            }
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
                            let x_height = params.accent_base_height;
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
                    let x_height = params.accent_base_height;
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
            let (h_u, d_u) = char_hd('h', face, font_size, ctx.font_family);
            let h = h_u;
            let d = d_u;

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

        MathNode::TextFont(content) => {
            let prev = ctx.face_override;
            ctx.face_override = Some(ctx.text_face);
            let b = layout_node(ctx, content, style, origin_x, origin_y);
            ctx.face_override = prev;
            b
        }
        MathNode::StyleChange { face, content } => {
            let prev = ctx.face_override;
            ctx.face_override = Some(*face);
            let b = layout_node(ctx, content, style, origin_x, origin_y);
            ctx.face_override = prev;
            b
        }

        // A new formula starts uncramped in text style; `\text` in script sizes keeps its
        // smaller fonts, which the script styles stand for.
        MathNode::Formula(content) => {
            let inner = if style.tier() <= 1 { MathStyle::Text } else { style.uncramped() };
            let prev = ctx.face_override.take();
            let mut b = layout_node(ctx, content, inner, origin_x, origin_y);
            b.w += node_italic_correction_styled(content, None, ctx.font_family, inner, &ctx.font_tier);
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
            let mu_table = if is_cm {
                let quad_sp = crate::math::math_box::ScaledPoints::from_pt(
                    params.quad,
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

            let med_shrink = std::mem::take(&mut ctx.top_med_shrink);
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
                    if med_shrink > 0.0 && atom_spacing_mu(prev_kind, curr_kind, is_disp_or_text) == 4 {
                        space -= med_shrink;
                    }

                    space += prev_node_ic;
                    if let Some(prev) = prev_node {
                        space += char_pair_adjust(ctx, prev, child, style, font_size, prev_node_ic);
                    }

                    curr_x += space;
                }

                prev_node = Some(child);
                prev_node_ic = if ctx.text_box {
                    0.0
                } else {
                    node_italic_correction_styled(child, ctx.face_override, ctx.font_family, style, &ctx.font_tier)
                };
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

        // latex.ltx `\rule`: `\hbox{\raise#1\vrule width#2 height#3 depth\z@}`.
        MathNode::Rule { w, h, raise } => {
            if *w > 0.0 && *h > 0.0 {
                ctx.emit_rule(origin_x, origin_y + raise, *w, *h);
            }
            MathBox::with_dims(*w, (raise + h).max(0.0), (-raise).max(0.0))
        }

        MathNode::OpenBox => {
            use crate::backend::scaled::{em_mul, face_quad_pt, sp_pt, to_sp};
            let quad = face_quad_pt(ctx.font_family, ctx.text_face, ctx.base_font_size);
            let (w, ht, hw, rule) = (to_sp(em_mul(0.77778, quad)), to_sp(em_mul(0.675, quad)), to_sp(em_mul(0.6, quad)), 26214);
            // The two \hfil share the slack; hlist_out rounds the first at half of it.
            let left = ((w - hw - 2 * rule) as f64 / 2.0).round() as i32;
            let (x0, x1) = (origin_x + sp_pt(left), origin_x + sp_pt(left + rule));
            ctx.emit_rule(x0, origin_y, sp_pt(rule), sp_pt(ht));
            ctx.emit_rule(x1, origin_y + sp_pt(ht - rule), sp_pt(hw), sp_pt(rule));
            ctx.emit_rule(x1, origin_y, sp_pt(hw), sp_pt(rule));
            ctx.emit_rule(origin_x + sp_pt(left + rule + hw), origin_y, sp_pt(rule), sp_pt(ht));
            MathBox::with_dims(sp_pt(w), sp_pt(ht), 0.0)
        }

        MathNode::Raise { content, shift } => {
            let shift = *shift;
            let was_text = std::mem::replace(&mut ctx.text_box, true);
            let b = layout_node(ctx, content, MathStyle::Text, origin_x, origin_y + shift);
            ctx.text_box = was_text;
            MathBox::with_dims(b.w, (b.h + shift).max(0.0), (b.d - shift).max(0.0))
        }

        MathNode::Pmb(content) => {
            use crate::math::math_ast::MathSkip;
            let mu = |m: f32| ctx.skip_width(MathSkip::Mu(m), style);
            // amsbsy `\pmb@`: \mkern-.8mu, a copy, \mkern.4mu, the copy raised by the width
            // of \mkern.5mu, \mkern.4mu, the copy; each kern rounded on its own (§716).
            let (left, step, raise) = (mu(-0.8), mu(0.4), mu(0.5));
            let b = layout_node(ctx, content, style, origin_x + left, origin_y);
            layout_node(ctx, content, style, origin_x + left + step, origin_y + raise);
            layout_node(ctx, content, style, origin_x + left + 2.0 * step, origin_y);
            MathBox::with_dims(b.w + left + 2.0 * step, b.h + raise, b.d)
        }

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
                (params.axis_height * 2.0).max(font_size)
            });
            let y_center = origin_y + axis_h;
            let b = layout_delimiter(
                ctx, delim, false, origin_x, y_center, target_h, font_size, rule_t,
            );
            MathBox::with_dims(b.w, (axis_h + b.h).max(0.0), (b.d - axis_h).max(0.0))
        }

        MathNode::SizedDelim { delim, size_tier, is_left, .. } => {
            // The TeXbook Chapter 17: Delimiter sizing hierarchy (\big, \Big, \bigg, \Bigg)
            // amsmath `\bBigg@`: `\left` around an empty box 1, 1.5, 2 or 2.5 times
            // `\big@size` (1.2em of the text font) tall, centred on the axis, so the
            // delimiter must reach Rule 19's max(0.901 z, z - 5pt).
            let factor = [1.0f32, 1.5, 2.0, 2.5][(*size_tier as usize).min(3)];
            let y_center = origin_y + axis_h;
            if ctx.font_family == metrics::FontFamily::ComputerModern {
                // `ig@size` is 1.2 times the height plus 1.2 times the depth of the text
                // font's `(` (`\Mathstrutbox@`); `center to` that many `ig@size` is
                // centred on the axis (§736) and `make_left_right` (§762) sizes the delimiter.
                use crate::backend::scaled::{em_mul, half, sp_pt, to_sp};
                let paren = crate::math::math_tfm::glyph_dims(Face::Roman, b'(', ctx.base_font_size, ctx.font_family)
                    .map_or((0.0, 0.0), |g| (g.h, g.d));
                let big = to_sp(em_mul(1.2, paren.0)) + to_sp(em_mul(1.2, paren.1));
                let v = to_sp(em_mul(factor, sp_pt(big)));
                let axis = to_sp(axis_h);
                let (vc_h, vc_d) = (half(v) + axis, v - (half(v) + axis));
                let target = left_right_target_sp(vc_h, vc_d, axis, 901, to_sp(5.0));
                let b = layout_delimiter(ctx, delim, *is_left, origin_x, y_center, sp_pt(target), font_size, rule_t);
                return MathBox::with_dims(b.w, sp_pt(vc_h).max(axis_h + b.h), sp_pt(vc_d).max(b.d - axis_h));
            }
            let z = factor * 1.2 * ctx.base_font_size;
            let target_h = (z * 0.901).max(z - 5.0 * ctx.base_font_size / 10.0);
            let b = layout_delimiter(
                ctx, delim, *is_left, origin_x, y_center, target_h, font_size, rule_t,
            );
            MathBox::with_dims(b.w, (axis_h + b.h).max(0.0), (b.d - axis_h).max(0.0))
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
            let mut b = layout_node(ctx, content, style, origin_x, origin_y);
            // A list nucleus is a `clean_box` (§720): its last character's italic correction
            // kern (§755) is inside it.
            if ctx.font_family == metrics::FontFamily::ComputerModern && !is_char_like(content) {
                b.w += node_italic_correction_styled(content, ctx.face_override, ctx.font_family, style, &ctx.font_tier);
            }
            b
        }

        // Painted by the matrix that holds it, across the columns it spans.
        MathNode::HDotsFor { .. } => MathBox::zero(),
        MathNode::Ellipsis(kind) => {
            layout_ellipsis(ctx, *kind, style, origin_x, origin_y, font_size, axis_h)
        }

        MathNode::Tagged { formula, .. } => {
            layout_node(ctx, formula, style, origin_x, origin_y)
        }

        MathNode::Strut => {
            // LaTeX's `\strutbox`: 0.7 and 0.3 of the normal-size `\baselineskip` (1.2em).
            let bls = 1.2 * ctx.base_font_size;
            let h = crate::backend::scaled::em_mul(0.7, bls);
            let d = crate::backend::scaled::em_mul(0.3, bls);
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
            // (`SideSetLead`) precedes this nucleus in the list.
            let (op, pre, post) = &**parts;
            let wd0 = measure_math(op, ctx.font_tier.font_size(MathStyle::Display), ctx.font_family, MathStyle::Display).w;
            let pre_start = ctx.out_ops_len();
            let pre_box = layout_node(ctx, pre, MathStyle::Display, 0.0, origin_y);
            let pre_end = ctx.out_ops_len();
            let post_start = ctx.out_ops_len();
            let post_box = layout_node(ctx, post, MathStyle::Display, 0.0, origin_y);
            let post_end = ctx.out_ops_len();
            let dimen = pre_box.w + wd0 - post_box.w;
            ctx.shift_ops_range(pre_start, pre_end, origin_x - dimen, 0.0);
            ctx.shift_ops_range(post_start, post_end, origin_x - dimen + pre_box.w, 0.0);
            MathBox::with_dims(pre_box.w + post_box.w - dimen, pre_box.h.max(post_box.h), pre_box.d.max(post_box.d))
        }

        MathNode::VarLim { kind, lim } => layout_varlim(ctx, *kind, lim, style, origin_x, origin_y),

        MathNode::InnerAlign(inner) => {
            let outer = std::mem::replace(&mut ctx.inner_align, true);
            let b = layout_node(ctx, inner, style, origin_x, origin_y);
            ctx.inner_align = outer;
            // Numbered, its rows end with the number column at the display's right edge.
            let numbered = matches!(&**inner, MathNode::Matrix(m) if m.numbered_env);
            let w = if numbered { ctx.display_width.max(b.w) } else { b.w };
            MathBox::with_dims(w, b.h, b.d)
        }

        MathNode::SplitLead(split) => MathBox::with_dims(split_left_width(ctx, split), 0.0, 0.0),

        MathNode::SplitTail(split) => {
            let left = split_left_width(ctx, split);
            let b = layout_node(ctx, split, style, origin_x - left, origin_y);
            MathBox::with_dims(b.w - left, b.h, b.d)
        }

        MathNode::SideSetLead(parts) => {
            let (op, pre, post) = &**parts;
            let size = ctx.font_tier.font_size(MathStyle::Display);
            let m = |n: &MathNode<'a>| measure_math(n, size, ctx.font_family, MathStyle::Display).w;
            MathBox::with_dims(m(pre) + m(op) - m(post), 0.0, 0.0)
        }

        MathNode::Boxed(content) => {
            // `\fbox`: `\fboxsep` 3pt of padding inside a `\fboxrule` .4pt frame (as
            // `scan_dimen` reads them) around `$\displaystyle#1$`, its italic correction included.
            let (sep, rule) = (3.0, 26214.0 / 65536.0);
            let pad = sep + rule;
            let start = ctx.out_ops_len();
            let mut inner = layout_node(ctx, content, MathStyle::Display, origin_x + pad, origin_y);
            inner.w += node_italic_correction_styled(content, ctx.face_override, ctx.font_family, MathStyle::Display, &ctx.font_tier);
            let w = inner.w + 2.0 * pad;
            let (top, bot) = (origin_y + inner.h + sep, origin_y - inner.d - pad);
            if !ctx.is_measuring() {
                // `\@frameb@x`: a vbox of the top rule, an hbox of `\vrule`, the padded content
                // and `\vrule` (as tall as the padded content), and the bottom rule.
                let side = top - bot - rule;
                let before = ctx.out_ops_len();
                ctx.emit_rule(origin_x, top, w, rule);
                ctx.emit_rule(origin_x, bot + rule, rule, side);
                let n = ctx.out_ops_len() - before;
                ctx.move_last_ops_to(start, n);
                ctx.emit_rule(origin_x + w - rule, bot + rule, rule, side);
                ctx.emit_rule(origin_x, bot, w, rule);
            }
            MathBox::with_dims(w, inner.h + pad, inner.d + pad)
        }

        MathNode::DimensionBox { content, zero_width, shift_left, smash } => {
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
            let h = if smash & 1 != 0 { 0.0 } else { painted_box.h };
            let d = if smash & 2 != 0 { 0.0 } else { painted_box.d };
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
    let axis_h = params.axis_height;
    let unit = ctx.base_font_size / 10.0;
    let (cell_style, baselineskip, lineskip, lineskiplimit, col_sep, outer) = match env_kind {
        MatrixEnvKind::Subarray => {
            // amsmath: \baselineskip is fontdimens 10 + 12 of \scriptfont2 (cmsy7's num3 and
            // denom2); \lineskip and \lineskiplimit are 3 × fontdimen 8 of \scriptfont3 (cmex7).
            let s = ctx.font_tier.font_size(MathStyle::Script);
            let sy = ctx.font_params(MathStyle::Script);
            let _ = s;
            let ls = 3.0 * sy.rule_thickness;
            (MathStyle::Script, sy.num3 + sy.denom2, ls, ls, 0.0, 0.0)
        }
        MatrixEnvKind::Overlay => (style, 0.0, 0.0, f32::NEG_INFINITY, 0.0, 0.0),
        MatrixEnvKind::Vereq => (style, 0.0, -0.5 * unit, f32::INFINITY, 0.0, 0.0),
        _ => {
            // amsgen's `\ex@` for the text size.
            let ex_at = amsgen_ex_at(ctx.base_font_size);
            let thin = ctx.skip_width(crate::math::math_ast::MathSkip::Mu(3.0), style);
            let em = crate::backend::scaled::em_mul;
            // The `\thickspace` between columns is in text mode: `.2777em` of the text font.
            let quad = crate::backend::scaled::face_quad_pt(ctx.font_family, ctx.text_face, ctx.base_font_size);
            (MathStyle::Script, em(6.0, ex_at), em(1.5, ex_at), em(1.5, ex_at), em(0.2777, quad), thin)
        }
    };

    let n_cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut col_w = vec![0.0f32; n_cols];
    let mut row_hd = vec![(0.0f32, 0.0f32); rows.len()];
    let mut cells: Vec<(usize, usize, f32)> = Vec::with_capacity(rows.len() * n_cols);
    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            let start = ctx.out_ops_len();
            let mut b = layout_node(ctx, cell, cell_style, 0.0, 0.0);
            b.w += node_italic_correction_styled(cell, ctx.face_override, ctx.font_family, cell_style, &ctx.font_tier);
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
    let text = ctx.base_font_size;
    let ex_at = amsgen_ex_at(ctx.base_font_size);
    let dot = crate::math::math_tfm::glyph_dims(Face::Roman, b'.', text, ctx.font_family)
        .unwrap_or(crate::math::math_tfm::GlyphDims { w: 0.278 * text, h: 0.106 * text, d: 0.0, ic: 0.0 });
    let (dot_w, dot_h) = (dot.w, dot.h);
    // `\,` in text: amsmath's `\tmspace+\thinmuskip{.1667em}`, a kern of the text font's em.
    let thin = crate::backend::scaled::em_mul(0.1667, crate::backend::scaled::quad_pt(ctx.font_family, text));
    let sup_w = thin + n_dots as f32 * dot_w;
    let width = nuc_box.w.max(sup_w);
    // A narrower limit is reboxed (§715), which hpacks the vbox and clamps its height to 0.
    let sup_h = if sup_w < width { 0.0 } else { -1.4 * ex_at };
    ctx.shift_ops_range(nuc_start, nuc_end, 0.5 * (width - nuc_box.w), 0.0);
    let shift_up = (params.big_op_spacing1).max(params.big_op_spacing3);
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
    let bos5 = params.big_op_spacing5;
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
/// amsgen's `\ex@` for the current text size: 1pt at 10pt, scaled by .97 per 0.5pt step.
fn amsgen_ex_at(size: f32) -> f32 {
    // `\compute@ex@` in scaled points: 1pt moved by 1 - .97^k toward 0.5pt (smaller sizes)
    // or 1.5pt, k the number of whole points in twice the distance from 10pt.
    use crate::backend::scaled::{sp_pt, times_unit_f, to_sp};
    const PT: i32 = 65536;
    let mut dimen = -to_sp(size);
    if dimen < -20 * PT {
        return 1.5;
    }
    dimen = (dimen + 10 * PT) * 2;
    let raise = dimen <= 0;
    dimen = dimen.abs() - 1000;
    let mut vfuzz = PT;
    while dimen > 0 {
        vfuzz = times_unit_f(0.97, vfuzz as i64) as i32;
        dimen -= PT;
    }
    let delta = PT - vfuzz;
    sp_pt(if raise { PT + delta } else { PT - delta })
}

/// amsmath `\arrowfill@#1#2#3` set `width` wide on baseline `y`: the end glyphs `left` and
/// `right`, and between them, overlapped by `\mkern-7mu` each side, `\cleaders` of
/// `\hbox{$\mkern-2mu\relbar\mkern-2mu$}` with the leftover split evenly.
#[allow(clippy::too_many_arguments)]
fn emit_arrow_fill(ctx: &mut MathLayoutContext<'_, '_>, face: Face, size: f32, mu: f32, left: u8, right: u8, x: f32, width: f32, y: f32) {
    let metric = |c: u8| {
        crate::math::math_tfm_faces::cm_face_metric(face, c).unwrap_or_else(|| crate::math::math_tfm::cmsy10_metric(c))
    };
    let em = |v: f32| v * 0.001 * size;
    let (lw, rw, bar_w) = (em(metric(left).w), em(metric(right).w), em(metric(0x00).w));
    // Shipped in list order: #1, the `\cleaders` copies left to right, #3.
    ctx.emit_glyph_run(x, y, face, size, byte_str(left));
    let span = width - lw - rw + 14.0 * mu;
    let leader = bar_w - 4.0 * mu;
    if leader > 0.0 && span >= leader {
        let n = (span / leader).floor();
        let start = x + lw - 7.0 * mu + 0.5 * (span - n * leader);
        for i in 0..n as usize {
            ctx.emit_glyph_run(start + i as f32 * leader - 2.0 * mu, y, face, size, byte_str(0x00));
        }
    }
    ctx.emit_glyph_run(x + width - rw, y, face, size, byte_str(right));
}

/// amsopn's `\varlimsup` (`\@@overline`), `\varliminf` (`\@@underline` of the box made
/// .2`\ex@` deep) and `\varinjlim`/`\varprojlim` (`\varlim@`: a `\vtop` of `lim`, a
/// 1.5`\ex@` kern, a `\textstyle` arrow fill as wide as the column, and a -`\ex@` kern).
fn layout_varlim<'a, 'p>(
    ctx: &mut MathLayoutContext<'a, 'p>,
    kind: u8,
    lim: &MathNode<'a>,
    style: MathStyle,
    origin_x: f32,
    origin_y: f32,
) -> MathBox {
    let start = ctx.out_ops_len();
    let b = layout_node(ctx, lim, style, 0.0, 0.0);
    let end = ctx.out_ops_len();
    let params = ctx.font_params(style);
    let theta = params.rule_thickness;
    let ex_at = amsgen_ex_at(ctx.base_font_size);
    match kind {
        0 | 1 => {
            ctx.shift_ops_range(start, end, origin_x, origin_y);
            if kind == 0 {
                ctx.emit_rule(origin_x, origin_y + b.h + 3.0 * theta, b.w, theta);
                MathBox::with_dims(b.w, b.h + 5.0 * theta, b.d)
            } else {
                let d = b.d.max(0.2 * ex_at);
                ctx.emit_rule(origin_x, origin_y - d - 4.0 * theta, b.w, theta);
                MathBox::with_dims(b.w, b.h, d + 5.0 * theta)
            }
        }
        _ => {
            let text = MathStyle::Text;
            let size = ctx.font_tier.font_size(text);
            let face = resolve_optical_face(Face::Symbol, ctx.font_tier.font_size(text), ctx.font_family);
            let mu = ctx.skip_width(crate::math::math_ast::MathSkip::Mu(1.0), text);
            let (left, right) = if kind == 2 { (0x00, 0x21) } else { (0x20, 0x00) };
            let head = if kind == 2 { right } else { left };
            let metric = |c: u8| {
                crate::math::math_tfm_faces::cm_face_metric(face, c).unwrap_or_else(|| crate::math::math_tfm::cmsy10_metric(c))
            };
            let em = |v: f32| v * 0.001 * size;
            let natural = em(metric(left).w) + em(metric(right).w) - 14.0 * mu;
            let w = b.w.max(natural);
            ctx.shift_ops_range(start, end, origin_x + 0.5 * (w - b.w), origin_y);
            // The arrow row's height and depth are its arrowhead's, from the TFM.
            let (row_h, row_d) = crate::math::math_tfm::glyph_dims(face, head, size, ctx.font_family)
                .map_or((em(metric(head).h), em(metric(head).d)), |g| (g.h, g.d));
            let row_y = origin_y - b.d - 1.5 * ex_at - row_h;
            if !ctx.is_measuring() {
                emit_arrow_fill(ctx, face, size, mu, left, right, origin_x, w, row_y);
            }
            MathBox::with_dims(w, b.h, b.d + 1.5 * ex_at + row_h + row_d - ex_at)
        }
    }
}

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
    let face = resolve_optical_face(Face::Symbol, ctx.font_tier.font_size(style), ctx.font_family);
    let metric = |c: u8| {
        crate::math::math_tfm_faces::cm_face_metric(face, c).unwrap_or_else(|| crate::math::math_tfm::cmsy10_metric(c))
    };
    let em = |v: f32| v * 0.001 * font_size;
    let mu = ctx.skip_width(crate::math::math_ast::MathSkip::Mu(1.0), style);
    let (lw, rw) = (em(metric(left).w), em(metric(right).w));
    let row_h = crate::math::math_tfm::glyph_dims(face, 0x21, font_size, ctx.font_family).map_or(em(metric(0x21).h), |g| g.h);
    let content_w = nuc_box.w + ic;
    let width = content_w.max(lw + rw - 14.0 * mu);
    ctx.shift_ops_range(nuc_start, nuc_end, 0.5 * (width - content_w), 0.0);

    let below_gap = 1.3 * (ctx.base_font_size / 10.0);
    let row_y = if below { origin_y - nuc_box.d - below_gap - row_h } else { origin_y + nuc_box.h };
    if !ctx.is_measuring() {
        emit_arrow_fill(ctx, face, font_size, mu, left, right, origin_x, width, row_y);
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
    let axis_h = params.axis_height;

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
    let ic = node_italic_correction_styled(content, ctx.face_override, ctx.font_family, style, &ctx.font_tier);
    let mut dy = 0.0;
    let delta = if is_char { ic } else { 0.0 };
    if is_char {
        // §749: shift_amount(x) := half(height(x) − depth(x)) − axis_height.
        use crate::backend::scaled::{half, sp_pt, to_sp};
        let v = sp_pt(half(to_sp(nuc.h) - to_sp(nuc.d)) - to_sp(axis_h));
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
            &params,
            ctx.script_drops(style),
        );
        let script_space = SCRIPT_SPACE_PT;
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
    let metrics = BigOpShiftResolver::resolve_limit_metrics(ctx.font_family, font_size, &params);
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
    // §750 stacks superscript, operator, subscript: drawn in that order.
    debug_assert!(nuc_end == sup_start);
    ctx.swap_op_ranges(nuc_start, nuc_end, sup_end);
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
        params.big_op_spacing5,
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

/// The `(cols, space)` of an `\hdotsfor` cell.
fn hdotsfor_cell(cell: &MathNode<'_>) -> Option<(u8, f32)> {
    match cell {
        MathNode::HDotsFor { cols, space } => Some((*cols, *space)),
        MathNode::Row(v) => match v.as_slice() {
            [MathNode::HDotsFor { cols, space }] => Some((*cols, *space)),
            _ => None,
        },
        _ => None,
    }
}

/// amsmath `\hdots@for`: `\multicolumn` content `\mkern-s\dotsspace@ \xleaders\hbox{$\mkern
/// s\dotsspace@.\mkern s\dotsspace@$}\hfill \mkern-s\dotsspace@` between the spanned
/// columns' outer edges `x0` and `x1` (inside their `\arraycolsep`). The leader boxes are
/// spaced as tex.web §627 does for `\xleaders`.
fn paint_hdotsfor(ctx: &mut MathLayoutContext<'_, '_>, space: f32, x0: f32, x1: f32, y: f32, style: MathStyle) {
    let font_size = ctx.font_tier.font_size(style);
    let kern = space * ctx.skip_width(crate::math::math_ast::MathSkip::Mu(1.5), style);
    let (face, dot_w, glyph) = if ctx.font_family == metrics::FontFamily::ComputerModern {
        let face = resolve_optical_face(Face::MathItalic, ctx.font_tier.font_size(style), ctx.font_family);
        let w = crate::math::math_tfm_faces::cm_face_metric(face, 0x3A).map_or(0.0, |m| m.w);
        (face, w * font_size * 0.001, crate::math::math_delim::byte_str(0x3A))
    } else {
        (Face::Roman, cm_or_base14_width('.', Face::Roman, font_size, ctx.font_family), ".")
    };
    let leader_w = dot_w + 2.0 * kern;
    let rule_w = x1 - x0 + 2.0 * kern;
    if leader_w <= 0.0 || rule_w < leader_w {
        return;
    }
    let lq = (rule_w / leader_w).floor();
    let lr = rule_w - lq * leader_w;
    let lx = lr / (lq + 1.0);
    let mut x = x0 - kern + (lr - (lq - 1.0) * lx) * 0.5;
    for _ in 0..lq as u32 {
        ctx.emit_glyph_run(x + kern, y, face, font_size, glyph);
        x += leader_w + lx;
    }
}

fn layout_ellipsis<'a, 'p>(
    ctx: &mut MathLayoutContext<'a, 'p>,
    kind: crate::math::math_ast::EllipsisKind,
    style: MathStyle,
    origin_x: f32,
    origin_y: f32,
    font_size: f32,
    axis_h: f32,
) -> MathBox {
    use crate::math::math_ast::EllipsisKind;

    // latex.ltx `\mathellipsis` is `\mathinner{\ldotp\ldotp\ldotp}` and `\cdots` is
    // `\mathinner{\cdotp\cdotp\cdotp}`: three Punct atoms (cmmi `.`, cmsy `\cdot`) of the
    // current style, a thin space apart except in script styles (TeXbook Chapter 18).
    if ctx.font_family == metrics::FontFamily::ComputerModern && matches!(kind, EllipsisKind::Low | EllipsisKind::Centered) {
        let (base, code) = if kind == EllipsisKind::Low { (Face::MathItalic, 0x3A) } else { (Face::Symbol, 0x01) };
        let face = resolve_optical_face(base, ctx.font_tier.font_size(style), ctx.font_family);
        let m = crate::math::math_tfm::face_tfm(face)
            .map_or(crate::math::math_tfm::GlyphDims { w: 0.0, h: 0.0, d: 0.0, ic: 0.0 }, |t| {
                crate::math::math_tfm::GlyphDims::of(t, code, font_size)
            });
        let w_dot = m.w;
        let thin = if style.tier() <= 1 { ctx.skip_width(crate::math::math_ast::MathSkip::Mu(3.0), style) } else { 0.0 };
        let glyph = crate::math::math_delim::byte_str(code);
        for i in 0..3 {
            ctx.emit_glyph_run(origin_x + i as f32 * (w_dot + thin), origin_y, face, font_size, glyph);
        }
        return MathBox::with_dims(3.0 * w_dot + 2.0 * thin, m.h, m.d);
    }

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
    let size_ratio = font_size / 10.0;
    let params = ctx.font_params(style);
    let rule_t = params.rule_thickness;
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
    let sup_clearance = (params.big_op_spacing1)
        .max(params.big_op_spacing3 - sup_box.d)
        .max(2.0 * size_ratio - sup_box.d);
    let extra_pad = params.big_op_spacing5;
    let sup_y = origin_y + inner_top_h + sup_clearance + sup_box.d;
    let sup_x = origin_x + (total_w - sup_box.w) * 0.5;

    let sub_clearance = (params.big_op_spacing2)
        .max(params.big_op_spacing4 - sub_box.h)
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
    let size_ratio = font_size / 10.0;
    let params = ctx.font_params(style);
    let rule_t = params.rule_thickness;
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
    let sup_clearance = (params.big_op_spacing1)
        .max(params.big_op_spacing3 - sup_box.d)
        .max(2.0 * size_ratio - sup_box.d);
    let extra_pad = params.big_op_spacing5;
    let sup_y = origin_y + inner_top_h + sup_clearance + sup_box.d;
    let sup_x = origin_x + (total_w - sup_box.w) * 0.5;

    let sub_clearance = (params.big_op_spacing2)
        .max(params.big_op_spacing4 - sub_box.h)
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
    measure_math_in(node, base_font_size, font_family, style, Face::Roman)
}

/// [`measure_math`] for a formula set in text font `text_face`.
pub fn measure_math_in<'a>(
    node: &MathNode<'a>,
    base_font_size: f32,
    font_family: FontFamily,
    style: MathStyle,
    text_face: Face,
) -> MathBox {
    measure_display(node, base_font_size, font_family, style, text_face, 0.0, 1)
}

/// [`measure_math_in`] for a display `display_width` wide.
pub fn measure_display<'a>(
    node: &MathNode<'a>,
    base_font_size: f32,
    font_family: FontFamily,
    style: MathStyle,
    text_face: Face,
    display_width: f32,
    first_equation: u32,
) -> MathBox {
    let mut ctx = MathLayoutContext::new(base_font_size, font_family, None, None, None, style);
    ctx.text_face = text_face;
    ctx.display_width = display_width;
    ctx.first_equation = first_equation;
    let mut b = layout_node(&mut ctx, node, style, 0.0, 0.0);
    let trailing_ic = node_italic_correction_styled(node, ctx.face_override, ctx.font_family, style, &ctx.font_tier);
    b.w += trailing_ic;
    // The formula is an hbox, whose height and depth start at zero (TeX §653).
    b.h = b.h.max(0.0);
    b.d = b.d.max(0.0);
    b
}

/// Style of a matrix cell: amsmath's alignments set each cell `\displaystyle`.
fn matrix_cell_style(env_kind: MatrixEnvKind, style: MathStyle) -> MathStyle {
    match env_kind {
        MatrixEnvKind::SmallMatrix => MathStyle::Script,
        MatrixEnvKind::Aligned | MatrixEnvKind::AlignedAt | MatrixEnvKind::Gather | MatrixEnvKind::Multline => {
            MathStyle::Display
        }
        _ => style.matrix_cell_style(),
    }
}

/// Rows of a top-level amsmath display alignment (`align`, `gather`, …), which TeX sets
/// as lines of the vertical list rather than as one box.
pub struct DisplayRows {
    /// Baseline of each row above the formula baseline that `render_math` uses.
    pub baselines: Vec<f32>,
    /// Whether each row carries an equation number (all false for unnumbered forms).
    pub numbered: Vec<bool>,
    /// Height of the first row and depth of the last, for the interline glue around it.
    pub first_h: f32,
    pub last_d: f32,
    /// Baselines of the first and last rows of the display's own alignment (a nested
    /// alignment's rows lie inside one of its rows).
    pub outer_first: f32,
    pub outer_last: f32,
}

/// The rows of `node` when it is a multi-row display alignment, else `None`.
/// Whether the display is one amsmath `split`, which amsmath sets as a display alignment.
/// amsmath `\measure@` and `\calc@shift@align` (numbers on the right, not `fleqn`) for a
/// display `align`: (\eqnshift@, \alignsep@).
fn align_shift_and_sep(
    ctx: &MathLayoutContext,
    data: &crate::math::math_ast::MatrixData<'_>,
    col_widths: &[f32],
    cell_widths: &[f32],
    num_cols: usize,
) -> (f32, f32) {
    // amsmath `\measure@` and `\x@calc@shift@rc` in scaled points: `\divide` truncates.
    use crate::backend::scaled::{half, sp_pt};
    let to_sp = |x: f32| crate::backend::scaled::to_sp(x) as i64;
    let dw = to_sp(ctx.display_width);
    let size = ctx.base_font_size;
    let min_sep = to_sp(size);
    // \mintagsep: half the quad of \textfont2.
    let min_tag_sep = half(crate::backend::scaled::to_sp(ctx.font_params(MathStyle::Text).quad)) as i64;
    let tot: i64 = col_widths.iter().map(|&w| to_sp(w)).sum();
    let n = num_cols.div_ceil(2) as i64;
    let mut eqnshift = (dw - tot) / (n + 1);
    let mut sep = eqnshift;
    if sep < min_sep {
        sep = min_sep;
        if eqnshift > 0 {
            eqnshift = (dw - tot - (n - 1) * sep) / 2;
        }
    }
    eqnshift = eqnshift.max(0);
    let family = ctx.font_family;
    let paren_w = to_sp(crate::font_metrics::width_family('(', Face::Roman, size, family))
        + to_sp(crate::font_metrics::width_family(')', Face::Roman, size, family));
    let tags = display_row_tags_of(data);
    let mut number = ctx.first_equation;
    for (r, row) in data.rows.iter().enumerate() {
        let tag_w = match tags.get(r) {
            Some(Some((tag, starred))) => {
                to_sp(measure_math(tag, size, family, MathStyle::Text).w) + if *starred { 0 } else { paren_w }
            }
            _ if data.numbered_env && (r >= 64 || data.notag_mask & (1 << r) == 0) => {
                let w = paren_w
                    + number.to_string().chars().map(|c| to_sp(crate::font_metrics::width_family(c, Face::Roman, size, family))).sum::<i64>();
                number += 1;
                w
            }
            _ => 0,
        };
        if tag_w <= 0 {
            continue;
        }
        // `\x@rcalc@width`: the row's width to the end of its last non-empty field.
        let (mut pending, mut used) = (0i64, 0i64);
        for c in 0..row.len().min(num_cols) {
            let field = to_sp(cell_widths[r * num_cols + c]);
            if field > 0 {
                used += pending;
                if c % 2 == 0 {
                    used += to_sp(col_widths[c]);
                    pending = 0;
                } else {
                    used += field;
                    pending = to_sp(col_widths[c]) - field;
                }
            } else {
                pending += to_sp(col_widths[c]);
            }
        }
        let mut seps = n - 1;
        let mut parts = n + 1;
        let half_fields = (row.len() as i64 - 1) / 2;
        if seps > half_fields {
            parts -= seps - half_fields;
            seps = half_fields;
        }
        let with_tag = used + tag_w;
        if min_sep * seps + 2 * min_tag_sep + with_tag > dw {
            // The number goes on a line of its own (`\saveshift@1`).
            continue;
        }
        if eqnshift + with_tag + seps * sep + tag_w > dw {
            let d = (dw - with_tag) / parts;
            if d < min_sep {
                sep = min_sep;
                eqnshift = (dw - with_tag - seps * sep) / 2;
            } else {
                if d < eqnshift {
                    eqnshift = d.max(0);
                }
                if d < sep {
                    sep = d;
                }
            }
        }
    }
    (sp_pt(eqnshift as i32), sp_pt(sep as i32))
}

fn display_row_tags_of<'b, 'a>(data: &'b crate::math::math_ast::MatrixData<'a>) -> Vec<Option<(&'b MathNode<'a>, bool)>> {
    fn tag_of<'b, 'a>(n: &'b MathNode<'a>) -> Option<(&'b MathNode<'a>, bool)> {
        match n {
            MathNode::Tagged { tag, starred, .. } => Some((&**tag, *starred)),
            MathNode::Row(v) => v.iter().find_map(tag_of),
            _ => None,
        }
    }
    data.rows.iter().map(|row| row.iter().find_map(tag_of)).collect()
}

/// The `\tag` (node, starred) of each row of a display alignment, numbered or not.
pub fn display_row_tags<'b, 'a>(node: &'b MathNode<'a>) -> Vec<Option<(&'b MathNode<'a>, bool)>> {
    let node = match node {
        MathNode::Row(items) if items.len() == 1 => &items[0],
        other => other,
    };
    let MathNode::Matrix(data) = node else { return Vec::new() };
    let mut tags = Vec::with_capacity(data.rows.len());
    for (row, tag) in data.rows.iter().zip(display_row_tags_of(data)) {
        match numbered_inner_align(row) {
            Some(inner) => tags.extend(display_row_tags_of(inner)),
            None => tags.push(tag),
        }
    }
    tags
}

/// The numbered alignment a display row holds (amsmath `\ingather@`): its rows are numbered
/// as rows of the display.
fn numbered_inner_align<'b, 'a>(row: &'b [MathNode<'a>]) -> Option<&'b crate::math::math_ast::MatrixData<'a>> {
    fn find<'b, 'a>(n: &'b MathNode<'a>) -> Option<&'b crate::math::math_ast::MatrixData<'a>> {
        match n {
            MathNode::InnerAlign(inner) => match &**inner {
                MathNode::Matrix(m) if m.numbered_env => Some(m),
                _ => None,
            },
            MathNode::Row(v) => v.iter().find_map(find),
            _ => None,
        }
    }
    row.iter().find_map(find)
}

/// Width of a `split`'s left column: its widest first cell, in display style.
fn split_left_width(ctx: &MathLayoutContext, split: &MathNode<'_>) -> f32 {
    let MathNode::Matrix(data) = split else { return 0.0 };
    let size = ctx.font_tier.font_size(MathStyle::Display);
    data.rows
        .iter()
        .filter_map(|r| r.first())
        .map(|c| measure_math(c, size, ctx.font_family, MathStyle::Display).w)
        .fold(0.0, f32::max)
}

pub fn is_split_display(node: &MathNode<'_>) -> bool {
    let node = match node {
        MathNode::Row(items) if items.len() == 1 => &items[0],
        other => other,
    };
    matches!(node, MathNode::Matrix(data) if data.split)
}

/// The lines of a `multline`, each one cell.
pub fn multline_rows<'b, 'a>(node: &'b MathNode<'a>) -> Option<&'b [Vec<MathNode<'a>>]> {
    let node = match node {
        MathNode::Row(items) if items.len() == 1 => &items[0],
        other => other,
    };
    match node {
        MathNode::Matrix(data) if data.env_kind == MatrixEnvKind::Multline && !data.rows.is_empty() => Some(&data.rows),
        _ => None,
    }
}

/// The `\multlinegap` of a `multline` display.
pub fn multline_gap(node: &MathNode<'_>) -> f32 {
    let node = match node {
        MathNode::Row(items) if items.len() == 1 => &items[0],
        other => other,
    };
    match node {
        MathNode::Matrix(data) => data.multline_gap(),
        _ => 10.0,
    }
}

pub fn display_rows(node: &MathNode<'_>, base_font_size: f32, font_family: FontFamily) -> Option<DisplayRows> {
    let node = match node {
        MathNode::Row(items) if items.len() == 1 => &items[0],
        other => other,
    };
    let MathNode::Matrix(data) = node else { return None };
    let env_kind = data.env_kind;
    if !data.display_env || data.rows.is_empty() {
        return None;
    }
    let style = MathStyle::Display;
    let ctx = MathLayoutContext::new(base_font_size, font_family, None, None, None, style);
    let font_size = ctx.font_tier.font_size(style);
    let axis_h = ctx.font_params(style).axis_height;
    let (min_rh, min_rd, row_sep) = crate::math::math_matrix::MatrixLayoutMetricsEngine::row_parameters_font(
        env_kind,
        font_size,
        // Computer Modern has LaTeX's \strutbox; the OpenType families their own strut.
        (font_family != FontFamily::ComputerModern).then(|| ctx.opentype_engine().constants()),
    );
    let cell_style = matrix_cell_style(env_kind, style);
    let n = data.rows.len();
    let mut heights = vec![0.0f32; n];
    let mut depths = vec![0.0f32; n];
    let mut dry = ctx.dry_context();
    for (r, row) in data.rows.iter().enumerate() {
        let (mut h, mut d) = (min_rh, min_rd);
        for cell in row {
            let b = layout_node(&mut dry, cell, cell_style, 0.0, 0.0);
            h = h.max(b.h);
            d = d.max(b.d);
        }
        heights[r] = h;
        depths[r] = d;
    }
    let mut offsets = vec![0.0f32; n];
    let total = crate::math::math_matrix::MatrixLayoutMetricsEngine::compute_row_baseline_offsets_and_total_height(
        &heights,
        &depths,
        &mut offsets,
        row_sep,
        crate::math::math_matrix::MatrixLayoutMetricsEngine::openup_baselineskip(env_kind, font_size),
        data.row_skips(),
    );
    let top = axis_h + crate::backend::scaled::sp_pt(crate::backend::scaled::half(crate::backend::scaled::to_sp(total)));
    let tags = display_row_tags_of(data);
    let (mut baselines, mut numbered) = (Vec::with_capacity(n), Vec::with_capacity(n));
    for r in 0..n {
        let y = top - offsets[r];
        // A nested numbered alignment is centred on its row's axis like the display itself.
        if let Some(inner) = numbered_inner_align(&data.rows[r]).and_then(|m| {
            display_rows(&MathNode::Matrix(Box::new(m.clone())), base_font_size, font_family)
        }) {
            baselines.extend(inner.baselines.iter().map(|b| y + b));
            numbered.extend(inner.numbered);
            continue;
        }
        baselines.push(y);
        numbered.push(tags[r].is_some() || data.numbered_env && (r >= 64 || data.notag_mask & (1 << r) == 0));
    }
    Some(DisplayRows {
        baselines,
        numbered,
        first_h: heights[0],
        last_d: depths[n - 1],
        outer_first: top - offsets[0],
        outer_last: top - offsets[n - 1],
    })
}

/// Size of `lmex10` in every style: `omxlmex.fd` declares it `sfixed`.
const LMEX_FIXED_SIZE: f32 = 10.0;

/// Total shrink of the `\medmuskip` glue (4mu minus 4mu) between the atoms of the
/// outermost row of a formula, and the number of those glues. Inner lists are packed boxes
/// whose glue is fixed.
pub fn top_level_shrink(node: &MathNode<'_>, base_font_size: f32, font_family: FontFamily, style: MathStyle) -> (f32, u32) {
    let MathNode::Row(nodes) = node else {
        return (0.0, 0);
    };
    let mut kinds: Vec<MathAtomKind> = nodes
        .iter()
        .filter(|c| !matches!(c, MathNode::Space(_) | MathNode::Nonscript))
        .map(|c| if matches!(c, MathNode::MiddleDelim { .. }) { MathAtomKind::Close } else { c.atom_kind_in_style(style) })
        .collect();
    normalize_atom_kinds(&mut kinds);
    let count = kinds.windows(2).filter(|k| atom_spacing_mu(k[0], k[1], style.is_display_or_text()) == 4).count() as u32;
    let quad = if font_family == FontFamily::ComputerModern {
        crate::math::math_fonts::math_font_parameters(font_family).quad as f32 * base_font_size * 0.001
    } else {
        base_font_size
    };
    (count as f32 * 4.0 * quad / 18.0, count)
}

/// `render_math` with each `\medmuskip` of the outermost row shrunk by `med_shrink`.
#[allow(clippy::too_many_arguments)]
pub fn render_math_shrunk<'a, 'p>(
    node: &MathNode<'a>,
    base_font_size: f32,
    font_family: FontFamily,
    style: MathStyle,
    origin_x: Sp,
    origin_y: Sp,
    ops: &'p mut Vec<Paint>,
    pool: &'p mut Vec<&'a str>,
    med_shrink: f32,
    text_face: Face,
    display_width: f32,
    first_equation: u32,
    row_ends: &'p mut Vec<usize>,
) -> MathBox {
    let first_op = ops.len();
    let mut ctx = MathLayoutContext::new(base_font_size, font_family, None, Some(ops), Some(pool), style);
    ctx.text_face = text_face;
    ctx.display_width = display_width;
    ctx.first_equation = first_equation;
    row_ends.clear();
    ctx.row_ends = Some(row_ends);
    if matches!(node, MathNode::Row(_)) {
        ctx.top_med_shrink = med_shrink;
    }
    let mut b = layout_node(&mut ctx, node, style, 0.0, 0.0);
    ctx.place_ops(first_op, origin_x, origin_y);
    let trailing_ic = node_italic_correction_styled(node, ctx.face_override, ctx.font_family, style, &ctx.font_tier);
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
    origin_x: Sp,
    origin_y: Sp,
    ops: &'p mut Vec<Paint>,
    pool: &'p mut Vec<&'a str>,
) -> MathBox {
    render_math_in(node, base_font_size, font_family, style, origin_x, origin_y, ops, pool, Face::Roman)
}

/// [`render_math`] for a formula set in text font `text_face`.
#[allow(clippy::too_many_arguments)]
pub fn render_math_in<'a, 'p>(
    node: &MathNode<'a>,
    base_font_size: f32,
    font_family: FontFamily,
    style: MathStyle,
    origin_x: Sp,
    origin_y: Sp,
    ops: &'p mut Vec<Paint>,
    pool: &'p mut Vec<&'a str>,
    text_face: Face,
) -> MathBox {
    let first_op = ops.len();
    let mut ctx = MathLayoutContext::new(
        base_font_size,
        font_family,
        None,
        Some(ops),
        Some(pool),
        style,
    );
    ctx.text_face = text_face;
    let mut b = layout_node(&mut ctx, node, style, 0.0, 0.0);
    ctx.place_ops(first_op, origin_x, origin_y);
    let trailing_ic = node_italic_correction_styled(node, ctx.face_override, ctx.font_family, style, &ctx.font_tier);
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
    let params = ctx.font_params(style);
    let axis_h = params.axis_height;
    let rule_t = params.rule_thickness;

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
        crate::backend::scaled::mu_pt(1.0, ctx.font_params(st).quad)
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
        let w = crate::math::math_tfm::glyph_dims(Face::Symbol, code, arrow_size, ctx.font_family)
            .map_or((crate::math::math_tfm::cmsy10_metric(code).w * 0.001) * arrow_size, |g| g.w);
        (w, origin_y, true)
    } else {
        let sym_idx = if is_left { 0xAC } else { 0xAE };
        let w = (metrics::SYMBOL_WIDTHS[sym_idx] as f32 * 0.001) * arrow_size;
        let shaft_y = origin_y + axis_h;
        (w, shaft_y - 0.13 * arrow_size, false)
    };

    // `\arrowfill@` in display style: arrow head(s) and one relbar overlapped by -7mu each side.
    let bar_w = if is_cm {
        crate::math::math_tfm::glyph_dims(Face::Symbol, 0x00, arrow_size, ctx.font_family)
            .map_or((crate::math::math_tfm::cmsy10_metric(0x00).w * 0.001) * arrow_size, |g| g.w)
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

    let arrow_start = ctx.out_ops_len();
    if is_cm {
        // amsmath `\arrowfill@#1#2#3`: #1, `\mkern-7mu`, `\cleaders` of
        // `\hbox{$\mkern-2mu#2\mkern-2mu$}` filling the rest, `\mkern-7mu`, #3, where the
        // ends are the arrow heads or `\relbar` (a minus).
        let mu_d = mu(ctx, MathStyle::Display);
        let left = if is_left || is_both { 0x20 } else { 0x00 };
        let right = if !is_left || is_both { 0x21 } else { 0x00 };
        emit_arrow_fill(ctx, Face::Symbol, arrow_size, mu_d, left, right, left_x, target_w, arrow_y);
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
        crate::math::math_tfm::glyph_dims(Face::Symbol, 0x21, arrow_size, ctx.font_family)
            .map_or((crate::math::math_tfm::cmsy10_metric(0x21).h * 0.001) * arrow_size, |g| g.h)
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
        let clr = (params.big_op_spacing1)
            .max(params.big_op_spacing3 - sup_box.d);
        let sup_y = origin_y + nuc_h + clr + sup_box.d;
        ctx.shift_ops_range(sup_start, sup_end, sup_x, sup_y);
        total_h = sup_y - origin_y + sup_box.h + params.big_op_spacing5;
    }

    if data.sub.is_some() {
        let m = mu(ctx, sub_style);
        let lim_w = (lim_pre + lim_post) * m + sub_box.w;
        let sub_x = origin_x + (target_w - lim_w) * 0.5 + lim_pre * m;
        let clr = (params.big_op_spacing2)
            .max(params.big_op_spacing4 - sub_box.h);
        let sub_y = origin_y - nuc_d - clr - sub_box.h;
        ctx.shift_ops_range(sub_start, sub_end, sub_x, sub_y);
        total_d = origin_y - sub_y + sub_box.d + params.big_op_spacing5;
    }
    // §750 `make_op`'s limits vlist ships the superscript, the arrow, the subscript.
    let n = ctx.out_ops_len() - arrow_start;
    ctx.move_last_ops_to(sub_start, n);

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
    let rule_t = params.rule_thickness;
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
    let rule_t = params.rule_thickness;
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
