# Plan: native translation for 20–50× speed, demand loading, debt guards
Author: abhi+claude | Date: 2026-10-22
Files: `memorycastle/journal/progress.md`, `memorycastle/journal/native-architecture.md`, `memorycastle/journal/native-port-plan.md`, `memorycastle/journal/2026-10-22-document-needs-measure.md`

## Context
After Z1, the user set the direction:
- at least 20× pdflatex on every document, aiming for 50×;
- bare-metal native translation only;
- every `.sty` as tables plus code, loading only what a document truly requires;
- no technical debt ("we were saved yesterday but that luck might not be with us forever").

## Decision
`progress.md` is reorganised into four phases.

**1. Foundations.**
- A0: an `oracle` cargo feature boundary that CI enforces.
- A1: one pinned TeX Live, 2023 as in CI.
- Z2–Z4: buffers reserved once; token model M; capacity reserved from the source size.
- B1–B4: copies removed and a hot-loop census, scoped to the core and the expander, the two parts that stay in the product.

**2. Tables and demand loading.**
- D1: one table library (fingerprinted perfect hash, SoA, CSR, bitsets, Roaring).
- D2: the package registry generated from oracle runs.
- D3: the source pre-scan with per-block needs and reads.
- D4: hyphenation, metrics and font programs loaded on demand.

**3. Native translation:** P0–P6, with each `.sty` split into generated data and one behaviour module.

**4. Product:** P7 with a speed gate of at least 20× on every corpus document; P8 incremental.

Debt guards are written as gates:
- one implementation per job (the old pipeline is frozen);
- the feature boundary;
- one pinned version with checksummed generated files;
- one table format;
- every native command proven against its macro version;
- overridable internals;
- an unported package is a release error;
- per-commit budgets.

## Rejected
- **Lazy undump of the oracle's format** (proposed earlier the same day). It saves about 3 MB in a component that leaves the release at P7. The effort goes to D1–D4, which survive into the native product.
- **A separate XOR filter in front of the name hash.** The fingerprinted perfect hash does both in one probe.
- **B-phase tuning of LaTeX macro paths in the interpreter.** Native translation deletes those paths; only the user-macro expander and the core are tuned.

## Open
- The speed ratio is reachable only per document whose every package is native. Documents dominated by their own macros (pgf/TikZ-style code) are bounded by the expander. B4 measures how much that costs.
- arXiv sample size and source for the package order (P0).
