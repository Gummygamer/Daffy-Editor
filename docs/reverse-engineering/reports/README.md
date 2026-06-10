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
