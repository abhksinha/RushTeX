//! pgfmath's trigonometry and division as pgf computes them: every value a length in pt held
//! in sp, as `\pgfmath@returnone` leaves it (printed by `\pgfmath@tonumber` and read back,
//! which TeX's printing makes the same length), and every step TeX's own arithmetic:
//! `\divide` truncating toward zero, `<factor><dimen>` by §455 (the factor printed from a
//! length, so its fraction is that length's), lengths scanned from printed digits by
//! `round_decimals`. Sources: pgfmathfunctions.trigonometric.code.tex (sine, cosine, tangent,
//! arc tangent and their tables, generated into `engine_pgfmath_tables.rs` by
//! scripts/gen_pgfmath_tables.py) and pgfmathfunctions.basic.code.tex (division, reciprocal).

#[path = "engine_pgfmath_tables.rs"]
mod tables;

use crate::backend::backend_scaled::{round_decimals, times_unit, xn_over_d};
use tables::{ATAN, COS};

const PT: i32 = 1 << 16;

/// `<factor><dimen>` (TeX §455) with the factor printed from `e` sp: its integer part times
/// `v` plus `xn_over_d(v, fraction, 2^16)`, the sign applied last.
pub(super) fn fmul(e: i32, v: i32) -> i32 {
    let a = e.unsigned_abs() as i64;
    let r = times_unit(a >> 16, (a & 0xffff) as i32, v as i64);
    (if e < 0 { -r } else { r }) as i32
}

/// `\pgfmath@tonumber`: `\the` without `pt`.
pub(super) fn print(v: i32) -> String {
    let mut s = String::new();
    crate::engine::engine_value::print_scaled(v, &mut s);
    s
}

/// A decimal TeX scans as a length in pt: the sign, the integer part, `round_decimals` of the
/// first 17 digits after the point.
fn scan(text: &str) -> i32 {
    let (neg, body) = match text.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, text),
    };
    let (whole, frac) = body.split_once('.').unwrap_or((body, ""));
    let v = whole.parse::<i32>().unwrap_or(0) * PT + round_decimals(frac.as_bytes());
    if neg {
        -v
    } else {
        v
    }
}

/// `\pgfmathcos@` (trigonometric.code.tex:134): the angle reduced by whole turns of its
/// integer part, folded into 0..180, then `\pgfmath@cos@<n>` at a whole degree or the two
/// neighbours weighted by the fraction.
pub(super) fn cos(mut x: i32) -> i32 {
    // `\c@pgfmath@counta\the\pgfmath@x`: the integer part, its sign kept.
    let turns = x / PT / 360 * -360;
    x += turns * PT;
    if x < 0 {
        x = -x;
    }
    if x >= 180 * PT {
        x = 360 * PT - x;
    }
    let (n, xa) = ((x / PT) as usize, x % PT);
    if xa == 0 {
        COS[n]
    } else {
        fmul(COS[n], PT - xa) + fmul(COS[n + 1], xa)
    }
}

/// `\pgfmathsin@` (:88): the cosine's steps from `x - 90`.
pub(super) fn sin(x: i32) -> i32 {
    cos(x - 90 * PT)
}

/// `\pgfmathtan@` (:192): the reciprocal of the cosine as a factor of the sine.
pub(super) fn tan(x: i32) -> i32 {
    fmul(reciprocal(cos(x)), sin(x))
}

/// `\pgfmathreciprocal@` (basic.code.tex:204): from the printed number's integer part and its
/// first five decimals (zero-padded), `10^9` divided by them in integers, the quotient's
/// last four digits put back as a fraction.
pub(super) fn reciprocal(x: i32) -> i32 {
    let s = print(x) + "0000000";
    let (whole, digits) = s.split_once('.').unwrap_or((&s, "00000"));
    let five: i32 = digits[..5].parse().unwrap_or(0);
    let int: i32 = whole.parse().unwrap_or(0);
    if five == 0 {
        return PT / int;
    }
    let wide = int > 100;
    let a: i32 = format!("{whole}{}", &digits[..if wide { 3 } else { 5 }]).parse().unwrap_or(1);
    let mut b = 1_000_000_000 / a;
    let q = b / 10000;
    b -= q * 10000;
    let mut y = b * PT;
    if wide {
        y /= 1_000_000;
    } else {
        // `\pgfmath@y.1\pgfmath@y` four times.
        for _ in 0..4 {
            y = xn_over_d(y, 6554, PT);
        }
    }
    q * PT + y
}

/// Below `\maxdimen`, or TeX stops with "Dimension too large" / "Arithmetic overflow".
const MAX: i64 = (1 << 30) - 1;

/// `\pgfmathparse` (pgfmathparser.code.tex) of `+ - * /`, parentheses, numbers, lengths with
/// a unit and length registers (`reg` gives their values), all in sp, and whether a unit or
/// register appeared (`\ifpgfmathunitsdeclared`). The fast lane (:77): only digits and points
/// are the number as written. Else operands: unary `+`, `-` (`neg`, precedence 500 like `+`,
/// :49 of basic.code.tex), `(`; a number (`\pgfmath@parse@@number`), then a two-letter unit
/// (`\pgfmath@dimen=<number><unit>`) or a register (`<number>\reg`, a factor), or alone; a
/// register. Operators `+ -` (500), `* /` (700) applied on the stack while the one below is
/// not weaker (`\pgfmath@operation@@process`, left to right): `add`, `subtract` by `\advance`,
/// `multiply` the second printed as the factor of the first, `divide` as [`divide`]. Of the
/// functions `mod` and `int` ([`function`]); others, `^`, comparisons, exponents, octal (`0`
/// then digits, `\pgfmath@base` 8) are refused.
pub(super) fn parse(text: &str, reg: &dyn Fn(&str) -> Option<i32>) -> Option<(i32, bool)> {
    let t = text.as_bytes();
    if !t.is_empty() && t.iter().all(|b| b.is_ascii_digit() || *b == b'.') {
        return number(text).map(|v| (v, false));
    }
    let (mut vals, mut ops): (Vec<i64>, Vec<u8>) = (Vec::new(), Vec::new());
    let mut units = false;
    let mut k = 0;
    let skip = |k: &mut usize| {
        while *k < t.len() && matches!(t[*k], b' ' | b'\t' | b'\n') {
            *k += 1;
        }
    };
    let prec = |op: u8| match op {
        b'n' | b'+' | b'-' => 500,
        b'*' | b'/' => 700,
        _ => 5,
    };
    let apply = |vals: &mut Vec<i64>, op: u8| -> Option<()> {
        let b = vals.pop()?;
        let r = if op == b'n' {
            -b
        } else {
            let a = vals.pop()?;
            match op {
                b'+' => a + b,
                b'-' => a - b,
                b'*' => {
                    let f = b.unsigned_abs() as i64;
                    let r = crate::backend::backend_scaled::times_unit(f >> 16, (f & 0xffff) as i32, a);
                    if b < 0 { -r } else { r }
                }
                b'/' if b != 0 => divide(a as i32, b as i32) as i64,
                _ => return None,
            }
        };
        (r.abs() <= MAX).then(|| vals.push(r))
    };
    loop {
        // An operand.
        skip(&mut k);
        match t.get(k)? {
            b'+' => {
                k += 1;
                continue;
            }
            b'-' => {
                ops.push(b'n');
                k += 1;
                continue;
            }
            b'(' => {
                ops.push(b'(');
                k += 1;
                continue;
            }
            _ => {}
        }
        // A function: its name, then its arguments in parentheses, each parsed alone.
        if t[k].is_ascii_alphabetic() {
            let at = k;
            while k < t.len() && t[k].is_ascii_alphabetic() {
                k += 1;
            }
            let fname = &text[at..k];
            if t.get(k) != Some(&b'(') {
                return None;
            }
            let mut depth = 0;
            let mut args = Vec::new();
            let mut start = k + 1;
            loop {
                match t.get(k)? {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b',' if depth == 1 => {
                        args.push(&text[start..k]);
                        start = k + 1;
                    }
                    _ => {}
                }
                k += 1;
            }
            args.push(&text[start..k]);
            k += 1;
            let mut vs = Vec::new();
            for a in args {
                let (v, u) = parse(a.trim(), reg)?;
                units |= u;
                vs.push(v);
            }
            vals.push(function(fname, &vs)? as i64);
        } else {
        let from = k;
        while k < t.len() && (t[k].is_ascii_digit() || t[k] == b'.') {
            k += 1;
        }
        let num = &text[from..k];
        let name = |k: &mut usize| {
            let at = *k + 1;
            let mut e = at;
            while e < t.len() && t[e].is_ascii_alphabetic() {
                e += 1;
            }
            *k = e;
            (e > at).then(|| &text[at..e])
        };
        let v = if num.is_empty() {
            if t[k] != b'\\' {
                return None;
            }
            units = true;
            reg(name(&mut k)?)? as i64
        } else {
            if num.matches('.').count() > 1 || num == "." || (num.len() > 1 && num.starts_with('0') && !num.contains('.')) {
                return None;
            }
            let n = number(num)?;
            match t.get(k) {
                Some(b'\\') => {
                    units = true;
                    let r = reg(name(&mut k)?)? as i64;
                    crate::backend::backend_scaled::times_unit(n as i64 >> 16, n & 0xffff, r)
                }
                Some(c) if c.is_ascii_alphabetic() => {
                    let unit = text.get(k..k + 2)?;
                    if !matches!(unit, "pt" | "cm" | "mm" | "in" | "bp" | "pc" | "dd" | "cc" | "sp") {
                        return None;
                    }
                    k += 2;
                    if t.get(k).is_some_and(|c| c.is_ascii_alphabetic()) {
                        return None;
                    }
                    units = true;
                    crate::backend::backend_scaled::scan_dimen(&format!("{num}{unit}"), 0, 0)?
                }
                _ => n as i64,
            }
        };
        if v.abs() > MAX {
            return None;
        }
        vals.push(v);
        }
        // An operator, `)` or the end.
        loop {
            skip(&mut k);
            match t.get(k) {
                None => {
                    while let Some(op) = ops.pop() {
                        if op == b'(' {
                            return None;
                        }
                        apply(&mut vals, op)?;
                    }
                    return (vals.len() == 1).then(|| (vals[0] as i32, units));
                }
                Some(b')') => {
                    k += 1;
                    loop {
                        match ops.pop()? {
                            b'(' => break,
                            op => apply(&mut vals, op)?,
                        }
                    }
                }
                Some(&op @ (b'+' | b'-' | b'*' | b'/')) => {
                    k += 1;
                    while let Some(&top) = ops.last() {
                        if top == b'(' || prec(op) > prec(top) {
                            break;
                        }
                        ops.pop();
                        apply(&mut vals, top)?;
                    }
                    ops.push(op);
                    break;
                }
                _ => return None,
            }
        }
    }
}

/// pgfmath's `mod` (basic.code.tex:271: the counts of the two lengths `\divide`d, the
/// second times that taken from the first) and `int` (round.code.tex:83: the printed
/// number's digits before the point, `-0` as `0`).
fn function(name: &str, args: &[i32]) -> Option<i32> {
    match (name, args) {
        ("mod", &[a, b]) if b != 0 => {
            let q = a / b;
            let r = a as i64 - b as i64 * q as i64;
            (r.abs() <= MAX).then_some(r as i32)
        }
        ("int", &[a]) => {
            let p = print(a);
            let whole: i32 = p.split('.').next()?.parse().ok()?;
            Some(whole * PT)
        }
        _ => None,
    }
}

/// `\pgfmathtruncatemacro` (pgfmathcalc.code.tex:227): `\pgfmathsetcount`, the integer part
/// of the result as printed.
pub(super) fn truncate(text: &str, reg: &dyn Fn(&str) -> Option<i32>) -> Option<i32> {
    let (v, _) = parse(text, reg)?;
    print(v).split('.').next()?.parse().ok()
}

/// A number as written, as TeX scans it in pt (below `16384pt`).
fn number(text: &str) -> Option<i32> {
    let (whole, _) = text.split_once('.').unwrap_or((text, ""));
    (text != "." && text.matches('.').count() <= 1 && whole.len() <= 5 && whole.parse::<i32>().unwrap_or(0) < 16384).then(|| scan(text))
}

/// `\pgfmathdivide@` (basic.code.tex:66): by an integer divisor `\divide`; by one below 1 its
/// reciprocal as a factor; else `\pgfmathdivide@@`'s long division written out in digits.
pub(super) fn divide(mut x: i32, mut y: i32) -> i32 {
    if y % PT == 0 {
        return x / (y / PT);
    }
    let mut neg = false;
    if x < 0 {
        neg = true;
        x = -x;
    }
    if y < 0 {
        neg = !neg;
        y = -y;
    }
    if y < PT {
        let r = reciprocal(y);
        return fmul(if neg { -r } else { r }, x);
    }
    let mut result = String::from("0");
    let mut period = true;
    // `\pgfmath@small@number` 0.00002pt.
    let small = round_decimals(b"00002");
    while small < x && small < y {
        if y > x {
            if period {
                result.push('.');
                period = false;
            }
            y = by_ten(y);
            if y > x {
                result.push('0');
            }
        } else {
            let mut q = x / y;
            x -= q * y;
            let mut append = true;
            if !period && q > 9 {
                advance_last_digit(&mut result);
                q -= 10;
                append = q != 0;
            }
            if append {
                result.push_str(&q.to_string());
            }
        }
    }
    let v = scan(&result);
    if neg {
        -v
    } else {
        v
    }
}

/// `\pgfmathdivide@dimenbyten`: the printed length with its point moved one digit left.
fn by_ten(y: i32) -> i32 {
    let s = print(y);
    let (whole, frac) = s.split_once('.').unwrap_or((&s, ""));
    let i: i32 = whole.parse().unwrap_or(0);
    scan(&format!("{}.{}{}", i / 10, i % 10, frac))
}

/// `\pgfmathdivide@advance@last@digit`: one unit of the last decimal written added, the sum
/// printed again without a trailing `0`.
fn advance_last_digit(result: &mut String) {
    let after = result.split_once('.').map_or(0, |(_, f)| f.len());
    let unit = match after {
        0 => PT,
        1 => round_decimals(b"1"),
        2 => round_decimals(b"01"),
        3 => round_decimals(b"001"),
        4 => round_decimals(b"0001"),
        _ => round_decimals(b"00001"),
    };
    let s = print(scan(result) + unit);
    *result = match s.strip_suffix(".0") {
        Some(w) => format!("{w}."),
        None => s,
    };
}

/// `\pgfmath@multiply@thousand` of the printed number: its point moved three digits right.
fn times_thousand(x: i32) -> i32 {
    let s = print(x) + "000";
    let (whole, frac) = s.split_once('.').unwrap_or((&s, "000"));
    scan(&format!("{whole}{}.{}", &frac[..3], &frac[3..]))
}

/// `\pgfmath@table@lookup`: the entry at the integer part, plus the fraction (the printed
/// digits as a factor) of the step to the next entry when there is one.
fn lookup(x: i32, table: &[i32]) -> i32 {
    let n = (x / PT) as usize;
    let v = table[n];
    if n + 1 < table.len() {
        v + fmul(x % PT, table[n + 1] - v)
    } else {
        v
    }
}

/// `\pgfmathatan@` (trigonometric.code.tex:329): `\pgfmath@atan@` at a thousand times the
/// argument, or at its reciprocal's taken from 90.
pub(super) fn atan(x: i32) -> i32 {
    let a = x.abs();
    let r = lookup(times_thousand(if a > PT { reciprocal(a) } else { a }), &ATAN);
    let r = if a > PT { 90 * PT - r } else { r };
    if x < 0 {
        -r
    } else {
        r
    }
}

/// `\pgfmathatan2@` (:362), y first: the axes below 0.001 in `|y|`, else the arc tangent of
/// the smaller absolute value over the larger.
pub(super) fn atan2(y: i32, x: i32) -> i32 {
    if y.abs() < round_decimals(b"001") {
        return if x < 0 {
            180 * PT
        } else if x > 0 {
            0
        } else if y < 0 {
            -90 * PT
        } else {
            90 * PT
        };
    }
    let (ta, tb) = (y.abs(), x.abs());
    let r = if ta > tb {
        90 * PT - atan(divide(x, ta))
    } else {
        let r = atan(divide(ta, x));
        if x < 0 {
            r + 180 * PT
        } else {
            r
        }
    };
    if y < 0 {
        -r
    } else {
        r
    }
}

/// `\pgfmathsqrt@` (basic.code.tex:562): the printed number, a `0` put before it for an odd
/// count of integer digits, read two digits at a time (integer and fraction digits run
/// together, `00` once they end), each digit of the root the largest `d` with
/// `(20r + d)d` within the remainder; five digits after the point, read back.
pub(super) fn sqrt(x: i32) -> i32 {
    let (zero, mut target) = match x / PT {
        0..=9 => (true, 1),
        10..=99 => (false, 1),
        100..=999 => (true, 2),
        1000..=9999 => (false, 2),
        _ => (true, 3),
    };
    let printed = print(x.max(0));
    let mut digits: Vec<i64> = printed.bytes().filter(u8::is_ascii_digit).map(|b| (b - b'0') as i64).collect();
    if zero {
        digits.insert(0, 0);
    }
    let mut next = digits.into_iter();
    let first = next.next().unwrap_or(0) * 10 + next.next().unwrap_or(0);
    let mut root = (0..=9).rev().find(|r| r * r <= first).unwrap_or(0);
    let mut rem = first - root * root;
    let mut text = root.to_string();
    loop {
        let pair = next.next().map_or(0, |a| a * 10 + next.next().unwrap_or(0));
        rem = rem * 100 + pair;
        let d = (0..=9).rev().find(|d| (20 * root + d) * d <= rem).unwrap_or(0);
        rem -= (20 * root + d) * d;
        root = root * 10 + d;
        target -= 1;
        if target == 0 {
            text.push('.');
        }
        text.push((b'0' + d as u8) as char);
        if target == -4 {
            break;
        }
    }
    scan(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// pdfLaTeX's `\pgfmathparse` of each (`\linewidth` 345pt): the result and the units flag.
    #[test]
    fn parses_as_pgfmath() {
        let reg = |n: &str| (n == "linewidth").then_some(345 << 16);
        let p = |t: &str| parse(t, &reg).map(|(v, u)| (print(v), u));
        let oracle = [
            ("1*7mm", "19.91692", true), ("\\linewidth-18mm", "293.78505", true), ("0.5\\linewidth", "172.5", true), ("2+3*4", "14.0", false),
            ("-1+2", "1.0", false), ("(1+2)/3", "1.0", false), ("10-2-3", "5.0", false), (" 1.5 ", "1.5", false), ("3*7mm", "59.75075", true),
            ("7mm*3", "59.75075", true), ("1.3*2.7", "3.51", false), ("10/3", "3.33333", false), ("7/0.3", "23.33362", false), ("1cm/3", "9.48424", true),
            ("2.5*1cm+1", "72.13185", true), ("-0.75*(2+1.1)", "-2.325", false), ("\\linewidth/7", "49.2857", true), ("1.7*\\linewidth", "586.49895", true),
            ("1.7\\linewidth", "586.49895", true),
        ];
        for (t, v, u) in oracle {
            assert_eq!(p(t), Some((v.to_string(), u)), "{t}");
        }
        // `012` alone is the fast lane's 12; in an expression octal.
        assert_eq!(p("012"), Some(("12.0".into(), false)));
        for t in ["sin(30)", "2^2", "1+012", "1e3", "\\foo", "1+", "(1", "3em"] {
            assert!(p(t).is_none(), "{t}");
        }
    }

    /// pdfLaTeX's `\pgfmathsin@`, `\pgfmathcos@`, `\pgfmathtan@`, `\pgfmathdivide@`,
    /// `\pgfmathreciprocal@`, `\pgfmathatan2@` of a few arguments.
    #[test]
    fn computes_as_pgfmath() {
        let n = scan;
        let cases: [(i32, &str); 12] = [
            (sin(n("-281.9")), "0.97849"),
            (cos(n("-281.9")), "0.20619"),
            (tan(n("33.66")), "0.66586"),
            (tan(n("330.23")), "-0.57196"),
            (divide(n("426"), n("10")), "42.59999"),
            (divide(n("16.56284"), n("-0.35")), "-47.32498"),
            (divide(n("-31.189"), n("15")), "-2.07925"),
            (reciprocal(n("-181.8")), "-0.0055"),
            (atan(n("15.97607")), "86.42368"),
            (atan2(n("4"), n("-3")), "126.86989"),
            (atan2(n("28.45274"), n("28.45274")), "45.0"),
            (atan2(n("0"), n("-1")), "180.0"),
        ];
        for (k, (got, want)) in cases.iter().enumerate() {
            assert_eq!(print(*got), *want, "case {k}");
        }
        // `\pgfmathsqrt@`'s results as pdfLaTeX prints them, read back as lengths.
        for (x, want) in [("17", "4.12310"), ("2", "1.41421"), ("150.25", "12.25765"), ("0.04", "0.20000")] {
            assert_eq!(sqrt(n(x)), n(want), "sqrt {x}");
        }
        // `\pgfmathtruncatemacro` with `mod` and `int`.
        let none = |_: &str| None;
        for (e, want) in [("mod(11,7)", 4), ("int(11/7)", 1), ("4+31-1", 34), ("mod(-9,4)", -1), ("int((12-1)/4)", 2), ("7/2", 3)] {
            assert_eq!(truncate(e, &none), Some(want), "{e}");
        }
    }

    /// Every line of the file `RT_PGFMATH_ORACLE` names (`fn args... = result`, results from
    /// pdfLaTeX) checked; nothing without it.
    #[test]
    fn matches_an_oracle_file() {
        let Ok(path) = std::env::var("RT_PGFMATH_ORACLE") else { return };
        let text = std::fs::read_to_string(path).expect("oracle");
        let mut bad = 0;
        for line in text.lines() {
            let Some((call, want)) = line.split_once(" = ") else { continue };
            let w: Vec<&str> = call.split_whitespace().collect();
            let a = |k: usize| scan(w[k]);
            let got = match w[0] {
                "sin" => sin(a(1)),
                "cos" => cos(a(1)),
                "tan" => tan(a(1)),
                "reciprocal" => reciprocal(a(1)),
                "divide" => divide(a(1), a(2)),
                "atan" => atan(a(1)),
                "atan2" => atan2(a(1), a(2)),
                _ => continue,
            };
            if print(got) != want.trim() {
                bad += 1;
                if bad < 20 {
                    eprintln!("{call} = {} (pgf {})", print(got), want.trim());
                }
            }
        }
        assert_eq!(bad, 0);
    }
}
