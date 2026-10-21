# Page builder: LaTeX's \addpenalty, \vspace and legal breakpoints
Author: abhi+claude | Date: 2026-10-19
Files: `src/layout/layout_pagebuilder.rs`, `src/layout/layout_engine.rs`, `texlive-source/texk/web2c/tex.web`

## Context
`docs/manual.tex` broke pages differently from pdfLaTeX (e.g. page 11 kept `\subsection` 6.1 that pdfLaTeX moved). Each mismatch was located by comparing pdfTeX's `\tracingpages=1` log (t, stretch, shrink, penalty per breakpoint) with the same values from `page_breakpoint`, then fixed from the macro or tex.web section that produces it.

## Decision
- `page_addpenalty` implements latex.ltx `\addpenalty`: the penalty goes before the last skip, so that skip's width, stretch and shrink are not on the page there. Used for headings, lists, quotes, verbatim.
- `zero_lastskip`: `\vspace` (`\medskip`, `\bigskip`) ends with `\vskip\z@skip`, so `\lastskip` is 0 after it.
- `tail_discardable`: glue after glue or a penalty is no legal breakpoint (tex.web §973).
- `note_glue`: the page total at a breakpoint includes the last box's depth and the glue after it (§1004).
- `trivlist_topsep`: `\@topsep` is `\@topsepadd` plus `\parskip` (`\@trivlist`); verbatim and center use full glue.
- Heading afterskip is glue with .2ex stretch; after `\hrule` a heading gets no interline glue.
- Lines ending in an explicit hyphen get `\brokenpenalty`.

## Measured
- `docs/manual.tex` breakpoints identical to pdfTeX's trace on pages 4, 5, 6, 10, 12 (all values). Page count 25 = 25 (was 26 mid-way).
- Glyphs within 0.1pt: manual 47.4% to 60.1%.

## Open
- pdfTeX lists breakpoints for material carried to a new page (t=11.0 ...); `fire_page` does not re-cost them.
- `lstlisting` start: pdfTeX has a glue breakpoint after `\lst@aboveskip`, ours none; ours has one after the listing end that pdfTeX lacks. Read `listings.sty` for the exact item order.
- `\begin{figure}[h]` on page 13: pdfTeX places it here, rushtex defers it to a float page.
- A float in vertical mode: pdfTeX's `\penalty-\@Miv` special output.
