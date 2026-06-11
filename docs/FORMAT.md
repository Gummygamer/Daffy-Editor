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
Parser `src/gfx/table.rs`; dumpers `scan_gfx_table` / `decode_gfx_table`. See
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

## Level data — unknown

Level/tilemap/metatile/object/enemy/exit/collision formats: **no findings
yet**. The editor currently displays synthetic placeholder data, labeled as
such in the UI. Candidate-hunting tools: `scan_pointers`,
`scan_repeated_blocks`, `inspect_offset`.

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
