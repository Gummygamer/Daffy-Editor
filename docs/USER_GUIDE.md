# User Guide

## Requirements

- Windows or Linux (macOS untested but expected to work).
- Rust toolchain (build from source): https://rustup.rs
- Your own, legally obtained ROM of *Daffy Duck: The Marvin Missions* (USA).
  The editor ships with **no game data** and will not work without your file.

## Building and running

```sh
cargo run --release
```

## First steps

1. **File > Open ROM…** (Ctrl+O) and select your `.sfc`/`.smc` file.
   - A 512-byte copier header is detected and stripped automatically.
   - The side panel shows size, CRC32, SHA-1, internal header fields, and a
     version badge. Green = supported USA ROM (CRC32 `5F02A044`).
   - An orange **UNKNOWN ROM** warning means the hash is not recognized:
     the editor will still show generic info, but nothing game-specific is
     guaranteed to be correct.
2. The central canvas currently shows a **synthetic placeholder level**
   (clearly labeled). The real level format is still being reverse engineered;
   the canvas exists so editing workflows are ready the moment it is decoded.

## Canvas controls

| Action | Input |
|---|---|
| Zoom (around cursor) | mouse wheel |
| Pan | middle/right drag (or left drag with Select tool) |
| Select tile/object | left click (Select tool) |
| Move selected object | drag it, release to drop (Select tool) |
| Paint metatile | left click/drag (Paint tool; pick a metatile in the side panel) |
| Undo / Redo | Ctrl+Z / Ctrl+Y (or Edit menu) |

View menu toggles: tile grid, screen boundaries, object overlay, collision
overlay. The status bar shows the hovered tile coordinate, zoom, the ROM
CRC32, and an "unsaved" dot when there are unsaved edits.

## Projects

- **File > Save Project** (Ctrl+S) writes a JSON project file: your levels,
  byte-level changes, and the *identity* (hashes) of the ROM — never ROM data.
- **File > Open Project…** restores it. Projects are text; diff them in git.

## Exporting your hack

- **File > Export IPS Patch…** — classic format, applies with Flips, etc.
- **File > Export BPS Patch…** — includes source/target checksums, so players
  get a clear error if they patch the wrong ROM.
- Patches contain only your changed bytes. If you have made no byte-level
  changes yet, the editor says so instead of writing an empty patch.
- **Export Modified ROM** writes a patched copy for testing in your emulator.
  Keep it local; distribute patches only.

## Reverse-engineering CLI tools

See docs/reverse-engineering/README.md for the five scanner/inspector tools
and how to record findings.

## Troubleshooting

- *"ROM file too small" / "unexpected ROM size"* — the file is not a SNES ROM
  image (or is corrupt). Sizes must be a multiple of 32 KiB, optionally +512.
- *Linux: file dialog does not appear* — rfd uses the XDG desktop portal;
  ensure `xdg-desktop-portal` (and a backend like `-gtk`/`-kde`) is installed.
