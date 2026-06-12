Scanner JSON reports (offsets and hashes only - never ROM bytes).

- `scan_pointers.json`, `scan_palettes.json`, `scan_tile_patterns.json`,
  `scan_repeated_blocks.json` — heuristic candidate lists (**speculative**).
- `scan_dma.json` — reconstructed immediate-fed DMA transfers (**likely**);
  see `../dma-transfers.md`.
- `scan_dma_helper.json` — parameterized DMA setup sites and the source-pointer
  variables they read (**likely**/**speculative**); see `../dma-helper.md`.
- `live_dma_capture.json` — Mesen2 live DMA capture + decompressor trace
  (**confirmed**): graphics in ROM banks `$92-$96`, decompressor `$82:8549`,
  upload loop `$82:9BBE`; see `../graphics-pipeline.md`.
- `disasm_decompressor.json` — full 65816 listing of the decompressor
  (`$82:84FD`-`$82:865F`) from `cargo run --bin disasm` (instruction-decode
  **confirmed**); the codec it implements is documented in
  `../compression-codec.md`.
- `scan_gfx_table.json` — the 159-entry graphics descriptor table at `$82:8000`
  (id → compressed source), from `cargo run --bin scan_gfx_table`; source
  pointers **confirmed**. See `../graphics-table.md`.
- `gfx_table_trace.json` — `trace_gfx_loader.lua` live capture cross-validating
  the table's source/dest pointers against 36 distinct decompress calls
  (**confirmed**).
- `scan_levels.json` — the **21-level table**: each scene-setup routine's
  recovered data-pointer block (primary bank + tileset/map/attr offsets, map
  width×height, secondary bank + entity/handler offsets), from
  `cargo run --bin scan_levels`. Pointer block + dimensions + per-level tilemap
  format **confirmed** (live trace + contiguous packing); region semantics
  **likely**. See `../level-format.md`.
- `trace_fields.json` — `trace_fields.lua` live read-watch (driven into level 0)
  + disassembly of the caught reader PCs: the metatile **rendering pipeline**
  (cell → `$D5` tileset → 4×4 tile words → `$DB` per-tile-char attribute) and the
  22-byte **object record** fields, all **confirmed**. See `../level-format.md`.
- `render_level.json` — per-level **tile-graphics coherence** report from
  `cargo run --bin render_level -- <rom> all`: each scene's statically
  reconstructed VRAM/palette and the share of referenced tile characters that
  resolve to populated VRAM at character base `$2000` (98–100% for all 20
  levels). Decode/placement **confirmed**, char base **likely**. See
  `../tile-graphics.md`.
