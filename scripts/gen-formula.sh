#!/usr/bin/env bash
# Generate Formula/stedi.rb from the checksums attached to a GitHub release.
#
# Usage: ./scripts/gen-formula.sh <tag>
#   e.g. ./scripts/gen-formula.sh v0.1.0
#
# Requires the `gh` CLI authenticated with read access to the repo. Used by the
# release workflow and runnable locally to regenerate the formula by hand.
set -euo pipefail

TAG="${1:?usage: gen-formula.sh <tag>}"
VERSION="${TAG#v}"
REPO="${GITHUB_REPOSITORY:-useTaiga/stedi-cli}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$HERE/Formula/stedi.rb"
BASE="https://github.com/${REPO}/releases/download/${TAG}"

# Read the sha256 of a target's archive from its release checksum asset.
sha() {
  gh release download "$TAG" --repo "$REPO" --pattern "stedi-$1.sha256" --output - \
    | awk '{print $1}'
}

MAC_ARM="$(sha aarch64-apple-darwin)"
MAC_X86="$(sha x86_64-apple-darwin)"
LIN_ARM="$(sha aarch64-unknown-linux-gnu)"
LIN_X86="$(sha x86_64-unknown-linux-gnu)"

for v in MAC_ARM MAC_X86 LIN_ARM LIN_X86; do
  [ -n "${!v}" ] || { echo "error: empty checksum for $v" >&2; exit 1; }
done

cat > "$OUT" <<EOF
class Stedi < Formula
  desc "Agent-friendly CLI for the Stedi APIs, driven by the official OpenAPI specs"
  homepage "https://github.com/${REPO}"
  version "${VERSION}"
  license "MIT"

  # This formula is rewritten automatically by the release workflow with the
  # real version, release URLs, and sha256 checksums on each tagged release.
  on_macos do
    on_arm do
      url "${BASE}/stedi-aarch64-apple-darwin.tar.gz"
      sha256 "${MAC_ARM}"
    end
    on_intel do
      url "${BASE}/stedi-x86_64-apple-darwin.tar.gz"
      sha256 "${MAC_X86}"
    end
  end

  on_linux do
    on_arm do
      url "${BASE}/stedi-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "${LIN_ARM}"
    end
    on_intel do
      url "${BASE}/stedi-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "${LIN_X86}"
    end
  end

  def install
    bin.install "stedi"
  end

  test do
    assert_match "stedi", shell_output("#{bin}/stedi --version")
  end
end
EOF

echo "Wrote $OUT for $TAG"
