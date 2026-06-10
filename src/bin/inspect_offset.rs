//! Inspect bytes at a PC offset or SNES address in a user-supplied ROM:
//! hex dump plus little-endian/pointer interpretations. JSON to stdout.
//!
//! Usage:
//!   cargo run --bin inspect_offset -- <rom-path> --pc 0x12345 [--len 64]
//!   cargo run --bin inspect_offset -- <rom-path> --snes 0x028000 [--len 64]

use anyhow::{bail, Context, Result};
use daffy_editor::rom::info::analyze_rom;
use daffy_editor::rom::loader::load_rom_file;
use daffy_editor::rom::reader::RomReader;
use daffy_editor::snes::lorom::{pc_to_snes, snes_to_pc};

fn parse_num(s: &str) -> Result<usize> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("$")) {
        usize::from_str_radix(hex, 16).context("invalid hex number")
    } else {
        s.parse::<usize>().context("invalid number")
    }
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        bail!("usage: inspect_offset <rom-path> (--pc 0xNNN | --snes 0xNNNNNN) [--len N]");
    };
    let len = arg_value(&args, "--len").map(|v| parse_num(&v)).transpose()?.unwrap_or(64);

    let rom = load_rom_file(path.as_ref())?;
    let info = analyze_rom(&rom.data, rom.had_copier_header);
    let reader = RomReader::new(&rom.data);

    let pc = match (arg_value(&args, "--pc"), arg_value(&args, "--snes")) {
        (Some(v), None) => parse_num(&v)?,
        (None, Some(v)) => snes_to_pc(parse_num(&v)? as u32)?,
        _ => bail!("pass exactly one of --pc or --snes"),
    };

    let bytes = reader.slice(pc, len)?;
    let hex_rows: Vec<String> = bytes
        .chunks(16)
        .enumerate()
        .map(|(row, chunk)| {
            let hex = chunk.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ");
            let ascii: String = chunk
                .iter()
                .map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' })
                .collect();
            format!("{:06X}  {hex:<47}  {ascii}", pc + row * 16)
        })
        .collect();

    let words_le: Vec<String> = (0..len.saturating_sub(1).min(15))
        .step_by(2)
        .filter_map(|i| reader.read_u16_le(pc + i).ok())
        .map(|w| format!("{w:04X}"))
        .collect();

    let report = serde_json::json!({
        "tool": "inspect_offset",
        "rom": { "crc32": format!("{:08X}", info.crc32), "size": info.size,
                 "version": format!("{:?}", info.version) },
        "pc_offset": format!("{pc:#X}"),
        "snes_addr": pc_to_snes(pc).ok().map(|a| format!("${a:06X}")),
        "len": len,
        "hex_dump": hex_rows,
        "first_words_le": words_le,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
