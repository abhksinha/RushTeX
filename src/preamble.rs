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
    pub page_numbering: bool,
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
            page_numbering: true,
        }
    }
}

/// Parsed document preamble containing document class, packages, and metadata.
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
                        document_config.page_width = 595.0;
                        document_config.page_height = 842.0;
                    }
                    "letterpaper" => {
                        document_config.page_width = 612.0;
                        document_config.page_height = 792.0;
                    }
                    "landscape" => {
                        if document_config.page_width < document_config.page_height {
                            std::mem::swap(&mut document_config.page_width, &mut document_config.page_height);
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
        for handler in &self.handlers {
            if handler.name().eq_ignore_ascii_case(package.name) {
                handler.apply(&package.options, config);
                return;
            }
        }
    }
}

/// Apply document class options (e.g. `12pt`, `letterpaper`, `landscape`) to configuration.
pub fn apply_doc_class(doc_class: &DocClass<'_>, config: &mut DocumentConfig) {
    for &option_str in &doc_class.options {
        match option_str.trim() {
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
pub fn parse_dimension_pt(dimension_str: &str) -> Option<f32> {
    let trimmed_str = dimension_str.trim();
    if trimmed_str.is_empty() {
        return None;
    }

    let (number_part, unit_suffix) = if let Some(stripped) = trimmed_str.strip_suffix("in") {
        (stripped, "in")
    } else if let Some(stripped) = trimmed_str.strip_suffix("cm") {
        (stripped, "cm")
    } else if let Some(stripped) = trimmed_str.strip_suffix("mm") {
        (stripped, "mm")
    } else if let Some(stripped) = trimmed_str.strip_suffix("pt") {
        (stripped, "pt")
    } else {
        (trimmed_str, "pt")
    };

    let numeric_value = number_part.trim().parse::<f32>().ok()?;
    match unit_suffix {
        "in" => Some(numeric_value * 72.0),
        "cm" => Some(numeric_value * (72.0 / 2.54)),
        "mm" => Some(numeric_value * (72.0 / 25.4)),
        "pt" => Some(numeric_value),
        _ => None,
    }
}
