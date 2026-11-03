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

## Two builds

RushTeX the product is native only: `cargo build --release` gives `target/release/rushtex`,
which compiles a document with no TeX installation (the fonts it reads are in the binary)
and declines, with an error naming what it does not take, a document it cannot compile
exactly. The TeX engine is a development tool, behind the `oracle` feature, in its own
target folder:

```sh
cargo build --release --features oracle --target-dir target/oracle
```

`target/oracle/release/rushtex` runs a document the native job declines on the engine, as
pdflatex does. A change that touches exactness uses it: the box checks
(`scripts/native_probes.sh` runs it by default), `parity-testing` and the table generators
(`rushtex-gen`, built with `--features gen,oracle --target-dir target/oracle`). `cargo
benchmark` measures the product's build and builds both; `check` runs the unit tests of both.

## What you need

- Rust (stable), from <https://rustup.rs>
- On macOS, macOS 13 or later: `.cargo/config.toml` sets `MACOSX_DEPLOYMENT_TARGET` to 13.0
- TeX Live with `pdflatex` (the reference RushTeX is compared against)
- MuPDF's `mutool` (`brew install mupdf-tools`, `apt install mupdf-tools`)
- `zsh` (preinstalled on macOS; `apt install zsh`)

## What `check` does

1. Builds `rushtex` (the product and the oracle build) and the testing framework (`testing/`).
2. Runs the unit tests of both builds.
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
  ported declines (the product says so; the oracle build runs the engine), so a document is
  never typeset wrong.
- No `cargo fmt`: the code keeps its long-line style.

## Releasing

Releases come from tags; `.github/workflows/release.yml` does the rest (maintainers only).

- **Dry run.** Actions > Release > Run workflow (or `gh workflow run release.yml --ref master`)
  builds all ten archives and keeps them as artifacts of that run; no release is created.

- **Binaries, `v*`.** Set `version` in `Cargo.toml` (and `Cargo.lock`) to match, commit, then
  `git tag v0.1.0 && git push origin v0.1.0`. The workflow builds macOS (arm, Intel; 13 or
  later), Linux (x86_64, arm; glibc of Ubuntu 22.04) and Windows (x86_64), each as core (the
  default build) and full (`--features fonts-full`, archive name ending `-full`), writes
  `SHA256SUMS` and attaches everything to the release `v0.1.0`, marked latest. A failed target
  is rerun from the Actions page; the release is published once all ten have built.
- **Font packs, `fonts-v*`.** When the packs change (`scripts/texlive/packs.tsv`, the pin, or
  `embed.set`): run `rushtex-gen embed --list scripts/texlive/files.tsv` and
  `rushtex-gen packs --list scripts/texlive/files.tsv` (built with `--features gen,oracle
  --target-dir target/oracle`), which write `assets/tables/embed.rtdb`, the packs
  (`assets/packs/<pack>.rtpack`, not tracked in git: they live only as release assets) and the
  catalog `assets/tables/packs.rtdb` (each pack's SHA-256, pinned in the binary); bump
  `FONTS_RELEASE` in `src/data/data_pack.rs` to the new N, commit, then
  `git tag fonts-vN && git push origin fonts-vN`. The workflow builds the packs
  with `rushtex-gen packs` from the pinned TeX Live snapshot, checks that the catalog they
  give is the committed `assets/tables/packs.rtdb`, and attaches them with `SHA256SUMS` to
  the release `fonts-vN` (never marked latest, so `releases/latest` keeps naming the
  binaries). `rushtex fonts install <pack>` downloads `fonts-v<N>/<pack>.rtpack` from it, and
  the `v*` workflow's full builds fetch the packs from it (checked against its `SHA256SUMS`),
  so publish `fonts-vN` before the `v*` tag that needs it.
  A change to the pack file's layout also bumps `PACK_FORMAT`, so an installed pack of the
  old layout is refused with "reinstall with: rushtex fonts install <pack>".
- **Homebrew.** After a `v*` release has finished:
  `packaging/homebrew/update.sh v0.1.0 ../homebrew-rushtex/Formula/rushtex.rb`, then commit
  and push the tap (`packaging/README.md`).
- **Install script.** `scripts/install.sh` and `scripts/install.ps1` take the latest release by
  themselves. To try one against a local folder:
  `RUSHTEX_RELEASE_BASE=file:///path/to/folder RUSHTEX_INSTALL_DIR=/tmp/bin sh scripts/install.sh --version v0.1.0`
  with the archives and `SHA256SUMS` in `/path/to/folder/download/v0.1.0/`.
