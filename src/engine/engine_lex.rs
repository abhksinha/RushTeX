//! The tokenizer: tex.web §343–§357 `get_next` over a source file, in our own form.
//!
//! As in TeX, input is read a line at a time into a buffer (§31 `input_ln`: trailing spaces
//! dropped, `\endlinechar` appended when it is 0–255), and a state (§303 `mid_line`,
//! `skip_blanks`, `new_line`) decides what spaces and line ends become. Catcodes are read at
//! the moment each character is scanned, so a `\catcode` change takes effect from the next
//! character on, as it does in TeX. The buffer is reused for every line.

use std::borrow::Cow;

use super::engine_catcode::{Cat, CatTable};
use super::engine_token::{CsTable, Token};

/// §303: the three states of a line being read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    MidLine,
    SkipBlanks,
    NewLine,
}

/// The tokenizer over one source text.
#[derive(PartialEq, Clone, Debug)]
pub struct Lexer<'a> {
    /// The document is borrowed; a file read by `\input` is owned.
    src: Cow<'a, str>,
    /// Byte offset of the next unread line in `src`.
    next_line: usize,
    buf: Vec<char>,
    loc: usize,
    state: State,
    /// The `\par` id, interned on first use.
    par: Option<u32>,
    /// Characters of catcode 15 met so far (§346 "Text line contains an invalid character").
    pub invalid: u32,
    /// §362 `\endinput`: the file ends after the current line.
    pub end_after_line: bool,
    /// Lines read so far (§304 `line`).
    pub line: u32,
    /// e-TeX's `eof_seen`: `\everyeof` has been inserted at this file's end.
    pub eof_seen: bool,
}

impl<'a> Lexer<'a> {
    pub fn new(src: impl Into<Cow<'a, str>>) -> Self {
        Lexer { src: src.into(), next_line: 0, buf: Vec::with_capacity(128), loc: 0, state: State::NewLine, par: None, invalid: 0, end_after_line: false, line: 0, eof_seen: false }
    }

    /// This lexer at its place over `src` instead of its own text: a document resumed after its
    /// preamble (`engine_resume`) reads on from the same byte in the new text, whose bytes up to
    /// there are the old ones.
    pub fn over<'b>(&self, src: impl Into<Cow<'b, str>>) -> Lexer<'b> {
        Lexer { src: src.into(), next_line: self.next_line, buf: self.buf.clone(), loc: self.loc, state: self.state, par: self.par, invalid: self.invalid, end_after_line: self.end_after_line, line: self.line, eof_seen: self.eof_seen }
    }

    /// Byte offset of the next unread line: everything before it has been read.
    pub fn read_to(&self) -> usize {
        self.next_line
    }

    /// The text, when borrowed (the document's).
    pub fn text(&self) -> &'a str {
        match self.src {
            Cow::Borrowed(s) => s,
            Cow::Owned(_) => "",
        }
    }

    /// Moves the read offset by `d`.
    pub fn shift(&mut self, d: isize, lines: i32) {
        self.next_line = self.next_line.wrapping_add_signed(d);
        self.line = self.line.wrapping_add_signed(lines);
    }

    /// §362: moves to the next line of the file; `false` at its end.
    fn next_line(&mut self, end_line_char: i32) -> bool {
        if self.next_line >= self.src.len() || self.end_after_line {
            return false;
        }
        self.line += 1;
        let rest = &self.src[self.next_line..];
        let (line, used) = match rest.find('\n') {
            Some(i) => (&rest[..i], i + 1),
            None => (rest, rest.len()),
        };
        self.next_line += used;
        let line = line.strip_suffix('\r').unwrap_or(line);
        self.buf.clear();
        self.buf.extend(line.trim_end_matches(' ').chars());
        if (0..=255).contains(&end_line_char) {
            self.buf.push(char::from(end_line_char as u8));
        }
        self.loc = 0;
        self.state = State::NewLine;
        true
    }

    /// The text up to `end`, read past the tokenizer (`\@xverbatim`): the rest of the current
    /// line, then whole lines, each ending in `\n` (trailing spaces removed, as §362 reads
    /// lines). The lexer resumes after `end`; `None` at the file's end.
    /// The rest of the current line, unread.
    pub fn raw_peek(&self) -> String {
        self.buf[self.loc.min(self.buf.len())..].iter().collect()
    }

    pub fn raw_until(&mut self, end: &str, end_line_char: i32) -> Option<String> {
        let mut text = String::new();
        loop {
            let mut line: String = self.buf[self.loc.min(self.buf.len())..].iter().collect();
            if let Some(k) = line.find(end) {
                text.push_str(&line[..k]);
                self.loc += line[..k].chars().count() + end.chars().count();
                self.state = State::MidLine;
                return Some(text);
            }
            if (0..=255).contains(&end_line_char) && line.ends_with(char::from(end_line_char as u8)) {
                line.pop();
            }
            text.push_str(&line);
            text.push('\n');
            if !self.next_line(end_line_char) {
                return None;
            }
        }
    }

    /// §352: `c` (of catcode 7) has just been read and `j` is the index after it. If `c`
    /// starts an expanded code, the character it stands for and how many more buffer
    /// characters it uses: `c` again and two lowercase hex digits, or `c` again and one
    /// character below 128 (shifted by 64).
    fn reduce(&self, c: char, j: usize) -> Option<(char, usize)> {
        let b = &self.buf;
        if b.get(j) != Some(&c) {
            return None;
        }
        let x = *b.get(j + 1)?;
        if (x as u32) >= 128 {
            return None;
        }
        let is_hex = |c: char| c.is_ascii_digit() || ('a'..='f').contains(&c);
        if is_hex(x) {
            if let Some(&y) = b.get(j + 2).filter(|&&y| is_hex(y)) {
                let v = x.to_digit(16)? * 16 + y.to_digit(16)?;
                return Some((char::from(v as u8), 3));
            }
        }
        let v = x as u32;
        Some((char::from_u32(if v < 64 { v + 64 } else { v - 64 })?, 2))
    }

    /// §355: replaces a `^^` code inside a control-sequence name by its character.
    fn reduce_in_buffer(&mut self, k: usize, cats: &CatTable) -> bool {
        if cats.get(self.buf[k]) != Cat::Superscript {
            return false;
        }
        match self.reduce(self.buf[k], k + 1) {
            Some((c, n)) => {
                self.buf[k] = c;
                self.buf.drain(k + 1..k + 1 + n);
                true
            }
            None => false,
        }
    }

    /// §354 "Scan a control sequence": `loc` is just past the escape character.
    fn scan_cs(&mut self, cats: &CatTable, names: &mut CsTable) -> Token {
        let limit = self.buf.len();
        if self.loc >= limit {
            // §354 `null_cs`: an escape at the very end of a line with no `\endlinechar`.
            return Token::cs(names.intern(""));
        }
        loop {
            let start = self.loc;
            let cat = cats.get(self.buf[start]);
            self.state = if matches!(cat, Cat::Letter | Cat::Space) { State::SkipBlanks } else { State::MidLine };
            if cat == Cat::Letter {
                // §356: scan ahead to the first nonletter, reducing `^^` codes as they come.
                let mut k = start + 1;
                loop {
                    while k < self.buf.len() && cats.get(self.buf[k]) == Cat::Letter {
                        k += 1;
                    }
                    if k < self.buf.len() && self.reduce_in_buffer(k, cats) {
                        continue;
                    }
                    break;
                }
                if k > start + 1 {
                    let name: String = self.buf[start..k].iter().collect();
                    self.loc = k;
                    return Token::cs(names.intern(&name));
                }
            } else if self.reduce_in_buffer(start, cats) {
                // §355: an expanded code where the name starts; scan again.
                continue;
            }
            let mut one = [0u8; 4];
            let name = self.buf[start].encode_utf8(&mut one);
            self.loc = start + 1;
            return Token::cs(names.intern(name));
        }
    }

    /// GDL's word table: where the run of characters from `loc` ends, if what follows it
    /// ends a word in the main loop (§1038) as a token that is not a character and expands
    /// to nothing. The run is the characters of catcode letter or other below 256; after
    /// it must come a space, the line's end, a brace, a math shift, an alignment tab, a
    /// parameter or a subscript character. Anything else (an escape, an active, comment,
    /// ignored or superscript character, a line without `\endlinechar`) could continue the
    /// word or change its tokens, and gives `None`.
    pub fn word_run(&self, cats: &CatTable) -> Option<usize> {
        let mut k = self.loc;
        while let Some(&c) = self.buf.get(k) {
            if (c as u32) < 256 && matches!(cats.get(c), Cat::Letter | Cat::Other) {
                k += 1;
            } else {
                break;
            }
        }
        let &t = self.buf.get(k)?;
        // With no characters before it, a space or the line's end is skipped unless the
        // state is `mid_line` (after a control word, §354), and the word goes on past it.
        if k == self.loc && self.state != State::MidLine {
            return None;
        }
        matches!(cats.get(t), Cat::Space | Cat::EndLine | Cat::BeginGroup | Cat::EndGroup | Cat::MathShift | Cat::AlignTab | Cat::Param | Cat::Subscript).then_some(k)
    }

    /// Past a word of the table: the space after it and the first character of the next
    /// word, read as [`Lexer::next`] reads them (§347–§348: a space token in state
    /// `mid_line`, from a space or the line's end; the blanks after it skipped, past the
    /// next line's start if the line ended; then a character, `mid_line` again).
    /// `None`, with nothing read, unless a space token comes next; `Some(None)` when what
    /// follows the space is not a letter or other character below 256, which is left unread.
    pub fn space_then_char(&mut self, cats: &CatTable, end_line_char: i32) -> Option<Option<(u8, Cat)>> {
        if self.state != State::MidLine {
            return None;
        }
        match cats.get(*self.buf.get(self.loc)?) {
            Cat::Space => {
                self.loc += 1;
                self.state = State::SkipBlanks;
            }
            Cat::EndLine => {
                self.loc = self.buf.len();
                if !self.next_line(end_line_char) {
                    return Some(None);
                }
            }
            _ => return None,
        }
        while self.buf.get(self.loc).is_some_and(|&c| cats.get(c) == Cat::Space) {
            self.loc += 1;
        }
        let Some(&c) = self.buf.get(self.loc) else { return Some(None) };
        let cat = cats.get(c);
        if (c as u32) >= 256 || !matches!(cat, Cat::Letter | Cat::Other) {
            return Some(None);
        }
        self.loc += 1;
        self.state = State::MidLine;
        Some(Some((c as u8, cat)))
    }

    /// The characters from `loc` to `end` (a [`Lexer::word_run`]), as bytes.
    pub fn run_bytes(&self, end: usize) -> impl Iterator<Item = u8> + '_ {
        self.buf[self.loc..end].iter().map(|&c| c as u8)
    }

    /// Past `n` characters of a run, as if each had been read as a token (§347: state
    /// `mid_line` after one).
    pub fn skip_run(&mut self, n: usize) {
        if n > 0 {
            self.loc += n;
            self.state = State::MidLine;
        }
    }

    /// §341 `get_next` for file input: the next token, or `None` at the end of the source.
    /// `end_line_char` is `\endlinechar`'s current value.
    pub fn next(&mut self, cats: &CatTable, names: &mut CsTable, end_line_char: i32) -> Option<Token> {
        loop {
            if self.loc >= self.buf.len() {
                if !self.next_line(end_line_char) {
                    return None;
                }
                continue;
            }
            let mut c = self.buf[self.loc];
            self.loc += 1;
            let mut cat = cats.get(c);
            // §344 `reswitch`: a reduced `^^` code is read again as the new character.
            while cat == Cat::Superscript {
                match self.reduce(c, self.loc) {
                    Some((r, n)) => {
                        self.loc += n;
                        c = r;
                        cat = cats.get(c);
                    }
                    None => break,
                }
            }
            let state = self.state;
            match cat {
                Cat::Ignored => continue,
                Cat::Space if state != State::MidLine => continue,
                Cat::Escape => return Some(self.scan_cs(cats, names)),
                Cat::Active => {
                    self.state = State::MidLine;
                    return Some(Token::char(Cat::Active, c));
                }
                Cat::Invalid => {
                    self.invalid += 1;
                    continue;
                }
                Cat::Space => {
                    // §348: every space token is the character ` `.
                    self.state = State::SkipBlanks;
                    return Some(Token::char(Cat::Space, ' '));
                }
                Cat::EndLine => {
                    self.loc = self.buf.len();
                    match state {
                        State::MidLine => return Some(Token::char(Cat::Space, ' ')),
                        State::SkipBlanks => continue,
                        State::NewLine => {
                            let par = *self.par.get_or_insert_with(|| names.intern("par"));
                            return Some(Token::cs(par));
                        }
                    }
                }
                Cat::Comment => {
                    self.loc = self.buf.len();
                    continue;
                }
                _ => {
                    self.state = State::MidLine;
                    return Some(Token::char(cat, c));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::engine_token::show;
    use super::*;

    fn lex(src: &str, cats: &CatTable) -> String {
        let mut names = CsTable::default();
        let mut lx = Lexer::new(src);
        let mut toks = Vec::new();
        while let Some(t) = lx.next(cats, &mut names, 13) {
            toks.push(t);
        }
        let mut out = String::new();
        show(&toks, &names, &super::super::engine_token::ShowCtx { cats, escape: 92, file: None }, &mut out);
        out
    }

    #[test]
    fn spaces_and_line_ends() {
        let t = CatTable::latex();
        // A control word swallows the spaces after it; a line end is one space; a blank
        // line is `\par`; a comment eats the line end.
        assert_eq!(lex("\\foo   bar  baz\nx% c\ny\n\nz", &t), "\\foo bar baz xy \\par z ");
        // Leading spaces on a line are skipped (state new_line).
        assert_eq!(lex("a\n   b", &t), "a b ");
    }

    #[test]
    fn control_symbols_and_space() {
        let t = CatTable::latex();
        assert_eq!(lex("\\@x\\ y", &t), "\\@x\\ y ");
    }

    #[test]
    fn catcode_changes_name_scanning() {
        let mut t = CatTable::latex();
        assert_eq!(lex("\\a@b", &t), "\\a @b ");
        t.0[b'@' as usize] = Cat::Letter as u8;
        assert_eq!(lex("\\a@b", &t), "\\a@b ");
    }

    #[test]
    fn hat_hat_codes() {
        let t = CatTable::latex();
        // `^^41` is `A`, `^^M` is a carriage return (end of line), and `^^` inside a name.
        assert_eq!(lex("^^41\\fo^^6f", &t), "A\\foo ");
        assert_eq!(lex("a^^Mignored\nb", &t), "a b ");
    }
}
