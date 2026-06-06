class Stedi < Formula
  desc "Agent-friendly CLI for the Stedi APIs, driven by the official OpenAPI specs"
  homepage "https://github.com/useTaiga/stedi-cli"
  version "0.1.0"
  license "MIT"

  # This formula is rewritten automatically by the release workflow with the
  # real version, release URLs, and sha256 checksums on each tagged release.
  on_macos do
    on_arm do
      url "https://github.com/useTaiga/stedi-cli/releases/download/v0.1.0/stedi-aarch64-apple-darwin.tar.gz"
      sha256 "145d528db0bb4361aa63a0023c847ba1fc446a5cfd23e1aeda6cc1d4fce865f4"
    end
    on_intel do
      url "https://github.com/useTaiga/stedi-cli/releases/download/v0.1.0/stedi-x86_64-apple-darwin.tar.gz"
      sha256 "46f6bc965274201fba7cf380d9fc3948ab66e4b9ebea85525b3b7d6976cb4e1d"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/useTaiga/stedi-cli/releases/download/v0.1.0/stedi-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "bc1d3b62b039c8c0bdabe3f5d7e9b26fb9028ee484abb96e2ae7b6f94324b1a8"
    end
    on_intel do
      url "https://github.com/useTaiga/stedi-cli/releases/download/v0.1.0/stedi-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "6de80f72c10fcb346bd5f616e58ad0315029749274d6191178ce0298c43bd920"
    end
  end

  def install
    bin.install "stedi"
  end

  test do
    assert_match "stedi", shell_output("#{bin}/stedi --version")
  end
end
