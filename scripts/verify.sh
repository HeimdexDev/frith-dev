#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -n "${FRITH_LEAN_BIN:-}" ]]; then
  lake="$FRITH_LEAN_BIN/lake"
  lean="$FRITH_LEAN_BIN/lean"
else
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) platform=darwin_aarch64 ;;
    Darwin-x86_64) platform=darwin ;;
    Linux-x86_64) platform=linux ;;
    Linux-aarch64) platform=linux_aarch64 ;;
    *) echo "Set FRITH_LEAN_BIN to the pinned toolchain's bin directory." >&2; exit 1 ;;
  esac
  lake="$PWD/.tools/lean-4.24.0-$platform/bin/lake"
  lean="$PWD/.tools/lean-4.24.0-$platform/bin/lean"
fi
[[ -x "$lean" ]] || { echo "Run bash scripts/bootstrap-lean.sh first." >&2; exit 1; }
"$lean" --version | python3 -c 'import sys; s=sys.stdin.read(); print(s,end=""); assert "version 4.24.0," in s, "Wrong Lean toolchain"'
"$lake" -d formal build
python3 scripts/audit-proofs.py "$lake"
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo run --locked --bin frith-demo
