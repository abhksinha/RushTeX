# CTAN corpus expansion: ten more documents
Author: abhi+claude | Date: 2026-10-20
Files: `scripts/parity/corpus/`, `scripts/parity/parity.py`

## Context
sample2e, small2e, lppl and testmath all reached 100% strict. Only testmath (41 pages) and lppl (8 pages) have real volume. The user asked for 10 more CTAN examples, with no TikZ, to match in our own optimised style.

## Decision
- Added ten documents to `scripts/parity/corpus/` (237 pages in total):

  | Document | Pages | Notes |
  |---|---|---|
  | `fntguide` | 46 | ltxguide class |
  | `amsldoc` | 44 | math, index |
  | `clsguide` | 37 | ltxguide class |
  | `usrguide` | 30 | ltxguide class |
  | `encguide` | 30 | Latin Modern |
  | `short-math-guide` | 21 | rsfs, eufm |
  | `cfgguide` | 12 | ltxguide class |
  | `first-latex-doc` | 9 | PNG screenshots, `picture` |
  | `modguide` | 7 | ltxguide class |
  | `bare_jrnl` | 1 | IEEEtran, two columns, Times |

- Also added `latex-first.png` and `latex-second-a/b/c.png`, which `first-latex-doc` includes.
- pdfLaTeX compiles all ten with no errors.
- The new documents are not in `DEFAULT_DOCS`, so the default score stays fast. Name them explicitly to score them.
- TeX Live 2026basic lacked packages. They were installed with `tlmgr --usermode` into `~/Library/texmf`, because the system tree is root-owned. The packages are imakeidx, ieeetran, csquotes, footmisc, fewerfloatpages, hyperref, etoolbox, hypdoc, rcsinfo and rsfs.

## Rejected
- `bare_conf`: a near-duplicate of `bare_jrnl`.
- `booktabs.dtx`: it does not compile as a document ("Improper alphabetic constant").
- `l2tabuen` and `latex-doc-ptr`: they need the eurosans and spectral fonts, which are not Computer Modern.

## Measured
Baseline: rushtex compiles none of the ten. The first error in each is:

| Document | First error |
|---|---|
| `fntguide`, `cfgguide` | `\LaTeXe` unknown |
| `clsguide`, `usrguide`, `modguide` | unknown command early in the ltxguide preamble |
| `encguide` | unknown command at line 195 |
| `amsldoc` | macro nesting too deep at line 304 |
| `short-math-guide` | macro nesting too deep (`\expandafter`) at line 978 |
| `first-latex-doc` | `picture` environment unsupported |
| `bare_jrnl` | `\title` rejected at line 361 |

## Open
- pdfLaTeX's output uses fonts beyond the bundled CM set:
  - fntguide, clsguide and usrguide use T1 EC fonts, rendered as Type 3 bitmaps because cm-super is not installed;
  - encguide uses Latin Modern;
  - bare_jrnl uses Times (Nimbus).
- Planned order:
  1. ltxguide and `\LaTeXe` (unblocks five documents);
  2. the expansion-depth errors in amsldoc and short-math-guide;
  3. `picture`;
  4. IEEEtran two-column layout;
  5. then strict scoring per document.
