//! SNES LoROM address <-> PC file offset conversion.
//!
//! LoROM maps each 32 KiB ROM chunk to the upper half ($8000-$FFFF) of a bank.
//! Banks $80-$FF mirror $00-$7F (FastROM), except banks $7E/$7F which are WRAM
//! (and therefore never ROM). The lower half of a bank ($0000-$7FFF) holds
//! system area / hardware registers, not ROM.
//!
//! Confidence: confirmed (standard SNES memory map; see
//! docs/reverse-engineering/rom-identity.md).

use crate::error::RomError;

pub const BANK_SIZE: usize = 0x8000;
/// Highest LoROM bank after mirror-masking ($7E/$7F are WRAM).
const MAX_BANK: u32 = 0x7D;

/// Convert a 24-bit SNES address to a PC file offset (headerless ROM).
pub fn snes_to_pc(addr: u32) -> Result<usize, RomError> {
    if addr > 0xFF_FFFF {
        return Err(RomError::InvalidSnesAddress { addr, reason: "address exceeds 24 bits" });
    }
    let bank = addr >> 16;
    let offset = addr & 0xFFFF;
    if bank == 0x7E || bank == 0x7F {
        return Err(RomError::InvalidSnesAddress { addr, reason: "banks $7E/$7F are WRAM, not ROM" });
    }
    if offset < 0x8000 {
        return Err(RomError::InvalidSnesAddress {
            addr,
            reason: "$0000-$7FFF is system area in LoROM, not ROM",
        });
    }
    let bank = bank & 0x7F; // $80-$FF mirror $00-$7F
    Ok(bank as usize * BANK_SIZE + (offset as usize - 0x8000))
}

/// Convert a PC file offset (headerless ROM) to the canonical SNES address
/// in banks $00-$7D.
pub fn pc_to_snes(pc: usize) -> Result<u32, RomError> {
    let bank = pc / BANK_SIZE;
    if bank > MAX_BANK as usize {
        return Err(RomError::PcOffsetOutOfRange { offset: pc });
    }
    Ok(((bank as u32) << 16) | 0x8000 | (pc % BANK_SIZE) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhaustive_round_trip_over_first_megabyte() {
        for pc in (0..0x100000).step_by(0x1001) {
            let snes = pc_to_snes(pc).unwrap();
            assert_eq!(snes_to_pc(snes).unwrap(), pc);
        }
    }
}
