//! TeX Appendix G inter-atom mathematical spacing matrix.
//!
//! Provides a 64-byte lookup table fitting entirely within a single L1 CPU cache line,
//! supporting branchless computation of typographic spacing between math atoms.

use crate::backend::fonts::Face;
use crate::math::math_box::MathBox;

/// Eight basic mathematical atom classifications defined by Donald Knuth in The TeXbook.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum MathAtomKind {
    /// Ordinary symbol, variable, or digit ($x$, $2$, $\alpha$).
    #[default]
    Ord = 0,
    /// Large operator ($\sum$, $\prod$, $\int$, $\lim$).
    Op = 1,
    /// Binary operator ($+$, $-$, $\times$, $\cdot$).
    Bin = 2,
    /// Relational operator ($=$, $<$, $\le$, $\approx$, $\in$).
    Rel = 3,
    /// Opening bracket or delimiter ($($ , $[$, $\{$, $|$).
    Open = 4,
    /// Closing bracket or delimiter ($)$ , $]$, $\}$, $|$).
    Close = 5,
    /// Punctuation mark (comma, semicolon).
    Punct = 6,
    /// Inner sub-formula, fraction, or radical.
    Inner = 7,
}

/// 64-byte branchless L1-resident lookup table encoding TeX atom spacing codes:
/// - `0`: No space (0 mu)
/// - `1`: Thin space (3 mu = 3/18 em)
/// - `2`: Medium space (4 mu = 4/18 em)
/// - `3`: Thick space (5 mu = 5/18 em)
pub static MATH_SPACING_TABLE: [u8; 64] = [
    // Ord (0) -> Ord, Op, Bin, Rel, Open, Close, Punct, Inner
    0, 1, 2, 3, 0, 0, 0, 1, // Op (1)  -> Ord, Op, Bin, Rel, Open, Close, Punct, Inner
    1, 1, 0, 3, 0, 0, 0, 1, // Bin (2) -> Ord, Op, Bin, Rel, Open, Close, Punct, Inner
    2, 2, 0, 0, 2, 0, 0, 2, // Rel (3) -> Ord, Op, Bin, Rel, Open, Close, Punct, Inner
    3, 3, 0, 0, 3, 0, 0, 3, // Open (4)-> Ord, Op, Bin, Rel, Open, Close, Punct, Inner
    0, 0, 0, 0, 0, 0, 0, 0, // Close (5)->Ord, Op, Bin, Rel, Open, Close, Punct, Inner
    0, 1, 2, 3, 0, 0, 0, 1, // Punct (6)->Ord, Op, Bin, Rel, Open, Close, Punct, Inner
    1, 1, 0, 1, 1, 1, 1, 1, // Inner (7)->Ord, Op, Bin, Rel, Open, Close, Punct, Inner
    1, 1, 2, 3, 1, 0, 1, 1,
];

const _: () = assert!(core::mem::size_of_val(&MATH_SPACING_TABLE) == 64);

/// Precomputed branchless L1-resident mathematical spacing lookup table in em units.
///
/// Precomputed for all 3 mathematical style contexts (Display, Text, Script/ScriptScript)
/// across all 64 atom pairs.
/// - 0: Display style (with display glue expansion: 5 mu Bin, 6.5 mu Rel)
/// - 1: Text style (4 mu Bin, 5 mu Rel)
/// - 2: Script / ScriptScript style (Bin and Rel spacing suppressed to 0 mu)
#[repr(C, align(64))]
pub struct SpacingLut(pub [[f32; 64]; 3]);

#[allow(clippy::excessive_precision)]
pub static ATOM_SPACING_LUT: SpacingLut = SpacingLut([
    // Display style
    [
        0.00000000, 0.16666667, 0.27777778, 0.36111111, 0.00000000, 0.00000000, 0.00000000,
        0.16666667, 0.16666667, 0.16666667, 0.00000000, 0.36111111, 0.00000000, 0.00000000,
        0.00000000, 0.16666667, 0.27777778, 0.27777778, 0.00000000, 0.00000000, 0.27777778,
        0.00000000, 0.00000000, 0.27777778, 0.36111111, 0.36111111, 0.00000000, 0.00000000,
        0.36111111, 0.00000000, 0.00000000, 0.36111111, 0.00000000, 0.00000000, 0.00000000,
        0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.16666667,
        0.27777778, 0.36111111, 0.00000000, 0.00000000, 0.00000000, 0.16666667, 0.22222222,
        0.22222222, 0.00000000, 0.22222222, 0.22222222, 0.22222222, 0.22222222, 0.22222222,
        0.16666667, 0.16666667, 0.27777778, 0.36111111, 0.16666667, 0.00000000, 0.16666667,
        0.16666667,
    ],
    // Text style
    [
        0.00000000, 0.16666667, 0.22222222, 0.27777778, 0.00000000, 0.00000000, 0.00000000,
        0.16666667, 0.16666667, 0.16666667, 0.00000000, 0.27777778, 0.00000000, 0.00000000,
        0.00000000, 0.16666667, 0.22222222, 0.22222222, 0.00000000, 0.00000000, 0.22222222,
        0.00000000, 0.00000000, 0.22222222, 0.27777778, 0.27777778, 0.00000000, 0.00000000,
        0.27777778, 0.00000000, 0.00000000, 0.27777778, 0.00000000, 0.00000000, 0.00000000,
        0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.16666667,
        0.22222222, 0.27777778, 0.00000000, 0.00000000, 0.00000000, 0.16666667, 0.22222222,
        0.22222222, 0.00000000, 0.22222222, 0.22222222, 0.22222222, 0.22222222, 0.22222222,
        0.16666667, 0.16666667, 0.22222222, 0.27777778, 0.16666667, 0.00000000, 0.16666667,
        0.16666667,
    ],
    // Script style
    [
        0.00000000, 0.16666667, 0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.00000000,
        0.16666667, 0.16666667, 0.16666667, 0.00000000, 0.00000000, 0.00000000, 0.00000000,
        0.00000000, 0.16666667, 0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.00000000,
        0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.00000000,
        0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.00000000,
        0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.16666667,
        0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.00000000, 0.16666667, 0.22222222,
        0.22222222, 0.00000000, 0.22222222, 0.22222222, 0.22222222, 0.22222222, 0.22222222,
        0.16666667, 0.16666667, 0.00000000, 0.00000000, 0.16666667, 0.00000000, 0.16666667,
        0.16666667,
    ],
]);

const _: () = assert!(core::mem::size_of::<SpacingLut>() == 768);
const _: () = assert!(core::mem::align_of::<SpacingLut>() == 64);

/// Returns the typographic spacing between two adjacent atoms in mathematical units (`mu`),
/// incorporating TeX display-style natural glue expansion.
///
/// $1\,\text{em} = 18\,\text{mu}$. The returned float is the spacing in fractional `em` units
/// ($1.0 = \text{font\_size}$).
#[inline(always)]
pub fn atom_spacing(
    prev: MathAtomKind,
    curr: MathAtomKind,
    is_display: bool,
    is_display_or_text: bool,
) -> f32 {
    let mode = if is_display {
        0
    } else if is_display_or_text {
        1
    } else {
        2
    };
    let idx = ((prev as usize & 7) << 3) | (curr as usize & 7);
    ATOM_SPACING_LUT.0[mode][idx]
}

/// Standard canonical inter-atom spacing without display glue expansion.
#[inline(always)]
pub fn inter_atom_spacing(prev: MathAtomKind, curr: MathAtomKind, is_display_or_text: bool) -> f32 {
    atom_spacing(prev, curr, false, is_display_or_text)
}

/// Normalizes a sequence of raw math atom kinds into their context-effective classifications
/// according to TeX Appendix G Rule 5.
///
/// Any `Bin` atom is converted to `Ord` if:
/// 1. It is the first atom in the list or preceded by an invalid left operand (not Ord/Op/Close/Inner).
/// 2. It is the last atom in the list or followed by an invalid right operand (not Ord/Op/Open/Inner).
#[inline(always)]
pub fn normalize_atom_kinds(kinds: &mut [MathAtomKind]) {
    let len = kinds.len();
    if len == 0 {
        return;
    }

    // Step 1: Forward pass — check left operand validity via AtomBoundaryClassifier
    for i in 0..len {
        if kinds[i] == MathAtomKind::Bin {
            let left_ok = i > 0 && AtomBoundaryClassifier::is_valid_left(kinds[i - 1]);
            if !left_ok {
                kinds[i] = MathAtomKind::Ord;
            }
        }
    }

    // Step 2: Backward pass — check right operand validity via AtomBoundaryClassifier
    for i in (0..len).rev() {
        if kinds[i] == MathAtomKind::Bin {
            let right_ok = i + 1 < len && AtomBoundaryClassifier::is_valid_right(kinds[i + 1]);
            if !right_ok {
                kinds[i] = MathAtomKind::Ord;
            }
        }
    }
}

/// Limits placement preference for mathematical operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum AtomLimits {
    /// Follows default behavior for the operator.
    #[default]
    Default = 0,
    /// Explicit `\nolimits` (scripts attached at side).
    NoLimits = 1,
    /// Explicit `\limits` (scripts attached above and below).
    Limits = 2,
    /// Explicit `\displaylimits` (limits in display style, side scripts otherwise).
    DisplayLimits = 3,
}

/// Bit-packed atom classification, limits preference, and typographic flags.
///
/// - Bits 0..2 (3 bits): MathAtomKind (0..7: Ord, Op, Bin, Rel, Open, Close, Punct, Inner)
/// - Bits 3..4 (2 bits): AtomLimits preference (0 = Default, 1 = NoLimits, 2 = Limits, 3 = DisplayLimits)
/// - Bit 5 (1 bit): Has accent flag
/// - Bit 6 (1 bit): Converted boundary atom flag
/// - Bit 7 (1 bit): Reserved (0)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct PackedAtomMetadata(pub u8);

impl PackedAtomMetadata {
    /// Creates a packed atom metadata with default limits and no flags.
    #[inline(always)]
    pub const fn new(kind: MathAtomKind) -> Self {
        Self((kind as u8) & 0x07)
    }

    /// Creates packed atom metadata with explicit limits preference.
    #[inline(always)]
    pub const fn with_limits(self, limits: AtomLimits) -> Self {
        Self((self.0 & !0x18) | (((limits as u8) & 0x03) << 3))
    }

    /// Extracts the mathematical atom kind.
    #[inline(always)]
    pub const fn kind(self) -> MathAtomKind {
        match self.0 & 0x07 {
            0 => MathAtomKind::Ord,
            1 => MathAtomKind::Op,
            2 => MathAtomKind::Bin,
            3 => MathAtomKind::Rel,
            4 => MathAtomKind::Open,
            5 => MathAtomKind::Close,
            6 => MathAtomKind::Punct,
            _ => MathAtomKind::Inner,
        }
    }

    /// Extracts the operator limits preference.
    #[inline(always)]
    pub const fn limits(self) -> AtomLimits {
        match (self.0 >> 3) & 0x03 {
            1 => AtomLimits::NoLimits,
            2 => AtomLimits::Limits,
            3 => AtomLimits::DisplayLimits,
            _ => AtomLimits::Default,
        }
    }

    /// Whether this atom has a mathematical accent attached.
    #[inline(always)]
    pub const fn has_accent(self) -> bool {
        (self.0 & 0x20) != 0
    }

    /// Sets or clears the accent flag.
    #[inline(always)]
    pub fn set_accent(&mut self, has_accent: bool) {
        if has_accent {
            self.0 |= 0x20;
        } else {
            self.0 &= !0x20;
        }
    }

    /// Whether this atom was converted from a binary operator at a boundary.
    #[inline(always)]
    pub const fn is_boundary_converted(self) -> bool {
        (self.0 & 0x40) != 0
    }

    /// Sets or clears the boundary-converted flag.
    #[inline(always)]
    pub fn set_boundary_converted(&mut self, converted: bool) {
        if converted {
            self.0 |= 0x40;
        } else {
            self.0 &= !0x40;
        }
    }
}

/// Branchless classifier for math atom boundary transitions (TeX Appendix G Rule 5).
pub struct AtomBoundaryClassifier;

impl AtomBoundaryClassifier {
    /// Valid left operands for a binary operator: Ord (0), Op (1), Close (5), Inner (7).
    /// Binary mask: (1 << 0) | (1 << 1) | (1 << 5) | (1 << 7) = 0xA3.
    pub const VALID_LEFT_MASK: u8 = (1 << (MathAtomKind::Ord as u8))
        | (1 << (MathAtomKind::Op as u8))
        | (1 << (MathAtomKind::Close as u8))
        | (1 << (MathAtomKind::Inner as u8));

    /// Valid right operands for a binary operator: Ord (0), Op (1), Open (4), Inner (7).
    /// Binary mask: (1 << 0) | (1 << 1) | (1 << 4) | (1 << 7) = 0x93.
    pub const VALID_RIGHT_MASK: u8 = (1 << (MathAtomKind::Ord as u8))
        | (1 << (MathAtomKind::Op as u8))
        | (1 << (MathAtomKind::Open as u8))
        | (1 << (MathAtomKind::Inner as u8));

    /// Branchless check if an atom kind is a valid left operand for a binary operator.
    #[inline(always)]
    pub const fn is_valid_left(kind: MathAtomKind) -> bool {
        ((1 << (kind as u8)) & Self::VALID_LEFT_MASK) != 0
    }

    /// Branchless check if an atom kind is a valid right operand for a binary operator.
    #[inline(always)]
    pub const fn is_valid_right(kind: MathAtomKind) -> bool {
        ((1 << (kind as u8)) & Self::VALID_RIGHT_MASK) != 0
    }

    /// Classifies an atom kind given its adjacent neighbors.
    /// Converts `Bin` to `Ord` if either neighbor is missing or invalid.
    #[inline(always)]
    pub fn classify(
        kind: MathAtomKind,
        prev: Option<MathAtomKind>,
        next: Option<MathAtomKind>,
    ) -> MathAtomKind {
        if kind != MathAtomKind::Bin {
            return kind;
        }
        let left_ok = prev.is_some_and(Self::is_valid_left);
        let right_ok = next.is_some_and(Self::is_valid_right);
        if left_ok && right_ok {
            MathAtomKind::Bin
        } else {
            MathAtomKind::Ord
        }
    }
}

/// Non-owning geometric view of a math atom during layout passes.
///
/// Avoids intermediate MathBox clones or heap allocations.
/// Exactly 16 bytes: 4 bytes w, 4 bytes h, 4 bytes d, 1 byte kind, 1 byte flags, 2 bytes padding.
/// Exactly 4 views pack into a single 64-byte L1 cache line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct TransientAtomView {
    pub w: f32,
    pub h: f32,
    pub d: f32,
    pub kind: MathAtomKind,
    pub flags: u8,
    pub _pad: [u8; 2],
}

impl TransientAtomView {
    #[inline(always)]
    pub const fn new(w: f32, h: f32, d: f32, kind: MathAtomKind) -> Self {
        Self {
            w,
            h,
            d,
            kind,
            flags: 0,
            _pad: [0; 2],
        }
    }

    #[inline(always)]
    pub const fn from_box(b: MathBox, kind: MathAtomKind) -> Self {
        Self {
            w: b.w,
            h: b.h,
            d: b.d,
            kind,
            flags: 0,
            _pad: [0; 2],
        }
    }

    #[inline(always)]
    pub const fn to_box(self) -> MathBox {
        MathBox::with_dims(self.w, self.h, self.d)
    }
}

/// Structure of Arrays (SoA) layout for horizontal math atom sequences.
///
/// Stores widths, heights, depths, atom kinds, and inter-atom kerns in separate contiguous vectors,
/// maximizing cacheline density and enabling SIMD-friendly vectorized summation of widths.
#[derive(Clone, Debug, Default)]
pub struct MathHorizontalListSoA {
    /// Widths of each atom in points. Contiguous f32 array for SIMD auto-vectorization.
    pub widths: Vec<f32>,
    /// Heights of each atom.
    pub heights: Vec<f32>,
    /// Depths of each atom.
    pub depths: Vec<f32>,
    /// Atom classifications packed tightly (1 byte per atom).
    pub kinds: Vec<MathAtomKind>,
    /// Inter-atom kerns/spacings inserted between atom `i` and `i+1`.
    pub kerns: Vec<f32>,
}

impl MathHorizontalListSoA {
    /// Creates a new SoA list with preallocated capacity.
    #[inline(always)]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            widths: Vec::with_capacity(cap),
            heights: Vec::with_capacity(cap),
            depths: Vec::with_capacity(cap),
            kinds: Vec::with_capacity(cap),
            kerns: Vec::with_capacity(cap),
        }
    }

    /// Appends an atom to the SoA collection.
    #[inline(always)]
    pub fn push(&mut self, w: f32, h: f32, d: f32, kind: MathAtomKind) {
        self.widths.push(w);
        self.heights.push(h);
        self.depths.push(d);
        self.kinds.push(kind);
    }

    /// Number of atoms in the sequence.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    /// Whether the sequence is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    /// Clears all buffers while retaining allocated memory.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.widths.clear();
        self.heights.clear();
        self.depths.clear();
        self.kinds.clear();
        self.kerns.clear();
    }

    /// Vectorized total width calculation: sums all atom widths and inter-atom kerns.
    #[inline(always)]
    pub fn total_width(&self) -> f32 {
        let w_sum: f32 = self.widths.iter().sum();
        let k_sum: f32 = self.kerns.iter().sum();
        w_sum + k_sum
    }

    /// Computes max ink height across all atoms.
    #[inline(always)]
    pub fn max_height(&self) -> f32 {
        self.heights.iter().copied().fold(0.0f32, f32::max)
    }

    /// Computes max ink depth across all atoms.
    #[inline(always)]
    pub fn max_depth(&self) -> f32 {
        self.depths.iter().copied().fold(0.0f32, f32::max)
    }
}

/// 64-byte aligned static lookup table for common mathematical kerning and italic corrections
/// in 1000-units-per-em. Eliminates runtime match expressions and branching.
#[repr(C, align(64))]
pub struct KernPairLut {
    /// Italic correction in 1000-units-per-em for ASCII lowercase 'a'..='z' (indices 0..25)
    pub lower_ic: [u8; 26],
    /// Italic correction in 1000-units-per-em for ASCII uppercase 'A'..='Z' (indices 0..25)
    pub upper_ic: [u8; 26],
    /// Additional pair kern offsets for high-frequency math collisions
    pub pair_kerns: [i8; 12],
}

pub static KERN_PAIR_LUT: KernPairLut = KernPairLut {
    lower_ic: [
        20,  // 'a'
        35,  // 'b'
        20,  // 'c'
        40,  // 'd'
        20,  // 'e'
        120, // 'f'
        20,  // 'g'
        35,  // 'h'
        20,  // 'i'
        50,  // 'j'
        35,  // 'k'
        40,  // 'l'
        20,  // 'm'
        20,  // 'n'
        20,  // 'o'
        50,  // 'p'
        20,  // 'q'
        20,  // 'r'
        20,  // 's'
        40,  // 't'
        20,  // 'u'
        35,  // 'v'
        35,  // 'w'
        30,  // 'x'
        50,  // 'y'
        30,  // 'z'
    ],
    upper_ic: [
        40, // 'A'
        50, // 'B'
        70, // 'C'
        50, // 'D'
        60, // 'E'
        80, // 'F'
        60, // 'G'
        60, // 'H'
        60, // 'I'
        70, // 'J'
        60, // 'K'
        40, // 'L'
        70, // 'M'
        70, // 'N'
        60, // 'O'
        80, // 'P'
        50, // 'Q'
        50, // 'R'
        60, // 'S'
        70, // 'T'
        60, // 'U'
        90, // 'V'
        80, // 'W'
        70, // 'X'
        90, // 'Y'
        70, // 'Z'
    ],
    pair_kerns: [
        40,  // 0: 'f' followed by ')' or ']'
        -30, // 1: 'f' followed by ';' or ','
        -50, // 2: 'P' followed by subscript
        -60, // 3: 'T' followed by subscript
        -60, // 4: 'V' followed by subscript
        -60, // 5: 'Y' followed by subscript
        -50, // 6: 'W' followed by subscript
        -30, // 7: 'A' followed by superscript
        20,  // 8: '(' followed by 'j'
        10,  // 9: '(' followed by 'y'
        30,  // 10: 'd' followed by 'x' (calculus dx)
        0,   // 11: reserved
    ],
};

const _: () = assert!(core::mem::size_of::<KernPairLut>() == 64);
const _: () = assert!(core::mem::align_of::<KernPairLut>() == 64);

impl KernPairLut {
    /// Returns the italic correction in 1000-units-per-em for an ASCII character.
    #[inline(always)]
    pub fn italic_correction(&self, ch: char) -> u16 {
        if ch.is_ascii_lowercase() {
            let idx = (ch as u8 - b'a') as usize;
            self.lower_ic[idx] as u16
        } else if ch.is_ascii_uppercase() {
            let idx = (ch as u8 - b'A') as usize;
            self.upper_ic[idx] as u16
        } else {
            0
        }
    }

    /// Fast lookup for pair kerning between adjacent characters in 1000-units-per-em.
    #[inline(always)]
    pub fn pair_kern(&self, prev: char, curr: char) -> i32 {
        match (prev, curr) {
            ('f', ')' | ']') => self.pair_kerns[0] as i32,
            ('f', ';' | ',') => self.pair_kerns[1] as i32,
            ('P', '_') => self.pair_kerns[2] as i32,
            ('T', '_') => self.pair_kerns[3] as i32,
            ('V', '_') => self.pair_kerns[4] as i32,
            ('Y', '_') => self.pair_kerns[5] as i32,
            ('W', '_') => self.pair_kerns[6] as i32,
            ('A', '^') => self.pair_kerns[7] as i32,
            ('(', 'j') => self.pair_kerns[8] as i32,
            ('(', 'y') => self.pair_kerns[9] as i32,
            ('d', 'x') => self.pair_kerns[10] as i32,
            _ => 0,
        }
    }
}

/// Precomputed 128-byte lookup table mapping ASCII bytes (0..128) directly to
/// packed atom classification and default math face in a single L1 memory read.
#[repr(C, align(64))]
pub struct AsciiMathFastPath {
    pub entries: [u8; 128],
}

const fn build_ascii_fast_path() -> AsciiMathFastPath {
    let mut entries = [0u8; 128];
    let mut b = 0u8;
    while b < 128 {
        let (kind, face, valid) = match b {
            b'a'..=b'z' | b'A'..=b'Z' => (0u8, 1u8, true), // Ord, Italic
            b'0'..=b'9' => (0, 0, true),                   // Ord, Roman
            b'+' | b'-' | b'*' => (2, 0, true),            // Bin, Roman
            b'=' | b'<' | b'>' | b':' => (3, 0, true),     // Rel, Roman
            b'(' | b'[' | b'{' => (4, 0, true),            // Open, Roman
            b')' | b']' | b'}' => (5, 0, true),            // Close, Roman
            b',' | b';' => (6, 0, true),                   // Punct, Roman
            b'.' | b'/' => (0, 0, true),                   // Ord, Roman
            _ => (0, 0, false),
        };
        if valid {
            entries[b as usize] = (kind & 0x07) | ((face & 0x07) << 3) | 0x40;
        }
        b += 1;
    }
    AsciiMathFastPath { entries }
}

pub static ASCII_MATH_FAST_PATH: AsciiMathFastPath = build_ascii_fast_path();

const _: () = assert!(core::mem::size_of::<AsciiMathFastPath>() == 128);
const _: () = assert!(core::mem::align_of::<AsciiMathFastPath>() == 64);

impl AsciiMathFastPath {
    /// Resolves the atom kind, font face, and validity for an ASCII character.
    #[inline(always)]
    pub fn lookup(&self, byte: u8) -> (MathAtomKind, Face, bool) {
        if byte < 128 {
            let entry = self.entries[byte as usize];
            let is_valid = (entry & 0x40) != 0;
            let kind = match entry & 0x07 {
                0 => MathAtomKind::Ord,
                1 => MathAtomKind::Op,
                2 => MathAtomKind::Bin,
                3 => MathAtomKind::Rel,
                4 => MathAtomKind::Open,
                5 => MathAtomKind::Close,
                6 => MathAtomKind::Punct,
                _ => MathAtomKind::Inner,
            };
            let face = match (entry >> 3) & 0x07 {
                1 => Face::Italic,
                2 => Face::Bold,
                3 => Face::BoldItalic,
                4 => Face::Symbol,
                _ => Face::Roman,
            };
            (kind, face, is_valid)
        } else {
            (MathAtomKind::Ord, Face::Roman, false)
        }
    }
}
