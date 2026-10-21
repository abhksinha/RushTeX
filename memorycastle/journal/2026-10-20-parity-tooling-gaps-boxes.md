# Parity tooling: gap inventory, corpus baseline, pdfTeX box oracle, LaTeX sources
Author: abhi+claude | Date: 2026-10-20
Files: `src/lib.rs`, `src/main.rs`, `src/frontend/frontend_parser.rs`, `src/expand/expand_mod.rs`, `scripts/parity/parity.py`, `scripts/parity/baseline.json`, `scripts/fetch_latex_source.sh`, `.gitignore`

## Context
The ten new CTAN documents (`2026-10-20-ctan-corpus-expansion.md`) all fail at their first error, so one compile shows one gap. The user asked for tooling first ("least resistance") under the Resource_Conscious_Software_Engineering.md rules.

## Decision
- `rushtex --gaps FILE` (`gap_report`): expand with `expand_recovering`, parse without layout, record every unknown command and environment, and keep going. Parse errors resync after the next blank line. Expander errors drop back to the source frame. Errors are capped at 1000.
- `Parser.gaps` is `None` in normal compiles. The only normal-path cost is one extra guard on the error arms.
- `parity.py gaps` ranks gaps by how many documents each blocks.
- `parity.py score --all [--save-baseline]` scores every document. A failing document is reported and the run continues; regressions are checked against the committed `scripts/parity/baseline.json`.
- `parity.py boxes DOC PAGE`: shows every drifting line with pdfTeX's vertical recipe (glue, kerns, box sizes) from `\showbox\ShipoutBox`.
- `latex-source/` holds the commented `.dtx` sources. It is gitignored and fetched by `scripts/fetch_latex_source.sh`.

## Rejected
- `Option<Box<Vec>>` for the gaps field: clippy rejects it, and `Parser` is not size-critical.
- `str.splitlines()` on the pdfTeX log: pdfTeX prints glyphs 10 (Ω) and 12 (fi) raw, which split the box listing.
- Treating pdflatex's halt as fatal: encguide skips uninstalled fonts on purpose, so the score now retries without `-halt-on-error`.

## Measured
- testmath in-process A/B, same conditions (battery, low-power mode): 8.89/8.91 ms without the change against 8.93/8.93 ms with it, which is noise. The earlier 4.46 ms was measured on mains power.
- Gap inventory over 14 documents:
  - 122 distinct unknown commands;
  - 13 environments;
  - 6 error kinds.
- Top blockers:
  - `\LaTeXe` and `\copyright` (6 documents, the ltxguide class);
  - `\textwidth` and similar length registers (4);
  - "expected { after command" (5);
  - `\texorpdfstring` (2045 uses);
  - `\fontfamily`, `\selectfont` and the other NFSS commands (amsthdoc).
- Baseline: sample2e, small2e, lppl and testmath are at 100%; the other ten do not compile.

## Open
- Write the work plan from `parity.py gaps`, and decide between macro execution and native reimplementation for class files (ltxguide, amsldoc, IEEEtran).
- Most gaps are length registers and NFSS commands, which suggests real `\dimen` and `\font` state instead of a list of known names.

## Decision (addendum: refindex)
- `scripts/refindex.py NAME` gives every definition of a name across the reference sources:
  - tex.web and pdftex.web: §, procedure, `@d`, module, primitive and its main_control handler;
  - `latex-source/*.dtx` macro blocks;
  - `latex.ltx`, `expl3-code.tex`, and every class, sty, clo and fd file named in the pdfLaTeX corpus logs.
- It also lists rushtex comments that cite the same name.
- The cache is `target/refindex.json`. A rebuild takes 1.4 s; a query takes 0.15–0.3 s. Ranking puts tex.web and `latex.ltx` first and `\setlength` assignments last, and shows 10 hits unless `-a` is given.

## Rejected (addendum)
- Using the memcastle code graph for TeX sources: it does not parse WEB or `.dtx`, and the reference trees are gitignored.
- A substring test for "tex.web" when ranking: it also matches "pdftex.web".
