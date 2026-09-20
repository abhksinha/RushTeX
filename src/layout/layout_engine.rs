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
use crate::tex_parser::{Block, Document, Graphic, Inline, TableData};

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
    pub used_faces: u8,
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
            Paint::Rule { .. } => {}
        }
        self.ops.push(op);
    }

    #[inline(always)]
    pub fn compute_used_faces(&self) -> u8 {
        if self.used_faces != 0 {
            self.used_faces
        } else {
            let mut mask = 0;
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
                    Paint::Rule { .. } => {}
                }
            }
            mask
        }
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
}

pub fn layout<'a>(document: &'a Document<'a>) -> Vec<Page<'a>> {
    let has_indices = document.blocks.iter().any(|b| {
        matches!(
            b,
            Block::TableOfContents | Block::ListOfFigures | Block::ListOfTables
        )
    });

    let registry = if has_indices {
        let mut dry_registry = IndexRegistry::new();
        pre_scan_entries(&document.blocks, &mut dry_registry);

        let mut dry_layouter = Layouter::new(&document.preamble.config);
        dry_layouter.is_dry_run = true;
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
    pub(crate) after_heading: bool,
    scratch_runs: Vec<FlatRun<'a>>,
    pub(crate) words_soa: ParagraphSoA<'a>,
    pub(crate) line_descriptors: Vec<LineDescriptor>,
    pub(crate) cell_words_soa: ParagraphSoA<'a>,
    pub(crate) cell_lines: Vec<CellLineRange>,
    pub(crate) cell_line_spans: Vec<(usize, usize)>,
    pub(crate) registry: IndexRegistry<'a>,
    pub(crate) is_dry_run: bool,
    pub(crate) current_sec_idx: usize,
    pub(crate) current_fig_idx: usize,
    pub(crate) current_tbl_idx: usize,
    pub(crate) section_counters: [u16; 4],
}

impl<'a> Layouter<'a> {
    fn new(config: &DocumentConfig) -> Self {
        let left = config.margin_left;
        let right = config.page_width - config.margin_right;
        let y = config.page_height - config.margin_top;
        Self {
            pages: Vec::new(),
            current_page: Page {
                width: config.page_width,
                height: config.page_height,
                font_family: config.font_family,
                used_faces: 0,
                ops: Vec::with_capacity(128),
                text_pool: Vec::with_capacity(256),
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
            after_heading: true,
            scratch_runs: Vec::with_capacity(64),
            words_soa: ParagraphSoA::with_capacity(256),
            line_descriptors: Vec::with_capacity(32),
            cell_words_soa: ParagraphSoA::with_capacity(64),
            cell_lines: Vec::with_capacity(16),
            cell_line_spans: Vec::with_capacity(8),
            registry: IndexRegistry::new(),
            is_dry_run: false,
            current_sec_idx: 0,
            current_fig_idx: 0,
            current_tbl_idx: 0,
            section_counters: [0; 4],
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

    fn finish(mut self) -> Vec<Page<'a>> {
        if !self.current_page.ops.is_empty() || self.pages.is_empty() {
            self.pages.push(self.current_page);
        }
        if self.page_numbering {
            for (page_idx, page) in self.pages.iter_mut().enumerate() {
                let page_number = (page_idx + 1) as u32;
                let digit_count = count_digits(page_number);
                let page_num_size = 10.0f32;
                let digit_units = metrics::resolve_digit_units(self.font_family, Face::Roman);
                let num_width = (digit_count * digit_units) as f32 * (page_num_size * 0.001);
                let x = (page.width - num_width) * 0.5;
                let y = self.margin_bottom * 0.45;
                page.push_op(Paint::PageNumber {
                    x,
                    y,
                    face: Face::Roman,
                    size: page_num_size,
                    page_number,
                });
            }
        }
        self.pages
    }

    #[inline(always)]
    pub(crate) fn push_op(&mut self, op: Paint) {
        self.current_page.push_op(op);
    }

    pub(crate) fn new_page(&mut self) {
        let previous_page = std::mem::replace(
            &mut self.current_page,
            Page {
                width: self.page_width,
                height: self.page_height,
                font_family: self.font_family,
                used_faces: 0,
                ops: Vec::with_capacity(128),
                text_pool: Vec::with_capacity(256),
            },
        );
        self.pages.push(previous_page);
        self.y = self.page_height - self.margin_top;
    }

    pub(crate) fn ensure_room(&mut self, required_height: f32) -> bool {
        if self.y - required_height < self.margin_bottom {
            self.new_page();
            true
        } else {
            false
        }
    }

    fn layout_blocks(&mut self, blocks: &'a [Block<'a>], depth: usize) {
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

    fn apply_config_change(&mut self, change: &crate::tex_parser::ConfigChange) {
        match *change {
            crate::tex_parser::ConfigChange::ParIndent(v) => self.par_indent = v,
            crate::tex_parser::ConfigChange::ParSkip(v) => self.par_skip = v,
            crate::tex_parser::ConfigChange::HangIndent(v) => self.hang_indent = v,
            crate::tex_parser::ConfigChange::HangAfter(v) => self.hang_after = v,
            crate::tex_parser::ConfigChange::LeftSkip(v) => self.left_skip = v,
            crate::tex_parser::ConfigChange::RightSkip(v) => self.right_skip = v,
            crate::tex_parser::ConfigChange::LabelSep(v) => self.list_label_sep = v,
            crate::tex_parser::ConfigChange::LeftMargin(v) => self.list_left_margin = v,
            crate::tex_parser::ConfigChange::ItemIndent(v) => self.list_item_indent = v,
            crate::tex_parser::ConfigChange::ItemSep(v) => self.list_item_sep = v,
            crate::tex_parser::ConfigChange::ListParIndent(v) => self.list_par_indent = v,
        }
    }

    fn layout_single_block(&mut self, b: &'a Block<'a>, depth: usize) {
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

    fn layout_list(
        &mut self,
        kind: crate::frontend::ast::ListKind,
        options: &crate::tex_parser::ListOptions,
        items: &'a [crate::frontend::ast::ListItem<'a>],
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
                let marker_w = self.marker_width(marker, Face::Roman, self.base_font_size);

                let first_line_x = self.left + left_margin + item_indent;
                let mut marker_x = first_line_x - label_sep - marker_w;
                if marker_x < self.left {
                    marker_x = self.left;
                }

                if !self.is_dry_run {
                    self.push_op(Paint::Marker {
                        x: marker_x,
                        y: self.y,
                        face: Face::Roman,
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

    fn layout_title_block(
        &mut self,
        title: &'a [Inline<'a>],
        author: Option<&'a [Inline<'a>]>,
        date: Option<&'a [Inline<'a>]>,
    ) {
        let max_w = (self.right - self.right_skip) - (self.left + self.left_skip);
        self.cell_words_soa.clear();
        let title_w = explode_inlines_into_family(
            title,
            Face::Bold,
            18.0,
            &mut self.cell_words_soa,
            self.font_family,
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
            let lines = self.cell_lines.clone();
            for line_desc in lines {
                let line_x = self.left + self.left_skip + (max_w - line_desc.line_w).max(0.0) * 0.5;
                self.paint_cell_line_at(
                    line_desc.start_word as usize,
                    (line_desc.start_word + line_desc.word_count) as usize,
                    line_x,
                    self.y,
                    Face::Bold,
                    18.0,
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
    }

    fn layout_section(&mut self, level: u8, starred: bool, title: &'a [Inline<'a>]) {
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
        let title_w = explode_inlines_into_family(
            title,
            Face::Bold,
            font_size,
            &mut self.cell_words_soa,
            self.font_family,
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
            self.registry.toc_entries[self.current_sec_idx].page = (self.pages.len() + 1) as u32;
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
            let lines = self.cell_lines.clone();
            for (line_idx, line_desc) in lines.into_iter().enumerate() {
                let line_x = self.left + self.left_skip + if line_idx == 0 { num_w } else { 0.0 };
                self.paint_cell_line_at(
                    line_desc.start_word as usize,
                    (line_desc.start_word + line_desc.word_count) as usize,
                    line_x,
                    self.y,
                    Face::Bold,
                    font_size,
                );
                self.y -= heading_leading;
            }
            self.y -= skip_after;
        }
        self.after_heading = true;
    }

    fn layout_table(&mut self, table_data: &'a TableData<'a>) {
        crate::tables::layout_table(self, table_data);
    }

    fn layout_figure(&mut self, graphic: Option<Graphic<'a>>, caption: &'a [Inline<'a>]) {
        self.figure_counter += 1;
        let figure_width = (self.right - self.left).min(360.0);
        let figure_height = FIG_H;
        self.ensure_room(figure_height + self.base_leading * 2.0 + PARA_SKIP);

        if self.current_fig_idx < self.registry.figure_entries.len() {
            self.registry.figure_entries[self.current_fig_idx].page = (self.pages.len() + 1) as u32;
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
            let cap_w = explode_inlines_into_family(
                caption,
                Face::Roman,
                self.base_font_size,
                &mut self.cell_words_soa,
                self.font_family,
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
                let lines = self.cell_lines.clone();
                for (idx, line_desc) in lines.into_iter().enumerate() {
                    let cur_y = self.y;
                    self.paint_cell_line(
                        line_desc.start_word as usize,
                        (line_desc.start_word + line_desc.word_count) as usize,
                        cap_x,
                        cur_y,
                    );
                    if idx + 1 < self.cell_lines.len() {
                        self.y -= self.base_leading;
                    }
                }
                self.y -= self.base_leading;
            }
        }
        self.after_heading = false;
    }

    fn layout_equation(&mut self, starred: bool, math: &'a crate::math::MathNode<'a>) {
        let m_box = crate::math::measure_math(
            math,
            self.base_font_size,
            self.font_family,
            crate::math::MathStyle::Display,
        );

        let eq_num = if !starred {
            self.equation_counter += 1;
            Some(self.equation_counter)
        } else {
            None
        };

        let num_w = if let Some(n) = eq_num {
            equation_number_width(n, Face::Roman, self.base_font_size, self.font_family)
        } else {
            0.0
        };

        let above_skip = 8.0f32;
        let below_skip = 8.0f32;
        let total_h = above_skip + m_box.h + m_box.d + below_skip;

        self.ensure_room(total_h);

        self.y -= above_skip;
        let baseline_y = self.y - m_box.h;

        let available_w = (self.right - self.right_skip) - (self.left + self.left_skip);
        let mut eq_x = (self.left + self.left_skip) + (available_w - m_box.w) * 0.5;

        let right_margin = self.right - self.right_skip;
        let num_x = right_margin - num_w;

        if eq_num.is_some() && eq_x + m_box.w > num_x - 12.0 {
            eq_x = self.left + self.left_skip;
        }

        if !self.is_dry_run {
            crate::math::render_math(
                math,
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

            if let Some(n) = eq_num {
                self.push_op(Paint::EquationNumber {
                    x: num_x,
                    y: baseline_y,
                    face: Face::Roman,
                    size: self.base_font_size,
                    n,
                });
            }
        }

        self.y = baseline_y - m_box.d - below_skip;
        self.after_heading = false;
    }

    fn layout_code_listing(&mut self, listing: &'a crate::code_listing::CodeListingData<'a>) {
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
                    if let Some(p) = self.pages.last_mut() {
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

    fn layout_para(&mut self, inls: &'a [Inline<'a>]) {
        self.layout_para_full(inls, None, None);
    }

    fn layout_para_full(
        &mut self,
        inls: &'a [Inline<'a>],
        custom_first_line_indent: Option<f32>,
        custom_hanging_indent: Option<f32>,
    ) {
        self.words_soa.clear();
        explode_inlines_into_family(
            inls,
            Face::Roman,
            self.base_font_size,
            &mut self.words_soa,
            self.font_family,
        );
        if self.words_soa.is_empty() {
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
                _ => {}
            }
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
        let custom_hang = custom_hanging_indent.unwrap_or(0.0);

        let measure = (self.right - self.right_skip) - (self.left + self.left_skip);
        self.line_descriptors.clear();

        let num_words = self.words_soa.len();
        let mut line_index: usize = 0;
        let mut line_start: u32 = 0;
        let mut line_w: f32 = 0.0;
        let mut num_spaces: u32 = 0;
        let mut line_indent = self.calculate_line_indent(0, first_line_indent, custom_hang);
        let mut available = measure - line_indent;

        let mut word_idx = 0;
        while word_idx < num_words {
            let k = self.words_soa.kinds[word_idx];
            let w = self.words_soa.widths[word_idx];

            if k == WordKind::Break {
                let (trimmed_end, trimmed_w, trimmed_sp) =
                    self.trim_line_trailing_space(line_start, word_idx as u32, line_w, num_spaces);
                self.line_descriptors.push(LineDescriptor {
                    start_word: line_start as u16,
                    word_count: (trimmed_end - line_start) as u16,
                    line_w: trimmed_w,
                    num_spaces: trimmed_sp as u16,
                    line_indent,
                    justify: false,
                });
                line_index += 1;
                line_indent =
                    self.calculate_line_indent(line_index, first_line_indent, custom_hang);
                available = measure - line_indent;
                word_idx += 1;
                line_start = word_idx as u32;
                line_w = 0.0;
                num_spaces = 0;
                continue;
            }

            if line_start < word_idx as u32 && line_w + w > available && k != WordKind::Space {
                let (trimmed_end, trimmed_w, trimmed_sp) =
                    self.trim_line_trailing_space(line_start, word_idx as u32, line_w, num_spaces);
                self.line_descriptors.push(LineDescriptor {
                    start_word: line_start as u16,
                    word_count: (trimmed_end - line_start) as u16,
                    line_w: trimmed_w,
                    num_spaces: trimmed_sp as u16,
                    line_indent,
                    justify: true,
                });
                line_index += 1;
                line_indent =
                    self.calculate_line_indent(line_index, first_line_indent, custom_hang);
                available = measure - line_indent;
                if k == WordKind::Space {
                    word_idx += 1;
                }
                line_start = word_idx as u32;
                line_w = 0.0;
                num_spaces = 0;
                continue;
            }

            if line_start == word_idx as u32 && k == WordKind::Space {
                word_idx += 1;
                line_start = word_idx as u32;
                continue;
            }

            line_w += w;
            if k == WordKind::Space {
                num_spaces += 1;
            }
            word_idx += 1;
        }

        if line_start < num_words as u32 {
            let (trimmed_end, trimmed_w, trimmed_sp) =
                self.trim_line_trailing_space(line_start, num_words as u32, line_w, num_spaces);
            if trimmed_end > line_start {
                self.line_descriptors.push(LineDescriptor {
                    start_word: line_start as u16,
                    word_count: (trimmed_end - line_start) as u16,
                    line_w: trimmed_w,
                    num_spaces: trimmed_sp as u16,
                    line_indent,
                    justify: false,
                });
            }
        }

        // Reset one-shot hanging indent
        self.hang_indent = 0.0;
        self.hang_after = 1;

        for d in 0..self.line_descriptors.len() {
            let desc = self.line_descriptors[d];
            self.flush_line_descriptor(&desc);
        }
    }

    fn calculate_line_indent(
        &self,
        line_index: usize,
        first_line_indent: f32,
        custom_hang: f32,
    ) -> f32 {
        let mut indent = if line_index == 0 {
            first_line_indent
        } else {
            custom_hang
        };

        if self.hang_indent != 0.0 {
            let line_num = (line_index + 1) as i32;
            if self.hang_after >= 0 {
                if line_num > self.hang_after {
                    indent += self.hang_indent;
                }
            } else if line_num <= self.hang_after.abs() {
                indent += self.hang_indent;
            }
        }
        indent
    }

    fn trim_line_trailing_space(
        &self,
        start: u32,
        end: u32,
        mut line_w: f32,
        mut num_spaces: u32,
    ) -> (u32, f32, u32) {
        let mut trimmed_end = end;
        while trimmed_end > start {
            let prev = (trimmed_end - 1) as usize;
            if self.words_soa.kinds[prev] == WordKind::Space {
                line_w -= self.words_soa.widths[prev];
                num_spaces = num_spaces.saturating_sub(1);
                trimmed_end -= 1;
            } else {
                break;
            }
        }
        (trimmed_end, line_w.max(0.0), num_spaces)
    }

    fn flush_line_descriptor(&mut self, desc: &LineDescriptor) {
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

        let word_spacing = if desc.justify && desc.num_spaces > 0 && desc.line_w < available {
            let slack = available - desc.line_w;
            let extra = slack / (desc.num_spaces as f32);
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

        let mut i = start;
        while i < end {
            let k = self.words_soa.kinds[i];
            let w = self.words_soa.widths[i];
            if k == WordKind::Space {
                x += w + word_spacing;
                i += 1;
                continue;
            }
            if k == WordKind::Math {
                if let Some(node) = self.words_soa.math_nodes[i] {
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
                i += 1;
                continue;
            }
            let face = self.words_soa.faces[i];
            let x0 = x;
            let pool_start = self.current_page.text_pool.len() as u32;
            while i < end {
                let cur_k = self.words_soa.kinds[i];
                let cur_face = self.words_soa.faces[i];
                if cur_k != WordKind::Word && cur_k != WordKind::Space {
                    break;
                }
                if cur_k == WordKind::Word && cur_face != face {
                    break;
                }
                self.current_page.text_pool.push(self.words_soa.texts[i]);
                let extra_sp = if cur_k == WordKind::Space {
                    word_spacing
                } else {
                    0.0
                };
                x += self.words_soa.widths[i] + extra_sp;
                i += 1;
            }
            let pool_len = (self.current_page.text_pool.len() as u32) - pool_start;
            self.push_op(Paint::GlyphRun {
                x: x0,
                y: self.y,
                face,
                size: self.base_font_size,
                part_start: pool_start,
                part_count: pool_len,
                word_spacing,
            });
        }
        self.y -= self.base_leading;
    }

    fn paint_inlines_at(&mut self, inlines: &'a [Inline<'a>], x: f32, y: f32) {
        self.paint_inlines_at_size(inlines, Face::Roman, self.base_font_size, x, y);
    }

    pub(crate) fn paint_inlines_at_size(
        &mut self,
        inlines: &'a [Inline<'a>],
        default_face: Face,
        font_size: f32,
        mut x: f32,
        y: f32,
    ) {
        if self.is_dry_run {
            return;
        }
        self.scratch_runs.clear();
        flatten_into(inlines, default_face, &mut self.scratch_runs);
        let scale = font_size * 0.001;
        for r_idx in 0..self.scratch_runs.len() {
            let run = self.scratch_runs[r_idx];
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
            let pool_start = self.current_page.text_pool.len() as u32;
            let x0 = x;
            let space_units = metrics::resolve_space_units(self.font_family, run.face);
            let space_w = space_units as f32 * scale;
            let table = metrics::resolve_width_table(self.font_family, run.face);

            while i < bytes.len() {
                let b = bytes[i];
                if is_tex_space_byte(b) {
                    self.current_page.text_pool.push(" ");
                    x += space_w;
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
                    self.current_page.text_pool.push(word);
                    x += metrics::string_units_table(word, table) as f32 * scale;
                }
            }
            let pool_len = (self.current_page.text_pool.len() as u32) - pool_start;
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

    pub(crate) fn break_cell_words_into_lines(&mut self, start: usize, end: usize, inner_w: f32) {
        let mut line_start = start;
        let mut line_w = 0.0f32;
        let mut i = start;

        while i < end {
            let k = self.cell_words_soa.kinds[i];
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

            if line_start < i && line_w + w > inner_w && k != WordKind::Space {
                let (trimmed_end, trimmed_w) = self.trim_cell_line(line_start, i, line_w);
                self.cell_lines.push(CellLineRange {
                    start_word: line_start as u16,
                    word_count: (trimmed_end - line_start) as u16,
                    line_w: trimmed_w,
                });
                if k == WordKind::Space {
                    i += 1;
                }
                line_start = i;
                line_w = 0.0;
                continue;
            }

            if line_start == i && k == WordKind::Space {
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

    fn trim_cell_line(&self, start: usize, mut end: usize, mut line_w: f32) -> (usize, f32) {
        while end > start && self.cell_words_soa.kinds[end - 1] == WordKind::Space {
            end -= 1;
            line_w -= self.cell_words_soa.widths[end];
        }
        (end, line_w.max(0.0))
    }

    pub(crate) fn paint_cell_line(&mut self, start: usize, end: usize, x: f32, y: f32) {
        self.paint_cell_line_at(start, end, x, y, Face::Roman, self.base_font_size);
    }

    fn paint_cell_line_at(
        &mut self,
        start: usize,
        end: usize,
        mut x: f32,
        y: f32,
        _default_face: Face,
        font_size: f32,
    ) {
        if self.is_dry_run {
            return;
        }
        let mut i = start;
        while i < end {
            let k = self.cell_words_soa.kinds[i];
            let w = self.cell_words_soa.widths[i];
            if k == WordKind::Space {
                x += w;
                i += 1;
                continue;
            }
            if k == WordKind::Math {
                if let Some(node) = self.cell_words_soa.math_nodes[i] {
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
                i += 1;
                continue;
            }
            let face = self.cell_words_soa.faces[i];
            let x0 = x;
            let pool_start = self.current_page.text_pool.len() as u32;
            while i < end {
                let cur_k = self.cell_words_soa.kinds[i];
                let cur_face = self.cell_words_soa.faces[i];
                if cur_k != WordKind::Word && cur_k != WordKind::Space {
                    break;
                }
                if cur_k == WordKind::Word && cur_face != face {
                    break;
                }
                self.current_page
                    .text_pool
                    .push(self.cell_words_soa.texts[i]);
                x += self.cell_words_soa.widths[i];
                i += 1;
            }
            let pool_len = (self.current_page.text_pool.len() as u32) - pool_start;
            if pool_len > 0 {
                self.push_op(Paint::GlyphRun {
                    x: x0,
                    y,
                    face,
                    size: font_size,
                    part_start: pool_start,
                    part_count: pool_len,
                    word_spacing: 0.0,
                });
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
    Word,
    Space,
    Break,
    Math,
}

#[derive(Default)]
pub struct ParagraphSoA<'a> {
    pub widths: Vec<f32>,
    pub kinds: Vec<WordKind>,
    pub texts: Vec<&'a str>,
    pub faces: Vec<Face>,
    pub math_nodes: Vec<Option<&'a crate::math::MathNode<'a>>>,
}

impl<'a> ParagraphSoA<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            widths: Vec::with_capacity(cap),
            kinds: Vec::with_capacity(cap),
            texts: Vec::with_capacity(cap),
            faces: Vec::with_capacity(cap),
            math_nodes: Vec::with_capacity(cap),
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.widths.clear();
        self.kinds.clear();
        self.texts.clear();
        self.faces.clear();
        self.math_nodes.clear();
    }

    #[inline]
    pub fn push(&mut self, text: &'a str, face: Face, width: f32, kind: WordKind) {
        self.widths.push(width);
        self.kinds.push(kind);
        self.texts.push(text);
        self.faces.push(face);
        self.math_nodes.push(None);
    }

    #[inline]
    pub fn push_math(&mut self, width: f32, node: &'a crate::math::MathNode<'a>) {
        self.widths.push(width);
        self.kinds.push(WordKind::Math);
        self.texts.push("");
        self.faces.push(Face::Roman);
        self.math_nodes.push(Some(node));
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

/// Compact 16-byte formatted line descriptor aligned to power-of-two.
/// Exactly 4 descriptors fit into a single 64-byte L1 CPU cache line (64 / 16 = 4).
#[derive(Clone, Copy, Debug)]
pub struct LineDescriptor {
    pub start_word: u16,
    pub word_count: u16,
    pub num_spaces: u16,
    pub justify: bool,
    pub line_w: f32,
    pub line_indent: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct CellLineRange {
    pub start_word: u16,
    pub word_count: u16,
    pub line_w: f32,
}

fn inlines_have_break(inls: &[Inline<'_>]) -> bool {
    inls.iter().any(|inl| match inl {
        Inline::Break => true,
        Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x) => {
            inlines_have_break(x)
        }
        _ => false,
    })
}

#[derive(Clone, Copy)]
struct FlatRun<'a> {
    text: &'a str,
    face: Face,
    is_break: bool,
    math: Option<&'a crate::math::MathNode<'a>>,
}

fn flatten_into<'a>(inls: &'a [Inline<'a>], face: Face, out: &mut Vec<FlatRun<'a>>) {
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
            | Inline::HSpace(_) => {}
        }
    }
}

#[allow(dead_code)]
fn explode_inlines_into<'a>(
    inls: &'a [Inline<'a>],
    face: Face,
    size: f32,
    soa: &mut ParagraphSoA<'a>,
) -> f32 {
    explode_inlines_into_family(inls, face, size, soa, metrics::FontFamily::Times)
}

pub(crate) fn explode_inlines_into_family<'a>(
    inls: &'a [Inline<'a>],
    face: Face,
    size: f32,
    soa: &mut ParagraphSoA<'a>,
    family: metrics::FontFamily,
) -> f32 {
    let scale = size * 0.001;
    let space_units = metrics::resolve_space_units(family, face);
    let space_w = space_units as f32 * scale;
    let table = metrics::resolve_width_table(family, face);
    let mut total_w = 0.0;

    for inl in inls {
        match inl {
            Inline::Text(s) => {
                let bytes = s.as_bytes();
                let mut i = 0;
                while i < bytes.len() {
                    let b = bytes[i];
                    if is_tex_space_byte(b) {
                        let text = if b == b' ' { &s[i..i + 1] } else { " " };
                        soa.push(text, face, space_w, WordKind::Space);
                        total_w += space_w;
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
                        let width = metrics::string_units_table(word, table) as f32 * scale;
                        soa.push(word, face, width, WordKind::Word);
                        total_w += width;
                    }
                }
            }
            Inline::Bold(x) => {
                total_w += explode_inlines_into_family(x, face.with_bold(true), size, soa, family)
            }
            Inline::Italic(x) => {
                total_w += explode_inlines_into_family(x, face.with_italic(true), size, soa, family)
            }
            Inline::Mono(x) => {
                total_w += explode_inlines_into_family(x, face.with_mono(true), size, soa, family)
            }
            Inline::Group(x) => total_w += explode_inlines_into_family(x, face, size, soa, family),
            Inline::Break => {
                soa.push("", face, 0.0, WordKind::Break);
            }
            Inline::HSpace(dim) => {
                if *dim > 0.0 {
                    soa.push("", face, *dim, WordKind::Space);
                    total_w += *dim;
                }
            }
            Inline::Math(node) => {
                let mb =
                    crate::math::measure_math(node, size, family, crate::math::MathStyle::Text);
                soa.push_math(mb.w, node);
                total_w += mb.w;
            }
            Inline::NoIndent | Inline::Indent | Inline::HangIndent(_) | Inline::HangAfter(_) => {}
        }
    }
    total_w
}

#[inline]
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
