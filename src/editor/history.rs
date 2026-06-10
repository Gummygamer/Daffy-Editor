//! Undo/redo history with save-point based dirty tracking.

use crate::editor::commands::EditorCommand;
use crate::error::EditError;
use crate::model::level::Level;

#[derive(Debug, Default)]
pub struct EditorHistory {
    /// (forward, inverse) pairs, oldest first.
    undo: Vec<(EditorCommand, EditorCommand)>,
    redo: Vec<(EditorCommand, EditorCommand)>,
    /// Undo-stack depth at the last save; `None` if that state is no longer
    /// reachable (a new edit was made after undoing past the save point).
    saved_depth: Option<usize>,
}

impl EditorHistory {
    pub fn new() -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), saved_depth: Some(0) }
    }

    pub fn apply(&mut self, level: &mut Level, cmd: EditorCommand) -> Result<(), EditError> {
        let inverse = cmd.apply(level)?;
        // The redo branch (and the save point, if it lived there) is gone.
        if self.saved_depth.is_some_and(|d| d > self.undo.len()) {
            self.saved_depth = None;
        }
        self.redo.clear();
        self.undo.push((cmd, inverse));
        Ok(())
    }

    pub fn undo(&mut self, level: &mut Level) -> bool {
        let Some((forward, inverse)) = self.undo.pop() else { return false };
        inverse
            .apply(level)
            .expect("inverse of an applied command must apply cleanly");
        self.redo.push((forward, inverse));
        true
    }

    pub fn redo(&mut self, level: &mut Level) -> bool {
        let Some((forward, inverse)) = self.redo.pop() else { return false };
        forward
            .apply(level)
            .expect("redo of a previously applied command must apply cleanly");
        self.undo.push((forward, inverse));
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn mark_saved(&mut self) {
        self.saved_depth = Some(self.undo.len());
    }

    pub fn is_dirty(&self) -> bool {
        self.saved_depth != Some(self.undo.len())
    }
}
