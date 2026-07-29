#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
TARGET=${1:-}
VERSION=${2:-}
MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-14.0}
export MACOSX_DEPLOYMENT_TARGET

if [ -z "$TARGET" ] || [ -z "$VERSION" ]; then
  echo "usage: $0 <rust-target> <version>" >&2
  exit 2
fi

case "$TARGET" in
  aarch64-apple-darwin|x86_64-apple-darwin) ;;
  *)
    echo "unsupported target: $TARGET" >&2
    exit 2
    ;;
esac

case "$VERSION" in
  *[!0-9A-Za-z.+-]*|'')
    echo "invalid version: $VERSION" >&2
    exit 2
    ;;
esac

DIST="$ROOT/dist"
ARCHIVE="$DIST/kedu-$VERSION-$TARGET.tar.gz"

cd "$ROOT"
cargo build --locked --release --target "$TARGET" --bin kedu

mkdir -p "$DIST"
STAGE=$(mktemp -d "$DIST/.kedu-package.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT HUP INT TERM
cp "$ROOT/target/$TARGET/release/kedu" "$STAGE/kedu"

rm -f "$ARCHIVE" "$ARCHIVE.sha256"
COPYFILE_DISABLE=1 tar -C "$STAGE" -czf "$ARCHIVE" kedu

cd "$DIST"
shasum -a 256 "$(basename "$ARCHIVE")" > "$(basename "$ARCHIVE").sha256"

printf '%s\n' "$ARCHIVE" "$ARCHIVE.sha256"
