# Page cache hit skips the font-usage scan
Author: abhi+claude | Date: 2026-10-19
Files: src/backend/backend_pdf.rs

## Context
The recompile profile in `memorycastle/journal/2026-10-19-incremental-recompile.md` put `push_page`
at 12%, named as the font-usage scan and hashing. On a page-cache hit the
stream was reused but every op was still walked for faces, masters and CID
glyph ids.

## Decision
`PageCache` holds a `CachedPage`: the stream plus `faces`, `masters` and the
page's sorted CID glyph ids. A hit ORs the bits in and inserts the ids;
the op walk runs only when the stream is built. Byte identity holds
(`session_recompiles_match_fresh_compiles`).

## Rejected
- Keeping the whole `cid_set` per page: a `CidBitSet` is 1 KB of words per
  page; the id list is a few bytes per distinct glyph.

## Measured
- Recompile, session warm, median of 15, this machine today: `stress_500k`
  50.3 ms before, 48.6 to 49.1 ms after on four runs (about 3%);
  `stress_40k` 3.4 to 4.3 ms both ways, within noise. The scan was a small
  part of the 12%; the hash over the ops is the rest.
