//! Scan a user-supplied ROM for general-purpose DMA transfers by recognizing
//! their 65816 setup code. The reconstructed source addresses are the
//! strongest leads for *where* the game stores graphics and palettes (the
//! transfers whose destination is VRAM or CGRAM). Outputs a JSON report.
//!
//! Usage: cargo run --bin scan_dma -- <rom-path>
//!
//! Everything reported here is SPECULATIVE; see docs/reverse-engineering/.

use anyhow::{bail, Result};
use daffy_editor::codecs::experimental::scan_dma_sources;
use daffy_editor::rom::info::analyze_rom;
use daffy_editor::rom::loader::load_rom_file;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        bail!("usage: scan_dma <rom-path>");
    };

    let rom = load_rom_file(path.as_ref())?;
    let info = analyze_rom(&rom.data, rom.had_copier_header);

    let mut transfers = scan_dma_sources(&rom.data);
    // Group the high-value graphics/palette transfers first, then by source.
    transfers.sort_by_key(|t| (t.kind, t.source_addr, t.code_offset));

    let report = serde_json::json!({
        "tool": "scan_dma",
        "confidence": "speculative",
        "rom": { "crc32": format!("{:08X}", info.crc32), "size": info.size,
                 "version": format!("{:?}", info.version) },
        "transfer_count": transfers.len(),
        "transfers": transfers,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
