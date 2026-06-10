# ROM Identity — USA release

Status: **confirmed** (external database + standard SNES facts)

| Property | Value |
|---|---|
| Title | Daffy Duck: The Marvin Missions |
| Region | USA |
| Mapping | LoROM |
| Size | 0x100000 (1 MiB / 8 Mbit) |
| SRAM | none |
| CRC32 (headerless) | `5F02A044` |
| SHA-1 (headerless) | *to be recorded from a verified dump* |

Source: No-Intro database, "Daffy Duck - The Marvin Missions (USA)".

Code references:
- `src/rom/version.rs` — `DAFFY_USA_CRC32`, `DAFFY_USA_ROM_SIZE`, `identify()`
- `src/snes/lorom.rs` — LoROM mapping used for all address conversion
- `src/rom/info.rs` — `LOROM_HEADER_OFFSET = 0x7FC0` (standard LoROM internal
  header location, generic SNES fact)

Open items:
- [ ] Record SHA-1 and the internal header field values (title bytes, map
  mode, checksum) from a legally obtained dump via
  `cargo run --bin inspect_offset -- <rom> --pc 0x7FC0 --len 32`.
- [ ] Note whether other regional releases (Europe/Japan "Looney Tunes")
  share engine layout — out of scope until USA is mapped.
