# Reverse-Engineering Notes

One file per topic. Every claim carries a confidence label:

- **confirmed** — proven by tests, emulator observation, or unambiguous
  hardware definition. Code may depend on it; a regression test must exist.
- **likely** — strong evidence, not yet verified end-to-end. Code may surface
  it only behind a clearly labeled experimental view.
- **speculative** — a hypothesis or raw scanner output. Never hardcoded.
- **rejected** — investigated and disproven (kept to avoid re-treading).

Rules:

- Offsets are headerless PC offsets (`0x...`) or SNES addresses (`$BB:AAAA`).
- Every ROM offset referenced from source code must link back to a note here.
- Scanner JSON reports go in `reports/` (they contain offsets and hashes
  only — never ROM bytes).

## How to run the scanners

All tools take the path to *your own* ROM file and print JSON to stdout:

```sh
cargo run --bin scan_pointers        -- path/to/rom.sfc --min-entries 8
cargo run --bin scan_palettes        -- path/to/rom.sfc --min-rows 2
cargo run --bin scan_tile_patterns   -- path/to/rom.sfc --min-tiles 16
cargo run --bin scan_repeated_blocks -- path/to/rom.sfc --block-len 32 --min-count 4
cargo run --bin scan_dma             -- path/to/rom.sfc
cargo run --bin inspect_offset       -- path/to/rom.sfc --snes 0x008000 --len 64
cargo run --bin inspect_offset       -- path/to/rom.sfc --pc 0x7FC0 --len 32
```

`scan_dma` disassembles just enough 65816 to reconstruct general-purpose DMA
transfers (source address + VRAM/CGRAM/OAM destination) from their setup code.
Unlike the pattern scanners it yields a small, high-signal list.

## Index

- [rom-identity.md](rom-identity.md) — supported ROM identification (confirmed)
- [dma-transfers.md](dma-transfers.md) — DMA upload sources/destinations (likely)
