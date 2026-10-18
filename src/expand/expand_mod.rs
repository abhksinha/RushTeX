//! # Macro Expansion Stage (`expand`)
//!
//! Runs once between the source text and the parser. It resolves everything that
//! *generates* text the borrowed AST could not otherwise point at: user macros and
//! environments, conditionals, counters, cross-references, citations, theorem heads,
//! and `\today`.
//!
//! ## Architectural Principles
//!
//! - **Free common case**: [`expand`] first runs a single byte scan for trigger control
//!   words. Documents without any are returned as `Cow::Borrowed` — no allocation, no copy.
//! - **Contiguous input stack**: pending text lives in a `Vec<Frame>` of byte ranges over
//!   three bases (original source, an append-only definition arena, or a pooled owned
//!   buffer). Parameterless macro bodies are pushed as arena ranges without copying.
//! - **Bulk copying**: plain prose between `\` and `%` bytes is copied with one
//!   `push_str` per run.
//! - **One pass + patch**: `\ref`/`\eqref`/`\cite` sites are recorded as output offsets and
//!   patched in a single splice once all labels and bibliography keys are known.
//! - **Bounded**: expansion count, input stack depth, and output size are capped, so
//!   runaway recursion (`\def\a{\a}`) becomes a diagnostic instead of unbounded growth.
//!
//! ## Known approximations
//!
//! - Definitions are global (TeX grouping is not modelled); `\edef`/`\xdef` behave like `\def`.
//! - Category codes are fixed (only `\makeatletter` is honoured for macro names).
//! - `\setcounter` on `section`/`figure`/`table`/`equation` changes reference numbers only;
//!   the layout engine keeps its own numbering.
//! - `\pageref` resolves to `??` until layout-position feedback exists (roadmap phase 4).

use std::borrow::Cow;
use std::collections::HashMap;

use crate::diagnostic::{locate_diagnostic, Diagnostic};

#[path = "expand_native.rs"]
mod native;

const MAX_EXPANSIONS: u32 = 1_000_000;
const MAX_STACK_DEPTH: usize = 1024;
const MIN_OUTPUT_LIMIT: usize = 64 << 20;

/// Expands user macros, conditionals, counters, and references in `source`.
///
/// Returns the input unchanged (borrowed) when it contains no constructs that need expansion.
pub fn expand(source: &str) -> Result<Cow<'_, str>, Diagnostic> {
    if !needs_expansion(source) {
        return Ok(Cow::Borrowed(source));
    }
    Expander::new(source).run().map(Cow::Owned)
}

/// Single linear scan for control words that require the expansion stage.
pub fn needs_expansion(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut i = 0;
    while let Some(off) = bytes[i..].iter().position(|&b| b == b'\\') {
        let start = i + off + 1;
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_alphabetic() {
            end += 1;
        }
        if end > start {
            let name = &source[start..end];
            if is_trigger(name) {
                return true;
            }
            if name == "begin" {
                let after = &bytes[end..];
                if after.starts_with(b"{proof}") || after.starts_with(b"{comment}") {
                    return true;
                }
            }
            i = end;
        } else {
            i = (start + 1).min(bytes.len());
        }
    }
    false
}

#[inline]
fn is_trigger(name: &str) -> bool {
    matches!(
        name,
        "newcommand"
            | "renewcommand"
            | "providecommand"
            | "def"
            | "gdef"
            | "edef"
            | "xdef"
            | "let"
            | "newenvironment"
            | "renewenvironment"
            | "newif"
            | "iftrue"
            | "iffalse"
            | "newcounter"
            | "setcounter"
            | "addtocounter"
            | "stepcounter"
            | "refstepcounter"
            | "newtheorem"
            | "theoremstyle"
            | "label"
            | "ref"
            | "eqref"
            | "pageref"
            | "cite"
            | "bibitem"
            | "today"
            | "DeclareMathOperator"
            | "makeatletter"
    )
}

// ---------------------------------------------------------------------------
// Input stack
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Base {
    Source,
    Arena,
    Owned(u32),
}

#[derive(Clone, Copy)]
struct Frame {
    base: Base,
    pos: u32,
    end: u32,
}

struct Input<'s> {
    source: &'s str,
    /// Append-only storage for macro bodies and environment code.
    arena: String,
    /// Owned buffers for substituted bodies; indexed by `Base::Owned`.
    owned: Vec<String>,
    /// Indices into `owned` that are free for reuse.
    free_owned: Vec<u32>,
    frames: Vec<Frame>,
}

impl<'s> Input<'s> {
    #[inline]
    fn base_str(&self, base: Base) -> &str {
        match base {
            Base::Source => self.source,
            Base::Arena => &self.arena,
            Base::Owned(i) => &self.owned[i as usize],
        }
    }

    /// Pops exhausted frames and returns the next byte without consuming it.
    #[inline]
    fn peek(&mut self) -> Option<u8> {
        loop {
            let top = *self.frames.last()?;
            if top.pos < top.end {
                return Some(self.base_str(top.base).as_bytes()[top.pos as usize]);
            }
            self.pop_frame();
        }
    }

    fn pop_frame(&mut self) {
        if let Some(frame) = self.frames.pop() {
            if let Base::Owned(i) = frame.base {
                self.owned[i as usize].clear();
                self.free_owned.push(i);
            }
        }
    }

    /// Remaining text of the top frame. Call only after `peek` returned `Some`.
    #[inline]
    fn top_rest(&self) -> &str {
        let top = self.frames.last().unwrap();
        &self.base_str(top.base)[top.pos as usize..top.end as usize]
    }

    #[inline]
    fn bump(&mut self, n: usize) {
        if let Some(top) = self.frames.last_mut() {
            top.pos = (top.pos + n as u32).min(top.end);
        }
    }

    #[inline]
    fn peek_char(&mut self) -> Option<char> {
        self.peek()?;
        self.top_rest().chars().next()
    }

    fn take_owned(&mut self) -> (u32, String) {
        match self.free_owned.pop() {
            Some(i) => (i, std::mem::take(&mut self.owned[i as usize])),
            None => {
                self.owned.push(String::new());
                ((self.owned.len() - 1) as u32, String::new())
            }
        }
    }

    /// Discards exhausted frames so tail-position expansions do not deepen the stack.
    fn trim_exhausted(&mut self) {
        while let Some(top) = self.frames.last() {
            if top.pos < top.end || self.frames.len() == 1 {
                break;
            }
            self.pop_frame();
        }
    }

    fn push_owned(&mut self, idx: u32, text: String) {
        self.trim_exhausted();
        let end = text.len() as u32;
        self.owned[idx as usize] = text;
        self.frames.push(Frame {
            base: Base::Owned(idx),
            pos: 0,
            end,
        });
    }

    fn push_arena(&mut self, start: u32, end: u32) {
        self.trim_exhausted();
        self.frames.push(Frame {
            base: Base::Arena,
            pos: start,
            end,
        });
    }

    fn skip_spaces(&mut self) {
        while let Some(b) = self.peek() {
            if matches!(b, b' ' | b'\t' | b'\r' | b'\n') {
                self.bump(1);
            } else {
                break;
            }
        }
    }

    /// Reads a balanced `{...}` group (outer braces stripped) or a single token.
    fn read_arg(&mut self, out: &mut String) {
        self.skip_spaces();
        match self.peek() {
            Some(b'{') => {
                self.bump(1);
                self.read_balanced_until(b'}', out);
            }
            Some(b'\\') => self.read_token(out),
            Some(_) => {
                if let Some(c) = self.peek_char() {
                    out.push(c);
                    self.bump(c.len_utf8());
                }
            }
            None => {}
        }
    }

    /// Reads a control sequence (`\name` or `\x`) into `out`.
    fn read_token(&mut self, out: &mut String) {
        if self.peek() != Some(b'\\') {
            if let Some(c) = self.peek_char() {
                out.push(c);
                self.bump(c.len_utf8());
            }
            return;
        }
        self.bump(1);
        out.push('\\');
        let rest = self.top_rest();
        let n = rest.bytes().take_while(|b| b.is_ascii_alphabetic() || *b == b'@').count();
        if n > 0 {
            out.push_str(&rest[..n]);
            self.bump(n);
        } else if let Some(c) = self.peek_char() {
            out.push(c);
            self.bump(c.len_utf8());
        }
    }

    /// Reads until the unbalanced `close` byte (consumed, not copied).
    fn read_balanced_until(&mut self, close: u8, out: &mut String) {
        let mut depth = 0u32;
        while self.peek().is_some() {
            let rest = self.top_rest();
            let bytes = rest.as_bytes();
            let mut i = 0;
            let mut done = false;
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' if i + 1 < bytes.len() => {
                        i += 2;
                        continue;
                    }
                    b'{' => depth += 1,
                    b'}' if depth > 0 => depth -= 1,
                    b if b == close && depth == 0 => {
                        done = true;
                        break;
                    }
                    _ => {}
                }
                i += 1;
            }
            let i = i.min(bytes.len());
            // Never split a multi-byte character: `i` only stops on ASCII bytes or the end.
            out.push_str(&rest[..i]);
            if done {
                self.bump(i + 1);
                return;
            }
            self.bump(i);
        }
    }

    /// Reads an optional `[...]` argument after skipping spaces.
    fn read_optional(&mut self) -> Option<String> {
        self.skip_spaces();
        if self.peek() != Some(b'[') {
            return None;
        }
        self.bump(1);
        let mut s = String::new();
        self.read_balanced_until(b']', &mut s);
        Some(s)
    }

    /// Reads a control-sequence name given either as `\name` or `{\name}`.
    fn read_cs_target(&mut self) -> String {
        let mut raw = String::new();
        self.read_arg(&mut raw);
        let t = raw.trim();
        t.strip_prefix('\\').unwrap_or(t).to_string()
    }
}

// ---------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------

enum Piece {
    Lit(Box<str>),
    Param(u8),
}

struct Macro {
    nargs: u8,
    default: Option<(u32, u32)>,
    body: (u32, u32),
    /// `\def` parameter text with delimiters; `None` for plain `#1#2…` signatures.
    pattern: Option<Box<[Piece]>>,
    has_params: bool,
}

struct EnvDef {
    nargs: u8,
    default: Option<(u32, u32)>,
    begin: (u32, u32),
    end: (u32, u32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TheoremStyle {
    Plain,
    Definition,
    Remark,
}

struct TheoremDef {
    title: String,
    counter: Option<u32>,
    style: TheoremStyle,
}

struct Counter {
    value: i32,
    parent: Option<u32>,
}

enum EnvKind {
    User(u32),
    Theorem,
    Proof,
    Bibliography,
    Float { is_table: bool },
}

struct EnvEntry {
    name: String,
    kind: EnvKind,
}

enum Pending {
    Ref { at: usize, key: String, paren: bool },
    Cite { at: usize, keys: String, note: Option<String> },
}

// ---------------------------------------------------------------------------
// Expander
// ---------------------------------------------------------------------------

struct Expander<'s> {
    input: Input<'s>,
    out: String,
    out_limit: usize,
    /// Output currently ends with an alphabetic control word; a following letter needs a space.
    out_ends_cs: bool,
    expansions: u32,

    macros: HashMap<String, Macro>,
    envs: HashMap<String, EnvDef>,
    env_ids: Vec<String>,
    bools: HashMap<String, bool>,
    counters: Vec<Counter>,
    counter_ids: HashMap<String, u32>,
    theorems: HashMap<String, TheoremDef>,
    theorem_style: TheoremStyle,
    at_letter: bool,
    cond_depth: u32,
    env_stack: Vec<EnvEntry>,

    current_label: String,
    labels: HashMap<String, String>,
    bib: HashMap<String, String>,
    bib_count: u32,
    pending: Vec<Pending>,
}

const C_SECTION: u32 = 0;
const C_SUBSECTION: u32 = 1;
const C_SUBSUBSECTION: u32 = 2;
const C_FIGURE: u32 = 3;
const C_TABLE: u32 = 4;
const C_EQUATION: u32 = 5;

impl<'s> Expander<'s> {
    fn new(source: &'s str) -> Self {
        let mut counters = Vec::with_capacity(16);
        let mut counter_ids = HashMap::with_capacity(16);
        for (name, parent) in [
            ("section", None),
            ("subsection", Some(C_SECTION)),
            ("subsubsection", Some(C_SUBSECTION)),
            ("figure", None),
            ("table", None),
            ("equation", None),
        ] {
            counter_ids.insert(name.to_string(), counters.len() as u32);
            counters.push(Counter { value: 0, parent });
        }
        Self {
            input: Input {
                source,
                arena: String::new(),
                owned: Vec::new(),
                free_owned: Vec::new(),
                frames: vec![Frame {
                    base: Base::Source,
                    pos: 0,
                    end: source.len() as u32,
                }],
            },
            out: String::with_capacity(source.len() + source.len() / 8),
            out_limit: MIN_OUTPUT_LIMIT.max(source.len().saturating_mul(64)),
            out_ends_cs: false,
            expansions: 0,
            macros: HashMap::new(),
            envs: HashMap::new(),
            env_ids: Vec::new(),
            bools: HashMap::new(),
            counters,
            counter_ids,
            theorems: HashMap::new(),
            theorem_style: TheoremStyle::Plain,
            at_letter: false,
            cond_depth: 0,
            env_stack: Vec::new(),
            current_label: String::new(),
            labels: HashMap::new(),
            bib: HashMap::new(),
            bib_count: 0,
            pending: Vec::new(),
        }
    }

    fn error(&self, message: impl Into<String>) -> Diagnostic {
        let pos = self
            .input
            .frames
            .first()
            .map_or(self.input.source.len(), |f| f.pos as usize);
        locate_diagnostic(self.input.source, pos, message)
    }

    fn run(mut self) -> Result<String, Diagnostic> {
        while let Some(b) = self.input.peek() {
            match b {
                b'\\' => self.control()?,
                b'%' => self.copy_comment(),
                _ => self.copy_run(),
            }
            if self.out.len() > self.out_limit {
                return Err(self.error("macro expansion produced too much output (possible runaway recursion)"));
            }
        }
        Ok(self.finish())
    }

    // ----- output helpers ---------------------------------------------------

    #[inline]
    fn emit(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.out_ends_cs && text.as_bytes()[0].is_ascii_alphabetic() {
            self.out.push(' ');
        }
        self.out.push_str(text);
        self.out_ends_cs = false;
    }

    fn emit_cs(&mut self, name: &str) {
        self.out.push('\\');
        self.out.push_str(name);
        self.out_ends_cs = name.as_bytes().first().is_some_and(|b| b.is_ascii_alphabetic());
    }

    fn copy_run(&mut self) {
        let rest = self.input.top_rest();
        let n = rest
            .as_bytes()
            .iter()
            .position(|&b| b == b'\\' || b == b'%')
            .unwrap_or(rest.len());
        let first_alpha = rest.as_bytes()[0].is_ascii_alphabetic();
        if self.out_ends_cs && first_alpha {
            self.out.push(' ');
        }
        self.out.push_str(&rest[..n]);
        self.out_ends_cs = false;
        self.input.bump(n);
    }

    fn copy_comment(&mut self) {
        let rest = self.input.top_rest();
        let n = rest.find('\n').map_or(rest.len(), |p| p + 1);
        self.out.push_str(&rest[..n]);
        self.out_ends_cs = false;
        self.input.bump(n);
    }

    /// Replaces a consumed definition so the parser sees neither the definition nor a
    /// spurious blank line; internal newlines are kept as comment lines for line numbering.
    fn finish_definition(&mut self, newlines: usize) {
        for _ in 0..newlines {
            self.out.push_str("%\n");
        }
        if self.input.peek() == Some(b'\n') {
            self.out.push('%');
        }
        self.out_ends_cs = false;
    }

    // ----- control sequences --------------------------------------------------

    fn control(&mut self) -> Result<(), Diagnostic> {
        self.input.bump(1);
        let Some(first) = self.input.peek() else {
            self.out.push('\\');
            return Ok(());
        };
        let at_letter = self.at_letter;
        let is_letter = |b: u8| b.is_ascii_alphabetic() || (at_letter && b == b'@');
        if !is_letter(first) {
            let c = self.input.peek_char().unwrap_or('\\');
            self.out.push('\\');
            self.out.push(c);
            self.out_ends_cs = false;
            self.input.bump(c.len_utf8());
            return Ok(());
        }
        let rest = self.input.top_rest();
        let n = rest.bytes().take_while(|&b| is_letter(b)).count();
        let name = rest[..n].to_string();
        self.input.bump(n);
        self.dispatch(name)
    }

    fn dispatch(&mut self, name: String) -> Result<(), Diagnostic> {
        if self.macros.contains_key(&name) {
            return self.expand_macro(&name);
        }
        match name.as_str() {
            "newcommand" => self.define_command(DefineKind::New),
            "renewcommand" => self.define_command(DefineKind::Renew),
            "providecommand" => self.define_command(DefineKind::Provide),
            "def" | "gdef" | "edef" | "xdef" => self.define_def(),
            "let" => self.define_let(),
            "newenvironment" | "renewenvironment" => self.define_environment(),
            "DeclareMathOperator" => self.define_math_operator(),
            "newif" => {
                let target = self.input.read_cs_target();
                if let Some(flag) = target.strip_prefix("if") {
                    self.bools.insert(flag.to_string(), false);
                }
                self.finish_definition(0);
                Ok(())
            }
            "makeatletter" => {
                self.at_letter = true;
                Ok(())
            }
            "makeatother" => {
                self.at_letter = false;
                Ok(())
            }
            "else" if self.cond_depth > 0 => {
                self.skip_branch(false)?;
                self.cond_depth -= 1;
                self.skip_hspace();
                Ok(())
            }
            "fi" if self.cond_depth > 0 => {
                self.cond_depth -= 1;
                self.skip_hspace();
                Ok(())
            }
            "iftrue" => self.conditional(true),
            "iffalse" => self.conditional(false),
            "ifx" => {
                let mut a = String::new();
                let mut b = String::new();
                self.input.skip_spaces();
                self.input.read_token(&mut a);
                self.input.skip_spaces();
                self.input.read_token(&mut b);
                let equal = self.same_meaning(&a, &b);
                self.conditional(equal)
            }
            "ifdefined" => {
                let mut a = String::new();
                self.input.skip_spaces();
                self.input.read_token(&mut a);
                let n = a.trim_start_matches('\\');
                let defined = self.macros.contains_key(n) || self.counter_ids.contains_key(n);
                self.conditional(defined)
            }
            "ifnum" => {
                let lhs = self.read_int_operand();
                self.input.skip_spaces();
                let rel = self.input.peek();
                self.input.bump(1);
                let rhs = self.read_int_operand();
                let truth = match rel {
                    Some(b'<') => lhs < rhs,
                    Some(b'>') => lhs > rhs,
                    _ => lhs == rhs,
                };
                self.conditional(truth)
            }
            "newcounter" => {
                let mut c = String::new();
                self.input.read_arg(&mut c);
                let parent = self
                    .input
                    .read_optional()
                    .and_then(|p| self.counter_ids.get(p.trim()).copied());
                if !self.counter_ids.contains_key(c.trim()) {
                    self.counter_ids.insert(c.trim().to_string(), self.counters.len() as u32);
                    self.counters.push(Counter { value: 0, parent });
                }
                self.finish_definition(0);
                Ok(())
            }
            "setcounter" | "addtocounter" => {
                let mut c = String::new();
                let mut v = String::new();
                self.input.read_arg(&mut c);
                self.input.read_arg(&mut v);
                let value = self.eval_int(&v);
                if let Some(&id) = self.counter_ids.get(c.trim()) {
                    let counter = &mut self.counters[id as usize];
                    if name == "setcounter" {
                        counter.value = value;
                    } else {
                        counter.value += value;
                    }
                }
                self.finish_definition(0);
                Ok(())
            }
            "stepcounter" | "refstepcounter" => {
                let mut c = String::new();
                self.input.read_arg(&mut c);
                if let Some(&id) = self.counter_ids.get(c.trim()) {
                    self.step_counter(id);
                    if name == "refstepcounter" {
                        self.current_label = self.format_the(id);
                    }
                }
                Ok(())
            }
            "arabic" | "roman" | "Roman" | "alph" | "Alph" | "value" => {
                let mut c = String::new();
                self.input.read_arg(&mut c);
                let value = self
                    .counter_ids
                    .get(c.trim())
                    .map_or(0, |&id| self.counters[id as usize].value);
                let text = format_counter(value, &name);
                self.emit(&text);
                Ok(())
            }
            "newtheorem" => self.define_theorem(),
            "theoremstyle" => {
                let mut s = String::new();
                self.input.read_arg(&mut s);
                self.theorem_style = match s.trim() {
                    "definition" => TheoremStyle::Definition,
                    "remark" => TheoremStyle::Remark,
                    _ => TheoremStyle::Plain,
                };
                self.finish_definition(0);
                Ok(())
            }
            "label" => {
                let mut key = String::new();
                self.input.read_arg(&mut key);
                self.labels.insert(key.trim().to_string(), self.current_label.clone());
                Ok(())
            }
            "ref" | "eqref" => {
                let mut key = String::new();
                self.input.read_arg(&mut key);
                self.out_ends_cs_guard();
                self.pending.push(Pending::Ref {
                    at: self.out.len(),
                    key: key.trim().to_string(),
                    paren: name == "eqref",
                });
                Ok(())
            }
            "pageref" => {
                let mut key = String::new();
                self.input.read_arg(&mut key);
                self.emit("??");
                Ok(())
            }
            "cite" => {
                let note = self.input.read_optional();
                let mut keys = String::new();
                self.input.read_arg(&mut keys);
                self.out_ends_cs_guard();
                self.pending.push(Pending::Cite {
                    at: self.out.len(),
                    keys,
                    note,
                });
                Ok(())
            }
            "bibitem" => {
                let custom = self.input.read_optional();
                let mut key = String::new();
                self.input.read_arg(&mut key);
                self.bib_count += 1;
                let label = custom.unwrap_or_else(|| self.bib_count.to_string());
                let item = format!("\\item[{{[{label}]}}] ");
                self.bib.insert(key.trim().to_string(), label);
                self.emit(&item);
                Ok(())
            }
            "today" => {
                let text = today_string();
                self.emit(&text);
                Ok(())
            }
            "begin" => self.begin_environment(),
            "end" => self.end_environment(),
            "section" | "subsection" | "subsubsection" => {
                let starred = self.input.peek() == Some(b'*');
                if !starred {
                    let id = match name.as_str() {
                        "section" => C_SECTION,
                        "subsection" => C_SUBSECTION,
                        _ => C_SUBSUBSECTION,
                    };
                    self.step_counter(id);
                    self.current_label = self.format_the(id);
                }
                self.emit_cs(&name);
                Ok(())
            }
            "caption" => {
                let in_table = self
                    .env_stack
                    .iter()
                    .rev()
                    .find_map(|e| match e.kind {
                        EnvKind::Float { is_table } => Some(is_table),
                        _ => None,
                    })
                    .unwrap_or(false);
                if in_table {
                    self.step_counter(C_TABLE);
                    self.current_label = self.format_the(C_TABLE);
                }
                self.emit_cs(&name);
                Ok(())
            }
            "verb" | "lstinline" => {
                self.emit_cs(&name);
                self.out_ends_cs = false;
                self.copy_verb();
                Ok(())
            }
            other => {
                if let Some(flag) = other.strip_suffix("true").filter(|f| self.bools.contains_key(*f)) {
                    let flag = flag.to_string();
                    self.bools.insert(flag, true);
                    self.skip_hspace();
                    return Ok(());
                }
                if let Some(flag) = other.strip_suffix("false").filter(|f| self.bools.contains_key(*f)) {
                    let flag = flag.to_string();
                    self.bools.insert(flag, false);
                    self.skip_hspace();
                    return Ok(());
                }
                if let Some(flag) = other.strip_prefix("if") {
                    if let Some(&value) = self.bools.get(flag) {
                        return self.conditional(value);
                    }
                }
                if let Some(counter) = other.strip_prefix("the") {
                    if let Some(&id) = self.counter_ids.get(counter) {
                        let text = self.format_the(id);
                        self.emit(&text);
                        return Ok(());
                    }
                }
                self.emit_cs(other);
                Ok(())
            }
        }
    }

    /// A placeholder is about to be inserted at the current output position; make sure a
    /// preceding control word cannot absorb the patched-in text.
    fn out_ends_cs_guard(&mut self) {
        if self.out_ends_cs {
            self.out.push(' ');
            self.out_ends_cs = false;
        }
    }

    fn copy_verb(&mut self) {
        if self.input.peek() == Some(b'*') {
            self.out.push('*');
            self.input.bump(1);
        }
        let Some(delim) = self.input.peek_char() else {
            return;
        };
        self.input.bump(delim.len_utf8());
        self.out.push(delim);
        let rest = self.input.top_rest();
        let n = rest.find(delim).map_or(rest.len(), |p| p + delim.len_utf8());
        self.out.push_str(&rest[..n]);
        self.input.bump(n);
    }

    // ----- macro invocation ---------------------------------------------------

    fn count_expansion(&mut self, name: &str) -> Result<(), Diagnostic> {
        self.expansions += 1;
        if self.expansions > MAX_EXPANSIONS {
            return Err(self.error(format!(
                "macro expansion limit exceeded while expanding \\{name} (possible infinite recursion)"
            )));
        }
        if self.input.frames.len() >= MAX_STACK_DEPTH {
            return Err(self.error(format!(
                "macro nesting too deep while expanding \\{name} (possible infinite recursion)"
            )));
        }
        Ok(())
    }

    fn expand_macro(&mut self, name: &str) -> Result<(), Diagnostic> {
        self.count_expansion(name)?;
        let m = &self.macros[name];
        let (nargs, default, body, has_params) = (m.nargs, m.default, m.body, m.has_params);
        let is_pattern = m.pattern.is_some();
        let mut args: Vec<String> = Vec::with_capacity(nargs as usize);

        if is_pattern {
            self.read_pattern_args(name, &mut args);
        } else {
            if nargs == 0 {
                // TeX skips spaces after a control word.
                while matches!(self.input.peek(), Some(b' ' | b'\t')) {
                    self.input.bump(1);
                }
            }
            let mut first = 0;
            if let Some((ds, de)) = default {
                let opt = self.input.read_optional();
                args.push(opt.unwrap_or_else(|| self.input.arena[ds as usize..de as usize].to_string()));
                first = 1;
            }
            for _ in first..nargs {
                let mut a = String::new();
                self.input.read_arg(&mut a);
                args.push(a);
            }
        }

        if !has_params {
            self.input.push_arena(body.0, body.1);
            return Ok(());
        }
        let (idx, mut buf) = self.input.take_owned();
        substitute(&self.input.arena[body.0 as usize..body.1 as usize], &args, &mut buf);
        self.input.push_owned(idx, buf);
        Ok(())
    }

    fn read_pattern_args(&mut self, name: &str, args: &mut Vec<String>) {
        let pieces: Vec<(bool, String)> = self.macros[name]
            .pattern
            .as_ref()
            .unwrap()
            .iter()
            .map(|p| match p {
                Piece::Lit(s) => (true, s.to_string()),
                Piece::Param(n) => (false, n.to_string()),
            })
            .collect();
        let mut i = 0;
        while i < pieces.len() {
            let (is_lit, ref text) = pieces[i];
            if is_lit {
                // Leading/intermediate literal: consume it if present.
                self.input.skip_spaces();
                if self.input.peek().is_some() && self.input.top_rest().starts_with(text.as_str()) {
                    self.input.bump(text.len());
                }
                i += 1;
                continue;
            }
            match pieces.get(i + 1) {
                Some((true, delim)) => {
                    let mut a = String::new();
                    self.read_until_delim(delim, &mut a);
                    strip_outer_braces(&mut a);
                    args.push(a);
                    i += 2;
                }
                _ => {
                    let mut a = String::new();
                    self.input.read_arg(&mut a);
                    args.push(a);
                    i += 1;
                }
            }
        }
    }

    fn read_until_delim(&mut self, delim: &str, out: &mut String) {
        let mut depth = 0u32;
        while self.input.peek().is_some() {
            let rest = self.input.top_rest();
            if depth == 0 && rest.starts_with(delim) {
                self.input.bump(delim.len());
                return;
            }
            let c = rest.chars().next().unwrap();
            match c {
                '{' => depth += 1,
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
            out.push(c);
            self.input.bump(c.len_utf8());
        }
    }

    fn same_meaning(&self, a: &str, b: &str) -> bool {
        let (na, nb) = (a.trim_start_matches('\\'), b.trim_start_matches('\\'));
        match (self.macros.get(na), self.macros.get(nb)) {
            (Some(x), Some(y)) => {
                let arena = &self.input.arena;
                x.nargs == y.nargs
                    && arena[x.body.0 as usize..x.body.1 as usize] == arena[y.body.0 as usize..y.body.1 as usize]
            }
            (None, None) => a == b,
            _ => false,
        }
    }

    // ----- definitions --------------------------------------------------------

    fn store(&mut self, text: &str) -> (u32, u32) {
        let start = self.input.arena.len() as u32;
        self.input.arena.push_str(text);
        // A body ending in a control word must not fuse with letters that follow the call site.
        let ends_cs = {
            let bytes = text.as_bytes();
            let letters = bytes.iter().rev().take_while(|b| b.is_ascii_alphabetic()).count();
            letters > 0 && letters < bytes.len() && bytes[bytes.len() - letters - 1] == b'\\'
        };
        if ends_cs {
            self.input.arena.push(' ');
        }
        (start, self.input.arena.len() as u32)
    }

    fn insert_macro(&mut self, name: String, nargs: u8, default: Option<String>, body: &str, pattern: Option<Box<[Piece]>>) {
        let default = default.map(|d| self.store(&d));
        let has_params = body.contains('#');
        let body = self.store(body);
        self.macros.insert(
            name,
            Macro {
                nargs,
                default,
                body,
                pattern,
                has_params,
            },
        );
    }

    fn define_command(&mut self, kind: DefineKind) -> Result<(), Diagnostic> {
        if self.input.peek() == Some(b'*') {
            self.input.bump(1);
        }
        let name = self.input.read_cs_target();
        let nargs = self
            .input
            .read_optional()
            .and_then(|n| n.trim().parse::<u8>().ok())
            .unwrap_or(0)
            .min(9);
        let default = if nargs > 0 { self.input.read_optional() } else { None };
        let mut body = String::new();
        self.input.read_arg(&mut body);

        if kind == DefineKind::Renew && name == "familydefault" {
            // Owned by the preamble parser: pass through untouched.
            self.out.push_str("\\renewcommand{\\familydefault}{");
            self.out.push_str(&body);
            self.out.push('}');
            self.out_ends_cs = false;
            return Ok(());
        }
        let newlines = body.bytes().filter(|&b| b == b'\n').count();
        let already_defined = self.macros.contains_key(&name) || native::is_native(&name);
        if !(kind == DefineKind::Provide && already_defined) {
            self.insert_macro(name, nargs, default, &body, None);
        }
        self.finish_definition(newlines);
        Ok(())
    }

    fn define_def(&mut self) -> Result<(), Diagnostic> {
        self.input.skip_spaces();
        let mut target = String::new();
        self.input.read_token(&mut target);
        let name = target.trim_start_matches('\\').to_string();

        let mut params = String::new();
        while let Some(b) = self.input.peek() {
            if b == b'{' {
                break;
            }
            let c = self.input.peek_char().unwrap();
            params.push(c);
            self.input.bump(c.len_utf8());
        }
        let mut body = String::new();
        self.input.read_arg(&mut body);

        let pieces = parse_param_text(&params);
        let nargs = pieces.iter().filter(|p| matches!(p, Piece::Param(_))).count() as u8;
        let plain = pieces.iter().all(|p| matches!(p, Piece::Param(_)));
        let pattern = if plain { None } else { Some(pieces.into_boxed_slice()) };
        let newlines = body.bytes().filter(|&b| b == b'\n').count();
        self.insert_macro(name, nargs, None, &body, pattern);
        self.finish_definition(newlines);
        Ok(())
    }

    fn define_let(&mut self) -> Result<(), Diagnostic> {
        self.input.skip_spaces();
        let mut target = String::new();
        self.input.read_token(&mut target);
        self.input.skip_spaces();
        if self.input.peek() == Some(b'=') {
            self.input.bump(1);
            if self.input.peek() == Some(b' ') {
                self.input.bump(1);
            }
        }
        let mut source = String::new();
        self.input.read_token(&mut source);
        let name = target.trim_start_matches('\\').to_string();
        let src_name = source.trim_start_matches('\\');
        if let Some(m) = self.macros.get(src_name) {
            let copy = Macro {
                nargs: m.nargs,
                default: m.default,
                body: m.body,
                pattern: m.pattern.as_ref().map(|p| {
                    p.iter()
                        .map(|x| match x {
                            Piece::Lit(s) => Piece::Lit(s.clone()),
                            Piece::Param(n) => Piece::Param(*n),
                        })
                        .collect()
                }),
                has_params: m.has_params,
            };
            self.macros.insert(name, copy);
        } else {
            self.insert_macro(name, 0, None, &source, None);
        }
        self.finish_definition(0);
        Ok(())
    }

    fn define_environment(&mut self) -> Result<(), Diagnostic> {
        if self.input.peek() == Some(b'*') {
            self.input.bump(1);
        }
        let mut name = String::new();
        self.input.read_arg(&mut name);
        let nargs = self
            .input
            .read_optional()
            .and_then(|n| n.trim().parse::<u8>().ok())
            .unwrap_or(0)
            .min(9);
        let default = if nargs > 0 { self.input.read_optional() } else { None };
        let mut begin = String::new();
        let mut end = String::new();
        self.input.read_arg(&mut begin);
        self.input.read_arg(&mut end);
        let newlines = begin.bytes().chain(end.bytes()).filter(|&b| b == b'\n').count();
        let default = default.map(|d| self.store(&d));
        let begin = self.store(&begin);
        let end = self.store(&end);
        let name = name.trim().to_string();
        if !self.envs.contains_key(&name) {
            self.env_ids.push(name.clone());
        }
        self.envs.insert(
            name,
            EnvDef {
                nargs,
                default,
                begin,
                end,
            },
        );
        self.finish_definition(newlines);
        Ok(())
    }

    fn define_math_operator(&mut self) -> Result<(), Diagnostic> {
        let starred = self.input.peek() == Some(b'*');
        if starred {
            self.input.bump(1);
        }
        let name = self.input.read_cs_target();
        let mut text = String::new();
        self.input.read_arg(&mut text);
        let body = format!("\\operatorname{}{{{text}}}", if starred { "*" } else { "" });
        self.insert_macro(name, 0, None, &body, None);
        self.finish_definition(0);
        Ok(())
    }

    fn define_theorem(&mut self) -> Result<(), Diagnostic> {
        let unnumbered = self.input.peek() == Some(b'*');
        if unnumbered {
            self.input.bump(1);
        }
        let mut env = String::new();
        self.input.read_arg(&mut env);
        let shared = self.input.read_optional();
        let mut title = String::new();
        self.input.read_arg(&mut title);
        let parent = self.input.read_optional();
        let env = env.trim().to_string();

        let counter = if unnumbered {
            None
        } else if let Some(shared) = shared.as_deref().map(str::trim) {
            self.counter_ids.get(shared).copied()
        } else {
            let parent = parent.as_deref().and_then(|p| self.counter_ids.get(p.trim()).copied());
            let id = self.counters.len() as u32;
            self.counters.push(Counter { value: 0, parent });
            self.counter_ids.insert(env.clone(), id);
            Some(id)
        };
        self.theorems.insert(
            env,
            TheoremDef {
                title,
                counter,
                style: self.theorem_style,
            },
        );
        self.finish_definition(0);
        Ok(())
    }

    // ----- environments -------------------------------------------------------

    fn begin_environment(&mut self) -> Result<(), Diagnostic> {
        let mut raw = String::new();
        self.input.read_arg(&mut raw);
        let name = raw.trim().to_string();

        if let Some((nargs, default, begin)) = self.envs.get(&name).map(|e| (e.nargs, e.default, e.begin)) {
            self.count_expansion(&name)?;
            let id = self.env_ids.iter().position(|n| *n == name).unwrap_or(0) as u32;
            let mut args = Vec::with_capacity(nargs as usize);
            let mut first = 0;
            if let Some((ds, de)) = default {
                let opt = self.input.read_optional();
                args.push(opt.unwrap_or_else(|| self.input.arena[ds as usize..de as usize].to_string()));
                first = 1;
            }
            for _ in first..nargs {
                let mut a = String::new();
                self.input.read_arg(&mut a);
                args.push(a);
            }
            self.env_stack.push(EnvEntry {
                name,
                kind: EnvKind::User(id),
            });
            let (idx, mut buf) = self.input.take_owned();
            substitute(&self.input.arena[begin.0 as usize..begin.1 as usize], &args, &mut buf);
            self.input.push_owned(idx, buf);
            return Ok(());
        }

        if self.theorems.contains_key(&name) {
            return self.begin_theorem(name);
        }

        match name.as_str() {
            "verbatim" | "verbatim*" | "lstlisting" => {
                self.emit_cs("begin");
                self.out.push('{');
                self.out.push_str(&name);
                self.out.push('}');
                self.out_ends_cs = false;
                let end = format!("\\end{{{name}}}");
                let rest = self.input.top_rest();
                let n = rest.find(&end).map_or(rest.len(), |p| p + end.len());
                self.out.push_str(&rest[..n]);
                self.input.bump(n);
                return Ok(());
            }
            "comment" => {
                let rest = self.input.top_rest();
                let end = "\\end{comment}";
                let n = rest.find(end).map_or(rest.len(), |p| p + end.len());
                self.input.bump(n);
                return Ok(());
            }
            "proof" => {
                let title = self.input.read_optional().unwrap_or_else(|| "Proof".to_string());
                self.env_stack.push(EnvEntry {
                    name,
                    kind: EnvKind::Proof,
                });
                let head = format!("\n\n\\noindent\\textit{{{title}.}}\\hspace{{0.5em}}");
                self.push_text(&head);
                return Ok(());
            }
            "thebibliography" => {
                let mut widest = String::new();
                self.input.read_arg(&mut widest);
                self.env_stack.push(EnvEntry {
                    name,
                    kind: EnvKind::Bibliography,
                });
                self.emit("\\section*{References}\n\\begin{enumerate}");
                return Ok(());
            }
            "figure" | "figure*" => {
                self.step_counter(C_FIGURE);
                self.current_label = self.format_the(C_FIGURE);
                self.env_stack.push(EnvEntry {
                    name: name.clone(),
                    kind: EnvKind::Float { is_table: false },
                });
            }
            "table" | "table*" | "longtable" => {
                self.env_stack.push(EnvEntry {
                    name: name.clone(),
                    kind: EnvKind::Float { is_table: true },
                });
            }
            "equation" | "align" | "alignat" | "gather" | "multline" | "equation*" | "align*"
            | "alignat*" | "gather*" | "multline*" | "displaymath" => {
                let numbered = !name.ends_with('*') && name != "displaymath";
                let end = format!("\\end{{{name}}}");
                let tag = self.input.peek().and_then(|_| {
                    let rest = self.input.top_rest();
                    let body = &rest[..rest.find(&end).unwrap_or(rest.len())];
                    find_tag(body)
                });
                if let Some(tag) = tag {
                    self.current_label = tag;
                } else if numbered {
                    self.step_counter(C_EQUATION);
                    self.current_label = self.format_the(C_EQUATION);
                }
            }
            _ => {}
        }
        self.emit_cs("begin");
        self.out.push('{');
        self.out.push_str(&name);
        self.out.push('}');
        self.out_ends_cs = false;
        Ok(())
    }

    fn end_environment(&mut self) -> Result<(), Diagnostic> {
        let mut raw = String::new();
        self.input.read_arg(&mut raw);
        let name = raw.trim();
        let matches_top = self.env_stack.last().is_some_and(|e| e.name == name);
        if matches_top {
            let entry = self.env_stack.pop().unwrap();
            match entry.kind {
                EnvKind::User(id) => {
                    let end = self.envs[&self.env_ids[id as usize]].end;
                    self.input.push_arena(end.0, end.1);
                    return Ok(());
                }
                EnvKind::Theorem => {
                    self.push_text("\n\n");
                    return Ok(());
                }
                EnvKind::Proof => {
                    self.push_text("\\hspace{1em}$\\square$\n\n");
                    return Ok(());
                }
                EnvKind::Bibliography => {
                    self.emit("\\end{enumerate}");
                    return Ok(());
                }
                EnvKind::Float { .. } => {}
            }
        }
        let name = name.to_string();
        self.emit_cs("end");
        self.out.push('{');
        self.out.push_str(&name);
        self.out.push('}');
        self.out_ends_cs = false;
        Ok(())
    }

    fn push_text(&mut self, text: &str) {
        let (idx, mut buf) = self.input.take_owned();
        buf.push_str(text);
        self.input.push_owned(idx, buf);
    }

    fn begin_theorem(&mut self, name: String) -> Result<(), Diagnostic> {
        let note = self.input.read_optional();
        let thm = &self.theorems[&name];
        let (counter, style) = (thm.counter, thm.style);
        let title = thm.title.clone();
        let number = counter.map(|id| {
            self.step_counter(id);
            self.format_the(id)
        });
        if let Some(n) = &number {
            self.current_label = n.clone();
        }

        let mut head = String::with_capacity(64);
        head.push_str("\n\n\\noindent");
        let title_cmd = if style == TheoremStyle::Remark { "\\textit{" } else { "\\textbf{" };
        head.push_str(title_cmd);
        head.push_str(&title);
        if let Some(n) = &number {
            head.push(' ');
            head.push_str(n);
        }
        head.push('}');
        if let Some(note) = &note {
            head.push_str(" (");
            head.push_str(note);
            head.push(')');
        }
        head.push_str(title_cmd);
        head.push_str(".}\\hspace{0.5em}");

        // Look ahead for the matching \end in the current frame so the body can be
        // italicised paragraph by paragraph (groups cannot span blank lines or lists).
        let begin_tok = format!("\\begin{{{name}}}");
        let end_tok = format!("\\end{{{name}}}");
        let body_len = self.input.peek().and_then(|_| find_matching_end(self.input.top_rest(), &begin_tok, &end_tok));
        match body_len {
            Some(len) => {
                let (idx, mut buf) = self.input.take_owned();
                buf.push_str(&head);
                let body = &self.input.top_rest()[..len];
                if style == TheoremStyle::Plain {
                    italicize_paragraphs(body, &mut buf);
                } else {
                    buf.push_str(body);
                }
                buf.push_str("\n\n");
                self.input.bump(len + end_tok.len());
                self.input.push_owned(idx, buf);
            }
            None => {
                self.env_stack.push(EnvEntry {
                    name,
                    kind: EnvKind::Theorem,
                });
                self.push_text(&head);
            }
        }
        Ok(())
    }

    // ----- conditionals -------------------------------------------------------

    fn conditional(&mut self, truth: bool) -> Result<(), Diagnostic> {
        if truth || self.skip_branch(true)? {
            self.cond_depth += 1;
        }
        self.skip_hspace();
        Ok(())
    }

    /// TeX drops spaces after a control word; applied to control words this stage consumes.
    fn skip_hspace(&mut self) {
        while matches!(self.input.peek(), Some(b' ' | b'\t')) {
            self.input.bump(1);
        }
    }

    /// Skips input up to the matching `\fi` (consumed), or to `\else` when `stop_at_else`.
    /// Returns `true` when it stopped at `\else`.
    fn skip_branch(&mut self, stop_at_else: bool) -> Result<bool, Diagnostic> {
        let mut depth = 0u32;
        loop {
            let Some(b) = self.input.peek() else {
                return Err(self.error("incomplete conditional: missing \\fi"));
            };
            match b {
                b'\\' => {
                    self.input.bump(1);
                    if self.input.peek().is_none() {
                        continue;
                    }
                    let rest = self.input.top_rest();
                    let n = rest.bytes().take_while(|b| b.is_ascii_alphabetic() || *b == b'@').count();
                    if n == 0 {
                        let c = self.input.peek_char().unwrap();
                        self.input.bump(c.len_utf8());
                        continue;
                    }
                    let name = &rest[..n];
                    let is_if = self.is_if_name(name);
                    let is_fi = name == "fi";
                    let is_else = name == "else";
                    self.input.bump(n);
                    if is_if {
                        depth += 1;
                    } else if is_fi {
                        if depth == 0 {
                            return Ok(false);
                        }
                        depth -= 1;
                    } else if is_else && depth == 0 && stop_at_else {
                        return Ok(true);
                    }
                }
                b'%' => {
                    let rest = self.input.top_rest();
                    let n = rest.find('\n').map_or(rest.len(), |p| p + 1);
                    self.input.bump(n);
                }
                _ => {
                    let rest = self.input.top_rest();
                    let n = rest.bytes().position(|b| b == b'\\' || b == b'%').unwrap_or(rest.len());
                    self.input.bump(n);
                }
            }
        }
    }

    fn is_if_name(&self, name: &str) -> bool {
        let Some(flag) = name.strip_prefix("if") else {
            return false;
        };
        matches!(
            flag,
            "" | "x" | "num" | "dim" | "odd" | "case" | "defined" | "csname" | "mmode" | "vmode"
                | "hmode" | "inner" | "true" | "false" | "cat" | "void" | "eof" | "hbox" | "vbox"
        ) || self.bools.contains_key(flag)
    }

    fn read_int_operand(&mut self) -> i32 {
        self.input.skip_spaces();
        let mut s = String::new();
        if self.input.peek() == Some(b'\\') {
            self.input.read_token(&mut s);
            if s == "\\value" {
                let mut c = String::new();
                self.input.read_arg(&mut c);
                return self.counter_ids.get(c.trim()).map_or(0, |&id| self.counters[id as usize].value);
            }
            if let Some(&id) = s.strip_prefix("\\the").and_then(|c| self.counter_ids.get(c)) {
                return self.counters[id as usize].value;
            }
            return 0;
        }
        while let Some(b) = self.input.peek() {
            if b.is_ascii_digit() || (s.is_empty() && b == b'-') {
                s.push(b as char);
                self.input.bump(1);
            } else {
                break;
            }
        }
        s.parse().unwrap_or(0)
    }

    // ----- counters -------------------------------------------------------------

    fn step_counter(&mut self, id: u32) {
        self.counters[id as usize].value += 1;
        self.reset_children(id);
    }

    fn reset_children(&mut self, id: u32) {
        for child in 0..self.counters.len() {
            if self.counters[child].parent == Some(id) {
                self.counters[child].value = 0;
                self.reset_children(child as u32);
            }
        }
    }

    fn format_the(&self, id: u32) -> String {
        let counter = &self.counters[id as usize];
        match counter.parent {
            Some(parent) => format!("{}.{}", self.format_the(parent), counter.value),
            None => counter.value.to_string(),
        }
    }

    fn eval_int(&self, expr: &str) -> i32 {
        let t = expr.trim();
        if let Some(c) = t.strip_prefix("\\value") {
            let c = c.trim().trim_start_matches('{').trim_end_matches('}').trim();
            return self.counter_ids.get(c).map_or(0, |&id| self.counters[id as usize].value);
        }
        t.parse().unwrap_or(0)
    }

    // ----- finalisation ---------------------------------------------------------

    fn finish(mut self) -> String {
        if self.pending.is_empty() {
            return self.out;
        }
        let mut result = String::with_capacity(self.out.len() + self.pending.len() * 8);
        let mut cursor = 0;
        for p in std::mem::take(&mut self.pending) {
            match p {
                Pending::Ref { at, key, paren } => {
                    result.push_str(&self.out[cursor..at]);
                    let value = self.labels.get(&key).map_or("??", String::as_str);
                    if paren {
                        result.push('(');
                        result.push_str(value);
                        result.push(')');
                    } else {
                        result.push_str(value);
                    }
                    cursor = at;
                }
                Pending::Cite { at, keys, note } => {
                    result.push_str(&self.out[cursor..at]);
                    result.push('[');
                    for (i, key) in keys.split(',').map(str::trim).filter(|k| !k.is_empty()).enumerate() {
                        if i > 0 {
                            result.push_str(", ");
                        }
                        result.push_str(self.bib.get(key).map_or("?", String::as_str));
                    }
                    if let Some(note) = note {
                        result.push_str(", ");
                        result.push_str(&note);
                    }
                    result.push(']');
                    cursor = at;
                }
            }
        }
        result.push_str(&self.out[cursor..]);
        result
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DefineKind {
    New,
    Renew,
    Provide,
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

/// Replaces `#n` with `args[n-1]` and `##` with `#`.
fn substitute(body: &str, args: &[String], out: &mut String) {
    let bytes = body.as_bytes();
    let mut last = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next == b'#' {
                out.push_str(&body[last..i + 1]);
                i += 2;
                last = i;
                continue;
            }
            if next.is_ascii_digit() && next != b'0' {
                out.push_str(&body[last..i]);
                if let Some(arg) = args.get((next - b'1') as usize) {
                    out.push_str(arg);
                }
                i += 2;
                last = i;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&body[last..]);
}

fn parse_param_text(params: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut lit = String::new();
    let mut chars = params.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '#' {
            if let Some(d) = chars.peek().and_then(|d| d.to_digit(10)) {
                chars.next();
                if !lit.is_empty() {
                    pieces.push(Piece::Lit(std::mem::take(&mut lit).into_boxed_str()));
                }
                pieces.push(Piece::Param(d as u8));
                continue;
            }
        }
        lit.push(c);
    }
    if !lit.trim().is_empty() {
        pieces.push(Piece::Lit(lit.into_boxed_str()));
    }
    pieces
}

fn strip_outer_braces(s: &mut String) {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('{') && t.ends_with('}') {
        let inner = &t[1..t.len() - 1];
        // Only strip when the braces enclose the whole argument.
        let mut depth = 0i32;
        let balanced = inner.bytes().all(|b| {
            match b {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            depth >= 0
        });
        if balanced && depth == 0 {
            *s = inner.to_string();
        }
    }
}

fn find_tag(body: &str) -> Option<String> {
    let pos = body.find("\\tag")?;
    let rest = body[pos + 4..].trim_start_matches('*').trim_start();
    let inner = rest.strip_prefix('{')?;
    let mut depth = 0u32;
    for (i, b) in inner.bytes().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' if depth == 0 => return Some(inner[..i].to_string()),
            b'}' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// Byte length of the body before the `end_tok` matching an already-consumed `begin_tok`.
fn find_matching_end(text: &str, begin_tok: &str, end_tok: &str) -> Option<usize> {
    let mut depth = 0u32;
    let mut i = 0;
    while let Some(off) = text[i..].find('\\') {
        let at = i + off;
        let rest = &text[at..];
        if rest.starts_with(end_tok) {
            if depth == 0 {
                return Some(at);
            }
            depth -= 1;
            i = at + end_tok.len();
        } else if rest.starts_with(begin_tok) {
            depth += 1;
            i = at + begin_tok.len();
        } else {
            i = at + 1;
        }
    }
    None
}

/// Wraps each plain paragraph in `\textit{…}`; paragraphs holding block constructs stay upright.
fn italicize_paragraphs(body: &str, out: &mut String) {
    let mut rest = body;
    loop {
        let (para, sep, next) = match find_blank_line(rest) {
            Some((s, e)) => (&rest[..s], &rest[s..e], Some(&rest[e..])),
            None => (rest, "", None),
        };
        let blocky = ["\\begin", "\\item", "$$", "\\[", "\\section", "\\par"]
            .iter()
            .any(|k| para.contains(k));
        if para.trim().is_empty() || blocky {
            out.push_str(para);
        } else {
            out.push_str("\\textit{");
            out.push_str(para.trim_end());
            if para.contains('%') {
                out.push('\n');
            }
            out.push('}');
        }
        out.push_str(sep);
        match next {
            Some(n) => rest = n,
            None => break,
        }
    }
}

/// Finds the first blank-line separator (`\n`, optional horizontal space, `\n`).
fn find_blank_line(text: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while let Some(off) = bytes[i..].iter().position(|&b| b == b'\n') {
        let start = i + off;
        let mut j = start + 1;
        while j < bytes.len() && matches!(bytes[j], b' ' | b'\t' | b'\r') {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b'\n' {
            let mut end = j + 1;
            while end < bytes.len() && matches!(bytes[end], b'\n' | b'\r') {
                end += 1;
            }
            return Some((start, end));
        }
        i = start + 1;
    }
    None
}

fn format_counter(value: i32, style: &str) -> String {
    let v = value.max(0) as u32;
    match style {
        "roman" => to_roman(v, false),
        "Roman" => to_roman(v, true),
        "alph" | "Alph" if (1..=26).contains(&v) => {
            let base = if style == "alph" { b'a' } else { b'A' };
            ((base + (v - 1) as u8) as char).to_string()
        }
        "alph" | "Alph" => String::new(),
        _ => value.to_string(),
    }
}

fn to_roman(mut v: u32, upper: bool) -> String {
    const TABLE: [(u32, &str); 13] = [
        (1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"),
        (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i"),
    ];
    let mut s = String::new();
    for &(n, r) in &TABLE {
        while v >= n {
            s.push_str(r);
            v -= n;
        }
    }
    if upper {
        s.make_ascii_uppercase();
    }
    s
}

/// Current date as `Month D, YYYY`, honouring `SOURCE_DATE_EPOCH` for reproducible builds.
fn today_string() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        String::new()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let secs = std::env::var("SOURCE_DATE_EPOCH")
            .ok()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs() as i64)
            });
        let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
        const MONTHS: [&str; 12] = [
            "January", "February", "March", "April", "May", "June", "July", "August", "September",
            "October", "November", "December",
        ];
        format!("{} {}, {}", MONTHS[(m - 1) as usize], d, y)
    }
}

/// Howard Hinnant's days-to-civil conversion (proleptic Gregorian calendar).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(src: &str) -> String {
        expand(src).unwrap().into_owned()
    }

    #[test]
    fn borrows_when_nothing_to_expand() {
        let src = "\\documentclass{article}\\begin{document}Hello \\textbf{x}\\end{document}";
        assert!(matches!(expand(src).unwrap(), Cow::Borrowed(_)));
    }

    #[test]
    fn newcommand_with_args_and_default() {
        let out = ex("\\newcommand{\\pair}[2]{(#1,#2)}\\newcommand{\\hi}[1][World]{Hi #1}\\pair{a}{b} \\hi \\hi[You]");
        assert_eq!(out, "(a,b) Hi WorldHi You");
    }

    #[test]
    fn nested_macros_and_trailing_control_word() {
        let out = ex("\\newcommand\\R{\\mathbb{R}}\\newcommand\\al{\\alpha}$x\\in\\R, \\al x$");
        assert_eq!(out, "$x\\in\\mathbb{R}, \\alpha x$");
    }

    #[test]
    fn def_with_delimited_parameters() {
        let out = ex("\\def\\sw(#1,#2){#2#1}\\sw(a,b)");
        assert_eq!(out, "ba");
    }

    #[test]
    fn let_and_providecommand() {
        let out = ex("\\newcommand\\a{A}\\let\\b\\a\\providecommand\\a{Z}\\renewcommand\\a{Y}\\b\\a");
        assert_eq!(out, "AY");
    }

    #[test]
    fn user_environment() {
        let out = ex("\\newenvironment{boxed}[1]{[#1:}{]}\\begin{boxed}{t}x\\end{boxed}");
        assert_eq!(out, "[t:x]");
    }

    #[test]
    fn conditionals() {
        let out = ex("\\newif\\ifdraft\\drafttrue\\ifdraft D\\else F\\fi\\draftfalse\\ifdraft D\\else F\\fi\\iffalse\\iftrue x\\fi\\fi!");
        assert_eq!(out, "DF!");
    }

    #[test]
    fn counters() {
        let out = ex("\\newcounter{c}\\setcounter{c}{3}\\stepcounter{c}\\arabic{c} \\roman{c} \\Alph{c} \\thec");
        assert_eq!(out, "4 iv D 4");
    }

    #[test]
    fn forward_and_backward_references() {
        let src = "See \\ref{s2} and \\eqref{e}.\\section{A}\\section{B}\\label{s2}\\begin{equation}x\\label{e}\\end{equation}Back: \\ref{s2}, \\ref{nope}";
        let out = ex(src);
        assert!(out.starts_with("See 2 and (1)."), "{out}");
        assert!(out.ends_with("Back: 2, ??"), "{out}");
        assert!(!out.contains("\\label"));
    }

    #[test]
    fn subsection_and_tagged_equation_numbers() {
        let src = "\\section{A}\\subsection{B}\\label{b}\\begin{equation}y\\tag{*}\\label{t}\\end{equation}\\ref{b} \\ref{t}";
        assert!(ex(src).ends_with("1.1 *"));
    }

    #[test]
    fn citations_and_bibliography() {
        let src = "\\cite{k2,k1} \\cite[p.~3]{k1}\\begin{thebibliography}{9}\\bibitem{k1} One.\\bibitem[Knu]{k2} Two.\\end{thebibliography}";
        let out = ex(src);
        assert!(out.starts_with("[Knu, 1] [1, p.~3]"), "{out}");
        assert!(out.contains("\\section*{References}\n\\begin{enumerate}\\item[{[1]}]"));
        assert!(out.ends_with("\\end{enumerate}"));
    }

    #[test]
    fn theorems_and_proof() {
        let src = "\\newtheorem{thm}{Theorem}[section]\\newtheorem{lem}[thm]{Lemma}\\section{S}\\begin{thm}[Main]\\label{m}True.\\end{thm}\\begin{lem}Also.\\end{lem}\\begin{proof}Easy.\\end{proof}\\ref{m}";
        let out = ex(src);
        assert!(out.contains("\\textbf{Theorem 1.1} (Main)\\textbf{.}\\hspace{0.5em}\\textit{True.}"), "{out}");
        assert!(out.contains("\\textbf{Lemma 1.2}"), "{out}");
        assert!(out.contains("\\textit{Proof.}\\hspace{0.5em}Easy.\\hspace{1em}$\\square$"), "{out}");
        assert!(out.ends_with("1.1"), "{out}");
    }

    #[test]
    fn verbatim_regions_are_not_expanded() {
        let src = "\\newcommand\\x{X}\\verb|\\x| \\begin{verbatim}\\x\\end{verbatim} \\x";
        assert_eq!(ex(src), "\\verb|\\x| \\begin{verbatim}\\x\\end{verbatim} X");
    }

    #[test]
    fn definition_lines_do_not_create_paragraph_breaks() {
        let out = ex("one\n\\newcommand\\a{A}\ntwo \\a");
        assert!(!out.contains("\n\n"), "{out:?}");
    }

    #[test]
    fn runaway_recursion_is_bounded() {
        let err = expand("\\def\\a{x\\a}\\a").unwrap_err();
        assert!(err.message.contains("possible infinite recursion"), "{}", err.message);
        let err = expand("\\def\\b{\\b\\b}\\b").unwrap_err();
        assert!(err.message.contains("possible infinite recursion"), "{}", err.message);
    }

    #[test]
    fn providecommand_keeps_native_commands() {
        let out = ex("\\providecommand{\\utilde}[1]{X#1}\\providecommand{\\mine}{M}$\\utilde{x}$\\mine");
        assert_eq!(out, "$\\utilde{x}$M");
    }

    #[test]
    fn familydefault_passes_through() {
        let src = "\\renewcommand{\\familydefault}{\\sfdefault}";
        assert_eq!(ex(src), src);
    }

    #[test]
    fn civil_date_conversion() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_744), (2026, 10, 18));
    }
}
