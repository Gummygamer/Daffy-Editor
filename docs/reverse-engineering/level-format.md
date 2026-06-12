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
| `$D3` | **tileset** data **bank** | confirmed |
| `$D5` | shared per-world **tileset / metatile** offset (always `$8000`) | likely |
| `$D7` | **tilemap bank** (often `== $D3`, but distinct when the map is in another bank) | **confirmed** |
| `$D9` | **per-level tilemap** offset (`width*height` 16-bit cells), in bank `$D7` | **confirmed** |
| `$DB` | shared **attribute / collision** offset (`$A600` or `$C000`) | likely |
| `$DD` | map **width** in cells | confirmed |
| `$DF` | map **height** in cells | confirmed |
| `$1EF8` | secondary data **bank** (= the routine's own bank) | confirmed |
| `$1EF4` | **entity / object spawn list** offset | likely |
| `$1EE8` | **object count** for the spawn-list iterator (`$80:E9A8`) | likely |
| `$1EFA` | handler / pointer-table offset | likely |

> **The tilemap bank is `$D7`, not `$D3`.** Every setup routine writes
> `LDA #bank : STA $D7` right beside `$D5`/`$D9`. For most worlds `$D7 == $D3`
> (tileset and map share a bank), which originally hid the distinction — but
> several scenes put the map in a *separate* bank, e.g. tileset `$89:8000` with
> map **`$8A:8000`** (level 6), or tileset `$83:8000` with map `$83:8000` vs a
> neighbour at a different bank. Using `$D3` for the map there decodes the
> *tileset* as a map and yields wildly out-of-range metatile indices. With `$D7`
> as the map bank, **all 20 levels** decode with their max metatile index inside
> the tileset capacity (0 out-of-range cells) — the same invariant that confirms
> the cell format, now holding game-wide.

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
definitions**.

## Rendering pipeline — confirmed (`$80:F5A8`–`$80:F5F7`)

A live read-watch (`tools/mesen/trace_fields.lua`, driven into level 0) caught
the metatile renderer; disassembled, it nails the whole expansion:

```
metatile_def = $D5 (tileset) + (cell & 0x7FFF)        ; bit 15 masked off here
tile_word    = metatile_def[(subrow&3)*8 + (subcol&3)*2]   ; 4×4 grid, 16 words
char_index   = tile_word & 0x03FF                     ; SNES tilemap char
attr_byte    = $DB[char_index]                        ; 1 byte per tile char
```

This **confirms**: (a) the cell is a tileset byte offset with **bit 15 a flag the
renderer ignores** for tile selection (collision/priority — likely); (b) a
metatile is a **4×4 block of 8×8 tiles (32×32 px)**, row stride 8 bytes
([`metatile_word_offset`](../../src/level/cell.rs)); (c) the **`$DB` table is
indexed per tile character** (`char & 0x3FF`), *not* per-cell or per-metatile.
Readers: map cell `$80:F5B9`, attribute `$80:F5F1`.

**`$DB` is NOT the display attribute source.** The tile word itself is a full
SNES tilemap word — palette row bits 10..12, priority 13, h/v-flip 14/15 —
live-confirmed against the real BG1 tilemap (109/109 on-screen chars, see
[tile-graphics.md](tile-graphics.md)). What the `$DB[char]` byte feeds is open
(collision/priority hypothesis).

## Object / entity records — confirmed stride + partial fields

The spawn list (`$1EF4`) is read by the object processor `$80:E99D`: it copies a
**22-byte record** (`($16),Y` loop, `$80:E9BF`) into direct page `$3B..$50`, then
interprets the fields (`$80:E9CB`+):

| record offset | use | confidence |
|---|---|---|
| `$04` | packed **Y position** (`AND #$01E0 >> 3`) | likely |
| `$06` | packed **X position** (`AND #$00E0 << 2`, low byte `>> 5`) | likely |
| `$0C` | **map column** (added to map base `$D9`) | likely |
| `$0E` | **object type** — dispatched via `JSR $80:F1FD` | likely |
| others | per-type parameters (7 more words) | unknown |

## Editor integration — the level loader

[`src/level/loader.rs`](../../src/level/loader.rs) (`load_rom_level`) walks the
whole chain and builds the editor's [`Level`](../../src/model/level.rs) from ROM
bytes — **the editor now reads real levels instead of the synthetic placeholder**:

1. `parse_game_index` → pick the level's setup routine; `scan_levels` → recover
   its pointer block (matched to the routine by bank + nearest anchor).
2. **Tilemap** ← `width*height` 16-bit cells at `$D7:$D9` (cell → metatile index).
3. **Metatiles** ← the tileset at `$D3:$D5`, `(attr-$8000)/$20` defs of 16 words.
4. **Palette** ← replay the routine's inline `LDA #id : JSL $80:FC26` loads;
   every **mode-1** entry ([`gfx::table`](../../src/gfx/table.rs)) decompresses
   ([`codecs::gfx_rle`](../../src/codecs/gfx_rle.rs)) to BGR555 CGRAM colors.
5. **Objects** ← `$1EE8` records of 22 bytes at `$1EF8:$1EF4` (positions
   best-effort, see above).

Validation over the shipping ROM: `cargo run --bin load_level -- <rom> all`
decodes all 20 levels with **0 out-of-range metatile indices** (see
[reports/load_level.json](reports/load_level.json)).

## Status of the level format

The end-to-end chain is mapped, the rendering decode is confirmed, and it is
**wired into the editor**: **`$1EEA` (level number) → master order table
(`$80:E8D8`/`$80:E900`) → per-level setup routine → data-pointer block → tilemap
(`$D7:$D9`, `index<<5|flag` cells) → tileset (`$D3:$D5`, `$20`-byte 4×4 metatiles)
→ SNES tile words (char + palette row + flips)**, plus a 22-byte object record
list (`$1EE8` count at `$1EF4`). Confirmed live in Mesen2 (level 0 pointer block)
and statically game-wide (the `$D7` map-bank fix makes every level's indices fit).

Remaining (smaller) gaps: bit 15's exact meaning; the object record's position
packing + per-type parameter words; and rendering **real tile pixels** (the
metatile tile-words → 4bpp graphics need the scene's VRAM gfx reconstruction —
the editor currently shows metatiles as flat palette colors).

## Next steps

1. Reconstruct each scene's VRAM from its mode-0 graphics loads so the tileset's
   tile-words resolve to real 4bpp pixels ([`crate::snes::tiles`]) instead of flat
   colors — the last step to a pixel-accurate level view.
2. Pin bit 15's meaning and the object record's position/parameter words (live
   observation of a known object).
