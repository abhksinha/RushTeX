//! Conditionals: tex.web §487–§510 (`pass_text`, `conditional`, `\else`/`\fi`/`\or`) and
//! e-TeX's `\unless`, `\ifdefined`, `\ifcsname`, in our own form.
//!
//! The condition stack is one byte per open conditional: its limit (§489 `if_limit`), the
//! code past which an `\else`, `\or` or `\fi` is extra. Text is skipped by meaning, without
//! expansion, so `\newif`'s `\iffoo` (a `\let` of `\iffalse`) nests like any `\if`.

use super::engine_catcode::Cat;
use super::engine_prim::{Meaning, Prim, ELSE_CODE, FI_CODE, IF_CODE, OR_CODE};
use super::engine_run::{Machine, Mode, X};
use super::engine_token::Token;

impl Machine<'_, '_> {
    /// §494 `pass_text`: skips to the `\else`, `\or` or `\fi` of the current level and
    /// returns its code (`FI_CODE` at the end of the input, after an error).
    fn pass_text(&mut self) -> u8 {
        let mut level = 0u32;
        loop {
            let Some(t) = self.get_token() else {
                self.error("Incomplete \\if; all text was ignored after line");
                return FI_CODE;
            };
            match self.meaning(t) {
                Meaning::Prim(p) => {
                    if let Some(code) = p.fi_or_else() {
                        if level == 0 {
                            return code;
                        }
                        if code == FI_CODE {
                            level -= 1;
                        }
                    } else if p.is_if() {
                        level += 1;
                    }
                }
                _ => continue,
            }
        }
    }

    /// §498 `conditional` for `if_test` `p`, negated after `\unless`.
    pub(super) fn conditional(&mut self, p: Prim, unless: bool) {
        // §495: push the condition stack; this conditional is entry `me`.
        self.conds.push(IF_CODE);
        let me = self.conds.len() - 1;
        let b = match p {
            Prim::IfCase => return self.if_case(me),
            Prim::If | Prim::IfCat => self.test_chars(p == Prim::IfCat),
            Prim::IfNum | Prim::IfDim => self.test_relation(p == Prim::IfDim, false),
            // pdfTeX's `\ifpdfabsnum` and `\ifpdfabsdim`: the relation of absolute values.
            Prim::IfPdfAbsNum | Prim::IfPdfAbsDim => self.test_relation(p == Prim::IfPdfAbsDim, true),
            Prim::IfOdd => self.scan_int() % 2 != 0,
            Prim::IfVmode => self.mode == Mode::Vertical,
            Prim::IfHmode => self.mode == Mode::Horizontal,
            Prim::IfMmode => self.mode == Mode::Math,
            Prim::IfInner => self.inner,
            // §505: boxes are the native pipeline's; to the engine every box is void.
            Prim::IfVoid | Prim::IfHbox | Prim::IfVbox => {
                let n = self.scan_int();
                let b = u32::try_from(n).ok().and_then(|n| self.e.eqtb.box_reg(n));
                match (p, b) {
                    (Prim::IfVoid, b) => b.is_none(),
                    (_, None) => false,
                    (p, Some(b)) => self.e.nodes.boxes[b as usize].vertical == (p == Prim::IfVbox),
                }
            }
            Prim::IfEof => self.if_eof(),
            Prim::IfX => self.test_ifx(),
            Prim::IfTrue => true,
            // e-TeX: whether the font has the character.
            Prim::IfFontChar => {
                let f = self.scan_font_ident() as usize;
                let c = self.scan_int();
                let font = &self.e.fonts.list[f];
                u8::try_from(c).ok().and_then(|c| font.dims(c, &self.e.fonts.info)).is_some()
            }
            Prim::IfInCsname => self.in_csname > 0,
            Prim::IfFalse => false,
            Prim::IfDefined => match self.get_token() {
                Some(t) => self.meaning(t) != Meaning::Undefined,
                None => false,
            },
            Prim::IfCsname => self.test_csname(),
            _ => false,
        };
        if b != unless {
            // §498: wait for `\else` or `\fi`.
            self.conds[me] = ELSE_CODE;
            return;
        }
        // §500: skip to this conditional's `\else` or `\fi`.
        let code = loop {
            let code = self.pass_text();
            if self.conds.len() - 1 == me {
                if code != OR_CODE {
                    break code;
                }
                self.error("Extra \\or");
            } else if code == FI_CODE {
                self.conds.pop();
            }
        };
        self.common_ending(me, code);
    }

    /// §498 `common_ending`: at `\fi` the conditional ends; after `\else` its limit is `\fi`.
    fn common_ending(&mut self, me: usize, code: u8) {
        if code == FI_CODE {
            self.conds.pop();
        } else {
            self.conds[me] = FI_CODE;
        }
    }

    /// §509: `\ifcase n` skips `n` `\or`s.
    fn if_case(&mut self, me: usize) {
        let mut n = self.scan_int();
        while n != 0 {
            let code = self.pass_text();
            if self.conds.len() - 1 == me {
                if code == OR_CODE {
                    n -= 1;
                } else {
                    return self.common_ending(me, code);
                }
            } else if code == FI_CODE {
                self.conds.pop();
            }
        }
        self.conds[me] = OR_CODE;
    }

    /// §510: an `\else`, `\or` or `\fi` met while expanding (token `t`, code `code`).
    pub(super) fn fi_or_else(&mut self, t: Token, code: u8) {
        let limit = self.conds.last().copied().unwrap_or(0);
        if code > limit {
            if limit == IF_CODE {
                // §510 `insert_relax`: the condition is still being read.
                self.input.back(t);
                let relax = self.frozen_relax();
                self.input.back(relax);
            } else {
                self.error(format!("Extra {}", self.name_of(t)));
            }
            return;
        }
        let mut code = code;
        while code != FI_CODE {
            code = self.pass_text();
        }
        self.conds.pop();
    }

    /// §1503 (e-TeX) `\unless`: the next token must be a test other than `\ifcase`.
    pub(super) fn unless(&mut self) {
        let Some(t) = self.get_token() else { return };
        match self.meaning(t) {
            Meaning::Prim(p) if p.is_if() && p != Prim::IfCase => self.conditional(p, true),
            _ => {
                self.error("You can't use `\\unless' before this");
                self.input.back(t);
            }
        }
    }

    /// §506 `\if` and `\ifcat`: two tokens after expansion, compared by character code or
    /// catcode. A control sequence that is not `\let` to a character is (relax, 256).
    fn test_chars(&mut self, by_cat: bool) -> bool {
        let a = self.get_x_token(false).map(char_pair);
        let b = self.get_x_token(false).map(char_pair);
        match (a, b) {
            (Some(a), Some(b)) if by_cat => a.0 == b.0,
            (Some(a), Some(b)) => a.1 == b.1,
            _ => false,
        }
    }

    /// §503 `\ifnum` and `\ifdim`: two quantities and a relation `<`, `=` or `>`.
    fn test_relation(&mut self, dim: bool, abs: bool) -> bool {
        let n = if dim { self.scan_dimen(false).0 } else { self.scan_int() };
        let n = if abs { n.wrapping_abs() } else { n };
        let r = match self.next_non_blank() {
            Some(x) if x.t.cs_id().is_none() && x.t.cat() == Some(Cat::Other) && matches!(x.t.chr(), '<' | '=' | '>') => x.t.chr(),
            other => {
                self.error(format!("Missing = inserted for {}", if dim { "\\ifdim" } else { "\\ifnum" }));
                if let Some(x) = other {
                    self.input.back(x.t);
                }
                '='
            }
        };
        let v = if dim { self.scan_dimen(false).0 } else { self.scan_int() };
        let v = if abs { v.wrapping_abs() } else { v };
        match r {
            '<' => n < v,
            '>' => n > v,
            _ => n == v,
        }
    }

    /// §507 `\ifx`: two tokens without expansion, equal when they mean the same: the same
    /// primitive or quantity, the same character, or macros with the same prefixes and text.
    fn test_ifx(&mut self) -> bool {
        let Some(a) = self.get_token() else { return false };
        let ma = self.meaning_read(a);
        let Some(b) = self.get_token() else { return false };
        let mb = self.meaning_read(b);
        match (ma, mb) {
            (Meaning::Macro(x), Meaning::Macro(y)) => x.flags == y.flags && (x.text == y.text || self.e.toks.get(x.text) == self.e.toks.get(y.text)),
            _ => ma == mb,
        }
    }

    /// e-TeX `\ifcsname ... \endcsname`: whether the name is defined, without defining it.
    fn test_csname(&mut self) -> bool {
        let name = self.scan_cs_name();
        let defined = match self.e.cs.get(&name) {
            Some(id) => self.meaning(Token::cs(id)) != Meaning::Undefined,
            None => false,
        };
        self.name_buf = name;
        defined
    }
}

/// §506: (command, character) of a token for `\if` and `\ifcat`: a character token's
/// catcode and code; a control sequence `\let` to a character, that character's; anything
/// else (relax, 256). Active characters after `\noexpand` stay catcode 13.
fn char_pair(x: X) -> (u8, u32) {
    const RELAX: (u8, u32) = (16, 256);
    // §506 `get_x_token_or_active_char`: a `\noexpand`ed active character is itself;
    // otherwise a name or active character stands for the character it is `\let` to.
    let t = match x.m {
        Meaning::DontExpand if x.t.cs_id().is_none() => x.t,
        Meaning::Char(c) => c,
        _ => return RELAX,
    };
    match t.cat() {
        Some(c) if (t.chr() as u32) < 256 => (c as u8, t.chr() as u32),
        _ => RELAX,
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Engine, Machine};

    fn run(src: &str) -> (String, Vec<String>, Engine) {
        let mut e = Engine::default();
        let mut m = Machine::new(&mut e, src);
        m.run();
        let (out, errors) = (m.out_text(), m.errors.clone());
        drop(m);
        (out, errors, e)
    }

    fn ok(src: &str) -> String {
        let (out, errors, _) = run(src);
        assert!(errors.is_empty(), "{errors:?}");
        out
    }

    #[test]
    fn true_false_and_nesting() {
        assert_eq!(ok("\\iftrue a\\else b\\fi"), "a");
        assert_eq!(ok("\\iffalse a\\else b\\fi"), "b");
        // A skipped branch skips whole nested conditionals, `\else` included.
        assert_eq!(ok("\\iffalse\\iftrue x\\else y\\fi\\else z\\fi"), "z");
        assert_eq!(ok("\\iftrue\\iffalse x\\else y\\fi\\else z\\fi"), "y");
        assert_eq!(ok("\\unless\\iffalse u\\fi"), "u");
    }

    #[test]
    fn newif_is_a_let() {
        let src = "\\let\\iffoo\\iffalse \\def\\footrue{\\let\\iffoo\\iftrue}\\iffalse\\iffoo\\fi a\\fi\\footrue\\iffoo b\\fi";
        assert_eq!(ok(src), "b");
    }

    #[test]
    fn numbers_and_dimensions() {
        assert_eq!(ok("\\ifnum 3<12 y\\else n\\fi"), "y");
        assert_eq!(ok("\\ifnum`a=97 y\\fi"), "y");
        assert_eq!(ok("\\ifnum\"FF='377 y\\fi"), "y");
        assert_eq!(ok("\\ifdim 1in=72.26999pt y\\fi\\ifdim 1in=72.27pt n\\fi"), "y");
        assert_eq!(ok("\\ifdim 1pt<1.00001pt y\\else n\\fi"), "y");
        assert_eq!(ok("\\ifodd 7 y\\fi\\ifodd -2 n\\fi"), "y");
        // §510: a `\fi` while the condition is read ends the number (a `\relax` goes in).
        assert_eq!(ok("\\ifnum1=1\\fi x"), "x ");
    }

    #[test]
    fn ifcase() {
        assert_eq!(ok("\\ifcase 2 a\\or b\\or c\\else d\\fi"), "c");
        assert_eq!(ok("\\ifcase 5 a\\or b\\else d\\fi"), "d");
        assert_eq!(ok("\\ifcase 0 a\\or b\\fi"), "a");
    }

    #[test]
    fn ifx_compares_meanings() {
        assert_eq!(ok("\\def\\a{x}\\def\\b{x}\\ifx\\a\\b y\\else n\\fi"), "y");
        assert_eq!(ok("\\def\\a{x}\\long\\def\\b{x}\\ifx\\a\\b y\\else n\\fi"), "n");
        assert_eq!(ok("\\ifx\\undefined\\alsoundefined y\\fi"), "y");
        assert_eq!(ok("\\let\\r\\relax\\ifx\\r\\relax y\\fi"), "y");
        assert_eq!(ok("\\ifx aay\\fi\\ifx abn\\fi"), "y");
    }

    #[test]
    fn if_and_ifcat() {
        assert_eq!(ok("\\def\\a{x}\\if\\a xy\\fi"), "y");
        assert_eq!(ok("\\ifcat a1 n\\else y\\fi"), "y");
        assert_eq!(ok("\\if\\relax\\relax y\\fi"), "y");
    }

    #[test]
    fn defined_and_csname() {
        assert_eq!(ok("\\ifdefined\\nosuch n\\else y\\fi\\ifdefined\\def y\\fi"), "yy");
        assert_eq!(ok("\\def\\q{}\\ifcsname q\\endcsname y\\fi\\ifcsname zz\\endcsname n\\fi"), "y");
    }

    #[test]
    fn registers_and_the() {
        assert_eq!(ok("\\count1=42 \\the\\count1"), "42");
        assert_eq!(ok("\\countdef\\c=5 \\c=7 \\advance\\c by 3 \\multiply\\c 2 \\divide\\c -3 \\the\\c"), "-6");
        assert_eq!(ok("\\dimen0=1in \\the\\dimen0"), "72.26999pt");
        // A glue is read on past the space for `plus` and `minus`, expanding as it goes, so
        // `\relax` ends it (otherwise `\the` would expand first and show the old value).
        assert_eq!(ok("\\skip3=1pt plus 2fil minus 1fill\\relax\\the\\skip3"), "\\relax 1.0pt plus 2.0fil minus 1.0fill");
        assert_eq!(ok("\\skip3=1pt \\the\\skip3"), "0.0pt");
        assert_eq!(ok("\\parindent=15pt \\dimen2=.5\\parindent \\the\\dimen2"), "7.5pt");
        assert_eq!(ok("\\chardef\\x=65 \\number\\x\\x"), "65A");
        assert_eq!(ok("\\romannumeral 1994"), "mcmxciv");
        assert_eq!(ok("\\tolerance=200 \\the\\tolerance"), "200");
    }

    #[test]
    fn registers_are_grouped() {
        assert_eq!(ok("\\count1=1 {\\count1=2 }\\the\\count1"), "{}1");
        assert_eq!(ok("\\count1=1 {\\global\\count1=2 }\\the\\count1"), "{}2");
    }

    #[test]
    fn futurelet_and_afterassignment() {
        assert_eq!(ok("\\def\\p{\\ifx\\n x[x]\\else[o]\\fi}\\futurelet\\n\\p x"), "[x]x ");
        assert_eq!(ok("\\def\\b{B}\\afterassignment\\b\\count1=3 c"), "Bc ");
    }

    #[test]
    fn keywords_put_back_what_they_read() {
        // `p` of `plus` is not followed by `l`: the width ends, `pt` is read, `px` stays.
        assert_eq!(ok("\\skip1=1pt px\\the\\skip1"), "px1.0pt");
    }

    /// `RT_SMOKE=file.tex cargo test --release smoke -- --ignored --nocapture`: runs a real
    /// file (with `@` a letter) and prints the errors and timing.
    #[test]
    #[ignore]
    fn smoke() {
        let Ok(path) = std::env::var("RT_SMOKE") else { return };
        let src = std::fs::read_to_string(&path).unwrap();
        let src = format!("\\catcode`\\@=11 {src}");
        let t = std::time::Instant::now();
        let (out, errors, e) = run(&src);
        eprintln!("{path}: {:?}, {} output bytes, {} names, {} stored tokens", t.elapsed(), out.len(), e.cs.len(), e.toks.len());
        for err in errors.iter().take(20) {
            eprintln!("  {err}");
        }
        eprintln!("  {} errors", errors.len());
    }

    #[test]
    fn extra_fi_is_an_error() {
        let (_, errors, _) = run("\\fi");
        assert!(errors[0].contains("Extra \\fi"), "{errors:?}");
    }

    #[test]
    fn loop_counts_down() {
        // plain.tex `\loop`, with conditionals and registers.
        let src = "\\def\\loop#1\\repeat{\\def\\body{#1}\\iterate}\\def\\iterate{\\body\\let\\next\\iterate\\else\\let\\next\\relax\\fi\\next}\\let\\repeat\\fi\\count1=3 \\loop\\the\\count1 \\ifnum\\count1>0 \\advance\\count1 -1 \\repeat";
        let out = ok(src);
        assert_eq!(out.replace("\\relax ", ""), "3210");
    }
}

#[cfg(test)]
mod noexpand_tests {
    use super::super::{Engine, Machine};

    #[test]
    fn noexpand_marks_only_expandable_tokens() {
        let mut e = Engine::default();
        let src = "\\def\\a{x}\\chardef\\c=5 \\expandafter\\ifx\\noexpand\\c\\c T\\else F\\fi\\expandafter\\ifx\\noexpand\\a\\a T\\else F\\fi\\expandafter\\ifx\\noexpand\\a\\relax T\\else F\\fi\\edef\\b{\\noexpand\\a}\\meaning\\b";
        let mut m = Machine::new(&mut e, src);
        m.run();
        assert!(m.errors.is_empty(), "{:?}", m.errors);
        // §358: a marked chardef keeps its meaning; a marked macro is neither itself nor `\relax`.
        assert_eq!(m.out_text(), "TFFmacro:->\\a ");
    }
}
