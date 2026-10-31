# Contributing to RushTeX

RushTeX's promise is that its PDF is identical to pdfLaTeX's. Every change keeps that
promise, so before you open a pull request, run, from the repository root:

```sh
cargo benchmark
```

move to **4. Check my change** with the arrow keys (or press `4`) and press Enter for the
base, `origin/master`. The same check without the menu, for scripts and CI:

```sh
cargo benchmark check origin/master
```

It prints `PASS` or `FAIL` for each step and ends with `check: PASS` or `check: FAIL`
(exit code 1). A first run takes about ten minutes; later runs take about three. The
benchmark and the check are written in Rust, in the testing framework (`testing/`);
`cargo benchmark` is an alias (`.cargo/config.toml`) that builds and starts it.

## What you need

- Rust (stable), from <https://rustup.rs>
- TeX Live with `pdflatex` (the reference RushTeX is compared against)
- MuPDF's `mutool` (`brew install mupdf-tools`, `apt install mupdf-tools`)
- `zsh` (preinstalled on macOS; `apt install zsh`)

## What `check` does

1. Builds `rushtex` and the testing framework (`testing/`).
2. Runs the unit tests.
3. Runs every probe in `scripts/native_probes/` and `docs/master.tex` twice: on RushTeX's
   native path and on its TeX engine. Every page's boxes must be identical.
4. Compares `docs/master.tex` and the documents in `scripts/bench/` with pdfLaTeX's
   output: every glyph, rule, link and pixel at 300 dpi (`parity-testing zero`).
5. Edits `docs/master.tex` (native) and `scripts/parity/corpus/bare_jrnl.tex` (on the
   engine) at random, in one `--watch` session, and compares the session's PDF after every
   save with a fresh compile, byte for byte (`parity-testing incr`).
6. With a base (`origin/master`), builds the base and checks every probe and benchmark PDF
   is byte-identical to the base's. A change meant to alter output (a fix) differs here
   on purpose: say so in the pull request; steps 3 and 4 must still pass.

## If a step fails

- Step 3 prints the first differing lines of the page's box dump. Rerun one file with
  `scripts/native_probes.sh path/to/file.tex -k /tmp/boxes` to keep both dumps.
- Step 4: `testing/target/release/parity-testing zero docs/master.tex --why PAGE`
  shows the first mark on that page without an exact twin.
- Step 5 names the first save that differs, its edit and source lines, the first PDF object
  that differs, and keeps both PDFs and both sources in `target/parity/incr/<doc>-fail/`.
  The edits come from a seed, so `testing/target/release/parity-testing incr docs/master.tex
  --edits N --seed S -v` replays them exactly. Any document can be tested so; a change to
  `--watch` or to what it keeps between compiles is also run on a few templates:
  `parity-testing incr DOC.. --edits 60 --seed S` (`--edits` default 20, `--seed` 1).
- Step 6 keeps both PDFs in `target/check-pdfs/base` and `target/check-pdfs/now`.

## Speed

Speed is the other promise. `cargo benchmark` and choose:

- **1. master.tex against pdfLaTeX**: instructions retired, CPU and wall time, peak RAM, PDF
  size, cost per page, and the glyph-to-glyph and pixel match, in one report.
- **2. Any document against pdfLaTeX**: the same report for a file you name.
- **3. Every bench document**: master and `scripts/bench/*.tex` (10 to 95 pages).

Compare instructions retired, not seconds: they do not change with the machine's load or
power state. The menu prints each choice as a command (`cargo benchmark docs/master.tex`)
to run again directly; `cargo benchmark --help` lists them all.

## Code

- Native features follow `docs/architecture/` (guarded direct lowering): anything not
  ported declines to the engine, so a document is never typeset wrong.
- No `cargo fmt`: the code keeps its long-line style.
