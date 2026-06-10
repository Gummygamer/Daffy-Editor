//! Central error types. GUI code converts these to user-facing messages;
//! library code never panics on bad input.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RomError {
    #[error("ROM file too small: {size} bytes (minimum {min} bytes / 32 KiB)")]
    TooSmall { size: usize, min: usize },

    #[error(
        "unexpected ROM size {size} bytes: not a multiple of 32 KiB, \
         with or without a 512-byte copier header"
    )]
    BadSize { size: usize },

    #[error("read/write out of range: offset {offset:#x} + {len} bytes exceeds ROM size {size:#x}")]
    OutOfRange { offset: usize, len: usize, size: usize },

    #[error("invalid SNES address ${addr:06X} for LoROM: {reason}")]
    InvalidSnesAddress { addr: u32, reason: &'static str },

    #[error("PC offset {offset:#x} is beyond LoROM addressable range")]
    PcOffsetOutOfRange { offset: usize },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum PatchError {
    #[error("bad patch magic: expected {expected:?}")]
    BadMagic { expected: &'static str },

    #[error("patch data truncated at offset {offset}")]
    Truncated { offset: usize },

    #[error("original and modified buffers differ in length ({original} vs {modified}); IPS export needs equal sizes")]
    LengthMismatch { original: usize, modified: usize },

    #[error("change at offset {offset:#x} cannot be represented in an IPS patch (24-bit offset limit)")]
    OffsetTooLarge { offset: usize },

    #[error("source checksum mismatch: patch expects {expected:08X}, file is {actual:08X}")]
    SourceChecksumMismatch { expected: u32, actual: u32 },

    #[error("target checksum mismatch after patching: expected {expected:08X}, got {actual:08X}")]
    TargetChecksumMismatch { expected: u32, actual: u32 },

    #[error("patch checksum mismatch: patch file is corrupted")]
    PatchChecksumMismatch,

    #[error("malformed patch: {0}")]
    Malformed(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CodecError {
    #[error("compressed stream ended unexpectedly while reading {what} (offset {offset})")]
    UnexpectedEnd { what: &'static str, offset: usize },
}

#[derive(Debug, Error)]
pub enum EditError {
    #[error("room index {0} out of range")]
    RoomOutOfRange(usize),

    #[error("tile ({x}, {y}) out of range for room of {width}x{height} metatiles")]
    TileOutOfRange { x: u32, y: u32, width: u32, height: u32 },

    #[error("object index {0} out of range")]
    ObjectOutOfRange(usize),
}
