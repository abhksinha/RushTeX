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

/// A native command: settles the call and returns `true`, or assigns nothing and returns
/// `false`, and the macro it replaced runs instead.
type Native = fn(&mut Machine, &mut Kernel) -> bool;

/// Every native command, by `Meaning::Kernel` index. A robust command is bound by its
/// body's name (`selectfont ` with its space): the `\protect` wrapper stays a macro, so
/// `\protected@edef` and moving arguments see what they always saw.
static KERNEL: [(&str, Native); 2] = [("get@external@font", |m, k| m.external_font_native(k)), ("selectfont ", |m, k| m.select_font_native(k))];
const SELECTFONT: usize = 1;

/// The control sequences native code reads or assigns, looked up once per job.
const NAMES: [&str; 13] = ["f@encoding", "f@family", "f@series", "f@shape", "f@size", "external@font", "f@linespread", "baselinestretch", "delayed@f@adjustment", "@empty", "font@name", "if@forced@series", "iffalse"];
const F_ENCODING: usize = 0;
const F_SIZE: usize = 4;
const EXTERNAL_FONT: usize = 5;
const F_LINESPREAD: usize = 6;
const BASELINESTRETCH: usize = 7;
const DELAYED_F_ADJUSTMENT: usize = 8;
const EMPTY: usize = 9;
const FONT_NAME: usize = 10;
const IF_FORCED_SERIES: usize = 11;
const IFFALSE: usize = 12;

/// The internal macros native code acts for: it runs only while each keeps the meaning the
/// format gave it (`native-port-plan.md` §5), so a package that redefines one gets the
/// macro path.
const WATCHED: [&str; 3] = ["curr@fontshape", "@forced@seriesfalse", "pickup@font"];
const W_CURR_FONTSHAPE: usize = 0;

/// Names LaTeX uses as scratch inside one command: what native code leaves in them is not
/// compared. ltfsstrc.dtx's temporaries, and the kernel's shared scratch macros and
/// registers.
const SCRATCH: [&str; 22] = [
    "reserved@a", "reserved@b", "reserved@c", "reserved@d", "reserved@e", "reserved@f", "@tempa", "@tempb", "font@info", "sizefn@info", "mandatory@arg", "optional@arg", "best@size", "extract@fontinfo", "f@user@size", "lower@bound", "upper@bound", "@tempdima", "@tempdimb", "@tempdimc", "dimen@", "count@",
];

/// A watched name and the meaning it had when the job started (with a macro's text, to
/// recognise it after the collector moves it).
struct Watch {
    id: u32,
    m: Meaning,
    text: Vec<Token>,
}

/// The per-job state of the native commands.
#[derive(Default)]
pub(super) struct Kernel {
    /// The names bound, by `KERNEL` index, and the IDs their macros are kept under.
    bound: Vec<(u16, u32)>,
    /// Calls settled natively and calls given to the macro, by `KERNEL` index.
    native: [u32; KERNEL.len()],
    fallback: [u32; KERNEL.len()],
    /// [`NAMES`]' IDs; `ready` when the format has every name and watched macro.
    ids: [u32; NAMES.len()],
    ready: bool,
    watch: Vec<Watch>,
    /// `\selectfont`'s text after `\pickup@font\font@name` (`\UseHook{selectfont}`,
    /// `\size@update`, `\enc@update`), read after the native part as the macro would.
    select_tail: Vec<Token>,
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
        for name in WATCHED {
            let id = self.e.cs.get(name).unwrap_or(u32::MAX);
            let m = if id == u32::MAX { Meaning::Unset } else { self.e.eqtb.meaning(id) };
            let text = match m {
                Meaning::Macro(mac) => self.e.toks.get(mac.text).to_vec(),
                _ => Vec::new(),
            };
            k.watch.push(Watch { id, m, text });
        }
        k.ready = !k.ids.contains(&u32::MAX) && k.watch.iter().all(|w| matches!(w.m, Meaning::Macro(_)));
        for want in native.as_deref().unwrap_or("").split(',').map(str::trim).filter(|s| !s.is_empty()) {
            for (i, (name, _)) in KERNEL.iter().enumerate() {
                if want != "all" && want != name.trim_end() {
                    continue;
                }
                let Some(id) = self.e.cs.get(name) else { continue };
                let Meaning::Macro(mac) = self.e.eqtb.meaning(id) else { continue };
                if i == SELECTFONT {
                    // The text after `\pickup@font\font@name`, the part left to the macros.
                    let (pickup, font_name) = (self.e.cs.get("pickup@font"), self.e.cs.get("font@name"));
                    let (Some(pickup), Some(font_name)) = (pickup, font_name) else { continue };
                    let text = self.e.toks.get(mac.text);
                    let Some(at) = text.windows(2).position(|w| w == [Token::cs(pickup), Token::cs(font_name)]) else { continue };
                    k.select_tail = text[at + 2..].to_vec();
                }
                let orig = self.e.cs.intern(&format!("rt@orig@{}", name.trim_end()));
                self.e.eqtb.set_meaning(orig, Meaning::Macro(mac), true);
                self.e.eqtb.set_meaning(id, Meaning::Kernel(i as u16), true);
                k.bound.push((i as u16, orig));
            }
        }
        self.kernel = Some(Box::new(k));
    }

    /// Runs native command `i`, named by `t`: natively, or the macro it replaced.
    pub(super) fn kernel_command(&mut self, i: u16, t: Token) {
        let (Some(&(_, f)), Some(mut k)) = (KERNEL.get(i as usize), self.kernel.take()) else { return };
        let done = k.ready && f(self, &mut k);
        if done {
            k.native[i as usize] += 1;
        } else {
            k.fallback[i as usize] += 1;
        }
        let orig = k.bound.iter().find(|b| b.0 == i).map(|b| b.1);
        self.kernel = Some(k);
        if !done {
            if let Some(Meaning::Macro(m)) = orig.map(|id| self.e.eqtb.meaning(id)) {
                self.macro_call(t, m);
            }
        }
    }

    /// Whether assignments made here act as the macro's would, in order: no `\globaldefs`
    /// (which would make local ones global) and no `\afterassignment` token waiting (which
    /// the macro's first assignment would insert before its later steps).
    fn plain_assignments(&self) -> bool {
        self.after_assignment.is_none() && self.e.eqtb.word(Reg { region: Region::Int, slot: super::engine_run::GLOBAL_DEFS }) == 0
    }

    /// Whether watched name `w` still has the meaning it had when the job started: the same
    /// meaning word, or (a macro the collector moved) the same flags and text, whose new
    /// meaning word is then the one compared.
    fn unchanged(&self, w: &mut Watch) -> bool {
        let m = self.e.eqtb.meaning(w.id);
        if m == w.m {
            return true;
        }
        match (m, w.m) {
            (Meaning::Macro(a), Meaning::Macro(b)) if a.flags == b.flags && self.e.toks.get(a.text) == w.text.as_slice() => {
                w.m = m;
                true
            }
            _ => false,
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

    /// `\curr@fontshape` (`enc/family/series/shape`) into `k.name` and `\f@size` into
    /// `k.size`, while `\curr@fontshape` is the format's and each part holds characters.
    fn shape_key(&mut self, k: &mut Kernel) -> bool {
        if !self.unchanged(&mut k.watch[W_CURR_FONTSHAPE]) {
            return false;
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
        self.macro_chars(k.ids[F_SIZE], &mut k.size)
    }

    /// `\get@external@font` (ltfsstrc.dtx) where the NFSS table settles the shape and size
    /// ([`data_nfss::NfssTable::direct`]): `\external@font` defined locally, as the macro's
    /// `\edef` does.
    fn external_font_native(&mut self, k: &mut Kernel) -> bool {
        let Some(table) = data_nfss::nfss() else { return false };
        if !self.plain_assignments() || !self.shape_key(k) {
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

    /// `\selectfont` (ltfssbas.dtx) where nothing but the font changes: the line spread is
    /// the one in force (`\f@linespread` is `\baselinestretch`), no series or shape
    /// adjustment is pending (`\delayed@f@adjustment` is empty) and the font of the shape
    /// and size is already loaded, so `\pickup@font` would do nothing. Then, as the macro:
    /// `\@forced@seriesfalse`, `\font@name` defined globally as that font's name, the font
    /// selected, and the rest of the macro (`\UseHook{selectfont}\size@update\enc@update`)
    /// read next.
    fn select_font_native(&mut self, k: &mut Kernel) -> bool {
        if !self.plain_assignments() || !k.watch.iter_mut().all(|w| self.unchanged(w)) {
            return false;
        }
        let m = |i: usize| self.meaning(Token::cs(k.ids[i]));
        if !self.same_meaning(m(F_LINESPREAD), m(BASELINESTRETCH)) || !self.same_meaning(m(DELAYED_F_ADJUSTMENT), m(EMPTY)) {
            return false;
        }
        if !self.shape_key(k) {
            return false;
        }
        // `\csname\curr@fontshape/\f@size\endcsname`, a font already defined.
        k.name.push('/');
        k.name.push_str(&k.size);
        let Some(font_cs) = self.e.cs.get(&k.name) else { return false };
        let Meaning::Font(f) = self.e.eqtb.meaning(font_cs) else { return false };
        // `\@forced@seriesfalse`: `\let\if@forced@series\iffalse`.
        let iffalse = self.meaning(Token::cs(k.ids[IFFALSE]));
        self.e.eqtb.set_meaning(k.ids[IF_FORCED_SERIES], iffalse, false);
        let text = self.e.toks.store(&[Token::END_MATCH, Token::cs(font_cs)]);
        self.e.eqtb.set_meaning(k.ids[FONT_NAME], Meaning::Macro(Macro { text, flags: 0 }), true);
        self.select_font(f, false);
        self.back_list(&k.select_tail);
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
