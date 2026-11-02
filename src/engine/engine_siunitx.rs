//! siunitx (v3, `siunitx.sty` 2026-10-12 3.6.4) on the GDL path, at its default settings:
//! `\num`, `\si`/`\unit`, `\SI`/`\qty`, `\ang`, `\numrange`, `\SIrange`/`\qtyrange`, and the
//! `S[table-format=a.b]` column (`engine_tabular`'s `Cell::SiBegin`/`Cell::SiEnd`).
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
//!   `quantity-product = \,`, `allow-quantity-breaks = false`): one formula. A unit that is
//!   `\degree`, `\arcminute` or `\arcsecond` alone has `quantity-product = { }`
//!   (`\siunitx_unit_options_declare:Nn`, siunitx.sty:7800, applied only to a single token,
//!   `\siunitx_unit_options_apply:n`): no `\,`.
//! - `\degree` is `{}^{\circ}` in math, `\arcminute` `\ensuremath{{}'}`, `\arcsecond`
//!   `\ensuremath{{}''}` (the `\AtBeginDocument` at siunitx.sty:8457, pdfTeX without
//!   unicode-math): `'` is `^{\prime}` (latex.ltx's `\active@math@prime`).
//! - `\ang{<number>}` (`angle-mode = input`, no `;`): the quantity `<number>` `\degree`.
//! - a range (`\numrange`, `\SIrange`/`\qtyrange`, `range-units = repeat`): each number or
//!   quantity its own formula, `range-phrase` between: `\TextOrMath{\space}{\ }`,
//!   `\text{to}` (translations' English "to"), `\TextOrMath{\space}{\ }`; in text `\text`
//!   is `\mbox` (amstext). In a formula, or with babel's main language other than English
//!   (`\GetTranslation`), not native.
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
    /// `\,` inside a literal unit's `\mathrm{...}` (`\protect\,` kept by its
    /// `\protected@edef`): `\mskip\thinmuskip`.
    Thin,
    /// `{}^{\prime…}` with `n` primes: `\arcminute` (1), `\arcsecond` (2).
    Primes(u8),
}

/// One unit: `\mathrm{symbols}` with its power (1 for none), after the inter-unit product
/// `\,` when `sep`.
#[derive(Clone, PartialEq, Debug)]
pub(super) struct Unit {
    pub(super) syms: Vec<Sym>,
    pub(super) power: i8,
    pub(super) sep: bool,
}

/// siunitx's units (siunitx.sty:7362–7432), by name: their symbol text, `O` standing for
/// `\Omega` and `D` for the degree sign. `\kilogram` carries its prefix.
const UNITS: [(&str, &str); 49] = [
    ("ampere", "A"), ("becquerel", "Bq"), ("candela", "cd"), ("celsius", "DC"), ("coulomb", "C"), ("dalton", "Da"), ("day", "d"), ("degreeCelsius", "DC"), ("electronvolt", "eV"), ("farad", "F"),
    ("gram", "g"), ("gray", "Gy"), ("hectare", "ha"), ("henry", "H"), ("hertz", "Hz"), ("hour", "h"), ("joule", "J"), ("katal", "kat"), ("kelvin", "K"), ("kilogram", "kg"),
    ("liter", "L"), ("litre", "L"), ("lumen", "lm"), ("lux", "lx"), ("meter", "m"), ("metre", "m"), ("minute", "min"), ("mole", "mol"), ("newton", "N"), ("ohm", "O"),
    ("pascal", "Pa"), ("radian", "rad"), ("second", "s"), ("siemens", "S"), ("sievert", "Sv"), ("steradian", "sr"), ("tesla", "T"), ("tonne", "t"), ("volt", "V"), ("watt", "W"),
    ("weber", "Wb"), ("bel", "B"), ("decibel", "dB"), ("neper", "Np"), ("astronomicalunit", "au"), ("percent", "%"),
    ("degree", "D"), ("arcminute", "'"), ("arcsecond", "''"),
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
/// (`\siunitx_unit_format:nN`, `per-mode = power`), or a literal unit ([`literal`]);
/// `None` for anything else.
pub(super) fn units(toks: &[St]) -> Option<Vec<Unit>> {
    let toks: Vec<St> = toks.iter().copied().filter(|t| *t != St::Ch(b' ')).collect();
    if toks.is_empty() {
        return None;
    }
    if toks.iter().all(|t| matches!(t, St::Ch(_) | St::Cs(","))) {
        return literal(&toks);
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
        match u.0 {
            "degree" => syms.push(Sym::Degree),
            "arcminute" => syms.push(Sym::Primes(1)),
            "arcsecond" => syms.push(Sym::Primes(2)),
            _ => syms.extend(u.1.bytes().map(|c| match c {
                b'O' if u.0 == "ohm" => Sym::Omega,
                b'D' if u.1 == "DC" => Sym::Degree,
                c => Sym::Ch(c),
            })),
        }
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
        out.push(Unit { syms, power, sep: !out.is_empty() });
    }
    Some(out)
}

/// A literal unit (`\__siunitx_unit_format_literal:n`, siunitx.sty:6458): split at each `.`
/// (the pieces after the first behind the inter-unit product `\,`, `auxi`), each piece's
/// text up to a `^` one `\mathrm{...}` (`\__siunitx_unit_format_literal_add:n`) with the
/// `^{...}` after it (`auxvi`, `super`), spaces dropped (`auxx`). Letters, digits and `\,`
/// in the text, an integer power; `/`, `_`, `~` and empty pieces decline.
fn literal(toks: &[St]) -> Option<Vec<Unit>> {
    let mut out = Vec::new();
    let mut pieces = vec![Vec::new()];
    let mut depth = 0;
    for &t in toks {
        match t {
            St::Ch(b'.') if depth == 0 => pieces.push(Vec::new()),
            St::Ch(b'{') => depth += 1,
            St::Ch(b'}') => depth -= 1,
            _ => {}
        }
        if t != St::Ch(b'.') || depth != 0 {
            pieces.last_mut()?.push(t);
        }
    }
    for (k, piece) in pieces.iter().enumerate() {
        let mut syms = Vec::new();
        let mut sep = k > 0;
        let mut i = 0;
        if piece.is_empty() {
            return None;
        }
        while i < piece.len() {
            match piece[i] {
                St::Ch(b'^') => {
                    // `^{n}` or `^n`: the power of the text before it, never empty.
                    let (text, next) = match piece.get(i + 1)? {
                        St::Ch(b'{') => {
                            let end = i + 2 + piece[i + 2..].iter().position(|t| *t == St::Ch(b'}'))?;
                            (&piece[i + 2..end], end + 1)
                        }
                        t => (std::slice::from_ref(t), i + 2),
                    };
                    let digits: Option<String> = text.iter().map(|t| if let St::Ch(c) = t { Some(*c as char) } else { None }).collect();
                    let power = digits?.parse::<i8>().ok().filter(|&p| p != 1)?;
                    if syms.is_empty() {
                        return None;
                    }
                    out.push(Unit { syms: std::mem::take(&mut syms), power, sep });
                    sep = false;
                    i = next;
                }
                St::Ch(c) if c.is_ascii_alphanumeric() => {
                    syms.push(Sym::Ch(c));
                    i += 1;
                }
                St::Cs(",") => {
                    syms.push(Sym::Thin);
                    i += 1;
                }
                _ => return None,
            }
        }
        if !syms.is_empty() {
            out.push(Unit { syms, power: 1, sep });
        }
    }
    Some(out)
}

/// A scanned number: integer digits, decimal digits, and the exponent with its sign.
type SiNumber = (Vec<u8>, Vec<u8>, Option<(bool, Vec<u8>)>);

/// A number the defaults print with its digits as they are, grouped ([`grouped`]): integer
/// digits (no leading zero but a lone `0`) and, after a `.`, decimal digits.
pub(super) fn number(s: &[u8]) -> Option<(&[u8], &[u8])> {
    let (int, dec) = match s.iter().position(|&c| c == b'.') {
        Some(k) => (&s[..k], &s[k + 1..]),
        None => (s, &[][..]),
    };
    let digits = |p: &[u8]| p.len() <= 18 && p.iter().all(u8::is_ascii_digit);
    let lead = int.len() == 1 || int.first() != Some(&b'0');
    (!int.is_empty() && digits(int) && digits(dec) && lead && (dec.is_empty() == !s.contains(&b'.'))).then_some((int, dec))
}

/// [`sci_number`]'s answer: integer and decimal digits, the exponent's sign and digits.
pub(super) type ExpNumber<'a> = ((&'a [u8], &'a [u8]), Option<(bool, &'a [u8])>);

/// A number with an exponent (`input-exponent-markers` `dDeE`): the mantissa [`number`]
/// takes, then the exponent's sign (`true` for `-`) and digits (no leading zero, not `0`:
/// `print-zero-exponent` is false and the parser drops leading zeros); no marker, no exponent.
pub(super) fn sci_number(s: &[u8]) -> Option<ExpNumber<'_>> {
    let Some(k) = s.iter().position(|c| matches!(c, b'd' | b'D' | b'e' | b'E')) else { return Some((number(s)?, None)) };
    let mant = number(&s[..k])?;
    let (neg, exp) = match s[k + 1..].split_first() {
        Some((b'-', r)) => (true, r),
        _ => (false, &s[k + 1..]),
    };
    let ok = !exp.is_empty() && exp.len() <= 9 && exp.iter().all(u8::is_ascii_digit) && exp[0] != b'0';
    ok.then_some((mant, Some((neg, exp))))
}

/// Whether a unit argument is one of the units whose `quantity-product` is empty
/// (`\degree`, `\arcminute`, `\arcsecond`) as a single token.
pub(super) fn bare_unit(toks: &[St]) -> bool {
    matches!(toks.iter().filter(|t| **t != St::Ch(b' ')).collect::<Vec<_>>()[..], [St::Cs("degree" | "arcminute" | "arcsecond")])
}

/// The pre-scan's view of `\name` (`si`, `unit`, `num`, `SI`, `qty`, `ang`, `numrange`,
/// `SIrange`, `qtyrange`) followed by `rest`: the bytes its arguments take when [`number`]
/// and [`units`] take them (an angle or a range bound without an exponent); `None` for an
/// option, `\SI`'s pre-unit or anything else.
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
    let bounds = match name {
        "num" | "SI" | "qty" | "ang" => 1,
        "numrange" | "SIrange" | "qtyrange" => 2,
        _ => 0,
    };
    for _ in 0..bounds {
        let (num, end) = arg(at)?;
        let digits: Vec<u8> = num.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
        if matches!(name, "num" | "SI" | "qty") {
            sci_number(&digits)?;
        } else {
            number(&digits)?;
        }
        at = end;
    }
    if bounds > 0 {
        if matches!(name, "num" | "ang" | "numrange") {
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
                // A literal unit's braces and `^` ([`literal`]).
                Some(Cat::BeginGroup) => Some(St::Ch(b'{')),
                Some(Cat::EndGroup) => Some(St::Ch(b'}')),
                Some(Cat::Superscript) => Some(St::Ch(b'^')),
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
    pub(super) fn si_math_char(&self, name: &str) -> Option<u32> {
        let id = self.e.cs.get(name)?;
        match self.meaning(Token::cs(id)) {
            Meaning::MathCharDef(n) => Some(n),
            _ => None,
        }
    }

    /// `$` in a horizontal list (§1139) with the format's `\everymath`, `\check@mathfonts`
    /// (`rt@everymath`); `None` when the document changed `\everymath`.
    pub(super) fn si_math_on(&mut self) -> Option<()> {
        let span = self.e.eqtb.toks(param!("everymath"));
        if self.e.toks.get(span) != [self.doc_id(super::EVERY_MATH)] {
            return None;
        }
        self.push_text_math();
        self.check_math_fonts();
        Some(())
    }

    /// Math characters, each a noad (§1155).
    pub(super) fn si_chars(&mut self, s: &[u8]) -> Option<()> {
        for &c in s {
            let code = self.si_math_code(c)?;
            self.set_math_char(code, c);
        }
        Some(())
    }

    /// A number with siunitx's digit groups (`\__siunitx_number_output_digits:nn`,
    /// siunitx.sty:2410): a part of five digits or more (`group-minimum-digits`) in threes,
    /// the integer from the right (`integer_3_3`), the decimals from the marker
    /// (`decimal_3_3`), `group-separator` `\,` between: `\mskip\thinmuskip`.
    fn si_grouped_noads(&mut self, int: &[u8], dec: &[u8]) -> Option<()> {
        // Each group's end: five or more digits in threes (the first group of an integer
        // takes the remainder).
        let ends = |p: &[u8], right: bool| -> Vec<usize> {
            if p.len() < 5 {
                return vec![p.len()];
            }
            let first = if right { (p.len() - 1) % 3 + 1 } else { 3 };
            let mut v: Vec<usize> = (first..p.len()).step_by(3).collect();
            v.push(p.len());
            v
        };
        let digits = |m: &mut Self, p: &[u8], right: bool| -> Option<()> {
            let mut start = 0;
            for (n, end) in ends(p, right).into_iter().enumerate() {
                if n > 0 {
                    let g = m.thin_mu_skip();
                    m.build_mskip(g);
                }
                m.si_chars(&p[start..end])?;
                start = end;
            }
            Some(())
        };
        digits(self, int, true)?;
        if !dec.is_empty() {
            self.si_decimal()?;
            digits(self, dec, false)?;
        }
        Some(())
    }

    /// `\__siunitx_number_output_exponent_auxii:nnnnn` (siunitx.sty:2805) at the defaults:
    /// `exponent-product` `\times` (not `tight-spacing`, so not `\mathord`), the
    /// `exponent-base` `10`, `^{sign digits}`, the digits grouped as an integer. The mantissa
    /// is printed when it is 1 too (`print-unity-mantissa`).
    fn si_exponent(&mut self, neg: bool, x: &[u8]) -> Option<()> {
        let times = self.si_math_char("times")?;
        self.set_math_char(times, 0);
        self.si_chars(b"10")?;
        self.open_sup_group();
        if neg {
            self.si_chars(b"-")?;
        }
        self.si_grouped_noads(x, b"")?;
        self.end_math_group();
        Some(())
    }

    /// The output decimal marker: `.`, or babel's `,` (`engine_babel`'s `babel_decimal_comma`)
    /// set as `{,}` (`\__siunitx_number_output_decimal:`), an ordinary atom whose
    /// sub-formula is boxed (§720).
    fn si_decimal(&mut self) -> Option<()> {
        if !self.babel_decimal_comma() {
            return self.si_chars(b".");
        }
        self.open_ord_group();
        let ok = self.si_chars(b",");
        self.end_math_group();
        ok
    }

    /// Units in the formula being built: `\mathrm{...}` (`{\mathgroup0\relax ...}`, whose
    /// lone ordinary atom loses its braces, §1186), `^{power}`, `\,` between.
    fn si_unit_noads(&mut self, units: &[Unit]) -> Option<()> {
        let (omega, circ) = (self.si_math_char("Omega"), self.si_math_char("circ"));
        for u in units {
            if u.sep {
                let g = self.thin_mu_skip();
                self.build_mskip(g);
            }
            self.open_ord_group();
            self.set_cur_fam(0);
            for s in &u.syms {
                match *s {
                    Sym::Ch(c) => self.si_chars(&[c])?,
                    Sym::Omega => self.set_math_char(omega?, 0),
                    Sym::Thin => {
                        let g = self.thin_mu_skip();
                        self.build_mskip(g);
                    }
                    Sym::Degree => {
                        self.open_ord_group();
                        self.end_math_group();
                        self.open_sup_group();
                        self.set_math_char(circ?, 0);
                        self.end_math_group();
                    }
                    Sym::Primes(n) => {
                        let prime = self.si_math_char("prime")?;
                        self.open_ord_group();
                        self.end_math_group();
                        self.open_sup_group();
                        for _ in 0..n {
                            self.set_math_char(prime, 0);
                        }
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

    /// `\si`/`\unit`, `\num`, `\SI`/`\qty`, `\ang`, `\numrange`, `\SIrange`/`\qtyrange` (`kind`
    /// 0 to 5): `[options]` and `\SI`'s pre-unit decline; outside math,
    /// `\mode_leave_vertical:` and a formula of their own (one per range bound).
    pub(super) fn siunitx_command(&mut self, kind: u8) {
        if self.mode == Mode::Vertical {
            self.leave_vmode();
            return;
        }
        if self.next_is('[') {
            return self.not_native("siunitx options");
        }
        let bounds = match kind {
            1..=3 => 1,
            4 | 5 => 2,
            _ => 0,
        };
        let mut nums = [Vec::new(), Vec::new()];
        let mut unit = Vec::new();
        for n in nums.iter_mut().take(bounds) {
            self.read_arg(n);
        }
        if kind == 2 && self.next_is('[') {
            return self.not_native("\\SI's pre-unit");
        }
        if matches!(kind, 0 | 2 | 5) {
            self.read_arg(&mut unit);
        }
        let mut ns = Vec::new();
        for num in nums.iter().take(bounds) {
            let s: Option<Vec<u8>> = num.iter().filter(|t| t.cat() != Some(Cat::Space)).map(|t| (t.cs_id().is_none() && (t.cat() == Some(Cat::Other) || t.cat() == Some(Cat::Letter) && matches!(t.chr(), 'd' | 'D' | 'e' | 'E'))).then(|| t.chr() as u8)).collect();
            match s.as_deref().and_then(sci_number) {
                // An angle or a range bound takes no exponent ([`scan_args`]).
                Some((_, Some(_))) if kind >= 3 => return self.not_native("a siunitx angle or range with an exponent"),
                Some(((i, d), e)) => ns.push((i.to_vec(), d.to_vec(), e.map(|(neg, x)| (neg, x.to_vec())))),
                None => return self.not_native("a siunitx number the native path does not take"),
            }
        }
        let (u, bare) = match kind {
            0 | 2 | 5 => match st_of(self, &unit).and_then(|s| Some((units(&s)?, bare_unit(&s)))) {
                Some(u) => (Some(u.0), u.1),
                None => return self.not_native("a siunitx unit the native path does not take"),
            },
            3 => (Some(vec![Unit { syms: vec![Sym::Degree], power: 1, sep: false }]), true),
            _ => (None, false),
        };
        let math = self.mode == Mode::Math;
        if math && !matches!(self.cur_fam(), -1 | 0) {
            return self.not_native("siunitx in a math alphabet");
        }
        if bounds == 2 && (math || self.babel_date().is_some_and(|k| k != super::babel::ENGLISH)) {
            return self.not_native("a siunitx range in a formula or another language");
        }
        let one = |m: &mut Self, n: Option<&SiNumber>| -> Option<()> {
            if !math {
                m.si_math_on()?;
            }
            if let Some((i, d, e)) = n {
                m.si_grouped_noads(i, d)?;
                if let Some((neg, x)) = e {
                    m.si_exponent(*neg, x)?;
                }
            }
            if let Some(u) = &u {
                if n.is_some() {
                    // `\nobreak\,`, or `\nobreak` alone for a bare unit.
                    m.cur.push(Node::new(Kind::Penalty, 10000));
                    if !bare {
                        let g = m.thin_mu_skip();
                        m.build_mskip(g);
                    }
                }
                m.si_unit_noads(u)?;
            }
            if !math {
                m.after_math();
            }
            Some(())
        };
        let ok = if bounds == 2 {
            one(self, ns.first()).and_then(|_| {
                // `range-phrase`: `\space\text{to}\space` in text.
                self.build_text(b" ");
                self.build_hbox(|m| m.build_text(b"to"));
                self.build_text(b" ");
                one(self, ns.get(1))
            })
        } else {
            one(self, ns.first())
        };
        if ok.is_none() {
            self.not_native("a siunitx symbol the native path does not take");
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
        let ok = self.si_chars(int).and_then(|_| if dec.is_empty() { Some(()) } else { self.si_decimal().and_then(|_| self.si_chars(dec)) });
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
        assert_eq!(number(b"0.04914"), Some((&b"0"[..], &b"04914"[..])));
        for bad in [&b".5"[..], b"2.", b"007", b"-1", b"1e3", b"1,5", b""] {
            assert_eq!(number(bad), None);
        }
        assert_eq!(sci_number(b"2.998e8"), Some(((&b"2"[..], &b"998"[..]), Some((false, &b"8"[..])))));
        assert_eq!(sci_number(b"1e-3"), Some(((&b"1"[..], &b""[..]), Some((true, &b"3"[..])))));
        assert_eq!(sci_number(b"6.02d23"), Some(((&b"6"[..], &b"02"[..]), Some((false, &b"23"[..])))));
        assert_eq!(sci_number(b"18.4"), Some(((&b"18"[..], &b"4"[..]), None)));
        for bad in [&b"1e+3"[..], b"1e03", b"1e", b"e3", b"1e-", b"1e3e4", b"1e0", b"1e-0"] {
            assert_eq!(sci_number(bad), None);
        }
    }

    #[test]
    fn units_follow_per_mode_power() {
        let u = units(&st_tokens(r"\meter\per\second\squared").unwrap()).unwrap();
        assert_eq!(u, vec![Unit { syms: vec![Sym::Ch(b'm')], power: 1, sep: false }, Unit { syms: vec![Sym::Ch(b's')], power: -2, sep: true }]);
        let u = units(&st_tokens(r"\kilo\watt\hour").unwrap()).unwrap();
        assert_eq!(u[0].syms, vec![Sym::Ch(b'k'), Sym::Ch(b'W')]);
        assert_eq!(units(&st_tokens(r"\ohm\per\degreeCelsius").unwrap()).unwrap()[1], Unit { syms: vec![Sym::Degree, Sym::Ch(b'C')], power: -1, sep: true });
        assert_eq!(units(&st_tokens("mL").unwrap()).unwrap(), vec![Unit { syms: vec![Sym::Ch(b'm'), Sym::Ch(b'L')], power: 1, sep: false }]);
        let ch = |s: &str| s.bytes().map(Sym::Ch).collect::<Vec<_>>();
        assert_eq!(units(&st_tokens("mol.L^{-1}").unwrap()).unwrap(), vec![Unit { syms: ch("mol"), power: 1, sep: false }, Unit { syms: ch("L"), power: -1, sep: true }]);
        assert_eq!(units(&st_tokens(r"mol\,L^{-1}").unwrap()).unwrap(), vec![Unit { syms: [ch("mol"), vec![Sym::Thin], ch("L")].concat(), power: -1, sep: false }]);
        assert_eq!(units(&st_tokens("m^2s").unwrap()).unwrap(), vec![Unit { syms: ch("m"), power: 2, sep: false }, Unit { syms: ch("s"), power: 1, sep: false }]);
        assert!(units(&st_tokens("mol.").unwrap()).is_none());
        assert!(units(&st_tokens("^2").unwrap()).is_none());
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
        assert_eq!(scan_args("num", "{2.998e8}"), Some(9));
        assert_eq!(scan_args("num", "{1e08}"), None);
        assert_eq!(scan_args("ang", "{12.5} x"), Some(6));
        assert_eq!(scan_args("ang", "{1;2;3}"), None);
        assert_eq!(scan_args("ang", "{1e3}"), None);
        assert_eq!(scan_args("numrange", "{1}{2} x"), Some(6));
        assert_eq!(scan_args("SIrange", r"{0}{3.6}{\meter\per\second}."), Some(27));
        assert_eq!(scan_args("qtyrange", r"{1}{2}[x]"), None);
        assert!(bare_unit(&st_tokens(r" \degree ").unwrap()));
        assert!(!bare_unit(&st_tokens(r"\degree\per\second").unwrap()));
        assert_eq!(units(&st_tokens(r"\arcsecond").unwrap()).unwrap()[0].syms, vec![Sym::Primes(2)]);
        assert_eq!(sci_number(b"3.5E-4"), Some(((&b"3"[..], &b"5"[..]), Some((true, &b"4"[..])))));
        assert_eq!(si_format("[table-format=2.1] S"), Some((21, 18)));
        assert_eq!(si_format("[ table-format = 3.0 ]"), Some((30, 22)));
        assert_eq!(si_format("[table-format=+2.1]"), None);
        assert_eq!(si_format("{x}"), None);
    }
}
