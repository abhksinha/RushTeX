# texrs

Zero-crate TeX-subset typesetter. It reads a tiny TeX dialect and writes a
PDF 1.4 file using only the Rust standard library.

This is not pdfTeX. There is no hyphenation, no math, no boxes-and-glue
optimizer, and no font embedding. What it *does* implement from scratch:

1. A recursive-descent parser for a useful fragment of TeX.
2. Greedy line breaking with real Adobe AFM widths.
3. Multi-page packing on A4.
4. A PDF object/xref writer that never calls a PDF library.

Bold and italic are the PDF Standard-14 faces
`Helvetica-Bold` / `Helvetica-Oblique` / `Helvetica-BoldOblique`. Viewers
already know those fonts, so the file stays tiny — no embedded TTF.

## Build

```bash
cargo build --release
./target/release/texrs examples/hello.tex
./target/release/texrs examples/sample.tex -o out.pdf
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
