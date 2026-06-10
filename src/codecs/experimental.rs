//! Heuristic scanners for reverse engineering — shared by the CLI tools in
//! `src/bin/` and the experimental GUI views.
//!
//! Everything in this module is SPECULATIVE tooling: it reports *candidates*
//! to a human, never facts. Confirmed findings graduate to documented codecs
//! with regression tests; see docs/reverse-engineering/README.md.

use serde::Serialize;

use crate::snes::lorom::{pc_to_snes, snes_to_pc};

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
pub fn scan_pointer_tables_16(
    data: &[u8],
    bank: u8,
    min_entries: usize,
) -> Vec<PointerTableCandidate> {
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
        while i + (rows + 1) * ROW_BYTES <= data.len()
            && plausible_row(&data[i + rows * ROW_BYTES..][..ROW_BYTES])
        {
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
            preview_hex: block
                .iter()
                .take(16)
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
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

/// A candidate general-purpose DMA transfer reconstructed from 65816 setup
/// code. Anchored on the transfer trigger (`STA $420B`); the per-channel
/// register writes preceding it are paired with their immediate loads.
#[derive(Debug, Clone, Serialize)]
pub struct DmaCandidate {
    /// PC offset of the `STA $420B` (MDMAEN) that triggers the transfer.
    pub code_offset: usize,
    pub code_snes_addr: Option<u32>,
    /// DMA channel (0-7) this candidate describes.
    pub channel: u8,
    /// 24-bit SNES source address (A1B:A1T) the channel copies *from*.
    pub source_addr: u32,
    /// Source as a headerless PC offset, when it maps into LoROM.
    pub source_pc: Option<usize>,
    /// B-bus destination register low byte (`$21xx`), e.g. 0x18 = VRAM data.
    pub b_bus_reg: Option<u8>,
    /// Coarse classification of the destination from `b_bus_reg`.
    pub kind: &'static str,
    /// Transfer size in bytes (DAS register), when present in the setup.
    pub size: Option<u16>,
    /// True when the trigger's channel mask was an immediate that selected
    /// this channel; false when the mask was set elsewhere (lower confidence).
    pub mask_confirmed: bool,
}

/// Classify a DMA transfer by its B-bus destination register (`$21xx` low byte).
fn dma_kind(b_bus_reg: Option<u8>) -> &'static str {
    match b_bus_reg {
        Some(0x18 | 0x19) => "vram",
        Some(0x22) => "cgram",
        Some(0x04) => "oam",
        Some(0x80) => "wram",
        Some(_) => "other",
        None => "unknown",
    }
}

/// Scan for general-purpose DMA transfers by recognizing their 65816 setup
/// code. Each transfer is anchored on `STA $420B` (MDMAEN); for every channel
/// enabled by the preceding `LDA #mask`, the scanner walks back a bounded
/// window collecting that channel's `STA $43xN` register writes and pairs each
/// with the immediate (`LDA #imm`) that fed it. This reconstructs the source
/// address and destination, which are the strongest leads for *where* the game
/// keeps graphics and palettes. Heuristic and SPECULATIVE: it does not track
/// the M/X flags, so it accepts both 8- and 16-bit immediates.
pub fn scan_dma_sources(data: &[u8]) -> Vec<DmaCandidate> {
    /// How far back from the trigger to look for register setup.
    const WINDOW: usize = 96;
    let mut out = Vec::new();

    // Read the immediate operand that a `STA` at `sta` consumes: a preceding
    // `LDA #imm`, either 16-bit (`A9 lo hi`) or 8-bit (`A9 imm`).
    let preceding_imm = |sta: usize| -> Option<u16> {
        if sta >= 3 && data[sta - 3] == 0xA9 {
            return Some(u16::from_le_bytes([data[sta - 2], data[sta - 1]]));
        }
        if sta >= 2 && data[sta - 2] == 0xA9 {
            return Some(data[sta - 1] as u16);
        }
        None
    };

    let mut i = 0;
    while i + 3 <= data.len() {
        // STA $420B (MDMAEN) — the DMA trigger.
        if !(data[i] == 0x8D && data[i + 1] == 0x0B && data[i + 2] == 0x42) {
            i += 1;
            continue;
        }
        // The channel mask is an immediate in the common case, but shared DMA
        // helpers leave it in A from elsewhere. Treat it as an optional hint.
        let mask = preceding_imm(i).map(|m| m as u8);

        // Collect per-channel register writes in the preceding window. Index 2
        // of each tuple is the register low-nibble: 1=BBAD, 2=A1TL, 3=A1TH,
        // 4=A1B, 5=DASL, 6=DASH.
        let lo = i.saturating_sub(WINDOW);
        // regs[channel][reg_nibble] = immediate written there (last wins).
        let mut regs = [[None::<u16>; 7]; 8];
        let mut j = lo;
        while j + 3 <= i {
            if data[j] == 0x8D && data[j + 2] == 0x43 {
                let reg_lo = data[j + 1];
                let ch = ((reg_lo >> 4) & 0x07) as usize;
                let nibble = (reg_lo & 0x0F) as usize;
                if nibble < 7 {
                    if let Some(imm) = preceding_imm(j) {
                        regs[ch][nibble] = Some(imm);
                    }
                }
            }
            j += 1;
        }

        for (ch, regch) in regs.iter().enumerate() {
            // Need at least a source low/word to say anything useful. When the
            // mask is a known immediate, only report the channels it selects;
            // otherwise report any channel that was set up in the window.
            let selected = mask.map(|m| m & (1 << ch) != 0);
            if selected == Some(false) {
                continue;
            }
            let Some(a1tl) = regch[2] else { continue };
            // A 16-bit A1TL immediate already carries the high byte; otherwise
            // fold in a separate A1TH store.
            let a1t = if a1tl > 0xFF {
                a1tl
            } else {
                a1tl | (regch[3].unwrap_or(0) << 8)
            };
            let bank = regch[4].unwrap_or(0) as u32 & 0xFF;
            let source_addr = (bank << 16) | a1t as u32;
            let size = match (regch[5], regch[6]) {
                (None, None) => None,
                (l, h) => Some(l.unwrap_or(0) | (h.unwrap_or(0) << 8)),
            };
            let b_bus_reg = regch[1].map(|v| v as u8);
            out.push(DmaCandidate {
                code_offset: i,
                code_snes_addr: pc_to_snes(i).ok(),
                channel: ch as u8,
                source_addr,
                source_pc: snes_to_pc(source_addr).ok(),
                b_bus_reg,
                kind: dma_kind(b_bus_reg),
                size,
                mask_confirmed: selected == Some(true),
            });
        }
        i += 3;
    }
    out
}

/// One store into a DMA channel register ($43xx), with the addressing mode of
/// the store and a classification of where the *value* came from.
#[derive(Debug, Clone, Serialize)]
pub struct DmaRegWrite {
    /// PC offset of the `STA` instruction.
    pub offset: usize,
    /// `$43xx` low byte written. With absolute addressing this encodes
    /// channel = `(reg_low >> 4) & 7` and register = `reg_low & 0x0F`
    /// (2=A1TL, 3=A1TH, 4=A1B, 5=DASL...). With indexed addressing it is the
    /// base (usually `$4300`,X) and the channel is selected by X at run time.
    pub reg_low: u8,
    /// Channel when statically known (absolute store); `None` when the store is
    /// indexed (`STA $43xx,X/Y`) so the channel depends on a runtime index.
    pub channel: Option<u8>,
    /// How the register address was formed: `abs`, `abs,x`, `abs,y`, `long`.
    pub store_mode: &'static str,
    /// Where the stored value came from, decoded from the preceding load:
    /// `imm` (constant), `mem` (absolute/direct variable), `indexed`
    /// (table,X / table,Y — a parameter table), `indirect` ((dp),Y etc. — a
    /// pointer), `reg` (TXA/TYA/PLA), or `?` (undecoded).
    pub value_source: &'static str,
    /// Operand of a `mem`/`indexed`/`indirect` load — a candidate parameter
    /// table or variable address (raw 16- or 24-bit value as encoded).
    pub source_operand: Option<u32>,
}

/// A cluster of DMA-register stores: one DMA *setup site*. The fixed
/// init/HUD uploads are `immediate`; the shared parameterized helper that
/// uploads the bulk of the game's graphics is `parameterized` (its register
/// values come from memory/tables, which is why `scan_dma` cannot follow it).
#[derive(Debug, Clone, Serialize)]
pub struct DmaSetupSite {
    pub start_offset: usize,
    pub start_snes: Option<u32>,
    pub writes: Vec<DmaRegWrite>,
    /// `immediate` (all values constant) or `parameterized` (≥1 value from
    /// memory/table/pointer/register).
    pub kind: &'static str,
    /// True if any store used `,X`/`,Y` on the register address — the textbook
    /// shape of a generic "DMA channel N" helper.
    pub uses_index: bool,
    /// True if a `STA $420B` (MDMAEN trigger) follows the cluster within a
    /// short window — i.e. this site actually fires a transfer here.
    pub triggers_dma: bool,
    /// Distinct operands of the non-immediate loads feeding the registers:
    /// the strongest leads for the parameter table(s) the helper reads.
    pub param_operands: Vec<u32>,
}

/// Decode the load that produced the value a `STA` at `sta` stores, using the
/// same bounded backward heuristic as [`scan_dma_sources`]. Returns the source
/// classification and, for memory/indexed/indirect loads, the operand. Longer
/// encodings are tried first so a 3/4-byte load is not mistaken for a shorter
/// opcode sitting in its operand bytes. Heuristic and SPECULATIVE: it does not
/// track M/X widths or real instruction boundaries.
fn classify_value_source(data: &[u8], sta: usize) -> (&'static str, Option<u32>) {
    // 4-byte loads: long / long,X.
    if sta >= 4 {
        let op = data[sta - 4];
        let operand =
            Some(u32::from_le_bytes([data[sta - 3], data[sta - 2], data[sta - 1], 0]));
        match op {
            0xAF => return ("mem", operand),     // LDA addr (long)
            0xBF => return ("indexed", operand), // LDA addr,X (long)
            _ => {}
        }
    }
    // 3-byte loads: immediate(16), absolute, absolute,X/Y.
    if sta >= 3 {
        let op = data[sta - 3];
        let operand = Some(u16::from_le_bytes([data[sta - 2], data[sta - 1]]) as u32);
        match op {
            0xA9 => return ("imm", None),         // LDA #imm16
            0xAD => return ("mem", operand),      // LDA abs
            0xBD | 0xB9 => return ("indexed", operand), // LDA abs,X / abs,Y
            _ => {}
        }
    }
    // 2-byte loads: immediate(8), direct page and its indexed/indirect forms.
    if sta >= 2 {
        let op = data[sta - 2];
        let operand = Some(data[sta - 1] as u32);
        match op {
            0xA9 => return ("imm", None),                   // LDA #imm8
            0xA5 => return ("mem", operand),                // LDA dp
            0xB5 => return ("indexed", operand),            // LDA dp,X
            0xA1 | 0xB1 | 0xB2 | 0xA7 | 0xB7 => return ("indirect", operand), // (dp,X)/(dp),Y/(dp)/[dp]/[dp],Y
            _ => {}
        }
    }
    // 1-byte: value moved into A from a register or the stack.
    if sta >= 1 {
        match data[sta - 1] {
            0x8A | 0x98 | 0x68 => return ("reg", None), // TXA / TYA / PLA
            _ => {}
        }
    }
    ("?", None)
}

/// Decode a store to a DMA register at `i`, if any. Returns the write plus the
/// instruction length so the caller can advance. Accepts absolute (`8D`),
/// absolute,X (`9D`), absolute,Y (`99`) into `$4300-$437B` and the long form
/// (`8F .. 43 00`). Only register low-nibbles 0x0-0xB (the real DMA registers)
/// are accepted, which keeps random `?? .. 43` bytes from matching.
fn decode_dma_reg_store(data: &[u8], i: usize) -> Option<(DmaRegWrite, usize)> {
    let valid = |low: u8| (low >> 4) <= 0x07 && (low & 0x0F) <= 0x0B;
    // Long: 8F low 43 00  (STA $0043xx)
    if i + 4 <= data.len() && data[i] == 0x8F && data[i + 2] == 0x43 && data[i + 3] == 0x00 {
        let low = data[i + 1];
        if valid(low) {
            let (vs, op) = classify_value_source(data, i);
            return Some((
                DmaRegWrite {
                    offset: i,
                    reg_low: low,
                    channel: Some((low >> 4) & 7),
                    store_mode: "long",
                    value_source: vs,
                    source_operand: op,
                },
                4,
            ));
        }
    }
    if i + 3 > data.len() || data[i + 2] != 0x43 {
        return None;
    }
    let low = data[i + 1];
    if !valid(low) {
        return None;
    }
    let (mode, channel) = match data[i] {
        0x8D => ("abs", Some((low >> 4) & 7)),
        0x9D => ("abs,x", None),
        0x99 => ("abs,y", None),
        _ => return None,
    };
    let (vs, op) = classify_value_source(data, i);
    Some((
        DmaRegWrite {
            offset: i,
            reg_low: low,
            channel,
            store_mode: mode,
            value_source: vs,
            source_operand: op,
        },
        3,
    ))
}

/// Find DMA *setup sites* — clusters of stores to the channel registers — and
/// classify each as `immediate` or `parameterized`. The parameterized sites
/// are the shared DMA helper(s) that move the bulk of the game's graphics and
/// whose register values come from tables in ROM; their `param_operands` are
/// the leads for those tables. This complements [`scan_dma_sources`], which
/// only reconstructs the immediate-fed transfers.
///
/// Heuristic / SPECULATIVE: clustering and the backward value decode do not
/// disassemble precisely; confirm any lead before writing code against it.
pub fn scan_dma_setup_sites(data: &[u8]) -> Vec<DmaSetupSite> {
    /// Max byte gap between consecutive register stores in one cluster.
    const CLUSTER_GAP: usize = 24;
    /// How far past the last store to look for the MDMAEN trigger.
    const TRIGGER_WINDOW: usize = 48;

    // Pass 1: every DMA-register store, in order.
    let mut writes: Vec<DmaRegWrite> = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if let Some((w, len)) = decode_dma_reg_store(data, i) {
            writes.push(w);
            i += len;
        } else {
            i += 1;
        }
    }

    // Pass 2: cluster consecutive stores; the end of a store is offset+3 (or +4
    // for the long form, but +3 is a safe lower bound for gap purposes).
    let mut sites = Vec::new();
    let mut start = 0;
    while start < writes.len() {
        let mut end = start;
        while end + 1 < writes.len()
            && writes[end + 1].offset.saturating_sub(writes[end].offset + 3) <= CLUSTER_GAP
        {
            end += 1;
        }
        let group = &writes[start..=end];
        // A real setup writes several registers; require ≥2 to suppress the
        // stray `8D xx 43` that lands in data.
        if group.len() >= 2 {
            let uses_index = group.iter().any(|w| w.store_mode != "abs" && w.store_mode != "long");
            let parameterized = group
                .iter()
                .any(|w| matches!(w.value_source, "mem" | "indexed" | "indirect" | "reg"));
            let last = group.last().unwrap();
            let scan_from = last.offset + 3;
            let scan_to = (scan_from + TRIGGER_WINDOW).min(data.len().saturating_sub(2));
            let triggers_dma = (scan_from..scan_to)
                .any(|k| data[k] == 0x8D && data[k + 1] == 0x0B && data[k + 2] == 0x42);
            let mut param_operands: Vec<u32> = group
                .iter()
                .filter(|w| w.value_source != "imm")
                .filter_map(|w| w.source_operand)
                .collect();
            param_operands.sort_unstable();
            param_operands.dedup();
            sites.push(DmaSetupSite {
                start_offset: group[0].offset,
                start_snes: pc_to_snes(group[0].offset).ok(),
                writes: group.to_vec(),
                kind: if parameterized { "parameterized" } else { "immediate" },
                uses_index,
                triggers_dma,
                param_operands,
            });
        }
        start = end + 1;
    }
    sites
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
        assert!(
            found.iter().any(|c| c.offset == 0x100 && c.entries >= 8),
            "{found:?}"
        );
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
        assert!(
            found.iter().any(|c| c.offset == 0x80 && c.rows >= 1),
            "{found:?}"
        );
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
    fn finds_planted_vram_dma_with_16bit_source() {
        // Channel 0: DMAP=$01, BBAD=$18 (VRAM), A1T=$C000 (16-bit imm),
        // A1B=$05, DAS=$0800, then trigger MDMAEN with channel-0 mask.
        let mut data = vec![0u8; 0x200];
        let prog = [
            0xA9, 0x01, 0x8D, 0x00, 0x43, // LDA #$01 : STA $4300 (DMAP)
            0xA9, 0x18, 0x8D, 0x01, 0x43, // LDA #$18 : STA $4301 (BBAD = VRAM)
            0xA9, 0x00, 0xC0, 0x8D, 0x02, 0x43, // LDA #$C000 : STA $4302 (A1TL/H)
            0xA9, 0x05, 0x8D, 0x04, 0x43, // LDA #$05 : STA $4304 (A1B)
            0xA9, 0x00, 0x08, 0x8D, 0x05, 0x43, // LDA #$0800 : STA $4305 (DAS)
            0xA9, 0x01, 0x8D, 0x0B, 0x42, // LDA #$01 : STA $420B (trigger)
        ];
        data[0x40..0x40 + prog.len()].copy_from_slice(&prog);

        let found = scan_dma_sources(&data);
        let c = found
            .iter()
            .find(|c| c.channel == 0 && c.kind == "vram")
            .unwrap_or_else(|| panic!("no VRAM DMA found: {found:?}"));
        assert_eq!(c.source_addr, 0x05_C000);
        assert_eq!(c.source_pc, Some(snes_to_pc(0x05_C000).unwrap()));
        assert_eq!(c.b_bus_reg, Some(0x18));
        assert_eq!(c.size, Some(0x0800));
    }

    #[test]
    fn classifies_cgram_dma_and_split_8bit_source() {
        // Channel 3, palette upload to CGRAM ($2122), source bank/addr loaded
        // as separate 8-bit immediates into A1TL and A1TH.
        let mut data = vec![0u8; 0x100];
        let prog = [
            0xA9, 0x22, 0x8D, 0x31, 0x43, // STA $4331 (BBAD = CGRAM)
            0xA9, 0x34, 0x8D, 0x32, 0x43, // A1TL = $34
            0xA9, 0x12, 0x8D, 0x33, 0x43, // A1TH = $12  -> A1T = $1234
            0xA9, 0x06, 0x8D, 0x34, 0x43, // A1B  = $06
            0xA9, 0x08, 0x8D, 0x0B, 0x42, // trigger channel 3 (mask $08)
        ];
        data[0x10..0x10 + prog.len()].copy_from_slice(&prog);

        let found = scan_dma_sources(&data);
        let c = found
            .iter()
            .find(|c| c.channel == 3)
            .expect("channel 3 DMA");
        assert_eq!(c.kind, "cgram");
        assert_eq!(c.source_addr, 0x06_1234);
    }

    #[test]
    fn ignores_trigger_without_setup() {
        // A bare STA $420B with no register writes yields nothing.
        let mut data = vec![0u8; 0x40];
        data[0x20..0x25].copy_from_slice(&[0xA9, 0x01, 0x8D, 0x0B, 0x42]);
        assert!(scan_dma_sources(&data).is_empty());
    }

    #[test]
    fn empty_input_is_safe() {
        assert!(scan_pointer_tables_16(&[], 0, 4).is_empty());
        assert!(scan_pointer_tables_24(&[], 4).is_empty());
        assert!(scan_palettes(&[]).is_empty());
        assert!(scan_repeated_blocks(&[], 16, 2).is_empty());
        assert!(scan_tile_regions(&[], 4).is_empty());
        assert!(scan_dma_sources(&[]).is_empty());
        assert!(scan_dma_setup_sites(&[]).is_empty());
    }

    #[test]
    fn finds_parameterized_indexed_dma_helper() {
        // A generic "DMA channel X" helper: source registers loaded from a
        // table ($9000,X) and stored to $43xx,X, then the transfer triggered.
        let mut data = vec![0u8; 0x200];
        let prog = [
            0xBD, 0x00, 0x90, 0x9D, 0x02, 0x43, // LDA $9000,X : STA $4302,X (A1TL)
            0xBD, 0x01, 0x90, 0x9D, 0x03, 0x43, // LDA $9001,X : STA $4303,X (A1TH)
            0xBD, 0x02, 0x90, 0x9D, 0x04, 0x43, // LDA $9002,X : STA $4304,X (A1B)
            0xA9, 0x01, 0x8D, 0x0B, 0x42, //       LDA #$01    : STA $420B (trigger)
        ];
        data[0x40..0x40 + prog.len()].copy_from_slice(&prog);

        let sites = scan_dma_setup_sites(&data);
        // start_offset is the first *store* (0x43), not the first load (0x40).
        let site = sites
            .iter()
            .find(|s| s.start_offset == 0x43)
            .unwrap_or_else(|| panic!("no site at 0x43: {sites:?}"));
        assert_eq!(site.kind, "parameterized");
        assert!(site.uses_index, "should detect ,X store: {site:?}");
        assert!(site.triggers_dma, "trigger should be in window: {site:?}");
        assert_eq!(site.writes.len(), 3);
        assert!(
            site.param_operands.contains(&0x9000),
            "table operand missing: {:?}",
            site.param_operands
        );
        assert!(site.writes.iter().all(|w| w.value_source == "indexed"));
    }

    #[test]
    fn classifies_immediate_fed_site() {
        // Three absolute register stores fed by immediates: the fixed-upload
        // shape that scan_dma already reconstructs.
        let mut data = vec![0u8; 0x100];
        let prog = [
            0xA9, 0x18, 0x8D, 0x01, 0x43, //       LDA #$18    : STA $4301 (BBAD)
            0xA9, 0x00, 0xC0, 0x8D, 0x02, 0x43, // LDA #$C000  : STA $4302 (A1TL/H)
            0xA9, 0x05, 0x8D, 0x04, 0x43, //       LDA #$05    : STA $4304 (A1B)
        ];
        data[0x20..0x20 + prog.len()].copy_from_slice(&prog);

        let sites = scan_dma_setup_sites(&data);
        let site = sites.iter().find(|s| s.start_offset == 0x22).expect("site");
        assert_eq!(site.kind, "immediate");
        assert!(!site.uses_index);
        assert!(site.param_operands.is_empty());
        assert!(site.writes.iter().all(|w| w.value_source == "imm"));
    }

    #[test]
    fn ignores_lone_register_store() {
        // A single `STA $4302` with no neighbours is not a setup site.
        let mut data = vec![0u8; 0x40];
        data[0x10..0x13].copy_from_slice(&[0x8D, 0x02, 0x43]);
        assert!(scan_dma_setup_sites(&data).is_empty());
    }
}
