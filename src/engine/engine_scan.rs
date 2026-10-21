//! Reading quantities from macro-argument text: tex.web §448 `scan_dimen` and §461
//! `scan_glue`, over a written dimension (`2.5pt`, `-1em`), an internal quantity
//! (`\parindent`, `.5\textwidth`), or glue with `plus` and `minus` parts (`fil` orders
//! included). Units are read by [`crate::backend::scaled::scan_dimen`].

use super::engine_eqtb::Glue;
use crate::backend::scaled::{scan_dimen, times_unit_f};

/// `[-]<factor>?\name` or a written dimension, in scaled points. `lookup` gives an internal
/// quantity's glue (its width is the dimension, §455).
pub fn scan_dimen_ref(text: &str, em: i64, ex: i64, lookup: &impl Fn(&str) -> Option<Glue>) -> Option<i32> {
    let t = text.trim();
    let Some(bs) = t.find('\\') else {
        return scan_dimen(t, em, ex).map(|v| v as i32);
    };
    let (sign, factor_text) = match t[..bs].trim().strip_prefix('-') {
        Some(f) => (-1, f.trim()),
        None => (1, t[..bs].trim()),
    };
    let name = t[bs + 1..].trim();
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphabetic() || b == b'@') {
        return None;
    }
    let width = lookup(name)?.width as i64;
    let v = if factor_text.is_empty() { width } else { times_unit_f(factor_text.parse::<f32>().ok()?, width) };
    Some((sign * v) as i32)
}

/// A glue `<dimen> [plus <dimen or fil>] [minus <dimen or fil>]`, or an internal glue.
pub fn scan_glue(text: &str, em: i64, ex: i64, lookup: &impl Fn(&str) -> Option<Glue>) -> Option<Glue> {
    let t = text.trim();
    let (w, rest) = split_keyword(t, "plus").map(|(a, b)| (a, Some(b))).unwrap_or((t, None));
    let (w, minus_from_w) = match rest {
        Some(_) => (w, None),
        None => split_keyword(w, "minus").map(|(a, b)| (a, Some(b))).unwrap_or((w, None)),
    };
    // `\name` alone is the whole internal glue (§461), not just its width.
    if rest.is_none() && minus_from_w.is_none() {
        if let Some(name) = w.strip_prefix('\\') {
            if let Some(g) = lookup(name.trim()) {
                return Some(g);
            }
        }
    }
    let mut g = Glue { width: scan_dimen_ref(w, em, ex, lookup)?, ..Glue::default() };
    let (plus, minus) = match rest {
        Some(r) => match split_keyword(r, "minus") {
            Some((p, m)) => (Some(p), Some(m)),
            None => (Some(r), None),
        },
        None => (None, minus_from_w),
    };
    if let Some(p) = plus {
        (g.stretch, g.stretch_order) = scan_stretch(p, em, ex, lookup)?;
    }
    if let Some(m) = minus {
        (g.shrink, g.shrink_order) = scan_stretch(m, em, ex, lookup)?;
    }
    Some(g)
}

/// A stretch or shrink: a dimension, or `<factor>fil`, `fill`, `filll` (§454).
fn scan_stretch(text: &str, em: i64, ex: i64, lookup: &impl Fn(&str) -> Option<Glue>) -> Option<(i32, u8)> {
    let t = text.trim();
    if let Some(p) = t.find("fil") {
        let order = t[p..].bytes().filter(|&b| b == b'l').count() as u8;
        let factor = t[..p].trim();
        let v = if factor.is_empty() { 1.0 } else { factor.parse::<f32>().ok()? };
        return Some((times_unit_f(v, 65536) as i32, order.clamp(1, 3)));
    }
    Some((scan_dimen_ref(t, em, ex, lookup)?, 0))
}

/// §440 `scan_int` over the start of `text`: signs, then a character code (`` `x `` or
/// `` `\x ``), an octal (`'`), hexadecimal (`"`, digits `0-9A-F`, §445) or decimal constant,
/// or an internal integer `\name` read by `lookup`. A constant ends at one optional space
/// (§443). Returns the value and the bytes used.
pub fn scan_int(text: &str, lookup: &impl Fn(&str) -> Option<i32>) -> Option<(i32, usize)> {
    let b = text.as_bytes();
    let mut i = 0;
    let mut neg = false;
    // §441: optional signs and spaces.
    while let Some(&c) = b.get(i) {
        match c {
            b' ' => {}
            b'-' => neg = !neg,
            b'+' => {}
            _ => break,
        }
        i += 1;
    }
    let sign = |v: i64| (if neg { -v } else { v }) as i32;
    let space = |i: usize| if b.get(i) == Some(&b' ') { i + 1 } else { i };
    match *b.get(i)? {
        b'`' => {
            // §442: an alphabetic constant; `\x` stands for `x`.
            i += 1;
            if b.get(i) == Some(&b'\\') {
                i += 1;
            }
            let c = text[i..].chars().next()?;
            Some((sign(c as i64), space(i + c.len_utf8())))
        }
        b'\\' => {
            let n = b[i + 1..].iter().take_while(|c| c.is_ascii_alphabetic() || **c == b'@').count().max(1);
            let name = text.get(i + 1..i + 1 + n)?;
            Some((sign(lookup(name)? as i64), i + 1 + n))
        }
        q => {
            let (radix, start) = match q {
                b'\'' => (8, i + 1),
                b'"' => (16, i + 1),
                _ => (10, i),
            };
            let digit = |c: u8| match c {
                b'0'..=b'9' => Some((c - b'0') as i64),
                b'A'..=b'F' if radix == 16 => Some((c - b'A' + 10) as i64),
                _ => None,
            };
            let mut j = start;
            let mut v: i64 = 0;
            while let Some(d) = b.get(j).and_then(|&c| digit(c)).filter(|&d| d < radix) {
                // §445: "Number too big" stops at 2^31 - 1.
                v = (v * radix + d).min(i32::MAX as i64);
                j += 1;
            }
            (j > start).then(|| (sign(v), space(j)))
        }
    }
}

/// `text` split at the keyword `kw` (a whole word, as TeX's `scan_keyword` matches it).
fn split_keyword<'t>(text: &'t str, kw: &str) -> Option<(&'t str, &'t str)> {
    let p = text.find(kw)?;
    Some((text[..p].trim_end(), text[p + kw.len()..].trim_start()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PT: i32 = 65536;
    fn none(_: &str) -> Option<Glue> {
        None
    }

    #[test]
    fn written_and_internal_dimensions() {
        assert_eq!(scan_dimen_ref("2.5pt", 0, 0, &none), Some(163840));
        let tw = |n: &str| (n == "textwidth").then_some(Glue { width: 345 * PT, ..Glue::default() });
        assert_eq!(scan_dimen_ref("\\textwidth", 0, 0, &tw), Some(345 * PT));
        assert_eq!(scan_dimen_ref("-.5\\textwidth", 0, 0, &tw), Some(-(345 * PT) / 2));
        assert_eq!(scan_dimen_ref("\\nosuch", 0, 0, &tw), None);
    }

    #[test]
    fn integers() {
        let none = |_: &str| None;
        assert_eq!(scan_int("`\\@=11", &none), Some((64, 3)));
        assert_eq!(scan_int("`@ x", &none), Some((64, 3)));
        assert_eq!(scan_int("- 12 x", &none), Some((-12, 5)));
        assert_eq!(scan_int("'777", &none), Some((511, 4)));
        assert_eq!(scan_int("\"1F", &none), Some((31, 3)));
        assert_eq!(scan_int("\"1f", &none), Some((1, 2)));
        let reg = |n: &str| (n == "count@").then_some(7);
        assert_eq!(scan_int("\\count@ ", &reg), Some((7, 7)));
        assert_eq!(scan_int("x", &none), None);
    }

    #[test]
    fn glue_with_orders() {
        let g = scan_glue("10pt plus 2fil minus 3pt", 0, 0, &none).unwrap();
        assert_eq!(g, Glue { width: 10 * PT, stretch: 2 * PT, stretch_order: 1, shrink: 3 * PT, shrink_order: 0 });
        let g = scan_glue("0pt minus 1fill", 0, 0, &none).unwrap();
        assert_eq!((g.shrink, g.shrink_order, g.stretch), (PT, 2, 0));
    }
}
