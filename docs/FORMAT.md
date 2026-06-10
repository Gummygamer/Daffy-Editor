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

## Graphics — likely (generic), locations unknown

The SNES PPU dictates 4bpp planar tiles (32 bytes/tile) and BGR555 palettes;
decoding helpers are in `src/snes/tiles.rs` / `src/snes/palette.rs`
(confirmed as formats, since they are hardware-defined). *Where* this game
stores tiles/palettes, and whether they are compressed, is **unknown**.
Use `scan_tile_patterns` / `scan_palettes` to gather candidates.

## Level data — unknown

Level/tilemap/metatile/object/enemy/exit/collision formats: **no findings
yet**. The editor currently displays synthetic placeholder data, labeled as
such in the UI. Candidate-hunting tools: `scan_pointers`,
`scan_repeated_blocks`, `inspect_offset`.

## Compression — unknown

No compression scheme has been identified or ruled out.
