#!/usr/bin/env python3
"""
Generate a realistic stress-test LaTeX document with ~40,000 words.
Includes random paragraphs with occasional bold/italic styling to benchmark
texrs line-breaking, page-packing, and PDF generation under heavy load.
"""

import argparse
import random
import sys
from pathlib import Path

WORDS = [
    "lorem", "ipsum", "dolor", "sit", "amet", "consectetur", "adipiscing", "elit",
    "sed", "do", "eiusmod", "tempor", "incididunt", "ut", "labore", "et", "dolore",
    "magna", "aliqua", "enim", "ad", "minim", "veniam", "quis", "nostrud",
    "exercitation", "ullamco", "laboris", "nisi", "aliquip", "ex", "ea", "commodo",
    "consequat", "duis", "aute", "irure", "in", "reprehenderit", "voluptate",
    "velit", "esse", "cillum", "fugiat", "nulla", "pariatur", "excepteur", "sint",
    "occaecat", "cupidatat", "non", "proident", "sunt", "culpa", "qui", "officia",
    "deserunt", "mollit", "anim", "id", "est", "laborum", "compiler", "renderer",
    "rust", "performance", "memory", "latency", "throughput", "cache", "vector",
    "optimization", "stream", "buffer", "metric", "typesetter", "glyph", "document",
    "layout", "paragraph", "format", "pipeline", "execution", "algorithm"
]

def generate_paragraph(target_words: int, rng: random.Random) -> str:
    words = []
    sentence_len = rng.randint(8, 18)
    capital_next = True

    for i in range(target_words):
        w = rng.choice(WORDS)
        if capital_next:
            w = w.capitalize()
            capital_next = False

        # Occasional styling: 3% bold, 3% italic
        r = rng.random()
        if r < 0.03:
            w = f"\\textbf{{{w}}}"
        elif r < 0.06:
            w = f"\\textit{{{w}}}"

        # End of sentence
        if (i + 1) % sentence_len == 0 and i < target_words - 1:
            w += "."
            capital_next = True
            sentence_len = rng.randint(8, 18)

        words.append(w)

    res = " ".join(words)
    if not res.endswith("."):
        res += "."
    return res

def main():
    parser = argparse.ArgumentParser(description="Generate LaTeX stress document.")
    parser.add_argument("--words", type=int, default=40000, help="Target word count (default: 40000)")
    parser.add_argument("-o", "--output", type=str, default="examples/stress_40k.tex", help="Output file path")
    parser.add_argument("--seed", type=int, default=42, help="RNG seed for reproducibility")
    args = parser.parse_args()

    rng = random.Random(args.seed)
    paragraphs = []
    words_generated = 0

    while words_generated < args.words:
        para_words = min(rng.randint(60, 120), args.words - words_generated)
        para = generate_paragraph(para_words, rng)
        paragraphs.append(para)
        words_generated += para_words

    content = "\n\n".join(paragraphs) + "\n"

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(content, encoding="utf-8")

    size_kb = len(content.encode("utf-8")) / 1024
    print(f"Generated {len(paragraphs)} paragraphs, {words_generated} words.")
    print(f"Output: {out_path} ({size_kb:.1f} KB)")

if __name__ == "__main__":
    main()
