#!/bin/sh
# Installs rushtex from its GitHub release, checked against the release's SHA256SUMS.
#
#   curl -fsSL https://raw.githubusercontent.com/abhksinha/RushTeX/master/scripts/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/abhksinha/RushTeX/master/scripts/install.sh | sh -s -- --full --version v0.1.0
#
# Options (or the environment variable beside each):
#   --full            the full build, every font pack inside   (RUSHTEX_FULL=1)
#   --version vX.Y.Z  that release instead of the latest       (RUSHTEX_VERSION)
#   --dir DIR         where the binary goes, default ~/.local/bin (RUSHTEX_INSTALL_DIR)
# RUSHTEX_RELEASE_BASE replaces https://github.com/abhksinha/RushTeX/releases (a mirror, or a
# file:// folder laid out as latest/download/<asset> and download/<tag>/<asset>).
# No sudo: it writes only to the install folder and a temporary folder.
set -eu

repo_base="${RUSHTEX_RELEASE_BASE:-https://github.com/abhksinha/RushTeX/releases}"
version="${RUSHTEX_VERSION:-}"
full="${RUSHTEX_FULL:-}"
dir="${RUSHTEX_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf 'rushtex install: %s\n' "$*"; }
die() { printf 'rushtex install: error: %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
    case "$1" in
        --full) full=1 ;;
        --version) [ $# -ge 2 ] || die "--version needs a tag, such as v0.1.0"; version="$2"; shift ;;
        --version=*) version="${1#--version=}" ;;
        --dir) [ $# -ge 2 ] || die "--dir needs a folder"; dir="$2"; shift ;;
        --dir=*) dir="${1#--dir=}" ;;
        -h|--help) printf 'usage: install.sh [--full] [--version vX.Y.Z] [--dir DIR]\n'; exit 0 ;;
        *) die "unknown option $1 (options: --full, --version vX.Y.Z, --dir DIR)" ;;
    esac
    shift
done
case "$full" in 0|'') full='' ;; *) full=1 ;; esac

# The release target for this machine.
os=$(uname -s)
arch=$(uname -m)
case "$arch" in
    arm64|aarch64) arch=aarch64 ;;
    x86_64|amd64) arch=x86_64 ;;
    *) die "no rushtex build for the processor $arch; build it from source: cargo install --git https://github.com/abhksinha/RushTeX rushtex" ;;
esac
case "$os" in
    Darwin)
        # An x86_64 shell under Rosetta on Apple silicon: take the native build.
        if [ "$arch" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = 1 ]; then arch=aarch64; fi
        target="$arch-apple-darwin" ;;
    Linux) target="$arch-unknown-linux-gnu" ;;
    MINGW*|MSYS*|CYGWIN*) die "on Windows, run in PowerShell: irm https://raw.githubusercontent.com/abhksinha/RushTeX/master/scripts/install.ps1 | iex" ;;
    *) die "no rushtex build for $os; build it from source: cargo install --git https://github.com/abhksinha/RushTeX rushtex" ;;
esac

name="rushtex-$target${full:+-full}"
asset="$name.tar.gz"
if [ -n "$version" ]; then
    url_dir="$repo_base/download/$version"
else
    url_dir="$repo_base/latest/download"
fi

command -v curl > /dev/null 2>&1 || die "curl is needed"
command -v tar > /dev/null 2>&1 || die "tar is needed"
if command -v sha256sum > /dev/null 2>&1; then
    hash_of() { sha256sum "$1" | cut -d ' ' -f 1; }
elif command -v shasum > /dev/null 2>&1; then
    hash_of() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
else
    die "sha256sum or shasum is needed to check the download"
fi

tmp=$(mktemp -d 2>/dev/null || mktemp -d -t rushtex)
trap 'rm -rf "$tmp"' EXIT
trap 'exit 1' HUP INT TERM

say "downloading $asset (${version:-latest})"
curl -fsSL --retry 3 -o "$tmp/$asset" "$url_dir/$asset" || die "could not download $url_dir/$asset"
curl -fsSL --retry 3 -o "$tmp/SHA256SUMS" "$url_dir/SHA256SUMS" || die "could not download $url_dir/SHA256SUMS"

# The line for this asset, by its exact name (with or without sha256sum's binary-mode `*`).
want=$(awk -v f="$asset" '$2 == f || $2 == "*" f { print $1; exit }' "$tmp/SHA256SUMS")
[ -n "$want" ] || die "SHA256SUMS names no $asset"
got=$(hash_of "$tmp/$asset")
[ "$got" = "$want" ] || die "checksum mismatch for $asset (expected $want, got $got); nothing installed"
say "checksum verified"

tar -xzf "$tmp/$asset" -C "$tmp"
[ -f "$tmp/$name/rushtex" ] || die "$asset holds no $name/rushtex"
mkdir -p "$dir" || die "cannot create $dir (set RUSHTEX_INSTALL_DIR or --dir to a folder you can write)"
# Copy beside the old binary, then rename over it: a running rushtex keeps working.
cp "$tmp/$name/rushtex" "$dir/.rushtex.new"
chmod 755 "$dir/.rushtex.new"
mv -f "$dir/.rushtex.new" "$dir/rushtex"
say "installed $dir/rushtex"

case ":$PATH:" in
    *":$dir:"*) say "run: rushtex paper.tex" ;;
    *)
        say "$dir is not on your PATH. Add it, then open a new terminal:"
        line="export PATH=\"$dir:\$PATH\""
        case "${SHELL:-}" in
            */zsh) printf "    echo '%s' >> ~/.zshrc\n" "$line" ;;
            */fish) printf '    fish_add_path %s\n' "$dir" ;;
            *) printf "    echo '%s' >> ~/.profile\n" "$line" ;;
        esac ;;
esac
