#!/usr/bin/env bash
# Download the face detector weights and verify their full SHA-256. Works on macOS and Linux.
# MODELS_DIR overrides the destination (default: worker/models).
set -euo pipefail
cd "$(dirname "$0")/.."

NAME=yolov11n-face.pt
URL="https://github.com/akanametov/yolo-face/releases/download/1.0.0/$NAME"
EXPECTED=9420a9d4933940d2202f511d45df1750e3c1546dd9efc7a3985caae3bbbf7ed2
MODELS_DIR="${MODELS_DIR:-models}"

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    echo "sha256sum or shasum is required" >&2
    return 1
  fi
}

mkdir -p "$MODELS_DIR"
target="$MODELS_DIR/$NAME"

if [[ -e "$target" ]]; then
  actual="$(sha256_of "$target")"
  if [[ "$actual" != "$EXPECTED" ]]; then
    echo "existing $target has the wrong SHA-256: $actual" >&2
    exit 1
  fi
  echo "$target already verified"
  exit 0
fi

partial="$(mktemp "$MODELS_DIR/.$NAME.XXXXXX")"
trap 'rm -f "$partial"' EXIT
curl -sSfL --max-time 600 -o "$partial" "$URL"
actual="$(sha256_of "$partial")"
if [[ "$actual" != "$EXPECTED" ]]; then
  echo "downloaded $NAME has the wrong SHA-256: $actual" >&2
  exit 1
fi
chmod 644 "$partial"
mv "$partial" "$target"
echo "$target verified"
