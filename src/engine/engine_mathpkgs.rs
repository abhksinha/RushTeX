//! Commands of small math packages natively, as their packages define them at the
//! defaults: gensymb's `\degree`, `\celsius`, `\ohm`, `\micro` and `\perthousand`;
//! nicefrac's `\nicefrac`; units' `\unit` and `\unitfrac`; braket's `\set`, `\Set`,
//! `\Braket`.
//!
//! gensymb (`gensymb.sty` 2022/10/17 v1.0.2) defines them in its `\AtBeginDocument`, after
//! `\@ifpackageloaded{textcomp}`:
//! - without textcomp (`\gns@usefakedsymbols`, `\gns@useOmega`): `\degree` is
//!   `\ensuremath{^\circ}`, `\celsius` `\ifmmode^\circ\mathrm{C}\else$^\circ$C\fi`, `\ohm`
//!   `\ifmmode\Omega\else$\Omega$\fi`; `\micro` and `\perthousand` stay undefined
//!   (`\gns@usenomu`, "Not defining \perthousand").
//! - with textcomp (`\gns@setupmathcomp`, `\gns@usetcsymbols`, `\gns@usetextohm`,
//!   `\gns@usetextmu`): in text `\textdegree`, `\textcelsius`, `\textohm`, `\textmu`,
//!   `\textperthousand`; in a formula `\tcdegree` and kin, characters of a symbol font
//!   `gns@font` (TS1 in `\familydefault`) declared at `\begin{document}`, a family the native
//!   job does not hold: not native.
//!
//! The package options `Upomega`, `Omega` and `upmu` decline at `\usepackage`.

use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::{Code, Glue, Reg, Region};
use super::super::engine_pack::BASELINE_SKIP;
use super::super::engine_prim::Meaning;
use super::super::engine_run::{Machine, Mode, SIMPLE_GROUP};
use super::super::engine_token::Token;

/// The natives in `DOC` (`DOC_PARTS`), reached through `PACKAGE_NATIVES`.
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@gnsdegree", |m| m.gensymb(Gns::Degree)),
    ("rt@gnscelsius", |m| m.gensymb(Gns::Celsius)),
    ("rt@gnsohm", |m| m.gensymb(Gns::Ohm)),
    ("rt@gnsmicro", |m| m.gensymb(Gns::Micro)),
    ("rt@gnsperthousand", |m| m.gensymb(Gns::Perthousand)),
    ("rt@nicefrac", |m| m.nice_frac(false)),
    ("rt@nfsize", |m| m.nicefrac_font()),
    ("rt@unit", |m| m.units_command(false)),
    ("rt@unitfracv", |m| m.units_command(true)),
    ("rt@unitfrac", |m| m.nice_frac(true)),
    ("rt@set", |m| m.braket_set(0)),
    ("rt@Set", |m| m.braket_set(1)),
    ("rt@Braket", |m| m.braket_set(2)),
];

/// gensymb's five commands.
#[derive(Clone, Copy, PartialEq)]
enum Gns {
    Degree,
    Celsius,
    Ohm,
    Micro,
    Perthousand,
}

impl Machine<'_, '_> {
    /// Whether textcomp is loaded (`\@ifpackageloaded{textcomp}` at `\begin{document}`).
    fn textcomp_loaded(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.packages.contains(&"textcomp"))
    }

    /// gensymb's `\degree`, `\celsius`, `\ohm`, `\micro`, `\perthousand` (module comment).
    fn gensymb(&mut self, k: Gns) {
        if self.textcomp_loaded() {
            if self.mode == Mode::Math {
                return self.not_native("gensymb's math companion symbols");
            }
            // `\textdegree` (latex.ltx's `\DeclareTextSymbolDefault{\textdegree}{TS1}`) and
            // the `\tc@check@symbol` ones.
            return match k {
                Gns::Degree => self.text_symbol_command("TS1", 176),
                Gns::Celsius => self.ts1_checked(137, 9),
                Gns::Ohm => self.ts1_checked(87, 7),
                Gns::Micro => self.ts1_checked(181, 7),
                Gns::Perthousand => self.not_native("\\textperthousand"),
            };
        }
        if matches!(k, Gns::Micro | Gns::Perthousand) {
            return self.not_native("gensymb's \\micro or \\perthousand without textcomp (undefined)");
        }
        if self.mode == Mode::Vertical {
            self.leave_vmode();
            return;
        }
        let math = self.mode == Mode::Math;
        // `^\circ` on an atom whose superscript is taken is TeX's "Double superscript".
        if math && k != Gns::Ohm && self.sup_taken() {
            return self.not_native("gensymb's ^\\circ after a superscript");
        }
        let (Some(circ), Some(omega)) = (self.si_math_char("circ"), self.si_math_char("Omega")) else {
            return self.not_native("gensymb without \\circ or \\Omega");
        };
        if math && !matches!(self.cur_fam(), -1 | 0) {
            return self.not_native("gensymb in a math alphabet");
        }
        // `\ensuremath{...}` and `$...$`: `$` with the format's `\everymath`.
        if !math && self.si_math_on().is_none() {
            return self.not_native("gensymb with \\everymath");
        }
        match k {
            Gns::Ohm => self.set_math_char(omega, 0),
            _ => {
                self.open_sup_group();
                self.set_math_char(circ, 0);
                self.end_math_group();
            }
        }
        if k == Gns::Celsius && math {
            // `\mathrm{C}`: `{\mathgroup0 C}`, whose lone atom loses its braces (§1186).
            self.open_ord_group();
            self.set_cur_fam(0);
            if self.si_chars(b"C").is_none() {
                return self.not_native("gensymb's C with an active math code");
            }
            self.end_math_group();
        }
        if !math {
            self.after_math();
            if k == Gns::Celsius {
                // `$^\circ$C`: the `C` is the document's text, with its ligatures and kerns.
                self.back_list(&[Token::char(Cat::Letter, 'C')]);
            }
        }
    }

    /// nicefrac's `\@UnitsNiceFrac[#1]{#2}{#3}` (`nicefrac.sty` 1998/08/04 v0.9b, option
    /// `nice`), `#1` empty or units' `\mathrm` (`rm`), as the tokens it leaves once its
    /// heights are known:
    /// - in a formula: `\mathchoice` of `\raisebox{r}{\ensuremath{\scriptstyle#1{#2}}}`
    ///   (`\scriptscriptstyle` in the last two), `r` the height of
    ///   `\ensuremath{<style>#1{M}}` less that one style smaller (display and text less
    ///   script, script less scriptscript), then `\mkern-2mu/\mkern-1mu` and
    ///   `\bgroup\mathchoice{\scriptstyle}{\scriptstyle}{\scriptscriptstyle}
    ///   {\scriptscriptstyle}#1{#3}\egroup`;
    /// - in text: `\raisebox{r}{\ensuremath{\mbox{\fontsize\sf@size\z@\selectfont#1{#2}}}}`,
    ///   `r` the height of `#1{M}` less its height in that font, then
    ///   `\ensuremath{\mkern-2mu}/\ensuremath{\mkern-1mu}` and
    ///   `\ensuremath{\mbox{\fontsize\sf@size\z@\selectfont#1{#3}}}`.
    ///
    /// Each height is `M`'s in its font (`\settoheight` of a box holding only it). Those
    /// `\settoheight`s set `\L@UnitsRaise…` and `\@tempdima`, names a native document cannot
    /// read.
    fn nice_frac(&mut self, rm: bool) {
        if self.leave_vmode() {
            return;
        }
        if self.next_is('[') {
            return self.not_native("\\nicefrac with a font argument");
        }
        let (mut num, mut den) = (Vec::new(), Vec::new());
        self.read_arg(&mut num);
        self.read_arg(&mut den);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let math = Token::char(Cat::MathShift, '$');
        let in_math = self.mode == Mode::Math;
        let font = match (rm && in_math, self.e.cs.get("mathrm")) {
            (false, _) => Vec::new(),
            (true, Some(id)) => vec![Token::cs(id)],
            (true, None) => return self.not_native("\\unitfrac without \\mathrm"),
        };
        // `#1{x}`: the font command, then the argument as a group.
        let arg = |x: &[Token]| -> Vec<Token> { [&font[..], &[open], x, &[close]].concat() };
        let word = |w: &str| w.chars().map(|c| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c)).collect::<Vec<_>>();
        let raised = |m: &Self, r: i32, body: Vec<Token>, t: &mut Vec<Token>| {
            t.extend([m.doc_id(doc!("raisebox")), open]);
            Self::sp_tokens(r, t);
            t.extend([close, open]);
            t.extend(body);
            t.push(close);
        };
        let mkern = self.named_cs(named!("mkern"));
        let mut t = Vec::new();
        if in_math {
            // `M`'s family in a new formula: 1 (`\mathcode`M="714D, `\fam` -1), 0 under `\mathrm`.
            if self.e.eqtb.code(Code::Math, 'M' as u32) != 0x714D {
                return self.not_native("\\nicefrac with M's math code changed");
            }
            let fam = if font.is_empty() { 1 } else { 0 };
            let h = |m: &Self, size: u32| m.e.fonts.dims(m.fam_fnt(fam, size), b'M').map_or(0, |d| d[1]);
            let (ht, hs, hss) = (h(self, 0), h(self, 16), h(self, 32));
            let styles = [named!("scriptstyle"), named!("scriptstyle"), named!("scriptscriptstyle"), named!("scriptscriptstyle")];
            t.push(self.named_cs(named!("mathchoice")));
            for (k, style) in styles.into_iter().enumerate() {
                let r = if k < 2 { ht - hs } else { hs - hss };
                let body = [&[math, self.named_cs(style)][..], &arg(&num), &[math]].concat();
                t.push(open);
                raised(self, r, body, &mut t);
                t.push(close);
            }
            t.push(mkern);
            t.extend(word("-2mu"));
            t.push(Token::char(Cat::Other, '/'));
            t.push(mkern);
            t.extend(word("-1mu"));
            t.extend([open, self.named_cs(named!("mathchoice"))]);
            for style in styles {
                t.extend([open, self.named_cs(style), close]);
            }
            t.extend(arg(&den));
            t.push(close);
        } else {
            // `\fontsize\sf@size\z@\selectfont`'s `M`, measured inside a group.
            let h1 = self.e.fonts.dims(self.cur_font(), b'M').map_or(0, |d| d[1]);
            self.begin_group(SIMPLE_GROUP);
            self.nicefrac_font();
            let h2 = self.e.fonts.dims(self.cur_font(), b'M').map_or(0, |d| d[1]);
            self.end_group(SIMPLE_GROUP);
            let small = |m: &Self, x: &[Token]| -> Vec<Token> { [&[math, m.named_cs(named!("mbox")), open, m.doc_id(doc!("rt@nfsize"))][..], &arg(x), &[close, math]].concat() };
            raised(self, h1 - h2, small(self, &num), &mut t);
            t.push(math);
            t.push(mkern);
            t.extend(word("-2mu"));
            t.extend([math, Token::char(Cat::Other, '/'), math, mkern]);
            t.extend(word("-1mu"));
            t.push(math);
            t.extend(small(self, &den));
        }
        self.back_list(&t);
    }

    /// `\fontsize\sf@size\z@\selectfont`: the current shape at the script size of the size
    /// around it (`\sf@size` after `\check@mathfonts`), `\baselineskip` zero.
    fn nicefrac_font(&mut self) {
        let (key, size) = self.font_key();
        let script = self.math_sizes(size).1;
        self.e.eqtb.set_glue(Reg { region: Region::Glue, slot: BASELINE_SKIP }, Glue::default(), false);
        self.strut_update();
        self.select_nfss(&key, script);
    }

    /// units' `\unit[value]{unit}` and `\unitfrac[value]{num}{den}` (`units.sty` 1998/08/04
    /// v0.9b, option `tight`): a nonempty value then `\,`; then `\mathrm{unit}` in a
    /// formula, `unit` in text; `\nicefrac[\mathrm]{num}{den}` in a formula,
    /// `\nicefrac{num}{den}` in text (`rt@unitfrac`, after the value).
    fn units_command(&mut self, frac: bool) {
        if self.leave_vmode() {
            return;
        }
        let mut t = if self.next_is('[') { self.bracket_arg() } else { Vec::new() };
        if !t.is_empty() {
            t.push(self.named_cs(named!(",")));
        }
        if frac {
            t.push(self.doc_id(doc!("rt@unitfrac")));
            return self.back_list(&t);
        }
        let mut unit = Vec::new();
        self.read_arg(&mut unit);
        if self.mode == Mode::Math {
            let Some(id) = self.e.cs.get("mathrm") else { return self.not_native("\\unit without \\mathrm") };
            t.extend([Token::cs(id), Token::char(Cat::BeginGroup, '{')]);
            t.extend(unit);
            t.push(Token::char(Cat::EndGroup, '}'));
        } else {
            t.extend(unit);
        }
        self.back_list(&t);
    }

    /// Whether `toks`, or a macro they name (to a depth of 8), hold a `|` character, or
    /// `\|` when `double`: what an active `|` (or braket's `\let\|`) would take.
    fn bar_in(&self, toks: &[Token], double: bool, depth: u8) -> bool {
        toks.iter().any(|t| match t.cs_id() {
            Some(id) if double && self.e.cs.name(id) == "|" => true,
            Some(id) => match self.e.eqtb.meaning(id) {
                Meaning::Macro(m) => depth == 0 || self.bar_in(self.e.toks.get(m.text), double, depth - 1),
                _ => false,
            },
            None => t.cat() == Some(Cat::Other) && t.chr() == '|',
        })
    }

    /// braket's `\set{#1}`, `\Set{#1}`, `\Braket{#1}` (`kind` 0, 1, 2; `braket.sty`), whose
    /// `|` is math-active (`\mathcode`\|32768`) inside the argument:
    /// - `\set`: `\mathinner{\lbrace\,{\mathcode…\let|\midvert #1}\,\rbrace}`, `\midvert` =
    ///   `\egroup\mid\bgroup`. The `\egroup` ends the group that made `|` active, so only the
    ///   first `|` divides: `\mathinner{\lbrace\,{A}\mid{B}\,\rbrace}`, later bars ordinary.
    /// - `\Set`: `\left\{\:{\let\|\SetDoubleVert\mathcode…\let|\SetVert #1}\:\right\}`;
    ///   `\SetVert` is `\egroup\;\middle|\;\bgroup`, or for `||` (`\@ifnextchar|`) and `\|`
    ///   `\egroup\;\middle\|\;\bgroup` (`\SavedDoubleVert`, `\middle` being e-TeX's): the first
    ///   bar divides, as in `\set`.
    /// - `\Braket`: `\begingroup…\mathcode…\left\langle{#1}\right\rangle\endgroup`, the bars
    ///   active for the whole argument, each `\egroup\,\middle|\,\bgroup` (or `\|`).
    ///
    /// A bar the active one would take that is not a dividing one at the argument's top
    /// level (in a group, after a control sequence such as `\left`, or in a macro's text) is
    /// not native; nor is a formula-less use.
    fn braket_set(&mut self, kind: u8) {
        if self.mode != Mode::Math {
            return self.not_native("braket's \\set, \\Set or \\Braket outside a formula");
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let double = kind > 0;
        // The pieces between dividing bars, and for each division whether it is double.
        let (mut pieces, mut doubles) = (vec![Vec::new()], Vec::new());
        let (mut depth, mut i) = (0, 0);
        let mut active = true;
        let mut after_cs = false;
        while i < arg.len() {
            let t = arg[i];
            i += 1;
            let bar = t.cs_id().is_none() && t.cat() == Some(Cat::Other) && t.chr() == '|';
            let dbar = double && t.cs_id().is_some_and(|id| self.e.cs.name(id) == "|");
            if active && (bar || dbar) {
                if depth > 0 || after_cs {
                    return self.not_native("braket's active bar inside a group or after a command");
                }
                let mut two = dbar;
                if bar && double {
                    // `\@ifnextchar|`: spaces skipped.
                    let k = arg[i..].iter().position(|t| t.cat() != Some(Cat::Space)).map_or(arg.len(), |k| i + k);
                    if arg.get(k).is_some_and(|t| t.cs_id().is_none() && t.cat() == Some(Cat::Other) && t.chr() == '|') {
                        two = true;
                        i = k + 1;
                    }
                }
                doubles.push(two);
                pieces.push(Vec::new());
                active = kind == 2;
                after_cs = false;
                continue;
            }
            if active && t.cs_id().is_some_and(|id| matches!(self.e.eqtb.meaning(id), Meaning::Macro(_))) && self.bar_in(&[t], double, 8) {
                return self.not_native("braket's active bar in a macro's text");
            }
            match t.cat() {
                Some(Cat::BeginGroup) => depth += 1,
                Some(Cat::EndGroup) => depth -= 1,
                _ => {}
            }
            // A command that may read the bar (`\left`, `\big`, a macro); a math character
            // (`\phi`, `\in`) reads nothing.
            if t.cat() != Some(Cat::Space) {
                after_cs = t.cs_id().is_some_and(|id| match self.e.eqtb.meaning(id) {
                    Meaning::MathCharDef(_) | Meaning::CharDef(_) => false,
                    Meaning::Macro(_) => true,
                    // The spaces, natives that read no argument.
                    _ => !matches!(self.e.cs.name(id), "," | ";" | ":" | "!" | "quad" | "qquad"),
                });
            }
            pieces.last_mut().expect("one piece").push(t);
        }
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let cs = |m: &Self, name: &str| m.e.cs.get(name).map(Token::cs);
        let (Some(mid), Some(middle), Some(medspace), Some(dvert), Some(langle)) = (cs(self, "mid"), cs(self, "middle"), cs(self, ":"), cs(self, "|"), cs(self, "langle")) else {
            return self.not_native("braket without \\mid, \\middle, \\:, \\| or \\langle");
        };
        let sep = if kind == 1 { self.named_cs(named!(";")) } else { self.named_cs(named!(",")) };
        let mut t = match kind {
            0 => vec![self.named_cs(named!("mathinner")), open, self.named_cs(named!("lbrace")), self.named_cs(named!(","))],
            1 => vec![self.named_cs(named!("left")), self.named_cs(named!("{")), medspace],
            _ => vec![self.named_cs(named!("left")), langle],
        };
        for (k, piece) in pieces.iter().enumerate() {
            if k > 0 {
                if kind == 0 {
                    t.push(mid);
                } else {
                    t.extend([sep, middle, if doubles[k - 1] { dvert } else { Token::char(Cat::Other, '|') }, sep]);
                }
            }
            t.push(open);
            t.extend(piece.iter().copied());
            t.push(close);
        }
        match kind {
            0 => t.extend([self.named_cs(named!(",")), self.named_cs(named!("rbrace")), close]),
            1 => t.extend([medspace, self.named_cs(named!("right")), self.named_cs(named!("}"))]),
            _ => t.extend([self.named_cs(named!("right")), self.named_cs(named!("rangle"))]),
        }
        self.back_list(&t);
    }
}
