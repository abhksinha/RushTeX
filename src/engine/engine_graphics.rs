//! graphicx's `\includegraphics` for the native job, with pdftex.def as its driver
//! (graphics.sty, graphicx.sty, pdftex.def), lowered straight to the boxes it builds.
//!
//! The size of the image goes through the same steps as in the macros, each in TeX's
//! scaled arithmetic: the image's natural size read back as a decimal of big points
//! (`\Gin@defaultbp`: `.99626\dimen@`, `\strip@pt`), the requested size (`\Gin@esetsize`),
//! scale factors as decimals (`\Gscale@div`), and the factor times a dimension where the
//! macros write `\Gin@scalex\Gin@nat@height`. The box is `\Gin@setfile`'s: an `\hbox`
//! of the requested size holding `\Ginclude@@pdftex`'s `\hbox`, which holds `\Gscale@box`'s
//! `\hbox to` the scaled width around the image between `\pdfsave`, `\pdfsetmatrix` and
//! `\pdfrestore` (none at scale 1).
//!
//! Taken: no star; the keys `width`, `height`, `scale` and `keepaspectratio`; `.pdf` and
//! `.png` files, found as graphicx finds them (`\Gin@extensions`). Anything else, a file
//! that is not there or a `.jpg`, runs the document on the engine.

use super::engine_catcode::Cat;
use super::engine_eqtb::Glue;
use super::engine_node::{Kind, Node};
use super::engine_run::Machine;
use super::engine_token::Token;
use super::engine_value::print_scaled;
use super::engine_whatsit::{RESTORE, SAVE, SET_MATRIX};
use crate::backend::backend_scaled::{round_decimals, times_unit};

/// `\Gin@extensions` (pdftex.def), each with its rule's type; a `None` type is one the
/// native job does not read.
const EXTENSIONS: [(&str, Option<bool>); 13] = [
    (".pdf", Some(true)),
    (".png", Some(false)),
    (".jpg", None),
    (".mps", None),
    (".jpeg", None),
    (".jbig2", None),
    (".jb2", None),
    (".PDF", Some(true)),
    (".PNG", Some(false)),
    (".JPG", None),
    (".JPEG", None),
    (".JBIG2", None),
    (".JB2", None),
];

/// A decimal as `scan_dimen` reads it before its unit (§448–§452): the integer part and
/// the fraction in 2^-16. Only digits and one `.` or `,`; no sign.
fn decimal(text: &str) -> Option<(i64, i32)> {
    let t = text.trim();
    let (int, frac) = t.split_once(['.', ',']).unwrap_or((t, ""));
    if t.is_empty() || !int.bytes().chain(frac.bytes()).all(|c| c.is_ascii_digit()) || int.len() > 9 {
        return None;
    }
    let i = if int.is_empty() { 0 } else { int.parse().ok()? };
    Some((i, round_decimals(frac.as_bytes())))
}

/// `<decimal><internal dimen>` (§455): the factor times `v`.
fn times(text: &str, v: i32) -> Option<i32> {
    let (i, f) = decimal(text)?;
    let r = times_unit(i, f, v as i64);
    (r.abs() < 1 << 30).then_some(r as i32)
}

/// `<decimal>bp` (§458): 7227/7200 of a point, the remainder carried into the fraction.
fn bp(text: &str) -> Option<i32> {
    let (i, f) = decimal(text)?;
    let (num, denom) = (7227i64, 7200i64);
    let p = i * num;
    let (mut v, r) = (p / denom, p % denom);
    let mut f = (num * f as i64 + 65536 * r) / denom;
    v += f / 65536;
    f %= 65536;
    (v < 0o40000).then_some((v * 65536 + f) as i32)
}

/// `\strip@pt`: `\the` of a dimension without `pt`, and without `.0`.
fn strip_pt(v: i32) -> String {
    let mut s = String::new();
    print_scaled(v, &mut s);
    if let Some(i) = s.strip_suffix(".0") {
        return i.to_string();
    }
    s
}

/// `\Gin@defaultbp` of a dimension: `.99626\dimen@`, stripped.
fn default_bp(v: i32) -> Option<String> {
    times(".99626", v).map(strip_pt)
}

/// `\Gscale@div`: `num` over `den` as a decimal, by halving `\count@` while doubling
/// `\dimen@` below 8192pt, then `\divide`s. `None` where the macro divides by zero.
fn scale_div(num: i32, den: i32) -> Option<String> {
    if den == 0 {
        return None;
    }
    let (mut d, mut count) = (num as i64, 65536i64);
    if d < 0 {
        d = -d;
        count = -count;
    }
    if d > 0 {
        while (if count < 2 { 0o7777777777 } else { d }) < 8192 * 65536 {
            d *= 2;
            count /= 2;
        }
        let d2 = den as i64 / count;
        if d2 == 0 {
            return None;
        }
        d /= d2;
    }
    Some(strip_pt(d as i32))
}

/// pdftex.def's `\GPT@NormalizeNumber`: no spaces or leading `+`, no leading zeros in the
/// integer part or trailing zeros in the fraction, `0` for nothing left.
fn normalize(text: &str) -> String {
    let t: String = text.chars().filter(|c| *c != ' ').collect();
    let t = t.trim_start_matches('+');
    let (sign, t) = match t.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", t),
    };
    let (int, frac) = t.split_once('.').unwrap_or((t, ""));
    let (int, frac) = (int.trim_start_matches('0'), frac.trim_end_matches('0'));
    if int.is_empty() && frac.is_empty() {
        return "0".to_string();
    }
    if frac.is_empty() {
        format!("{sign}{int}")
    } else {
        format!("{sign}{int}.{frac}")
    }
}

/// The keys the native job takes, as `\setkeys{Gin}` leaves them.
#[derive(Default)]
struct Keys {
    width: Option<Vec<Token>>,
    height: Option<Vec<Token>>,
    scale: Option<String>,
    iso: bool,
}

impl Machine<'_, '_> {
    /// `\includegraphics[keys]{file}`: `\leavevmode`, then the image's box.
    pub(super) fn include_graphics(&mut self) {
        if self.leave_vmode() {
            return;
        }
        if self.mode == super::engine_run::Mode::Math {
            return self.not_native("\\includegraphics in math");
        }
        if self.next_is('*') {
            return self.not_native("\\includegraphics*");
        }
        let keys = if self.next_is('[') { Some(self.bracket_arg()) } else { None };
        if self.next_is('[') {
            return self.not_native("\\includegraphics with a bounding box");
        }
        let Some(keys) = self.graphics_keys(keys.as_deref().unwrap_or(&[])) else { return };
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        if arg.iter().any(|t| t.cat().is_none()) {
            return self.not_native("\\includegraphics of a file named with commands");
        }
        let name: String = arg.iter().map(|t| t.chr()).collect();
        let Some((file, pdf)) = self.graphics_file(name.trim()) else { return };
        // `\Gread@pdftex`: `\pdfximage` once per file and attributes (`\Gread@pdf` asks
        // for the crop box and a transparency group, `\Gread@png` for neither).
        let key = if pdf { format!("{file} image:cropbox") } else { format!("{file} image") };
        let k = match self.pdf_doc.graphics.get(&key) {
            Some(&k) => k,
            None => {
                let attr = if pdf { "/Group<</S/Transparency/K false/I false>>" } else { "" };
                let Some(k) = self.image_file(file, attr.to_string(), [None; 3], 1, 1, false) else {
                    return self.not_native("an image file that cannot be read");
                };
                self.pdf_doc.graphics.insert(key, k);
                k
            }
        };
        let [w0, h0, d0] = self.pdf_doc.images[k].dims;
        // `\Gin@urx`, `\Gin@ury`, then `\Gin@nat@width`, `\Gin@nat@height` (`\Gin@llx`,
        // `\Gin@lly` are 0).
        let (Some(urx), Some(ury)) = (default_bp(w0), default_bp(h0)) else { return self.not_native("an image too large") };
        let (Some(nat_w), Some(nat_h)) = (bp(&urx), bp(&ury)) else { return self.not_native("an image too large") };
        // `\Gin@req@sizes` as the keys and `\Gin@esetsize` define it.
        let width = keys.width.map(|v| self.scan_length(v));
        let height = keys.height.map(|v| self.scan_length(v));
        let scaled_key = keys.scale.is_some();
        let (sx, sy, req_w, req_h) = match (width, height, keys.scale) {
            (None, None, None) => (Some("1".to_string()), None, Some(nat_w), Some(nat_h)),
            (None, None, Some(s)) => (Some(s.clone()), None, times(&s, nat_w), times(&s, nat_h)),
            (Some(w), None, None) => {
                let sx = scale_div(w, nat_w);
                let h = sx.as_deref().and_then(|s| times(s, nat_h));
                (sx, None, Some(w), h)
            }
            (None, Some(h), None) => {
                let sy = scale_div(h, nat_h);
                let w = sy.as_deref().and_then(|s| times(s, nat_w));
                (None, sy, w, Some(h))
            }
            (Some(w), Some(h), None) => {
                let (Some(mut sx), Some(mut sy)) = (scale_div(w, nat_w), scale_div(h, nat_h)) else { return self.not_native("an image of size 0") };
                if keys.iso {
                    if times(&sy, 65536) > times(&sx, 65536) {
                        sy = sx.clone();
                    } else {
                        sx = sy.clone();
                    }
                }
                let (rw, rh) = (times(&sx, nat_w), times(&sy, nat_h));
                (Some(sx), Some(sy), rw, rh)
            }
            _ => return self.not_native("\\includegraphics with scale and a size"),
        };
        // `\GPT@setup@scale`: a missing factor is the other one.
        let (Some(sx), Some(sy)) = (sx.clone().or_else(|| sy.clone()), sy.or(sx)) else { return self.not_native("an image of size 0") };
        let (Some(req_w), Some(req_h)) = (req_w, req_h) else { return self.not_native("an image of size 0") };
        let (Some(scaled_w), Some(ht2), Some(dp2)) = (times(&sx, w0), times(&sy, h0), times(&sy, d0)) else {
            return self.not_native("an image scaled out of range");
        };
        // `\Gscale@box{sx}[sy]{\pdfrefximage n}`: box 0 the image, box 2 `\Gscale@start
        // \rlap{\copy\z@}\Gscale@end` with box 0's height and depth scaled.
        let start = self.cur.len();
        self.ref_image(k);
        let b0 = self.hpack(start, 0, false);
        let matrix = format!("{} 0 0 {}", normalize(&sx), normalize(&sy));
        let identity = matrix == "1 0 0 1";
        let start = self.cur.len();
        if !identity {
            self.cur.push(Node { kind: Kind::Whatsit, sub: SAVE, font: 0, a: 0, b: 0 });
            let toks: Vec<Token> = matrix.chars().map(|c| if c == ' ' { Token::char(Cat::Space, ' ') } else { Token::char(Cat::Other, c) }).collect();
            let span = self.e.toks.store(&toks);
            self.cur.push(Node { kind: Kind::Whatsit, sub: SET_MATRIX, font: 0, a: span.start as i32, b: span.end as i32 });
        }
        let hss = Glue { stretch: 65536, stretch_order: 1, shrink: 65536, shrink_order: 1, ..Glue::default() };
        let rlap = self.cur.len();
        let b0 = self.e.nodes.add_box(b0);
        self.cur.push(Node::new(Kind::Box, b0 as i32));
        let n = self.e.nodes.glue_node(hss, 0);
        self.cur.push(n);
        let r = self.hpack(rlap, 0, true);
        let r = self.e.nodes.add_box(r);
        self.cur.push(Node::new(Kind::Box, r as i32));
        if !identity {
            self.cur.push(Node { kind: Kind::Whatsit, sub: RESTORE, font: 0, a: 0, b: 0 });
        }
        let mut b2 = self.hpack(start, 0, false);
        (b2.height, b2.depth) = (ht2, dp2);
        // `\hb@xt@ sx\wd\z@{\box\tw@\kern sx\wd\z@\hss}`.
        let start = self.cur.len();
        let b2 = self.e.nodes.add_box(b2);
        self.cur.push(Node::new(Kind::Box, b2 as i32));
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, scaled_w) });
        let n = self.e.nodes.glue_node(hss, 0);
        self.cur.push(n);
        let scaled = self.hpack(start, scaled_w, true);
        // `\Ginclude@@pdftex`'s `\hbox`, inside `\Gin@setfile`'s `\setbox\z@\hbox{…}`.
        let mut b = scaled;
        for _ in 0..2 {
            let start = self.cur.len();
            let id = self.e.nodes.add_box(b);
            self.cur.push(Node::new(Kind::Box, id as i32));
            b = self.hpack(start, 0, false);
        }
        // `\dp\z@\z@ \ht\z@\Gin@req@height \wd\z@\Gin@req@width \box\z@`.
        (b.depth, b.height, b.width) = (0, req_h, req_w);
        // With `scale`, `\Gin@esetsize` puts it in `\Gscale@@box` with `!` for both sizes:
        // an `\mbox`.
        if scaled_key {
            let start = self.cur.len();
            let id = self.e.nodes.add_box(b);
            self.cur.push(Node::new(Kind::Box, id as i32));
            b = self.hpack(start, 0, false);
        }
        let id = self.e.nodes.add_box(b);
        self.cur.push(Node::new(Kind::Box, id as i32));
    }

    /// `\setkeys{Gin}` for the keys the native job takes; `None` (and the engine) for
    /// any other, or for `scale` given twice (which `\Gin@esetsize` scales as a box).
    fn graphics_keys(&mut self, list: &[Token]) -> Option<Keys> {
        let mut keys = Keys::default();
        let mut parts: Vec<Vec<Token>> = vec![Vec::new()];
        let mut depth = 0;
        for &t in list {
            match t.cat() {
                Some(Cat::BeginGroup) => depth += 1,
                Some(Cat::EndGroup) => depth -= 1,
                _ if depth == 0 && t == Token::char(Cat::Other, ',') => {
                    parts.push(Vec::new());
                    continue;
                }
                _ => {}
            }
            parts.last_mut()?.push(t);
        }
        let trim = |v: &[Token]| -> Vec<Token> {
            let is_space = |t: &Token| t.cat() == Some(Cat::Space);
            let a = v.iter().position(|t| !is_space(t)).unwrap_or(v.len());
            let b = v.iter().rposition(|t| !is_space(t)).map_or(a, |k| k + 1);
            let v = &v[a..b.max(a)];
            // keyval removes one pair of braces around a whole value.
            if v.len() >= 2 && v[0].cat() == Some(Cat::BeginGroup) && v[v.len() - 1].cat() == Some(Cat::EndGroup) {
                let mut d = 0;
                let whole = v.iter().enumerate().all(|(k, t)| {
                    match t.cat() {
                        Some(Cat::BeginGroup) => d += 1,
                        Some(Cat::EndGroup) => d -= 1,
                        _ => {}
                    }
                    d > 0 || k == v.len() - 1
                });
                if whole {
                    return v[1..v.len() - 1].to_vec();
                }
            }
            v.to_vec()
        };
        for part in parts {
            let eq = part.iter().position(|&t| t == Token::char(Cat::Other, '='));
            let (k, v) = match eq {
                Some(i) => (trim(&part[..i]), Some(trim(&part[i + 1..]))),
                None => (trim(&part), None),
            };
            if k.is_empty() && v.is_none() {
                continue;
            }
            if k.iter().any(|t| t.cat().is_none()) {
                self.not_native("an \\includegraphics key named with commands");
                return None;
            }
            let name: String = k.iter().map(|t| t.chr()).collect();
            let text = |v: &Option<Vec<Token>>| v.as_ref().filter(|v| v.iter().all(|t| t.cat().is_some())).map(|v| v.iter().map(|t| t.chr()).collect::<String>());
            match (name.as_str(), &v) {
                ("width", Some(v)) => keys.width = Some(v.clone()),
                ("height", Some(v)) => keys.height = Some(v.clone()),
                ("scale", Some(_)) if keys.scale.is_none() && text(&v).as_deref().and_then(decimal).is_some() => keys.scale = text(&v),
                ("keepaspectratio", _) => match text(&v).map(|s| s.to_ascii_lowercase()).as_deref() {
                    None | Some("true") => keys.iso = true,
                    Some("false") => keys.iso = false,
                    _ => {
                        self.not_native("a keepaspectratio value");
                        return None;
                    }
                },
                _ => {
                    self.not_native(&format!("the \\includegraphics key {name}"));
                    return None;
                }
            }
        }
        Some(keys)
    }

    /// `\Ginclude@graphics`' search: the name as given when its extension has a rule,
    /// else the name with each of `\Gin@extensions` in turn. The file and whether it is
    /// a PDF; `None` (and the engine) for a file not found or one of another type.
    fn graphics_file(&mut self, name: &str) -> Option<(String, bool)> {
        let base = name.rsplit('/').next().unwrap_or(name);
        if let Some(dot) = base.rfind('.') {
            let ext = &base[dot..];
            if let Some(&(_, kind)) = EXTENSIONS.iter().find(|e| e.0 == ext) {
                match kind {
                    Some(pdf) if self.open_bytes(name).is_some() => return Some((name.to_string(), pdf)),
                    Some(_) => {}
                    None => {
                        self.not_native("an image type the native job does not read");
                        return None;
                    }
                }
            }
        }
        for &(ext, kind) in &EXTENSIONS {
            let file = format!("{name}{ext}");
            if self.open_bytes(&file).is_some() {
                let Some(pdf) = kind else {
                    self.not_native("an image type the native job does not read");
                    return None;
                };
                return Some((file, pdf));
            }
        }
        self.not_native("an image file not found");
        None
    }

    /// `\setlength{<register>}{value}`: the value read by TeX's own scanner.
    fn scan_length(&mut self, mut value: Vec<Token>) -> i32 {
        let relax = Token::cs(self.e.cs.intern("relax"));
        value.push(relax);
        self.back_list(&value);
        let (v, _) = self.scan_dimen(false);
        if let Some(t) = self.get_token() {
            if t != relax {
                self.input.back(t);
            }
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphics_arithmetic() {
        assert_eq!(normalize("0.35"), ".35");
        assert_eq!(normalize("1"), "1");
        assert_eq!(normalize("2.500"), "2.5");
        assert_eq!(normalize("0"), "0");
        assert_eq!(strip_pt(65536), "1");
        assert_eq!(strip_pt(32768), "0.5");
        // 72bp is one inch, 72.27pt.
        assert_eq!(bp("72"), Some(4736286));
        assert_eq!(scale_div(65536, 65536).as_deref(), Some("1"));
        assert_eq!(scale_div(32768, 65536).as_deref(), Some("0.5"));
    }
}
