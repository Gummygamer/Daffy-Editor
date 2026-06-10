# Graphics pipeline — decompress → WRAM → DMA (emulator-confirmed)

**Confidence: confirmed.** Captured live in Mesen2 (built from source; see
`tools/mesen/run-headless.sh`) on the USA ROM, boot → title screen. Raw report:
[reports/live_dma_capture.json](reports/live_dma_capture.json). This confirms,
end to end, the decompress-then-DMA hypothesis that
[dma-transfers.md](dma-transfers.md) and [dma-helper.md](dma-helper.md) inferred
statically — and pins down where the graphics actually live.

## The pipeline

```
ROM banks $92,$93,$95,$96  ──decompress──▶  WRAM $7F:C000-$7F:CFFF (+ $7F:D000)  ──DMA──▶  VRAM
   (compressed graphics)      $82:8549..       (tile staging area)                 $82:9BBE   (PPU)
```

| Stage | Where | Evidence |
|---|---|---|
| Compressed graphics in ROM | banks **$92, $93, $95, $96** (reads up to `$96:986D`; PC ≈ `0x90000`–`0xB7FFF`) | `trace_decompressor.lua` — ROM reads while the staging area is being filled |
| Decompressor routine | **`$82:8549`–`$82:8655`** (PC ≈ `0x10549`); dominant store `$82:85F8` (25.7k writes) | PCs that write `$7F:C000-CFFF` |
| WRAM tile staging | **`$7F:C000`–`$7F:CFFF`** (4 KB tiles) and **`$7F:D000`** (2 KB) | DMA source addresses |
| VRAM upload loop | **`$82:9BBE`** — 64 transfers of 64 bytes, `$7F:C080`→`$7F:CFC0` | `dma_log.lua` live capture |
| Tilemap upload | **`$82:9AE2`** — `$7F:D000`, 2048 bytes | live capture (matches static `scan_dma` 0x11ADF) |
| OAM upload | **`$80:92AC`** — `$00:1C6A`, 544 bytes | live capture (matches static `scan_dma` 0x12A9) |

The OAM and `$7F:D000` transfers reproduce the static `scan_dma` findings to
within a few bytes (trigger PC), which cross-validates both tools.

## What this nails down

- **Graphics are compressed and live in ROM banks `$92-$96`.** That is the
  first concrete graphics-storage location for this game. The pattern-scanner
  candidates elsewhere were, as predicted, not the raw storage.
- **The decompressor is `$82:8549-$82:8655`.** Its store loop fills the WRAM
  staging area; its ROM reads come from `$92-$96`.
- Every graphics/tilemap DMA sources from **WRAM**, never ROM — confirmed live,
  not inferred.

## Next steps

1. ~~Disassemble `$82:8549-$82:8655` to identify the compression scheme.~~
   **Done** — the routine is `$82:84FD-$82:865F`, a custom control-byte RLE.
   See [compression-codec.md](compression-codec.md). Still to do: write the
   codec and confirm it with a round-trip test before committing.
2. Find the **table that maps a graphics id → the `$92-$96` source address**
   (the loader sets the decompressor's source pointer from somewhere); that is
   the index the level/screen loader uses.
3. Re-run `trace_decompressor.lua` after reaching **level 1** (drive the GUI, or
   script input via `emu.setInput`) to capture the in-level graphics sources and
   confirm they also live in `$92-$96` (or find additional banks).
