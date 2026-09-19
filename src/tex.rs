//! Tiny TeX lexer + recursive-descent parser.
//!
//! Almost every `Text` node is a subslice of the input. The only owned
//! data on the success path is the tree scaffolding (`Vec`). Errors own
//! their message.

use std::fmt;
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
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub line: usize,
    pub col: usize,
    pub msg: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.msg)
    }
}

pub fn parse(src: &str) -> Result<Document<'_>, ParseError> {
    let mut p = Parser::new(src);
    let has_doc_env = p.parse_preamble_if_present()?;
    let blocks = p.parse_blocks()?;
    if has_doc_env {
        let _ = p.eat_end_env("document");
    } else {
        p.skip_ws();
        if p.peek().is_some() && !p.starts_with("\\end") {
            return Err(p.err("trailing unparsed input"));
        }
    }
    Ok(Document {
        preamble: p.preamble,
        blocks,
    })
}

struct Parser<'a> {
    src: &'a str,
    i: usize,
    line: usize,
    col: usize,
    preamble: Preamble<'a>,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            i: 0,
            line: 1,
            col: 1,
            preamble: Preamble::default(),
        }
    }

    fn err(&self, msg: impl Into<String>) -> ParseError {
        ParseError {
            line: self.line,
            col: self.col,
            msg: msg.into(),
        }
    }

    fn rest(&self) -> &'a str {
        &self.src[self.i..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let mut cs = self.rest().chars();
        let c = cs.next()?;
        self.i += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn starts_with(&self, s: &str) -> bool {
        self.rest().starts_with(s)
    }

    fn eat(&mut self, s: &str) -> bool {
        if self.starts_with(s) {
            for _ in s.chars() {
                self.bump();
            }
            true
        } else {
            false
        }
    }

    fn skip_ws(&mut self) {
        loop {
            match self.peek() {
                Some(' ' | '\t' | '\r' | '\n') => {
                    self.bump();
                }
                Some('%') => self.skip_comment(),
                _ => break,
            }
        }
    }

    fn skip_hws(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\r')) {
            self.bump();
        }
    }

    fn skip_comment(&mut self) {
        while !matches!(self.peek(), Some('\n') | None) {
            self.bump();
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
        match self.src[self.i + cmd.len()..].chars().next() {
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
        let after = &self.src[self.i + PFX.len()..];
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
        let after = &self.src[self.i + PFX.len()..];
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
        let start = self.i;
        while !matches!(self.peek(), Some(']') | None) {
            self.bump();
        }
        let raw = &self.src[start..self.i];
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
        let start = self.i;
        while !matches!(self.peek(), Some('}') | None) {
            self.bump();
        }
        let path = self.src[start..self.i].trim();
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
        let start = self.i;
        loop {
            match self.peek() {
                Some('}') => {
                    let ident = &self.src[start..self.i];
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
                    || self.cmd_ends_at("\\maketitle") =>
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
                let start = self.i;
                self.bump();
                Ok(Inline::Text(&self.src[start..self.i]))
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
        self.bump(); // '\'
        if let Some(c) = self.peek() {
            if is_single_escape(c) {
                let start = self.i;
                self.bump();
                return Ok(Inline::Text(&self.src[start..self.i]));
            }
        }
        let start = self.i;
        while matches!(self.peek(), Some(ch) if ch.is_ascii_alphabetic()) {
            self.bump();
        }
        let name = &self.src[start..self.i];
        if name.is_empty() {
            return Err(self.err("stray '\\'"));
        }
        self.skip_hws();
        match name {
            "textbf" => Ok(Inline::Bold(self.require_group()?)),
            "textit" | "emph" => Ok(Inline::Italic(self.require_group()?)),
            "par" => Ok(Inline::Break),
            other => Err(self.err(format!("unknown command \\{other}"))),
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
        let start = self.i;
        loop {
            match self.peek() {
                Some('\\' | '{' | '}' | '%' | '&') | None => break,
                Some('\n') if self.at_blank_line() => break,
                Some(_) => {
                    self.bump();
                }
            }
        }
        &self.src[start..self.i]
    }
}

fn is_single_escape(c: char) -> bool {
    matches!(c, '%' | '{' | '}' | '\\' | '#' | '&' | '_' | '$' | '~' | '^')
}

fn is_empty_inlines(v: &[Inline<'_>]) -> bool {
    v.iter().all(|i| match i {
        Inline::Text(s) => s.trim().is_empty(),
        Inline::Break => true,
        Inline::Bold(x) | Inline::Italic(x) | Inline::Group(x) => is_empty_inlines(x),
    })
}
