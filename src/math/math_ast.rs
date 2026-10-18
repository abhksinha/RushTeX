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

impl MathStyle {
    /// Canonical superscript style transition (TeX Appendix G Rule 18).
    #[inline(always)]
    pub fn sup(self) -> MathStyle {
        STYLE_TRANSITION_LUT.sup[self as usize]
    }

    /// Canonical subscript style transition (TeX Appendix G Rule 18).
    #[inline(always)]
    pub fn sub(self) -> MathStyle {
        STYLE_TRANSITION_LUT.sub[self as usize]
    }
}

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
    /// `OP_LIMITS_DISPLAY`, `OP_LIMITS_LIMITS` or `OP_LIMITS_NOLIMITS`.
    pub limits: u8,
}

/// Delimiter option bitfield constants (Batch 1: Item 55).
pub const DELIMITER_OPT_NONE: u8 = 0;
pub const DELIMITER_OPT_AXIS: u8 = 1 << 0;
pub const DELIMITER_OPT_NOAXIS: u8 = 1 << 1;
pub const DELIMITER_OPT_EXACT: u8 = 1 << 2;

/// Fraction option bitfield constants (Batch 1: Item 52).
pub const FRACTION_OPT_NONE: u8 = 0;
pub const FRACTION_OPT_EXACT: u8 = 1 << 0;
pub const FRACTION_OPT_NORULE: u8 = 1 << 1;
pub const FRACTION_OPT_NOAXIS: u8 = 1 << 2;
pub const FRACTION_OPT_SKEWED: u8 = 1 << 3;
/// The fraction carries its own delimiters (`\atopwithdelims`, `\binom`, `\genfrac`), which
/// replace the `\nulldelimiterspace` TeX otherwise puts on both sides (§748).
pub const FRACTION_OPT_DELIMITED: u8 = 1 << 4;

/// Data payload for dynamic delimiters (boxed to guarantee MathNode fits within 32 bytes).
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct DelimitedData<'a> {
    pub open: &'a str,
    pub close: &'a str,
    pub content: Box<MathNode<'a>>,
    pub height: f32,
    pub depth: f32,
    pub options: u8,
    pub class: u8,
    #[cfg(target_pointer_width = "64")]
    pub _padding: [u8; 14],
    #[cfg(target_pointer_width = "32")]
    pub _padding: [u8; 2],
}

impl<'a> DelimitedData<'a> {
    /// Creates a new DelimitedData with default zero-options and default sizing.
    #[inline(always)]
    pub fn new(open: &'a str, close: &'a str, content: MathNode<'a>) -> Self {
        Self {
            open,
            close,
            content: Box::new(content),
            height: 0.0,
            depth: 0.0,
            options: 0,
            class: 0,
            #[cfg(target_pointer_width = "64")]
            _padding: [0; 14],
            #[cfg(target_pointer_width = "32")]
            _padding: [0; 2],
        }
    }
}

/// Data payload for 4-branch \mathchoice (boxed to guarantee MathNode fits within 32 bytes).
#[derive(Debug, Clone, PartialEq)]
pub struct MathChoiceData<'a> {
    pub display: MathNode<'a>,
    pub text: MathNode<'a>,
    pub script: MathNode<'a>,
    pub script_script: MathNode<'a>,
}

/// Semantic environment kind for mathematical grids and multiline equations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum MatrixEnvKind {
    #[default]
    Matrix = 0,
    Aligned = 1,
    Cases = 2,
    Gather = 3,
    Multline = 4,
    SmallMatrix = 5,
    /// amsmath `subarray` / `\substack`: script-style rows set as tight as limits.
    Subarray = 6,
    /// LaTeX `array`: like `matrix`, plus `\arraycolsep` outside the first and last columns.
    Array = 7,
    /// amsmath `alignedat` / `alignat`: `aligned` with no space between column pairs.
    AlignedAt = 8,
    /// plain `\ooalign`: rows overprinted on one baseline in a `\vtop`, in the current style.
    Overlay = 9,
    /// fontmath `\@vereq` (`\cong`): rows stacked with `\lineskip -.5pt`, lowered .5pt.
    Vereq = 10,
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
    pub env_kind: MatrixEnvKind,
    pub col_alignments: Vec<ColAlign>,
}

/// Four canonical mathematical ellipsis kinds defined in Donald Knuth's The TeXbook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EllipsisKind {
    /// Baseline low dots: `\ldots` or low `\dots` ($x_1, \dots, x_n$).
    Low = 0,
    /// Axis-centered dots: `\cdots` or binary `\dots` ($x_1 + \dots + x_n$).
    Centered = 1,
    /// Vertical dots: `\vdots` (matrix columns).
    Vertical = 2,
    /// Diagonal dots: `\ddots` (matrix diagonals).
    Diagonal = 3,
}

const _: () = assert!(core::mem::size_of::<EllipsisKind>() == 1);

/// Data payload for horizontal extensible arrows (\xrightarrow[sub]{sup}, \xleftarrow[sub]{sup}).
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensibleArrowData<'a> {
    pub arrow: &'a str,
    pub sub: Option<MathNode<'a>>,
    pub sup: Option<MathNode<'a>>,
}

/// A horizontal math skip in the unit TeX measures it in: `TextEm` follows the text font
/// (`\quad`, control space), `Mu` follows the current style's math quad (`\,`, `\mkern`),
/// and `Pt` is absolute (`\kern3pt`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MathSkip {
    TextEm(f32),
    Mu(f32),
    Pt(f32),
    /// The interword space (fontdimen 2) of the text font at the current style's size, as
    /// between the words of `\text{...}`.
    TextSpace,
}

/// Abstract Syntax Tree node for mathematical expressions.
///
/// Exactly 32 bytes in size to allow precisely two nodes to occupy a single
/// 64-byte L1 CPU cache line with 100% packing density and zero line splits.
#[derive(Debug, Clone, PartialEq)]
pub enum MathNode<'a> {
    /// Mathematical ellipsis (\ldots, \cdots, \vdots, \ddots).
    Ellipsis(EllipsisKind),
    /// Mathematical symbol (Greek, operator, relation, differential).
    Symbol {
        kind: MathAtomKind,
        face: Face,
        glyph: &'a str,
        code: Option<u8>,
        /// TeX math code (`class << 12 | family << 8 | char`) from LaTeX's symbol
        /// declarations; 0 when the symbol has none. Used by Computer Modern layout.
        mathcode: u16,
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
        options: u8,
        /// Rule thickness in pt from `\above` or `\genfrac`; negative takes the font's
        /// default rule thickness.
        thickness: f32,
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
    /// Explicit horizontal spacing (`\,`, `\quad`, `\mkern`, `\kern`).
    Space(MathSkip),
    /// Font style switch ($\mathrm{...}$, $\mathbf{...}$).
    StyleChange {
        face: Face,
        content: Box<MathNode<'a>>,
    },
    /// Reduced Planck constant ($\hbar$).
    HBar,
    /// Conditional math glue primitive (\nonscript) suppressing following glue in script styles (mlist.c:4557).
    Nonscript,
    /// Horizontal sequence of math nodes.
    Row(Vec<MathNode<'a>>),
    /// Explicit mathematical style scope (`\displaystyle`, `\textstyle`, etc.).
    StyleScope {
        style: MathStyle,
        content: Box<MathNode<'a>>,
    },
    /// Vertically centered formula box on the math axis (TeX Appendix G Rule 12).
    Vcenter(Box<MathNode<'a>>),
    /// 4-branch mathematical choice for style-dependent layouts (`\mathchoice{D}{T}{S}{SS}`).
    MathChoice(Box<MathChoiceData<'a>>),
    /// A LaTeX composite built from TeX glyphs (first, used with Computer Modern) and the
    /// equivalent single Base-14 glyph (second, used with other font families).
    FamilyChoice(Box<(MathNode<'a>, MathNode<'a>)>),
    /// Middle delimiter within a dynamic fence (`\middle |`).
    MiddleDelim { delim: &'a str, options: u8 },
    /// Manually sized delimiter (`\big`, `\Big`, `\bigg`, `\Bigg` and directional variants).
    SizedDelim {
        delim: &'a str,
        size_tier: u8,
        kind: MathAtomKind,
        is_left: bool,
    },
    /// Extensible horizontal overbrace (`\overbrace{content}`).
    Overbrace(Box<MathNode<'a>>),
    /// Extensible horizontal underbrace (`\underbrace{content}`).
    Underbrace(Box<MathNode<'a>>),
    /// Extensible horizontal overbracket (`\overbracket{content}`).
    Overbracket(Box<MathNode<'a>>),
    /// Extensible horizontal underbracket (`\underbracket{content}`).
    Underbracket(Box<MathNode<'a>>),
    /// Horizontal extensible arrow with optional limits above/below (`\xrightarrow[sub]{sup}`).
    ExtensibleArrow(Box<ExtensibleArrowData<'a>>),
    /// Explicit atom class coercion (`\mathord`, `\mathbin`, `\mathrel`, etc.).
    Coerce {
        kind: MathAtomKind,
        content: Box<MathNode<'a>>,
    },
    /// xcolor colour scope (`\color`, `\textcolor`); transparent for atom spacing.
    Colored {
        color: crate::frontend::ast::PackedColor,
        content: Box<MathNode<'a>>,
    },
    /// Display equation with optional custom tag and alignment side (TeX \eqno / \leqno, LaTeX \tag).
    Tagged {
        formula: Box<MathNode<'a>>,
        tag: Box<MathNode<'a>>,
        is_left: bool,
        starred: bool,
    },
    /// Standard TeX strut ensuring baseline height and depth.
    Strut,
    /// Transparent phantom spacing node (`\phantom`, `\hphantom`, `\vphantom`).
    Phantom {
        content: Box<MathNode<'a>>,
        width: bool,
        height_depth: bool,
    },
    /// Smashed or zero-dimension box (`\smash`, `\rlap`, `\llap`).
    DimensionBox {
        content: Box<MathNode<'a>>,
        zero_width: bool,
        shift_left: bool,
        zero_height_depth: bool,
    },
    /// An Op atom with an arbitrary nucleus (`\mathop{...}`), carrying its `\limits`
    /// setting (`OP_LIMITS_*`) for Rule 13.
    OpLimits {
        content: Box<MathNode<'a>>,
        mode: u8,
    },
    /// `\boxed{...}`: the content in display style inside an `\fbox`.
    Boxed(Box<MathNode<'a>>),
    /// amsmath `\sideset{pre}{post}\op`: (the operator, the pre-scripts on an empty box as
    /// tall as it, the operator with `\nolimits` post-scripts).
    SideSet(Box<(MathNode<'a>, MathNode<'a>, MathNode<'a>)>),
}

/// Glyph strings of math-letter runs joined into a ligature word (`\mathrm{ff}`). They are
/// recognised by address, so a `\text{ff}` word from the source is never mistaken for one.
pub static MATH_LIGATURE_WORDS: [&str; 5] = ["ffi", "ffl", "ff", "fi", "fl"];

/// Whether `glyph` is one of [`MATH_LIGATURE_WORDS`] (same address, not just same text).
#[inline]
pub fn is_math_ligature_word(glyph: &str) -> bool {
    MATH_LIGATURE_WORDS.iter().any(|w| core::ptr::eq(w.as_ptr(), glyph.as_ptr()) && w.len() == glyph.len())
}

/// `\displaylimits` (the default): limits above and below only in display style.
pub const OP_LIMITS_DISPLAY: u8 = 0;
/// `\limits`: limits above and below in every style.
pub const OP_LIMITS_LIMITS: u8 = 1;
/// `\nolimits`: scripts beside the nucleus.
pub const OP_LIMITS_NOLIMITS: u8 = 2;
/// Mask for the three modes above.
pub const OP_LIMITS_MODE_MASK: u8 = 3;
/// The nucleus is a box (e.g. amsmath's `\mathop{\kern0pt #1}`), never a single character, so
/// Rule 13 does not centre it on the axis.
pub const OP_LIMITS_BOXED_NUCLEUS: u8 = 1 << 2;

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
const _: () = assert!(core::mem::size_of::<MatrixEnvKind>() == 1);
const _: () = assert!(core::mem::size_of::<ColAlign>() == 1);
const _: () = assert!(core::mem::size_of::<MathStyle>() == 1);
const _: () = assert!(core::mem::size_of::<MatrixData>() <= 64);

impl<'a> MathNode<'a> {
    /// Returns the trailing `MathAtomKind` for inter-element spacing calculations,
    /// skipping trailing explicit spaces in rows.
    pub fn atom_kind(&self) -> MathAtomKind {
        match self {
            MathNode::Ellipsis(kind) => match kind {
                EllipsisKind::Low | EllipsisKind::Centered | EllipsisKind::Diagonal => MathAtomKind::Inner,
                EllipsisKind::Vertical => MathAtomKind::Ord,
            },
            MathNode::Symbol { kind, .. } => *kind,
            MathNode::Number(_) => MathAtomKind::Ord,
            MathNode::Variable(_) => MathAtomKind::Ord,
            MathNode::SubSup { nucleus, .. } => nucleus.atom_kind(),
            MathNode::Fraction { .. } => MathAtomKind::Inner,
            MathNode::Radical { .. } => MathAtomKind::Ord,
            MathNode::BigOp(_) => MathAtomKind::Op,
            MathNode::Delimited(_) => MathAtomKind::Inner,
            MathNode::Matrix(_) => MathAtomKind::Inner,
            MathNode::Accent { nucleus, .. } => nucleus.atom_kind(),
            MathNode::Space(_) => MathAtomKind::Ord,
            MathNode::StyleChange { content, .. } => content.atom_kind(),
            MathNode::HBar => MathAtomKind::Ord,
            MathNode::Nonscript => MathAtomKind::Ord,
            MathNode::Row(nodes) => {
                for n in nodes.iter().rev() {
                    if !matches!(n, MathNode::Space(_) | MathNode::Nonscript) {
                        return n.atom_kind();
                    }
                }
                MathAtomKind::Ord
            }
            MathNode::StyleScope { content, .. } => content.atom_kind(),
            MathNode::Vcenter(_) => MathAtomKind::Ord,
            MathNode::MathChoice(choice) => choice.display.atom_kind(),
            MathNode::FamilyChoice(choice) => choice.0.atom_kind(),
            // e-TeX: `\middle` closes the preceding sublist and opens the next one.
            MathNode::MiddleDelim { .. } => MathAtomKind::Open,
            MathNode::SizedDelim { kind, .. } => *kind,
            MathNode::Overbrace(_) | MathNode::Underbrace(_)
            | MathNode::Overbracket(_) | MathNode::Underbracket(_) => MathAtomKind::Op,
            MathNode::ExtensibleArrow(_) => MathAtomKind::Rel,
            MathNode::Coerce { kind, .. } => *kind,
            MathNode::Colored { content, .. } => content.atom_kind(),
            MathNode::Tagged { formula, .. } => formula.atom_kind(),
            MathNode::Strut => MathAtomKind::Ord,
            // `\phantom`, `\smash` and the laps build boxes, which are Ord atoms.
            MathNode::Phantom { .. } | MathNode::DimensionBox { .. } => MathAtomKind::Ord,
            MathNode::OpLimits { .. } => MathAtomKind::Op,
            MathNode::Boxed(_) => MathAtomKind::Ord,
            MathNode::SideSet(_) => MathAtomKind::Op,
        }
    }

    /// Returns the leading `MathAtomKind` of this node, skipping leading explicit spaces if any.
    pub fn first_atom_kind(&self) -> MathAtomKind {
        match self {
            MathNode::Row(nodes) => {
                for n in nodes {
                    if !matches!(n, MathNode::Space(_) | MathNode::Nonscript) {
                        return n.first_atom_kind();
                    }
                }
                MathAtomKind::Ord
            }
            MathNode::StyleScope { content, .. } => content.first_atom_kind(),
            MathNode::Vcenter(_) => MathAtomKind::Ord,
            MathNode::MathChoice(choice) => choice.display.first_atom_kind(),
            MathNode::FamilyChoice(choice) => choice.0.first_atom_kind(),
            MathNode::MiddleDelim { .. } => MathAtomKind::Close,
            MathNode::Overbrace(_) | MathNode::Underbrace(_)
            | MathNode::Overbracket(_) | MathNode::Underbracket(_) => MathAtomKind::Op,
            MathNode::ExtensibleArrow(_) => MathAtomKind::Rel,
            MathNode::Coerce { kind, .. } => *kind,
            MathNode::Colored { content, .. } => content.first_atom_kind(),
            MathNode::Tagged { formula, .. } => formula.first_atom_kind(),
            MathNode::Strut => MathAtomKind::Ord,
            MathNode::Phantom { .. } | MathNode::DimensionBox { .. } => MathAtomKind::Ord,
            _ => self.atom_kind(),
        }
    }

    /// Returns the trailing `MathAtomKind` respecting the active `MathStyle` (especially for `MathChoice`).
    pub fn atom_kind_in_style(&self, style: MathStyle) -> MathAtomKind {
        match self {
            MathNode::MathChoice(choice) => match style.tier() {
                0 => choice.display.atom_kind_in_style(style),
                1 => choice.text.atom_kind_in_style(style),
                2 => choice.script.atom_kind_in_style(style),
                _ => choice.script_script.atom_kind_in_style(style),
            },
            MathNode::SubSup { nucleus, .. } => nucleus.atom_kind_in_style(style),
            MathNode::Coerce { kind, .. } => *kind,
            MathNode::Colored { content, .. } => content.atom_kind_in_style(style),
            MathNode::Tagged { formula, .. } => formula.atom_kind_in_style(style),
            MathNode::Phantom { .. } | MathNode::DimensionBox { .. } => MathAtomKind::Ord,
            MathNode::Row(nodes) => {
                for n in nodes.iter().rev() {
                    if !matches!(n, MathNode::Space(_) | MathNode::Nonscript) {
                        return n.atom_kind_in_style(style);
                    }
                }
                MathAtomKind::Ord
            }
            _ => self.atom_kind(),
        }
    }
}
