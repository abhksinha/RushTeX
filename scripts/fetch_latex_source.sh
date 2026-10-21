#!/bin/sh
# Downloads the commented LaTeX sources (.dtx/.ins/.fdd) of the kernel and the required
# packages into latex-source/, a read-only reference like texlive-source/ (both gitignored).
# Usage: scripts/fetch_latex_source.sh   (needs curl and unzip; rerun to refresh)
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
dest="$root/latex-source"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mirror=https://mirrors.ctan.org
for pkg in \
    macros/latex/base \
    macros/latex/required/tools \
    macros/latex/required/amsmath \
    macros/latex/required/amscls \
    macros/latex/required/graphics \
    macros/latex/contrib/hyperref; do
    name=$(basename "$pkg")
    curl -sfL -o "$tmp/$name.zip" "$mirror/$pkg.zip"
    mkdir -p "$dest/$name"
    unzip -qo "$tmp/$name.zip" '*.dtx' '*.ins' '*.fdd' '*.tex' -d "$tmp/x" || true
    find "$tmp/x" -type f \( -name '*.dtx' -o -name '*.ins' -o -name '*.fdd' \) -exec cp {} "$dest/$name/" \;
    rm -rf "$tmp/x"
    echo "$name: $(ls "$dest/$name" | wc -l | tr -d ' ') files"
done
