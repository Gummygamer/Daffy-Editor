//! Load a real level end-to-end with [`daffy_editor::level::load_rom_level`] and
//! print a structural summary — the integration check that the reverse-engineered
//! level format actually decodes the shipping ROM.
//!
//! Output is aggregate statistics only (dimensions, counts, index histograms,
//! palette color values) — never raw ROM tile/map bytes — so it is safe to
//! commit a captured report under docs/reverse-engineering/reports/.
//!
//! Usage: cargo run --bin load_level -- <rom-path> [level-number | all]

use std::collections::BTreeMap;

use anyhow::{bail, Result};
use daffy_editor::level::{level_count, load_rom_level};
use daffy_editor::rom::info::analyze_rom;
use daffy_editor::rom::loader::load_rom_file;

fn summarize(rom: &[u8], n: usize) -> Result<serde_json::Value> {
    let level = load_rom_level(rom, n)?;
    let room = &level.rooms[0];

    // Metatile-index histogram (how many distinct metatiles the map references and
    // whether any exceed the tileset capacity).
    let mut used: BTreeMap<u16, u32> = BTreeMap::new();
    let mut max_idx = 0u16;
    for t in &room.tiles {
        *used.entry(t.metatile).or_default() += 1;
        max_idx = max_idx.max(t.metatile);
    }
    let oob = room.tiles.iter().filter(|t| t.metatile as usize >= level.metatiles.len()).count();
    let nonzero_palette = level.palette.colors.iter().filter(|&&c| c != 0).count();

    Ok(serde_json::json!({
        "level": n,
        "width": room.width,
        "height": room.height,
        "cells": room.tiles.len(),
        "metatiles": level.metatiles.len(),
        "distinct_metatiles_used": used.len(),
        "max_metatile_index": max_idx,
        "metatile_indices_out_of_range": oob,
        "objects": room.objects.len(),
        "object_types": room.objects.iter().map(|o| o.kind).collect::<std::collections::BTreeSet<_>>(),
        "palette_nonzero_colors": nonzero_palette,
        "provenance": match &level.provenance {
            daffy_editor::model::level::Provenance::Confirmed { note } => note.clone(),
            other => format!("{other:?}"),
        },
    }))
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        bail!("usage: load_level <rom-path> [level-number | all]");
    };
    let rom = load_rom_file(path.as_ref())?;
    let info = analyze_rom(&rom.data, rom.had_copier_header);
    let count = level_count(&rom.data);

    let which = args.get(1).map(String::as_str).unwrap_or("0");
    let levels: Vec<serde_json::Value> = if which == "all" {
        (0..count).filter_map(|n| summarize(&rom.data, n).ok()).collect()
    } else {
        let n: usize = which.parse()?;
        vec![summarize(&rom.data, n)?]
    };

    let report = serde_json::json!({
        "tool": "load_level",
        "rom": {
            "crc32": format!("{:08X}", info.crc32),
            "version": format!("{:?}", info.version),
        },
        "level_count": count,
        "levels": levels,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
