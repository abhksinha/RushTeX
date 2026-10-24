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
/// The shape switches are `\protected` macros (ltfssini.dtx's `\reinstall@nfss@defs`), so
/// they are bound by their own names.
static KERNEL: [(&str, Native); 8] = [
    ("get@external@font", |m, k| m.external_font_native(k)),
    ("selectfont ", |m, k| m.select_font_native(k)),
    ("itshape", |m, k| m.shape_switch_native(k)),
    ("upshape", |m, k| m.shape_switch_native(k)),
    ("slshape", |m, k| m.shape_switch_native(k)),
    ("scshape", |m, k| m.shape_switch_native(k)),
    ("bfseries ", |m, k| m.series_switch_native(k)),
    ("mdseries ", |m, k| m.series_switch_native(k)),
];
const SELECTFONT: usize = 1;
const FIRST_SHAPE: usize = 2;

/// The control sequences native code reads or assigns, looked up once per job.
const NAMES: [&str; 20] = [
    "f@encoding", "f@family", "f@series", "f@shape", "f@size", "external@font", "f@linespread", "baselinestretch", "delayed@f@adjustment", "@empty", "font@name", "if@forced@series", "iffalse", "delayed@merge@font@series", "delayed@merge@font@shape", "merge@font@series@without@substitution", "merge@font@shape@without@substitution", "edef", "f@series@saved", "f@shape@saved",
];
const F_ENCODING: usize = 0;
const F_FAMILY: usize = 1;
const F_SERIES: usize = 2;
const F_SHAPE: usize = 3;
const F_SIZE: usize = 4;
const EXTERNAL_FONT: usize = 5;
const F_LINESPREAD: usize = 6;
const BASELINESTRETCH: usize = 7;
const DELAYED_F_ADJUSTMENT: usize = 8;
const EMPTY: usize = 9;
const FONT_NAME: usize = 10;
const IF_FORCED_SERIES: usize = 11;
const IFFALSE: usize = 12;
const DELAYED_MERGE_SERIES: usize = 13;
const DELAYED_MERGE_SHAPE: usize = 14;
const MERGE_SERIES: usize = 15;
const MERGE_SHAPE: usize = 16;
const EDEF: usize = 17;
const F_SERIES_SAVED: usize = 18;
const F_SHAPE_SAVED: usize = 19;

/// The names the switches read or assign (`\expand@font@defaults`' results among them),
/// looked up once per job.
const SWITCH_NAMES: [&str; 20] = [
    "hook_use:n", "__hook_use_initialized:n", "not@math@alphabet", "fontshape", "fontseries", "expand@font@defaults", "ifx", "else", "fi", "f@family", "rmdef@ult", "sfdef@ult", "ttdef@ult", "rmdefault", "sfdefault", "ttdefault", "bfdefault", "mddefault", "bfdef@ult", "mddef@ult",
];
const S_HOOK_USE: usize = 0;
const S_HOOK_USE_INITIALIZED: usize = 1;
const S_F_FAMILY: usize = 9;
const S_RMDEF: usize = 10;
const S_RMDEFAULT: usize = 13;
const S_BFDEF: usize = 18;

/// The internal macros the switches act for, watched as [`WATCHED`] is. The switches' own
/// hooks are in the tails they leave to the macros; the hook `expand@font@defaults` runs
/// inside `\expand@font@defaults`, so it must be empty ([`Machine::hook_empty`]).
const SWITCH_WATCHED: [&str; 11] = ["not@math@alphabet", "fontshape", "fontshape ", "fontseries", "fontseries ", "expand@font@defaults", "maybe@update@bfseries@defaults", "maybe@update@mdseries@defaults", "@forced@seriesfalse", "UseHook", "__hook_use_initialized:n"];
/// The hook's code, its one-time code and its next-use code (lthooks.dtx), as `\__hook <name>`
/// runs them.
const DEFAULTS_HOOK: [&str; 3] = ["__hook expand@font@defaults", "__hook_toplevel expand@font@defaults", "__hook_next expand@font@defaults"];

/// A switch's macro read at binding: the default it requests (`\itdefault`; for a series
/// switch `\bfseries@rm`, `@sf`, `@tt` and `\bfdefault`, by family), its `\…default@previous`,
/// and the text after its native part (`\selectfont`, or `\UseHook{bfseries}\selectfont`).
#[derive(Default)]
struct Switch {
    pick: [u32; 4],
    previous: u32,
    default: u32,
    tail: Vec<Token>,
}

/// The internal macros native code acts for: it runs only while each keeps the meaning the
/// format gave it (`native-port-plan.md` §5), so a package that redefines one gets the
/// macro path.
const WATCHED: [&str; 13] = [
    "curr@fontshape", "@forced@seriesfalse", "pickup@font", "merge@font@series@without@substitution", "merge@font@series@without@substitution@", "merge@font@shape@without@substitution", "merge@font@shape@without@substitution@", "set@target@series", "series@maybe@drop@one@m", "series@maybe@drop@one@m@x", "series@drop@one@m", "maybe@load@fontshape", "try@load@fontshape",
];
const W_CURR_FONTSHAPE: usize = 0;

/// Names LaTeX uses as scratch inside one command: what native code leaves in them is not
/// compared. ltfsstrc.dtx's temporaries, `\series@maybe@drop@one@m`'s list test, and the
/// kernel's shared scratch macros and registers.
const SCRATCH: [&str; 24] = [
    "in@@", "series@check@toks", "reserved@a", "reserved@b", "reserved@c", "reserved@d", "reserved@e", "reserved@f", "@tempa", "@tempb", "font@info", "sizefn@info", "mandatory@arg", "optional@arg", "best@size", "extract@fontinfo", "f@user@size", "lower@bound", "upper@bound", "@tempdima", "@tempdimb", "@tempdimc", "dimen@", "count@",
];

/// A watched name and the meaning it had when the job started (with a macro's text, to
/// recognise it after the collector moves it).
struct Watch {
    id: u32,
    m: Meaning,
    text: Vec<Token>,
    /// A known package patch that leaves the macro's effect the same, or empty.
    alt: Vec<Token>,
}

impl Watch {
    fn of(m: &Machine<'_, '_>, name: &str) -> Watch {
        let id = m.e.cs.get(name).unwrap_or(u32::MAX);
        let mean = if id == u32::MAX { Meaning::Unset } else { m.e.eqtb.meaning(id) };
        let text = match mean {
            Meaning::Macro(mac) => m.e.toks.get(mac.text).to_vec(),
            _ => Vec::new(),
        };
        Watch { id, m: mean, text, alt: Vec::new() }
    }
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
    /// `\selectfont`'s merged series and shape, a change rule's target, and the rule
    /// names the macro's `\csname` would make `\relax`.
    series: Vec<Token>,
    shape: Vec<Token>,
    arg: Vec<Token>,
    relax: Vec<u32>,
    /// Names whose definition binds them again ([`Machine::kernel_defined`]), with their
    /// `KERNEL` index.
    on_def: Vec<(u32, u16)>,
    /// The command running, by `KERNEL` index.
    cur: u16,
    /// [`SWITCH_NAMES`]' IDs, the switches' watches and macros (by `KERNEL` index less
    /// `FIRST_SHAPE`); `switch_ready` when the format has them all.
    sids: [u32; SWITCH_NAMES.len()],
    switch_watch: Vec<Watch>,
    switches: Vec<Switch>,
    switch_ready: bool,
    /// [`DEFAULTS_HOOK`]' IDs, `u32::MAX` for a name the format lacks.
    hook: [u32; 3],
    /// `\expand@font@defaults`' five values.
    defaults: [Vec<Token>; 5],
    /// `RT_STATE`: the file, and the dumps so far in this run.
    state_file: Option<String>,
    state: String,
}

/// `\series@maybe@drop@one@m` (ltfssaxes.dtx): a series in its list (`ulm`, `bm`, `mc`,
/// ...) loses its first `m` (`\series@drop@one@m`'s `#1m#2m`), any other is kept.
fn drop_one_m(v: &[Token], out: &mut Vec<Token>) {
    const LIST: [&str; 17] = ["ulm", "elm", "lm", "slm", "mm", "sbm", "bm", "ebm", "ubm", "muc", "mec", "mc", "msc", "msx", "mx", "mex", "mux"];
    let m = Token::char(Cat::Letter, 'm');
    let listed = LIST.iter().any(|e| e.len() == v.len() && e.chars().zip(v).all(|(c, &t)| Token::char(Cat::Letter, c) == t));
    match v.iter().position(|&t| t == m) {
        // `v m`: the text before its first `m` and between that and the next.
        Some(first) if listed => {
            out.extend_from_slice(&v[..first]);
            let rest = &v[first + 1..];
            out.extend_from_slice(&rest[..rest.iter().position(|&t| t == m).unwrap_or(rest.len())]);
        }
        _ => out.extend_from_slice(v),
    }
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
            let Watch { id, m, text, .. } = Watch::of(self, name);
            // listings.sty puts `\def\space{ }` before `\try@load@fontshape`'s text, a local
            // definition inside `\maybe@load@fontshape`'s group, so the effect is the same.
            let mut alt = Vec::new();
            if name == "try@load@fontshape" && !text.is_empty() {
                if let (Some(def), Some(space)) = (self.e.cs.get("def"), self.e.cs.get("space")) {
                    alt.extend([Token::END_MATCH, Token::cs(def), Token::cs(space), Token::char(Cat::BeginGroup, '{'), Token::char(Cat::Space, ' '), Token::char(Cat::EndGroup, '}')]);
                    alt.extend_from_slice(&text[1..]);
                }
            }
            k.watch.push(Watch { id, m, text, alt });
        }
        k.ready = !k.ids.contains(&u32::MAX) && k.watch.iter().all(|w| matches!(w.m, Meaning::Macro(_)));
        self.bind_switches(&mut k);
        for want in native.as_deref().unwrap_or("").split(',').map(str::trim).filter(|s| !s.is_empty()) {
            for (i, (name, _)) in KERNEL.iter().enumerate() {
                if want != "all" && want != name.trim_end() {
                    continue;
                }
                let Some(id) = self.e.cs.get(name) else { continue };
                if i >= FIRST_SHAPE && !k.on_def.iter().any(|d| d.0 == id) {
                    k.on_def.push((id, i as u16));
                }
                if let Meaning::Macro(mac) = self.e.eqtb.meaning(id) {
                    self.bind_one(&mut k, i, id, mac, true);
                }
            }
        }
        self.kernel = Some(Box::new(k));
    }

    /// The switches' names, watches and macros (`Switch`), each switch's macro checked
    /// against the format's form; one that differs is never taken natively.
    fn bind_switches(&self, k: &mut Kernel) {
        for (i, name) in SWITCH_NAMES.iter().enumerate() {
            k.sids[i] = self.e.cs.get(name).unwrap_or(u32::MAX);
        }
        k.switch_watch = SWITCH_WATCHED.iter().map(|n| Watch::of(self, n)).collect();
        for (i, name) in DEFAULTS_HOOK.iter().enumerate() {
            k.hook[i] = self.e.cs.get(name).unwrap_or(u32::MAX);
        }
        k.switch_ready = !k.sids.contains(&u32::MAX) && k.switch_watch.iter().all(|w| matches!(w.m, Meaning::Macro(_)));
        k.switches = (FIRST_SHAPE..KERNEL.len()).map(|_| Switch::default()).collect();
    }

    /// Binds native command `i` to name `id`, whose macro is `mac`, if the macro has the form
    /// the native code reads: the macro is kept under `rt@orig@<name>` and the name means
    /// [`Meaning::Kernel`], both with the assignment's scope (`global`).
    fn bind_one(&mut self, k: &mut Kernel, i: usize, id: u32, mac: Macro, global: bool) -> bool {
        let name = KERNEL[i].0;
        if i == SELECTFONT {
            // The text after `\pickup@font\font@name`, the part left to the macros.
            let (pickup, font_name) = (self.e.cs.get("pickup@font"), self.e.cs.get("font@name"));
            let (Some(pickup), Some(font_name)) = (pickup, font_name) else { return false };
            let text = self.e.toks.get(mac.text);
            let Some(at) = text.windows(2).position(|w| w == [Token::cs(pickup), Token::cs(font_name)]) else { return false };
            k.select_tail = text[at + 2..].to_vec();
        } else if i >= FIRST_SHAPE {
            // A shape switch must stay unexpandable (`\protected`, as `\reinstall@nfss@defs`
            // makes it); a series switch is the body of a robust command.
            if !name.ends_with(' ') && mac.flags & super::engine_prim::PROTECTED == 0 {
                return false;
            }
            let s = self.parse_switch(k, name, self.e.toks.get(mac.text));
            if s.tail.is_empty() {
                return false;
            }
            k.switches[i - FIRST_SHAPE] = s;
        }
        let orig = self.e.cs.intern(&format!("rt@orig@{}", name.trim_end()));
        self.e.eqtb.set_meaning(orig, Meaning::Macro(mac), global);
        self.e.eqtb.set_meaning(id, Meaning::Kernel(i as u16), global);
        match k.bound.iter_mut().find(|b| b.0 == i as u16) {
            Some(b) => b.1 = orig,
            None => k.bound.push((i as u16, orig)),
        }
        true
    }

    /// A macro just defined (`\def` and kin): a switch's name defined again in the format's
    /// form is bound again. The kernel does this for the shape switches at
    /// `\begin{document}` (`\reinstall@nfss@defs`), so they are bound from then on.
    pub(super) fn kernel_defined(&mut self, id: u32, global: bool) {
        let Some(mut k) = self.kernel.take() else { return };
        if let Some(&(_, i)) = k.on_def.iter().find(|d| d.0 == id) {
            if let Meaning::Macro(mac) = self.e.eqtb.meaning(id) {
                self.bind_one(&mut k, i as usize, id, mac, global);
            }
        }
        self.kernel = Some(k);
    }

    /// Switch `name`'s macro text read as [`Switch`], or a `Switch` with no tail when the
    /// text is not of the format's form.
    fn parse_switch(&self, k: &Kernel, name: &str, text: &[Token]) -> Switch {
        let mut s = Switch::default();
        let cs = |n: &str| self.e.cs.get(n).map_or(Token(0), Token::cs);
        let sw = |n: &str| Token::cs(k.sids[SWITCH_NAMES.iter().position(|x| *x == n).unwrap_or(0)]);
            if let Some(p) = name.strip_suffix("series ") {
                // `\not@math@alphabet\bfseries\mathbf\expand@font@defaults
                // \maybe@update@bfseries@defaults\ifx\f@family\rmdef@ult\fontseries\bfseries@rm
                // \else…\else\fontseries\bfdefault\fi\fi\fi`, then the tail.
                let pick = [format!("{p}series@rm"), format!("{p}series@sf"), format!("{p}series@tt"), format!("{p}default")];
                let mut want = vec![sw("expand@font@defaults"), cs(&format!("maybe@update@{p}series@defaults"))];
                for (fam, pk) in ["rmdef@ult", "sfdef@ult", "ttdef@ult"].iter().zip(&pick) {
                    want.extend([sw("ifx"), sw("f@family"), sw(fam), sw("fontseries"), cs(pk), sw("else")]);
                }
                want.extend([sw("fontseries"), cs(&pick[3]), sw("fi"), sw("fi"), sw("fi")]);
                if text.len() > 4 + want.len() && text[1] == sw("not@math@alphabet") && text[4..4 + want.len()] == want[..] && !want.contains(&Token(0)) {
                    for (n, pk) in pick.iter().enumerate() {
                        s.pick[n] = cs(pk).cs_id().unwrap_or(u32::MAX);
                    }
                    s.default = s.pick[3];
                    s.previous = cs(&format!("{p}default@previous")).cs_id().unwrap_or(u32::MAX);
                    s.tail = text[4 + want.len()..].to_vec();
                }
            } else if text.len() == 7 && text[1] == sw("not@math@alphabet") && text[4] == sw("fontshape") {
                // `\not@math@alphabet\itshape\mathit\fontshape\itdefault\selectfont`.
                if let Some(d) = text[5].cs_id() {
                    s.pick[0] = d;
                    s.tail = text[6..].to_vec();
                }
            }
        s
    }

    /// The name native command `i`'s macro is kept under, while it is bound.
    pub(super) fn kernel_orig(&self, i: u16) -> Option<u32> {
        self.kernel.as_ref()?.bound.iter().find(|b| b.0 == i).map(|b| b.1)
    }

    /// Runs native command `i`, named by `t`: natively, or the macro it replaced.
    pub(super) fn kernel_command(&mut self, i: u16, t: Token) {
        let (Some(&(_, f)), Some(mut k)) = (KERNEL.get(i as usize), self.kernel.take()) else { return };
        k.cur = i;
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
    /// meaning word, or (a macro the collector moved, or patched as [`Watch::alt`] says) the
    /// same flags and text, whose new meaning word is then the one compared.
    fn unchanged(&self, w: &mut Watch) -> bool {
        let m = self.e.eqtb.meaning(w.id);
        if m == w.m {
            return true;
        }
        match (m, w.m) {
            (Meaning::Macro(a), Meaning::Macro(b)) if a.flags == b.flags && (self.e.toks.get(a.text) == w.text.as_slice() || self.e.toks.get(a.text) == w.alt.as_slice()) => {
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

    /// The characters `list` expands to, appended to `out`: characters as they are, and
    /// parameterless macros whose texts expand to characters (`\bfdefault`'s `bx\@empty`),
    /// as `\edef` and `\csname` would; `false` for anything else.
    fn expand_chars(&self, list: &[Token], out: &mut Vec<Token>, depth: u8) -> bool {
        for &t in list {
            match t.cs_id() {
                Some(id) => {
                    let Meaning::Macro(m) = self.e.eqtb.meaning(id) else { return false };
                    let text = self.e.toks.get(m.text);
                    let Some((&Token::END_MATCH, body)) = text.split_first() else { return false };
                    if depth == 0 || !self.expand_chars(body, out, depth - 1) {
                        return false;
                    }
                }
                None if matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space)) => out.push(t),
                None => return false,
            }
        }
        true
    }

    /// The change rule `\<kind>@<current>@<requested>` (`\DeclareFontSeriesChangeRule`,
    /// `\DeclareFontShapeChangeRule`) applied without substitution, as
    /// `\merge@font@series@without@substitution` does: a rule `{c}{d}` gives `c`, no rule
    /// gives the request (in `k.arg`), and leaves its name `\relax` (§372). The result is
    /// in `k.arg`.
    fn change_rule(&mut self, k: &mut Kernel, kind: &str, current: usize) -> bool {
        if k.arg.is_empty() {
            return false;
        }
        k.name.clear();
        k.name.push_str(kind);
        let cur = if current == F_SERIES { &k.series } else { &k.shape };
        k.name.extend(cur.iter().map(|t| t.chr()));
        k.name.push('@');
        k.name.extend(k.arg.iter().map(|t| t.chr()));
        let id = self.e.cs.intern(&k.name);
        match self.meaning(Token::cs(id)) {
            Meaning::Undefined => {
                if !k.relax.contains(&id) {
                    k.relax.push(id);
                }
            }
            Meaning::Prim(super::engine_prim::Prim::Relax) => {}
            Meaning::Macro(m) => {
                // `{c}{d}`: `c` is the target.
                let text = self.e.toks.get(m.text);
                let Some((&Token::END_MATCH, body)) = text.split_first() else { return false };
                let Some((&open, rest)) = body.split_first() else { return false };
                let Some(close) = rest.iter().position(|t| t.cat() == Some(Cat::EndGroup) && t.cs_id().is_none()) else { return false };
                if open.cat() != Some(Cat::BeginGroup) || open.cs_id().is_some() || rest[..close].iter().any(|t| t.cat() == Some(Cat::BeginGroup) && t.cs_id().is_none()) {
                    return false;
                }
                k.list.clear();
                if !self.expand_chars(&rest[..close], &mut k.list, 8) {
                    return false;
                }
                std::mem::swap(&mut k.arg, &mut k.list);
            }
            _ => return false,
        }
        true
    }

    /// `\delayed@f@adjustment`'s steps applied to `k.series` and `k.shape`: the
    /// `\fontseries`/`\fontshape` merges (`\delayed@merge@font@series{x}`) and the
    /// `\fontseriesforce`/`\fontshapeforce` settings (`\edef\f@series{x}`), with each
    /// argument characters or macros that expand to them. `false` for any other text.
    fn delayed_adjustment(&mut self, k: &mut Kernel, text: &[Token]) -> bool {
        let mut at = 0;
        while at < text.len() {
            let (head, target) = match text[at].cs_id() {
                Some(id) if id == k.ids[EDEF] => match text.get(at + 1).and_then(|t| t.cs_id()) {
                    Some(t) if t == k.ids[F_SERIES] || t == k.ids[F_SHAPE] => (EDEF, t),
                    _ => return false,
                },
                Some(id) if id == k.ids[DELAYED_MERGE_SERIES] => (DELAYED_MERGE_SERIES, k.ids[F_SERIES]),
                Some(id) if id == k.ids[DELAYED_MERGE_SHAPE] => (DELAYED_MERGE_SHAPE, k.ids[F_SHAPE]),
                _ => return false,
            };
            at += if head == EDEF { 2 } else { 1 };
            // The braced argument, with no group inside.
            if text.get(at).is_none_or(|t| t.cat() != Some(Cat::BeginGroup) || t.cs_id().is_some()) {
                return false;
            }
            let Some(len) = text[at + 1..].iter().position(|t| t.cs_id().is_none() && matches!(t.cat(), Some(Cat::BeginGroup | Cat::EndGroup))) else { return false };
            let arg = &text[at + 1..at + 1 + len];
            if text[at + 1 + len].cat() != Some(Cat::EndGroup) {
                return false;
            }
            at += len + 2;
            k.arg.clear();
            if !self.expand_chars(arg, &mut k.arg, 8) {
                return false;
            }
            let series = target == k.ids[F_SERIES];
            if head != EDEF && !self.change_rule(k, if series { "series@" } else { "shape@" }, if series { F_SERIES } else { F_SHAPE }) {
                return false;
            }
            if series {
                // `\set@target@series` after a merge; `\fontseriesforce` takes its value as given.
                k.series.clear();
                if head == EDEF {
                    k.series.extend_from_slice(&k.arg);
                } else {
                    drop_one_m(&k.arg, &mut k.series);
                }
            } else {
                std::mem::swap(&mut k.shape, &mut k.arg);
            }
        }
        true
    }

    /// `\selectfont` (ltfssbas.dtx) where nothing but the font and its series and shape
    /// change: the line spread is the one in force (`\f@linespread` is `\baselinestretch`),
    /// a pending `\delayed@f@adjustment` holds only the merges `\fontseries` and
    /// `\fontshape` store, applied without substitution because the shape they give is
    /// declared, its family's `.fd` is already read (`\maybe@load@fontshape` does nothing),
    /// and the font of the shape and size is already loaded, so `\pickup@font` does
    /// nothing. Then, as the macro: the saved series and shape, the merged ones,
    /// `\delayed@f@adjustment` emptied, `\@forced@seriesfalse`, `\font@name` defined
    /// globally as that font's name, the font selected, and the rest of the macro
    /// (`\UseHook{selectfont}\size@update\enc@update`) read next.
    fn select_font_native(&mut self, k: &mut Kernel) -> bool {
        if !self.plain_assignments() || !k.watch.iter_mut().all(|w| self.unchanged(w)) {
            return false;
        }
        let m = |i: usize| self.meaning(Token::cs(k.ids[i]));
        if !self.same_meaning(m(F_LINESPREAD), m(BASELINESTRETCH)) {
            return false;
        }
        let pending = !self.same_meaning(m(DELAYED_F_ADJUSTMENT), m(EMPTY));
        if !pending {
            if !self.shape_key(k) {
                return false;
            }
        } else {
            if !self.same_meaning(m(DELAYED_MERGE_SERIES), m(MERGE_SERIES)) || !self.same_meaning(m(DELAYED_MERGE_SHAPE), m(MERGE_SHAPE)) {
                return false;
            }
            let Meaning::Macro(delayed) = m(DELAYED_F_ADJUSTMENT) else { return false };
            k.series.clear();
            k.shape.clear();
            k.relax.clear();
            if !self.expand_chars(&[Token::cs(k.ids[F_SERIES])], &mut k.series, 8) || !self.expand_chars(&[Token::cs(k.ids[F_SHAPE])], &mut k.shape, 8) {
                return false;
            }
            let text = self.e.toks.get(delayed.text).to_vec();
            let Some((&Token::END_MATCH, steps)) = text.split_first() else { return false };
            if !self.delayed_adjustment(k, steps) || !self.unchanged(&mut k.watch[W_CURR_FONTSHAPE]) {
                return false;
            }
            // `\maybe@load@fontshape`: the family's `.fd` is read when `\<enc>+<family>` is
            // a macro (`\@empty`).
            k.name.clear();
            if !self.macro_chars(k.ids[F_ENCODING], &mut k.name) {
                return false;
            }
            let plus = k.name.len();
            k.name.push('+');
            if !self.macro_chars(k.ids[F_FAMILY], &mut k.name) {
                return false;
            }
            if !self.e.cs.get(&k.name).is_some_and(|id| matches!(self.e.eqtb.meaning(id), Meaning::Macro(_))) {
                return false;
            }
            k.name.replace_range(plus..plus + 1, "/");
            k.name.push('/');
            k.name.extend(k.series.iter().map(|t| t.chr()));
            k.name.push('/');
            k.name.extend(k.shape.iter().map(|t| t.chr()));
            // `\ifcsname\f@encoding/\f@family/\f@series/\f@shape\endcsname`.
            if self.e.cs.get(&k.name).is_none_or(|id| self.meaning(Token::cs(id)) == Meaning::Undefined) {
                return false;
            }
            k.size.clear();
            if !self.macro_chars(k.ids[F_SIZE], &mut k.size) {
                return false;
            }
        }
        // `\csname\curr@fontshape/\f@size\endcsname`, a font already defined.
        k.name.push('/');
        k.name.push_str(&k.size);
        let Some(font_cs) = self.e.cs.get(&k.name) else { return false };
        let Meaning::Font(f) = self.e.eqtb.meaning(font_cs) else { return false };
        if pending {
            // `\let\f@shape@saved\f@shape`, `\let\f@series@saved\f@series`, the rules'
            // `\relax`, the merged values, `\let\delayed@f@adjustment\@empty`.
            for (saved, from) in [(F_SHAPE_SAVED, F_SHAPE), (F_SERIES_SAVED, F_SERIES)] {
                let v = self.e.eqtb.meaning(k.ids[from]);
                self.e.eqtb.set_meaning(k.ids[saved], v, false);
            }
            for &id in &k.relax {
                self.e.eqtb.set_meaning(id, Meaning::Prim(super::engine_prim::Prim::Relax), false);
            }
            for (i, v) in [(F_SERIES, &k.series), (F_SHAPE, &k.shape)] {
                k.list.clear();
                k.list.push(Token::END_MATCH);
                k.list.extend_from_slice(v);
                let text = self.e.toks.store(&k.list);
                self.e.eqtb.set_meaning(k.ids[i], Meaning::Macro(Macro { text, flags: 0 }), false);
            }
            let empty = self.e.eqtb.meaning(k.ids[EMPTY]);
            self.e.eqtb.set_meaning(k.ids[DELAYED_F_ADJUSTMENT], empty, false);
        }
        // `\@forced@seriesfalse`: `\let\if@forced@series\iffalse`.
        let iffalse = self.meaning(Token::cs(k.ids[IFFALSE]));
        self.e.eqtb.set_meaning(k.ids[IF_FORCED_SERIES], iffalse, false);
        let text = self.e.toks.store(&[Token::END_MATCH, Token::cs(font_cs)]);
        self.e.eqtb.set_meaning(k.ids[FONT_NAME], Meaning::Macro(Macro { text, flags: 0 }), true);
        self.select_font(f, false);
        self.back_list(&k.select_tail);
        true
    }

    /// Whether a switch can run natively: not in math mode (`\not@math@alphabet` stops
    /// there), its macro of the format's form, assignments plain, the internals it acts for
    /// unchanged, and `\delayed@f@adjustment` a macro without parameters.
    fn switch_ok(&mut self, k: &mut Kernel) -> bool {
        let s = &k.switches[k.cur as usize - FIRST_SHAPE];
        if !k.switch_ready || s.tail.is_empty() || self.mode == super::engine_run::Mode::Math || !self.plain_assignments() {
            return false;
        }
        if !k.switch_watch.iter_mut().all(|w| self.unchanged(w)) {
            return false;
        }
        matches!(self.e.eqtb.meaning(k.ids[DELAYED_F_ADJUSTMENT]), Meaning::Macro(m) if self.e.toks.get(m.text).first() == Some(&Token::END_MATCH))
    }

    /// Whether `\UseHook{expand@font@defaults}` does nothing: after `\begin{document}`
    /// (`\hook_use:n` is `\__hook_use_initialized:n`, which only expands the hook's code;
    /// in the preamble it first initialises the hook), with the code undefined or empty or
    /// only the one-time and next-use parts, both empty.
    fn hook_empty(&self, k: &Kernel) -> bool {
        let m = |id: u32| if id == u32::MAX { Meaning::Undefined } else { self.meaning(Token::cs(id)) };
        if !self.same_meaning(m(k.sids[S_HOOK_USE]), m(k.sids[S_HOOK_USE_INITIALIZED])) {
            return false;
        }
        let body = |id: u32| match m(id) {
            Meaning::Macro(mac) => self.e.toks.get(mac.text).split_first().filter(|(&e, _)| e == Token::END_MATCH).map(|(_, b)| b),
            _ => None,
        };
        match m(k.hook[0]) {
            Meaning::Undefined => true,
            _ => match body(k.hook[0]) {
                Some([]) => true,
                Some([a, b]) => *a == Token::cs(k.hook[1]) && *b == Token::cs(k.hook[2]) && body(k.hook[1]) == Some(&[]) && body(k.hook[2]) == Some(&[]),
                _ => false,
            },
        }
    }

    /// `\fontshape{x}` or `\fontseries{x}` (ltfssbas.dtx): `\delayed@merge@font@…{x}`
    /// appended to `\delayed@f@adjustment` (`\expandafter\def`), with `x` the token given.
    fn delay_merge(&mut self, k: &mut Kernel, merge: usize, x: u32) {
        let Meaning::Macro(m) = self.e.eqtb.meaning(k.ids[DELAYED_F_ADJUSTMENT]) else { return };
        k.list.clear();
        k.list.extend_from_slice(self.e.toks.get(m.text));
        k.list.extend([Token::cs(k.ids[merge]), Token::char(Cat::BeginGroup, '{'), Token::cs(x), Token::char(Cat::EndGroup, '}')]);
        let text = self.e.toks.store(&k.list);
        self.e.eqtb.set_meaning(k.ids[DELAYED_F_ADJUSTMENT], Meaning::Macro(Macro { text, flags: 0 }), false);
    }

    /// `\itshape`, `\upshape`, `\slshape`, `\scshape` (ltfssini.dtx): `\fontshape` with the
    /// switch's default, then `\selectfont` read next.
    fn shape_switch_native(&mut self, k: &mut Kernel) -> bool {
        if !self.switch_ok(k) {
            return false;
        }
        let d = k.switches[k.cur as usize - FIRST_SHAPE].pick[0];
        self.delay_merge(k, DELAYED_MERGE_SHAPE, d);
        self.back_list(&k.switches[k.cur as usize - FIRST_SHAPE].tail);
        true
    }

    /// `\bfseries`, `\mdseries` (ltfssini.dtx) while `\…default` is `\…default@previous`
    /// (`\maybe@update@…series@defaults` does nothing): `\expand@font@defaults`' five values,
    /// the series the family's meta family asks for (`\bfseries@rm` while `\f@family` is
    /// `\rmdefault`, …, else `\bfdefault`) given to `\fontseries`, then the tail
    /// (`\UseHook{bfseries}\selectfont`) read next.
    fn series_switch_native(&mut self, k: &mut Kernel) -> bool {
        if !self.switch_ok(k) {
            return false;
        }
        let at = k.cur as usize - FIRST_SHAPE;
        let (default, previous) = (k.switches[at].default, k.switches[at].previous);
        if previous == u32::MAX || !self.hook_empty(k) || !self.same_meaning(self.e.eqtb.meaning(default), self.e.eqtb.meaning(previous)) {
            return false;
        }
        // `\edef\rmdef@ult{\rmdefault}` and kin, `\series@maybe@drop@one@m\bfdefault\bfdef@ult`
        // and `\mddefault\mddef@ult`.
        let mut defaults = std::mem::take(&mut k.defaults);
        let mut ok = true;
        for (n, v) in defaults.iter_mut().enumerate() {
            v.clear();
            ok = ok && self.expand_chars(&[Token::cs(k.sids[S_RMDEFAULT + n])], v, 8);
        }
        if ok {
            for (n, v) in defaults.iter().enumerate() {
                let into = if n < 3 { S_RMDEF + n } else { S_BFDEF + n - 3 };
                k.list.clear();
                k.list.push(Token::END_MATCH);
                if n < 3 {
                    k.list.extend_from_slice(v);
                } else {
                    drop_one_m(v, &mut k.list);
                }
                // An equal value is left in place: assigning it again changes nothing.
                let same = matches!(self.e.eqtb.meaning(k.sids[into]), Meaning::Macro(m) if m.flags == 0 && self.e.toks.get(m.text) == k.list.as_slice());
                if !same {
                    let text = self.e.toks.store(&k.list);
                    self.e.eqtb.set_meaning(k.sids[into], Meaning::Macro(Macro { text, flags: 0 }), false);
                }
            }
        }
        k.defaults = defaults;
        if !ok {
            return false;
        }
        // `\ifx\f@family\rmdef@ult`, `\sfdef@ult`, `\ttdef@ult`.
        let family = self.e.eqtb.meaning(k.sids[S_F_FAMILY]);
        let pick = (0..3).find(|&n| self.same_meaning(family, self.e.eqtb.meaning(k.sids[S_RMDEF + n]))).unwrap_or(3);
        let x = k.switches[at].pick[pick];
        // `\fontseries`: `\@forced@seriesfalse`, then the merge delayed.
        let iffalse = self.meaning(Token::cs(k.ids[IFFALSE]));
        self.e.eqtb.set_meaning(k.ids[IF_FORCED_SERIES], iffalse, false);
        self.delay_merge(k, DELAYED_MERGE_SERIES, x);
        self.back_list(&k.switches[at].tail);
        true
    }

    /// `RT_STATE`: one dump of the state, headed `label` (module documentation).
    pub(super) fn kernel_state(&mut self, label: &str) {
        if self.kernel.as_ref().is_some_and(|k| k.state_file.is_some()) {
            // A bound name shows its macro (`print_meaning`), so it is compared as well.
            let skip: Vec<&str> = SCRATCH.to_vec();
            // Scratch registers (`\series@check@toks`, `\dimen@`) are skipped by slot.
            let regs: Vec<Reg> = SCRATCH
                .iter()
                .filter_map(|n| match self.e.cs.get(n).map(|id| self.e.eqtb.meaning(id)) {
                    Some(Meaning::Reg(r)) => Some(r),
                    _ => None,
                })
                .collect();
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
                    if regs.contains(&reg) {
                        continue;
                    }
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
            let Some(k) = self.kernel.as_mut() else { return };
            k.state.push_str(&format!("== {label}\n"));
            for l in lines {
                k.state.push_str(&l);
                k.state.push('\n');
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_one_m_follows_the_macro() {
        let toks = |s: &str| s.chars().map(|c| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c)).collect::<Vec<_>>();
        // `\series@drop@one@m #1m#2m#3`, applied to `<v>m`.
        for (v, want) in [("ulm", "ul"), ("lm", "l"), ("mm", ""), ("bm", "b"), ("mc", "c"), ("msx", "sx"), ("mux", "ux"), ("bx", "bx"), ("m", "m"), ("b", "b"), ("sbc", "sbc")] {
            let mut out = Vec::new();
            drop_one_m(&toks(v), &mut out);
            assert_eq!(out, toks(want), "{v}");
        }
        // A list entry whose `m` is not a letter is not in the list.
        let mut out = Vec::new();
        let v = [Token::char(Cat::Letter, 'b'), Token::char(Cat::Other, 'm')];
        drop_one_m(&v, &mut out);
        assert_eq!(out, v);
    }
}
