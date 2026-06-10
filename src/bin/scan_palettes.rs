//! Scan a user-supplied ROM for candidate CGRAM palette blocks.
//! Outputs a JSON report to stdout.
//!
//! Usage: cargo run --bin scan_palettes -- <rom-path> [--min-rows N]
//!
//! Everything reported here is SPECULATIVE; see docs/reverse-engineering/.

use anyhow::{bail, Context, Result};
use daffy_editor::codecs::experimental::scan_palettes;
use daffy_editor::rom::info::analyze_rom;
use daffy_editor::rom::loader::load_rom_file;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        bail!("usage: scan_palettes <rom-path> [--min-rows N]");
    };
    let min_rows = args
        .iter()
        .position(|a| a == "--min-rows")
        .and_then(|i| args.get(i + 1))
        .map(|v| v.parse::<usize>())
        .transpose()
        .context("--min-rows expects a number")?
        .unwrap_or(2);

    let rom = load_rom_file(path.as_ref())?;
    let info = analyze_rom(&rom.data, rom.had_copier_header);
    let mut candidates = scan_palettes(&rom.data);
    candidates.retain(|c| c.rows >= min_rows);
    candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
    candidates.truncate(200);

    let report = serde_json::json!({
        "tool": "scan_palettes",
        "confidence": "speculative",
        "rom": { "crc32": format!("{:08X}", info.crc32), "size": info.size,
                 "version": format!("{:?}", info.version) },
        "min_rows": min_rows,
        "candidates": candidates,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
