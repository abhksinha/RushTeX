//! Assignments other than definitions: tex.web §1224 (`\chardef` and its kin), §1228
//! (registers and parameters), §1236–§1240 (`\advance`, `\multiply`, `\divide`), §1221
//! (`\futurelet`) and §1269 (`\afterassignment`), in our own form.

use super::engine_catcode::Cat;
use super::engine_eqtb::{Code, Glue, Reg, Region};
use super::engine_token::Token;
use super::engine_prim::{Meaning, Prim};
use super::engine_run::{Machine, X};
use super::engine_value::MAX_DIMEN;

impl Machine<'_, '_> {
    /// §1228: `<register or parameter>[=]<value>`.
    pub(super) fn assign_register(&mut self, reg: Reg, global: bool) {
        self.scan_optional_equals();
        match reg.region {
            Region::Int => {
                let v = self.scan_int();
                self.e.eqtb.set_word(reg, v, global);
            }
            Region::Dimen => {
                let v = self.scan_dimen(false).0;
                self.e.eqtb.set_word(reg, v, global);
            }
            Region::Glue | Region::Mu => {
                let g = self.scan_glue(reg.is_mu());
                self.e.eqtb.set_glue(reg, g, global);
            }
            Region::Toks => {
                // §1226: another token list is copied; otherwise a braced general text.
                let Some(x) = self.next_non_blank_non_relax() else { return };
                if let Some(src) = self.register_of(x.m).filter(|r| r.region == Region::Toks) {
                    let span = self.e.eqtb.toks(src);
                    self.e.eqtb.set_toks(reg, span, global);
                    return;
                }
                if x.t.cs_id().is_some() || x.t.cat() != Some(super::engine_catcode::Cat::BeginGroup) {
                    self.error("Missing { inserted");
                    self.input.back(x.t);
                }
                let mut span = self.scan_general_text(false);
                // §1226: `\output` keeps its braces.
                if reg == (Reg { region: Region::Toks, slot: 0 }) {
                    let mut list = Vec::with_capacity(span.len() as usize + 2);
                    list.push(Token::char(super::engine_catcode::Cat::BeginGroup, '{'));
                    list.extend_from_slice(self.e.toks.get(span));
                    list.push(Token::char(super::engine_catcode::Cat::EndGroup, '}'));
                    span = self.e.toks.store(&list);
                }
                self.e.eqtb.set_toks(reg, span, global);
            }
        }
    }

    /// §1236 `do_register_command` for `\advance`, `\multiply` and `\divide` (`p`).
    pub(super) fn register_command(&mut self, p: Prim, global: bool) {
        let Some(x) = self.get_x_token(false) else { return };
        let Some(reg) = self.register_of(x.m) else {
            self.error(format!("You can't use `{}' after \\{}", self.name_of(x.t), p.name()));
            return;
        };
        let _ = self.scan_keyword("by");
        let old = self.e.eqtb.glue(reg);
        let result = match p {
            _ if reg.region == Region::Toks => {
                self.error(format!("You can't use a token list after \\{}", p.name()));
                return;
            }
            Prim::Advance => match reg.region {
                Region::Int => Some(Glue { width: old.width.wrapping_add(self.scan_int()), ..old }),
                Region::Dimen => {
                    let v = self.scan_dimen(false).0;
                    let w = old.width as i64 + v as i64;
                    (w.abs() <= MAX_DIMEN as i64).then_some(Glue { width: w as i32, ..old })
                }
                _ => {
                    let g = self.scan_glue(reg.is_mu());
                    Some(add_glue(old, g))
                }
            },
            Prim::Multiply => {
                let n = self.scan_int() as i64;
                let mul = |v: i32| i32::try_from(v as i64 * n).ok();
                match reg.region {
                    Region::Glue | Region::Mu => match (mul(old.width), mul(old.stretch), mul(old.shrink)) {
                        (Some(width), Some(stretch), Some(shrink)) => Some(Glue { width, stretch, shrink, ..old }),
                        _ => None,
                    },
                    _ => mul(old.width).filter(|&w| reg.region == Region::Int || w.abs() <= MAX_DIMEN).map(|width| Glue { width, ..old }),
                }
            }
            _ => {
                // §1240 `x_over_n`: truncation toward zero; dividing by 0 is an error.
                let n = self.scan_int();
                if n == 0 {
                    None
                } else {
                    Some(Glue { width: old.width / n, stretch: old.stretch / n, shrink: old.shrink / n, ..old })
                }
            }
        };
        match result {
            None => self.error("Arithmetic overflow"),
            Some(g) if matches!(reg.region, Region::Glue | Region::Mu) => self.e.eqtb.set_glue(reg, g, global),
            Some(g) => self.e.eqtb.set_word(reg, g.width, global),
        }
    }

    /// §1224 `shorthand_def`: `\chardef`, `\mathchardef`, `\countdef`, `\dimendef`, `\skipdef`.
    pub(super) fn shorthand_def(&mut self, p: Prim, global: bool) {
        let Some(id) = self.get_r_token() else { return };
        // §1224: the name means `\relax` while its value is read.
        self.e.eqtb.set_meaning(id, Meaning::Prim(Prim::Relax), global);
        self.scan_optional_equals();
        let n = self.scan_int();
        let m = match p {
            Prim::CharDef | Prim::MathCharDef => {
                let max = if p == Prim::CharDef { 255 } else { 0x8000 };
                if !(0..=max).contains(&n) {
                    self.error(format!("Bad character code ({n})"));
                    return;
                }
                if p == Prim::CharDef {
                    Meaning::CharDef(n as u32)
                } else {
                    Meaning::MathCharDef(n as u32)
                }
            }
            _ => {
                let region = match p {
                    Prim::CountDef => Region::Int,
                    Prim::DimenDef => Region::Dimen,
                    Prim::ToksDef => Region::Toks,
                    Prim::MuSkipDef => Region::Mu,
                    _ => Region::Glue,
                };
                if !(0..=32767).contains(&n) {
                    self.error(format!("Bad register code ({n})"));
                    return;
                }
                Meaning::Reg(Reg::register(region, n as u32))
            }
        };
        self.e.eqtb.set_meaning(id, m, global);
    }

    /// §1221 `\futurelet\cs<t1><t2>`: `\cs` means `<t2>`; both tokens are read again.
    pub(super) fn future_let(&mut self, global: bool) {
        let Some(id) = self.get_r_token() else { return };
        let (Some(a), Some(b)) = (self.get_token(), self.get_token()) else { return };
        self.input.back(b);
        self.input.back(a);
        let m = self.meaning(b);
        self.e.eqtb.set_meaning(id, m, global);
    }

    /// §1232 `def_code`: `\catcode`, `\lccode`, `\uccode`, `\sfcode`, `\mathcode`,
    /// `\delcode` `<char>[=]<value>`, each checked against its range.
    pub(super) fn def_code(&mut self, p: Prim, global: bool) {
        let ch = self.scan_int();
        self.scan_optional_equals();
        let v = self.scan_int();
        let Ok(c) = u32::try_from(ch).map_err(|_| ()).and_then(|c| if c <= 255 { Ok(c) } else { Err(()) }) else {
            return self.error(format!("Bad character code ({ch})"));
        };
        if p == Prim::Catcode {
            match u8::try_from(v).ok().and_then(Cat::from_u8) {
                Some(cat) => self.e.eqtb.set_catcode(c as u8, cat, global),
                None => self.error(format!("Invalid code ({v}), should be in the range 0..15")),
            }
            return;
        }
        let kind = code_kind(p);
        let min = if kind == Code::Del { -0xFF_FFFF } else { 0 };
        if v < min || v > kind.max() {
            return self.error(format!("Invalid code ({v}), should be at most {}", kind.max()));
        }
        self.e.eqtb.set_code(kind, c, v, global);
    }

    /// §1285 `shift_case`: `\lowercase` or `\uppercase` of a general text; characters with a
    /// nonzero code change, keeping their catcode, and the result is read next.
    pub(super) fn case_shift(&mut self, upper: bool) {
        let span = self.scan_braced_text(false);
        let kind = if upper { Code::Uc } else { Code::Lc };
        let list: Vec<Token> = self
            .e
            .toks
            .get(span)
            .iter()
            .map(|&t| match (t.cs_id(), t.cat()) {
                (None, Some(cat)) if (t.chr() as u32) < 256 => {
                    let c = self.e.eqtb.code(kind, t.chr() as u32);
                    if c != 0 {
                        Token::char(cat, char::from_u32(c as u32).unwrap_or(t.chr()))
                    } else {
                        t
                    }
                }
                _ => t,
            })
            .collect();
        self.back_list(&list);
    }

    /// §1269: after the next assignment, the saved token is read.
    pub(super) fn after_assignment(&mut self) {
        if let Some(t) = self.after_assignment.take() {
            self.input.back(t);
        }
    }

    /// A `\let`-able meaning of `x` for the main loop: a register meaning or `\count`-like
    /// primitive names a quantity to assign.
    pub(super) fn assignment_target(&mut self, x: X) -> Option<Reg> {
        match x.m {
            Meaning::Reg(_) | Meaning::Prim(Prim::Count | Prim::Dimen | Prim::Skip | Prim::MuSkip | Prim::Toks) => self.register_of(x.m),
            _ => None,
        }
    }
}

/// §1239: glue plus glue; a stretch or shrink of a higher order wins.
fn add_glue(a: Glue, b: Glue) -> Glue {
    let mut r = Glue { width: a.width.wrapping_add(b.width), ..a };
    if a.stretch_order == b.stretch_order {
        r.stretch = a.stretch.wrapping_add(b.stretch);
    } else if a.stretch_order < b.stretch_order && b.stretch != 0 {
        r.stretch = b.stretch;
        r.stretch_order = b.stretch_order;
    }
    if a.shrink_order == b.shrink_order {
        r.shrink = a.shrink.wrapping_add(b.shrink);
    } else if a.shrink_order < b.shrink_order && b.shrink != 0 {
        r.shrink = b.shrink;
        r.shrink_order = b.shrink_order;
    }
    if r.stretch == 0 {
        r.stretch_order = 0;
    }
    if r.shrink == 0 {
        r.shrink_order = 0;
    }
    r
}

/// The code table a `def_code` primitive other than `\catcode` assigns.
pub(super) fn code_kind(p: Prim) -> Code {
    match p {
        Prim::LcCode => Code::Lc,
        Prim::UcCode => Code::Uc,
        Prim::SfCode => Code::Sf,
        Prim::MathCode => Code::Math,
        _ => Code::Del,
    }
}
