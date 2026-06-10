#!/usr/bin/env bash
# roundtrip.sh — verify the graphics decoder against real ROM data.
#
# 1. Runs Mesen2 with tools/mesen/roundtrip_decompressor.lua to capture the
#    decompressor's source pointer and the WRAM staging bytes it produced.
# 2. Feeds the captured dump + source address into `cargo run --bin roundtrip_gfx`,
#    which decodes the ROM bytes and compares.
#
# The captured dump contains decoded ROM graphics, so it is written to a temp
# file and never committed. Only the PASS/FAIL verdict is printed.
#
# Usage:
#   MESEN_BIN=/path/to/Mesen ./tools/roundtrip.sh "/path/to/Daffy Duck.smc" [timeout_s]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"

MESEN_BIN="${MESEN_BIN:?set MESEN_BIN to your built Mesen binary}"
ROM="${1:?usage: roundtrip.sh <rom> [timeout_s]}"
TIMEOUT="${2:-120}"

if [ -d "$HOME/.dotnet" ]; then
  export DOTNET_ROOT="$HOME/.dotnet" PATH="$HOME/.dotnet:$PATH"
fi
export DISPLAY="${DISPLAY:-:0}"

CAP="$(mktemp)"
trap 'rm -f "$CAP"' EXIT

echo ">> capturing decompressor ground truth via Mesen (timeout ${TIMEOUT}s)…" >&2
timeout "$TIMEOUT" "$MESEN_BIN" --testRunner "$ROM" \
  "$HERE/mesen/roundtrip_decompressor.lua" 2>/dev/null \
  | grep '^STRACE' > "$CAP" || true

if grep -q '^STRACE|timeout' "$CAP"; then
  echo "!! no qualifying decompress call was captured (try a longer timeout)" >&2
  exit 1
fi

SRC="$(sed -n 's/^STRACE|src=\([0-9A-Fa-f]*\).*/\1/p' "$CAP" | head -n1)"
if [ -z "$SRC" ]; then
  echo "!! capture did not report a source address; raw capture:" >&2
  cat "$CAP" >&2
  exit 1
fi

echo ">> decompressor source = \$$SRC; running Rust round-trip…" >&2
sed -n 's/^STRACE|dump //p' "$CAP" \
  | cargo run --quiet --manifest-path "$REPO/Cargo.toml" --bin roundtrip_gfx -- \
      "$ROM" --src "0x$SRC"
