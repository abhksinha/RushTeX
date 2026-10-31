//! babel on the native path (`2026-10-31-native-babel.md`): babel.sty (2026), english.ldf,
//! portuges.ldf and brazil.ldf, spanish.ldf. What a language changes in the boxes is data
//! ([`LANGS`]); `\bbl@switch` (babel.sty:683) is [`Machine::babel_switch`]:
//!
//! - `\bbl@patterns`: `\language` is the format's `\l@<name>` (`data_hyph`), and
//!   `\set@hyphenmins` the language's `\<name>hyphenmins`;
//! - `\captions<name>` (with `\selectlanguage`, not `\foreignlanguage`): the caption macros
//!   the native job reads by meaning, and `\date<name>`, which [`Machine::babel_today`] reads;
//! - `\extras<name>`: spanish's `\textspanish` (`\bbl@frenchspacing`, the spaced `\%`) and,
//!   without `es-nodecimaldot`, its shorthand `.` in math (`\mathcode`\.="8000`).
//!
//! `\selectlanguage` writes `\babel@aux` to the `.aux` file (an inert whatsit here) unless
//! only one language is loaded; the main language (the last option) is selected so at
//! `\begin{document}`. A shorthand character (`"`, spanish's `<` `>`), a command a language
//! redefines that is not ported, and spanish's layout changes other than the heading
//! numbers' dot and the first paragraph's indent are refused by the pre-scan
//! ([`ScanBabel`]).

use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::Code;
use super::super::engine_node::Kind;
use super::super::engine_prim::{Macro, Meaning};
use super::super::engine_run::{Machine, Mode};
use super::super::engine_token::Token;
use super::{Reg, Region};

/// The language in force (a [`LANGS`] index), grouped as babel's `\languagename` and the
/// switch's assignments are.
pub(super) const BABEL_LANG: Reg = Reg::register(Region::Int, 32712);
/// The language of `\today` (`\date<name>` redefines it, `\foreignlanguage` does not).
pub(super) const BABEL_DATE: Reg = Reg::register(Region::Int, 32713);

/// How a language's `\today` reads (`\date<name>`).
#[derive(Clone, Copy, PartialEq)]
enum Date {
    /// english.ldf's US form (the kernel's): `October 31, 2026`.
    Us,
    /// english.ldf's UK form: `31st~October 2026`.
    Uk,
    /// portuges.ldf: `\number\day\space de\space <month>\space de\space\number\year`.
    Pt,
    /// spanish.ldf's `\es@date`: `31\relax~de octubre\ de\relax~2026\relax`.
    Es,
}

/// One of babel's languages as the native job selects it.
pub(super) struct Lang {
    /// The package options naming it (babel's `\languagename` is the option).
    names: &'static [&'static str],
    /// Its patterns: `None` for language 0 (`hyphen.tex`), else a `data_hyph::LANGUAGES` index.
    hyph: Option<usize>,
    /// `\<name>hyphenmins`' right value (the left is 2 for all of them).
    right_min: i32,
    /// `\captions<name>`: the macros the native job reads by meaning, as `\meaning` prints
    /// them under pdfLaTeX (spanish's `\SetString`s after their `\protected@edef`).
    captions: [&'static str; CAPTIONS.len()],
    date: Date,
    /// `\bbl@frenchspacing` in its extras.
    french: bool,
    /// Its extras make `"` a shorthand (`\initiate@active@char{"}`).
    quote: bool,
}

/// The caption macros [`Lang::captions`] sets, by name.
const CAPTIONS: [&str; 5] = ["figurename", "tablename", "abstractname", "refname", "proofname"];

pub(super) const ENGLISH: usize = 0;
pub(super) const BRITISH: usize = 1;
pub(super) const SPANISH: usize = 4;

pub(super) static LANGS: [Lang; 5] = [
    Lang { names: &["english", "american", "USenglish"], hyph: None, right_min: 3, captions: ["Figure", "Table", "Abstract", "References", "Proof"], date: Date::Us, french: false, quote: false },
    Lang { names: &["british", "UKenglish"], hyph: Some(0), right_min: 3, captions: ["Figure", "Table", "Abstract", "References", "Proof"], date: Date::Uk, french: false, quote: false },
    Lang { names: &["brazilian", "brazil"], hyph: Some(1), right_min: 3, captions: ["Figura", "Tabela", "Resumo", "Refer\\^encias", "Demonstra\\c {c}\\~ao"], date: Date::Pt, french: false, quote: true },
    Lang { names: &["portuguese", "portuges"], hyph: Some(1), right_min: 3, captions: ["Figura", "Tabela", "Resumo", "Refer\\^encias", "Demonstra\\c {c}\\~ao"], date: Date::Pt, french: false, quote: true },
    Lang { names: &["spanish"], hyph: Some(2), right_min: 2, captions: ["Figura", "Cuadro", "Resumen", "Referencias", "Demostraci\\'{o}n"], date: Date::Es, french: true, quote: true },
];

/// spanish.ldf's options the native job takes, each a [`Babel`] flag.
const ES_NODECIMALDOT: u8 = 1;
const ES_TABLA: u8 = 2;
const ES_NOSECTIONDOT: u8 = 4;
const ES_NOQUOTING: u8 = 8;
const ES_LCROMAN: u8 = 16;
const ES_OPTIONS: [(&str, u8); 5] = [("es-nodecimaldot", ES_NODECIMALDOT), ("es-tabla", ES_TABLA), ("es-nosectiondot", ES_NOSECTIONDOT), ("es-noquoting", ES_NOQUOTING), ("es-lcroman", ES_LCROMAN)];

/// `\usepackage[opts]{babel}` read: the languages in option order (the last is the main
/// language) and spanish's flags; `None` for an option the native job does not take.
pub(super) fn babel_options(opts: &str) -> Option<(Vec<usize>, u8)> {
    let mut langs = Vec::new();
    let mut flags = 0;
    for o in opts.split(',').map(str::trim).filter(|o| !o.is_empty()) {
        if let Some(k) = LANGS.iter().position(|l| l.names.contains(&o)) {
            // A language named twice (or by two of its names, two locales of one `.ldf`)
            // is not taken.
            if langs.contains(&k) {
                return None;
            }
            langs.push(k);
            continue;
        }
        flags |= ES_OPTIONS.iter().find(|e| e.0 == o)?.1;
    }
    // A spanish option without spanish, or no language, is babel's error or warning.
    (!langs.is_empty() && (flags == 0 || langs.contains(&SPANISH))).then_some((langs, flags))
}

/// What the pre-scan knows of a babel load (`scan_with`).
#[derive(Clone, Copy, Default, PartialEq)]
pub(super) struct ScanBabel {
    /// The options' languages, as a bit set of [`LANGS`] indexes.
    pub langs: u32,
    pub main: usize,
    pub flags: u8,
}

impl ScanBabel {
    pub(super) fn new(opts: &str) -> Option<ScanBabel> {
        let (langs, flags) = babel_options(opts)?;
        Some(ScanBabel { langs: langs.iter().fold(0, |m, &k| m | 1 << k), main: *langs.last()?, flags })
    }

    pub(super) fn spanish(&self) -> bool {
        self.langs & 1 << SPANISH != 0
    }

    /// Whether `"` is a shorthand character (each extras' `\initiate@active@char{"}` makes
    /// it active at `\begin{document}`, in every language).
    pub(super) fn quote(&self) -> bool {
        (0..LANGS.len()).any(|k| self.langs & 1 << k != 0 && LANGS[k].quote)
    }

    /// Whether `<` and `>` are spanish's quoting shorthands.
    pub(super) fn quoting(&self) -> bool {
        self.spanish() && self.flags & ES_NOQUOTING == 0
    }

    /// Whether spanish's math `.` before a digit is a decimal comma.
    pub(super) fn decimal(&self) -> bool {
        self.spanish() && self.flags & ES_NODECIMALDOT == 0
    }

    /// Whether `\layoutspanish` runs (spanish is the main language).
    pub(super) fn layout(&self) -> bool {
        self.main == SPANISH
    }

    /// Whether `name` is one of the loaded languages' names.
    pub(super) fn has(&self, name: &str) -> bool {
        (0..LANGS.len()).any(|k| self.langs & 1 << k != 0 && LANGS[k].names.contains(&name))
    }
}

/// A native document's babel: the languages loaded and what the switch saves.
#[derive(PartialEq, Clone)]
pub(super) struct Babel {
    pub langs: Vec<usize>,
    /// The option each language was loaded by (`\languagename`), as the document must name it.
    names: Vec<&'static str>,
    pub main: usize,
    pub flags: u8,
    /// Each [`LANGS`] entry's `\language` number in the format (0 when not loaded).
    numbers: [u8; LANGS.len()],
    /// The meanings the switch restores: `\%` and `\mathcode`\.`.
    percent: Meaning,
    period: i32,
    /// `\frenchspacing`'s codes for `.?!:;,` when the switch runs: LaTeX's 1000s, or
    /// amsgen.sty's 1006 to 1001 once it is loaded (amsgen.sty:127).
    french: [i32; 6],
}

/// `\nonfrenchspacing`'s codes for `.?!:;,` (latex.ltx:554).
pub(super) const NONFRENCH: [(u8, i32); 6] = [(b'.', 3000), (b'?', 3000), (b'!', 3000), (b':', 2000), (b';', 1500), (b',', 1250)];

impl Babel {
    /// `\ifbbl@single`: one language, so `\selectlanguage` is `\@gobble` after
    /// `\begin{document}` and writes nothing.
    fn single(&self) -> bool {
        self.langs.len() == 1
    }

    pub(super) fn layout(&self) -> bool {
        self.main == SPANISH
    }

    /// spanish.ldf's `\es@optlayout{nosectiondot}`: `\@seccntformat` with a dot.
    pub(super) fn section_dot(&self) -> bool {
        self.layout() && self.flags & ES_NOSECTIONDOT == 0
    }
}

impl Machine<'_, '_> {
    /// `\usepackage[opts]{babel}`: the languages' patterns as the format has them, the
    /// language commands, and the state the switch restores. The main language is
    /// selected at `\begin{document}` ([`Machine::babel_begin_document`]).
    pub(super) fn babel_load(&mut self, opts: &str) {
        let Some(doc) = self.doc.as_ref() else { return };
        if doc.babel.is_some() {
            return self.not_native("babel loaded twice");
        }
        let Some((langs, flags)) = babel_options(opts) else { return self.not_native("a babel option") };
        let mut numbers = [0u8; LANGS.len()];
        for &k in &langs {
            let Some(h) = LANGS[k].hyph else { continue };
            let Some(t) = crate::data::data_hyph::language(h) else { return self.not_native("a babel language's patterns") };
            numbers[k] = t.number;
            self.e.hyph.set_language(t.number, t.patterns, t.exceptions());
            if let Some(parts) = &t.trie {
                self.e.hyph.set_trie(t.number, parts.each_ref().map(|s| s.iter().collect()));
            }
        }
        // The option each language was named by, the last of its names in the list.
        let names = langs.iter().map(|&k| opts.split(',').map(str::trim).rfind(|o| LANGS[k].names.contains(o)).and_then(|o| LANGS[k].names.iter().find(|n| *n == &o).copied()).unwrap_or(LANGS[k].names[0])).collect();
        let percent_id = self.e.cs.intern("%");
        let percent = self.e.eqtb.meaning(percent_id);
        let period = self.e.eqtb.code(Code::Math, '.' as u32);
        let main = *langs.last().unwrap_or(&ENGLISH);
        for (name, native) in [("selectlanguage", "rt@selectlanguage"), ("foreignlanguage", "rt@foreignlanguage")] {
            if let Some(i) = super::DOC.iter().position(|d| d.0 == native) {
                let id = self.e.cs.intern(name);
                self.e.eqtb.set_meaning(id, Meaning::Doc(i as u16), true);
            }
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.babel = Some(Box::new(Babel { langs, names, main, flags, numbers, percent, period, french: [1000; 6] }));
        }
    }

    /// babel's `\AtBeginDocument`: `\selectlanguage{<main>}`, whose `\babel@aux` write is
    /// always made (`\ifbbl@single` takes effect after it). spanish's switch changes the
    /// OT1 accents (`\shorthandsspanish`), so a spanish document must be T1.
    pub(super) fn babel_begin_document(&mut self) {
        let Some(b) = self.doc.as_ref().and_then(|d| d.babel.as_ref()) else { return };
        let main = b.main;
        if b.langs.contains(&SPANISH) && !self.t1() {
            return self.not_native("babel's spanish without T1");
        }
        let amsgen = self.doc.as_ref().is_some_and(|d| d.packages.contains(&"amsgen"));
        if let Some(b) = self.doc.as_mut().and_then(|d| d.babel.as_mut()) {
            b.french = if amsgen { [1006, 1005, 1004, 1003, 1002, 1001] } else { [1000; 6] };
        }
        self.babel_switch(main, true);
        self.aux_write(0);
    }

    /// The language named by `name` among those loaded.
    fn babel_named(&self, name: &[Token]) -> Option<usize> {
        let b = self.doc.as_ref()?.babel.as_ref()?;
        let text: String = name.iter().map(|t| if t.cs_id().is_some() { '\\' } else { t.chr() }).collect();
        b.langs.iter().zip(&b.names).find(|(_, n)| **n == text.trim()).map(|(&k, _)| k)
    }

    /// `\selectlanguage{name}` (`\bbl@select@s`): at the document's own level, where
    /// `\aftergroup\bbl@pop@language` never runs. The switch with captions and date, then
    /// the `\babel@aux` write (none with one language: `\selectlanguage` is gobbled).
    pub(super) fn select_language(&mut self) {
        if self.next_is('[') {
            return self.not_native("\\selectlanguage options");
        }
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let Some(k) = self.babel_named(&name) else { return self.not_native("\\selectlanguage of a language not loaded") };
        // The document's own level (`\document` ends `\begin`'s group).
        if self.e.eqtb.level() != 1 {
            return self.not_native("\\selectlanguage in a group");
        }
        if self.doc.as_ref().and_then(|d| d.babel.as_ref()).is_some_and(|b| b.single()) {
            return;
        }
        self.babel_switch(k, true);
        self.aux_write(0);
    }

    /// `\foreignlanguage{name}{text}` (`\bbl@foreign@x`): `\begingroup`, the switch without
    /// captions and date, the text, `\endgroup`. With one language it is `#2`.
    pub(super) fn foreign_language(&mut self) {
        if self.next_is('*') || self.next_is('[') {
            return self.not_native("\\foreignlanguage* or options");
        }
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let mut text = Vec::new();
        self.read_arg(&mut text);
        let Some(k) = self.babel_named(&name) else { return self.not_native("\\foreignlanguage of a language not loaded") };
        if self.doc.as_ref().and_then(|d| d.babel.as_ref()).is_some_and(|b| b.single()) {
            return self.back_list(&text);
        }
        self.begin_group(super::SEMI_SIMPLE_GROUP);
        self.babel_switch(k, false);
        text.push(self.doc_id(doc!("rt@foreignend")));
        self.back_list(&text);
    }

    pub(super) fn foreign_end(&mut self) {
        self.end_group(super::SEMI_SIMPLE_GROUP);
    }

    /// `\bbl@switch{name}`: `\originalTeX` (the last language's saved values back), the
    /// captions and date when `select`, the extras, `\language` and the hyphen minima. All
    /// local. Its `\bbl@bsphack`/`\bbl@esphack` add and take off a `\hskip\z@skip`: nothing.
    fn babel_switch(&mut self, k: usize, select: bool) {
        let Some(b) = self.doc.as_ref().and_then(|d| d.babel.as_ref()) else { return };
        let (number, flags, percent, period, french) = (b.numbers[k], b.flags, b.percent, b.period, b.french);
        let lang = &LANGS[k];
        self.e.eqtb.set_word(BABEL_LANG, k as i32, false);
        if select {
            for (i, name) in CAPTIONS.iter().enumerate() {
                let mut text = if k == SPANISH && i == 1 && flags & ES_TABLA != 0 { self.tex_tokens("Tabla") } else { self.tex_tokens(lang.captions[i]) };
                text.insert(0, Token::END_MATCH);
                let text = self.e.toks.store(&text);
                let id = self.e.cs.intern(name);
                self.e.eqtb.set_meaning(id, Meaning::Macro(Macro { text, flags: 0 }), false);
            }
            // `\date<name>` redefines `\today`, which the native `\today` reads.
            self.e.eqtb.set_word(BABEL_DATE, k as i32, false);
            let id = self.e.cs.intern("today");
            self.e.eqtb.set_meaning(id, Meaning::Doc(doc!("today") as u16), false);
        }
        // `\extras<name>`: spanish's `\bbl@frenchspacing` (LaTeX's `\frenchspacing`) or the
        // `\nonfrenchspacing` that `\noextrasspanish` restores.
        for (i, (c, normal)) in NONFRENCH.into_iter().enumerate() {
            self.e.eqtb.set_code(Code::Sf, c as u32, if lang.french { french[i] } else { normal }, false);
        }
        // `\textspanish`'s `\es@sDRC\%{\es@sppercent\percentsign{}}`, and without
        // `es-nodecimaldot` `\shorthandsspanish`'s `\mathcode`\.="8000` with `.` the
        // shorthand (`\es@period@code` the old `\mathcode`).
        let percent_id = self.e.cs.intern("%");
        let spanish = k == SPANISH;
        self.e.eqtb.set_meaning(percent_id, if spanish { Meaning::Doc(doc!("rt@espercent") as u16) } else { percent }, false);
        if flags & ES_NODECIMALDOT == 0 && self.doc.as_ref().and_then(|d| d.babel.as_ref()).is_some_and(|b| b.langs.contains(&SPANISH)) {
            self.e.eqtb.set_code(Code::Math, '.' as u32, if spanish { 0x8000 } else { period }, false);
            if spanish {
                self.e.eqtb.set_meaning('.' as u32, Meaning::Doc(doc!("rt@esperiod") as u16), false);
                let id = self.e.cs.intern("rt@esperiodcode");
                self.e.eqtb.set_meaning(id, Meaning::MathCharDef(period as u32), false);
            }
        }
        // `\bbl@patterns{name}` and `\set@hyphenmins`.
        self.e.eqtb.set_word(param!("language"), number as i32, false);
        self.e.eqtb.set_word(param!("lefthyphenmin"), 2, false);
        self.e.eqtb.set_word(param!("righthyphenmin"), lang.right_min, false);
    }

    /// `\@outputpage`'s `\normalsfcodes`: babel's `\AtBeginDocument` (babel.sty:3204) makes
    /// it the `\frenchspacing` or `\nonfrenchspacing` the main language left, in the output
    /// group.
    pub(super) fn babel_output_sfcodes(&mut self) {
        let Some(b) = self.doc.as_ref().and_then(|d| d.babel.as_ref()) else { return };
        let french = LANGS[b.main].french.then_some(b.french);
        for (i, (c, normal)) in NONFRENCH.into_iter().enumerate() {
            self.e.eqtb.set_code(Code::Sf, c as u32, french.map_or(normal, |f| f[i]), false);
        }
    }

    /// The language whose `\today` is in force, if babel is loaded.
    pub(super) fn babel_date(&self) -> Option<usize> {
        self.doc.as_ref()?.babel.as_ref()?;
        Some(self.e.eqtb.word(BABEL_DATE) as usize)
    }

    /// `\today` as `\date<name>` defines it, for a language whose form is not the kernel's.
    pub(super) fn babel_today(&mut self, k: usize, day: i32, month: i32, year: i32) -> Option<Vec<Token>> {
        let letters = |s: &str, t: &mut Vec<Token>| {
            for c in s.chars() {
                t.push(match c {
                    ' ' => Token::char(Cat::Space, ' '),
                    '~' => Token::char(Cat::Active, '~'),
                    c if c.is_ascii_alphabetic() => Token::char(Cat::Letter, c),
                    c => Token::char(Cat::Other, c),
                });
            }
        };
        const EN: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
        const PT: [&str; 12] = ["janeiro", "fevereiro", "mar\\c{c}o", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"];
        const ES: [&str; 12] = ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"];
        let m = (month - 1).clamp(0, 11) as usize;
        let mut t = Vec::new();
        match LANGS[k].date {
            Date::Us => return None,
            Date::Uk => {
                let suffix = match day {
                    1 | 21 | 31 => "st",
                    2 | 22 => "nd",
                    3 | 23 => "rd",
                    _ => "th",
                };
                letters(&format!("{day}{suffix}~{} {year}", EN[m]), &mut t);
            }
            // `mar\c{c}o` is the only month with a command.
            Date::Pt => {
                letters(&format!("{day} de "), &mut t);
                if m == 2 {
                    t.extend(self.tex_tokens(PT[2]));
                } else {
                    letters(PT[m], &mut t);
                }
                letters(&format!(" de {year}"), &mut t);
            }
            // `\es@date`: `\number#4\relax ~de <month>\ de\es@yearl~\number#2\relax` (the
            // `\relax` after an internal integer stays), `\es@datefirst` is `1`.
            Date::Es => {
                let relax = self.named_cs(named!("relax"));
                letters(&day.to_string(), &mut t);
                t.push(relax);
                letters(&format!("~de {}", ES[m]), &mut t);
                t.push(Token::cs(self.e.cs.intern(" ")));
                letters("de", &mut t);
                // `\ifnum#2>1999\relax\es@yearl`: the `\relax` ends the number and is gone.
                letters(&format!("~{year}"), &mut t);
                t.push(relax);
            }
        }
        Some(t)
    }

    /// spanish's `\%` (`\es@sppercent\percentsign{}`): `\leavevmode`, the space before it
    /// taken off when finite (`\ifnum\gluestretchorder\lastskip=\z@\unskip\fi`), a thin
    /// space (`$\m@th\,$`), then the old `\%`.
    pub(super) fn es_percent(&mut self) {
        if self.mode == Mode::Math {
            return self.not_native("spanish's \\% in math");
        }
        if self.leave_vmode() {
            return;
        }
        if self.tail().is_some_and(|t| t.kind == Kind::Glue && self.e.nodes.glue(&t).stretch_order == 0) {
            if let Some(n) = self.cur.pop() {
                self.e.nodes.flush_node(n);
            }
        }
        let Some(percent) = self.doc.as_ref().and_then(|d| d.babel.as_ref()).map(|b| b.percent) else { return };
        let id = self.e.cs.intern("rt@percentsign");
        self.e.eqtb.set_meaning(id, percent, false);
        let shift = Token::char(Cat::MathShift, '$');
        let mut t = vec![shift, self.named_cs(named!("mathsurround"))];
        t.extend(super::ZERO_PT);
        t.extend([self.named_cs(named!(",")), shift, Token::cs(id), Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}')]);
        self.back_list(&t);
    }

    /// spanish's math `.` (`\normal@char.` in math): `\@ifnextchar\egroup` the old `.`,
    /// else the shorthand reads the next token as its argument: a digit is `\es@decimal`
    /// (`{,}` and the digit), anything else the old `.` (`\system@sh@.@`) and the token.
    pub(super) fn es_period(&mut self) {
        let code = Token::cs(self.e.cs.intern("rt@esperiodcode"));
        let Some(next) = self.next_unblank() else { return self.input.back(code) };
        if next.cat() == Some(Cat::EndGroup) || next.cat() == Some(Cat::BeginGroup) {
            if next.cat() == Some(Cat::BeginGroup) {
                return self.not_native("spanish's math . before a group");
            }
            return self.back_list(&[code, next]);
        }
        if next.cat() == Some(Cat::Other) && next.chr().is_ascii_digit() {
            return self.back_list(&[Token::char(Cat::BeginGroup, '{'), Token::char(Cat::Other, ','), Token::char(Cat::EndGroup, '}'), next]);
        }
        self.back_list(&[code, next]);
    }
}
