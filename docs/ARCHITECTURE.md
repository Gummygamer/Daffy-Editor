# Architecture

Single native Rust desktop application (eframe/egui). No web stack, no
embedded browser. Windows and Linux are first-class; macOS should work via
eframe but is untested.

## Layering

```
┌───────────────────────────────────────────────┐
│ src/main.rs — eframe shell (window, run loop) │
├───────────────────────────────────────────────┤
│ src/app.rs — DaffyApp: all mutable state,     │
│   high-level actions (open/save/export/undo)  │
├──────────────┬────────────────────────────────┤
│ src/ui/*     │ GUI only. Calls DaffyApp       │
│              │ methods; never owns ROM bytes. │
├──────────────┴────────────────────────────────┤
│ Library (everything below is GUI-free,        │
│ deterministic, and unit-testable):            │
│  rom/      loading, header detect, hashes,    │
│            version id, bounds-checked I/O     │
│  snes/     LoROM mapping, 4bpp tiles, BGR555  │
│  model/    Project/Level/... + validation     │
│  editor/   commands, undo/redo, selection     │
│  patch/    IPS + BPS create/apply             │
│  rendering/ viewport math, RGBA tile renderer │
│  codecs/   experimental: heuristic scanners   │
│  error.rs  thiserror types                    │
└───────────────────────────────────────────────┘
src/bin/*  — CLI reverse-engineering tools on top of the library
```

## Key invariants

1. **ROM bytes live in one place.** `app::RomState` holds a single
   `rom::writer::RomImage` (original + working copy). UI code never receives
   a raw mutable buffer.
2. **Model ≠ binary format.** `model::*` is the canonical editor
   representation. Game-specific binary codecs live in `codecs/` and are only
   written once a structure is documented in `docs/reverse-engineering/` with
   `confirmed` confidence. Until then the editor runs on synthetic data
   labeled `Provenance::Synthetic` in the UI.
3. **All parsing is bounds-checked** (`RomReader`/`RomImage` return
   `RomError::OutOfRange`, never panic on user input).
4. **Edits go through commands.** `EditorCommand::apply` returns its inverse;
   `EditorHistory` provides undo/redo and save-point dirty tracking. UI
   never mutates the level directly.
5. **Export = diff.** Patches are generated from
   (original bytes, project changes) so they contain only user modifications.
   BPS embeds source/target/patch CRC32s; the IPS path validates input sizes.
6. **Determinism for tests.** `rendering::viewport_model` (zoom/pan math) and
   `rendering::tile_renderer` (RGBA buffers) are pure functions of their
   inputs — testable without a GPU or window.

## GUI structure (egui)

- `ui::menu` — menu bar + native file dialogs (rfd) + keyboard shortcuts
  (Ctrl+O/S/Z/Y).
- `ui::panels` — left side panel (ROM info, provenance banner, room selector,
  tool switch, metatile picker, palette viewer, object table via
  `egui_extras::TableBuilder`, validation list) and bottom status bar (status
  message, dirty dot, zoom, hovered tile, CRC32).
- `ui::viewport` — central canvas: painter-based tile rendering with view
  culling, scroll-to-zoom around the cursor, drag panning, screen-boundary
  lines, object/spawn/exit/checkpoint overlays, click selection, paint tool.
- `ui::dialogs` — About/legal window.

User preferences (`app::Prefs`: viewport, last directory, overlay toggles)
persist through eframe's storage (`persistence` feature).

## Threading

Currently single-threaded; rfd dialogs block the UI thread briefly, which is
acceptable for this tool. ROM scans run in the CLI tools, not in the GUI loop.

## Where new findings go

1. Hypothesis + evidence → `docs/reverse-engineering/<topic>.md` with a
   confidence label and `docs/RESEARCH_LOG.md` entry.
2. Once `confirmed`: implement a codec in `codecs/`, link the doc note from
   the code, add regression tests with synthetic fixtures reproducing the
   structure (never ROM bytes).
3. Wire into the model behind `Provenance::Confirmed`.
