# testmath pages 5–6: genfrac is Ord, cite separator, italic-correction kern breakpoints
Author: abhi+claude | Date: 2026-10-19
Files: `src/math/math_parser.rs`, `src/expand/expand_mod.rs`, `src/layout/layout_linebreak.rs`, `src/layout/layout_engine.rs`, `src/lib.rs`, `src/math/math_tests.rs`

## Context
After `memorycastle/journal/2026-10-19-testmath-trivlist-labels-units.md`, page 5 (display 23) and page 6 (two justified lines) still drifted by 0.1–1.7pt.

## Decision
- `\binom`, `\dbinom`, `\tbinom` and `\genfrac` with delimiters are built as `Delimited`, which is Inner. amsmath wraps `\genfrac` in braces, so `genfrac_ord` coerces them to Ord. The primitive `\atopwithdelims`/`\choose` family stays Inner (tex.web §761). The earlier note already said this; the delimited builders had been missed.
- `\cite` separator: latex.ltx `\@citea` is `,\penalty\@m\ `. We emit `,\@ ` (space factor 1000). The break penalty of 1000 is not modelled; there is no inline `\penalty`.
- The italic correction `\/` is an explicit kern. It is recorded in `ParagraphSoA.kerns` by `add_italic_correction` (called from `check_icr`/`check_icl`), and `build_nodes` emits it as `NodeKind::Kern` after the word box.
  - It is a breakpoint when glue follows (§866 `kern_break`), and the glue after it is not (§868).
  - It is discardable (§879).
  - A line that breaks there excludes its width; pdfLaTeX's box dump shows `\kern 0.0` at the line end (§881).
  - Before this, a line ending in `\emph{..}` or `\textit{..}` was the correction's width (about 0.75pt) too long, so its spaces shrank too much.

## Rejected
- Making amsmath `\frac` Inner after misreading display 23. `memcastle search` showed amsmath fractions are Ord; the real cause was `\binom`.

## Measured
- testmath went from 25.3% to 25.7%. Pages 5 and 6 are now at 100%; pages 1–7 are all at 99–100%. Page count is still 46 against 41.
- Repros: display (23) 100%; a synthetic paragraph with italic words at line ends 100% (it was 71.3%).
- sample2e, small2e and lppl stay at 100%. Tests: 234 pass, plus the 4 known NewCM failures. Clippy is clean.
- `ParagraphSoA` grew from 176 to 200 bytes (one sparse `Vec`).

## Open
- Latency was not measurable: load average was 25, with other `rustc` and `memcastle` processes using the CPU. Re-measure the manual and testmath against 5.5ms and 7.2ms.
- A display that starts a paragraph in vertical mode (`\[` right after `\begin{document}` or a blank line) should first set an empty indented line. Ours is 11pt too high in a repro; testmath does not hit this.
- `\penalty` in horizontal mode (the `\@citea` penalty of 1000).

## Decision (page 8: make_ord kerns in breakable inline math)
- An inline formula with breakpoints (after Bin/Rel) becomes one line-breaker item per atom, each measured alone. Those items lost TeX's §752 kern between neighbours (for example CMMI `H` with the math comma, −0.556pt).
- `char_pair_adjust` in `src/math/math_layout.rs` is now the single rule for both the Row layout and the split path: alphabet pairs, the TFM kern, and dropping the italic correction in text fonts. `inline_pair_adjust` gives the split path that same adjustment, added to the earlier atom's item width.
- The line painter advances a math item by its item width (`words_soa.widths[i]`), not by the rendered box width, so painting matches what the line breaker measured.

## Measured (page 8)
- testmath pages 1–8 are now all at 99–100%. Total 25.7%.
