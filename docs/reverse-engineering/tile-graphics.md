# Per-scene tile graphics: static VRAM reconstruction

This is the last link in the graphics chain: turning a level's metatile **tile
words** into actual **pixels**. The editor now renders real ROM tiles, not flat
placeholder colors.

## The chain

A level cell → metatile index → 16 tile words (a 4×4 block of 8×8 tiles, see
[level-format.md](level-format.md)). Each tile word is a standard SNES tilemap
entry whose **character** is `tile_word & 0x3FF`. That character indexes tile
pixel data in **VRAM**. To draw a metatile we therefore need the scene's VRAM.

VRAM is never stored in ROM directly — it is uploaded at scene-setup time by the
graphics loader (`$80:FC26`, see [graphics-table.md](graphics-table.md)). So we
**replay the scene's graphics loads statically**:

1. Scan the scene-setup routine body for inline `LDA #id : JSL $80:FC26`
   (`A9 id [00] 22 26 FC 80`). The id can be loaded 8- or 16-bit. The loads are
   scattered through the *whole* routine, not just before the pointer block —
   the bank-`$81` world writes its `$D3..$DF` pointer block **first** and loads
   graphics afterwards — so the scan runs from the routine entry to the next
   scene routine, capped at the routine's own LoROM bank.
2. For each load, look the id up in the descriptor table and act on its `mode`:
   - **mode 0 (VRAM):** decompress the source and place it at its true `$2116`
     word address (byte offset `word_addr * 2`) in a 64 KiB VRAM buffer.
   - **mode 1 (CGRAM):** decompress to BGR555 colors at its `$2121` address →
     the palette.
   - mode 2 (WRAM) loads are sprite/work data, ignored for the background.

Implemented in `src/level/loader.rs` (`reconstruct_vram`, `reconstruct_palette`,
`read_attr_table`); rendered in `src/rendering/tile_renderer.rs`
(`render_metatile_rgba`).

## Background character base — `$2000` (validated statically)

A tilemap character is only 10 bits (`& 0x3FF`, 0..1023); the PPU adds the BG
**character base** before fetching, so tile `char c` lives at VRAM word
`char_base + c * 16`. The scene uploads its main background tile sheet in one
large mode-0 DMA, and **that DMA's `$2116` word address is the character base**.
For every level it is the largest mode-0 load's word address, `$2000`.

This is validated without an emulator: with `char_base = $2000`, the characters
the level's placed metatiles reference resolve to **populated** (non-zero) VRAM
tiles almost perfectly — see `reports/render_level.json`:

| coverage | levels |
|----------|--------|
| 100% | 0,1,2,3,5,6,7,8,9,13,16,17,18,19 |
| 98–99% | 4,10,11,12,14,15 |

The 1–2 char misses per level are the all-zero blank/transparent character
(`char 0`), which is legitimately empty. A wrong base scatters coverage to
~5–40% (the sweep in the tool's earlier revisions), so this is a sharp signal.

## Per-character attributes — the `$DB` table

The renderer (`$80:F5F1`) reads `$DB[char]` to get each tile's SNES tilemap
**high byte**: palette row in bits 2..4 (`(attr >> 2) & 7`), h-flip bit 6, v-flip
bit 7. `render_metatile_rgba` applies exactly this. Pixel index 0 of any row is
the SNES backdrop (`CGRAM[0]`), matching single-layer-over-backdrop compositing.

## Confidence

- VRAM/palette **decode + placement**: confirmed (decoder round-trip-confirmed
  in [compression-codec.md](compression-codec.md); mode targets confirmed in
  [graphics-table.md](graphics-table.md)).
- **char base `$2000`** and the **`$DB` attribute interpretation**: *likely* —
  strongly corroborated by the static coverage above and the renderer
  disassembly. A live PPU VRAM/CGRAM dump would promote them to *confirmed*; the
  byte-for-byte cross-check is a future step (it must use the `print`-hex output
  idiom, not `io.*`, which corrupts Mesen's heap under `--testRunner`).

## Reproduce

```sh
# compact coherence report for all 20 levels (aggregate stats, committable):
cargo run --bin render_level -- <rom> all
# one level + a rendered PNG (LOCAL ONLY — contains ROM graphics, never commit):
cargo run --bin render_level -- <rom> 0 --png /tmp/level0.png
```
