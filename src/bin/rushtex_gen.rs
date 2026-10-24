//! `rushtex-gen`: the build-time table generator (pillar 2). Dev only (`--features gen`).
//!
//!     rushtex-gen fonts --list FILE --use USE [--out assets/tables/fonts.rtdb]   # write, self-check
//!     rushtex-gen fonts --list FILE --use USE --check                           # self-check only
//!     rushtex-gen nfss --list FILE [--format FMT] [--out assets/tables/nfss.rtdb] [--dump]
//!     rushtex-gen pkgs --list FILE [--format FMT] [--out assets/tables/pkgs.rtdb]
//!     rushtex-gen embed --list FILE [--set assets/tables/embed.set] [--out assets/tables/embed.rtdb]
//!     rushtex-gen class [--format FMT] [--out assets/tables/class-article.rtdb]
//!
//! `FILE` lists TFM paths, one per line, or `scripts/texlive/files.tsv` rows (path first).
//! `USE` (`scripts/texlive/font-use.tsv`) gives each font's popularity: `name<TAB>documents`.
//! IDs follow popularity, then name; the same inputs always give the same bytes. Tier 1 is
//! the fonts at least `TIER1_DOCS` documents load, tier 2 the other used fonts, tier 3 the rest.

use rushtex::data::data_blob::{Blob, BlobWriter};
#[cfg(feature = "oracle")]
use rushtex::data::data_nfss;
use rushtex::data::data_tfm::{self, FontTable, GenFont};
use std::process::ExitCode;

/// Documents that must load a font for it to be in tier 1.
const TIER1_DOCS: u32 = 2;

/// A file of the list: at its recorded path, else by name through kpathsea (the pinned
/// snapshot unpacked elsewhere, as in CI under `RUSHTEX_KPSE_PATHS`).
fn read_listed(path: &str) -> Result<Vec<u8>, String> {
    #[cfg(feature = "oracle")]
    thread_local! {
        // The search index, built once on the first file not at its recorded path.
        static KPSE: std::cell::OnceCell<Option<rushtex::engine::engine_host::Kpse>> = const { std::cell::OnceCell::new() };
    }
    std::fs::read(path).or_else(|e| {
        #[cfg(feature = "oracle")]
        if let Some(b) = KPSE.with(|k| k.get_or_init(|| rushtex::engine::engine_host::Kpse::new("pdflatex", None, None)).as_ref().and_then(|k| k.read(path.rsplit('/').next().unwrap_or(path)))) {
            return Ok(b);
        }
        Err(format!("{path}: {e}"))
    })
}

fn read_fonts(list: &str, uses: &str) -> Result<Vec<GenFont>, String> {
    let text = std::fs::read_to_string(list).map_err(|e| format!("{list}: {e}"))?;
    let use_text = std::fs::read_to_string(uses).map_err(|e| format!("{uses}: {e}"))?;
    let docs: std::collections::HashMap<&str, u32> = use_text.lines().filter_map(|l| l.split_once('\t')).filter_map(|(n, c)| Some((n, c.trim().parse().ok()?))).collect();
    let mut fonts = Vec::new();
    for path in text.lines().filter_map(|l| l.split('\t').next()).filter(|p| p.ends_with(".tfm")) {
        let bytes = read_listed(path)?;
        let parts = data_tfm::parse(&bytes).ok_or_else(|| format!("{path}: not a well-formed TFM"))?;
        let name = std::path::Path::new(path).file_stem().and_then(|s| s.to_str()).ok_or_else(|| format!("{path}: no name"))?;
        let n = docs.get(name).copied().unwrap_or(0);
        let tier = if n >= TIER1_DOCS { 1 } else if n > 0 { 2 } else { 3 };
        fonts.push((n, GenFont { name: name.to_string(), parts, tier }));
    }
    fonts.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    let mut names: Vec<&str> = fonts.iter().map(|f| f.1.name.as_str()).collect();
    names.sort_unstable();
    if let Some(d) = names.windows(2).find(|w| w[0] == w[1]) {
        return Err(format!("{}.tfm is listed twice", d[0]));
    }
    Ok(fonts.into_iter().map(|f| f.1).collect())
}

fn check(out: &str, fonts: &[GenFont]) -> Result<(), String> {
    let bytes = std::fs::read(out).map_err(|e| format!("{out}: {e}"))?;
    let blob = Blob::new(&bytes).ok_or("not a table blob")?;
    let table = FontTable::from_blob(&blob).ok_or("not a font table")?;
    data_tfm::check(&table, fonts)
}

fn fonts(args: &[String]) -> Result<String, String> {
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let list = arg("--list").ok_or("--list FILE is required")?;
    let uses = arg("--use").ok_or("--use FILE is required")?;
    let out = arg("--out").unwrap_or_else(|| "assets/tables/fonts.rtdb".into());
    let fonts = read_fonts(&list, &uses)?;
    let mut saved = 0;
    if !args.iter().any(|a| a == "--check") {
        let mut w = BlobWriter::default();
        saved = data_tfm::write(&mut w, &fonts)?;
        std::fs::write(&out, w.finish()).map_err(|e| format!("{out}: {e}"))?;
    }
    check(&out, &fonts)?;
    let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    let tiers: Vec<usize> = (1..=3).map(|t| fonts.iter().filter(|f| f.tier == t).count()).collect();
    Ok(format!("{out}: {} fonts (tiers {tiers:?}), {size} bytes ({saved} saved by shared arrays), every word checked against its TFM", fonts.len()))
}

/// The LaTeX format and the file reader the engine runs on: `--format`, else the command's
/// cache (`~/.cache/rushtex/pdflatex.rfmt`); files as kpathsea finds them (or the pin's
/// search paths under `RUSHTEX_KPSE_PATHS`).
/// Every control sequence's meaning at the end of a run, by name.
#[cfg(feature = "oracle")]
type Meanings = std::collections::HashMap<String, rushtex::engine::engine_prim::Meaning>;

#[cfg(feature = "oracle")]
struct Engine {
    fmt: Vec<u8>,
    kpse: rushtex::engine::engine_host::Kpse,
}

#[cfg(feature = "oracle")]
impl Engine {
    fn new(format: Option<String>) -> Result<Engine, String> {
        let cache = std::env::var_os("RUSHTEX_CACHE").map(std::path::PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".cache/rushtex")));
        let fmt_path = format.map(std::path::PathBuf::from).or_else(|| cache.as_ref().map(|c| c.join("pdflatex.rfmt"))).ok_or("no --format and no cache directory")?;
        let fmt = std::fs::read(&fmt_path).map_err(|e| format!("{}: {e} (run rushtex once to make the format)", fmt_path.display()))?;
        let kpse = rushtex::engine::engine_host::Kpse::new("pdflatex", cache.as_ref().map(|c| c.join("kpse-paths")).as_deref(), None).ok_or("no TeX search paths")?;
        Ok(Engine { fmt, kpse })
    }

    /// Runs `src` once on the format: the text it wrote to `file`, the run's errors, and the
    /// meaning of every control sequence at the end (by name).
    fn run_meanings(&self, src: &str, file: &str) -> Result<(String, Vec<String>, Meanings), String> {
        use rushtex::engine::engine_job;
        let read = |name: &str| self.kpse.read(name);
        let (mut text, mut meanings) = (None, std::collections::HashMap::new());
        let job = engine_job::run(&|| Some(self.fmt.clone()), src, &engine_job::Opts { job_name: "rtgen", now: 0, max_runs: 1 }, &read, &mut |_| {}, &mut |m| {
            text = m.closed.iter().rev().find(|w| w.name.trim_end_matches(".tex") == file).map(|w| w.text.clone());
            meanings = (0..m.e.cs.len() as u32).map(|id| (m.e.cs.name(id).to_string(), m.e.eqtb.meaning(id))).collect();
        })
        .ok_or("the format is not one of this build")?;
        Ok((text.unwrap_or_default(), job.errors, meanings))
    }

    /// Runs `src` once on the format; the text it wrote to `file`, and the run's errors.
    fn run(&self, src: &str, file: &str) -> Result<(String, Vec<String>), String> {
        use rushtex::engine::engine_job;
        let read = |name: &str| self.kpse.read(name);
        let (mut text, mut names) = (None, Vec::new());
        let job = engine_job::run(&|| Some(self.fmt.clone()), src, &engine_job::Opts { job_name: "rtgen", now: 0, max_runs: 1 }, &read, &mut |_| {}, &mut |m| {
            names = m.closed.iter().map(|w| w.name.clone()).collect();
            text = m.closed.iter().rev().find(|w| w.name.trim_end_matches(".tex") == file).map(|w| w.text.clone());
        })
        .ok_or("the format is not one of this build")?;
        Ok((text.ok_or_else(|| format!("the run wrote no {file} (wrote {names:?}; errors {:?})", job.errors))?, job.errors))
    }
}

/// `rushtex-gen nfss`: every pinned `.fd` file read by LaTeX itself (its macro code runs), the
/// declarations recorded as `\DeclareFontFamily` and `\DeclareFontShape` make them.
#[cfg(feature = "oracle")]
fn nfss(args: &[String]) -> Result<String, String> {
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let list = arg("--list").ok_or("--list FILE is required")?;
    let text = std::fs::read_to_string(&list).map_err(|e| format!("{list}: {e}"))?;
    let mut fds: Vec<&str> = text.lines().filter_map(|l| l.split('\t').next()).filter(|p| p.ends_with(".fd")).filter_map(|p| p.rsplit('/').next()).collect();
    fds.sort_unstable();
    fds.dedup();
    let engine = Engine::new(arg("--format"))?;
    let (decls, errors) = engine.run(&data_nfss::declare_source(&fds), data_nfss::DECLARE_FILE)?;
    if !errors.is_empty() {
        return Err(format!("reading the .fd files: {}", errors.join("; ")));
    }
    if args.iter().any(|a| a == "--dump") {
        print!("{decls}");
    }
    let (fams, shapes) = data_nfss::parse_declarations(&decls)?;
    let out = arg("--out").unwrap_or_else(|| "assets/tables/nfss.rtdb".into());
    let mut w = BlobWriter::default();
    let strings = data_nfss::write(&mut w, &fams, &shapes)?;
    std::fs::write(&out, w.finish()).map_err(|e| format!("{out}: {e}"))?;
    // The self-check: LaTeX's own `\get@external@font` against the table, every shape at
    // every size `check_sizes` gives.
    let cases: Vec<(String, String)> = shapes.iter().flat_map(|s| data_nfss::check_sizes(s).into_iter().map(|z| (s.name.clone(), z))).collect();
    let (want, check_errors) = engine.run(&data_nfss::check_source(&fds, &cases), data_nfss::CHECK_FILE)?;
    if let Ok(dir) = std::env::var("RT_GEN_DEBUG") {
        let _ = std::fs::write(format!("{dir}/nfss-check.tex"), data_nfss::check_source(&fds, &cases));
        let _ = std::fs::write(format!("{dir}/nfss-check.out"), &want);
        let _ = std::fs::write(format!("{dir}/nfss-check.err"), check_errors.join("\n"));
    }
    let bytes = std::fs::read(&out).map_err(|e| format!("{out}: {e}"))?;
    let table = Blob::new(&bytes).and_then(|b| data_nfss::NfssTable::from_blob(&b)).ok_or("not an NFSS table")?;
    let n = data_nfss::check(&table, &fams, &shapes, &want)?;
    if n != cases.len() {
        return Err(format!("the check run answered {n} of {} cases", cases.len()));
    }
    let size = bytes.len();
    Ok(format!("{out}: {} .fd files, {} families, {} shapes, {} entries, {size} bytes ({strings} of strings); {n} (shape, size) cases equal to LaTeX's \\get@external@font", fds.len(), fams.len(), shapes.len(), shapes.iter().map(|s| s.entries.len()).sum::<usize>()))
}

/// `rushtex-gen pkgs`: every pinned `.sty` and `.cls` loaded by LaTeX itself (a package on
/// `article`, a class as the class), recording its direct loads, its declared options and
/// the control sequences it defines: those whose meaning differs from the same run without
/// it, less those of the registered files it loads, less LaTeX's own bookkeeping (names
/// that change in at least half of all runs, and per-file names such as `\ver@x.sty`),
/// and only names of letters (what a document writes without `\makeatletter`).
#[cfg(feature = "oracle")]
fn pkgs(args: &[String]) -> Result<String, String> {
    use rushtex::data::data_pkgs::{self, GenPkg};
    use std::collections::{BTreeSet, HashMap};
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let list = arg("--list").ok_or("--list FILE is required")?;
    let text = std::fs::read_to_string(&list).map_err(|e| format!("{list}: {e}"))?;
    let mut files: Vec<&str> = text.lines().filter_map(|l| l.split('\t').next()).filter(|p| p.ends_with(".sty") || p.ends_with(".cls")).filter_map(|p| p.rsplit('/').next()).collect();
    files.sort_unstable();
    files.dedup();
    let engine = Engine::new(arg("--format"))?;
    let (_, _, base_sty) = engine.run_meanings(&data_pkgs::base_source(false), data_pkgs::LOAD_FILE)?;
    let (_, _, base_cls) = engine.run_meanings(&data_pkgs::base_source(true), data_pkgs::LOAD_FILE)?;
    let per_file = |n: &str| [".sty", ".cls", ".clo", ".cfg", ".def", ".fd", ".tex", ".ldf", "/"].iter().any(|x| n.contains(x)) || n.starts_with("ds@") || n.starts_with("rt@");
    let (mut recs, mut failed) = (Vec::new(), Vec::new());
    let mut all_options: HashMap<String, Vec<String>> = HashMap::new();
    for f in &files {
        let base = if f.ends_with(".cls") { &base_cls } else { &base_sty };
        let (out, errors, after) = engine.run_meanings(&data_pkgs::load_source(f), data_pkgs::LOAD_FILE)?;
        if !errors.is_empty() {
            failed.push(format!("{f} ({} errors: {})", errors.len(), errors[0].lines().next().unwrap_or("")));
        }
        let (loads, options) = data_pkgs::parse_load(&out, f);
        for (p, o) in options {
            let v = all_options.entry(p).or_default();
            if !v.contains(&o) {
                v.push(o);
            }
        }
        let defs: BTreeSet<String> = after.iter().filter(|(n, m)| base.get(*n) != Some(m) && !per_file(n)).map(|(n, _)| n.clone()).collect();
        recs.push((f.to_string(), loads, defs));
    }
    // LaTeX's bookkeeping: what most loads change.
    let mut count: HashMap<&str, usize> = HashMap::new();
    for (_, _, d) in &recs {
        for n in d {
            *count.entry(n.as_str()).or_default() += 1;
        }
    }
    let noise: BTreeSet<String> = count.iter().filter(|(_, &c)| 2 * c >= recs.len()).map(|(n, _)| n.to_string()).collect();
    let trans: HashMap<&str, &BTreeSet<String>> = recs.iter().map(|(n, _, d)| (n.as_str(), d)).collect();
    let loads_of: HashMap<&str, &Vec<String>> = recs.iter().map(|(n, l, _)| (n.as_str(), l)).collect();
    let mut pkgs = Vec::with_capacity(recs.len());
    for (name, loads, defs) in &recs {
        // The registered files below this one, directly or not.
        let (mut below, mut stack): (BTreeSet<&str>, Vec<&str>) = (BTreeSet::new(), loads.iter().map(String::as_str).collect());
        while let Some(x) = stack.pop() {
            if below.insert(x) {
                stack.extend(loads_of.get(x).into_iter().flat_map(|l| l.iter().map(String::as_str)));
            }
        }
        let inherited: BTreeSet<&str> = below.iter().filter_map(|b| trans.get(b)).flat_map(|d| d.iter().map(String::as_str)).collect();
        // Only names a document can write without `\makeatletter`: letters only. Internal
        // names (`@`, expl3's `_` and `:`, encoding-prefixed text commands) reach a document
        // only through code that redefines internals, which takes the engine fallback.
        let user = |n: &str| !n.is_empty() && n.bytes().all(|b| b.is_ascii_alphabetic());
        let defines = defs.iter().filter(|n| user(n) && !noise.contains(*n) && !inherited.contains(n.as_str())).cloned().collect();
        pkgs.push(GenPkg { name: name.clone(), loads: loads.clone(), options: all_options.get(name).cloned().unwrap_or_default(), defines });
    }
    let out = arg("--out").unwrap_or_else(|| "assets/tables/pkgs.rtdb".into());
    let mut w = BlobWriter::default();
    let (n_files, n_cmds) = data_pkgs::write(&mut w, &pkgs)?;
    std::fs::write(&out, w.finish()).map_err(|e| format!("{out}: {e}"))?;
    let bytes = std::fs::read(&out).map_err(|e| format!("{out}: {e}"))?;
    let table = Blob::new(&bytes).and_then(|b| data_pkgs::PkgTable::from_blob(&b)).ok_or("not a package table")?;
    data_pkgs::check(&table, &pkgs)?;
    if let Ok(dir) = std::env::var("RT_GEN_DEBUG") {
        let mut s = String::new();
        for p in &pkgs {
            s.push_str(&format!("{}\tloads {}\toptions {}\tdefines {}\n", p.name, p.loads.join(","), p.options.join(","), p.defines.len()));
        }
        s.push_str(&format!("noise\t{}\nfailed\t{}\n", noise.iter().cloned().collect::<Vec<_>>().join(" "), failed.join(" | ")));
        let _ = std::fs::write(format!("{dir}/pkgs.tsv"), s);
    }
    Ok(format!("{out}: {} packages and classes, {n_files} files, {n_cmds} commands, {} bytes; {} bookkeeping names left out; {} loaded with errors", pkgs.len(), bytes.len(), noise.len(), failed.len()))
}

/// Whether basename `name` matches `pattern`, which holds at most one `*`.
fn glob(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        Some((a, b)) => name.len() >= a.len() + b.len() && name.starts_with(a) && name.ends_with(b),
        None => pattern == name,
    }
}

/// `rushtex-gen embed`: the pinned files whose names match the set's patterns, carried in
/// the binary (`data_embed`), each read back and compared before the table is accepted.
fn embed(args: &[String]) -> Result<String, String> {
    use rushtex::data::data_embed;
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let list = arg("--list").ok_or("--list FILE is required")?;
    let set_path = arg("--set").unwrap_or_else(|| "assets/tables/embed.set".into());
    let set = std::fs::read_to_string(&set_path).map_err(|e| format!("{set_path}: {e}"))?;
    let patterns: Vec<&str> = set.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    let text = std::fs::read_to_string(&list).map_err(|e| format!("{list}: {e}"))?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for path in text.lines().filter_map(|l| l.split('\t').next()) {
        let name = path.rsplit('/').next().unwrap_or(path);
        if patterns.iter().any(|p| glob(p, name)) && !files.iter().any(|f| f.0 == name) {
            files.push((name.to_string(), read_listed(path)?));
        }
    }
    // What the native document job reads with no search (`engine_doc::carried`): for each
    // font of the TFM table, its `.vf` if the pin has one, and its first `pdftex.map` line,
    // gathered into `rushtex.map`.
    let tfm = rushtex::data::data_tfm::fonts().ok_or("no TFM table in this build")?;
    let in_table = |stem: &str| tfm.lookup(stem).is_some();
    let mut map = String::new();
    for path in text.lines().filter_map(|l| l.split('\t').next()) {
        let name = path.rsplit('/').next().unwrap_or(path);
        if name.strip_suffix(".vf").is_some_and(in_table) && !files.iter().any(|f| f.0 == name) {
            files.push((name.to_string(), read_listed(path)?));
        }
        if name == "pdftex.map" && map.is_empty() {
            let full: String = read_listed(path)?.iter().map(|&c| char::from(c)).collect();
            let mut seen = std::collections::HashSet::new();
            for l in full.lines() {
                if let Some(n) = l.split_whitespace().next().filter(|n| !n.starts_with(['%', '#', '*', ';'])) {
                    // mapfile.c keeps a name's first line.
                    if seen.insert(n) && in_table(n) {
                        map.push_str(l);
                        map.push('\n');
                    }
                }
            }
        }
    }
    if map.is_empty() {
        return Err("the list has no pdftex.map".into());
    }
    files.push(("rushtex.map".into(), map.bytes().collect()));
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let raw: usize = files.iter().map(|f| f.1.len()).sum();
    let out = arg("--out").unwrap_or_else(|| "assets/tables/embed.rtdb".into());
    let mut w = BlobWriter::default();
    let packed = data_embed::write(&mut w, &files)?;
    std::fs::write(&out, w.finish()).map_err(|e| format!("{out}: {e}"))?;
    let bytes = std::fs::read(&out).map_err(|e| format!("{out}: {e}"))?;
    let table = Blob::new(&bytes).and_then(|b| data_embed::EmbedTable::from_blob(&b)).ok_or("not an embed table")?;
    data_embed::check(&table, &files)?;
    Ok(format!("{out}: {} files, {raw} bytes as {packed} compressed ({} in all), every file read back exactly", files.len(), bytes.len()))
}

/// The harvest rule (`2026-10-23-kernel-macro-harvest.md`): the macros of `e` whose bodies use
/// only characters (no `&`, no active character), parameters, the primitives of
/// `data_kernel::MACRO_PRIMITIVES`, TeX parameters, `\chardef`, `\mathchardef` and `\let`
/// characters, and names already taken, iterated to a fixed point from `taken` (plus every
/// `\chardef` and `\mathchardef`). A robust command (`\protect\name\space`) qualifies with its
/// inner macro. Also, for each macro left out, every distinct token of its body that keeps it
/// out (a macro left out, a primitive outside the set, `&`, an active character).
#[cfg(feature = "oracle")]
fn harvest(e: &rushtex::engine::Engine, mut taken: std::collections::HashSet<u32>) -> (Vec<u32>, std::collections::HashMap<u32, Vec<String>>) {
    use rushtex::data::data_kernel::MACRO_PRIMITIVES;
    use rushtex::engine::engine_catcode::Cat;
    use rushtex::engine::engine_eqtb::{Reg, Region};
    use rushtex::engine::engine_prim::Meaning;
    use rushtex::engine::engine_tables::{DIMEN_PARAMS, GLUE_PARAMS, INT_PARAMS};
    use rushtex::engine::engine_token::Token;
    let params = |r: Reg| match r.region {
        Region::Int => r.slot < INT_PARAMS,
        Region::Dimen => r.slot < DIMEN_PARAMS,
        Region::Glue => r.slot < GLUE_PARAMS,
        _ => false,
    };
    taken.extend((0..e.cs.len() as u32).filter(|&id| matches!(e.eqtb.meaning(id), Meaning::CharDef(_) | Meaning::MathCharDef(_))));
    // Why a token is not taken, or `None` if it is.
    let blocker = |t: Token, taken: &std::collections::HashSet<u32>| -> Option<String> {
        match t.cs_id() {
            Some(id) if taken.contains(&id) => None,
            Some(id) => match e.eqtb.meaning(id) {
                Meaning::Prim(p) if MACRO_PRIMITIVES.contains(&p.name()) => None,
                Meaning::Reg(r) if params(r) => None,
                Meaning::Char(c) if !matches!(c.cat(), Some(Cat::AlignTab | Cat::Active)) => None,
                Meaning::Prim(p) => Some(format!("\\{} (primitive)", p.name())),
                Meaning::Macro(_) => Some(format!("\\{}", e.cs.name(id))),
                m => Some(format!("\\{} ({})", e.cs.name(id), format!("{m:?}").split('(').next().unwrap_or(""))),
            },
            None if matches!(t.cat(), Some(Cat::AlignTab)) => Some("& (alignment)".into()),
            None if matches!(t.cat(), Some(Cat::Active)) => Some(format!("active {}", t.chr())),
            None => None,
        }
    };
    let inner = |id: u32| -> Option<u32> {
        let Meaning::Macro(mac) = e.eqtb.meaning(id) else { return None };
        match e.toks.get(mac.text) {
            [m, p, x] if *m == Token::END_MATCH && p.cs_id().is_some_and(|p| e.cs.name(p) == "protect") => x.cs_id().filter(|&x| e.cs.name(x) == format!("{} ", e.cs.name(id))),
            _ => None,
        }
    };
    let body_blocker = |id: u32, taken: &std::collections::HashSet<u32>| -> Option<String> {
        let body = inner(id).unwrap_or(id);
        if body != id && taken.contains(&body) {
            return None;
        }
        let Meaning::Macro(mac) = e.eqtb.meaning(body) else { return Some("not a macro".into()) };
        e.toks.get(mac.text).iter().find_map(|&t| blocker(t, taken))
    };
    let mut harvested = Vec::new();
    loop {
        let before = taken.len();
        for id in 0..e.cs.len() as u32 {
            if !taken.contains(&id) && matches!(e.eqtb.meaning(id), Meaning::Macro(_)) && body_blocker(id, &taken).is_none() {
                taken.insert(id);
                harvested.push(id);
            }
        }
        if taken.len() == before {
            break;
        }
    }
    let left = (0..e.cs.len() as u32)
        .filter(|id| !taken.contains(id) && matches!(e.eqtb.meaning(*id), Meaning::Macro(_)))
        .filter_map(|id| {
            let body = inner(id).filter(|b| !taken.contains(b)).unwrap_or(id);
            let Meaning::Macro(mac) = e.eqtb.meaning(body) else { return None };
            let mut all: Vec<String> = e.toks.get(mac.text).iter().filter_map(|&t| blocker(t, &taken)).collect();
            all.sort_unstable();
            all.dedup();
            Some((id, all))
        })
        .collect();
    (harvested, left)
}

/// The harvested macros a document can reach: names `user` accepts and every harvested name
/// their bodies name, to a fixed point; robust commands as their inner macro, protected.
#[cfg(feature = "oracle")]
fn harvested_macros<'e>(e: &'e rushtex::engine::Engine, harvested: &[u32], user: impl Fn(&str) -> bool) -> Vec<(String, u8, Vec<rushtex::data::data_kernel::MacroToken<'e>>)> {
    use rushtex::data::data_kernel::MacroToken;
    use rushtex::engine::engine_prim::Meaning;
    use rushtex::engine::engine_token::Token;
    let inner = |id: u32| -> Option<u32> {
        let Meaning::Macro(mac) = e.eqtb.meaning(id) else { return None };
        match e.toks.get(mac.text) {
            [m, p, x] if *m == Token::END_MATCH && p.cs_id().is_some_and(|p| e.cs.name(p) == "protect") => x.cs_id().filter(|&x| e.cs.name(x) == format!("{} ", e.cs.name(id))),
            _ => None,
        }
    };
    let mut macros: Vec<(String, u8, Vec<MacroToken>)> = harvested
        .iter()
        .filter_map(|&id| {
            let body = inner(id).unwrap_or(id);
            let Meaning::Macro(mac) = e.eqtb.meaning(body) else { return None };
            let flags = if body != id { mac.flags | rushtex::engine::engine_prim::PROTECTED } else { mac.flags };
            let toks = e.toks.get(mac.text).iter().map(|t| match t.cs_id() {
                Some(c) => MacroToken::Cs(e.cs.name(c)),
                None => MacroToken::Raw(t.0),
            });
            let name = e.cs.name(id);
            (!name.contains(['\n', '\u{1f}'])).then(|| (name.to_string(), flags, toks.collect()))
        })
        .collect();
    let mut keep: std::collections::HashSet<String> = macros.iter().filter(|m| user(&m.0)).map(|m| m.0.clone()).collect();
    loop {
        let before = keep.len();
        let reached: Vec<String> = macros.iter().filter(|m| keep.contains(&m.0)).flat_map(|m| m.2.iter().filter_map(|t| match t { MacroToken::Cs(n) => Some(n.to_string()), _ => None })).collect();
        keep.extend(reached.into_iter().filter(|n| macros.iter().any(|m| &m.0 == n)));
        if keep.len() == before {
            break;
        }
    }
    macros.retain(|m| keep.contains(&m.0));
    macros.sort_by(|a, b| a.0.cmp(&b.0));
    macros.dedup_by(|a, b| a.0 == b.0);
    macros
}

/// `rushtex-gen measure --pkgs FILE --corpus DIR`: how much of what papers use from each
/// package the harvest rule takes. FILE has one package per line (`name<TAB>papers`). DIR is
/// a corpus from `fetch_arxiv_corpus.py`: each distinct paper's `main.tex` (comments and white
/// space ignored), less the papers with their own `.sty`/`.cls` (they run on the engine,
/// `native-architecture.md` §2a); a paper uses a name when it writes `\name` or
/// `\begin{name}`. Each package is loaded on `article`; its commands are the letter names whose
/// meaning differs from plain `article`'s, less names that change in at least half the loads
/// (LaTeX's own bookkeeping). A command plain `article` already has is a kernel command the
/// package redefines, listed apart. An added command is data (a `\chardef`, register, `\let`
/// or font), taken by the rule, or left out with what keeps it out. Uses are papers writing
/// the command, each command counted once. Measures; writes nothing.
#[cfg(feature = "oracle")]
fn measure(args: &[String]) -> Result<String, String> {
    use rushtex::engine::engine_prim::Meaning;
    use std::collections::{BTreeSet, HashMap, HashSet};
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let list = arg("--pkgs").ok_or("--pkgs FILE is required")?;
    let text = std::fs::read_to_string(&list).map_err(|e| format!("{list}: {e}"))?;
    let pkgs: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.split('\t').next()).filter(|p| !p.is_empty()).collect();
    // The corpus: per name, the papers that write it.
    let dir = arg("--corpus").ok_or("--corpus DIR is required")?;
    let (mut uses, mut seen, mut papers): (HashMap<String, u32>, HashSet<u64>, u32) = (HashMap::new(), HashSet::new(), 0);
    let mut paper_dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&dir).map_err(|e| format!("{dir}: {e}"))?.filter_map(|d| d.ok().map(|d| d.path())).filter(|p| p.join("main.tex").is_file()).collect();
    paper_dirs.sort();
    fn has_style(d: &std::path::Path) -> bool {
        std::fs::read_dir(d).into_iter().flatten().flatten().any(|e| {
            let p = e.path();
            if p.is_dir() {
                has_style(&p)
            } else {
                p.extension().is_some_and(|x| x == "sty" || x == "cls")
            }
        })
    }
    for d in &paper_dirs {
        if has_style(d) {
            continue;
        }
        let raw = std::fs::read(d.join("main.tex")).map_err(|e| format!("{}: {e}", d.display()))?;
        let src: String = raw.iter().map(|&b| b as char).collect();
        let body: String = src.lines().map(|l| match l.char_indices().find(|&(i, c)| c == '%' && !l[..i].ends_with('\\')) {
            Some((i, _)) => &l[..i],
            None => l,
        }).collect::<String>();
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            body.chars().filter(|c| !c.is_whitespace()).collect::<String>().hash(&mut h);
            h.finish()
        };
        if !seen.insert(key) {
            continue;
        }
        papers += 1;
        let b = body.as_bytes();
        let mut names = BTreeSet::new();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'\\' {
                let n = b[i + 1..].iter().take_while(|c| c.is_ascii_alphabetic()).count();
                let name = &body[i + 1..i + 1 + n];
                if name == "begin" {
                    if let Some(env) = body[i + 1 + n..].strip_prefix('{').and_then(|r| r.split_once('}')).map(|r| r.0) {
                        names.insert(env.trim_end_matches('*').to_string());
                    }
                }
                if n > 0 {
                    names.insert(name.to_string());
                }
                i += 1 + n.max(1);
            } else {
                i += 1;
            }
        }
        for n in names {
            *uses.entry(n).or_default() += 1;
        }
    }
    let engine = Engine::new(arg("--format"))?;
    let read = |name: &str| engine.kpse.read(name);
    let user = |n: &str| !n.is_empty() && n.bytes().all(|b| b.is_ascii_alphabetic());
    let run = |src: &str, inspect: &mut dyn FnMut(&mut rushtex::engine::engine_run::Machine)| -> Result<Vec<String>, String> {
        let job = rushtex::engine::engine_job::run(&|| Some(engine.fmt.clone()), src, &rushtex::engine::engine_job::Opts { job_name: "rtgen", now: 0, max_runs: 1 }, &read, &mut |_| {}, inspect).ok_or("the format is not one of this build")?;
        Ok(job.errors)
    };
    let mut base: HashMap<String, Meaning> = HashMap::new();
    run("\\documentclass{article}\\begin{document}", &mut |m| {
        base = (0..m.e.cs.len() as u32).map(|id| (m.e.cs.name(id).to_string(), m.e.eqtb.meaning(id))).filter(|(n, _)| user(n)).collect();
    })?;
    // Per package: its commands, each as data, taken, or left out with the tokens that keep it
    // out, here and in the macros it reaches that are left out too.
    enum Kind {
        Data,
        Taken,
        Left { direct: Vec<String>, reach: usize, prims: BTreeSet<String> },
    }
    struct Cmd {
        name: String,
        patch: bool,
        kind: Kind,
    }
    let missing: Vec<&str> = pkgs.iter().copied().filter(|p| engine.kpse.read(&format!("{p}.sty")).is_none()).collect();
    let mut loaded: Vec<(&str, Vec<Cmd>)> = Vec::new();
    for &pkg in &pkgs {
        if missing.contains(&pkg) {
            continue;
        }
        let mut cmds = Vec::new();
        run(&format!("\\documentclass{{article}}\\usepackage{{{pkg}}}\\begin{{document}}"), &mut |m| {
            let e = &*m.e;
            let seed: HashSet<u32> = ["protect"].iter().filter_map(|n| e.cs.get(n)).collect();
            let (harvested, left) = harvest(e, seed);
            let harvested: HashSet<u32> = harvested.into_iter().collect();
            let macro_of = |b: &str| b.strip_prefix('\\').filter(|n| !n.contains(" (")).and_then(|n| e.cs.get(n)).filter(|id| left.contains_key(id));
            cmds = (0..e.cs.len() as u32)
                .filter(|&id| user(e.cs.name(id)) && base.get(e.cs.name(id)) != Some(&e.eqtb.meaning(id)))
                .map(|id| {
                    let kind = match e.eqtb.meaning(id) {
                        Meaning::Macro(_) if harvested.contains(&id) => Kind::Taken,
                        Meaning::Macro(_) => {
                            let direct = left.get(&id).cloned().unwrap_or_default();
                            let (mut seen, mut stack, mut prims) = (HashSet::from([id]), vec![id], BTreeSet::new());
                            while let Some(x) = stack.pop() {
                                for b in left.get(&x).into_iter().flatten() {
                                    match macro_of(b) {
                                        Some(m) => {
                                            if seen.insert(m) {
                                                stack.push(m);
                                            }
                                        }
                                        None => {
                                            prims.insert(b.clone());
                                        }
                                    }
                                }
                            }
                            Kind::Left { direct, reach: seen.len() - 1, prims }
                        }
                        _ => Kind::Data,
                    };
                    Cmd { name: e.cs.name(id).to_string(), patch: base.contains_key(e.cs.name(id)), kind }
                })
                .collect();
        })?;
        loaded.push((pkg, cmds));
    }
    let mut changed: HashMap<&str, usize> = HashMap::new();
    for (_, cmds) in &loaded {
        for c in cmds {
            *changed.entry(c.name.as_str()).or_default() += 1;
        }
    }
    let noise: HashSet<&str> = changed.iter().filter(|(_, &c)| 2 * c >= loaded.len()).map(|(n, _)| *n).collect();
    // Rows per package; the headline counts each command once (its first package).
    let mut rows = Vec::new();
    let mut counted: HashSet<&str> = HashSet::new();
    let (mut sum, mut sum_ok) = (0u32, 0u32);
    let mut blockers: HashMap<&str, (u32, BTreeSet<&str>)> = HashMap::new();
    let mut needs: Vec<(u32, &str, &str, &Cmd)> = Vec::new();
    let mut patched: Vec<(u32, &str, &str)> = Vec::new();
    for (pkg, cmds) in &loaded {
        let (mut w, mut w_ok, mut n_used) = (0u32, 0u32, 0);
        for c in cmds.iter().filter(|c| !noise.contains(c.name.as_str())) {
            let u = uses.get(&c.name).copied().unwrap_or(0);
            if u == 0 {
                continue;
            }
            if c.patch {
                if counted.insert(&c.name) {
                    patched.push((u, &c.name, pkg));
                }
                continue;
            }
            n_used += 1;
            w += u;
            let ok = !matches!(c.kind, Kind::Left { .. });
            if ok {
                w_ok += u;
            }
            if counted.insert(&c.name) {
                sum += u;
                if ok {
                    sum_ok += u;
                }
                if let Kind::Left { direct, .. } = &c.kind {
                    needs.push((u, &c.name, pkg, c));
                    for b in direct {
                        let x = blockers.entry(b.as_str()).or_default();
                        x.0 += u;
                        x.1.insert(&c.name);
                    }
                }
            }
        }
        let pct = |k: u32| (100 * k).checked_div(w).unwrap_or(100);
        rows.push(format!("{pkg:<11} {n_used:>3} added commands used  uses {w:>5}  rule or data {:>3}%", pct(w_ok)));
    }
    let top = arg("--top").and_then(|t| t.parse().ok()).unwrap_or(40);
    let mut s = format!("{papers} distinct papers without their own .sty/.cls; {} names used\n", uses.len());
    s.push_str(&rows.join("\n"));
    s.push_str(&format!("\nnot in the pinned tree: {}\nall: {sum} uses of commands the packages add (each counted once), {}% taken by the rule or data\n", missing.join(" "), (100 * sum_ok).checked_div(sum).unwrap_or(100)));
    needs.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    s.push_str("commands left out, by uses (package; macros left out it reaches; primitives they need; what its own body needs):\n");
    for (u, n, pkg, c) in needs.iter().take(top) {
        if let Kind::Left { direct, reach, prims } = &c.kind {
            s.push_str(&format!("  {u:>4} \\{n:<16} {pkg:<10} reach {reach:>3}  prims {:>2}  needs {}\n", prims.len(), direct.join(" ")));
        }
    }
    let mut ranked: Vec<(&&str, &(u32, BTreeSet<&str>))> = blockers.iter().collect();
    ranked.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(b.0)));
    s.push_str("what their own bodies need, by the uses of the commands needing it:\n");
    for (b, (u, cs)) in ranked.iter().take(top) {
        s.push_str(&format!("  {u:>5}  {b:<28} {}\n", cs.iter().take(8).map(|c| format!("\\{c}")).collect::<Vec<_>>().join(" ")));
    }
    patched.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    s.push_str(&format!("kernel commands the packages redefine (hook points a native command must offer), by uses:\n  {}\n", patched.iter().take(top).map(|(u, n, p)| format!("\\{n} {u} ({p})")).collect::<Vec<_>>().join(", ")));
    Ok(s)
}

/// The math version `normal` of `e` (`data_kernel::MathFont`): `\mv@normal`'s symbol fonts
/// (`\getanddefine@fonts \symX \ENC/fam/ser/sh`) and math alphabets (`\install@mathalphabet
/// \name {\select@group \name \M@ENC \ENC/fam/ser/sh}`) in order, each with its shape in
/// `\mv@bold`, and the symbol font alphabets: names whose inner macro is `\relax \ifmmode
/// \else \non@alpherr \name \fi \use@mathgroup \M@ENC \symX`.
#[cfg(feature = "oracle")]
fn math_fonts(e: &rushtex::engine::Engine) -> Result<Vec<rushtex::data::data_kernel::MathFont<String>>, String> {
    use rushtex::data::data_kernel::MathFont;
    use rushtex::engine::engine_prim::Meaning;
    let body = |name: &str| -> Result<Vec<rushtex::engine::engine_token::Token>, String> {
        match e.cs.get(name).map(|id| e.eqtb.meaning(id)) {
            Some(Meaning::Macro(m)) => Ok(e.toks.get(m.text).iter().copied().filter(|&t| t != rushtex::engine::engine_token::Token::END_MATCH).collect()),
            _ => Err(format!("\\{name} is not a macro")),
        }
    };
    let cs = |t: &rushtex::engine::engine_token::Token| t.cs_id().map(|c| e.cs.name(c).to_string());
    let fam = |t: &rushtex::engine::engine_token::Token| -> Result<u8, String> {
        match t.cs_id().map(|c| e.eqtb.meaning(c)) {
            Some(Meaning::CharDef(v) | Meaning::MathCharDef(v)) => Ok(v as u8),
            _ => Err(format!("{:?} is not a family", cs(t))),
        }
    };
    let text_enc = body("f@encoding")?.iter().map(|t| t.chr()).collect::<String>();
    // Whether the switch `\M@ENC` changes nothing the native job reads: it means what the text
    // encoding's does (`\use@mathgroup` skips it), or it only runs macros that are empty or
    // `\default@M` (`\def\accentclass@{7}`: the class of math accents declared later, while
    // the kernel's and the tables' accents carry their class as digits).
    let same = |enc: &str| -> Result<bool, String> {
        let b = body(enc)?;
        if b == body(&format!("M@{text_enc}"))? {
            return Ok(true);
        }
        // `\default@M` must still be `\def\accentclass@{7}` (or empty, as amsmath leaves it).
        let d: Vec<String> = body("default@M")?.iter().map(|t| cs(t).unwrap_or_else(|| t.chr().to_string())).collect();
        if !d.is_empty() && d != ["def", "accentclass@", "{", "7", "}"] {
            return Err(format!("\\default@M is {d:?}"));
        }
        Ok(b.iter().all(|t| match cs(t).as_deref() {
            Some("default@M") => true,
            Some(n) => body(n).is_ok_and(|x| x.is_empty()),
            None => false,
        }))
    };
    // `\mv@normal` and `\mv@bold` as (kind, name or family, encoding switch, shape).
    let version = |v: &str| -> Result<Vec<(bool, String, String, String)>, String> {
        let b = body(v)?;
        let mut out = Vec::new();
        let mut i = 0;
        while i < b.len() {
            match cs(&b[i]).as_deref() {
                Some("getanddefine@fonts") => {
                    out.push((true, fam(&b[i + 1])?.to_string(), String::new(), cs(&b[i + 2]).ok_or("no shape")?));
                    i += 3;
                }
                Some("install@mathalphabet") => {
                    // `\name {\select@group \name \M@ENC \SHAPE }`
                    let name = cs(&b[i + 1]).ok_or("no alphabet")?;
                    out.push((false, name.trim_end().to_string(), cs(&b[i + 5]).ok_or("no encoding")?, cs(&b[i + 6]).ok_or("no shape")?));
                    i += 8;
                }
                x => return Err(format!("\\{v}: {x:?}")),
            }
        }
        Ok(out)
    };
    let (normal, bold) = (version("mv@normal")?, version("mv@bold")?);
    if normal.len() != bold.len() || normal.iter().zip(&bold).any(|(n, b)| n.0 != b.0 || n.1 != b.1) {
        return Err("\\mv@normal and \\mv@bold differ in shape".into());
    }
    let mut fonts = Vec::new();
    for (n, b) in normal.iter().zip(&bold) {
        fonts.push(if n.0 {
            MathFont::Symbol { fam: n.1.parse().map_err(|_| "a family")?, normal: n.3.clone(), bold: b.3.clone() }
        } else {
            MathFont::Alphabet { name: n.1.clone(), same: same(&n.2)?, normal: n.3.clone(), bold: b.3.clone() }
        });
    }
    let mut symbol = Vec::new();
    for id in 0..e.cs.len() as u32 {
        let name = e.cs.name(id);
        let Some(outer) = name.strip_suffix(' ').filter(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_alphabetic())) else { continue };
        let Meaning::Macro(m) = e.eqtb.meaning(id) else { continue };
        let t: Vec<_> = e.toks.get(m.text).iter().copied().filter(|&t| t != rushtex::engine::engine_token::Token::END_MATCH).collect();
        let names: Vec<Option<String>> = t.iter().map(cs).collect();
        let want = ["relax", "ifmmode", "else", "non@alpherr", name, "fi", "use@mathgroup"];
        if t.len() == 9 && want.iter().zip(&names).all(|(w, n)| n.as_deref() == Some(*w)) {
            symbol.push(MathFont::SymbolAlphabet { name: outer.to_string(), fam: fam(&t[8])?, same: same(names[7].as_deref().ok_or("no encoding")?)? });
        }
    }
    symbol.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    fonts.extend(symbol);
    Ok(fonts)
}

/// A table's math font as the generator's own (owned) value.
#[cfg(feature = "oracle")]
fn owned_math_font(f: &rushtex::data::data_kernel::MathFont<&str>) -> rushtex::data::data_kernel::MathFont<String> {
    use rushtex::data::data_kernel::MathFont;
    match *f {
        MathFont::Symbol { fam, normal, bold } => MathFont::Symbol { fam, normal: normal.into(), bold: bold.into() },
        MathFont::SymbolAlphabet { name, fam, same } => MathFont::SymbolAlphabet { name: name.into(), fam, same },
        MathFont::Alphabet { name, same, normal, bold } => MathFont::Alphabet { name: name.into(), same, normal: normal.into(), bold: bold.into() },
    }
}

/// A harvested macro's token, owned (`data_kernel::MacroToken` borrows the engine's names).
#[cfg(feature = "oracle")]
#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Cs(String),
    Raw(u32),
}

#[cfg(feature = "oracle")]
impl Tok {
    fn own(t: rushtex::data::data_kernel::MacroToken) -> Tok {
        match t {
            rushtex::data::data_kernel::MacroToken::Cs(n) => Tok::Cs(n.to_string()),
            rushtex::data::data_kernel::MacroToken::Raw(v) => Tok::Raw(v),
        }
    }

    fn borrow(&self) -> rushtex::data::data_kernel::MacroToken<'_> {
        match self {
            Tok::Cs(n) => rushtex::data::data_kernel::MacroToken::Cs(n),
            Tok::Raw(v) => rushtex::data::data_kernel::MacroToken::Raw(*v),
        }
    }
}

/// What a format or a package leaves that `data_kernel` holds as data: `\chardef`s and
/// `\mathchardef`s, log-like operators, delimiter macros, harvested macros, the math version.
#[cfg(feature = "oracle")]
struct KernelValues {
    chars: Vec<(String, rushtex::data::data_kernel::Def)>,
    ops: Vec<(String, String, bool)>,
    delims: Vec<(String, u32)>,
    macros: Vec<(String, u8, Vec<Tok>)>,
    fonts: Vec<rushtex::data::data_kernel::MathFont<String>>,
}

/// The [`KernelValues`] of `e`, at the document's first token.
#[cfg(feature = "oracle")]
fn kernel_values(e: &rushtex::engine::Engine) -> Result<KernelValues, String> {
    use rushtex::engine::engine_prim::Meaning;
    let user = |n: &str| n.bytes().all(|b| b.is_ascii_alphabetic()) || n.chars().count() == 1 && !n.starts_with(|c: char| c.is_ascii_alphabetic());
    let mut chars: Vec<(String, rushtex::data::data_kernel::Def)> = (0..e.cs.len() as u32)
        .filter(|&id| user(e.cs.name(id)) && !e.cs.name(id).is_empty())
        .filter_map(|id| match e.eqtb.meaning(id) {
            Meaning::CharDef(v) => Some((e.cs.name(id).to_string(), rushtex::data::data_kernel::Def::Char(v))),
            Meaning::MathCharDef(v) => Some((e.cs.name(id).to_string(), rushtex::data::data_kernel::Def::MathChar(v))),
            _ => None,
        })
        .collect();
    chars.sort();
    // The log-like operators: a robust command whose inner `\name ` is
    // `\mathop{\operator@font word}` with or without `\nolimits`.
    let mut ops: Vec<(String, String, bool)> = Vec::new();
    let mut delims: Vec<(String, u32)> = Vec::new();
    for id in 0..e.cs.len() as u32 {
        let full = e.cs.name(id);
        // fontmath.ltx's `\DeclareMathDelimiter`s: `\name` is `\delimiter"XXXXXXX `.
        if user(full) && !full.is_empty() {
            if let Meaning::Macro(mac) = e.eqtb.meaning(id) {
                let text: String = e.toks.get(mac.text).iter().map(|t| if t.cs_id().is_some() { '\u{1}' } else { t.chr() }).collect();
                let is_delim = e.toks.get(mac.text).get(1).and_then(|t| t.cs_id()).is_some_and(|c| e.cs.name(c) == "delimiter");
                if let Some(v) = text.strip_prefix("\u{0}\u{1}\"").and_then(|r| r.strip_suffix(' ')).and_then(|h| u32::from_str_radix(h, 16).ok()).filter(|_| is_delim) {
                    delims.push((full.to_string(), v));
                }
            }
        }
        let Some(name) = full.strip_suffix(' ').filter(|n| user(n)) else { continue };
        let Meaning::Macro(mac) = e.eqtb.meaning(id) else { continue };
        let mut body = String::new();
        for t in e.toks.get(mac.text) {
            match t.cs_id() {
                Some(c) => {
                    body.push('\\');
                    body.push_str(e.cs.name(c));
                    body.push(' ');
                }
                None => body.push(t.chr()),
            }
        }
        let Some(rest) = body.strip_prefix("\u{0}\\mathop {\\operator@font ").or_else(|| body.strip_prefix("\\mathop {\\operator@font ")) else { continue };
        let (word, nolimits) = match rest.split_once('}') {
            Some((w, "")) => (w, false),
            Some((w, "\\nolimits ")) => (w, true),
            _ => continue,
        };
        let word = word.replace("\\, ", "\\,");
        if word.replace("\\,", "").bytes().all(|b| b.is_ascii_alphabetic()) && !word.is_empty() {
            ops.push((name.to_string(), word, nolimits));
        }
    }
    ops.sort();
    delims.sort();
    // Kernel macros to a fixed point (`harvest`), keeping what a document can name.
    let seed = ops.iter().map(|o| o.0.as_str()).chain(delims.iter().map(|d| d.0.as_str())).chain(["protect"]).filter_map(|n| e.cs.get(n));
    let (harvested, _) = harvest(e, seed.collect());
    let macros = harvested_macros(e, &harvested, |n| n.bytes().all(|b| b.is_ascii_alphabetic())).into_iter().map(|(n, f, t)| (n, f, t.into_iter().map(Tok::own).collect())).collect::<Vec<(String, u8, Vec<Tok>)>>();
    // The `\chardef`s and `\mathchardef`s the macros name (`\dabar@`), internal or not.
    for (_, _, toks) in &macros {
        for t in toks {
            let Tok::Cs(n) = t else { continue };
            let d = match e.cs.get(n.as_str()).map(|id| e.eqtb.meaning(id)) {
                Some(Meaning::CharDef(v)) => rushtex::data::data_kernel::Def::Char(v),
                Some(Meaning::MathCharDef(v)) => rushtex::data::data_kernel::Def::MathChar(v),
                _ => continue,
            };
            if !chars.iter().any(|c| c.0 == *n) {
                chars.push((n.clone(), d));
            }
        }
    }
    chars.sort();
    Ok(KernelValues { chars, ops, delims, macros, fonts: math_fonts(e)? })
}

/// `rushtex-gen natpkg NAME… [--out-dir DIR]`: each package's table for the native job
/// (`DIR/pkg-NAME.rtdb`, the `data_kernel` format plus `pkg`). The package is loaded by LaTeX
/// on `article` after the packages it loads (the registry's load graph, `pkgs.rtdb`), and
/// again without it; the table holds the differences of their [`KernelValues`]: new or
/// changed names, the symbol fonts past the earlier families, new alphabets. Refused when the
/// package changes a parameter, code or catcode, or a name a document can write that the
/// table does not hold and the earlier run had (the native job would keep the old meaning).
/// Names it adds that the table does not hold are listed: the native job does not know them,
/// so the pre-scan sends a document using them to the engine.
#[cfg(feature = "oracle")]
fn natpkg(args: &[String]) -> Result<String, String> {
    use rushtex::data::data_kernel::{Def, MathFont};
    use rushtex::engine::engine_eqtb::{Reg, Region};
    use rushtex::engine::engine_prim::Meaning;
    use rushtex::engine::engine_tables::{DIMEN_PARAMS, GLUE_PARAMS, INT_PARAMS};
    use std::collections::HashMap;
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let dir = arg("--out-dir").unwrap_or_else(|| "assets/tables".into());
    let names: Vec<&String> = args.iter().enumerate().filter(|(i, a)| !a.starts_with("--") && (*i == 0 || !args[i - 1].starts_with("--"))).map(|(_, a)| a).collect();
    let engine = Engine::new(arg("--format"))?;
    let read = |name: &str| engine.kpse.read(name);
    let reg = rushtex::data::data_pkgs::pkgs().ok_or("no package registry")?;
    // A run's values, its parameters, codes and catcodes, and each letter name's meaning.
    type Run = (KernelValues, Vec<i32>, HashMap<String, String>);
    let probe = |pkgs: &[String]| -> Result<Run, String> {
        let src = format!("\\documentclass{{article}}{}\\begin{{document}}", pkgs.iter().map(|p| format!("\\usepackage{{{p}}}")).collect::<String>());
        let mut out = None;
        let job = rushtex::engine::engine_job::run(&|| Some(engine.fmt.clone()), &src, &rushtex::engine::engine_job::Opts { job_name: "rtgen", now: 0, max_runs: 1 }, &read, &mut |_| {}, &mut |m| {
            let e = &*m.e;
            let mut words: Vec<i32> = (0..INT_PARAMS).map(|slot| e.eqtb.word(Reg { region: Region::Int, slot })).chain((0..DIMEN_PARAMS).map(|slot| e.eqtb.word(Reg { region: Region::Dimen, slot }))).collect();
            words.extend((0..GLUE_PARAMS).flat_map(|slot| {
                let g = e.eqtb.glue(Reg { region: Region::Glue, slot });
                [g.width, g.stretch, g.shrink, g.stretch_order as i32, g.shrink_order as i32]
            }));
            use rushtex::engine::engine_eqtb::Code;
            words.extend([Code::Lc, Code::Uc, Code::Sf, Code::Math, Code::Del].iter().flat_map(|&k| (0..256).map(move |c| e.eqtb.code(k, c))));
            words.extend(e.eqtb.cats().0.iter().map(|&c| c as i32));
            let meanings = (0..e.cs.len() as u32)
                .filter(|&id| !e.cs.name(id).is_empty())
                .map(|id| {
                    let m = match e.eqtb.meaning(id) {
                        Meaning::Macro(mac) => format!("macro {} {:?}", mac.flags, e.toks.get(mac.text).iter().map(|t| t.cs_id().map_or_else(|| format!("{:x}", t.0), |c| e.cs.name(c).to_string())).collect::<Vec<_>>()),
                        Meaning::Font(_) => "font".into(),
                        m => format!("{m:?}"),
                    };
                    (e.cs.name(id).to_string(), m)
                })
                .collect();
            out = Some(kernel_values(e).map(|v| (v, words, meanings)));
        })
        .ok_or("the format is not one of this build")?;
        if !job.errors.is_empty() {
            return Err(format!("loading {pkgs:?}: {}", job.errors[0].lines().next().unwrap_or("")));
        }
        out.ok_or("no state")?
    };
    let mut notes = Vec::new();
    for &name in &names {
        // The packages it loads, in load order, each after its own.
        let id = reg.file(&format!("{name}.sty")).ok_or_else(|| format!("{name}.sty is not registered"))?;
        let (mut deps, mut direct) = (Vec::new(), Vec::new());
        fn visit(reg: &rushtex::data::data_pkgs::PkgTable, id: u32, deps: &mut Vec<String>) {
            for d in reg.loads(id).iter() {
                visit(reg, d, deps);
                if let Some(n) = reg.file_name(d).strip_suffix(".sty") {
                    if !deps.iter().any(|x| x == n) {
                        deps.push(n.to_string());
                    }
                }
            }
        }
        visit(reg, id, &mut deps);
        for d in reg.loads(id).iter() {
            if let Some(n) = reg.file_name(d).strip_suffix(".sty") {
                direct.push(n.to_string());
            }
        }
        let (before, before_words, before_m) = probe(&deps)?;
        let mut with = deps.clone();
        with.push(name.clone());
        let (after, after_words, after_m) = probe(&with)?;
        if before_words != after_words {
            let n = before_words.iter().zip(&after_words).filter(|(a, b)| a != b).count();
            return Err(format!("{name} changes {n} parameters, codes or catcodes"));
        }
        let new = |a: &[(String, Def)], b: &[(String, Def)]| a.iter().filter(|x| !b.contains(x)).cloned().collect::<Vec<_>>();
        let chars = new(&after.chars, &before.chars);
        let ops: Vec<_> = after.ops.iter().filter(|x| !before.ops.contains(x)).cloned().collect();
        let delims: Vec<_> = after.delims.iter().filter(|x| !before.delims.contains(x)).cloned().collect();
        let macros: Vec<_> = after.macros.iter().filter(|x| !before.macros.contains(x)).cloned().collect();
        let fam0 = before.fonts.iter().filter(|f| matches!(f, MathFont::Symbol { .. })).count() as u8;
        let fonts: Vec<_> = after.fonts.iter().filter(|f| !before.fonts.contains(f)).cloned().collect();
        // What changed that the table does not hold.
        let held = |n: &str| {
            let n = n.trim_end();
            chars.iter().any(|c| c.0 == n) || ops.iter().any(|o| o.0 == n) || delims.iter().any(|d| d.0 == n) || macros.iter().any(|m| m.0 == n) || fonts.iter().any(|f| matches!(f, MathFont::SymbolAlphabet { name, .. } | MathFont::Alphabet { name, .. } if name == n))
        };
        // What the native job holds before the package: letter names, and every name of the
        // earlier tables (internal macros the harvest reached, `\dabar@`).
        let letters = |n: &str| n.trim_end().bytes().all(|b| b.is_ascii_alphabetic());
        let had = |n: &str| letters(n) || before.chars.iter().any(|c| c.0 == n) || before.macros.iter().any(|m| m.0 == n) || before.delims.iter().any(|d| d.0 == n) || before.ops.iter().any(|o| o.0 == n);
        let mut lost = Vec::new();
        let mut unknown = Vec::new();
        for (n, m) in &after_m {
            if before_m.get(n) == Some(m) || held(n) || !had(n) && before_m.contains_key(n) {
                continue;
            }
            if before_m.contains_key(n) {
                lost.push(n.trim_end().to_string());
            } else if !n.ends_with(' ') && letters(n) {
                unknown.push(n.clone());
            }
        }
        // A held name the native job writes itself would be skipped at install.
        lost.extend(macros.iter().map(|m| &m.0).chain(chars.iter().map(|c| &c.0)).filter(|n| rushtex::engine::engine_doc::native_name(n)).cloned());
        lost.sort();
        lost.dedup();
        unknown.sort();
        lost.retain(|n| !rushtex::engine::engine_doc::PACKAGE_NATIVES.iter().any(|p| p.0 == name.as_str() && p.1 == n));
        if !lost.is_empty() {
            return Err(format!("{name} changes names the native job would keep: {}", lost.join(" ")));
        }
        // The font shapes it declares (`\DeclareFontShape`), as an NFSS table of its own,
        // checked against LaTeX's `\get@external@font` after the package loads.
        let (decls, errors) = engine.run(&data_nfss::declare_package_source(&deps, name), data_nfss::DECLARE_FILE)?;
        if !errors.is_empty() {
            return Err(format!("{name}: declaring: {}", errors[0].lines().next().unwrap_or("")));
        }
        let own = decls.split_once(&format!("D {name}\n")).map_or("", |r| r.1);
        let (nfams, nshapes) = data_nfss::parse_declarations(own)?;
        let mut w = BlobWriter::default();
        if !nshapes.is_empty() || !nfams.is_empty() {
            data_nfss::write(&mut w, &nfams, &nshapes)?;
        }
        let macros_b: Vec<(String, u8, Vec<rushtex::data::data_kernel::MacroToken>)> = macros.iter().map(|(n, f, t)| (n.clone(), *f, t.iter().map(Tok::borrow).collect())).collect();
        rushtex::data::data_kernel::write(&mut w, &chars, &ops, &delims, &macros_b, &fonts);
        rushtex::data::data_kernel::write_package(&mut w, fam0, &direct);
        let bytes = w.finish();
        let t = Blob::new(&bytes).and_then(|b| rushtex::data::data_kernel::KernelTable::from_blob(&b)).ok_or("the table does not read back")?;
        if !(t.chars().map(|(n, d)| (n.to_string(), d)).eq(chars.iter().cloned()) && t.math_fonts().map(|f| owned_math_font(&f)).eq(fonts.iter().cloned()) && t.macros().count() == macros.len() && t.package().is_some_and(|(f, d)| f == fam0 && d.eq(direct.iter().map(String::as_str)))) {
            return Err(format!("{name}: the table does not read back"));
        }
        if !nshapes.is_empty() {
            let cases: Vec<(String, String)> = nshapes.iter().flat_map(|s| data_nfss::check_sizes(s).into_iter().map(|z| (s.name.clone(), z))).collect();
            let (want, _) = engine.run(&data_nfss::check_package_source(&deps, name, &cases), data_nfss::CHECK_FILE)?;
            let nt = Blob::new(&bytes).and_then(|b| data_nfss::NfssTable::from_blob(&b)).ok_or("the shapes do not read back")?;
            let n = data_nfss::check(&nt, &nfams, &nshapes, &want)?;
            if n != cases.len() {
                return Err(format!("{name}: the check run answered {n} of {} cases", cases.len()));
            }
        }
        let out = format!("{dir}/pkg-{name}.rtdb");
        std::fs::write(&out, &bytes).map_err(|e| format!("{out}: {e}"))?;
        notes.push(format!("{out}: after {deps:?}, {} characters, {} operators, {} delimiters, {} macros, {} math fonts and alphabets, {} font shapes; {} names left to the engine{}", chars.len(), ops.len(), delims.len(), macros.len(), fonts.len(), nshapes.len(), unknown.len(), if args.iter().any(|a| a == "--list") { format!(": {}", unknown.join(" ")) } else { String::new() }));
    }
    Ok(notes.join("\n"))
}

/// `rushtex-gen class`: the state `article` leaves at the first token of the document body
/// (`data_class`), one table per size option, read from the oracle and checked against it.
///
/// Per size: the base probe `\\documentclass[10pt]{article}\\begin{document}` (no `\\end`: the
/// run stops where its input does); one probe per combination of the parameter-only options
/// (`data_class::option_keys`), kept as its differences from the base; one per size command
/// (its `\\baselineskip`, display skips, `\\@listi` and math sizes, each found by setting the
/// quantity to 1sp first and seeing whether the command changes it); one per `\\@listii` to
/// `\\@listvi`. Every combination's table is then loaded into a fresh engine by the native
/// job's own `load_class`, and every parameter, code, catcode, register and font is compared
/// with the oracle's state. The 10pt run also writes the hyphenation and kernel tables.
#[cfg(feature = "oracle")]
fn class(args: &[String]) -> Result<String, String> {
    use rushtex::data::data_class::{self, GenClass, CLASS_DIMENS, SIZE_COMMANDS, SIZE_ROW};
    use rushtex::engine::engine_eqtb::{Code, Glue, Reg, Region};
    use rushtex::engine::engine_prim::{Meaning, Prim};
    use rushtex::engine::engine_tables::{DIMEN_PARAMS, GLUE_PARAMS, INT_PARAMS};
    const CODES: [Code; 5] = [Code::Lc, Code::Uc, Code::Sf, Code::Math, Code::Del];
    type Fonts = Vec<(String, String, i32, i32, i32, Vec<i32>)>;
    type State = (Vec<i32>, Vec<Glue>, Vec<u8>, Fonts);
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let engine = Engine::new(arg("--format"))?;
    let read = |name: &str| engine.kpse.read(name);
    let dir = arg("--out-dir").unwrap_or_else(|| "assets/tables".into());
    // What the check compares, from a machine: parameters, codes, catcodes, and per font
    // (by identifier) its TFM, size, `\hyphenchar`, `\skewchar` and parameters.
    let state = |m: &rushtex::engine::engine_run::Machine, keep: &dyn Fn(&str) -> bool| -> State {
        let e = &*m.e;
        let words = (0..INT_PARAMS).map(|slot| e.eqtb.word(Reg { region: Region::Int, slot })).chain((0..DIMEN_PARAMS).map(|slot| e.eqtb.word(Reg { region: Region::Dimen, slot })));
        let words = words.chain(CODES.iter().flat_map(|&k| (0..256).map(move |c| e.eqtb.code(k, c)))).collect();
        let glues = (0..GLUE_PARAMS).map(|slot| e.eqtb.glue(Reg { region: Region::Glue, slot })).collect();
        let fonts = e.fonts.list.iter().skip(1).filter(|f| keep(e.cs.name(f.ident))).map(|f| (e.cs.name(f.ident).to_string(), f.name.to_string(), f.size, f.hyphen_char, f.skew_char, f.params(&e.fonts.info).to_vec())).collect();
        (words, glues, e.eqtb.cats().0.to_vec(), fonts)
    };
    let glue5 = |x: Glue| [x.width, x.stretch, x.shrink, x.stretch_order as i32, x.shrink_order as i32];
    // A register's value by name, as glue words.
    let named = |e: &rushtex::engine::Engine, name: &str| -> Option<[i32; 5]> {
        match e.cs.get(name).map(|id| e.eqtb.meaning(id)) {
            Some(Meaning::Reg(r)) if r.region == Region::Glue => Some(glue5(e.eqtb.glue(r))),
            Some(Meaning::Reg(r)) => Some([e.eqtb.word(r), 0, 0, 0, 0]),
            _ => None,
        }
    };
    let macro_text = |e: &rushtex::engine::Engine, name: &str| -> Option<String> {
        match e.cs.get(name).map(|id| e.eqtb.meaning(id)) {
            // The text after the parameter text's end marker.
            Some(Meaning::Macro(mac)) => Some(e.toks.get(mac.text).iter().filter(|t| **t != rushtex::engine::engine_token::Token::END_MATCH).map(|t| t.chr()).collect()),
            _ => None,
        }
    };
    let run = |src: &str, inspect: &mut dyn FnMut(&mut rushtex::engine::engine_run::Machine)| -> Result<(), String> {
        rushtex::engine::engine_job::run(&|| Some(engine.fmt.clone()), src, &rushtex::engine::engine_job::Opts { job_name: "rtgen", now: 0, max_runs: 1 }, &read, &mut |_| {}, inspect).map(|_| ()).ok_or_else(|| "the format is not one of this build".to_string())
    };
    let mut notes = Vec::new();
    for (si, size) in data_class::CLASS_SIZES.iter().enumerate() {
        let doc = |opts: &str, body: &str| format!("\\documentclass[{size}{opts}]{{article}}\\begin{{document}}{body}");
        // The base and each combination: the table's words and the oracle's state.
        let mut combos: Vec<(u32, GenClass, State)> = Vec::new();
        let mut dropped = Vec::new();
        let mut hyph_note = String::new();
        for key in data_class::option_keys() {
            let mut opts = String::new();
            if key & 7 != 0 {
                opts = format!(",{}", data_class::PAPERS[(key & 7) as usize]);
            }
            for (bit, name) in [(8, "landscape"), (16, "twoside"), (32, "draft")] {
                if key & bit != 0 {
                    opts.push(',');
                    opts.push_str(name);
                }
            }
            let mut g = GenClass::default();
            let mut err = None;
            let mut want = None;
            run(&doc(&opts, ""), &mut |m| {
                let e = &*m.e;
                if e.eqtb.level() != 1 || !m.errors.is_empty() {
                    err = Some(format!("the probe ended in a group or with errors: {:?}", m.errors));
                    return;
                }
                g.ints = (0..INT_PARAMS).map(|slot| e.eqtb.word(Reg { region: Region::Int, slot })).collect();
                g.dimens = (0..DIMEN_PARAMS).map(|slot| e.eqtb.word(Reg { region: Region::Dimen, slot })).collect();
                g.glues = (0..GLUE_PARAMS).map(|slot| glue5(e.eqtb.glue(Reg { region: Region::Glue, slot }))).collect();
                g.codes = CODES.iter().flat_map(|&k| (0..256).map(move |c| e.eqtb.code(k, c))).collect();
                g.cats = e.eqtb.cats().0.to_vec();
                let cur = match e.eqtb.meaning(rushtex::engine::CUR_FONT_ID) {
                    Meaning::Font(f) => f as usize,
                    _ => 0,
                };
                for (i, f) in e.fonts.list.iter().enumerate().skip(1) {
                    let ident = e.cs.name(f.ident).to_string();
                    // expl3's `intarray`s: integers kept in the parameters of fonts loaded at a few sp.
                    if f.size < 65536 {
                        dropped.push(format!("{ident} ({} at {}sp)", f.name, f.size));
                        continue;
                    }
                    g.fonts.push((f.name.to_string(), ident, f.size, f.hyphen_char, f.skew_char, i == cur));
                }
                let reg = |name: &str| match e.cs.get(name).map(|id| e.eqtb.meaning(id)) {
                    Some(Meaning::Reg(r)) => Some(e.eqtb.word(r)),
                    _ => None,
                };
                let macro_nonempty = |name: &str| matches!(e.cs.get(name).map(|id| e.eqtb.meaning(id)), Some(Meaning::Macro(mac)) if !mac.text.is_empty());
                for name in CLASS_DIMENS {
                    match reg(name) {
                        Some(v) => g.class.push(v),
                        None => err = Some(format!("\\{name} is not a register")),
                    }
                }
                g.class.push(macro_nonempty("@textbottom") as i32);
                for name in data_class::CLASS_REGS {
                    let r = match name.split_once(' ') {
                        Some((kind, class)) => match e.cs.get(class).map(|id| e.eqtb.meaning(id)) {
                            Some(Meaning::CharDef(n)) => Some(Reg::register(
                                match kind {
                                    "count" => Region::Int,
                                    "dimen" => Region::Dimen,
                                    _ => Region::Glue,
                                },
                                n,
                            )),
                            _ => None,
                        },
                        None => match e.cs.get(name).map(|id| e.eqtb.meaning(id)) {
                            Some(Meaning::Reg(r)) if matches!(r.region, Region::Int | Region::Dimen | Region::Glue) => Some(r),
                            _ => None,
                        },
                    };
                    let Some(r) = r else {
                        err = Some(format!("{name}: not a count, dimen or skip register"));
                        return;
                    };
                    let v = match r.region {
                        Region::Glue => glue5(e.eqtb.glue(r)),
                        _ => [e.eqtb.word(r), 0, 0, 0, 0],
                    };
                    g.regs.push([r.region as i32, r.slot as i32, v[0], v[1], v[2], v[3], v[4]]);
                }
                g.class.push(matches!(e.cs.get("if@twoside").map(|id| e.eqtb.meaning(id)), Some(Meaning::Prim(Prim::IfTrue))) as i32);
                if si == 0 && key == 0 {
                    // Language 0's hyphenation (`data_hyph`), read back and compared.
                    let hyph_out = arg("--hyph-out").unwrap_or_else(|| "assets/tables/hyph-en.rtdb".into());
                    let Some(trie) = e.hyph.trie_parts(0) else {
                        err = Some("language 0 has no patterns".into());
                        return;
                    };
                    let (patterns, ex) = e.hyph.language(0);
                    let mut w = rushtex::data::data_blob::BlobWriter::default();
                    rushtex::data::data_hyph::write(&mut w, patterns, ex.iter().copied(), &trie);
                    let bytes = w.finish();
                    let back = Blob::new(&bytes).and_then(|b| rushtex::data::data_hyph::HyphTable::from_blob(&b));
                    match back {
                        Some(t) if t.patterns == patterns && t.exceptions().eq(ex.iter().copied()) && t.trie.as_ref().is_some_and(|s| s.iter().zip(&trie).all(|(s, v)| s.len() == v.len() && (0..v.len()).all(|i| s.get(i) == v[i]))) => {
                            if let Err(x) = std::fs::write(&hyph_out, &bytes) {
                                err = Some(format!("{hyph_out}: {x}"));
                                return;
                            }
                            hyph_note = format!("; language 0: {} pattern bytes, {} exceptions", patterns.len(), ex.len());
                        }
                        _ => {
                            err = Some("the hyphenation table does not read back".into());
                            return;
                        }
                    }
                    // The kernel's `\\chardef`s and `\\mathchardef`s (`data_kernel`), read back and compared.
                    let kernel_out = arg("--kernel-out").unwrap_or_else(|| "assets/tables/kernel.rtdb".into());
                    let KernelValues { chars, ops, delims, macros, fonts } = match kernel_values(e) {
                        Ok(v) => v,
                        Err(x) => {
                            err = Some(x);
                            return;
                        }
                    };
                    let macros: Vec<(String, u8, Vec<rushtex::data::data_kernel::MacroToken>)> = macros.iter().map(|(n, f, t)| (n.clone(), *f, t.iter().map(Tok::borrow).collect())).collect();
                    if std::env::var_os("RT_GEN_DUMP").is_some() {
                        for (n, f, t) in &macros {
                            eprintln!("{n} {f} {t:?}");
                        }
                    }
                    let mut w = rushtex::data::data_blob::BlobWriter::default();
                    rushtex::data::data_kernel::write(&mut w, &chars, &ops, &delims, &macros, &fonts);
                    let bytes = w.finish();
                    let back = Blob::new(&bytes).and_then(|b| rushtex::data::data_kernel::KernelTable::from_blob(&b));
                    match back {
                        Some(t) if t.chars().map(|(n, d)| (n.to_string(), d)).eq(chars.iter().cloned()) && t.ops().map(|o| (o.name.to_string(), o.word.to_string(), o.nolimits)).eq(ops.iter().cloned()) && t.delims().map(|(n, v)| (n.to_string(), v)).eq(delims.iter().cloned()) && t.macros().map(|(n, f, k)| (n.to_string(), f, k)).eq(macros.iter().map(|(n, f, k)| (n.clone(), *f, k.clone()))) && t.math_fonts().map(|f| owned_math_font(&f)).eq(fonts.iter().cloned()) => {
                            if let Err(x) = std::fs::write(&kernel_out, &bytes) {
                                err = Some(format!("{kernel_out}: {x}"));
                                return;
                            }
                            hyph_note.push_str(&format!("; {} kernel character names, {} operators, {} delimiters, {} macros, {} math fonts and alphabets", chars.len(), ops.len(), delims.len(), macros.len(), fonts.len()));
                        }
                        _ => {
                            err = Some("the kernel table does not read back".into());
                            return;
                        }
                    }
                }
                let kept: Vec<String> = g.fonts.iter().map(|f| f.1.clone()).collect();
                want = Some(state(m, &|ident: &str| kept.iter().any(|k| k == ident)));
            })?;
            if let Some(e) = err {
                return Err(format!("{size} option key {key}: {e}"));
            }
            combos.push((key, g, want.ok_or("no state")?));
        }
        let mut g = std::mem::take(&mut combos[0].1);
        // Each combination as its differences from the base (the base has none).
        g.options.push((0, Vec::new()));
        for (key, c, _) in &combos[1..] {
            if c.fonts != g.fonts || c.codes != g.codes || c.cats != g.cats {
                return Err(format!("{size} option key {key}: fonts or codes differ from the base"));
            }
            let mut d = Vec::new();
            let flat = |v: &[[i32; 5]]| v.concat();
            let regs = |v: &[[i32; 7]]| v.concat();
            for (sec, (a, b)) in [(0, (&g.ints, &c.ints)), (1, (&g.dimens, &c.dimens)), (2, (&flat(&g.glues), &flat(&c.glues))), (3, (&g.class, &c.class)), (4, (&regs(&g.regs), &regs(&c.regs)))] {
                for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
                    if x != y {
                        d.push([sec, i as u32, *y as u32]);
                    }
                }
            }
            g.options.push((*key, d));
        }
        // The size commands, each after the quantities it may set are made 1sp.
        let one = [1, 0, 0, 0, 0];
        let skips = ["abovedisplayskip", "abovedisplayshortskip", "belowdisplayshortskip", "belowdisplayskip"];
        let listglues = ["topsep", "parsep", "itemsep", "partopsep"];
        let ones = |names: &[&str]| names.iter().map(|n| format!("\\{n}=1sp ")).collect::<String>();
        let mask = |v: &[[i32; 5]]| v.iter().enumerate().filter(|(_, x)| **x != one).fold(0u32, |m, (k, _)| m | 1 << k);
        for cmd in SIZE_COMMANDS {
            let body = format!("{}\\expandafter\\def\\csname @listi\\endcsname{{}}\\{cmd} {}\\csname @listi\\endcsname $\\relax$", ones(&skips), ones(&listglues));
            let mut row = None;
            let mut err = None;
            run(&doc("", &body), &mut |m| {
                let e = &*m.e;
                let get = |n: &str| named(e, n).or_else(|| rushtex::engine::engine_names::param(n).map(|r| glue5(e.eqtb.glue(r))));
                let (Some(f), Some(sf), Some(ssf)) = (macro_text(e, "f@size"), macro_text(e, "sf@size"), macro_text(e, "ssf@size")) else {
                    err = Some(format!("\\{cmd}: no \\f@size, \\sf@size or \\ssf@size"));
                    return;
                };
                let disp: Vec<[i32; 5]> = skips.iter().map(|n| get(n).unwrap_or(one)).collect();
                let list: Vec<[i32; 5]> = listglues.iter().map(|n| get(n).unwrap_or(one)).collect();
                let Some(bskip) = get("baselineskip") else { return };
                let mut w = bskip.to_vec();
                w.push(mask(&disp) as i32);
                w.extend(disp.concat());
                // No `\@listi` of its own: the 1sp values stay.
                w.push(mask(&list) as i32);
                w.extend(list.concat());
                row = Some(([f, sf, ssf], w));
            })?;
            match (row, err) {
                (Some(r), None) if r.1.len() == SIZE_ROW => g.sizes.push(r),
                (_, e) => return Err(format!("{size} \\{cmd}: {}", e.unwrap_or_else(|| "no row".into()))),
            }
        }
        for depth in ["ii", "iii", "iv", "v", "vi"] {
            let body = format!("{}\\csname @list{depth}\\endcsname", ones(&listglues));
            let mut row = None;
            run(&doc("", &body), &mut |m| {
                let list: Vec<[i32; 5]> = listglues.iter().map(|n| named(m.e, n).unwrap_or(one)).collect();
                row = Some((mask(&list), list));
            })?;
            let (k, list) = row.ok_or("no list row")?;
            g.lists.push(k as i32);
            g.lists.extend(list.concat());
        }
        // The table, written, read back, and loaded per combination by `load_class`.
        let out = if si == 0 { format!("{dir}/class-article.rtdb") } else { format!("{dir}/class-article-{size}.rtdb") };
        let mut w = rushtex::data::data_blob::BlobWriter::default();
        data_class::write(&mut w, &g);
        let bytes: &'static [u8] = Box::leak(w.finish().into_boxed_slice());
        std::fs::write(&out, bytes).map_err(|x| format!("{out}: {x}"))?;
        let table = Blob::new(bytes).and_then(|b| data_class::ClassTable::from_blob(&b)).ok_or("the written table does not read back")?;
        let table: &'static data_class::ClassTable<'static> = Box::leak(Box::new(table));
        for (key, c, want) in &combos {
            let mut fresh = rushtex::engine::Engine::default();
            let mut n = rushtex::engine::engine_run::Machine::new(&mut fresh, "");
            n.load_class_with(table, *key).map_err(|x| format!("{size} key {key}: {x}"))?;
            let kept: Vec<String> = g.fonts.iter().map(|f| f.1.clone()).collect();
            let got = state(&n, &|ident: &str| kept.iter().any(|k| k == ident));
            let mut diffs = Vec::new();
            let read_reg = |eq: &rushtex::engine::engine_eqtb::Eqtb, r: &[i32; 7]| {
                let reg = Reg { region: [Region::Int, Region::Dimen, Region::Glue][r[0] as usize], slot: r[1] as u32 };
                match reg.region {
                    Region::Glue => glue5(eq.glue(reg)),
                    _ => [eq.word(reg), 0, 0, 0, 0],
                }
            };
            let regs = if *key == 0 { &g.regs } else { &c.regs };
            let bad: Vec<&str> = regs.iter().zip(data_class::CLASS_REGS).filter(|(r, _)| read_reg(&n.e.eqtb, r) != [r[2], r[3], r[4], r[5], r[6]]).map(|(_, name)| name).collect();
            if !bad.is_empty() {
                diffs.push(format!("registers {bad:?}"));
            }
            if got.0 != want.0 {
                diffs.push(format!("{} integer, dimension or code entries", got.0.iter().zip(&want.0).filter(|(a, b)| a != b).count()));
            }
            if got.1 != want.1 {
                diffs.push("glue parameters".into());
            }
            if got.2 != want.2 {
                diffs.push("catcodes".into());
            }
            let mut gf = got.3.clone();
            let mut wf = want.3.clone();
            gf.sort();
            wf.sort();
            if gf != wf {
                diffs.push(format!("fonts: got {:?}, want {:?}", gf.iter().map(|f| (&f.0, f.2, f.3, f.4)).collect::<Vec<_>>(), wf.iter().map(|f| (&f.0, f.2, f.3, f.4)).collect::<Vec<_>>()));
            }
            let class = if *key == 0 { &g.class } else { &c.class };
            if n.class_values() != Some(class.clone()) {
                diffs.push("class dimensions or flags".into());
            }
            if !diffs.is_empty() {
                return Err(format!("{size} key {key}: the table differs from the oracle's state: {}", diffs.join("; ")));
            }
        }
        notes.push(format!(
            "{size}: {} fonts, {} combinations ({} differing words), every parameter, code, catcode, register and font equal to the oracle's; dropped {} intarray fonts{hyph_note}",
            g.fonts.len(),
            combos.len(),
            g.options.iter().map(|o| o.1.len()).sum::<usize>(),
            dropped.len() / combos.len().max(1)
        ));
    }
    Ok(notes.join("\n"))
}

/// `rushtex-gen names [--out PATH]`: the name table the native pre-scan asks (`table_names`),
/// a perfect hash from every name the kernel and package tables give to the bits of the tables
/// giving it. It is derived from the committed tables alone, so run it after `class` and
/// `natpkg`; a unit test fails while it is stale.
fn names(args: &[String]) -> Result<String, String> {
    let out = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| "assets/tables/names.rtdb".into());
    let blob = rushtex::engine::engine_doc::names_table();
    std::fs::write(&out, &blob).map_err(|e| format!("{out}: {e}"))?;
    Ok(format!("{out}: {} bytes", blob.len()))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("fonts") => fonts(&args[1..]),
        #[cfg(feature = "oracle")]
        Some("nfss") => nfss(&args[1..]),
        Some("embed") => embed(&args[1..]),
        Some("names") => names(&args[1..]),
        #[cfg(feature = "oracle")]
        Some("class") => class(&args[1..]),
        #[cfg(feature = "oracle")]
        Some("pkgs") => pkgs(&args[1..]),
        #[cfg(feature = "oracle")]
        Some("measure") => measure(&args[1..]),
        #[cfg(feature = "oracle")]
        Some("natpkg") => natpkg(&args[1..]),
        _ => Err("usage: rushtex-gen fonts --list FILE --use USE [--out PATH] [--check] | nfss --list FILE [--format FMT] [--out PATH] [--dump] | pkgs --list FILE [--format FMT] [--out PATH] | embed --list FILE [--set PATH] [--out PATH] | class [--format FMT] [--out-dir DIR] [--hyph-out PATH] [--kernel-out PATH] | measure --pkgs FILE --corpus DIR [--format FMT] [--top N] | natpkg NAME.. [--format FMT] [--out-dir DIR] | names [--out PATH]".into()),
    };
    match result {
        Ok(msg) => {
            println!("{msg}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("rushtex-gen: {e}");
            ExitCode::FAILURE
        }
    }
}
