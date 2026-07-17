#!/bin/zsh
set -euo pipefail

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
APP="$ROOT/dist/刻度.app"
CONFIG="${1:-release}"

cd "$ROOT"

GIT_COUNT="$(git rev-list --count HEAD 2>/dev/null || echo 0)"
GIT_HASH="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
git diff --quiet HEAD 2>/dev/null || GIT_HASH="${GIT_HASH}+"
VERSION="${KEDU_VERSION:-0.1.$GIT_COUNT}"
BUILD="${KEDU_BUILD:-dev.$GIT_HASH}"

swift build -c "$CONFIG"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/.build/$CONFIG/KeduMonitor" "$APP/Contents/MacOS/KeduMonitor"
cp "$ROOT/Info.plist" "$APP/Contents/Info.plist"
cp "$ROOT/Assets/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
plutil -replace CFBundleShortVersionString -string "$VERSION" "$APP/Contents/Info.plist"
plutil -replace CFBundleVersion -string "$BUILD" "$APP/Contents/Info.plist"

SIGN_ID="${KEDU_SIGN_ID:-}"
codesign --force --deep --sign "${SIGN_ID:--}" "$APP"

printf '%s\n' "$APP ($VERSION, $BUILD; ${SIGN_ID:-ad-hoc})"
