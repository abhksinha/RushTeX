//! `rushtex-gen`: the build-time table generator (pillar 2). Dev only (`--features gen`).
//!
//!     rushtex-gen fonts --list FILE --use USE [--out assets/tables/fonts.rtdb]   # write, self-check
//!     rushtex-gen fonts --list FILE --use USE --check                           # self-check only
//!     rushtex-gen nfss --list FILE [--format FMT] [--out assets/tables/nfss.rtdb] [--dump]
//!     rushtex-gen pkgs --list FILE [--format FMT] [--out assets/tables/pkgs.rtdb]
//!     rushtex-gen embed --list FILE [--set assets/tables/embed.set] [--out assets/tables/embed.rtdb]
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

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("fonts") => fonts(&args[1..]),
        #[cfg(feature = "oracle")]
        Some("nfss") => nfss(&args[1..]),
        Some("embed") => embed(&args[1..]),
        #[cfg(feature = "oracle")]
        Some("pkgs") => pkgs(&args[1..]),
        _ => Err("usage: rushtex-gen fonts --list FILE --use USE [--out PATH] [--check] | nfss --list FILE [--format FMT] [--out PATH] [--dump] | pkgs --list FILE [--format FMT] [--out PATH] | embed --list FILE [--set PATH] [--out PATH]".into()),
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
