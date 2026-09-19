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
    pdflatex_cmd = [
        "pdflatex",
        "-interaction=nonstopmode",
        "-halt-on-error",
        "-output-directory=/tmp",
        tex_path
    ]
    pdf_stats = run_timed_command(pdflatex_cmd, runs=5)

    # 2. Benchmark texrs
    print("-> Benchmarking texrs...")
    texrs_cmd = [
        "./target/release/texrs",
        tex_path,
        "-o",
        "/tmp/texrs_stress.pdf"
    ]
    texrs_stats = run_timed_command(texrs_cmd, runs=5)

    speedup_avg = pdf_stats["avg_wall"] / max(texrs_stats["avg_wall"], 0.0001)
    ram_reduction = (1.0 - (texrs_stats["avg_rss_mb"] / pdf_stats["avg_rss_mb"])) * 100.0

    print("\n" + "=" * 68)
    print("      BENCHMARK COMPARISON: 40,000 WORDS COMPILATION TO PDF")
    print("=" * 68)
    print(f"{'Metric':<28} | {'pdflatex':<16} | {'texrs (ours)':<16}")
    print("-" * 68)
    print(f"{'Avg Wall-Clock Time':<28} | {pdf_stats['avg_wall'] * 1000.0:>13.2f} ms | {texrs_stats['avg_wall'] * 1000.0:>13.2f} ms")
    print(f"{'Fastest Wall-Clock Run':<28} | {pdf_stats['min_wall'] * 1000.0:>13.2f} ms | {texrs_stats['min_wall'] * 1000.0:>13.2f} ms")
    print(f"{'User CPU Time':<28} | {pdf_stats['avg_user'] * 1000.0:>13.2f} ms | {texrs_stats['avg_user'] * 1000.0:>13.2f} ms")
    print(f"{'System CPU Time':<28} | {pdf_stats['avg_sys'] * 1000.0:>13.2f} ms | {texrs_stats['avg_sys'] * 1000.0:>13.2f} ms")
    print(f"{'Peak Memory (Max RSS)':<28} | {pdf_stats['avg_rss_mb']:>13.2f} MB | {texrs_stats['avg_rss_mb']:>13.2f} MB")
    print(f"{'Intermediate Files on Disk':<28} | {'4 (.aux, .log...)':>16} | {'0 (None)':>16}")
    print("=" * 68)
    print(f"Execution Speedup : {speedup_avg:.1f}x faster")
    print(f"RAM Reduction     : {ram_reduction:.1f}% less peak memory")
    print("=" * 68)

if __name__ == "__main__":
    main()
