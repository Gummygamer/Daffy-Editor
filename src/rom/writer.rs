//! Controlled, diff-tracked write access to a ROM image.
//!
//! `RomImage` keeps the pristine original alongside the working copy so that
//! patch export can emit exactly the user's changes and nothing else.

use crate::error::RomError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedRun {
    pub offset: usize,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct RomImage {
    original: Vec<u8>,
    current: Vec<u8>,
}

impl RomImage {
    pub fn new(data: Vec<u8>) -> Self {
        Self { original: data.clone(), current: data }
    }

    pub fn original(&self) -> &[u8] {
        &self.original
    }

    pub fn current(&self) -> &[u8] {
        &self.current
    }

    pub fn len(&self) -> usize {
        self.current.len()
    }

    pub fn is_empty(&self) -> bool {
        self.current.is_empty()
    }

    fn check_range(&self, offset: usize, len: usize) -> Result<(), RomError> {
        let end = offset.checked_add(len);
        match end {
            Some(end) if end <= self.current.len() => Ok(()),
            _ => Err(RomError::OutOfRange { offset, len, size: self.current.len() }),
        }
    }

    pub fn write_u8(&mut self, offset: usize, value: u8) -> Result<(), RomError> {
        self.check_range(offset, 1)?;
        self.current[offset] = value;
        Ok(())
    }

    pub fn write_bytes(&mut self, offset: usize, bytes: &[u8]) -> Result<(), RomError> {
        self.check_range(offset, bytes.len())?;
        self.current[offset..offset + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    pub fn is_modified(&self) -> bool {
        self.current != self.original
    }

    /// Contiguous runs of bytes that differ from the original.
    pub fn diff(&self) -> Vec<ChangedRun> {
        let mut runs = Vec::new();
        let mut i = 0;
        let n = self.current.len();
        while i < n {
            if self.current[i] != self.original[i] {
                let start = i;
                while i < n && self.current[i] != self.original[i] {
                    i += 1;
                }
                runs.push(ChangedRun { offset: start, bytes: self.current[start..i].to_vec() });
            } else {
                i += 1;
            }
        }
        runs
    }
}
