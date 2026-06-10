# Mesen2 dynamic-analysis scripts

Live (emulator) reverse-engineering helpers that capture **ground truth** the
static scanners in `src/bin/` cannot reach. Mesen2 is **not** part of the build
and is never committed; supply your own. These scripts emit only
addresses/sizes/PCs — never ROM bytes — so their captures are safe to commit
under `docs/reverse-engineering/reports/`.

## Getting a working Mesen2

The official prebuilt Mesen2 Linux binary may crash at load with a libstdc++
`std::bad_cast` (a static-init `std::regex` incompatibility). Building from
source against your own toolchain fixes it:

```sh
# needs: .NET 8 SDK, SDL2, a C++17 compiler (gcc works)
git clone --depth 1 https://github.com/SourMesen/Mesen2 && cd Mesen2
USE_GCC=true make LTO=false -j"$(nproc)"
# binary: bin/linux-x64/Release/linux-x64/publish/Mesen
```

`--testRunner` needs an X display even though it renders nothing (`DISPLAY=:0`).

## Running

```sh
MESEN_BIN=/path/to/Mesen ./run-headless.sh /path/to/rom.smc dma_log.lua
MESEN_BIN=/path/to/Mesen ./run-headless.sh /path/to/rom.smc trace_decompressor.lua
```

`run-headless.sh` filters Mesen's noisy stdout down to the capture lines
(`DMACAP` / `DTRACE` / `STRACE`). Scripts default to a 1500-frame cap
(≈ boot → title screen); play to a level first (GUI, or `emu.setInput`) to
capture in-level data.

## Scripts

- **`dma_log.lua`** — hooks the MDMAEN trigger (`$420B`) and logs every unique
  transfer's true source→dest→size + trigger PC. Works headless (stdout) and in
  the GUI Script Window. Found the OAM/tilemap/tile uploads at title screen.
- **`trace_decompressor.lua`** — logs the PCs that fill the WRAM tile-staging
  area (`$7F:C000-CFFF`) — the **decompressor** — and which ROM banks it reads
  (the compressed-graphics source). Do **not** add `emu.getState()` to its ROM
  read callback (fires per opcode fetch → crash).

## Confirmed result (USA ROM, see `../../docs/reverse-engineering/graphics-pipeline.md`)

```
ROM banks $92,$93,$95,$96  ──$82:8549──▶  WRAM $7F:C000-CFFF  ──$82:9BBE DMA──▶  VRAM
   (compressed graphics)    (decompressor)   (tile staging)
```

## API gotchas (Mesen 2.x)

- callback-type enum is `emu.callbackType` (not `memCallbackType`);
- `emu.getState()` returns a FLAT table keyed `"cpu.pc"`/`"cpu.k"` (no nesting);
- Lua `io.*`/`dofile` corrupt the heap / are sandboxed under `--testRunner` —
  use `print()`, the only channel that reaches stdout headless.
