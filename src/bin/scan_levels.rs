//! Dump the **level table** — every scene-setup routine's recovered level-data
//! pointer block — from a user-supplied ROM.
//!
//! Each scene routine fills a fixed block of variables (`$D3/$D5/$D9/$DB`,
//! dimensions `$DD/$DF`, secondary `$1EF8/$1EF4/$1EFA`) with its data pointers.
//! The pointer values + dimensions are *confirmed* (live Mesen2 trace matched and
//! all 21 routines share the block shape); the per-region *semantics* are likely.
//! See docs/reverse-engineering/level-format.md.
//!
//! The report contains only addresses/dimensions — never ROM bytes — so it is
//! safe to commit under docs/reverse-engineering/reports/.
//!
//! Usage: cargo run --bin scan_levels -- <rom-path>

use anyhow::{bail, Result};
use daffy_editor::level::scan_levels;
use daffy_editor::rom::info::analyze_rom;
use daffy_editor::rom::loader::load_rom_file;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        bail!("usage: scan_levels <rom-path>");
    };

    let rom = load_rom_file(path.as_ref())?;
    let info = analyze_rom(&rom.data, rom.had_copier_header);
    let levels = scan_levels(&rom.data);

    let entries: Vec<_> = levels
        .iter()
        .enumerate()
        .map(|(i, l)| {
            serde_json::json!({
                "index": i,
                "setup_routine": format!("{:06X}", l.anchor_snes),
                "primary_bank": format!("{:02X}", l.primary_bank),
                "map": format!("{:06X}", l.map_ptr()),
                "tileset": format!("{:06X}", l.tileset_ptr()),
                "attr_map": format!("{:06X}", l.attr_ptr()),
                "width": l.width,
                "height": l.height,
                "cells": l.width as u32 * l.height as u32,
                "map_bytes": l.map_bytes(),
                "secondary_bank": format!("{:02X}", l.secondary_bank),
                "entity_list": format!("{:06X}", l.entity_ptr()),
                "handler_table": format!("{:02X}{:04X}", l.secondary_bank, l.handler_off),
            })
        })
        .collect();

    let report = serde_json::json!({
        "tool": "scan_levels",
        "confidence": "pointer block + dimensions confirmed (live trace + 21x consistent); region semantics likely",
        "rom": {
            "crc32": format!("{:08X}", info.crc32),
            "size": info.size,
            "version": format!("{:?}", info.version),
        },
        "anchor": "STA $1EF8 (8D F8 1E) inside each scene-setup routine",
        "level_count": levels.len(),
        "levels": entries,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
