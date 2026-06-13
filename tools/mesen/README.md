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
- **`trace_gfx_loader.lua`** — hooks the decompressor entry (`$82:84FD`) and, per
  call, reads the CALLER state off the stack plus the source/dest pointers and
  `X`/`Y`/`A`. Revealed that the loader falls through into the decompressor and
  indexes the **graphics descriptor table** at `$82:8000` with `Y = id*8`. Emits
  addresses/registers only (safe to commit) → `reports/gfx_table_trace.json`. See
  `../../docs/reverse-engineering/graphics-table.md`.
- **`trace_entities.lua`** — enumerates the objects/items/enemies a level actually
  spawns. Hooks the activator `$80:E9A8` (entry) + `$80:E9CA` (after the 22-byte
  record copy to `$3B`) and dumps each unique record, **auto-driving input**
  (pulse START to gameplay, then hold RIGHT + pulse A to scroll the level) so
  spawns trigger headlessly. The static scanners cannot reach this — the spawn
  count is a runtime counter and the type field is unidentified (see
  `../../docs/reverse-engineering/level-format.md`). Its dump contains ROM record
  bytes, so (like `roundtrip_decompressor`) the output is **local-only, never
  committed**. Run with a longer timeout, e.g.
  `... run-headless.sh <rom> trace_entities.lua 1200`.
- **`gen_savestate_capture.py`** — generator (not a Lua script): emits a Lua that
  **loads a savestate and captures one frame** (screenshot + on-screen OAM +
  scene/list pointers). Use this instead of blind input to reach gameplay — it is
  deterministic, whereas driving `emu.setInput` to navigate menus is unreliable
  and destabilises the --testRunner sandbox. It embeds the .mss as base64 (Lua
  `io.*` is sandboxed) and loads it inside the NMI exec callback (`emu.loadSavestate`
  requires a CPU exec context). Single-frame only; make multiple savestates for
  multiple views. Confirmed level 0 live (Daffy = OAM #4–9). Output has ROM pixels
  → **local-only, never committed**:
  ```sh
  ./gen_savestate_capture.py <state.mss> [delay] > /tmp/cap.lua
  MESEN_BIN=... DISPLAY=:0 "$MESEN_BIN" --testRunner <rom> /tmp/cap.lua > out.txt
  grep '^SB|' out.txt | cut -c4- | base64 -d > shot.png    # the screenshot
  ```
- **`roundtrip_decompressor.lua`** — captures the decompressor's source pointer
  (`$16/$17/$18`) at entry (`$82:84FD`) and the staging bytes it produced at the
  RTL (`$82:8577`), for the first call sourcing from the gfx banks `$92/93/95/96`.
  Drives the codec round-trip via `../roundtrip.sh`. Its dump contains decoded
  ROM graphics, so (unlike the other scripts) its output is **local-only and
  never committed** — `roundtrip.sh` writes it to a temp file.

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
