# Research Log

Newest entries first. Every entry states what was tried, on what data, and a
confidence label. Findings graduate to docs/FORMAT.md and
docs/reverse-engineering/ when they stabilize.

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
