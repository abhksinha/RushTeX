//! # Abstract Syntax Tree for TeX Documents (`ast`)

use crate::config::Preamble;
use crate::diagnostic::Diagnostic;

pub type ParseError = Diagnostic;

#[derive(Debug, Clone)]
pub struct Document<'a> {
    pub preamble: Preamble<'a>,
    pub blocks: Vec<Block<'a>>,
}

pub use crate::tables::{ColAlign, ColSpec, TableCell, TableData, TableRow, TableRule};

#[derive(Debug, Clone)]
pub struct FigureData<'a> {
    pub graphic: Option<Graphic<'a>>,
    pub caption: Vec<Inline<'a>>,
}

#[derive(Debug, Clone)]
pub struct TitleBlockData<'a> {
    pub title: Vec<Inline<'a>>,
    pub author: Option<Vec<Inline<'a>>>,
    pub date: Option<Vec<Inline<'a>>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListOptions {
    pub left_margin: f32,
    pub label_sep: f32,
    pub item_indent: f32,
    pub label_width: f32,
    pub item_sep: f32,
    pub parsep: f32,
    pub hang_indent: f32,
    pub present_mask: u8,
    pub start: u16,
    pub label_format: u8,
}

impl Default for ListOptions {
    fn default() -> Self {
        Self {
            left_margin: 0.0,
            label_sep: 0.0,
            item_indent: 0.0,
            label_width: 0.0,
            item_sep: 0.0,
            parsep: 0.0,
            hang_indent: 0.0,
            present_mask: 0,
            start: 1,
            label_format: 0,
        }
    }
}

impl ListOptions {
    pub const MASK_LEFT_MARGIN: u8 = 1 << 0;
    pub const MASK_LABEL_SEP: u8 = 1 << 1;
    pub const MASK_ITEM_INDENT: u8 = 1 << 2;
    pub const MASK_LABEL_WIDTH: u8 = 1 << 3;
    pub const MASK_ITEM_SEP: u8 = 1 << 4;
    pub const MASK_PARSEP: u8 = 1 << 5;
    pub const MASK_HANG_INDENT: u8 = 1 << 6;

    #[inline(always)]
    pub fn left_margin(&self) -> Option<f32> {
        if self.present_mask & Self::MASK_LEFT_MARGIN != 0 {
            Some(self.left_margin)
        } else {
            None
        }
    }
    #[inline(always)]
    pub fn set_left_margin(&mut self, val: f32) {
        self.left_margin = val;
        self.present_mask |= Self::MASK_LEFT_MARGIN;
    }

    #[inline(always)]
    pub fn label_sep(&self) -> Option<f32> {
        if self.present_mask & Self::MASK_LABEL_SEP != 0 {
            Some(self.label_sep)
        } else {
            None
        }
    }
    #[inline(always)]
    pub fn set_label_sep(&mut self, val: f32) {
        self.label_sep = val;
        self.present_mask |= Self::MASK_LABEL_SEP;
    }

    #[inline(always)]
    pub fn item_indent(&self) -> Option<f32> {
        if self.present_mask & Self::MASK_ITEM_INDENT != 0 {
            Some(self.item_indent)
        } else {
            None
        }
    }
    #[inline(always)]
    pub fn set_item_indent(&mut self, val: f32) {
        self.item_indent = val;
        self.present_mask |= Self::MASK_ITEM_INDENT;
    }

    #[inline(always)]
    pub fn label_width(&self) -> Option<f32> {
        if self.present_mask & Self::MASK_LABEL_WIDTH != 0 {
            Some(self.label_width)
        } else {
            None
        }
    }
    #[inline(always)]
    pub fn set_label_width(&mut self, val: f32) {
        self.label_width = val;
        self.present_mask |= Self::MASK_LABEL_WIDTH;
    }

    #[inline(always)]
    pub fn item_sep(&self) -> Option<f32> {
        if self.present_mask & Self::MASK_ITEM_SEP != 0 {
            Some(self.item_sep)
        } else {
            None
        }
    }
    #[inline(always)]
    pub fn set_item_sep(&mut self, val: f32) {
        self.item_sep = val;
        self.present_mask |= Self::MASK_ITEM_SEP;
    }

    #[inline(always)]
    pub fn parsep(&self) -> Option<f32> {
        if self.present_mask & Self::MASK_PARSEP != 0 {
            Some(self.parsep)
        } else {
            None
        }
    }
    #[inline(always)]
    pub fn set_parsep(&mut self, val: f32) {
        self.parsep = val;
        self.present_mask |= Self::MASK_PARSEP;
    }

    #[inline(always)]
    pub fn hang_indent(&self) -> Option<f32> {
        if self.present_mask & Self::MASK_HANG_INDENT != 0 {
            Some(self.hang_indent)
        } else {
            None
        }
    }
    #[inline(always)]
    pub fn set_hang_indent(&mut self, val: f32) {
        self.hang_indent = val;
        self.present_mask |= Self::MASK_HANG_INDENT;
    }

    #[inline(always)]
    pub fn left_margin_or(&self, default: f32) -> f32 {
        if self.present_mask & Self::MASK_LEFT_MARGIN != 0 {
            self.left_margin
        } else {
            default
        }
    }
    #[inline(always)]
    pub fn label_sep_or(&self, default: f32) -> f32 {
        if self.present_mask & Self::MASK_LABEL_SEP != 0 {
            self.label_sep
        } else {
            default
        }
    }
    #[inline(always)]
    pub fn item_indent_or(&self, default: f32) -> f32 {
        if self.present_mask & Self::MASK_ITEM_INDENT != 0 {
            self.item_indent
        } else {
            default
        }
    }
    #[inline(always)]
    pub fn item_sep_or(&self, default: f32) -> f32 {
        if self.present_mask & Self::MASK_ITEM_SEP != 0 {
            self.item_sep
        } else {
            default
        }
    }
    #[inline(always)]
    pub fn label_width_or(&self, default: f32) -> f32 {
        if self.present_mask & Self::MASK_LABEL_WIDTH != 0 {
            self.label_width
        } else {
            default
        }
    }
    #[inline(always)]
    pub fn parsep_or(&self, default: f32) -> f32 {
        if self.present_mask & Self::MASK_PARSEP != 0 {
            self.parsep
        } else {
            default
        }
    }
    #[inline(always)]
    pub fn hang_indent_or(&self, default: f32) -> f32 {
        if self.present_mask & Self::MASK_HANG_INDENT != 0 {
            self.hang_indent
        } else {
            default
        }
    }

    #[inline(always)]
    pub fn start_or(&self, default: u16) -> u16 {
        if self.start > 0 {
            self.start
        } else {
            default
        }
    }

    #[inline(always)]
    pub fn set_start(&mut self, val: u16) {
        self.start = val;
    }

    #[inline(always)]
    pub fn set_label_format(&mut self, val: u8) {
        self.label_format = val;
    }
}

#[derive(Debug, Clone)]
pub struct ListItem<'a> {
    pub custom_label: Option<Vec<Inline<'a>>>,
    pub blocks: Vec<Block<'a>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Enumerate,
    Itemize,
    Description,
}

#[derive(Debug, Clone)]
pub struct ListData<'a> {
    pub kind: ListKind,
    pub options: ListOptions,
    pub items: Vec<ListItem<'a>>,
}

impl<'a> ListData<'a> {
    #[inline(always)]
    pub fn ordered(&self) -> bool {
        self.kind == ListKind::Enumerate
    }
}

#[derive(Debug, Clone)]
pub struct QuoteData<'a> {
    pub is_quotation: bool,
    pub blocks: Vec<Block<'a>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigChange {
    ParIndent(f32),
    ParSkip(f32),
    HangIndent(f32),
    HangAfter(i32),
    LeftSkip(f32),
    RightSkip(f32),
    LabelSep(f32),
    LeftMargin(f32),
    ItemIndent(f32),
    ItemSep(f32),
    ListParIndent(f32),
    ParShape(Box<[(f32, f32)]>),
    PreTolerance(i32),
    Tolerance(i32),
    EmergencyStretch(f32),
    Looseness(i32),
    ParFillSkip { width: f32, stretch: f32 },
    PrevGraf(i32),
    LastLineFit(i32),
    Sloppy,
    Fussy,
}

#[derive(Debug, Clone)]
pub enum Block<'a> {
    Para(Vec<Inline<'a>>),
    List(Box<ListData<'a>>),
    Table(Box<TableData<'a>>),
    Figure(Box<FigureData<'a>>),
    TitleBlock(Box<TitleBlockData<'a>>),
    Section {
        level: u8,
        starred: bool,
        title: Vec<Inline<'a>>,
    },
    PageBreak,
    VSpace(f32),
    HRule,
    CodeListing(Box<crate::code_listing::CodeListingData<'a>>),
    ConfigChange(ConfigChange),
    Quote(Box<QuoteData<'a>>),
    TableOfContents,
    ListOfFigures,
    ListOfTables,
    Equation {
        starred: bool,
        math: Box<crate::math::MathNode<'a>>,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct Graphic<'a> {
    pub path: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inline<'a> {
    Text(&'a str),
    Bold(Vec<Inline<'a>>),
    Italic(Vec<Inline<'a>>),
    Mono(Vec<Inline<'a>>),
    Group(Vec<Inline<'a>>),
    Break,
    NoIndent,
    Indent,
    HangIndent(f32),
    HangAfter(i32),
    ParShape(Box<[(f32, f32)]>),
    PreTolerance(i32),
    Tolerance(i32),
    EmergencyStretch(f32),
    Looseness(i32),
    LeftSkip(f32),
    RightSkip(f32),
    ParFillSkip { width: f32, stretch: f32 },
    PrevGraf(i32),
    LastLineFit(i32),
    Sloppy,
    Fussy,
    HSpace(f32),
    Math(Box<crate::math::MathNode<'a>>),
}

pub fn parse_list_options(input: &str) -> ListOptions {
    let mut options = ListOptions::default();
    let content = input.trim().trim_start_matches('[').trim_end_matches(']');
    for part in content.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut kv = part.splitn(2, '=');
        let key = kv.next().unwrap_or("").trim().to_ascii_lowercase();
        let val = kv.next().unwrap_or("").trim();
        match key.as_str() {
            "labelsep" | "label_sep" => {
                if let Some(dim) = crate::doc_config::parse_dimension_pt(val) {
                    options.set_label_sep(dim);
                }
            }
            "leftmargin" | "left_margin" => {
                if let Some(dim) = crate::doc_config::parse_dimension_pt(val) {
                    options.set_left_margin(dim);
                }
            }
            "itemindent" | "item_indent" => {
                if let Some(dim) = crate::doc_config::parse_dimension_pt(val) {
                    options.set_item_indent(dim);
                }
            }
            "labelwidth" | "label_width" => {
                if let Some(dim) = crate::doc_config::parse_dimension_pt(val) {
                    options.set_label_width(dim);
                }
            }
            "itemsep" | "item_sep" => {
                if let Some(dim) = crate::doc_config::parse_dimension_pt(val) {
                    options.set_item_sep(dim);
                }
            }
            "parsep" => {
                if let Some(dim) = crate::doc_config::parse_dimension_pt(val) {
                    options.set_parsep(dim);
                }
            }
            "hangindent" | "hang_indent" | "hanging" => {
                if let Some(dim) = crate::doc_config::parse_dimension_pt(val) {
                    options.set_hang_indent(dim);
                }
            }
            "start" => {
                if let Ok(num) = val.parse::<u16>() {
                    options.set_start(num);
                }
            }
            "label" => {
                let fmt = if val.contains("(\\alph*)") || val.contains("(a)") {
                    2 // Alpha in parens: (a), (b)
                } else if val.contains("\\alph*.") || val.contains("a.") {
                    3 // Alpha with dot: a., b.
                } else if val.contains("\\Alph*.") || val.contains("A.") {
                    4 // Upper alpha with dot: A., B.
                } else if val.contains("(\\arabic*)") || val.contains("(1)") {
                    5 // Number in parens: (1), (2)
                } else if val.contains("\\arabic*.") || val.contains("1.") {
                    1 // Arabic with dot: 1., 2.
                } else if val.contains("\\roman*.") || val.contains("i.") {
                    6 // Roman lower with dot: i., ii.
                } else if val.contains("\\Roman*.") || val.contains("I.") {
                    7 // Roman upper with dot: I., II.
                } else if val.contains("--") {
                    8 // Dash
                } else if val.contains('*') {
                    9 // Asterisk
                } else {
                    0
                };
                options.set_label_format(fmt);
            }
            _ => {}
        }
    }
    options
}
