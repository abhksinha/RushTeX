//! Small amsmath and mathtools commands natively, as their packages define them: amsmath's
//! `\boxed`, `\cfrac`, `\iint`, `\iiint`, `\iiiint`; mathtools' `\mathllap`, `\mathrlap`, `\mathclap`,
//! `\DeclarePairedDelimiter`.

use super::super::engine_catcode::Cat;
use super::super::engine_prim::{Meaning, Prim};
use super::super::engine_run::{Machine, Mode};
use super::super::engine_token::Token;

/// The natives in `DOC` (`DOC_PARTS`), reached through `PACKAGE_NATIVES`.
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@boxed", |m| m.boxed()),
    ("rt@mathllap", |m| m.math_lap(0)),
    ("rt@mathrlap", |m| m.math_lap(1)),
    ("rt@mathclap", |m| m.math_lap(2)),
    ("rt@cfrac", |m| m.cfrac()),
    ("rt@declpaired", |m| m.declare_paired_delimiter()),
    ("rt@paired", |m| m.paired_delimiter()),
    ("rt@declpairedx", |m| m.declare_paired_delimiter_x()),
    ("rt@sideset", |m| m.sideset()),
    ("rt@pairedx", |m| m.paired_delimiter_x()),
    // `\intertext` in `align` and `gather` is read with their body (`ams_align_begin`).
    ("rt@intertext", |m| m.not_native("\\intertext outside align or gather")),
    ("rt@iint", |m| m.multi_integral(2)),
    ("rt@iiint", |m| m.multi_integral(3)),
    ("rt@iiiint", |m| m.multi_integral(4)),
    // A `\tag` of tokens (`df_tag`): `\hbox{\m@th\normalfont(\ignorespaces #1\unskip\/)}`.
    ("rt@tagfont", |m| {
        m.math_surround_zero();
        m.normal_font();
    }),
    // microtype's `\tagform@` patch: `\leftprotrusion{(}`, `\rightprotrusion{)}`.
    ("rt@tagopen", |m| {
        m.protrusion_kern(b'(', false);
        m.build_text(b"(");
        m.ignore_spaces();
    }),
    ("rt@tagclose", |m| {
        m.unskip();
        m.append_italic_correction();
        m.build_text(b")");
        m.protrusion_kern(b')', true);
    }),
    ("rt@atagend", |m| m.align_tag_end()),
    ("rt@mtcases", |m| m.mt_cases_init()),
    ("rt@strutbox", |m| m.copy_strut()),
    // amsthm's `\qed@tag` in an `align*` row (`qed_row_end`).
    ("rt@qedw", |m| m.qed_width()),
    ("rt@qedkeep", |m| m.qed_keep()),
];

impl Machine<'_, '_> {
    /// amsmath's `\boxed{#1}`: `\fbox{\m@th$\displaystyle#1$}`.
    fn boxed(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let mut t = self.tex_tokens("\\fbox{\\mathsurround0pt$\\displaystyle ");
        t.extend(arg);
        t.extend(self.tex_tokens("$}"));
        self.back_list(&t);
    }

    /// mathtools' `\mathllap[#1]{#2}` (and `\mathrlap`, `\mathclap`): with no style,
    /// `\mathpalette\MT_mathllap:Nn{#2}`, that is `\mathchoice` of the four styles; each
    /// `{}\llap{$\m@th#1{#2}$}` (`\rlap`, `\clap`: `\hb@xt@\z@{\hss#1\hss}`).
    fn math_lap(&mut self, kind: u8) {
        if self.mode != Mode::Math {
            return self.not_native("\\mathclap outside math");
        }
        let style = if self.next_is('[') {
            let s = self.bracket_arg();
            let name: Option<&str> = match s[..] {
                [t] => t.cs_id().map(|id| self.e.cs.name(id)),
                _ => None,
            };
            match name {
                Some(n @ ("displaystyle" | "textstyle" | "scriptstyle" | "scriptscriptstyle")) => Some(n.to_string()),
                _ => return self.not_native("\\mathclap's style not a style"),
            }
        } else {
            None
        };
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let (open, close) = match kind {
            0 => ("{}\\hbox to0pt{\\hss$\\mathsurround0pt\\", "}$}"),
            1 => ("{}\\hbox to0pt{$\\mathsurround0pt\\", "}$\\hss}"),
            _ => ("{}\\hbox to0pt{\\hss$\\mathsurround0pt\\", "}$\\hss}"),
        };
        let one = |m: &mut Self, s: &str| {
            let mut t = m.tex_tokens(&format!("{open}{s}{{"));
            t.extend(arg.iter().copied());
            t.extend(m.tex_tokens(close));
            t
        };
        let t = match style {
            Some(s) => one(self, &s),
            None => {
                let mut t = vec![self.named_cs(named!("mathchoice"))];
                for s in ["displaystyle", "textstyle", "scriptstyle", "scriptscriptstyle"] {
                    t.push(Token::char(Cat::BeginGroup, '{'));
                    t.extend(one(self, s));
                    t.push(Token::char(Cat::EndGroup, '}'));
                }
                t
            }
        };
        self.back_list(&t);
    }

    /// amsmath's `\cfrac[#1]{#2}{#3}`: `{\displaystyle\frac{\strut\ifx r#1\hfill\fi#2\ifx
    /// l#1\hfill\fi}{#3}}\kern-\nulldelimiterspace`; `#1` is `c` unless given, and only `l`,
    /// `c` or `r` are native.
    fn cfrac(&mut self) {
        let side = if self.next_is('[') {
            let s = self.bracket_arg();
            match s[..] {
                [t] if t.cs_id().is_none() && matches!(t.chr(), 'l' | 'c' | 'r') => t.chr(),
                _ => return self.not_native("\\cfrac's position not l, c or r"),
            }
        } else {
            'c'
        };
        let (mut num, mut den) = (Vec::new(), Vec::new());
        self.read_arg(&mut num);
        self.read_arg(&mut den);
        let mut t = self.tex_tokens(if side == 'r' { "{\\displaystyle\\frac{\\strut\\hfill " } else { "{\\displaystyle\\frac{\\strut " });
        t.extend(num);
        t.extend(self.tex_tokens(if side == 'l' { "\\hfill}{" } else { "}{" }));
        t.extend(den);
        t.extend(self.tex_tokens("}}\\kern-\\nulldelimiterspace "));
        self.back_list(&t);
    }

    /// mathtools' `\DeclarePairedDelimiter{\name}{left}{right}`: `\@ifdefinable`, then
    /// `\DeclareRobustCommand{\name}` of `\@ifstar` and the default wrappers; here a
    /// protected macro of `\rt@paired{left}{right}` ([`Self::paired_delimiter`]).
    fn declare_paired_delimiter(&mut self) {
        let (mut name, mut left, mut right) = (Vec::new(), Vec::new(), Vec::new());
        self.read_arg(&mut name);
        self.read_arg(&mut left);
        self.read_arg(&mut right);
        let Some(id) = name.first().filter(|_| name.len() == 1).and_then(|t| t.cs_id()) else { return self.not_native("\\DeclarePairedDelimiter's name") };
        if self.meaning(Token::cs(id)) != Meaning::Undefined {
            return self.not_native("\\DeclarePairedDelimiter of a defined name");
        }
        let mut t = vec![Token::END_MATCH, self.doc_id(doc!("rt@paired")), Token::char(Cat::BeginGroup, '{')];
        t.extend(left);
        t.extend([Token::char(Cat::EndGroup, '}'), Token::char(Cat::BeginGroup, '{')]);
        t.extend(right);
        t.push(Token::char(Cat::EndGroup, '}'));
        let text = self.e.toks.store(&t);
        self.e.eqtb.set_meaning(id, Meaning::Macro(super::super::engine_prim::Macro { text, flags: super::super::engine_prim::PROTECTED }), false);
    }

    /// A paired delimiter `\name` declared by `\DeclarePairedDelimiter`, its `{left}{right}`
    /// read: `\name*{x}` is the star wrapper, `\mathopen{}\mathclose\bgroup\left left x
    /// \aftergroup\egroup\right right` (the `\egroup` closes the `\mathclose` after the
    /// `\right`'s inner noad); `\name[\big]{x}` the scaled one, `\mathopen{\bigl left}x
    /// \mathclose{\bigr right}`; `\name{x}` (or a blank size) the plain one, `\ifx.left\else
    /// \mathopen left\fi x\ifx.right\else\mathclose right\fi`.
    fn paired_delimiter(&mut self) {
        let (mut left, mut right) = (Vec::new(), Vec::new());
        self.read_arg(&mut left);
        self.read_arg(&mut right);
        let star = self.next_is('*');
        let size = if !star && self.next_is('[') {
            let s = self.bracket_arg();
            match s.iter().filter(|t| t.cat() != Some(Cat::Space)).collect::<Vec<_>>()[..] {
                [] => None,
                [&t] if t.cs_id().is_some() => t.cs_id(),
                _ => return self.not_native("a paired delimiter's size not one command"),
            }
        } else {
            None
        };
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let t = self.paired_tokens(left, right, star, size, arg);
        self.back_list(&t);
    }

    /// mathtools' `\DeclarePairedDelimiterX{\name}[n]{left}{right}{body}`
    /// (`\DeclarePairedDelimiterXPP` with empty pre and post code): a protected macro of
    /// `\rt@pairedx{n}{left}{right}{body}` ([`Self::paired_delimiter_x`]), the body's `#k`
    /// kept as parameter characters.
    fn declare_paired_delimiter_x(&mut self) {
        let (mut name, mut left, mut right, mut body) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        self.read_arg(&mut name);
        let n = if self.next_is('[') { self.bracket_arg() } else { Vec::new() };
        let n = match n.iter().filter(|t| t.cat() != Some(Cat::Space)).collect::<Vec<_>>()[..] {
            [t] if t.cs_id().is_none() && ('1'..='9').contains(&t.chr()) => t.chr(),
            _ => return self.not_native("\\DeclarePairedDelimiterX's count not 1 to 9"),
        };
        self.read_arg(&mut left);
        self.read_arg(&mut right);
        self.read_arg(&mut body);
        let Some(id) = name.first().filter(|_| name.len() == 1).and_then(|t| t.cs_id()) else { return self.not_native("\\DeclarePairedDelimiterX's name") };
        if self.meaning(Token::cs(id)) != Meaning::Undefined {
            return self.not_native("\\DeclarePairedDelimiterX of a defined name");
        }
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![Token::END_MATCH, self.doc_id(doc!("rt@pairedx")), open, Token::char(Cat::Other, n), close, open];
        t.extend(left);
        t.extend([close, open]);
        t.extend(right);
        t.extend([close, open]);
        t.extend(body);
        t.push(close);
        let text = self.e.toks.store(&t);
        self.e.eqtb.set_meaning(id, Meaning::Macro(super::super::engine_prim::Macro { text, flags: super::super::engine_prim::PROTECTED }), false);
    }

    /// A delimiter `\name` of `\DeclarePairedDelimiterX`, its `{n}{left}{right}{body}` read:
    /// `\name*` is `\begingroup\def\delimsize{\middle}` and the star wrapper, `\name[\big]`
    /// `\def\delimsize{\big}` and the scaled one, `\name` `\delimsize` empty and the plain
    /// one, each of the body with its `#1`..`#n` the n arguments read after, then
    /// `\endgroup`.
    fn paired_delimiter_x(&mut self) {
        let (mut n, mut left, mut right, mut body) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        self.read_arg(&mut n);
        self.read_arg(&mut left);
        self.read_arg(&mut right);
        self.read_arg(&mut body);
        let n = n.first().map_or(1, |t| t.chr() as usize - '0' as usize);
        let star = self.next_is('*');
        let size = if !star && self.next_is('[') {
            let s = self.bracket_arg();
            match s.iter().filter(|t| t.cat() != Some(Cat::Space)).collect::<Vec<_>>()[..] {
                [] => None,
                [&t] if t.cs_id().is_some() => t.cs_id(),
                _ => return self.not_native("a paired delimiter's size not one command"),
            }
        } else {
            None
        };
        let mut args = Vec::with_capacity(n);
        for _ in 0..n {
            let mut a = Vec::new();
            self.read_arg(&mut a);
            args.push(a);
        }
        // The body as `\newcommand` stored it: `#k` the k-th argument, `##` one `#`.
        let mut arg = Vec::with_capacity(body.len());
        let mut i = 0;
        while i < body.len() {
            let t = body[i];
            if t.cat() == Some(Cat::Param) && i + 1 < body.len() {
                let next = body[i + 1];
                if next.cat() == Some(Cat::Param) {
                    arg.push(next);
                    i += 2;
                    continue;
                }
                match (next.cs_id(), next.chr().to_digit(10)) {
                    (None, Some(k)) if k >= 1 && k as usize <= n => {
                        arg.extend(args[k as usize - 1].iter().copied());
                        i += 2;
                        continue;
                    }
                    _ => return self.not_native("a \\DeclarePairedDelimiterX body's parameter"),
                }
            }
            arg.push(t);
            i += 1;
        }
        // `\begingroup\def\delimsize{...}`.
        self.begin_group(super::super::engine_run::SEMI_SIMPLE_GROUP);
        let ds = self.e.cs.intern("delimsize");
        let mut text = vec![Token::END_MATCH];
        if star {
            text.push(self.e.cs.get("middle").map_or(Token::char(Cat::Other, ' '), Token::cs));
        } else if let Some(c) = size {
            text.push(Token::cs(c));
        }
        let text = self.e.toks.store(&text);
        self.e.eqtb.set_meaning(ds, Meaning::Macro(super::super::engine_prim::Macro { text, flags: 0 }), false);
        let mut t = self.paired_tokens(left, right, star, size, arg);
        t.push(self.named_cs(named!("endgroup")));
        self.back_list(&t);
    }

    /// The wrappers of a paired delimiter (`\MT_delim_default_inner_wrappers:n`), `left`
    /// and `right` with `\{` and `\}` read as `\lbrace` and `\rbrace`.
    fn paired_tokens(&mut self, mut left: Vec<Token>, mut right: Vec<Token>, star: bool, size: Option<u32>, arg: Vec<Token>) -> Vec<Token> {
        // `\{` and `\}` (`\ifmmode\lbrace\else\textbraceleft\fi`) in a formula.
        let (lb, rb) = (self.named_cs(named!("{")), self.named_cs(named!("}")));
        let (lbrace, rbrace) = (self.named_cs(named!("lbrace")), self.named_cs(named!("rbrace")));
        for t in left.iter_mut().chain(right.iter_mut()) {
            if *t == lb {
                *t = lbrace;
            } else if *t == rb {
                *t = rbrace;
            }
        }
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = Vec::new();
        if star {
            t.extend(self.tex_tokens("\\mathopen{}\\mathclose{\\left"));
            t.extend(left);
            t.extend(arg);
            t.extend(self.tex_tokens("\\right"));
            t.extend(right);
            t.push(close);
        } else if let Some(c) = size {
            let name = self.e.cs.name(c).to_string();
            let (l, r) = (self.e.cs.intern(&format!("{name}l")), self.e.cs.intern(&format!("{name}r")));
            t.extend(self.tex_tokens("\\mathopen"));
            t.extend([open, Token::cs(l)]);
            t.extend(left);
            t.push(close);
            t.extend(arg);
            t.extend(self.tex_tokens("\\mathclose"));
            t.extend([open, Token::cs(r)]);
            t.extend(right);
            t.push(close);
        } else {
            let dot = |d: &[Token]| d.first() == Some(&Token::char(Cat::Other, '.'));
            if !dot(&left) {
                t.extend(self.tex_tokens("\\mathopen"));
                t.extend(left);
            }
            t.extend(arg);
            if !dot(&right) {
                t.extend(self.tex_tokens("\\mathclose"));
                t.extend(right);
            }
        }
        t
    }

    /// amsmath's `\sideset{#1}{#2}{#3}` (amsmath.sty 921), `\@mathmeasure` of the current
    /// kernel being `\setbox n\hbox{\m@th$style text$}`: `\@mathmeasure\z@\displaystyle{#3}
    /// \global\setbox\@ne\vbox to\ht\z@{}\dp\@ne\dp\z@ \setbox\tw@\box\@ne
    /// \@mathmeasure4\displaystyle{\copy\tw@#1}\@mathmeasure6\displaystyle{#3\nolimits#2}
    /// \dimen@-\wd6 \advance\dimen@\wd4 \advance\dimen@\wd\z@ \hbox to\dimen@{}\mathop{
    /// \kern-\dimen@\box4\box6}`; the registers it sets (`\dimen@` among them) are the
    /// document's to read, so the same assignments are made.
    fn sideset(&mut self) {
        if self.mode != Mode::Math {
            return self.not_native("\\sideset outside math");
        }
        let (mut pre, mut post, mut op) = (Vec::new(), Vec::new(), Vec::new());
        self.read_arg(&mut pre);
        self.read_arg(&mut post);
        self.read_arg(&mut op);
        // `\@mathmeasure`'s third argument loses its braces (tex.web §400).
        let mut t = self.tex_tokens("\\setbox0\\hbox{\\mathsurround0pt$\\displaystyle ");
        t.extend(op.iter().copied());
        t.extend(self.tex_tokens("$}\\global\\setbox1\\vbox to\\ht0{}\\dp1\\dp0 \\setbox2\\box1 \\setbox4\\hbox{\\mathsurround0pt$\\displaystyle\\copy2 "));
        t.extend(pre);
        t.extend(self.tex_tokens("$}\\setbox6\\hbox{\\mathsurround0pt$\\displaystyle "));
        t.extend(op);
        t.extend(self.tex_tokens("\\nolimits "));
        t.extend(post);
        t.extend(self.tex_tokens("$}\\dimen0-\\wd6 \\advance\\dimen0\\wd4 \\advance\\dimen0\\wd0 \\hbox to\\dimen0{}\\mathop{\\kern-\\dimen0\\box4\\box6}"));
        self.back_list(&t);
    }

    /// amsmath's `\MultiIntegral{n}` (`\iint` is `\DOTSI\MultiIntegral{2}`): `\ints@c` is
    /// `\intop\intkern@` n-1 times, then `\intop\ilimits@`, with `\intkern@`
    /// `\mkern-6mu\mathchoice{\mkern-3mu}{}{}{}`. `\futurelet` sees the next token: before
    /// `\limits` or `\displaylimits`, or with `\ilimits@` `\displaylimits` (the `intlimits`
    /// option), `\ints@b` first opens a `\mathop` (`\mkern-7mu\mathchoice{\mkern-2mu}{}{}{}
    /// \mathop\bgroup\mkern7mu\mathchoice{\mkern2mu}{}{}{}`) that `\ilimits@`, let to
    /// `\egroup`, closes.
    fn multi_integral(&mut self, n: u8) {
        let ilimits = self.e.cs.intern("ilimits@");
        let im = self.e.eqtb.meaning(ilimits);
        let next = self.get_token();
        if let Some(t) = next {
            self.input.back(t);
        }
        let nm = next.and_then(|t| t.cs_id()).map(|c| self.e.eqtb.meaning(c));
        let limits = matches!(nm, Some(Meaning::Prim(Prim::Limits | Prim::DisplayLimits))) || im == Meaning::Prim(Prim::DisplayLimits);
        let mut text = String::new();
        if limits {
            text.push_str("\\mkern-7mu\\mathchoice{\\mkern-2mu}{}{}{}\\mathop{\\mkern7mu\\mathchoice{\\mkern2mu}{}{}{}");
        }
        for _ in 1..n {
            text.push_str("\\intop\\mkern-6mu\\mathchoice{\\mkern-3mu}{}{}{}");
        }
        text.push_str("\\intop");
        let mut t = self.tex_tokens(&text);
        t.push(if limits { Token::char(Cat::EndGroup, '}') } else { Token::cs(ilimits) });
        self.back_list(&t);
    }
}
