//! Print the statically reconstructed `$DB` per-character attribute table for a
//! level as a single hex string (0x400 bytes) — for diffing the editor's
//! palette-row/flip derivation against a live Mesen VRAM tilemap dump
//! (`tools/mesen/dump_ppu.lua`). Aggregate attribute data only; no tile bytes.
//!
//! Usage: cargo run --bin dump_attr -- <rom> [level]

use anyhow::{bail, Result};
use daffy_editor::level::load_rom_level;
use daffy_editor::rom::loader::load_rom_file;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else { bail!("usage: dump_attr <rom> [level]") };
    let n: usize = args.get(1).map(|s| s.parse()).transpose()?.unwrap_or(0);
    let rom = load_rom_file(path.as_ref())?;
    let level = load_rom_level(&rom.data, n)?;
    let mut hex = String::with_capacity(level.gfx.attr.len() * 2);
    for &b in &level.gfx.attr {
        hex.push_str(&format!("{b:02X}"));
    }
    println!("{hex}");
    Ok(())
}
