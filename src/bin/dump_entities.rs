//! Research: dump the raw entity/object spawn records (22-byte stride) for every
//! scene, plus a per-byte-column variance analysis, to figure out which record
//! offset is the *type* and which are coordinates/params.
//!
//! Output includes raw record bytes — DO NOT commit its output.
//!
//! Usage: cargo run --bin dump_entities -- <rom-path> [level-number | all]

use anyhow::{bail, Result};
use daffy_editor::level::scan_levels;
use daffy_editor::rom::loader::load_rom_file;
use daffy_editor::snes::lorom::snes_to_pc;

const REC: usize = 0x16;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        bail!("usage: dump_entities <rom-path> [level-number | all]");
    };
    let rom = load_rom_file(path.as_ref())?;
    let blocks = scan_levels(&rom.data);

    let which = args.get(1).map(String::as_str).unwrap_or("all");
    let sel: Vec<usize> = if which == "all" {
        (0..blocks.len()).collect()
    } else {
        vec![which.parse()?]
    };

    // Per-column accumulators across ALL records of ALL selected levels.
    let mut col_min = [255u8; REC];
    let mut col_max = [0u8; REC];
    let mut col_vals: Vec<std::collections::BTreeSet<u8>> = vec![Default::default(); REC];
    let mut total = 0usize;

    for &n in &sel {
        let Some(b) = blocks.get(n) else { continue };
        let count = b.entity_count as usize;
        let Ok(base) = snes_to_pc(b.entity_ptr()) else { continue };
        println!(
            "\n=== level {n}: entity_ptr={:06X} count={count} (off={:04X} bank={:02X}) ===",
            b.entity_ptr(),
            b.entity_off,
            b.secondary_bank
        );
        print!("  rec  ");
        for c in 0..REC {
            print!("{c:02X} ");
        }
        println!();
        for i in 0..count {
            let off = base + i * REC;
            let Some(rec) = rom.data.get(off..off + REC) else { break };
            print!("  #{i:<3} ");
            for (c, &v) in rec.iter().enumerate() {
                print!("{v:02X} ");
                col_min[c] = col_min[c].min(v);
                col_max[c] = col_max[c].max(v);
                col_vals[c].insert(v);
            }
            println!();
            total += 1;
        }
    }

    println!("\n=== per-column analysis over {total} records ===");
    println!(" off  min  max  distinct  (interpretation hint)");
    for c in 0..REC {
        let n = col_vals[c].len();
        let hint = if n == 1 {
            "constant"
        } else if n <= 24 && col_max[c] < 0x40 {
            "<-- small enum (TYPE candidate)"
        } else {
            "wide/coordinate"
        };
        println!(
            "  {c:02X}   {:02X}   {:02X}    {n:>3}      {hint}",
            col_min[c], col_max[c]
        );
    }
    Ok(())
}
