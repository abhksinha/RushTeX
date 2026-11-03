# Packaging

How rushtex reaches users, besides `cargo install`. Every channel installs the archives the
release workflow (`.github/workflows/release.yml`) attaches to a `v*` release:
`rushtex-<target>.tar.gz` (core) and `rushtex-<target>-full.tar.gz` (every font pack inside),
`.zip` on Windows, and `SHA256SUMS` over all of them.

| Channel | Command | Source |
|---|---|---|
| Homebrew (macOS, Linux) | `brew install abhksinha/rushtex/rushtex` | `homebrew/rushtex.rb` in the tap |
| Script (macOS, Linux) | `curl -fsSL https://raw.githubusercontent.com/abhksinha/RushTeX/master/scripts/install.sh \| sh` | `scripts/install.sh` |
| Script (Windows) | `irm https://raw.githubusercontent.com/abhksinha/RushTeX/master/scripts/install.ps1 \| iex` | `scripts/install.ps1` |
| Cargo | `cargo install --locked --git https://github.com/abhksinha/RushTeX rushtex` | the source |

The Homebrew formula installs the core build; the full build comes from the script
(`sh -s -- --full`), or font packs one at a time with `rushtex install fonts <pack>`.

## Homebrew tap

A tap is a GitHub repository named `homebrew-<name>`; `brew tap abhksinha/rushtex` clones
`github.com/abhksinha/homebrew-rushtex`. The formula is bottle-less: it downloads the
release's prebuilt archive for the machine, so the tap needs no CI.

Once, to create the tap:

```sh
gh repo create abhksinha/homebrew-rushtex --public --description "Homebrew tap for rushtex"
git clone https://github.com/abhksinha/homebrew-rushtex ../homebrew-rushtex
mkdir -p ../homebrew-rushtex/Formula
```

For every release, after the release workflow has finished (the formula reads its SHA256SUMS):

```sh
packaging/homebrew/update.sh v0.1.0 ../homebrew-rushtex/Formula/rushtex.rb
cd ../homebrew-rushtex
git add Formula/rushtex.rb && git commit -m "rushtex 0.1.0" && git push
brew update && brew install abhksinha/rushtex/rushtex && brew test rushtex
brew style abhksinha/rushtex/rushtex
```

(Homebrew loads formulae only from a tap, so the checks run after the push; a failing one is
fixed with another commit to the tap.)

`update.sh` fills the version and the four archive hashes (macOS and Linux, arm and Intel)
into `homebrew/rushtex.rb` and fails if `SHA256SUMS` lacks one of them. Users then run
`brew install abhksinha/rushtex/rushtex`, and `brew upgrade` picks up each new version.
