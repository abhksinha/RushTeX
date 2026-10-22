//! LaTeX commands written natively: the port's strangler step (`native-port-plan.md` §2).
//!
//! A kernel command keeps its control-sequence name. `RT_NATIVE=name,…` (or `all`) binds
//! the name to [`Meaning::Kernel`] at the start of a job, after the format is loaded; the
//! macro it replaces is kept under `rt@orig@<name>`, where the token arena's collector keeps
//! it current, and the native code expands it for every case it does not settle itself. A
//! document or package that redefines the name replaces the native meaning as it would the
//! macro, so overrides need no check.
//!
//! `RT_STATE=<file>`: at every `\shipout` and at the end, every control sequence's meaning
//! and every nonzero register, sorted by name, in text, so two runs (macro and native) diff
//! with `diff`. Names a native command may leave different are left out: the commands it
//! can bind and LaTeX's scratch names ([`SCRATCH`]), which a macro reading them after the command
//! returns would be a LaTeX bug.

use super::engine_catcode::Cat;
use super::engine_eqtb::{Reg, Region};
use super::engine_prim::{Macro, Meaning};
use super::engine_run::Machine;
use super::engine_token::{Span, Token};
use crate::data::data_nfss;
use std::collections::HashMap;

/// A native command: its name and its code, given the token that named it.
type Native = fn(&mut Machine, Token);

/// Every native command, by `Meaning::Kernel` index.
static KERNEL: [(&str, Native); 1] = [("get@external@font", get_external_font)];

/// The control sequences a native command reads, looked up once per job.
const NAMES: [&str; 7] = ["f@encoding", "f@family", "f@series", "f@shape", "f@size", "curr@fontshape", "external@font"];
const F_ENCODING: usize = 0;
const F_SIZE: usize = 4;
const CURR_FONTSHAPE: usize = 5;
const EXTERNAL_FONT: usize = 6;

/// Names LaTeX uses as scratch inside one command: what native code leaves in them is not
/// compared. ltfsstrc.dtx's temporaries, and the kernel's shared scratch macros and
/// registers.
const SCRATCH: [&str; 22] = [
    "reserved@a", "reserved@b", "reserved@c", "reserved@d", "reserved@e", "reserved@f", "@tempa", "@tempb", "font@info", "sizefn@info", "mandatory@arg", "optional@arg", "best@size", "extract@fontinfo", "f@user@size", "lower@bound", "upper@bound", "@tempdima", "@tempdimb", "@tempdimc", "dimen@", "count@",
];

/// The per-job state of the native commands.
#[derive(Default)]
pub(super) struct Kernel {
    /// The names bound, by `KERNEL` index, and the IDs their macros are kept under.
    bound: Vec<(u16, u32)>,
    /// Calls settled natively and calls given to the macro, by `KERNEL` index.
    native: [u32; KERNEL.len()],
    fallback: [u32; KERNEL.len()],
    /// [`NAMES`]' IDs (`u32::MAX` for a name the format lacks).
    ids: [u32; NAMES.len()],
    /// `\curr@fontshape`'s text in the format, for a one-comparison check that no one
    /// redefined it.
    curr_fontshape: Vec<Token>,
    /// A shape's spec (the ID of the control sequence its declaration stores) checked
    /// against the NFSS table: the table's shape, or `u32::MAX` where the document or a
    /// package declared it differently. Names never change their text, so one check each.
    specs: HashMap<u32, u32>,
    name: String,
    size: String,
    list: Vec<Token>,
    /// `RT_STATE`: the file, and the dumps so far in this run.
    state_file: Option<String>,
    state: String,
}

/// Native command `i`'s name.
pub(super) fn kernel_name(i: u16) -> &'static str {
    KERNEL.get(i as usize).map_or("", |k| k.0)
}

impl Machine<'_, '_> {
    /// `RT_NATIVE` and `RT_STATE` (module documentation), read once a job.
    pub(super) fn bind_kernel(&mut self) {
        let native = std::env::var("RT_NATIVE").ok();
        let state_file = std::env::var("RT_STATE").ok();
        if native.is_none() && state_file.is_none() {
            return;
        }
        let mut k = Kernel { state_file, ..Kernel::default() };
        for (i, name) in NAMES.iter().enumerate() {
            k.ids[i] = self.e.cs.get(name).unwrap_or(u32::MAX);
        }
        if let Some(Meaning::Macro(m)) = k.ids.get(CURR_FONTSHAPE).filter(|&&id| id != u32::MAX).map(|&id| self.e.eqtb.meaning(id)) {
            k.curr_fontshape = self.e.toks.get(m.text).to_vec();
        }
        for want in native.as_deref().unwrap_or("").split(',').filter(|s| !s.is_empty()) {
            for (i, (name, _)) in KERNEL.iter().enumerate() {
                if want != "all" && want != *name {
                    continue;
                }
                let Some(id) = self.e.cs.get(name) else { continue };
                let m = self.e.eqtb.meaning(id);
                if !matches!(m, Meaning::Macro(_)) {
                    continue;
                }
                let orig = self.e.cs.intern(&format!("rt@orig@{name}"));
                self.e.eqtb.set_meaning(orig, m, true);
                self.e.eqtb.set_meaning(id, Meaning::Kernel(i as u16), true);
                k.bound.push((i as u16, orig));
            }
        }
        self.kernel = Some(Box::new(k));
    }

    /// Runs native command `i`, named by `t`.
    pub(super) fn kernel_command(&mut self, i: u16, t: Token) {
        if let Some(&(_, f)) = KERNEL.get(i as usize) {
            f(self, t);
        }
    }

    /// The macro native command `i` replaced, expanded in its place.
    fn kernel_macro(&mut self, i: u16, t: Token) {
        let orig = self.kernel.as_ref().and_then(|k| k.bound.iter().find(|b| b.0 == i)).map(|b| b.1);
        if let Some(Meaning::Macro(m)) = orig.map(|id| self.e.eqtb.meaning(id)) {
            self.macro_call(t, m);
        }
    }

    /// The characters of parameterless macro `id`'s text appended to `out`; `false` if it
    /// is not one or its text holds anything but characters.
    fn macro_chars(&self, id: u32, out: &mut String) -> bool {
        let Meaning::Macro(m) = self.e.eqtb.meaning(id) else { return false };
        let text = self.e.toks.get(m.text);
        let Some((&Token::END_MATCH, body)) = text.split_first() else { return false };
        for &t in body {
            if t.cs_id().is_some() || !matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space)) {
                return false;
            }
            out.push(t.chr());
        }
        true
    }

    /// `\get@external@font`'s result for the current shape and size when the NFSS table
    /// settles it ([`data_nfss::NfssTable::direct`]): `\external@font` defined locally, as
    /// the macro's `\edef` does; `false` (nothing assigned) for every other case.
    fn external_font_native(&mut self, k: &mut Kernel) -> bool {
        let Some(table) = data_nfss::nfss() else { return false };
        if k.ids.contains(&u32::MAX) {
            return false;
        }
        // `\curr@fontshape` as the format defines it: `\f@encoding/\f@family/…`.
        match self.e.eqtb.meaning(k.ids[CURR_FONTSHAPE]) {
            Meaning::Macro(m) if self.e.toks.get(m.text) == k.curr_fontshape.as_slice() => {}
            _ => return false,
        }
        k.name.clear();
        for (n, &id) in k.ids[F_ENCODING..F_SIZE].iter().enumerate() {
            if n > 0 {
                k.name.push('/');
            }
            if !self.macro_chars(id, &mut k.name) {
                return false;
            }
        }
        k.size.clear();
        if !self.macro_chars(k.ids[F_SIZE], &mut k.size) {
            return false;
        }
        let Some(shape) = table.shape(&k.name) else { return false };
        // The shape's macro holds one token, its spec's name (`\DeclareFontShape@`).
        let Some(shape_cs) = self.e.cs.get(&k.name) else { return false };
        let Meaning::Macro(m) = self.e.eqtb.meaning(shape_cs) else { return false };
        let spec = match self.e.toks.get(m.text) {
            [Token::END_MATCH, s] => match s.cs_id() {
                Some(s) => s,
                None => return false,
            },
            _ => return false,
        };
        let known = *k.specs.entry(spec).or_insert_with(|| if data_nfss::spec_hash(self.e.cs.name(spec).as_bytes()) == table.spec_hash(shape) { shape } else { u32::MAX });
        if known != shape {
            return false;
        }
        let Some(ext) = table.direct(shape, &k.size) else { return false };
        let data_nfss::At::Sp(at) = ext.at else { return false };
        // `\edef\external@font{\mandatory@arg\space at\the\@tempdimb}`: the name's letters
        // and others as the `.fd` file's catcodes read them, `at` from ltfsstrc, `\the`'s
        // characters all `other`.
        k.list.clear();
        k.list.push(Token::END_MATCH);
        let cat = |c: char| if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other };
        k.list.extend(ext.name().chars().map(|c| Token::char(cat(c), c)));
        k.list.push(Token::char(Cat::Space, ' '));
        k.list.extend("at".chars().map(|c| Token::char(Cat::Letter, c)));
        k.name.clear();
        super::engine_value::print_scaled(at, &mut k.name);
        k.name.push_str("pt");
        k.list.extend(k.name.chars().map(|c| Token::char(Cat::Other, c)));
        let text = self.e.toks.store(&k.list);
        self.e.eqtb.set_meaning(k.ids[EXTERNAL_FONT], Meaning::Macro(Macro { text, flags: 0 }), false);
        true
    }

    /// `RT_STATE`: one dump of the state, headed `label` (module documentation).
    pub(super) fn kernel_state(&mut self, label: &str) {
        let Some(mut k) = self.kernel.take() else { return };
        if k.state_file.is_some() {
            let mut skip: Vec<&str> = SCRATCH.to_vec();
            // Every name a native command can take, bound in this run or not, so a macro run
            // and a native run dump the same names.
            skip.extend(KERNEL.iter().map(|k| k.0));
            let ctx = self.show_ctx();
            let mut lines: Vec<String> = Vec::new();
            for id in 0..self.e.cs.len() as u32 {
                let name = self.e.cs.name(id);
                if matches!(self.e.eqtb.meaning(id), Meaning::Unset) || skip.contains(&name) || name.starts_with("rt@orig@") {
                    continue;
                }
                let mut s = String::new();
                ctx.esc(name, &mut s);
                s.push('=');
                self.print_meaning(Token::cs(id), &mut s);
                lines.push(s);
            }
            for (region, kind) in [(Region::Int, "count"), (Region::Dimen, "dimen"), (Region::Glue, "skip"), (Region::Mu, "muskip"), (Region::Toks, "toks")] {
                let end = Reg::register(region, 32767).slot + 1;
                for slot in 0..end {
                    let reg = Reg { region, slot };
                    let v = match region {
                        Region::Glue | Region::Mu => {
                            let g = self.e.eqtb.glue(reg);
                            if g == Default::default() {
                                continue;
                            }
                            format!("{} {} {} {} {}", g.width, g.stretch, g.stretch_order, g.shrink, g.shrink_order)
                        }
                        Region::Toks => {
                            let span: Span = self.e.eqtb.toks(reg);
                            if span.is_empty() {
                                continue;
                            }
                            let mut s = String::new();
                            super::engine_token::show(self.e.toks.get(span), &self.e.cs, &ctx, &mut s);
                            s
                        }
                        _ => match self.e.eqtb.word(reg) {
                            0 => continue,
                            v => v.to_string(),
                        },
                    };
                    lines.push(format!("{kind}@{slot}={v}"));
                }
            }
            lines.sort_unstable();
            k.state.push_str(&format!("== {label}\n"));
            for l in lines {
                k.state.push_str(&l);
                k.state.push('\n');
            }
        }
        self.kernel = Some(k);
    }

    /// Writes this run's `RT_STATE` dumps (the last run of a job overwrites the others).
    pub(super) fn kernel_finish(&mut self) {
        self.kernel_state("end");
        if let Some(k) = &self.kernel {
            if let Some(f) = &k.state_file {
                let _ = std::fs::write(f, &k.state);
            }
            for &(i, _) in &k.bound {
                eprintln!("RT_NATIVE: \\{} native {}, macro {}", KERNEL[i as usize].0, k.native[i as usize], k.fallback[i as usize]);
            }
        }
    }
}

/// `\get@external@font` (ltfsstrc.dtx): natively where the NFSS table settles the shape and
/// size, else LaTeX's macro.
fn get_external_font(m: &mut Machine, t: Token) {
    let Some(mut k) = m.kernel.take() else { return };
    let done = m.external_font_native(&mut k);
    if done {
        k.native[0] += 1;
    } else {
        k.fallback[0] += 1;
    }
    m.kernel = Some(k);
    if !done {
        m.kernel_macro(0, t);
    }
}
