//! Scan a user-supplied ROM for regions that look like uncompressed 4bpp
//! tile graphics. Outputs a JSON report to stdout.
//!
//! Usage: cargo run --bin scan_tile_patterns -- <rom-path> [--min-tiles N]
//!
//! Everything reported here is SPECULATIVE; see docs/reverse-engineering/.

use anyhow::{bail, Context, Result};
use daffy_editor::codecs::experimental::scan_tile_regions;
use daffy_editor::rom::info::analyze_rom;
use daffy_editor::rom::loader::load_rom_file;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        bail!("usage: scan_tile_patterns <rom-path> [--min-tiles N]");
    };
    let min_tiles = args
        .iter()
        .position(|a| a == "--min-tiles")
        .and_then(|i| args.get(i + 1))
        .map(|v| v.parse::<usize>())
        .transpose()
        .context("--min-tiles expects a number")?
        .unwrap_or(16);

    let rom = load_rom_file(path.as_ref())?;
    let info = analyze_rom(&rom.data, rom.had_copier_header);
    let mut regions = scan_tile_regions(&rom.data, min_tiles);
    regions.sort_by(|a, b| b.tiles.cmp(&a.tiles));

    let report = serde_json::json!({
        "tool": "scan_tile_patterns",
        "confidence": "speculative",
        "rom": { "crc32": format!("{:08X}", info.crc32), "size": info.size,
                 "version": format!("{:?}", info.version) },
        "min_tiles": min_tiles,
        "regions": regions,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
