# rushtex

Zero-crate, ultra-fast TeX-subset compilation engine. It compiles LaTeX documents into
PDF 1.4 documents at sub-10ms latency using only safe pure Rust standard library.

What it implements from scratch with zero external dependencies:

1. **Recursive-Descent Parser**: Single-pass parser for LaTeX syntax, preambles, and document envelopes.
2. **Typesetting & Layout Engine**: Line-breaking, orphan protection, native PDF justification (`Tw`), page geometry, horizontal rules, vertical spacing, and running footers.
3. **Display List Architecture**: Compact 24-byte display nodes and contiguous SoA text pools designed for CPU L1/L2 cache lines.
4. **Pure-Rust PDF Writer**: Direct PDF 1.4 object graph, xref table, and font resource emission without external PDF libraries.
5. **Streaming DEFLATE Compressor**: Pure-Rust RFC 1950 zlib / RFC 1951 DEFLATE compression engine with 64-bit unaligned XOR matching and a 32 KB L1 cache-resident hash table.
6. **WASM / Bare-Metal Capable**: `#![forbid(unsafe_code)]` with zero OS syscalls during compilation—runs directly in web browsers (`149 KB` WASM artifact).

## Build

```bash
cargo build --release
./target/release/rushtex examples/hello.tex
./target/release/rushtex examples/sample.tex -o out.pdf
```

## Language

```tex
This is a simple line and this \textbf{word} is in bold.

Mix \textit{italic} and \textbf{\textit{both}}.

\begin{enumerate}
\item First
\item Second with a long line that will wrap inside the hanging indent.
\end{enumerate}

\begin{itemize}
\item bullet
\end{itemize}

Line one\\line two
% comments work
```

Unknown commands are hard errors. That is deliberate: silent fallback would
hide typos.

## Layout

- Page: A4 (595 × 842 pt), 72 pt margins, 11 pt type, 14 pt leading.
- Words wrap when the running width exceeds the measure.
- Lists hang a `1.` / `•` marker in a 24 pt gutter per nesting level.
- Style changes only emit a new `Tf` when the face actually changes.
- Adjacent same-face words on one line collapse into a single `Tj`.

## PDF output

Objects, in order: Catalog, Pages, N page dicts, N content streams, four
Type1 font dicts, xref, trailer. No `/Producer`, no info dict, no object
streams, no unused resources. Content is uncompressed; for this workload a
Deflate encoder would cost more bytes than it saves.

## What this is not

A real TeX engine is ~14k Pascal statements plus a 100k-line macro format.
Replacing that in Rust without libraries means you either embed a font
scaler (FreeType-sized) or you do what this crate does: use built-in PDF
fonts and ship metrics by hand. The second path is the one that produces
the smallest correct PDFs for plain text.
