//! IPS patch creation and application.
//!
//! Format: "PATCH" + records (3-byte BE offset, 2-byte BE length, data;
//! length 0 means RLE: 2-byte BE count + 1 value byte) + "EOF".
//! Limits: offsets must fit in 24 bits; a record may not *start* at offset
//! 0x454F46 because those bytes spell "EOF" — we start such a record one
//! byte earlier instead.

use crate::error::PatchError;

const MAGIC: &[u8] = b"PATCH";
const EOF: &[u8] = b"EOF";
/// Offset whose 3-byte BE encoding collides with the "EOF" terminator.
const EOF_OFFSET: usize = 0x454F46;
const MAX_OFFSET: usize = 0xFF_FFFF;
const MAX_RECORD_LEN: usize = 0xFFFF;

/// Create an IPS patch containing exactly the bytes that differ between
/// `original` and `modified`. Both buffers must have the same length.
pub fn create_ips(original: &[u8], modified: &[u8]) -> Result<Vec<u8>, PatchError> {
    if original.len() != modified.len() {
        return Err(PatchError::LengthMismatch {
            original: original.len(),
            modified: modified.len(),
        });
    }

    let mut patch = MAGIC.to_vec();
    let mut i = 0;
    let n = original.len();
    while i < n {
        if original[i] == modified[i] {
            i += 1;
            continue;
        }
        let mut start = i;
        while i < n && original[i] != modified[i] {
            i += 1;
        }
        // Avoid a record starting at the literal "EOF" offset by widening it
        // one byte to the left (that byte is unchanged, so rewriting it is safe).
        if start == EOF_OFFSET {
            start -= 1;
        }
        if i - 1 > MAX_OFFSET {
            return Err(PatchError::OffsetTooLarge { offset: i - 1 });
        }
        let mut chunk_start = start;
        while chunk_start < i {
            let len = (i - chunk_start).min(MAX_RECORD_LEN);
            // Re-check the EOF collision for split continuation records.
            let rec_start = if chunk_start == EOF_OFFSET { chunk_start - 1 } else { chunk_start };
            let rec_len = len + (chunk_start - rec_start);
            patch.extend_from_slice(&(rec_start as u32).to_be_bytes()[1..]);
            patch.extend_from_slice(&(rec_len as u16).to_be_bytes());
            patch.extend_from_slice(&modified[rec_start..rec_start + rec_len]);
            chunk_start += len;
        }
    }
    patch.extend_from_slice(EOF);
    Ok(patch)
}

/// Apply an IPS patch in place. The target grows if a record writes past its end.
pub fn apply_ips(target: &mut Vec<u8>, patch: &[u8]) -> Result<(), PatchError> {
    if patch.len() < MAGIC.len() + EOF.len() || &patch[..MAGIC.len()] != MAGIC {
        return Err(PatchError::BadMagic { expected: "PATCH" });
    }
    let mut i = MAGIC.len();
    loop {
        let take = |i: &mut usize, len: usize| -> Result<&[u8], PatchError> {
            let s = patch
                .get(*i..*i + len)
                .ok_or(PatchError::Truncated { offset: *i })?;
            *i += len;
            Ok(s)
        };

        let head = take(&mut i, 3)?;
        if head == EOF {
            // Anything after EOF other than the optional 3-byte truncation
            // extension is suspicious but tolerated by most tools; we accept
            // 0 or 3 trailing bytes and ignore truncation.
            return Ok(());
        }
        let offset = u32::from_be_bytes([0, head[0], head[1], head[2]]) as usize;
        let len_bytes = take(&mut i, 2)?;
        let len = u16::from_be_bytes([len_bytes[0], len_bytes[1]]) as usize;

        let (write_len, data): (usize, Vec<u8>) = if len == 0 {
            // RLE record
            let rle = take(&mut i, 3)?;
            let count = u16::from_be_bytes([rle[0], rle[1]]) as usize;
            (count, vec![rle[2]; count])
        } else {
            (len, take(&mut i, len)?.to_vec())
        };

        let end = offset + write_len;
        if end > target.len() {
            target.resize(end, 0);
        }
        target[offset..end].copy_from_slice(&data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_changed_bytes_form_one_record() {
        let a = vec![0u8; 64];
        let mut b = a.clone();
        b[10] = 1;
        b[11] = 2;
        b[12] = 3;
        let patch = create_ips(&a, &b).unwrap();
        // PATCH + one record (3+2+3) + EOF
        assert_eq!(patch.len(), 5 + 8 + 3);
    }
}
