//! # Document Preamble and Package Architecture (`preamble`)
//!
//! Provides modular, extensible processing for LaTeX document preambles, configuration directives,
//! and package handlers.
//!
//! ## Architectural Overview
//!
//! In standard LaTeX, packages like `geometry` manipulate page dimensions and margins.
//! This module decouples LaTeX syntax from internal engine state via an open plugin-style trait:
//! - **Document Classes ([`DocClass`])**: Evaluates `\documentclass[options]{name}` to configure
//!   standard page sizes (`a4paper`, `letterpaper`, `landscape`) and base typography (`10pt`, `11pt`, `12pt`).
//! - **Package Handler Trait ([`PackageHandler`])**: Allows contributors to register handlers for
//!   standard packages without touching core engine internals.
//! - **Package Registry ([`PackageRegistry`])**: Dispatches options to registered package handlers.
//! - **Document Configuration ([`DocumentConfig`])**: Compact struct tracking page width, height,
//!   margins, base font size, and line height (leading).
//!
//! ## Public APIs
//!
//! - [`DocClass`]: Extracted document class directive.
//! - [`Package`]: Extracted package directive with options.
//! - [`DocumentConfig`]: Resolved layout and page geometry values.
//! - [`Preamble`]: Complete parsed preamble state with document metadata.
//! - [`PackageHandler`]: Trait for package extensions (`name()`, `apply()`).
//! - [`PackageRegistry`]: Extensible registry of active package handlers.
//! - [`apply_doc_class`]: Updates `DocumentConfig` according to class options.
//! - [`parse_dimension_pt`]: Converts TeX dimensions (`in`, `cm`, `mm`, `pt`) to PDF points.

pub use crate::font_metrics::FontFamily;
use crate::tex_parser::Inline;

/// Extracted document class directive, e.g. `\documentclass[11pt,a4paper]{article}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocClass<'a> {
    pub name: &'a str,
    pub options: Vec<&'a str>,
}

/// Extracted package declaration, e.g. `\usepackage[margin=1in]{geometry}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package<'a> {
    pub name: &'a str,
    pub options: Vec<&'a str>,
}

/// Layout and geometry configuration derived from class and packages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentConfig {
    pub page_width: f32,
    pub page_height: f32,
    pub margin_left: f32,
    pub margin_right: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
    /// TeX `\topskip`: minimum distance from the top of the text block to the
    /// first baseline on every page (size1x.clo: 10pt / 11pt / 12pt).
    pub top_skip: f32,
    /// LaTeX `\footskip`: distance from the bottom of the text block to the
    /// baseline of the running foot (page number).
    pub foot_skip: f32,
    pub base_font_size: f32,
    pub base_leading: f32,
    pub font_family: FontFamily,
    pub page_numbering: bool,
    pub page_style: crate::frontend::ast::PageStyle,
    /// LaTeX `\headsep`: running-head baseline to the top of the text block.
    pub head_sep: f32,
    pub par_indent: f32,
    pub par_skip: f32,
    /// `\c@secnumdepth` and `\c@tocdepth` (article: 3 and 3).
    pub sec_num_depth: i8,
    pub toc_depth: i8,
    /// `\parskip`'s stretch and shrink in pt (article: `0pt plus 1pt`).
    pub par_skip_stretch: f32,
    pub par_skip_shrink: f32,
    pub hang_indent: f32,
    pub hang_after: i32,
    pub left_skip: f32,
    pub right_skip: f32,
    pub list_left_margin: f32,
    pub list_label_sep: f32,
    pub list_item_indent: f32,
    pub list_item_sep: f32,
    pub list_par_indent: f32,
    pub french_spacing: bool,
    pub pre_tolerance: i32,
    pub tolerance: i32,
    pub emergency_stretch: f32,
    pub looseness: i32,
    pub par_fill_width: f32,
    pub par_fill_stretch: f32,
    pub prev_graf: i32,
    pub last_line_fit: i32,
}

impl Default for DocumentConfig {
    fn default() -> Self {
        Self {
            // A4 (210mm x 297mm) in TeX points; every layout dimension is in TeX points and
            // the backend scales to PDF big points (72 per inch) once per page.
            page_width: A4_PT.0,
            page_height: A4_PT.1,
            margin_left: TEX_INCH,
            margin_right: TEX_INCH,
            margin_top: TEX_INCH,
            margin_bottom: TEX_INCH,
            top_skip: 10.0,
            foot_skip: 30.0,
            // LaTeX's standard classes default to 10pt.
            base_font_size: 10.0,
            base_leading: 12.0,
            // LaTeX's default text font is Computer Modern (`\rmdefault` = cmr).
            font_family: FontFamily::ComputerModern,
            page_numbering: true,
            page_style: crate::frontend::ast::PageStyle::Plain,
            head_sep: 25.0,
            par_indent: 15.0,
            // LaTeX kernel: \parskip = 0pt plus 1pt (natural width 0).
            par_skip: 0.0,
            sec_num_depth: 3,
            toc_depth: 3,
            par_skip_stretch: 1.0,
            par_skip_shrink: 0.0,
            hang_indent: 0.0,
            hang_after: 1,
            left_skip: 0.0,
            right_skip: 0.0,
            // NaN: the class values (`\leftmargini` 2.5em, `\labelsep` .5em, `\itemsep` of
            // the list level) until the document sets them.
            list_left_margin: f32::NAN,
            list_label_sep: f32::NAN,
            list_item_indent: 0.0,
            list_item_sep: f32::NAN,
            list_par_indent: 0.0,
            french_spacing: false,
            pre_tolerance: 100,
            tolerance: 200,
            emergency_stretch: 0.0,
            looseness: 0,
            par_fill_width: 0.0,
            par_fill_stretch: f32::INFINITY,
            prev_graf: 0,
            last_line_fit: 0,
        }
    }
}

/// Parsed document preamble containing document class, packages, and metadata.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Preamble<'a> {
    pub class: Option<DocClass<'a>>,
    pub packages: Vec<Package<'a>>,
    pub title: Option<Vec<Inline<'a>>>,
    pub author: Option<Vec<Inline<'a>>>,
    pub date: Option<Vec<Inline<'a>>>,
    pub config: DocumentConfig,
}

/// Trait for package handlers.
///
/// Contributors can implement this trait to add support for standard LaTeX packages
/// without modifying parser internals.
pub trait PackageHandler: Sync + Send {
    /// Canonical name of the package, e.g. "geometry".
    fn name(&self) -> &'static str;

    /// Apply package options to document configuration.
    fn apply(&self, package_options: &[&str], document_config: &mut DocumentConfig);
}

/// Built-in handler for `\usepackage{geometry}`.
pub struct GeometryPackage;

impl PackageHandler for GeometryPackage {
    fn name(&self) -> &'static str {
        "geometry"
    }

    fn apply(&self, package_options: &[&str], document_config: &mut DocumentConfig) {
        for &option_str in package_options {
            let option_str = option_str.trim();
            if let Some((option_key, option_value)) = option_str.split_once('=') {
                let option_key = option_key.trim();
                let option_value = option_value.trim();
                if let Some(points) = parse_dimension_pt(option_value) {
                    match option_key {
                        "margin" => {
                            document_config.margin_left = points;
                            document_config.margin_right = points;
                            document_config.margin_top = points;
                            document_config.margin_bottom = points;
                        }
                        "left" | "lmargin" => document_config.margin_left = points,
                        "right" | "rmargin" => document_config.margin_right = points,
                        "top" | "tmargin" => document_config.margin_top = points,
                        "bottom" | "bmargin" => document_config.margin_bottom = points,
                        _ => {}
                    }
                }
            } else {
                match option_str {
                    "a4paper" => {
                        (document_config.page_width, document_config.page_height) = A4_PT;
                    }
                    "letterpaper" => {
                        (document_config.page_width, document_config.page_height) = LETTER_PT;
                    }
                    "landscape"
                        if document_config.page_width < document_config.page_height => {
                            std::mem::swap(
                                &mut document_config.page_width,
                                &mut document_config.page_height,
                            );
                        }
                    _ => {}
                }
            }
        }
    }
}

/// Handler for `\usepackage{parskip}` which removes first-line indentation and configures paragraph skip.
pub struct ParskipPackage;

impl PackageHandler for ParskipPackage {
    fn name(&self) -> &'static str {
        "parskip"
    }

    fn apply(&self, _options: &[&str], config: &mut DocumentConfig) {
        // parskip.sty: \parskip = .5\baselineskip plus 2pt, \parindent = 0pt.
        config.par_indent = 0.0;
        config.par_skip = 0.5 * config.base_leading;
        config.par_skip_stretch = 2.0;
        config.par_skip_shrink = 0.0;
    }
}

/// No-op compatibility package handler for common LaTeX packages that require
/// no configuration changes (e.g. `graphicx`, `amsmath`, `inputenc`).
pub struct NoOpPackage(&'static str);

impl PackageHandler for NoOpPackage {
    fn name(&self) -> &'static str {
        self.0
    }

    fn apply(&self, _options: &[&str], _config: &mut DocumentConfig) {}
}

/// Handler for typeface packages (e.g. `times`, `helvet`, `helvetica`).
pub struct FontPackage(&'static str, FontFamily);

impl PackageHandler for FontPackage {
    fn name(&self) -> &'static str {
        self.0
    }

    fn apply(&self, _options: &[&str], config: &mut DocumentConfig) {
        config.font_family = self.1;
    }
}

/// Extensible registry of package handlers.
pub struct PackageRegistry {
    handlers: Vec<Box<dyn PackageHandler>>,
}

impl Default for PackageRegistry {
    fn default() -> Self {
        let mut reg = Self::new();
        reg.register(GeometryPackage);
        reg.register(ParskipPackage);
        reg.register(FontPackage("times", FontFamily::Times));
        reg.register(FontPackage("mathptmx", FontFamily::Times));
        reg.register(FontPackage("newtxtext", FontFamily::Times));
        reg.register(FontPackage("tgtermes", FontFamily::Times));
        reg.register(FontPackage("helvet", FontFamily::Helvetica));
        reg.register(FontPackage("helvetica", FontFamily::Helvetica));
        reg.register(FontPackage("tgheros", FontFamily::Helvetica));
        reg.register(FontPackage("sans", FontFamily::Helvetica));
        reg.register(FontPackage("palatino", FontFamily::Palatino));
        reg.register(FontPackage("mathpazo", FontFamily::Palatino));
        reg.register(FontPackage("tgpagella", FontFamily::Palatino));
        reg.register(FontPackage("newpxtext", FontFamily::Palatino));
        reg.register(FontPackage("lmodern", FontFamily::ComputerModern));
        reg.register(FontPackage("computermodern", FontFamily::ComputerModern));
        reg.register(FontPackage("cm", FontFamily::ComputerModern));
        reg.register(NoOpPackage("graphicx"));
        reg.register(NoOpPackage("amsmath"));
        reg.register(NoOpPackage("amssymb"));
        reg.register(NoOpPackage("inputenc"));
        reg.register(NoOpPackage("fontenc"));
        reg.register(NoOpPackage("hyperref"));
        reg.register(NoOpPackage("microtype"));
        reg.register(NoOpPackage("xcolor"));
        reg.register(NoOpPackage("babel"));
        reg.register(NoOpPackage("enumitem"));
        reg.register(NoOpPackage("listings"));
        reg.register(NoOpPackage("booktabs"));
        reg.register(NoOpPackage("array"));
        reg
    }
}

impl PackageRegistry {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    pub fn register<H: PackageHandler + 'static>(&mut self, handler: H) {
        self.handlers.push(Box::new(handler));
    }

    pub fn apply(&self, package: &Package<'_>, config: &mut DocumentConfig) {
        for handler in &self.handlers {
            if handler.name().eq_ignore_ascii_case(package.name) {
                handler.apply(&package.options, config);
                return;
            }
        }
    }
}

/// One inch in TeX points (`1in = 72.27pt`).
/// One inch in TeX points.
pub const TEX_INCH: f32 = 72.27;
/// PDF big points per TeX point (72 / 72.27): the backend's page transform.
pub const BP_PER_PT: f32 = 72.0 / 72.27;
/// A4 (`210mm`, `297mm`) and US letter (`8.5in`, `11in`) paper as `scan_dimen` (§458)
/// converts them, in scaled points.
pub const A4_SP: (i32, i32) = (39158276, 55380990);
pub const LETTER_SP: (i32, i32) = (40258437, 52099153);
/// The same in TeX points.
pub const A4_PT: (f32, f32) = (A4_SP.0 as f32 / 65536.0, A4_SP.1 as f32 / 65536.0);
pub const LETTER_PT: (f32, f32) = (LETTER_SP.0 as f32 / 65536.0, LETTER_SP.1 as f32 / 65536.0);

/// A paper dimension in scaled points: the exact value of a standard paper size whose
/// points `pt` holds (f32 cannot carry its last sp bits), else `pt` rounded.
pub fn paper_sp(pt: f32) -> i32 {
    let sp = crate::backend::scaled::to_sp(pt);
    [A4_SP.0, A4_SP.1, LETTER_SP.0, LETTER_SP.1].into_iter().find(|&k| (k - sp).abs() <= 4).unwrap_or(sp)
}

/// `\paperwidth` × `\paperheight` in TeX points for the standard class paper options.
fn class_paper_size(option: &str) -> Option<(f32, f32)> {
    match option {
        "letterpaper" => Some(LETTER_PT),
        "a4paper" => Some(A4_PT),
        "a5paper" => Some((421.104, 597.508)),
        "b5paper" => Some((499.464, 708.661)),
        "legalpaper" => Some((614.295, 1011.78)),
        "executivepaper" => Some((542.025, 722.7)),
        _ => None,
    }
}

/// Text-block geometry of the standard LaTeX classes (`article`, `report`, `book`),
/// transcribed from `size10.clo` / `size11.clo` / `size12.clo` (non-compatibility branch):
///
/// - `\textwidth = min(\paperwidth - 2in, W)` with W = 345 / 360 / 390pt (twocolumn: 2W),
///   truncated to whole points (`\@settopoint`);
/// - `\textheight = floor((\paperheight - 3.5in) / \baselineskip) * \baselineskip + \topskip`;
/// - `\oddsidemargin = .5(\paperwidth - \textwidth) - 1in` (twoside: `.4`), truncated;
/// - `\topmargin = .5(\paperheight - 2in - \headheight - \headsep - \textheight - \footskip)`, truncated.
///
/// `\paperwidth`/`\paperheight` come from the class paper option (letter by default), which
/// can differ from the PDF media box exactly as in pdfLaTeX without `geometry`.
fn apply_standard_class_geometry(
    config: &mut DocumentConfig,
    paper: (f32, f32),
    size_pt: u8,
    twoside: bool,
    twocolumn: bool,
) {
    let (paper_w, paper_h) = paper;
    let (base_tw, top_skip) = match size_pt {
        11 => (360.0f32, 11.0f32),
        12 => (390.0, 12.0),
        _ => (345.0, 10.0),
    };
    let head_height = 12.0f32;
    let head_sep = 25.0f32;
    let foot_skip = 30.0f32;
    let baseline_skip = config.base_leading;

    let max_tw = if twocolumn { 2.0 * base_tw } else { base_tw };
    let text_width = (paper_w - 2.0 * TEX_INCH).min(max_tw).floor();
    let lines = ((paper_h - 3.5 * TEX_INCH) / baseline_skip).floor().max(1.0);
    let text_height = lines * baseline_skip + top_skip;
    let side_frac = if twoside { 0.4 } else { 0.5 };
    let odd_side_margin = (side_frac * (paper_w - text_width) - TEX_INCH).trunc();
    let top_margin =
        (0.5 * (paper_h - 2.0 * TEX_INCH - head_height - head_sep - text_height - foot_skip)).trunc();

    config.margin_left = TEX_INCH + odd_side_margin;
    config.margin_right = config.page_width - config.margin_left - text_width;
    config.margin_top = TEX_INCH + top_margin + head_height + head_sep;
    // In sp from the exact paper height: f32 cannot hold A4's last bits at 845pt.
    {
        use crate::backend::scaled::{sp_pt, to_sp};
        config.margin_bottom = sp_pt(paper_sp(config.page_height) - to_sp(config.margin_top) - to_sp(text_height));
    }
    config.top_skip = top_skip;
    config.foot_skip = foot_skip;
    config.head_sep = head_sep;
}

/// Apply document class options (e.g. `12pt`, `letterpaper`, `landscape`, `sans`) to configuration.
///
/// For the standard classes this also derives the text block (`\textwidth`, `\textheight`,
/// `\oddsidemargin`, `\topmargin`, `\topskip`) the way the class size file does.
pub fn apply_doc_class(doc_class: &DocClass<'_>, config: &mut DocumentConfig) {
    let mut paper = class_paper_size("letterpaper").unwrap_or((614.295, 794.97));
    let mut size_pt = 10u8;
    let mut landscape = false;
    let mut twoside = crate::expand::classes::base_class(doc_class.name) == "book";
    let mut twocolumn = false;
    for &option_str in &doc_class.options {
        let option_str = option_str.trim();
        if let Some(sz) = class_paper_size(option_str) {
            paper = sz;
        }
        match option_str {
            // Values from LaTeX's size10.clo / size11.clo / size12.clo (\@xipt = 10.95pt).
            "10pt" => {
                size_pt = 10;
                config.base_font_size = 10.0;
                config.base_leading = 12.0;
                config.par_indent = 15.0;
            }
            "11pt" => {
                size_pt = 11;
                config.base_font_size = 10.95;
                config.base_leading = 13.6;
                config.par_indent = 17.0;
            }
            "12pt" => {
                size_pt = 12;
                config.base_font_size = 12.0;
                config.base_leading = 14.5;
                // size12.clo: 1.5em in cmr12, the font at class load (its quad is 11.75pt).
                config.par_indent = 17.625;
            }
            "a4paper" => {
                (config.page_width, config.page_height) = A4_PT;
            }
            "letterpaper" => {
                (config.page_width, config.page_height) = LETTER_PT;
            }
            "landscape" => {
                landscape = true;
                if config.page_width < config.page_height {
                    std::mem::swap(&mut config.page_width, &mut config.page_height);
                }
            }
            "twoside" => twoside = true,
            "oneside" => twoside = false,
            "twocolumn" => twocolumn = true,
            "onecolumn" => twocolumn = false,
            "sans" | "sansserif" | "sf" => {
                config.font_family = FontFamily::Helvetica;
            }
            "serif" | "roman" | "rm" => {
                config.font_family = FontFamily::Times;
            }
            "palatino" => {
                config.font_family = FontFamily::Palatino;
            }
            "computermodern" | "cm" => {
                config.font_family = FontFamily::ComputerModern;
            }
            _ => {}
        }
    }
    if landscape {
        paper = (paper.1, paper.0);
    }
    if matches!(crate::expand::classes::base_class(doc_class.name), "article" | "report" | "book") {
        apply_standard_class_geometry(config, paper, size_pt, twoside, twocolumn);
    }
}

/// Parse TeX dimension string into PDF points (e.g. `1in`, `2.54cm`, `20mm`, `72pt`).
pub fn parse_dimension_pt(dimension_str: &str) -> Option<f32> {
    parse_dimension_pt_in(dimension_str, 11.0, 5.5)
}

/// [`parse_dimension_pt`] with the current font's `em` (quad) and `ex` (x-height) in pt.
pub fn parse_dimension_pt_in(dimension_str: &str, em: f32, ex: f32) -> Option<f32> {
    // TeX's own arithmetic (`scan_dimen`), so a length is the scaled points TeX has.
    let sp = |v: f32| (v as f64 * 65536.0).round() as i64;
    crate::backend::scaled::scan_dimen(dimension_str, sp(em), sp(ex)).map(|v| (v as f64 / 65536.0) as f32)
}

/// [`parse_glue_pt`] with the shrink: (width, stretch, shrink) in pt.
pub fn parse_glue3_pt(glue_str: &str) -> Option<(f32, f32, f32)> {
    parse_glue3_pt_in(glue_str, 11.0, 5.5)
}

/// [`parse_glue3_pt`] with the current font's `em` and `ex` in pt.
pub fn parse_glue3_pt_in(glue_str: &str, em: f32, ex: f32) -> Option<(f32, f32, f32)> {
    if let Some(g) = named_skip(glue_str.trim()) {
        return Some(g);
    }
    let (w, stretch) = parse_glue_pt_in(glue_str, em, ex)?;
    let shrink = glue_str
        .find("minus")
        .and_then(|p| parse_dimension_pt_in(glue_str[p + "minus".len()..].trim(), em, ex))
        .unwrap_or(0.0);
    Some((w, stretch, shrink))
}

/// The kernel's fixed skip registers (latex.ltx, `\smallskipamount` etc.): (width, stretch,
/// shrink) in pt.
pub fn named_skip(name: &str) -> Option<(f32, f32, f32)> {
    Some(match name {
        "\\smallskipamount" => (3.0, 1.0, 1.0),
        "\\medskipamount" => (6.0, 2.0, 2.0),
        "\\bigskipamount" => (12.0, 4.0, 4.0),
        "\\z@skip" | "\\z@" => (0.0, 0.0, 0.0),
        _ => return None,
    })
}

/// Parse TeX glue string into (width_pt, stretch_pt).
/// For example:
/// - `0pt plus 1fil` -> `(0.0, f32::INFINITY)`
/// - `0pt` -> `(0.0, 0.0)`
/// - `1cm plus 1fil` -> `(28.346..., f32::INFINITY)`
/// - `10pt plus 5pt` -> `(10.0, 5.0)`
pub fn parse_glue_pt(glue_str: &str) -> Option<(f32, f32)> {
    parse_glue_pt_in(glue_str, 11.0, 5.5)
}

/// [`parse_glue_pt`] with the current font's `em` and `ex` in pt.
pub fn parse_glue_pt_in(glue_str: &str, em: f32, ex: f32) -> Option<(f32, f32)> {
    let s = glue_str.trim();
    if s.is_empty() {
        return None;
    }
    let (w_str, rest) = if let Some(pos) = s.find("plus") {
        (&s[..pos], &s[pos..])
    } else if let Some(pos) = s.find("minus") {
        (&s[..pos], "")
    } else {
        (s, "")
    };
    let width = parse_dimension_pt_in(w_str.trim(), em, ex)?;
    let stretch = if let Some(after_plus) = rest.strip_prefix("plus") {
        let after_plus = after_plus.trim();
        let stretch_str = if let Some(m_pos) = after_plus.find("minus") {
            &after_plus[..m_pos]
        } else {
            after_plus
        }
        .trim();
        if stretch_str.contains("fil") {
            f32::INFINITY
        } else {
            parse_dimension_pt_in(stretch_str, em, ex).unwrap_or(0.0)
        }
    } else {
        0.0
    };
    Some((width, stretch))
}
