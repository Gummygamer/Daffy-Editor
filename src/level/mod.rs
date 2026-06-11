//! Level / scene data structures for *Daffy Duck: The Marvin Missions*.
//!
//! The bridge from "a level" to its data is **code**, not a flat data table: each
//! scene is set up by a dedicated routine that loads a graphics batch (inline
//! `JSL $80:FC26`, see [`crate::gfx::table`]) and then fills a fixed block of
//! direct-page / low-RAM variables with the scene's data pointers and map size,
//! before handing off to the engine. [`scan`] recovers that pointer block from
//! every such routine, yielding the game's level table.
//!
//! See `docs/reverse-engineering/level-format.md`.

pub mod cell;
pub mod index;
pub mod scan;

pub use index::{parse_game_index, LevelEntry};
pub use scan::{scan_levels, LevelData};
