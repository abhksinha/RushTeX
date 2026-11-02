//! cancel.sty (2013/04/12 v2.2) natively with its options (`samesize`, `smaller`,
//! `Smaller`, `makeroom`, `overlap`, `thicklines`): `\cancel`, `\bcancel`, `\xcancel` and
//! `\cancelto`.
//!
//! `\cancel{x}` is `\mathpalette{\@cancel{\@can@slash{}}}{x}` in a formula and
//! `\@cancel{\@can@slash{}}\hbox{x}` in text. `\@cancel` sets the material in `\@tempboxa`
//! (`\@begin@tempboxa\hbox{\m@th$#2{#3}$}`), centres it on the axis (`\hbox{$\m@th\vcenter
//! {\box\@tempboxa}$}`, the difference in height kept), and puts it `\ooalign`ed under the
//! slash: `{\/\raise\dimen@\hbox{\ooalign{#1\hfil\box\@tempboxa\hfil \cr}}}`. The slash
//! (`\@can@slash`) is a picture `\line` whose slope and length come from the box's width
//! and total height by the package's integer cases, in `\vcenter{\hbox{..}}` between two
//! `\hidewidth`s; `\bcancel` draws it falling, `\xcancel` both. `\cancelto{v}{x}` draws a
//! `\vector` the same way (`\@cancelto`, `\canto@vector`) with `v` as its superscript, in the
//! next smaller style.
//!
//! Here the material is set by the engine's own `\setbox` (`rt@cancelx`), measured
//! (`rt@cancely`), centred and measured again (`rt@cancelz`), and the rest becomes the
//! tokens the macros leave, with the package's arithmetic done in Rust: the slope and the
//! length (printed by `\strip@pt` and read back, which gives the same value), and
//! `\canto@vector`'s kerns and rule. The picture `\line` and `\vector` are the native ones
//! (`engine_picture`). The options, which `\ProcessOptions` runs in the order they are
//! declared, are kept in one hidden register ([`cancel_options`]).

use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::{Reg, Region};
use super::super::engine_run::{Machine, Mode};
use super::super::engine_token::Token;
use super::TEMP_BOX;
use crate::backend::backend_scaled::xn_over_d;

const PT: i32 = 65536;

/// The options: bits 0-1 `\cancelto`'s sizes (0 `smaller`, 1 `Smaller`, 2 `samesize`), bit 2
/// `makeroom` (`\canto@fil` `\hfil`, not `\hidewidth`), bit 3 `thicklines`.
const OPTIONS: Reg = Reg::register(Region::Int, 32719);

/// cancel's options as `\ProcessOptions` runs them (declaration order: `samesize`,
/// `smaller`, `Smaller`, `makeroom`, `overlap`, `thicklines`), as [`OPTIONS`] keeps them;
/// `None` for an option the package does not declare.
pub(super) fn cancel_options(opts: &str) -> Option<i32> {
    let given: Vec<&str> = opts.split(',').map(str::trim).filter(|o| !o.is_empty()).collect();
    if given.iter().any(|o| !["samesize", "smaller", "Smaller", "makeroom", "overlap", "thicklines"].contains(o)) {
        return None;
    }
    let has = |o: &str| given.contains(&o);
    let mut v = 0;
    for (o, size) in [("samesize", 2), ("smaller", 0), ("Smaller", 1)] {
        if has(o) {
            v = size;
        }
    }
    if has("makeroom") && !has("overlap") {
        v |= 4;
    }
    if has("thicklines") {
        v |= 8;
    }
    Some(v)
}

/// The natives in `DOC` (`DOC_PARTS`), reached through `PACKAGE_NATIVES`.
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@cancel", |m| m.cancel_command(0)),
    ("rt@bcancel", |m| m.cancel_command(1)),
    ("rt@xcancel", |m| m.cancel_command(2)),
    ("rt@cancelto", |m| m.cancel_command(3)),
    ("rt@cancelx", |m| m.cancel_set()),
    ("rt@cancely", |m| m.cancel_measured()),
    ("rt@cancelz", |m| m.cancel_centred()),
];

/// `\@min@pt#1#2`: `#1` at least `#2`pt.
fn at_least(v: i32, pts: i32) -> i32 {
    v.max(pts * PT)
}

/// `\@can@slash`'s line for a box `w` wide and `t` tall (total): its slope `(x, y)` with
/// `y` positive, and its length.
fn slash(w: i32, t: i32) -> (i32, i32, i32) {
    let mut d = at_least(w, 2);
    let mut t = at_least(t, 6);
    if t < d {
        // Wide.
        d = at_least(d, 8);
        let c = t.wrapping_mul(5) / d;
        d += 2 * PT;
        let (x, y) = match c {
            0 => (6, 1),
            1 => (4, 1),
            2 => (2, 1),
            3 => (4, 3),
            _ => (1, 1),
        };
        (x, y, d)
    } else {
        // Tall: `\dimen@ .16\totalheight` and kin.
        t = at_least(t, 8);
        t += 2 * PT;
        let c = d.wrapping_mul(5) / t;
        let (f, x, y) = match c {
            0 => (10486, 1, 6),
            1 => (16384, 1, 4),
            2 => (32768, 1, 2),
            3 => (49152, 3, 4),
            _ => (65536, 1, 1),
        };
        let d = if f == 65536 { t } else { xn_over_d(t, f, 65536) };
        (x, y, d)
    }
}

/// `\@cancelto`'s vector for a box `w` wide and `t` tall: `\canto@vector`'s four digits, its
/// length and `\@tempdimb`.
fn arrow(w: i32, t: i32) -> ([i32; 4], i32, i32) {
    let mut d = at_least(w, 2);
    let mut t = at_least(t, 4);
    if t < d {
        let c = t.wrapping_mul(5) / d;
        d += match c {
            0 | 1 => 5,
            2 => 4,
            3 => 3,
            _ => 2,
        } * PT;
        d = at_least(d, 9);
        d += PT;
        let digits = match c {
            0 | 1 => [5, 4, 4, 1],
            2 => [5, 4, 2, 1],
            3 => [4, 4, 4, 3],
            _ => [3, 6, 1, 1],
        };
        (digits, d, 3 * PT)
    } else {
        t += 3 * PT;
        let c = d.wrapping_mul(5) / t;
        t += 3 * PT;
        // `.25`, `.5`, `.75`, `1` times `\totalheight`; `.8`, `1.2`, `1.5`, `2` times `\p@`.
        let (f, b, digits) = match c {
            0 | 1 => (16384, 52429, [0, 8, 1, 4]),
            2 => (32768, 78643, [1, 8, 1, 2]),
            3 => (49152, 98304, [2, 7, 3, 4]),
            _ => (65536, 2 * PT, [3, 6, 1, 1]),
        };
        let d = if f == 65536 { t } else { xn_over_d(t, f, 65536) };
        (digits, d, b)
    }
}

impl Machine<'_, '_> {
    /// The package's options at `\usepackage` ([`cancel_options`]).
    pub(super) fn cancel_set_options(&mut self, v: i32) {
        self.e.eqtb.set_word(OPTIONS, v, true);
    }

    /// `\cancel` (0), `\bcancel` (1), `\xcancel` (2), `\cancelto` (3): in a formula the four
    /// styles of `\mathpalette` (`\cancelto`'s `\mathchoice` with the value in the styles
    /// its option gives), in text `\hbox{x}`.
    fn cancel_command(&mut self, kind: u8) {
        let mut to = Vec::new();
        if kind == 3 {
            self.read_arg(&mut to);
        }
        let mut body = Vec::new();
        self.read_arg(&mut body);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let k = Token::char(Cat::Other, char::from(b'0' + kind));
        let x = self.doc_id(doc!("rt@cancelx"));
        let relax = self.named_cs(named!("relax"));
        let mut t = Vec::with_capacity(4 * (body.len() + to.len() + 12));
        let one = |t: &mut Vec<Token>, s1: Token, s2: Token| {
            t.extend([x, k, s1, open]);
            t.extend_from_slice(&to);
            t.extend([close, s2, open]);
            t.extend_from_slice(&body);
            t.push(close);
        };
        match self.mode {
            Mode::Math => {
                let [d, tx, s, ss] = [named!("displaystyle"), named!("textstyle"), named!("scriptstyle"), named!("scriptscriptstyle")].map(|i| self.named_cs(i));
                let pairs = match (kind, self.e.eqtb.word(OPTIONS) & 3) {
                    (3, 1) => [(s, d), (s, tx), (ss, s), (ss, ss)],
                    (3, 2) => [(tx, d), (tx, tx), (s, s), (ss, ss)],
                    (3, _) => [(tx, d), (s, tx), (ss, s), (ss, ss)],
                    _ => [(relax, d), (relax, tx), (relax, s), (relax, ss)],
                };
                t.push(self.named_cs(named!("mathchoice")));
                for (s1, s2) in pairs {
                    t.push(open);
                    one(&mut t, s1, s2);
                    t.push(close);
                }
            }
            Mode::Horizontal if kind < 3 => one(&mut t, relax, self.named_cs(named!("hbox"))),
            _ => return self.not_native("cancel's commands outside a formula or a paragraph"),
        }
        self.back_list(&t);
    }

    /// `rt@cancelx k s1{v}s2{x}`: `\@begin@tempboxa\hbox{\m@th$s2{x}$}`, then `rt@cancely`
    /// with `k s1{v}`.
    fn cancel_set(&mut self) {
        let (Some(k), Some(s1)) = (self.get_token(), self.get_token()) else { return };
        let mut to = Vec::new();
        self.read_arg(&mut to);
        let Some(s2) = self.get_token() else { return };
        let mut body = Vec::new();
        self.read_arg(&mut body);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let colored = self.colored();
        let mut t = self.tex_tokens(&format!("\\begingroup\\setbox{TEMP_BOX}\\hbox{{"));
        if colored {
            t.push(self.named_cs(named!("begingroup")));
        }
        t.extend(self.tex_tokens("\\mathsurround0pt$"));
        t.extend([s2, open]);
        t.extend(body);
        t.extend([close, Token::char(Cat::MathShift, '$')]);
        if colored {
            t.push(self.named_cs(named!("endgroup")));
        }
        t.extend([close, self.doc_id(doc!("rt@cancely")), k, s1, open]);
        t.extend(to);
        t.push(close);
        self.back_list(&t);
    }

    fn cancel_box(&self) -> (i32, i32, i32) {
        self.e.eqtb.box_reg(TEMP_BOX).map_or((0, 0, 0), |b| {
            let b = &self.e.nodes.boxes[b as usize];
            (b.width, b.height, b.depth)
        })
    }

    /// `\dimen@\height`, `\totalheight` kept; `\setbox\@tempboxa\hbox{$\m@th\vcenter{\box
    /// \@tempboxa}$}`, then `rt@cancelz{height}{total}k s1{v}`.
    fn cancel_measured(&mut self) {
        let Some(k) = self.get_token() else { return };
        let (_, h, d) = self.cancel_box();
        let mut t = self.tex_tokens(&format!("\\setbox{TEMP_BOX}\\hbox{{$\\mathsurround0pt\\vcenter{{\\box{TEMP_BOX}}}$}}"));
        t.push(self.doc_id(doc!("rt@cancelz")));
        t.extend(self.tex_tokens(&format!("{{{h}}}{{{}}}", h.wrapping_add(d))));
        t.push(k);
        self.back_list(&t);
    }

    /// The rest of `\@cancel` or `\@cancelto` once the box is centred: `\advance
    /// \dimen@-\height`, `\unitlength\p@ \canc@thinlines`, the slash or the vector over the
    /// box raised by `\dimen@`, `\@end@tempboxa`.
    fn cancel_centred(&mut self) {
        let (mut h0, mut total) = (Vec::new(), Vec::new());
        self.read_arg(&mut h0);
        self.read_arg(&mut total);
        let num = |a: &[Token]| a.iter().map(|t| t.chr()).collect::<String>().parse::<i32>().unwrap_or(0);
        let (h0, total) = (num(&h0), num(&total));
        let Some(k) = self.get_token() else { return };
        let kind = k.chr() as u8 - b'0';
        let Some(s1) = self.get_token() else { return };
        let mut to = Vec::new();
        self.read_arg(&mut to);
        let (w, h1, _) = self.cancel_box();
        let raise = h0.wrapping_sub(h1);
        // `\unitlength\p@ \canc@thinlines`, local to `\@begin@tempboxa`'s group.
        let opt = self.e.eqtb.word(OPTIONS);
        self.e.eqtb.set_word(super::picture::UNITLENGTH, PT, false);
        self.picture_lines(opt & 8 != 0);
        let open = "\\lineskiplimit-16383.99999pt\\vtop{\\baselineskip0pt\\lineskip.25ex\\everycr{}\\tabskip0pt\\halign{#\\crcr";
        // `\canto@fil`: `\hidewidth`, or `\hfil` with `makeroom`.
        let room = opt & 4 != 0;
        let hide = if room { "\\hfil " } else { "\\hskip-1000pt plus1fill" };
        let mut t = self.tex_tokens(&format!("{{\\/\\raise{raise}sp\\hbox{{"));
        if kind < 3 {
            let mut s = String::from(open);
            for sign in [&[1][..], &[-1], &[1, -1]][kind as usize] {
                let (x, y, len) = slash(w, total);
                s.push_str(&format!("{hide}$\\mathsurround0pt\\vcenter{{\\hbox{{\\line({x},{}){{{len}sp}}}}}}${hide}\\cr", sign * y));
            }
            s.push_str(&format!("\\hfil\\box{TEMP_BOX} \\hfil \\cr\\crcr}}}}}}}}\\endgroup"));
            t.extend(self.tex_tokens(&s));
            return self.back_list(&t);
        }
        // `\canto@vector#1#2#3#4#5#6{s1}{v}` with `\dimen@.5\p@`: the value in
        // `\setbox\z@\vbox{\boxmaxdepth.5\p@\hbox{\kern-1.2\p@\kern#1\dimen@$#7{#8}\m@th$}}`,
        // `\wd\z@\z@` (`overlap`; `\kern-#6\unitlength` with `makeroom`), and the vector in `\vcenter{\hbox{\kern#6\unitlength
        // \vrule\@depth(#6*#4/#3)\@width\z@\vector(#3,#4){#5}}}` with the value as its
        // superscript, `\raise#2\dimen@\copy\z@\kern-\scriptspace`.
        let ([d1, d2, d3, d4], len, b) = arrow(w, total);
        let depth = b.wrapping_mul(d4) / d3;
        t.extend(self.tex_tokens(&format!("\\setbox0\\vbox{{\\boxmaxdepth.5pt\\hbox{{\\kern-1.2pt\\kern{}sp$", d1 * 32768)));
        t.extend([s1, Token::char(Cat::BeginGroup, '{')]);
        t.extend(to);
        let (narrow, after) = if room { (format!("\\kern{}sp", b.wrapping_neg()), "\\kern\\wd0 ") } else { ("\\wd0=0pt".to_string(), "\\kern0pt") };
        let rest = self.tex_tokens(&format!("}}\\mathsurround0pt$}}}}{narrow}{open}{hide}$\\mathsurround0pt\\vcenter{{\\hbox{{\\kern{b}sp\\vrule depth{depth}sp width0pt\\vector({d3},{d4}){{{len}sp}}}}}}^{{\\raise{}sp\\copy0\\kern-\\scriptspace}}${hide}\\cr\\hfil\\box{TEMP_BOX} {after}\\hfil \\cr\\crcr}}}}}}}}\\endgroup", d2 * 32768));
        // The `^` is a superscript character.
        let sup = Token::char(Cat::Other, '^');
        t.extend(rest.into_iter().map(|x| if x == sup { Token::char(Cat::Superscript, '^') } else { x }));
        self.back_list(&t);
    }
}
