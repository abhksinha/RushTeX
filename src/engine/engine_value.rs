//! Reading and writing quantities on tokens: tex.web §402–§470, in our own form.
//!
//! `scan_int` (§440), `scan_dimen` (§448) and `scan_glue` (§461) read constants and internal
//! quantities (§413 `scan_something_internal`: parameters, registers, `\chardef` constants,
//! `\catcode`); `scan_keyword` (§407) matches `pt`, `plus`, `by` and the rest, putting back
//! what it read when it fails. `\the`, `\number` and `\romannumeral` (§464–§470) turn values
//! back into character tokens.

use super::engine_catcode::Cat;
use super::engine_eqtb::{Glue, Reg, Region};
use super::engine_prim::{Meaning, Prim};
use super::engine_expr::Level;
use super::engine_run::{Machine, X};
use super::engine_token::Token;
use crate::backend::scaled::{round_decimals, times_unit};

/// §421: the largest register number (e-TeX's 32767).
const MAX_REGISTER: i32 = 32767;
/// §448 `max_dimen`.
pub const MAX_DIMEN: i32 = 0o7777777777;
const UNITY: i64 = 65536;

/// A scanned internal quantity (§410 `cur_val_level`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    Int(i32),
    Dimen(i32),
    Glue(Glue),
    /// Mu glue (§410 `mu_val`): widths in units of 2^-16 mu.
    Mu(Glue),
    Toks(super::engine_token::Span),
    /// §410 `ident_val`: a font identifier.
    Font(u16),
}

impl Value {
    /// §429 coercion down: a glue's width, a dimension's sp.
    fn int(self) -> i32 {
        match self {
            Value::Int(v) | Value::Dimen(v) => v,
            Value::Glue(g) | Value::Mu(g) => g.width,
            Value::Toks(_) | Value::Font(_) => 0,
        }
    }
}

/// Whether `x` acts as a space (§406: command `spacer`): a space token, or a name or active
/// character `\let` to one.
#[inline]
pub(super) fn blank(x: X) -> bool {
    matches!(x.m, Meaning::Char(c) if c.cat() == Some(Cat::Space))
}

#[inline]
fn other(c: char) -> Token {
    Token::char(Cat::Other, c)
}

const SPACE: Token = Token::char(Cat::Space, ' ');

impl Machine<'_, '_> {
    /// The next non-space token after expansion (§406).
    pub(super) fn next_non_blank(&mut self) -> Option<X> {
        loop {
            let x = self.get_x_token(false)?;
            if !blank(x) {
                return Some(x);
            }
        }
    }

    /// §404: the next token that is neither a space nor `\relax`, after expansion.
    pub(super) fn next_non_blank_non_relax(&mut self) -> Option<X> {
        loop {
            let x = self.get_x_token(false)?;
            if !blank(x) && x.m != Meaning::Prim(Prim::Relax) {
                return Some(x);
            }
        }
    }

    /// §405 `scan_optional_equals`.
    pub(super) fn scan_optional_equals(&mut self) {
        if let Some(x) = self.next_non_blank() {
            if x.t != other('=') {
                self.input.back(x.t);
            }
        }
    }

    /// §443: one optional space after a constant.
    pub(super) fn skip_one_space(&mut self) {
        if let Some(x) = self.get_x_token(false) {
            if !blank(x) {
                self.input.back(x.t);
            }
        }
    }

    /// §407 `scan_keyword`: matches `kw` (lowercase letters, either case in the input) after
    /// optional spaces; on failure the tokens read are put back.
    pub(super) fn scan_keyword(&mut self, kw: &str) -> bool {
        // The longest keyword is pdfTeX's `nonewwindow`; the matched tokens stay on the stack.
        debug_assert!(kw.len() <= 16);
        let mut matched = [SPACE; 16];
        let mut n = 0;
        for k in kw.chars() {
            loop {
                let Some(x) = self.get_x_token(false) else { return false };
                let t = x.t;
                if t.cs_id().is_none() && t != SPACE && (t.chr() == k || t.chr() == k.to_ascii_uppercase()) {
                    matched[n] = t;
                    n += 1;
                    break;
                }
                if n == 0 && blank(x) {
                    continue;
                }
                self.input.back(t);
                if n > 0 {
                    self.back_list(&matched[..n]);
                }
                return false;
            }
        }
        true
    }

    /// The register `\count`, `\dimen` or `\skip` names after its number (§420).
    fn scan_register(&mut self, region: Region) -> Reg {
        let n = self.scan_int();
        if !(0..=MAX_REGISTER).contains(&n) {
            self.error(format!("Bad register code ({n})"));
            return Reg::register(region, 0);
        }
        Reg::register(region, n as u32)
    }

    /// The register or parameter a token stands for in an assignment, after reading a
    /// register number if it needs one.
    pub(super) fn register_of(&mut self, m: Meaning) -> Option<Reg> {
        match m {
            Meaning::Reg(r) => Some(r),
            Meaning::Prim(Prim::Count) => Some(self.scan_register(Region::Int)),
            Meaning::Prim(Prim::Dimen) => Some(self.scan_register(Region::Dimen)),
            Meaning::Prim(Prim::Skip) => Some(self.scan_register(Region::Glue)),
            Meaning::Prim(Prim::MuSkip) => Some(self.scan_register(Region::Mu)),
            Meaning::Prim(Prim::Toks) => Some(self.scan_register(Region::Toks)),
            _ => None,
        }
    }

    /// §413 `scan_something_internal`: the quantity `x` stands for, if it is internal.
    pub(super) fn internal(&mut self, x: X) -> Option<Value> {
        let reg = match x.m {
            Meaning::CharDef(n) | Meaning::MathCharDef(n) => return Some(Value::Int(n as i32)),
            Meaning::Prim(Prim::Catcode) => {
                let n = self.scan_int();
                let c = u8::try_from(n).map_or(Cat::Other, |c| self.e.eqtb.catcode(c as char));
                return Some(Value::Int(c as i32));
            }
            Meaning::Prim(p @ (Prim::LcCode | Prim::UcCode | Prim::SfCode | Prim::MathCode | Prim::DelCode)) => {
                let n = self.scan_int();
                let v = u32::try_from(n).map_or(0, |c| self.e.eqtb.code(super::engine_assign::code_kind(p), c));
                return Some(Value::Int(v));
            }
            // e-TeX's `last_item` values: expressions and `\eTeXversion`.
            Meaning::Prim(Prim::NumExpr) => return Some(self.scan_expr(Level::Int)),
            Meaning::Prim(Prim::DimExpr) => return Some(self.scan_expr(Level::Dimen)),
            Meaning::Prim(Prim::GlueExpr) => return Some(self.scan_expr(Level::Glue)),
            Meaning::Prim(Prim::MuExpr) => return Some(self.scan_expr(Level::Mu)),
            Meaning::Prim(Prim::ETeXVersion) => return Some(Value::Int(2)),
            Meaning::Prim(Prim::FontDimen) => {
                let v = self.find_font_dimen().map_or(0, |(f, k)| self.e.fonts.list[f as usize].params[k]);
                return Some(Value::Dimen(v));
            }
            Meaning::Prim(p @ (Prim::HyphenChar | Prim::SkewChar)) => {
                let f = self.scan_font_ident() as usize;
                let f = &self.e.fonts.list[f];
                return Some(Value::Int(if p == Prim::SkewChar { f.skew_char } else { f.hyphen_char }));
            }
            Meaning::Native(i) => return self.native_value(i),
            Meaning::Prim(p @ (Prim::Wd | Prim::Ht | Prim::Dp)) => return Some(Value::Dimen(self.box_dimen(p))),
            Meaning::Font(f) => return Some(Value::Font(f)),
            Meaning::Prim(Prim::ParShape) => return Some(Value::Int(self.e.eqtb.par_shape().len() as i32)),
            Meaning::Prim(Prim::Font) => return Some(Value::Font(self.cur_font())),
            Meaning::Prim(p @ (Prim::TextFont | Prim::ScriptFont | Prim::ScriptScriptFont)) => {
                let n = self.scan_int();
                return Some(Value::Font(self.fam_font(p, n)));
            }
            // §418: `\spacefactor` and `\prevdepth` of the list being built. At the outer
            // level the lists are the native pipeline's, which reads them as 1000 and 0.
            // §422: `\prevgraf` of the innermost vertical list.
            Meaning::Prim(Prim::PrevGraf) => return Some(Value::Int(if self.mode_zero { 0 } else { self.prev_graf_of_vlist() })),
            // §418
            Meaning::Prim(p @ (Prim::SpaceFactor | Prim::PrevDepth)) if self.mode_zero => {
                self.error(format!("Improper \\{}", p.name()));
                return Some(if p == Prim::SpaceFactor { Value::Int(0) } else { Value::Dimen(0) });
            }
            // §421
            Meaning::Prim(p @ (Prim::PageGoal | Prim::PageTotal | Prim::PageStretch | Prim::PageFilStretch | Prim::PageFillStretch | Prim::PageFilllStretch | Prim::PageShrink | Prim::PageDepth)) => {
                return Some(Value::Dimen(self.page_dimen(p as usize - Prim::PageGoal as usize)));
            }
            Meaning::Prim(Prim::DeadCycles) => return Some(Value::Int(self.page.dead_cycles)),
            Meaning::Prim(Prim::InsertPenalties) => return Some(Value::Int(self.page.insert_penalties)),
            Meaning::Prim(Prim::SpaceFactor) => return Some(Value::Int(if self.building() && self.mode == super::engine_run::Mode::Horizontal { self.aux } else { 1000 })),
            Meaning::Prim(Prim::PrevDepth) => return Some(Value::Dimen(if self.building() && self.mode == super::engine_run::Mode::Vertical { self.aux } else { 0 })),
            m => self.register_of(m)?,
        };
        Some(match reg.region {
            Region::Int => Value::Int(self.e.eqtb.word(reg)),
            Region::Dimen => Value::Dimen(self.e.eqtb.word(reg)),
            Region::Glue | Region::Mu if reg.is_mu() => Value::Mu(self.e.eqtb.glue(reg)),
            Region::Glue | Region::Mu => Value::Glue(self.e.eqtb.glue(reg)),
            Region::Toks => Value::Toks(self.e.eqtb.toks(reg)),
        })
    }

    /// Signs and spaces before a number (§441); `true` when negative, and the token after.
    fn scan_signs(&mut self) -> (bool, Option<X>) {
        let mut neg = false;
        loop {
            let Some(x) = self.next_non_blank() else { return (neg, None) };
            if x.t == other('-') {
                neg = !neg;
            } else if x.t != other('+') {
                return (neg, Some(x));
            }
        }
    }

    /// §440 `scan_int`.
    pub(super) fn scan_int(&mut self) -> i32 {
        let (neg, x) = self.scan_signs();
        let Some(x) = x else { return 0 };
        let v = self.scan_int_after(x).0;
        if neg {
            v.wrapping_neg()
        } else {
            v
        }
    }

    /// §440 after the signs, from token `x`: the value and the radix of a constant (0 for an
    /// internal quantity).
    fn scan_int_after(&mut self, x: X) -> (i32, u32) {
        match self.internal(x) {
            Some(Value::Toks(_)) => {
                self.error("Missing number, treated as zero (at a token list)");
                return (0, 10);
            }
            Some(v) => return (v.int(), 0),
            None => {}
        }
        let is_other = x.t.cs_id().is_none() && x.t.cat() == Some(Cat::Other);
        if is_other && x.t.chr() == '`' {
            // §442: the character, or a one-character control sequence's name.
            let Some(t) = self.get_token() else { return (0, 10) };
            let c = match t.cs_id() {
                Some(id) => {
                    let mut it = self.e.cs.name(id).chars();
                    match (it.next(), it.next()) {
                        (Some(c), None) => c,
                        _ => {
                            self.error("Improper alphabetic constant");
                            '0'
                        }
                    }
                }
                None => {
                    // A brace used as a constant does not count in `align_state`.
                    match t.cat() {
                        Some(Cat::BeginGroup) => self.input.align_state -= 1,
                        Some(Cat::EndGroup) => self.input.align_state += 1,
                        _ => {}
                    }
                    t.chr()
                }
            };
            self.skip_one_space();
            return (c as i32, 10);
        }
        let (radix, first) = match (is_other, x.t.chr()) {
            (true, '\'') => (8, None),
            (true, '"') => (16, None),
            (true, c) if c.is_ascii_digit() => (10, Some(x.t)),
            _ => {
                // TeX shows the context; the token that stopped the number is the useful part.
                self.error(format!("Missing number, treated as zero (at {})", self.name_of(x.t)));
                self.input.back(x.t);
                return (0, 10);
            }
        };
        let digit = |t: Token| -> Option<i64> {
            if t.cs_id().is_some() {
                return None;
            }
            let c = t.chr();
            match t.cat() {
                Some(Cat::Other) if c.is_ascii_digit() => Some(c as i64 - '0' as i64),
                // §445: hexadecimal `A`–`F` as letters or others.
                Some(Cat::Other | Cat::Letter) if radix == 16 && ('A'..='F').contains(&c) => Some(c as i64 - 'A' as i64 + 10),
                _ => None,
            }
            .filter(|&d| d < radix)
        };
        let mut v: i64 = first.and_then(digit).unwrap_or(0);
        let mut any = first.is_some();
        let mut big = false;
        while let Some(x) = self.get_x_token(false) {
            match digit(x.t) {
                Some(d) => {
                    v = v * radix + d;
                    if v > i32::MAX as i64 {
                        big = true;
                        v = i32::MAX as i64;
                    }
                    any = true;
                }
                None => {
                    if !blank(x) {
                        self.input.back(x.t);
                    }
                    break;
                }
            }
        }
        if big {
            self.error("Number too big");
        }
        if !any {
            self.error("Missing number, treated as zero");
        }
        (v as i32, radix as u32)
    }

    /// §448 `scan_dimen`: a dimension (with `inf`, also `fil` units), and its order.
    pub(super) fn scan_dimen(&mut self, inf: bool) -> (i32, u8) {
        self.scan_dimen_in(inf, false)
    }

    /// §448 `scan_dimen` in points, or with `mu`, in math units.
    pub(super) fn scan_dimen_in(&mut self, inf: bool, mu: bool) -> (i32, u8) {
        let (neg, x) = self.scan_signs();
        let Some(x) = x else { return (0, 0) };
        let (v, order) = self.scan_dimen_after(x, inf, mu);
        (if neg { -v } else { v }, order)
    }

    /// The dimension starting at `x`, after its signs.
    fn scan_dimen_after(&mut self, x: X, inf: bool, mu: bool) -> (i32, u8) {
        // §449: an internal quantity; a dimension (or mu glue, with `mu`) is the answer, an
        // integer a factor.
        let (int_part, f) = match self.internal(x) {
            Some(Value::Int(v)) => (v as i64, 0i64),
            Some(Value::Toks(_)) => {
                self.error("Missing number, treated as zero (at a token list)");
                return (0, 0);
            }
            Some(v) => {
                if mu != matches!(v, Value::Mu(_)) {
                    self.error("Incompatible glue units");
                }
                return (attach_sign_check(self, v.int() as i64), 0);
            }
            None => {
                let is_point = x.t == other('.') || x.t == other(',');
                let (int_part, radix) = if is_point {
                    (0, 10)
                } else {
                    self.input.back(x.t);
                    let (n, x2) = self.scan_signs();
                    let Some(x2) = x2 else { return (0, 0) };
                    let (v, r) = self.scan_int_after(x2);
                    (if n { -v } else { v }, r)
                };
                let mut f = 0i64;
                if radix == 10 {
                    // §452: a decimal fraction, up to 17 digits, rounded.
                    let mut point = is_point;
                    if !point {
                        if let Some(y) = self.get_x_token(false) {
                            if y.t == other('.') || y.t == other(',') {
                                point = true;
                            } else {
                                self.input.back(y.t);
                            }
                        }
                    }
                    if point {
                        let mut digits: Vec<u8> = Vec::new();
                        while let Some(y) = self.get_x_token(false) {
                            let c = y.t.chr();
                            if y.t.cs_id().is_none() && y.t.cat() == Some(Cat::Other) && c.is_ascii_digit() {
                                if digits.len() < 17 {
                                    digits.push(c as u8);
                                }
                            } else {
                                if !blank(y) {
                                    self.input.back(y.t);
                                }
                                break;
                            }
                        }
                        f = round_decimals(&digits) as i64;
                    }
                }
                (int_part as i64, f)
            }
        };
        let (v, order) = self.dimen_units(int_part, f, inf, mu);
        (attach_sign_check(self, v), order)
    }

    /// §453–§458: the units after the number `int_part.f` (`f` in 2^-16), as scaled points
    /// (or 2^-16 mu), unsigned range not yet checked, and the infinity order.
    fn dimen_units(&mut self, int_part: i64, f: i64, inf: bool, mu: bool) -> (i64, u8) {
        let (mut int_part, mut f) = (int_part, f);
        let neg = int_part < 0;
        if neg {
            int_part = -int_part;
        }
        let sign = |v: i64| if neg { -v } else { v };
        // §454: `fil` units.
        if inf && self.scan_keyword("fil") {
            let mut order = 1u8;
            while self.scan_keyword("l") {
                if order == 3 {
                    self.error("Illegal unit of measure (replaced by filll)");
                } else {
                    order += 1;
                }
            }
            self.skip_one_space();
            return (sign(int_part * UNITY + f).clamp(-(MAX_DIMEN as i64), MAX_DIMEN as i64), order);
        }
        // §455: an internal quantity as the unit (mu glue in math units), or `em`, `ex`, or
        // §456 `mu`.
        if let Some(y) = self.next_non_blank() {
            match self.internal(y) {
                Some(u) => {
                    if mu && !matches!(u, Value::Mu(_) | Value::Int(_)) {
                        self.error("Incompatible glue units");
                    }
                    return (sign(times_unit(int_part, f as i32, u.int() as i64)), 0);
                }
                None => self.input.back(y.t),
            }
        }
        if mu {
            if !self.scan_keyword("mu") {
                self.error("Illegal unit of measure (mu inserted)");
            }
            let v = attach_fraction(self, int_part, f);
            self.skip_one_space();
            return (sign(v), 0);
        }
        // §455: the current font's quad (`\fontdimen6`) and x-height (`\fontdimen5`).
        for (kw, param) in [("em", 5), ("ex", 4)] {
            if self.scan_keyword(kw) {
                let font = self.cur_font() as usize;
                let unit = self.e.fonts.list.get(font).and_then(|x| x.params.get(param)).copied().unwrap_or(0) as i64;
                self.skip_one_space();
                return (sign(times_unit(int_part, f as i32, unit)), 0);
            }
        }
        // §457: `true` would undo `\mag`, which the engine keeps at 1000.
        let _ = self.scan_keyword("true");
        // §458: the physical units, exact ratios to the point.
        let ratio = if self.scan_keyword("pt") {
            None
        } else {
            let units: [(&str, i64, i64); 7] = [("in", 7227, 100), ("pc", 12, 1), ("cm", 7227, 254), ("mm", 7227, 2540), ("bp", 7227, 7200), ("dd", 1238, 1157), ("cc", 14856, 1157)];
            let mut found = None;
            for (kw, n, d) in units {
                if self.scan_keyword(kw) {
                    found = Some((n, d));
                    break;
                }
            }
            if found.is_none() {
                if self.scan_keyword("sp") {
                    self.skip_one_space();
                    return (sign(int_part), 0);
                }
                self.error("Illegal unit of measure (pt inserted)");
            }
            found
        };
        if let Some((n, d)) = ratio {
            let q = int_part * n;
            let rem = q % d;
            int_part = q / d;
            f = (n * f + UNITY * rem) / d;
            int_part += f / UNITY;
            f %= UNITY;
        }
        let v = attach_fraction(self, int_part, f);
        // §443: an optional space after the unit.
        self.skip_one_space();
        (sign(v), 0)
    }

    /// §461 `scan_glue`: an internal glue (mu glue with `mu`), or a dimension with `plus`
    /// and `minus` parts.
    pub(super) fn scan_glue(&mut self, mu: bool) -> Glue {
        self.scan_glue_spec(mu).0
    }

    /// [`Self::scan_glue`], and whether the glue is a new specification (§461 `new_spec`)
    /// rather than an internal glue's own: only an internal zero glue is `zero_glue`
    /// (every zero glue assigned is, §1229 `trap_zero_glue`), which pdfTeX's protrusion
    /// passes over.
    pub(super) fn scan_glue_spec(&mut self, mu: bool) -> (Glue, bool) {
        let (neg, x) = self.scan_signs();
        let Some(x) = x else { return (Glue::default(), true) };
        // e-TeX's `\glueexpr` and `\muexpr`, and pdfTeX's conversions, compute new glue.
        let computed = matches!(x.m, Meaning::Prim(Prim::GlueExpr | Prim::MuExpr) | Meaning::Native(_));
        let width = match self.internal(x) {
            Some(v @ (Value::Glue(g) | Value::Mu(g))) => {
                if mu != matches!(v, Value::Mu(_)) {
                    self.error("Incompatible glue units");
                }
                return if neg { (negate(g), true) } else { (g, computed) };
            }
            // §428: a font identifier is no number.
            Some(Value::Font(_)) => {
                self.error("Missing number, treated as zero");
                0
            }
            Some(Value::Int(n)) => {
                let (v, _) = self.dimen_units(n as i64, 0, false, mu);
                attach_sign_check(self, v)
            }
            Some(v @ Value::Dimen(_)) => {
                if mu {
                    self.error("Incompatible glue units");
                }
                v.int()
            }
            Some(Value::Toks(_)) => {
                self.error("Missing number, treated as zero (at a token list)");
                0
            }
            None => self.scan_dimen_after(x, false, mu).0,
        };
        let mut g = Glue { width: if neg { -width } else { width }, ..Glue::default() };
        if self.scan_keyword("plus") {
            (g.stretch, g.stretch_order) = self.scan_dimen_in(true, mu);
        }
        if self.scan_keyword("minus") {
            (g.shrink, g.shrink_order) = self.scan_dimen_in(true, mu);
        }
        (g, true)
    }

    /// §465 `the_toks`: the tokens `\the` makes of the next quantity, appended to `out`:
    /// a token list as it is, anything else as characters (catcode 12, spaces 10, §464
    /// `str_toks`).
    pub(super) fn the_toks(&mut self, out: &mut Vec<Token>) {
        let Some(x) = self.get_x_token(false) else { return };
        let mut s = String::new();
        match self.internal(x) {
            Some(Value::Toks(span)) => return out.extend_from_slice(self.e.toks.get(span)),
            Some(Value::Int(v)) => s.push_str(&v.to_string()),
            Some(Value::Dimen(v)) => {
                print_scaled(v, &mut s);
                s.push_str("pt");
            }
            Some(Value::Glue(g)) => print_spec(g, "pt", &mut s),
            Some(Value::Mu(g)) => print_spec(g, "mu", &mut s),
            // §465: a font identifier gives back the name that loaded the font.
            Some(Value::Font(f)) => {
                let ident = self.e.fonts.list[f as usize].ident;
                let id = if ident == u32::MAX { self.e.cs.intern("nullfont") } else { ident };
                return out.push(Token::cs(id));
            }
            None => {
                self.error(format!("You can't use `{}' after \\the", self.name_of(x.t)));
                return;
            }
        }
        str_toks(&s, out);
    }

    /// §468 `\number`, §469 `\romannumeral`, and `\the` (§367 `ins_the_toks`): the result
    /// is read next.
    pub(super) fn convert(&mut self, p: Prim) {
        let mut list = Vec::new();
        match p {
            Prim::Number => str_toks(&self.scan_int().to_string(), &mut list),
            Prim::RomanNumeral => {
                let mut s = String::new();
                roman(self.scan_int(), &mut s);
                str_toks(&s, &mut list);
            }
            _ => self.the_toks(&mut list),
        }
        self.back_list(&list);
    }

    /// e-TeX's and pdfTeX's conversions: `\detokenize` (a general text as characters),
    /// `\expanded` (a general text expanded as by `\edef`), `\eTeXrevision` (`.6`) and
    /// `\pdfstrcmp` (two expanded texts compared as strings: -1, 0 or 1).
    pub(super) fn etex_convert(&mut self, p: Prim, out: &mut Vec<Token>) {
        match p {
            Prim::Detokenize => {
                let span = self.scan_braced_text(false);
                let mut s = String::new();
                self.show(self.e.toks.get(span), &mut s);
                str_toks(&s, out);
            }
            Prim::Expanded => {
                let span = self.scan_braced_text(true);
                out.extend_from_slice(self.e.toks.get(span));
            }
            Prim::ETeXRevision => str_toks(".6", out),
            _ => {
                let mut a = String::new();
                let span = self.scan_braced_text(true);
                self.show(self.e.toks.get(span), &mut a);
                let mut b = String::new();
                let span = self.scan_braced_text(true);
                self.show(self.e.toks.get(span), &mut b);
                let r = match a.cmp(&b) {
                    std::cmp::Ordering::Less => "-1",
                    std::cmp::Ordering::Equal => "0",
                    std::cmp::Ordering::Greater => "1",
                };
                str_toks(r, out);
            }
        }
    }
}

/// §453 `attach_fraction`: `int_part.f`, or "Dimension too large".
fn attach_fraction(m: &mut Machine, int_part: i64, f: i64) -> i64 {
    if int_part >= 0o40000 {
        m.error("Dimension too large");
        return MAX_DIMEN as i64;
    }
    int_part * UNITY + f
}

/// §448 `attach_sign`'s range check.
fn attach_sign_check(m: &mut Machine, v: i64) -> i32 {
    if v.unsigned_abs() >= 0o10000000000 {
        m.error("Dimension too large");
        return if v < 0 { -MAX_DIMEN } else { MAX_DIMEN };
    }
    v as i32
}

/// §464 `str_toks`: characters of catcode 12, spaces of catcode 10.
pub fn str_toks(s: &str, out: &mut Vec<Token>) {
    out.extend(s.chars().map(|c| if c == ' ' { SPACE } else { other(c) }));
}

fn negate(g: Glue) -> Glue {
    Glue { width: -g.width, stretch: -g.stretch, shrink: -g.shrink, ..g }
}

/// §103 `print_scaled`: the shortest decimal that reads back as `s` sp.
pub fn print_scaled(mut s: i32, out: &mut String) {
    if s < 0 {
        out.push('-');
        s = -s;
    }
    out.push_str(&(s / 65536).to_string());
    out.push('.');
    let mut s = 10 * (s % 65536) as i64 + 5;
    let mut delta: i64 = 10;
    loop {
        if delta > UNITY {
            s += 0o100000 - 50000; // round the last digit
        }
        out.push(char::from(b'0' + (s / UNITY) as u8));
        s = 10 * (s % UNITY);
        delta *= 10;
        if s <= delta {
            break;
        }
    }
}

/// §178 `print_spec` with unit `pt`, and §177 `print_glue` for its parts.
/// §178 `print_spec` with the unit `pt` or `mu`.
pub fn print_spec(g: Glue, unit: &str, out: &mut String) {
    print_scaled(g.width, out);
    out.push_str(unit);
    for (label, v, order) in [(" plus ", g.stretch, g.stretch_order), (" minus ", g.shrink, g.shrink_order)] {
        if v != 0 {
            out.push_str(label);
            print_scaled(v, out);
            if order > 0 {
                out.push_str("fil");
                for _ in 1..order {
                    out.push('l');
                }
            } else {
                out.push_str(unit);
            }
        }
    }
}

/// §69 `print_roman_int`: lowercase roman numerals; nothing for `n <= 0`.
pub fn roman(mut n: i32, out: &mut String) {
    const J: &[u8] = b"m2d5c2l5x2v5i";
    let mut j = 0usize;
    let mut v: i32 = 1000;
    loop {
        while n >= v {
            out.push(J[j] as char);
            n -= v;
        }
        if n <= 0 {
            return;
        }
        let mut k = j + 2;
        let mut u = v / (J[k - 1] - b'0') as i32;
        if J[k - 1] == b'2' {
            k += 2;
            u /= (J[k - 1] - b'0') as i32;
        }
        if n + u >= v {
            out.push(J[k] as char);
            n += u;
        } else {
            j += 2;
            v /= (J[j - 1] - b'0') as i32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaled_printing() {
        let p = |v: i32| {
            let mut s = String::new();
            print_scaled(v, &mut s);
            s
        };
        assert_eq!(p(65536), "1.0");
        assert_eq!(p(-32768), "-0.5");
        assert_eq!(p(26214), "0.4");
        assert_eq!(p(550500), "8.39996");
    }

    #[test]
    fn roman_numerals() {
        for (n, r) in [(1, "i"), (4, "iv"), (9, "ix"), (14, "xiv"), (40, "xl"), (1994, "mcmxciv"), (0, "")] {
            let mut s = String::new();
            roman(n, &mut s);
            assert_eq!(s, r, "{n}");
        }
    }
}
