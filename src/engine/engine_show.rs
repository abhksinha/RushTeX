//! What a token means, as text: tex.web §296 `print_meaning` and §298 `print_cmd_chr`, for
//! `\meaning` (and later `\show`), in our own form.

use super::engine_catcode::Cat;
use super::engine_eqtb::Region;
use super::engine_prim::{Meaning, LONG, OUTER, PROTECTED};
use super::engine_run::Machine;
use super::engine_tables::{PARAMS, PRIMITIVES};
use super::engine_token::Token;

impl Machine<'_, '_> {
    /// §296 `print_meaning` of `t` into `out`.
    pub(super) fn print_meaning(&self, t: Token, out: &mut String) {
        let ctx = self.show_ctx();
        match self.meaning(t) {
            Meaning::Unset | Meaning::Undefined => out.push_str("undefined"),
            Meaning::DontExpand => ctx.esc("relax", out),
            Meaning::Prim(p) => ctx.esc(p.name(), out),
            // The macro it stands for, as LaTeX shows it.
            Meaning::Kernel(i) => match self.kernel_orig(i) {
                Some(orig) => self.print_meaning(Token::cs(orig), out),
                None => ctx.esc(super::engine_kernel::kernel_name(i), out),
            },
            Meaning::Doc(i) => ctx.esc(super::engine_doc::doc_name(i), out),
            Meaning::Native(i) => {
                let (name, cmd) = PRIMITIVES[i as usize];
                ctx.esc(name, out);
                // §296: a mark command is followed by `:` and its current text (empty
                // while nothing is typeset).
                // (e-TeX's `\topmarks` and its kin take a class number and print plain).
                if cmd == super::engine_tables::Cmd::TopBotMark && !name.ends_with('s') {
                    out.push(':');
                }
            }
            Meaning::Reg(r) => {
                // A parameter prints its name; a register `\count<n>` and its kin (§1224).
                let base = match r.region {
                    Region::Int => super::engine_tables::INT_PARAMS,
                    Region::Dimen => super::engine_tables::DIMEN_PARAMS,
                    Region::Glue => super::engine_tables::GLUE_PARAMS,
                    Region::Toks => super::engine_tables::TOKS_PARAMS,
                    Region::Mu => 0,
                };
                if r.slot < base {
                    let name = PARAMS.iter().find(|p| p.1 == r.region && p.2 as u32 == r.slot).map_or("", |p| p.0);
                    ctx.esc(name, out);
                } else {
                    let kind = match r.region {
                        Region::Int => "count",
                        Region::Dimen => "dimen",
                        Region::Glue => "skip",
                        Region::Toks => "toks",
                        Region::Mu => "muskip",
                    };
                    ctx.esc(kind, out);
                    out.push_str(&(r.slot - base).to_string());
                }
            }
            Meaning::CharDef(n) => {
                ctx.esc("char", out);
                out.push_str(&format!("\"{n:X}"));
            }
            Meaning::MathCharDef(n) => {
                ctx.esc("mathchar", out);
                out.push_str(&format!("\"{n:X}"));
            }
            Meaning::Char(c) => char_meaning(c, out),
            Meaning::Font(f) => {
                // §1261: `select font <name>[ at <size>]`.
                let font = &self.e.fonts.list[f as usize];
                out.push_str("select font ");
                out.push_str(&font.name);
                if font.size != font.design {
                    out.push_str(" at ");
                    super::engine_value::print_scaled(font.size, out);
                    out.push_str("pt");
                }
            }
            Meaning::Macro(m) => {
                // e-TeX's §296: `\protected`, `\long`, `\outer`, then `macro:` and the text.
                let mut any = false;
                for (flag, name) in [(PROTECTED, "protected"), (LONG, "long"), (OUTER, "outer")] {
                    if m.flags & flag != 0 {
                        ctx.esc(name, out);
                        any = true;
                    }
                }
                if any {
                    out.push(' ');
                }
                out.push_str("macro:");
                super::engine_token::show(self.e.toks.get(m.text), &self.e.cs, &ctx, out);
            }
        }
    }
}

/// §298 `print_cmd_chr` for a character token.
fn char_meaning(t: Token, out: &mut String) {
    let what = match t.cat() {
        Some(Cat::BeginGroup) => "begin-group character ",
        Some(Cat::EndGroup) => "end-group character ",
        Some(Cat::MathShift) => "math shift character ",
        Some(Cat::AlignTab) => "alignment tab character ",
        Some(Cat::Param) => "macro parameter character ",
        Some(Cat::Superscript) => "superscript character ",
        Some(Cat::Subscript) => "subscript character ",
        Some(Cat::Space) => "blank space ",
        Some(Cat::Letter) => "the letter ",
        _ => "the character ",
    };
    out.push_str(what);
    out.push(t.chr());
}
