//! Render a real ROM level's reconstructed tile graphics and report coherence
//! statistics — the integration check that the static VRAM reconstruction
//! (`level::loader`) + metatile renderer actually produce real pixels.
//!
//! Two outputs:
//!   * stdout JSON: aggregate stats only (counts / coverage) — safe to inspect,
//!     never raw ROM bytes.
//!   * optional `--png <path>`: a rendered image of room 0. This contains decoded
//!     ROM graphics, so it is LOCAL ONLY — write it to /tmp, never commit it.
//!
//! Usage: cargo run --bin render_level -- <rom> [level] [--png <out.png>]

use anyhow::{bail, Result};
use daffy_editor::gfx::table::UploadTarget;
use daffy_editor::level::{load_rom_level, scene_gfx_loads};
use daffy_editor::model::level::Level;
use daffy_editor::rendering::tile_renderer::{render_metatile_rgba, METATILE_RENDER_PX};
use daffy_editor::rom::loader::load_rom_file;

/// How many distinct tile characters the level's *used* metatiles reference, and
/// how many of those land in a populated (non-zero) VRAM tile — the key signal
/// that the BG character base assumption is right.
fn gfx_stats(level: &Level) -> serde_json::Value {
    let room = &level.rooms[0];
    let gfx = &level.gfx;

    // Metatile ids actually placed in the map.
    let mut used_ids = std::collections::BTreeSet::new();
    for t in &room.tiles {
        used_ids.insert(t.metatile);
    }

    let mut chars = std::collections::BTreeSet::new();
    for &id in &used_ids {
        if let Some(m) = level.metatiles.get(id as usize) {
            for &word in &m.tiles {
                chars.insert(word & 0x03FF);
            }
        }
    }
    // A char's 32-byte tile is populated if any of its VRAM bytes are non-zero.
    let char_populated = |c: u16| -> bool {
        let base = (gfx.char_base as usize + c as usize * 16) * 2;
        gfx.vram.get(base..base + 32).map(|s| s.iter().any(|&b| b != 0)).unwrap_or(false)
    };
    let populated = chars.iter().filter(|&&c| char_populated(c)).count();

    let vram_nonzero = gfx.vram.iter().filter(|&&b| b != 0).count();
    let attr_nonzero = gfx.attr.iter().filter(|&&b| b != 0).count();
    let pal_nonzero = level.palette.colors.iter().filter(|&&c| c != 0).count();

    serde_json::json!({
        "vram_bytes": gfx.vram.len(),
        "vram_nonzero_bytes": vram_nonzero,
        "attr_table_len": gfx.attr.len(),
        "attr_nonzero": attr_nonzero,
        "char_base_word": format!("${:04X}", gfx.char_base),
        "distinct_chars_referenced": chars.len(),
        "chars_in_populated_vram": populated,
        "chars_coverage_pct": if chars.is_empty() { 0.0 }
            else { (populated as f64 / chars.len() as f64 * 100.0).round() },
        "palette_nonzero_colors": pal_nonzero,
    })
}

/// Composite room 0 into one RGBA image at 32px / metatile.
fn render_room(level: &Level) -> (usize, usize, Vec<u8>) {
    let room = &level.rooms[0];
    let w = room.width as usize * METATILE_RENDER_PX;
    let h = room.height as usize * METATILE_RENDER_PX;
    let mut out = vec![0u8; w * h * 4];
    for ty in 0..room.height {
        for tx in 0..room.width {
            let id = room.tile(tx, ty).unwrap_or(0);
            let Some(m) = level.metatiles.get(id as usize) else { continue };
            let Some(img) = render_metatile_rgba(&level.gfx, &level.palette, m) else { continue };
            let ox = tx as usize * METATILE_RENDER_PX;
            let oy = ty as usize * METATILE_RENDER_PX;
            for sy in 0..METATILE_RENDER_PX {
                for sx in 0..METATILE_RENDER_PX {
                    let si = (sy * img.width + sx) * 4;
                    let di = ((oy + sy) * w + (ox + sx)) * 4;
                    out[di..di + 4].copy_from_slice(&img.pixels[si..si + 4]);
                }
            }
        }
    }
    (w, h, out)
}

// --- minimal PNG writer (8-bit RGBA, zlib stored deflate) ------------------

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01]; // zlib header (no compression)
    let mut i = 0;
    while i < raw.len() {
        let n = (raw.len() - i).min(0xFFFF);
        let final_block = (i + n) >= raw.len();
        out.push(if final_block { 1 } else { 0 });
        out.extend_from_slice(&(n as u16).to_le_bytes());
        out.extend_from_slice(&(!(n as u16)).to_le_bytes());
        out.extend_from_slice(&raw[i..i + n]);
        i += n;
    }
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut h = crc32fast::Hasher::new();
    h.update(kind);
    h.update(data);
    out.extend_from_slice(&h.finalize().to_be_bytes());
}

fn write_png(path: &str, w: usize, h: usize, rgba: &[u8]) -> Result<()> {
    // Pre-filter: filter-type 0 byte at the start of each scanline.
    let mut filtered = Vec::with_capacity(h * (1 + w * 4));
    for y in 0..h {
        filtered.push(0);
        filtered.extend_from_slice(&rgba[y * w * 4..(y + 1) * w * 4]);
    }
    let mut png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit, RGBA, default
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &zlib_stored(&filtered));
    chunk(&mut png, b"IEND", &[]);
    std::fs::write(path, png)?;
    Ok(())
}

/// Decode the loads a scene issues into a compact JSON list.
fn loads_json(rom: &[u8], level_n: usize) -> Vec<serde_json::Value> {
    scene_gfx_loads(rom, level_n)
        .iter()
        .map(|e| {
            let target = match e.upload() {
                UploadTarget::Vram { word_addr, size } => format!("VRAM word ${word_addr:04X} size ${size:04X}"),
                UploadTarget::Cgram { addr, size } => format!("CGRAM ${addr:02X} size ${size:04X}"),
                UploadTarget::Wram { dest } => format!("WRAM ${dest:06X}"),
                UploadTarget::Unknown { mode } => format!("mode {mode}?"),
            };
            serde_json::json!({ "id": e.index, "mode": e.mode, "source": format!("${:06X}", e.source), "target": target })
        })
        .collect()
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        bail!("usage: render_level <rom> [level | all] [--png <out.png>]");
    };
    let which = args.get(1).map(String::as_str).filter(|s| !s.starts_with("--")).unwrap_or("0");
    let png_out = args.iter().position(|a| a == "--png").and_then(|i| args.get(i + 1)).cloned();

    let rom = load_rom_file(path.as_ref())?;

    // `all` -> compact per-level coherence report (aggregate stats; committable).
    if which == "all" {
        let count = daffy_editor::level::level_count(&rom.data);
        let levels: Vec<serde_json::Value> = (0..count)
            .filter_map(|n| load_rom_level(&rom.data, n).ok().map(|lvl| {
                serde_json::json!({ "level": n, "gfx_loads": loads_json(&rom.data, n).len(), "graphics": gfx_stats(&lvl) })
            }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "tool": "render_level",
            "note": "Static reconstruction of each scene's VRAM/palette from its mode-0/1 graphics \
                     loads; coverage = referenced tile chars landing in populated VRAM at char_base.",
            "rom": format!("CRC32 {:08X}", daffy_editor::rom::info::analyze_rom(&rom.data, rom.had_copier_header).crc32),
            "level_count": count,
            "levels": levels,
        }))?);
        return Ok(());
    }

    let level_n: usize = which.parse()?;
    let level = load_rom_level(&rom.data, level_n)?;
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "tool": "render_level",
        "level": level_n,
        "name": level.name,
        "gfx_loads": loads_json(&rom.data, level_n),
        "graphics": gfx_stats(&level),
    }))?);

    if let Some(out) = png_out {
        let (w, h, rgba) = render_room(&level);
        write_png(&out, w, h, &rgba)?;
        eprintln!("wrote {w}x{h} PNG -> {out} (LOCAL ONLY — contains ROM graphics)");
    }
    Ok(())
}
