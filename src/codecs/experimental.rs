//! Heuristic scanners for reverse engineering — shared by the CLI tools in
//! `src/bin/` and the experimental GUI views.
//!
//! Everything in this module is SPECULATIVE tooling: it reports *candidates*
//! to a human, never facts. Confirmed findings graduate to documented codecs
//! with regression tests; see docs/reverse-engineering/README.md.

use serde::Serialize;

use crate::snes::lorom::pc_to_snes;

/// A candidate run of 16-bit little-endian pointers that would land in ROM
/// when interpreted within `assumed_bank`.
#[derive(Debug, Clone, Serialize)]
pub struct PointerTableCandidate {
    pub offset: usize,
    pub snes_addr: Option<u32>,
    pub entries: usize,
    pub width: u8,
    pub assumed_bank: Option<u8>,
    pub first_targets: Vec<u32>,
}

/// Scan for runs of plausible 16-bit pointers ($8000-$FFFF values) assumed to
/// live in `bank`. Many SNES games store per-bank pointer tables this way.
pub fn scan_pointer_tables_16(data: &[u8], bank: u8, min_entries: usize) -> Vec<PointerTableCandidate> {
    let mut out = Vec::new();
    let plausible = |off: usize| -> bool {
        off + 1 < data.len() && data[off + 1] >= 0x80 // value in $8000-$FFFF
    };
    let mut i = 0;
    while i + 2 * min_entries <= data.len() {
        if plausible(i) {
            let start = i;
            let mut n = 0;
            while plausible(i) {
                n += 1;
                i += 2;
            }
            if n >= min_entries {
                let first_targets = (0..n.min(4))
                    .map(|k| {
                        let v = u16::from_le_bytes([data[start + 2 * k], data[start + 2 * k + 1]]);
                        ((bank as u32) << 16) | v as u32
                    })
                    .collect();
                out.push(PointerTableCandidate {
                    offset: start,
                    snes_addr: pc_to_snes(start).ok(),
                    entries: n,
                    width: 2,
                    assumed_bank: Some(bank),
                    first_targets,
                });
            }
            i = start + 2 * n.max(1);
        } else {
            i += 1;
        }
    }
    out
}

/// Scan for runs of 24-bit pointers whose targets fall inside the ROM's
/// LoROM address space.
pub fn scan_pointer_tables_24(data: &[u8], min_entries: usize) -> Vec<PointerTableCandidate> {
    let rom_banks = (data.len() / 0x8000) as u8;
    let plausible = |off: usize| -> bool {
        if off + 2 >= data.len() {
            return false;
        }
        let bank = data[off + 2] & 0x7F;
        data[off + 1] >= 0x80 && bank < rom_banks && bank < 0x7E
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 * min_entries <= data.len() {
        if plausible(i) {
            let start = i;
            let mut n = 0;
            while plausible(i) {
                n += 1;
                i += 3;
            }
            if n >= min_entries {
                let first_targets = (0..n.min(4))
                    .map(|k| {
                        u32::from_le_bytes([
                            data[start + 3 * k],
                            data[start + 3 * k + 1],
                            data[start + 3 * k + 2],
                            0,
                        ])
                    })
                    .collect();
                out.push(PointerTableCandidate {
                    offset: start,
                    snes_addr: pc_to_snes(start).ok(),
                    entries: n,
                    width: 3,
                    assumed_bank: None,
                    first_targets,
                });
            }
            i = start + 3 * n.max(1);
        } else {
            i += 1;
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct PaletteCandidate {
    pub offset: usize,
    pub snes_addr: Option<u32>,
    /// 16-color rows that look plausible at this offset.
    pub rows: usize,
    pub starts_with_black: bool,
    pub score: f32,
}

/// Scan for plausible CGRAM palette data: runs of 16-bit words with bit 15
/// clear and decent color variety. SNES palettes are usually stored as one
/// or more 16-color (32-byte) rows.
pub fn scan_palettes(data: &[u8]) -> Vec<PaletteCandidate> {
    const ROW_BYTES: usize = 32;
    let mut out = Vec::new();
    let mut i = 0;
    while i + ROW_BYTES <= data.len() {
        let mut rows = 0;
        while i + (rows + 1) * ROW_BYTES <= data.len() && plausible_row(&data[i + rows * ROW_BYTES..][..ROW_BYTES]) {
            rows += 1;
        }
        if rows >= 1 {
            let first = u16::from_le_bytes([data[i], data[i + 1]]);
            let score = rows as f32 + if first == 0 { 0.5 } else { 0.0 };
            out.push(PaletteCandidate {
                offset: i,
                snes_addr: pc_to_snes(i).ok(),
                rows,
                starts_with_black: first == 0,
                score,
            });
            i += rows * ROW_BYTES;
        } else {
            i += 2;
        }
    }
    out
}

fn plausible_row(row: &[u8]) -> bool {
    let mut colors = [0u16; 16];
    for (k, c) in colors.iter_mut().enumerate() {
        *c = u16::from_le_bytes([row[k * 2], row[k * 2 + 1]]);
        if *c & 0x8000 != 0 {
            return false; // bit 15 must be clear in CGRAM data
        }
    }
    // Reject flat runs (all-equal words are usually padding, not a palette).
    let distinct = {
        let mut s = colors.to_vec();
        s.sort_unstable();
        s.dedup();
        s.len()
    };
    distinct >= 6
}

#[derive(Debug, Clone, Serialize)]
pub struct RepeatedBlock {
    pub len: usize,
    pub count: usize,
    pub offsets: Vec<usize>,
    pub preview_hex: String,
}

/// Find byte blocks of length `block_len` (aligned to `block_len`) that occur
/// at least `min_count` times. Repetition often marks structure arrays,
/// blank tiles, or padding.
pub fn scan_repeated_blocks(data: &[u8], block_len: usize, min_count: usize) -> Vec<RepeatedBlock> {
    use std::collections::HashMap;
    if block_len == 0 {
        return Vec::new();
    }
    let mut map: HashMap<&[u8], Vec<usize>> = HashMap::new();
    let mut i = 0;
    while i + block_len <= data.len() {
        map.entry(&data[i..i + block_len]).or_default().push(i);
        i += block_len;
    }
    let mut out: Vec<RepeatedBlock> = map
        .into_iter()
        .filter(|(_, offs)| offs.len() >= min_count)
        .map(|(block, offsets)| RepeatedBlock {
            len: block_len,
            count: offsets.len(),
            offsets: offsets.into_iter().take(16).collect(),
            preview_hex: block.iter().take(16).map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "),
        })
        .collect();
    out.sort_by(|a, b| b.count.cmp(&a.count).then(a.offsets.cmp(&b.offsets)));
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct TileRegionCandidate {
    pub offset: usize,
    pub snes_addr: Option<u32>,
    pub tiles: usize,
    pub score: f32,
}

/// Heuristic scan for 4bpp tile graphics regions. Real tile data tends to
/// have moderate byte entropy and strong row-pair correlation; padding and
/// code do not. This is a coarse human-guided filter, nothing more.
pub fn scan_tile_regions(data: &[u8], min_tiles: usize) -> Vec<TileRegionCandidate> {
    const TILE: usize = 32;
    let looks_like_tile = |chunk: &[u8]| -> bool {
        let zeros = chunk.iter().filter(|&&b| b == 0).count();
        let distinct = {
            let mut seen = [false; 256];
            for &b in chunk {
                seen[b as usize] = true;
            }
            seen.iter().filter(|&&s| s).count()
        };
        // Not blank, not saturated noise.
        zeros < 30 && (2..=24).contains(&distinct)
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i + TILE <= data.len() {
        if looks_like_tile(&data[i..i + TILE]) {
            let start = i;
            let mut tiles = 0;
            while i + TILE <= data.len() && looks_like_tile(&data[i..i + TILE]) {
                tiles += 1;
                i += TILE;
            }
            if tiles >= min_tiles {
                out.push(TileRegionCandidate {
                    offset: start,
                    snes_addr: pc_to_snes(start).ok(),
                    tiles,
                    score: tiles as f32,
                });
            }
        } else {
            i += TILE;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_planted_16bit_pointer_table() {
        let mut data = vec![0x00u8; 0x400]; // 0x00 high bytes are implausible
        // Plant 8 pointers at 0x100: $8100, $8200, ...
        for k in 0..8usize {
            data[0x100 + 2 * k] = 0x00;
            data[0x100 + 2 * k + 1] = 0x81 + k as u8;
        }
        let found = scan_pointer_tables_16(&data, 0x01, 6);
        assert!(found.iter().any(|c| c.offset == 0x100 && c.entries >= 8), "{found:?}");
    }

    #[test]
    fn finds_planted_palette() {
        let mut data = vec![0xFFu8; 0x200]; // bit 15 set => implausible everywhere
        // Plant a 16-color row at 0x80 with distinct low-bit-15 colors.
        for k in 0..16usize {
            let c = (k as u16) * 0x111 & 0x7FFF;
            data[0x80 + 2 * k..0x80 + 2 * k + 2].copy_from_slice(&c.to_le_bytes());
        }
        let found = scan_palettes(&data);
        assert!(found.iter().any(|c| c.offset == 0x80 && c.rows >= 1), "{found:?}");
    }

    #[test]
    fn finds_planted_repeated_blocks() {
        let mut data = vec![0u8; 0x100];
        for (n, chunk) in data.chunks_mut(16).enumerate() {
            chunk.fill(if n % 2 == 0 { 0xAB } else { n as u8 });
        }
        let found = scan_repeated_blocks(&data, 16, 3);
        assert!(!found.is_empty());
        assert!(found[0].count >= 8);
    }

    #[test]
    fn empty_input_is_safe() {
        assert!(scan_pointer_tables_16(&[], 0, 4).is_empty());
        assert!(scan_pointer_tables_24(&[], 4).is_empty());
        assert!(scan_palettes(&[]).is_empty());
        assert!(scan_repeated_blocks(&[], 16, 2).is_empty());
        assert!(scan_tile_regions(&[], 4).is_empty());
    }
}
