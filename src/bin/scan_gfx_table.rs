//! Dump the **graphics descriptor table** — the graphics-id → compressed-source
//! index at SNES `$82:8000` (PC `0x10000`) — from a user-supplied ROM.
//!
//! Each of the 159 records is `mode(1) source(3) params(4)`. The 24-bit source
//! pointer is *confirmed* (a live loader trace matched 36 distinct calls; the
//! decompressor reproduces real graphics from it). The mode byte and params are
//! also *confirmed* from the loader wrapper `$80:FC26`: mode 0 = VRAM DMA
//! (`params` = `$2116` word + size), mode 1 = CGRAM/palette DMA (`params` =
//! `$2121` addr + size), mode 2 = decompress straight to WRAM (`params` = dest).
//! See docs/reverse-engineering/graphics-table.md.
//!
//! The report contains only addresses/modes/banks — never ROM bytes — so it is
//! safe to commit under docs/reverse-engineering/reports/.
//!
//! Usage: cargo run --bin scan_gfx_table -- <rom-path>

use std::collections::BTreeMap;

use anyhow::{bail, Result};
use daffy_editor::gfx::table::{parse_game_table, UploadTarget, ENTRY_COUNT, TABLE_PC, TABLE_SNES};
use daffy_editor::rom::info::analyze_rom;
use daffy_editor::rom::loader::load_rom_file;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        bail!("usage: scan_gfx_table <rom-path>");
    };

    let rom = load_rom_file(path.as_ref())?;
    let info = analyze_rom(&rom.data, rom.had_copier_header);
    let table = parse_game_table(&rom.data)?;

    let mut by_bank: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_mode: BTreeMap<u8, usize> = BTreeMap::new();
    let mut implausible = Vec::new();

    let entries: Vec<_> = table
        .iter()
        .map(|e| {
            *by_bank.entry(format!("{:02X}", e.source_bank())).or_default() += 1;
            *by_mode.entry(e.mode).or_default() += 1;
            if !e.source_is_plausible() {
                implausible.push(e.index);
            }
            let upload = match e.upload() {
                UploadTarget::Vram { word_addr, size } => serde_json::json!({
                    "kind": "vram", "vram_word": format!("{:04X}", word_addr), "size": size,
                }),
                UploadTarget::Cgram { addr, size } => serde_json::json!({
                    "kind": "cgram", "cgram_addr": format!("{:02X}", addr), "size": size,
                }),
                UploadTarget::Wram { dest } => serde_json::json!({
                    "kind": "wram", "dest": format!("{:06X}", dest),
                }),
                UploadTarget::Unknown { mode } => serde_json::json!({
                    "kind": "unknown", "mode": mode,
                }),
            };
            serde_json::json!({
                "index": e.index,
                "mode": e.mode,
                "source": format!("{:06X}", e.source),
                "source_pc": e.source_pc().ok().map(|pc| format!("{:06X}", pc)),
                "dest_wram": e.dest_wram().map(|d| format!("{:06X}", d)),
                "params": e.params.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(""),
                "upload": upload,
                "source_plausible": e.source_is_plausible(),
            })
        })
        .collect();

    let report = serde_json::json!({
        "tool": "scan_gfx_table",
        "confidence": "source pointers confirmed (live trace + round-trip); modes/params confirmed from loader wrapper $80:FC26",
        "rom": {
            "crc32": format!("{:08X}", info.crc32),
            "size": info.size,
            "version": format!("{:?}", info.version),
        },
        "table_snes": format!("{:06X}", TABLE_SNES),
        "table_pc": format!("{:06X}", TABLE_PC),
        "record_size": 8,
        "entry_count": ENTRY_COUNT,
        "mode_histogram": by_mode.iter().map(|(m, c)| (m.to_string(), c)).collect::<BTreeMap<_, _>>(),
        "source_bank_histogram": by_bank,
        "implausible_source_indices": implausible,
        "entries": entries,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
