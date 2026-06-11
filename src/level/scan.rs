//! Recover the per-scene **level-data pointer block** from the scene-setup
//! routines, statically.
//!
//! ## What a scene-setup routine looks like
//!
//! Each scene/level is initialised by a dedicated routine in a high ROM bank.
//! After loading its graphics (inline `JSL $80:FC26`), the routine fills a fixed
//! block of direct-page / low-RAM variables with `LDA #imm16 : STA <var>` pairs
//! (the CPU is in 16-bit accumulator mode, `REP #$20`):
//!
//! | var | meaning | confidence |
//! |-----|---------|------------|
//! | `$D3` | primary data **bank** (low byte) | confirmed (live + 21× consistent) |
//! | `$D5` | shared per-world **tileset / metatile** offset (always `$8000`) | likely |
//! | `$D9` | **per-level tilemap** offset (`width*height` 16-bit cells) | **confirmed** |
//! | `$DB` | shared attribute / collision-map offset (`$A600` or `$C000`) | likely |
//! | `$DD` | map **width** in cells | confirmed |
//! | `$DF` | map **height** in cells | confirmed |
//! | `$1EF8` | secondary data **bank** (= the routine's own bank) | confirmed |
//! | `$1EF4` | **entity / object spawn list** offset in the secondary bank | likely |
//! | `$1EFA` | handler / pointer-table offset in the secondary bank | likely |
//!
//! The pointer *values* and the map dimensions are **confirmed**: three of these
//! routines were caught live in Mesen2 (`tools/mesen/trace_scene.lua`) with
//! exactly the values this scanner extracts, and the same block shape repeats
//! across all 21 real scene routines (an accidental match is implausible).
//!
//! The **per-level tilemap** (`$D9`) and its **`width*height` 16-bit-cell** size
//! are **confirmed by contiguous packing**: in the bank-`$88` world the four
//! levels' `$D9` offsets are exactly `prev + width*height*2` apart
//! (`$A86B → $B76B → $CB6B → $EB6B` for 80×24, 64×40, 64×64), which only closes
//! if each map is `width*height` two-byte cells stored back-to-back. The shared
//! `$D5`/`$DB` regions (identical for every level in a bank) are therefore the
//! per-**world** tileset / attribute data, not per-level — labelled **likely**
//! pending the consumer disassembly.
//!
//! ## How the scanner works
//!
//! `STA $1EF8` (`8D F8 1E`) is the distinctive anchor every scene routine
//! contains. For each occurrence we scan a small window for the other stores and
//! read the immediate that precedes each (`A9 lo hi` directly before the `STA`).
//! Routines that do not yield a full, plausible block are dropped (this rejects
//! the lone non-scene `STA $1EF8` site).
//!
//! See `docs/reverse-engineering/level-format.md`.

use crate::snes::lorom::pc_to_snes;

/// `STA $1EF8` — `8D F8 1E`. The anchor present in every scene-setup routine.
const ANCHOR: [u8; 3] = [0x8D, 0xF8, 0x1E];
/// How far either side of the anchor to look for the other stores.
const WINDOW: usize = 0xC0;

/// One scene's recovered level-data pointer block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelData {
    /// PC file offset of the anchor (`STA $1EF8`) inside the setup routine.
    pub anchor_pc: usize,
    /// SNES address of the anchor, in the game's `$80+` (FastROM) banks.
    pub anchor_snes: u32,
    /// Primary data bank (`$D3` low byte).
    pub primary_bank: u8,
    /// Shared per-world tileset/metatile offset in the primary bank (`$D5`,
    /// always `$8000`).
    pub tileset_off: u16,
    /// **Per-level tilemap** offset in the primary bank (`$D9`): `width*height`
    /// 16-bit cells, packed contiguously across the world's levels.
    pub map_off: u16,
    /// Shared attribute / collision-map offset (`$DB`).
    pub attr_off: u16,
    /// Map width in cells (`$DD`).
    pub width: u16,
    /// Map height in cells (`$DF`).
    pub height: u16,
    /// Secondary data bank (`$1EF8` low byte) — holds the entity/handler data.
    pub secondary_bank: u8,
    /// Entity / object spawn-list offset in the secondary bank (`$1EF4`).
    pub entity_off: u16,
    /// Handler / pointer-table offset in the secondary bank (`$1EFA`).
    pub handler_off: u16,
}

impl LevelData {
    /// 24-bit SNES pointer to the per-level tilemap (`primary_bank:map_off`).
    pub fn map_ptr(&self) -> u32 {
        ((self.primary_bank as u32) << 16) | self.map_off as u32
    }
    /// 24-bit SNES pointer to the shared per-world tileset (`:tileset_off`).
    pub fn tileset_ptr(&self) -> u32 {
        ((self.primary_bank as u32) << 16) | self.tileset_off as u32
    }
    /// 24-bit SNES pointer to the attribute/collision map.
    pub fn attr_ptr(&self) -> u32 {
        ((self.primary_bank as u32) << 16) | self.attr_off as u32
    }
    /// 24-bit SNES pointer to the entity/object spawn list.
    pub fn entity_ptr(&self) -> u32 {
        ((self.secondary_bank as u32) << 16) | self.entity_off as u32
    }
    /// Tilemap size in bytes: `width * height` two-byte cells (confirmed by the
    /// contiguous packing of the bank-`$88` world's maps).
    pub fn map_bytes(&self) -> u32 {
        self.width as u32 * self.height as u32 * 2
    }

    /// Number of metatile definitions the world's tileset can hold: the bytes
    /// between the tileset (`$D5`) and the attribute table (`$DB`), divided by
    /// the `$20`-byte metatile stride. Every map index seen fits under this.
    pub fn tileset_metatile_count(&self) -> u32 {
        self.attr_off
            .saturating_sub(self.tileset_off)
            .wrapping_div(crate::level::cell::METATILE_BYTES as u16) as u32
    }
}

/// The 16-bit operand of an `LDA #imm16` (`A9 lo hi`) sitting immediately before
/// the `STA` at `sta_pc`, if present.
fn imm16_before(rom: &[u8], sta_pc: usize) -> Option<u16> {
    let a9 = sta_pc.checked_sub(3)?;
    if rom[a9] != 0xA9 {
        return None;
    }
    Some(rom[a9 + 1] as u16 | ((rom[a9 + 2] as u16) << 8))
}

/// Find, nearest to `center`, a `STA` whose opcode bytes are `op` and which is
/// preceded by an `LDA #imm16`; return that immediate. Searches outward so the
/// closest matching store to the anchor wins.
fn nearest_imm(rom: &[u8], center: usize, op: &[u8]) -> Option<u16> {
    let lo = center.saturating_sub(WINDOW);
    let hi = (center + WINDOW).min(rom.len().saturating_sub(op.len()));
    let mut best: Option<(usize, u16)> = None;
    for k in lo..=hi {
        if &rom[k..k + op.len()] == op {
            if let Some(v) = imm16_before(rom, k) {
                let dist = k.abs_diff(center);
                if best.map_or(true, |(bd, _)| dist < bd) {
                    best = Some((dist, v));
                }
            }
        }
    }
    best.map(|(_, v)| v)
}

/// Extract the level-data block anchored at `anchor_pc` (a `STA $1EF8` site).
/// Returns `None` if the routine does not carry a full, plausible block.
fn extract(rom: &[u8], anchor_pc: usize) -> Option<LevelData> {
    let sta = |dp: u8| [0x85u8, dp];
    let sta_abs = |lo: u8, hi: u8| [0x8Du8, lo, hi];

    let d3 = nearest_imm(rom, anchor_pc, &sta(0xD3))?;
    let d5 = nearest_imm(rom, anchor_pc, &sta(0xD5))?;
    let dd = nearest_imm(rom, anchor_pc, &sta(0xDD))?;
    let df = nearest_imm(rom, anchor_pc, &sta(0xDF))?;
    let ef8 = nearest_imm(rom, anchor_pc, &sta_abs(0xF8, 0x1E))?;
    let ef4 = nearest_imm(rom, anchor_pc, &sta_abs(0xF4, 0x1E))?;
    // Optional fields — present in every real scene but not required to qualify.
    let d9 = nearest_imm(rom, anchor_pc, &sta(0xD9)).unwrap_or(0);
    let db = nearest_imm(rom, anchor_pc, &sta(0xDB)).unwrap_or(0);
    let efa = nearest_imm(rom, anchor_pc, &sta_abs(0xFA, 0x1E)).unwrap_or(0);

    // Plausibility: a real scene's primary layout pointer is always $8000, the
    // banks are real ROM banks ($80..=$9F on this 1 MiB cart) and the map has
    // non-zero dimensions. This rejects the lone non-scene STA $1EF8 site.
    let primary_bank = (d3 & 0xFF) as u8;
    let secondary_bank = (ef8 & 0xFF) as u8;
    let bank_ok = |b: u8| (0x80..=0x9F).contains(&b);
    if d5 != 0x8000 || !bank_ok(primary_bank) || !bank_ok(secondary_bank) || dd == 0 || df == 0 {
        return None;
    }

    Some(LevelData {
        anchor_pc,
        anchor_snes: pc_to_snes(anchor_pc).map(|s| s | 0x80_0000).unwrap_or(0),
        primary_bank,
        tileset_off: d5,
        map_off: d9,
        attr_off: db,
        width: dd,
        height: df,
        secondary_bank,
        entity_off: ef4,
        handler_off: efa,
    })
}

/// Scan the whole ROM for scene-setup routines and return each one's recovered
/// level-data block, in ROM order.
pub fn scan_levels(rom: &[u8]) -> Vec<LevelData> {
    let mut out = Vec::new();
    if rom.len() < ANCHOR.len() {
        return out;
    }
    for i in 0..=rom.len() - ANCHOR.len() {
        if rom[i..i + ANCHOR.len()] == ANCHOR {
            if let Some(ld) = extract(rom, i) {
                out.push(ld);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Emit `LDA #imm16 : STA <op...>` into `buf`.
    fn lda_sta(buf: &mut Vec<u8>, imm: u16, op: &[u8]) {
        buf.push(0xA9);
        buf.push((imm & 0xFF) as u8);
        buf.push((imm >> 8) as u8);
        buf.extend_from_slice(op);
    }

    /// Build a synthetic scene-setup routine body with the given block values.
    /// (Invented bytes — no ROM data.)
    fn synth_scene(d3: u16, d5: u16, d9: u16, db: u16, dd: u16, df: u16, ef8: u16, ef4: u16, efa: u16) -> Vec<u8> {
        let mut b = Vec::new();
        lda_sta(&mut b, d3, &[0x85, 0xD3]);
        lda_sta(&mut b, d5, &[0x85, 0xD5]);
        lda_sta(&mut b, d9, &[0x85, 0xD9]);
        lda_sta(&mut b, db, &[0x85, 0xDB]);
        lda_sta(&mut b, dd, &[0x85, 0xDD]);
        lda_sta(&mut b, df, &[0x85, 0xDF]);
        lda_sta(&mut b, ef4, &[0x8D, 0xF4, 0x1E]);
        lda_sta(&mut b, ef8, &[0x8D, 0xF8, 0x1E]); // anchor
        lda_sta(&mut b, efa, &[0x8D, 0xFA, 0x1E]);
        b
    }

    fn rom_with(body: &[u8], at: usize) -> Vec<u8> {
        let mut rom = vec![0u8; at + body.len() + WINDOW + 4];
        rom[at..at + body.len()].copy_from_slice(body);
        rom
    }

    #[test]
    fn extracts_a_full_block() {
        // Mirrors scene #3: primary $88, tilemap $8000, obj $A86B, attr $A600,
        // 80x24, secondary $81, entity $8DAE, handler $83B0.
        let body = synth_scene(0x0088, 0x8000, 0xA86B, 0xA600, 0x0050, 0x0018, 0x0081, 0x8DAE, 0x83B0);
        let rom = rom_with(&body, 0x100);
        let levels = scan_levels(&rom);
        assert_eq!(levels.len(), 1);
        let l = levels[0];
        assert_eq!(l.primary_bank, 0x88);
        assert_eq!(l.tileset_off, 0x8000);
        assert_eq!(l.map_off, 0xA86B);
        assert_eq!(l.attr_off, 0xA600);
        assert_eq!(l.width, 0x50);
        assert_eq!(l.height, 0x18);
        assert_eq!(l.secondary_bank, 0x81);
        assert_eq!(l.entity_off, 0x8DAE);
        assert_eq!(l.handler_off, 0x83B0);
        assert_eq!(l.map_ptr(), 0x88_A86B);
        assert_eq!(l.tileset_ptr(), 0x88_8000);
        assert_eq!(l.attr_ptr(), 0x88_A600);
        assert_eq!(l.entity_ptr(), 0x81_8DAE);
        assert_eq!(l.map_bytes(), 80 * 24 * 2);
    }

    #[test]
    fn rejects_anchor_without_a_plausible_block() {
        // A bare STA $1EF8 with no surrounding stores (the false-positive case).
        let mut rom = vec![0u8; 0x400];
        rom[0x200..0x203].copy_from_slice(&[0x8D, 0xF8, 0x1E]);
        assert!(scan_levels(&rom).is_empty());
    }

    #[test]
    fn rejects_block_with_wrong_tilemap_offset() {
        // d5 != $8000 disqualifies (not a scene layout pointer).
        let body = synth_scene(0x0088, 0x9000, 0xA86B, 0xA600, 0x0050, 0x0018, 0x0081, 0x8DAE, 0x83B0);
        let rom = rom_with(&body, 0x100);
        assert!(scan_levels(&rom).is_empty());
    }

    #[test]
    fn rejects_block_with_zero_dimensions() {
        let body = synth_scene(0x0088, 0x8000, 0xA86B, 0xA600, 0x0000, 0x0018, 0x0081, 0x8DAE, 0x83B0);
        let rom = rom_with(&body, 0x100);
        assert!(scan_levels(&rom).is_empty());
    }

    #[test]
    fn finds_multiple_scenes() {
        let a = synth_scene(0x0088, 0x8000, 0xA86B, 0xA600, 0x0050, 0x0018, 0x0081, 0x8DAE, 0x83B0);
        let b = synth_scene(0x008B, 0x8000, 0xC17E, 0xC000, 0x0018, 0x005A, 0x008C, 0x86B2, 0x837A);
        let mut rom = vec![0u8; 0x800];
        rom[0x100..0x100 + a.len()].copy_from_slice(&a);
        rom[0x400..0x400 + b.len()].copy_from_slice(&b);
        let levels = scan_levels(&rom);
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0].primary_bank, 0x88);
        assert_eq!(levels[1].primary_bank, 0x8B);
        assert_eq!(levels[1].height, 0x5A);
    }

    #[test]
    fn map_bytes_two_per_cell() {
        // The real bank-$88 world packs maps `prev + width*height*2` apart;
        // map_bytes() is the size that arithmetic relies on.
        let body = synth_scene(0x0088, 0x8000, 0xA86B, 0xA600, 80, 24, 0x0081, 0x8DAE, 0x83B0);
        let rom = rom_with(&body, 0x100);
        let l = scan_levels(&rom)[0];
        assert_eq!(l.map_bytes(), 3840);
        assert_eq!(l.map_off as u32 + l.map_bytes(), 0xB76B); // = next level's $D9
    }
}
