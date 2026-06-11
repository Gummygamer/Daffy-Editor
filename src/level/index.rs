//! The **master level table** — `level number → per-level setup routine`.
//!
//! The engine keeps the current level number in `$1EEA`. To start a level it
//! doubles it and indexes two **parallel word tables** in bank `$80`
//! (`$80:E8A9`: `LDA $1EEA / ASL A / TAY / LDA $E8D8,Y → $1EF6 / LDA $E900,Y →
//! $1EF8`), then far-calls `$1EF8:($1EF6 + 6)` — the level's setup routine (the
//! same routines [`super::scan`] finds, minus their short header).
//!
//! - [`OFFSET_TABLE_PC`] (`$80:E8D8`) holds each level's 16-bit routine offset.
//! - [`BANK_TABLE_PC`] (`$80:E900`) holds each level's routine bank.
//!
//! The two tables are **adjacent and exactly [`LEVEL_COUNT`] entries**:
//! `$E900 - $E8D8 = 0x28 = 20 words`, and the bank table ends (`$E928`) right
//! where code resumes. So the game has **20 ordered levels**.
//!
//! Confidence: **likely**. The table location/stride is read straight from the
//! indexing code; the entry count is fixed by the adjacent-table arithmetic; and
//! the 20 banks match the per-level setup-routine banks recovered independently
//! by [`super::scan`] (multiset-identical except the one non-level `$82` screen).
//! See `docs/reverse-engineering/level-format.md`.

/// PC offset of the per-level routine **offset** table (`$80:E8D8`).
pub const OFFSET_TABLE_PC: usize = 0x6_8D8;
/// PC offset of the per-level routine **bank** table (`$80:E900`).
pub const BANK_TABLE_PC: usize = 0x6_900;
/// Number of ordered levels (the offset table is exactly this many words).
pub const LEVEL_COUNT: usize = 20;
/// The far-call header skipped before the routine body (`ADC #$0006`).
pub const ROUTINE_HEADER: u16 = 6;

/// One master-table entry: a level's setup-routine pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelEntry {
    /// Level number (`$1EEA`), i.e. play order.
    pub level: usize,
    /// Routine bank (`$80:E900[level]`).
    pub bank: u8,
    /// Routine offset as stored (`$80:E8D8[level]`).
    pub offset: u16,
}

impl LevelEntry {
    /// 24-bit SNES address actually called: `bank:(offset + ROUTINE_HEADER)`.
    pub fn routine_ptr(&self) -> u32 {
        ((self.bank as u32) << 16) | self.offset.wrapping_add(ROUTINE_HEADER) as u32
    }
}

/// Read a 16-bit little-endian word at PC `at`, or `None` if out of range.
fn word(rom: &[u8], at: usize) -> Option<u16> {
    Some(*rom.get(at)? as u16 | ((*rom.get(at + 1)? as u16) << 8))
}

/// Parse `count` entries of the master level table from the two parallel word
/// tables at `off_base` / `bank_base`. Generic over the bases/count so tests can
/// use a synthetic fixture; callers pass the real constants via [`parse_game_index`].
pub fn parse_index(
    rom: &[u8],
    off_base: usize,
    bank_base: usize,
    count: usize,
) -> Option<Vec<LevelEntry>> {
    let mut out = Vec::with_capacity(count);
    for level in 0..count {
        let offset = word(rom, off_base + level * 2)?;
        let bank = (word(rom, bank_base + level * 2)? & 0xFF) as u8;
        out.push(LevelEntry { level, bank, offset });
    }
    Some(out)
}

/// Parse the real game's [`LEVEL_COUNT`]-entry master table.
pub fn parse_game_index(rom: &[u8]) -> Option<Vec<LevelEntry>> {
    parse_index(rom, OFFSET_TABLE_PC, BANK_TABLE_PC, LEVEL_COUNT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_word(rom: &mut [u8], at: usize, v: u16) {
        rom[at] = (v & 0xFF) as u8;
        rom[at + 1] = (v >> 8) as u8;
    }

    #[test]
    fn parses_parallel_tables_into_pointers() {
        // Two adjacent 2-entry tables (synthetic, invented values).
        let mut rom = vec![0u8; 0x40];
        // offsets at 0x00, banks at 0x10
        put_word(&mut rom, 0x00, 0x7FFF);
        put_word(&mut rom, 0x02, 0xA6EF);
        put_word(&mut rom, 0x10, 0x0081);
        put_word(&mut rom, 0x12, 0x0081);
        let idx = parse_index(&rom, 0x00, 0x10, 2).unwrap();
        assert_eq!(idx.len(), 2);
        assert_eq!(idx[0].bank, 0x81);
        assert_eq!(idx[0].offset, 0x7FFF);
        // routine_ptr skips the 6-byte header: $81:(7FFF+6) = $81:8005.
        assert_eq!(idx[0].routine_ptr(), 0x81_8005);
        assert_eq!(idx[1].routine_ptr(), 0x81_A6F5);
    }

    #[test]
    fn out_of_range_returns_none() {
        let rom = vec![0u8; 4];
        assert!(parse_index(&rom, 0x00, 0x10, 2).is_none());
    }

    #[test]
    fn table_constants_are_adjacent_and_sized() {
        // The defining structural fact: offset table is exactly LEVEL_COUNT words
        // and the bank table starts right after it.
        assert_eq!(BANK_TABLE_PC - OFFSET_TABLE_PC, LEVEL_COUNT * 2);
    }
}
