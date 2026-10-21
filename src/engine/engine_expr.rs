//! e-TeX's expressions: `\numexpr`, `\dimexpr`, `\glueexpr`, `\muexpr` (pdftex.web's
//! `scan_expr`), in our own form: a recursive descent where TeX keeps an explicit stack, with
//! TeX's rounding (`quotient`, `fract`) and range checks, so every result is TeX's.

use super::engine_catcode::Cat;
use super::engine_eqtb::Glue;
use super::engine_prim::{Meaning, Prim};
use super::engine_run::Machine;
use super::engine_token::Token;
use super::engine_value::{Value, MAX_DIMEN};

/// What an expression computes (e-TeX's `cur_val_level`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Int,
    Dimen,
    Glue,
    Mu,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    None,
    Add,
    Sub,
    Mult,
    Div,
    Scale,
}

const INFINITY: i64 = i32::MAX as i64;

/// `quotient(n, d)`: `n / d` rounded half away from zero; `None` for `d = 0`.
fn quotient(n: i64, d: i64) -> Option<i64> {
    if d == 0 {
        return None;
    }
    let negative = (n < 0) != (d < 0);
    let (n, d) = (n.abs(), d.abs());
    let mut a = n / d;
    if 2 * (n - a * d) >= d {
        a += 1;
    }
    Some(if negative { -a } else { a })
}

/// `fract(x, n, d, max)`: `x * n / d` rounded half away from zero, exactly; `None` when `d`
/// is 0 or the result exceeds `max`.
fn fract(x: i64, n: i64, d: i64, max: i64) -> Option<i64> {
    if d == 0 {
        return None;
    }
    let negative = ((x < 0) != (n < 0)) != (d < 0);
    let (x, n, d) = (x.unsigned_abs() as i128, n.unsigned_abs() as i128, d.unsigned_abs() as i128);
    let a = (2 * x * n + d) / (2 * d);
    if a > max as i128 {
        return None;
    }
    let a = a as i64;
    Some(if negative { -a } else { a })
}

/// `add_or_sub(x, y, max, negative)`.
fn add_or_sub(x: i64, y: i64, max: i64, negative: bool) -> Option<i64> {
    let a = if negative { x - y } else { x + y };
    (a.abs() <= max).then_some(a)
}

fn normalize(mut g: Glue) -> Glue {
    if g.stretch == 0 {
        g.stretch_order = 0;
    }
    if g.shrink == 0 {
        g.shrink_order = 0;
    }
    g
}

/// A value of any level, as wide integers while it is computed.
#[derive(Clone, Copy, Debug, Default)]
struct V {
    w: i64,
    st: i64,
    sh: i64,
    sto: u8,
    sho: u8,
}

impl V {
    fn of(g: Glue) -> V {
        V { w: g.width as i64, st: g.stretch as i64, sh: g.shrink as i64, sto: g.stretch_order, sho: g.shrink_order }
    }

    fn glue(self) -> Glue {
        normalize(Glue { width: self.w as i32, stretch: self.st as i32, shrink: self.sh as i32, stretch_order: self.sto, shrink_order: self.sho })
    }

    /// Applies `f` to each component of a glue, or to the word alone.
    fn map(self, glue: bool, f: impl Fn(i64) -> Option<i64>) -> Option<V> {
        Some(if glue { V { w: f(self.w)?, st: f(self.st)?, sh: f(self.sh)?, ..self } } else { V { w: f(self.w)?, ..self } })
    }
}

impl Machine<'_, '_> {
    /// `scan_expr` for `\numexpr` (`Int`), `\dimexpr` (`Dimen`), `\glueexpr` (`Glue`) and
    /// `\muexpr` (`Mu`): the expression up to a token that cannot continue it (an optional `\relax`
    /// ending it is consumed). Overflow is TeX's "Arithmetic overflow" error and a zero result.
    pub(super) fn scan_expr(&mut self, level: Level) -> Value {
        let v = match self.expr(level, false) {
            Some(v) => v,
            None => {
                self.error("Arithmetic overflow");
                V::default()
            }
        };
        match level {
            Level::Int => Value::Int(v.w as i32),
            Level::Dimen => Value::Dimen(v.w as i32),
            Level::Glue => Value::Glue(v.glue()),
            Level::Mu => Value::Mu(v.glue()),
        }
    }

    /// One (sub)expression; `nested` when it began with `(` and ends at `)`.
    fn expr(&mut self, l: Level, nested: bool) -> Option<V> {
        let glue = matches!(l, Level::Glue | Level::Mu);
        let max = if l == Level::Int { INFINITY } else { MAX_DIMEN as i64 };
        let (mut r, mut e) = (Op::None, V::default());
        let (mut s, mut t, mut n) = (Op::None, V::default(), 0i64);
        let mut ok = true;
        loop {
            // A factor: of the expression's level first, then integers after `*` or `/`.
            let factor_level = if s == Op::None { l } else { Level::Int };
            let x = self.next_non_blank()?;
            let f = if x.t == Token::char(Cat::Other, '(') {
                match self.expr(factor_level, true) {
                    Some(v) => v,
                    None => {
                        ok = false;
                        V::default()
                    }
                }
            } else {
                self.input.back(x.t);
                match factor_level {
                    Level::Int => V { w: self.scan_int() as i64, ..V::default() },
                    Level::Dimen => V { w: self.scan_dimen(false).0 as i64, ..V::default() },
                    Level::Glue | Level::Mu => V::of(self.scan_glue(factor_level == Level::Mu)),
                }
            };
            // The next operator.
            let mut o = Op::None;
            if let Some(y) = self.next_non_blank() {
                o = match y.t {
                    t if t == Token::char(Cat::Other, '+') => Op::Add,
                    t if t == Token::char(Cat::Other, '-') => Op::Sub,
                    t if t == Token::char(Cat::Other, '*') => Op::Mult,
                    t if t == Token::char(Cat::Other, '/') => Op::Div,
                    _ => {
                        if !nested {
                            if y.m != Meaning::Prim(Prim::Relax) {
                                self.input.back(y.t);
                            }
                        } else if y.t != Token::char(Cat::Other, ')') {
                            self.error("Missing ) inserted for expression");
                            self.input.back(y.t);
                        }
                        Op::None
                    }
                };
            }
            // Range of the factor.
            let fmax = if l == Level::Int || s > Op::Sub { INFINITY } else { MAX_DIMEN as i64 };
            let f = if f.w.abs() > fmax || (glue && s == Op::None && (f.st.abs() > fmax || f.sh.abs() > fmax)) {
                ok = false;
                V::default()
            } else {
                f
            };
            // Apply the factor to the term.
            let res = match s {
                Op::None => Some(f),
                Op::Mult if o == Op::Div => {
                    n = f.w;
                    o = Op::Scale;
                    Some(t)
                }
                Op::Mult => t.map(glue, |c| {
                    let p = c * f.w;
                    (p.abs() <= max).then_some(p)
                }),
                Op::Div => t.map(glue, |c| quotient(c, f.w)),
                Op::Scale => t.map(glue, |c| fract(c, n, f.w, max)),
                _ => Some(t),
            };
            t = match res {
                Some(v) => v,
                None => {
                    ok = false;
                    V::default()
                }
            };
            if o > Op::Sub {
                s = o;
            } else {
                // The term is complete: into the expression.
                s = Op::None;
                if r == Op::None {
                    e = t;
                } else {
                    let neg = r == Op::Sub;
                    let sum = |a: i64, b: i64| add_or_sub(a, b, max, neg);
                    let w = sum(e.w, t.w);
                    let mut next = V { w: w.unwrap_or(0), ..e };
                    ok &= w.is_some();
                    if glue {
                        let tst = if neg { -t.st } else { t.st };
                        let tsh = if neg { -t.sh } else { t.sh };
                        if e.sto == t.sto {
                            next.st = e.st + tst;
                        } else if e.sto < t.sto && t.st != 0 {
                            next.st = tst;
                            next.sto = t.sto;
                        }
                        if e.sho == t.sho {
                            next.sh = e.sh + tsh;
                        } else if e.sho < t.sho && t.sh != 0 {
                            next.sh = tsh;
                            next.sho = t.sho;
                        }
                        ok &= next.st.abs() <= max && next.sh.abs() <= max;
                        next = V::of(next.glue());
                    }
                    e = next;
                }
                r = o;
            }
            if o == Op::None {
                return ok.then_some(e);
            }
        }
    }
}

impl PartialOrd for Op {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some((*self as u8).cmp(&(*other as u8)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding() {
        assert_eq!(quotient(7, 2), Some(4));
        assert_eq!(quotient(-7, 2), Some(-4));
        assert_eq!(quotient(5, 3), Some(2));
        assert_eq!(quotient(1, 0), None);
        assert_eq!(fract(7, 11, 3, INFINITY), Some(26));
        assert_eq!(fract(-7, 11, 3, INFINITY), Some(-26));
        assert_eq!(fract(i32::MAX as i64, 2, 1, INFINITY), None);
    }

    fn ok(src: &str) -> String {
        let mut e = super::super::Engine::default();
        let mut m = Machine::new(&mut e, src);
        m.run();
        assert!(m.errors.is_empty(), "{:?}", m.errors);
        m.out_text()
    }

    #[test]
    fn expressions_follow_etex() {
        assert_eq!(ok("\\the\\numexpr 7*11/3\\relax"), "26");
        assert_eq!(ok("\\the\\numexpr -7/2\\relax"), "-4");
        assert_eq!(ok("\\the\\numexpr (1+2)*3\\relax"), "9");
        assert_eq!(ok("\\the\\numexpr 2*(3+4)-1 x"), "13x ");
        assert_eq!(ok("\\the\\dimexpr 1pt*3/2\\relax"), "1.5pt");
        assert_eq!(ok("\\the\\glueexpr 1pt plus 2fil + 3pt plus 1fill\\relax"), "4.0pt plus 1.0fill");
        assert_eq!(ok("\\count1=\\numexpr\\count1+5\\relax\\the\\count1"), "5");
    }

    #[test]
    fn meaning_and_strings() {
        assert_eq!(ok("\\def\\a#1{x#1}\\meaning\\a"), "macro:#1->x#1");
        assert_eq!(ok("\\long\\def\\a{}\\meaning\\a"), "\\long macro:->");
        assert_eq!(ok("\\meaning a"), "the letter a ");
        assert_eq!(ok("\\detokenize{\\a b#}"), "\\a b## ");
        assert_eq!(ok("\\pdfstrcmp{a}{b}\\pdfstrcmp{b}{b}"), "-10 ");
        assert_eq!(ok("\\def\\a#1{<#1>}\\expanded{\\a{y}}"), "<y> ");
        assert_eq!(ok("\\lowercase{AbC}\\uppercase{x}"), "abcX ");
        assert_eq!(ok("\\lccode`A=`z \\lowercase{A}\\the\\sfcode`A"), "z999");
    }

    #[test]
    fn read_streams() {
        let mut e = super::super::Engine::default();
        let mut m = Machine::new(&mut e, "\\openin3=f \\read3 to\\x \\meaning\\x;\\read3 to\\x \\ifeof3 E\\fi\\immediate\\write16{w\\x}\\message{m}");
        m.read_file = Some(Box::new(|n: &str| (n == "f.tex").then(|| b"{a\nb}\nc".to_vec())));
        m.run();
        assert!(m.errors.is_empty(), "{:?}", m.errors);
        assert_eq!(m.out_text(), "macro:->{a b} ; ");
        assert_eq!(m.log, "wc \nm ");
    }
}

#[cfg(test)]
mod probe {
    use super::*;
    #[test]
    fn texsys_probe() {
        let mut e = super::super::Engine::default();
        let src = "\\newlinechar`\\^^J\\def\\today{1/2}\\immediate\\openout15=texsys.aux\n\\immediate\\write15{\\today^^J}\\immediate\\closeout15 \\endlinechar=-1 \\openin3./texsys.aux \\ifeof3 EOF\\else\\read3 to \\reserved@a \\immediate\\write17{[\\meaning\\reserved@a][\\reserved@a]}\\fi";
        let mut m = Machine::new(&mut e, src);
        m.run();
        eprintln!("{:?} {:?} {:?}", m.errors, m.log, m.out_text());
    }
}

#[cfg(test)]
mod mu_tests {
    fn ok(src: &str) -> String {
        let mut e = super::super::Engine::default();
        let mut m = super::Machine::new(&mut e, src);
        m.run();
        assert!(m.errors.is_empty(), "{:?}", m.errors);
        m.out_text()
    }

    #[test]
    fn mu_glue_is_its_own_kind() {
        assert_eq!(ok("\\muskip3=3mu plus 1fil\\relax\\the\\muskip3"), "\\relax 3.0mu plus 1.0fil");
        assert_eq!(ok("\\thinmuskip=2mu\\advance\\thinmuskip by 1.5mu\\relax\\the\\thinmuskip"), "\\relax 3.5mu");
        assert_eq!(ok("\\muskipdef\\m=4 \\m=1mu\\relax\\the\\muexpr\\m*2\\relax"), "\\relax 2.0mu");
        assert_eq!(ok("\\skip1=\\glueexpr 1pt plus 2fil\\relax\\skip2=\\skip1\\relax\\the\\skip2"), "\\relax 1.0pt plus 2.0fil");
        assert_eq!(ok("\\skip1=2\\dimexpr 1.5pt\\relax\\relax\\the\\skip1"), "\\relax 3.0pt");
        assert_eq!(ok("\\the\\mutoglue\\thinmuskip"), "0.0pt");
    }
}
