class Stedi < Formula
  desc "Agent-friendly CLI for the Stedi APIs, driven by the official OpenAPI specs"
  homepage "https://github.com/useTaiga/stedi-cli"
  version "0.0.0"
  license "MIT"

  # This formula is rewritten automatically by the release workflow with the
  # real version, release URLs, and sha256 checksums on each tagged release.
  on_macos do
    on_arm do
      url "https://github.com/useTaiga/stedi-cli/releases/download/v0.0.0/stedi-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_intel do
      url "https://github.com/useTaiga/stedi-cli/releases/download/v0.0.0/stedi-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/useTaiga/stedi-cli/releases/download/v0.0.0/stedi-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_intel do
      url "https://github.com/useTaiga/stedi-cli/releases/download/v0.0.0/stedi-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    bin.install "stedi"
  end

  test do
    assert_match "stedi", shell_output("#{bin}/stedi --version")
  end
end
