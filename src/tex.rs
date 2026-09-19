//! # TeX Syntax Lexer & Recursive-Descent Parser (`tex`)
//!
//! Provides a zero-copy lexical scanner and recursive-descent parser for LaTeX document syntax.
//!
//! ## Architectural Principles
//!
//! - **Zero-Copy AST Construction**: Text nodes ([`Inline::Text`]) are direct slices (`&'a str`)
//!   of the input source string. No string copying or duplicate memory allocations occur.
//! - **Single-Pass Parsing**: Syntax is parsed in a single sequential sweep without backtracking
//!   or multiple passes over the input buffer.
//! - **Extensible Grammar Support**:
//!   - **Document Structure**: `\documentclass`, `\usepackage`, `\begin{document}...\end{document}`.
//!   - **Headings & Titles**: `\title`, `\author`, `\date`, `\maketitle`, `\section`, `\subsection`.
//!   - **Typography**: `\textbf{...}`, `\textit{...}`, `\emph{...}`, scoped `{...}` blocks.
//!   - **Lists**: `\begin{enumerate}` (ordered) and `\begin{itemize}` (unordered) with nested `\item`.
//!   - **Tabular Data**: `\begin{tabular}{|l|c|r|}` with horizontal rules `\hline`, column separators `&`,
//!     and row terminators `\\`.
//!   - **Floating Figures**: `\begin{figure}` with `\includegraphics{...}` and `\caption{...}`.
//!   - **Escapes & Breaks**: `\\`, `\{`, `\}`, `\%`, `\ `, `\n\n` (paragraph separation).
//!
//! ## Public APIs
//!
//! - [`parse`]: Parses a LaTeX source string into a [`Document`] Abstract Syntax Tree.
//! - [`Document`]: Root AST container holding resolved preamble and block elements.
//! - [`Block`]: Top-level layout elements (`Para`, `List`, `Table`, `Figure`, `TitleBlock`, `Section`).
//! - [`Inline`]: Inline text elements (`Text`, `Bold`, `Italic`, `Group`, `Break`).
//! - [`ParseError`]: Syntax error reporting 1-based line, column, and message.

use crate::preamble::Preamble;

#[derive(Debug, Clone)]
pub struct Document<'a> {
    pub preamble: Preamble<'a>,
    pub blocks: Vec<Block<'a>>,
}

#[derive(Debug, Clone)]
pub enum Block<'a> {
    Para(Vec<Inline<'a>>),
    List {
        ordered: bool,
        items: Vec<Vec<Block<'a>>>,
    },
    Table {
        cols: Vec<ColSpec>,
        rows: Vec<TableRow<'a>>,
    },
    Figure {
        graphic: Option<Graphic<'a>>,
        caption: Vec<Inline<'a>>,
    },
    TitleBlock {
        title: Vec<Inline<'a>>,
        author: Option<Vec<Inline<'a>>>,
        date: Option<Vec<Inline<'a>>>,
    },
    Section {
        level: u8,
        title: Vec<Inline<'a>>,
    },
    PageBreak,
    VSpace(f32),
    HRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy)]
pub struct ColSpec {
    pub align: ColAlign,
    pub rule_left: bool,
}

#[derive(Debug, Clone)]
pub struct TableRow<'a> {
    pub rule_above: bool,
    pub cells: Vec<Vec<Inline<'a>>>,
}

#[derive(Debug, Clone, Copy)]
pub struct Graphic<'a> {
    pub path: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline<'a> {
    Text(&'a str),
    Bold(Vec<Inline<'a>>),
    Italic(Vec<Inline<'a>>),
    Group(Vec<Inline<'a>>),
    Break,
    NoIndent,
}

pub type ParseError = crate::diagnostic::Diagnostic;

pub fn parse(source_code: &str) -> Result<Document<'_>, ParseError> {
    let mut parser = Parser::new(source_code);
    let has_document_environment = parser.parse_preamble_if_present()?;
    let blocks = parser.parse_blocks()?;
    if has_document_environment {
        let _ = parser.eat_end_env("document");
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

struct Parser<'a> {
    source_code: &'a str,
    byte_offset: usize,
    preamble: Preamble<'a>,
}

impl<'a> Parser<'a> {
    fn new(source_code: &'a str) -> Self {
        Self {
            source_code,
            byte_offset: 0,
            preamble: Preamble::default(),
        }
    }

    #[inline(always)]
    fn err(&self, message: impl Into<String>) -> ParseError {
        self.err_at(self.byte_offset, message)
    }

    #[inline(always)]
    fn err_at(&self, byte_offset: usize, message: impl Into<String>) -> ParseError {
        crate::diagnostic::locate_diagnostic(self.source_code, byte_offset, message)
    }

    #[inline(always)]
    fn rest(&self) -> &'a str {
        &self.source_code[self.byte_offset..]
    }

    #[inline(always)]
    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    #[inline(always)]
    fn bump(&mut self) -> Option<char> {
        let character = self.rest().chars().next()?;
        self.byte_offset += character.len_utf8();
        Some(character)
    }

    #[inline(always)]
    fn starts_with(&self, prefix: &str) -> bool {
        self.rest().starts_with(prefix)
    }

    #[inline(always)]
    fn eat(&mut self, token: &str) -> bool {
        if self.starts_with(token) {
            self.byte_offset += token.len();
            true
        } else {
            false
        }
    }

    fn skip_ws(&mut self) {
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

    fn skip_hws(&mut self) {
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

    fn skip_comment(&mut self) {
        if let Some(pos) = self.rest().find('\n') {
            self.byte_offset += pos + 1;
        } else {
            self.byte_offset = self.source_code.len();
        }
    }

    fn at_blank_line(&self) -> bool {
        let r = self.rest();
        r.starts_with("\n\n") || r.starts_with("\r\n\r\n") || r.starts_with("\n\r\n")
    }

    fn cmd_ends_at(&self, cmd: &str) -> bool {
        if !self.starts_with(cmd) {
            return false;
        }
        match self.source_code[self.byte_offset + cmd.len()..].chars().next() {
            Some(c) if c.is_ascii_alphabetic() => false,
            _ => true,
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

    fn eat_end_env(&mut self, name: &str) -> bool {
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

    fn parse_optional_brackets(&mut self) -> Result<Vec<&'a str>, ParseError> {
        self.skip_hws();
        if !self.eat("[") {
            return Ok(Vec::new());
        }
        let start = self.byte_offset;
        while !matches!(self.peek(), Some(']') | None) {
            self.bump();
        }
        let raw = &self.source_code[start..self.byte_offset];
        if !self.eat("]") {
            return Err(self.err("unclosed optional argument ["));
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
        let registry = crate::preamble::PackageRegistry::default();
        let mut in_preamble = false;

        loop {
            self.skip_ws();
            if self.peek().is_none() {
                return Ok(false);
            }
            if self.at_begin_env("document") {
                self.eat_begin_env("document");
                return Ok(true);
            }
            if self.cmd_ends_at("\\documentclass") {
                in_preamble = true;
                self.eat("\\documentclass");
                let options = self.parse_optional_brackets()?;
                let name = self.brace_ident()?;
                let class = crate::preamble::DocClass { name, options };
                crate::preamble::apply_doc_class(&class, &mut self.preamble.config);
                self.preamble.class = Some(class);
                continue;
            }
            if self.cmd_ends_at("\\usepackage") {
                in_preamble = true;
                self.eat("\\usepackage");
                let options = self.parse_optional_brackets()?;
                let name = self.brace_ident()?;
                let pkg = crate::preamble::Package { name, options };
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
                if let Some(dim) = crate::preamble::parse_dimension_pt(val_str) {
                    if param == "\\parindent" || param == "parindent" {
                        self.preamble.config.par_indent = dim;
                    } else if param == "\\parskip" || param == "parskip" {
                        self.preamble.config.par_skip = dim;
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
        let mut blocks = Vec::new();
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
                    blocks.push(Block::TitleBlock {
                        title: title.clone(),
                        author: self.preamble.author.clone(),
                        date: self.preamble.date.clone(),
                    });
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
                if let Some(dim) = crate::preamble::parse_dimension_pt(val_str) {
                    if param == "\\parindent" || param == "parindent" {
                        self.preamble.config.par_indent = dim;
                    } else if param == "\\parskip" || param == "parskip" {
                        self.preamble.config.par_skip = dim;
                    }
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
                let dim = crate::preamble::parse_dimension_pt(val_str).unwrap_or(12.0);
                blocks.push(Block::VSpace(dim));
                continue;
            }
            if self.cmd_ends_at("\\section") {
                self.eat("\\section");
                let _ = self.eat("*");
                let title = self.require_group()?;
                blocks.push(Block::Section { level: 1, title });
                continue;
            }
            if self.cmd_ends_at("\\subsection") {
                self.eat("\\subsection");
                let _ = self.eat("*");
                let title = self.require_group()?;
                blocks.push(Block::Section { level: 2, title });
                continue;
            }
            if self.cmd_ends_at("\\subsubsection") {
                self.eat("\\subsubsection");
                let _ = self.eat("*");
                let title = self.require_group()?;
                blocks.push(Block::Section { level: 3, title });
                continue;
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
                {
                    break;
                }
            }
        }
        Ok(blocks)
    }

    fn parse_env(&mut self) -> Result<Block<'a>, ParseError> {
        self.eat("\\begin");
        let name = self.brace_ident()?;
        match name {
            "enumerate" => self.parse_list(true),
            "itemize" => self.parse_list(false),
            "tabular" => self.parse_tabular(),
            "figure" => self.parse_figure(),
            other => Err(self.err(format!("unsupported environment {{{other}}}"))),
        }
    }

    fn parse_list(&mut self, ordered: bool) -> Result<Block<'a>, ParseError> {
        let name = if ordered { "enumerate" } else { "itemize" };
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            if self.eat_end_env(name) {
                break;
            }
            if self.peek().is_none() {
                return Err(self.err(format!("unclosed \\begin{{{name}}}")));
            }
            if !self.at_item() {
                return Err(self.err(format!(
                    "expected \\item or \\end{{{name}}}, found something else"
                )));
            }
            self.eat("\\item");
            self.skip_hws();
            items.push(self.parse_blocks()?);
        }
        Ok(Block::List { ordered, items })
    }

    fn parse_tabular(&mut self) -> Result<Block<'a>, ParseError> {
        self.skip_hws();
        let cols = self.parse_colspec()?;
        let mut rows = Vec::new();
        let mut pending_rule = false;
        loop {
            self.skip_ws();
            if self.eat_end_env("tabular") {
                break;
            }
            if self.peek().is_none() {
                return Err(self.err("unclosed \\begin{tabular}"));
            }
            if self.cmd_ends_at("\\hline") {
                self.eat("\\hline");
                pending_rule = true;
                continue;
            }
            let mut cells = Vec::new();
            loop {
                cells.push(self.parse_cell_inlines()?);
                self.skip_hws();
                if self.eat("&") {
                    continue;
                }
                if self.starts_with("\\\\") {
                    self.eat("\\\\");
                }
                break;
            }
            rows.push(TableRow {
                rule_above: pending_rule,
                cells,
            });
            pending_rule = false;
        }
        Ok(Block::Table { cols, rows })
    }

    fn parse_colspec(&mut self) -> Result<Vec<ColSpec>, ParseError> {
        if !self.eat("{") {
            return Err(self.err("expected {lcr} column spec"));
        }
        let mut cols = Vec::new();
        let mut rule_left = false;
        loop {
            match self.peek() {
                Some('}') => {
                    self.bump();
                    break;
                }
                Some('|') => {
                    self.bump();
                    rule_left = true;
                }
                Some('l' | 'c' | 'r') => {
                    let align = match self.bump() {
                        Some('l') => ColAlign::Left,
                        Some('c') => ColAlign::Center,
                        _ => ColAlign::Right,
                    };
                    cols.push(ColSpec { align, rule_left });
                    rule_left = false;
                }
                Some(' ' | '\t') => {
                    self.bump();
                }
                Some(c) => {
                    return Err(self.err(format!(
                        "unsupported column '{c}' (use l, c, r, |)"
                    )));
                }
                None => return Err(self.err("unclosed column spec")),
            }
        }
        if cols.is_empty() {
            return Err(self.err("empty column spec"));
        }
        Ok(cols)
    }

    fn parse_cell_inlines(&mut self) -> Result<Vec<Inline<'a>>, ParseError> {
        let mut out = Vec::new();
        loop {
            self.skip_hws();
            match self.peek() {
                None => break,
                Some('&') => break,
                Some('%') => self.skip_comment(),
                Some('\n') if self.at_blank_line() => break,
                Some(_) if self.starts_with("\\\\") => break,
                Some(_) if self.starts_with("\\end") || self.cmd_ends_at("\\hline") => break,
                Some(_) if self.starts_with("\\begin") => {
                    return Err(self.err("nested environments are not allowed in a cell"));
                }
                Some(_) => out.push(self.parse_inline()?),
            }
        }
        Ok(out)
    }

    fn parse_figure(&mut self) -> Result<Block<'a>, ParseError> {
        self.skip_hws();
        if self.eat("[") {
            while !matches!(self.peek(), Some(']') | None) {
                self.bump();
            }
            self.eat("]");
        }
        let mut graphic = None;
        let mut caption = Vec::new();
        loop {
            self.skip_ws();
            if self.eat_end_env("figure") {
                break;
            }
            if self.peek().is_none() {
                return Err(self.err("unclosed \\begin{figure}"));
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
        Ok(Block::Figure { graphic, caption })
    }

    fn parse_graphic(&mut self) -> Result<Graphic<'a>, ParseError> {
        self.skip_hws();
        if self.eat("[") {
            while !matches!(self.peek(), Some(']') | None) {
                self.bump();
            }
            if !self.eat("]") {
                return Err(self.err("unclosed \\includegraphics optional arg"));
            }
        }
        self.skip_hws();
        if !self.eat("{") {
            return Err(self.err("expected {path} after \\includegraphics"));
        }
        let start = self.byte_offset;
        while !matches!(self.peek(), Some('}') | None) {
            self.bump();
        }
        let path = self.source_code[start..self.byte_offset].trim();
        if !self.eat("}") {
            return Err(self.err("unclosed \\includegraphics path"));
        }
        Ok(Graphic { path })
    }

    fn brace_ident(&mut self) -> Result<&'a str, ParseError> {
        self.skip_hws();
        if !self.eat("{") {
            return Err(self.err("expected {name}"));
        }
        let start = self.byte_offset;
        loop {
            match self.peek() {
                Some('}') => {
                    let ident = &self.source_code[start..self.byte_offset];
                    self.bump();
                    return Ok(ident);
                }
                Some(c) if c.is_ascii_alphabetic() => {
                    self.bump();
                }
                _ => return Err(self.err("invalid or unclosed name")),
            }
        }
    }

    fn brace_arg(&mut self) -> Result<&'a str, ParseError> {
        self.skip_hws();
        if !self.eat("{") {
            return Err(self.err("expected {arg}"));
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
                None => return Err(self.err("unclosed brace")),
            }
        }
    }

    fn parse_para_inlines(&mut self) -> Result<Vec<Inline<'a>>, ParseError> {
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None => break,
                Some('%') => self.skip_comment(),
                Some('\n') if self.at_blank_line() => break,
                Some(_) if self.starts_with("\\begin")
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
                    || self.cmd_ends_at("\\vspace") =>
                {
                    break;
                }
                Some(_) if self.starts_with("\\\\") => {
                    self.eat("\\\\");
                    out.push(Inline::Break);
                    self.skip_hws();
                }
                Some(_) => out.push(self.parse_inline()?),
            }
        }
        Ok(out)
    }

    fn parse_inline(&mut self) -> Result<Inline<'a>, ParseError> {
        match self.peek() {
            Some('\\') => self.parse_command(),
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
        let mut inner = Vec::new();
        loop {
            match self.peek() {
                Some('}') => {
                    self.bump();
                    break;
                }
                None => return Err(self.err("unclosed {")),
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
            return Err(self.err_at(cmd_start, "stray '\\'"));
        }
        self.skip_hws();
        match name {
            "textbf" => Ok(Inline::Bold(self.require_group()?)),
            "textit" | "emph" => Ok(Inline::Italic(self.require_group()?)),
            "par" => Ok(Inline::Break),
            "noindent" => Ok(Inline::NoIndent),
            other => Err(self.err_at(cmd_start, format!("unknown command \\{other}"))),
        }
    }

    fn require_group(&mut self) -> Result<Vec<Inline<'a>>, ParseError> {
        self.skip_hws();
        if !self.eat("{") {
            return Err(self.err("expected { after command"));
        }
        self.parse_until_rbrace()
    }

    /// Contiguous source slice. Newlines stay in the slice; layout treats
    /// them as space. A `%` or command ends the run so the slice stays
    /// untransformed.
    fn parse_text_run(&mut self) -> &'a str {
        let start = self.byte_offset;
        loop {
            match self.peek() {
                Some('\\' | '{' | '}' | '%' | '&') | None => break,
                Some('\n') if self.at_blank_line() => break,
                Some(_) => {
                    self.bump();
                }
            }
        }
        &self.source_code[start..self.byte_offset]
    }
}

fn is_single_escape(c: char) -> bool {
    matches!(c, '%' | '{' | '}' | '\\' | '#' | '&' | '_' | '$' | '~' | '^')
}

fn is_empty_inlines(v: &[Inline<'_>]) -> bool {
    v.iter().all(|i| match i {
        Inline::Text(s) => s.trim().is_empty(),
        Inline::Break | Inline::NoIndent => true,
        Inline::Bold(x) | Inline::Italic(x) | Inline::Group(x) => is_empty_inlines(x),
    })
}
