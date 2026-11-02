//! `\MakeUppercase{text}` on the GDL path for plain text: latex.ltx's `\MakeUppercase   `
//! (`{{\let\oe\OE \@@text@case@aux{uppercase}{#1}{#2}}}`) runs `\text_uppercase:nn` on the
//! text inside a group. For ASCII letters and the Latin-1 letters of UTF-8 input that is the
//! one-to-one map of the Unicode data l3text reads (`a`-`z` to `A`-`Z`, U+00E0-U+00FE to
//! U+00C0-U+00DE but `÷`), and the result is typeset as if typed. Anything else in the text
//! (a command, math, `~`, `ß` which becomes `SS`, `ÿ` whose capital is outside Latin-1, a
//! locale argument) goes to the engine.

use super::super::engine_catcode::Cat;
use super::super::engine_run::Machine;
use super::super::engine_token::Token;

/// Whether the pre-scan may take `\MakeUppercase{text}` with this text (each byte of UTF-8 a
/// `char` below 256, as the pre-scan reads the file): the characters
/// [`Machine::make_uppercase`] maps or keeps.
pub(super) fn upper_ok(text: &str) -> bool {
    let mut lead = false;
    text.chars().map(|c| c as u32).all(|c| {
        if std::mem::take(&mut lead) {
            return (0x80..=0xBE).contains(&c) && !matches!(c, 0x97 | 0x9F | 0xB7);
        }
        lead = c == 0xC3;
        lead || c < 128 && (char::from(c as u8).is_ascii_alphanumeric() || b" \t\r\n.,;:!?'`()[]-/*+=<>|@{}".contains(&(c as u8)))
    }) && !lead
}

/// [`upper_ok`] for `\MakeLowercase`: every Latin-1 letter after the lead byte 0xC3 maps or
/// stays.
pub(super) fn lower_ok(text: &str) -> bool {
    let mut lead = false;
    text.chars().map(|c| c as u32).all(|c| {
        if std::mem::take(&mut lead) {
            return (0x80..=0xBF).contains(&c);
        }
        lead = c == 0xC3;
        lead || c < 128 && (char::from(c as u8).is_ascii_alphanumeric() || b" \t\r\n.,;:!?'`()[]-/*+=<>|@{}".contains(&(c as u8)))
    }) && !lead
}

impl Machine<'_, '_> {
    /// `\MakeUppercase{text}`: `{` `{` the text upper-cased `}` `}` back in the input, the two
    /// groups latex.ltx opens. Each UTF-8 byte is a token here: a lowercase Latin-1 letter is
    /// the lead byte 0xC3 and a byte 0xA0-0xBE, its capital that byte less 0x20.
    pub(super) fn make_uppercase(&mut self) {
        self.make_case(true);
    }

    /// `\MakeLowercase{text}` (`\@@text@case@aux{lowercase}`): as [`Self::make_uppercase`],
    /// a capital Latin-1 letter (0xC3, then 0x80-0x9E but `×`) its byte plus 0x20; `ß` and
    /// `ÿ` are lowercase already.
    pub(super) fn make_lowercase(&mut self) {
        self.make_case(false);
    }

    fn make_case(&mut self, upper: bool) {
        if self.next_is('[') {
            return self.not_native("\\MakeUppercase with a locale");
        }
        let mut t = Vec::new();
        self.read_arg(&mut t);
        let mut out = Vec::with_capacity(t.len() + 4);
        out.extend([Token::char(Cat::BeginGroup, '{'), Token::char(Cat::BeginGroup, '{')]);
        let mut lead = false;
        for &x in &t {
            let Some(cat) = x.cat() else { return self.not_native("\\MakeUppercase of a command") };
            let c = x.chr() as u32;
            let y = match cat {
                _ if lead => {
                    lead = false;
                    match c {
                        0x80..=0x9E if c != 0x97 && upper => x,
                        0x80..=0x9E if c != 0x97 => Token::char(cat, char::from_u32(c + 0x20).unwrap_or('?')),
                        0x97 | 0x9F | 0xA0..=0xBF if !upper => x,
                        0xA0..=0xBE if c != 0xB7 => Token::char(cat, char::from_u32(c - 0x20).unwrap_or('?')),
                        _ => return self.not_native("\\MakeUppercase of a character it maps otherwise"),
                    }
                }
                _ if c == 0xC3 => {
                    lead = true;
                    x
                }
                Cat::Letter | Cat::Other if c < 128 => Token::char(cat, if upper { x.chr().to_ascii_uppercase() } else { x.chr().to_ascii_lowercase() }),
                Cat::Space | Cat::BeginGroup | Cat::EndGroup => x,
                _ => return self.not_native("\\MakeUppercase of a character it maps otherwise"),
            };
            out.push(y);
        }
        if lead {
            return self.not_native("\\MakeUppercase of a broken character");
        }
        out.extend([Token::char(Cat::EndGroup, '}'), Token::char(Cat::EndGroup, '}')]);
        self.back_list(&out);
    }

    /// `\uppercase{text}` (tex.web §1285 `shift_case`): the text, read unexpanded to its
    /// matching `}`, back in the input without its braces, each character token's code
    /// replaced by its nonzero `\uccode`, its category kept; commands are left as they are.
    /// latex.ltx's `\uccode`s in ASCII: `a`-`z` to `A`-`Z`, `^^Y` to `I` and `^^Z` to `J`
    /// (the rest zero). A byte above 127 (latex.ltx's Latin-1 codes, UTF-8 input) and a
    /// left brace that is not an explicit `{` go to the engine.
    pub(super) fn uppercase(&mut self) {
        match self.next_unblank() {
            Some(t) if t.cat() == Some(Cat::BeginGroup) => self.input.back(t),
            _ => return self.not_native("\\uppercase without an explicit {"),
        }
        let mut t = Vec::new();
        self.read_arg(&mut t);
        for x in &mut t {
            let Some(cat) = x.cat() else { continue };
            *x = match x.chr() {
                c @ 'a'..='z' => Token::char(cat, c.to_ascii_uppercase()),
                '\u{19}' => Token::char(cat, 'I'),
                '\u{1a}' => Token::char(cat, 'J'),
                c if c as u32 > 127 => return self.not_native("\\uppercase of a character above 127"),
                _ => continue,
            };
        }
        self.back_list(&t);
    }
}
