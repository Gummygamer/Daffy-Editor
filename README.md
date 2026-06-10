# Daffy Editor

A native desktop level editor and reverse-engineering workbench for the SNES
game **Daffy Duck: The Marvin Missions** — pure Rust, eframe/egui GUI, no web
stack.

> **You must provide your own legally obtained ROM.** This repository contains
> no game data of any kind and never will. See [docs/LEGAL.md](docs/LEGAL.md).

## Status

The game's level format is **not yet reverse engineered**. What works today:

- ROM loading with copier-header detection/stripping, CRC32 + SHA-1 hashing,
  and identification of the supported USA ROM (LoROM, 1 MiB, CRC32
  `5F02A044`) — with a prominent warning for unknown ROMs.
- ROM info panel (hashes, internal SNES header, mapping).
- LoROM ↔ PC address conversion, bounds-checked ROM access.
- A synthetic, clearly-labeled placeholder level rendered in the editor
  canvas with zoom/pan, tile painting, object overlay/moving, selection,
  undo/redo, dirty tracking, and validation.
- JSON project save/load (stores ROM *hashes*, never ROM bytes).
- IPS and BPS patch export (changed bytes only; BPS carries checksums).
- Five CLI scanners for hunting pointer tables, palettes, tile graphics and
  repeated structures — all output labeled *speculative*.

## Quick start

```sh
cargo run --release      # the editor
cargo test               # 82 automated tests, no ROM required
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
