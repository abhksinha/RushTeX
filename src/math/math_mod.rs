//! # Mathematical Typesetting Subsystem (`math`)
//!
//! Provides parsing, TeX Appendix G mathematical layout, calculus fractions,
//! radicals, limits, matrices, extensible delimiters, and physics notation.
//!
//! Designed for zero heap thrashing, L1 CPU cache residency, and branchless
//! atom spacing tables.

pub mod math_assembly;
pub mod math_ast;
pub mod math_bigop;
pub mod math_box;
pub mod math_delim;
pub mod math_diff;
pub mod math_fonts;
pub mod math_fraction;
pub mod math_layout;
pub mod math_matrix;
pub mod math_parser;
pub mod math_radical;
pub mod math_scripts;
pub mod math_spacing;
pub mod math_tfm;

pub use math_assembly::{
    ComprehensiveMathVerifier, FormulaBoundingBoxUnion, MathGeometricTraceBuffer,
    MathGeometricTraceRecord, MathPaintEmitter,
};
pub use math_ast::{
    ColAlign, CrampedStyleGovernor, FontVariantTier, MathContextFlags, MathNode, MathStyle,
    MatrixDelim, StyleFrameStack, StyleHierarchy, StyleScaleLut, StyleTransitionLut,
    STYLE_SCALE_LUT, STYLE_TRANSITION_LUT,
};
pub use math_bigop::{
    BigOpVariantTier, IntegralOverhangCorrector, LimitClearanceGovernor, OperatorCenteringEngine,
    OperatorLimitSwitch, PackedBigOpMetrics, BIG_OP_VARIANT_TIER,
};
pub use math_box::MathBox;
pub use math_delim::{
    DelimiterPairMatch, DelimiterSizingGovernor, DelimiterVariantResolver,
    ExtensibleAssemblyEngine, NullDelimiterEliminator, DELIMITER_VARIANT_RESOLVER,
};
pub use math_diff::{
    compare_geometry, trace_math_geometry, GeometryDiff, GeometryTolerance, GlyphPlacement,
    MathGeometryTrace, RulePlacement,
};
pub use math_fonts::{lookup_math_command, measure_char, measure_symbol_byte, MathSymbolDef};
pub use math_fraction::{
    BinomialDelimiterCache, ContinuedFractionPipeline, FractionClearanceGovernor,
    GeneralizedFractionEngine, NumeratorDenominatorAligner, PackedFractionMetrics,
    BINOMIAL_DELIMITER_CACHE,
};
pub use math_layout::{layout_node, measure_math, render_math, MathLayoutContext};
pub use math_matrix::{
    MatrixColumnSoA, MatrixDelimBinder, RowBaselineAligner, SparseGridCompressor,
    VcenterGeometryEngine,
};
pub use math_parser::{parse_math_str, parse_math_str as parse_math, MathParser};
pub use math_radical::{
    ExtensibleRadicalAssembler, PackedRadicalMetrics, RadicalClearanceGovernor,
    RadicalExcessDistributor, RadicalVariantSelector, RootDegreePositioner,
    RADICAL_VARIANT_SELECTOR,
};
pub use math_scripts::{
    KernBeforeScriptLut, PackedScriptMetrics, ScriptClearanceGovernor, ScriptNestingLimiter,
    SubSupShiftOptimizer, TransientScriptPair, KERN_BEFORE_SCRIPT_LUT,
};
pub use math_spacing::{
    atom_spacing, inter_atom_spacing, normalize_atom_kinds, AsciiMathFastPath,
    AtomBoundaryClassifier, AtomLimits, KernPairLut, MathAtomKind, MathHorizontalListSoA,
    PackedAtomMetadata, SpacingLut, TransientAtomView, ASCII_MATH_FAST_PATH, ATOM_SPACING_LUT,
    KERN_PAIR_LUT, MATH_SPACING_TABLE,
};
pub use math_tfm::{cmex10_metric, cmmi10_metric, cmsy10_metric, DelimiterRecipe, GlyphMetric};

#[cfg(test)]
mod math_tests;
