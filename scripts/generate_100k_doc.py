#!/usr/bin/env python3
r"""
Generate a comprehensive 100,000-word LaTeX stress-test document.
Exercises the full feature set of rushtex and standard LaTeX:
  - Paragraph indentation control (\setlength{\parindent}, \indent, \noindent, \hangindent, \hangafter)
  - Enumeration and itemize lists with enumitem-style bracket options (labelsep, leftmargin, itemindent)
  - Tables (\begin{tabular} with column alignment and \hline)
  - Figures with \includegraphics and \caption
  - Code listings (\begin{lstlisting}[numbers=left] and \begin{verbatim})
  - Typography (\textbf, \textit, \texttt, \emph, \verb, escaped characters)
  - Headings (\section, \subsection, \subsubsection)
  - Spacing and dividers (\bigskip, \medskip, \smallskip, \hrule)
"""

import argparse
import random
from pathlib import Path

WORDS = [
    "algorithm", "allocation", "architecture", "benchmark", "buffer", "cache",
    "compiler", "compression", "concurrency", "cpu", "data", "deflate",
    "descriptor", "diagnostic", "document", "efficiency", "emission", "engine",
    "enumeration", "execution", "figure", "font", "format", "glyph",
    "greedy", "hardware", "heading", "hrule", "indentation", "inline",
    "justification", "latency", "layout", "line", "listing", "locality",
    "margin", "marker", "memory", "metric", "microsecond", "module",
    "monospace", "noindent", "optimization", "page", "paragraph", "parse",
    "parser", "performance", "pipeline", "preamble", "pt", "reduction",
    "register", "renderer", "rss", "rust", "safe", "scale",
    "section", "separation", "simd", "slice", "soa", "spacing",
    "stream", "stress", "structure", "system", "table", "tabular",
    "target", "tex", "throughput", "token", "typesetter", "typesetting",
    "uncompressed", "vector", "verbatim", "vspace", "width", "zero"
]

CODE_SNIPPETS = [
    """pub fn process_tokens<'a>(tokens: &'a [Token<'a>]) -> Result<Document<'a>, Error> {
    let mut ast = Vec::with_capacity(tokens.len());
    for token in tokens {
        if let Some(block) = parse_block(token)? {
            ast.push(block);
        }
    }
    Ok(Document { blocks: ast })
}""",
    """fn calculate_line_indent(line_idx: usize, first_indent: f32, hang: f32) -> f32 {
    if line_idx == 0 {
        first_indent
    } else {
        hang
    }
}""",
    """struct LineDescriptor {
    start_word: u32,
    word_count: u32,
    line_w: f32,
    num_spaces: u32,
    line_indent: f32,
    justify: bool,
}""",
    """pub fn emit_pdf_stream(ops: &[Paint], out: &mut Vec<u8>) -> Result<(), Error> {
    let mut compressor = DeflateCompressor::new();
    compressor.compress_stream(ops, out)?;
    Ok(())
}"""
]

def generate_sentence(rng: random.Random, min_words=8, max_words=18) -> (str, int):
    n = rng.randint(min_words, max_words)
    words = []
    for i in range(n):
        w = rng.choice(WORDS)
        if i == 0:
            w = w.capitalize()

        # 3% bold, 3% italic, 2% mono, 1% escaped char
        r = rng.random()
        if r < 0.03:
            w = f"\\textbf{{{w}}}"
        elif r < 0.06:
            w = f"\\textit{{{w}}}"
        elif r < 0.08:
            w = f"\\texttt{{{w}}}"
        elif r < 0.09:
            w = f"\\emph{{{w}}}"
        elif r < 0.095:
            sym = rng.choice(["\\%", "\\$", "\\&", "\\#", "\\_"])
            w = f"{w} {sym}"

        words.append(w)
    return " ".join(words) + ".", n

def generate_paragraph(rng: random.Random, target_words: int, prefix="") -> (str, int):
    sentences = []
    total = 0
    while total < target_words:
        s, cnt = generate_sentence(rng)
        sentences.append(s)
        total += cnt
    text = " ".join(sentences)
    if prefix:
        text = f"{prefix} {text}"
    return text, total

def generate_table(rng: random.Random, section_num: int) -> (str, int):
    rows = []
    rows.append("\\begin{tabular}{|l|c|r|}")
    rows.append("\\hline")
    rows.append("\\textbf{Parameter} & \\textbf{Observed Metric} & \\textbf{Throughput (ops/s)} \\\\")
    rows.append("\\hline")
    for r in range(4):
        p_name = rng.choice(WORDS).capitalize() + f" {r+1}"
        obs = f"{rng.uniform(1.0, 99.9):.2f} ms"
        tput = f"{rng.randint(10000, 95000):,}"
        rows.append(f"{p_name} & {obs} & {tput} \\\\")
        rows.append("\\hline")
    rows.append("\\end{tabular}")
    table_text = "\n".join(rows)
    # Estimate word count
    words = len(table_text.split())
    return table_text, words

def generate_figure(rng: random.Random, fig_num: int) -> (str, int):
    caption_word = rng.choice(WORDS).capitalize()
    fig_text = (
        f"\\begin{{figure}}\n"
        f"\\includegraphics{{figure_{fig_num}.png}}\n"
        f"\\caption{{Architecture layout and benchmark topology for {caption_word} stage {fig_num}.}}\n"
        f"\\end{{figure}}"
    )
    return fig_text, len(fig_text.split())

def generate_listing(rng: random.Random, idx: int) -> (str, int):
    code = CODE_SNIPPETS[idx % len(CODE_SNIPPETS)]
    listing_text = f"\\begin{{lstlisting}}[numbers=left]\n{code}\n\\end{{lstlisting}}"
    return listing_text, len(code.split())

def generate_verbatim(rng: random.Random) -> (str, int):
    verb_text = (
        "\\begin{verbatim}\n"
        "cargo bench --bench throughput -- --nocapture\n"
        "   Compiling rushtex v0.1.0\n"
        "   Running target/release/deps/benchmark\n"
        "   Throughput: 53,531 documents/sec (18.68 us/doc)\n"
        "\\end{verbatim}"
    )
    return verb_text, len(verb_text.split())

def generate_list(rng: random.Random, ordered: bool, custom_options: bool) -> (str, int):
    env = "enumerate" if ordered else "itemize"
    opts = ""
    if custom_options:
        labelsep = rng.randint(8, 16)
        leftmargin = rng.randint(24, 38)
        itemindent = rng.randint(0, 8)
        opts = f"[labelsep={labelsep}pt, leftmargin={leftmargin}pt, itemindent={itemindent}pt]"

    items = []
    total_words = 0
    num_items = rng.randint(3, 5)
    for i in range(num_items):
        # Generate 25-45 words per item to test hanging line wrapping
        item_text, wcnt = generate_paragraph(rng, rng.randint(25, 45))
        items.append(f"\\item {item_text}")
        total_words += wcnt

    content = "\n".join(items)
    list_str = f"\\begin{{{env}}}{opts}\n{content}\n\\end{{{env}}}"
    return list_str, total_words

def main():
    parser = argparse.ArgumentParser(description="Generate 100k-word comprehensive LaTeX stress document.")
    parser.add_argument("--words", type=int, default=100000, help="Target word count (default: 100,000)")
    parser.add_argument("-o", "--output", type=str, default="examples/stress_100k.tex", help="Output file path")
    parser.add_argument("--seed", type=int, default=42, help="RNG seed for reproducibility")
    args = parser.parse_args()

    rng = random.Random(args.seed)
    blocks = []
    total_words = 0

    section_idx = 0
    subsection_idx = 0
    fig_idx = 0

    print(f"Generating ~{args.words:,} words with comprehensive LaTeX feature set...")

    while total_words < args.words:
        section_idx += 1
        sec_title = f"Section {section_idx}: {rng.choice(WORDS).capitalize()} Architecture and Analysis"
        blocks.append(f"\\section{{{sec_title}}}")

        # 1. Forced indent paragraph after section
        p1, w1 = generate_paragraph(rng, rng.randint(80, 130), prefix="\\indent")
        blocks.append(p1)
        total_words += w1

        # 2. Subsection
        subsection_idx += 1
        sub_title = f"Subsection {section_idx}.{subsection_idx % 4 + 1}: Memory Locality and Layout"
        blocks.append(f"\\subsection{{{sub_title}}}")

        # 3. Standard paragraph with parindent
        p2, w2 = generate_paragraph(rng, rng.randint(90, 140))
        blocks.append(p2)
        total_words += w2

        # 4. Noindent paragraph
        p3, w3 = generate_paragraph(rng, rng.randint(70, 120), prefix="\\noindent")
        blocks.append(p3)
        total_words += w3

        # 5. Hanging indent paragraph
        hang_pt = rng.choice([20, 24, 30])
        hang_prefix = f"\\hangindent {hang_pt}pt\n\\hangafter 1\n\\noindent"
        p4, w4 = generate_paragraph(rng, rng.randint(80, 130), prefix=hang_prefix)
        blocks.append(p4)
        total_words += w4

        # 6. Enumeration with custom enumitem-style options
        ordered_list, w_list = generate_list(rng, ordered=True, custom_options=True)
        blocks.append(ordered_list)
        total_words += w_list

        # 7. Itemize with bullet options
        bullet_list, w_b_list = generate_list(rng, ordered=False, custom_options=True)
        blocks.append(bullet_list)
        total_words += w_b_list

        # 8. Subsubsection
        blocks.append(f"\\subsubsection{{Detailed Evaluation Matrix {section_idx}}}")

        # 9. Table with column alignments
        tbl, w_tbl = generate_table(rng, section_idx)
        blocks.append(tbl)
        total_words += w_tbl

        # 10. Horizontal divider
        blocks.append("\\medskip\n\\hrule\n\\medskip")

        # 11. Code listing
        code_block, w_code = generate_listing(rng, section_idx)
        blocks.append(code_block)
        total_words += w_code

        # 12. Verbatim block every 3 sections
        if section_idx % 3 == 0:
            verb_block, w_verb = generate_verbatim(rng)
            blocks.append(verb_block)
            total_words += w_verb

        # 13. Figure every 2 sections
        if section_idx % 2 == 0:
            fig_idx += 1
            fig_block, w_fig = generate_figure(rng, fig_idx)
            blocks.append(fig_block)
            total_words += w_fig

        # 14. Periodic explicit page break every 10 sections
        if section_idx % 10 == 0:
            blocks.append("\\newpage")

        if section_idx % 5 == 0:
            print(f"  Progress: {total_words:,} / {args.words:,} words ({section_idx} sections generated)...")

    preamble = (
        "% Comprehensive 100,000-Word Benchmark Document\n"
        "% Compatible with both pdflatex and rushtex\n"
        "\\documentclass[11pt, a4paper]{article}\n"
        "\\usepackage[margin=1in]{geometry}\n"
        "\\usepackage{enumitem}\n"
        "\\usepackage{listings}\n"
        "\\usepackage[draft]{graphicx}\n\n"
        "\\title{Comprehensive 100,000-Word Vector Typesetting Stress Test}\n"
        "\\author{rushtex Automated Performance Benchmarking Suite}\n"
        "\\date{September 2026}\n\n"
        "\\begin{document}\n"
        "\\maketitle\n\n"
        "\\setlength{\\parindent}{18pt}\n"
        "\\setlength{\\parskip}{5pt}\n"
        "\\setlength{\\labelsep}{10pt}\n\n"
    )

    body = "\n\n".join(blocks)
    full_document = preamble + body + "\n\n\\end{document}\n"

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(full_document, encoding="utf-8")

    size_mb = len(full_document.encode("utf-8")) / (1024 * 1024)
    print(f"\nCompleted!")
    print(f"  Output file : {out_path}")
    print(f"  Total words : {total_words:,}")
    print(f"  Sections    : {section_idx}")
    print(f"  Subsections : {subsection_idx}")
    print(f"  Figures     : {fig_idx}")
    print(f"  File size   : {size_mb:.2f} MB")

if __name__ == "__main__":
    main()
