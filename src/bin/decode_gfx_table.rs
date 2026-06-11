//! End-to-end pipeline check: parse the graphics descriptor table
//! (`src/gfx/table.rs`), then run the [`gfx_rle`](daffy_editor::codecs::gfx_rle)
//! decompressor on the ROM bytes at **every** entry's source pointer. Reports
//! each entry's decoded/consumed byte counts and a final PASS/FAIL (PASS = all
//! 159 sources decode without the stream running dry).
//!
//! This reads the user's own (gitignored) ROM and prints only addresses and
//! byte *counts* — never decoded ROM bytes — so its summary line is safe to
//! quote, though we don't commit a report from it (counts are derived data).
//!
//! Usage: cargo run --bin decode_gfx_table -- <rom-path> [--verbose]

use anyhow::{bail, Result};
use daffy_editor::codecs::gfx_rle::decompress;
use daffy_editor::gfx::table::parse_game_table;
use daffy_editor::rom::loader::load_rom_file;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        bail!("usage: decode_gfx_table <rom-path> [--verbose]");
    };
    let verbose = args.iter().any(|a| a == "--verbose");

    let rom = load_rom_file(path.as_ref())?;
    let table = parse_game_table(&rom.data)?;

    let mut failures = 0usize;
    let mut total_decoded = 0usize;
    for e in &table {
        let src_pc = match e.source_pc() {
            Ok(pc) => pc,
            Err(err) => {
                println!("entry {:3}: source {:06X} not a ROM address: {err}", e.index, e.source);
                failures += 1;
                continue;
            }
        };
        let Some(stream) = rom.data.get(src_pc..) else {
            println!("entry {:3}: source PC {src_pc:#08X} past end of ROM", e.index);
            failures += 1;
            continue;
        };
        match decompress(stream) {
            Ok(d) => {
                total_decoded += d.data.len();
                if verbose {
                    println!(
                        "entry {:3}: mode {} src {:06X} -> decoded {:5} bytes (consumed {})",
                        e.index, e.mode, e.source, d.data.len(), d.bytes_consumed
                    );
                }
            }
            Err(err) => {
                println!("entry {:3}: src {:06X} DECODE ERROR: {err}", e.index, e.source);
                failures += 1;
            }
        }
    }

    println!(
        "decode_gfx_table: {} entries, {} ok, {} failed, {} total decoded bytes",
        table.len(),
        table.len() - failures,
        failures,
        total_decoded
    );
    if failures == 0 {
        println!("RESULT: PASS — every descriptor source decodes cleanly");
        Ok(())
    } else {
        bail!("RESULT: FAIL — {failures} descriptor sources failed to decode");
    }
}
