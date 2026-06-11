//! Reverse-engineered graphics-asset layer: the bridge from a graphics id to the
//! decoded tiles.
//!
//! [`table`] parses the ROM's descriptor table (id → compressed-source pointer);
//! the compressed blob it points at is decoded by [`crate::codecs::gfx_rle`].

pub mod table;
