# A1/A1+: the pinned TeX Live 2026 snapshot, and CI gating zero shift on it
Author: abhi+claude | Date: 2026-10-22
Files: `scripts/texlive/pin.py`, `scripts/texlive/files.tsv`, `scripts/texlive/manifest.tsv`, `src/engine/engine_host.rs`, `src/engine/engine_run.rs`, `src/main.rs`, `scripts/parity/parity.py`, `.github/workflows/ci.yml`, `CLAUDE.md`, `memorycastle/journal/progress.md`

## Context
Zero shift is proven against one pdfTeX and one LaTeX kernel. Before this, CI installed Ubuntu's apt TeX Live (a different year), so the parity job could only report. The user chose TeX Live 2026 over 2023.

## Decision
- **Pin.** The pin is a file snapshot, not an installer.
  - `scripts/texlive/pin.py files` runs the corpus with `RT_FILES` and lists every TeX file read, with sizes and sha256 (`files.tsv`).
  - `record` writes `manifest.tsv`: TeX Live, tlmgr, pdfTeX and kernel versions, plus every installed package's revision.
  - `check` reports drift.
  - `pack` writes one relocatable tarball: the files, a per-tree `ls-R`, and the search paths with host directories dropped. It adds the 85 references and the sha256 of each corpus source.
- **Binary.** It records `PINNED_TEXLIVE` and `PINNED_LATEX` (2025-11-01, patch level 0), and warns at format build when another kernel is installed (`latex_version`). `RUSHTEX_KPSE_PATHS` forces the search-path file.
- **CI.** CI downloads the tarball from a release of the private repo, checks its sha256, unpacks it, builds the format, gates `parity.py zero --engine --pixels --all` (`RUSHTEX_PINNED_REFS`) and runs the leak audit.

## Rejected
- **Installing TeX Live 2026 in CI from tlnet.** It moves daily, and the historic archive keeps only a year's final state, which does not exist for 2026 until 2027. A full install is also about 1 GB.
- **Committing the 46 MB tarball to git.** It would bloat every clone.
- **Ubuntu apt TeX Live.** It is a different year, so it can never match.
- **Regenerating references in CI.** That needs pdfTeX, and a pinned reference must not move.

## Measured
- 1,111 files (34.2 MB) cover all 85 documents. The tarball is 46.1 MB and holds 5 trees, 85 references and 398 package revisions.
- With no TeX Live on `PATH`, small2e, testmath, listings and fancyhdr compile byte-identically to the host build.
- CI rehearsal on macOS: TeX Live off `PATH`, everything from the unpacked tarball, `RUSHTEX_PINNED_REFS=1`. Result: `parity.py zero --engine --pixels --all` 85/85.
- The snapshot-built format differs from the host's only in the build date. `\time` and `\day` are stored at format build, as in TeX: 0 and 20 under the fixed `SOURCE_DATE_EPOCH`, against 1208 and 21.

## Open
- `scripts/engine_suites.py` runs `pdftex -ini` live, so it stays a local gate.
- Yearly upgrade: install the new TeX Live, `parity.py score --build`, then `pin.py files`, `record` and `pack`. Then a new release tag, plus `PIN_TAG`/`PIN_SHA256` and `PINNED_LATEX`.
- A corpus document added or edited needs a repack (pinned mode refuses a stale reference).
