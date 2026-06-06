#!/usr/bin/env bash
# Refresh the vendored OpenAPI specs from Stedi's public repository.
#
# Usage: ./scripts/sync-specs.sh [ref]
#   ref  git ref/branch/tag to pull from (default: main)
#
# After running, rebuild and run the tests:
#   cargo test
set -euo pipefail

REPO="${STEDI_SPECS_REPO:-https://raw.githubusercontent.com/Stedi/openapi}"
REF="${1:-main}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$HERE/specs"

specs=(claims core enrollment event-destinations healthcare manager payers)

echo "Syncing specs from $REPO@$REF -> $DEST"
for name in "${specs[@]}"; do
  url="$REPO/$REF/$name.json"
  echo "  - $name.json"
  curl -fsSL "$url" -o "$DEST/$name.json"
done

echo "Done. Rebuild with: cargo test"
