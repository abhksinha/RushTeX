//! LaTeX's `picture` environment natively, as latex.ltx (2026) defines it: `\begin{picture}`,
//! `\put`, `\multiput`, `\line`, `\vector`, `\thinlines`, `\thicklines`, `\linethickness`
//! and `\unitlength`.
//!
//! The environment, `\put` and `\multiput` become the primitives LaTeX's macros leave
//! (`\setbox\@picbox\hb@xt@..`, `\raise..\hb@xt@\z@{\kern..#3\hss}`), each coordinate
//! scanned as `\@defaultunitsset` scans it (`\dimexpr#1\unitlength\relax`: a number is a
//! factor of `\unitlength`, a length is taken as it is). `\line` and `\vector` build their
//! nodes directly: horizontal and vertical lines are rules of `\@wholewidth`; a sloped line
//! is `\@sline`'s run of characters of the line font (`\tenln`, `line10`, or `\tenlnw`,
//! `linew10`, after `\thicklines`), each in its own box raised or lowered, with the last
//! piece placed by `\@sline`'s integer arithmetic; arrow heads are characters of the same
//! font (`\@getlarrow`, `\@getrarrow`).
//!
//! `\shortstack`, `\dashbox`, `\circle`, `\oval` and `\qbezier` are the tokens their
//! macros leave, their counts (dashes, the circle font's characters, `\@bezier`'s dots)
//! computed in Rust with the macros' integer arithmetic.
//!
//! Every branch that ends in `\@badlinearg` (a slope outside the font, a negative length) or
//! `\@picture@warn` (a sloped line shorter than one character, a circle outside the circle
//! font) declines.

use super::super::engine_box::{fil_glue, Ctx};
use super::super::engine_pack::EVERY_HBOX;
use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::{Glue, Reg, Region};
use super::super::engine_node::{Kind, Node};
use super::super::engine_prim::{Meaning, Prim};
use super::super::engine_run::{Machine, Mode};
use super::super::engine_token::Token;
use crate::backend::backend_scaled::xn_over_d;

/// `\unitlength` (1pt), `\@wholewidth`, `\@halfwidth` and `\@picht`, hidden registers; the
/// first under its name, so `\setlength{\unitlength}{..}` sets it.
pub(super) const UNITLENGTH: Reg = Reg::register(Region::Dimen, 32712);
const WHOLEWIDTH: Reg = Reg::register(Region::Dimen, 32713);
const HALFWIDTH: Reg = Reg::register(Region::Dimen, 32714);
const PICHT: Reg = Reg::register(Region::Dimen, 32715);
/// `\@linefnt`: 0 for `\tenln` (`\thinlines`), 1 for `\tenlnw` (`\thicklines`).
const THICK: Reg = Reg::register(Region::Int, 32717);
/// `\@picbox`.
const PIC_BOX: u32 = 32722;
/// `\@tempboxa` of `\circle` and `\oval`.
const CIRC_BOX: u32 = 32723;

/// The natives in `DOC` (`DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@picture", |m| m.picture_begin()),
    ("rt@endpicture", |m| m.picture_end()),
    ("put", |m| m.picture_put()),
    ("multiput", |m| m.picture_multiput()),
    ("line", |m| m.picture_line(false)),
    ("vector", |m| m.picture_line(true)),
    ("thinlines", |m| m.picture_lines(false)),
    ("thicklines", |m| m.picture_lines(true)),
    ("linethickness", |m| m.picture_thickness()),
    ("shortstack", |m| m.picture_shortstack()),
    ("dashbox", |m| m.picture_dashbox()),
    ("circle", |m| m.picture_circle()),
    ("oval", |m| m.picture_oval()),
    ("qbezier", |m| m.picture_qbezier()),
];

impl Machine<'_, '_> {
    /// The registers as latex.ltx leaves them: `\unitlength=1pt`, `\@wholewidth\fontdimen8
    /// \tenln`, `\@halfwidth .5\@wholewidth`, `\@linefnt` `\tenln`.
    pub(super) fn picture_init(&mut self) {
        self.e.eqtb.set_word(UNITLENGTH, 65536, true);
        let id = self.e.cs.intern("unitlength");
        self.e.eqtb.set_meaning(id, Meaning::Reg(UNITLENGTH), true);
        self.e.eqtb.set_word(THICK, 0, true);
        let f = self.line_font();
        let w = self.e.fonts.params(f).get(7).copied().unwrap_or(0);
        self.e.eqtb.set_word(WHOLEWIDTH, w, true);
        self.e.eqtb.set_word(HALFWIDTH, xn_over_d(w, 32768, 65536), true);
    }

    /// `\@linefnt`: `line10` or `linew10` at 10pt, loaded by the format.
    fn line_font(&mut self) -> u16 {
        let name = if self.e.eqtb.word(THICK) != 0 { "linew10" } else { "line10" };
        self.find_font(name, -1000, None)
    }

    /// `(#1,#2)` after a command: the two arguments delimited by `,` and `)`, as TeX matches
    /// them (§392: the first delimiter outside braces; a lone braced group loses its braces).
    /// `None` when `(` does not come next, which LaTeX reports as an error.
    fn picture_pair(&mut self, skip_spaces: bool) -> Option<(Vec<Token>, Vec<Token>)> {
        let open = if skip_spaces { self.next_unblank() } else { self.get_token() };
        if open != Some(Token::char(Cat::Other, '(')) {
            if let Some(t) = open {
                self.input.back(t);
            }
            return None;
        }
        let a = self.picture_delimited(',');
        let b = self.picture_delimited(')');
        Some((a?, b?))
    }

    fn picture_delimited(&mut self, stop: char) -> Option<Vec<Token>> {
        let mut arg = Vec::new();
        let mut depth = 0;
        loop {
            let t = self.get_token()?;
            match t.cat() {
                _ if depth == 0 && t == Token::char(Cat::Other, stop) => break,
                Some(Cat::BeginGroup) => depth += 1,
                Some(Cat::EndGroup) => depth -= 1,
                _ => {}
            }
            arg.push(t);
        }
        // §400: `{x}` alone is `x`.
        if arg.len() >= 2 && arg[0].cat() == Some(Cat::BeginGroup) && arg[arg.len() - 1].cat() == Some(Cat::EndGroup) {
            let mut d = 0;
            let whole = arg.iter().enumerate().all(|(i, t)| {
                match t.cat() {
                    Some(Cat::BeginGroup) => d += 1,
                    Some(Cat::EndGroup) => d -= 1,
                    _ => {}
                }
                d > 0 || i == arg.len() - 1
            });
            if whole {
                arg.pop();
                arg.remove(0);
            }
        }
        Some(arg)
    }

    /// `\@defaultunitsset\x{#1}\unitlength`: `\x\dimexpr#1\unitlength\relax\relax\@nnil`. A
    /// number is a factor of `\unitlength`; a length stops the expression before
    /// `\unitlength`, which `\remove@to@nnil` drops. Anything else (an expression) declines.
    fn picture_length(&mut self, arg: &[Token]) -> Option<i32> {
        let relax = self.named_cs(named!("relax"));
        let unit = Token::cs(self.e.cs.intern("unitlength"));
        let mut list = Vec::with_capacity(arg.len() + 2);
        list.extend_from_slice(arg);
        list.extend([unit, relax]);
        self.back_list(&list);
        let (v, _) = self.scan_dimen(false);
        let mut next = self.get_token();
        if next == Some(unit) {
            next = self.get_token();
        }
        if next != Some(relax) {
            self.not_native("a picture coordinate the native path does not take");
            return None;
        }
        Some(v)
    }

    /// `\@xarg #1\relax`: an integer.
    fn picture_int(&mut self, arg: &[Token]) -> Option<i32> {
        let relax = self.named_cs(named!("relax"));
        let mut list = Vec::with_capacity(arg.len() + 1);
        list.extend_from_slice(arg);
        list.push(relax);
        self.back_list(&list);
        let v = self.scan_int();
        if self.get_token() != Some(relax) {
            self.not_native("a picture slope the native path does not take");
            return None;
        }
        Some(v)
    }

    /// `\begin{picture}(w,h)(x,y)` inside `\begin`'s group: `\pictur@`, `\@picture`:
    /// `\setbox\@picbox\hb@xt@w\bgroup \hskip-x \lower y\hbox\bgroup \ignorespaces`, with
    /// `\@picht` h.
    fn picture_begin(&mut self) {
        let Some((w, h)) = self.picture_pair(true) else { return self.not_native("a picture without its size") };
        // `\@ifnextchar(` skips spaces either way.
        let (x, y) = match self.picture_pair(true) {
            Some(p) => p,
            None => (vec![Token::char(Cat::Other, '0')], vec![Token::char(Cat::Other, '0')]),
        };
        let Some(ht) = self.picture_length(&h) else { return };
        let Some(wd) = self.picture_length(&w) else { return };
        let Some(x) = self.picture_length(&x) else { return };
        let Some(y) = self.picture_length(&y) else { return };
        self.e.eqtb.set_word(PICHT, ht, false);
        let open = Token::char(Cat::BeginGroup, '{');
        let mut t = vec![self.named_cs(named!("setbox"))];
        Self::picture_box_num(&mut t);
        t.push(self.named_cs(named!("hbox")));
        self.word_tokens("to", &mut t);
        Self::sp_tokens(wd, &mut t);
        t.extend([open, self.named_cs(named!("hskip"))]);
        Self::sp_tokens(x.wrapping_neg(), &mut t);
        t.push(self.named_cs(named!("lower")));
        Self::sp_tokens(y, &mut t);
        t.extend([self.named_cs(named!("hbox")), open, self.named_cs(named!("ignorespaces"))]);
        self.back_list(&t);
    }

    fn picture_box_num(t: &mut Vec<Token>) {
        t.extend(PIC_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        t.push(Token::char(Cat::Space, ' '));
    }

    /// `\endpicture`: `\egroup\hss\egroup \ht\@picbox\@picht \dp\@picbox\z@
    /// \mbox{\box\@picbox}`.
    fn picture_end(&mut self) {
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let ht = self.e.eqtb.word(PICHT);
        let mut t = vec![close, self.named_cs(named!("hss")), close, self.named_cs(named!("ht"))];
        Self::picture_box_num(&mut t);
        t.push(Token::char(Cat::Other, '='));
        Self::sp_tokens(ht, &mut t);
        t.push(self.named_cs(named!("dp")));
        Self::picture_box_num(&mut t);
        t.push(Token::char(Cat::Other, '='));
        Self::sp_tokens(0, &mut t);
        t.extend([self.named_cs(named!("mbox")), open, self.named_cs(named!("box"))]);
        Self::picture_box_num(&mut t);
        t.push(close);
        self.back_list(&t);
    }

    /// `\@killglue`: `\unskip`, then `\unskip` while `\lastskip>\z@`.
    fn kill_glue(&mut self) {
        self.unskip();
        while self.tail().is_some_and(|t| t.kind == Kind::Glue && self.e.nodes.glue(&t).width > 0) {
            self.unskip();
        }
    }

    /// `\raise y\hb@xt@\z@{\kern x <obj>\hss}`.
    fn picture_put_tokens(&mut self, x: i32, y: i32, obj: &[Token], t: &mut Vec<Token>) {
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        t.push(self.named_cs(named!("raise")));
        Self::sp_tokens(y, t);
        t.push(self.named_cs(named!("hbox")));
        self.word_tokens("to", t);
        Self::sp_tokens(0, t);
        t.extend([open, self.named_cs(named!("kern"))]);
        Self::sp_tokens(x, t);
        t.extend_from_slice(obj);
        t.extend([self.named_cs(named!("hss")), close]);
    }

    /// `\put(x,y){obj}`: `\@killglue`, the object in a box of width zero raised by `y` and
    /// moved right by `x`, then `\ignorespaces`.
    fn picture_put(&mut self) {
        if self.mode != Mode::Horizontal {
            return self.not_native("\\put outside a picture");
        }
        let Some((x, y)) = self.picture_pair(false) else { return self.not_native("\\put without its coordinates") };
        let mut obj = Vec::new();
        self.read_arg(&mut obj);
        let Some(y) = self.picture_length(&y) else { return };
        let Some(x) = self.picture_length(&x) else { return };
        self.kill_glue();
        let mut t = Vec::with_capacity(obj.len() + 24);
        self.picture_put_tokens(x, y, &obj, &mut t);
        t.push(self.named_cs(named!("ignorespaces")));
        self.back_list(&t);
    }

    /// `\multiput(x,y)(dx,dy){n}{obj}`: `\@killglue`, then `n` times the object put at
    /// `(x,y)`, both advanced by `(dx,dy)` after each, then `\ignorespaces`.
    fn picture_multiput(&mut self) {
        if self.mode != Mode::Horizontal {
            return self.not_native("\\multiput outside a picture");
        }
        let Some((x, y)) = self.picture_pair(false) else { return self.not_native("\\multiput without its coordinates") };
        let Some((dx, dy)) = self.picture_pair(false) else { return self.not_native("\\multiput without its steps") };
        let (mut n, mut obj) = (Vec::new(), Vec::new());
        self.read_arg(&mut n);
        self.read_arg(&mut obj);
        let Some(mut x) = self.picture_length(&x) else { return };
        let Some(mut y) = self.picture_length(&y) else { return };
        let Some(n) = self.picture_int(&n) else { return };
        let Some(dx) = self.picture_length(&dx) else { return };
        let Some(dy) = self.picture_length(&dy) else { return };
        if n > 1000 {
            return self.not_native("a \\multiput of more than 1000 objects");
        }
        self.kill_glue();
        let mut t = Vec::new();
        for _ in 0..n.max(0) {
            self.picture_put_tokens(x, y, &obj, &mut t);
            x = x.wrapping_add(dx);
            y = y.wrapping_add(dy);
        }
        t.push(self.named_cs(named!("ignorespaces")));
        self.back_list(&t);
    }

    /// `\thinlines`, `\thicklines`: the line font and `\@wholewidth` its `\fontdimen8`.
    pub(super) fn picture_lines(&mut self, thick: bool) {
        self.e.eqtb.set_word(THICK, thick as i32, false);
        let f = self.line_font();
        let w = self.e.fonts.params(f).get(7).copied().unwrap_or(0);
        self.set_line_width(w);
    }

    fn set_line_width(&mut self, w: i32) {
        self.e.eqtb.set_word(WHOLEWIDTH, w, false);
        self.e.eqtb.set_word(HALFWIDTH, xn_over_d(w, 32768, 65536), false);
    }

    /// `\linethickness{len}`: `\@wholewidth #1\relax \@halfwidth .5\@wholewidth
    /// \ignorespaces`.
    fn picture_thickness(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let w = self.dimen_arg(arg);
        self.set_line_width(w);
        let t = self.named_cs(named!("ignorespaces"));
        self.input.back(t);
    }

    /// `\makebox(w,h)[pos]{text}` and `\framebox(w,h)[pos]{text}` after their `(`:
    /// `\@imakepicbox`, `\vbox to h{\mb@t \hb@xt@ w{\mb@l text\mb@r}\mb@b \kern\z@}`, each
    /// `\mb@` `\vss` or `\hss` unless `pos` names it (`s` both sides), then `\relax`; the
    /// frame is `\frame`'s rules of `\@wholewidth` around it.
    pub(super) fn picture_makebox(&mut self, frame: bool) {
        let (Some(w), Some(h)) = (self.picture_delimited(','), self.picture_delimited(')')) else { return };
        let pos = if self.next_is('[') { self.bracket_arg() } else { Vec::new() };
        let mut text = Vec::new();
        self.read_arg(&mut text);
        let (mut t_, mut b_, mut l_, mut r_) = (true, true, true, true);
        for t in &pos {
            match (t.cs_id(), t.chr()) {
                (None, 't') => t_ = false,
                (None, 'b') => b_ = false,
                (None, 'l') => l_ = false,
                (None, 'r') => r_ = false,
                (None, 's') => (l_, r_) = (false, false),
                (None, ' ') => {}
                _ => return self.not_native("a picture box position other than t, b, l, r or s"),
            }
        }
        let Some(h) = self.picture_length(&h) else { return };
        let Some(w) = self.picture_length(&w) else { return };
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let relax = self.named_cs(named!("relax"));
        let (vss, hss) = (self.named_cs(named!("vss")), self.named_cs(named!("hss")));
        let whole = self.e.eqtb.word(WHOLEWIDTH);
        let mut t = Vec::with_capacity(text.len() + 64);
        if frame {
            t.extend([self.named_cs(named!("hbox")), open, self.named_cs(named!("hskip"))]);
            Self::sp_tokens(whole.wrapping_neg(), &mut t);
            t.extend([self.named_cs(named!("vbox")), open, self.named_cs(named!("vskip"))]);
            Self::sp_tokens(whole.wrapping_neg(), &mut t);
            t.push(self.named_cs(named!("hrule")));
            self.word_tokens("height", &mut t);
            Self::sp_tokens(whole, &mut t);
            t.extend([self.named_cs(named!("hbox")), open, self.named_cs(named!("vrule"))]);
            self.word_tokens("width", &mut t);
            Self::sp_tokens(whole, &mut t);
        }
        t.push(self.named_cs(named!("vbox")));
        self.word_tokens("to", &mut t);
        Self::sp_tokens(h, &mut t);
        t.extend([open, if t_ { vss } else { relax }, self.named_cs(named!("hbox"))]);
        self.word_tokens("to", &mut t);
        Self::sp_tokens(w, &mut t);
        t.extend([open, if l_ { hss } else { relax }]);
        t.extend(text);
        t.extend([if r_ { hss } else { relax }, close, if b_ { vss } else { relax }, self.named_cs(named!("kern"))]);
        Self::sp_tokens(0, &mut t);
        t.push(close);
        if frame {
            t.push(self.named_cs(named!("vrule")));
            self.word_tokens("width", &mut t);
            Self::sp_tokens(whole, &mut t);
            t.extend([close, self.named_cs(named!("hrule"))]);
            self.word_tokens("height", &mut t);
            Self::sp_tokens(whole, &mut t);
            t.push(self.named_cs(named!("vskip")));
            Self::sp_tokens(whole.wrapping_neg(), &mut t);
            t.extend([close, self.named_cs(named!("hskip"))]);
            Self::sp_tokens(whole.wrapping_neg(), &mut t);
            t.push(close);
        }
        self.back_list(&t);
    }

    /// `\shortstack[pos]{text}`: `\leavevmode\vbox\bgroup\baselineskip-\p@\lineskip3\p@`, `\\`
    /// as `\@stackcr` (`\cr\ignorespaces`), `\ialign{\mb@l{##}\unskip\mb@r\cr text\crcr}
    /// \egroup`, `\mb@l` and `\mb@r` `\hss` unless `pos` (`l`, `r`) makes one `\relax`. A
    /// `\\` with a space below declines.
    fn picture_shortstack(&mut self) {
        if self.mode == Mode::Math {
            return self.not_native("\\shortstack in a formula");
        }
        if self.leave_vmode() {
            return;
        }
        let pos = if self.next_is('[') { self.bracket_arg() } else { vec![Token::char(Cat::Letter, 'c')] };
        let (l, r) = match pos.as_slice() {
            [t] if t.cs_id().is_none() && t.chr() == 'c' => (true, true),
            [t] if t.cs_id().is_none() && t.chr() == 'l' => (false, true),
            [t] if t.cs_id().is_none() && t.chr() == 'r' => (true, false),
            _ => return self.not_native("a \\shortstack position other than c, l or r"),
        };
        let mut text = Vec::new();
        self.read_arg(&mut text);
        let cr = self.named_cs(named!("\\"));
        let begin = self.named_cs(named!("begin"));
        if text.contains(&begin) {
            return self.not_native("an environment in a \\shortstack");
        }
        let mut body = Vec::with_capacity(text.len() + 8);
        let mut k = 0;
        while k < text.len() {
            let t = text[k];
            if t == cr {
                // `\@stackcr`: `\@ifstar` and `\@ifnextchar[`, each skipping spaces; the star
                // changes nothing.
                let is = |n: Option<&Token>, c: char| n.is_some_and(|n| n.cs_id().is_none() && n.chr() == c && n.cat() != Some(Cat::Space));
                let skip = |k: &mut usize| {
                    while text.get(*k + 1).is_some_and(|n| n.cat() == Some(Cat::Space)) {
                        *k += 1;
                    }
                };
                skip(&mut k);
                if is(text.get(k + 1), '*') {
                    k += 1;
                    skip(&mut k);
                }
                if is(text.get(k + 1), '[') {
                    return self.not_native("a \\\\ with a space below it in a \\shortstack");
                }
                body.extend([self.named_cs(named!("cr")), self.named_cs(named!("ignorespaces"))]);
            } else {
                body.push(t);
            }
            k += 1;
        }
        let side = |m: &mut Self, on: bool| m.named_cs(if on { named!("hss") } else { named!("relax") });
        let mut t = self.tex_tokens("\\vbox{\\baselineskip-1pt\\lineskip3pt\\everycr{}\\tabskip0pt\\halign{");
        t.push(side(self, l));
        t.extend(self.tex_tokens("{#}\\unskip"));
        t.push(side(self, r));
        t.push(self.named_cs(named!("cr")));
        t.extend(body);
        t.extend([self.named_cs(named!("crcr")), Token::char(Cat::EndGroup, '}'), Token::char(Cat::EndGroup, '}')]);
        self.back_list(&t);
    }

    /// `\dashbox{dash}(w,h)[pos]{text}`: latex.ltx's `\leavevmode\hb@xt@\z@{\baselineskip\z@skip
    /// \lineskip\z@skip ..\@makepicbox(w,h)}`: the dashes of each side `\put` as the macro
    /// counts them (`(len+200sp)/dash` dashes; an odd count centres them, an even one puts
    /// half dashes at the corners), each `\copy\@dashbox` a rule of its own, then the
    /// picture box.
    fn picture_dashbox(&mut self) {
        if self.mode != Mode::Horizontal {
            return self.not_native("\\dashbox outside a picture");
        }
        let mut dash = Vec::new();
        self.read_arg(&mut dash);
        let Some((w, h)) = self.picture_pair(true) else { return self.not_native("\\dashbox without its size") };
        let Some(d) = self.picture_length(&dash) else { return };
        let Some(w) = self.picture_length(&w) else { return };
        let Some(h) = self.picture_length(&h) else { return };
        if d <= 0 || w < 0 || h < 0 || w / d.max(1) > 2000 || h / d.max(1) > 2000 {
            return self.not_native("a \\dashbox the native path does not take");
        }
        let (whole, half) = (self.e.eqtb.word(WHOLEWIDTH), self.e.eqtb.word(HALFWIDTH));
        let mut s = String::from("\\hbox to0pt{\\baselineskip0pt\\lineskip0pt");
        let put = |s: &mut String, x: i32, y: i32, obj: &str| s.push_str(&format!("\\put({x}sp,{y}sp){{{obj}}}"));
        let hrule = |len: i32| format!("\\hbox{{\\vrule height{half}sp depth{half}sp width{len}sp}}");
        let vrule = |len: i32| format!("\\hbox{{\\hskip{}sp\\vrule width{whole}sp height{len}sp}}", half.wrapping_neg());
        // The horizontal sides.
        let mut cnt = (w + 200) / d;
        let mut dd;
        if cnt % 2 != 0 {
            dd = 0;
            cnt = (cnt + 1) / 2;
        } else {
            dd = d / 2;
            cnt = cnt / 2 - 1;
            let b = hrule(dd);
            put(&mut s, 0, 0, &b);
            put(&mut s, 0, h, &b);
            put(&mut s, w, 0, &format!("\\hskip{}sp{b}", -dd));
            put(&mut s, w, h, &format!("\\hskip{}sp{b}", -dd));
            dd *= 3;
        }
        let dash_h = format!("\\hbox{{\\vrule height{half}sp depth{half}sp width{d}sp\\hskip{d}sp}}");
        let row: String = format!("\\hskip{dd}sp") + &dash_h.repeat(cnt.max(0) as usize);
        put(&mut s, 0, 0, &row);
        put(&mut s, 0, h, &row);
        // The vertical sides.
        let mut cnt = (h + 200) / d;
        let mut dd;
        if cnt % 2 != 0 {
            dd = 0;
            cnt = (cnt + 1) / 2;
        } else {
            dd = d / 2;
            cnt = cnt / 2 - 1;
            let b = vrule(dd);
            put(&mut s, 0, 0, &b);
            put(&mut s, w, 0, &b);
            put(&mut s, 0, h, &format!("\\lower{dd}sp{b}"));
            put(&mut s, w, h, &format!("\\lower{dd}sp{b}"));
            dd *= 3;
        }
        let dash_v = format!("\\vskip{d}sp\\hbox{{\\vrule width{whole}sp height{d}sp}}");
        let col = format!("\\hskip{}sp\\vbox{{{}\\vskip{dd}sp}}", half.wrapping_neg(), dash_v.repeat(cnt.max(0) as usize));
        put(&mut s, 0, 0, &col);
        put(&mut s, w, 0, &col);
        // The box of width zero ends before `\@makepicbox(#2,#3)`.
        s.push_str(&format!("}}\\makebox({w}sp,{h}sp)"));
        let mut t = self.tex_tokens(&s);
        // `\@makepicbox(#2,#3)` reads `[pos]{text}` from what follows.
        let mut text = Vec::new();
        if self.next_is('[') {
            t.push(Token::char(Cat::Other, '['));
            t.extend(self.bracket_arg());
            t.push(Token::char(Cat::Other, ']'));
        }
        self.read_arg(&mut text);
        t.push(Token::char(Cat::BeginGroup, '{'));
        t.extend(text);
        t.push(Token::char(Cat::EndGroup, '}'));
        self.back_list(&t);
    }

    /// `\@circlefnt`: `lcircle10` or `lcirclew10` at 10pt, and the name of its font
    /// identifier (`\tencirc`, `\tencircw`).
    fn circle_font(&mut self) -> (u16, &'static str) {
        let thick = self.e.eqtb.word(THICK) != 0;
        let f = self.find_font(if thick { "lcirclew10" } else { "lcircle10" }, -1000, None);
        (f, if thick { "\\tencircw" } else { "\\tencirc" })
    }

    /// `\@getcirc{d}`: the quarter circle of `d`: `(d + 2pt) / 4pt` less one, times four,
    /// and that character's width. Above 10 or at zero it is `\@picture@warn`: declined.
    fn picture_getcirc(&mut self, f: u16, d: i32) -> Option<(i32, [i32; 4])> {
        let n = d.saturating_add(131072) / 262144;
        if n > 10 || n <= 0 {
            self.not_native("an \\oval or \\circle size outside the circle font");
            return None;
        }
        let c = (n - 1) * 4;
        let Some(dims) = self.e.fonts.dims(f, c as u8) else {
            self.not_native("a circle font without its character");
            return None;
        };
        Some((c, dims))
    }

    /// `\circle{d}` and `\circle*{d}`. Up to 15.5pt, `\@circ`: the character `(d + .5pt) /
    /// 1pt` less one (at most 14) after 96 (open) or 112 (filled; `\@dot` changes the font
    /// with no group, as the tokens here do). Larger, `\@circle`'s four quarters of
    /// `\@getcirc` in a box of no height or depth, moved down and left by its radius.
    fn picture_circle(&mut self) {
        if self.mode != Mode::Horizontal {
            return self.not_native("\\circle outside a picture");
        }
        let star = self.next_is('*');
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let Some(d) = self.picture_length(&arg) else { return };
        let (f, font) = self.circle_font();
        if star || d <= 15 * 65536 + 32768 {
            let mut n = d.saturating_add(32768) / 65536;
            n = n.min(15);
            if n > 0 {
                n -= 1;
            }
            let c = n + if star { 112 } else { 96 };
            let s = if star { format!("{font}\\char{c} ") } else { format!("\\begingroup{font}\\char{c} \\endgroup") };
            let t = self.tex_tokens(&s);
            return self.back_list(&t);
        }
        let Some((c, [wd, ht, _, _])) = self.picture_getcirc(f, d) else { return };
        let ro = ht.max(0);
        let b = CIRC_BOX;
        let s = format!(
            "\\begingroup\\setbox{b}\\hbox{{{font}\\char{} \\char{} \\kern{}sp\\raise{wd}sp\\hbox{{\\char{} }}\\raise{wd}sp\\hbox{{\\char{c} }}}}\\ht{b}=0sp\\dp{b}=0sp\\raise{}sp\\hbox to0sp{{\\hskip{}sp\\box{b} \\hss}}\\endgroup",
            c + 2,
            c + 1,
            wd.wrapping_mul(-2),
            c + 3,
            -ro,
            -ro
        );
        let t = self.tex_tokens(&s);
        self.back_list(&t);
    }

    /// `\oval(w,h)[part]`: `\@oval`'s box of the four quarter circles of `\@getcirc` (the
    /// smaller side less 2pt) joined by leaders of rules, the parts `t`, `b`, `l`, `r` leave
    /// out, as tokens.
    fn picture_oval(&mut self) {
        if self.mode != Mode::Horizontal {
            return self.not_native("\\oval outside a picture");
        }
        let Some((xx, yy)) = self.picture_pair(true) else { return self.not_native("\\oval without its size") };
        let part = if self.next_is('[') { self.bracket_arg() } else { Vec::new() };
        let (mut ovt, mut ovb, mut ovl, mut ovr) = (true, true, true, true);
        for t in &part {
            match (t.cs_id(), t.chr()) {
                (None, 't') => ovt = false,
                (None, 'b') => ovb = false,
                (None, 'l') => ovl = false,
                (None, 'r') => ovr = false,
                _ => return self.not_native("an \\oval part other than t, b, l or r"),
            }
        }
        let Some(xx) = self.picture_length(&xx) else { return };
        let Some(yy) = self.picture_length(&yy) else { return };
        let (mut vline, mut hline) = (false, false);
        let mut dimb = if yy > xx {
            vline = true;
            xx
        } else {
            if yy != xx {
                hline = true;
            }
            yy
        };
        dimb = dimb.saturating_sub(131072);
        let (f, font) = self.circle_font();
        let Some((c, [dima, ht, dp, _])) = self.picture_getcirc(f, dimb) else { return };
        let (ro, ri) = (ht.max(0), dp.max(0));
        let dx = (xx - dima) / 2;
        let dy = (yy - dima) / 2;
        hline |= dx > 0;
        vline |= dy > 0;
        let whole = self.e.eqtb.word(WHOLEWIDTH);
        let vert = |a: i32, b: i32| {
            let mut s = format!("\\vbox to{yy}sp{{");
            if ovb {
                s += &format!("\\kern{}sp\\hbox{{\\char{} }}\\prevdepth-1000pt", -ro, c + a);
            } else {
                s += &format!("\\kern{ri}sp\\kern{dy}sp");
            }
            if vline {
                s += &format!("\\leaders\\vrule width{whole}sp");
            }
            s += "\\vfil\\prevdepth-1000pt";
            if ovt {
                s += &format!("\\hbox{{\\char{} }}}}", c + b);
            } else {
                s += &format!("\\kern{dy}sp\\kern{ro}sp}}");
            }
            s
        };
        let mut horz = format!("\\hbox to{xx}sp{{\\kern{ro}sp");
        if !ovr {
            horz += &format!("\\kern{dx}sp");
        }
        if hline {
            horz += &format!("\\leaders\\hrule height{whole}sp");
        }
        horz += "\\hfil";
        if !ovl {
            horz += &format!("\\kern{dx}sp");
        }
        horz += &format!("\\kern{ri}sp}}");
        let b = CIRC_BOX;
        let mut s = format!("\\begingroup\\boxmaxdepth1073741823sp{font}\\setbox{b}\\hbox{{");
        if ovr {
            s += &vert(3, 2);
            s += &format!("\\kern{}sp", -dima);
        }
        if ovl {
            s += &format!("\\kern{xx}sp{}\\kern{}sp\\kern{}sp", vert(0, 1), -dima, -xx);
        }
        if ovt {
            s += &format!("{horz}\\kern{}sp", -xx);
        }
        if ovb {
            s += &format!("\\raise{yy}sp{horz}");
        }
        s += &format!("}}\\ht{b}=0sp\\dp{b}=0sp\\raise{}sp\\hbox to0sp{{\\hskip{}sp\\box{b} \\hss}}\\endgroup", -(dy + ro), -(dx + ro));
        let t = self.tex_tokens(&s);
        self.back_list(&t);
    }

    /// `\qbezier[n](x1,y1)(x2,y2)(x3,y3)`: `\@bezier`'s `n + 1` dots (with `n` zero, the
    /// larger extent over `.5\@halfwidth`, at most `\qbeziermax`, 500), each a rule of
    /// `\@wholewidth` square at `\@bezier`'s integer arithmetic, inside one `\put`.
    fn picture_qbezier(&mut self) {
        if self.mode != Mode::Horizontal {
            return self.not_native("\\qbezier outside a picture");
        }
        let n = if self.next_is('[') { self.bracket_arg() } else { vec![Token::char(Cat::Other, '0')] };
        let mut p = [(0, 0); 3];
        let mut raw = Vec::with_capacity(3);
        for _ in 0..3 {
            let Some((x, y)) = self.picture_pair(true) else { return self.not_native("\\qbezier without its points") };
            raw.push((x, y));
        }
        for (k, (x, y)) in raw.iter().enumerate() {
            let Some(x) = self.picture_length(x) else { return };
            let Some(y) = self.picture_length(y) else { return };
            p[k] = (x, y);
        }
        let Some(n) = self.picture_int(&n) else { return };
        let (whole, half) = (self.e.eqtb.word(WHOLEWIDTH), self.e.eqtb.word(HALFWIDTH));
        let [(x1, y1), (x2, y2), (x3, y3)] = p;
        let m = if n == 0 {
            let xx = (x2 - x1).abs().max((x3 - x2).abs());
            let yy = (y2 - y1).abs().max((y3 - y2).abs());
            let step = xn_over_d(half, 32768, 65536);
            if step == 0 {
                return self.not_native("a \\qbezier of lines of no width");
            }
            (xx.max(yy) / step).min(500)
        } else {
            n
        };
        if m <= 0 || m > 5000 {
            return self.not_native("a \\qbezier the native path does not take");
        }
        let dx = (x2 - x1) * 2;
        let ddx = ((x3 - x1) - dx) / m;
        let dy = (y2 - y1) * 2;
        let ddy = ((y3 - y1) - dy) / m;
        let mut dots = Vec::with_capacity(m as usize + 1);
        for c in 0..=m {
            let (Some(xd), Some(yd)) = (c.checked_mul(ddx), c.checked_mul(ddy)) else { return self.not_native("a \\qbezier too large") };
            let (Some(xd), Some(yd)) = (((xd + dx) / m).checked_mul(c), ((yd + dy) / m).checked_mul(c)) else { return self.not_native("a \\qbezier too large") };
            dots.push((xd, yd));
        }
        // `\put(x1,y1){..}` of the dots, each `\raise yd\hbox to0sp{\kern xd\hskip-\@halfwidth
        // \vrule height\@halfwidth depth\@halfwidth width\@wholewidth\hss}`, built as the
        // primitives build them (`\@killglue`, `\raise` of an `\hbox to`, an explicit kern, a
        // written-out glue, a rule, `\hss`), as `picture_line` builds `\@vline`; then `\put`'s
        // `\ignorespaces`. The boxes' groups hold no assignment. An `\everyhbox` would be
        // inserted at each `\hbox`'s `{`: then the tokens go to TeX as written.
        if self.e.eqtb.toks(Reg { region: Region::Toks, slot: EVERY_HBOX }).is_empty() {
            let hss = fil_glue(Prim::Hss);
            self.kill_glue();
            self.picture_box(Some(0), -y1, |mc| {
                mc.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, x1) });
                for &(xd, yd) in &dots {
                    mc.picture_box(Some(0), -yd, |mc| {
                        mc.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, xd) });
                        mc.build_glue(Glue { width: -half, ..Glue::default() }, true);
                        mc.build_rule([whole, half, half]);
                        mc.build_glue(hss, false);
                    });
                }
                mc.build_glue(hss, false);
            });
            let t = self.named_cs(named!("ignorespaces"));
            self.input.back(t);
            return;
        }
        let mut s = format!("\\put({x1}sp,{y1}sp){{");
        for (xd, yd) in dots {
            s += &format!("\\raise{yd}sp\\hbox to0sp{{\\kern{xd}sp\\hskip{}sp\\vrule height{half}sp depth{half}sp width{whole}sp\\hss}}", -half);
        }
        s.push('}');
        let t = self.tex_tokens(&s);
        self.back_list(&t);
    }

    fn picture_glue(&mut self, w: i32) {
        self.build_glue(Glue { width: w, ..Glue::default() }, false);
    }

    /// A box of the material `fill` builds (natural width, or `to` `w`), shifted down by
    /// `shift` (`\raise` is a negative one).
    fn picture_box(&mut self, w: Option<i32>, shift: i32, fill: impl FnOnce(&mut Self)) {
        self.push_nest(Mode::Horizontal, true, 1000);
        fill(self);
        let start = self.start as usize;
        let b = self.hpack(start, w.unwrap_or(0), w.is_some());
        self.pop_nest();
        let idx = self.e.nodes.add_box(b);
        self.box_end(Ctx::Shift(shift), Some(idx));
    }

    fn picture_char(&mut self, f: u16, c: u8) {
        self.cur.push(Node { kind: Kind::Char, sub: 0, font: f, a: c as i32, b: 0 });
    }

    /// `\line(x,y){len}` and `\vector(x,y){len}`.
    fn picture_line(&mut self, vector: bool) {
        let name = if vector { "\\vector" } else { "\\line" };
        let Some((x, y)) = self.picture_pair(false) else { return self.not_native(&format!("{name} without its slope")) };
        let mut len = Vec::new();
        self.read_arg(&mut len);
        if self.mode != Mode::Horizontal {
            return self.not_native(&format!("{name} outside a picture"));
        }
        let Some(x) = self.picture_int(&x) else { return };
        let Some(y) = self.picture_int(&y) else { return };
        if vector && x.unsigned_abs() >= 5 {
            return self.not_native("a \\vector slope outside the line font");
        }
        let Some(len) = self.picture_length(&len) else { return };
        if len < 0 {
            return self.not_native("a picture line of negative length");
        }
        let (whole, half) = (self.e.eqtb.word(WHOLEWIDTH), self.e.eqtb.word(HALFWIDTH));
        let f = self.line_font();
        let hss = fil_glue(Prim::Hss);
        if x == 0 {
            // `\@vline`: `\hb@xt@\z@{\hskip -\@halfwidth \vrule \@width \@wholewidth \@height
            // \@linelen \@depth \z@\hss}` (`\@upline`), or height zero and depth `\@linelen`.
            let up = y >= 0;
            self.picture_box(Some(0), 0, |m| {
                m.picture_glue(-half);
                m.build_rule(if up { [whole, len, 0] } else { [whole, 0, len] });
                m.build_glue(hss, false);
            });
            if vector {
                if up {
                    // `\@upvector`: `\raise\@linelen\hb@xt@\z@{\lower\ht\@tempboxa\box\@tempboxa
                    // \hss}`, `\@tempboxa` `\hbox{\@linefnt\char 54}`.
                    let Some([_, ht, _, _]) = self.e.fonts.dims(f, 54) else { return self.not_native("a line font without its arrow head") };
                    self.picture_box(Some(0), -len, |m| {
                        m.picture_box(None, ht, |m| m.picture_char(f, 54));
                        m.build_glue(hss, false);
                    });
                } else {
                    // `\@downvector`: `\lower\@linelen\hb@xt@\z@{\@linefnt\char 63\hss}`.
                    self.picture_box(Some(0), len, |m| {
                        m.picture_char(f, 63);
                        m.build_glue(hss, false);
                    });
                }
            }
            return;
        }
        if y == 0 {
            // `\@hline`: `\vrule \@height \@halfwidth \@depth \@halfwidth \@width \@linelen`,
            // drawn leftwards for a negative `x`.
            if x < 0 {
                self.picture_glue(-len);
            }
            self.build_rule([len, half, half]);
            if x < 0 {
                self.picture_glue(-len);
            }
            if vector {
                // `\@hvector`: `\hb@xt@\z@{\@linefnt \@getlarrow(1,0)\hss}` or
                // `\hss\@getrarrow(1,0)`.
                self.picture_box(Some(0), 0, |m| {
                    if x < 0 {
                        m.picture_char(f, 27);
                        m.build_glue(hss, false);
                    } else {
                        m.build_glue(hss, false);
                        m.picture_char(f, 45);
                    }
                });
            }
            return;
        }
        // `\@sline`.
        let neg = x < 0;
        let (xa, yy) = if neg { (-x, -y) } else { (x, y) };
        if yy.abs() > 6 || xa > 6 {
            return self.not_native("a picture line slope outside the line font");
        }
        // `\@getlinechar`: `8x - 9 + y`, or `+ 64 - y` for a falling line.
        let code = xa * 8 - 9 + if yy > 0 { yy } else { -yy + 64 };
        let Some([wd, ht, _, _]) = u8::try_from(code).ok().and_then(|c| self.e.fonts.dims(f, c)).filter(|d| d[0] != 0) else {
            return self.not_native("a picture line slope outside the line font");
        };
        let code = code as u8;
        if len < wd && len != 0 {
            return self.not_native("a picture line shorter than its slope's character");
        }
        // `\@upordown` `\raise` from zero, or `\lower` from the character's height.
        let up = y > 0;
        let place = |h: i32| if up { -h } else { h };
        let mut clnht = if up { 0 } else { ht };
        let mut clnwd = wd;
        if neg {
            self.picture_glue(-wd);
        }
        while clnwd < len {
            self.picture_box(None, place(clnht), |m| m.picture_char(f, code));
            if neg {
                self.picture_glue(-2 * wd);
            }
            clnht += ht;
            clnwd += wd;
        }
        clnht -= ht;
        clnwd -= wd;
        let mut dima = len - clnwd;
        let dimb = dima - wd;
        self.picture_glue(if neg { -dimb } else { dimb });
        dima = dima.wrapping_mul(1000);
        let cnta = dima / wd;
        let dima = ht.wrapping_mul(cnta) / 1000;
        clnht += dima;
        if len < wd {
            self.picture_glue(wd);
        } else {
            self.picture_box(None, place(clnht), |m| m.picture_char(f, code));
        }
        if vector {
            // `\@svector`: `\hskip -\wd\@linechar \@upordown\@clnht \hbox{\@linefnt
            // \@getlarrow(\@xarg,\@yyarg)}` (or `\@getrarrow`), for `|y| < 5`.
            if y.abs() >= 5 {
                return self.not_native("a \\vector slope outside the line font");
            }
            let head = if neg { left_arrow(xa, yy) } else { right_arrow(xa, yy) };
            self.picture_glue(-wd);
            self.picture_box(None, place(clnht), |m| m.picture_char(f, head));
        }
    }
}

/// `\@getlarrow(#1,#2)`: 27 for a level arrow, else `16x - 9 + 2y`, or `+ 64 - 2y` for
/// a falling one.
fn left_arrow(x: i32, y: i32) -> u8 {
    if y == 0 {
        return 27;
    }
    let b = 2 * y;
    (x * 16 - 9 + if b > 0 { b } else { -b + 64 }) as u8
}

/// `\@getrarrow(#1,#2)`, by `|y|`: 45; `24x - 6` (x < 3), 49 (x = 3) or 58; `24x - 3`
/// (x < 3) or 51; `16x - 2`; `16x + 7`; plus 64 for a falling arrow.
fn right_arrow(x: i32, y: i32) -> u8 {
    let c = match y.abs() {
        0 => 45,
        1 if x < 3 => x * 24 - 6,
        1 if x == 3 => 49,
        1 => 58,
        2 if x < 3 => x * 24 - 3,
        2 => 51,
        3 => x * 16 - 2,
        _ => x * 16 + 7,
    };
    (c + if y < 0 { 64 } else { 0 }) as u8
}
