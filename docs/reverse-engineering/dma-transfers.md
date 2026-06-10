# DMA Transfers — graphics/palette upload paths

Reconstructed by `scan_dma` from immediate-fed 65816 DMA setup code, then
labeled by destination register. Raw report:
[reports/scan_dma.json](reports/scan_dma.json). Confidence **likely** for the
individual transfers (well-formed, plausible sizes), **speculative** for the
interpretation below until confirmed in an emulator (Mesen2 event viewer /
DMA log).

## What the scanner found

Only **four** immediate-fed transfers exist in the whole 1 MiB ROM:

| Dest | B-bus | Source (SNES) | Size | Channel | Code @ PC |
|---|---|---|---|---|---|
| CGRAM | `$2122` | `$7F:C000` | — | 1 | `0x7CDD` |
| OAM   | `$2104` | `$00:1C6A` | 544 | 2 | `0x12A9` |
| VRAM  | `$2118` | `$7F:C000` | — | 1 | `0x7C93` |
| VRAM  | `$2118` | `$7F:D000` | 2048 | 1 | `0x11ADF` |

The 544-byte OAM transfer is exactly the SNES OAM table size (512 + 32),
which corroborates that the scanner is reading real DMA setup.

## Interpretation — likely

- **Every source is RAM, not ROM** (`$7F` = WRAM, `$00` low RAM). The game
  does not DMA graphics straight out of the ROM; it builds them in WRAM first
  and uploads from there. See [[compression]] reasoning in
  [../FORMAT.md](../FORMAT.md): RAM-staged uploads are the signature of a
  decompress-then-DMA pipeline, so tile/palette data is **likely compressed**
  in ROM rather than stored raw at the addresses the pattern scanners flag.
- **Most DMA goes through a parameterized helper.** Four immediate-fed
  transfers is far too few for a whole game, so the common upload path must
  load the channel registers from variables/tables (not `LDA #imm`), which a
  byte-pattern scanner cannot follow. `scan_dma` therefore captures only the
  fixed init/HUD uploads.

## Next steps to confirm / extend

1. In Mesen2, log DMA at the title screen and level 1 entry; compare the live
   source addresses against this table and against the `scan_tile_patterns` /
   `scan_palettes` candidates.
2. Trace the code that fills `$7F:C000` / `$7F:D000` *before* these transfers —
   that routine is the decompressor, and its ROM source pointer is the real
   lead for graphics storage.
3. Find the parameterized DMA helper (a routine that writes `$43x2/$43x4` from
   indexed addresses) and the table that feeds it.
