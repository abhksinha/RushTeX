//! # Command-Line Interface Driver (`main`)
//!
//! Provides the executable CLI frontend for `rushtex`.
//!
//! ## Usage Modes
//!
//! 1. **Direct File Compilation**:
//!    ```bash
//!    rushtex input.tex [-o output.pdf]
//!    ```
//! 2. **Piped Stream Compilation**:
//!    ```bash
//!    cat input.tex | rushtex > output.pdf
//!    ```
//! 3. **AST Inspection**:
//!    ```bash
//!    rushtex --ast input.tex
//!    ```
//! 4. **Uncompressed Stream Debugging**:
//!    ```bash
//!    rushtex input.tex --no-compress -o uncompressed.pdf
//!    ```
//! 5. **Native pipeline** (no TeX installation needed; files compile with the TeX engine
//!    on the LaTeX format when one is installed):
//!    ```bash
//!    rushtex --native input.tex [-o output.pdf]
//!    ```
//! 6. **Watch Mode** (recompile on every save, reusing unchanged paragraphs and pages):
//!    ```bash
//!    rushtex --watch input.tex [-o output.pdf]
//!    ```

#![forbid(unsafe_code)]

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    // `rushtex fonts ...`: the font packs (native-architecture.md §4b).
    if env::args().nth(1).as_deref() == Some("fonts") {
        return fonts_command(&env::args().skip(2).collect::<Vec<_>>());
    }
    let mut cli_arguments = env::args().skip(1);
    let mut input_file_path: Option<PathBuf> = None;
    let mut output_file_path: Option<PathBuf> = None;
    let mut dump_ast = false;
    let mut dump_expanded = false;
    let mut dump_gaps = false;
    let mut native_gaps = false;
    let mut enable_compression = true;
    let mut threads = false;
    let mut watch = false;
    #[cfg_attr(target_arch = "wasm32", allow(unused_variables, unused_assignments))]
    let mut native = false;
    #[cfg_attr(target_arch = "wasm32", allow(unused_variables, unused_assignments))]
    let mut format_path: Option<PathBuf> = None;

    while let Some(argument) = cli_arguments.next() {
        match argument.as_str() {
            "-o" | "--output" => match cli_arguments.next() {
                Some(path_str) => output_file_path = Some(PathBuf::from(path_str)),
                None => {
                    eprintln!("[ERROR] -o requires a path");
                    return ExitCode::from(2);
                }
            },
            "--no-compress" => enable_compression = false,
            "--threads" => threads = true,
            "--watch" => watch = true,
            "--native" => native = true,
            "--fmt" => match cli_arguments.next() {
                Some(path_str) => format_path = Some(PathBuf::from(path_str)),
                None => {
                    eprintln!("[ERROR] --fmt requires a path");
                    return ExitCode::from(2);
                }
            },
            "--ast" => dump_ast = true,
            "--expand" => dump_expanded = true,
            "--gaps" => dump_gaps = true,
            "--native-gaps" => native_gaps = true,
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            flag if flag.starts_with('-') => {
                eprintln!("[ERROR] unknown flag {flag}");
                return ExitCode::from(2);
            }
            positional_arg => {
                if input_file_path.is_some() {
                    eprintln!("[ERROR] extra argument {positional_arg}");
                    return ExitCode::from(2);
                }
                input_file_path = Some(PathBuf::from(positional_arg));
            }
        }
    }

    let Some(input_path) = input_file_path else {
        // stdin → stdout if piped, else display help
        if atty_stdin() {
            print_help();
            return ExitCode::from(2);
        }
        #[cfg(not(target_arch = "wasm32"))]
        let start_time = std::time::Instant::now();
        let mut source_content = String::new();
        if let Err(err) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut source_content)
        {
            eprintln!("{}", rushtex::Error::Io(err));
            return ExitCode::from(1);
        }
        let mut pdf_bytes = Vec::new();
        let page_count = match rushtex::compile_into_with(&source_content, &mut pdf_bytes, rushtex::CompileOptions { compress: enable_compression, threads }) {
            Ok(count) => count,
            Err(err) => {
                eprintln!("{err}");
                return ExitCode::from(1);
            }
        };
        drop(source_content);
        if let Err(err) = std::io::Write::write_all(&mut std::io::stdout(), &pdf_bytes) {
            eprintln!("{}", rushtex::Error::Io(err));
            return ExitCode::from(1);
        }
        drop(pdf_bytes);
        #[cfg(not(target_arch = "wasm32"))]
        let elapsed_secs = start_time.elapsed().as_secs_f64();
        #[cfg(target_arch = "wasm32")]
        let elapsed_secs = 0.0;
        eprintln!("[COMPILED] Output written to standard output, {}", pages_in(page_count, elapsed_secs));
        return ExitCode::SUCCESS;
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
                eprintln!("{}", rushtex::Error::Io(err));
                ExitCode::from(1)
            }
        };
    }

    if dump_gaps {
        return match std::fs::read_to_string(&input_path) {
            Ok(source_content) => {
                print!("{}", rushtex::gap_report(&source_content));
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("{}", rushtex::Error::Io(err));
                ExitCode::from(1)
            }
        };
    }

    if dump_expanded {
        return match std::fs::read_to_string(&input_path) {
            Ok(source_content) => match rushtex::expand::expand(&source_content) {
                Ok(expanded) => {
                    print!("{expanded}");
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("{}", rushtex::Error::from(err));
                    ExitCode::from(1)
                }
            },
            Err(err) => {
                eprintln!("{}", rushtex::Error::Io(err));
                ExitCode::from(1)
            }
        };
    }

    if dump_ast {
        match std::fs::read_to_string(&input_path) {
            Ok(source_content) => match rushtex::expand::expand(&source_content)
                .map_err(rushtex::Error::from)
                .and_then(|src| rushtex::tex::parse(&src).map(|doc| format!("{doc:#?}")).map_err(rushtex::Error::from))
            {
                Ok(document_ast) => {
                    println!("{document_ast}");
                    return ExitCode::SUCCESS;
                }
                Err(err) => {
                    eprintln!("{err}");
                    return ExitCode::from(1);
                }
            },
            Err(err) => {
                eprintln!("{}", rushtex::Error::Io(err));
                return ExitCode::from(1);
            }
        }
    }

    if let Err(message) = check_paths(&input_path, &output_file_path.clone().unwrap_or_else(|| input_path.with_extension("pdf"))) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    let options = rushtex::CompileOptions { compress: enable_compression, threads };
    if watch {
        let output = output_file_path.unwrap_or_else(|| input_path.with_extension("pdf"));
        return watch_and_compile(&input_path, &output, options);
    }

    // Without the oracle the native pipeline is the only path; `--native` and `--fmt` are moot.
    #[cfg(not(all(feature = "oracle", not(target_arch = "wasm32"))))]
    let _ = (native, format_path);
    #[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
    if !native {
        if !enable_compression || threads {
            eprintln!("[WARN] --no-compress and --threads apply to the native pipeline only (add --native)");
        }
        let output = output_file_path.clone().unwrap_or_else(|| input_path.with_extension("pdf"));
        if let Some(code) = compile_engine(&input_path, &output, format_path) {
            return code;
        }
    }

    match rushtex::compile_file_with(&input_path, output_file_path.as_deref(), options) {
        Ok(report) => {
            let output = output_file_path.clone().unwrap_or_else(|| input_path.with_extension("pdf"));
            compiled(&output, report.page_count, report.elapsed.as_secs_f64());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}

/// How often `--watch` looks at the source's modification time.
#[cfg(not(target_arch = "wasm32"))]
const WATCH_POLL: std::time::Duration = std::time::Duration::from_millis(50);

/// Compiles `input` to `output` now and again whenever the file changes, until killed.
/// One [`rushtex::Session`] serves every compile, so a save redoes only what changed. The
/// PDF is written to a temporary file and renamed over `output`, so a viewer never reads
/// half of it.
#[cfg(not(target_arch = "wasm32"))]
fn watch_and_compile(input: &std::path::Path, output: &std::path::Path, options: rushtex::CompileOptions) -> ExitCode {
    let mut session = rushtex::Session::new();
    let mut pdf = Vec::new();
    let mut partial = output.as_os_str().to_owned();
    partial.push(".part");
    let partial = PathBuf::from(partial);
    let modified = || std::fs::metadata(input).and_then(|m| m.modified()).ok();
    let mut seen = None;
    eprintln!("[WATCH] {} -> {} (Ctrl-C to stop)", input.display(), output.display());
    loop {
        let now = modified();
        if now.is_some() && now != seen {
            seen = now;
            let start = std::time::Instant::now();
            let result = std::fs::read_to_string(input)
                .map_err(rushtex::Error::Io)
                .and_then(|source| session.compile_into(&source, &mut pdf, options))
                .and_then(|pages| {
                    std::fs::write(&partial, &pdf)?;
                    std::fs::rename(&partial, output)?;
                    Ok(pages)
                });
            match result {
                Ok(pages) => compiled(output, pages, start.elapsed().as_secs_f64()),
                Err(err) => eprintln!("{err}"),
            }
        }
        std::thread::sleep(WATCH_POLL);
    }
}

#[cfg(target_arch = "wasm32")]
fn watch_and_compile(_input: &std::path::Path, _output: &std::path::Path, _options: rushtex::CompileOptions) -> ExitCode {
    eprintln!("[ERROR] --watch needs a file system");
    ExitCode::from(2)
}

/// The document as `pdflatex` runs it: the TeX engine on the LaTeX format, files found as
/// kpathsea finds them, run again while its `.aux` and `.toc` change. The format is made
/// from `pdflatex.ini` on first use and kept in the cache directory. `None` when there is no
/// TeX installation; the native pipeline then compiles the document.
#[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
fn compile_engine(input: &std::path::Path, output: &std::path::Path, format_path: Option<PathBuf>) -> Option<ExitCode> {
    use rushtex::engine::{engine_host, engine_job};
    let start_time = std::time::Instant::now();
    let cache = cache_dir();
    let doc_dir = input.parent().filter(|d| !d.as_os_str().is_empty()).map(std::path::Path::to_path_buf);
    // The file search is set up on first use: the native job reads no TeX file unless its
    // font files are not the ones the binary carries.
    let kpse_cell = std::cell::OnceCell::new();
    let kpse = || kpse_cell.get_or_init(|| engine_host::Kpse::new("pdflatex", cache.as_ref().map(|c| c.join("kpse-paths")).as_deref(), doc_dir.clone())).as_ref();
    // `RT_READS=1`: each file the job asks for, on standard error.
    let read = |name: &str| {
        if std::env::var_os("RT_READS").is_some() {
            eprintln!("read {name}");
        }
        // The document's own files (`.bbl`, figures) are found beside it first, as
        // `Kpse::find` looks there first: read so, they cost no `ls-R` index.
        let local = (!name.starts_with('/')).then(|| doc_dir.as_deref().unwrap_or(std::path::Path::new(".")).join(name));
        if let Some(b) = local.filter(|p| p.is_file()).and_then(|p| std::fs::read(p).ok()) {
            return Some(b);
        }
        kpse()?.read(name)
    };
    let fmt_path = format_path.or_else(|| cache.as_ref().map(|c| c.join("pdflatex.rfmt")))?;
    let source: String = match std::fs::read(input) {
        // pdfTeX reads each byte as one character code.
        Ok(bytes) => bytes.iter().map(|&b| char::from(b)).collect(),
        Err(err) => {
            eprintln!("{}", rushtex::Error::Io(err));
            return Some(ExitCode::from(1));
        }
    };
    let job_name = input.file_stem().and_then(|s| s.to_str()).unwrap_or("texput");
    let now = engine_host::run_date();
    // The native document job first (`engine_doc`): no format, no TeX files, one pass.
    // `RT_ENGINE=1` runs the engine on the format instead, as the oracle.
    if std::env::var_os("RT_ENGINE").is_none() {
        // `RT_TIME=1`: the job's phases (`engine_doc::run`).
        if std::env::var_os("RT_TIME").is_some() {
            eprintln!("{:8.3} ms  before the job (paths, source)", start_time.elapsed().as_secs_f64() * 1e3);
        }
        // `\bibliography` reads `\jobname.bbl` where the engine's file search finds it.
        let bbl = if source.contains("\\bibliography") { read(&format!("{job_name}.bbl")).map(|b| b.iter().map(|&b| char::from(b)).collect::<String>()) } else { None };
        let source = rushtex::engine::engine_doc::splice_bibliography(&source, bbl.as_deref());
        let job = rushtex::engine::engine_doc::run(&source, now, Some(&read));
        // `RT_NATIVE_WHY=1`: why the document runs on the engine.
        if let (Err(why), Some(_)) = (&job, std::env::var_os("RT_NATIVE_WHY")) {
            eprintln!("native job declined: {why}");
        }
        if let Ok(job) = job {
            if let Err(err) = write_pdf(output, &job.pdf) {
                eprintln!("{err}");
                return Some(ExitCode::from(1));
            }
            for err in &job.errors {
                eprintln!("! {err}");
            }
            // `RT_NATIVE_WHY=1`: which path ran, for the testing scripts.
            if std::env::var_os("RT_NATIVE_WHY").is_some() {
                eprintln!("native job accepted");
            }
            compiled(output, job.pages, start_time.elapsed().as_secs_f64());
            return Some(if job.errors.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) });
        }
    }
    // Without a TeX installation the engine cannot run: the native pipeline compiles the
    // document.
    kpse()?;
    // The format is read again for each run (from the OS cache) and dropped once undumped;
    // only a format that could not be written to the cache is kept in memory.
    let mut unsaved: Option<Vec<u8>> = None;
    // A missing format, or one an older build dumped, is made again (once).
    for attempt in 0..2 {
        let load = || unsaved.clone().or_else(|| std::fs::read(&fmt_path).ok());
        if let Some(job) = engine_job::run(&load, &source, &engine_job::Opts { job_name, now, max_runs: engine_job::MAX_RUNS }, &read, &mut |_| {}, &mut |m| {
            // `RT_WRITTEN=<dir>`: the files the last run wrote (`.aux`, `.toc`, …), for the
            // P2b inventory of what LaTeX carries between runs.
            if let Ok(dir) = std::env::var("RT_WRITTEN") {
                for w in &m.closed {
                    let name = std::path::Path::new(&w.name).file_name().map_or(w.name.as_str(), |n| n.to_str().unwrap_or(""));
                    let _ = std::fs::write(std::path::Path::new(&dir).join(name), &w.text);
                }
            }
        }) {
            if let Err(err) = write_pdf(output, &job.pdf) {
                eprintln!("{err}");
                return Some(ExitCode::from(1));
            }
            for err in &job.errors {
                eprintln!("! {err}");
            }
            if std::env::var_os("RT_NATIVE_WHY").is_some() {
                eprintln!("engine job: {} runs", job.runs);
            }
            compiled(output, job.pages, start_time.elapsed().as_secs_f64());
            return Some(if job.errors.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) });
        }
        if attempt == 1 {
            break;
        }
        let ini: String = kpse()?.read("pdflatex.ini")?.iter().map(|&b| char::from(b)).collect();
        // Exactness is proven against one pinned kernel; another one is used, but said so.
        let (pin_date, pin_pl) = engine_host::PINNED_LATEX;
        if let Some((date, pl)) = kpse()?.read("latex.ltx").as_deref().and_then(engine_host::latex_version) {
            if (date.as_str(), pl) != (pin_date, pin_pl) {
                eprintln!("[WARN] LaTeX {date} patch level {pl} is installed; RushTeX's output is proven identical to pdfLaTeX for LaTeX {pin_date} patch level {pin_pl} (TeX Live {})", engine_host::PINNED_TEXLIVE);
            }
        }
        eprintln!("[INFO] Making the LaTeX format {} (once)", fmt_path.display());
        let built = engine_job::build_format(&ini, "pdflatex", now, &read)?;
        if let Some(dir) = fmt_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::write(&fmt_path, &built).is_err() {
            unsaved = Some(built);
        }
    }
    eprintln!("[ERROR] the format {} could not be made", fmt_path.display());
    Some(ExitCode::from(1))
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
rushtex — ultra-fast, zero-dependency LaTeX compiler in pure safe Rust

USAGE:
  rushtex <file.tex> [-o out.pdf]
  rushtex fonts list | install --from <dir>
  rushtex --ast <file.tex>
  rushtex <file.tex> --no-compress -o out.pdf
  rushtex --watch <file.tex> [-o out.pdf]
  cat file.tex | rushtex > out.pdf

OPTIONS:
  -o, --output <path>    Output PDF file path (default: <file>.pdf)
  --native               Use the native pipeline instead of the TeX engine (the engine,
                         pdfLaTeX's exact output, is used when TeX Live is installed)
  --fmt <path>           The LaTeX format to run on (default: made once from
                         pdflatex.ini into ~/.cache/rushtex/pdflatex.rfmt)
  --no-compress          Disable FlateDecode stream compression for debugging
  --threads              Write pages on a second thread (faster wall time on long
                         documents, slightly more CPU)
  --watch                Recompile whenever the file changes; unchanged paragraphs
                         and pages are reused from the previous compile
  --ast                  Dump internal AST representation to stdout
  --expand               Print the source after macro expansion to stdout
  --gaps                 List unsupported commands, environments and errors (no PDF)
  --native-gaps          List everything the native job's pre-scan does not take yet
  -h, --help             Display this help message

DOCUMENT STRUCTURE:
  \\documentclass[10pt|11pt|12pt, a4paper|letterpaper]{{article}}
  \\usepackage[margin=1in]{{geometry}}
  \\title{{...}}  \\author{{...}}  \\date{{...}}  \\maketitle
  \\pagestyle{{plain|empty}}
  \\section{{...}}  \\subsection{{...}}  \\subsubsection{{...}}
  \\newpage  \\clearpage  \\vspace{{...}}  \\hrule

TYPOGRAPHY & FORMATTING:
  \\textbf{{...}}  \\textit{{...}}  \\emph{{...}}  \\texttt{{...}}  \\verb|...|
  \\par  \\noindent  \\%  \\$  \\&  \\#  \\_  \\{{  \\}}

ENVIRONMENTS:
  \\begin{{verbatim}} ... \\end{{verbatim}}
  \\begin{{lstlisting}}[language=..., numbers=left] ... \\end{{lstlisting}}
  \\begin{{tabular}}{{|l|c|r|}} \\hline ... \\\\ \\hline \\end{{tabular}}
  \\begin{{itemize}}   \\item ... \\end{{itemize}}
  \\begin{{enumerate}} \\item ... \\end{{enumerate}}
  \\begin{{figure}}[h] \\includegraphics{{...}} \\caption{{...}} \\end{{figure}}"
    );
}

fn atty_stdin() -> bool {
    // no libc crate: treat a seek(0) failure on stdin as "probably a tty"
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

/// An error explained in plain words, between the compiler's log banners.
fn explain(message: &str) -> String {
    format!("==========================================\nRUSHTEX - THE ULTRA FAST TEX FILE COMPILER\n==========================================\n[ERROR] {message}\n==========END OF COMPILER LOGS============")
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

/// Writes the PDF to `output` (its folder checked by [`check_paths`]); an error says
/// which file could not be written and why.
#[cfg(all(feature = "oracle", not(target_arch = "wasm32")))]
fn write_pdf(output: &std::path::Path, pdf: &[u8]) -> Result<(), String> {
    std::fs::write(output, pdf).map_err(|e| explain(&format!("RushTeX could not write the PDF to '{}': {e}. Check that the folder exists and that you may write to it.", output.display())))
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
