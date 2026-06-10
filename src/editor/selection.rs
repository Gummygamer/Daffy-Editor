//! Current selection state in the editor viewport.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Selection {
    #[default]
    None,
    Tile { room: usize, x: u32, y: u32 },
    Object { room: usize, index: usize },
    EnemySpawn { room: usize, index: usize },
}

impl Selection {
    pub fn is_some(&self) -> bool {
        *self != Selection::None
    }

    pub fn clear(&mut self) {
        *self = Selection::None;
    }
}
