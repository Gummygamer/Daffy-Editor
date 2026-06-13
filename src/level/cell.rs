//! Decoding of a **level map cell** and the metatile model it references.
//!
//! A level map (the `$D9` pointer in [`super::scan`]) is `width * height`
//! 16-bit cells, row-major, uncompressed. Each cell is a reference into the
//! per-world **tileset** (`$D5`), which is a flat array of fixed `$20`-byte
//! **metatile** definitions (16 SNES tilemap words = a 4×4 block of 8×8 tiles,
//! i.e. a 32×32-pixel metatile).
//!
//! ## Cell bit layout
//!
//! ```text
//!  15        5 4    0
//! +-+----------+-----+
//! |F|  index   |  0  |   value = (index << 5) | (F << 15)
//! +-+----------+-----+
//! ```
//!
//! - **bits 0..5** are always zero — the cell value is exactly `index * $20`,
//!   the byte offset of the metatile inside the tileset. (Confirmed: across the
//!   `$88`/`$8B`/`$83` worlds *every* one of thousands of cells has its low five
//!   bits clear.)
//! - **bits 5..15** are the metatile **index**. The largest index used by each
//!   world's maps fits inside that world's tileset capacity
//!   (`$88`: max 298 < 304; `$83`: max 378 < 512) — independent corroboration.
//! - **bit 15** is a per-cell **flag**. The metatile renderer at `$80:F5B9`
//!   explicitly `AND #$7FFF`s it away before using the cell as a tileset offset,
//!   so bit 15 does **not** affect tile selection — it is a separate flag
//!   (collision/priority — *likely*). `$8000` (index 0, flag set) is the dominant
//!   "empty/sky" cell.
//!
//! ## Metatile layout — confirmed
//!
//! The renderer (`$80:F5A8`–`$80:F5F7`) computes the metatile definition address
//! as `tileset($D5) + (cell & 0x7FFF)`, then indexes a **4×4 grid of 16-bit tile
//! words**: word offset `= (subrow & 3) * 8 + (subcol & 3) * 2` (see
//! [`metatile_word_offset`]). So a metatile is a **4×4 block of 8×8 tiles
//! (32×32 px)**, [`METATILE_WORDS`] words, row stride 8 bytes — **confirmed**.
//! Each tile word is a standard SNES tilemap entry (char index = `word & 0x3FF`),
//! and that char index keys the per-tile attribute byte in the `$DB` table.
//!
//! Confidence: the stride/index decode **and** the 4×4 metatile shape are now
//! **confirmed** from the renderer; only bit 15's exact meaning is *likely*. See
//! `docs/reverse-engineering/level-format.md`.

/// Bytes per metatile definition in the tileset (16 tilemap words).
pub const METATILE_BYTES: usize = 0x20;
/// SNES tilemap words per metatile (a 4×4 block of 8×8 tiles).
pub const METATILE_WORDS: usize = METATILE_BYTES / 2;
/// A metatile is a `DIM × DIM` block of 8×8 tiles (confirmed 4×4 by `$80:F5A8`).
pub const METATILE_DIM: usize = 4;

/// Byte offset of a metatile's `(subcol, subrow)` tile word within its `$20`-byte
/// definition: `(subrow & 3) * 8 + (subcol & 3) * 2` — the exact arithmetic the
/// renderer at `$80:F5C2`/`$80:F5CF` performs (row stride 8 bytes, col stride 2).
pub fn metatile_word_offset(subcol: usize, subrow: usize) -> usize {
    (subrow & 3) * (METATILE_DIM * 2) + (subcol & 3) * 2
}

/// The metatile index a cell selects (`(cell & 0x7FFF) >> 5`).
pub fn metatile_index(cell: u16) -> u16 {
    (cell & 0x7FFF) >> 5
}

/// Byte offset of the cell's metatile within the tileset (`metatile_index * $20`).
pub fn tileset_offset(cell: u16) -> usize {
    metatile_index(cell) as usize * METATILE_BYTES
}

/// The cell bits occupied by the metatile index (bits 5..=14, since
/// `metatile_index = (cell & 0x7FFF) >> 5`).
const INDEX_MASK: u16 = 0x7FE0;

/// Re-encode a tilemap cell to select `metatile`, preserving every bit the
/// index does not own (bit 15 and the low five bits). Inverse of
/// [`metatile_index`]: `metatile_index(encode_cell(c, metatile)) == metatile`
/// for any `metatile < 1024`, and `encode_cell(c, metatile_index(c)) == c`, so
/// unedited cells re-encode to themselves (no spurious diff on export).
pub fn encode_cell(original: u16, metatile: u16) -> u16 {
    (original & !INDEX_MASK) | ((metatile << 5) & INDEX_MASK)
}

/// The per-cell flag (bit 15).
pub fn cell_flag(cell: u16) -> bool {
    cell & 0x8000 != 0
}

/// Whether a cell's low five bits are clear, i.e. it is a well-formed
/// `index * $20` reference. Used to sanity-check that a region really is a map.
pub fn is_aligned(cell: u16) -> bool {
    cell & 0x1F == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_index_and_flag() {
        // $8000: index 0, flag set (the dominant empty cell).
        assert_eq!(metatile_index(0x8000), 0);
        assert!(cell_flag(0x8000));
        assert_eq!(tileset_offset(0x8000), 0);

        // $0040: index 2, no flag.
        assert_eq!(metatile_index(0x0040), 2);
        assert!(!cell_flag(0x0040));
        assert_eq!(tileset_offset(0x0040), 0x40);

        // $8D20: flag set, masked $0D20 -> index 0x69.
        assert_eq!(metatile_index(0x8D20), 0x69);
        assert!(cell_flag(0x8D20));
        assert_eq!(tileset_offset(0x8D20), 0x69 * 0x20);
    }

    #[test]
    fn aligned_detects_low_bits() {
        assert!(is_aligned(0x8000));
        assert!(is_aligned(0x0040));
        assert!(!is_aligned(0x0041));
        assert!(!is_aligned(0x001F));
    }

    #[test]
    fn metatile_word_offsets_cover_the_4x4_grid() {
        // The 16 (subcol,subrow) positions map 1:1 onto the 16 words ($00..$1E).
        let mut seen = std::collections::BTreeSet::new();
        for subrow in 0..METATILE_DIM {
            for subcol in 0..METATILE_DIM {
                seen.insert(metatile_word_offset(subcol, subrow));
            }
        }
        assert_eq!(seen.len(), METATILE_WORDS);
        assert_eq!(*seen.iter().min().unwrap(), 0);
        assert_eq!(*seen.iter().max().unwrap(), METATILE_BYTES - 2);
        // row stride 8, col stride 2 (renderer's ASL pattern).
        assert_eq!(metatile_word_offset(1, 0), 2);
        assert_eq!(metatile_word_offset(0, 1), 8);
        assert_eq!(metatile_word_offset(3, 3), 30);
    }

    #[test]
    fn index_offset_round_trips_within_a_metatile_grid() {
        for idx in [0u16, 1, 2, 0x69, 0x12A] {
            let cell = idx << 5;
            assert_eq!(metatile_index(cell), idx);
            assert_eq!(tileset_offset(cell), idx as usize * METATILE_BYTES);
        }
    }

    #[test]
    fn encode_cell_round_trips_and_preserves_other_bits() {
        // Re-encoding a cell to its own index reproduces it exactly (no diff
        // for unedited cells on export), even with flag/low bits set.
        for cell in [0x0000u16, 0x8000, 0x0040, 0x8D20, 0x8D21, 0xFFFF] {
            assert_eq!(encode_cell(cell, metatile_index(cell)), cell);
        }
        // Setting a new index changes only bits 5..=14; flag + low bits stay.
        let edited = encode_cell(0x8D21, 0x12A);
        assert_eq!(metatile_index(edited), 0x12A);
        assert_eq!(edited & 0x801F, 0x8D21 & 0x801F);
    }
}
