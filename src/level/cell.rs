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
//! - **bit 15** is a per-cell **flag** (orientation/solidity — *likely*, not yet
//!   pinned down). `$8000` (index 0, flag set) is the dominant "empty/sky" cell.
//!
//! Confidence: the stride/index decode is **confirmed**; the flag meaning and the
//! 4×4 metatile shape are **likely** pending the renderer disassembly. See
//! `docs/reverse-engineering/level-format.md`.

/// Bytes per metatile definition in the tileset (16 tilemap words).
pub const METATILE_BYTES: usize = 0x20;
/// SNES tilemap words per metatile (a 4×4 block of 8×8 tiles).
pub const METATILE_WORDS: usize = METATILE_BYTES / 2;

/// The metatile index a cell selects (`(cell & 0x7FFF) >> 5`).
pub fn metatile_index(cell: u16) -> u16 {
    (cell & 0x7FFF) >> 5
}

/// Byte offset of the cell's metatile within the tileset (`metatile_index * $20`).
pub fn tileset_offset(cell: u16) -> usize {
    metatile_index(cell) as usize * METATILE_BYTES
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
    fn index_offset_round_trips_within_a_metatile_grid() {
        for idx in [0u16, 1, 2, 0x69, 0x12A] {
            let cell = idx << 5;
            assert_eq!(metatile_index(cell), idx);
            assert_eq!(tileset_offset(cell), idx as usize * METATILE_BYTES);
        }
    }
}
