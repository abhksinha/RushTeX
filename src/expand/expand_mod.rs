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
//! - `\pageref` reads the pages of a previous pass, as LaTeX reads its `.aux` file
//!   (`expand_with_pages`); without one it prints `??`.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::diagnostic::{locate_diagnostic, Diagnostic};

/// amsthm `\qed` in text: `\leavevmode\unskip\penalty9999 \hbox{}\nobreak\hfill\quad\hbox{\qedsymbol}`
/// (the \penalty9999 breakpoint is not modelled).
/// Marks a `\let` copy of a builtin control word in a macro body (see `define_let`).
const LET_FROZEN: u8 = 0x01;

const QED: &str = "\\unskip\\nobreak\\hfill\\quad$\\openbox$";

#[path = "expand_native.rs"]
mod native;
#[path = "expand_classes.rs"]
pub(crate) mod classes;

const MAX_EXPANSIONS: u32 = 1_000_000;
const MAX_STACK_DEPTH: usize = 1024;
const MIN_OUTPUT_LIMIT: usize = 64 << 20;

/// Expands user macros, conditionals, counters, and references in `source`.
///
/// Returns the input unchanged (borrowed) when it contains no constructs that need expansion.
pub fn expand(source: &str) -> Result<Cow<'_, str>, Diagnostic> {
    expand_with_pages(source, no_pages()).map(|(out, _)| out)
}

/// [`expand`] for a document that uses `\pageref`: `pages` holds each label's page from a
/// previous pass (the `.aux` file's `\newlabel`s), and each `\label` leaves a
/// `\rushtexpagelabel{N}` whatsit whose page layout records. Returns the label keys in
/// `N` order; empty when the source has no `\pageref`.
pub fn expand_with_pages<'s>(source: &'s str, pages: &'s HashMap<String, u32>) -> Result<(Cow<'s, str>, Vec<String>), Diagnostic> {
    if !needs_expansion(source) {
        return Ok((Cow::Borrowed(source), Vec::new()));
    }
    let mut ex = Expander::new(source);
    ex.track_pages = source.contains("\\pageref");
    ex.known_pages = pages;
    ex.run().map(|(out, keys)| (Cow::Owned(out), keys))
}

/// Expands like [`expand`] but keeps going past errors, for `rushtex --gaps`: each error is
/// recorded and the runaway expansion is abandoned at the source position it came from.
/// At most `MAX_RECOVERED` errors are kept, so pathological input stays bounded.
pub fn expand_recovering(source: &str) -> (Cow<'_, str>, Vec<Diagnostic>) {
    if !needs_expansion(source) {
        return (Cow::Borrowed(source), Vec::new());
    }
    const MAX_RECOVERED: usize = 1000;
    let mut ex = Expander::new(source);
    let mut errors = Vec::new();
    while let Some(b) = ex.input.peek() {
        let step = match b {
            b'\\' => ex.control(),
            b'%' => {
                ex.copy_comment();
                Ok(())
            }
            b if b == ex.short_verb => {
                ex.short_verb_run();
                Ok(())
            }
            _ => {
                ex.copy_run();
                Ok(())
            }
        };
        if let Err(e) = step {
            errors.push(e);
            // Back to the source frame: the macro being expanded is dropped.
            while ex.input.frames.len() > 1 {
                ex.input.pop_frame();
            }
            ex.expansions = 0;
            if errors.len() >= MAX_RECOVERED {
                break;
            }
        }
        if ex.out.len() > ex.out_limit {
            errors.push(ex.error("macro expansion produced too much output (possible runaway recursion)"));
            break;
        }
    }
    (Cow::Owned(ex.finish()), errors)
}

/// The material around the column in `\setbox\@outputbox\hbox{<left>\vbox..{..}<right>}`.
fn column_wrap(body: &str, regs: &crate::engine::Engine) -> Option<(String, String)> {
    let start = body.find("\\setbox\\@outputbox\\hbox")?;
    let rest = &body[start + "\\setbox\\@outputbox\\hbox".len()..];
    let open = rest.find('{')?;
    let inner = &rest[open + 1..open + group_len(&rest[open..])? - 1];
    let v = inner.find("\\vbox")?;
    let vopen = v + inner[v..].find('{')?;
    let vend = vopen + group_len(&inner[vopen..])?;
    Some((wrap_items(&inner[..v], regs)?, wrap_items(&inner[vend..], regs)?))
}

/// Byte length of the balanced `{..}` group `s` starts with.
fn group_len(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// `\vrule [width d]` and `\kern d`/`\hskip d` items as `r<pt>`/`k<pt>`, comma separated.
fn wrap_items(s: &str, regs: &crate::engine::Engine) -> Option<String> {
    let mut out = String::new();
    let mut rest = s.trim_start();
    while !rest.is_empty() {
        let (kind, after) = if let Some(r) = rest.strip_prefix("\\vrule") {
            ('r', r)
        } else if let Some(r) = rest.strip_prefix("\\kern").or_else(|| rest.strip_prefix("\\hskip")) {
            ('k', r)
        } else {
            let r = rest.strip_prefix('%')?;
            rest = r.split_once('\n').map_or("", |(_, t)| t).trim_start();
            continue;
        };
        let after = after.trim_start();
        let (width, tail) = if kind == 'r' {
            match after.strip_prefix("width") {
                Some(w) => wrap_dimen(w.trim_start(), regs)?,
                None => (0.4, after),
            }
        } else {
            wrap_dimen(after, regs)?
        };
        if !out.is_empty() {
            out.push(',');
        }
        out.push(kind);
        out.push_str(&width.to_string());
        rest = tail.trim_start();
    }
    Some(out)
}

/// A dimension: `<factor>?\register` or a literal with its unit; the rest after it.
fn wrap_dimen<'s>(s: &'s str, regs: &crate::engine::Engine) -> Option<(f32, &'s str)> {
    let num = s.bytes().take_while(|b| b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'+')).count();
    let factor = if num == 0 { 1.0 } else { s[..num].parse::<f32>().ok()? };
    let r = &s[num..];
    if let Some(cs) = r.strip_prefix('\\') {
        let n = cs.bytes().take_while(|b| b.is_ascii_alphabetic() || *b == b'@').count();
        let v = crate::backend::scaled::sp_pt(regs.eqtb.word(regs.names.user(&cs[..n])?));
        return Some((factor * v, &cs[n..]));
    }
    let unit = r.bytes().take_while(|b| b.is_ascii_alphabetic()).count();
    Some((crate::doc_config::parse_dimension_pt(&s[..num + unit])?, &r[unit..]))
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
            // `\maketitle` without `\date` prints `\today`, which the expander supplies.
            if name == "maketitle" && !source.contains("\\date") {
                return true;
            }
            if name == "documentclass" {
                let after = &source[end..];
                let after = after.trim_start();
                let after = if after.starts_with('[') { &after[after.find(']').map_or(0, |p| p + 1)..] } else { after };
                if let Some(class) = after.trim_start().strip_prefix('{').and_then(|r| r.split('}').next()) {
                    if classes::class_code(class.trim()).is_some() {
                        return true;
                    }
                }
            }
            if name == "begin" {
                let after = &bytes[end..];
                if after.starts_with(b"{proof}") || after.starts_with(b"{comment}") || after.starts_with(b"{abstract}") {
                    return true;
                }
                if let Some(env) = after.strip_prefix(b"{").and_then(|r| r.iter().position(|&b| b == b'}').map(|e| &r[..e])) {
                    if std::str::from_utf8(env).is_ok_and(|e| declaration_env(e).is_some()) {
                        return true;
                    }
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
            | "chardef"
            | "appendix"
            | "newdimen"
            | "newskip"
            | "newcount"
            | "newlength"
            | "newenvironment"
            | "renewenvironment"
            | "newif"
            | "iftrue"
            | "iffalse"
            | "ifx"
            | "ifdefined"
            | "ifnum"
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
            | "qed"
            | "DeclareMathOperator"
            | "DeclareRobustCommand"
            | "makeatletter"
            | "lowercase"
            | "uppercase"
            | "MakeUppercase"
            | "MakeLowercase"
            | "MakeShortVerb"
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
    /// The engine's catcode table, copied after each change: control words are scanned
    /// with it (§356).
    cats: crate::engine::CatTable,
}

impl<'s> Input<'s> {
    /// Whether byte `b` is a letter (catcode 11) now.
    #[inline]
    fn is_letter(&self, b: u8) -> bool {
        b < 128 && self.cats.get(b as char) == crate::engine::Cat::Letter
    }

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

    /// TeX's `\ignorespaces` on the input: spaces and one end of line, not a blank line.
    fn ignore_spaces(&mut self) {
        if self.peek().is_some() {
            let rest = self.top_rest();
            let n = rest.len() - ignore_spaces(rest).len();
            self.bump(n);
        }
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
        let n = rest.bytes().take_while(|&b| self.is_letter(b)).count();
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

#[derive(Clone)]
enum Piece {
    Lit(Box<str>),
    Param(u8),
}

#[derive(Clone)]
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
    /// A theorem whose `\end` was not found ahead; `true` in the remark style.
    Theorem(bool),
    Proof,
    Bibliography,
    Float { is_table: bool },
    /// `\begin{X}` for a declaration `\X`: a group that starts with the declaration.
    Declaration { par: bool },
    /// The `math` environment: `$..$`.
    InlineMath,
    /// article.cls's one-column `abstract`: `\small`, a centred bold heading, `quotation`.
    Abstract,
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
    cond_depth: u32,
    /// `\begin{document}` has been reached.
    in_document: bool,
    /// `\title`, `\author` and `\date` from the preamble, unexpanded. LaTeX stores their
    /// arguments and expands them at `\maketitle`, so they may use macros defined after
    /// them; they are replayed just before `\begin{document}`, once the preamble's
    /// definitions are all known.
    front_matter: String,
    /// Registers, parameters and grouping (`\newdimen`, `\setlength`, `\begingroup`).
    engine: crate::engine::Engine,
    /// The body of a user `\@makecol`, re-read whenever a register it uses changes.
    makecol: Option<String>,
    env_stack: Vec<EnvEntry>,

    current_label: String,
    labels: HashMap<String, String>,
    /// Whether `\label`s leave page whatsits (the source uses `\pageref`).
    track_pages: bool,
    /// The label keys of the page whatsits, in their `N` order.
    page_keys: Vec<String>,
    /// The previous pass's label pages.
    known_pages: &'s HashMap<String, u32>,
    /// `\label`s inside the rows of a display alignment, bound to their row's number.
    row_labels: HashMap<String, String>,
    /// Alignments nested in a display alignment whose rows that one already numbered.
    numbered_nested_aligns: u32,
    /// Open lists, innermost last: the `enumerate` item counter, or `None` for the others.
    lists: Vec<Option<i32>>,
    /// The list label macros as they were when each open list began: the environment is a
    /// group, so a `\renewcommand{\labelenumi}` inside it ends with it.
    list_label_saves: Vec<[Option<Macro>; 4]>,
    /// After `\appendix`: `\thesection` is `\Alph{section}`.
    appendix: bool,
    bib: HashMap<String, String>,
    bib_count: u32,
    pending: Vec<Pending>,
    /// shortvrb's `\MakeShortVerb` character (`|` in ltxguide), 0 when none: `|x|` is `\verb|x|`.
    short_verb: u8,
    /// `\c@secnumdepth`: deeper headings take no number.
    sec_num_depth: i32,
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
                cats: crate::engine::CatTable::latex(),
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
            cond_depth: 0,
            in_document: false,
            front_matter: String::new(),
            engine: crate::engine::Engine::default(),
            makecol: None,
            env_stack: Vec::with_capacity(16),
            current_label: String::new(),
            labels: HashMap::new(),
            track_pages: false,
            page_keys: Vec::new(),
            known_pages: no_pages(),
            row_labels: HashMap::new(),
            numbered_nested_aligns: 0,
            lists: Vec::new(),
            list_label_saves: Vec::new(),
            appendix: false,
            bib: HashMap::new(),
            bib_count: 0,
            pending: Vec::new(),
            short_verb: 0,
            sec_num_depth: 3,
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

    fn run(mut self) -> Result<(String, Vec<String>), Diagnostic> {
        while let Some(b) = self.input.peek() {
            match b {
                b'\\' => self.control()?,
                b'%' => self.copy_comment(),
                b if b == self.short_verb => self.short_verb_run(),
                _ => self.copy_run(),
            }
            if self.out.len() > self.out_limit {
                return Err(self.error("macro expansion produced too much output (possible runaway recursion)"));
            }
        }
        let keys = std::mem::take(&mut self.page_keys);
        Ok((self.finish(), keys))
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
            .position(|&b| b == b'\\' || b == b'%' || b == self.short_verb)
            .unwrap_or(rest.len());
        let first_alpha = rest.as_bytes()[0].is_ascii_alphabetic();
        if self.out_ends_cs && first_alpha {
            self.out.push(' ');
        }
        self.out.push_str(&rest[..n]);
        self.out_ends_cs = false;
        self.input.bump(n);
    }

    /// `|text|` under `\MakeShortVerb{\|}`: shortvrb makes the character active as `\verb|`.
    fn short_verb_run(&mut self) {
        self.emit_cs("verb");
        self.out_ends_cs = false;
        self.copy_verb();
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
        // §356: a control word is the letters (catcode 11) after the escape.
        let is_letter = |b: u8| self.input.is_letter(b);
        if first == LET_FROZEN {
            // A `\let` copy of a builtin: its meaning as it was, whatever the name means now.
            self.input.bump(1);
            let rest = self.input.top_rest();
            let n = rest.bytes().take_while(|&b| b.is_ascii_alphabetic() || b == b'@').count();
            let name = rest[..n].to_string();
            self.input.bump(n);
            return self.dispatch_builtin(name);
        }
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
        self.dispatch_builtin(name)
    }

    /// A control word that is not a macro: the expander's builtins, or passed on to the parser.
    fn dispatch_builtin(&mut self, name: String) -> Result<(), Diagnostic> {
        match name.as_str() {
            "documentclass" => {
                // The class's own code runs after the `\documentclass` line.
                let options = self.input.read_optional();
                let mut class = String::new();
                self.input.read_arg(&mut class);
                self.emit_cs("documentclass");
                if let Some(o) = &options {
                    self.out.push('[');
                    self.out.push_str(o);
                    self.out.push(']');
                }
                self.out.push('{');
                self.out.push_str(&class);
                self.out.push('}');
                self.out_ends_cs = false;
                if let Some(code) = classes::class_code(class.trim()) {
                    self.push_text(code);
                }
                Ok(())
            }
            "newcommand" => self.define_command(DefineKind::New),
            "renewcommand" => self.define_command(DefineKind::Renew),
            "providecommand" => self.define_command(DefineKind::Provide),
            // Robust or not makes no difference without `\protect`; it defines or redefines.
            "DeclareRobustCommand" => self.define_command(DefineKind::Renew),
            "title" | "author" | "date" if !self.in_document => {
                let mut arg = String::new();
                self.input.read_arg(&mut arg);
                let newlines = arg.bytes().filter(|&b| b == b'\n').count();
                self.front_matter.push('\\');
                self.front_matter.push_str(&name);
                self.front_matter.push('{');
                self.front_matter.push_str(&arg);
                self.front_matter.push_str("}\n");
                self.finish_definition(newlines);
                Ok(())
            }
            "def" | "gdef" | "edef" | "xdef" => self.define_def(),
            "let" => self.define_let(),
            "chardef" => self.define_chardef(),
            "appendix" => {
                self.counters[C_SECTION as usize].value = 0;
                self.reset_children(C_SECTION);
                self.appendix = true;
                self.emit_cs("appendix");
                Ok(())
            }
            // Dimension registers keep their `\setlength` values for `\@makecol`; the others
            // only consume their name.
            "newdimen" | "newskip" | "newcount" | "newlength" => {
                self.input.skip_spaces();
                let mut target = String::new();
                if self.input.peek() == Some(b'{') {
                    self.input.read_arg(&mut target);
                } else {
                    self.input.read_token(&mut target);
                }
                // latex.ltx: `\newlength` is `\newskip`.
                let region = match name.as_str() {
                    "newcount" => crate::engine::Region::Int,
                    "newdimen" => crate::engine::Region::Dimen,
                    _ => crate::engine::Region::Glue,
                };
                self.engine.names.allocate(target.trim().trim_start_matches('\\'), region);
                self.finish_definition(0);
                Ok(())
            }
            "setlength" | "addtolength" if !self.engine.names.is_empty() => {
                let (mut reg, mut value) = (String::new(), String::new());
                self.input.read_arg(&mut reg);
                self.input.read_arg(&mut value);
                let key = reg.trim().trim_start_matches('\\');
                self.out.push('\\');
                self.out.push_str(&name);
                self.out.push('{');
                self.out.push_str(&reg);
                self.out.push_str("}{");
                self.out.push_str(&value);
                self.out.push('}');
                self.out_ends_cs = false;
                if self.assign_length(key, &value, name == "addtolength") && self.makecol.as_deref().is_some_and(|b| b.contains(reg.trim())) {
                    self.emit_column_wrap();
                }
                Ok(())
            }
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
            // §1063 `\begingroup`/§1065 `\endgroup`: the engine's group; the parser still sees them.
            "begingroup" => {
                let _ = self.engine.eqtb.begin_group();
                self.emit_cs("begingroup");
                Ok(())
            }
            "endgroup" => {
                self.engine.eqtb.end_group();
                self.input.cats = *self.engine.eqtb.cats();
                self.emit_cs("endgroup");
                Ok(())
            }
            // latex.ltx `\makeatletter`/`\makeatother`: `\catcode`\@11` and `12`, local.
            "makeatletter" | "makeatother" => {
                let cat = if name == "makeatletter" { crate::engine::Cat::Letter } else { crate::engine::Cat::Other };
                self.engine.eqtb.set_catcode(b'@', cat, false);
                self.input.cats = *self.engine.eqtb.cats();
                Ok(())
            }
            "catcode" => {
                if !self.assign_catcode() {
                    self.emit_cs("catcode");
                }
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
                if name == "setcounter" && matches!(c.trim(), "secnumdepth" | "tocdepth") {
                    // The parser numbers the headings and fills the contents.
                    if c.trim() == "secnumdepth" {
                        self.sec_num_depth = value;
                    }
                    self.emit(&format!("\\setcounter{{{}}}{{{value}}}", c.trim()));
                    return Ok(());
                }
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
                let c = c.trim();
                let value = match self.counter_ids.get(c) {
                    Some(&id) => self.counters[id as usize].value,
                    // `enumi`…`enumiv` are the open `enumerate` counters.
                    None => ENUM_COUNTERS
                        .iter()
                        .position(|n| *n == c)
                        .and_then(|d| self.lists.iter().flatten().nth(d).copied())
                        .unwrap_or(0),
                };
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
                let key = key.trim();
                let value = self.row_labels.remove(key).unwrap_or_else(|| self.current_label.clone());
                self.labels.insert(key.to_string(), value);
                if self.track_pages {
                    // latex.ltx `\label`: `\protected@write\@auxout{}{\string\newlabel{key}{{..}{\thepage}}}`.
                    self.out_ends_cs_guard();
                    let n = self.page_keys.len();
                    self.page_keys.push(key.to_string());
                    self.out.push_str("\\rushtexpagelabel{");
                    self.out.push_str(&n.to_string());
                    self.out.push('}');
                }
                // latex.ltx `\@esphack`: after a positive skip the spaces that follow are ignored.
                if ends_with_hskip(&self.out) {
                    self.input.ignore_spaces();
                }
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
                match self.known_pages.get(key.trim()) {
                    Some(page) => self.emit(&page.to_string()),
                    None => self.emit(UNDEFINED_REF),
                }
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
            // amsthm `\qed` outside math, like the end of a `proof`.
            "qed" => {
                self.push_text(QED);
                Ok(())
            }
            "lowercase" | "uppercase" | "MakeUppercase" | "MakeLowercase" => {
                let mut arg = String::new();
                self.input.read_arg(&mut arg);
                let upper = matches!(name.as_str(), "uppercase" | "MakeUppercase");
                // `\MakeUppercase` leaves math alone; TeX's primitives change every letter token.
                let text = change_case(&arg, upper, name.starts_with("Make"));
                self.push_text(&text);
                Ok(())
            }
            "today" => {
                let text = today_string();
                self.emit(&text);
                Ok(())
            }
            "item" => {
                // latex.ltx `\@item`: without an optional label, an `enumerate` item does
                // `\refstepcounter{enum<depth>}`.
                let mut label = None;
                if self.input.peek() != Some(b'[') {
                    if let Some(Some(n)) = self.lists.last_mut() {
                        *n += 1;
                        self.current_label = self.enum_ref();
                        // A user `\labelenum<depth>` is the label; the default ones are the
                        // layout's markers.
                        let depth = self.lists.iter().flatten().count();
                        label = ENUM_LABELS.get(depth - 1).filter(|l| self.macros.contains_key(**l));
                    }
                }
                self.emit_cs("item");
                if let Some(l) = label {
                    self.push_text(&format!("[{{\\{l}}}]"));
                }
                Ok(())
            }
            "begin" => self.begin_environment(),
            "end" => self.end_environment(),
            "section" | "subsection" | "subsubsection" => {
                let starred = self.input.peek() == Some(b'*');
                let level = match name.as_str() {
                    "section" => 1,
                    "subsection" => 2,
                    _ => 3,
                };
                // `\@startsection` steps the counter only for numbered levels.
                if !starred && level <= self.sec_num_depth {
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
            "MakeShortVerb" | "DeleteShortVerb" => {
                let _star = self.input.peek() == Some(b'*') && {
                    self.input.bump(1);
                    true
                };
                let mut arg = String::new();
                self.input.read_arg(&mut arg);
                let c = arg.trim().trim_start_matches('\\').bytes().next().unwrap_or(0);
                self.short_verb = if name == "MakeShortVerb" && c.is_ascii_punctuation() { c } else { 0 };
                self.finish_definition(0);
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
            (None, None) if a == b => true,
            // Undefined control sequences all have the same meaning, which is how
            // `\ifx\foo\undefined` tests for a definition. The expander does not know every
            // command the engine defines, so a name counts as undefined only when it is
            // compared with one of the conventional undefined names.
            (None, None) => (is_undefined_name(na) && !engine_defines(nb)) || (is_undefined_name(nb) && !engine_defines(na)),
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
        let makecol = name == "@makecol";
        self.insert_macro(name, nargs, None, &body, pattern);
        if makecol {
            self.makecol = Some(body);
            self.emit_column_wrap();
        }
        self.finish_definition(newlines);
        Ok(())
    }

    /// An environment is a group (latex.ltx `\begin`): its local assignments end with it.
    fn push_env(&mut self, entry: EnvEntry) {
        // Past TeX's own limit the document is runaway; grouping just stops nesting.
        let _ = self.engine.eqtb.begin_group();
        self.env_stack.push(entry);
    }

    /// `\setlength{\reg}{value}` (or `\addtolength`) on a register the engine knows, in
    /// scaled points; whether it was one.
    fn assign_length(&mut self, key: &str, value: &str, add: bool) -> bool {
        use crate::engine::{engine_scan, Region};
        let Some(reg) = self.engine.names.user(key) else { return false };
        let (em, ex) = (crate::backend::scaled::to_sp(11.0) as i64, crate::backend::scaled::to_sp(5.5) as i64);
        let e = &self.engine;
        let lookup = |n: &str| e.names.lookup(n).map(|r| e.eqtb.glue(r));
        let Some(mut g) = engine_scan::scan_glue(value, em, ex, &lookup) else { return false };
        if reg.region != Region::Glue {
            g = crate::engine::Glue { width: g.width, ..Default::default() };
        }
        if add {
            let old = self.engine.eqtb.glue(reg);
            g.width = g.width.saturating_add(old.width);
            if g.stretch_order == old.stretch_order {
                g.stretch = g.stretch.saturating_add(old.stretch);
            }
            if g.shrink_order == old.shrink_order {
                g.shrink = g.shrink.saturating_add(old.shrink);
            }
        }
        self.engine.eqtb.set_glue(reg, g, false);
        true
    }

    /// Emits `\rushtexcolumnwrap{left}{right}` for a user `\@makecol` that sets
    /// `\@outputbox` to `\hbox{<left> \vbox to\@colht{..} <right>}`: each side a list of
    /// `r<width>` (`\vrule`) and `k<width>` (`\kern`, `\hskip`) items in pt. Anything
    /// else clears the wrap.
    /// §1232 `def_code` for `\catcode`: `<char number>[=]<code>`, a local assignment. `false`
    /// (nothing read) when the operands are not numbers.
    fn assign_catcode(&mut self) -> bool {
        if self.input.peek().is_none() {
            return false;
        }
        let rest = self.input.top_rest();
        let int = |n: &str| self.int_value(n);
        let Some((ch, a)) = crate::engine::engine_scan::scan_int(rest, &int) else {
            return false;
        };
        let after = rest[a..].trim_start_matches(' ');
        let after = after.strip_prefix('=').unwrap_or(after);
        let Some((code, b)) = crate::engine::engine_scan::scan_int(after, &int) else {
            return false;
        };
        let used = rest.len() - after.len() + b;
        let (Ok(ch), Some(cat)) = (u8::try_from(ch), u8::try_from(code).ok().and_then(crate::engine::Cat::from_u8)) else {
            return false;
        };
        self.input.bump(used);
        self.engine.eqtb.set_catcode(ch, cat, false);
        self.input.cats = *self.engine.eqtb.cats();
        true
    }

    /// An internal integer by name: a count register or parameter, or a `\chardef` constant
    /// (`\@ne`, `\m@ne`).
    fn int_value(&self, name: &str) -> Option<i32> {
        if let Some(reg) = self.engine.lookup(name).filter(|r| r.region == crate::engine::Region::Int) {
            return Some(self.engine.eqtb.word(reg));
        }
        let m = self.macros.get(name)?;
        let body = self.input.arena.get(m.body.0 as usize..m.body.1 as usize)?;
        body.strip_prefix("\\char")?.trim().parse().ok()
    }

    fn emit_column_wrap(&mut self) {
        let (left, right) = self.makecol.as_deref().and_then(|b| column_wrap(b, &self.engine)).unwrap_or_default();
        self.out.push_str("\\rushtexcolumnwrap{");
        self.out.push_str(&left);
        self.out.push_str("}{");
        self.out.push_str(&right);
        self.out.push('}');
        self.out_ends_cs = false;
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
        } else if source.len() > 1 && source[1..].bytes().all(|b| b.is_ascii_alphabetic() || b == b'@') {
            // tex.web §1221: `\let` copies the current meaning, so a later `\def` of the
            // source does not change the copy (`\let\oldcs\cs \def\cs{..\oldcs..}`).
            let frozen = format!("\\{}{}", LET_FROZEN as char, src_name);
            self.insert_macro(name, 0, None, &frozen, None);
        } else {
            self.insert_macro(name, 0, None, &source, None);
        }
        self.finish_definition(0);
        Ok(())
    }

    /// `\chardef\cs=<number>`: `\cs` typesets that character code (`\char`). The number
    /// may be decimal, `'` octal, `"` hex or `` `c `` / `` `\c `` (tex.web §442).
    fn define_chardef(&mut self) -> Result<(), Diagnostic> {
        self.input.skip_spaces();
        let mut target = String::new();
        self.input.read_token(&mut target);
        self.input.skip_spaces();
        if self.input.peek() == Some(b'=') {
            self.input.bump(1);
        }
        self.input.skip_spaces();
        let code = match self.input.peek() {
            Some(b'`') => {
                self.input.bump(1);
                if self.input.peek() == Some(b'\\') {
                    self.input.bump(1);
                }
                let c = self.input.peek().unwrap_or(b' ');
                self.input.bump(1);
                c as u32
            }
            Some(q @ (b'\'' | b'"')) => {
                self.input.bump(1);
                let radix = if q == b'"' { 16 } else { 8 };
                let mut n = 0;
                while let Some(d) = self.input.peek().and_then(|b| (b as char).to_digit(radix)) {
                    n = n * radix + d;
                    self.input.bump(1);
                }
                n
            }
            _ => {
                let mut n = 0;
                while let Some(d) = self.input.peek().and_then(|b| (b as char).to_digit(10)) {
                    n = n * 10 + d;
                    self.input.bump(1);
                }
                n
            }
        };
        // One optional space ends the number.
        if self.input.peek() == Some(b' ') {
            self.input.bump(1);
        }
        let name = target.trim_start_matches('\\').to_string();
        self.insert_macro(name, 0, None, &format!("\\char{code} "), None);
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

    /// The environment `name` stands for when `\name` was `\let` to another environment's
    /// command (`\let\Foo\quote`, `\let\endFoo\endquote`): `\begin{Foo}` runs `\Foo`,
    /// so it opens `quote`.
    fn resolve_env_alias(&self, mut name: String) -> String {
        for _ in 0..8 {
            if self.envs.contains_key(&name) || self.theorems.contains_key(&name) {
                break;
            }
            let Some(m) = self.macros.get(&name).filter(|m| m.nargs == 0 && !m.has_params && m.pattern.is_none()) else {
                break;
            };
            let body = self.input.arena[m.body.0 as usize..m.body.1 as usize].trim();
            // A `\let` copy of a builtin names it after the `LET_FROZEN` marker.
            let Some(target) = body.strip_prefix('\\').map(|t| t.strip_prefix(LET_FROZEN as char).unwrap_or(t)).filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_alphabetic())) else {
                break;
            };
            if target == name {
                break;
            }
            name = target.to_string();
        }
        name
    }

    fn begin_environment(&mut self) -> Result<(), Diagnostic> {
        let mut raw = String::new();
        self.input.read_arg(&mut raw);
        let name = self.resolve_env_alias(raw.trim().to_string());
        if name == "document" && !self.in_document {
            self.in_document = true;
            if !self.front_matter.is_empty() {
                let mut replay = std::mem::take(&mut self.front_matter);
                // latex.ltx starts `\@date` as `\today`.
                if !replay.contains("\\date{") {
                    replay.push_str("\\date{\\today}\n");
                }
                replay.push_str("\\begin{document}");
                self.push_text(&replay);
                return Ok(());
            }
        }

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
            self.push_env(EnvEntry {
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

        if name == "abstract" {
            // article.cls (not titlepage, one column); the environment is a group.
            let heading = if self.macros.contains_key("abstractname") { "\\abstractname" } else { "Abstract" };
            self.push_text(&format!("{{\\small\\begin{{center}}{{\\bfseries {heading}\\vspace{{-.5em}}\\vspace{{0pt}}}}\\end{{center}}\\begin{{quotation}}"));
            self.push_env(EnvEntry { name, kind: EnvKind::Abstract });
            return Ok(());
        }

        // LaTeX's `\begin{X}` runs `\X` inside a group when no environment X exists, so
        // every declaration is also an environment. `sloppypar` adds a `\par` at each end.
        if let Some(decl) = declaration_env(&name) {
            let par = name == "sloppypar";
            self.push_text(&format!("{}{{\\{decl} ", if par { "\\par" } else { "" }));
            self.push_env(EnvEntry { name, kind: EnvKind::Declaration { par } });
            return Ok(());
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
            "math" => {
                self.push_env(EnvEntry { name, kind: EnvKind::InlineMath });
                self.emit("$");
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
                self.push_env(EnvEntry {
                    name,
                    kind: EnvKind::Proof,
                });
                // amsthm: `\item[\hskip\labelsep\itshape #1.]\ignorespaces`, a label box with
                // no italic correction, then `\labelsep`.
                self.input.ignore_spaces();
                // The wrapper goes straight out: expanded, it would cover `proof` on the stack.
                self.emit("\n\n\\begin{rushtexproof}");
                let head = format!("\\noindent{{\\itshape {title}.}}\\hspace{{0.5em}}");
                self.push_text(&head);
                return Ok(());
            }
            "thebibliography" => {
                let mut widest = String::new();
                self.input.read_arg(&mut widest);
                self.push_env(EnvEntry {
                    name,
                    kind: EnvKind::Bibliography,
                });
                // article.cls: `\leftmargin` is the width of `\@biblabel{#1}` plus `\labelsep`.
                let widest: String = widest.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
                // It also sets `\sloppy` and `\sfcode`\.\@m`, both ended with the list.
                self.emit(&format!(
                    "\\section*{{References}}\n\\rushtexperiodsf{{1}}\\sloppy\n\\begin{{enumerate}}[rushtexbib={widest}]"
                ));
                return Ok(());
            }
            "enumerate" => {
                self.save_list_labels();
                self.lists.push(Some(0));
            }
            "itemize" | "description" => {
                self.save_list_labels();
                self.lists.push(None);
            }
            "figure" | "figure*" => {
                self.step_counter(C_FIGURE);
                self.current_label = self.format_the(C_FIGURE);
                self.push_env(EnvEntry {
                    name: name.clone(),
                    kind: EnvKind::Float { is_table: false },
                });
            }
            "table" | "table*" | "longtable" => {
                self.push_env(EnvEntry {
                    name: name.clone(),
                    kind: EnvKind::Float { is_table: true },
                });
            }
            "align" | "alignat" | "gather" | "flalign" | "align*" | "alignat*" | "gather*" | "flalign*" => {
                // amsmath numbers each row of these (`\incr@eqnum` as the row ends).
                let end = format!("\\end{{{name}}}");
                if self.numbered_nested_aligns > 0 {
                    self.numbered_nested_aligns -= 1;
                } else if self.input.peek().is_some() {
                    let rest = self.input.top_rest();
                    let body = rest[..find_matching_end(rest, &format!("\\begin{{{name}}}"), &end).unwrap_or(rest.len())].to_string();
                    self.number_rows(&body, !name.ends_with('*'));
                }
            }
            "equation" | "multline" | "equation*" | "multline*" | "displaymath" => {
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
        let resolved = self.resolve_env_alias(raw.trim().to_string());
        let name = resolved.as_str();
        let matches_top = self.env_stack.last().is_some_and(|e| e.name == name);
        if matches_top {
            let entry = self.env_stack.pop().unwrap();
            self.engine.eqtb.end_group();
            self.input.cats = *self.engine.eqtb.cats();
            match entry.kind {
                EnvKind::User(id) => {
                    let end = self.envs[&self.env_ids[id as usize]].end;
                    self.input.push_arena(end.0, end.1);
                    return Ok(());
                }
                EnvKind::Theorem(remark) => {
                    self.push_text(if remark { "\n\n\\end{rushtextheoremhalf}\n\n" } else { "\n\n\\end{rushtextheorem}\n\n" });
                    return Ok(());
                }
                EnvKind::Proof => {
                    self.emit(QED);
                    self.emit("\n\n\\end{rushtexproof}\n\n");
                    return Ok(());
                }
                EnvKind::Bibliography => {
                    self.emit("\\end{enumerate}\\rushtexperiodsf{0}\\fussy");
                    return Ok(());
                }
                EnvKind::Abstract => {
                    self.push_text("\\end{quotation}}");
                    return Ok(());
                }
                EnvKind::Declaration { par } => {
                    self.push_text(if par { "\\par}" } else { "}" });
                    return Ok(());
                }
                EnvKind::InlineMath => {
                    self.emit("$");
                    return Ok(());
                }
                EnvKind::Float { .. } => {}
            }
        }
        if matches!(name, "enumerate" | "itemize" | "description") {
            self.lists.pop();
            if let Some(saved) = self.list_label_saves.pop() {
                for (name, m) in ENUM_LABELS.iter().zip(saved) {
                    match m {
                        Some(m) => self.macros.insert(name.to_string(), m),
                        None => self.macros.remove(*name),
                    };
                }
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

    /// `\p@enumN\theenumN` of the innermost `enumerate` item (article.cls): `2`, `2a`,
    /// `2(a)iii`, `2(a)iiiB`.
    fn save_list_labels(&mut self) {
        let saved = ENUM_LABELS.map(|n| self.macros.get(n).cloned());
        self.list_label_saves.push(saved);
    }

    fn enum_ref(&self) -> String {
        let mut out = String::new();
        let counts: Vec<i32> = self.lists.iter().filter_map(|l| *l).collect();
        for (depth, &n) in counts.iter().enumerate() {
            match depth {
                0 => out.push_str(&format_counter(n, "arabic")),
                1 => out.push_str(&format_counter(n, "alph")),
                2 => {
                    // `\p@enumiii` is `\theenumi(\theenumii)`.
                    let last = out.pop();
                    out.push('(');
                    out.extend(last);
                    out.push(')');
                    out.push_str(&format_counter(n, "roman"));
                }
                _ => out.push_str(&format_counter(n, "Alph")),
            }
        }
        out
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

        // amsthm `\@thm` is a trivlist; the parser takes its spacing from the wrapper.
        // The body font is the wrapper's (`\itshape` for `plain`); the head sets its own
        // (`\thm@headfont`, the note in `\thm@notefont`: medium upright).
        let wrapper = match style {
            TheoremStyle::Plain => "rushtextheorem",
            TheoremStyle::Definition => "rushtextheoremup",
            TheoremStyle::Remark => "rushtextheoremhalf",
        };
        let head_font = if style == TheoremStyle::Remark { "\\normalfont\\itshape " } else { "\\normalfont\\bfseries " };
        let mut head = String::with_capacity(64);
        head.push_str("\n\n\\begin{");
        head.push_str(wrapper);
        head.push_str("}\\noindent{");
        head.push_str(head_font);
        head.push_str(&title);
        // amsthm `\thmnumber{ \@upn{#2}}`: the number is upright in every style.
        if let Some(n) = &number {
            head.push_str(" \\textup{");
            head.push_str(n);
            head.push('}');
        }
        if let Some(note) = &note {
            head.push_str(" {\\normalfont (");
            head.push_str(note);
            head.push_str(")}");
        }
        // amsthm `\thm@headpunct` and `\thm@headsep`; `\ignorespaces` drops the space
        // after `\begin{..}`.
        head.push_str(".}\\hspace{5pt plus 1pt minus 1pt}");

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
                buf.push_str(ignore_spaces(body));
                buf.push_str("\n\n\\end{");
                buf.push_str(wrapper);
                buf.push_str("}\n\n");
                self.input.bump(len + end_tok.len());
                self.input.push_owned(idx, buf);
            }
            None => {
                self.push_env(EnvEntry {
                    name,
                    kind: EnvKind::Theorem(style == TheoremStyle::Remark),
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
                    let n = rest.bytes().take_while(|&b| self.input.is_letter(b)).count();
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

    /// The numbers of a display alignment's rows: each row not `\tag`ged, `\notag`ged or
    /// holding a nested alignment steps `equation` when the environment is numbered; a
    /// nested alignment (amsmath `\ingather@`) numbers its own rows, and a starred one
    /// clears the row's number. A row's `\label`s refer to its number.
    fn number_rows(&mut self, body: &str, numbered: bool) {
        for row in split_display_rows(body) {
            let nested = row.match_indices("\\begin{").find_map(|(at, _)| {
                let rest = &row[at + 7..];
                let env = &rest[..rest.find('}')?];
                if !ALIGN_ENVS.contains(&env) {
                    return None;
                }
                let inner = &rest[env.len() + 1..];
                let len = find_matching_end(inner, &format!("\\begin{{{env}}}"), &format!("\\end{{{env}}}")).unwrap_or(inner.len());
                Some((inner[..len].to_string(), !env.ends_with('*')))
            });
            if let Some((inner, inner_numbered)) = nested {
                self.numbered_nested_aligns += 1;
                self.number_rows(&inner, inner_numbered);
                continue;
            }
            let label = if let Some(tag) = find_tag(row) {
                tag
            } else if numbered && !row.contains("\\notag") && !row.contains("\\nonumber") {
                self.step_counter(C_EQUATION);
                self.format_the(C_EQUATION)
            } else {
                continue;
            };
            let mut rest = row;
            while let Some(at) = rest.find("\\label{") {
                rest = &rest[at + 7..];
                let key = rest.split('}').next().unwrap_or("").trim();
                self.row_labels.insert(key.to_string(), label.clone());
            }
            self.current_label = label;
        }
    }

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
        let own = if id == C_SECTION && self.appendix {
            format_counter(counter.value, "Alph")
        } else {
            counter.value.to_string()
        };
        match counter.parent {
            Some(parent) => format!("{}.{own}", self.format_the(parent)),
            None => own,
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
                    let value = self.labels.get(&key).map_or(UNDEFINED_REF, String::as_str);
                    if paren {
                        // amsmath `\eqref`: `\textup{\tagform@{..}}`.
                        result.push_str("\\textup{(");
                        result.push_str(value);
                        result.push_str(")}");
                    } else {
                        result.push_str(value);
                    }
                    cursor = at;
                }
                Pending::Cite { at, keys, note } => {
                    result.push_str(&self.out[cursor..at]);
                    result.push('[');
                    for (i, key) in keys.split(',').map(str::trim).filter(|k| !k.is_empty()).enumerate() {
                        // latex.ltx `\@citea`: `,\penalty\@m\ `, a space at space factor 1000.
                        if i > 0 {
                            result.push_str(",\\@ ");
                        }
                        // latex.ltx `\@citex`: `\hbox{\reset@font\bfseries ?}`.
                        result.push_str(self.bib.get(key).map_or("\\mbox{\\normalfont\\bfseries ?}", String::as_str));
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

/// The declaration that `\begin{name}` runs, for the declarations that rushtex knows.
fn declaration_env(name: &str) -> Option<&'static str> {
    Some(match name {
        "sloppypar" => "sloppy",
        "em" => "em",
        "bfseries" => "bfseries",
        "mdseries" => "mdseries",
        "itshape" => "itshape",
        "slshape" => "slshape",
        "upshape" => "upshape",
        "scshape" => "scshape",
        "ttfamily" => "ttfamily",
        "sffamily" => "sffamily",
        "rmfamily" => "rmfamily",
        "normalfont" => "normalfont",
        "tiny" => "tiny",
        "scriptsize" => "scriptsize",
        "footnotesize" => "footnotesize",
        "small" => "small",
        "normalsize" => "normalsize",
        "large" => "large",
        "Large" => "Large",
        "LARGE" => "LARGE",
        "huge" => "huge",
        "Huge" => "Huge",
        _ => return None,
    })
}

/// Replaces `#n` with `args[n-1]` and `##` with `#`.
/// Names left undefined on purpose so `\ifx` can test against them.
fn is_undefined_name(name: &str) -> bool {
    matches!(name, "undefined" | "@undefined" | "@nil@undefined")
}

/// Control sequences pdfTeX and the LaTeX kernel define that documents test with
/// `\ifx...\undefined` to detect the engine or format. Everything else that is not a user
/// macro is assumed to be undefined in such a test.
fn engine_defines(name: &str) -> bool {
    name.starts_with("pdf") || matches!(name, "documentclass" | "NeedsTeXFormat" | "@preamblecmds" | "@notprerr" | "relax" | "fmtname")
}

fn starts_with_letter(text: &str) -> bool {
    text.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
}

/// Whether `text` ends in a control word (`\name`, the backslash not itself escaped).
fn ends_with_control_word(text: &str) -> bool {
    let bytes = text.as_bytes();
    let letters = bytes.iter().rev().take_while(|b| b.is_ascii_alphabetic()).count();
    if letters == 0 || letters == bytes.len() {
        return false;
    }
    let slashes = bytes[..bytes.len() - letters].iter().rev().take_while(|&&b| b == b'\\').count();
    slashes % 2 == 1
}

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
                    // Tokens cannot fuse: `\bfseries#1` with `word` is `\bfseries` then
                    // `word`, so a space (skipped after a control word) keeps them apart, on
                    // both sides of the argument.
                    if starts_with_letter(arg) && ends_with_control_word(out) {
                        out.push(' ');
                    }
                    out.push_str(arg);
                    if ends_with_control_word(arg) && body.as_bytes().get(i + 2).is_some_and(u8::is_ascii_alphabetic) {
                        out.push(' ');
                    }
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

/// The amsmath environments whose rows are numbered.
const ALIGN_ENVS: [&str; 8] = ["align", "align*", "alignat", "alignat*", "gather", "gather*", "flalign", "flalign*"];

/// The rows of an alignment body: split at each `\\` outside braces and nested
/// environments; a last row holding only spaces (after a final `\\`) is dropped.
fn split_display_rows(body: &str) -> Vec<&str> {
    let b = body.as_bytes();
    let (mut rows, mut start, mut depth, mut i) = (Vec::new(), 0, 0i32, 0);
    while i < b.len() {
        match b[i] {
            b'%' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'\\' => {
                let rest = &body[i..];
                if rest.starts_with("\\begin{") {
                    depth += 1;
                } else if rest.starts_with("\\end{") {
                    depth -= 1;
                } else if rest.starts_with("\\\\") && depth == 0 {
                    rows.push(&body[start..i]);
                    start = i + 2;
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let last = &body[start..];
    if !last.trim().is_empty() || rows.is_empty() {
        rows.push(last);
    }
    rows
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

/// Whether horizontal-mode output ends in positive glue (`\@bsphack`'s `\lastskip`): a space,
/// an end of line that is not a blank line, or an `\hspace` with positive width.
fn ends_with_hskip(out: &str) -> bool {
    let t = out.trim_end_matches([' ', '\t']);
    if t.len() < out.len() {
        return !t.is_empty() && !t.ends_with('\n');
    }
    if let Some(r) = t.strip_suffix('\n') {
        return !r.trim_end_matches([' ', '\t', '\r']).ends_with('\n') && !r.is_empty();
    }
    // Only the last control sequence can be the `\hspace` (its argument is a plain length).
    t.strip_suffix('}')
        .and_then(|r| r.rfind('\\').and_then(|i| r[i..].strip_prefix("\\hspace{")))
        .and_then(crate::config::parse_glue3_pt)
        .is_some_and(|(w, _, _)| w > 0.0)
}

/// latex.ltx `\@setref` for an undefined label: `\nfss@text{\reset@font\bfseries ??}`, a box.
/// The empty page table of a first pass.
fn no_pages() -> &'static HashMap<String, u32> {
    static EMPTY: std::sync::OnceLock<HashMap<String, u32>> = std::sync::OnceLock::new();
    EMPTY.get_or_init(HashMap::new)
}

const UNDEFINED_REF: &str = "\\mbox{\\normalfont\\bfseries ??}";

const ENUM_LABELS: [&str; 4] = ["labelenumi", "labelenumii", "labelenumiii", "labelenumiv"];
const ENUM_COUNTERS: [&str; 4] = ["enumi", "enumii", "enumiii", "enumiv"];

/// TeX's `\ignorespaces` on source text: spaces and one end of line, not a blank line.
fn ignore_spaces(s: &str) -> &str {
    let t = s.trim_start_matches([' ', '\t']);
    match t.strip_prefix("\r\n").or_else(|| t.strip_prefix('\n')) {
        Some(r) if !r.trim_start_matches([' ', '\t', '\r']).starts_with('\n') => r.trim_start_matches([' ', '\t']),
        _ => t,
    }
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
pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
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

/// TeX's `\uppercase`/`\lowercase` on source text: letters change case, control sequence
/// names do not (they are not character tokens). With `skip_math`, `$...$` is kept, as
/// LaTeX's `\MakeUppercase` does.
fn change_case(text: &str, upper: bool, skip_math: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_math = false;
    while let Some(c) = chars.next() {
        if c == '\\' {
            out.push(c);
            match chars.peek() {
                Some(&n) if n.is_ascii_alphabetic() || n == '@' => {
                    while let Some(&n) = chars.peek().filter(|n| n.is_ascii_alphabetic() || **n == '@') {
                        out.push(n);
                        chars.next();
                    }
                }
                Some(&n) => {
                    out.push(n);
                    chars.next();
                }
                None => {}
            }
            continue;
        }
        if c == '$' && skip_math {
            in_math = !in_math;
        }
        if in_math {
            out.push(c);
        } else if upper {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(src: &str) -> String {
        expand(src).unwrap().into_owned()
    }

    /// The engine state after expanding `src`.
    fn engine_after(src: &str) -> crate::engine::Engine {
        let mut e = Expander::new(src);
        while let Some(b) = e.input.peek() {
            match b {
                b'\\' => e.control().unwrap(),
                _ => e.copy_run(),
            }
        }
        e.engine
    }

    #[test]
    fn lengths_are_registers_with_grouping() {
        let e = engine_after("\\newlength\\a\\setlength{\\a}{2pt}\\begingroup\\setlength{\\a}{5pt}\\addtolength{\\a}{1pt}\\endgroup\\newlength\\b\\setlength{\\b}{.5\\a plus 1fil}");
        assert_eq!(e.dimen_sp("a"), Some(2 * 65536));
        let b = e.eqtb.glue(e.lookup("b").unwrap());
        assert_eq!((b.width, b.stretch, b.stretch_order), (65536, 65536, 1));
    }

    #[test]
    fn catcodes_are_engine_state() {
        use crate::engine::Cat;
        assert_eq!(engine_after("\\makeatletter").eqtb.catcode('@'), Cat::Letter);
        assert_eq!(engine_after("\\begingroup\\makeatletter\\endgroup").eqtb.catcode('@'), Cat::Other);
        assert_eq!(engine_after("\\catcode`\\|=13 x").eqtb.catcode('|'), Cat::Active);
        assert_eq!(engine_after("\\catcode 42 = 11").eqtb.catcode('*'), Cat::Letter);
        // With `*` a letter, `\a*b` is one control word.
        assert_eq!(ex("\\catcode`\\*=11 \\def\\a*b{X}\\a*b"), "X");
    }

    #[test]
    fn let_keeps_a_builtin_meaning() {
        let out = ex("\\let\\oldemph\\emph\\def\\emph#1{[\\oldemph{#1}]}\\emph{x}");
        assert_eq!(out, "[\\emph{x}]");
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
        assert!(out.starts_with("See 2 and \\textup{(1)}."), "{out}");
        assert!(out.ends_with("Back: 2, \\mbox{\\normalfont\\bfseries ??}"), "{out}");
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
        assert!(out.starts_with("[Knu,\\@ 1] [1, p.~3]"), "{out}");
        assert!(out.contains("\\section*{References}\n\\rushtexperiodsf{1}\\sloppy\n\\begin{enumerate}[rushtexbib=9]\\item[{[1]}]"), "{out}");
        assert!(out.ends_with("\\end{enumerate}\\rushtexperiodsf{0}\\fussy"));
    }

    #[test]
    fn theorems_and_proof() {
        let src = "\\newtheorem{thm}{Theorem}[section]\\newtheorem{lem}[thm]{Lemma}\\section{S}\\begin{thm}[Main]\\label{m}True.\\end{thm}\\begin{lem}Also.\\end{lem}\\begin{proof}Easy.\\end{proof}\\ref{m}";
        let out = ex(src);
        assert!(
            out.contains("\\begin{rushtextheorem}\\noindent{\\normalfont\\bfseries Theorem \\textup{1.1} {\\normalfont (Main)}.}\\hspace{5pt plus 1pt minus 1pt}True."),
            "{out}"
        );
        assert!(out.contains("\\normalfont\\bfseries Lemma \\textup{1.2}."), "{out}");
        assert!(
            out.contains("\\begin{rushtexproof}\\noindent{\\itshape Proof.}\\hspace{0.5em}Easy.\\unskip\\nobreak\\hfill\\quad$\\openbox$"),
            "{out}"
        );
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
