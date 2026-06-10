//! Supported ROM version identification.
//!
//! Identification uses CRC32 of the headerless image plus exact size, so a
//! coincidental hash on a wrong-sized file is never accepted. Hash source:
//! No-Intro database entry for the USA release; see
//! docs/reverse-engineering/rom-identity.md (confidence: confirmed).

use serde::{Deserialize, Serialize};

/// CRC32 of the headerless USA ROM, "Daffy Duck - The Marvin Missions (USA)".
pub const DAFFY_USA_CRC32: u32 = 0x5F02_A044;
/// 1 MiB (8 Mbit), LoROM, no SRAM.
pub const DAFFY_USA_ROM_SIZE: usize = 0x10_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RomVersion {
    DaffyDuckMarvinMissionsUsa,
    Unknown,
}

impl RomVersion {
    pub fn is_supported(self) -> bool {
        self != RomVersion::Unknown
    }

    pub fn display_name(self) -> &'static str {
        match self {
            RomVersion::DaffyDuckMarvinMissionsUsa => {
                "Daffy Duck: The Marvin Missions (USA) — supported"
            }
            RomVersion::Unknown => "Unknown ROM — unsupported, read-only inspection only",
        }
    }
}

/// Identify a headerless ROM image by CRC32 and exact size.
pub fn identify(crc32: u32, size: usize) -> RomVersion {
    match (crc32, size) {
        (DAFFY_USA_CRC32, DAFFY_USA_ROM_SIZE) => RomVersion::DaffyDuckMarvinMissionsUsa,
        _ => RomVersion::Unknown,
    }
}
