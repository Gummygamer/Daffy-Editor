# Daffy Editor

A native desktop level editor and reverse-engineering workbench for the SNES
game **Daffy Duck: The Marvin Missions** — pure Rust, eframe/egui GUI, no web
stack.

> **You must provide your own legally obtained ROM.** This repository contains
> no game data of any kind and never will. See [docs/LEGAL.md](docs/LEGAL.md).

## Status

The game's level format is **reverse engineered and the editor reads real
levels from the ROM** — opening the supported USA ROM loads all 20 levels (no
more synthetic placeholder). See
[docs/reverse-engineering/level-format.md](docs/reverse-engineering/level-format.md).
What works today:

- ROM loading with copier-header detection/stripping, CRC32 + SHA-1 hashing,
  and identification of the supported USA ROM (LoROM, 1 MiB, CRC32
  `5F02A044`) — with a prominent warning for unknown ROMs.
- ROM info panel (hashes, internal SNES header, mapping).
- LoROM ↔ PC address conversion, bounds-checked ROM access.
- **Real level loading** (`level::load_rom_level`): master level table →
  per-scene setup routine → tilemap (`$D7:$D9`), tileset/metatiles (`$D3:$D5`),
  and palette (reconstructed from the scene's CGRAM graphics loads). A level
  picker switches between all 20 levels; verify with
  `cargo run --bin load_level -- <rom> all`.
- **Real tile-pixel graphics** for the recognized ROM: each scene's VRAM and
  palette are reconstructed statically by replaying its graphics loads, and
  metatiles are drawn as actual 4bpp tiles (not flat colors). See
  [docs/reverse-engineering/tile-graphics.md](docs/reverse-engineering/tile-graphics.md);
  inspect with `cargo run --bin render_level -- <rom> all`.
- Editor canvas with zoom/pan, tile painting, object overlay/moving, selection,
  undo/redo, dirty tracking, and validation (a synthetic level is still used as
  a fallback when no recognized ROM is open).
- JSON project save/load (stores ROM *hashes*, never ROM bytes).
- IPS and BPS patch export (changed bytes only; BPS carries checksums).
- CLI scanners for hunting pointer tables, palettes, tile graphics, repeated
  structures, and DMA upload sources. The highest-signal ones reconstruct DMA
  from the game's own setup code: `scan_dma` (immediate-fed transfers) and
  `scan_dma_helper` (the parameterized setup sites and the source-pointer
  variables they read).

## Quick start

```sh
cargo run --release      # the editor
cargo test               # automated tests, no ROM required
```

## Documentation

| Doc | Contents |
|---|---|
| [docs/USER_GUIDE.md](docs/USER_GUIDE.md) | using the editor & CLI tools |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | code layout & invariants |
| [docs/FORMAT.md](docs/FORMAT.md) | what is known about the ROM (with confidence labels) |
| [docs/RESEARCH_LOG.md](docs/RESEARCH_LOG.md) | dated reverse-engineering log |
| [docs/TEST_PLAN.md](docs/TEST_PLAN.md) | TDD strategy & manual checklist |
| [docs/LEGAL.md](docs/LEGAL.md) | what may never enter this repo |
| [docs/reverse-engineering/](docs/reverse-engineering/) | per-topic findings & scanner reports |
