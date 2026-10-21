# testmath page 7: trivlist list depth, `\label` spaces, em units, proof trivlist, list labels
Author: abhi+claude | Date: 2026-10-19
Files: `src/layout/layout_engine.rs`, `src/expand/expand_mod.rs`, `src/frontend/frontend_parser.rs`, `src/frontend/frontend_ast.rs`, `src/config/config_mod.rs`, `src/backend/backend_scaled.rs`

## Context
testmath page 7 (Lemma 6.1 and its proof) matched 4%. A memcastle search showed the page builder already follows tex.web §980–1028, so the cause had to be in the content. Each fix below is a latex.ltx, amsthm or tex.web rule.

## Decision
- `\trivlist` does not raise `\@listdepth`. A list inside a theorem, `center` or proof uses level-1 parameters. The `Block::Aligned` arm of `layout_blocks` passes `depth`, not `depth + 1`.
- latex.ltx `\@bsphack`/`\@esphack`: when the skip before `\label` is positive, the spaces after it are ignored. Positive skip means a space, a single end of line, or an `\hspace` with positive width (`ends_with_hskip`, `Input::ignore_spaces`).
- `em` and `ex` in parser dimensions come from the document font: `\fontdimen6` and `\fontdimen5` of the roman face at the base size (`em_ex`, `dim_pt`, `x_height_pt`, `parse_glue3_pt_in`). They had been fixed at 11pt and 5.5pt for every class size.
- amsthm `proof` is a trivlist (`TrivlistKind::Proof`, wrapper `rushtexproof`, emitted with `emit` so it does not cover `proof` on the env stack). It runs `\par` first, so `\@topsepadd` is `\topsep` (6pt plus 6pt) plus `\partopsep`. It has `\@beginparpenalty`/`\@endparpenalty` and `\ignorespaces` after the head.
- A user `\renewcommand{\labelenumi..iv}` becomes `\item[{\labelenum<d>}]`. `\roman{enumi}` and the other counter commands read the open enumerate counters. The list environment is a group, so the label macros are saved at list begin and restored at list end (`list_label_saves`).
- List labels and enumerate markers use the current body face (italic in a plain-style theorem). The description label uses that face with bold added.
- An undefined `\ref`/`\pageref` is `\mbox{\normalfont\bfseries ??}` (latex.ltx `\@setref`). An unknown `\cite` key is `\mbox{\normalfont\bfseries ?}`.
- `\mbox`/`\hbox` end with `SpaceFactor(1000)`: a box appended in horizontal mode resets the space factor (tex.web §1076).

## Rejected
- Treating page 7 as a page-builder problem. pdfLaTeX's `\tracingpages` and ours agreed at every breakpoint up to the second enumerate; the difference was content.

## Measured
- testmath went from 21.1% to 25.3% (10737/42400). Page 7: 4% to 100%. Page 6: 89% to 95%. Page count is unchanged: 46 against pdfLaTeX's 41.
- Repros: lemma with enumerate 100%, the whole proof plus the paragraph after it 100%, a minimal proof 100%.
- sample2e, small2e and lppl stay at 100%. Tests: 234 pass, plus the 4 known NewCM failures. Clippy is clean.

## Open
- `\item \label{..}` in vertical mode leaves a whatsit, so the `\parskip` after it is a legal breakpoint (p=0 in pdfLaTeX's trace). Not modelled; page 7 matches without it.
- em/ex do not follow size changes inside the document (`\small`, `\large`). They use the document base size.
- `\mbox` content can still break across lines.
- amsthm `\qed` (`\hfill\quad\openbox`) is still approximated by `\hspace{1em}$\square$`.
- testmath page 9 onward still diverges (6%).
