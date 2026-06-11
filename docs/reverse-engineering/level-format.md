# Level format — scene-setup pointer block & level table

**Confidence: pointer block + dimensions + per-level tilemap confirmed; region
semantics likely.** This is the bridge from "a level" to its data. There is **no
flat level table** in the ROM — each level is set up by a dedicated *routine*
that loads its graphics and then writes a fixed block of pointers/dimensions into
direct page + low RAM before handing control to the level engine.

Scanner: [`src/level/scan.rs`](../../src/level/scan.rs) (`scan_levels`). Dumper:
`cargo run --bin scan_levels -- <rom>`. Report:
[reports/scan_levels.json](reports/scan_levels.json). Live capture:
[`tools/mesen/trace_scene.lua`](../../tools/mesen/trace_scene.lua).

## How a level is selected — the master order table

The engine holds the **current level number in `$1EEA`**. To start a level
(`$80:E8A9`) it doubles that and indexes two **parallel word tables** in bank
`$80`, then far-calls the level's setup routine:

```
LDA $1EEA / ASL A / TAY
LDA $E8D8,Y -> $1EF6      ; per-level routine OFFSET table  ($80:E8D8)
LDA $E900,Y -> $1EF8      ; per-level routine BANK   table  ($80:E900)
... far-call $1EF8:($1EF6 + 6)   ; the level's setup routine
```

The two tables are **adjacent and exactly 20 entries** (`$E900 - $E8D8 = 0x28`
= 20 words; the bank table ends at `$E928` where code resumes) — so the game has
**20 ordered levels**. The 20 routine banks
(`81×5, 8A×2, 8C×5, 8D×2, 8E, 8F×4, 91`) match the per-level setup-routine banks
recovered independently below — multiset-identical except the one non-level
`$82` screen. Parser: [`src/level/index.rs`](../../src/level/index.rs)
(`parse_game_index`); the `scan_levels` report carries it under
`master_order_table`. **Confidence: likely.**

## How a level is set up

Each scene-setup routine (21 of them, scattered across banks `$81`, `$82`, `$8A`,
`$8C`, `$8D`, `$8E`, `$8F`, `$91`) does, in 16-bit accumulator mode:

1. A batch of inline `LDA #id : JSL $80:FC26` calls — the level's graphics (see
   [graphics-table.md](graphics-table.md)).
2. A block of `LDA #imm16 : STA <var>` writing its **data pointers + map size**:

| var | role | confidence |
|-----|------|------------|
| `$D3` | primary data **bank** | confirmed |
| `$D5` | shared per-world **tileset / metatile** offset (always `$8000`) | likely |
| `$D9` | **per-level tilemap** offset (`width*height` 16-bit cells) | **confirmed** |
| `$DB` | shared **attribute / collision** offset (`$A600` or `$C000`) | likely |
| `$DD` | map **width** in cells | confirmed |
| `$DF` | map **height** in cells | confirmed |
| `$1EF8` | secondary data **bank** (= the routine's own bank) | confirmed |
| `$1EF4` | **entity / object spawn list** offset | likely |
| `$1EFA` | handler / pointer-table offset | likely |

3. Sound-bank upload via `JSL $80:99AD` (the SPC700 uploader at `$80:FB48` — the
   `$BBAA` handshake + APU ports `$2140-$2143`; **not** level data), then hands
   off to the engine.

## The level table (21 levels)

`scan_levels` recovers the block from every routine (anchored on the distinctive
`STA $1EF8` = `8D F8 1E`, rejecting the one non-scene site). Levels group by
primary bank — each "world" shares a tileset/attribute bank, with its levels'
tilemaps packed contiguously:

| world (bank) | levels | tileset | attr | example maps (W×H) |
|---|---|---|---|---|
| `$88` | 4 | `$88:8000` | `$88:A600` | 80×24, 64×40, 64×64, 16×8 |
| `$89` | 4 | `$89:8000` | `$89:C000` | 128×16, 64×64, 8×8, 128×32 |
| `$8B` | 4 | `$8B:8000` | `$8B:C000` | 24×90, 88×40, 64×64, 13×13 |
| `$83` | 4 | `$83:8000` | `$83:C000` | 120×48, 104×32, 16×16, 208×24 |
| `$8D` | 5 | `$8D:8000` | `$8D:C000` | 8×8, 120×32, 112×56, 120×48, 16×16 |

(Small maps — 8×8, 13×13, 16×8/16 — are bonus/transition/boss rooms.) The full
list with every pointer is in `reports/scan_levels.json`.

## Per-level tilemap — confirmed format

`$D9` points at the level's tilemap: **`width * height` cells, 2 bytes each,
row-major, uncompressed**. Proven by contiguous packing in the `$88` world —
each level's `$D9` is exactly `previous + width*height*2`:

```
L0 $88:A86B  80×24  +0xF00  -> $B76B
L1 $88:B76B  64×40  +0x1400 -> $CB6B
L2 $88:CB6B  64×64  +0x2000 -> $EB6B
L3 $88:EB6B  16×8         (bonus room)
```

This only closes if each map is `width*height` two-byte cells stored back-to-back
— so the maps are **raw 16-bit metatile grids** (no compression; the
[graphics RLE](compression-codec.md) is not used here).

## Cell format — index decode confirmed

A cell is a reference into the per-world tileset (`$D5`), decoded by
[`src/level/cell.rs`](../../src/level/cell.rs):

```text
 15        5 4    0
+-+----------+-----+
|F|  index   |  0  |    value = (index << 5) | (F << 15)
+-+----------+-----+
```

- **bits 0..5 are always zero** — confirmed across the `$88`/`$8B`/`$83` worlds,
  *every* cell (thousands) has its low five bits clear. So the value is exactly
  `index * $20`, i.e. the **byte offset of the metatile** inside the tileset.
- **bits 5..15 = metatile index.** The largest index each world's maps use fits
  inside that world's tileset capacity (`(attr_off - $8000)/$20`):
  `$88` max 298 < 304, `$8B` max 276 < 512, `$83` max 378 < 512 — independent
  corroboration that the cell is a tileset offset.
- **bit 15 = a per-cell flag** (orientation/solidity — *likely*). `$8000`
  (index 0, flag set) is the dominant empty/sky cell of the upper rows.

So the **tileset** at `$D5` is a flat array of fixed **`$20`-byte metatile
definitions** (16 SNES tilemap words = a 4×4 block of 8×8 tiles, a 32×32-px
metatile — *likely* the exact shape). The `$DB` region (`$88:A600`, ~619 bytes ≈
304 metatiles × 2) is therefore a **per-metatile** attribute/collision table
(one entry per metatile, shared per world), not a per-cell map.

## Status of the level format

The end-to-end chain is mapped: **`$1EEA` (level number) → master order table
(`$80:E8D8`/`$80:E900`) → per-level setup routine → data-pointer block → tilemap
(`$D9`, `index<<5|flag` cells) + tileset (`$D5`, `$20`-byte metatiles) + objects
(`$1EF4`) + attributes (`$DB`)**. The pointer plumbing, the level count (20), the
tilemap layout and the cell index decode are confirmed; the object/attribute
*record* formats and the metatile pixel shape remain to be decoded.

## Next steps

1. Confirm bit 15's meaning and the 4×4 metatile shape by disassembling the
   column renderer that reads `$D9`/`$D5`, or by editing a cell live in Mesen2.
2. Decode the **entity / object spawn list** at `$1EF4`. The record **stride is
   confirmed 22 bytes** (`$0016`): the iterator at `$80:E9A8` random-accesses the
   list by `LDA $1EF4 → $16` then advancing `$16` by `#$0016` per object,
   `$1EE8` times (`ENTITY_RECORD_BYTES` in `src/level/scan.rs`). The per-field
   layout inside the 22 bytes is still undecoded — needs the field reader's
   disassembly or a live spawn observation.
3. Decode the per-metatile **attribute / collision** table at `$DB`.
4. Wire `level::scan` + `level::cell` + the tileset into the editor to render a
   real level (metatile expand → 4bpp tiles via [`crate::snes::tiles`]).
