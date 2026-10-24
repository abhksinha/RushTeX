#!/bin/zsh
# The native document job against the oracle, page box by page box (P2,
# memorycastle/journal/2026-10-23-p2-native-document-path.md).
#
#   scripts/native_probes.sh [FILE.tex ..] [-n LINES] [-k DIR]
#
# Each file (default: scripts/native_probes/*.tex and docs/master.tex) runs twice: on the engine with the
# LaTeX format (`RT_ENGINE=1`, the oracle, its last run) and on the native job. Every
# page's `\showbox` dump (`RT_PAGEDUMP`) is compared, whatsits included: the native job keeps
# LaTeX's `.aux` writes as inert whatsits, since they decide where glue may break. A file the native job does not accept is reported, not compared.
# `-k DIR` keeps the dumps in DIR/o (oracle) and DIR/n (native) of the last file.
# Needs target/release/rushtex and the format in target/parity (parity-testing makes it).
root=${0:A:h:h}
n=12
files=()
while (( $# )); do
  case $1 in
    -n) n=$2; shift 2 ;;
    -k) keep=${2:A}; shift 2 ;;
    *) files+=(${1:A}); shift ;;
  esac
done
(( ${#files} )) || files=($root/scripts/native_probes/*.tex $root/docs/master.tex)
if [[ -n $keep ]]; then tmp=$keep; mkdir -p $tmp; else tmp=$(mktemp -d); trap 'rm -rf $tmp' EXIT; fi
cd $root
fail=0
for f in $files; do
  rm -rf $tmp/o $tmp/n; mkdir -p $tmp/o $tmp/n
  export SOURCE_DATE_EPOCH=1792454400 FORCE_SOURCE_DATE=1
  RT_ENGINE=1 RT_PAGEHASH=$tmp/o/hash RT_PAGEDUMP=$tmp/o RUSHTEX_CACHE=target/parity ./target/release/rushtex --fmt target/parity/pdflatex.rfmt $f -o $tmp/o.pdf >/dev/null 2>&1
  out=$(RT_PAGEHASH=$tmp/n/hash RT_PAGEDUMP=$tmp/n ./target/release/rushtex $f -o $tmp/n.pdf 2>&1)
  if [[ $out != *natively* ]]; then echo "${f:t}: not accepted by the native job"; fail=1; continue; fi
  [[ $out == *'! '* ]] && print -r -- "${f:t}: ${out%%$'\n'*}"
  last=$(ls $tmp/o/*.box | sed 's/.*\/\([0-9]*\)\.[0-9]*\.box/\1/' | sort -n | tail -1)
  pages=0; bad=0
  for ob in $tmp/o/$last.*.box; do
    p=${ob:t:r:e}; nb=$tmp/n/1.$p.box; pages=$((pages + 1))
    if [[ ! -f $nb ]]; then echo "${f:t} page $p: missing in the native run"; bad=1; continue; fi
    d=$(diff $ob $nb)
    if [[ -n $d ]]; then echo "${f:t} page $p:"; print -r -- "$d" | head -$n; bad=1; fi
  done
  extra=$(( $(ls $tmp/n/*.box | wc -l) - pages ))
  (( extra > 0 )) && { echo "${f:t}: $extra extra native pages"; bad=1; }
  (( bad )) && fail=1 || echo "${f:t}: $pages pages identical"
done
exit $fail
