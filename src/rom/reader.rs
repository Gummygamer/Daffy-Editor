//! Bounds-checked read access to ROM bytes (by PC offset or SNES address).

use crate::error::RomError;
use crate::snes::lorom::snes_to_pc;

pub struct RomReader<'a> {
    data: &'a [u8],
}

impl<'a> RomReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn slice(&self, offset: usize, len: usize) -> Result<&'a [u8], RomError> {
        let end = offset.checked_add(len).ok_or(RomError::OutOfRange {
            offset,
            len,
            size: self.data.len(),
        })?;
        self.data.get(offset..end).ok_or(RomError::OutOfRange {
            offset,
            len,
            size: self.data.len(),
        })
    }

    pub fn read_u8(&self, offset: usize) -> Result<u8, RomError> {
        Ok(self.slice(offset, 1)?[0])
    }

    pub fn read_u16_le(&self, offset: usize) -> Result<u16, RomError> {
        let s = self.slice(offset, 2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }

    pub fn read_u24_le(&self, offset: usize) -> Result<u32, RomError> {
        let s = self.slice(offset, 3)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], 0]))
    }

    pub fn read_u8_snes(&self, addr: u32) -> Result<u8, RomError> {
        self.read_u8(snes_to_pc(addr)?)
    }

    pub fn read_u16_le_snes(&self, addr: u32) -> Result<u16, RomError> {
        self.read_u16_le(snes_to_pc(addr)?)
    }

    pub fn read_u24_le_snes(&self, addr: u32) -> Result<u32, RomError> {
        self.read_u24_le(snes_to_pc(addr)?)
    }

    pub fn slice_snes(&self, addr: u32, len: usize) -> Result<&'a [u8], RomError> {
        self.slice(snes_to_pc(addr)?, len)
    }
}
