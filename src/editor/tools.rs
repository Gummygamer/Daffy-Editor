//! Editing tools selectable in the toolbar.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Tool {
    /// Click to select tiles/objects, drag to pan.
    #[default]
    Select,
    /// Click/drag to paint the active metatile.
    Paint,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Paint => "Paint",
        }
    }
}
