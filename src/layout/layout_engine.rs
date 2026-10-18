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

use crate::doc_config::DocumentConfig;
use crate::font_metrics::{self as metrics, Face};
use crate::index::{pre_scan_entries, IndexRegistry};
use crate::layout::layout_hz::{
    build_breakpoints, hash_paragraph, hz_break_paragraph_breakpoints_pass,
    hz_break_paragraph_fast_shape, ragged_break_paragraph, try_layout_single_line_paragraph, Breakpoint, HzConfig, ParCacheEntry, PAR_CACHE_MAX_LINES,
};
use crate::frontend::ast::PackedColor;
use crate::tex_parser::{Block, Document, Graphic, Inline, TableData};

pub const PAR_CACHE_CAP: usize = 256;
pub const PAR_CACHE_MASK: usize = PAR_CACHE_CAP - 1;

pub const PAGE_W: f32 = 597.508;
pub const PAGE_H: f32 = 845.047;
pub const MARGIN: f32 = 72.27;
pub const FONT_SIZE: f32 = 10.0;
pub const LEADING: f32 = 12.0;

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

#[derive(Clone, Debug)]
pub struct Page<'a> {
    pub width: f32,
    pub height: f32,
    pub font_family: metrics::FontFamily,
    pub used_faces: u32,
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
    pub fn compute_used_faces(&self) -> u32 {
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

#[derive(Clone, Copy, Debug)]
pub enum Paint {
    GlyphRun {
        x: f32,
        y: f32,
        face: Face,
        size: f32,
        part_start: u32,
        part_count: u32,
        word_spacing: f32,
    },
    Marker {
        x: f32,
        y: f32,
        face: Face,
        size: f32,
        marker: Marker,
    },
    /// Axis-aligned filled or stroked rectangle. `y` is the bottom edge.
    Rule {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: bool,
    },
    CaptionTag {
        x: f32,
        y: f32,
        size: f32,
        n: u32,
    },
    TableCaptionTag {
        x: f32,
        y: f32,
        size: f32,
        n: u32,
    },
    PageNumber {
        x: f32,
        y: f32,
        face: Face,
        size: f32,
        page_number: u32,
    },
    SectionNumber {
        x: f32,
        y: f32,
        face: Face,
        size: f32,
        number: [u16; 4],
        level: u8,
    },
    FigureTag {
        x: f32,
        y: f32,
        face: Face,
        size: f32,
        n: u32,
    },
    TableTag {
        x: f32,
        y: f32,
        face: Face,
        size: f32,
        n: u32,
    },
    EquationNumber {
        x: f32,
        y: f32,
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
        x: f32,
        y: f32,
        size: f32,
        glyph_id: u16,
    },
    /// OpenType dynamic CID glyph emission rendered via composite CIDFont text operators.
    CidGlyphRun {
        x: f32,
        y: f32,
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

pub(crate) struct Layouter<'a> {
    pub(crate) pages: Vec<Page<'a>>,
    pub(crate) current_page: Page<'a>,
    pub(crate) y: f32,
    pub(crate) left: f32,
    pub(crate) right: f32,
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
    pub(crate) font_family: metrics::FontFamily,
    pub(crate) figure_counter: u32,
    pub(crate) table_counter: u32,
    pub(crate) equation_counter: u32,
    pub(crate) page_numbering: bool,
    pub(crate) par_indent: f32,
    pub(crate) par_skip: f32,
    pub(crate) hang_indent: f32,
    pub(crate) hang_after: i32,
    pub(crate) left_skip: f32,
    pub(crate) right_skip: f32,
    pub(crate) list_left_margin: f32,
    pub(crate) list_label_sep: f32,
    pub(crate) list_item_indent: f32,
    pub(crate) list_item_sep: f32,
    pub(crate) list_par_indent: f32,
    pub(crate) french_spacing: bool,
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
    /// Font size of the largest text on the previous line (for `\lineskip`).
    pub(crate) prev_line_size: f32,
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
        let y = config.page_height - config.margin_top - config.top_skip;
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
            page_numbering: config.page_numbering,
            par_indent: config.par_indent,
            par_skip: config.par_skip,
            hang_indent: config.hang_indent,
            hang_after: config.hang_after,
            left_skip: config.left_skip,
            right_skip: config.right_skip,
            list_left_margin: config.list_left_margin,
            list_label_sep: config.list_label_sep,
            list_item_indent: config.list_item_indent,
            list_item_sep: config.list_item_sep,
            list_par_indent: config.list_par_indent,
            french_spacing: config.french_spacing,
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
            par_end_size: None,
            leading_override: None,
            prev_line_size: config.base_font_size,
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
            pending_page: None,
            completed_pages: std::collections::VecDeque::with_capacity(2),
            completed_pages_count: 0,
            recycled_pages: Vec::with_capacity(4),
        }
    }

    #[inline]
    pub(crate) fn measure_inlines_face_size(
        &self,
        inls: &[Inline<'_>],
        face: Face,
        size: f32,
    ) -> f32 {
        measure_inlines_face_size_family(inls, face, size, self.font_family)
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
        foot_baseline: f32,
        text_span: (f32, f32),
    ) {
        if !page_numbering {
            return;
        }
        let digit_count = count_digits(page_number);
        let page_num_size = 10.0f32;
        let digit_units = metrics::resolve_digit_units(font_family, Face::Roman);
        let num_width = (digit_count * digit_units) as f32 * (page_num_size * 0.001);
        // The plain/article footer is `\hfil\thepage\hfil` in a box of \textwidth.
        let x = text_span.0 + (text_span.1 - text_span.0 - num_width) * 0.5;
        let y = foot_baseline;
        page.push_op(Paint::PageNumber {
            x,
            y,
            face: Face::Roman,
            size: page_num_size,
            page_number,
        });
    }

    #[inline(always)]
    pub(crate) fn add_page_number_to(&self, page: &mut Page<'a>, page_number: u32) {
        Self::add_page_number_to_page(
            page,
            page_number,
            self.page_numbering,
            self.font_family,
            self.foot_baseline(),
            (self.left, self.right),
        );
    }

    pub(crate) fn finish_streaming(&mut self) -> Vec<Page<'a>> {
        let mut last_pages = Vec::with_capacity(2);
        let page_numbering = self.page_numbering;
        let font_family = self.font_family;
        let margin_bottom = self.foot_baseline();
        if let Some(mut old_pending) = self.pending_page.take() {
            self.completed_pages_count += 1;
            Self::add_page_number_to_page(
                &mut old_pending,
                self.completed_pages_count as u32,
                page_numbering,
                font_family,
                margin_bottom,
                (self.left, self.right),
            );
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
            self.completed_pages_count += 1;
            Self::add_page_number_to_page(
                &mut self.current_page,
                self.completed_pages_count as u32,
                page_numbering,
                font_family,
                margin_bottom,
                (self.left, self.right),
            );
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

        let page_numbering = self.page_numbering;
        let font_family = self.font_family;
        let margin_bottom = self.foot_baseline();

        if !self.current_page.ops.is_empty() || self.pages.is_empty() {
            self.pages.push(self.current_page);
        }
        if page_numbering {
            for (page_idx, page) in self.pages.iter_mut().enumerate() {
                let page_number = (page_idx + 1) as u32;
                Self::add_page_number_to_page(
                    page,
                    page_number,
                    page_numbering,
                    font_family,
                    margin_bottom,
                    (self.left, self.right),
                );
            }
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
            if let Some(mut old_pending) = self.pending_page.take() {
                self.completed_pages_count += 1;
                self.add_page_number_to(&mut old_pending, self.completed_pages_count as u32);
                self.completed_pages.push_back(old_pending);
            }
            self.pending_page = Some(previous_page);
        } else {
            self.pages.push(previous_page);
        }

        self.y = self.page_start_y();
        self.has_preceding_text = false;
        self.last_line_width = 0.0;
        self.last_line_indent = 0.0;
        self.prev_graf = 0;
    }

    /// Baseline of the running foot (`\footskip` below the text block).
    #[inline(always)]
    pub(crate) fn foot_baseline(&self) -> f32 {
        self.margin_bottom - self.foot_skip
    }

    /// Top edge of the text block on the current page.
    #[inline(always)]
    pub(crate) fn text_top(&self) -> f32 {
        self.page_height - self.margin_top
    }

    /// Baseline of the first line on a fresh page (`\topskip` below the text-block top).
    #[inline(always)]
    pub(crate) fn page_start_y(&self) -> f32 {
        self.text_top() - self.top_skip
    }

    /// True while nothing has been contributed to the current page; TeX discards
    /// glue, kerns and penalties in this state (tex.web §1000).
    #[inline(always)]
    pub(crate) fn at_page_top(&self) -> bool {
        self.y >= self.page_start_y() - 0.01
    }

    /// Discardable vertical glue: vanishes at the top of a page like TeX's page builder does.
    #[inline(always)]
    pub(crate) fn vskip(&mut self, amount: f32) {
        if !self.at_page_top() {
            self.y -= amount;
        }
    }

    pub(crate) fn ensure_room(&mut self, required_height: f32) -> bool {
        if self.y - required_height < self.margin_bottom - 0.01 {
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
                self.vskip(self.par_skip);
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
            self.vskip(self.par_skip);
        }
        *prev_was_block = true;
        self.layout_single_block(b, 0);
    }

    fn apply_config_change(&mut self, change: &crate::tex_parser::ConfigChange) {
        match change {
            crate::tex_parser::ConfigChange::ParIndent(v) => self.par_indent = *v,
            crate::tex_parser::ConfigChange::ParSkip(v) => self.par_skip = *v,
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
        // Declarations between paragraphs leave the previous line's advance in place.
        if !matches!(b, Block::ConfigChange(_)) {
            self.block_entry_advance = std::mem::take(&mut self.last_line_advance);
        }
        match b {
            Block::ConfigChange(cfg) => self.apply_config_change(cfg),
            Block::Para(inls) => self.layout_para(inls),
            Block::List(list_data) => {
                self.layout_list(list_data.kind, &list_data.options, &list_data.items, depth);
            }
            Block::Quote(quote_data) => {
                let saved_left = self.left;
                let saved_right = self.right;
                let saved_par_indent = self.par_indent;
                self.left += 25.0;
                self.right -= 25.0;
                if !quote_data.is_quotation {
                    self.par_indent = 0.0;
                }
                self.y -= self.par_skip.max(4.0);
                self.layout_blocks(&quote_data.blocks, depth + 1);
                self.y -= self.par_skip.max(4.0);
                self.left = saved_left;
                self.right = saved_right;
                self.par_indent = saved_par_indent;
            }
            Block::Aligned(aligned) => {
                // latex.ltx: `center`, `flushleft` and `flushright` are trivlists, so
                // \topsep is `\addvspace`d above and below: two in a row share one gap.
                let saved_align = self.par_align;
                let saved_par_indent = self.par_indent;
                let top_sep = (TOPSEP + if aligned.in_vmode { PARTOPSEP } else { 0.0 }) * (self.base_font_size / 10.0);
                self.vskip((top_sep - added_vspace).max(0.0));
                self.par_align = aligned.align;
                self.par_indent = 0.0;
                self.layout_blocks(&aligned.blocks, depth + 1);
                self.par_align = saved_align;
                self.par_indent = saved_par_indent;
                self.vskip(top_sep);
                self.last_addvspace = top_sep;
            }
            Block::Table(table_data) => self.layout_table(table_data),
            Block::Figure(fig_data) => self.layout_figure(fig_data.graphic, &fig_data.caption),
            Block::TitleBlock(title_data) => self.layout_title_block(
                &title_data.title,
                title_data.author.as_deref(),
                title_data.date.as_deref(),
            ),
            Block::Section {
                level,
                starred,
                title,
            } => self.layout_section(*level, *starred, title),
            Block::PageBreak => self.new_page(),
            Block::VSpace(h) => self.vskip(*h),
            Block::HRule => {
                let w = self.right - self.left;
                self.ensure_room(4.0);
                if !self.is_dry_run {
                    self.push_op(Paint::Rule {
                        x: self.left,
                        y: self.y,
                        w,
                        h: 0.4,
                        fill: true,
                    });
                }
                self.y -= 4.0;
            }
            Block::CodeListing(listing) => self.layout_code_listing(listing),
            Block::TableOfContents => {
                let reg = std::mem::take(&mut self.registry);
                crate::index::layout_toc(self, &reg);
                self.registry = reg;
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
            Block::Equation { starred, math } => {
                self.layout_equation(*starred, math);
            }
        }
    }

    fn layout_list<'doc>(
        &mut self,
        kind: crate::frontend::ast::ListKind,
        options: &crate::tex_parser::ListOptions,
        items: &'doc [crate::frontend::ast::ListItem<'a>],
        depth: usize,
    ) {
        if items.is_empty() {
            return;
        }

        // 1. Depth-dependent standard LaTeX margins
        let base_depth_margin = if kind == crate::frontend::ast::ListKind::Description {
            30.0
        } else {
            match depth {
                0 => self.list_left_margin,
                1 => 22.0,
                2 => 18.7,
                _ => 17.0,
            }
        };
        let label_sep = options.label_sep_or(self.list_label_sep);
        let item_indent = options.item_indent_or(self.list_item_indent);
        let item_sep = options.item_sep_or(self.list_item_sep);
        let parsep = options.parsep_or(2.0);
        let custom_hang = options.hang_indent_or(0.0);

        // 2. Select marker constructor based on depth, kind, and options
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
                    _ => match depth {
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
                    _ => match depth {
                        0 => Marker::Bullet,
                        1 => Marker::Dash,
                        2 => Marker::Asterisk,
                        _ => Marker::MiddleDot,
                    },
                }
            }
        };

        // 3. Compute max marker width across this list to prevent margin overflow
        let max_marker = make_marker(items.len().saturating_sub(1));
        let max_marker_w = self.marker_width(max_marker, Face::Roman, self.base_font_size);

        // 4. Mathematical left margin:
        // If user specified label_width, use it: left_margin = label_width + label_sep
        // If user specified left_margin, respect it.
        // Otherwise, dynamically ensure left_margin >= max_marker_w + label_sep.
        let left_margin = if let Some(lm) = options.left_margin() {
            lm
        } else if let Some(lw) = options.label_width() {
            lw + label_sep
        } else {
            let needed = max_marker_w + label_sep;
            if needed > base_depth_margin {
                needed.ceil()
            } else {
                base_depth_margin
            }
        };

        // 5. Vertical spacing: Top separation before list
        let top_sep = if depth == 0 { 4.0 } else { 2.0 };
        self.y -= top_sep;

        for (idx, item) in items.iter().enumerate() {
            // Ensure room for marker + at least 1 line of text to prevent orphan markers
            self.ensure_room(self.base_leading * 1.5);

            if let Some(ref custom_inls) = item.custom_label {
                let label_face = if kind == crate::frontend::ast::ListKind::Description {
                    Face::Bold
                } else {
                    Face::Roman
                };
                let custom_w =
                    self.measure_inlines_face_size(custom_inls, label_face, self.base_font_size);

                let marker_x = if kind == crate::frontend::ast::ListKind::Description {
                    self.left
                } else {
                    let first_line_x = self.left + left_margin + item_indent;
                    (first_line_x - label_sep - custom_w).max(self.left)
                };

                self.paint_inlines_at_size(
                    custom_inls,
                    label_face,
                    self.base_font_size,
                    marker_x,
                    self.y,
                );
            } else if kind != crate::frontend::ast::ListKind::Description {
                let marker = make_marker(idx);
                let marker_face = if marker == Marker::Bullet && self.font_family == metrics::FontFamily::ComputerModern {
                    Face::Symbol
                } else {
                    Face::Roman
                };
                let marker_w = self.marker_width(marker, marker_face, self.base_font_size);

                let first_line_x = self.left + left_margin + item_indent;
                let mut marker_x = first_line_x - label_sep - marker_w;
                if marker_x < self.left {
                    marker_x = self.left;
                }

                if !self.is_dry_run {
                    self.push_op(Paint::Marker {
                        x: marker_x,
                        y: self.y,
                        face: marker_face,
                        size: self.base_font_size,
                        marker,
                    });
                }
            }

            let saved_left = self.left;
            self.left += left_margin;

            let first_block_indent = if kind == crate::frontend::ast::ListKind::Description {
                if let Some(ref custom_inls) = item.custom_label {
                    let custom_w = self.measure_inlines_face_size(
                        custom_inls,
                        Face::Bold,
                        self.base_font_size,
                    );
                    let label_end = saved_left + custom_w + label_sep;
                    if label_end > self.left {
                        label_end - self.left
                    } else {
                        item_indent
                    }
                } else {
                    item_indent
                }
            } else {
                item_indent
            };

            let mut is_first_block = true;
            for block in &item.blocks {
                match block {
                    Block::ConfigChange(cfg) => {
                        self.apply_config_change(cfg);
                    }
                    Block::Para(inls) => {
                        if is_first_block {
                            self.layout_para_full(
                                inls,
                                Some(first_block_indent),
                                Some(custom_hang),
                            );
                            is_first_block = false;
                        } else {
                            self.y -= parsep;
                            self.layout_para_full(
                                inls,
                                Some(self.list_par_indent),
                                Some(custom_hang),
                            );
                        }
                    }
                    other => {
                        if !is_first_block {
                            self.y -= parsep;
                        }
                        self.layout_single_block(other, depth + 1);
                        is_first_block = false;
                    }
                }
            }

            self.left = saved_left;
            if idx + 1 < items.len() {
                self.y -= item_sep;
            }
        }

        // Bottom separation after list
        self.y -= top_sep;
    }

    fn layout_title_block<'doc>(
        &mut self,
        title: &'doc [Inline<'a>],
        author: Option<&'doc [Inline<'a>]>,
        date: Option<&'doc [Inline<'a>]>,
    ) {
        let max_w = (self.right - self.right_skip) - (self.left + self.left_skip);
        self.cell_words_soa.clear();
        let mut title_math = Vec::new();
        let title_w = explode_inlines_into_family(
            title,
            Face::Bold,
            18.0,
            &mut self.cell_words_soa,
            self.font_family,
            &mut title_math,
        );
        let title_lines = if title_w <= max_w && !inlines_have_break(title) {
            1
        } else {
            self.cell_lines.clear();
            self.break_cell_words_into_lines(0, self.cell_words_soa.len(), max_w, 18.0, false, true);
            self.cell_lines.len().max(1)
        };

        let title_leading = 22.0f32;
        let title_total_h = (title_lines as f32) * title_leading;
        self.ensure_room(title_total_h + 48.0 + 2.0 * self.base_leading);
        self.y -= 12.0;

        // Title: 18pt, bold, centered
        if title_lines == 1 {
            let title_x = self.left + self.left_skip + (max_w - title_w).max(0.0) * 0.5;
            self.paint_inlines_at_size(title, Face::Bold, 18.0, title_x, self.y);
            self.y -= title_leading + 4.0;
        } else {
            let line_count = self.cell_lines.len();
            for i in 0..line_count {
                let line_desc = self.cell_lines[i];
                let line_x = self.left + self.left_skip + (max_w - line_desc.line_w).max(0.0) * 0.5;
                self.paint_cell_line_at(line_desc, line_x, self.y, 18.0, &title_math);
                self.y -= title_leading;
            }
            self.y -= 4.0;
        }

        // Author: 12pt, Roman, centered
        if let Some(auth) = author {
            let auth_w = self.measure_inlines_face_size(auth, Face::Roman, 12.0);
            let auth_x = self.left + self.left_skip + (max_w - auth_w).max(0.0) * 0.5;
            self.paint_inlines_at_size(auth, Face::Roman, 12.0, auth_x, self.y);
            self.y -= 18.0;
        }

        // Date: 10pt, Roman, centered
        if let Some(dt) = date {
            let dt_w = self.measure_inlines_face_size(dt, Face::Roman, 10.0);
            let dt_x = self.left + self.left_skip + (max_w - dt_w).max(0.0) * 0.5;
            self.paint_inlines_at_size(dt, Face::Roman, 10.0, dt_x, self.y);
            self.y -= 16.0;
        }

        self.y -= 16.0;
        self.after_heading = true;
        self.has_preceding_text = false;
        self.last_line_width = 0.0;
        self.last_line_indent = 0.0;
        self.prev_graf = 0;
    }

    fn layout_section<'doc>(&mut self, level: u8, starred: bool, title: &'doc [Inline<'a>]) {
        // article.cls \@startsection parameters: \section{-3.5ex plus -1ex minus -.2ex}{2.3ex plus .2ex}
        // in \Large, \subsection / \subsubsection {-3.25ex ...}{1.5ex plus .2ex} in \large /
        // \normalsize. The ex unit is that of the body font (cmr: 0.430554em).
        let ex = 0.430554 * self.base_font_size;
        let (large, large_bls, bigger, bigger_bls) = section_font_sizes(self.base_font_size, self.base_leading);
        let (skip_before, font_size, heading_leading, skip_after) = match level {
            1 => (3.5 * ex, large, large_bls, 2.3 * ex),
            2 => (3.25 * ex, bigger, bigger_bls, 1.5 * ex),
            _ => (3.25 * ex, self.base_font_size, self.base_leading, 1.5 * ex),
        };

        let (sec_number, num_w) = if !starred {
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
        let title_lines = if (title_w + num_w) <= max_w && !inlines_have_break(title) {
            1
        } else {
            self.cell_lines.clear();
            self.break_cell_words_into_lines(0, self.cell_words_soa.len(), (max_w - num_w).max(20.0), font_size, false, false);
            self.cell_lines.len().max(1)
        };

        // Vertical model (TeX): `self.y` is where the next baseline would go at \baselineskip.
        // The heading's first baseline lies \addvspace{beforeskip} plus its own \baselineskip
        // below the previous baseline; at the top of a page the skip is discarded and the
        // baseline sits \topskip (or the heading's height) below the text top.
        // \@afterheading forbids a break after the heading and after the first line of the
        // next paragraph (\clubpenalty 10000), so the heading needs two body lines below it.
        let heading_ht = 0.7 * font_size;
        let title_extra = ((title_lines as f32) - 1.0) * heading_leading;
        let below_first_baseline = title_extra + skip_after + self.par_skip + 2.0 * self.base_leading;
        let top_drop = |l: &Self| (heading_ht - l.top_skip).max(0.0);
        let first_baseline_offset = if self.at_page_top() {
            top_drop(self)
        } else {
            skip_before + (heading_leading - self.base_leading)
        };
        if self.ensure_room(first_baseline_offset + below_first_baseline) {
            self.y -= top_drop(self);
        } else {
            self.y -= first_baseline_offset;
        }

        if !starred && self.current_sec_idx < self.registry.toc_entries.len() {
            self.registry.toc_entries[self.current_sec_idx].page = self.current_page_number();
            self.current_sec_idx += 1;
        }

        if let Some(num) = sec_number {
            if !self.is_dry_run {
                self.push_op(Paint::SectionNumber {
                    x: self.left + self.left_skip,
                    y: self.y,
                    face: Face::Bold,
                    size: font_size,
                    number: num,
                    level,
                });
            }
        }

        if title_lines == 1 {
            self.paint_inlines_at_size(
                title,
                Face::Bold,
                font_size,
                self.left + self.left_skip + num_w,
                self.y,
            );
            self.y -= skip_after + self.base_leading;
        } else {
            let line_count = self.cell_lines.len();
            for line_idx in 0..line_count {
                let line_desc = self.cell_lines[line_idx];
                let line_x = self.left + self.left_skip + if line_idx == 0 { num_w } else { 0.0 };
                self.paint_cell_line_at(line_desc, line_x, self.y, font_size, &section_math);
                if line_idx + 1 < line_count {
                    self.y -= heading_leading;
                }
            }
            self.y -= skip_after + self.base_leading;
        }
        self.after_heading = true;
        self.has_preceding_text = false;
        self.last_line_width = 0.0;
        self.last_line_indent = 0.0;
        self.prev_graf = 0;
    }

    fn layout_table<'doc>(&mut self, table_data: &'doc TableData<'a>) {
        crate::tables::layout_table(self, table_data);
    }

    fn layout_figure<'doc>(&mut self, graphic: Option<Graphic<'a>>, caption: &'doc [Inline<'a>]) {
        self.figure_counter += 1;
        let figure_width = (self.right - self.left).min(360.0);
        let figure_height = FIG_H;
        self.ensure_room(figure_height + self.base_leading * 2.0 + PARA_SKIP);

        if self.current_fig_idx < self.registry.figure_entries.len() {
            self.registry.figure_entries[self.current_fig_idx].page = self.current_page_number();
            self.current_fig_idx += 1;
        }

        let x = self.left + ((self.right - self.left) - figure_width) * 0.5;
        let y_bottom = self.y - figure_height;
        if !self.is_dry_run {
            self.push_op(Paint::Rule {
                x,
                y: y_bottom,
                w: figure_width,
                h: figure_height,
                fill: false,
            });
            if let Some(graphic_obj) = graphic {
                if !graphic_obj.path.is_empty() {
                    let text_width = metrics::string_width_family(
                        graphic_obj.path,
                        Face::Roman,
                        self.base_font_size,
                        self.font_family,
                    );
                    let pool_start = self.current_page.text_pool.len() as u32;
                    self.current_page.text_pool.push(graphic_obj.path);
                    self.push_op(Paint::GlyphRun {
                        x: x + ((figure_width - text_width) * 0.5).max(CELL_PAD),
                        y: y_bottom + figure_height * 0.5,
                        face: Face::Roman,
                        size: self.base_font_size,
                        part_start: pool_start,
                        part_count: 1,
                        word_spacing: 0.0,
                    });
                }
            }
        }
        self.y = y_bottom - self.base_leading;
        if !caption.is_empty() {
            let tag_width = 56.0;
            let cap_max_w = (figure_width - tag_width).max(10.0);
            self.cell_words_soa.clear();
            let mut cap_math = Vec::new();
            let cap_w = explode_inlines_into_family(
                caption,
                Face::Roman,
                self.base_font_size,
                &mut self.cell_words_soa,
                self.font_family,
                &mut cap_math,
            );
            if !self.is_dry_run {
                self.push_op(Paint::CaptionTag {
                    x: self.left + ((self.right - self.left) - figure_width) * 0.5,
                    y: self.y,
                    size: self.base_font_size,
                    n: self.figure_counter,
                });
            }
            let cap_x = self.left + ((self.right - self.left) - figure_width) * 0.5 + tag_width;
            if cap_w <= cap_max_w && !inlines_have_break(caption) {
                self.paint_inlines_at(caption, cap_x, self.y);
                self.y -= self.base_leading;
            } else {
                self.cell_lines.clear();
                let base = self.base_font_size;
                self.break_cell_words_into_lines(0, self.cell_words_soa.len(), cap_max_w, base, true, false);
                let line_count = self.cell_lines.len();
                for idx in 0..line_count {
                    let line_desc = self.cell_lines[idx];
                    let cur_y = self.y;
                    self.paint_cell_line(line_desc, cap_x, cur_y, &cap_math);
                    if idx + 1 < line_count {
                        self.y -= self.base_leading;
                    }
                }
                self.y -= self.base_leading;
            }
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

        let m_box = crate::math::measure_math(
            actual_math,
            self.base_font_size,
            self.font_family,
            crate::math::MathStyle::Display,
        );

        let tag_w = if let Some(tag_node) = custom_tag {
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

        let available_w = (self.right - self.right_skip) - (self.left + self.left_skip);
        let has_any_tag = custom_tag.is_some() || (!starred && tag_w > 0.0);
        let disp_geom = crate::math::DisplayGeometryEngine::compute_display_geometry(
            available_w,
            self.hang_indent,
            self.hang_after,
            self.prev_graf,
            self.last_line_width,
            self.last_line_indent,
            self.has_preceding_text,
            m_box.w,
            tag_w,
            is_left_tag,
            has_any_tag,
            self.base_font_size,
        );
        let crate::math::PackedDisplayGeometry {
            display_indent,
            displacement,
            above_skip,
            below_skip,
            must_split_tag,
            ..
        } = disp_geom;

        let tag_line_h = if must_split_tag { self.base_leading } else { 0.0 };
        let total_h = above_skip + m_box.h + m_box.d + below_skip + tag_line_h;

        let page_broken = self.ensure_room(total_h);
        // In TeX, discardable vertical glue at top of page is discarded:
        let actual_above_skip = if page_broken { 0.0f32 } else { above_skip };

        self.y -= actual_above_skip;
        let baseline_y = self.y - m_box.h;

        let eq_x = self.left + self.left_skip + display_indent + displacement;
        let right_margin = self.right - self.right_skip;
        let tag_x = if is_left_tag {
            self.left + self.left_skip + display_indent
        } else {
            right_margin - tag_w
        };

        if !self.is_dry_run {
            crate::math::render_math(
                actual_math,
                self.base_font_size,
                self.font_family,
                crate::math::MathStyle::Display,
                eq_x,
                baseline_y,
                &mut self.current_page.ops,
                &mut self.current_page.text_pool,
            );
            self.current_page.used_faces |= (1 << (Face::Symbol as u8))
                | (1 << (Face::Roman as u8))
                | (1 << (Face::Italic as u8));

            let tag_y = if must_split_tag {
                baseline_y - m_box.d - self.base_leading * 0.8
            } else {
                baseline_y
            };

            if let Some(tag_node) = custom_tag {
                let render_tag_x = if !tag_starred {
                    let open_paren_w = metrics::width_family('(', Face::Roman, self.base_font_size, self.font_family);
                    let start_p = self.current_page.text_pool.len() as u32;
                    self.current_page.text_pool.push("(");
                    self.push_op(Paint::GlyphRun {
                        x: tag_x,
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
                    render_tag_x,
                    tag_y,
                    &mut self.current_page.ops,
                    &mut self.current_page.text_pool,
                );

                if !tag_starred {
                    let close_p = self.current_page.text_pool.len() as u32;
                    self.current_page.text_pool.push(")");
                    self.push_op(Paint::GlyphRun {
                        x: render_tag_x + t_box.w,
                        y: tag_y,
                        face: Face::Roman,
                        size: self.base_font_size,
                        part_start: close_p,
                        part_count: 1,
                        word_spacing: 0.0,
                    });
                }
            } else if !starred {
                self.push_op(Paint::EquationNumber {
                    x: tag_x,
                    y: tag_y,
                    face: Face::Roman,
                    size: self.base_font_size,
                    n: self.equation_counter,
                });
            }
        }

        if must_split_tag {
            self.y = baseline_y - m_box.d - self.base_leading * 0.8 - below_skip;
        } else {
            self.y = baseline_y - m_box.d - below_skip;
        }

        let mut space_factor = 1000u16;
        crate::math::DisplayResumeStateRestorer::restore_state(&mut self.prev_graf, &mut space_factor);
        self.has_preceding_text = false;
        self.after_heading = false;
    }

    fn layout_code_listing<'doc>(&mut self, listing: &'doc crate::code_listing::CodeListingData<'a>) {
        let font_size = listing.options.font_size;
        let line_leading = font_size * 1.30;
        let lines: Vec<&'a str> = listing.lines().collect();
        if lines.is_empty() {
            return;
        }

        let show_line_numbers = listing.options.show_line_numbers;
        let total_lines = lines.len();
        let num_digits = if total_lines >= 100 {
            3
        } else if total_lines >= 10 {
            2
        } else {
            1
        };
        let mono_char_w = metrics::width('0', Face::Mono, font_size);
        let gutter_w = if show_line_numbers {
            (num_digits as f32 + 1.5) * mono_char_w
        } else {
            0.0
        };

        let pad_x = 8.0;
        let pad_y = 8.0;
        let skip_before = 8.0;
        let skip_after = 8.0;

        let top_pad = pad_y + font_size * 0.75;
        self.ensure_room(skip_before + top_pad + pad_y + line_leading);
        self.y -= skip_before;

        let code_x = self.left + pad_x + gutter_w;
        let gutter_x = self.left + pad_x;

        let mut block_page_start_y = self.y;
        let mut lines_on_current_page = 0;

        for (line_idx, &line_text) in lines.iter().enumerate() {
            let y_before = self.y;
            let broken = self.ensure_room(line_leading + pad_y);
            if broken {
                if lines_on_current_page > 0 && !self.is_dry_run {
                    let block_bottom = y_before - pad_y;
                    let block_h = block_page_start_y - block_bottom;
                    let w = self.right - self.left;
                    if let Some(p) = self.pending_page.as_mut().or_else(|| self.pages.last_mut()) {
                        if listing.options.left_accent_bar {
                            p.push_op(Paint::Rule {
                                x: self.left,
                                y: block_bottom,
                                w: 2.0,
                                h: block_h,
                                fill: true,
                            });
                        }
                        if listing.options.frame {
                            p.push_op(Paint::Rule {
                                x: self.left,
                                y: block_bottom,
                                w,
                                h: block_h,
                                fill: false,
                            });
                        }
                        if show_line_numbers {
                            p.push_op(Paint::Rule {
                                x: self.left + pad_x + gutter_w - 3.0,
                                y: block_bottom,
                                w: 0.4,
                                h: block_h,
                                fill: true,
                            });
                        }
                    }
                }
                block_page_start_y = self.y;
                lines_on_current_page = 0;
            }

            if lines_on_current_page == 0 {
                self.y -= top_pad;
            }

            if show_line_numbers && !self.is_dry_run {
                self.push_op(Paint::PageNumber {
                    x: gutter_x,
                    y: self.y,
                    face: Face::Mono,
                    size: font_size * 0.85,
                    page_number: (line_idx + 1) as u32,
                });
            }

            if !line_text.is_empty() && !self.is_dry_run {
                let pool_start = self.current_page.text_pool.len() as u32;
                self.current_page.text_pool.push(line_text);
                self.push_op(Paint::GlyphRun {
                    x: code_x,
                    y: self.y,
                    face: Face::Mono,
                    size: font_size,
                    part_start: pool_start,
                    part_count: 1,
                    word_spacing: 0.0,
                });
            }

            self.y -= line_leading;
            lines_on_current_page += 1;
        }

        let block_bottom = self.y - pad_y;
        let block_h = block_page_start_y - block_bottom;
        let w = self.right - self.left;

        if !self.is_dry_run {
            if listing.options.left_accent_bar {
                self.push_op(Paint::Rule {
                    x: self.left,
                    y: block_bottom,
                    w: 2.0,
                    h: block_h,
                    fill: true,
                });
            }
            if listing.options.frame {
                self.push_op(Paint::Rule {
                    x: self.left,
                    y: block_bottom,
                    w,
                    h: block_h,
                    fill: false,
                });
            }
            if show_line_numbers {
                self.push_op(Paint::Rule {
                    x: self.left + pad_x + gutter_w - 3.0,
                    y: block_bottom,
                    w: 0.4,
                    h: block_h,
                    fill: true,
                });
            }
        }

        self.y = block_bottom - skip_after;
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
            Face::Roman,
            self.base_font_size,
            &mut self.words_soa,
            self.font_family,
            &mut math_collector,
            &mut space_factor,
            self.french_spacing,
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

        let has_first_line_indent = if is_indent {
            self.after_heading = false;
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
            pass = crate::layout::layout_linebreak::tex_line_break(
                &self.words_soa,
                0..self.words_soa.len(),
                &params,
                &shape,
                self.base_font_size,
                &mut self.break_scratch,
                &mut self.line_descriptors,
            ) as usize;
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
        for d in 0..self.line_descriptors.len() {
            let desc = self.line_descriptors[d];
            self.flush_line_descriptor(&desc, &math_collector);
        }
        if let Some(t) = t_fl {
            self.time_flush_ns += t.elapsed().as_nanos() as u64;
        }

        let line_count = self.line_descriptors.len();
        if line_count > 0 {
            let last_desc = self.line_descriptors[line_count - 1];
            self.last_line_width = last_desc.line_w;
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

    /// Lowers the baseline of a line whose text is larger than the paragraph's
    /// `\baselineskip` allows: when the glue between the previous line's depth and this
    /// line's height would drop below `\lineskiplimit` (0pt), TeX uses `\lineskip` (1pt)
    /// instead (§679). Heights and depths are those of Computer Modern's tallest and
    /// deepest letters (0.694 and 0.194 em). Returns the line's largest size.
    fn raise_for_line_size(&mut self, start: usize, end: usize, par_size: f32) -> f32 {
        let runs = &self.words_soa.size_runs;
        let mut max_size = if runs.first().is_some_and(|r| r.0 as usize <= start) { 0.0 } else { par_size };
        let k = runs.partition_point(|r| r.0 as usize <= start);
        if k > 0 {
            max_size = runs[k - 1].1;
        }
        for r in &runs[k..] {
            if r.0 as usize >= end {
                break;
            }
            max_size = max_size.max(r.1);
        }
        let glue = self.base_leading - 0.194 * self.prev_line_size - 0.694 * max_size;
        if glue < 0.0 {
            // Only between lines of one page; ensure_room already ran for this line.
            self.y -= 1.0 - glue;
        }
        max_size
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
        let x = available - desc.line_w;
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

    /// A formula with `\color` inside ends its scopes black; the text after it continues in
    /// the line's colour.
    fn restore_color_after_math(&mut self, ops_before: usize, line_color: PackedColor) {
        let colored = self.current_page.ops[ops_before..].iter().any(|op| matches!(op, Paint::Color { .. }));
        if colored && line_color != PackedColor::BLACK {
            self.push_op(Paint::Color { color: line_color });
        }
    }

    fn flush_line_descriptor(
        &mut self,
        desc: &LineDescriptor,
        math_collector: &[&crate::math::MathNode<'a>],
    ) {
        if desc.word_count == 0 {
            return;
        }
        // A line fits while its baseline stays inside the text block; its depth may hang
        // into \maxdepth below it (tex.web §1002–1010).
        self.ensure_room(0.0);
        if self.is_dry_run {
            self.y -= self.base_leading;
            return;
        }

        let measure = (self.right - self.right_skip) - (self.left + self.left_skip);
        let available = measure - desc.line_indent;

        let ragged = self.par_align != crate::frontend::ast::ParAlign::Justified;
        let is_last_line = (desc.start_word + desc.word_count) as usize >= self.words_soa.len();
        let glue_set = self.line_glue_set(desc, available, ragged, is_last_line);

        let slack = (available - desc.line_w).max(0.0);
        let align_shift = match self.par_align {
            crate::frontend::ast::ParAlign::Center => 0.5 * slack,
            crate::frontend::ast::ParAlign::RaggedLeft => slack,
            _ => 0.0,
        };
        let mut x = self.left + self.left_skip + desc.line_indent + align_shift;
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
        let line_size = if self.words_soa.size_runs.is_empty() {
            par_size
        } else {
            self.raise_for_line_size(start, end, par_size)
        };
        self.prev_line_size = line_size;

        while i < end {
            let attr = self.words_soa.attrs[i];
            let w = self.words_soa.widths[i];
            if attr.is_space() {
                x += w + glue_set.adjust(self.words_soa.glue[i]);
                i += 1;
                continue;
            }
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
                        let m_box = crate::math::render_math(
                            node,
                            run_size,
                            self.font_family,
                            crate::math::MathStyle::Text,
                            x,
                            self.y,
                            &mut self.current_page.ops,
                            &mut self.current_page.text_pool,
                        );
                        x += m_box.w;
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
                    x,
                    y: self.y,
                    face,
                    size: run_size,
                    part_start: pool_start,
                    part_count: 1 | GLYPH_RUN_SHAPED,
                    word_spacing: 0.0,
                });
                let shaped = |t: &str| crate::backend::shape::word_width_pt(t, face, run_size, self.font_family);
                x += shaped(piece);
                if i == start && desc.head_cut != 0 {
                    // Material after the word's letters (an italic correction) stays with
                    // its post-break part.
                    x += self.words_soa.widths[i] - shaped(self.words_soa.texts[i]);
                }
                i += 1;
                continue;
            }
            let x0 = x;
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
                if cur_attr.is_word_like() && cur_face != face {
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
                    x: x0,
                    y: self.y,
                    face,
                    size: run_size,
                    part_start: pool_start,
                    part_count: part_count | GLYPH_RUN_SHAPED,
                    word_spacing,
                });
            }

            x += run_w + run_extra;
        }

        if end > start
            && (desc.tail_cut != 0
                || (self.words_soa.attrs[end - 1].kind() == WordKind::Hyphen
                    && !self.words_soa.texts[end - 1].ends_with('-')))
        {
            let hyph_face = self.words_soa.attrs[end - 1].face();
            let pool_start = self.current_page.text_pool.len() as u32;
            self.current_page.text_pool.push("-");
            let hyph_x = x;
            self.push_op(Paint::GlyphRun {
                x: hyph_x,
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

        self.y -= self.base_leading;
        self.last_line_advance = self.base_leading;
    }

    fn paint_inlines_at<'doc>(&mut self, inlines: &'doc [Inline<'a>], x: f32, y: f32) {
        self.paint_inlines_at_size(inlines, Face::Roman, self.base_font_size, x, y);
    }

    pub(crate) fn paint_inlines_at_size<'doc>(
        &mut self,
        inlines: &'doc [Inline<'a>],
        default_face: Face,
        font_size: f32,
        mut x: f32,
        y: f32,
    ) {
        if self.is_dry_run {
            return;
        }
        let mut scratch_runs = Vec::with_capacity(32);
        flatten_into(inlines, default_face, self.text_color, &mut scratch_runs);
        let mut cur_color = PackedColor::BLACK;
        let mut scratch_slices = Vec::with_capacity(32);
        let scale = font_size * 0.001;
        for run in &scratch_runs {
            let run = *run;
            if run.color != cur_color && (run.math.is_some() || !run.text.trim().is_empty()) {
                self.push_op(Paint::Color { color: run.color });
                cur_color = run.color;
            }
            if let Some(m) = run.math {
                let m_box = crate::math::render_math(
                    m,
                    font_size,
                    self.font_family,
                    crate::math::MathStyle::Text,
                    x,
                    y,
                    &mut self.current_page.ops,
                    &mut self.current_page.text_pool,
                );
                x += m_box.w;
                continue;
            }
            if run.is_break || run.text.is_empty() {
                continue;
            }
            let s = run.text;
            let bytes = s.as_bytes();
            let mut i = 0;
            let x0 = x;
            let space_units = metrics::resolve_space_units_at(self.font_family, run.face, font_size);
            let space_w = space_units * scale;
            let table = metrics::resolve_width_table_at(self.font_family, run.face, font_size);
            let shaper = crate::backend::shape::text_font_at(self.font_family, run.face, font_size);

            let is_cm = self.font_family == metrics::FontFamily::ComputerModern;
            scratch_slices.clear();
            while i < bytes.len() {
                let b = bytes[i];
                if is_tex_space_byte(b) {
                    let space_start = i;
                    i += 1;
                    while i < bytes.len() && is_tex_space_byte(bytes[i]) {
                        i += 1;
                    }
                    if !is_cm {
                        let space_slice = if b == b' ' && i == space_start + 1 {
                            &s[space_start..space_start + 1]
                        } else {
                            " "
                        };
                        scratch_slices.push(space_slice);
                    }
                    x += space_w;
                } else {
                    let start = i;
                    i += 1;
                    while i < bytes.len() && !is_tex_space_byte(bytes[i]) {
                        i += 1;
                    }
                    let word = &s[start..i];
                    if is_cm {
                        let pool_idx = self.current_page.text_pool.len() as u32;
                        self.current_page.text_pool.push(word);
                        self.push_op(Paint::GlyphRun {
                            x,
                            y,
                            face: run.face,
                            size: font_size,
                            part_start: pool_idx,
                            part_count: 1 | GLYPH_RUN_SHAPED,
                            word_spacing: 0.0,
                        });
                    } else {
                        scratch_slices.push(word);
                    }
                    x += match &shaper {
                        Some(font) => font.width_units(word),
                        None => metrics::string_units_table(word, table) as f32,
                    } * scale;
                }
            }
            if !is_cm && !scratch_slices.is_empty() {
                let pool_start = self.current_page.text_pool.len() as u32;
                let pool_len = Self::push_batched_slices(
                    &mut self.current_page.text_pool,
                    &scratch_slices,
                );
                if pool_len > 0 {
                    self.push_op(Paint::GlyphRun {
                        x: x0,
                        y,
                        face: run.face,
                        size: font_size,
                        part_start: pool_start,
                        part_count: pool_len | GLYPH_RUN_SHAPED,
                        word_spacing: 0.0,
                    });
                }
            }
        }
        if cur_color != PackedColor::BLACK {
            self.push_op(Paint::Color { color: PackedColor::BLACK });
        }
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
            let glue = GlueSet::for_line(inner_w - d.line_w, self.cell_words_soa.line_glue(s, s + d.word_count as usize), d.justify() && !centered);
            self.cell_lines.push(CellLineRange {
                start_word: d.start_word,
                word_count: d.word_count,
                line_w: d.line_w,
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
    pub(crate) fn paint_cell_line(&mut self, line: CellLineRange, x: f32, y: f32, math_collector: &[&crate::math::MathNode<'a>]) {
        self.paint_cell_line_at(line, x, y, self.base_font_size, math_collector);
    }

    fn paint_cell_line_at(
        &mut self,
        line: CellLineRange,
        mut x: f32,
        y: f32,
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
                        let m_box = crate::math::render_math(
                            node,
                            run_size,
                            self.font_family,
                            crate::math::MathStyle::Text,
                            x,
                            y,
                            &mut self.current_page.ops,
                            &mut self.current_page.text_pool,
                        );
                        x += m_box.w;
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
                    x,
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
                    x: x0,
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
                x,
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
    let scale = size * 0.001;
    let table = metrics::resolve_width_table_at(family, face, size);
    match marker {
        Marker::Bullet => metrics::width_family('•', face, size, family),
        Marker::Dash => metrics::width_family('–', face, size, family),
        Marker::Asterisk => (table[b'*' as usize] as f32) * scale,
        Marker::MiddleDot => metrics::width_family('·', face, size, family),
        Marker::Number(n) => {
            let num_digits = count_digits(n);
            let digit_units = metrics::resolve_digit_units(family, face);
            let dot_units = table[b'.' as usize] as u32;
            ((num_digits * digit_units + dot_units) as f32) * scale
        }
        Marker::ParenNumber(n) => {
            let num_digits = count_digits(n);
            let digit_units = metrics::resolve_digit_units(family, face);
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
fn section_font_sizes(base_size: f32, base_leading: f32) -> (f32, f32, f32, f32) {
    if base_size >= 10.5 {
        (17.28, 22.0, 14.4, 18.0) // size11.clo and size12.clo
    } else if (base_size - 10.0).abs() < 0.01 {
        (14.4, 18.0, 12.0, 14.0) // size10.clo
    } else {
        let r = base_leading / base_size;
        (base_size * 1.44, base_size * 1.44 * r, base_size * 1.2, base_size * 1.2 * r)
    }
}

pub(crate) fn section_number_width(
    number: [u16; 4],
    level: u8,
    face: Face,
    size: f32,
    family: metrics::FontFamily,
) -> f32 {
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
        let digits = count_digits(num as u32);
        total_units += digits * digit_units;
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

/// Compact 1-byte bit-packed word attribute storing both `WordKind` (low 3 bits)
/// and `Face` (high 5 bits).
/// Exactly 64 word attributes fit into a single 64-byte L1 CPU cache line with
/// 100% packing density and zero line splits.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
#[repr(transparent)]
pub struct PackedWordAttr(pub u8);

const _: () = assert!(core::mem::size_of::<PackedWordAttr>() == 1);

impl PackedWordAttr {
    #[inline(always)]
    pub const fn new(kind: WordKind, face: Face) -> Self {
        Self((kind as u8) | ((face as u8) << 3))
    }

    #[inline(always)]
    pub fn kind(self) -> WordKind {
        WordKind::from_u8(self.0 & 0x07)
    }

    #[inline(always)]
    pub fn face(self) -> Face {
        Face::from_u8(self.0 >> 3)
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
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<ParagraphSoA>() == 176);

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

    #[inline]
    pub fn push_math(&mut self, width: f32, math_idx: u32) {
        self.push_math_atom(width, math_idx, WordKind::Math);
    }

    #[inline]
    pub fn push_math_atom(&mut self, width: f32, math_idx: u32, kind: WordKind) {
        let idx = self.widths.len() as u32;
        self.widths.push(width);
        self.glue.push([0.0; 2]);
        self.attrs.push(PackedWordAttr::new(kind, Face::Roman));
        self.texts.push("");
        self.math_indices.push(SparseMathIdx {
            word_idx: idx,
            math_idx,
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

/// Whether the inlines typeset nothing (only spaces and declarations).
fn is_blank_inlines(inls: &[Inline<'_>]) -> bool {
    inls.iter().all(|inl| match inl {
        Inline::Text(s) => s.trim().is_empty(),
        Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x) => is_blank_inlines(x),
        Inline::Math(_) | Inline::HSpace(_) => false,
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

pub(crate) fn inlines_have_break(inls: &[Inline<'_>]) -> bool {
    inls.iter().any(|inl| match inl {
        Inline::Break | Inline::ParEnd => true,
        Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x) => {
            inlines_have_break(x)
        }
        _ => false,
    })
}

#[derive(Clone, Copy)]
struct FlatRun<'a, 'doc> {
    text: &'a str,
    face: Face,
    color: PackedColor,
    is_break: bool,
    math: Option<&'doc crate::math::MathNode<'a>>,
}

fn flatten_into<'a, 'doc>(inls: &'doc [Inline<'a>], face: Face, color: PackedColor, out: &mut Vec<FlatRun<'a, 'doc>>) {
    for (k, i) in inls.iter().enumerate() {
        match i {
            Inline::Text(s) => {
                if !s.is_empty() {
                    out.push(FlatRun {
                        text: s,
                        face,
                        color,
                        is_break: false,
                        math: None,
                    });
                }
            }
            Inline::Bold(x) => flatten_into(x, face.with_bold(true), color, out),
            Inline::Italic(x) => flatten_into(x, face.with_italic(true), color, out),
            Inline::Mono(x) => flatten_into(x, face.with_mono(true), color, out),
            Inline::Group(x) => flatten_into(x, face, color, out),
            Inline::ScShape => {
                flatten_into(&inls[k + 1..], Face::SmallCaps, color, out);
                break;
            }
            Inline::Color(c) => {
                flatten_into(&inls[k + 1..], face, *c, out);
                break;
            }
            Inline::FontSize(_) => {}
            Inline::Break | Inline::ParEnd => out.push(FlatRun {
                text: "",
                face,
                color,
                is_break: true,
                math: None,
            }),
            Inline::Math(node) => out.push(FlatRun {
                text: "",
                face,
                color,
                is_break: false,
                math: Some(node),
            }),
            Inline::NoIndent
            | Inline::Indent
            | Inline::HangIndent(_)
            | Inline::HangAfter(_)
            | Inline::ParShape(_)
            | Inline::PreTolerance(_)
            | Inline::Tolerance(_)
            | Inline::EmergencyStretch(_)
            | Inline::Looseness(_)
            | Inline::LeftSkip(_)
            | Inline::RightSkip(_)
            | Inline::ParFillSkip { .. }
            | Inline::PrevGraf(_)
            | Inline::LastLineFit(_)
            | Inline::Sloppy
            | Inline::Fussy
            | Inline::Align(_)
            | Inline::HSpace(_) => {}
        }
    }
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
        false,
    )
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
    french_spacing: bool,
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
    let word_width = |word: &str| -> f32 { crate::backend::shape::word_width_pt(word, face, size, family) };
    let mut total_w = 0.0;

    for (idx, inl) in inls.iter().enumerate() {
        match inl {
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
                                    if !french_spacing && (!cb.is_ascii_lowercase() || w_sf != 1000) {
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
                            if !french_spacing {
                                *space_factor = w_sf;
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
                                if !french_spacing
                                    && (!cb.is_ascii_lowercase() || fb_sf != 1000) {
                                        fb_sf = crate::layout::layout_hz::update_space_factor(fb_sf, cb, false);
                                    }
                            }
                            if !french_spacing {
                                *space_factor = fb_sf;
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
                            } else if has_hyphen && word.len() > 1 && !word.starts_with('-') && !word.ends_with('-') {
                                let mut last = 0;
                                for (hyphen_pos, _) in word.match_indices('-') {
                                    // TeX puts the explicit-hyphen discretionary after a character
                                    // that is still the hyphen once ligatures are formed: `--`/`---`
                                    // become en/em dashes in shaped fonts and get no break.
                                    if shaper.is_some()
                                        && (word.as_bytes().get(hyphen_pos + 1) == Some(&b'-')
                                            || (hyphen_pos > 0 && word.as_bytes()[hyphen_pos - 1] == b'-'))
                                    {
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
                total_w += explode_inlines_into_family_sf(x, face.with_bold(true), size, soa, family, math_collector, space_factor, french_spacing);
                soa.set_color(color);
                soa.set_size(size, size);
            }
            Inline::Italic(x) => {
                let color = soa.current_color();
                let first = soa.len();
                let it_face = face.with_italic(true);
                total_w += explode_inlines_into_family_sf(x, it_face, size, soa, family, math_collector, space_factor, french_spacing);
                // `\textit` ends with LaTeX's `\check@icr`: an italic correction unless a
                // period or comma follows (`\nocorrlist`).
                let next_nocorr = matches!(inls.get(idx + 1), Some(Inline::Text(t)) if t.starts_with(['.', ',']));
                if !next_nocorr && soa.len() > first {
                    let last = soa.len() - 1;
                    let attr = soa.attrs[last];
                    if attr.is_word_like() && attr.face() == Face::Italic {
                        if let Some(ch) = soa.texts[last].chars().next_back() {
                            let ic = crate::backend::scaled::italic_correction_pt(family, Face::Italic, size, ch);
                            soa.widths[last] += ic;
                            total_w += ic;
                        }
                    }
                }
                soa.set_color(color);
                soa.set_size(size, size);
            }
            Inline::Mono(x) => {
                let color = soa.current_color();
                total_w += explode_inlines_into_family_sf(x, face.with_mono(true), size, soa, family, math_collector, space_factor, french_spacing);
                soa.set_color(color);
                soa.set_size(size, size);
            }
            Inline::Group(x) => {
                let color = soa.current_color();
                total_w += explode_inlines_into_family_sf(x, face, size, soa, family, math_collector, space_factor, french_spacing);
                soa.set_color(color);
                soa.set_size(size, size);
            }
            Inline::Color(c) => soa.set_color(*c),
            Inline::ScShape => {
                // The rest of the group is set in small caps (cmcsc10); the Base-14 families
                // have no small-caps face, so they keep the upright one.
                let sc = if family == metrics::FontFamily::ComputerModern { Face::SmallCaps } else { face };
                total_w += explode_inlines_into_family_sf(&inls[idx + 1..], sc, size, soa, family, math_collector, space_factor, french_spacing);
                break;
            }
            Inline::FontSize(fs) => {
                // The rest of the group is set at the new size.
                let class = if soa.class_size > 0.0 { soa.class_size } else { FONT_SIZE };
                let new_size = fs.metrics(class).0;
                soa.set_size(new_size, size);
                total_w += explode_inlines_into_family_sf(&inls[idx + 1..], face, new_size, soa, family, math_collector, space_factor, french_spacing);
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
            Inline::Math(node) => {
                *space_factor = 1000;
                let has_breakable_atoms = if let crate::math::MathNode::Row(children) = &**node {
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
                    if let crate::math::MathNode::Row(children) = &**node {
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

                        for (i, child) in children.iter().enumerate() {
                            let mb = crate::math::measure_math(child, size, family, crate::math::MathStyle::Text);
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

                            soa.push_math_atom(mb.w, math_idx, word_kind);
                            total_w += mb.w;

                            if is_not_last {
                                let space = if family == metrics::FontFamily::ComputerModern {
                                    let params = crate::math::math_fonts::math_font_parameters(family);
                                    let scale = size * 0.001;
                                    let quad_sp = crate::math::math_box::ScaledPoints::from_pt(
                                        (params.quad as f32) * scale,
                                    );
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
                                    soa.push("", Face::Roman, space, WordKind::MathSpace);
                                    total_w += space;
                                }
                            }
                        }
                    }
                } else {
                    let mb =
                        crate::math::measure_math(node, size, family, crate::math::MathStyle::Text);
                    let math_idx = math_collector.len() as u32;
                    math_collector.push(node);
                    soa.push_math(mb.w, math_idx);
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
                if !next_nocorr && face.with_italic(true) == Face::Italic {
                    if let Some(ch) = last_text_char(x) {
                        total += crate::backend::scaled::italic_correction_pt(family, Face::Italic, size, ch);
                    }
                }
            }
            Inline::Mono(x) => total += measure_inlines_pt(x, face.with_mono(true), size, family, sf),
            Inline::Group(x) => total += measure_inlines_pt(x, face, size, family, sf),
            Inline::ScShape => {
                let sc = if family == metrics::FontFamily::ComputerModern { Face::SmallCaps } else { face };
                total += measure_inlines_pt(&inls[idx + 1..], sc, size, family, sf);
                break;
            }
            Inline::FontSize(fs) => {
                total += measure_inlines_pt(&inls[idx + 1..], face, fs.metrics(FONT_SIZE).0, family, sf);
                break;
            }
            Inline::HSpace(dim) => total += dim.max(0.0),
            Inline::Math(node) => {
                *sf = 1000;
                total += crate::math::measure_math(node, size, family, crate::math::MathStyle::Text).w;
            }
            Inline::Break | Inline::ParEnd => *sf = 1000,
            _ => {}
        }
    }
    total
}

/// The last character of the text at the end of `inls`, looking into nested groups.
fn last_text_char(inls: &[Inline<'_>]) -> Option<char> {
    match inls.last()? {
        Inline::Text(t) => t.chars().next_back().filter(|c| !c.is_whitespace()),
        Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x) => last_text_char(x),
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
            assert_eq!(d1.line_w, d2.line_w);
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
