//! Tokens (tex.web §289) and control-sequence names (§256–§262), in our own representation.
//!
//! A token is one `u32`:
//! - a character token is `cat << 21 | char` (21 bits hold any Unicode scalar);
//! - a control sequence is `CS_FLAG | id`, where `id` names it in a [`CsTable`];
//! - inside a macro, `#n` in the body is `EndLine << 21 | n` (§289 `out_param`), a parameter
//!   in the parameter text is `Ignored << 21 | char` (§289 `match`), and the end of the
//!   parameter text is `Comment << 21` (§289 `end_match`). The tokenizer never makes
//!   tokens of those three catcodes, so the codes are free for these uses.
//!
//! Active characters stay character tokens of catcode 13; their meaning is looked up by
//! character. A one-character name like `\a` and `\csname a\endcsname` intern to the same id,
//! as §374 makes them the same `single_base` entry.
//!
//! Token lists live in one [`Tokens`] arena and are addressed by [`Span`]: no list owns a
//! heap allocation.

use super::engine_catcode::Cat;

pub const CS_FLAG: u32 = 1 << 31;
/// §358 `dont_expand`: a token `\noexpand` put back. It lives in the input only; reading it
/// clears the flag (see `Machine::get_token`).
pub const DONT_EXPAND: u32 = 1 << 30;
const CHAR_BITS: u32 = 21;
const CHAR_MASK: u32 = (1 << CHAR_BITS) - 1;

/// One token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Token(pub u32);

impl Token {
    #[inline]
    pub const fn char(cat: Cat, c: char) -> Token {
        Token((cat as u32) << CHAR_BITS | c as u32)
    }

    #[inline]
    pub const fn cs(id: u32) -> Token {
        Token(CS_FLAG | id)
    }

    /// §289 `out_param`: `#n` in a macro body.
    #[inline]
    pub const fn out_param(n: u8) -> Token {
        Token((Cat::EndLine as u32) << CHAR_BITS | n as u32)
    }

    /// §289 `match`: a parameter in the parameter text, with its parameter character.
    #[inline]
    pub const fn match_param(c: char) -> Token {
        Token((Cat::Ignored as u32) << CHAR_BITS | c as u32)
    }

    /// §289 `end_match`.
    pub const END_MATCH: Token = Token((Cat::Comment as u32) << CHAR_BITS);

    /// This token marked by `\noexpand` (§367).
    #[inline]
    pub const fn dont_expand(self) -> Token {
        Token(self.0 | DONT_EXPAND)
    }

    /// The control sequence's id, if this is one.
    #[inline]
    pub const fn cs_id(self) -> Option<u32> {
        if self.0 & CS_FLAG != 0 {
            Some(self.0 & !CS_FLAG)
        } else {
            None
        }
    }

    /// The catcode of a character token (or of a macro-only token's code); `None` for a
    /// control sequence.
    #[inline]
    pub fn cat(self) -> Option<Cat> {
        if self.0 & CS_FLAG != 0 {
            None
        } else {
            Cat::from_u8((self.0 >> CHAR_BITS) as u8)
        }
    }

    /// The character of a character token; `#n`'s `n` for an `out_param`.
    #[inline]
    pub fn chr(self) -> char {
        char::from_u32(self.0 & CHAR_MASK).unwrap_or('\0')
    }
}

/// Control-sequence names, interned once: the name bytes in one string, a span per id, and
/// an open-addressing table of ids keyed by the name's hash (§259 `id_lookup`, with our
/// hash and no fixed `hash_size`).
#[derive(Debug, Default)]
pub struct CsTable {
    text: String,
    spans: Vec<(u32, u32)>,
    /// Power-of-two table of `id + 1`; 0 is empty.
    slots: Vec<u32>,
}

/// FNV-1a over the name's bytes.
#[inline]
fn hash(name: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in name.as_bytes() {
        h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
    }
    h
}

impl CsTable {
    /// The names and their spans; the hash table is rebuilt on load.
    pub(super) fn dump(&self, w: &mut super::engine_snapshot::Writer) {
        w.str(&self.text);
        w.count(self.spans.len());
        for &(a, b) in &self.spans {
            w.u32(a);
            w.u32(b);
        }
    }

    pub(super) fn undump(r: &mut super::engine_snapshot::Reader) -> Option<CsTable> {
        let text = r.str()?;
        let n = r.count()?;
        let mut spans = Vec::with_capacity(n);
        for _ in 0..n {
            let (a, b) = (r.u32()?, r.u32()?);
            if a > b || !text.is_char_boundary(a as usize) || !text.is_char_boundary(b as usize) || b as usize > text.len() {
                return None;
            }
            spans.push((a, b));
        }
        let mut t = CsTable { text, spans, slots: Vec::new() };
        t.slots = vec![0; (t.spans.len() * 2).next_power_of_two().max(64)];
        for id in 0..t.spans.len() as u32 {
            let i = t.probe(t.name(id));
            t.slots[i] = id + 1;
        }
        Some(t)
    }

    pub fn len(&self) -> usize {
        self.spans.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// The name of control sequence `id`.
    pub fn name(&self, id: u32) -> &str {
        let (a, b) = self.spans[id as usize];
        &self.text[a as usize..b as usize]
    }

    /// The slot `name` occupies, or the empty slot where it would go.
    fn probe(&self, name: &str) -> usize {
        let mask = self.slots.len() - 1;
        let mut i = hash(name) as usize & mask;
        loop {
            match self.slots[i] {
                0 => return i,
                v if self.name(v - 1) == name => return i,
                _ => i = (i + 1) & mask,
            }
        }
    }

    /// The id of `name`, if it was interned.
    pub fn get(&self, name: &str) -> Option<u32> {
        if self.slots.is_empty() {
            return None;
        }
        match self.slots[self.probe(name)] {
            0 => None,
            v => Some(v - 1),
        }
    }

    /// The id of `name`, interning it on first sight.
    pub fn intern(&mut self, name: &str) -> u32 {
        if let Some(id) = self.get(name) {
            return id;
        }
        // Keep the load at or below one half.
        if (self.spans.len() + 1) * 2 > self.slots.len() {
            self.grow();
        }
        let id = self.spans.len() as u32;
        let a = self.text.len() as u32;
        self.text.push_str(name);
        self.spans.push((a, self.text.len() as u32));
        let i = self.probe(name);
        self.slots[i] = id + 1;
        id
    }

    fn grow(&mut self) {
        let n = (self.slots.len() * 2).max(64);
        self.slots = vec![0; n];
        for id in 0..self.spans.len() as u32 {
            let i = self.probe(self.name(id));
            self.slots[i] = id + 1;
        }
    }
}

/// A token list in a [`Tokens`] arena.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub const fn len(self) -> u32 {
        self.end - self.start
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// Every stored token list, back to back.
#[derive(Debug, Default)]
pub struct Tokens {
    toks: Vec<Token>,
}

impl Tokens {
    pub(super) fn dump(&self, w: &mut super::engine_snapshot::Writer) {
        w.all(&self.toks);
    }

    pub(super) fn undump(r: &mut super::engine_snapshot::Reader) -> Option<Tokens> {
        Some(Tokens { toks: r.all()? })
    }

    /// Stores `list` and returns its span.
    pub fn store(&mut self, list: &[Token]) -> Span {
        let start = self.toks.len() as u32;
        self.toks.extend_from_slice(list);
        Span { start, end: self.toks.len() as u32 }
    }

    /// The token at index `i`.
    #[inline]
    pub fn at(&self, i: u32) -> Token {
        self.toks[i as usize]
    }

    #[inline]
    pub fn get(&self, span: Span) -> &[Token] {
        &self.toks[span.start as usize..span.end as usize]
    }

    pub fn len(&self) -> usize {
        self.toks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.toks.is_empty()
    }
}

/// How names print (§262 `print_cs`): the escape character (`\escapechar`; none outside
/// 0–255) and the catcodes that decide whether a one-letter name is followed by a space.
#[derive(Clone, Copy)]
pub struct ShowCtx<'c> {
    pub cats: &'c super::engine_catcode::CatTable,
    pub escape: i32,
    /// `Some(\newlinechar)` when printing to a file or the terminal (§59, `selector<pseudo`):
    /// a character token prints in `^^` form when it is not printable (§49), and the
    /// new-line character ends the line. `None` for a string (`\meaning`, `\detokenize`),
    /// where characters print as they are.
    pub file: Option<i32>,
}

/// §49 with pdflatex's `cp227.tcx` (the format records it): codes 9–11, 32–126 and 128–255
/// print as themselves.
pub fn printable(c: u32) -> bool {
    matches!(c, 9..=11 | 32..=126 | 128..=255)
}

impl ShowCtx<'_> {
    /// §63 `print_esc`: the escape character, then `name`.
    pub fn esc(&self, name: &str, out: &mut String) {
        if let Ok(c) = u8::try_from(self.escape) {
            self.chr(char::from(c), out);
        }
        // §59: a one-character name prints as a character, a longer one as a string.
        let mut chars = name.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => self.chr(c, out),
            _ => out.push_str(name),
        }
    }

    /// §59 `print` of one character code.
    pub fn chr(&self, c: char, out: &mut String) {
        let Some(nl) = self.file else { return out.push(c) };
        let k = c as u32;
        if k as i32 == nl {
            out.push('\n');
        } else if printable(k) || k > 255 {
            out.push(c);
        } else {
            // §49: `^^` and the character 64 away, for codes below 64 and 127.
            out.push_str("^^");
            out.push(char::from_u32(if k < 64 { k + 64 } else { k - 64 }).unwrap_or('?'));
        }
    }

    /// §262 `print_cs`: a multi-letter name is followed by a space, a one-character name only
    /// when that character is a letter now, the empty name is `\csname\endcsname `.
    pub fn cs(&self, name: &str, out: &mut String) {
        let mut chars = name.chars();
        match (chars.next(), chars.next()) {
            (None, _) => {
                self.esc("csname", out);
                self.esc("endcsname", out);
                out.push(' ');
            }
            (Some(c), None) => {
                self.esc(name, out);
                if self.cats.get(c) == Cat::Letter {
                    out.push(' ');
                }
            }
            _ => {
                self.esc(name, out);
                out.push(' ');
            }
        }
    }
}

/// §292 `show_token_list`: a list printed back as text. Parameters print with the macro's
/// parameter character (§294 `match_chr`, `#` by default) and their number; a macro
/// parameter character in the text is doubled; `end_match` is `->`.
pub fn show(list: &[Token], names: &CsTable, ctx: &ShowCtx, out: &mut String) {
    let mut match_chr = '#';
    let mut n = 0u8;
    for &t in list {
        if let Some(id) = t.cs_id() {
            ctx.cs(names.name(id), out);
            continue;
        }
        match t.cat() {
            Some(Cat::EndLine) => {
                out.push(match_chr);
                out.push(char::from_digit(t.chr() as u32, 10).unwrap_or('!'));
            }
            Some(Cat::Ignored) => {
                match_chr = t.chr();
                n += 1;
                out.push(match_chr);
                out.push(char::from(b'0' + n.min(9)));
            }
            Some(Cat::Comment) => out.push_str("->"),
            Some(Cat::Param) => {
                ctx.chr(t.chr(), out);
                ctx.chr(t.chr(), out);
            }
            _ => ctx.chr(t.chr(), out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_fields() {
        let a = Token::char(Cat::Letter, 'a');
        assert_eq!(a.cat(), Some(Cat::Letter));
        assert_eq!(a.chr(), 'a');
        assert_eq!(a.cs_id(), None);
        let u = Token::char(Cat::Other, '\u{10FFFF}');
        assert_eq!(u.chr(), '\u{10FFFF}');
        assert_eq!(u.cat(), Some(Cat::Other));
        assert_eq!(Token::cs(7).cs_id(), Some(7));
        assert_eq!(Token::cs(7).cat(), None);
        assert_eq!(Token::out_param(3).chr() as u32, 3);
        assert_ne!(Token::char(Cat::Letter, 'a'), Token::char(Cat::Other, 'a'));
    }

    #[test]
    fn names_intern_once() {
        let mut t = CsTable::default();
        let a = t.intern("relax");
        let b = t.intern("par");
        assert_eq!(t.intern("relax"), a);
        assert_ne!(a, b);
        assert_eq!(t.name(b), "par");
        assert_eq!(t.get("nosuch"), None);
        for i in 0..1000 {
            t.intern(&format!("n{i}"));
        }
        assert_eq!(t.get("n999").map(|id| t.name(id)), Some("n999"));
        assert_eq!(t.get("relax"), Some(a));
        assert_eq!(t.intern(""), t.intern(""));
    }

    #[test]
    fn lists_share_one_arena() {
        let mut names = CsTable::default();
        let mut store = Tokens::default();
        let relax = names.intern("relax");
        let at = names.intern("@");
        let s = store.store(&[Token::cs(relax), Token::char(Cat::Letter, 'x'), Token::cs(at), Token::out_param(1)]);
        let mut out = String::new();
        let cats = super::super::engine_catcode::CatTable::latex();
        show(store.get(s), &names, &ShowCtx { cats: &cats, escape: 92, file: None }, &mut out);
        assert_eq!(out, "\\relax x\\@#1");
        assert_eq!(s.len(), 4);
    }
}
