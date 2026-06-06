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
      sha256 "efed564e6b1be3d0a83a62603d9e48ecbf148ef0e01b287cc088065f0febc3da"
    end
    on_intel do
      url "https://github.com/useTaiga/stedi-cli/releases/download/v0.1.1/stedi-x86_64-apple-darwin.tar.gz"
      sha256 "280c13b01782027a8eeeb09af2a81ce9131e44c2e0106a7d1a01840bacd5b531"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/useTaiga/stedi-cli/releases/download/v0.1.1/stedi-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "5cbc155d892efef7f9d4cdd39d72f446f482c160edb381f3bc9d6f3114bf3cd5"
    end
    on_intel do
      url "https://github.com/useTaiga/stedi-cli/releases/download/v0.1.1/stedi-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "6703c76937ee50ff82d25104a8d331ff6ac5608fe6ef73d4f65c04c1d52b17c6"
    end
  end

  def install
    bin.install "stedi"
  end

  test do
    assert_match "stedi", shell_output("#{bin}/stedi --version")
  end
end
