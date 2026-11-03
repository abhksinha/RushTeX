# rushtex

A LaTeX-to-PDF compiler written from scratch in pure safe Rust (`#![forbid(unsafe_code)]`, no dependencies). Its PDF is pixel-identical to pdfLaTeX's.

## How it works

rushtex is native only: LaTeX's typesetting is solved as a computation in Rust (TeX's exact line breaking, page building and math arithmetic over typed nodes and build-time tables), not by interpreting LaTeX's macros. One self-contained binary, no TeX Live, one pass, no `.aux` files or reruns. A document that uses something not supported yet gets a clear error; `rushtex --native-gaps paper.tex` lists what. Supported classes and packages: [docs/native-packages.md](docs/native-packages.md).

## Speed

[`docs/master.tex`](docs/master.tex) is the reference document: 87 pages covering everything ported so far. Its output is [`docs/master.pdf`](docs/master.pdf), compiled natively and identical to pdfLaTeX's on every page.

| Compiler | Instructions retired |
|---|---|
| rushtex, one pass | 0.51 billion |
| pdfLaTeX, one run | 43.1 billion (85x more) |
| pdfLaTeX, the usual 3 runs | 134.8 billion (266x more) |

All benchmarks, with wall time, memory and PDF size: [abhksinha.github.io/RushTeX](https://abhksinha.github.io/RushTeX/#benchmarks).

## Build and use

### Install

```sh
brew install abhksinha/rushtex/rushtex                    # macOS, Linux
curl -fsSL https://raw.githubusercontent.com/abhksinha/RushTeX/master/scripts/install.sh | sh
cargo install --locked --git https://github.com/abhksinha/RushTeX rushtex
```

On Windows (PowerShell): `irm https://raw.githubusercontent.com/abhksinha/RushTeX/master/scripts/install.ps1 | iex`.
The script installs to `~/.local/bin`; end it with `| sh -s -- --full` for the build with every
font pack, or `--version v0.1.0` for a given release. Archives for macOS (13 or later), Linux and Windows, with
`SHA256SUMS`: [Releases](https://github.com/abhksinha/RushTeX/releases).

### Build from source

Needs Rust stable; on macOS, version 13 or later.

```sh
cargo build --release
target/release/rushtex paper.tex            # writes paper.pdf
target/release/rushtex --watch paper.tex    # recompiles on every save
target/release/rushtex --native-gaps paper.tex   # what is not supported yet
```

Contributing: see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT or Apache-2.0, at your option.
