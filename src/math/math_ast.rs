//! Mathematical Abstract Syntax Tree and style hierarchy.
//!
//! Provides the bit-packed math styles, atom representations, and recursive formula nodes.

use crate::backend::fonts::Face;
use crate::math::math_spacing::MathAtomKind;

/// TeX mathematical rendering styles governing relative font sizing and vertical clearances.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum MathStyle {
    /// Display style for standalone/block equations ($D$).
    #[default]
    Display = 0,
    /// Cramped display style ($D'$).
    CrampedDisplay = 1,
    /// Text style for inline equations ($T$).
    Text = 2,
    /// Cramped text style ($T'$).
    CrampedText = 3,
    /// Script style for first-level subscripts/superscripts ($S$).
    Script = 4,
    /// Cramped script style ($S'$).
    CrampedScript = 5,
    /// Script-script style for nested scripts ($SS$).
    ScriptScript = 6,
    /// Cramped script-script style ($SS'$).
    CrampedScriptScript = 7,
}

/// Bit-packed alias for mathematical style hierarchy.
pub type StyleHierarchy = MathStyle;

/// Branchless governor for cramped style transitions using bit-masking.
pub struct CrampedStyleGovernor;

impl CrampedStyleGovernor {
    /// Bit-masking transition to cramped style variant.
    #[inline(always)]
    pub const fn to_cramped(style: MathStyle) -> MathStyle {
        MathStyle::from_u8_clamped((style as u8) | 0x01)
    }

    /// Bit-masking transition to uncramped style variant.
    #[inline(always)]
    pub const fn to_uncramped(style: MathStyle) -> MathStyle {
        MathStyle::from_u8_clamped((style as u8) & !0x01)
    }

    /// Tests if a style is cramped in a single bitwise operation.
    #[inline(always)]
    pub const fn is_cramped(style: MathStyle) -> bool {
        ((style as u8) & 0x01) != 0
    }
}

/// 64-byte aligned static lookup table mapping each of the 8 styles directly
/// to font size scaling factor, eliminating runtime style-matching branches.
#[repr(align(64))]
pub struct StyleScaleLut {
    pub scales: [f32; 8],
}

pub static STYLE_SCALE_LUT: StyleScaleLut = StyleScaleLut {
    scales: [
        1.0,  // Display = 0
        1.0,  // CrampedDisplay = 1
        1.0,  // Text = 2
        1.0,  // CrampedText = 3
        0.70, // Script = 4
        0.70, // CrampedScript = 5
        0.50, // ScriptScript = 6
        0.50, // CrampedScriptScript = 7
    ],
};

/// 64-byte aligned static transition table mapping each style to its target style
/// across all TeX math operations (superscript, subscript, fraction numerator,
/// fraction denominator, matrix cell). Fits entirely within a single 64-byte L1 cache line.
#[repr(C, align(64))]
pub struct StyleTransitionLut {
    pub sup: [MathStyle; 8],
    pub sub: [MathStyle; 8],
    pub frac_num: [MathStyle; 8],
    pub frac_den: [MathStyle; 8],
    pub matrix_cell: [MathStyle; 8],
    pub _padding: [u8; 24],
}

pub static STYLE_TRANSITION_LUT: StyleTransitionLut = StyleTransitionLut {
    sup: [
        MathStyle::Script,              // Display -> Script
        MathStyle::CrampedScript,       // CrampedDisplay -> CrampedScript
        MathStyle::Script,              // Text -> Script
        MathStyle::CrampedScript,       // CrampedText -> CrampedScript
        MathStyle::ScriptScript,        // Script -> ScriptScript
        MathStyle::CrampedScriptScript, // CrampedScript -> CrampedScriptScript
        MathStyle::ScriptScript,        // ScriptScript -> ScriptScript
        MathStyle::CrampedScriptScript, // CrampedScriptScript -> CrampedScriptScript
    ],
    sub: [
        MathStyle::CrampedScript,       // Display -> CrampedScript
        MathStyle::CrampedScript,       // CrampedDisplay -> CrampedScript
        MathStyle::CrampedScript,       // Text -> CrampedScript
        MathStyle::CrampedScript,       // CrampedText -> CrampedScript
        MathStyle::CrampedScriptScript, // Script -> CrampedScriptScript
        MathStyle::CrampedScriptScript, // CrampedScript -> CrampedScriptScript
        MathStyle::CrampedScriptScript, // ScriptScript -> CrampedScriptScript
        MathStyle::CrampedScriptScript, // CrampedScriptScript -> CrampedScriptScript
    ],
    frac_num: [
        MathStyle::Text,                // Display -> Text
        MathStyle::CrampedText,         // CrampedDisplay -> CrampedText
        MathStyle::Script,              // Text -> Script
        MathStyle::CrampedScript,       // CrampedText -> CrampedScript
        MathStyle::ScriptScript,        // Script -> ScriptScript
        MathStyle::CrampedScriptScript, // CrampedScript -> CrampedScriptScript
        MathStyle::ScriptScript,        // ScriptScript -> ScriptScript
        MathStyle::CrampedScriptScript, // CrampedScriptScript -> CrampedScriptScript
    ],
    frac_den: [
        MathStyle::CrampedText,         // Display -> CrampedText
        MathStyle::CrampedText,         // CrampedDisplay -> CrampedText
        MathStyle::CrampedScript,       // Text -> CrampedScript
        MathStyle::CrampedScript,       // CrampedText -> CrampedScript
        MathStyle::CrampedScriptScript, // Script -> CrampedScriptScript
        MathStyle::CrampedScriptScript, // CrampedScript -> CrampedScriptScript
        MathStyle::CrampedScriptScript, // ScriptScript -> CrampedScriptScript
        MathStyle::CrampedScriptScript, // CrampedScriptScript -> CrampedScriptScript
    ],
    matrix_cell: [
        MathStyle::Text,                // Display -> Text
        MathStyle::CrampedText,         // CrampedDisplay -> CrampedText
        MathStyle::Text,                // Text -> Text
        MathStyle::CrampedText,         // CrampedText -> CrampedText
        MathStyle::Script,              // Script -> Script
        MathStyle::CrampedScript,       // CrampedScript -> CrampedScript
        MathStyle::ScriptScript,        // ScriptScript -> ScriptScript
        MathStyle::CrampedScriptScript, // CrampedScriptScript -> CrampedScriptScript
    ],
    _padding: [0; 24],
};

/// Contiguous stack-allocated array for math style scoping.
///
/// Exactly 32 bytes (`align(32)`), fitting in half of a 64-byte L1 cache line,
/// completely avoiding heap allocations during recursive style transitions.
#[derive(Clone, Copy, Debug)]
#[repr(C, align(32))]
pub struct StyleFrameStack {
    frames: [MathStyle; 31],
    depth: u8,
}

impl StyleFrameStack {
    /// Creates a new style stack initialized with a root style.
    #[inline(always)]
    pub const fn new(root: MathStyle) -> Self {
        let mut frames = [MathStyle::Display; 31];
        frames[0] = root;
        Self { frames, depth: 1 }
    }

    /// Pushes a new style onto the stack, saturating at maximum nesting depth.
    #[inline(always)]
    pub fn push(&mut self, style: MathStyle) {
        if (self.depth as usize) < self.frames.len() {
            self.frames[self.depth as usize] = style;
            self.depth += 1;
        }
    }

    /// Pops the top style from the stack.
    #[inline(always)]
    pub fn pop(&mut self) -> MathStyle {
        if self.depth > 1 {
            self.depth -= 1;
            self.frames[self.depth as usize]
        } else {
            self.frames[0]
        }
    }

    /// Returns the active style at top of stack.
    #[inline(always)]
    pub fn current(&self) -> MathStyle {
        self.frames[(self.depth - 1) as usize]
    }

    /// Current nesting depth.
    #[inline(always)]
    pub fn depth(&self) -> usize {
        self.depth as usize
    }

    /// Whether stack is at root level.
    #[inline(always)]
    pub fn is_root(&self) -> bool {
        self.depth <= 1
    }
}

/// Bit-packed 1-byte mathematical rendering context flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct MathContextFlags(pub u8);

impl MathContextFlags {
    pub const NONE: u8 = 0;
    pub const IN_DISPLAY: u8 = 1 << 0;
    pub const IN_FRACTION_NUMERATOR: u8 = 1 << 1;
    pub const IN_FRACTION_DENOMINATOR: u8 = 1 << 2;
    pub const IN_RADICAL: u8 = 1 << 3;
    pub const LIMITS_DEFAULT: u8 = 1 << 4;
    pub const IN_VCENTER: u8 = 1 << 5;
    pub const IN_SCRIPT: u8 = 1 << 6;
    pub const CRAMPED: u8 = 1 << 7;

    #[inline(always)]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[inline(always)]
    pub const fn contains(self, flag: u8) -> bool {
        (self.0 & flag) != 0
    }

    #[inline(always)]
    pub const fn with(self, flag: u8) -> Self {
        Self(self.0 | flag)
    }

    #[inline(always)]
    pub const fn without(self, flag: u8) -> Self {
        Self(self.0 & !flag)
    }

    #[inline(always)]
    pub fn set(&mut self, flag: u8) {
        self.0 |= flag;
    }

    #[inline(always)]
    pub fn unset(&mut self, flag: u8) {
        self.0 &= !flag;
    }
}

/// Sizing tier cache resolving font size switches in a single memory read.
///
/// Precomputes and caches the scaled font sizes for all 8 styles.
/// Aligned to 32 bytes (8 x f32 = 32 bytes) for single-vector L1 residency.
#[derive(Clone, Copy, Debug)]
#[repr(C, align(32))]
pub struct FontVariantTier {
    sizes: [f32; 8],
}

impl FontVariantTier {
    /// Precomputes font sizes across all 8 styles from a base font size.
    #[inline(always)]
    pub fn new(base_font_size: f32) -> Self {
        let text_sz = base_font_size;
        let script_sz = base_font_size * 0.70;
        let scriptscript_sz = base_font_size * 0.50;
        Self {
            sizes: [
                text_sz,         // Display = 0
                text_sz,         // CrampedDisplay = 1
                text_sz,         // Text = 2
                text_sz,         // CrampedText = 3
                script_sz,       // Script = 4
                script_sz,       // CrampedScript = 5
                scriptscript_sz, // ScriptScript = 6
                scriptscript_sz, // CrampedScriptScript = 7
            ],
        }
    }

    /// Resolves the font size for a given mathematical style in a single memory read.
    #[inline(always)]
    pub fn font_size(&self, style: MathStyle) -> f32 {
        self.sizes[(style as usize) & 0x07]
    }
}

impl MathStyle {
    /// Safe conversion from raw u8.
    #[inline(always)]
    pub const fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(MathStyle::Display),
            1 => Some(MathStyle::CrampedDisplay),
            2 => Some(MathStyle::Text),
            3 => Some(MathStyle::CrampedText),
            4 => Some(MathStyle::Script),
            5 => Some(MathStyle::CrampedScript),
            6 => Some(MathStyle::ScriptScript),
            7 => Some(MathStyle::CrampedScriptScript),
            _ => None,
        }
    }

    /// Clamped conversion from raw u8 (masks to lowest 3 bits).
    #[inline(always)]
    pub const fn from_u8_clamped(val: u8) -> Self {
        match val & 0x07 {
            0 => MathStyle::Display,
            1 => MathStyle::CrampedDisplay,
            2 => MathStyle::Text,
            3 => MathStyle::CrampedText,
            4 => MathStyle::Script,
            5 => MathStyle::CrampedScript,
            6 => MathStyle::ScriptScript,
            _ => MathStyle::CrampedScriptScript,
        }
    }

    /// Raw 1-byte representation.
    #[inline(always)]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Sizing tier (0 = Display, 1 = Text, 2 = Script, 3 = ScriptScript).
    #[inline(always)]
    pub const fn tier(self) -> u8 {
        (self as u8) >> 1
    }

    /// Returns the scale factor relative to the base font size via 64-byte aligned LUT.
    #[inline(always)]
    pub fn scale(self) -> f32 {
        STYLE_SCALE_LUT.scales[(self as usize) & 0x07]
    }

    /// Whether this style is in display mode.
    #[inline(always)]
    pub fn is_display(self) -> bool {
        (self as u8) < 2
    }

    /// Whether this style is display or text style (where Bin and Rel spacing is preserved).
    #[inline(always)]
    pub fn is_display_or_text(self) -> bool {
        (self as u8) < 4
    }

    /// Whether this style is a cramped variant.
    #[inline(always)]
    pub fn is_cramped(self) -> bool {
        CrampedStyleGovernor::is_cramped(self)
    }

    /// Style transition for superscripts.
    #[inline(always)]
    pub fn sup_style(self) -> Self {
        STYLE_TRANSITION_LUT.sup[(self as usize) & 0x07]
    }

    /// Style transition for subscripts (always enters cramped variant).
    #[inline(always)]
    pub fn sub_style(self) -> Self {
        STYLE_TRANSITION_LUT.sub[(self as usize) & 0x07]
    }

    /// Style transition for fraction numerator.
    #[inline(always)]
    pub fn frac_num_style(self) -> Self {
        STYLE_TRANSITION_LUT.frac_num[(self as usize) & 0x07]
    }

    /// Style transition for fraction denominator (always cramped).
    #[inline(always)]
    pub fn frac_den_style(self) -> Self {
        STYLE_TRANSITION_LUT.frac_den[(self as usize) & 0x07]
    }

    /// Returns the corresponding cramped style variant.
    #[inline(always)]
    pub fn cramped(self) -> Self {
        CrampedStyleGovernor::to_cramped(self)
    }

    /// Returns the corresponding uncramped style variant.
    #[inline(always)]
    pub fn uncramped(self) -> Self {
        CrampedStyleGovernor::to_uncramped(self)
    }

    /// Style for matrix/array cells (converts display style to text style).
    #[inline(always)]
    pub fn matrix_cell_style(self) -> Self {
        STYLE_TRANSITION_LUT.matrix_cell[(self as usize) & 0x07]
    }
}

/// Delimiter type for matrices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum MatrixDelim {
    #[default]
    None = 0,
    Parentheses = 1,
    Brackets = 2,
    Braces = 3,
    VBar = 4,
    DoubleVBar = 5,
    LeftBrace = 6,
}

/// Data payload for big operators (boxed to guarantee MathNode fits within 32 bytes).
#[derive(Debug, Clone, PartialEq)]
pub struct BigOpData<'a> {
    pub op: &'a str,
    pub sub: Option<Box<MathNode<'a>>>,
    pub sup: Option<Box<MathNode<'a>>>,
    pub limits: bool,
}

/// Data payload for dynamic delimiters (boxed to guarantee MathNode fits within 32 bytes).
#[derive(Debug, Clone, PartialEq)]
pub struct DelimitedData<'a> {
    pub open: &'a str,
    pub close: &'a str,
    pub content: MathNode<'a>,
}

/// Column alignment for matrices and alignment grids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ColAlign {
    #[default]
    Center,
    Left,
    Right,
}

/// Data payload for matrix grids (boxed to guarantee MathNode fits within 32 bytes).
#[derive(Debug, Clone, PartialEq)]
pub struct MatrixData<'a> {
    pub rows: Vec<Vec<MathNode<'a>>>,
    pub delim: MatrixDelim,
    pub col_alignments: Vec<ColAlign>,
}

/// Abstract Syntax Tree node for mathematical expressions.
///
/// Exactly 32 bytes in size to allow precisely two nodes to occupy a single
/// 64-byte L1 CPU cache line with 100% packing density and zero line splits.
#[derive(Debug, Clone, PartialEq)]
pub enum MathNode<'a> {
    /// Mathematical symbol (Greek, operator, relation, differential).
    Symbol {
        kind: MathAtomKind,
        face: Face,
        glyph: &'a str,
        code: Option<u8>,
    },
    /// Numerical constant (rendered in Roman font).
    Number(&'a str),
    /// Variable letter (rendered in math italic font, e.g. $x$, $y$).
    Variable(char),
    /// Subscript and/or superscript attached to a nucleus expression.
    SubSup {
        nucleus: Box<MathNode<'a>>,
        sub: Option<Box<MathNode<'a>>>,
        sup: Option<Box<MathNode<'a>>>,
    },
    /// Vertical fraction or generalized fraction with optional rule ($\frac{A}{B}$, $\binom{n}{k}$).
    Fraction {
        num: Box<MathNode<'a>>,
        den: Box<MathNode<'a>>,
        has_rule: bool,
    },
    /// Radical with optional index root ($\sqrt{A}$ or $\sqrt[n]{A}$).
    Radical {
        degree: Option<Box<MathNode<'a>>>,
        radicand: Box<MathNode<'a>>,
    },
    /// Big operator with optional limits ($\sum_{i=0}^n$, $\int_a^b$, $\lim_{x \to 0}$).
    BigOp(Box<BigOpData<'a>>),
    /// Dynamically stretched delimiters ($\left( A \right)$).
    Delimited(Box<DelimitedData<'a>>),
    /// Matrix or grid table ($A_{ij}$).
    Matrix(Box<MatrixData<'a>>),
    /// Vector or tensor accent ($\vec{v}$, $\hat{r}$, $\dot{x}$, $\ddot{x}$).
    Accent {
        accent: &'a str,
        nucleus: Box<MathNode<'a>>,
    },
    /// Explicit horizontal spacing in points ($\,$, $\;$, $\quad$).
    Space(f32),
    /// Font style switch ($\mathrm{...}$, $\mathbf{...}$).
    StyleChange {
        face: Face,
        content: Box<MathNode<'a>>,
    },
    /// Reduced Planck constant ($\hbar$).
    HBar,
    /// Horizontal sequence of math nodes.
    Row(Vec<MathNode<'a>>),
    /// Explicit mathematical style scope (`\displaystyle`, `\textstyle`, etc.).
    StyleScope {
        style: MathStyle,
        content: Box<MathNode<'a>>,
    },
    /// Vertically centered formula box on the math axis (TeX Appendix G Rule 12).
    Vcenter(Box<MathNode<'a>>),
}

#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(core::mem::size_of::<MathNode>() == 32);
    assert!(core::mem::size_of::<DelimitedData>() == 64);
};

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(core::mem::size_of::<MathNode>() == 16);
    assert!(core::mem::size_of::<DelimitedData>() == 32);
};

const _: () = assert!(core::mem::size_of::<MatrixDelim>() == 1);
const _: () = assert!(core::mem::size_of::<ColAlign>() == 1);
const _: () = assert!(core::mem::size_of::<MathStyle>() == 1);

impl<'a> MathNode<'a> {
    /// Returns the trailing `MathAtomKind` for inter-element spacing calculations,
    /// skipping trailing explicit spaces in rows.
    pub fn atom_kind(&self) -> MathAtomKind {
        match self {
            MathNode::Symbol { kind, .. } => *kind,
            MathNode::Number(_) => MathAtomKind::Ord,
            MathNode::Variable(_) => MathAtomKind::Ord,
            MathNode::SubSup { nucleus, .. } => nucleus.atom_kind(),
            MathNode::Fraction { .. } => MathAtomKind::Inner,
            MathNode::Radical { .. } => MathAtomKind::Inner,
            MathNode::BigOp(_) => MathAtomKind::Op,
            MathNode::Delimited(_) => MathAtomKind::Inner,
            MathNode::Matrix(_) => MathAtomKind::Inner,
            MathNode::Accent { nucleus, .. } => nucleus.atom_kind(),
            MathNode::Space(_) => MathAtomKind::Ord,
            MathNode::StyleChange { content, .. } => content.atom_kind(),
            MathNode::HBar => MathAtomKind::Ord,
            MathNode::Row(nodes) => {
                for n in nodes.iter().rev() {
                    if !matches!(n, MathNode::Space(_)) {
                        return n.atom_kind();
                    }
                }
                MathAtomKind::Ord
            }
            MathNode::StyleScope { content, .. } => content.atom_kind(),
            MathNode::Vcenter(_) => MathAtomKind::Ord,
        }
    }

    /// Returns the leading `MathAtomKind` of this node, skipping leading explicit spaces if any.
    pub fn first_atom_kind(&self) -> MathAtomKind {
        match self {
            MathNode::Row(nodes) => {
                for n in nodes {
                    if !matches!(n, MathNode::Space(_)) {
                        return n.first_atom_kind();
                    }
                }
                MathAtomKind::Ord
            }
            MathNode::StyleScope { content, .. } => content.first_atom_kind(),
            MathNode::Vcenter(_) => MathAtomKind::Ord,
            _ => self.atom_kind(),
        }
    }
}
