#!/bin/sh
# Fills the Homebrew formula template (rushtex.rb beside this script) for a release:
#
#   packaging/homebrew/update.sh v0.1.0 > ../homebrew-rushtex/Formula/rushtex.rb
#   packaging/homebrew/update.sh v0.1.0 ../homebrew-rushtex/Formula/rushtex.rb
#
# It reads the release's SHA256SUMS (RUSHTEX_RELEASE_BASE replaces
# https://github.com/abhksinha/RushTeX/releases, as in scripts/install.sh) and fails if any
# of the four archives the formula names is missing from it.
set -eu

[ $# -ge 1 ] || { echo "usage: $0 vX.Y.Z [OUT]" >&2; exit 2; }
tag="$1"
case "$tag" in v[0-9]*) ;; *) echo "update.sh: the tag looks like v0.1.0, not $tag" >&2; exit 2 ;; esac
out="${2:-}"
here=$(cd "$(dirname "$0")" && pwd)
base="${RUSHTEX_RELEASE_BASE:-https://github.com/abhksinha/RushTeX/releases}"

sums=$(curl -fsSL "$base/download/$tag/SHA256SUMS") || { echo "update.sh: no SHA256SUMS at $base/download/$tag" >&2; exit 1; }

script="s/@VERSION@/${tag#v}/"
for t in aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu; do
    h=$(printf '%s\n' "$sums" | awk -v f="rushtex-$t.tar.gz" '$2 == f || $2 == "*" f { print $1; exit }')
    case "$h" in
        ????????????????????????????????????????????????????????????????) ;;
        *) echo "update.sh: SHA256SUMS of $tag has no rushtex-$t.tar.gz" >&2; exit 1 ;;
    esac
    script="$script;s/@SHA256_$t@/$h/"
done

if [ -n "$out" ]; then
    sed "$script" "$here/rushtex.rb" > "$out.tmp" && mv "$out.tmp" "$out"
    echo "update.sh: wrote $out for $tag" >&2
else
    sed "$script" "$here/rushtex.rb"
fi
