# Test Plan — Daffy Editor

Native desktop level editor for *Daffy Duck: The Marvin Missions* (SNES, USA, LoROM, 1 MiB, CRC32 `5F02A044`).

## Principles

1. **Tests first.** Every core module gets failing tests before implementation (red → green → refactor).
2. **No copyrighted data.** Automated tests never read a real ROM. All fixtures are synthetic
   buffers built in test code or generated into `test-fixtures/synthetic_roms/` at test time
   (via `tempfile` when a real file path is needed).
3. **Determinism.** Rendering-model and codec tests operate on pure data transformations,
   never on live GPU/window state.
4. **Speculation is not tested as truth.** Only *confirmed* format findings get regression
   tests against fixed offsets; speculative scanners are tested for behavior, not for
   game-specific results.

## Test layers

| Layer | Location | Runner |
|---|---|---|
| Unit tests | `#[cfg(test)]` modules in `src/**` | `cargo test` |
| Integration tests | `/tests/*.rs` | `cargo test` |
| Snapshot tests | `insta` (project JSON, validation reports) | `cargo test` |
| Manual checklist | this file, bottom section | human + legally obtained ROM |

## Module test matrices

### 1. ROM loading & normalization (`rom::loader`) — `tests/rom_tests.rs`

| Case | Input | Expected |
|---|---|---|
| Unheadered ROM | 1 MiB synthetic buffer | loads, `had_copier_header == false`, size 0x100000 |
| Headered ROM | 512 + 1 MiB buffer | header stripped, `had_copier_header == true`, size 0x100000 |
| Minimum size | 32 KiB buffer | loads |
| Too small | 100-byte buffer | `RomError::TooSmall` |
| Odd size | 1 MiB + 17 bytes | `RomError::BadSize` |
| Headered minimum | 512 + 32 KiB | header stripped |
| File round trip | temp file on disk | identical bytes after load |

### 2. Hashing & version identification (`rom::info`, `rom::version`)

| Case | Expected |
|---|---|
| CRC32 of known synthetic buffer | matches independently computed constant |
| SHA1 of known synthetic buffer | matches known hex digest |
| `identify(0x5F02A044, 0x100000)` | `RomVersion::DaffyDuckMarvinMissionsUsa` |
| `identify(0x5F02A044, wrong size)` | `Unknown` (hash collision guard) |
| `identify(other, 0x100000)` | `Unknown` |
| Internal header parse | title/map-mode/checksum fields read from offset 0x7FC0 of synthetic LoROM image |
| Internal header on tiny ROM | no panic; graceful absence |

### 3. LoROM address conversion (`snes::lorom`) — `tests/lorom_tests.rs`

| Case | Expected |
|---|---|
| `$00:8000` → PC | `0x000000` |
| `$01:8000` → PC | `0x008000` |
| `$00:FFFF` → PC | `0x007FFF` |
| `$1F:FFFF` → PC | `0x0FFFFF` (last byte of 1 MiB) |
| `$80:8000` (fast mirror) → PC | `0x000000` |
| `$FF:FFFF` → PC | `0x3FFFFF` (ROM, not WRAM) |
| `$00:0000`–`$00:7FFF` | error: not ROM in LoROM |
| `$7E:xxxx`, `$7F:xxxx` | error: WRAM |
| addr > `$FFFFFF` | error |
| PC → SNES round trip | `pc_to_snes(snes_to_pc(a)) == a` for canonical addresses |
| PC beyond LoROM range | error |

### 4. Bounds-checked ROM access (`rom::reader`, `rom::writer`)

| Case | Expected |
|---|---|
| `read_u8/u16_le/u24_le` in range | correct little-endian values |
| any read crossing end of ROM | `RomError::OutOfRange`, no panic |
| slice with huge len (overflow bait) | error, no panic |
| read via SNES address | equals read via converted PC offset |
| write in range | byte changed, original preserved |
| write out of range | error, buffer untouched |
| `diff()` after writes | exactly the changed runs, nothing else |
| write same value as original | not reported as a change |

### 5. IPS patch (`patch::ips`) — `tests/patch_tests.rs`

| Case | Expected |
|---|---|
| create on identical buffers | patch with zero records, applies as no-op |
| single changed byte | one record, only that byte in patch |
| two distant runs | two records |
| run longer than 0xFFFF | split into multiple records |
| change at offset 0x454F46 (`"EOF"`) | record start shifted so it never encodes literal `EOF` offset |
| create→apply round trip | patched copy of original == modified |
| apply truncated patch | `PatchError`, no panic |
| apply bad magic | `PatchError::BadMagic` |
| RLE record apply | supported on apply |
| record past end of target | target grows (IPS growth semantics) |
| offset beyond 0xFFFFFF in create | error (IPS limit) |

### 6. BPS patch (`patch::bps`)

| Case | Expected |
|---|---|
| create→apply round trip | output == target, CRCs verify |
| apply with wrong source | `PatchError::SourceChecksumMismatch` |
| corrupted patch body | checksum/parse error, no panic |
| varint encode/decode round trip | all of 0, 1, 127, 128, 0xFFFF, 0xFFFFFFFF |
| metadata field | preserved through round trip |

### 7. Project model (`model::*`) — `tests/project_tests.rs`

| Case | Expected |
|---|---|
| JSON round trip | `Project` == deserialize(serialize(p)) |
| `insta` snapshot of synthetic project | stable schema (catches accidental format breaks) |
| unknown ROM in project | loads with `version: Unknown`, warning-level validation issue |
| validation: tile index ≥ metatile count | `Error` issue |
| validation: object outside room bounds | `Warning` issue |
| validation: exit → nonexistent room | `Error` issue |
| validation: clean synthetic level | zero issues |

### 8. Editor commands & history (`editor::*`) — `tests/editor_command_tests.rs`

| Case | Expected |
|---|---|
| `SetTile` applies | tile changed |
| undo | original value restored |
| redo | edit re-applied |
| undo on empty history | no-op, no panic |
| new edit after undo | redo stack cleared |
| `MoveObject` + undo/redo | position round trips |
| dirty tracking | clean → dirty on edit → clean on mark-saved → dirty again on undo past save point |
| selection set/clear | state transitions correct |
| command on out-of-range target | error, level unchanged, history unchanged |

### 9. SNES graphics decoding (`snes::tiles`, `snes::palette`) — unit tests

| Case | Expected |
|---|---|
| 4bpp decode of hand-built tile | exact 8×8 pixel-index matrix |
| 4bpp encode→decode round trip | identity |
| all-zero tile | all pixel 0 |
| BGR555 → RGB8 | known colors (black, white, pure R/G/B) map exactly, 5→8 bit scaling correct |

### 10. Rendering model (`rendering::*`) — unit tests

| Case | Expected |
|---|---|
| world↔screen round trip at zoom 1, 2, 0.5 | identity |
| zoom-at-cursor | cursor's world point invariant |
| zoom clamped | within [min, max] |
| tile hit-test from screen point | correct (tile_x, tile_y) |
| room → RGBA buffer | deterministic bytes for a fixed synthetic room |
| tileset (4bpp + palette) → RGBA | deterministic bytes for fixed input |

### 11. Scanner CLI tools (`src/bin/*`)

Tested at library level (`codecs::experimental` / scanner functions):
- pointer scan on a synthetic buffer with a planted 16-bit pointer table → table found at planted offset.
- palette scan on a buffer with a planted plausible CGRAM block → candidate reported.
- repeated-block scan with planted duplicates → duplicates reported.
- scans never report results past the end of the buffer; empty buffer → empty report, no panic.

## Manual test checklist (requires a legally obtained ROM — never in CI)

- [ ] Open unheadered USA ROM (`.sfc`, 1,048,576 bytes): CRC32 shows `5F02A044`, version badge shows "USA (supported)".
- [ ] Open headered copy (`.smc`, 1,049,088 bytes): "copier header detected & stripped" note shown; same CRC32 as above.
- [ ] Open any other SNES ROM: prominent **unknown ROM** warning; editor stays read-only-safe.
- [ ] ROM info panel shows internal title, map mode, checksum/complement from offset `0x7FC0`.
- [ ] Synthetic level renders; zoom (scroll), pan (drag), tile selection, object overlay all work.
- [ ] Undo/redo via menu and Ctrl+Z / Ctrl+Y.
- [ ] Save and reopen a project JSON; state survives.
- [ ] Export IPS with no changes → clear "nothing to export" message, not an empty file.
- [ ] (Later milestones) apply exported IPS in Flips/emulator and verify behavior.

## Definition of done per milestone

A milestone is complete only when: all its planned tests exist and pass via `cargo test`,
no test depends on copyrighted data, and any new ROM offsets used in code are linked to a
note in `/docs/reverse-engineering/` with a confidence label.
