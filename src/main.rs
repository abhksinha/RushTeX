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
                    eprintln!("rushtex: -o requires a path");
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
                eprintln!("rushtex: unknown flag {flag}");
                return ExitCode::from(2);
            }
            positional_arg => {
                if input_file_path.is_some() {
                    eprintln!("rushtex: extra argument {positional_arg}");
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
        let mut source_content = String::new();
        if let Err(err) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut source_content) {
            eprintln!("rushtex: {err}");
            return ExitCode::from(1);
        }
        return match rushtex::compile_str_opts(&source_content, enable_compression) {
            Ok(pdf_bytes) => {
                if let Err(err) = std::io::Write::write_all(&mut std::io::stdout(), &pdf_bytes) {
                    eprintln!("rushtex: {err}");
                    return ExitCode::from(1);
                }
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("rushtex: {err}");
                ExitCode::from(1)
            }
        };
    };

    if dump_ast {
        match std::fs::read_to_string(&input_path) {
            Ok(source_content) => match rushtex::tex::parse(&source_content) {
                Ok(document_ast) => {
                    println!("{document_ast:#?}");
                    return ExitCode::SUCCESS;
                }
                Err(err) => {
                    eprintln!("rushtex: parse: {err}");
                    return ExitCode::from(1);
                }
            },
            Err(err) => {
                eprintln!("rushtex: {err}");
                return ExitCode::from(1);
            }
        }
    }

    match rushtex::compile_file_opts(&input_path, output_file_path.as_deref(), enable_compression) {
        Ok(output_path) => {
            eprintln!("wrote {}", output_path.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("rushtex: {err}");
            ExitCode::from(1)
        }
    }
}

fn print_help() {
    eprintln!(
        "\
rushtex — zero-dependency TeX-subset → PDF

USAGE:
  rushtex <file.tex> [-o out.pdf]
  rushtex --ast <file.tex>
  cat file.tex | rushtex > out.pdf

COMMANDS understood:
  \\textbf{{...}}   \\textit{{...}}   \\emph{{...}}
  \\begin{{enumerate}} \\item ... \\end{{enumerate}}
  \\begin{{itemize}}   \\item ... \\end{{itemize}}
  \\\\  blank-line paragraph  % comment
  \\{{  \\}}  \\%  \\\\"
    );
}

fn atty_stdin() -> bool {
    // no libc crate: treat a seek(0) failure on stdin as "probably a tty"
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}
