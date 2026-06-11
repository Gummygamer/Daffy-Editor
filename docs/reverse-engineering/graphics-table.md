# Graphics descriptor table — id → compressed source (confirmed)

**Confidence: confirmed (source pointers).** This is the bridge the roadmap kept
flagging as missing: the table that maps a graphics id to a compressed-graphics
source address, which the loader feeds to the
[decompressor](compression-codec.md). The source pointers are cross-validated
two ways — a live loader trace and a full decode pass — so the id → tiles path is
now complete.

Parser: [`src/gfx/table.rs`](../../src/gfx/table.rs). Dumpers:
`cargo run --bin scan_gfx_table -- <rom>` (committable report) and
`cargo run --bin decode_gfx_table -- <rom>` (end-to-end pipeline check).
Reports: [reports/scan_gfx_table.json](reports/scan_gfx_table.json),
[reports/gfx_table_trace.json](reports/gfx_table_trace.json).

## Location & layout

The table is a flat array of **159 fixed 8-byte records** at the very start of
bank `$82`:

```
$82:8000  (PC 0x10000)  ... 159 records (1272 bytes) ...  $82:84F7
$82:84F8  (PC 0x104F8)  loader code: PHP / REP #$30 / PHX / PHY / ...
$82:84FD  (PC 0x104FD)  decompressor entry (falls through from the preamble)
```

Record (little-endian):

| bytes | field | meaning | confidence |
|---|---|---|---|
| `0` | `mode` | upload mode: `0` (59×) VRAM, `1` (33×) CGRAM, `2` (67×) WRAM | **confirmed** |
| `1..4` | `source` | 24-bit SNES pointer to the compressed blob | **confirmed** |
| `4..8` | `params` | mode-dependent upload target (see below) | **confirmed** |

The loader passes the id in **`Y`** as `id * 8` (the records' byte stride). The
id alone selects the source — the same id yields the same source regardless of
the loader's other (`X`/`A`) parameters.

### Mode byte & params — confirmed from the loader wrapper `$80:FC26`

Every id is loaded by `LDA #id : JSL $80:FC26` (302 such call sites in the ROM;
no data table of ids — graphics selection is **inline in each scene's setup
code**). The wrapper computes `Y = id*8`, sets `DB = $82`, copies the 24-bit
`source` into the decompressor's DP pointer `$16/$17/$18`, then dispatches on
`mode`:

| mode | dest of decompress | then | `params` meaning |
|---|---|---|---|
| `0` | `$7F:C000` | DMA → **VRAM** | `params[0..2]` = `$2116` VRAM **word** address; `params[2..4]` = DMA **byte size** |
| `1` | `$7F:C000` | DMA → **CGRAM** (palette) | `params[0]` = `$2121` CGRAM byte address; `params[2..4]` = DMA byte size |
| `2` | `params[0..3]` (24-bit WRAM) | — (no DMA) | the WRAM destination itself |

Decoded by [`GfxEntry::upload`](../../src/gfx/table.rs) → [`UploadTarget`]; the
`scan_gfx_table` report carries the decoded `upload` per entry. Disassembly of
the wrapper: `disasm <rom> --snes 0x80FC26 --end 0x80FD20 --m8 --x16`.

## How the loader was found

There is **no immediate `JSL $82:84FD` anywhere in the ROM** (verified
statically across both `$82` and its `$02` LoROM mirror, and as a pointer in any
table). The decompressor is reached by **fall-through**: the loader preamble at
`$82:84F8` (`PHP / REP #$30 / PHX / PHY`) runs straight into the entry at
`$82:84FD`. So the "caller" on the stack is just the preamble's saved `X`/`Y`.

`tools/mesen/trace_gfx_loader.lua` hooks the entry and records, per call, the
live source/dest pointers and the `X`/`Y`/`A` registers. Boot → title produced
**39 calls / 36 distinct ids**, and every one's live source pointer matched
`source(id)` from this table exactly; every `mode 2` call's live destination
matched the record's `params` dest. See `gfx_table_trace.json`.

## Source banks (wider than previously documented)

The descriptor sources span ROM banks **`$92`–`$9F`** (plus a single `$87`
record), not just the `$92/$93/$95/$96` that the boot→title
[`trace_decompressor`](graphics-pipeline.md) capture happened to touch:

```
92:44  93:13  94:7  95:7  96:8  97:2  98:3  99:7  9A:4  9B:8  9C:17  9D:13  9E:14  9F:11   (87:1)
```

`graphics-pipeline.md` and `FORMAT.md` are updated accordingly: compressed
graphics occupy roughly PC `0x90000`–`0xFFFFF` (banks `$92`–`$9F`).

## Validation

`decode_gfx_table` runs the committed [`gfx_rle`](../../src/codecs/gfx_rle.rs)
decoder on **every** record's source: all **159 decode cleanly** (no stream
underrun), totalling 910,276 decoded bytes. Record 2's source `$93:B9C9` is the
exact blob the earlier byte-for-byte Mesen2 round-trip already verified.

## Next steps

1. ~~Pin down the `mode` byte and the `mode 0/1` `params`~~ — **done** (loader
   wrapper `$80:FC26`, above): mode 0 = VRAM, 1 = CGRAM, 2 = WRAM, params decoded.
2. The id is selected **inline in scene-setup code** (302 `JSL $80:FC26` sites,
   not a data table), so the bridge from "a level" to "its graphics" is a code
   path, not a table — pursue the **level data** path separately (tilemap /
   object / enemy data). See `docs/reverse-engineering/level-format.md`.
3. Wire `gfx::table` + `gfx_rle` into the editor's tile renderer so a chosen id
   shows real decoded tiles (replacing the synthetic placeholder).
