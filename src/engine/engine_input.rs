//! The input stack: tex.web §300–§325, in our own form.
//!
//! The bottom is the source file, read by the [`Lexer`]. Above it are token-list frames: a
//! macro body (whose `#n` tokens read that call's arguments), an argument being read, or
//! tokens put back (§325 `back_input`). Lists live in one of two arenas: the engine's
//! permanent [`Tokens`] (macro texts) or this stack's scratch array (arguments and backed-up
//! tokens), which is freed in stack order as frames end. The parameter stack (§308) is a
//! list of spans into the scratch array.

use super::engine_catcode::Cat;
use super::engine_eqtb::{Eqtb, Reg, Region};
use super::engine_lex::Lexer;
use super::engine_token::{CsTable, Span, Token, Tokens};

/// e-TeX's `\everyeof` token parameter.
const EVERY_EOF: u32 = 13;

/// Which arena a frame reads: a token list, or (`File`) the file `pos` in `files`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arena {
    Defs,
    Scratch,
    File,
}

/// A token-list frame (§307 `token_list` state).
#[derive(Clone, Copy, Debug)]
struct Frame {
    arena: Arena,
    pos: u32,
    end: u32,
    /// For a macro body, the base of its parameters in the parameter stack (§390
    /// `param_start`); `u32::MAX` otherwise.
    params: u32,
    /// The scratch length to restore when the frame ends, or `u32::MAX` to keep it.
    scratch_mark: u32,
    /// §307 `u_template`: an alignment entry's ⟨u_j⟩, whose end starts the entry proper.
    u_template: bool,
}

/// The input stack.
#[derive(Debug)]
pub struct Input<'a> {
    /// The document, then each file `\input` opened, innermost last (§304 `in_open`).
    files: Vec<Lexer<'a>>,
    frames: Vec<Frame>,
    scratch: Vec<Token>,
    params: Vec<Span>,
    /// §309 `align_state`: the brace balance since the current alignment entry began
    /// (§789); an alignment character or `\cr` at zero ends the entry.
    pub align_state: i32,
    /// `engine_mprof`: by file level, whether it is the document's own text.
    #[cfg(feature = "mprof")]
    doc_files: Vec<bool>,
}

const NONE: u32 = u32::MAX;

impl<'a> Input<'a> {
    pub fn new(src: &'a str) -> Self {
        Input { files: vec![Lexer::new(src)], frames: Vec::new(), scratch: Vec::new(), params: Vec::new(), align_state: super::engine_align::ALIGN_IDLE, #[cfg(feature = "mprof")] doc_files: Vec::new() }
    }

    /// §324 `end_token_list`: drops the top frame and what it alone used.
    fn end_list(&mut self) {
        if let Some(f) = self.frames.pop() {
            // §324: the entry proper begins at the end of its ⟨u_j⟩.
            if f.u_template {
                self.align_state = 0;
            }
            if f.params != NONE {
                self.params.truncate(f.params as usize);
            }
            if f.scratch_mark != NONE {
                self.scratch.truncate(f.scratch_mark as usize);
            }
        }
    }

    /// §390: drops the depleted lists on top, so a macro that ends by calling itself needs
    /// no more stack.
    pub fn trim(&mut self) {
        while self.frames.last().is_some_and(|f| f.arena != Arena::File && f.pos >= f.end) {
            self.end_list();
        }
    }

    /// §537 `start_input`: `text` is read next, as a file, until its end or `\endinput`.
    pub fn push_file(&mut self, text: String) {
        self.files.push(Lexer::new(text));
        let i = self.files.len() as u32 - 1;
        self.frames.push(Frame { arena: Arena::File, pos: i, end: i + 1, params: NONE, scratch_mark: NONE, u_template: false });
    }

    /// §1131 `\endinput`: the innermost file ends after its current line.
    pub fn end_input(&mut self) {
        if let Some(l) = self.files.last_mut() {
            l.end_after_line = true;
        }
    }

    /// The number of files open, the document included.
    pub fn file_depth(&self) -> usize {
        self.files.len()
    }

    /// The current line of the innermost file.
    pub fn line(&self) -> u32 {
        self.files.last().map_or(0, |l| l.line)
    }

    /// §343 `get_next`: the next token, or `None` at the end of the source. `#n` in a macro
    /// body reads its argument (§357 `out_param`). Explicit braces move `align_state`
    /// (§347, §357).
    #[inline]
    pub fn next(&mut self, defs: &Tokens, eqtb: &Eqtb, names: &mut CsTable, end_line_char: i32) -> Option<Token> {
        let t = self.next_token(defs, eqtb, names, end_line_char)?;
        if t.cs_id().is_none() {
            match t.cat() {
                Some(Cat::BeginGroup) => self.align_state += 1,
                Some(Cat::EndGroup) => self.align_state -= 1,
                _ => {}
            }
        }
        Some(t)
    }

    fn next_token(&mut self, defs: &Tokens, eqtb: &Eqtb, names: &mut CsTable, end_line_char: i32) -> Option<Token> {
        let cats = eqtb.cats();
        loop {
            let Some(f) = self.frames.last_mut() else {
                return self.files[0].next(cats, names, end_line_char);
            };
            if f.arena == Arena::File {
                // §362: at a file's end, its level ends and the input below resumes.
                let i = f.pos as usize;
                match self.files[i].next(cats, names, end_line_char) {
                    Some(t) => return Some(t),
                    // e-TeX §362: `\everyeof` is read once before the file's level ends.
                    None if i > 0 && !self.files[i].eof_seen => {
                        self.files[i].eof_seen = true;
                        let every = eqtb.toks(Reg { region: Region::Toks, slot: EVERY_EOF });
                        if every.start < every.end {
                            self.frames.push(Frame { arena: Arena::Defs, pos: every.start, end: every.end, params: NONE, scratch_mark: NONE, u_template: false });
                        }
                        continue;
                    }
                    None => {
                        self.frames.pop();
                        self.files.truncate(i);
                        continue;
                    }
                }
            }
            if f.pos >= f.end {
                self.end_list();
                continue;
            }
            let t = match f.arena {
                Arena::Defs => defs.at(f.pos),
                _ => self.scratch[f.pos as usize],
            };
            f.pos += 1;
            if f.params != NONE && t.cs_id().is_none() && t.cat() == Some(Cat::EndLine) {
                let arg = self.params[(f.params + t.chr() as u32 - 1) as usize];
                self.frames.push(Frame { arena: Arena::Scratch, pos: arg.start, end: arg.end, params: NONE, scratch_mark: NONE, u_template: false });
                continue;
            }
            return Some(t);
        }
    }

    /// The file read next, when no token list is pending, not even an exhausted one.
    pub fn file_next(&mut self) -> Option<&mut Lexer<'a>> {
        match self.frames.last() {
            None => self.files.first_mut(),
            Some(f) if f.arena == Arena::File => self.files.get_mut(f.pos as usize),
            Some(_) => None,
        }
    }

    /// [`Lexer::raw_until`] on the file being read, when no token list is pending.
    pub fn raw_until(&mut self, end: &str, end_line_char: i32) -> Option<String> {
        self.trim();
        let i = match self.frames.last() {
            None => 0,
            Some(f) if f.arena == Arena::File => f.pos as usize,
            Some(_) => return None,
        };
        self.files[i].raw_until(end, end_line_char)
    }

    /// §325 `back_input`: `t` is read again next; a brace is uncounted until then.
    pub fn back(&mut self, t: Token) {
        self.trim();
        if t.cs_id().is_none() {
            match t.cat() {
                Some(Cat::BeginGroup) => self.align_state -= 1,
                Some(Cat::EndGroup) => self.align_state += 1,
                _ => {}
            }
        }
        let start = self.scratch.len() as u32;
        self.scratch.push(t);
        self.frames.push(Frame { arena: Arena::Scratch, pos: start, end: start + 1, params: NONE, scratch_mark: start, u_template: false });
    }

    /// §323 `back_list` of tokens already in the scratch array from `start` to its end: they
    /// are read next, in order.
    pub fn back_scratch(&mut self, start: u32) {
        let end = self.scratch.len() as u32;
        self.frames.push(Frame { arena: Arena::Scratch, pos: start, end, params: NONE, scratch_mark: start, u_template: false });
    }

    /// The scratch array's and the parameter stack's capacities, for `RT_MEM`.
    pub fn capacities(&self) -> (usize, usize, usize) {
        (self.scratch.capacity(), self.params.capacity(), self.frames.capacity())
    }

    /// The scratch array, for collecting arguments and backed-up lists.
    pub fn scratch(&mut self) -> &mut Vec<Token> {
        &mut self.scratch
    }

    /// §390: feeds a macro body whose arguments are the last `n` spans pushed with
    /// [`Input::push_param`]; they and their tokens are freed when the body ends.
    pub fn begin_macro(&mut self, body: Span, n: u32, scratch_mark: u32) {
        let base = self.params.len() as u32 - n;
        self.frames.push(Frame { arena: Arena::Defs, pos: body.start, end: body.end, params: base, scratch_mark, u_template: false });
    }

    /// §788, §789: an alignment template from the engine's token arena; the end of a
    /// ⟨u_j⟩ (`u`) starts the entry proper. An empty ⟨u_j⟩ does so at once.
    pub fn push_template(&mut self, span: Span, u: bool) {
        if u && span.is_empty() {
            self.align_state = 0;
            return;
        }
        self.frames.push(Frame { arena: Arena::Defs, pos: span.start, end: span.end, params: NONE, scratch_mark: NONE, u_template: u });
    }

    /// Each token-arena list being read, from its current position (for `engine_gc`,
    /// which moves it).
    pub(super) fn for_each_list(&mut self, mut f: impl FnMut(&mut Span)) {
        for fr in self.frames.iter_mut().filter(|fr| fr.arena == Arena::Defs) {
            let mut s = Span { start: fr.pos, end: fr.end };
            f(&mut s);
            (fr.pos, fr.end) = (s.start, s.end);
        }
    }

    pub fn push_param(&mut self, span: Span) {
        self.params.push(span);
    }

    /// Marks the file just pushed as the document's own text or not (`engine_mprof`).
    #[cfg(feature = "mprof")]
    pub fn mark_document(&mut self, doc: bool) {
        self.doc_files.resize(self.files.len(), true);
        if let Some(d) = self.doc_files.last_mut() {
            *d = doc;
        }
    }

    /// The innermost frame, if a token list from the arena: its index and end
    /// (`engine_mprof`).
    #[cfg(feature = "mprof")]
    pub fn top_frame(&self) -> Option<(usize, u32)> {
        let f = self.frames.last()?;
        (f.arena == Arena::Defs).then_some((self.frames.len() - 1, f.end))
    }

    /// Whether the innermost level is a file (`engine_mprof`).
    #[cfg(feature = "mprof")]
    pub fn reading_file_any(&self) -> bool {
        self.frames.last().is_none_or(|f| f.arena == Arena::File)
    }

    /// Whether the token just read came from the document's own text (`engine_mprof`).
    #[cfg(feature = "mprof")]
    pub fn reading_document(&self) -> bool {
        match self.frames.last() {
            None => true,
            Some(f) if f.arena == Arena::File => self.doc_files.get(f.pos as usize).copied().unwrap_or(true),
            _ => false,
        }
    }

    /// The depth of the token-list stack, for runaway guards.
    pub fn depth(&self) -> usize {
        self.frames.len()
    }
}
