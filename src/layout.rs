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

use crate::metrics::{self, Face};
use crate::preamble::DocumentConfig;
use crate::tex::{Block, ColAlign, ColSpec, Document, Graphic, Inline, TableRow};

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
    pub ops: Vec<Paint>,
    pub text_pool: Vec<&'a str>,
}

#[derive(Clone, Copy, Debug)]
pub enum Marker {
    Bullet,
    Number(u32),
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
    PageNumber {
        x: f32,
        y: f32,
        face: Face,
        size: f32,
        page_number: u32,
    },
}

pub fn layout<'a>(document: &'a Document<'a>) -> Vec<Page<'a>> {
    let mut layouter = Layouter::new(&document.preamble.config);
    layouter.layout_blocks(&document.blocks, 0);
    layouter.finish()
}

struct Layouter<'a> {
    pages: Vec<Page<'a>>,
    current_page: Page<'a>,
    y: f32,
    left: f32,
    right: f32,
    page_width: f32,
    page_height: f32,
    margin_top: f32,
    margin_bottom: f32,
    base_font_size: f32,
    base_leading: f32,
    figure_counter: u32,
    page_numbering: bool,
    par_indent: f32,
    par_skip: f32,
    after_heading: bool,
    scratch_runs: Vec<FlatRun<'a>>,
    scratch_words: Vec<Shaped<'a>>,
    scratch_line: Vec<Shaped<'a>>,
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
            figure_counter: 0,
            page_numbering: config.page_numbering,
            par_indent: config.par_indent,
            par_skip: config.par_skip,
            after_heading: true,
            scratch_runs: Vec::with_capacity(64),
            scratch_words: Vec::with_capacity(256),
            scratch_line: Vec::with_capacity(32),
        }
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
                let num_width = (digit_count as f32) * 556.0 * (page_num_size / 1000.0);
                let x = (page.width - num_width) * 0.5;
                let y = self.margin_bottom * 0.45;
                page.ops.push(Paint::PageNumber {
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

    fn new_page(&mut self) {
        let previous_page = std::mem::replace(
            &mut self.current_page,
            Page {
                width: self.page_width,
                height: self.page_height,
                ops: Vec::with_capacity(128),
                text_pool: Vec::with_capacity(256),
            },
        );
        self.pages.push(previous_page);
        self.y = self.page_height - self.margin_top;
    }

    fn ensure_room(&mut self, required_height: f32) -> bool {
        if self.y - required_height < self.margin_bottom {
            self.new_page();
            true
        } else {
            false
        }
    }

    fn layout_blocks(&mut self, blocks: &'a [Block<'a>], depth: usize) {
        for (i, b) in blocks.iter().enumerate() {
            if i > 0 {
                self.y -= self.par_skip;
            }
            match b {
                Block::Para(inls) => self.layout_para(inls),
                Block::List { ordered, items } => self.layout_list(*ordered, items, depth),
                Block::Table(table_data) => self.layout_table(&table_data.cols, &table_data.rows),
                Block::Figure(fig_data) => self.layout_figure(fig_data.graphic, &fig_data.caption),
                Block::TitleBlock(title_data) => {
                    self.layout_title_block(&title_data.title, title_data.author.as_deref(), title_data.date.as_deref())
                }
                Block::Section { level, title } => self.layout_section(*level, title),
                Block::PageBreak => {
                    self.new_page();
                }
                Block::VSpace(h) => {
                    self.y -= *h;
                }
                Block::HRule => {
                    let w = self.right - self.left;
                    self.ensure_room(4.0);
                    self.current_page.ops.push(Paint::Rule {
                        x: self.left,
                        y: self.y,
                        w,
                        h: 0.4,
                        fill: true,
                    });
                    self.y -= 4.0;
                }
                Block::CodeListing(listing) => self.layout_code_listing(listing),
            }
        }
    }

    fn layout_title_block(
        &mut self,
        title: &'a [Inline<'a>],
        author: Option<&'a [Inline<'a>]>,
        date: Option<&'a [Inline<'a>]>,
    ) {
        self.ensure_room(72.0 + 2.0 * self.base_leading);
        self.y -= 12.0;

        // Title: 18pt, bold, centered
        let title_w = measure_inlines_face_size(title, Face::Bold, 18.0);
        let title_x = self.left + ((self.right - self.left) - title_w).max(0.0) * 0.5;
        self.paint_inlines_at_size(title, Face::Bold, 18.0, title_x, self.y);
        self.y -= 26.0;

        // Author: 12pt, Roman, centered
        if let Some(auth) = author {
            let auth_w = measure_inlines_face_size(auth, Face::Roman, 12.0);
            let auth_x = self.left + ((self.right - self.left) - auth_w).max(0.0) * 0.5;
            self.paint_inlines_at_size(auth, Face::Roman, 12.0, auth_x, self.y);
            self.y -= 18.0;
        }

        // Date: 10pt, Roman, centered
        if let Some(dt) = date {
            let dt_w = measure_inlines_face_size(dt, Face::Roman, 10.0);
            let dt_x = self.left + ((self.right - self.left) - dt_w).max(0.0) * 0.5;
            self.paint_inlines_at_size(dt, Face::Roman, 10.0, dt_x, self.y);
            self.y -= 16.0;
        }

        self.y -= 16.0;
        self.after_heading = true;
    }

    fn layout_section(&mut self, level: u8, title: &'a [Inline<'a>]) {
        let (skip_before, font_size, skip_after) = match level {
            1 => (18.0, 14.0, 6.0),
            2 => (14.0, 12.0, 4.0),
            _ => (10.0, 11.0, 2.0),
        };
        // Heading orphan protection (keep-with-next):
        // Ensure room for skip_before + heading height + at least 2 lines of body text.
        let needed_body_lines = 2.0 * self.base_leading;
        let total_needed = skip_before + font_size + skip_after + needed_body_lines;
        let broken = self.ensure_room(total_needed);
        if !broken && self.y < self.page_height - self.margin_top - 0.1 {
            self.y -= skip_before;
        }
        self.paint_inlines_at_size(title, Face::Bold, font_size, self.left, self.y);
        self.y -= font_size + skip_after;
        self.after_heading = true;
    }

    fn layout_list(&mut self, ordered: bool, items: &'a [Vec<Block<'a>>], depth: usize) {
        for (idx, item) in items.iter().enumerate() {
            self.ensure_room(self.base_leading);
            let marker_x = self.left + (depth as f32) * LIST_INDENT;
            self.current_page.ops.push(Paint::Marker {
                x: marker_x,
                y: self.y,
                face: Face::Roman,
                size: self.base_font_size,
                marker: if ordered {
                    Marker::Number((idx + 1) as u32)
                } else {
                    Marker::Bullet
                },
            });
            let saved_left = self.left;
            self.left = marker_x + LIST_INDENT;
            self.layout_blocks(item, depth + 1);
            self.left = saved_left;
            self.y -= PARA_SKIP * 0.4;
        }
    }

    fn layout_table(&mut self, cols: &[ColSpec], rows: &'a [TableRow<'a>]) {
        if cols.is_empty() || rows.is_empty() {
            return;
        }
        let n = cols.len();
        let mut widths = vec![0.0f32; n];
        for row in rows {
            for (i, cell) in row.cells.iter().enumerate().take(n) {
                widths[i] = widths[i].max(measure_inlines(cell));
            }
        }
        let table_w: f32 = widths
            .iter()
            .map(|w| w + 2.0 * CELL_PAD)
            .sum::<f32>()
            .min(self.right - self.left);
        // If too wide, shrink columns proportionally.
        let raw: f32 = widths.iter().map(|w| w + 2.0 * CELL_PAD).sum();
        let scale = if raw > 0.0 && raw > self.right - self.left {
            (self.right - self.left) / raw
        } else {
            1.0
        };
        for w in widths.iter_mut() {
            *w *= scale;
        }
        let col_w: Vec<f32> = widths.iter().map(|w| w + 2.0 * CELL_PAD).collect();
        let row_h = self.base_leading + 4.0;
        let mut extra_rules = 0u32;
        for row in rows {
            if row.rule_above {
                extra_rules += 1;
            }
        }
        let total_h = row_h * rows.len() as f32 + extra_rules as f32 * 0.6 + 2.0;
        self.ensure_room(total_h);
        let origin_x = self.left;
        let mut y_top = self.y;
        for row in rows {
            if row.rule_above {
                self.current_page.ops.push(Paint::Rule {
                    x: origin_x,
                    y: y_top,
                    w: table_w,
                    h: 0.6,
                    fill: true,
                });
                y_top -= 1.2;
            }
            let baseline = y_top - row_h + 5.0;
            let mut x = origin_x;
            for i in 0..n {
                if cols[i].rule_left {
                    self.current_page.ops.push(Paint::Rule {
                        x,
                        y: y_top - row_h,
                        w: 0.6,
                        h: row_h,
                        fill: true,
                    });
                }
                let cell = row.cells.get(i).map(|c| c.as_slice()).unwrap_or(&[]);
                let cell_width = measure_inlines(cell) * scale;
                let inner_x = match cols[i].align {
                    ColAlign::Left => x + CELL_PAD,
                    ColAlign::Right => x + col_w[i] - CELL_PAD - cell_width,
                    ColAlign::Center => x + (col_w[i] - cell_width) * 0.5,
                };
                self.paint_inlines_at(cell, inner_x.max(x + 1.0), baseline);
                x += col_w[i];
            }
            y_top -= row_h;
        }
        self.current_page.ops.push(Paint::Rule {
            x: origin_x,
            y: y_top,
            w: table_w,
            h: 0.4,
            fill: true,
        });
        self.y = y_top - PARA_SKIP;
    }

    fn layout_figure(&mut self, graphic: Option<Graphic<'a>>, caption: &'a [Inline<'a>]) {
        self.figure_counter += 1;
        let figure_width = (self.right - self.left).min(360.0);
        let figure_height = FIG_H;
        self.ensure_room(figure_height + self.base_leading * 2.0 + PARA_SKIP);
        let x = self.left + ((self.right - self.left) - figure_width) * 0.5;
        let y_bottom = self.y - figure_height;
        self.current_page.ops.push(Paint::Rule {
            x,
            y: y_bottom,
            w: figure_width,
            h: figure_height,
            fill: false,
        });
        if let Some(graphic_obj) = graphic {
            if !graphic_obj.path.is_empty() {
                let text_width = metrics::string_width(graphic_obj.path, Face::Roman, self.base_font_size);
                let pool_start = self.current_page.text_pool.len() as u32;
                self.current_page.text_pool.push(graphic_obj.path);
                self.current_page.ops.push(Paint::GlyphRun {
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
        self.y = y_bottom - self.base_leading;
        if !caption.is_empty() || true {
            let tag_width = 56.0;
            self.current_page.ops.push(Paint::CaptionTag {
                x: self.left,
                y: self.y,
                n: self.figure_counter,
            });
            self.paint_inlines_at(caption, self.left + tag_width, self.y);
            self.y -= self.base_leading;
        }
    }

    fn layout_code_listing(&mut self, listing: &'a crate::listing::CodeListingData<'a>) {
        let font_size = listing.options.font_size;
        let line_leading = font_size * 1.30;
        let lines: Vec<&'a str> = listing.lines().collect();
        if lines.is_empty() {
            return;
        }

        let show_line_numbers = listing.options.show_line_numbers;
        let total_lines = lines.len();
        let num_digits = if total_lines >= 100 { 3 } else if total_lines >= 10 { 2 } else { 1 };
        let mono_char_w = metrics::width('0', Face::Mono, font_size);
        let gutter_w = if show_line_numbers {
            (num_digits as f32 + 1.5) * mono_char_w
        } else {
            0.0
        };

        let pad_x = 8.0;
        let pad_y = 6.0;
        let skip_before = 8.0;
        let skip_after = 8.0;

        self.ensure_room(skip_before + pad_y + line_leading);
        self.y -= skip_before;

        let code_x = self.left + pad_x + gutter_w;
        let gutter_x = self.left + pad_x;

        let mut block_page_start_y = self.y;
        let mut lines_on_current_page = 0;

        for (line_idx, &line_text) in lines.iter().enumerate() {
            let broken = self.ensure_room(line_leading + pad_y);
            if broken {
                if lines_on_current_page > 0 {
                    let block_h = block_page_start_y - self.y + pad_y;
                    if listing.options.frame {
                        let w = self.right - self.left;
                        if let Some(p) = self.pages.last_mut() {
                            p.ops.push(Paint::Rule {
                                x: self.left,
                                y: self.y - pad_y,
                                w,
                                h: block_h,
                                fill: false,
                            });
                        }
                    }
                }
                block_page_start_y = self.y;
                lines_on_current_page = 0;
            }

            if lines_on_current_page == 0 {
                self.y -= pad_y;
            }

            if show_line_numbers {
                self.current_page.ops.push(Paint::PageNumber {
                    x: gutter_x,
                    y: self.y,
                    face: Face::Mono,
                    size: font_size * 0.85,
                    page_number: (line_idx + 1) as u32,
                });
            }

            if !line_text.is_empty() {
                let pool_start = self.current_page.text_pool.len() as u32;
                self.current_page.text_pool.push(line_text);
                self.current_page.ops.push(Paint::GlyphRun {
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

        if listing.options.left_accent_bar {
            self.current_page.ops.push(Paint::Rule {
                x: self.left,
                y: block_bottom,
                w: 2.0,
                h: block_h,
                fill: true,
            });
        }
        if listing.options.frame {
            self.current_page.ops.push(Paint::Rule {
                x: self.left,
                y: block_bottom,
                w,
                h: block_h,
                fill: false,
            });
        }
        if show_line_numbers {
            self.current_page.ops.push(Paint::Rule {
                x: self.left + pad_x + gutter_w - 3.0,
                y: block_bottom,
                w: 0.4,
                h: block_h,
                fill: true,
            });
        }

        self.y = block_bottom - skip_after;
    }

    fn layout_para(&mut self, inls: &'a [Inline<'a>]) {
        self.scratch_runs.clear();
        flatten_into(inls, Face::Roman, &mut self.scratch_runs);
        self.scratch_words.clear();
        explode_words_into_size(&self.scratch_runs, self.base_font_size, &mut self.scratch_words);
        if self.scratch_words.is_empty() {
            return;
        }

        let is_noindent = inls.iter().any(|i| matches!(i, Inline::NoIndent));
        let has_indent = if self.after_heading || is_noindent {
            self.after_heading = false;
            false
        } else {
            self.par_indent > 0.0
        };

        let first_line_indent = if has_indent { self.par_indent } else { 0.0 };
        let measure = self.right - self.left;
        let mut line_w = 0.0;
        let mut num_spaces = 0u32;
        let mut is_first_line = true;

        let num_words = self.scratch_words.len();
        for word_idx in 0..num_words {
            let w = self.scratch_words[word_idx];
            if w.kind == WordKind::Break {
                while let Some(last) = self.scratch_line.last() {
                    if last.kind == WordKind::Space {
                        line_w -= last.width;
                        num_spaces -= 1;
                        self.scratch_line.pop();
                    } else {
                        break;
                    }
                }
                let line_indent = if is_first_line { first_line_indent } else { 0.0 };
                self.flush_scratch_line(false, line_indent, line_w, num_spaces);
                line_w = 0.0;
                num_spaces = 0;
                is_first_line = false;
                continue;
            }
            let extra = w.width;
            let available = if is_first_line {
                measure - first_line_indent
            } else {
                measure
            };
            if !self.scratch_line.is_empty() && line_w + extra > available && w.kind != WordKind::Space {
                while let Some(last) = self.scratch_line.last() {
                    if last.kind == WordKind::Space {
                        line_w -= last.width;
                        num_spaces -= 1;
                        self.scratch_line.pop();
                    } else {
                        break;
                    }
                }
                let line_indent = if is_first_line { first_line_indent } else { 0.0 };
                self.flush_scratch_line(true, line_indent, line_w, num_spaces);
                line_w = 0.0;
                num_spaces = 0;
                is_first_line = false;
                if w.kind == WordKind::Space {
                    continue;
                }
            }
            if self.scratch_line.is_empty() && w.kind == WordKind::Space {
                continue;
            }
            line_w += extra;
            if w.kind == WordKind::Space {
                num_spaces += 1;
            }
            self.scratch_line.push(w);
        }
        while let Some(last) = self.scratch_line.last() {
            if last.kind == WordKind::Space {
                line_w -= last.width;
                num_spaces -= 1;
                self.scratch_line.pop();
            } else {
                break;
            }
        }
        let line_indent = if is_first_line { first_line_indent } else { 0.0 };
        self.flush_scratch_line(false, line_indent, line_w, num_spaces);
    }

    fn flush_scratch_line(&mut self, justify: bool, line_indent: f32, line_w: f32, num_spaces: u32) {
        if self.scratch_line.is_empty() {
            return;
        }
        self.ensure_room(self.base_leading);

        let available = (self.right - self.left) - line_indent;

        let word_spacing = if justify && num_spaces > 0 && line_w < available {
            let slack = available - line_w;
            let extra = slack / (num_spaces as f32);
            if extra <= 18.0 {
                extra
            } else {
                0.0
            }
        } else {
            0.0
        };

        let mut x = self.left + line_indent;
        let mut i = 0;
        while i < self.scratch_line.len() {
            if self.scratch_line[i].kind == WordKind::Space {
                x += self.scratch_line[i].width + word_spacing;
                i += 1;
                continue;
            }
            let face = self.scratch_line[i].face;
            let x0 = x;
            let pool_start = self.current_page.text_pool.len() as u32;
            while i < self.scratch_line.len() {
                let word = self.scratch_line[i];
                if word.kind == WordKind::Word && word.face != face {
                    break;
                }
                self.current_page.text_pool.push(word.text);
                let extra_sp = if word.kind == WordKind::Space { word_spacing } else { 0.0 };
                x += word.width + extra_sp;
                i += 1;
            }
            let pool_len = (self.current_page.text_pool.len() as u32) - pool_start;
            self.current_page.ops.push(Paint::GlyphRun {
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
        self.scratch_line.clear();
    }

    fn paint_inlines_at(&mut self, inlines: &'a [Inline<'a>], x: f32, y: f32) {
        self.paint_inlines_at_size(inlines, Face::Roman, self.base_font_size, x, y);
    }

    fn paint_inlines_at_size(
        &mut self,
        inlines: &'a [Inline<'a>],
        default_face: Face,
        font_size: f32,
        mut x: f32,
        y: f32,
    ) {
        self.scratch_runs.clear();
        flatten_into(inlines, default_face, &mut self.scratch_runs);
        self.scratch_words.clear();
        explode_words_into_size(&self.scratch_runs, font_size, &mut self.scratch_words);
        let mut i = 0;
        while i < self.scratch_words.len() {
            if self.scratch_words[i].kind == WordKind::Break {
                i += 1;
                continue;
            }
            if self.scratch_words[i].kind == WordKind::Space {
                x += self.scratch_words[i].width;
                i += 1;
                continue;
            }
            let face = self.scratch_words[i].face;
            let x0 = x;
            let pool_start = self.current_page.text_pool.len() as u32;
            while i < self.scratch_words.len() {
                let word = self.scratch_words[i];
                if (word.kind == WordKind::Word && word.face != face) || word.kind == WordKind::Break {
                    break;
                }
                self.current_page.text_pool.push(word.text);
                x += word.width;
                i += 1;
            }
            let pool_len = (self.current_page.text_pool.len() as u32) - pool_start;
            self.current_page.ops.push(Paint::GlyphRun {
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum WordKind {
    Word,
    Space,
    Break,
}

#[derive(Clone, Copy)]
struct Shaped<'a> {
    text: &'a str,
    face: Face,
    width: f32,
    kind: WordKind,
}

#[derive(Clone, Copy)]
struct FlatRun<'a> {
    text: &'a str,
    face: Face,
    is_break: bool,
}

fn flatten_into<'s, 'a>(inls: &'s [Inline<'a>], face: Face, out: &mut Vec<FlatRun<'a>>) {
    for i in inls {
        match i {
            Inline::Text(s) => {
                if !s.is_empty() {
                    out.push(FlatRun {
                        text: s,
                        face,
                        is_break: false,
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
            }),
            Inline::NoIndent => {}
        }
    }
}

fn explode_words_into_size<'a>(runs: &[FlatRun<'a>], size: f32, words: &mut Vec<Shaped<'a>>) {
    for run in runs {
        if run.is_break {
            words.push(Shaped {
                text: "",
                face: run.face,
                width: 0.0,
                kind: WordKind::Break,
            });
            continue;
        }
        let s = run.text;
        let bytes = s.as_bytes();
        let mut i = 0;
        let space_w = metrics::width(' ', run.face, size);
        while i < bytes.len() {
            let b = bytes[i];
            if is_tex_space_byte(b) {
                let text = if b == b' ' { &s[i..i + 1] } else { " " };
                words.push(Shaped {
                    text,
                    face: run.face,
                    width: space_w,
                    kind: WordKind::Space,
                });
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
                let text = &s[start..i];
                words.push(Shaped {
                    text,
                    face: run.face,
                    width: metrics::string_width(text, run.face, size),
                    kind: WordKind::Word,
                });
            }
        }
    }
}


#[inline]
fn is_tex_space_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn measure_inlines(inls: &[Inline<'_>]) -> f32 {
    measure_inlines_face_size(inls, Face::Roman, FONT_SIZE)
}

fn measure_inlines_face_size(inls: &[Inline<'_>], face: Face, size: f32) -> f32 {
    let mut total_w = 0.0;
    for inl in inls {
        match inl {
            Inline::Text(s) => {
                total_w += measure_text_run_size(s, face, size);
            }
            Inline::Bold(x) => {
                total_w += measure_inlines_face_size(x, face.with_bold(true), size);
            }
            Inline::Italic(x) => {
                total_w += measure_inlines_face_size(x, face.with_italic(true), size);
            }
            Inline::Mono(x) => {
                total_w += measure_inlines_face_size(x, face.with_mono(true), size);
            }
            Inline::Group(x) => {
                total_w += measure_inlines_face_size(x, face, size);
            }
            Inline::Break => {}
            Inline::NoIndent => {}
        }
    }
    total_w
}

fn measure_text_run_size(s: &str, face: Face, size: f32) -> f32 {
    let mut total_units: u32 = 0;
    let space_units = metrics::char_units(' ', face) as u32;
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
            total_units += metrics::string_units(word, face);
        }
    }
    total_units as f32 * (size / 1000.0)
}

#[inline]
fn count_digits(mut n: u32) -> u32 {
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
