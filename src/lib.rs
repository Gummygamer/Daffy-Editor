//! Daffy Editor — native desktop level editor / reverse-engineering workbench
//! for the SNES game *Daffy Duck: The Marvin Missions*.
//!
//! The library contains everything testable (ROM handling, codecs, model,
//! editor state, rendering math); the binary in `main.rs` is a thin
//! eframe/egui shell over [`app::DaffyApp`].
//!
//! No copyrighted ROM data is included anywhere; users must supply their own
//! legally obtained ROM. See docs/LEGAL.md.

pub mod app;
pub mod codecs;
pub mod editor;
pub mod error;
pub mod model;
pub mod patch;
pub mod rendering;
pub mod rom;
pub mod snes;
pub mod ui;
