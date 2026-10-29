//! Whatsits: tex.web §1340–§1378 (`\openout`, `\write`, `\closeout`, `\special`,
//! `\setlanguage`) and pdfTeX's page-stream whatsits (`\pdfliteral`, `\pdfcolorstack`,
//! `\pdfsetmatrix`, `\pdfsave`, `\pdfrestore`), in our own form.
//!
//! A whatsit is one [`Kind::Whatsit`] node: its `sub` says which, `font` holds the stream,
//! literal mode or colour stack, and `a..b` is a token span (the text, or the file name as
//! character tokens). The deferred ones act when their box is shipped out (§1366
//! `out_what`): files open, close and are written, and `\pdfliteral shipout` text expands.

use super::engine_catcode::Cat;
use super::engine_io::WriteStream;
use super::engine_node::{Kind, Node};
use super::engine_prim::Prim;
use super::engine_run::{Machine, Mode};
use super::engine_token::{Span, Token};

/// The `sub` of a whatsit node.
pub(super) const OPEN: u8 = 0;
pub(super) const WRITE: u8 = 1;
/// The stream of a `\write` the native job keeps for its place in the list only: LaTeX's
/// writes to the `.aux` file, whose text the native job has no use for.
pub(super) const NULL_STREAM: u16 = u16::MAX;
pub(super) const CLOSE: u8 = 2;
pub(super) const SPECIAL: u8 = 3;
/// §1341 `language_node`: `a` the language, `b` its `\lefthyphenmin`·64 + `\righthyphenmin`.
pub(super) const LANGUAGE: u8 = 4;
pub(super) const LITERAL: u8 = 5;
/// `\pdfliteral shipout`: the text expands when the page is shipped out.
pub(super) const LATE_LITERAL: u8 = 6;
pub(super) const COLOR_SET: u8 = 7;
pub(super) const COLOR_PUSH: u8 = 8;
pub(super) const COLOR_POP: u8 = 9;
pub(super) const COLOR_CURRENT: u8 = 10;
pub(super) const SET_MATRIX: u8 = 11;
pub(super) const SAVE: u8 = 12;
pub(super) const RESTORE: u8 = 13;
/// `\pdfrefximage` (14–17 are `engine_pdfdoc`'s): see `engine_image`.
pub(super) const XIMAGE: u8 = 18;
/// `\pdfrefxform`: see `engine_xform`.
pub(super) const XFORM: u8 = 19;
/// pdfTeX's `pdf_save_pos_node`: the walk that ships the page records where it is, and the
/// `\write`s after it read the position with `\pdflastxpos` and `\pdflastypos`.
pub(super) const SAVE_POS: u8 = 20;

/// Whether a whatsit keeps a token list as its span `a..b` (for `engine_gc`).
#[inline]
pub(super) fn holds_tokens(sub: u8) -> bool {
    matches!(sub, OPEN | WRITE | SPECIAL | LITERAL | LATE_LITERAL | COLOR_SET | COLOR_PUSH | SET_MATRIX)
}

/// A whatsit with a size, kept like a rule's (`a` indexes `nodes.rules`): pdfTeX's
/// `pdf_refximage_node` and `pdf_refxform_node`.
#[inline]
pub(super) fn sized(sub: u8) -> bool {
    sub == XIMAGE || sub == XFORM
}

// §236 parameter slots.
const LANGUAGE_CODE: u32 = 50;
const LEFT_HYPHEN_MIN: u32 = 51;
const RIGHT_HYPHEN_MIN: u32 = 52;

/// pdfTeX's literal modes: `set_origin`, `direct_page`, `direct_always`.
const DIRECT_PAGE: u16 = 1;
const DIRECT_ALWAYS: u16 = 2;

impl Machine<'_, '_> {
    /// `Implement \pdfsavepos`.
    pub(super) fn save_pos(&mut self) {
        self.whatsit(SAVE_POS, 0, Span { start: 0, end: 0 });
    }

    /// color.sty's `\set@color` (`\pdfcolorstack0 push{text}`) with `Some(text)`, or its
    /// `\reset@color` (`pop`), as a whatsit on the current list.
    pub(super) fn color_stack(&mut self, push: Option<&str>) {
        let span = match push {
            Some(text) => {
                let mut toks = Vec::with_capacity(text.len());
                super::engine_value::str_toks(text, &mut toks);
                self.e.toks.store(&toks)
            }
            None => Span::default(),
        };
        self.whatsit(if push.is_some() { COLOR_PUSH } else { COLOR_POP }, 0, span);
    }

    fn whatsit(&mut self, sub: u8, font: u16, span: Span) {
        self.cur.push(Node { kind: Kind::Whatsit, sub, font, a: span.start as i32, b: span.end as i32 });
    }

    /// §1348 `do_extension` for the whatsits that go on the current list.
    pub(super) fn whatsit_command(&mut self, p: Prim) {
        match p {
            // §1351
            Prim::OpenOut => {
                let n = self.scan_four_bit();
                self.scan_optional_equals();
                let name = self.scan_file_name();
                let mut toks = Vec::with_capacity(name.len());
                super::engine_value::str_toks(&name, &mut toks);
                let span = self.e.toks.store(&toks);
                self.whatsit(OPEN, n, span);
            }
            // §1352: the stream is kept as 16 or 17 when it is out of range.
            Prim::Write => {
                let n = self.scan_int();
                let n = if n < 0 {
                    17
                } else {
                    n.min(16) as u16
                };
                let span = self.scan_braced_text(false);
                self.whatsit(WRITE, n, span);
            }
            // §1353
            Prim::CloseOut => {
                let n = self.scan_four_bit();
                self.whatsit(CLOSE, n, Span::default());
            }
            // §1354: the text is expanded at once.
            Prim::Special => {
                let span = self.scan_braced_text(true);
                self.whatsit(SPECIAL, 0, span);
            }
            // §1377
            Prim::SetLanguage => {
                let n = self.scan_int();
                if self.mode != Mode::Horizontal {
                    self.error("Improper \\setlanguage");
                    return;
                }
                let lang = if (1..=255).contains(&n) { n } else { 0 };
                self.set_clang(lang);
                let hm = self.hyphen_mins();
                self.cur.push(Node { kind: Kind::Whatsit, sub: LANGUAGE, font: 0, a: lang, b: hm });
            }
            Prim::PdfLiteral => {
                let late = self.scan_keyword("shipout");
                let mode = if self.scan_keyword("direct") {
                    DIRECT_ALWAYS
                } else if self.scan_keyword("page") {
                    DIRECT_PAGE
                } else {
                    0
                };
                let span = self.scan_braced_text(!late);
                self.whatsit(if late { LATE_LITERAL } else { LITERAL }, mode, span);
            }
            Prim::PdfColorStack => {
                let n = self.scan_int();
                let n = if n < 0 {
                    self.error("Invalid negative color stack number");
                    0
                } else {
                    n as u16
                };
                let sub = if self.scan_keyword("set") {
                    COLOR_SET
                } else if self.scan_keyword("push") {
                    COLOR_PUSH
                } else if self.scan_keyword("pop") {
                    COLOR_POP
                } else if self.scan_keyword("current") {
                    COLOR_CURRENT
                } else {
                    self.error("Color stack action is missing");
                    return;
                };
                let span = if sub <= COLOR_PUSH { self.scan_braced_text(true) } else { Span::default() };
                self.whatsit(sub, n, span);
            }
            Prim::PdfSetMatrix => {
                let span = self.scan_braced_text(true);
                self.whatsit(SET_MATRIX, 0, span);
            }
            Prim::PdfSave => self.whatsit(SAVE, 0, Span::default()),
            Prim::PdfRestore => self.whatsit(RESTORE, 0, Span::default()),
            _ => {}
        }
    }

    /// §1091 `set_cur_lang`: `\language`, or 0 outside 1–255.
    pub(super) fn cur_lang(&self) -> i32 {
        let l = self.e.eqtb.word(super::engine_eqtb::Reg { region: super::engine_eqtb::Region::Int, slot: LANGUAGE_CODE });
        if (1..=255).contains(&l) {
            l
        } else {
            0
        }
    }

    pub(super) fn set_clang(&mut self, l: i32) {
        self.clang = l;
    }

    /// §1091 `norm_min` of `\lefthyphenmin` and `\righthyphenmin`, packed as l·64 + r.
    pub(super) fn hyphen_mins(&self) -> i32 {
        let norm = |slot| self.e.eqtb.word(super::engine_eqtb::Reg { region: super::engine_eqtb::Region::Int, slot }).clamp(1, 63);
        norm(LEFT_HYPHEN_MIN) * 64 + norm(RIGHT_HYPHEN_MIN)
    }

    /// §1376 `fix_language`: a language whatsit when `\language` has changed in a paragraph.
    pub(super) fn fix_language(&mut self) {
        let raw = self.e.eqtb.word(super::engine_eqtb::Reg { region: super::engine_eqtb::Region::Int, slot: LANGUAGE_CODE });
        if raw == self.clang {
            return;
        }
        let l = self.cur_lang();
        if l != self.clang {
            self.clang = l;
            let hm = self.hyphen_mins();
            self.cur.push(Node { kind: Kind::Whatsit, sub: LANGUAGE, font: 0, a: l, b: hm });
        }
    }

    /// §435 `scan_four_bit_int`.
    fn scan_four_bit(&mut self) -> u16 {
        let n = self.scan_int();
        u16::try_from(n).ok().filter(|&n| n < 16).unwrap_or_else(|| {
            self.error(format!("Bad number ({n})"));
            0
        })
    }

    /// §1370 `write_out`: the text of a `\write` expands now, with `mode` 0, and goes to
    /// its stream, or to the log when the stream is not open.
    pub(super) fn write_out(&mut self, n: u16, span: Span) {
        let mut list = Vec::with_capacity(span.len() as usize + 2);
        list.push(Token::char(Cat::BeginGroup, '{'));
        list.extend_from_slice(self.e.toks.get(span));
        list.push(Token::char(Cat::EndGroup, '}'));
        self.back_list(&list);
        let old = std::mem::replace(&mut self.mode_zero, true);
        let span = self.scan_braced_text(true);
        self.mode_zero = old;
        let mut text = self.print_list(span);
        text.push('\n');
        match self.writes.get_mut(n as usize).and_then(Option::as_mut) {
            Some(w) => w.text.push_str(&text),
            None => self.log.push_str(&text),
        }
    }

    /// §1373 `out_what` for the box being shipped out, depth first (§1366, §1367); leader
    /// boxes are not visited, as `doing_leaders` suppresses them in TeX.
    ///
    /// `late` does the `\pdfliteral shipout`s, before the page is walked; otherwise the
    /// files are written, after it, each `\pdfsavepos` taking the next of `positions` (in
    /// the walk's order, which is this one's) for `\pdflastxpos` and `\pdflastypos`.
    pub(super) fn out_whats(&mut self, b: u32, late: bool, positions: &mut std::slice::Iter<(i32, i32)>) {
        let (start, end) = self.e.nodes.boxes[b as usize].list;
        for i in start..end {
            let n = self.e.nodes.list[i as usize];
            match (n.kind, n.sub) {
                (Kind::Box, _) => self.out_whats(n.a as u32, late, positions),
                (Kind::Whatsit, SAVE_POS) if !late => {
                    if let Some(&p) = positions.next() {
                        self.pdf_doc.last_pos = p;
                    }
                }
                (Kind::Whatsit, OPEN | CLOSE | WRITE) if late => {}
                (Kind::Whatsit, LATE_LITERAL) if !late => {}
                (Kind::Whatsit, OPEN | CLOSE) => {
                    let k = n.font as usize;
                    if let Some(w) = self.writes[k].take() {
                        self.closed.push(w);
                    }
                    if n.sub == OPEN {
                        let span = Span { start: n.a as u32, end: n.b as u32 };
                        let name = self.print_list(span);
                        self.writes[k] = Some(WriteStream { name, text: String::new() });
                    }
                }
                (Kind::Whatsit, WRITE) if n.font == NULL_STREAM => {
                    if n.a != 0 {
                        self.shipped_entry(n.a);
                    }
                }
                (Kind::Whatsit, WRITE) => self.write_out(n.font, Span { start: n.a as u32, end: n.b as u32 }),
                (Kind::Whatsit, LATE_LITERAL) => {
                    let list = self.e.toks.get(Span { start: n.a as u32, end: n.b as u32 }).to_vec();
                    let mut braced = Vec::with_capacity(list.len() + 2);
                    braced.push(Token::char(Cat::BeginGroup, '{'));
                    braced.extend_from_slice(&list);
                    braced.push(Token::char(Cat::EndGroup, '}'));
                    self.back_list(&braced);
                    let span = self.scan_braced_text(true);
                    self.e.nodes.list[i as usize] = Node { sub: LITERAL, a: span.start as i32, b: span.end as i32, ..n };
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::engine_prim::prim;
    use super::*;

    #[test]
    fn slots_match_the_table() {
        use super::super::engine_eqtb::{Reg, Region};
        use super::super::engine_names::param;
        for (name, slot) in [("language", LANGUAGE_CODE), ("lefthyphenmin", LEFT_HYPHEN_MIN), ("righthyphenmin", RIGHT_HYPHEN_MIN)] {
            assert_eq!(param(name), Some(Reg { region: Region::Int, slot }), "{name}");
        }
    }

    #[test]
    fn whatsit_primitives_are_engine_primitives() {
        for (name, p) in [("special", Prim::Special), ("setlanguage", Prim::SetLanguage), ("pdfliteral", Prim::PdfLiteral), ("pdfcolorstack", Prim::PdfColorStack), ("pdfsetmatrix", Prim::PdfSetMatrix), ("pdfsave", Prim::PdfSave), ("pdfrestore", Prim::PdfRestore)] {
            assert_eq!(prim(name), Some(p), "{name}");
        }
    }
}
