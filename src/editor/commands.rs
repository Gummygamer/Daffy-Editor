//! Editing commands. Every command application returns its inverse so the
//! history can undo it without snapshots.

use serde::{Deserialize, Serialize};

use crate::error::EditError;
use crate::model::level::Level;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorCommand {
    SetTile { room: usize, x: u32, y: u32, metatile: u16 },
    MoveObject { room: usize, object: usize, x: u32, y: u32 },
    SetObjectParams { room: usize, object: usize, params: Vec<u8> },
}

impl EditorCommand {
    /// Apply to `level`, returning the inverse command.
    /// On error the level is left unchanged.
    pub fn apply(&self, level: &mut Level) -> Result<EditorCommand, EditError> {
        match *self {
            EditorCommand::SetTile { room, x, y, metatile } => {
                let r = level.rooms.get_mut(room).ok_or(EditError::RoomOutOfRange(room))?;
                let prev = r.set_tile(x, y, metatile)?;
                Ok(EditorCommand::SetTile { room, x, y, metatile: prev })
            }
            EditorCommand::MoveObject { room, object, x, y } => {
                let r = level.rooms.get_mut(room).ok_or(EditError::RoomOutOfRange(room))?;
                let o = r.objects.get_mut(object).ok_or(EditError::ObjectOutOfRange(object))?;
                let (px, py) = (o.x, o.y);
                o.x = x;
                o.y = y;
                Ok(EditorCommand::MoveObject { room, object, x: px, y: py })
            }
            EditorCommand::SetObjectParams { room, object, ref params } => {
                let r = level.rooms.get_mut(room).ok_or(EditError::RoomOutOfRange(room))?;
                let o = r.objects.get_mut(object).ok_or(EditError::ObjectOutOfRange(object))?;
                let prev = std::mem::replace(&mut o.params, params.clone());
                Ok(EditorCommand::SetObjectParams { room, object, params: prev })
            }
        }
    }
}
