//! Linear 65816 disassembler for a user-supplied ROM. Built to read the
//! graphics decompressor at `$82:8549` (docs/reverse-engineering/graphics-pipeline.md)
//! but works on any routine.
//!
//! Usage:
//!   cargo run --bin disasm -- <rom-path> --snes 0x828549 --end 0x828655
//!   cargo run --bin disasm -- <rom-path> --pc 0x10549 --len 0x10C
//!
//! Flags:
//!   --snes 0xBBAAAA | --pc 0xNNN   start (one required)
//!   --end 0xBBAAAA  | --len N      extent (one required)
//!   --m8 / --m16                   initial accumulator width (default --m8)
//!   --x8 / --x16                   initial index width (default --x8)
//!   --json                         emit a JSON report instead of a listing
//!
//! The decoder honours REP/SEP as it sweeps, so the only widths that matter are
//! the ones in effect at the start address. Output interpretation is a game
//! finding; record it under docs/reverse-engineering/ with a confidence label.

use anyhow::{bail, Context, Result};
use daffy_editor::rom::info::analyze_rom;
use daffy_editor::rom::loader::load_rom_file;
use daffy_editor::snes::disasm::disassemble;
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

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        bail!("usage: disasm <rom-path> (--snes 0xBBAAAA | --pc 0xNNN) (--end 0xBBAAAA | --len N) [--m8|--m16] [--x8|--x16] [--json]");
    };

    let rom = load_rom_file(path.as_ref())?;
    let info = analyze_rom(&rom.data, rom.had_copier_header);

    let (start_pc, start_addr) = match (arg_value(&args, "--pc"), arg_value(&args, "--snes")) {
        (Some(v), None) => {
            let pc = parse_num(&v)?;
            (pc, pc_to_snes(pc)?)
        }
        (None, Some(v)) => {
            let addr = parse_num(&v)? as u32;
            (snes_to_pc(addr)?, addr)
        }
        _ => bail!("pass exactly one of --pc or --snes"),
    };

    let count = match (arg_value(&args, "--len"), arg_value(&args, "--end")) {
        (Some(v), None) => parse_num(&v)?,
        (None, Some(v)) => {
            let end_addr = parse_num(&v)? as u32;
            let end_pc = snes_to_pc(end_addr)?;
            if end_pc < start_pc {
                bail!("--end is before the start address");
            }
            end_pc - start_pc + 1 // inclusive of the opcode at --end
        }
        _ => bail!("pass exactly one of --len or --end"),
    };

    let m8 = !has_flag(&args, "--m16");
    let x8 = !has_flag(&args, "--x16");

    let instrs = disassemble(&rom.data, start_pc, start_addr, count, m8, x8);

    if has_flag(&args, "--json") {
        let lines: Vec<serde_json::Value> = instrs
            .iter()
            .map(|i| {
                serde_json::json!({
                    "addr": format!("${:06X}", i.addr),
                    "bytes": i.bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>(),
                    "mnemonic": i.mnemonic,
                    "operand": i.operand,
                    "m8": i.m8,
                    "x8": i.x8,
                })
            })
            .collect();
        let report = serde_json::json!({
            "tool": "disasm",
            "confidence": "instruction-decode confirmed; interpretation per docs",
            "rom": { "crc32": format!("{:08X}", info.crc32), "size": info.size,
                     "version": format!("{:?}", info.version) },
            "start": format!("${start_addr:06X}"),
            "count_bytes": count,
            "instruction_count": instrs.len(),
            "instructions": lines,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for i in &instrs {
            println!("{}", i.listing());
        }
    }
    Ok(())
}
