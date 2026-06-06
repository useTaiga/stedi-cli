class Stedi < Formula
  desc "Agent-friendly CLI for the Stedi APIs, driven by the official OpenAPI specs"
  homepage "https://github.com/useTaiga/stedi-cli"
  version "0.1.1"
  license "MIT"

  # This formula is rewritten automatically by the release workflow with the
  # real version, release URLs, and sha256 checksums on each tagged release.
  on_macos do
    on_arm do
      url "https://github.com/useTaiga/stedi-cli/releases/download/v0.1.1/stedi-aarch64-apple-darwin.tar.gz"
      sha256 "b6c0aa5d714e3ce560589eb0e3d545301a03c611d6aa9436389cd25d76ab847b"
    end
    on_intel do
      url "https://github.com/useTaiga/stedi-cli/releases/download/v0.1.1/stedi-x86_64-apple-darwin.tar.gz"
      sha256 "25463997fe9b4e7fbe4cef2bad5e5f2a85e3afae83af891eebacd32ad5fff3d3"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/useTaiga/stedi-cli/releases/download/v0.1.1/stedi-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "d5662ff77a0ff6ef1f1773aade747c717ab76ab5d74a1aeeec7b122b98a2e66c"
    end
    on_intel do
      url "https://github.com/useTaiga/stedi-cli/releases/download/v0.1.1/stedi-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "d1b88e8dec91ca64c5f23bc75df5ba85e9b454ad07b9d4f0192f3e634b063930"
    end
  end

  def install
    bin.install "stedi"
  end

  test do
    assert_match "stedi", shell_output("#{bin}/stedi --version")
  end
end
