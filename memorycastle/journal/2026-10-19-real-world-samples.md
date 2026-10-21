# Real-world LaTeX samples against pdfLaTeX
Author: abhi+claude | Date: 2026-10-19
Files: `src/config/config_mod.rs`, `src/frontend/frontend_parser.rs`, `src/expand/expand_mod.rs`, `src/backend/backend_fonts.rs`

## Context
The user asked to download sample LaTeX files from the internet, compile them with pdfLaTeX and rushtex, and find what is wrong and what is right. Files from CTAN: `sample2e.tex`, `small2e.tex` (LaTeX base), `testmath.tex` (amsmath test suite, 41 pages), `amsthdoc.tex` (amsthm guide, 15 pages), `lppl.tex` (the LaTeX licence, 8 pages). None of them is in the regression corpus, which explains why the corpus passes while these fail.

## Measured
- Out of the box: 2 of 5 produce a PDF; only `small2e` produces the right content. `lppl` produces 1 page (only a page number) instead of 8, with exit 0.
- `small2e` with `\usepackage{lmodern}` added: 158 of 160 words matched, median position deviation 0.01 pt, 90th percentile 0.14 pt, 99% within 0.5 pt of pdfLaTeX.
- `testmath`, after working around the hang below: 13.9 GB RSS after 58 s before the OS killed it.

## Open
Defects found, most severe first (each reproduced in a minimal file):
- A blank line inside a brace group (`\textit{a`, blank line, `b}`) makes `parse_until_rbrace` loop forever, pushing inlines without consuming input; memory grows without limit. A theorem with an optional argument followed by `\label` on its own line expands to exactly this.
- Silent content loss: `lppl.tex` defines its document environment with `\let\LPPLicense\document` inside `\ifx\@preamblecmds\@notprerr ... \else ... \fi`; rushtex writes an empty page and reports success. `\ifx\foo\undefined` in the preamble takes the false branch although both are undefined.
- Default font is `FontFamily::Times` (`src/config/config_mod.rs`); LaTeX's default is Computer Modern. Every corpus file loads `lmodern`, which hides this.
- Macro parameter substitution is textual: `\newcommand{\p}[1]{{\bfseries#1}}` with `word` becomes `\bfseriesword`.
- `\title{...}` arguments are expanded where they appear; LaTeX stores them until `\maketitle`, so macros defined after `\title` fail.
- Preamble detection fails when anything precedes `\documentclass` (`\NeedsTeXFormat{LaTeX2e}[...]`), and every later command becomes "unknown".
- Unsupported, each common in real documents: `\,` `\thinspace` `\quad` `\enspace` `\@` `\/` `\dots` `\ldots` (text mode) `\em` `\bfseries` `\itshape` `\ttfamily` `\sffamily` `\normalfont` `\textrm` `\textup` `\textsf` `\textsl` `\mbox` `\fbox` `\makebox` `\parbox` `\raisebox` `\footnote` `\url` `\hfill` `\linebreak` `\newline` `\underline` `\textsuperscript` `\S` `\P` `\copyright` `\pounds` `\oe` `\ss` `\textbar` `\ensuremath` `\phantom` `\rule` `\pagebreak` `\nopagebreak` `\cleardoublepage` `\begingroup`/`\endgroup` `\markboth` `\DeclareRobustCommand` (silently defines nothing) `\fontfamily`/`\selectfont` `\setlength\x{..}` without braces, environments `em` and `sloppypar`, and `\begin{X}` for environments made with `\let`.
- `\{` and `\}` in Computer Modern print the OT1 glyphs at those codes (`˙ –`, `˝`); LaTeX takes the braces from cmsy.
- In Times, a control space (`etc.\ are`) prints a `?` glyph.
- Error line numbers drift after macro expansion (an error at line 113 was reported at 106).

## Decision (follow-up: fixes 1–4)
- `Parser::parse_inline`: an empty text run before a blank line now yields `Inline::ParEnd` (TeX's `\par`, accepted by LaTeX's `\long` text commands) after consuming the blank lines; any other empty run is an error. This removes the unbounded loop in `parse_until_rbrace`.
- `parse_preamble_if_present`: reaching the end of the file inside a preamble is the error "missing \begin{document}" (as in LaTeX), and `\NeedsTeXFormat{..}[..]` is accepted before `\documentclass`.
- Expander: `\ifx`, `\ifdefined` and `\ifnum` trigger expansion; `resolve_env_alias` maps `\begin{X}`/`\end{X}` to the environment that `\X` was `\let` to; `same_meaning` treats two undefined names as equal when one is `\undefined`/`\@undefined` and the other is neither a user macro nor in `engine_defines` (pdfTeX `\pdf…` primitives and a few kernel names); `substitute` puts a space between a control word and a following argument that starts with a letter, and after an argument ending in a control word before a letter.
- Default `font_family` is `ComputerModern`; `\renewcommand\familydefault\rmdefault` returns to CM from Helvetica. 19 unit tests that encoded Times output now load `times` explicitly (they test structure, not the default); `test_default_font_is_times_roman` became `test_default_font_is_computer_modern`; three bare-body tests assert the CM kerns from the TFM.

## Measured (follow-up)
- Corpus: only the 11 documents without a font package changed (Times to CM). `stress_40k` now matches pdfLaTeX's page count (78 = 78); the 100k stress document gives 277 pages vs pdfLaTeX's 267.
- Harness 38/38 A; tests 226 pass with the 4 known NewCM failures; clippy clean.
- Samples after the fixes: `small2e` correct; `lppl` now reaches line 487 (`\paragraph` unsupported) instead of an empty page; `sample2e` stops at `\,`, `testmath` and `amsthdoc` at `\pkg` inside `\title`.

## Open (follow-up)
- The CM default exposed parity gaps the Times default hid: `lstlisting` defaults (rushtex draws a frame, visible spaces and a smaller font; listings' defaults have none of these) and an indent on the first paragraph after a heading in the 100k document; that document is 10 pages longer than pdfLaTeX's.
- `\textbf{\textit{..}}` in CM writes an unembedded "CMBX10" with WinAnsi encoding; LaTeX uses cmbxti10, which is not among the bundled fonts.
- `helvet` switches the whole document to Helvetica; in LaTeX it only changes `\sfdefault`.
- Benchmarks of the 100k stress document are no longer comparable with earlier numbers: it is now set in CM (one run per word) instead of Times.
