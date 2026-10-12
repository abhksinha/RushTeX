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
    hz_break_paragraph_fast_shape, try_layout_single_line_paragraph, Breakpoint, HzConfig, ParCacheEntry, PAR_CACHE_MAX_LINES,
};
use crate::tex_parser::{Block, Document, Graphic, Inline, TableData};

pub const PAR_CACHE_CAP: usize = 256;
pub const PAR_CACHE_MASK: usize = PAR_CACHE_CAP - 1;

pub const PAGE_W: f32 = 595.0;
pub const PAGE_H: f32 = 842.0;
pub const MARGIN: f32 = 72.0;
pub const FONT_SIZE: f32 = 11.0;
pub const LEADING: f32 = 14.0;
pub const PARA_SKIP: f32 = 8.0;
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
    #[inline(always)]
    pub fn push_op(&mut self, op: Paint) {
        match op {
            Paint::GlyphRun { face, .. }
            | Paint::Marker { face, .. }
            | Paint::PageNumber { face, .. }
            | Paint::SectionNumber { face, .. }
            | Paint::FigureTag { face, .. }
            | Paint::TableTag { face, .. }
            | Paint::EquationNumber { face, .. } => {
                self.used_faces |= 1 << (face as u8);
            }
            Paint::CaptionTag { .. } | Paint::TableCaptionTag { .. } => {
                self.used_faces |= 1 << (Face::Roman as u8);
            }
            Paint::Rule { .. } | Paint::VectorGlyph { .. } | Paint::CidGlyphRun { .. } => {}
        }
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
                Paint::Rule { .. } | Paint::VectorGlyph { .. } | Paint::CidGlyphRun { .. } => {}
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
        n: u32,
    },
    TableCaptionTag {
        x: f32,
        y: f32,
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
    pub(crate) words_soa: ParagraphSoA<'a>,
    pub(crate) breakpoints: Vec<Breakpoint>,
    pub(crate) hyphen_scratch: Vec<&'a str>,
    pub(crate) hyphen_cache: Box<[crate::layout::layout_hyphen::HyphenCacheEntry; crate::layout::layout_hyphen::HYPHEN_CACHE_SIZE]>,
    pub(crate) par_cache: Box<[ParCacheEntry; PAR_CACHE_CAP]>,
    pub(crate) dp_nodes: Vec<crate::layout::layout_hz::DpNode>,
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
        let y = config.page_height - config.margin_top;
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
            after_heading: true,
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
            words_soa: ParagraphSoA::with_capacity(1024),
            breakpoints: Vec::with_capacity(512),
            hyphen_scratch: Vec::with_capacity(16),
            hyphen_cache: vec![crate::layout::layout_hyphen::HyphenCacheEntry::empty(); crate::layout::layout_hyphen::HYPHEN_CACHE_SIZE]
                .into_boxed_slice()
                .try_into()
                .unwrap(),
            par_cache: vec![ParCacheEntry::default(); PAR_CACHE_CAP]
                .into_boxed_slice()
                .try_into()
                .unwrap(),
            dp_nodes: Vec::with_capacity(1024 * 4),
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
        margin_bottom: f32,
    ) {
        if !page_numbering {
            return;
        }
        let digit_count = count_digits(page_number);
        let page_num_size = 10.0f32;
        let digit_units = metrics::resolve_digit_units(font_family, Face::Roman);
        let num_width = (digit_count * digit_units) as f32 * (page_num_size * 0.001);
        let x = (page.width - num_width) * 0.5;
        let y = margin_bottom * 0.45;
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
            self.margin_bottom,
        );
    }

    pub(crate) fn finish_streaming(&mut self) -> Vec<Page<'a>> {
        let mut last_pages = Vec::with_capacity(2);
        let page_numbering = self.page_numbering;
        let font_family = self.font_family;
        let margin_bottom = self.margin_bottom;
        if let Some(mut old_pending) = self.pending_page.take() {
            self.completed_pages_count += 1;
            Self::add_page_number_to_page(
                &mut old_pending,
                self.completed_pages_count as u32,
                page_numbering,
                font_family,
                margin_bottom,
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
        let margin_bottom = self.margin_bottom;

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

        self.y = self.page_height - self.margin_top;
        self.has_preceding_text = false;
        self.last_line_width = 0.0;
        self.last_line_indent = 0.0;
        self.prev_graf = 0;
    }

    pub(crate) fn ensure_room(&mut self, required_height: f32) -> bool {
        if self.y - required_height < self.margin_bottom {
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
                self.y -= self.par_skip;
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
            self.y -= self.par_skip;
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
        }
    }

    pub(crate) fn layout_single_block<'doc>(&mut self, b: &'doc Block<'a>, depth: usize) {
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
            Block::VSpace(h) => self.y -= *h,
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
            self.break_cell_words_into_lines(0, self.cell_words_soa.len(), max_w);
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
                self.paint_cell_line_at(
                    line_desc.start_word as usize,
                    (line_desc.start_word + line_desc.word_count) as usize,
                    line_x,
                    self.y,
                    Face::Bold,
                    18.0,
                    &title_math,
                );
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
        let (skip_before, font_size, skip_after) = match level {
            1 => (18.0, 14.0, 6.0),
            2 => (14.0, 12.0, 4.0),
            _ => (10.0, 11.0, 2.0),
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
        let heading_leading = font_size * 1.25;

        let title_lines = if (title_w + num_w) <= max_w && !inlines_have_break(title) {
            1
        } else {
            self.cell_lines.clear();
            self.break_cell_words_into_lines(
                0,
                self.cell_words_soa.len(),
                (max_w - num_w).max(20.0),
            );
            self.cell_lines.len().max(1)
        };

        let title_h = (title_lines as f32) * heading_leading;
        let needed_body_lines = 2.0 * self.base_leading;
        let total_needed = skip_before + title_h + skip_after + needed_body_lines;
        let broken = self.ensure_room(total_needed);
        if !broken && self.y < self.page_height - self.margin_top - 0.1 {
            self.y -= skip_before;
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
            self.y -= font_size + skip_after;
        } else {
            let line_count = self.cell_lines.len();
            for line_idx in 0..line_count {
                let line_desc = self.cell_lines[line_idx];
                let line_x = self.left + self.left_skip + if line_idx == 0 { num_w } else { 0.0 };
                self.paint_cell_line_at(
                    line_desc.start_word as usize,
                    (line_desc.start_word + line_desc.word_count) as usize,
                    line_x,
                    self.y,
                    Face::Bold,
                    font_size,
                    &section_math,
                );
                self.y -= heading_leading;
            }
            self.y -= skip_after;
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
                    n: self.figure_counter,
                });
            }
            let cap_x = self.left + ((self.right - self.left) - figure_width) * 0.5 + tag_width;
            if cap_w <= cap_max_w && !inlines_have_break(caption) {
                self.paint_inlines_at(caption, cap_x, self.y);
                self.y -= self.base_leading;
            } else {
                self.cell_lines.clear();
                self.break_cell_words_into_lines(0, self.cell_words_soa.len(), cap_max_w);
                let line_count = self.cell_lines.len();
                for idx in 0..line_count {
                    let line_desc = self.cell_lines[idx];
                    let cur_y = self.y;
                    self.paint_cell_line(
                        line_desc.start_word as usize,
                        (line_desc.start_word + line_desc.word_count) as usize,
                        cap_x,
                        cur_y,
                        &cap_math,
                    );
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
        self.layout_para_full(inls, None, None);
    }

    fn layout_para_full<'doc>(
        &mut self,
        inls: &'doc [Inline<'a>],
        custom_first_line_indent: Option<f32>,
        custom_hanging_indent: Option<f32>,
    ) {
        if inls.is_empty() {
            return;
        }

        let mut is_noindent = false;
        let mut is_indent = false;
        for inl in inls {
            match inl {
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

        let measure = (self.right - self.right_skip) - (self.left + self.left_skip);
        self.line_descriptors.clear();

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

        let mut pass = 0;

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
                &mut self.hyphen_cache,
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
                    self.hyphen_cache.as_ref(),
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
                    self.hyphen_cache.as_ref(),
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
                    self.hyphen_cache.as_ref(),
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
                    self.hyphen_cache.as_ref(),
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

        self.y -= self.par_skip;
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

    fn flush_line_descriptor(
        &mut self,
        desc: &LineDescriptor,
        math_collector: &[&crate::math::MathNode<'a>],
    ) {
        if desc.word_count == 0 {
            return;
        }
        self.ensure_room(self.base_leading);
        if self.is_dry_run {
            self.y -= self.base_leading;
            return;
        }

        let measure = (self.right - self.right_skip) - (self.left + self.left_skip);
        let available = measure - desc.line_indent;

        let is_last_line = (desc.start_word + desc.word_count) as usize >= self.words_soa.len();
        let target_available = if is_last_line && self.par_fill_width > 0.0 {
            (available - self.par_fill_width).max(0.0)
        } else {
            available
        };

        let word_spacing = if desc.justify() && desc.num_spaces > 0 && desc.line_w < target_available {
            let slack = target_available - desc.line_w;
            let extra = slack / (desc.num_spaces as f32);
            if extra <= 18.0 {
                extra
            } else {
                0.0
            }
        } else if is_last_line && self.last_line_fit > 0 && desc.num_spaces > 0 && desc.line_w < target_available {
            let slack = target_available - desc.line_w;
            let max_fit = (self.last_line_fit as f32 / 1000.0).clamp(0.0, 1.0);
            let extra = (slack / (desc.num_spaces as f32)) * max_fit * 0.25;
            if extra <= 18.0 {
                extra
            } else {
                0.0
            }
        } else {
            0.0
        };

        let mut x = self.left + self.left_skip + desc.line_indent;
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

        while i < end {
            let attr = self.words_soa.attrs[i];
            let w = self.words_soa.widths[i];
            if attr.is_space() {
                let extra = if attr.kind() == WordKind::Space { word_spacing } else { 0.0 };
                x += w + extra;
                i += 1;
                continue;
            }
            if attr.is_math() {
                if let Some(m_idx) = self.words_soa.get_math_idx(i) {
                    if let Some(&node) = math_collector.get(m_idx) {
                        let m_box = crate::math::render_math(
                            node,
                            self.base_font_size,
                            self.font_family,
                            crate::math::MathStyle::Text,
                            x,
                            self.y,
                            &mut self.current_page.ops,
                            &mut self.current_page.text_pool,
                        );
                        x += m_box.w;
                    }
                }
                i += 1;
                continue;
            }
            let face = attr.face();
            let x0 = x;
            let run_start = i;
            let is_cm = self.font_family == metrics::FontFamily::ComputerModern;

            let mut run_w = 0.0f32;
            let mut run_extra = 0.0f32;

            while i < end {
                let cur_attr = self.words_soa.attrs[i];
                let cur_face = cur_attr.face();
                if !cur_attr.is_word_like() && !cur_attr.is_space() {
                    break;
                }
                if cur_attr.is_word_like() && cur_face != face {
                    break;
                }
                if cur_attr.is_space() && is_cm {
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
                    size: self.base_font_size,
                    part_start: pool_start,
                    part_count,
                    word_spacing,
                });
            }

            x += run_w + run_extra;
        }

        if end > start
            && self.words_soa.attrs[end - 1].kind() == WordKind::Hyphen
            && !self.words_soa.texts[end - 1].ends_with('-')
        {
            let hyph_face = self.words_soa.attrs[end - 1].face();
            let pool_start = self.current_page.text_pool.len() as u32;
            self.current_page.text_pool.push("-");
            let hyph_x = x;
            self.push_op(Paint::GlyphRun {
                x: hyph_x,
                y: self.y,
                face: hyph_face,
                size: self.base_font_size,
                part_start: pool_start,
                part_count: 1,
                word_spacing: 0.0,
            });
        }

        self.y -= self.base_leading;
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
        flatten_into(inlines, default_face, &mut scratch_runs);
        let mut scratch_slices = Vec::with_capacity(32);
        let scale = font_size * 0.001;
        for run in &scratch_runs {
            let run = *run;
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
            let space_units = metrics::resolve_space_units(self.font_family, run.face);
            let space_w = space_units as f32 * scale;
            let table = metrics::resolve_width_table(self.font_family, run.face);

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
                            part_count: 1,
                            word_spacing: 0.0,
                        });
                    } else {
                        scratch_slices.push(word);
                    }
                    x += metrics::string_units_table(word, table) as f32 * scale;
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
                        part_count: pool_len,
                        word_spacing: 0.0,
                    });
                }
            }
        }
    }

    pub(crate) fn break_cell_words_into_lines(&mut self, start: usize, end: usize, inner_w: f32) {
        let mut line_start = start;
        let mut line_w = 0.0f32;
        let mut i = start;

        while i < end {
            let attr = self.cell_words_soa.attrs[i];
            let k = attr.kind();
            let w = self.cell_words_soa.widths[i];

            if k == WordKind::Break {
                let (trimmed_end, trimmed_w) = self.trim_cell_line(line_start, i, line_w);
                self.cell_lines.push(CellLineRange {
                    start_word: line_start as u16,
                    word_count: (trimmed_end - line_start) as u16,
                    line_w: trimmed_w,
                });
                i += 1;
                line_start = i;
                line_w = 0.0;
                continue;
            }

            if line_start < i && line_w + w > inner_w && !attr.is_space() {
                let (trimmed_end, trimmed_w) = self.trim_cell_line(line_start, i, line_w);
                self.cell_lines.push(CellLineRange {
                    start_word: line_start as u16,
                    word_count: (trimmed_end - line_start) as u16,
                    line_w: trimmed_w,
                });
                if attr.is_space() {
                    i += 1;
                }
                line_start = i;
                line_w = 0.0;
                continue;
            }

            if line_start == i && attr.is_space() {
                i += 1;
                line_start = i;
                continue;
            }

            line_w += w;
            i += 1;
        }

        if line_start < end {
            let (trimmed_end, trimmed_w) = self.trim_cell_line(line_start, end, line_w);
            if trimmed_end > line_start {
                self.cell_lines.push(CellLineRange {
                    start_word: line_start as u16,
                    word_count: (trimmed_end - line_start) as u16,
                    line_w: trimmed_w,
                });
            }
        }
    }

    pub(crate) fn trim_cell_line(&self, start: usize, mut end: usize, mut line_w: f32) -> (usize, f32) {
        while end > start && self.cell_words_soa.attrs[end - 1].is_space() {
            end -= 1;
            line_w -= self.cell_words_soa.widths[end];
        }
        (end, line_w.max(0.0))
    }

    pub(crate) fn paint_cell_line(
        &mut self,
        start: usize,
        end: usize,
        x: f32,
        y: f32,
        math_collector: &[&crate::math::MathNode<'a>],
    ) {
        self.paint_cell_line_at(
            start,
            end,
            x,
            y,
            Face::Roman,
            self.base_font_size,
            math_collector,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_cell_line_at(
        &mut self,
        start: usize,
        end: usize,
        mut x: f32,
        y: f32,
        _default_face: Face,
        font_size: f32,
        math_collector: &[&crate::math::MathNode<'a>],
    ) {
        if self.is_dry_run {
            return;
        }
        let mut i = start;
        while i < end && self.cell_words_soa.attrs[i].is_space() {
            i += 1;
        }

        while i < end {
            let attr = self.cell_words_soa.attrs[i];
            let w = self.cell_words_soa.widths[i];
            if attr.is_space() {
                x += w;
                i += 1;
                continue;
            }
            if attr.is_math() {
                if let Some(m_idx) = self.cell_words_soa.get_math_idx(i) {
                    if let Some(&node) = math_collector.get(m_idx) {
                        let m_box = crate::math::render_math(
                            node,
                            font_size,
                            self.font_family,
                            crate::math::MathStyle::Text,
                            x,
                            y,
                            &mut self.current_page.ops,
                            &mut self.current_page.text_pool,
                        );
                        x += m_box.w;
                    }
                }
                i += 1;
                continue;
            }
            let face = attr.face();
            let x0 = x;
            let run_start = i;
            let is_cm = self.font_family == metrics::FontFamily::ComputerModern;

            while i < end {
                let cur_attr = self.cell_words_soa.attrs[i];
                let cur_face = cur_attr.face();
                if !cur_attr.is_word_like() && !cur_attr.is_space() {
                    break;
                }
                if cur_attr.is_word_like() && cur_face != face {
                    break;
                }
                if cur_attr.is_space() && is_cm {
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
                    size: font_size,
                    part_start: pool_start,
                    part_count,
                    word_spacing: 0.0,
                });
            }

            for idx in run_start..run_end {
                x += self.cell_words_soa.widths[idx];
            }
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
    let table = metrics::resolve_width_table(family, face);
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

pub(crate) fn section_number_width(
    number: [u16; 4],
    level: u8,
    face: Face,
    size: f32,
    family: metrics::FontFamily,
) -> f32 {
    let table = metrics::resolve_width_table(family, face);
    let digit_units = metrics::resolve_digit_units(family, face);
    let dot_units = table[b'.' as usize] as u32;
    let space_units = metrics::resolve_space_units(family, face);
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
    let table = metrics::resolve_width_table(family, face);
    let open_w = table[b'(' as usize] as f32 * scale;
    let close_w = table[b')' as usize] as f32 * scale;
    let digit_units = metrics::resolve_digit_units(family, face);
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
    pub attrs: Vec<PackedWordAttr>,
    pub texts: Vec<&'a str>,
    pub math_indices: Vec<SparseMathIdx>,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<ParagraphSoA>() == 96);

impl<'a> ParagraphSoA<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            widths: Vec::with_capacity(cap),
            attrs: Vec::with_capacity(cap),
            texts: Vec::with_capacity(cap),
            math_indices: Vec::with_capacity(4),
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.widths.clear();
        self.attrs.clear();
        self.texts.clear();
        self.math_indices.clear();
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
        self.attrs.push(PackedWordAttr::new(kind, face));
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

/// Bit-packed 16-byte formatted line descriptor from hz line breaking.
/// Exactly 4 descriptors fit into a single 64-byte L1 CPU cache line (64 / 16 = 4).
pub use crate::layout::layout_hz::FlatAsciiWidthTable;
pub use crate::layout::layout_hz::HzLineDescriptor as LineDescriptor;

#[derive(Clone, Copy, Debug)]
pub struct CellLineRange {
    pub start_word: u16,
    pub word_count: u16,
    pub line_w: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellSpan {
    pub start: u16,
    pub count: u16,
}

pub(crate) fn inlines_have_break(inls: &[Inline<'_>]) -> bool {
    inls.iter().any(|inl| match inl {
        Inline::Break => true,
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
    is_break: bool,
    math: Option<&'doc crate::math::MathNode<'a>>,
}

fn flatten_into<'a, 'doc>(inls: &'doc [Inline<'a>], face: Face, out: &mut Vec<FlatRun<'a, 'doc>>) {
    for i in inls {
        match i {
            Inline::Text(s) => {
                if !s.is_empty() {
                    out.push(FlatRun {
                        text: s,
                        face,
                        is_break: false,
                        math: None,
                    });
                }
            }
            Inline::Bold(x) => flatten_into(x, face.with_bold(true), out),
            Inline::Italic(x) => flatten_into(x, face.with_italic(true), out),
            Inline::Mono(x) => flatten_into(x, face.with_mono(true), out),
            Inline::Group(x) => flatten_into(x, face, out),
            Inline::Break => out.push(FlatRun {
                text: "",
                face,
                is_break: true,
                math: None,
            }),
            Inline::Math(node) => out.push(FlatRun {
                text: "",
                face,
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
    let space_units = metrics::resolve_space_units(family, face);
    let extra_space_units = metrics::resolve_extra_space_units(family, face);
    let space_w = space_units as f32 * scale;
    let extra_space_w = extra_space_units as f32 * scale;
    let table = metrics::resolve_width_table(family, face);
    let flat_table = FlatAsciiWidthTable::from_table(table);
    let mut total_w = 0.0;

    for inl in inls {
        match inl {
            Inline::Text(s) => {
                let bytes = s.as_bytes();
                let len = bytes.len();
                let mut i = 0;
                while i < len {
                    let b = bytes[i];
                    if is_tex_space_byte(b) {
                        let text = if b == b' ' { &s[i..i + 1] } else { " " };
                        let current_space_w = if *space_factor >= 2000 {
                            space_w + extra_space_w
                        } else {
                            space_w
                        };
                        soa.push(text, face, current_space_w, WordKind::Space);
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
                            let width = word_units as f32 * scale;
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
                                if !french_spacing {
                                    if !cb.is_ascii_lowercase() || fb_sf != 1000 {
                                        fb_sf = crate::layout::layout_hz::update_space_factor(fb_sf, cb, false);
                                    }
                                }
                            }
                            if !french_spacing {
                                *space_factor = fb_sf;
                            }

                            if !has_hyphen && !has_soft_hyphen {
                                let width = metrics::string_units_table(word, table) as f32 * scale;
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
                                        let p_w = metrics::string_units_table(piece, table) as f32 * scale;
                                        soa.push(piece, face, p_w, WordKind::Hyphen);
                                        total_w += p_w;
                                    }
                                    last = shy_pos + '\u{00AD}'.len_utf8();
                                }
                                if last < word.len() {
                                    let piece = &word[last..];
                                    let p_w = metrics::string_units_table(piece, table) as f32 * scale;
                                    soa.push(piece, face, p_w, WordKind::Word);
                                    total_w += p_w;
                                }
                            } else if has_hyphen && word.len() > 1 && !word.starts_with('-') && !word.ends_with('-') {
                                let mut last = 0;
                                for (hyphen_pos, _) in word.match_indices('-') {
                                    let piece = &word[last..=hyphen_pos];
                                    let p_w = metrics::string_units_table(piece, table) as f32 * scale;
                                    soa.push(piece, face, p_w, WordKind::Hyphen);
                                    total_w += p_w;
                                    last = hyphen_pos + 1;
                                }
                                if last < word.len() {
                                    let piece = &word[last..];
                                    let p_w = metrics::string_units_table(piece, table) as f32 * scale;
                                    soa.push(piece, face, p_w, WordKind::Word);
                                    total_w += p_w;
                                }
                            } else {
                                let width = metrics::string_units_table(word, table) as f32 * scale;
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
                total_w += explode_inlines_into_family_sf(x, face.with_bold(true), size, soa, family, math_collector, space_factor, french_spacing)
            }
            Inline::Italic(x) => {
                total_w += explode_inlines_into_family_sf(x, face.with_italic(true), size, soa, family, math_collector, space_factor, french_spacing)
            }
            Inline::Mono(x) => {
                total_w += explode_inlines_into_family_sf(x, face.with_mono(true), size, soa, family, math_collector, space_factor, french_spacing)
            }
            Inline::Group(x) => total_w += explode_inlines_into_family_sf(x, face, size, soa, family, math_collector, space_factor, french_spacing),
            Inline::Break => {
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
            | Inline::Sloppy | Inline::Fussy => {}
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
    let units = measure_inlines_units_family(inls, face, family);
    let mut total_w = units as f32 * (size * 0.001);
    for inl in inls {
        if let Inline::HSpace(dim) = inl {
            total_w += *dim;
        }
    }
    total_w
}

fn measure_inlines_units_family(
    inls: &[Inline<'_>],
    face: Face,
    family: metrics::FontFamily,
) -> u32 {
    let mut total_units: u32 = 0;
    let table = metrics::resolve_width_table(family, face);
    let space_units = metrics::resolve_space_units(family, face);

    for inl in inls {
        match inl {
            Inline::Text(s) => {
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
                        total_units += metrics::string_units_table(word, table);
                    }
                }
            }
            Inline::Bold(x) => {
                total_units += measure_inlines_units_family(x, face.with_bold(true), family);
            }
            Inline::Italic(x) => {
                total_units += measure_inlines_units_family(x, face.with_italic(true), family);
            }
            Inline::Mono(x) => {
                total_units += measure_inlines_units_family(x, face.with_mono(true), family);
            }
            Inline::Group(x) => {
                total_units += measure_inlines_units_family(x, face, family);
            }
            Inline::Break => {}
            Inline::Math(node) => {
                let mb =
                    crate::math::measure_math(node, 1000.0, family, crate::math::MathStyle::Text);
                total_units += mb.w as u32;
            }
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
            | Inline::HSpace(_) => {}
        }
    }
    total_units
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
    let mut total_units: u32 = 0;
    let space_units = metrics::resolve_space_units(family, face);
    let table = metrics::resolve_width_table(family, face);
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
            total_units += metrics::string_units_table(word, table);
        }
    }
    total_units as f32 * (size * 0.001)
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
            assert_eq!(d1.num_spaces, d2.num_spaces);
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
