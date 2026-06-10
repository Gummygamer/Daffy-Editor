//! ROM file loading and normalization.
//!
//! The user must supply their own legally obtained ROM file; this crate never
//! embeds or distributes ROM data. Copier headers (512 bytes prepended by old
//! backup units) are detected by size modulo 32 KiB and stripped so all
//! internal offsets are headerless.

use std::path::{Path, PathBuf};

use crate::error::RomError;

pub const COPIER_HEADER_SIZE: usize = 512;
/// Smallest plausible SNES ROM: one 32 KiB bank.
pub const MIN_ROM_SIZE: usize = 0x8000;

#[derive(Debug, Clone)]
pub struct LoadedRom {
    /// Headerless ROM bytes (copier header stripped if present).
    pub data: Vec<u8>,
    pub had_copier_header: bool,
    pub source_path: Option<PathBuf>,
}

/// Normalize raw file bytes: detect/strip a copier header, validate size.
pub fn normalize_rom(raw: Vec<u8>) -> Result<LoadedRom, RomError> {
    if raw.len() < MIN_ROM_SIZE {
        return Err(RomError::TooSmall { size: raw.len(), min: MIN_ROM_SIZE });
    }
    match raw.len() % 0x8000 {
        0 => Ok(LoadedRom { data: raw, had_copier_header: false, source_path: None }),
        COPIER_HEADER_SIZE => Ok(LoadedRom {
            data: raw[COPIER_HEADER_SIZE..].to_vec(),
            had_copier_header: true,
            source_path: None,
        }),
        _ => Err(RomError::BadSize { size: raw.len() }),
    }
}

/// Load and normalize a ROM from a user-selected file path.
pub fn load_rom_file(path: &Path) -> Result<LoadedRom, RomError> {
    let raw = std::fs::read(path)?;
    let mut rom = normalize_rom(raw)?;
    rom.source_path = Some(path.to_path_buf());
    Ok(rom)
}
