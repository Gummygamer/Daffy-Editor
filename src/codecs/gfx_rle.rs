//! Graphics decompressor — the custom control-byte RLE used by the SNES game's
//! tile loader (routine `$82:84FD`–`$82:865F`).
//!
//! Confidence: the per-command mechanics are **decode-confirmed** — every line
//! below is a transcription of the disassembled routine
//! (`docs/reverse-engineering/reports/disasm_decompressor.json`). The two-pass /
//! stride-2 layout is the SNES 2-bitplane interleave (see
//! [docs/reverse-engineering/compression-codec.md]). A live ROM round-trip
//! (`tools/mesen/roundtrip_decompressor.lua` + `tools/roundtrip.sh`) confirms
//! the decoder reproduces a Mesen2 `$7F:C000` staging dump byte-for-byte.
//!
//! The original routine streams compressed bytes from ROM banks
//! `$92/$93/$95/$96` (24-bit DP source pointer `$16/$17/$18`, which resets the
//! offset to `$8000` on a 16-bit wrap so it traverses LoROM banks contiguously)
//! and writes decoded bytes into WRAM `$7F:C000…` via a 24-bit destination
//! pointer (`$19/$1A/$1B`) advanced by **2** after every byte. This decoder
//! models the source as a flat slice (the caller assembles the contiguous PC
//! bytes) and the destination as a `Vec<u8>` with the same stride-2 interleave.

use crate::error::CodecError;

/// Number of interleaved passes the routine performs (`$1F` is initialised to
/// `2`). Pass 1 fills the even output bytes (plane 0), pass 2 the odd bytes
/// (plane 1); each pass is terminated by an end-of-pass command (`$40`).
pub const PASS_COUNT: u8 = 2;

/// Result of [`decompress`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decompressed {
    /// The decoded, bitplane-interleaved output buffer.
    pub data: Vec<u8>,
    /// Number of compressed bytes consumed (including both `$40` terminators).
    /// Lets a caller find where the next blob begins in a packed stream.
    pub bytes_consumed: usize,
}

/// The operation selected by the top 3 bits of a command byte (`cmd & 0xE0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    /// `$00`/`$20` — copy the next `N` stream bytes verbatim.
    Literal,
    /// `$40` — end of pass (switch to the odd plane / terminate). Length ignored.
    EndPass,
    /// `$60` — read `B`,`S`; emit the pair `B,S` repeated `N` times.
    Pattern,
    /// `$80` — read `B`; emit `B` `N` times.
    ByteRun,
    /// `$A0` — read `B`; emit `B, B+1, …` (`N` bytes, 8-bit wrap).
    IncRun,
    /// `$C0` — emit `0x00` `N` times (no stream byte).
    ZeroFill,
    /// `$E0` — read `B`; emit `B, B-1, …` (`N` bytes, 8-bit wrap).
    DecRun,
}

impl Op {
    fn from_cmd(cmd: u8) -> Self {
        match cmd & 0xE0 {
            0x00 | 0x20 => Op::Literal,
            0x40 => Op::EndPass,
            0x60 => Op::Pattern,
            0x80 => Op::ByteRun,
            0xA0 => Op::IncRun,
            0xC0 => Op::ZeroFill,
            0xE0 => Op::DecRun,
            _ => unreachable!("all 8 high-bit values are covered"),
        }
    }
}

/// Decode a control-byte-RLE blob into its bitplane-interleaved bytes.
///
/// `src` must begin at a command byte (the first byte the original routine
/// reads through `[$16]`). Decoding stops after [`PASS_COUNT`] end-of-pass
/// markers; the bytes after that are left untouched and reported via
/// [`Decompressed::bytes_consumed`].
pub fn decompress(src: &[u8]) -> Result<Decompressed, CodecError> {
    let mut cur = 0usize;
    let mut out: Vec<u8> = Vec::new();
    // Destination cursor. Pass 1 starts at byte 0 (even plane); the first `$40`
    // moves it to byte 1 (the `$1C` = base+1 odd plane). Stride is always 2.
    let mut pos = 0usize;
    let mut passes_done = 0u8;

    while passes_done < PASS_COUNT {
        let cmd = read(src, &mut cur, "command byte")?;
        // Top 3 bits select the op; low 5 bits are length-1, so N = 1..=32.
        let n = (cmd & 0x1F) as usize + 1;
        match Op::from_cmd(cmd) {
            Op::EndPass => {
                passes_done += 1;
                pos = 1; // dest := $1C (even base + 1)
            }
            Op::Literal => {
                for _ in 0..n {
                    let b = read(src, &mut cur, "literal byte")?;
                    put(&mut out, &mut pos, b);
                }
            }
            Op::Pattern => {
                let b = read(src, &mut cur, "pattern byte 0")?;
                let s = read(src, &mut cur, "pattern byte 1")?;
                for _ in 0..n {
                    put(&mut out, &mut pos, b);
                    put(&mut out, &mut pos, s);
                }
            }
            Op::ByteRun => {
                let b = read(src, &mut cur, "run byte")?;
                for _ in 0..n {
                    put(&mut out, &mut pos, b);
                }
            }
            Op::IncRun => {
                let mut v = read(src, &mut cur, "increment seed")?;
                for _ in 0..n {
                    put(&mut out, &mut pos, v);
                    v = v.wrapping_add(1);
                }
            }
            Op::ZeroFill => {
                for _ in 0..n {
                    put(&mut out, &mut pos, 0);
                }
            }
            Op::DecRun => {
                let mut v = read(src, &mut cur, "decrement seed")?;
                for _ in 0..n {
                    put(&mut out, &mut pos, v);
                    v = v.wrapping_sub(1);
                }
            }
        }
    }

    Ok(Decompressed {
        data: out,
        bytes_consumed: cur,
    })
}

/// Read one stream byte, advancing the cursor; error if the stream is exhausted.
fn read(src: &[u8], cur: &mut usize, what: &'static str) -> Result<u8, CodecError> {
    let b = *src
        .get(*cur)
        .ok_or(CodecError::UnexpectedEnd { what, offset: *cur })?;
    *cur += 1;
    Ok(b)
}

/// Store one decoded byte at the destination cursor and advance it by the
/// interleave stride of 2, growing the buffer (zero-filling skipped slots) as
/// needed. The skipped slots belong to the other plane's pass.
fn put(out: &mut Vec<u8>, pos: &mut usize, b: u8) {
    if *pos >= out.len() {
        out.resize(*pos + 1, 0);
    }
    out[*pos] = b;
    *pos += 2;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `$40` end-of-pass; the length bits are ignored, so any low 5 bits work.
    const END: u8 = 0x40;

    /// Run a single command as pass 1 and an empty pass 2, returning only the
    /// even (plane-0) bytes the command produced.
    fn plane0(cmd_stream: &[u8]) -> Vec<u8> {
        let mut s = cmd_stream.to_vec();
        s.push(END); // terminate pass 1
        s.push(END); // empty pass 2
        let d = decompress(&s).unwrap();
        // Even bytes only (odd plane is all zero here).
        d.data.iter().step_by(2).copied().collect()
    }

    #[test]
    fn literal_copy() {
        // cmd $02 => op $00, N=3, then 3 verbatim bytes.
        assert_eq!(plane0(&[0x02, 0xAA, 0xBB, 0xCC]), vec![0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn literal_max_length_is_32() {
        // cmd $1F => op $00, N=32.
        let data: Vec<u8> = (0..32).collect();
        let mut stream = vec![0x1F];
        stream.extend_from_slice(&data);
        assert_eq!(plane0(&stream), data);
    }

    #[test]
    fn byte_run_rle() {
        // cmd $82 => op $80, N=3; seed $55.
        assert_eq!(plane0(&[0x82, 0x55]), vec![0x55, 0x55, 0x55]);
    }

    #[test]
    fn zero_fill_consumes_no_data_byte() {
        // cmd $C2 => op $C0, N=3; no stream byte follows.
        assert_eq!(plane0(&[0xC2]), vec![0x00, 0x00, 0x00]);
    }

    #[test]
    fn incrementing_run_wraps_at_8_bits() {
        // cmd $A1 => op $A0, N=2; seed $FF => $FF, $00.
        assert_eq!(plane0(&[0xA1, 0xFF]), vec![0xFF, 0x00]);
    }

    #[test]
    fn decrementing_run_wraps_at_8_bits() {
        // cmd $E1 => op $E0, N=2; seed $00 => $00, $FF.
        assert_eq!(plane0(&[0xE1, 0x00]), vec![0x00, 0xFF]);
    }

    #[test]
    fn pattern_fill_emits_alternating_pair() {
        // cmd $62 => op $60, N=3; bytes $AB,$CD => AB CD AB CD AB CD.
        assert_eq!(
            plane0(&[0x62, 0xAB, 0xCD]),
            vec![0xAB, 0xCD, 0xAB, 0xCD, 0xAB, 0xCD]
        );
    }

    #[test]
    fn two_passes_interleave_into_planes() {
        // Pass 1 (evens): literal AA,BB,CC. Pass 2 (odds): literal 11,22,33.
        let stream = [
            0x02, 0xAA, 0xBB, 0xCC, END, // even plane
            0x02, 0x11, 0x22, 0x33, END, // odd plane
        ];
        let d = decompress(&stream).unwrap();
        assert_eq!(d.data, vec![0xAA, 0x11, 0xBB, 0x22, 0xCC, 0x33]);
        assert_eq!(d.bytes_consumed, stream.len());
    }

    #[test]
    fn bytes_consumed_stops_at_second_end_marker() {
        // A trailing byte after the two $40s must not be consumed.
        let stream = [0x82, 0x55, END, 0x82, 0x66, END, 0xFF];
        let d = decompress(&stream).unwrap();
        assert_eq!(d.bytes_consumed, 6);
        // evens = 55,55,55 ; odds = 66,66,66
        assert_eq!(d.data, vec![0x55, 0x66, 0x55, 0x66, 0x55, 0x66]);
    }

    #[test]
    fn end_of_pass_ignores_length_bits() {
        // $5F has low bits set but is still a plain end-of-pass.
        let stream = [0x80, 0x55, 0x5F, 0x80, 0x66, 0x5F];
        let d = decompress(&stream).unwrap();
        assert_eq!(d.data, vec![0x55, 0x66]);
    }

    #[test]
    fn truncated_literal_errors() {
        // Promises 3 literal bytes but supplies 1.
        let err = decompress(&[0x02, 0xAA]).unwrap_err();
        assert_eq!(
            err,
            CodecError::UnexpectedEnd {
                what: "literal byte",
                offset: 2,
            }
        );
    }

    #[test]
    fn missing_end_marker_errors() {
        // No $40 at all: runs off the end looking for the next command.
        let err = decompress(&[0x82, 0x55]).unwrap_err();
        assert_eq!(
            err,
            CodecError::UnexpectedEnd {
                what: "command byte",
                offset: 2,
            }
        );
    }
}
