//! Find repeated fixed-size byte blocks in a user-supplied ROM (structure
//! arrays, padding, blank tiles). Outputs a JSON report to stdout.
//!
//! Usage: cargo run --bin scan_repeated_blocks -- <rom-path> [--block-len N] [--min-count N]
//!
//! Everything reported here is SPECULATIVE; see docs/reverse-engineering/.

use anyhow::{bail, Context, Result};
use daffy_editor::codecs::experimental::scan_repeated_blocks;
use daffy_editor::rom::info::analyze_rom;
use daffy_editor::rom::loader::load_rom_file;

fn arg_num(args: &[String], flag: &str, default: usize) -> Result<usize> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(|v| v.parse::<usize>())
        .transpose()
        .with_context(|| format!("{flag} expects a number"))
        .map(|v| v.unwrap_or(default))
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        bail!("usage: scan_repeated_blocks <rom-path> [--block-len N] [--min-count N]");
    };
    let block_len = arg_num(&args, "--block-len", 32)?;
    let min_count = arg_num(&args, "--min-count", 4)?;

    let rom = load_rom_file(path.as_ref())?;
    let info = analyze_rom(&rom.data, rom.had_copier_header);
    let mut blocks = scan_repeated_blocks(&rom.data, block_len, min_count);
    blocks.truncate(200);

    let report = serde_json::json!({
        "tool": "scan_repeated_blocks",
        "confidence": "speculative",
        "rom": { "crc32": format!("{:08X}", info.crc32), "size": info.size,
                 "version": format!("{:?}", info.version) },
        "block_len": block_len,
        "min_count": min_count,
        "blocks": blocks,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
