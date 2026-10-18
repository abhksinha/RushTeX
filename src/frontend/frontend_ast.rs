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

/// The LaTeX size declarations, smallest first. The sizes they select depend on the
/// class option (10pt, 11pt, 12pt); see [`FontSize::metrics`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FontSize {
    Tiny = 0,
    ScriptSize,
    FootnoteSize,
    Small,
    NormalSize,
    Large,
    LargeX,
    LargeXX,
    Huge,
    HugeX,
}

impl FontSize {
    pub const ALL: [Self; 10] = [
        Self::Tiny,
        Self::ScriptSize,
        Self::FootnoteSize,
        Self::Small,
        Self::NormalSize,
        Self::Large,
        Self::LargeX,
        Self::LargeXX,
        Self::Huge,
        Self::HugeX,
    ];

    pub fn from_command(name: &str) -> Option<Self> {
        Some(match name {
            "tiny" => Self::Tiny,
            "scriptsize" => Self::ScriptSize,
            "footnotesize" => Self::FootnoteSize,
            "small" => Self::Small,
            "normalsize" => Self::NormalSize,
            "large" => Self::Large,
            "Large" => Self::LargeX,
            "LARGE" => Self::LargeXX,
            "huge" => Self::Huge,
            "Huge" => Self::HugeX,
            _ => return None,
        })
    }

    /// (font size, `\baselineskip`) in pt from `size10.clo`, `size11.clo` and `size12.clo`
    /// for a class whose normal size is `class_pt`; other class sizes scale the 10pt table.
    pub fn metrics(self, class_pt: f32) -> (f32, f32) {
        const SIZE10: [(f32, f32); 10] = [
            (5.0, 6.0), (7.0, 8.0), (8.0, 9.5), (9.0, 11.0), (10.0, 12.0),
            (12.0, 14.0), (14.4, 18.0), (17.28, 22.0), (20.74, 25.0), (24.88, 30.0),
        ];
        const SIZE11: [(f32, f32); 10] = [
            (6.0, 7.0), (8.0, 9.5), (9.0, 11.0), (10.0, 12.0), (10.95, 13.6),
            (12.0, 14.0), (14.4, 18.0), (17.28, 22.0), (20.74, 25.0), (24.88, 30.0),
        ];
        const SIZE12: [(f32, f32); 10] = [
            (6.0, 7.0), (8.0, 9.5), (10.0, 12.0), (10.95, 13.6), (12.0, 14.5),
            (14.4, 18.0), (17.28, 22.0), (20.74, 25.0), (24.88, 30.0), (24.88, 30.0),
        ];
        let k = self as usize;
        if (class_pt - 10.95).abs() < 0.06 {
            SIZE11[k]
        } else if (class_pt - 12.0).abs() < 0.01 {
            SIZE12[k]
        } else {
            let (size, skip) = SIZE10[k];
            let scale = class_pt / 10.0;
            (size * scale, skip * scale)
        }
    }
}

/// Paragraph alignment set by `\centering`, `\raggedright`, `\raggedleft` and the
/// `center`, `flushleft` and `flushright` environments (latex.ltx: `\leftskip` /
/// `\rightskip` of `\@flushglue` = 0pt plus 1fil).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ParAlign {
    #[default]
    Justified = 0,
    /// `\leftskip` and `\rightskip` both fil, `\parfillskip` 0pt.
    Center = 1,
    /// `\rightskip` fil (`\raggedright`, `flushleft`).
    RaggedRight = 2,
    /// `\leftskip` fil, `\parfillskip` 0pt (`\raggedleft`, `flushright`).
    RaggedLeft = 3,
}

/// A `center`, `flushleft` or `flushright` environment: a trivlist whose paragraphs take
/// the alignment.
#[derive(Debug, Clone)]
pub struct AlignedData<'a> {
    pub align: ParAlign,
    /// The environment began in vertical mode (not inside a paragraph), so `\partopsep`
    /// joins `\topsep` above and below it.
    pub in_vmode: bool,
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
    Align(ParAlign),
    FontSize(FontSize),
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
    Aligned(Box<AlignedData<'a>>),
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
    /// An alignment declaration inside a paragraph; the paragraph takes the last one.
    Align(ParAlign),
    /// A size declaration (`\small`, `\Large`, …) until the enclosing group ends.
    FontSize(FontSize),
    /// `\scshape` until the enclosing group ends; `\textsc` is a group that starts with it.
    ScShape,
    /// `\par` inside a paragraph (also inside a group): the paragraph ends here.
    ParEnd,
    /// `\color` declaration: text colour until the enclosing group ends. `\textcolor`
    /// becomes a group that starts with one.
    Color(PackedColor),
}

/// An RGB colour with each channel in 1/1000 (10 bits each), so xcolor's decimal values
/// round-trip exactly. Black is 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct PackedColor(pub u32);

impl PackedColor {
    pub const BLACK: Self = Self(0);

    pub fn from_rgb(r: f32, g: f32, b: f32) -> Self {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 1000.0).round() as u32;
        Self((q(r) << 20) | (q(g) << 10) | q(b))
    }

    /// Channels in 0..=1000.
    #[inline]
    pub const fn channels(self) -> [u32; 3] {
        [(self.0 >> 20) & 0x3ff, (self.0 >> 10) & 0x3ff, self.0 & 0x3ff]
    }

    /// Channels as fractions in 0..=1.
    pub fn rgb(self) -> [f32; 3] {
        self.channels().map(|c| c as f32 * 0.001)
    }

    /// xcolor `a!p!b`: p percent of `self` mixed with `other`.
    pub fn mix(self, pct: f32, other: Self) -> Self {
        let t = (pct * 0.01).clamp(0.0, 1.0);
        let (a, b) = (self.rgb(), other.rgb());
        Self::from_rgb(a[0] * t + b[0] * (1.0 - t), a[1] * t + b[1] * (1.0 - t), a[2] * t + b[2] * (1.0 - t))
    }

    /// The xcolor base colours (and dvipsnames-free extras xcolor always defines).
    pub fn named(name: &str) -> Option<Self> {
        let (r, g, b) = match name {
            "black" => (0.0, 0.0, 0.0),
            "white" => (1.0, 1.0, 1.0),
            "red" => (1.0, 0.0, 0.0),
            "green" => (0.0, 1.0, 0.0),
            "blue" => (0.0, 0.0, 1.0),
            "cyan" => (0.0, 1.0, 1.0),
            "magenta" => (1.0, 0.0, 1.0),
            "yellow" => (1.0, 1.0, 0.0),
            "gray" => (0.5, 0.5, 0.5),
            "darkgray" => (0.25, 0.25, 0.25),
            "lightgray" => (0.75, 0.75, 0.75),
            "brown" => (0.75, 0.5, 0.25),
            "lime" => (0.75, 1.0, 0.0),
            "olive" => (0.5, 0.5, 0.0),
            "orange" => (1.0, 0.5, 0.0),
            "pink" => (1.0, 0.75, 0.75),
            "purple" => (0.75, 0.0, 0.25),
            "teal" => (0.0, 0.5, 0.5),
            "violet" => (0.5, 0.0, 0.5),
            _ => return None,
        };
        Some(Self::from_rgb(r, g, b))
    }

    /// An xcolor expression: `name`, `name!pct` (mixed with white) or `name!pct!name2`,
    /// chained left to right. `user` holds the `\definecolor` names (the last one wins),
    /// looked up before the base colours.
    pub fn resolve(expr: &str, user: &[(&str, PackedColor)]) -> Option<Self> {
        let lookup = |n: &str| {
            let n = n.trim();
            user.iter().rev().find(|(k, _)| *k == n).map(|(_, c)| *c).or_else(|| Self::named(n))
        };
        let mut parts = expr.split('!');
        let mut color = lookup(parts.next()?)?;
        while let Some(pct) = parts.next() {
            let pct = pct.trim().parse::<f32>().ok()?;
            let other = match parts.next() {
                Some(n) => lookup(n)?,
                None => Self::from_rgb(1.0, 1.0, 1.0),
            };
            color = color.mix(pct, other);
        }
        Some(color)
    }

    /// A colour given in an xcolor model: `rgb`, `RGB`, `HTML`, `gray` or `cmyk`.
    pub fn from_model(model: &str, spec: &str) -> Option<Self> {
        let nums = || spec.split(',').map(|v| v.trim().parse::<f32>().ok());
        match model.trim() {
            "rgb" => {
                let v: Option<Vec<f32>> = nums().collect();
                match v?.as_slice() {
                    [r, g, b] => Some(Self::from_rgb(*r, *g, *b)),
                    _ => None,
                }
            }
            "RGB" => {
                let v: Option<Vec<f32>> = nums().collect();
                match v?.as_slice() {
                    [r, g, b] => Some(Self::from_rgb(r / 255.0, g / 255.0, b / 255.0)),
                    _ => None,
                }
            }
            "HTML" => {
                let h = spec.trim();
                if h.len() != 6 {
                    return None;
                }
                let v = u32::from_str_radix(h, 16).ok()?;
                let c = |s: u32| ((v >> s) & 0xff) as f32 / 255.0;
                Some(Self::from_rgb(c(16), c(8), c(0)))
            }
            "gray" => {
                let g = spec.trim().parse::<f32>().ok()?;
                Some(Self::from_rgb(g, g, g))
            }
            "cmyk" => {
                let v: Option<Vec<f32>> = nums().collect();
                match v?.as_slice() {
                    // xcolor: r = 1 - min(1, c + k), and likewise for g and b.
                    [c, m, y, k] => Some(Self::from_rgb(
                        1.0 - (c + k).min(1.0),
                        1.0 - (m + k).min(1.0),
                        1.0 - (y + k).min(1.0),
                    )),
                    _ => None,
                }
            }
            _ => None,
        }
    }
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
