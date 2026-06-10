//! ROM analysis: hashes, internal SNES header, version identification.

use serde::{Deserialize, Serialize};

use crate::rom::version::{identify, RomVersion};

/// LoROM internal header location in a headerless image.
/// Confidence: confirmed (standard SNES layout, not game-specific).
pub const LOROM_HEADER_OFFSET: usize = 0x7FC0;
pub const INTERNAL_TITLE_LEN: usize = 21;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InternalHeader {
    /// Internal title, lossy-decoded as ASCII (21 bytes at +0x00).
    pub title: String,
    /// Map mode byte (+0x15): 0x20 = LoROM slow, 0x30 = LoROM fast, ...
    pub map_mode: u8,
    /// Cartridge type byte (+0x16): 0x00 = ROM only.
    pub rom_type: u8,
    /// ROM size byte (+0x17): size = 1 KiB << value (0x0A = 1 MiB).
    pub rom_size: u8,
    /// SRAM size byte (+0x18): 0x00 = none.
    pub sram_size: u8,
    /// Region/destination byte (+0x19): 0x01 = USA.
    pub region: u8,
    /// Checksum complement (+0x1C, little-endian).
    pub checksum_complement: u16,
    /// Checksum (+0x1E, little-endian).
    pub checksum: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RomInfo {
    pub size: usize,
    pub crc32: u32,
    pub sha1_hex: String,
    pub had_copier_header: bool,
    pub internal: Option<InternalHeader>,
    pub version: RomVersion,
}

pub fn crc32_of(data: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(data);
    h.finalize()
}

pub fn sha1_hex_of(data: &[u8]) -> String {
    let mut h = sha1_smol::Sha1::new();
    h.update(data);
    h.digest().to_string()
}

fn parse_internal_header(data: &[u8]) -> Option<InternalHeader> {
    let h = data.get(LOROM_HEADER_OFFSET..LOROM_HEADER_OFFSET + 0x20)?;
    let title = h[..INTERNAL_TITLE_LEN]
        .iter()
        .map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' })
        .collect::<String>();
    Some(InternalHeader {
        title,
        map_mode: h[0x15],
        rom_type: h[0x16],
        rom_size: h[0x17],
        sram_size: h[0x18],
        region: h[0x19],
        checksum_complement: u16::from_le_bytes([h[0x1C], h[0x1D]]),
        checksum: u16::from_le_bytes([h[0x1E], h[0x1F]]),
    })
}

/// Analyze a headerless ROM image.
pub fn analyze_rom(data: &[u8], had_copier_header: bool) -> RomInfo {
    let crc32 = crc32_of(data);
    RomInfo {
        size: data.len(),
        crc32,
        sha1_hex: sha1_hex_of(data),
        had_copier_header,
        internal: parse_internal_header(data),
        version: identify(crc32, data.len()),
    }
}
