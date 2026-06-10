//! Scan a user-supplied ROM for candidate pointer tables (16-bit per-bank and
//! 24-bit long pointers). Outputs a JSON report to stdout.
//!
//! Usage: cargo run --bin scan_pointers -- <rom-path> [--min-entries N]
//!
//! Everything reported here is SPECULATIVE; see docs/reverse-engineering/.

use anyhow::{bail, Context, Result};
use daffy_editor::codecs::experimental::{scan_pointer_tables_16, scan_pointer_tables_24};
use daffy_editor::rom::info::analyze_rom;
use daffy_editor::rom::loader::load_rom_file;
use daffy_editor::snes::lorom::pc_to_snes;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        bail!("usage: scan_pointers <rom-path> [--min-entries N]");
    };
    let min_entries = args
        .iter()
        .position(|a| a == "--min-entries")
        .and_then(|i| args.get(i + 1))
        .map(|v| v.parse::<usize>())
        .transpose()
        .context("--min-entries expects a number")?
        .unwrap_or(8);

    let rom = load_rom_file(path.as_ref())?;
    let info = analyze_rom(&rom.data, rom.had_copier_header);

    let banks = rom.data.len() / 0x8000;
    let mut tables16 = Vec::new();
    for bank in 0..banks.min(0x7E) as u8 {
        let lo = bank as usize * 0x8000;
        let hi = lo + 0x8000;
        for mut t in scan_pointer_tables_16(&rom.data[lo..hi], bank, min_entries) {
            t.offset += lo;
            t.snes_addr = pc_to_snes(t.offset).ok();
            tables16.push(t);
        }
    }
    let tables24 = scan_pointer_tables_24(&rom.data, min_entries);

    let report = serde_json::json!({
        "tool": "scan_pointers",
        "confidence": "speculative",
        "rom": { "crc32": format!("{:08X}", info.crc32), "size": info.size,
                 "version": format!("{:?}", info.version) },
        "min_entries": min_entries,
        "pointer_tables_16bit": tables16,
        "pointer_tables_24bit": tables24,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
