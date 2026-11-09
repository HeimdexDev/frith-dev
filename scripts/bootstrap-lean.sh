#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) platform=darwin_aarch64; checksum=17ee554702c199fc03f37b1c3708245671c93f5d932dcff55a03fa6cdb5e5adf ;;
  Darwin-x86_64) platform=darwin; checksum=a5ef0fb0e14645eaa0e60bc6ba39a07e7deeeec73dc93ce7195b837eb9de2c9f ;;
  Linux-x86_64) platform=linux; checksum=b14f5e5159219dd1a1956c3b806813319f5e94ccd5bdfd56f54520609a5bb5ec ;;
  Linux-aarch64) platform=linux_aarch64; checksum=237f8ef43fb40d16681871fb04b2c56f023afc02a5a3877831d35dd493823c23 ;;
  *) echo "Unsupported platform; install the pinned lean-toolchain manually." >&2; exit 1 ;;
esac
tool_dir=".tools/lean-4.24.0-$platform"
if [[ -x "$tool_dir/bin/lean" ]]; then
  "$tool_dir/bin/lean" --version
  exit 0
fi
command -v zstd >/dev/null || { echo "Install zstd before extracting Lean." >&2; exit 1; }
mkdir -p .tools
archive=".tools/lean-4.24.0-$platform.tar.zst"
curl --fail --location --silent --show-error \
  "https://github.com/leanprover/lean4/releases/download/v4.24.0/lean-4.24.0-$platform.tar.zst" \
  --output "$archive"
python3 - "$archive" "$checksum" <<'PY'
import hashlib, sys
with open(sys.argv[1], 'rb') as f:
    digest = hashlib.sha256()
    for block in iter(lambda: f.read(1024 * 1024), b''):
        digest.update(block)
if digest.hexdigest() != sys.argv[2]:
    raise SystemExit('Lean archive checksum mismatch')
print('Verified pinned official Lean archive checksum')
PY
tar --zstd -xf "$archive" -C .tools
"$tool_dir/bin/lean" --version
