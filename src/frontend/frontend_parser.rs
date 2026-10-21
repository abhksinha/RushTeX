//! # TeX Syntax Lexer & Recursive-Descent Parser (`parser`)

use super::frontend_ast::*;
use crate::backend::scaled::{EM_1, EM_16667, EM_2222, EM_2777};
use crate::config::Preamble;

pub fn parse(source_code: &str) -> Result<Document<'_>, ParseError> {
    let mut parser = Parser::new(source_code);
    let has_document_environment = parser.parse_preamble_if_present()?;
    let blocks = parser.parse_blocks()?;
    parser.finish_document(has_document_environment)?;
    Ok(Document {
        preamble: parser.preamble,
        blocks,
    })
}

/// An unsupported construct seen in gap-inventory mode: byte offset, kind, name.
pub(crate) type Gap<'a> = (u32, &'static str, &'a str);

pub(crate) struct Parser<'a> {
    pub(crate) source_code: &'a str,
    pub(crate) byte_offset: usize,
    pub(crate) preamble: Preamble<'a>,
    pub(crate) doc_env_start: Option<usize>,
    /// latex.ltx `\@endpe`: a list-like environment ended without a blank line after it,
    /// so the paragraph that follows is not indented.
    pub(crate) end_par_env: bool,
    /// The text after the last display continues its paragraph (no blank line between).
    pub(crate) display_resumes: bool,
    /// A `\paragraph` heading waiting for the paragraph it runs into.
    run_in_heading: Option<Vec<Inline<'a>>>,
    /// Colours from `\definecolor`, looked up before the xcolor base colours.
    pub(crate) colors: Vec<(&'a str, PackedColor)>,
    /// LaTeX's `footnote` counter.
    footnote_counter: u32,
    /// Inside `\title`, `\author` or `\date`: a `\footnote` is a `\thanks`.
    in_title: bool,
    /// Closing braces of open groups that hold a display or list (`{\setlength..
    /// \begin{multline*}..}`): they only scope settings, so the parser reads through them.
    open_block_groups: Vec<usize>,
    /// The size declaration and math parameters in force when each open block group
    /// began, restored at its `}`.
    block_group_sizes: Vec<(Option<FontSize>, crate::math::MathParams)>,
    /// Math parameters in force (`\delimiterfactor`, `\multlinegap`, ...).
    math_params: crate::math::MathParams,
    /// The last size declaration outside any inline group (`None`: the class's
    /// `\normalsize`).
    cur_size: Option<FontSize>,
    /// Depth of inline groups being parsed, whose declarations end with them.
    inline_depth: u32,
    /// Gap-inventory mode (`rushtex --gaps`): unknown commands and environments are
    /// recorded as `(byte offset, kind, name)` and skipped instead of ending the parse.
    /// `None` in normal compiles, so the error paths stay cold.
    pub(crate) gaps: Option<Vec<Gap<'a>>>,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(source_code: &'a str) -> Self {
        Self {
            source_code,
            byte_offset: 0,
            preamble: Preamble::default(),
            doc_env_start: None,
            end_par_env: false,
            display_resumes: false,
            run_in_heading: None,
            colors: Vec::new(),
            footnote_counter: 0,
            in_title: false,
            open_block_groups: Vec::new(),
            block_group_sizes: Vec::new(),
            math_params: crate::math::MathParams::default(),
            cur_size: None,
            inline_depth: 0,
            gaps: None,
        }
    }

    #[inline(always)]
    pub(crate) fn current_line_number(&self) -> usize {
        let safe = self.byte_offset.min(self.source_code.len());
        self.source_code.as_bytes()[..safe]
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
            + 1
    }

    #[inline(always)]
    pub(crate) fn err(&self, message: impl Into<String>) -> ParseError {
        self.err_at(self.byte_offset, message)
    }

    #[inline(always)]
    pub(crate) fn err_at(&self, byte_offset: usize, message: impl Into<String>) -> ParseError {
        crate::error_diagnostic::locate_diagnostic(self.source_code, byte_offset, message)
    }

    #[inline(always)]
    pub(crate) fn err_span(
        &self,
        byte_offset: usize,
        span_len: usize,
        message: impl Into<String>,
        label: Option<&str>,
        note: Option<&str>,
        help: Option<&str>,
    ) -> ParseError {
        crate::error_diagnostic::locate_diagnostic_detailed(
            self.source_code,
            byte_offset,
            span_len,
            message,
            label,
            note,
            help,
        )
    }

    #[inline(always)]
    pub(crate) fn rest(&self) -> &'a str {
        &self.source_code[self.byte_offset..]
    }

    #[inline(always)]
    pub(crate) fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    #[inline(always)]
    pub(crate) fn bump(&mut self) -> Option<char> {
        let character = self.rest().chars().next()?;
        self.byte_offset += character.len_utf8();
        Some(character)
    }

    #[inline(always)]
    pub(crate) fn starts_with(&self, prefix: &str) -> bool {
        self.rest().starts_with(prefix)
    }

    #[inline(always)]
    pub(crate) fn eat(&mut self, token: &str) -> bool {
        if self.starts_with(token) {
            self.byte_offset += token.len();
            true
        } else {
            false
        }
    }

    pub(crate) fn skip_ws(&mut self) {
        let bytes = self.rest().as_bytes();
        let mut idx = 0;
        let len = bytes.len();
        while idx + 8 <= len {
            let chunk = u64::from_le_bytes(bytes[idx..idx + 8].try_into().unwrap());
            if chunk == 0x2020_2020_2020_2020 {
                idx += 8;
            } else {
                break;
            }
        }
        while idx < len {
            match bytes[idx] {
                b' ' | b'\t' | b'\r' | b'\n' => idx += 1,
                b'%' => {
                    self.byte_offset += idx;
                    self.skip_comment();
                    return self.skip_ws();
                }
                _ => break,
            }
        }
        self.byte_offset += idx;
    }

    /// `em` and `ex` of the document font (`\fontdimen6`, `\fontdimen5` of the roman face).
    fn em_ex(&self) -> (f32, f32) {
        let c = &self.preamble.config;
        (
            crate::backend::scaled::quad_pt(c.font_family, c.base_font_size),
            crate::backend::scaled::x_height_pt(c.font_family, c.base_font_size),
        )
    }

    /// A dimension in pt, with `em`/`ex` taken from the document font.
    fn dim_pt(&self, s: &str) -> Option<f32> {
        let (em, ex) = self.em_ex();
        crate::doc_config::parse_dimension_pt_in(s, em, ex)
    }

    pub(crate) fn skip_hws(&mut self) {
        let bytes = self.rest().as_bytes();
        let mut idx = 0;
        let len = bytes.len();
        while idx + 8 <= len {
            let chunk = u64::from_le_bytes(bytes[idx..idx + 8].try_into().unwrap());
            if chunk == 0x2020_2020_2020_2020 {
                idx += 8;
            } else {
                break;
            }
        }
        while idx < len {
            match bytes[idx] {
                b' ' | b'\t' | b'\r' => idx += 1,
                _ => break,
            }
        }
        self.byte_offset += idx;
    }

    pub(crate) fn skip_comment(&mut self) {
        if let Some(pos) = self.rest().find('\n') {
            self.byte_offset += pos + 1;
            // TeX skips the blanks that start the next line (state N).
            let rest = self.rest();
            self.byte_offset += rest.len() - rest.trim_start_matches([' ', '\t']).len();
        } else {
            self.byte_offset = self.source_code.len();
        }
    }

    /// TeX skips spaces before an undelimited macro argument, the end of a line
    /// included, but not a blank line (a `\par`).
    pub(crate) fn skip_arg_space(&mut self) {
        self.skip_hws();
        let rest = self.rest();
        let after = rest.strip_prefix("\r\n").or_else(|| rest.strip_prefix('\n'));
        if let Some(next) = after {
            if !next.trim_start_matches([' ', '\t', '\r']).starts_with('\n') {
                self.byte_offset += rest.len() - next.len();
                self.skip_hws();
            }
        }
    }

    pub(crate) fn at_blank_line(&self) -> bool {
        let r = self.rest();
        r.starts_with("\n\n") || r.starts_with("\r\n\r\n") || r.starts_with("\n\r\n")
    }

    #[inline(always)]
    pub(crate) fn peek_control_word(&self) -> Option<&'a str> {
        let bytes = self.rest().as_bytes();
        if bytes.len() < 2 || bytes[0] != b'\\' {
            return None;
        }
        if !bytes[1].is_ascii_alphabetic() {
            return None;
        }
        let mut end = 2;
        while end < bytes.len() && bytes[end].is_ascii_alphabetic() {
            end += 1;
        }
        Some(&self.rest()[1..end])
    }

    pub(crate) fn cmd_ends_at(&self, cmd: &str) -> bool {
        if !self.starts_with(cmd) {
            return false;
        }
        let after_idx = self.byte_offset + cmd.len();
        let bytes = self.source_code.as_bytes();
        if after_idx < bytes.len() {
            !bytes[after_idx].is_ascii_alphabetic()
        } else {
            true
        }
    }

    fn at_item(&self) -> bool {
        self.cmd_ends_at("\\item")
    }

    fn at_end_env(&self, name: &str) -> bool {
        const PFX: &str = "\\end{";
        if !self.starts_with(PFX) {
            return false;
        }
        let after = &self.source_code[self.byte_offset + PFX.len()..];
        after.starts_with(name) && after[name.len()..].starts_with('}')
    }

    pub(crate) fn eat_end_env(&mut self, name: &str) -> bool {
        if !self.at_end_env(name) {
            return false;
        }
        self.eat("\\end{");
        self.eat(name);
        self.eat("}");
        true
    }

    fn at_begin_env(&self, name: &str) -> bool {
        const PFX: &str = "\\begin{";
        if !self.starts_with(PFX) {
            return false;
        }
        let after = &self.source_code[self.byte_offset + PFX.len()..];
        after.starts_with(name) && after[name.len()..].starts_with('}')
    }

    fn eat_begin_env(&mut self, name: &str) -> bool {
        if !self.at_begin_env(name) {
            return false;
        }
        self.eat("\\begin{");
        self.eat(name);
        self.eat("}");
        true
    }

    /// `\definecolor{name}{model}{spec}`.
    fn parse_definecolor(&mut self) -> Result<(), ParseError> {
        self.skip_hws();
        let name = self.brace_arg()?.trim();
        self.skip_hws();
        let model = self.brace_arg()?;
        self.skip_hws();
        let spec = self.brace_arg()?;
        let color = PackedColor::from_model(model, spec).ok_or_else(|| self.err(format!("unknown colour model or value '{model}' '{spec}'")))?;
        match self.colors.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = color,
            None => self.colors.push((name, color)),
        }
        Ok(())
    }

    /// The colour argument of `\color`/`\textcolor`: `[model]{spec}` or `{expression}`.
    fn parse_color_arg(&mut self) -> Result<PackedColor, ParseError> {
        self.skip_hws();
        if self.eat("[") {
            let start = self.byte_offset;
            while !matches!(self.peek(), Some(']') | None) {
                self.bump();
            }
            let model = &self.source_code[start..self.byte_offset];
            if !self.eat("]") {
                return Err(self.err("unclosed colour model '['"));
            }
            self.skip_hws();
            let spec = self.brace_arg()?;
            return PackedColor::from_model(model, spec)
                .ok_or_else(|| self.err(format!("unknown colour model or value '{model}' '{spec}'")));
        }
        let expr = self.brace_arg()?;
        self.color_expression(expr).ok_or_else(|| self.err(format!("undefined colour '{}'", expr.trim())))
    }

    /// An xcolor expression: `name`, `name!pct` (mixed with white) or `name!pct!name2`,
    /// chained left to right.
    fn color_expression(&self, expr: &str) -> Option<PackedColor> {
        PackedColor::resolve(expr, &self.colors)
    }

    /// Parses a math formula with the document's `\definecolor` names in scope.
    fn parse_math(&self, src: &'a str) -> crate::math::MathNode<'a> {
        crate::math::parse_math_str_with_colors(src, &self.colors, &self.math_params)
    }

    pub(crate) fn parse_optional_brackets(&mut self) -> Result<Vec<&'a str>, ParseError> {
        self.skip_hws();
        let open_offset = self.byte_offset;
        if !self.eat("[") {
            return Ok(Vec::new());
        }
        let start = self.byte_offset;
        while !matches!(self.peek(), Some(']') | None) {
            self.bump();
        }
        let raw = &self.source_code[start..self.byte_offset];
        if !self.eat("]") {
            return Err(self.err_span(
                open_offset,
                1,
                "unclosed optional argument '['",
                Some("opening '[' starts here"),
                Some("reached end of file without finding matching ']'"),
                Some("add closing ']' to terminate optional arguments"),
            ));
        }
        let options = raw
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        Ok(options)
    }

    fn skip_preamble_command(&mut self) -> Result<(), ParseError> {
        if self.peek() == Some('\\') {
            self.bump();
            while matches!(self.peek(), Some(c) if c.is_ascii_alphabetic()) {
                self.bump();
            }
            self.skip_hws();
            while self.starts_with("[") || self.starts_with("{") {
                if self.starts_with("[") {
                    let _ = self.parse_optional_brackets();
                } else if self.starts_with("{") {
                    let _ = self.require_group();
                }
                self.skip_hws();
            }
        } else {
            self.bump();
        }
        Ok(())
    }

    /// `\setcounter{secnumdepth}{n}` and `{tocdepth}`; the expander keeps the other counters.
    fn parse_setcounter(&mut self) -> Result<(), ParseError> {
        let name = self.brace_arg()?.trim();
        let value = self.brace_arg()?.trim().parse::<i8>().unwrap_or(0);
        match name {
            "secnumdepth" => self.preamble.config.sec_num_depth = value,
            "tocdepth" => self.preamble.config.toc_depth = value,
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn parse_preamble_if_present(&mut self) -> Result<bool, ParseError> {
        let registry = crate::doc_config::PackageRegistry::default();
        let mut in_preamble = false;

        loop {
            self.skip_ws();
            if self.peek().is_none() {
                if in_preamble {
                    // LaTeX stops with "Missing \begin{document}"; the material after the
                    // preamble would otherwise be dropped without a word.
                    return Err(self.err("missing \\begin{document}"));
                }
                return Ok(false);
            }
            if self.cmd_ends_at("\\NeedsTeXFormat") {
                // `\NeedsTeXFormat{LaTeX2e}[date]` may come before `\documentclass`.
                in_preamble = true;
                self.eat("\\NeedsTeXFormat");
                self.brace_arg()?;
                self.parse_optional_brackets()?;
                continue;
            }
            if self.at_begin_env("document") {
                self.doc_env_start = Some(self.byte_offset);
                self.eat_begin_env("document");
                return Ok(true);
            }
            if self.cmd_ends_at("\\documentclass") {
                in_preamble = true;
                self.eat("\\documentclass");
                let options = self.parse_optional_brackets()?;
                let name = self.brace_ident()?;
                let class = crate::doc_config::DocClass { name, options };
                crate::doc_config::apply_doc_class(&class, &mut self.preamble.config);
                self.preamble.class = Some(class);
                continue;
            }
            if self.cmd_ends_at("\\usepackage") {
                in_preamble = true;
                self.eat("\\usepackage");
                let options = self.parse_optional_brackets()?;
                // `\usepackage{amsmath, amssymb}` loads each package with the same options.
                let list = self.brace_name_list()?;
                for name in list.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                    let pkg = crate::doc_config::Package { name, options: options.clone() };
                    registry.apply(&pkg, &mut self.preamble.config);
                    self.preamble.packages.push(pkg);
                }
                continue;
            }
            if self.cmd_ends_at("\\setcounter") {
                in_preamble = true;
                self.eat("\\setcounter");
                self.parse_setcounter()?;
                continue;
            }
            if self.cmd_ends_at("\\definecolor") {
                in_preamble = true;
                self.eat("\\definecolor");
                self.parse_definecolor()?;
                continue;
            }
            if self.cmd_ends_at("\\title") {
                in_preamble = true;
                self.eat("\\title");
                self.preamble.title = Some(self.title_arg()?);
                continue;
            }
            if self.cmd_ends_at("\\author") {
                in_preamble = true;
                self.eat("\\author");
                self.preamble.author = Some(self.title_arg()?);
                continue;
            }
            if self.cmd_ends_at("\\date") {
                in_preamble = true;
                self.eat("\\date");
                self.preamble.date = Some(self.title_arg()?);
                continue;
            }
            if self.cmd_ends_at("\\pagestyle") {
                in_preamble = true;
                self.eat("\\pagestyle");
                let style = self.brace_ident()?;
                if let Some(style) = crate::frontend::ast::PageStyle::from_name(style) {
                    self.preamble.config.page_style = style;
                    self.preamble.config.page_numbering = style != crate::frontend::ast::PageStyle::Empty;
                }
                continue;
            }
            if self.cmd_ends_at("\\frenchspacing") {
                in_preamble = true;
                self.eat("\\frenchspacing");
                self.preamble.config.french_spacing = true;
                continue;
            }
            if self.cmd_ends_at("\\nonfrenchspacing") {
                in_preamble = true;
                self.eat("\\nonfrenchspacing");
                self.preamble.config.french_spacing = false;
                continue;
            }
            if self.cmd_ends_at("\\setlength") {
                in_preamble = true;
                self.eat("\\setlength");
                let param = self.macro_arg()?;
                let val_str = self.brace_arg()?;
                let key = param.trim().trim_start_matches('\\');
                if key == "parfillskip" {
                    if let Some((w, s)) = crate::doc_config::parse_glue_pt(val_str) {
                        self.preamble.config.par_fill_width = w;
                        self.preamble.config.par_fill_stretch = s;
                    }
                } else if key == "parskip" {
                    let (em, ex) = self.em_ex();
                    if let Some((w, st, sh)) = crate::doc_config::parse_glue3_pt_in(val_str, em, ex) {
                        let c = &mut self.preamble.config;
                        (c.par_skip, c.par_skip_stretch, c.par_skip_shrink) = (w, st, sh);
                    }
                } else if let Some(dim) = self.dim_pt(val_str) {
                    match key {
                        "parindent" => self.preamble.config.par_indent = dim,
                        "parskip" => self.preamble.config.par_skip = dim,
                        "hangindent" => self.preamble.config.hang_indent = dim,
                        "hangafter" => self.preamble.config.hang_after = dim as i32,
                        "leftskip" => self.preamble.config.left_skip = dim,
                        "rightskip" => self.preamble.config.right_skip = dim,
                        "labelsep" => self.preamble.config.list_label_sep = dim,
                        "leftmargin" | "leftmargini" => self.preamble.config.list_left_margin = dim,
                        "itemindent" => self.preamble.config.list_item_indent = dim,
                        "itemsep" => self.preamble.config.list_item_sep = dim,
                        "listparindent" => self.preamble.config.list_par_indent = dim,
                        "pretolerance" => self.preamble.config.pre_tolerance = dim as i32,
                        "tolerance" => self.preamble.config.tolerance = dim as i32,
                        "emergencystretch" => self.preamble.config.emergency_stretch = dim,
                        "looseness" => self.preamble.config.looseness = dim as i32,
                        "prevgraf" => self.preamble.config.prev_graf = dim as i32,
                        "lastlinefit" => self.preamble.config.last_line_fit = dim as i32,
                        _ => {}
                    }
                }
                continue;
            }
            if self.cmd_ends_at("\\renewcommand") {
                in_preamble = true;
                self.eat("\\renewcommand");
                self.skip_hws();
                let cmd = if self.starts_with("{") {
                    self.brace_arg()?
                } else if self.starts_with("\\") {
                    let start = self.byte_offset;
                    self.bump();
                    while matches!(self.peek(), Some(c) if c.is_ascii_alphabetic()) {
                        self.bump();
                    }
                    &self.source_code[start..self.byte_offset]
                } else {
                    ""
                };
                self.skip_hws();
                let val = if self.starts_with("{") {
                    self.brace_arg()?
                } else if self.starts_with("\\") {
                    let start = self.byte_offset;
                    self.bump();
                    while matches!(self.peek(), Some(c) if c.is_ascii_alphabetic()) {
                        self.bump();
                    }
                    &self.source_code[start..self.byte_offset]
                } else {
                    ""
                };
                if cmd.trim().trim_start_matches('\\') == "familydefault" {
                    if val.contains("sfdefault") {
                        self.preamble.config.font_family =
                            crate::font_metrics::FontFamily::Helvetica;
                    } else if val.contains("rmdefault")
                        && self.preamble.config.font_family == crate::font_metrics::FontFamily::Helvetica
                    {
                        // Back to the roman family, which is Computer Modern unless a font
                        // package replaced it.
                        self.preamble.config.font_family = crate::font_metrics::FontFamily::ComputerModern;
                    }
                }
                continue;
            }
            if in_preamble {
                self.skip_preamble_command()?;
                continue;
            }
            return Ok(false);
        }
    }

    pub(crate) fn finish_document(
        &mut self,
        has_document_environment: bool,
    ) -> Result<(), ParseError> {
        if has_document_environment {
            if !self.eat_end_env("document") {
                let doc_start = self.doc_env_start.unwrap_or(0);
                return Err(self.err_span(
                    doc_start,
                    "\\begin{document}".len(),
                    "unclosed \\begin{document}",
                    Some("this '\\begin{document}' was never closed"),
                    Some("reached end of file without finding matching '\\end{document}'"),
                    Some("add '\\end{document}' at the end of your file"),
                ));
            }
        } else {
            self.skip_ws();
            if self.peek().is_some() && !self.starts_with("\\end") {
                return Err(self.err("trailing unparsed input"));
            }
        }
        Ok(())
    }

    pub(crate) fn parse_next_block(&mut self) -> Result<Option<Block<'a>>, ParseError> {
        // Only the block right after a display can resume its paragraph.
        let display_resumes = std::mem::take(&mut self.display_resumes);
        macro_rules! parse_para_or_continue {
            () => {{
                let para_start = self.byte_offset;
                let mut inls = self.parse_para_inlines()?;
                if let Some(head) = self.run_in_heading.take() {
                    inls.splice(0..0, head);
                    return Ok(Some(Block::Para(inls)));
                }
                if !is_empty_inlines(&inls) {
                    if display_resumes {
                        inls.splice(0..0, [Inline::NoIndent, Inline::DisplayResumes]);
                    } else if std::mem::take(&mut self.end_par_env) {
                        inls.insert(0, Inline::NoIndent);
                    }
                    return Ok(Some(Block::Para(inls)));
                } else if has_lasting_declaration(&inls) {
                    // `\color{..}` or `\small` alone before a blank line typesets nothing
                    // but holds for the paragraphs after it.
                    return Ok(Some(Block::Para(inls)));
                } else if self.byte_offset > para_start {
                    // Only spaces and paragraph-local declarations: nothing to set.
                    continue;
                } else if self.peek().is_some()
                    && !self.starts_with("\\begin")
                    && !self.starts_with("\\end")
                    && !self.at_item()
                    && !self.starts_with("$$")
                    && !self.starts_with("\\[")
                {
                    return Ok(None);
                }
                continue;
            }};
        }

        loop {
            self.skip_ws();
            let next_b = match self.rest().as_bytes().first() {
                Some(&b) => b,
                None => return Ok(None),
            };

            if next_b == b'$' {
                if self.starts_with("$$") {
                    let open_offset = self.byte_offset;
                    self.eat("$$");
                    let start = self.byte_offset;
                    let rest = self.rest();
                    if let Some(pos) = rest.find("$$") {
                        let math_slice = &self.source_code[start..start + pos];
                        self.byte_offset = start + pos + 2;
                        let math = self.parse_math(math_slice);
                        let par = self.display_par(open_offset, false);
                        self.note_display_end();
                        return Ok(Some(Block::Equation {
                            starred: true,
                            par,
                            math: Box::new(math),
                        }));
                    } else {
                        return Err(self.err_span(
                            open_offset,
                            2,
                            "unclosed $$",
                            Some("display math started here"),
                            None,
                            Some("did you forget a closing '$$'?"),
                        ));
                    }
                }
                parse_para_or_continue!();
            }

            // A group opened or closed between paragraphs (`{\small\begin{center}..}`) only
            // scopes declarations: TeX stays in vertical mode, and no paragraph starts.
            if self.starts_with("{") {
                if let Some(close) = block_group_end(self.source_code, self.byte_offset) {
                    self.open_block_groups.push(close);
                    self.block_group_sizes.push((self.cur_size, self.math_params));
                    self.bump();
                    continue;
                }
            }
            if self.starts_with("}") && self.open_block_groups.last() == Some(&self.byte_offset) {
                self.open_block_groups.pop();
                self.bump();
                let (saved, math_params) = self.block_group_sizes.pop().unwrap_or_default();
                self.math_params = math_params;
                if saved != self.cur_size {
                    self.cur_size = saved;
                    return Ok(Some(Block::ConfigChange(ConfigChange::FontSize(saved.unwrap_or(FontSize::NormalSize)))));
                }
                continue;
            }
            if next_b != b'\\' {
                parse_para_or_continue!();
            }

            // Command or delimiter starting with '\\'
            if self.starts_with("\\[") {
                let open_offset = self.byte_offset;
                self.eat("\\[");
                let start = self.byte_offset;
                let rest = self.rest();
                if let Some(pos) = rest.find("\\]") {
                    let math_slice = &self.source_code[start..start + pos];
                    self.byte_offset = start + pos + 2;
                    let math = self.parse_math(math_slice);
                    let par = self.display_par(open_offset, true);
                    self.note_display_end();
                    return Ok(Some(Block::Equation {
                        starred: true,
                        par,
                        math: Box::new(math),
                    }));
                } else {
                    return Err(self.err_span(
                        open_offset,
                        2,
                        "unclosed \\[",
                        Some("display math started here"),
                        None,
                        Some("did you forget a closing '\\]'?"),
                    ));
                }
            }

            if let Some(cmd) = self.peek_control_word() {
                match cmd {
                    "item" | "end" | "includegraphics" | "caption" => return Ok(None),
                    "begin" => return Ok(Some(self.parse_env()?)),
                    "maketitle" => {
                        self.byte_offset += 1 + cmd.len();
                        // article.cls: `\setcounter{footnote}{0}` after the title.
                        self.footnote_counter = 0;
                        if let Some(title) = &self.preamble.title {
                            return Ok(Some(Block::TitleBlock(Box::new(TitleBlockData {
                                title: title.clone(),
                                author: self.preamble.author.clone(),
                                date: self.preamble.date.clone(),
                            }))));
                        }
                        continue;
                    }
                    // A `\label` whatsit in vertical mode (`expand_with_pages`).
                    "rushtexpagelabel" => {
                        self.byte_offset += 1 + cmd.len();
                        let id = self.brace_arg()?.trim().parse::<u16>().unwrap_or(u16::MAX);
                        return Ok(Some(Block::PageLabel(id)));
                    }
                    "markboth" | "markright" => {
                        self.byte_offset += 1 + cmd.len();
                        self.skip_arg_space();
                        let first = self.require_group()?;
                        let mark = if cmd == "markboth" {
                            self.skip_arg_space();
                            let right = self.require_group()?;
                            crate::frontend::ast::MarkData { left: Some(first), right }
                        } else {
                            crate::frontend::ast::MarkData { left: None, right: first }
                        };
                        return Ok(Some(Block::VMark(Some(Box::new(mark)))));
                    }
                    "pagestyle" | "thispagestyle" => {
                        self.byte_offset += 1 + cmd.len();
                        let style = self.brace_ident()?;
                        if let Some(style) = crate::frontend::ast::PageStyle::from_name(style) {
                            return Ok(Some(Block::ConfigChange(if cmd == "pagestyle" {
                                ConfigChange::PageStyle(style)
                            } else {
                                ConfigChange::ThisPageStyle(style)
                            })));
                        }
                        continue;
                    }
                    "appendix" => {
                        self.byte_offset += 1 + cmd.len();
                        return Ok(Some(Block::ConfigChange(ConfigChange::Appendix)));
                    }
                    "rushtexperiodsf" => {
                        self.byte_offset += 1 + cmd.len();
                        let on = self.brace_arg()?.trim() == "1";
                        return Ok(Some(Block::ConfigChange(ConfigChange::PeriodSf1000(on))));
                    }
                    "rushtexcolumnwrap" => {
                        self.byte_offset += 1 + cmd.len();
                        let items = |s: &str| {
                            s.split(',')
                                .filter_map(|t| {
                                    let (k, v) = t.split_at_checked(1)?;
                                    Some((k == "r", v.parse::<f32>().ok()?))
                                })
                                .collect::<Vec<_>>()
                        };
                        let left = items(self.brace_arg()?);
                        let right = items(self.brace_arg()?);
                        let wrap = crate::frontend::ast::ColumnWrap { left, right };
                        return Ok(Some(Block::ConfigChange(ConfigChange::ColumnWrap(Box::new(wrap)))));
                    }
                    "setlength" => {
                        self.byte_offset += 1 + cmd.len();
                        let param = self.macro_arg()?;
                        let val_str = self.brace_arg()?;
                        let key = param.trim().trim_start_matches('\\');
                        self.set_math_length(param, val_str, false);
                        if key == "parfillskip" {
                            if let Some((w, s)) = crate::doc_config::parse_glue_pt(val_str) {
                                return Ok(Some(Block::ConfigChange(ConfigChange::ParFillSkip { width: w, stretch: s })));
                            }
                        } else if key == "parskip" {
                            let (em, ex) = self.em_ex();
                            if let Some((w, st, sh)) = crate::doc_config::parse_glue3_pt_in(val_str, em, ex) {
                                return Ok(Some(Block::ConfigChange(ConfigChange::ParSkip(w, st, sh))));
                            }
                        } else if let Some(dim) = self.dim_pt(val_str) {
                            let block = match key {
                                "parindent" => Some(Block::ConfigChange(ConfigChange::ParIndent(dim))),
                                "hangindent" => Some(Block::ConfigChange(ConfigChange::HangIndent(dim))),
                                "hangafter" => Some(Block::ConfigChange(ConfigChange::HangAfter(dim as i32))),
                                "leftskip" => Some(Block::ConfigChange(ConfigChange::LeftSkip(dim))),
                                "rightskip" => Some(Block::ConfigChange(ConfigChange::RightSkip(dim))),
                                "labelsep" => Some(Block::ConfigChange(ConfigChange::LabelSep(dim))),
                                "leftmargin" | "leftmargini" => Some(Block::ConfigChange(ConfigChange::LeftMargin(dim))),
                                "itemindent" => Some(Block::ConfigChange(ConfigChange::ItemIndent(dim))),
                                "itemsep" => Some(Block::ConfigChange(ConfigChange::ItemSep(dim))),
                                "listparindent" => Some(Block::ConfigChange(ConfigChange::ListParIndent(dim))),
                                "pretolerance" => Some(Block::ConfigChange(ConfigChange::PreTolerance(dim as i32))),
                                "tolerance" => Some(Block::ConfigChange(ConfigChange::Tolerance(dim as i32))),
                                "emergencystretch" => Some(Block::ConfigChange(ConfigChange::EmergencyStretch(dim))),
                                "looseness" => Some(Block::ConfigChange(ConfigChange::Looseness(dim as i32))),
                                "prevgraf" => Some(Block::ConfigChange(ConfigChange::PrevGraf(dim as i32))),
                                "lastlinefit" => Some(Block::ConfigChange(ConfigChange::LastLineFit(dim as i32))),
                                _ => None,
                            };
                            if let Some(b) = block {
                                return Ok(Some(b));
                            }
                        }
                        continue;
                    }
                    "parfillskip" => {
                        if let Some((w, s)) = self.parse_raw_glue_command("\\parfillskip")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::ParFillSkip { width: w, stretch: s })));
                        }
                        continue;
                    }
                    "prevgraf" => {
                        if let Some(dim) = self.parse_raw_dimen_command("\\prevgraf")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::PrevGraf(dim as i32))));
                        }
                        continue;
                    }
                    "lastlinefit" => {
                        if let Some(dim) = self.parse_raw_dimen_command("\\lastlinefit")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::LastLineFit(dim as i32))));
                        }
                        continue;
                    }
                    "looseness" => {
                        if let Some(dim) = self.parse_raw_dimen_command("\\looseness")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::Looseness(dim as i32))));
                        }
                        continue;
                    }
                    "pretolerance" => {
                        if let Some(dim) = self.parse_raw_dimen_command("\\pretolerance")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::PreTolerance(dim as i32))));
                        }
                        continue;
                    }
                    "tolerance" => {
                        if let Some(dim) = self.parse_raw_dimen_command("\\tolerance")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::Tolerance(dim as i32))));
                        }
                        continue;
                    }
                    "emergencystretch" => {
                        if let Some(dim) = self.parse_raw_dimen_command("\\emergencystretch")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::EmergencyStretch(dim))));
                        }
                        continue;
                    }
                    "sloppy" => {
                        self.byte_offset += 1 + cmd.len();
                        return Ok(Some(Block::ConfigChange(ConfigChange::Sloppy)));
                    }
                    "centering" | "raggedright" | "raggedleft" => {
                        self.byte_offset += 1 + cmd.len();
                        return Ok(Some(Block::ConfigChange(ConfigChange::Align(align_of_declaration(cmd)))));
                    }
                    _ if FontSize::from_command(cmd).is_some() => {
                        self.byte_offset += 1 + cmd.len();
                        self.skip_hws();
                        let size = FontSize::from_command(cmd).unwrap_or(FontSize::NormalSize);
                        self.cur_size = Some(size);
                        return Ok(Some(Block::ConfigChange(ConfigChange::FontSize(size))));
                    }
                    "fussy" => {
                        self.byte_offset += 1 + cmd.len();
                        return Ok(Some(Block::ConfigChange(ConfigChange::Fussy)));
                    }
                    "hangindent" => {
                        if let Some(dim) = self.parse_raw_dimen_command("\\hangindent")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::HangIndent(dim))));
                        }
                        continue;
                    }
                    "hangafter" => {
                        if let Some(dim) = self.parse_raw_dimen_command("\\hangafter")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::HangAfter(dim as i32))));
                        }
                        continue;
                    }
                    "parshape" => {
                        self.byte_offset += 1 + cmd.len();
                        let shape = self.parse_parshape_args()?;
                        return Ok(Some(Block::ConfigChange(ConfigChange::ParShape(shape))));
                    }
                    "parindent" => {
                        if let Some(dim) = self.parse_raw_dimen_command("\\parindent")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::ParIndent(dim))));
                        }
                        continue;
                    }
                    "parskip" => {
                        if let Some(dim) = self.parse_raw_dimen_command("\\parskip")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::ParSkip(dim, 0.0, 0.0))));
                        }
                        continue;
                    }
                    "leftskip" => {
                        if let Some(dim) = self.parse_raw_dimen_command("\\leftskip")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::LeftSkip(dim))));
                        }
                        continue;
                    }
                    "rightskip" => {
                        if let Some(dim) = self.parse_raw_dimen_command("\\rightskip")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::RightSkip(dim))));
                        }
                        continue;
                    }
                    "labelsep" => {
                        if let Some(dim) = self.parse_raw_dimen_command("\\labelsep")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::LabelSep(dim))));
                        }
                        continue;
                    }
                    "leftmargin" => {
                        if let Some(dim) = self.parse_raw_dimen_command("\\leftmargin")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::LeftMargin(dim))));
                        }
                        continue;
                    }
                    "itemindent" => {
                        if let Some(dim) = self.parse_raw_dimen_command("\\itemindent")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::ItemIndent(dim))));
                        }
                        continue;
                    }
                    "itemsep" => {
                        if let Some(dim) = self.parse_raw_dimen_command("\\itemsep")? {
                            return Ok(Some(Block::ConfigChange(ConfigChange::ItemSep(dim))));
                        }
                        continue;
                    }
                    "newpage" | "clearpage" | "cleardoublepage" | "pagebreak" => {
                        self.byte_offset += 1 + cmd.len();
                        if cmd == "pagebreak" {
                            self.parse_optional_brackets()?;
                        } else {
                            // `\newpage` starts with `\par`, which `\@doendpe` made restore
                            // `\everypar{}`: the next paragraph is indented.
                            self.end_par_env = false;
                        }
                        return Ok(Some(Block::PageBreak));
                    }
                    "paragraph" | "subparagraph" => {
                        // LaTeX's run-in headings: `\@startsection` with a before-skip of
                        // 3.25ex plus 1ex minus .2ex, the title in `\bfseries`
                        // and `\hskip 1em` after it at the start of the next paragraph.
                        self.byte_offset += 1 + cmd.len();
                        let _ = self.eat("*");
                        self.parse_optional_brackets()?;
                        let title = self.require_group()?;
                        // `\subparagraph` keeps the paragraph indent.
                        let mut head = Vec::with_capacity(3);
                        if cmd == "paragraph" {
                            head.push(Inline::NoIndent);
                        }
                        head.push(Inline::Bold(title));
                        head.push(Inline::EmSpace { em: EM_1, breakable: true });
                        self.run_in_heading = Some(head);
                        return Ok(Some(Block::RunInHeadSkip { w: 3.25 * 4.30554, stretch: 4.30554, shrink: 0.2 * 4.30554 }));
                    }
                    "bigskip" => {
                        self.byte_offset += 1 + cmd.len();
                        return Ok(Some(Block::VSpace { w: 12.0, stretch: 4.0, shrink: 4.0 }));
                    }
                    "medskip" => {
                        self.byte_offset += 1 + cmd.len();
                        return Ok(Some(Block::VSpace { w: 6.0, stretch: 2.0, shrink: 2.0 }));
                    }
                    "smallskip" => {
                        self.byte_offset += 1 + cmd.len();
                        return Ok(Some(Block::VSpace { w: 3.0, stretch: 1.0, shrink: 1.0 }));
                    }
                    "hrule" => {
                        self.byte_offset += 1 + cmd.len();
                        return Ok(Some(Block::HRule));
                    }
                    "setcounter" => {
                        self.byte_offset += 1 + cmd.len();
                        self.parse_setcounter()?;
                        continue;
                    }
                    "vspace" => {
                        self.byte_offset += 1 + cmd.len();
                        let _ = self.eat("*");
                        let val_str = self.brace_arg()?;
                        let dim = self.dim_pt(val_str).unwrap_or(12.0);
                        return Ok(Some(Block::VSpace { w: dim, stretch: 0.0, shrink: 0.0 }));
                    }
                    "section" => {
                        self.byte_offset += 1 + cmd.len();
                        let starred = self.eat("*");
                        let short = self.optional_inlines("unclosed short title '['")?;
                        let title = with_short_title(short, self.require_group()?);
                        return Ok(Some(Block::Section {
                            level: 1,
                            starred,
                            numbered: !starred && 1 <= self.preamble.config.sec_num_depth,
                            title,
                        }));
                    }
                    "subsection" => {
                        self.byte_offset += 1 + cmd.len();
                        let starred = self.eat("*");
                        let short = self.optional_inlines("unclosed short title '['")?;
                        let title = with_short_title(short, self.require_group()?);
                        return Ok(Some(Block::Section {
                            level: 2,
                            starred,
                            numbered: !starred && 2 <= self.preamble.config.sec_num_depth,
                            title,
                        }));
                    }
                    "subsubsection" => {
                        self.byte_offset += 1 + cmd.len();
                        let starred = self.eat("*");
                        let short = self.optional_inlines("unclosed short title '['")?;
                        let title = with_short_title(short, self.require_group()?);
                        return Ok(Some(Block::Section {
                            level: 3,
                            starred,
                            numbered: !starred && 3 <= self.preamble.config.sec_num_depth,
                            title,
                        }));
                    }
                    "tableofcontents" => {
                        self.byte_offset += 1 + cmd.len();
                        return Ok(Some(Block::TableOfContents));
                    }
                    "listoffigures" => {
                        self.byte_offset += 1 + cmd.len();
                        return Ok(Some(Block::ListOfFigures));
                    }
                    "listoftables" => {
                        self.byte_offset += 1 + cmd.len();
                        return Ok(Some(Block::ListOfTables));
                    }
                    _ => {
                        parse_para_or_continue!();
                    }
                }
            } else {
                parse_para_or_continue!();
            }
        }
    }

    fn parse_blocks(&mut self) -> Result<Vec<Block<'a>>, ParseError> {
        let mut blocks = Vec::with_capacity(32);
        while let Some(b) = self.parse_next_block()? {
            blocks.push(b);
        }
        blocks.shrink_to_fit();
        Ok(blocks)
    }

    fn parse_env(&mut self) -> Result<Block<'a>, ParseError> {
        let env_start = self.byte_offset;
        self.eat("\\begin");
        let name = self.brace_ident()?;
        match name {
            "enumerate" => self.parse_list(ListKind::Enumerate, env_start),
            "itemize" => self.parse_list(ListKind::Itemize, env_start),
            "description" => self.parse_list(ListKind::Description, env_start),
            "quote" => self.parse_quote(QuoteKind::Quote, env_start),
            "quotation" => self.parse_quote(QuoteKind::Quotation, env_start),
            "verse" => self.parse_quote(QuoteKind::Verse, env_start),
            "center" => self.parse_aligned("center", ParAlign::Center, env_start),
            "flushleft" => self.parse_aligned("flushleft", ParAlign::RaggedRight, env_start),
            "flushright" => self.parse_aligned("flushright", ParAlign::RaggedLeft, env_start),
            // The expander's wrapper around an amsthm theorem.
            "rushtextheorem" => self.parse_aligned("rushtextheorem", ParAlign::Justified, env_start),
            "rushtextheoremhalf" => self.parse_aligned("rushtextheoremhalf", ParAlign::Justified, env_start),
            "rushtextheoremup" => self.parse_aligned("rushtextheoremup", ParAlign::Justified, env_start),
            "rushtexproof" => self.parse_aligned("rushtexproof", ParAlign::Justified, env_start),
            "tabular" => crate::tables::parse_tabular(self, env_start),
            "longtable" => crate::tables::parse_longtable(self, env_start),
            "table" => crate::tables::parse_table_float(self, env_start),
            "figure" => self.parse_figure(env_start),
            "verbatim" => self.parse_verbatim(env_start),
            "lstlisting" => self.parse_lstlisting(env_start),
            "equation" => self.parse_equation_env("equation", false, env_start),
            "equation*" => self.parse_equation_env("equation*", true, env_start),
            "displaymath" => self.parse_equation_env("displaymath", true, env_start),
            "align" => self.parse_ams_math_env("align", false, env_start),
            "align*" => self.parse_ams_math_env("align*", true, env_start),
            "alignat" => self.parse_ams_math_env("alignat", false, env_start),
            "alignat*" => self.parse_ams_math_env("alignat*", true, env_start),
            "gather" => self.parse_ams_math_env("gather", false, env_start),
            "gather*" => self.parse_ams_math_env("gather*", true, env_start),
            "multline" => self.parse_ams_math_env("multline", false, env_start),
            "multline*" => self.parse_ams_math_env("multline*", true, env_start),
            "split" => self.parse_ams_math_env("split", true, env_start),
            other if self.gaps.is_some() => {
                // Skip the environment's body: its contents are reported once it is supported.
                let end = format!("\\end{{{other}}}");
                self.byte_offset = self.rest().find(&end).map_or(self.source_code.len(), |p| self.byte_offset + p + end.len());
                if let Some(g) = self.gaps.as_mut() {
                    g.push((env_start as u32, "environment", other));
                }
                Ok(Block::Para(Vec::new()))
            }
            other => Err(self.err_span(
                env_start,
                "\\begin{".len() + other.len() + 1,
                format!("unsupported environment {{{other}}}"),
                Some("unsupported environment"),
                None,
                Some("supported environments: enumerate, itemize, description, quote, quotation, center, flushleft, flushright, tabular, longtable, table, figure, verbatim, lstlisting, equation, equation*, displaymath, align, align*, gather, gather*, multline, multline*"),
            )),
        }
    }

    fn parse_equation_env(
        &mut self,
        name: &str,
        starred: bool,
        open_offset: usize,
    ) -> Result<Block<'a>, ParseError> {
        let end_token = if name == "equation*" {
            "\\end{equation*}"
        } else if name == "equation" {
            "\\end{equation}"
        } else {
            "\\end{displaymath}"
        };
        let start = self.byte_offset;
        let rest = self.rest();
        if let Some(pos) = rest.find(end_token) {
            let math_slice = &self.source_code[start..start + pos];
            self.byte_offset = start + pos + end_token.len();
            let math = self.parse_math(math_slice);
            let par = self.display_par(open_offset, name == "displaymath");
            self.note_display_end();
            Ok(Block::Equation {
                starred,
                par,
                math: Box::new(math),
            })
        } else {
            let end_line = self.current_line_number();
            let note_str = format!(
                "reached end of file at line {end_line} without finding matching '{end_token}'"
            );
            Err(self.err_span(
                open_offset,
                format!("\\begin{{{name}}}").len(),
                format!("unclosed \\begin{{{name}}}"),
                Some("this environment is never closed"),
                Some(&note_str),
                Some("did you forget to close it?"),
            ))
        }
    }

    fn parse_ams_math_env(
        &mut self,
        name: &str,
        starred: bool,
        env_start: usize,
    ) -> Result<Block<'a>, ParseError> {
        let end_token = match name {
            "align" => "\\end{align}",
            "align*" => "\\end{align*}",
            "alignat" => "\\end{alignat}",
            "alignat*" => "\\end{alignat*}",
            "gather" => "\\end{gather}",
            "gather*" => "\\end{gather*}",
            "multline" => "\\end{multline}",
            "multline*" => "\\end{multline*}",
            "split" => "\\end{split}",
            _ => "\\end{align*}",
        };
        let rest = self.rest();
        if let Some(pos) = rest.find(end_token) {
            let full_end = self.byte_offset + pos + end_token.len();
            let math_slice = &self.source_code[env_start..full_end];
            self.byte_offset = full_end;
            let math = self.parse_math(math_slice);
            let par = self.display_par(env_start, false);
            self.note_display_end();
            Ok(Block::Equation {
                starred,
                par,
                math: Box::new(math),
            })
        } else {
            let end_line = self.current_line_number();
            let note_str = format!(
                "reached end of file at line {end_line} without finding matching '{end_token}'"
            );
            Err(self.err_span(
                env_start,
                "\\begin{".len() + name.len() + 1,
                format!("unclosed environment {{{name}}}"),
                Some("this environment is never closed"),
                Some(&note_str),
                Some("did you forget to close it?"),
            ))
        }
    }

    fn parse_verbatim(&mut self, open_offset: usize) -> Result<Block<'a>, ParseError> {
        if self.eat("\r\n") || self.eat("\n") {
            // strip single leading newline
        }
        let end_token = "\\end{verbatim}";
        let start = self.byte_offset;
        let rest = self.rest();
        if let Some(pos) = rest.find(end_token) {
            let code_slice = &self.source_code[start..start + pos];
            self.byte_offset = start + pos + end_token.len();
            let mut data = crate::code_listing::CodeListingData::new(code_slice);
            data.in_vmode = self.begins_in_vmode(open_offset);
            let class = self.preamble.class.as_ref().map_or("", |c| c.name);
            if crate::expand::classes::ltxguide_verbatim(class) {
                data.in_vmode = true;
                if matches!(self.cur_size, None | Some(FontSize::NormalSize)) {
                    data.options.font_size = FontSize::Small.metrics(self.preamble.config.base_font_size).0;
                }
            }
            self.note_env_end();
            Ok(Block::CodeListing(Box::new(data)))
        } else {
            let end_line = self.current_line_number();
            let note_str = format!("reached end of file at line {end_line} without finding matching '\\end{{verbatim}}'");
            Err(self.err_span(
                open_offset,
                "\\begin{verbatim}".len(),
                "unclosed \\begin{verbatim}",
                Some("this '\\begin{verbatim}' is never closed"),
                Some(&note_str),
                Some("did you forget to add '\\end{verbatim}'?"),
            ))
        }
    }

    fn parse_lstlisting(&mut self, open_offset: usize) -> Result<Block<'a>, ParseError> {
        self.skip_hws();
        let mut options = crate::code_listing::ListingOptions::default();
        if self.starts_with("[") {
            let opt_start = self.byte_offset;
            if let Some(pos) = self.rest().find(']') {
                let opt_str = &self.source_code[opt_start..opt_start + pos + 1];
                self.byte_offset = opt_start + pos + 1;
                options = crate::code_listing::parse_listing_options(opt_str);
            }
        }
        if self.eat("\r\n") || self.eat("\n") {
            // strip single leading newline
        }
        let end_token = "\\end{lstlisting}";
        let start = self.byte_offset;
        let rest = self.rest();
        if let Some(pos) = rest.find(end_token) {
            let code_slice = &self.source_code[start..start + pos];
            self.byte_offset = start + pos + end_token.len();
            // `listings` ends a display with `\@doendpe`, as LaTeX environments do.
            self.note_env_end();
            Ok(Block::CodeListing(Box::new(
                crate::code_listing::CodeListingData::with_options(code_slice, options),
            )))
        } else {
            let end_line = self.current_line_number();
            let note_str = format!("reached end of file at line {end_line} without finding matching '\\end{{lstlisting}}'");
            Err(self.err_span(
                open_offset,
                "\\begin{lstlisting}".len(),
                "unclosed \\begin{lstlisting}",
                Some("this '\\begin{lstlisting}' is never closed"),
                Some(&note_str),
                Some("did you forget to add '\\end{lstlisting}'?"),
            ))
        }
    }

    fn parse_raw_dimen_command(&mut self, cmd: &str) -> Result<Option<f32>, ParseError> {
        self.eat(cmd);
        self.skip_hws();
        let _ = self.eat("=");
        self.skip_hws();
        if self.starts_with("{") {
            let s = self.brace_arg()?;
            return Ok(self.dim_pt(s));
        }
        let start = self.byte_offset;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
        {
            self.bump();
        }
        let s = &self.source_code[start..self.byte_offset];
        Ok(self.dim_pt(s))
    }

    fn parse_raw_glue_command(&mut self, cmd: &str) -> Result<Option<(f32, f32)>, ParseError> {
        self.eat(cmd);
        self.skip_hws();
        let _ = self.eat("=");
        self.skip_hws();
        if self.starts_with("{") {
            let s = self.brace_arg()?;
            return Ok(crate::doc_config::parse_glue_pt(s));
        }
        let start = self.byte_offset;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
        {
            self.bump();
        }
        let s = &self.source_code[start..self.byte_offset];
        let width = match self.dim_pt(s) {
            Some(w) => w,
            None => return Ok(None),
        };
        self.skip_hws();
        let stretch = if self.starts_with("plus") {
            self.eat("plus");
            self.skip_hws();
            let plus_start = self.byte_offset;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
            {
                self.bump();
            }
            let plus_s = &self.source_code[plus_start..self.byte_offset];
            if plus_s.contains("fil") {
                f32::INFINITY
            } else {
                self.dim_pt(plus_s).unwrap_or(0.0)
            }
        } else {
            0.0
        };
        self.skip_hws();
        if self.starts_with("minus") {
            self.eat("minus");
            self.skip_hws();
            while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
            {
                self.bump();
            }
        }
        Ok(Some((width, stretch)))
    }

    fn parse_single_dimen_value(&mut self) -> Result<f32, ParseError> {
        self.skip_ws();
        if self.starts_with("{") {
            let s = self.brace_arg()?;
            let trimmed = s.trim();
            if trimmed.starts_with('\\') {
                return Ok(Self::resolve_dimen_macro(trimmed.trim_start_matches('\\')));
            }
            return Ok(self.dim_pt(trimmed).unwrap_or(0.0));
        }

        if self.peek() == Some('\\') {
            self.bump();
            let start = self.byte_offset;
            while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric()) {
                self.bump();
            }
            let name = &self.source_code[start..self.byte_offset];
            return Ok(Self::resolve_dimen_macro(name));
        }

        let start = self.byte_offset;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
        {
            self.bump();
        }
        let s = &self.source_code[start..self.byte_offset];
        Ok(self.dim_pt(s).unwrap_or(0.0))
    }

    fn resolve_dimen_macro(name: &str) -> f32 {
        match name {
            "hsize" | "linewidth" | "textwidth" => 468.0,
            "zero" | "empty" => 0.0,
            _ => 468.0,
        }
    }

    fn parse_parshape_args(&mut self) -> Result<Box<[(f32, f32)]>, ParseError> {
        self.skip_ws();
        let _ = self.eat("=");
        self.skip_ws();
        let n: usize = if self.starts_with("{") {
            let s = self.brace_arg()?;
            s.trim().parse().unwrap_or(0)
        } else {
            let start = self.byte_offset;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.bump();
            }
            let s = &self.source_code[start..self.byte_offset];
            s.trim().parse().unwrap_or(0)
        };

        if n == 0 {
            return Ok(Vec::new().into_boxed_slice());
        }

        let mut entries = Vec::with_capacity(n.min(32));
        for _ in 0..n {
            let indent = self.parse_single_dimen_value()?;
            let length = self.parse_single_dimen_value()?;
            entries.push((indent, length));
        }
        Ok(entries.into_boxed_slice())
    }

    fn parse_list(&mut self, kind: ListKind, open_offset: usize) -> Result<Block<'a>, ParseError> {
        let name = match kind {
            ListKind::Enumerate => "enumerate",
            ListKind::Itemize => "itemize",
            ListKind::Description => "description",
        };
        self.skip_hws();
        let mut options = ListOptions::default();
        if self.starts_with("[") {
            let opt_start = self.byte_offset;
            if let Some(pos) = self.rest().find(']') {
                let opt_str = &self.source_code[opt_start..opt_start + pos + 1];
                self.byte_offset = opt_start + pos + 1;
                options = parse_list_options(opt_str);
                if let Some(widest) = opt_str.strip_prefix("[rushtexbib=").and_then(|s| s.strip_suffix(']')) {
                    // `thebibliography`: `\labelwidth` is the width of `[widest]`.
                    let c = &self.preamble.config;
                    let mut label = String::with_capacity(widest.len() + 2);
                    label.push('[');
                    label.push_str(widest);
                    label.push(']');
                    let w = crate::backend::shape::word_width_pt(&label, crate::font_metrics::Face::Roman, c.base_font_size, c.font_family);
                    let sep = if c.list_label_sep.is_nan() {
                        0.5 * crate::backend::scaled::quad_pt(c.font_family, c.base_font_size)
                    } else {
                        c.list_label_sep
                    };
                    options.set_label_width(w);
                    options.set_left_margin(w + sep);
                }
            }
        }
        let mut items = Vec::with_capacity(8);
        loop {
            self.skip_ws();
            if self.eat_end_env(name) {
                break;
            }
            if self.peek().is_none() {
                let end_line = self.current_line_number();
                let span_len = "\\begin{".len() + name.len() + 1;
                let label_str = format!("this '\\begin{{{name}}}' is never closed");
                let note_str = format!("reached end of file at line {end_line} without finding matching '\\end{{{name}}}'");
                let help_str = format!("did you forget to add '\\end{{{name}}}'?");
                return Err(self.err_span(
                    open_offset,
                    span_len,
                    format!("unclosed \\begin{{{name}}}"),
                    Some(&label_str),
                    Some(&note_str),
                    Some(&help_str),
                ));
            }
            if self.starts_with("\\end{document}") {
                let doc_line = self.current_line_number();
                let span_len = "\\begin{".len() + name.len() + 1;
                let label_str = format!("this '\\begin{{{name}}}' is never closed");
                let note_str = format!("reached '\\end{{document}}' at line {doc_line} while '\\begin{{{name}}}' was still open");
                let help_str = format!("add '\\end{{{name}}}' before '\\end{{document}}'");
                return Err(self.err_span(
                    open_offset,
                    span_len,
                    format!("unclosed \\begin{{{name}}}"),
                    Some(&label_str),
                    Some(&note_str),
                    Some(&help_str),
                ));
            }
            if !self.at_item() {
                if self.starts_with("\\end{") {
                    let other_start = self.byte_offset;
                    let rest = self.rest();
                    let other_name = rest
                        .strip_prefix("\\end{")
                        .and_then(|s| s.split('}').next())
                        .unwrap_or("unknown");
                    let label_str = format!("expected '\\end{{{name}}}'");
                    let note_str = format!("'\\begin{{{name}}}' was opened earlier");
                    let help_str = format!("change '\\end{{{other_name}}}' to '\\end{{{name}}}' or close nested environments");
                    return Err(self.err_span(
                        other_start,
                        "\\end{".len() + other_name.len() + 1,
                        format!("mismatched environment: expected \\end{{{name}}}, found \\end{{{other_name}}}"),
                        Some(&label_str),
                        Some(&note_str),
                        Some(&help_str),
                    ));
                }
                let label_str = format!("expected '\\item' or '\\end{{{name}}}'");
                let help_str = format!("items in '{name}' must start with '\\item'");
                return Err(self.err_span(
                    self.byte_offset,
                    1,
                    format!("expected \\item or \\end{{{name}}}, found something else"),
                    Some(&label_str),
                    None,
                    Some(&help_str),
                ));
            }
            self.eat("\\item");
            self.skip_hws();
            let custom_label = self.optional_inlines("unclosed custom item label '['")?;
            let blocks = self.parse_blocks()?;
            items.push(ListItem {
                custom_label,
                blocks,
            });
        }
        items.shrink_to_fit();
        self.note_env_end();
        Ok(Block::List(Box::new(ListData {
            kind,
            options,
            items,
            in_vmode: self.begins_in_vmode(open_offset),
        })))
    }

    fn parse_quote(
        &mut self,
        kind: QuoteKind,
        open_offset: usize,
    ) -> Result<Block<'a>, ParseError> {
        let name = match kind {
            QuoteKind::Quote => "quote",
            QuoteKind::Quotation => "quotation",
            QuoteKind::Verse => "verse",
        };
        let in_vmode = self.begins_in_vmode(open_offset);
        self.skip_ws();
        let blocks = self.parse_blocks()?;
        if !self.eat_end_env(name) {
            let end_line = self.current_line_number();
            let span_len = "\\begin{".len() + name.len() + 1;
            return Err(self.err_span(
                open_offset,
                span_len,
                format!("unclosed \\begin{{{name}}}"),
                Some(&format!("this '\\begin{{{name}}}' is never closed")),
                Some(&format!("reached end of file at line {end_line} without finding matching '\\end{{{name}}}'")),
                Some(&format!("did you forget to add '\\end{{{name}}}'?")),
            ));
        }
        self.note_env_end();
        Ok(Block::Quote(Box::new(QuoteData {
            kind,
            in_vmode,
            blocks,
        })))
    }

    fn parse_aligned(
        &mut self,
        name: &'static str,
        align: ParAlign,
        open_offset: usize,
    ) -> Result<Block<'a>, ParseError> {
        let in_vmode = self.begins_in_vmode(open_offset);
        self.skip_ws();
        let blocks = self.parse_blocks()?;
        if !self.eat_end_env(name) {
            let end_line = self.current_line_number();
            let span_len = "\\begin{".len() + name.len() + 1;
            return Err(self.err_span(
                open_offset,
                span_len,
                format!("unclosed \\begin{{{name}}}"),
                Some(&format!("this '\\begin{{{name}}}' is never closed")),
                Some(&format!("reached end of file at line {end_line} without finding matching '\\end{{{name}}}'")),
                Some(&format!("did you forget to add '\\end{{{name}}}'?")),
            ));
        }
        self.note_env_end();
        let kind = match name {
            "rushtextheorem" => TrivlistKind::Theorem { half_skip: false, italic: true },
            "rushtextheoremup" => TrivlistKind::Theorem { half_skip: false, italic: false },
            "rushtextheoremhalf" => TrivlistKind::Theorem { half_skip: true, italic: false },
            "rushtexproof" => TrivlistKind::Proof,
            _ => TrivlistKind::Environment,
        };
        Ok(Block::Aligned(Box::new(AlignedData { align, in_vmode, kind, blocks })))
    }

    /// Whether the source before `offset` leaves TeX in vertical mode: a blank line, the
    /// end of another environment, or `\begin{document}` precedes it rather than paragraph
    /// text.
    fn begins_in_vmode(&self, offset: usize) -> bool {
        /// latex.ltx's environments built on `\trivlist` or `\list`.
        const TRIVLIST_ENVS: [&str; 13] = [
            "center", "flushleft", "flushright", "quote", "quotation", "verse", "itemize", "enumerate",
            "description", "list", "trivlist", "verbatim", "thebibliography",
        ];
        let mut before = self.source_code[..offset.min(self.source_code.len())].trim_end_matches([' ', '\t']);
        // A group's `{` and font or size declarations typeset nothing: TeX stays in the mode
        // it was in (`{\small\begin{center}` after a blank line is in vertical mode).
        loop {
            let b = before.trim_end_matches([' ', '\t']);
            if let Some(r) = b.strip_suffix('{') {
                before = r;
                continue;
            }
            let cs_start = b.rfind('\\').filter(|&i| b[i + 1..].bytes().all(|c| c.is_ascii_alphabetic()) && i + 1 < b.len());
            match cs_start {
                Some(i) if VMODE_NEUTRAL_DECLS.contains(&&b[i + 1..]) => before = &b[..i],
                _ => {
                    before = b;
                    break;
                }
            }
        }
        // `\endtrivlist` ends with `\par` (`\@endparenv`): after these environments TeX is
        // in vertical mode wherever their `\end` stands.
        if let Some(name) = before.strip_suffix('}').and_then(|b| b.rsplit_once("\\end{")).map(|(_, n)| n) {
            if TRIVLIST_ENVS.contains(&name.trim_end_matches('*')) {
                return true;
            }
        }
        let Some(head) = before.strip_suffix('\n') else {
            return before.trim_end().is_empty();
        };
        let head = head.trim_end_matches([' ', '\t', '\r']);
        if head.ends_with('\n') || head.is_empty() {
            return true;
        }
        let last_line = head.rsplit('\n').next().unwrap_or("").trim_start();
        // Headings (not the run-in `\paragraph`), `\maketitle` and page breaks leave TeX in
        // vertical mode.
        const VMODE_CMDS: [&str; 11] = [
            "\\end{",
            "\\begin{document}",
            "\\par",
            "\\part",
            "\\chapter",
            "\\section",
            "\\subsection",
            "\\subsubsection",
            "\\maketitle",
            "\\newpage",
            "\\clearpage",
        ];
        // A display leaves TeX in horizontal mode, continuing its paragraph (tex.web §1200).
        if ends_display(last_line) {
            return false;
        }
        VMODE_CMDS.iter().any(|c| last_line.starts_with(c)) && !last_line.starts_with("\\paragraph")
    }

    /// Records whether a blank line follows the `\end{...}` just read (`\@endparenv`).
    /// Where a display opening at `offset` stands: `latex_bracket` for latex.ltx's `\[`,
    /// which amsmath replaces with `equation*` (a plain `$$`).
    fn display_par(&self, offset: usize, latex_bracket: bool) -> crate::frontend::ast::DisplayPar {
        use crate::frontend::ast::DisplayPar;
        if !self.begins_in_vmode(offset) {
            DisplayPar::Continues
        } else if latex_bracket && !self.amsmath_loaded() {
            DisplayPar::StartsWithBox
        } else if self.end_par_env {
            DisplayPar::StartsEmpty
        } else {
            DisplayPar::Starts
        }
    }

    /// `\thinspace` in em (2^-16): latex.ltx's `.16667em`, amsmath's `.1667em`.
    fn thin_em(&self) -> i32 {
        if self.amsmath_loaded() { crate::backend::scaled::round_decimals(b"1667") } else { EM_16667 }
    }

    fn amsmath_loaded(&self) -> bool {
        self.preamble.packages.iter().any(|p| matches!(p.name, "amsmath" | "mathtools"))
            || self.preamble.class.as_ref().is_some_and(|c| matches!(c.name, "amsart" | "amsbook" | "amsproc"))
    }

    /// After a display: text before the next blank line resumes the paragraph (§1200).
    fn note_display_end(&mut self) {
        self.note_env_end();
        self.display_resumes = std::mem::take(&mut self.end_par_env);
    }

    /// `\setlength`/`\addtolength` of a math length the parser records (`\multlinegap`).
    fn set_math_length(&mut self, reg: &str, value: &str, add: bool) {
        if reg.trim().trim_start_matches('\\') == "multlinegap" {
            if let Some(v) = self.dim_pt(value) {
                let gap = &mut self.math_params.multline_gap;
                *gap = if add { *gap + v } else { v };
            }
        }
    }

    fn note_env_end(&mut self) {
        // TeX's input states: an end of line in state N (a line start, also after a
        // comment, which takes its line end with it) is `\par`.
        let mut rest = self.rest();
        let mut line_start = false;
        let blank = loop {
            rest = rest.trim_start_matches([' ', '\t', '\r']);
            if let Some(r) = rest.strip_prefix('%') {
                rest = r.split_once('\n').map_or("", |(_, t)| t);
                line_start = true;
            } else if let Some(r) = rest.strip_prefix('\n') {
                if line_start {
                    break true;
                }
                rest = r;
                line_start = true;
            } else {
                break false;
            }
        };
        self.end_par_env = !blank;
    }

    fn parse_figure(&mut self, open_offset: usize) -> Result<Block<'a>, ParseError> {
        self.skip_hws();
        let mut spec = "";
        if self.eat("[") {
            let start = self.byte_offset;
            while !matches!(self.peek(), Some(']') | None) {
                self.bump();
            }
            spec = &self.source_code[start..self.byte_offset];
            self.eat("]");
        }
        let placement = crate::tables::tables_model::parse_float_placement(spec);
        let mut centered = false;
        let mut graphic = None;
        let mut caption = Vec::with_capacity(8);
        let mut vskip = 0.0;
        loop {
            self.skip_ws();
            if self.eat_end_env("figure") {
                break;
            }
            if self.peek().is_none() {
                let end_line = self.current_line_number();
                let note_str = format!("reached end of file at line {end_line} without finding matching '\\end{{figure}}'");
                return Err(self.err_span(
                    open_offset,
                    "\\begin{figure}".len(),
                    "unclosed \\begin{figure}",
                    Some("this '\\begin{figure}' is never closed"),
                    Some(&note_str),
                    Some("did you forget to add '\\end{figure}'?"),
                ));
            }
            if self.starts_with("\\end{document}") {
                let doc_line = self.current_line_number();
                let note_str = format!("reached '\\end{{document}}' at line {doc_line} while '\\begin{{figure}}' was still open");
                return Err(self.err_span(
                    open_offset,
                    "\\begin{figure}".len(),
                    "unclosed \\begin{figure}",
                    Some("this '\\begin{figure}' is never closed"),
                    Some(&note_str),
                    Some("add '\\end{figure}' before '\\end{document}'"),
                ));
            }
            if self.cmd_ends_at("\\includegraphics") {
                self.eat("\\includegraphics");
                graphic = Some(self.parse_graphic()?);
                continue;
            }
            if self.cmd_ends_at("\\caption") {
                self.eat("\\caption");
                // The short (list-of-figures) form is not used by the float itself.
                self.parse_optional_brackets()?;
                caption = self.require_group()?;
                continue;
            }
            if self.cmd_ends_at("\\centering") {
                self.eat("\\centering");
                centered = true;
                continue;
            }
            if self.cmd_ends_at("\\vspace") {
                self.eat("\\vspace");
                let _ = self.eat("*");
                let val_str = self.brace_arg()?;
                vskip += self.dim_pt(val_str).unwrap_or(0.0);
                continue;
            }
            // ignore extra paragraph text inside figure for v1
            let before = self.byte_offset;
            let _ = self.parse_para_inlines()?;
            if self.byte_offset == before {
                // A vertical-mode command the figure does not model: skip its name.
                self.bump();
                while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                    self.bump();
                }
            }
        }
        Ok(Block::Figure(Box::new(FigureData { graphic, caption, placement, centered, vskip })))
    }

    fn parse_graphic(&mut self) -> Result<Graphic<'a>, ParseError> {
        self.skip_hws();
        let mut options = "";
        if self.eat("[") {
            let opt_start = self.byte_offset - 1;
            while !matches!(self.peek(), Some(']') | None) {
                self.bump();
            }
            options = &self.source_code[opt_start + 1..self.byte_offset];
            if !self.eat("]") {
                return Err(self.err_span(
                    opt_start,
                    1,
                    "unclosed \\includegraphics optional arg",
                    Some("opening '[' starts here"),
                    Some("reached end of file without closing ']'"),
                    Some("add closing ']' to terminate optional arguments"),
                ));
            }
        }
        self.skip_hws();
        let open_offset = self.byte_offset;
        if !self.eat("{") {
            return Err(self.err_span(
                open_offset,
                1,
                "expected {path} after \\includegraphics",
                Some("expected '{path}' here"),
                None,
                Some("provide an image path in braces, e.g. '\\includegraphics{image.png}'"),
            ));
        }
        let start = self.byte_offset;
        while !matches!(self.peek(), Some('}') | None) {
            self.bump();
        }
        let path = self.source_code[start..self.byte_offset].trim();
        if !self.eat("}") {
            return Err(self.err_span(
                open_offset,
                1,
                "unclosed \\includegraphics path",
                Some("path opened here"),
                Some("reached end of file without closing '}'"),
                Some("add closing '}' to terminate image path"),
            ));
        }
        Ok(Graphic { path, options })
    }

    /// The raw text of a `{name, name, …}` list, as `\usepackage` takes it.
    fn brace_name_list(&mut self) -> Result<&'a str, ParseError> {
        self.skip_ws();
        let open_offset = self.byte_offset;
        if !self.eat("{") {
            return self.brace_ident();
        }
        let start = self.byte_offset;
        while let Some(c) = self.peek() {
            if c == '}' {
                let list = &self.source_code[start..self.byte_offset];
                self.bump();
                return Ok(list);
            }
            if c == '{' || c == '\\' {
                break;
            }
            self.bump();
        }
        Err(self.err_span(
            open_offset,
            1,
            "invalid or unclosed name",
            Some("name opened here"),
            None,
            Some("names should be separated by commas and end with '}'"),
        ))
    }

    fn brace_ident(&mut self) -> Result<&'a str, ParseError> {
        self.skip_hws();
        let open_offset = self.byte_offset;
        if !self.eat("{") {
            return Err(self.err_span(
                open_offset,
                1,
                "expected {name}",
                Some("expected '{name}' here"),
                None,
                Some("provide a name inside braces, e.g. '{myname}'"),
            ));
        }
        let start = self.byte_offset;
        loop {
            match self.peek() {
                Some('}') => {
                    let ident = &self.source_code[start..self.byte_offset];
                    self.bump();
                    return Ok(ident);
                }
                Some(c) if c.is_ascii_alphabetic() || c == '*' => {
                    self.bump();
                }
                _ => {
                    return Err(self.err_span(
                        open_offset,
                        1,
                        "invalid or unclosed name",
                        Some("name opened here"),
                        None,
                        Some("names should contain only alphanumeric characters or '*' and end with '}'"),
                    ));
                }
            }
        }
    }

    /// An undelimited macro argument: a braced group's contents, or a single token (a
    /// control sequence such as `\leftmargini`, or one character).
    pub(crate) fn macro_arg(&mut self) -> Result<&'a str, ParseError> {
        self.skip_arg_space();
        let rest = self.rest();
        if rest.starts_with('{') {
            return self.brace_arg();
        }
        let start = self.byte_offset;
        let bytes = rest.as_bytes();
        let len = match bytes.first() {
            None => return self.brace_arg(),
            Some(b'\\') => {
                let n = bytes[1..].iter().take_while(|b| b.is_ascii_alphabetic()).count();
                1 + n.max(1).min(bytes.len() - 1)
            }
            Some(_) => rest.chars().next().map_or(1, char::len_utf8),
        };
        self.byte_offset += len;
        Ok(&self.source_code[start..start + len])
    }

    pub(crate) fn brace_arg(&mut self) -> Result<&'a str, ParseError> {
        self.skip_arg_space();
        let open_offset = self.byte_offset;
        if !self.eat("{") {
            return Err(self.err_span(
                open_offset,
                1,
                "expected {arg}",
                Some("expected '{arg}' here"),
                None,
                Some("enclose argument in braces, e.g. '{...}'"),
            ));
        }
        let start = self.byte_offset;
        let mut depth = 0u32;
        loop {
            match self.peek() {
                Some('}') if depth == 0 => {
                    let arg = &self.source_code[start..self.byte_offset];
                    self.bump();
                    return Ok(arg.trim());
                }
                Some('}') => {
                    depth -= 1;
                    self.bump();
                }
                Some('{') => {
                    depth += 1;
                    self.bump();
                }
                Some('\\') => {
                    // `\{`, `\}` and `\\` are no group delimiters.
                    self.bump();
                    if self.peek().is_some() {
                        self.bump();
                    }
                }
                Some(_) => {
                    self.bump();
                }
                None => {
                    return Err(self.err_span(
                        open_offset,
                        1,
                        "unclosed brace",
                        Some("opening '{' starts here"),
                        Some("reached end of file without finding matching '}'"),
                        Some("did you forget a closing '}'?"),
                    ));
                }
            }
        }
    }

    fn parse_para_inlines(&mut self) -> Result<Vec<Inline<'a>>, ParseError> {
        let mut out = Vec::with_capacity(8);
        loop {
            match self.peek() {
                None => break,
                Some('%') => self.skip_comment(),
                Some('\n') if self.at_blank_line() => break,
                Some('$') => {
                    if self.starts_with("$$") {
                        break;
                    }
                    out.push(self.parse_inline()?);
                }
                Some('\\') => {
                    let bytes = self.rest().as_bytes();
                    if bytes.len() >= 2 && bytes[1].is_ascii_lowercase() {
                        const TERM_MASK: u32 = (1 << (b'b' - b'a'))
                            | (1 << (b'c' - b'a'))
                            | (1 << (b'e' - b'a'))
                            | (1 << (b'h' - b'a'))
                            | (1 << (b'i' - b'a'))
                            | (1 << (b'l' - b'a'))
                            | (1 << (b'm' - b'a'))
                            | (1 << (b'n' - b'a'))
                            | (1 << (b'p' - b'a'))
                            | (1 << (b's' - b'a'))
                            | (1 << (b't' - b'a'))
                            | (1 << (b'v' - b'a'));
                        let shift = bytes[1] - b'a';
                        if shift < 26 && ((TERM_MASK & (1 << shift)) != 0) {
                            if let Some(cmd) = self.peek_control_word() {
                                let is_terminator = matches!(
                                    cmd,
                                    "begin" | "bigskip" | "end" | "item" | "section" | "subsection"
                                        | "subsubsection" | "smallskip" | "maketitle" | "medskip"
                                        | "newpage" | "clearpage" | "hrule" | "tableofcontents"
                                        | "listoffigures" | "listoftables" | "vspace" | "paragraph"
                                        | "subparagraph" | "pagebreak" | "cleardoublepage"
                                );
                                if is_terminator {
                                    break;
                                }
                            }
                        }
                    } else if self.starts_with("\\[") {
                        break;
                    }
                    if self.starts_with("\\\\") {
                        self.eat("\\\\");
                        out.push(Inline::Break);
                        self.skip_arg_space();
                    } else {
                        out.push(self.parse_inline()?);
                    }
                }
                Some(_) => out.push(self.parse_inline()?),
            }
        }
        if !out.is_empty() && self.at_display_start() {
            out.push(Inline::DisplayFollows);
        }
        out.shrink_to_fit();
        Ok(out)
    }

    /// Whether a display starts here: `$$`, `\[` or a display math environment.
    fn at_display_start(&self) -> bool {
        let rest = self.rest();
        if rest.starts_with("$$") || rest.starts_with("\\[") {
            return true;
        }
        let Some(env) = rest.strip_prefix("\\begin{") else { return false };
        let name = env.split('}').next().unwrap_or("");
        matches!(
            name.trim_end_matches('*'),
            "equation" | "align" | "alignat" | "gather" | "multline" | "flalign" | "eqnarray" | "displaymath"
        )
    }

    /// The letter after a text accent command, `o` or `{o}` (`\\i` for a dotless i), as
    /// its precomposed character.
    fn parse_accent_base(&mut self, accent: char) -> Option<&'static str> {
        let save = self.byte_offset;
        let braced = self.eat("{");
        if braced {
            self.skip_hws();
        }
        let base = if self.eat("\\i") {
            'i'
        } else if self.eat("\\j") {
            'j'
        } else {
            let c = self.peek()?;
            self.bump();
            c
        };
        if braced {
            self.skip_hws();
            if !self.eat("}") {
                self.byte_offset = save;
                return None;
            }
        }
        let letter = compose_accent(accent, base);
        if letter.is_none() {
            self.byte_offset = save;
        }
        letter
    }

    pub(crate) fn parse_inline(&mut self) -> Result<Inline<'a>, ParseError> {
        match self.peek() {
            Some('\\') => self.parse_command(),
            Some('$') => {
                let open_offset = self.byte_offset;
                self.bump();
                let start = self.byte_offset;
                let rest = self.rest();
                if let Some(pos) = find_unescaped_char(rest, '$') {
                    let math_slice = &self.source_code[start..start + pos];
                    self.byte_offset = start + pos + 1;
                    let math = self.parse_math(math_slice);
                    Ok(Inline::Math(Box::new(math)))
                } else {
                    Err(self.err_span(
                        open_offset,
                        1,
                        "unclosed $",
                        Some("inline math started here"),
                        None,
                        Some("did you forget a closing '$'?"),
                    ))
                }
            }
            Some('{') => {
                if let Some(close) = block_group_end(self.source_code, self.byte_offset) {
                    self.open_block_groups.push(close);
                    self.block_group_sizes.push((self.cur_size, self.math_params));
                    self.bump();
                    return Ok(Inline::Text(""));
                }
                self.bump();
                Ok(Inline::Group(self.parse_until_rbrace()?))
            }
            Some('}') if self.open_block_groups.last() == Some(&self.byte_offset) => {
                self.open_block_groups.pop();
                self.bump();
                // The group's end undoes a size declared in it, possibly blocks earlier.
                let (saved, math_params) = self.block_group_sizes.pop().unwrap_or_default();
                self.math_params = math_params;
                if saved != self.cur_size {
                    self.cur_size = saved;
                    return Ok(Inline::FontSize(saved.unwrap_or(FontSize::NormalSize)));
                }
                Ok(Inline::Text(""))
            }
            Some('~') => {
                self.bump();
                Ok(Inline::Text(crate::layout::engine::TIE_TEXT))
            }
            Some('&') => {
                // alignment tab belongs to tabular; if it leaks here, keep it
                // as a one-char token so we cannot spin on an empty text run.
                let start = self.byte_offset;
                self.bump();
                Ok(Inline::Text(&self.source_code[start..self.byte_offset]))
            }
            Some(_) => {
                let text = self.parse_text_run();
                if !text.is_empty() {
                    return Ok(Inline::Text(text));
                }
                // A text run stops only before a blank line here (the paragraph loop ends
                // the paragraph there first). Inside a group the blank line is TeX's \par,
                // which the text commands accept as LaTeX's are \long.
                if self.at_blank_line() {
                    while matches!(self.peek(), Some('\n' | '\r' | ' ' | '\t')) {
                        self.bump();
                    }
                    return Ok(Inline::ParEnd);
                }
                Err(self.err("unexpected character"))
            }
            None => Err(self.err("unexpected eof")),
        }
    }

    fn parse_until_rbrace(&mut self) -> Result<Vec<Inline<'a>>, ParseError> {
        self.inline_depth += 1;
        let math_params = self.math_params;
        let r = self.parse_until_rbrace_inner();
        self.math_params = math_params;
        self.inline_depth -= 1;
        r
    }

    fn parse_until_rbrace_inner(&mut self) -> Result<Vec<Inline<'a>>, ParseError> {
        let open_offset = self.byte_offset.saturating_sub(1);
        let mut inner = Vec::with_capacity(1);
        loop {
            match self.peek() {
                Some('}') => {
                    self.bump();
                    break;
                }
                None => {
                    return Err(self.err_span(
                        open_offset,
                        1,
                        "unclosed {",
                        Some("opening '{' starts here"),
                        Some("reached end of file without finding matching '}'"),
                        Some("did you forget a closing '}'?"),
                    ));
                }
                Some('%') => self.skip_comment(),
                Some(_) if self.starts_with("\\begin") || self.at_item() => {
                    return Err(self.err("lists are not allowed inside a group"));
                }
                Some(_) => inner.push(self.parse_inline()?),
            }
        }
        inner.shrink_to_fit();
        Ok(inner)
    }

    fn parse_command(&mut self) -> Result<Inline<'a>, ParseError> {
        let cmd_start = self.byte_offset;
        self.bump(); // '\'
        if let Some(c) = self.peek() {
            if c == '(' {
                let open_offset = self.byte_offset - 1;
                self.bump();
                let start = self.byte_offset;
                let rest = self.rest();
                if let Some(pos) = rest.find("\\)") {
                    let math_slice = &self.source_code[start..start + pos];
                    self.byte_offset = start + pos + 2;
                    let math = self.parse_math(math_slice);
                    return Ok(Inline::Math(Box::new(math)));
                } else {
                    return Err(self.err_span(
                        open_offset,
                        2,
                        "unclosed \\(",
                        Some("inline math started here"),
                        None,
                        Some("did you forget a closing '\\)'?"),
                    ));
                }
            }
            if c == '[' {
                let open_offset = self.byte_offset - 1;
                self.bump();
                let start = self.byte_offset;
                let rest = self.rest();
                if let Some(pos) = rest.find("\\]") {
                    let math_slice = &self.source_code[start..start + pos];
                    self.byte_offset = start + pos + 2;
                    let math = self.parse_math(math_slice);
                    return Ok(Inline::Math(Box::new(math)));
                } else {
                    return Err(self.err_span(
                        open_offset,
                        2,
                        "unclosed \\[",
                        Some("display math started here"),
                        None,
                        Some("did you forget a closing '\\]'?"),
                    ));
                }
            }
            if c == '-' {
                self.bump();
                return Ok(Inline::Text("\u{00AD}"));
            }
            if matches!(c, ' ' | '\t' | '\n' | '\r') {
                // Control space: one space, then TeX is in state S: the blanks after it and
                // the end of the line are skipped.
                // A blank line after it still ends the paragraph.
                if c != '\n' {
                    self.bump();
                    self.skip_hws();
                }
                let rest = self.rest();
                let after_eol = rest.strip_prefix("\r\n").or_else(|| rest.strip_prefix('\n'));
                if let Some(next) = after_eol {
                    if !next.trim_start_matches([' ', '\t', '\r']).starts_with('\n') {
                        self.byte_offset += rest.len() - next.len();
                        self.skip_hws();
                    }
                }
                return Ok(Inline::Text(crate::layout::engine::CONTROL_SPACE_TEXT));
            }
            if matches!(c, '"' | '\'' | '`' | '^' | '~' | '=' | '.') {
                let c_at = self.byte_offset;
                self.bump();
                if let Some(letter) = self.parse_accent_base(c) {
                    return Ok(Inline::Text(letter));
                }
                if is_single_escape(c) {
                    // `\~{}` and `\^{}`: the accent alone.
                    let _ = self.eat("{}");
                    return Ok(Inline::Text(&self.source_code[c_at..c_at + 1]));
                }
                return Err(self.err_span(cmd_start, 2, format!("accent '\\{c}' needs a letter"), None, None, Some("write it as \\\"{o}")));
            }
            if c == '{' || c == '}' {
                // `\{` in text is `\textbraceleft`: OT1 has no braces, LaTeX takes cmsy's.
                self.bump();
                return Ok(Inline::Math(Box::new(self.parse_math(if c == '{' { "\\{" } else { "\\}" }))));
            }
            if c == '_' && self.preamble.config.font_family == crate::font_metrics::FontFamily::ComputerModern {
                // `\_` is `\textunderscore`: OT1 has no underscore glyph (latex.ltx draws a rule).
                // The Base-14 families have the glyph.
                self.bump();
                return Ok(Inline::Text(crate::layout::engine::UNDERSCORE_TEXT));
            }
            if c == '\\' {
                // `\\`, `\\*` and `\\[len]` inside a group (`\title{A\\B}`): a line break.
                self.bump();
                let _ = self.eat("*");
                self.parse_optional_brackets()?;
                // `\@ifnextchar` looks past spaces and one end of line (not a `\par`).
                self.skip_arg_space();
                return Ok(Inline::Break);
            }
            // latex.ltx: `\$` is `\textdollar`, a TS1 symbol since LaTeX 2020.
            if c == '$' && self.preamble.config.font_family == crate::font_metrics::FontFamily::ComputerModern {
                self.bump();
                return Ok(Inline::Logo(crate::frontend::ast::LogoKind::Ts1(36)));
            }
            if is_single_escape(c) {
                let start = self.byte_offset;
                self.bump();
                return Ok(Inline::Text(&self.source_code[start..self.byte_offset]));
            }
            // LaTeX's text-mode spacing kerns (`\,` = `\thinspace`, `\:` `\;` `\!`), `\@`
            // (space factor 1000) and `\/` (the italic correction).
            let kern = match c {
                '@' => {
                    self.bump();
                    return Ok(Inline::SpaceFactor(1000));
                }
                '/' => {
                    self.bump();
                    return Ok(Inline::ItalicCorrection);
                }
                // latex.ltx `\tmspace`: `\kern.16667em`, `.2222em`, `.2777em` in text;
                // amsmath's `\,` and `\!` are `.1667em`.
                ',' => Some(self.thin_em()),
                ':' => Some(EM_2222),
                ';' => Some(EM_2777),
                '!' => Some(-self.thin_em()),
                _ => None,
            };
            if let Some(em) = kern {
                self.bump();
                return Ok(Inline::EmSpace { em, breakable: false });
            }
        }
        let bytes = self.source_code.as_bytes();
        let start = self.byte_offset;
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_alphabetic() {
            end += 1;
        }
        self.byte_offset = end;
        let name = &self.source_code[start..end];
        if name.is_empty() {
            return Err(self.err_span(
                cmd_start,
                1,
                "stray '\\'",
                Some("stray backslash with no command name"),
                None,
                Some("escape it with '\\\\' if you want a backslash"),
            ));
        }
        if let [accent @ (b'u' | b'v' | b'H' | b'c' | b'r' | b'k')] = name.as_bytes() {
            self.skip_hws();
            if let Some(letter) = self.parse_accent_base(*accent as char) {
                return Ok(Inline::Text(letter));
            }
            return Err(self.err_span(cmd_start, 2, format!("accent '\\{name}' needs a letter"), None, None, None));
        }
        // After a control word TeX is in state S: blanks and the end of the line are
        // skipped (§354), but a blank line still ends the paragraph.
        self.skip_arg_space();
        // Fast-path for ubiquitous formatting commands (instruction-cache and branch predictor optimal)
        match name.as_bytes() {
            b"textbf" => return Ok(Inline::Group(vec![Inline::TextFontCommand, Inline::Bold(self.require_group()?)])),
            b"textit" => return Ok(Inline::Italic(self.require_group()?)),
            b"emph" => {
                // `\em`: upright inside italic text.
                let mut body = self.require_group()?;
                body.splice(0..0, [Inline::TextFontCommand, Inline::FontDecl(FontDecl::Em)]);
                return Ok(Inline::Group(body));
            }
            b"bfseries" => return Ok(Inline::FontDecl(FontDecl::Bold)),
            b"mdseries" => return Ok(Inline::FontDecl(FontDecl::Medium)),
            b"itshape" => return Ok(Inline::FontDecl(FontDecl::Italic)),
            b"slshape" => return Ok(Inline::FontDecl(FontDecl::Slanted)),
            b"upshape" => return Ok(Inline::FontDecl(FontDecl::Upright)),
            b"ttfamily" => return Ok(Inline::FontDecl(FontDecl::Mono)),
            b"rmfamily" => return Ok(Inline::FontDecl(FontDecl::Roman)),
            b"sffamily" => return Ok(Inline::FontDecl(FontDecl::Sans)),
            b"normalfont" => return Ok(Inline::FontDecl(FontDecl::Normal)),
            b"em" => return Ok(Inline::FontDecl(FontDecl::Em)),
            b"rm" | b"bf" | b"it" | b"sl" | b"tt" | b"sf" | b"sc" => {
                use crate::frontend::ast::OldFont;
                return Ok(Inline::FontDecl(FontDecl::Old(match name {
                    "rm" => OldFont::Rm,
                    "bf" => OldFont::Bf,
                    "it" => OldFont::It,
                    "sl" => OldFont::Sl,
                    "tt" => OldFont::Tt,
                    "sf" => OldFont::Sf,
                    _ => OldFont::Sc,
                })));
            }
            b"textrm" | b"textup" | b"textsf" | b"textsl" | b"textmd" | b"textnormal" => {
                let decl = match name {
                    "textrm" => FontDecl::Roman,
                    "textup" => FontDecl::Upright,
                    "textsf" => FontDecl::Sans,
                    "textsl" => FontDecl::Slanted,
                    "textmd" => FontDecl::Medium,
                    _ => FontDecl::Normal,
                };
                let mut body = self.require_group()?;
                body.splice(0..0, [Inline::TextFontCommand, Inline::FontDecl(decl)]);
                return Ok(Inline::Group(body));
            }
            b"footnote" | b"thanks" => {
                let num = if name == "thanks" { String::new() } else { self.parse_optional_brackets()?.concat() };
                self.footnote_counter = match num.trim().parse::<u32>() {
                    Ok(n) => n,
                    Err(_) => self.footnote_counter + 1,
                };
                let text = self.require_group()?;
                let mark = if self.in_title {
                    thanks_mark(self.footnote_counter)
                } else {
                    footnote_mark(self.footnote_counter)
                };
                return Ok(Inline::Footnote(Box::new(crate::frontend::ast::FootnoteData { mark, text })));
            }
            // An `\hbox` in TeX; its contents are set as an ordinary group here.
            b"mbox" | b"hbox" => {
                // A box appended in horizontal mode sets the space factor to 1000 (tex.web §1076).
                let mut items = self.require_group()?;
                items.push(Inline::SpaceFactor(1000));
                return Ok(Inline::Group(items));
            }
            b"url" | b"path" => {
                let raw = self.brace_arg()?;
                return Ok(Inline::Mono(vec![Inline::Text(raw)]));
            }
            b"ensuremath" => {
                let raw = self.brace_arg()?;
                return Ok(Inline::Math(Box::new(self.parse_math(raw))));
            }
            b"dots" | b"ldots" | b"textellipsis" => {
                // latex.ltx `\textellipsis`: each period is followed by
                // `\kern\fontdimen3\font`, the interword stretch.
                let _ = self.eat("{}");
                let kern = Inline::FontDimenKern(3);
                return Ok(Inline::Group(vec![Inline::Text("."), kern.clone(), Inline::Text("."), kern.clone(), Inline::Text("."), kern]));
            }
            // latex.ltx: `\hskip1em`, `\hskip2em`, `\hskip.5em`, `\kern.5em`, `\tmspace`.
            b"quad" => return Ok(Inline::EmSpace { em: EM_1, breakable: true }),
            b"qquad" => return Ok(Inline::EmSpace { em: 2 * EM_1, breakable: true }),
            b"enskip" => return Ok(Inline::EmSpace { em: EM_1 / 2, breakable: true }),
            b"enspace" => return Ok(Inline::EmSpace { em: EM_1 / 2, breakable: false }),
            b"thinspace" => return Ok(Inline::EmSpace { em: self.thin_em(), breakable: false }),
            b"negthinspace" => return Ok(Inline::EmSpace { em: -self.thin_em(), breakable: false }),
            b"newline" => return Ok(Inline::Break),
            // Declarations with no effect on a page set here: running heads are not printed,
            // and page-break hints inside a paragraph are left to the page builder.
            b"unskip" => return Ok(Inline::Unskip),
            // `\nobreak\hfil(l)`: the glue is not a breakpoint (§866).
            b"nobreak" if self.starts_with("\\hfil") => {
                self.bump();
                let fill = self.starts_with("hfill");
                for _ in 0..if fill { 5 } else { 4 } {
                    self.bump();
                }
                return Ok(Inline::HFil(1.0, if fill { 2 } else { 1 } | crate::frontend::ast::HFIL_NOBREAK));
            }
            b"nopagebreak" | b"nolinebreak" | b"protect" | b"relax" | b"ignorespaces" => {
                if name.ends_with("break") {
                    self.parse_optional_brackets()?;
                }
                return Ok(Inline::Group(Vec::new()));
            }
            b"markboth" | b"markright" => {
                self.brace_arg()?;
                if name == "markboth" {
                    self.skip_hws();
                    self.brace_arg()?;
                }
                return Ok(Inline::Group(Vec::new()));
            }
            b"ss" | b"oe" | b"OE" | b"ae" | b"AE" | b"o" | b"O" | b"aa" | b"AA" => {
                let _ = self.eat("{}");
                return Ok(Inline::Text(match name {
                    "ss" => "ß",
                    "oe" => "œ",
                    "OE" => "Œ",
                    "ae" => "æ",
                    "AE" => "Æ",
                    "o" => "ø",
                    "O" => "Ø",
                    "aa" => "å",
                    _ => "Å",
                }));
            }
            // OT1 has no section, pilcrow or brace glyphs: LaTeX takes them from the math
            // symbol font, as these do.
            // latex.ltx: `\S`, `\P` are `\TextOrMath\textsection\mathsection` (and `\textparagraph`),
            // and these come from TS1 in Computer Modern.
            b"S" | b"P" | b"textdagger" | b"textdaggerdbl" if self.preamble.config.font_family == crate::font_metrics::FontFamily::ComputerModern => {
                let _ = self.eat("{}");
                let slot = ts1_slot(match name {
                    "S" => "textsection",
                    "P" => "textparagraph",
                    other => other,
                });
                return Ok(Inline::Logo(crate::frontend::ast::LogoKind::Ts1(slot.unwrap_or(0))));
            }
            b"S" | b"P" | b"textbraceleft" | b"textbraceright" | b"textbar" | b"textless" | b"textgreater" | b"textdagger" | b"textdaggerdbl" => {
                let _ = self.eat("{}");
                let math = match name {
                    "S" => "\\mathsection",
                    "P" => "\\mathparagraph",
                    "textbraceleft" => "\\{",
                    "textbraceright" => "\\}",
                    "textbar" => "|",
                    "textless" => "<",
                    "textgreater" => ">",
                    "textdagger" => "\\dagger",
                    _ => "\\ddagger",
                };
                return Ok(Inline::Math(Box::new(self.parse_math(math))));
            }
            b"texttt" => return Ok(Inline::Group(vec![Inline::TextFontCommand, Inline::Mono(self.require_group()?)])),
            b"par" => return Ok(Inline::ParEnd),
            b"color" => {
                // xcolor's \color ends in \ignorespaces.
                let color = self.parse_color_arg()?;
                self.skip_hws();
                return Ok(Inline::Color(color));
            }
            b"textsc" => {
                let mut body = self.require_group()?;
                body.splice(0..0, [Inline::TextFontCommand, Inline::ScShape]);
                return Ok(Inline::Group(body));
            }
            b"scshape" => {
                self.skip_hws();
                return Ok(Inline::ScShape);
            }
            b"textcolor" => {
                let color = self.parse_color_arg()?;
                let mut body = self.require_group()?;
                body.insert(0, Inline::Color(color));
                return Ok(Inline::Group(body));
            }
            b"definecolor" => {
                self.parse_definecolor()?;
                return Ok(Inline::Text(""));
            }
            b"noindent" => return Ok(Inline::NoIndent),
            b"indent" => return Ok(Inline::Indent),
            b"frenchspacing" => {
                self.preamble.config.french_spacing = true;
                return Ok(Inline::Text(""));
            }
            b"nonfrenchspacing" => {
                self.preamble.config.french_spacing = false;
                return Ok(Inline::Text(""));
            }
            b"verb" => return self.parse_verb(cmd_start),
            _ => {}
        }
        match name {
            "hangindent" => {
                self.skip_hws();
                let _ = self.eat("=");
                self.skip_hws();
                let dim = if self.starts_with("{") {
                    let s = self.brace_arg()?;
                    self.dim_pt(s).unwrap_or(0.0)
                } else {
                    let start = self.byte_offset;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
                    {
                        self.bump();
                    }
                    let s = &self.source_code[start..self.byte_offset];
                    self.dim_pt(s).unwrap_or(0.0)
                };
                Ok(Inline::HangIndent(dim))
            }
            "hangafter" => {
                self.skip_hws();
                let _ = self.eat("=");
                self.skip_hws();
                let val = if self.starts_with("{") {
                    let s = self.brace_arg()?;
                    s.trim().parse::<i32>().unwrap_or(1)
                } else {
                    let start = self.byte_offset;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '-') {
                        self.bump();
                    }
                    let s = &self.source_code[start..self.byte_offset];
                    s.trim().parse::<i32>().unwrap_or(1)
                };
                Ok(Inline::HangAfter(val))
            }
            "parshape" => {
                let shape = self.parse_parshape_args()?;
                Ok(Inline::ParShape(shape))
            }
            "pretolerance" => {
                self.skip_hws();
                let _ = self.eat("=");
                self.skip_hws();
                let val = if self.starts_with("{") {
                    let s = self.brace_arg()?;
                    s.trim().parse::<i32>().unwrap_or(100)
                } else {
                    let start = self.byte_offset;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '-') {
                        self.bump();
                    }
                    let s = &self.source_code[start..self.byte_offset];
                    s.trim().parse::<i32>().unwrap_or(100)
                };
                Ok(Inline::PreTolerance(val))
            }
            "tolerance" => {
                self.skip_hws();
                let _ = self.eat("=");
                self.skip_hws();
                let val = if self.starts_with("{") {
                    let s = self.brace_arg()?;
                    s.trim().parse::<i32>().unwrap_or(200)
                } else {
                    let start = self.byte_offset;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '-') {
                        self.bump();
                    }
                    let s = &self.source_code[start..self.byte_offset];
                    s.trim().parse::<i32>().unwrap_or(200)
                };
                Ok(Inline::Tolerance(val))
            }
            "emergencystretch" => {
                self.skip_hws();
                let _ = self.eat("=");
                self.skip_hws();
                let dim = if self.starts_with("{") {
                    let s = self.brace_arg()?;
                    self.dim_pt(s).unwrap_or(0.0)
                } else {
                    let start = self.byte_offset;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
                    {
                        self.bump();
                    }
                    let s = &self.source_code[start..self.byte_offset];
                    self.dim_pt(s).unwrap_or(0.0)
                };
                Ok(Inline::EmergencyStretch(dim))
            }
            "sloppy" => Ok(Inline::Sloppy),
            "centering" | "raggedright" | "raggedleft" => Ok(Inline::Align(align_of_declaration(name))),
            _ if FontSize::from_command(name).is_some() => {
                // A control word swallows the spaces after it.
                self.skip_hws();
                let size = FontSize::from_command(name).unwrap_or(FontSize::NormalSize);
                if self.inline_depth == 0 {
                    self.cur_size = Some(size);
                }
                Ok(Inline::FontSize(size))
            }
            "fussy" => Ok(Inline::Fussy),
            "looseness" => {
                self.skip_hws();
                let _ = self.eat("=");
                self.skip_hws();
                let val = if self.starts_with("{") {
                    let s = self.brace_arg()?;
                    s.trim().parse::<i32>().unwrap_or(0)
                } else {
                    let start = self.byte_offset;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '-') {
                        self.bump();
                    }
                    let s = &self.source_code[start..self.byte_offset];
                    s.trim().parse::<i32>().unwrap_or(0)
                };
                Ok(Inline::Looseness(val))
            }
            "leftskip" => {
                self.skip_hws();
                let _ = self.eat("=");
                self.skip_hws();
                let dim = if self.starts_with("{") {
                    let s = self.brace_arg()?;
                    self.dim_pt(s).unwrap_or(0.0)
                } else {
                    let start = self.byte_offset;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
                    {
                        self.bump();
                    }
                    let s = &self.source_code[start..self.byte_offset];
                    self.dim_pt(s).unwrap_or(0.0)
                };
                Ok(Inline::LeftSkip(dim))
            }
            "rightskip" => {
                self.skip_hws();
                let _ = self.eat("=");
                self.skip_hws();
                let dim = if self.starts_with("{") {
                    let s = self.brace_arg()?;
                    self.dim_pt(s).unwrap_or(0.0)
                } else {
                    let start = self.byte_offset;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
                    {
                        self.bump();
                    }
                    let s = &self.source_code[start..self.byte_offset];
                    self.dim_pt(s).unwrap_or(0.0)
                };
                Ok(Inline::RightSkip(dim))
            }
            "parfillskip" => {
                self.skip_hws();
                let _ = self.eat("=");
                self.skip_hws();
                let (width, stretch) = if self.starts_with("{") {
                    let s = self.brace_arg()?;
                    crate::doc_config::parse_glue_pt(s).unwrap_or((0.0, f32::INFINITY))
                } else {
                    let start = self.byte_offset;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
                    {
                        self.bump();
                    }
                    let s = &self.source_code[start..self.byte_offset];
                    let width = self.dim_pt(s).unwrap_or(0.0);
                    self.skip_hws();
                    let stretch = if self.starts_with("plus") {
                        self.eat("plus");
                        self.skip_hws();
                        let plus_start = self.byte_offset;
                        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
                        {
                            self.bump();
                        }
                        let plus_s = &self.source_code[plus_start..self.byte_offset];
                        if plus_s.contains("fil") {
                            f32::INFINITY
                        } else {
                            self.dim_pt(plus_s).unwrap_or(0.0)
                        }
                    } else {
                        0.0
                    };
                    self.skip_hws();
                    if self.starts_with("minus") {
                        self.eat("minus");
                        self.skip_hws();
                        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
                        {
                            self.bump();
                        }
                    }
                    (width, stretch)
                };
                Ok(Inline::ParFillSkip { width, stretch })
            }
            "prevgraf" => {
                self.skip_hws();
                let _ = self.eat("=");
                self.skip_hws();
                let val = if self.starts_with("{") {
                    let s = self.brace_arg()?;
                    s.trim().parse::<i32>().unwrap_or(0)
                } else {
                    let start = self.byte_offset;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '-') {
                        self.bump();
                    }
                    let s = &self.source_code[start..self.byte_offset];
                    s.trim().parse::<i32>().unwrap_or(0)
                };
                Ok(Inline::PrevGraf(val))
            }
            "lastlinefit" => {
                self.skip_hws();
                let _ = self.eat("=");
                self.skip_hws();
                let val = if self.starts_with("{") {
                    let s = self.brace_arg()?;
                    s.trim().parse::<i32>().unwrap_or(0)
                } else {
                    let start = self.byte_offset;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '-') {
                        self.bump();
                    }
                    let s = &self.source_code[start..self.byte_offset];
                    s.trim().parse::<i32>().unwrap_or(0)
                };
                Ok(Inline::LastLineFit(val))
            }
            "rushtexpagelabel" => {
                let id = self.brace_arg()?.trim().parse::<u16>().unwrap_or(u16::MAX);
                Ok(Inline::PageLabel(id))
            }
            "vspace" => {
                let _ = self.eat("*");
                let val_str = self.brace_arg()?;
                let (pt, em, ex) = font_relative_dim(val_str).unwrap_or((0.0, 0.0, 0.0));
                Ok(Inline::VAdjust { pt, em, ex })
            }
            // plain.tex: `\hskip 0pt plus 1fil` (`\hfil`), `plus 1fill` (`\hfill`), `plus -1fil`.
            "hfil" => Ok(Inline::HFil(1.0, 1)),
            "hfill" => Ok(Inline::HFil(1.0, 2)),
            "hfilneg" => Ok(Inline::HFil(-1.0, 1)),
            "hspace" => {
                let _ = self.eat("*");
                self.skip_hws();
                let dim = if self.starts_with("{") {
                    let s = self.brace_arg()?;
                    // latex.ltx: `\fill` is `0pt plus 1fill`; `\stretch{n}` is `0pt plus n fill`.
                    let t = s.trim();
                    if t == "\\fill" {
                        return Ok(Inline::HFil(1.0, 2));
                    }
                    if let Some(n) = t.strip_prefix("\\stretch{").and_then(|r| r.strip_suffix('}')).and_then(|n| n.trim().parse::<f32>().ok()) {
                        return Ok(Inline::HFil(n, 2));
                    }
                    if s.contains("plus") || s.contains("minus") {
                        if let Some((w, st, sh)) = {
                            let (em, ex) = self.em_ex();
                            crate::doc_config::parse_glue3_pt_in(s, em, ex)
                        } {
                            if st.is_finite() {
                                return Ok(Inline::HGlue(w, st, sh));
                            }
                        }
                    }
                    self.dim_pt(s).unwrap_or(0.0)
                } else {
                    let start = self.byte_offset;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
                    {
                        self.bump();
                    }
                    let s = &self.source_code[start..self.byte_offset];
                    self.dim_pt(s).unwrap_or(0.0)
                };
                Ok(Inline::HSpace(dim))
            }
            "TeX" | "LaTeX" | "AmS" | "LaTeXe" | "copyright" | "textcopyright" => {
                use crate::frontend::ast::LogoKind;
                let _ = self.eat("{}");
                Ok(Inline::Logo(match name {
                    "TeX" => LogoKind::TeX,
                    "LaTeX" => LogoKind::LaTeX,
                    "LaTeXe" => LogoKind::LaTeXe,
                    "copyright" | "textcopyright" if self.preamble.config.font_family == crate::font_metrics::FontFamily::ComputerModern => {
                        LogoKind::Ts1(169)
                    }
                    "copyright" | "textcopyright" => LogoKind::Copyright,
                    _ => LogoKind::AmS,
                }))
            }
            "textbackslash" => {
                // OT1 has no backslash: LaTeX's default takes cmsy's (OMS), even in `\texttt`.
                // The Base-14 families have the glyph in the text font.
                let _ = self.eat("{}");
                if self.preamble.config.font_family == crate::font_metrics::FontFamily::ComputerModern {
                    Ok(Inline::Math(Box::new(self.parse_math("\\backslash"))))
                } else {
                    Ok(Inline::Text("\\"))
                }
            }
            "textasciitilde" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("~"))
            }
            "textunderscore" => {
                let _ = self.eat("{}");
                if self.preamble.config.font_family == crate::font_metrics::FontFamily::ComputerModern {
                    Ok(Inline::Text(crate::layout::engine::UNDERSCORE_TEXT))
                } else {
                    Ok(Inline::Text("_"))
                }
            }
            "textasciicircum" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("^"))
            }
            "textbullet" if self.preamble.config.font_family != crate::font_metrics::FontFamily::ComputerModern => {
                let _ = self.eat("{}");
                Ok(Inline::Text("•"))
            }
            "textendash" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("–"))
            }
            "textemdash" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("—"))
            }
            // Greek lowercase
            "alpha" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("α"))
            }
            "beta" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("β"))
            }
            "gamma" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("γ"))
            }
            "delta" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("δ"))
            }
            "epsilon" | "varepsilon" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("ε"))
            }
            "zeta" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("ζ"))
            }
            "eta" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("η"))
            }
            "theta" | "vartheta" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("θ"))
            }
            "iota" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("ι"))
            }
            "kappa" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("κ"))
            }
            "lambda" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("λ"))
            }
            "mu" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("μ"))
            }
            "nu" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("ν"))
            }
            "xi" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("ξ"))
            }
            "pi" | "varpi" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("π"))
            }
            "rho" | "varrho" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("ρ"))
            }
            "sigma" | "varsigma" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("σ"))
            }
            "tau" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("τ"))
            }
            "upsilon" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("υ"))
            }
            "phi" | "varphi" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("φ"))
            }
            "chi" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("χ"))
            }
            "psi" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("ψ"))
            }
            "omega" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("ω"))
            }

            // Greek uppercase
            "Gamma" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("Γ"))
            }
            "Delta" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("Δ"))
            }
            "Theta" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("Θ"))
            }
            "Lambda" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("Λ"))
            }
            "Xi" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("Ξ"))
            }
            "Pi" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("Π"))
            }
            "Sigma" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("Σ"))
            }
            "Upsilon" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("Υ"))
            }
            "Phi" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("Φ"))
            }
            "Psi" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("Ψ"))
            }
            "Omega" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("Ω"))
            }

            // STEM mathematical operators, relations, symbols
            "sum" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("∑"))
            }
            "prod" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("∏"))
            }
            "int" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("∫"))
            }
            "partial" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("∂"))
            }
            "nabla" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("∇"))
            }
            "infty" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("∞"))
            }
            "le" | "leq" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("≤"))
            }
            "ge" | "geq" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("≥"))
            }
            "neq" | "ne" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("≠"))
            }
            "approx" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("≈"))
            }
            "equiv" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("≡"))
            }
            "pm" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("±"))
            }
            "times" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("×"))
            }
            "div" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("÷"))
            }
            "cdot" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("⋅"))
            }
            "forall" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("∀"))
            }
            "exists" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("∃"))
            }
            "in" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("∈"))
            }
            "notin" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("∉"))
            }
            "subset" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("⊂"))
            }
            "subseteq" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("⊆"))
            }
            "supset" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("⊃"))
            }
            "supseteq" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("⊇"))
            }
            "cup" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("∪"))
            }
            "cap" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("∩"))
            }
            "to" | "rightarrow" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("→"))
            }
            "leftarrow" | "gets" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("←"))
            }
            "leftrightarrow" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("↔"))
            }
            "Rightarrow" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("⇒"))
            }
            "Leftarrow" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("⇐"))
            }
            "Leftrightarrow" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("⇔"))
            }
            "sqrt" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("√"))
            }
            "perp" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("⊥"))
            }
            "sim" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("∼"))
            }
            "cong" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("≅"))
            }
            "oplus" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("⊕"))
            }
            "otimes" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("⊗"))
            }
            "emptyset" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("∅"))
            }
            "propto" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("∝"))
            }
            "dag" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("†"))
            }
            "ddag" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("‡"))
            }
            "circ" | "degree" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("°"))
            }
            // `\char<number>`: the character of that code in the current font; codes are
            // taken as ASCII, which the Computer Modern text fonts follow for these slots.
            "char" => {
                let start = self.byte_offset;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.bump();
                }
                let code: usize = self.source_code[start..self.byte_offset].parse().unwrap_or(0);
                let _ = self.eat(" ");
                Ok(Inline::Text(crate::math::math_fonts::ASCII_GLYPH_STRS[code & 127]))
            }
            // A length set inside a paragraph or group (`{\setlength{\multlinegap}{0pt}..}`):
            // of these only the math parameters are modelled, until the group ends.
            "setlength" | "addtolength" => {
                let reg = self.macro_arg()?;
                let value = self.brace_arg()?;
                self.set_math_length(reg, value, name == "addtolength");
                Ok(Inline::Text(""))
            }
            "rule" => {
                // `em` and `ex` of the document's roman font (cmr: quad 1em, x-height .430554em).
                let size = self.preamble.config.base_font_size;
                let dim = |s: &str| crate::doc_config::parse_dimension_pt_in(s, size, 0.430554 * size).unwrap_or(0.0);
                let raise = self.parse_optional_brackets()?.first().map_or(0.0, |s| dim(s));
                let w = dim(self.brace_arg()?);
                let h = dim(self.brace_arg()?);
                Ok(Inline::Math(Box::new(crate::math::MathNode::Rule { w, h, raise })))
            }
            // Math parameter assignments (`{\delimiterfactor750 ..}`): the delimiter ones
            // reach the `\left`s parsed while they hold.
            "delimiterfactor" | "delimitershortfall" | "nulldelimiterspace" | "scriptspace" => {
                self.skip_hws();
                let _ = self.eat("=");
                self.skip_hws();
                let start = self.byte_offset;
                while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
                    self.bump();
                }
                let value = &self.source_code[start..self.byte_offset];
                match name {
                    "delimiterfactor" => {
                        if let Ok(v) = value.parse::<i32>() {
                            self.math_params.delim.0 = v.clamp(0, i32::from(u16::MAX)) as u16;
                        }
                    }
                    "delimitershortfall" => {
                        let size = self.preamble.config.base_font_size;
                        if let Some(v) = crate::doc_config::parse_dimension_pt_in(value, size, 0.430554 * size) {
                            self.math_params.delim.1 = v;
                        }
                    }
                    _ => {}
                }
                Ok(Inline::Text(""))
            }
            // A kernel-internal switch under `\makeatletter` (`\ctagsplit@true`): the
            // settings these flip are not modelled.
            _ if self.peek() == Some('@') => {
                while self.peek().is_some_and(|c| c.is_ascii_alphabetic() || c == '@') {
                    self.bump();
                }
                Ok(Inline::Text(""))
            }
            // latex.ltx's TS1 text symbols (`\DeclareTextSymbolDefault{..}{TS1}`, ts1enc.def).
            other if self.preamble.config.font_family == crate::font_metrics::FontFamily::ComputerModern && ts1_slot(other).is_some() => {
                let _ = self.eat("{}");
                Ok(Inline::Logo(crate::frontend::ast::LogoKind::Ts1(ts1_slot(other).unwrap_or(0))))
            }
            other if self.gaps.is_some() => {
                if let Some(g) = self.gaps.as_mut() {
                    g.push((cmd_start as u32, "command", other));
                }
                Ok(Inline::Text(""))
            }
            other => {
                let span_len = 1 + other.len();
                let help_msg = suggest_command(other).map(|sug| format!("did you mean '\\{sug}'?"));
                Err(self.err_span(
                    cmd_start,
                    span_len,
                    format!("unknown command \\{other}"),
                    Some("unknown command"),
                    None,
                    help_msg.as_deref(),
                ))
            }
        }
    }

    fn parse_verb(&mut self, _cmd_start: usize) -> Result<Inline<'a>, ParseError> {
        let _ = self.eat("*");
        let delim_start = self.byte_offset;
        let delim = match self.bump() {
            Some(ch) if !ch.is_whitespace() => ch,
            _ => {
                return Err(self.err_span(
                    delim_start,
                    1,
                    "expected delimiter after \\verb",
                    Some("expected non-whitespace delimiter character here"),
                    None,
                    Some("e.g. \\verb|code| or \\verb+code+"),
                ));
            }
        };
        let start = self.byte_offset;
        let rest = self.rest();
        if let Some(pos) = rest.find(delim) {
            let code = &self.source_code[start..start + pos];
            self.byte_offset = start + pos + delim.len_utf8();
            Ok(Inline::Mono(vec![Inline::Text(code)]))
        } else {
            let help_str = format!("add closing delimiter '{delim}'");
            Err(self.err_span(
                delim_start,
                delim.len_utf8(),
                "unclosed \\verb delimiter",
                Some("delimiter opened here"),
                Some("reached end of file without finding matching closing delimiter"),
                Some(&help_str),
            ))
        }
    }

    /// The argument of `\title`, `\author` or `\date`, where `\footnote` is `\thanks`.
    fn title_arg(&mut self) -> Result<Vec<Inline<'a>>, ParseError> {
        self.in_title = true;
        let arg = self.require_group();
        self.in_title = false;
        arg
    }

    /// LaTeX's optional argument `[...]` (a `]` inside braces does not end it), parsed as
    /// text; `None` when no `[` follows.
    fn optional_inlines(&mut self, unclosed: &'static str) -> Result<Option<Vec<Inline<'a>>>, ParseError> {
        if !self.starts_with("[") {
            return Ok(None);
        }
        let open_offset = self.byte_offset;
        self.bump();
        let start_offset = self.byte_offset;
        let mut brace_depth = 0usize;
        let mut end_offset = None;
        while let Some(ch) = self.peek() {
            match ch {
                '{' => brace_depth += 1,
                '}' => brace_depth = brace_depth.saturating_sub(1),
                ']' if brace_depth == 0 => {
                    end_offset = Some(self.byte_offset);
                    self.bump();
                    break;
                }
                _ => {}
            }
            self.bump();
        }
        let Some(end) = end_offset else {
            return Err(self.err_span(
                open_offset,
                1,
                unclosed,
                Some("opening '[' starts here"),
                Some("reached end of file without finding matching ']'"),
                Some("add the closing ']'"),
            ));
        };
        let mut sub_parser = Parser::new(&self.source_code[start_offset..end]);
        let inls = sub_parser.parse_para_inlines()?;
        self.skip_hws();
        Ok(Some(inls))
    }

    /// The blanks TeX skips before an undelimited macro argument (§393): spaces, comments
    /// with their line ends, and one end of line, but not the blank line of a `\par`.
    pub(crate) fn skip_arg_blanks(&mut self) {
        loop {
            self.skip_hws();
            let rest = self.rest();
            if rest.starts_with('%') {
                // A comment drops the rest of its line; the next line's leading blanks go too.
                self.byte_offset += rest.find('\n').map_or(rest.len(), |i| i + 1);
                continue;
            }
            break;
        }
        self.skip_arg_space();
    }

    pub(crate) fn require_group(&mut self) -> Result<Vec<Inline<'a>>, ParseError> {
        self.skip_arg_blanks();
        let pos = self.byte_offset;
        if !self.eat("{") {
            return Err(self.err_span(
                pos,
                1,
                "expected { after command",
                Some("expected '{' here"),
                None,
                Some("enclose the command arguments in curly braces, e.g. '{...}'"),
            ));
        }
        self.parse_until_rbrace()
    }

#[inline(always)]
fn is_pure_text_word(word: u64) -> bool {
    // 1. Any byte < 32 (ASCII control characters: \0, \t, \n, \r, etc.)
    let lt_32 = (word.wrapping_sub(0x2020_2020_2020_2020) & !word) & 0x8080_8080_8080_8080;

    // 2. Any byte >= 123 ({, |, }, ~, DEL, or non-ASCII >= 128)
    let ge_123 = (word.wrapping_add(0x0505_0505_0505_0505) | word) & 0x8080_8080_8080_8080;

    if (lt_32 | ge_123) != 0 {
        return false;
    }

    // 3. Delimiters in [32, 122]:
    // Backslash '\' (0x5C = 92), '$' (0x24 = 36), '%' (0x25 = 37), '&' (0x26 = 38)
    let w_bs = word ^ 0x5C5C_5C5C_5C5C_5C5C;
    let w_d  = word ^ 0x2424_2424_2424_2424;
    let w_p  = word ^ 0x2525_2525_2525_2525;
    let w_a  = word ^ 0x2626_2626_2626_2626;

    const ONES: u64 = 0x0101_0101_0101_0101;
    let z_bs = w_bs.wrapping_sub(ONES) & !w_bs;
    let z_d  = w_d.wrapping_sub(ONES)  & !w_d;
    let z_p  = w_p.wrapping_sub(ONES)  & !w_p;
    let z_a  = w_a.wrapping_sub(ONES)  & !w_a;

    ((z_bs | z_d | z_p | z_a) & 0x8080_8080_8080_8080) == 0
}

    /// Contiguous source slice. Newlines stay in the slice; layout treats
    /// them as space. A `%` or command ends the run so the slice stays
    /// untransformed.
    fn parse_text_run(&mut self) -> &'a str {
        let bytes = self.source_code.as_bytes();
        let start = self.byte_offset;
        let mut i = start;
        let len = bytes.len();

        loop {
            // SWAR fast scan for contiguous printable ASCII prose (8 bytes per step)
            while i + 8 <= len {
                let chunk = u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
                if Self::is_pure_text_word(chunk) {
                    i += 8;
                } else {
                    break;
                }
            }

            if i >= len {
                break;
            }

            let b = bytes[i];
            match b {
                b'\\' | b'{' | b'}' | b'%' | b'&' | b'$' | b'~' => break,
                b'\n' => {
                    let r = &self.source_code[i..];
                    if r.starts_with("\n\n") || r.starts_with("\n\r\n") {
                        break;
                    }
                    i += 1;
                }
                b'\r' => {
                    let r = &self.source_code[i..];
                    if r.starts_with("\r\n\r\n") {
                        break;
                    }
                    i += 1;
                }
                _ => {
                    if b < 128 {
                        i += 1;
                    } else {
                        let rest = &self.source_code[i..];
                        if let Some(ch) = rest.chars().next() {
                            i += ch.len_utf8();
                        } else {
                            break;
                        }
                    }
                }
            }
        }
        self.byte_offset = i;
        &self.source_code[start..i]
    }
}

#[inline]
fn find_unescaped_char(s: &str, target: char) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    let len = bytes.len();
    let target_byte = target as u8;
    let mask_t = (target_byte as u64) * 0x0101_0101_0101_0101;
    const MASK_BS: u64 = 0x5C5C_5C5C_5C5C_5C5C;
    const ONES: u64 = 0x0101_0101_0101_0101;
    const HIGHS: u64 = 0x8080_8080_8080_8080;

    loop {
        while i + 8 <= len {
            let word = u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
            let wt = word ^ mask_t;
            let wb = word ^ MASK_BS;
            let has_zero_t = (wt.wrapping_sub(ONES) & !wt) & HIGHS;
            let has_zero_b = (wb.wrapping_sub(ONES) & !wb) & HIGHS;
            if (has_zero_t | has_zero_b) == 0 {
                i += 8;
            } else {
                break;
            }
        }

        if i >= len {
            break;
        }

        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == target_byte {
            return Some(i);
        }
        i += 1;
    }
    None
}

pub(crate) const SINGLE_ESCAPE_LUT: [bool; 256] = {
    let mut lut = [false; 256];
    lut[b'%' as usize] = true;
    lut[b'{' as usize] = true;
    lut[b'}' as usize] = true;
    lut[b'\\' as usize] = true;
    lut[b'#' as usize] = true;
    lut[b'&' as usize] = true;
    lut[b'_' as usize] = true;
    lut[b'$' as usize] = true;
    lut[b'~' as usize] = true;
    lut[b'^' as usize] = true;
    lut
};

/// A dimension split into its pt, `em` and `ex` parts, for a font that is known only at
/// layout time. The units are linear, so the parts are the value at em = ex = 0 and the
/// change for a unit em or ex.
fn font_relative_dim(s: &str) -> Option<(f32, f32, f32)> {
    let pt = crate::doc_config::parse_dimension_pt_in(s, 0.0, 0.0)?;
    let em = crate::doc_config::parse_dimension_pt_in(s, 1.0, 0.0)? - pt;
    let ex = crate::doc_config::parse_dimension_pt_in(s, 0.0, 1.0)? - pt;
    Some((pt, em, ex))
}

/// A heading's title with its optional short form (`\section[toc]{title}`) in front,
/// ended by `Inline::ShortTitleEnd` (`crate::frontend::ast::section_title_parts` splits it).
fn with_short_title<'a>(short: Option<Vec<Inline<'a>>>, title: Vec<Inline<'a>>) -> Vec<Inline<'a>> {
    match short {
        Some(mut v) => {
            v.push(Inline::ShortTitleEnd);
            v.extend(title);
            v
        }
        None => title,
    }
}

/// article.cls `\maketitle`: `\thefootnote` is `\@fnsymbol\c@footnote` and the mark in
/// the text is `\rlap{\@textsuperscript{..}}`, a zero-width box. The footnote body takes
/// the mark out of the box (`layout_footnote_body`). The text symbols are TS1 in the
/// kernel; the math ones (cmsy) stand in for them here.
fn thanks_mark<'a>(n: u32) -> crate::math::MathNode<'a> {
    use crate::math::MathNode;
    const SYMBOLS: [&str; 9] = [
        "\\ast",
        "\\dagger",
        "\\ddagger",
        "\\S",
        "\\P",
        "\\|",
        "\\ast\\ast",
        "\\dagger\\dagger",
        "\\ddagger\\ddagger",
    ];
    let Some(sym) = n.checked_sub(1).and_then(|k| SYMBOLS.get(k as usize)) else {
        // `\@ctrerr`: the counter is out of range for `\@fnsymbol`.
        return footnote_mark(n);
    };
    // In text, `\TextOrMath` picks the TS1 symbols (`\textasteriskcentered`, `\textdagger`,
    // `\textdaggerdbl`, `\textsection`, `\textparagraph`); `\textbardbl` stays OMS.
    const TS1: [&[u8]; 9] = [&[42], &[132], &[133], &[167], &[182], &[], &[42, 42], &[132, 132], &[133, 133]];
    let ts1 = TS1[n as usize - 1];
    let sup = if ts1.is_empty() {
        crate::math::math_parser::parse_math_str(sym)
    } else {
        MathNode::Row(ts1.iter().map(|&slot| ts1_glyph(slot)).collect())
    };
    MathNode::DimensionBox {
        content: Box::new(MathNode::SubSup { nucleus: Box::new(MathNode::Row(Vec::new())), sub: None, sup: Some(Box::new(sup)) }),
        zero_width: true,
        shift_left: false,
        smash: 0,
    }
}

/// latex.ltx `\@makefnmark`: `\@textsuperscript{\normalfont\@thefnmark}`, the arabic
/// number in the roman font as a superscript to an empty nucleus.
fn footnote_mark<'a>(n: u32) -> crate::math::MathNode<'a> {
    use crate::math::MathNode;
    const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
    let digits: Vec<MathNode<'a>> = n.to_string().bytes().map(|b| MathNode::Number(DIGITS[(b - b'0') as usize])).collect();
    MathNode::SubSup {
        nucleus: Box::new(MathNode::Row(Vec::new())),
        sub: None,
        sup: Some(Box::new(MathNode::Row(digits))),
    }
}

/// latex.ltx `\TeX` (`T\kern-.1667em\lower.5ex\hbox{E}\kern-.125emX`) and `\LaTeX`
/// (`L\kern-.36em`, an `A` at `\sf@size` raised to the top of a `T`, `\kern-.15em\TeX`) in
/// the roman font.
/// latex.ltx's `\TeX` and `\LaTeX` in text face `face` (Computer Modern): the kerns are in
/// ems of that font, `E` drops .5ex, and `LaTeX`'s `A` is set at `\sf@size` (script
/// size) with its top level with the `T`'s.
pub(crate) fn tex_logo(kind: crate::frontend::ast::LogoKind, face: crate::backend::fonts::Face, size: f32) -> crate::math::MathNode<'static> {
    use crate::backend::fonts::{Face, FontFamily};
    use crate::backend::scaled;
    use crate::frontend::ast::LogoKind;
    use crate::math::math_ast::MathSkip;
    use crate::math::{MathNode, MathStyle};
    if kind == LogoKind::AmS {
        // amsmath: `{\AmSfont A\kern-.1667em\lower.5ex\hbox{M}\kern-.125em S}`, `\AmSfont`
        // being cmsy (quad 1em, x-height .430555em).
        let row = vec![
            MathNode::Variable('A'),
            MathNode::Space(MathSkip::TextEm(-0.1667)),
            MathNode::Raise { content: Box::new(MathNode::Variable('M')), shift: -0.5 * 0.430555 * size },
            MathNode::Space(MathSkip::TextEm(-0.125)),
            MathNode::Variable('S'),
        ];
        return text_hbox(MathNode::StyleChange { face: Face::Symbol, content: Box::new(MathNode::Row(row)) });
    }
    if kind == LogoKind::Copyright {
        return copyright_logo(face, size);
    }
    if let LogoKind::Ts1(slot) = kind {
        return text_hbox(ts1_glyph(slot));
    }
    let latex = matches!(kind, LogoKind::LaTeX | LogoKind::LaTeXe);
    let face = match face {
        Face::Bold | Face::Italic | Face::Mono => face,
        Face::BoldItalic => Face::Bold,
        _ => Face::Roman,
    };
    let fam = FontFamily::ComputerModern;
    // The font at `size` (its design-size master), and `A` at `\sf@size`. The kerns and
    // `\lower.5ex` are TeX's `<decimal>em` and `ex` of the font (`scan_dimen`).
    let (em, ex) = (scaled::font_dimen_sp(fam, face, size, 6), scaled::font_dimen_sp(fam, face, size, 5));
    let kern = |digits: &[u8], unit: i64| MathNode::Space(MathSkip::Pt(-scaled::em_times(scaled::round_decimals(digits), unit) as f32 / 65536.0));
    let cap = crate::math::math_fonts::char_height_depth('T', face, fam).0 / 1000.0;
    let sf_ratio = crate::math::math_ast::latex_math_sizes(size).0 / size;
    let mut row = Vec::with_capacity(9);
    if latex {
        row.push(MathNode::Variable('L'));
        row.push(kern(b"36", em));
        row.push(MathNode::Raise {
            content: Box::new(MathNode::StyleScope { style: MathStyle::Script, content: Box::new(MathNode::Variable('A')) }),
            shift: cap * (1.0 - sf_ratio) * size,
        });
        row.push(kern(b"15", em));
    }
    row.push(MathNode::Variable('T'));
    row.push(kern(b"1667", em));
    row.push(MathNode::Raise { content: Box::new(MathNode::Variable('E')), shift: -(scaled::em_times(scaled::EM_1 / 2, ex) as f32) / 65536.0 });
    row.push(kern(b"125", em));
    row.push(MathNode::Variable('X'));
    if kind == LogoKind::LaTeXe {
        // `\kern.15em2$_{\textstyle\varepsilon}$`, the formula with `\m@th`.
        row.push(MathNode::Space(MathSkip::Pt(scaled::em_times(scaled::round_decimals(b"15"), em) as f32 / 65536.0)));
        row.push(MathNode::Variable('2'));
        let text = MathNode::StyleChange { face, content: Box::new(MathNode::Row(row)) };
        let eps = MathNode::Formula(Box::new(crate::math::math_parser::parse_math_str("{}_{\\textstyle\\varepsilon}")));
        return text_hbox(MathNode::Row(vec![text, eps]));
    }
    text_hbox(MathNode::StyleChange { face, content: Box::new(MathNode::Row(row)) })
}

/// ltoutenc.dtx's OMS `\textcircled{c}`: `\ooalign{\hfil\raise.07ex\hbox{\upshape c}\hfil\crcr
/// \char13}`, the `c` centred over cmsy's large circle (both rows start at the same point).
fn copyright_logo(face: crate::backend::fonts::Face, size: f32) -> crate::math::MathNode<'static> {
    use crate::backend::fonts::{Face, FontFamily};
    use crate::math::math_ast::MathSkip;
    use crate::math::MathNode;
    let up = match face {
        Face::Bold | Face::BoldItalic => Face::Bold,
        Face::Mono => Face::Mono,
        _ => Face::Roman,
    };
    let fam = FontFamily::ComputerModern;
    let m = crate::backend::scaled::fix_metrics(fam, up, size);
    let c_w = m.map_or(0.444446, |m| m.widths[b'c' as usize] as f32 / 1048576.0);
    let ex = crate::backend::scaled::font_dimen_sp(fam, up, size, 5);
    // cmsy's char 13 is 1em wide at every design size.
    let pad = (1.0 - c_w) / 2.0;
    let c = MathNode::Raise { content: Box::new(MathNode::StyleChange { face: up, content: Box::new(MathNode::Variable('c')) }), shift: crate::backend::scaled::em_times(crate::backend::scaled::round_decimals(b"07"), ex) as f32 / 65536.0 };
    let lap = MathNode::DimensionBox {
        content: Box::new(MathNode::Row(vec![MathNode::Space(MathSkip::Pt(pad * size)), c])),
        zero_width: true,
        shift_left: false,
        smash: 0,
    };
    let circle = MathNode::Formula(Box::new(crate::math::math_parser::parse_math_str("\\bigcirc")));
    text_hbox(MathNode::Row(vec![lap, circle]))
}

/// The slot of a TS1 text command (`textbullet`), from `backend_ts1::TS1_TEXT_SYMBOLS`.
pub(crate) fn ts1_slot(name: &str) -> Option<u8> {
    let t = &crate::backend::backend_ts1::TS1_TEXT_SYMBOLS;
    t.binary_search_by_key(&name, |&(n, _)| n).ok().map(|i| t[i].1)
}

/// TS1 slot `slot` as a glyph of `Face::TextComp` (tcrm at the current size).
fn ts1_glyph(slot: u8) -> crate::math::MathNode<'static> {
    let code = crate::backend::backend_ts1::TS1_SLOT_CODE[slot as usize];
    crate::math::MathNode::StyleChange { face: crate::backend::fonts::Face::TextComp, content: Box::new(crate::math::MathNode::Variable((code & 0x7F) as char)) }
}

/// The logos are text, not formulas: an unshifted hbox, so the last character takes no
/// italic correction (cmsy `S` has .075em, which math would append to `\AmS`).
fn text_hbox(content: crate::math::MathNode<'static>) -> crate::math::MathNode<'static> {
    crate::math::MathNode::Raise { content: Box::new(content), shift: 0.0 }
}

/// Declarations that change no mode: a `{` group starting with them in vertical mode is still
/// in vertical mode.
const VMODE_NEUTRAL_DECLS: [&str; 26] = [
    "tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE", "huge",
    "Huge", "bfseries", "mdseries", "itshape", "slshape", "upshape", "scshape", "rmfamily",
    "sffamily", "ttfamily", "normalfont", "em", "centering", "raggedright", "raggedleft", "sloppy", "fussy",
];

/// The offset of the `}` closing the group opened at `open` when the group holds a display
/// or list environment (outside math), which an inline group cannot.
fn block_group_end(src: &str, open: usize) -> Option<usize> {
    const BLOCK_ENVS: &[&str] = &[
        "equation", "align", "gather", "multline", "flalign", "alignat", "displaymath", "itemize",
        "enumerate", "description", "center", "flushleft", "flushright", "quote", "quotation",
        "verse", "verbatim",
    ];
    let b = src.as_bytes();
    let (mut i, mut depth, mut math, mut found) = (open + 1, 0usize, false, false);
    while i < b.len() {
        match b[i] {
            b'\\' => {
                let rest = &src[i + 1..];
                if !math && rest.starts_with("begin{") {
                    let name = &rest[6..rest.find('}').unwrap_or(6)];
                    found |= BLOCK_ENVS.contains(&name.trim_end_matches('*'));
                } else if !math && rest.starts_with('[') {
                    found = true;
                }
                i += 2;
                continue;
            }
            b'%' => {
                i += src[i..].find('\n').unwrap_or(b.len() - i);
            }
            b'$' => math = !math,
            b'{' => depth += 1,
            b'}' if depth == 0 => return found.then_some(i),
            b'}' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    None
}

/// Precomposed letters of the text accents (`\"o`, `\'e`, `\v{s}`, …): (accent, base, letter).
static TEXT_ACCENTS: &[(char, char, &str)] = &[
    ('`', 'A', "À"),
    ('`', 'a', "à"),
    ('`', 'E', "È"),
    ('`', 'e', "è"),
    ('`', 'I', "Ì"),
    ('`', 'i', "ì"),
    ('`', 'O', "Ò"),
    ('`', 'o', "ò"),
    ('`', 'U', "Ù"),
    ('`', 'u', "ù"),
    ('`', 'Y', "Ỳ"),
    ('`', 'y', "ỳ"),
    ('`', 'N', "Ǹ"),
    ('`', 'n', "ǹ"),
    ('`', 'W', "Ẁ"),
    ('`', 'w', "ẁ"),
    ('\'', 'A', "Á"),
    ('\'', 'a', "á"),
    ('\'', 'E', "É"),
    ('\'', 'e', "é"),
    ('\'', 'I', "Í"),
    ('\'', 'i', "í"),
    ('\'', 'O', "Ó"),
    ('\'', 'o', "ó"),
    ('\'', 'U', "Ú"),
    ('\'', 'u', "ú"),
    ('\'', 'Y', "Ý"),
    ('\'', 'y', "ý"),
    ('\'', 'C', "Ć"),
    ('\'', 'c', "ć"),
    ('\'', 'G', "Ǵ"),
    ('\'', 'g', "ǵ"),
    ('\'', 'K', "Ḱ"),
    ('\'', 'k', "ḱ"),
    ('\'', 'L', "Ĺ"),
    ('\'', 'l', "ĺ"),
    ('\'', 'N', "Ń"),
    ('\'', 'n', "ń"),
    ('\'', 'R', "Ŕ"),
    ('\'', 'r', "ŕ"),
    ('\'', 'S', "Ś"),
    ('\'', 's', "ś"),
    ('\'', 'Z', "Ź"),
    ('\'', 'z', "ź"),
    ('\'', 'W', "Ẃ"),
    ('\'', 'w', "ẃ"),
    ('^', 'A', "Â"),
    ('^', 'a', "â"),
    ('^', 'E', "Ê"),
    ('^', 'e', "ê"),
    ('^', 'I', "Î"),
    ('^', 'i', "î"),
    ('^', 'O', "Ô"),
    ('^', 'o', "ô"),
    ('^', 'U', "Û"),
    ('^', 'u', "û"),
    ('^', 'Y', "Ŷ"),
    ('^', 'y', "ŷ"),
    ('^', 'C', "Ĉ"),
    ('^', 'c', "ĉ"),
    ('^', 'G', "Ĝ"),
    ('^', 'g', "ĝ"),
    ('^', 'S', "Ŝ"),
    ('^', 's', "ŝ"),
    ('^', 'Z', "Ẑ"),
    ('^', 'z', "ẑ"),
    ('^', 'H', "Ĥ"),
    ('^', 'h', "ĥ"),
    ('^', 'J', "Ĵ"),
    ('^', 'j', "ĵ"),
    ('^', 'W', "Ŵ"),
    ('^', 'w', "ŵ"),
    ('"', 'A', "Ä"),
    ('"', 'a', "ä"),
    ('"', 'E', "Ë"),
    ('"', 'e', "ë"),
    ('"', 'I', "Ï"),
    ('"', 'i', "ï"),
    ('"', 'O', "Ö"),
    ('"', 'o', "ö"),
    ('"', 'U', "Ü"),
    ('"', 'u', "ü"),
    ('"', 'Y', "Ÿ"),
    ('"', 'y', "ÿ"),
    ('"', 't', "ẗ"),
    ('"', 'H', "Ḧ"),
    ('"', 'h', "ḧ"),
    ('"', 'W', "Ẅ"),
    ('"', 'w', "ẅ"),
    ('~', 'A', "Ã"),
    ('~', 'a', "ã"),
    ('~', 'E', "Ẽ"),
    ('~', 'e', "ẽ"),
    ('~', 'I', "Ĩ"),
    ('~', 'i', "ĩ"),
    ('~', 'O', "Õ"),
    ('~', 'o', "õ"),
    ('~', 'U', "Ũ"),
    ('~', 'u', "ũ"),
    ('~', 'Y', "Ỹ"),
    ('~', 'y', "ỹ"),
    ('~', 'N', "Ñ"),
    ('~', 'n', "ñ"),
    ('=', 'A', "Ā"),
    ('=', 'a', "ā"),
    ('=', 'E', "Ē"),
    ('=', 'e', "ē"),
    ('=', 'I', "Ī"),
    ('=', 'i', "ī"),
    ('=', 'O', "Ō"),
    ('=', 'o', "ō"),
    ('=', 'U', "Ū"),
    ('=', 'u', "ū"),
    ('=', 'Y', "Ȳ"),
    ('=', 'y', "ȳ"),
    ('=', 'G', "Ḡ"),
    ('=', 'g', "ḡ"),
    ('.', 'A', "Ȧ"),
    ('.', 'a', "ȧ"),
    ('.', 'E', "Ė"),
    ('.', 'e', "ė"),
    ('.', 'I', "İ"),
    ('.', 'O', "Ȯ"),
    ('.', 'o', "ȯ"),
    ('.', 'Y', "Ẏ"),
    ('.', 'y', "ẏ"),
    ('.', 'C', "Ċ"),
    ('.', 'c', "ċ"),
    ('.', 'G', "Ġ"),
    ('.', 'g', "ġ"),
    ('.', 'N', "Ṅ"),
    ('.', 'n', "ṅ"),
    ('.', 'R', "Ṙ"),
    ('.', 'r', "ṙ"),
    ('.', 'S', "Ṡ"),
    ('.', 's', "ṡ"),
    ('.', 'T', "Ṫ"),
    ('.', 't', "ṫ"),
    ('.', 'Z', "Ż"),
    ('.', 'z', "ż"),
    ('.', 'D', "Ḋ"),
    ('.', 'd', "ḋ"),
    ('.', 'H', "Ḣ"),
    ('.', 'h', "ḣ"),
    ('.', 'W', "Ẇ"),
    ('.', 'w', "ẇ"),
    ('u', 'A', "Ă"),
    ('u', 'a', "ă"),
    ('u', 'E', "Ĕ"),
    ('u', 'e', "ĕ"),
    ('u', 'I', "Ĭ"),
    ('u', 'i', "ĭ"),
    ('u', 'O', "Ŏ"),
    ('u', 'o', "ŏ"),
    ('u', 'U', "Ŭ"),
    ('u', 'u', "ŭ"),
    ('u', 'G', "Ğ"),
    ('u', 'g', "ğ"),
    ('v', 'A', "Ǎ"),
    ('v', 'a', "ǎ"),
    ('v', 'E', "Ě"),
    ('v', 'e', "ě"),
    ('v', 'I', "Ǐ"),
    ('v', 'i', "ǐ"),
    ('v', 'O', "Ǒ"),
    ('v', 'o', "ǒ"),
    ('v', 'U', "Ǔ"),
    ('v', 'u', "ǔ"),
    ('v', 'C', "Č"),
    ('v', 'c', "č"),
    ('v', 'G', "Ǧ"),
    ('v', 'g', "ǧ"),
    ('v', 'K', "Ǩ"),
    ('v', 'k', "ǩ"),
    ('v', 'L', "Ľ"),
    ('v', 'l', "ľ"),
    ('v', 'N', "Ň"),
    ('v', 'n', "ň"),
    ('v', 'R', "Ř"),
    ('v', 'r', "ř"),
    ('v', 'S', "Š"),
    ('v', 's', "š"),
    ('v', 'T', "Ť"),
    ('v', 't', "ť"),
    ('v', 'Z', "Ž"),
    ('v', 'z', "ž"),
    ('v', 'D', "Ď"),
    ('v', 'd', "ď"),
    ('v', 'H', "Ȟ"),
    ('v', 'h', "ȟ"),
    ('v', 'j', "ǰ"),
    ('H', 'O', "Ő"),
    ('H', 'o', "ő"),
    ('H', 'U', "Ű"),
    ('H', 'u', "ű"),
    ('c', 'E', "Ȩ"),
    ('c', 'e', "ȩ"),
    ('c', 'C', "Ç"),
    ('c', 'c', "ç"),
    ('c', 'G', "Ģ"),
    ('c', 'g', "ģ"),
    ('c', 'K', "Ķ"),
    ('c', 'k', "ķ"),
    ('c', 'L', "Ļ"),
    ('c', 'l', "ļ"),
    ('c', 'N', "Ņ"),
    ('c', 'n', "ņ"),
    ('c', 'R', "Ŗ"),
    ('c', 'r', "ŗ"),
    ('c', 'S', "Ş"),
    ('c', 's', "ş"),
    ('c', 'T', "Ţ"),
    ('c', 't', "ţ"),
    ('c', 'D', "Ḑ"),
    ('c', 'd', "ḑ"),
    ('c', 'H', "Ḩ"),
    ('c', 'h', "ḩ"),
    ('r', 'A', "Å"),
    ('r', 'a', "å"),
    ('r', 'U', "Ů"),
    ('r', 'u', "ů"),
    ('r', 'y', "ẙ"),
    ('r', 'w', "ẘ"),
    ('k', 'A', "Ą"),
    ('k', 'a', "ą"),
    ('k', 'E', "Ę"),
    ('k', 'e', "ę"),
    ('k', 'I', "Į"),
    ('k', 'i', "į"),
    ('k', 'O', "Ǫ"),
    ('k', 'o', "ǫ"),
    ('k', 'U', "Ų"),
    ('k', 'u', "ų"),
];

/// The precomposed letter for accent `accent` over `base`.
fn compose_accent(accent: char, base: char) -> Option<&'static str> {
    TEXT_ACCENTS.iter().find(|&&(a, b, _)| a == accent && b == base).map(|&(_, _, s)| s)
}

#[inline(always)]
fn is_single_escape(c: char) -> bool {
    (c as u32) < 256 && SINGLE_ESCAPE_LUT[c as usize]
}

/// Whether a top-level declaration in `v` outlives the paragraph (text colour, size).
fn has_lasting_declaration(v: &[Inline<'_>]) -> bool {
    v.iter().any(|i| matches!(i, Inline::Color(_) | Inline::FontSize(_)))
}

fn is_empty_inlines(v: &[Inline<'_>]) -> bool {
    v.iter().all(|i| match i {
        Inline::Text(s) => s.trim().is_empty(),
        Inline::VAdjust { .. } => false,
        Inline::ShortTitleEnd
        | Inline::ItalicCorrection
        | Inline::PageLabel(_)
        | Inline::Break
        | Inline::ParEnd
        | Inline::Color(_)
        | Inline::FontSize(_)
        | Inline::ScShape
        | Inline::FontDecl(_) | Inline::TextFontCommand
        | Inline::SpaceFactor(_)
        | Inline::NoIndent
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
        | Inline::HSpace(_)
        | Inline::HGlue(..)
        | Inline::HFil(..)
        | Inline::Unskip
        | Inline::DisplayFollows
        | Inline::DisplayResumes
        | Inline::EmSpace { .. }
        | Inline::FontDimenKern(_)
        | Inline::Kern(_) => true,
        Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x) => {
            is_empty_inlines(x)
        }
        Inline::Math(_) | Inline::Footnote(_) | Inline::Logo(_) => false,
    })
}

/// The alignment a `\centering`, `\raggedright` or `\raggedleft` declaration selects.
fn align_of_declaration(cmd: &str) -> ParAlign {
    match cmd {
        "centering" => ParAlign::Center,
        "raggedleft" => ParAlign::RaggedLeft,
        _ => ParAlign::RaggedRight,
    }
}

fn suggest_command(name: &str) -> Option<&'static str> {
    const KNOWN_COMMANDS: &[&str] = &[
        "textbf",
        "textit",
        "texttt",
        "emph",
        "verb",
        "par",
        "hspace",
        "hfill",
        "noindent",
        "indent",
        "frenchspacing",
        "nonfrenchspacing",
        "hangindent",
        "hangafter",
        "parshape",
        "pretolerance",
        "tolerance",
        "emergencystretch",
        "looseness",
        "leftskip",
        "rightskip",
        "parfillskip",
        "prevgraf",
        "lastlinefit",
        "sloppy",
        "fussy",
        "textbackslash",
        "textasciitilde",
        "textunderscore",
        "textasciicircum",
        "section",
        "subsection",
        "subsubsection",
        "maketitle",
        "newpage",
        "clearpage",
        "bigskip",
        "medskip",
        "smallskip",
        "hrule",
        "vspace",
        "begin",
        "end",
        "item",
        "title",
        "author",
        "date",
        "documentclass",
        "usepackage",
        "includegraphics",
        "centering",
        "textsc",
        "scshape",
        "tiny",
        "scriptsize",
        "footnotesize",
        "small",
        "normalsize",
        "large",
        "Large",
        "LARGE",
        "huge",
        "Huge",
        "color",
        "textcolor",
        "definecolor",
        "raggedright",
        "raggedleft",
        "caption",
        "label",
        "ref",
        "cite",
        "tableofcontents",
        "hline",
        "textbullet",
        "textendash",
        "textemdash",
    ];

    let mut best_match = None;
    let mut best_dist = usize::MAX;

    for &cmd in KNOWN_COMMANDS {
        let dist = edit_distance(name, cmd);
        if dist <= 2 && dist < best_dist {
            best_dist = dist;
            best_match = Some(cmd);
        }
    }

    best_match
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let mut prev = (0..=b_bytes.len()).collect::<Vec<_>>();
    let mut curr = vec![0; b_bytes.len() + 1];

    for i in 1..=a_bytes.len() {
        curr[0] = i;
        for j in 1..=b_bytes.len() {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (curr[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        prev.copy_from_slice(&curr);
    }
    prev[b_bytes.len()]
}

/// Whether a source line ends a display: `\]`, `$$` or the end of a display math
/// environment.
fn ends_display(line: &str) -> bool {
    let line = line.trim_end();
    if line.ends_with("\\]") || line.ends_with("$$") {
        return true;
    }
    let Some(name) = line.strip_suffix('}').and_then(|l| l.rsplit_once("\\end{")).map(|(_, n)| n) else {
        return false;
    };
    matches!(
        name.trim_end_matches('*'),
        "equation" | "align" | "alignat" | "gather" | "multline" | "flalign" | "eqnarray" | "displaymath"
    )
}
