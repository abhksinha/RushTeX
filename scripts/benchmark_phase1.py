#!/usr/bin/env python3
"""
benchmark_phase1.py
===================
Benchmarks RushTeX vs. pdfLaTeX on the Phase 1 100k-word stress test document
(examples/phase1_stress_100k.tex).

Measures:
  - Total elapsed wall-clock time (min, median, avg, stddev)
  - Layout & parsing throughput (words/sec, pages/sec)
  - Peak Resident Set Size (RSS memory in MB)
  - Output PDF file size (bytes)
  - Relative speedup vs. pdfLaTeX
"""

import os
import sys
import time
import subprocess
import statistics

TEX_FILE = "examples/phase1_stress_100k.tex"
RUSHTEX_BIN = "./target/release/rushtex"
OUTPUT_DIR = "examples"
RUSHTEX_PDF = os.path.join(OUTPUT_DIR, "phase1_rushtex.pdf")
PDFLATEX_PDF = os.path.join(OUTPUT_DIR, "phase1_pdflatex.pdf")

def count_words_in_file(path):
    with open(path, "r", encoding="utf-8") as f:
        return len(f.read().split())

def measure_process(cmd):
    """Runs a command via /usr/bin/time -v to measure wall clock and peak RSS."""
    time_cmd = ["/usr/bin/time", "-v"] + cmd
    t0 = time.perf_counter()
    res = subprocess.run(time_cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    t1 = time.perf_counter()
    wall_ms = (t1 - t0) * 1000.0
    
    max_rss_kb = 0
    for line in res.stderr.splitlines():
        if "Maximum resident set size" in line:
            parts = line.split(":")
            if len(parts) > 1:
                max_rss_kb = int(parts[1].strip())
                break
    return wall_ms, max_rss_kb, res.returncode

def benchmark_rushtex(tex_file, iters=15):
    print(f"\n[1/2] Benchmarking RushTeX on {tex_file} ({iters} iterations)...")
    # Warmup
    subprocess.run([RUSHTEX_BIN, tex_file, "-o", RUSHTEX_PDF], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    
    times = []
    rss_list = []
    for i in range(iters):
        wall_ms, rss_kb, code = measure_process([RUSHTEX_BIN, tex_file, "-o", RUSHTEX_PDF])
        assert code == 0, f"RushTeX failed on iteration {i}"
        times.append(wall_ms)
        rss_list.append(rss_kb / 1024.0) # MB
        
    return {
        "engine": "RushTeX (Rust)",
        "min_ms": min(times),
        "median_ms": statistics.median(times),
        "avg_ms": statistics.mean(times),
        "std_ms": statistics.stdev(times) if len(times) > 1 else 0.0,
        "max_rss_mb": max(rss_list),
        "pdf_size": os.path.getsize(RUSHTEX_PDF) if os.path.exists(RUSHTEX_PDF) else 0
    }

def benchmark_pdflatex(tex_file, iters=3):
    print(f"[2/2] Benchmarking pdfLaTeX on {tex_file} ({iters} iterations)...")
    cmd = ["pdflatex", "-interaction=batchmode", f"-jobname=phase1_pdflatex", f"-output-directory={OUTPUT_DIR}", tex_file]
    
    # Warmup
    subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    
    times = []
    rss_list = []
    for i in range(iters):
        wall_ms, rss_kb, code = measure_process(cmd)
        times.append(wall_ms)
        rss_list.append(rss_kb / 1024.0) # MB
        
    return {
        "engine": "pdfLaTeX (TeX Live 2023)",
        "min_ms": min(times),
        "median_ms": statistics.median(times),
        "avg_ms": statistics.mean(times),
        "std_ms": statistics.stdev(times) if len(times) > 1 else 0.0,
        "max_rss_mb": max(rss_list),
        "pdf_size": os.path.getsize(PDFLATEX_PDF) if os.path.exists(PDFLATEX_PDF) else 0
    }

def main():
    if not os.path.exists(TEX_FILE):
        print(f"Error: {TEX_FILE} not found. Run generate_phase1_benchmark.py first.")
        sys.exit(1)
        
    word_count = count_words_in_file(TEX_FILE)
    print(f"==================================================================")
    print(f" RUSHTEX PHASE 1 100K-WORD PARITY & PERFORMANCE BENCHMARK")
    print(f" Input: {TEX_FILE} ({word_count:,} words)")
    print(f"==================================================================")
    
    rush_res = benchmark_rushtex(TEX_FILE, iters=15)
    pdf_res = benchmark_pdflatex(TEX_FILE, iters=3)
    
    speedup = pdf_res["avg_ms"] / rush_res["avg_ms"]
    rush_words_per_sec = (word_count / (rush_res["avg_ms"] / 1000.0))
    rush_pages = 229 # from rushtex compilation
    rush_pages_per_sec = (rush_pages / (rush_res["avg_ms"] / 1000.0))
    
    print("\n" + "=" * 70)
    print(f"{'METRIC':<32} | {'RushTeX (Rust)':<16} | {'pdfLaTeX':<16}")
    print("-" * 70)
    print(f"{'Average Total Wall Time':<32} | {rush_res['avg_ms']:>10.2f} ms   | {pdf_res['avg_ms']:>10.2f} ms")
    print(f"{'Minimum Wall Time':<32} | {rush_res['min_ms']:>10.2f} ms   | {pdf_res['min_ms']:>10.2f} ms")
    print(f"{'Median Wall Time':<32} | {rush_res['median_ms']:>10.2f} ms   | {pdf_res['median_ms']:>10.2f} ms")
    print(f"{'Peak RSS Memory':<32} | {rush_res['max_rss_mb']:>10.2f} MB   | {pdf_res['max_rss_mb']:>10.2f} MB")
    print(f"{'Output PDF File Size':<32} | {rush_res['pdf_size']:>10,d} B    | {pdf_res['pdf_size']:>10,d} B")
    print("-" * 70)
    print(f"{'Throughput (Words / Sec)':<32} | {rush_words_per_sec:>10,.0f} w/s  | {word_count / (pdf_res['avg_ms']/1000.0):>10,.0f} w/s")
    print(f"{'Throughput (Pages / Sec)':<32} | {rush_pages_per_sec:>10,.1f} p/s  | {222 / (pdf_res['avg_ms']/1000.0):>10,.1f} p/s")
    print(f"{'RushTeX Acceleration Factor':<32} | {speedup:>10.1f}x      | {'1.0x':>10}")
    print("=" * 70)

if __name__ == "__main__":
    main()
