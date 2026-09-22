//! # TeX Syntax Lexer & Recursive-Descent Parser (`parser`)

use super::frontend_ast::*;
use crate::config::Preamble;

pub fn parse(source_code: &str) -> Result<Document<'_>, ParseError> {
    let mut parser = Parser::new(source_code);
    let has_document_environment = parser.parse_preamble_if_present()?;
    let blocks = parser.parse_blocks()?;
    if has_document_environment {
        if !parser.eat_end_env("document") {
            let doc_start = parser.doc_env_start.unwrap_or(0);
            return Err(parser.err_span(
                doc_start,
                "\\begin{document}".len(),
                "unclosed \\begin{document}",
                Some("this '\\begin{document}' was never closed"),
                Some("reached end of file without finding matching '\\end{document}'"),
                Some("add '\\end{document}' at the end of your file"),
            ));
        }
    } else {
        parser.skip_ws();
        if parser.peek().is_some() && !parser.starts_with("\\end") {
            return Err(parser.err("trailing unparsed input"));
        }
    }
    Ok(Document {
        preamble: parser.preamble,
        blocks,
    })
}

pub(crate) struct Parser<'a> {
    pub(crate) source_code: &'a str,
    pub(crate) byte_offset: usize,
    pub(crate) preamble: Preamble<'a>,
    pub(crate) doc_env_start: Option<usize>,
}

impl<'a> Parser<'a> {
    fn new(source_code: &'a str) -> Self {
        Self {
            source_code,
            byte_offset: 0,
            preamble: Preamble::default(),
            doc_env_start: None,
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
    fn rest(&self) -> &'a str {
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
        while idx < bytes.len() {
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

    pub(crate) fn skip_hws(&mut self) {
        let bytes = self.rest().as_bytes();
        let mut idx = 0;
        while idx < bytes.len() {
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
        } else {
            self.byte_offset = self.source_code.len();
        }
    }

    pub(crate) fn at_blank_line(&self) -> bool {
        let r = self.rest();
        r.starts_with("\n\n") || r.starts_with("\r\n\r\n") || r.starts_with("\n\r\n")
    }

    pub(crate) fn cmd_ends_at(&self, cmd: &str) -> bool {
        if !self.starts_with(cmd) {
            return false;
        }
        !matches!(
            self.source_code[self.byte_offset + cmd.len()..].chars().next(),
            Some(c) if c.is_ascii_alphabetic()
        )
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
            if self.starts_with("[") {
                let _ = self.parse_optional_brackets();
            }
            self.skip_hws();
            if self.starts_with("{") {
                let _ = self.require_group();
            }
        } else {
            self.bump();
        }
        Ok(())
    }

    fn parse_preamble_if_present(&mut self) -> Result<bool, ParseError> {
        let registry = crate::doc_config::PackageRegistry::default();
        let mut in_preamble = false;

        loop {
            self.skip_ws();
            if self.peek().is_none() {
                return Ok(false);
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
                let name = self.brace_ident()?;
                let pkg = crate::doc_config::Package { name, options };
                registry.apply(&pkg, &mut self.preamble.config);
                self.preamble.packages.push(pkg);
                continue;
            }
            if self.cmd_ends_at("\\title") {
                in_preamble = true;
                self.eat("\\title");
                self.preamble.title = Some(self.require_group()?);
                continue;
            }
            if self.cmd_ends_at("\\author") {
                in_preamble = true;
                self.eat("\\author");
                self.preamble.author = Some(self.require_group()?);
                continue;
            }
            if self.cmd_ends_at("\\date") {
                in_preamble = true;
                self.eat("\\date");
                self.preamble.date = Some(self.require_group()?);
                continue;
            }
            if self.cmd_ends_at("\\pagestyle") {
                in_preamble = true;
                self.eat("\\pagestyle");
                let style = self.brace_ident()?;
                if style == "empty" {
                    self.preamble.config.page_numbering = false;
                } else if style == "plain" {
                    self.preamble.config.page_numbering = true;
                }
                continue;
            }
            if self.cmd_ends_at("\\setlength") {
                in_preamble = true;
                self.eat("\\setlength");
                let param = self.brace_arg()?;
                let val_str = self.brace_arg()?;
                if let Some(dim) = crate::doc_config::parse_dimension_pt(val_str) {
                    let key = param.trim().trim_start_matches('\\');
                    match key {
                        "parindent" => self.preamble.config.par_indent = dim,
                        "parskip" => self.preamble.config.par_skip = dim,
                        "hangindent" => self.preamble.config.hang_indent = dim,
                        "hangafter" => self.preamble.config.hang_after = dim as i32,
                        "leftskip" => self.preamble.config.left_skip = dim,
                        "rightskip" => self.preamble.config.right_skip = dim,
                        "labelsep" => self.preamble.config.list_label_sep = dim,
                        "leftmargin" => self.preamble.config.list_left_margin = dim,
                        "itemindent" => self.preamble.config.list_item_indent = dim,
                        "itemsep" => self.preamble.config.list_item_sep = dim,
                        "listparindent" => self.preamble.config.list_par_indent = dim,
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
                    } else if val.contains("rmdefault") {
                        self.preamble.config.font_family = crate::font_metrics::FontFamily::Times;
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

    fn parse_blocks(&mut self) -> Result<Vec<Block<'a>>, ParseError> {
        let mut blocks = Vec::with_capacity(32);
        loop {
            self.skip_ws();
            if self.peek().is_none() {
                break;
            }
            if self.at_item() || self.starts_with("\\end") {
                break;
            }
            if self.starts_with("\\includegraphics") || self.cmd_ends_at("\\caption") {
                break;
            }
            if self.cmd_ends_at("\\maketitle") {
                self.eat("\\maketitle");
                if let Some(title) = &self.preamble.title {
                    blocks.push(Block::TitleBlock(Box::new(TitleBlockData {
                        title: title.clone(),
                        author: self.preamble.author.clone(),
                        date: self.preamble.date.clone(),
                    })));
                }
                continue;
            }
            if self.cmd_ends_at("\\pagestyle") {
                self.eat("\\pagestyle");
                let style = self.brace_ident()?;
                if style == "empty" {
                    self.preamble.config.page_numbering = false;
                } else if style == "plain" {
                    self.preamble.config.page_numbering = true;
                }
                continue;
            }
            if self.cmd_ends_at("\\thispagestyle") {
                self.eat("\\thispagestyle");
                let _ = self.brace_ident()?;
                continue;
            }
            if self.cmd_ends_at("\\setlength") {
                self.eat("\\setlength");
                let param = self.brace_arg()?;
                let val_str = self.brace_arg()?;
                if let Some(dim) = crate::doc_config::parse_dimension_pt(val_str) {
                    let key = param.trim().trim_start_matches('\\');
                    match key {
                        "parindent" => {
                            blocks.push(Block::ConfigChange(ConfigChange::ParIndent(dim)))
                        }
                        "parskip" => blocks.push(Block::ConfigChange(ConfigChange::ParSkip(dim))),
                        "hangindent" => {
                            blocks.push(Block::ConfigChange(ConfigChange::HangIndent(dim)))
                        }
                        "hangafter" => {
                            blocks.push(Block::ConfigChange(ConfigChange::HangAfter(dim as i32)))
                        }
                        "leftskip" => blocks.push(Block::ConfigChange(ConfigChange::LeftSkip(dim))),
                        "rightskip" => {
                            blocks.push(Block::ConfigChange(ConfigChange::RightSkip(dim)))
                        }
                        "labelsep" => blocks.push(Block::ConfigChange(ConfigChange::LabelSep(dim))),
                        "leftmargin" => {
                            blocks.push(Block::ConfigChange(ConfigChange::LeftMargin(dim)))
                        }
                        "itemindent" => {
                            blocks.push(Block::ConfigChange(ConfigChange::ItemIndent(dim)))
                        }
                        "itemsep" => blocks.push(Block::ConfigChange(ConfigChange::ItemSep(dim))),
                        "listparindent" => {
                            blocks.push(Block::ConfigChange(ConfigChange::ListParIndent(dim)))
                        }
                        _ => {}
                    }
                }
                continue;
            }
            if self.cmd_ends_at("\\hangindent") {
                if let Some(dim) = self.parse_raw_dimen_command("\\hangindent")? {
                    blocks.push(Block::ConfigChange(ConfigChange::HangIndent(dim)));
                }
                continue;
            }
            if self.cmd_ends_at("\\hangafter") {
                if let Some(dim) = self.parse_raw_dimen_command("\\hangafter")? {
                    blocks.push(Block::ConfigChange(ConfigChange::HangAfter(dim as i32)));
                }
                continue;
            }
            if self.cmd_ends_at("\\parindent") {
                if let Some(dim) = self.parse_raw_dimen_command("\\parindent")? {
                    blocks.push(Block::ConfigChange(ConfigChange::ParIndent(dim)));
                }
                continue;
            }
            if self.cmd_ends_at("\\parskip") {
                if let Some(dim) = self.parse_raw_dimen_command("\\parskip")? {
                    blocks.push(Block::ConfigChange(ConfigChange::ParSkip(dim)));
                }
                continue;
            }
            if self.cmd_ends_at("\\leftskip") {
                if let Some(dim) = self.parse_raw_dimen_command("\\leftskip")? {
                    blocks.push(Block::ConfigChange(ConfigChange::LeftSkip(dim)));
                }
                continue;
            }
            if self.cmd_ends_at("\\rightskip") {
                if let Some(dim) = self.parse_raw_dimen_command("\\rightskip")? {
                    blocks.push(Block::ConfigChange(ConfigChange::RightSkip(dim)));
                }
                continue;
            }
            if self.cmd_ends_at("\\labelsep") {
                if let Some(dim) = self.parse_raw_dimen_command("\\labelsep")? {
                    blocks.push(Block::ConfigChange(ConfigChange::LabelSep(dim)));
                }
                continue;
            }
            if self.cmd_ends_at("\\leftmargin") {
                if let Some(dim) = self.parse_raw_dimen_command("\\leftmargin")? {
                    blocks.push(Block::ConfigChange(ConfigChange::LeftMargin(dim)));
                }
                continue;
            }
            if self.cmd_ends_at("\\itemindent") {
                if let Some(dim) = self.parse_raw_dimen_command("\\itemindent")? {
                    blocks.push(Block::ConfigChange(ConfigChange::ItemIndent(dim)));
                }
                continue;
            }
            if self.cmd_ends_at("\\itemsep") {
                if let Some(dim) = self.parse_raw_dimen_command("\\itemsep")? {
                    blocks.push(Block::ConfigChange(ConfigChange::ItemSep(dim)));
                }
                continue;
            }
            if self.cmd_ends_at("\\newpage") {
                self.eat("\\newpage");
                blocks.push(Block::PageBreak);
                continue;
            }
            if self.cmd_ends_at("\\clearpage") {
                self.eat("\\clearpage");
                blocks.push(Block::PageBreak);
                continue;
            }
            if self.cmd_ends_at("\\bigskip") {
                self.eat("\\bigskip");
                blocks.push(Block::VSpace(12.0));
                continue;
            }
            if self.cmd_ends_at("\\medskip") {
                self.eat("\\medskip");
                blocks.push(Block::VSpace(6.0));
                continue;
            }
            if self.cmd_ends_at("\\smallskip") {
                self.eat("\\smallskip");
                blocks.push(Block::VSpace(3.0));
                continue;
            }
            if self.cmd_ends_at("\\hrule") {
                self.eat("\\hrule");
                blocks.push(Block::HRule);
                continue;
            }
            if self.cmd_ends_at("\\vspace") {
                self.eat("\\vspace");
                let _ = self.eat("*");
                let val_str = self.brace_arg()?;
                let dim = crate::doc_config::parse_dimension_pt(val_str).unwrap_or(12.0);
                blocks.push(Block::VSpace(dim));
                continue;
            }
            if self.cmd_ends_at("\\section") {
                self.eat("\\section");
                let starred = self.eat("*");
                let title = self.require_group()?;
                blocks.push(Block::Section {
                    level: 1,
                    starred,
                    title,
                });
                continue;
            }
            if self.cmd_ends_at("\\subsection") {
                self.eat("\\subsection");
                let starred = self.eat("*");
                let title = self.require_group()?;
                blocks.push(Block::Section {
                    level: 2,
                    starred,
                    title,
                });
                continue;
            }
            if self.cmd_ends_at("\\subsubsection") {
                self.eat("\\subsubsection");
                let starred = self.eat("*");
                let title = self.require_group()?;
                blocks.push(Block::Section {
                    level: 3,
                    starred,
                    title,
                });
                continue;
            }
            if self.cmd_ends_at("\\tableofcontents") {
                self.eat("\\tableofcontents");
                blocks.push(Block::TableOfContents);
                continue;
            }
            if self.cmd_ends_at("\\listoffigures") {
                self.eat("\\listoffigures");
                blocks.push(Block::ListOfFigures);
                continue;
            }
            if self.cmd_ends_at("\\listoftables") {
                self.eat("\\listoftables");
                blocks.push(Block::ListOfTables);
                continue;
            }
            if self.starts_with("$$") {
                let open_offset = self.byte_offset;
                self.eat("$$");
                let start = self.byte_offset;
                let rest = self.rest();
                if let Some(pos) = rest.find("$$") {
                    let math_slice = &self.source_code[start..start + pos];
                    self.byte_offset = start + pos + 2;
                    let math = crate::math::parse_math(math_slice);
                    blocks.push(Block::Equation {
                        starred: true,
                        math: Box::new(math),
                    });
                    continue;
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
            if self.starts_with("\\[") {
                let open_offset = self.byte_offset;
                self.eat("\\[");
                let start = self.byte_offset;
                let rest = self.rest();
                if let Some(pos) = rest.find("\\]") {
                    let math_slice = &self.source_code[start..start + pos];
                    self.byte_offset = start + pos + 2;
                    let math = crate::math::parse_math(math_slice);
                    blocks.push(Block::Equation {
                        starred: true,
                        math: Box::new(math),
                    });
                    continue;
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
            if self.starts_with("\\begin") {
                blocks.push(self.parse_env()?);
            } else {
                let inls = self.parse_para_inlines()?;
                if !is_empty_inlines(&inls) {
                    blocks.push(Block::Para(inls));
                } else if self.peek().is_some()
                    && !self.starts_with("\\begin")
                    && !self.starts_with("\\end")
                    && !self.at_item()
                    && !self.starts_with("$$")
                    && !self.starts_with("\\[")
                {
                    break;
                }
            }
        }
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
            "quote" => self.parse_quote(false, env_start),
            "quotation" => self.parse_quote(true, env_start),
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
            other => Err(self.err_span(
                env_start,
                "\\begin{".len() + other.len() + 1,
                format!("unsupported environment {{{other}}}"),
                Some("unsupported environment"),
                None,
                Some("supported environments: enumerate, itemize, description, quote, quotation, tabular, longtable, table, figure, verbatim, lstlisting, equation, equation*, displaymath, align, align*, gather, gather*, multline, multline*"),
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
            let math = crate::math::parse_math(math_slice);
            Ok(Block::Equation {
                starred,
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
            let math = crate::math::parse_math(math_slice);
            Ok(Block::Equation {
                starred,
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
            Ok(Block::CodeListing(Box::new(
                crate::code_listing::CodeListingData::new(code_slice),
            )))
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
            return Ok(crate::doc_config::parse_dimension_pt(s));
        }
        let start = self.byte_offset;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
        {
            self.bump();
        }
        let s = &self.source_code[start..self.byte_offset];
        Ok(crate::doc_config::parse_dimension_pt(s))
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
            let custom_label = if self.starts_with("[") {
                let open_offset = self.byte_offset;
                self.bump();
                let start_offset = self.byte_offset;
                let mut brace_depth = 0;
                let mut end_offset = None;
                while let Some(ch) = self.peek() {
                    match ch {
                        '{' => {
                            brace_depth += 1;
                            self.bump();
                        }
                        '}' => {
                            if brace_depth > 0 {
                                brace_depth -= 1;
                            }
                            self.bump();
                        }
                        ']' if brace_depth == 0 => {
                            end_offset = Some(self.byte_offset);
                            self.bump();
                            break;
                        }
                        _ => {
                            self.bump();
                        }
                    }
                }
                let end = match end_offset {
                    Some(e) => e,
                    None => {
                        return Err(self.err_span(
                            open_offset,
                            1,
                            "unclosed custom item label '['",
                            Some("opening '[' starts here"),
                            Some("reached end of file without finding matching ']'"),
                            Some("add closing ']' to terminate custom label"),
                        ));
                    }
                };
                let label_slice = &self.source_code[start_offset..end];
                let mut sub_parser = Parser::new(label_slice);
                let inls = sub_parser.parse_para_inlines()?;
                self.skip_hws();
                Some(inls)
            } else {
                None
            };
            let blocks = self.parse_blocks()?;
            items.push(ListItem {
                custom_label,
                blocks,
            });
        }
        Ok(Block::List(Box::new(ListData {
            kind,
            options,
            items,
        })))
    }

    fn parse_quote(
        &mut self,
        is_quotation: bool,
        open_offset: usize,
    ) -> Result<Block<'a>, ParseError> {
        let name = if is_quotation { "quotation" } else { "quote" };
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
        Ok(Block::Quote(Box::new(QuoteData {
            is_quotation,
            blocks,
        })))
    }

    fn parse_figure(&mut self, open_offset: usize) -> Result<Block<'a>, ParseError> {
        self.skip_hws();
        if self.eat("[") {
            while !matches!(self.peek(), Some(']') | None) {
                self.bump();
            }
            self.eat("]");
        }
        let mut graphic = None;
        let mut caption = Vec::with_capacity(8);
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
                caption = self.require_group()?;
                continue;
            }
            // ignore extra paragraph text inside figure for v1
            let _ = self.parse_para_inlines()?;
        }
        Ok(Block::Figure(Box::new(FigureData { graphic, caption })))
    }

    fn parse_graphic(&mut self) -> Result<Graphic<'a>, ParseError> {
        self.skip_hws();
        if self.eat("[") {
            let opt_start = self.byte_offset - 1;
            while !matches!(self.peek(), Some(']') | None) {
                self.bump();
            }
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
        Ok(Graphic { path })
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

    pub(crate) fn brace_arg(&mut self) -> Result<&'a str, ParseError> {
        self.skip_hws();
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
        loop {
            match self.peek() {
                Some('}') => {
                    let arg = &self.source_code[start..self.byte_offset];
                    self.bump();
                    return Ok(arg.trim());
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
        let mut out = Vec::with_capacity(16);
        loop {
            match self.peek() {
                None => break,
                Some('%') => self.skip_comment(),
                Some('\n') if self.at_blank_line() => break,
                Some('\\' | '$') => {
                    if self.starts_with("\\begin")
                        || self.starts_with("\\end")
                        || self.at_item()
                        || self.cmd_ends_at("\\section")
                        || self.cmd_ends_at("\\subsection")
                        || self.cmd_ends_at("\\subsubsection")
                        || self.cmd_ends_at("\\maketitle")
                        || self.cmd_ends_at("\\newpage")
                        || self.cmd_ends_at("\\clearpage")
                        || self.cmd_ends_at("\\bigskip")
                        || self.cmd_ends_at("\\medskip")
                        || self.cmd_ends_at("\\smallskip")
                        || self.cmd_ends_at("\\hrule")
                        || self.cmd_ends_at("\\tableofcontents")
                        || self.cmd_ends_at("\\listoffigures")
                        || self.cmd_ends_at("\\listoftables")
                        || self.cmd_ends_at("\\vspace")
                        || self.starts_with("$$")
                        || self.starts_with("\\[")
                    {
                        break;
                    }
                    if self.starts_with("\\\\") {
                        self.eat("\\\\");
                        out.push(Inline::Break);
                        self.skip_hws();
                    } else {
                        out.push(self.parse_inline()?);
                    }
                }
                Some(_) => out.push(self.parse_inline()?),
            }
        }
        Ok(out)
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
                    let math = crate::math::parse_math(math_slice);
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
                self.bump();
                Ok(Inline::Group(self.parse_until_rbrace()?))
            }
            Some('&') => {
                // alignment tab belongs to tabular; if it leaks here, keep it
                // as a one-char token so we cannot spin on an empty text run.
                let start = self.byte_offset;
                self.bump();
                Ok(Inline::Text(&self.source_code[start..self.byte_offset]))
            }
            Some(_) => Ok(Inline::Text(self.parse_text_run())),
            None => Err(self.err("unexpected eof")),
        }
    }

    fn parse_until_rbrace(&mut self) -> Result<Vec<Inline<'a>>, ParseError> {
        let open_offset = self.byte_offset.saturating_sub(1);
        let mut inner = Vec::with_capacity(4);
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
                    let math = crate::math::parse_math(math_slice);
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
                    let math = crate::math::parse_math(math_slice);
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
            if is_single_escape(c) {
                let start = self.byte_offset;
                self.bump();
                return Ok(Inline::Text(&self.source_code[start..self.byte_offset]));
            }
        }
        let start = self.byte_offset;
        while matches!(self.peek(), Some(ch) if ch.is_ascii_alphabetic()) {
            self.bump();
        }
        let name = &self.source_code[start..self.byte_offset];
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
        self.skip_hws();
        // Fast-path for ubiquitous formatting commands (instruction-cache and branch predictor optimal)
        match name.as_bytes() {
            b"textbf" => return Ok(Inline::Bold(self.require_group()?)),
            b"textit" | b"emph" => return Ok(Inline::Italic(self.require_group()?)),
            b"texttt" => return Ok(Inline::Mono(self.require_group()?)),
            b"par" => return Ok(Inline::Break),
            b"noindent" => return Ok(Inline::NoIndent),
            b"indent" => return Ok(Inline::Indent),
            b"verb" => return self.parse_verb(cmd_start),
            _ => {}
        }
        match name {
            "textbf" => Ok(Inline::Bold(self.require_group()?)),
            "textit" | "emph" => Ok(Inline::Italic(self.require_group()?)),
            "texttt" => Ok(Inline::Mono(self.require_group()?)),
            "verb" => self.parse_verb(cmd_start),
            "par" => Ok(Inline::Break),
            "noindent" => Ok(Inline::NoIndent),
            "indent" => Ok(Inline::Indent),
            "hangindent" => {
                self.skip_hws();
                let _ = self.eat("=");
                self.skip_hws();
                let dim = if self.starts_with("{") {
                    let s = self.brace_arg()?;
                    crate::doc_config::parse_dimension_pt(s).unwrap_or(0.0)
                } else {
                    let start = self.byte_offset;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
                    {
                        self.bump();
                    }
                    let s = &self.source_code[start..self.byte_offset];
                    crate::doc_config::parse_dimension_pt(s).unwrap_or(0.0)
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
            "hspace" => {
                let _ = self.eat("*");
                self.skip_hws();
                let dim = if self.starts_with("{") {
                    let s = self.brace_arg()?;
                    crate::doc_config::parse_dimension_pt(s).unwrap_or(0.0)
                } else {
                    let start = self.byte_offset;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
                    {
                        self.bump();
                    }
                    let s = &self.source_code[start..self.byte_offset];
                    crate::doc_config::parse_dimension_pt(s).unwrap_or(0.0)
                };
                Ok(Inline::HSpace(dim))
            }
            "textbackslash" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("\\"))
            }
            "textasciitilde" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("~"))
            }
            "textasciicircum" => {
                let _ = self.eat("{}");
                Ok(Inline::Text("^"))
            }
            "textbullet" => {
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

    pub(crate) fn require_group(&mut self) -> Result<Vec<Inline<'a>>, ParseError> {
        self.skip_hws();
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

    /// Contiguous source slice. Newlines stay in the slice; layout treats
    /// them as space. A `%` or command ends the run so the slice stays
    /// untransformed.
    fn parse_text_run(&mut self) -> &'a str {
        let bytes = self.source_code.as_bytes();
        let start = self.byte_offset;
        let mut i = start;
        while i < bytes.len() {
            let b = bytes[i];
            match b {
                b'\\' | b'{' | b'}' | b'%' | b'&' | b'$' => break,
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
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == target as u8 {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn is_single_escape(c: char) -> bool {
    matches!(
        c,
        '%' | '{' | '}' | '\\' | '#' | '&' | '_' | '$' | '~' | '^'
    )
}

fn is_empty_inlines(v: &[Inline<'_>]) -> bool {
    v.iter().all(|i| match i {
        Inline::Text(s) => s.trim().is_empty(),
        Inline::Break
        | Inline::NoIndent
        | Inline::Indent
        | Inline::HangIndent(_)
        | Inline::HangAfter(_)
        | Inline::HSpace(_) => true,
        Inline::Bold(x) | Inline::Italic(x) | Inline::Mono(x) | Inline::Group(x) => {
            is_empty_inlines(x)
        }
        Inline::Math(_) => false,
    })
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
        "noindent",
        "indent",
        "hangindent",
        "hangafter",
        "textbackslash",
        "textasciitilde",
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
