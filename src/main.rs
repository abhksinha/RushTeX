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

#![forbid(unsafe_code)]

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut cli_arguments = env::args().skip(1);
    let mut input_file_path: Option<PathBuf> = None;
    let mut output_file_path: Option<PathBuf> = None;
    let mut dump_ast = false;
    let mut enable_compression = true;

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
            "--ast" => dump_ast = true,
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
        let page_count = match rushtex::compile_into_opts_counted(&source_content, &mut pdf_bytes, enable_compression) {
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
        let page_unit = if page_count == 1 { "page" } else { "pages" };
        #[cfg(not(target_arch = "wasm32"))]
        let elapsed_secs = start_time.elapsed().as_secs_f64();
        #[cfg(target_arch = "wasm32")]
        let elapsed_secs = 0.0;
        eprintln!(
            "[DONE] Compilation of <stdin> completed and RUSHTEX wrote {} {} in {:.3} second",
            page_count, page_unit, elapsed_secs
        );
        return ExitCode::SUCCESS;
    };

    if dump_ast {
        match std::fs::read_to_string(&input_path) {
            Ok(source_content) => match rushtex::tex::parse(&source_content) {
                Ok(document_ast) => {
                    println!("{document_ast:#?}");
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

    match rushtex::compile_file_opts(&input_path, output_file_path.as_deref(), enable_compression) {
        Ok(report) => {
            let page_unit = if report.page_count == 1 {
                "page"
            } else {
                "pages"
            };
            eprintln!(
                "[DONE] Compilation of {} completed and RUSHTEX wrote {} {} in {:.3} second",
                input_path.display(),
                report.page_count,
                page_unit,
                report.elapsed.as_secs_f64()
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}

fn print_help() {
    eprintln!(
        "\
rushtex — ultra-fast, zero-dependency LaTeX compiler in pure safe Rust

USAGE:
  rushtex <file.tex> [-o out.pdf]
  rushtex --ast <file.tex>
  rushtex <file.tex> --no-compress -o out.pdf
  cat file.tex | rushtex > out.pdf

OPTIONS:
  -o, --output <path>    Output PDF file path (default: <file>.pdf)
  --no-compress          Disable FlateDecode stream compression for debugging
  --ast                  Dump internal AST representation to stdout
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
