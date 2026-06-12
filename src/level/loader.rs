//! Build the editor's [`Level`] model directly from ROM bytes.
//!
//! This is where the reverse-engineered pieces converge into something the GUI
//! can edit: the [master level table](super::index) selects a scene-setup
//! routine; [`super::scan`] recovers that routine's data-pointer block; and this
//! module follows those pointers into ROM to read the **tilemap**, the
//! per-world **tileset** (metatile definitions), the scene's **object spawn
//! list**, and — by replaying the scene's inline graphics loads through
//! [`crate::gfx::table`] + [`crate::codecs::gfx_rle`] — its **palette**.
//!
//! Confidence (mirrored into the produced [`Provenance`]):
//! - tilemap cells, map width/height, and the 4×4 metatile structure are
//!   **confirmed** (see [`super::cell`] / [`super::scan`]);
//! - the palette is reconstructed from the routine's mode-1 (CGRAM) graphics
//!   uploads — the decode path is confirmed, the *selection* of which upload is
//!   the background palette is best-effort;
//! - object record positions are **speculative** (the 22-byte record's field
//!   packing is only partly decoded).
//!
//! See `docs/reverse-engineering/level-format.md`.

use crate::codecs::gfx_rle;
use crate::error::LevelError;
use crate::gfx::table::{parse_game_table, GfxEntry, UploadTarget};
use crate::level::cell::{metatile_index, METATILE_BYTES, METATILE_WORDS};
use crate::level::index::{parse_game_index, LevelEntry};
use crate::level::scan::{scan_levels, LevelData, ENTITY_RECORD_BYTES};
use crate::model::level::{Level, Metatile, Object, Palette, Provenance, Room, Tile, TileGraphics};
use crate::snes::lorom::snes_to_pc;

/// `JSL $80:FC26` — the graphics-load wrapper. Bytes `22 26 FC 80`.
const GFX_LOAD_JSL: [u8; 4] = [0x22, 0x26, 0xFC, 0x80];

/// Number of palette colors we reconstruct (a full SNES CGRAM bank).
const PALETTE_COLORS: usize = 256;

/// SNES VRAM size in bytes (32 K 16-bit words).
const VRAM_BYTES: usize = 0x1_0000;

/// How many `$DB` per-character attribute bytes we read. A tile word's character
/// field is `& 0x3FF` (10 bits), so the table is indexed by 0..=`0x3FF`.
const ATTR_TABLE_LEN: usize = 0x400;


/// Read a 16-bit little-endian word at PC `at`.
fn word(rom: &[u8], at: usize) -> Option<u16> {
    Some(*rom.get(at)? as u16 | ((*rom.get(at + 1)? as u16) << 8))
}

/// Map a 24-bit SNES pointer to a PC offset, turning the lorom error into a
/// [`LevelError::BadPointer`] tagged with the failed address.
fn pc(addr: u32) -> Result<usize, LevelError> {
    snes_to_pc(addr).map_err(|_| LevelError::BadPointer { addr, reason: "not a LoROM ROM address" })
}

/// Match the scanned data-pointer block belonging to `entry`'s setup routine.
///
/// The routine starts at `entry.routine_ptr()`; its (single) `STA $1EF8` anchor
/// sits a little way inside it, so the block we want is the one whose anchor is
/// in the same bank and is the *first* at or after the routine start.
fn match_block<'a>(blocks: &'a [LevelData], entry: &LevelEntry) -> Option<&'a LevelData> {
    let start = snes_to_pc(entry.routine_ptr()).ok()?;
    let bank = start / crate::snes::lorom::BANK_SIZE;
    blocks
        .iter()
        .filter(|b| b.anchor_pc >= start && b.anchor_pc / crate::snes::lorom::BANK_SIZE == bank)
        .min_by_key(|b| b.anchor_pc)
}

/// Read the per-world tileset into metatile definitions (each `$20` bytes = 16
/// SNES tilemap words). The count is the tileset's capacity; reads stop early if
/// the region runs past the ROM end (a corrupt/short file) rather than failing.
fn read_metatiles(rom: &[u8], block: &LevelData) -> Result<Vec<Metatile>, LevelError> {
    let base = pc(block.tileset_ptr())?;
    let count = block.tileset_metatile_count() as usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let def = base + i * METATILE_BYTES;
        let mut tiles = Vec::with_capacity(METATILE_WORDS);
        for w in 0..METATILE_WORDS {
            match word(rom, def + w * 2) {
                Some(v) => tiles.push(v),
                None => return Ok(out), // short ROM: keep what we have
            }
        }
        out.push(Metatile { id: i as u16, tiles, palette_row: 0, collision: 0 });
    }
    Ok(out)
}

/// Read the `width*height` 16-bit tilemap and turn each cell into a [`Tile`]
/// (storing the metatile index; bit 15 / low bits are decoded by [`super::cell`]).
fn read_tiles(rom: &[u8], block: &LevelData) -> Result<Vec<Tile>, LevelError> {
    let base = pc(block.map_ptr())?;
    let cells = block.width as usize * block.height as usize;
    let mut out = Vec::with_capacity(cells);
    for i in 0..cells {
        let cell = word(rom, base + i * 2).ok_or(LevelError::Truncated { what: "tilemap" })?;
        out.push(Tile { metatile: metatile_index(cell) });
    }
    Ok(out)
}

/// Read the object spawn list (`entity_count` records of [`ENTITY_RECORD_BYTES`]).
///
/// Field decoding is partial (hence the level's speculative provenance): byte
/// `$0E` is the object type, word `$0C` the map column, words `$04`/`$06` the
/// packed Y/X. The full 22-byte record is preserved in `params` for inspection.
fn read_objects(rom: &[u8], block: &LevelData) -> Vec<Object> {
    let count = block.entity_count as usize;
    if count == 0 {
        return Vec::new();
    }
    let Ok(base) = pc(block.entity_ptr()) else { return Vec::new() };
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let off = base + i * ENTITY_RECORD_BYTES;
        let Some(rec) = rom.get(off..off + ENTITY_RECORD_BYTES) else { break };
        let kind = rec[0x0E] as u16;
        let x = word(rom, off + 0x06).unwrap_or(0) as u32;
        let y = word(rom, off + 0x04).unwrap_or(0) as u32;
        out.push(Object {
            id: i as u32,
            kind,
            x,
            y,
            params: rec.to_vec(),
            label: format!("obj #{i} (type {kind:#04X})"),
        });
    }
    out
}

/// PC offset just past the end of `entry`'s setup routine: the start of the next
/// scene routine in ROM order, the end of the routine's own LoROM bank, or the
/// ROM end — whichever is smallest. The setup routines are disjoint and each
/// fits in one bank, so this bounds a routine's body without disassembling it.
/// (The bank cap matters for the last routine in a bank, whose next-in-PC-order
/// sibling lives in a far bank and would otherwise drag the scan across
/// unrelated code.)
fn routine_end_pc(index: &[LevelEntry], entry: &LevelEntry, rom_len: usize) -> usize {
    let Ok(start) = snes_to_pc(entry.routine_ptr()) else { return rom_len };
    let bank_end = (start / crate::snes::lorom::BANK_SIZE + 1) * crate::snes::lorom::BANK_SIZE;
    let next_routine = index
        .iter()
        .filter_map(|e| snes_to_pc(e.routine_ptr()).ok())
        .filter(|&pc| pc > start)
        .min()
        .unwrap_or(rom_len);
    next_routine.min(bank_end).min(rom_len)
}

/// Collect the graphics entries a setup routine loads inline, in ROM order.
///
/// Each load is an `LDA #id : JSL $80:FC26` (`A9 id 22 26 FC 80`); `id` indexes
/// the [graphics descriptor table](crate::gfx::table). The loads are scattered
/// through the *whole* routine body — some worlds write the level-data pointer
/// block first and only then load graphics — so we scan from the routine entry
/// to `end` (the next routine, see [`routine_end_pc`]), not just up to the
/// pointer block. Entries whose source pointer is not a plausible ROM graphics
/// address are dropped (guards against stray `22 26 FC 80` bytes in data).
fn scan_gfx_loads(rom: &[u8], entry: &LevelEntry, end: usize) -> Vec<GfxEntry> {
    let Ok(table) = parse_game_table(rom) else { return Vec::new() };
    let Ok(start) = snes_to_pc(entry.routine_ptr()) else { return Vec::new() };
    let end = end.min(rom.len());
    let mut loads = Vec::new();
    let mut k = start;
    while k + GFX_LOAD_JSL.len() <= end {
        if rom[k..k + GFX_LOAD_JSL.len()] != GFX_LOAD_JSL {
            k += 1;
            continue;
        }
        // The id immediately precedes the `JSL`, loaded either 16-bit
        // (`A9 id 00`, A in 16-bit mode) or 8-bit (`A9 id`).
        let id = if k >= 3 && rom[k - 3] == 0xA9 && rom[k - 1] == 0x00 {
            Some(rom[k - 2] as usize)
        } else if k >= 2 && rom[k - 2] == 0xA9 {
            Some(rom[k - 1] as usize)
        } else {
            None
        };
        if let Some(e) = id.and_then(|id| table.get(id)).filter(|e| e.source_is_plausible()) {
            loads.push(*e);
        }
        k += GFX_LOAD_JSL.len();
    }
    loads
}

/// Reconstruct the scene's palette from its graphics loads: every mode-1 load
/// decompresses to a run of BGR555 CGRAM colors at its `$2121` address. Returns
/// a 256-color palette, or a neutral grayscale ramp if no CGRAM upload is found.
fn reconstruct_palette(rom: &[u8], loads: &[GfxEntry]) -> Palette {
    let mut colors = vec![0u16; PALETTE_COLORS];
    let mut found = false;
    for e in loads {
        if let UploadTarget::Cgram { addr, size } = e.upload() {
            if apply_cgram(rom, e.source, addr, size, &mut colors) {
                found = true;
            }
        }
    }
    if found {
        Palette { colors }
    } else {
        fallback_palette()
    }
}

/// Reconstruct the scene's VRAM from its graphics loads: every mode-0 load
/// decompresses and is placed at its true `$2116` word address (byte offset
/// `word_addr * 2`), exactly as the loader's DMA would. The result is the tile
/// pixel sheet the metatile renderer indexes by character. Returns the 64 KiB
/// VRAM buffer and whether any tile data was written.
fn reconstruct_vram(rom: &[u8], loads: &[GfxEntry]) -> (Vec<u8>, bool) {
    let mut vram = vec![0u8; VRAM_BYTES];
    let mut wrote = false;
    for e in loads {
        let UploadTarget::Vram { word_addr, size } = e.upload() else { continue };
        let Ok(src_pc) = snes_to_pc(e.source) else { continue };
        let Some(src) = rom.get(src_pc..) else { continue };
        let Ok(d) = gfx_rle::decompress(src) else { continue };
        let dst = word_addr as usize * 2;
        let n = (size as usize).min(d.data.len());
        if dst >= vram.len() {
            continue;
        }
        let n = n.min(vram.len() - dst);
        vram[dst..dst + n].copy_from_slice(&d.data[..n]);
        wrote |= n > 0;
    }
    (vram, wrote)
}

/// The background **character base** in VRAM words: where tile character 0 lives,
/// so a metatile's character `c` (`tile_word & 0x3FF`) is the tile at word
/// `char_base + c * 16`. The scene uploads its main background tile sheet in one
/// large mode-0 DMA; that DMA's `$2116` word address *is* the character base
/// (validated statically: with this base 416/417 of level 0's referenced
/// characters resolve to populated VRAM, the lone miss being the all-zero blank
/// char 0). Taken from the largest mode-0 load; `0` if the scene has none.
fn bg_char_base(loads: &[GfxEntry]) -> u16 {
    loads
        .iter()
        .filter_map(|e| match e.upload() {
            UploadTarget::Vram { word_addr, size } => Some((size, word_addr)),
            _ => None,
        })
        .max_by_key(|&(size, _)| size)
        .map(|(_, word_addr)| word_addr)
        .unwrap_or(0)
}

/// Read the `$DB` per-character attribute table (one byte per tile character).
/// Each byte is the SNES tilemap high byte the renderer ORs in: palette row in
/// bits 2..5, h/v-flip in bits 6/7. Returns up to [`ATTR_TABLE_LEN`] bytes,
/// short or empty if the pointer is unset / out of range.
fn read_attr_table(rom: &[u8], block: &LevelData) -> Vec<u8> {
    if block.attr_off == 0 {
        return Vec::new();
    }
    let Ok(base) = pc(block.attr_ptr()) else { return Vec::new() };
    let end = (base + ATTR_TABLE_LEN).min(rom.len());
    rom.get(base..end).map(|s| s.to_vec()).unwrap_or_default()
}

/// Decompress the blob at SNES `source` and write its BGR555 colors into
/// `colors` starting at CGRAM color index `addr`. Returns whether any color was
/// written.
fn apply_cgram(rom: &[u8], source: u32, addr: u8, size: u16, colors: &mut [u16]) -> bool {
    let Ok(src_pc) = snes_to_pc(source) else { return false };
    // The codec streams contiguous LoROM bytes; hand it the rest of the file.
    let Some(src) = rom.get(src_pc..) else { return false };
    let Ok(d) = gfx_rle::decompress(src) else { return false };
    let n = (size as usize / 2).min(d.data.len() / 2);
    let mut wrote = false;
    for j in 0..n {
        let idx = addr as usize + j;
        if idx >= colors.len() {
            break;
        }
        colors[idx] = d.data[j * 2] as u16 | ((d.data[j * 2 + 1] as u16) << 8);
        wrote = true;
    }
    wrote
}

/// A neutral 256-entry grayscale ramp used when no real palette is recoverable;
/// deliberately drab so it cannot be mistaken for confirmed color data.
fn fallback_palette() -> Palette {
    let colors = (0..PALETTE_COLORS as u16)
        .map(|i| {
            let v = (i & 0x1F).min(31);
            v | (v << 5) | (v << 10)
        })
        .collect();
    Palette { colors }
}

/// Build the editor [`Level`] for play-order level number `level_number`,
/// reading every field from `rom`.
pub fn load_rom_level(rom: &[u8], level_number: usize) -> Result<Level, LevelError> {
    let index = parse_game_index(rom).ok_or(LevelError::MasterTableUnreadable)?;
    let entry = index
        .get(level_number)
        .ok_or(LevelError::LevelOutOfRange { level: level_number, count: index.len() })?;

    let blocks = scan_levels(rom);
    let block = match_block(&blocks, entry)
        .ok_or(LevelError::SceneNotFound { level: level_number, routine: entry.routine_ptr() })?;

    let metatiles = read_metatiles(rom, block)?;
    let tiles = read_tiles(rom, block)?;
    let objects = read_objects(rom, block);

    let routine_end = routine_end_pc(&index, entry, rom.len());
    let loads = scan_gfx_loads(rom, entry, routine_end);
    let palette = reconstruct_palette(rom, &loads);
    let (vram, has_gfx) = reconstruct_vram(rom, &loads);
    let gfx = if has_gfx {
        TileGraphics { vram, attr: read_attr_table(rom, block), char_base: bg_char_base(&loads) }
    } else {
        TileGraphics::default()
    };

    let room = Room {
        id: 0,
        name: format!("Level {level_number}"),
        width: block.width as u32,
        height: block.height as u32,
        tiles,
        objects,
        enemy_spawns: Vec::new(),
        exits: Vec::new(),
        transitions: Vec::new(),
        checkpoints: Vec::new(),
        collision: None,
    };

    Ok(Level {
        id: level_number as u32,
        name: format!("Level {level_number}"),
        provenance: Provenance::Confirmed {
            note: format!(
                "ROM level {level_number}: {w}×{h} tilemap @ {map:#08X}, {mt} metatiles @ \
                 {ts:#08X} (confirmed). Palette/object positions are best-effort.",
                w = block.width,
                h = block.height,
                map = block.map_ptr(),
                mt = metatiles.len(),
                ts = block.tileset_ptr(),
            ),
        },
        palette,
        metatiles,
        gfx,
        rooms: vec![room],
    })
}

/// How many levels the master table reports (for UI bounds / iteration).
pub fn level_count(rom: &[u8]) -> usize {
    parse_game_index(rom).map(|i| i.len()).unwrap_or(0)
}

/// Diagnostic: the graphics-load descriptor entries a level's setup routine
/// issues inline (in routine order). Used by tooling to inspect which uploads
/// (VRAM / CGRAM / WRAM) a scene performs.
pub fn scene_gfx_loads(rom: &[u8], level_number: usize) -> Vec<GfxEntry> {
    let Some(index) = parse_game_index(rom) else { return Vec::new() };
    let Some(entry) = index.get(level_number) else { return Vec::new() };
    let end = routine_end_pc(&index, entry, rom.len());
    scan_gfx_loads(rom, entry, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx::table::{RECORD_SIZE, TABLE_PC};
    use crate::level::index::{BANK_TABLE_PC, OFFSET_TABLE_PC};
    use crate::snes::lorom::BANK_SIZE;

    fn put_word(rom: &mut [u8], at: usize, v: u16) {
        rom[at] = (v & 0xFF) as u8;
        rom[at + 1] = (v >> 8) as u8;
    }

    /// Emit `LDA #imm16 : STA <op>` (the scene routine's store pattern).
    fn lda_sta(buf: &mut Vec<u8>, imm: u16, op: &[u8]) {
        buf.push(0xA9);
        buf.push((imm & 0xFF) as u8);
        buf.push((imm >> 8) as u8);
        buf.extend_from_slice(op);
    }

    /// A synthetic but structurally faithful ROM: master table entry 0 points at
    /// a setup routine in bank `$81` that declares a `W×H` map and a tileset, both
    /// in bank `$81`. All bytes are invented. Returns the ROM and the level dims.
    ///
    /// Layout (bank $81 == PC `0x8000`):
    ///   routine body starts at PC `0x8100` (`$81:8100`)
    ///   tileset       at PC `0x8000` (`$81:8000`)
    ///   tilemap       at PC `0xC000` (`$81:C000`)
    fn synthetic_rom(extra_routine_prefix: &[u8]) -> (Vec<u8>, u16, u16) {
        let (w, h) = (4u16, 3u16);
        let mut rom = vec![0u8; TABLE_PC + ENTRY_COUNT_BYTES()];

        // --- master table entry 0 -> routine $81:8100 (offset stores +6) ---
        put_word(&mut rom, OFFSET_TABLE_PC, 0x8100 - 6);
        put_word(&mut rom, BANK_TABLE_PC, 0x0081);

        // --- tileset: 3 metatiles of 16 words each, at $81:8000 == PC 0x8000 ---
        let tileset_pc = BANK_SIZE; // bank 1
        for m in 0..3usize {
            for wi in 0..METATILE_WORDS {
                put_word(&mut rom, tileset_pc + m * METATILE_BYTES + wi * 2, (m * 16 + wi) as u16);
            }
        }
        // attr table just past the 3 metatiles so capacity == 3.
        let attr_off = 0x8000 + 3 * METATILE_BYTES as u16;

        // --- tilemap at $81:C000 == PC 0xC000: cells cycle metatile idx 0,1,2 ---
        let map_pc = BANK_SIZE + 0x4000;
        for i in 0..(w as usize * h as usize) {
            let idx = (i % 3) as u16;
            put_word(&mut rom, map_pc + i * 2, idx << 5); // cell = index<<5
        }

        // --- routine body at PC 0x8100 ---
        let mut body = Vec::new();
        body.extend_from_slice(extra_routine_prefix);
        lda_sta(&mut body, 0x0081, &[0x85, 0xD3]); // tileset bank
        lda_sta(&mut body, 0x8000, &[0x85, 0xD5]); // tileset offset
        lda_sta(&mut body, 0x0081, &[0x85, 0xD7]); // tilemap bank (== tileset here)
        lda_sta(&mut body, 0xC000, &[0x85, 0xD9]); // tilemap offset
        lda_sta(&mut body, attr_off, &[0x85, 0xDB]); // attr/capacity
        lda_sta(&mut body, w, &[0x85, 0xDD]);
        lda_sta(&mut body, h, &[0x85, 0xDF]);
        lda_sta(&mut body, 0x9000, &[0x8D, 0xF4, 0x1E]); // entity off
        lda_sta(&mut body, 0x0081, &[0x8D, 0xF8, 0x1E]); // secondary bank (anchor)
        lda_sta(&mut body, 0x8400, &[0x8D, 0xFA, 0x1E]); // handler off
        let routine_pc = BANK_SIZE + 0x100;
        rom[routine_pc..routine_pc + body.len()].copy_from_slice(&body);

        (rom, w, h)
    }

    #[allow(non_snake_case)]
    fn ENTRY_COUNT_BYTES() -> usize {
        // Room for the full graphics table plus a bit of slack, so palette
        // reconstruction can read it when present.
        ENTRY_COUNT_GFX * RECORD_SIZE + 0x100
    }
    const ENTRY_COUNT_GFX: usize = crate::gfx::table::ENTRY_COUNT;

    #[test]
    fn loads_dimensions_metatiles_and_tiles() {
        let (rom, w, h) = synthetic_rom(&[]);
        let level = load_rom_level(&rom, 0).unwrap();
        assert_eq!(level.rooms.len(), 1);
        let room = &level.rooms[0];
        assert_eq!((room.width, room.height), (w as u32, h as u32));
        assert_eq!(room.tiles.len(), (w * h) as usize);
        // Tileset capacity was 3 metatiles.
        assert_eq!(level.metatiles.len(), 3);
        // Each metatile carries the real 16 tile words.
        assert_eq!(level.metatiles[1].tiles.len(), METATILE_WORDS);
        assert_eq!(level.metatiles[1].tiles[0], 16);
        // Cells cycle 0,1,2 -> metatile indices 0,1,2,0,...
        assert_eq!(room.tiles[0].metatile, 0);
        assert_eq!(room.tiles[1].metatile, 1);
        assert_eq!(room.tiles[2].metatile, 2);
        assert_eq!(room.tiles[3].metatile, 0);
        assert!(matches!(level.provenance, Provenance::Confirmed { .. }));
    }

    #[test]
    fn unknown_level_number_errors() {
        let (rom, _, _) = synthetic_rom(&[]);
        let err = load_rom_level(&rom, 999).unwrap_err();
        assert!(matches!(err, LevelError::LevelOutOfRange { .. }));
    }

    #[test]
    fn missing_master_table_errors() {
        let rom = vec![0u8; 16];
        assert_eq!(load_rom_level(&rom, 0).unwrap_err(), LevelError::MasterTableUnreadable);
    }

    #[test]
    fn falls_back_to_grayscale_without_cgram_loads() {
        let (rom, _, _) = synthetic_rom(&[]);
        let level = load_rom_level(&rom, 0).unwrap();
        // Grayscale ramp: red == green == blue channel for every entry.
        let c = level.palette.colors[5];
        let (r, g, b) = (c & 0x1F, (c >> 5) & 0x1F, (c >> 10) & 0x1F);
        assert_eq!((r, g, b), (5, 5, 5));
    }

    #[test]
    fn reconstructs_palette_from_mode1_gfx_load() {
        // Graphics id 7 is a mode-1 (CGRAM) upload of 2 colors at CGRAM index 0.
        // Its compressed source emits two BGR555 words: $7FFF (white), $001F (red).
        let gfx_id = 7u8;
        let source_snes = 0x81_E000u32; // $81:E000 == PC 0xE000
        let source_pc = BANK_SIZE + 0x6000;

        // Routine prefix: `LDA #7 : JSL $80:FC26`.
        let prefix = [0xA9, gfx_id, 0x22, 0x26, 0xFC, 0x80];
        let (mut rom, _, _) = synthetic_rom(&prefix);

        // Graphics table record `gfx_id`: mode 1, source, CGRAM addr 0, size 4.
        let rec = TABLE_PC + gfx_id as usize * RECORD_SIZE;
        rom[rec] = 0x01; // mode 1
        rom[rec + 1] = (source_snes & 0xFF) as u8;
        rom[rec + 2] = ((source_snes >> 8) & 0xFF) as u8;
        rom[rec + 3] = ((source_snes >> 16) & 0xFF) as u8;
        rom[rec + 4] = 0x00; // CGRAM addr
        rom[rec + 6] = 0x04; // size = 4 bytes (2 colors)

        // Compressed blob: two passes (even/odd plane) of a 2-byte literal so the
        // interleave yields CGRAM bytes FF 7F 1F 00 -> colors $7FFF (idx0), $001F
        // (idx1). Pass 1 (even bytes 0,2): literal FF,1F ; pass 2 (odd bytes 1,3):
        // literal 7F,00.
        let blob = [0x01, 0xFF, 0x1F, 0x40, 0x01, 0x7F, 0x00, 0x40];
        rom[source_pc..source_pc + blob.len()].copy_from_slice(&blob);

        let level = load_rom_level(&rom, 0).unwrap();
        assert_eq!(level.palette.colors[0], 0x7FFF);
        assert_eq!(level.palette.colors[1], 0x001F);
    }

    #[test]
    fn reconstructs_vram_from_mode0_gfx_load() {
        // Graphics id 9 is a mode-0 (VRAM) upload to VRAM word $0010 (byte $20).
        let gfx_id = 9u8;
        let source_snes = 0x81_E000u32; // $81:E000 == PC 0xE000
        let source_pc = BANK_SIZE + 0x6000;

        // Routine prefix: `LDA #9 : JSL $80:FC26`.
        let prefix = [0xA9, gfx_id, 0x22, 0x26, 0xFC, 0x80];
        let (mut rom, _, _) = synthetic_rom(&prefix);

        // Table record: mode 0, source, VRAM word $0010, size 4 bytes.
        let rec = TABLE_PC + gfx_id as usize * RECORD_SIZE;
        rom[rec] = 0x00; // mode 0 (VRAM)
        rom[rec + 1] = (source_snes & 0xFF) as u8;
        rom[rec + 2] = ((source_snes >> 8) & 0xFF) as u8;
        rom[rec + 3] = ((source_snes >> 16) & 0xFF) as u8;
        rom[rec + 4] = 0x10; // $2116 word addr low
        rom[rec + 5] = 0x00; // word addr high
        rom[rec + 6] = 0x04; // size = 4 bytes
        rom[rec + 7] = 0x00;

        // Same blob as the palette test: two passes decode to bytes FF 7F 1F 00.
        let blob = [0x01, 0xFF, 0x1F, 0x40, 0x01, 0x7F, 0x00, 0x40];
        rom[source_pc..source_pc + blob.len()].copy_from_slice(&blob);

        let level = load_rom_level(&rom, 0).unwrap();
        assert!(!level.gfx.is_empty(), "mode-0 load should populate VRAM");
        // Placed at the true word address: byte offset word_addr * 2 == 0x20.
        assert_eq!(&level.gfx.vram[0x20..0x24], &[0xFF, 0x7F, 0x1F, 0x00]);
        // Bytes outside the written window stay zero.
        assert_eq!(level.gfx.vram[0x00], 0x00);
        // The $DB attribute table is read alongside.
        assert!(!level.gfx.attr.is_empty());
    }

    #[test]
    fn no_gfx_without_mode0_loads_leaves_graphics_empty() {
        // Plain scene with no graphics loads -> flat-color fallback path.
        let (rom, _, _) = synthetic_rom(&[]);
        let level = load_rom_level(&rom, 0).unwrap();
        assert!(level.gfx.is_empty());
    }

    #[test]
    fn reads_objects_when_count_is_set() {
        // Add `LDA #2 : STA $1EE8` (object count) and an entity list at $81:9000.
        let prefix = {
            let mut b = Vec::new();
            lda_sta(&mut b, 0x0002, &[0x8D, 0xE8, 0x1E]);
            b
        };
        let (mut rom, _, _) = synthetic_rom(&prefix);
        // entity_off was $9000 in bank $81 -> PC 0x9000. Two 22-byte records.
        let ent_pc = BANK_SIZE + 0x1000;
        // record 0: type byte at $0E = 0x12; X word at $06 = 0x0040; Y at $04 = 0x0020.
        put_word(&mut rom, ent_pc + 0x04, 0x0020);
        put_word(&mut rom, ent_pc + 0x06, 0x0040);
        rom[ent_pc + 0x0E] = 0x12;
        let level = load_rom_level(&rom, 0).unwrap();
        let objs = &level.rooms[0].objects;
        assert_eq!(objs.len(), 2);
        assert_eq!(objs[0].kind, 0x12);
        assert_eq!((objs[0].x, objs[0].y), (0x40, 0x20));
    }
}
