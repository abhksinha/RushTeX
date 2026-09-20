#!/usr/bin/env bash
# ==============================================================================
# RushTeX vs pdfLaTeX Comparative Benchmark Suite
# Measures: Wall-clock Latency (ms), Peak RAM (RSS), Pages/sec, Words/sec
# ==============================================================================

set -euo pipefail

# Print banner
echo "======================================================================"
echo "         RushTeX vs pdfLaTeX Comparative Benchmark Suite              "
echo "======================================================================"

# Target TeX file
TEX_FILE="${1:-docs/manual.tex}"
RUNS="${2:-3}"

if [ ! -f "$TEX_FILE" ]; then
    echo "Error: TeX file '$TEX_FILE' not found!"
    echo "Usage: $0 <path_to_tex_file> [iterations (default: 3)]"
    exit 1
fi

# Ensure runs is a positive integer
if ! [[ "$RUNS" =~ ^[0-9]+$ ]] || [ "$RUNS" -lt 1 ]; then
    RUNS=3
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSH_BIN="$SCRIPT_DIR/target/release/rushtex"

# Check / build rushtex binary
if [ ! -x "$RUSH_BIN" ]; then
    echo "[*] Building rushtex in release mode..."
    cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"
fi

# Check pdflatex
if ! command -v pdflatex &>/dev/null; then
    echo "Error: 'pdflatex' command not found on PATH. Please install TeX Live."
    exit 1
fi

# Setup isolated temp directory
TMP_DIR=$(mktemp -d /tmp/rushtex_bench_XXXXXX)
trap 'rm -rf "$TMP_DIR"' EXIT

# Calculate word count of input source
WORD_COUNT=$(wc -w < "$TEX_FILE" | tr -d ' ')
FILE_NAME=$(basename "$TEX_FILE")

echo "[*] Benchmarking file: $TEX_FILE ($WORD_COUNT words, $RUNS iterations)"
echo ""

# ------------------------------------------------------------------------------
# 1. Benchmark RushTeX
# ------------------------------------------------------------------------------
echo -n "[*] Running RushTeX ($RUNS iterations)... "
TOTAL_RUSH_TIME_NS=0
TOTAL_RUSH_RSS_KB=0
RUSH_PAGES=0
RUSH_PDF="$TMP_DIR/rush_out.pdf"

for ((i=1; i<=RUNS; i++)); do
    TIME_FILE="$TMP_DIR/rush_time_$i.log"
    START_NS=$(date +%s%N)
    /usr/bin/time -v "$RUSH_BIN" -o "$RUSH_PDF" "$TEX_FILE" >/dev/null 2>"$TIME_FILE"
    END_NS=$(date +%s%N)

    ELAPSED_NS=$((END_NS - START_NS))
    TOTAL_RUSH_TIME_NS=$((TOTAL_RUSH_TIME_NS + ELAPSED_NS))

    RSS_KB=$(grep "Maximum resident set size" "$TIME_FILE" | awk '{print $NF}')
    TOTAL_RUSH_RSS_KB=$((TOTAL_RUSH_RSS_KB + RSS_KB))
done
echo "done."

if command -v pdfinfo &>/dev/null; then
    RUSH_PAGES=$(pdfinfo "$RUSH_PDF" 2>/dev/null | grep -i "Pages:" | awk '{print $2}')
fi
if [ -z "$RUSH_PAGES" ] || [ "$RUSH_PAGES" -eq 0 ]; then
    RUSH_PAGES=1
fi
RUSH_PDF_SIZE_BYTES=$(stat -c%s "$RUSH_PDF" 2>/dev/null || wc -c < "$RUSH_PDF")
RUSH_PDF_SIZE_KB=$(awk "BEGIN {printf \"%.1f\", $RUSH_PDF_SIZE_BYTES / 1024}")

AVG_RUSH_TIME_MS=$(awk "BEGIN {printf \"%.2f\", ($TOTAL_RUSH_TIME_NS / $RUNS) / 1000000}")
AVG_RUSH_RSS_MB=$(awk "BEGIN {printf \"%.2f\", ($TOTAL_RUSH_RSS_KB / $RUNS) / 1024}")
RUSH_PAGES_PER_SEC=$(awk "BEGIN {if ($AVG_RUSH_TIME_MS > 0) printf \"%.1f\", ($RUSH_PAGES * 1000) / $AVG_RUSH_TIME_MS; else printf \"N/A\"}")
RUSH_WORDS_PER_SEC=$(awk "BEGIN {if ($AVG_RUSH_TIME_MS > 0) printf \"%.0f\", ($WORD_COUNT * 1000) / $AVG_RUSH_TIME_MS; else printf \"N/A\"}")

# ------------------------------------------------------------------------------
# 2. Benchmark pdfLaTeX
# ------------------------------------------------------------------------------
echo -n "[*] Running pdfLaTeX ($RUNS iterations)... "
TOTAL_PDF_TIME_NS=0
TOTAL_PDF_RSS_KB=0
PDF_PAGES=0
PDF_OUT_PDF="$TMP_DIR/${FILE_NAME%.tex}.pdf"

for ((i=1; i<=RUNS; i++)); do
    TIME_FILE="$TMP_DIR/pdf_time_$i.log"
    START_NS=$(date +%s%N)
    /usr/bin/time -v pdflatex -interaction=batchmode -output-directory="$TMP_DIR" "$TEX_FILE" >/dev/null 2>"$TIME_FILE" || true
    END_NS=$(date +%s%N)

    ELAPSED_NS=$((END_NS - START_NS))
    TOTAL_PDF_TIME_NS=$((TOTAL_PDF_TIME_NS + ELAPSED_NS))

    RSS_KB=$(grep "Maximum resident set size" "$TIME_FILE" | awk '{print $NF}')
    TOTAL_PDF_RSS_KB=$((TOTAL_PDF_RSS_KB + RSS_KB))
done
echo "done."

if [ -f "$PDF_OUT_PDF" ]; then
    if command -v pdfinfo &>/dev/null; then
        PDF_PAGES=$(pdfinfo "$PDF_OUT_PDF" 2>/dev/null | grep -i "Pages:" | awk '{print $2}')
    fi
    PDF_SIZE_BYTES=$(stat -c%s "$PDF_OUT_PDF" 2>/dev/null || wc -c < "$PDF_OUT_PDF")
    PDF_SIZE_KB=$(awk "BEGIN {printf \"%.1f\", $PDF_SIZE_BYTES / 1024}")
else
    PDF_PAGES=0
    PDF_SIZE_KB="0.0"
fi
if [ -z "$PDF_PAGES" ] || [ "$PDF_PAGES" -eq 0 ]; then
    PDF_PAGES=$RUSH_PAGES
fi

AVG_PDF_TIME_MS=$(awk "BEGIN {printf \"%.2f\", ($TOTAL_PDF_TIME_NS / $RUNS) / 1000000}")
AVG_PDF_RSS_MB=$(awk "BEGIN {printf \"%.2f\", ($TOTAL_PDF_RSS_KB / $RUNS) / 1024}")
PDF_PAGES_PER_SEC=$(awk "BEGIN {if ($AVG_PDF_TIME_MS > 0) printf \"%.1f\", ($PDF_PAGES * 1000) / $AVG_PDF_TIME_MS; else printf \"N/A\"}")
PDF_WORDS_PER_SEC=$(awk "BEGIN {if ($AVG_PDF_TIME_MS > 0) printf \"%.0f\", ($WORD_COUNT * 1000) / $AVG_PDF_TIME_MS; else printf \"N/A\"}")

# ------------------------------------------------------------------------------
# 3. Compute Multipliers
# ------------------------------------------------------------------------------
SPEEDUP=$(awk "BEGIN {if ($AVG_RUSH_TIME_MS > 0) printf \"%.1f\", $AVG_PDF_TIME_MS / $AVG_RUSH_TIME_MS; else printf \"N/A\"}")
RAM_SAVINGS=$(awk "BEGIN {if ($AVG_RUSH_RSS_MB > 0) printf \"%.1f\", $AVG_PDF_RSS_MB / $AVG_RUSH_RSS_MB; else printf \"N/A\"}")
SIZE_RATIO=$(awk "BEGIN {if ($RUSH_PDF_SIZE_KB > 0) printf \"%.1f\", $PDF_SIZE_KB / $RUSH_PDF_SIZE_KB; else printf \"N/A\"}")

# ------------------------------------------------------------------------------
# 4. Render Table
# ------------------------------------------------------------------------------
echo ""
echo "+----------------------+-----------------+-----------------+----------------+"
echo "| Metric               | RushTeX         | pdfLaTeX        | Factor         |"
echo "+----------------------+-----------------+-----------------+----------------+"
printf "| %-20s | %15s | %15s | %14s |\n" "Compilation Time" "${AVG_RUSH_TIME_MS} ms" "${AVG_PDF_TIME_MS} ms" "${SPEEDUP}x faster"
printf "| %-20s | %15s | %15s | %14s |\n" "Peak RAM (RSS)" "${AVG_RUSH_RSS_MB} MB" "${AVG_PDF_RSS_MB} MB" "${RAM_SAVINGS}x leaner"
printf "| %-20s | %15s | %15s | %14s |\n" "Pages Rendered" "${RUSH_PAGES} pages" "${PDF_PAGES} pages" "---"
printf "| %-20s | %15s | %15s | %14s |\n" "Page Throughput" "${RUSH_PAGES_PER_SEC} p/s" "${PDF_PAGES_PER_SEC} p/s" "${SPEEDUP}x"
printf "| %-20s | %15s | %15s | %14s |\n" "Word Throughput" "${RUSH_WORDS_PER_SEC} w/s" "${PDF_WORDS_PER_SEC} w/s" "${SPEEDUP}x"
printf "| %-20s | %15s | %15s | %14s |\n" "Output PDF Size" "${RUSH_PDF_SIZE_KB} KB" "${PDF_SIZE_KB} KB" "${SIZE_RATIO}x smaller"
echo "+----------------------+-----------------+-----------------+----------------+"
echo ""
echo "Executive Summary:"
echo "  • Speed:       RushTeX compiles ${SPEEDUP}x faster than pdfLaTeX."
echo "  • Memory:      RushTeX uses ${RAM_SAVINGS}x less peak RAM."
echo "  • Footprint:   RushTeX PDF output is ${SIZE_RATIO}x more compact."
echo "======================================================================"
