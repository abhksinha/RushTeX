//! Files and messages: tex.web §480–§486 (`\openin`, `\read`, `\closein`), §1342–§1378
//! (`\immediate`, `\openout`, `\write`, `\closeout`), §1279–§1283 (`\message`,
//! `\errmessage`) and e-TeX's `\readline`, in our own form.
//!
//! The engine owns no file system: input files come from [`Machine::read_file`], and what is
//! written goes to in-memory buffers (`\openout` streams and the log) for the caller to keep.

use super::engine_catcode::Cat;
use super::engine_eqtb::{Reg, Region};
use super::engine_lex::Lexer;
use super::engine_names::END_LINE_CHAR;
use super::engine_prim::{Macro, Meaning, Prim};
use super::engine_run::Machine;
use super::engine_token::{Span, Token};

/// An open `\read` stream (§480): the file's text and where the next line starts.
#[derive(Debug)]
pub struct ReadStream {
    text: String,
    pos: usize,
}

impl ReadStream {
    /// §486 `input_ln`: the next line without its end, or `None` at the end of the file.
    fn line(&mut self) -> Option<String> {
        if self.pos >= self.text.len() {
            return None;
        }
        let rest = &self.text[self.pos..];
        let (line, used) = match rest.find('\n') {
            Some(i) => (&rest[..i], i + 1),
            None => (rest, rest.len()),
        };
        self.pos += used;
        Some(line.strip_suffix('\r').unwrap_or(line).to_string())
    }
}

/// An `\openout` stream: its file name and what was written to it.
#[derive(Debug, Default, Clone)]
pub struct WriteStream {
    pub name: String,
    pub text: String,
}

/// The `\newlinechar` parameter slot (tex.web §236 `new_line_char_code`).
pub(super) const NEW_LINE_CHAR: u32 = 49;

impl Machine<'_, '_> {
    /// A stream number 0–15, or `None` (the terminal) for any other.
    fn stream(&mut self) -> Option<usize> {
        let n = self.scan_int();
        (0..16).contains(&n).then_some(n as usize)
    }

    /// §526 `scan_file_name` after `\openin` or `\openout`.
    pub(super) fn scan_file_name(&mut self) -> String {
        let mut name = String::new();
        if let Some(x) = self.next_non_blank() {
            if x.t.cs_id().is_none() && x.t.cat() == Some(Cat::BeginGroup) {
                let span = self.scan_general_text(true);
                let toks: Vec<Token> = self.e.toks.get(span).to_vec();
                for t in toks {
                    if t.cs_id().is_none() {
                        name.push(t.chr());
                    }
                }
                return name;
            }
            let mut x = x;
            loop {
                if x.t.cs_id().is_some() {
                    self.input.back(x.t);
                    break;
                }
                match x.t.chr() {
                    ' ' => break,
                    '"' => {}
                    c => name.push(c),
                }
                match self.get_x_token(false) {
                    Some(y) => x = y,
                    None => break,
                }
            }
        }
        name
    }

    /// The bytes of file `name`, trying `.tex` after it (§537). A file this run wrote and
    /// closed reads back (`./` is the current directory).
    pub(super) fn open_bytes(&self, name: &str) -> Option<Vec<u8>> {
        let local = name.strip_prefix("./").unwrap_or(name);
        let written = |w: &&WriteStream| {
            let n = w.name.strip_prefix("./").unwrap_or(&w.name);
            n == local || n.strip_suffix(".tex") == Some(local) || local.strip_suffix(".tex") == Some(n)
        };
        if let Some(w) = self.closed.iter().rev().find(written) {
            return Some(w.text.chars().map(|c| c as u32 as u8).collect());
        }
        let read = self.read_file.as_ref()?;
        read(name).or_else(|| read(&format!("{name}.tex")))
    }

    /// File `name` as text: one character per byte, as pdfTeX reads it.
    pub(super) fn open_file(&self, name: &str) -> Option<String> {
        self.open_bytes(name).map(|b| b.iter().map(|&b| char::from(b)).collect())
    }

    /// §1275 `\openin<n>[=]<file>` and `\closein<n>`.
    pub(super) fn open_in(&mut self, close: bool) {
        let n = self.stream();
        if let Some(n) = n {
            self.reads[n] = None;
        }
        if close {
            return;
        }
        self.scan_optional_equals();
        let name = self.scan_file_name();
        if let Some(n) = n {
            self.reads[n] = self.open_file(&name).map(|text| ReadStream { text, pos: 0 });
        }
    }

    /// §498 `\ifeof<n>`: whether stream `n` is closed.
    pub(super) fn if_eof(&mut self) -> bool {
        match self.stream() {
            Some(n) => self.reads[n].is_none(),
            None => true,
        }
    }

    /// §482 `read_toks` for `\read<n> to <cs>` (and e-TeX's `\readline`): lines until the
    /// braces balance, tokenized with the current catcodes (`\readline`: as characters of
    /// catcode 12, spaces 10), stored as a parameterless macro.
    pub(super) fn read_to(&mut self, line_only: bool, global: bool) {
        let n = self.stream();
        if !self.scan_keyword("to") {
            self.error("Missing `to' inserted");
        }
        let Some(id) = self.get_r_token() else { return };
        let mut list = vec![Token::END_MATCH];
        let elc = self.e.eqtb.word(Reg { region: Region::Int, slot: END_LINE_CHAR });
        let mut depth: i32 = 0;
        loop {
            let line = match n.and_then(|n| self.reads[n].as_mut()) {
                Some(stream) => match stream.line() {
                    Some(l) => l,
                    None => {
                        // §486: at the end of the file the stream closes; the empty line
                        // still counts.
                        self.reads[n.unwrap_or(0)] = None;
                        String::new()
                    }
                },
                None => {
                    // §484: fatal; nothing is read from a terminal.
                    self.error("*** (cannot \\read from terminal in nonstop modes)");
                    self.halted = true;
                    return;
                }
            };
            if line_only {
                let mut text = line.trim_end_matches(' ').to_string();
                if let Ok(c) = u8::try_from(elc) {
                    text.push(char::from(c));
                }
                super::engine_value::str_toks(&text, &mut list);
                break;
            }
            // One line, even an empty one, ends with `\endlinechar` (§483 `begin_file_reading`
            // of the line); the lexer counts a source without a line end as one only when
            // it has text.
            let mut line = line;
            line.push('\n');
            let mut lexer = Lexer::new(line);
            while let Some(tok) = lexer.next(self.e.eqtb.cats(), &mut self.e.cs, elc) {
                match (tok.cs_id(), tok.cat()) {
                    (None, Some(Cat::BeginGroup)) => depth += 1,
                    (None, Some(Cat::EndGroup)) => {
                        depth -= 1;
                        if depth < 0 {
                            // §483: an unmatched `}` ends the line.
                            depth = 0;
                            break;
                        }
                    }
                    _ => {}
                }
                list.push(tok);
            }
            if depth == 0 || n.is_none_or(|n| self.reads[n].is_none()) {
                break;
            }
        }
        let span = self.e.toks.store(&list);
        self.e.eqtb.set_meaning(id, Meaning::Macro(Macro { text: span, flags: 0 }), global);
    }

    /// The text of a token list as `\write` and `\message` print it: `\newlinechar` starts a
    /// new line (§59).
    pub(super) fn print_list(&self, span: Span) -> String {
        let mut s = String::new();
        let nl = self.e.eqtb.word(Reg { region: Region::Int, slot: NEW_LINE_CHAR });
        let ctx = super::engine_token::ShowCtx { file: Some(nl), ..self.show_ctx() };
        super::engine_token::show(self.e.toks.get(span), &self.e.cs, &ctx, &mut s);
        s
    }

    /// §1350 `\openout`, §1351 `\write`, §1353 `\closeout`, done at once (§1375
    /// `\immediate`). Without `\immediate`, `\write` is a whatsit for the native pipeline,
    /// which expands it when the page ships out; `\openout` and `\closeout` act at once
    /// either way, as nothing ships out while the engine runs class code.
    pub(super) fn extension(&mut self, p: Prim, immediate: bool) {
        match p {
            Prim::OpenOut | Prim::Write | Prim::CloseOut if !immediate && self.building() => self.whatsit_command(p),
            Prim::OpenOut => {
                let n = self.stream();
                self.scan_optional_equals();
                let name = self.scan_file_name();
                if let Some(n) = n {
                    // §1374: a stream still open is closed first.
                    if let Some(w) = self.writes[n].replace(WriteStream { name, text: String::new() }) {
                        self.closed.push(w);
                    }
                }
            }
            Prim::CloseOut => {
                if let Some(n) = self.stream() {
                    if let Some(w) = self.writes[n].take() {
                        self.closed.push(w);
                    }
                }
            }
            _ => {
                let n = self.scan_int();
                if !immediate {
                    // The whatsit: `\write`, its stream, and its unexpanded text.
                    let span = self.scan_braced_text(false);
                    let write = self.e.cs.intern("write");
                    self.out.push(Token::cs(write));
                    let mut num = Vec::new();
                    super::engine_value::str_toks(&format!("{n} "), &mut num);
                    self.out.extend_from_slice(&num);
                    self.out.push(Token::char(Cat::BeginGroup, '{'));
                    let toks = self.e.toks.get(span).to_vec();
                    self.out.extend_from_slice(&toks);
                    self.out.push(Token::char(Cat::EndGroup, '}'));
                    return;
                }
                let old = std::mem::replace(&mut self.mode_zero, true);
                let span = self.scan_braced_text(true);
                self.mode_zero = old;
                let mut text = self.print_list(span);
                text.push('\n');
                match usize::try_from(n).ok().filter(|&n| n < 16).and_then(|n| self.writes[n].as_mut()) {
                    Some(w) => w.text.push_str(&text),
                    None => self.log.push_str(&text),
                }
            }
        }
    }

    /// §1280 `\message` and `\errmessage`: an expanded text, to the log (or as an error).
    pub(super) fn message(&mut self, error: bool) {
        let span = self.scan_braced_text(true);
        let text = self.print_list(span);
        if error {
            self.error(format!("! {text}"));
        } else {
            self.log.push_str(&text);
            self.log.push(' ');
        }
    }
}
