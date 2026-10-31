//! siunitx (v3, `siunitx.sty` 2026-10-12 3.6.4) on the GDL path, at its default settings:
//! `\num`, `\si`/`\unit`, `\SI`/`\qty`, and the `S[table-format=a.b]` column
//! (`engine_tabular`'s `Cell::SiBegin`/`Cell::SiEnd`).
//!
//! What the defaults print (`\keys_set:nn { siunitx }` at siunitx.sty:5205, 7501, 7797):
//! - `mode = math`: a number or unit outside math is `$…$` (`\__siunitx_print_math_ensure_aux:n`),
//!   since `\fam` is -1 there (`\__siunitx_print_math_auxv:n`); in math with `\fam` -1 or
//!   `\symoperators` it is added to the formula as it is. `\MathMLintent{:unit}{#1}` is `#1`
//!   (latex.ltx).
//! - a number: its digits and the decimal marker `.` as math characters (`.` is `"013A`).
//!   Digit grouping starts at five digits (`group-minimum-digits = 5`): longer parts decline.
//! - a unit: each `\mathrm{<prefix><symbol>}` (`unit-font-command`), a power as `^{n}`
//!   outside it (`per-mode = power`: `\per` is the power -1), units joined by `\,`
//!   (`inter-unit-product`). `\degreeCelsius` is `{}^{\circ}C` (siunitx.sty:8478, pdfTeX
//!   without unicode-math), `\ohm` `\Omega`.
//! - a quantity: `\ensuremath{<number>\nobreak\,<unit>}` (`\siunitx_quantity_print:nn`,
//!   `quantity-product = \,`, `allow-quantity-breaks = false`): one formula.
//!
//! Options (`[...]`, `\sisetup`), signs, exponents, uncertainties, `\SI`'s pre-unit and
//! units outside the table below decline.

use super::super::engine_eqtb::Code;
use super::super::engine_node::{Kind, Node};
use super::super::engine_prim::Meaning;
use super::super::engine_run::{Machine, Mode};
use super::super::engine_token::Token;
use super::super::engine_catcode::Cat;
use super::tabular::St;

/// A symbol of a unit: a character, `\Omega` or `\circ` (`{}^{\circ}`, the empty group
/// with its superscript).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Sym {
    Ch(u8),
    Omega,
    Degree,
}

/// One unit: `\mathrm{symbols}` with its power (1 for none).
#[derive(Clone, PartialEq, Debug)]
pub(super) struct Unit {
    pub(super) syms: Vec<Sym>,
    pub(super) power: i8,
}

/// siunitx's units (siunitx.sty:7362–7432), by name: their symbol text, `O` standing for
/// `\Omega` and `D` for the degree sign. `\kilogram` carries its prefix.
const UNITS: [(&str, &str); 45] = [
    ("ampere", "A"), ("becquerel", "Bq"), ("candela", "cd"), ("celsius", "DC"), ("coulomb", "C"), ("dalton", "Da"), ("day", "d"), ("degreeCelsius", "DC"), ("electronvolt", "eV"), ("farad", "F"),
    ("gram", "g"), ("gray", "Gy"), ("hectare", "ha"), ("henry", "H"), ("hertz", "Hz"), ("hour", "h"), ("joule", "J"), ("katal", "kat"), ("kelvin", "K"), ("kilogram", "kg"),
    ("liter", "L"), ("litre", "L"), ("lumen", "lm"), ("lux", "lx"), ("meter", "m"), ("metre", "m"), ("minute", "min"), ("mole", "mol"), ("newton", "N"), ("ohm", "O"),
    ("pascal", "Pa"), ("radian", "rad"), ("second", "s"), ("siemens", "S"), ("sievert", "Sv"), ("steradian", "sr"), ("tesla", "T"), ("tonne", "t"), ("volt", "V"), ("watt", "W"),
    ("weber", "Wb"), ("bel", "B"), ("decibel", "dB"), ("neper", "Np"), ("astronomicalunit", "au"),
];

/// siunitx's prefixes but `\micro` (a non-Latin symbol), siunitx.sty:7371–7395.
const PREFIXES: [(&str, &str); 24] = [
    ("quecto", "q"), ("ronto", "r"), ("yocto", "y"), ("zepto", "z"), ("atto", "a"), ("femto", "f"), ("pico", "p"), ("nano", "n"), ("milli", "m"), ("centi", "c"), ("deci", "d"), ("deca", "da"),
    ("deka", "da"), ("hecto", "h"), ("kilo", "k"), ("mega", "M"), ("giga", "G"), ("tera", "T"), ("peta", "P"), ("exa", "E"), ("zetta", "Z"), ("yotta", "Y"), ("ronna", "R"), ("quetta", "Q"),
];

/// Whether `name` is a unit, prefix or power macro of a unit argument: [`predict`] passes it.
pub(super) fn is_unit_name(name: &str) -> bool {
    UNITS.iter().any(|u| u.0 == name) || PREFIXES.iter().any(|p| p.0 == name) || matches!(name, "per" | "squared" | "cubed" | "square" | "cubic")
}

/// A unit argument (`\TrimSpaces`'d): unit macros with their prefixes, `\per` and powers
/// (`\siunitx_unit_format:nN`, `per-mode = power`), or literal letters (one
/// `\mathrm{...}`); `None` for anything else.
pub(super) fn units(toks: &[St]) -> Option<Vec<Unit>> {
    let toks: Vec<St> = toks.iter().copied().filter(|t| *t != St::Ch(b' ')).collect();
    if toks.is_empty() {
        return None;
    }
    if toks.iter().all(|t| matches!(t, St::Ch(c) if c.is_ascii_alphabetic())) {
        let syms = toks.iter().map(|t| if let St::Ch(c) = t { Sym::Ch(*c) } else { Sym::Ch(b'?') }).collect();
        return Some(vec![Unit { syms, power: 1 }]);
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let cs = |k: usize| match toks.get(k) {
            Some(St::Cs(n)) => Some(*n),
            _ => None,
        };
        let mut power: i8 = 1;
        if cs(i) == Some("per") {
            power = -1;
            i += 1;
        }
        match cs(i) {
            Some("square") => {
                power *= 2;
                i += 1;
            }
            Some("cubic") => {
                power *= 3;
                i += 1;
            }
            _ => {}
        }
        let mut syms = Vec::new();
        if let Some(p) = cs(i).and_then(|n| PREFIXES.iter().find(|p| p.0 == n)) {
            syms.extend(p.1.bytes().map(Sym::Ch));
            i += 1;
        }
        let u = cs(i).and_then(|n| UNITS.iter().find(|u| u.0 == n))?;
        if u.0 == "kilogram" && !syms.is_empty() {
            return None;
        }
        syms.extend(u.1.bytes().map(|c| match c {
            b'O' if u.0 == "ohm" => Sym::Omega,
            b'D' if u.1 == "DC" => Sym::Degree,
            c => Sym::Ch(c),
        }));
        i += 1;
        match cs(i) {
            Some("squared") => {
                power *= 2;
                i += 1;
            }
            Some("cubed") => {
                power *= 3;
                i += 1;
            }
            _ => {}
        }
        out.push(Unit { syms, power });
    }
    Some(out)
}

/// A number as the defaults print it unchanged: integer digits (no leading zero but a lone
/// `0`) and, after a `.`, decimal digits, each part under five digits (`group-minimum-digits`).
pub(super) fn number(s: &[u8]) -> Option<(&[u8], &[u8])> {
    let (int, dec) = match s.iter().position(|&c| c == b'.') {
        Some(k) => (&s[..k], &s[k + 1..]),
        None => (s, &[][..]),
    };
    let digits = |p: &[u8]| p.len() <= 4 && p.iter().all(u8::is_ascii_digit);
    let lead = int.len() == 1 || int.first() != Some(&b'0');
    (!int.is_empty() && digits(int) && digits(dec) && lead && (dec.is_empty() == !s.contains(&b'.'))).then_some((int, dec))
}

/// The pre-scan's view of `\name` (`si`, `unit`, `num`, `SI`, `qty`) followed by `rest`: the
/// bytes its arguments take when [`number`] and [`units`] take them; `None` for an option,
/// `\SI`'s pre-unit or anything else.
pub(super) fn scan_args(name: &str, rest: &str) -> Option<usize> {
    let arg = |at: usize| -> Option<(&str, usize)> {
        let r = &rest[at..];
        let t = r.trim_start();
        let body = t.strip_prefix('{')?;
        let k = super::balanced(body)?;
        Some((&body[..k], at + (r.len() - t.len()) + 1 + k + 1))
    };
    if rest.trim_start().starts_with('[') {
        return None;
    }
    let mut at = 0;
    if matches!(name, "num" | "SI" | "qty") {
        let (num, end) = arg(at)?;
        let digits: Vec<u8> = num.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
        number(&digits)?;
        at = end;
        if name == "num" {
            return Some(at);
        }
        if rest[at..].trim_start().starts_with('[') {
            return None;
        }
    }
    let (unit, end) = arg(at)?;
    units(&super::tabular::st_tokens(unit)?)?;
    Some(end)
}

/// The tokens of an argument as [`St`]: control words by name, characters by code; `None`
/// for a character past ASCII or of a category the arguments here never hold.
fn st_of<'a>(m: &'a Machine, toks: &[Token]) -> Option<Vec<St<'a>>> {
    toks.iter()
        .map(|t| match t.cs_id() {
            Some(id) => Some(St::Cs(m.e.cs.name(id))),
            None => match t.cat() {
                Some(Cat::Space) => Some(St::Ch(b' ')),
                Some(Cat::Letter | Cat::Other) => u8::try_from(t.chr() as u32).ok().filter(u8::is_ascii).map(St::Ch),
                _ => None,
            },
        })
        .collect()
}

impl Machine<'_, '_> {
    /// The math code of character `c`; `None` for one that acts as active (`"8000`).
    fn si_math_code(&self, c: u8) -> Option<u32> {
        let code = self.e.eqtb.code(Code::Math, c as u32) as u32;
        (code < 0x8000).then_some(code)
    }

    /// The math character `\name` was given by `\mathchardef` (`\Omega`, `\circ`).
    fn si_math_char(&self, name: &str) -> Option<u32> {
        let id = self.e.cs.get(name)?;
        match self.meaning(Token::cs(id)) {
            Meaning::MathCharDef(n) => Some(n),
            _ => None,
        }
    }

    /// `$` in a horizontal list (§1139) with the format's `\everymath`, `\check@mathfonts`
    /// (`rt@everymath`); `None` when the document changed `\everymath`.
    fn si_math_on(&mut self) -> Option<()> {
        let span = self.e.eqtb.toks(param!("everymath"));
        if self.e.toks.get(span) != [self.doc_id(super::EVERY_MATH)] {
            return None;
        }
        self.push_text_math();
        self.check_math_fonts();
        Some(())
    }

    /// Math characters, each a noad (§1155).
    fn si_chars(&mut self, s: &[u8]) -> Option<()> {
        for &c in s {
            let code = self.si_math_code(c)?;
            self.set_math_char(code, c);
        }
        Some(())
    }

    /// A number's digits and marker in the formula being built.
    fn si_number_noads(&mut self, int: &[u8], dec: &[u8]) -> Option<()> {
        self.si_chars(int)?;
        if !dec.is_empty() {
            self.si_chars(b".")?;
            self.si_chars(dec)?;
        }
        Some(())
    }

    /// Units in the formula being built: `\mathrm{...}` (`{\mathgroup0\relax ...}`, whose
    /// lone ordinary atom loses its braces, §1186), `^{power}`, `\,` between.
    fn si_unit_noads(&mut self, units: &[Unit]) -> Option<()> {
        let (omega, circ) = (self.si_math_char("Omega"), self.si_math_char("circ"));
        for (k, u) in units.iter().enumerate() {
            if k > 0 {
                let g = self.thin_mu_skip();
                self.build_mskip(g);
            }
            self.open_ord_group();
            self.set_cur_fam(0);
            for s in &u.syms {
                match *s {
                    Sym::Ch(c) => self.si_chars(&[c])?,
                    Sym::Omega => self.set_math_char(omega?, 0),
                    Sym::Degree => {
                        self.open_ord_group();
                        self.end_math_group();
                        self.open_sup_group();
                        self.set_math_char(circ?, 0);
                        self.end_math_group();
                    }
                }
            }
            self.end_math_group();
            if u.power != 1 {
                self.open_sup_group();
                self.si_chars(u.power.to_string().as_bytes())?;
                self.end_math_group();
            }
        }
        Some(())
    }

    /// `\si`/`\unit`, `\num`, `\SI`/`\qty` (`kind` 0, 1, 2): `[options]` and `\SI`'s pre-unit
    /// decline; outside math, `\mode_leave_vertical:` and a formula of their own.
    pub(super) fn siunitx_command(&mut self, kind: u8) {
        if self.mode == Mode::Vertical {
            self.leave_vmode();
            return;
        }
        if self.next_is('[') {
            return self.not_native("siunitx options");
        }
        let (mut num, mut unit) = (Vec::new(), Vec::new());
        if kind >= 1 {
            self.read_arg(&mut num);
        }
        if kind == 2 && self.next_is('[') {
            return self.not_native("\\SI's pre-unit");
        }
        if kind != 1 {
            self.read_arg(&mut unit);
        }
        let n = if kind >= 1 {
            let s: Option<Vec<u8>> = num.iter().filter(|t| t.cat() != Some(Cat::Space)).map(|t| (t.cs_id().is_none() && t.cat() == Some(Cat::Other)).then(|| t.chr() as u8)).collect();
            match s.as_deref().and_then(number) {
                Some((i, d)) => Some((i.to_vec(), d.to_vec())),
                None => return self.not_native("a siunitx number the native path does not take"),
            }
        } else {
            None
        };
        let u = if kind != 1 {
            match st_of(self, &unit).and_then(|s| units(&s)) {
                Some(u) => Some(u),
                None => return self.not_native("a siunitx unit the native path does not take"),
            }
        } else {
            None
        };
        let math = self.mode == Mode::Math;
        if math && !matches!(self.cur_fam(), -1 | 0) {
            return self.not_native("siunitx in a math alphabet");
        }
        if !math && self.si_math_on().is_none() {
            return self.not_native("siunitx with \\everymath");
        }
        let ok = (|| {
            if let Some((i, d)) = &n {
                self.si_number_noads(i, d)?;
            }
            if let Some(u) = &u {
                if n.is_some() {
                    // `\nobreak\,`.
                    self.cur.push(Node::new(Kind::Penalty, 10000));
                    let g = self.thin_mu_skip();
                    self.build_mskip(g);
                }
                self.si_unit_noads(u)?;
            }
            Some(())
        })();
        if ok.is_none() {
            return self.not_native("a siunitx symbol the native path does not take");
        }
        if !math {
            self.after_math();
        }
    }

    /// `\hbox{$<chars>$}`'s width (a table model, `\__siunitx_table_print_model_box:Nn`).
    pub(super) fn si_width(&mut self, int: &[u8], dec: &[u8]) -> Option<i32> {
        let start = self.cur.len();
        self.si_math_box(int, dec)?;
        let n = self.cur.pop()?;
        debug_assert_eq!(self.cur.len(), start);
        Some(self.e.nodes.boxes[n.a as usize].width)
    }

    /// `$int.dec$` (`\siunitx_print_number:n` outside math), nothing with no digits.
    pub(super) fn si_formula(&mut self, int: &[u8], dec: &[u8]) -> Option<()> {
        if int.is_empty() && dec.is_empty() {
            return Some(());
        }
        self.si_math_on()?;
        let ok = self.si_chars(int).and_then(|_| if dec.is_empty() { Some(()) } else { self.si_chars(b".").and_then(|_| self.si_chars(dec)) });
        self.after_math();
        ok
    }

    /// `\hbox{$int.dec$}` in the current list, empty with no digits.
    pub(super) fn si_math_box(&mut self, int: &[u8], dec: &[u8]) -> Option<()> {
        let mut ok = Some(());
        self.build_hbox(|m| ok = m.si_formula(int, dec));
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::super::tabular::{si_format, st_tokens};
    use super::*;

    #[test]
    fn numbers_take_the_default_forms() {
        assert_eq!(number(b"18.4"), Some((&b"18"[..], &b"4"[..])));
        assert_eq!(number(b"0.040"), Some((&b"0"[..], &b"040"[..])));
        assert_eq!(number(b"100"), Some((&b"100"[..], &b""[..])));
        for bad in [&b"12345"[..], b".5", b"2.", b"007", b"-1", b"1e3", b"1,5", b""] {
            assert_eq!(number(bad), None);
        }
    }

    #[test]
    fn units_follow_per_mode_power() {
        let u = units(&st_tokens(r"\meter\per\second\squared").unwrap()).unwrap();
        assert_eq!(u, vec![Unit { syms: vec![Sym::Ch(b'm')], power: 1 }, Unit { syms: vec![Sym::Ch(b's')], power: -2 }]);
        let u = units(&st_tokens(r"\kilo\watt\hour").unwrap()).unwrap();
        assert_eq!(u[0].syms, vec![Sym::Ch(b'k'), Sym::Ch(b'W')]);
        assert_eq!(units(&st_tokens(r"\ohm\per\degreeCelsius").unwrap()).unwrap()[1], Unit { syms: vec![Sym::Degree, Sym::Ch(b'C')], power: -1 });
        assert_eq!(units(&st_tokens("mL").unwrap()).unwrap(), vec![Unit { syms: vec![Sym::Ch(b'm'), Sym::Ch(b'L')], power: 1 }]);
        assert!(units(&st_tokens(r"\micro\meter").unwrap()).is_none());
        assert!(units(&st_tokens(r"\kilo\kilogram").unwrap()).is_none());
        assert!(units(&st_tokens("m/s").unwrap()).is_none());
    }

    #[test]
    fn the_prescan_takes_the_arguments() {
        assert_eq!(scan_args("SI", "{0.5}{m} x"), Some(8));
        assert_eq!(scan_args("si", r" {\meter} x"), Some(9));
        assert_eq!(scan_args("num", "{12} x"), Some(4));
        assert_eq!(scan_args("si", "[mode=text]{m}"), None);
        assert_eq!(scan_args("SI", r"{1}[\$]{m}"), None);
        assert_eq!(si_format("[table-format=2.1] S"), Some((21, 18)));
        assert_eq!(si_format("[ table-format = 3.0 ]"), Some((30, 22)));
        assert_eq!(si_format("[table-format=+2.1]"), None);
        assert_eq!(si_format("{x}"), None);
    }
}
