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
use super::super::engine_node::{Kind, Node};
use super::super::engine_prim::{Macro, Meaning};
use super::super::engine_run::{Machine, Mode};
use super::super::engine_token::Token;
use super::{Reg, Region};

/// The language in force (a [`LANGS`] index), grouped as babel's `\languagename` and the
/// switch's assignments are.
pub(super) const BABEL_LANG: Reg = Reg::register(Region::Int, 32712);
/// The language of `\today` (`\date<name>` redefines it, `\foreignlanguage` does not).
pub(super) const BABEL_DATE: Reg = Reg::register(Region::Int, 32713);
/// french3.ldf's `\ifFB@spacing` turned off (1): `\ttfamily` is `\ttfamilyFB`
/// (`\FB@spacingfalse`), `\rmfamily` and `\sffamily` turn it back on; local, as `\newif`'s
/// switches are.
pub(super) const FB_NOSPACING: Reg = Reg::register(Region::Int, 32716);

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
    /// babel-german.def's `\date@german@de`: `\number\day.~\month@german\space\number\year`.
    De,
    /// french3.ldf's `\FB@date`: `{{\number\day}\FBdatespace<month>\FBdatespace\number\year}`
    /// (the first day's `\ier`, a raised `\up{er}`, is not ported).
    Fr,
    /// italian.ldf's `\dateitalian`: `\number\day~<month>\space\number\year` (`1` on the first
    /// day: `\OrdinalDayNumberfalse`).
    It,
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
    /// Its `babel-<tag>.ini` has `decimal = ,`, which siunitx's `\AtBeginDocument`
    /// (`\__siunitx_locale_setup:`, siunitx.sty:4644) makes its output decimal marker when the
    /// language is main: `\BCPdata{language}` with no region for an `.ldf` language, so
    /// `babel-es.ini`, `babel-pt.ini`, `babel-de.ini`, `babel-en.ini`.
    comma: bool,
}

/// The caption macros [`Lang::captions`] sets, by name.
const CAPTIONS: [&str; 5] = ["figurename", "tablename", "abstractname", "refname", "proofname"];

pub(super) const ENGLISH: usize = 0;
pub(super) const BRITISH: usize = 1;
pub(super) const SPANISH: usize = 4;
pub(super) const NGERMAN: usize = 5;
pub(super) const ITALIAN: usize = 6;
pub(super) const FRENCH: usize = 7;
pub(super) const GERMAN: usize = 8;

/// german.ldf and ngerman.ldf share babel-german.def (captions, `\date@german@de`, the
/// extras: `\umlautlow`, `\lccode25`); they differ in their patterns (`\l@german`, the old
/// orthography's) and in shorthands the pre-scan refuses.
fn german(k: usize) -> bool {
    k == NGERMAN || k == GERMAN
}

pub(super) static LANGS: [Lang; 9] = [
    Lang { names: &["english", "american", "USenglish"], hyph: None, right_min: 3, captions: ["Figure", "Table", "Abstract", "References", "Proof"], date: Date::Us, french: false, quote: false, comma: false },
    Lang { names: &["british", "UKenglish"], hyph: Some(0), right_min: 3, captions: ["Figure", "Table", "Abstract", "References", "Proof"], date: Date::Uk, french: false, quote: false, comma: false },
    Lang { names: &["brazilian", "brazil"], hyph: Some(1), right_min: 3, captions: ["Figura", "Tabela", "Resumo", "Refer\\^encias", "Demonstra\\c {c}\\~ao"], date: Date::Pt, french: false, quote: true, comma: true },
    Lang { names: &["portuguese", "portuges"], hyph: Some(1), right_min: 3, captions: ["Figura", "Tabela", "Resumo", "Refer\\^encias", "Demonstra\\c {c}\\~ao"], date: Date::Pt, french: false, quote: true, comma: true },
    Lang { names: &["spanish"], hyph: Some(2), right_min: 2, captions: ["Figura", "Cuadro", "Resumen", "Referencias", "Demostraci\\'{o}n"], date: Date::Es, french: true, quote: true, comma: true },
    // ngerman.ldf (babel-german.def): `\providehyphenmins{ngerman}{\tw@\tw@}`; the
    // `\l@ngerman` patterns (hyphenrules `legacy`, no `-x-` dialect).
    Lang { names: &["ngerman"], hyph: Some(3), right_min: 2, captions: ["Abbildung", "Tabelle", "Zusammenfassung", "Literatur", "Beweis"], date: Date::De, french: true, quote: true, comma: true },
    // italian.ldf: `\providehyphenmins{\CurrentOption}{\tw@\tw@}`; no `\bbl@frenchspacing`;
    // `"` is a shorthand only after `\setactivedoublequote`. Its extras are
    // [`Machine::babel_switch`]'s.
    Lang { names: &["italian"], hyph: Some(4), right_min: 2, captions: ["Figura", "Tabella", "Sommario", "Riferimenti bibliografici", "Dimostrazione"], date: Date::It, french: false, quote: false, comma: true },
    // french.ldf under pdfTeX is french3.ldf: `\providehyphenmins{french}{\tw@\thr@@}`,
    // `\bbl@frenchspacing`, the active `: ; ! ?` ([`Machine::fr_punct`]). Native as the only
    // language ([`ScanBabel::new`]), so its extras run once and are never undone.
    Lang { names: &["french"], hyph: Some(5), right_min: 3, captions: ["Figure", "Table", "R\\'esum\\'e", "R\\'ef\\'erences", "D\\'emonstration"], date: Date::Fr, french: true, quote: false, comma: true },
    // german.ldf (`germanb` its old name): babel-german.def as ngerman, `\providehyphenmins
    // {\CurrentOption}{\tw@\tw@}`, the `\l@german` patterns.
    Lang { names: &["german", "germanb"], hyph: Some(6), right_min: 2, captions: ["Abbildung", "Tabelle", "Zusammenfassung", "Literatur", "Beweis"], date: Date::De, french: true, quote: true, comma: true },
];

/// The heading names the classes set that `\captions<name>` changes, by [`LANGS`] index
/// (`\contentsname`, `\listfigurename`, `\listtablename`, `\bibname`, `\chaptername`,
/// `\appendixname`), as `\meaning` prints them under pdfLaTeX (spanish's and french's
/// `\spanishcontentsname` and kin read through). A column of its own beside [`LANGS`]: only
/// a heading reads it ([`Machine::head_name`]).
static HEADS: [[&str; 6]; 9] = [
    ["Contents", "List of Figures", "List of Tables", "Bibliography", "Chapter", "Appendix"],
    ["Contents", "List of Figures", "List of Tables", "Bibliography", "Chapter", "Appendix"],
    ["Sum\\'ario", "Lista de Figuras", "Lista de Tabelas", "Refer\\^encias Bibliogr\\'aficas", "Cap\\'{\\i }tulo", "Ap\\^endice"],
    ["\\'Indice", "Lista de Figuras", "Lista de Tabelas", "Bibliografia", "Cap\\'{\\i }tulo", "Ap\\^endice"],
    ["\\'{I}ndice general", "\\'{I}ndice de figuras", "\\'{I}ndice de cuadros", "Bibliograf\\'{\\i }a", "Cap\\'{\\i }tulo", "Ap\\'{e}ndice"],
    ["Inhaltsverzeichnis", "Abbildungsverzeichnis", "Tabellenverzeichnis", "Literaturverzeichnis", "Kapitel", "Anhang"],
    ["Indice", "Elenco delle figure", "Elenco delle tabelle", "Bibliografia", "Capitolo", "Appendice"],
    ["Table des mati\\`eres", "Table des figures", "Liste des tableaux", "Bibliographie", "Chapitre", "Annexe"],
    ["Inhaltsverzeichnis", "Abbildungsverzeichnis", "Tabellenverzeichnis", "Literaturverzeichnis", "Kapitel", "Anhang"],
];
/// [`HEADS`]' columns.
pub(super) const HEAD_CONTENTS: usize = 0;
pub(super) const HEAD_LOF: usize = 1;
pub(super) const HEAD_LOT: usize = 2;
pub(super) const HEAD_BIB: usize = 3;
pub(super) const HEAD_CHAPTER: usize = 4;
pub(super) const HEAD_APPENDIX: usize = 5;

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
        // french is taken alone, its switch made once at `\begin{document}`.
        if langs.contains(&FRENCH) && langs.len() > 1 {
            return None;
        }
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

    /// Whether french is the (only) language.
    pub(super) fn french(&self) -> bool {
        self.main == FRENCH
    }

    /// Whether the main language is english.ldf's (`english`, `british`): its captions
    /// (`\chaptername`, `\bibname`, `\contentsname`, `\appendixname`, `\partname`, ...) are
    /// the classes' own, so it is native under report and book too.
    pub(super) fn english_main(&self) -> bool {
        matches!(self.main, ENGLISH | BRITISH)
    }

    /// Whether babel is native under the standard report and book: english.ldf's languages,
    /// and ngerman, whose heading names are [`HEADS`] (its `\partname` `Teil` is refused
    /// where `\part` is read).
    pub(super) fn report_ok(&self) -> bool {
        self.english_main() || german(self.main)
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
    /// The meanings the switch restores: `\%` and `\mathcode`\.`, and `\lccode25`
    /// (`\babel@savevariable` in german's extras).
    percent: Meaning,
    period: i32,
    lc25: i32,
    /// `\frenchspacing`'s codes for `.?!:;,` when the switch runs: LaTeX's 1000s, or
    /// amsgen.sty's 1006 to 1001 once it is loaded (amsgen.sty:127).
    french: [i32; 6],
}

/// `\nonfrenchspacing`'s codes for `.?!:;,` (latex.ltx:554).
pub(super) const NONFRENCH: [(u8, i32); 6] = [(b'.', 3000), (b'?', 3000), (b'!', 3000), (b':', 2000), (b';', 1500), (b',', 1250)];

impl Babel {
    /// The main language's `\language` (`\l@<name>`).
    pub(super) fn main_number(&self) -> u8 {
        self.numbers[self.main]
    }

    /// `\ifbbl@single`: one language, so `\selectlanguage` is `\@gobble` after
    /// `\begin{document}` and writes nothing.
    fn single(&self) -> bool {
        self.langs.len() == 1
    }

    pub(super) fn layout(&self) -> bool {
        self.main == SPANISH
    }

    /// `\let\@afterindentfalse\@afterindenttrue`: spanish's layout, french's `IndentFirst`
    /// (french3.ldf `\bbl@frenchindent`, on when french is the main language).
    pub(super) fn indent_first(&self) -> bool {
        self.layout() || self.main == FRENCH
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
        let lc25 = self.e.eqtb.code(Code::Lc, 25);
        let main = *langs.last().unwrap_or(&ENGLISH);
        for (name, native) in [("selectlanguage", "rt@selectlanguage"), ("foreignlanguage", "rt@foreignlanguage")] {
            if let Some(i) = super::DOC.iter().position(|d| d.0 == native) {
                let id = self.e.cs.intern(name);
                self.e.eqtb.set_meaning(id, Meaning::Doc(i as u16), true);
            }
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.babel = Some(Box::new(Babel { langs, names, main, flags, numbers, percent, period, lc25, french: [1000; 6] }));
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
        // french3.ldf's captions (`\FB@makecaption` when `\@makecaption` is article's) and
        // footnotes are those of the standard classes; KOMA, memoir, the AMS and IEEE classes,
        // letters and the caption package have their own.
        if main == FRENCH && self.doc.as_ref().is_some_and(|d| d.koma || d.memoir || d.ams.is_some() || d.ieee.is_some() || d.letter.is_some() || d.packages.iter().any(|p| matches!(*p, "caption" | "subcaption" | "floatrow"))) {
            return self.not_native("babel's french under this class or with caption");
        }
        let amsgen = self.doc.as_ref().is_some_and(|d| d.packages.contains(&"amsgen"));
        if let Some(b) = self.doc.as_mut().and_then(|d| d.babel.as_mut()) {
            b.french = if amsgen { [1006, 1005, 1004, 1003, 1002, 1001] } else { [1000; 6] };
        }
        self.babel_switch(main, true);
        self.aux_write(0);
        // italian.ldf's own `\AtBeginDocument`: `\ifdefstring{\languagename}{italian}
        // {\selectlanguage{italian}}`, a second `\babel@aux` write (made with one language
        // too: the `.aux` holds `\babel@aux{italian}{}` twice).
        if main == ITALIAN {
            self.aux_write(0);
        }
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
        let (number, flags, percent, period, french, lc25) = (b.numbers[k], b.flags, b.percent, b.period, b.french, b.lc25);
        let lang = &LANGS[k];
        // italian.ldf's `\noextrasitalian` gives back what `\babel@savevariable` kept when
        // the switch to italian was made (the club and widow penalties, `\@clubpenalty`,
        // `\finalhyphendemerits`): a switch away from it at the document's level is not
        // ported (a `\foreignlanguage` group restores them itself).
        if self.e.eqtb.word(BABEL_LANG) as usize == ITALIAN && k != ITALIAN {
            return self.not_native("babel: a switch away from italian");
        }
        // Under report and book a language's captions reach `\chaptername` and the like,
        // which only english.ldf's leave as the class has them.
        if select && !matches!(k, ENGLISH | BRITISH) && self.doc.as_ref().is_some_and(|d| d.report && (!german(k) || d.koma || d.memoir)) {
            return self.not_native("babel's captions of a language under report or book");
        }
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
        // italian.ldf's extras: `\clubpenalty3000\widowpenalty3000\@clubpenalty\clubpenalty`,
        // `\finalhyphendemerits50000000`, `\lccode`'=`'` (`\noextrasitalian` sets it to 0, the
        // format's value); its `\@alph`/`\@Alph` (`\it@alph` with `\it@lettering` 1) are the
        // kernel's letters for 1 to 26, and `\ap`, `\ped` are not native names.
        if k == ITALIAN {
            self.set_int_named(param!("clubpenalty"), 3000);
            self.set_int_named(param!("widowpenalty"), 3000);
            self.set_rw(super::R_CLUBPENALTY, 3000, false);
            self.set_int_named(param!("finalhyphendemerits"), 50000000);
            self.e.eqtb.set_code(Code::Lc, 0x27, 0x27, false);
        }
        // german's `\lccode25=25` (`\babel@savevariable{\lccode25}` restores it).
        self.e.eqtb.set_code(Code::Lc, 25, if german(k) { 25 } else { lc25 }, false);
        // french's extras: `\lccode"27="27`, and `\bbl@activate` of `: ; ! ?`, active
        // (`\initiate@active@char`'s `\AtBeginDocument{\catcode`#2\active}`) with their
        // shorthands ([`Machine::fr_punct`]).
        if k == FRENCH {
            self.e.eqtb.set_code(Code::Lc, 0x27, 0x27, false);
            for (c, native) in [(':', doc!("rt@frcolon")), (';', doc!("rt@frsemicolon")), ('!', doc!("rt@frexclam")), ('?', doc!("rt@frquestion"))] {
                self.e.eqtb.set_catcode(c as u8, Cat::Active, false);
                self.e.eqtb.set_meaning(c as u32, Meaning::Doc(native as u16), false);
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

    /// Whether siunitx's output decimal marker is babel's `,` (the main language's ini).
    pub(super) fn babel_decimal_comma(&self) -> bool {
        self.doc.as_ref().and_then(|d| d.babel.as_ref()).is_some_and(|b| LANGS[b.main].comma)
    }

    /// The language whose `\today` is in force, if babel is loaded.
    /// The class's heading name `i` ([`HEADS`]' column) in the captions' language
    /// (`\captions<name>` goes with `\date<name>`), the class's own (english) without babel.
    pub(super) fn head_name(&self, i: usize) -> &'static str {
        let k = self.babel_date().unwrap_or(ENGLISH);
        // spanish.ldf's `es-tabla`: `\spanishlisttablename` `\'{I}ndice de tablas`.
        if k == SPANISH && i == HEAD_LOT && self.doc.as_ref().and_then(|d| d.babel.as_ref()).is_some_and(|b| b.flags & ES_TABLA != 0) {
            return "\\'{I}ndice de tablas";
        }
        HEADS[k][i]
    }

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
            // `M\"arz` is the only month with a command.
            Date::De => {
                const DE: [&str; 12] = ["Januar", "Februar", "M\\\"arz", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"];
                letters(&format!("{day}.~"), &mut t);
                if m == 2 {
                    t.extend(self.tex_tokens(DE[2]));
                } else {
                    letters(DE[m], &mut t);
                }
                letters(&format!(" {year}"), &mut t);
            }
            Date::Fr => {
                if day == 1 {
                    self.not_native("french \\today on the first of the month (\\ier)");
                    return Some(Vec::new());
                }
                const FR: [&str; 12] = ["janvier", "f\\'evrier", "mars", "avril", "mai", "juin", "juillet", "ao\\^ut", "septembre", "octobre", "novembre", "d\\'ecembre"];
                letters(&format!("{day} "), &mut t);
                t.extend(self.tex_tokens(FR[m]));
                letters(&format!(" {year}"), &mut t);
            }
            Date::It => {
                const IT: [&str; 12] = ["gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre"];
                letters(&format!("{day}~{} {year}", IT[m]), &mut t);
            }
        }
        Some(t)
    }

    /// babel's OT1 `\"` composites (babel.sty:2166, at `\begin{document}`) over `base`:
    /// `\bbl@umlauta` for `a o u A O U`, `\bbl@umlaute` for `e i \i E I`; `false` for
    /// another letter (the kernel's `\add@accent`). german's extras make `\bbl@umlauta`
    /// `\lower@umlaut` (`\umlautlow`): `\leavevmode\bgroup`, the x-height lowered to
    /// `\ht` of the dieresis less `.45ex` when that is more than 1ex (`\fontdimen5\font` is
    /// global), `\accent127`, `\fontdimen5\font\U@D` back between the accent and its letter
    /// (`make_accent` read `x_height` before, §1123, so the font is as it was), the letter,
    /// `\egroup`: the space factor
    /// is `\accent`'s 1000. `\umlauthigh` (other languages, and `\bbl@umlaute` in every
    /// language: `\umlautlow` redefines only `\bbl@umlauta`) is `\leavevmode\bgroup\accent127
    /// #1\bbl@allowhyphens\egroup`: the kernel's accent, then `\nobreak\hskip\z@skip`
    /// (babel.sty:1720; horizontal mode after the accent), read again.
    pub(super) fn babel_umlaut(&mut self, base: u32) -> bool {
        if self.doc.as_ref().and_then(|d| d.babel.as_ref()).is_none() {
            return false;
        }
        match u8::try_from(base).unwrap_or(0) {
            b'a' | b'o' | b'u' | b'A' | b'O' | b'U' if german(self.e.eqtb.word(BABEL_LANG) as usize) => {
                let f = self.cur_font();
                let ex = self.e.fonts.params(f)[4];
                let Some([_, ht, _, _]) = self.e.fonts.list[f as usize].dims(127, &self.e.fonts.info) else {
                    self.not_native("babel's OT1 \\\" in a font without a dieresis");
                    return true;
                };
                // `\dimen@ -.45ex \advance\dimen@\ht\z@` (`.45` is 29491/65536, §102).
                let d = ht - crate::backend::backend_scaled::xn_over_d(ex, 29491, 65536);
                self.build_accent_at(127, f, Some(base as u8), if ex < d { d } else { ex });
                self.aux = 1000;
                true
            }
            b'a' | b'o' | b'u' | b'A' | b'O' | b'U' | b'e' | b'i' | b'E' | b'I' | 16 => {
                let f = self.cur_font();
                self.build_accent(127, f, Some(base as u8));
                self.aux = 1000;
                // `\penalty\@M`, then `\z@skip`'s spec, the shared `zero_glue` (font 0).
                self.cur.push(Node::new(Kind::Penalty, 10000));
                let g = self.e.nodes.glue_node(Default::default(), 0);
                self.cur.push(g);
                true
            }
            _ => false,
        }
    }

    /// Whether `\layoutspanish` is in force (spanish the main language).
    pub(super) fn es_layout(&self) -> bool {
        self.doc.as_ref().and_then(|d| d.babel.as_ref()).is_some_and(|b| b.layout())
    }

    /// `\layoutspanish`'s lists (spanish.ldf `\es@enumerate`, `\es@itemize`): a list at
    /// `\@enumdepth`/`\@itemdepth` `d` (the new depth) the native job takes. Level 1 of
    /// enumerate is the kernel's `\theenumi.` behind `\es@listquot` (`\rquot` undefined
    /// outside `quoting`: `\relax`, and a `\global` flag); itemize's first two marks are
    /// [`Machine::es_item_mark`]. Deeper levels (`\emph{\alph{enumii}})`, `\theenumiii)`,
    /// `$\m@th\circ$`) are not ported.
    pub(super) fn es_list_ok(&self, enumerate: bool, d: i32) -> bool {
        !self.es_layout() || d <= if enumerate { 1 } else { 2 }
    }

    /// spanish's `\labelitemi`/`\labelitemii` (`\spanishsymbitems`): `\es@listquot` then a
    /// square, `\leavevmode\hbox to 1.2ex{\hss\vrule height .9ex width .7ex depth -.2ex\hss}`,
    /// or `\textbullet`; in the font in force (no `\labelitemfont`). The label is set in
    /// `\@tempboxa` (`\@item`), so `\leavevmode` does nothing there. `false` when spanish's
    /// layout is not in force.
    pub(super) fn es_item_mark(&mut self, d: u8, t: &mut Vec<Token>) -> bool {
        if !self.es_layout() {
            return false;
        }
        t.extend(self.tex_tokens(if d == 1 { "\\hbox to 1.2ex{\\hss\\vrule height .9ex width .7ex depth -.2ex\\hss}" } else { "\\textbullet" }));
        true
    }

    /// spanish's `\@roman` (small capitals in an `\hbox` unless `es-lcroman`) and `\@alph`
    /// (`\~n` at 14): a label of counter style `style` at `n` the native job cannot set.
    pub(super) fn es_counter_differs(&self, style: u8, n: i32) -> bool {
        let Some(b) = self.doc.as_ref().and_then(|d| d.babel.as_ref()).filter(|b| b.layout()) else { return false };
        style == b'i' && b.flags & ES_LCROMAN == 0 || matches!(style, b'a' | b'A') && n >= 14
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

impl Machine<'_, '_> {
    /// Whether french is babel's language.
    pub(super) fn babel_french(&self) -> bool {
        self.doc.as_ref().and_then(|d| d.babel.as_ref()).is_some_and(|b| b.main == FRENCH)
    }

    /// french3.ldf's `\ttfamilyFB`, `\rmfamilyFB`, `\sffamilyFB` (`\FBprocess@options`):
    /// the family change `k` (0 roman, 1 sans, 2 typewriter) sets `\ifFB@spacing`.
    pub(super) fn babel_family(&mut self, k: u8) {
        if self.babel_french() {
            self.e.eqtb.set_word(FB_NOSPACING, (k == 2) as i32, false);
        }
    }

    /// french's shorthands `;` `!` `?` (`\FBthinspace`, `\hskip .5\fontdimen2\font`) and `:`
    /// (`\FBcolonspace`, `\space`), french3.ldf's `\declare@shorthand{french}`: with
    /// `\ifFB@spacing` in horizontal mode, `\unskip` a space before (`\ifdim\lastskip>1sp`),
    /// `\penalty\@M` and the space (`\FDP@thinspace`, `\FDP@colonspace`: the same under
    /// `AutoSpaceBeforeFDP`); then `\string<c>`, the character of category 12.
    fn fr_punct(&mut self, c: char) {
        if self.mode == Mode::Horizontal && self.e.eqtb.word(FB_NOSPACING) == 0 {
            if self.last_skip().width > 1 && self.tail().is_some_and(|t| t.kind == Kind::Glue) {
                self.unskip();
            }
            self.cur.push(super::super::engine_node::Node::new(Kind::Penalty, 10000));
            if c == ':' {
                self.append_space();
            } else {
                let w = self.e.fonts.params(self.cur_font())[1];
                let g = super::super::engine_eqtb::Glue { width: crate::backend::scaled::xn_over_d(w, 0x8000, 0x10000), ..Default::default() };
                let n = self.e.nodes.glue_node(g, 0);
                self.cur.push(n);
            }
        }
        self.input.back(Token::char(Cat::Other, c));
    }

    /// `\FBguillspace` (`\hskip .8\fontdimen2\font plus .3\fontdimen3\font minus
    /// .8\fontdimen4\font\relax`) after `\penalty\@M` (`\FB@guillspace`), as tokens.
    fn fr_guill_space(&self, t: &mut String) {
        let p = self.e.fonts.params(self.cur_font());
        let x = |v: i32, f: i32| crate::backend::scaled::xn_over_d(v, f, 0x10000);
        t.push_str(&format!("\\penalty10000\\hskip{}sp plus{}sp minus{}sp\\relax", x(p[1], 52429), x(p[2], 19661), x(p[3], 52429)));
    }

    /// french's `\og` (`\FB@og`, `\@FB@og`): `\leavevmode\guillemotleft\FB@guillspace`; the
    /// T1 `\guillemotleft` is `\char19`.
    fn fr_og(&mut self) {
        if !self.babel_french() || !self.t1() {
            return self.not_native("\\og without babel's french in T1");
        }
        let mut t = String::from("\\char19\\relax");
        self.fr_guill_space(&mut t);
        let mut list = vec![self.doc_id(super::LEAVE)];
        list.extend(self.tex_tokens(&t));
        self.back_list(&list);
    }

    /// french's `\fg` (`\FB@fg\xspace`, `\xspace` `\relax`): `\ifdim\lastskip>\z@\unskip\fi
    /// \FB@guillspace\guillemotright` (`\char20`).
    fn fr_fg(&mut self) {
        if !self.babel_french() || !self.t1() {
            return self.not_native("\\fg without babel's french in T1");
        }
        if self.mode == Mode::Horizontal && self.last_skip().width > 0 && self.tail().is_some_and(|t| t.kind == Kind::Glue) {
            self.unskip();
        }
        let mut t = String::new();
        self.fr_guill_space(&mut t);
        t.push_str("\\char20\\relax\\relax");
        let list = self.tex_tokens(&t);
        self.back_list(&list);
    }

    /// french's `\FB@listVsettings` (`\listFB`, `\let\list\listFB`): after `\@list<depth>`,
    /// `\topsep 0.8ex plus 0.4ex minus 0.4ex`, `\partopsep 0.4ex plus 0.2ex minus 0.2ex`,
    /// `\itemsep` and `\parsep` `0.4ex plus 0.2ex minus 0.2ex`, in the current font; with a
    /// positive `\parskip` they change again (not ported: `false`).
    pub(super) fn fr_list_vsettings(&mut self) -> bool {
        if !self.babel_french() {
            return true;
        }
        if self.e.eqtb.glue(param!("parskip")).width > 0 {
            return false;
        }
        let top = self.glue_of("0.8ex plus 0.4ex minus 0.4ex");
        let small = self.glue_of("0.4ex plus 0.2ex minus 0.2ex");
        for (r, g) in [(super::R_TOPSEP, top), (super::R_PARTOPSEP, small), (super::R_ITEMSEP, small), (super::R_PARSEP, small)] {
            self.set_rg(r, g);
        }
        true
    }
}

/// babel's, installed when it loads natives, in `Meaning::Doc` order after the kernel's (`engine_doc::DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@selectlanguage", |m| m.select_language()),
    ("rt@foreignlanguage", |m| m.foreign_language()),
    ("rt@foreignend", |m| m.foreign_end()),
    ("rt@espercent", |m| m.es_percent()),
    ("rt@esperiod", |m| m.es_period()),
    ("rt@frcolon", |m| m.fr_punct(':')),
    ("rt@frsemicolon", |m| m.fr_punct(';')),
    ("rt@frexclam", |m| m.fr_punct('!')),
    ("rt@frquestion", |m| m.fr_punct('?')),
    ("og", |m| m.fr_og()),
    ("fg", |m| m.fr_fg()),
];
