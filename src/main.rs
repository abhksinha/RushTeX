//! # Command-Line Interface Driver (`main`)
//!
//! Two paths compile a document: the native job (`engine_doc`, guarded direct lowering),
//! and, when its pre-scan declines the document, the TeX engine on the LaTeX format, as
//! `pdflatex` runs it (needs TeX Live).
//!
//! ```bash
//! rushtex input.tex [-o output.pdf]
//! rushtex --watch input.tex [-o output.pdf]   # recompile on every save
//! ```

#![forbid(unsafe_code)]

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    // `rushtex fonts ...`: the font packs (native-architecture.md §4b).
    if env::args().nth(1).as_deref() == Some("fonts") {
        return fonts_command(&env::args().skip(2).collect::<Vec<_>>());
    }
    let mut cli_arguments = env::args().skip(1);
    let mut input_file_path: Option<PathBuf> = None;
    let mut output_file_path: Option<PathBuf> = None;
    let mut native_gaps = false;
    let mut watch = false;
    let mut replay = false;
    let mut format_path: Option<PathBuf> = None;

    while let Some(argument) = cli_arguments.next() {
        match argument.as_str() {
            "-o" | "--output" => match cli_arguments.next() {
                Some(path_str) => output_file_path = Some(PathBuf::from(path_str)),
                None => return usage("-o needs the path of the PDF to write, as in: rushtex paper.tex -o out/paper.pdf"),
            },
            "--watch" => watch = true,
            // Not in the help: the edit-fuzz test's session (`watch_replay`).
            "--watch-replay" => replay = true,
            "--fmt" => match cli_arguments.next() {
                Some(path_str) => format_path = Some(PathBuf::from(path_str)),
                None => return usage("--fmt needs the path of a LaTeX format file, as in: rushtex paper.tex --fmt pdflatex.rfmt"),
            },
            "--native-gaps" => native_gaps = true,
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            flag if flag.starts_with('-') => return usage(&format!("There is no option {flag}. Run rushtex --help to see the options.")),
            positional_arg => {
                if let Some(first) = &input_file_path {
                    return usage(&format!("RushTeX compiles one .tex file at a time, but was given two: '{}' and '{positional_arg}'. A path with spaces needs quotes.", first.display()));
                }
                input_file_path = Some(PathBuf::from(positional_arg));
            }
        }
    }

    let Some(input_path) = input_file_path else {
        print_help();
        return ExitCode::from(2);
    };

    if native_gaps {
        // One `char` per byte, as the job reads the file (its UTF-8 is read byte by byte).
        return match std::fs::read(&input_path) {
            Ok(bytes) => {
                let source_content: String = bytes.iter().map(|&b| char::from(b)).collect();
                let bbl = std::fs::read(input_path.with_extension("bbl")).ok().map(|b| b.iter().map(|&b| char::from(b)).collect::<String>());
                let source_content = rushtex::engine::engine_doc::splice_bibliography(&source_content, bbl.as_deref());
                for what in rushtex::engine::engine_doc::native_blockers(&source_content) {
                    println!("{what}");
                }
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("{}", explain(&format!("The input file '{}' cannot be read: {err}.", input_path.display())));
                ExitCode::from(1)
            }
        };
    }

    let output = output_file_path.unwrap_or_else(|| input_path.with_extension("pdf"));
    if let Err(message) = check_paths(&input_path, &output) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    if watch {
        return watch_and_compile(&input_path, &output, format_path.as_deref());
    }
    if replay {
        return watch_replay(&input_path, &output, format_path.as_deref());
    }
    compile(&input_path, &output, format_path.as_deref(), None)
}

/// How often `--watch` looks at the source's modification time and size: a look is one
/// `stat`, so the delay a save waits for is kept small.
#[cfg(not(target_arch = "wasm32"))]
const WATCH_POLL: std::time::Duration = std::time::Duration::from_millis(5);
/// How long a changed source must stay unchanged before it is compiled: an editor that
/// truncates the file and then writes it is not compiled half written.
#[cfg(not(target_arch = "wasm32"))]
const WATCH_SETTLE: std::time::Duration = std::time::Duration::from_millis(2);

/// How long the watched files must stay unchanged after a compile before the idle check
/// compiles the document afresh in the background and compares the PDFs.
#[cfg(not(target_arch = "wasm32"))]
const WATCH_IDLE: std::time::Duration = std::time::Duration::from_secs(1);

/// What `--watch` keeps between compiles: what does not depend on the document, each part
/// made again when what it was made from changes, and the native job's PDF work keyed by
/// the bytes it was made from, so every compile's PDF is the one a new `rushtex` process
/// would write. The native job's tables are process-wide already (the `OnceLock`s of
/// `data_*`); the engine's file search (its `ls-R` index) and its format, undumped, are kept
/// here, and the native job's checkpoints of the last compile (used only while what they
/// depend on is unchanged).
#[derive(Default)]
struct Warm {
    /// Fonts subset, page streams compressed and images decoded by earlier compiles, by a
    /// hash of their inputs (`backend_cache`).
    pdf: rushtex::backend::backend_cache::PdfCache,
    /// The files beside the document the last compile read (figures, `\input` files, the
    /// `.bbl`): a change to one compiles again, as a change to the source does.
    deps: std::cell::RefCell<Vec<PathBuf>>,
    /// The native job's checkpoints of the last compile, to start the next one from the last
    /// checkpoint before the first changed byte (`engine_doc::run_session`).
    native: std::cell::RefCell<rushtex::engine::engine_doc::Session>,
    /// The PDF the last compile wrote, for the idle check (`watch_and_compile`).
    written: Option<Vec<u8>>,
    #[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
    kpse: std::cell::OnceCell<Option<rushtex::engine::engine_host::Kpse>>,
    /// The format undumped, with its file and that file's size and modification time: each
    /// engine run starts from a copy (`engine_job::run_from`) instead of an undump.
    #[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
    format: std::cell::RefCell<Option<KeptFormat>>,
    /// The last compile's engine runs, each copied where its preamble ended (and one where
    /// `\document` reads the `.aux`): a run whose preamble reads the same bytes and files
    /// starts there (`engine_job::run_kept`).
    #[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
    runs: std::cell::RefCell<rushtex::engine::engine_resume::Kept>,
}

#[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
type KeptFormat = (PathBuf, Option<(u64, std::time::SystemTime)>, rushtex::engine::Engine);

/// Compiles `input` to `output` now and again whenever it or a file beside it that the last
/// compile read changes, until killed.
///
/// The idle check, the safety net of a session that keeps state between compiles: once the
/// watched files have stayed unchanged for [`WATCH_IDLE`] after a compile, a thread compiles
/// the document afresh (`compile` with no session, as a new `rushtex` process does) and the
/// PDF is compared with the one written. A mismatch writes the fresh PDF and says where the
/// two first differ. A change that comes meanwhile is compiled at once: the check's result is
/// then dropped (a job cannot be stopped from outside, so its thread runs on to its end, and
/// no new check starts before it has).
#[cfg(not(target_arch = "wasm32"))]
fn watch_and_compile(input: &Path, output: &Path, format_path: Option<&Path>) -> ExitCode {
    let stamps = |files: &[PathBuf]| files.iter().map(|f| std::fs::metadata(f).ok().map(|m| (m.modified().ok(), m.len()))).collect::<Vec<_>>();
    let mut files = vec![input.to_path_buf()];
    let mut seen = Vec::new();
    let mut warm = Warm::default();
    // The compiles made (each a generation), when the last one ended, and the idle check
    // running, with the generation it checks.
    let mut generation = 0u64;
    let mut idle_from: Option<std::time::Instant> = None;
    let mut check: Option<(std::thread::JoinHandle<Option<Vec<u8>>>, u64)> = None;
    eprintln!("[WATCH] {} -> {} (Ctrl-C to stop)", input.display(), output.display());
    loop {
        let mut now = stamps(&files);
        if now[0].is_some() && now == seen {
            if check.as_ref().is_some_and(|(t, _)| t.is_finished()) {
                let (thread, checked) = check.take().expect("checked above");
                let fresh = thread.join().ok().flatten();
                // Only for the PDF on disk, and only if no file changed since its compile.
                if let (Some(fresh), Some(written)) = (fresh, warm.written.as_ref()) {
                    if checked == generation && fresh != *written && stamps(&files) == seen {
                        let at = first_difference(written, &fresh);
                        match write_pdf(output, &fresh) {
                            Ok(()) => eprintln!("[WARN] The PDF this session wrote differed from a fresh compile (first at {at}); the fresh PDF is written now."),
                            Err(err) => eprintln!("{err}"),
                        }
                        warm.written = Some(fresh);
                    }
                }
            }
            if check.is_none() && idle_from.is_some_and(|t| t.elapsed() >= WATCH_IDLE) {
                idle_from = None;
                if warm.written.is_some() {
                    let (input, format_path) = (input.to_path_buf(), format_path.map(Path::to_path_buf));
                    // A stack as large as the main thread's, which the one-shot compile runs on.
                    let thread = std::thread::Builder::new().stack_size(16 << 20).spawn(move || build(&input, format_path.as_deref(), None, std::time::Instant::now()).ok().map(|b| b.pdf));
                    check = thread.ok().map(|t| (t, generation));
                }
            }
        }
        if now[0].is_some() && now != seen {
            loop {
                std::thread::sleep(WATCH_SETTLE);
                let again = stamps(&files);
                if again == now {
                    break;
                }
                now = again;
            }
            // An editor that saves to a new file and renames it over the source leaves no
            // source for a moment: it is looked at again, not compiled.
            if now[0].is_none() {
                continue;
            }
            compile(input, output, format_path, Some(&mut warm));
            generation += 1;
            idle_from = Some(std::time::Instant::now());
            // The files read beside the document: by name (`local`), or found there by the
            // engine's file search.
            files.truncate(1);
            files.extend(warm.deps.take());
            #[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
            if let Some(k) = warm.kpse.get().and_then(Option::as_ref) {
                let dir = input.parent().filter(|d| !d.as_os_str().is_empty());
                files.extend(k.found_paths().into_iter().filter(|p| p.is_relative() || dir.is_some_and(|d| p.starts_with(d))));
            }
            files[1..].sort();
            files.dedup();
            files.retain(|f| f != output);
            // The source as it was when compiled: a save during the compile compiles again.
            seen = std::iter::once(now[0]).chain(stamps(&files[1..])).collect();
        }
        std::thread::sleep(WATCH_POLL);
    }
}

#[cfg(target_arch = "wasm32")]
fn watch_and_compile(_input: &Path, _output: &Path, _format_path: Option<&Path>) -> ExitCode {
    usage("--watch needs a file system, which this build of RushTeX does not have.")
}

/// `rushtex --watch-replay input.tex -o out.pdf`, for the edit-fuzz test (`parity-testing
/// incr`): a watch session driven from standard input instead of the files' times. Each line
/// read compiles the files as they are now, with the session's state (the compile `--watch`
/// makes), then prints `compiled` on standard output; the session ends with its input.
fn watch_replay(input: &Path, output: &Path, format_path: Option<&Path>) -> ExitCode {
    use std::io::{BufRead as _, Write as _};
    let mut warm = Warm::default();
    for line in std::io::stdin().lock().lines() {
        if line.is_err() {
            break;
        }
        compile(input, output, format_path, Some(&mut warm));
        warm.deps.take();
        let mut out = std::io::stdout().lock();
        if writeln!(out, "compiled").and_then(|()| out.flush()).is_err() {
            break;
        }
    }
    ExitCode::SUCCESS
}

/// Where two PDFs first differ, in words: the object (`N 0 obj`) holding the first differing
/// byte of `a`, or the part of the file before the first object or after the last.
#[cfg(not(target_arch = "wasm32"))]
fn first_difference(a: &[u8], b: &[u8]) -> String {
    let at = a.iter().zip(b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
    let head = &a[..at];
    let Some(obj) = head.windows(6).rposition(|w| w == b" 0 obj") else { return format!("byte {at}, before the first object") };
    let start = head[..obj].iter().rposition(|c| !c.is_ascii_digit()).map_or(0, |i| i + 1);
    let number = String::from_utf8_lossy(&head[start..obj]);
    if head[obj..].windows(6).any(|w| w == b"endobj") {
        return format!("byte {at}, after object {number}");
    }
    format!("object {number}, byte {at}")
}

/// The document compiled: the native job (`engine_doc`: no format, no TeX files, one pass)
/// where its pre-scan accepts it, else the TeX engine on the LaTeX format as `pdflatex` runs
/// it (files found as kpathsea finds them, run again while its `.aux` and `.toc` change; the
/// format made from `pdflatex.ini` on first use and kept in the cache directory). The engine
/// needs TeX Live; without it, a document the native job declines is an error saying why.
/// `warm` is what a `--watch` session keeps between compiles ([`Warm`]); a one-shot run has
/// none.
fn compile(input: &Path, output: &Path, format_path: Option<&Path>, mut warm: Option<&mut Warm>) -> ExitCode {
    let start_time = std::time::Instant::now();
    let built = match build(input, format_path, warm.as_deref_mut(), start_time) {
        Ok(b) => b,
        Err(code) => {
            if let Some(w) = warm {
                w.written = None;
            }
            return code;
        }
    };
    let (code, written) = finish(output, &built.pdf, &built.report, built.failed, built.pages, start_time, &built.path);
    // What a session wrote, for its idle check (`watch_and_compile`).
    if let Some(w) = warm {
        w.written = written.then_some(built.pdf);
    }
    code
}

/// A compiled document: the PDF, the job's errors explained, its pages and which path ran.
struct Built {
    pdf: Vec<u8>,
    /// Whether the job had errors, and what is printed about them (`Seen::report`).
    failed: bool,
    report: String,
    pages: usize,
    path: String,
}

/// The document compiled as [`compile`] says, its PDF not yet written; a failure has been
/// reported and gives the exit code.
fn build(input: &Path, format_path: Option<&Path>, warm: Option<&mut Warm>, start_time: std::time::Instant) -> Result<Built, ExitCode> {
    let doc_dir = input.parent().filter(|d| !d.as_os_str().is_empty()).map(Path::to_path_buf);
    // The document's own files (`.bbl`, figures) are found beside it first, as `Kpse::find`
    // looks there first: read so, they cost no `ls-R` index.
    // A kept search is renewed (`Kpse::renew`), or made again if an `ls-R` database changed.
    let warm: Option<&Warm> = warm.map(|w| {
        #[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
        if !w.kpse.get().is_some_and(|k| k.as_ref().is_some_and(|k| k.renew())) {
            w.kpse = std::cell::OnceCell::new();
        }
        &*w
    });
    // A watch session notes each file read so (`Warm::deps`).
    let local = |name: &str| {
        let path = (!name.starts_with('/')).then(|| doc_dir.as_deref().unwrap_or(Path::new(".")).join(name)).filter(|p| p.is_file())?;
        let bytes = std::fs::read(&path).ok();
        if let Some(w) = warm {
            w.deps.borrow_mut().push(path);
        }
        bytes
    };
    #[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
    let cache = cache_dir();
    // The file search is set up on first use: the native job reads no TeX file unless its
    // font files are not the ones the binary carries.
    #[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
    let own_kpse = std::cell::OnceCell::new();
    #[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
    let kpse_cell = warm.map_or(&own_kpse, |w| &w.kpse);
    #[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
    let kpse = || kpse_cell.get_or_init(|| rushtex::engine::engine_host::Kpse::new("pdflatex", cache.as_ref().map(|c| c.join("kpse-paths")).as_deref(), doc_dir.clone())).as_ref();
    // `RT_READS=1`: each file the job asks for, on standard error.
    let read = |name: &str| {
        if std::env::var_os("RT_READS").is_some() {
            eprintln!("read {name}");
        }
        if let Some(b) = local(name) {
            return Some(b);
        }
        #[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
        return kpse()?.read(name);
        #[cfg(not(all(feature = "oracle", not(target_arch = "wasm32"))))]
        None
    };
    let source: String = match std::fs::read(input) {
        // pdfTeX reads each byte as one character code.
        Ok(bytes) => bytes.iter().map(|&b| char::from(b)).collect(),
        Err(err) => {
            eprintln!("{}", explain(&format!("The input file '{}' cannot be read: {err}.", input.display())));
            return Err(ExitCode::from(1));
        }
    };
    let job_name = input.file_stem().and_then(|s| s.to_str()).unwrap_or("texput");
    let now = rushtex::engine::engine_doc::run_date();
    // `RT_ENGINE=1` runs the engine on the format instead, as the oracle.
    let why = if std::env::var_os("RT_ENGINE").is_some() {
        "RT_ENGINE=1".to_string()
    } else {
        // `RT_TIME=1`: the job's phases (`engine_doc::run`).
        if std::env::var_os("RT_TIME").is_some() {
            eprintln!("{:8.3} ms  before the job (paths, source)", start_time.elapsed().as_secs_f64() * 1e3);
        }
        // `\bibliography` reads `\jobname.bbl` where the engine's file search finds it.
        let bbl = if source.contains("\\bibliography") { read(&format!("{job_name}.bbl")).map(|b| b.iter().map(|&b| char::from(b)).collect::<String>()) } else { None };
        let spliced = rushtex::engine::engine_doc::splice_bibliography(&source, bbl.as_deref());
        let run = match warm {
            Some(w) => {
                // Where `read` finds a file, by presence alone (`engine_doc::run_session`): a
                // file found beside the document is still one the watch looks at.
                let locate = |name: &str| {
                    if let Some(p) = (!name.starts_with('/')).then(|| doc_dir.as_deref().unwrap_or(Path::new(".")).join(name)).filter(|p| p.is_file()) {
                        w.deps.borrow_mut().push(p.clone());
                        return Some(p);
                    }
                    #[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
                    return kpse()?.locate(name);
                    #[cfg(not(all(feature = "oracle", not(target_arch = "wasm32"))))]
                    None
                };
                rushtex::engine::engine_doc::run_session(&spliced, now, Some(&read), Some(&locate), Some(&w.pdf), &mut w.native.borrow_mut())
            }
            None => rushtex::engine::engine_doc::run(&spliced, now, Some(&read)),
        };
        match run {
            // `RT_NATIVE_WHY=1`: which path ran, for the testing scripts.
            // A TeX error makes the native job decline (`engine_doc::run`): it has none to report.
            Ok(job) => return Ok(Built { pdf: job.pdf, failed: !job.errors.is_empty(), report: String::new(), pages: job.pages, path: "native job accepted".to_string() }),
            Err(why) => {
                // `RT_NATIVE_WHY=1`: why the document runs on the engine.
                if std::env::var_os("RT_NATIVE_WHY").is_some() {
                    eprintln!("native job declined: {why}");
                }
                why
            }
        }
    };
    #[cfg(not(all(feature = "oracle", not(target_arch = "wasm32"))))]
    {
        let _ = (format_path, why);
        eprintln!("{}", explain(&format!("This document uses something RushTeX cannot compile on its own yet, and this build of RushTeX has no TeX engine to fall back on. rushtex --native-gaps {} lists what.", input.display())));
        Err(ExitCode::from(1))
    }
    #[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
    {
        use rushtex::engine::{engine_host, engine_job};
        let Some(fmt_path) = format_path.map(Path::to_path_buf).or_else(|| cache.as_ref().map(|c| c.join("pdflatex.rfmt"))) else {
            let _ = why;
            eprintln!("{}", explain("This document uses something RushTeX cannot compile on its own yet, so it runs on TeX Live's LaTeX, whose format RushTeX keeps in a cache folder, but there is none: set RUSHTEX_CACHE or HOME, or give the format with --fmt."));
            return Err(ExitCode::from(1));
        };
        if kpse().is_none() {
            eprintln!("{}", explain(&format!("This document uses something RushTeX cannot compile on its own yet, so it needs TeX Live, which was not found (kpsewhich is not on the PATH). Install TeX Live, or leave out what RushTeX does not take yet: rushtex --native-gaps {} lists it.", input.display())));
            return Err(ExitCode::from(1));
        }
        // The format is read again for each run (from the OS cache) and dropped once
        // undumped; only a format that could not be written to the cache is kept in memory
        // (and, in a `--watch` session, the format undumped: `Warm`).
        let mut unsaved: Option<Vec<u8>> = None;
        // What the last run leaves for the error report (`inspect`).
        let mut seen: Option<Seen> = None;
        // A missing format, or one an older build dumped, is made again (once).
        for attempt in 0..2 {
            let load = || unsaved.clone().or_else(|| std::fs::read(&fmt_path).ok());
            // `RT_WRITTEN=<dir>`: the files the last run wrote (`.aux`, `.toc`, …), for the
            // P2b inventory of what LaTeX carries between runs.
            let mut inspect = |m: &mut rushtex::engine::Machine| {
                seen = Some(Seen::of(m));
                if let Ok(dir) = std::env::var("RT_WRITTEN") {
                    for w in &m.closed {
                        let name = Path::new(&w.name).file_name().map_or(w.name.as_str(), |n| n.to_str().unwrap_or(""));
                        let _ = std::fs::write(Path::new(&dir).join(name), &w.text);
                    }
                }
            };
            let opts = engine_job::Opts { job_name, now, max_runs: engine_job::MAX_RUNS };
            let job = match warm {
                // A session undumps the format once, and again only when its file changes.
                Some(w) => {
                    let mut kept = w.format.borrow_mut();
                    let stamp = std::fs::metadata(&fmt_path).ok().and_then(|m| Some((m.len(), m.modified().ok()?)));
                    let mut runs = w.runs.borrow_mut();
                    if !kept.as_ref().is_some_and(|(p, s, _)| *p == fmt_path && *s == stamp) {
                        *kept = load().and_then(|b| rushtex::engine::Engine::undump(&b)).map(|e| (fmt_path.clone(), stamp, e));
                        runs.clear();
                    }
                    // Where `read` finds a file, by presence alone, so the copies' keys re-read
                    // only files whose stamp changed (`engine_resume::hash_stamped`).
                    let locate = |name: &str| (!name.starts_with('/')).then(|| doc_dir.as_deref().unwrap_or(Path::new(".")).join(name)).filter(|p| p.is_file()).or_else(|| kpse()?.locate(name));
                    kept.as_ref().and_then(|(_, _, e)| engine_job::run_kept(&|| Some(e.clone()), &source, &opts, &read, &locate, &mut inspect, &mut runs))
                }
                None => engine_job::run(&load, &source, &opts, &read, &mut |_| {}, &mut inspect),
            };
            if let Some(job) = job {
                let report = seen.take().map_or_else(String::new, |s| s.report(&job.errors, input, &source));
                return Ok(Built { path: format!("engine job: {} runs", job.runs), pdf: job.pdf, failed: !job.errors.is_empty(), report, pages: job.pages });
            }
            if attempt == 1 {
                break;
            }
            let Some(ini) = kpse().and_then(|k| k.read("pdflatex.ini")) else { break };
            let ini: String = ini.iter().map(|&b| char::from(b)).collect();
            // Exactness is proven against one pinned kernel; another one is used, but said so.
            let (pin_date, pin_pl) = engine_host::PINNED_LATEX;
            if let Some((date, pl)) = kpse().and_then(|k| k.read("latex.ltx")).as_deref().and_then(engine_host::latex_version) {
                if (date.as_str(), pl) != (pin_date, pin_pl) {
                    eprintln!("[WARN] LaTeX {date} patch level {pl} is installed; RushTeX's output is proven identical to pdfLaTeX for LaTeX {pin_date} patch level {pin_pl} (TeX Live {})", engine_host::PINNED_TEXLIVE);
                }
            }
            eprintln!("[INFO] Making the LaTeX format {} (once)", fmt_path.display());
            let Some(built) = engine_job::build_format(&ini, "pdflatex", now, &read) else { break };
            if let Some(dir) = fmt_path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if std::fs::write(&fmt_path, &built).is_err() {
                unsaved = Some(built);
            }
        }
        eprintln!("{}", explain(&format!("RushTeX could not make the LaTeX format {} from TeX Live's pdflatex.ini. Check that TeX Live is installed whole (kpsewhich pdflatex.ini finds it), or give a format with --fmt.", fmt_path.display())));
        Err(ExitCode::from(1))
    }
}

/// The job's PDF written, then its error report, the path that ran (`RT_NATIVE_WHY=1`) and
/// the line a finished compilation prints. The PDF comes first: a reader that stops reading
/// standard error (`| grep -m1`) must not keep it from being written. Also says whether the
/// PDF was written.
fn finish(output: &Path, pdf: &[u8], report: &str, failed: bool, pages: usize, start_time: std::time::Instant, path: &str) -> (ExitCode, bool) {
    if let Err(err) = write_pdf(output, pdf) {
        eprintln!("{err}");
        return (ExitCode::from(1), false);
    }
    eprint!("{report}");
    if std::env::var_os("RT_NATIVE_WHY").is_some() {
        eprintln!("{path}");
    }
    compiled(output, pages, start_time.elapsed().as_secs_f64());
    (if failed { ExitCode::from(1) } else { ExitCode::SUCCESS }, true)
}

/// What the engine's last run leaves for the error report: its terminal text, the boxes TeX
/// reports as underfull and overfull, and, when it has errors, the commands it had defined
/// (two or more letters, sorted), which a misspelt command is compared with.
#[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
struct Seen {
    log: String,
    box_warnings: [u32; 2],
    defined: Vec<String>,
}

#[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
impl Seen {
    fn of(m: &rushtex::engine::Machine) -> Seen {
        use rushtex::engine::engine_prim::Meaning;
        let mut defined = Vec::new();
        if !m.errors.is_empty() {
            for id in 0..m.e.cs.len() as u32 {
                let name = m.e.cs.name(id);
                if name.len() > 1 && name.bytes().all(|c| c.is_ascii_alphabetic()) && !matches!(m.e.eqtb.meaning(id), Meaning::Unset | Meaning::Undefined) {
                    defined.push(name.to_string());
                }
            }
            defined.sort_unstable();
            defined.dedup();
        }
        Seen { log: m.log.clone(), box_warnings: m.box_warnings, defined }
    }

    /// The errors in plain words (`engine_explain`) and the warnings worth a line; with
    /// `RT_TEX_LOG=1`, the run's terminal text and each error in TeX's words instead
    /// (`! l.<line> [<file depth>] <message>`, which the testing framework reads).
    fn report(&self, errors: &[String], input: &Path, source: &str) -> String {
        use rushtex::engine::engine_explain;
        if std::env::var_os("RT_TEX_LOG").is_some() {
            let mut out = self.log.clone();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            for err in errors {
                out += &format!("! {err}\n");
            }
            return out;
        }
        let dir = input.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        let (file, dir) = (input.display().to_string(), dir.display().to_string());
        let run = engine_explain::Run { file: &file, dir: &dir, source, log: &self.log, defined: &self.defined };
        engine_explain::report(errors, &run) + &engine_explain::warnings(&self.log, self.box_warnings)
    }
}

/// `RUSHTEX_CACHE`, else `$XDG_CACHE_HOME/rushtex`, else `~/.cache/rushtex`: where the
/// format and the search paths are kept between runs.
#[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
fn cache_dir() -> Option<PathBuf> {
    if let Some(d) = env::var_os("RUSHTEX_CACHE") {
        return Some(PathBuf::from(d));
    }
    if let Some(d) = env::var_os("XDG_CACHE_HOME").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d).join("rushtex"));
    }
    env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache").join("rushtex"))
}

fn print_help() {
    eprintln!(
        "\
rushtex — LaTeX to PDF, identical to pdfLaTeX, in pure safe Rust

USAGE:
  rushtex <file.tex> [-o out.pdf]
  rushtex --watch <file.tex> [-o out.pdf]
  rushtex fonts list | install --from <dir>

A document compiles natively where RushTeX takes everything it uses; otherwise it runs
whole on the TeX engine on the LaTeX format, as pdflatex does (needs TeX Live).

OPTIONS:
  -o, --output <path>    Output PDF file path (default: <file>.pdf)
  --fmt <path>           The LaTeX format the engine runs on (default: made once from
                         pdflatex.ini into ~/.cache/rushtex/pdflatex.rfmt)
  --watch                Recompile whenever the file changes
  --native-gaps          List everything the native job does not take yet
  -h, --help             Display this help message

Errors are explained in plain words, each with its line and a hint, and TeX's own
message last. RT_TEX_LOG=1 prints TeX's messages alone, as pdflatex words them."
    );
}

/// An error explained in plain words, between the compiler's log banners.
fn explain(message: &str) -> String {
    format!("==========================================\nRUSHTEX - THE ULTRA FAST TEX FILE COMPILER\n==========================================\n[ERROR] {message}\n==========END OF COMPILER LOGS============")
}

/// A command-line mistake explained; exit code 2.
fn usage(message: &str) -> ExitCode {
    eprintln!("{}", explain(message));
    ExitCode::from(2)
}

/// Before any work: the input file must exist, and the folder the PDF goes into must
/// exist or be made. Each failure says what is wrong and what to do.
fn check_paths(input: &std::path::Path, output: &std::path::Path) -> Result<(), String> {
    let here = std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default();
    match std::fs::metadata(input) {
        Ok(m) if m.is_dir() => return Err(explain(&format!("'{}' is a folder, not a .tex file. Give the path of the .tex file to compile.", input.display()))),
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(explain(&format!("The input file '{}' does not exist. Check its name, and that you run rushtex from the folder that holds it (you are in '{here}').", input.display())));
        }
        Err(e) => return Err(explain(&format!("The input file '{}' cannot be read: {e}.", input.display()))),
    }
    let Some(dir) = output.parent().filter(|d| !d.as_os_str().is_empty()) else { return Ok(()) };
    match std::fs::metadata(dir) {
        Ok(m) if m.is_dir() => Ok(()),
        Ok(_) => Err(explain(&format!("The output folder '{}' you chose with -o is a file, not a folder, so '{}' cannot be written there. Choose another path with -o.", dir.display(), output.display()))),
        Err(_) => match std::fs::create_dir_all(dir) {
            Ok(()) => {
                eprintln!("[NOTE] The output folder '{}' you chose with -o did not exist, so RushTeX created it.", dir.display());
                Ok(())
            }
            Err(e) => Err(explain(&format!(
                "There is no output folder '{}' (the folder you chose with -o for '{}'), and RushTeX could not create it: {e}. Create the folder yourself, or choose a folder that exists with -o.",
                dir.display(),
                output.display()
            ))),
        },
    }
}

/// Writes the PDF to `output` (its folder checked by [`check_paths`]) through `<output>.part`
/// and a rename, so an open viewer reloads the old PDF or the new one, never half a file, and
/// a killed run leaves the old PDF whole; an output that is not a plain file (`/dev/null`, a
/// pipe) is written directly. An error says which file could not be written and why.
fn write_pdf(output: &std::path::Path, pdf: &[u8]) -> Result<(), String> {
    let mut partial = output.as_os_str().to_owned();
    partial.push(".part");
    let partial = std::path::PathBuf::from(partial);
    let direct = std::fs::metadata(output).is_ok_and(|m| !m.is_file());
    let written = if direct { std::fs::write(output, pdf) } else { std::fs::write(&partial, pdf).and_then(|()| std::fs::rename(&partial, output)) };
    written.map_err(|e| {
        if !direct {
            let _ = std::fs::remove_file(&partial);
        }
        explain(&format!("RushTeX could not write the PDF to '{}': {e}. Check that the folder exists and that you may write to it.", output.display()))
    })
}

/// The line a finished compilation prints: where the PDF is (an absolute path), its pages
/// and the time taken.
fn compiled(output: &std::path::Path, pages: usize, secs: f64) {
    let path = std::fs::canonicalize(output).unwrap_or_else(|_| output.to_path_buf());
    eprintln!("[COMPILED] Output file is in {}, {}", path.display(), pages_in(pages, secs));
}

/// `12 pages in 0.012 seconds`.
fn pages_in(pages: usize, secs: f64) -> String {
    format!("{pages} {} in {secs:.3} seconds", if pages == 1 { "page" } else { "pages" })
}

/// `rushtex fonts list`: this build's font packs and where each is installed.
/// `rushtex fonts install --from DIR`: copies the packs found in DIR into the cache.
fn fonts_command(args: &[String]) -> ExitCode {
    use rushtex::data::data_pack;
    match args.first().map(String::as_str) {
        Some("list") => {
            for l in data_pack::list() {
                println!("{l}");
            }
            ExitCode::SUCCESS
        }
        Some("install") if args.get(1).map(String::as_str) == Some("--from") && args.len() == 3 => match data_pack::install_from(std::path::Path::new(&args[2])) {
            Ok((done, missing)) => {
                println!("{done} packs installed, {missing} not in {}", args[2]);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("[ERROR] {e}");
                ExitCode::from(1)
            }
        },
        _ => {
            eprintln!("usage: rushtex fonts list | rushtex fonts install --from <dir>");
            ExitCode::from(2)
        }
    }
}
