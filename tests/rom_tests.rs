//! ROM loading, normalization, hashing and version identification tests.
//! All fixtures are synthetic; no copyrighted ROM data is used.

use std::io::Write;

use daffy_editor::error::RomError;
use daffy_editor::rom::info::{analyze_rom, crc32_of, sha1_hex_of};
use daffy_editor::rom::loader::{load_rom_file, normalize_rom, COPIER_HEADER_SIZE, MIN_ROM_SIZE};
use daffy_editor::rom::reader::RomReader;
use daffy_editor::rom::version::{identify, RomVersion, DAFFY_USA_CRC32, DAFFY_USA_ROM_SIZE};
use daffy_editor::rom::writer::RomImage;

/// Deterministic synthetic ROM-like buffer (NOT real game data).
fn synth_rom(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 251) as u8).collect()
}

/// Synthetic LoROM image with a plausible internal header at 0x7FC0.
fn synth_lorom(size: usize) -> Vec<u8> {
    let mut d = synth_rom(size);
    let title = b"SYNTHETIC TEST IMAGE "; // 21 bytes
    d[0x7FC0..0x7FC0 + 21].copy_from_slice(title);
    d[0x7FD5] = 0x20; // map mode: LoROM, slow
    d[0x7FD6] = 0x00; // ROM type: ROM only
    d[0x7FD7] = 0x0A; // ROM size: 1 MiB
    d[0x7FD8] = 0x00; // SRAM: none
    d[0x7FD9] = 0x01; // region: USA
    d[0x7FDC] = 0x34; // checksum complement (arbitrary, paired below)
    d[0x7FDD] = 0x12;
    d[0x7FDE] = 0xCB; // checksum
    d[0x7FDF] = 0xED;
    d
}

#[test]
fn unheadered_rom_loads_unchanged() {
    let raw = synth_rom(0x100000);
    let loaded = normalize_rom(raw.clone()).unwrap();
    assert!(!loaded.had_copier_header);
    assert_eq!(loaded.data.len(), 0x100000);
    assert_eq!(loaded.data, raw);
}

#[test]
fn headered_rom_is_stripped() {
    let body = synth_rom(0x100000);
    let mut raw = vec![0u8; COPIER_HEADER_SIZE];
    raw.extend_from_slice(&body);
    let loaded = normalize_rom(raw).unwrap();
    assert!(loaded.had_copier_header);
    assert_eq!(loaded.data, body);
}

#[test]
fn minimum_size_rom_loads() {
    let loaded = normalize_rom(synth_rom(MIN_ROM_SIZE)).unwrap();
    assert!(!loaded.had_copier_header);
    assert_eq!(loaded.data.len(), MIN_ROM_SIZE);
}

#[test]
fn headered_minimum_size_rom_loads() {
    let mut raw = vec![0u8; COPIER_HEADER_SIZE];
    raw.extend_from_slice(&synth_rom(MIN_ROM_SIZE));
    let loaded = normalize_rom(raw).unwrap();
    assert!(loaded.had_copier_header);
    assert_eq!(loaded.data.len(), MIN_ROM_SIZE);
}

#[test]
fn too_small_rom_is_rejected() {
    let err = normalize_rom(vec![0u8; 100]).unwrap_err();
    assert!(matches!(err, RomError::TooSmall { .. }), "got {err:?}");
}

#[test]
fn odd_size_rom_is_rejected() {
    let err = normalize_rom(synth_rom(0x100000 + 17)).unwrap_err();
    assert!(matches!(err, RomError::BadSize { .. }), "got {err:?}");
}

#[test]
fn rom_file_round_trips_from_disk() {
    let body = synth_rom(MIN_ROM_SIZE);
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(&body).unwrap();
    f.flush().unwrap();
    let loaded = load_rom_file(f.path()).unwrap();
    assert_eq!(loaded.data, body);
    assert_eq!(loaded.source_path.as_deref(), Some(f.path()));
}

#[test]
fn crc32_matches_known_check_value() {
    // Standard CRC-32 check value for "123456789".
    assert_eq!(crc32_of(b"123456789"), 0xCBF4_3926);
}

#[test]
fn sha1_matches_known_check_value() {
    // FIPS 180-1 test vector for "abc".
    assert_eq!(sha1_hex_of(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
}

#[test]
fn known_crc_and_size_identify_daffy_usa() {
    assert_eq!(
        identify(DAFFY_USA_CRC32, DAFFY_USA_ROM_SIZE),
        RomVersion::DaffyDuckMarvinMissionsUsa
    );
    assert_eq!(DAFFY_USA_CRC32, 0x5F02_A044);
    assert_eq!(DAFFY_USA_ROM_SIZE, 0x100000);
}

#[test]
fn known_crc_with_wrong_size_is_unknown() {
    assert_eq!(identify(DAFFY_USA_CRC32, 0x80000), RomVersion::Unknown);
}

#[test]
fn unknown_crc_is_unknown() {
    assert_eq!(identify(0xDEAD_BEEF, DAFFY_USA_ROM_SIZE), RomVersion::Unknown);
}

#[test]
fn analyze_reads_internal_header_fields() {
    let data = synth_lorom(0x100000);
    let info = analyze_rom(&data, false);
    assert_eq!(info.size, 0x100000);
    assert_eq!(info.crc32, crc32_of(&data));
    assert_eq!(info.sha1_hex, sha1_hex_of(&data));
    assert_eq!(info.version, RomVersion::Unknown); // synthetic data is never "supported"
    let h = info.internal.expect("internal header should parse");
    assert_eq!(h.title.trim_end(), "SYNTHETIC TEST IMAGE");
    assert_eq!(h.map_mode, 0x20);
    assert_eq!(h.rom_size, 0x0A);
    assert_eq!(h.sram_size, 0x00);
    assert_eq!(h.checksum_complement, 0x1234);
    assert_eq!(h.checksum, 0xEDCB);
}

#[test]
fn analyze_tiny_buffer_has_no_internal_header() {
    let info = analyze_rom(&[0u8; 16], false);
    assert!(info.internal.is_none());
}

#[test]
fn reader_reads_little_endian_values() {
    let data = vec![0x11, 0x22, 0x33, 0x44];
    let r = RomReader::new(&data);
    assert_eq!(r.read_u8(0).unwrap(), 0x11);
    assert_eq!(r.read_u16_le(0).unwrap(), 0x2211);
    assert_eq!(r.read_u16_le(2).unwrap(), 0x4433);
    assert_eq!(r.read_u24_le(1).unwrap(), 0x443322);
    assert_eq!(r.slice(1, 2).unwrap(), &[0x22, 0x33]);
}

#[test]
fn reader_rejects_out_of_range_reads() {
    let data = vec![0u8; 4];
    let r = RomReader::new(&data);
    assert!(matches!(r.read_u8(4), Err(RomError::OutOfRange { .. })));
    assert!(matches!(r.read_u16_le(3), Err(RomError::OutOfRange { .. })));
    assert!(matches!(r.read_u24_le(2), Err(RomError::OutOfRange { .. })));
    assert!(matches!(r.slice(0, 5), Err(RomError::OutOfRange { .. })));
    // overflow bait: offset + len wraps usize
    assert!(matches!(r.slice(usize::MAX, 2), Err(RomError::OutOfRange { .. })));
}

#[test]
fn reader_snes_address_matches_pc_offset() {
    let data = synth_rom(0x100000);
    let r = RomReader::new(&data);
    // $01:8004 in LoROM == PC 0x8004
    assert_eq!(r.read_u8_snes(0x01_8004).unwrap(), data[0x8004]);
    assert_eq!(
        r.read_u16_le_snes(0x01_8004).unwrap(),
        u16::from_le_bytes([data[0x8004], data[0x8005]])
    );
}

#[test]
fn rom_image_tracks_writes_and_diffs() {
    let base = synth_rom(0x8000);
    let mut img = RomImage::new(base.clone());
    assert!(!img.is_modified());
    assert!(img.diff().is_empty());

    img.write_u8(0x10, 0xAA).unwrap();
    img.write_bytes(0x100, &[1, 2, 3]).unwrap();
    assert!(img.is_modified());
    assert_eq!(img.current()[0x10], 0xAA);
    assert_eq!(img.original(), &base[..]);

    let diff = img.diff();
    assert_eq!(diff.len(), 2);
    assert_eq!(diff[0].offset, 0x10);
    assert_eq!(diff[0].bytes, vec![0xAA]);
    assert_eq!(diff[1].offset, 0x100);
    assert_eq!(diff[1].bytes, vec![1, 2, 3]);
}

#[test]
fn rom_image_write_of_same_value_is_not_a_change() {
    let base = synth_rom(0x8000);
    let mut img = RomImage::new(base.clone());
    let v = base[0x20];
    img.write_u8(0x20, v).unwrap();
    assert!(!img.is_modified());
    assert!(img.diff().is_empty());
}

#[test]
fn rom_image_rejects_out_of_range_writes() {
    let mut img = RomImage::new(vec![0u8; 16]);
    assert!(matches!(img.write_u8(16, 0), Err(RomError::OutOfRange { .. })));
    assert!(matches!(
        img.write_bytes(15, &[1, 2]),
        Err(RomError::OutOfRange { .. })
    ));
    // buffer untouched after failed writes
    assert!(!img.is_modified());
}
