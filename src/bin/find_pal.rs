//! Diagnostic: given a live CGRAM hex dump (512 bytes, e.g. captured by
//! `tools/mesen/dump_ppu.lua`), report which graphics-table entries —
//! decompressed — reproduce runs of it. This is how the per-level palette is
//! validated against the real machine and how palette uploads issued *outside*
//! the per-scene setup routine (common/transition code the static
//! `level::loader` scan can't see) are identified.
//!
//! It is what established that level 0's live BG palette = the scene's CGRAM-`$00`
//! upload (id 10, rows 1..7) overlaid by the common palette (id 1) on row 0 —
//! the finding behind `loader::COMMON_BG_PALETTE_GFX_ID`.
//!
//! Output is match offsets/lengths only (no ROM bytes); safe to commit a report.
//!
//! Usage: cargo run --bin find_pal -- <rom> <cgram_hex_file>

use anyhow::{bail, Result};
use daffy_editor::codecs::gfx_rle;
use daffy_editor::gfx::table::{parse_game_table, UploadTarget};
use daffy_editor::rom::loader::load_rom_file;
use daffy_editor::snes::lorom::snes_to_pc;

fn parse_hex(s: &str) -> Vec<u8> {
    let s: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap()).collect()
}

/// Longest run length where `a[ao+k] == b[bo+k]`.
fn match_len(a: &[u8], ao: usize, b: &[u8], bo: usize) -> usize {
    let mut k = 0;
    while ao + k < a.len() && bo + k < b.len() && a[ao + k] == b[bo + k] {
        k += 1;
    }
    k
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(path), Some(hexf)) = (args.first(), args.get(1)) else {
        bail!("usage: find_pal <rom> <cgram_hex_file>");
    };
    let rom = load_rom_file(path.as_ref())?;
    let live = parse_hex(&std::fs::read_to_string(hexf)?);
    println!("live cgram: {} bytes ({} colors)", live.len(), live.len() / 2);

    let table = parse_game_table(&rom.data)?;
    for e in &table {
        let Ok(src_pc) = snes_to_pc(e.source) else { continue };
        let Some(src) = rom.data.get(src_pc..) else { continue };
        let Ok(d) = gfx_rle::decompress(src) else { continue };

        // Longest contiguous match of the decoded blob anywhere in live CGRAM.
        let (mut best, mut best_at) = (0usize, 0usize);
        for bo in 0..live.len() {
            let m = match_len(&d.data, 0, &live, bo);
            if m > best {
                best = m;
                best_at = bo;
            }
        }
        if best >= 16 {
            let tgt = match e.upload() {
                UploadTarget::Cgram { addr, size } => format!("CGRAM ${addr:02X} size ${size:04X}"),
                UploadTarget::Vram { word_addr, size } => format!("VRAM ${word_addr:04X} size ${size:04X}"),
                UploadTarget::Wram { dest } => format!("WRAM ${dest:06X}"),
                UploadTarget::Unknown { mode } => format!("mode {mode}?"),
            };
            println!(
                "id {:3} mode {} src {:06X} [{}] decoded {:5}B -> matches {:3} bytes of live at color {} (byte {})",
                e.index, e.mode, e.source, tgt, d.data.len(), best, best_at / 2, best_at
            );
        }
    }
    Ok(())
}
