#!/bin/zsh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist"
ARCHIVE="$DIST/KeduMonitor-macOS.zip"

cd "$ROOT"
"$ROOT/scripts/build-app.sh" release
rm -f "$ARCHIVE" "$ARCHIVE.sha256"
ditto -c -k --sequesterRsrc --keepParent "$DIST/刻度.app" "$ARCHIVE"

cd "$DIST"
shasum -a 256 "$(basename "$ARCHIVE")" > "$(basename "$ARCHIVE").sha256"
printf '%s\n' "$ARCHIVE" "$ARCHIVE.sha256"
