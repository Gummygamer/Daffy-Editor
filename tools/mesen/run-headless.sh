#!/usr/bin/env bash
# Run a Mesen2 Lua capture script headlessly and print its tagged output.
#
# Mesen2 is NOT part of this project and is never committed. Build it from
# source (the official prebuilt Linux binary may crash at load with a libstdc++
# std::bad_cast — building against your own toolchain fixes it):
#   git clone --depth 1 https://github.com/SourMesen/Mesen2 && cd Mesen2
#   USE_GCC=true make LTO=false -j"$(nproc)"        # needs .NET 8 SDK + SDL2
#
# Then point MESEN_BIN at the built binary and pass a ROM + script:
#   MESEN_BIN=/path/to/Mesen ./run-headless.sh /path/to/rom.smc dma_log.lua
#
# Notes:
#  * --testRunner needs an X display even though it renders nothing (set DISPLAY).
#  * Mesen floods stdout with "[CPU] Uninitialized memory read" lines; we keep
#    only the capture lines (DMACAP / DTRACE / STRACE prefixes).
set -euo pipefail

MESEN_BIN="${MESEN_BIN:?set MESEN_BIN to your built Mesen binary}"
ROM="${1:?usage: run-headless.sh <rom> <script.lua> [timeout_s]}"
SCRIPT="${2:?usage: run-headless.sh <rom> <script.lua> [timeout_s]}"
TIMEOUT="${3:-120}"

# Framework-dependent single-file builds need DOTNET_ROOT on PATH.
if [ -d "$HOME/.dotnet" ]; then
  export DOTNET_ROOT="$HOME/.dotnet" PATH="$HOME/.dotnet:$PATH"
fi
export DISPLAY="${DISPLAY:-:0}"

timeout "$TIMEOUT" "$MESEN_BIN" --testRunner "$ROM" "$SCRIPT" 2>/dev/null \
  | grep -E '^(DMACAP|DTRACE|STRACE|GLOAD|GDISP|SCENE)'
