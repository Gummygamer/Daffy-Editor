# Research Log

Newest entries first. Every entry states what was tried, on what data, and a
confidence label. Findings graduate to docs/FORMAT.md and
docs/reverse-engineering/ when they stabilize.

---

## 2026-06-11 — LEVEL FORMAT: scene pointer block + per-level tilemap (CONFIRMED)

- The bridge from a level to its data is **code, not a table**. New tool
  `tools/mesen/trace_scene.lua` hooks the per-scene sound-upload `$80:99AD` and
  dumps the data-pointer block the setup routine just wrote. Caught three scenes
  live, each with a consistent block: a **primary bank** + tileset (`$D5=$8000`)
  + per-level map (`$D9`) + attr (`$DB`), **map width×height** (`$DD/$DF`), and a
  **secondary bank** (`$1EF8`, = the routine's own bank) + entity list (`$1EF4`)
  + handler table (`$1EFA`). Scene #3's live values matched the static immediates
  of the setup routine `$81:800F` exactly.
- Static scanner `src/level/scan.rs` (`scan_levels`, TDD +6 synthetic tests)
  recovers the block from every scene routine — anchored on the distinctive
  `STA $1EF8` (`8D F8 1E`), rejecting the one non-scene site — giving the game's
  **21-level table** (bin `scan_levels`, report `reports/scan_levels.json`).
  Levels group by "world" bank (`$88/$89/$8B/$83/$8D`); each world shares its
  tileset/attr bank.
- **Per-level tilemap format CONFIRMED** = `width*height` 16-bit cells,
  row-major, **uncompressed**. Proof: in the `$88` world the four levels' `$D9`
  offsets are exactly `prev + width*height*2` apart
  (`$A86B→$B76B→$CB6B→$EB6B` for 80×24, 64×40, 64×64) — only closes for raw
  two-byte cells packed back-to-back. (Aside: `$80:99AD`/`$80:FB48` turned out to
  be the **SPC700 sound uploader** — `$BBAA` handshake + APU `$2140-3` — not level
  data; corrected.)

### Next research steps

1. Decode the 16-bit **cell layout** (metatile index vs. flags) + the `$D5`
   tileset/metatile expansion — disassemble the column renderer reading `$D9/$D5`.
2. Decode the **entity/object spawn list** (`$1EF4`) and the **attr/collision
   map** (`$DB`).
3. Wire `level::scan` + the tilemap into the editor's renderer.

---

## 2026-06-11 — Graphics loader wrapper `$80:FC26` (mode byte + params CONFIRMED)

- The descriptor table's `mode`/`params` were the last "likely" piece of the
  graphics path. Found the real caller of the decompressor by reading the stack
  at the **correct depth**: the loader preamble pushes `P`(1)+`X`(2)+`Y`(2) before
  the decompressor entry, so a `JSL` return address sits at `sp+6..8`, not
  `sp+1..3` (the earlier trace's `ret` was just the pushed `X`/`Y`). New tool
  `tools/mesen/trace_gfx_dispatcher.lua` reads `sp+6..8` and recovers three real
  call sites — all in bank `$80`: `$80:FC54`, `$80:FCA9`, `$80:FCF6`.
- Those are inside the **loader wrapper at `$80:FC26`** (disassembled). It takes a
  gfx **id in `A`**, computes `Y = id*8`, sets `DB = $82`, copies `source` into
  the decompressor DP `$16/$17/$18`, then `JSL $82:84F8` and dispatches on `mode`:
  - **mode 0** → DMA `$7F:C000` to **VRAM**: `params[0..2]` = `$2116` word addr,
    `params[2..4]` = byte size;
  - **mode 1** → DMA to **CGRAM** (palette): `params[0]` = `$2121` addr,
    `params[2..4]` = size;
  - **mode 2** → decompress straight to the WRAM address in `params[0..3]`.
  Mode/params now **confirmed**. Decoder: `GfxEntry::upload` → `UploadTarget`
  (TDD, +3 tests); `scan_gfx_table` emits the decoded `upload` per record.
- The wrapper is reached by **302 inline `JSL $80:FC26` sites** (no `JSR`, no id
  data table) — graphics ids are selected **inline in each scene's setup code**
  (e.g. the consecutive batch at `$81:8014/801B/8022/...`). So the bridge from a
  level to "its graphics" is a code path, not a table; the **level data** (tilemap
  / objects / enemies) must be pursued on its own. The graphics path is now
  end-to-end complete (id → source → decompress → upload target).

### Next research steps

1. Attack the **level format** directly: find the current-level RAM variable and
   the level loader; trace ROM reads during a level load to locate the level /
   mission pointer table and the tilemap/object encoding. See
   `docs/reverse-engineering/level-format.md`.

---

## 2026-06-10 — Graphics descriptor table FOUND (id → compressed source)

- The decompressor `$82:84FD` has **no immediate `JSL` caller** anywhere in the
  ROM (checked statically in bank `$82`, its `$02` LoROM mirror, and as a 3-byte
  pointer in any table). It is reached by **fall-through**: a loader preamble at
  `$82:84F8` (`PHP / REP #$30 / PHX / PHY`) runs straight into the entry.
- New dynamic tool `tools/mesen/trace_gfx_loader.lua` hooks the entry and reads
  the live source/dest pointers + `X`/`Y`/`A` per call. Boot → title: **39 calls,
  36 distinct ids**. The index is `Y = id * 8`; the same id always yields the
  same source regardless of `X`/`A`.
- That `*8` stride led straight to a **159-entry, 8-byte-record descriptor table
  at `$82:8000`** (PC `0x10000`), running right up to the loader code at
  `$82:84F8`. Record = `mode(1) source24(3) params(4)`. **Every** live call's
  source pointer matched `source(id)` exactly, and every `mode 2` call's live
  destination matched the record's `params` dest. — source pointers **confirmed**.
- Built `src/gfx/table.rs` (parser, TDD, 7 synthetic-fixture tests) + two bins:
  `scan_gfx_table` (committable id→source report) and `decode_gfx_table`
  (end-to-end check). `decode_gfx_table` runs the committed `gfx_rle` decoder on
  **all 159** sources: every one decodes cleanly (910,276 bytes total). Record 2
  (`$93:B9C9`) is the exact blob the earlier byte-for-byte round-trip verified.
- Sources span banks **`$92`–`$9F`** (plus one `$87`), wider than the
  `$92/$93/$95/$96` the boot→title trace happened to touch — `FORMAT.md` and
  `graphics-pipeline.md` corrected. Write-up:
  `reverse-engineering/graphics-table.md`; reports `scan_gfx_table.json`,
  `gfx_table_trace.json`.

### Next research steps

1. Decode the `mode` byte and `mode 0/1` `params` (suspected VRAM word + size)
   by tracing the upload stage that drains `$7F:C000` after each call.
2. Find what selects the **id** — a per-screen/per-level list of gfx ids (the
   loader's `X`/`A` likely index a higher-level scene table). That is the bridge
   from a level to its graphics.
3. Wire `gfx::table` + `gfx_rle` into the editor's tile renderer (replace the
   synthetic placeholder with real decoded tiles for a chosen id).

---

## 2026-06-10 — Decompressor disassembled (compression scheme IDENTIFIED)

- Built a linear **65816 disassembler** (`src/snes/disasm.rs`, full 256-opcode
  table + `M`/`X` width tracking via `REP`/`SEP`) and a `disasm` CLI bin, TDD
  (10 new tests on synthetic opcodes: immediate-width-follows-M/X, REP/SEP
  toggles mid-stream, long/indirect-long/relative/block-move formatting,
  inclusive SNES-range helper). Whole suite green.
- Ran it on the real ROM's decompressor. The routine is `$82:84FD`–`$82:865F`
  (the earlier `$82:8549` was just the dominant-store window). Disassembly read
  cleanly once the widths were pinned (entry sets 16-bit `A`, then `SEP #$20` at
  `$82:8505` → the main loop is 8-bit `A`, 16-bit `X`). Listing committed:
  `reverse-engineering/reports/disasm_decompressor.json`.
- **The compression scheme is a custom control-byte RLE** (NOT LZ — no sliding
  window / back-references). Main loop `$82:850B`: read a command byte through
  the 24-bit source pointer DP `$16/$17/$18`, `AND #$E0` selects 1 of 7 ops from
  the top 3 bits, low 5 bits are length−1 (`N = (cmd&0x1F)+1`, 1..32):
  literal-copy (`$00/$20`), end-of-pass (`$40`), 2-byte pattern fill (`$60`),
  byte-RLE (`$80`), incrementing run (`$A0`), zero-fill (`$C0`), decrementing
  run (`$E0`). Output via 24-bit dest pointer DP `$19/$1A/$1B` with **stride 2**,
  decoded **twice** (pass counter `$1F`=2, `$40` flips dest to base+1) ⇒ SNES
  2-bitplane interleave. The source pointer's wrap-to-`$8000`-on-bank-carry
  confirms it streams contiguously across LoROM banks `$92/$93/$95/$96`.
  Mechanics **decode-confirmed**; the bitplane-interleave intent **likely**.
  Write-up: `reverse-engineering/compression-codec.md`.

### Next research steps

1. Write the decoder in `src/codecs/` from the command table and **round-trip**
   it against a Mesen2 `$7F:C000` dump before committing/promoting to confirmed.
2. Find the loader that sets DP `$16/$17/$18` + dest + `$1F` before calling
   `$82:84FD` — that is the graphics-id → source-address index.
3. Disassemble the follow-on routine at `$82:8662` (writes DP `$E4…`); likely
   the next plane-pair / upload stage.

---

## 2026-06-10 — Live DMA capture in Mesen2 (graphics pipeline CONFIRMED)

- Got a working emulator: the official Mesen2 2.1.1 Linux binary crashes at
  load here (`std::bad_cast`), so built Mesen2 **from source** with GCC
  (`USE_GCC=true make LTO=false`, .NET 8 SDK installed user-local to `~/.dotnet`,
  SDL2 already present). The from-source `MesenCore.so` links against this
  system's libstdc++ and **does not crash** — headless `--testRunner` runs Lua.
- Mesen2 Lua API gotchas (now baked into `tools/mesen/*.lua`): the callback enum
  is `emu.callbackType` (not `memCallbackType`); `emu.getState()` is a FLAT table
  keyed `"cpu.pc"`/`"cpu.k"`; Lua `io.*` corrupts the heap under testRunner — use
  `print()` (the only stdout channel); stdout is noisy with
  `[CPU] Uninitialized memory read`, so filter by tag.
- `dma_log.lua` (hooks `$420B`) captured **64 unique transfers** boot→title. The
  OAM (`$80:92AC` ← `$00:1C6A`, 544B) and `$7F:D000` (`$82:9AE2`, 2048B) uploads
  reproduce the static `scan_dma` hits to within a few bytes — cross-validated.
- `trace_decompressor.lua` located the pipeline — **confirmed**:
  - compressed graphics live in ROM banks **`$92,$93,$95,$96`** (reads to
    `$96:986D`; PC ≈ `0x90000-0xB7FFF`);
  - the **decompressor is `$82:8549-$82:8655`** (dominant store `$82:85F8`),
    filling WRAM `$7F:C000-CFFF`;
  - the VRAM upload loop is `$82:9BBE` (64×64-byte chunks of `$7F:C080-CFC0`).
  Report: `reverse-engineering/reports/live_dma_capture.json`. Write-up:
  `reverse-engineering/graphics-pipeline.md`. This promotes graphics storage from
  *unknown* to a **confirmed** ROM location and compression from *likely* to
  *confirmed present*.

### Next research steps

1. Disassemble `$82:8549-$82:8655` to identify the **compression scheme** and its
   source pointer; write a codec only after a round-trip test passes.
2. Find the table mapping graphics-id → `$92-$96` source address (what sets the
   decompressor's source pointer) — that index is what the loader uses.
3. Reach **level 1** (drive the GUI or script `emu.setInput`) and re-run
   `trace_decompressor.lua` to capture in-level graphics sources.

---

## 2026-06-10 — Parameterized DMA helper trace (static)

- Tried to bring up **Mesen2** for live DMA logging (the roadmap's preferred
  path). Wrote `tools/mesen/dma_log.lua` (hooks the real `$420B` trigger to log
  every transfer's source→dest→size + PC). The official Mesen2 **2.1.1 Linux
  binary crashes at load** on this system — `std::bad_cast` thrown from a
  `std::regex`/`use_facet<collate>` in MesenCore.so's static init (gdb backtrace
  in session notes); not fixable via locale env. Dynamic analysis is therefore
  **parked** pending a from-source Mesen build or another scriptable emulator.
  The Lua logger is ready for whenever that lands.
- Added **`scan_dma_helper`** (static): clusters stores to the DMA registers
  (`$43xx`) and classifies each register's value as `immediate` vs
  `parameterized` (loaded from memory/table), then reports the operands feeding
  the source registers. Verified on planted code (indexed helper, immediate
  site, lone-store rejection). Report:
  `reverse-engineering/reports/scan_dma_helper.json`.
- On the USA ROM: **16 parameterized, triggering DMA setups**, all in bank $00
  (`$00:82xx`–`$00:95xx`). They load the channel source registers from
  direct-page pointers **`$E7..$E9` / `$EA..$EC` / `$16..$18`** (24-bit) and
  low-RAM `$1Fxx` variables — the source address is *computed into RAM*, never a
  ROM immediate. — **likely**. This sharpens the decompress-then-DMA picture:
  the graphics lead is now "whatever writes `$E7..$E9`/`$16..$18`", i.e. the
  loader/decompressor. See `reverse-engineering/dma-helper.md`.

### Next research steps

1. Trace the *writers* of DP `$E7..$E9` / `$16..$18` (a "stores to DP $E7" scan,
   or a DP write breakpoint once an emulator runs) — that routine is the loader.
2. `inspect_offset --snes 0x008412` — check the lone ROM-space source operand.
3. Get a working emulator (build Mesen2 from source / try BizHawk) and confirm
   the live transfer PCs fall inside the `$00:82xx`–`$00:95xx` sites.

---

## 2026-06-09 — DMA upload scan (first real-ROM finding)

- Added `scan_dma`: reconstructs general-purpose DMA transfers from immediate-fed
  65816 setup code, anchored on `STA $420B` and classified by B-bus destination.
  Verified on synthetic planted code (VRAM/CGRAM/OAM, 8- and 16-bit source
  immediates). Report: `reverse-engineering/reports/scan_dma.json`.
- Ran it on the USA ROM: exactly **four** immediate-fed transfers, all sourcing
  from **RAM** (`$7F` WRAM / `$00` low RAM), none from ROM. One is a 544-byte
  OAM upload (= SNES OAM table size), a good correctness check. — **likely**.
- Inference: graphics/palettes are staged in WRAM before upload, i.e. a
  decompress-then-DMA pipeline ⇒ ROM data is **likely compressed**, and the
  pattern-scanner tile/palette candidates are probably not the raw storage.
  Promoted "Compression" in FORMAT.md from *unknown* to *likely present*.
  See `reverse-engineering/dma-transfers.md`. Still **speculative** until a
  Mesen2 DMA log confirms the live sources.

### Next research steps

1. Mesen2: log DMA at the title screen and level 1; compare live sources to
   `scan_dma.json` and to the tile/palette candidates.
2. Trace the routine that fills `$7F:C000` / `$7F:D000` before the transfers —
   that is the decompressor; its ROM source pointer is the graphics lead.
3. Locate the parameterized DMA helper and the table that feeds its registers.

---

## 2026-06-09 — Project bootstrap (no ROM analysis yet)

- Recorded ROM identity for the supported USA release (LoROM, 1 MiB, no SRAM,
  CRC32 `5F02A044`) from the No-Intro database — **confirmed**. See
  `reverse-engineering/rom-identity.md`.
- Implemented and tested generic SNES facts (LoROM mapping, copier header
  detection, internal header layout, 4bpp tiles, BGR555 palettes) —
  **confirmed** as hardware/ecosystem standards, not game findings.
- Built heuristic scanners (`scan_pointers`, `scan_palettes`,
  `scan_tile_patterns`, `scan_repeated_blocks`, `inspect_offset`). Verified on
  synthetic planted data only. All of their output on the real ROM is to be
  treated as **speculative** until cross-checked in an emulator.
- No game-specific structure has been examined yet. Level format: **unknown**.

### Next research steps

1. Run all four scanners against a legally obtained USA ROM; commit the JSON
   reports (reports contain offsets/hashes only, no ROM bytes) under
   `reverse-engineering/reports/`.
2. Inspect the reset vector and early init code to find DMA routines that
   load tiles/palettes — their source addresses are high-value leads.
3. Cross-check palette candidates against Mesen2 CGRAM dumps at the title
   screen and first level.
4. Look for level pointer tables indexed by the level select / mission order.
