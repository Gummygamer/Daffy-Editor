//! Editing commands. Every command application returns its inverse so the
//! history can undo it without snapshots.

use serde::{Deserialize, Serialize};

use crate::error::EditError;
use crate::model::level::{EnemySpawn, Level, Object};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorCommand {
    SetTile {
        room: usize,
        x: u32,
        y: u32,
        metatile: u16,
    },
    MoveObject {
        room: usize,
        object: usize,
        x: u32,
        y: u32,
    },
    MoveEnemySpawn {
        room: usize,
        spawn: usize,
        x: u32,
        y: u32,
    },
    SetObjectParams {
        room: usize,
        object: usize,
        params: Vec<u8>,
    },
    InsertObject {
        room: usize,
        index: usize,
        object: Object,
    },
    DeleteObject {
        room: usize,
        object: usize,
    },
    InsertEnemySpawn {
        room: usize,
        index: usize,
        spawn: EnemySpawn,
    },
    DeleteEnemySpawn {
        room: usize,
        spawn: usize,
    },
}

impl EditorCommand {
    /// Apply to `level`, returning the inverse command.
    /// On error the level is left unchanged.
    pub fn apply(&self, level: &mut Level) -> Result<EditorCommand, EditError> {
        match *self {
            EditorCommand::SetTile {
                room,
                x,
                y,
                metatile,
            } => {
                let r = level
                    .rooms
                    .get_mut(room)
                    .ok_or(EditError::RoomOutOfRange(room))?;
                let prev = r.set_tile(x, y, metatile)?;
                Ok(EditorCommand::SetTile {
                    room,
                    x,
                    y,
                    metatile: prev,
                })
            }
            EditorCommand::MoveObject { room, object, x, y } => {
                let r = level
                    .rooms
                    .get_mut(room)
                    .ok_or(EditError::RoomOutOfRange(room))?;
                let o = r
                    .objects
                    .get_mut(object)
                    .ok_or(EditError::ObjectOutOfRange(object))?;
                let (px, py) = (o.x, o.y);
                o.x = x;
                o.y = y;
                Ok(EditorCommand::MoveObject {
                    room,
                    object,
                    x: px,
                    y: py,
                })
            }
            EditorCommand::MoveEnemySpawn { room, spawn, x, y } => {
                let r = level
                    .rooms
                    .get_mut(room)
                    .ok_or(EditError::RoomOutOfRange(room))?;
                let s = r
                    .enemy_spawns
                    .get_mut(spawn)
                    .ok_or(EditError::EnemySpawnOutOfRange(spawn))?;
                let (px, py) = (s.x, s.y);
                s.x = x;
                s.y = y;
                Ok(EditorCommand::MoveEnemySpawn {
                    room,
                    spawn,
                    x: px,
                    y: py,
                })
            }
            EditorCommand::SetObjectParams {
                room,
                object,
                ref params,
            } => {
                let r = level
                    .rooms
                    .get_mut(room)
                    .ok_or(EditError::RoomOutOfRange(room))?;
                let o = r
                    .objects
                    .get_mut(object)
                    .ok_or(EditError::ObjectOutOfRange(object))?;
                let prev = std::mem::replace(&mut o.params, params.clone());
                Ok(EditorCommand::SetObjectParams {
                    room,
                    object,
                    params: prev,
                })
            }
            EditorCommand::InsertObject {
                room,
                index,
                ref object,
            } => {
                let r = level
                    .rooms
                    .get_mut(room)
                    .ok_or(EditError::RoomOutOfRange(room))?;
                if index > r.objects.len() {
                    return Err(EditError::ObjectOutOfRange(index));
                }
                r.objects.insert(index, object.clone());
                Ok(EditorCommand::DeleteObject {
                    room,
                    object: index,
                })
            }
            EditorCommand::DeleteObject { room, object } => {
                let r = level
                    .rooms
                    .get_mut(room)
                    .ok_or(EditError::RoomOutOfRange(room))?;
                if object >= r.objects.len() {
                    return Err(EditError::ObjectOutOfRange(object));
                }
                let removed = r.objects.remove(object);
                Ok(EditorCommand::InsertObject {
                    room,
                    index: object,
                    object: removed,
                })
            }
            EditorCommand::InsertEnemySpawn {
                room,
                index,
                ref spawn,
            } => {
                let r = level
                    .rooms
                    .get_mut(room)
                    .ok_or(EditError::RoomOutOfRange(room))?;
                if index > r.enemy_spawns.len() {
                    return Err(EditError::EnemySpawnOutOfRange(index));
                }
                r.enemy_spawns.insert(index, spawn.clone());
                Ok(EditorCommand::DeleteEnemySpawn { room, spawn: index })
            }
            EditorCommand::DeleteEnemySpawn { room, spawn } => {
                let r = level
                    .rooms
                    .get_mut(room)
                    .ok_or(EditError::RoomOutOfRange(room))?;
                if spawn >= r.enemy_spawns.len() {
                    return Err(EditError::EnemySpawnOutOfRange(spawn));
                }
                let removed = r.enemy_spawns.remove(spawn);
                Ok(EditorCommand::InsertEnemySpawn {
                    room,
                    index: spawn,
                    spawn: removed,
                })
            }
        }
    }
}
