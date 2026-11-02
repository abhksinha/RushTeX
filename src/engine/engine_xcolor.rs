//! xcolor's colour arithmetic on the GDL path: the HTML and RGB models (`\XC@cnv@HTML`,
//! `\XC@calcD`) and colour expressions `c!p` and `c1!p!c2` (`\@xcolor@@`, `\XC@mix`).
//!
//! xcolor computes with dimensions and with the printed text of dimensions: `\rdivide` is a
//! long division to five decimals, `\rshift` moves the decimal point of `\the\dimen@` one
//! place left as text, and a value times a dimension is §455's factor. Each is ported here
//! as it runs, so the literals equal pdfLaTeX's (`0.09021`, not `23/255` rounded).

use super::super::engine_value::print_scaled;
use crate::backend::scaled::xn_over_d;
use crate::data::data_nfss::decimal_sp;

const MAX_DIMEN: i32 = 0x3FFF_FFFF;

/// A colour as xcolor keeps it: its model and its components as printed text.
#[derive(Clone, PartialEq, Debug)]
pub(super) struct Spec {
    pub rgb: bool,
    pub vals: Vec<String>,
}

impl Spec {
    /// From a pdftex.def literal (`0 0 1 rg 0 0 1 RG`, `0.5 g 0.5 G`, `0 0 1 0 k 0 0 1 0 K`):
    /// cmyk the four values of a non-rgb spec.
    pub(super) fn from_literal(lit: &str) -> Option<Spec> {
        let w: Vec<&str> = lit.split(' ').collect();
        match w.as_slice() {
            [c, m, y, k, "k", ..] => Some(Spec { rgb: false, vals: [c, m, y, k].iter().map(|v| v.to_string()).collect() }),
            [r, g, b, "rg", ..] => Some(Spec { rgb: true, vals: vec![r.to_string(), g.to_string(), b.to_string()] }),
            [v, "g", ..] => Some(Spec { rgb: false, vals: vec![v.to_string()] }),
            _ => None,
        }
    }

    /// pdftex.def's `\c@lor@@rgb` or `\c@lor@@gray` literal.
    pub(super) fn literal(&self) -> String {
        let v = self.vals.join(" ");
        if self.rgb {
            format!("{v} rg {v} RG")
        } else if self.cmyk() {
            format!("{v} k {v} K")
        } else {
            format!("{v} g {v} G")
        }
    }

    pub(super) fn cmyk(&self) -> bool {
        self.vals.len() == 4
    }

    /// `\XC@cnv@gray` to cmyk: `0,0,0,\XC@calcC{g}`.
    fn gray_to_cmyk(&self) -> Option<Spec> {
        Some(Spec { rgb: false, vals: vec!["0".into(), "0".into(), "0".into(), calc_c(&self.vals[0])?] })
    }

    /// `\XC@cnv@rgb` to cmyk: cmy by `\XC@calcC` of each, then `\XC@cnv@cmy@cmyk` with
    /// `\adjustUCRBG` at its `1,1,1,1`: the least of the three (`\@@scl`, as written) scaled
    /// by each factor (`\XC@calcS`), taken from each of cmy (`\XC@add`), the fourth the
    /// black; each value `\XC@calcN`.
    fn rgb_to_cmyk(&self) -> Option<Spec> {
        let cmy = self.vals.iter().map(|v| calc_c(v)).collect::<Option<Vec<_>>>()?;
        let d = cmy.iter().map(|v| dec(v)).collect::<Option<Vec<_>>>()?;
        let k = &cmy[if d[0] < d[1] { if d[0] < d[2] { 0 } else { 2 } } else if d[1] < d[2] { 1 } else { 2 }];
        // `\XC@calcS{1}`: `\lshiftset\dimen@{1}`, under 100 `\dimen@\lshiftnum{k}\dimen@
        // \rrshift\dimen@`.
        if dec(k)? >= 100 << 16 {
            return None;
        }
        let s = strip(rshift(rshift(factor_times(&shift_text(k, 1), dec(&shift_text("1", 1))?)?)?)?);
        let mut vals = Vec::with_capacity(4);
        for v in &cmy {
            // `\XC@add@`: both `\llshiftset`, added, `\rrshift`.
            let sum = dec(&shift_text(v, 2))?.checked_add(dec(&shift_text(&neg(&s), 2))?)?;
            vals.push(calc_n(&strip(rshift(rshift(sum)?)?)));
        }
        vals.push(calc_n(&s));
        Some(Spec { rgb: false, vals })
    }

    /// `\XC@cnv@gray` to rgb: `\XC@calcN` of the value three times.
    fn to_rgb(&self) -> Spec {
        if self.rgb {
            return self.clone();
        }
        let n = calc_n(&self.vals[0]);
        Spec { rgb: true, vals: vec![n.clone(), n.clone(), n] }
    }
}

/// A signed decimal as TeX scans it before `pt`.
fn dec(s: &str) -> Option<i32> {
    match s.strip_prefix('-') {
        Some(r) => decimal_sp(r).map(|v| -v),
        None => decimal_sp(s.strip_prefix('+').unwrap_or(s)),
    }
}

/// `\strip@pt`: the printed dimension without `.0` and `pt`.
fn strip(sp: i32) -> String {
    let mut s = String::new();
    print_scaled(sp, &mut s);
    if s.ends_with(".0") {
        s.truncate(s.len() - 2);
    }
    s
}

/// `<factor><dimen>` (§455): the factor's integer part times `v` plus `xn_over_d` of its
/// fraction, the factor's sign last.
fn factor_times(f: &str, v: i32) -> Option<i32> {
    let (neg, f) = match f.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, f),
    };
    let (int, frac) = f.split_once('.').unwrap_or((f, ""));
    let i: i64 = if int.is_empty() { 0 } else { int.parse().ok()? };
    let fr = decimal_sp(&format!("0.{frac}"))?;
    let r = i * v as i64 + xn_over_d(v, fr, 0x10000) as i64;
    if r.abs() > MAX_DIMEN as i64 {
        return None;
    }
    Some(if neg { -r as i32 } else { r as i32 })
}

/// `\rshift`: the decimal point of `\the` the dimension one place left, as text.
fn rshift(sp: i32) -> Option<i32> {
    let mut s = String::new();
    print_scaled(sp, &mut s);
    let (int, frac) = s.split_once('.')?;
    let k = int.len() - 1;
    dec(&format!("{}.{}{}", &int[..k], &int[k..], frac))
}

/// `\lshift`: the decimal point one place right (`\lshift@#1.#2#3PT`).
fn lshift(sp: i32) -> Option<i32> {
    let mut s = String::new();
    print_scaled(sp, &mut s);
    let (int, frac) = s.split_once('.')?;
    let (d, rest) = frac.split_at(1);
    let rest_zero = rest.bytes().all(|b| b == b'0');
    dec(&if rest_zero { format!("{int}{d}") } else { format!("{int}{d}.{rest}") })
}

/// `\rdivide\dimen@{b}` (xcolor.sty): both made positive, both `\lshift`ed when small, the
/// integer quotient, five decimals by long division and a sixth that adds 1sp when above 4.
fn rdivide(a: i32, b: &str) -> Option<i32> {
    let mut b = dec(b)?;
    let mut a = a;
    let mut neg = false;
    if a < 0 {
        a = -a;
        neg = true;
    }
    if b < 0 {
        b = -b;
        neg = !neg;
    }
    if a < factor_times(".1", MAX_DIMEN)? && b < factor_times(".01", MAX_DIMEN)? {
        a = lshift(a)?;
        b = lshift(b)?;
    }
    if b == 0 {
        return None;
    }
    let (mut ca, cb) = (a as i64, b as i64);
    let mut count = ca / cb;
    let mut text = format!("{count}.");
    for k in 0..6 {
        ca = (ca - count * cb) * 10;
        if ca.abs() > i32::MAX as i64 {
            return None;
        }
        count = ca / cb;
        if k < 5 {
            text.push_str(&count.to_string());
        }
    }
    let mut d = dec(&text)?;
    if count > 4 {
        d += 1;
    }
    Some(if neg { -d } else { d })
}

/// The text with a minus sign put in front, or taken off.
fn neg(v: &str) -> String {
    v.strip_prefix('-').map_or_else(|| format!("-{v}"), str::to_string)
}

/// `\lshiftnum` (one place) and `\llshiftnum` (two): the decimal point moved right in the
/// text.
fn shift_text(v: &str, places: usize) -> String {
    let (int, frac) = v.split_once('.').unwrap_or((v, ""));
    let frac = format!("{frac}00");
    format!("{int}{}.{}", &frac[..places], &frac[places..])
}

/// `\XC@calcC`: the complement, `\llshiftset\dimen@{-v}\advance\dimen@100\p@
/// \rrshift\dimen@`, then `\XC@calcN` of it.
fn calc_c(v: &str) -> Option<String> {
    let d = dec(&shift_text(&neg(v), 2))?.checked_add(100 << 16)?;
    Some(calc_n(&strip(rshift(rshift(d)?)?)))
}

/// `\XC@calcN`: `\XC@calcR` (clamped to 0..1, five decimals) without trailing zeros.
fn calc_n(v: &str) -> String {
    let (int, frac) = v.split_once('.').unwrap_or((v, ""));
    let r = if int.parse::<i64>().is_ok_and(|i| i > 0) && !int.starts_with('-') {
        "1.00000".to_string()
    } else if int.starts_with('-') {
        "0.00000".to_string()
    } else {
        format!("0.{}", format!("{frac}00000").get(..5).unwrap_or("00000"))
    };
    let (i, f) = r.split_once('.').expect("a point");
    let f = f.trim_end_matches('0');
    if f.is_empty() { i.to_string() } else { format!("{i}.{f}") }
}

/// An HTML (`17A46E`) or RGB (`23,74,110`) value as xcolor's `\definecolor` keeps it: rgb,
/// each component `\strip@pt` of `\rdivide{v pt}{255}`.
pub(super) fn from_255(model: &str, spec: &str) -> Option<Spec> {
    let parts: Vec<String> = match model {
        "HTML" if spec.len() == 6 && spec.bytes().all(|b| b.is_ascii_hexdigit()) => (0..3).map(|k| u32::from_str_radix(&spec[2 * k..2 * k + 2], 16).map(|v| v.to_string())).collect::<Result<_, _>>().ok()?,
        "RGB" => spec.split(',').map(|p| p.trim().to_string()).collect(),
        _ => return None,
    };
    if parts.len() != 3 {
        return None;
    }
    let vals = parts.iter().map(|p| rdivide(dec(p)?, "255").map(strip)).collect::<Option<Vec<_>>>()?;
    Some(Spec { rgb: true, vals })
}

/// `\@xcolor@@`'s mix of `base` with `mix` at `p` percent (`\XC@mix@`): per component
/// `\dimen@ b\@tempdima \advance\dimen@ m\@tempdimb \rrshift\dimen@`, the base's model,
/// or the mix colour's when the base is gray.
pub(super) fn mix(base: &Spec, p: &str, mix: &Spec) -> Option<Spec> {
    let pd = dec(p)?;
    let hundred = 100 << 16;
    if pd == hundred {
        return Some(base.clone());
    }
    if pd == 0 {
        return Some(mix.clone());
    }
    // cmyk mixes only with cmyk and gray (gray taken to cmyk); rgb with cmyk is not native.
    let (base, mix) = match (base.cmyk(), mix.cmyk()) {
        (true, true) => (base.clone(), mix.clone()),
        (true, false) if !mix.rgb => (base.clone(), mix.gray_to_cmyk()?),
        (true, false) => (base.clone(), mix.rgb_to_cmyk()?),
        (false, true) if !base.rgb => (base.gray_to_cmyk()?, mix.clone()),
        (false, true) => return None,
        _ => match (base.rgb, mix.rgb) {
            (false, true) => (base.to_rgb(), mix.clone()),
            (true, false) => (base.clone(), mix.to_rgb()),
            _ => (base.clone(), mix.clone()),
        },
    };
    let (a, b) = (pd, hundred - pd);
    let vals = base
        .vals
        .iter()
        .zip(&mix.vals)
        .map(|(x, y)| {
            let d = factor_times(x, a)?.checked_add(factor_times(y, b)?)?;
            Some(strip(rshift(rshift(d)?)?))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Spec { rgb: base.rgb, vals })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(v: [&str; 3]) -> Spec {
        Spec { rgb: true, vals: v.iter().map(|s| s.to_string()).collect() }
    }

    /// The values pdfLaTeX writes (2026-10-27, xcolor 2024).
    #[test]
    fn matches_pdflatex() {
        let accent = from_255("HTML", "174A6E").unwrap();
        assert_eq!(accent, rgb(["0.09021", "0.2902", "0.43137"]));
        assert_eq!(from_255("RGB", "23,74,110").unwrap(), accent);
        let white = Spec { rgb: false, vals: vec!["1".into()] };
        let black = Spec { rgb: false, vals: vec!["0".into()] };
        assert_eq!(mix(&accent, "12", &white).unwrap(), rgb(["0.89082", "0.91483", "0.93176"]));
        assert_eq!(mix(&rgb(["0", "0", "1"]), "20", &white).unwrap(), rgb(["0.8", "0.8", "1"]));
        assert_eq!(mix(&rgb(["1", "0", "0"]), "30", &rgb(["0", "1", "0"])).unwrap(), rgb(["0.3", "0.7", "0"]));
        assert_eq!(mix(&accent, "50", &black).unwrap(), rgb(["0.0451", "0.1451", "0.21568"]));
    }

    /// cmyk mixes as pdfLaTeX writes them (2026-11-01, xcolor 2024): `\current@color` after
    /// `\color{expr}`, the colours xcolor predefines and `acc` of HTML `174A6E`.
    #[test]
    fn mixes_cmyk_as_pdflatex() {
        let n = |s: &str| Spec::from_literal(crate::data::data_color::named(s, true).unwrap()).unwrap();
        let accent = from_255("HTML", "174A6E").unwrap();
        let k = |s: &Spec| s.literal().split(" k ").next().unwrap().to_string();
        assert_eq!(k(&mix(&n("cyan"), "55", &n("blue")).unwrap()), "1 0.45 0 0");
        assert_eq!(k(&mix(&n("cyan"), "30", &n("white")).unwrap()), "0.3 0 0 0");
        assert_eq!(k(&mix(&n("magenta"), "37", &accent).unwrap()), "0.21494 0.45894 0 0.35825");
        assert_eq!(k(&mix(&n("yellow"), "12.5", &n("black")).unwrap()), "0 0 0.125 0.875");
        assert_eq!(k(&mix(&n("olive"), "40", &n("gray")).unwrap()), "0 0 0.4 0.5");
        assert_eq!(k(&mix(&n("gray"), "40", &n("cyan")).unwrap()), "0.6 0 0 0.2");
        assert_eq!(k(&mix(&n("black"), "25", &n("magenta")).unwrap()), "0 0.75 0 0.25");
        assert_eq!(k(&mix(&n("cyan"), "50", &n("magenta")).unwrap()), "0.5 0.5 0 0");
        assert_eq!(k(&mix(&n("cyan"), "33", &n("orange")).unwrap()), "0.33 0.335 0.67 0");
        let first = mix(&n("cyan"), "71", &accent).unwrap();
        assert_eq!(k(&mix(&first, "40", &n("yellow")).unwrap()), "0.32358 0.01637 0.6 0.06596");
        // rgb with cmyk (`acc!60!cyan`, cmyk to rgb) is not ported.
        assert!(mix(&accent, "60", &n("cyan")).is_none());
    }
}
