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

codec_enum!(Arena { Defs, Scratch, File });

/// A token-list frame (§307 `token_list` state).
#[derive(PartialEq, Clone, Copy, Debug)]
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

codec_struct!(Frame { arena, pos, end, params, scratch_mark, u_template });

/// The input stack.
#[derive(PartialEq, Clone, Debug)]
pub struct Input<'a> {
    /// The document, then each file `\input` opened, innermost last (§304 `in_open`).
    files: Vec<Lexer<'a>>,
    /// For each of `files`, the name its text was read under when it can be read again
    /// that way (`engine_resume`: a copy taken inside it reads it again on resume); `None`
    /// for the document, `\scantokens` text and a file this run wrote.
    origins: Vec<Option<String>>,
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

#[cfg(feature = "oracle")]
impl super::engine_snapshot::Codec for Input<'static> {
    fn put(&self, w: &mut super::engine_snapshot::Writer) {
        let Input { files, origins, frames, scratch, params, align_state, #[cfg(feature = "mprof")] doc_files: _ } = self;
        files.put(w);
        origins.put(w);
        frames.put(w);
        scratch.put(w);
        params.put(w);
        align_state.put(w);
    }
    fn take(r: &mut super::engine_snapshot::Reader) -> Option<Self> {
        use super::engine_snapshot::Codec;
        Some(Input { files: Codec::take(r)?, origins: Codec::take(r)?, frames: Codec::take(r)?, scratch: Codec::take(r)?, params: Codec::take(r)?, align_state: Codec::take(r)?, #[cfg(feature = "mprof")] doc_files: Vec::new() })
    }
}

const NONE: u32 = u32::MAX;

impl<'a> Input<'a> {
    pub fn new(src: &'a str) -> Self {
        Input { files: vec![Lexer::new(src)], origins: vec![None], frames: Vec::new(), scratch: Vec::new(), params: Vec::new(), align_state: super::engine_align::ALIGN_IDLE, #[cfg(feature = "mprof")] doc_files: Vec::new() }
    }

    /// The input stack as owned data, its texts left out (`engine_resume`), when the
    /// document is the only file open or, with `nested`, every file `\input` opened can be
    /// read again by its name ([`Input::origins`]); with [`Input::over`] (and
    /// [`Input::refill`] for each file opened) it reads on over new texts.
    pub fn detach(&self, nested: bool) -> Option<Input<'static>> {
        if self.files.len() > 1 && !(nested && self.origins[1..].iter().all(Option::is_some)) {
            return None;
        }
        Some(Input { files: self.files.iter().map(|l| l.over("")).collect(), origins: self.origins.clone(), frames: self.frames.clone(), scratch: self.scratch.clone(), params: self.params.clone(), align_state: self.align_state, #[cfg(feature = "mprof")] doc_files: self.doc_files.clone() })
    }

    /// A detached input stack reading on over `src`, the document's text.
    pub fn over<'b>(&self, src: &'b str) -> Input<'b> {
        Input { files: self.files.iter().map(|l| l.over(src)).collect(), origins: self.origins.clone(), frames: self.frames.clone(), scratch: self.scratch.clone(), params: self.params.clone(), align_state: self.align_state, #[cfg(feature = "mprof")] doc_files: self.doc_files.clone() }
    }

    /// The files `\input` opened, outermost first, by the name each was read under.
    pub fn opened(&self) -> impl Iterator<Item = Option<&str>> {
        self.origins[1..].iter().map(Option::as_deref)
    }

    /// Each file `\input` opened, outermost first: its name and the text read so far (to its
    /// next unread line); `None` for one that cannot be read again by name.
    pub fn opened_read(&self) -> Vec<Option<(String, &str)>> {
        self.files[1..].iter().zip(&self.origins[1..]).map(|(l, o)| o.as_ref().map(|o| (o.clone(), l.read_text()))).collect()
    }

    /// File level `level` (1 for the first file `\input` opened) reading on over `text`
    /// from where it was.
    pub fn refill(&mut self, level: usize, text: String) {
        if let Some(l) = self.files.get_mut(level) {
            *l = l.over(text);
        }
    }

    /// Byte offset in the document of its next unread line.
    pub fn base_read_to(&self) -> usize {
        self.files.first().map_or(0, |l| l.read_to())
    }

    /// Whether a file is being read, no token list open above it: the document alone, or
    /// with `nested` a file `\input` opened (every one open can be read again by name). The
    /// lists below a file (the rest of the macro that called `\input`, as expl3's
    /// `\file_input:n` leaves its pop code there) are read only once the file ends.
    pub fn at_file_top(&mut self, nested: bool) -> bool {
        self.trim();
        if self.files.len() == 1 {
            return self.frames.is_empty();
        }
        nested && self.frames.last().is_some_and(|f| f.arena == Arena::File) && self.origins[1..].iter().all(Option::is_some)
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

    /// §537 `start_input`: `text` is read next, as a file, until its end or `\endinput`;
    /// `origin` is the name it can be read again under ([`Input::origins`]).
    pub fn push_file(&mut self, text: String, origin: Option<String>) {
        self.files.push(Lexer::new(text));
        self.origins.push(origin);
        let i = self.files.len() as u32 - 1;
        self.frames.push(Frame { arena: Arena::File, pos: i, end: i + 1, params: NONE, scratch_mark: NONE, u_template: false });
    }

    /// The document's text.
    pub fn base_text(&self) -> &'a str {
        self.files.first().map_or("", |l| l.text())
    }

    /// Moves the document's read offset by `d` (the text before it grew by `d` bytes).
    pub fn shift(&mut self, d: isize, lines: i32) {
        if let Some(l) = self.files.first_mut() {
            l.shift(d, lines);
        }
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
    pub fn next(&mut self, defs: &Tokens, eqtb: &Eqtb, names: &mut CsTable) -> Option<Token> {
        let t = self.next_token(defs, eqtb, names)?;
        if t.cs_id().is_none() {
            match t.cat() {
                Some(Cat::BeginGroup) => self.align_state += 1,
                Some(Cat::EndGroup) => self.align_state -= 1,
                _ => {}
            }
        }
        Some(t)
    }

    /// The common case inline: the top frame a token list with a token left that is not a
    /// macro body's `#n`; the rest in [`Input::next_token_slow`], which does the same first.
    #[inline(always)]
    fn next_token(&mut self, defs: &Tokens, eqtb: &Eqtb, names: &mut CsTable) -> Option<Token> {
        if let Some(f) = self.frames.last_mut() {
            if f.arena != Arena::File && f.pos < f.end {
                let t = match f.arena {
                    Arena::Defs => defs.at(f.pos),
                    _ => self.scratch[f.pos as usize],
                };
                if f.params == NONE || t.cs_id().is_some() || t.cat() != Some(Cat::EndLine) {
                    f.pos += 1;
                    return Some(t);
                }
            }
        }
        self.next_token_slow(defs, eqtb, names)
    }

    /// The rest of §343 `get_next`: files (the lexer reads `\endlinechar` now), ended
    /// lists, and `#n` in a macro body.
    #[inline(never)]
    fn next_token_slow(&mut self, defs: &Tokens, eqtb: &Eqtb, names: &mut CsTable) -> Option<Token> {
        // Read only when a file is: most calls end a token list or read a `#n`.
        let end_line_char = || eqtb.word(Reg { region: Region::Int, slot: super::engine_names::END_LINE_CHAR });
        loop {
            let Some(f) = self.frames.last_mut() else {
                // The document's own text: `\everyjob` is over (`DateWatch`).
                eqtb.watch.arm();
                return self.files[0].next(eqtb.cats(), names, end_line_char());
            };
            if f.arena == Arena::File {
                // §362: at a file's end, its level ends and the input below resumes.
                let i = f.pos as usize;
                match self.files[i].next(eqtb.cats(), names, end_line_char()) {
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
                        self.origins.truncate(i);
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

    /// The tokens a scanner would read one by one from the top token list and push unchanged
    /// (§399 a braced argument, §392 a delimited one, §473 a general text or a macro body,
    /// unexpanded or expanded), moved to `out` in one copy: from the frame's place up to, not
    /// including, the first token it treats otherwise, which `next` then reads. That is a
    /// token `keep` refuses (it sees no brace and no `#n`), a macro body's `#n` (§357 reads
    /// its argument), a token marked by `\noexpand`, the list's end, and with `depth` the
    /// right brace that ends the group (`depth` is its brace depth, kept up to date), without
    /// it any brace. Braces move `align_state` as `next` moves them; nothing is read when it
    /// could reach zero (§342: then each token is checked for an alignment entry's end).
    /// Returns the number of tokens moved.
    #[inline(always)]
    pub fn take_while(&mut self, defs: &Tokens, out: &mut Vec<Token>, depth: Option<&mut u32>, mut keep: impl FnMut(Token) -> bool) -> usize {
        use super::engine_token::{CHAR_BITS, CS_FLAG, DONT_EXPAND};
        const BEGIN: u32 = Cat::BeginGroup as u32;
        const END: u32 = Cat::EndGroup as u32;
        const OUT_PARAM: u32 = Cat::EndLine as u32;
        let Some(f) = self.frames.last_mut() else { return 0 };
        // Inside the group `align_state` stays above its value now minus `depth`.
        let floor = depth.as_ref().map_or(1, |d| i64::from(**d));
        if f.arena == Arena::File || f.pos >= f.end || i64::from(self.align_state) < floor {
            return 0;
        }
        let list = match f.arena {
            Arena::Defs => defs.get(Span { start: f.pos, end: f.end }),
            _ => &self.scratch[f.pos as usize..f.end as usize],
        };
        let body = f.params != NONE;
        let start = depth.as_ref().map_or(0, |d| **d);
        let mut d = start;
        let mut n = 0;
        for &Token(t) in list {
            if t & (CS_FLAG | DONT_EXPAND) != 0 {
                if t & DONT_EXPAND != 0 || !keep(Token(t)) {
                    break;
                }
            } else {
                match t >> CHAR_BITS {
                    BEGIN if d > 0 => d += 1,
                    END if d > 1 => d -= 1,
                    BEGIN | END => break,
                    OUT_PARAM if body => break,
                    _ if !keep(Token(t)) => break,
                    _ => {}
                }
            }
            n += 1;
        }
        if n == 0 {
            return 0;
        }
        // A short run is pushed in place: the copy call costs more than the copy.
        if n <= 8 {
            out.reserve(8);
            for &t in &list[..n] {
                out.push(t);
            }
        } else {
            out.extend_from_slice(&list[..n]);
        }
        f.pos += n as u32;
        if let Some(depth) = depth {
            self.align_state += d as i32 - start as i32;
            *depth = d;
        }
        n
    }

    /// The tokens a skipping scanner would read one by one from the top token list and pass
    /// over (§494 `pass_text`), passed over in one step: from the frame's place up to, not
    /// including, the first token for which `skip` is false, a macro body's `#n` (§357 reads
    /// its argument) or the list's end. Braces move `align_state` as `next` moves them; a
    /// token after which it would be zero is left to `next` (§342 checks it for an
    /// alignment entry's end).
    #[inline]
    pub fn skip_while(&mut self, defs: &Tokens, mut skip: impl FnMut(Token) -> bool) {
        use super::engine_token::{CHAR_BITS, CS_FLAG, DONT_EXPAND};
        const BEGIN: u32 = Cat::BeginGroup as u32;
        const END: u32 = Cat::EndGroup as u32;
        const OUT_PARAM: u32 = Cat::EndLine as u32;
        let Some(f) = self.frames.last_mut() else { return };
        if f.arena == Arena::File || f.pos >= f.end {
            return;
        }
        let list = match f.arena {
            Arena::Defs => defs.get(Span { start: f.pos, end: f.end }),
            _ => &self.scratch[f.pos as usize..f.end as usize],
        };
        let body = f.params != NONE;
        let mut a = self.align_state;
        let mut n = 0;
        for &Token(t) in list {
            let t = Token(t & !DONT_EXPAND);
            let b = if t.0 & CS_FLAG != 0 {
                a
            } else {
                match t.0 >> CHAR_BITS {
                    BEGIN => a + 1,
                    END => a - 1,
                    OUT_PARAM if body => break,
                    _ => a,
                }
            };
            if b == 0 || !skip(t) {
                break;
            }
            a = b;
            n += 1;
        }
        f.pos += n as u32;
        self.align_state = a;
    }

    /// The file read next, when no token list is pending, not even an exhausted one.
    pub fn file_next(&mut self) -> Option<&mut Lexer<'a>> {
        match self.frames.last() {
            None => self.files.first_mut(),
            Some(f) if f.arena == Arena::File => self.files.get_mut(f.pos as usize),
            Some(_) => None,
        }
    }

    /// The rest of the file line being read, unread; `None` when a token list is pending.
    pub fn raw_peek(&mut self) -> Option<String> {
        self.trim();
        let i = match self.frames.last() {
            None => 0,
            Some(f) if f.arena == Arena::File => f.pos as usize,
            Some(_) => return None,
        };
        Some(self.files[i].raw_peek())
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
