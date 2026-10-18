#!/usr/bin/env python3
"""
generate_phase1_benchmark.py
============================
Generates a comprehensive 100,000-word LaTeX stress benchmark document
specifically designed to exercise every feature implemented in Phase 1 of RushTeX:

  - Step 1A: Space factor tables (\\sfcode), punctuation spacing, and \\frenchspacing toggling.
  - Step 1B: Knuth-Plass 4-class fitness states (Tight, Decent, Loose, Very Loose) & demerits.
  - Step 1C: Knuth-Liang hyphenation dictionary, discretionary soft-hyphens (\\-), and compound hyphens.
  - Step 1D: Dynamic multi-line \\parshape matrices with geometric indentations and repeat tails.
  - Step 1E: Three-pass line breaking engine (\\pretolerance, \\tolerance, \\emergencystretch, \\sloppy, \\fussy).
  - Step 1F: Paragraph stretch tuning (\\looseness=1, \\looseness=-1) and persistent margins (\\leftskip, \\rightskip).
  - Step 1G: Paragraph termination & ragged/justified tuning (\\parfillskip=0pt, \\lastlinefit, \\prevgraf).

Target: 100,000+ words across ~1,000 structured paragraphs and sections.
"""

import sys
import os
import random

# Seed for deterministic reproduction
random.seed(42)

# Rich technical vocabulary for triggering Knuth-Liang hyphenation patterns and varying word lengths
WORDS_HYPHEN = [
    "computational", "characterization", "electromagnetism", "reproducibility",
    "interdisciplinary", "thermodynamic", "inconsequential", "parallelization",
    "proportionality", "superconducting", "cryptographic", "indistinguishable",
    "microarchitecture", "asynchronous", "deterministic", "optimization",
    "synchronization", "differentiation", "parameterization", "generalization",
    "initialization", "serialization", "multidimensional", "intercontinental",
    "justification", "hyphenation", "microtypography", "demerit", "architecture",
    "vectorization", "decompression", "discretionary", "specification", "foundational"
]

WORDS_COMPOUND = [
    "state-of-the-art", "cache-coherent", "zero-overhead", "high-throughput",
    "single-threaded", "memory-efficient", "lock-free", "content-sensitive",
    "low-latency", "hardware-accelerated", "bit-packed", "space-optimized"
]

WORDS_DISCRETIONARY = [
    r"macro\-typography", r"syn\-chro\-ni\-za\-tion", r"in\-con\-se\-quen\-tial",
    r"mul\-ti\-di\-men\-sion\-al", r"hy\-phen\-ation", r"de\-ter\-min\-is\-tic",
    r"pa\-ram\-e\-ter\-i\-za\-tion", r"re\-pro\-du\-ci\-bil\-i\-ty", r"com\-pu\-ta\-tion\-al"
]

WORDS_ABBREV = [
    "e.g.", "i.e.", "U.S.A.", "NASA", "IEEE", "Ph.D.", "etc.", "Dr. E. W. Dijkstra"
]

WORDS_COMMON = [
    "the", "of", "and", "to", "a", "in", "that", "is", "was", "he", "for", "it",
    "with", "as", "his", "on", "be", "at", "by", "this", "had", "not", "are", "but",
    "from", "or", "have", "an", "they", "which", "one", "you", "were", "her", "all",
    "she", "there", "would", "their", "we", "him", "been", "has", "when", "who", "will",
    "more", "no", "if", "out", "so", "said", "what", "up", "its", "about", "into",
    "than", "them", "can", "only", "other", "new", "some", "could", "time", "these",
    "two", "may", "then", "do", "first", "any", "my", "now", "such", "like", "our",
    "over", "man", "me", "even", "most", "made", "after", "also", "did", "many", "before",
    "must", "through", "back", "years", "where", "much", "your", "way", "well", "down",
    "should", "because", "each", "just", "those", "people", "how", "too", "little", "state",
    "good", "very", "make", "world", "still", "own", "see", "men", "work", "long", "get",
    "here", "between", "both", "life", "being", "under", "never", "day", "same", "another"
]

PUNCTUATIONS = [".", ".", ".", "!", "?", ":", ";", ","]

def generate_sentence():
    """Generates a pseudo-technical sentence with rich hyphenation words and punctuation."""
    length = random.randint(8, 22)
    tokens = []
    
    # Capitalized first word
    w0 = random.choice(WORDS_HYPHEN if random.random() < 0.3 else WORDS_COMMON).capitalize()
    tokens.append(w0)
    
    for _ in range(length - 1):
        r = random.random()
        if r < 0.25:
            tokens.append(random.choice(WORDS_HYPHEN))
        elif r < 0.35:
            tokens.append(random.choice(WORDS_COMPOUND))
        elif r < 0.42:
            tokens.append(random.choice(WORDS_DISCRETIONARY))
        elif r < 0.48:
            tokens.append(random.choice(WORDS_ABBREV))
        else:
            tokens.append(random.choice(WORDS_COMMON))
            
    punct = random.choice(PUNCTUATIONS)
    # If the last word is an abbreviation with a dot, avoid double dots
    if tokens[-1].endswith(".") and punct == ".":
        sentence = " ".join(tokens)
    else:
        sentence = " ".join(tokens) + punct
    return sentence

def generate_paragraph(min_words=90, max_words=125):
    """Generates a multi-sentence paragraph containing target word count."""
    sentences = []
    word_count = 0
    while word_count < min_words:
        s = generate_sentence()
        sentences.append(s)
        # Approximate word count in sentence
        word_count += len(s.split())
    return " ".join(sentences)

def build_benchmark_document(target_words=100000, output_path="examples/phase1_stress_100k.tex"):
    print(f"[INFO] Generating Phase 1 benchmark document targeting {target_words:,} words...")
    
    preamble = r"""% RushTeX Phase 1 Comprehensive Stress Benchmark
% Tests all features from Chapters 15-20 of TeX by Topic:
% Space factors (\sfcode, \frenchspacing), 4 fitness classes & demerits,
% Knuth-Liang hyphenation & discretionaries, \parshape multi-line matrix,
% Three-pass line breaking (\pretolerance, \tolerance, \emergencystretch, \sloppy, \fussy),
% Paragraph stretch tuning (\looseness, \leftskip, \rightskip),
% Paragraph termination & ragged/justified tuning (\parfillskip, \prevgraf, \lastlinefit).
\documentclass[11pt, a4paper]{article}
\usepackage[margin=1in]{geometry}

\title{RushTeX Phase 1 Comprehensive Line Breaking \& Paragraph Benchmark (100,000 Words)}
\author{RushTeX Architectural Validation Suite}
\date{September 2026}

\begin{document}
\maketitle

\setlength{\parindent}{15pt}
\setlength{\parskip}{4pt}
"""
    
    sections = [
        "Foundational Knuth-Plass Demerits and Multi-Class Traversal",
        "Knuth-Liang Discretionary Hyphenation and Compound Syllabification",
        "Dynamic Multi-Line Parshape Inset and Matrix Geometries",
        "Three-Pass Line Breaking Driver and Elasticity Tolerances",
        "Paragraph Stretch Tuning, Looseness Biasing, and Coordinate Skips",
        "Paragraph Termination, PrevGraf Propagation, and LastLineFit Adjustments",
        "Space Factor Code Adjustments, Punctuation Expansion, and French Spacing",
        "Combined High-Stress Microtypography and Non-Linear Constraint Matrices"
    ]
    
    total_words = 0
    para_count = 0
    sec_idx = 0
    doc_body = []
    
    # We will cycle through Phase 1 variations
    variation_cycle = 0
    num_variations = 15
    
    while total_words < target_words:
        # Emit a section every ~12,500 words
        if total_words >= sec_idx * (target_words // len(sections)):
            sec_name = sections[sec_idx % len(sections)]
            doc_body.append(f"\n\\section{{{sec_name} (Epoch {sec_idx + 1})}}\n")
            sec_idx += 1
            
        # Emit a subsection periodically
        if para_count > 0 and para_count % 15 == 0:
            doc_body.append(f"\n\\subsection{{Algorithmic Verification Subsystem {para_count // 15}}}\n")
            
        p_text = generate_paragraph(min_words=90, max_words=120)
        p_words = len(p_text.split())
        
        # Apply specific Phase 1 variations
        v = variation_cycle % num_variations
        variation_cycle += 1
        
        if v == 0:
            # Standard paragraph exercising space factors & Knuth-Liang hyphenation
            doc_body.append(p_text + "\n\n")
        elif v == 1:
            # French spacing block
            doc_body.append(r"\frenchspacing" + "\n" + p_text + "\n" + r"\nonfrenchspacing" + "\n\n")
        elif v == 2:
            # Non-french spacing with abbreviations and punctuated pauses
            doc_body.append(r"\nonfrenchspacing" + "\n" + p_text + "\n\n")
        elif v == 3:
            # Step 1D: Triangular / pyramid \parshape
            parshape_cmd = r"\parshape 5 0pt 420pt 20pt 380pt 40pt 340pt 60pt 300pt 80pt 260pt"
            doc_body.append(parshape_cmd + "\n" + p_text + "\n\n")
        elif v == 4:
            # Step 1D: Inset cut-out \parshape (lines 1-2 inset, then full width)
            parshape_cmd = r"\parshape 4 35pt 350pt 35pt 350pt 0pt 420pt 0pt 420pt"
            doc_body.append(parshape_cmd + "\n" + p_text + "\n\n")
        elif v == 5:
            # Step 1E: \sloppy line breaking
            doc_body.append(r"\sloppy" + "\n" + p_text + "\n" + r"\fussy" + "\n\n")
        elif v == 6:
            # Step 1E: Strict \fussy line breaking
            doc_body.append(r"\fussy" + "\n" + p_text + "\n\n")
        elif v == 7:
            # Step 1E: Three-pass parameter tuning
            doc_body.append(r"\pretolerance=120" + "\n" + r"\tolerance=350" + "\n" + r"\emergencystretch=15pt" + "\n" + p_text + "\n\n")
        elif v == 8:
            # Step 1F: \looseness=1 (force Knuth-Plass DP to seek 1 extra line)
            doc_body.append(r"\looseness=1" + "\n" + p_text + "\n\n")
        elif v == 9:
            # Step 1F: \looseness=-1 (force Knuth-Plass DP to compress by 1 line)
            doc_body.append(r"\looseness=-1" + "\n" + p_text + "\n\n")
        elif v == 10:
            # Step 1F: Persistent left and right skips
            doc_body.append(r"\leftskip=25pt" + "\n" + r"\rightskip=25pt" + "\n" + p_text + "\n" + r"\leftskip=0pt" + "\n" + r"\rightskip=0pt" + "\n\n")
        elif v == 11:
            # Step 1F: Hanging indentation (\hangindent + \hangafter)
            doc_body.append(r"\hangindent=35pt" + "\n" + r"\hangafter=2" + "\n" + p_text + "\n\n")
        elif v == 12:
            # Step 1G: Zero-stretch \parfillskip forcing full justification on final line
            doc_body.append(r"\parfillskip=0pt" + "\n" + p_text + "\n" + r"\parfillskip=0pt plus 1fil" + "\n\n")
        elif v == 13:
            # Step 1G: Proportional \lastlinefit and \prevgraf tracking
            doc_body.append(r"\parfillskip=20pt plus 1fil" + "\n" + r"\lastlinefit=1000" + "\n" + r"\prevgraf=4" + "\n" + p_text + "\n" + r"\parfillskip=0pt plus 1fil" + "\n" + r"\lastlinefit=0" + "\n\n")
        elif v == 14:
            # Step 1C: Heavy discretionary soft-hyphens and compound hyphen words
            doc_body.append(p_text + "\n\n")
            
        total_words += p_words
        para_count += 1
        
        if para_count % 100 == 0:
            print(f"  -> Generated {para_count} paragraphs ({total_words:,} words)...")
            
    doc_body.append("\n\\end{document}\n")
    
    full_content = preamble + "".join(doc_body)
    
    os.makedirs(os.path.dirname(output_path), exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        f.write(full_content)
        
    actual_file_words = len(full_content.split())
    file_size_mb = os.path.getsize(output_path) / (1024 * 1024)
    print(f"[SUCCESS] Wrote {output_path}:")
    print(f"  - Paragraphs: {para_count:,}")
    print(f"  - Total words: {actual_file_words:,}")
    print(f"  - File size: {file_size_mb:.2f} MB")
    return actual_file_words

if __name__ == "__main__":
    words_target = 100000
    if len(sys.argv) > 1:
        words_target = int(sys.argv[1])
    build_benchmark_document(target_words=words_target)
