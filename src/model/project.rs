//! Project file model and JSON (de)serialization.
//!
//! A project stores the user's work plus the *identity* of the ROM it was
//! made against (hashes only — never ROM bytes), so the editor can refuse or
//! warn when reopened against a different file.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::level::Level;
use crate::rom::info::RomInfo;
use crate::rom::version::RomVersion;

pub const PROJECT_FORMAT_VERSION: u32 = 1;
pub const PROJECT_FILE_EXTENSION: &str = "daffyproj.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RomIdentity {
    pub crc32: u32,
    pub sha1_hex: String,
    pub size: usize,
    pub had_copier_header: bool,
    pub version: RomVersion,
}

impl From<&RomInfo> for RomIdentity {
    fn from(info: &RomInfo) -> Self {
        Self {
            crc32: info.crc32,
            sha1_hex: info.sha1_hex.clone(),
            size: info.size,
            had_copier_header: info.had_copier_header,
            version: info.version,
        }
    }
}

/// A byte-level change relative to the original ROM, with provenance note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchChange {
    pub offset: usize,
    pub original: Vec<u8>,
    pub modified: Vec<u8>,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub format_version: u32,
    pub name: String,
    pub rom: Option<RomIdentity>,
    pub levels: Vec<Level>,
    pub changes: Vec<PatchChange>,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            format_version: PROJECT_FORMAT_VERSION,
            name: "untitled".to_string(),
            rom: None,
            levels: Vec::new(),
            changes: Vec::new(),
        }
    }
}

impl Project {
    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn save_to_file(&self, path: &Path) -> anyhow::Result<()> {
        std::fs::write(path, self.to_json()?)?;
        Ok(())
    }

    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }
}
