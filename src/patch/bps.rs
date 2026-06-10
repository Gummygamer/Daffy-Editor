//! BPS patch creation and application (byuu's beat format).
//!
//! Layout: "BPS1", varint source-size, varint target-size, varint
//! metadata-size, metadata, actions, then CRC32 of source, target and of the
//! patch itself (all little-endian).
//!
//! Action encoding: varint `((length - 1) << 2) | command` with commands
//! 0 = SourceRead, 1 = TargetRead (raw bytes follow), 2 = SourceCopy,
//! 3 = TargetCopy (signed varint relative offset follows).
//!
//! Our encoder emits only SourceRead/TargetRead runs — valid, verifiable
//! patches that are larger than delta-optimal ones; good enough for the
//! "only changed bytes" guarantee. The decoder supports all four commands.

use crate::error::PatchError;
use crate::rom::info::crc32_of;

const MAGIC: &[u8] = b"BPS1";

fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let x = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(x | 0x80);
            return;
        }
        out.push(x);
        value -= 1;
    }
}

fn read_varint(data: &[u8], pos: &mut usize) -> Result<u64, PatchError> {
    let mut value: u64 = 0;
    let mut shift: u64 = 1;
    loop {
        let byte = *data.get(*pos).ok_or(PatchError::Truncated { offset: *pos })?;
        *pos += 1;
        value = value
            .checked_add((byte as u64 & 0x7F).checked_mul(shift).ok_or_else(malformed_varint)?)
            .ok_or_else(malformed_varint)?;
        if byte & 0x80 != 0 {
            return Ok(value);
        }
        shift = shift.checked_mul(128).ok_or_else(malformed_varint)?;
        value = value.checked_add(shift).ok_or_else(malformed_varint)?;
    }
}

fn malformed_varint() -> PatchError {
    PatchError::Malformed("varint overflow".to_string())
}

fn write_action(out: &mut Vec<u8>, command: u64, length: usize) {
    debug_assert!(length > 0);
    write_varint(out, ((length as u64 - 1) << 2) | command);
}

/// Create a BPS patch transforming `source` into `target`.
pub fn create_bps(source: &[u8], target: &[u8], metadata: &str) -> Result<Vec<u8>, PatchError> {
    let mut p = MAGIC.to_vec();
    write_varint(&mut p, source.len() as u64);
    write_varint(&mut p, target.len() as u64);
    write_varint(&mut p, metadata.len() as u64);
    p.extend_from_slice(metadata.as_bytes());

    let mut i = 0;
    while i < target.len() {
        let matches_source = |j: usize| j < source.len() && source[j] == target[j];
        if matches_source(i) {
            let start = i;
            while i < target.len() && matches_source(i) {
                i += 1;
            }
            write_action(&mut p, 0, i - start); // SourceRead
        } else {
            let start = i;
            while i < target.len() && !matches_source(i) {
                i += 1;
            }
            write_action(&mut p, 1, i - start); // TargetRead
            p.extend_from_slice(&target[start..i]);
        }
    }

    p.extend_from_slice(&crc32_of(source).to_le_bytes());
    p.extend_from_slice(&crc32_of(target).to_le_bytes());
    p.extend_from_slice(&crc32_of(&p).to_le_bytes());
    Ok(p)
}

#[derive(Debug)]
pub struct AppliedBps {
    pub output: Vec<u8>,
    pub metadata: String,
}

/// Apply a BPS patch to `source`, verifying all three checksums.
pub fn apply_bps(source: &[u8], patch: &[u8]) -> Result<AppliedBps, PatchError> {
    if patch.len() < MAGIC.len() + 12 {
        return Err(PatchError::Truncated { offset: patch.len() });
    }
    if &patch[..MAGIC.len()] != MAGIC {
        return Err(PatchError::BadMagic { expected: "BPS1" });
    }

    let footer_at = patch.len() - 12;
    let stored_patch_crc =
        u32::from_le_bytes(patch[footer_at + 8..].try_into().expect("4 bytes"));
    if crc32_of(&patch[..footer_at + 8]) != stored_patch_crc {
        return Err(PatchError::PatchChecksumMismatch);
    }
    let stored_source_crc =
        u32::from_le_bytes(patch[footer_at..footer_at + 4].try_into().expect("4 bytes"));
    let stored_target_crc =
        u32::from_le_bytes(patch[footer_at + 4..footer_at + 8].try_into().expect("4 bytes"));

    let actual_source_crc = crc32_of(source);
    if actual_source_crc != stored_source_crc {
        return Err(PatchError::SourceChecksumMismatch {
            expected: stored_source_crc,
            actual: actual_source_crc,
        });
    }

    let body = &patch[..footer_at];
    let mut pos = MAGIC.len();
    let source_size = read_varint(body, &mut pos)? as usize;
    let target_size = read_varint(body, &mut pos)? as usize;
    let metadata_size = read_varint(body, &mut pos)? as usize;
    if source_size != source.len() {
        return Err(PatchError::Malformed(format!(
            "patch expects source of {source_size} bytes, file is {} bytes",
            source.len()
        )));
    }
    let metadata = body
        .get(pos..pos + metadata_size)
        .ok_or(PatchError::Truncated { offset: pos })?;
    let metadata = String::from_utf8_lossy(metadata).into_owned();
    pos += metadata_size;

    let mut output = Vec::with_capacity(target_size);
    let mut source_rel: usize = 0;
    let mut target_rel: usize = 0;

    while pos < body.len() {
        let action = read_varint(body, &mut pos)?;
        let length = (action >> 2) as usize + 1;
        let err = |what: &str| PatchError::Malformed(format!("{what} out of range"));
        match action & 3 {
            0 => {
                // SourceRead: copy from source at the current output offset
                let at = output.len();
                let chunk = source.get(at..at + length).ok_or_else(|| err("SourceRead"))?;
                output.extend_from_slice(chunk);
            }
            1 => {
                // TargetRead: raw bytes from the patch
                let chunk = body.get(pos..pos + length).ok_or(PatchError::Truncated { offset: pos })?;
                pos += length;
                output.extend_from_slice(chunk);
            }
            2 => {
                // SourceCopy
                let delta = read_signed_offset(body, &mut pos)?;
                source_rel = apply_delta(source_rel, delta).ok_or_else(|| err("SourceCopy"))?;
                let chunk = source
                    .get(source_rel..source_rel + length)
                    .ok_or_else(|| err("SourceCopy"))?;
                output.extend_from_slice(chunk);
                source_rel += length;
            }
            _ => {
                // TargetCopy: may overlap itself, copy byte by byte
                let delta = read_signed_offset(body, &mut pos)?;
                target_rel = apply_delta(target_rel, delta).ok_or_else(|| err("TargetCopy"))?;
                for _ in 0..length {
                    let b = *output.get(target_rel).ok_or_else(|| err("TargetCopy"))?;
                    output.push(b);
                    target_rel += 1;
                }
            }
        }
        if output.len() > target_size {
            return Err(PatchError::Malformed("output exceeds declared target size".into()));
        }
    }

    if output.len() != target_size {
        return Err(PatchError::Malformed(format!(
            "output is {} bytes, patch declares {target_size}",
            output.len()
        )));
    }
    let actual_target_crc = crc32_of(&output);
    if actual_target_crc != stored_target_crc {
        return Err(PatchError::TargetChecksumMismatch {
            expected: stored_target_crc,
            actual: actual_target_crc,
        });
    }
    Ok(AppliedBps { output, metadata })
}

fn read_signed_offset(body: &[u8], pos: &mut usize) -> Result<i64, PatchError> {
    let raw = read_varint(body, pos)?;
    let magnitude = (raw >> 1) as i64;
    Ok(if raw & 1 != 0 { -magnitude } else { magnitude })
}

fn apply_delta(base: usize, delta: i64) -> Option<usize> {
    let v = base as i64 + delta;
    usize::try_from(v).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_round_trip() {
        for v in [0u64, 1, 127, 128, 129, 0xFFFF, 0x12_3456, u32::MAX as u64] {
            let mut buf = Vec::new();
            write_varint(&mut buf, v);
            let mut pos = 0;
            assert_eq!(read_varint(&buf, &mut pos).unwrap(), v);
            assert_eq!(pos, buf.len());
        }
    }
}
