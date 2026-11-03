# Homebrew formula for rushtex, bottle-less: it installs the release's prebuilt binary (the
# core build). A template: packaging/homebrew/update.sh vX.Y.Z fills VERSION and the four
# sha256 values from that release's SHA256SUMS and writes the formula for the tap
# (packaging/README.md).
class Rushtex < Formula
  desc "LaTeX to PDF compiler, output identical to pdfLaTeX, in pure safe Rust"
  homepage "https://abhksinha.github.io/RushTeX/"
  version "@VERSION@"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    depends_on macos: :ventura

    on_arm do
      url "https://github.com/abhksinha/RushTeX/releases/download/v#{version}/rushtex-aarch64-apple-darwin.tar.gz"
      sha256 "@SHA256_aarch64-apple-darwin@"
    end
    on_intel do
      url "https://github.com/abhksinha/RushTeX/releases/download/v#{version}/rushtex-x86_64-apple-darwin.tar.gz"
      sha256 "@SHA256_x86_64-apple-darwin@"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/abhksinha/RushTeX/releases/download/v#{version}/rushtex-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "@SHA256_aarch64-unknown-linux-gnu@"
    end
    on_intel do
      url "https://github.com/abhksinha/RushTeX/releases/download/v#{version}/rushtex-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "@SHA256_x86_64-unknown-linux-gnu@"
    end
  end

  def install
    bin.install "rushtex"
  end

  test do
    (testpath/"t.tex").write <<~TEX
      \\documentclass{article}
      \\begin{document}
      Hello, $x^2$.
      \\end{document}
    TEX
    system bin/"rushtex", "t.tex"
    assert_path_exists testpath/"t.pdf"
  end
end
