//! Round-trip the graphics decompressor against real ROM data.
//!
//! Reads a hex byte dump (the WRAM `$7F:C000` staging area captured live by
//! `tools/mesen/roundtrip_decompressor.lua`) from stdin, runs
//! [`daffy_editor::codecs::gfx_rle::decompress`] on the ROM bytes at the
//! captured source address, and checks the two match byte-for-byte.
//!
//! This is a developer verification tool: it reads the user's own (gitignored)
//! ROM and a transient dump, and prints only a PASS/FAIL verdict — no ROM bytes
//! are emitted or committed. See `tools/roundtrip.sh` for the glue.
//!
//! Usage:
//!   <capture> | cargo run --bin roundtrip_gfx -- <rom-path> --src 0x928000

use anyhow::{bail, Context, Result};
use daffy_editor::codecs::gfx_rle::decompress;
use daffy_editor::rom::loader::load_rom_file;
use daffy_editor::snes::lorom::snes_to_pc;
use std::io::Read;

fn parse_num(s: &str) -> Result<u32> {
    let s = s.trim();
    let v = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("$")) {
        u32::from_str_radix(hex, 16)
    } else {
        s.parse::<u32>()
    };
    v.with_context(|| format!("invalid number: {s:?}"))
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Parse a whitespace-separated hex byte stream (ignores any non-hex tokens).
fn parse_hex_bytes(text: &str) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for tok in text.split_whitespace() {
        // Accept either "AABBCC.." packed words or single "AA" tokens.
        if tok.len() % 2 != 0 || !tok.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        for pair in tok.as_bytes().chunks(2) {
            let s = std::str::from_utf8(pair).unwrap();
            out.push(u8::from_str_radix(s, 16).context("bad hex byte")?);
        }
    }
    Ok(out)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        bail!("usage: roundtrip_gfx <rom-path> --src 0xNNNNNN  (expected hex on stdin)");
    };
    let src = arg_value(&args, "--src").context("missing --src <snes-addr>")?;
    let src_pc = snes_to_pc(parse_num(&src)?)?;

    let mut expected_text = String::new();
    std::io::stdin().read_to_string(&mut expected_text)?;
    let expected = parse_hex_bytes(&expected_text)?;
    if expected.is_empty() {
        bail!("no expected bytes on stdin (did the capture run?)");
    }

    let rom = load_rom_file(path.as_ref())?;
    let stream = rom
        .data
        .get(src_pc..)
        .with_context(|| format!("source PC {src_pc:#x} past end of ROM"))?;

    let decoded = decompress(stream).context("decoder failed on ROM stream")?;
    let n = decoded.data.len();

    println!("source        : {src} (PC {src_pc:#08X})");
    println!("decoded bytes : {n}");
    println!(
        "consumed      : {} compressed bytes",
        decoded.bytes_consumed
    );
    println!("dump bytes    : {}", expected.len());

    if expected.len() < n {
        bail!(
            "dump ({}) shorter than decoded output ({n}); increase DUMP_LEN",
            expected.len()
        );
    }

    if decoded.data == expected[..n] {
        println!("RESULT        : PASS — decoder output matches WRAM staging dump");
        Ok(())
    } else {
        let first = (0..n).find(|&i| decoded.data[i] != expected[i]);
        if let Some(i) = first {
            println!(
                "first mismatch: offset {i}: decoded {:02X} != dump {:02X}",
                decoded.data[i], expected[i]
            );
        }
        bail!("RESULT: FAIL — decoder output diverges from the WRAM staging dump");
    }
}
