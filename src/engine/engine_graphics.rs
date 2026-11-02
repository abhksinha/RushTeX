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
use super::engine_node::{BoxRec, Kind, Node};
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
pub(super) fn times(text: &str, v: i32) -> Option<i32> {
    let (i, f) = decimal(text)?;
    let r = times_unit(i, f, v as i64);
    (r.abs() < 1 << 30).then_some(r as i32)
}

/// `<optional signs><decimal><internal dimen>` (§441, §448, §455): the factor's magnitude
/// times `v`, negated for an odd number of `-` signs (§461).
pub(crate) fn signed_times(text: &str, v: i32) -> Option<i32> {
    let t = text.trim_start();
    let digits = t.trim_start_matches(['-', '+', ' ']);
    let neg = t[..t.len() - digits.len()].bytes().filter(|&c| c == b'-').count() % 2 == 1;
    let r = times(digits, v)?;
    Some(if neg { -r } else { r })
}

/// An angle as trig.sty names its values (`\number#1`): an integer, optionally signed.
pub(crate) fn angle_number(text: &str) -> Option<i32> {
    let t = text.trim();
    let digits = t.strip_prefix('-').unwrap_or(t);
    if digits.is_empty() || digits.len() > 5 || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    t.parse().ok()
}

/// trig.sty's `\CalculateSin{n}` read back by `\UseSin`: the values it predefines for the
/// multiples of 90 up to 360 (`1 `, `0 `, `-1 `), else `\TG@@sin` of `n\p@` (`\TG@reduce`
/// into -90..90, then `\TG@series`: radians as `71/4068` of a degree, the Taylor terms
/// over 5040, each step TeX's own `\divide` and decimal-times-dimension), `\TG@rem@pt`.
/// `\CalculateCos{n}` is this of `90 - n`.
pub(crate) fn trig_sin(n: i32) -> Option<String> {
    if n % 90 == 0 && n.abs() <= 360 {
        return Some(["0", "1", "0", "-1"][(n.rem_euclid(360) / 90) as usize].to_string());
    }
    let mut d = n.checked_mul(65536).filter(|d| d.abs() < 0x4000_0000)?;
    let p = 65536;
    loop {
        if d > 90 * p {
            d = -(d - 180 * p);
        } else if d < -90 * p {
            d = -(d + 180 * p);
        } else {
            break;
        }
    }
    let rem = |v: i32| {
        let mut s = String::new();
        print_scaled(v, &mut s);
        s
    };
    d = d.checked_mul(71)? / 4068;
    let a = rem(d);
    d = signed_times(&a, d)?;
    let b = rem(d);
    d = d / 72 - p;
    for c in [42, -840, 5040] {
        d = signed_times(&b, d)? + c * p;
    }
    d = signed_times(&a, d)? / 5040;
    Some(rem(d))
}

/// `<optional signs><decimal>pt` (§448, §453): `None` for anything else.
fn pt_of(text: &str) -> Option<i32> {
    let t = text.trim_start();
    let digits = t.trim_start_matches(['-', '+', ' ']);
    let neg = t[..t.len() - digits.len()].bytes().filter(|&c| c == b'-').count() % 2 == 1;
    let (i, f) = decimal(digits)?;
    let v = i.checked_mul(65536)? + f as i64;
    (v < 0x4000_0000).then_some(if neg { -v } else { v } as i32)
}

/// `<optional signs><decimal>bp`: [`bp`] of the magnitude, negated for an odd number of `-`.
fn signed_bp(text: &str) -> Option<i32> {
    let t = text.trim_start();
    let digits = t.trim_start_matches(['-', '+', ' ']);
    let neg = t[..t.len() - digits.len()].bytes().filter(|&c| c == b'-').count() % 2 == 1;
    let v = bp(digits)?;
    Some(if neg { -v } else { v })
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
pub(crate) fn scale_div(num: i32, den: i32) -> Option<String> {
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
    /// `angle`: the `\Gin@erotate{angle}` the image is put in (an integer, as trig.sty's
    /// `\number#1` names its values); the sizes given after it scale the rotated box
    /// (`\Gin@esetsize` with `\@tempswatrue`).
    angle: Option<i32>,
    post_width: Option<Vec<Token>>,
    post_height: Option<Vec<Token>>,
    /// `trim` (`true`) or `viewport` and its four values as `\Gin@defaultbp` leaves them
    /// (`\Gin@vllx`, `\Gin@vlly`, `\Gin@vurx`, `\Gin@vury`), and `clip`.
    vp: Option<(bool, [String; 4])>,
    clip: bool,
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
        // graphics.sty's `[llx,lly]` (`graphics` marks graphicx's keys absent).
        if keys.is_some() && self.graphics_only() {
            return self.not_native("\\includegraphics[..] of the graphics package");
        }
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
        // `\Gin@viewport@code`: `\Gin@trim` or `\Gin@viewport` on `\Gin@llx`.. in points.
        let (mut llx, mut lly, mut urx, mut ury) = ("0".to_string(), "0".to_string(), urx, ury);
        if let Some((trim, v)) = &keys.vp {
            let add = |a: &str, b: &str, sign: i32| -> Option<String> { Some(strip_pt(pt_of(a)?.checked_add(sign * pt_of(b)?)?)) };
            let r = if *trim {
                (add(&llx, &v[0], 1), add(&lly, &v[1], 1), add(&urx, &v[2], -1), add(&ury, &v[3], -1))
            } else {
                (add(&llx, &v[0], 1), add(&lly, &v[1], 1), add(&llx, &v[2], 1), add(&lly, &v[3], 1))
            };
            let (Some(a), Some(b), Some(c), Some(d)) = r else { return self.not_native("a trim out of range") };
            (llx, lly, urx, ury) = (a, b, c, d);
        }
        // `\Gin@nat@height\Gin@ury bp\advance\Gin@nat@height-\Gin@lly bp`, the width alike.
        let span = |hi: &str, lo: &str| -> Option<i32> { signed_bp(hi)?.checked_sub(signed_bp(lo)?) };
        let (Some(nat_w), Some(nat_h)) = (span(&urx, &llx), span(&ury, &lly)) else { return self.not_native("an image too large") };
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
        // `\Gscale@box{sx}[sy]{\pdfrefximage n}`: box 0 the image (`\Ginclude@@pdftex`'s
        // viewport and clip around it), then `gscale_box`.
        let _ = (w0, h0, d0);
        let start = self.cur.len();
        if let Some((_, v)) = &keys.vp {
            let Some(()) = self.viewport_content(k, v, keys.clip, [&llx, &lly, &urx, &ury]) else { return self.not_native("a trim out of range") };
        } else {
            self.ref_image(k);
        }
        let b0 = self.hpack(start, 0, false);
        let dims0 = [b0.width, b0.height, b0.depth];
        let b0 = self.e.nodes.add_box(b0);
        let Some(scaled) = self.gscale_box(b0, &sx, &sy, dims0) else {
            return self.not_native("an image scaled out of range");
        };
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
        if let Some(angle) = keys.angle {
            // `\Grot@box@std{angle}{\Ginclude@graphics{file}}`: box 0 `\hbox{{..}}`, rotated;
            // then `\Gscale@@box\Gin@eresize{w}{h}` of it: `\mbox` when no size came after
            // the angle, else `\@begin@tempboxa\hbox{..}` scaled to the sizes.
            let inner = self.e.nodes.add_box(b);
            let b0 = self.hbox_of(inner);
            let Some(rot) = self.grot_box(b0, angle) else { return self.not_native("an image rotated out of range") };
            let rot = self.e.nodes.add_box(rot);
            let (pw, ph) = (keys.post_width.map(|v| self.scan_length(v)), keys.post_height.map(|v| self.scan_length(v)));
            b = match (pw, ph) {
                (None, None) => {
                    let id = self.hbox_of(rot);
                    self.e.nodes.boxes[id as usize]
                }
                _ => {
                    let temp = self.hbox_of(rot);
                    let r = self.e.nodes.boxes[temp as usize];
                    let sx = pw.map(|w| scale_div(w, r.width));
                    let sy = ph.map(|h| scale_div(h, r.height));
                    let (sx, sy) = match (sx, sy) {
                        (Some(Some(a)), None) | (None, Some(Some(a))) => (a.clone(), a),
                        (Some(Some(mut a)), Some(Some(mut c))) => {
                            if keys.iso {
                                if times(&a, 65536) > times(&c, 65536) {
                                    a = c.clone();
                                } else {
                                    c = a.clone();
                                }
                            }
                            (a, c)
                        }
                        _ => return self.not_native("an image of size 0"),
                    };
                    // `\Gscale@box\@tempa[\@tempb]{\box\@tempboxa}`: box 0 `\hbox{{\box\@tempboxa}}`.
                    let b0 = self.hbox_of(temp);
                    let r0 = self.e.nodes.boxes[b0 as usize];
                    let Some(g) = self.gscale_box(b0, &sx, &sy, [r0.width, r0.height, r0.depth]) else { return self.not_native("an image scaled out of range") };
                    g
                }
            };
        }
        let id = self.e.nodes.add_box(b);
        // §1076: a box appended in a paragraph sets the space factor to 1000.
        self.aux = 1000;
        self.cur.push(Node::new(Kind::Box, id as i32));
    }

    /// graphics.sty's `\Gscale@box{sx}[sy]{..}` once box 0 is made (`dims`: its width, height
    /// and depth; `inner`: the box `\copy\z@` puts in the `\rlap`): box 2 `\hbox{\Gscale@start
    /// \rlap{\copy\z@}\Gscale@end}` with its height and depth the factor times box 0's (the
    /// other one's, negated, when `sy` is negative), then `\hb@xt@ sx\wd\z@{\box\tw@\kern
    /// sx\wd\z@\hss}` (for a negative `sx`, `\hb@xt@-sx\wd\z@{\kern-sx\wd\z@\box\tw@\hss}`).
    /// pdftex.def's `\Gscale@start` is `\pdfsave\pdfsetmatrix{sx 0 0 sy}` of the normalized
    /// factors (`\GPT@NormalizeNumber`), nothing for the identity. `None` for a factor TeX
    /// would not read as a decimal, or a size out of range.
    pub(crate) fn gscale_box(&mut self, inner: u32, sx: &str, sy: &str, dims: [i32; 3]) -> Option<BoxRec> {
        let [w0, h0, d0] = dims;
        let (neg_x, neg_y) = (signed_times(sx, 65536)? < 0, signed_times(sy, 65536)? < 0);
        let (ht2, dp2) = if neg_y { (-signed_times(sy, d0)?, -signed_times(sy, h0)?) } else { (signed_times(sy, h0)?, signed_times(sy, d0)?) };
        let w = signed_times(sx, w0)?;
        let w = if neg_x { -w } else { w };
        let matrix = format!("{} 0 0 {}", normalize(sx), normalize(sy));
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
        self.cur.push(Node::new(Kind::Box, inner as i32));
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
        let start = self.cur.len();
        let b2 = self.e.nodes.add_box(b2);
        if neg_x {
            self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, w) });
        }
        self.cur.push(Node::new(Kind::Box, b2 as i32));
        if !neg_x {
            self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, w) });
        }
        let n = self.e.nodes.glue_node(hss, 0);
        self.cur.push(n);
        Some(self.hpack(start, w, true))
    }

    /// `\Gread@parse@vp#1 #2 #3 #4 #5\\` of a `trim` or `viewport` value, each through
    /// `\Gin@defaultbp`: a number alone stays as written (big points), a length becomes
    /// `.99626` of it in points, stripped.
    fn viewport_values(&mut self, v: &[Token]) -> Option<[String; 4]> {
        let mut parts: Vec<Vec<Token>> = vec![Vec::new()];
        for &t in v {
            if t.cat() == Some(Cat::Space) {
                if !parts.last()?.is_empty() {
                    parts.push(Vec::new());
                }
            } else if matches!(t.cat(), Some(Cat::BeginGroup | Cat::EndGroup)) {
                return None;
            } else {
                parts.last_mut()?.push(t);
            }
        }
        parts.retain(|p| !p.is_empty());
        if parts.len() != 4 {
            return None;
        }
        let mut out: [String; 4] = Default::default();
        for (o, p) in out.iter_mut().zip(parts) {
            let text: Option<String> = p.iter().all(|t| t.cat().is_some()).then(|| p.iter().map(|t| t.chr()).collect());
            *o = match text {
                Some(t) if pt_of(&t).is_some() => t,
                _ => {
                    let v = self.scan_length(p);
                    times(".99626", v.checked_abs()?).map(|m| strip_pt(if v < 0 { -m } else { m }))?
                }
            };
        }
        Some(out)
    }

    /// The content `\Ginclude@@pdftex` puts in box 0 with a viewport: `\lower\Gin@vlly bp
    /// \hbox{..}` (unless zero) around `\hskip-\Gin@vllx bp` (unless zero) and the image; with
    /// `clip`, that in `\@tempboxa`, its depth zero, its width and height `\Gin@urx` less
    /// `\Gin@llx` (`\Gin@ury` less `\Gin@lly`) big points (`\GPT@DoClipEnd`), made a form
    /// (`\pdfxform`) and referred to (`\pdfrefxform`).
    fn viewport_content(&mut self, k: usize, v: &[String; 4], clip: bool, b: [&String; 4]) -> Option<()> {
        let (vx, vy) = (pt_of(&v[0])?, pt_of(&v[1])?);
        let outer = self.cur.len();
        let inner = self.cur.len();
        if vx != 0 {
            let g = Glue { width: signed_bp(&v[0])?.checked_neg()?, ..Glue::default() };
            let n = self.e.nodes.glue_node(g, 0);
            self.cur.push(n);
        }
        self.ref_image(k);
        if vy != 0 {
            let h = self.hpack(inner, 0, false);
            let h = self.e.nodes.add_box(h);
            self.e.nodes.boxes[h as usize].shift = signed_bp(&v[1])?;
            self.cur.push(Node::new(Kind::Box, h as i32));
        }
        if clip {
            let mut t = self.hpack(outer, 0, false);
            let diff = |hi: &String, lo: &String| -> Option<i32> { signed_bp(&strip_pt(pt_of(hi)?.checked_sub(pt_of(lo)?)?)) };
            (t.depth, t.width, t.height) = (0, diff(b[2], b[0])?, diff(b[3], b[1])?);
            let dims = [t.width, t.height, t.depth];
            let id = self.e.nodes.add_box(t);
            let obj = self.pdf_doc.new_obj();
            self.pdf_doc.forms.push(super::engine_xform::XForm { obj, box_rec: id, dims, attr: String::new(), resources: String::new(), shipped: false });
            self.pdf_doc.last_xform = obj;
            let a = self.e.nodes.add_rule(dims) as i32;
            let f = self.pdf_doc.forms.len() as i32 - 1;
            self.cur.push(Node { kind: Kind::Whatsit, sub: super::engine_whatsit::XFORM, font: 0, a, b: f });
        }
        Some(())
    }

    /// A new `\hbox` holding box `b` alone, at its natural size.
    pub(crate) fn hbox_of(&mut self, b: u32) -> u32 {
        let start = self.cur.len();
        self.cur.push(Node::new(Kind::Box, b as i32));
        let h = self.hpack(start, 0, false);
        self.e.nodes.add_box(h)
    }

    /// graphics.sty's `\Grot@box` with the origin at the reference point (`\Grot@x`,
    /// `\Grot@y` zero, `\rotatebox`'s and `angle`'s default) of box 0 (`b0`, used as it is):
    /// the four corners' extremes by the signs of the sine and cosine (`\Grot@Px`, `\Grot@Py`
    /// in TeX's decimal-times-dimension), then `\hbox{\kern-left\raise0pt\hbox{\Grot@start
    /// \box\z@\Grot@end}}` with the new height, depth and width. pdftex.def's `\Grot@start`:
    /// `\pdfsave\pdfsetmatrix{cos sin -sin cos}` and box 0's width made zero, nothing for
    /// the identity.
    pub(crate) fn grot_box(&mut self, b0: u32, angle: i32) -> Option<BoxRec> {
        let (sin, cos) = (trig_sin(angle)?, trig_sin(90i32.checked_sub(angle)?)?);
        let r = self.e.nodes.boxes[b0 as usize];
        let (rr, l, h, d) = (r.width, 0, r.height, -r.depth);
        let px = |a: i32, b: i32| -> Option<i32> { signed_times(&cos, a)?.checked_add(signed_times(&format!("-{sin}"), b)?) };
        let py = |a: i32, b: i32| -> Option<i32> { signed_times(&sin, a)?.checked_add(signed_times(&cos, b)?) };
        let (s_pos, c_pos, c_neg) = (signed_times(&sin, 65536)? > 0, signed_times(&cos, 65536)? > 0, signed_times(&cos, 65536)? < 0);
        let (height, right, left, depth) = match (s_pos, c_pos, c_neg) {
            (true, true, _) => (py(rr, h)?, px(rr, d)?, px(l, h)?, py(l, d)?),
            (true, false, _) => (py(rr, d)?, px(l, d)?, px(rr, h)?, py(l, h)?),
            (false, _, true) => (py(l, d)?, px(l, h)?, px(rr, d)?, py(rr, h)?),
            (false, _, false) => (py(l, h)?, px(rr, h)?, px(l, d)?, py(rr, d)?),
        };
        let (c, s) = (normalize(&cos), normalize(&sin));
        let neg_s = match s.strip_prefix('-') {
            Some(rest) => rest.to_string(),
            None if s == "0" => s.clone(),
            None => format!("-{s}"),
        };
        let matrix = format!("{c} {s} {neg_s} {c}");
        let identity = matrix == "1 0 0 1";
        let start = self.cur.len();
        if !identity {
            self.cur.push(Node { kind: Kind::Whatsit, sub: SAVE, font: 0, a: 0, b: 0 });
            let toks: Vec<Token> = matrix.chars().map(|c| if c == ' ' { Token::char(Cat::Space, ' ') } else { Token::char(Cat::Other, c) }).collect();
            let span = self.e.toks.store(&toks);
            self.cur.push(Node { kind: Kind::Whatsit, sub: SET_MATRIX, font: 0, a: span.start as i32, b: span.end as i32 });
            self.e.nodes.boxes[b0 as usize].width = 0;
        }
        self.cur.push(Node::new(Kind::Box, b0 as i32));
        if !identity {
            self.cur.push(Node { kind: Kind::Whatsit, sub: RESTORE, font: 0, a: 0, b: 0 });
        }
        let inner = self.hpack(start, 0, false);
        let inner = self.e.nodes.add_box(inner);
        let start = self.cur.len();
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, left.checked_neg()?) });
        self.cur.push(Node::new(Kind::Box, inner as i32));
        let mut b = self.hpack(start, 0, false);
        (b.height, b.depth, b.width) = (height, depth.checked_neg()?, right.checked_sub(left)?);
        Some(b)
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
                ("width", Some(v)) if keys.angle.is_some() => keys.post_width = Some(v.clone()),
                ("height", Some(v)) if keys.angle.is_some() => keys.post_height = Some(v.clone()),
                ("width", Some(v)) => keys.width = Some(v.clone()),
                ("height", Some(v)) => keys.height = Some(v.clone()),
                ("scale", Some(_)) if keys.scale.is_none() && keys.angle.is_none() && text(&v).as_deref().and_then(decimal).is_some() => keys.scale = text(&v),
                ("angle", Some(_)) if keys.angle.is_none() && keys.scale.is_none() && text(&v).as_deref().and_then(angle_number).is_some() => keys.angle = text(&v).as_deref().and_then(angle_number),
                ("trim" | "viewport", Some(v)) if keys.vp.is_none() => {
                    let Some(vals) = self.viewport_values(v) else {
                        self.not_native("a trim or viewport value");
                        return None;
                    };
                    keys.vp = Some((name == "trim", vals));
                }
                ("clip", _) => match text(&v).map(|s| s.to_ascii_lowercase()).as_deref() {
                    None | Some("true") => keys.clip = true,
                    Some("false") => keys.clip = false,
                    _ => {
                        self.not_native("a clip value");
                        return None;
                    }
                },
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
    /// Each try is `\IfFileExists`: the name, then each `\graphicspath` prefix before it
    /// (`\input@path` is `\Ginput@path` there, `graphics_paths`).
    fn graphics_file(&mut self, name: &str) -> Option<(String, bool)> {
        let paths = self.graphics_paths();
        let find = |m: &Self, f: &str| -> Option<String> {
            if m.open_bytes(f).is_some() {
                return Some(f.to_string());
            }
            paths.iter().map(|p| format!("{p}{f}")).find(|g| m.open_bytes(g).is_some())
        };
        let base = name.rsplit('/').next().unwrap_or(name);
        if let Some(dot) = base.rfind('.') {
            let ext = &base[dot..];
            if let Some(&(_, kind)) = EXTENSIONS.iter().find(|e| e.0 == ext) {
                match kind {
                    Some(pdf) => {
                        if let Some(f) = find(self, name) {
                            return Some((f, pdf));
                        }
                    }
                    None => {
                        self.not_native("an image type the native job does not read");
                        return None;
                    }
                }
            }
        }
        for &(ext, kind) in &EXTENSIONS {
            if let Some(file) = find(self, &format!("{name}{ext}")) {
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
