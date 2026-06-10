# Research Log

Newest entries first. Every entry states what was tried, on what data, and a
confidence label. Findings graduate to docs/FORMAT.md and
docs/reverse-engineering/ when they stabilize.

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
