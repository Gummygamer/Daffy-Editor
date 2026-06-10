//! SNES LoROM <-> PC file offset conversion tests.

use daffy_editor::error::RomError;
use daffy_editor::snes::lorom::{pc_to_snes, snes_to_pc};

#[test]
fn canonical_lorom_mappings() {
    assert_eq!(snes_to_pc(0x00_8000).unwrap(), 0x000000);
    assert_eq!(snes_to_pc(0x00_FFFF).unwrap(), 0x007FFF);
    assert_eq!(snes_to_pc(0x01_8000).unwrap(), 0x008000);
    assert_eq!(snes_to_pc(0x1F_FFFF).unwrap(), 0x0FFFFF); // last byte of a 1 MiB ROM
    assert_eq!(snes_to_pc(0x20_8000).unwrap(), 0x100000);
}

#[test]
fn fast_rom_mirror_banks_map_to_same_offsets() {
    assert_eq!(snes_to_pc(0x80_8000).unwrap(), 0x000000);
    assert_eq!(snes_to_pc(0x9F_FFFF).unwrap(), 0x0FFFFF);
    // $FE/$FF are ROM in LoROM (only $7E/$7F are WRAM)
    assert_eq!(snes_to_pc(0xFF_FFFF).unwrap(), 0x3FFFFF);
}

#[test]
fn lower_half_of_bank_is_not_rom() {
    for addr in [0x00_0000u32, 0x00_7FFF, 0x10_4242, 0x80_0000] {
        let err = snes_to_pc(addr).unwrap_err();
        assert!(
            matches!(err, RomError::InvalidSnesAddress { .. }),
            "addr {addr:#08x} got {err:?}"
        );
    }
}

#[test]
fn wram_banks_are_rejected() {
    for addr in [0x7E_0000u32, 0x7E_8000, 0x7F_FFFF] {
        let err = snes_to_pc(addr).unwrap_err();
        assert!(
            matches!(err, RomError::InvalidSnesAddress { .. }),
            "addr {addr:#08x} got {err:?}"
        );
    }
}

#[test]
fn address_above_24_bits_is_rejected() {
    assert!(snes_to_pc(0x0100_0000).is_err());
}

#[test]
fn pc_to_snes_canonical_values() {
    assert_eq!(pc_to_snes(0x000000).unwrap(), 0x00_8000);
    assert_eq!(pc_to_snes(0x007FFF).unwrap(), 0x00_FFFF);
    assert_eq!(pc_to_snes(0x008000).unwrap(), 0x01_8000);
    assert_eq!(pc_to_snes(0x0FFFFF).unwrap(), 0x1F_FFFF);
}

#[test]
fn pc_to_snes_round_trips_through_snes_to_pc() {
    for pc in [0usize, 1, 0x7FFF, 0x8000, 0x12345, 0x0FFFFF, 0x3DFFFF] {
        let snes = pc_to_snes(pc).unwrap();
        assert_eq!(snes_to_pc(snes).unwrap(), pc, "pc {pc:#x} snes {snes:#08x}");
    }
}

#[test]
fn pc_beyond_lorom_capacity_is_rejected() {
    // LoROM tops out below banks $7E/$7F: 0x3E * 0x8000 bytes addressable
    assert!(pc_to_snes(0x3F_0000).is_err());
    assert!(pc_to_snes(usize::MAX).is_err());
}
