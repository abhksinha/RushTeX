//! Extensible preamble parsing and document configuration.
//!
//! Provides modular architecture for LaTeX preamble processing:
//! - Document class options (`10pt`, `11pt`, `12pt`, `a4paper`, `letterpaper`, `landscape`)
//! - Package handler registry (`\usepackage[options]{package}`)
//! - Document metadata (`\title`, `\author`, `\date`)
//! - Open API designed for multi-contributor package extensions.

use crate::tex::Inline;

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
    pub base_font_size: f32,
    pub base_leading: f32,
}

impl Default for DocumentConfig {
    fn default() -> Self {
        Self {
            page_width: 595.0,  // Standard A4 width in pt
            page_height: 842.0, // Standard A4 height in pt
            margin_left: 72.0,  // 1 inch = 72 pt
            margin_right: 72.0,
            margin_top: 72.0,
            margin_bottom: 72.0,
            base_font_size: 11.0,
            base_leading: 14.0,
        }
    }
}

/// Parsed document preamble.
#[derive(Debug, Clone, PartialEq)]
pub struct Preamble<'a> {
    pub class: Option<DocClass<'a>>,
    pub packages: Vec<Package<'a>>,
    pub title: Option<Vec<Inline<'a>>>,
    pub author: Option<Vec<Inline<'a>>>,
    pub date: Option<Vec<Inline<'a>>>,
    pub config: DocumentConfig,
}

impl<'a> Default for Preamble<'a> {
    fn default() -> Self {
        Self {
            class: None,
            packages: Vec::new(),
            title: None,
            author: None,
            date: None,
            config: DocumentConfig::default(),
        }
    }
}

/// Trait for package handlers.
///
/// Contributors can implement this trait to add support for standard LaTeX packages
/// without modifying parser internals.
pub trait PackageHandler: Sync + Send {
    /// Canonical name of the package, e.g. "geometry".
    fn name(&self) -> &'static str;

    /// Apply package options to document configuration.
    fn apply(&self, options: &[&str], config: &mut DocumentConfig);
}

/// Built-in handler for `\usepackage{geometry}`.
pub struct GeometryPackage;

impl PackageHandler for GeometryPackage {
    fn name(&self) -> &'static str {
        "geometry"
    }

    fn apply(&self, options: &[&str], config: &mut DocumentConfig) {
        for &opt in options {
            let opt = opt.trim();
            if let Some((key, val)) = opt.split_once('=') {
                let key = key.trim();
                let val = val.trim();
                if let Some(pts) = parse_dimension_pt(val) {
                    match key {
                        "margin" => {
                            config.margin_left = pts;
                            config.margin_right = pts;
                            config.margin_top = pts;
                            config.margin_bottom = pts;
                        }
                        "left" | "lmargin" => config.margin_left = pts,
                        "right" | "rmargin" => config.margin_right = pts,
                        "top" | "tmargin" => config.margin_top = pts,
                        "bottom" | "bmargin" => config.margin_bottom = pts,
                        _ => {}
                    }
                }
            } else {
                match opt {
                    "a4paper" => {
                        config.page_width = 595.0;
                        config.page_height = 842.0;
                    }
                    "letterpaper" => {
                        config.page_width = 612.0;
                        config.page_height = 792.0;
                    }
                    "landscape" => {
                        if config.page_width < config.page_height {
                            std::mem::swap(&mut config.page_width, &mut config.page_height);
                        }
                    }
                    _ => {}
                }
            }
        }
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

/// Extensible registry of package handlers.
pub struct PackageRegistry {
    handlers: Vec<Box<dyn PackageHandler>>,
}

impl Default for PackageRegistry {
    fn default() -> Self {
        let mut reg = Self::new();
        reg.register(GeometryPackage);
        reg.register(NoOpPackage("graphicx"));
        reg.register(NoOpPackage("amsmath"));
        reg.register(NoOpPackage("amssymb"));
        reg.register(NoOpPackage("inputenc"));
        reg.register(NoOpPackage("fontenc"));
        reg.register(NoOpPackage("hyperref"));
        reg.register(NoOpPackage("microtype"));
        reg.register(NoOpPackage("xcolor"));
        reg.register(NoOpPackage("babel"));
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
        for h in &self.handlers {
            if h.name().eq_ignore_ascii_case(package.name) {
                h.apply(&package.options, config);
                return;
            }
        }
    }
}

/// Apply document class options (e.g. `12pt`, `letterpaper`, `landscape`) to configuration.
pub fn apply_doc_class(class: &DocClass<'_>, config: &mut DocumentConfig) {
    for &opt in &class.options {
        match opt.trim() {
            "10pt" => {
                config.base_font_size = 10.0;
                config.base_leading = 13.0;
            }
            "11pt" => {
                config.base_font_size = 11.0;
                config.base_leading = 14.0;
            }
            "12pt" => {
                config.base_font_size = 12.0;
                config.base_leading = 15.0;
            }
            "a4paper" => {
                config.page_width = 595.0;
                config.page_height = 842.0;
            }
            "letterpaper" => {
                config.page_width = 612.0;
                config.page_height = 792.0;
            }
            "landscape" => {
                if config.page_width < config.page_height {
                    std::mem::swap(&mut config.page_width, &mut config.page_height);
                }
            }
            _ => {}
        }
    }
}

/// Parse TeX dimension string into PDF points (e.g. `1in`, `2.54cm`, `20mm`, `72pt`).
pub fn parse_dimension_pt(val: &str) -> Option<f32> {
    let s = val.trim();
    if s.is_empty() {
        return None;
    }

    let (num_part, unit) = if let Some(stripped) = s.strip_suffix("in") {
        (stripped, "in")
    } else if let Some(stripped) = s.strip_suffix("cm") {
        (stripped, "cm")
    } else if let Some(stripped) = s.strip_suffix("mm") {
        (stripped, "mm")
    } else if let Some(stripped) = s.strip_suffix("pt") {
        (stripped, "pt")
    } else {
        (s, "pt")
    };

    let num = num_part.trim().parse::<f32>().ok()?;
    match unit {
        "in" => Some(num * 72.0),
        "cm" => Some(num * (72.0 / 2.54)),
        "mm" => Some(num * (72.0 / 25.4)),
        "pt" => Some(num),
        _ => None,
    }
}
