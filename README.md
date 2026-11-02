# rushtex

A LaTeX-to-PDF compiler written from scratch in pure safe Rust (`#![forbid(unsafe_code)]`, no dependencies). Its PDF is pixel-identical to pdfLaTeX's.

## How it works

- **Native path:** a document whose every command is ported compiles in Rust, in one pass, with no `.aux` files or reruns.
- **Engine path:** any other document runs whole on a Rust port of TeX and pdfTeX, on the LaTeX format, as `pdflatex` does (needs TeX Live).

## Speed

[`docs/master.tex`](docs/master.tex) is the reference document: 87 pages covering everything ported so far. Its output is [`docs/master.pdf`](docs/master.pdf), compiled natively and identical to pdfLaTeX's on every page.

| Compiler | Instructions retired |
|---|---|
| rushtex, one pass | 0.50 billion |
| pdfLaTeX, one run | 40.5 billion (80x more) |
| pdfLaTeX, the usual 3 runs | 128.3 billion (255x more) |

## Build and use

Needs Rust stable; on macOS, version 13 or later.

```sh
cargo build --release
target/release/rushtex paper.tex            # writes paper.pdf
target/release/rushtex --watch paper.tex    # recompiles on every save
target/release/rushtex --native-gaps paper.tex   # what the native path does not take yet
```

Contributing: see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT or Apache-2.0, at your option.
