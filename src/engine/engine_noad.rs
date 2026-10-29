//! Building math lists: tex.web §1136–§1206, with e-TeX's `\middle`, in our own form.
//!
//! A math list is built in the build buffer like any other list; its noads go to the
//! machine's noad table (`engine_mlist`). §1139's `saved` words of the math groups are a
//! small stack: where a subformula goes (noad index·4 + field), `\mathchoice`'s count, and
//! whether an equation number is `\leqno`. In math mode `aux` is `incompleat_noad` (−1 for
//! none) and the nest's `delim` is e-TeX's `delim_ptr`, as an offset into the list.

use super::engine_catcode::Cat;
use super::engine_eqtb::{Code, Glue, Reg, Region};
use super::engine_mlist::{
    boxed, field_mut, half, kern, noad_node, Field, Noad, ACCENT, CHOICE, DEFAULT_CODE, DISPLAY, FRACTION, INNER, LEFT, MIDDLE, MU_GLUE, COND_MATH_GLUE, OP, ORD, OVER, RADICAL, RIGHT, STYLE, TEXT, TEXT_SIZE, UNDER, VCENTER,
};
use super::engine_node::{BoxRec, Kind, Node, Sign};
use super::engine_prim::{Meaning, Prim};
use super::engine_run::{Machine, Mode, X};
use super::engine_token::Token;

/// §269 group codes.
pub(super) const MATH_GROUP: u8 = 9;
pub(super) const VCENTER_GROUP: u8 = 12;
pub(super) const MATH_CHOICE_GROUP: u8 = 13;
pub(super) const MATH_SHIFT_GROUP: u8 = 15;
pub(super) const MATH_LEFT_GROUP: u8 = 16;

/// §1151 `var_code`: a math code whose family follows `\fam`.
const VAR_CODE: u32 = 0x7000;
const MAX_DIMEN: i32 = 0o7777777777;
const INF_PENALTY: i32 = 10_000;

// §236, §247, §224, §230 parameter slots.
pub(super) const CUR_FAM: u32 = 44;
const PRE_DISPLAY_PENALTY: u32 = 11;
const POST_DISPLAY_PENALTY: u32 = 12;
const HANG_AFTER: u32 = 41;
const PRE_DISPLAY_DIRECTION: u32 = 98;
const MATH_SURROUND: u32 = 1;
const HSIZE: u32 = 3;
const PRE_DISPLAY_SIZE: u32 = 13;
pub(super) const DISPLAY_WIDTH: u32 = 14;
pub(super) const DISPLAY_INDENT: u32 = 15;
const HANG_INDENT: u32 = 17;
const ABOVE_DISPLAY_SKIP: u32 = 3;
const BELOW_DISPLAY_SKIP: u32 = 4;
const ABOVE_DISPLAY_SHORT_SKIP: u32 = 5;
const BELOW_DISPLAY_SHORT_SKIP: u32 = 6;
const SPACE_SKIP: u32 = 12;
const EVERY_MATH: u32 = 2;
const EVERY_DISPLAY: u32 = 3;

/// §1046: the commands that make sense in math mode only.
fn math_only(p: Prim) -> bool {
    matches!(
        p,
        Prim::MathOrd
            | Prim::MathOp
            | Prim::MathBin
            | Prim::MathRel
            | Prim::MathOpen
            | Prim::MathClose
            | Prim::MathPunct
            | Prim::MathInner
            | Prim::Underline
            | Prim::Overline
            | Prim::DisplayLimits
            | Prim::Limits
            | Prim::NoLimits
            | Prim::Radical
            | Prim::MathAccent
            | Prim::Vcenter
            | Prim::DisplayStyle
            | Prim::TextStyle
            | Prim::ScriptStyle
            | Prim::ScriptScriptStyle
            | Prim::NonScript
            | Prim::MathChoice
            | Prim::Above
            | Prim::Over
            | Prim::Atop
            | Prim::AboveWithDelims
            | Prim::OverWithDelims
            | Prim::AtopWithDelims
            | Prim::Left
            | Prim::Right
            | Prim::Middle
            | Prim::MathChar
            | Prim::Delimiter
            | Prim::Mkern
            | Prim::Mskip
    )
}

impl Machine<'_, '_> {
    fn int_slot(&self, slot: u32) -> i32 {
        self.e.eqtb.word(Reg { region: Region::Int, slot })
    }

    pub(super) fn set_local(&mut self, region: Region, slot: u32, v: i32) {
        self.e.eqtb.set_word(Reg { region, slot }, v, false);
    }

    /// §1136 `push_math`.
    fn push_math(&mut self, group: u8, display: bool) {
        self.push_nest(Mode::Math, !display, -1);
        self.begin_group(group);
    }

    fn incompleat(&self) -> Option<usize> {
        usize::try_from(self.aux).ok()
    }

    /// The noad at the end of the current list, if the list has one there.
    fn tail_noad(&self) -> Option<usize> {
        (self.cur.len() > self.start as usize).then(|| self.cur[self.cur.len() - 1]).filter(|n| n.kind == Kind::Noad).map(|n| n.a as usize)
    }

    fn append_noad(&mut self, n: Noad) -> u32 {
        let i = self.add_noad(n);
        self.cur.push(noad_node(i));
        i
    }

    /// §1047 `insert_dollar_sign`.
    pub(super) fn insert_dollar_sign(&mut self, t: Token) {
        self.input.back(t);
        self.input.back(Token::char(Cat::MathShift, '$'));
        self.error("Missing $ inserted");
    }

    /// §1064 `off_save`: the token that closes the current group goes in first.
    pub(super) fn off_save(&mut self, t: Token) {
        let (close, name) = match self.groups.last() {
            None => return self.error("Extra token"),
            Some(&super::engine_run::SEMI_SIMPLE_GROUP) => (vec![Token::cs(self.e.cs.intern("endgroup"))], "\\endgroup"),
            Some(&MATH_SHIFT_GROUP) => (vec![Token::char(Cat::MathShift, '$')], "$"),
            Some(&MATH_LEFT_GROUP) => (vec![Token::cs(self.e.cs.intern("right")), Token::char(Cat::Other, '.')], "\\right."),
            Some(_) => (vec![Token::char(Cat::EndGroup, '}')], "}"),
        };
        self.input.back(t);
        self.back_list(&close);
        self.error(format!("Missing {name} inserted"));
    }

    /// The math-mode commands, and the math-only ones elsewhere (§1046); `false` when `p`
    /// is not one of them here.
    pub(super) fn math_command(&mut self, x: X, p: Prim) -> bool {
        if self.mode != Mode::Math {
            if math_only(p) {
                self.insert_dollar_sign(x.t);
                return true;
            }
            if matches!(p, Prim::Eqno | Prim::Leqno) {
                self.error(format!("You can't use \\{} in {} mode", p.name(), if self.mode == Mode::Vertical { "vertical" } else { "horizontal" }));
                return true;
            }
            return false;
        }
        match p {
            // §1154–§1156
            Prim::MathOrd | Prim::MathOp | Prim::MathBin | Prim::MathRel | Prim::MathOpen | Prim::MathClose | Prim::MathPunct | Prim::MathInner | Prim::Underline | Prim::Overline => {
                let kind = match p {
                    Prim::Underline => UNDER,
                    Prim::Overline => OVER,
                    _ => p as u8 - Prim::MathOrd as u8,
                };
                let i = self.append_noad(Noad::new(kind));
                self.scan_math(i * 4);
            }
            // §1159 `math_limit_switch`
            Prim::DisplayLimits | Prim::Limits | Prim::NoLimits => match self.tail_noad() {
                Some(q) if self.noads[q].kind == OP => self.noads[q].sub = p as u8 - Prim::DisplayLimits as u8,
                _ => self.error("Limit controls must follow a math operator"),
            },
            // §1163 `math_radical`
            Prim::Radical => {
                let d = self.scan_delimiter(true);
                let i = self.append_noad(Noad { delims: [d, 0], ..Noad::new(RADICAL) });
                self.scan_math(i * 4);
            }
            // §1165 `math_ac`; §1166: `\accent` is an error in a formula.
            Prim::MathAccent | Prim::Accent => {
                if p == Prim::Accent {
                    self.error("Please use \\mathaccent for accents in math mode");
                }
                let c = self.scan_fifteen_bit();
                let accent = self.math_char_field(c);
                let i = self.append_noad(Noad { extra: accent, ..Noad::new(ACCENT) });
                self.scan_math(i * 4);
            }
            Prim::Vcenter => self.begin_vcenter(),
            // §1169
            Prim::DisplayStyle | Prim::TextStyle | Prim::ScriptStyle | Prim::ScriptScriptStyle => self.build_style(2 * (p as u8 - Prim::DisplayStyle as u8)),
            Prim::NonScript => self.build_nonscript(),
            // §1172 `append_choices`
            Prim::MathChoice => {
                self.math_choice_noad();
                self.scan_left_brace();
            }
            Prim::Above | Prim::Over | Prim::Atop | Prim::AboveWithDelims | Prim::OverWithDelims | Prim::AtopWithDelims => self.math_fraction(p as u8 - Prim::Above as u8),
            Prim::Left | Prim::Right | Prim::Middle => self.math_left_right(x, p),
            // §1140 `start_eq_no`, in a display only.
            Prim::Eqno | Prim::Leqno => {
                if self.inner {
                    self.error(format!("You can't use \\{} in math mode", p.name()));
                } else if self.groups.last() == Some(&MATH_SHIFT_GROUP) {
                    self.math_saved.push((p == Prim::Leqno) as u32);
                    self.enter_text_math();
                } else {
                    self.off_save(x.t);
                }
            }
            // §1154
            Prim::MathChar => {
                let c = self.scan_fifteen_bit();
                self.set_math_char(c, 0);
            }
            Prim::Delimiter => {
                let c = self.scan_twenty_seven_bit();
                self.set_math_char(c / 0o10000, 0);
            }
            Prim::Char => {
                let c = self.scan_char_num();
                self.append_char(c);
            }
            // §1057, §1061: `\mkern` and `\mskip` in math units.
            Prim::Mkern => {
                let d = self.scan_dimen_in(false, true).0;
                self.cur.push(Node { sub: MU_GLUE, ..kern(d) });
            }
            Prim::Mskip => {
                let g = self.scan_glue(true);
                let n = self.e.nodes.glue_node(g, MU_GLUE);
                self.cur.push(n);
            }
            // §1046: vertical material does not belong in a formula.
            Prim::Par | Prim::Vskip | Prim::Vfil | Prim::Vfill | Prim::Vss | Prim::VfilNeg | Prim::Hrule | Prim::Unvbox | Prim::Unvcopy | Prim::End => self.insert_dollar_sign(x.t),
            // §1093: `\indent` gives an empty box `\parindent` wide.
            Prim::Indent => {
                let b = self.e.nodes.add_box(BoxRec { width: self.dimen_param(super::engine_pack::PAR_INDENT), ..BoxRec::default() });
                self.append_noad(Noad { nucleus: Field::Box(b), ..Noad::new(ORD) });
            }
            Prim::NoIndent => {}
            // §1112
            Prim::ItalCorr => self.cur.push(kern(0)),
            // §1045 `append_normal_space`.
            Prim::ExSpace => {
                let space = self.glue_param(SPACE_SKIP);
                let (g, sub) = if space != Glue::default() {
                    (space, SPACE_SKIP as u8 + 1)
                } else {
                    let p = self.e.fonts.params(self.cur_font());
                    (Glue { width: p[1], stretch: p[2], shrink: p[3], stretch_order: 0, shrink_order: 0 }, 0)
                };
                let n = self.e.nodes.glue_node(g, sub);
                self.cur.push(n);
            }
            // §1110: no list can be unboxed into a formula.
            Prim::Unhbox | Prim::Unhcopy => {
                let n = self.scan_int();
                if u32::try_from(n).ok().and_then(|n| self.e.eqtb.box_reg(n)).is_some() {
                    self.error("Incompatible list can't be unboxed");
                }
            }
            _ => return false,
        }
        true
    }

    /// A character, or a character token, in a formula; `false` when it is not math's.
    pub(super) fn math_char_token(&mut self, t: Token) -> bool {
        match t.cat() {
            // §1153
            Some(Cat::BeginGroup) => {
                let i = self.append_noad(Noad::new(ORD));
                self.input.back(t);
                self.scan_math(i * 4);
            }
            Some(Cat::EndGroup) => match self.groups.last() {
                Some(&MATH_GROUP) => self.end_math_group(),
                Some(&MATH_CHOICE_GROUP) => self.build_choices(),
                // §1069 `extra_right_brace`
                Some(&MATH_SHIFT_GROUP) => self.error("Extra }, or forgotten $"),
                Some(&MATH_LEFT_GROUP) => self.error("Extra }, or forgotten \\right"),
                _ => return false,
            },
            Some(Cat::MathShift) => {
                if self.groups.last() == Some(&MATH_SHIFT_GROUP) {
                    self.after_math();
                } else {
                    self.off_save(t);
                }
            }
            Some(Cat::Superscript) => self.sub_sup(1),
            Some(Cat::Subscript) => self.sub_sup(2),
            Some(Cat::Space) => {}
            _ => return false,
        }
        true
    }

    /// `$`, `^` and `_` outside a formula (§1090, §1137, §1046).
    pub(super) fn math_shift_outside(&mut self, t: Token) {
        match (t.cat(), self.mode) {
            (Some(Cat::MathShift), Mode::Vertical) => {
                self.input.back(t);
                self.new_graf(true);
            }
            (Some(Cat::MathShift), _) => self.init_math(),
            _ => self.insert_dollar_sign(t),
        }
    }

    /// §1151 the math code `c` of a character as a noad field.
    fn math_char_field(&self, c: u32) -> Field {
        let fam = self.int_slot(CUR_FAM);
        let fam = if c >= VAR_CODE && (0..16).contains(&fam) { fam as u8 } else { (c >> 8 & 15) as u8 };
        Field::Char { fam, c: c as u8 }
    }

    /// §1155 `set_math_char`: a noad for math code `c` (of character `chr`, which acts as an
    /// active character when its code is `"8000`).
    pub(super) fn set_math_char(&mut self, c: u32, chr: u8) {
        if c >= 0x8000 {
            self.input.back(Token::char(Cat::Active, chr as char));
            return;
        }
        let kind = if c >= VAR_CODE { ORD } else { (c >> 12) as u8 };
        let nucleus = self.math_char_field(c);
        self.append_noad(Noad { nucleus, ..Noad::new(kind) });
    }

    /// §1151 `scan_math`: the next math character or braced subformula, into field `slot`
    /// (noad·4 + field).
    fn scan_math(&mut self, slot: u32) {
        let c = loop {
            let Some(x) = self.next_non_blank_non_relax() else { return };
            let ch = match x.m {
                Meaning::Char(t) if matches!(t.cat(), Some(Cat::Letter | Cat::Other)) => u8::try_from(t.chr() as u32).ok(),
                Meaning::CharDef(n) => u8::try_from(n).ok(),
                Meaning::Prim(Prim::Char) => Some(self.scan_char_num()),
                _ => None,
            };
            if let Some(ch) = ch {
                let c = self.e.eqtb.code(Code::Math, ch as u32) as u32;
                if c == 0x8000 {
                    // §1152: the character acts as an active character.
                    self.input.back(Token::char(Cat::Active, ch as char));
                    continue;
                }
                break c;
            }
            match x.m {
                Meaning::Prim(Prim::MathChar) => break self.scan_fifteen_bit(),
                Meaning::MathCharDef(n) => break n,
                Meaning::Prim(Prim::Delimiter) => break self.scan_twenty_seven_bit() / 0o10000,
                _ => {
                    // §1153
                    self.input.back(x.t);
                    self.scan_left_brace();
                    self.math_saved.push(slot);
                    self.push_math(MATH_GROUP, false);
                    return;
                }
            }
        };
        let f = self.math_char_field(c);
        *field_mut(&mut self.noads[(slot / 4) as usize], slot % 4) = f;
    }

    /// §436 `scan_fifteen_bit_int`.
    fn scan_fifteen_bit(&mut self) -> u32 {
        let v = self.scan_int();
        u32::try_from(v).ok().filter(|&v| v <= 0x7FFF).unwrap_or_else(|| {
            self.error(format!("Bad mathchar ({v})"));
            0
        })
    }

    /// §437 `scan_twenty_seven_bit_int`.
    fn scan_twenty_seven_bit(&mut self) -> u32 {
        let v = self.scan_int();
        u32::try_from(v).ok().filter(|&v| v <= 0o777777777).unwrap_or_else(|| {
            self.error(format!("Bad delimiter code ({v})"));
            0
        })
    }

    /// §1160 `scan_delimiter`: a `\delcode`, `\delimiter`, or (after `\radical`, `r`) a
    /// 27-bit number.
    fn scan_delimiter(&mut self, r: bool) -> u32 {
        let v = if r {
            self.scan_twenty_seven_bit() as i32
        } else {
            loop {
                break match self.next_non_blank_non_relax() {
                    Some(x) => match x.m {
                        Meaning::Char(t) if matches!(t.cat(), Some(Cat::Letter | Cat::Other)) => self.e.eqtb.code(Code::Del, t.chr() as u32),
                        Meaning::Prim(Prim::Delimiter) => self.scan_twenty_seven_bit() as i32,
                        // A native that is a delimiter's macro in a formula (LaTeX's `\{`,
                        // `\ifmmode\lbrace`), which `get_x_token` would have expanded.
                        Meaning::Doc(_) => match self.math_delimiter_alias(x.t) {
                            Some(t) => {
                                self.input.back(t);
                                continue;
                            }
                            None => {
                                self.input.back(x.t);
                                -1
                            }
                        },
                        _ => {
                            self.input.back(x.t);
                            -1
                        }
                    },
                    None => -1,
                };
            }
        };
        if v < 0 {
            // §1161
            self.error("Missing delimiter (. inserted)");
            return 0;
        }
        v as u32 & 0xFF_FFFF
    }

    /// §1176 `sub_sup`: field 1 (superscript) or 2 (subscript) of the last noad, or of a
    /// new empty one.
    fn sub_sup(&mut self, k: u32) {
        let mut taken = false;
        let mut slot = None;
        if let Some(q) = self.tail_noad().filter(|&q| self.noads[q].kind < LEFT) {
            if *field_mut(&mut self.noads[q], k) == Field::Empty {
                slot = Some(q as u32 * 4 + k);
            } else {
                taken = true;
            }
        }
        let slot = match slot {
            Some(s) => s,
            None => {
                // §1177
                let i = self.append_noad(Noad::new(ORD));
                if taken {
                    self.error(if k == 1 { "Double superscript" } else { "Double subscript" });
                }
                i * 4 + k
            }
        };
        self.scan_math(slot);
    }

    /// §1181 `math_fraction`: the list so far becomes the numerator of an incompleat
    /// fraction (`c`: 0 `\above`, 1 `\over`, 2 `\atop`, plus 3 with delimiters).
    fn math_fraction(&mut self, c: u8) {
        if self.incompleat().is_some() {
            // §1183
            if c >= 3 {
                self.scan_delimiter(false);
                self.scan_delimiter(false);
            }
            if c.is_multiple_of(3) {
                self.scan_dimen(false);
            }
            self.error("Ambiguous; you need another { and }");
            return;
        }
        let start = self.start as usize;
        let (a, b) = self.e.nodes.store(&self.cur[start..]);
        self.cur.truncate(start);
        let mut n = Noad { supscr: Field::Mlist(a, b), ..Noad::new(FRACTION) };
        // §1182
        if c >= 3 {
            n.delims = [self.scan_delimiter(false), self.scan_delimiter(false)];
        }
        n.thickness = match c % 3 {
            0 => self.scan_dimen(false).0,
            1 => DEFAULT_CODE,
            _ => 0,
        };
        self.aux = self.add_noad(n) as i32;
    }

    /// §1184 `fin_mlist`: the finished list (ending with `p`, a `\right` or `\middle`), the
    /// incompleat fraction completed (§1185); the nest is popped.
    fn fin_mlist(&mut self, p: Option<Node>) -> Vec<Node> {
        let start = self.start as usize;
        let q = match self.incompleat() {
            Some(inc) => {
                let den = self.e.nodes.store(&self.cur[start..]);
                self.noads[inc].subscr = Field::Mlist(den.0, den.1);
                match p {
                    None => vec![noad_node(inc as u32)],
                    Some(p) => {
                        let Field::Mlist(a, b) = self.noads[inc].supscr else { unreachable!("a fraction's numerator is a list") };
                        let cut = (a + (self.delim.max(0) as u32 + 1)).min(b);
                        // The numerator after the cut gets a block of its own (model M frees
                        // whole blocks); the nodes before it move into `q`.
                        let num = self.e.nodes.get((cut, b)).to_vec();
                        let mut q = self.e.nodes.get((a, cut)).to_vec();
                        self.e.nodes.free_block((a, b));
                        let (na, nb) = self.e.nodes.store(&num);
                        self.noads[inc].supscr = Field::Mlist(na, nb);
                        q.extend([noad_node(inc as u32), p]);
                        q
                    }
                }
            }
            None => {
                let mut q = self.cur[start..].to_vec();
                q.extend(p);
                q
            }
        };
        self.cur.truncate(start);
        self.pop_nest();
        q
    }

    /// §1186: the `}` of a subformula; braces around a lone ordinary atom, or around an
    /// accent that is an atom's nucleus, vanish.
    pub(super) fn end_math_group(&mut self) {
        self.end_group(MATH_GROUP);
        let slot = self.math_saved.pop().unwrap_or(0);
        let p = self.fin_mlist(None);
        let (q, k) = ((slot / 4) as usize, slot % 4);
        if let [n] = p[..] {
            if n.kind == Kind::Noad {
                let m = self.noads[n.a as usize];
                if m.kind == ORD && m.subscr == Field::Empty && m.supscr == Field::Empty {
                    *field_mut(&mut self.noads[q], k) = m.nucleus;
                    return;
                }
                if m.kind == ACCENT && k == 0 && self.tail_noad() == Some(q) && self.noads[q].kind == ORD {
                    // §1187
                    let last = self.cur.len() - 1;
                    self.cur[last] = n;
                    return;
                }
            }
        }
        let (a, b) = self.e.nodes.store(&p);
        *field_mut(&mut self.noads[q], k) = Field::Mlist(a, b);
    }

    /// §1174 `build_choices`: one of `\mathchoice`'s four lists.
    fn build_choices(&mut self) {
        self.end_group(MATH_CHOICE_GROUP);
        let p = self.fin_mlist(None);
        let (a, b) = self.e.nodes.store(&p);
        let k = self.math_saved.last().copied().unwrap_or(0);
        if let Some(q) = self.tail_noad() {
            *field_mut(&mut self.noads[q], k) = Field::Mlist(a, b);
        }
        if k == 3 {
            self.math_saved.pop();
            return;
        }
        if let Some(s) = self.math_saved.last_mut() {
            *s += 1;
        }
        self.push_math(MATH_CHOICE_GROUP, false);
        self.scan_left_brace();
    }

    /// §1191 `math_left_right`, with e-TeX's `\middle`.
    fn math_left_right(&mut self, x: X, p: Prim) {
        if p != Prim::Left && self.groups.last() != Some(&MATH_LEFT_GROUP) {
            // §1192
            if self.groups.last() == Some(&MATH_SHIFT_GROUP) {
                self.scan_delimiter(false);
                self.error(format!("Extra \\{}", p.name()));
            } else {
                self.off_save(x.t);
            }
            return;
        }
        let d = self.scan_delimiter(false);
        let kind = if p == Prim::Left { LEFT } else { RIGHT };
        let sub = if p == Prim::Middle { MIDDLE } else { 0 };
        let i = self.add_noad(Noad { sub, delims: [d, 0], ..Noad::new(kind) });
        let q = if p == Prim::Left {
            vec![noad_node(i)]
        } else {
            let q = self.fin_mlist(Some(noad_node(i)));
            self.end_group(MATH_LEFT_GROUP);
            q
        };
        if p != Prim::Right {
            self.push_math(MATH_LEFT_GROUP, false);
            self.cur.extend_from_slice(&q);
            self.delim = q.len() as i32 - 1;
        } else {
            let (a, b) = self.e.nodes.store(&q);
            self.append_noad(Noad { nucleus: Field::Mlist(a, b), ..Noad::new(INNER) });
        }
    }

    /// §1139 "Go into ordinary math mode".
    /// §1169: a style noad (0 `\displaystyle`, 2 text, 4 script, 6 scriptscript).
    pub(super) fn build_style(&mut self, style: u8) {
        self.append_noad(Noad { sub: style, ..Noad::new(STYLE) });
    }

    /// A noad of `kind` whose nucleus is a formula group opened here, `sub` its limits: as
    /// `\mathop{` (§1151 then §1153), or `{` in a formula with `ORD`. The caller's tokens
    /// and a `}` token close the group, so the brace count of an alignment entry
    /// (`align_state`, §347) counts the `{` this stands for.
    pub(super) fn build_math_group(&mut self, kind: u8, sub: u8) {
        self.input.align_state += 1;
        let i = self.append_noad(Noad { sub, ..Noad::new(kind) });
        self.math_saved.push(i * 4);
        self.push_math(MATH_GROUP, false);
    }

    /// `\mathaccent c{` (§1165): the accent noad, its nucleus a formula group opened here
    /// and closed by a `}` token, as [`Self::build_math_group`].
    pub(super) fn build_math_accent(&mut self, c: u32) {
        self.input.align_state += 1;
        let accent = self.math_char_field(c);
        let i = self.append_noad(Noad { extra: accent, ..Noad::new(ACCENT) });
        self.math_saved.push(i * 4);
        self.push_math(MATH_GROUP, false);
    }

    /// `\mathchoice{` (§1172): the choice noad and its first branch's group, closed by a `}`
    /// token. Each later branch reads its own `{`.
    pub(super) fn build_math_choice(&mut self) {
        self.input.align_state += 1;
        self.math_choice_noad();
    }

    /// §1172 before the first branch's group is read.
    fn math_choice_noad(&mut self) {
        self.append_noad(Noad::new(CHOICE));
        self.math_saved.push(0);
        self.push_math(MATH_CHOICE_GROUP, false);
    }

    /// `\mskip g` (§1057), `g` in math units.
    pub(super) fn build_mskip(&mut self, g: Glue) {
        let n = self.e.nodes.glue_node(g, MU_GLUE);
        self.cur.push(n);
    }

    /// `\mkern d` (§1061).
    pub(super) fn build_mkern(&mut self, d: i32) {
        self.cur.push(Node { sub: MU_GLUE, ..kern(d) });
    }

    /// `\nonscript` (§1171).
    pub(super) fn build_nonscript(&mut self) {
        let g = self.e.nodes.glue_node(Glue::default(), COND_MATH_GLUE);
        self.cur.push(g);
    }

    /// `\thinmuskip`.
    pub(super) fn thin_mu_skip(&self) -> Glue {
        self.e.eqtb.glue(Reg { region: Region::Glue, slot: super::engine_mlist::THIN_MU_SKIP })
    }

    /// `\fam n`, local.
    pub(super) fn set_cur_fam(&mut self, n: i32) {
        self.set_local(Region::Int, CUR_FAM, n);
    }

    /// `\everymath{t}`, local.
    pub(super) fn set_every_math(&mut self, t: &[Token]) {
        let span = self.e.toks.store(t);
        self.e.eqtb.set_toks(Reg { region: Region::Toks, slot: EVERY_MATH }, span, false);
    }

    /// `\nulldelimiterspace\z@`, local.
    pub(super) fn null_delimiter_space_zero(&mut self) {
        self.set_local(Region::Dimen, super::engine_mlist::NULL_DELIMITER_SPACE, 0);
    }

    /// `\m@th`: `\mathsurround\z@`, local.
    pub(super) fn math_surround_zero(&mut self) {
        self.set_local(Region::Dimen, MATH_SURROUND, 0);
    }

    pub(super) fn enter_text_math(&mut self) {
        self.push_text_math();
        self.insert_toks(EVERY_MATH);
    }

    /// `$` in a horizontal list without `\everymath`, for a builder that does its work itself.
    pub(super) fn push_text_math(&mut self) {
        self.push_math(MATH_SHIFT_GROUP, false);
        self.set_local(Region::Int, CUR_FAM, -1);
    }

    /// §1138 `init_math`: `$` in a horizontal list; `$$` in a paragraph starts a display.
    fn init_math(&mut self) {
        let t = self.get_token();
        let display = t.is_some_and(|t| matches!(self.meaning(t), Meaning::Char(c) if c.cat() == Some(Cat::MathShift))) && !self.inner;
        if !display {
            if let Some(t) = t {
                self.input.back(t);
            }
            return self.enter_text_math();
        }
        // §1145
        let w = if self.cur.len() == self.start as usize {
            self.pop_nest();
            -MAX_DIMEN
        } else {
            self.line_break(true);
            self.pre_display_width()
        };
        // §1149: the display is the paragraph's lines `prev_graf + 2`.
        let pg = self.prev_graf;
        let shape = self.e.eqtb.par_shape();
        let (l, s) = if shape.is_empty() {
            let (hi, ha) = (self.dimen_param(HANG_INDENT), self.int_slot(HANG_AFTER));
            let hsize = self.dimen_param(HSIZE);
            if hi != 0 && ((ha >= 0 && pg + 2 > ha) || pg + 1 < -ha) {
                (hsize - hi.abs(), hi.max(0))
            } else {
                (hsize, 0)
            }
        } else {
            let k = ((pg + 2) as usize).min(shape.len()) - 1;
            (shape[k][1], shape[k][0])
        };
        self.push_math(MATH_SHIFT_GROUP, true);
        self.set_local(Region::Int, CUR_FAM, -1);
        self.set_local(Region::Dimen, PRE_DISPLAY_SIZE, w);
        self.set_local(Region::Dimen, DISPLAY_WIDTH, l);
        self.set_local(Region::Dimen, DISPLAY_INDENT, s);
        self.set_local(Region::Int, PRE_DISPLAY_DIRECTION, 0);
        self.insert_toks(EVERY_DISPLAY);
        if self.typeset && self.nest.len() == 1 {
            self.build_page();
        }
    }

    /// §1146–§1148: how far the last line before a display reaches, plus two quads; the
    /// maximum when glue there stretches or shrinks.
    fn pre_display_width(&self) -> i32 {
        let jb = self.e.nodes.boxes[self.just_box as usize];
        let quad = self.e.fonts.params(self.cur_font()).get(5).copied().unwrap_or(0);
        let mut v = jb.shift + 2 * quad;
        let mut w = -MAX_DIMEN;
        // A word counts as the nodes it stands for.
        let nodes = &self.e.nodes;
        let list = nodes.get(jb.list).iter().flat_map(|n| if n.kind == Kind::Word { nodes.word_items(n) } else { std::slice::from_ref(n) });
        for n in list {
            let (d, visible) = match n.kind {
                Kind::Char | Kind::Lig => {
                    let f = &self.e.fonts.list[n.font as usize];
                    (u8::try_from(n.a).ok().and_then(|c| f.dims(c, &self.e.fonts.info)).map_or(0, |d| d[0]), true)
                }
                Kind::Box => (self.e.nodes.boxes[n.a as usize].width, true),
                Kind::Rule => (self.e.nodes.rules[n.a as usize][0], true),
                Kind::Kern | Kind::Math => (n.a, false),
                Kind::Glue => {
                    let g = self.e.nodes.glue(n);
                    let moves = match jb.glue_sign {
                        Sign::Stretching => jb.glue_order == g.stretch_order && g.stretch != 0,
                        Sign::Shrinking => jb.glue_order == g.shrink_order && g.shrink != 0,
                        Sign::Normal => false,
                    };
                    if moves {
                        v = MAX_DIMEN;
                    }
                    (g.width, n.sub >= 100)
                }
                _ => (0, false),
            };
            if visible {
                if v < MAX_DIMEN {
                    v += d;
                    w = v;
                } else {
                    return MAX_DIMEN;
                }
            } else if v < MAX_DIMEN {
                v += d;
            }
        }
        w
    }

    /// §1195: whether the math fonts lack parameters; the formula is then deleted.
    fn math_fonts_missing(&mut self) -> bool {
        let short = |m: &Self, fam: u8, need: usize| [0, 16, 32].iter().any(|&s| m.e.fonts.params(m.fam_fnt(fam, s)).len() < need);
        let what = if short(self, 2, 22) {
            "symbol"
        } else if short(self, 3, 13) {
            "extension"
        } else {
            return false;
        };
        self.error(format!("Math formula deleted: Insufficient {what} fonts"));
        // §719 `flush_math`
        while self.cur.len() > self.start as usize {
            if let Some(n) = self.cur.pop() {
                self.flush_math_node(n);
            }
        }
        self.aux = -1;
        true
    }

    /// §1197 "Check that another `$` follows".
    fn check_dollar(&mut self) {
        match self.get_x_token(false) {
            Some(x) if matches!(x.m, Meaning::Char(c) if c.cat() == Some(Cat::MathShift)) => {}
            other => {
                if let Some(x) = other {
                    self.input.back(x.t);
                }
                self.error("Display math should end with $$");
            }
        }
    }

    /// §1194 `after_math`: the `$` that ends a formula, an equation number, or a display.
    pub(super) fn after_math(&mut self) {
        let mut danger = self.math_fonts_missing();
        let mut inner = self.inner;
        let mut leqno = false;
        let mut p = self.fin_mlist(None);
        let mut a = None;
        if inner && self.mode == Mode::Math && !self.inner {
            // The equation number, then the display it belongs to.
            self.check_dollar();
            let h = self.mlist_to_hlist(p, TEXT, false);
            a = Some(self.hpack_nodes(&h, 0, false));
            self.end_group(MATH_SHIFT_GROUP);
            leqno = self.math_saved.pop() == Some(1);
            danger = self.math_fonts_missing();
            inner = self.inner;
            p = self.fin_mlist(None);
        }
        if inner {
            // §1196
            let surround = self.dimen_param(MATH_SURROUND);
            self.cur.push(Node { sub: 0, ..Node::new(Kind::Math, surround) });
            let penalties = self.mode == Mode::Horizontal && !self.inner;
            let h = self.mlist_to_hlist(p, TEXT, penalties);
            self.cur.extend_from_slice(&h);
            self.cur.push(Node { sub: 1, ..Node::new(Kind::Math, surround) });
            self.aux = 1000;
            self.end_group(MATH_SHIFT_GROUP);
        } else {
            if a.is_none() {
                self.check_dollar();
            }
            self.finish_display(p, a, leqno, danger);
        }
        if self.mode != Mode::Math && !self.nest.iter().any(|l| l.mode == Mode::Math) {
            self.noads.clear();
        }
    }

    /// §1199–§1205: the display centred on its line, with its equation number `a`.
    fn finish_display(&mut self, p: Vec<Node>, a: Option<u32>, leqno: bool, danger: bool) {
        let h = self.mlist_to_hlist(p, DISPLAY, false);
        let start = self.cur.len();
        self.cur.extend_from_slice(&h);
        let shrink = self.measure_h(&self.cur[start..]).4;
        self.adjust = Some(Vec::new());
        let br = self.hpack(start, 0, false);
        let adjusted = self.adjust.take().unwrap_or_default();
        let list = br.list;
        let mut b = self.e.nodes.add_box(br);
        let mut w = br.width;
        let z = self.dimen_param(DISPLAY_WIDTH);
        let s = self.dimen_param(DISPLAY_INDENT);
        let (mut e, q) = match a {
            Some(a) if !danger => {
                let ew = self.e.nodes.boxes[a as usize].width;
                (ew, ew + self.mathsy(6, TEXT_SIZE))
            }
            _ => (0, 0),
        };
        // §1201: `free_node(b)`, then the same list packed again; its nodes move.
        let repack = |m: &mut Self, b: u32, width: i32| {
            let start = m.cur.len();
            m.cur.extend_from_slice(&m.e.nodes.list[list.0 as usize..list.1 as usize]);
            m.e.nodes.free_block(list);
            m.e.nodes.free_box(b);
            let r = m.hpack(start, width, true);
            m.e.nodes.add_box(r)
        };
        if w + q > z {
            // §1201
            if e != 0 && (w - shrink[0] + q <= z || shrink[1..].iter().any(|&t| t != 0)) {
                b = repack(self, b, z - q);
            } else {
                e = 0;
                if w > z {
                    b = repack(self, b, z);
                }
            }
            w = self.e.nodes.boxes[b as usize].width;
        }
        // §1202
        let mut d = half(z - w);
        if e > 0 && d < 2 * e {
            d = half(z - w - e);
            let list = self.e.nodes.boxes[b as usize].list;
            if self.e.nodes.get(list).first().is_some_and(|n| n.kind == Kind::Glue) {
                d = 0;
            }
        }
        // §1203
        self.cur.push(Node::new(Kind::Penalty, self.int_slot(PRE_DISPLAY_PENALTY)));
        let (g1, g2) = if d + s <= self.dimen_param(PRE_DISPLAY_SIZE) || leqno { (ABOVE_DISPLAY_SKIP, BELOW_DISPLAY_SKIP) } else { (ABOVE_DISPLAY_SHORT_SKIP, BELOW_DISPLAY_SHORT_SKIP) };
        let mut g2 = Some(g2);
        let param_glue = |m: &mut Self, g: u32| {
            let n = m.e.nodes.glue_node(m.glue_param(g), g as u8 + 1);
            m.cur.push(n);
        };
        if let (true, 0, Some(a)) = (leqno, e, a) {
            self.e.nodes.boxes[a as usize].shift = s;
            self.append_to_vlist(a);
            self.cur.push(Node::new(Kind::Penalty, INF_PENALTY));
        } else {
            param_glue(self, g1);
        }
        // §1204
        if let (true, Some(a)) = (e != 0, a) {
            let r = kern(z - w - e - d);
            let parts = if leqno {
                d = 0;
                [boxed(a), r, boxed(b)]
            } else {
                [boxed(b), r, boxed(a)]
            };
            b = self.hpack_nodes(&parts, 0, false);
        }
        self.e.nodes.boxes[b as usize].shift = s + d;
        self.append_to_vlist(b);
        // §1205
        if let (Some(a), 0, false) = (a, e, leqno) {
            self.cur.push(Node::new(Kind::Penalty, INF_PENALTY));
            let aw = self.e.nodes.boxes[a as usize].width;
            self.e.nodes.boxes[a as usize].shift = s + z - aw;
            self.append_to_vlist(a);
            g2 = None;
        }
        self.cur.extend_from_slice(&adjusted);
        self.cur.push(Node::new(Kind::Penalty, self.int_slot(POST_DISPLAY_PENALTY)));
        if let Some(g2) = g2 {
            param_glue(self, g2);
        }
        self.resume_after_display();
    }

    /// §1200 `resume_after_display`: the paragraph goes on after the display.
    fn resume_after_display(&mut self) {
        self.end_group(MATH_SHIFT_GROUP);
        self.prev_graf += 3;
        self.push_nest(Mode::Horizontal, false, 1000);
        self.clang = self.cur_lang();
        self.prev_graf = self.hyphen_mins() * 0o200000 + self.clang;
        // §443 "Scan an optional space".
        if let Some(x) = self.get_x_token(false) {
            if x.t.cat() != Some(Cat::Space) {
                self.input.back(x.t);
            }
        }
        if self.typeset && self.nest.len() == 1 {
            self.build_page();
        }
    }

    /// §1206: an alignment that was a display's contents, between the display's glue and
    /// penalties; `prev_depth` is the alignment's.
    pub(super) fn finish_display_align(&mut self, prev_depth: i32) {
        self.do_assignments();
        match self.get_x_token(false) {
            Some(x) if matches!(x.m, Meaning::Char(c) if c.cat() == Some(Cat::MathShift)) => self.check_dollar(),
            other => {
                if let Some(x) = other {
                    self.input.back(x.t);
                }
                self.error("Missing $$ inserted");
            }
        }
        let rows: Vec<Node> = self.cur.drain(self.start as usize..).collect();
        self.pop_nest();
        self.cur.push(Node::new(Kind::Penalty, self.int_slot(PRE_DISPLAY_PENALTY)));
        let g = self.e.nodes.glue_node(self.glue_param(ABOVE_DISPLAY_SKIP), ABOVE_DISPLAY_SKIP as u8 + 1);
        self.cur.push(g);
        self.cur.extend_from_slice(&rows);
        self.cur.push(Node::new(Kind::Penalty, self.int_slot(POST_DISPLAY_PENALTY)));
        let g = self.e.nodes.glue_node(self.glue_param(BELOW_DISPLAY_SKIP), BELOW_DISPLAY_SKIP as u8 + 1);
        self.cur.push(g);
        self.aux = prev_depth;
        self.resume_after_display();
    }

    /// §1270 `do_assignments`: assignments, up to the first command that is not one.
    pub(super) fn do_assignments(&mut self) {
        loop {
            let Some(x) = self.next_non_blank_non_relax() else { return };
            let assignment = match x.m {
                Meaning::Reg(_) | Meaning::Font(_) => true,
                Meaning::Prim(p) => matches!(
                    p,
                    Prim::Advance
                        | Prim::Multiply
                        | Prim::Divide
                        | Prim::Global
                        | Prim::Long
                        | Prim::Outer
                        | Prim::Protected
                        | Prim::Def
                        | Prim::Edef
                        | Prim::Gdef
                        | Prim::Xdef
                        | Prim::Let
                        | Prim::FutureLet
                        | Prim::CharDef
                        | Prim::MathCharDef
                        | Prim::CountDef
                        | Prim::DimenDef
                        | Prim::SkipDef
                        | Prim::MuSkipDef
                        | Prim::ToksDef
                        | Prim::Count
                        | Prim::Dimen
                        | Prim::Skip
                        | Prim::MuSkip
                        | Prim::Toks
                        | Prim::Catcode
                        | Prim::MathCode
                        | Prim::LcCode
                        | Prim::UcCode
                        | Prim::SfCode
                        | Prim::DelCode
                        | Prim::TextFont
                        | Prim::ScriptFont
                        | Prim::ScriptScriptFont
                        | Prim::Font
                        | Prim::NullFont
                        | Prim::FontDimen
                        | Prim::HyphenChar
                        | Prim::SkewChar
                        | Prim::ParShape
                        | Prim::PrevGraf
                        | Prim::SpaceFactor
                        | Prim::PrevDepth
                        | Prim::Wd
                        | Prim::Ht
                        | Prim::Dp
                        | Prim::Read
                        | Prim::ReadLine
                        | Prim::Patterns
                        | Prim::Hyphenation
                        | Prim::DeadCycles
                        | Prim::InsertPenalties
                        | Prim::BatchMode
                        | Prim::NonstopMode
                        | Prim::ScrollMode
                        | Prim::ErrorstopMode
                ) || (Prim::PageGoal as u8..=Prim::PageDepth as u8).contains(&(p as u8)),
                _ => false,
            };
            if !assignment {
                self.input.back(x.t);
                return;
            }
            self.command(x, 0);
        }
    }

    /// §1168: the `}` of `\vcenter`: its box becomes a vcenter noad.
    pub(super) fn end_vcenter(&mut self, b: u32) {
        self.append_noad(Noad { nucleus: Field::Box(b), ..Noad::new(VCENTER) });
    }
}

#[cfg(test)]
mod tests {
    use super::super::engine_names::param;
    use super::super::engine_prim::prim;
    use super::*;

    #[test]
    fn slots_match_the_table() {
        for (name, region, slot) in [
            ("fam", Region::Int, CUR_FAM),
            ("predisplaypenalty", Region::Int, PRE_DISPLAY_PENALTY),
            ("postdisplaypenalty", Region::Int, POST_DISPLAY_PENALTY),
            ("hangafter", Region::Int, HANG_AFTER),
            ("predisplaydirection", Region::Int, PRE_DISPLAY_DIRECTION),
            ("mathsurround", Region::Dimen, MATH_SURROUND),
            ("hsize", Region::Dimen, HSIZE),
            ("predisplaysize", Region::Dimen, PRE_DISPLAY_SIZE),
            ("displaywidth", Region::Dimen, DISPLAY_WIDTH),
            ("displayindent", Region::Dimen, DISPLAY_INDENT),
            ("hangindent", Region::Dimen, HANG_INDENT),
            ("abovedisplayskip", Region::Glue, ABOVE_DISPLAY_SKIP),
            ("belowdisplayskip", Region::Glue, BELOW_DISPLAY_SKIP),
            ("abovedisplayshortskip", Region::Glue, ABOVE_DISPLAY_SHORT_SKIP),
            ("belowdisplayshortskip", Region::Glue, BELOW_DISPLAY_SHORT_SKIP),
            ("spaceskip", Region::Glue, SPACE_SKIP),
            ("everymath", Region::Toks, EVERY_MATH),
            ("everydisplay", Region::Toks, EVERY_DISPLAY),
        ] {
            assert_eq!(param(name), Some(Reg { region, slot }), "{name}");
        }
    }

    #[test]
    fn noad_kinds_follow_the_primitives() {
        // §1156: `\mathord` .. `\mathinner` give the noad types in order.
        for (name, kind) in [("mathord", ORD), ("mathop", OP), ("mathinner", INNER)] {
            assert_eq!(prim(name).map(|p| p as u8 - Prim::MathOrd as u8), Some(kind), "{name}");
        }
        assert_eq!(Prim::NoLimits as u8 - Prim::DisplayLimits as u8, super::super::engine_mlist::NO_LIMITS);
        assert_eq!(Prim::ScriptScriptStyle as u8 - Prim::DisplayStyle as u8, 3);
        assert_eq!(Prim::AtopWithDelims as u8 - Prim::Above as u8, 5);
    }
}
