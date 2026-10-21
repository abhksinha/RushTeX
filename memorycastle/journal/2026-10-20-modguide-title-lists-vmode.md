# modguide: ltxguide class code, \thanks, list parameters per size, vertical-mode groups
Author: abhi+claude | Date: 2026-10-20
Files: `src/expand/expand_classes.rs`, `src/expand/expand_mod.rs`, `src/frontend/frontend_parser.rs`, `src/frontend/frontend_ast.rs`, `src/config/config_mod.rs`, `src/layout/layout_engine.rs`, `src/layout/layout_float.rs`, `src/layout/layout_pagebuilder.rs`, `src/index/index_toc.rs`

## Context
modguide (ltxguide class) is the first CTAN corpus document to compile; the work is to match it to pdfLaTeX.

## Decision
- Class code: `expand_classes.rs` holds what a class runs after `\LoadClass` (ltxguide: `\parskip` `\medskipamount`, `\MakeShortVerb`, tocdepth 2). `base_class` maps ltxguide to article geometry.
- `\thanks`: in `\title`/`\author`/`\date` (`title_arg`), a `\footnote` gets an `\@fnsymbol` mark (`thanks_mark`) in a zero-width `DimensionBox` (`\rlap`). `layout_footnote_body` unwraps it (`\hb@xt@1.8em{\hss mark}`). `insert_thanks` inserts the footnotes after the title (`\@thanks`). The counter resets at `\maketitle`.
- `\\` skips spaces and one line end (`skip_arg_space`), as `\@ifnextchar` does.
- `list_level_params(class, size, level)`: `\small` and `\footnotesize` have their own `\@listi` (size1x.clo). `\leftmargini` and `\labelsep` are ems of the class `\normalsize`.
- `Inline::VAdjust{pt,em,ex}`: em and ex are resolved with the face and size current at layout (`font_relative_dim`).
- `begins_in_vmode`: true after `\end{<trivlist env>}` anywhere on the line.
- A block-level `{`/`}` (block group) emits `ConfigChange::FontSize`, not an empty Para.

## Measured
- modguide: from 1.2% to 71.6% strict. Pages: 1:1, 2:100, 3:67, 4:8, 5:98, 6:100, 7:96.
- On page 1, our shrink ratio is 0.2197 against pdfTeX's 0.15768. Total shrink is equal at 30.722; our natural height is about 1.9pt too tall.

## Open
- testmath page 20 regressed from 100 to 99 (2 glyphs).
- Gap above "Abstract": ours 28 against ref 27 (`\small` `\baselineskip` 11 on the first line).
- TS1 `\textcopyright` and `\textasteriskcentered` metrics (cmsy stands in for them).

## Decision (addendum: modguide to 99.7%)
- §738 with §709: `shift_amount := s + half(w - width(char_box))`; `char_box` is the accent's width plus its italic correction (`resolve_metric` returns it). Checked with pdfTeX `\showbox` of `\Vec{V}` (shift 0.75868pt).
- `\@makecol` puts text and footnotes in one `\vbox to \@colht`. Its natural height holds the text's last depth but not the last footnote's (`last_footnote_depth` in `fire_page`).
- `{`/`}` between paragraphs is handled before the paragraph fallback in `parse_next_block`. Trivlist blocks pass `last_line_advance` to their first paragraph. TOC entries set it. Headings and verbatim correct their first interline glue by it.
- ltxguide `verbatim` (`ltxguide_verbatim`): always in vertical mode, and `\small` at `\normalsize`.
- Verbatim's `\@item` order: `\addvspace\@topsep`, then `\addvspace{-\parskip}`, then the item's `\parskip`. There is no `par_skip_glue` before it.
- §354 state S: after a control word the parser skips spaces and one line end.
- `EmSpace` uses the current face's quad (cmtt10's quad is 1.05em).
- cmss10 is shaped with its own kerns and ligatures.
- Logos are built at the painted size (`flatten_into` takes `size`).

## Measured (addendum)
- modguide: 71.6% to 99.7%. Pages 2–7 are at 99–100%. Page 1 is at 96%: only the TS1 © row is left.
- sample2e, small2e, lppl and testmath stay at 100%. Baseline re-saved.

## Open (addendum)
- TS1 and T1 fonts (cm-super SFRM) for ©, `\textasteriskcentered` and the EC documents.
- 4 OpenType math tests fail here: NewCM and STIX are not in the 2026basic install. This is the environment, not the code.
