//! Scan a user-supplied ROM for DMA *setup sites* — clusters of stores to the
//! channel registers ($43xx) — and classify each as `immediate` (the fixed
//! init/HUD uploads) or `parameterized` (the shared helper that uploads the
//! bulk of the game's graphics from tables in ROM). The parameterized sites,
//! their trigger PCs, and their `param_operands` are the leads `scan_dma`
//! cannot reach, because that tool only follows immediate-fed transfers.
//!
//! Usage: cargo run --bin scan_dma_helper -- <rom-path>
//!
//! Everything reported here is SPECULATIVE; see docs/reverse-engineering/.

use anyhow::{bail, Result};
use daffy_editor::codecs::experimental::scan_dma_setup_sites;
use daffy_editor::rom::info::analyze_rom;
use daffy_editor::rom::loader::load_rom_file;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        bail!("usage: scan_dma_helper <rom-path>");
    };

    let rom = load_rom_file(path.as_ref())?;
    let info = analyze_rom(&rom.data, rom.had_copier_header);

    let mut sites = scan_dma_setup_sites(&rom.data);
    // Surface the parameterized, actually-triggering helpers first.
    sites.sort_by_key(|s| {
        (
            s.kind != "parameterized",
            !s.triggers_dma,
            !s.uses_index,
            s.start_offset,
        )
    });

    let parameterized = sites.iter().filter(|s| s.kind == "parameterized").count();
    let report = serde_json::json!({
        "tool": "scan_dma_helper",
        "confidence": "speculative",
        "rom": { "crc32": format!("{:08X}", info.crc32), "size": info.size,
                 "version": format!("{:?}", info.version) },
        "site_count": sites.len(),
        "parameterized_count": parameterized,
        "sites": sites,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
