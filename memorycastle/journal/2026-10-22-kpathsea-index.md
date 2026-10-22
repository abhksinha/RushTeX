# File lookup as kpathsea, from an `ls-R` index instead of a `kpsewhich` per file
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_host.rs`, `src/engine/engine_run.rs`, `src/main.rs`

## Context
`format_smoke` found every file with its own `kpsewhich` process, at about 30 ms each. That was 491 ms of small2e's run and 2.3 s of polydemo's, far more than the typesetting.

## Decision
`engine_host::Kpse` follows kpathsea's rules, ported from `pathsearch.c`, `db.c` (`match`, `kpathsea_db_search`) and `tex-file.c`, in our own representation:
- The search paths come from one `kpsewhich -progname=pdflatex -show-path` per format, all spawned at once and cached in a file keyed by the environment.
- The `ls-R` databases are read into one flat array of (FNV hash, directory) entries, sorted stably by hash.
- `//` elements without a database are walked from disk once.
- The suffix picks the format; any other suffix is searched as `\input` searches (the tex format).

## Rejected
- **`kpsewhich -interactive`.** It never flushes its output, so a pipe round trip deadlocks; misses also print nothing.
- **A `HashMap` from name to a list of directories.** It allocates for every entry; the sorted flat array needs one binary search and no allocation per entry.

## Measured
- File lookups: small2e from 491 ms to 22 ms, including building the index; sample2e from 632 ms to 4.7 ms; polydemo from 2.29 s to 13 ms.
- The local TeX Live 2026 basic `ls-R` is 450 KB.
- 85 of 85 documents stay at zero shift.

## Open
- kpathsea's second pass for must-exist files is not ported: it searches the disk for elements a database covers but where the database misses the file. No such element exists in this installation.
- Font-map aliases (`texfonts.map`) are not ported.
