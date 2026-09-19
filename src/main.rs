use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut dump_ast = false;
    let mut compress = true;

    while let Some(a) = args.next() {
        match a.as_str() {
            "-o" | "--output" => match args.next() {
                Some(p) => output = Some(PathBuf::from(p)),
                None => {
                    eprintln!("texrs: -o requires a path");
                    return ExitCode::from(2);
                }
            },
            "--no-compress" => compress = false,
            "--ast" => dump_ast = true,
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            s if s.starts_with('-') => {
                eprintln!("texrs: unknown flag {s}");
                return ExitCode::from(2);
            }
            s => {
                if input.is_some() {
                    eprintln!("texrs: extra argument {s}");
                    return ExitCode::from(2);
                }
                input = Some(PathBuf::from(s));
            }
        }
    }

    let Some(input) = input else {
        // stdin → stdout if piped, else help
        if atty_stdin() {
            print_help();
            return ExitCode::from(2);
        }
        let mut src = String::new();
        if let Err(e) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut src) {
            eprintln!("texrs: {e}");
            return ExitCode::from(1);
        }
        return match texrs::compile_str_opts(&src, compress) {
            Ok(pdf) => {
                if let Err(e) = std::io::Write::write_all(&mut std::io::stdout(), &pdf) {
                    eprintln!("texrs: {e}");
                    return ExitCode::from(1);
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("texrs: {e}");
                ExitCode::from(1)
            }
        };
    };

    if dump_ast {
        match std::fs::read_to_string(&input) {
            Ok(src) => match texrs::tex::parse(&src) {
                Ok(doc) => {
                    println!("{doc:#?}");
                    return ExitCode::SUCCESS;
                }
                Err(e) => {
                    eprintln!("texrs: parse: {e}");
                    return ExitCode::from(1);
                }
            },
            Err(e) => {
                eprintln!("texrs: {e}");
                return ExitCode::from(1);
            }
        }
    }

    match texrs::compile_file_opts(&input, output.as_deref(), compress) {
        Ok(out) => {
            eprintln!("wrote {}", out.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("texrs: {e}");
            ExitCode::from(1)
        }
    }
}

fn print_help() {
    eprintln!(
        "\
texrs — zero-dependency TeX-subset → PDF

USAGE:
  texrs <file.tex> [-o out.pdf]
  texrs --ast <file.tex>
  cat file.tex | texrs > out.pdf

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
