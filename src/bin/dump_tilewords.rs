//! Print every metatile's 16 raw tile words for a level, one metatile per line
//! (`<index>: <16x 4-hex words>`), to test whether the words carry the SNES
//! tilemap palette/flip bits directly (vs the `$DB` per-char attr table).
//! Aggregate index data only; no tile pixels.
//!
//! Usage: cargo run --bin dump_tilewords -- <rom> [level]

use anyhow::{bail, Result};
use daffy_editor::level::load_rom_level;
use daffy_editor::rom::loader::load_rom_file;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else { bail!("usage: dump_tilewords <rom> [level]") };
    let n: usize = args.get(1).map(|s| s.parse()).transpose()?.unwrap_or(0);
    let rom = load_rom_file(path.as_ref())?;
    let level = load_rom_level(&rom.data, n)?;
    for (i, mt) in level.metatiles.iter().enumerate() {
        let words: Vec<String> = mt.tiles.iter().map(|w| format!("{w:04X}")).collect();
        println!("{i}: {}", words.join(" "));
    }
    Ok(())
}
