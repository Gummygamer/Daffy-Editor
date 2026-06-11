# ROM Format Notes — Daffy Duck: The Marvin Missions (SNES)

Confidence labels: **confirmed** / **likely** / **speculative** / **rejected**.
Anything not listed here is unknown. Do not write code against unconfirmed
structures (see docs/ARCHITECTURE.md, "Where new findings go").

## ROM identity — confirmed

| Property | Value | Source |
|---|---|---|
| Platform | SNES | — |
| Region (supported) | USA | No-Intro |
| Mapping | LoROM | internal header / No-Intro |
| Size | 1 MiB (0x100000, 8 Mbit) | No-Intro |
| SRAM | none | internal header |
| CRC32 (headerless) | `5F02A044` | No-Intro |

The editor identifies the supported ROM by CRC32 **and** exact size
(`src/rom/version.rs`). Anything else is `RomVersion::Unknown` and triggers a
prominent warning; views remain usable but are explicitly unverified.

## Copier headers — confirmed (generic)

Files whose size ≡ 512 (mod 32 KiB) carry a 512-byte copier header, which is
stripped on load. All offsets in this project are **headerless PC offsets**
unless written as `$BB:AAAA` (SNES address).

## Internal SNES header — confirmed (generic layout)

At PC `0x7FC0` (LoROM): 21-byte title, map mode (+0x15), cartridge type
(+0x16), ROM size (+0x17), SRAM size (+0x18), region (+0x19), checksum
complement (+0x1C, LE), checksum (+0x1E, LE). Parsed by `src/rom/info.rs`.
Field values for the real USA ROM should be recorded in
`docs/reverse-engineering/rom-identity.md` after first manual inspection.

## Graphics — storage location confirmed, format compressed

The SNES PPU dictates 4bpp planar tiles (32 bytes/tile) and BGR555 palettes;
decoding helpers are in `src/snes/tiles.rs` / `src/snes/palette.rs`
(confirmed as formats, since they are hardware-defined).

**Compressed graphics are stored in ROM banks `$92`–`$9F`** (PC ≈ `0x90000`–
`0xFFFFF`) — **confirmed**. The boot→title Mesen2 trace touched only
`$92/$93/$95/$96`, but the graphics descriptor table (below) sources every bank
from `$92` to `$9F` (plus one `$87`). They are decompressed by the routine at
**`$82:84FD`–`$82:865F`** into WRAM `$7F:C000-$7F:CFFF`, then DMA'd to VRAM. Full
pipeline:
[reverse-engineering/graphics-pipeline.md](reverse-engineering/graphics-pipeline.md).
The `scan_tile_patterns` / `scan_palettes` candidates elsewhere are therefore
*not* the raw storage, as predicted.

### Graphics descriptor table — confirmed (id → source)

A flat array of **159 fixed 8-byte records at `$82:8000`** (PC `0x10000`) maps a
graphics id to its compressed source: `mode(1) source24(3) params(4)`. The
loader (which falls through into the decompressor at `$82:84FD`) indexes it with
`Y = id*8`. The **24-bit source pointer is confirmed** — a live loader trace
matched 36 distinct ids and all 159 sources decode cleanly through the codec.
The **mode byte and params are also confirmed** from the loader wrapper
`$80:FC26` (reached by `LDA #id : JSL $80:FC26`, 302 inline call sites): mode 0
DMAs the decoded blob to VRAM (`params` = `$2116` word + size), mode 1 to CGRAM
(`params` = `$2121` addr + size), mode 2 decompresses straight to a WRAM address
(`params` = dest). Graphics ids are selected **inline by each scene's setup
code**, not from a data table. Parser `src/gfx/table.rs` (`GfxEntry::upload`);
dumpers `scan_gfx_table` / `decode_gfx_table`. See
[reverse-engineering/graphics-table.md](reverse-engineering/graphics-table.md).

## DMA uploads — likely

`scan_dma` reconstructs four immediate-fed DMA transfers (the fixed
init/HUD uploads). All of them source from **RAM** (`$7F` WRAM / `$00` low
RAM), never directly from ROM, and one is a textbook 544-byte OAM upload.
Details and emulator-confirmation steps:
[reverse-engineering/dma-transfers.md](reverse-engineering/dma-transfers.md).
Most uploads run through parameterized setup code the immediate scanner cannot
follow.

`scan_dma_helper` recovers those parameterized sites statically: 16 triggering
DMA setups in bank $00 (`$00:82xx`–`$00:95xx`) that load each channel's source
register from **direct-page pointers `$E7..$E9`, `$EA..$EC`, `$16..$18`** and
low-RAM `$1Fxx` variables — i.e. the source address is *computed into RAM*, not
read from a ROM immediate. The graphics lead is therefore the code that writes
those pointers (the loader/decompressor). See
[reverse-engineering/dma-helper.md](reverse-engineering/dma-helper.md).

## Level data — table + tilemap confirmed; cell/object formats in progress

There is **no flat level table**: each of the **21 levels** is set up by a
dedicated routine (banks `$81/$82/$8A/$8C/$8D/$8E/$8F/$91`) that loads its
graphics inline and then writes a fixed block of data pointers + map size into
direct page / low RAM (`$D3/$D5/$D9/$DB`, dims `$DD/$DF`, secondary
`$1EF8/$1EF4/$1EFA`). `scan_levels` (`src/level/scan.rs`) recovers that block
from all 21; three were caught live in Mesen2 with the exact same values.

The **per-level tilemap** (`$D9`) is **confirmed**: `width * height` cells,
**2 bytes each, row-major, uncompressed**. Proven by contiguous packing — in the
`$88` world each level's `$D9` is exactly `previous + width*height*2`
(`$A86B → $B76B → $CB6B → $EB6B` for 80×24, 64×40, 64×64). Levels group by
"world" bank that holds a shared tileset (`$D5` = `:$8000`) and attribute map
(`$DB` = `:$A600`/`:$C000`).

The **cell format** is decoded (`src/level/cell.rs`): a cell is
`(metatile_index << 5) | (flag << 15)` — its low 5 bits are always zero (verified
across thousands of cells in three worlds), so the value is the metatile's byte
offset into the tileset (`$20` bytes each = 16 tilemap words, a 4×4-tile
metatile). Every map's max index fits its world's tileset capacity. Bit 15 is a
per-cell flag.

Still **likely / in progress**: bit 15's exact meaning + the 4×4 metatile shape,
the entity/object spawn list (`$1EF4`), and the per-metatile attribute/collision
table (`$DB`). Full write-up:
[reverse-engineering/level-format.md](reverse-engineering/level-format.md);
report [reports/scan_levels.json](reverse-engineering/reports/scan_levels.json).
The editor still displays synthetic placeholder data until the cell format and
tileset are wired in.

## Compression — confirmed (codec round-trips against real ROM)

**Confirmed**: graphics are decompressed from ROM banks `$92-$96` into WRAM by
the routine `$82:84FD-$82:865F` before being DMA'd (live capture, not
inference). The decompress-into-RAM-then-DMA pipeline is real.

**Scheme — confirmed (round-trip verified):** the routine is a **custom
control-byte RLE** (not LZ — no back-references). A command byte's top 3 bits
select 1 of 7 operations (literal copy, byte/zero/incrementing/decrementing run,
2-byte pattern fill, end-of-pass), the low 5 bits give a run length `1..32`;
output is written with stride 2 over two passes (SNES 2-bitplane interleave).
Source is the 24-bit DP pointer `$16/$17/$18` (streams contiguously across LoROM
banks `$92/$93/$95/$96`); dest is `$19/$1A/$1B`. Full command table:
[reverse-engineering/compression-codec.md](reverse-engineering/compression-codec.md).

**The decoder is committed** at `src/codecs/gfx_rle.rs` (synthetic-fixture unit
tests). It was promoted to *confirmed* after a live round-trip: Mesen2 captured
a real decompress call (source `$93:B9C9`) and its 6208-byte WRAM staging
output, and the Rust decoder reproduced that output **byte-for-byte** from the
ROM source. Reproduce with `tools/roundtrip.sh` (needs your own ROM + Mesen2).
Tracking notes:
[reverse-engineering/graphics-pipeline.md](reverse-engineering/graphics-pipeline.md),
[reverse-engineering/dma-helper.md](reverse-engineering/dma-helper.md).
