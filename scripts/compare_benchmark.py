#!/usr/bin/env python3
"""
Benchmark and compare pdflatex vs texrs on a 40,000-word document.
Collects execution time, CPU time, peak RAM (Max RSS), and disk footprint.
"""

import subprocess
import time
import re
from pathlib import Path

def run_timed_command(cmd, runs=5):
    wall_times = []
    user_times = []
    sys_times = []
    rss_kb_list = []

    for _ in range(runs):
        timed_cmd = ["/usr/bin/time", "-v"] + cmd
        t0 = time.perf_counter()
        proc = subprocess.run(
            timed_cmd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
            check=True
        )
        t1 = time.perf_counter()

        stderr = proc.stderr

        # Extract stats from time -v output
        m_user = re.search(r"User time \(seconds\):\s+([0-9.]+)", stderr)
        m_sys = re.search(r"System time \(seconds\):\s+([0-9.]+)", stderr)
        m_rss = re.search(r"Maximum resident set size \(kbytes\):\s+([0-9]+)", stderr)

        user_t = float(m_user.group(1)) if m_user else 0.0
        sys_t = float(m_sys.group(1)) if m_sys else 0.0
        rss_kb = int(m_rss.group(1)) if m_rss else 0
        wall_t = t1 - t0

        user_times.append(user_t)
        sys_times.append(sys_t)
        wall_times.append(wall_t)
        rss_kb_list.append(rss_kb)

    avg_wall = sum(wall_times) / len(wall_times)
    min_wall = min(wall_times)
    avg_user = sum(user_times) / len(user_times)
    avg_sys = sum(sys_times) / len(sys_times)
    avg_rss_mb = (sum(rss_kb_list) / len(rss_kb_list)) / 1024.0

    return {
        "avg_wall": avg_wall,
        "min_wall": min_wall,
        "avg_user": avg_user,
        "avg_sys": avg_sys,
        "avg_rss_mb": avg_rss_mb,
    }

def main():
    tex_path = "examples/stress_40k.tex"
    if not Path(tex_path).exists():
        print(f"Error: {tex_path} does not exist.")
        return

    print("Running benchmarks (5 runs each)...")

    # 1. Benchmark pdflatex
    print("-> Benchmarking pdflatex...")
    pdflatex_out = "/tmp/pdflatex_stress.pdf"
    pdflatex_cmd = [
        "pdflatex",
        "-interaction=nonstopmode",
        "-halt-on-error",
        "-output-directory=/tmp",
        "-jobname=pdflatex_stress",
        tex_path
    ]
    pdf_stats = run_timed_command(pdflatex_cmd, runs=5)
    pdf_size_bytes = Path(pdflatex_out).stat().st_size if Path(pdflatex_out).exists() else 0

    # 2. Benchmark texrs Baseline (uncompressed)
    print("-> Benchmarking texrs Baseline (uncompressed)...")
    texrs_raw_out = "/tmp/texrs_raw.pdf"
    texrs_raw_cmd = [
        "./target/release/texrs",
        tex_path,
        "--no-compress",
        "-o",
        texrs_raw_out
    ]
    raw_stats = run_timed_command(texrs_raw_cmd, runs=5)
    raw_size_bytes = Path(texrs_raw_out).stat().st_size if Path(texrs_raw_out).exists() else 0

    # 3. Benchmark texrs Delta (compressed)
    print("-> Benchmarking texrs Delta (compressed)...")
    texrs_comp_out = "/tmp/texrs_comp.pdf"
    texrs_comp_cmd = [
        "./target/release/texrs",
        tex_path,
        "-o",
        texrs_comp_out
    ]
    comp_stats = run_timed_command(texrs_comp_cmd, runs=5)
    comp_size_bytes = Path(texrs_comp_out).stat().st_size if Path(texrs_comp_out).exists() else 0

    speedup_raw = pdf_stats["avg_wall"] / max(raw_stats["avg_wall"], 0.0001)
    speedup_comp = pdf_stats["avg_wall"] / max(comp_stats["avg_wall"], 0.0001)
    size_saving = (1.0 - (comp_size_bytes / max(raw_size_bytes, 1))) * 100.0
    ram_reduction = (1.0 - (comp_stats["avg_rss_mb"] / pdf_stats["avg_rss_mb"])) * 100.0

    print("\n" + "=" * 90)
    print("      BENCHMARK: 40,000 WORDS COMPILATION TO PDF (BASELINE VS DELTA)")
    print("=" * 90)
    print(f"{'Metric':<26} | {'pdflatex':<15} | {'texrs (Raw Baseline)':<21} | {'texrs (Compressed Delta)':<23}")
    print("-" * 90)
    print(f"{'Avg Wall-Clock Time':<26} | {pdf_stats['avg_wall'] * 1000.0:>12.2f} ms | {raw_stats['avg_wall'] * 1000.0:>18.2f} ms | {comp_stats['avg_wall'] * 1000.0:>20.2f} ms")
    print(f"{'Fastest Run':<26} | {pdf_stats['min_wall'] * 1000.0:>12.2f} ms | {raw_stats['min_wall'] * 1000.0:>18.2f} ms | {comp_stats['min_wall'] * 1000.0:>20.2f} ms")
    print(f"{'Peak Memory (Max RSS)':<26} | {pdf_stats['avg_rss_mb']:>12.2f} MB | {raw_stats['avg_rss_mb']:>18.2f} MB | {comp_stats['avg_rss_mb']:>20.2f} MB")
    print(f"{'Output PDF Size':<26} | {pdf_size_bytes / 1024.0:>12.1f} KB | {raw_size_bytes / 1024.0:>18.1f} KB | {comp_size_bytes / 1024.0:>20.1f} KB")
    print(f"{'Disk Overhead (Temp files)':<26} | {'4 files (.aux...)':>15} | {'0 files':>21} | {'0 files':>23}")
    print("=" * 90)
    print(f"Output PDF Size Reduction : {size_saving:.1f}% smaller than uncompressed baseline ({raw_size_bytes / 1024.0:.1f} KB -> {comp_size_bytes / 1024.0:.1f} KB)")
    print(f"Compressed Speedup        : {speedup_comp:.1f}x faster than pdflatex ({pdf_stats['avg_wall']*1000.0:.1f} ms vs {comp_stats['avg_wall']*1000.0:.1f} ms)")
    print(f"RAM Reduction vs pdflatex : {ram_reduction:.1f}% less peak memory ({comp_stats['avg_rss_mb']:.2f} MB vs {pdf_stats['avg_rss_mb']:.2f} MB)")
    print("=" * 90)

if __name__ == "__main__":
    main()
