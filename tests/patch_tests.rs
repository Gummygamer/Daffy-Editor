//! IPS and BPS patch generation/application tests against synthetic buffers only.

use daffy_editor::error::PatchError;
use daffy_editor::patch::bps::{apply_bps, create_bps};
use daffy_editor::patch::ips::{apply_ips, create_ips};

fn synth(size: usize, seed: u8) -> Vec<u8> {
    (0..size).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect()
}

fn ips_records(patch: &[u8]) -> usize {
    // Count records by walking the patch (data records only; assumes no RLE in our output).
    assert_eq!(&patch[..5], b"PATCH");
    let mut n = 0;
    let mut i = 5;
    while &patch[i..i + 3] != b"EOF" {
        let len = u16::from_be_bytes([patch[i + 3], patch[i + 4]]) as usize;
        assert!(len > 0, "create_ips should not emit RLE/zero-length records");
        i += 5 + len;
        n += 1;
    }
    n
}

#[test]
fn ips_identical_buffers_produce_empty_patch() {
    let a = synth(0x1000, 7);
    let patch = create_ips(&a, &a).unwrap();
    assert_eq!(ips_records(&patch), 0);
    let mut target = a.clone();
    apply_ips(&mut target, &patch).unwrap();
    assert_eq!(target, a);
}

#[test]
fn ips_single_byte_change_is_one_minimal_record() {
    let a = synth(0x1000, 7);
    let mut b = a.clone();
    b[0x123] ^= 0xFF;
    let patch = create_ips(&a, &b).unwrap();
    assert_eq!(ips_records(&patch), 1);
    // PATCH + (3 offset + 2 len + 1 data) + EOF
    assert_eq!(patch.len(), 5 + 6 + 3);

    let mut target = a.clone();
    apply_ips(&mut target, &patch).unwrap();
    assert_eq!(target, b);
}

#[test]
fn ips_two_distant_runs_are_two_records() {
    let a = synth(0x4000, 1);
    let mut b = a.clone();
    b[0x10..0x14].copy_from_slice(&[9, 9, 9, 9]);
    b[0x3000] = 0x55;
    let patch = create_ips(&a, &b).unwrap();
    assert_eq!(ips_records(&patch), 2);
    let mut target = a.clone();
    apply_ips(&mut target, &patch).unwrap();
    assert_eq!(target, b);
}

#[test]
fn ips_run_longer_than_record_limit_is_split() {
    let a = vec![0u8; 0x20000];
    let mut b = a.clone();
    for i in 0..0x12000 {
        b[i] = 0xAB;
    }
    let patch = create_ips(&a, &b).unwrap();
    assert!(ips_records(&patch) >= 2);
    let mut target = a.clone();
    apply_ips(&mut target, &patch).unwrap();
    assert_eq!(target, b);
}

#[test]
fn ips_change_at_eof_offset_still_round_trips() {
    // Offset 0x454F46 spells "EOF"; a record starting there would be ambiguous.
    let size = 0x460000;
    let a = vec![0x11u8; size];
    let mut b = a.clone();
    b[0x454F46] = 0x99;
    let patch = create_ips(&a, &b).unwrap();
    // No record may start at the literal EOF offset.
    let mut i = 5;
    while &patch[i..i + 3] != b"EOF" {
        let off = u32::from_be_bytes([0, patch[i], patch[i + 1], patch[i + 2]]);
        assert_ne!(off, 0x454F46, "record must not start at the 'EOF' offset");
        let len = u16::from_be_bytes([patch[i + 3], patch[i + 4]]) as usize;
        i += 5 + len;
    }
    let mut target = a.clone();
    apply_ips(&mut target, &patch).unwrap();
    assert_eq!(target, b);
}

#[test]
fn ips_apply_rejects_bad_magic() {
    let mut target = vec![0u8; 16];
    let err = apply_ips(&mut target, b"NOPE!whatever").unwrap_err();
    assert!(matches!(err, PatchError::BadMagic { .. }), "got {err:?}");
}

#[test]
fn ips_apply_rejects_truncated_patch() {
    let a = synth(0x100, 3);
    let mut b = a.clone();
    b[4] = 0;
    let patch = create_ips(&a, &b).unwrap();
    for cut in 1..patch.len() {
        let mut target = a.clone();
        assert!(
            apply_ips(&mut target, &patch[..patch.len() - cut]).is_err(),
            "truncation by {cut} bytes must error"
        );
    }
}

#[test]
fn ips_apply_supports_rle_records_and_growth() {
    // Hand-built patch: RLE record writing 0x10 bytes of 0x7E at offset 0x20,
    // which also grows the 16-byte target.
    let mut patch = b"PATCH".to_vec();
    patch.extend_from_slice(&[0x00, 0x00, 0x20]); // offset
    patch.extend_from_slice(&[0x00, 0x00]); // len 0 => RLE
    patch.extend_from_slice(&[0x00, 0x10]); // RLE count
    patch.push(0x7E); // RLE value
    patch.extend_from_slice(b"EOF");

    let mut target = vec![1u8; 16];
    apply_ips(&mut target, &patch).unwrap();
    assert_eq!(target.len(), 0x30);
    assert!(target[0x20..0x30].iter().all(|&b| b == 0x7E));
}

#[test]
fn ips_create_rejects_offsets_beyond_24_bits() {
    let size = 0x100_0001;
    let a = vec![0u8; size];
    let mut b = a.clone();
    b[size - 1] = 1; // offset 0x1000000 is unrepresentable in IPS
    assert!(create_ips(&a, &b).is_err());
}

#[test]
fn ips_create_requires_equal_lengths() {
    let a = vec![0u8; 32];
    let b = vec![0u8; 33];
    assert!(matches!(
        create_ips(&a, &b),
        Err(PatchError::LengthMismatch { .. })
    ));
}

// ---------- BPS ----------

#[test]
fn bps_round_trip_restores_target_exactly() {
    let source = synth(0x2000, 5);
    let mut target = source.clone();
    target[0x40] = 0;
    target[0x41] = 1;
    target[0x1FFF] ^= 0x80;
    let patch = create_bps(&source, &target, "test-meta").unwrap();
    let applied = apply_bps(&source, &patch).unwrap();
    assert_eq!(applied.output, target);
    assert_eq!(applied.metadata, "test-meta");
}

#[test]
fn bps_identical_buffers_round_trip() {
    let source = synth(0x800, 9);
    let patch = create_bps(&source, &source, "").unwrap();
    let applied = apply_bps(&source, &patch).unwrap();
    assert_eq!(applied.output, source);
}

#[test]
fn bps_detects_wrong_source() {
    let source = synth(0x1000, 5);
    let mut target = source.clone();
    target[10] = 0xFF;
    let patch = create_bps(&source, &target, "").unwrap();

    let wrong_source = synth(0x1000, 6);
    let err = apply_bps(&wrong_source, &patch).unwrap_err();
    assert!(
        matches!(err, PatchError::SourceChecksumMismatch { .. }),
        "got {err:?}"
    );
}

#[test]
fn bps_detects_corrupted_patch() {
    let source = synth(0x1000, 5);
    let mut target = source.clone();
    target[10] = 0xFF;
    let mut patch = create_bps(&source, &target, "").unwrap();
    let mid = patch.len() / 2;
    patch[mid] ^= 0x01;
    assert!(apply_bps(&source, &patch).is_err());
}

#[test]
fn bps_apply_rejects_truncated_patch() {
    let source = synth(0x400, 2);
    let mut target = source.clone();
    target[0] = !target[0];
    let patch = create_bps(&source, &target, "").unwrap();
    assert!(apply_bps(&source, &patch[..patch.len() - 4]).is_err());
    assert!(apply_bps(&source, &patch[..6]).is_err());
}

#[test]
fn bps_supports_different_target_size() {
    let source = synth(0x100, 1);
    let mut target = source.clone();
    target.extend_from_slice(&[0xAA; 0x40]);
    let patch = create_bps(&source, &target, "").unwrap();
    let applied = apply_bps(&source, &patch).unwrap();
    assert_eq!(applied.output, target);
}
