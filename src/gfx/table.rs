//! The graphics descriptor table — the **graphics-id → compressed-source**
//! index that the game's tile loader walks before calling the decompressor.
//!
//! ## What it is
//!
//! A flat array of fixed 8-byte records at SNES `$82:8000` (PC `0x10000`), the
//! very start of bank `$82`, running up to the loader code that immediately
//! follows it at `$82:84F8`. There are [`ENTRY_COUNT`] records.
//!
//! Record layout (little-endian):
//!
//! | bytes | field    | meaning |
//! |-------|----------|---------|
//! | `0`   | `mode`   | upload/handling mode (`0`, `1`, `2` observed) |
//! | `1..4`| `source` | 24-bit SNES pointer to the compressed blob |
//! | `4..8`| `params` | mode-dependent: for `mode == 2` the low 3 bytes are a 24-bit WRAM destination; for `mode` `0`/`1` an upload target/size pair |
//!
//! ## Confidence
//!
//! - The **24-bit source pointer** (bytes `1..4`) is **confirmed**: the loader
//!   trace (`tools/mesen/trace_gfx_loader.lua`) caught 36 distinct live
//!   decompress calls and every one's source pointer matched
//!   `source(index = Y/8)` here exactly, where `Y` is the loader's table index
//!   register. The decompressor then reproduces real ROM graphics from that
//!   pointer (see [`crate::codecs::gfx_rle`]).
//! - The **mode-2 WRAM destination** (params low 3 bytes) is **confirmed by the
//!   same trace**: every mode-2 sample's live destination pointer matched.
//! - The **mode byte's full meaning** and the **mode-0/1 params** are **likely**
//!   (a VRAM target word + size for the upload stage) — not yet round-tripped.
//!
//! See `docs/reverse-engineering/graphics-table.md`.

use crate::error::RomError;
use crate::snes::lorom::snes_to_pc;

/// SNES address of the first record (`$82:8000`).
pub const TABLE_SNES: u32 = 0x82_8000;
/// PC file offset of the first record (headerless ROM).
pub const TABLE_PC: usize = 0x1_0000;
/// Bytes per record.
pub const RECORD_SIZE: usize = 8;
/// Number of records before the loader code at `$82:84F8`.
///
/// `(0x84F8 - 0x8000) / 8 == 159`; record 158 ends exactly where the loader's
/// `PHP / REP #$30 / PHX / PHY` preamble begins.
pub const ENTRY_COUNT: usize = 159;

/// One parsed descriptor-table record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GfxEntry {
    /// Record index (the graphics id; the loader passes `id * 8` in `Y`).
    pub index: usize,
    /// Mode byte (record byte 0). Selects how the loader uploads the blob.
    pub mode: u8,
    /// 24-bit SNES pointer to the compressed blob (record bytes 1..4).
    pub source: u32,
    /// Raw mode-dependent trailing bytes (record bytes 4..8).
    pub params: [u8; 4],
}

impl GfxEntry {
    /// Parse one record from an 8-byte slice.
    fn from_record(index: usize, rec: &[u8]) -> Self {
        debug_assert!(rec.len() >= RECORD_SIZE);
        let source = (rec[1] as u32) | ((rec[2] as u32) << 8) | ((rec[3] as u32) << 16);
        GfxEntry {
            index,
            mode: rec[0],
            source,
            params: [rec[4], rec[5], rec[6], rec[7]],
        }
    }

    /// PC file offset of the compressed source blob (LoROM mapped). `Err` if the
    /// source pointer is not a valid LoROM ROM address.
    pub fn source_pc(&self) -> Result<usize, RomError> {
        snes_to_pc(self.source)
    }

    /// The compressed source's bank byte.
    pub fn source_bank(&self) -> u8 {
        (self.source >> 16) as u8
    }

    /// For `mode == 2`, the explicit 24-bit WRAM destination encoded in the low
    /// three `params` bytes (confirmed by the loader trace). `None` otherwise —
    /// modes 0/1 decompress into the fixed `$7F:C000` staging area and use
    /// `params` for upload targeting instead.
    pub fn dest_wram(&self) -> Option<u32> {
        if self.mode == 2 {
            Some(
                (self.params[0] as u32)
                    | ((self.params[1] as u32) << 8)
                    | ((self.params[2] as u32) << 16),
            )
        } else {
            None
        }
    }

    /// Whether `source` looks like a real compressed-graphics pointer: a ROM
    /// bank (`$80..=$9F` here — the cartridge is 1 MiB) addressing the upper
    /// (`>= $8000`) LoROM half. The loader trace only ever read banks
    /// `$92..=$9F`; anything outside that is worth flagging.
    pub fn source_is_plausible(&self) -> bool {
        let bank = self.source_bank();
        let offset = self.source & 0xFFFF;
        (0x80..=0x9F).contains(&bank) && offset >= 0x8000
    }
}

/// Parse `count` consecutive descriptor records starting at PC offset `base`.
///
/// Generic over `base`/`count` so tests can use a synthetic fixture; callers
/// that want the real table pass [`TABLE_PC`] and [`ENTRY_COUNT`].
pub fn parse_table(rom: &[u8], base: usize, count: usize) -> Result<Vec<GfxEntry>, RomError> {
    let len = count * RECORD_SIZE;
    let end = base.checked_add(len).ok_or(RomError::OutOfRange {
        offset: base,
        len,
        size: rom.len(),
    })?;
    if end > rom.len() {
        return Err(RomError::OutOfRange {
            offset: base,
            len,
            size: rom.len(),
        });
    }
    Ok((0..count)
        .map(|i| {
            let off = base + i * RECORD_SIZE;
            GfxEntry::from_record(i, &rom[off..off + RECORD_SIZE])
        })
        .collect())
}

/// Parse the real game's table ([`ENTRY_COUNT`] records at [`TABLE_PC`]).
pub fn parse_game_table(rom: &[u8]) -> Result<Vec<GfxEntry>, RomError> {
    parse_table(rom, TABLE_PC, ENTRY_COUNT)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic ROM image whose bank `$82` starts with the given
    /// records (no copyrighted bytes — these are invented).
    fn rom_with_records(records: &[[u8; 8]]) -> Vec<u8> {
        let mut rom = vec![0u8; TABLE_PC + records.len() * RECORD_SIZE + 16];
        for (i, r) in records.iter().enumerate() {
            let off = TABLE_PC + i * RECORD_SIZE;
            rom[off..off + RECORD_SIZE].copy_from_slice(r);
        }
        rom
    }

    #[test]
    fn parses_mode_source_and_params() {
        // mode 0, source $93:B9C9, params 11 22 33 44
        let rom = rom_with_records(&[[0x00, 0xC9, 0xB9, 0x93, 0x11, 0x22, 0x33, 0x44]]);
        let t = parse_table(&rom, TABLE_PC, 1).unwrap();
        assert_eq!(t.len(), 1);
        let e = t[0];
        assert_eq!(e.index, 0);
        assert_eq!(e.mode, 0x00);
        assert_eq!(e.source, 0x93_B9C9);
        assert_eq!(e.params, [0x11, 0x22, 0x33, 0x44]);
        assert_eq!(e.source_bank(), 0x93);
    }

    #[test]
    fn source_pc_maps_through_lorom() {
        // $93:B9C9 -> PC. $93 mirrors $13: bank 0x13 * 0x8000 + 0x39C9.
        let rom = rom_with_records(&[[0x00, 0xC9, 0xB9, 0x93, 0, 0, 0, 0]]);
        let e = parse_table(&rom, TABLE_PC, 1).unwrap()[0];
        assert_eq!(e.source_pc().unwrap(), 0x13 * 0x8000 + 0x39C9);
    }

    #[test]
    fn mode2_exposes_explicit_wram_dest() {
        // mode 2, dest $7F:A000 in params low 3 bytes (00 A0 7F).
        let rom = rom_with_records(&[[0x02, 0x67, 0x91, 0x99, 0x00, 0xA0, 0x7F, 0x00]]);
        let e = parse_table(&rom, TABLE_PC, 1).unwrap()[0];
        assert_eq!(e.dest_wram(), Some(0x7F_A000));
    }

    #[test]
    fn non_mode2_has_no_explicit_dest() {
        let rom = rom_with_records(&[[0x00, 0x67, 0x91, 0x99, 0x00, 0xA0, 0x7F, 0x00]]);
        let e = parse_table(&rom, TABLE_PC, 1).unwrap()[0];
        assert_eq!(e.dest_wram(), None);
    }

    #[test]
    fn parses_multiple_indexed_records() {
        let rom = rom_with_records(&[
            [0x00, 0xD7, 0xAE, 0x92, 0, 0, 0, 0x10],
            [0x01, 0x00, 0x80, 0x92, 0x01, 0x00, 0x1E, 0x00],
            [0x02, 0xC9, 0xB9, 0x93, 0, 0x20, 0, 0x40],
        ]);
        let t = parse_table(&rom, TABLE_PC, 3).unwrap();
        assert_eq!(t[0].source, 0x92_AED7);
        assert_eq!(t[1].source, 0x92_8000);
        assert_eq!(t[2].source, 0x93_B9C9);
        assert_eq!(t[2].index, 2);
    }

    #[test]
    fn plausibility_flags_bad_banks() {
        let rom = rom_with_records(&[
            [0x00, 0x00, 0x80, 0x92, 0, 0, 0, 0], // $92:8000 plausible
            [0x00, 0x00, 0x00, 0x7F, 0, 0, 0, 0], // $7F WRAM, implausible
            [0x00, 0x00, 0x00, 0x92, 0, 0, 0, 0], // $92:0000 system area, implausible
        ]);
        let t = parse_table(&rom, TABLE_PC, 3).unwrap();
        assert!(t[0].source_is_plausible());
        assert!(!t[1].source_is_plausible());
        assert!(!t[2].source_is_plausible());
    }

    #[test]
    fn out_of_range_errors_instead_of_panicking() {
        let rom = vec![0u8; TABLE_PC + 4]; // not even one full record
        assert!(matches!(
            parse_table(&rom, TABLE_PC, 1),
            Err(RomError::OutOfRange { .. })
        ));
    }
}
