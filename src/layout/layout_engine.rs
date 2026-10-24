//! # High-Speed Page Layout Engine (`layout`)
//!
//! Provides a zero-allocation, greedy line-breaking, table-formatting, and page-packing engine.
//!
//! ## Architectural Principles
//!
//! - **24-Byte `Copy` Display List**: The [`Paint`] enum is strictly flat without heap allocations
//!   (no inner `Vec`), fitting multiple draw commands in a single 64-byte L1 CPU cache line.
//! - **Contiguous Page Text Pool**: All text slice tokens for a page reside in [`Page::text_pool`],
//!   referenced by compact `(part_start, part_count)` indices.
//! - **Scratch Buffer Recycling**: `Layouter` allocates scratch buffers (`scratch_runs`, `scratch_words`,
//!   `scratch_line`) once upon instantiation and clears them across paragraphs, eliminating heap churn.
//! - **Zero-Allocation Inline Measurement**: Walks AST nodes in-place to calculate line lengths directly.
//! - **Dynamic Document Geometry**: Margins, dimensions, leading, and font sizes are dynamically
//!   configured via [`DocumentConfig`].
//!
//! ## Public APIs
//!
//! - [`layout`]: Typesets a parsed [`Document`] AST into a sequence of [`Page`] structures ready for PDF emission.
//! - [`Page`]: Represents a single rendered page with width, height, draw operations, and text pool.
//! - [`Paint`]: Display list operation (`GlyphRun`, `Marker`, `Rule`, `CaptionTag`).
//! - [`Marker`]: Enumerated or bullet list marker.

pub use crate::backend::scaled::Sp;
use crate::doc_config::DocumentConfig;
use crate::frontend::TrivlistKind;
use crate::font_metrics::{self as metrics, Face};
use crate::index::{pre_scan_entries, IndexRegistry};
use crate::layout::layout_hz::{
    build_breakpoints, hash_paragraph, hz_break_paragraph_breakpoints_pass,
    hz_break_paragraph_fast_shape, ragged_break_paragraph, try_layout_single_line_paragraph, Breakpoint, HzConfig, ParCacheEntry, PAR_CACHE_MAX_LINES,
};
use crate::frontend::ast::{DisplayPar, PackedColor};
use crate::tex_parser::{Block, Document, Graphic, Inline, TableData};

pub const PAR_CACHE_CAP: usize = 256;
pub const PAR_CACHE_MASK: usize = PAR_CACHE_CAP - 1;

pub const PAGE_W: f32 = 597.508;
pub const PAGE_H: f32 = 845.047;
pub const MARGIN: f32 = 72.27;
pub const FONT_SIZE: f32 = 10.0;
pub const LEADING: f32 = 12.0;
/// Text of a tie (`~`): unbreakable interword glue at space factor 1000.
pub const TIE_TEXT: &str = "\u{00A0}";
/// Text of a control space (`\ `): interword glue at space factor 1000.
pub const CONTROL_SPACE_TEXT: &str = "\u{E000}";
/// `\textunderscore` (latex.ltx): `\kern.06em\vbox{\hrule\@width.3em}`, a word with no
/// glyph that the painter draws as a 0.4pt rule on the baseline.
pub const UNDERSCORE_TEXT: &str = "\u{E001}";
/// `\lineskip` (plain and LaTeX: 1pt), with `\lineskiplimit` 0pt.
pub const LINESKIP_PT: f32 = 1.0;

/// Flag bit in [`Paint::GlyphRun`]'s `part_count`: the run is body text, so the backend applies
/// the font's ligature/kern program to each word ([`crate::backend::shape`]), exactly as layout
/// measured it. Runs without the flag (math, listings, generated labels) are emitted verbatim.
pub const GLYPH_RUN_SHAPED: u32 = 1 << 31;
/// Mask extracting the number of text-pool parts from [`Paint::GlyphRun`]'s `part_count`.
pub const GLYPH_RUN_COUNT_MASK: u32 = GLYPH_RUN_SHAPED - 1;
pub const PARA_SKIP: f32 = 8.0;
/// LaTeX `\topsep` at 10pt (8pt plus 2pt minus 4pt), the space around trivlists.
pub const TOPSEP: f32 = 8.0;
/// LaTeX `\partopsep` at 10pt (2pt plus 1pt minus 1pt), added when a trivlist starts in
/// vertical mode.
pub const PARTOPSEP: f32 = 2.0;
pub const LIST_INDENT: f32 = 24.0;
pub const CELL_PAD: f32 = 6.0;
pub const FIG_H: f32 = 140.0;

/// The size of an `\includegraphics` box from its `width`, `height` and `scale` options;
/// the natural size is graphicx's 1in square for a file it cannot read. Lengths may be
/// multiples of `\textwidth`, `\linewidth`, `\columnwidth` or `\hsize` (all `hsize`).
fn graphic_box_size(options: &str, hsize: f32) -> (f32, f32) {
    fn dim(v: &str, hsize: f32) -> Option<f32> {
        let v = v.trim();
        for name in ["\\textwidth", "\\linewidth", "\\columnwidth", "\\hsize"] {
            if let Some(f) = v.strip_suffix(name) {
                let f = f.trim();
                return Some(if f.is_empty() { 1.0 } else { f.parse::<f32>().ok()? } * hsize);
            }
        }
        crate::doc_config::parse_dimension_pt(v)
    }
    const NATURAL: f32 = 72.27;
    let (mut w, mut h, mut scale) = (None, None, 1.0f32);
    for kv in options.split(',') {
        let Some((k, v)) = kv.split_once('=') else { continue };
        match k.trim() {
            "width" => w = dim(v, hsize),
            "height" | "totalheight" => h = dim(v, hsize),
            "scale" => scale = v.trim().parse().unwrap_or(1.0),
            _ => {}
        }
    }
    let (w, h) = match (w, h) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, w),
        (None, Some(h)) => (h, h),
        (None, None) => (NATURAL * scale, NATURAL * scale),
    };
    (w.max(0.0), h.max(0.0))
}

/// The marks of latex.ltx's `\markboth`/`\markright` as the output routine sees them.
#[derive(Default)]
pub(crate) struct PageMarks<'a> {
    /// The marks in force when the page began (TeX's `\topmark`).
    top_left: Option<Vec<Inline<'a>>>,
    top_right: Option<Vec<Inline<'a>>>,
    /// The first right mark and the last marks put on the page being built.
    first_right: Option<Vec<Inline<'a>>>,
    last_left: Option<Vec<Inline<'a>>>,
    last_right: Option<Vec<Inline<'a>>>,
}

impl<'a> PageMarks<'a> {
    fn insert(&mut self, mark: &crate::frontend::ast::MarkData<'a>) {
        if let Some(left) = &mark.left {
            self.last_left = Some(left.clone());
        }
        if self.first_right.is_none() {
            self.first_right = Some(mark.right.clone());
        }
        self.last_right = Some(mark.right.clone());
    }

    /// `\rightmark` of the finished page (`\mark_use_first:nn{page}{2e-right}`: the first
    /// right mark on it, else the one in force at its top); the page's last marks become
    /// the next page's top marks.
    fn finish_page(&mut self) -> Option<Vec<Inline<'a>>> {
        let right = self.first_right.take().or_else(|| self.top_right.clone());
        if let Some(r) = self.last_right.take() {
            self.top_right = Some(r);
        }
        if let Some(l) = self.last_left.take() {
            self.top_left = Some(l);
        }
        right
    }
}

#[derive(Clone, Debug)]
pub struct Page<'a> {
    pub width: f32,
    pub height: f32,
    pub font_family: metrics::FontFamily,
    pub used_faces: u64,
    pub ops: Vec<Paint>,
    pub text_pool: Vec<&'a str>,
}

impl<'a> Page<'a> {
    /// Appends a paint op. The PDF writer derives the fonts to embed from the ops (a text op
    /// in a design-size master needs only that master); `used_faces` holds only faces used
    /// outside the ops.
    #[inline(always)]
    pub fn push_op(&mut self, op: Paint) {
        self.ops.push(op);
    }

    #[inline(always)]
    pub fn compute_used_faces(&self) -> u64 {
        let mut mask = self.used_faces;
        for op in &self.ops {
            match op {
                Paint::GlyphRun { face, .. }
                | Paint::Marker { face, .. }
                | Paint::PageNumber { face, .. }
                | Paint::SectionNumber { face, .. }
                | Paint::FigureTag { face, .. }
                | Paint::TableTag { face, .. }
                | Paint::EquationNumber { face, .. } => {
                    mask |= 1 << (*face as u8);
                }
                Paint::CaptionTag { .. } | Paint::TableCaptionTag { .. } => {
                    mask |= 1 << (Face::Roman as u8);
                }
                Paint::Rule { .. } | Paint::VectorGlyph { .. } | Paint::CidGlyphRun { .. } | Paint::Color { .. } => {}
            }
        }
        mask
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Marker {
    Bullet,
    Dash,
    Asterisk,
    MiddleDot,
    Number(u32),
    ParenNumber(u32),
    Alpha(u32),
    AlphaDot(u32),
    Roman(u32),
    UpperRoman(u32),
    UpperAlpha(u32),
}

impl Marker {
    /// The TS1 slot of a symbol marker: `\textbullet`, `\textasteriskcentered`,
    /// `\textperiodcentered`.
    #[inline]
    pub fn ts1_slot(self) -> u8 {
        match self {
            Marker::Asterisk => 42,
            Marker::MiddleDot => 183,
            _ => 136,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Paint {
    GlyphRun {
        x: Sp,
        y: Sp,
        face: Face,
        size: f32,
        part_start: u32,
        part_count: u32,
        word_spacing: f32,
    },
    Marker {
        x: Sp,
        y: Sp,
        face: Face,
        size: f32,
        marker: Marker,
    },
    /// Axis-aligned filled or stroked rectangle. `y` is the bottom edge.
    Rule {
        x: Sp,
        y: Sp,
        w: Sp,
        h: Sp,
        fill: bool,
    },
    CaptionTag {
        x: Sp,
        y: Sp,
        size: f32,
        n: u32,
    },
    TableCaptionTag {
        x: Sp,
        y: Sp,
        size: f32,
        n: u32,
    },
    PageNumber {
        x: Sp,
        y: Sp,
        face: Face,
        size: f32,
        page_number: u32,
    },
    SectionNumber {
        x: Sp,
        y: Sp,
        face: Face,
        size: f32,
        number: [u16; 4],
        level: u8,
    },
    FigureTag {
        x: Sp,
        y: Sp,
        face: Face,
        size: f32,
        n: u32,
    },
    TableTag {
        x: Sp,
        y: Sp,
        face: Face,
        size: f32,
        n: u32,
    },
    EquationNumber {
        x: Sp,
        y: Sp,
        face: Face,
        size: f32,
        n: u32,
    },
    /// Sets the fill and stroke colour for the ops that follow.
    Color {
        color: PackedColor,
    },
    /// Native resolution-independent OpenType vector glyph outline rendered via Bézier paths.
    VectorGlyph {
        x: Sp,
        y: Sp,
        size: f32,
        glyph_id: u16,
    },
    /// OpenType dynamic CID glyph emission rendered via composite CIDFont text operators.
    CidGlyphRun {
        x: Sp,
        y: Sp,
        size: f32,
        glyph_id: u16,
    },
}

pub fn layout<'a>(document: &Document<'a>) -> Vec<Page<'a>> {
    let has_indices = document.blocks.iter().any(|b| {
        matches!(
            b,
            Block::TableOfContents | Block::ListOfFigures | Block::ListOfTables
        )
    });

    let registry = if has_indices {
        let mut dry_registry = IndexRegistry::new();
        dry_registry.tocdepth = document.preamble.config.toc_depth.max(0) as u8;
        pre_scan_entries(&document.blocks, &mut dry_registry);

        let mut dry_layouter = Layouter::new_dry_run(&document.preamble.config);
        dry_layouter.registry = dry_registry;
        dry_layouter.layout_blocks(&document.blocks, 0);
        dry_layouter.registry
    } else {
        IndexRegistry::new()
    };

    let mut layouter = Layouter::new(&document.preamble.config);
    layouter.registry = registry;
    layouter.layout_blocks(&document.blocks, 0);
    layouter.finish()
}

/// A first pass for `\pageref` (LaTeX's run that writes the `.aux` file): a dry layout that
/// returns the page of each `\label` whatsit by its number (0 when it was not laid out).
pub fn label_pages(document: &Document<'_>) -> Vec<u32> {
    let mut registry = IndexRegistry::new();
    registry.tocdepth = document.preamble.config.toc_depth.max(0) as u8;
    pre_scan_entries(&document.blocks, &mut registry);
    let mut dry = Layouter::new_dry_run(&document.preamble.config);
    dry.registry = registry;
    dry.layout_blocks(&document.blocks, 0);
    std::mem::take(&mut dry.registry.label_pages)
}

pub(crate) struct Layouter<'a> {
    pub(crate) pages: Vec<Page<'a>>,
    pub(crate) current_page: Page<'a>,
    /// The current baseline position on the page, exact in scaled points.
    pub(crate) y: Sp,
    pub(crate) left: f32,
    pub(crate) right: f32,
    /// The text block's left and right edges (`\textwidth`), which lists and quotes do
    /// not move; the footer is centred in it.
    pub(crate) text_span: (f32, f32),
    pub(crate) page_width: f32,
    pub(crate) page_height: f32,
    pub(crate) margin_top: f32,
    pub(crate) margin_bottom: f32,
    /// TeX `\topskip`: the first baseline of a page sits this far below the text-block top.
    pub(crate) top_skip: f32,
    /// LaTeX `\footskip`: text-block bottom to page-number baseline.
    pub(crate) foot_skip: f32,
    pub(crate) base_font_size: f32,
    pub(crate) base_leading: f32,
    /// A user `\@makecol`'s rules and kerns beside the column.
    column_wrap: Option<Box<crate::frontend::ast::ColumnWrap>>,
    /// The display skips in force (`FontSize::display_skips`).
    display_skips: [f32; 7],
    pub(crate) font_family: metrics::FontFamily,
    pub(crate) figure_counter: u32,
    pub(crate) table_counter: u32,
    pub(crate) equation_counter: u32,
    pub(crate) page_style: crate::frontend::ast::PageStyle,
    /// `\thispagestyle`: overrides `page_style` for the page being built.
    pub(crate) this_page_style: Option<crate::frontend::ast::PageStyle>,
    pub(crate) head_sep: f32,
    /// The font paragraphs start in (`\itshape` inside a `plain` theorem).
    pub(crate) body_face: Face,
    pub(crate) marks: PageMarks<'a>,
    pub(crate) par_indent: f32,
    pub(crate) par_skip: f32,
    /// `\parskip`'s stretch: LaTeX's default is 0pt plus 1pt; `\setlength` gives none.
    pub(crate) par_skip_stretch: f32,
    pub(crate) par_skip_shrink: f32,
    pub(crate) hang_indent: f32,
    pub(crate) hang_after: i32,
    pub(crate) left_skip: f32,
    pub(crate) right_skip: f32,
    pub(crate) list_left_margin: f32,
    pub(crate) list_label_sep: f32,
    pub(crate) list_item_indent: f32,
    pub(crate) list_item_sep: f32,
    /// `\parskip` inside a list item: the list's `\parsep` (`\list` sets it); `None` outside
    /// lists, where it is `par_skip`.
    item_par_skip: Option<(f32, f32, f32)>,
    /// The next list starts an item before any of its text (`\item\begin{enumerate}`):
    /// latex.ltx `\@inlabel`, whose `\@noparlist` drops the list's `\@topsep` and
    /// `\@endparenv`, so its first label shares the outer label's line.
    in_label: bool,
    pub(crate) list_par_indent: f32,
    pub(crate) french_spacing: bool,
    /// `\sfcode`\.` is 1000 (`thebibliography`).
    pub(crate) period_sf_1000: bool,
    pub(crate) after_heading: bool,
    pub(crate) has_preceding_text: bool,
    pub(crate) last_line_width: f32,
    pub(crate) last_line_indent: f32,
    pub(crate) prev_graf: i32,
    pub(crate) par_shape: Vec<(f32, f32)>,
    pub(crate) pre_tolerance: i32,
    pub(crate) tolerance: i32,
    pub(crate) emergency_stretch: f32,
    pub(crate) looseness: i32,
    pub(crate) par_fill_width: f32,
    pub(crate) par_fill_stretch: f32,
    pub(crate) last_line_fit: i32,
    /// Alignment of the paragraphs being set (`\centering`, `\raggedright`, …).
    pub(crate) par_align: crate::frontend::ast::ParAlign,
    /// The `\addvspace` amount that ended the previous block (0 after any other block).
    pub(crate) last_addvspace: f32,
    /// Text colour in force between paragraphs (`\color` outside any group).
    pub(crate) text_color: PackedColor,
    /// The class's normal size (10pt, 11pt, 12pt), which `\small`, `\Large`, … key off.
    pub(crate) class_font_size: f32,
    /// Size in force at the end of the paragraph just exploded; it sets the paragraph's
    /// `\baselineskip` and, at the top level, the size after it.
    pub(crate) par_end_size: Option<f32>,
    /// `\baselineskip` for the next paragraph when its last `\par` falls inside a group
    /// that changed the size.
    pub(crate) leading_override: Option<f32>,
    /// TeX's `\prevdepth`: depth of the last line or display on the current page, for the
    /// interline glue before the next one.
    pub(crate) prev_depth: f32,
    /// `\prevdepth` is TeX's ignore-depth value (after a rule): the next box gets no
    /// interline glue.
    pub(crate) ignore_depth: bool,
    /// Depth of the last line of a box being set apart (`capture_float`), when it has one.
    pub(crate) box_depth: f32,
    /// Height of a strut the next box holds (0 for none).
    pub(crate) strut_height: f32,
    /// LaTeX's `\@enumdepth` and `\@itemdepth`: the label style of a list follows the
    /// nesting of its own kind.
    enum_depth: u8,
    item_depth: u8,
    /// Baseline advance after the last text line, while the block just set ended in one
    /// (0 otherwise): a paragraph with another `\baselineskip` corrects its first line.
    pub(crate) last_line_advance: f32,
    /// `last_line_advance` as it stood when the current block began.
    pub(crate) block_entry_advance: f32,
    pub(crate) words_soa: ParagraphSoA<'a>,
    pub(crate) breakpoints: Vec<Breakpoint>,
    pub(crate) hyphen_scratch: Vec<&'a str>,
    /// The hz breaker's hyphenation cache (512 KB), allocated when that breaker first runs.
    pub(crate) hyphen_cache: Option<Box<[crate::layout::layout_hyphen::HyphenCacheEntry; crate::layout::layout_hyphen::HYPHEN_CACHE_SIZE]>>,
    pub(crate) par_cache: Box<[ParCacheEntry; PAR_CACHE_CAP]>,
    pub(crate) dp_nodes: Vec<crate::layout::layout_hz::DpNode>,
    /// Buffers of TeX's line breaker, reused for every paragraph.
    pub(crate) break_scratch: crate::layout::layout_linebreak::BreakScratch,
    /// Paragraphs broken by earlier compiles of a [`crate::Session`]; `None` outside one,
    /// so a one-off compile does not hash its paragraphs.
    pub(crate) break_cache: Option<crate::layout::layout_linebreak::BreakCache>,
    /// Break paragraphs with the hz (font expansion and protrusion) breaker instead of
    /// TeX's; pdfTeX does this only under `microtype`.
    pub(crate) hz_microtype: bool,
    pub(crate) dp_prefix: Vec<crate::layout::layout_hz::PrefixSum>,
    pub(crate) pass_stats: [u32; 4],
    pub(crate) debug_timings: bool,
    pub(crate) time_explode_ns: u64,
    pub(crate) time_pass1_ns: u64,
    pub(crate) time_hyph_ns: u64,
    pub(crate) time_pass2_ns: u64,
    pub(crate) time_flush_ns: u64,
    pub(crate) line_descriptors: Vec<LineDescriptor>,
    pub(crate) cell_words_soa: ParagraphSoA<'a>,
    /// Scratch words for measuring or painting one line of inlines (titles, labels,
    /// captions) without disturbing `cell_words_soa`.
    scratch_soa: ParagraphSoA<'a>,
    pub(crate) cell_lines: Vec<CellLineRange>,
    /// Line descriptors of the cell paragraph being broken (reused).
    pub(crate) cell_descs: Vec<LineDescriptor>,
    pub(crate) cell_line_spans: Vec<CellSpan>,
    pub(crate) registry: IndexRegistry<'a>,
    pub(crate) is_dry_run: bool,
    pub(crate) current_sec_idx: usize,
    pub(crate) current_fig_idx: usize,
    pub(crate) current_tbl_idx: usize,
    pub(crate) section_counters: [u16; 4],
    pub(crate) streaming_mode: bool,
    pub(crate) pending_page: Option<Page<'a>>,
    pub(crate) completed_pages: std::collections::VecDeque<Page<'a>>,
    pub(crate) completed_pages_count: usize,
    pub(crate) recycled_pages: Vec<Page<'a>>,
    pub(crate) floats: crate::layout::layout_float::FloatState<'a>,
    pub(crate) page_builder: crate::layout::layout_pagebuilder::PageBuilder,
}

impl<'a> Layouter<'a> {
    pub(crate) fn new(config: &DocumentConfig) -> Self {
        Self::new_with_caps(config, 64, 600, false)
    }

    pub(crate) fn new_dry_run(config: &DocumentConfig) -> Self {
        Self::new_with_caps(config, 0, 0, true)
    }

    fn new_with_caps(
        config: &DocumentConfig,
        ops_cap: usize,
        pool_cap: usize,
        is_dry_run: bool,
    ) -> Self {
        let left = config.margin_left;
        let right = config.page_width - config.margin_right;
        let y = Sp(crate::config::paper_sp(config.page_height)) - config.margin_top - config.top_skip;
        Self {
            pages: Vec::with_capacity(32),
            current_page: Page {
                width: config.page_width,
                height: config.page_height,
                font_family: config.font_family,
                used_faces: 0,
                ops: if ops_cap > 0 {
                    Vec::with_capacity(ops_cap)
                } else {
                    Vec::new()
                },
                text_pool: if pool_cap > 0 {
                    Vec::with_capacity(pool_cap)
                } else {
                    Vec::new()
                },
            },
            y,
            left,
            right,
            text_span: (left, right),
            page_width: config.page_width,
            page_height: config.page_height,
            margin_top: config.margin_top,
            margin_bottom: config.margin_bottom,
            top_skip: config.top_skip,
            foot_skip: config.foot_skip,
            base_font_size: config.base_font_size,
            base_leading: config.base_leading,
            font_family: config.font_family,
            figure_counter: 0,
            table_counter: 0,
            equation_counter: 0,
            page_style: if config.page_numbering { config.page_style } else { crate::frontend::ast::PageStyle::Empty },
            this_page_style: None,
            head_sep: config.head_sep,
            body_face: Face::Roman,
            marks: PageMarks::default(),
            par_indent: config.par_indent,
            par_skip: config.par_skip,
            par_skip_stretch: config.par_skip_stretch,
            par_skip_shrink: config.par_skip_shrink,
            hang_indent: config.hang_indent,
            hang_after: config.hang_after,
            left_skip: config.left_skip,
            right_skip: config.right_skip,
            list_left_margin: config.list_left_margin,
            list_label_sep: config.list_label_sep,
            list_item_indent: config.list_item_indent,
            list_item_sep: config.list_item_sep,
            item_par_skip: None,
            in_label: false,
            list_par_indent: config.list_par_indent,
            french_spacing: config.french_spacing,
            period_sf_1000: false,
            after_heading: false,
            has_preceding_text: false,
            last_line_width: 0.0,
            last_line_indent: 0.0,
            prev_graf: 0,
            par_shape: Vec::with_capacity(8),
            pre_tolerance: config.pre_tolerance,
            tolerance: config.tolerance,
            emergency_stretch: config.emergency_stretch,
            looseness: config.looseness,
            par_fill_width: config.par_fill_width,
            par_fill_stretch: config.par_fill_stretch,
            last_line_fit: config.last_line_fit,
            par_align: crate::frontend::ast::ParAlign::Justified,
            last_addvspace: 0.0,
            text_color: PackedColor::BLACK,
            class_font_size: config.base_font_size,
            column_wrap: None,
            display_skips: crate::frontend::ast::FontSize::NormalSize
                .display_skips(config.base_font_size)
                .unwrap_or_default(),
            par_end_size: None,
            leading_override: None,
            prev_depth: 0.0,
            ignore_depth: false,
            box_depth: 0.0,
            strut_height: 0.0,
            enum_depth: 0,
            item_depth: 0,
            last_line_advance: 0.0,
            block_entry_advance: 0.0,
            words_soa: ParagraphSoA::with_capacity(1024),
            breakpoints: Vec::with_capacity(512),
            hyphen_scratch: Vec::with_capacity(16),
            hyphen_cache: None,
            par_cache: vec![ParCacheEntry::default(); PAR_CACHE_CAP]
                .into_boxed_slice()
                .try_into()
                .unwrap(),
            dp_nodes: Vec::with_capacity(1024 * 4),
            break_scratch: Default::default(),
            break_cache: None,
            hz_microtype: false,
            dp_prefix: Vec::with_capacity(1024),
            pass_stats: [0; 4],
            debug_timings: std::env::var_os("RUSHTEX_DEBUG_PASSES").is_some(),
            time_explode_ns: 0,
            time_pass1_ns: 0,
            time_hyph_ns: 0,
            time_pass2_ns: 0,
            time_flush_ns: 0,
            line_descriptors: Vec::with_capacity(64),
            cell_words_soa: ParagraphSoA::with_capacity(256),
            scratch_soa: ParagraphSoA::with_capacity(64),
            cell_lines: Vec::with_capacity(32),
            cell_descs: Vec::new(),
            cell_line_spans: Vec::with_capacity(32),
            registry: IndexRegistry::new(),
            is_dry_run,
            current_sec_idx: 0,
            current_fig_idx: 0,
            current_tbl_idx: 0,
            section_counters: [0; 4],
            streaming_mode: false,
            floats: crate::layout::layout_float::FloatState::new(),
            page_builder: crate::layout::layout_pagebuilder::PageBuilder::new(),
            pending_page: None,
            completed_pages: std::collections::VecDeque::with_capacity(2),
            completed_pages_count: 0,
            recycled_pages: Vec::with_capacity(4),
        }
    }

    /// Natural width of `inls` set on one line: the paragraph path's widths, kerns and glue
    /// (an hbox of them), so a measured box holds exactly what is later set in it.
    pub(crate) fn measure_inlines_face_size<'doc>(&mut self, inls: &'doc [Inline<'a>], face: Face, size: f32) -> f32 {
        self.scratch_soa.clear();
        let mut math = Vec::new();
        explode_inlines_into_family(inls, face, size, &mut self.scratch_soa, self.font_family, &mut math)
    }

    #[inline]
    fn marker_width(&self, marker: Marker, face: Face, size: f32) -> f32 {
        marker_width_family(marker, face, size, self.font_family)
    }

    pub(crate) fn enable_streaming(&mut self) {
        self.streaming_mode = true;
    }

    pub(crate) fn take_completed_page(&mut self) -> Option<Page<'a>> {
        self.completed_pages.pop_front()
    }

    #[inline(always)]
    pub(crate) fn current_page_number(&self) -> u32 {
        if self.streaming_mode {
            (self.completed_pages_count
                + if self.pending_page.is_some() { 1 } else { 0 }
                + 1) as u32
        } else {
            (self.pages.len() + 1) as u32
        }
    }

    pub(crate) fn add_page_number_to_page(
        page: &mut Page<'_>,
        page_number: u32,
        page_numbering: bool,
        font_family: metrics::FontFamily,
        foot_baseline: Sp,
        text_span: (f32, f32),
        page_num_size: f32,
    ) {
        if !page_numbering {
            return;
        }
        // `\thepage` is set in `\normalsize`.
        let num_width = page_number_width(page_number, font_family, page_num_size);
        // The plain/article footer is `\hfil\thepage\hfil` in a box of \textwidth.
        let x = text_span.0 + (text_span.1 - text_span.0 - num_width) * 0.5;
        let y = foot_baseline;
        page.push_op(Paint::PageNumber {
            x: x.into(),
            y,
            face: Face::Roman,
            size: page_num_size,
            page_number,
        });
    }

    /// latex.ltx `\@outputpage` for the page being finished: its head and foot in the page
    /// style (`\thispagestyle` first). The head is `\hb@xt@\textwidth{\@oddhead}` with its
    /// baseline `\headsep` above the text block; `\@oddhead` is article.cls's
    /// `{\slshape\rightmark}\hfil\thepage` under `headings` and `myheadings`.
    /// A user `\@makecol` boxes the column, footnotes and floats included, as
    /// `\hbox{<left>\vbox to\@colht{..}<right>}`: the column moves right by the left
    /// material, and each `\vrule` runs the column's height.
    fn make_column(&mut self) {
        let Some(wrap) = &self.column_wrap else { return };
        let shift: f32 = wrap.left.iter().map(|&(_, w)| w).sum();
        let shift = Sp::pt(shift);
        for op in &mut self.current_page.ops {
            op.shift_x(shift);
        }
        let (bottom, h) = (self.margin_bottom, self.text_top() - self.margin_bottom);
        let mut x = self.text_span.0;
        let (mut left, mut right) = (Vec::new(), Vec::new());
        for (i, &(rule, w)) in wrap.left.iter().chain(&wrap.right).enumerate() {
            if i == wrap.left.len() {
                x += self.text_span.1 - self.text_span.0;
            }
            if rule {
                let r = Paint::Rule { x: x.into(), y: bottom.into(), w: w.into(), h, fill: true };
                if i < wrap.left.len() { left.push(r) } else { right.push(r) }
            }
            x += w;
        }
        // hlist_out draws the box in order: the left material, the column, the right.
        self.current_page.ops.splice(0..0, left);
        for r in right {
            self.current_page.push_op(r);
        }
    }

    /// Puts each row's number right after that row's ops (`row_ends`, from the alignment),
    /// as each row's hbox holds it in TeX: the order decides how pdfTeX groups text into
    /// strings. `tags` are (row, start, end) op ranges, appended after the display.
    fn order_display_rows(&mut self, row_ends: &[usize], tags: &[(usize, usize, usize)]) {
        let Some(&(_, first_tag, _)) = tags.first() else { return };
        if row_ends.is_empty() || tags.iter().any(|&(r, ..)| r >= row_ends.len()) || *row_ends.last().unwrap() > first_tag {
            return;
        }
        let tail: Vec<Paint> = self.current_page.ops.drain(row_ends[0]..).collect();
        let base = row_ends[0];
        let mut from = base;
        for (r, &end) in row_ends.iter().enumerate() {
            self.current_page.ops.extend_from_slice(&tail[from - base..end - base]);
            from = end;
            for &(_, s, e) in tags.iter().filter(|&&(tr, ..)| tr == r) {
                self.current_page.ops.extend_from_slice(&tail[s - base..e - base]);
            }
        }
        self.current_page.ops.extend_from_slice(&tail[from - base..first_tag - base]);
    }

    /// A row's `\tag{..}` (in parentheses unless `\tag*`), ending at `right`.
    fn paint_row_tag(&mut self, tag: &crate::math::MathNode<'a>, starred: bool, right: f32, y: Sp) {
        let size = self.base_font_size;
        let family = self.font_family;
        let t_w = crate::math::measure_math(tag, size, family, crate::math::MathStyle::Text).w;
        let (open_w, close_w) = if starred {
            (0.0, 0.0)
        } else {
            (metrics::width_family('(', Face::Roman, size, family), metrics::width_family(')', Face::Roman, size, family))
        };
        let x = right - open_w - t_w - close_w;
        let paren = |s: &'static str, x: f32, page: &mut Page<'a>| {
            let start = page.text_pool.len() as u32;
            page.text_pool.push(s);
            page.push_op(Paint::GlyphRun { x: x.into(), y, face: Face::Roman, size, part_start: start, part_count: 1, word_spacing: 0.0 });
        };
        if !starred {
            paren("(", x, &mut self.current_page);
        }
        crate::math::render_math(
            tag,
            size,
            family,
            crate::math::MathStyle::Text,
            Sp::pt(x + open_w),
            y,
            &mut self.current_page.ops,
            &mut self.current_page.text_pool,
        );
        if !starred {
            paren(")", x + open_w + t_w, &mut self.current_page);
        }
    }

    pub(crate) fn decorate_page(&mut self) {
        use crate::frontend::ast::PageStyle;
        self.make_column();
        let style = self.this_page_style.take().unwrap_or(self.page_style);
        let right_mark = self.marks.finish_page();
        let page_number = self.current_page_number();
        match style {
            PageStyle::Empty => {}
            PageStyle::Plain => {
                let foot = self.foot_baseline();
                Self::add_page_number_to_page(&mut self.current_page, page_number, true, self.font_family, foot, self.text_span, self.base_font_size);
            }
            PageStyle::Headings | PageStyle::MyHeadings => {
                let head_start = self.current_page.ops.len();
                let y = self.text_top() + self.head_sep;
                if let Some(mark) = right_mark {
                    let face = if self.font_family == metrics::FontFamily::ComputerModern { Face::Slanted } else { Face::Italic };
                    self.paint_inlines_at_size(&mark, face, self.base_font_size, self.text_span.0, y);
                }
                let num_w = page_number_width(page_number, self.font_family, self.base_font_size);
                self.current_page.push_op(Paint::PageNumber {
                    x: (self.text_span.1 - num_w).into(),
                    y,
                    face: Face::Roman,
                    size: self.base_font_size,
                    page_number,
                });
                // `\@outputpage` ships the head above the column: it is drawn first.
                let n = self.current_page.ops.len() - head_start;
                self.current_page.ops.rotate_right(n);
            }
        }
    }

    pub(crate) fn finish_streaming(&mut self) -> Vec<Page<'a>> {
        self.settle_page();
        self.flush_floats();
        self.merge_page_floats();
        self.place_footnotes();
        let mut last_pages = Vec::with_capacity(2);
        if let Some(old_pending) = self.pending_page.take() {
            self.completed_pages_count += 1;
            last_pages.push(old_pending);
        }
        if std::env::var("RUSHTEX_DEBUG_PASSES").is_ok() {
            eprintln!(
                "Pass stats: P1={}, P2={}, P3={}, Fallback={}",
                self.pass_stats[1], self.pass_stats[2], self.pass_stats[3], self.pass_stats[0]
            );
            eprintln!(
                "Timings: Explode={:.2}ms, Pass1={:.2}ms, HyphSoa={:.2}ms, Pass2={:.2}ms, Flush={:.2}ms",
                self.time_explode_ns as f64 / 1_000_000.0,
                self.time_pass1_ns as f64 / 1_000_000.0,
                self.time_hyph_ns as f64 / 1_000_000.0,
                self.time_pass2_ns as f64 / 1_000_000.0,
                self.time_flush_ns as f64 / 1_000_000.0,
            );
        }
        if !self.current_page.ops.is_empty() || self.completed_pages_count == 0 {
            self.decorate_page();
            self.completed_pages_count += 1;
            let page = std::mem::replace(
                &mut self.current_page,
                Page {
                    width: self.page_width,
                    height: self.page_height,
                    font_family: self.font_family,
                    used_faces: 0,
                    ops: Vec::new(),
                    text_pool: Vec::new(),
                },
            );
            last_pages.push(page);
        }
        last_pages
    }

    pub(crate) fn finish(mut self) -> Vec<Page<'a>> {
        if self.streaming_mode {
            let mut remaining = self.finish_streaming();
            let mut all = Vec::new();
            while let Some(p) = self.take_completed_page() {
                all.push(p);
            }
            all.append(&mut remaining);
            return all;
        }

        self.settle_page();
        self.flush_floats();
        self.merge_page_floats();
        self.place_footnotes();

        if !self.current_page.ops.is_empty() || self.pages.is_empty() {
            self.decorate_page();
            self.pages.push(self.current_page);
        }
        if std::env::var("RUSHTEX_DEBUG_PASSES").is_ok() {
            eprintln!(
                "Pass stats: P1={}, P2={}, P3={}, Fallback={}",
                self.pass_stats[1], self.pass_stats[2], self.pass_stats[3], self.pass_stats[0]
            );
        }
        self.pages
    }

    #[inline(always)]
    pub(crate) fn push_op(&mut self, op: Paint) {
        self.current_page.push_op(op);
    }

    #[inline(always)]
    pub(crate) fn recycle_page(&mut self, mut page: Page<'a>) {
        if self.recycled_pages.len() < 4 {
            page.ops.clear();
            page.text_pool.clear();
            page.used_faces = 0;
            self.recycled_pages.push(page);
        }
    }

    pub(crate) fn new_page(&mut self) {
        self.merge_page_floats();
        self.place_footnotes();
        self.decorate_page();
        let previous_page = if let Some(mut recycled) = self.recycled_pages.pop() {
            recycled.width = self.page_width;
            recycled.height = self.page_height;
            recycled.font_family = self.font_family;
            recycled.used_faces = 0;
            recycled.ops.clear();
            recycled.text_pool.clear();
            std::mem::replace(&mut self.current_page, recycled)
        } else {
            let (ops, text_pool) = if self.is_dry_run {
                (Vec::new(), Vec::new())
            } else {
                (Vec::with_capacity(64), Vec::with_capacity(600))
            };
            std::mem::replace(
                &mut self.current_page,
                Page {
                    width: self.page_width,
                    height: self.page_height,
                    font_family: self.font_family,
                    used_faces: 0,
                    ops,
                    text_pool,
                },
            )
        };

        if self.streaming_mode {
            if let Some(old_pending) = self.pending_page.take() {
                self.completed_pages_count += 1;
                self.completed_pages.push_back(old_pending);
            }
            self.pending_page = Some(previous_page);
        } else {
            self.pages.push(previous_page);
        }

        self.floats.area_h = 0.0;
        self.y = self.page_start_y();
        self.page_builder.reset(self.text_top());
        self.has_preceding_text = false;
        self.last_line_width = 0.0;
        self.last_line_indent = 0.0;
        self.prev_graf = 0;
        self.start_page_floats();
    }

    /// Baseline of the running foot (`\footskip` below the text block).
    #[inline(always)]
    pub(crate) fn foot_baseline(&self) -> Sp {
        Sp::pt(self.margin_bottom) - self.foot_skip
    }

    /// Top edge of the text block on the current page.
    #[inline(always)]
    pub(crate) fn text_top(&self) -> Sp {
        Sp(crate::config::paper_sp(self.page_height)) - self.margin_top
    }

    /// Baseline of the first line on a fresh page (`\topskip` below the text-block top).
    #[inline(always)]
    pub(crate) fn page_start_y(&self) -> Sp {
        self.text_top() - self.top_float_offset() - self.top_skip
    }

    /// True while nothing has been contributed to the current page; TeX discards
    /// glue, kerns and penalties in this state (tex.web §1000).
    #[inline(always)]
    pub(crate) fn at_page_top(&self) -> bool {
        self.y >= self.page_start_y() - 0.01
    }

    /// LaTeX's `\addvspace`: when the last glue (`last`, already on the page) is smaller,
    /// it is replaced by `skip` (natural, stretch, shrink); otherwise nothing is added.
    /// Returns the size of the last skip afterwards.
    pub(crate) fn addvspace(&mut self, last: f32, skip: (f32, f32, f32)) -> f32 {
        if last <= 0.0 {
            self.vglue(skip.0, skip.1, skip.2);
            skip.0
        } else if skip.0 > last {
            self.cancel_last_glue();
            self.vglue(skip.0 - last, skip.1, skip.2);
            self.set_lastskip_width(skip.0);
            skip.0
        } else {
            last
        }
    }

    /// Discardable vertical glue: vanishes at the top of a page like TeX's page builder does.
    #[inline(always)]
    pub(crate) fn vskip(&mut self, amount: f32) {
        if !self.at_page_top() {
            self.y -= amount;
            self.note_glue(amount);
        }
    }

    pub(crate) fn ensure_room(&mut self, required_height: f32) -> bool {
        if self.y - required_height < self.goal_bottom() - 0.01 {
            self.new_page();
            true
        } else {
            false
        }
    }

    pub(crate) fn layout_blocks<'doc>(&mut self, blocks: &'doc [Block<'a>], depth: usize) {
        let mut prev_was_block = false;
        for b in blocks {
            if let Block::ConfigChange(cfg) = b {
                self.apply_config_change(cfg);
                continue;
            }
            if prev_was_block {
                self.block_breakpoint(b);
                self.par_skip_glue(b);
            }
            prev_was_block = true;
            self.layout_single_block(b, depth);
        }
    }

    pub(crate) fn layout_stream_block<'doc>(&mut self, b: &'doc Block<'a>, prev_was_block: &mut bool) {
        if let Block::ConfigChange(cfg) = b {
            self.apply_config_change(cfg);
            return;
        }
        if *prev_was_block {
            self.block_breakpoint(b);
            self.par_skip_glue(b);
        }
        *prev_was_block = true;
        self.layout_single_block(b, 0);
    }

    /// `\parskip` (LaTeX: 0pt plus 1pt) before a new paragraph; a display belongs to the
    /// paragraph before it. Vertical-mode material (skips, rules, page breaks) starts no
    /// paragraph and gets none, nor does a `lstlisting`: its lines are boxes appended in
    /// vertical mode. (`verbatim` is a trivlist item, a paragraph.)
    fn par_skip_glue(&mut self, b: &Block<'_>) {
        // A heading's `\parskip` comes after its `\addvspace` (`layout_section`); a list's
        // is in its `\@topsep` (`trivlist_topsep`).
        // `\maketitle` starts with `\null` and `\tableofcontents` with its heading: no paragraph.
        if matches!(b, Block::VSpace { .. } | Block::RunInHeadSkip { .. } | Block::VMark(_) | Block::PageLabel(_) | Block::HRule | Block::PageBreak | Block::Figure(_) | Block::Section { .. } | Block::List(_) | Block::Quote(_))
            || matches!(b, Block::TitleBlock(_) | Block::TableOfContents | Block::ListOfFigures | Block::ListOfTables | Block::Aligned(_))
            // A paragraph of nothing but declarations (`{\small` in vertical mode) is none.
            || matches!(b, Block::Para(inls) if is_blank_inlines(inls))
            // `verbatim`'s item paragraph adds its `\parskip` after `\@topsep`
            // (`layout_code_listing`).
            || matches!(b, Block::CodeListing(_))
            || matches!(b, Block::Table(t) if t.placement != 0 && !t.is_longtable)
        {
            return;
        }
        if continues_par(b) {
            return;
        }
        self.vglue(self.par_skip, self.par_skip_stretch, self.par_skip_shrink);
    }

    /// `\@trivlist`'s `\@topsep`: `\@topsepadd` advanced by `\parskip`.
    fn trivlist_topsep(&self, sep: (f32, f32, f32)) -> (f32, f32, f32) {
        let ps = self.item_par_skip.unwrap_or((self.par_skip, self.par_skip_stretch, self.par_skip_shrink));
        (sep.0 + ps.0, sep.1 + ps.1, sep.2 + ps.2)
    }

    /// The `\parskip` glue starting a later paragraph of a list item: a breakpoint when a
    /// line box precedes it (tex.web §973).
    fn par_glue_breakpoint(&mut self) {
        if !self.page_builder.tail_discardable {
            self.page_breakpoint(0);
        }
    }

    /// The skip before a list's first `\item`: `\addvspace\@topsep` then
    /// `\addvspace{-\parskip}`, which the item's paragraph's own `\parskip` (`\parsep`)
    /// cancels. Right after a heading (`\@nobreak`) it is `\@nbitem`'s
    /// `\addvspace{\@outerparskip - \parskip}` instead, which nets the outer `\parskip`.
    fn list_begin_skip(&mut self, added_vspace: f32, top: (f32, f32, f32), parsep: f32, nopar: bool) {
        if nopar {
        } else if !self.after_heading {
            self.addvspace(added_vspace, self.trivlist_topsep(top));
        } else if parsep > 0.0 {
            self.vglue(self.par_skip, self.par_skip_stretch, self.par_skip_shrink);
        }
    }

    /// The breakpoint between the previous block and `b`. LaTeX's own penalties (headings,
    /// lists, trivlists) are `\addpenalty`s, which go in front of the previous skip.
    fn block_breakpoint(&mut self, b: &Block<'_>) {
        let penalty = self.block_break_penalty(b);
        // A paragraph's `\parskip` is a breakpoint only after a box (tex.web §973).
        if penalty == 0 && self.page_builder.tail_discardable {
            return;
        }
        if matches!(b, Block::Section { .. } | Block::RunInHeadSkip { .. } | Block::List(_) | Block::Quote(_))
            || matches!(b, Block::CodeListing(l) if l.options.verbatim)
            || is_theorem(b)
        {
            self.page_addpenalty(penalty);
        } else {
            self.page_breakpoint(penalty);
        }
    }

    /// Penalty of the breakpoint between the previous block and `b`: `\@secpenalty` before a
    /// heading, none after one, before a display (`\predisplaypenalty`) or after one (the
    /// display already ended with its `\postdisplaypenalty`).
    fn block_break_penalty(&self, b: &Block<'_>) -> i32 {
        use crate::layout::layout_pagebuilder::{INF_PENALTY, SEC_PENALTY};
        match b {
            _ if self.after_heading => INF_PENALTY,
            // A rule is no breakpoint: only glue and penalties are (tex.web §973). A float
            // in vertical mode leaves nothing in the list: its `\@floatpenalty` goes to the
            // output routine, which puts the page back unbroken.
            // A display starting a paragraph has its `\parskip` glue for a breakpoint.
            Block::Equation {
                par: DisplayPar::Starts | DisplayPar::StartsWithBox | DisplayPar::StartsEmpty,
                ..
            } => 0,
            Block::Equation { .. } | Block::HRule | Block::Figure(_) | Block::VMark(_) | Block::PageLabel(_) => INF_PENALTY,
            Block::Table(t) if t.placement != 0 && !t.is_longtable => INF_PENALTY,
            Block::Section { .. } | Block::RunInHeadSkip { .. } => SEC_PENALTY,
            // The list's `\@beginparpenalty`.
            Block::List(_) | Block::Quote(_) => -51,
            _ if is_theorem(b) => -51,
            // `listings`' `\par\penalty-50` and the trivlist's `\@beginparpenalty`.
            Block::CodeListing(l) if !l.options.verbatim => -50,
            Block::CodeListing(_) => -51,
            _ if self.page_builder.after_display => INF_PENALTY,
            _ => 0,
        }
    }

    fn apply_config_change(&mut self, change: &crate::tex_parser::ConfigChange) {
        match change {
            crate::tex_parser::ConfigChange::Appendix => self.section_counters = [APPENDIX_SECTION, 0, 0, 0],
            crate::tex_parser::ConfigChange::PeriodSf1000(on) => self.period_sf_1000 = *on,
            crate::tex_parser::ConfigChange::ColumnWrap(w) => {
                self.column_wrap = (!w.left.is_empty() || !w.right.is_empty()).then(|| w.clone());
            }
            crate::tex_parser::ConfigChange::PageStyle(s) => self.page_style = *s,
            crate::tex_parser::ConfigChange::ThisPageStyle(s) => self.this_page_style = Some(*s),
            crate::tex_parser::ConfigChange::ParIndent(v) => self.par_indent = *v,
            crate::tex_parser::ConfigChange::ParSkip(w, st, sh) => {
                (self.par_skip, self.par_skip_stretch, self.par_skip_shrink) = (*w, *st, *sh);
            }
            crate::tex_parser::ConfigChange::HangIndent(v) => self.hang_indent = *v,
            crate::tex_parser::ConfigChange::HangAfter(v) => self.hang_after = *v,
            crate::tex_parser::ConfigChange::LeftSkip(v) => self.left_skip = *v,
            crate::tex_parser::ConfigChange::RightSkip(v) => self.right_skip = *v,
            crate::tex_parser::ConfigChange::LabelSep(v) => self.list_label_sep = *v,
            crate::tex_parser::ConfigChange::LeftMargin(v) => self.list_left_margin = *v,
            crate::tex_parser::ConfigChange::ItemIndent(v) => self.list_item_indent = *v,
            crate::tex_parser::ConfigChange::ItemSep(v) => self.list_item_sep = *v,
            crate::tex_parser::ConfigChange::ListParIndent(v) => self.list_par_indent = *v,
            crate::tex_parser::ConfigChange::ParShape(shape) => {
                self.par_shape.clear();
                self.par_shape.extend_from_slice(shape);
            }
            crate::tex_parser::ConfigChange::PreTolerance(v) => self.pre_tolerance = *v,
            crate::tex_parser::ConfigChange::Tolerance(v) => self.tolerance = *v,
            crate::tex_parser::ConfigChange::EmergencyStretch(v) => self.emergency_stretch = *v,
            crate::tex_parser::ConfigChange::Looseness(v) => self.looseness = *v,
            crate::tex_parser::ConfigChange::ParFillSkip { width, stretch } => {
                self.par_fill_width = *width;
                self.par_fill_stretch = *stretch;
            }
            crate::tex_parser::ConfigChange::PrevGraf(v) => self.prev_graf = *v,
            crate::tex_parser::ConfigChange::LastLineFit(v) => self.last_line_fit = *v,
            crate::tex_parser::ConfigChange::Sloppy => {
                self.tolerance = 9999;
                self.emergency_stretch = 33.0;
            }
            crate::tex_parser::ConfigChange::Fussy => {
                self.tolerance = 200;
                self.emergency_stretch = 0.0;
            }
            crate::tex_parser::ConfigChange::FontSize(fs) => {
                (self.base_font_size, self.base_leading) = fs.metrics(self.class_font_size);
                if let Some(s) = fs.display_skips(self.class_font_size) {
                    self.display_skips = s;
                }
            }
            crate::tex_parser::ConfigChange::Align(a) => {
                // Each alignment declaration also sets \parindent to 0pt.
                self.par_align = *a;
                self.par_indent = 0.0;
            }
        }
    }

    pub(crate) fn layout_single_block<'doc>(&mut self, b: &'doc Block<'a>, depth: usize) {
        // `\addvspace` glue left by the previous block, merged with the next one's.
        let added_vspace = std::mem::take(&mut self.last_addvspace);
        // Declarations between paragraphs leave the previous line's advance in place, and
        // so do the trivlists, whose first paragraph takes it.
        if !matches!(b, Block::ConfigChange(_) | Block::Aligned(_) | Block::Quote(_)) {
            self.block_entry_advance = std::mem::take(&mut self.last_line_advance);
        }
        match b {
            Block::ConfigChange(cfg) => self.apply_config_change(cfg),
            Block::Para(inls) => {
                for inl in inls.iter() {
                    if let Inline::PageLabel(id) = inl {
                        self.note_page_label(*id);
                    }
                }
                self.layout_para(inls);
                // Only spaces and declarations (`\]}` and a line end before a list): the
                // next `\list` does `\unskip\par`, so the empty paragraph vanishes and its
                // `\addvspace` still merges with the skip before it (latex.ltx `\list`).
                if is_blank_inlines(inls) {
                    self.last_addvspace = added_vspace;
                }
            }
            Block::List(list_data) => {
                self.layout_list(list_data, depth, added_vspace);
            }
            Block::Quote(quote_data) => self.layout_quote(quote_data, depth, added_vspace),
            Block::Aligned(aligned) => {
                // latex.ltx: `center`, `flushleft` and `flushright` are trivlists, so
                // \topsep is `\addvspace`d above and below: two in a row share one gap.
                let saved_align = self.par_align;
                let saved_par_indent = self.par_indent;
                // `\trivlist` reads `\topsep` as it is: the size commands change it only in
                // `\@listi`, which `\list` runs and `\trivlist` does not.
                let (top, partop) = class_topsep(self.class_font_size);
                // `\@topsepadd`: `\topsep`, plus `\partopsep` after a blank line, with their
                // stretch and shrink. amsthm sets `\@topsep` and `\@topsepadd` to its pre- and
                // post-skip instead.
                let sep = match aligned.kind {
                    TrivlistKind::Environment => topsep_add(top, partop, aligned.in_vmode),
                    TrivlistKind::Theorem { half_skip: false, .. } => top,
                    TrivlistKind::Theorem { half_skip: true, .. } => (0.5 * top.0, 0.5 * top.1, 0.5 * top.2),
                    TrivlistKind::Proof => topsep_add((6.0, 6.0, 0.0), partop, true),
                };
                let top_sep = sep.0;
                // Right after a heading (`\@nobreak`) the first `\item` is `\@nbitem`: no `\topsep`.
                if !self.after_heading {
                    self.addvspace(added_vspace, self.trivlist_topsep(sep));
                }
                let theorem = aligned.kind != TrivlistKind::Environment;
                self.par_align = aligned.align;
                if !theorem {
                    self.par_indent = 0.0;
                }
                let saved_face = self.body_face;
                if let TrivlistKind::Theorem { italic: true, .. } = aligned.kind {
                    self.body_face = Face::Italic;
                }
                // `\trivlist` leaves `\@listdepth` alone: a list inside is at the outer level.
                self.layout_blocks(&aligned.blocks, depth);
                self.body_face = saved_face;
                self.par_align = saved_align;
                self.par_indent = saved_par_indent;
                if theorem {
                    // `\@endparenv`: `\@endparpenalty` and `\addvspace\@topsepadd`.
                    self.page_addpenalty(-51);
                    let after = std::mem::take(&mut self.last_addvspace);
                    self.last_addvspace = self.addvspace(after, sep);
                } else {
                    self.vglue(sep.0, sep.1, sep.2);
                    self.last_addvspace = top_sep;
                }
            }
            Block::Table(table_data) if table_data.placement != 0 && !table_data.is_longtable => {
                // The float box starts at its first caption's \abovecaptionskip, or at
                // the tabular's top.
                let top_y = if table_data.caption_above && table_data.caption.is_some() {
                    Sp::pt(-0.7 * self.base_leading)
                } else {
                    Sp(0)
                };
                let placement = table_data.placement | crate::layout::layout_float::FLOAT_TABLE_KIND;
                let f = self.capture_float(top_y, placement, |l| l.layout_table(table_data));
                self.add_vmode_float(f);
            }
            Block::Table(table_data) if table_data.caption.is_none() && !table_data.is_longtable => {
                self.layout_inline_tabular(table_data);
            }
            Block::Table(table_data) => {
                self.layout_table(table_data);
                self.prev_depth = 0.0;
                self.note_box(self.y, 0.0);
            }
            Block::Figure(fig) => {
                let f = self.capture_float(Sp(0), fig.placement, |l| l.layout_figure(fig.graphic, &fig.caption, fig.centered, fig.vskip));
                self.add_vmode_float(f);
            }
            Block::TitleBlock(title_data) => self.layout_title_block(
                &title_data.title,
                title_data.author.as_deref(),
                title_data.date.as_deref(),
            ),
            Block::Section {
                level,
                starred,
                numbered,
                title,
            } => self.layout_section(*level, *starred, *numbered, crate::frontend::ast::section_title_parts(title).1, added_vspace),
            Block::PageBreak => self.page_breakpoint(crate::layout::layout_pagebuilder::EJECT_PENALTY),
            Block::VSpace { w, stretch, shrink } => {
                self.vglue(*w, *stretch, *shrink);
                self.zero_lastskip();
            }
            Block::VMark(mark) => {
                if let Some(mark) = mark {
                    self.marks.insert(mark);
                }
                self.note_vmark();
            }
            Block::PageLabel(id) => {
                self.note_page_label(*id);
                self.note_vmark();
            }
            Block::RunInHeadSkip { w, stretch, shrink } => {
                if !self.after_heading {
                    self.last_addvspace = self.addvspace(added_vspace, (*w, *stretch, *shrink));
                }
            }
            // `\hrule` in vertical mode: a rule 0.4pt high across `\hsize`, with no
            // interline glue above or below it (tex.web §679 and `\prevdepth` ignore-depth).
            Block::HRule => {
                const RULE_H: f32 = 0.4;
                let bottom = if self.at_page_top() {
                    self.y - (RULE_H - self.top_skip).max(0.0)
                } else {
                    self.y + self.base_leading - self.prev_depth - RULE_H
                };
                if !self.is_dry_run {
                    self.push_op(Paint::Rule { x: self.left.into(), y: bottom, w: (self.right - self.left).into(), h: RULE_H.into(), fill: true });
                }
                self.note_box(bottom, RULE_H);
                self.y = bottom - self.base_leading;
                self.ignore_depth = true;
            }
            Block::CodeListing(listing) => self.layout_code_listing(listing, added_vspace),
            Block::TableOfContents => {
                self.last_addvspace = added_vspace;
                let reg = std::mem::take(&mut self.registry);
                crate::index::layout_toc(self, &reg);
                self.registry = reg;
                // The entries end in a text line advanced by the normal `\baselineskip`.
                self.last_line_advance = self.base_leading;
            }
            Block::ListOfFigures => {
                let reg = std::mem::take(&mut self.registry);
                crate::index::layout_lof(self, &reg);
                self.registry = reg;
            }
            Block::ListOfTables => {
                let reg = std::mem::take(&mut self.registry);
                crate::index::layout_lot(self, &reg);
                self.registry = reg;
            }
            Block::Equation { starred, par, math } => {
                match *par {
                    DisplayPar::Continues => {}
                    DisplayPar::StartsEmpty => self.has_preceding_text = false,
                    _ => self.empty_par_line(*par == DisplayPar::StartsWithBox),
                }
                self.layout_equation(*starred, math);
            }
        }
        if matches!(
            b,
            // Headings and listings leave their last line's depth.
            Block::HRule | Block::TableOfContents | Block::ListOfFigures | Block::ListOfTables
        ) {
            self.prev_depth = 0.0;
        }
    }

    /// LaTeX's `\list` (latex.ltx `\@trivlist`, `\@item`, `\endtrivlist`) with the level
    /// parameters of size10/11/12.clo; enumitem options and `\setlength` override them.
    ///
    /// Vertically: `\addvspace` of `\topsep` (plus `\partopsep` after a blank line) above
    /// and below, `\itemsep` plus `\parskip` (= `\parsep`) before every later item and
    /// `\parsep` before every later paragraph of an item, with `\@itempenalty` (-51)
    /// before items. Horizontally: the text is indented by `\leftmargin`; the label is
    /// right-aligned in a `\labelwidth` box that ends `\labelsep` before the first line,
    /// which a wider label pushes right.
    fn layout_list<'doc>(&mut self, list: &'doc crate::frontend::ast::ListData<'a>, depth: usize, added_vspace: f32) {
        use crate::frontend::ast::ListKind;
        let (kind, options, items) = (list.kind, &list.options, &list.items[..]);
        if items.is_empty() {
            return;
        }
        let level = depth.min(5);
        let em = crate::backend::scaled::quad_pt(self.font_family, self.base_font_size);
        let p = list_level_params(self.class_font_size, self.base_font_size, level);
        // article.cls sets `\leftmargini`.. and `\labelsep` in ems of the class's
        // `\normalsize` font when it loads: they do not follow later size changes.
        let class_em = crate::backend::scaled::quad_pt(self.font_family, self.class_font_size);
        const LEFT_MARGINS: [f32; 6] = [2.5, 2.2, 1.87, 1.7, 1.0, 1.0];
        let class_left = |lvl: usize| if lvl == 0 && !self.list_left_margin.is_nan() { self.list_left_margin } else { crate::backend::scaled::em_mul(LEFT_MARGINS[lvl], class_em) };
        let label_sep = options.label_sep_or(if self.list_label_sep.is_nan() { crate::backend::scaled::em_mul(0.5, class_em) } else { self.list_label_sep });
        let item_indent = options.item_indent_or(self.list_item_indent);
        let parsep = options.parsep_or(p.parsep.0);
        let custom_hang = options.hang_indent_or(0.0);
        let left_margin = if kind == ListKind::Description { class_left(level) } else { options.left_margin_or(class_left(level)) };
        // article.cls sets `\labelwidth` to `\leftmargini - \labelsep` once, with the class
        // `\labelsep`; deeper levels recompute it with the current one. enumitem derives it
        // from its own options.
        let label_width = if kind == ListKind::Description {
            0.0
        } else if let Some(lw) = options.label_width() {
            lw
        } else if options.left_margin().is_some() || options.label_sep_or(f32::NAN).is_finite() {
            left_margin + item_indent - label_sep
        } else if level == 0 {
            class_left(0) - crate::backend::scaled::em_mul(0.5, em)
        } else {
            left_margin - label_sep
        };
        // `\begin{description}` sets `\itemindent` to `-\leftmargin`.
        let item_indent = if kind == ListKind::Description { -left_margin } else { item_indent };
        let saved_item_sep = self.list_item_sep;
        self.list_item_sep = options.item_sep_or(p.itemsep.0);
        let custom_item_sep = options.item_sep_or(f32::NAN).is_finite();

        // 2. Select marker constructor based on depth, kind, and options
        let depth_slot = match kind {
            ListKind::Enumerate => Some(&mut self.enum_depth),
            ListKind::Description => None,
            _ => Some(&mut self.item_depth),
        };
        let kind_depth = depth_slot.map_or(0, |d| {
            *d += 1;
            (*d - 1) as usize
        });
        let make_marker = |idx: usize| -> Marker {
            if kind == crate::frontend::ast::ListKind::Enumerate {
                let num = options.start_or(1) as u32 + idx as u32;
                match options.label_format {
                    1 => Marker::Number(num),      // 1.
                    2 => Marker::Alpha(num),       // (a)
                    3 => Marker::AlphaDot(num),    // a.
                    4 => Marker::UpperAlpha(num),  // A.
                    5 => Marker::ParenNumber(num), // (1)
                    6 => Marker::Roman(num),       // i.
                    7 => Marker::UpperRoman(num),  // I.
                    _ => match kind_depth {
                        0 => Marker::Number(num),
                        1 => Marker::Alpha(num),
                        2 => Marker::Roman(num),
                        _ => Marker::UpperAlpha(num),
                    },
                }
            } else {
                match options.label_format {
                    8 => Marker::Dash,
                    9 => Marker::Asterisk,
                    _ => match kind_depth {
                        0 => Marker::Bullet,
                        1 => Marker::Dash,
                        2 => Marker::Asterisk,
                        _ => Marker::MiddleDot,
                    },
                }
            }
        };

        let top = if list.in_vmode {
            (p.topsep.0 + p.partopsep.0, p.topsep.1 + p.partopsep.1, p.topsep.2 + p.partopsep.2)
        } else {
            p.topsep
        };
        let nopar = std::mem::take(&mut self.in_label);
        self.list_begin_skip(added_vspace, top, parsep, nopar);

        for (idx, item) in items.iter().enumerate() {
            if idx > 0 {
                // `\addpenalty\@itempenalty`, `\addvspace\itemsep`, then the item's `\parskip`.
                self.page_addpenalty(-51);
                let (st, sh) = if custom_item_sep { (0.0, 0.0) } else { (p.itemsep.1, p.itemsep.2) };
                // After a nested list it merges with that list's `\addvspace\@topsepadd`.
                let last = std::mem::take(&mut self.last_addvspace);
                self.addvspace(last, (self.list_item_sep, st, sh));
                self.vglue(parsep, p.parsep.1, p.parsep.2);
            } else if parsep <= 0.0 {
                // `\addvspace{-\parskip}` after `\@topsep` does nothing for a `\parsep` of
                // no natural width (`\@xaddvskip` compares dimensions), so the item's
                // paragraph keeps its own `\parskip`.
                self.vglue(parsep, p.parsep.1, p.parsep.2);
            }
            self.last_addvspace = 0.0;

            // The label box and where the item's first line starts relative to it.
            let label_x0 = self.left + left_margin + item_indent - label_sep - label_width;
            let mut label_overflow = 0.0;
            if let Some(ref custom_inls) = item.custom_label {
                // The label is set in the current font (italic inside a plain-style theorem).
                let label_face = self.body_face.with_bold(kind == ListKind::Description);
                let w = self.measure_inlines_face_size(custom_inls, label_face, self.base_font_size);
                // `\descriptionlabel` starts with `\hspace\labelsep`.
                let lead = if kind == ListKind::Description { label_sep } else { 0.0 };
                let box_w = lead + w;
                label_overflow = (box_w - label_width).max(0.0);
                let x = label_x0 + lead + (label_width - box_w).max(0.0);
                self.paint_inlines_at_size(custom_inls, label_face, self.base_font_size, x, self.y);
            } else if kind != ListKind::Description {
                let marker = make_marker(idx);
                // article.cls `\labelitemi`..`iv`: `\textbullet`, `\bfseries\textendash`,
                // `\textasteriskcentered`, `\textperiodcentered`; all but the dash are TS1
                // symbols (latex.ltx `\DeclareTextSymbolDefault{..}{TS1}`).
                let marker_face = match marker {
                    _ if self.font_family != metrics::FontFamily::ComputerModern => Face::Roman,
                    Marker::Bullet | Marker::Asterisk | Marker::MiddleDot => Face::TextComp,
                    Marker::Dash => self.body_face.with_bold(true),
                    _ => self.body_face,
                };
                let w = self.marker_width(marker, marker_face, self.base_font_size);
                label_overflow = (w - label_width).max(0.0);
                if !self.is_dry_run {
                    self.push_op(Paint::Marker {
                        x: (label_x0 + (label_width - w).max(0.0)).into(),
                        y: self.y,
                        face: marker_face,
                        size: self.base_font_size,
                        marker,
                    });
                }
            }

            let saved_left = self.left;
            self.left += left_margin;
            let first_block_indent = item_indent + label_overflow;

            let mut is_first_block = true;
            for block in &item.blocks {
                match block {
                    Block::ConfigChange(cfg) => {
                        self.apply_config_change(cfg);
                    }
                    Block::Para(inls) => {
                        let indent = if is_first_block {
                            first_block_indent
                        } else {
                            if !continues_par(block) {
                                self.par_glue_breakpoint();
                                self.vglue(parsep, p.parsep.1, p.parsep.2);
                            }
                            self.last_addvspace = 0.0;
                            self.list_par_indent
                        };
                        self.layout_para_full(inls, Some(indent), Some(custom_hang));
                        is_first_block = false;
                    }
                    other => {
                        if !is_first_block {
                            self.block_breakpoint(other);
                            // A nested list's `\parskip` is in its `\@topsep`.
                            if !matches!(other, Block::List(_) | Block::Quote(_)) && !continues_par(other) {
                                self.vglue(parsep, p.parsep.1, p.parsep.2);
                            }
                        }
                        let saved = self.item_par_skip.replace((parsep, p.parsep.1, p.parsep.2));
                        self.in_label = is_first_block && matches!(other, Block::List(_) | Block::Quote(_));
                        self.layout_single_block(other, depth + 1);
                        self.in_label = false;
                        self.item_par_skip = saved;
                        is_first_block = false;
                    }
                }
            }

            self.left = saved_left;
        }

        // `\endtrivlist`: a skip ending the last item (a display's) takes on the list's
        // `\parskip` in place of the outer one.
        if self.last_addvspace > 0.0 {
            let outer = self.item_par_skip.unwrap_or((self.par_skip, self.par_skip_stretch, self.par_skip_shrink));
            let d = (parsep - outer.0, p.parsep.1 - outer.1, p.parsep.2 - outer.2);
            self.last_addvspace = self.endtrivlist_skip(self.last_addvspace, d);
        }
        // `\@endparenv`: `\@endparpenalty` and `\addvspace\@topsepadd`.
        if !nopar {
            self.page_addpenalty(-51);
            let after = std::mem::take(&mut self.last_addvspace);
            self.last_addvspace = self.addvspace(after, top);
        }
        self.list_item_sep = saved_item_sep;
        match kind {
            ListKind::Enumerate => self.enum_depth -= 1,
            ListKind::Description => {}
            _ => self.item_depth -= 1,
        }
    }

    /// `quote`, `quotation` and `verse` (latex.ltx): one-item `\list`s with an empty label
    /// and `\rightmargin\leftmargin`. `quotation` indents paragraphs by 1.5em with
    /// `\parsep` 0pt plus 1pt; `verse` hangs lines by 1.5em and ends a paragraph at every
    /// `\\` (`\@centercr`, whose `\addvspace{-\parskip}` cancels the next `\parsep`).
    fn layout_quote<'doc>(&mut self, q: &'doc crate::frontend::ast::QuoteData<'a>, depth: usize, added_vspace: f32) {
        use crate::frontend::ast::QuoteKind;
        let level = depth.min(5);
        let em = crate::backend::scaled::quad_pt(self.font_family, self.base_font_size);
        let p = list_level_params(self.class_font_size, self.base_font_size, level);
        // `\leftmargini`.. are in ems of the class's `\normalsize` (`layout_list`).
        let class_em = crate::backend::scaled::quad_pt(self.font_family, self.class_font_size);
        const LEFT_MARGINS: [f32; 6] = [2.5, 2.2, 1.87, 1.7, 1.0, 1.0];
        let margin = if level == 0 && !self.list_left_margin.is_nan() { self.list_left_margin } else { LEFT_MARGINS[level] * class_em };
        let (item_indent, par_indent, parsep, extra_left) = match q.kind {
            QuoteKind::Quote => (0.0, 0.0, p.parsep, 0.0),
            QuoteKind::Quotation => (crate::backend::scaled::em_mul(1.5, em), crate::backend::scaled::em_mul(1.5, em), (0.0, 1.0, 0.0), 0.0),
            QuoteKind::Verse => (crate::backend::scaled::em_mul(-1.5, em), crate::backend::scaled::em_mul(-1.5, em), p.parsep, crate::backend::scaled::em_mul(1.5, em)),
        };
        let top = if q.in_vmode {
            (p.topsep.0 + p.partopsep.0, p.topsep.1 + p.partopsep.1, p.topsep.2 + p.partopsep.2)
        } else {
            p.topsep
        };
        let nopar = std::mem::take(&mut self.in_label);
        self.list_begin_skip(added_vspace, top, parsep.0, nopar);
        // As for a list's first item (`layout_list`).
        if parsep.0 <= 0.0 {
            self.vglue(parsep.0, parsep.1, parsep.2);
        }
        self.last_addvspace = 0.0;
        let (saved_left, saved_right) = (self.left, self.right);
        self.left += margin + extra_left;
        self.right -= margin;
        let mut first = true;
        for block in &q.blocks {
            match block {
                Block::ConfigChange(cfg) => self.apply_config_change(cfg),
                Block::Para(inls) => {
                    let lines: Vec<&'doc [Inline<'a>]> = if q.kind == QuoteKind::Verse {
                        inls.split(|i| matches!(i, Inline::Break)).collect()
                    } else {
                        vec![&inls[..]]
                    };
                    for (k, line) in lines.into_iter().enumerate() {
                        if !first && k == 0 {
                            self.par_glue_breakpoint();
                            self.vglue(parsep.0, parsep.1, parsep.2);
                            self.last_addvspace = 0.0;
                        }
                        let indent = if first { item_indent } else { par_indent };
                        self.layout_para_full(line, Some(indent), None);
                        first = false;
                    }
                }
                other => {
                    if !first {
                        self.block_breakpoint(other);
                        if !matches!(other, Block::List(_) | Block::Quote(_)) {
                            self.vglue(parsep.0, parsep.1, parsep.2);
                        }
                    }
                    let saved = self.item_par_skip.replace(parsep);
                    self.layout_single_block(other, depth + 1);
                    self.item_par_skip = saved;
                    first = false;
                }
            }
        }
        self.left = saved_left;
        self.right = saved_right;
        if !nopar {
            self.page_addpenalty(-51);
            let after = std::mem::take(&mut self.last_addvspace);
            self.last_addvspace = self.addvspace(after, top);
        }
    }

    fn layout_title_block<'doc>(
        &mut self,
        title: &'doc [Inline<'a>],
        author: Option<&'doc [Inline<'a>]>,
        date: Option<&'doc [Inline<'a>]>,
    ) {
        // `\maketitle` sets `\@topnum` to zero: no float at the top of the title page, and
        // does `\thispagestyle{plain}`.
        self.floats.top_left = 0;
        self.this_page_style = Some(crate::frontend::ast::PageStyle::Plain);
        // article.cls \maketitle: `\null\vskip 2em`, then a `center` environment holding
        // `{\LARGE \@title\par}`, `\vskip 1.5em`, `{\large\lineskip .5em` the authors in a
        // `tabular[t]{c}``\par}`, `\vskip 1em`, `{\large \@date}`; `\par\vskip 1.5em` after it.
        let max_w = (self.right - self.right_skip) - (self.left + self.left_skip);
        // The em of the body font: cmr12's quad is 11.75pt, not 12pt.
        let em = crate::backend::scaled::quad_pt(self.font_family, self.base_font_size);
        let (title_size, title_bls) = crate::frontend::ast::FontSize::LargeXX.metrics(self.base_font_size);
        let (large, large_bls) = crate::frontend::ast::FontSize::Large.metrics(self.base_font_size);
        // Baseline distance after a box of depth `pd` to one of height `h` (tex.web §679).
        let interline = |bls: f32, lineskip: f32, pd: f32, h: f32| if bls - pd - h >= 0.0 { bls } else { pd + h + lineskip };

        // Author and date are measured first: the title's words stay in `cell_words_soa`.
        let extent = |l: &mut Self, inls: &'doc [Inline<'a>], size: f32| {
            l.cell_words_soa.clear();
            let mut math = Vec::new();
            explode_inlines_into_family(inls, Face::Roman, size, &mut l.cell_words_soa, l.font_family, &mut math);
            let n = l.cell_words_soa.len();
            l.cell_words_soa.line_extent(0, n, size, l.font_family)
        };
        // The authors are a `tabular[t]{c}`: a row per `\\`, each with `\@arstrut`, stacked
        // with `\lineskip` 0 (`\baselineskip` is 0 inside); the box's height is the first
        // row's, its depth the rest.
        let rows: Vec<&'doc [Inline<'a>]> = author.map_or(Vec::new(), split_top_level_rows);
        let row_ext: Vec<(f32, f32)> = rows
            .iter()
            .map(|r| {
                let (h, d) = extent(self, r, large);
                (h.max(crate::backend::scaled::em_mul(0.7, large_bls)), d.max(crate::backend::scaled::em_mul(0.3, large_bls)))
            })
            .collect();
        let (author_h, author_d) = match row_ext.first() {
            Some(&(h, _)) => {
                let below = row_ext.windows(2).map(|w| w[0].1 + w[1].0).sum::<f32>();
                (h, below + row_ext.last().map_or(0.0, |r| r.1))
            }
            None => (0.0, 0.0),
        };
        let (date_h, date_d) = date.map_or((0.0, 0.0), |d| extent(self, d, large));

        self.cell_words_soa.clear();
        let mut title_math = Vec::new();
        let title_w = explode_inlines_into_family(
            title,
            Face::Roman,
            title_size,
            &mut self.cell_words_soa,
            self.font_family,
            &mut title_math,
        );
        let n = self.cell_words_soa.len();
        let (title_h, title_d) = self.cell_words_soa.line_extent(0, n, title_size, self.font_family);
        let title_lines = if title_w <= max_w && !inlines_have_break(title) {
            1
        } else {
            self.cell_lines.clear();
            self.break_cell_words_into_lines(0, n, max_w, title_size, false, true);
            self.cell_lines.len().max(1)
        };


        // `\null` is an empty line at the current baseline (\topskip at a page top).
        self.place_baseline(0.0);
        let first = crate::backend::scaled::em_mul(2.0, em) + interline(title_bls, LINESKIP_PT, 0.0, title_h);
        let below_title = ((title_lines - 1) as f32) * title_bls;
        let mut needed = first + below_title;
        if author.is_some() {
            needed += crate::backend::scaled::em_mul(1.5, em) + self.par_skip + interline(large_bls, 0.5 * large, title_d, author_h);
        }
        if date.is_some() {
            needed += em + self.par_skip + interline(self.base_leading, LINESKIP_PT, author_d, date_h);
        }
        if self.ensure_room(needed) {
            self.place_baseline(0.0);
        }
        self.y -= first;

        if title_lines == 1 {
            let title_x = self.left + self.left_skip + (max_w - title_w).max(0.0) * 0.5;
            self.paint_inlines_at_size(title, Face::Roman, title_size, title_x, self.y);
        } else {
            let line_count = self.cell_lines.len();
            for i in 0..line_count {
                let line_desc = self.cell_lines[i];
                let line_x = self.left + self.left_skip + (max_w - line_desc.line_w).max(0.0) * 0.5;
                self.paint_cell_line_at(line_desc, line_x, self.y, title_size, &title_math);
                if i + 1 < line_count {
                    self.y -= title_bls;
                }
            }
        }
        let mut depth = title_d;

        // Each part of the title is a paragraph of the `center` environment: `\parskip` first.
        let par_skip = (self.par_skip, self.par_skip_stretch, self.par_skip_shrink);
        if author.is_some() {
            self.vglue(crate::backend::scaled::em_mul(1.5, em), 0.0, 0.0);
            self.vglue(par_skip.0, par_skip.1, par_skip.2);
            self.y -= interline(large_bls, 0.5 * large, depth, author_h);
            // The rows are centred in the tabular, which is centred on the line;
            // `\tabcolsep` pads each side.
            let widths: Vec<f32> = rows.iter().map(|r| self.measure_inlines_face_size(r, Face::Roman, large)).collect();
            let col_w = widths.iter().copied().fold(0.0, f32::max);
            let tab_x = self.left + self.left_skip + (max_w - col_w - 2.0 * TABCOLSEP_PT).max(0.0) * 0.5;
            let top = self.y;
            let mut y = top;
            for (k, (row, w)) in rows.iter().zip(&widths).enumerate() {
                if k > 0 {
                    y -= row_ext[k - 1].1 + row_ext[k].0;
                }
                let x = tab_x + TABCOLSEP_PT + (col_w - w) * 0.5;
                self.paint_inlines_at_size(row, Face::Roman, large, x, y);
            }
            depth = author_d;
        }

        if let Some(dt) = date {
            self.vglue(em, 0.0, 0.0);
            self.vglue(par_skip.0, par_skip.1, par_skip.2);
            self.y -= interline(self.base_leading, LINESKIP_PT, depth, date_h);
            let dt_w = self.measure_inlines_face_size(dt, Face::Roman, large);
            let dt_x = self.left + self.left_skip + (max_w - dt_w).max(0.0) * 0.5;
            self.paint_inlines_at_size(dt, Face::Roman, large, dt_x, self.y);
            depth = date_d;
        }
        self.note_box(self.y, 0.0);

        // `\end{center}` adds \topsep + \partopsep, then `\vskip 1.5em`; a following
        // heading's \addvspace replaces that last skip.
        let (top, partop) = class_topsep(self.base_font_size);
        self.y -= self.base_leading;
        // \topsep (8pt plus 2pt minus 4pt at 10pt) and \partopsep, then \vskip 1.5em.
        self.vglue(top.0 + partop.0, top.1 + partop.1, top.2 + partop.2);
        self.vglue(crate::backend::scaled::em_mul(1.5, em), 0.0, 0.0);
        self.prev_depth = depth;
        self.last_addvspace = crate::backend::scaled::em_mul(1.5, em);
        // `\@thanks` puts the title's footnotes after the skip: an insertion is the last item,
        // so a following `\addvspace` finds no `\lastskip`.
        let has_thanks = inlines_have_footnote(title)
            || author.is_some_and(inlines_have_footnote)
            || date.is_some_and(inlines_have_footnote);
        if has_thanks {
            for part in [Some(title), author, date].into_iter().flatten() {
                self.insert_thanks(part);
            }
            self.last_addvspace = 0.0;
            self.note_vmark();
        }
        // `\maketitle` is no heading: the next paragraph is indented.
        self.after_heading = false;
        self.has_preceding_text = false;
        self.last_line_width = 0.0;
        self.last_line_indent = 0.0;
        self.prev_graf = 0;
    }

    /// `merged` is `\addvspace` glue the previous block left, which the heading's own
    /// \addvspace{beforeskip} replaces when larger.
    pub(crate) fn layout_section<'doc>(&mut self, level: u8, starred: bool, numbered: bool, title: &'doc [Inline<'a>], merged: f32) {
        // article.cls \@startsection parameters: \section{-3.5ex plus -1ex minus -.2ex}{2.3ex plus .2ex}
        // in \Large, \subsection / \subsubsection {-3.25ex ...}{1.5ex plus .2ex} in \large /
        // \normalsize. The ex unit is that of the body font (cmr: 0.430554em).
        let ex = 0.430554 * self.base_font_size;
        let (large, large_bls, bigger, bigger_bls) = section_font_sizes(self.base_font_size, self.base_leading);
        let (skip_before, font_size, heading_leading, skip_after) = match level {
            1 => (crate::backend::scaled::em_mul(3.5, ex), large, large_bls, crate::backend::scaled::em_mul(2.3, ex)),
            2 => (crate::backend::scaled::em_mul(3.25, ex), bigger, bigger_bls, crate::backend::scaled::em_mul(1.5, ex)),
            _ => (crate::backend::scaled::em_mul(3.25, ex), self.base_font_size, self.base_leading, crate::backend::scaled::em_mul(1.5, ex)),
        };

        let (sec_number, num_w) = if numbered {
            let lvl = (level as usize).clamp(1, 4);
            self.section_counters[lvl - 1] += 1;
            self.section_counters[lvl..4].fill(0);
            let num = self.section_counters;
            let w = section_number_width(num, level, Face::Bold, font_size, self.font_family);
            (Some(num), w)
        } else {
            (None, 0.0)
        };

        let max_w = (self.right - self.right_skip) - (self.left + self.left_skip);
        self.cell_words_soa.clear();
        let mut section_math = Vec::new();
        let title_w = explode_inlines_into_family(
            title,
            Face::Bold,
            font_size,
            &mut self.cell_words_soa,
            self.font_family,
            &mut section_math,
        );
        let _ = title_w;
        // One path for every heading: the exploded words carry TeX's glue, kerns and the
        // text symbols (`\ `, `\_`, logos).
        self.cell_lines.clear();
        self.break_cell_words_into_lines(0, self.cell_words_soa.len(), (max_w - num_w).max(20.0), font_size, false, false);
        let title_lines = self.cell_lines.len().max(1);

        // Vertical model (TeX): `self.y` is where the next baseline would go at \baselineskip.
        // The heading's first baseline lies \addvspace{beforeskip} plus its own \baselineskip
        // below the previous baseline; at the top of a page the skip is discarded and the
        // baseline sits \topskip (or the heading's height) below the text top.
        // \@afterheading forbids a break after the heading and after the first line of the
        // next paragraph (\clubpenalty 10000), so the heading needs two body lines below it.
        // The page builder keeps the heading with two lines of the next paragraph:
        // \@afterheading's \nobreak and \clubpenalty 10000 leave no breakpoint before them.
        // The first line's height: its letters and the number's digits.
        let first_end = if title_lines == 1 {
            self.cell_words_soa.len()
        } else {
            let l = self.cell_lines[0];
            (l.start_word + l.word_count) as usize
        };
        let (title_h, _) = self.cell_words_soa.line_extent(0, first_end, font_size, self.font_family);
        let last_start = if title_lines == 1 { 0 } else { self.cell_lines[self.cell_lines.len() - 1].start_word as usize };
        let n_words = self.cell_words_soa.len();
        let title_d = self.cell_words_soa.line_extent(last_start, n_words, font_size, self.font_family).1;
        let digit_h = if sec_number.is_some() {
            crate::math::math_fonts::char_height_depth('1', Face::Bold, self.font_family).0 * font_size / 1000.0
        } else {
            0.0
        };
        let heading_ht = title_h.max(digit_h);
        if self.at_page_top() {
            self.y -= (heading_ht - self.top_skip).max(0.0);
        } else {
            // beforeskip: 3.5ex / 3.25ex plus 1ex minus .2ex; a smaller skip before it is
            // replaced (\addvspace). Right after a heading (`\if@nobreak`) `\@startsection`
            // adds neither it nor `\@secpenalty`.
            if !self.after_heading {
                if merged > 0.0 && merged < skip_before {
                    self.cancel_last_glue();
                }
                self.vglue((skip_before - merged).max(0.0), ex, crate::backend::scaled::em_mul(0.2, ex));
            }
            // The heading is a paragraph: `\parskip` follows the `\addvspace`.
            self.vglue(self.par_skip, self.par_skip_stretch, self.par_skip_shrink);
            // Interline glue at the heading's \baselineskip, or \lineskip when the previous
            // depth leaves too little room (tex.web §679).
            // After a rule (`\prevdepth` -1000pt) there is no interline glue at all.
            // `self.y` already moved by the last line's advance, which a size change
            // since then (`{\small ..}`) does not alter.
            let advanced = match std::mem::take(&mut self.block_entry_advance) {
                a if a > 0.0 => a,
                _ => self.base_leading,
            };
            if self.ignore_depth {
                self.y -= heading_ht - advanced;
            } else {
                let tight = (self.prev_depth + LINESKIP_PT + heading_ht - heading_leading).max(0.0);
                self.y -= heading_leading - advanced + tight;
            }
        }
        self.note_box(self.y, heading_ht);

        if !starred && self.current_sec_idx < self.registry.toc_entries.len() {
            self.registry.toc_entries[self.current_sec_idx].page = self.current_page_number();
            self.note_toc_entry(self.current_sec_idx);
            self.current_sec_idx += 1;
        }

        if let Some(num) = sec_number {
            if !self.is_dry_run {
                self.push_op(Paint::SectionNumber {
                    x: (self.left + self.left_skip).into(),
                    y: self.y,
                    face: Face::Bold,
                    size: font_size,
                    number: num,
                    level,
                });
            }
        }

        {
            let line_count = self.cell_lines.len();
            for line_idx in 0..line_count {
                let line_desc = self.cell_lines[line_idx];
                let line_x = self.left + self.left_skip + if line_idx == 0 { num_w } else { 0.0 };
                self.paint_cell_line_at(line_desc, line_x, self.y, font_size, &section_math);
                if line_idx + 1 < line_count {
                    self.y -= heading_leading;
                    self.note_box(self.y, heading_ht);
                }
            }
            self.y -= self.base_leading;
        }
        // The afterskip is glue on the page: 2.3ex / 1.5ex plus .2ex.
        self.vglue(skip_after, crate::backend::scaled::em_mul(0.2, ex), 0.0);
        // `\prevdepth` is the heading's last line's depth, for the interline glue below it.
        self.prev_depth = title_d;
        self.after_heading = true;
        self.has_preceding_text = false;
        self.last_line_width = 0.0;
        self.last_line_indent = 0.0;
        self.prev_graf = 0;
    }

    /// The footnotes whose marks are in the line `desc`, just added to the page.
    /// The `\vadjust` skips of a line, below it: `\vskip #1\vskip\z@skip`, so the last skip
    /// that a following `\addvspace` sees is zero.
    fn line_vadjusts(&mut self, desc: &LineDescriptor) {
        let (start, end) = (desc.start_word as u32, desc.start_word as u32 + desc.word_count as u32);
        let mut total = 0.0;
        for &(w, skip) in &self.words_soa.vadjusts {
            if (start..end).contains(&w) {
                total += skip;
            }
        }
        if total != 0.0 {
            self.vskip(total);
            self.last_addvspace = 0.0;
        }
    }

    fn insert_line_footnotes(&mut self, desc: &LineDescriptor, math_collector: &[&crate::math::MathNode<'a>]) {
        let start = desc.start_word as usize;
        for i in start..start + desc.word_count as usize {
            if let Some(&node) = self.words_soa.get_math_idx(i).and_then(|m| math_collector.get(m)) {
                self.insert_footnote_of(node as *const _ as usize);
            }
        }
    }

    /// The vertical list of a `\footnote` insertion: `\footnotesize`, `\hsize\columnwidth`,
    /// `\@makefntext` (`\noindent\hb@xt@1.8em{\hss\@makefnmark}` then the text) with
    /// `\rule\z@\footnotesep` in the first line and `\@finalstrut\strutbox` in the last.
    /// Set from y = 0 down; on return `self.y` is the box's bottom.
    pub(crate) fn layout_footnote_body<'doc>(&mut self, f: &'doc crate::frontend::ast::FootnoteData<'a>) {
        let (size, lead) = crate::frontend::ast::FontSize::FootnoteSize.metrics(self.class_font_size);
        let saved_skips = self.display_skips;
        self.display_skips = crate::frontend::ast::FontSize::FootnoteSize
            .display_skips(self.class_font_size)
            .unwrap_or(saved_skips);
        let saved = (
            self.base_font_size,
            self.base_leading,
            self.left,
            self.right,
            self.left_skip,
            self.right_skip,
            self.par_align,
            self.after_heading,
            self.has_preceding_text,
            self.last_line_width,
            self.last_line_indent,
            self.prev_graf,
        );
        self.base_font_size = size;
        self.base_leading = lead;
        (self.left, self.right) = self.text_span;
        self.left_skip = 0.0;
        self.right_skip = 0.0;
        self.par_align = crate::frontend::ast::ParAlign::Justified;
        self.after_heading = false;
        // The footnote is a vertical list of its own: no `\parskip` above its paragraph.
        self.has_preceding_text = false;
        let family = self.font_family;
        let quad = crate::backend::scaled::quad_pt(family, size);
        // A `\thanks` mark is `\rlap`ped in the text but not in `\@makefntext`.
        let mark = match &f.mark {
            crate::math::MathNode::DimensionBox { content, zero_width: true, .. } => &**content,
            m => m,
        };
        let mark_w = crate::math::measure_math(mark, size, family, crate::math::MathStyle::Text).w;
        let mut inls = Vec::with_capacity(f.text.len() + 3);
        inls.push(Inline::NoIndent);
        // `\hb@xt@1.8em{\hss\@makefnmark}`: the mark ends 1.8em from the left.
        let box_w = crate::backend::scaled::em_times(crate::backend::scaled::EM_1 + crate::backend::scaled::round_decimals(b"8"), sp64(quad));
        inls.push(Inline::Kern((box_w - sp64(mark_w)) as i32));
        inls.push(Inline::Math(Box::new(mark.clone())));
        inls.extend(f.text.iter().cloned());
        // The first line holds a strut `\footnotesep` (.7 of the footnote `\baselineskip`)
        // high and starts at the top of the box: no interline glue.
        self.y = Sp::pt(-lead);
        self.prev_depth = 0.0;
        self.ignore_depth = true;
        self.strut_height = 0.7 * lead;
        // The outer paragraph's entry advance is not this list's.
        let entry = std::mem::take(&mut self.block_entry_advance);
        let line_advance = std::mem::take(&mut self.last_line_advance);
        self.layout_para_full(&inls, Some(0.0), None);
        self.block_entry_advance = entry;
        self.last_line_advance = line_advance;
        let last = self.y + self.base_leading;
        self.box_depth = self.prev_depth.max(crate::backend::scaled::em_mul(0.3, lead));
        self.y = last - self.box_depth;
        (
            self.base_font_size,
            self.base_leading,
            self.left,
            self.right,
            self.left_skip,
            self.right_skip,
            self.par_align,
            self.after_heading,
            self.has_preceding_text,
            self.last_line_width,
            self.last_line_indent,
            self.prev_graf,
        ) = saved;
        self.display_skips = saved_skips;
    }

    fn layout_table<'doc>(&mut self, table_data: &'doc TableData<'a>) {
        crate::tables::layout_table(self, table_data);
    }

    /// A `tabular` outside a float is a box in a paragraph of its own: indented by
    /// `\parindent` (none right after a heading), vertically centred on the math axis
    /// (`[c]`), and placed by interline glue like any line, so a tall one sits `\lineskip`
    /// below the previous line's depth.
    fn layout_inline_tabular<'doc>(&mut self, table_data: &'doc TableData<'a>) {
        let indent = if self.after_heading { 0.0 } else { self.par_indent };
        let saved_skip = self.left_skip;
        self.left_skip += indent;
        let ops_start = self.current_page.ops.len();
        let y0 = self.y;
        crate::tables::layout_table(self, table_data);
        let total = (y0 - self.y).to_pt();
        self.left_skip = saved_skip;
        // cmsy's axis height, a quarter of the em at every size.
        let axis = 0.25 * self.base_font_size;
        let (h, d) = (0.5 * total + axis, (0.5 * total - axis).max(0.0));
        self.y = y0;
        self.place_baseline(h);
        let baseline = self.y;
        let dy = baseline + h - y0;
        for op in &mut self.current_page.ops[ops_start..] {
            op.shift_y(dy);
        }
        self.prev_depth = d;
        self.y = baseline - self.base_leading;
        self.after_heading = false;
    }

    /// The box of a `figure` float (`\@float`), set from its top edge at `self.y`; on
    /// return `self.y` is its bottom edge. Inside the float `\parindent` is 0pt, so the
    /// graphic starts at the left margin unless `\centering` is in force; the caption
    /// follows by `\@makecaption`. rushtex does not read image files: a graphic is
    /// graphicx's draft box (its frame and the file name in `\ttfamily`), 1in square as for
    /// a missing file unless `width`, `height` or `scale` give its size.
    fn layout_figure<'doc>(&mut self, graphic: Option<Graphic<'a>>, caption: &'doc [Inline<'a>], centered: bool, vskip: f32) {
        self.figure_counter += 1;
        if self.current_fig_idx < self.registry.figure_entries.len() {
            self.registry.figure_entries[self.current_fig_idx].page = self.current_page_number();
            self.current_fig_idx += 1;
        }
        let hsize = (self.right - self.right_skip) - (self.left + self.left_skip);
        let mut prev_depth = None;
        if let Some(g) = graphic {
            const RULE: f32 = 0.4;
            let (w, h) = graphic_box_size(g.options, hsize);
            let x0 = self.left + self.left_skip + if centered { (hsize - w).max(0.0) * 0.5 } else { 0.0 };
            let top = self.y;
            let bottom = top - h;
            if !self.is_dry_run {
                for (x, y, rw, rh) in [
                    (x0, bottom, RULE, h),
                    (x0 + w - RULE, bottom, RULE, h),
                    (x0, top - RULE, w, RULE),
                    (x0, bottom, w, RULE),
                ] {
                    self.push_op(Paint::Rule { x: x.into(), y, w: rw.into(), h: rh.into(), fill: true });
                }
                if !g.path.is_empty() {
                    // `\rlap{ \ttfamily name}` centred by `\vss` between the two rules; the
                    // space before `\ttfamily` is in the surrounding roman font.
                    let size = self.base_font_size;
                    let (mut th, mut td) = (0.0f32, 0.0f32);
                    for ch in g.path.chars() {
                        let (ch_h, ch_d) = crate::math::math_fonts::char_height_depth(ch, Face::Mono, self.font_family);
                        th = th.max(ch_h);
                        td = td.max(ch_d);
                    }
                    let (th, td) = (th * size / 1000.0, td * size / 1000.0);
                    let baseline = top - RULE - (h - 2.0 * RULE - th - td) * 0.5 - th;
                    let space = crate::font_metrics::resolve_space_units(self.font_family, Face::Roman) as f32 * size * 0.001;
                    let pool_start = self.current_page.text_pool.len() as u32;
                    self.current_page.text_pool.push(g.path);
                    self.push_op(Paint::GlyphRun {
                        x: (x0 + space).into(),
                        y: baseline,
                        face: Face::Mono,
                        size,
                        part_start: pool_start,
                        part_count: 1,
                        word_spacing: 0.0,
                    });
                }
            }
            // The graphic's line: its baseline at the box bottom, depth 0.
            self.y = bottom;
            prev_depth = Some(0.0);
        }
        // Glue in the float's vbox: kept, even at its top.
        self.y -= vskip;
        if !caption.is_empty() {
            let top = self.y;
            let d = crate::tables::tables_layout::render_caption(self, caption, top, prev_depth, true);
            self.y -= d;
        }
        self.after_heading = false;
    }

    fn layout_equation<'doc>(&mut self, starred: bool, math: &'doc crate::math::MathNode<'a>) {
        let (actual_math, custom_tag, is_left_tag, tag_starred) = match math {
            crate::math::MathNode::Tagged { formula, tag, is_left, starred: t_star } => {
                (&**formula, Some(&**tag), *is_left, *t_star)
            }
            other => (other, None, false, false),
        };

        let available_w = (self.right - self.right_skip) - (self.left + self.left_skip);
        let m_box = crate::math::measure_display(
            actual_math,
            self.base_font_size,
            self.font_family,
            crate::math::MathStyle::Display,
            self.body_face,
            available_w,
            self.equation_counter + 1,
        );

        // A numbered `align`/`gather` numbers its rows instead of the whole display.
        let rows = if custom_tag.is_none() {
            crate::math::display_rows(actual_math, self.base_font_size, self.font_family)
        } else {
            None
        };
        let row_numbers = rows.as_ref().is_some_and(|r| r.numbered.iter().any(|&n| n));

        let tag_w = if row_numbers {
            0.0
        } else if let Some(tag_node) = custom_tag {
            let t_box = crate::math::measure_math(
                tag_node,
                self.base_font_size,
                self.font_family,
                crate::math::MathStyle::Text,
            );
            let pw = if !tag_starred {
                metrics::width_family('(', Face::Roman, self.base_font_size, self.font_family)
                    + metrics::width_family(')', Face::Roman, self.base_font_size, self.font_family)
            } else {
                0.0
            };
            t_box.w + pw
        } else if !starred {
            self.equation_counter += 1;
            equation_number_width(self.equation_counter, Face::Roman, self.base_font_size, self.font_family)
        } else {
            0.0
        };

        let has_any_tag = custom_tag.is_some() || (!starred && !row_numbers && tag_w > 0.0);
        // A formula too wide for its number beside it is packed to the width that leaves
        // room, if its glue can shrink that far (tex.web §1201).
        let mut eq_w = m_box.w;
        let mut med_shrink = 0.0;
        if has_any_tag && rows.is_none() {
            let q = tag_w + self.base_font_size;
            let excess = m_box.w + q - available_w;
            if excess > 0.0 {
                let (shrink, count) =
                    crate::math::top_level_shrink(actual_math, self.base_font_size, self.font_family, crate::math::MathStyle::Display);
                if count > 0 && excess <= shrink {
                    eq_w = available_w - q;
                    med_shrink = excess / count as f32;
                }
            }
        }
        let disp_geom = crate::math::DisplayGeometryEngine::compute_display_geometry(
            available_w,
            self.hang_indent,
            self.hang_after,
            self.prev_graf,
            self.last_line_width,
            self.last_line_indent,
            self.has_preceding_text,
            eq_w,
            tag_w,
            is_left_tag,
            has_any_tag,
            self.base_font_size,
            crate::backend::scaled::face_quad_pt(self.font_family, self.body_face, self.base_font_size),
        );
        let crate::math::PackedDisplayGeometry {
            display_indent,
            displacement,
            must_split_tag,
            use_short_skip,
            ..
        } = disp_geom;
        // A display alignment always takes the regular skips (tex.web §1206); amsmath sets a
        // display that is one `split` as an alignment too.
        let alignment = rows.is_some() || crate::math::is_split_display(actual_math);
        let [ab, ab_st, ab_sh, abs_st, bes, bes_st, bes_sh] = self.display_skips;
        // (width, stretch, shrink) above and below.
        let (above, below) = if use_short_skip && !alignment {
            ((0.0, abs_st, 0.0), (bes, bes_st, bes_sh))
        } else {
            ((ab, ab_st, ab_sh), (ab, ab_st, ab_sh))
        };

        // \abovedisplayskip, then the interline glue to the display's first line (tex.web
        // §1199–§1205); an alignment's rows are lines of the vertical list. The display is
        // unbreakable, so it moves to the next page whole when its last baseline would not fit.
        let (first_h, first_off, span, last_d) = match &rows {
            Some(r) => (r.first_h, r.outer_first, r.outer_first - r.outer_last, r.last_d),
            None => (m_box.h, 0.0, 0.0, m_box.d),
        };
        let em = self.base_font_size;
        self.vglue(above.0, above.1, above.2);
        // amsmath's alignments run `\openup\jot` (`\spread@equation`), which also widens
        // the interline glue above their first row by `\jot`, unless the `\everycr` of
        // `\@display@init` takes it back with `\vskip-\lineskiplimit` there: it does at the
        // first `\cr` it sees, which is the real alignment's in `align` and `gather`, but a
        // measuring pass's in `multline`, or the inner alignment's for a `split`.
        let first_row_jot = (rows.is_some() && crate::math::multline_rows(actual_math).is_some())
            || crate::math::is_split_display(actual_math);
        if first_row_jot && !self.at_page_top() && !self.ignore_depth {
            self.y -= JOT_PT;
        }
        self.place_baseline(first_h);
        let baseline_y = self.y - first_off;

        // A number that does not fit beside the display goes on its own line below it, after
        // \penalty10000 and the usual interline glue from the display's depth; no
        // \belowdisplayskip follows (tex.web §1205).
        let (num_h, num_d) = {
            let (h, d) = crate::math::math_fonts::char_height_depth('(', Face::Roman, self.font_family);
            (h * em / 1000.0, d * em / 1000.0)
        };
        // amsmath sets an `equation` holding a `split` as a `\vcenter`ed `gather`, whose
        // `\place@tag` hangs a number that does not fit below the last row inside the box:
        // `\vtop{\normalbaselines\null\boxz@}`, the `\null` as deep as the row and the
        // number with its strut (amsmath `\gather@split`).
        let ams_split_tag = crate::math::is_split_display(actual_math) && rows.is_none();
        let (num_h, num_d) = if ams_split_tag {
            (num_h.max(crate::backend::scaled::em_mul(0.7, self.base_leading)), num_d.max(crate::backend::scaled::em_mul(0.3, self.base_leading)))
        } else {
            (num_h, num_d)
        };
        let split_tag_y = {
            let d = if ams_split_tag { m_box.d.max(crate::backend::scaled::em_mul(0.3, self.base_leading)) } else { m_box.d };
            let gap = self.base_leading - d - num_h;
            if ams_split_tag {
                baseline_y - (d + if gap >= 0.0 { gap } else { LINESKIP_PT } + num_h)
            } else {
                baseline_y - if gap >= 0.0 { self.base_leading } else { m_box.d + LINESKIP_PT + num_h }
            }
        };
        let eq_x = self.left + self.left_skip + display_indent + displacement;
        let right_margin = self.right - self.right_skip;
        let tag_x = if is_left_tag {
            self.left + self.left_skip + display_indent
        } else {
            right_margin - tag_w
        };

        let multline = crate::math::multline_rows(actual_math).filter(|_| rows.is_some());
        let must_split_tag = must_split_tag && multline.is_none();
        let mut tag_x = tag_x;
        let mut tag_row_y = None;
        if let (Some(lines), Some(r), false) = (multline, &rows, self.is_dry_run) {
            // amsmath `\multline@`: every line is `\hbox to\displaywidth`; the first starts
            // `\multlinegap` from the left, the last ends `\multlinegap` from the right, or
            // `\multlinetaggap` before a right number, and the others are centred.
            let gap = crate::math::multline_gap(actual_math);
            let line_left = self.left + self.left_skip + display_indent;
            let n = lines.len();
            for (i, (line, &row_y)) in lines.iter().zip(&r.baselines).enumerate() {
                let w: f32 = line
                    .iter()
                    .map(|c| {
                        crate::math::measure_math_in(c, self.base_font_size, self.font_family, crate::math::MathStyle::Display, self.body_face).w
                    })
                    .sum();
                let x = if i == 0 && (n > 1 || is_left_tag || !has_any_tag) {
                    line_left + if is_left_tag && has_any_tag { tag_w + MULTLINE_TAG_GAP_PT } else { gap }
                } else if i + 1 == n {
                    right_margin - w - if has_any_tag && !is_left_tag { tag_w + MULTLINE_TAG_GAP_PT } else { gap }
                } else {
                    line_left + (available_w - w) * 0.5
                };
                let mut cx = x;
                for cell in line {
                    let b = crate::math::render_math_in(
                        cell,
                        self.base_font_size,
                        self.font_family,
                        crate::math::MathStyle::Display,
                        Sp::pt(cx),
                        baseline_y + row_y,
                        &mut self.current_page.ops,
                        &mut self.current_page.text_pool,
                        self.body_face,
                    );
                    cx += b.w;
                }
            }
            if is_left_tag {
                tag_x = line_left;
                tag_row_y = Some(baseline_y + r.baselines[0]);
            } else {
                tag_row_y = r.baselines.last().map(|&y| baseline_y + y);
            }
        }
        let mut row_ends = Vec::new();
        let mut row_tags: Vec<(usize, usize, usize)> = Vec::new();
        if !self.is_dry_run {
            if multline.is_none() {
                crate::math::render_math_shrunk(
                actual_math,
                self.base_font_size,
                self.font_family,
                crate::math::MathStyle::Display,
                Sp::pt(eq_x),
                baseline_y,
                &mut self.current_page.ops,
                &mut self.current_page.text_pool,
                med_shrink,
                self.body_face,
                available_w,
                self.equation_counter + 1,
                &mut row_ends,
            );
            }
            self.current_page.used_faces |= (1 << (Face::Symbol as u8))
                | (1 << (Face::Roman as u8))
                | (1 << (Face::Italic as u8));

            // A `split`'s number sits on the baseline of the row that holds it, which the
            // `-\jot` above that row raises by half a `\jot` in the centred display.
            let split_raise = if crate::math::is_split_display(actual_math) && !must_split_tag { 0.5 * JOT_PT } else { 0.0 };
            let tag_y = tag_row_y.unwrap_or(if must_split_tag { split_tag_y } else { baseline_y + split_raise });

            if let Some(tag_node) = custom_tag {
                let render_tag_x = if !tag_starred {
                    let open_paren_w = metrics::width_family('(', Face::Roman, self.base_font_size, self.font_family);
                    let start_p = self.current_page.text_pool.len() as u32;
                    self.current_page.text_pool.push("(");
                    self.push_op(Paint::GlyphRun {
                        x: tag_x.into(),
                        y: tag_y,
                        face: Face::Roman,
                        size: self.base_font_size,
                        part_start: start_p,
                        part_count: 1,
                        word_spacing: 0.0,
                    });
                    tag_x + open_paren_w
                } else {
                    tag_x
                };

                let t_box = crate::math::render_math(
                    tag_node,
                    self.base_font_size,
                    self.font_family,
                    crate::math::MathStyle::Text,
                    Sp::pt(render_tag_x),
                    tag_y,
                    &mut self.current_page.ops,
                    &mut self.current_page.text_pool,
                );

                if !tag_starred {
                    let close_p = self.current_page.text_pool.len() as u32;
                    self.current_page.text_pool.push(")");
                    self.push_op(Paint::GlyphRun {
                        x: (render_tag_x + t_box.w).into(),
                        y: tag_y,
                        face: Face::Roman,
                        size: self.base_font_size,
                        part_start: close_p,
                        part_count: 1,
                        word_spacing: 0.0,
                    });
                }
            } else if row_numbers {
                if let Some(r) = &rows {
                    let tags = crate::math::display_row_tags(actual_math);
                    for (i, (&row_y, _)) in r.baselines.iter().zip(&r.numbered).enumerate().filter(|(_, (_, &n))| n) {
                        let tag_start = self.current_page.ops.len();
                        if let Some(&Some((tag, tag_starred))) = tags.get(i) {
                            // amsmath `\tag`: `\tagform@`, right-aligned like a number.
                            self.paint_row_tag(tag, tag_starred, right_margin, baseline_y + row_y);
                            row_tags.push((i, tag_start, self.current_page.ops.len()));
                            continue;
                        }
                        self.equation_counter += 1;
                        let n = self.equation_counter;
                        let w = equation_number_width(n, Face::Roman, self.base_font_size, self.font_family);
                        self.push_op(Paint::EquationNumber {
                            x: (right_margin - w).into(),
                            y: (baseline_y + row_y),
                            face: Face::Roman,
                            size: self.base_font_size,
                            n,
                        });
                        row_tags.push((i, tag_start, self.current_page.ops.len()));
                    }
                    self.order_display_rows(&row_ends, &row_tags);
                }
            } else if !starred {
                self.push_op(Paint::EquationNumber {
                    x: tag_x.into(),
                    y: tag_y,
                    face: Face::Roman,
                    size: self.base_font_size,
                    n: self.equation_counter,
                });
            }
        } else if row_numbers {
            if let Some(r) = &rows {
                let tags = crate::math::display_row_tags(actual_math);
                self.equation_counter +=
                    r.numbered.iter().enumerate().filter(|&(i, &n)| n && !matches!(tags.get(i), Some(Some(_)))).count() as u32;
            }
        }

        // \belowdisplayskip after the last line; the next line's interline glue starts
        // from that line's depth.
        let split = must_split_tag && !is_left_tag && has_any_tag;
        let (last_baseline, depth) = if split && ams_split_tag {
            // The `\vcenter`ed box's depth: half its height and depth less the axis height.
            let bottom = split_tag_y - num_d;
            let axis = crate::math::math_fonts::math_font_parameters_styled(self.font_family, crate::math::MathStyle::Text)
                .axis_height as f32
                * em
                / 1000.0;
            let depth = 0.5 * (baseline_y + m_box.h - bottom).to_pt() - axis;
            self.note_box(split_tag_y, num_h);
            (bottom + depth, depth)
        } else if split {
            self.note_box(split_tag_y, num_h);
            (split_tag_y, num_d)
        } else {
            (baseline_y + first_off - span, last_d)
        };
        self.page_builder.last_baseline = last_baseline;
        self.y = last_baseline - self.base_leading;
        // The next line's interline glue is its own paragraph's \baselineskip.
        self.last_line_advance = self.base_leading;
        // \postdisplaypenalty (0), then the skip. An amsmath alignment ends every row with
        // `\penalty10000` and a zero `\vskip`, so the last row's whole depth is on the page
        // at the penalty (tex.web §1004).
        self.prev_depth = depth;
        if rows.is_some() {
            self.note_glue(0.0);
        }
        self.page_breakpoint(0);
        if !split || ams_split_tag {
            self.vglue(below.0, below.1, below.2);
            // A heading right after the display replaces this skip with its own (\addvspace).
            self.last_addvspace = below.0;
        }
        self.page_builder.after_display = true;
        self.prev_depth = depth;

        let mut space_factor = 1000u16;
        crate::math::DisplayResumeStateRestorer::restore_state(&mut self.prev_graf, &mut space_factor);
        self.has_preceding_text = false;
        self.after_heading = false;
    }

    /// A `verbatim` or `lstlisting` display. Each source line is a box set at
    /// `\baselineskip` with a legal page break after it, as in TeX.
    ///
    /// `verbatim` is LaTeX's `\@verbatim`: a trivlist (`\topsep`, plus `\partopsep` after
    /// a blank line, `\addvspace`d above and below) of lines in the typewriter face, with
    /// obeyed spaces one character wide. `lstlisting` follows the `listings` defaults:
    /// `\penalty-50` and `\vspace\medskipamount` above and below, the document font in
    /// `columns=fixed` (every token of n characters in a box n × 0.6em wide, its characters
    /// spread by `\hss`), trailing blank lines dropped, and `numbers=left` in an `\llap`
    /// `numbersep` (10pt) left of the text.
    fn layout_code_listing<'doc>(&mut self, listing: &'doc crate::code_listing::CodeListingData<'a>, added_vspace: f32) {
        let opts = &listing.options;
        let size = if opts.font_size > 0.0 { opts.font_size } else { self.base_font_size };
        // A size declaration inside `verbatim` (ltxguide's `\small`) also sets its
        // `\baselineskip`.
        let saved_leading = self.base_leading;
        if opts.verbatim && opts.font_size > 0.0 {
            self.base_leading = self.leading_for_size(size);
        }
        // `self.y` moved by the previous line's advance; the first line's interline glue is
        // this `\baselineskip`'s.
        let entry = std::mem::take(&mut self.block_entry_advance);
        if entry > 0.0 && (entry - self.base_leading).abs() > 1e-4 && !self.at_page_top() {
            self.y -= self.base_leading - entry;
        }
        let face = if opts.mono { Face::Mono } else { Face::Roman };
        let family = self.font_family;
        let cm = opts.mono && family == metrics::FontFamily::ComputerModern;
        let mut lines: Vec<&'a str> = listing.lines().collect();
        if !opts.verbatim {
            while lines.last().is_some_and(|l| l.trim().is_empty()) {
                lines.pop();
            }
        }
        // `\medskipamount` is 6pt plus 2pt minus 2pt at every class size.
        const MED_SKIP: (f32, f32, f32) = (6.0, 2.0, 2.0);
        let above = if opts.verbatim {
            let (top, partop) = class_topsep(self.base_font_size);
            let sep = topsep_add(top, partop, listing.in_vmode);
            // Right after a heading (`\@nobreak`) the first `\item` is `\@nbitem`: no `\topsep`.
            // Otherwise `\@item` does `\addvspace\@topsep` (`\@topsepadd` plus `\parskip`),
            // then `\addvspace{-\parskip}`; the item's paragraph adds its `\parskip`.
            let ps = self.item_par_skip.unwrap_or((self.par_skip, self.par_skip_stretch, self.par_skip_shrink));
            // At the top of a page the page builder discards both skips.
            if !self.after_heading && !self.at_page_top() {
                let last = self.addvspace(added_vspace, self.trivlist_topsep(sep));
                if last > 0.0 && ps.0 > 0.0 {
                    self.endtrivlist_skip(last, (-ps.0, -ps.1, -ps.2));
                }
            }
            self.vglue(ps.0, ps.1, ps.2);
            sep
        } else {
            self.vglue(MED_SKIP.0, MED_SKIP.1, MED_SKIP.2);
            MED_SKIP
        };

        let col_w = if opts.verbatim {
            crate::backend::shape::word_width_pt("m", face, size, family)
        } else {
            0.6 * crate::backend::scaled::quad_pt(family, size)
        };
        let left = self.left + self.left_skip;
        let frame_top = self.y;
        let mut cols: Vec<(usize, &'a str)> = Vec::new();
        for (line_idx, &line) in lines.iter().enumerate() {
            if line_idx > 0 {
                // `\penalty\interlinepenalty` (0) after every line.
                self.page_breakpoint(0);
            }
            // Columns: a tab advances to the next multiple of `tabsize` in a listing; in
            // `verbatim` it is an ordinary space, one column wide.
            cols.clear();
            let mut col = 0usize;
            let mut run: Option<(usize, usize)> = None;
            let tab = if opts.verbatim { 1 } else { opts.tab_spaces.max(1) as usize };
            for (bi, ch) in line.char_indices() {
                if ch == ' ' || ch == '\t' {
                    if let Some((rs, rc)) = run.take() {
                        cols.push((rc, &line[rs..bi]));
                    }
                    col += if ch == '\t' { tab - col % tab } else { 1 };
                    continue;
                }
                if run.is_none() {
                    run = Some((bi, col));
                }
                col += 1;
            }
            if let Some((rs, rc)) = run.take() {
                cols.push((rc, &line[rs..]));
            }
            let (mut h, mut d) = (0.0f32, 0.0f32);
            for &(_, text) in &cols {
                for ch in text.chars() {
                    // Verbatim characters are typewriter codes (`\verbatim@font`), set one by one.
                    if let Some(g) = (cm && ch.is_ascii()).then(|| crate::math::math_tfm::glyph_dims(face, ch as u8, size, family)).flatten() {
                        h = h.max(g.h);
                        d = d.max(g.d);
                        continue;
                    }
                    let (ch_h, ch_d) = crate::math::math_fonts::char_height_depth(ch, face, family);
                    h = h.max(ch_h * size * 0.001);
                    d = d.max(ch_d * size * 0.001);
                }
            }
            if !opts.verbatim {
                // Every listings line holds a `\strut` (.7 and .3 of `\baselineskip`).
                let em = crate::backend::scaled::em_mul;
                h = h.max(em(0.7, self.base_leading));
                d = d.max(em(0.3, self.base_leading));
            }
            self.place_baseline(h);
            if !self.is_dry_run {
                let y = self.y;
                if opts.show_line_numbers {
                    let n = line_idx as u32 + 1;
                    let digits = n.to_string();
                    let w = crate::backend::shape::word_width_pt(&digits, Face::Roman, self.base_font_size, family);
                    self.push_op(Paint::PageNumber {
                        x: (left - 10.0 - w).into(),
                        y,
                        face: Face::Roman,
                        size: self.base_font_size,
                        page_number: n,
                    });
                }
                for &(c0, text) in &cols {
                    if opts.verbatim {
                        self.push_listing_text(left + c0 as f32 * col_w, y, face, size, text);
                    } else {
                        self.push_fixed_tokens(left + c0 as f32 * col_w, y, face, size, col_w, text);
                    }
                }
            }
            self.prev_depth = d;
            self.y -= self.base_leading;
        }
        // Like a paragraph, the last line advanced `y` by \baselineskip: a paragraph after
        // it with another \baselineskip corrects the advance.
        if !lines.is_empty() {
            self.last_line_advance = self.base_leading;
        }
        if opts.frame && !self.is_dry_run && !lines.is_empty() {
            // `frame=single`: \framerule .4pt at \framesep 3pt around the lines.
            let bottom = self.y + self.base_leading - self.prev_depth - 3.0;
            let top = frame_top - (self.base_leading - 0.7 * size) + 3.0;
            let x = left - 3.4;
            let w = (self.right - self.right_skip) - left + 6.8;
            self.push_op(Paint::Rule { x: x.into(), y: bottom, w: w.into(), h: (top - bottom), fill: false });
        }
        // `\@endparenv` after the trivlist, or `listings`' `\vspace\lst@belowskip`
        // followed by its `\penalty-50`, which leaves `\lastskip` 0: a heading after it
        // adds its whole skip.
        if opts.verbatim {
            // verbatim's `\par` ends the last line with `\penalty\interlinepenalty`; with
            // `\lastskip` 0, `\@endparenv`'s `\addpenalty\@endparpenalty` is a plain penalty.
            self.page_breakpoint(0);
            self.page_breakpoint(-51);
            self.vglue(above.0, above.1, above.2);
            self.last_addvspace = above.0;
        } else {
            self.page_breakpoint(-50);
            self.vglue(MED_SKIP.0, MED_SKIP.1, MED_SKIP.2);
            self.last_addvspace = 0.0;
        }
        self.base_leading = saved_leading;
        self.after_heading = false;
    }

    /// One run of characters at `x`, emitted as it is (no ligatures or kerns).
    fn push_listing_text(&mut self, x: f32, y: Sp, face: Face, size: f32, text: &'a str) {
        let pool_start = self.current_page.text_pool.len() as u32;
        self.current_page.text_pool.push(text);
        self.push_op(Paint::GlyphRun { x: x.into(), y, face, size, part_start: pool_start, part_count: 1, word_spacing: 0.0 });
    }

    /// `listings`' `columns=[c]fixed`: letters (with the digits after them) form one token
    /// and other characters another; a token of n characters is a box n × `col_w` wide whose
    /// characters are separated, and flanked, by equal `\hss` glue.
    fn push_fixed_tokens(&mut self, x0: f32, y: Sp, face: Face, size: f32, col_w: f32, text: &'a str) {
        let family = self.font_family;
        let is_letter = |c: char| c.is_alphabetic() || c == '_' || c == '@' || c == '$';
        let mut x = x0;
        let mut rest = text;
        let mut glyphs: Vec<ListingGlyph<'a>> = Vec::new();
        while let Some(first) = rest.chars().next() {
            let letter = is_letter(first);
            let end = rest
                .char_indices()
                .skip(1)
                .find(|&(_, c)| (is_letter(c) || (c.is_ascii_digit() && letter)) != letter)
                .map_or(rest.len(), |(i, _)| i);
            let token = &rest[..end];
            glyphs.clear();
            glyphs.extend(token.char_indices().map(|(i, c)| listing_glyph(&token[i..i + c.len_utf8()], face, family, size)));
            let n = glyphs.len();
            let natural: f32 = glyphs.iter().map(|g| g.width).sum();
            let gap = (n as f32 * col_w - natural) / (n as f32 + 1.0);
            let mut cx = x + gap;
            for g in &glyphs {
                match g.text {
                    Some(t) => self.push_listing_text(cx, y, g.face, size, t),
                    // OT1's `\textunderscore`: `\kern.06em\vbox{\hrule\@width.3em}`.
                    None if !self.is_dry_run => {
                        let em = crate::backend::scaled::quad_pt(family, size);
                        self.push_op(Paint::Rule { x: (cx + crate::backend::scaled::em_mul(0.06, em)).into(), y, w: crate::backend::scaled::em_mul(0.3, em).into(), h: Sp::pt(0.4), fill: true });
                    }
                    None => {}
                }
                cx += g.width + gap;
            }
            x += n as f32 * col_w;
            rest = &rest[end..];
        }
    }

    fn layout_para<'doc>(&mut self, inls: &'doc [Inline<'a>]) {
        if !inlines_have_par_end(inls) {
            self.layout_para_full(inls, None, None);
            return;
        }
        // `\par` inside the paragraph (possibly inside a group) ends it there: each piece is
        // a paragraph of its own, with the declarations of the groups open at its end.
        let pieces = split_at_par_end(inls, None, false);
        let mut first = true;
        for piece in &pieces {
            if is_blank_inlines(&piece.inls) {
                continue;
            }
            if !first {
                self.vskip(self.par_skip);
            }
            first = false;
            let saved = (self.par_align, self.par_indent);
            if let Some(a) = piece.end_align {
                self.par_align = a;
                self.par_indent = 0.0;
            }
            if let Some(fs) = piece.end_size {
                self.leading_override = Some(fs.metrics(self.class_font_size).1);
            }
            let indent = if piece.start_align.is_some() && leading_group_has_text(&piece.inls) { Some(0.0) } else { None };
            self.layout_para_full(&piece.inls, indent, None);
            if piece.end_align.is_some() {
                (self.par_align, self.par_indent) = saved;
            }
        }
    }

    /// `\baselineskip` that goes with text of `size` pt: the size declaration's own, or
    /// 1.2 times the size for sizes no declaration selects.
    fn leading_for_size(&self, size: f32) -> f32 {
        crate::frontend::ast::FontSize::ALL
            .iter()
            .map(|fs| fs.metrics(self.class_font_size))
            .find(|(s, _)| (s - size).abs() < 0.005)
            .map_or(size * 1.2, |(_, l)| l)
    }

    fn layout_para_full<'doc>(
        &mut self,
        inls: &'doc [Inline<'a>],
        custom_first_line_indent: Option<f32>,
        custom_hanging_indent: Option<f32>,
    ) {
        let par_leading = self.base_leading;
        let par_size = self.base_font_size;
        self.par_end_size = None;
        self.layout_para_full_inner(inls, custom_first_line_indent, custom_hanging_indent);
        // A size declared at the paragraph's top level holds after it (until its group ends).
        match self.par_end_size.take() {
            Some(end) if (end - par_size).abs() > 1e-4 => {
                self.base_font_size = end;
                self.base_leading = self.leading_for_size(end);
                let class = self.class_font_size;
                if let Some(s) = crate::frontend::ast::FontSize::ALL
                    .iter()
                    .find(|fs| (fs.metrics(class).0 - end).abs() < 0.005)
                    .and_then(|fs| fs.display_skips(class))
                {
                    self.display_skips = s;
                }
            }
            _ => self.base_leading = par_leading,
        }
    }

    fn layout_para_full_inner<'doc>(
        &mut self,
        inls: &'doc [Inline<'a>],
        custom_first_line_indent: Option<f32>,
        custom_hanging_indent: Option<f32>,
    ) {
        if inls.is_empty() {
            return;
        }
        if inlines_have_footnote(inls) {
            self.set_footnotes(inls);
        }
        let follows_heading = self.after_heading;

        let mut is_noindent = starts_with_align(inls);
        let mut is_indent = false;
        let mut inline_align = None;
        for inl in inls {
            match inl {
                Inline::Align(a) => inline_align = Some(*a),
                Inline::NoIndent => is_noindent = true,
                Inline::Indent => is_indent = true,
                Inline::HangIndent(dim) => self.hang_indent = *dim,
                Inline::HangAfter(n) => self.hang_after = *n,
                Inline::ParShape(shape) => {
                    self.par_shape.clear();
                    self.par_shape.extend_from_slice(shape);
                }
                Inline::PreTolerance(v) => self.pre_tolerance = *v,
                Inline::Tolerance(v) => self.tolerance = *v,
                Inline::EmergencyStretch(v) => self.emergency_stretch = *v,
                Inline::Sloppy => {
                    self.tolerance = 9999;
                    self.emergency_stretch = 33.0;
                }
                Inline::Fussy => {
                    self.tolerance = 200;
                    self.emergency_stretch = 0.0;
                }
                Inline::Looseness(v) => self.looseness = *v,
                Inline::LeftSkip(v) => self.left_skip = *v,
                Inline::RightSkip(v) => self.right_skip = *v,
                Inline::ParFillSkip { width, stretch } => {
                    self.par_fill_width = *width;
                    self.par_fill_stretch = *stretch;
                }
                Inline::PrevGraf(v) => self.prev_graf = *v,
                Inline::LastLineFit(v) => self.last_line_fit = *v,
                _ => {}
            }
        }

        self.words_soa.clear();
        let est_words: usize = inls.iter().map(|inl| match inl {
            Inline::Text(s) => s.len() / 5,
            _ => 2,
        }).sum();
        self.words_soa.reserve(est_words);
        self.words_soa.set_color(self.text_color);
        self.words_soa.class_size = self.class_font_size;
        let mut math_collector = Vec::new();
        let mut space_factor = 1000u16;
        let t_exp_start = if self.debug_timings { Some(std::time::Instant::now()) } else { None };
        explode_inlines_into_family_sf(
            inls,
            self.body_face,
            self.base_font_size,
            &mut self.words_soa,
            self.font_family,
            &mut math_collector,
            &mut space_factor,
            u8::from(self.french_spacing) * SF_FRENCH + u8::from(self.period_sf_1000) * SF_PERIOD_1000,
        );
        if let Some(t) = t_exp_start {
            self.time_explode_ns += t.elapsed().as_nanos() as u64;
        }
        // A `\color` at the paragraph's top level holds until its group ends, past the
        // paragraph.
        self.text_color = self.words_soa.current_color();
        // TeX sets the whole paragraph with the \baselineskip in force at its \par.
        let end_size = self.words_soa.current_size(self.base_font_size);
        self.par_end_size = Some(end_size);
        if let Some(leading) = self.leading_override.take() {
            self.base_leading = leading;
        } else if (end_size - self.base_font_size).abs() > 1e-4 {
            self.base_leading = self.leading_for_size(end_size);
        }
        // The interline glue above the first line is this paragraph's \baselineskip, not
        // the advance the previous line already made.
        let entry = std::mem::take(&mut self.block_entry_advance);
        if entry > 0.0 && (entry - self.base_leading).abs() > 1e-4 && !self.at_page_top() {
            self.y -= self.base_leading - entry;
        }
        if self.words_soa.is_empty() {
            return;
        }

        // After a heading LaTeX's `\@afterheading` removes the indent box with `\lastbox`
        // when the paragraph starts, the one `\indent` makes too.
        let has_first_line_indent = if is_indent && !self.after_heading {
            true
        } else if self.after_heading || is_noindent {
            self.after_heading = false;
            false
        } else {
            self.par_indent > 0.0
        };

        let base_first_indent = if has_first_line_indent {
            self.par_indent
        } else {
            0.0
        };
        let first_line_indent = custom_first_line_indent.unwrap_or(base_first_indent);
        let custom_hang = custom_hanging_indent.unwrap_or(self.hang_indent);
        // The paragraph is already indented when a declaration inside it is reached; the
        // alignment holds from here on, and so does its \parindent of 0pt.
        if let Some(a) = inline_align {
            self.par_align = a;
            self.par_indent = 0.0;
        }

        let measure = (self.right - self.right_skip) - (self.left + self.left_skip);
        self.line_descriptors.clear();

        let mut pass = 0;
        if !self.hz_microtype {
            let t_br = if self.debug_timings { Some(std::time::Instant::now()) } else { None };
            let params = self.tex_break_params(first_line_indent);
            let shape = crate::layout::layout_linebreak::LineShape {
                measure,
                hang_indent: custom_hang,
                hang_after: self.hang_after,
                par_shape: &self.par_shape,
            };
            let range = 0..self.words_soa.len();
            let key = self.break_cache.as_ref().map(|_| {
                crate::layout::layout_linebreak::break_key(&self.words_soa, range.clone(), &params, &shape, self.base_font_size)
            });
            let cached = match (key, self.break_cache.as_mut()) {
                (Some(key), Some(cache)) => cache.get(key).map(|c| {
                    self.line_descriptors.extend_from_slice(&c.lines);
                    c.pass as usize
                }),
                _ => None,
            };
            pass = match cached {
                Some(pass) => pass,
                None => {
                    let pass = crate::layout::layout_linebreak::tex_line_break(
                        &self.words_soa,
                        range,
                        &params,
                        &shape,
                        self.base_font_size,
                        &mut self.break_scratch,
                        &mut self.line_descriptors,
                    );
                    if let (Some(key), Some(cache)) = (key, self.break_cache.as_mut()) {
                        cache.insert(key, crate::layout::layout_linebreak::CachedBreak { pass, lines: self.line_descriptors.as_slice().into() });
                    }
                    pass as usize
                }
            };
            if let Some(t) = t_br {
                self.time_pass1_ns += t.elapsed().as_nanos() as u64;
            }
        } else if self.par_align != crate::frontend::ast::ParAlign::Justified {
            // With fil glue at a margin every line that is not overfull has badness 0, so
            // TeX's optimum is the fewest lines; the greedy fill reaches it (§829–§836).
            ragged_break_paragraph(
                &self.words_soa,
                measure,
                first_line_indent,
                custom_hang,
                self.hang_after,
                &self.par_shape,
                &mut self.line_descriptors,
            );
            pass = 1;
        } else {
            let par_hash = hash_paragraph(
                &self.words_soa,
                measure,
                first_line_indent,
                custom_hang,
                self.hang_after,
                &self.par_shape,
                self.tolerance,
                self.pre_tolerance,
                self.emergency_stretch,
                self.looseness,
                self.par_fill_width,
                self.par_fill_stretch,
                self.base_font_size,
                self.font_family,
                math_collector.len(),
            );

            let cache_slot = (par_hash as usize) & PAR_CACHE_MASK;
            let cached_entry = self.par_cache[cache_slot];
            let is_cached = cached_entry.hash == par_hash && par_hash != 0;

            if is_cached {
                self.line_descriptors.extend_from_slice(&cached_entry.descriptors[..cached_entry.line_count as usize]);
            } else if try_layout_single_line_paragraph(
                &self.words_soa,
                measure,
                first_line_indent,
                self.par_fill_width,
                self.par_fill_stretch,
                &self.par_shape,
                self.looseness,
                &mut self.line_descriptors,
            ) {
                pass = 1;
                if par_hash != 0 && !self.line_descriptors.is_empty() {
                    let entry = &mut self.par_cache[cache_slot];
                    entry.hash = par_hash;
                    entry.line_count = 1;
                    entry.descriptors[0] = self.line_descriptors[self.line_descriptors.len() - 1];
                }
            } else {
                let hz_config = HzConfig::default();

                // Build candidate breakpoints and prefix sums once per paragraph
                let t_hy = if self.debug_timings { Some(std::time::Instant::now()) } else { None };
                build_breakpoints(
                    &self.words_soa,
                    self.font_family,
                    self.base_font_size,
                    measure,
                    custom_hang,
                    &self.par_shape,
                    self.hyphen_cache.get_or_insert_with(|| {
                        vec![crate::layout::layout_hyphen::HyphenCacheEntry::empty(); crate::layout::layout_hyphen::HYPHEN_CACHE_SIZE]
                            .into_boxed_slice()
                            .try_into()
                            .unwrap()
                    }),
                    &mut self.hyphen_scratch,
                    &hz_config,
                    &mut self.breakpoints,
                    &mut self.dp_prefix,
                );
                if let Some(t) = t_hy {
                    self.time_hyph_ns += t.elapsed().as_nanos() as u64;
                }

                // Pass 1: Try without hyphenation under pre_tolerance and tolerance
                if self.pre_tolerance >= 0 {
                    let t_p1 = if self.debug_timings { Some(std::time::Instant::now()) } else { None };
                    let max_unhyph = (self.tolerance as f32).max(self.pre_tolerance as f32);
                    if hz_break_paragraph_breakpoints_pass(
                        &mut self.words_soa,
                        &self.breakpoints,
                        &self.dp_prefix,
                        self.hyphen_cache.as_deref().map_or(&[][..], |c| &c[..]),
                        self.base_font_size * 0.001,
                        self.font_family,
                        measure,
                        first_line_indent,
                        custom_hang,
                        self.hang_after,
                        &self.par_shape,
                        &hz_config,
                        false, // allow_hyphenation = false
                        self.pre_tolerance as f32,
                        max_unhyph,
                        0.0,
                        self.looseness,
                        self.par_fill_width,
                        self.par_fill_stretch,
                        &mut self.dp_nodes,
                        &mut self.line_descriptors,
                    ) {
                        pass = 1;
                    }
                    if let Some(t) = t_p1 {
                        self.time_pass1_ns += t.elapsed().as_nanos() as u64;
                    }
                }

                // Pass 2: Retry with hyphenation enabled under tolerance
                if pass == 0 {
                    self.line_descriptors.clear();
                    let t_p2 = if self.debug_timings { Some(std::time::Instant::now()) } else { None };
                    if hz_break_paragraph_breakpoints_pass(
                        &mut self.words_soa,
                        &self.breakpoints,
                        &self.dp_prefix,
                        self.hyphen_cache.as_deref().map_or(&[][..], |c| &c[..]),
                        self.base_font_size * 0.001,
                        self.font_family,
                        measure,
                        first_line_indent,
                        custom_hang,
                        self.hang_after,
                        &self.par_shape,
                        &hz_config,
                        true, // allow_hyphenation = true
                        -1.0,
                        self.tolerance as f32,
                        0.0,
                        self.looseness,
                        self.par_fill_width,
                        self.par_fill_stretch,
                        &mut self.dp_nodes,
                        &mut self.line_descriptors,
                    ) {
                        pass = 2;
                    }
                    if let Some(t) = t_p2 {
                        self.time_pass2_ns += t.elapsed().as_nanos() as u64;
                    }
                }

                // Pass 3: If emergency_stretch > 0, retry with emergency stretch added to elasticity
                if pass == 0 && self.emergency_stretch > 0.0 {
                    self.line_descriptors.clear();
                    if hz_break_paragraph_breakpoints_pass(
                        &mut self.words_soa,
                        &self.breakpoints,
                        &self.dp_prefix,
                        self.hyphen_cache.as_deref().map_or(&[][..], |c| &c[..]),
                        self.base_font_size * 0.001,
                        self.font_family,
                        measure,
                        first_line_indent,
                        custom_hang,
                        self.hang_after,
                        &self.par_shape,
                        &hz_config,
                        true, // allow_hyphenation = true
                        -1.0,
                        self.tolerance as f32,
                        self.emergency_stretch,
                        self.looseness,
                        self.par_fill_width,
                        self.par_fill_stretch,
                        &mut self.dp_nodes,
                        &mut self.line_descriptors,
                    ) {
                        pass = 3;
                    }
                }

                // Fallback: If all passes failed, typeset with relaxed badness (10000.0)
                if pass == 0 {
                    self.line_descriptors.clear();
                    if !hz_break_paragraph_breakpoints_pass(
                        &mut self.words_soa,
                        &self.breakpoints,
                        &self.dp_prefix,
                        self.hyphen_cache.as_deref().map_or(&[][..], |c| &c[..]),
                        self.base_font_size * 0.001,
                        self.font_family,
                        measure,
                        first_line_indent,
                        custom_hang,
                        self.hang_after,
                        &self.par_shape,
                        &hz_config,
                        true, // allow_hyphenation = true
                        -1.0,
                        10000.0,
                        self.emergency_stretch,
                        self.looseness,
                        self.par_fill_width,
                        self.par_fill_stretch,
                        &mut self.dp_nodes,
                        &mut self.line_descriptors,
                    ) {
                        hz_break_paragraph_fast_shape(
                            &self.words_soa,
                            measure,
                            first_line_indent,
                            custom_hang,
                            self.hang_after,
                            &self.par_shape,
                            &hz_config,
                            &mut self.line_descriptors,
                        );
                    }
                }

                if !self.line_descriptors.is_empty() && self.line_descriptors.len() <= PAR_CACHE_MAX_LINES {
                    let entry = &mut self.par_cache[cache_slot];
                    entry.hash = par_hash;
                    entry.line_count = self.line_descriptors.len() as u16;
                    entry.descriptors[..self.line_descriptors.len()].copy_from_slice(&self.line_descriptors);
                }
            }
        }

        self.pass_stats[pass.min(3)] += 1;

        // Reset one-shot hanging indent, parshape, and looseness (TeX by Topic Ch 18.2.2 & 19.1.4)
        self.hang_indent = 0.0;
        self.hang_after = 1;
        self.par_shape.clear();
        self.looseness = 0;

        let t_fl = if self.debug_timings { Some(std::time::Instant::now()) } else { None };
        // Breakpoints between lines carry \interlinepenalty (0), \clubpenalty after the
        // first line (10000 after a heading), \widowpenalty before the last and
        // \brokenpenalty after a hyphenated line (tex.web §890).
        let n_lines = self.line_descriptors.len();
        let display_follows = matches!(inls.last(), Some(Inline::DisplayFollows));
        for d in 0..n_lines {
            let desc = self.line_descriptors[d];
            if d > 0 {
                use crate::layout::layout_pagebuilder::{
                    BROKEN_PENALTY, CLUB_PENALTY, DISPLAY_WIDOW_PENALTY, INF_PENALTY, WIDOW_PENALTY,
                };
                let mut pen = 0;
                if d == 1 {
                    pen += if follows_heading { INF_PENALTY } else { CLUB_PENALTY };
                }
                if d + 1 == n_lines {
                    pen += if display_follows { DISPLAY_WIDOW_PENALTY } else { WIDOW_PENALTY };
                }
                // A line ending at a discretionary: a hyphenation point or an explicit hyphen.
                let prev = self.line_descriptors[d - 1];
                let prev_end = (prev.start_word + prev.word_count) as usize;
                if prev.tail_cut != 0
                    || (prev_end > 0 && self.words_soa.attrs[prev_end - 1].kind() == WordKind::Hyphen)
                {
                    pen += BROKEN_PENALTY;
                }
                self.page_breakpoint(pen);
            }
            self.flush_line_descriptor(&desc, &math_collector);
            if !self.pending_foot_is_empty() {
                self.insert_line_footnotes(&desc, &math_collector);
            }
            if !self.words_soa.vadjusts.is_empty() {
                self.line_vadjusts(&desc);
            }
        }
        if let Some(t) = t_fl {
            self.time_flush_ns += t.elapsed().as_nanos() as u64;
        }

        let line_count = self.line_descriptors.len();
        if line_count > 0 {
            let last_desc = self.line_descriptors[line_count - 1];
            self.last_line_width = last_desc.line_w();
            self.last_line_indent = last_desc.line_indent;
            self.has_preceding_text = true;
            self.prev_graf = line_count as i32;
        } else {
            self.has_preceding_text = false;
            self.last_line_width = 0.0;
            self.last_line_indent = 0.0;
            self.prev_graf = 0;
        }
    }

    #[inline]
    pub(crate) fn push_batched_slices<'b>(
        pool: &mut Vec<&'b str>,
        slices: &[&'b str],
    ) -> u32 {
        if slices.is_empty() {
            return 0;
        }
        let count_before = pool.len();
        pool.reserve(slices.len());
        for &s in slices {
            if !s.is_empty() {
                pool.push(s);
            }
        }
        (pool.len() - count_before) as u32
    }

    /// Moves `y` from the baseline a line would get at `\baselineskip` to the one TeX's
    /// interline glue gives a box of height `h` (tex.web §679): when the glue between the
    /// previous depth and `h` would fall below `\lineskiplimit` (0pt), `\lineskip` (1pt)
    /// separates them instead; at the top of a page `\topskip` is measured to the box's
    /// top (§1001). The page builder decides later where the page breaks.
    /// tex.web §1145: `$$` in vertical mode starts a paragraph, which `line_break` turns into
    /// one line holding the indentation box. After a heading (`\@afterheading` removes the
    /// box) or `\noindent` the paragraph is empty: no line, and `\predisplaysize` is
    /// `-\maxdimen`.
    /// With `with_box` (latex.ltx's `\[`) the line follows `\nointerlineskip` and holds a
    /// `.6\linewidth` box after the indentation.
    fn empty_par_line(&mut self, with_box: bool) {
        if self.after_heading && !with_box {
            self.has_preceding_text = false;
            return;
        }
        if with_box && !self.at_page_top() {
            // `\nointerlineskip`: the empty line sits right under the previous depth.
            self.y += self.base_leading - self.prev_depth;
            self.note_box(self.y, 0.0);
        } else {
            self.place_baseline(0.0);
        }
        self.y -= self.base_leading;
        self.prev_depth = 0.0;
        let indent = if self.after_heading { 0.0 } else { self.par_indent };
        let line_width = (self.right - self.right_skip) - (self.left + self.left_skip);
        self.last_line_width = indent + if with_box { 0.6 * line_width } else { 0.0 };
        self.last_line_indent = 0.0;
        self.has_preceding_text = true;
        self.prev_graf = 1;
    }

    pub(crate) fn place_baseline(&mut self, h: f32) {
        // A strut in the box (`\rule\z@\footnotesep`) raises its height.
        let h = h.max(self.strut_height);
        self.y -= self.interline_extra(h);
        self.note_box(self.y, h);
    }

    #[inline]
    fn interline_extra(&self, h: f32) -> f32 {
        if self.at_page_top() {
            (h - self.top_skip).max(0.0)
        } else if self.ignore_depth {
            h - self.base_leading
        } else if self.base_leading - self.prev_depth - h >= 0.0 {
            0.0
        } else {
            self.prev_depth + LINESKIP_PT + h - self.base_leading
        }
    }

    /// The \linebreak parameters of the current paragraph, with LaTeX's alignment skips.
    fn tex_break_params(&self, indent: f32) -> crate::layout::layout_linebreak::BreakParams {
        use crate::frontend::ast::ParAlign;
        use crate::layout::layout_linebreak::{BreakParams, Skip};
        let mut p = BreakParams {
            pretolerance: self.pre_tolerance,
            tolerance: self.tolerance,
            emergency_stretch: self.emergency_stretch,
            looseness: self.looseness,
            indent,
            family: self.font_family,
            ..BreakParams::default()
        };
        p.par_fill_skip = if self.par_fill_stretch.is_infinite() {
            Skip { width: self.par_fill_width, ..Skip::FIL }
        } else {
            Skip { width: self.par_fill_width, stretch: self.par_fill_stretch, stretch_order: 0 }
        };
        match self.par_align {
            ParAlign::Justified => {}
            ParAlign::RaggedRight => p.right_skip = Skip::FIL,
            ParAlign::Center => {
                p.left_skip = Skip::FIL;
                p.right_skip = Skip::FIL;
                p.par_fill_skip = Skip::ZERO;
                p.final_hyphen_demerits = 0;
            }
            ParAlign::RaggedLeft => {
                p.left_skip = Skip::FIL;
                p.par_fill_skip = Skip::ZERO;
            }
        }
        p
    }

    /// TeX's glue setting of a line (`hpack`, tex.web §656–§667): the ratio by which every
    /// interword glue of the line stretches or shrinks. Lines that end in fil glue (the
    /// last line, `\\`, ragged margins) keep their natural spacing unless overfull; an
    /// overfull line shrinks at most fully.
    fn line_glue_set(&self, desc: &LineDescriptor, available: f32, ragged: bool, is_last_line: bool) -> GlueSet {
        let start = desc.start_word as usize;
        let glue = self.words_soa.line_glue(start, start + desc.word_count as usize);
        let stretch = glue[0];
        let x = available - desc.line_w();
        if x < 0.0 || ragged || stretch <= 0.0 || desc.justify() {
            GlueSet::for_line(x, glue, desc.justify() && !ragged)
        } else if is_last_line && self.last_line_fit > 0 {
            // e-TeX \lastlinefit: a fraction of the previous lines' looseness.
            let fit = (self.last_line_fit as f32 / 1000.0).clamp(0.0, 1.0);
            GlueSet::Stretch(x / stretch * fit * 0.25)
        } else {
            GlueSet::Natural
        }
    }

    /// The highest order of infinite glue in a line holding `\hfil`/`\hfill`, and its total
    /// stretch in sp (1fil = 65536), as `hpack` picks it (§659): the line's own fil glue
    /// (leading glue is discarded after a break), the `\parfillskip` of the last line, and
    /// the `\hfil` of a `\\` break. None when the line has no fil glue of its own.
    fn line_fil(&self, desc: &LineDescriptor, is_last_line: bool) -> Option<(u8, i64)> {
        let start = desc.start_word as usize;
        let end = start + desc.word_count as usize;
        let soa = &self.words_soa;
        let first = (start..end).find(|&i| !soa.attrs[i].is_space()).unwrap_or(end);
        let lo = soa.fils.partition_point(|f| (f.0 as usize) < first);
        let mut tot = [0i64; 4];
        let mut any = false;
        for &(_, st, order) in soa.fils[lo..].iter().take_while(|f| (f.0 as usize) < end) {
            tot[order as usize & 3] += (st as f64 * 65536.0).round() as i64;
            any = true;
        }
        if !any {
            return None;
        }
        if is_last_line && self.par_fill_stretch > 0.0 {
            tot[1] += (self.par_fill_stretch as f64 * 65536.0).round() as i64;
        }
        if end > start && soa.attrs[end - 1].kind() == WordKind::Break {
            tot[1] += 65536;
        }
        (1..4).rev().find(|&o| tot[o] != 0).map(|o| (o as u8, tot[o]))
    }

    /// A formula with `\color` inside ends its scopes black; the text after it continues in
    /// the line's colour.
    fn restore_color_after_math(&mut self, ops_before: usize, line_color: PackedColor) {
        let colored = self.current_page.ops[ops_before..].iter().any(|op| matches!(op, Paint::Color { .. }));
        if colored && line_color != PackedColor::BLACK {
            self.push_op(Paint::Color { color: line_color });
        }
    }

    /// Total stretch (`which` 0) or shrink (1) of a line's glue in scaled points, summed as
    /// TeX sums it (`line_glue` in sp).
    fn line_glue_sp(&self, desc: &LineDescriptor, which: usize) -> i64 {
        let start = desc.start_word as usize;
        let end = start + desc.word_count as usize;
        let mut i = start;
        while i < end && self.words_soa.attrs[i].is_space() {
            i += 1;
        }
        self.words_soa.glue[i..end].iter().map(|g| sp64(g[which])).sum()
    }

    fn flush_line_descriptor(
        &mut self,
        desc: &LineDescriptor,
        math_collector: &[&crate::math::MathNode<'a>],
    ) {
        if desc.word_count == 0 {
            return;
        }
        let (line_h, line_d) = self.words_soa.line_extent(
            desc.start_word as usize,
            (desc.start_word + desc.word_count) as usize,
            self.base_font_size,
            self.font_family,
        );
        self.place_baseline(line_h);
        if self.is_dry_run {
            self.prev_depth = line_d;
            self.y -= self.base_leading;
            return;
        }

        let measure = (self.right - self.right_skip) - (self.left + self.left_skip);
        let available = measure - desc.line_indent;

        let ragged = self.par_align != crate::frontend::ast::ParAlign::Justified;
        let is_last_line = (desc.start_word + desc.word_count) as usize >= self.words_soa.len();
        let glue_set = self.line_glue_set(desc, available, ragged, is_last_line);
        // Infinite glue in the line takes all the slack; finite glue keeps its width.
        let fil = if self.words_soa.fils.is_empty() || ragged || sp64(available) < desc.line_w_sp as i64 {
            None
        } else {
            self.line_fil(desc, is_last_line)
        };
        let glue_set = if fil.is_some() { GlueSet::Natural } else { glue_set };

        let slack = (available - desc.line_w()).max(0.0);
        let align_shift = match self.par_align {
            crate::frontend::ast::ParAlign::Center => 0.5 * slack,
            crate::frontend::ast::ParAlign::RaggedLeft => slack,
            _ => 0.0,
        };
        // The line is an hbox (§619 `hlist_out`): positions in scaled points, each glue
        // advanced by its width plus the change of `round(glue_set * cur_glue)` (§625), so
        // rounding never accumulates along the line.
        let mut xs: i64 = sp64(self.left + self.left_skip + desc.line_indent + align_shift);
        let (mut cur_glue, mut cur_g) = (0.0f64, 0i64);
        let (g_ratio, g_index) = match glue_set {
            _ if fil.is_some() => (glue_ratio(sp64(available) - desc.line_w_sp as i64, fil.map_or(1, |f| f.1)), 0),
            GlueSet::Natural => (0.0f64, 0usize),
            // §658/§664: glue_set is x / total_stretch (or shrink) of the box, in sp, as a
            // double; with \lastlinefit the setting is not hpack's own.
            GlueSet::Stretch(r) if is_last_line && self.last_line_fit > 0 => (r as f64, 0),
            GlueSet::Stretch(_) => (glue_ratio(sp64(available) - desc.line_w_sp as i64, self.line_glue_sp(desc, 0)), 0),
            GlueSet::Shrink(_) => (-glue_ratio(desc.line_w_sp as i64 - sp64(available), self.line_glue_sp(desc, 1)).min(1.0), 1),
        };
        if trace_hlist() {
            eprintln!("G {g_ratio} {} {} {} {}", sp64(available), desc.line_w_sp, self.line_glue_sp(desc, 0), self.line_glue_sp(desc, 1));
            for k in desc.start_word as usize..(desc.start_word + desc.word_count) as usize {
                eprintln!("W {:?} {}", self.words_soa.texts[k], sp64(self.words_soa.widths[k]));
            }
        }
        let start = desc.start_word as usize;
        let end = (desc.start_word + desc.word_count) as usize;
        let count = end.saturating_sub(start);
        if self.current_page.text_pool.len() + count > self.current_page.text_pool.capacity() {
            self.current_page.text_pool.reserve(count + 64);
        }
        if self.current_page.ops.len() + 2 > self.current_page.ops.capacity() {
            self.current_page.ops.reserve(16);
        }

        let mut i = start;
        // Skip leading whitespace / discarded math glue at start of line (TeX discardable item rule)
        while i < end && self.words_soa.attrs[i].is_space() {
            i += 1;
        }

        // Colour of the ops emitted so far on this line; each line ends black again, so
        // page furniture and other blocks never inherit a text colour.
        let mut line_color = PackedColor::BLACK;
        let mut color_end = if self.words_soa.color_runs.is_empty() { usize::MAX } else { 0 };
        let par_size = self.base_font_size;
        let mut run_size = par_size;
        let mut size_end = if self.words_soa.size_runs.is_empty() { usize::MAX } else { 0 };

        while i < end {
            let attr = self.words_soa.attrs[i];
            let w = self.words_soa.widths[i];
            if attr.is_space() {
                cur_glue += match fil {
                    Some((order, _)) => self.words_soa.fil_at(i).filter(|f| f.1 == order).map_or(0.0, |f| (f.0 as f64 * 65536.0).round()),
                    None => sp64(self.words_soa.glue[i][g_index]) as f64,
                };
                let g = (g_ratio * cur_glue).round() as i64;
                xs += sp64(w) + g - cur_g;
                cur_g = g;
                i += 1;
                continue;
            }
            let x_sp = Sp(xs as i32);
            let x = x_sp.to_pt();
            if i >= color_end {
                let (color, next) = self.words_soa.color_span(i);
                color_end = next;
                if color != line_color {
                    self.push_op(Paint::Color { color });
                    line_color = color;
                }
            }
            if i >= size_end {
                (run_size, size_end) = self.words_soa.size_span(i, par_size);
            }
            if attr.is_math() {
                if let Some(m_idx) = self.words_soa.get_math_idx(i) {
                    if let Some(&node) = math_collector.get(m_idx) {
                        let ops_before = self.current_page.ops.len();
                        crate::math::render_math_in(
                            node,
                            run_size,
                            self.font_family,
                            crate::math::MathStyle::Text,
                            x_sp,
                            self.y,
                            &mut self.current_page.ops,
                            &mut self.current_page.text_pool,
                            attr.face(),
                        );
                        // The item's width: the box plus any make_ord kern to the next atom.
                        xs += sp64(self.words_soa.widths[i]);
                        self.restore_color_after_math(ops_before, line_color);
                    }
                }
                i += 1;
                continue;
            }
            let face = attr.face();
            // A word cut by hyphenation is painted from its slice.
            let cut = if i == start && desc.head_cut != 0 {
                Some(&self.words_soa.texts[i][desc.head_cut as usize..])
            } else if i + 1 == end && desc.tail_cut != 0 {
                Some(&self.words_soa.texts[i][..desc.tail_cut as usize])
            } else {
                None
            };
            if let Some(piece) = cut {
                let pool_start = self.current_page.text_pool.len() as u32;
                self.current_page.text_pool.push(piece);
                self.push_op(Paint::GlyphRun {
                    x: x_sp,
                    y: self.y,
                    face,
                    size: run_size,
                    part_start: pool_start,
                    part_count: 1 | GLYPH_RUN_SHAPED,
                    word_spacing: 0.0,
                });
                let shaped = |t: &str| crate::backend::shape::word_width_pt(t, face, run_size, self.font_family);
                xs += sp64(shaped(piece));
                if i == start && desc.head_cut != 0 {
                    // Material after the word's letters (an italic correction) stays with
                    // its post-break part.
                    xs += sp64(self.words_soa.widths[i]) - sp64(shaped(self.words_soa.texts[i]));
                }
                i += 1;
                continue;
            }
            if self.words_soa.attrs[i].is_word_like() && self.words_soa.texts[i].is_empty() {
                // A kern (`\,`, the kerns of `\dots`): it moves the next glyph, and a shaped
                // run would lose it.
                xs += sp64(self.words_soa.widths[i]);
                i += 1;
                continue;
            }
            if self.words_soa.texts[i] == UNDERSCORE_TEXT {
                let w = self.words_soa.widths[i];
                self.push_op(Paint::Rule { x: (x + w / 6.0).into(), y: self.y, w: (w * 5.0 / 6.0).into(), h: Sp::pt(0.4), fill: true });
                xs += sp64(w);
                i += 1;
                continue;
            }
            let run_start = i;
            let is_cm = self.font_family == metrics::FontFamily::ComputerModern;
            // A space can ride inside a run only if the font's space glyph (plus `Tw`) has
            // exactly its measured width and glue; a space after punctuation (other space
            // factor) or of another face is positioned explicitly instead.
            let plain = crate::backend::scaled::space_glue(self.font_family, face, run_size);
            let plain_space_w = crate::backend::scaled::to_pt(plain.space as i64);
            let plain_glue = [
                crate::backend::scaled::to_pt(plain.stretch as i64),
                crate::backend::scaled::to_pt(plain.shrink as i64),
            ];
            let word_spacing = glue_set.adjust(plain_glue);

            let mut run_w = 0.0f32;
            let mut run_extra = 0.0f32;

            while i < end && i < color_end && i < size_end {
                let cur_attr = self.words_soa.attrs[i];
                let cur_face = cur_attr.face();
                if !cur_attr.is_word_like() && !cur_attr.is_space() {
                    break;
                }
                if cur_attr.is_word_like()
                    && (cur_face != face || self.words_soa.texts[i].is_empty() || self.words_soa.texts[i] == UNDERSCORE_TEXT)
                {
                    break;
                }
                if cur_attr.is_space()
                    && (is_cm
                        || cur_attr.kind() != WordKind::Space
                        || cur_face != face
                        || (self.words_soa.widths[i] - plain_space_w).abs() > 1e-3
                        || self.words_soa.glue[i] != plain_glue)
                {
                    break;
                }
                if cur_attr.is_word_like() && i + 1 == end && desc.tail_cut != 0 {
                    break;
                }
                run_w += self.words_soa.widths[i];
                if cur_attr.kind() == WordKind::Space {
                    run_extra += word_spacing;
                }
                i += 1;
            }

            let run_end = i;
            if run_start == run_end {
                i += 1;
                continue;
            }
            let pool_start = self.current_page.text_pool.len() as u32;
            let part_count = Self::push_batched_slices(
                &mut self.current_page.text_pool,
                &self.words_soa.texts[run_start..run_end],
            );

            if part_count > 0 {
                self.push_op(Paint::GlyphRun {
                    x: x_sp,
                    y: self.y,
                    face,
                    size: run_size,
                    part_start: pool_start,
                    part_count: part_count | GLYPH_RUN_SHAPED,
                    word_spacing,
                });
            }

            xs += sp64(run_w) + sp64(run_extra);
        }

        if end > start
            && (desc.tail_cut != 0
                || (self.words_soa.attrs[end - 1].kind() == WordKind::Hyphen
                    && !self.words_soa.texts[end - 1].ends_with('-')))
        {
            let hyph_face = self.words_soa.attrs[end - 1].face();
            let pool_start = self.current_page.text_pool.len() as u32;
            self.current_page.text_pool.push("-");
            self.push_op(Paint::GlyphRun {
                x: Sp(xs as i32),
                y: self.y,
                face: hyph_face,
                size: run_size,
                part_start: pool_start,
                part_count: 1 | GLYPH_RUN_SHAPED,
                word_spacing: 0.0,
            });
        }
        if line_color != PackedColor::BLACK {
            self.push_op(Paint::Color { color: PackedColor::BLACK });
        }

        self.prev_depth = line_d;
        self.y -= self.base_leading;
        self.last_line_advance = self.base_leading;
    }

    /// Paints `inls` as one line at its natural width from (`x`, `y`), through the paragraph
    /// path (fonts, kerns, `\ `, logos, colour).
    pub(crate) fn paint_inlines_at_size<'doc>(&mut self, inlines: &'doc [Inline<'a>], default_face: Face, font_size: f32, x: f32, y: Sp) {
        if self.is_dry_run {
            return;
        }
        std::mem::swap(&mut self.cell_words_soa, &mut self.scratch_soa);
        self.cell_words_soa.clear();
        let mut math = Vec::new();
        let w = explode_inlines_into_family(inlines, default_face, font_size, &mut self.cell_words_soa, self.font_family, &mut math);
        let n = self.cell_words_soa.len() as u16;
        self.paint_cell_line_at(CellLineRange::natural(0, n, w), x, y, font_size, &math);
        std::mem::swap(&mut self.cell_words_soa, &mut self.scratch_soa);
    }

    /// Breaks the cell (or caption) paragraph `cell_words_soa[start..end]` into lines of
    /// width `inner_w` with TeX's line breaker, unindented. `boxed` material (p-cells, float
    /// contents) is set after `\@parboxrestore`, which ends in `\sloppy`; `centered` sets
    /// the lines with `\centering` instead of justified.
    pub(crate) fn break_cell_words_into_lines(&mut self, start: usize, end: usize, inner_w: f32, base_size: f32, boxed: bool, centered: bool) {
        use crate::layout::layout_linebreak::{BreakParams, Skip};
        let mut params = BreakParams {
            pretolerance: self.pre_tolerance,
            tolerance: self.tolerance,
            emergency_stretch: self.emergency_stretch,
            family: self.font_family,
            ..Default::default()
        };
        if boxed {
            // \sloppy: \tolerance 9999, \emergencystretch 3em.
            params.tolerance = 9999;
            params.emergency_stretch = 3.0 * crate::backend::scaled::quad_pt(self.font_family, base_size);
        }
        if centered {
            params.left_skip = Skip::FIL;
            params.right_skip = Skip::FIL;
            params.par_fill_skip = Skip::ZERO;
            params.final_hyphen_demerits = 0;
        }
        let shape = crate::layout::layout_linebreak::LineShape { measure: inner_w, hang_indent: 0.0, hang_after: 1, par_shape: &[] };
        let mut descs = std::mem::take(&mut self.cell_descs);
        descs.clear();
        crate::layout::layout_linebreak::tex_line_break(
            &self.cell_words_soa,
            start..end,
            &params,
            &shape,
            base_size,
            &mut self.break_scratch,
            &mut descs,
        );
        for d in &descs {
            let s = d.start_word as usize;
            let glue = GlueSet::for_line(inner_w - d.line_w(), self.cell_words_soa.line_glue(s, s + d.word_count as usize), d.justify() && !centered);
            self.cell_lines.push(CellLineRange {
                start_word: d.start_word,
                word_count: d.word_count,
                line_w: d.line_w(),
                glue,
                head_cut: d.head_cut,
                tail_cut: d.tail_cut,
            });
        }
        self.cell_descs = descs;
    }

    pub(crate) fn trim_cell_line(&self, start: usize, mut end: usize, mut line_w: f32) -> (usize, f32) {
        while end > start && self.cell_words_soa.attrs[end - 1].is_space() {
            end -= 1;
            line_w -= self.cell_words_soa.widths[end];
        }
        (end, line_w.max(0.0))
    }

    /// Paints one cell (or caption) line with its glue setting, its first word starting at
    /// `x` on baseline `y`.
    pub(crate) fn paint_cell_line(&mut self, line: CellLineRange, x: f32, y: Sp, math_collector: &[&crate::math::MathNode<'a>]) {
        self.paint_cell_line_at(line, x, y, self.base_font_size, math_collector);
    }

    pub(crate) fn paint_cell_line_at(
        &mut self,
        line: CellLineRange,
        mut x: f32,
        y: Sp,
        font_size: f32,
        math_collector: &[&crate::math::MathNode<'a>],
    ) {
        if self.is_dry_run {
            return;
        }
        let start = line.start_word as usize;
        let end = start + line.word_count as usize;
        let glue_set = line.glue;
        let mut i = start;
        while i < end && self.cell_words_soa.attrs[i].is_space() {
            i += 1;
        }
        // Colour and size runs, as in `flush_line_descriptor`; the line ends black.
        let mut line_color = PackedColor::BLACK;
        let mut color_end = if self.cell_words_soa.color_runs.is_empty() { usize::MAX } else { 0 };
        let mut run_size = font_size;
        let mut size_end = if self.cell_words_soa.size_runs.is_empty() { usize::MAX } else { 0 };
        let is_cm = self.font_family == metrics::FontFamily::ComputerModern;

        while i < end {
            let attr = self.cell_words_soa.attrs[i];
            let w = self.cell_words_soa.widths[i];
            if attr.is_space() {
                x += w + glue_set.adjust(self.cell_words_soa.glue[i]);
                i += 1;
                continue;
            }
            if i >= color_end {
                let (color, next) = self.cell_words_soa.color_span(i);
                color_end = next;
                if color != line_color {
                    self.push_op(Paint::Color { color });
                    line_color = color;
                }
            }
            if i >= size_end {
                (run_size, size_end) = self.cell_words_soa.size_span(i, font_size);
            }
            if attr.is_math() {
                if let Some(m_idx) = self.cell_words_soa.get_math_idx(i) {
                    if let Some(&node) = math_collector.get(m_idx) {
                        let ops_before = self.current_page.ops.len();
                        crate::math::render_math(
                            node,
                            run_size,
                            self.font_family,
                            crate::math::MathStyle::Text,
                            Sp::pt(x),
                            y,
                            &mut self.current_page.ops,
                            &mut self.current_page.text_pool,
                        );
                        x += self.cell_words_soa.widths[i];
                        self.restore_color_after_math(ops_before, line_color);
                    }
                }
                i += 1;
                continue;
            }
            let face = attr.face();
            // A word cut by hyphenation is painted from its slice.
            let cut = if i == start && line.head_cut != 0 {
                Some(&self.cell_words_soa.texts[i][line.head_cut as usize..])
            } else if i + 1 == end && line.tail_cut != 0 {
                Some(&self.cell_words_soa.texts[i][..line.tail_cut as usize])
            } else {
                None
            };
            if let Some(piece) = cut {
                let pool_start = self.current_page.text_pool.len() as u32;
                self.current_page.text_pool.push(piece);
                self.push_op(Paint::GlyphRun {
                    x: x.into(),
                    y,
                    face,
                    size: run_size,
                    part_start: pool_start,
                    part_count: 1 | GLYPH_RUN_SHAPED,
                    word_spacing: 0.0,
                });
                let shaped = |t: &str| crate::backend::shape::word_width_pt(t, face, run_size, self.font_family);
                x += shaped(piece);
                if i == start && line.head_cut != 0 {
                    x += self.cell_words_soa.widths[i] - shaped(self.cell_words_soa.texts[i]);
                }
                i += 1;
                continue;
            }
            let x0 = x;
            let run_start = i;
            // Spaces ride inside a run only at their natural width in a font without
            // explicit positioning.
            let spaces_in_run = !is_cm && glue_set == GlueSet::Natural;
            while i < end && i < color_end && i < size_end {
                let cur_attr = self.cell_words_soa.attrs[i];
                if !cur_attr.is_word_like() && !cur_attr.is_space() {
                    break;
                }
                if cur_attr.is_word_like() && (cur_attr.face() != face || (i + 1 == end && line.tail_cut != 0)) {
                    break;
                }
                if cur_attr.is_space() && !spaces_in_run {
                    break;
                }
                i += 1;
            }
            let run_end = i;
            if run_start == run_end {
                i += 1;
                continue;
            }
            let pool_start = self.current_page.text_pool.len() as u32;
            let part_count = Self::push_batched_slices(
                &mut self.current_page.text_pool,
                &self.cell_words_soa.texts[run_start..run_end],
            );
            if part_count > 0 {
                self.push_op(Paint::GlyphRun {
                    x: x0.into(),
                    y,
                    face,
                    size: run_size,
                    part_start: pool_start,
                    part_count: part_count | GLYPH_RUN_SHAPED,
                    word_spacing: 0.0,
                });
            }
            for idx in run_start..run_end {
                x += self.cell_words_soa.widths[idx];
            }
        }
        // The hyphen of a line broken at a discretionary.
        if end > start
            && (line.tail_cut != 0
                || (self.cell_words_soa.attrs[end - 1].kind() == WordKind::Hyphen
                    && !self.cell_words_soa.texts[end - 1].ends_with('-')))
        {
            let pool_start = self.current_page.text_pool.len() as u32;
            self.current_page.text_pool.push("-");
            self.push_op(Paint::GlyphRun {
                x: x.into(),
                y,
                face: self.cell_words_soa.attrs[end - 1].face(),
                size: run_size,
                part_start: pool_start,
                part_count: 1 | GLYPH_RUN_SHAPED,
                word_spacing: 0.0,
            });
        }
        if line_color != PackedColor::BLACK {
            self.push_op(Paint::Color { color: PackedColor::BLACK });
        }
    }
}

pub fn to_roman(n: u32) -> &'static str {
    const ROMANS: &[&str] = &[
        "i", "ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x", "xi", "xii", "xiii", "xiv",
        "xv", "xvi", "xvii", "xviii", "xix", "xx", "xxi", "xxii", "xxiii", "xxiv", "xxv", "xxvi",
        "xxvii", "xxviii", "xxix", "xxx",
    ];
    if n >= 1 && (n as usize) <= ROMANS.len() {
        ROMANS[(n - 1) as usize]
    } else {
        "i"
    }
}

pub fn to_upper_roman(n: u32) -> &'static str {
    const UPPER_ROMANS: &[&str] = &[
        "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII", "XIII", "XIV",
        "XV", "XVI", "XVII", "XVIII", "XIX", "XX", "XXI", "XXII", "XXIII", "XXIV", "XXV", "XXVI",
        "XXVII", "XXVIII", "XXIX", "XXX",
    ];
    if n >= 1 && (n as usize) <= UPPER_ROMANS.len() {
        UPPER_ROMANS[(n - 1) as usize]
    } else {
        "I"
    }
}

pub fn marker_width(marker: Marker, face: Face, size: f32) -> f32 {
    marker_width_family(marker, face, size, metrics::FontFamily::Times)
}

pub fn marker_width_family(
    marker: Marker,
    face: Face,
    size: f32,
    family: metrics::FontFamily,
) -> f32 {
    // With TFM data: the codes the PDF writer emits for the label, at TeX's widths.
    if let Some(m) = crate::backend::scaled::fix_metrics(family, face, size) {
        let mut codes = Vec::with_capacity(16);
        crate::backend::backend_pdf::write_marker(&mut codes, marker, face);
        let scaler = crate::backend::scaled::Scaler::at_pt(size);
        let sp: i64 = codes.iter().map(|&c| scaler.scale(m.widths[c as usize & 127] as i32) as i64).sum();
        return crate::backend::scaled::to_pt(sp);
    }
    let scale = size * 0.001;
    let table = metrics::resolve_width_table_at(family, face, size);
    match marker {
        Marker::Bullet => metrics::width_family('•', face, size, family),
        Marker::Dash => metrics::width_family('–', face, size, family),
        Marker::Asterisk if face == Face::Symbol => (table[0x03] as f32) * scale,
        Marker::Asterisk => (table[b'*' as usize] as f32) * scale,
        Marker::MiddleDot if face == Face::Symbol => (table[0x01] as f32) * scale,
        Marker::MiddleDot => metrics::width_family('·', face, size, family),
        Marker::Number(n) => {
            let num_digits = count_digits(n);
            let digit_units = metrics::resolve_digit_units_at(family, face, size);
            let dot_units = table[b'.' as usize] as u32;
            ((num_digits * digit_units + dot_units) as f32) * scale
        }
        Marker::ParenNumber(n) => {
            let num_digits = count_digits(n);
            let digit_units = metrics::resolve_digit_units_at(family, face, size);
            let parens = table[b'(' as usize] as u32 + table[b')' as usize] as u32;
            ((num_digits * digit_units + parens) as f32) * scale
        }
        Marker::Alpha(n) => {
            let ch = (b'a' + ((n.max(1) - 1) % 26) as u8) as usize;
            let units =
                table[b'(' as usize] as u32 + table[ch] as u32 + table[b')' as usize] as u32;
            (units as f32) * scale
        }
        Marker::AlphaDot(n) => {
            let ch = (b'a' + ((n.max(1) - 1) % 26) as u8) as usize;
            let units = table[ch] as u32 + table[b'.' as usize] as u32;
            (units as f32) * scale
        }
        Marker::Roman(n) => {
            let s = to_roman(n);
            let mut units = table[b'.' as usize] as u32;
            for b in s.bytes() {
                units += table[b as usize] as u32;
            }
            (units as f32) * scale
        }
        Marker::UpperRoman(n) => {
            let s = to_upper_roman(n);
            let mut units = table[b'.' as usize] as u32;
            for b in s.bytes() {
                units += table[b as usize] as u32;
            }
            (units as f32) * scale
        }
        Marker::UpperAlpha(n) => {
            let ch = (b'A' + ((n.max(1) - 1) % 26) as u8) as usize;
            let units = table[ch] as u32 + table[b'.' as usize] as u32;
            (units as f32) * scale
        }
    }
}

/// `\Large` and `\large` sizes with their \baselineskip for the standard class size files.
/// `\topsep`, `\partopsep`, `\parsep` and `\itemsep` (natural, stretch, shrink).
struct ListParams {
    topsep: (f32, f32, f32),
    partopsep: (f32, f32, f32),
    parsep: (f32, f32, f32),
    itemsep: (f32, f32, f32),
}

/// The list parameters of list level `level` (0 = `\@listi`) from size10.clo,
/// size11.clo and size12.clo; levels four to six keep the third level's values, and other
/// class sizes scale the 10pt ones.
/// The list parameters of size10.clo, size11.clo and size12.clo for nesting `level` in a
/// class of `class_size` with the current font at `size`. `\small` and `\footnotesize`
/// redefine `\@listi` (`\normalsize` restores `\@listI`); the other size commands leave it.
/// Other class sizes scale the 10pt values.
fn list_level_params(class_size: f32, size: f32, level: usize) -> ListParams {
    use crate::frontend::ast::FontSize;
    let class = if (class_size - 10.95).abs() < 0.06 {
        1
    } else if (class_size - 12.0).abs() < 0.01 {
        2
    } else {
        0
    };
    let k = if class == 0 { class_size / 10.0 } else { 1.0 };
    let s = |v: (f32, f32, f32)| (v.0 * k, v.1 * k, v.2 * k);
    let partop = [(2.0, 1.0, 1.0), (3.0, 1.0, 1.0), (3.0, 2.0, 2.0)][class];
    let is_size = |f: FontSize| (f.metrics(class_size).0 - size).abs() < 0.01;
    let p = match level {
        0 if is_size(FontSize::Small) => {
            let (topsep, parsep) = [((4.0, 2.0, 2.0), (2.0, 1.0, 1.0)), ((6.0, 2.0, 2.0), (3.0, 2.0, 1.0)), ((9.0, 3.0, 5.0), (4.5, 2.0, 1.0))][class];
            ListParams { topsep, partopsep: partop, parsep, itemsep: parsep }
        }
        0 if is_size(FontSize::FootnoteSize) => {
            let (topsep, parsep) = [((3.0, 1.0, 1.0), (2.0, 1.0, 1.0)), ((4.0, 2.0, 2.0), (2.0, 1.0, 1.0)), ((6.0, 2.0, 2.0), (3.0, 2.0, 1.0))][class];
            ListParams { topsep, partopsep: partop, parsep, itemsep: parsep }
        }
        0 => {
            let (parsep, topsep) = [((4.0, 2.0, 1.0), (8.0, 2.0, 4.0)), ((4.5, 2.0, 1.0), (9.0, 3.0, 5.0)), ((5.0, 2.5, 1.0), (10.0, 4.0, 6.0))][class];
            ListParams { topsep, partopsep: partop, parsep, itemsep: parsep }
        }
        1 => {
            let (topsep, parsep) = [((4.0, 2.0, 1.0), (2.0, 1.0, 1.0)), ((4.5, 2.0, 1.0), (2.0, 1.0, 1.0)), ((5.0, 2.5, 1.0), (2.5, 1.0, 1.0))][class];
            ListParams { topsep, partopsep: partop, parsep, itemsep: parsep }
        }
        _ => {
            let topsep = [(2.0, 1.0, 1.0), (2.0, 1.0, 1.0), (2.5, 1.0, 1.0)][class];
            ListParams { topsep, partopsep: (1.0, 0.0, 1.0), parsep: (0.0, 0.0, 0.0), itemsep: topsep }
        }
    };
    ListParams { topsep: s(p.topsep), partopsep: s(p.partopsep), parsep: s(p.parsep), itemsep: s(p.itemsep) }
}

/// The first-level `\topsep` and `\partopsep` (natural, stretch, shrink) of size10.clo,
/// size11.clo and size12.clo; other class sizes scale the 10pt values.
/// The `\TeX`/`\LaTeX`/`\AmS` logo for text face `face` at `size`, built once per face and size.
fn logo_node(latex: crate::frontend::ast::LogoKind, face: Face, size: f32) -> &'static crate::math::MathNode<'static> {
    use std::sync::{Mutex, OnceLock};
    type Cache = Mutex<Vec<(crate::frontend::ast::LogoKind, Face, u32, &'static crate::math::MathNode<'static>)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let key = size.to_bits();
    let mut cache = CACHE.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&(_, _, _, n)) = cache.iter().find(|&&(l, f, k, _)| l == latex && f == face && k == key) {
        return n;
    }
    // A handful per document, kept for the process's lifetime.
    let n: &'static _ = Box::leak(Box::new(crate::frontend::parser::tex_logo(latex, face, size)));
    cache.push((latex, face, key, n));
    n
}

/// amsmath `\jot`.
const JOT_PT: f32 = 3.0;

/// amsmath `\multlinetaggap`.
const MULTLINE_TAG_GAP_PT: f32 = 10.0;

fn is_theorem(b: &Block<'_>) -> bool {
    matches!(b, Block::Aligned(a) if a.kind != TrivlistKind::Environment)
}

/// latex.ltx `\@topsepadd`: `\topsep`, plus `\partopsep` when the environment starts in
/// vertical mode.
fn topsep_add(top: (f32, f32, f32), partop: (f32, f32, f32), in_vmode: bool) -> (f32, f32, f32) {
    if in_vmode {
        (top.0 + partop.0, top.1 + partop.1, top.2 + partop.2)
    } else {
        top
    }
}

fn class_topsep(base_size: f32) -> ((f32, f32, f32), (f32, f32, f32)) {
    if (base_size - 10.95).abs() < 0.06 {
        ((9.0, 3.0, 5.0), (3.0, 1.0, 1.0))
    } else if (base_size - 12.0).abs() < 0.01 {
        ((10.0, 4.0, 6.0), (3.0, 2.0, 2.0))
    } else {
        let k = base_size / 10.0;
        ((TOPSEP * k, 2.0 * k, 4.0 * k), (PARTOPSEP * k, k, k))
    }
}

fn section_font_sizes(base_size: f32, base_leading: f32) -> (f32, f32, f32, f32) {
    use crate::frontend::ast::FontSize;
    // `\section` is in `\Large` and `\subsection` in `\large`, from size1x.clo.
    if [10.0, 10.95, 12.0].iter().any(|c| (base_size - c).abs() < 0.06) {
        let (l, lb) = FontSize::LargeX.metrics(base_size);
        let (m, mb) = FontSize::Large.metrics(base_size);
        (l, lb, m, mb)
    } else {
        let r = base_leading / base_size;
        (base_size * 1.44, base_size * 1.44 * r, base_size * 1.2, base_size * 1.2 * r)
    }
}

/// Flag on a section number's first counter: after `\appendix` it prints as a capital
/// letter.
pub(crate) const APPENDIX_SECTION: u16 = 0x8000;

/// Writes section number `number` to `level` (`1.2`, `A.1`).
pub(crate) fn write_section_number(number: [u16; 4], level: u8, out: &mut Vec<u8>) {
    for (i, &n) in number.iter().take((level as usize).clamp(1, 4)).enumerate() {
        if i > 0 {
            out.push(b'.');
        }
        if i == 0 && n & APPENDIX_SECTION != 0 {
            let k = (n & !APPENDIX_SECTION) as u8;
            if (1..=26).contains(&k) {
                out.push(b'A' + k - 1);
            }
        } else {
            out.extend_from_slice(n.to_string().as_bytes());
        }
    }
}

/// Width in points of font codes at their TFM widths (§571), summed in scaled points.
fn tfm_codes_pt(m: &crate::backend::backend_cm_fix::FixMetrics, codes: &[u8], size: f32) -> f32 {
    let scaler = crate::backend::scaled::Scaler::at_pt(size);
    crate::backend::scaled::to_pt(codes.iter().map(|&c| scaler.scale(m.widths[c as usize & 127] as i32) as i64).sum())
}

pub(crate) fn section_number_width(
    number: [u16; 4],
    level: u8,
    face: Face,
    size: f32,
    family: metrics::FontFamily,
) -> f32 {
    // `\@seccntformat`: `\the<counter>\quad`, at the TFM widths and quad of the face.
    if let Some(m) = crate::backend::scaled::fix_metrics(family, face, size) {
        let mut codes = Vec::with_capacity(16);
        write_section_number(number, level, &mut codes);
        let quad = crate::backend::scaled::face_quad_pt(family, face, size);
        return tfm_codes_pt(m, &codes, size) + quad;
    }
    let table = metrics::resolve_width_table_at(family, face, size);
    let digit_units = metrics::resolve_digit_units_at(family, face, size);
    let dot_units = table[b'.' as usize] as u32;
    let space_units = metrics::resolve_space_units_at(family, face, size).round() as u32;
    let scale = size * 0.001;

    let mut total_units = 0;
    let lvl = (level as usize).clamp(1, 4);
    for (i, &num) in number.iter().take(lvl).enumerate() {
        if i > 0 {
            total_units += dot_units;
        }
        if i == 0 && num & APPENDIX_SECTION != 0 {
            let k = (num & !APPENDIX_SECTION) as usize;
            if (1..=26).contains(&k) {
                total_units += table[b'A' as usize + k - 1] as u32;
            }
        } else {
            total_units += count_digits(num as u32) * digit_units;
        }
    }
    total_units += space_units * 3;

    (total_units as f32) * scale
}

pub(crate) fn equation_number_width(
    n: u32,
    face: Face,
    size: f32,
    family: metrics::FontFamily,
) -> f32 {
    if let Some(m) = crate::backend::scaled::fix_metrics(family, face, size) {
        let mut codes = Vec::with_capacity(12);
        codes.push(b'(');
        crate::backend::backend_pdf::write_u64(&mut codes, n as u64);
        codes.push(b')');
        return tfm_codes_pt(m, &codes, size);
    }
    let scale = size * 0.001;
    let table = metrics::resolve_width_table_at(family, face, size);
    let open_w = table[b'(' as usize] as f32 * scale;
    let close_w = table[b')' as usize] as f32 * scale;
    let digit_units = metrics::resolve_digit_units_at(family, face, size);
    let digits = count_digits(n);
    open_w + close_w + (digits as f32) * (digit_units as f32 * scale)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum WordKind {
    Word = 0,
    Space = 1,
    Break = 2,
    Math = 3,
    MathBin = 4,
    MathRel = 5,
    MathSpace = 6,
    Hyphen = 7,
}

impl WordKind {
    #[inline(always)]
    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => WordKind::Word,
            1 => WordKind::Space,
            2 => WordKind::Break,
            3 => WordKind::Math,
            4 => WordKind::MathBin,
            5 => WordKind::MathRel,
            6 => WordKind::MathSpace,
            7 => WordKind::Hyphen,
            _ => WordKind::Word,
        }
    }

    #[inline(always)]
    pub fn is_word_like(self) -> bool {
        matches!(self, WordKind::Word | WordKind::Hyphen)
    }

    #[inline(always)]
    pub fn is_hyphen(self) -> bool {
        self == WordKind::Hyphen
    }

    #[inline(always)]
    pub fn is_math(self) -> bool {
        matches!(self, WordKind::Math | WordKind::MathBin | WordKind::MathRel)
    }

    #[inline(always)]
    pub fn is_space(self) -> bool {
        matches!(self, WordKind::Space | WordKind::MathSpace)
    }

    #[inline(always)]
    pub fn is_breakable_math(self) -> bool {
        matches!(self, WordKind::MathBin | WordKind::MathRel)
    }

    #[inline(always)]
    pub fn math_penalty(self) -> Option<i32> {
        match self {
            WordKind::MathBin => Some(crate::math::InlineMathPenaltyInserter::BIN_PENALTY),
            WordKind::MathRel => Some(crate::math::InlineMathPenaltyInserter::REL_PENALTY),
            _ => None,
        }
    }
}

/// Compact 2-byte bit-packed word attribute storing both `WordKind` (low 3 bits)
/// and `Face` (the 6 bits above).
/// 32 word attributes fit into a single 64-byte L1 CPU cache line with no line splits.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
#[repr(transparent)]
pub struct PackedWordAttr(pub u16);

const _: () = assert!(core::mem::size_of::<PackedWordAttr>() == 2);

impl PackedWordAttr {
    #[inline(always)]
    pub const fn new(kind: WordKind, face: Face) -> Self {
        Self((kind as u16) | ((face as u16) << 3))
    }

    #[inline(always)]
    pub fn kind(self) -> WordKind {
        WordKind::from_u8((self.0 & 0x07) as u8)
    }

    #[inline(always)]
    pub fn face(self) -> Face {
        Face::from_u8((self.0 >> 3) as u8)
    }

    #[inline(always)]
    pub fn is_word_like(self) -> bool {
        let k = self.0 & 0x07;
        k == 0 || k == 7
    }

    #[inline(always)]
    pub fn is_hyphen(self) -> bool {
        (self.0 & 0x07) == 7
    }

    #[inline(always)]
    pub fn is_space(self) -> bool {
        let k = self.0 & 0x07;
        k == 1 || k == 6
    }

    #[inline(always)]
    pub fn is_math(self) -> bool {
        let k = self.0 & 0x07;
        (3..=5).contains(&k)
    }

    #[inline(always)]
    pub fn is_breakable_math(self) -> bool {
        let k = self.0 & 0x07;
        k == 4 || k == 5
    }

    #[inline(always)]
    pub fn math_penalty(self) -> Option<i32> {
        match self.0 & 0x07 {
            4 => Some(crate::math::InlineMathPenaltyInserter::BIN_PENALTY),
            5 => Some(crate::math::InlineMathPenaltyInserter::REL_PENALTY),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SparseMathIdx {
    pub word_idx: u32,
    pub math_idx: u32,
    /// Height and depth of the formula in points, for the line's interline glue.
    pub h: f32,
    pub d: f32,
}

#[derive(Default)]
pub struct ParagraphSoA<'a> {
    pub widths: Vec<f32>,
    /// Stretch and shrink of each item in points: the interword glue of spaces (TeX's
    /// font glue adjusted by the space factor), zero for boxes.
    pub glue: Vec<[f32; 2]>,
    pub attrs: Vec<PackedWordAttr>,
    pub texts: Vec<&'a str>,
    pub math_indices: Vec<SparseMathIdx>,
    /// Sparse colour runs: (first word, colour), sorted. Empty for all-black text, so
    /// uncoloured paragraphs pay nothing.
    pub color_runs: Vec<(u32, PackedColor)>,
    /// Sparse font-size runs (first word, size in pt), sorted; empty while every word is at
    /// the paragraph's size.
    pub size_runs: Vec<(u32, f32)>,
    /// The class's normal size, which the size declarations resolve against.
    pub class_size: f32,
    /// Sparse explicit kerns (word, width in pt), sorted: the italic corrections (`\/`) that
    /// end a word. The word's width includes it; the line breaker splits it off, since a
    /// kern before glue is a breakpoint and vanishes at the end of a line (§866, §881).
    pub kerns: Vec<(u32, f32)>,
    /// Sparse `\vadjust` skips (`\vspace` in a paragraph): (word, skip in pt), sorted; the
    /// skip goes below the line that holds the word.
    pub vadjusts: Vec<(u32, f32)>,
    /// Sparse infinite glue (`\hfil`, `\hfill`): (space word, stretch, order), sorted. The
    /// word's own `glue` is zero, so finite sums never see it.
    pub fils: Vec<(u32, f32, u8)>,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<ParagraphSoA>() == 248);

impl<'a> ParagraphSoA<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            widths: Vec::with_capacity(cap),
            glue: Vec::with_capacity(cap),
            attrs: Vec::with_capacity(cap),
            texts: Vec::with_capacity(cap),
            math_indices: Vec::with_capacity(4),
            color_runs: Vec::new(),
            size_runs: Vec::new(),
            class_size: FONT_SIZE,
            kerns: Vec::new(),
            vadjusts: Vec::new(),
            fils: Vec::new(),
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.widths.clear();
        self.glue.clear();
        self.attrs.clear();
        self.texts.clear();
        self.math_indices.clear();
        self.color_runs.clear();
        self.size_runs.clear();
        self.kerns.clear();
        self.vadjusts.clear();
        self.fils.clear();
    }

    /// `\unskip`: drops a trailing space; returns its width.
    pub fn unskip(&mut self) -> f32 {
        let n = self.widths.len();
        if n == 0 || self.attrs[n - 1].kind() != WordKind::Space {
            return 0.0;
        }
        if self.fils.last().is_some_and(|f| f.0 as usize == n - 1) {
            self.fils.pop();
        }
        self.glue.pop();
        self.attrs.pop();
        self.texts.pop();
        self.widths.pop().unwrap_or(0.0)
    }

    /// The infinite glue of space word `i`: (stretch, order).
    #[inline]
    pub fn fil_at(&self, i: usize) -> Option<(f32, u8)> {
        self.fils.binary_search_by_key(&(i as u32), |f| f.0).ok().map(|k| (self.fils[k].1, self.fils[k].2))
    }

    /// Adds an italic correction of `ic` pt after word `w`, as an explicit kern.
    #[inline]
    pub fn add_italic_correction(&mut self, w: usize, ic: f32) {
        if ic == 0.0 {
            return;
        }
        self.widths[w] += ic;
        match self.kerns.last_mut() {
            Some(k) if k.0 == w as u32 => k.1 += ic,
            _ => self.kerns.push((w as u32, ic)),
        }
    }

    /// Size of the next word pushed; `base` is the paragraph's size.
    #[inline]
    pub fn current_size(&self, base: f32) -> f32 {
        self.size_runs.last().map_or(base, |r| r.1)
    }

    /// Sets the size from the next word on; `base` is the size words without a run have.
    pub fn set_size(&mut self, size: f32, base: f32) {
        if size == self.current_size(base) {
            return;
        }
        let at = self.widths.len() as u32;
        match self.size_runs.last_mut() {
            Some(last) if last.0 == at => last.1 = size,
            _ => self.size_runs.push((at, size)),
        }
        let n = self.size_runs.len();
        if n >= 2 && self.size_runs[n - 1].0 == at && self.size_runs[n - 2].1 == size {
            self.size_runs.pop();
        }
    }

    /// Size of word `i` (or `base`) and the index of the next word whose size may differ.
    pub fn size_span(&self, i: usize, base: f32) -> (f32, usize) {
        let k = self.size_runs.partition_point(|r| r.0 as usize <= i);
        let size = if k == 0 { base } else { self.size_runs[k - 1].1 };
        let next = self.size_runs.get(k).map_or(usize::MAX, |r| r.0 as usize);
        (size, next)
    }

    /// Height and depth in points of the box TeX packs from items `start..end` (§653): its
    /// tallest and deepest characters and formulas.
    pub fn line_extent(&self, start: usize, end: usize, base: f32, family: metrics::FontFamily) -> (f32, f32) {
        let (mut h, mut d) = (0.0f32, 0.0f32);
        let (mut size, mut size_end) = if self.size_runs.is_empty() { (base, usize::MAX) } else { (base, 0) };
        // The font of the previous word, reused while face and size stay the same.
        let mut font: Option<(Face, f32, Option<crate::backend::shape::TextFont>, crate::backend::scaled::Scaler)> = None;
        for i in start..end {
            let attr = self.attrs[i];
            if !attr.is_word_like() || attr.is_math() {
                continue;
            }
            if i >= size_end {
                (size, size_end) = self.size_span(i, base);
            }
            let text = self.texts[i];
            if family == metrics::FontFamily::ComputerModern {
                let face = attr.face();
                if !matches!(font, Some((f, s, ..)) if f == face && s == size) {
                    font = Some((face, size, crate::backend::shape::text_font_at(family, face, size), crate::backend::scaled::Scaler::at_pt(size)));
                }
                if let Some((_, _, Some(tf), scaler)) = &font {
                    if let Some((wh, wd)) = tf.height_depth_sp(text, scaler) {
                        h = h.max(crate::backend::scaled::sp_pt(wh));
                        d = d.max(crate::backend::scaled::sp_pt(wd));
                        continue;
                    }
                }
                // Unshaped faces (typewriter): each character's TFM height and depth.
                let face = attr.face();
                let dims: Option<Vec<_>> = text.bytes().map(|b| crate::math::math_tfm::glyph_dims(face, b, size, family)).collect();
                if let Some(dims) = dims.filter(|_| text.is_ascii()) {
                    for g in dims {
                        h = h.max(g.h);
                        d = d.max(g.d);
                    }
                    continue;
                }
            }
            let (mut wh, mut wd) = (0.0f32, 0.0f32);
            if text.is_ascii() {
                let table = crate::math::math_fonts::ascii_height_depth_table(attr.face(), family);
                for &b in text.as_bytes() {
                    let (ch_h, ch_d) = table[b as usize];
                    wh = wh.max(ch_h);
                    wd = wd.max(ch_d);
                }
            } else {
                for ch in text.chars() {
                    let (ch_h, ch_d) = crate::math::math_fonts::char_height_depth(ch, attr.face(), family);
                    wh = wh.max(ch_h);
                    wd = wd.max(ch_d);
                }
            }
            h = h.max(wh * size * 0.001);
            d = d.max(wd * size * 0.001);
        }
        let k = self.math_indices.partition_point(|m| (m.word_idx as usize) < start);
        for m in &self.math_indices[k..] {
            if m.word_idx as usize >= end {
                break;
            }
            h = h.max(m.h);
            d = d.max(m.d);
        }
        (h, d)
    }

    /// Colour of the next word pushed.
    #[inline]
    pub fn current_color(&self) -> PackedColor {
        self.color_runs.last().map_or(PackedColor::BLACK, |r| r.1)
    }

    /// Sets the colour from the next word on.
    pub fn set_color(&mut self, color: PackedColor) {
        if color == self.current_color() {
            return;
        }
        let at = self.widths.len() as u32;
        match self.color_runs.last_mut() {
            Some(last) if last.0 == at => last.1 = color,
            _ => self.color_runs.push((at, color)),
        }
        // Setting back the colour of the run before an empty run merges the two.
        let n = self.color_runs.len();
        if n >= 2 && self.color_runs[n - 1].0 == at && self.color_runs[n - 2].1 == color {
            self.color_runs.pop();
        }
        if self.color_runs.len() == 1 && self.color_runs[0] == (0, PackedColor::BLACK) {
            self.color_runs.clear();
        }
    }

    /// Colour of word `i` and the index of the next word whose colour may differ.
    pub fn color_span(&self, i: usize) -> (PackedColor, usize) {
        let k = self.color_runs.partition_point(|r| r.0 as usize <= i);
        let color = if k == 0 { PackedColor::BLACK } else { self.color_runs[k - 1].1 };
        let next = self.color_runs.get(k).map_or(usize::MAX, |r| r.0 as usize);
        (color, next)
    }

    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.widths.reserve(additional);
        self.attrs.reserve(additional);
        self.texts.reserve(additional);
    }

    #[inline]
    pub fn push(&mut self, text: &'a str, face: Face, width: f32, kind: WordKind) {
        self.widths.push(width);
        self.glue.push([0.0; 2]);
        self.attrs.push(PackedWordAttr::new(kind, face));
        self.texts.push(text);
    }

    /// Appends interword glue: natural width, stretch and shrink in points.
    #[inline]
    /// Stretch and shrink of the glue in items `start..end`, leading spaces excluded (TeX
    /// discards them after a break).
    pub fn line_glue(&self, start: usize, end: usize) -> [f32; 2] {
        let mut i = start;
        while i < end && self.attrs[i].is_space() {
            i += 1;
        }
        self.glue[i..end].iter().fold([0.0, 0.0], |a, g| [a[0] + g[0], a[1] + g[1]])
    }

    pub fn push_space(&mut self, text: &'a str, face: Face, glue: (f32, f32, f32)) {
        self.widths.push(glue.0);
        self.glue.push([glue.1, glue.2]);
        self.attrs.push(PackedWordAttr::new(WordKind::Space, face));
        self.texts.push(text);
    }

    /// The text font of the last item: a formula's `\text` takes it.
    #[inline]
    pub fn set_last_face(&mut self, face: Face) {
        if let Some(a) = self.attrs.last_mut() {
            *a = PackedWordAttr::new(a.kind(), face);
        }
    }

    #[inline]
    pub fn push_math(&mut self, mb: crate::math::MathBox, math_idx: u32) {
        self.push_math_atom(mb, math_idx, WordKind::Math);
    }

    #[inline]
    pub fn push_math_atom(&mut self, mb: crate::math::MathBox, math_idx: u32, kind: WordKind) {
        let idx = self.widths.len() as u32;
        self.widths.push(mb.w);
        self.glue.push([0.0; 2]);
        self.attrs.push(PackedWordAttr::new(kind, Face::Roman));
        self.texts.push("");
        self.math_indices.push(SparseMathIdx {
            word_idx: idx,
            math_idx,
            h: mb.h,
            d: mb.d,
        });
    }

    #[inline(always)]
    pub fn kind(&self, idx: usize) -> WordKind {
        self.attrs[idx].kind()
    }

    #[inline(always)]
    pub fn face(&self, idx: usize) -> Face {
        self.attrs[idx].face()
    }

    #[inline]
    pub fn kinds(&self) -> impl Iterator<Item = WordKind> + '_ {
        self.attrs.iter().map(|a| a.kind())
    }

    #[inline]
    pub fn get_math_idx(&self, word_idx: usize) -> Option<usize> {
        self.math_indices
            .binary_search_by_key(&(word_idx as u32), |m| m.word_idx)
            .ok()
            .map(|idx| self.math_indices[idx].math_idx as usize)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.widths.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.widths.is_empty()
    }
}

/// The glue setting of one line: interword glue keeps its natural width, or every glue
/// changes by the ratio times its stretch or shrink.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GlueSet {
    Natural,
    Stretch(f32),
    Shrink(f32),
}

/// Scaled points of a layout length (f64: exact for any page coordinate).
#[inline(always)]
fn sp64(pt: f32) -> i64 {
    (pt as f64 * 65536.0).round() as i64
}

/// `RUSHTEX_TRACE_G` set: each paragraph line is written to stderr as `G <glue_set>
/// <width> <natural> <stretch> <shrink>` (scaled points) and its items as `W <text> <width>`.
fn trace_hlist() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("RUSHTEX_TRACE_G").is_some())
}

/// Points of scaled points, as layout keeps them.
#[inline(always)]
fn pt64(sp: i64) -> f32 {
    (sp as f64 / 65536.0) as f32
}

/// `x / total` as hpack sets `glue_set` (§658): a double from two sp values.
#[inline(always)]
fn glue_ratio(x: i64, total: i64) -> f64 {
    if total > 0 { x as f64 / total as f64 } else { 0.0 }
}

impl GlueSet {
    /// TeX's glue setting (`hpack`, §656–§664) for a line `slack` points short of its width
    /// (negative: too long) whose glue totals `[stretch, shrink]`. A line that does not
    /// justify keeps its natural spacing unless it must shrink; shrinking stops at the
    /// glue's full shrink.
    #[inline]
    pub(crate) fn for_line(slack: f32, glue: [f32; 2], justify: bool) -> Self {
        if slack < 0.0 {
            if glue[1] > 0.0 { GlueSet::Shrink((-slack / glue[1]).min(1.0)) } else { GlueSet::Natural }
        } else if justify && glue[0] > 0.0 {
            GlueSet::Stretch(slack / glue[0])
        } else {
            GlueSet::Natural
        }
    }

    /// The change of a glue with `[stretch, shrink]` under this setting, in points.
    #[inline(always)]
    pub(crate) fn adjust(self, g: [f32; 2]) -> f32 {
        match self {
            GlueSet::Natural => 0.0,
            GlueSet::Stretch(r) => r * g[0],
            GlueSet::Shrink(r) => -r * g[1],
        }
    }
}

/// Bit-packed 16-byte formatted line descriptor from hz line breaking.
/// Exactly 4 descriptors fit into a single 64-byte L1 CPU cache line (64 / 16 = 4).
pub use crate::layout::layout_hz::FlatAsciiWidthTable;
pub use crate::layout::layout_hz::HzLineDescriptor as LineDescriptor;

/// One line of a table cell or caption: its items, natural width, glue setting and the
/// byte offsets of hyphenation cuts in its first and last word (0 = whole word).
#[derive(Clone, Copy, Debug)]
pub struct CellLineRange {
    pub start_word: u16,
    pub word_count: u16,
    pub line_w: f32,
    pub(crate) glue: GlueSet,
    pub head_cut: u8,
    pub tail_cut: u8,
}

impl CellLineRange {
    /// A line set at its natural width.
    #[inline]
    pub fn natural(start_word: u16, word_count: u16, line_w: f32) -> Self {
        Self { start_word, word_count, line_w, glue: GlueSet::Natural, head_cut: 0, tail_cut: 0 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellSpan {
    pub start: u16,
    pub count: u16,
}

/// A paragraph piece between `\par`s (see [`split_at_par_end`]).
struct ParPiece<'a> {
    inls: Vec<Inline<'a>>,
    /// Alignment declared in a group still open where the piece ends: the piece takes it.
    end_align: Option<crate::frontend::ast::ParAlign>,
    /// Alignment declared in a group open where the piece starts (its \parindent is 0pt).
    start_align: Option<crate::frontend::ast::ParAlign>,
    /// Size declared in a group still open where the piece ends: its `\baselineskip`
    /// spaces the piece's lines.
    end_size: Option<crate::frontend::ast::FontSize>,
}

fn inlines_have_par_end(inls: &[Inline<'_>]) -> bool {
    inls.iter().any(|inl| match inl {
        Inline::ParEnd => true,
        Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x) => inlines_have_par_end(x),
        _ => false,
    })
}

/// Whether a `\footnote` is among the inlines, looking into groups.
fn inlines_have_footnote(inls: &[Inline<'_>]) -> bool {
    inls.iter().any(|inl| match inl {
        Inline::Footnote(_) => true,
        Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x) => inlines_have_footnote(x),
        _ => false,
    })
}

/// Whether the inlines typeset nothing (only spaces and declarations).
fn is_blank_inlines(inls: &[Inline<'_>]) -> bool {
    inls.iter().all(|inl| match inl {
        Inline::Text(s) => s.trim().is_empty(),
        Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x) => is_blank_inlines(x),
        Inline::Math(_) | Inline::HSpace(_) | Inline::HGlue(..) | Inline::HFil(..) | Inline::Footnote(_) | Inline::Logo(_) => false,
        _ => true,
    })
}

/// Whether the piece's first item is a group with material, i.e. it starts inside a group.
fn leading_group_has_text(inls: &[Inline<'_>]) -> bool {
    match inls.first() {
        Some(Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x)) => !is_blank_inlines(x),
        _ => false,
    }
}

/// Splits a paragraph's inlines at every `\par`, at any group depth. Each piece keeps the
/// containers (groups, font changes) its material sits in, and records the group-scoped
/// alignment in effect at its end and at its start. `in_group` says whether `items` is a
/// group's content, whose declarations end with the group; declarations at the top level
/// of the paragraph persist and are read by the paragraph layout itself.
fn split_at_par_end<'a>(
    items: &[Inline<'a>],
    inherited: Option<crate::frontend::ast::ParAlign>,
    in_group: bool,
) -> Vec<ParPiece<'a>> {
    let mut pieces = vec![ParPiece { inls: Vec::new(), end_align: None, start_align: inherited, end_size: None }];
    let mut cur = inherited;
    let mut cur_size = None;
    for item in items {
        match item {
            Inline::ParEnd => {
                let last = pieces.last_mut().unwrap();
                last.end_align = if in_group { cur } else { None };
                last.end_size = if in_group { cur_size } else { None };
                pieces.push(ParPiece { inls: Vec::new(), end_align: None, start_align: if in_group { cur } else { None }, end_size: None });
            }
            Inline::Align(a) if in_group => {
                cur = Some(*a);
                pieces.last_mut().unwrap().inls.push(item.clone());
            }
            Inline::FontSize(fs) if in_group => {
                cur_size = Some(*fs);
                pieces.last_mut().unwrap().inls.push(item.clone());
            }
            Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x) if inlines_have_par_end(x) => {
                let wrap = |v: Vec<Inline<'a>>| match item {
                    Inline::Bold(_) => Inline::Bold(v),
                    Inline::Italic(_) => Inline::Italic(v),
                    Inline::Mono(_) => Inline::Mono(v),
                    _ => Inline::Group(v),
                };
                let sub = split_at_par_end(x, if in_group { cur } else { None }, true);
                let n = sub.len();
                for (k, sp) in sub.into_iter().enumerate() {
                    if k > 0 {
                        pieces.push(ParPiece { inls: Vec::new(), end_align: None, start_align: sp.start_align, end_size: None });
                    }
                    let last = pieces.last_mut().unwrap();
                    if k + 1 < n {
                        last.end_align = sp.end_align;
                        last.end_size = sp.end_size.or(cur_size);
                    }
                    last.inls.push(wrap(sp.inls));
                }
            }
            _ => pieces.last_mut().unwrap().inls.push(item.clone()),
        }
    }
    pieces
}

/// Whether the paragraph opens with an alignment declaration (possibly inside a leading
/// group): it ran in vertical mode, so the paragraph starts with \parindent 0pt.
fn starts_with_align(inls: &[Inline<'_>]) -> bool {
    for inl in inls {
        match inl {
            Inline::Text(s) if s.trim().is_empty() => continue,
            Inline::Align(_) => return true,
            Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x) => return starts_with_align(x),
            _ => return false,
        }
    }
    false
}

/// `inls` split at its top-level `\\` (`Inline::Break`): the rows of a tabular cell.
fn split_top_level_rows<'s, 'b>(inls: &'b [Inline<'s>]) -> Vec<&'b [Inline<'s>]> {
    let mut rows = Vec::new();
    let mut start = 0;
    for (i, inl) in inls.iter().enumerate() {
        if matches!(inl, Inline::Break) {
            rows.push(&inls[start..i]);
            start = i + 1;
        }
    }
    rows.push(&inls[start..]);
    rows
}

/// latex.ltx `\tabcolsep`.
const TABCOLSEP_PT: f32 = 6.0;

pub(crate) fn inlines_have_break(inls: &[Inline<'_>]) -> bool {
    inls.iter().any(|inl| match inl {
        Inline::Break | Inline::ParEnd => true,
        Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x) => {
            inlines_have_break(x)
        }
        _ => false,
    })
}

/// One character of a listing as `listings` prints it.
struct ListingGlyph<'a> {
    face: Face,
    /// The text to show; `None` is OT1's rule-drawn underscore.
    text: Option<&'a str>,
    width: f32,
}

/// `listings` prints `{ } | \ * - < > _` with `\textbraceleft` and the like; in OT1
/// (Computer Modern text) those come from the math fonts (cmsy, cmmi) or, for the
/// underscore, a rule. Every other character is the font's own.
fn listing_glyph<'a>(ch: &'a str, face: Face, family: metrics::FontFamily, size: f32) -> ListingGlyph<'a> {
    if family == metrics::FontFamily::ComputerModern && face == Face::Roman {
        let sym = |code: u8, face: Face| {
            let m = if face == Face::Symbol { crate::math::math_tfm::cmsy10_metric(code) } else { crate::math::math_tfm::cmmi10_metric(code) };
            ListingGlyph { face, text: Some(crate::math::math_delim::byte_str(code)), width: m.w * size * 0.001 }
        };
        match ch {
            "{" => return sym(0x66, Face::Symbol),
            "}" => return sym(0x67, Face::Symbol),
            "|" => return sym(0x6A, Face::Symbol),
            "\\" => return sym(0x6E, Face::Symbol),
            "*" => return sym(0x03, Face::Symbol),
            "-" => return sym(0x00, Face::Symbol),
            "<" => return sym(0x3C, Face::MathItalic),
            ">" => return sym(0x3E, Face::MathItalic),
            "_" => return ListingGlyph { face, text: None, width: 0.36 * crate::backend::scaled::quad_pt(family, size) },
            _ => {}
        }
    }
    ListingGlyph { face, text: Some(ch), width: crate::backend::shape::word_width_pt(ch, face, size, family) }
}

/// LaTeX's `\check@icr` at the end of a text font command whose words start at `first`:
/// `\maybe@ic` runs after the group, so when the font there (`outer`) is upright and no
/// period or comma follows (`\nocorrlist`), `\/` adds the italic correction of the last
/// character, whatever its font (cmbx10's `y` has one too). Returns the width added.
#[allow(clippy::too_many_arguments)]
fn check_icr(
    soa: &mut ParagraphSoA<'_>,
    first: usize,
    next: Option<&Inline<'_>>,
    outer: Face,
    family: metrics::FontFamily,
    size: f32,
    x: &[Inline<'_>],
    inner: Face,
) -> f32 {
    let next_nocorr = matches!(next, Some(Inline::Text(t)) if t.starts_with(['.', ',']));
    if next_nocorr || crate::backend::scaled::has_slant(family, outer, size) || soa.len() <= first {
        return 0.0;
    }
    let last = soa.len() - 1;
    let attr = soa.attrs[last];
    let ic = if attr.is_word_like() && soa.texts[last] != UNDERSCORE_TEXT {
        crate::backend::shape::word_italic_correction_pt(soa.texts[last], attr.face(), size, family)
    } else if let Some(ch) = trailing_logo_char(x) {
        crate::backend::scaled::italic_correction_pt(family, inner, size, ch)
    } else {
        return 0.0;
    };
    soa.add_italic_correction(last, ic);
    ic
}

/// The character a logo ends with when it is the last thing `inls` typesets (empty groups
/// add nothing): latex.ltx's `\TeX` and `\LaTeX` end in a bare `X`, so `\/` after them uses
/// its italic correction.
fn trailing_logo_char(inls: &[Inline<'_>]) -> Option<char> {
    for inl in inls.iter().rev() {
        match inl {
            Inline::Group(x) | Inline::Italic(x) | Inline::Bold(x) | Inline::Mono(x) if x.iter().all(|i| matches!(i, Inline::Group(g) if g.is_empty())) => continue,
            Inline::Group(x) | Inline::Italic(x) | Inline::Bold(x) | Inline::Mono(x) => return trailing_logo_char(x),
            Inline::Logo(crate::frontend::ast::LogoKind::TeX | crate::frontend::ast::LogoKind::LaTeX) => return Some('X'),
            _ => return None,
        }
    }
    None
}

/// LaTeX's `\check@icl` (`\maybe@ic`, `\sw@slant`) at the start of the text font command
/// group `x`: when the new font is upright and its text does not start with `.` or `,`,
/// the italic correction of the character before is put back in front of the space
/// between them. Returns the width added.
fn check_icl(soa: &mut ParagraphSoA<'_>, x: &[Inline<'_>], face: Face, family: metrics::FontFamily, size: f32) -> f32 {
    let (new_face, arg) = match x.get(1) {
        Some(Inline::FontDecl(d)) => (d.apply(face, family), &x[2..]),
        Some(Inline::Bold(a)) => (face.with_bold(true), &a[..]),
        Some(Inline::Mono(a)) => (face.with_mono(true), &a[..]),
        Some(Inline::ScShape) => (Face::SmallCaps, &x[2..]),
        _ => return 0.0,
    };
    if crate::backend::scaled::has_slant(family, new_face, size) || soa.is_empty() {
        return 0.0;
    }
    match first_text_char(arg) {
        None | Some('.' | ',') => return 0.0,
        _ => {}
    }
    let mut w = soa.len() - 1;
    if soa.attrs[w].is_space() {
        if w == 0 {
            return 0.0;
        }
        w -= 1;
    }
    let attr = soa.attrs[w];
    if !attr.is_word_like() {
        return 0.0;
    }
    let ic = crate::backend::shape::word_italic_correction_pt(soa.texts[w], attr.face(), size, family);
    soa.add_italic_correction(w, ic);
    ic
}

/// The first character the inlines typeset, looking into groups.
fn first_text_char(inls: &[Inline<'_>]) -> Option<char> {
    for inl in inls {
        match inl {
            Inline::Text(t) => {
                if let Some(c) = t.chars().next() {
                    return Some(c);
                }
            }
            Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x) => {
                if let Some(c) = first_text_char(x) {
                    return Some(c);
                }
            }
            Inline::FontDecl(_) | Inline::TextFontCommand | Inline::ScShape | Inline::Color(_) | Inline::FontSize(_) => {}
            _ => return Some('\0'),
        }
    }
    None
}

#[allow(dead_code)]
fn explode_inlines_into<'a, 'doc>(
    inls: &'doc [Inline<'a>],
    face: Face,
    size: f32,
    soa: &mut ParagraphSoA<'a>,
    math_collector: &mut Vec<&'doc crate::math::MathNode<'a>>,
) -> f32 {
    explode_inlines_into_family(inls, face, size, soa, metrics::FontFamily::Times, math_collector)
}

pub(crate) fn explode_inlines_into_family<'a, 'doc>(
    inls: &'doc [Inline<'a>],
    face: Face,
    size: f32,
    soa: &mut ParagraphSoA<'a>,
    family: metrics::FontFamily,
    math_collector: &mut Vec<&'doc crate::math::MathNode<'a>>,
) -> f32 {
    let mut space_factor = 1000u16;
    explode_inlines_into_family_sf(
        inls,
        face,
        size,
        soa,
        family,
        math_collector,
        &mut space_factor,
        0,
    )
}

/// `sf_mode` bit: `\frenchspacing` (every `\sfcode` 1000).
pub(crate) const SF_FRENCH: u8 = 1;
/// `sf_mode` bit: `\sfcode`\.=1000`, as `thebibliography` sets it.
pub(crate) const SF_PERIOD_1000: u8 = 2;

/// The space factor after `word` when `\sfcode`\.` is 1000: a period that the word
/// ends with, behind transparent closers (sfcode 0), sets 1000 instead of 3000.
#[inline]
fn period_sf(word: &str, sf: u16, sf_mode: u8) -> u16 {
    if sf_mode & SF_PERIOD_1000 != 0 && word.trim_end_matches([')', '\'', ']']).ends_with('.') {
        1000
    } else {
        sf
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn explode_inlines_into_family_sf<'a, 'doc>(
    inls: &'doc [Inline<'a>],
    face: Face,
    size: f32,
    soa: &mut ParagraphSoA<'a>,
    family: metrics::FontFamily,
    math_collector: &mut Vec<&'doc crate::math::MathNode<'a>>,
    space_factor: &mut u16,
    sf_mode: u8,
) -> f32 {
    let scale = size * 0.001;
    let space_glue = crate::backend::scaled::space_glue(family, face, size);
    let table = metrics::resolve_width_table_at(family, face, size);
    let flat_table = FlatAsciiWidthTable::from_table(table);
    let shaper = crate::backend::shape::text_font_at(family, face, size);
    // Exact (TFM) widths need the whole word; the unit table only serves fonts without them.
    let exact_widths = shaper.is_some() || crate::backend::scaled::fix_metrics(family, face, size).is_some();
    // Word width in points: the font's ligature/kern program when the face has one (TeX builds
    // ligatures and implicit kerns while it reads the characters), plain advances otherwise.
    // The shaper resolved above serves every word, instead of being looked up per word.
    let word_width = |word: &str| -> f32 {
        match &shaper {
            Some(font) => font.width_pt(word, size),
            None => crate::backend::shape::word_width_pt(word, face, size, family),
        }
    };
    let mut total_w = 0.0;

    for (idx, inl) in inls.iter().enumerate() {
        match inl {
            Inline::Text(s) if *s == UNDERSCORE_TEXT => {
                // A box: the space factor becomes 1000.
                *space_factor = 1000;
                let w = 0.36 * crate::backend::scaled::face_quad_pt(family, face, size);
                soa.push(s, face, w, WordKind::Word);
                total_w += w;
            }
            Inline::Text(s) if *s == TIE_TEXT || *s == CONTROL_SPACE_TEXT => {
                // `\ ` (and `~`, which is `\nobreak\ `) sets the glue of space factor 1000.
                let (gw, gs, gk) = space_glue.at_space_factor(1000);
                let w = crate::backend::scaled::to_pt(gw as i64);
                soa.push_space(s, face, (w, crate::backend::scaled::to_pt(gs as i64), crate::backend::scaled::to_pt(gk as i64)));
                total_w += w;
            }
            Inline::Text(s) => {
                let bytes = s.as_bytes();
                let len = bytes.len();
                let mut i = 0;
                while i < len {
                    let b = bytes[i];
                    if is_tex_space_byte(b) {
                        let text = if b == b' ' { &s[i..i + 1] } else { " " };
                        let (gw, gs, gk) = space_glue.at_space_factor(*space_factor);
                        let current_space_w = crate::backend::scaled::to_pt(gw as i64);
                        soa.push_space(
                            text,
                            face,
                            (current_space_w, crate::backend::scaled::to_pt(gs as i64), crate::backend::scaled::to_pt(gk as i64)),
                        );
                        total_w += current_space_w;
                        i += 1;
                        while i < len && is_tex_space_byte(bytes[i]) {
                            i += 1;
                        }
                    } else {
                        let start = i;
                        let mut w_sf = *space_factor;
                        let mut word_units = 0u32;
                        let mut is_simple_ascii = true;

                        while i < len {
                            if is_simple_ascii && i + 16 <= len && crate::layout::layout_hz::is_all_ascii_lowercase_16(&bytes[i..i + 16]) {
                                if exact_widths {
                                    // Shaped faces measure the whole word below.
                                    w_sf = 1000;
                                    i += 16;
                                    continue;
                                }
                                let chunk = &bytes[i..i + 16];
                                word_units += flat_table.get_advance_units(chunk[0]) as u32
                                    + flat_table.get_advance_units(chunk[1]) as u32
                                    + flat_table.get_advance_units(chunk[2]) as u32
                                    + flat_table.get_advance_units(chunk[3]) as u32
                                    + flat_table.get_advance_units(chunk[4]) as u32
                                    + flat_table.get_advance_units(chunk[5]) as u32
                                    + flat_table.get_advance_units(chunk[6]) as u32
                                    + flat_table.get_advance_units(chunk[7]) as u32
                                    + flat_table.get_advance_units(chunk[8]) as u32
                                    + flat_table.get_advance_units(chunk[9]) as u32
                                    + flat_table.get_advance_units(chunk[10]) as u32
                                    + flat_table.get_advance_units(chunk[11]) as u32
                                    + flat_table.get_advance_units(chunk[12]) as u32
                                    + flat_table.get_advance_units(chunk[13]) as u32
                                    + flat_table.get_advance_units(chunk[14]) as u32
                                    + flat_table.get_advance_units(chunk[15]) as u32;
                                w_sf = 1000;
                                i += 16;
                                continue;
                            }
                            if is_simple_ascii && i + 8 <= len && crate::layout::layout_hz::is_all_ascii_lowercase_8(&bytes[i..i + 8]) {
                                if exact_widths {
                                    w_sf = 1000;
                                    i += 8;
                                    continue;
                                }
                                let chunk = &bytes[i..i + 8];
                                word_units += flat_table.get_advance_units(chunk[0]) as u32
                                    + flat_table.get_advance_units(chunk[1]) as u32
                                    + flat_table.get_advance_units(chunk[2]) as u32
                                    + flat_table.get_advance_units(chunk[3]) as u32
                                    + flat_table.get_advance_units(chunk[4]) as u32
                                    + flat_table.get_advance_units(chunk[5]) as u32
                                    + flat_table.get_advance_units(chunk[6]) as u32
                                    + flat_table.get_advance_units(chunk[7]) as u32;
                                w_sf = 1000;
                                i += 8;
                                continue;
                            }

                            let cb = bytes[i];
                            if is_tex_space_byte(cb) {
                                break;
                            }
                            if is_simple_ascii {
                                if cb < 128 && cb != b'-' {
                                    word_units += flat_table.get_advance_units(cb) as u32;
                                    if sf_mode & SF_FRENCH == 0 && (!cb.is_ascii_lowercase() || w_sf != 1000) {
                                        w_sf = crate::layout::layout_hz::update_space_factor(w_sf, cb, false);
                                    }
                                } else {
                                    is_simple_ascii = false;
                                }
                            }
                            i += 1;
                        }

                        let word = &s[start..i];
                        if is_simple_ascii {
                            if sf_mode & SF_FRENCH == 0 {
                                *space_factor = period_sf(word, w_sf, sf_mode);
                            }
                            let width = if exact_widths { word_width(word) } else { word_units as f32 * scale };
                            soa.push(word, face, width, WordKind::Word);
                            total_w += width;
                        } else {
                            let mut fb_sf = *space_factor;
                            let mut has_hyphen = false;
                            let mut has_soft_hyphen = false;
                            let w_bytes = word.as_bytes();
                            let w_len = w_bytes.len();
                            for idx in 0..w_len {
                                let cb = w_bytes[idx];
                                if cb == b'-' {
                                    has_hyphen = true;
                                } else if cb == 0xC2 && idx + 1 < w_len && w_bytes[idx + 1] == 0xAD {
                                    has_soft_hyphen = true;
                                }
                                if sf_mode & SF_FRENCH == 0
                                    && (!cb.is_ascii_lowercase() || fb_sf != 1000) {
                                        fb_sf = crate::layout::layout_hz::update_space_factor(fb_sf, cb, false);
                                    }
                            }
                            if sf_mode & SF_FRENCH == 0 {
                                *space_factor = period_sf(word, fb_sf, sf_mode);
                            }

                            if !has_hyphen && !has_soft_hyphen {
                                let width = word_width(word);
                                soa.push(word, face, width, WordKind::Word);
                                total_w += width;
                            } else if word == "\u{00AD}" {
                                if !soa.is_empty() {
                                    let last_idx = soa.len() - 1;
                                    if soa.attrs[last_idx].kind() == WordKind::Word {
                                        soa.attrs[last_idx] = PackedWordAttr::new(WordKind::Hyphen, soa.attrs[last_idx].face());
                                    }
                                }
                            } else if has_soft_hyphen {
                                let mut last = 0;
                                for (shy_pos, _) in word.match_indices('\u{00AD}') {
                                    if shy_pos > last {
                                        let piece = &word[last..shy_pos];
                                        let p_w = word_width(piece);
                                        soa.push(piece, face, p_w, WordKind::Hyphen);
                                        total_w += p_w;
                                    }
                                    last = shy_pos + '\u{00AD}'.len_utf8();
                                }
                                if last < word.len() {
                                    let piece = &word[last..];
                                    let p_w = word_width(piece);
                                    soa.push(piece, face, p_w, WordKind::Word);
                                    total_w += p_w;
                                }
                            } else if has_hyphen && word.len() > 1 && !word.ends_with('-') {
                                let mut last = 0;
                                for (hyphen_pos, _) in word.match_indices('-') {
                                    // TeX tests for the hyphen character before it packs a
                                    // ligature (§1035 `wrapup`), so an en or em dash (`--`, `---`)
                                    // is followed by a discretionary too; inside a run of hyphens
                                    // there is none.
                                    if word.as_bytes().get(hyphen_pos + 1) == Some(&b'-') {
                                        continue;
                                    }
                                    let piece = &word[last..=hyphen_pos];
                                    let p_w = word_width(piece);
                                    soa.push(piece, face, p_w, WordKind::Hyphen);
                                    total_w += p_w;
                                    last = hyphen_pos + 1;
                                }
                                if last < word.len() {
                                    let piece = &word[last..];
                                    let p_w = word_width(piece);
                                    soa.push(piece, face, p_w, WordKind::Word);
                                    total_w += p_w;
                                }
                            } else {
                                let width = word_width(word);
                                let kind = if word.ends_with('-') || word.ends_with('\u{00AD}') || word.ends_with('\u{2010}') {
                                    WordKind::Hyphen
                                } else {
                                    WordKind::Word
                                };
                                soa.push(word, face, width, kind);
                                total_w += width;
                            }
                        }
                    }
                }
            }
            Inline::Bold(x) => {
                let color = soa.current_color();
                total_w += explode_inlines_into_family_sf(x, face.with_bold(true), size, soa, family, math_collector, space_factor, sf_mode);
                soa.set_color(color);
                soa.set_size(size, size);
            }
            Inline::Italic(x) => {
                let color = soa.current_color();
                let first = soa.len();
                let it_face = face.with_italic(true);
                total_w += explode_inlines_into_family_sf(x, it_face, size, soa, family, math_collector, space_factor, sf_mode);
                total_w += check_icr(soa, first, inls.get(idx + 1), face, family, size, x, it_face);
                soa.set_color(color);
                soa.set_size(size, size);
            }
            Inline::Mono(x) => {
                let color = soa.current_color();
                total_w += explode_inlines_into_family_sf(x, face.with_mono(true), size, soa, family, math_collector, space_factor, sf_mode);
                soa.set_color(color);
                soa.set_size(size, size);
            }
            Inline::Group(x) => {
                let color = soa.current_color();
                // `\emph`, `\textsl` and the other text font commands start with
                // `\check@icl` and end with `\check@icr`.
                let text_cmd = matches!(x.first(), Some(Inline::TextFontCommand));
                if text_cmd {
                    total_w += check_icl(soa, x, face, family, size);
                }
                let first = soa.len();
                total_w += explode_inlines_into_family_sf(x, face, size, soa, family, math_collector, space_factor, sf_mode);
                if text_cmd {
                    let inner = match x.get(1) {
                        Some(Inline::FontDecl(d)) => d.apply(face, family),
                        Some(Inline::Bold(_)) => face.with_bold(true),
                        Some(Inline::Mono(_)) => face.with_mono(true),
                        _ => face,
                    };
                    total_w += check_icr(soa, first, inls.get(idx + 1), face, family, size, x, inner);
                }
                soa.set_color(color);
                soa.set_size(size, size);
            }
            Inline::Color(c) => soa.set_color(*c),
            Inline::ScShape => {
                // The rest of the group is set in small caps (cmcsc10); the Base-14 families
                // have no small-caps face, so they keep the upright one.
                let sc = if family == metrics::FontFamily::ComputerModern { Face::SmallCaps } else { face };
                total_w += explode_inlines_into_family_sf(&inls[idx + 1..], sc, size, soa, family, math_collector, space_factor, sf_mode);
                break;
            }
            Inline::FontDecl(d) => {
                total_w += explode_inlines_into_family_sf(&inls[idx + 1..], d.apply(face, family), size, soa, family, math_collector, space_factor, sf_mode);
                break;
            }
            Inline::SpaceFactor(f) => *space_factor = *f,
            Inline::TextFontCommand => {}
            Inline::EmSpace { em, breakable } => {
                // `em` is the current font's quad (`\fontdimen6`): cmtt10's is 1.05em.
                let quad = crate::backend::scaled::font_dimen_sp(family, face, size, 6);
                let w = pt64(crate::backend::scaled::em_times(*em, quad));
                if *breakable {
                    if w > 0.0 {
                        soa.push("", face, w, WordKind::Space);
                    }
                } else {
                    // A kern: an empty box, so no break and no stretch.
                    soa.push("", face, w, WordKind::Word);
                }
                total_w += w;
            }
            Inline::FontDimenKern(_) | Inline::Kern(_) => {
                let w = match *inl {
                    Inline::FontDimenKern(n) => pt64(crate::backend::scaled::font_dimen_sp(family, face, size, n)),
                    Inline::Kern(w) => pt64(w as i64),
                    _ => 0.0,
                };
                soa.push("", face, w, WordKind::Word);
                total_w += w;
            }
            Inline::FontSize(fs) => {
                // The rest of the group is set at the new size.
                let class = if soa.class_size > 0.0 { soa.class_size } else { FONT_SIZE };
                let new_size = fs.metrics(class).0;
                soa.set_size(new_size, size);
                total_w += explode_inlines_into_family_sf(&inls[idx + 1..], face, new_size, soa, family, math_collector, space_factor, sf_mode);
                break;
            }
            Inline::Break | Inline::ParEnd => {
                *space_factor = 1000;
                soa.push("", face, 0.0, WordKind::Break);
            }
            Inline::HSpace(dim) => {
                if *dim > 0.0 {
                    soa.push("", face, *dim, WordKind::Space);
                    total_w += *dim;
                }
            }
            Inline::HGlue(w, stretch, shrink) => {
                soa.push_space("", face, (*w, *stretch, *shrink));
                total_w += *w;
            }
            Inline::HFil(stretch, order) => {
                let nobreak = order & crate::frontend::ast::HFIL_NOBREAK != 0;
                soa.fils.push((soa.widths.len() as u32, *stretch, order & 3));
                // A tie's space carries the line breaker's \nobreak.
                soa.push(if nobreak { TIE_TEXT } else { "" }, face, 0.0, WordKind::Space);
            }
            Inline::Unskip => total_w -= soa.unskip(),
            Inline::DisplayFollows | Inline::DisplayResumes | Inline::ShortTitleEnd | Inline::PageLabel(_) => {}
            Inline::ItalicCorrection => {
                // §1113: when the list ends in a character, a kern of its italic correction.
                if let Some(last) = soa.len().checked_sub(1) {
                    let attr = soa.attrs[last];
                    if attr.is_word_like() && soa.texts[last] != UNDERSCORE_TEXT {
                        let ic = crate::backend::shape::word_italic_correction_pt(soa.texts[last], attr.face(), size, family);
                        soa.add_italic_correction(last, ic);
                    }
                }
            }
            Inline::VAdjust { pt, em, ex } => {
                // A zero-width marker in the line; its skip is set below the line.
                let ex_pt = crate::backend::scaled::fix_metrics(family, face, size)
                    .map_or(0.5 * size, |m| crate::backend::scaled::to_pt(crate::backend::scaled::Scaler::at_pt(size).scale(m.params[4]) as i64));
                let skip = pt + em * crate::backend::scaled::face_quad_pt(family, face, size) + ex * ex_pt;
                soa.vadjusts.push((soa.widths.len() as u32, skip));
                soa.push("", face, 0.0, WordKind::Word);
            }
            Inline::Math(_) | Inline::Footnote(_) | Inline::Logo(_) => {
                // A footnote contributes its mark; its text goes to the foot of the page.
                let node: &'doc crate::math::MathNode<'a> = match inl {
                    Inline::Math(n) => n,
                    Inline::Footnote(f) => &f.mark,
                    Inline::Logo(latex) => logo_node(*latex, face, size),
                    _ => unreachable!(),
                };
                *space_factor = 1000;
                let has_breakable_atoms = if let crate::math::MathNode::Row(children) = node {
                    if children.len() > 1 {
                        let mut kinds = [crate::math::MathAtomKind::Ord; 64];
                        let count = children.len().min(64);
                        for (i, c) in children[..count].iter().enumerate() {
                            kinds[i] = c.atom_kind_in_style(crate::math::MathStyle::Text);
                        }
                        crate::math::math_spacing::normalize_atom_kinds(&mut kinds[..count]);
                        (0..count.saturating_sub(1)).any(|i| {
                            (kinds[i] == crate::math::MathAtomKind::Bin || kinds[i] == crate::math::MathAtomKind::Rel)
                                && kinds[i + 1] != crate::math::MathAtomKind::Rel
                        })
                    } else {
                        false
                    }
                } else {
                    false
                };

                if has_breakable_atoms {
                    if let crate::math::MathNode::Row(children) = node {
                        let count = children.len();
                        // Stack buffer for atom kinds to ensure zero allocations on hot path
                        let mut stack_kinds = [crate::math::MathAtomKind::Ord; 64];
                        let mut heap_kinds;
                        let kinds: &mut [crate::math::MathAtomKind] = if count <= 64 {
                            &mut stack_kinds[..count]
                        } else {
                            heap_kinds = vec![crate::math::MathAtomKind::Ord; count];
                            &mut heap_kinds[..]
                        };
                        for (i, c) in children.iter().enumerate() {
                            kinds[i] = c.atom_kind_in_style(crate::math::MathStyle::Text);
                        }
                        crate::math::math_spacing::normalize_atom_kinds(&mut kinds[..count]);
                        // The math quad at this size, once per formula rather than per atom.
                        let math_quad = crate::math::math_fonts::MathParams::at(family, size).quad;

                        for (i, child) in children.iter().enumerate() {
                            // Math glue that can stretch (`\colon`'s `\mskip6mu plus1mu`).
                            let glue = match child {
                                crate::math::MathNode::Space(crate::math::math_ast::MathSkip::MuGlue(w, st)) if i > 0 => {
                                    let quad = math_quad;
                                    Some((crate::backend::scaled::mu_pt(*w, quad), crate::backend::scaled::mu_pt(*st, quad)))
                                }
                                _ => None,
                            };
                            if let Some((w, st)) = glue {
                                soa.push_space("", Face::Roman, (w, st, 0.0));
                                let last = soa.attrs.len() - 1;
                                soa.attrs[last] = PackedWordAttr::new(WordKind::MathSpace, Face::Roman);
                                total_w += w;
                            }
                            let mb = crate::math::measure_math_in(child, size, family, crate::math::MathStyle::Text, face);
                            let math_idx = math_collector.len() as u32;
                            math_collector.push(child);

                            let atom_kind = kinds[i];
                            let is_not_last = i + 1 < count;
                            let next_not_rel = is_not_last && kinds[i + 1] != crate::math::MathAtomKind::Rel;

                            let word_kind = if atom_kind == crate::math::MathAtomKind::Bin && is_not_last && next_not_rel {
                                WordKind::MathBin
                            } else if atom_kind == crate::math::MathAtomKind::Rel && is_not_last && next_not_rel {
                                WordKind::MathRel
                            } else {
                                WordKind::Math
                            };

                            // TeX §752 between this atom and the one before, when no glue
                            // separates them.
                            if glue.is_none() && i > 0 {
                                let adj = crate::math::inline_pair_adjust(&children[i - 1], child, size, family, face);
                                if adj != 0.0 {
                                    if let Some(last) = soa.widths.last_mut() {
                                        *last += adj;
                                    }
                                    total_w += adj;
                                }
                            }
                            if glue.is_none() {
                                soa.push_math_atom(mb, math_idx, word_kind);
                                soa.set_last_face(face);
                                total_w += mb.w;
                            }

                            if is_not_last {
                                // \thinmuskip 3mu, \medmuskip 4mu plus 2mu minus 4mu, \thickmuskip
                                // 5mu plus 5mu: glue the line may stretch or shrink (§716).
                                let mu_count = crate::math::math_spacing::atom_spacing_mu(atom_kind, kinds[i + 1], true);
                                let quad = if family == metrics::FontFamily::ComputerModern {
                                    math_quad
                                } else {
                                    size
                                };
                                let space = if family == metrics::FontFamily::ComputerModern {
                                    let quad_sp = crate::math::math_box::ScaledPoints::from_pt(quad);
                                    crate::math::math_spacing::atom_spacing_sp(
                                        atom_kind,
                                        kinds[i + 1],
                                        true,
                                        quad_sp,
                                    )
                                    .to_pt()
                                } else {
                                    crate::math::math_spacing::atom_spacing(
                                        atom_kind,
                                        kinds[i + 1],
                                        false,
                                        true,
                                    ) * size
                                };
                                if space > 0.0 {
                                    let mu = |n: f32| crate::backend::scaled::mu_pt(n, quad);
                                    let (stretch, shrink) = match mu_count {
                                        4 => (mu(2.0), mu(4.0)),
                                        5 => (mu(5.0), 0.0),
                                        _ => (0.0, 0.0),
                                    };
                                    soa.push_space("", Face::Roman, (space, stretch, shrink));
                                    let last = soa.attrs.len() - 1;
                                    soa.attrs[last] = PackedWordAttr::new(WordKind::MathSpace, Face::Roman);
                                    total_w += space;
                                }
                            }
                        }
                    }
                } else {
                    let mb = crate::math::measure_math_in(node, size, family, crate::math::MathStyle::Text, face);
                    let math_idx = math_collector.len() as u32;
                    math_collector.push(node);
                    soa.push_math_atom(mb, math_idx, WordKind::Math);
                    soa.set_last_face(face);
                    total_w += mb.w;
                }
            }
            Inline::NoIndent | Inline::Indent | Inline::HangIndent(_) | Inline::HangAfter(_) | Inline::ParShape(_)
            | Inline::PreTolerance(_) | Inline::Tolerance(_) | Inline::EmergencyStretch(_)
            | Inline::Looseness(_) | Inline::LeftSkip(_) | Inline::RightSkip(_)
            | Inline::ParFillSkip { .. } | Inline::PrevGraf(_) | Inline::LastLineFit(_)
            | Inline::Sloppy | Inline::Fussy | Inline::Align(_) => {}
        }
    }
    total_w
}

#[inline(always)]
fn is_tex_space_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

#[allow(dead_code)]
fn measure_inlines(inls: &[Inline<'_>]) -> f32 {
    measure_inlines_face_size(inls, Face::Roman, FONT_SIZE)
}

#[allow(dead_code)]
fn measure_inlines_face_size(inls: &[Inline<'_>], face: Face, size: f32) -> f32 {
    measure_inlines_face_size_family(inls, face, size, metrics::FontFamily::Times)
}

fn measure_inlines_face_size_family(
    inls: &[Inline<'_>],
    face: Face,
    size: f32,
    family: metrics::FontFamily,
) -> f32 {
    let mut sf = 1000u16;
    measure_inlines_pt(inls, face, size, family, &mut sf)
}

/// Natural width in points of `inls` set on one line, with the widths and interword glue
/// that `explode_inlines_into_family_sf` gives them (TFM widths, space factor, the italic
/// correction of `\textit`), so a measured box holds exactly what is later set in it.
fn measure_inlines_pt(
    inls: &[Inline<'_>],
    face: Face,
    size: f32,
    family: metrics::FontFamily,
    sf: &mut u16,
) -> f32 {
    use crate::backend::scaled::{space_glue, to_pt};
    let glue = space_glue(family, face, size);
    let mut total = 0.0f32;
    for (idx, inl) in inls.iter().enumerate() {
        match inl {
            Inline::Text(s) => {
                let bytes = s.as_bytes();
                let mut i = 0;
                while i < bytes.len() {
                    if is_tex_space_byte(bytes[i]) {
                        while i < bytes.len() && is_tex_space_byte(bytes[i]) {
                            i += 1;
                        }
                        total += to_pt(glue.at_space_factor(*sf).0 as i64);
                    } else {
                        let start = i;
                        while i < bytes.len() && !is_tex_space_byte(bytes[i]) {
                            *sf = crate::layout::layout_hz::update_space_factor(*sf, bytes[i], false);
                            i += 1;
                        }
                        total += crate::backend::shape::word_width_pt(&s[start..i], face, size, family);
                    }
                }
            }
            Inline::Bold(x) => total += measure_inlines_pt(x, face.with_bold(true), size, family, sf),
            Inline::Italic(x) => {
                total += measure_inlines_pt(x, face.with_italic(true), size, family, sf);
                let next_nocorr = matches!(inls.get(idx + 1), Some(Inline::Text(t)) if t.starts_with(['.', ',']));
                let inner = face.with_italic(true);
                if !next_nocorr && !crate::backend::scaled::has_slant(family, face, size) {
                    if let Some(word) = last_text_word(x) {
                        total += crate::backend::shape::word_italic_correction_pt(word, inner, size, family);
                    } else if let Some(ch) = trailing_logo_char(x) {
                        total += crate::backend::scaled::italic_correction_pt(family, inner, size, ch);
                    }
                }
            }
            Inline::Mono(x) => total += measure_inlines_pt(x, face.with_mono(true), size, family, sf),
            Inline::ItalicCorrection => {
                if let Some(word) = idx.checked_sub(1).and_then(|p| last_text_word(&inls[p..idx])) {
                    total += crate::backend::shape::word_italic_correction_pt(word, face, size, family);
                }
            }
            Inline::Group(x) => {
                total += measure_inlines_pt(x, face, size, family, sf);
                let next_nocorr = matches!(inls.get(idx + 1), Some(Inline::Text(t)) if t.starts_with(['.', ',']));
                if let Some(Inline::TextFontCommand) = x.first() {
                    let inner = match x.get(1) {
                        Some(Inline::FontDecl(d)) => d.apply(face, family),
                        Some(Inline::Bold(_)) => face.with_bold(true),
                        Some(Inline::Mono(_)) => face.with_mono(true),
                        _ => face,
                    };
                    if !next_nocorr && !crate::backend::scaled::has_slant(family, face, size) {
                        if let Some(word) = last_text_word(x) {
                            total += crate::backend::shape::word_italic_correction_pt(word, inner, size, family);
                        } else if let Some(ch) = trailing_logo_char(x) {
                            total += crate::backend::scaled::italic_correction_pt(family, inner, size, ch);
                        }
                    }
                }
            }
            Inline::ScShape => {
                let sc = if family == metrics::FontFamily::ComputerModern { Face::SmallCaps } else { face };
                total += measure_inlines_pt(&inls[idx + 1..], sc, size, family, sf);
                break;
            }
            Inline::FontSize(fs) => {
                total += measure_inlines_pt(&inls[idx + 1..], face, fs.metrics(FONT_SIZE).0, family, sf);
                break;
            }
            Inline::FontDecl(d) => {
                total += measure_inlines_pt(&inls[idx + 1..], d.apply(face, family), size, family, sf);
                break;
            }
            Inline::EmSpace { em, .. } => {
                total += pt64(crate::backend::scaled::em_times(*em, crate::backend::scaled::font_dimen_sp(family, face, size, 6)))
            }
            Inline::FontDimenKern(n) => total += pt64(crate::backend::scaled::font_dimen_sp(family, face, size, *n)),
            Inline::Kern(w) => total += pt64(*w as i64),
            Inline::SpaceFactor(f) => *sf = *f,
            Inline::HSpace(dim) => total += dim.max(0.0),
            Inline::HGlue(w, ..) => total += *w,
            Inline::Math(node) => {
                *sf = 1000;
                total += crate::math::measure_math(node, size, family, crate::math::MathStyle::Text).w;
            }
            Inline::Footnote(f) => {
                *sf = 1000;
                total += crate::math::measure_math(&f.mark, size, family, crate::math::MathStyle::Text).w;
            }
            Inline::Logo(latex) => {
                *sf = 1000;
                total += crate::math::measure_math(logo_node(*latex, face, size), size, family, crate::math::MathStyle::Text).w;
            }
            Inline::Break | Inline::ParEnd => *sf = 1000,
            _ => {}
        }
    }
    total
}

/// The last character of the text at the end of `inls`, looking into nested groups.
fn last_text_word<'s>(inls: &[Inline<'s>]) -> Option<&'s str> {
    match inls.last()? {
        Inline::Text(t) if !t.ends_with(char::is_whitespace) => t.rsplit(char::is_whitespace).next(),
        Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x) => last_text_word(x),
        _ => None,
    }
}

#[allow(dead_code)]
fn measure_text_run_size(s: &str, face: Face, size: f32) -> f32 {
    measure_text_run_size_family(s, face, size, metrics::FontFamily::Times)
}

fn measure_text_run_size_family(
    s: &str,
    face: Face,
    size: f32,
    family: metrics::FontFamily,
) -> f32 {
    let mut total_units: f32 = 0.0;
    let space_units = metrics::resolve_space_units_at(family, face, size);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if is_tex_space_byte(b) {
            total_units += space_units;
            i += 1;
            while i < bytes.len() && is_tex_space_byte(bytes[i]) {
                i += 1;
            }
        } else {
            let start = i;
            i += 1;
            while i < bytes.len() && !is_tex_space_byte(bytes[i]) {
                i += 1;
            }
            let word = &s[start..i];
            total_units += crate::backend::shape::text_units_at(word, face, size, family);
        }
    }
    total_units * (size * 0.001)
}

#[inline]
/// Width of `\thepage` (arabic, roman upright) at `size`.
fn page_number_width(page_number: u32, family: metrics::FontFamily, size: f32) -> f32 {
    // Computer Modern digits share one TFM width.
    if let Some(m) = crate::backend::scaled::fix_metrics(family, Face::Roman, size) {
        let digit = crate::backend::scaled::Scaler::at_pt(size).scale(m.widths[b'0' as usize] as i32) as i64;
        return crate::backend::scaled::to_pt(count_digits(page_number) as i64 * digit);
    }
    let digit_units = metrics::resolve_digit_units_at(family, Face::Roman, size);
    (count_digits(page_number) * digit_units) as f32 * (size * 0.001)
}

pub(crate) fn count_digits(mut n: u32) -> u32 {
    if n == 0 {
        return 1;
    }
    let mut count = 0;
    while n > 0 {
        count += 1;
        n /= 10;
    }
    count
}

/// A display inside a paragraph, and the text resuming after it, belong to that paragraph:
/// no `\parskip` (or `\parsep`) comes before them.
fn continues_par(b: &Block<'_>) -> bool {
    matches!(b, Block::Equation { par: DisplayPar::Continues, .. })
        || matches!(b, Block::Para(inls) if inls.iter().take(2).any(|i| matches!(i, Inline::DisplayResumes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tex_parser::Inline;

    #[test]
    fn test_paragraph_content_hash_cache() {
        let config = DocumentConfig::default();
        let mut layouter = Layouter::new(&config);

        let para1 = vec![
            Inline::Text("The quick brown fox jumps over the lazy dog. "),
            Inline::Text("This is a test of the paragraph content-hash cache in RushTeX. "),
            Inline::Text("Repeated paragraphs should bypass Knuth-Plass dynamic programming entirely."),
        ];

        layouter.layout_para(&para1);
        assert!(!layouter.line_descriptors.is_empty());
        let desc1 = layouter.line_descriptors.clone();

        // Second identical paragraph: hits content-hash cache
        layouter.layout_para(&para1);
        let desc2 = layouter.line_descriptors.clone();

        // Must match 100% identically
        assert_eq!(desc1.len(), desc2.len());
        for (d1, d2) in desc1.iter().zip(desc2.iter()) {
            assert_eq!(d1.start_word, d2.start_word);
            assert_eq!(d1.word_count, d2.word_count);
            assert_eq!(d1.line_w(), d2.line_w());
        }

        // Third modified paragraph: produces different layout
        let para2 = vec![
            Inline::Text("A completely different short sentence."),
        ];
        layouter.layout_para(&para2);
        let desc3 = layouter.line_descriptors.clone();
        assert_ne!(desc1.len(), desc3.len());
    }
}
