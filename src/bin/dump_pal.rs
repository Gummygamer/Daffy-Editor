//! Print the statically reconstructed CGRAM palette for a level as a single hex
//! string (512 bytes, low byte first) — for diffing against a live Mesen CGRAM
//! dump (`tools/mesen/dump_ppu.lua`). Aggregate color data only; no tile bytes.
//!
//! Usage: cargo run --bin dump_pal -- <rom> [level]

use anyhow::{bail, Result};
use daffy_editor::level::load_rom_level;
use daffy_editor::rom::loader::load_rom_file;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else { bail!("usage: dump_pal <rom> [level]") };
    let n: usize = args.get(1).map(|s| s.parse()).transpose()?.unwrap_or(0);
    let rom = load_rom_file(path.as_ref())?;
    let level = load_rom_level(&rom.data, n)?;
    let mut hex = String::with_capacity(512 * 2);
    for &c in &level.palette.colors {
        hex.push_str(&format!("{:02X}{:02X}", c & 0xFF, c >> 8));
    }
    println!("{hex}");
    Ok(())
}
