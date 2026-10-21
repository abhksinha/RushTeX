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
    /// Placement bits of `[htbp]` (`crate::tables::model::FLOAT_*`).
    pub placement: u8,
    /// `\centering` inside the float.
    pub centered: bool,
    /// `\vspace` in the float before its caption (a figure drawn by hand later).
    pub vskip: f32,
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
    /// The list starts in vertical mode (after a blank line), so `\partopsep` is added.
    pub in_vmode: bool,
}

impl<'a> ListData<'a> {
    #[inline(always)]
    pub fn ordered(&self) -> bool {
        self.kind == ListKind::Enumerate
    }
}

#[derive(Debug, Clone)]
pub struct QuoteData<'a> {
    pub kind: QuoteKind,
    /// The environment starts in vertical mode (`\partopsep` joins `\topsep`).
    pub in_vmode: bool,
    pub blocks: Vec<Block<'a>>,
}

/// latex.ltx's one-item list environments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteKind {
    Quote,
    Quotation,
    Verse,
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

    /// The display skips this declaration sets in `size1x.clo`: `[\abovedisplayskip` width,
    /// stretch, shrink, `\abovedisplayshortskip` stretch, `\belowdisplayshortskip` width,
    /// stretch, shrink`]`; `\belowdisplayskip` is `\abovedisplayskip`. Only `\normalsize`,
    /// `\small` and `\footnotesize` set them; other sizes keep the ones in force.
    pub fn display_skips(self, class_pt: f32) -> Option<[f32; 7]> {
        const SIZE10: [[f32; 7]; 3] = [
            [6.0, 2.0, 4.0, 1.0, 3.0, 1.0, 2.0],
            [8.5, 3.0, 4.0, 2.0, 4.0, 2.0, 2.0],
            [10.0, 2.0, 5.0, 3.0, 6.0, 3.0, 3.0],
        ];
        const SIZE11: [[f32; 7]; 3] = [
            [8.0, 2.0, 4.0, 1.0, 4.0, 2.0, 2.0],
            [10.0, 2.0, 5.0, 3.0, 6.0, 3.0, 3.0],
            [11.0, 3.0, 6.0, 3.0, 6.5, 3.5, 3.0],
        ];
        const SIZE12: [[f32; 7]; 3] = [
            [10.0, 2.0, 5.0, 3.0, 6.0, 3.0, 3.0],
            [11.0, 3.0, 6.0, 3.0, 6.5, 3.5, 3.0],
            [12.0, 3.0, 7.0, 3.0, 6.5, 3.5, 3.0],
        ];
        let k = match self {
            Self::FootnoteSize => 0,
            Self::Small => 1,
            Self::NormalSize => 2,
            _ => return None,
        };
        Some(if (class_pt - 10.95).abs() < 0.06 {
            SIZE11[k]
        } else if (class_pt - 12.0).abs() < 0.01 {
            SIZE12[k]
        } else {
            SIZE10[k].map(|v| v * class_pt / 10.0)
        })
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
    pub kind: TrivlistKind,
    pub blocks: Vec<Block<'a>>,
}

/// Which trivlist an [`AlignedData`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrivlistKind {
    /// `center`, `flushleft`, `flushright`.
    Environment,
    /// An amsthm theorem (`\@thm`): `\thm@preskip` and `\thm@postskip` are `\topsep`
    /// (half of it in the `remark` style) without `\partopsep`, after the list's
    /// `\@beginparpenalty` and `\@endparpenalty`; paragraphs keep `\parindent`.
    /// `italic`: the `plain` style's `\itshape` body.
    Theorem { half_skip: bool, italic: bool },
    /// amsthm `proof`: `\par` then a `\trivlist` with `\topsep` 6pt plus 6pt, so always
    /// `\partopsep` too.
    Proof,
}

/// What a user `\@makecol` puts beside the column in `\@outputbox`: `(is_rule, width pt)`
/// items left and right of it, in order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ColumnWrap {
    pub left: Vec<(bool, f32)>,
    pub right: Vec<(bool, f32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigChange {
    /// `\appendix`: sections restart at A (`\thesection` is `\@Alph\c@section`).
    Appendix,
    /// The expander's reading of a user `\@makecol`, for the pages shipped after it.
    ColumnWrap(Box<ColumnWrap>),
    /// `\sfcode`\.\@m` on (`thebibliography`) or back to the default.
    PeriodSf1000(bool),
    ParIndent(f32),
    /// `\parskip`: (width, stretch, shrink) in pt.
    ParSkip(f32, f32, f32),
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
    /// `\pagestyle` in the document body.
    PageStyle(PageStyle),
    /// `\thispagestyle`: the style of the page being built only.
    ThisPageStyle(PageStyle),
}

/// A page style (latex.ltx `\ps@empty`, `\ps@plain`; article.cls `\ps@headings`,
/// `\ps@myheadings`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageStyle {
    Empty,
    #[default]
    Plain,
    Headings,
    MyHeadings,
}

impl PageStyle {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "empty" => PageStyle::Empty,
            "plain" => PageStyle::Plain,
            "headings" => PageStyle::Headings,
            "myheadings" => PageStyle::MyHeadings,
            _ => return None,
        })
    }
}

/// The text of a mark: `\markboth{left}{right}` or `\markright{right}` (`left` is `None`).
#[derive(Debug, Clone, PartialEq)]
pub struct MarkData<'a> {
    pub left: Option<Vec<Inline<'a>>>,
    pub right: Vec<Inline<'a>>,
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
        /// Numbered: not starred and `level <= \c@secnumdepth` (`\@startsection`); an
        /// unnumbered unstarred heading still goes in the contents.
        numbered: bool,
        title: Vec<Inline<'a>>,
    },
    PageBreak,
    /// Vertical glue: natural width, stretch and shrink in pt.
    VSpace { w: f32, stretch: f32, shrink: f32 },
    /// A run-in heading's before-skip (`\paragraph`): `\@startsection`'s
    /// `\addpenalty\@secpenalty\addvspace{w plus stretch minus shrink}`, neither of which
    /// it adds right after another heading (`\if@nobreak`).
    RunInHeadSkip { w: f32, stretch: f32, shrink: f32 },
    /// A mark or whatsit in vertical mode (`\markboth`): no height, but no discardable
    /// item either, so `\lastskip` is zero after it and glue after it is a breakpoint.
    /// Carries the mark text of `\markboth`/`\markright`; `None` for other whatsits.
    VMark(Option<Box<MarkData<'a>>>),
    /// `\label`'s `\write` whatsit in vertical mode: like `VMark`, and the page it lands
    /// on is label `N`'s page (`\pageref`).
    PageLabel(u16),
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
        /// Whether the display continues a paragraph or starts one.
        par: DisplayPar,
        math: Box<crate::math::MathNode<'a>>,
    },
}

/// How a display relates to the paragraph around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DisplayPar {
    /// Inside a paragraph (horizontal mode at `$$`).
    Continues,
    /// `$$` in vertical mode starts a paragraph: `\parskip`, then a line holding only the
    /// indentation box (tex.web §1145).
    Starts,
    /// latex.ltx's `\[` (and `displaymath`) in vertical mode: `\nointerlineskip` and
    /// `\makebox[.6\linewidth]{}` before the `$$`.
    StartsWithBox,
    /// `$$` right after a list environment with no blank line: `\@doendpe` removes the
    /// indentation box, so the paragraph is empty and TeX pops it (§1145): `\parskip`, no line.
    StartsEmpty,
}

#[derive(Debug, Clone, Copy)]
pub struct Graphic<'a> {
    pub path: &'a str,
    /// The raw `key=value` options of `\includegraphics[...]`.
    pub options: &'a str,
}

/// Flag in [`Inline::HFil`]'s order: the glue follows `\\nobreak`.
pub const HFIL_NOBREAK: u8 = 0x80;

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
    /// `\hspace` with stretch or shrink: (width, stretch, shrink) in pt.
    HGlue(f32, f32, f32),
    /// Infinite glue of zero width: stretch in units of its order, and the order (1 fil,
    /// 2 fill), as `\hfil`, `\hfill`, `\hfilneg` and `\hspace{\fill}`. [`HFIL_NOBREAK`] in
    /// the order marks `\nobreak\hfil`: not a breakpoint.
    HFil(f32, u8),
    /// `\unskip`: removes the glue just before it.
    Unskip,
    /// `\vspace` in a paragraph: latex.ltx `\@vspace` is `\vadjust{\vskip #1\vskip\z@skip}`,
    /// the skip going below the line that holds it: pt, plus `em` and `ex` of the font
    /// current there (TeX evaluates them when the skip is scanned).
    VAdjust { pt: f32, em: f32, ex: f32 },
    /// In a heading's title: the optional short title (`\section[short]{..}`) ends here,
    /// and the title proper follows.
    ShortTitleEnd,
    /// `\/` (§1113): a kern of the italic correction of the character just before, if any.
    ItalicCorrection,
    /// `\label`'s `\write` whatsit in a paragraph: label `N` is on this line's page.
    PageLabel(u16),
    /// Ends a paragraph that a display interrupts: TeX breaks its lines with
    /// `\displaywidowpenalty` instead of `\widowpenalty` (tex.web §890, §1145).
    DisplayFollows,
    /// Starts the text after a display in the same paragraph: TeX resumes horizontal mode
    /// (§1200), so no `\parskip` and no indentation.
    DisplayResumes,
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
    /// A font declaration (`\bfseries`, `\itshape`, `\em`, …) until the enclosing group
    /// ends; `\textrm`, `\textup` and the like are groups that start with one.
    FontDecl(FontDecl),
    /// Starts the group of a text font command (`\textbf`, `\emph`, ...: latex.ltx
    /// `\DeclareTextFontCommand`), which adds `\check@icl` and `\check@icr`'s italic
    /// corrections; a `{\bfseries ...}` group has none.
    TextFontCommand,
    /// Horizontal space of `em` ems of the current font, the factor in units of 2^-16 as
    /// `scan_dimen` reads `<decimal>em` (§452 `round_decimals`): `\quad` and `\hskip`-like
    /// spaces are breakable glue, `\,` and `\enspace` are kerns.
    EmSpace { em: i32, breakable: bool },
    /// `\kern\fontdimen<n>\font`: a kern of a parameter of the current font
    /// (`\textellipsis`).
    FontDimenKern(u8),
    /// `\kern<dimen>`, in scaled points.
    Kern(i32),
    /// `\spacefactor` assignment; `\@` sets 1000.
    SpaceFactor(u16),
    /// `\footnote`: its mark in the text and its insertion at the foot of the page.
    Footnote(Box<FootnoteData<'a>>),
    /// `\TeX`, `\LaTeX` (latex.ltx, in the current text font) or `\AmS` (amsmath, in cmsy).
    Logo(LogoKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogoKind {
    TeX,
    LaTeX,
    AmS,
    /// latex.ltx `\LaTeXe`: `\LaTeX\kern.15em2$_{\textstyle\varepsilon}$`.
    LaTeXe,
    /// `\textcopyright` (`\copyright`) outside Computer Modern: ltoutenc.dtx's OMS
    /// `\textcircled{c}`.
    Copyright,
    /// A text symbol LaTeX takes from TS1 (`\textcopyright`, `\textbullet`, `\textdagger`,
    /// ...), by its slot: `backend_ts1::TS1_TEXT_SYMBOLS`.
    Ts1(u8),
}

/// A footnote (latex.ltx `\@footnotemark`, `\@footnotetext`).
#[derive(Debug, Clone, PartialEq)]
pub struct FootnoteData<'a> {
    /// `\@makefnmark`: the number as a superscript (`\@textsuperscript`).
    pub mark: crate::math::MathNode<'a>,
    pub text: Vec<Inline<'a>>,
}

/// The NFSS declarations that change one axis of the current font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontDecl {
    /// `\bfseries`
    Bold,
    /// `\mdseries`
    Medium,
    /// `\itshape`
    Italic,
    /// `\slshape`
    Slanted,
    /// `\upshape`
    Upright,
    /// `\ttfamily`
    Mono,
    /// `\rmfamily`
    Roman,
    /// `\sffamily`
    Sans,
    /// `\normalfont`
    Normal,
    /// `\em`: italic in upright text, upright in italic text.
    Em,
    /// latex.ltx's `\DeclareOldFontCommand`s (`\rm`, `\bf`, `\it`, ...):
    /// `\normalfont` then the one declaration.
    Old(OldFont),
}

/// The LaTeX 2.09 font commands, each `\normalfont` followed by one attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OldFont {
    Rm,
    Bf,
    It,
    Sl,
    Tt,
    Sf,
    Sc,
}

impl FontDecl {
    /// The face after the declaration, from the face before it.
    pub fn apply(self, face: crate::backend::fonts::Face, family: crate::backend::fonts::FontFamily) -> crate::backend::fonts::Face {
        use crate::backend::fonts::{Face, FontFamily};
        // The shape axis: small caps and slanted are shapes of the roman medium font, so a new
        // shape starts from it. Under `\em`, slanted counts as italic (its slant is nonzero).
        let face = match (face, self) {
            (Face::SmallCaps, FontDecl::Upright | FontDecl::Italic | FontDecl::Em | FontDecl::Slanted) => Face::Roman,
            (Face::Slanted, FontDecl::Em | FontDecl::Normal) => return Face::Roman,
            (Face::Slanted, FontDecl::Upright | FontDecl::Italic | FontDecl::Slanted) => Face::Roman,
            // cmbxsl10 and cmsltt10 are not bundled: bold or typewriter wins over the slant.
            (Face::Slanted, _) => {
                let f = Face::Roman;
                return match self.apply(f, family) {
                    Face::Roman => Face::Slanted,
                    other => other,
                };
            }
            _ => face,
        };
        match self {
            // The Base-14 families have no slanted face; their oblique/italic stands in.
            FontDecl::Slanted if family == FontFamily::ComputerModern && face == Face::Roman => Face::Slanted,
            FontDecl::Slanted => face.with_italic(true),
            FontDecl::Bold => face.with_bold(true),
            FontDecl::Medium => face.with_bold(false),
            FontDecl::Italic => face.with_italic(true),
            FontDecl::Upright => face.with_italic(false),
            FontDecl::Mono => face.with_mono(true),
            FontDecl::Roman => match face {
                Face::MathSans => Face::Roman,
                Face::SansBold => Face::Bold,
                _ => face.with_mono(false),
            },
            // cmss10 is the only sans face bundled: upright medium sans in Computer Modern.
            FontDecl::Sans if family == FontFamily::ComputerModern && face == Face::Roman => Face::MathSans,
            FontDecl::Sans if family == FontFamily::ComputerModern && face == Face::Bold => Face::SansBold,
            FontDecl::Sans => face.with_mono(false),
            FontDecl::Normal => Face::Roman,
            FontDecl::Em => face.with_italic(!face.is_italic()),
            FontDecl::Old(old) => {
                let decl = match old {
                    OldFont::Rm => return Face::Roman,
                    OldFont::Sc => return if family == FontFamily::ComputerModern { Face::SmallCaps } else { Face::Roman },
                    OldFont::Bf => FontDecl::Bold,
                    OldFont::It => FontDecl::Italic,
                    OldFont::Sl => FontDecl::Slanted,
                    OldFont::Tt => FontDecl::Mono,
                    OldFont::Sf => FontDecl::Sans,
                };
                decl.apply(Face::Roman, family)
            }
        }
    }
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

/// A heading's title split into the text for the contents (and running heads) and the
/// heading itself: `\section[short]{long}` gives (short, long), `\section{t}` (t, t).
pub fn section_title_parts<'s, 'a>(title: &'s [Inline<'a>]) -> (&'s [Inline<'a>], &'s [Inline<'a>]) {
    match title.iter().position(|i| matches!(i, Inline::ShortTitleEnd)) {
        Some(k) => (&title[..k], &title[k + 1..]),
        None => (title, title),
    }
}
